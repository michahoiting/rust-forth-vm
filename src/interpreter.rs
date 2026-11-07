// Copyright (c) 2026 M.W.R. Hoiting

//! The text interpreter, also known as the outer interpreter. It reads tokens from the
//! input, and either interprets or compiles each one.

use std::io::Write;

use crate::cell::Cell;
use crate::dictionary::{cfa_of, is_immediate};
use crate::error::ForthError;
use crate::vm::{trampoline_cfa, Vm};

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
/// `vm` must come from [`boot`](crate::boot).
unsafe fn interpret_token(vm: &mut Vm, token: &str) {
    if let Some(header) = vm.dictionary.find_word(token) {
        // --- CASE A: THE TOKEN IS A DICTIONARY WORD ---
        let cfa = cfa_of(header);
        if vm.compiler.is_compiling() && !is_immediate(header) {
            let result = vm.compiler.compile_word(&mut vm.dictionary, cfa as Cell);
            vm.check(result);
        } else {
            trampoline_cfa(vm, cfa);
        }
    } else if let Some(number) = parse_number(token) {
        // --- CASE B: THE TOKEN IS A NUMBER (LITERAL) ---
        if vm.compiler.is_compiling() {
            let result = vm.compiler.compile_literal(&mut vm.dictionary, number);
            vm.check(result);
        } else {
            vm.push(number); // Push directly onto the parameter stack
        }
    } else {
        // --- CASE C: SYNTAX / COMPILATION ERROR ---
        vm.set_error(ForthError::UnknownWord(token.to_string()));
    }
}

/// QUIT: Clears the return stack and returns to interpreting, discarding any half-built
/// definition, which could never be finished or used.
///
/// # Safety
///
/// `vm` must come from [`boot`](crate::boot).
unsafe fn quit(vm: &mut Vm) {
    vm.return_stack.clear();
    vm.compiler.abort(&mut vm.dictionary);
}

/// ABORT: Resets the VM after an error, by clearing the data stack and doing a [`quit`].
///
/// # Safety
///
/// `vm` must come from [`boot`](crate::boot).
unsafe fn abort(vm: &mut Vm) {
    vm.data_stack.clear();
    quit(vm);
}

/// Evaluates all of `input`, which can span several lines, stopping at the first error.
///
/// Numbers and definitions persist between calls, so the stack and the dictionary carry over,
/// and a definition can continue in the next call. Output is flushed before this returns.
///
/// An error discards the rest of `input`, clears both stacks, and rolls back any unfinished
/// colon definition. Words that were already finished stay, and the VM can still be used.
///
/// The word `QUIT` does the same, except that it keeps the data stack. It isn't a failure, but
/// it's reported as [`ForthError::Quit`] so that a host knows the input didn't finish. The
/// word `ABORT` does what an error does, and is reported as [`ForthError::Abort`]. `BYE` stops
/// evaluation and is reported as [`ForthError::Bye`] with its exit code; the library doesn't
/// leave the process.
///
/// # Errors
///
/// Returns the first [`ForthError`] that a word or token causes, such as
/// [`ForthError::UnknownWord`], [`ForthError::Quit`], [`ForthError::Abort`] or
/// [`ForthError::Bye`] if that word ran.
/// Returns [`ForthError::Output`] if the writer given to
/// [`boot`](crate::boot) fails. A failed flush is ignored after another error, so the
/// original error is the one reported.
///
/// # Safety
///
/// `vm` must come from [`boot`](crate::boot). The VM runs threaded code through
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
/// use rust_forth_vm::{boot, outer_interpreter, ForthError, VmConfig};
///
/// let output = Output::default();
///
/// // SAFETY: this VM is only used through the crate's functions.
/// unsafe {
///     let mut vm = boot(output.clone(), VmConfig::new(1024))?;
///     outer_interpreter(&mut vm, "40 2 + .")?;
///
///     // The error discards the rest of the input, so the second `.` never runs.
///     let error = outer_interpreter(&mut vm, "7 . 8 NO-SUCH-WORD .").unwrap_err();
///     assert!(matches!(error, ForthError::UnknownWord(_)));
///
///     // The stacks are clear, so the 8 is gone, but the VM still works.
///     let error = outer_interpreter(&mut vm, ".").unwrap_err();
///     assert!(matches!(error, ForthError::DataStackUnderflow));
///
///     // A definition can continue in the next call.
///     outer_interpreter(&mut vm, ": THREE 3")?;
///     outer_interpreter(&mut vm, "; THREE .")?;
/// }
///
/// assert_eq!(output.0.borrow().as_slice(), b"42 7 3 ");
/// # Ok::<(), ForthError>(())
/// ```
pub unsafe fn outer_interpreter(vm: &mut Vm, input: &str) -> Result<(), ForthError> {
    vm.input.load(input);

    // Tokenize the input string by whitespace (spaces, tabs, newlines)
    while let Some(token) = vm.input.parse_name() {
        interpret_token(vm, &token);
        if let Some(error) = vm.error.take() {
            if matches!(error, ForthError::Quit | ForthError::Bye(_)) {
                quit(vm);
            } else {
                abort(vm);
            }
            let _ = vm.output.flush(); // Keep earlier output, but report the original error
            return Err(error);
        }
    }
    if let Err(error) = vm.output.flush() {
        abort(vm);
        return Err(ForthError::Output(error));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io;

    use super::*;
    use crate::vm::run;
    use crate::{boot, VmConfig};

    const TEST_CELLS: usize = 1024;

    // Asserts that evaluating `input` fails with an error matching the pattern.
    macro_rules! assert_error {
        ($vm:expr, $input:expr, $($pattern:tt)+) => {
            let error = outer_interpreter($vm, $input).unwrap_err();
            assert!(matches!(&error, $($pattern)+), "got: {}", error);
        };
    }

    #[test]
    fn failed_definition_is_rolled_back() {
        unsafe {
            let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
            let mark = vm.dictionary.mark();
            assert_error!(&mut vm, ": BROKEN 1 NOPE ;", ForthError::UnknownWord(name) if name == "NOPE");
            assert_eq!(vm.dictionary.mark(), mark);
            assert!(!vm.compiler.is_compiling());
        }
    }

    #[test]
    fn dictionary_overflow_is_reported() {
        unsafe {
            let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
            let cells = "1 , ".repeat(TEST_CELLS);
            assert_error!(&mut vm, &cells, ForthError::DictionaryOverflow);
            assert_error!(&mut vm, "1 ,", ForthError::DictionaryOverflow);
            assert_error!(&mut vm, "CREATE X", ForthError::DictionaryOverflow);
        }
    }

    #[test]
    fn long_threads_do_not_grow_the_rust_stack() {
        unsafe {
            let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
            outer_interpreter(&mut vm, ": NOOP ;").unwrap();
            let cfa = cfa_of(vm.dictionary.find_word("NOOP").unwrap());

            // Miri is far too slow for the full length, but still checks each step.
            let cells = if cfg!(miri) { 1_000 } else { 5_000_000 };
            let mut thread = vec![cfa as Cell; cells];
            thread.push(0);
            vm.ip = thread.as_ptr();
            run(&mut vm);

            assert!(vm.error.is_none());
            assert!(vm.return_stack.is_empty());
        }
    }
}
