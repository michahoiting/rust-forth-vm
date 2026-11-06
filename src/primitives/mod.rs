//! The Forth words implemented in Rust.

mod compiler;
mod core;
mod runtime;

use crate::dictionary::{F_HIDDEN, F_IMMEDIATE};
use crate::error::ForthError;
use crate::vm::{code_cell, Registers};

// Registers the core primitives under their official text names.
pub(crate) unsafe fn install(regs: &mut Registers) -> Result<(), ForthError> {
    regs.dictionary
        .add_primitive_word("+", code_cell(core::plus), 0)?;
    regs.dictionary
        .add_primitive_word(".", code_cell(core::dot_print), 0)?;
    regs.dictionary
        .add_primitive_word("@", code_cell(core::fetch), 0)?;
    regs.dictionary
        .add_primitive_word("!", code_cell(core::store), 0)?;
    regs.dictionary
        .add_primitive_word(",", code_cell(compiler::comma), 0)?;
    regs.dictionary
        .add_primitive_word("CREATE", code_cell(compiler::create), 0)?;
    regs.dictionary
        .add_primitive_word("DOES>", code_cell(compiler::runtime_does), 0)?;
    regs.dictionary
        .add_primitive_word(":", code_cell(compiler::colon), 0)?;
    regs.dictionary
        .add_primitive_word(";", code_cell(compiler::semicolon), F_IMMEDIATE)?;
    let exit_cfa = regs
        .dictionary
        .add_primitive_word("EXIT", code_cell(runtime::exit), 0)?;
    // LIT reads its operand from the thread, so it only makes sense in compiled code.
    let lit_cfa = regs
        .dictionary
        .add_primitive_word("LIT", code_cell(runtime::lit), F_HIDDEN)?;
    regs.compiler.link_primitives(lit_cfa, exit_cfa);
    Ok(())
}
