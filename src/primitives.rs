use crate::cell::Cell;
use crate::dictionary::{cfa_of, is_created, F_CREATED, F_HIDDEN, F_IMMEDIATE};
use crate::vm::{code_cell, Registers};

// --- CORE PRIMITIVES (DOCOL, DOVAR, DODOES) ---

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

// --- DATA AND COMPILER PRIMITIVES ---

// LIT: Fetches the inline literal value from the thread, pushes it, and skips the data cell.
unsafe fn lit(regs: &mut Registers) {
    let literal_value = *regs.ip;
    regs.data_stack.push(literal_value);
    regs.ip = regs.ip.add(1); // Skip the inline data cell
}

// COMMA ( , ): Pops a Cell from the stack and presses it into the dictionary at HERE.
unsafe fn comma(regs: &mut Registers) {
    if let Some(value) = regs.pop() {
        regs.comma(value);
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

// --- RUNTIME CREATE AND DOES> MANIPULATION ---

// CREATE: Parses a name and allocates a new word with default DOVAR semantics.
unsafe fn create(regs: &mut Registers) {
    let Some(name) = regs.input.parse_name() else {
        regs.set_error("CREATE needs a name.");
        return;
    };
    if regs
        .write_header(&name, code_cell(dovar), F_CREATED)
        .is_some()
    {
        regs.comma(0); // Reserve the action cell for DOES>
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
    let child = regs.dictionary.last_word();
    if !is_created(child) {
        regs.set_error("DOES> needs a word made by CREATE.");
        return;
    }

    let child_cfa = cfa_of(child);
    *child_cfa = code_cell(dodoes); // Overwrite the child's CFA with DODOES
    *child_cfa.add(1) = regs.ip as Cell; // Store the pointer to the action code in W+1 of the child

    // Unnest from the defining word (EXIT behavior)
    exit(regs);
}

// COLON ( : ): Parses a name and switches the text interpreter to compiling.
// The word stays hidden until `;`, so a redefinition can still call the old word.
unsafe fn colon(regs: &mut Registers) {
    let Some(name) = regs.input.parse_name() else {
        regs.set_error(": needs a name.");
        return;
    };
    let restore_point = regs.dictionary.mark();
    if regs
        .write_header(&name, code_cell(docol), F_HIDDEN)
        .is_some()
    {
        regs.compiler.definition = Some(restore_point);
        regs.compiler.compiling = true;
    }
}

// SEMICOLON ( ; ): Ends the current definition. This word is immediate.
unsafe fn semicolon(regs: &mut Registers) {
    if !regs.compiler.compiling {
        regs.set_error("; can only be used while compiling.");
        return;
    }
    regs.comma(regs.compiler.exit_cfa);
    if regs.error.is_none() {
        regs.dictionary.reveal_last();
        regs.compiler.definition = None;
        regs.compiler.compiling = false;
    }
}

// Registers the core primitives under their official text names.
pub(crate) unsafe fn install(regs: &mut Registers) {
    regs.dictionary.add_primitive_word("+", code_cell(plus), 0);
    regs.dictionary
        .add_primitive_word(".", code_cell(dot_print), 0);
    regs.dictionary.add_primitive_word("@", code_cell(fetch), 0);
    regs.dictionary.add_primitive_word(",", code_cell(comma), 0);
    regs.dictionary
        .add_primitive_word("CREATE", code_cell(create), 0);
    regs.dictionary
        .add_primitive_word("DOES>", code_cell(runtime_does), 0);
    regs.dictionary.add_primitive_word(":", code_cell(colon), 0);
    regs.dictionary
        .add_primitive_word(";", code_cell(semicolon), F_IMMEDIATE);
    let exit_cfa = regs
        .dictionary
        .add_primitive_word("EXIT", code_cell(exit), 0);
    // LIT reads its operand from the thread, so it only makes sense in compiled code.
    let lit_cfa = regs
        .dictionary
        .add_primitive_word("LIT", code_cell(lit), F_HIDDEN);
    regs.compiler.link_primitives(lit_cfa, exit_cfa);
}
