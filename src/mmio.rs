// Copyright (c) 2026 M.W.R. Hoiting

//! Host permissions, active registrations, and volatile MMIO transfers.

use std::mem::size_of;
use std::ptr;

use crate::{Cell, ForthError};

/// Permission to read an MMIO range.
pub const MMIO_READ: Cell = 1;
/// Permission to write an MMIO range.
pub const MMIO_WRITE: Cell = 2;
/// Permission for 8-bit accesses.
pub const MMIO_WIDTH_8: Cell = 1;
/// Permission for 16-bit accesses.
pub const MMIO_WIDTH_16: Cell = 2;
/// Permission for 32-bit accesses.
pub const MMIO_WIDTH_32: Cell = 4;
/// Permission for 64-bit accesses, available on 64-bit targets.
#[cfg(target_pointer_width = "64")]
pub const MMIO_WIDTH_64: Cell = 8;

/// A host permission or active MMIO registration. Masks combine with `|` in Rust and `OR`
/// in Forth. Addresses name already accessible memory; this does not map physical memory.
#[derive(Clone, Copy, Debug)]
pub struct MmioRegion {
    /// Base pointer, preserving provenance for host-owned test buffers as well as registers.
    pub address: *mut u8,
    /// Size in bytes.
    pub bytes: usize,
    /// Combination of [`MMIO_READ`] and [`MMIO_WRITE`].
    pub rights: Cell,
    /// Combination of the `MMIO_WIDTH_*` flags.
    pub widths: Cell,
}

impl MmioRegion {
    /// Returns the exclusive end after checking size and masks.
    fn end(self) -> Result<Cell, ForthError> {
        if self.rights == 0
            || self.rights & !(MMIO_READ | MMIO_WRITE) != 0
            || self.widths == 0
            || self.widths & !(size_of::<Cell>() * 2 - 1) != 0
        {
            return Err(ForthError::MmioInvalidFlags);
        }
        if self.bytes == 0 {
            return Err(ForthError::MmioInvalidRegion);
        }
        (self.address as Cell)
            .checked_add(self.bytes)
            .ok_or(ForthError::MmioInvalidRegion)
    }

    /// Whether the whole range lies in this region; an empty range must start inside it.
    fn contains(self, addr: Cell, bytes: usize) -> bool {
        addr >= self.address as Cell
            && addr < self.address as Cell + self.bytes
            && addr
                .checked_add(bytes)
                .is_some_and(|end| end <= self.address as Cell + self.bytes)
    }
}

/// A validated access, retaining the host pointer's provenance.
#[derive(Clone, Copy)]
pub(crate) struct Access {
    /// Pointer to the first register.
    pointer: *mut u8,
    /// Access width in bytes.
    width: usize,
}

/// The VM's permissions and active registrations. Host permissions lock after first use.
#[derive(Default)]
pub(crate) struct Mmio {
    /// Host-authorized upper bounds.
    hosts: Vec<MmioRegion>,
    /// Active registrations and their unique IDs.
    active: Vec<(Cell, MmioRegion)>,
    /// Last issued ID, never reset on unregister.
    last_id: Cell,
}

impl Mmio {
    /// Adds host permission before any successful Forth registration.
    pub(crate) fn allow(&mut self, region: MmioRegion) -> Result<(), ForthError> {
        if self.last_id != 0 {
            return Err(ForthError::MmioHostLocked);
        }
        let end = region.end()?;
        if self.hosts.iter().any(|r| {
            (region.address as Cell) < r.address as Cell + r.bytes && (r.address as Cell) < end
        }) {
            return Err(ForthError::MmioOverlap);
        }
        self.hosts
            .try_reserve(1)
            .map_err(|_| ForthError::MmioAllocationFailed)?;
        self.hosts.push(region);
        Ok(())
    }

    /// Activates a subset of a host permission, with no overlap with another registration.
    pub(crate) fn register(
        &mut self,
        addr: Cell,
        bytes: usize,
        rights: Cell,
        widths: Cell,
    ) -> Result<Cell, ForthError> {
        // This pointer is only used for address validation; actual accesses derive from the host.
        let requested = MmioRegion {
            address: ptr::without_provenance_mut(addr),
            bytes,
            rights,
            widths,
        };
        let end = requested.end()?;
        let host = self
            .hosts
            .iter()
            .find(|r| r.contains(addr, bytes))
            .ok_or(ForthError::MmioOutOfBounds(addr))?;
        if rights & !host.rights != 0 || widths & !host.widths != 0 {
            return Err(ForthError::MmioPermissionDenied);
        }
        if self
            .active
            .iter()
            .any(|(_, r)| addr < r.address as Cell + r.bytes && (r.address as Cell) < end)
        {
            return Err(ForthError::MmioOverlap);
        }
        let region = MmioRegion {
            address: host.address.wrapping_add(addr - host.address as Cell),
            ..requested
        };
        let id = self
            .last_id
            .checked_add(1)
            .ok_or(ForthError::MmioIdsExhausted)?;
        self.active
            .try_reserve(1)
            .map_err(|_| ForthError::MmioAllocationFailed)?;
        self.active.push((id, region));
        self.last_id = id;
        Ok(id)
    }

    /// Removes a registration without recycling its ID.
    pub(crate) fn unregister(&mut self, id: Cell) -> Result<(), ForthError> {
        let index = self
            .active
            .iter()
            .position(|(key, _)| *key == id)
            .ok_or(ForthError::MmioUnknownId(id))?;
        self.active.swap_remove(index);
        Ok(())
    }

    /// Validates width and length independently of addresses.
    pub(crate) fn dimensions(width: Cell, bytes: Cell) -> Result<(), ForthError> {
        if !width.is_power_of_two() || width > size_of::<Cell>() {
            return Err(ForthError::MmioInvalidWidth(width));
        }
        if !bytes.is_multiple_of(width) {
            return Err(ForthError::MmioInvalidLength(bytes));
        }
        Ok(())
    }

    /// Checks one complete transfer, returning a pointer derived from the host permission.
    pub(crate) fn access(
        &self,
        addr: Cell,
        bytes: Cell,
        width: Cell,
        increment: bool,
        write: bool,
    ) -> Result<Access, ForthError> {
        Self::dimensions(width, bytes)?;
        if !addr.is_multiple_of(width) {
            return Err(ForthError::MisalignedAddress(addr));
        }
        let span = if increment || bytes == 0 {
            bytes
        } else {
            width
        };
        let region = self
            .active
            .iter()
            .map(|(_, r)| r)
            .find(|r| r.contains(addr, span))
            .ok_or(ForthError::MmioOutOfBounds(addr))?;
        let right = if write { MMIO_WRITE } else { MMIO_READ };
        if region.rights & right == 0 || region.widths & width == 0 {
            return Err(ForthError::MmioPermissionDenied);
        }
        Ok(Access {
            pointer: region.address.wrapping_add(addr - region.address as Cell),
            width,
        })
    }
}

/// An MMIO backend; the transfer loop can be tested without real device accesses.
trait Bus {
    /// Reads one element of a previously validated access.
    fn read(&mut self, access: Access) -> Cell;
    /// Writes one element of a previously validated access.
    fn write(&mut self, access: Access, value: Cell);
}

/// The production backend, performing exactly one volatile operation per element.
struct Volatile;

impl Bus for Volatile {
    fn read(&mut self, access: Access) -> Cell {
        // SAFETY: access was validated against a host permission whose unsafe contract
        // guarantees alignment, validity, provenance, lifetime and legal device accesses.
        unsafe {
            match access.width {
                1 => ptr::read_volatile(access.pointer) as Cell,
                2 => ptr::read_volatile(access.pointer.cast::<u16>()) as Cell,
                4 => ptr::read_volatile(access.pointer.cast::<u32>()) as Cell,
                #[cfg(target_pointer_width = "64")]
                8 => ptr::read_volatile(access.pointer.cast::<Cell>()),
                _ => unreachable!("validated width"),
            }
        }
    }

    fn write(&mut self, access: Access, value: Cell) {
        // SAFETY: the validated access and host contract hold as for `read` above.
        unsafe {
            match access.width {
                1 => ptr::write_volatile(access.pointer, value as u8),
                2 => ptr::write_volatile(access.pointer.cast::<u16>(), value as u16),
                4 => ptr::write_volatile(access.pointer.cast::<u32>(), value as u32),
                #[cfg(target_pointer_width = "64")]
                8 => ptr::write_volatile(access.pointer.cast::<Cell>(), value),
                _ => unreachable!("validated width"),
            }
        }
    }
}

/// Reads a validated register.
pub(crate) fn read(access: Access) -> Cell {
    Volatile.read(access)
}
/// Writes a validated register.
pub(crate) fn write(access: Access, value: Cell) {
    Volatile.write(access, value);
}

/// Transfers a validated dictionary buffer with native byte order and no dictionary alignment requirement.
pub(crate) fn transfer(access: Access, buffer: &mut [u8], increment: bool, write: bool) {
    transfer_with(&mut Volatile, access, buffer, increment, write);
}

/// Shared transfer loop; validation has completed before this runs.
fn transfer_with(
    bus: &mut impl Bus,
    access: Access,
    buffer: &mut [u8],
    increment: bool,
    write: bool,
) {
    for (index, chunk) in buffer.chunks_exact_mut(access.width).enumerate() {
        let current = Access {
            pointer: access
                .pointer
                .wrapping_add(if increment { index * access.width } else { 0 }),
            ..access
        };
        if write {
            let mut bytes = [0u8; size_of::<Cell>()];
            let offset = if cfg!(target_endian = "big") {
                bytes.len() - chunk.len()
            } else {
                0
            };
            bytes[offset..offset + chunk.len()].copy_from_slice(chunk);
            bus.write(current, Cell::from_ne_bytes(bytes));
        } else {
            let bytes = bus.read(current).to_ne_bytes();
            let offset = if cfg!(target_endian = "big") {
                bytes.len() - chunk.len()
            } else {
                0
            };
            chunk.copy_from_slice(&bytes[offset..offset + chunk.len()]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn permission(addr: Cell, bytes: Cell, rights: Cell, widths: Cell) -> MmioRegion {
        MmioRegion {
            address: ptr::without_provenance_mut(addr),
            bytes,
            rights,
            widths,
        }
    }

    #[test]
    fn registry_enforces_host_bounds_permissions_and_lifecycle() {
        let mut mmio = Mmio::default();
        assert!(matches!(
            mmio.register(16, 8, 1, 1),
            Err(ForthError::MmioOutOfBounds(16))
        ));
        mmio.allow(permission(16, 32, 3, 7)).unwrap();
        assert!(matches!(
            mmio.allow(permission(32, 32, 1, 1)),
            Err(ForthError::MmioOverlap)
        ));
        assert!(matches!(
            mmio.register(8, 16, 1, 1),
            Err(ForthError::MmioOutOfBounds(8))
        ));
        assert!(matches!(
            mmio.register(40, 16, 1, 1),
            Err(ForthError::MmioOutOfBounds(40))
        ));
        let id = mmio.register(16, 8, MMIO_READ, MMIO_WIDTH_32).unwrap();
        assert_eq!(id, 1);
        assert!(matches!(
            mmio.register(16, 8, 1, 1),
            Err(ForthError::MmioOverlap)
        ));
        assert!(matches!(
            mmio.access(16, 4, 4, true, true),
            Err(ForthError::MmioPermissionDenied)
        ));
        assert!(matches!(
            mmio.access(16, 1, 1, true, false),
            Err(ForthError::MmioPermissionDenied)
        ));
        assert!(matches!(
            mmio.access(20, 8, 4, true, false),
            Err(ForthError::MmioOutOfBounds(20))
        ));
        assert!(mmio.access(20, 64, 4, false, false).is_ok());
        assert!(matches!(
            mmio.access(17, 4, 4, true, false),
            Err(ForthError::MisalignedAddress(17))
        ));
        mmio.unregister(id).unwrap();
        assert!(matches!(
            mmio.access(16, 4, 4, true, false),
            Err(ForthError::MmioOutOfBounds(16))
        ));
        assert!(matches!(
            mmio.unregister(id),
            Err(ForthError::MmioUnknownId(1))
        ));
        assert!(matches!(
            mmio.allow(permission(64, 4, 1, 1)),
            Err(ForthError::MmioHostLocked)
        ));
        assert_eq!(mmio.register(16, 8, 1, 1).unwrap(), 2);
    }

    #[test]
    fn invalid_ranges_masks_dimensions_and_id_exhaustion_are_rejected() {
        let mut mmio = Mmio::default();
        for (addr, bytes) in [(16, 0), (Cell::MAX - 3, 8)] {
            assert!(matches!(
                mmio.allow(permission(addr, bytes, 1, 1)),
                Err(ForthError::MmioInvalidRegion)
            ));
        }
        for (rights, widths) in [(0, 1), (4, 1), (1, 0), (1, 16)] {
            assert!(matches!(
                mmio.allow(permission(16, 8, rights, widths)),
                Err(ForthError::MmioInvalidFlags)
            ));
        }
        for width in [0, 3, size_of::<Cell>() * 2] {
            assert!(matches!(
                Mmio::dimensions(width, 0),
                Err(ForthError::MmioInvalidWidth(_))
            ));
        }
        assert!(matches!(
            Mmio::dimensions(4, 3),
            Err(ForthError::MmioInvalidLength(3))
        ));
        mmio.allow(permission(16, 8, 1, 1)).unwrap();
        assert!(matches!(
            mmio.register(16, 8, 3, 1),
            Err(ForthError::MmioPermissionDenied)
        ));
        assert!(matches!(
            mmio.register(16, 8, 1, 2),
            Err(ForthError::MmioPermissionDenied)
        ));
        mmio.last_id = Cell::MAX;
        assert!(matches!(
            mmio.register(16, 8, 1, 1),
            Err(ForthError::MmioIdsExhausted)
        ));
        assert!(mmio.active.is_empty());
    }

    #[derive(Default)]
    struct RecordingBus {
        events: Vec<(bool, Cell, usize, Cell)>,
    }
    impl Bus for RecordingBus {
        fn read(&mut self, a: Access) -> Cell {
            self.events.push((false, a.pointer as Cell, a.width, 0));
            self.events.len()
        }
        fn write(&mut self, a: Access, value: Cell) {
            self.events.push((true, a.pointer as Cell, a.width, value));
        }
    }

    #[test]
    fn transfer_records_exact_accesses_for_both_directions_and_address_modes() {
        for width in [1, 2, 4, size_of::<Cell>()] {
            for increment in [false, true] {
                let access = Access {
                    pointer: ptr::without_provenance_mut(16),
                    width,
                };
                let mut buffer = vec![0; width * 3];
                let mut bus = RecordingBus::default();
                transfer_with(&mut bus, access, &mut buffer, increment, false);
                let addresses: Vec<_> = (0..3)
                    .map(|i| 16 + if increment { i * width } else { 0 })
                    .collect();
                assert_eq!(
                    bus.events,
                    addresses
                        .iter()
                        .map(|a| (false, *a, width, 0))
                        .collect::<Vec<_>>()
                );
                let mut expected = Vec::new();
                for value in 1..=3usize {
                    let bytes = value.to_ne_bytes();
                    let offset = if cfg!(target_endian = "big") {
                        bytes.len() - width
                    } else {
                        0
                    };
                    expected.extend_from_slice(&bytes[offset..offset + width]);
                }
                assert_eq!(buffer, expected);
                bus.events.clear();
                transfer_with(&mut bus, access, &mut buffer, increment, true);
                assert_eq!(
                    bus.events,
                    addresses
                        .iter()
                        .enumerate()
                        .map(|(i, a)| (true, *a, width, i + 1))
                        .collect::<Vec<_>>()
                );
                bus.events.clear();
                transfer_with(&mut bus, access, &mut [], increment, false);
                assert!(bus.events.is_empty());
            }
        }
    }

    #[test]
    fn zero_transfers_still_validate_permissions_and_start_address() {
        let mut mmio = Mmio::default();
        mmio.allow(permission(16, 8, 1, 4)).unwrap();
        mmio.register(16, 8, 1, 4).unwrap();
        assert!(mmio.access(16, 0, 4, true, false).is_ok());
        assert!(matches!(
            mmio.access(24, 0, 4, true, false),
            Err(ForthError::MmioOutOfBounds(24))
        ));
        assert!(matches!(
            mmio.access(16, 0, 4, false, true),
            Err(ForthError::MmioPermissionDenied)
        ));
        assert!(matches!(
            mmio.access(Cell::MAX - 3, 8, 4, true, false),
            Err(ForthError::MmioOutOfBounds(_))
        ));
    }
}
