// Copyright (c) 2026 M.W.R. Hoiting

//! Words that work on the data stack and on memory.

use std::io::Write;
use std::mem;

use crate::cell::Cell;
use crate::dictionary::{self, pfa_of};
use crate::error::ForthError;
use crate::vm::Registers;

/// The copyright notice, which `COPYRIGHT` prints.
pub const COPYRIGHT: &str = "Copyright (c) 2026 M.W.R. Hoiting.";

/// What `HELP` prints.
const HELP: &str = "Type BYE or press Ctrl-D to leave.\n";

/// The project's license, which `LICENSE` prints.
const LICENSE: &str = include_str!("../../LICENSE");

/// Writes `text` to the output.
fn write_text(regs: &mut Registers, text: &str) {
    let result = regs.output.write_all(text.as_bytes());
    regs.check(result.map_err(ForthError::Output));
}

/// `HELP ( -- )`: Writes how to leave the program to the output.
pub(super) unsafe fn help(regs: &mut Registers) {
    write_text(regs, HELP);
}

/// `COPYRIGHT ( -- )`: Writes the copyright notice to the output.
pub(super) unsafe fn copyright(regs: &mut Registers) {
    write_text(regs, COPYRIGHT);
    write_text(regs, "\n");
}

/// `LICENSE ( -- )`: Writes the project's license to the output.
pub(super) unsafe fn license(regs: &mut Registers) {
    write_text(regs, LICENSE);
}

/// `@ ( addr -- x )`: Pops a dictionary address from the stack and replaces it with the
/// contents of that Cell.
pub(super) unsafe fn fetch(regs: &mut Registers) {
    if let Some(addr) = regs.pop() {
        let result = regs.dictionary.fetch(addr);
        if let Some(value) = regs.check(result) {
            regs.push(value);
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

/// `C@ ( c-addr -- char )`: Pops a dictionary address from the stack and replaces it with the
/// byte stored there. The address needn't be aligned.
pub(super) unsafe fn c_fetch(regs: &mut Registers) {
    if let Some(addr) = regs.pop() {
        let result = regs.dictionary.fetch_byte(addr);
        if let Some(value) = regs.check(result) {
            regs.push(value as Cell);
        }
    }
}

/// `C! ( char c-addr -- )`: Pops a dictionary address and a Cell from the stack, and stores
/// the low byte of the Cell there. The address needn't be aligned.
pub(super) unsafe fn c_store(regs: &mut Registers) {
    if !regs.require(2) {
        return;
    }
    if let (Some(addr), Some(value)) = (regs.data_stack.pop(), regs.data_stack.pop()) {
        let result = regs.dictionary.store_byte(addr, value as u8);
        regs.check(result);
    }
}

/// `HERE ( -- addr )`: Pushes the address of the next free byte in the dictionary.
pub(super) unsafe fn here(regs: &mut Registers) {
    regs.push(regs.dictionary.here());
}

/// `ALIGNED ( addr -- a-addr )`: Rounds an address up to the next cell-aligned address.
pub(super) unsafe fn aligned(regs: &mut Registers) {
    if let Some(addr) = regs.pop() {
        regs.push(dictionary::aligned(addr));
    }
}

/// `CHAR+ ( c-addr1 -- c-addr2 )`: Adds the size of a character to an address, wrapping on
/// overflow.
pub(super) unsafe fn char_plus(regs: &mut Registers) {
    if let Some(addr) = regs.pop() {
        regs.push(addr.wrapping_add(1));
    }
}

/// `CHARS ( n1 -- n2 )`: Gives the size of `n1` characters in bytes. A character is one byte,
/// so this only checks that the Cell is there.
pub(super) unsafe fn chars(regs: &mut Registers) {
    regs.require(1);
}

/// `CELLS ( n1 -- n2 )`: Gives the size of `n1` Cells in bytes, wrapping on overflow.
pub(super) unsafe fn cells(regs: &mut Registers) {
    if let Some(n) = regs.pop() {
        regs.push(n.wrapping_mul(mem::size_of::<Cell>()));
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
        regs.push(pfa_of(xt as *const Cell) as Cell);
    }
}

/// `+ ( a b -- n )`: Pops two Cells from the stack, adds them, and pushes the result. Addition
/// wraps on overflow.
pub(super) unsafe fn plus(regs: &mut Registers) {
    if !regs.require(2) {
        return;
    }
    if let (Some(b), Some(a)) = (regs.data_stack.pop(), regs.data_stack.pop()) {
        regs.push(a.wrapping_add(b));
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
        regs.push(top);
    }
}

/// `?DUP ( x -- 0 | x x )`: Copies the top Cell if it isn't zero.
pub(super) unsafe fn question_dup(regs: &mut Registers) {
    if regs.require(1) {
        let top = regs.data_stack[regs.data_stack.len() - 1];
        if top != 0 {
            regs.push(top);
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
        regs.push(second);
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
        regs.push(cell);
    }
}

/// `DEPTH ( -- n )`: Pushes the number of Cells that were on the stack.
pub(super) unsafe fn depth(regs: &mut Registers) {
    regs.push(regs.data_stack.len() as Cell);
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::io;
    use std::mem;
    use std::rc::Rc;

    use super::*;
    use crate::dictionary::cfa_of;
    use crate::vm::PrimitiveFn;
    use crate::{boot, VmConfig};

    const TEST_CELLS: usize = 1024;

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

    // Runs a word on an empty stack, checks that it leaves the VM's state alone, and returns
    // what it wrote.
    unsafe fn output_of(word: PrimitiveFn) -> String {
        let output = Capture::default();
        let mut regs = boot(output.clone(), VmConfig::new(TEST_CELLS)).unwrap();
        let here = regs.dictionary.here();
        word(&mut regs);
        assert!(regs.error.is_none());
        assert!(regs.data_stack.is_empty());
        assert_eq!(regs.dictionary.here(), here);
        let bytes = output.0.borrow().clone();
        String::from_utf8(bytes).unwrap()
    }

    #[test]
    fn help_writes_how_to_leave() {
        unsafe {
            assert_eq!(output_of(help), "Type BYE or press Ctrl-D to leave.\n");
        }
    }

    #[test]
    fn copyright_writes_the_notice_on_a_line() {
        unsafe {
            assert_eq!(output_of(copyright), "Copyright (c) 2026 M.W.R. Hoiting.\n");
        }
    }

    #[test]
    fn license_writes_the_license_file() {
        unsafe {
            assert_eq!(output_of(license), include_str!("../../LICENSE"));
        }
    }

    // Runs a word on a stack holding `before`.
    unsafe fn run_on(word: PrimitiveFn, before: &[Cell]) -> Registers {
        let mut regs = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
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
    fn c_fetch_replaces_an_address_with_its_byte() {
        unsafe {
            let mut regs = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
            let addr = regs.dictionary.here();
            regs.dictionary.char_comma(0xAB).unwrap();
            regs.data_stack.extend_from_slice(&[1, addr]);

            c_fetch(&mut regs);
            assert!(regs.error.is_none(), "got: {:?}", regs.error);
            assert_eq!(regs.data_stack, [1, 0xAB]);
        }
    }

    #[test]
    fn c_store_stores_the_low_byte_and_pops_both_cells() {
        unsafe {
            let mut regs = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
            let addr = regs.dictionary.here();
            regs.dictionary.char_comma(0).unwrap();
            regs.data_stack.extend_from_slice(&[1, 0x1AB, addr]);

            c_store(&mut regs);
            assert!(regs.error.is_none(), "got: {:?}", regs.error);
            assert_eq!(regs.data_stack, [1]);
            assert_eq!(regs.dictionary.fetch_byte(addr).unwrap(), 0xAB);
        }
    }

    #[test]
    fn here_pushes_the_dictionary_pointer() {
        unsafe {
            let mut regs = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
            regs.dictionary.char_comma(0).unwrap();
            let expected = regs.dictionary.here();

            here(&mut regs);
            assert_eq!(regs.data_stack, [expected]);
        }
    }

    #[test]
    fn aligned_rounds_the_top_cell_up() {
        unsafe {
            let cell_size = mem::size_of::<Cell>();
            assert_eq!(stack_after(aligned, &[9, 1]), [9, cell_size]);
            assert_eq!(stack_after(aligned, &[cell_size]), [cell_size]);
        }
    }

    #[test]
    fn char_plus_adds_one_and_wraps() {
        unsafe {
            assert_eq!(stack_after(char_plus, &[9, 4]), [9, 5]);
            assert_eq!(stack_after(char_plus, &[Cell::MAX]), [0]);
        }
    }

    #[test]
    fn chars_leaves_the_top_cell_as_it_is() {
        unsafe {
            assert_eq!(stack_after(chars, &[9, 4]), [9, 4]);
        }
    }

    #[test]
    fn cells_multiplies_by_the_size_of_a_cell() {
        unsafe {
            let cell_size = mem::size_of::<Cell>();
            assert_eq!(stack_after(cells, &[9, 0]), [9, 0]);
            assert_eq!(stack_after(cells, &[9, 3]), [9, 3 * cell_size]);
        }
    }

    #[test]
    fn cells_wraps_on_overflow() {
        unsafe {
            let cell_size = mem::size_of::<Cell>();
            assert_eq!(
                stack_after(cells, &[Cell::MAX]),
                [0usize.wrapping_sub(cell_size)]
            );
        }
    }

    #[test]
    fn byte_words_need_enough_cells() {
        unsafe {
            assert_underflow(c_fetch, &[]);
            assert_underflow(c_store, &[]);
            assert_underflow(c_store, &[1]);
            assert_underflow(aligned, &[]);
            assert_underflow(char_plus, &[]);
            assert_underflow(chars, &[]);
            assert_underflow(cells, &[]);
        }
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
            let mut regs = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
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
