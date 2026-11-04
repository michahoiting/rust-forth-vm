use std::mem;

// --- THE FOUNDATION: THE FORTH CELL ---

// A Cell is the universal basic unit of Forth (64-bit on a 64-bit CPU).
// It blindly holds data, flags, offsets, or raw memory pointers.
pub(crate) type Cell = usize;

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
    pub(crate) here: *mut Cell, // Dictionary pointer to the next free Cell
    pub(crate) last_word: *mut Cell, // Pointer to the LFA of the most recently created word
    pub(crate) dictionary_end: *mut Cell, // One past the last usable dictionary Cell
    pub(crate) _dictionary: Vec<Cell>, // Owns the arena that the pointers above refer to

    pub(crate) compiling: bool, // STATE: true between `:` and `;`
    pub(crate) definition: Option<(*mut Cell, *mut Cell)>, // HERE and LAST to restore if a definition fails
    pub(crate) lit_cfa: Cell,  // CFA compiled in front of inline literals
    pub(crate) exit_cfa: Cell, // CFA compiled by `;`

    pub(crate) source: String,        // The line being interpreted
    pub(crate) source_pos: usize,     // Parse offset into `source`
    pub(crate) error: Option<String>, // Set by a primitive to stop the VM
}

impl Registers {
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

    // Parses the next whitespace-delimited token from the input source.
    pub(crate) fn parse_name(&mut self) -> Option<String> {
        let rest = &self.source[self.source_pos..];
        let start = rest.find(|c: char| !c.is_whitespace())?;
        let tail = &rest[start..];
        let len = tail.find(char::is_whitespace).unwrap_or(tail.len());
        let token = tail[..len].to_string();
        self.source_pos += start + len;
        Some(token)
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
