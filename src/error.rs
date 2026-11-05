//! The errors a Forth VM can report.

use std::error::Error;
use std::fmt;
use std::io;

/// An error that stopped the evaluation of a line.
#[derive(Debug)]
pub enum ForthError {
    /// A word needed more Cells than the data stack holds.
    DataStackUnderflow,
    /// `EXIT` ran with nothing to return to.
    ReturnStackUnderflow,
    /// A write did not fit in the dictionary.
    DictionaryOverflow,
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
    /// The output writer failed.
    Output(io::Error),
}

impl fmt::Display for ForthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ForthError::DataStackUnderflow => write!(f, "Data stack underflow."),
            ForthError::ReturnStackUnderflow => write!(f, "Return stack underflow."),
            ForthError::DictionaryOverflow => write!(f, "Dictionary overflow."),
            ForthError::MissingName(word) => write!(f, "{} needs a name.", word),
            ForthError::DoesOutsideDefinition => {
                write!(f, "DOES> can only be used inside a definition.")
            }
            ForthError::DoesWithoutCreate => write!(f, "DOES> needs a word made by CREATE."),
            ForthError::NotCompiling => write!(f, "; can only be used while compiling."),
            ForthError::UnknownWord(token) => {
                write!(f, "Word or numeric literal '{}' not recognized.", token)
            }
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
