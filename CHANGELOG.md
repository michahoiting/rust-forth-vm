# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- An indirect-threaded Forth VM with a linked-list dictionary, a text interpreter, and a
  compiler for colon definitions.
- A library API: `boot` creates a VM with a chosen dictionary size and an output writer,
  and `outer_interpreter` evaluates Forth source in it. Errors are `ForthError` values.
- Words for arithmetic and output: `+` and `.`.
- Memory words: `@`, `!`, `,` and `VARIABLE`. `@` and `!` are limited to dictionary
  addresses below `HERE`.
- The FORTH-83 data stack words: `DUP`, `?DUP`, `DROP`, `SWAP`, `OVER`, `ROT`, `PICK`,
  `ROLL` and `DEPTH`.
- Defining words: `:`, `;`, `:NONAME`, `CREATE`, `DOES>`, `IMMEDIATE` and `EXIT`.
- `EXECUTE`, which runs a word from its execution token.
- `'` and `[']`, which give the execution token of a named word.
- `>BODY`, which gives the address of a word's Parameter Field from its execution token.
- Comments with `(` and `\`.
- A `forth-vm` binary that runs a scripted demo.
- The MIT licence, and a minimum supported Rust version.
- This changelog, a contributing guide, a versioning policy, and getting-started steps in
  the README.
- Git hooks and a GitHub ruleset that protect `main`.
- `make setup-dev`, which installs the developer tools and the git hooks.

### Changed

- **Breaking:** Word headers shrink from 5 Cells to 4. The name length and flags share one
  Cell, and `DOES>` no longer needs an action cell, so the data of a `CREATE`d word starts
  directly after its Code Field. Forth code that computes addresses inside a word changes.
- **Breaking:** Word names can be at most 255 bytes long. Longer names give the new
  `ForthError::NameTooLong`.
- **Breaking:** `ForthError::NotCompiling` now holds the name of the word.

### Fixed

- A failed `CREATE` or `VARIABLE` no longer leaves a partial word behind, which `DOES>` could
  then use to write outside the dictionary.

[Unreleased]: https://github.com/michahoiting/rust-forth-vm/commits/main
