// Copyright (c) 2026 M.W.R. Hoiting

//! The compiler: whether a definition is being compiled, and how definitions,
//! words and literals are written to the dictionary.

use crate::cell::Cell;
use crate::dictionary::{cfa_of, Dictionary, Mark, F_PENDING};
use crate::error::ForthError;

/// A control structure that is still open in the definition. An entry for a forward branch holds
/// the address of the operand that the branch gets once the structure ends. An entry for a
/// backward branch holds the address that the branch goes to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ControlEntry {
    /// After `IF`: the operand of `0BRANCH`.
    If(Cell),
    /// After `ELSE`: the operand of `BRANCH`.
    Else(Cell),
    /// After `BEGIN`: the address of the code behind it, where `UNTIL` goes back to.
    Begin(Cell),
}

/// The compilation state of a VM.
#[derive(Default)]
pub(crate) struct Compiler {
    /// STATE: true between `:` and `;`.
    compiling: bool,
    /// Dictionary state to restore if a definition fails.
    definition: Option<Mark>,
    /// CFA compiled in front of inline literals.
    lit_cfa: Cell,
    /// CFA compiled by `;`.
    exit_cfa: Cell,
    /// CFA compiled by `ELSE`.
    branch_cfa: Cell,
    /// CFA compiled by `IF`.
    zero_branch_cfa: Cell,
    /// CFA of `TYPE`, which `."` compiles.
    type_cfa: Cell,
    /// CFA of the hidden `(ABORT")`, which `ABORT"` compiles.
    abort_quote_cfa: Cell,
    /// The control structures that are open in the definition, innermost last. This is the
    /// control flow stack, which is kept apart from the data stack so that a number can't be
    /// taken for the address of a branch operand.
    control: Vec<ControlEntry>,
}

impl Compiler {
    /// Records the primitives that compiled code depends on.
    pub(crate) fn link_primitives(
        &mut self,
        lit_cfa: Cell,
        exit_cfa: Cell,
        branch_cfa: Cell,
        zero_branch_cfa: Cell,
    ) {
        self.lit_cfa = lit_cfa;
        self.exit_cfa = exit_cfa;
        self.branch_cfa = branch_cfa;
        self.zero_branch_cfa = zero_branch_cfa;
    }

    /// Records the primitives that the compiled forms of `."` and `ABORT"` call.
    pub(crate) fn link_string_words(&mut self, type_cfa: Cell, abort_quote_cfa: Cell) {
        self.type_cfa = type_cfa;
        self.abort_quote_cfa = abort_quote_cfa;
    }

    /// Returns whether a definition is being compiled.
    pub(crate) fn is_compiling(&self) -> bool {
        self.compiling
    }

    /// Starts compiling a word that runs `code`, and returns its execution token. The word
    /// stays pending until it ends, so a redefinition can still call the old word. Any
    /// `flags` are kept: a word with F_HIDDEN stays hidden after it ends.
    ///
    /// # Errors
    ///
    /// See [`Dictionary::write_header`].
    ///
    /// # Safety
    ///
    /// `code` must come from [`code_cell`](crate::vm::code_cell), because the VM calls it.
    pub(crate) unsafe fn begin_definition(
        &mut self,
        dictionary: &mut Dictionary,
        name: &str,
        code: Cell,
        flags: Cell,
    ) -> Result<Cell, ForthError> {
        let restore_point = dictionary.mark();
        let header = dictionary.write_header(name, code, flags | F_PENDING)?;
        self.definition = Some(restore_point);
        self.compiling = true;
        Ok(cfa_of(header) as Cell)
    }

    /// Ends the current definition and marks the word as finished.
    ///
    /// # Errors
    ///
    /// Returns [`ForthError::NotCompiling`] if no definition is in progress,
    /// [`ForthError::UnbalancedControlFlow`] if a control structure is still open, and
    /// [`ForthError::DictionaryOverflow`] if `EXIT` doesn't fit in the dictionary.
    ///
    /// # Safety
    ///
    /// The dictionary must hold at least one word.
    pub(crate) unsafe fn end_definition(
        &mut self,
        dictionary: &mut Dictionary,
    ) -> Result<(), ForthError> {
        if !self.compiling {
            return Err(ForthError::NotCompiling(";"));
        }
        if !self.control.is_empty() {
            return Err(ForthError::UnbalancedControlFlow(";"));
        }
        dictionary.comma(self.exit_cfa)?;
        dictionary.finish_last();
        self.definition = None;
        self.compiling = false;
        Ok(())
    }

    /// Compiles a call to the word with this CFA.
    ///
    /// # Errors
    ///
    /// Returns [`ForthError::DictionaryOverflow`] if the call doesn't fit in the dictionary.
    pub(crate) unsafe fn compile_word(
        &self,
        dictionary: &mut Dictionary,
        cfa: Cell,
    ) -> Result<(), ForthError> {
        dictionary.comma(cfa)
    }

    /// Compiles code that pushes `number` when it runs.
    ///
    /// # Errors
    ///
    /// Returns [`ForthError::DictionaryOverflow`] if the literal doesn't fit in the dictionary.
    pub(crate) unsafe fn compile_literal(
        &self,
        dictionary: &mut Dictionary,
        number: Cell,
    ) -> Result<(), ForthError> {
        dictionary.comma(self.lit_cfa)?;
        dictionary.comma(number)
    }

    /// Compiles the bytes of a string, followed by the code that pushes its address and length.
    /// The layout is `BRANCH`, the bytes, and then two literals. The branch goes over the bytes,
    /// so they never run, and the literals push the address of the bytes and their length. The
    /// bytes are word-aligned after they end, so that the literals stay aligned.
    ///
    /// # Errors
    ///
    /// Returns [`ForthError::DictionaryOverflow`] if the string doesn't fit in the dictionary.
    /// Nothing is compiled then.
    ///
    /// # Safety
    ///
    /// `dictionary` must come from a booted VM, as every dictionary write requires.
    pub(crate) unsafe fn compile_string(
        &self,
        dictionary: &mut Dictionary,
        text: &[u8],
    ) -> Result<(), ForthError> {
        let mark = dictionary.mark();
        let result = self.write_string(dictionary, text);
        if result.is_err() {
            dictionary.restore(mark);
        }
        result
    }

    /// Writes the layout of [`compile_string`](Self::compile_string), without restoring on error.
    unsafe fn write_string(
        &self,
        dictionary: &mut Dictionary,
        text: &[u8],
    ) -> Result<(), ForthError> {
        dictionary.comma(self.branch_cfa)?;
        let operand = dictionary.here();
        dictionary.comma(0)?;
        let address = dictionary.here();
        for &byte in text {
            dictionary.char_comma(byte)?;
        }
        dictionary.align();
        dictionary.store(operand, dictionary.here())?;
        self.compile_literal(dictionary, address)?;
        self.compile_literal(dictionary, text.len() as Cell)
    }

    /// Compiles the code of `."`: the string, then a call to `TYPE`.
    ///
    /// # Errors
    ///
    /// See [`compile_string`](Self::compile_string) and [`compile_word`](Self::compile_word).
    ///
    /// # Safety
    ///
    /// The same as [`compile_string`](Self::compile_string), and `link_string_words` must have
    /// run.
    pub(crate) unsafe fn compile_print_string(
        &self,
        dictionary: &mut Dictionary,
        text: &[u8],
    ) -> Result<(), ForthError> {
        self.compile_string(dictionary, text)?;
        self.compile_word(dictionary, self.type_cfa)
    }

    /// Compiles the code of `ABORT"`: the string, then a call to the hidden `(ABORT")`.
    ///
    /// # Errors
    ///
    /// See [`compile_string`](Self::compile_string) and [`compile_word`](Self::compile_word).
    ///
    /// # Safety
    ///
    /// The same as [`compile_print_string`](Self::compile_print_string).
    pub(crate) unsafe fn compile_abort_quote(
        &self,
        dictionary: &mut Dictionary,
        text: &[u8],
    ) -> Result<(), ForthError> {
        self.compile_string(dictionary, text)?;
        self.compile_word(dictionary, self.abort_quote_cfa)
    }

    /// Compiles the start of `IF`: a branch that is taken when the flag is zero, with an operand
    /// that `ELSE` or `THEN` fills in.
    ///
    /// # Errors
    ///
    /// Returns [`ForthError::NotCompiling`] if no definition is open, and
    /// [`ForthError::DictionaryOverflow`] if the branch doesn't fit in the dictionary. Nothing
    /// is compiled then.
    pub(crate) unsafe fn begin_if(
        &mut self,
        dictionary: &mut Dictionary,
    ) -> Result<(), ForthError> {
        if !self.compiling {
            return Err(ForthError::NotCompiling("IF"));
        }
        let operand = Self::compile_branch(dictionary, self.zero_branch_cfa, 0)?;
        self.control.push(ControlEntry::If(operand));
        Ok(())
    }

    /// Compiles `ELSE`: a branch over the false part, which `THEN` fills in. The branch of
    /// the open `IF` now goes to the code after it.
    ///
    /// # Errors
    ///
    /// Returns [`ForthError::NotCompiling`] if no definition is open,
    /// [`ForthError::UnbalancedControlFlow`] unless the innermost open structure is an `IF`, and
    /// [`ForthError::DictionaryOverflow`] if the branch doesn't fit in the dictionary. Nothing
    /// is compiled then.
    pub(crate) unsafe fn begin_else(
        &mut self,
        dictionary: &mut Dictionary,
    ) -> Result<(), ForthError> {
        if !self.compiling {
            return Err(ForthError::NotCompiling("ELSE"));
        }
        let Some(ControlEntry::If(open)) = self.control.last().copied() else {
            return Err(ForthError::UnbalancedControlFlow("ELSE"));
        };
        let operand = Self::compile_branch(dictionary, self.branch_cfa, 0)?;
        // The new branch is compiled first, so that the `IF` goes to the code behind it.
        dictionary.store(open, dictionary.here())?;
        self.control.pop();
        self.control.push(ControlEntry::Else(operand));
        Ok(())
    }

    /// Ends an `IF`, with or without an `ELSE`: its open branch now goes to HERE.
    ///
    /// # Errors
    ///
    /// Returns [`ForthError::NotCompiling`] if no definition is open, and
    /// [`ForthError::UnbalancedControlFlow`] if no `IF` or `ELSE` is open.
    pub(crate) fn end_if(&mut self, dictionary: &mut Dictionary) -> Result<(), ForthError> {
        if !self.compiling {
            return Err(ForthError::NotCompiling("THEN"));
        }
        let Some(&(ControlEntry::If(open) | ControlEntry::Else(open))) = self.control.last() else {
            return Err(ForthError::UnbalancedControlFlow("THEN"));
        };
        dictionary.store(open, dictionary.here())?;
        self.control.pop();
        Ok(())
    }

    /// Compiles `BEGIN`: the start of a loop. It compiles nothing, and records where `UNTIL`
    /// goes back to.
    ///
    /// # Errors
    ///
    /// Returns [`ForthError::NotCompiling`] if no definition is open.
    pub(crate) fn begin_loop(&mut self, dictionary: &Dictionary) -> Result<(), ForthError> {
        if !self.compiling {
            return Err(ForthError::NotCompiling("BEGIN"));
        }
        self.control.push(ControlEntry::Begin(dictionary.here()));
        Ok(())
    }

    /// Compiles `UNTIL`: a branch back to `BEGIN`, taken when the flag is zero.
    ///
    /// # Errors
    ///
    /// Returns [`ForthError::NotCompiling`] if no definition is open,
    /// [`ForthError::UnbalancedControlFlow`] unless the innermost open structure is a `BEGIN`, and
    /// [`ForthError::DictionaryOverflow`] if the branch doesn't fit in the dictionary. Nothing
    /// is compiled then, and the `BEGIN` stays open.
    pub(crate) unsafe fn end_until(
        &mut self,
        dictionary: &mut Dictionary,
    ) -> Result<(), ForthError> {
        if !self.compiling {
            return Err(ForthError::NotCompiling("UNTIL"));
        }
        let Some(&ControlEntry::Begin(target)) = self.control.last() else {
            return Err(ForthError::UnbalancedControlFlow("UNTIL"));
        };
        Self::compile_branch(dictionary, self.zero_branch_cfa, target)?;
        self.control.pop();
        Ok(())
    }

    /// Compiles a call to a branch word with this operand, and returns the address of the
    /// operand. Nothing is compiled if either Cell doesn't fit.
    unsafe fn compile_branch(
        dictionary: &mut Dictionary,
        cfa: Cell,
        target: Cell,
    ) -> Result<Cell, ForthError> {
        let mark = dictionary.mark();
        dictionary.comma(cfa)?;
        let operand = dictionary.here();
        if let Err(error) = dictionary.comma(target) {
            dictionary.restore(mark);
            return Err(error);
        }
        Ok(operand)
    }

    /// Leaves compile mode and discards any half-built definition.
    pub(crate) fn abort(&mut self, dictionary: &mut Dictionary) {
        self.compiling = false;
        self.control.clear();
        if let Some(mark) = self.definition.take() {
            dictionary.restore(mark);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::mem;

    use super::*;
    use crate::dictionary::{is_hidden, is_pending, F_HIDDEN};

    const TEST_CELLS: usize = 64 * 1024;

    #[test]
    fn begin_definition_sets_pending_and_not_hidden() {
        unsafe {
            let mut dictionary = Dictionary::new(TEST_CELLS).unwrap();
            let mut compiler = Compiler::default();
            compiler
                .begin_definition(&mut dictionary, "X", 0, 0)
                .unwrap();

            let header = dictionary.last_word();
            assert!(is_pending(header));
            assert!(!is_hidden(header));
        }
    }

    #[test]
    fn end_definition_clears_only_pending() {
        unsafe {
            let mut dictionary = Dictionary::new(TEST_CELLS).unwrap();
            let mut compiler = Compiler::default();
            compiler
                .begin_definition(&mut dictionary, "", 0, F_HIDDEN)
                .unwrap();
            let header = dictionary.last_word();
            assert!(is_pending(header));
            assert!(is_hidden(header));

            compiler.end_definition(&mut dictionary).unwrap();
            assert!(!is_pending(header));
            assert!(is_hidden(header));
        }
    }

    const IF_CFA: Cell = 0x1000;
    const ELSE_CFA: Cell = 0x2000;
    const EXIT_CFA: Cell = 0x3000;

    // Starts a definition in a new dictionary, with fake CFAs for the primitives.
    unsafe fn open() -> (Dictionary, Compiler) {
        let mut dictionary = Dictionary::new(TEST_CELLS).unwrap();
        let mut compiler = Compiler::default();
        compiler.link_primitives(0, EXIT_CFA, ELSE_CFA, IF_CFA);
        compiler
            .begin_definition(&mut dictionary, "X", 0, 0)
            .unwrap();
        (dictionary, compiler)
    }

    #[test]
    fn if_then_patches_the_branch_to_the_code_behind_it() {
        unsafe {
            let (mut dictionary, mut compiler) = open();
            let start = dictionary.here();
            let cell = mem::size_of::<Cell>();

            compiler.begin_if(&mut dictionary).unwrap();
            assert_eq!(dictionary.fetch(start).unwrap(), IF_CFA);
            assert_eq!(dictionary.fetch(start + cell).unwrap(), 0);
            compiler.compile_word(&mut dictionary, 7).unwrap();
            compiler.end_if(&mut dictionary).unwrap();

            assert_eq!(dictionary.fetch(start + cell).unwrap(), start + 3 * cell);
            assert_eq!(dictionary.here(), start + 3 * cell);
            assert!(compiler.control.is_empty());
        }
    }

    #[test]
    fn if_else_then_patches_both_branches() {
        unsafe {
            let (mut dictionary, mut compiler) = open();
            let start = dictionary.here();
            let cell = mem::size_of::<Cell>();

            compiler.begin_if(&mut dictionary).unwrap();
            compiler.compile_word(&mut dictionary, 7).unwrap();
            compiler.begin_else(&mut dictionary).unwrap();
            // The IF goes to the code behind the ELSE branch, which is 5 Cells in.
            assert_eq!(dictionary.fetch(start + cell).unwrap(), start + 5 * cell);
            assert_eq!(dictionary.fetch(start + 3 * cell).unwrap(), ELSE_CFA);
            compiler.compile_word(&mut dictionary, 8).unwrap();
            compiler.end_if(&mut dictionary).unwrap();

            assert_eq!(
                dictionary.fetch(start + 4 * cell).unwrap(),
                start + 6 * cell
            );
            assert_eq!(dictionary.here(), start + 6 * cell);
            assert!(compiler.control.is_empty());
        }
    }

    #[test]
    fn mismatches_change_nothing() {
        unsafe {
            let (mut dictionary, mut compiler) = open();
            let before = dictionary.mark();

            assert!(matches!(
                compiler.begin_else(&mut dictionary),
                Err(ForthError::UnbalancedControlFlow("ELSE"))
            ));
            assert!(matches!(
                compiler.end_if(&mut dictionary),
                Err(ForthError::UnbalancedControlFlow("THEN"))
            ));
            assert_eq!(dictionary.mark(), before);

            compiler.begin_if(&mut dictionary).unwrap();
            compiler.begin_else(&mut dictionary).unwrap();
            let open = compiler.control.clone();
            let before = dictionary.mark();
            assert!(matches!(
                compiler.begin_else(&mut dictionary),
                Err(ForthError::UnbalancedControlFlow("ELSE"))
            ));
            assert!(matches!(
                compiler.end_definition(&mut dictionary),
                Err(ForthError::UnbalancedControlFlow(";"))
            ));
            assert_eq!(dictionary.mark(), before);
            assert_eq!(compiler.control, open);
        }
    }

    #[test]
    fn abort_clears_the_control_flow_stack() {
        unsafe {
            let (mut dictionary, mut compiler) = open();
            compiler.begin_if(&mut dictionary).unwrap();

            compiler.abort(&mut dictionary);
            assert!(compiler.control.is_empty());
        }
    }

    #[test]
    fn control_flow_needs_a_definition() {
        unsafe {
            let mut dictionary = Dictionary::new(TEST_CELLS).unwrap();
            let mut compiler = Compiler::default();
            let before = dictionary.mark();

            assert!(matches!(
                compiler.begin_if(&mut dictionary),
                Err(ForthError::NotCompiling("IF"))
            ));
            assert!(matches!(
                compiler.begin_else(&mut dictionary),
                Err(ForthError::NotCompiling("ELSE"))
            ));
            assert!(matches!(
                compiler.end_if(&mut dictionary),
                Err(ForthError::NotCompiling("THEN"))
            ));
            assert_eq!(dictionary.mark(), before);
        }
    }

    #[test]
    fn a_branch_that_does_not_fit_compiles_nothing() {
        unsafe {
            // Fill the dictionary until one Cell is left, which holds the CFA but not the operand.
            let (mut dictionary, mut compiler) = open();
            let mut before_last = dictionary.mark();
            loop {
                let mark = dictionary.mark();
                if dictionary.comma(0).is_err() {
                    break;
                }
                before_last = mark;
            }
            dictionary.restore(before_last);
            let before = dictionary.mark();

            assert!(matches!(
                compiler.begin_if(&mut dictionary),
                Err(ForthError::DictionaryOverflow)
            ));
            assert_eq!(dictionary.mark(), before);
            assert!(compiler.control.is_empty());
        }
    }

    #[test]
    fn begin_compiles_nothing_and_until_branches_back_to_it() {
        unsafe {
            let (mut dictionary, mut compiler) = open();
            let start = dictionary.here();
            let cell = mem::size_of::<Cell>();

            compiler.begin_loop(&dictionary).unwrap();
            assert_eq!(dictionary.here(), start);
            assert_eq!(compiler.control, [ControlEntry::Begin(start)]);
            compiler.compile_word(&mut dictionary, 7).unwrap();
            compiler.end_until(&mut dictionary).unwrap();

            assert_eq!(dictionary.fetch(start).unwrap(), 7);
            assert_eq!(dictionary.fetch(start + cell).unwrap(), IF_CFA);
            assert_eq!(dictionary.fetch(start + 2 * cell).unwrap(), start);
            assert_eq!(dictionary.here(), start + 3 * cell);
            assert!(compiler.control.is_empty());
        }
    }

    #[test]
    fn loops_nest_and_mix_with_conditionals() {
        unsafe {
            let (mut dictionary, mut compiler) = open();
            let outer = dictionary.here();
            let cell = mem::size_of::<Cell>();

            compiler.begin_loop(&dictionary).unwrap();
            compiler.begin_if(&mut dictionary).unwrap();
            compiler.begin_loop(&dictionary).unwrap();
            let inner = dictionary.here();
            compiler.end_until(&mut dictionary).unwrap();
            compiler.end_if(&mut dictionary).unwrap();
            compiler.end_until(&mut dictionary).unwrap();

            // IF operand, the inner UNTIL, and the outer UNTIL.
            assert_eq!(dictionary.fetch(outer + cell).unwrap(), inner + 2 * cell);
            assert_eq!(dictionary.fetch(inner + cell).unwrap(), inner);
            assert_eq!(dictionary.fetch(inner + 3 * cell).unwrap(), outer);
            assert!(compiler.control.is_empty());
        }
    }

    #[test]
    fn until_needs_a_begin_and_changes_nothing_without_one() {
        unsafe {
            let (mut dictionary, mut compiler) = open();
            let before = dictionary.mark();
            assert!(matches!(
                compiler.end_until(&mut dictionary),
                Err(ForthError::UnbalancedControlFlow("UNTIL"))
            ));
            assert_eq!(dictionary.mark(), before);

            for open_entry in [false, true] {
                compiler.begin_if(&mut dictionary).unwrap();
                if open_entry {
                    compiler.begin_else(&mut dictionary).unwrap();
                }
                let stack = compiler.control.clone();
                let before = dictionary.mark();
                assert!(matches!(
                    compiler.end_until(&mut dictionary),
                    Err(ForthError::UnbalancedControlFlow("UNTIL"))
                ));
                assert_eq!(dictionary.mark(), before);
                assert_eq!(compiler.control, stack);
                compiler.end_if(&mut dictionary).unwrap();
            }
        }
    }

    #[test]
    fn else_and_then_do_not_close_a_begin() {
        unsafe {
            let (mut dictionary, mut compiler) = open();
            compiler.begin_loop(&dictionary).unwrap();
            let before = dictionary.mark();

            assert!(matches!(
                compiler.begin_else(&mut dictionary),
                Err(ForthError::UnbalancedControlFlow("ELSE"))
            ));
            assert!(matches!(
                compiler.end_if(&mut dictionary),
                Err(ForthError::UnbalancedControlFlow("THEN"))
            ));
            assert!(matches!(
                compiler.end_definition(&mut dictionary),
                Err(ForthError::UnbalancedControlFlow(";"))
            ));
            assert_eq!(dictionary.mark(), before);
            assert_eq!(compiler.control.len(), 1);
        }
    }

    #[test]
    fn begin_and_until_need_a_definition() {
        unsafe {
            let mut dictionary = Dictionary::new(TEST_CELLS).unwrap();
            let mut compiler = Compiler::default();
            let before = dictionary.mark();

            assert!(matches!(
                compiler.begin_loop(&dictionary),
                Err(ForthError::NotCompiling("BEGIN"))
            ));
            assert!(matches!(
                compiler.end_until(&mut dictionary),
                Err(ForthError::NotCompiling("UNTIL"))
            ));
            assert_eq!(dictionary.mark(), before);
            assert!(compiler.control.is_empty());
        }
    }

    #[test]
    fn an_until_that_does_not_fit_compiles_nothing_and_keeps_the_begin() {
        unsafe {
            let (mut dictionary, mut compiler) = open();
            compiler.begin_loop(&dictionary).unwrap();
            // Fill the dictionary until one Cell is left, which holds the CFA but not the operand.
            let mut before_last = dictionary.mark();
            loop {
                let mark = dictionary.mark();
                if dictionary.comma(0).is_err() {
                    break;
                }
                before_last = mark;
            }
            dictionary.restore(before_last);
            let before = dictionary.mark();

            assert!(matches!(
                compiler.end_until(&mut dictionary),
                Err(ForthError::DictionaryOverflow)
            ));
            assert_eq!(dictionary.mark(), before);
            assert_eq!(compiler.control.len(), 1);
        }
    }

    #[test]
    fn abort_clears_an_open_begin() {
        unsafe {
            let (mut dictionary, mut compiler) = open();
            compiler.begin_loop(&dictionary).unwrap();

            compiler.abort(&mut dictionary);
            assert!(compiler.control.is_empty());
        }
    }

    #[test]
    fn begin_definition_returns_the_code_field() {
        unsafe {
            let mut dictionary = Dictionary::new(TEST_CELLS).unwrap();
            let mut compiler = Compiler::default();
            let xt = compiler
                .begin_definition(&mut dictionary, "X", 0, 0)
                .unwrap();
            assert_eq!(xt, cfa_of(dictionary.last_word()) as Cell);
        }
    }
}
