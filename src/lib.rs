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

pub use error::ForthError;
pub use interpreter::outer_interpreter;
pub use vm::Registers;

/// Allocates the VM and registers the core primitives. Forth words write their
/// output to `output`, which is flushed at the end of each evaluated line.
///
/// # Safety
///
/// The returned VM holds raw pointers into its own dictionary. It must only be
/// driven through this crate's functions.
pub unsafe fn boot(output: impl Write + 'static) -> Registers {
    let mut regs = Registers::new(Box::new(output));
    primitives::install(&mut regs);
    regs
}
