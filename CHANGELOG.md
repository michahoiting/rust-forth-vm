# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and after the first release, this project adheres to
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

The `[Unreleased]` section below is the unreleased `0.1.0`.

## [Unreleased]

### Added

- Single-cell arithmetic, comparison, logic and memory words, including floored division,
  double-cell stack/memory operations, and overlap-safe `MOVE`.
- An advisory incremental mutation check on pull requests, with elapsed time and results.
- Performance measurement: `make perf` runs Forth workloads for each part of the VM, counts
  their instructions, and stores the results in a SQLite database under `target/perf`.
  `make perf-report` shows the development, and `make perf-compare` compares with another commit.
- A performance report on each PR: the `perf` job counts the instructions of the base and the
  head with Cachegrind, and shows the change per workload in the job summary, with the
  functions behind each change from 2%. It doesn't fail the job yet.
- An indirect-threaded Forth VM with a linked-list dictionary, a text interpreter, and a
  compiler for colon definitions.
- A library API: `boot` creates a VM from a `VmConfig` and an output writer, and
  `outer_interpreter` evaluates Forth source in it. Errors are `ForthError` values.
- Stack limits: `VmConfig` sets the dictionary size and the limit of each stack, which
  defaults to 1024 Cells. Going past a limit gives `ForthError::DataStackOverflow` or
  `ForthError::ReturnStackOverflow` instead of using up memory, and the VM stays usable.
- Words for arithmetic and output: `+` and `.`.
- Memory words: `@`, `!`, `,` and `VARIABLE`. `@` and `!` are limited to dictionary
  addresses below `HERE`.
- Byte access to memory: `C@`, `C!`, `C,`, `HERE`, `ALLOT`, `ALIGN`, `ALIGNED`, `CHAR+`,
  `CHARS` and `CELLS`. `,` and defining words align `HERE` first, because `C,` and `ALLOT` can leave it
  unaligned. `C,` and `ALLOT` give an error while a definition is open, and a negative
  `ALLOT` can only give back data of the newest word made by `CREATE`.
- Specific memory errors: `@`, `!`, `C@` and `C!` give `ForthError::AddressOutOfBounds` for an
  address outside the dictionary, and `ForthError::UnwrittenAddress` for one at or above
  `HERE`, including a Cell that ends above it. `@` and `!` give
  `ForthError::MisalignedAddress` for a Cell that isn't cell-aligned. The checks run in that
  order. A word with a zero code field gives `ForthError::NullCodeField`.
- The FORTH-83 data stack words: `DUP`, `?DUP`, `DROP`, `SWAP`, `OVER`, `ROT`, `PICK`,
  `ROLL` and `DEPTH`.
- Defining words: `:`, `;`, `:NONAME`, `CREATE`, `CONSTANT`, `DOES>`, `IMMEDIATE` and
  `EXIT`.
- `EXECUTE`, which runs a word from its execution token.
  It and `>BODY` refuse `LIT`, which can't run outside a compiled thread, with
  `ForthError::InvalidExecutionToken`. The flag `F_COMPILE_ONLY` marks it, and every future
  word with an inline operand.
- `QUIT`, which clears the return stack, discards the rest of the input, and returns to
  interpreting. It's reported as `ForthError::Quit`, and keeps the data stack.
- `ABORT`, which clears the data stack and does the same as `QUIT`. It's reported as
  `ForthError::Abort`.
- `BYE` and `(BYE)`, which stop evaluation and let the program leave with an exit code.
- `'` and `[']`, which give the execution token of a named word.
- `COMPILE,`, which compiles a call to the word with a given execution token into the open
  definition, so that an immediate word can compile code. While a definition is open, `,`,
  `CREATE`, `VARIABLE`, `CONSTANT`, `:` and `:NONAME` give `ForthError::DefinitionOpen`,
  like `C,` and `ALLOT` do, because they would write into the definition and could make it
  run an arbitrary Cell.
- `>BODY`, which gives the address of a word's Parameter Field from its execution token.
- Word names of up to 255 bytes.
- Comments with `(` and `\`.
- A `forth-vm` binary that starts as an interactive Forth prompt. It prints `ok` after a
  line that succeeds, and `Vm::is_compiling` tells it when a definition is open.
- Line editing at the interactive prompt on a terminal, with a history that's kept in
  `~/.rustforth_history`. It uses `rustyline`, behind the default `readline` feature, so
  the library needs no dependencies with `default-features = false`.
- Tab completion of the words in the dictionary at the interactive prompt, and
  `Vm::word_names`, which lists them.
- A `--demo` option with a menu to run all the demo sections or one of them, each in a new
  VM. `make demo` opens this menu.
- A `--version` option, which prints the version, the build date, and the git tag and
  hash. `build.rs` provides the build information. The interactive prompt starts with
  the same line, the copyright notice, and the words that give more information.
- `HELP`, `COPYRIGHT` and `LICENSE`, which print how to leave the program, the copyright
  notice, and the license. The library exports the notice as `COPYRIGHT`.
- The MIT license, and a minimum supported Rust version.
- This changelog, a contributing guide, a versioning policy, and getting-started steps in
  the README.
- Git hooks and a GitHub ruleset that protect `main`.
- `make setup-dev`, which installs the developer tools and the git hooks.
- Faster CI: the two Miri targets run as separate jobs, PRs that only change Markdown skip
  Miri, and new pushes to a PR cancel its older runs.
- `make metrics`, which reports PR lead time, time to first review, CI reruns, and issues
  completed per week.
- Tests of the demo's full output and the README's Forth example, and `make demo-expected`,
  which updates the expected output.
- A guide in the README to working on this project with the Claude CLI, and `.gitignore`
  rules for Claude's worktrees and local settings.

[Unreleased]: https://github.com/michahoiting/rust-forth-vm/commits/main
