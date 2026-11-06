// Copyright (c) 2026 M.W.R. Hoiting

//! Words that build the dictionary.

use super::runtime::{docol, docon, dodoes, dovar, exit};
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
    if regs.check(result).is_some() {
        let result = regs.dictionary.comma(0); // Reserve the action cell for DOES>
        regs.check(result);
    }
}

/// `CREATE ( -- )`: Parses a name and allocates a new word with default DOVAR semantics.
pub(super) unsafe fn create(regs: &mut Registers) {
    create_word(regs, "CREATE");
}

/// `VARIABLE ( -- )`: Parses a name and creates a word that holds one Cell, initially 0.
pub(super) unsafe fn variable(regs: &mut Registers) {
    create_word(regs, "VARIABLE");
    if regs.error.is_none() {
        let result = regs.dictionary.comma(0);
        regs.check(result);
    }
}

/// `CONSTANT ( x -- )`: Parses a name and creates a word that pushes `x`. The word is not
/// marked as made by CREATE, so DOES> can't overwrite its value.
pub(super) unsafe fn constant(regs: &mut Registers) {
    let Some(value) = regs.pop() else {
        return;
    };
    let Some(name) = regs.input.parse_name() else {
        regs.set_error(ForthError::MissingName("CONSTANT"));
        return;
    };
    let restore_point = regs.dictionary.mark();
    let result = regs.dictionary.write_header(&name, code_cell(docon), 0);
    if regs.check(result).is_some() {
        let result = regs.dictionary.comma(value);
        if regs.check(result).is_none() {
            // A word without its value would read past the dictionary when it ran.
            regs.dictionary.restore(restore_point);
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

    // Only words made by CREATE have an action cell. Anything else has live data at W + 1.
    let child = regs.dictionary.last_word();
    if !is_created(child) {
        regs.set_error(ForthError::DoesWithoutCreate);
        return;
    }

    let child_cfa = cfa_of(child);
    *child_cfa = code_cell(dodoes); // Overwrite the child's CFA with DODOES
    *child_cfa.add(1) = regs.ip as Cell; // Store the pointer to the action code in W+1 of the child

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
    use crate::boot;
    use crate::dictionary::{is_hidden, is_immediate, is_pending};
    use crate::vm::enter_word;

    const TEST_CELLS: usize = 1024;

    #[test]
    fn constant_moves_its_value_into_a_new_word() {
        unsafe {
            let mut regs = boot(io::sink(), TEST_CELLS).unwrap();
            regs.input.load("X");
            regs.data_stack.extend_from_slice(&[1, 42]);

            constant(&mut regs);
            assert!(regs.error.is_none(), "got: {:?}", regs.error);
            assert_eq!(regs.data_stack, [1]);
            let header = regs.dictionary.last_word();
            assert_eq!(regs.dictionary.find_word("X"), Some(header));
            assert_eq!(*cfa_of(header).add(1), 42);
            assert!(!is_created(header));
            assert!(!is_hidden(header));
            assert!(!is_immediate(header));
            assert!(!is_pending(header));

            // Running the word shows which code it has. Function addresses can't show it,
            // because Rust doesn't guarantee that they compare equal.
            regs.w = cfa_of(header);
            enter_word(&mut regs);
            assert_eq!(regs.data_stack, [1, 42]);
        }
    }

    #[test]
    fn constant_needs_a_value_and_writes_nothing_without_one() {
        unsafe {
            let mut regs = boot(io::sink(), TEST_CELLS).unwrap();
            regs.input.load("X");
            let before = regs.dictionary.mark();

            constant(&mut regs);
            assert!(matches!(regs.error, Some(ForthError::DataStackUnderflow)));
            assert_eq!(regs.dictionary.mark(), before);
        }
    }

    #[test]
    fn constant_needs_a_name_and_writes_nothing_without_one() {
        unsafe {
            let mut regs = boot(io::sink(), TEST_CELLS).unwrap();
            regs.input.load("");
            regs.data_stack.push(42);
            let before = regs.dictionary.mark();

            constant(&mut regs);
            assert!(matches!(
                regs.error,
                Some(ForthError::MissingName("CONSTANT"))
            ));
            assert_eq!(regs.dictionary.mark(), before);
        }
    }

    #[test]
    fn constant_that_does_not_fit_writes_nothing() {
        unsafe {
            // The last size that fails has room for the header, but not for the value.
            let smallest = (0..TEST_CELLS)
                .find(|cells| boot(io::sink(), *cells).is_ok())
                .unwrap();
            for cells in smallest..TEST_CELLS {
                let mut regs = boot(io::sink(), cells).unwrap();
                regs.input.load("X");
                regs.data_stack.push(42);
                let before = regs.dictionary.mark();

                constant(&mut regs);
                if regs.error.is_none() {
                    assert!(cells > smallest);
                    return;
                }
                assert!(matches!(regs.error, Some(ForthError::DictionaryOverflow)));
                assert_eq!(regs.dictionary.mark(), before);
            }
            panic!("no dictionary size was large enough");
        }
    }

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
}
