// Copyright (c) 2026 M.W.R. Hoiting

//! The Forth words implemented in Rust.

mod compiler;
mod core;
mod parsing;
mod runtime;

use crate::compiler::{EXIT_XT, LIT_XT};
use crate::dictionary::{F_COMPILE_ONLY, F_HIDDEN, F_IMMEDIATE};
use crate::error::ForthError;
use crate::vm::Vm;

pub use self::core::COPYRIGHT;

/// Registers the core primitives under their official text names.
///
/// # Errors
///
/// Returns [`ForthError::DictionaryOverflow`] if they don't fit in the dictionary.
pub(crate) fn install(vm: &mut Vm) -> Result<(), ForthError> {
    // The compiler emits LIT and EXIT itself, so they come first and get the fixed execution
    // tokens LIT_XT and EXIT_XT. LIT reads its operand from the thread, so it only makes sense
    // in compiled code. EXECUTE would run it with IP somewhere else. Every word with an inline
    // operand needs F_COMPILE_ONLY.
    let lit = vm.add_primitive("LIT", runtime::lit, F_HIDDEN | F_COMPILE_ONLY)?;
    let exit = vm.add_primitive("EXIT", runtime::exit, 0)?;
    debug_assert_eq!((lit, exit), (LIT_XT, EXIT_XT));

    vm.add_primitive("+", core::plus, 0)?;
    vm.add_primitive("-", core::minus, 0)?;
    vm.add_primitive("*", core::multiply, 0)?;
    vm.add_primitive("/", core::divide, 0)?;
    vm.add_primitive("MOD", core::modulo, 0)?;
    vm.add_primitive("/MOD", core::divide_mod, 0)?;
    vm.add_primitive("1+", core::one_plus, 0)?;
    vm.add_primitive("1-", core::one_minus, 0)?;
    vm.add_primitive("2*", core::two_times, 0)?;
    vm.add_primitive("2/", core::two_divide, 0)?;
    vm.add_primitive("NEGATE", core::negate, 0)?;
    vm.add_primitive("ABS", core::absolute, 0)?;
    vm.add_primitive("MIN", core::minimum, 0)?;
    vm.add_primitive("MAX", core::maximum, 0)?;
    vm.add_primitive("=", core::equal, 0)?;
    vm.add_primitive("<", core::less_than, 0)?;
    vm.add_primitive(">", core::greater_than, 0)?;
    vm.add_primitive("U<", core::unsigned_less_than, 0)?;
    vm.add_primitive("0=", core::zero_equal, 0)?;
    vm.add_primitive("0<", core::zero_less_than, 0)?;
    vm.add_primitive("AND", core::and, 0)?;
    vm.add_primitive("OR", core::or, 0)?;
    vm.add_primitive("XOR", core::xor, 0)?;
    vm.add_primitive("INVERT", core::invert, 0)?;
    vm.add_primitive("LSHIFT", core::left_shift, 0)?;
    vm.add_primitive("RSHIFT", core::right_shift, 0)?;
    vm.add_primitive("+!", core::plus_store, 0)?;
    vm.add_primitive("CELL+", core::cell_plus, 0)?;
    vm.add_primitive("2!", core::two_store, 0)?;
    vm.add_primitive("2@", core::two_fetch, 0)?;
    vm.add_primitive("2DROP", core::two_drop, 0)?;
    vm.add_primitive("2DUP", core::two_dup, 0)?;
    vm.add_primitive("2OVER", core::two_over, 0)?;
    vm.add_primitive("2SWAP", core::two_swap, 0)?;
    vm.add_primitive("FILL", core::fill, 0)?;
    vm.add_primitive("MOVE", core::move_bytes, 0)?;
    vm.add_primitive("BL", core::blank, 0)?;
    vm.add_primitive(".", core::dot_print, 0)?;
    vm.add_primitive("@", core::fetch, 0)?;
    vm.add_primitive("C@", core::c_fetch, 0)?;
    vm.add_primitive("C!", core::c_store, 0)?;
    vm.add_primitive("HERE", core::here, 0)?;
    vm.add_primitive("ALIGNED", core::aligned, 0)?;
    vm.add_primitive("CHAR+", core::char_plus, 0)?;
    vm.add_primitive("CHARS", core::chars, 0)?;
    vm.add_primitive("CELLS", core::cells, 0)?;
    vm.add_primitive(">BODY", core::to_body, 0)?;
    vm.add_primitive("!", core::store, 0)?;
    vm.add_primitive("DUP", core::dup, 0)?;
    vm.add_primitive("?DUP", core::question_dup, 0)?;
    vm.add_primitive("DROP", core::drop, 0)?;
    vm.add_primitive("SWAP", core::swap, 0)?;
    vm.add_primitive("OVER", core::over, 0)?;
    vm.add_primitive("ROT", core::rot, 0)?;
    vm.add_primitive("PICK", core::pick, 0)?;
    vm.add_primitive("ROLL", core::roll, 0)?;
    vm.add_primitive("DEPTH", core::depth, 0)?;
    vm.add_primitive("HELP", core::help, 0)?;
    vm.add_primitive("COPYRIGHT", core::copyright, 0)?;
    vm.add_primitive("LICENSE", core::license, 0)?;
    vm.add_primitive(",", compiler::comma, 0)?;
    vm.add_primitive("COMPILE,", compiler::compile_comma, 0)?;
    vm.add_primitive("C,", compiler::c_comma, 0)?;
    vm.add_primitive("ALLOT", compiler::allot, 0)?;
    vm.add_primitive("ALIGN", compiler::align, 0)?;
    vm.add_primitive("CREATE", compiler::create, 0)?;
    vm.add_primitive("VARIABLE", compiler::variable, 0)?;
    vm.add_primitive("CONSTANT", compiler::constant, 0)?;
    vm.add_primitive("DOES>", compiler::runtime_does, 0)?;
    vm.add_primitive(":", compiler::colon, 0)?;
    vm.add_primitive(":NONAME", compiler::colon_noname, 0)?;
    vm.add_primitive(";", compiler::semicolon, F_IMMEDIATE)?;
    vm.add_primitive("IMMEDIATE", compiler::immediate, 0)?;
    vm.add_primitive("(", parsing::paren_comment, F_IMMEDIATE)?;
    vm.add_primitive("\\", parsing::line_comment, F_IMMEDIATE)?;
    vm.add_primitive("'", parsing::tick, 0)?;
    vm.add_primitive("[']", parsing::bracket_tick, F_IMMEDIATE)?;
    vm.add_primitive("EXECUTE", runtime::execute, 0)?;
    vm.add_primitive("QUIT", runtime::quit, 0)?;
    vm.add_primitive("ABORT", runtime::abort, 0)?;
    vm.add_primitive("BYE", runtime::bye, 0)?;
    vm.add_primitive("(BYE)", runtime::bye_with_code, 0)?;
    Ok(())
}
