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
- Defining words: `:`, `;`, `:NONAME`, `CREATE`, `CONSTANT`, `DOES>`, `IMMEDIATE` and
  `EXIT`.
- `EXECUTE`, which runs a word from its execution token.
- Comments with `(` and `\`.
- A `forth-vm` binary that runs a scripted demo.
- The MIT licence, and a minimum supported Rust version.
- This changelog, a contributing guide, a versioning policy, and getting-started steps in
  the README.
- Git hooks and a GitHub ruleset that protect `main`.
- `make setup-dev`, which installs the developer tools and the git hooks.

[Unreleased]: https://github.com/michahoiting/rust-forth-vm/commits/main
