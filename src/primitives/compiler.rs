//! Words that build the dictionary.

use super::runtime::{docol, dodoes, dovar, exit};
use crate::cell::Cell;
use crate::dictionary::{cfa_of, is_created, F_CREATED};
use crate::vm::{code_cell, Registers};

// COMMA ( , ): Pops a Cell from the stack and presses it into the dictionary at HERE.
pub(super) unsafe fn comma(regs: &mut Registers) {
    if let Some(value) = regs.pop() {
        let result = regs.dictionary.comma(value);
        regs.check(result);
    }
}

// CREATE: Parses a name and allocates a new word with default DOVAR semantics.
pub(super) unsafe fn create(regs: &mut Registers) {
    let Some(name) = regs.input.parse_name() else {
        regs.set_error("CREATE needs a name.");
        return;
    };
    let result = regs
        .dictionary
        .write_header(&name, code_cell(dovar), F_CREATED);
    if regs.check(result).is_some() {
        let result = regs.dictionary.comma(0); // Reserve the action cell for DOES>
        regs.check(result);
    }
}

// (DOES>): Hijacks the CFA of the child word while the defining word is executing.
// The Forth code compiled behind DOES> becomes the action of the child.
pub(super) unsafe fn runtime_does(regs: &mut Registers) {
    // An empty Return Stack means DOES> is running from the text interpreter. IP then
    // points into the temporary trampoline, which must not be stored in the child.
    if regs.return_stack.is_empty() {
        regs.set_error("DOES> can only be used inside a definition.");
        return;
    }

    // Only words made by CREATE have an action cell. Anything else has live data at W + 1.
    let child = regs.dictionary.last_word();
    if !is_created(child) {
        regs.set_error("DOES> needs a word made by CREATE.");
        return;
    }

    let child_cfa = cfa_of(child);
    *child_cfa = code_cell(dodoes); // Overwrite the child's CFA with DODOES
    *child_cfa.add(1) = regs.ip as Cell; // Store the pointer to the action code in W+1 of the child

    // Unnest from the defining word (EXIT behavior)
    exit(regs);
}

// COLON ( : ): Parses a name and switches the text interpreter to compiling.
pub(super) unsafe fn colon(regs: &mut Registers) {
    let Some(name) = regs.input.parse_name() else {
        regs.set_error(": needs a name.");
        return;
    };
    let result = regs
        .compiler
        .begin_definition(&mut regs.dictionary, &name, code_cell(docol));
    regs.check(result);
}

// SEMICOLON ( ; ): Ends the current definition. This word is immediate.
pub(super) unsafe fn semicolon(regs: &mut Registers) {
    let result = regs.compiler.end_definition(&mut regs.dictionary);
    regs.check(result);
}
