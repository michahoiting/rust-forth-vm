//! Words that work on the data stack and on memory.

use std::io::Write;

use crate::cell::Cell;
use crate::vm::{Registers, OUTPUT_ERROR};

// FETCH ( @ ): Pops an address from the stack and replaces it with the contents of that Cell.
pub(super) unsafe fn fetch(regs: &mut Registers) {
    if let Some(addr) = regs.pop() {
        let ptr = addr as *const Cell;
        regs.data_stack.push(*ptr);
    }
}

// PLUS ( + ): Pops two Cells from the stack, adds them, and pushes the result.
pub(super) unsafe fn plus(regs: &mut Registers) {
    if regs.data_stack.len() < 2 {
        regs.set_error("Data stack underflow.");
        return;
    }
    if let (Some(b), Some(a)) = (regs.data_stack.pop(), regs.data_stack.pop()) {
        regs.data_stack.push(a.wrapping_add(b));
    }
}

// PRINT ( . ): Pops a Cell from the stack and writes it to the output as a signed number.
pub(super) unsafe fn dot_print(regs: &mut Registers) {
    if let Some(value) = regs.pop() {
        let result = write!(regs.output, "{} ", value as isize);
        regs.check(result.map_err(|_| OUTPUT_ERROR));
    }
}
