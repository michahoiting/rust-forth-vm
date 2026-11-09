// Copyright (c) 2026 M.W.R. Hoiting

//! The compiler: whether a definition is being compiled, and how definitions,
//! words and literals are written to the dictionary.

use crate::cell::Cell;
use crate::dictionary::{cfa_of, Dictionary, Mark, F_PENDING};
use crate::error::ForthError;

/// A control structure that is still open in the definition. Each entry holds the address of the
/// operand that its forward branch gets once the structure ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ControlEntry {
    /// After `IF`: the operand of `0BRANCH`.
    If(Cell),
    /// After `ELSE`: the operand of `BRANCH`.
    Else(Cell),
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
        let operand = Self::compile_branch(dictionary, self.zero_branch_cfa)?;
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
        let operand = Self::compile_branch(dictionary, self.branch_cfa)?;
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

    /// Compiles a call to a branch word with an operand of 0, and returns the address of the
    /// operand. Nothing is compiled if either Cell doesn't fit.
    unsafe fn compile_branch(dictionary: &mut Dictionary, cfa: Cell) -> Result<Cell, ForthError> {
        let mark = dictionary.mark();
        dictionary.comma(cfa)?;
        let operand = dictionary.here();
        if let Err(error) = dictionary.comma(0) {
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
