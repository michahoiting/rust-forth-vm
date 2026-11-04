//! A clean-room Forth virtual machine.

mod cell;
mod compiler;
mod dictionary;
mod input;
mod interpreter;
mod primitives;
mod vm;

pub use interpreter::{evaluate, outer_interpreter};
pub use vm::Registers;

/// Allocates the VM and registers the core primitives.
///
/// # Safety
///
/// The returned VM holds raw pointers into its own dictionary. It must only be
/// driven through this crate's functions.
pub unsafe fn boot() -> Registers {
    let mut regs = Registers::new();
    primitives::install(&mut regs);
    regs
}
