// Copyright (c) 2026 M.W.R. Hoiting

//! A clean-room Forth virtual machine: token threaded, with a linked-list dictionary and
//! no dependencies. It has no `unsafe` code.
//!
//! [`boot`] creates a VM from a [`VmConfig`], and [`outer_interpreter`] evaluates Forth source in it. All the
//! state lives in [`Vm`], and carries over from one call to the next. Errors are
//! [`ForthError`]s. The stacks and the dictionary hold [`Cell`]s.
//!
//! `docs/words.md` has the list of words. The README has how the VM works, and its
//! limitations.
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
//! use rust_forth_vm::{boot, outer_interpreter, ForthError, VmConfig};
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
//! let mut vm = boot(output.clone(), VmConfig::new(1024))?;
//! outer_interpreter(&mut vm, ": DOUBLE DUP + ;")?;
//! outer_interpreter(&mut vm, "21 DOUBLE .")?;
//!
//! assert_eq!(output.0.borrow().as_slice(), b"42 ");
//! # Ok::<(), ForthError>(())
//! ```

#![forbid(unsafe_code)]

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
pub use interpreter::{outer_interpreter, quit_loop, LineResult};
pub use primitives::COPYRIGHT;
pub use vm::{Vm, VmConfig, DEFAULT_DATA_STACK_CELLS, DEFAULT_RETURN_STACK_CELLS};

/// Allocates the VM with the sizes in `config`, and registers the core primitives. The
/// dictionary and the stacks can't grow later. Forth words write their output to `output`,
/// which is flushed at the end of each evaluated input.
///
/// # Errors
///
/// Returns [`ForthError::DictionaryTooLarge`] if the dictionary can't be allocated,
/// [`ForthError::StackTooLarge`] if a stack can't be, and
/// [`ForthError::DictionaryOverflow`] if the dictionary is too small to hold the primitives.
///
/// # Examples
///
/// ```
/// use std::io;
///
/// use rust_forth_vm::{boot, ForthError, VmConfig};
///
/// assert!(boot(io::stdout(), VmConfig::new(1024)).is_ok());
/// assert!(matches!(boot(io::sink(), VmConfig::new(1)), Err(ForthError::DictionaryOverflow)));
/// ```
pub fn boot(output: impl Write + 'static, config: VmConfig) -> Result<Vm, ForthError> {
    let mut vm = Vm::new(Box::new(output), config)?;
    primitives::install(&mut vm)?;
    Ok(vm)
}
