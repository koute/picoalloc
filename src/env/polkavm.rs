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
    unsafe fn expand_memory_until(&mut self, base: *mut u8, size: Size) -> bool {
        let bytes = sbrk(0).addr() - base.addr();
        bytes >= size.bytes() as usize || !sbrk(size.bytes() as usize - bytes).is_null()
    }

    #[inline]
    unsafe fn free_address_space(&mut self, _base: *mut u8, _size: Size) {}
}
