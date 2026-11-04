//! The dictionary: the word header layout, and the functions that write to and
//! search the linked list of words. Its storage lives in [`Registers`].

use std::mem;
use std::ptr;

use crate::vm::{code_cell, Cell, PrimitiveFn, Registers};

// Size of the dictionary arena, in Cells.
pub(crate) const DICTIONARY_CELLS: usize = 1024;

// Layout of a dictionary header, relative to its LFA. Every word uses this
// layout, whether it is a primitive or was created at runtime. The name bytes
// are stored in the dictionary directly in front of the header.
pub(crate) const LFA: usize = 0; // Link Field: points to the LFA of the previous word
pub(crate) const NFA: usize = 1; // Name Field: points to the name bytes
pub(crate) const LEN: usize = 2; // Length of the name in bytes
pub(crate) const FLAGS: usize = 3; // Combination of the F_* flags below
pub(crate) const CFA: usize = 4; // Code Field: the machine-code pointer
pub(crate) const HEADER_CELLS: usize = 5;

pub(crate) const F_IMMEDIATE: Cell = 1; // Executes even while compiling
pub(crate) const F_HIDDEN: Cell = 2; // Invisible to dictionary lookups
pub(crate) const F_CREATED: Cell = 4; // Made by CREATE, so it owns an action cell for DOES>

// Presses a Cell into the dictionary at HERE.
pub(crate) unsafe fn dictionary_comma(regs: &mut Registers, value: Cell) {
    if regs.here >= regs.dictionary_end {
        regs.set_error("Dictionary overflow.");
        return;
    }
    *regs.here = value;
    regs.here = regs.here.add(1);
}

// Writes a complete word into the dictionary: the name bytes, then the header.
// Returns the LFA of the new word, or None if the dictionary is full.
pub(crate) unsafe fn write_header(
    regs: &mut Registers,
    name: &str,
    code: Cell,
    flags: Cell,
) -> Option<*mut Cell> {
    let name_cells = name.len().div_ceil(mem::size_of::<Cell>());
    let free_cells = regs.dictionary_end.offset_from(regs.here) as usize;
    if name_cells + HEADER_CELLS > free_cells {
        regs.set_error("Dictionary overflow.");
        return None;
    }

    // The dictionary owns its own copy, because the input line does not outlive this call.
    let name_ptr = regs.here as *mut u8;
    ptr::copy_nonoverlapping(name.as_ptr(), name_ptr, name.len());

    let header = regs.here.add(name_cells);
    *header.add(LFA) = regs.last_word as Cell;
    *header.add(NFA) = name_ptr as Cell;
    *header.add(LEN) = name.len();
    *header.add(FLAGS) = flags;
    *header.add(CFA) = code;

    regs.here = header.add(HEADER_CELLS);
    regs.last_word = header; // Update the head pointer of the dictionary
    Some(header)
}

// Simulates the built-in bootstrap dictionary populated when a Forth system boots.
// Returns the CFA of the new word.
pub(crate) unsafe fn add_primitive_word(
    regs: &mut Registers,
    name: &str,
    code_fn: PrimitiveFn,
    flags: Cell,
) -> Cell {
    let header = write_header(regs, name, code_cell(code_fn), flags)
        .expect("the dictionary must be able to hold the primitives");
    header.add(CFA) as Cell
}

// Linearly traverses the linked list of the dictionary (from newest to oldest).
// Returns the LFA of the matching word.
pub(crate) unsafe fn find_word(regs: &Registers, token: &str) -> Option<*mut Cell> {
    let mut current_lfa = regs.last_word;

    while !current_lfa.is_null() {
        let nfa_ptr = *current_lfa.add(NFA) as *const u8;
        let len = *current_lfa.add(LEN);
        let hidden = *current_lfa.add(FLAGS) & F_HIDDEN != 0;

        // Reconstruct the name from raw dictionary data pointers
        if !hidden && std::slice::from_raw_parts(nfa_ptr, len) == token.as_bytes() {
            return Some(current_lfa);
        }
        // Follow the link pointer to the previous word (LFA)
        current_lfa = *current_lfa.add(LFA) as *mut Cell;
    }
    None
}
