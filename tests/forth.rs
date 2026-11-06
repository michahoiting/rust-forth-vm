// Copyright (c) 2026 M.W.R. Hoiting

//! Tests of Forth behaviour, using only the public API. Results are checked
//! through `.` and the output writer.

use std::cell::RefCell;
use std::error::Error;
use std::io::{self, BufWriter, Write};
use std::mem;
use std::rc::Rc;

use rust_forth_vm::{boot, outer_interpreter, Cell, ForthError, Registers};

const TEST_CELLS: usize = 1024;

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

// Boots a VM with a dictionary that is large enough for every test.
unsafe fn test_vm(output: impl Write + 'static) -> Registers {
    boot(output, TEST_CELLS).unwrap()
}

// Returns the size of the smallest dictionary that can hold the primitives.
unsafe fn smallest_cells() -> usize {
    (0..TEST_CELLS)
        .find(|cells| boot(io::sink(), *cells).is_ok())
        .unwrap()
}

// Evaluates the lines in order, and returns everything they wrote.
fn output_of(lines: &[&str]) -> String {
    unsafe {
        let output = Capture::default();
        let mut regs = test_vm(output.clone());
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
        let mut regs = test_vm(output.clone());
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
        let mut regs = test_vm(output.clone());
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
        ": FIXED CREATE , DOES> @ ;",
        "42 FIXED ANSWER",
        "7 FIXED LUCKY",
        "ANSWER . LUCKY . ANSWER .",
    ];
    assert_eq!(output_of(&lines), "42 7 42 ");
}

#[test]
fn does_requires_a_definition() {
    unsafe {
        let mut regs = test_vm(io::sink());
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
        let mut regs = test_vm(io::sink());
        outer_interpreter(&mut regs, ": BAD 1 DOES> ;").unwrap();
        assert_error!(&mut regs, "BAD", ForthError::DoesWithoutCreate);
    }
}

#[test]
fn failed_definition_leaves_a_usable_vm() {
    unsafe {
        let output = Capture::default();
        let mut regs = test_vm(output.clone());
        assert_error!(&mut regs, ": BROKEN 1 NOPE ;", ForthError::UnknownWord(name) if name == "NOPE");
        assert_error!(&mut regs, "BROKEN", ForthError::UnknownWord(name) if name == "BROKEN");
        outer_interpreter(&mut regs, "1 2 + .").unwrap();
        assert_eq!(output.text(), "3 ");
    }
}

#[test]
fn stack_underflow_is_reported() {
    unsafe {
        let mut regs = test_vm(io::sink());
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
        let mut regs = test_vm(io::sink());
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
        let mut regs = test_vm(BufWriter::new(output.clone()));
        assert_error!(&mut regs, "1 . +", ForthError::DataStackUnderflow);
        assert_eq!(output.text(), "1 ");
    }
}

#[test]
fn underflow_in_dot_writes_nothing() {
    unsafe {
        let output = Capture::default();
        let mut regs = test_vm(output.clone());
        assert_error!(&mut regs, ".", ForthError::DataStackUnderflow);
        assert_eq!(output.text(), "");
    }
}

#[test]
fn failing_writer_is_reported() {
    unsafe {
        let mut regs = test_vm(FailingWriter);
        assert_error!(&mut regs, "1 .", ForthError::Output(_));
        outer_interpreter(&mut regs, "1 2 +").unwrap();
    }
}

#[test]
fn failing_flush_is_reported() {
    unsafe {
        let mut regs = test_vm(BufWriter::new(FailingWriter));
        assert_error!(&mut regs, "1 .", ForthError::Output(_));
    }
}

#[test]
fn buffered_output_is_flushed_at_the_end_of_a_line() {
    unsafe {
        let output = Capture::default();
        let mut regs = test_vm(BufWriter::new(output.clone()));
        outer_interpreter(&mut regs, "7 .").unwrap();
        assert_eq!(output.text(), "7 ");
    }
}

#[test]
fn output_error_keeps_its_cause() {
    unsafe {
        let mut regs = test_vm(FailingWriter);
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
        (
            ForthError::DictionaryTooLarge(42),
            "Cannot allocate a dictionary of 42 Cells.",
        ),
        (ForthError::InvalidAddress(0x2a), "Invalid address 0x2a."),
        (
            ForthError::InvalidExecutionToken(0x2a),
            "Invalid execution token 0x2a.",
        ),
        (
            ForthError::UnterminatedComment,
            "Comment has no closing parenthesis.",
        ),
        (ForthError::Output(io::Error::other("x")), "Output error."),
    ];
    for (error, message) in cases {
        assert_eq!(error.to_string(), message);
    }
}

#[test]
fn fetch_rejects_addresses_outside_the_dictionary() {
    unsafe {
        let mut regs = test_vm(io::sink());
        assert_error!(&mut regs, "0 @", ForthError::InvalidAddress(0));
        assert_error!(&mut regs, "5 @", ForthError::InvalidAddress(5));
        assert_error!(&mut regs, "-1 @", ForthError::InvalidAddress(Cell::MAX));
    }
}

#[test]
fn fetch_rejects_misaligned_addresses() {
    unsafe {
        let mut regs = test_vm(io::sink());
        for offset in 1..mem::size_of::<Cell>() {
            let line = format!("CREATE X 7 , X {} + @", offset);
            assert_error!(&mut regs, &line, ForthError::InvalidAddress(_));
        }
    }
}

#[test]
fn fetch_reads_aligned_cells_behind_the_first() {
    let line = format!("CREATE X 1 , 2 , X {} + @ .", mem::size_of::<Cell>());
    assert_eq!(output_of(&[&line]), "2 ");
}

#[test]
fn fetch_rejects_here() {
    unsafe {
        let mut regs = test_vm(io::sink());
        assert_error!(&mut regs, "CREATE X X @", ForthError::InvalidAddress(_));
    }
}

#[test]
fn failed_definition_invalidates_its_addresses() {
    unsafe {
        let mut regs = test_vm(io::sink());
        outer_interpreter(&mut regs, "CREATE X 7 ,").unwrap();
        assert_error!(&mut regs, ": BROKEN NOPE ;", ForthError::UnknownWord(_));

        // The definition wrote behind the data of X, and is gone again.
        let line = format!("X {} + @", mem::size_of::<Cell>());
        assert_error!(&mut regs, &line, ForthError::InvalidAddress(_));
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
        let mut regs = test_vm(output.clone());
        assert_error!(&mut regs, "5 @", ForthError::InvalidAddress(5));
        outer_interpreter(&mut regs, "1 2 + .").unwrap();
        assert_eq!(output.text(), "3 ");
    }
}

#[test]
fn store_writes_a_cell() {
    assert_eq!(output_of(&["CREATE X 0 , 42 X ! X @ ."]), "42 ");
}

#[test]
fn store_writes_aligned_cells_behind_the_first() {
    let size = mem::size_of::<Cell>();
    let line = format!("CREATE X 1 , 2 , 9 X {size} + ! X @ . X {size} + @ .");
    assert_eq!(output_of(&[&line]), "1 9 ");
}

#[test]
fn store_changes_a_does_child() {
    let lines = [": VAR CREATE , DOES> ;", "5 VAR V", "9 V !", "V @ ."];
    assert_eq!(output_of(&lines), "9 ");
}

#[test]
fn store_rejects_addresses_outside_the_dictionary() {
    unsafe {
        let mut regs = test_vm(io::sink());
        assert_error!(&mut regs, "1 0 !", ForthError::InvalidAddress(0));
        assert_error!(&mut regs, "1 5 !", ForthError::InvalidAddress(5));
        assert_error!(&mut regs, "1 -1 !", ForthError::InvalidAddress(Cell::MAX));
    }
}

#[test]
fn store_rejects_misaligned_addresses() {
    unsafe {
        let mut regs = test_vm(io::sink());
        for offset in 1..mem::size_of::<Cell>() {
            let line = format!("CREATE X 7 , 9 X {} + !", offset);
            assert_error!(&mut regs, &line, ForthError::InvalidAddress(_));
        }
    }
}

#[test]
fn store_rejects_here() {
    unsafe {
        let mut regs = test_vm(io::sink());
        assert_error!(&mut regs, "CREATE X 5 X !", ForthError::InvalidAddress(_));
    }
}

#[test]
fn store_needs_two_stack_items() {
    unsafe {
        let output = Capture::default();
        let mut regs = test_vm(output.clone());
        outer_interpreter(&mut regs, "CREATE X 7 ,").unwrap();
        assert_error!(&mut regs, "X !", ForthError::DataStackUnderflow);
        outer_interpreter(&mut regs, "X @ .").unwrap();
        assert_eq!(output.text(), "7 ");
    }
}

#[test]
fn failed_definition_invalidates_its_addresses_for_store() {
    unsafe {
        let mut regs = test_vm(io::sink());
        outer_interpreter(&mut regs, "CREATE X 7 ,").unwrap();
        assert_error!(&mut regs, ": BROKEN NOPE ;", ForthError::UnknownWord(_));

        // The definition wrote behind the data of X, and is gone again.
        let line = format!("9 X {} + !", mem::size_of::<Cell>());
        assert_error!(&mut regs, &line, ForthError::InvalidAddress(_));
    }
}

#[test]
fn invalid_store_leaves_a_usable_vm() {
    unsafe {
        let output = Capture::default();
        let mut regs = test_vm(output.clone());
        assert_error!(&mut regs, "1 5 !", ForthError::InvalidAddress(5));
        outer_interpreter(&mut regs, "1 2 + .").unwrap();
        assert_eq!(output.text(), "3 ");
    }
}

#[test]
fn larger_dictionary_holds_more_cells() {
    unsafe {
        let mut regs = boot(io::sink(), TEST_CELLS * 4).unwrap();
        let cells = "1 , ".repeat(TEST_CELLS * 2);
        outer_interpreter(&mut regs, &cells).unwrap();
    }
}

#[test]
fn smaller_dictionary_overflows_sooner() {
    unsafe {
        let output = Capture::default();
        let mut regs = boot(output.clone(), smallest_cells() + 2).unwrap();
        outer_interpreter(&mut regs, "1 , 2 ,").unwrap();
        assert_error!(&mut regs, "3 ,", ForthError::DictionaryOverflow);
        outer_interpreter(&mut regs, "1 2 + .").unwrap();
        assert_eq!(output.text(), "3 ");
    }
}

#[test]
fn boot_rejects_an_empty_dictionary() {
    unsafe {
        assert!(matches!(
            boot(io::sink(), 0),
            Err(ForthError::DictionaryOverflow)
        ));
    }
}

#[test]
fn smallest_dictionary_boots_and_runs_forth() {
    unsafe {
        let smallest = smallest_cells();
        assert!(matches!(
            boot(io::sink(), smallest - 1),
            Err(ForthError::DictionaryOverflow)
        ));

        let output = Capture::default();
        let mut regs = boot(output.clone(), smallest).unwrap();
        outer_interpreter(&mut regs, "1 2 + .").unwrap();
        assert_eq!(output.text(), "3 ");
    }
}

#[test]
fn boot_rejects_a_dictionary_too_large_to_allocate() {
    unsafe {
        assert!(matches!(
            boot(io::sink(), Cell::MAX),
            Err(ForthError::DictionaryTooLarge(Cell::MAX))
        ));
    }
}

#[test]
fn variable_starts_at_zero() {
    assert_eq!(output_of(&["VARIABLE X X @ ."]), "0 ");
}

#[test]
fn variable_stores_and_fetches() {
    let lines = ["VARIABLE X", "42 X ! X @ .", "X @ 1 + X !", "X @ ."];
    assert_eq!(output_of(&lines), "42 43 ");
}

#[test]
fn variables_are_independent() {
    let lines = ["VARIABLE X VARIABLE Y", "1 X ! 2 Y !", "X @ . Y @ ."];
    assert_eq!(output_of(&lines), "1 2 ");
}

#[test]
fn variable_works_in_a_colon_word() {
    let lines = ["VARIABLE X", ": BUMP X @ 1 + X ! ;", "BUMP BUMP X @ ."];
    assert_eq!(output_of(&lines), "2 ");
}

#[test]
fn constant_pushes_its_value() {
    assert_eq!(output_of(&["42 CONSTANT ANSWER", "ANSWER ."]), "42 ");
}

#[test]
fn constant_keeps_a_negative_value() {
    assert_eq!(output_of(&["-5 CONSTANT N", "N ."]), "-5 ");
}

#[test]
fn constant_takes_only_the_top_cell() {
    assert_eq!(output_of(&["1 2 CONSTANT TWO", "TWO . ."]), "2 1 ");
}

#[test]
fn constants_are_independent() {
    let lines = ["1 CONSTANT A 2 CONSTANT B", "A . B . A ."];
    assert_eq!(output_of(&lines), "1 2 1 ");
}

#[test]
fn constant_works_in_a_colon_word() {
    let lines = ["42 CONSTANT ANSWER", ": NEXT-UP ANSWER 1 + ;", "NEXT-UP ."];
    assert_eq!(output_of(&lines), "43 ");
}

#[test]
fn constant_works_in_a_defining_word() {
    let lines = [": FIVES 5 CONSTANT ;", "FIVES A FIVES B", "A . B ."];
    assert_eq!(output_of(&lines), "5 5 ");
}

#[test]
fn constant_can_be_redefined_by_a_forth_word() {
    let lines = [": CONSTANT CREATE , DOES> @ ;", "42 CONSTANT A", "A ."];
    assert_eq!(output_of(&lines), "42 ");
}

#[test]
fn does_cannot_change_a_constant() {
    unsafe {
        let output = Capture::default();
        let mut regs = test_vm(output.clone());
        outer_interpreter(&mut regs, ": MAKE 7 CONSTANT DOES> 8 ;").unwrap();
        assert_error!(&mut regs, "MAKE X", ForthError::DoesWithoutCreate);
        outer_interpreter(&mut regs, "X .").unwrap();
        assert_eq!(output.text(), "7 ");
    }
}

#[test]
fn constant_needs_a_value_and_a_name() {
    unsafe {
        let output = Capture::default();
        let mut regs = test_vm(output.clone());
        assert_error!(&mut regs, "CONSTANT X", ForthError::DataStackUnderflow);
        assert_error!(&mut regs, "CONSTANT", ForthError::DataStackUnderflow);
        assert_error!(&mut regs, "X", ForthError::UnknownWord(_));
        assert_error!(&mut regs, "5 CONSTANT", ForthError::MissingName("CONSTANT"));
        outer_interpreter(&mut regs, "1 2 + .").unwrap();
        assert_eq!(output.text(), "3 ");
    }
}

#[test]
fn constant_that_does_not_fit_leaves_no_word() {
    unsafe {
        // The last size that fails has room for the header, but not for the value.
        let smallest = smallest_cells();
        for cells in smallest..TEST_CELLS {
            let output = Capture::default();
            let mut regs = boot(output.clone(), cells).unwrap();
            match outer_interpreter(&mut regs, "1 CONSTANT X") {
                Ok(()) => {
                    assert!(cells > smallest);
                    outer_interpreter(&mut regs, "X .").unwrap();
                    assert_eq!(output.text(), "1 ");
                    return;
                }
                Err(error) => {
                    assert!(matches!(error, ForthError::DictionaryOverflow));
                    assert_error!(&mut regs, "X", ForthError::UnknownWord(_));
                }
            }
        }
        panic!("no dictionary size was large enough");
    }
}

#[test]
fn variable_needs_a_name() {
    unsafe {
        let mut regs = test_vm(io::sink());
        assert_error!(&mut regs, "VARIABLE", ForthError::MissingName("VARIABLE"));
        assert_error!(&mut regs, "CREATE", ForthError::MissingName("CREATE"));
    }
}

#[test]
fn variable_overflows_a_full_dictionary() {
    unsafe {
        let output = Capture::default();
        let mut regs = boot(output.clone(), smallest_cells()).unwrap();
        assert_error!(&mut regs, "VARIABLE X", ForthError::DictionaryOverflow);
        outer_interpreter(&mut regs, "1 2 + .").unwrap();
        assert_eq!(output.text(), "3 ");
    }
}

#[test]
fn dup_copies_the_top_cell() {
    assert_eq!(output_of(&["1 2 DUP . . ."]), "2 2 1 ");
}

#[test]
fn question_dup_copies_only_non_zero_cells() {
    assert_eq!(output_of(&["5 ?DUP . ."]), "5 5 ");
    assert_eq!(output_of(&["7 0 ?DUP . ."]), "0 7 ");
}

#[test]
fn drop_removes_the_top_cell() {
    assert_eq!(output_of(&["1 2 DROP ."]), "1 ");
}

#[test]
fn swap_exchanges_the_top_two_cells() {
    assert_eq!(output_of(&["1 2 SWAP . ."]), "1 2 ");
}

#[test]
fn over_copies_the_second_cell() {
    assert_eq!(output_of(&["3 4 OVER . . ."]), "3 4 3 ");
}

#[test]
fn rot_moves_the_third_cell_to_the_top() {
    assert_eq!(output_of(&["1 2 3 ROT . . ."]), "1 3 2 ");
}

#[test]
fn pick_copies_the_nth_cell() {
    assert_eq!(output_of(&["10 20 30 2 PICK . . . ."]), "10 30 20 10 ");
}

#[test]
fn roll_moves_the_nth_cell_to_the_top() {
    // DEPTH shows that the Cell was moved, not copied.
    assert_eq!(output_of(&["1 2 3 2 ROLL . . . DEPTH ."]), "1 3 2 0 ");
}

#[test]
fn depth_counts_the_cells() {
    assert_eq!(output_of(&["DEPTH ."]), "0 ");
    assert_eq!(output_of(&["1 2 3 DEPTH . . . ."]), "3 3 2 1 ");
}

#[test]
fn stack_words_work_in_colon_definitions() {
    let cases = [
        (": W DUP + ;", "21 W .", "42 "),
        (": W ?DUP ;", "5 W . .", "5 5 "),
        (": W DROP ;", "1 2 W .", "1 "),
        (": W SWAP ;", "1 2 W . .", "1 2 "),
        (": W OVER ;", "3 4 W . . .", "3 4 3 "),
        (": W ROT ;", "1 2 3 W . . .", "1 3 2 "),
        (": W 1 PICK ;", "3 4 W . . .", "3 4 3 "),
        (": W 2 ROLL ;", "1 2 3 W . . .", "1 3 2 "),
        (": W DEPTH ;", "9 W . .", "1 9 "),
    ];
    for (definition, line, expected) in cases {
        assert_eq!(output_of(&[definition, line]), expected);
    }
}

#[test]
fn stack_words_can_add_to_a_variable() {
    let lines = [
        "VARIABLE X",
        ": +! SWAP OVER @ + SWAP ! ;",
        "40 X ! 2 X +!",
        "X @ .",
    ];
    assert_eq!(output_of(&lines), "42 ");
}

#[test]
fn stack_words_report_underflow() {
    unsafe {
        let output = Capture::default();
        let mut regs = test_vm(output.clone());
        for word in ["DUP", "?DUP", "DROP", "SWAP", "OVER", "ROT", "PICK", "ROLL"] {
            assert_error!(&mut regs, word, ForthError::DataStackUnderflow);
        }
        outer_interpreter(&mut regs, "1 2 + .").unwrap();
        assert_eq!(output.text(), "3 ");
    }
}

#[test]
fn stack_words_report_underflow_when_one_cell_short() {
    unsafe {
        let mut regs = test_vm(io::sink());
        for line in ["1 SWAP", "1 OVER", "1 2 ROT"] {
            assert_error!(&mut regs, line, ForthError::DataStackUnderflow);
        }
    }
}

#[test]
fn pick_and_roll_reject_an_n_equal_to_the_depth() {
    unsafe {
        let mut regs = test_vm(io::sink());
        assert_error!(&mut regs, "1 2 2 PICK", ForthError::DataStackUnderflow);
        assert_error!(&mut regs, "1 2 2 ROLL", ForthError::DataStackUnderflow);
    }
}

#[test]
fn pick_and_roll_reject_a_negative_n() {
    unsafe {
        let mut regs = test_vm(io::sink());
        assert_error!(&mut regs, "1 -1 PICK", ForthError::DataStackUnderflow);
        assert_error!(&mut regs, "1 -1 ROLL", ForthError::DataStackUnderflow);
    }
}

#[test]
fn word_is_only_found_after_its_definition_ends() {
    unsafe {
        let output = Capture::default();
        let mut regs = test_vm(output.clone());
        assert_error!(&mut regs, ": X X ;", ForthError::UnknownWord(name) if name == "X");
        outer_interpreter(&mut regs, ": X 7 ; X .").unwrap();
        assert_eq!(output.text(), "7 ");
    }
}

#[test]
fn noname_leaves_an_xt_that_execute_runs() {
    assert_eq!(output_of(&[":NONAME 1 2 + ; EXECUTE ."]), "3 ");
}

#[test]
fn noname_leaves_only_the_xt_on_the_stack() {
    assert_eq!(output_of(&[":NONAME ; DEPTH ."]), "1 ");
}

#[test]
fn execute_passes_cells_through_the_stack() {
    assert_eq!(output_of(&["5 :NONAME DUP + ; EXECUTE ."]), "10 ");
}

#[test]
fn execute_consumes_the_xt() {
    assert_eq!(output_of(&[":NONAME ; EXECUTE DEPTH ."]), "0 ");
}

#[test]
fn xt_works_from_a_variable() {
    let lines = ["VARIABLE V", ":NONAME 42 ; V !", "V @ EXECUTE ."];
    assert_eq!(output_of(&lines), "42 ");
}

#[test]
fn xt_works_from_a_constant() {
    let lines = [":NONAME 3 + ; CONSTANT ADD3", "4 ADD3 EXECUTE ."];
    assert_eq!(output_of(&lines), "7 ");
}

#[test]
fn xt_works_from_a_colon_word() {
    let lines = [
        "VARIABLE V",
        ":NONAME 42 ; V !",
        ": RUN V @ EXECUTE ;",
        "RUN .",
    ];
    assert_eq!(output_of(&lines), "42 ");
}

#[test]
fn xt_works_as_a_callback() {
    let lines = [
        ": TWICE DUP ROT SWAP EXECUTE SWAP EXECUTE ;",
        "5 :NONAME DUP + ; TWICE .",
    ];
    assert_eq!(output_of(&lines), "20 ");
}

#[test]
fn replacing_an_xt_changes_behaviour_at_runtime() {
    let lines = [
        "VARIABLE V",
        ": RUN V @ EXECUTE ;",
        ":NONAME 1 ; V ! RUN .",
        ":NONAME 2 ; V ! RUN .",
    ];
    assert_eq!(output_of(&lines), "1 2 ");
}

#[test]
fn noname_definitions_stay_separate() {
    let line = ":NONAME 1 ; :NONAME 2 ; EXECUTE . EXECUTE .";
    assert_eq!(output_of(&[line]), "2 1 ");
}

#[test]
fn noname_can_span_lines() {
    assert_eq!(output_of(&[":NONAME 1", "2 + ;", "EXECUTE ."]), "3 ");
}

#[test]
fn failed_noname_clears_the_xt_and_leaves_a_usable_vm() {
    unsafe {
        let output = Capture::default();
        let mut regs = test_vm(output.clone());
        assert_error!(&mut regs, ":NONAME NOPE ;", ForthError::UnknownWord(_));
        outer_interpreter(&mut regs, "DEPTH . 1 2 + .").unwrap();
        assert_eq!(output.text(), "0 3 ");
    }
}

#[test]
fn execute_rejects_cells_that_are_not_execution_tokens() {
    unsafe {
        let mut regs = test_vm(io::sink());
        assert_error!(&mut regs, "0 EXECUTE", ForthError::InvalidExecutionToken(0));
        assert_error!(&mut regs, "5 EXECUTE", ForthError::InvalidExecutionToken(5));
        assert_error!(
            &mut regs,
            "-1 EXECUTE",
            ForthError::InvalidExecutionToken(Cell::MAX)
        );
    }
}

#[test]
fn execute_reports_underflow() {
    unsafe {
        let mut regs = test_vm(io::sink());
        assert_error!(&mut regs, "EXECUTE", ForthError::DataStackUnderflow);
    }
}

#[test]
fn execute_rejects_a_pending_definition() {
    unsafe {
        let output = Capture::default();
        let mut regs = test_vm(output.clone());
        outer_interpreter(&mut regs, ": N :NONAME EXECUTE ;").unwrap();
        assert_error!(&mut regs, "N", ForthError::InvalidExecutionToken(_));
        outer_interpreter(&mut regs, "1 2 + .").unwrap();
        assert_eq!(output.text(), "3 ");
    }
}

#[test]
fn immediate_word_runs_during_compilation_and_is_not_compiled() {
    unsafe {
        let output = Capture::default();
        let mut regs = test_vm(output.clone());
        outer_interpreter(&mut regs, ": A 99 . ; IMMEDIATE").unwrap();
        outer_interpreter(&mut regs, ": B A 1 ;").unwrap();
        assert_eq!(output.text(), "99 ");

        outer_interpreter(&mut regs, "B .").unwrap();
        assert_eq!(output.text(), "99 1 ");
    }
}

#[test]
fn normal_word_runs_only_when_called() {
    unsafe {
        let output = Capture::default();
        let mut regs = test_vm(output.clone());
        outer_interpreter(&mut regs, ": A 99 . ;").unwrap();
        outer_interpreter(&mut regs, ": B A 1 ;").unwrap();
        assert_eq!(output.text(), "");

        outer_interpreter(&mut regs, "B .").unwrap();
        assert_eq!(output.text(), "99 1 ");
    }
}

#[test]
fn immediate_word_runs_when_interpreted() {
    assert_eq!(output_of(&[": A 7 . ; IMMEDIATE", "A"]), "7 ");
}

#[test]
fn immediate_only_marks_the_newest_word() {
    let lines = [": A 1 . ;", ": B 2 . ; IMMEDIATE", ": C A B ;", "C"];
    assert_eq!(output_of(&lines), "2 1 ");
}

#[test]
fn immediate_applies_to_created_words() {
    // X runs during compilation and leaves its address, which Y then fetches from.
    let lines = ["CREATE X 7 , IMMEDIATE", ": Y X @ ;", "Y ."];
    assert_eq!(output_of(&lines), "7 ");
}

#[test]
fn immediate_applies_to_the_newest_word_after_a_failed_definition() {
    unsafe {
        let output = Capture::default();
        let mut regs = test_vm(output.clone());
        outer_interpreter(&mut regs, ": A 5 . ;").unwrap();
        assert_error!(&mut regs, ": B NOPE ;", ForthError::UnknownWord(_));
        outer_interpreter(&mut regs, "IMMEDIATE : C A ;").unwrap();
        assert_eq!(output.text(), "5 ");
    }
}

#[test]
fn immediate_word_can_use_the_stack_during_compilation() {
    let lines = [": PUSH 7 ; IMMEDIATE", ": C PUSH ;", ".", "C DEPTH ."];
    assert_eq!(output_of(&lines), "7 0 ");
}

#[test]
fn immediate_word_can_count_at_compile_time() {
    let lines = [
        "VARIABLE USES",
        ": COUNTED USES @ 1 + USES ! ; IMMEDIATE",
        ": FIRST COUNTED 1 ;",
        ": SECOND COUNTED COUNTED 2 ;",
        "USES @ .",
    ];
    assert_eq!(output_of(&lines), "3 ");
}

#[test]
fn error_in_an_immediate_word_rolls_back_the_definition() {
    unsafe {
        let output = Capture::default();
        let mut regs = test_vm(output.clone());
        outer_interpreter(&mut regs, ": BAD + ; IMMEDIATE").unwrap();
        assert_error!(&mut regs, ": C BAD ;", ForthError::DataStackUnderflow);
        assert_error!(&mut regs, "C", ForthError::UnknownWord(name) if name == "C");
        outer_interpreter(&mut regs, "1 2 + .").unwrap();
        assert_eq!(output.text(), "3 ");
    }
}

#[test]
fn immediate_keeps_a_noname_xt_working() {
    assert_eq!(output_of(&[":NONAME 5 ; IMMEDIATE EXECUTE ."]), "5 ");
}

#[test]
fn paren_comment_is_ignored_when_interpreting() {
    assert_eq!(output_of(&["1 ( ignored ) ."]), "1 ");
}

#[test]
fn paren_comment_is_ignored_in_a_definition() {
    assert_eq!(output_of(&[": A 1 ( ignored ) 2 + ;", "A ."]), "3 ");
}

#[test]
fn stack_effect_comment_documents_a_word() {
    assert_eq!(output_of(&[": ADD5 ( n -- n+5 ) 5 + ;", "2 ADD5 ."]), "7 ");
}

#[test]
fn comment_text_does_not_run() {
    assert_eq!(output_of(&["( 1 . NOPE ; ) 2 ."]), "2 ");
    assert_eq!(output_of(&[r"2 . \ 3 . NOPE ;"]), "2 ");
}

#[test]
fn empty_paren_comment_is_allowed() {
    assert_eq!(output_of(&["( ) 1 ."]), "1 ");
}

#[test]
fn comment_words_need_whitespace_after_them() {
    unsafe {
        let mut regs = test_vm(io::sink());
        assert_error!(
            &mut regs,
            "(comment)",
            ForthError::UnknownWord(name) if name == "(comment)"
        );
        assert_error!(
            &mut regs,
            r"\comment",
            ForthError::UnknownWord(name) if name == r"\comment"
        );
    }
}

#[test]
fn paren_comment_ends_at_the_first_closing_parenthesis() {
    unsafe {
        let mut regs = test_vm(io::sink());
        assert_error!(
            &mut regs,
            "( outer ( inner ) rest )",
            ForthError::UnknownWord(name) if name == "rest"
        );
    }
}

#[test]
fn paren_comment_can_span_lines_within_a_string() {
    assert_eq!(output_of(&["1 ( spans\nseveral\nlines ) ."]), "1 ");
}

#[test]
fn line_comment_stops_at_the_end_of_the_line() {
    assert_eq!(output_of(&["1 . \\ 2 .\n3 ."]), "1 3 ");
}

#[test]
fn line_comment_handles_windows_line_endings() {
    assert_eq!(output_of(&["1 . \\ 2 .\r\n3 ."]), "1 3 ");
}

#[test]
fn line_comment_can_end_the_input() {
    assert_eq!(output_of(&[r"1 . \"]), "1 ");
    assert_eq!(output_of(&[r"\"]), "");
}

#[test]
fn line_comment_is_ignored_in_a_definition() {
    assert_eq!(output_of(&[": A 1 \\ ignored\n2 + ;\nA ."]), "3 ");
}

#[test]
fn comments_can_hold_non_ascii_text() {
    assert_eq!(output_of(&["1 ( één ) . \\ één\n2 ."]), "1 2 ");
}

#[test]
fn definition_can_span_lines_within_a_string() {
    assert_eq!(output_of(&[": A\n    1\n    2 +\n;\nA ."]), "3 ");
}

#[test]
fn commented_program_runs_as_one_block() {
    let program = r"
\ Add to a variable
VARIABLE X
: +! ( n addr -- )
    SWAP OVER @ +  \ work out the new value
    SWAP !
;
40 X !
2 X +!
X @ .              \ prints 42
";
    assert_eq!(output_of(&[program]), "42 ");
}

#[test]
fn error_discards_the_rest_of_a_multi_line_string() {
    unsafe {
        let output = Capture::default();
        let mut regs = test_vm(output.clone());
        assert_error!(&mut regs, "1 .\nNOPE 2 .\n3 .", ForthError::UnknownWord(_));
        assert_eq!(output.text(), "1 ");
    }
}

#[test]
fn unterminated_comment_is_an_error() {
    unsafe {
        let output = Capture::default();
        let mut regs = test_vm(output.clone());
        assert_error!(
            &mut regs,
            "1 ( never closed",
            ForthError::UnterminatedComment
        );
        outer_interpreter(&mut regs, "DEPTH . 1 2 + .").unwrap();
        assert_eq!(output.text(), "0 3 ");
    }
}

#[test]
fn unterminated_comment_rolls_back_an_open_definition() {
    unsafe {
        let mut regs = test_vm(io::sink());
        assert_error!(
            &mut regs,
            ": A 1 ( never closed",
            ForthError::UnterminatedComment
        );
        assert_error!(&mut regs, "A", ForthError::UnknownWord(name) if name == "A");
    }
}
