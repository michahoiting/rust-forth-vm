// Copyright (c) 2026 M.W.R. Hoiting

//! Words for registering MMIO ranges, accessing registers, and transferring blocks.

use crate::cell::Cell;
use crate::error::ForthError;
use crate::vm::Vm;

/// `MMIO@ ( addr width -- x )`: Reads one authorized register, zero-extending its value.
pub(super) unsafe fn fetch(vm: &mut Vm) {
    let Some([addr, width]) = take_arguments(vm) else {
        return;
    };
    let result = vm.mmio.access(addr, width, width, true, false);
    if let Some(access) = vm.check(result) {
        vm.push(crate::mmio::read(access));
    }
}

/// `MMIO! ( x addr width -- )`: Writes the low bits of a Cell to one authorized register.
pub(super) unsafe fn store(vm: &mut Vm) {
    let Some([value, addr, width]) = take_arguments(vm) else {
        return;
    };
    let result = vm.mmio.access(addr, width, width, true, true);
    if let Some(access) = vm.check(result) {
        crate::mmio::write(access, value);
    }
}

/// Takes all arguments in stack order, leaving the stack unchanged on underflow.
fn take_arguments<const N: usize>(vm: &mut Vm) -> Option<[Cell; N]> {
    if !vm.require(N) {
        return None;
    }
    let start = vm.data_stack.len() - N;
    let mut arguments = [0; N];
    arguments.copy_from_slice(&vm.data_stack[start..]);
    vm.data_stack.truncate(start);
    Some(arguments)
}

/// `MMIO-REGISTER ( addr bytes rights widths -- id )`: Activates a permitted host subrange.
pub(super) unsafe fn register(vm: &mut Vm) {
    let Some([addr, bytes, rights, widths]) = take_arguments(vm) else {
        return;
    };
    if vm.dictionary.overlaps_arena(addr, bytes) {
        vm.set_error(ForthError::MmioInvalidRegion);
        return;
    }
    let result = vm.mmio.register(addr, bytes, rights, widths);
    if let Some(id) = vm.check(result) {
        vm.push(id);
    }
}

/// `MMIO-UNREGISTER ( id -- )`: Revokes an active registration without recycling its ID.
pub(super) unsafe fn unregister(vm: &mut Vm) {
    let Some([id]) = take_arguments(vm) else {
        return;
    };
    let result = vm.mmio.unregister(id);
    vm.check(result);
}

/// `MMIO>DICT ( mmio-addr dict-addr bytes width increment -- )`: Reads registers into a
/// reserved dictionary buffer. A zero increment flag reads the same FIFO register repeatedly.
pub(super) unsafe fn to_dict(vm: &mut Vm) {
    transfer_block(vm, false);
}

/// `DICT>MMIO ( dict-addr mmio-addr bytes width increment -- )`: Writes a reserved dictionary
/// buffer to registers. A zero increment flag writes the same FIFO register repeatedly.
pub(super) unsafe fn from_dict(vm: &mut Vm) {
    transfer_block(vm, true);
}

/// Validates a complete block before its first access; dictionary bytes may be unaligned.
fn transfer_block(vm: &mut Vm, write: bool) {
    let Some([source, destination, bytes, width, increment]) = take_arguments(vm) else {
        return;
    };
    let (addr, dict) = if write {
        (destination, source)
    } else {
        (source, destination)
    };
    let result = vm.mmio.access(addr, bytes, width, increment != 0, write);
    let Some(access) = vm.check(result) else {
        return;
    };
    match vm.dictionary.bytes_mut(dict, bytes) {
        Ok(buffer) => crate::mmio::transfer(access, buffer, increment != 0, write),
        Err(error) => vm.set_error(error),
    }
}

/// `MMIO_READ ( -- mask )`: Pushes its MMIO permission bit.
pub(super) unsafe fn read_flag(vm: &mut Vm) {
    vm.push(crate::mmio::MMIO_READ);
}

/// `MMIO_WRITE ( -- mask )`: Pushes its MMIO permission bit.
pub(super) unsafe fn write_flag(vm: &mut Vm) {
    vm.push(crate::mmio::MMIO_WRITE);
}

/// `MMIO_WIDTH_8 ( -- mask )`: Pushes its MMIO permission bit.
pub(super) unsafe fn width_8_flag(vm: &mut Vm) {
    vm.push(crate::mmio::MMIO_WIDTH_8);
}

/// `MMIO_WIDTH_16 ( -- mask )`: Pushes its MMIO permission bit.
pub(super) unsafe fn width_16_flag(vm: &mut Vm) {
    vm.push(crate::mmio::MMIO_WIDTH_16);
}

/// `MMIO_WIDTH_32 ( -- mask )`: Pushes its MMIO permission bit.
pub(super) unsafe fn width_32_flag(vm: &mut Vm) {
    vm.push(crate::mmio::MMIO_WIDTH_32);
}

/// `MMIO_WIDTH_64 ( -- mask )`: Pushes its MMIO permission bit.
#[cfg(target_pointer_width = "64")]
pub(super) unsafe fn width_64_flag(vm: &mut Vm) {
    vm.push(crate::mmio::MMIO_WIDTH_64);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem;

    #[test]
    fn words_preserve_exact_stack_on_underflow() {
        // SAFETY: VMs are only driven by primitives under their normal contract.
        unsafe {
            for (primitive, count) in [
                (fetch as crate::vm::PrimitiveFn, 2),
                (store, 3),
                (register, 4),
                (unregister, 1),
                (to_dict, 5),
                (from_dict, 5),
            ] {
                for available in 0..count {
                    let mut vm = crate::boot(std::io::sink(), crate::VmConfig::new(4096)).unwrap();
                    vm.data_stack.extend(0..available);
                    let before = vm.data_stack.clone();
                    primitive(&mut vm);
                    assert_eq!(vm.data_stack, before);
                    assert!(matches!(vm.error, Some(ForthError::DataStackUnderflow)));
                }
            }
        }
    }

    #[test]
    fn register_fetch_store_unregister_have_exact_stack_effects() {
        let mut storage = Box::new(0usize);
        let pointer = (&mut *storage) as *mut Cell;
        let addr = pointer as Cell;
        // SAFETY: the boxed Cell outlives the VM, with no conflicting accesses.
        unsafe {
            let mut vm = crate::boot(std::io::sink(), crate::VmConfig::new(4096)).unwrap();
            vm.allow_mmio(crate::MmioRegion {
                address: pointer.cast(),
                bytes: mem::size_of::<Cell>(),
                rights: 3,
                widths: 1,
            })
            .unwrap();
            vm.data_stack
                .extend([77, addr, mem::size_of::<Cell>(), 3, 1]);
            register(&mut vm);
            assert_eq!(vm.data_stack, [77, 1]);
            vm.data_stack.extend([0x1ab, addr, 1]);
            store(&mut vm);
            assert_eq!(vm.data_stack, [77, 1]);
            vm.data_stack.extend([addr, 1]);
            fetch(&mut vm);
            assert_eq!(vm.data_stack, [77, 1, 0xab]);
            vm.data_stack.pop();
            unregister(&mut vm);
            assert_eq!(vm.data_stack, [77]);
            assert!(vm.error.is_none());
        }
    }
}
