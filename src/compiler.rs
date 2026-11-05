//! The compiler: whether a definition is being compiled, and how definitions,
//! words and literals are written to the dictionary.

use crate::cell::Cell;
use crate::dictionary::{Dictionary, Mark, F_HIDDEN};
use crate::error::ForthError;

#[derive(Default)]
pub(crate) struct Compiler {
    compiling: bool,          // STATE: true between `:` and `;`
    definition: Option<Mark>, // Dictionary state to restore if a definition fails
    lit_cfa: Cell,            // CFA compiled in front of inline literals
    exit_cfa: Cell,           // CFA compiled by `;`
}

impl Compiler {
    // Records the primitives that compiled code depends on.
    pub(crate) fn link_primitives(&mut self, lit_cfa: Cell, exit_cfa: Cell) {
        self.lit_cfa = lit_cfa;
        self.exit_cfa = exit_cfa;
    }

    pub(crate) fn is_compiling(&self) -> bool {
        self.compiling
    }

    // Starts compiling a word that runs `code` when it is entered.
    // The word stays hidden until it ends, so a redefinition can still call the old word.
    pub(crate) unsafe fn begin_definition(
        &mut self,
        dictionary: &mut Dictionary,
        name: &str,
        code: Cell,
    ) -> Result<(), ForthError> {
        let restore_point = dictionary.mark();
        dictionary.write_header(name, code, F_HIDDEN)?;
        self.definition = Some(restore_point);
        self.compiling = true;
        Ok(())
    }

    // Ends the current definition and makes the word visible.
    pub(crate) unsafe fn end_definition(
        &mut self,
        dictionary: &mut Dictionary,
    ) -> Result<(), ForthError> {
        if !self.compiling {
            return Err(ForthError::NotCompiling);
        }
        dictionary.comma(self.exit_cfa)?;
        dictionary.reveal_last();
        self.definition = None;
        self.compiling = false;
        Ok(())
    }

    // Compiles a call to the word with this CFA.
    pub(crate) unsafe fn compile_word(
        &self,
        dictionary: &mut Dictionary,
        cfa: Cell,
    ) -> Result<(), ForthError> {
        dictionary.comma(cfa)
    }

    // Compiles code that pushes `number` when it runs.
    pub(crate) unsafe fn compile_literal(
        &self,
        dictionary: &mut Dictionary,
        number: Cell,
    ) -> Result<(), ForthError> {
        dictionary.comma(self.lit_cfa)?;
        dictionary.comma(number)
    }

    // Leaves compile mode and discards any half-built definition.
    pub(crate) fn abort(&mut self, dictionary: &mut Dictionary) {
        self.compiling = false;
        if let Some(mark) = self.definition.take() {
            dictionary.restore(mark);
        }
    }
}
