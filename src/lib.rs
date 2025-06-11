#![no_std]
#![allow(unexpected_cfgs)]

mod allocator;
mod env;

#[cfg(any(feature = "global_allocator_rust", feature = "global_allocator_libc"))]
mod global_allocator;

#[cfg(feature = "global_allocator_libc")]
mod global_allocator_libc;

#[cfg(feature = "global_allocator_rust")]
mod global_allocator_rust;

pub use crate::allocator::{Allocator, Size};
pub use crate::env::{Array, ArrayPointer, Env, System};

#[doc(hidden)]
pub use crate::env::abort;

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

#[cfg(any(all(target_arch = "x86_64", target_os = "linux"), target_env = "polkavm"))]
#[test]
fn test_allocator_system() {
    test_allocator(System);
}

#[test]
fn test_allocator_buffer() {
    let mut buffer = Array([0_u8; 4096]);
    test_allocator(ArrayPointer(&mut buffer));
}

#[cfg(test)]
fn test_many_small_allocations<E: Env>(env: E, count: usize) {
    extern crate alloc;
    let mut allocator = Allocator::new(env, Size::from_bytes_usize(32 * 1024 * 1024).unwrap());
    let mut allocations = alloc::vec::Vec::new();
    for _ in 0..count {
        allocations.push(
            allocator
                .alloc(Size::from_bytes_usize(1).unwrap(), Size::from_bytes_usize(1).unwrap())
                .unwrap(),
        );
    }

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

#[cfg(any(all(target_arch = "x86_64", target_os = "linux"), target_env = "polkavm"))]
#[test]
fn test_many_small_allocations_native() {
    test_many_small_allocations(System, 10000);
}

#[test]
fn test_many_small_allocations_buffer() {
    let mut buffer = Array([0_u8; 1024 * 16]);
    test_many_small_allocations(ArrayPointer(&mut buffer), 256);
}
