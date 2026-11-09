// Copyright (c) 2026 M.W.R. Hoiting

//! Words that read the input themselves, instead of leaving it to the text interpreter.
//! Most of them are immediate, so that they also work inside definitions.

use crate::cell::Cell;
use crate::error::ForthError;
use crate::vm::Vm;

/// `( ( -- )`: Skips the input up to and including the next `)`. Comments don't nest.
pub(super) fn paren_comment(vm: &mut Vm) {
    if !vm.input.skip_past(')') {
        vm.set_error(ForthError::UnterminatedComment);
    }
}

/// `\ ( -- )`: Skips the rest of the line.
pub(super) fn line_comment(vm: &mut Vm) {
    vm.input.skip_line();
}

/// Parses a name on behalf of `word`, and returns the execution token of that word.
fn parse_xt(vm: &mut Vm, word: &'static str) -> Option<Cell> {
    let Some(name) = vm.input.parse_name() else {
        vm.set_error(ForthError::MissingName(word));
        return None;
    };
    match vm.dictionary.find_word(&name) {
        Some(xt) => Some(xt),
        None => {
            vm.set_error(ForthError::UnknownWord(name));
            None
        }
    }
}

/// `' ( -- xt )`: Parses a name and pushes the execution token of that word.
pub(super) fn tick(vm: &mut Vm) {
    if let Some(xt) = parse_xt(vm, "'") {
        vm.push(xt);
    }
}

/// `['] ( -- )`: Parses a name while compiling, and compiles its execution token as a literal.
pub(super) fn bracket_tick(vm: &mut Vm) {
    if !vm.compiler.is_compiling() {
        vm.set_error(ForthError::NotCompiling("[']"));
        return;
    }
    if let Some(xt) = parse_xt(vm, "[']") {
        let result = vm.compiler.compile_literal(&mut vm.dictionary, xt);
        vm.check(result);
    }
}

#[cfg(test)]
mod tests {
    use std::io;

    use super::*;
    use crate::compiler::LIT_XT;
    use crate::{boot, outer_interpreter, VmConfig};

    const TEST_CELLS: usize = 64 * 1024;

    #[test]
    fn paren_comment_skips_to_the_closing_parenthesis() {
        let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
        vm.input.load("skipped ) next");
        vm.data_stack.push(7);

        paren_comment(&mut vm);
        assert!(vm.error.is_none(), "got: {:?}", vm.error);
        assert_eq!(vm.data_stack, [7]);
        assert_eq!(vm.input.parse_name().as_deref(), Some("next"));
    }

    #[test]
    fn paren_comment_reports_a_missing_parenthesis() {
        let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
        vm.input.load("never closed");

        paren_comment(&mut vm);
        assert!(matches!(vm.error, Some(ForthError::UnterminatedComment)));
    }

    #[test]
    fn line_comment_skips_the_rest_of_the_line() {
        let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
        vm.input.load("skipped\nnext");
        vm.data_stack.push(7);

        line_comment(&mut vm);
        assert!(vm.error.is_none(), "got: {:?}", vm.error);
        assert_eq!(vm.data_stack, [7]);
        assert_eq!(vm.input.parse_name().as_deref(), Some("next"));
    }

    #[test]
    fn tick_pushes_the_execution_token() {
        let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
        vm.input.load("DUP next");
        vm.data_stack.push(7);

        tick(&mut vm);
        assert!(vm.error.is_none(), "got: {:?}", vm.error);
        let xt = vm.dictionary.find_word("DUP").unwrap();
        assert_eq!(vm.data_stack, [7, xt]);
        assert_eq!(vm.input.parse_name().as_deref(), Some("next"));
    }

    #[test]
    fn tick_leaves_the_stack_alone_on_an_error() {
        let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
        vm.input.load("NOPE");
        vm.data_stack.push(7);

        tick(&mut vm);
        assert!(matches!(
            &vm.error,
            Some(ForthError::UnknownWord(name)) if name == "NOPE"
        ));
        assert_eq!(vm.data_stack, [7]);
    }

    #[test]
    fn bracket_tick_compiles_the_xt_as_a_literal() {
        let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
        outer_interpreter(&mut vm, ": X").unwrap();
        let xt = vm.dictionary.find_word("DUP").unwrap();
        let body = vm.dictionary.here();
        vm.input.load("DUP");
        bracket_tick(&mut vm);

        assert!(vm.error.is_none(), "got: {:?}", vm.error);
        assert!(vm.data_stack.is_empty());
        assert_eq!(vm.dictionary.fetch_two(body).unwrap(), (LIT_XT, xt));
    }

    #[test]
    fn bracket_tick_only_works_while_compiling() {
        let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
        vm.input.load("DUP next");

        bracket_tick(&mut vm);
        assert!(matches!(vm.error, Some(ForthError::NotCompiling("[']"))));
        assert_eq!(vm.input.parse_name().as_deref(), Some("DUP"));
    }
}
