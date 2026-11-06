// Copyright (c) 2026 M.W.R. Hoiting

//! The state of the VM, and the inner interpreter that runs threaded code.

use std::io::Write;
use std::mem;
use std::ptr;

use crate::cell::Cell;
use crate::compiler::Compiler;
use crate::dictionary::Dictionary;
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
    /// Creates a VM with an empty dictionary of `dictionary_cells` Cells.
    ///
    /// # Errors
    ///
    /// Returns [`ForthError::DictionaryTooLarge`] if the dictionary can't be allocated.
    pub(crate) unsafe fn new(
        output: Box<dyn Write>,
        dictionary_cells: usize,
    ) -> Result<Registers, ForthError> {
        Ok(Registers {
            ip: ptr::null(),
            w: ptr::null(),
            data_stack: Vec::new(),
            return_stack: Vec::new(),
            dictionary: Dictionary::new(dictionary_cells)?,
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

/// Runs the word that W points to.
///
/// # Safety
///
/// W must point to a Code Field, which must hold the Cell of a primitive.
#[inline(always)]
pub(crate) unsafe fn enter_word(regs: &mut Registers) {
    // Fetch the contents of the CFA Cell (the raw bits of the machine-code pointer)
    let code_bits = *regs.w as *const ();

    // Blindly transmute the raw bits into an executable Rust function (like a hardware jump)
    let code = mem::transmute::<*const (), PrimitiveFn>(code_bits);
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
