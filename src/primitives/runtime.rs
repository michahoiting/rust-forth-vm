//! Words that drive threaded code. Their addresses are stored in Code Fields
//! or compiled into threads.

use crate::cell::Cell;
use crate::error::ForthError;
use crate::vm::{enter_word, Registers};

// A word made by CREATE has two Cells behind its CFA: W + 1 is the action cell
// reserved for DOES>, and its data field starts at W + 2.

// DOCOL: Executed when entering a nested `:` (colon) word.
pub(super) unsafe fn docol(regs: &mut Registers) {
    // Push the current IP to the Return Stack (return address)
    regs.return_stack.push(regs.ip);
    // Move IP to the inside of the definition (physically 1 Cell after the CFA, so W + 1)
    regs.ip = regs.w.add(1);
}

// DOVAR: Default execution semantics for a word created via CREATE (or VARIABLE).
// It simply pushes the memory address of its own Data Field (W + 2) onto the stack.
pub(super) unsafe fn dovar(regs: &mut Registers) {
    let data_address = regs.w.add(2) as Cell;
    regs.data_stack.push(data_address);
}

// DODOES: Runtime execution semantics for a child word modified via DOES>.
pub(super) unsafe fn dodoes(regs: &mut Registers) {
    // 1. Push the address of the actual data onto the stack (W + 2, because W + 1 holds the action-pointer)
    let data_address = regs.w.add(2) as Cell;
    regs.data_stack.push(data_address);

    // 2. Save the current IP onto the Return Stack
    regs.return_stack.push(regs.ip);

    // 3. Jump with the IP to the Forth action code stored behind DOES> (saved in W + 1)
    regs.ip = *regs.w.add(1) as *const Cell;
}

// EXIT: Cleans up and exits a nested `:` word (compiled by `;`)
pub(super) unsafe fn exit(regs: &mut Registers) {
    match regs.return_stack.pop() {
        Some(old_ip) => regs.ip = old_ip,
        None => regs.set_error(ForthError::ReturnStackUnderflow),
    }
}

// EXECUTE ( i*x xt -- j*x ): Pops an execution token and runs that word.
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

// LIT: Fetches the inline literal value from the thread, pushes it, and skips the data cell.
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
            // Code 0 would crash if it ran, so this also shows that nothing ran.
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
