# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and after the first release, this project adheres to
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

The `[Unreleased]` section below is the unreleased `0.1.0`.

## [Unreleased]

### Added

- An indirect-threaded Forth VM with a linked-list dictionary, a text interpreter, and a
  compiler for colon definitions.
- A library API: `boot` creates a VM with a chosen dictionary size and an output writer,
  and `outer_interpreter` evaluates Forth source in it. Errors are `ForthError` values.
- Words for arithmetic and output: `+` and `.`.
- Memory words: `@`, `!`, `,` and `VARIABLE`. `@` and `!` are limited to dictionary
  addresses below `HERE`.
- Byte access to memory: `C@`, `C!`, `C,`, `HERE`, `ALLOT`, `ALIGN`, `ALIGNED`, `CHAR+`,
  `CHARS` and `CELLS`. `,` and defining words align `HERE` first, because `C,` and `ALLOT` can leave it
  unaligned. `C,` and `ALLOT` give an error while a definition is open, and a negative
  `ALLOT` can only give back data of the newest word made by `CREATE`.
- The FORTH-83 data stack words: `DUP`, `?DUP`, `DROP`, `SWAP`, `OVER`, `ROT`, `PICK`,
  `ROLL` and `DEPTH`.
- Defining words: `:`, `;`, `:NONAME`, `CREATE`, `CONSTANT`, `DOES>`, `IMMEDIATE` and
  `EXIT`.
- `EXECUTE`, which runs a word from its execution token.
- `QUIT`, which clears the return stack, discards the rest of the input, and returns to
  interpreting. It's reported as `ForthError::Quit`, and keeps the data stack.
- `ABORT`, which clears the data stack and does the same as `QUIT`. It's reported as
  `ForthError::Abort`.
- `BYE` and `(BYE)`, which stop evaluation and let the program leave with an exit code.
- `'` and `[']`, which give the execution token of a named word.
- `>BODY`, which gives the address of a word's Parameter Field from its execution token.
- Word names of up to 255 bytes.
- Comments with `(` and `\`.
- A `forth-vm` binary that starts as an interactive Forth prompt. It prints `ok` after a
  line that succeeds, and `Registers::is_compiling` tells it when a definition is open.
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
