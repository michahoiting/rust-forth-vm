// Copyright (c) 2026 M.W.R. Hoiting

//! The foundation: the Forth Cell.

/// A Cell is the universal basic unit of Forth (64-bit on a 64-bit CPU).
/// It blindly holds data, flags, offsets, or raw memory pointers.
///
/// Forth has no types, so a Cell means whatever the word using it says. Negative numbers
/// are stored in two's complement, and `.` prints a Cell as a signed number.
pub type Cell = usize;
