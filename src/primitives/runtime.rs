// Copyright (c) 2026 M.W.R. Hoiting

//! Words that drive threaded code. Their addresses are stored in Code Fields
//! or compiled into threads.
//!
//! Every word's Parameter Field directly follows its Code Field, so these words use
//! [`pfa_of`] on W. `DODOES` is in [`vm`](crate::vm), because the inner interpreter calls it.

use crate::cell::Cell;
use crate::dictionary::pfa_of;
use crate::error::ForthError;
use crate::vm::{enter_word, Registers};

/// `DOCOL`: Executed when entering a nested `:` (colon) word.
pub(super) unsafe fn docol(regs: &mut Registers) {
    // Push the current IP to the Return Stack (return address)
    regs.return_stack.push(regs.ip);
    // Move IP to the inside of the definition, which is the Parameter Field
    regs.ip = pfa_of(regs.w);
}

/// `DOVAR`: Default execution semantics for a word created via CREATE (or VARIABLE).
/// It simply pushes the memory address of its own Parameter Field onto the stack.
pub(super) unsafe fn dovar(regs: &mut Registers) {
    regs.data_stack.push(pfa_of(regs.w) as Cell);
}

/// `DOCON`: Execution semantics of a word made by CONSTANT.
/// It pushes the value held in the word's Parameter Field.
pub(super) unsafe fn docon(regs: &mut Registers) {
    regs.data_stack.push(*pfa_of(regs.w));
}

/// `EXIT ( -- )`: Cleans up and exits a nested `:` word (compiled by `;`).
pub(super) unsafe fn exit(regs: &mut Registers) {
    match regs.return_stack.pop() {
        Some(old_ip) => regs.ip = old_ip,
        None => regs.set_error(ForthError::ReturnStackUnderflow),
    }
}

/// `QUIT ( -- )`: Stops the threaded code and the current input, and returns to the text
/// interpreter. The interpreter does the rest: see
/// [`outer_interpreter`](crate::outer_interpreter).
pub(super) unsafe fn quit(regs: &mut Registers) {
    regs.set_error(ForthError::Quit);
}

/// `EXECUTE ( i*x xt -- j*x )`: Pops an execution token and runs that word.
pub(super) unsafe fn execute(regs: &mut Registers) {
    if let Some(xt) = regs.pop() {
        if !regs.dictionary.is_execution_token(xt) {
            regs.set_error(ForthError::InvalidExecutionToken(xt));
            return;
        }
        regs.w = xt as *const Cell;
        enter_word(regs);
    }
}

/// `LIT ( -- x )`: Fetches the inline literal value from the thread, pushes it, and skips the
/// data cell.
pub(super) unsafe fn lit(regs: &mut Registers) {
    let literal_value = *regs.ip;
    regs.data_stack.push(literal_value);
    regs.ip = regs.ip.add(1); // Skip the inline data cell
}

#[cfg(test)]
mod tests {
    use std::io;

    use super::*;
    use crate::boot;
    use crate::dictionary::cfa_of;

    const TEST_CELLS: usize = 1024;

    unsafe fn xt_of(regs: &Registers, name: &str) -> Cell {
        cfa_of(regs.dictionary.find_word(name).unwrap()) as Cell
    }

    #[test]
    fn docon_pushes_the_cell_behind_the_code_field() {
        unsafe {
            let mut regs = boot(io::sink(), TEST_CELLS).unwrap();
            let word: [Cell; 3] = [0, 42, 99];
            regs.w = word.as_ptr();
            regs.data_stack.push(1);

            docon(&mut regs);
            assert!(regs.error.is_none(), "got: {:?}", regs.error);
            assert_eq!(regs.data_stack, [1, 42]);
            assert_eq!(word, [0, 42, 99]);
        }
    }

    #[test]
    fn quit_stops_the_vm_and_leaves_the_data_stack_alone() {
        unsafe {
            let mut regs = boot(io::sink(), TEST_CELLS).unwrap();
            regs.data_stack.push(7);

            quit(&mut regs);
            assert!(matches!(regs.error, Some(ForthError::Quit)));
            assert_eq!(regs.data_stack, [7]);
        }
    }

    #[test]
    fn execute_runs_a_word_and_consumes_the_xt() {
        unsafe {
            let mut regs = boot(io::sink(), TEST_CELLS).unwrap();
            let xt = xt_of(&regs, "DUP");
            regs.data_stack.extend_from_slice(&[7, xt]);

            execute(&mut regs);
            assert!(regs.error.is_none(), "got: {:?}", regs.error);
            assert_eq!(regs.data_stack, [7, 7]);
        }
    }

    #[test]
    fn execute_rejects_a_pending_word() {
        unsafe {
            let mut regs = boot(io::sink(), TEST_CELLS).unwrap();
            // Code 0 would give another error if it ran, so this also shows that nothing ran.
            let xt = regs
                .compiler
                .begin_definition(&mut regs.dictionary, "X", 0, 0)
                .unwrap();
            regs.data_stack.push(xt);

            execute(&mut regs);
            assert!(matches!(
                regs.error,
                Some(ForthError::InvalidExecutionToken(token)) if token == xt
            ));
        }
    }

    #[test]
    fn execute_rejects_a_number_without_running_anything() {
        unsafe {
            let mut regs = boot(io::sink(), TEST_CELLS).unwrap();
            regs.data_stack.extend_from_slice(&[1, 5]);

            execute(&mut regs);
            assert!(matches!(
                regs.error,
                Some(ForthError::InvalidExecutionToken(5))
            ));
            assert_eq!(regs.data_stack, [1]);
        }
    }

    #[test]
    fn execute_needs_a_cell() {
        unsafe {
            let mut regs = boot(io::sink(), TEST_CELLS).unwrap();
            execute(&mut regs);
            assert!(matches!(regs.error, Some(ForthError::DataStackUnderflow)));
        }
    }
}
