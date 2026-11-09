// Copyright (c) 2026 M.W.R. Hoiting

//! Words that drive threaded code. Their addresses are stored in Code Fields
//! or compiled into threads.
//!
//! Every word's Parameter Field directly follows its Code Field, so these words use
//! [`pfa_of`] on W. `DODOES` is in [`vm`](crate::vm), because the inner interpreter calls it.

use crate::cell::Cell;
use crate::dictionary::pfa_of;
use crate::error::ForthError;
use crate::vm::{enter_word, Vm};

/// `DOCOL`: Executed when entering a nested `:` (colon) word.
pub(super) unsafe fn docol(vm: &mut Vm) {
    // Push the current IP to the Return Stack (return address)
    vm.push_return(vm.ip);
    // Move IP to the inside of the definition, which is the Parameter Field
    vm.ip = pfa_of(vm.w);
}

/// `DOVAR`: Default execution semantics for a word created via CREATE (or VARIABLE).
/// It simply pushes the memory address of its own Parameter Field onto the stack.
pub(super) unsafe fn dovar(vm: &mut Vm) {
    vm.push(pfa_of(vm.w) as Cell);
}

/// `DOCON`: Execution semantics of a word made by CONSTANT.
/// It pushes the value held in the word's Parameter Field.
pub(super) unsafe fn docon(vm: &mut Vm) {
    vm.push(*pfa_of(vm.w));
}

/// `EXIT ( -- )`: Cleans up and exits a nested `:` word (compiled by `;`).
pub(super) unsafe fn exit(vm: &mut Vm) {
    match vm.return_stack.pop() {
        Some(old_ip) => vm.ip = old_ip,
        None => vm.set_error(ForthError::ReturnStackUnderflow),
    }
}

/// `QUIT ( -- )`: Stops the threaded code and the current input, and returns to the text
/// interpreter. The interpreter does the rest: see
/// [`outer_interpreter`](crate::outer_interpreter).
pub(super) unsafe fn quit(vm: &mut Vm) {
    vm.set_error(ForthError::Quit);
}

/// `ABORT ( i*x -- )`: Does the same as `QUIT`, and also clears the data stack. The
/// interpreter does that part too, so that a host can tell the two apart.
pub(super) unsafe fn abort(vm: &mut Vm) {
    vm.set_error(ForthError::Abort);
}

/// `BYE ( -- )`: Stops the text interpreter and leaves the program with exit code 0.
pub(super) unsafe fn bye(vm: &mut Vm) {
    vm.set_error(ForthError::Bye(0));
}

/// `(BYE) ( n -- )`: Stops the text interpreter and leaves the program with exit code `n`.
pub(super) unsafe fn bye_with_code(vm: &mut Vm) {
    if let Some(code) = vm.pop() {
        vm.set_error(ForthError::Bye(code));
    }
}

/// `EXECUTE ( i*x xt -- j*x )`: Pops an execution token and runs that word.
pub(super) unsafe fn execute(vm: &mut Vm) {
    if let Some(xt) = vm.pop() {
        if !vm.dictionary.is_execution_token(xt) {
            vm.set_error(ForthError::InvalidExecutionToken(xt));
            return;
        }
        vm.w = xt as *const Cell;
        enter_word(vm);
    }
}

/// `LIT ( -- x )`: Fetches the inline literal value from the thread, pushes it, and skips the
/// data cell.
pub(super) unsafe fn lit(vm: &mut Vm) {
    let literal_value = *vm.ip;
    vm.push(literal_value);
    vm.ip = vm.ip.add(1); // Skip the inline data cell
}

/// `BRANCH ( -- )`: Jumps to the address in the inline operand of the thread. `ELSE` compiles it.
pub(super) unsafe fn branch(vm: &mut Vm) {
    vm.ip = *vm.ip as *const Cell;
}

/// `0BRANCH ( flag -- )`: Pops a flag, and jumps like `BRANCH` if it is zero. Otherwise it skips
/// the inline operand. `IF` compiles it.
pub(super) unsafe fn zero_branch(vm: &mut Vm) {
    if let Some(flag) = vm.pop() {
        if flag == 0 {
            branch(vm);
        } else {
            vm.ip = vm.ip.add(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::ptr;

    use super::*;
    use crate::dictionary::cfa_of;
    use crate::{boot, VmConfig};

    const TEST_CELLS: usize = 64 * 1024;

    unsafe fn xt_of(vm: &Vm, name: &str) -> Cell {
        cfa_of(vm.dictionary.find_word(name).unwrap()) as Cell
    }

    #[test]
    fn branch_jumps_to_the_operand() {
        unsafe {
            let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
            let mut thread: [Cell; 4] = [0; 4];
            thread[0] = thread.as_ptr().add(3) as Cell;

            vm.ip = thread.as_ptr();
            branch(&mut vm);
            assert_eq!(vm.ip, thread.as_ptr().add(3));
            assert!(vm.error.is_none(), "got: {:?}", vm.error);
        }
    }

    #[test]
    fn zero_branch_jumps_only_on_zero() {
        unsafe {
            let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
            let mut thread: [Cell; 4] = [0; 4];
            thread[0] = thread.as_ptr().add(3) as Cell;
            let start = thread.as_ptr();

            vm.ip = start;
            vm.data_stack.extend_from_slice(&[9, 0]);
            zero_branch(&mut vm);
            assert_eq!(vm.ip, start.add(3));
            assert_eq!(vm.data_stack, [9]);

            vm.ip = start;
            vm.data_stack.push(5);
            zero_branch(&mut vm);
            assert_eq!(vm.ip, start.add(1));
            assert_eq!(vm.data_stack, [9]);
            assert!(vm.error.is_none(), "got: {:?}", vm.error);
        }
    }

    #[test]
    fn zero_branch_needs_a_cell_and_stays_put() {
        unsafe {
            let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
            let thread: [Cell; 1] = [0];
            vm.ip = thread.as_ptr();

            zero_branch(&mut vm);
            assert!(matches!(vm.error, Some(ForthError::DataStackUnderflow)));
            assert_eq!(vm.ip, thread.as_ptr());
        }
    }

    #[test]
    fn docon_pushes_the_cell_behind_the_code_field() {
        unsafe {
            let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
            let word: [Cell; 3] = [0, 42, 99];
            vm.w = word.as_ptr();
            vm.data_stack.push(1);

            docon(&mut vm);
            assert!(vm.error.is_none(), "got: {:?}", vm.error);
            assert_eq!(vm.data_stack, [1, 42]);
            assert_eq!(word, [0, 42, 99]);
        }
    }

    #[test]
    fn quit_stops_the_vm_and_leaves_the_data_stack_alone() {
        unsafe {
            let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
            vm.data_stack.push(7);

            quit(&mut vm);
            assert!(matches!(vm.error, Some(ForthError::Quit)));
            assert_eq!(vm.data_stack, [7]);
        }
    }

    #[test]
    fn abort_stops_the_vm_and_leaves_the_stack_to_the_interpreter() {
        unsafe {
            let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
            vm.data_stack.push(7);

            abort(&mut vm);
            assert!(matches!(vm.error, Some(ForthError::Abort)));
            assert_eq!(vm.data_stack, [7]);
        }
    }

    #[test]
    fn bye_stops_the_vm_with_code_zero() {
        unsafe {
            let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
            bye(&mut vm);
            assert!(matches!(vm.error, Some(ForthError::Bye(0))));
        }
    }

    #[test]
    fn bye_with_code_pops_a_cell() {
        unsafe {
            let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
            vm.data_stack.push(3);
            bye_with_code(&mut vm);
            assert!(matches!(vm.error, Some(ForthError::Bye(3))));
            assert!(vm.data_stack.is_empty());
        }
    }

    #[test]
    fn bye_with_code_reports_stack_underflow() {
        unsafe {
            let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
            bye_with_code(&mut vm);
            assert!(matches!(vm.error, Some(ForthError::DataStackUnderflow)));
        }
    }

    #[test]
    fn execute_runs_a_word_and_consumes_the_xt() {
        unsafe {
            let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
            let xt = xt_of(&vm, "DUP");
            vm.data_stack.extend_from_slice(&[7, xt]);

            execute(&mut vm);
            assert!(vm.error.is_none(), "got: {:?}", vm.error);
            assert_eq!(vm.data_stack, [7, 7]);
        }
    }

    #[test]
    fn execute_rejects_a_pending_word() {
        unsafe {
            let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
            // Code 0 would give another error if it ran, so this also shows that nothing ran.
            let xt = vm
                .compiler
                .begin_definition(&mut vm.dictionary, "X", 0, 0)
                .unwrap();
            vm.data_stack.push(xt);

            execute(&mut vm);
            assert!(matches!(
                vm.error,
                Some(ForthError::InvalidExecutionToken(token)) if token == xt
            ));
        }
    }

    #[test]
    fn execute_rejects_lit_without_running_it() {
        unsafe {
            let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
            // A compiled literal starts with the CFA of LIT.
            let start = vm.dictionary.here() as *const Cell;
            vm.compiler.compile_literal(&mut vm.dictionary, 5).unwrap();
            let lit = *start;
            vm.data_stack.extend_from_slice(&[1, lit]);
            vm.ip = ptr::null();

            execute(&mut vm);
            assert!(matches!(
                vm.error,
                Some(ForthError::InvalidExecutionToken(token)) if token == lit
            ));
            assert_eq!(vm.data_stack, [1]);
            assert!(vm.ip.is_null());
        }
    }

    #[test]
    fn execute_rejects_a_number_without_running_anything() {
        unsafe {
            let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
            vm.data_stack.extend_from_slice(&[1, 5]);

            execute(&mut vm);
            assert!(matches!(
                vm.error,
                Some(ForthError::InvalidExecutionToken(5))
            ));
            assert_eq!(vm.data_stack, [1]);
        }
    }

    #[test]
    fn execute_needs_a_cell() {
        unsafe {
            let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
            execute(&mut vm);
            assert!(matches!(vm.error, Some(ForthError::DataStackUnderflow)));
        }
    }
}
