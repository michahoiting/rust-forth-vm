// Copyright (c) 2026 M.W.R. Hoiting

//! Words that build the dictionary.

use super::runtime::{docol, docon, dovar, exit};
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

/// Reports an error if a definition is open, and returns whether one is. `word` would put
/// something other than a whole Cell into its thread, which would then run as code.
fn refuse_in_definition(regs: &mut Registers, word: &'static str) -> bool {
    let open = regs.compiler.is_compiling();
    if open {
        regs.set_error(ForthError::DefinitionOpen(word));
    }
    open
}

/// `C, ( char -- )`: Pops a Cell from the stack and presses its low byte into the dictionary
/// at HERE, which can leave HERE unaligned. It can't be used while a definition is open.
pub(super) unsafe fn c_comma(regs: &mut Registers) {
    if refuse_in_definition(regs, "C,") {
        return;
    }
    if let Some(value) = regs.pop() {
        let result = regs.dictionary.char_comma(value as u8);
        regs.check(result);
    }
}

/// `ALLOT ( n -- )`: Reserves `n` bytes of the dictionary at HERE and clears them, which can
/// leave HERE unaligned. A negative `n` gives back bytes, but only from the data of the newest
/// word, and only if `CREATE` made it. It can't be used while a definition is open.
pub(super) unsafe fn allot(regs: &mut Registers) {
    if refuse_in_definition(regs, "ALLOT") {
        return;
    }
    if let Some(bytes) = regs.pop() {
        let result = regs.dictionary.allot(bytes as isize);
        regs.check(result);
    }
}

/// `ALIGN ( -- )`: Moves HERE to the next cell-aligned address, and clears the bytes it passes.
pub(super) unsafe fn align(regs: &mut Registers) {
    regs.dictionary.align();
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
    use std::mem;

    use super::*;
    use crate::dictionary::{is_hidden, is_immediate, is_pending, pfa_of};
    use crate::vm::{enter_word, PrimitiveFn};
    use crate::{boot, outer_interpreter};

    const TEST_CELLS: usize = 1024;

    #[test]
    fn c_comma_presses_the_low_byte_and_pops_it() {
        unsafe {
            let mut regs = boot(io::sink(), TEST_CELLS).unwrap();
            let addr = regs.dictionary.here();
            regs.data_stack.extend_from_slice(&[1, 0x1AB]);

            c_comma(&mut regs);
            assert!(regs.error.is_none(), "got: {:?}", regs.error);
            assert_eq!(regs.data_stack, [1]);
            assert_eq!(regs.dictionary.here(), addr + 1);
            assert_eq!(regs.dictionary.fetch_byte(addr).unwrap(), 0xAB);
        }
    }

    #[test]
    fn allot_moves_here_and_pops_the_size() {
        unsafe {
            let mut regs = boot(io::sink(), TEST_CELLS).unwrap();
            let addr = regs.dictionary.here();
            regs.data_stack.extend_from_slice(&[1, 3]);

            allot(&mut regs);
            assert!(regs.error.is_none(), "got: {:?}", regs.error);
            assert_eq!(regs.data_stack, [1]);
            assert_eq!(regs.dictionary.here(), addr + 3);
        }
    }

    #[test]
    fn allot_reads_its_size_as_a_signed_number() {
        unsafe {
            let mut regs = boot(io::sink(), TEST_CELLS).unwrap();
            regs.input.load("X");
            create(&mut regs);
            let data = regs.dictionary.here();
            regs.dictionary.allot(3).unwrap();
            regs.data_stack.push(-2isize as Cell);

            allot(&mut regs);
            assert!(regs.error.is_none(), "got: {:?}", regs.error);
            assert_eq!(regs.dictionary.here(), data + 1);
        }
    }

    #[test]
    fn align_aligns_here() {
        unsafe {
            let mut regs = boot(io::sink(), TEST_CELLS).unwrap();
            let addr = regs.dictionary.here();
            regs.dictionary.char_comma(1).unwrap();

            align(&mut regs);
            assert_eq!(regs.dictionary.here(), addr + mem::size_of::<Cell>());
        }
    }

    #[test]
    fn words_that_break_up_cells_are_refused_in_a_definition() {
        unsafe {
            for (word, name) in [(c_comma as PrimitiveFn, "C,"), (allot, "ALLOT")] {
                let mut regs = boot(io::sink(), TEST_CELLS).unwrap();
                outer_interpreter(&mut regs, ": X").unwrap();
                let before = regs.dictionary.mark();
                regs.data_stack.push(1);

                word(&mut regs);
                assert!(
                    matches!(regs.error, Some(ForthError::DefinitionOpen(n)) if n == name),
                    "got: {:?}",
                    regs.error
                );
                assert_eq!(regs.data_stack, [1]);
                assert_eq!(regs.dictionary.mark(), before);
            }
        }
    }

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
            assert_eq!(*pfa_of(cfa_of(header)), 42);
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
