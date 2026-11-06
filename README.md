# rust-forth-vm

A clean-room implementation of a Forth virtual machine in Rust. It is an
indirect-threaded VM with a linked-list dictionary, a text interpreter, and a
compiler for colon definitions and `CREATE ... DOES>` defining words.

The program runs a scripted demo. There is no interactive prompt or file input
yet.

## Usage

```sh
cargo run    # Run the demo
cargo test   # Run the unit tests
```

## Example

The demo evaluates lines of Forth like these:

```forth
40 2 + .                          \ 42
-5 3 + .                          \ -2

CREATE STATIONARY-SOURCE 1337 ,
STATIONARY-SOURCE @ .             \ 1337

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
```

The `\` comments are for this README only, because the interpreter does not
support comments yet.

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
| `:NONAME` | `( -- xt )` | Starts a definition without a name, and pushes its execution token |
| `EXECUTE` | `( i*x xt -- j*x )` | Runs the word that an execution token refers to |
| `EXIT`   | `( -- )`     | Returns from the current definition               |
| `DUP`    | `( x -- x x )` | Copies the top Cell                             |
| `?DUP`   | `( x -- 0 \| x x )` | Copies the top Cell if it isn't zero       |
| `DROP`   | `( x -- )`   | Removes the top Cell                              |
| `SWAP`   | `( a b -- b a )` | Exchanges the top two Cells                   |
| `OVER`   | `( a b -- a b a )` | Copies the second Cell to the top           |
| `ROT`    | `( a b c -- b c a )` | Moves the third Cell to the top           |
| `PICK`   | `( ... n -- ... x )` | Copies the `n`th Cell below `n`, counting from 0 |
| `ROLL`   | `( ... n -- ... x )` | Moves the `n`th Cell below `n` to the top, counting from 0 |
| `DEPTH`  | `( -- n )`   | Pushes the number of Cells on the stack           |

Word names are case-sensitive. Numbers are decimal, and may be negative.

## How it works

Every dictionary entry has the same header. The name bytes are stored in the
dictionary directly in front of it.

```
LFA     Link to the previous word
NFA     Pointer to the name bytes
LEN     Length of the name in bytes
FLAGS   F_IMMEDIATE, F_HIDDEN, F_CREATED, F_PENDING
CFA     Pointer to the machine code
...     Body
```

The body depends on how the word was defined:

- **Primitives** have no body.
- **Colon words** hold a thread of CFAs, ending in `EXIT`. Literals are compiled
  as `LIT` followed by the value.
- **`CREATE`d words** hold an action cell for `DOES>` at `CFA + 1`, and their
  data starts at `CFA + 2`.

Primitives return to the `run` loop instead of calling `next` themselves, so
threaded code does not grow the Rust call stack. A zero Cell ends a thread.

A word being defined stays pending until `;`, so a redefinition can call the
previous word of the same name. This also means a word cannot call itself.

If an error occurs, the interpreter discards the rest of the line, clears both
stacks, and rolls back any unfinished colon definition.

## Limitations

- There are no return stack words, such as `>R` and `R>`.
- There is no control flow, such as `IF` or `BEGIN`, and so no loops.
- There is no `IMMEDIATE`, `[` or `]`.
- The dictionary size is chosen when the VM boots, and can't grow afterwards.
- `!` can overwrite word headers and compiled code, which can crash the process.
