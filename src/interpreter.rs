use crate::cell::Cell;
use crate::dictionary::{cfa_of, is_immediate};
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
        regs.set_error(format!(
            "Word or numeric literal '{}' not recognized.",
            token
        ));
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
/// Forth code has raw memory access: `@` dereferences any address it is given,
/// so the input must not fetch from an invalid address.
pub unsafe fn outer_interpreter(regs: &mut Registers, input: &str) -> Result<(), String> {
    regs.input.load(input);

    // Tokenize the input string by whitespace (spaces, tabs, newlines)
    while let Some(token) = regs.input.parse_name() {
        interpret_token(regs, &token);
        if let Some(message) = regs.error.take() {
            abort(regs);
            return Err(message);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boot;
    use crate::dictionary::DICTIONARY_CELLS;
    use crate::vm::run;

    // Evaluates the lines in order, and returns the resulting data stack.
    unsafe fn eval_stack(lines: &[&str]) -> Vec<Cell> {
        let mut regs = boot();
        for line in lines {
            outer_interpreter(&mut regs, line).unwrap();
        }
        regs.data_stack
    }

    // Asserts that evaluating `input` fails with a message containing `expected`.
    unsafe fn assert_error(regs: &mut Registers, input: &str, expected: &str) {
        let message = outer_interpreter(regs, input).unwrap_err();
        assert!(message.contains(expected), "got: {}", message);
    }

    #[test]
    fn arithmetic_and_literals() {
        unsafe {
            assert_eq!(eval_stack(&["40 2 +"]), vec![42]);
            assert_eq!(eval_stack(&["-5 3 +"]), vec![-2isize as Cell]);
            assert_eq!(eval_stack(&["18446744073709551615"]), vec![Cell::MAX]);
        }
    }

    #[test]
    fn create_comma_fetch() {
        unsafe {
            assert_eq!(eval_stack(&["CREATE X 1337 , X @"]), vec![1337]);
        }
    }

    #[test]
    fn name_is_owned_by_the_dictionary() {
        unsafe {
            let mut regs = boot();
            let line = String::from("CREATE X 1 ,");
            outer_interpreter(&mut regs, &line).unwrap();
            drop(line);
            outer_interpreter(&mut regs, "X @").unwrap();
            assert_eq!(regs.data_stack, vec![1]);
        }
    }

    #[test]
    fn colon_definitions_nest() {
        unsafe {
            let lines = [": ADD3 3 + ;", ": ADD6 ADD3 ADD3 ;", "36 ADD6"];
            assert_eq!(eval_stack(&lines), vec![42]);
        }
    }

    #[test]
    fn definition_can_span_lines() {
        unsafe {
            assert_eq!(eval_stack(&[": A 1", "2 + ;", "A"]), vec![3]);
        }
    }

    #[test]
    fn redefinition_calls_the_previous_word() {
        unsafe {
            assert_eq!(eval_stack(&[": X 1 ;", ": X X 2 + ;", "X"]), vec![3]);
        }
    }

    #[test]
    fn exit_leaves_a_definition_early() {
        unsafe {
            assert_eq!(eval_stack(&[": A 1 EXIT 2 ;", "A"]), vec![1]);
        }
    }

    #[test]
    fn does_gives_each_child_its_own_data() {
        unsafe {
            let lines = [
                ": CONSTANT CREATE , DOES> @ ;",
                "42 CONSTANT ANSWER",
                "7 CONSTANT LUCKY",
                "ANSWER LUCKY ANSWER",
            ];
            assert_eq!(eval_stack(&lines), vec![42, 7, 42]);
        }
    }

    #[test]
    fn does_requires_a_definition() {
        unsafe {
            let mut regs = boot();
            assert_error(&mut regs, "CREATE X DOES>", "inside a definition");
        }
    }

    #[test]
    fn does_requires_a_created_word() {
        unsafe {
            let mut regs = boot();
            outer_interpreter(&mut regs, ": BAD 1 DOES> ;").unwrap();
            assert_error(&mut regs, "BAD", "made by CREATE");
        }
    }

    #[test]
    fn failed_definition_is_rolled_back() {
        unsafe {
            let mut regs = boot();
            let mark = regs.dictionary.mark();
            assert_error(&mut regs, ": BROKEN 1 NOPE ;", "not recognized");
            assert_eq!(regs.dictionary.mark(), mark);
            assert!(!regs.compiler.is_compiling());
            assert_error(&mut regs, "BROKEN", "not recognized");
            outer_interpreter(&mut regs, "1 2 +").unwrap();
            assert_eq!(regs.data_stack, vec![3]);
        }
    }

    #[test]
    fn stack_underflow_is_reported() {
        unsafe {
            let mut regs = boot();
            assert_error(&mut regs, "1 +", "underflow");
            assert_error(&mut regs, ".", "underflow");
            assert_error(&mut regs, "@", "underflow");
            assert_error(&mut regs, ",", "underflow");
            assert_error(&mut regs, "EXIT", "underflow");
        }
    }

    #[test]
    fn misused_words_are_rejected() {
        unsafe {
            let mut regs = boot();
            assert_error(&mut regs, ";", "while compiling");
            assert_error(&mut regs, ":", "needs a name");
            assert_error(&mut regs, "CREATE", "needs a name");
            assert_error(&mut regs, "LIT", "not recognized");
        }
    }

    #[test]
    fn dictionary_overflow_is_reported() {
        unsafe {
            let mut regs = boot();
            let cells = "1 , ".repeat(DICTIONARY_CELLS);
            assert_error(&mut regs, &cells, "Dictionary overflow");
            assert_error(&mut regs, "1 ,", "Dictionary overflow");
            assert_error(&mut regs, "CREATE X", "Dictionary overflow");
        }
    }

    #[test]
    fn long_threads_do_not_grow_the_rust_stack() {
        unsafe {
            let mut regs = boot();
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
