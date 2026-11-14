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
/// A primitive reports failure with [`Vm::set_error`], and leaves the VM to stop.
///
/// # Safety
///
/// Every primitive shares this contract, so they don't repeat it. `vm` must be a booted VM,
/// with `IP` pointing to the rest of the running thread. Primitives that use `W` also need it
/// to point to the Code Field of their word, which [`enter_word`] arranges.
pub(crate) type PrimitiveFn = unsafe fn(vm: &mut Vm);

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
///     let mut vm = boot(io::sink(), config)?;
///     assert!(matches!(
///         outer_interpreter(&mut vm, "1 2 3"),
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

/// The size of each transient buffer that interpreted `S"` copies its text into.
pub(crate) const STRING_BUFFER_BYTES: usize = 256;

/// Two transient buffers in the dictionary, below HERE, for the text of interpreted `S"`. A
/// string goes into the next buffer, so two strings in a row are both still valid. A third
/// string reuses the first buffer.
#[derive(Default)]
pub(crate) struct StringBuffers {
    /// The addresses of the two buffers, which are set when the VM boots.
    addresses: [Cell; 2],
    /// The index in `addresses` of the buffer for the next string.
    next: usize,
}

impl StringBuffers {
    /// Reserves both buffers at HERE, and leaves HERE cell-aligned after them.
    ///
    /// # Errors
    ///
    /// Returns [`ForthError::DictionaryOverflow`] if the buffers don't fit in the dictionary.
    ///
    /// # Safety
    ///
    /// `dictionary` must come from [`Vm::new`]. The buffers are reserved with a positive
    /// [`Dictionary::allot`], which only moves HERE, so this must run before any word is
    /// defined, which puts the buffers below every word.
    pub(crate) unsafe fn reserve(&mut self, dictionary: &mut Dictionary) -> Result<(), ForthError> {
        for address in &mut self.addresses {
            *address = dictionary.here();
            dictionary.allot(STRING_BUFFER_BYTES as isize)?;
        }
        dictionary.align();
        Ok(())
    }

    /// Returns the address of the buffer for the next string, and moves on to the other one.
    pub(crate) fn take(&mut self) -> Cell {
        let address = self.addresses[self.next];
        self.next = (self.next + 1) % self.addresses.len();
        address
    }
}

/// The state of a Forth VM. Create one with [`boot`](crate::boot), and evaluate Forth with
/// [`outer_interpreter`](crate::outer_interpreter).
///
/// This is the whole VM, not only its registers: it owns both stacks, the dictionary, the
/// input and the output. Its contents are private, so the data stack can only be seen
/// through Forth words such as `.` and `DEPTH`. It keeps its state between calls.
//
// CPU Vm and Stacks conforming to the Forth hardware architecture
pub struct Vm {
    /// Host permissions and active MMIO registrations.
    pub(crate) mmio: crate::mmio::Mmio,
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
    /// The transient buffers for the text of interpreted `S"`.
    pub(crate) string_buffers: StringBuffers,
    /// Set by a primitive to stop the VM.
    pub(crate) error: Option<ForthError>,
}

impl Vm {
    /// Authorizes an already mapped MMIO range before the first Forth registration.
    /// An initially booted VM permits no MMIO. Active ranges are registered from Forth.
    ///
    /// # Errors
    /// Returns an MMIO error for invalid masks, empty/overflowing or overlapping ranges,
    /// locked permissions, allocation failure, or overlap with the dictionary arena.
    ///
    /// # Safety
    /// The pointer must remain valid for the VM's lifetime for all permitted access widths
    /// and rights. Host-owned memory must retain provenance and obey Rust aliasing rules;
    /// device accesses must not trap or invalidate Rust memory. The host must provide any
    /// hardware barriers, synchronization and mapping, and prevent concurrent conflicting
    /// access. Reads must produce initialized integer values. Volatile is not atomic.
    ///
    /// # Examples
    /// ```
    /// use rust_forth_vm::{boot, outer_interpreter, MmioRegion, VmConfig, MMIO_READ, MMIO_WIDTH_8};
    /// let mut registers = Box::new([42u8]);
    /// // SAFETY: registers remain alive and exclusively available throughout VM execution.
    /// unsafe {
    ///     let mut vm = boot(std::io::sink(), VmConfig::new(4096))?;
    ///     let address = registers.as_mut_ptr();
    ///     vm.allow_mmio(MmioRegion { address, bytes: 1, rights: MMIO_READ, widths: MMIO_WIDTH_8 })?;
    ///     outer_interpreter(&mut vm, &format!("{} 1 MMIO_READ MMIO_WIDTH_8 MMIO-REGISTER DROP", address as usize))?;
    ///     outer_interpreter(&mut vm, &format!("{} 1 MMIO@ DROP", address as usize))?;
    /// }
    /// # Ok::<(), rust_forth_vm::ForthError>(())
    /// ```
    pub unsafe fn allow_mmio(&mut self, region: crate::MmioRegion) -> Result<(), ForthError> {
        if self
            .dictionary
            .overlaps_arena(region.address as Cell, region.bytes)
        {
            return Err(ForthError::MmioInvalidRegion);
        }
        self.mmio.allow(region)
    }

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
    ///     let mut vm = boot(io::sink(), VmConfig::new(1024))?;
    ///     assert!(!vm.is_compiling());
    ///     outer_interpreter(&mut vm, ": THREE 3")?;
    ///     assert!(vm.is_compiling());
    ///     outer_interpreter(&mut vm, ";")?;
    ///     assert!(!vm.is_compiling());
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
    /// use rust_forth_vm::{boot, outer_interpreter, ForthError, VmConfig};
    ///
    /// // SAFETY: this VM is only used through the crate's functions.
    /// unsafe {
    ///     let mut vm = boot(io::sink(), VmConfig::new(1024)).unwrap();
    ///     outer_interpreter(&mut vm, ": THREE 3 ;")?;
    ///     assert!(vm.word_names().iter().any(|name| name == "THREE"));
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
    /// Returns [`ForthError::DictionaryTooLarge`] if the dictionary can't be allocated, and
    /// [`ForthError::StackTooLarge`] if a stack can't be.
    pub(crate) unsafe fn new(output: Box<dyn Write>, config: VmConfig) -> Result<Vm, ForthError> {
        Ok(Vm {
            mmio: crate::mmio::Mmio::default(),
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
            string_buffers: StringBuffers::default(),
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
unsafe fn next(vm: &mut Vm) {
    // W = *IP (Load the address of the current CFA Cell into the W register)
    vm.w = *vm.ip as *const Cell;
    if vm.w.is_null() {
        return;
    }

    // IP++ (Step to the next Cell in the currently executing thread)
    vm.ip = vm.ip.add(1);

    enter_word(vm);
}

/// `DODOES`: Runs a word changed by DOES>. Its Code Field holds the address of its action
/// thread, so unlike the other primitives that drive threaded code, this one is called
/// directly by [`enter_word`].
unsafe fn dodoes(vm: &mut Vm) {
    // 1. Push the address of the word's data onto the stack
    vm.push(pfa_of(vm.w) as Cell);

    // 2. Save the current IP onto the Return Stack
    vm.push_return(vm.ip);

    // 3. Jump with the IP to the Forth action code stored behind DOES>
    vm.ip = *vm.w as *const Cell;
}

/// Runs the word that W points to.
///
/// # Safety
///
/// W must point to a Code Field, which must hold the Cell of a primitive, or the address of
/// an action thread that DOES> stored there. Zero gives [`ForthError::NullCodeField`].
#[inline(always)]
pub(crate) unsafe fn enter_word(vm: &mut Vm) {
    // Fetch the contents of the CFA Cell (the raw bits of the machine-code pointer)
    let code_bits = *vm.w;

    // A null pointer is undefined behavior, and `!` can write zero here.
    if code_bits == 0 {
        vm.set_error(ForthError::NullCodeField(vm.w as Cell));
        return;
    }

    // Primitives are compiled into the binary, so their address is never in the dictionary,
    // and DOES> only stores addresses of threads, which always are.
    if vm.dictionary.is_in_arena(code_bits) {
        dodoes(vm);
        return;
    }

    // Blindly transmute the raw bits into an executable Rust function (like a hardware jump)
    let code = mem::transmute::<*const (), PrimitiveFn>(code_bits as *const ());
    code(vm);
}

/// Runs NEXT until the thread ends or a primitive reports an error.
///
/// # Safety
///
/// IP must meet the requirements of [`next`].
pub(crate) unsafe fn run(vm: &mut Vm) {
    loop {
        next(vm);
        if vm.w.is_null() || vm.error.is_some() {
            return;
        }
    }
}

/// Runs a single word for the text interpreter, through a trampoline thread. This differs from
/// the `EXECUTE` word, which enters a word directly from the thread that called it.
///
/// # Safety
///
/// `cfa` must point to the Code Field of a word in the dictionary.
pub(crate) unsafe fn trampoline_cfa(vm: &mut Vm, cfa: *mut Cell) {
    // To safely execute this single binary CFA via NEXT, we construct a mini 'trampoline' thread in RAM:
    // [ Target CFA Address ] [ Stop / End Marker (0) ]
    let trampoline: [Cell; 2] = [cfa as Cell, 0];
    vm.ip = trampoline.as_ptr();

    // Kickstart the Inner Interpreter engine
    run(vm);

    vm.ip = ptr::null(); // The trampoline is about to go out of scope
}

#[cfg(test)]
mod tests {
    use std::io;

    use super::*;
    use crate::dictionary::cfa_of;
    use crate::{boot, outer_interpreter, VmConfig};

    const TEST_CELLS: usize = 64 * 1024;

    // Boots a VM with these stack limits, and a dictionary large enough for every test.
    unsafe fn limited_vm(data_stack_cells: usize, return_stack_cells: usize) -> Vm {
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
            let vm = limited_vm(3, 2);
            assert!(vm.data_stack.capacity() >= 3);
            assert!(vm.return_stack.capacity() >= 2);
        }
    }

    #[test]
    fn push_fills_the_data_stack_up_to_its_limit() {
        unsafe {
            let mut vm = limited_vm(2, 2);
            vm.push(1);
            vm.push(2);
            assert!(vm.error.is_none(), "got: {:?}", vm.error);
            assert_eq!(vm.data_stack, [1, 2]);
        }
    }

    #[test]
    fn push_past_the_limit_is_an_error_and_keeps_the_data_stack() {
        unsafe {
            let mut vm = limited_vm(2, 2);
            vm.push(1);
            vm.push(2);
            vm.push(3);
            assert!(matches!(vm.error, Some(ForthError::DataStackOverflow)));
            assert_eq!(vm.data_stack, [1, 2]);
        }
    }

    #[test]
    fn push_return_fills_the_return_stack_up_to_its_limit() {
        unsafe {
            let mut vm = limited_vm(2, 2);
            let thread: [Cell; 2] = [0, 0];
            vm.push_return(thread.as_ptr());
            vm.push_return(thread.as_ptr().add(1));
            assert!(vm.error.is_none(), "got: {:?}", vm.error);
            assert_eq!(vm.return_stack, [thread.as_ptr(), thread.as_ptr().add(1)]);
        }
    }

    #[test]
    fn push_return_past_the_limit_is_an_error_and_keeps_the_return_stack() {
        unsafe {
            let mut vm = limited_vm(2, 2);
            let thread: [Cell; 3] = [0, 0, 0];
            vm.push_return(thread.as_ptr());
            vm.push_return(thread.as_ptr().add(1));
            vm.push_return(thread.as_ptr().add(2));
            assert!(matches!(vm.error, Some(ForthError::ReturnStackOverflow)));
            assert_eq!(vm.return_stack, [thread.as_ptr(), thread.as_ptr().add(1)]);
        }
    }

    #[test]
    fn a_word_changed_by_does_needs_room_on_both_stacks() {
        unsafe {
            let mut vm = limited_vm(8, 8);
            outer_interpreter(&mut vm, ": D CREATE DOES> ; D X").unwrap();
            let cfa = cfa_of(vm.dictionary.find_word("X").unwrap());
            let thread: [Cell; 1] = [0];

            vm.data_stack_limit = 0;
            vm.w = cfa;
            vm.ip = thread.as_ptr();
            enter_word(&mut vm);
            assert!(matches!(vm.error, Some(ForthError::DataStackOverflow)));

            vm.error = None;
            vm.return_stack.clear();
            vm.data_stack_limit = 8;
            vm.return_stack_limit = 0;
            vm.w = cfa;
            vm.ip = thread.as_ptr();
            enter_word(&mut vm);
            assert!(matches!(vm.error, Some(ForthError::ReturnStackOverflow)));
        }
    }

    #[test]
    fn a_word_changed_by_does_pushes_its_pfa_and_jumps_to_its_action() {
        unsafe {
            let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
            outer_interpreter(&mut vm, ": D CREATE DOES> ; D X").unwrap();
            let cfa = cfa_of(vm.dictionary.find_word("X").unwrap());
            let thread: [Cell; 1] = [0];

            vm.w = cfa;
            vm.ip = thread.as_ptr();
            enter_word(&mut vm);

            assert!(vm.error.is_none(), "got: {:?}", vm.error);
            assert_eq!(vm.data_stack, [pfa_of(cfa) as Cell]);
            assert_eq!(vm.return_stack, [thread.as_ptr()]);
            assert_eq!(vm.ip, *cfa as *const Cell);
        }
    }

    #[test]
    fn a_zero_code_field_is_an_error() {
        unsafe {
            let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
            outer_interpreter(&mut vm, "CREATE X").unwrap();
            let cfa = cfa_of(vm.dictionary.find_word("X").unwrap());
            *cfa = 0;
            vm.data_stack.push(7);

            vm.w = cfa;
            enter_word(&mut vm);

            assert!(matches!(
                vm.error,
                Some(ForthError::NullCodeField(xt)) if xt == cfa as Cell
            ));
            assert_eq!(vm.data_stack, [7]);
            assert!(vm.return_stack.is_empty());
        }
    }
}
