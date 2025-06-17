use crate::env::System;
use crate::{Env, Size};

extern "C" {
    static mut __heap_base: u8;
}

#[inline]
fn heap_base() -> *mut u8 {
    let pointer = (&raw mut __heap_base);
    pointer.with_addr(pointer.addr().next_multiple_of(32))
}

const PAGE_SIZE: usize = 64 * 1024;

impl Env for System {
    #[inline]
    unsafe fn allocate_address_space(&mut self, _size: Size) -> *mut u8 {
        heap_base()
    }

    #[inline]
    unsafe fn expand_memory_until(&mut self, base: *mut u8, size: Size) -> bool {
        let current_size = core::arch::wasm32::memory_size(0) * PAGE_SIZE;
        let required_size = base.addr() + size.bytes() as usize;
        if current_size >= required_size {
            return true;
        }

        let delta = (required_size - current_size).next_multiple_of(PAGE_SIZE);
        core::arch::wasm32::memory_grow(0, delta / PAGE_SIZE) != usize::MAX
    }

    #[inline]
    unsafe fn free_address_space(&mut self, _base: *mut u8, _size: Size) {}
}
