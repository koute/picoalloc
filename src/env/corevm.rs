use crate::env::System;
use crate::{Env, Size};

#[polkavm_derive::polkavm_import]
extern "C" {
    pub fn alloc(size: u64) -> u64;
    pub fn free(address: u64, size: u64);
}

impl Env for System {
    #[inline]
    unsafe fn allocate_address_space(&mut self, size: Size) -> *mut u8 {
        // SAFETY: `alloc` always returns a valid pointer or zero on error.
        let address = unsafe { alloc(u64::from(size.bytes())) };
        core::ptr::with_exposed_provenance_mut(address as usize)
    }

    #[inline]
    unsafe fn expand_memory_until(&mut self, _base: *mut u8, _size: Size) -> bool {
        true
    }

    #[inline]
    unsafe fn free_address_space(&mut self, base: *mut u8, size: Size) {
        // SAFETY: `free` always succeeds.
        unsafe { free(base.expose_provenance() as u64, u64::from(size.bytes())) }
    }
}
