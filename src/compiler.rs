//! The compiler state: whether a definition is being compiled, and what is
//! needed to finish it or discard it.

use crate::cell::Cell;
use crate::dictionary::Mark;

#[derive(Default)]
pub(crate) struct Compiler {
    pub(crate) compiling: bool,          // STATE: true between `:` and `;`
    pub(crate) definition: Option<Mark>, // Dictionary state to restore if a definition fails
    pub(crate) lit_cfa: Cell,            // CFA compiled in front of inline literals
    pub(crate) exit_cfa: Cell,           // CFA compiled by `;`
}

impl Compiler {
    // Records the primitives that compiled code depends on.
    pub(crate) fn link_primitives(&mut self, lit_cfa: Cell, exit_cfa: Cell) {
        self.lit_cfa = lit_cfa;
        self.exit_cfa = exit_cfa;
    }
}
