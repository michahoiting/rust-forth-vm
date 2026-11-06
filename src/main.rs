use std::io;

use rust_forth_vm::{boot, outer_interpreter, Registers};

// The size of the demo's Forth dictionary.
const DICTIONARY_CELLS: usize = 1024;

// Echoes a line of input, evaluates it, and ends the line with its result.
// Output from Forth words appears between the two.
unsafe fn evaluate(regs: &mut Registers, input: &str) {
    print!("Input: '{}' ", input);
    match outer_interpreter(regs, input) {
        Ok(()) => println!("ok"),
        Err(error) => println!("Forth Error: {}", error),
    }
}

fn main() {
    unsafe {
        let mut regs = boot(io::stdout(), DICTIONARY_CELLS)
            .expect("the dictionary must be able to hold the primitives");

        println!("--- FORTH BOOTSEQUENCE COMPLETED ---");
        println!("The Text Interpreter (Outer Interpreter) is online.\n");

        // --- TEST RUN 1: Evaluating Text Operations ---
        evaluate(&mut regs, "40 2 +");
        // Parameter stack now contains 42. Print it out using the Forth '.' primitive
        evaluate(&mut regs, ".");
        evaluate(&mut regs, "-5 3 + .");
        println!("-------------------------------------\n");

        // --- TEST RUN 2: Dynamic Creation via Text interpreter ---
        evaluate(&mut regs, "CREATE STATIONARY-SOURCE"); // Creates a new word in the dictionary
        evaluate(&mut regs, "1337 ,"); // Compiles the value 1337 into the Data Field of the new word
                                       // We can now reference our dynamically generated word directly via text input!
        evaluate(&mut regs, "STATIONARY-SOURCE @ .");
        println!("-------------------------------------\n");

        // --- TEST RUN 3: Colon Definitions ---
        evaluate(&mut regs, ": ADD3 3 + ;");
        evaluate(&mut regs, ": ADD6 ADD3 ADD3 ;");
        evaluate(&mut regs, "36 ADD6 .");
        println!("-------------------------------------\n");

        // --- TEST RUN 4: Defining Words with DOES> ---
        evaluate(&mut regs, ": CONSTANT CREATE , DOES> @ ;");
        evaluate(&mut regs, "42 CONSTANT ANSWER");
        evaluate(&mut regs, "7 CONSTANT LUCKY");
        evaluate(&mut regs, "ANSWER . LUCKY .");
        println!("-------------------------------------\n");

        // --- TEST RUN 5: Variables with ! and @ ---
        evaluate(&mut regs, "VARIABLE COUNTER");
        evaluate(&mut regs, "COUNTER @ ."); // A new variable starts at zero
        evaluate(&mut regs, "42 COUNTER !");
        evaluate(&mut regs, "COUNTER @ .");
        evaluate(&mut regs, "COUNTER @ 1 + COUNTER !"); // Read, change, and write back
        evaluate(&mut regs, "COUNTER @ .");
        evaluate(&mut regs, ": BUMP COUNTER @ 1 + COUNTER ! ;"); // Used from a colon word
        evaluate(&mut regs, "BUMP BUMP COUNTER @ .");
        evaluate(&mut regs, "VARIABLE OTHER"); // Each variable has its own Cell
        evaluate(&mut regs, "7 OTHER !");
        evaluate(&mut regs, "OTHER @ . COUNTER @ .");
        println!("-------------------------------------\n");

        // --- TEST RUN 6: Stack Words ---
        evaluate(&mut regs, "1 2 DROP .");
        evaluate(&mut regs, "1 2 SWAP . .");
        evaluate(&mut regs, "3 4 OVER . . .");
        evaluate(&mut regs, "1 2 3 ROT . . .");
        evaluate(&mut regs, "5 ?DUP . .");
        evaluate(&mut regs, "0 ?DUP .");
        evaluate(&mut regs, "10 20 30 2 PICK . . . .");
        evaluate(&mut regs, "1 2 3 2 ROLL . . .");
        evaluate(&mut regs, "1 2 3 DEPTH . . . .");
        evaluate(&mut regs, ": DOUBLE DUP + ;"); // Reuse a value without naming it
        evaluate(&mut regs, "21 DOUBLE .");
        evaluate(&mut regs, ": +! SWAP OVER @ + SWAP ! ;"); // Add to a variable
        evaluate(&mut regs, "5 COUNTER +!");
        evaluate(&mut regs, "COUNTER @ .");
        println!("-------------------------------------\n");

        // --- TEST RUN 7: Execution Tokens ---
        // Run an unnamed definition straight away
        evaluate(&mut regs, ":NONAME 1 2 + ; EXECUTE .");
        // Pass arguments and results through the stack
        evaluate(&mut regs, "5 :NONAME DUP + ; EXECUTE .");
        // Keep an execution token under a name
        evaluate(&mut regs, ":NONAME 3 + ; CONSTANT ADD3-XT");
        evaluate(&mut regs, "4 ADD3-XT EXECUTE .");
        // Pass behaviour to a word, as a callback
        evaluate(&mut regs, ": TWICE DUP ROT SWAP EXECUTE SWAP EXECUTE ;");
        evaluate(&mut regs, "5 :NONAME DUP + ; TWICE .");
        evaluate(&mut regs, "1 ADD3-XT TWICE .");
        // Change what a word does at runtime, through a variable
        evaluate(&mut regs, "VARIABLE ACTION");
        evaluate(&mut regs, ":NONAME 42 ; ACTION !");
        evaluate(&mut regs, ": RUN-ACTION ACTION @ EXECUTE ;");
        evaluate(&mut regs, "RUN-ACTION .");
        evaluate(&mut regs, ":NONAME 7 ; ACTION !");
        evaluate(&mut regs, "RUN-ACTION .");
        println!("-------------------------------------\n");

        // --- TEST RUN 8: Error Handling ---
        evaluate(&mut regs, ": BROKEN 1 NO-SUCH-WORD ;");
        evaluate(&mut regs, "BROKEN");
        evaluate(&mut regs, "+");
        evaluate(&mut regs, "1 5 !");
        evaluate(&mut regs, "DROP");
        evaluate(&mut regs, "9 PICK");
        evaluate(&mut regs, "5 EXECUTE");
        println!("-------------------------------------\n");
    }
}
