//! The dictionary: its storage, the word header layout, and the functions that
//! write to and search the linked list of words. Nothing outside this module
//! knows how a header is laid out, except that a word's body follows its CFA.

use std::mem;
use std::ptr;

use crate::cell::Cell;
use crate::error::ForthError;

// Size of the dictionary arena, in Cells.
pub(crate) const DICTIONARY_CELLS: usize = 1024;

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
pub(crate) const F_HIDDEN: Cell = 2; // Invisible to dictionary lookups
pub(crate) const F_CREATED: Cell = 4; // Made by CREATE, so it owns an action cell for DOES>

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

unsafe fn is_hidden(header: *mut Cell) -> bool {
    *header.add(FLAGS) & F_HIDDEN != 0
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
    // Allocates an untyped arena for our Forth dictionary space.
    pub(crate) unsafe fn new() -> Dictionary {
        let mut storage = vec![0 as Cell; DICTIONARY_CELLS];
        let here = storage.as_mut_ptr();
        Dictionary {
            start: here,
            here,
            last_word: ptr::null_mut(), // Dictionary starts out empty
            end: here.add(storage.len()),
            _storage: storage,
        }
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

    // Reads the Cell at `addr`, which must be a cell-aligned address below HERE.
    pub(crate) fn fetch(&self, addr: Cell) -> Result<Cell, ForthError> {
        let start = self.start as Cell;
        let here = self.here as Cell;
        let cell_size = mem::size_of::<Cell>();
        if addr < start || addr >= here || !(addr - start).is_multiple_of(cell_size) {
            return Err(ForthError::InvalidAddress(addr));
        }

        // The checks above place this Cell in the part of the arena that has been written.
        // The number from the stack is only used as an offset, never as a pointer.
        Ok(unsafe { *self.start.add((addr - start) / cell_size) })
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
    ) -> Cell {
        let header = self
            .write_header(name, code, flags)
            .expect("the dictionary must be able to hold the primitives");
        cfa_of(header) as Cell
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

    // Makes the most recently created word visible to lookups.
    pub(crate) unsafe fn reveal_last(&mut self) {
        *self.last_word.add(FLAGS) &= !F_HIDDEN;
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
