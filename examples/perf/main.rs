// Copyright (c) 2026 M.W.R. Hoiting

//! Forth workloads for measuring the VM, which `scripts/perf.py` runs one process at a time.
//!
//! `perf <workload>` runs one workload and checks its result, and `perf --list` prints the
//! workloads as JSON. `--pad-words <n>` first defines `n` extra words, which shows whether a
//! workload depends on the size of the dictionary.
//!
//! Each file in `workloads/` holds the setup, and can have a line `\ MEASURE` followed by the
//! part to measure. A workload without one defines `LEAF`, and runs it through the tower of
//! words in `tower.fth`. The measured part only has to be long enough for a stable
//! count, so don't make it larger than the sizes here.

use std::cell::RefCell;
use std::env;
use std::fmt;
use std::io::{self, Write};
use std::process::ExitCode;
use std::rc::Rc;

use rust_forth_vm::{boot, outer_interpreter, ForthError, Vm, VmConfig};

/// The size of the dictionary, which is enough for the longest compiling workload.
const DICTIONARY_CELLS: usize = 1 << 20;

/// The size of the dictionary in the `boot` workload, which is what `forth-vm` uses.
const BOOT_DICTIONARY_CELLS: usize = 64 * 1024;

/// The tower that each workload without a measured part calls `LEAF` from.
const TOWER: &str = include_str!("workloads/tower.fth");

/// The line that separates the setup of a workload from the part to measure.
const MEASURE: &str = "\\ MEASURE\n";

/// How much work a workload does.
#[derive(Clone, Copy)]
enum Size {
    /// Runs `LEAF` 2^n times, through the word `Tn` of the tower.
    Tower(u32),
    /// Evaluates the measured part this many times.
    Lines(usize),
    /// Boots this many VMs.
    Boots(usize),
}

impl Size {
    /// Returns this size, divided by 2^`shift`, so that a test can run it quickly.
    #[cfg(all(test, not(miri)))]
    fn reduced(self, shift: u32) -> Size {
        match self {
            Size::Tower(depth) => Size::Tower(depth.saturating_sub(shift)),
            Size::Lines(lines) => Size::Lines(lines >> shift),
            Size::Boots(boots) => Size::Boots(boots >> shift),
        }
    }

    /// Returns how many times the repeated part of the workload runs.
    fn runs(self) -> usize {
        match self {
            Size::Tower(depth) => 1 << depth,
            Size::Lines(count) | Size::Boots(count) => count,
        }
    }
}

/// One thing to measure.
struct Workload {
    /// The name, which is also the name of its file in `workloads/`.
    name: &'static str,
    /// The part of the VM that it measures.
    category: &'static str,
    /// Whether a slowdown can fail the check. A workload that depends on the size of the
    /// dictionary must not be gated, because each new word would slow it down.
    gated: bool,
    /// The Forth source.
    source: &'static str,
    /// How much work it does.
    size: Size,
    /// Forth that runs after the measured part and prints the result.
    check: &'static str,
    /// What `check` prints, from the number of runs.
    expected: fn(usize) -> String,
    /// How many bytes one run of the measured part prints.
    bytes_per_run: usize,
}

/// A workload that uses the tower, and prints nothing while it runs.
const fn tower_workload(
    name: &'static str,
    category: &'static str,
    gated: bool,
    source: &'static str,
    depth: u32,
    check: &'static str,
    expected: fn(usize) -> String,
) -> Workload {
    Workload {
        name,
        category,
        gated,
        source,
        size: Size::Tower(depth),
        check,
        expected,
        bytes_per_run: 0,
    }
}

/// A workload that evaluates its measured part `lines` times, and prints nothing while it runs.
const fn line_workload(
    name: &'static str,
    category: &'static str,
    gated: bool,
    source: &'static str,
    lines: usize,
    check: &'static str,
    expected: fn(usize) -> String,
) -> Workload {
    Workload {
        name,
        category,
        gated,
        source,
        size: Size::Lines(lines),
        check,
        expected,
        bytes_per_run: 0,
    }
}

/// Prints a stack that is expected to be empty.
const EMPTY_STACK: &str = "DEPTH .";

/// Returns what [`EMPTY_STACK`] prints.
fn prints_zero(_runs: usize) -> String {
    "0 ".to_string()
}

/// Returns what printing the number of runs gives.
fn prints_runs(runs: usize) -> String {
    format!("{runs} ")
}

/// Every workload. `scripts/perf.py` reads this through `--list`.
const WORKLOADS: [Workload; 14] = [
    tower_workload(
        "mmio_block",
        "MMIO block transfer",
        true,
        include_str!("workloads/mmio_block.fth"),
        14,
        "BUFFER C@ . DEPTH .",
        |_| "90 0 ".to_string(),
    ),
    Workload {
        name: "boot",
        category: "Startup",
        gated: true,
        source: include_str!("workloads/boot.fth"),
        size: Size::Boots(300),
        check: "1 2 + .",
        expected: |_| "3 ".to_string(),
        bytes_per_run: 0,
    },
    tower_workload(
        "threading",
        "Inner interpreter",
        true,
        include_str!("workloads/threading.fth"),
        20,
        EMPTY_STACK,
        prints_zero,
    ),
    tower_workload(
        "stack",
        "Stack",
        true,
        include_str!("workloads/stack.fth"),
        18,
        "DEPTH . . .",
        |runs| format!("2 {runs} 0 "),
    ),
    tower_workload(
        "arith_memory",
        "Memory",
        true,
        include_str!("workloads/arith_memory.fth"),
        18,
        "V @ .",
        prints_runs,
    ),
    tower_workload(
        "control_flow",
        "Control flow",
        true,
        include_str!("workloads/control_flow.fth"),
        18,
        "V @ .",
        prints_runs,
    ),
    tower_workload(
        "dispatch",
        "Defining words",
        true,
        include_str!("workloads/dispatch.fth"),
        18,
        "V @ .",
        |runs| format!("{} ", runs * 10),
    ),
    tower_workload(
        "execute_near",
        "Execution tokens",
        true,
        include_str!("workloads/execute_near.fth"),
        18,
        "V @ .",
        prints_runs,
    ),
    Workload {
        bytes_per_run: 8,
        ..tower_workload(
            "output",
            "Output",
            true,
            include_str!("workloads/output.fth"),
            18,
            EMPTY_STACK,
            prints_zero,
        )
    },
    line_workload(
        "lookup_near",
        "Text interpreter",
        true,
        include_str!("workloads/lookup_near.fth"),
        1 << 15,
        "N @ .",
        prints_runs,
    ),
    line_workload(
        "compile_near",
        "Compiler",
        true,
        include_str!("workloads/compile_near.fth"),
        1 << 15,
        "; DEPTH .",
        prints_zero,
    ),
    tower_workload(
        "execute_far",
        "Dictionary size",
        false,
        include_str!("workloads/execute_far.fth"),
        18,
        "V @ .",
        prints_runs,
    ),
    line_workload(
        "interpret_far",
        "Dictionary size",
        false,
        include_str!("workloads/interpret_far.fth"),
        1 << 14,
        "N @ .",
        prints_runs,
    ),
    line_workload(
        "compile_far",
        "Dictionary size",
        false,
        include_str!("workloads/compile_far.fth"),
        1 << 10,
        "W .",
        |_| "3 ".to_string(),
    ),
];

/// What a VM wrote. It only counts while a workload runs, so that a long run doesn't fill the
/// memory, and keeps the text of the check.
#[derive(Default)]
struct Written {
    /// How many bytes were written.
    bytes: usize,
    /// Whether to keep the text.
    keep: bool,
    /// The text since `keep` was set.
    text: Vec<u8>,
}

/// A writer that can be read after the VM took it.
#[derive(Clone, Default)]
struct Output(Rc<RefCell<Written>>);

impl Write for Output {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let mut written = self.0.borrow_mut();
        written.bytes += bytes.len();
        if written.keep {
            written.text.extend_from_slice(bytes);
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Why a workload failed.
#[derive(Debug)]
enum Failure {
    /// The VM reported an error.
    Forth(ForthError),
    /// The result isn't what the workload should give.
    WrongResult(String),
}

impl fmt::Display for Failure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Failure::Forth(error) => write!(formatter, "{error}"),
            Failure::WrongResult(message) => write!(formatter, "{message}"),
        }
    }
}

impl From<ForthError> for Failure {
    fn from(error: ForthError) -> Failure {
        Failure::Forth(error)
    }
}

/// Evaluates the setup and the measured part of `workload`.
///
/// # Safety
///
/// `vm` must come from [`boot`], and is only used through the crate's functions.
unsafe fn measure(vm: &mut Vm, workload: &Workload, size: Size) -> Result<(), ForthError> {
    let (setup, lines) = workload
        .source
        .split_once(MEASURE)
        .unwrap_or((workload.source, ""));
    outer_interpreter(vm, setup)?;
    match size {
        Size::Tower(depth) => {
            outer_interpreter(vm, TOWER)?;
            outer_interpreter(vm, &format!("T{depth}"))?;
        }
        Size::Lines(count) => {
            for _ in 0..count {
                outer_interpreter(vm, lines)?;
            }
        }
        Size::Boots(_) => {}
    }
    Ok(())
}

/// Runs `workload` at `size`, with `pad_words` extra words, and checks its result.
///
/// # Errors
///
/// Returns a [`Failure`] if the VM reports an error, or if the result is wrong.
fn run(workload: &Workload, size: Size, pad_words: usize) -> Result<(), Failure> {
    let output = Output::default();
    let dictionary_cells = match size {
        Size::Boots(_) => BOOT_DICTIONARY_CELLS,
        _ => DICTIONARY_CELLS,
    };

    // SAFETY: these VMs are only used through the crate's functions.
    unsafe {
        if let Size::Boots(count) = size {
            for _ in 1..count {
                boot(io::sink(), VmConfig::new(dictionary_cells))?;
            }
        }
        let mut registers = Box::new([0usize; 128]);
        let mut vm = boot(output.clone(), VmConfig::new(dictionary_cells))?;
        if workload.name == "mmio_block" {
            // SAFETY: the buffer outlives vm and is exclusively used by MMIO while it runs.
            let address = registers.as_mut_ptr().cast::<u8>();
            vm.allow_mmio(rust_forth_vm::MmioRegion {
                address,
                bytes: std::mem::size_of_val(&*registers),
                rights: rust_forth_vm::MMIO_READ | rust_forth_vm::MMIO_WRITE,
                widths: rust_forth_vm::MMIO_WIDTH_8,
            })?;
            outer_interpreter(&mut vm, &format!("{} CONSTANT REGISTERS", address as usize))?;
        }
        for _ in 0..pad_words {
            outer_interpreter(&mut vm, ": PAD ;")?;
        }
        measure(&mut vm, workload, size)?;

        let bytes = output.0.borrow().bytes;
        let expected_bytes = workload.bytes_per_run * size.runs();
        if bytes != expected_bytes {
            return Err(Failure::WrongResult(format!(
                "printed {bytes} bytes instead of {expected_bytes}"
            )));
        }

        output.0.borrow_mut().keep = true;
        outer_interpreter(&mut vm, workload.check)?;
    }

    let text = String::from_utf8_lossy(&output.0.borrow().text).into_owned();
    let expected = (workload.expected)(size.runs());
    if text == expected {
        Ok(())
    } else {
        Err(Failure::WrongResult(format!(
            "printed {text:?} instead of {expected:?}"
        )))
    }
}

/// Returns the workloads as JSON. Every string is a plain name, so none needs escaping.
fn list() -> String {
    let rows: Vec<String> = WORKLOADS
        .iter()
        .map(|workload| {
            format!(
                r#"{{"name":"{}","category":"{}","gated":{}}}"#,
                workload.name, workload.category, workload.gated
            )
        })
        .collect();
    format!("[{}]", rows.join(","))
}

/// Runs what the command line asks for.
///
/// # Errors
///
/// Returns a message for arguments that it doesn't know, and for a failed workload.
fn run_command(args: &[String]) -> Result<(), String> {
    let usage = "Usage: perf --list | perf <workload> [--pad-words <n>]";
    match args {
        [flag] if flag == "--list" => {
            println!("{}", list());
            Ok(())
        }
        [name] | [name, _, _] => {
            let pad_words = match &args[1..] {
                [] => 0,
                [flag, count] if flag == "--pad-words" => count.parse().map_err(|_| usage)?,
                _ => return Err(usage.to_string()),
            };
            let workload = WORKLOADS
                .iter()
                .find(|workload| workload.name == name)
                .ok_or_else(|| format!("Unknown workload: {name}"))?;
            run(workload, workload.size, pad_words).map_err(|failure| format!("{name}: {failure}"))
        }
        _ => Err(usage.to_string()),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match run_command(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

// These only test the workloads, and each one boots a large dictionary, which took Miri three
// minutes. The library's own tests cover the VM under Miri.
#[cfg(all(test, not(miri)))]
mod tests {
    use super::*;

    #[test]
    fn each_workload_gives_its_result() {
        for workload in &WORKLOADS {
            let size = workload.size.reduced(6);
            let result = run(workload, size, 0);
            assert!(result.is_ok(), "{}: {result:?}", workload.name);
        }
    }

    #[test]
    fn extra_words_change_no_result() {
        for workload in &WORKLOADS {
            let size = workload.size.reduced(6);
            let result = run(workload, size, 5);
            assert!(result.is_ok(), "{}: {result:?}", workload.name);
        }
    }

    #[test]
    fn a_wrong_result_is_a_failure() {
        let workload = Workload {
            expected: |_| "wrong".to_string(),
            ..tower_workload(
                "threading",
                "Inner interpreter",
                true,
                WORKLOADS[2].source,
                1,
                EMPTY_STACK,
                prints_zero,
            )
        };
        let result = run(&workload, workload.size, 0);
        assert!(matches!(result, Err(Failure::WrongResult(_))), "{result:?}");
    }

    #[test]
    fn a_forth_error_is_a_failure() {
        let workload = line_workload(
            "broken",
            "None",
            true,
            "\\ MEASURE\nNO-SUCH-WORD\n",
            1,
            "",
            prints_zero,
        );
        let result = run(&workload, workload.size, 0);
        assert!(matches!(result, Err(Failure::Forth(_))), "{result:?}");
    }

    #[test]
    fn the_list_names_every_workload_in_order() {
        let listed = list();
        let mut rest = listed.as_str();
        for workload in &WORKLOADS {
            let name = format!(r#""name":"{}""#, workload.name);
            let found = rest.find(&name);
            assert!(
                found.is_some(),
                "{} is missing or out of order",
                workload.name
            );
            rest = &rest[found.unwrap()..];
        }
    }

    #[test]
    fn workload_names_are_unique() {
        let mut names: Vec<&str> = WORKLOADS.iter().map(|workload| workload.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), WORKLOADS.len());
    }
}
