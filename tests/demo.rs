// Copyright (c) 2026 M.W.R. Hoiting

//! Tests of the `forth-vm` program, which run its binary. Miri can't start processes, so they
//! don't run there.

#![cfg(not(miri))]

use std::io::Write;
use std::process::{Command, Output, Stdio};

/// The number of sections in the menu.
const SECTION_COUNT: usize = 15;

/// The number of the error handling section, the only one that should report errors.
const ERROR_SECTION: usize = 15;

/// The line that ends a section.
const FOOTER: &str = "-------------------------------------\n";

/// The first line of the menu.
const MENU: &str = "Demo sections:";

/// The end of the start-up text in interactive mode.
const INTERACTIVE_HINT: &str = "Press Ctrl-D to leave.\n\n";

/// The prompt of the menu.
const PROMPT: &str = "Enter a section number, Q to quit [0]: ";

/// The expected output of the menu with all sections, which `make demo-expected` updates.
const DEMO_OUTPUT: &str = include_str!("demo_output.txt");

/// The README, for its Forth example. Keep tests of Markdown out of what Miri runs: CI skips
/// Miri on PRs that only change Markdown, so such a test wouldn't run there.
const README: &str = include_str!("../README.md");

/// Runs the program with the given arguments and stdin.
fn run(arguments: &[&str], stdin: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_forth-vm"))
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(stdin).unwrap();
    child.wait_with_output().unwrap()
}

/// Runs the program, checks that it exited with code 0, and returns its stdout.
fn stdout_of(arguments: &[&str], stdin: &str) -> String {
    let output = run(arguments, stdin.as_bytes());
    assert_eq!(output.status.code(), Some(0));
    String::from_utf8(output.stdout).unwrap()
}

/// Runs the demo menu with the given stdin and returns its stdout.
fn menu(stdin: &str) -> String {
    stdout_of(&["--demo"], stdin)
}

/// Runs an interactive session with the given stdin, and returns its stdout after the
/// start-up text. A session ends with an empty line, for the end of the input.
fn session(stdin: &str) -> String {
    let output = stdout_of(&[], stdin);
    let (_, rest) = output.split_once(INTERACTIVE_HINT).unwrap();
    rest.to_string()
}

/// Returns the given section, numbered from 1, from the output of the menu. If it's there more
/// than once, this returns the first.
fn section(output: &str, number: usize) -> &str {
    let header = format!("--- TEST RUN {}: ", number);
    let start = output.find(&header).expect("the section must be there");
    let length = output[start..].find(FOOTER).expect("the section must end");
    &output[start..start + length + FOOTER.len()]
}

#[test]
fn a_line_prints_its_output_and_ok() {
    for arguments in [&[][..], &["--interactive"][..]] {
        let output = stdout_of(arguments, "40 2 + .\n");
        assert!(output.ends_with(&format!("{}42 ok\n\n", INTERACTIVE_HINT)));
        assert!(!output.contains(MENU));
    }
}

#[test]
fn an_empty_line_prints_ok() {
    assert_eq!(session("\n"), "ok\n\n");
}

#[test]
fn a_last_line_without_a_newline_is_evaluated() {
    assert_eq!(session("1 ."), "1 ok\n\n");
}

#[test]
fn definitions_and_the_stack_carry_over_between_lines() {
    assert_eq!(session(": ADD3 3 + ;\n4\nADD3 .\n"), "ok\nok\n7 ok\n\n");
}

#[test]
fn a_definition_can_span_lines_and_prints_ok_when_it_ends() {
    assert_eq!(session(": ADD3\n3 +\n;\n1 ADD3 .\n"), "ok\n4 ok\n\n");
}

#[test]
fn an_error_prints_no_ok_and_the_session_continues() {
    assert_eq!(
        session("1 NOPE\n.\n2 .\n"),
        "Forth Error: Word or numeric literal 'NOPE' not recognized.\n\
         Forth Error: Data stack underflow.\n\
         2 ok\n\n"
    );
}

#[test]
fn an_error_ends_an_open_definition() {
    assert_eq!(
        session(": X\nNOPE\n1 .\n"),
        "Forth Error: Word or numeric literal 'NOPE' not recognized.\n1 ok\n\n"
    );
}

#[test]
fn a_paren_comment_must_end_on_its_line() {
    assert_eq!(
        session("( open\n1 .\n"),
        "Forth Error: Comment has no closing parenthesis.\n1 ok\n\n"
    );
}

#[test]
fn quit_prints_no_ok_and_keeps_the_data_stack() {
    assert_eq!(session("7 1 . QUIT 2 .\n.\n"), "1 \n7 ok\n\n");
}

#[test]
fn quit_works_from_a_definition() {
    assert_eq!(
        session(": STOP 3 . QUIT 4 . ;\n8 STOP 5 .\n.\n"),
        "ok\n3 \n8 ok\n\n"
    );
}

#[test]
fn quit_does_not_end_the_session() {
    assert_eq!(session("QUIT\n1 .\n"), "\n1 ok\n\n");
}

#[test]
fn abort_prints_no_ok_and_clears_the_data_stack() {
    assert_eq!(session("7 1 . ABORT 2 .\nDEPTH .\n"), "1 \n0 ok\n\n");
}

#[test]
fn abort_works_from_a_definition() {
    assert_eq!(
        session(": GIVE-UP 3 . ABORT 4 . ;\n8 GIVE-UP 5 .\nDEPTH .\n"),
        "ok\n3 \n0 ok\n\n"
    );
}

#[test]
fn abort_does_not_end_the_session() {
    assert_eq!(session("ABORT\n1 .\n"), "\n1 ok\n\n");
}

#[test]
fn bye_exits_with_code_zero_and_keeps_prior_output() {
    let output = run(&[], b"40 2 + . BYE 99 .\n");
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.ends_with("42 "));
    assert!(!stdout.contains("99"));
}

#[test]
fn bye_exits_with_code_zero_from_a_colon_word() {
    let output = run(&[], b": STOP BYE ; STOP\n");
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn bye_with_code_exits_from_a_colon_word() {
    let output = run(&[], b": STOP 3 (BYE) 4 ; STOP\n");
    assert_eq!(output.status.code(), Some(3));
}

#[test]
fn bye_with_code_exits_with_the_given_code() {
    let output = run(&[], b"3 (BYE)\n");
    assert_eq!(output.status.code(), Some(3));
}

#[test]
fn bye_uses_only_the_low_byte_of_its_code() {
    let output = run(&[], b"259 (BYE)\n");
    assert_eq!(output.status.code(), Some(3));
}

#[test]
fn bye_with_code_underflow_reports_an_error_and_continues() {
    assert_eq!(
        session("(BYE)\n1 .\n"),
        "Forth Error: Data stack underflow.\n1 ok\n\n"
    );
}

#[test]
fn end_of_input_ends_the_session_with_code_zero() {
    for stdin in ["", "QUIT\n", "ABORT\n", ": OPEN\n"] {
        stdout_of(&[], stdin);
    }
}

#[test]
fn menu_lists_every_section() {
    let output = menu("q\n");
    assert!(output.contains("   0. All sections\n"));
    assert!(output.contains("   1. Evaluating Text Operations\n"));
    assert!(output.contains("  14. Returning to the Interpreter with QUIT and ABORT\n"));
    assert!(output.contains("  15. Error Handling\n"));
    assert!(output.ends_with(PROMPT));
}

#[test]
fn quitting_the_menu_runs_no_section() {
    for stdin in ["q\n", "Q\n", ""] {
        let output = menu(stdin);
        assert!(output.contains(MENU));
        assert!(!output.contains("--- TEST RUN "));
    }
}

#[test]
fn enter_and_zero_run_all_sections_in_order() {
    let enter = menu("\nq\n");
    assert_eq!(enter.matches("--- TEST RUN ").count(), SECTION_COUNT);
    for number in 1..SECTION_COUNT {
        assert!(enter.contains(section(&enter, number)));
        let this = enter.find(section(&enter, number)).unwrap();
        let next = enter.find(section(&enter, number + 1)).unwrap();
        assert!(this < next);
    }
    assert_eq!(menu("0\nq\n"), enter);
}

#[test]
fn each_section_runs_alone_as_it_does_among_all_sections() {
    let all = menu("\nq\n");
    for number in 1..=SECTION_COUNT {
        let output = menu(&format!("{}\nq\n", number));
        assert_eq!(section(&output, number), section(&all, number));
        assert_eq!(output.matches("--- TEST RUN ").count(), 1);
    }
}

#[test]
fn only_the_error_section_reports_errors() {
    for number in 1..=SECTION_COUNT {
        let output = menu(&format!("{}\nq\n", number));
        assert_eq!(output.contains("Forth Error"), number == ERROR_SECTION);
    }
}

#[test]
fn a_section_gives_the_same_output_each_time() {
    let output = menu("11\n11\nq\n");
    assert_eq!(output.matches(section(&output, 11)).count(), 2);
}

#[test]
fn the_menu_comes_back_after_a_run() {
    let output = menu("1\n2\nq\n");
    assert_eq!(output.matches(MENU).count(), 3);
    assert_eq!(output.matches("--- TEST RUN ").count(), 2);
}

#[test]
fn an_invalid_choice_shows_the_menu_again() {
    let output = menu("16\nabc\nq\n");
    assert!(output.contains("Invalid choice: 16\n"));
    assert!(output.contains("Invalid choice: abc\n"));
    assert_eq!(output.matches(MENU).count(), 3);
    assert!(!output.contains("--- TEST RUN "));
}

#[test]
fn input_that_cannot_be_read_fails_without_a_panic() {
    for arguments in [&[][..], &["--demo"][..]] {
        let output = run(arguments, &[0xff, b'\n']);
        assert_eq!(output.status.code(), Some(1));
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(
            stderr.starts_with("Cannot read the input: "),
            "got: {stderr}"
        );
    }
}

/// Returns whether the text is a date in the form `YYYY-MM-DD`.
fn is_date(text: &str) -> bool {
    let digits = |part: &str| part.bytes().all(|byte| byte.is_ascii_digit());
    let parts: Vec<&str> = text.split('-').collect();
    parts.iter().map(|part| part.len()).eq([4, 2, 2]) && parts.into_iter().all(digits)
}

/// Returns whether the git part of the version line has a valid form. The test can't compare
/// the tag or the hash, because they change, and CI has no tags.
fn is_git_state(text: &str) -> bool {
    let is_hash = |hash: &str| hash.len() >= 7 && hash.bytes().all(|b| b.is_ascii_hexdigit());
    match text.split(' ').collect::<Vec<_>>()[..] {
        ["unknown", "unknown"] => true,
        ["none", hash, "(dirty)"] => is_hash(hash),
        [tag, hash] | [tag, hash, "(dirty)"] => tag.starts_with('v') && is_hash(hash),
        _ => false,
    }
}

#[test]
fn version_prints_what_the_program_was_built_from() {
    let output = run(&["--version"], b"");
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let line = stdout.strip_suffix('\n').unwrap();
    assert!(!line.contains('\n'), "{}", stdout);

    let prefix = format!("Rust Forth VM {} (Built ", env!("CARGO_PKG_VERSION"));
    let build = line.strip_prefix(&prefix).unwrap();
    let (date, git) = build
        .strip_suffix(')')
        .unwrap()
        .split_once(", Git ")
        .unwrap();
    assert!(is_date(date), "{}", date);
    assert!(is_git_state(git), "{}", git);
}

#[test]
fn version_is_not_accepted_with_other_arguments() {
    let output = run(&["--version", "--demo"], b"");
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
}

#[test]
fn unknown_arguments_print_the_usage() {
    for arguments in [&["--nope"][..], &["--demo", "1"][..]] {
        let output = run(arguments, b"");
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert_eq!(
            String::from_utf8(output.stderr).unwrap(),
            "Usage: forth-vm [--interactive | --demo | --version]\n"
        );
    }
}

/// Says whether a word is a whole number, with an optional minus sign.
fn is_number(word: &str) -> bool {
    let digits = word.strip_prefix('-').unwrap_or(word);
    !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
}

/// Returns the Forth example from the README.
fn readme_example() -> &'static str {
    let (_, rest) = README
        .split_once("```forth\n")
        .expect("the README must have an example");
    let (example, _) = rest.split_once("```").expect("the example must end");
    example
}

/// Returns what a line of the README's example prints. A comment that starts with a number
/// states the result of the line, and other lines only print `ok`.
fn readme_line_output(line: &str) -> String {
    let result = line
        .split_once('\\')
        .and_then(|(_, comment)| comment.split_whitespace().next())
        .map(|word| word.trim_end_matches(','))
        .filter(|word| is_number(word));
    match result {
        Some(number) => format!("{} ok\n", number),
        None => "ok\n".to_string(),
    }
}

#[test]
fn demo_output_matches_the_expected_file() {
    assert_eq!(
        menu("0\nq\n"),
        DEMO_OUTPUT,
        "run `make demo-expected` if this change to the demo is intended"
    );
}

#[test]
fn readme_example_gives_the_results_it_states() {
    let example = readme_example();
    let expected: String = example.lines().map(readme_line_output).collect();
    assert_eq!(session(example), format!("{}\n", expected));
}

#[test]
fn readme_line_output_reads_the_comment() {
    assert_eq!(readme_line_output(r"40 2 + .   \ 42"), "42 ok\n");
    assert_eq!(readme_line_output(r"-5 3 + .   \ -2"), "-2 ok\n");
    assert_eq!(
        readme_line_output(r": A B ;    \ 99, printed while compiling"),
        "99 ok\n"
    );
    assert_eq!(
        readme_line_output(r": A 5 ;    \ a stack effect comment"),
        "ok\n"
    );
    assert_eq!(readme_line_output(": A 5 ;"), "ok\n");
    assert_eq!(readme_line_output(""), "ok\n");
}
