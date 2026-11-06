//! The compiler: whether a definition is being compiled, and how definitions,
//! words and literals are written to the dictionary.

use crate::cell::Cell;
use crate::dictionary::{cfa_of, Dictionary, Mark, F_PENDING};
use crate::error::ForthError;

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
}

impl Compiler {
    /// Records the primitives that compiled code depends on.
    pub(crate) fn link_primitives(&mut self, lit_cfa: Cell, exit_cfa: Cell) {
        self.lit_cfa = lit_cfa;
        self.exit_cfa = exit_cfa;
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
    /// Returns [`ForthError::DictionaryOverflow`] if the word doesn't fit in the dictionary.
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
    /// Returns [`ForthError::NotCompiling`] if no definition is in progress, and
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
            return Err(ForthError::NotCompiling);
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

    /// Leaves compile mode and discards any half-built definition.
    pub(crate) fn abort(&mut self, dictionary: &mut Dictionary) {
        self.compiling = false;
        if let Some(mark) = self.definition.take() {
            dictionary.restore(mark);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dictionary::{is_hidden, is_pending, F_HIDDEN};

    const TEST_CELLS: usize = 1024;

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
