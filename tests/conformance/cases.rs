// Copyright (c) 2026 M.W.R. Hoiting

//! Small, independent public-API probes. Stack expectations are bottom to top.

use std::cell::RefCell;
use std::io::{self, Write};
use std::rc::Rc;

use rust_forth_vm::{boot, outer_interpreter, Cell, ForthError, VmConfig};

/// An intentional transfer back to the host interpreter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stop {
    /// Normal completion.
    None,
    /// QUIT preserves the data stack.
    Quit,
    /// ABORT empties the data stack.
    Abort,
}

/// One independent scenario, including all its observed effects.
#[derive(Clone, Debug)]
pub struct Case {
    pub id: String,
    pub words: Vec<&'static str>,
    pub profile: Option<&'static str>,
    pub source: String,
    pub stack: Vec<isize>,
    pub output: String,
    pub stop: Stop,
    pub resume: String,
}

impl Case {
    /// Creates a stack probe; a sentinel and final depth check catch extra consumption/results.
    fn stack(id: &str, words: &[&'static str], source: &str, stack: &[isize]) -> Self {
        Self {
            id: id.into(),
            words: words.to_vec(),
            profile: None,
            source: source.into(),
            stack: stack.to_vec(),
            output: String::new(),
            stop: Stop::None,
            resume: String::new(),
        }
    }
}

/// A writer whose captured bytes remain accessible to the host.
#[derive(Clone, Default)]
struct Capture(Rc<RefCell<Vec<u8>>>);

impl Write for Capture {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.borrow_mut().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Substitutes native Cell constants without assuming a host pointer width.
fn expand(source: &str) -> String {
    source
        .replace("{cell}", &size_of::<Cell>().to_string())
        .replace("{bits}", &Cell::BITS.to_string())
        .replace("{max}", &isize::MAX.to_string())
        .replace("{min}", &isize::MIN.to_string())
}

/// Executes only valid, bounded Forth scenarios and observes their exact public output.
pub fn run(case: &Case) -> Result<(), String> {
    let output = Capture::default();
    // SAFETY: the VM is driven only through the public API; probes use allocated application
    // memory and balanced control flow, never forged xts, raw addresses, or unsafe return stacks.
    unsafe {
        let mut vm = boot(output.clone(), VmConfig::new(8192)).map_err(|e| e.to_string())?;
        outer_interpreter(&mut vm, "31415").map_err(|e| e.to_string())?;
        let result = outer_interpreter(&mut vm, &expand(&case.source));
        let correct_stop = matches!(
            (&result, case.stop),
            (Ok(()), Stop::None)
                | (Err(ForthError::Quit), Stop::Quit)
                | (Err(ForthError::Abort), Stop::Abort)
        );
        if !correct_stop {
            return Err(format!("unexpected completion: {result:?}"));
        }
        outer_interpreter(&mut vm, &expand(&case.resume)).map_err(|e| e.to_string())?;
        let sentinel = usize::from(case.stop != Stop::Abort);
        let observer = format!("{} DEPTH .", ". ".repeat(case.stack.len() + sentinel));
        outer_interpreter(&mut vm, &observer).map_err(|e| e.to_string())?;
        let mut expected = case.output.clone();
        for value in case.stack.iter().rev() {
            expected.push_str(&format!("{value} "));
        }
        if sentinel != 0 {
            expected.push_str("31415 ");
        }
        expected.push_str("0 ");
        if vm.is_compiling() {
            return Err("probe left compilation open".into());
        }
        let actual = output.0.borrow();
        if *actual != expected.as_bytes() {
            return Err(format!(
                "expected {:?}, got {:?}",
                expected,
                String::from_utf8_lossy(&actual)
            ));
        }
    }
    Ok(())
}

/// All probes, including profile-specific numeric and memory expectations.
pub fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    macro_rules! stack {
        ($id:expr, [$($word:expr),+], $source:expr, [$($value:expr),*]) => {
            cases.push(Case::stack($id, &[$($word),+], $source, &[$($value),*]));
        };
    }
    stack!("observer", [".", "DEPTH"], "11 -9 . DEPTH", [11, 2]);
    cases.last_mut().unwrap().output = "-9 ".into();
    stack!("drop", ["DROP"], "4 5 DROP", [4]);
    stack!("dup", ["DUP"], "5 DUP", [5, 5]);
    stack!("qdup-zero", ["?DUP"], "0 ?DUP", [0]);
    stack!("qdup-nonzero", ["?DUP"], "-2 ?DUP", [-2, -2]);
    stack!("swap", ["SWAP"], "1 2 SWAP", [2, 1]);
    stack!("over", ["OVER"], "1 2 OVER", [1, 2, 1]);
    stack!("rot", ["ROT"], "1 2 3 ROT", [2, 3, 1]);
    stack!("pick", ["PICK"], "1 2 3 0 PICK 3 PICK", [1, 2, 3, 3, 1]);
    stack!("roll", ["ROLL"], "1 2 3 2 ROLL 0 ROLL", [2, 3, 1]);
    stack!("two-drop", ["2DROP"], "1 2 3 2DROP", [1]);
    stack!("two-dup", ["2DUP"], "1 2 2DUP", [1, 2, 1, 2]);
    stack!("two-over", ["2OVER"], "1 2 3 4 2OVER", [1, 2, 3, 4, 1, 2]);
    stack!("two-swap", ["2SWAP"], "1 2 3 4 2SWAP", [3, 4, 1, 2]);
    stack!("add", ["+"], "7 -9 + 0 0 +", [-2, 0]);
    stack!("subtract", ["-"], "7 9 - -7 -9 -", [-2, 2]);
    stack!("multiply", ["*"], "7 -9 * 0 42 *", [-63, 0]);
    stack!("increments", ["1+", "1-"], "-1 1+ 0 1-", [0, -1]);
    stack!("halve", ["2/"], "7 2/ -7 2/ -8 2/", [3, -4, -4]);
    stack!("double", ["2*"], "7 2* -7 2*", [14, -14]);
    stack!("abs", ["ABS"], "-7 ABS 7 ABS 0 ABS", [7, 7, 0]);
    stack!(
        "negate",
        ["NEGATE"],
        "7 NEGATE -7 NEGATE 0 NEGATE",
        [-7, 7, 0]
    );
    stack!(
        "minmax",
        ["MIN", "MAX"],
        "-7 3 MIN -7 3 MAX 4 4 MIN 4 4 MAX",
        [-7, 3, 4, 4]
    );
    stack!(
        "divide-signs",
        ["/"],
        "7 3 / -7 3 / 7 -3 / -7 -3 /",
        [2, -3, -3, 2]
    );
    stack!(
        "mod-signs",
        ["MOD"],
        "7 3 MOD -7 3 MOD 7 -3 MOD -7 -3 MOD",
        [1, 2, -2, -1]
    );
    stack!(
        "divmod-signs",
        ["/MOD"],
        "7 3 /MOD -7 3 /MOD 7 -3 /MOD -7 -3 /MOD",
        [1, 2, 2, -3, -2, -3, -1, 2]
    );
    stack!("equal", ["="], "0 0 = 7 8 = -1 -1 =", [-1, 0, -1]);
    stack!(
        "signed-order",
        ["<", ">"],
        "-1 0 < 0 -1 < 0 0 < 0 -1 > -1 0 > 0 0 >",
        [-1, 0, 0, -1, 0, 0]
    );
    stack!(
        "unsigned-order",
        ["U<"],
        "0 -1 U< -1 0 U< -1 -1 U<",
        [-1, 0, 0]
    );
    stack!(
        "zero-order",
        ["0<", "0=", "0>"],
        "-1 0< 0 0< 1 0< -1 0= 0 0= 1 0= -1 0> 0 0> 1 0>",
        [-1, 0, 0, 0, -1, 0, 0, 0, -1]
    );
    stack!("and", ["AND"], "5 3 AND -1 42 AND 0 -1 AND", [1, 42, 0]);
    stack!("or", ["OR"], "5 3 OR 0 42 OR", [7, 42]);
    stack!("xor", ["XOR"], "5 3 XOR -1 -1 XOR", [6, 0]);
    stack!(
        "invert",
        ["INVERT", "NOT"],
        "0 INVERT -1 INVERT 5 INVERT",
        [-1, 0, -6]
    );
    // NOT and INVERT share the bitwise expectation, but must each actually execute.
    stack!("not", ["NOT"], "0 NOT -1 NOT 5 NOT", [-1, 0, -6]);
    cases.iter_mut().find(|c| c.id == "invert").unwrap().words = vec!["INVERT"];
    stack!(
        "shift-left",
        ["LSHIFT"],
        "1 0 LSHIFT 1 3 LSHIFT 3 2 LSHIFT",
        [1, 8, 12]
    );
    stack!(
        "shift-right",
        ["RSHIFT"],
        "8 0 RSHIFT 8 3 RSHIFT -1 1 RSHIFT",
        [8, 1, isize::MAX]
    );
    stack!("blank", ["BL"], "BL", [32]);
    stack!(
        "cell-size",
        ["CELL+", "CELLS"],
        "1 CELLS 0 CELL+ 3 CELLS",
        [
            size_of::<Cell>() as isize,
            size_of::<Cell>() as isize,
            3 * size_of::<Cell>() as isize
        ]
    );
    stack!("char-size", ["CHAR+", "CHARS"], "0 CHAR+ 3 CHARS", [1, 3]);
    stack!(
        "fetch-store",
        ["!", "@", "VARIABLE"],
        "VARIABLE V 42 V ! V @ -17 V ! V @",
        [42, -17]
    );
    stack!("plus-store", ["+!"], "VARIABLE V 7 V ! -9 V +! V @", [-2]);
    stack!(
        "char-fetch-store",
        ["C!", "C@"],
        "CREATE B 4 ALLOT 65 B C! 255 B 1+ C! B C@ B 1+ C@",
        [65, 255]
    );
    stack!(
        "two-store-order",
        ["2!"],
        "CREATE B 2 CELLS ALLOT 11 22 B 2! B @ B CELL+ @",
        [22, 11]
    );
    stack!(
        "two-fetch-order",
        ["2@"],
        "CREATE B 2 CELLS ALLOT 22 B ! 11 B CELL+ ! B 2@",
        [11, 22]
    );
    stack!(
        "comma",
        [",", "HERE"],
        "CREATE B 123 , -45 , B @ B {cell} + @ HERE B -",
        [123, -45, 2 * size_of::<Cell>() as isize]
    );
    stack!(
        "char-comma",
        ["C,"],
        "CREATE B 65 C, 66 C, B C@ B 1+ C@ HERE B -",
        [65, 66, 2]
    );
    stack!(
        "allot",
        ["ALLOT"],
        "HERE 8 ALLOT HERE SWAP - HERE 0 ALLOT HERE SWAP -",
        [8, 0]
    );
    stack!(
        "allot-negative",
        ["ALLOT"],
        "8 ALLOT HERE -4 ALLOT HERE SWAP -",
        [-4]
    );
    stack!(
        "align",
        ["ALIGN", "ALIGNED"],
        "CREATE B 1 ALLOT ALIGN HERE DUP ALIGNED = B 1+ ALIGNED B -",
        [-1, size_of::<Cell>() as isize]
    );
    stack!(
        "create-body",
        ["CREATE", ">BODY"],
        "CREATE B 42 , B ' B >BODY = ' B >BODY @",
        [-1, 42]
    );
    stack!(
        "constant",
        ["CONSTANT"],
        "42 CONSTANT ANSWER ANSWER : C ANSWER ; C",
        [42, 42]
    );
    stack!("colon-exit", [":", ";", "EXIT"], ": C 4 EXIT 5 ; C", [4]);
    stack!("tick-execute", ["'", "EXECUTE"], "7 ' DUP EXECUTE", [7, 7]);
    stack!("bracket-tick", ["[']"], ": C ['] DUP EXECUTE ; 7 C", [7, 7]);
    stack!(
        "immediate",
        ["IMMEDIATE"],
        "VARIABLE V 0 V ! : SET7 7 V ! ; IMMEDIATE : C SET7 ; V @ C V @",
        [7, 7]
    );
    stack!(
        "does",
        ["DOES>"],
        ": MAKER CREATE , DOES> @ ; 42 MAKER ANSWER ANSWER",
        [42]
    );
    stack!(
        "if-else-then",
        ["IF", "ELSE", "THEN"],
        ": C IF 11 ELSE 22 THEN ; 0 C 1 C -2 C",
        [22, 11, 11]
    );
    stack!(
        "if-without-else",
        ["IF", "THEN"],
        ": C IF 11 THEN 22 ; 0 C 3 C",
        [22, 11, 22]
    );
    stack!(
        "nested-control",
        ["IF", "ELSE", "THEN"],
        ": C IF IF 1 ELSE 2 THEN ELSE DROP 3 THEN ; 0 0 C 0 1 C 1 1 C",
        [3, 2, 1]
    );
    stack!(
        "begin-until",
        ["BEGIN", "UNTIL"],
        ": C BEGIN 1- DUP 0= UNTIL ; 3 C",
        [0]
    );
    stack!(
        "paren-comment",
        ["("],
        "( ignored tokens ) 5 : C ( comment ) 6 ; C",
        [5, 6]
    );
    stack!("char", ["CHAR"], "CHAR A CHAR z", [65, 122]);
    stack!("bracket-char", ["[CHAR]"], ": C [CHAR] A ; C", [65]);
    stack!(
        "count",
        ["COUNT"],
        "CREATE B 4 ALLOT 2 B C! 65 B 1+ C! 66 B 2 + C! B COUNT SWAP B -",
        [2, 1]
    );
    stack!("fill", ["FILL"], "CREATE B 4 ALLOT B 4 65 FILL B 1+ 2 66 FILL B C@ B 1+ C@ B 2 + C@ B 3 + C@ B 0 0 FILL B C@", [65, 66, 66, 65, 65]);
    stack!("move-overlap", ["MOVE"], "CREATE B 4 ALLOT 1 B C! 2 B 1+ C! 3 B 2 + C! 4 B 3 + C! B B 1+ 3 MOVE B C@ B 1+ C@ B 2 + C@ B 3 + C@ B 1+ B 3 MOVE B C@ B 1+ C@ B 2 + C@", [1, 1, 2, 3, 1, 2, 3]);
    stack!("cmove-forward", ["CMOVE"], "CREATE B 4 ALLOT 1 B C! 2 B 1+ C! 3 B 2 + C! B B 1+ 3 CMOVE B C@ B 1+ C@ B 2 + C@ B 3 + C@", [1, 1, 1, 1]);
    stack!("cmove-backward", ["CMOVE>"], "CREATE B 4 ALLOT 1 B C! 2 B 1+ C! 3 B 2 + C! B B 1+ 3 CMOVE> B C@ B 1+ C@ B 2 + C@ B 3 + C@", [1, 1, 2, 3]);
    stack!("emit", ["EMIT"], "65 EMIT 126 EMIT", []);
    cases.last_mut().unwrap().output = "A~".into();
    let mut ascii = Case::stack("ascii-graphics", &["EMIT"], "", &[]);
    for code in 32_u8..=126 {
        ascii.source.push_str(&format!("{code} EMIT "));
        ascii.output.push(char::from(code));
    }
    cases.push(ascii);
    stack!(
        "type",
        ["TYPE"],
        "CREATE B 3 ALLOT 65 B C! 66 B 1+ C! 67 B 2 + C! B 3 TYPE B 0 TYPE",
        []
    );
    cases.last_mut().unwrap().output = "ABC".into();
    stack!("space", ["SPACE"], "SPACE", []);
    cases.last_mut().unwrap().output = " ".into();
    stack!("spaces", ["SPACES"], "0 SPACES 3 SPACES", []);
    cases.last_mut().unwrap().output = "   ".into();
    stack!("iso-spaces-negative", ["SPACES"], "-2 SPACES", []);
    cases.last_mut().unwrap().profile = Some("iso");
    stack!("cr", ["CR"], "CR", []);
    cases.last_mut().unwrap().output = "\n".into();
    stack!("dot-quote", [".\""], ": C .\" hello\" ; C", []);
    cases.last_mut().unwrap().output = "hello".into();
    stack!("s-quote", ["S\""], ": C S\" hello\" ; C TYPE", []);
    cases.last_mut().unwrap().output = "hello".into();
    stack!(
        "abort-quote-false",
        ["ABORT\""],
        ": C ABORT\" failed\" 7 ; 0 C",
        [7]
    );
    stack!(
        "abort-quote-true",
        ["ABORT\""],
        ": C ABORT\" failed\" 7 ; 9 -2 C 99",
        []
    );
    cases.last_mut().unwrap().stop = Stop::Abort;
    cases.last_mut().unwrap().output = "failed".into();
    stack!("abort", ["ABORT"], ": C ABORT 99 ; 9 C 100", []);
    cases.last_mut().unwrap().stop = Stop::Abort;
    stack!("quit", ["QUIT"], ": C QUIT 99 ; 9 C 100", [9]);
    cases.last_mut().unwrap().stop = Stop::Quit;
    // Resuming input verifies the host-visible continuation after QUIT/ABORT.
    for case in cases.iter_mut().filter(|c| c.stop != Stop::None) {
        case.resume = "8 DROP".into();
    }
    stack!("name-search", [":", ";"], ": SAME 1 ; : OLD SAME ; : SAME 2 ; OLD SAME : ABCDEFGHIJKLMNOPQRSTUVWXYZABCDE 3 ; ABCDEFGHIJKLMNOPQRSTUVWXYZABCDE", [1, 2, 3]);
    stack!("delimiters", [":", ";"], "1\t2\n+ : C\t3\n; C", [3, 3]);
    stack!(
        "integer-input",
        ["."],
        "-32768 32767 65535",
        [-32768, 32767, 65535]
    );
    stack!(
        "dictionary-capacity",
        ["ALLOT"],
        "CREATE B 2000 ALLOT HERE B -",
        [2000]
    );
    stack!(
        "data-capacity",
        ["DEPTH"],
        "0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31",
        [
            0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23,
            24, 25, 26, 27, 28, 29, 30, 31
        ]
    );
    stack!("return-capacity", [":", ";"], ": N0 1 ; : N1 N0 ; : N2 N1 ; : N3 N2 ; : N4 N3 ; : N5 N4 ; : N6 N5 ; : N7 N6 ; : N8 N7 ; : N9 N8 ; : N10 N9 ; : N11 N10 ; : N12 N11 ; : N13 N12 ; : N14 N13 ; : N15 N14 ; : N16 N15 ; : N17 N16 ; : N18 N17 ; : N19 N18 ; : N20 N19 ; : N21 N20 ; : N22 N21 ; : N23 N22 ; : N24 N23 ; N24", [1]);
    cases
        .iter_mut()
        .find(|c| c.id == "integer-input")
        .unwrap()
        .profile = Some("iso");
    // These explicit 16-bit expectations must not be widened to the native Cell.
    stack!("f83-add-wrap", ["+", "1+"], "32767 1+", [-32768]);
    stack!("f83-minus-wrap", ["-", "1-"], "-32768 1-", [32767]);
    stack!("f83-multiply-wrap", ["*"], "256 256 *", [0]);
    stack!("f83-not-width", ["NOT"], "0 NOT 65535 =", [-1]);
    stack!(
        "f83-integer-input",
        ["."],
        "-32768 32767 65535",
        [-32768, 32767, -1]
    );
    stack!("f83-input-bits", ["."], "65535 0<", [-1]);
    stack!("f83-abs-min", ["ABS"], "-32768 ABS", [-32768]);
    stack!("f83-negate-min", ["NEGATE"], "-32768 NEGATE", [-32768]);
    stack!(
        "f83-variable-size",
        ["VARIABLE"],
        "VARIABLE V HERE V -",
        [2]
    );
    stack!("f83-comma-size", [","], "CREATE B 7 , HERE B -", [2]);
    for case in cases.iter_mut().filter(|c| c.id.starts_with("f83-")) {
        case.profile = Some("f83");
    }
    stack!(
        "iso-extremes",
        ["<", ">", "U<"],
        "{min} {max} < {max} {min} > -1 {max} U<",
        [-1, -1, 0]
    );
    cases.last_mut().unwrap().profile = Some("iso");
    cases
}
