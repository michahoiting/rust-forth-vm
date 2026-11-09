// Copyright (c) 2026 M.W.R. Hoiting

//! Words that build the dictionary.

use super::runtime::{docol, docon, dovar, exit};
use crate::cell::Cell;
use crate::dictionary::{cfa_of, is_created, F_CREATED, F_HIDDEN};
use crate::error::ForthError;
use crate::vm::{code_cell, Vm};

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
pub(super) unsafe fn comma(vm: &mut Vm) {
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
pub(super) unsafe fn compile_comma(vm: &mut Vm) {
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
pub(super) unsafe fn c_comma(vm: &mut Vm) {
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
pub(super) unsafe fn allot(vm: &mut Vm) {
    if refuse_in_definition(vm, "ALLOT") {
        return;
    }
    if let Some(bytes) = vm.pop() {
        let result = vm.dictionary.allot(bytes as isize);
        vm.check(result);
    }
}

/// `ALIGN ( -- )`: Moves HERE to the next cell-aligned address, and clears the bytes it passes.
pub(super) unsafe fn align(vm: &mut Vm) {
    vm.dictionary.align();
}

/// Parses a name on behalf of `word`, and allocates a new word with default DOVAR semantics.
/// It can't be used while a definition is open, because the header would go into that
/// definition.
unsafe fn create_word(vm: &mut Vm, word: &'static str) {
    if refuse_in_definition(vm, word) {
        return;
    }
    let Some(name) = vm.input.parse_name() else {
        vm.set_error(ForthError::MissingName(word));
        return;
    };
    let result = vm
        .dictionary
        .write_header(&name, code_cell(dovar), F_CREATED);
    vm.check(result);
}

/// `CREATE ( -- )`: Parses a name and allocates a new word with default DOVAR semantics.
pub(super) unsafe fn create(vm: &mut Vm) {
    create_word(vm, "CREATE");
}

/// `VARIABLE ( -- )`: Parses a name and creates a word that holds one Cell, initially 0.
pub(super) unsafe fn variable(vm: &mut Vm) {
    let mark = vm.dictionary.mark();
    create_word(vm, "VARIABLE");
    if vm.error.is_none() {
        let result = vm.dictionary.comma(0);
        if vm.check(result).is_none() {
            vm.dictionary.restore(mark); // Don't leave a word without its value
        }
    }
}

/// `CONSTANT ( x -- )`: Parses a name and creates a word that pushes `x`. The word is not
/// marked as made by CREATE, so DOES> can't overwrite its value. It can't be used while a
/// definition is open.
pub(super) unsafe fn constant(vm: &mut Vm) {
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
    let result = vm.dictionary.write_header(&name, code_cell(docon), 0);
    if vm.check(result).is_some() {
        let result = vm.dictionary.comma(value);
        if vm.check(result).is_none() {
            // A word without its value would read past the dictionary when it ran.
            vm.dictionary.restore(restore_point);
        }
    }
}

/// `DOES> ( -- )`: Hijacks the CFA of the child word while the defining word is executing.
/// The Forth code compiled behind DOES> becomes the action of the child.
pub(super) unsafe fn runtime_does(vm: &mut Vm) {
    // An empty Return Stack means DOES> is running from the text interpreter. IP then
    // points into the temporary trampoline, which must not be stored in the child.
    if vm.return_stack.is_empty() {
        vm.set_error(ForthError::DoesOutsideDefinition);
        return;
    }

    // Only words made by CREATE may change their action. Changing a colon word would make it
    // run its body as data.
    let child = vm.dictionary.last_word();
    if !is_created(child) {
        vm.set_error(ForthError::DoesWithoutCreate);
        return;
    }

    // Overwrite the child's CFA with the action code, which enter_word runs through DODOES.
    // The Return Stack isn't empty, so IP is in a thread in the dictionary.
    *cfa_of(child) = vm.ip as Cell;

    // Unnest from the defining word (EXIT behavior)
    exit(vm);
}

/// `: ( -- )`: Parses a name and switches the text interpreter to compiling. It can't be used
/// while a definition is open.
pub(super) unsafe fn colon(vm: &mut Vm) {
    if refuse_in_definition(vm, ":") {
        return;
    }
    let Some(name) = vm.input.parse_name() else {
        vm.set_error(ForthError::MissingName(":"));
        return;
    };
    let result = vm
        .compiler
        .begin_definition(&mut vm.dictionary, &name, code_cell(docol), 0);
    vm.check(result);
}

/// `:NONAME ( -- xt )`: Starts a definition without a name, and pushes its execution token. It
/// can't be used while a definition is open.
pub(super) unsafe fn colon_noname(vm: &mut Vm) {
    if refuse_in_definition(vm, ":NONAME") {
        return;
    }
    let result = vm
        .compiler
        .begin_definition(&mut vm.dictionary, "", code_cell(docol), F_HIDDEN);
    if let Some(xt) = vm.check(result) {
        vm.push(xt);
    }
}

/// `; ( -- )`: Ends the current definition. This word is immediate.
pub(super) unsafe fn semicolon(vm: &mut Vm) {
    let result = vm.compiler.end_definition(&mut vm.dictionary);
    vm.check(result);
}

/// `IF ( flag -- )`: Starts a conditional in the open definition: the code up to `ELSE` or
/// `THEN` runs only if `flag` is true. This word is immediate.
pub(super) unsafe fn if_(vm: &mut Vm) {
    let result = vm.compiler.begin_if(&mut vm.dictionary);
    vm.check(result);
}

/// `ELSE ( -- )`: Ends the true part of `IF`, and starts the code that runs when the flag was
/// false. This word is immediate.
pub(super) unsafe fn else_(vm: &mut Vm) {
    let result = vm.compiler.begin_else(&mut vm.dictionary);
    vm.check(result);
}

/// `THEN ( -- )`: Ends `IF`, or `IF` and `ELSE`: the code behind it runs in both cases. This
/// word is immediate.
pub(super) unsafe fn then(vm: &mut Vm) {
    let result = vm.compiler.end_if(&mut vm.dictionary);
    vm.check(result);
}

/// `BEGIN ( -- )`: Starts a loop in the open definition, which `UNTIL` ends. This word is
/// immediate.
pub(super) unsafe fn begin(vm: &mut Vm) {
    let result = vm.compiler.begin_loop(&vm.dictionary);
    vm.check(result);
}

/// `UNTIL ( flag -- )`: Ends a loop that `BEGIN` started: pops a flag, and goes back to `BEGIN`
/// if it is false. The loop body runs at least once. This word is immediate.
pub(super) unsafe fn until(vm: &mut Vm) {
    let result = vm.compiler.end_until(&mut vm.dictionary);
    vm.check(result);
}

/// `IMMEDIATE ( -- )`: Marks the most recently created word as immediate, so that it runs
/// during compilation instead of being compiled.
pub(super) unsafe fn immediate(vm: &mut Vm) {
    vm.dictionary.mark_last_immediate();
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::mem;

    use super::*;
    use crate::dictionary::{is_hidden, is_immediate, is_pending, pfa_of};
    use crate::vm::{enter_word, PrimitiveFn};
    use crate::{boot, outer_interpreter, VmConfig};

    const TEST_CELLS: usize = 64 * 1024;

    #[test]
    fn c_comma_presses_the_low_byte_and_pops_it() {
        unsafe {
            let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
            let addr = vm.dictionary.here();
            vm.data_stack.extend_from_slice(&[1, 0x1AB]);

            c_comma(&mut vm);
            assert!(vm.error.is_none(), "got: {:?}", vm.error);
            assert_eq!(vm.data_stack, [1]);
            assert_eq!(vm.dictionary.here(), addr + 1);
            assert_eq!(vm.dictionary.fetch_byte(addr).unwrap(), 0xAB);
        }
    }

    #[test]
    fn allot_moves_here_and_pops_the_size() {
        unsafe {
            let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
            let addr = vm.dictionary.here();
            vm.data_stack.extend_from_slice(&[1, 3]);

            allot(&mut vm);
            assert!(vm.error.is_none(), "got: {:?}", vm.error);
            assert_eq!(vm.data_stack, [1]);
            assert_eq!(vm.dictionary.here(), addr + 3);
        }
    }

    #[test]
    fn allot_reads_its_size_as_a_signed_number() {
        unsafe {
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
    }

    #[test]
    fn align_aligns_here() {
        unsafe {
            let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
            let addr = vm.dictionary.here();
            vm.dictionary.char_comma(1).unwrap();

            align(&mut vm);
            assert_eq!(vm.dictionary.here(), addr + mem::size_of::<Cell>());
        }
    }

    #[test]
    fn words_that_break_up_cells_are_refused_in_a_definition() {
        unsafe {
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
    }

    #[test]
    fn words_that_write_into_a_thread_or_make_headers_are_refused_in_a_definition() {
        unsafe {
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
    }

    #[test]
    fn comma_still_works_outside_a_definition() {
        unsafe {
            let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
            let addr = vm.dictionary.here();
            vm.data_stack.extend_from_slice(&[1, 42]);

            comma(&mut vm);
            assert!(vm.error.is_none(), "got: {:?}", vm.error);
            assert_eq!(vm.data_stack, [1]);
            assert_eq!(vm.dictionary.fetch(addr).unwrap(), 42);
        }
    }

    // Starts a definition, and returns the address that its thread starts at.
    unsafe fn open_definition(vm: &mut Vm) -> Cell {
        outer_interpreter(vm, ": X").unwrap();
        vm.dictionary.here()
    }

    #[test]
    fn compile_comma_appends_a_call_and_pops_the_xt() {
        unsafe {
            let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
            let thread = open_definition(&mut vm);
            let dup = cfa_of(vm.dictionary.find_word("DUP").unwrap()) as Cell;
            vm.data_stack.extend_from_slice(&[1, dup]);

            compile_comma(&mut vm);
            assert!(vm.error.is_none(), "got: {:?}", vm.error);
            assert_eq!(vm.data_stack, [1]);
            assert_eq!(vm.dictionary.here(), thread + mem::size_of::<Cell>());
            assert_eq!(vm.dictionary.fetch(thread).unwrap(), dup);
        }
    }

    #[test]
    fn compile_comma_needs_an_open_definition() {
        unsafe {
            let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
            let dup = cfa_of(vm.dictionary.find_word("DUP").unwrap()) as Cell;
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
    }

    #[test]
    fn compile_comma_refuses_anything_but_an_execution_token() {
        unsafe {
            let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
            let thread = open_definition(&mut vm);
            // The thread of X starts with the CFA of LIT, which a compiled literal writes.
            vm.compiler.compile_literal(&mut vm.dictionary, 5).unwrap();
            let lit = vm.dictionary.fetch(thread).unwrap();
            let pending = cfa_of(vm.dictionary.last_word()) as Cell;
            let before = vm.dictionary.mark();

            for xt in [lit, pending, 5, 0] {
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
    }

    #[test]
    fn compile_comma_needs_a_cell() {
        unsafe {
            let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
            open_definition(&mut vm);
            let before = vm.dictionary.mark();

            compile_comma(&mut vm);
            assert!(matches!(vm.error, Some(ForthError::DataStackUnderflow)));
            assert_eq!(vm.dictionary.mark(), before);
        }
    }

    #[test]
    fn constant_moves_its_value_into_a_new_word() {
        unsafe {
            let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
            vm.input.load("X");
            vm.data_stack.extend_from_slice(&[1, 42]);

            constant(&mut vm);
            assert!(vm.error.is_none(), "got: {:?}", vm.error);
            assert_eq!(vm.data_stack, [1]);
            let header = vm.dictionary.last_word();
            assert_eq!(vm.dictionary.find_word("X"), Some(header));
            assert_eq!(*pfa_of(cfa_of(header)), 42);
            assert!(!is_created(header));
            assert!(!is_hidden(header));
            assert!(!is_immediate(header));
            assert!(!is_pending(header));

            // Running the word shows which code it has. Function addresses can't show it,
            // because Rust doesn't guarantee that they compare equal.
            vm.w = cfa_of(header);
            enter_word(&mut vm);
            assert_eq!(vm.data_stack, [1, 42]);
        }
    }

    #[test]
    fn constant_needs_a_value_and_writes_nothing_without_one() {
        unsafe {
            let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
            vm.input.load("X");
            let before = vm.dictionary.mark();

            constant(&mut vm);
            assert!(matches!(vm.error, Some(ForthError::DataStackUnderflow)));
            assert_eq!(vm.dictionary.mark(), before);
        }
    }

    #[test]
    fn constant_needs_a_name_and_writes_nothing_without_one() {
        unsafe {
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
    }

    #[test]
    fn constant_that_does_not_fit_writes_nothing() {
        unsafe {
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
    }

    #[test]
    fn noname_starts_a_hidden_pending_definition_and_pushes_its_xt() {
        unsafe {
            let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
            colon_noname(&mut vm);

            assert!(vm.error.is_none());
            assert!(vm.compiler.is_compiling());
            let header = vm.dictionary.last_word();
            assert_eq!(vm.data_stack, [cfa_of(header) as Cell]);
            assert!(is_hidden(header));
            assert!(is_pending(header));
        }
    }

    #[test]
    fn colon_starts_a_pending_definition_that_is_not_hidden() {
        unsafe {
            let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
            vm.input.load("X");
            colon(&mut vm);

            assert!(vm.error.is_none());
            assert!(vm.compiler.is_compiling());
            assert!(vm.data_stack.is_empty());
            let header = vm.dictionary.last_word();
            assert!(is_pending(header));
            assert!(!is_hidden(header));
        }
    }

    #[test]
    fn immediate_marks_the_newest_word_and_leaves_the_stack_alone() {
        unsafe {
            let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
            vm.input.load("X");
            create(&mut vm);
            vm.data_stack.push(7);

            immediate(&mut vm);
            assert!(vm.error.is_none(), "got: {:?}", vm.error);
            assert_eq!(vm.data_stack, [7]);
            assert!(is_immediate(vm.dictionary.last_word()));
        }
    }

    #[test]
    fn does_stores_the_action_thread_in_the_code_field() {
        unsafe {
            let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
            outer_interpreter(&mut vm, ": D CREATE DOES> ; D X").unwrap();

            // The thread of D is CREATE, DOES>, EXIT, and the action is everything after DOES>.
            let defining_word = cfa_of(vm.dictionary.find_word("D").unwrap());
            let child = cfa_of(vm.dictionary.find_word("X").unwrap());
            assert_eq!(*child, pfa_of(defining_word).add(2) as Cell);
        }
    }
}
