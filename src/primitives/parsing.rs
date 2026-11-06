// Copyright (c) 2026 M.W.R. Hoiting

//! Words that read the input themselves, instead of leaving it to the text interpreter.
//! Most of them are immediate, so that they also work inside definitions.

use crate::cell::Cell;
use crate::dictionary::cfa_of;
use crate::error::ForthError;
use crate::vm::Registers;

/// `( ( -- )`: Skips the input up to and including the next `)`. Comments don't nest.
pub(super) unsafe fn paren_comment(regs: &mut Registers) {
    if !regs.input.skip_past(')') {
        regs.set_error(ForthError::UnterminatedComment);
    }
}

/// `\ ( -- )`: Skips the rest of the line.
pub(super) unsafe fn line_comment(regs: &mut Registers) {
    regs.input.skip_line();
}

/// Parses a name on behalf of `word`, and returns the execution token of that word.
unsafe fn parse_xt(regs: &mut Registers, word: &'static str) -> Option<Cell> {
    let Some(name) = regs.input.parse_name() else {
        regs.set_error(ForthError::MissingName(word));
        return None;
    };
    match regs.dictionary.find_word(&name) {
        Some(header) => Some(cfa_of(header) as Cell),
        None => {
            regs.set_error(ForthError::UnknownWord(name));
            None
        }
    }
}

/// `' ( -- xt )`: Parses a name and pushes the execution token of that word.
pub(super) unsafe fn tick(regs: &mut Registers) {
    if let Some(xt) = parse_xt(regs, "'") {
        regs.data_stack.push(xt);
    }
}

/// `['] ( -- )`: Parses a name while compiling, and compiles its execution token as a literal.
pub(super) unsafe fn bracket_tick(regs: &mut Registers) {
    if !regs.compiler.is_compiling() {
        regs.set_error(ForthError::NotCompiling("[']"));
        return;
    }
    if let Some(xt) = parse_xt(regs, "[']") {
        let result = regs.compiler.compile_literal(&mut regs.dictionary, xt);
        regs.check(result);
    }
}

#[cfg(test)]
mod tests {
    use std::io;

    use super::*;
    use crate::dictionary::pfa_of;
    use crate::{boot, outer_interpreter};

    const TEST_CELLS: usize = 1024;

    #[test]
    fn paren_comment_skips_to_the_closing_parenthesis() {
        unsafe {
            let mut regs = boot(io::sink(), TEST_CELLS).unwrap();
            regs.input.load("skipped ) next");
            regs.data_stack.push(7);

            paren_comment(&mut regs);
            assert!(regs.error.is_none(), "got: {:?}", regs.error);
            assert_eq!(regs.data_stack, [7]);
            assert_eq!(regs.input.parse_name().as_deref(), Some("next"));
        }
    }

    #[test]
    fn paren_comment_reports_a_missing_parenthesis() {
        unsafe {
            let mut regs = boot(io::sink(), TEST_CELLS).unwrap();
            regs.input.load("never closed");

            paren_comment(&mut regs);
            assert!(matches!(regs.error, Some(ForthError::UnterminatedComment)));
        }
    }

    #[test]
    fn line_comment_skips_the_rest_of_the_line() {
        unsafe {
            let mut regs = boot(io::sink(), TEST_CELLS).unwrap();
            regs.input.load("skipped\nnext");
            regs.data_stack.push(7);

            line_comment(&mut regs);
            assert!(regs.error.is_none(), "got: {:?}", regs.error);
            assert_eq!(regs.data_stack, [7]);
            assert_eq!(regs.input.parse_name().as_deref(), Some("next"));
        }
    }

    #[test]
    fn tick_pushes_the_execution_token() {
        unsafe {
            let mut regs = boot(io::sink(), TEST_CELLS).unwrap();
            regs.input.load("DUP next");
            regs.data_stack.push(7);

            tick(&mut regs);
            assert!(regs.error.is_none(), "got: {:?}", regs.error);
            let xt = cfa_of(regs.dictionary.find_word("DUP").unwrap()) as Cell;
            assert_eq!(regs.data_stack, [7, xt]);
            assert_eq!(regs.input.parse_name().as_deref(), Some("next"));
        }
    }

    #[test]
    fn tick_leaves_the_stack_alone_on_an_error() {
        unsafe {
            let mut regs = boot(io::sink(), TEST_CELLS).unwrap();
            regs.input.load("NOPE");
            regs.data_stack.push(7);

            tick(&mut regs);
            assert!(matches!(
                &regs.error,
                Some(ForthError::UnknownWord(name)) if name == "NOPE"
            ));
            assert_eq!(regs.data_stack, [7]);
        }
    }

    #[test]
    fn bracket_tick_compiles_the_xt_as_a_literal() {
        unsafe {
            let mut regs = boot(io::sink(), TEST_CELLS).unwrap();
            outer_interpreter(&mut regs, ": Y 5 ;").unwrap();
            let literal = cfa_of(regs.dictionary.find_word("Y").unwrap());
            let lit_cfa = *pfa_of(literal);

            outer_interpreter(&mut regs, ": X").unwrap();
            let xt = cfa_of(regs.dictionary.find_word("DUP").unwrap()) as Cell;
            let body = pfa_of(cfa_of(regs.dictionary.last_word()));
            regs.input.load("DUP");
            bracket_tick(&mut regs);

            assert!(regs.error.is_none(), "got: {:?}", regs.error);
            assert!(regs.data_stack.is_empty());
            assert_eq!(*body, lit_cfa);
            assert_eq!(*body.add(1), xt);
        }
    }

    #[test]
    fn bracket_tick_only_works_while_compiling() {
        unsafe {
            let mut regs = boot(io::sink(), TEST_CELLS).unwrap();
            regs.input.load("DUP next");

            bracket_tick(&mut regs);
            assert!(matches!(regs.error, Some(ForthError::NotCompiling("[']"))));
            assert_eq!(regs.input.parse_name().as_deref(), Some("DUP"));
        }
    }
}
