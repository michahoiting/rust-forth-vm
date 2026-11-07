// Copyright (c) 2026 M.W.R. Hoiting

//! The state of the VM, and the inner interpreter that runs threaded code.

use std::io::Write;
use std::mem;
use std::ptr;

use crate::cell::Cell;
use crate::compiler::Compiler;
use crate::dictionary::{pfa_of, Dictionary};
use crate::error::ForthError;
use crate::input::InputSource;

/// Definition of a Forth primitive (machine-code function).
/// Primitives return to the [`run`] loop instead of calling NEXT themselves, so
/// threaded code never grows the Rust call stack.
///
/// A primitive reports failure with [`Registers::set_error`], and leaves the VM to stop.
///
/// # Safety
///
/// Every primitive shares this contract, so they don't repeat it. `regs` must be a booted VM,
/// with `IP` pointing to the rest of the running thread. Primitives that use `W` also need it
/// to point to the Code Field of their word, which [`enter_word`] arranges.
pub(crate) type PrimitiveFn = unsafe fn(registers: &mut Registers);

/// The data stack limit that [`VmConfig::new`] chooses, in Cells.
pub const DEFAULT_DATA_STACK_CELLS: usize = 1024;

/// The return stack limit that [`VmConfig::new`] chooses, in Cells.
pub const DEFAULT_RETURN_STACK_CELLS: usize = 1024;

/// The sizes of a VM, in Cells, which [`boot`](crate::boot) takes. None of them can change
/// after that.
///
/// A push onto a full stack gives [`ForthError::DataStackOverflow`] or
/// [`ForthError::ReturnStackOverflow`]. A stack limit of 0 is allowed, and makes the first push
/// fail. The return stack holds one Cell for each level of nested colon words, including the
/// word that the text interpreter runs.
///
/// # Examples
///
/// ```
/// use std::io;
///
/// use rust_forth_vm::{boot, outer_interpreter, ForthError, VmConfig};
///
/// let config = VmConfig {
///     data_stack_cells: 2,
///     ..VmConfig::new(1024)
/// };
///
/// // SAFETY: this VM is only used through the crate's functions.
/// unsafe {
///     let mut regs = boot(io::sink(), config)?;
///     assert!(matches!(
///         outer_interpreter(&mut regs, "1 2 3"),
///         Err(ForthError::DataStackOverflow)
///     ));
/// }
/// # Ok::<(), ForthError>(())
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VmConfig {
    /// The size of the dictionary.
    pub dictionary_cells: usize,
    /// The most Cells that the data stack can hold.
    pub data_stack_cells: usize,
    /// The most Cells that the return stack can hold.
    pub return_stack_cells: usize,
}

impl VmConfig {
    /// Returns a configuration with a dictionary of `dictionary_cells` Cells, and stacks of
    /// [`DEFAULT_DATA_STACK_CELLS`] and [`DEFAULT_RETURN_STACK_CELLS`] Cells.
    pub fn new(dictionary_cells: usize) -> VmConfig {
        VmConfig {
            dictionary_cells,
            data_stack_cells: DEFAULT_DATA_STACK_CELLS,
            return_stack_cells: DEFAULT_RETURN_STACK_CELLS,
        }
    }
}

/// Creates an empty stack with room for `cells` entries, so that it doesn't reallocate later.
///
/// # Errors
///
/// Returns [`ForthError::StackTooLarge`] if the stack can't be allocated.
fn reserved_stack<T>(cells: usize) -> Result<Vec<T>, ForthError> {
    let mut stack = Vec::new();
    stack
        .try_reserve_exact(cells)
        .map_err(|_| ForthError::StackTooLarge(cells))?;
    Ok(stack)
}

/// The state of a Forth VM. Create one with [`boot`](crate::boot), and evaluate Forth with
/// [`outer_interpreter`](crate::outer_interpreter).
///
/// This is the whole VM, not only its registers: it owns both stacks, the dictionary, the
/// input and the output. Its contents are private, so the data stack can only be seen
/// through Forth words such as `.` and `DEPTH`. It keeps its state between calls.
//
// CPU Registers and Stacks conforming to the Forth hardware architecture
pub struct Registers {
    /// Instruction Pointer: points to a Cell in a thread.
    pub(crate) ip: *const Cell,
    /// Word Register: points to the currently active CFA Cell.
    pub(crate) w: *const Cell,
    /// Parameter stack (contains raw Cells).
    pub(crate) data_stack: Vec<Cell>,
    /// Return stack (R-stack) for nested IP pointers.
    pub(crate) return_stack: Vec<*const Cell>,
    /// The most Cells that the data stack can hold.
    pub(crate) data_stack_limit: usize,
    /// The most Cells that the return stack can hold.
    pub(crate) return_stack_limit: usize,
    /// Word storage, HERE and the head of the word list.
    pub(crate) dictionary: Dictionary,

    /// Compilation state.
    pub(crate) compiler: Compiler,
    /// Where Forth words write their output.
    pub(crate) output: Box<dyn Write>,
    /// The text being interpreted.
    pub(crate) input: InputSource,
    /// Set by a primitive to stop the VM.
    pub(crate) error: Option<ForthError>,
}

impl Registers {
    /// Returns whether a definition is open, so that the VM is compiling rather than
    /// interpreting. A definition can continue in the next call to
    /// [`outer_interpreter`](crate::outer_interpreter).
    ///
    /// # Examples
    ///
    /// ```
    /// use std::io;
    ///
    /// use rust_forth_vm::{boot, outer_interpreter, ForthError, VmConfig};
    ///
    /// // SAFETY: this VM is only used through the crate's functions.
    /// unsafe {
    ///     let mut regs = boot(io::sink(), VmConfig::new(1024))?;
    ///     assert!(!regs.is_compiling());
    ///     outer_interpreter(&mut regs, ": THREE 3")?;
    ///     assert!(regs.is_compiling());
    ///     outer_interpreter(&mut regs, ";")?;
    ///     assert!(!regs.is_compiling());
    /// }
    /// # Ok::<(), ForthError>(())
    /// ```
    pub fn is_compiling(&self) -> bool {
        self.compiler.is_compiling()
    }

    /// Creates a VM with an empty dictionary and empty stacks, of the sizes in `config`.
    ///
    /// # Errors
    ///
    /// Returns [`ForthError::DictionaryTooLarge`] if the dictionary can't be allocated, and
    /// [`ForthError::StackTooLarge`] if a stack can't be.
    pub(crate) unsafe fn new(
        output: Box<dyn Write>,
        config: VmConfig,
    ) -> Result<Registers, ForthError> {
        Ok(Registers {
            ip: ptr::null(),
            w: ptr::null(),
            data_stack: reserved_stack(config.data_stack_cells)?,
            return_stack: reserved_stack(config.return_stack_cells)?,
            data_stack_limit: config.data_stack_cells,
            return_stack_limit: config.return_stack_cells,
            dictionary: Dictionary::new(config.dictionary_cells)?,
            compiler: Compiler::default(),
            output,
            input: InputSource::default(),
            error: None,
        })
    }

    /// Records an error, unless one is already set, so that the first error wins. The inner
    /// interpreter stops, and the outer interpreter aborts.
    pub(crate) fn set_error(&mut self, error: ForthError) {
        if self.error.is_none() {
            self.error = Some(error);
        }
    }

    /// Pushes a Cell, flagging an error and leaving the stack alone when it is full. Everything
    /// outside of tests pushes through here, so that the limit holds.
    pub(crate) fn push(&mut self, value: Cell) {
        if self.data_stack.len() < self.data_stack_limit {
            self.data_stack.push(value);
        } else {
            self.set_error(ForthError::DataStackOverflow);
        }
    }

    /// Pushes a return address, flagging an error and leaving the stack alone when it is full.
    /// Everything outside of tests pushes through here, so that the limit holds.
    pub(crate) fn push_return(&mut self, address: *const Cell) {
        if self.return_stack.len() < self.return_stack_limit {
            self.return_stack.push(address);
        } else {
            self.set_error(ForthError::ReturnStackOverflow);
        }
    }

    /// Pops a Cell, flagging an error when the stack is empty.
    pub(crate) fn pop(&mut self) -> Option<Cell> {
        let value = self.data_stack.pop();
        if value.is_none() {
            self.set_error(ForthError::DataStackUnderflow);
        }
        value
    }

    /// Returns whether the stack holds at least `count` Cells, flagging an error when it doesn't.
    pub(crate) fn require(&mut self, count: usize) -> bool {
        let enough = self.data_stack.len() >= count;
        if !enough {
            self.set_error(ForthError::DataStackUnderflow);
        }
        enough
    }

    /// Reads `n` from the top of the stack without popping it, and returns the position of the
    /// `n`th Cell below it, counting from 0. A negative `n` is a huge Cell, so it is out of range.
    pub(crate) fn indexed_cell(&mut self) -> Option<usize> {
        if !self.require(1) {
            return None;
        }
        let depth = self.data_stack.len() - 1;
        let n = self.data_stack[depth];
        if n >= depth {
            self.set_error(ForthError::DataStackUnderflow);
            return None;
        }
        Some(depth - 1 - n)
    }

    /// Turns a failed Result into a VM error.
    pub(crate) fn check<T>(&mut self, result: Result<T, ForthError>) -> Option<T> {
        match result {
            Ok(value) => Some(value),
            Err(error) => {
                self.set_error(error);
                None
            }
        }
    }
}

/// Converts a primitive into the raw Cell stored in a Code Field.
pub(crate) fn code_cell(code: PrimitiveFn) -> Cell {
    code as *const () as Cell
}

// --- THE INNER INTERPRETER (NEXT) ---

/// The heart of the Forth engine. Fetches the CFA, increments IP, and jumps to the code.
/// A zero Cell in the thread marks its end, and leaves W null.
///
/// # Safety
///
/// IP must point into a thread that ends in a zero Cell, or that an `EXIT` leaves first.
#[inline(always)]
unsafe fn next(regs: &mut Registers) {
    // W = *IP (Load the address of the current CFA Cell into the W register)
    regs.w = *regs.ip as *const Cell;
    if regs.w.is_null() {
        return;
    }

    // IP++ (Step to the next Cell in the currently executing thread)
    regs.ip = regs.ip.add(1);

    enter_word(regs);
}

/// `DODOES`: Runs a word changed by DOES>. Its Code Field holds the address of its action
/// thread, so unlike the other primitives that drive threaded code, this one is called
/// directly by [`enter_word`].
unsafe fn dodoes(regs: &mut Registers) {
    // 1. Push the address of the word's data onto the stack
    regs.push(pfa_of(regs.w) as Cell);

    // 2. Save the current IP onto the Return Stack
    regs.push_return(regs.ip);

    // 3. Jump with the IP to the Forth action code stored behind DOES>
    regs.ip = *regs.w as *const Cell;
}

/// Runs the word that W points to.
///
/// # Safety
///
/// W must point to a Code Field, which must hold the Cell of a primitive, or the address of
/// an action thread that DOES> stored there. Zero gives [`ForthError::NullPointerCheck`].
#[inline(always)]
pub(crate) unsafe fn enter_word(regs: &mut Registers) {
    // Fetch the contents of the CFA Cell (the raw bits of the machine-code pointer)
    let code_bits = *regs.w;

    // A null pointer is undefined behavior, and `!` can write zero here.
    if code_bits == 0 {
        regs.set_error(ForthError::NullPointerCheck(regs.w as Cell));
        return;
    }

    // Primitives are compiled into the binary, so their address is never in the dictionary,
    // and DOES> only stores addresses of threads, which always are.
    if regs.dictionary.is_in_arena(code_bits) {
        dodoes(regs);
        return;
    }

    // Blindly transmute the raw bits into an executable Rust function (like a hardware jump)
    let code = mem::transmute::<*const (), PrimitiveFn>(code_bits as *const ());
    code(regs);
}

/// Runs NEXT until the thread ends or a primitive reports an error.
///
/// # Safety
///
/// IP must meet the requirements of [`next`].
pub(crate) unsafe fn run(regs: &mut Registers) {
    loop {
        next(regs);
        if regs.w.is_null() || regs.error.is_some() {
            return;
        }
    }
}

/// Executes a single word from the text interpreter.
///
/// # Safety
///
/// `cfa` must point to the Code Field of a word in the dictionary.
pub(crate) unsafe fn execute(regs: &mut Registers, cfa: *mut Cell) {
    // To safely execute this single binary CFA via NEXT, we construct a mini 'trampoline' thread in RAM:
    // [ Target CFA Address ] [ Stop / End Marker (0) ]
    let trampoline: [Cell; 2] = [cfa as Cell, 0];
    regs.ip = trampoline.as_ptr();

    // Kickstart the Inner Interpreter engine
    run(regs);

    regs.ip = ptr::null(); // The trampoline is about to go out of scope
}

#[cfg(test)]
mod tests {
    use std::io;

    use super::*;
    use crate::dictionary::cfa_of;
    use crate::{boot, outer_interpreter, VmConfig};

    const TEST_CELLS: usize = 1024;

    // Boots a VM with these stack limits, and a dictionary large enough for every test.
    unsafe fn limited_vm(data_stack_cells: usize, return_stack_cells: usize) -> Registers {
        let config = VmConfig {
            data_stack_cells,
            return_stack_cells,
            ..VmConfig::new(TEST_CELLS)
        };
        boot(io::sink(), config).unwrap()
    }

    #[test]
    fn new_uses_the_default_stack_limits() {
        assert_eq!(
            VmConfig::new(TEST_CELLS),
            VmConfig {
                dictionary_cells: TEST_CELLS,
                data_stack_cells: DEFAULT_DATA_STACK_CELLS,
                return_stack_cells: DEFAULT_RETURN_STACK_CELLS,
            }
        );
    }

    #[test]
    fn stacks_are_reserved_up_front() {
        unsafe {
            let regs = limited_vm(3, 2);
            assert!(regs.data_stack.capacity() >= 3);
            assert!(regs.return_stack.capacity() >= 2);
        }
    }

    #[test]
    fn push_fills_the_data_stack_up_to_its_limit() {
        unsafe {
            let mut regs = limited_vm(2, 2);
            regs.push(1);
            regs.push(2);
            assert!(regs.error.is_none(), "got: {:?}", regs.error);
            assert_eq!(regs.data_stack, [1, 2]);
        }
    }

    #[test]
    fn push_past_the_limit_is_an_error_and_keeps_the_data_stack() {
        unsafe {
            let mut regs = limited_vm(2, 2);
            regs.push(1);
            regs.push(2);
            regs.push(3);
            assert!(matches!(regs.error, Some(ForthError::DataStackOverflow)));
            assert_eq!(regs.data_stack, [1, 2]);
        }
    }

    #[test]
    fn push_return_fills_the_return_stack_up_to_its_limit() {
        unsafe {
            let mut regs = limited_vm(2, 2);
            let thread: [Cell; 2] = [0, 0];
            regs.push_return(thread.as_ptr());
            regs.push_return(thread.as_ptr().add(1));
            assert!(regs.error.is_none(), "got: {:?}", regs.error);
            assert_eq!(regs.return_stack, [thread.as_ptr(), thread.as_ptr().add(1)]);
        }
    }

    #[test]
    fn push_return_past_the_limit_is_an_error_and_keeps_the_return_stack() {
        unsafe {
            let mut regs = limited_vm(2, 2);
            let thread: [Cell; 3] = [0, 0, 0];
            regs.push_return(thread.as_ptr());
            regs.push_return(thread.as_ptr().add(1));
            regs.push_return(thread.as_ptr().add(2));
            assert!(matches!(regs.error, Some(ForthError::ReturnStackOverflow)));
            assert_eq!(regs.return_stack, [thread.as_ptr(), thread.as_ptr().add(1)]);
        }
    }

    #[test]
    fn a_word_changed_by_does_needs_room_on_both_stacks() {
        unsafe {
            let mut regs = limited_vm(8, 8);
            outer_interpreter(&mut regs, ": D CREATE DOES> ; D X").unwrap();
            let cfa = cfa_of(regs.dictionary.find_word("X").unwrap());
            let thread: [Cell; 1] = [0];

            regs.data_stack_limit = 0;
            regs.w = cfa;
            regs.ip = thread.as_ptr();
            enter_word(&mut regs);
            assert!(matches!(regs.error, Some(ForthError::DataStackOverflow)));

            regs.error = None;
            regs.return_stack.clear();
            regs.data_stack_limit = 8;
            regs.return_stack_limit = 0;
            regs.w = cfa;
            regs.ip = thread.as_ptr();
            enter_word(&mut regs);
            assert!(matches!(regs.error, Some(ForthError::ReturnStackOverflow)));
        }
    }

    #[test]
    fn a_word_changed_by_does_pushes_its_pfa_and_jumps_to_its_action() {
        unsafe {
            let mut regs = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
            outer_interpreter(&mut regs, ": D CREATE DOES> ; D X").unwrap();
            let cfa = cfa_of(regs.dictionary.find_word("X").unwrap());
            let thread: [Cell; 1] = [0];

            regs.w = cfa;
            regs.ip = thread.as_ptr();
            enter_word(&mut regs);

            assert!(regs.error.is_none(), "got: {:?}", regs.error);
            assert_eq!(regs.data_stack, [pfa_of(cfa) as Cell]);
            assert_eq!(regs.return_stack, [thread.as_ptr()]);
            assert_eq!(regs.ip, *cfa as *const Cell);
        }
    }

    #[test]
    fn a_zero_code_field_is_an_error() {
        unsafe {
            let mut regs = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
            outer_interpreter(&mut regs, "CREATE X").unwrap();
            let cfa = cfa_of(regs.dictionary.find_word("X").unwrap());
            *cfa = 0;
            regs.data_stack.push(7);

            regs.w = cfa;
            enter_word(&mut regs);

            assert!(matches!(
                regs.error,
                Some(ForthError::NullPointerCheck(xt)) if xt == cfa as Cell
            ));
            assert_eq!(regs.data_stack, [7]);
            assert!(regs.return_stack.is_empty());
        }
    }
}
