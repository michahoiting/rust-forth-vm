//! Tests of Forth behaviour, using only the public API. Results are checked
//! through `.` and the output writer.

use std::cell::RefCell;
use std::error::Error;
use std::io::{self, BufWriter, Write};
use std::mem;
use std::rc::Rc;

use rust_forth_vm::{boot, outer_interpreter, Cell, ForthError};

// A writer whose contents stay readable after it is moved into the VM.
#[derive(Clone, Default)]
struct Capture(Rc<RefCell<Vec<u8>>>);

impl Capture {
    fn text(&self) -> String {
        String::from_utf8(self.0.borrow().clone()).unwrap()
    }
}

impl Write for Capture {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.borrow_mut().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

// A writer that fails every write.
struct FailingWriter;

impl Write for FailingWriter {
    fn write(&mut self, _bytes: &[u8]) -> io::Result<usize> {
        Err(io::Error::other("write failed"))
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

// Evaluates the lines in order, and returns everything they wrote.
fn output_of(lines: &[&str]) -> String {
    unsafe {
        let output = Capture::default();
        let mut regs = boot(output.clone());
        for line in lines {
            outer_interpreter(&mut regs, line).unwrap();
        }
        output.text()
    }
}

// Asserts that evaluating `input` fails with an error matching the pattern.
macro_rules! assert_error {
    ($regs:expr, $input:expr, $($pattern:tt)+) => {
        let error = outer_interpreter($regs, $input).unwrap_err();
        assert!(matches!(&error, $($pattern)+), "got: {}", error);
    };
}

#[test]
fn arithmetic_and_literals() {
    assert_eq!(output_of(&["40 2 + ."]), "42 ");
    assert_eq!(output_of(&["-5 3 + ."]), "-2 ");
    // The full unsigned range is accepted, and `.` prints signed.
    assert_eq!(output_of(&[&format!("{} .", Cell::MAX)]), "-1 ");
}

#[test]
fn create_comma_fetch() {
    assert_eq!(output_of(&["CREATE X 1337 , X @ ."]), "1337 ");
}

#[test]
fn name_is_owned_by_the_dictionary() {
    unsafe {
        let output = Capture::default();
        let mut regs = boot(output.clone());
        let line = String::from("CREATE X 1 ,");
        outer_interpreter(&mut regs, &line).unwrap();
        drop(line);
        outer_interpreter(&mut regs, "X @ .").unwrap();
        assert_eq!(output.text(), "1 ");
    }
}

#[test]
fn colon_definitions_nest() {
    let lines = [": ADD3 3 + ;", ": ADD6 ADD3 ADD3 ;", "36 ADD6 ."];
    assert_eq!(output_of(&lines), "42 ");
}

#[test]
fn definition_can_span_lines() {
    assert_eq!(output_of(&[": A 1", "2 + ;", "A ."]), "3 ");
}

#[test]
fn redefinition_calls_the_previous_word() {
    assert_eq!(output_of(&[": X 1 ;", ": X X 2 + ;", "X ."]), "3 ");
}

#[test]
fn exit_leaves_a_definition_early() {
    unsafe {
        let output = Capture::default();
        let mut regs = boot(output.clone());
        outer_interpreter(&mut regs, ": A 1 EXIT 2 ;").unwrap();
        outer_interpreter(&mut regs, "A .").unwrap();
        assert_eq!(output.text(), "1 ");
        // The 2 behind EXIT was never pushed.
        assert_error!(&mut regs, ".", ForthError::DataStackUnderflow);
    }
}

#[test]
fn does_gives_each_child_its_own_data() {
    let lines = [
        ": CONSTANT CREATE , DOES> @ ;",
        "42 CONSTANT ANSWER",
        "7 CONSTANT LUCKY",
        "ANSWER . LUCKY . ANSWER .",
    ];
    assert_eq!(output_of(&lines), "42 7 42 ");
}

#[test]
fn does_requires_a_definition() {
    unsafe {
        let mut regs = boot(io::sink());
        assert_error!(
            &mut regs,
            "CREATE X DOES>",
            ForthError::DoesOutsideDefinition
        );
    }
}

#[test]
fn does_requires_a_created_word() {
    unsafe {
        let mut regs = boot(io::sink());
        outer_interpreter(&mut regs, ": BAD 1 DOES> ;").unwrap();
        assert_error!(&mut regs, "BAD", ForthError::DoesWithoutCreate);
    }
}

#[test]
fn failed_definition_leaves_a_usable_vm() {
    unsafe {
        let output = Capture::default();
        let mut regs = boot(output.clone());
        assert_error!(&mut regs, ": BROKEN 1 NOPE ;", ForthError::UnknownWord(name) if name == "NOPE");
        assert_error!(&mut regs, "BROKEN", ForthError::UnknownWord(name) if name == "BROKEN");
        outer_interpreter(&mut regs, "1 2 + .").unwrap();
        assert_eq!(output.text(), "3 ");
    }
}

#[test]
fn stack_underflow_is_reported() {
    unsafe {
        let mut regs = boot(io::sink());
        assert_error!(&mut regs, "1 +", ForthError::DataStackUnderflow);
        assert_error!(&mut regs, ".", ForthError::DataStackUnderflow);
        assert_error!(&mut regs, "@", ForthError::DataStackUnderflow);
        assert_error!(&mut regs, ",", ForthError::DataStackUnderflow);
        assert_error!(&mut regs, "EXIT", ForthError::ReturnStackUnderflow);
    }
}

#[test]
fn misused_words_are_rejected() {
    unsafe {
        let mut regs = boot(io::sink());
        assert_error!(&mut regs, ";", ForthError::NotCompiling);
        assert_error!(&mut regs, ":", ForthError::MissingName(":"));
        assert_error!(&mut regs, "CREATE", ForthError::MissingName("CREATE"));
        assert_error!(&mut regs, "LIT", ForthError::UnknownWord(name) if name == "LIT");
    }
}

#[test]
fn dot_writes_a_signed_number() {
    assert_eq!(output_of(&["-5 3 + ."]), "-2 ");
}

#[test]
fn output_keeps_its_order() {
    assert_eq!(output_of(&["1 . 2 ."]), "1 2 ");
}

#[test]
fn output_before_an_error_is_kept() {
    unsafe {
        let output = Capture::default();
        let mut regs = boot(BufWriter::new(output.clone()));
        assert_error!(&mut regs, "1 . +", ForthError::DataStackUnderflow);
        assert_eq!(output.text(), "1 ");
    }
}

#[test]
fn underflow_in_dot_writes_nothing() {
    unsafe {
        let output = Capture::default();
        let mut regs = boot(output.clone());
        assert_error!(&mut regs, ".", ForthError::DataStackUnderflow);
        assert_eq!(output.text(), "");
    }
}

#[test]
fn failing_writer_is_reported() {
    unsafe {
        let mut regs = boot(FailingWriter);
        assert_error!(&mut regs, "1 .", ForthError::Output(_));
        outer_interpreter(&mut regs, "1 2 +").unwrap();
    }
}

#[test]
fn failing_flush_is_reported() {
    unsafe {
        let mut regs = boot(BufWriter::new(FailingWriter));
        assert_error!(&mut regs, "1 .", ForthError::Output(_));
    }
}

#[test]
fn buffered_output_is_flushed_at_the_end_of_a_line() {
    unsafe {
        let output = Capture::default();
        let mut regs = boot(BufWriter::new(output.clone()));
        outer_interpreter(&mut regs, "7 .").unwrap();
        assert_eq!(output.text(), "7 ");
    }
}

#[test]
fn output_error_keeps_its_cause() {
    unsafe {
        let mut regs = boot(FailingWriter);
        let error = outer_interpreter(&mut regs, "1 .").unwrap_err();
        assert_eq!(error.source().unwrap().to_string(), "write failed");
    }
}

#[test]
fn errors_have_stable_messages() {
    let cases = [
        (ForthError::DataStackUnderflow, "Data stack underflow."),
        (ForthError::ReturnStackUnderflow, "Return stack underflow."),
        (ForthError::DictionaryOverflow, "Dictionary overflow."),
        (ForthError::MissingName("CREATE"), "CREATE needs a name."),
        (
            ForthError::DoesOutsideDefinition,
            "DOES> can only be used inside a definition.",
        ),
        (
            ForthError::DoesWithoutCreate,
            "DOES> needs a word made by CREATE.",
        ),
        (
            ForthError::NotCompiling,
            "; can only be used while compiling.",
        ),
        (
            ForthError::UnknownWord("NOPE".to_string()),
            "Word or numeric literal 'NOPE' not recognized.",
        ),
        (ForthError::InvalidAddress(0x2a), "Invalid address 0x2a."),
        (ForthError::Output(io::Error::other("x")), "Output error."),
    ];
    for (error, message) in cases {
        assert_eq!(error.to_string(), message);
    }
}

#[test]
fn fetch_rejects_addresses_outside_the_dictionary() {
    unsafe {
        let mut regs = boot(io::sink());
        assert_error!(&mut regs, "0 @", ForthError::InvalidAddress(0));
        assert_error!(&mut regs, "5 @", ForthError::InvalidAddress(5));
        assert_error!(&mut regs, "-1 @", ForthError::InvalidAddress(Cell::MAX));
    }
}

#[test]
fn fetch_rejects_misaligned_addresses() {
    unsafe {
        let mut regs = boot(io::sink());
        assert_error!(
            &mut regs,
            "CREATE X 7 , X 1 + @",
            ForthError::InvalidAddress(_)
        );
    }
}

#[test]
fn fetch_rejects_here() {
    unsafe {
        let mut regs = boot(io::sink());
        assert_error!(&mut regs, "CREATE X X @", ForthError::InvalidAddress(_));
    }
}

#[test]
fn fetch_reads_the_last_written_cell() {
    assert_eq!(output_of(&["CREATE X 7 , X @ ."]), "7 ");
}

#[test]
fn fetch_reads_headers() {
    // The Cell in front of the data field is the action cell, which starts at zero.
    let line = format!("CREATE X 7 , X -{} + @ .", mem::size_of::<Cell>());
    assert_eq!(output_of(&[&line]), "0 ");
}

#[test]
fn invalid_address_leaves_a_usable_vm() {
    unsafe {
        let output = Capture::default();
        let mut regs = boot(output.clone());
        assert_error!(&mut regs, "5 @", ForthError::InvalidAddress(5));
        outer_interpreter(&mut regs, "1 2 + .").unwrap();
        assert_eq!(output.text(), "3 ");
    }
}
