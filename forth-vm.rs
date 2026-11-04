use std::mem;
use std::ptr;

// --- 1. THE FOUNDATION: THE FORTH CELL ---
// A Cell is the universal basic unit of Forth (64-bit on a 64-bit CPU).
// It blindly holds data, flags, offsets, or raw memory pointers.
type Cell = usize;

// Definition of a Forth primitive (machine-code function).
type PrimitiveFn = unsafe fn(registers: &mut Registers);

// CPU Registers and Stacks conforming to the Forth hardware architecture
struct Registers {
    ip: *const Cell,                // Instruction Pointer: points to a Cell in a thread
    w: *const Cell,                 // Word Register: points to the currently active CFA Cell
    data_stack: Vec<Cell>,          // Parameter stack (contains raw Cells)
    return_stack: Vec<*const Cell>, // Return stack (R-stack) for nested IP pointers
    here: *mut Cell,                // Dictionary pointer to the next free Cell
    last_word: *mut Cell,           // Pointer to the CFA of the most recently created word
}

// --- 2. THE INNER INTERPRETER (NEXT) ---
// The heart of the Forth engine. Fetches the CFA, increments IP, and jumps to the code.
#[inline(always)]
unsafe fn next(regs: &mut Registers) {
    // W = *IP (Load the address of the current CFA Cell into the W register)
    regs.w = *regs.ip as *const Cell;

    // IP++ (Step to the next Cell in the currently executing thread)
    regs.ip = regs.ip.add(1);

    // Fetch the contents of the CFA Cell (the raw bits of the machine-code pointer)
    let code_bits: Cell = *regs.w;

    // Blindly transmute the raw bits into an executable Rust function (like a hardware jump)
    let code: PrimitiveFn = mem::transmute(code_bits);
    code(regs);
}

// --- 3. CORE PRIMITIVES (DOCOL, DOVAR, DODOES) ---

// DOCOL: Executed when entering a nested `:` (colon) word.
unsafe fn docol(regs: &mut Registers) {
    // Push the current IP to the Return Stack (return address)
    regs.return_stack.push(regs.ip);
    // Move IP to the inside of the definition (physically 1 Cell after the CFA, so W + 1)
    regs.ip = regs.w.add(1) as *const Cell;
    next(regs);
}

// DOVAR: Default execution semantics for a word created via CREATE (or VARIABLE).
// It simply pushes the memory address of its own Data Field (W + 1) onto the stack.
unsafe fn dovar(regs: &mut Registers) {
    let data_address = regs.w.add(1) as Cell;
    regs.data_stack.push(data_address);
    next(regs);
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
    next(regs);
}

// EXIT: Cleans up and exits a nested `:` word (compiled by `;`)
unsafe fn exit(regs: &mut Registers) {
    if let Some(old_ip) = regs.return_stack.pop() {
        regs.ip = old_ip;
    }
    next(regs);
}

// --- 4. DATA AND COMPILER PRIMITIVES ---

// LIT: Fetches the inline literal value from the thread, pushes it, and skips the data cell.
unsafe fn lit(regs: &mut Registers) {
    let literal_value = *regs.ip;
    regs.data_stack.push(literal_value);
    regs.ip = regs.ip.add(1); // Skip the inline data cell
    next(regs);
}

// COMMA ( , ): Pops a Cell from the stack and presses it into the dictionary at HERE.
unsafe fn comma(regs: &mut Registers) {
    if let Some(val) = regs.data_stack.pop() {
        *regs.here = val;
        regs.here = regs.here.add(1);
    }
    next(regs);
}

// FETCH ( @ ): Pops an address from the stack and replaces it with the contents of that Cell.
unsafe fn fetch(regs: &mut Registers) {
    if let Some(addr) = regs.data_stack.pop() {
        let ptr = addr as *const Cell;
        regs.data_stack.push(*ptr);
    }
    next(regs);
}

// PLUS ( + ): Pops two Cells from the stack, adds them, and pushes the result.
unsafe fn plus(regs: &mut Registers) {
    if let (Some(b), Some(a)) = (regs.data_stack.pop(), regs.data_stack.pop()) {
        regs.data_stack.push(a.wrapping_add(b));
    }
    next(regs);
}

// PRINT ( . ): Pops a Cell from the stack and prints it to stdout.
unsafe fn dot_print(regs: &mut Registers) {
    if let Some(val) = regs.data_stack.pop() {
        println!("Forth Output: {}", val);
    }
    next(regs);
}

// --- 5. RUNTIME CREATE AND DOES> MANIPULATION ---

// CREATE: Allocates a new dictionary word structure with default DOVAR semantics.
unsafe fn create(regs: &mut Registers, dovar_ptr: Cell, name_ptr: Cell) {
    let current_here = regs.here;

    *regs.here = regs.last_word as Cell; // LFA: points to the previous word in the list
    regs.here = regs.here.add(1);

    *regs.here = name_ptr; // NFA: points to the string slice bytes
    regs.here = regs.here.add(1);

    let cfa_location = regs.here; // This marks the CFA of our new child word
    *regs.here = dovar_ptr; // Set initial default DOVAR semantics
    regs.here = regs.here.add(1);

    // Update 'last_word' header pointer to point to the LFA of this new word
    regs.last_word = current_here;
}

// (DOES>): Hijacks the CFA of the child word while the defining word is executing.
unsafe fn runtime_does(regs: &mut Registers, dodoes_ptr: Cell) {
    // Locate the CFA of the word just created by CREATE.
    // Since regs.last_word points to the LFA: LFA is +0, NFA is +1, String Len is +2, CFA is +3.
    let kind_lfa = regs.last_word;
    let kind_cfa = kind_lfa.add(3);

    *kind_cfa = dodoes_ptr; // Overwrite the child's CFA with DODOES
    *kind_cfa.add(1) = regs.ip as Cell; // Store the pointer to the action code in W+1 of the child

    // Unnest from the defining word (EXIT behavior)
    if let Some(old_ip) = regs.return_stack.pop() {
        regs.ip = old_ip;
    }
    next(regs);
}

// --- 6. HELPER FUNCTION: INJECT PRIMITIVES DIRECTLY INTO THE DICTIONARY ---
// Simulates the built-in bootstrap dictionary populated when a Forth system boots.
unsafe fn add_primitive_word(regs: &mut Registers, name: &'static str, code_fn: PrimitiveFn) {
    let current_here = regs.here;

    *regs.here = regs.last_word as Cell; // 1. LFA
    regs.here = regs.here.add(1);

    *regs.here = name.as_ptr() as Cell; // 2. NFA (Pointer to string bytes)
    regs.here = regs.here.add(1);

    // Store the string length in the adjacent cell to facilitate direct string matching
    *regs.here = name.len() as Cell;
    regs.here = regs.here.add(1);

    *regs.here = code_fn as Cell; // 3. CFA (The machine code pointer)
    regs.here = regs.here.add(1);

    regs.last_word = current_here; // Update the head pointer of the dictionary
}

// --- 7. THE TEXT INTERPRETER / OUTER INTERPRETER ---
unsafe fn outer_interpreter(regs: &mut Registers, input: &str) {
    // Tokenize the input string by whitespace (spaces, tabs, newlines)
    for token in input.split_whitespace() {
        let mut found_word_cfa: Option<*mut Cell> = None;
        let mut current_lfa = regs.last_word;

        // Linearly traverse the linked list of the dictionary (from newest to oldest)
        while !current_lfa.is_null() {
            let nfa_ptr = *current_lfa.add(1) as *const u8;
            let len = *current_lfa.add(2) as usize;

            // Reconstruct the string slice from raw dictionary data pointers
            let dict_word_name =
                std::str::from_utf8(std::slice::from_raw_parts(nfa_ptr, len)).unwrap_or("");

            if dict_word_name == token {
                // Found! The CFA is located at LFA + 3 cells (LFA, NFA, LEN, then CFA)
                found_word_cfa = Some(current_lfa.add(3));
                break;
            }
            // Follow the link pointer to the previous word (LFA)
            current_lfa = *current_lfa as *mut Cell;
        }

        if let Some(cfa) = found_word_cfa {
            // --- CASE A: THE TOKEN IS A DICTIONARY WORD ---
            // To safely execute this single binaire CFA via NEXT, we construct a mini 'trampoline' thread in RAM:
            // [ Target CFA Address ] [ Stop / End Marker (0) ]
            let trampoline: [Cell; 2] = [cfa as Cell, 0];
            regs.ip = trampoline.as_ptr();

            // Kickstart the Inner Interpreter engine
            next(regs);
        } else {
            // --- CASE B: THE TOKEN IS A NUMBER (LITERAL) ---
            if let Ok(number) = token.parse::<Cell>() {
                regs.data_stack.push(number); // Push directly onto the parameter stack
            } else {
                // --- CASE C: SYNTAX / COMPILATION ERROR ---
                println!(
                    "Forth Error: Word or numeric literal '{}' not recognized.",
                    token
                );
                return;
            }
        }
    }
}

fn main() {
    unsafe {
        // Allocate an untyped arena for our Forth dictionary space
        let mut dictionary_space = vec![0 as Cell; 1024];

        let mut regs = Registers {
            ip: ptr::null(),
            w: ptr::null(),
            data_stack: Vec::new(),
            return_stack: Vec::new(),
            here: dictionary_space.as_mut_ptr(),
            last_word: ptr::null_mut(), // Dictionary starts out empty
        };

        // Bootstrap the dictionary: Register our core primitives under their official text names
        let f_dovar = dovar as Cell;
        let f_dodoes = dodoes as Cell;

        add_primitive_word(&mut regs, "+", plus);
        add_primitive_word(&mut regs, ".", dot_print);
        add_primitive_word(&mut regs, "@", fetch);
        add_primitive_word(&mut regs, ",", comma);

        // Inject bootstrap macro definitions for CREATE and DOES> structures
        add_primitive_word(&mut regs, "CREATE", |r| {
            // In a live system, CREATE parses the next token from the input stream.
            // For simplicity here, we simulate it by hardcoding a mock child word name.
            let simulated_name = "STATIONARY-SOURCE";
            create(r, f_dovar, simulated_name.as_ptr() as Cell);
            // Append the parsed string length right after the NFA
            *r.here = simulated_name.len();
            r.here = r.here.add(1);
        });
        add_primitive_word(&mut regs, "DOES>", |r| runtime_does(r, f_dodoes));
        println!("--- FORTH BOOTSEQUENCE COMPLETED ---");
        println!("The Text Interpreter (Outer Interpreter) is online.\n");
        // --- TEST RUN 1: Evaluating Text Operations ---
        println!("Input: '40 2 +'");
        outer_interpreter(&mut regs, "40 2 +");
        // Parameter stack now contains 42. Print it out using the Forth '.' primitive
        println!("Input: '.'");
        outer_interpreter(&mut regs, ".");
        println!("-------------------------------------\n");
        // --- TEST RUN 2: Dynamic Creation via Text interpreter ---
        println!("Input: 'CREATE'");
        outer_interpreter(&mut regs, "CREATE"); // Creates the word "STATIONARY-SOURCE" in the dictionary
        println!("Input: '1337 ,'");
        outer_interpreter(&mut regs, "1337 ,"); // Compiles the value 1337 into the Data Field of the new word
                                                // We can now reference our dynamically generated word directly via text input!
        println!("Input: 'STATIONARY-SOURCE @ .'");
        outer_interpreter(&mut regs, "STATIONARY-SOURCE @ .");
        println!("-------------------------------------\n");
    }
}
