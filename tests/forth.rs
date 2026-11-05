//! Tests of Forth behaviour, using only the public API. Results are checked
//! through `.` and the output writer.

use std::cell::RefCell;
use std::io::{self, BufWriter, Write};
use std::rc::Rc;

use rust_forth_vm::{boot, outer_interpreter, Registers};

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

// Asserts that evaluating `input` fails with a message containing `expected`.
unsafe fn assert_error(regs: &mut Registers, input: &str, expected: &str) {
    let message = outer_interpreter(regs, input).unwrap_err();
    assert!(message.contains(expected), "got: {}", message);
}

#[test]
fn arithmetic_and_literals() {
    assert_eq!(output_of(&["40 2 + ."]), "42 ");
    assert_eq!(output_of(&["-5 3 + ."]), "-2 ");
    // The full unsigned range is accepted, and `.` prints signed.
    assert_eq!(output_of(&["18446744073709551615 ."]), "-1 ");
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
        assert_error(&mut regs, ".", "underflow");
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
        assert_error(&mut regs, "CREATE X DOES>", "inside a definition");
    }
}

#[test]
fn does_requires_a_created_word() {
    unsafe {
        let mut regs = boot(io::sink());
        outer_interpreter(&mut regs, ": BAD 1 DOES> ;").unwrap();
        assert_error(&mut regs, "BAD", "made by CREATE");
    }
}

#[test]
fn failed_definition_leaves_a_usable_vm() {
    unsafe {
        let output = Capture::default();
        let mut regs = boot(output.clone());
        assert_error(&mut regs, ": BROKEN 1 NOPE ;", "not recognized");
        assert_error(&mut regs, "BROKEN", "not recognized");
        outer_interpreter(&mut regs, "1 2 + .").unwrap();
        assert_eq!(output.text(), "3 ");
    }
}

#[test]
fn stack_underflow_is_reported() {
    unsafe {
        let mut regs = boot(io::sink());
        assert_error(&mut regs, "1 +", "underflow");
        assert_error(&mut regs, ".", "underflow");
        assert_error(&mut regs, "@", "underflow");
        assert_error(&mut regs, ",", "underflow");
        assert_error(&mut regs, "EXIT", "underflow");
    }
}

#[test]
fn misused_words_are_rejected() {
    unsafe {
        let mut regs = boot(io::sink());
        assert_error(&mut regs, ";", "while compiling");
        assert_error(&mut regs, ":", "needs a name");
        assert_error(&mut regs, "CREATE", "needs a name");
        assert_error(&mut regs, "LIT", "not recognized");
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
        assert_error(&mut regs, "1 . +", "underflow");
        assert_eq!(output.text(), "1 ");
    }
}

#[test]
fn underflow_in_dot_writes_nothing() {
    unsafe {
        let output = Capture::default();
        let mut regs = boot(output.clone());
        assert_error(&mut regs, ".", "underflow");
        assert_eq!(output.text(), "");
    }
}

#[test]
fn failing_writer_is_reported() {
    unsafe {
        let mut regs = boot(FailingWriter);
        assert_error(&mut regs, "1 .", "Output error");
        outer_interpreter(&mut regs, "1 2 +").unwrap();
    }
}

#[test]
fn failing_flush_is_reported() {
    unsafe {
        let mut regs = boot(BufWriter::new(FailingWriter));
        assert_error(&mut regs, "1 .", "Output error");
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
