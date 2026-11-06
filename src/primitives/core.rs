// Copyright (c) 2026 M.W.R. Hoiting

//! Words that work on the data stack and on memory.

use std::io::Write;

use crate::cell::Cell;
use crate::dictionary::pfa_of;
use crate::error::ForthError;
use crate::vm::Registers;

/// `@ ( addr -- x )`: Pops a dictionary address from the stack and replaces it with the
/// contents of that Cell.
pub(super) unsafe fn fetch(regs: &mut Registers) {
    if let Some(addr) = regs.pop() {
        let result = regs.dictionary.fetch(addr);
        if let Some(value) = regs.check(result) {
            regs.data_stack.push(value);
        }
    }
}

/// `! ( x addr -- )`: Pops a dictionary address and a Cell from the stack, and stores the
/// Cell there.
pub(super) unsafe fn store(regs: &mut Registers) {
    if !regs.require(2) {
        return;
    }
    if let (Some(addr), Some(value)) = (regs.data_stack.pop(), regs.data_stack.pop()) {
        let result = regs.dictionary.store(addr, value);
        regs.check(result);
    }
}

/// `>BODY ( xt -- pfa )`: Replaces an execution token with the address of that word's Parameter
/// Field.
pub(super) unsafe fn to_body(regs: &mut Registers) {
    if let Some(xt) = regs.pop() {
        if !regs.dictionary.is_execution_token(xt) {
            regs.set_error(ForthError::InvalidExecutionToken(xt));
            return;
        }
        regs.data_stack.push(pfa_of(xt as *const Cell) as Cell);
    }
}

/// `+ ( a b -- n )`: Pops two Cells from the stack, adds them, and pushes the result. Addition
/// wraps on overflow.
pub(super) unsafe fn plus(regs: &mut Registers) {
    if !regs.require(2) {
        return;
    }
    if let (Some(b), Some(a)) = (regs.data_stack.pop(), regs.data_stack.pop()) {
        regs.data_stack.push(a.wrapping_add(b));
    }
}

/// `. ( x -- )`: Pops a Cell from the stack and writes it to the output as a signed number.
pub(super) unsafe fn dot_print(regs: &mut Registers) {
    if let Some(value) = regs.pop() {
        let result = write!(regs.output, "{} ", value as isize);
        regs.check(result.map_err(ForthError::Output));
    }
}

/// `DUP ( x -- x x )`: Copies the top Cell.
pub(super) unsafe fn dup(regs: &mut Registers) {
    if regs.require(1) {
        let top = regs.data_stack[regs.data_stack.len() - 1];
        regs.data_stack.push(top);
    }
}

/// `?DUP ( x -- 0 | x x )`: Copies the top Cell if it isn't zero.
pub(super) unsafe fn question_dup(regs: &mut Registers) {
    if regs.require(1) {
        let top = regs.data_stack[regs.data_stack.len() - 1];
        if top != 0 {
            regs.data_stack.push(top);
        }
    }
}

/// `DROP ( x -- )`: Removes the top Cell.
pub(super) unsafe fn drop(regs: &mut Registers) {
    regs.pop();
}

/// `SWAP ( a b -- b a )`: Exchanges the top two Cells.
pub(super) unsafe fn swap(regs: &mut Registers) {
    if regs.require(2) {
        let top = regs.data_stack.len() - 1;
        regs.data_stack.swap(top, top - 1);
    }
}

/// `OVER ( a b -- a b a )`: Copies the second Cell to the top.
pub(super) unsafe fn over(regs: &mut Registers) {
    if regs.require(2) {
        let second = regs.data_stack[regs.data_stack.len() - 2];
        regs.data_stack.push(second);
    }
}

/// `ROT ( a b c -- b c a )`: Moves the third Cell to the top.
pub(super) unsafe fn rot(regs: &mut Registers) {
    if regs.require(3) {
        let third = regs.data_stack.len() - 3;
        regs.data_stack[third..].rotate_left(1);
    }
}

/// `PICK ( ... n -- ... x )`: Replaces `n` with a copy of the `n`th Cell below it, counting
/// from 0.
pub(super) unsafe fn pick(regs: &mut Registers) {
    if let Some(index) = regs.indexed_cell() {
        let top = regs.data_stack.len() - 1;
        regs.data_stack[top] = regs.data_stack[index];
    }
}

/// `ROLL ( ... n -- ... x )`: Replaces `n` by moving the `n`th Cell below it to the top.
pub(super) unsafe fn roll(regs: &mut Registers) {
    if let Some(index) = regs.indexed_cell() {
        regs.data_stack.pop();
        let cell = regs.data_stack.remove(index);
        regs.data_stack.push(cell);
    }
}

/// `DEPTH ( -- n )`: Pushes the number of Cells that were on the stack.
pub(super) unsafe fn depth(regs: &mut Registers) {
    regs.data_stack.push(regs.data_stack.len() as Cell);
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::mem;

    use super::*;
    use crate::boot;
    use crate::dictionary::cfa_of;
    use crate::vm::PrimitiveFn;

    const TEST_CELLS: usize = 1024;

    // Runs a word on a stack holding `before`.
    unsafe fn run_on(word: PrimitiveFn, before: &[Cell]) -> Registers {
        let mut regs = boot(io::sink(), TEST_CELLS).unwrap();
        regs.data_stack.extend_from_slice(before);
        word(&mut regs);
        regs
    }

    // Runs a word that must succeed, and returns the resulting stack.
    unsafe fn stack_after(word: PrimitiveFn, before: &[Cell]) -> Vec<Cell> {
        let regs = run_on(word, before);
        assert!(regs.error.is_none(), "got: {:?}", regs.error);
        regs.data_stack
    }

    // Asserts that a word reports an underflow and leaves the stack unchanged.
    unsafe fn assert_underflow(word: PrimitiveFn, before: &[Cell]) {
        let regs = run_on(word, before);
        assert!(
            matches!(regs.error, Some(ForthError::DataStackUnderflow)),
            "got: {:?}",
            regs.error
        );
        assert_eq!(regs.data_stack, before);
    }

    #[test]
    fn dup_copies_the_top_cell() {
        unsafe {
            assert_eq!(stack_after(dup, &[1, 2]), [1, 2, 2]);
        }
    }

    #[test]
    fn question_dup_copies_a_non_zero_cell() {
        unsafe {
            assert_eq!(stack_after(question_dup, &[1, 2]), [1, 2, 2]);
        }
    }

    #[test]
    fn question_dup_leaves_zero_alone() {
        unsafe {
            assert_eq!(stack_after(question_dup, &[1, 0]), [1, 0]);
        }
    }

    #[test]
    fn drop_removes_the_top_cell() {
        unsafe {
            assert_eq!(stack_after(drop, &[1, 2]), [1]);
        }
    }

    #[test]
    fn swap_exchanges_the_top_two_cells() {
        unsafe {
            assert_eq!(stack_after(swap, &[1, 2, 3]), [1, 3, 2]);
        }
    }

    #[test]
    fn over_copies_the_second_cell() {
        unsafe {
            assert_eq!(stack_after(over, &[1, 2, 3]), [1, 2, 3, 2]);
        }
    }

    #[test]
    fn rot_moves_the_third_cell_to_the_top() {
        unsafe {
            assert_eq!(stack_after(rot, &[1, 2, 3, 4]), [1, 3, 4, 2]);
        }
    }

    #[test]
    fn pick_copies_the_nth_cell() {
        unsafe {
            assert_eq!(stack_after(pick, &[1, 2, 3, 0]), [1, 2, 3, 3]);
            assert_eq!(stack_after(pick, &[1, 2, 3, 2]), [1, 2, 3, 1]);
        }
    }

    #[test]
    fn pick_matches_dup_and_over() {
        unsafe {
            assert_eq!(stack_after(pick, &[1, 2, 0]), stack_after(dup, &[1, 2]));
            assert_eq!(stack_after(pick, &[1, 2, 1]), stack_after(over, &[1, 2]));
        }
    }

    #[test]
    fn roll_moves_the_nth_cell_to_the_top() {
        unsafe {
            assert_eq!(stack_after(roll, &[1, 2, 3, 0]), [1, 2, 3]);
            assert_eq!(stack_after(roll, &[1, 2, 3, 2]), [2, 3, 1]);
        }
    }

    #[test]
    fn roll_matches_swap_and_rot() {
        unsafe {
            assert_eq!(stack_after(roll, &[1, 2, 1]), stack_after(swap, &[1, 2]));
            assert_eq!(
                stack_after(roll, &[1, 2, 3, 2]),
                stack_after(rot, &[1, 2, 3])
            );
        }
    }

    #[test]
    fn depth_counts_the_cells() {
        unsafe {
            assert_eq!(stack_after(depth, &[]), [0]);
            assert_eq!(stack_after(depth, &[7, 8]), [7, 8, 2]);
        }
    }

    #[test]
    fn words_need_enough_cells() {
        unsafe {
            for word in [dup as PrimitiveFn, question_dup, drop, pick, roll] {
                assert_underflow(word, &[]);
            }
            for word in [swap as PrimitiveFn, over, plus, store] {
                assert_underflow(word, &[]);
                assert_underflow(word, &[1]);
            }
            assert_underflow(rot, &[]);
            assert_underflow(rot, &[1]);
            assert_underflow(rot, &[1, 2]);
        }
    }

    #[test]
    fn pick_and_roll_reject_an_out_of_range_n() {
        unsafe {
            for word in [pick as PrimitiveFn, roll] {
                assert_underflow(word, &[0]);
                assert_underflow(word, &[1, 2, 2]);
                assert_underflow(word, &[1, 2, Cell::MAX]);
            }
        }
    }

    #[test]
    fn to_body_replaces_an_xt_with_its_pfa() {
        unsafe {
            let mut regs = boot(io::sink(), TEST_CELLS).unwrap();
            let xt = cfa_of(regs.dictionary.find_word("DUP").unwrap()) as Cell;
            regs.data_stack.extend_from_slice(&[7, xt]);

            to_body(&mut regs);
            assert!(regs.error.is_none(), "got: {:?}", regs.error);
            assert_eq!(regs.data_stack, [7, xt + mem::size_of::<Cell>()]);
        }
    }

    #[test]
    fn to_body_rejects_anything_else() {
        unsafe {
            let regs = run_on(to_body, &[1, 5]);
            assert!(matches!(
                regs.error,
                Some(ForthError::InvalidExecutionToken(5))
            ));
            assert_eq!(regs.data_stack, [1]);
        }
    }

    #[test]
    fn to_body_needs_a_cell() {
        unsafe {
            let regs = run_on(to_body, &[]);
            assert!(matches!(regs.error, Some(ForthError::DataStackUnderflow)));
        }
    }
}
