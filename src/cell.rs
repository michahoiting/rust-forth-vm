// --- THE FOUNDATION: THE FORTH CELL ---

/// A Cell is the universal basic unit of Forth (64-bit on a 64-bit CPU).
/// It blindly holds data, flags, offsets, or raw memory pointers.
pub type Cell = usize;
