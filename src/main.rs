// Copyright (c) 2026 M.W.R. Hoiting

//! An interactive Forth prompt, and a scripted demo of the Forth VM.
//!
//! By default, or with `--interactive`, the program evaluates each line from stdin in one VM.
//! On a terminal, the lines can be edited, and there's a history.
//!
//! With `--demo`, it shows a menu of demo sections. Each section shows the normal use of some
//! words, and the last one shows how errors are handled.
//!
//! With `--version`, it prints what it was built from, and exits.

#[cfg(feature = "readline")]
mod line_editor;

use std::env;
#[cfg(feature = "readline")]
use std::io::IsTerminal;
use std::io::{self, BufRead, Write};
use std::process::ExitCode;
use std::slice;

use rust_forth_vm::{
    boot, outer_interpreter, quit_loop, ForthError, LineResult, Vm, VmConfig, COPYRIGHT,
};

#[cfg(feature = "readline")]
use line_editor::LineEditor;

/// The size of the interactive Forth dictionary.
const DICTIONARY_CELLS: usize = 64 * 1024;
/// The smaller dictionary used by the self-contained demo sections.
const DEMO_DICTIONARY_CELLS: usize = 1024;
/// The demo sections need only a small data stack.
const DEMO_DATA_STACK_CELLS: usize = 64;

/// The usage line, printed for arguments the program doesn't know.
const USAGE: &str = "Usage: forth-vm [--interactive | --demo | --version]";

/// What `--version` prints, and the first line of the interactive prompt. `build.rs` provides the build date and the git state.
const VERSION: &str = concat!(
    "Rust Forth VM ",
    env!("CARGO_PKG_VERSION"),
    " (",
    env!("BUILD_INFO"),
    ")"
);

/// The last line that the interactive prompt prints when it starts.
const MORE_INFORMATION: &str = r#"Type "HELP", "COPYRIGHT" or "LICENSE" for more information."#;

/// The demo sections, each with its title and one block of Forth source. Each section runs in a
/// new VM, so it must define everything it uses.
const SECTIONS: [(&str, &str); 17] = [
    (
        "Evaluating Text Operations",
        r"
40 2 +            \ leaves 42 on the stack
.                 \ prints 42
-5 3 + .          \ prints -2
",
    ),
    (
        "Dynamic Creation via Text Interpreter",
        r"
CREATE STATIONARY-SOURCE     \ creates a new word in the dictionary
1337 ,                       \ compiles 1337 into the data field of the new word
STATIONARY-SOURCE @ .        \ prints 1337
",
    ),
    (
        "Colon Definitions",
        r"
: ADD3 3 + ;
: ADD6 ADD3 ADD3 ;
36 ADD6 .         \ prints 42
",
    ),
    (
        "Constants",
        r"
42 CONSTANT ANSWER
7 CONSTANT LUCKY
ANSWER . LUCKY .              \ prints 42 7
40 2 + CONSTANT COMPUTED      \ the value can come from a calculation
COMPUTED .                    \ prints 42
: ANSWER+1 ANSWER 1 + ;       \ a constant used from a colon word
ANSWER+1 .                    \ prints 43
",
    ),
    (
        "Defining Words with DOES>",
        r"
: ADDER CREATE , DOES> @ + ;
3 ADDER PLUS3
10 ADDER PLUS10
1 PLUS3 . 1 PLUS10 .          \ prints 4 11
",
    ),
    (
        "Variables with ! and @",
        r"
VARIABLE COUNTER
COUNTER @ .                        \ prints 0: a new variable starts at zero
42 COUNTER !
COUNTER @ .                        \ prints 42
COUNTER @ 1 + COUNTER !            \ read, change, and write back
COUNTER @ .                        \ prints 43
: BUMP COUNTER @ 1 + COUNTER ! ;   \ a variable used from a colon word
BUMP BUMP COUNTER @ .              \ prints 45
VARIABLE OTHER                     \ each variable has its own Cell
7 OTHER !
OTHER @ . COUNTER @ .              \ prints 7 45
",
    ),
    (
        "Stack Words",
        r"
1 2 DROP .                    \ prints 1
1 2 SWAP . .                  \ prints 1 2
3 4 OVER . . .                \ prints 3 4 3
1 2 3 ROT . . .               \ prints 1 3 2
5 ?DUP . .                    \ prints 5 5
0 ?DUP .                      \ prints 0
10 20 30 2 PICK . . . .       \ prints 10 30 20 10
1 2 3 2 ROLL . . .            \ prints 1 3 2
1 2 3 DEPTH . . . .           \ prints 3 3 2 1
: DOUBLE DUP + ;              \ reuse a value without naming it
21 DOUBLE .                   \ prints 42
VARIABLE COUNTER
45 COUNTER !
5 COUNTER +!
COUNTER @ .                   \ prints 50
",
    ),
    (
        "Single-Cell Arithmetic",
        r"
9 4 - . 9 4 * .                \ prints 5 36
-7 3 / . -7 3 MOD .           \ prints -3 2: division rounds down
-7 3 /MOD . .                  \ prints -3 2: remainder, then quotient on the stack
41 1+ . 41 1- . 21 2* . -3 2/ .  \ prints 42 40 42 -2
-42 NEGATE . -42 ABS .         \ prints 42 42
-3 2 MIN . -3 2 MAX .          \ prints -3 2
",
    ),
    (
        "Comparison and Logic",
        r"
4 4 = . 2 3 < . 3 2 > . -1 0 U< .  \ prints -1 -1 -1 0
0 0= . -1 0< .                     \ prints -1 -1
12 10 AND . 12 10 OR . 12 10 XOR . \ prints 8 14 6
0 INVERT . 3 2 LSHIFT . 12 2 RSHIFT . \ prints -1 12 3
",
    ),
    (
        "Pairs and Memory",
        r"
1 2 3 4 2OVER . . . . . .          \ prints 2 1 4 3 2 1
1 2 3 4 2SWAP . . . .               \ prints 2 1 4 3
CREATE PAIR 2 CELLS ALLOT
11 22 PAIR 2! PAIR 2@ . .           \ prints 22 11
5 PAIR +! PAIR @ .                  \ prints 16
PAIR CELL+ PAIR - .                 \ prints the size of one Cell
CREATE BYTES 4 ALLOT
BYTES 4 65 FILL
BYTES BYTES CHAR+ 3 MOVE
BYTES C@ . BYTES CHAR+ C@ . BL .   \ prints 65 65 32
",
    ),
    (
        "Execution Tokens",
        r"
\ Run an unnamed definition straight away
:NONAME 1 2 + ; EXECUTE .     \ prints 3

\ Pass arguments and results through the stack
5 :NONAME DUP + ; EXECUTE .   \ prints 10

\ Keep an execution token under a name
:NONAME 3 + ; CONSTANT ADD3-XT
4 ADD3-XT EXECUTE .           \ prints 7

\ Pass behavior to a word, as a callback
: TWICE DUP ROT SWAP EXECUTE SWAP EXECUTE ;
5 :NONAME DUP + ; TWICE .     \ prints 20
1 ADD3-XT TWICE .             \ prints 7

\ Change what a word does at runtime, through a variable
VARIABLE ACTION
:NONAME 42 ; ACTION !
: RUN-ACTION ACTION @ EXECUTE ;
RUN-ACTION .                  \ prints 42
:NONAME 7 ; ACTION !
RUN-ACTION .                  \ prints 7
",
    ),
    (
        "Immediate Words",
        r"
\ An immediate word runs during compilation
: ANNOUNCE 99 . ; IMMEDIATE
: USES-ANNOUNCE ANNOUNCE 1 ;  \ prints 99 while compiling
USES-ANNOUNCE .               \ prints 1

\ A normal word only runs when called, for comparison
: QUIET 99 . ;
: USES-QUIET QUIET 1 ;        \ prints nothing while compiling
USES-QUIET .                  \ prints 99 1

\ Count something at compile time
VARIABLE USES
: COUNTED USES @ 1 + USES ! ; IMMEDIATE
: FIRST COUNTED 1 ;
: SECOND COUNTED COUNTED 2 ;
USES @ .                      \ prints 3

\ An immediate word still runs normally when interpreted
ANNOUNCE                      \ prints 99
",
    ),
    (
        "Comments",
        r"
\ A whole line can be a comment
1 2 + . ( a comment can sit in the middle of a line ) 4 .  \ prints 3 4
5 . \ the rest of this line is ignored: 6 .

( a parenthesis comment
  can span several lines )

: ADD5 ( n -- n+5 ) 5 + ;  \ a stack effect comment documents a word
2 ADD5 .                   \ prints 7

: PAIR ( n -- n n )
    DUP  \ keep a copy for later
;
3 PAIR . .                 \ prints 3 3

\ BYE leaves the program with code 0; uncomment one example to try it.
\ BYE
\ 3 (BYE)
",
    ),
    (
        "Reaching a Word's Parameter Field",
        r"
\ Some words to look into
: ADD3 3 + ;
VARIABLE COUNTER
50 COUNTER !
: ADDER CREATE , DOES> @ + ;
3 ADDER PLUS3

\ ' gives the execution token of a word, which :NONAME gives for an unnamed one
3 ' ADD3 EXECUTE .            \ prints 6

\ >BODY turns an execution token into the address of the word's Parameter Field
' COUNTER >BODY @ .           \ prints 50

\ A word made with DOES> doesn't push that address, so this is the way to its data
1 PLUS3 .                     \ prints 4
' PLUS3 >BODY @ .             \ prints 3
5 ' PLUS3 >BODY !
1 PLUS3 .                     \ prints 6

\ ['] does the same as ' inside a definition, using a name from the source
: SET-PLUS3 ['] PLUS3 >BODY ! ;
9 SET-PLUS3
1 PLUS3 .                     \ prints 10
: RUN-ADD3 ['] ADD3 EXECUTE ;
1 RUN-ADD3 .                  \ prints 4
",
    ),
    (
        "Bytes and Reserved Memory",
        r"
\ ALLOT reserves bytes after a word, and C! and C@ reach each one of them
CREATE LETTERS 3 CHARS ALLOT
72 LETTERS C!
105 LETTERS CHAR+ C!
LETTERS C@ .                  \ prints 72
LETTERS CHAR+ C@ .            \ prints 105
LETTERS CHAR+ CHAR+ C@ .      \ prints 0: ALLOT clears what it reserves

\ C, stores one byte at HERE, so a table can be built a byte at a time
CREATE PRIMES 2 C, 3 C, 5 C,
PRIMES CHAR+ CHAR+ C@ .       \ prints 5

\ A Cell needs an aligned address. ALIGN moves HERE to the next one
ALIGN HERE 99 , @ .           \ prints 99

\ ALIGNED gives that address without moving HERE, and , aligns HERE by itself
1 C, HERE ALIGNED 7 , @ .     \ prints 7

\ New words align HERE too, so they can follow bytes
1 C,
: SEVEN 7 ;
SEVEN .                       \ prints 7

\ CELLS gives the size of some Cells in bytes, which is how to index a table of them
CREATE TABLE 3 CELLS ALLOT
11 TABLE !
22 TABLE 1 CELLS + !
TABLE @ .                     \ prints 11
TABLE 1 CELLS + @ .           \ prints 22

\ A negative ALLOT gives back data of the newest word made by CREATE
CREATE SCRATCH 16 ALLOT
-16 ALLOT
",
    ),
    (
        "Compiling Calls from an Immediate Word",
        r"
\ COMPILE, appends a call to the word with the given execution token, to the open definition
: ADD1 1 + ;
: TWICE ['] ADD1 COMPILE, ['] ADD1 COMPILE, ; IMMEDIATE
: ADD2 TWICE ;                \ TWICE ran while ADD2 was compiled, so ADD2 calls ADD1 twice
5 ADD2 .                      \ prints 7

\ Any definition can use TWICE
: ADD4 TWICE TWICE ;
5 ADD4 .                      \ prints 9

\ , can't write into a definition, so an immediate word has to use COMPILE,
CREATE TABLE 5 , TABLE @ .    \ prints 5: , still works when no definition is open
",
    ),
    (
        "Program Information",
        r"
HELP                          \ prints how to leave the prompt
COPYRIGHT                     \ prints the copyright notice
LICENSE                       \ prints the license
",
    ),
];

/// The `QUIT` and `ABORT` examples. Both discard the rest of their text, so each one is a
/// separate block, followed by a block that shows what's left on the data stack.
const RESTART_TESTS: [&str; 8] = [
    r"7 1 . QUIT 2 .                  \ prints 1, and leaves 7 on the stack",
    r".                               \ prints 7",
    r": STOP 3 . QUIT 4 . ; 8 STOP 5 .  \ prints 3: QUIT also leaves a definition",
    r".                               \ prints 8",
    r"7 1 . ABORT 2 .                 \ prints 1, and clears the stack",
    r"DEPTH .                         \ prints 0",
    r": GIVE-UP 3 . ABORT 4 . ; 8 GIVE-UP 5 .  \ prints 3: ABORT also leaves a definition",
    r"DEPTH .                         \ prints 0",
];

/// The error handling tests. An error discards the rest of its text, so each one is a separate
/// block.
const ERROR_TESTS: [&str; 12] = [
    ": BROKEN 1 NO-SUCH-WORD ;",
    "BROKEN",
    "+",
    "1 5 !",
    "DROP",
    "9 PICK",
    "5 EXECUTE",
    "1 ( never closed",
    ": W 5 , ; IMMEDIATE",
    ": X W ;",
    "' DUP COMPILE,",
    "1 0 /",
];

/// The sections with several blocks of source, which run after [`SECTIONS`]. Each one has its
/// title and its blocks, which share a VM.
const BLOCK_SECTIONS: [(&str, &[&str]); 2] = [
    (
        "Returning to the Interpreter with QUIT and ABORT",
        &RESTART_TESTS,
    ),
    ("Error Handling", &ERROR_TESTS),
];

/// The number of sections to choose from.
const SECTION_COUNT: usize = SECTIONS.len() + BLOCK_SECTIONS.len();

/// Returns the title and the blocks of a section, given its number counting from 1.
fn section(number: usize) -> (&'static str, &'static [&'static str]) {
    match SECTIONS.get(number - 1) {
        Some((title, source)) => (title, slice::from_ref(source)),
        None => BLOCK_SECTIONS[number - 1 - SECTIONS.len()],
    }
}

/// Boots a VM that writes to stdout.
///
/// # Safety
///
/// The result has the same requirements as the result of [`boot`].
unsafe fn new_vm(config: VmConfig) -> Vm {
    boot(io::stdout(), config).expect("the dictionary must be able to hold the primitives")
}

/// Evaluates some Forth source and ends the output line with the result. That's `ok` if all of
/// it ran, unless a definition is still open. After `QUIT` and `ABORT`, the standard allows no
/// message.
///
/// # Safety
///
/// `vm` must come from [`boot`].
unsafe fn interpret(vm: &mut Vm, input: &str) -> Result<(), ExitCode> {
    match outer_interpreter(vm, input) {
        Ok(()) if vm.is_compiling() => {}
        Ok(()) => println!("ok"),
        Err(ForthError::Quit | ForthError::Abort) => println!(),
        Err(ForthError::Bye(code)) => return Err(ExitCode::from(code as u8)),
        Err(error) => println!("Forth Error: {}", error),
    }
    Ok(())
}

/// Echoes a block of Forth source and evaluates it. Output from Forth words follows on one
/// line, which ends with the result.
///
/// # Safety
///
/// `vm` must come from [`boot`].
unsafe fn evaluate(vm: &mut Vm, input: &str) -> Result<(), ExitCode> {
    println!("Input:");
    for line in input.trim().lines() {
        println!("{}", format!("    {}", line).trim_end());
    }
    print!("Output: ");
    interpret(vm, input)
}

/// Runs one section in a new VM, given its number counting from 1.
fn run_section(number: usize) -> Result<(), ExitCode> {
    let (title, blocks) = section(number);
    println!("--- TEST RUN {}: {} ---", number, title);
    // SAFETY: `vm` comes from `boot`, through `new_vm`.
    unsafe {
        let mut vm = new_vm(VmConfig {
            data_stack_cells: DEMO_DATA_STACK_CELLS,
            ..VmConfig::new(DEMO_DICTIONARY_CELLS)
        });
        for source in blocks {
            evaluate(&mut vm, source)?;
        }
    }
    println!("-------------------------------------\n");
    Ok(())
}

/// One answer to the demo menu.
#[derive(Debug, PartialEq)]
enum Choice {
    /// Run all the sections.
    All,
    /// Run the section with this number, counting from 1.
    Section(usize),
    /// Leave the program.
    Quit,
}

/// Reads one answer to the demo menu, with `section_count` sections to choose from. Returns
/// `None` if the answer isn't valid.
fn parse_choice(input: &str, section_count: usize) -> Option<Choice> {
    let input = input.trim();
    match input {
        "" => Some(Choice::All),
        "q" | "Q" => Some(Choice::Quit),
        _ => match input.parse() {
            Ok(0) => Some(Choice::All),
            Ok(number) if number <= section_count => Some(Choice::Section(number)),
            _ => None,
        },
    }
}

/// Reads a line of input. Returns the exit code instead if the program should end, which it
/// does at the end of the input, or if the input can't be read.
fn read_line(input: &mut impl BufRead) -> Result<String, ExitCode> {
    let mut line = String::new();
    let result = input.read_line(&mut line);
    match result {
        Ok(0) => {
            println!();
            Err(ExitCode::SUCCESS)
        }
        Ok(_) => Ok(line),
        Err(error) => {
            println!();
            eprintln!("Cannot read the input: {}", error);
            Err(ExitCode::FAILURE)
        }
    }
}

/// Prints the list of sections.
fn print_menu() {
    println!("Demo sections:");
    println!("{:>4}. All sections", 0);
    for number in 1..=SECTION_COUNT {
        println!("{:>4}. {}", number, section(number).0);
    }
}

/// Shows the demo menu and runs what the user chooses from `input`, until they quit or the
/// input ends.
fn demo_menu(mut input: impl BufRead) -> ExitCode {
    loop {
        print_menu();
        print!("Enter a section number, Q to quit [0]: ");
        let _ = io::stdout().flush(); // The prompt has no newline. The menu works without it

        let line = match read_line(&mut input) {
            Ok(line) => line,
            Err(code) => return code,
        };
        match parse_choice(&line, SECTION_COUNT) {
            Some(Choice::All) => {
                for number in 1..=SECTION_COUNT {
                    if let Err(code) = run_section(number) {
                        return code;
                    }
                }
            }
            Some(Choice::Section(number)) => {
                if let Err(code) = run_section(number) {
                    return code;
                }
            }
            Some(Choice::Quit) => return ExitCode::SUCCESS,
            None => println!("Invalid choice: {}\n", line.trim()),
        }
    }
}

/// Evaluates each line from `read_line` in one VM, until it returns an exit code or Forth ends
/// the program. `read_line` gets the VM, for what it can complete.
fn interactive(mut read_line: impl FnMut(&Vm) -> Result<String, ExitCode>) -> ExitCode {
    let mut end = ExitCode::SUCCESS;
    // SAFETY: `vm` comes from `boot`, through `new_vm`.
    let bye = unsafe {
        let mut vm = new_vm(VmConfig::new(DICTIONARY_CELLS));
        quit_loop(
            &mut vm,
            |vm| match read_line(vm) {
                Ok(line) => Some(line),
                Err(code) => {
                    end = code;
                    None
                }
            },
            |vm, result| match result {
                LineResult::Ok if vm.is_compiling() => {}
                LineResult::Ok => println!("ok"),
                LineResult::Quit | LineResult::Abort => println!(),
                LineResult::Error(error) => println!("Forth Error: {}", error),
            },
        )
    };
    match bye {
        Some(code) => ExitCode::from(code as u8),
        None => end,
    }
}

/// Runs the interactive prompt, with line editing if both ends are a terminal. Input from
/// anything else is read as it is, so nothing but its output reaches a pipe.
fn prompt() -> ExitCode {
    #[cfg(feature = "readline")]
    if io::stdin().is_terminal() && io::stdout().is_terminal() {
        match LineEditor::new() {
            // SAFETY: the VM's word headers are as intact as `interpret` needs them to be for
            // the words of each line, which it finds through the same pointers.
            Ok(mut editor) => return interactive(|vm| unsafe { editor.read_line(vm) }),
            Err(error) => eprintln!("Cannot edit lines: {}", error),
        }
    }
    let mut input = io::stdin().lock();
    interactive(|_| read_line(&mut input))
}

/// Starts the interactive prompt or the demo menu, or prints the version, depending on the
/// arguments.
fn main() -> ExitCode {
    let mut arguments = env::args_os().skip(1);
    let demo = match (arguments.next(), arguments.next()) {
        (None, _) => false,
        (Some(argument), None) if argument == "--interactive" => false,
        (Some(argument), None) if argument == "--demo" => true,
        (Some(argument), None) if argument == "--version" => {
            println!("{}", VERSION);
            return ExitCode::SUCCESS;
        }
        _ => {
            eprintln!("{}", USAGE);
            return ExitCode::from(2);
        }
    };

    if demo {
        println!("--- FORTH BOOTSEQUENCE COMPLETED ---");
        println!("The Text Interpreter (Outer Interpreter) is online.\n");
        demo_menu(io::stdin().lock())
    } else {
        println!("{}\n{}\n{}\n", VERSION, COPYRIGHT, MORE_INFORMATION);
        prompt()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_choice_reads_all_quit_and_section_numbers() {
        assert_eq!(parse_choice("", 13), Some(Choice::All));
        assert_eq!(parse_choice("\n", 13), Some(Choice::All));
        assert_eq!(parse_choice("  \r\n", 13), Some(Choice::All));
        assert_eq!(parse_choice("0\n", 13), Some(Choice::All));
        assert_eq!(parse_choice("q\n", 13), Some(Choice::Quit));
        assert_eq!(parse_choice("Q\n", 13), Some(Choice::Quit));
        assert_eq!(parse_choice("1\n", 13), Some(Choice::Section(1)));
        assert_eq!(parse_choice(" 13 \n", 13), Some(Choice::Section(13)));
    }

    #[test]
    fn parse_choice_rejects_anything_else() {
        assert_eq!(parse_choice("14\n", 13), None);
        assert_eq!(parse_choice("-1\n", 13), None);
        assert_eq!(parse_choice("abc\n", 13), None);
        assert_eq!(parse_choice("quit\n", 13), None);
        assert_eq!(parse_choice("1 2\n", 13), None);
        assert_eq!(parse_choice("99999999999999999999999\n", 13), None);
    }

    #[test]
    fn section_numbers_cover_every_section_in_order() {
        assert_eq!(section(1).0, SECTIONS[0].0);
        assert_eq!(section(1).1, [SECTIONS[0].1]);
        assert_eq!(section(SECTIONS.len()).0, SECTIONS[SECTIONS.len() - 1].0);
        assert_eq!(section(SECTION_COUNT - 1).1, RESTART_TESTS);
        assert_eq!(section(SECTION_COUNT).1, ERROR_TESTS);
    }
}
