// Copyright (c) 2026 M.W.R. Hoiting

//! Words that read the input themselves, instead of leaving it to the text interpreter.
//! Most of them are immediate, so that they also work inside definitions.

use crate::cell::Cell;
use crate::dictionary::cfa_of;
use crate::error::ForthError;
use crate::vm::{Vm, STRING_BUFFER_BYTES};

/// `( ( -- )`: Skips the input up to and including the next `)`. Comments don't nest.
pub(super) unsafe fn paren_comment(vm: &mut Vm) {
    if !vm.input.skip_past(')') {
        vm.set_error(ForthError::UnterminatedComment);
    }
}

/// `\ ( -- )`: Skips the rest of the line.
pub(super) unsafe fn line_comment(vm: &mut Vm) {
    vm.input.skip_line();
}

/// Parses a name on behalf of `word`, and returns the execution token of that word.
unsafe fn parse_xt(vm: &mut Vm, word: &'static str) -> Option<Cell> {
    let Some(name) = vm.input.parse_name() else {
        vm.set_error(ForthError::MissingName(word));
        return None;
    };
    match vm.dictionary.find_word(&name) {
        Some(header) => Some(cfa_of(header) as Cell),
        None => {
            vm.set_error(ForthError::UnknownWord(name));
            None
        }
    }
}

/// `' ( -- xt )`: Parses a name and pushes the execution token of that word.
pub(super) unsafe fn tick(vm: &mut Vm) {
    if let Some(xt) = parse_xt(vm, "'") {
        vm.push(xt);
    }
}

/// `['] ( -- )`: Parses a name while compiling, and compiles its execution token as a literal.
pub(super) unsafe fn bracket_tick(vm: &mut Vm) {
    if !vm.compiler.is_compiling() {
        vm.set_error(ForthError::NotCompiling("[']"));
        return;
    }
    if let Some(xt) = parse_xt(vm, "[']") {
        let result = vm.compiler.compile_literal(&mut vm.dictionary, xt);
        vm.check(result);
    }
}

/// Parses the text of a string up to `"`, on behalf of `word`. Gives `None`, after setting the
/// error, if there is no closing quotation mark.
fn parse_string(vm: &mut Vm, word: &'static str) -> Option<String> {
    let text = vm.input.parse_until('"');
    if text.is_none() {
        vm.set_error(ForthError::UnterminatedString(word));
    }
    text
}

/// `S" ( -- c-addr u )`: Parses text up to `"`. While compiling, the text is compiled, and the
/// word pushes its address and length when the definition runs. While interpreting, the text is
/// copied to a transient buffer, which the next string after it doesn't overwrite.
pub(super) unsafe fn s_quote(vm: &mut Vm) {
    let Some(text) = parse_string(vm, "S\"") else {
        return;
    };
    if vm.compiler.is_compiling() {
        let result = vm
            .compiler
            .compile_string(&mut vm.dictionary, text.as_bytes());
        vm.check(result);
        return;
    }
    if text.len() > STRING_BUFFER_BYTES {
        vm.set_error(ForthError::StringTooLong(text.len()));
        return;
    }
    let address = vm.string_buffers.take();
    let result = vm.dictionary.write_bytes(address, text.as_bytes());
    if vm.check(result).is_some() {
        vm.push(address);
        vm.push(text.len() as Cell);
    }
}

/// `." ( -- )`: Parses text up to `"`. The definition prints the text when it runs. This word is
/// compile-only.
pub(super) unsafe fn dot_quote(vm: &mut Vm) {
    if !vm.compiler.is_compiling() {
        vm.set_error(ForthError::NotCompiling(".\""));
        return;
    }
    let Some(text) = parse_string(vm, ".\"") else {
        return;
    };
    let result = vm
        .compiler
        .compile_print_string(&mut vm.dictionary, text.as_bytes());
    vm.check(result);
}

/// `ABORT" ( i*x flag -- )`: Parses text up to `"`. The definition prints the text and does
/// `ABORT` when it runs with a true flag. This word is compile-only.
pub(super) unsafe fn abort_quote(vm: &mut Vm) {
    if !vm.compiler.is_compiling() {
        vm.set_error(ForthError::NotCompiling("ABORT\""));
        return;
    }
    let Some(text) = parse_string(vm, "ABORT\"") else {
        return;
    };
    let result = vm
        .compiler
        .compile_abort_quote(&mut vm.dictionary, text.as_bytes());
    vm.check(result);
}

/// Parses a name on behalf of `word`, and returns its first byte. Strings hold bytes, so a
/// character that takes more than one byte gives only its first one.
fn parse_char(vm: &mut Vm, word: &'static str) -> Option<u8> {
    let Some(name) = vm.input.parse_name() else {
        vm.set_error(ForthError::MissingName(word));
        return None;
    };
    let first = name.as_bytes().first().copied();
    if first.is_none() {
        vm.set_error(ForthError::MissingName(word));
    }
    first
}

/// `CHAR ( "name" -- char )`: Parses a name, and pushes the first byte of it.
pub(super) unsafe fn char_word(vm: &mut Vm) {
    if let Some(byte) = parse_char(vm, "CHAR") {
        vm.push(byte as Cell);
    }
}

/// `[CHAR] ( "name" -- )`: Parses a name while compiling, and compiles its first byte as a
/// literal.
pub(super) unsafe fn bracket_char(vm: &mut Vm) {
    if !vm.compiler.is_compiling() {
        vm.set_error(ForthError::NotCompiling("[CHAR]"));
        return;
    }
    if let Some(byte) = parse_char(vm, "[CHAR]") {
        let result = vm
            .compiler
            .compile_literal(&mut vm.dictionary, byte as Cell);
        vm.check(result);
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::io::{self, Write};
    use std::rc::Rc;

    use super::*;
    use crate::dictionary::pfa_of;
    use crate::{boot, outer_interpreter, VmConfig};

    const TEST_CELLS: usize = 64 * 1024;

    // A writer that shares what it receives with the test.
    #[derive(Clone, Default)]
    struct Capture(Rc<RefCell<Vec<u8>>>);

    impl Write for Capture {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0.borrow_mut().write(bytes)
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    // Boots a VM whose output is captured.
    unsafe fn capturing_vm() -> (Vm, Capture) {
        let output = Capture::default();
        let vm = boot(output.clone(), VmConfig::new(TEST_CELLS)).unwrap();
        (vm, output)
    }

    // Returns what a capture holds as text.
    fn output_text(output: &Capture) -> String {
        String::from_utf8(output.0.borrow().clone()).unwrap()
    }

    #[test]
    fn paren_comment_skips_to_the_closing_parenthesis() {
        unsafe {
            let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
            vm.input.load("skipped ) next");
            vm.data_stack.push(7);

            paren_comment(&mut vm);
            assert!(vm.error.is_none(), "got: {:?}", vm.error);
            assert_eq!(vm.data_stack, [7]);
            assert_eq!(vm.input.parse_name().as_deref(), Some("next"));
        }
    }

    #[test]
    fn paren_comment_reports_a_missing_parenthesis() {
        unsafe {
            let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
            vm.input.load("never closed");

            paren_comment(&mut vm);
            assert!(matches!(vm.error, Some(ForthError::UnterminatedComment)));
        }
    }

    #[test]
    fn line_comment_skips_the_rest_of_the_line() {
        unsafe {
            let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
            vm.input.load("skipped\nnext");
            vm.data_stack.push(7);

            line_comment(&mut vm);
            assert!(vm.error.is_none(), "got: {:?}", vm.error);
            assert_eq!(vm.data_stack, [7]);
            assert_eq!(vm.input.parse_name().as_deref(), Some("next"));
        }
    }

    #[test]
    fn s_quote_interpreted_pushes_the_address_and_length() {
        unsafe {
            let (mut vm, _output) = capturing_vm();
            outer_interpreter(&mut vm, "S\" hi\"").unwrap();
            let [addr, len] = vm.data_stack[..] else {
                panic!("got: {:?}", vm.data_stack)
            };
            assert_eq!(len, 2);
            assert_eq!(vm.dictionary.bytes(addr, 2).unwrap(), b"hi");
        }
    }

    #[test]
    fn s_quote_keeps_two_strings_in_a_row_valid() {
        unsafe {
            let (mut vm, _output) = capturing_vm();
            outer_interpreter(&mut vm, "S\" a\" S\" bc\"").unwrap();
            let [first, 1, second, 2] = vm.data_stack[..] else {
                panic!("got: {:?}", vm.data_stack)
            };
            assert_eq!(vm.dictionary.bytes(first, 1).unwrap(), b"a");
            assert_eq!(vm.dictionary.bytes(second, 2).unwrap(), b"bc");
        }
    }

    #[test]
    fn s_quote_reuses_the_first_buffer_for_a_third_string() {
        unsafe {
            let (mut vm, _output) = capturing_vm();
            outer_interpreter(&mut vm, "S\" a\" S\" b\" S\" c\"").unwrap();
            assert_eq!(vm.data_stack.len(), 6);
            assert_eq!(vm.data_stack[0], vm.data_stack[4]);
            assert_ne!(vm.data_stack[0], vm.data_stack[2]);
        }
    }

    #[test]
    fn s_quote_fits_a_text_of_one_buffer_and_no_longer() {
        unsafe {
            let (mut vm, _output) = capturing_vm();
            let fits = format!("S\" {}\"", "x".repeat(STRING_BUFFER_BYTES));
            outer_interpreter(&mut vm, &fits).unwrap();
            assert_eq!(vm.data_stack[1], STRING_BUFFER_BYTES);

            let too_long = format!("S\" {}\"", "x".repeat(STRING_BUFFER_BYTES + 1));
            let error = outer_interpreter(&mut vm, &too_long).unwrap_err();
            assert!(
                matches!(error, ForthError::StringTooLong(len) if len == STRING_BUFFER_BYTES + 1),
                "got: {:?}",
                error
            );
        }
    }

    #[test]
    fn s_quote_without_a_closing_quote_is_an_error() {
        unsafe {
            let (mut vm, _output) = capturing_vm();
            let error = outer_interpreter(&mut vm, "S\" open").unwrap_err();
            assert!(
                matches!(error, ForthError::UnterminatedString("S\"")),
                "got: {:?}",
                error
            );
            assert!(vm.data_stack.is_empty());
        }
    }

    #[test]
    fn s_quote_in_a_definition_compiles_the_text() {
        unsafe {
            let (mut vm, _output) = capturing_vm();
            outer_interpreter(&mut vm, ": T S\" hi\" ; T").unwrap();
            let [addr, 2] = vm.data_stack[..] else {
                panic!("got: {:?}", vm.data_stack)
            };
            assert_eq!(vm.dictionary.bytes(addr, 2).unwrap(), b"hi");
        }
    }

    #[test]
    fn dot_quote_prints_the_text_when_the_definition_runs() {
        unsafe {
            let (mut vm, output) = capturing_vm();
            outer_interpreter(&mut vm, ": G .\" hi\" ; G G").unwrap();
            assert_eq!(output_text(&output), "hihi");
            assert!(vm.data_stack.is_empty());
        }
    }

    #[test]
    fn dot_quote_outside_a_definition_is_an_error() {
        unsafe {
            let (mut vm, output) = capturing_vm();
            let error = outer_interpreter(&mut vm, ".\" hi\"").unwrap_err();
            assert!(
                matches!(error, ForthError::NotCompiling(".\"")),
                "got: {:?}",
                error
            );
            assert_eq!(output_text(&output), "");
        }
    }

    #[test]
    fn abort_quote_with_a_false_flag_does_nothing() {
        unsafe {
            let (mut vm, output) = capturing_vm();
            outer_interpreter(&mut vm, ": A ABORT\" bad\" ; 0 A").unwrap();
            assert!(vm.data_stack.is_empty());
            assert_eq!(output_text(&output), "");
        }
    }

    #[test]
    fn abort_quote_with_a_true_flag_prints_the_text_and_aborts() {
        unsafe {
            let (mut vm, output) = capturing_vm();
            let error = outer_interpreter(&mut vm, ": A ABORT\" bad\" ; 7 1 A").unwrap_err();
            assert!(matches!(error, ForthError::Abort), "got: {:?}", error);
            assert!(vm.data_stack.is_empty());
            assert_eq!(output_text(&output), "bad");
        }
    }

    #[test]
    fn abort_quote_outside_a_definition_is_an_error() {
        unsafe {
            let (mut vm, output) = capturing_vm();
            let error = outer_interpreter(&mut vm, "ABORT\" bad\"").unwrap_err();
            assert!(
                matches!(error, ForthError::NotCompiling("ABORT\"")),
                "got: {:?}",
                error
            );
            assert_eq!(output_text(&output), "");
        }
    }

    #[test]
    fn char_pushes_the_first_byte_of_the_next_name() {
        unsafe {
            let (mut vm, _output) = capturing_vm();
            outer_interpreter(&mut vm, "CHAR A CHAR Bcd").unwrap();
            assert_eq!(vm.data_stack, [65, 66]);
        }
    }

    #[test]
    fn char_without_a_name_is_an_error() {
        unsafe {
            let (mut vm, _output) = capturing_vm();
            let error = outer_interpreter(&mut vm, "CHAR").unwrap_err();
            assert!(
                matches!(error, ForthError::MissingName("CHAR")),
                "got: {:?}",
                error
            );
        }
    }

    #[test]
    fn bracket_char_compiles_the_first_byte_as_a_literal() {
        unsafe {
            let (mut vm, _output) = capturing_vm();
            outer_interpreter(&mut vm, ": Z [CHAR] Z ; Z").unwrap();
            assert_eq!(vm.data_stack, [90]);
        }
    }

    #[test]
    fn bracket_char_outside_a_definition_is_an_error() {
        unsafe {
            let (mut vm, _output) = capturing_vm();
            let error = outer_interpreter(&mut vm, "[CHAR] Z").unwrap_err();
            assert!(
                matches!(error, ForthError::NotCompiling("[CHAR]")),
                "got: {:?}",
                error
            );
        }
    }

    #[test]
    fn tick_pushes_the_execution_token() {
        unsafe {
            let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
            vm.input.load("DUP next");
            vm.data_stack.push(7);

            tick(&mut vm);
            assert!(vm.error.is_none(), "got: {:?}", vm.error);
            let xt = cfa_of(vm.dictionary.find_word("DUP").unwrap()) as Cell;
            assert_eq!(vm.data_stack, [7, xt]);
            assert_eq!(vm.input.parse_name().as_deref(), Some("next"));
        }
    }

    #[test]
    fn tick_leaves_the_stack_alone_on_an_error() {
        unsafe {
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
    }

    #[test]
    fn bracket_tick_compiles_the_xt_as_a_literal() {
        unsafe {
            let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
            outer_interpreter(&mut vm, ": Y 5 ;").unwrap();
            let literal = cfa_of(vm.dictionary.find_word("Y").unwrap());
            let lit_cfa = *pfa_of(literal);

            outer_interpreter(&mut vm, ": X").unwrap();
            let xt = cfa_of(vm.dictionary.find_word("DUP").unwrap()) as Cell;
            let body = pfa_of(cfa_of(vm.dictionary.last_word()));
            vm.input.load("DUP");
            bracket_tick(&mut vm);

            assert!(vm.error.is_none(), "got: {:?}", vm.error);
            assert!(vm.data_stack.is_empty());
            assert_eq!(*body, lit_cfa);
            assert_eq!(*body.add(1), xt);
        }
    }

    #[test]
    fn bracket_tick_only_works_while_compiling() {
        unsafe {
            let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
            vm.input.load("DUP next");

            bracket_tick(&mut vm);
            assert!(matches!(vm.error, Some(ForthError::NotCompiling("[']"))));
            assert_eq!(vm.input.parse_name().as_deref(), Some("DUP"));
        }
    }
}
