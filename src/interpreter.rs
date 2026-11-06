// Copyright (c) 2026 M.W.R. Hoiting

//! The text interpreter, also known as the outer interpreter. It reads tokens from the
//! input, and either interprets or compiles each one.

use std::io::Write;

use crate::cell::Cell;
use crate::dictionary::{cfa_of, is_immediate};
use crate::error::ForthError;
use crate::vm::{execute, Registers};

/// Parses a decimal number. Accepts negative numbers as well as the full unsigned range of
/// a Cell.
fn parse_number(token: &str) -> Option<Cell> {
    let signed = token.parse::<isize>().map(|number| number as Cell);
    signed.or_else(|_| token.parse::<Cell>()).ok()
}

/// Handles a single token, either interpreting or compiling it.
///
/// # Safety
///
/// `regs` must come from [`boot`](crate::boot).
unsafe fn interpret_token(regs: &mut Registers, token: &str) {
    if let Some(header) = regs.dictionary.find_word(token) {
        // --- CASE A: THE TOKEN IS A DICTIONARY WORD ---
        let cfa = cfa_of(header);
        if regs.compiler.is_compiling() && !is_immediate(header) {
            let result = regs
                .compiler
                .compile_word(&mut regs.dictionary, cfa as Cell);
            regs.check(result);
        } else {
            execute(regs, cfa);
        }
    } else if let Some(number) = parse_number(token) {
        // --- CASE B: THE TOKEN IS A NUMBER (LITERAL) ---
        if regs.compiler.is_compiling() {
            let result = regs.compiler.compile_literal(&mut regs.dictionary, number);
            regs.check(result);
        } else {
            regs.data_stack.push(number); // Push directly onto the parameter stack
        }
    } else {
        // --- CASE C: SYNTAX / COMPILATION ERROR ---
        regs.set_error(ForthError::UnknownWord(token.to_string()));
    }
}

/// ABORT: Resets the VM after an error and discards any half-built definition.
///
/// # Safety
///
/// `regs` must come from [`boot`](crate::boot).
unsafe fn abort(regs: &mut Registers) {
    regs.data_stack.clear();
    regs.return_stack.clear();
    regs.compiler.abort(&mut regs.dictionary);
}

/// Evaluates all of `input`, which can span several lines, stopping at the first error.
///
/// Numbers and definitions persist between calls, so the stack and the dictionary carry over,
/// and a definition can continue in the next call. Output is flushed before this returns.
///
/// An error discards the rest of `input`, clears both stacks, and rolls back any unfinished
/// colon definition. Words that were already finished stay, and the VM can still be used.
///
/// # Errors
///
/// Returns the first [`ForthError`] that a word or token causes, such as
/// [`ForthError::UnknownWord`]. Returns [`ForthError::Output`] if the writer given to
/// [`boot`](crate::boot) fails. A failed flush is ignored after another error, so the
/// original error is the one reported.
///
/// # Safety
///
/// `regs` must come from [`boot`](crate::boot). The VM runs threaded code through
/// raw pointers into its dictionary, which only this crate's functions keep valid.
/// Some Forth input can still crash the process: see the limitations in the README.
///
/// # Examples
///
/// ```
/// # use std::cell::RefCell;
/// # use std::io::{self, Write};
/// # use std::rc::Rc;
/// # #[derive(Clone, Default)]
/// # struct Output(Rc<RefCell<Vec<u8>>>);
/// # impl Write for Output {
/// #     fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
/// #         self.0.borrow_mut().write(bytes)
/// #     }
/// #     fn flush(&mut self) -> io::Result<()> {
/// #         Ok(())
/// #     }
/// # }
/// use rust_forth_vm::{boot, outer_interpreter, ForthError};
///
/// let output = Output::default();
///
/// // SAFETY: this VM is only used through the crate's functions.
/// unsafe {
///     let mut regs = boot(output.clone(), 1024)?;
///     outer_interpreter(&mut regs, "40 2 + .")?;
///
///     // The error discards the rest of the input, so the second `.` never runs.
///     let error = outer_interpreter(&mut regs, "7 . 8 NO-SUCH-WORD .").unwrap_err();
///     assert!(matches!(error, ForthError::UnknownWord(_)));
///
///     // The stacks are clear, so the 8 is gone, but the VM still works.
///     let error = outer_interpreter(&mut regs, ".").unwrap_err();
///     assert!(matches!(error, ForthError::DataStackUnderflow));
///
///     // A definition can continue in the next call.
///     outer_interpreter(&mut regs, ": THREE 3")?;
///     outer_interpreter(&mut regs, "; THREE .")?;
/// }
///
/// assert_eq!(output.0.borrow().as_slice(), b"42 7 3 ");
/// # Ok::<(), ForthError>(())
/// ```
pub unsafe fn outer_interpreter(regs: &mut Registers, input: &str) -> Result<(), ForthError> {
    regs.input.load(input);

    // Tokenize the input string by whitespace (spaces, tabs, newlines)
    while let Some(token) = regs.input.parse_name() {
        interpret_token(regs, &token);
        if let Some(error) = regs.error.take() {
            abort(regs);
            let _ = regs.output.flush(); // Keep earlier output, but report the original error
            return Err(error);
        }
    }
    if let Err(error) = regs.output.flush() {
        abort(regs);
        return Err(ForthError::Output(error));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io;

    use super::*;
    use crate::boot;
    use crate::vm::run;

    const TEST_CELLS: usize = 1024;

    // Asserts that evaluating `input` fails with an error matching the pattern.
    macro_rules! assert_error {
        ($regs:expr, $input:expr, $($pattern:tt)+) => {
            let error = outer_interpreter($regs, $input).unwrap_err();
            assert!(matches!(&error, $($pattern)+), "got: {}", error);
        };
    }

    #[test]
    fn failed_definition_is_rolled_back() {
        unsafe {
            let mut regs = boot(io::sink(), TEST_CELLS).unwrap();
            let mark = regs.dictionary.mark();
            assert_error!(&mut regs, ": BROKEN 1 NOPE ;", ForthError::UnknownWord(name) if name == "NOPE");
            assert_eq!(regs.dictionary.mark(), mark);
            assert!(!regs.compiler.is_compiling());
        }
    }

    #[test]
    fn dictionary_overflow_is_reported() {
        unsafe {
            let mut regs = boot(io::sink(), TEST_CELLS).unwrap();
            let cells = "1 , ".repeat(TEST_CELLS);
            assert_error!(&mut regs, &cells, ForthError::DictionaryOverflow);
            assert_error!(&mut regs, "1 ,", ForthError::DictionaryOverflow);
            assert_error!(&mut regs, "CREATE X", ForthError::DictionaryOverflow);
        }
    }

    #[test]
    fn long_threads_do_not_grow_the_rust_stack() {
        unsafe {
            let mut regs = boot(io::sink(), TEST_CELLS).unwrap();
            outer_interpreter(&mut regs, ": NOOP ;").unwrap();
            let cfa = cfa_of(regs.dictionary.find_word("NOOP").unwrap());

            // Miri is far too slow for the full length, but still checks each step.
            let cells = if cfg!(miri) { 1_000 } else { 5_000_000 };
            let mut thread = vec![cfa as Cell; cells];
            thread.push(0);
            regs.ip = thread.as_ptr();
            run(&mut regs);

            assert!(regs.error.is_none());
            assert!(regs.return_stack.is_empty());
        }
    }
}
