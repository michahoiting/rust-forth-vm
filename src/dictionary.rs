// Copyright (c) 2026 M.W.R. Hoiting

//! The dictionary: its storage, the word header layout, the table of execution tokens, and the
//! functions that write to and search the linked list of words. Nothing outside this module
//! knows how a header is laid out: other modules use its accessors.
//!
//! The arena is a block of bytes, and every address in it is a byte offset from its start, so
//! Forth address 0 is its first byte. No Cell in the arena holds a native pointer, and every
//! access is bounds-checked.
//!
//! Every word has an entry in the XT table, and its execution token (XT) is the index of that
//! entry. The entry holds what the word does, as a [`Code`], and the address of its body, which
//! is its data or its thread. The body itself stays in the arena, so `ALLOT`, `,` and `>BODY`
//! work on it like on any other memory.
//!
//! Every word also has a header in the arena, with the same layout for primitives and for words
//! created at runtime. The name bytes are stored directly in front of the header. A word's
//! `header` is the address of its first Cell, the link, and the constants below are Cell
//! indexes from there.

use std::mem;
use std::ops::Range;

use crate::cell::Cell;
use crate::error::ForthError;

/// The size of a Cell in bytes.
const CELL_SIZE: usize = mem::size_of::<Cell>();

/// Link Field: the address of the header of the previous word, or [`NO_WORD`].
const LFA: usize = 0;
/// Name Field: the address of the name bytes.
const NFA: usize = 1;
/// The length of the name in bytes, in the bits of [`LEN_MASK`], combined with the `F_*` flags
/// below.
const LEN_FLAGS: usize = 2;
/// The execution token of the word, which leads from its header back to its entry in the XT
/// table.
const XT: usize = 3;
/// The size of a header in Cells.
const HEADER_CELLS: usize = 4;

/// The link of the oldest word. The search also stops at any link that doesn't point to an
/// older header, so a header that `!` changed can't make it loop.
const NO_WORD: Cell = Cell::MAX;

/// The bits of [`LEN_FLAGS`] that hold the name length, which is also the longest name.
const LEN_MASK: Cell = 0xFF;

/// Executes even while compiling.
pub(crate) const F_IMMEDIATE: Cell = 1 << 8;
/// Can never be found by name. Unlike [`F_PENDING`], this stays set, and the word can still
/// be run through its execution token.
pub(crate) const F_HIDDEN: Cell = 1 << 9;
/// Still being defined, so it can't be found or executed. Unlike [`F_HIDDEN`], this is
/// cleared when the definition ends.
pub(crate) const F_PENDING: Cell = 1 << 10;
/// Reads an inline operand from the thread that calls it, so it only works in compiled code.
/// [`is_execution_token`](Dictionary::is_execution_token) refuses it, because `EXECUTE` would
/// run it with `IP` somewhere else. Every word with an inline operand needs this flag.
pub(crate) const F_COMPILE_ONLY: Cell = 1 << 11;

/// What a word does when it runs. This takes the place of a code field. `P` is the type of a
/// primitive, which the dictionary only stores: the VM's function type, or a number in tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Code<P> {
    /// Calls this primitive.
    Primitive(P),
    /// Runs the thread at the body, as a word made by `:` or `:NONAME`.
    Colon,
    /// Pushes the address of the body, as a word made by `CREATE` or `VARIABLE`.
    Variable,
    /// Pushes the Cell at the body, as a word made by `CONSTANT`.
    Constant,
    /// Pushes the address of the body, and runs the action thread at this address. `DOES>`
    /// gives a word made by `CREATE` this code.
    Does(Cell),
}

/// The entry of one word in the XT table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct XtEntry<P> {
    /// What the word does when it runs.
    pub(crate) code: Code<P>,
    /// The address of the word's body: its data or its thread. It directly follows the header.
    pub(crate) body: Cell,
    /// The address of the word's header.
    header: usize,
}

/// Returns `addr` rounded up to the next cell-aligned address. Like other Cell arithmetic, this
/// wraps at the top of the address range.
pub(crate) fn aligned(addr: Cell) -> Cell {
    let mask = CELL_SIZE - 1;
    addr.wrapping_add(mask) & !mask
}

/// A saved dictionary state, used to discard everything written after it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Mark {
    /// The saved [`Dictionary::here`].
    here: usize,
    /// The saved length of the XT table.
    words: usize,
}

/// The arena that holds every word, the linked list running through it, and the XT table, whose
/// primitives have the type `P`.
pub(crate) struct Dictionary<P> {
    /// The arena. Its length is a whole number of Cells, and never changes.
    bytes: Vec<u8>,
    /// The address of the next free byte. It is cell-aligned unless `C,` or `ALLOT` left it
    /// unaligned.
    here: usize,
    /// The XT table, oldest word first. The last entry is the newest word.
    words: Vec<XtEntry<P>>,
}

impl<P: Copy> Dictionary<P> {
    /// Allocates an arena of `cells` Cells for our Forth dictionary space. Its size can't change
    /// later.
    ///
    /// # Errors
    ///
    /// Returns [`ForthError::DictionaryTooLarge`] if the arena can't be allocated.
    pub(crate) fn new(cells: usize) -> Result<Dictionary<P>, ForthError> {
        let too_large = ForthError::DictionaryTooLarge(cells);
        let size = cells.checked_mul(CELL_SIZE).ok_or(too_large)?;
        let mut bytes: Vec<u8> = Vec::new();
        bytes
            .try_reserve_exact(size)
            .map_err(|_| ForthError::DictionaryTooLarge(cells))?;
        bytes.resize(size, 0);

        Ok(Dictionary {
            bytes,
            here: 0,
            words: Vec::new(),
        })
    }

    /// Returns HERE, which is the address of the next free byte.
    pub(crate) fn here(&self) -> Cell {
        self.here
    }

    /// Returns the first cell-aligned address at or after HERE. The arena ends on a Cell
    /// boundary, so this is never past its end.
    fn next_cell(&self) -> usize {
        aligned(self.here)
    }

    /// Returns how many bytes are free from `from` to the end of the arena.
    fn free_bytes(&self, from: usize) -> usize {
        self.bytes.len() - from
    }

    /// Reads the Cell at `offset`, which the caller has checked.
    fn read_cell(&self, offset: usize) -> Cell {
        let mut bytes = [0; CELL_SIZE];
        bytes.copy_from_slice(&self.bytes[offset..offset + CELL_SIZE]);
        Cell::from_ne_bytes(bytes)
    }

    /// Writes the Cell at `offset`, which the caller has checked.
    fn write_cell(&mut self, offset: usize, value: Cell) {
        self.bytes[offset..offset + CELL_SIZE].copy_from_slice(&value.to_ne_bytes());
    }

    /// Moves HERE to the next cell-aligned address, and clears the bytes it passes.
    pub(crate) fn align(&mut self) {
        let cell = self.next_cell();
        self.bytes[self.here..cell].fill(0);
        self.here = cell;
    }

    /// Presses a Cell into the dictionary at HERE, aligning HERE first.
    ///
    /// # Errors
    ///
    /// Returns [`ForthError::DictionaryOverflow`] if the Cell doesn't fit. HERE doesn't move.
    pub(crate) fn comma(&mut self, value: Cell) -> Result<(), ForthError> {
        if self.free_bytes(self.next_cell()) < CELL_SIZE {
            return Err(ForthError::DictionaryOverflow);
        }
        self.align();
        self.write_cell(self.here, value);
        self.here += CELL_SIZE;
        Ok(())
    }

    /// Presses one byte into the dictionary at HERE, which can leave HERE unaligned.
    ///
    /// # Errors
    ///
    /// Returns [`ForthError::DictionaryOverflow`] if the dictionary is full.
    pub(crate) fn char_comma(&mut self, value: u8) -> Result<(), ForthError> {
        if self.free_bytes(self.here) == 0 {
            return Err(ForthError::DictionaryOverflow);
        }
        self.bytes[self.here] = value;
        self.here += 1;
        Ok(())
    }

    /// Moves HERE by `bytes`, which can leave it unaligned. Reserved bytes are cleared. Only
    /// the data of the newest word can be given back, and only if `CREATE` made it. Any other
    /// word ends in code or a value that would be overwritten.
    ///
    /// # Errors
    ///
    /// Returns [`ForthError::DictionaryOverflow`] if `bytes` doesn't fit, and
    /// [`ForthError::AllotBelowData`] if it gives back more than that data.
    pub(crate) fn allot(&mut self, bytes: isize) -> Result<(), ForthError> {
        let size = bytes.unsigned_abs();
        if bytes >= 0 {
            if size > self.free_bytes(self.here) {
                return Err(ForthError::DictionaryOverflow);
            }
            self.bytes[self.here..self.here + size].fill(0);
            self.here += size;
        } else {
            let data = match self.words.last() {
                Some(entry) if is_created(entry.code) => entry.body,
                _ => return Err(ForthError::AllotBelowData(bytes)),
            };
            if size > self.here - data {
                return Err(ForthError::AllotBelowData(bytes));
            }
            self.here -= size;
        }
        Ok(())
    }

    /// Checks that `len` bytes at `addr` are in the written part of the arena, and returns
    /// `addr`. The checks run in a fixed order, so the error names the first fault.
    ///
    /// # Errors
    ///
    /// Returns [`ForthError::AddressOutOfBounds`] if `addr` isn't in the arena, and
    /// [`ForthError::UnwrittenAddress`] if the bytes reach HERE or past it.
    #[inline(always)]
    fn written_offset(&self, addr: Cell, len: usize) -> Result<usize, ForthError> {
        let end = self.bytes.len();
        if addr >= end && !(len == 0 && addr == end && addr == self.here) {
            return Err(ForthError::AddressOutOfBounds(addr));
        }
        if addr.checked_add(len).is_none_or(|end| end > self.here) {
            return Err(ForthError::UnwrittenAddress(addr));
        }
        Ok(addr)
    }

    /// Checks that `len` bytes at `addr` are written and returns their range in the arena.
    ///
    /// # Errors
    ///
    /// Returns [`ForthError::AddressOutOfBounds`] if `addr` isn't in the arena, and
    /// [`ForthError::UnwrittenAddress`] if the range reaches HERE or past it. An empty range
    /// at the end of the arena is valid.
    fn written_range(&self, addr: Cell, len: usize) -> Result<Range<usize>, ForthError> {
        let offset = self.written_offset(addr, len)?;
        Ok(offset..offset + len)
    }

    /// Checks that the `count` Cells at `addr` are written and cell-aligned, and returns
    /// `addr`.
    ///
    /// # Errors
    ///
    /// Returns [`ForthError::AddressOutOfBounds`] or [`ForthError::UnwrittenAddress`] from
    /// [`written_offset`](Self::written_offset), and [`ForthError::MisalignedAddress`] if
    /// `addr` isn't cell-aligned.
    #[inline(always)]
    fn cells_offset(&self, addr: Cell, count: usize) -> Result<usize, ForthError> {
        let offset = self.written_offset(addr, count * CELL_SIZE)?;
        if !offset.is_multiple_of(CELL_SIZE) {
            return Err(ForthError::MisalignedAddress(addr));
        }
        Ok(offset)
    }

    /// Reads two adjacent Cells at `addr`, which must be aligned.
    ///
    /// # Errors
    ///
    /// Returns the same address errors as [`fetch`](Self::fetch).
    pub(crate) fn fetch_two(&self, addr: Cell) -> Result<(Cell, Cell), ForthError> {
        let offset = self.cells_offset(addr, 2)?;
        Ok((self.read_cell(offset), self.read_cell(offset + CELL_SIZE)))
    }

    /// Writes two adjacent Cells at `addr`, checking both before either write.
    ///
    /// # Errors
    ///
    /// Returns the same address errors as [`store`](Self::store). Nothing is written.
    pub(crate) fn store_two(
        &mut self,
        addr: Cell,
        first: Cell,
        second: Cell,
    ) -> Result<(), ForthError> {
        let offset = self.cells_offset(addr, 2)?;
        self.write_cell(offset, first);
        self.write_cell(offset + CELL_SIZE, second);
        Ok(())
    }

    /// Fills `len` bytes at `addr`.
    ///
    /// # Errors
    ///
    /// Returns [`ForthError::AddressOutOfBounds`] or [`ForthError::UnwrittenAddress`] if the
    /// range isn't written. No bytes are changed on error.
    pub(crate) fn fill(&mut self, addr: Cell, len: usize, byte: u8) -> Result<(), ForthError> {
        let range = self.written_range(addr, len)?;
        self.bytes[range].fill(byte);
        Ok(())
    }

    /// Moves `len` bytes from `source` to `destination`, allowing overlap.
    ///
    /// # Errors
    ///
    /// Returns [`ForthError::AddressOutOfBounds`] or [`ForthError::UnwrittenAddress`] if either
    /// range isn't written. No bytes are changed on error.
    pub(crate) fn move_bytes(
        &mut self,
        source: Cell,
        destination: Cell,
        len: usize,
    ) -> Result<(), ForthError> {
        let source_range = self.written_range(source, len)?;
        let destination_range = self.written_range(destination, len)?;
        self.bytes
            .copy_within(source_range, destination_range.start);
        Ok(())
    }

    /// Reads the Cell at `addr`, which must be cell-aligned and end at or below HERE. This keeps
    /// the Cell inside the part of the arena that has been written. The inner interpreter reads
    /// threads through here too.
    ///
    /// # Errors
    ///
    /// Returns [`ForthError::AddressOutOfBounds`] if `addr` isn't in the arena,
    /// [`ForthError::UnwrittenAddress`] if the Cell reaches HERE or past it, and
    /// [`ForthError::MisalignedAddress`] if `addr` isn't cell-aligned.
    #[inline(always)]
    pub(crate) fn fetch(&self, addr: Cell) -> Result<Cell, ForthError> {
        let offset = self.cells_offset(addr, 1)?;
        Ok(self.read_cell(offset))
    }

    /// Reads the Cell of a thread at `ip`, for the inner interpreter and words with inline
    /// operands. This is the hot path, so it only checks that the Cell is inside the arena, which
    /// is all that keeps the read safe. Unlike [`fetch`](Self::fetch), it doesn't check HERE or
    /// the alignment. `ip` is always cell-aligned, because it only ever comes from the body of a
    /// word, the action of `DOES>`, or the return stack, and moves a Cell at a time. It only
    /// passes HERE if `!` overwrote the `EXIT` of a thread, and then reads whatever is there.
    /// Returns `None` if the Cell isn't inside the arena, as for
    /// [`NO_IP`](crate::vm::NO_IP), whose range wraps.
    #[inline(always)]
    pub(crate) fn fetch_thread(&self, ip: Cell) -> Option<Cell> {
        let bytes = self.bytes.get(ip..ip.wrapping_add(CELL_SIZE))?;
        let mut cell = [0; CELL_SIZE];
        cell.copy_from_slice(bytes);
        Some(Cell::from_ne_bytes(cell))
    }

    /// Writes the Cell at `addr`, checked in the same way as `fetch`. This can change word
    /// headers and compiled code, but not the XT table.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`fetch`](Self::fetch).
    pub(crate) fn store(&mut self, addr: Cell, value: Cell) -> Result<(), ForthError> {
        let offset = self.cells_offset(addr, 1)?;
        self.write_cell(offset, value);
        Ok(())
    }

    /// Reads the byte at `addr`, which needn't be aligned.
    ///
    /// # Errors
    ///
    /// Returns [`ForthError::AddressOutOfBounds`] or [`ForthError::UnwrittenAddress`] if the
    /// byte isn't below HERE.
    pub(crate) fn fetch_byte(&self, addr: Cell) -> Result<u8, ForthError> {
        let offset = self.written_offset(addr, 1)?;
        Ok(self.bytes[offset])
    }

    /// Writes the byte at `addr`, checked in the same way as `fetch_byte`. Like `store`, this
    /// can change word headers and compiled code.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`fetch_byte`](Self::fetch_byte).
    pub(crate) fn store_byte(&mut self, addr: Cell, value: u8) -> Result<(), ForthError> {
        let offset = self.written_offset(addr, 1)?;
        self.bytes[offset] = value;
        Ok(())
    }

    /// Writes a complete word into the dictionary: the name bytes, then the header, both at
    /// the next cell-aligned address, and adds its entry to the XT table. The body of the word
    /// starts at HERE afterwards. Returns the execution token of the new word.
    ///
    /// # Errors
    ///
    /// Returns [`ForthError::NameTooLong`] if the length doesn't fit in [`LEN_MASK`], and
    /// [`ForthError::DictionaryOverflow`] if the word doesn't fit. Nothing is written.
    pub(crate) fn write_header(
        &mut self,
        name: &str,
        code: Code<P>,
        flags: Cell,
    ) -> Result<Cell, ForthError> {
        if name.len() > LEN_MASK {
            return Err(ForthError::NameTooLong(name.len()));
        }

        let name_cells = name.len().div_ceil(CELL_SIZE);
        if (name_cells + HEADER_CELLS) * CELL_SIZE > self.free_bytes(self.next_cell()) {
            return Err(ForthError::DictionaryOverflow);
        }
        self.align();

        // The dictionary owns its own copy, because the input does not outlive this call.
        let name_offset = self.here;
        self.bytes[name_offset..name_offset + name.len()].copy_from_slice(name.as_bytes());
        // Clear the padding, which may hold bytes from a word that was rolled back.
        self.bytes[name_offset + name.len()..name_offset + name_cells * CELL_SIZE].fill(0);

        let header = name_offset + name_cells * CELL_SIZE;
        let link = self.words.last().map_or(NO_WORD, |entry| entry.header);
        let xt = self.words.len();
        self.write_cell(header + LFA * CELL_SIZE, link);
        self.write_cell(header + NFA * CELL_SIZE, name_offset);
        self.write_cell(header + LEN_FLAGS * CELL_SIZE, name.len() | flags);
        self.write_cell(header + XT * CELL_SIZE, xt);

        self.here = header + HEADER_CELLS * CELL_SIZE;
        self.words.push(XtEntry {
            code,
            body: self.here,
            header,
        });
        Ok(xt)
    }

    /// Reads all Cells of the header at `header`, or returns `None` if it isn't in the arena.
    /// Headers can be changed by `!`, so nothing read from one is trusted. Reading the whole
    /// header at once takes one bounds check, which matters to the word search.
    #[inline(always)]
    fn header_cells(&self, header: usize) -> Option<[Cell; HEADER_CELLS]> {
        let end = header.checked_add(HEADER_CELLS * CELL_SIZE)?;
        let bytes: &[u8; HEADER_CELLS * CELL_SIZE] =
            self.bytes.get(header..end)?.try_into().ok()?;
        let mut cells = [0; HEADER_CELLS];
        for (cell, chunk) in cells.iter_mut().zip(bytes.chunks_exact(CELL_SIZE)) {
            let mut cell_bytes = [0; CELL_SIZE];
            cell_bytes.copy_from_slice(chunk);
            *cell = Cell::from_ne_bytes(cell_bytes);
        }
        Some(cells)
    }

    /// Reads Cell `field` of the header at `header`, or returns `None` if the header isn't in the
    /// arena.
    fn header_cell(&self, header: usize, field: usize) -> Option<Cell> {
        self.header_cells(header).map(|cells| cells[field])
    }

    /// Returns the flags of the header at `header`, without the name length. A header that
    /// isn't in the arena reads as [`F_HIDDEN`] and [`F_PENDING`], so it can't be found or run.
    fn header_flags(&self, header: usize) -> Cell {
        self.header_cell(header, LEN_FLAGS)
            .map_or(F_HIDDEN | F_PENDING, |cell| cell & !LEN_MASK)
    }

    /// Returns the name bytes of a header with these Cells, or `None` if they aren't in the
    /// arena.
    #[inline(always)]
    fn name_of(&self, cells: &[Cell; HEADER_CELLS]) -> Option<&[u8]> {
        let name = cells[NFA];
        let len = cells[LEN_FLAGS] & LEN_MASK;
        self.bytes.get(name..name.checked_add(len)?)
    }

    /// Returns whether a name can find a word with these header flags, so it's neither hidden nor
    /// still being defined.
    fn is_findable(len_flags: Cell) -> bool {
        len_flags & (F_HIDDEN | F_PENDING) == 0
    }

    /// Returns the Cells of the headers in the word list, from newest to oldest. It follows the
    /// links, but stops at a link that doesn't point to an older header, or at a header that
    /// isn't in the arena, so it always ends.
    #[inline(always)]
    fn headers(&self) -> impl Iterator<Item = [Cell; HEADER_CELLS]> + '_ {
        let mut current = self.words.last().map(|entry| entry.header);
        std::iter::from_fn(move || {
            let header = current?;
            let cells = self.header_cells(header)?;
            current = Some(cells[LFA]).filter(|&link| link < header);
            Some(cells)
        })
    }

    /// Linearly traverses the linked list of the dictionary (from newest to oldest). Returns
    /// the execution token stored in the header of the matching word. The length of the name is
    /// compared before its bytes are read.
    pub(crate) fn find_word(&self, token: &str) -> Option<Cell> {
        let token = token.as_bytes();
        self.headers()
            .find(|cells| {
                let len_flags = cells[LEN_FLAGS];
                Self::is_findable(len_flags)
                    && len_flags & LEN_MASK == token.len()
                    && self.name_of(cells) == Some(token)
            })
            .map(|cells| cells[XT])
    }

    /// Returns the sorted names of the words that [`find_word`](Self::find_word) can find, each
    /// one once. A name that isn't valid UTF-8, because `C!` overwrote it, comes back with
    /// replacement characters.
    pub(crate) fn word_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .headers()
            .filter(|cells| Self::is_findable(cells[LEN_FLAGS]))
            .filter_map(|cells| self.name_of(&cells))
            .filter(|name| !name.is_empty())
            .map(|name| String::from_utf8_lossy(name).into_owned())
            .collect();
        names.sort_unstable();
        names.dedup();
        names
    }

    /// Returns the entry of the word with this execution token, or `None` if there is none.
    #[inline(always)]
    pub(crate) fn entry(&self, xt: Cell) -> Option<XtEntry<P>> {
        self.words.get(xt).copied()
    }

    /// Returns the flags of the word with this execution token, without the name length, or
    /// `None` if there is no such word.
    pub(crate) fn flags(&self, xt: Cell) -> Option<Cell> {
        self.words
            .get(xt)
            .map(|entry| self.header_flags(entry.header))
    }

    /// Returns whether the word with this execution token has [`F_IMMEDIATE`] set.
    pub(crate) fn is_immediate(&self, xt: Cell) -> bool {
        self.flags(xt).is_some_and(|flags| flags & F_IMMEDIATE != 0)
    }

    /// Returns the execution token of the most recently created word, or `None` if there are
    /// none.
    #[cfg(test)]
    pub(crate) fn last_xt(&self) -> Option<Cell> {
        self.words.len().checked_sub(1)
    }

    /// Changes the flags of the most recently created word with `change`, if there is one. The
    /// name length stays as it is.
    fn change_last_flags(&mut self, change: impl FnOnce(Cell) -> Cell) {
        let Some(header) = self.words.last().map(|entry| entry.header) else {
            return;
        };
        let offset = header + LEN_FLAGS * CELL_SIZE;
        let len_flags = self.read_cell(offset);
        let flags = change(len_flags & !LEN_MASK) & !LEN_MASK;
        self.write_cell(offset, len_flags & LEN_MASK | flags);
    }

    /// Marks the most recently created word as finished. Its other flags stay as they are.
    pub(crate) fn finish_last(&mut self) {
        self.change_last_flags(|flags| flags & !F_PENDING);
    }

    /// Marks the most recently created word as immediate. Its other flags stay as they are.
    pub(crate) fn mark_last_immediate(&mut self) {
        self.change_last_flags(|flags| flags | F_IMMEDIATE);
    }

    /// Returns whether the most recently created word was made by `CREATE`, so that `DOES>`
    /// may change its action.
    pub(crate) fn is_last_created(&self) -> bool {
        self.words
            .last()
            .is_some_and(|entry| is_created(entry.code))
    }

    /// Gives the most recently created word a new action, if there is one.
    pub(crate) fn set_last_code(&mut self, code: Code<P>) {
        if let Some(entry) = self.words.last_mut() {
            entry.code = code;
        }
    }

    /// Returns whether `xt` is the execution token of a finished word that can run from
    /// anywhere: hidden or not, but not one with [`F_COMPILE_ONLY`].
    pub(crate) fn is_execution_token(&self, xt: Cell) -> bool {
        self.flags(xt)
            .is_some_and(|flags| flags & (F_PENDING | F_COMPILE_ONLY) == 0)
    }

    /// Saves the current state, for a later [`restore`](Self::restore).
    pub(crate) fn mark(&self) -> Mark {
        Mark {
            here: self.here,
            words: self.words.len(),
        }
    }

    /// Discards everything written since `mark` was saved, including the words in the XT
    /// table.
    pub(crate) fn restore(&mut self, mark: Mark) {
        self.here = mark.here;
        self.words.truncate(mark.words);
    }
}

/// Returns whether a word with this code was made by `CREATE`, so that `DOES>` may change its
/// action and `ALLOT` may give back its data.
fn is_created<P>(code: Code<P>) -> bool {
    matches!(code, Code::Variable | Code::Does(_))
}

#[cfg(test)]
mod tests {
    use std::io;

    use super::*;
    use crate::{boot, outer_interpreter, VmConfig};

    const TEST_CELLS: usize = 64 * 1024;

    // A dictionary whose primitives are numbers.
    type Dictionary = super::Dictionary<usize>;

    // A Cell that stands for -1, and for -1 Cell, as a negative address.
    const MINUS_ONE: Cell = Cell::MAX;
    const MINUS_ONE_CELL: Cell = Cell::MAX - CELL_SIZE + 1;

    // Creates a dictionary holding these Cells, starting at its first Cell.
    fn dictionary_with(values: &[Cell]) -> Dictionary {
        let mut dictionary = Dictionary::new(TEST_CELLS).unwrap();
        for value in values {
            dictionary.comma(*value).unwrap();
        }
        dictionary
    }

    // Returns the address one past the last byte of the arena.
    fn end_of(dictionary: &Dictionary) -> Cell {
        dictionary.bytes.len()
    }

    // Returns the header of the word with this execution token.
    fn header_of<P>(dictionary: &super::Dictionary<P>, xt: Cell) -> usize {
        dictionary.words[xt].header
    }

    // Returns the length and flags Cell of the word with this execution token.
    fn len_flags_of(dictionary: &Dictionary, xt: Cell) -> Cell {
        dictionary
            .header_cell(header_of(dictionary, xt), LEN_FLAGS)
            .unwrap()
    }

    fn has_flag(dictionary: &Dictionary, xt: Cell, flag: Cell) -> bool {
        dictionary.flags(xt).unwrap() & flag != 0
    }

    // The error variants carry no payload besides the address, so Debug output pins the exact
    // variant and address.
    fn assert_fetch_rejected(dictionary: &Dictionary, addr: Cell, make: fn(Cell) -> ForthError) {
        let result = dictionary.fetch(addr);
        assert_eq!(format!("{:?}", result), format!("Err({:?})", make(addr)));
    }

    fn assert_store_rejected(
        dictionary: &mut Dictionary,
        addr: Cell,
        make: fn(Cell) -> ForthError,
    ) {
        let result = dictionary.store(addr, 9);
        assert_eq!(format!("{:?}", result), format!("Err({:?})", make(addr)));
    }

    // Checks that `fetch` and `store` both reject `addr` with the error that `make` builds.
    fn assert_rejected(dictionary: &mut Dictionary, addr: Cell, make: fn(Cell) -> ForthError) {
        assert_fetch_rejected(dictionary, addr, make);
        assert_store_rejected(dictionary, addr, make);
    }

    #[test]
    fn aligned_rounds_up_to_a_cell_boundary() {
        assert_eq!(aligned(0), 0);
        assert_eq!(aligned(1), CELL_SIZE);
        assert_eq!(aligned(CELL_SIZE - 1), CELL_SIZE);
        assert_eq!(aligned(CELL_SIZE), CELL_SIZE);
        assert_eq!(aligned(CELL_SIZE + 1), 2 * CELL_SIZE);
    }

    #[test]
    fn aligned_wraps_at_the_top_of_the_address_range() {
        assert_eq!(aligned(Cell::MAX), 0);
    }

    #[test]
    fn new_rejects_a_size_that_overflows() {
        assert!(matches!(
            Dictionary::new(usize::MAX),
            Err(ForthError::DictionaryTooLarge(usize::MAX))
        ));
    }

    #[test]
    fn store_byte_changes_only_the_target_byte() {
        let mut dictionary = dictionary_with(&[0, 0]);
        dictionary.store_byte(1, 0xAB).unwrap();

        for offset in 0..2 * CELL_SIZE {
            let expected = if offset == 1 { 0xAB } else { 0 };
            assert_eq!(dictionary.fetch_byte(offset).unwrap(), expected);
        }
    }

    #[test]
    fn byte_access_covers_exactly_the_written_bytes() {
        let mut dictionary = dictionary_with(&[0]);
        let here = dictionary.here();
        let end = end_of(&dictionary);

        assert!(dictionary.fetch_byte(0).is_ok());
        assert!(dictionary.fetch_byte(here - 1).is_ok());
        for addr in [MINUS_ONE, end] {
            assert!(matches!(
                dictionary.fetch_byte(addr),
                Err(ForthError::AddressOutOfBounds(a)) if a == addr
            ));
            assert!(matches!(
                dictionary.store_byte(addr, 1),
                Err(ForthError::AddressOutOfBounds(a)) if a == addr
            ));
        }
        for addr in [here, here + 1, end - 1] {
            assert!(matches!(
                dictionary.fetch_byte(addr),
                Err(ForthError::UnwrittenAddress(a)) if a == addr
            ));
            assert!(matches!(
                dictionary.store_byte(addr, 1),
                Err(ForthError::UnwrittenAddress(a)) if a == addr
            ));
        }
    }

    #[test]
    fn a_cell_that_ends_past_here_is_rejected() {
        let mut dictionary = dictionary_with(&[1]);
        let addr = dictionary.here();
        dictionary.char_comma(2).unwrap();
        assert!(addr < dictionary.here());

        assert_rejected(&mut dictionary, addr, ForthError::UnwrittenAddress);
    }

    #[test]
    fn cell_access_checks_bounds_then_here_then_alignment() {
        let mut dictionary = dictionary_with(&[1, 2]);
        let here = dictionary.here();
        let end = end_of(&dictionary);

        assert_eq!(dictionary.fetch(0).unwrap(), 1);
        assert_eq!(dictionary.fetch(here - CELL_SIZE).unwrap(), 2);
        assert!(dictionary.store(here - CELL_SIZE, 3).is_ok());

        assert_rejected(&mut dictionary, MINUS_ONE, ForthError::AddressOutOfBounds);
        assert_rejected(
            &mut dictionary,
            MINUS_ONE_CELL,
            ForthError::AddressOutOfBounds,
        );
        assert_rejected(&mut dictionary, end, ForthError::AddressOutOfBounds);

        assert_rejected(&mut dictionary, here, ForthError::UnwrittenAddress);
        assert_rejected(&mut dictionary, here - 1, ForthError::UnwrittenAddress);
        assert_rejected(&mut dictionary, here + 1, ForthError::UnwrittenAddress);
        assert_rejected(
            &mut dictionary,
            end - CELL_SIZE,
            ForthError::UnwrittenAddress,
        );
        assert_rejected(&mut dictionary, end - 1, ForthError::UnwrittenAddress);

        assert_rejected(&mut dictionary, 1, ForthError::MisalignedAddress);
        assert_rejected(
            &mut dictionary,
            here - CELL_SIZE + 1,
            ForthError::UnwrittenAddress,
        );
    }

    #[test]
    fn char_comma_stores_one_byte_and_can_leave_here_unaligned() {
        let mut dictionary = dictionary_with(&[]);
        let start = dictionary.here();
        dictionary.char_comma(7).unwrap();

        assert_eq!(dictionary.here(), start + 1);
        assert_eq!(dictionary.fetch_byte(start).unwrap(), 7);
    }

    #[test]
    fn char_comma_fills_the_arena_and_then_overflows() {
        let mut dictionary = Dictionary::new(1).unwrap();
        for byte in 0..CELL_SIZE {
            dictionary.char_comma(byte as u8).unwrap();
        }
        let here = dictionary.here();

        assert!(matches!(
            dictionary.char_comma(0),
            Err(ForthError::DictionaryOverflow)
        ));
        assert_eq!(dictionary.here(), here);
    }

    #[test]
    fn align_moves_here_and_clears_the_bytes_it_passes() {
        let mut dictionary = dictionary_with(&[Cell::MAX]);
        let mark = dictionary.mark();
        dictionary.restore(Mark { here: 0, ..mark });
        dictionary.char_comma(1).unwrap();
        dictionary.align();

        assert_eq!(dictionary.here(), CELL_SIZE);
        assert_eq!(
            dictionary.fetch(0).unwrap(),
            Cell::from_ne_bytes({
                let mut bytes = [0; CELL_SIZE];
                bytes[0] = 1;
                bytes
            })
        );
    }

    #[test]
    fn align_leaves_an_aligned_here_alone() {
        let mut dictionary = dictionary_with(&[1]);
        let here = dictionary.here();
        dictionary.align();
        assert_eq!(dictionary.here(), here);
    }

    #[test]
    fn align_stops_at_the_end_of_the_arena() {
        let mut dictionary = Dictionary::new(1).unwrap();
        dictionary.char_comma(1).unwrap();
        dictionary.align();
        assert_eq!(dictionary.here(), end_of(&dictionary));
    }

    #[test]
    fn comma_aligns_here_first() {
        let mut dictionary = dictionary_with(&[]);
        let start = dictionary.here();
        dictionary.char_comma(1).unwrap();
        dictionary.comma(7).unwrap();

        assert_eq!(dictionary.here(), start + 2 * CELL_SIZE);
        assert_eq!(dictionary.fetch(start + CELL_SIZE).unwrap(), 7);
    }

    #[test]
    fn comma_that_does_not_fit_after_aligning_leaves_here_alone() {
        let mut dictionary = Dictionary::new(1).unwrap();
        dictionary.char_comma(1).unwrap();
        let here = dictionary.here();

        assert!(matches!(
            dictionary.comma(2),
            Err(ForthError::DictionaryOverflow)
        ));
        assert_eq!(dictionary.here(), here);
    }

    #[test]
    fn write_header_aligns_here_first() {
        let mut dictionary = dictionary_with(&[]);
        dictionary.char_comma(1).unwrap();
        let xt = dictionary.write_header("X", Code::Colon, 0).unwrap();

        assert!(header_of(&dictionary, xt).is_multiple_of(CELL_SIZE));
        assert_eq!(dictionary.find_word("X"), Some(xt));
        assert!(dictionary.here().is_multiple_of(CELL_SIZE));
    }

    #[test]
    fn write_header_that_does_not_fit_after_aligning_changes_nothing() {
        // The name and header fill the arena exactly when HERE starts aligned.
        let mut dictionary = Dictionary::new(HEADER_CELLS + 1).unwrap();
        dictionary.char_comma(1).unwrap();
        let mark = dictionary.mark();

        assert!(matches!(
            dictionary.write_header("X", Code::Colon, 0),
            Err(ForthError::DictionaryOverflow)
        ));
        assert_eq!(dictionary.mark(), mark);
    }

    #[test]
    fn write_header_lays_out_the_name_and_the_header() {
        let mut dictionary = dictionary_with(&[]);
        let first = dictionary.write_header("A", Code::Colon, 0).unwrap();
        let second = dictionary
            .write_header("ABCDEFGHI", Code::Variable, F_HIDDEN)
            .unwrap();
        let name_cells = "ABCDEFGHI".len().div_ceil(CELL_SIZE);
        let first_header = CELL_SIZE;
        let second_header = first_header + HEADER_CELLS * CELL_SIZE + name_cells * CELL_SIZE;

        assert_eq!((first, second), (0, 1));
        assert_eq!(header_of(&dictionary, first), first_header);
        assert_eq!(header_of(&dictionary, second), second_header);
        let header = |field| dictionary.header_cell(second_header, field).unwrap();
        assert_eq!(header(LFA), first_header);
        assert_eq!(header(NFA), first_header + HEADER_CELLS * CELL_SIZE);
        assert_eq!(header(LEN_FLAGS), 9 | F_HIDDEN);
        assert_eq!(header(XT), second);
        assert_eq!(dictionary.header_cell(first_header, LFA), Some(NO_WORD));
        assert_eq!(
            dictionary.entry(second),
            Some(XtEntry {
                code: Code::Variable,
                body: dictionary.here(),
                header: second_header,
            })
        );
    }

    #[test]
    fn write_header_clears_the_padding_after_the_name() {
        let mut dictionary = dictionary_with(&[Cell::MAX]);
        dictionary.restore(Mark { here: 0, words: 0 });
        dictionary.write_header("X", Code::Colon, 0).unwrap();
        let mut expected = [0; CELL_SIZE];
        expected[0] = b'X';
        assert_eq!(dictionary.fetch(0).unwrap(), Cell::from_ne_bytes(expected));
    }

    #[test]
    fn restore_returns_to_an_unaligned_here() {
        let mut dictionary = dictionary_with(&[]);
        dictionary.char_comma(1).unwrap();
        let mark = dictionary.mark();
        dictionary.comma(2).unwrap();
        dictionary.restore(mark);
        assert_eq!(dictionary.mark(), mark);
    }

    #[test]
    fn allot_reserves_and_clears_bytes() {
        let mut dictionary = dictionary_with(&[Cell::MAX]);
        let mark = dictionary.mark();
        dictionary.restore(Mark { here: 0, ..mark });
        dictionary.allot(3).unwrap();

        assert_eq!(dictionary.here(), 3);
        for offset in 0..3 {
            assert_eq!(dictionary.fetch_byte(offset).unwrap(), 0);
        }
    }

    #[test]
    fn allot_fills_the_arena_and_then_overflows() {
        let mut dictionary = Dictionary::new(2).unwrap();
        let here = dictionary.here();
        let size = (2 * CELL_SIZE) as isize;

        assert!(matches!(
            dictionary.allot(size + 1),
            Err(ForthError::DictionaryOverflow)
        ));
        assert!(matches!(
            dictionary.allot(isize::MAX),
            Err(ForthError::DictionaryOverflow)
        ));
        assert_eq!(dictionary.here(), here);

        dictionary.allot(size).unwrap();
        assert_eq!(dictionary.here(), end_of(&dictionary));
    }

    #[test]
    fn allot_gives_back_the_data_of_a_created_word() {
        for code in [Code::Variable, Code::Does(0)] {
            let mut dictionary = dictionary_with(&[]);
            dictionary.write_header("X", code, 0).unwrap();
            let data = dictionary.here();
            dictionary.allot(5).unwrap();
            dictionary.allot(-3).unwrap();
            assert_eq!(dictionary.here(), data + 2);
            dictionary.allot(-2).unwrap();
            assert_eq!(dictionary.here(), data);
        }
    }

    #[test]
    fn allot_rejects_going_below_the_data_of_a_created_word() {
        let mut dictionary = dictionary_with(&[]);
        dictionary.write_header("X", Code::Variable, 0).unwrap();
        dictionary.allot(2).unwrap();
        let here = dictionary.here();

        for bytes in [-3, isize::MIN] {
            assert!(matches!(
                dictionary.allot(bytes),
                Err(ForthError::AllotBelowData(b)) if b == bytes
            ));
        }
        assert_eq!(dictionary.here(), here);
    }

    #[test]
    fn allot_gives_nothing_back_after_other_words() {
        let mut empty = dictionary_with(&[1]);
        assert!(matches!(
            empty.allot(-1),
            Err(ForthError::AllotBelowData(-1))
        ));

        for code in [Code::Colon, Code::Constant, Code::Primitive(0)] {
            let mut dictionary = dictionary_with(&[]);
            dictionary.write_header("X", code, 0).unwrap();
            dictionary.comma(1).unwrap();
            let here = dictionary.here();
            assert!(matches!(
                dictionary.allot(-1),
                Err(ForthError::AllotBelowData(-1))
            ));
            assert_eq!(dictionary.here(), here);
        }
    }

    #[test]
    fn fetch_reads_the_first_cell() {
        let dictionary = dictionary_with(&[7]);
        assert_eq!(dictionary.fetch(0).unwrap(), 7);
    }

    #[test]
    fn fetch_rejects_negative_addresses() {
        let dictionary = dictionary_with(&[7]);
        assert_fetch_rejected(&dictionary, MINUS_ONE, ForthError::AddressOutOfBounds);
        assert_fetch_rejected(&dictionary, MINUS_ONE_CELL, ForthError::AddressOutOfBounds);
    }

    #[test]
    fn fetch_rejects_misaligned_addresses_inside_the_written_cells() {
        // The first Cell is written, so only the alignment check rejects `offset`. A
        // misaligned Cell in the last written Cell ends past HERE, which is checked first.
        let dictionary = dictionary_with(&[1, 2]);
        for offset in 1..CELL_SIZE {
            assert_fetch_rejected(&dictionary, offset, ForthError::MisalignedAddress);
            assert_fetch_rejected(
                &dictionary,
                CELL_SIZE + offset,
                ForthError::UnwrittenAddress,
            );
        }
    }

    #[test]
    fn fetch_rejects_addresses_after_a_restore() {
        let mut dictionary = dictionary_with(&[1]);
        let mark = dictionary.mark();
        let addr = dictionary.here();
        dictionary.comma(2).unwrap();
        assert_eq!(dictionary.fetch(addr).unwrap(), 2);

        dictionary.restore(mark);
        assert_fetch_rejected(&dictionary, addr, ForthError::UnwrittenAddress);
    }

    #[test]
    fn store_writes_the_first_cell() {
        let mut dictionary = dictionary_with(&[1, 2]);
        dictionary.store(0, 9).unwrap();
        assert_eq!(dictionary.fetch(0).unwrap(), 9);
    }

    #[test]
    fn store_writes_the_last_written_cell() {
        let mut dictionary = dictionary_with(&[1, 2]);
        dictionary.store(CELL_SIZE, 9).unwrap();
        assert_eq!(dictionary.fetch(CELL_SIZE).unwrap(), 9);
    }

    #[test]
    fn store_changes_only_the_target_cell() {
        let mut dictionary = dictionary_with(&[1, 2, 3]);
        dictionary.store(CELL_SIZE, 9).unwrap();
        assert_eq!(dictionary.fetch(0).unwrap(), 1);
        assert_eq!(dictionary.fetch(CELL_SIZE).unwrap(), 9);
        assert_eq!(dictionary.fetch(2 * CELL_SIZE).unwrap(), 3);
    }

    #[test]
    fn written_ranges_accept_empty_ranges_at_here_and_the_arena_end() {
        let mut dictionary = dictionary_with(&[1]);
        let here = dictionary.here();

        assert_eq!(dictionary.written_range(0, 0).unwrap(), 0..0);
        assert_eq!(
            dictionary.written_range(here, 0).unwrap(),
            CELL_SIZE..CELL_SIZE
        );
        dictionary
            .allot((TEST_CELLS * CELL_SIZE - CELL_SIZE) as isize)
            .unwrap();
        let end = end_of(&dictionary);
        assert_eq!(
            dictionary.written_range(end, 0).unwrap(),
            TEST_CELLS * CELL_SIZE..TEST_CELLS * CELL_SIZE
        );
        assert!(matches!(
            dictionary.written_range(end, 1),
            Err(ForthError::AddressOutOfBounds(addr)) if addr == end
        ));
    }

    #[test]
    fn written_ranges_reject_overflow_and_ranges_past_here() {
        let dictionary = dictionary_with(&[1, 2]);
        let here = dictionary.here();

        assert!(matches!(
            dictionary.written_range(0, Cell::MAX),
            Err(ForthError::UnwrittenAddress(0))
        ));
        assert!(matches!(
            dictionary.written_range(here - 1, 2),
            Err(ForthError::UnwrittenAddress(addr)) if addr == here - 1
        ));
        assert!(matches!(
            dictionary.written_range(MINUS_ONE, 0),
            Err(ForthError::AddressOutOfBounds(MINUS_ONE))
        ));
    }

    #[test]
    fn store_two_checks_both_cells_before_writing() {
        let mut dictionary = dictionary_with(&[1, 2, 3]);
        let here = dictionary.here();

        assert!(matches!(
            dictionary.store_two(here - CELL_SIZE, 7, 8),
            Err(ForthError::UnwrittenAddress(addr)) if addr == here - CELL_SIZE
        ));
        assert_eq!(dictionary.fetch(0).unwrap(), 1);
        assert_eq!(dictionary.fetch(CELL_SIZE).unwrap(), 2);
        assert_eq!(dictionary.fetch(2 * CELL_SIZE).unwrap(), 3);

        dictionary.store_two(0, 7, 8).unwrap();
        assert_eq!(dictionary.fetch_two(0).unwrap(), (7, 8));
        assert!(matches!(
            dictionary.fetch_two(1),
            Err(ForthError::MisalignedAddress(1))
        ));
    }

    #[test]
    fn two_cell_access_needs_both_whole_cells_to_be_written() {
        let mut dictionary = dictionary_with(&[1]);
        for _ in 0..CELL_SIZE / 2 {
            dictionary.char_comma(0).unwrap();
        }

        assert!(matches!(
            dictionary.fetch_two(0),
            Err(ForthError::UnwrittenAddress(0))
        ));
        assert!(matches!(
            dictionary.store_two(0, 7, 8),
            Err(ForthError::UnwrittenAddress(0))
        ));
        assert_eq!(dictionary.fetch(0).unwrap(), 1);
    }

    #[test]
    fn fill_and_move_allow_empty_ranges_and_overlapping_moves() {
        let mut dictionary = dictionary_with(&[]);
        for offset in 0..8 {
            dictionary.char_comma((offset + 1) as u8).unwrap();
        }
        let here = dictionary.here();
        let bytes_of = |dictionary: &Dictionary| -> Vec<u8> {
            (0..8)
                .map(|offset| dictionary.fetch_byte(offset).unwrap())
                .collect()
        };

        dictionary.fill(here, 0, 0xAA).unwrap();
        dictionary.move_bytes(here, here, 0).unwrap();
        dictionary.move_bytes(0, 1, 7).unwrap();
        assert_eq!(bytes_of(&dictionary), [1, 1, 2, 3, 4, 5, 6, 7]);

        dictionary.move_bytes(1, 0, 7).unwrap();
        assert_eq!(bytes_of(&dictionary), [1, 2, 3, 4, 5, 6, 7, 7]);
        dictionary.fill(1, 3, 0xAB).unwrap();
        assert_eq!(bytes_of(&dictionary), [1, 0xAB, 0xAB, 0xAB, 5, 6, 7, 7]);
    }

    #[test]
    fn fill_and_move_reject_invalid_ranges_without_writing() {
        let mut dictionary = Dictionary::new(TEST_CELLS).unwrap();
        dictionary.comma(0x0403_0201).unwrap();
        let here = dictionary.here();

        assert!(matches!(
            dictionary.fill(0, Cell::MAX, 0xFF),
            Err(ForthError::UnwrittenAddress(0))
        ));
        assert!(matches!(
            dictionary.move_bytes(0, here - 1, 2),
            Err(ForthError::UnwrittenAddress(addr)) if addr == here - 1
        ));
        assert_eq!(dictionary.fetch(0).unwrap(), 0x0403_0201);
    }

    #[test]
    fn store_rejects_negative_addresses() {
        let mut dictionary = dictionary_with(&[7]);
        assert_store_rejected(&mut dictionary, MINUS_ONE, ForthError::AddressOutOfBounds);
        assert_store_rejected(
            &mut dictionary,
            MINUS_ONE_CELL,
            ForthError::AddressOutOfBounds,
        );
    }

    #[test]
    fn store_rejects_here() {
        let mut dictionary = dictionary_with(&[7]);
        let here = dictionary.here();
        assert_store_rejected(&mut dictionary, here, ForthError::UnwrittenAddress);
    }

    #[test]
    fn store_rejects_misaligned_addresses_inside_the_written_cells() {
        // The first Cell is written, so only the alignment check rejects `offset`. A
        // misaligned Cell in the last written Cell ends past HERE, which is checked first.
        let mut dictionary = dictionary_with(&[1, 2]);
        for offset in 1..CELL_SIZE {
            assert_store_rejected(&mut dictionary, offset, ForthError::MisalignedAddress);
            assert_store_rejected(
                &mut dictionary,
                CELL_SIZE + offset,
                ForthError::UnwrittenAddress,
            );
        }
        assert_eq!(dictionary.fetch(0).unwrap(), 1);
        assert_eq!(dictionary.fetch(CELL_SIZE).unwrap(), 2);
    }

    #[test]
    fn store_rejects_addresses_after_a_restore() {
        let mut dictionary = dictionary_with(&[1]);
        let mark = dictionary.mark();
        let addr = dictionary.here();
        dictionary.comma(2).unwrap();
        dictionary.store(addr, 9).unwrap();

        dictionary.restore(mark);
        assert_store_rejected(&mut dictionary, addr, ForthError::UnwrittenAddress);
    }

    // `!` is deliberately not limited to Parameter Fields. This pins that down, using the
    // hidden flag because it has a visible effect without corrupting the VM. If `!` gets
    // restricted later, this test should change with it.
    #[test]
    fn store_can_change_a_word_header() {
        let mut vm = boot(io::sink(), VmConfig::new(TEST_CELLS)).unwrap();
        outer_interpreter(&mut vm, "CREATE X 7 ,").unwrap();
        let xt = vm.dictionary.find_word("X").unwrap();

        let addr = header_of(&vm.dictionary, xt) + LEN_FLAGS * CELL_SIZE;
        let flags = 1 | F_HIDDEN; // The length of the name stays the same
        outer_interpreter(&mut vm, &format!("{} {} !", flags, addr)).unwrap();

        outer_interpreter(&mut vm, &format!("{} @", addr)).unwrap();
        assert_eq!(vm.data_stack, vec![flags]);

        let error = outer_interpreter(&mut vm, "X").unwrap_err();
        assert!(matches!(error, ForthError::UnknownWord(name) if name == "X"));
    }

    #[test]
    fn find_word_stops_at_a_link_that_does_not_point_back() {
        let mut dictionary = Dictionary::new(TEST_CELLS).unwrap();
        dictionary.write_header("A", Code::Colon, 0).unwrap();
        let b = dictionary.write_header("B", Code::Colon, 0).unwrap();
        let link = header_of(&dictionary, b) + LFA * CELL_SIZE;

        for target in [header_of(&dictionary, b), Cell::MAX - 1] {
            dictionary.store(link, target).unwrap();
            assert_eq!(dictionary.find_word("B"), Some(b));
            assert_eq!(dictionary.find_word("A"), None);
            assert_eq!(dictionary.word_names(), ["B"]);
        }
    }

    #[test]
    fn find_word_skips_a_header_whose_name_is_outside_the_arena() {
        let mut dictionary = Dictionary::new(TEST_CELLS).unwrap();
        dictionary.write_header("A", Code::Colon, 0).unwrap();
        let b = dictionary.write_header("A", Code::Colon, 0).unwrap();
        let name = header_of(&dictionary, b) + NFA * CELL_SIZE;

        dictionary.store(name, Cell::MAX).unwrap();
        assert_eq!(dictionary.find_word("A"), Some(0));
        assert_eq!(dictionary.word_names(), ["A"]);
    }

    #[test]
    fn find_word_returns_the_xt_in_the_header() {
        let mut dictionary = Dictionary::new(TEST_CELLS).unwrap();
        let a = dictionary.write_header("A", Code::Colon, 0).unwrap();
        let xt = header_of(&dictionary, a) + XT * CELL_SIZE;

        dictionary.store(xt, 42).unwrap();
        assert_eq!(dictionary.find_word("A"), Some(42));
    }

    #[test]
    fn fetch_thread_checks_only_the_arena() {
        let mut dictionary = dictionary_with(&[1, 2]);
        let end = end_of(&dictionary);
        dictionary.restore(Mark { here: 0, words: 0 });

        assert_eq!(dictionary.fetch_thread(0), Some(1));
        assert_eq!(dictionary.fetch_thread(CELL_SIZE), Some(2));
        assert_eq!(dictionary.fetch_thread(end - CELL_SIZE), Some(0));
        assert_eq!(dictionary.fetch_thread(end - CELL_SIZE + 1), None);
        assert_eq!(dictionary.fetch_thread(end), None);
        assert_eq!(dictionary.fetch_thread(Cell::MAX), None);
        assert_eq!(dictionary.fetch_thread(MINUS_ONE_CELL), None);
    }

    #[test]
    fn dictionary_holds_exactly_the_requested_cells() {
        let mut dictionary = Dictionary::new(2).unwrap();
        dictionary.comma(1).unwrap();
        dictionary.comma(2).unwrap();
        assert!(matches!(
            dictionary.comma(3),
            Err(ForthError::DictionaryOverflow)
        ));
    }

    #[test]
    fn empty_dictionary_is_valid() {
        let mut dictionary = Dictionary::new(0).unwrap();
        assert!(matches!(
            dictionary.comma(1),
            Err(ForthError::DictionaryOverflow)
        ));
        assert!(matches!(
            dictionary.fetch(0),
            Err(ForthError::AddressOutOfBounds(0))
        ));
    }

    #[test]
    fn write_header_must_fit() {
        // This name takes exactly one Cell.
        let name = "X".repeat(CELL_SIZE);

        let mut dictionary = Dictionary::new(HEADER_CELLS + 1).unwrap();
        assert!(dictionary.write_header(&name, Code::Colon, 0).is_ok());

        let mut dictionary = Dictionary::new(HEADER_CELLS).unwrap();
        assert!(matches!(
            dictionary.write_header(&name, Code::Colon, 0),
            Err(ForthError::DictionaryOverflow)
        ));
        assert_eq!(dictionary.last_xt(), None);
    }

    #[test]
    fn find_word_skips_a_pending_word_until_it_is_finished() {
        let mut dictionary = Dictionary::new(TEST_CELLS).unwrap();
        let xt = dictionary
            .write_header("X", Code::Colon, F_PENDING)
            .unwrap();
        assert!(dictionary.find_word("X").is_none());

        dictionary.finish_last();
        assert_eq!(dictionary.find_word("X"), Some(xt));
    }

    #[test]
    fn find_word_skips_a_hidden_word() {
        let mut dictionary = Dictionary::new(TEST_CELLS).unwrap();
        dictionary.write_header("X", Code::Colon, F_HIDDEN).unwrap();
        assert!(dictionary.find_word("X").is_none());
    }

    #[test]
    fn find_word_finds_the_newest_definition() {
        let mut dictionary = Dictionary::new(TEST_CELLS).unwrap();
        dictionary.write_header("X", Code::Colon, 0).unwrap();
        let newest = dictionary.write_header("X", Code::Colon, 0).unwrap();
        assert_eq!(dictionary.find_word("X"), Some(newest));
    }

    #[test]
    fn word_names_are_sorted() {
        let mut dictionary = Dictionary::new(TEST_CELLS).unwrap();
        assert_eq!(dictionary.word_names(), Vec::<String>::new());

        dictionary.write_header("B", Code::Colon, 0).unwrap();
        dictionary.write_header("A", Code::Colon, 0).unwrap();
        assert_eq!(dictionary.word_names(), ["A", "B"]);
    }

    #[test]
    fn word_names_holds_a_redefined_word_once() {
        let mut dictionary = Dictionary::new(TEST_CELLS).unwrap();
        dictionary.write_header("X", Code::Colon, 0).unwrap();
        dictionary.write_header("X", Code::Colon, 0).unwrap();
        assert_eq!(dictionary.word_names(), ["X"]);
    }

    #[test]
    fn word_names_skips_what_find_word_skips() {
        let mut dictionary = Dictionary::new(TEST_CELLS).unwrap();
        dictionary.write_header("FOUND", Code::Colon, 0).unwrap();
        dictionary
            .write_header("HIDDEN", Code::Colon, F_HIDDEN)
            .unwrap();
        dictionary
            .write_header("PENDING", Code::Colon, F_PENDING)
            .unwrap();
        assert_eq!(dictionary.word_names(), ["FOUND"]);
    }

    #[test]
    fn word_names_skips_an_empty_name() {
        let mut dictionary = Dictionary::new(TEST_CELLS).unwrap();
        dictionary.write_header("", Code::Colon, 0).unwrap();
        assert_eq!(dictionary.word_names(), Vec::<String>::new());
    }

    #[test]
    fn word_names_replaces_invalid_utf8() {
        let mut dictionary = Dictionary::new(TEST_CELLS).unwrap();
        let xt = dictionary.write_header("X", Code::Colon, 0).unwrap();
        let name = dictionary
            .header_cell(header_of(&dictionary, xt), NFA)
            .unwrap();
        dictionary.store_byte(name, 0xff).unwrap();
        assert_eq!(dictionary.word_names(), ["\u{fffd}"]);
    }

    #[test]
    fn finish_last_keeps_the_hidden_flag() {
        let mut dictionary = Dictionary::new(TEST_CELLS).unwrap();
        let xt = dictionary
            .write_header("X", Code::Colon, F_HIDDEN | F_PENDING)
            .unwrap();

        dictionary.finish_last();
        assert!(!has_flag(&dictionary, xt, F_PENDING));
        assert!(has_flag(&dictionary, xt, F_HIDDEN));
    }

    #[test]
    fn changing_the_flags_of_an_empty_dictionary_does_nothing() {
        let mut dictionary = Dictionary::new(TEST_CELLS).unwrap();
        dictionary.finish_last();
        dictionary.mark_last_immediate();
        dictionary.set_last_code(Code::Colon);
        assert_eq!(dictionary.mark(), Mark { here: 0, words: 0 });
        assert!(!dictionary.is_last_created());
    }

    #[test]
    fn execution_tokens_include_finished_and_hidden_words() {
        let mut dictionary = Dictionary::new(TEST_CELLS).unwrap();
        let named = dictionary.write_header("X", Code::Colon, 0).unwrap();
        let hidden = dictionary.write_header("", Code::Colon, F_HIDDEN).unwrap();
        assert!(dictionary.is_execution_token(named));
        assert!(dictionary.is_execution_token(hidden));
    }

    #[test]
    fn execution_tokens_exclude_compile_only_words() {
        let mut dictionary = Dictionary::new(TEST_CELLS).unwrap();
        let xt = dictionary
            .write_header("X", Code::Colon, F_HIDDEN | F_COMPILE_ONLY)
            .unwrap();
        assert!(has_flag(&dictionary, xt, F_COMPILE_ONLY));
        assert!(!dictionary.is_execution_token(xt));

        // The flag stays when the definition ends.
        let pending = dictionary
            .write_header("Y", Code::Colon, F_COMPILE_ONLY | F_PENDING)
            .unwrap();
        dictionary.finish_last();
        assert!(has_flag(&dictionary, pending, F_COMPILE_ONLY));
        assert!(!dictionary.is_execution_token(pending));

        let plain = dictionary.write_header("Z", Code::Colon, 0).unwrap();
        assert!(!has_flag(&dictionary, plain, F_COMPILE_ONLY));
        assert!(dictionary.is_execution_token(plain));
    }

    #[test]
    fn execution_tokens_exclude_pending_words() {
        let mut dictionary = Dictionary::new(TEST_CELLS).unwrap();
        let xt = dictionary
            .write_header("X", Code::Colon, F_PENDING)
            .unwrap();
        assert!(!dictionary.is_execution_token(xt));

        dictionary.finish_last();
        assert!(dictionary.is_execution_token(xt));
    }

    #[test]
    fn execution_tokens_exclude_numbers_past_the_table() {
        let mut dictionary = Dictionary::new(TEST_CELLS).unwrap();
        let xt = dictionary.write_header("X", Code::Colon, 0).unwrap();
        assert!(dictionary.is_execution_token(xt));
        assert!(!dictionary.is_execution_token(xt + 1));
        assert!(!dictionary.is_execution_token(Cell::MAX));
        assert_eq!(dictionary.entry(xt + 1), None);
    }

    #[test]
    fn execution_tokens_exclude_words_after_a_restore() {
        let mut dictionary = Dictionary::new(TEST_CELLS).unwrap();
        let mark = dictionary.mark();
        let xt = dictionary.write_header("X", Code::Colon, 0).unwrap();

        dictionary.restore(mark);
        assert!(!dictionary.is_execution_token(xt));
        assert_eq!(dictionary.entry(xt), None);
        assert_eq!(dictionary.last_xt(), None);
    }

    #[test]
    fn execution_tokens_exclude_a_word_whose_header_was_moved_out_of_the_arena() {
        let mut dictionary = Dictionary::new(TEST_CELLS).unwrap();
        let xt = dictionary.write_header("X", Code::Colon, 0).unwrap();
        dictionary.words[xt].header = Cell::MAX;
        assert!(!dictionary.is_execution_token(xt));
        assert!(!dictionary.is_immediate(xt));
    }

    #[test]
    fn mark_last_immediate_only_marks_the_newest_word() {
        let mut dictionary = Dictionary::new(TEST_CELLS).unwrap();
        let older = dictionary.write_header("A", Code::Colon, 0).unwrap();
        let newest = dictionary.write_header("B", Code::Colon, 0).unwrap();

        dictionary.mark_last_immediate();
        assert!(dictionary.is_immediate(newest));
        assert!(!dictionary.is_immediate(older));
    }

    #[test]
    fn mark_last_immediate_keeps_other_flags() {
        let mut dictionary = Dictionary::new(TEST_CELLS).unwrap();
        let flags = F_HIDDEN | F_PENDING | F_COMPILE_ONLY;
        let xt = dictionary.write_header("X", Code::Colon, flags).unwrap();

        dictionary.mark_last_immediate();
        assert_eq!(dictionary.flags(xt), Some(flags | F_IMMEDIATE));
    }

    #[test]
    fn mark_last_immediate_can_be_repeated() {
        let mut dictionary = Dictionary::new(TEST_CELLS).unwrap();
        let xt = dictionary.write_header("X", Code::Colon, 0).unwrap();

        dictionary.mark_last_immediate();
        dictionary.mark_last_immediate();
        assert_eq!(len_flags_of(&dictionary, xt), 1 | F_IMMEDIATE);
    }

    #[test]
    fn the_body_directly_follows_the_header() {
        let mut dictionary = Dictionary::new(TEST_CELLS).unwrap();
        let xt = dictionary.write_header("X", Code::Colon, 0).unwrap();
        let entry = dictionary.entry(xt).unwrap();
        assert_eq!(
            entry.body,
            header_of(&dictionary, xt) + HEADER_CELLS * CELL_SIZE
        );
        assert_eq!(entry.body, dictionary.here());
    }

    #[test]
    fn only_created_words_can_change_their_action() {
        let mut dictionary = Dictionary::new(TEST_CELLS).unwrap();
        for (code, created) in [
            (Code::Variable, true),
            (Code::Does(0), true),
            (Code::Colon, false),
            (Code::Constant, false),
            (Code::Primitive(0), false),
        ] {
            dictionary.write_header("X", code, 0).unwrap();
            assert_eq!(dictionary.is_last_created(), created, "{:?}", code);
        }
    }

    #[test]
    fn set_last_code_changes_only_the_newest_word() {
        let mut dictionary = Dictionary::new(TEST_CELLS).unwrap();
        let older = dictionary.write_header("A", Code::Variable, 0).unwrap();
        let newest = dictionary.write_header("B", Code::Variable, 0).unwrap();

        dictionary.set_last_code(Code::Does(8));
        assert_eq!(dictionary.entry(newest).unwrap().code, Code::Does(8));
        assert_eq!(dictionary.entry(older).unwrap().code, Code::Variable);
    }

    #[test]
    fn flags_use_their_specified_bits() {
        assert_eq!(F_IMMEDIATE, 1 << 8);
        assert_eq!(F_HIDDEN, 1 << 9);
        assert_eq!(F_PENDING, 1 << 10);
        assert_eq!(F_COMPILE_ONLY, 1 << 11);
    }

    #[test]
    fn write_header_combines_the_length_and_the_flags() {
        let mut dictionary = Dictionary::new(TEST_CELLS).unwrap();
        let flags = F_HIDDEN | F_COMPILE_ONLY;
        let xt = dictionary.write_header("ABC", Code::Colon, flags).unwrap();
        assert_eq!(len_flags_of(&dictionary, xt), 3 | flags);
    }

    #[test]
    fn changing_flags_keeps_the_length() {
        let mut dictionary = Dictionary::new(TEST_CELLS).unwrap();
        let xt = dictionary
            .write_header("ABC", Code::Colon, F_PENDING)
            .unwrap();

        dictionary.finish_last();
        assert_eq!(len_flags_of(&dictionary, xt), 3);

        dictionary.mark_last_immediate();
        assert_eq!(len_flags_of(&dictionary, xt), 3 | F_IMMEDIATE);
    }

    #[test]
    fn find_word_compares_the_length_without_the_flags() {
        let longest = "X".repeat(LEN_MASK);
        let mut dictionary = Dictionary::new(TEST_CELLS).unwrap();
        let flags = F_IMMEDIATE | F_COMPILE_ONLY;
        let short = dictionary.write_header("A", Code::Colon, flags).unwrap();
        let long = dictionary
            .write_header(&longest, Code::Colon, flags)
            .unwrap();

        assert_eq!(dictionary.find_word("A"), Some(short));
        assert_eq!(dictionary.find_word(&longest), Some(long));
        assert_eq!(dictionary.find_word(&longest[1..]), None);
    }

    #[test]
    fn write_header_rejects_a_name_that_does_not_fit_the_length_bits() {
        let name = "X".repeat(LEN_MASK + 1);
        let mut dictionary = Dictionary::new(TEST_CELLS).unwrap();
        let mark = dictionary.mark();

        let result = dictionary.write_header(&name, Code::Colon, 0);
        assert!(matches!(result, Err(ForthError::NameTooLong(256))));
        assert_eq!(dictionary.mark(), mark);
    }
}
