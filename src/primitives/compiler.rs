// Copyright (c) 2026 M.W.R. Hoiting

//! Words that build the dictionary.

use super::runtime::exit;
use crate::dictionary::{Code, F_HIDDEN};
use crate::error::ForthError;
use crate::vm::Vm;

/// Reports an error if a definition is open, and returns whether one is. `word` would put
/// something other than the compiler's own cells into its thread, which would then run as code,
/// or would write a word header into the middle of it.
fn refuse_in_definition(vm: &mut Vm, word: &'static str) -> bool {
    let open = vm.compiler.is_compiling();
    if open {
        vm.set_error(ForthError::DefinitionOpen(word));
    }
    open
}

/// `, ( x -- )`: Pops a Cell from the stack and presses it into the dictionary at HERE. It
/// can't be used while a definition is open, because only the compiler writes to the thread.
/// An immediate word compiles a call with `COMPILE,` instead.
pub(super) fn comma(vm: &mut Vm) {
    if refuse_in_definition(vm, ",") {
        return;
    }
    if let Some(value) = vm.pop() {
        let result = vm.dictionary.comma(value);
        vm.check(result);
    }
}

/// `COMPILE, ( xt -- )`: Pops an execution token and compiles a call to that word into the open
/// definition. This is how an immediate word compiles code.
///
/// It reports [`ForthError::NotCompiling`] if no definition is open, and
/// [`ForthError::InvalidExecutionToken`] unless `xt` can run from anywhere, which refuses a
/// pending word and a word with an inline operand such as `LIT`.
pub(super) fn compile_comma(vm: &mut Vm) {
    if !vm.compiler.is_compiling() {
        vm.set_error(ForthError::NotCompiling("COMPILE,"));
        return;
    }
    if let Some(xt) = vm.pop() {
        if !vm.dictionary.is_execution_token(xt) {
            vm.set_error(ForthError::InvalidExecutionToken(xt));
            return;
        }
        let result = vm.compiler.compile_word(&mut vm.dictionary, xt);
        vm.check(result);
    }
}

/// `C, ( char -- )`: Pops a Cell from the stack and presses its low byte into the dictionary
/// at HERE, which can leave HERE unaligned. It can't be used while a definition is open.
pub(super) fn c_comma(vm: &mut Vm) {
    if refuse_in_definition(vm, "C,") {
        return;
    }
    if let Some(value) = vm.pop() {
        let result = vm.dictionary.char_comma(value as u8);
        vm.check(result);
    }
}

/// `ALLOT ( n -- )`: Reserves `n` bytes of the dictionary at HERE and clears them, which can
/// leave HERE unaligned. A negative `n` gives back bytes, but only from the data of the newest
/// word, and only if `CREATE` made it. It can't be used while a definition is open.
pub(super) fn allot(vm: &mut Vm) {
    if refuse_in_definition(vm, "ALLOT") {
        return;
    }
    if let Some(bytes) = vm.pop() {
        let result = vm.dictionary.allot(bytes as isize);
        vm.check(result);
    }
}

/// `ALIGN ( -- )`: Moves HERE to the next cell-aligned address, and clears the bytes it passes.
pub(super) fn align(vm: &mut Vm) {
    vm.dictionary.align();
}

/// Parses a name on behalf of `word`, and allocates a new word that pushes the address of its
/// body.
/// It can't be used while a definition is open, because the header would go into that
/// definition.
fn create_word(vm: &mut Vm, word: &'static str) {
    if refuse_in_definition(vm, word) {
        return;
    }
    let Some(name) = vm.input.parse_name() else {
        vm.set_error(ForthError::MissingName(word));
        return;
    };
    let result = vm.dictionary.write_header(&name, Code::Variable, 0);
    vm.check(result);
}

/// `CREATE ( -- )`: Parses a name and allocates a new word that pushes the address of its body.
pub(super) fn create(vm: &mut Vm) {
    create_word(vm, "CREATE");
}

/// `VARIABLE ( -- )`: Parses a name and creates a word that holds one Cell, initially 0.
pub(super) fn variable(vm: &mut Vm) {
    let mark = vm.dictionary.mark();
    create_word(vm, "VARIABLE");
    if vm.error.is_none() {
        let result = vm.dictionary.comma(0);
        if vm.check(result).is_none() {
            vm.dictionary.restore(mark); // Don't leave a word without its value
        }
    }
}

/// `CONSTANT ( x -- )`: Parses a name and creates a word that pushes `x`. The word wasn't made by
/// CREATE, so DOES> can't change it. It can't be used while a
/// definition is open.
pub(super) fn constant(vm: &mut Vm) {
    if refuse_in_definition(vm, "CONSTANT") {
        return;
    }
    let Some(value) = vm.pop() else {
        return;
    };
    let Some(name) = vm.input.parse_name() else {
        vm.set_error(ForthError::MissingName("CONSTANT"));
        return;
    };
    let restore_point = vm.dictionary.mark();
    let result = vm.dictionary.write_header(&name, Code::Constant, 0);
    if vm.check(result).is_some() {
        let result = vm.dictionary.comma(value);
        if vm.check(result).is_none() {
            // A word without its value would read past the dictionary when it ran.
            vm.dictionary.restore(restore_point);
        }
    }
}

/// `DOES> ( -- )`: Changes the action of the newest word while the defining word is executing.
/// The Forth code compiled behind DOES> becomes the action of the child.
pub(super) fn runtime_does(vm: &mut Vm) {
    // An empty Return Stack means DOES> is running from the text interpreter, so IP isn't in a
    // thread that could become the action.
    if vm.return_stack.is_empty() {
        vm.set_error(ForthError::DoesOutsideDefinition);
        return;
    }

    // Only words made by CREATE may change their action. Changing a colon word would make it
    // run its body as data.
    if !vm.dictionary.is_last_created() {
        vm.set_error(ForthError::DoesWithoutCreate);
        return;
    }

    // The action code starts at IP, which the inner interpreter runs as the word's action.
    vm.dictionary.set_last_code(Code::Does(vm.ip));

    // Unnest from the defining word (EXIT behavior)
    exit(vm);
}

/// `: ( -- )`: Parses a name and switches the text interpreter to compiling. It can't be used
/// while a definition is open.
pub(super) fn colon(vm: &mut Vm) {
    if refuse_in_definition(vm, ":") {
        return;
    }
    let Some(name) = vm.input.parse_name() else {
        vm.set_error(ForthError::MissingName(":"));
        return;
    };
    let result = vm
        .compiler
        .begin_definition(&mut vm.dictionary, &name, Code::Colon, 0);
    vm.check(result);
}

/// `:NONAME ( -- xt )`: Starts a definition without a name, and pushes its execution token. It
/// can't be used while a definition is open.
pub(super) fn colon_noname(vm: &mut Vm) {
    if refuse_in_definition(vm, ":NONAME") {
        return;
    }
    let result = vm
        .compiler
        .begin_definition(&mut vm.dictionary, "", Code::Colon, F_HIDDEN);
    if let Some(xt) = vm.check(result) {
        vm.push(xt);
    }
}

/// `; ( -- )`: Ends the current definition. This word is immediate.
pub(super) fn semicolon(vm: &mut Vm) {
    let result = vm.compiler.end_definition(&mut vm.dictionary);
    vm.check(result);
}

/// `IMMEDIATE ( -- )`: Marks the most recently created word as immediate, so that it runs
/// during compilation instead of being compiled.
pub(super) fn immediate(vm: &mut Vm) {
    vm.dictionary.mark_last_immediate();
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::mem;

    use super::*;
    use crate::cell::Cell;
    use crate::dictionary::{F_IMMEDIATE, F_PENDING};
    use crate::vm::{execute_xt, PrimitiveFn};
    use crate::{boot, outer_interpreter, VmConfig};

    const TEST_CELLS: usize = 64 * 1024;

    // Returns whether the word with this execution token has `flag` set.
    fn has_flag(vm: &Vm, xt: Cell, flag: Cell) -> bool {
        vm.dictionary.flags(xt).unwrap() & flag != 0
    }

    fn xt_of(vm: &Vm, name: &str) -> Cell {
        vm.dictionary.find_word(name).unwrap()
    }

    fn newest(vm: &Vm) -> Cell {
        vm.dictionary.last_xt().unwrap()
    }

    #[test]
    fn c_comma_presses_the_low_byte_and_pops_it() {
        let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
        let addr = vm.dictionary.here();
        vm.data_stack.extend_from_slice(&[1, 0x1AB]);

        c_comma(&mut vm);
        assert!(vm.error.is_none(), "got: {:?}", vm.error);
        assert_eq!(vm.data_stack, [1]);
        assert_eq!(vm.dictionary.here(), addr + 1);
        assert_eq!(vm.dictionary.fetch_byte(addr).unwrap(), 0xAB);
    }

    #[test]
    fn allot_moves_here_and_pops_the_size() {
        let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
        let addr = vm.dictionary.here();
        vm.data_stack.extend_from_slice(&[1, 3]);

        allot(&mut vm);
        assert!(vm.error.is_none(), "got: {:?}", vm.error);
        assert_eq!(vm.data_stack, [1]);
        assert_eq!(vm.dictionary.here(), addr + 3);
    }

    #[test]
    fn allot_reads_its_size_as_a_signed_number() {
        let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
        vm.input.load("X");
        create(&mut vm);
        let data = vm.dictionary.here();
        vm.dictionary.allot(3).unwrap();
        vm.data_stack.push(-2isize as Cell);

        allot(&mut vm);
        assert!(vm.error.is_none(), "got: {:?}", vm.error);
        assert_eq!(vm.dictionary.here(), data + 1);
    }

    #[test]
    fn align_aligns_here() {
        let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
        let addr = vm.dictionary.here();
        vm.dictionary.char_comma(1).unwrap();

        align(&mut vm);
        assert_eq!(vm.dictionary.here(), addr + mem::size_of::<Cell>());
    }

    #[test]
    fn words_that_break_up_cells_are_refused_in_a_definition() {
        for (word, name) in [(c_comma as PrimitiveFn, "C,"), (allot, "ALLOT")] {
            let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
            outer_interpreter(&mut vm, ": X").unwrap();
            let before = vm.dictionary.mark();
            vm.data_stack.push(1);

            word(&mut vm);
            assert!(
                matches!(vm.error, Some(ForthError::DefinitionOpen(n)) if n == name),
                "got: {:?}",
                vm.error
            );
            assert_eq!(vm.data_stack, [1]);
            assert_eq!(vm.dictionary.mark(), before);
        }
    }

    #[test]
    fn words_that_write_into_a_thread_or_make_headers_are_refused_in_a_definition() {
        let words: [(PrimitiveFn, &str); 6] = [
            (comma, ","),
            (create, "CREATE"),
            (variable, "VARIABLE"),
            (constant, "CONSTANT"),
            (colon, ":"),
            (colon_noname, ":NONAME"),
        ];
        for (word, name) in words {
            let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
            outer_interpreter(&mut vm, ": X").unwrap();
            vm.input.load("Y");
            let before = vm.dictionary.mark();
            vm.data_stack.push(1);

            word(&mut vm);
            assert!(
                matches!(vm.error, Some(ForthError::DefinitionOpen(n)) if n == name),
                "{}: {:?}",
                name,
                vm.error
            );
            assert_eq!(vm.data_stack, [1], "{}", name);
            assert_eq!(vm.dictionary.mark(), before, "{}", name);
            assert!(vm.compiler.is_compiling(), "{}", name);
        }
    }

    #[test]
    fn comma_still_works_outside_a_definition() {
        let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
        let addr = vm.dictionary.here();
        vm.data_stack.extend_from_slice(&[1, 42]);

        comma(&mut vm);
        assert!(vm.error.is_none(), "got: {:?}", vm.error);
        assert_eq!(vm.data_stack, [1]);
        assert_eq!(vm.dictionary.fetch(addr).unwrap(), 42);
    }

    // Starts a definition, and returns the address that its thread starts at.
    fn open_definition(vm: &mut Vm) -> Cell {
        outer_interpreter(vm, ": X").unwrap();
        vm.dictionary.here()
    }

    #[test]
    fn compile_comma_appends_a_call_and_pops_the_xt() {
        let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
        let thread = open_definition(&mut vm);
        let dup = xt_of(&vm, "DUP");
        vm.data_stack.extend_from_slice(&[1, dup]);

        compile_comma(&mut vm);
        assert!(vm.error.is_none(), "got: {:?}", vm.error);
        assert_eq!(vm.data_stack, [1]);
        assert_eq!(vm.dictionary.here(), thread + mem::size_of::<Cell>());
        assert_eq!(vm.dictionary.fetch(thread).unwrap(), dup);
    }

    #[test]
    fn compile_comma_needs_an_open_definition() {
        let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
        let dup = xt_of(&vm, "DUP");
        let before = vm.dictionary.mark();
        vm.data_stack.push(dup);

        compile_comma(&mut vm);
        assert!(matches!(
            vm.error,
            Some(ForthError::NotCompiling("COMPILE,"))
        ));
        assert_eq!(vm.data_stack, [dup]);
        assert_eq!(vm.dictionary.mark(), before);
    }

    #[test]
    fn compile_comma_refuses_anything_but_an_execution_token() {
        let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
        let thread = open_definition(&mut vm);
        // The thread of X starts with the XT of LIT, which a compiled literal writes.
        vm.compiler.compile_literal(&mut vm.dictionary, 5).unwrap();
        let lit = vm.dictionary.fetch(thread).unwrap();
        let pending = newest(&vm);
        let before = vm.dictionary.mark();

        for xt in [lit, pending, pending + 1, Cell::MAX] {
            vm.error = None;
            vm.data_stack.clear();
            vm.data_stack.push(xt);

            compile_comma(&mut vm);
            assert!(
                matches!(vm.error, Some(ForthError::InvalidExecutionToken(t)) if t == xt),
                "{:#x}: {:?}",
                xt,
                vm.error
            );
            assert!(vm.data_stack.is_empty());
            assert_eq!(vm.dictionary.mark(), before);
        }
    }

    #[test]
    fn compile_comma_needs_a_cell() {
        let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
        open_definition(&mut vm);
        let before = vm.dictionary.mark();

        compile_comma(&mut vm);
        assert!(matches!(vm.error, Some(ForthError::DataStackUnderflow)));
        assert_eq!(vm.dictionary.mark(), before);
    }

    #[test]
    fn constant_moves_its_value_into_a_new_word() {
        let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
        vm.input.load("X");
        vm.data_stack.extend_from_slice(&[1, 42]);

        constant(&mut vm);
        assert!(vm.error.is_none(), "got: {:?}", vm.error);
        assert_eq!(vm.data_stack, [1]);
        let xt = newest(&vm);
        assert_eq!(vm.dictionary.find_word("X"), Some(xt));
        let entry = vm.dictionary.entry(xt).unwrap();
        assert_eq!(entry.code, Code::Constant);
        assert_eq!(vm.dictionary.fetch(entry.body).unwrap(), 42);
        assert!(!vm.dictionary.is_last_created());
        assert_eq!(vm.dictionary.flags(xt), Some(0));

        execute_xt(&mut vm, xt);
        assert_eq!(vm.data_stack, [1, 42]);
    }

    #[test]
    fn constant_needs_a_value_and_writes_nothing_without_one() {
        let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
        vm.input.load("X");
        let before = vm.dictionary.mark();

        constant(&mut vm);
        assert!(matches!(vm.error, Some(ForthError::DataStackUnderflow)));
        assert_eq!(vm.dictionary.mark(), before);
    }

    #[test]
    fn constant_needs_a_name_and_writes_nothing_without_one() {
        let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
        vm.input.load("");
        vm.data_stack.push(42);
        let before = vm.dictionary.mark();

        constant(&mut vm);
        assert!(matches!(
            vm.error,
            Some(ForthError::MissingName("CONSTANT"))
        ));
        assert_eq!(vm.dictionary.mark(), before);
    }

    #[test]
    fn constant_that_does_not_fit_writes_nothing() {
        // The last size that fails has room for the header, but not for the value.
        let smallest = (0..TEST_CELLS)
            .find(|cells| boot(io::sink(), VmConfig::new(*cells)).is_ok())
            .unwrap();
        for cells in smallest..TEST_CELLS {
            let mut vm = boot(io::sink(), VmConfig::new(cells)).unwrap();
            vm.input.load("X");
            vm.data_stack.push(42);
            let before = vm.dictionary.mark();

            constant(&mut vm);
            if vm.error.is_none() {
                assert!(cells > smallest);
                return;
            }
            assert!(matches!(vm.error, Some(ForthError::DictionaryOverflow)));
            assert_eq!(vm.dictionary.mark(), before);
        }
        panic!("no dictionary size was large enough");
    }

    #[test]
    fn noname_starts_a_hidden_pending_definition_and_pushes_its_xt() {
        let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
        colon_noname(&mut vm);

        assert!(vm.error.is_none());
        assert!(vm.compiler.is_compiling());
        let xt = newest(&vm);
        assert_eq!(vm.data_stack, [xt]);
        assert!(has_flag(&vm, xt, F_HIDDEN));
        assert!(has_flag(&vm, xt, F_PENDING));
        assert_eq!(vm.dictionary.entry(xt).unwrap().code, Code::Colon);
    }

    #[test]
    fn colon_starts_a_pending_definition_that_is_not_hidden() {
        let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
        vm.input.load("X");
        colon(&mut vm);

        assert!(vm.error.is_none());
        assert!(vm.compiler.is_compiling());
        assert!(vm.data_stack.is_empty());
        let xt = newest(&vm);
        assert!(has_flag(&vm, xt, F_PENDING));
        assert!(!has_flag(&vm, xt, F_HIDDEN));
        assert_eq!(vm.dictionary.entry(xt).unwrap().code, Code::Colon);
    }

    #[test]
    fn immediate_marks_the_newest_word_and_leaves_the_stack_alone() {
        let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
        vm.input.load("X");
        create(&mut vm);
        vm.data_stack.push(7);

        immediate(&mut vm);
        assert!(vm.error.is_none(), "got: {:?}", vm.error);
        assert_eq!(vm.data_stack, [7]);
        assert!(has_flag(&vm, newest(&vm), F_IMMEDIATE));
    }

    #[test]
    fn does_gives_the_newest_word_the_action_thread() {
        let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
        outer_interpreter(&mut vm, ": D CREATE DOES> ; D X").unwrap();

        // The thread of D is CREATE, DOES>, EXIT, and the action is everything after DOES>.
        let defining_word = vm.dictionary.entry(xt_of(&vm, "D")).unwrap();
        let child = vm.dictionary.entry(xt_of(&vm, "X")).unwrap();
        let action = defining_word.body + 2 * mem::size_of::<Cell>();
        assert_eq!(child.code, Code::Does(action));
    }

    #[test]
    fn create_makes_a_word_that_pushes_its_body() {
        let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
        vm.input.load("X");
        create(&mut vm);

        assert!(vm.error.is_none(), "got: {:?}", vm.error);
        let xt = newest(&vm);
        assert_eq!(vm.dictionary.find_word("X"), Some(xt));
        let entry = vm.dictionary.entry(xt).unwrap();
        assert_eq!(entry.code, Code::Variable);
        assert_eq!(entry.body, vm.dictionary.here());
        assert_eq!(vm.dictionary.flags(xt), Some(0));
    }

    #[test]
    fn does_refuses_a_word_not_made_by_create() {
        for definition in [": X ;", "1 CONSTANT X"] {
            let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
            outer_interpreter(&mut vm, definition).unwrap();
            let xt = newest(&vm);
            let code = vm.dictionary.entry(xt).unwrap().code;
            vm.return_stack.push(crate::vm::NO_IP);

            runtime_does(&mut vm);
            assert!(matches!(vm.error, Some(ForthError::DoesWithoutCreate)));
            assert_eq!(vm.dictionary.entry(xt).unwrap().code, code);
        }
    }
}
