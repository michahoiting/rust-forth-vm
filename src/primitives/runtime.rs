// Copyright (c) 2026 M.W.R. Hoiting

//! Words that drive threaded code. The actions of colon words, variables, constants and words
//! changed by `DOES>` aren't primitives: the inner interpreter runs them from their
//! [`Code`](crate::dictionary::Code), in [`vm`](crate::vm).

use std::mem;

use crate::cell::Cell;
use crate::error::ForthError;
use crate::vm::{execute_xt, Vm};

/// `EXIT ( -- )`: Cleans up and exits a nested `:` word (compiled by `;`).
pub(super) fn exit(vm: &mut Vm) {
    match vm.return_stack.pop() {
        Some(old_ip) => vm.ip = old_ip,
        None => vm.set_error(ForthError::ReturnStackUnderflow),
    }
}

/// `QUIT ( -- )`: Stops the threaded code and the current input, and returns to the text
/// interpreter. The interpreter does the rest: see
/// [`outer_interpreter`](crate::outer_interpreter).
pub(super) fn quit(vm: &mut Vm) {
    vm.set_error(ForthError::Quit);
}

/// `ABORT ( i*x -- )`: Does the same as `QUIT`, and also clears the data stack. The
/// interpreter does that part too, so that a host can tell the two apart.
pub(super) fn abort(vm: &mut Vm) {
    vm.set_error(ForthError::Abort);
}

/// `BYE ( -- )`: Stops the text interpreter and leaves the program with exit code 0.
pub(super) fn bye(vm: &mut Vm) {
    vm.set_error(ForthError::Bye(0));
}

/// `(BYE) ( n -- )`: Stops the text interpreter and leaves the program with exit code `n`.
pub(super) fn bye_with_code(vm: &mut Vm) {
    if let Some(code) = vm.pop() {
        vm.set_error(ForthError::Bye(code));
    }
}

/// `EXECUTE ( i*x xt -- j*x )`: Pops an execution token and runs that word.
pub(super) fn execute(vm: &mut Vm) {
    if let Some(xt) = vm.pop() {
        if !vm.dictionary.is_execution_token(xt) {
            vm.set_error(ForthError::InvalidExecutionToken(xt));
            return;
        }
        execute_xt(vm, xt);
    }
}

/// `LIT ( -- x )`: Fetches the inline literal value from the thread, pushes it, and skips the
/// data cell.
pub(super) fn lit(vm: &mut Vm) {
    let result = vm.dictionary.fetch(vm.ip);
    if let Some(literal_value) = vm.check(result) {
        vm.push(literal_value);
        vm.ip += mem::size_of::<Cell>(); // Skip the inline data cell
    }
}

#[cfg(test)]
mod tests {
    use std::io;

    use super::*;
    use crate::compiler::LIT_XT;
    use crate::dictionary::Code;
    use crate::vm::NO_IP;
    use crate::{boot, outer_interpreter, VmConfig};

    const TEST_CELLS: usize = 64 * 1024;

    fn xt_of(vm: &Vm, name: &str) -> Cell {
        vm.dictionary.find_word(name).unwrap()
    }

    #[test]
    fn quit_stops_the_vm_and_leaves_the_data_stack_alone() {
        let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
        vm.data_stack.push(7);

        quit(&mut vm);
        assert!(matches!(vm.error, Some(ForthError::Quit)));
        assert_eq!(vm.data_stack, [7]);
    }

    #[test]
    fn abort_stops_the_vm_and_leaves_the_stack_to_the_interpreter() {
        let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
        vm.data_stack.push(7);

        abort(&mut vm);
        assert!(matches!(vm.error, Some(ForthError::Abort)));
        assert_eq!(vm.data_stack, [7]);
    }

    #[test]
    fn bye_stops_the_vm_with_code_zero() {
        let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
        bye(&mut vm);
        assert!(matches!(vm.error, Some(ForthError::Bye(0))));
    }

    #[test]
    fn bye_with_code_pops_a_cell() {
        let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
        vm.data_stack.push(3);
        bye_with_code(&mut vm);
        assert!(matches!(vm.error, Some(ForthError::Bye(3))));
        assert!(vm.data_stack.is_empty());
    }

    #[test]
    fn bye_with_code_reports_stack_underflow() {
        let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
        bye_with_code(&mut vm);
        assert!(matches!(vm.error, Some(ForthError::DataStackUnderflow)));
    }

    #[test]
    fn execute_runs_a_word_and_consumes_the_xt() {
        let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
        let xt = xt_of(&vm, "DUP");
        vm.data_stack.extend_from_slice(&[7, xt]);

        execute(&mut vm);
        assert!(vm.error.is_none(), "got: {:?}", vm.error);
        assert_eq!(vm.data_stack, [7, 7]);
    }

    #[test]
    fn execute_rejects_a_pending_word() {
        let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
        let xt = vm
            .compiler
            .begin_definition(&mut vm.dictionary, "X", Code::Colon, 0)
            .unwrap();
        vm.data_stack.push(xt);

        execute(&mut vm);
        assert!(matches!(
            vm.error,
            Some(ForthError::InvalidExecutionToken(token)) if token == xt
        ));
    }

    #[test]
    fn execute_rejects_lit_without_running_it() {
        let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
        vm.data_stack.extend_from_slice(&[1, LIT_XT]);

        execute(&mut vm);
        assert!(matches!(
            vm.error,
            Some(ForthError::InvalidExecutionToken(LIT_XT))
        ));
        assert_eq!(vm.data_stack, [1]);
        assert_eq!(vm.ip, NO_IP);
    }

    #[test]
    fn execute_runs_a_colon_word_from_the_calling_thread() {
        let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
        outer_interpreter(&mut vm, ": X ;").unwrap();
        let xt = xt_of(&vm, "X");
        vm.data_stack.push(xt);
        vm.ip = 40;

        execute(&mut vm);
        assert!(vm.error.is_none(), "got: {:?}", vm.error);
        assert_eq!(vm.return_stack, [40]);
        assert_eq!(vm.ip, vm.dictionary.entry(xt).unwrap().body);
    }

    #[test]
    fn execute_rejects_a_number_without_running_anything() {
        let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
        let past = vm.dictionary.last_xt().unwrap() + 1;
        vm.data_stack.extend_from_slice(&[1, past]);

        execute(&mut vm);
        assert!(matches!(
            vm.error,
            Some(ForthError::InvalidExecutionToken(xt)) if xt == past
        ));
        assert_eq!(vm.data_stack, [1]);
    }

    #[test]
    fn execute_needs_a_cell() {
        let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
        execute(&mut vm);
        assert!(matches!(vm.error, Some(ForthError::DataStackUnderflow)));
    }
}
