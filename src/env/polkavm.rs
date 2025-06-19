use crate::env::System;
use crate::{Env, Size};

#[inline]
fn sbrk(size: usize) -> *mut u8 {
    // SAFETY: Allocating memory is always safe.
    unsafe {
        let address;
        core::arch::asm!(
            ".insn r 0xb, 1, 0, {dst}, {size}, zero",
            size = in(reg) size,
            dst = lateout(reg) address,
        );
        address
    }
}

impl<const SIZE: usize> Env for System<SIZE> {
    #[inline]
    fn total_space(&self) -> Size {
        const { Size::from_bytes_usize(SIZE).unwrap() }
    }

    #[inline]
    unsafe fn allocate_address_space(&mut self) -> *mut u8 {
        unsafe {
            let mut pointer: *mut u8;
            core::arch::asm!(
                ".insn r 0xb, 3, 0, {dst}, zero, zero",
                dst = out(reg) pointer,
            );

            let aligned_pointer = pointer.with_addr(pointer.addr().next_multiple_of(32));
            let sbrk_pointer = sbrk(0);
            if sbrk_pointer.addr() < aligned_pointer.addr() {
                let bytes = aligned_pointer.addr() - sbrk_pointer.addr();
                if sbrk(bytes).is_null() {
                    return core::ptr::null_mut();
                }
            }

            aligned_pointer
        }
    }

    #[inline]
    unsafe fn expand_memory_until(&mut self, base: *mut u8, size: Size) -> bool {
        let bytes = sbrk(0).addr() - base.addr();
        bytes >= size.bytes() as usize || !sbrk(size.bytes() as usize - bytes).is_null()
    }

    #[inline]
    unsafe fn free_address_space(&mut self, _base: *mut u8) {}
}
