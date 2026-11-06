// Copyright (c) 2026 M.W.R. Hoiting

//! An interactive Forth prompt, and a scripted demo of the Forth VM.
//!
//! By default, or with `--interactive`, the program evaluates each line from stdin in one VM.
//!
//! With `--demo`, it shows a menu of demo sections. Each section shows the normal use of some
//! words, and the last one shows how errors are handled.

use std::env;
use std::io::{self, BufRead, Write};
use std::process::ExitCode;
use std::slice;

use rust_forth_vm::{boot, outer_interpreter, ForthError, Registers};

/// The size of the demo's Forth dictionary.
const DICTIONARY_CELLS: usize = 1024;

/// The usage line, printed for arguments the program doesn't know.
const USAGE: &str = "Usage: forth-vm [--interactive | --demo]";

/// The demo sections, each with its title and one block of Forth source. Each section runs in a
/// new VM, so it must define everything it uses.
const SECTIONS: [(&str, &str); 12] = [
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
: +! SWAP OVER @ + SWAP ! ;   \ add to a variable
VARIABLE COUNTER
45 COUNTER !
5 COUNTER +!
COUNTER @ .                   \ prints 50
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

\ Pass behaviour to a word, as a callback
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
const ERROR_TESTS: [&str; 8] = [
    ": BROKEN 1 NO-SUCH-WORD ;",
    "BROKEN",
    "+",
    "1 5 !",
    "DROP",
    "9 PICK",
    "5 EXECUTE",
    "1 ( never closed",
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
unsafe fn new_vm() -> Registers {
    boot(io::stdout(), DICTIONARY_CELLS)
        .expect("the dictionary must be able to hold the primitives")
}

/// Evaluates some Forth source and ends the output line with the result. That's `ok` if all of
/// it ran, unless a definition is still open. After `QUIT` and `ABORT`, the standard allows no
/// message.
///
/// # Safety
///
/// `regs` must come from [`boot`].
unsafe fn interpret(regs: &mut Registers, input: &str) {
    match outer_interpreter(regs, input) {
        Ok(()) if regs.is_compiling() => {}
        Ok(()) => println!("ok"),
        Err(ForthError::Quit | ForthError::Abort) => println!(),
        Err(error) => println!("Forth Error: {}", error),
    }
}

/// Echoes a block of Forth source and evaluates it. Output from Forth words follows on one
/// line, which ends with the result.
///
/// # Safety
///
/// `regs` must come from [`boot`].
unsafe fn evaluate(regs: &mut Registers, input: &str) {
    println!("Input:");
    for line in input.trim().lines() {
        println!("{}", format!("    {}", line).trim_end());
    }
    print!("Output: ");
    interpret(regs, input);
}

/// Runs one section in a new VM, given its number counting from 1.
fn run_section(number: usize) {
    let (title, blocks) = section(number);
    println!("--- TEST RUN {}: {} ---", number, title);
    // SAFETY: `regs` comes from `boot`, through `new_vm`.
    unsafe {
        let mut regs = new_vm();
        for source in blocks {
            evaluate(&mut regs, source);
        }
    }
    println!("-------------------------------------\n");
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
            Some(Choice::All) => (1..=SECTION_COUNT).for_each(run_section),
            Some(Choice::Section(number)) => run_section(number),
            Some(Choice::Quit) => return ExitCode::SUCCESS,
            None => println!("Invalid choice: {}\n", line.trim()),
        }
    }
}

/// Evaluates each line of `input` in one VM, until the input ends.
fn interactive(mut input: impl BufRead) -> ExitCode {
    // SAFETY: `regs` comes from `boot`, through `new_vm`.
    unsafe {
        let mut regs = new_vm();
        loop {
            match read_line(&mut input) {
                Ok(line) => interpret(&mut regs, &line),
                Err(code) => return code,
            }
        }
    }
}

/// Starts the interactive prompt or the demo menu, depending on the arguments.
fn main() -> ExitCode {
    let mut arguments = env::args_os().skip(1);
    let demo = match (arguments.next(), arguments.next()) {
        (None, _) => false,
        (Some(argument), None) if argument == "--interactive" => false,
        (Some(argument), None) if argument == "--demo" => true,
        _ => {
            eprintln!("{}", USAGE);
            return ExitCode::from(2);
        }
    };

    println!("--- FORTH BOOTSEQUENCE COMPLETED ---");
    println!("The Text Interpreter (Outer Interpreter) is online.");
    if demo {
        println!();
        demo_menu(io::stdin().lock())
    } else {
        println!("Press Ctrl-D to leave.\n");
        interactive(io::stdin().lock())
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
