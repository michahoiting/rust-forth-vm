// Copyright (c) 2026 M.W.R. Hoiting

//! The Forth words implemented in Rust.

mod compiler;
mod core;
mod parsing;
mod runtime;

use crate::dictionary::{F_HIDDEN, F_IMMEDIATE};
use crate::error::ForthError;
use crate::vm::{code_cell, Registers};

/// Registers the core primitives under their official text names.
///
/// # Errors
///
/// Returns [`ForthError::DictionaryOverflow`] if they don't fit in the dictionary.
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
        .add_primitive_word("DUP", code_cell(core::dup), 0)?;
    regs.dictionary
        .add_primitive_word("?DUP", code_cell(core::question_dup), 0)?;
    regs.dictionary
        .add_primitive_word("DROP", code_cell(core::drop), 0)?;
    regs.dictionary
        .add_primitive_word("SWAP", code_cell(core::swap), 0)?;
    regs.dictionary
        .add_primitive_word("OVER", code_cell(core::over), 0)?;
    regs.dictionary
        .add_primitive_word("ROT", code_cell(core::rot), 0)?;
    regs.dictionary
        .add_primitive_word("PICK", code_cell(core::pick), 0)?;
    regs.dictionary
        .add_primitive_word("ROLL", code_cell(core::roll), 0)?;
    regs.dictionary
        .add_primitive_word("DEPTH", code_cell(core::depth), 0)?;
    regs.dictionary
        .add_primitive_word(",", code_cell(compiler::comma), 0)?;
    regs.dictionary
        .add_primitive_word("CREATE", code_cell(compiler::create), 0)?;
    regs.dictionary
        .add_primitive_word("VARIABLE", code_cell(compiler::variable), 0)?;
    regs.dictionary
        .add_primitive_word("CONSTANT", code_cell(compiler::constant), 0)?;
    regs.dictionary
        .add_primitive_word("DOES>", code_cell(compiler::runtime_does), 0)?;
    regs.dictionary
        .add_primitive_word(":", code_cell(compiler::colon), 0)?;
    regs.dictionary
        .add_primitive_word(":NONAME", code_cell(compiler::colon_noname), 0)?;
    regs.dictionary
        .add_primitive_word(";", code_cell(compiler::semicolon), F_IMMEDIATE)?;
    regs.dictionary
        .add_primitive_word("IMMEDIATE", code_cell(compiler::immediate), 0)?;
    regs.dictionary
        .add_primitive_word("(", code_cell(parsing::paren_comment), F_IMMEDIATE)?;
    regs.dictionary
        .add_primitive_word("\\", code_cell(parsing::line_comment), F_IMMEDIATE)?;
    regs.dictionary
        .add_primitive_word("EXECUTE", code_cell(runtime::execute), 0)?;
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
