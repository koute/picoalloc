use crate::Size;

#[cold]
pub fn abort() -> ! {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        core::arch::asm!("ud2", options(noreturn, nostack));
    }

    #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
    unsafe {
        core::arch::asm!("unimp", options(noreturn, nostack));
    }

    #[cfg(target_family = "wasm")]
    {
        core::arch::wasm32::unreachable();
    }

    #[cfg(not(any(target_arch = "x86_64", target_arch = "riscv32", target_arch = "riscv64", target_family = "wasm")))]
    unreachable!();
}

pub trait Env {
    unsafe fn allocate_address_space(&mut self, size: Size) -> *mut u8;
    unsafe fn expand_memory_until(&mut self, end: *mut u8) -> bool;
    unsafe fn free_address_space(&mut self, base: *mut u8, size: Size);
}

#[repr(align(32))]
pub struct Array<const SIZE: usize>(pub [u8; SIZE]);

#[repr(transparent)]
pub struct ArrayPointer<const SIZE: usize>(pub *mut Array<SIZE>);

impl<const SIZE: usize> Env for ArrayPointer<SIZE> {
    unsafe fn allocate_address_space(&mut self, _size: Size) -> *mut u8 {
        self.0.cast()
    }

    unsafe fn expand_memory_until(&mut self, end: *mut u8) -> bool {
        (end.addr() - self.0.addr()) <= SIZE
    }

    unsafe fn free_address_space(&mut self, _base: *mut u8, _size: Size) {}
}

pub struct System;

#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
mod linux;

#[cfg(all(target_env = "polkavm", not(feature = "corevm")))]
mod polkavm;

#[cfg(all(target_env = "polkavm", feature = "corevm"))]
mod corevm;
