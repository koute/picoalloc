#![no_std]
#![allow(unexpected_cfgs)]

mod allocator;
mod env;
mod mutex;

#[cfg(feature = "global_allocator_libc")]
mod global_allocator_libc;

#[cfg(any(feature = "global_allocator_rust", feature = "global_allocator_libc"))]
#[cfg_attr(feature = "global_allocator_rust", global_allocator)]
pub(crate) static GLOBAL_ALLOCATOR: Mutex<Allocator<crate::env::System>> = Mutex::new(Allocator::new(
    crate::env::System,
    Size::from_bytes_usize(1024 * 1024 * 1024).unwrap(),
));

pub use crate::allocator::{Allocator, Size};
pub use crate::env::{Array, ArrayPointer, Env};
pub use crate::mutex::Mutex;

#[doc(hidden)]
pub use crate::env::abort;

#[doc(hidden)]
pub use crate::env::System as UnsafeSystem;

#[cfg(test)]
fn test_allocator<E: Env>(env: E) {
    let mut allocator = Allocator::new(env, Size::from_bytes_usize(8 * 1024 * 1024).unwrap());
    let a0 = allocator
        .alloc(Size::from_bytes_usize(1).unwrap(), Size::from_bytes_usize(1).unwrap())
        .unwrap();
    let a1 = allocator
        .alloc(Size::from_bytes_usize(1).unwrap(), Size::from_bytes_usize(0).unwrap())
        .unwrap();
    let a2 = allocator
        .alloc(Size::from_bytes_usize(255).unwrap(), Size::from_bytes_usize(0).unwrap())
        .unwrap();

    unsafe {
        assert!(Allocator::<E>::usable_size(a0) >= 1);
        assert_eq!(Allocator::<E>::usable_size(a1), 0);
        assert_eq!(Allocator::<E>::usable_size(a2), 0);
    }

    unsafe {
        let a3 = allocator
            .alloc(Size::from_bytes_usize(1).unwrap(), Size::from_bytes_usize(0).unwrap())
            .unwrap();
        allocator.free(a3);
        allocator.free(a0);
        let a4 = allocator
            .alloc(Size::from_bytes_usize(1).unwrap(), Size::from_bytes_usize(0).unwrap())
            .unwrap();
        allocator.free(a4);
        allocator.free(a1);
        let a5 = allocator
            .alloc(Size::from_bytes_usize(1).unwrap(), Size::from_bytes_usize(0).unwrap())
            .unwrap();
        allocator.free(a5);
        allocator.free(a2);
        let a6 = allocator
            .alloc(Size::from_bytes_usize(1).unwrap(), Size::from_bytes_usize(0).unwrap())
            .unwrap();
        allocator.free(a6);
    }

    let a7 = allocator
        .alloc(Size::from_bytes_usize(255).unwrap(), Size::from_bytes_usize(255).unwrap())
        .unwrap();
    let a8 = allocator
        .alloc(Size::from_bytes_usize(128).unwrap(), Size::from_bytes_usize(65).unwrap())
        .unwrap();
    unsafe {
        allocator.free(a7);
        allocator.free(a8);
    }
}

#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
#[test]
fn test_allocator_system() {
    test_allocator(crate::env::System);
}

#[test]
fn test_allocator_buffer() {
    let mut buffer = Array([0_u8; 4096]);
    test_allocator(unsafe { ArrayPointer::new(&mut buffer) });
}

#[cfg(test)]
fn test_many_small_allocations<E: Env>(env: E, count: usize) {
    extern crate alloc;
    let mut allocator = Allocator::new(env, Size::from_bytes_usize(32 * 1024 * 1024).unwrap());
    let mut allocations = alloc::vec::Vec::new();
    for nth in 0..count {
        let Some(pointer) = allocator.alloc(Size::from_bytes_usize(1).unwrap(), Size::from_bytes_usize(1).unwrap()) else {
            panic!("allocation {nth} failed!");
        };

        allocations.push(pointer);
    }

    assert!(allocator
        .alloc(Size::from_bytes_usize(1).unwrap(), Size::from_bytes_usize(1).unwrap())
        .is_none());

    unsafe {
        allocator.free(allocations.pop().unwrap());
        allocator.free(allocations.swap_remove(0));
    }

    while !allocations.is_empty() {
        unsafe {
            allocator.free(allocations.swap_remove(allocations.len() / 2));
        }
    }
}

#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
#[test]
fn test_many_small_allocations_native() {
    test_many_small_allocations(crate::env::System, 524288);
}

#[test]
fn test_many_small_allocations_buffer() {
    #[repr(C)]
    struct Storage {
        buffer: Array<{ 1024 * 16 }>,
        sentinel: [u8; 64],
    }
    let mut storage = Storage {
        buffer: Array([0_u8; 1024 * 16]),
        sentinel: [0b10101010; 64],
    };

    test_many_small_allocations(unsafe { ArrayPointer::new(&mut storage.buffer) }, 255);
    unsafe {
        for offset in 0..storage.sentinel.len() {
            // Make sure there were no out-of-bounds writes.
            assert_eq!(core::ptr::read_volatile(storage.sentinel.as_ptr().add(offset)), 0b10101010);
        }
    }
}

#[test]
fn test_boundary() {
    #[repr(align(32))]
    pub struct TestEnv<const SIZE: usize, const LIMIT: usize> {
        buffer: [u8; SIZE],
    }

    impl<const SIZE: usize, const LIMIT: usize> Drop for TestEnv<SIZE, LIMIT> {
        fn drop(&mut self) {
            // Make sure there were no out-of-bounds writes.
            assert!(self.buffer[LIMIT..].iter().all(|&byte| byte == 0b10101010));
        }
    }

    impl<const SIZE: usize, const LIMIT: usize> Env for TestEnv<SIZE, LIMIT> {
        unsafe fn allocate_address_space(&mut self, _size: Size) -> *mut u8 {
            self.buffer.as_mut_ptr()
        }

        unsafe fn expand_memory_until(&mut self, _base: *mut u8, size: Size) -> bool {
            size <= const { Size::from_bytes_usize(LIMIT).unwrap() }
        }

        unsafe fn free_address_space(&mut self, _base: *mut u8, _size: Size) {}
    }

    impl<const SIZE: usize, const LIMIT: usize> Default for TestEnv<SIZE, LIMIT> {
        fn default() -> Self {
            Self {
                buffer: [0b10101010; SIZE],
            }
        }
    }

    let env = TestEnv::<256, 64>::default();
    let mut alloc = Allocator::new(env, Size::from_bytes_usize(64).unwrap());
    let p = alloc
        .alloc(Size::from_bytes_usize(1).unwrap(), Size::from_bytes_usize(32).unwrap())
        .unwrap();
    unsafe {
        alloc.free(p);
    }
    assert!(alloc
        .alloc(Size::from_bytes_usize(1).unwrap(), Size::from_bytes_usize(33).unwrap())
        .is_none());
}
