use std::io::Write;
use std::mem;
use std::ptr;

use crate::cell::Cell;
use crate::compiler::Compiler;
use crate::dictionary::Dictionary;
use crate::input::InputSource;

// Error reported when the output writer fails.
pub(crate) const OUTPUT_ERROR: &str = "Output error.";

// Definition of a Forth primitive (machine-code function).
// Primitives return to the `run` loop instead of calling NEXT themselves, so
// threaded code never grows the Rust call stack.
pub(crate) type PrimitiveFn = unsafe fn(registers: &mut Registers);

/// The state of a Forth VM. Create one with [`boot`](crate::boot).
//
// CPU Registers and Stacks conforming to the Forth hardware architecture
pub struct Registers {
    pub(crate) ip: *const Cell, // Instruction Pointer: points to a Cell in a thread
    pub(crate) w: *const Cell,  // Word Register: points to the currently active CFA Cell
    pub(crate) data_stack: Vec<Cell>, // Parameter stack (contains raw Cells)
    pub(crate) return_stack: Vec<*const Cell>, // Return stack (R-stack) for nested IP pointers
    pub(crate) dictionary: Dictionary, // Word storage, HERE and the head of the word list

    pub(crate) compiler: Compiler,     // Compilation state
    pub(crate) output: Box<dyn Write>, // Where Forth words write their output
    pub(crate) input: InputSource,     // The text being interpreted
    pub(crate) error: Option<String>,  // Set by a primitive to stop the VM
}

impl Registers {
    // Creates a VM with an empty dictionary.
    pub(crate) unsafe fn new(output: Box<dyn Write>) -> Registers {
        Registers {
            ip: ptr::null(),
            w: ptr::null(),
            data_stack: Vec::new(),
            return_stack: Vec::new(),
            dictionary: Dictionary::new(),
            compiler: Compiler::default(),
            output,
            input: InputSource::default(),
            error: None,
        }
    }

    // Records an error. The inner interpreter stops, and the outer interpreter aborts.
    pub(crate) fn set_error(&mut self, message: impl Into<String>) {
        if self.error.is_none() {
            self.error = Some(message.into());
        }
    }

    // Pops a Cell, flagging an error when the stack is empty.
    pub(crate) fn pop(&mut self) -> Option<Cell> {
        let value = self.data_stack.pop();
        if value.is_none() {
            self.set_error("Data stack underflow.");
        }
        value
    }

    // Turns a failed Result into a VM error.
    pub(crate) fn check<T>(&mut self, result: Result<T, &'static str>) -> Option<T> {
        match result {
            Ok(value) => Some(value),
            Err(message) => {
                self.set_error(message);
                None
            }
        }
    }
}

// Converts a primitive into the raw Cell stored in a Code Field.
pub(crate) fn code_cell(code: PrimitiveFn) -> Cell {
    code as *const () as Cell
}

// --- THE INNER INTERPRETER (NEXT) ---

// The heart of the Forth engine. Fetches the CFA, increments IP, and jumps to the code.
// A zero Cell in the thread marks its end, and leaves W null.
#[inline(always)]
unsafe fn next(regs: &mut Registers) {
    // W = *IP (Load the address of the current CFA Cell into the W register)
    regs.w = *regs.ip as *const Cell;
    if regs.w.is_null() {
        return;
    }

    // IP++ (Step to the next Cell in the currently executing thread)
    regs.ip = regs.ip.add(1);

    // Fetch the contents of the CFA Cell (the raw bits of the machine-code pointer)
    let code_bits = *regs.w as *const ();

    // Blindly transmute the raw bits into an executable Rust function (like a hardware jump)
    let code = mem::transmute::<*const (), PrimitiveFn>(code_bits);
    code(regs);
}

// Runs NEXT until the thread ends or a primitive reports an error.
pub(crate) unsafe fn run(regs: &mut Registers) {
    loop {
        next(regs);
        if regs.w.is_null() || regs.error.is_some() {
            return;
        }
    }
}

// Executes a single word from the text interpreter.
pub(crate) unsafe fn execute(regs: &mut Registers, cfa: *mut Cell) {
    // To safely execute this single binary CFA via NEXT, we construct a mini 'trampoline' thread in RAM:
    // [ Target CFA Address ] [ Stop / End Marker (0) ]
    let trampoline: [Cell; 2] = [cfa as Cell, 0];
    regs.ip = trampoline.as_ptr();

    // Kickstart the Inner Interpreter engine
    run(regs);

    regs.ip = ptr::null(); // The trampoline is about to go out of scope
}
