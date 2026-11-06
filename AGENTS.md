# AGENTS.md

Instructions for coding agents working on this project.

## Project

A clean-room Forth virtual machine in Rust: indirect threaded, with a linked-list dictionary and no dependencies.

The FORTH-83 standard is the reference for how words behave. Later ANS Forth words are fine where they're useful.

See `README.md` for the word list and internals, and the GitHub issues for known problems. Don't copy either into this file, because they change.

## Commands

CI runs these same `Makefile` targets:

```sh
make ci        # Formatting, lints, build, debug and release tests, and docs
make ci-all    # Also Miri, on 64-bit and 32-bit targets. Needs nightly
make msrv      # Checks the oldest supported Rust version, from `rust-version`
make mutants   # Mutation testing, which CI runs weekly. Needs cargo-mutants
make demo      # Runs the demo
make doc-serve # Serves the docs on localhost, including private items. Needs python3
```

`make ci` must pass before a PR. Each step is also its own target, such as `make test`. Add new CI steps to the `Makefile`, not directly to the workflows.

CI uses the latest stable toolchain on purpose, so a new lint can fail it with no code change. Raise `rust-version` when code needs a newer feature, and check it with a run on GitHub.

## Architecture

Modules are layered, and the layering must stay free of cycles:

```
main → lib → interpreter, primitives → vm → compiler → dictionary → error, cell
                                       vm → input
```

- Header offsets stay private to `dictionary.rs`. Other modules use its accessors.
- `Dictionary` and `Compiler` can't see `Registers`, so they return `Result<_, ForthError>`. `Registers::check` turns that into a VM error.
- Words go in `src/primitives/`:
  - `core.rs` for data and memory words.
  - `compiler.rs` for words that build the dictionary.
  - `runtime.rs` for words that drive threaded code.
  - `parsing.rs` for words that read the input.
- Primitives return to the `run` loop, and never call `next`.
- The library doesn't print. Output goes through the writer given to `boot`.

## Conventions

- Use `Cell` and `size_of::<Cell>()` for anything Cell-sized, never 64-bit types or hard-coded sizes. This includes tests.
- Errors are `ForthError` variants, not strings. Their messages are pinned in `errors_have_stable_messages`.
- Bad Forth input should give a `ForthError`, not a panic or a crash. Known exceptions need a GitHub issue.
- New source files start with the copyright header `(C) Copyright 2026 M.W.R. Hoiting`, as a comment, followed by a blank line. Config and Markdown files don't need it.
- New `unsafe` code needs a comment saying why it holds.
- Every item, public or private, has a doc comment. `make clippy` enforces this, except in test code.
- Doc comments on `unsafe fn` have a `# Safety` section when the caller has something to uphold. The contract shared by primitives is on `PrimitiveFn`.
- Doc comments on primitives start with the word and its stack effect in one code span, such as `` `@ ( addr -- x )`: ``. Outside a code span, rustdoc would change `--` and `*`.
- Public API changes update the doc examples, which run as doctests.
- Docs describe behaviour, and link to the README for the word list and limitations instead of copying them.
- Breaking changes are fine until the project owner says otherwise.
- Stay within the requested scope. Report anything else you find instead of fixing it in passing.

## New Forth words

Every new word or feature includes:

- A doc comment on each primitive.
- Unit tests next to the code, comparing exact state.
- API tests in `tests/forth.rs`, using only the public API.
- Normal use cases in the demo, as a commented block in `SECTIONS` in `src/main.rs`.
- README updates: the word table, and the limitations if they change.

## Working on changes

1. When planning new work, first check the open issues with `gh issue list --state open`. Show them in a table with columns for the issue number, type, description, and how it relates to the planned work. Say which issues the work solves, depends on, or changes.
2. Plan first for anything beyond a small fix, and agree on it before writing code.
3. Create or use a GitHub issue, stating the problem and the acceptance criteria.
4. Work on a branch, never directly on `main`.
5. Run `make ci`, and compare the `make demo` output with `main`. Only the intended changes may appear.
6. For behaviour changes, temporarily break the code to confirm the tests catch it. `make mutants` skips every `unsafe fn`, which includes all primitives, so it doesn't replace this step. Use `--no-fail-fast` and check the exit code, because a crash in a test binary prints no `FAILED` lines.
7. Open a PR that closes the issue, then stop. The project owner reviews and merges.
8. After the merge, sync `main`, and delete the branch once asked. The PR head can differ from the last local commit, because of review suggestions.

## Accuracy

- Reproduce a bug or test a claim before reporting it, and say which claims come only from reading the code.
- GitHub closing keywords work inside any sentence, so "doesn't fix #15" closes the issue. When a PR only mentions an issue, write "see #15" or "related to #15".
- Ask before pushing, merging, or creating issues, unless the request already covers it. A question isn't an instruction to change anything.
