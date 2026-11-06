// Copyright (c) 2026 M.W.R. Hoiting

//! A scripted demo of the Forth VM. Each section shows the normal use of some words, and
//! the last one shows how errors are handled.

use std::io;

use rust_forth_vm::{boot, outer_interpreter, Registers};

/// The size of the demo's Forth dictionary.
const DICTIONARY_CELLS: usize = 1024;

/// The demo sections, each with its title and one block of Forth source. All of them run in
/// the same VM, so later sections can use words from earlier ones.
const SECTIONS: [(&str, &str); 9] = [
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
        "Defining Words with DOES>",
        r"
: CONSTANT CREATE , DOES> @ ;
42 CONSTANT ANSWER
7 CONSTANT LUCKY
ANSWER . LUCKY .  \ prints 42 7
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
    match outer_interpreter(regs, input) {
        Ok(()) => println!("ok"),
        Err(error) => println!("Forth Error: {}", error),
    }
}

/// Boots a VM and runs every section of the demo in it.
fn main() {
    unsafe {
        let mut regs = boot(io::stdout(), DICTIONARY_CELLS)
            .expect("the dictionary must be able to hold the primitives");

        println!("--- FORTH BOOTSEQUENCE COMPLETED ---");
        println!("The Text Interpreter (Outer Interpreter) is online.\n");

        for (number, (title, source)) in SECTIONS.iter().enumerate() {
            println!("--- TEST RUN {}: {} ---", number + 1, title);
            evaluate(&mut regs, source);
            println!("-------------------------------------\n");
        }

        println!("--- TEST RUN {}: Error Handling ---", SECTIONS.len() + 1);
        for source in ERROR_TESTS {
            evaluate(&mut regs, source);
        }
        println!("-------------------------------------\n");
    }
}
