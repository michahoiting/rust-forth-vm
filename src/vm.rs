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
    /// Returns whether a definition is open, so that the VM is compiling rather than
    /// interpreting. A definition can continue in the next call to
    /// [`outer_interpreter`](crate::outer_interpreter).
    ///
    /// # Examples
    ///
    /// ```
    /// use std::io;
    ///
    /// use rust_forth_vm::{boot, outer_interpreter, ForthError};
    ///
    /// // SAFETY: this VM is only used through the crate's functions.
    /// unsafe {
    ///     let mut regs = boot(io::sink(), 1024)?;
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

    /// Returns the sorted names of the words that Forth input can find, each one once. Hidden
    /// words and an open definition aren't there. A name that `C!` overwrote can come back
    /// with replacement characters.
    ///
    /// # Safety
    ///
    /// Every word header must be intact, because links and names are followed as raw
    /// pointers. `!` can overwrite them.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::io;
    ///
    /// use rust_forth_vm::{boot, outer_interpreter, ForthError};
    ///
    /// // SAFETY: this VM is only used through the crate's functions.
    /// unsafe {
    ///     let mut regs = boot(io::sink(), 1024)?;
    ///     outer_interpreter(&mut regs, ": THREE 3 ;")?;
    ///     assert!(regs.word_names().iter().any(|name| name == "THREE"));
    /// }
    /// # Ok::<(), ForthError>(())
    /// ```
    pub unsafe fn word_names(&self) -> Vec<String> {
        self.dictionary.word_names()
    }

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

/// `DODOES`: Runs a word changed by DOES>. Its Code Field holds the address of its action
/// thread, so unlike the other primitives that drive threaded code, this one is called
/// directly by [`enter_word`].
unsafe fn dodoes(regs: &mut Registers) {
    // 1. Push the address of the word's data onto the stack
    regs.data_stack.push(pfa_of(regs.w) as Cell);

    // 2. Save the current IP onto the Return Stack
    regs.return_stack.push(regs.ip);

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
    use crate::{boot, outer_interpreter};

    const TEST_CELLS: usize = 1024;

    #[test]
    fn a_word_changed_by_does_pushes_its_pfa_and_jumps_to_its_action() {
        unsafe {
            let mut regs = boot(io::sink(), TEST_CELLS).unwrap();
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
            let mut regs = boot(io::sink(), TEST_CELLS).unwrap();
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
