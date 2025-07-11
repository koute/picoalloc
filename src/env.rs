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
    fn total_space(&self) -> Size;
    unsafe fn allocate_address_space(&mut self) -> *mut u8;
    unsafe fn expand_memory_until(&mut self, base: *mut u8, size: Size) -> bool;
    unsafe fn free_address_space(&mut self, base: *mut u8);
}

#[repr(align(32))]
pub struct Array<const SIZE: usize>(pub [u8; SIZE]);

#[repr(transparent)]
pub struct ArrayPointer<const SIZE: usize>(*mut Array<SIZE>);

impl<const SIZE: usize> ArrayPointer<SIZE> {
    pub const unsafe fn new(array: *mut Array<SIZE>) -> Self {
        ArrayPointer(array)
    }
}

impl<const SIZE: usize> Env for ArrayPointer<SIZE> {
    fn total_space(&self) -> Size {
        const { Size::from_bytes_usize(SIZE).unwrap() }
    }

    unsafe fn allocate_address_space(&mut self) -> *mut u8 {
        self.0.cast()
    }

    unsafe fn expand_memory_until(&mut self, _base: *mut u8, size: Size) -> bool {
        size <= const { Size::from_bytes_usize(SIZE).unwrap() }
    }

    unsafe fn free_address_space(&mut self, _base: *mut u8) {}
}

pub struct System<const SIZE: usize>;

#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
mod linux;

#[cfg(all(target_env = "polkavm", not(feature = "corevm")))]
mod polkavm;

#[cfg(all(target_env = "polkavm", feature = "corevm"))]
mod corevm;

#[cfg(target_family = "wasm")]
mod wasm;
