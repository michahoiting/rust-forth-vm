//! The dictionary: its storage, the word header layout, and the functions that
//! write to and search the linked list of words. Nothing outside this module
//! knows how a header is laid out, except that a word's body follows its CFA.

use std::mem;
use std::ptr;

use crate::cell::Cell;
use crate::error::ForthError;

// Layout of a dictionary header, relative to its LFA. Every word uses this
// layout, whether it is a primitive or was created at runtime. The name bytes
// are stored in the dictionary directly in front of the header.
const LFA: usize = 0; // Link Field: points to the LFA of the previous word
const NFA: usize = 1; // Name Field: points to the name bytes
const LEN: usize = 2; // Length of the name in bytes
const FLAGS: usize = 3; // Combination of the F_* flags below
const CFA: usize = 4; // Code Field: the machine-code pointer
const HEADER_CELLS: usize = 5;

pub(crate) const F_IMMEDIATE: Cell = 1; // Executes even while compiling
pub(crate) const F_HIDDEN: Cell = 2; // Can never be found by name
pub(crate) const F_CREATED: Cell = 4; // Made by CREATE, so it owns an action cell for DOES>
pub(crate) const F_PENDING: Cell = 8; // Still being defined, so it can't be found or executed

// Returns the CFA of the word with this header.
pub(crate) unsafe fn cfa_of(header: *mut Cell) -> *mut Cell {
    header.add(CFA)
}

pub(crate) unsafe fn is_immediate(header: *mut Cell) -> bool {
    *header.add(FLAGS) & F_IMMEDIATE != 0
}

pub(crate) unsafe fn is_created(header: *mut Cell) -> bool {
    *header.add(FLAGS) & F_CREATED != 0
}

pub(crate) unsafe fn is_hidden(header: *mut Cell) -> bool {
    *header.add(FLAGS) & F_HIDDEN != 0
}

pub(crate) unsafe fn is_pending(header: *mut Cell) -> bool {
    *header.add(FLAGS) & F_PENDING != 0
}

// A saved dictionary state, used to discard everything written after it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Mark {
    here: *mut Cell,
    last_word: *mut Cell,
}

pub(crate) struct Dictionary {
    start: *mut Cell,     // The first Cell of the arena
    here: *mut Cell,      // Dictionary pointer to the next free Cell
    last_word: *mut Cell, // Pointer to the LFA of the most recently created word
    end: *mut Cell,       // One past the last usable Cell
    _storage: Vec<Cell>,  // Owns the arena that the pointers above refer to
}

impl Dictionary {
    // Allocates an untyped arena of `cells` Cells for our Forth dictionary space. The arena
    // can't grow later, because threads, links and action cells hold raw addresses into it.
    pub(crate) unsafe fn new(cells: usize) -> Result<Dictionary, ForthError> {
        let mut storage: Vec<Cell> = Vec::new();
        storage
            .try_reserve_exact(cells)
            .map_err(|_| ForthError::DictionaryTooLarge(cells))?;
        storage.resize(cells, 0);

        let here = storage.as_mut_ptr();
        Ok(Dictionary {
            start: here,
            here,
            last_word: ptr::null_mut(), // Dictionary starts out empty
            end: here.add(storage.len()),
            _storage: storage,
        })
    }

    // Presses a Cell into the dictionary at HERE.
    pub(crate) unsafe fn comma(&mut self, value: Cell) -> Result<(), ForthError> {
        if self.here >= self.end {
            return Err(ForthError::DictionaryOverflow);
        }
        *self.here = value;
        self.here = self.here.add(1);
        Ok(())
    }

    // Returns the index of the Cell at `addr`, which must be a cell-aligned address below
    // HERE. This keeps the Cell inside the part of the arena that has been written.
    fn cell_index(&self, addr: Cell) -> Result<usize, ForthError> {
        let start = self.start as Cell;
        let here = self.here as Cell;
        let cell_size = mem::size_of::<Cell>();
        if addr < start || addr >= here || !(addr - start).is_multiple_of(cell_size) {
            return Err(ForthError::InvalidAddress(addr));
        }
        Ok((addr - start) / cell_size)
    }

    // Reads the Cell at `addr`. The number from the stack is only used as an offset,
    // never as a pointer.
    pub(crate) fn fetch(&self, addr: Cell) -> Result<Cell, ForthError> {
        let index = self.cell_index(addr)?;
        Ok(unsafe { *self.start.add(index) })
    }

    // Writes the Cell at `addr`, checked in the same way as `fetch`. This can change word
    // headers and compiled code, which the VM trusts.
    pub(crate) fn store(&mut self, addr: Cell, value: Cell) -> Result<(), ForthError> {
        let index = self.cell_index(addr)?;
        unsafe { *self.start.add(index) = value };
        Ok(())
    }

    // Writes a complete word into the dictionary: the name bytes, then the header.
    // Returns the LFA of the new word.
    pub(crate) unsafe fn write_header(
        &mut self,
        name: &str,
        code: Cell,
        flags: Cell,
    ) -> Result<*mut Cell, ForthError> {
        let name_cells = name.len().div_ceil(mem::size_of::<Cell>());
        let free_cells = self.end.offset_from(self.here) as usize;
        if name_cells + HEADER_CELLS > free_cells {
            return Err(ForthError::DictionaryOverflow);
        }

        // The dictionary owns its own copy, because the input line does not outlive this call.
        let name_ptr = self.here as *mut u8;
        ptr::copy_nonoverlapping(name.as_ptr(), name_ptr, name.len());

        let header = self.here.add(name_cells);
        *header.add(LFA) = self.last_word as Cell;
        *header.add(NFA) = name_ptr as Cell;
        *header.add(LEN) = name.len();
        *header.add(FLAGS) = flags;
        *header.add(CFA) = code;

        self.here = header.add(HEADER_CELLS);
        self.last_word = header; // Update the head pointer of the dictionary
        Ok(header)
    }

    // Simulates the built-in bootstrap dictionary populated when a Forth system boots.
    // Returns the CFA of the new word.
    pub(crate) unsafe fn add_primitive_word(
        &mut self,
        name: &str,
        code: Cell,
        flags: Cell,
    ) -> Result<Cell, ForthError> {
        let header = self.write_header(name, code, flags)?;
        Ok(cfa_of(header) as Cell)
    }

    // Linearly traverses the linked list of the dictionary (from newest to oldest).
    // Returns the LFA of the matching word.
    pub(crate) unsafe fn find_word(&self, token: &str) -> Option<*mut Cell> {
        let mut current_lfa = self.last_word;

        while !current_lfa.is_null() {
            let nfa_ptr = *current_lfa.add(NFA) as *const u8;
            let len = *current_lfa.add(LEN);

            // Reconstruct the name from raw dictionary data pointers
            if !is_hidden(current_lfa)
                && !is_pending(current_lfa)
                && std::slice::from_raw_parts(nfa_ptr, len) == token.as_bytes()
            {
                return Some(current_lfa);
            }
            // Follow the link pointer to the previous word (LFA)
            current_lfa = *current_lfa.add(LFA) as *mut Cell;
        }
        None
    }

    // Returns the header of the most recently created word.
    pub(crate) fn last_word(&self) -> *mut Cell {
        self.last_word
    }

    // Marks the most recently created word as finished. Its other flags stay as they are.
    pub(crate) unsafe fn finish_last(&mut self) {
        *self.last_word.add(FLAGS) &= !F_PENDING;
    }

    // Marks the most recently created word as immediate. Its other flags stay as they are.
    pub(crate) unsafe fn mark_last_immediate(&mut self) {
        *self.last_word.add(FLAGS) |= F_IMMEDIATE;
    }

    // Returns whether `xt` is the code field of a finished word, hidden or not.
    pub(crate) unsafe fn is_execution_token(&self, xt: Cell) -> bool {
        let mut current_lfa = self.last_word;

        while !current_lfa.is_null() {
            if cfa_of(current_lfa) as Cell == xt {
                return !is_pending(current_lfa);
            }
            current_lfa = *current_lfa.add(LFA) as *mut Cell;
        }
        false
    }

    // Saves the current state, for a later `restore`.
    pub(crate) fn mark(&self) -> Mark {
        Mark {
            here: self.here,
            last_word: self.last_word,
        }
    }

    // Discards everything written since `mark` was saved.
    pub(crate) fn restore(&mut self, mark: Mark) {
        self.here = mark.here;
        self.last_word = mark.last_word;
    }
}

#[cfg(test)]
mod tests {
    use std::io;

    use super::*;
    use crate::{boot, outer_interpreter};

    const CELL_SIZE: usize = mem::size_of::<Cell>();
    const TEST_CELLS: usize = 1024;

    // Creates a dictionary holding these Cells, starting at its first Cell.
    unsafe fn dictionary_with(values: &[Cell]) -> Dictionary {
        let mut dictionary = Dictionary::new(TEST_CELLS).unwrap();
        for value in values {
            dictionary.comma(*value).unwrap();
        }
        dictionary
    }

    fn assert_invalid(dictionary: &Dictionary, addr: Cell) {
        let result = dictionary.fetch(addr);
        assert!(
            matches!(result, Err(ForthError::InvalidAddress(a)) if a == addr),
            "got: {:?}",
            result
        );
    }

    fn assert_store_invalid(dictionary: &mut Dictionary, addr: Cell) {
        let result = dictionary.store(addr, 9);
        assert!(
            matches!(result, Err(ForthError::InvalidAddress(a)) if a == addr),
            "got: {:?}",
            result
        );
    }

    #[test]
    fn fetch_reads_the_first_cell() {
        unsafe {
            let dictionary = dictionary_with(&[7]);
            assert_eq!(dictionary.fetch(dictionary.start as Cell).unwrap(), 7);
        }
    }

    #[test]
    fn fetch_rejects_addresses_just_below_the_start() {
        unsafe {
            let dictionary = dictionary_with(&[7]);
            let start = dictionary.start as Cell;
            assert_invalid(&dictionary, start - 1);
            assert_invalid(&dictionary, start - CELL_SIZE);
        }
    }

    #[test]
    fn fetch_rejects_misaligned_addresses_inside_the_written_cells() {
        unsafe {
            // Every address here is below HERE, so only the alignment check can reject it.
            let dictionary = dictionary_with(&[1, 2]);
            let start = dictionary.start as Cell;
            for offset in 1..CELL_SIZE {
                assert_invalid(&dictionary, start + offset);
                assert_invalid(&dictionary, start + CELL_SIZE + offset);
            }
        }
    }

    #[test]
    fn fetch_rejects_addresses_after_a_restore() {
        unsafe {
            let mut dictionary = dictionary_with(&[1]);
            let mark = dictionary.mark();
            let addr = dictionary.here as Cell;
            dictionary.comma(2).unwrap();
            assert_eq!(dictionary.fetch(addr).unwrap(), 2);

            dictionary.restore(mark);
            assert_invalid(&dictionary, addr);
        }
    }

    #[test]
    fn store_writes_the_first_cell() {
        unsafe {
            let mut dictionary = dictionary_with(&[1, 2]);
            let start = dictionary.start as Cell;
            dictionary.store(start, 9).unwrap();
            assert_eq!(dictionary.fetch(start).unwrap(), 9);
        }
    }

    #[test]
    fn store_writes_the_last_written_cell() {
        unsafe {
            let mut dictionary = dictionary_with(&[1, 2]);
            let last = dictionary.start as Cell + CELL_SIZE;
            dictionary.store(last, 9).unwrap();
            assert_eq!(dictionary.fetch(last).unwrap(), 9);
        }
    }

    #[test]
    fn store_changes_only_the_target_cell() {
        unsafe {
            let mut dictionary = dictionary_with(&[1, 2, 3]);
            let start = dictionary.start as Cell;
            dictionary.store(start + CELL_SIZE, 9).unwrap();
            assert_eq!(dictionary.fetch(start).unwrap(), 1);
            assert_eq!(dictionary.fetch(start + CELL_SIZE).unwrap(), 9);
            assert_eq!(dictionary.fetch(start + 2 * CELL_SIZE).unwrap(), 3);
        }
    }

    #[test]
    fn store_rejects_addresses_just_below_the_start() {
        unsafe {
            let mut dictionary = dictionary_with(&[7]);
            let start = dictionary.start as Cell;
            assert_store_invalid(&mut dictionary, start - 1);
            assert_store_invalid(&mut dictionary, start - CELL_SIZE);
        }
    }

    #[test]
    fn store_rejects_here() {
        unsafe {
            let mut dictionary = dictionary_with(&[7]);
            let here = dictionary.here as Cell;
            assert_store_invalid(&mut dictionary, here);
        }
    }

    #[test]
    fn store_rejects_misaligned_addresses_inside_the_written_cells() {
        unsafe {
            // Every address here is below HERE, so only the alignment check can reject it.
            let mut dictionary = dictionary_with(&[1, 2]);
            let start = dictionary.start as Cell;
            for offset in 1..CELL_SIZE {
                assert_store_invalid(&mut dictionary, start + offset);
                assert_store_invalid(&mut dictionary, start + CELL_SIZE + offset);
            }
            assert_eq!(dictionary.fetch(start).unwrap(), 1);
            assert_eq!(dictionary.fetch(start + CELL_SIZE).unwrap(), 2);
        }
    }

    #[test]
    fn store_rejects_addresses_after_a_restore() {
        unsafe {
            let mut dictionary = dictionary_with(&[1]);
            let mark = dictionary.mark();
            let addr = dictionary.here as Cell;
            dictionary.comma(2).unwrap();
            dictionary.store(addr, 9).unwrap();

            dictionary.restore(mark);
            assert_store_invalid(&mut dictionary, addr);
        }
    }

    // `!` is deliberately not limited to data fields for now. This pins that down, using
    // the hidden flag because it has a visible effect without corrupting the VM. If `!`
    // gets restricted later, this test should change with it.
    #[test]
    fn store_can_change_a_word_header() {
        unsafe {
            let mut regs = boot(io::sink(), TEST_CELLS).unwrap();
            outer_interpreter(&mut regs, "CREATE X 7 ,").unwrap();
            let header = regs.dictionary.find_word("X").unwrap();

            let addr = header.add(FLAGS) as Cell;
            let flags = F_CREATED | F_HIDDEN;
            outer_interpreter(&mut regs, &format!("{} {} !", flags, addr)).unwrap();

            outer_interpreter(&mut regs, &format!("{} @", addr)).unwrap();
            assert_eq!(regs.data_stack, vec![flags]);

            let error = outer_interpreter(&mut regs, "X").unwrap_err();
            assert!(matches!(error, ForthError::UnknownWord(name) if name == "X"));
        }
    }

    #[test]
    fn dictionary_holds_exactly_the_requested_cells() {
        unsafe {
            let mut dictionary = Dictionary::new(2).unwrap();
            dictionary.comma(1).unwrap();
            dictionary.comma(2).unwrap();
            assert!(matches!(
                dictionary.comma(3),
                Err(ForthError::DictionaryOverflow)
            ));
        }
    }

    #[test]
    fn empty_dictionary_is_valid() {
        unsafe {
            let mut dictionary = Dictionary::new(0).unwrap();
            assert!(matches!(
                dictionary.comma(1),
                Err(ForthError::DictionaryOverflow)
            ));
        }
    }

    #[test]
    fn write_header_must_fit() {
        unsafe {
            // This name takes exactly one Cell.
            let name = "X".repeat(CELL_SIZE);

            let mut dictionary = Dictionary::new(HEADER_CELLS + 1).unwrap();
            assert!(dictionary.write_header(&name, 0, 0).is_ok());

            let mut dictionary = Dictionary::new(HEADER_CELLS).unwrap();
            assert!(matches!(
                dictionary.write_header(&name, 0, 0),
                Err(ForthError::DictionaryOverflow)
            ));
        }
    }

    #[test]
    fn find_word_skips_a_pending_word_until_it_is_finished() {
        unsafe {
            let mut dictionary = Dictionary::new(TEST_CELLS).unwrap();
            dictionary.write_header("X", 0, F_PENDING).unwrap();
            assert!(dictionary.find_word("X").is_none());

            dictionary.finish_last();
            assert!(dictionary.find_word("X").is_some());
        }
    }

    #[test]
    fn find_word_skips_a_hidden_word() {
        unsafe {
            let mut dictionary = Dictionary::new(TEST_CELLS).unwrap();
            dictionary.write_header("X", 0, F_HIDDEN).unwrap();
            assert!(dictionary.find_word("X").is_none());
        }
    }

    #[test]
    fn finish_last_keeps_the_hidden_flag() {
        unsafe {
            let mut dictionary = Dictionary::new(TEST_CELLS).unwrap();
            let header = dictionary
                .write_header("X", 0, F_HIDDEN | F_PENDING)
                .unwrap();

            dictionary.finish_last();
            assert!(!is_pending(header));
            assert!(is_hidden(header));
        }
    }

    #[test]
    fn execution_tokens_include_finished_and_hidden_words() {
        unsafe {
            let mut dictionary = Dictionary::new(TEST_CELLS).unwrap();
            let named = dictionary.write_header("X", 0, 0).unwrap();
            let hidden = dictionary.write_header("", 0, F_HIDDEN).unwrap();
            assert!(dictionary.is_execution_token(cfa_of(named) as Cell));
            assert!(dictionary.is_execution_token(cfa_of(hidden) as Cell));
        }
    }

    #[test]
    fn execution_tokens_exclude_pending_words() {
        unsafe {
            let mut dictionary = Dictionary::new(TEST_CELLS).unwrap();
            let header = dictionary.write_header("X", 0, F_PENDING).unwrap();
            assert!(!dictionary.is_execution_token(cfa_of(header) as Cell));

            dictionary.finish_last();
            assert!(dictionary.is_execution_token(cfa_of(header) as Cell));
        }
    }

    #[test]
    fn execution_tokens_exclude_other_addresses_and_numbers() {
        unsafe {
            let mut dictionary = Dictionary::new(TEST_CELLS).unwrap();
            let header = dictionary.write_header("X", 0, 0).unwrap();
            assert!(!dictionary.is_execution_token(header as Cell));
            assert!(!dictionary.is_execution_token(5));
            assert!(!dictionary.is_execution_token(0));
        }
    }

    #[test]
    fn execution_tokens_exclude_words_after_a_restore() {
        unsafe {
            let mut dictionary = Dictionary::new(TEST_CELLS).unwrap();
            let mark = dictionary.mark();
            let header = dictionary.write_header("X", 0, 0).unwrap();
            let xt = cfa_of(header) as Cell;

            dictionary.restore(mark);
            assert!(!dictionary.is_execution_token(xt));
        }
    }

    #[test]
    fn mark_last_immediate_only_marks_the_newest_word() {
        unsafe {
            let mut dictionary = Dictionary::new(TEST_CELLS).unwrap();
            let older = dictionary.write_header("A", 0, 0).unwrap();
            let newest = dictionary.write_header("B", 0, 0).unwrap();

            dictionary.mark_last_immediate();
            assert!(is_immediate(newest));
            assert!(!is_immediate(older));
        }
    }

    #[test]
    fn mark_last_immediate_keeps_other_flags() {
        unsafe {
            let mut dictionary = Dictionary::new(TEST_CELLS).unwrap();
            let flags = F_HIDDEN | F_PENDING | F_CREATED;
            let header = dictionary.write_header("X", 0, flags).unwrap();

            dictionary.mark_last_immediate();
            assert!(is_immediate(header));
            assert!(is_hidden(header));
            assert!(is_pending(header));
            assert!(is_created(header));
        }
    }

    #[test]
    fn mark_last_immediate_can_be_repeated() {
        unsafe {
            let mut dictionary = Dictionary::new(TEST_CELLS).unwrap();
            let header = dictionary.write_header("X", 0, 0).unwrap();

            dictionary.mark_last_immediate();
            dictionary.mark_last_immediate();
            assert_eq!(*header.add(FLAGS), F_IMMEDIATE);
        }
    }
}
