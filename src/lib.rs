//! A clean-room Forth virtual machine.

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
/// `output`, which is flushed at the end of each evaluated line.
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
pub unsafe fn boot(
    output: impl Write + 'static,
    dictionary_cells: usize,
) -> Result<Registers, ForthError> {
    let mut regs = Registers::new(Box::new(output), dictionary_cells)?;
    primitives::install(&mut regs)?;
    Ok(regs)
}
