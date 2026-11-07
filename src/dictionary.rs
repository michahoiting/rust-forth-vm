// Copyright (c) 2026 M.W.R. Hoiting

//! The dictionary: its storage, the word header layout, and the functions that
//! write to and search the linked list of words. Nothing outside this module
//! knows how a header is laid out: other modules use its accessors, such as [`pfa_of`].
//!
//! Every word uses the same header layout, whether it is a primitive or was created at
//! runtime. The name bytes are stored in the dictionary directly in front of the header. A
//! word's `header` is a pointer to its first Cell, the LFA, and the constants below are
//! offsets from there.

use std::mem;
use std::ptr;

use crate::cell::Cell;
use crate::error::ForthError;

/// Link Field: points to the LFA of the previous word.
const LFA: usize = 0;
/// Name Field: points to the name bytes.
const NFA: usize = 1;
/// The length of the name in bytes, in the bits of [`LEN_MASK`], combined with the `F_*` flags
/// below.
const LEN_FLAGS: usize = 2;
/// Code Field: the machine-code pointer. For a word changed by DOES>, this is the address of
/// its action thread instead, which [`enter_word`](crate::vm::enter_word) tells apart.
const CFA: usize = 3;
/// The size of a header in Cells.
const HEADER_CELLS: usize = 4;

/// The bits of [`LEN_FLAGS`] that hold the name length, which is also the longest name.
const LEN_MASK: Cell = 0xFF;

/// Executes even while compiling.
pub(crate) const F_IMMEDIATE: Cell = 1 << 8;
/// Can never be found by name. Unlike [`F_PENDING`], this stays set, and the word can still
/// be run through its execution token.
pub(crate) const F_HIDDEN: Cell = 1 << 9;
/// Made by CREATE, so DOES> may change its action.
pub(crate) const F_CREATED: Cell = 1 << 10;
/// Still being defined, so it can't be found or executed. Unlike [`F_HIDDEN`], this is
/// cleared when the definition ends.
pub(crate) const F_PENDING: Cell = 1 << 11;

/// Returns the CFA of the word with this header.
///
/// # Safety
///
/// `header` must point to a word in the dictionary.
pub(crate) unsafe fn cfa_of(header: *mut Cell) -> *mut Cell {
    header.add(CFA)
}

/// Returns the PFA of the word with this CFA. Every word's Parameter Field follows its Code
/// Field, but a primitive has nothing there.
///
/// # Safety
///
/// `cfa` must point to the Code Field of a word in the dictionary.
pub(crate) unsafe fn pfa_of(cfa: *const Cell) -> *const Cell {
    cfa.add(1)
}

/// Returns `addr` rounded up to the next cell-aligned address. Like other Cell arithmetic, this
/// wraps at the top of the address range.
pub(crate) fn aligned(addr: Cell) -> Cell {
    let mask = mem::size_of::<Cell>() - 1;
    addr.wrapping_add(mask) & !mask
}

/// Returns whether the word has [`F_IMMEDIATE`] set.
///
/// # Safety
///
/// See [`cfa_of`].
pub(crate) unsafe fn is_immediate(header: *mut Cell) -> bool {
    *header.add(LEN_FLAGS) & F_IMMEDIATE != 0
}

/// Returns whether the word has [`F_CREATED`] set.
///
/// # Safety
///
/// See [`cfa_of`].
pub(crate) unsafe fn is_created(header: *mut Cell) -> bool {
    *header.add(LEN_FLAGS) & F_CREATED != 0
}

/// Returns whether a name can find the word, so it's neither hidden nor still being defined.
///
/// # Safety
///
/// `header` must point to a word header.
unsafe fn is_findable(header: *mut Cell) -> bool {
    !is_hidden(header) && !is_pending(header)
}

/// Returns whether the word has [`F_HIDDEN`] set.
///
/// # Safety
///
/// See [`cfa_of`].
pub(crate) unsafe fn is_hidden(header: *mut Cell) -> bool {
    *header.add(LEN_FLAGS) & F_HIDDEN != 0
}

/// Returns whether the word has [`F_PENDING`] set.
///
/// # Safety
///
/// See [`cfa_of`].
pub(crate) unsafe fn is_pending(header: *mut Cell) -> bool {
    *header.add(LEN_FLAGS) & F_PENDING != 0
}

/// A saved dictionary state, used to discard everything written after it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Mark {
    /// The saved [`Dictionary::here`].
    here: *mut u8,
    /// The saved [`Dictionary::last_word`].
    last_word: *mut Cell,
}

/// The arena that holds every word, and the linked list running through it.
pub(crate) struct Dictionary {
    /// The first Cell of the arena.
    start: *mut Cell,
    /// Dictionary pointer to the next free byte. It is cell-aligned unless `C,` or `ALLOT` left
    /// it unaligned.
    here: *mut u8,
    /// Pointer to the LFA of the most recently created word.
    last_word: *mut Cell,
    /// One past the last usable Cell.
    end: *mut Cell,
    /// Owns the arena that the pointers above refer to.
    _storage: Vec<Cell>,
}

impl Dictionary {
    /// Allocates an untyped arena of `cells` Cells for our Forth dictionary space. The arena
    /// can't grow later, because threads, links and Code Fields hold raw addresses into it.
    ///
    /// # Errors
    ///
    /// Returns [`ForthError::DictionaryTooLarge`] if the arena can't be allocated.
    pub(crate) unsafe fn new(cells: usize) -> Result<Dictionary, ForthError> {
        let mut storage: Vec<Cell> = Vec::new();
        storage
            .try_reserve_exact(cells)
            .map_err(|_| ForthError::DictionaryTooLarge(cells))?;
        storage.resize(cells, 0);

        let start = storage.as_mut_ptr();
        Ok(Dictionary {
            start,
            here: start as *mut u8,
            last_word: ptr::null_mut(), // Dictionary starts out empty
            end: start.add(storage.len()),
            _storage: storage,
        })
    }

    /// Returns HERE, which is the address of the next free byte.
    pub(crate) fn here(&self) -> Cell {
        self.here as Cell
    }

    /// Returns the first cell-aligned address at or after HERE. The arena ends on a Cell
    /// boundary, so this is never past its end.
    fn next_cell(&self) -> *mut u8 {
        let padding = aligned(self.here()) - self.here();
        self.here.wrapping_add(padding)
    }

    /// Returns how many bytes are free from `from` to the end of the arena.
    fn free_bytes(&self, from: *mut u8) -> usize {
        self.end as Cell - from as Cell
    }

    /// Moves HERE to the next cell-aligned address, and clears the bytes it passes.
    pub(crate) fn align(&mut self) {
        let cell = self.next_cell();
        // SAFETY: HERE and `cell` are both in the arena, and `cell` isn't before HERE.
        unsafe { ptr::write_bytes(self.here, 0, cell as Cell - self.here as Cell) };
        self.here = cell;
    }

    /// Presses a Cell into the dictionary at HERE, aligning HERE first.
    ///
    /// # Errors
    ///
    /// Returns [`ForthError::DictionaryOverflow`] if the Cell doesn't fit. HERE doesn't move.
    pub(crate) unsafe fn comma(&mut self, value: Cell) -> Result<(), ForthError> {
        if self.free_bytes(self.next_cell()) < mem::size_of::<Cell>() {
            return Err(ForthError::DictionaryOverflow);
        }
        self.align();
        *(self.here as *mut Cell) = value;
        self.here = self.here.add(mem::size_of::<Cell>());
        Ok(())
    }

    /// Presses one byte into the dictionary at HERE, which can leave HERE unaligned.
    ///
    /// # Errors
    ///
    /// Returns [`ForthError::DictionaryOverflow`] if the dictionary is full.
    pub(crate) unsafe fn char_comma(&mut self, value: u8) -> Result<(), ForthError> {
        if self.free_bytes(self.here) == 0 {
            return Err(ForthError::DictionaryOverflow);
        }
        *self.here = value;
        self.here = self.here.add(1);
        Ok(())
    }

    /// Moves HERE by `bytes`, which can leave it unaligned. Reserved bytes are cleared. Only
    /// the data of the newest word can be given back, and only if `CREATE` made it. Any other
    /// word ends in code or a value that would be overwritten.
    ///
    /// # Errors
    ///
    /// Returns [`ForthError::DictionaryOverflow`] if `bytes` doesn't fit, and
    /// [`ForthError::AllotBelowData`] if it gives back more than that data.
    ///
    /// # Safety
    ///
    /// The newest word's header must be intact, as for [`find_word`](Self::find_word).
    pub(crate) unsafe fn allot(&mut self, bytes: isize) -> Result<(), ForthError> {
        let size = bytes.unsigned_abs();
        if bytes >= 0 {
            if size > self.free_bytes(self.here) {
                return Err(ForthError::DictionaryOverflow);
            }
            ptr::write_bytes(self.here, 0, size);
            self.here = self.here.add(size);
        } else {
            if self.last_word.is_null() || !is_created(self.last_word) {
                return Err(ForthError::AllotBelowData(bytes));
            }
            let data = pfa_of(cfa_of(self.last_word)) as Cell;
            if size > self.here() - data {
                return Err(ForthError::AllotBelowData(bytes));
            }
            self.here = self.here.sub(size);
        }
        Ok(())
    }

    /// Returns whether `addr` points into the arena, to a written Cell or not.
    #[inline(always)]
    pub(crate) fn is_in_arena(&self, addr: Cell) -> bool {
        addr >= self.start as Cell && addr < self.end as Cell
    }

    /// Checks that `len` bytes at `addr` are in the written part of the arena, and returns
    /// the offset of `addr` from the start of the arena. The checks run in a fixed order, so
    /// the error names the first fault.
    ///
    /// # Errors
    ///
    /// Returns [`ForthError::AddressOutOfBounds`] if `addr` isn't in the arena, and
    /// [`ForthError::UnwrittenAddress`] if the bytes reach HERE or past it.
    fn written_offset(&self, addr: Cell, len: usize) -> Result<usize, ForthError> {
        if !self.is_in_arena(addr) {
            return Err(ForthError::AddressOutOfBounds(addr));
        }
        // HERE never passes the end of the arena, so this can't overflow.
        if addr + len > self.here() {
            return Err(ForthError::UnwrittenAddress(addr));
        }
        Ok(addr - self.start as Cell)
    }

    /// Returns the index of the Cell at `addr`, which must be cell-aligned and end at or below
    /// HERE. This keeps the Cell inside the part of the arena that has been written.
    ///
    /// # Errors
    ///
    /// Returns [`ForthError::AddressOutOfBounds`] or [`ForthError::UnwrittenAddress`] from
    /// [`written_offset`](Self::written_offset), and [`ForthError::MisalignedAddress`] if
    /// `addr` isn't cell-aligned.
    fn cell_index(&self, addr: Cell) -> Result<usize, ForthError> {
        let cell_size = mem::size_of::<Cell>();
        let offset = self.written_offset(addr, cell_size)?;
        if !offset.is_multiple_of(cell_size) {
            return Err(ForthError::MisalignedAddress(addr));
        }
        Ok(offset / cell_size)
    }

    /// Returns the offset of the byte at `addr`, which must be below HERE.
    ///
    /// # Errors
    ///
    /// Returns [`ForthError::AddressOutOfBounds`] or [`ForthError::UnwrittenAddress`] from
    /// [`written_offset`](Self::written_offset).
    fn byte_offset(&self, addr: Cell) -> Result<usize, ForthError> {
        self.written_offset(addr, 1)
    }

    /// Reads the Cell at `addr`. The number from the stack is only used as an offset,
    /// never as a pointer.
    ///
    /// # Errors
    ///
    /// Returns the error of [`cell_index`](Self::cell_index) if it rejects `addr`.
    pub(crate) fn fetch(&self, addr: Cell) -> Result<Cell, ForthError> {
        let index = self.cell_index(addr)?;
        Ok(unsafe { *self.start.add(index) })
    }

    /// Writes the Cell at `addr`, checked in the same way as `fetch`. This can change word
    /// headers and compiled code, which the VM trusts.
    ///
    /// # Errors
    ///
    /// Returns the error of [`cell_index`](Self::cell_index) if it rejects `addr`.
    pub(crate) fn store(&mut self, addr: Cell, value: Cell) -> Result<(), ForthError> {
        let index = self.cell_index(addr)?;
        unsafe { *self.start.add(index) = value };
        Ok(())
    }

    /// Reads the byte at `addr`, which needn't be aligned.
    ///
    /// # Errors
    ///
    /// Returns the error of [`byte_offset`](Self::byte_offset) if it rejects `addr`.
    pub(crate) fn fetch_byte(&self, addr: Cell) -> Result<u8, ForthError> {
        let offset = self.byte_offset(addr)?;
        // SAFETY: `byte_offset` keeps the byte in the arena, and every byte there is initialized.
        Ok(unsafe { *(self.start as *const u8).add(offset) })
    }

    /// Writes the byte at `addr`, checked in the same way as `fetch_byte`. Like `store`, this
    /// can change word headers and compiled code.
    ///
    /// # Errors
    ///
    /// Returns the error of [`byte_offset`](Self::byte_offset) if it rejects `addr`.
    pub(crate) fn store_byte(&mut self, addr: Cell, value: u8) -> Result<(), ForthError> {
        let offset = self.byte_offset(addr)?;
        // SAFETY: `byte_offset` keeps the byte in the arena.
        unsafe { *(self.start as *mut u8).add(offset) = value };
        Ok(())
    }

    /// Writes a complete word into the dictionary: the name bytes, then the header, both at
    /// the next cell-aligned address. Returns the LFA of the new word.
    ///
    /// # Errors
    ///
    /// Returns [`ForthError::NameTooLong`] if the length doesn't fit in [`LEN_MASK`], and
    /// [`ForthError::DictionaryOverflow`] if the word doesn't fit. Nothing is written.
    ///
    /// # Safety
    ///
    /// `code` must come from [`code_cell`](crate::vm::code_cell), because the VM calls it.
    pub(crate) unsafe fn write_header(
        &mut self,
        name: &str,
        code: Cell,
        flags: Cell,
    ) -> Result<*mut Cell, ForthError> {
        if name.len() > LEN_MASK {
            return Err(ForthError::NameTooLong(name.len()));
        }

        let cell_size = mem::size_of::<Cell>();
        let name_cells = name.len().div_ceil(cell_size);
        if (name_cells + HEADER_CELLS) * cell_size > self.free_bytes(self.next_cell()) {
            return Err(ForthError::DictionaryOverflow);
        }
        self.align();

        // The dictionary owns its own copy, because the input does not outlive this call.
        let name_ptr = self.here;
        ptr::copy_nonoverlapping(name.as_ptr(), name_ptr, name.len());

        let header = (name_ptr as *mut Cell).add(name_cells);
        *header.add(LFA) = self.last_word as Cell;
        *header.add(NFA) = name_ptr as Cell;
        *header.add(LEN_FLAGS) = name.len() | flags;
        *header.add(CFA) = code;

        self.here = header.add(HEADER_CELLS) as *mut u8;
        self.last_word = header; // Update the head pointer of the dictionary
        Ok(header)
    }

    /// Simulates the built-in bootstrap dictionary populated when a Forth system boots.
    /// Returns the CFA of the new word.
    ///
    /// # Errors
    ///
    /// See [`write_header`](Self::write_header).
    ///
    /// # Safety
    ///
    /// `code` must come from [`code_cell`](crate::vm::code_cell), because the VM calls it.
    pub(crate) unsafe fn add_primitive_word(
        &mut self,
        name: &str,
        code: Cell,
        flags: Cell,
    ) -> Result<Cell, ForthError> {
        let header = self.write_header(name, code, flags)?;
        Ok(cfa_of(header) as Cell)
    }

    /// Linearly traverses the linked list of the dictionary (from newest to oldest).
    /// Returns the LFA of the matching word.
    ///
    /// # Safety
    ///
    /// Every word header must be intact, because links and names are followed as raw
    /// pointers. `!` can overwrite them.
    pub(crate) unsafe fn find_word(&self, token: &str) -> Option<*mut Cell> {
        let mut current_lfa = self.last_word;

        while !current_lfa.is_null() {
            let nfa_ptr = *current_lfa.add(NFA) as *const u8;
            let len = *current_lfa.add(LEN_FLAGS) & LEN_MASK;

            // Reconstruct the name from raw dictionary data pointers
            if is_findable(current_lfa)
                && std::slice::from_raw_parts(nfa_ptr, len) == token.as_bytes()
            {
                return Some(current_lfa);
            }
            // Follow the link pointer to the previous word (LFA)
            current_lfa = *current_lfa.add(LFA) as *mut Cell;
        }
        None
    }

    /// Returns the sorted names of the words that [`find_word`](Self::find_word) can find, each
    /// one once. A name that isn't valid UTF-8, because `C!` overwrote it, comes back with
    /// replacement characters.
    ///
    /// # Safety
    ///
    /// Every word header must be intact, as for [`find_word`](Self::find_word).
    pub(crate) unsafe fn word_names(&self) -> Vec<String> {
        let mut names = Vec::new();
        let mut current_lfa = self.last_word;

        while !current_lfa.is_null() {
            let nfa_ptr = *current_lfa.add(NFA) as *const u8;
            let len = *current_lfa.add(LEN_FLAGS) & LEN_MASK;

            if is_findable(current_lfa) && len > 0 {
                let name = std::slice::from_raw_parts(nfa_ptr, len);
                names.push(String::from_utf8_lossy(name).into_owned());
            }
            current_lfa = *current_lfa.add(LFA) as *mut Cell;
        }
        names.sort_unstable();
        names.dedup();
        names
    }

    /// Returns the header of the most recently created word, or null if there are none.
    pub(crate) fn last_word(&self) -> *mut Cell {
        self.last_word
    }

    /// Marks the most recently created word as finished. Its other flags stay as they are.
    ///
    /// # Safety
    ///
    /// The dictionary must hold at least one word.
    pub(crate) unsafe fn finish_last(&mut self) {
        *self.last_word.add(LEN_FLAGS) &= !F_PENDING;
    }

    /// Marks the most recently created word as immediate. Its other flags stay as they are.
    ///
    /// # Safety
    ///
    /// The dictionary must hold at least one word.
    pub(crate) unsafe fn mark_last_immediate(&mut self) {
        *self.last_word.add(LEN_FLAGS) |= F_IMMEDIATE;
    }

    /// Returns whether `xt` is the code field of a finished word, hidden or not.
    ///
    /// # Safety
    ///
    /// Every link in the word list must be intact, as for [`find_word`](Self::find_word).
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

    /// Saves the current state, for a later [`restore`](Self::restore).
    pub(crate) fn mark(&self) -> Mark {
        Mark {
            here: self.here,
            last_word: self.last_word,
        }
    }

    /// Discards everything written since `mark` was saved.
    pub(crate) fn restore(&mut self, mark: Mark) {
        self.here = mark.here;
        self.last_word = mark.last_word;
    }
}

#[cfg(test)]
mod tests {
    use std::io;

    use super::*;
    use crate::{boot, outer_interpreter, VmConfig};

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

    // The error variants carry no payload besides the address, so Debug output pins the exact
    // variant and address.
    fn assert_fetch_rejected(dictionary: &Dictionary, addr: Cell, make: fn(Cell) -> ForthError) {
        let result = dictionary.fetch(addr);
        assert_eq!(format!("{:?}", result), format!("Err({:?})", make(addr)));
    }

    fn assert_store_rejected(
        dictionary: &mut Dictionary,
        addr: Cell,
        make: fn(Cell) -> ForthError,
    ) {
        let result = dictionary.store(addr, 9);
        assert_eq!(format!("{:?}", result), format!("Err({:?})", make(addr)));
    }

    // Checks that `fetch` and `store` both reject `addr` with the error that `make` builds.
    fn assert_rejected(dictionary: &mut Dictionary, addr: Cell, make: fn(Cell) -> ForthError) {
        assert_fetch_rejected(dictionary, addr, make);
        assert_store_rejected(dictionary, addr, make);
    }

    #[test]
    fn aligned_rounds_up_to_a_cell_boundary() {
        assert_eq!(aligned(0), 0);
        assert_eq!(aligned(1), CELL_SIZE);
        assert_eq!(aligned(CELL_SIZE - 1), CELL_SIZE);
        assert_eq!(aligned(CELL_SIZE), CELL_SIZE);
        assert_eq!(aligned(CELL_SIZE + 1), 2 * CELL_SIZE);
    }

    #[test]
    fn aligned_wraps_at_the_top_of_the_address_range() {
        assert_eq!(aligned(Cell::MAX), 0);
    }

    #[test]
    fn store_byte_changes_only_the_target_byte() {
        unsafe {
            let mut dictionary = dictionary_with(&[0, 0]);
            let start = dictionary.start as Cell;
            dictionary.store_byte(start + 1, 0xAB).unwrap();

            for offset in 0..2 * CELL_SIZE {
                let expected = if offset == 1 { 0xAB } else { 0 };
                assert_eq!(dictionary.fetch_byte(start + offset).unwrap(), expected);
            }
        }
    }

    #[test]
    fn byte_access_covers_exactly_the_written_bytes() {
        unsafe {
            let mut dictionary = dictionary_with(&[0]);
            let start = dictionary.start as Cell;
            let here = dictionary.here();

            assert!(dictionary.fetch_byte(start).is_ok());
            assert!(dictionary.fetch_byte(here - 1).is_ok());
            for addr in [start - 1, Cell::MAX, dictionary.end as Cell] {
                assert!(matches!(
                    dictionary.fetch_byte(addr),
                    Err(ForthError::AddressOutOfBounds(a)) if a == addr
                ));
                assert!(matches!(
                    dictionary.store_byte(addr, 1),
                    Err(ForthError::AddressOutOfBounds(a)) if a == addr
                ));
            }
            for addr in [here, here + 1, dictionary.end as Cell - 1] {
                assert!(matches!(
                    dictionary.fetch_byte(addr),
                    Err(ForthError::UnwrittenAddress(a)) if a == addr
                ));
                assert!(matches!(
                    dictionary.store_byte(addr, 1),
                    Err(ForthError::UnwrittenAddress(a)) if a == addr
                ));
            }
        }
    }

    #[test]
    fn a_cell_that_ends_past_here_is_rejected() {
        unsafe {
            let mut dictionary = dictionary_with(&[1]);
            let addr = dictionary.here();
            dictionary.char_comma(2).unwrap();
            assert!(addr < dictionary.here());

            assert_rejected(&mut dictionary, addr, ForthError::UnwrittenAddress);
        }
    }

    #[test]
    fn cell_access_checks_bounds_then_here_then_alignment() {
        unsafe {
            let mut dictionary = dictionary_with(&[1, 2]);
            let start = dictionary.start as Cell;
            let here = dictionary.here();
            let end = dictionary.end as Cell;

            assert_eq!(dictionary.fetch(start).unwrap(), 1);
            assert_eq!(dictionary.fetch(here - CELL_SIZE).unwrap(), 2);
            assert!(dictionary.store(here - CELL_SIZE, 3).is_ok());

            assert_rejected(&mut dictionary, 0, ForthError::AddressOutOfBounds);
            assert_rejected(&mut dictionary, start - 1, ForthError::AddressOutOfBounds);
            assert_rejected(
                &mut dictionary,
                start - CELL_SIZE,
                ForthError::AddressOutOfBounds,
            );
            assert_rejected(&mut dictionary, end, ForthError::AddressOutOfBounds);
            assert_rejected(&mut dictionary, Cell::MAX, ForthError::AddressOutOfBounds);

            assert_rejected(&mut dictionary, here, ForthError::UnwrittenAddress);
            assert_rejected(&mut dictionary, here - 1, ForthError::UnwrittenAddress);
            assert_rejected(&mut dictionary, here + 1, ForthError::UnwrittenAddress);
            assert_rejected(
                &mut dictionary,
                end - CELL_SIZE,
                ForthError::UnwrittenAddress,
            );
            assert_rejected(&mut dictionary, end - 1, ForthError::UnwrittenAddress);

            assert_rejected(&mut dictionary, start + 1, ForthError::MisalignedAddress);
            assert_rejected(
                &mut dictionary,
                here - CELL_SIZE + 1,
                ForthError::UnwrittenAddress,
            );
        }
    }

    #[test]
    fn char_comma_stores_one_byte_and_can_leave_here_unaligned() {
        unsafe {
            let mut dictionary = dictionary_with(&[]);
            let start = dictionary.here();
            dictionary.char_comma(7).unwrap();

            assert_eq!(dictionary.here(), start + 1);
            assert_eq!(dictionary.fetch_byte(start).unwrap(), 7);
        }
    }

    #[test]
    fn char_comma_fills_the_arena_and_then_overflows() {
        unsafe {
            let mut dictionary = Dictionary::new(1).unwrap();
            for byte in 0..CELL_SIZE {
                dictionary.char_comma(byte as u8).unwrap();
            }
            let here = dictionary.here();

            assert!(matches!(
                dictionary.char_comma(0),
                Err(ForthError::DictionaryOverflow)
            ));
            assert_eq!(dictionary.here(), here);
        }
    }

    #[test]
    fn align_moves_here_and_clears_the_bytes_it_passes() {
        unsafe {
            let mut dictionary = dictionary_with(&[Cell::MAX]);
            let start = dictionary.start as Cell;
            let mark = dictionary.mark();
            dictionary.restore(Mark {
                here: dictionary.start as *mut u8,
                ..mark
            });
            dictionary.char_comma(1).unwrap();
            dictionary.align();

            assert_eq!(dictionary.here(), start + CELL_SIZE);
            assert_eq!(
                dictionary.fetch(start).unwrap(),
                Cell::from_ne_bytes({
                    let mut bytes = [0; CELL_SIZE];
                    bytes[0] = 1;
                    bytes
                })
            );
        }
    }

    #[test]
    fn align_leaves_an_aligned_here_alone() {
        unsafe {
            let mut dictionary = dictionary_with(&[1]);
            let here = dictionary.here();
            dictionary.align();
            assert_eq!(dictionary.here(), here);
        }
    }

    #[test]
    fn align_stops_at_the_end_of_the_arena() {
        unsafe {
            let mut dictionary = Dictionary::new(1).unwrap();
            dictionary.char_comma(1).unwrap();
            dictionary.align();
            assert_eq!(dictionary.here(), dictionary.end as Cell);
        }
    }

    #[test]
    fn comma_aligns_here_first() {
        unsafe {
            let mut dictionary = dictionary_with(&[]);
            let start = dictionary.here();
            dictionary.char_comma(1).unwrap();
            dictionary.comma(7).unwrap();

            assert_eq!(dictionary.here(), start + 2 * CELL_SIZE);
            assert_eq!(dictionary.fetch(start + CELL_SIZE).unwrap(), 7);
        }
    }

    #[test]
    fn comma_that_does_not_fit_after_aligning_leaves_here_alone() {
        unsafe {
            let mut dictionary = Dictionary::new(1).unwrap();
            dictionary.char_comma(1).unwrap();
            let here = dictionary.here();

            assert!(matches!(
                dictionary.comma(2),
                Err(ForthError::DictionaryOverflow)
            ));
            assert_eq!(dictionary.here(), here);
        }
    }

    #[test]
    fn write_header_aligns_here_first() {
        unsafe {
            let mut dictionary = dictionary_with(&[]);
            dictionary.char_comma(1).unwrap();
            let header = dictionary.write_header("X", 0, 0).unwrap();

            assert!((header as Cell).is_multiple_of(CELL_SIZE));
            assert!(dictionary.find_word("X").is_some());
            assert!(dictionary.here().is_multiple_of(CELL_SIZE));
        }
    }

    #[test]
    fn write_header_that_does_not_fit_after_aligning_changes_nothing() {
        unsafe {
            // The name and header fill the arena exactly when HERE starts aligned.
            let mut dictionary = Dictionary::new(HEADER_CELLS + 1).unwrap();
            dictionary.char_comma(1).unwrap();
            let mark = dictionary.mark();

            assert!(matches!(
                dictionary.write_header("X", 0, 0),
                Err(ForthError::DictionaryOverflow)
            ));
            assert_eq!(dictionary.mark(), mark);
        }
    }

    #[test]
    fn restore_returns_to_an_unaligned_here() {
        unsafe {
            let mut dictionary = dictionary_with(&[]);
            dictionary.char_comma(1).unwrap();
            let mark = dictionary.mark();
            dictionary.comma(2).unwrap();
            dictionary.restore(mark);
            assert_eq!(dictionary.mark(), mark);
        }
    }

    #[test]
    fn allot_reserves_and_clears_bytes() {
        unsafe {
            let mut dictionary = dictionary_with(&[Cell::MAX]);
            let start = dictionary.start as Cell;
            let mark = dictionary.mark();
            dictionary.restore(Mark {
                here: dictionary.start as *mut u8,
                ..mark
            });
            dictionary.allot(3).unwrap();

            assert_eq!(dictionary.here(), start + 3);
            for offset in 0..3 {
                assert_eq!(dictionary.fetch_byte(start + offset).unwrap(), 0);
            }
        }
    }

    #[test]
    fn allot_fills_the_arena_and_then_overflows() {
        unsafe {
            let mut dictionary = Dictionary::new(2).unwrap();
            let here = dictionary.here();
            let size = (2 * CELL_SIZE) as isize;

            assert!(matches!(
                dictionary.allot(size + 1),
                Err(ForthError::DictionaryOverflow)
            ));
            assert!(matches!(
                dictionary.allot(isize::MAX),
                Err(ForthError::DictionaryOverflow)
            ));
            assert_eq!(dictionary.here(), here);

            dictionary.allot(size).unwrap();
            assert_eq!(dictionary.here(), dictionary.end as Cell);
        }
    }

    #[test]
    fn allot_gives_back_the_data_of_a_created_word() {
        unsafe {
            let mut dictionary = dictionary_with(&[]);
            dictionary.write_header("X", 0, F_CREATED).unwrap();
            let data = dictionary.here();
            dictionary.allot(5).unwrap();
            dictionary.allot(-3).unwrap();
            assert_eq!(dictionary.here(), data + 2);
            dictionary.allot(-2).unwrap();
            assert_eq!(dictionary.here(), data);
        }
    }

    #[test]
    fn allot_rejects_going_below_the_data_of_a_created_word() {
        unsafe {
            let mut dictionary = dictionary_with(&[]);
            dictionary.write_header("X", 0, F_CREATED).unwrap();
            dictionary.allot(2).unwrap();
            let here = dictionary.here();

            for bytes in [-3, isize::MIN] {
                assert!(matches!(
                    dictionary.allot(bytes),
                    Err(ForthError::AllotBelowData(b)) if b == bytes
                ));
            }
            assert_eq!(dictionary.here(), here);
        }
    }

    #[test]
    fn allot_gives_nothing_back_after_other_words() {
        unsafe {
            let mut empty = dictionary_with(&[1]);
            assert!(matches!(
                empty.allot(-1),
                Err(ForthError::AllotBelowData(-1))
            ));

            let mut dictionary = dictionary_with(&[]);
            dictionary.write_header("X", 0, 0).unwrap();
            dictionary.comma(1).unwrap();
            let here = dictionary.here();
            assert!(matches!(
                dictionary.allot(-1),
                Err(ForthError::AllotBelowData(-1))
            ));
            assert_eq!(dictionary.here(), here);
        }
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
            assert_fetch_rejected(&dictionary, start - 1, ForthError::AddressOutOfBounds);
            assert_fetch_rejected(
                &dictionary,
                start - CELL_SIZE,
                ForthError::AddressOutOfBounds,
            );
        }
    }

    #[test]
    fn fetch_rejects_misaligned_addresses_inside_the_written_cells() {
        unsafe {
            // The first Cell is written, so only the alignment check rejects `start + offset`.
            // A misaligned Cell in the last written Cell ends past HERE, which is checked first.
            let dictionary = dictionary_with(&[1, 2]);
            let start = dictionary.start as Cell;
            for offset in 1..CELL_SIZE {
                assert_fetch_rejected(&dictionary, start + offset, ForthError::MisalignedAddress);
                assert_fetch_rejected(
                    &dictionary,
                    start + CELL_SIZE + offset,
                    ForthError::UnwrittenAddress,
                );
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
            assert_fetch_rejected(&dictionary, addr, ForthError::UnwrittenAddress);
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
            assert_store_rejected(&mut dictionary, start - 1, ForthError::AddressOutOfBounds);
            assert_store_rejected(
                &mut dictionary,
                start - CELL_SIZE,
                ForthError::AddressOutOfBounds,
            );
        }
    }

    #[test]
    fn store_rejects_here() {
        unsafe {
            let mut dictionary = dictionary_with(&[7]);
            let here = dictionary.here as Cell;
            assert_store_rejected(&mut dictionary, here, ForthError::UnwrittenAddress);
        }
    }

    #[test]
    fn store_rejects_misaligned_addresses_inside_the_written_cells() {
        unsafe {
            // The first Cell is written, so only the alignment check rejects `start + offset`.
            // A misaligned Cell in the last written Cell ends past HERE, which is checked first.
            let mut dictionary = dictionary_with(&[1, 2]);
            let start = dictionary.start as Cell;
            for offset in 1..CELL_SIZE {
                assert_store_rejected(
                    &mut dictionary,
                    start + offset,
                    ForthError::MisalignedAddress,
                );
                assert_store_rejected(
                    &mut dictionary,
                    start + CELL_SIZE + offset,
                    ForthError::UnwrittenAddress,
                );
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
            assert_store_rejected(&mut dictionary, addr, ForthError::UnwrittenAddress);
        }
    }

    // `!` is deliberately not limited to Parameter Fields for now. This pins that down, using
    // the hidden flag because it has a visible effect without corrupting the VM. If `!`
    // gets restricted later, this test should change with it.
    #[test]
    fn store_can_change_a_word_header() {
        unsafe {
            let mut regs = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
            outer_interpreter(&mut regs, "CREATE X 7 ,").unwrap();
            let header = regs.dictionary.find_word("X").unwrap();

            let addr = header.add(LEN_FLAGS) as Cell;
            let flags = 1 | F_CREATED | F_HIDDEN; // The length of the name stays the same
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
    fn word_names_are_sorted() {
        unsafe {
            let mut dictionary = Dictionary::new(TEST_CELLS).unwrap();
            assert_eq!(dictionary.word_names(), Vec::<String>::new());

            dictionary.write_header("B", 0, 0).unwrap();
            dictionary.write_header("A", 0, 0).unwrap();
            assert_eq!(dictionary.word_names(), ["A", "B"]);
        }
    }

    #[test]
    fn word_names_holds_a_redefined_word_once() {
        unsafe {
            let mut dictionary = Dictionary::new(TEST_CELLS).unwrap();
            dictionary.write_header("X", 0, 0).unwrap();
            dictionary.write_header("X", 0, 0).unwrap();
            assert_eq!(dictionary.word_names(), ["X"]);
        }
    }

    #[test]
    fn word_names_skips_what_find_word_skips() {
        unsafe {
            let mut dictionary = Dictionary::new(TEST_CELLS).unwrap();
            dictionary.write_header("FOUND", 0, 0).unwrap();
            dictionary.write_header("HIDDEN", 0, F_HIDDEN).unwrap();
            dictionary.write_header("PENDING", 0, F_PENDING).unwrap();
            assert_eq!(dictionary.word_names(), ["FOUND"]);
        }
    }

    #[test]
    fn word_names_skips_an_empty_name() {
        unsafe {
            let mut dictionary = Dictionary::new(TEST_CELLS).unwrap();
            dictionary.write_header("", 0, 0).unwrap();
            assert_eq!(dictionary.word_names(), Vec::<String>::new());
        }
    }

    #[test]
    fn word_names_replaces_invalid_utf8() {
        unsafe {
            let mut dictionary = Dictionary::new(TEST_CELLS).unwrap();
            let header = dictionary.write_header("X", 0, 0).unwrap();
            *(*header.add(NFA) as *mut u8) = 0xff;
            assert_eq!(dictionary.word_names(), ["\u{fffd}"]);
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
            assert_eq!(*header.add(LEN_FLAGS), 1 | F_IMMEDIATE);
        }
    }

    #[test]
    fn pfa_is_the_cell_after_the_code_field() {
        unsafe {
            let mut dictionary = Dictionary::new(TEST_CELLS).unwrap();
            let header = dictionary.write_header("X", 0, 0).unwrap();
            assert_eq!(pfa_of(cfa_of(header)), header.add(CFA + 1) as *const Cell);
            assert_eq!(pfa_of(cfa_of(header)), dictionary.here as *const Cell);
        }
    }

    #[test]
    fn is_in_arena_covers_exactly_the_arena() {
        unsafe {
            let dictionary = Dictionary::new(2).unwrap();
            let start = dictionary.start as Cell;
            assert!(dictionary.is_in_arena(start));
            assert!(dictionary.is_in_arena(start + CELL_SIZE));
            assert!(dictionary.is_in_arena(start + 2 * CELL_SIZE - 1));
            assert!(!dictionary.is_in_arena(start - 1));
            assert!(!dictionary.is_in_arena(start + 2 * CELL_SIZE));
        }
    }

    #[test]
    fn is_in_arena_excludes_primitives_and_an_empty_arena() {
        unsafe {
            let dictionary = Dictionary::new(TEST_CELLS).unwrap();
            assert!(!dictionary.is_in_arena(io::sink as fn() -> io::Sink as usize));

            let empty = Dictionary::new(0).unwrap();
            assert!(!empty.is_in_arena(empty.start as Cell));
        }
    }

    #[test]
    fn flags_use_their_specified_bits() {
        assert_eq!(F_IMMEDIATE, 1 << 8);
        assert_eq!(F_HIDDEN, 1 << 9);
        assert_eq!(F_CREATED, 1 << 10);
        assert_eq!(F_PENDING, 1 << 11);
    }

    #[test]
    fn write_header_combines_the_length_and_the_flags() {
        unsafe {
            let mut dictionary = Dictionary::new(TEST_CELLS).unwrap();
            let flags = F_HIDDEN | F_CREATED;
            let header = dictionary.write_header("ABC", 0, flags).unwrap();
            assert_eq!(*header.add(LEN_FLAGS), 3 | flags);
        }
    }

    #[test]
    fn changing_flags_keeps_the_length() {
        unsafe {
            let mut dictionary = Dictionary::new(TEST_CELLS).unwrap();
            let header = dictionary.write_header("ABC", 0, F_PENDING).unwrap();

            dictionary.finish_last();
            assert_eq!(*header.add(LEN_FLAGS), 3);

            dictionary.mark_last_immediate();
            assert_eq!(*header.add(LEN_FLAGS), 3 | F_IMMEDIATE);
        }
    }

    #[test]
    fn find_word_compares_the_length_without_the_flags() {
        unsafe {
            let longest = "X".repeat(LEN_MASK);
            let mut dictionary = Dictionary::new(TEST_CELLS).unwrap();
            let flags = F_IMMEDIATE | F_CREATED;
            let short = dictionary.write_header("A", 0, flags).unwrap();
            let long = dictionary.write_header(&longest, 0, flags).unwrap();

            assert_eq!(dictionary.find_word("A"), Some(short));
            assert_eq!(dictionary.find_word(&longest), Some(long));
            assert_eq!(dictionary.find_word(&longest[1..]), None);
        }
    }

    #[test]
    fn write_header_rejects_a_name_that_does_not_fit_the_length_bits() {
        unsafe {
            let name = "X".repeat(LEN_MASK + 1);
            let mut dictionary = Dictionary::new(TEST_CELLS).unwrap();
            let mark = dictionary.mark();

            let result = dictionary.write_header(&name, 0, 0);
            assert!(matches!(result, Err(ForthError::NameTooLong(256))));
            assert_eq!(dictionary.mark(), mark);
        }
    }
}
