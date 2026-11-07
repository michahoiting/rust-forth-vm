# rust-forth-vm

[![CI](https://github.com/michahoiting/rust-forth-vm/actions/workflows/ci.yml/badge.svg?branch=main&event=push)](https://github.com/michahoiting/rust-forth-vm/actions/workflows/ci.yml?query=branch%3Amain+event%3Apush)
[![Lint](https://github.com/michahoiting/rust-forth-vm/actions/workflows/lint.yml/badge.svg?branch=main&event=push)](https://github.com/michahoiting/rust-forth-vm/actions/workflows/lint.yml?query=branch%3Amain+event%3Apush)
[![Mutation testing](https://github.com/michahoiting/rust-forth-vm/actions/workflows/mutants.yml/badge.svg?branch=main)](https://github.com/michahoiting/rust-forth-vm/actions/workflows/mutants.yml?query=branch%3Amain)

A clean-room implementation of a Forth virtual machine in Rust. It is an
indirect-threaded VM with a linked-list dictionary, a text interpreter, and a
compiler for colon definitions.

The program is an interactive Forth prompt, and also has a scripted demo. There is
no file input yet.

## Getting started

You need `git`, `make`, and Rust with the `rustfmt` and `clippy` components. The
oldest supported Rust version is the `rust-version` in [Cargo.toml](Cargo.toml).
If you don't have Rust yet, install it with [rustup](https://rustup.rs), which
includes both components. Serving the docs with `make doc-serve` also needs
`python3`.

Get the code and run the demo:

```sh
git clone https://github.com/michahoiting/rust-forth-vm.git
cd rust-forth-vm
make demo
```

The demo shows a menu of its sections. Enter a number to run one section, press
Enter to run them all, or enter `Q` to quit. Each section runs in a new VM.

Start the interactive prompt with:

```sh
cargo run --locked
```

It evaluates each line you enter, in one VM, and prints `ok` after a line that
succeeds, unless a definition is still open. Press Ctrl-D to leave. `forth-vm`
starts this way by default, and also accepts `--interactive`, `--demo` for the demo,
and `--version`.

`--version` prints the version, the UTC date of the build, and the git tag and hash. The
prompt starts with the same line, followed by the copyright notice and the words that give
more information:

```text
Rust Forth VM 0.1.0 (Built 2026-11-06, Git v0.1.0 5d28858)
Copyright (c) 2026 M.W.R. Hoiting.
All Rights Reserved.
Type "HELP", "COPYRIGHT" or "LICENCE" for more information.
```

`(dirty)` follows the hash unless the build is a clean release, which means `HEAD` has a
tag of its own and the working tree has no changes. So unlike the usual meaning of
"dirty", any commit without a tag shows it, even with a clean tree, and its tag is shown
as `none`. A build with no git, or outside a repository, shows `unknown` for the tag and
the hash. These only update when a file that git knows about or git's own state changes,
so a new untracked file on its own doesn't show up until the next such change.

These commands are useful from there:

```sh
make test       # Run the unit and API tests
make ci         # Run all the checks that work on the stable toolchain
make doc-serve  # Serve the docs at http://localhost:8000/rust_forth_vm/
```

`cargo build --release` builds the program as `target/release/forth-vm`.

To use the VM as a library, add it as a git dependency. It isn't published on
crates.io. The [API docs](src/lib.rs) start with a complete example.

```toml
[dependencies]
rust-forth-vm = { git = "https://github.com/michahoiting/rust-forth-vm" }
```

To work on the project, run `make setup-dev`, which installs all the developer
tools, and see [CONTRIBUTING.md](CONTRIBUTING.md). Notable changes are listed in
[CHANGELOG.md](CHANGELOG.md).

## Example

The demo evaluates lines of Forth like these:

```forth
40 2 + .                          \ 42
-5 3 + .                          \ -2

CREATE STATIONARY-SOURCE 1337 ,
STATIONARY-SOURCE @ .             \ 1337
' STATIONARY-SOURCE >BODY @ .     \ 1337

: ADD3 3 + ;
: ADD6 ADD3 ADD3 ;
36 ADD6 .                         \ 42

42 CONSTANT ANSWER
ANSWER .                          \ 42

: ADDER CREATE , DOES> @ + ;
3 ADDER PLUS3
1 PLUS3 .                         \ 4

VARIABLE COUNTER
42 COUNTER !
COUNTER @ .                       \ 42
COUNTER @ 1 + COUNTER !
COUNTER @ .                       \ 43

: DOUBLE DUP + ;
21 DOUBLE .                       \ 42

: +! SWAP OVER @ + SWAP ! ;
5 COUNTER +!
COUNTER @ .                       \ 48

:NONAME 1 2 + ; EXECUTE .         \ 3

: ANNOUNCE 99 . ; IMMEDIATE
: USES-ANNOUNCE ANNOUNCE 1 ;      \ 99, printed while compiling
USES-ANNOUNCE .                   \ 1

: ADD5 ( n -- n+5 ) 5 + ;         \ a stack effect comment
2 ADD5 .                          \ 7
```

Everything after `\` on a line is a comment, and so is anything between `(` and
`)`. Both need whitespace after them, so `(comment)` is not a comment. Source can
span several lines, and is evaluated as one block: an error discards the rest of
it.

## Words

| Word     | Stack effect | Description                                       |
|----------|--------------|---------------------------------------------------|
| `+`      | `( a b -- n )` | Adds two Cells, wrapping on overflow            |
| `.`      | `( n -- )`   | Prints a Cell as a signed number                  |
| `@`      | `( addr -- x )` | Fetches the Cell at a dictionary address below `HERE` |
| `!`      | `( x addr -- )` | Stores a Cell at a dictionary address below `HERE` |
| `,`      | `( x -- )`   | Aligns `HERE`, stores a Cell there and advances it |
| `C@`     | `( c-addr -- char )` | Fetches the byte at a dictionary address below `HERE` |
| `C!`     | `( char c-addr -- )` | Stores the low byte of a Cell at a dictionary address below `HERE` |
| `C,`     | `( char -- )` | Stores the low byte of a Cell at `HERE` and advances it, which can leave `HERE` unaligned |
| `HERE`   | `( -- addr )` | Pushes the address of the next free byte in the dictionary |
| `ALLOT`  | `( n -- )`   | Reserves and clears `n` bytes at `HERE`. A negative `n` gives back data of the newest word, if `CREATE` made it |
| `ALIGN`  | `( -- )`     | Moves `HERE` to the next cell-aligned address     |
| `ALIGNED` | `( addr -- a-addr )` | Rounds an address up to a cell-aligned address |
| `CHAR+`  | `( c-addr1 -- c-addr2 )` | Adds the size of a character to an address |
| `CHARS`  | `( n1 -- n2 )` | Gives the size of `n1` characters in bytes. A character is one byte |
| `CELLS`  | `( n1 -- n2 )` | Gives the size of `n1` Cells in bytes             |
| `CREATE` | `( -- )`     | Parses a name and creates a word that pushes its data address |
| `VARIABLE` | `( -- )`   | Parses a name and creates a word holding one Cell, initially 0 |
| `CONSTANT` | `( x -- )` | Parses a name and creates a word that pushes `x`  |
| `DOES>`  | `( -- )`     | Inside a definition: gives the newest `CREATE`d word the code that follows |
| `:`      | `( -- )`     | Parses a name and starts compiling a definition   |
| `;`      | `( -- )`     | Ends the definition (immediate)                   |
| `IMMEDIATE` | `( -- )`  | Marks the newest word, so that it runs during compilation |
| `:NONAME` | `( -- xt )` | Starts a definition without a name, and pushes its execution token |
| `EXECUTE` | `( i*x xt -- j*x )` | Runs the word that an execution token refers to |
| `'`      | `( -- xt )`  | Parses a name and pushes the execution token of that word |
| `[']`    | `( -- )`     | Inside a definition: parses a name, and compiles its execution token as a literal (immediate) |
| `>BODY`  | `( xt -- addr )` | Replaces an execution token with the address of that word's Parameter Field |
| `EXIT`   | `( -- )`     | Returns from the current definition               |
| `QUIT`   | `( -- )`     | Clears the return stack, discards the rest of the input, and returns to interpreting. Keeps the data stack, and doesn't leave the program |
| `ABORT`  | `( i*x -- )` | Clears the data stack, then does the same as `QUIT` |
| `BYE`    | `( -- )`     | Stops the program with exit code 0                          |
| `(BYE)`  | `( n -- )`    | Stops the program with exit code `n`'s low byte             |
| `(`      | `( -- )`     | Skips the input up to and including the next `)` (immediate) |
| `\`      | `( -- )`     | Skips the rest of the line (immediate)            |
| `DUP`    | `( x -- x x )` | Copies the top Cell                             |
| `?DUP`   | `( x -- 0 \| x x )` | Copies the top Cell if it isn't zero       |
| `DROP`   | `( x -- )`   | Removes the top Cell                              |
| `SWAP`   | `( a b -- b a )` | Exchanges the top two Cells                   |
| `OVER`   | `( a b -- a b a )` | Copies the second Cell to the top           |
| `ROT`    | `( a b c -- b c a )` | Moves the third Cell to the top           |
| `PICK`   | `( ... n -- ... x )` | Copies the `n`th Cell below `n`, counting from 0 |
| `ROLL`   | `( ... n -- ... x )` | Moves the `n`th Cell below `n` to the top, counting from 0 |
| `DEPTH`  | `( -- n )`   | Pushes the number of Cells on the stack           |
| `HELP`   | `( -- )`     | Prints how to leave the program                   |
| `COPYRIGHT` | `( -- )`  | Prints the copyright notice                       |
| `LICENCE` | `( -- )`    | Prints the project's licence                      |

Word names are case-sensitive, and can be up to 255 bytes long. Numbers are decimal,
and may be negative.

## How it works

Every dictionary entry has the same header. The name bytes are stored in the
dictionary directly in front of it.

```
LFA        Link Field Address (LFA) Link to the previous word
NFA        Name Field Address (NFA) Pointer to the name bytes
LEN_FLAGS  Length of the name, and F_IMMEDIATE, F_HIDDEN, F_CREATED, F_PENDING
CFA        Code Field Address (CFA) Pointer to the machine code
...        Parameter Field
```

The Parameter Field Address (PFA) always starts at `CFA + 1`, which `>BODY` gives from an execution
token. What it holds depends on how the word was defined:

- **Primitives** have no Parameter Field, so `>BODY` gives whatever comes next in the
  dictionary.
- **Colon words** hold a thread of CFAs, ending in `EXIT`. Literals are compiled
  as `LIT` followed by the value.
- **`CREATE`d words** hold their data.
- **`CONSTANT` words** hold their value.

An execution token (xt) identifies a word, and is that word's CFA. `'`, `[']` and
`:NONAME` give one, and `EXECUTE` runs it. `EXECUTE` and `>BODY` reject anything that
isn't the xt of a finished word.

A thread is a list of xts. The inner interpreter takes each one and runs the machine
code that its Code Field points to. For a primitive, that code is the whole word. For a
colon word, it is `DOCOL`, which continues with the thread in the Parameter Field. For a
`CREATE`d word, it is `DOVAR`, which pushes the PFA. For a `CONSTANT` word, it is
`DOCON`, which pushes the value in the Parameter Field.

`DOES>` changes the CFA of a `CREATE`d word to the address of the thread behind
`DOES>`. That address is in the dictionary and machine code never is, so `enter_word`
can tell them apart with a range check, and run the thread through `DODOES`.

Primitives return to the `run` loop instead of calling `next` themselves, so
threaded code does not grow the Rust call stack. A zero Cell ends a thread.

A word being defined stays pending until `;`, so a redefinition can call the
previous word of the same name. This also means a word cannot call itself.

If an error occurs, the interpreter discards the rest of the line, clears both
stacks, and rolls back any unfinished colon definition.

## Limitations

- There are no return stack words, such as `>R` and `R>`.
- There is no control flow, such as `IF` or `BEGIN`, and so no loops.
- There is no `[` or `]`.
- Immediate words must not use `,` while a definition is being compiled, because it
  writes into the thread and can crash the process (see issue #15). Using `CREATE`,
  `VARIABLE`, `CONSTANT`, `:` or `:NONAME` there leaves the open definition unfinished,
  so it can't be found or executed. `C,` and `ALLOT` give an error there instead.
- The dictionary size is chosen when the VM boots, and can't grow afterwards.
- The interactive prompt evaluates each line by itself, so a `(` comment must end on
  the line where it starts.
- `!` and `C!` can overwrite word headers and compiled code, which can crash the process.
- A negative `ALLOT` can only give back data of the newest word, and only if `CREATE` made
  it.

## Working with Claude Code

We use the [Claude Code](https://code.claude.com/docs/en/overview) CLI to work on this
project. It reads the project's rules from [AGENTS.md](AGENTS.md), and
[CONTRIBUTING.md](CONTRIBUTING.md#making-a-change) describes the process it follows.

Give each piece of work its own session and its own worktree, so parallel sessions can't
change each other's files. Keep your main checkout on a clean `main`, and start every
session from there:

```sh
claude -w add-nip
```

This creates `.claude/worktrees/add-nip/` on a new `worktree-add-nip` branch, based on the
latest `main` on GitHub, and starts the session inside it.

### From an idea to the merge

As an example, say you want to add the word `NIP`.

1. **Start a session.** In a terminal, in your main checkout, run `claude -w add-nip`.
2. **Plan.** Describe the idea: "I'd like to add NIP. Check the open issues and make a
   plan." Claude shows how the issues relate to the work and proposes a plan. Change it
   until you agree with it.
3. **Create the issue.** Ask Claude to create the issue, unless one already exists.
4. **Build it.** Claude writes the code, tests, demo, README and `CHANGELOG.md` changes,
   and runs `make ci`. Review the changes as they're made.
5. **Open the PR.** Ask Claude to commit, push, and open a PR that closes the issue.
6. **Pause the session.** You don't need to wait for CI. Run `/exit`, choose to keep the
   worktree, and start the next idea in a new session. Claude prints the command that
   resumes this one.
7. **Handle feedback.** To check the PR, run `gh pr checks <number>`. If a check fails or a
   review asks for changes, resume the session with the printed command, or with
   `claude -r` from your main checkout. Fix it on the same branch.
8. **Merge.** The project owner reviews the PR and merges it. Claude never merges.
9. **Clean up.** Once the PR is merged, and with its session closed, run this in your main
   checkout:

   ```sh
   git pull --ff-only
   git worktree remove .claude/worktrees/add-nip
   git branch -D worktree-add-nip
   ```

### Starting and stopping sessions

Inside a session:

| Command                | What it does                                                    |
|------------------------|-----------------------------------------------------------------|
| `/rename <name>`       | Names this session, so it's easy to find again                  |
| `/clear`               | Starts a new, empty conversation                                |
| `/resume [session]`    | Switches to another session, or opens a list of them            |
| `/branch [name]`       | Copies this conversation and switches to the copy               |
| `/fork [prompt]`       | Copies this conversation to a new background session           |
| `/background [prompt]` | Moves this session to the background                            |
| `/tasks`               | Lists the background work in this session                       |
| `/stop`                | Stops the background session you're attached to                 |
| `/exit`                | Closes this session, or detaches from a background session      |
| `Esc`                  | Interrupts Claude without ending the session                    |
| `Ctrl+C` twice         | Exits, when nothing is running                                  |

There's no command to open a second interactive session or a new worktree from inside a
session. Open another terminal and run `claude -w <name>`, or ask Claude to "work in a
worktree" if the current session should move to one. `/fork` and `/background` keep work
going without another terminal.

Exiting a session with changes or commits asks whether to keep or remove its worktree.
Removing it discards those changes, so keep it until the work is pushed.

From the shell:

| Command                    | What it does                                            |
|----------------------------|---------------------------------------------------------|
| `claude -w <name>`         | Starts a session in a new worktree                      |
| `claude -c`                | Continues the latest session in this folder             |
| `claude -r [name]`         | Resumes a session by name, or opens a list of them      |
| `claude --bg "<prompt>"`   | Starts a session in the background                      |
| `claude agents`            | Lists background sessions, and manages them             |
| `claude attach <id\|name>` | Opens a background session in this terminal             |
| `claude logs <id\|name>`   | Prints a background session's recent output             |
| `claude stop <id>`         | Stops a background session, and keeps its conversation  |

Background sessions are a research preview, so these commands may change. See the
[worktrees](https://code.claude.com/docs/en/worktrees) and
[commands](https://code.claude.com/docs/en/commands) documentation for details.

## Versioning

The current version is `0.1.0`, which hasn't been released yet.

After the first release, this project adheres to
[Semantic Versioning](https://semver.org/spec/v2.0.0.html). It is
in initial development, so the API can still change. See
[CONTRIBUTING.md](CONTRIBUTING.md#versioning) for what the version number covers.

## License

This project is licensed under the [MIT License](LICENSE).
