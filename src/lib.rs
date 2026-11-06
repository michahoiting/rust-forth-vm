//! A clean-room Forth virtual machine: indirect threaded, with a linked-list dictionary and
//! no dependencies.
//!
//! [`boot`] creates a VM, and [`outer_interpreter`] evaluates Forth source in it. All the
//! state lives in [`Registers`], and carries over from one call to the next. Errors are
//! [`ForthError`]s. The stacks and the dictionary hold [`Cell`]s.
//!
//! The README has the list of words, how the VM works, and its limitations.
//!
//! # Examples
//!
//! The VM takes ownership of its output writer, so this one shares its buffer in order to
//! read it afterwards. Anything that implements [`Write`] will do, such as
//! [`std::io::stdout`].
//!
//! ```
//! use std::cell::RefCell;
//! use std::io::{self, Write};
//! use std::rc::Rc;
//!
//! use rust_forth_vm::{boot, outer_interpreter, ForthError};
//!
//! #[derive(Clone, Default)]
//! struct Output(Rc<RefCell<Vec<u8>>>);
//!
//! impl Write for Output {
//!     fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
//!         self.0.borrow_mut().write(bytes)
//!     }
//!
//!     fn flush(&mut self) -> io::Result<()> {
//!         Ok(())
//!     }
//! }
//!
//! let output = Output::default();
//!
//! // SAFETY: this VM is only used through the crate's functions.
//! unsafe {
//!     let mut regs = boot(output.clone(), 1024)?;
//!     outer_interpreter(&mut regs, ": DOUBLE DUP + ;")?;
//!     outer_interpreter(&mut regs, "21 DOUBLE .")?;
//! }
//!
//! assert_eq!(output.0.borrow().as_slice(), b"42 ");
//! # Ok::<(), ForthError>(())
//! ```
//!
//! # Safety
//!
//! [`boot`] and [`outer_interpreter`] are `unsafe` because the VM runs threaded code through
//! raw pointers. Bad Forth input normally gives a [`ForthError`], and the README lists the
//! known exceptions.

use std::io::Write;

mod cell;
mod compiler;
mod dictionary;
mod error;
mod input;
mod interpreter;
mod primitives;
mod vm;

pub use cell::Cell;
pub use error::ForthError;
pub use interpreter::outer_interpreter;
pub use vm::Registers;

/// Allocates the VM with a dictionary of `dictionary_cells` Cells, and registers the
/// core primitives. The dictionary can't grow later. Forth words write their output to
/// `output`, which is flushed at the end of each evaluated input.
///
/// # Errors
///
/// Returns [`ForthError::DictionaryTooLarge`] if the dictionary can't be allocated, and
/// [`ForthError::DictionaryOverflow`] if it is too small to hold the primitives.
///
/// # Safety
///
/// The returned VM holds raw pointers into its own dictionary. It must only be
/// driven through this crate's functions.
///
/// # Examples
///
/// ```
/// use std::io;
///
/// use rust_forth_vm::{boot, ForthError};
///
/// // SAFETY: neither VM is used for anything else.
/// unsafe {
///     assert!(boot(io::stdout(), 1024).is_ok());
///     assert!(matches!(boot(io::sink(), 1), Err(ForthError::DictionaryOverflow)));
/// }
/// ```
pub unsafe fn boot(
    output: impl Write + 'static,
    dictionary_cells: usize,
) -> Result<Registers, ForthError> {
    let mut regs = Registers::new(Box::new(output), dictionary_cells)?;
    primitives::install(&mut regs)?;
    Ok(regs)
}
