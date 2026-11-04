use std::mem;
use std::ptr;

// --- 1. THE FOUNDATION: THE FORTH CELL ---

// A Cell is the universal basic unit of Forth (64-bit on a 64-bit CPU).
// It blindly holds data, flags, offsets, or raw memory pointers.
type Cell = usize;

// Definition of a Forth primitive (machine-code function).
// Primitives return to the `run` loop instead of calling NEXT themselves, so
// threaded code never grows the Rust call stack.
type PrimitiveFn = unsafe fn(registers: &mut Registers);

// Size of the dictionary arena, in Cells.
const DICTIONARY_CELLS: usize = 1024;

// Layout of a dictionary header, relative to its LFA. Every word uses this
// layout, whether it is a primitive or was created at runtime. The name bytes
// are stored in the dictionary directly in front of the header.
const LFA: usize = 0; // Link Field: points to the LFA of the previous word
const NFA: usize = 1; // Name Field: points to the name bytes
const LEN: usize = 2; // Length of the name in bytes
const FLAGS: usize = 3; // Combination of the F_* flags below
const CFA: usize = 4; // Code Field: the machine-code pointer
const HEADER_CELLS: usize = 5;

const F_IMMEDIATE: Cell = 1; // Executes even while compiling
const F_HIDDEN: Cell = 2; // Invisible to dictionary lookups
const F_CREATED: Cell = 4; // Made by CREATE, so it owns an action cell for DOES>

// CPU Registers and Stacks conforming to the Forth hardware architecture
struct Registers {
    ip: *const Cell,                // Instruction Pointer: points to a Cell in a thread
    w: *const Cell,                 // Word Register: points to the currently active CFA Cell
    data_stack: Vec<Cell>,          // Parameter stack (contains raw Cells)
    return_stack: Vec<*const Cell>, // Return stack (R-stack) for nested IP pointers
    here: *mut Cell,                // Dictionary pointer to the next free Cell
    last_word: *mut Cell,           // Pointer to the LFA of the most recently created word
    dictionary_end: *mut Cell,      // One past the last usable dictionary Cell
    _dictionary: Vec<Cell>,         // Owns the arena that the pointers above refer to

    compiling: bool,                            // STATE: true between `:` and `;`
    definition: Option<(*mut Cell, *mut Cell)>, // HERE and LAST to restore if a definition fails
    lit_cfa: Cell,                              // CFA compiled in front of inline literals
    exit_cfa: Cell,                             // CFA compiled by `;`

    source: String,        // The line being interpreted
    source_pos: usize,     // Parse offset into `source`
    error: Option<String>, // Set by a primitive to stop the VM
}

impl Registers {
    // Records an error. The inner interpreter stops, and the outer interpreter aborts.
    fn set_error(&mut self, message: impl Into<String>) {
        if self.error.is_none() {
            self.error = Some(message.into());
        }
    }

    // Pops a Cell, flagging an error when the stack is empty.
    fn pop(&mut self) -> Option<Cell> {
        let value = self.data_stack.pop();
        if value.is_none() {
            self.set_error("Data stack underflow.");
        }
        value
    }

    // Parses the next whitespace-delimited token from the input source.
    fn parse_name(&mut self) -> Option<String> {
        let rest = &self.source[self.source_pos..];
        let start = rest.find(|c: char| !c.is_whitespace())?;
        let tail = &rest[start..];
        let len = tail.find(char::is_whitespace).unwrap_or(tail.len());
        let token = tail[..len].to_string();
        self.source_pos += start + len;
        Some(token)
    }
}

// Converts a primitive into the raw Cell stored in a Code Field.
fn code_cell(code: PrimitiveFn) -> Cell {
    code as *const () as Cell
}

// --- 2. THE INNER INTERPRETER (NEXT) ---

// The heart of the Forth engine. Fetches the CFA, increments IP, and jumps to the code.
// A zero Cell in the thread marks its end, and leaves W null.
#[inline(always)]
unsafe fn next(regs: &mut Registers) {
    // W = *IP (Load the address of the current CFA Cell into the W register)
    regs.w = *regs.ip as *const Cell;
    if regs.w.is_null() {
        return;
    }

    // IP++ (Step to the next Cell in the currently executing thread)
    regs.ip = regs.ip.add(1);

    // Fetch the contents of the CFA Cell (the raw bits of the machine-code pointer)
    let code_bits = *regs.w as *const ();

    // Blindly transmute the raw bits into an executable Rust function (like a hardware jump)
    let code = mem::transmute::<*const (), PrimitiveFn>(code_bits);
    code(regs);
}

// Runs NEXT until the thread ends or a primitive reports an error.
unsafe fn run(regs: &mut Registers) {
    loop {
        next(regs);
        if regs.w.is_null() || regs.error.is_some() {
            return;
        }
    }
}

// --- 3. CORE PRIMITIVES (DOCOL, DOVAR, DODOES) ---

// A word made by CREATE has two Cells behind its CFA: W + 1 is the action cell
// reserved for DOES>, and its data field starts at W + 2.

// DOCOL: Executed when entering a nested `:` (colon) word.
unsafe fn docol(regs: &mut Registers) {
    // Push the current IP to the Return Stack (return address)
    regs.return_stack.push(regs.ip);
    // Move IP to the inside of the definition (physically 1 Cell after the CFA, so W + 1)
    regs.ip = regs.w.add(1);
}

// DOVAR: Default execution semantics for a word created via CREATE (or VARIABLE).
// It simply pushes the memory address of its own Data Field (W + 2) onto the stack.
unsafe fn dovar(regs: &mut Registers) {
    let data_address = regs.w.add(2) as Cell;
    regs.data_stack.push(data_address);
}

// DODOES: Runtime execution semantics for a child word modified via DOES>.
unsafe fn dodoes(regs: &mut Registers) {
    // 1. Push the address of the actual data onto the stack (W + 2, because W + 1 holds the action-pointer)
    let data_address = regs.w.add(2) as Cell;
    regs.data_stack.push(data_address);

    // 2. Save the current IP onto the Return Stack
    regs.return_stack.push(regs.ip);

    // 3. Jump with the IP to the Forth action code stored behind DOES> (saved in W + 1)
    regs.ip = *regs.w.add(1) as *const Cell;
}

// EXIT: Cleans up and exits a nested `:` word (compiled by `;`)
unsafe fn exit(regs: &mut Registers) {
    match regs.return_stack.pop() {
        Some(old_ip) => regs.ip = old_ip,
        None => regs.set_error("Return stack underflow."),
    }
}

// --- 4. DATA AND COMPILER PRIMITIVES ---

// LIT: Fetches the inline literal value from the thread, pushes it, and skips the data cell.
unsafe fn lit(regs: &mut Registers) {
    let literal_value = *regs.ip;
    regs.data_stack.push(literal_value);
    regs.ip = regs.ip.add(1); // Skip the inline data cell
}

// Presses a Cell into the dictionary at HERE.
unsafe fn dictionary_comma(regs: &mut Registers, value: Cell) {
    if regs.here >= regs.dictionary_end {
        regs.set_error("Dictionary overflow.");
        return;
    }
    *regs.here = value;
    regs.here = regs.here.add(1);
}

// COMMA ( , ): Pops a Cell from the stack and presses it into the dictionary at HERE.
unsafe fn comma(regs: &mut Registers) {
    if let Some(value) = regs.pop() {
        dictionary_comma(regs, value);
    }
}

// FETCH ( @ ): Pops an address from the stack and replaces it with the contents of that Cell.
unsafe fn fetch(regs: &mut Registers) {
    if let Some(addr) = regs.pop() {
        let ptr = addr as *const Cell;
        regs.data_stack.push(*ptr);
    }
}

// PLUS ( + ): Pops two Cells from the stack, adds them, and pushes the result.
unsafe fn plus(regs: &mut Registers) {
    if regs.data_stack.len() < 2 {
        regs.set_error("Data stack underflow.");
        return;
    }
    if let (Some(b), Some(a)) = (regs.data_stack.pop(), regs.data_stack.pop()) {
        regs.data_stack.push(a.wrapping_add(b));
    }
}

// PRINT ( . ): Pops a Cell from the stack and prints it to stdout as a signed number.
unsafe fn dot_print(regs: &mut Registers) {
    if let Some(value) = regs.pop() {
        println!("Forth Output: {}", value as isize);
    }
}

// --- 5. RUNTIME CREATE AND DOES> MANIPULATION ---

// Writes a complete word into the dictionary: the name bytes, then the header.
// Returns the LFA of the new word, or None if the dictionary is full.
unsafe fn write_header(
    regs: &mut Registers,
    name: &str,
    code: Cell,
    flags: Cell,
) -> Option<*mut Cell> {
    let name_cells = name.len().div_ceil(mem::size_of::<Cell>());
    let free_cells = regs.dictionary_end.offset_from(regs.here) as usize;
    if name_cells + HEADER_CELLS > free_cells {
        regs.set_error("Dictionary overflow.");
        return None;
    }

    // The dictionary owns its own copy, because the input line does not outlive this call.
    let name_ptr = regs.here as *mut u8;
    ptr::copy_nonoverlapping(name.as_ptr(), name_ptr, name.len());

    let header = regs.here.add(name_cells);
    *header.add(LFA) = regs.last_word as Cell;
    *header.add(NFA) = name_ptr as Cell;
    *header.add(LEN) = name.len();
    *header.add(FLAGS) = flags;
    *header.add(CFA) = code;

    regs.here = header.add(HEADER_CELLS);
    regs.last_word = header; // Update the head pointer of the dictionary
    Some(header)
}

// CREATE: Parses a name and allocates a new word with default DOVAR semantics.
unsafe fn create(regs: &mut Registers) {
    let Some(name) = regs.parse_name() else {
        regs.set_error("CREATE needs a name.");
        return;
    };
    if write_header(regs, &name, code_cell(dovar), F_CREATED).is_some() {
        dictionary_comma(regs, 0); // Reserve the action cell for DOES>
    }
}

// (DOES>): Hijacks the CFA of the child word while the defining word is executing.
// The Forth code compiled behind DOES> becomes the action of the child.
unsafe fn runtime_does(regs: &mut Registers) {
    // An empty Return Stack means DOES> is running from the text interpreter. IP then
    // points into the temporary trampoline, which must not be stored in the child.
    if regs.return_stack.is_empty() {
        regs.set_error("DOES> can only be used inside a definition.");
        return;
    }

    // Only words made by CREATE have an action cell. Anything else has live data at W + 1.
    let child = regs.last_word;
    if *child.add(FLAGS) & F_CREATED == 0 {
        regs.set_error("DOES> needs a word made by CREATE.");
        return;
    }

    let child_cfa = child.add(CFA);
    *child_cfa = code_cell(dodoes); // Overwrite the child's CFA with DODOES
    *child_cfa.add(1) = regs.ip as Cell; // Store the pointer to the action code in W+1 of the child

    // Unnest from the defining word (EXIT behavior)
    exit(regs);
}

// COLON ( : ): Parses a name and switches the text interpreter to compiling.
// The word stays hidden until `;`, so a redefinition can still call the old word.
unsafe fn colon(regs: &mut Registers) {
    let Some(name) = regs.parse_name() else {
        regs.set_error(": needs a name.");
        return;
    };
    let restore_point = (regs.here, regs.last_word);
    if write_header(regs, &name, code_cell(docol), F_HIDDEN).is_some() {
        regs.definition = Some(restore_point);
        regs.compiling = true;
    }
}

// SEMICOLON ( ; ): Ends the current definition. This word is immediate.
unsafe fn semicolon(regs: &mut Registers) {
    if !regs.compiling {
        regs.set_error("; can only be used while compiling.");
        return;
    }
    dictionary_comma(regs, regs.exit_cfa);
    if regs.error.is_none() {
        *regs.last_word.add(FLAGS) &= !F_HIDDEN;
        regs.definition = None;
        regs.compiling = false;
    }
}

// --- 6. HELPER FUNCTION: INJECT PRIMITIVES DIRECTLY INTO THE DICTIONARY ---

// Simulates the built-in bootstrap dictionary populated when a Forth system boots.
// Returns the CFA of the new word.
unsafe fn add_primitive_word(
    regs: &mut Registers,
    name: &str,
    code_fn: PrimitiveFn,
    flags: Cell,
) -> Cell {
    let header = write_header(regs, name, code_cell(code_fn), flags)
        .expect("the dictionary must be able to hold the primitives");
    header.add(CFA) as Cell
}

// Allocates the VM and registers the core primitives under their official text names.
unsafe fn boot() -> Registers {
    // Allocate an untyped arena for our Forth dictionary space
    let mut dictionary = vec![0 as Cell; DICTIONARY_CELLS];
    let here = dictionary.as_mut_ptr();

    let mut regs = Registers {
        ip: ptr::null(),
        w: ptr::null(),
        data_stack: Vec::new(),
        return_stack: Vec::new(),
        here,
        last_word: ptr::null_mut(), // Dictionary starts out empty
        dictionary_end: here.add(dictionary.len()),
        _dictionary: dictionary,
        compiling: false,
        definition: None,
        lit_cfa: 0,
        exit_cfa: 0,
        source: String::new(),
        source_pos: 0,
        error: None,
    };

    add_primitive_word(&mut regs, "+", plus, 0);
    add_primitive_word(&mut regs, ".", dot_print, 0);
    add_primitive_word(&mut regs, "@", fetch, 0);
    add_primitive_word(&mut regs, ",", comma, 0);
    add_primitive_word(&mut regs, "CREATE", create, 0);
    add_primitive_word(&mut regs, "DOES>", runtime_does, 0);
    add_primitive_word(&mut regs, ":", colon, 0);
    add_primitive_word(&mut regs, ";", semicolon, F_IMMEDIATE);
    regs.exit_cfa = add_primitive_word(&mut regs, "EXIT", exit, 0);
    // LIT reads its operand from the thread, so it only makes sense in compiled code.
    regs.lit_cfa = add_primitive_word(&mut regs, "LIT", lit, F_HIDDEN);

    regs
}

// --- 7. THE TEXT INTERPRETER / OUTER INTERPRETER ---

// Linearly traverses the linked list of the dictionary (from newest to oldest).
// Returns the LFA of the matching word.
unsafe fn find_word(regs: &Registers, token: &str) -> Option<*mut Cell> {
    let mut current_lfa = regs.last_word;

    while !current_lfa.is_null() {
        let nfa_ptr = *current_lfa.add(NFA) as *const u8;
        let len = *current_lfa.add(LEN);
        let hidden = *current_lfa.add(FLAGS) & F_HIDDEN != 0;

        // Reconstruct the name from raw dictionary data pointers
        if !hidden && std::slice::from_raw_parts(nfa_ptr, len) == token.as_bytes() {
            return Some(current_lfa);
        }
        // Follow the link pointer to the previous word (LFA)
        current_lfa = *current_lfa.add(LFA) as *mut Cell;
    }
    None
}

// Accepts negative numbers as well as the full unsigned range of a Cell.
fn parse_number(token: &str) -> Option<Cell> {
    let signed = token.parse::<isize>().map(|number| number as Cell);
    signed.or_else(|_| token.parse::<Cell>()).ok()
}

// Executes a single word from the text interpreter.
unsafe fn execute(regs: &mut Registers, cfa: *mut Cell) {
    // To safely execute this single binary CFA via NEXT, we construct a mini 'trampoline' thread in RAM:
    // [ Target CFA Address ] [ Stop / End Marker (0) ]
    let trampoline: [Cell; 2] = [cfa as Cell, 0];
    regs.ip = trampoline.as_ptr();

    // Kickstart the Inner Interpreter engine
    run(regs);

    regs.ip = ptr::null(); // The trampoline is about to go out of scope
}

// Handles a single token, either interpreting or compiling it.
unsafe fn interpret_token(regs: &mut Registers, token: &str) {
    if let Some(header) = find_word(regs, token) {
        // --- CASE A: THE TOKEN IS A DICTIONARY WORD ---
        let cfa = header.add(CFA);
        let immediate = *header.add(FLAGS) & F_IMMEDIATE != 0;
        if regs.compiling && !immediate {
            dictionary_comma(regs, cfa as Cell);
        } else {
            execute(regs, cfa);
        }
    } else if let Some(number) = parse_number(token) {
        // --- CASE B: THE TOKEN IS A NUMBER (LITERAL) ---
        if regs.compiling {
            dictionary_comma(regs, regs.lit_cfa);
            dictionary_comma(regs, number);
        } else {
            regs.data_stack.push(number); // Push directly onto the parameter stack
        }
    } else {
        // --- CASE C: SYNTAX / COMPILATION ERROR ---
        regs.set_error(format!(
            "Word or numeric literal '{}' not recognized.",
            token
        ));
    }
}

// ABORT: Resets the VM after an error and discards any half-built definition.
unsafe fn abort(regs: &mut Registers) {
    regs.data_stack.clear();
    regs.return_stack.clear();
    regs.compiling = false;
    if let Some((here, last_word)) = regs.definition.take() {
        regs.here = here;
        regs.last_word = last_word;
    }
}

unsafe fn outer_interpreter(regs: &mut Registers, input: &str) -> Result<(), String> {
    regs.source = input.to_string();
    regs.source_pos = 0;

    // Tokenize the input string by whitespace (spaces, tabs, newlines)
    while let Some(token) = regs.parse_name() {
        interpret_token(regs, &token);
        if let Some(message) = regs.error.take() {
            abort(regs);
            return Err(message);
        }
    }
    Ok(())
}

// Echoes a line of input, evaluates it, and reports any error.
unsafe fn evaluate(regs: &mut Registers, input: &str) {
    println!("Input: '{}'", input);
    if let Err(message) = outer_interpreter(regs, input) {
        println!("Forth Error: {}", message);
    }
}

fn main() {
    unsafe {
        let mut regs = boot();

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

        // --- TEST RUN 5: Error Handling ---
        evaluate(&mut regs, ": BROKEN 1 NO-SUCH-WORD ;");
        evaluate(&mut regs, "BROKEN");
        evaluate(&mut regs, "+");
        println!("-------------------------------------\n");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Evaluates the lines in order, and returns the resulting data stack.
    unsafe fn eval_stack(lines: &[&str]) -> Vec<Cell> {
        let mut regs = boot();
        for line in lines {
            outer_interpreter(&mut regs, line).unwrap();
        }
        regs.data_stack
    }

    // Asserts that evaluating `input` fails with a message containing `expected`.
    unsafe fn assert_error(regs: &mut Registers, input: &str, expected: &str) {
        let message = outer_interpreter(regs, input).unwrap_err();
        assert!(message.contains(expected), "got: {}", message);
    }

    #[test]
    fn arithmetic_and_literals() {
        unsafe {
            assert_eq!(eval_stack(&["40 2 +"]), vec![42]);
            assert_eq!(eval_stack(&["-5 3 +"]), vec![-2isize as Cell]);
            assert_eq!(eval_stack(&["18446744073709551615"]), vec![Cell::MAX]);
        }
    }

    #[test]
    fn create_comma_fetch() {
        unsafe {
            assert_eq!(eval_stack(&["CREATE X 1337 , X @"]), vec![1337]);
        }
    }

    #[test]
    fn name_is_owned_by_the_dictionary() {
        unsafe {
            let mut regs = boot();
            let line = String::from("CREATE X 1 ,");
            outer_interpreter(&mut regs, &line).unwrap();
            drop(line);
            outer_interpreter(&mut regs, "X @").unwrap();
            assert_eq!(regs.data_stack, vec![1]);
        }
    }

    #[test]
    fn colon_definitions_nest() {
        unsafe {
            let lines = [": ADD3 3 + ;", ": ADD6 ADD3 ADD3 ;", "36 ADD6"];
            assert_eq!(eval_stack(&lines), vec![42]);
        }
    }

    #[test]
    fn definition_can_span_lines() {
        unsafe {
            assert_eq!(eval_stack(&[": A 1", "2 + ;", "A"]), vec![3]);
        }
    }

    #[test]
    fn redefinition_calls_the_previous_word() {
        unsafe {
            assert_eq!(eval_stack(&[": X 1 ;", ": X X 2 + ;", "X"]), vec![3]);
        }
    }

    #[test]
    fn exit_leaves_a_definition_early() {
        unsafe {
            assert_eq!(eval_stack(&[": A 1 EXIT 2 ;", "A"]), vec![1]);
        }
    }

    #[test]
    fn does_gives_each_child_its_own_data() {
        unsafe {
            let lines = [
                ": CONSTANT CREATE , DOES> @ ;",
                "42 CONSTANT ANSWER",
                "7 CONSTANT LUCKY",
                "ANSWER LUCKY ANSWER",
            ];
            assert_eq!(eval_stack(&lines), vec![42, 7, 42]);
        }
    }

    #[test]
    fn does_requires_a_definition() {
        unsafe {
            let mut regs = boot();
            assert_error(&mut regs, "CREATE X DOES>", "inside a definition");
        }
    }

    #[test]
    fn does_requires_a_created_word() {
        unsafe {
            let mut regs = boot();
            outer_interpreter(&mut regs, ": BAD 1 DOES> ;").unwrap();
            assert_error(&mut regs, "BAD", "made by CREATE");
        }
    }

    #[test]
    fn failed_definition_is_rolled_back() {
        unsafe {
            let mut regs = boot();
            let here = regs.here;
            assert_error(&mut regs, ": BROKEN 1 NOPE ;", "not recognized");
            assert_eq!(regs.here, here);
            assert!(!regs.compiling);
            assert_error(&mut regs, "BROKEN", "not recognized");
            outer_interpreter(&mut regs, "1 2 +").unwrap();
            assert_eq!(regs.data_stack, vec![3]);
        }
    }

    #[test]
    fn stack_underflow_is_reported() {
        unsafe {
            let mut regs = boot();
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
            let mut regs = boot();
            assert_error(&mut regs, ";", "while compiling");
            assert_error(&mut regs, ":", "needs a name");
            assert_error(&mut regs, "CREATE", "needs a name");
            assert_error(&mut regs, "LIT", "not recognized");
        }
    }

    #[test]
    fn dictionary_overflow_is_reported() {
        unsafe {
            let mut regs = boot();
            let cells = "1 , ".repeat(DICTIONARY_CELLS);
            assert_error(&mut regs, &cells, "Dictionary overflow");
            assert_eq!(regs.here, regs.dictionary_end);
            assert_error(&mut regs, "CREATE X", "Dictionary overflow");
        }
    }

    #[test]
    fn long_threads_do_not_grow_the_rust_stack() {
        unsafe {
            let mut regs = boot();
            outer_interpreter(&mut regs, ": NOOP ;").unwrap();
            let cfa = find_word(&regs, "NOOP").unwrap().add(CFA);

            let mut thread = vec![cfa as Cell; 5_000_000];
            thread.push(0);
            regs.ip = thread.as_ptr();
            run(&mut regs);

            assert!(regs.error.is_none());
            assert!(regs.return_stack.is_empty());
        }
    }
}
