# Words

This is every word of these word sets, with the words that rust-forth-vm has beyond them:

- The Required Word Set of the [FORTH-83 Standard](https://www.complang.tuwien.ac.at/forth/fth83std/FORTH83.TXT), chapter 12.
- The CORE word set of ANS Forth (X3.215-1994), section 6.1.
- The CORE EXT word set of ANS Forth, section 6.2. See the [draft text of section 6](https://www.taygeta.com/forth/dpans6.htm).

The columns are:

- **Stack effect:** what the word takes from the data stack and leaves there. Text that a word
  parses from the input isn't shown, and for words that are compiled into a definition, it's the
  effect when the definition runs.
- **Standard:** the word sets that the word belongs to. `rust-forth-vm` marks a word of this
  project's own.
- **Implemented:** `Yes` if rust-forth-vm has the word. Otherwise `No`, followed by the issue
  that adds it, if one is planned. A test checks that the words marked `Yes` are exactly the words
  that the VM has.
- **Description:** what the word does here, or what it does in the standard if it isn't
  implemented yet.

Where FORTH-83 and ANS Forth differ, rust-forth-vm follows FORTH-83, and the description says so.

Division by zero gives `ForthError::DivisionByZero`. `/`, `MOD` and `/MOD` round down, as
FORTH-83 requires. ANS Forth lets the system choose. `FILL` and `MOVE` validate their complete
ranges before writing; an invalid range leaves memory unchanged. Comparison words return all bits
set for true, and zero for false. Word names are case-sensitive, and can be up to 255 bytes long.
Numbers are decimal, and may be negative.

See the [limitations](../README.md#limitations) in the README for what the VM doesn't check.

## Data stack

| Word | Stack effect | Standard | Implemented | Description |
|------|--------------|----------|-------------|-------------|
| `2DROP` | `( x1 x2 -- )` | ANS CORE | Yes | Removes the top two Cells |
| `2DUP` | `( x1 x2 -- x1 x2 x1 x2 )` | ANS CORE | Yes | Copies the top two Cells |
| `2OVER` | `( x1 x2 x3 x4 -- x1 x2 x3 x4 x1 x2 )` | ANS CORE | Yes | Copies the third and fourth Cells |
| `2SWAP` | `( x1 x2 x3 x4 -- x3 x4 x1 x2 )` | ANS CORE | Yes | Exchanges the top two pairs of Cells |
| `?DUP` | `( x -- 0 \| x x )` | FORTH-83, ANS CORE | Yes | Copies the top Cell if it isn't zero |
| `DEPTH` | `( -- n )` | FORTH-83, ANS CORE | Yes | Pushes the number of Cells on the stack |
| `DROP` | `( x -- )` | FORTH-83, ANS CORE | Yes | Removes the top Cell |
| `DUP` | `( x -- x x )` | FORTH-83, ANS CORE | Yes | Copies the top Cell |
| `NIP` | `( x1 x2 -- x2 )` | ANS CORE EXT | Yes | Removes the second Cell |
| `OVER` | `( a b -- a b a )` | FORTH-83, ANS CORE | Yes | Copies the second Cell to the top |
| `PICK` | `( ... n -- ... x )` | FORTH-83, ANS CORE EXT | Yes | Copies the `n`th Cell below `n`, counting from 0 |
| `ROLL` | `( ... n -- ... x )` | FORTH-83, ANS CORE EXT | Yes | Moves the `n`th Cell below `n` to the top, counting from 0 |
| `ROT` | `( a b c -- b c a )` | FORTH-83, ANS CORE | Yes | Moves the third Cell to the top |
| `SWAP` | `( a b -- b a )` | FORTH-83, ANS CORE | Yes | Exchanges the top two Cells |
| `TUCK` | `( x1 x2 -- x2 x1 x2 )` | ANS CORE EXT | Yes | Copies the top Cell below the second one |

## Return stack

| Word | Stack effect | Standard | Implemented | Description |
|------|--------------|----------|-------------|-------------|
| `2>R` | `( x1 x2 -- )` | ANS CORE EXT | No | Moves two Cells to the return stack, keeping their order |
| `2R>` | `( -- x1 x2 )` | ANS CORE EXT | No | Moves two Cells from the return stack, keeping their order |
| `2R@` | `( -- x1 x2 )` | ANS CORE EXT | No | Copies the top two Cells of the return stack |
| `>R` | `( x -- )` | FORTH-83, ANS CORE | No ([#117](https://github.com/michahoiting/rust-forth-vm/issues/117)) | Moves a Cell to the return stack |
| `R>` | `( -- x )` | FORTH-83, ANS CORE | No ([#117](https://github.com/michahoiting/rust-forth-vm/issues/117)) | Moves a Cell from the return stack |
| `R@` | `( -- x )` | FORTH-83, ANS CORE | No ([#117](https://github.com/michahoiting/rust-forth-vm/issues/117)) | Copies the top Cell of the return stack |

## Arithmetic

| Word | Stack effect | Standard | Implemented | Description |
|------|--------------|----------|-------------|-------------|
| `*` | `( a b -- n )` | FORTH-83, ANS CORE | Yes | Multiplies two Cells, wrapping on overflow |
| `*/` | `( n1 n2 n3 -- n4 )` | FORTH-83, ANS CORE | No ([#116](https://github.com/michahoiting/rust-forth-vm/issues/116)) | Multiplies `n1` by `n2` and divides by `n3`, with a double-cell intermediate |
| `*/MOD` | `( n1 n2 n3 -- n4 n5 )` | FORTH-83, ANS CORE | No ([#116](https://github.com/michahoiting/rust-forth-vm/issues/116)) | Like `*/`, and also gives the remainder `n4` |
| `+` | `( a b -- n )` | FORTH-83, ANS CORE | Yes | Adds two Cells, wrapping on overflow |
| `-` | `( a b -- n )` | FORTH-83, ANS CORE | Yes | Subtracts two Cells, wrapping on overflow |
| `/` | `( n1 n2 -- n3 )` | FORTH-83, ANS CORE | Yes | Divides signed numbers, rounding down |
| `/MOD` | `( n1 n2 -- n3 n4 )` | FORTH-83, ANS CORE | Yes | Gives the remainder `n3` and quotient `n4` |
| `1+` | `( n1 -- n2 )` | FORTH-83, ANS CORE | Yes | Adds one, wrapping on overflow |
| `1-` | `( n1 -- n2 )` | FORTH-83, ANS CORE | Yes | Subtracts one, wrapping on overflow |
| `2*` | `( n1 -- n2 )` | ANS CORE | Yes | Doubles a signed number, wrapping on overflow |
| `2+` | `( n1 -- n2 )` | FORTH-83 | No | Adds two |
| `2-` | `( n1 -- n2 )` | FORTH-83 | No | Subtracts two |
| `2/` | `( n1 -- n2 )` | FORTH-83, ANS CORE | Yes | Divides a signed number by two, rounding down |
| `ABS` | `( n1 -- n2 )` | FORTH-83, ANS CORE | Yes | Gives the absolute value, wrapping on overflow |
| `MAX` | `( n1 n2 -- n3 )` | FORTH-83, ANS CORE | Yes | Gives the greater signed number |
| `MIN` | `( n1 n2 -- n3 )` | FORTH-83, ANS CORE | Yes | Gives the lesser signed number |
| `MOD` | `( n1 n2 -- n3 )` | FORTH-83, ANS CORE | Yes | Gives the floored remainder of signed division |
| `NEGATE` | `( n1 -- n2 )` | FORTH-83, ANS CORE | Yes | Negates a signed number, wrapping on overflow |

## Mixed and double-cell arithmetic

| Word | Stack effect | Standard | Implemented | Description |
|------|--------------|----------|-------------|-------------|
| `D+` | `( d1 d2 -- d3 )` | FORTH-83 | No | Adds two double-cell numbers |
| `D<` | `( d1 d2 -- flag )` | FORTH-83 | No | Gives true if `d1` is less than `d2` |
| `DNEGATE` | `( d1 -- d2 )` | FORTH-83 | No | Negates a double-cell number |
| `FM/MOD` | `( d n1 -- n2 n3 )` | ANS CORE | No ([#116](https://github.com/michahoiting/rust-forth-vm/issues/116)) | Divides a double by `n1`, rounding down, giving the remainder and quotient |
| `M*` | `( n1 n2 -- d )` | ANS CORE | No ([#116](https://github.com/michahoiting/rust-forth-vm/issues/116)) | Multiplies two signed numbers into a double-cell product |
| `S>D` | `( n -- d )` | ANS CORE | No ([#116](https://github.com/michahoiting/rust-forth-vm/issues/116)) | Extends a signed number to a double-cell number |
| `SM/REM` | `( d n1 -- n2 n3 )` | ANS CORE | No ([#116](https://github.com/michahoiting/rust-forth-vm/issues/116)) | Divides a double by `n1`, rounding toward zero, giving the remainder and quotient |
| `UM*` | `( u1 u2 -- ud )` | FORTH-83, ANS CORE | No ([#116](https://github.com/michahoiting/rust-forth-vm/issues/116)) | Multiplies two unsigned numbers into a double-cell product |
| `UM/MOD` | `( ud u1 -- u2 u3 )` | FORTH-83, ANS CORE | No ([#116](https://github.com/michahoiting/rust-forth-vm/issues/116)) | Divides an unsigned double by `u1`, giving the remainder `u2` and quotient `u3` |

## Comparison and logic

| Word | Stack effect | Standard | Implemented | Description |
|------|--------------|----------|-------------|-------------|
| `0<` | `( n -- flag )` | FORTH-83, ANS CORE | Yes | Gives true if `n` is negative |
| `0<>` | `( x -- flag )` | ANS CORE EXT | No | Gives true if `x` isn't zero |
| `0=` | `( n -- flag )` | FORTH-83, ANS CORE | Yes | Gives true if `n` is zero |
| `0>` | `( n -- flag )` | FORTH-83, ANS CORE EXT | No | Gives true if `n` is greater than zero |
| `<` | `( n1 n2 -- flag )` | FORTH-83, ANS CORE | Yes | Gives true if `n1` is less than `n2` signed |
| `<>` | `( x1 x2 -- flag )` | ANS CORE EXT | No | Gives true if two Cells differ |
| `=` | `( n1 n2 -- flag )` | FORTH-83, ANS CORE | Yes | Gives true if two Cells are equal |
| `>` | `( n1 n2 -- flag )` | FORTH-83, ANS CORE | Yes | Gives true if `n1` is greater than `n2` signed |
| `AND` | `( x1 x2 -- x3 )` | FORTH-83, ANS CORE | Yes | Gives the bitwise AND of two Cells |
| `FALSE` | `( -- flag )` | ANS CORE EXT | No | Pushes a false flag, zero |
| `INVERT` | `( x1 -- x2 )` | ANS CORE | Yes | Inverts every bit in a Cell |
| `LSHIFT` | `( x1 u -- x2 )` | ANS CORE | Yes | Shifts left logically; large shifts give zero |
| `NOT` | `( x1 -- x2 )` | FORTH-83 | No | FORTH-83: inverts every bit, like `INVERT`. ANS Forth dropped it, because other Forths use it as `0=` |
| `OR` | `( x1 x2 -- x3 )` | FORTH-83, ANS CORE | Yes | Gives the bitwise OR of two Cells |
| `RSHIFT` | `( x1 u -- x2 )` | ANS CORE | Yes | Shifts right logically; large shifts give zero |
| `TRUE` | `( -- flag )` | ANS CORE EXT | No | Pushes a true flag, with all bits set |
| `U<` | `( u1 u2 -- flag )` | FORTH-83, ANS CORE | Yes | Gives true if `u1` is less than `u2` unsigned |
| `U>` | `( u1 u2 -- flag )` | ANS CORE EXT | No | Gives true if `u1` is greater than `u2` unsigned |
| `WITHIN` | `( n1 n2 n3 -- flag )` | ANS CORE EXT | No | Gives true if `n1` is at least `n2` and below `n3`, counting around the end of the range |
| `XOR` | `( x1 x2 -- x3 )` | FORTH-83, ANS CORE | Yes | Gives the bitwise exclusive OR of two Cells |

## Memory

| Word | Stack effect | Standard | Implemented | Description |
|------|--------------|----------|-------------|-------------|
| `!` | `( x addr -- )` | FORTH-83, ANS CORE | Yes | Stores a Cell at a dictionary address below `HERE` |
| `+!` | `( n addr -- )` | FORTH-83, ANS CORE | Yes | Adds `n` to the Cell at `addr`, wrapping |
| `,` | `( x -- )` | FORTH-83, ANS CORE | Yes | Aligns `HERE`, stores a Cell there and advances it. It can't be used while a definition is open |
| `2!` | `( x1 x2 addr -- )` | ANS CORE | Yes | Stores two adjacent Cells |
| `2@` | `( addr -- x1 x2 )` | ANS CORE | Yes | Fetches two adjacent Cells |
| `@` | `( addr -- x )` | FORTH-83, ANS CORE | Yes | Fetches the Cell at a dictionary address below `HERE` |
| `ALIGN` | `( -- )` | ANS CORE | Yes | Moves `HERE` to the next cell-aligned address |
| `ALIGNED` | `( addr -- a-addr )` | ANS CORE | Yes | Rounds an address up to a cell-aligned address |
| `ALLOT` | `( n -- )` | FORTH-83, ANS CORE | Yes | Reserves and clears `n` bytes at `HERE`. A negative `n` gives back data of the newest word, if `CREATE` made it |
| `C!` | `( char c-addr -- )` | FORTH-83, ANS CORE | Yes | Stores the low byte of a Cell at a dictionary address below `HERE` |
| `C,` | `( char -- )` | ANS CORE | Yes | Stores the low byte of a Cell at `HERE` and advances it, which can leave `HERE` unaligned |
| `C@` | `( c-addr -- char )` | FORTH-83, ANS CORE | Yes | Fetches the byte at a dictionary address below `HERE` |
| `CELL+` | `( addr1 -- addr2 )` | ANS CORE | Yes | Advances an address by one Cell |
| `CELLS` | `( n1 -- n2 )` | ANS CORE | Yes | Gives the size of `n1` Cells in bytes |
| `CHAR+` | `( c-addr1 -- c-addr2 )` | ANS CORE | Yes | Adds the size of a character to an address |
| `CHARS` | `( n1 -- n2 )` | ANS CORE | Yes | Gives the size of `n1` characters in bytes. A character is one byte |
| `CMOVE` | `( c-addr1 c-addr2 u -- )` | FORTH-83 | No ([#120](https://github.com/michahoiting/rust-forth-vm/issues/120)) | Copies `u` bytes, starting with the lowest address |
| `CMOVE>` | `( c-addr1 c-addr2 u -- )` | FORTH-83 | No | Copies `u` bytes, starting with the highest address |
| `ERASE` | `( addr u -- )` | ANS CORE EXT | No | Sets `u` bytes to zero |
| `FILL` | `( c-addr u char -- )` | FORTH-83, ANS CORE | Yes | Fills `u` bytes with the low byte of `char` |
| `HERE` | `( -- addr )` | FORTH-83, ANS CORE | Yes | Pushes the address of the next free byte in the dictionary |
| `MOVE` | `( addr1 addr2 u -- )` | ANS CORE | Yes | Moves `u` bytes, allowing overlapping ranges |
| `PAD` | `( -- c-addr )` | FORTH-83, ANS CORE EXT | No | Pushes the address of a scratch area for strings |
| `UNUSED` | `( -- u )` | ANS CORE EXT | No | Pushes the number of free bytes in the dictionary |

## Defining words

| Word | Stack effect | Standard | Implemented | Description |
|------|--------------|----------|-------------|-------------|
| `:` | `( -- )` | FORTH-83, ANS CORE | Yes | Parses a name and starts compiling a definition |
| `:NONAME` | `( -- xt )` | ANS CORE EXT | Yes | Starts a definition without a name, and pushes its execution token |
| `;` | `( -- )` | FORTH-83, ANS CORE | Yes | Ends the definition (immediate) |
| `CONSTANT` | `( x -- )` | FORTH-83, ANS CORE | Yes | Parses a name and creates a word that pushes `x` |
| `CREATE` | `( -- )` | FORTH-83, ANS CORE | Yes | Parses a name and creates a word that pushes its data address |
| `DOES>` | `( -- )` | FORTH-83, ANS CORE | Yes | Inside a definition: gives the newest `CREATE`d word the code that follows |
| `FORGET` | `( -- )` | FORTH-83 | No | Parses a name and removes that word and every later definition |
| `IMMEDIATE` | `( -- )` | FORTH-83, ANS CORE | Yes | Marks the newest word, so that it runs during compilation |
| `MARKER` | `( -- )` | ANS CORE EXT | No | Parses a name and creates a word that removes itself and every later definition |
| `TO` | `( x -- )` | ANS CORE EXT | No | Parses the name of a `VALUE` and gives it the value `x` |
| `VALUE` | `( x -- )` | ANS CORE EXT | No | Parses a name and creates a word that pushes `x`, which `TO` can change |
| `VARIABLE` | `( -- )` | FORTH-83, ANS CORE | Yes | Parses a name and creates a word holding one Cell, initially 0 |
| `VOCABULARY` | `( -- )` | FORTH-83 | No | Parses a name and creates a word list with that name |

## Compiling

| Word | Stack effect | Standard | Implemented | Description |
|------|--------------|----------|-------------|-------------|
| `'` | `( -- xt )` | FORTH-83, ANS CORE | Yes | Parses a name and pushes the execution token of that word |
| `>BODY` | `( xt -- addr )` | FORTH-83, ANS CORE | Yes | Replaces an execution token with the address of that word's Parameter Field |
| `COMPILE` | `( -- )` | FORTH-83 | No | FORTH-83: inside a definition, compiles the word that follows it in the thread. ANS Forth uses `POSTPONE` |
| `COMPILE,` | `( xt -- )` | ANS CORE EXT | Yes | Appends a call to the word with this execution token to the open definition |
| `EXECUTE` | `( i*x xt -- j*x )` | FORTH-83, ANS CORE | Yes | Runs the word that an execution token refers to |
| `EXIT` | `( -- )` | FORTH-83, ANS CORE | Yes | Returns from the current definition |
| `LITERAL` | `( x -- )` | FORTH-83, ANS CORE | No ([#118](https://github.com/michahoiting/rust-forth-vm/issues/118)) | Inside a definition: compiles `x` as a literal (immediate) |
| `POSTPONE` | `( -- )` | ANS CORE | No ([#118](https://github.com/michahoiting/rust-forth-vm/issues/118)) | Inside a definition: parses a name, and compiles its compilation behavior (immediate) |
| `RECURSE` | `( -- )` | ANS CORE | No ([#118](https://github.com/michahoiting/rust-forth-vm/issues/118)) | Inside a definition: compiles a call to the definition itself (immediate) |
| `STATE` | `( -- a-addr )` | FORTH-83, ANS CORE | No ([#118](https://github.com/michahoiting/rust-forth-vm/issues/118)) | Pushes the address of a Cell that's true while compiling |
| `[` | `( -- )` | FORTH-83, ANS CORE | No ([#118](https://github.com/michahoiting/rust-forth-vm/issues/118)) | Switches to interpreting, inside a definition (immediate) |
| `[']` | `( -- )` | FORTH-83, ANS CORE | Yes | Inside a definition: parses a name, and compiles its execution token as a literal (immediate) |
| `[COMPILE]` | `( -- )` | FORTH-83, ANS CORE EXT | No | Inside a definition: parses the name of an immediate word and compiles it (immediate) |
| `]` | `( -- )` | FORTH-83, ANS CORE | No ([#118](https://github.com/michahoiting/rust-forth-vm/issues/118)) | Switches to compiling |

## Control flow

| Word | Stack effect | Standard | Implemented | Description |
|------|--------------|----------|-------------|-------------|
| `+LOOP` | `( n -- )` | FORTH-83, ANS CORE | No ([#119](https://github.com/michahoiting/rust-forth-vm/issues/119)) | Inside a definition: adds `n` to the index, and repeats until it crosses the limit (immediate) |
| `?DO` | `( n1 n2 -- )` | ANS CORE EXT | No | Like `DO`, but skips the loop if `n1` equals `n2` (immediate) |
| `AGAIN` | `( -- )` | ANS CORE EXT | No ([#119](https://github.com/michahoiting/rust-forth-vm/issues/119)) | Inside a definition: always goes back to `BEGIN` (immediate) |
| `BEGIN` | `( -- )` | FORTH-83, ANS CORE | No ([#119](https://github.com/michahoiting/rust-forth-vm/issues/119)) | Inside a definition: marks the start of a loop (immediate) |
| `CASE` | `( -- )` | ANS CORE EXT | No | Inside a definition: starts a selection among `OF` clauses (immediate) |
| `DO` | `( n1 n2 -- )` | FORTH-83, ANS CORE | No ([#119](https://github.com/michahoiting/rust-forth-vm/issues/119)) | Inside a definition: starts a counted loop from `n2` up to the limit `n1` (immediate) |
| `ELSE` | `( -- )` | FORTH-83, ANS CORE | No ([#119](https://github.com/michahoiting/rust-forth-vm/issues/119)) | Inside a definition: starts the code that runs when the flag of `IF` is false (immediate) |
| `ENDCASE` | `( x -- )` | ANS CORE EXT | No | Inside a definition: ends a `CASE`, and drops the value that no clause matched (immediate) |
| `ENDOF` | `( -- )` | ANS CORE EXT | No | Inside a definition: ends an `OF` clause (immediate) |
| `I` | `( -- n )` | FORTH-83, ANS CORE | No ([#119](https://github.com/michahoiting/rust-forth-vm/issues/119)) | Pushes the index of the innermost loop |
| `IF` | `( flag -- )` | FORTH-83, ANS CORE | No ([#119](https://github.com/michahoiting/rust-forth-vm/issues/119)) | Inside a definition: runs the code up to `ELSE` or `THEN` only if `flag` is true (immediate) |
| `J` | `( -- n )` | FORTH-83, ANS CORE | No ([#119](https://github.com/michahoiting/rust-forth-vm/issues/119)) | Pushes the index of the next outer loop |
| `LEAVE` | `( -- )` | FORTH-83, ANS CORE | No ([#119](https://github.com/michahoiting/rust-forth-vm/issues/119)) | Leaves the innermost loop right away |
| `LOOP` | `( -- )` | FORTH-83, ANS CORE | No ([#119](https://github.com/michahoiting/rust-forth-vm/issues/119)) | Inside a definition: adds one to the index, and repeats until it reaches the limit (immediate) |
| `OF` | `( x1 x2 -- \| x1 )` | ANS CORE EXT | No | Inside a definition: runs the clause up to `ENDOF` if `x1` equals `x2` (immediate) |
| `REPEAT` | `( -- )` | FORTH-83, ANS CORE | No ([#119](https://github.com/michahoiting/rust-forth-vm/issues/119)) | Inside a definition: goes back to `BEGIN`, and ends the loop of `WHILE` (immediate) |
| `THEN` | `( -- )` | FORTH-83, ANS CORE | No ([#119](https://github.com/michahoiting/rust-forth-vm/issues/119)) | Inside a definition: ends an `IF` (immediate) |
| `UNLOOP` | `( -- )` | ANS CORE | No ([#119](https://github.com/michahoiting/rust-forth-vm/issues/119)) | Drops the loop parameters, so that `EXIT` can leave from inside a loop |
| `UNTIL` | `( flag -- )` | FORTH-83, ANS CORE | No ([#119](https://github.com/michahoiting/rust-forth-vm/issues/119)) | Inside a definition: goes back to `BEGIN` while `flag` is false (immediate) |
| `WHILE` | `( flag -- )` | FORTH-83, ANS CORE | No ([#119](https://github.com/michahoiting/rust-forth-vm/issues/119)) | Inside a definition: leaves a `BEGIN` loop when `flag` is false (immediate) |

## Strings and output

| Word | Stack effect | Standard | Implemented | Description |
|------|--------------|----------|-------------|-------------|
| `-TRAILING` | `( c-addr u1 -- c-addr u2 )` | FORTH-83 | No | Shortens a string to drop its trailing spaces |
| `."` | `( -- )` | FORTH-83, ANS CORE | No ([#120](https://github.com/michahoiting/rust-forth-vm/issues/120)) | Inside a definition: parses text up to `"`, and prints it when the definition runs (immediate) |
| `.(` | `( -- )` | FORTH-83, ANS CORE EXT | No | Parses text up to `)`, and prints it right away (immediate) |
| `BL` | `( -- char )` | ANS CORE | Yes | Pushes the value of the space character |
| `C"` | `( -- c-addr )` | ANS CORE EXT | No | Inside a definition: parses text up to `"`, and pushes it as a counted string (immediate) |
| `CHAR` | `( -- char )` | ANS CORE | No ([#120](https://github.com/michahoiting/rust-forth-vm/issues/120)) | Parses a name and pushes its first character |
| `COUNT` | `( c-addr1 -- c-addr2 u )` | FORTH-83, ANS CORE | No ([#120](https://github.com/michahoiting/rust-forth-vm/issues/120)) | Gives the bytes and the length of a counted string |
| `CR` | `( -- )` | FORTH-83, ANS CORE | No ([#120](https://github.com/michahoiting/rust-forth-vm/issues/120)) | Starts a new line |
| `EMIT` | `( char -- )` | FORTH-83, ANS CORE | No ([#120](https://github.com/michahoiting/rust-forth-vm/issues/120)) | Prints one character |
| `S"` | `( -- c-addr u )` | ANS CORE | No ([#120](https://github.com/michahoiting/rust-forth-vm/issues/120)) | Parses text up to `"`, and pushes it as a string |
| `SPACE` | `( -- )` | FORTH-83, ANS CORE | No ([#120](https://github.com/michahoiting/rust-forth-vm/issues/120)) | Prints a space |
| `SPACES` | `( n -- )` | FORTH-83, ANS CORE | No ([#120](https://github.com/michahoiting/rust-forth-vm/issues/120)) | Prints `n` spaces |
| `TYPE` | `( c-addr u -- )` | FORTH-83, ANS CORE | No ([#120](https://github.com/michahoiting/rust-forth-vm/issues/120)) | Prints `u` bytes |
| `[CHAR]` | `( -- )` | ANS CORE | No ([#120](https://github.com/michahoiting/rust-forth-vm/issues/120)) | Inside a definition: parses a name, and compiles its first character as a literal (immediate) |

## Numbers

| Word | Stack effect | Standard | Implemented | Description |
|------|--------------|----------|-------------|-------------|
| `#` | `( ud1 -- ud2 )` | FORTH-83, ANS CORE | No ([#121](https://github.com/michahoiting/rust-forth-vm/issues/121)) | Adds the next digit of `ud1` to the text |
| `#>` | `( xd -- c-addr u )` | FORTH-83, ANS CORE | No ([#121](https://github.com/michahoiting/rust-forth-vm/issues/121)) | Ends the conversion, and gives the text |
| `#S` | `( ud1 -- ud2 )` | FORTH-83, ANS CORE | No ([#121](https://github.com/michahoiting/rust-forth-vm/issues/121)) | Adds the remaining digits to the text, leaving zero |
| `.` | `( n -- )` | FORTH-83, ANS CORE | Yes | Prints a Cell as a signed number |
| `.R` | `( n1 n2 -- )` | ANS CORE EXT | No | Prints `n1` as a signed number, right-aligned in `n2` characters |
| `<#` | `( -- )` | FORTH-83, ANS CORE | No ([#121](https://github.com/michahoiting/rust-forth-vm/issues/121)) | Starts converting a number to text |
| `>NUMBER` | `( ud1 c-addr1 u1 -- ud2 c-addr2 u2 )` | ANS CORE | No ([#121](https://github.com/michahoiting/rust-forth-vm/issues/121)) | Adds the digits of a string to `ud1`, up to the first character that isn't a digit |
| `BASE` | `( -- a-addr )` | FORTH-83, ANS CORE | No ([#121](https://github.com/michahoiting/rust-forth-vm/issues/121)) | Pushes the address of the number base |
| `CONVERT` | `( ud1 c-addr1 -- ud2 c-addr2 )` | FORTH-83, ANS CORE EXT | No | Like `>NUMBER`, for the string after `c-addr1`, which ends at the first character that isn't a digit |
| `DECIMAL` | `( -- )` | FORTH-83, ANS CORE | No ([#121](https://github.com/michahoiting/rust-forth-vm/issues/121)) | Sets the number base to 10 |
| `HEX` | `( -- )` | ANS CORE EXT | No ([#121](https://github.com/michahoiting/rust-forth-vm/issues/121)) | Sets the number base to 16 |
| `HOLD` | `( char -- )` | FORTH-83, ANS CORE | No ([#121](https://github.com/michahoiting/rust-forth-vm/issues/121)) | Adds a character to the text |
| `SIGN` | `( n -- )` | FORTH-83, ANS CORE | No ([#121](https://github.com/michahoiting/rust-forth-vm/issues/121)) | Adds a minus sign to the text if `n` is negative |
| `U.` | `( u -- )` | FORTH-83, ANS CORE | No ([#121](https://github.com/michahoiting/rust-forth-vm/issues/121)) | Prints a Cell as an unsigned number |
| `U.R` | `( u n -- )` | ANS CORE EXT | No | Prints `u` as an unsigned number, right-aligned in `n` characters |

## Input and the interpreter

| Word | Stack effect | Standard | Implemented | Description |
|------|--------------|----------|-------------|-------------|
| `#TIB` | `( -- a-addr )` | FORTH-83, ANS CORE EXT | No | Pushes the address of the length of the terminal input buffer |
| `(` | `( -- )` | FORTH-83, ANS CORE | Yes | Skips the input up to and including the next `)` (immediate) |
| `>IN` | `( -- a-addr )` | FORTH-83, ANS CORE | No ([#122](https://github.com/michahoiting/rust-forth-vm/issues/122)) | Pushes the address of the offset into the input |
| `ACCEPT` | `( c-addr +n1 -- +n2 )` | ANS CORE | No ([#122](https://github.com/michahoiting/rust-forth-vm/issues/122)) | Reads a line of up to `+n1` characters, and gives its length |
| `ENVIRONMENT?` | `( c-addr u -- false \| i*x true )` | ANS CORE | No ([#122](https://github.com/michahoiting/rust-forth-vm/issues/122)) | Answers a question about the system, given as a string |
| `EVALUATE` | `( i*x c-addr u -- j*x )` | ANS CORE | No ([#122](https://github.com/michahoiting/rust-forth-vm/issues/122)) | Interprets a string |
| `EXPECT` | `( c-addr +n -- )` | FORTH-83, ANS CORE EXT | No | Reads a line of up to `+n` characters, and stores its length in `SPAN` |
| `FIND` | `( c-addr -- c-addr 0 \| xt 1 \| xt -1 )` | FORTH-83, ANS CORE | No ([#122](https://github.com/michahoiting/rust-forth-vm/issues/122)) | Looks up a word by a counted string, and tells whether it's immediate |
| `KEY` | `( -- char )` | FORTH-83, ANS CORE | No ([#122](https://github.com/michahoiting/rust-forth-vm/issues/122)) | Reads one character |
| `PARSE` | `( char -- c-addr u )` | ANS CORE EXT | No ([#122](https://github.com/michahoiting/rust-forth-vm/issues/122)) | Parses text up to `char`, and gives it as a string |
| `QUERY` | `( -- )` | ANS CORE EXT | No | Reads a line into the terminal input buffer |
| `REFILL` | `( -- flag )` | ANS CORE EXT | No | Reads the next line of input, and tells whether there was one |
| `RESTORE-INPUT` | `( xn ... x1 n -- flag )` | ANS CORE EXT | No | Restores the state of the input, and tells whether that failed |
| `SAVE-INPUT` | `( -- xn ... x1 n )` | ANS CORE EXT | No | Saves the state of the input |
| `SOURCE` | `( -- c-addr u )` | ANS CORE | No ([#122](https://github.com/michahoiting/rust-forth-vm/issues/122)) | Gives the current input |
| `SOURCE-ID` | `( -- 0 \| -1 )` | ANS CORE EXT | No | Gives 0 for the user input device, or -1 for a string from `EVALUATE` |
| `SPAN` | `( -- a-addr )` | FORTH-83, ANS CORE EXT | No | Pushes the address of the length that `EXPECT` read |
| `TIB` | `( -- c-addr )` | FORTH-83, ANS CORE EXT | No | Pushes the address of the terminal input buffer |
| `WORD` | `( char -- c-addr )` | FORTH-83, ANS CORE | No ([#122](https://github.com/michahoiting/rust-forth-vm/issues/122)) | Parses text up to `char`, and gives it as a counted string |
| `\` | `( -- )` | ANS CORE EXT | Yes | Skips the rest of the line (immediate) |

## Program and system

| Word | Stack effect | Standard | Implemented | Description |
|------|--------------|----------|-------------|-------------|
| `(BYE)` | `( n -- )` | rust-forth-vm | Yes | Stops the program with exit code `n`'s low byte |
| `ABORT` | `( i*x -- )` | FORTH-83, ANS CORE | Yes | Clears the data stack, then does the same as `QUIT` |
| `ABORT"` | `( i*x flag -- )` | FORTH-83, ANS CORE | No ([#120](https://github.com/michahoiting/rust-forth-vm/issues/120)) | Inside a definition: parses text up to `"`. When the definition runs with a true flag, it prints the text and does `ABORT` (immediate) |
| `BYE` | `( -- )` | ANS TOOLS EXT | Yes | Stops the program with exit code 0 |
| `COPYRIGHT` | `( -- )` | rust-forth-vm | Yes | Prints the copyright notice |
| `FORTH-83` | `( -- )` | FORTH-83 | No | Exists to show that the system follows the FORTH-83 Standard |
| `HELP` | `( -- )` | rust-forth-vm | Yes | Prints how to leave the program |
| `LICENSE` | `( -- )` | rust-forth-vm | Yes | Prints the project's license |
| `QUIT` | `( -- )` | FORTH-83, ANS CORE | Yes | Clears the return stack, discards the rest of the line, and returns to interpreting. Keeps the data stack, and doesn't leave the program. `quit_loop` reads on with the next line |

## Blocks and word lists

| Word | Stack effect | Standard | Implemented | Description |
|------|--------------|----------|-------------|-------------|
| `BLK` | `( -- a-addr )` | FORTH-83 | No | Pushes the address of the number of the block being interpreted, or 0 |
| `BLOCK` | `( u -- addr )` | FORTH-83 | No | Gives the address of block `u` in memory, reading it if needed |
| `BUFFER` | `( u -- addr )` | FORTH-83 | No | Gives a memory buffer for block `u`, without reading it |
| `DEFINITIONS` | `( -- )` | FORTH-83 | No | Adds new definitions to the first word list that's searched |
| `FLUSH` | `( -- )` | FORTH-83 | No | Writes every changed block buffer, and frees them |
| `FORTH` | `( -- )` | FORTH-83 | No | Makes the FORTH word list the first one to search |
| `LOAD` | `( u -- )` | FORTH-83 | No | Interprets block `u` |
| `SAVE-BUFFERS` | `( -- )` | FORTH-83 | No | Writes every changed block buffer to storage |
| `UPDATE` | `( -- )` | FORTH-83 | No | Marks the newest block buffer as changed |
