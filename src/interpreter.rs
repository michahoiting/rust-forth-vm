use std::io::Write;

use crate::cell::Cell;
use crate::dictionary::{cfa_of, is_immediate};
use crate::error::ForthError;
use crate::vm::{execute, Registers};

// --- THE TEXT INTERPRETER / OUTER INTERPRETER ---

// Accepts negative numbers as well as the full unsigned range of a Cell.
fn parse_number(token: &str) -> Option<Cell> {
    let signed = token.parse::<isize>().map(|number| number as Cell);
    signed.or_else(|_| token.parse::<Cell>()).ok()
}

// Handles a single token, either interpreting or compiling it.
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

// ABORT: Resets the VM after an error and discards any half-built definition.
unsafe fn abort(regs: &mut Registers) {
    regs.data_stack.clear();
    regs.return_stack.clear();
    regs.compiler.abort(&mut regs.dictionary);
}

/// Evaluates a line of Forth, stopping at the first error.
///
/// # Safety
///
/// `regs` must come from [`boot`](crate::boot). The VM runs threaded code through
/// raw pointers into its dictionary, which only this crate's functions keep valid.
/// The input must not use `!` to overwrite a word's header or compiled code, because
/// the VM trusts those cells.
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
    use crate::dictionary::DICTIONARY_CELLS;
    use crate::vm::run;

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
            let mut regs = boot(io::sink());
            let mark = regs.dictionary.mark();
            assert_error!(&mut regs, ": BROKEN 1 NOPE ;", ForthError::UnknownWord(name) if name == "NOPE");
            assert_eq!(regs.dictionary.mark(), mark);
            assert!(!regs.compiler.is_compiling());
        }
    }

    #[test]
    fn dictionary_overflow_is_reported() {
        unsafe {
            let mut regs = boot(io::sink());
            let cells = "1 , ".repeat(DICTIONARY_CELLS);
            assert_error!(&mut regs, &cells, ForthError::DictionaryOverflow);
            assert_error!(&mut regs, "1 ,", ForthError::DictionaryOverflow);
            assert_error!(&mut regs, "CREATE X", ForthError::DictionaryOverflow);
        }
    }

    #[test]
    fn long_threads_do_not_grow_the_rust_stack() {
        unsafe {
            let mut regs = boot(io::sink());
            outer_interpreter(&mut regs, ": NOOP ;").unwrap();
            let cfa = cfa_of(regs.dictionary.find_word("NOOP").unwrap());

            let mut thread = vec![cfa as Cell; 5_000_000];
            thread.push(0);
            regs.ip = thread.as_ptr();
            run(&mut regs);

            assert!(regs.error.is_none());
            assert!(regs.return_stack.is_empty());
        }
    }
}
