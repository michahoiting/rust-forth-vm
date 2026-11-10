// Copyright (c) 2026 M.W.R. Hoiting

//! Words that work on the data stack and on memory.

use std::io::Write;
use std::mem;

use crate::cell::Cell;
use crate::dictionary::{self, pfa_of};
use crate::error::ForthError;
use crate::vm::Vm;

/// The copyright notice, which `COPYRIGHT` prints.
pub const COPYRIGHT: &str = "Copyright (c) 2026 M.W.R. Hoiting.";

/// What `HELP` prints.
const HELP: &str = "Type BYE or press Ctrl-D to leave.\n";

/// The project's license, which `LICENSE` prints.
const LICENSE: &str = include_str!("../../LICENSE");

/// Writes `text` to the output.
fn write_text(vm: &mut Vm, text: &str) {
    let result = vm.output.write_all(text.as_bytes());
    vm.check(result.map_err(ForthError::Output));
}

/// `HELP ( -- )`: Writes how to leave the program to the output.
pub(super) unsafe fn help(vm: &mut Vm) {
    write_text(vm, HELP);
}

/// `COPYRIGHT ( -- )`: Writes the copyright notice to the output.
pub(super) unsafe fn copyright(vm: &mut Vm) {
    write_text(vm, COPYRIGHT);
    write_text(vm, "\n");
}

/// `LICENSE ( -- )`: Writes the project's license to the output.
pub(super) unsafe fn license(vm: &mut Vm) {
    write_text(vm, LICENSE);
}

/// `@ ( addr -- x )`: Pops a dictionary address from the stack and replaces it with the
/// contents of that Cell.
pub(super) unsafe fn fetch(vm: &mut Vm) {
    if let Some(addr) = vm.pop() {
        let result = vm.dictionary.fetch(addr);
        if let Some(value) = vm.check(result) {
            vm.push(value);
        }
    }
}

/// `! ( x addr -- )`: Pops a dictionary address and a Cell from the stack, and stores the
/// Cell there.
pub(super) unsafe fn store(vm: &mut Vm) {
    if !vm.require(2) {
        return;
    }
    if let (Some(addr), Some(value)) = (vm.data_stack.pop(), vm.data_stack.pop()) {
        let result = vm.dictionary.store(addr, value);
        vm.check(result);
    }
}

/// `C@ ( c-addr -- char )`: Pops a dictionary address from the stack and replaces it with the
/// byte stored there. The address needn't be aligned.
pub(super) unsafe fn c_fetch(vm: &mut Vm) {
    if let Some(addr) = vm.pop() {
        let result = vm.dictionary.fetch_byte(addr);
        if let Some(value) = vm.check(result) {
            vm.push(value as Cell);
        }
    }
}

/// `C! ( char c-addr -- )`: Pops a dictionary address and a Cell from the stack, and stores
/// the low byte of the Cell there. The address needn't be aligned.
pub(super) unsafe fn c_store(vm: &mut Vm) {
    if !vm.require(2) {
        return;
    }
    if let (Some(addr), Some(value)) = (vm.data_stack.pop(), vm.data_stack.pop()) {
        let result = vm.dictionary.store_byte(addr, value as u8);
        vm.check(result);
    }
}

/// `HERE ( -- addr )`: Pushes the address of the next free byte in the dictionary.
pub(super) unsafe fn here(vm: &mut Vm) {
    vm.push(vm.dictionary.here());
}

/// `ALIGNED ( addr -- a-addr )`: Rounds an address up to the next cell-aligned address.
pub(super) unsafe fn aligned(vm: &mut Vm) {
    if let Some(addr) = vm.pop() {
        vm.push(dictionary::aligned(addr));
    }
}

/// `CHAR+ ( c-addr1 -- c-addr2 )`: Adds the size of a character to an address, wrapping on
/// overflow.
pub(super) unsafe fn char_plus(vm: &mut Vm) {
    if let Some(addr) = vm.pop() {
        vm.push(addr.wrapping_add(1));
    }
}

/// `CHARS ( n1 -- n2 )`: Gives the size of `n1` characters in bytes. A character is one byte,
/// so this only checks that the Cell is there.
pub(super) unsafe fn chars(vm: &mut Vm) {
    vm.require(1);
}

/// `CELLS ( n1 -- n2 )`: Gives the size of `n1` Cells in bytes, wrapping on overflow.
pub(super) unsafe fn cells(vm: &mut Vm) {
    if let Some(n) = vm.pop() {
        vm.push(n.wrapping_mul(mem::size_of::<Cell>()));
    }
}

/// `>BODY ( xt -- pfa )`: Replaces an execution token with the address of that word's Parameter
/// Field.
pub(super) unsafe fn to_body(vm: &mut Vm) {
    if let Some(xt) = vm.pop() {
        if !vm.dictionary.is_execution_token(xt) {
            vm.set_error(ForthError::InvalidExecutionToken(xt));
            return;
        }
        vm.push(pfa_of(xt as *const Cell) as Cell);
    }
}

/// `+ ( a b -- n )`: Pops two Cells from the stack, adds them, and pushes the result. Addition
/// wraps on overflow.
pub(super) unsafe fn plus(vm: &mut Vm) {
    if !vm.require(2) {
        return;
    }
    if let (Some(b), Some(a)) = (vm.data_stack.pop(), vm.data_stack.pop()) {
        vm.push(a.wrapping_add(b));
    }
}

/// Replaces the top two Cells with the result of a binary operation.
fn binary_replace(vm: &mut Vm, operation: fn(Cell, Cell) -> Cell) {
    if !vm.require(2) {
        return;
    }
    let len = vm.data_stack.len();
    let result = operation(vm.data_stack[len - 2], vm.data_stack[len - 1]);
    vm.data_stack.truncate(len - 2);
    vm.push(result);
}

/// Returns the Forth true or false flag for `value`.
fn flag(value: bool) -> Cell {
    if value {
        Cell::MAX
    } else {
        0
    }
}

/// `- ( a b -- n )`: Subtracts `b` from `a`, wrapping on overflow.
pub(super) unsafe fn minus(vm: &mut Vm) {
    binary_replace(vm, Cell::wrapping_sub);
}

/// `* ( a b -- n )`: Multiplies two Cells, wrapping on overflow.
pub(super) unsafe fn multiply(vm: &mut Vm) {
    binary_replace(vm, Cell::wrapping_mul);
}

/// Returns the floored quotient and remainder of signed Cells.
fn floored_div_mod(dividend: Cell, divisor: Cell) -> (Cell, Cell) {
    let dividend = dividend as isize;
    let divisor = divisor as isize;
    if dividend == isize::MIN && divisor == -1 {
        return (isize::MIN as Cell, 0);
    }
    let mut quotient = dividend / divisor;
    let mut remainder = dividend % divisor;
    if remainder != 0 && remainder.signum() != divisor.signum() {
        quotient = quotient.wrapping_sub(1);
        remainder = remainder.wrapping_add(divisor);
    }
    (quotient as Cell, remainder as Cell)
}

/// Replaces two stack Cells with one result after checking the divisor.
fn division_result(vm: &mut Vm, select: fn(Cell, Cell) -> Cell) {
    if !vm.require(2) {
        return;
    }
    let len = vm.data_stack.len();
    let divisor = vm.data_stack[len - 1];
    if divisor == 0 {
        vm.set_error(ForthError::DivisionByZero);
        return;
    }
    let (quotient, remainder) = floored_div_mod(vm.data_stack[len - 2], divisor);
    vm.data_stack.truncate(len - 2);
    vm.push(select(quotient, remainder));
}

/// `/ ( n1 n2 -- n3 )`: Divides signed numbers, rounding down to negative infinity.
pub(super) unsafe fn divide(vm: &mut Vm) {
    division_result(vm, |quotient, _| quotient);
}

/// `MOD ( n1 n2 -- n3 )`: Gives the floored remainder of signed division.
pub(super) unsafe fn modulo(vm: &mut Vm) {
    division_result(vm, |_, remainder| remainder);
}

/// `/MOD ( n1 n2 -- n3 n4 )`: Gives the floored remainder and quotient of signed division.
pub(super) unsafe fn divide_mod(vm: &mut Vm) {
    if !vm.require(2) {
        return;
    }
    let len = vm.data_stack.len();
    let divisor = vm.data_stack[len - 1];
    if divisor == 0 {
        vm.set_error(ForthError::DivisionByZero);
        return;
    }
    let (quotient, remainder) = floored_div_mod(vm.data_stack[len - 2], divisor);
    vm.data_stack.truncate(len - 2);
    vm.push(remainder);
    vm.push(quotient);
}

/// `1+ ( n1 -- n2 )`: Adds one, wrapping on overflow.
pub(super) unsafe fn one_plus(vm: &mut Vm) {
    if let Some(value) = vm.pop() {
        vm.push(value.wrapping_add(1));
    }
}

/// `1- ( n1 -- n2 )`: Subtracts one, wrapping on overflow.
pub(super) unsafe fn one_minus(vm: &mut Vm) {
    if let Some(value) = vm.pop() {
        vm.push(value.wrapping_sub(1));
    }
}

/// `2* ( n1 -- n2 )`: Doubles a signed number, wrapping on overflow.
pub(super) unsafe fn two_times(vm: &mut Vm) {
    if let Some(value) = vm.pop() {
        vm.push(value.wrapping_mul(2));
    }
}

/// `2/ ( n1 -- n2 )`: Divides a signed number by two, rounding toward negative infinity.
pub(super) unsafe fn two_divide(vm: &mut Vm) {
    if let Some(value) = vm.pop() {
        vm.push(((value as isize) >> 1) as Cell);
    }
}

/// `NEGATE ( n1 -- n2 )`: Negates a signed number, wrapping on overflow.
pub(super) unsafe fn negate(vm: &mut Vm) {
    if let Some(value) = vm.pop() {
        vm.push(value.wrapping_neg());
    }
}

/// `ABS ( n1 -- n2 )`: Gives the absolute value of a signed number, wrapping on overflow.
pub(super) unsafe fn absolute(vm: &mut Vm) {
    if let Some(value) = vm.pop() {
        vm.push((value as isize).wrapping_abs() as Cell);
    }
}

/// `MIN ( n1 n2 -- n3 )`: Gives the lesser of two signed numbers.
pub(super) unsafe fn minimum(vm: &mut Vm) {
    binary_replace(vm, |a, b| if (a as isize) < (b as isize) { a } else { b });
}

/// `MAX ( n1 n2 -- n3 )`: Gives the greater of two signed numbers.
pub(super) unsafe fn maximum(vm: &mut Vm) {
    binary_replace(vm, |a, b| if (a as isize) > (b as isize) { a } else { b });
}

/// `= ( n1 n2 -- flag )`: Gives true if two Cells are equal.
pub(super) unsafe fn equal(vm: &mut Vm) {
    binary_replace(vm, |a, b| flag(a == b));
}

/// `< ( n1 n2 -- flag )`: Gives true if `n1` is less than `n2` as signed numbers.
pub(super) unsafe fn less_than(vm: &mut Vm) {
    binary_replace(vm, |a, b| flag((a as isize) < (b as isize)));
}

/// `> ( n1 n2 -- flag )`: Gives true if `n1` is greater than `n2` as signed numbers.
pub(super) unsafe fn greater_than(vm: &mut Vm) {
    binary_replace(vm, |a, b| flag((a as isize) > (b as isize)));
}

/// `U< ( u1 u2 -- flag )`: Gives true if `u1` is less than `u2` as unsigned numbers.
pub(super) unsafe fn unsigned_less_than(vm: &mut Vm) {
    binary_replace(vm, |a, b| flag(a < b));
}

/// `0= ( n -- flag )`: Gives true if `n` is zero.
pub(super) unsafe fn zero_equal(vm: &mut Vm) {
    if let Some(value) = vm.pop() {
        vm.push(flag(value == 0));
    }
}

/// `0< ( n -- flag )`: Gives true if `n` is negative.
pub(super) unsafe fn zero_less_than(vm: &mut Vm) {
    if let Some(value) = vm.pop() {
        vm.push(flag((value as isize) < 0));
    }
}

/// `0<> ( x -- flag )`: Gives true if `x` isn't zero.
pub(super) unsafe fn zero_not_equal(vm: &mut Vm) {
    if let Some(value) = vm.pop() {
        vm.push(flag(value != 0));
    }
}

/// `0> ( n -- flag )`: Gives true if `n` is greater than zero.
pub(super) unsafe fn zero_greater_than(vm: &mut Vm) {
    if let Some(value) = vm.pop() {
        vm.push(flag((value as isize) > 0));
    }
}

/// `<> ( x1 x2 -- flag )`: Gives true if two Cells differ.
pub(super) unsafe fn not_equal(vm: &mut Vm) {
    binary_replace(vm, |a, b| flag(a != b));
}

/// `U> ( u1 u2 -- flag )`: Gives true if `u1` is greater than `u2` as unsigned numbers.
pub(super) unsafe fn unsigned_greater_than(vm: &mut Vm) {
    binary_replace(vm, |a, b| flag(a > b));
}

/// `WITHIN ( n1 n2 n3 -- flag )`: Gives true if `n1` is at least `n2` and below `n3`, counting
/// around the end of the range when `n2` is greater than `n3`.
pub(super) unsafe fn within(vm: &mut Vm) {
    if !vm.require(3) {
        return;
    }
    let len = vm.data_stack.len();
    let (value, low, high) = (
        vm.data_stack[len - 3],
        vm.data_stack[len - 2],
        vm.data_stack[len - 1],
    );
    vm.data_stack.truncate(len - 3);
    vm.push(flag(value.wrapping_sub(low) < high.wrapping_sub(low)));
}

/// `AND ( x1 x2 -- x3 )`: Gives the bitwise AND of two Cells.
pub(super) unsafe fn and(vm: &mut Vm) {
    binary_replace(vm, |a, b| a & b);
}

/// `OR ( x1 x2 -- x3 )`: Gives the bitwise OR of two Cells.
pub(super) unsafe fn or(vm: &mut Vm) {
    binary_replace(vm, |a, b| a | b);
}

/// `XOR ( x1 x2 -- x3 )`: Gives the bitwise exclusive OR of two Cells.
pub(super) unsafe fn xor(vm: &mut Vm) {
    binary_replace(vm, |a, b| a ^ b);
}

/// `INVERT ( x1 -- x2 )`: Inverts every bit in a Cell. `NOT` is the same primitive, as FORTH-83
/// defines it.
pub(super) unsafe fn invert(vm: &mut Vm) {
    if let Some(value) = vm.pop() {
        vm.push(!value);
    }
}

/// `LSHIFT ( x1 u -- x2 )`: Shifts left logically, giving zero for a shift of `Cell::BITS` or
/// more.
pub(super) unsafe fn left_shift(vm: &mut Vm) {
    binary_replace(vm, |value, amount| {
        if amount >= Cell::BITS as usize {
            0
        } else {
            value << amount
        }
    });
}

/// `RSHIFT ( x1 u -- x2 )`: Shifts right logically, giving zero for a shift of `Cell::BITS` or
/// more.
pub(super) unsafe fn right_shift(vm: &mut Vm) {
    binary_replace(vm, |value, amount| {
        if amount >= Cell::BITS as usize {
            0
        } else {
            value >> amount
        }
    });
}

/// `+! ( n addr -- )`: Adds `n` to the Cell at `addr`, wrapping on overflow.
pub(super) unsafe fn plus_store(vm: &mut Vm) {
    if !vm.require(2) {
        return;
    }
    let address = vm.data_stack.pop().unwrap();
    let value = vm.data_stack.pop().unwrap();
    if let Some(old) = vm.check(vm.dictionary.fetch(address)) {
        let result = vm.dictionary.store(address, old.wrapping_add(value));
        vm.check(result);
    }
}

/// `CELL+ ( addr1 -- addr2 )`: Advances an address by the size of one Cell, wrapping on
/// overflow.
pub(super) unsafe fn cell_plus(vm: &mut Vm) {
    if let Some(address) = vm.pop() {
        vm.push(address.wrapping_add(mem::size_of::<Cell>()));
    }
}

/// `2! ( x1 x2 addr -- )`: Stores `x1` and `x2` in adjacent Cells, checking both before writing.
pub(super) unsafe fn two_store(vm: &mut Vm) {
    if !vm.require(3) {
        return;
    }
    let len = vm.data_stack.len();
    let address = vm.data_stack[len - 1];
    let second = vm.data_stack[len - 2];
    let first = vm.data_stack[len - 3];
    let result = vm.dictionary.store_two(address, first, second);
    vm.data_stack.truncate(len - 3);
    vm.check(result);
}

/// `2@ ( addr -- x1 x2 )`: Fetches two adjacent Cells from `addr`.
pub(super) unsafe fn two_fetch(vm: &mut Vm) {
    if let Some(address) = vm.pop() {
        if let Some((first, second)) = vm.check(vm.dictionary.fetch_two(address)) {
            vm.push(first);
            vm.push(second);
        }
    }
}

/// `2DROP ( x1 x2 -- )`: Removes the top two Cells.
pub(super) unsafe fn two_drop(vm: &mut Vm) {
    if vm.require(2) {
        vm.data_stack.truncate(vm.data_stack.len() - 2);
    }
}

/// `2DUP ( x1 x2 -- x1 x2 x1 x2 )`: Copies the top two Cells.
pub(super) unsafe fn two_dup(vm: &mut Vm) {
    if vm.require(2) {
        let len = vm.data_stack.len();
        let first = vm.data_stack[len - 2];
        let second = vm.data_stack[len - 1];
        vm.push(first);
        vm.push(second);
    }
}

/// `2OVER ( x1 x2 x3 x4 -- x1 x2 x3 x4 x1 x2 )`: Copies the third and fourth Cells from the top.
pub(super) unsafe fn two_over(vm: &mut Vm) {
    if vm.require(4) {
        let len = vm.data_stack.len();
        let first = vm.data_stack[len - 4];
        let second = vm.data_stack[len - 3];
        vm.push(first);
        vm.push(second);
    }
}

/// `2SWAP ( x1 x2 x3 x4 -- x3 x4 x1 x2 )`: Exchanges the top two pairs of Cells.
pub(super) unsafe fn two_swap(vm: &mut Vm) {
    if vm.require(4) {
        let len = vm.data_stack.len();
        vm.data_stack[len - 4..].rotate_left(2);
    }
}

/// `FILL ( c-addr u char -- )`: Fills `u` bytes beginning at `c-addr` with `char`'s low byte.
pub(super) unsafe fn fill(vm: &mut Vm) {
    if !vm.require(3) {
        return;
    }
    let len = vm.data_stack.len();
    let byte = vm.data_stack[len - 1] as u8;
    let count = vm.data_stack[len - 2];
    let address = vm.data_stack[len - 3];
    let result = vm.dictionary.fill(address, count, byte);
    vm.data_stack.truncate(len - 3);
    vm.check(result);
}

/// `MOVE ( addr1 addr2 u -- )`: Moves `u` bytes from `addr1` to `addr2`, allowing overlap.
pub(super) unsafe fn move_bytes(vm: &mut Vm) {
    if !vm.require(3) {
        return;
    }
    let len = vm.data_stack.len();
    let count = vm.data_stack[len - 1];
    let destination = vm.data_stack[len - 2];
    let source = vm.data_stack[len - 3];
    let result = vm.dictionary.move_bytes(source, destination, count);
    vm.data_stack.truncate(len - 3);
    vm.check(result);
}

/// `BL ( -- char )`: Pushes the value of the space character.
pub(super) unsafe fn blank(vm: &mut Vm) {
    vm.push(b' ' as Cell);
}

/// `TRUE ( -- flag )`: Pushes a true flag, with all bits set.
pub(super) unsafe fn true_flag(vm: &mut Vm) {
    vm.push(flag(true));
}

/// `FALSE ( -- flag )`: Pushes a false flag, zero.
pub(super) unsafe fn false_flag(vm: &mut Vm) {
    vm.push(flag(false));
}

/// `. ( x -- )`: Pops a Cell from the stack and writes it to the output as a signed number.
pub(super) unsafe fn dot_print(vm: &mut Vm) {
    if let Some(value) = vm.pop() {
        let result = write!(vm.output, "{} ", value as isize);
        vm.check(result.map_err(ForthError::Output));
    }
}

/// `EMIT ( char -- )`: Pops a Cell from the stack and writes its low byte to the output as one
/// character. Like `C,`, it ignores the other bytes of the Cell.
pub(super) unsafe fn emit(vm: &mut Vm) {
    if let Some(value) = vm.pop() {
        let result = vm.output.write_all(&[value as u8]);
        vm.check(result.map_err(ForthError::Output));
    }
}

/// `TYPE ( c-addr u -- )`: Pops a dictionary address and a length, and writes that many bytes
/// from the address to the output. The whole range must be written, and a negative `u` is out
/// of range too. On an error, nothing is written.
pub(super) unsafe fn type_print(vm: &mut Vm) {
    if !vm.require(2) {
        return;
    }
    if let (Some(len), Some(addr)) = (vm.data_stack.pop(), vm.data_stack.pop()) {
        let bytes = match vm.dictionary.bytes(addr, len) {
            Ok(bytes) => bytes,
            Err(error) => {
                vm.check::<()>(Err(error));
                return;
            }
        };
        let result = vm.output.write_all(bytes);
        vm.check(result.map_err(ForthError::Output));
    }
}

/// `DUP ( x -- x x )`: Copies the top Cell.
pub(super) unsafe fn dup(vm: &mut Vm) {
    if vm.require(1) {
        let top = vm.data_stack[vm.data_stack.len() - 1];
        vm.push(top);
    }
}

/// `?DUP ( x -- 0 | x x )`: Copies the top Cell if it isn't zero.
pub(super) unsafe fn question_dup(vm: &mut Vm) {
    if vm.require(1) {
        let top = vm.data_stack[vm.data_stack.len() - 1];
        if top != 0 {
            vm.push(top);
        }
    }
}

/// `DROP ( x -- )`: Removes the top Cell.
pub(super) unsafe fn drop(vm: &mut Vm) {
    vm.pop();
}

/// `SWAP ( a b -- b a )`: Exchanges the top two Cells.
pub(super) unsafe fn swap(vm: &mut Vm) {
    if vm.require(2) {
        let top = vm.data_stack.len() - 1;
        vm.data_stack.swap(top, top - 1);
    }
}

/// `OVER ( a b -- a b a )`: Copies the second Cell to the top.
pub(super) unsafe fn over(vm: &mut Vm) {
    if vm.require(2) {
        let second = vm.data_stack[vm.data_stack.len() - 2];
        vm.push(second);
    }
}

/// `NIP ( x1 x2 -- x2 )`: Removes the second Cell.
pub(super) unsafe fn nip(vm: &mut Vm) {
    if vm.require(2) {
        let second = vm.data_stack.len() - 2;
        vm.data_stack.swap_remove(second);
    }
}

/// `TUCK ( x1 x2 -- x2 x1 x2 )`: Copies the top Cell below the second one.
pub(super) unsafe fn tuck(vm: &mut Vm) {
    if vm.require(2) {
        let len = vm.data_stack.len();
        let top = vm.data_stack[len - 1];
        vm.push(top);
        // Only rotate when the push succeeded, so an overflow leaves the stack unchanged.
        if vm.data_stack.len() > len {
            vm.data_stack[len - 2..].rotate_right(1);
        }
    }
}

/// `ROT ( a b c -- b c a )`: Moves the third Cell to the top.
pub(super) unsafe fn rot(vm: &mut Vm) {
    if vm.require(3) {
        let third = vm.data_stack.len() - 3;
        vm.data_stack[third..].rotate_left(1);
    }
}

/// `PICK ( ... n -- ... x )`: Replaces `n` with a copy of the `n`th Cell below it, counting
/// from 0.
pub(super) unsafe fn pick(vm: &mut Vm) {
    if let Some(index) = vm.indexed_cell() {
        let top = vm.data_stack.len() - 1;
        vm.data_stack[top] = vm.data_stack[index];
    }
}

/// `ROLL ( ... n -- ... x )`: Replaces `n` by moving the `n`th Cell below it to the top.
pub(super) unsafe fn roll(vm: &mut Vm) {
    if let Some(index) = vm.indexed_cell() {
        vm.data_stack.pop();
        let cell = vm.data_stack.remove(index);
        vm.push(cell);
    }
}

/// `DEPTH ( -- n )`: Pushes the number of Cells that were on the stack.
pub(super) unsafe fn depth(vm: &mut Vm) {
    vm.push(vm.data_stack.len() as Cell);
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

    // Runs a word on an empty stack, checks that it leaves the VM's state alone, and returns
    // what it wrote.
    unsafe fn output_of(word: PrimitiveFn) -> String {
        let output = Capture::default();
        let mut vm = boot(output.clone(), VmConfig::new(TEST_CELLS)).unwrap();
        let here = vm.dictionary.here();
        word(&mut vm);
        assert!(vm.error.is_none());
        assert!(vm.data_stack.is_empty());
        assert_eq!(vm.dictionary.here(), here);
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
    unsafe fn run_on(word: PrimitiveFn, before: &[Cell]) -> Vm {
        let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
        vm.data_stack.extend_from_slice(before);
        word(&mut vm);
        vm
    }

    // Runs a word that must succeed, and returns the resulting stack.
    unsafe fn stack_after(word: PrimitiveFn, before: &[Cell]) -> Vec<Cell> {
        let vm = run_on(word, before);
        assert!(vm.error.is_none(), "got: {:?}", vm.error);
        vm.data_stack
    }

    // Asserts that a word reports an underflow and leaves the stack unchanged.
    unsafe fn assert_underflow(word: PrimitiveFn, before: &[Cell]) {
        let vm = run_on(word, before);
        assert!(
            matches!(vm.error, Some(ForthError::DataStackUnderflow)),
            "got: {:?}",
            vm.error
        );
        assert_eq!(vm.data_stack, before);
    }

    // Boots a VM whose output is captured, and returns the VM with the capture.
    unsafe fn captured_vm() -> (Vm, Capture) {
        let output = Capture::default();
        let vm = boot(output.clone(), VmConfig::new(TEST_CELLS)).unwrap();
        (vm, output)
    }

    // Returns what a capture holds as text.
    fn text(output: &Capture) -> String {
        String::from_utf8(output.0.borrow().clone()).unwrap()
    }

    #[test]
    fn emit_writes_the_low_byte_as_one_character() {
        unsafe {
            let (mut vm, output) = captured_vm();
            vm.data_stack.extend_from_slice(&[0x141]);

            emit(&mut vm);
            assert!(vm.error.is_none(), "got: {:?}", vm.error);
            assert!(vm.data_stack.is_empty());
            assert_eq!(text(&output), "A");
        }
    }

    #[test]
    fn type_writes_the_bytes_at_an_address() {
        unsafe {
            let (mut vm, output) = captured_vm();
            let addr = vm.dictionary.here();
            vm.dictionary.char_comma(b'h').unwrap();
            vm.dictionary.char_comma(b'i').unwrap();
            vm.data_stack.extend_from_slice(&[addr, 2]);

            type_print(&mut vm);
            assert!(vm.error.is_none(), "got: {:?}", vm.error);
            assert!(vm.data_stack.is_empty());
            assert_eq!(text(&output), "hi");
        }
    }

    #[test]
    fn type_of_zero_bytes_at_here_writes_nothing() {
        unsafe {
            let (mut vm, output) = captured_vm();
            let here = vm.dictionary.here();
            vm.data_stack.extend_from_slice(&[here, 0]);

            type_print(&mut vm);
            assert!(vm.error.is_none(), "got: {:?}", vm.error);
            assert!(vm.data_stack.is_empty());
            assert_eq!(text(&output), "");
        }
    }

    #[test]
    fn type_of_an_unwritten_range_writes_nothing_and_reports_the_address() {
        unsafe {
            let (mut vm, output) = captured_vm();
            let addr = vm.dictionary.here();
            vm.dictionary.char_comma(b'x').unwrap();
            vm.data_stack.extend_from_slice(&[addr, 2]);

            type_print(&mut vm);
            assert!(
                matches!(vm.error, Some(ForthError::UnwrittenAddress(a)) if a == addr),
                "got: {:?}",
                vm.error
            );
            assert!(vm.data_stack.is_empty());
            assert_eq!(text(&output), "");
        }
    }

    #[test]
    fn type_of_a_negative_length_writes_nothing() {
        unsafe {
            let (mut vm, output) = captured_vm();
            let addr = vm.dictionary.here();
            vm.dictionary.char_comma(b'x').unwrap();
            vm.data_stack.extend_from_slice(&[addr, -1isize as Cell]);

            type_print(&mut vm);
            assert!(
                matches!(vm.error, Some(ForthError::UnwrittenAddress(a)) if a == addr),
                "got: {:?}",
                vm.error
            );
            assert_eq!(text(&output), "");
        }
    }

    #[test]
    fn c_fetch_replaces_an_address_with_its_byte() {
        unsafe {
            let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
            let addr = vm.dictionary.here();
            vm.dictionary.char_comma(0xAB).unwrap();
            vm.data_stack.extend_from_slice(&[1, addr]);

            c_fetch(&mut vm);
            assert!(vm.error.is_none(), "got: {:?}", vm.error);
            assert_eq!(vm.data_stack, [1, 0xAB]);
        }
    }

    #[test]
    fn c_store_stores_the_low_byte_and_pops_both_cells() {
        unsafe {
            let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
            let addr = vm.dictionary.here();
            vm.dictionary.char_comma(0).unwrap();
            vm.data_stack.extend_from_slice(&[1, 0x1AB, addr]);

            c_store(&mut vm);
            assert!(vm.error.is_none(), "got: {:?}", vm.error);
            assert_eq!(vm.data_stack, [1]);
            assert_eq!(vm.dictionary.fetch_byte(addr).unwrap(), 0xAB);
        }
    }

    #[test]
    fn here_pushes_the_dictionary_pointer() {
        unsafe {
            let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
            vm.dictionary.char_comma(0).unwrap();
            let expected = vm.dictionary.here();

            here(&mut vm);
            assert_eq!(vm.data_stack, [expected]);
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
    fn remaining_comparison_words_give_forth_flags() {
        unsafe {
            let minus_one = (-1isize) as Cell;
            assert_eq!(stack_after(zero_not_equal, &[7, 0]), [7, 0]);
            assert_eq!(stack_after(zero_not_equal, &[7, 1]), [7, Cell::MAX]);
            assert_eq!(stack_after(zero_not_equal, &[minus_one]), [Cell::MAX]);
            assert_eq!(stack_after(zero_greater_than, &[7, 1]), [7, Cell::MAX]);
            assert_eq!(stack_after(zero_greater_than, &[7, 0]), [7, 0]);
            assert_eq!(stack_after(zero_greater_than, &[minus_one]), [0]);
            assert_eq!(stack_after(not_equal, &[7, 3, 3]), [7, 0]);
            assert_eq!(stack_after(not_equal, &[7, 3, 4]), [7, Cell::MAX]);
            assert_eq!(
                stack_after(unsigned_greater_than, &[Cell::MAX, 0]),
                [Cell::MAX]
            );
            assert_eq!(stack_after(unsigned_greater_than, &[0, Cell::MAX]), [0]);
            assert_eq!(stack_after(unsigned_greater_than, &[5, 5]), [0]);
        }
    }

    #[test]
    fn true_and_false_push_flags() {
        unsafe {
            assert_eq!(stack_after(true_flag, &[9]), [9, Cell::MAX]);
            assert_eq!(stack_after(false_flag, &[9]), [9, 0]);
        }
    }

    #[test]
    fn within_includes_the_low_limit_and_excludes_the_high_limit() {
        unsafe {
            assert_eq!(stack_after(within, &[7, 5, 1, 10]), [7, Cell::MAX]);
            assert_eq!(stack_after(within, &[1, 1, 10]), [Cell::MAX]);
            assert_eq!(stack_after(within, &[10, 1, 10]), [0]);
            assert_eq!(stack_after(within, &[11, 1, 10]), [0]);
            assert_eq!(stack_after(within, &[0, 1, 10]), [0]);
            assert_eq!(stack_after(within, &[5, 5, 5]), [0]);
        }
    }

    #[test]
    fn within_counts_around_the_end_of_the_range() {
        unsafe {
            assert_eq!(stack_after(within, &[1, 10, 3]), [Cell::MAX]);
            assert_eq!(stack_after(within, &[Cell::MAX, 10, 3]), [Cell::MAX]);
            assert_eq!(stack_after(within, &[3, 10, 3]), [0]);
            assert_eq!(stack_after(within, &[5, 10, 3]), [0]);
            assert_eq!(stack_after(within, &[10, 10, 3]), [Cell::MAX]);
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
    fn nip_removes_the_second_cell() {
        unsafe {
            assert_eq!(stack_after(nip, &[1, 2, 3]), [1, 3]);
        }
    }

    #[test]
    fn tuck_copies_the_top_cell_below_the_second() {
        unsafe {
            assert_eq!(stack_after(tuck, &[1, 2, 3]), [1, 3, 2, 3]);
        }
    }

    #[test]
    fn tuck_on_a_full_stack_overflows_and_leaves_the_stack_unchanged() {
        unsafe {
            let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
            vm.data_stack_limit = 2;
            vm.data_stack.extend_from_slice(&[1, 2]);
            tuck(&mut vm);
            assert!(matches!(vm.error, Some(ForthError::DataStackOverflow)));
            assert_eq!(vm.data_stack, [1, 2]);
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
            for word in [swap as PrimitiveFn, over, nip, tuck, plus, store] {
                assert_underflow(word, &[]);
                assert_underflow(word, &[1]);
            }
            assert_underflow(rot, &[]);
            assert_underflow(rot, &[1]);
            assert_underflow(rot, &[1, 2]);
        }
    }

    #[test]
    fn new_words_have_their_expected_stack_effects() {
        unsafe {
            let min = isize::MIN as Cell;
            let max = isize::MAX as Cell;
            assert_eq!(stack_after(minus, &[9, 4]), [5]);
            assert_eq!(stack_after(multiply, &[9, 4]), [36]);
            assert_eq!(stack_after(minus, &[0, 1]), [Cell::MAX]);
            assert_eq!(stack_after(multiply, &[min, Cell::MAX]), [min]);
            assert_eq!(stack_after(one_plus, &[Cell::MAX]), [0]);
            assert_eq!(stack_after(one_plus, &[max]), [min]);
            assert_eq!(stack_after(one_minus, &[0]), [Cell::MAX]);
            assert_eq!(stack_after(two_times, &[max]), [Cell::MAX - 1]);
            assert_eq!(
                stack_after(two_divide, &[(-3isize) as Cell]),
                [(-2isize) as Cell]
            );
            assert_eq!(stack_after(negate, &[min]), [min]);
            assert_eq!(stack_after(absolute, &[min]), [min]);
            assert_eq!(
                stack_after(minimum, &[(-3isize) as Cell, 2]),
                [(-3isize) as Cell]
            );
            assert_eq!(stack_after(maximum, &[(-3isize) as Cell, 2]), [2]);
            assert_eq!(stack_after(minimum, &[min, max]), [min]);
            assert_eq!(stack_after(maximum, &[min, max]), [max]);
            assert_eq!(stack_after(equal, &[4, 4]), [Cell::MAX]);
            assert_eq!(stack_after(less_than, &[(-1isize) as Cell, 0]), [Cell::MAX]);
            assert_eq!(stack_after(greater_than, &[2, 1]), [Cell::MAX]);
            assert_eq!(stack_after(unsigned_less_than, &[Cell::MAX, 0]), [0]);
            assert_eq!(stack_after(zero_equal, &[0]), [Cell::MAX]);
            assert_eq!(
                stack_after(zero_less_than, &[(-1isize) as Cell]),
                [Cell::MAX]
            );
            assert_eq!(stack_after(and, &[0b1100, 0b1010]), [0b1000]);
            assert_eq!(stack_after(or, &[0b1100, 0b1010]), [0b1110]);
            assert_eq!(stack_after(xor, &[0b1100, 0b1010]), [0b0110]);
            assert_eq!(stack_after(invert, &[0]), [Cell::MAX]);
            assert_eq!(stack_after(left_shift, &[3, 2]), [12]);
            assert_eq!(stack_after(right_shift, &[12, 2]), [3]);
            assert_eq!(stack_after(cell_plus, &[16]), [16 + mem::size_of::<Cell>()]);
            assert_eq!(stack_after(two_drop, &[1, 2, 3]), [1]);
            assert_eq!(stack_after(two_dup, &[1, 2]), [1, 2, 1, 2]);
            assert_eq!(stack_after(two_over, &[1, 2, 3, 4]), [1, 2, 3, 4, 1, 2]);
            assert_eq!(stack_after(two_swap, &[1, 2, 3, 4]), [3, 4, 1, 2]);
            assert_eq!(stack_after(blank, &[]), [b' ' as Cell]);
        }
    }

    #[test]
    fn division_is_floored_for_every_sign_combination() {
        unsafe {
            let cases = [
                (7, 3, 2, 1),
                ((-7isize) as Cell, 3, (-3isize) as Cell, 2),
                (7, (-3isize) as Cell, (-3isize) as Cell, (-2isize) as Cell),
                ((-7isize) as Cell, (-3isize) as Cell, 2, (-1isize) as Cell),
            ];
            for (dividend, divisor, quotient, remainder) in cases {
                assert_eq!(stack_after(divide, &[dividend, divisor]), [quotient]);
                assert_eq!(stack_after(modulo, &[dividend, divisor]), [remainder]);
                assert_eq!(
                    stack_after(divide_mod, &[dividend, divisor]),
                    [remainder, quotient]
                );
            }
            let minus_one = Cell::MAX;
            let min = isize::MIN as Cell;
            assert_eq!(stack_after(divide, &[5, minus_one]), [(-5isize) as Cell]);
            assert_eq!(stack_after(modulo, &[5, minus_one]), [0]);
            assert_eq!(
                stack_after(divide, &[min, 3]),
                [(isize::MIN / 3 - 1) as Cell]
            );
            assert_eq!(
                stack_after(modulo, &[min, 3]),
                [(isize::MIN % 3 + 3) as Cell]
            );
            assert_eq!(stack_after(divide, &[min, 1]), [min]);
            assert_eq!(
                stack_after(divide, &[isize::MIN as Cell, Cell::MAX]),
                [isize::MIN as Cell]
            );
            assert_eq!(stack_after(modulo, &[isize::MIN as Cell, Cell::MAX]), [0]);
            assert_eq!(
                stack_after(divide_mod, &[isize::MIN as Cell, Cell::MAX]),
                [0, isize::MIN as Cell]
            );
        }
    }

    #[test]
    fn division_by_zero_preserves_the_stack_and_sets_an_error() {
        unsafe {
            for word in [divide as PrimitiveFn, modulo, divide_mod] {
                let vm = run_on(word, &[7, 0]);
                assert!(matches!(vm.error, Some(ForthError::DivisionByZero)));
                assert_eq!(vm.data_stack, [7, 0]);
            }
        }
    }

    #[test]
    fn shifts_handle_zero_and_cell_width_boundaries() {
        unsafe {
            let high_bit = (1 as Cell) << (Cell::BITS - 1);
            assert_eq!(stack_after(left_shift, &[1, 0]), [1]);
            assert_eq!(
                stack_after(left_shift, &[1, (Cell::BITS - 1) as Cell]),
                [high_bit]
            );
            assert_eq!(
                stack_after(left_shift, &[Cell::MAX, Cell::BITS as Cell]),
                [0]
            );
            assert_eq!(stack_after(left_shift, &[Cell::MAX, Cell::MAX]), [0]);
            assert_eq!(stack_after(right_shift, &[Cell::MAX, 0]), [Cell::MAX]);
            assert_eq!(
                stack_after(right_shift, &[Cell::MAX, (Cell::BITS - 1) as Cell]),
                [1]
            );
            assert_eq!(
                stack_after(right_shift, &[Cell::MAX, Cell::BITS as Cell]),
                [0]
            );
        }
    }

    #[test]
    fn memory_words_read_write_and_move_data() {
        unsafe {
            let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
            let start = vm.dictionary.here();
            vm.dictionary.comma(2).unwrap();
            vm.dictionary.comma(3).unwrap();
            let cell_size = mem::size_of::<Cell>();

            vm.data_stack.extend_from_slice(&[5, start]);
            plus_store(&mut vm);
            assert_eq!(vm.dictionary.fetch(start).unwrap(), 7);
            vm.dictionary.store(start, Cell::MAX).unwrap();
            vm.data_stack.extend_from_slice(&[1, start]);
            plus_store(&mut vm);
            assert_eq!(vm.dictionary.fetch(start).unwrap(), 0);

            vm.data_stack.extend_from_slice(&[11, 12, start]);
            two_store(&mut vm);
            assert!(vm.error.is_none());
            assert!(vm.data_stack.is_empty());
            assert_eq!(vm.dictionary.fetch_two(start).unwrap(), (11, 12));

            vm.data_stack.push(start);
            two_fetch(&mut vm);
            assert_eq!(vm.data_stack, [11, 12]);
            vm.data_stack.clear();

            let byte_start = start + 2 * cell_size;
            for byte in 0..8 {
                vm.dictionary.char_comma((byte + 1) as u8).unwrap();
            }
            vm.data_stack.extend_from_slice(&[byte_start, 8, 0xAA]);
            fill(&mut vm);
            assert!(vm.error.is_none());
            assert!(vm.data_stack.is_empty());
            vm.data_stack
                .extend_from_slice(&[byte_start, byte_start + 1, 7]);
            move_bytes(&mut vm);
            assert!(vm.error.is_none());
            assert_eq!(
                (0..8)
                    .map(|offset| vm.dictionary.fetch_byte(byte_start + offset).unwrap())
                    .collect::<Vec<_>>(),
                [0xAA; 8]
            );
        }
    }

    #[test]
    fn all_new_words_leave_the_stack_unchanged_on_underflow() {
        unsafe {
            for word in [
                one_plus as PrimitiveFn,
                one_minus,
                two_times,
                two_divide,
                negate,
                absolute,
                zero_equal,
                zero_less_than,
                zero_not_equal,
                zero_greater_than,
                invert,
                cell_plus,
                emit,
                type_print,
            ] {
                assert_underflow(word, &[]);
            }
            for word in [
                minus as PrimitiveFn,
                multiply,
                divide,
                modulo,
                divide_mod,
                minimum,
                maximum,
                equal,
                less_than,
                greater_than,
                unsigned_less_than,
                not_equal,
                unsigned_greater_than,
                and,
                or,
                xor,
                left_shift,
                right_shift,
                plus_store,
            ] {
                assert_underflow(word, &[]);
                assert_underflow(word, &[1]);
            }
            for word in [two_store as PrimitiveFn, fill, move_bytes, within] {
                for stack in [&[][..], &[1][..], &[1, 2][..]] {
                    assert_underflow(word, stack);
                }
            }
            assert_underflow(two_fetch, &[]);
            for word in [two_drop as PrimitiveFn, two_dup] {
                assert_underflow(word, &[]);
                assert_underflow(word, &[1]);
            }
            assert_underflow(two_over, &[]);
            assert_underflow(two_over, &[1]);
            assert_underflow(two_over, &[1, 2]);
            assert_underflow(two_over, &[1, 2, 3]);
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
            let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
            let xt = cfa_of(vm.dictionary.find_word("DUP").unwrap()) as Cell;
            vm.data_stack.extend_from_slice(&[7, xt]);

            to_body(&mut vm);
            assert!(vm.error.is_none(), "got: {:?}", vm.error);
            assert_eq!(vm.data_stack, [7, xt + mem::size_of::<Cell>()]);
        }
    }

    #[test]
    fn to_body_rejects_anything_else() {
        unsafe {
            let vm = run_on(to_body, &[1, 5]);
            assert!(matches!(
                vm.error,
                Some(ForthError::InvalidExecutionToken(5))
            ));
            assert_eq!(vm.data_stack, [1]);
        }
    }

    #[test]
    fn to_body_needs_a_cell() {
        unsafe {
            let vm = run_on(to_body, &[]);
            assert!(matches!(vm.error, Some(ForthError::DataStackUnderflow)));
        }
    }
}
