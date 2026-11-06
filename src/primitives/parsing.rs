//! Words that read the input themselves, instead of leaving it to the text interpreter.
//! They are immediate, so that they also work inside definitions.

use crate::error::ForthError;
use crate::vm::Registers;

// PAREN ( ( ): Skips the input up to and including the next `)`. Comments don't nest.
pub(super) unsafe fn paren_comment(regs: &mut Registers) {
    if !regs.input.skip_past(')') {
        regs.set_error(ForthError::UnterminatedComment);
    }
}

// BACKSLASH ( \ ): Skips the rest of the line.
pub(super) unsafe fn line_comment(regs: &mut Registers) {
    regs.input.skip_line();
}

#[cfg(test)]
mod tests {
    use std::io;

    use super::*;
    use crate::boot;

    const TEST_CELLS: usize = 1024;

    #[test]
    fn paren_comment_skips_to_the_closing_parenthesis() {
        unsafe {
            let mut regs = boot(io::sink(), TEST_CELLS).unwrap();
            regs.input.load("skipped ) next");
            regs.data_stack.push(7);

            paren_comment(&mut regs);
            assert!(regs.error.is_none(), "got: {:?}", regs.error);
            assert_eq!(regs.data_stack, [7]);
            assert_eq!(regs.input.parse_name().as_deref(), Some("next"));
        }
    }

    #[test]
    fn paren_comment_reports_a_missing_parenthesis() {
        unsafe {
            let mut regs = boot(io::sink(), TEST_CELLS).unwrap();
            regs.input.load("never closed");

            paren_comment(&mut regs);
            assert!(matches!(regs.error, Some(ForthError::UnterminatedComment)));
        }
    }

    #[test]
    fn line_comment_skips_the_rest_of_the_line() {
        unsafe {
            let mut regs = boot(io::sink(), TEST_CELLS).unwrap();
            regs.input.load("skipped\nnext");
            regs.data_stack.push(7);

            line_comment(&mut regs);
            assert!(regs.error.is_none(), "got: {:?}", regs.error);
            assert_eq!(regs.data_stack, [7]);
            assert_eq!(regs.input.parse_name().as_deref(), Some("next"));
        }
    }
}
