// Copyright (c) 2026 M.W.R. Hoiting

//! The compiler: whether a definition is being compiled, and how definitions,
//! words and literals are written to the dictionary.

use crate::cell::Cell;
use crate::dictionary::{Code, Dictionary, Mark, F_PENDING};
use crate::error::ForthError;

/// The execution token of `LIT`, which compiled code puts in front of inline literals. It is
/// fixed, because `LIT` is the first primitive registered.
pub(crate) const LIT_XT: Cell = 0;

/// The execution token of `EXIT`, which `;` compiles. It is fixed, because `EXIT` is the second
/// primitive registered.
pub(crate) const EXIT_XT: Cell = 1;

/// The compilation state of a VM.
#[derive(Default)]
pub(crate) struct Compiler {
    /// STATE: true between `:` and `;`.
    compiling: bool,
    /// Dictionary state to restore if a definition fails.
    definition: Option<Mark>,
}

impl Compiler {
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
    pub(crate) fn begin_definition(
        &mut self,
        dictionary: &mut Dictionary,
        name: &str,
        code: Code,
        flags: Cell,
    ) -> Result<Cell, ForthError> {
        let restore_point = dictionary.mark();
        let xt = dictionary.write_header(name, code, flags | F_PENDING)?;
        self.definition = Some(restore_point);
        self.compiling = true;
        Ok(xt)
    }

    /// Ends the current definition and marks the word as finished.
    ///
    /// # Errors
    ///
    /// Returns [`ForthError::NotCompiling`] if no definition is in progress, and
    /// [`ForthError::DictionaryOverflow`] if `EXIT` doesn't fit in the dictionary.
    pub(crate) fn end_definition(&mut self, dictionary: &mut Dictionary) -> Result<(), ForthError> {
        if !self.compiling {
            return Err(ForthError::NotCompiling(";"));
        }
        dictionary.comma(EXIT_XT)?;
        dictionary.finish_last();
        self.definition = None;
        self.compiling = false;
        Ok(())
    }

    /// Compiles a call to the word with this execution token.
    ///
    /// # Errors
    ///
    /// Returns [`ForthError::DictionaryOverflow`] if the call doesn't fit in the dictionary.
    pub(crate) fn compile_word(
        &self,
        dictionary: &mut Dictionary,
        xt: Cell,
    ) -> Result<(), ForthError> {
        dictionary.comma(xt)
    }

    /// Compiles code that pushes `number` when it runs.
    ///
    /// # Errors
    ///
    /// Returns [`ForthError::DictionaryOverflow`] if the literal doesn't fit in the dictionary.
    pub(crate) fn compile_literal(
        &self,
        dictionary: &mut Dictionary,
        number: Cell,
    ) -> Result<(), ForthError> {
        dictionary.comma(LIT_XT)?;
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
    use crate::dictionary::F_HIDDEN;

    const TEST_CELLS: usize = 64 * 1024;

    // Returns whether `xt` can be found by its name.
    fn findable(dictionary: &Dictionary, name: &str, xt: Cell) -> bool {
        dictionary.find_word(name) == Some(xt)
    }

    #[test]
    fn begin_definition_sets_pending_and_not_hidden() {
        let mut dictionary = Dictionary::new(TEST_CELLS).unwrap();
        let mut compiler = Compiler::default();
        let xt = compiler
            .begin_definition(&mut dictionary, "X", Code::Colon, 0)
            .unwrap();
        assert!(!findable(&dictionary, "X", xt));
        assert!(!dictionary.is_execution_token(xt));

        compiler.end_definition(&mut dictionary).unwrap();
        assert!(findable(&dictionary, "X", xt));
    }

    #[test]
    fn end_definition_clears_only_pending() {
        let mut dictionary = Dictionary::new(TEST_CELLS).unwrap();
        let mut compiler = Compiler::default();
        let xt = compiler
            .begin_definition(&mut dictionary, "X", Code::Colon, F_HIDDEN)
            .unwrap();
        assert!(!dictionary.is_execution_token(xt));

        compiler.end_definition(&mut dictionary).unwrap();
        assert!(dictionary.is_execution_token(xt));
        assert!(!findable(&dictionary, "X", xt));
    }

    #[test]
    fn begin_definition_returns_the_newest_xt() {
        let mut dictionary = Dictionary::new(TEST_CELLS).unwrap();
        let mut compiler = Compiler::default();
        let xt = compiler
            .begin_definition(&mut dictionary, "X", Code::Colon, 0)
            .unwrap();
        assert_eq!(Some(xt), dictionary.last_xt());
        assert_eq!(dictionary.entry(xt).unwrap().code, Code::Colon);
    }

    #[test]
    fn end_definition_compiles_exit() {
        let mut dictionary = Dictionary::new(TEST_CELLS).unwrap();
        let mut compiler = Compiler::default();
        let xt = compiler
            .begin_definition(&mut dictionary, "X", Code::Colon, 0)
            .unwrap();
        compiler.compile_literal(&mut dictionary, 7).unwrap();
        compiler.compile_word(&mut dictionary, 5).unwrap();
        compiler.end_definition(&mut dictionary).unwrap();

        let body = dictionary.entry(xt).unwrap().body;
        let cell = std::mem::size_of::<Cell>();
        let thread: Vec<Cell> = (0..4)
            .map(|index| dictionary.fetch(body + index * cell).unwrap())
            .collect();
        assert_eq!(thread, [LIT_XT, 7, 5, EXIT_XT]);
        assert_eq!(dictionary.here(), body + 4 * cell);
    }

    #[test]
    fn abort_rolls_back_the_definition() {
        let mut dictionary = Dictionary::new(TEST_CELLS).unwrap();
        let mut compiler = Compiler::default();
        let mark = dictionary.mark();
        compiler
            .begin_definition(&mut dictionary, "X", Code::Colon, 0)
            .unwrap();
        compiler.compile_literal(&mut dictionary, 7).unwrap();

        compiler.abort(&mut dictionary);
        assert!(!compiler.is_compiling());
        assert_eq!(dictionary.mark(), mark);
    }
}
