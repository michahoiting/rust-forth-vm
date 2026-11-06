// Copyright (c) 2026 M.W.R. Hoiting

//! The errors a Forth VM can report.

use std::error::Error;
use std::fmt;
use std::io;

use crate::cell::Cell;

/// An error that stopped the evaluation of some input.
///
/// [`outer_interpreter`](crate::outer_interpreter) returns these, and so does
/// [`boot`](crate::boot). `Display` gives a message for the user.
///
/// # Examples
///
/// ```
/// use std::io;
///
/// use rust_forth_vm::{boot, outer_interpreter, ForthError};
///
/// // SAFETY: this VM is only used through the crate's functions.
/// let result = unsafe {
///     let mut regs = boot(io::sink(), 1024)?;
///     outer_interpreter(&mut regs, "HELLO")
/// };
///
/// match result {
///     Err(ForthError::UnknownWord(token)) => assert_eq!(token, "HELLO"),
///     other => panic!("unexpected result: {:?}", other),
/// }
/// # Ok::<(), ForthError>(())
/// ```
#[derive(Debug)]
pub enum ForthError {
    /// A word needed more Cells than the data stack holds.
    DataStackUnderflow,
    /// `EXIT` ran with nothing to return to.
    ReturnStackUnderflow,
    /// A write did not fit in the dictionary.
    DictionaryOverflow,
    /// A dictionary of this many Cells could not be allocated.
    DictionaryTooLarge(usize),
    /// A parsing word, named here, found no name in the input.
    MissingName(&'static str),
    /// `DOES>` ran from the text interpreter.
    DoesOutsideDefinition,
    /// `DOES>` ran when the newest word was not made by `CREATE`.
    DoesWithoutCreate,
    /// `;` ran without a definition in progress.
    NotCompiling,
    /// A token, held here, is neither a known word nor a number.
    UnknownWord(String),
    /// `@` or `!` was given an address, held here, that it may not access: one outside
    /// the written part of the dictionary, or one that is not cell-aligned.
    InvalidAddress(Cell),
    /// `EXECUTE` was given a Cell, held here, that isn't the code field of a finished word.
    InvalidExecutionToken(Cell),
    /// `(` found no closing `)` in the input.
    UnterminatedComment,
    /// The output writer failed.
    Output(io::Error),
}

impl fmt::Display for ForthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ForthError::DataStackUnderflow => write!(f, "Data stack underflow."),
            ForthError::ReturnStackUnderflow => write!(f, "Return stack underflow."),
            ForthError::DictionaryOverflow => write!(f, "Dictionary overflow."),
            ForthError::DictionaryTooLarge(cells) => {
                write!(f, "Cannot allocate a dictionary of {} Cells.", cells)
            }
            ForthError::MissingName(word) => write!(f, "{} needs a name.", word),
            ForthError::DoesOutsideDefinition => {
                write!(f, "DOES> can only be used inside a definition.")
            }
            ForthError::DoesWithoutCreate => write!(f, "DOES> needs a word made by CREATE."),
            ForthError::NotCompiling => write!(f, "; can only be used while compiling."),
            ForthError::UnknownWord(token) => {
                write!(f, "Word or numeric literal '{}' not recognized.", token)
            }
            ForthError::InvalidAddress(addr) => write!(f, "Invalid address {:#x}.", addr),
            ForthError::InvalidExecutionToken(xt) => {
                write!(f, "Invalid execution token {:#x}.", xt)
            }
            ForthError::UnterminatedComment => write!(f, "Comment has no closing parenthesis."),
            ForthError::Output(_) => write!(f, "Output error."),
        }
    }
}

impl Error for ForthError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            ForthError::Output(error) => Some(error),
            _ => None,
        }
    }
}
