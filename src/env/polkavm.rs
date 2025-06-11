use crate::{Env, Size, System};

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

impl Env for System {
    #[inline]
    unsafe fn allocate_address_space(&mut self, _size: Size) -> *mut u8 {
        unsafe {
            let mut output;
            core::arch::asm!(
                ".insn r 0xb, 3, 0, {dst}, zero, zero",
                dst = out(reg) output,
            );
            output
        }
    }

    #[inline]
    unsafe fn expand_memory_until(&mut self, end: *mut u8) -> bool {
        let heap_end = sbrk(0);
        if heap_end.addr() >= end.addr() {
            return true;
        }

        !sbrk(end.addr() - heap_end.addr()).is_null()
    }

    #[inline]
    unsafe fn free_address_space(&mut self, _base: *mut u8, _size: Size) {}
}
