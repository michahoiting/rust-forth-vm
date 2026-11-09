// Copyright (c) 2026 M.W.R. Hoiting

//! The Forth words implemented in Rust.

mod compiler;
mod core;
mod parsing;
mod runtime;

use crate::dictionary::{F_COMPILE_ONLY, F_HIDDEN, F_IMMEDIATE};
use crate::error::ForthError;
use crate::vm::{code_cell, Vm};

pub use self::core::COPYRIGHT;

/// Vm the core primitives under their official text names.
///
/// # Errors
///
/// Returns [`ForthError::DictionaryOverflow`] if they don't fit in the dictionary.
pub(crate) unsafe fn install(vm: &mut Vm) -> Result<(), ForthError> {
    vm.dictionary
        .add_primitive_word("+", code_cell(core::plus), 0)?;
    vm.dictionary
        .add_primitive_word("-", code_cell(core::minus), 0)?;
    vm.dictionary
        .add_primitive_word("*", code_cell(core::multiply), 0)?;
    vm.dictionary
        .add_primitive_word("/", code_cell(core::divide), 0)?;
    vm.dictionary
        .add_primitive_word("MOD", code_cell(core::modulo), 0)?;
    vm.dictionary
        .add_primitive_word("/MOD", code_cell(core::divide_mod), 0)?;
    vm.dictionary
        .add_primitive_word("1+", code_cell(core::one_plus), 0)?;
    vm.dictionary
        .add_primitive_word("1-", code_cell(core::one_minus), 0)?;
    vm.dictionary
        .add_primitive_word("2*", code_cell(core::two_times), 0)?;
    vm.dictionary
        .add_primitive_word("2/", code_cell(core::two_divide), 0)?;
    vm.dictionary
        .add_primitive_word("NEGATE", code_cell(core::negate), 0)?;
    vm.dictionary
        .add_primitive_word("ABS", code_cell(core::absolute), 0)?;
    vm.dictionary
        .add_primitive_word("MIN", code_cell(core::minimum), 0)?;
    vm.dictionary
        .add_primitive_word("MAX", code_cell(core::maximum), 0)?;
    vm.dictionary
        .add_primitive_word("=", code_cell(core::equal), 0)?;
    vm.dictionary
        .add_primitive_word("<", code_cell(core::less_than), 0)?;
    vm.dictionary
        .add_primitive_word(">", code_cell(core::greater_than), 0)?;
    vm.dictionary
        .add_primitive_word("U<", code_cell(core::unsigned_less_than), 0)?;
    vm.dictionary
        .add_primitive_word("<>", code_cell(core::not_equal), 0)?;
    vm.dictionary
        .add_primitive_word("U>", code_cell(core::unsigned_greater_than), 0)?;
    vm.dictionary
        .add_primitive_word("WITHIN", code_cell(core::within), 0)?;
    vm.dictionary
        .add_primitive_word("0=", code_cell(core::zero_equal), 0)?;
    vm.dictionary
        .add_primitive_word("0<", code_cell(core::zero_less_than), 0)?;
    vm.dictionary
        .add_primitive_word("0<>", code_cell(core::zero_not_equal), 0)?;
    vm.dictionary
        .add_primitive_word("0>", code_cell(core::zero_greater_than), 0)?;
    vm.dictionary
        .add_primitive_word("AND", code_cell(core::and), 0)?;
    vm.dictionary
        .add_primitive_word("OR", code_cell(core::or), 0)?;
    vm.dictionary
        .add_primitive_word("XOR", code_cell(core::xor), 0)?;
    vm.dictionary
        .add_primitive_word("INVERT", code_cell(core::invert), 0)?;
    vm.dictionary
        .add_primitive_word("NOT", code_cell(core::invert), 0)?;
    vm.dictionary
        .add_primitive_word("LSHIFT", code_cell(core::left_shift), 0)?;
    vm.dictionary
        .add_primitive_word("RSHIFT", code_cell(core::right_shift), 0)?;
    vm.dictionary
        .add_primitive_word("+!", code_cell(core::plus_store), 0)?;
    vm.dictionary
        .add_primitive_word("CELL+", code_cell(core::cell_plus), 0)?;
    vm.dictionary
        .add_primitive_word("2!", code_cell(core::two_store), 0)?;
    vm.dictionary
        .add_primitive_word("2@", code_cell(core::two_fetch), 0)?;
    vm.dictionary
        .add_primitive_word("2DROP", code_cell(core::two_drop), 0)?;
    vm.dictionary
        .add_primitive_word("2DUP", code_cell(core::two_dup), 0)?;
    vm.dictionary
        .add_primitive_word("2OVER", code_cell(core::two_over), 0)?;
    vm.dictionary
        .add_primitive_word("2SWAP", code_cell(core::two_swap), 0)?;
    vm.dictionary
        .add_primitive_word("FILL", code_cell(core::fill), 0)?;
    vm.dictionary
        .add_primitive_word("MOVE", code_cell(core::move_bytes), 0)?;
    vm.dictionary
        .add_primitive_word("BL", code_cell(core::blank), 0)?;
    vm.dictionary
        .add_primitive_word("TRUE", code_cell(core::true_flag), 0)?;
    vm.dictionary
        .add_primitive_word("FALSE", code_cell(core::false_flag), 0)?;
    vm.dictionary
        .add_primitive_word(".", code_cell(core::dot_print), 0)?;
    vm.dictionary
        .add_primitive_word("@", code_cell(core::fetch), 0)?;
    vm.dictionary
        .add_primitive_word("C@", code_cell(core::c_fetch), 0)?;
    vm.dictionary
        .add_primitive_word("C!", code_cell(core::c_store), 0)?;
    vm.dictionary
        .add_primitive_word("HERE", code_cell(core::here), 0)?;
    vm.dictionary
        .add_primitive_word("ALIGNED", code_cell(core::aligned), 0)?;
    vm.dictionary
        .add_primitive_word("CHAR+", code_cell(core::char_plus), 0)?;
    vm.dictionary
        .add_primitive_word("CHARS", code_cell(core::chars), 0)?;
    vm.dictionary
        .add_primitive_word("CELLS", code_cell(core::cells), 0)?;
    vm.dictionary
        .add_primitive_word(">BODY", code_cell(core::to_body), 0)?;
    vm.dictionary
        .add_primitive_word("!", code_cell(core::store), 0)?;
    vm.dictionary
        .add_primitive_word("DUP", code_cell(core::dup), 0)?;
    vm.dictionary
        .add_primitive_word("?DUP", code_cell(core::question_dup), 0)?;
    vm.dictionary
        .add_primitive_word("DROP", code_cell(core::drop), 0)?;
    vm.dictionary
        .add_primitive_word("SWAP", code_cell(core::swap), 0)?;
    vm.dictionary
        .add_primitive_word("OVER", code_cell(core::over), 0)?;
    vm.dictionary
        .add_primitive_word("NIP", code_cell(core::nip), 0)?;
    vm.dictionary
        .add_primitive_word("TUCK", code_cell(core::tuck), 0)?;
    vm.dictionary
        .add_primitive_word("ROT", code_cell(core::rot), 0)?;
    vm.dictionary
        .add_primitive_word("PICK", code_cell(core::pick), 0)?;
    vm.dictionary
        .add_primitive_word("ROLL", code_cell(core::roll), 0)?;
    vm.dictionary
        .add_primitive_word("DEPTH", code_cell(core::depth), 0)?;
    vm.dictionary
        .add_primitive_word("HELP", code_cell(core::help), 0)?;
    vm.dictionary
        .add_primitive_word("COPYRIGHT", code_cell(core::copyright), 0)?;
    vm.dictionary
        .add_primitive_word("LICENSE", code_cell(core::license), 0)?;
    vm.dictionary
        .add_primitive_word(",", code_cell(compiler::comma), 0)?;
    vm.dictionary
        .add_primitive_word("COMPILE,", code_cell(compiler::compile_comma), 0)?;
    vm.dictionary
        .add_primitive_word("C,", code_cell(compiler::c_comma), 0)?;
    vm.dictionary
        .add_primitive_word("ALLOT", code_cell(compiler::allot), 0)?;
    vm.dictionary
        .add_primitive_word("ALIGN", code_cell(compiler::align), 0)?;
    vm.dictionary
        .add_primitive_word("CREATE", code_cell(compiler::create), 0)?;
    vm.dictionary
        .add_primitive_word("VARIABLE", code_cell(compiler::variable), 0)?;
    vm.dictionary
        .add_primitive_word("CONSTANT", code_cell(compiler::constant), 0)?;
    vm.dictionary
        .add_primitive_word("DOES>", code_cell(compiler::runtime_does), 0)?;
    vm.dictionary
        .add_primitive_word(":", code_cell(compiler::colon), 0)?;
    vm.dictionary
        .add_primitive_word(":NONAME", code_cell(compiler::colon_noname), 0)?;
    vm.dictionary
        .add_primitive_word(";", code_cell(compiler::semicolon), F_IMMEDIATE)?;
    vm.dictionary
        .add_primitive_word("IMMEDIATE", code_cell(compiler::immediate), 0)?;
    vm.dictionary
        .add_primitive_word("IF", code_cell(compiler::if_), F_IMMEDIATE)?;
    vm.dictionary
        .add_primitive_word("ELSE", code_cell(compiler::else_), F_IMMEDIATE)?;
    vm.dictionary
        .add_primitive_word("THEN", code_cell(compiler::then), F_IMMEDIATE)?;
    vm.dictionary
        .add_primitive_word("BEGIN", code_cell(compiler::begin), F_IMMEDIATE)?;
    vm.dictionary
        .add_primitive_word("UNTIL", code_cell(compiler::until), F_IMMEDIATE)?;
    vm.dictionary
        .add_primitive_word("(", code_cell(parsing::paren_comment), F_IMMEDIATE)?;
    vm.dictionary
        .add_primitive_word("\\", code_cell(parsing::line_comment), F_IMMEDIATE)?;
    vm.dictionary
        .add_primitive_word("'", code_cell(parsing::tick), 0)?;
    vm.dictionary
        .add_primitive_word("[']", code_cell(parsing::bracket_tick), F_IMMEDIATE)?;
    vm.dictionary
        .add_primitive_word("EXECUTE", code_cell(runtime::execute), 0)?;
    vm.dictionary
        .add_primitive_word("QUIT", code_cell(runtime::quit), 0)?;
    vm.dictionary
        .add_primitive_word("ABORT", code_cell(runtime::abort), 0)?;
    vm.dictionary
        .add_primitive_word("BYE", code_cell(runtime::bye), 0)?;
    vm.dictionary
        .add_primitive_word("(BYE)", code_cell(runtime::bye_with_code), 0)?;
    let exit_cfa = vm
        .dictionary
        .add_primitive_word("EXIT", code_cell(runtime::exit), 0)?;
    // LIT reads its operand from the thread, so it only makes sense in compiled code. EXECUTE
    // would run it with IP somewhere else. Every word with an inline operand needs F_COMPILE_ONLY.
    let lit_cfa = vm.dictionary.add_primitive_word(
        "LIT",
        code_cell(runtime::lit),
        F_HIDDEN | F_COMPILE_ONLY,
    )?;
    // The branches read an operand from the thread too.
    let branch_cfa = vm.dictionary.add_primitive_word(
        "BRANCH",
        code_cell(runtime::branch),
        F_HIDDEN | F_COMPILE_ONLY,
    )?;
    let zero_branch_cfa = vm.dictionary.add_primitive_word(
        "0BRANCH",
        code_cell(runtime::zero_branch),
        F_HIDDEN | F_COMPILE_ONLY,
    )?;
    vm.compiler
        .link_primitives(lit_cfa, exit_cfa, branch_cfa, zero_branch_cfa);
    Ok(())
}
