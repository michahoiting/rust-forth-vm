# rust-forth-vm

A clean-room implementation of a Forth virtual machine in Rust. It is an
indirect-threaded VM with a linked-list dictionary, a text interpreter, and a
compiler for colon definitions.

The program runs a scripted demo. There is no interactive prompt or file input
yet.

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

: CONSTANT CREATE , DOES> @ ;
42 CONSTANT ANSWER
ANSWER .                          \ 42

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
| `,`      | `( x -- )`   | Stores a Cell at `HERE` and advances it           |
| `CREATE` | `( -- )`     | Parses a name and creates a word that pushes its data address |
| `VARIABLE` | `( -- )`   | Parses a name and creates a word holding one Cell, initially 0 |
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

Word names are case-sensitive, and can be up to 255 bytes long. Numbers are decimal,
and may be negative.

## How it works

Every dictionary entry has the same header. The name bytes are stored in the
dictionary directly in front of it.

```
LFA        Link to the previous word
NFA        Pointer to the name bytes
LEN_FLAGS  Length of the name in bits 0-7, and the flags from bit 8 upwards
CFA        Pointer to the machine code
...        Parameter Field
```

The flags are `F_IMMEDIATE` (bit 8), `F_HIDDEN` (bit 9), `F_CREATED` (bit 10) and
`F_PENDING` (bit 11).

The Parameter Field always starts at `CFA + 1`, which `>BODY` gives from an execution
token. What it holds depends on how the word was defined:

- **Primitives** have no Parameter Field, so `>BODY` gives whatever comes next in the
  dictionary.
- **Colon words** hold a thread of CFAs, ending in `EXIT`. Literals are compiled
  as `LIT` followed by the value.
- **`CREATE`d words** hold their data.

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
  `VARIABLE`, `:` or `:NONAME` there leaves the open definition unfinished, so it
  can't be found or executed.
- The dictionary size is chosen when the VM boots, and can't grow afterwards.
- `!` can overwrite word headers and compiled code, which can crash the process.

## Versioning

This project follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html). It
is in initial development, so the API can still change. See
[CONTRIBUTING.md](CONTRIBUTING.md#versioning) for what the version number covers.

## License

This project is licensed under the [MIT License](LICENSE).
