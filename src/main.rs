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

        // --- TEST RUN 6: Error Handling ---
        evaluate(&mut regs, ": BROKEN 1 NO-SUCH-WORD ;");
        evaluate(&mut regs, "BROKEN");
        evaluate(&mut regs, "+");
        evaluate(&mut regs, "1 5 !");
        println!("-------------------------------------\n");
    }
}
