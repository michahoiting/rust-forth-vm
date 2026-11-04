//! A clean-room Forth virtual machine.

mod cell;
mod dictionary;
mod input;
mod interpreter;
mod primitives;
mod vm;

pub use interpreter::{evaluate, outer_interpreter};
pub use primitives::boot;
pub use vm::Registers;
