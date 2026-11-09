// Copyright (c) 2026 M.W.R. Hoiting

//! The state of the VM, and the inner interpreter that runs threaded code.

use std::io::Write;
use std::mem;

use crate::cell::Cell;
use crate::compiler::Compiler;
use crate::dictionary::{Code, Dictionary};
use crate::error::ForthError;
use crate::input::InputSource;

/// Definition of a Forth primitive. Primitives return to the [`run`] loop instead of calling
/// NEXT themselves, so threaded code never grows the Rust call stack.
///
/// A primitive reports failure with [`Vm::set_error`], and leaves the VM to stop. `IP` is the
/// address of the rest of the running thread, or [`NO_IP`] when the text interpreter runs the
/// primitive directly.
pub(crate) type PrimitiveFn = fn(vm: &mut Vm);

/// The value of `IP` when no thread is running. It is never a Cell-aligned address in the
/// arena. The text interpreter puts it on the return stack below the first colon word, so that
/// the `EXIT` of that word stops [`run`].
pub(crate) const NO_IP: Cell = Cell::MAX;

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
/// let mut vm = boot(io::sink(), config)?;
/// assert!(matches!(
///     outer_interpreter(&mut vm, "1 2 3"),
///     Err(ForthError::DataStackOverflow)
/// ));
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
// CPU Vm and Stacks conforming to the Forth hardware architecture
pub struct Vm {
    /// Instruction Pointer: the address of the next Cell in the running thread, or [`NO_IP`].
    pub(crate) ip: Cell,
    /// Parameter stack (contains raw Cells).
    pub(crate) data_stack: Vec<Cell>,
    /// Return stack (R-stack) for nested IP addresses.
    pub(crate) return_stack: Vec<Cell>,
    /// The most Cells that the data stack can hold.
    pub(crate) data_stack_limit: usize,
    /// The most Cells that the return stack can hold.
    pub(crate) return_stack_limit: usize,
    /// Word storage, HERE, the head of the word list and the XT table.
    pub(crate) dictionary: Dictionary<PrimitiveFn>,

    /// Compilation state.
    pub(crate) compiler: Compiler,
    /// Where Forth words write their output.
    pub(crate) output: Box<dyn Write>,
    /// The text being interpreted.
    pub(crate) input: InputSource,
    /// Set by a primitive to stop the VM.
    pub(crate) error: Option<ForthError>,
}

impl Vm {
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
    /// let mut vm = boot(io::sink(), VmConfig::new(1024))?;
    /// assert!(!vm.is_compiling());
    /// outer_interpreter(&mut vm, ": THREE 3")?;
    /// assert!(vm.is_compiling());
    /// outer_interpreter(&mut vm, ";")?;
    /// assert!(!vm.is_compiling());
    /// # Ok::<(), ForthError>(())
    /// ```
    pub fn is_compiling(&self) -> bool {
        self.compiler.is_compiling()
    }

    /// Returns the sorted names of the words that Forth input can find, each one once. Hidden
    /// words and an open definition aren't there. A name that `C!` overwrote can come back
    /// with replacement characters, and a header that `!` broke can hide older words.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::io;
    ///
    /// use rust_forth_vm::{boot, outer_interpreter, ForthError, VmConfig};
    ///
    /// let mut vm = boot(io::sink(), VmConfig::new(1024))?;
    /// outer_interpreter(&mut vm, ": THREE 3 ;")?;
    /// assert!(vm.word_names().iter().any(|name| name == "THREE"));
    /// # Ok::<(), ForthError>(())
    /// ```
    pub fn word_names(&self) -> Vec<String> {
        self.dictionary.word_names()
    }

    /// Creates a VM with an empty dictionary of `dictionary_cells` Cells, without primitives.
    ///
    /// # Errors
    ///
    /// Returns [`ForthError::DictionaryTooLarge`] if the dictionary can't be allocated, and
    /// [`ForthError::StackTooLarge`] if a stack can't be.
    pub(crate) fn new(output: Box<dyn Write>, config: VmConfig) -> Result<Vm, ForthError> {
        Ok(Vm {
            ip: NO_IP,
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

    /// Adds a primitive to the dictionary under `name`, and returns its execution token.
    ///
    /// # Errors
    ///
    /// See [`Dictionary::write_header`].
    pub(crate) fn add_primitive(
        &mut self,
        name: &str,
        code: PrimitiveFn,
        flags: Cell,
    ) -> Result<Cell, ForthError> {
        self.dictionary
            .write_header(name, Code::Primitive(code), flags)
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
    pub(crate) fn push_return(&mut self, address: Cell) {
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

// --- THE INNER INTERPRETER (NEXT) ---

/// Runs the word with this execution token, from a thread or from `EXECUTE`. An `xt` that isn't
/// in the XT table gives [`ForthError::InvalidExecutionToken`]. Unlike `EXECUTE`, this doesn't
/// refuse words that are pending or compile-only, because a thread may call those.
#[inline(always)]
pub(crate) fn execute_xt(vm: &mut Vm, xt: Cell) {
    let Some(entry) = vm.dictionary.entry(xt) else {
        vm.set_error(ForthError::InvalidExecutionToken(xt));
        return;
    };
    match entry.code {
        Code::Primitive(primitive) => primitive(vm),
        // DOCOL: save the return address, and run the thread at the body.
        Code::Colon => {
            vm.push_return(vm.ip);
            vm.ip = entry.body;
        }
        // DOVAR: push the address of the body.
        Code::Variable => vm.push(entry.body),
        // DOCON: push the Cell at the body.
        Code::Constant => {
            let result = vm.dictionary.fetch(entry.body);
            if let Some(value) = vm.check(result) {
                vm.push(value);
            }
        }
        // DODOES: push the address of the body, and run the action thread behind DOES>.
        Code::Does(action) => {
            vm.push(entry.body);
            vm.push_return(vm.ip);
            vm.ip = action;
        }
    }
}

/// Runs NEXT until no thread is running, or a word reports an error. NEXT is the heart of the
/// Forth engine: it fetches the execution token at IP, moves IP to the next Cell, and runs the
/// word.
pub(crate) fn run(vm: &mut Vm) {
    while vm.error.is_none() {
        let Some(xt) = vm.dictionary.fetch_thread(vm.ip) else {
            // [`NO_IP`] ends the run. Any other IP that can't be read is an error, which a
            // full fetch names.
            if vm.ip != NO_IP {
                let result = vm.dictionary.fetch(vm.ip);
                vm.check(result);
            }
            return;
        };
        // The fetch succeeded, so the Cell ends inside the arena and this can't overflow.
        vm.ip += mem::size_of::<Cell>();
        execute_xt(vm, xt);
    }
}

/// Runs a single word for the text interpreter. A colon word saves [`NO_IP`] as its return
/// address, so that its `EXIT` ends the run. This differs from the `EXECUTE` word, which enters
/// a word directly from the thread that called it.
pub(crate) fn execute_word(vm: &mut Vm, xt: Cell) {
    vm.ip = NO_IP;
    execute_xt(vm, xt);
    run(vm);
    vm.ip = NO_IP; // An error can leave IP inside a thread
}

#[cfg(test)]
mod tests {
    use std::io;

    use super::*;
    use crate::{boot, outer_interpreter, VmConfig};

    const TEST_CELLS: usize = 64 * 1024;
    const CELL_SIZE: usize = mem::size_of::<Cell>();

    // Boots a VM with these stack limits, and a dictionary large enough for every test.
    fn limited_vm(data_stack_cells: usize, return_stack_cells: usize) -> Vm {
        let config = VmConfig {
            data_stack_cells,
            return_stack_cells,
            ..VmConfig::new(TEST_CELLS)
        };
        boot(io::sink(), config).unwrap()
    }

    // Returns the execution token of a word that Forth can find.
    fn xt_of(vm: &Vm, name: &str) -> Cell {
        vm.dictionary.find_word(name).unwrap()
    }

    // Returns the body of a word that Forth can find.
    fn body_of(vm: &Vm, name: &str) -> Cell {
        vm.dictionary.entry(xt_of(vm, name)).unwrap().body
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
        let vm = limited_vm(3, 2);
        assert!(vm.data_stack.capacity() >= 3);
        assert!(vm.return_stack.capacity() >= 2);
    }

    #[test]
    fn push_fills_the_data_stack_up_to_its_limit() {
        let mut vm = limited_vm(2, 2);
        vm.push(1);
        vm.push(2);
        assert!(vm.error.is_none(), "got: {:?}", vm.error);
        assert_eq!(vm.data_stack, [1, 2]);
    }

    #[test]
    fn push_past_the_limit_is_an_error_and_keeps_the_data_stack() {
        let mut vm = limited_vm(2, 2);
        vm.push(1);
        vm.push(2);
        vm.push(3);
        assert!(matches!(vm.error, Some(ForthError::DataStackOverflow)));
        assert_eq!(vm.data_stack, [1, 2]);
    }

    #[test]
    fn push_return_fills_the_return_stack_up_to_its_limit() {
        let mut vm = limited_vm(2, 2);
        vm.push_return(8);
        vm.push_return(16);
        assert!(vm.error.is_none(), "got: {:?}", vm.error);
        assert_eq!(vm.return_stack, [8, 16]);
    }

    #[test]
    fn push_return_past_the_limit_is_an_error_and_keeps_the_return_stack() {
        let mut vm = limited_vm(2, 2);
        vm.push_return(8);
        vm.push_return(16);
        vm.push_return(24);
        assert!(matches!(vm.error, Some(ForthError::ReturnStackOverflow)));
        assert_eq!(vm.return_stack, [8, 16]);
    }

    #[test]
    fn no_ip_is_never_an_aligned_address() {
        assert_ne!(NO_IP % CELL_SIZE, 0);
    }

    #[test]
    fn a_colon_word_saves_ip_and_jumps_to_its_body() {
        let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
        outer_interpreter(&mut vm, ": X ;").unwrap();
        vm.ip = 40;

        let xt = xt_of(&vm, "X");

        execute_xt(&mut vm, xt);
        assert!(vm.error.is_none(), "got: {:?}", vm.error);
        assert!(vm.data_stack.is_empty());
        assert_eq!(vm.return_stack, [40]);
        assert_eq!(vm.ip, body_of(&vm, "X"));
    }

    #[test]
    fn a_created_word_pushes_its_body() {
        let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
        outer_interpreter(&mut vm, "CREATE X").unwrap();
        vm.ip = 40;

        let xt = xt_of(&vm, "X");

        execute_xt(&mut vm, xt);
        assert!(vm.error.is_none(), "got: {:?}", vm.error);
        assert_eq!(vm.data_stack, [body_of(&vm, "X")]);
        assert!(vm.return_stack.is_empty());
        assert_eq!(vm.ip, 40);
    }

    #[test]
    fn a_constant_pushes_the_cell_at_its_body() {
        let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
        outer_interpreter(&mut vm, "42 CONSTANT X 99 ,").unwrap();
        vm.data_stack.push(1);

        let xt = xt_of(&vm, "X");

        execute_xt(&mut vm, xt);
        assert!(vm.error.is_none(), "got: {:?}", vm.error);
        assert_eq!(vm.data_stack, [1, 42]);
    }

    #[test]
    fn a_constant_without_its_value_is_an_error() {
        let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
        let xt = vm.dictionary.write_header("X", Code::Constant, 0).unwrap();
        let body = vm.dictionary.entry(xt).unwrap().body;

        execute_xt(&mut vm, xt);
        assert!(matches!(
            vm.error,
            Some(ForthError::UnwrittenAddress(addr)) if addr == body
        ));
        assert!(vm.data_stack.is_empty());
    }

    #[test]
    fn a_word_changed_by_does_needs_room_on_both_stacks() {
        let mut vm = limited_vm(8, 8);
        outer_interpreter(&mut vm, ": D CREATE DOES> ; D X").unwrap();
        let xt = xt_of(&vm, "X");

        vm.data_stack_limit = 0;
        execute_xt(&mut vm, xt);
        assert!(matches!(vm.error, Some(ForthError::DataStackOverflow)));

        vm.error = None;
        vm.return_stack.clear();
        vm.data_stack_limit = 8;
        vm.return_stack_limit = 0;
        execute_xt(&mut vm, xt);
        assert!(matches!(vm.error, Some(ForthError::ReturnStackOverflow)));
    }

    #[test]
    fn a_word_changed_by_does_pushes_its_body_and_jumps_to_its_action() {
        let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
        outer_interpreter(&mut vm, ": D CREATE DOES> ; D X").unwrap();
        let Code::Does(action) = vm.dictionary.entry(xt_of(&vm, "X")).unwrap().code else {
            panic!("X has no DOES> action");
        };
        vm.ip = 40;

        let xt = xt_of(&vm, "X");

        execute_xt(&mut vm, xt);
        assert!(vm.error.is_none(), "got: {:?}", vm.error);
        assert_eq!(vm.data_stack, [body_of(&vm, "X")]);
        assert_eq!(vm.return_stack, [40]);
        assert_eq!(vm.ip, action);
        // The action thread follows `DOES>` in the thread of D.
        assert_eq!(action, body_of(&vm, "D") + 2 * CELL_SIZE);
    }

    #[test]
    fn an_execution_token_past_the_table_is_an_error() {
        let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
        vm.data_stack.push(7);
        let xt = vm.dictionary.last_xt().unwrap() + 1;

        execute_xt(&mut vm, xt);
        assert!(matches!(
            vm.error,
            Some(ForthError::InvalidExecutionToken(bad)) if bad == xt
        ));
        assert_eq!(vm.data_stack, [7]);
        assert!(vm.return_stack.is_empty());
    }

    #[test]
    fn next_reports_an_ip_outside_the_dictionary() {
        let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
        let ip = Cell::MAX - CELL_SIZE + 1;
        vm.ip = ip;

        run(&mut vm);
        assert!(matches!(
            vm.error,
            Some(ForthError::AddressOutOfBounds(addr)) if addr == ip
        ));
    }

    #[test]
    fn run_stops_at_no_ip_without_an_error() {
        let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
        vm.ip = NO_IP;

        run(&mut vm);
        assert!(vm.error.is_none(), "got: {:?}", vm.error);
    }

    #[test]
    fn execute_word_runs_a_colon_word_to_its_end() {
        let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
        outer_interpreter(&mut vm, ": X 1 2 ;").unwrap();

        let xt = xt_of(&vm, "X");

        execute_word(&mut vm, xt);
        assert!(vm.error.is_none(), "got: {:?}", vm.error);
        assert_eq!(vm.data_stack, [1, 2]);
        assert!(vm.return_stack.is_empty());
        assert_eq!(vm.ip, NO_IP);
    }

    #[test]
    fn execute_word_leaves_no_ip_after_an_error() {
        let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
        outer_interpreter(&mut vm, ": X DROP ;").unwrap();

        let xt = xt_of(&vm, "X");

        execute_word(&mut vm, xt);
        assert!(matches!(vm.error, Some(ForthError::DataStackUnderflow)));
        assert_eq!(vm.ip, NO_IP);
    }

    #[test]
    fn add_primitive_gives_consecutive_execution_tokens() {
        let mut vm = Vm::new(Box::new(io::sink()), VmConfig::new(TEST_CELLS)).unwrap();
        fn push_seven(vm: &mut Vm) {
            vm.push(7);
        }
        let first = vm.add_primitive("A", push_seven, 0).unwrap();
        let second = vm.add_primitive("B", push_seven, 0).unwrap();
        assert_eq!((first, second), (0, 1));

        // Running the word shows which code it has. Function addresses can't show it, because
        // Rust doesn't guarantee that they compare equal.
        execute_xt(&mut vm, second);
        assert!(vm.error.is_none(), "got: {:?}", vm.error);
        assert_eq!(vm.data_stack, [7]);
    }
}
