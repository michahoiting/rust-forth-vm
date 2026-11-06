// Copyright (c) 2026 M.W.R. Hoiting

//! Words that build the dictionary.

use super::runtime::{docol, dovar, exit};
use crate::cell::Cell;
use crate::dictionary::{cfa_of, is_created, F_CREATED, F_HIDDEN};
use crate::error::ForthError;
use crate::vm::{code_cell, Registers};

/// `, ( x -- )`: Pops a Cell from the stack and presses it into the dictionary at HERE.
pub(super) unsafe fn comma(regs: &mut Registers) {
    if let Some(value) = regs.pop() {
        let result = regs.dictionary.comma(value);
        regs.check(result);
    }
}

/// Parses a name on behalf of `word`, and allocates a new word with default DOVAR semantics.
unsafe fn create_word(regs: &mut Registers, word: &'static str) {
    let Some(name) = regs.input.parse_name() else {
        regs.set_error(ForthError::MissingName(word));
        return;
    };
    let result = regs
        .dictionary
        .write_header(&name, code_cell(dovar), F_CREATED);
    regs.check(result);
}

/// `CREATE ( -- )`: Parses a name and allocates a new word with default DOVAR semantics.
pub(super) unsafe fn create(regs: &mut Registers) {
    create_word(regs, "CREATE");
}

/// `VARIABLE ( -- )`: Parses a name and creates a word that holds one Cell, initially 0.
pub(super) unsafe fn variable(regs: &mut Registers) {
    let mark = regs.dictionary.mark();
    create_word(regs, "VARIABLE");
    if regs.error.is_none() {
        let result = regs.dictionary.comma(0);
        if regs.check(result).is_none() {
            regs.dictionary.restore(mark); // Don't leave a word without its value
        }
    }
}

/// `DOES> ( -- )`: Hijacks the CFA of the child word while the defining word is executing.
/// The Forth code compiled behind DOES> becomes the action of the child.
pub(super) unsafe fn runtime_does(regs: &mut Registers) {
    // An empty Return Stack means DOES> is running from the text interpreter. IP then
    // points into the temporary trampoline, which must not be stored in the child.
    if regs.return_stack.is_empty() {
        regs.set_error(ForthError::DoesOutsideDefinition);
        return;
    }

    // Only words made by CREATE may change their action. Changing a colon word would make it
    // run its body as data.
    let child = regs.dictionary.last_word();
    if !is_created(child) {
        regs.set_error(ForthError::DoesWithoutCreate);
        return;
    }

    // Overwrite the child's CFA with the action code, which enter_word runs through DODOES.
    // The Return Stack isn't empty, so IP is in a thread in the dictionary.
    *cfa_of(child) = regs.ip as Cell;

    // Unnest from the defining word (EXIT behavior)
    exit(regs);
}

/// `: ( -- )`: Parses a name and switches the text interpreter to compiling.
pub(super) unsafe fn colon(regs: &mut Registers) {
    let Some(name) = regs.input.parse_name() else {
        regs.set_error(ForthError::MissingName(":"));
        return;
    };
    let result = regs
        .compiler
        .begin_definition(&mut regs.dictionary, &name, code_cell(docol), 0);
    regs.check(result);
}

/// `:NONAME ( -- xt )`: Starts a definition without a name, and pushes its execution token.
pub(super) unsafe fn colon_noname(regs: &mut Registers) {
    let result =
        regs.compiler
            .begin_definition(&mut regs.dictionary, "", code_cell(docol), F_HIDDEN);
    if let Some(xt) = regs.check(result) {
        regs.data_stack.push(xt);
    }
}

/// `; ( -- )`: Ends the current definition. This word is immediate.
pub(super) unsafe fn semicolon(regs: &mut Registers) {
    let result = regs.compiler.end_definition(&mut regs.dictionary);
    regs.check(result);
}

/// `IMMEDIATE ( -- )`: Marks the most recently created word as immediate, so that it runs
/// during compilation instead of being compiled.
pub(super) unsafe fn immediate(regs: &mut Registers) {
    regs.dictionary.mark_last_immediate();
}

#[cfg(test)]
mod tests {
    use std::io;

    use super::*;
    use crate::dictionary::{is_hidden, is_immediate, is_pending, pfa_of};
    use crate::{boot, outer_interpreter};

    const TEST_CELLS: usize = 1024;

    #[test]
    fn noname_starts_a_hidden_pending_definition_and_pushes_its_xt() {
        unsafe {
            let mut regs = boot(io::sink(), TEST_CELLS).unwrap();
            colon_noname(&mut regs);

            assert!(regs.error.is_none());
            assert!(regs.compiler.is_compiling());
            let header = regs.dictionary.last_word();
            assert_eq!(regs.data_stack, [cfa_of(header) as Cell]);
            assert!(is_hidden(header));
            assert!(is_pending(header));
        }
    }

    #[test]
    fn colon_starts_a_pending_definition_that_is_not_hidden() {
        unsafe {
            let mut regs = boot(io::sink(), TEST_CELLS).unwrap();
            regs.input.load("X");
            colon(&mut regs);

            assert!(regs.error.is_none());
            assert!(regs.compiler.is_compiling());
            assert!(regs.data_stack.is_empty());
            let header = regs.dictionary.last_word();
            assert!(is_pending(header));
            assert!(!is_hidden(header));
        }
    }

    #[test]
    fn immediate_marks_the_newest_word_and_leaves_the_stack_alone() {
        unsafe {
            let mut regs = boot(io::sink(), TEST_CELLS).unwrap();
            regs.input.load("X");
            create(&mut regs);
            regs.data_stack.push(7);

            immediate(&mut regs);
            assert!(regs.error.is_none(), "got: {:?}", regs.error);
            assert_eq!(regs.data_stack, [7]);
            assert!(is_immediate(regs.dictionary.last_word()));
        }
    }

    #[test]
    fn does_stores_the_action_thread_in_the_code_field() {
        unsafe {
            let mut regs = boot(io::sink(), TEST_CELLS).unwrap();
            outer_interpreter(&mut regs, ": D CREATE DOES> ; D X").unwrap();

            // The thread of D is CREATE, DOES>, EXIT, and the action is everything after DOES>.
            let defining_word = cfa_of(regs.dictionary.find_word("D").unwrap());
            let child = cfa_of(regs.dictionary.find_word("X").unwrap());
            assert_eq!(*child, pfa_of(defining_word).add(2) as Cell);
        }
    }
}
