use crate::env::abort;
use crate::{Env, Size, System};

#[inline]
fn abort_on_fail(result: usize) -> usize {
    if (result as isize) >= -4095 && (result as isize) < 0 {
        abort();
    }

    result
}

#[inline]
unsafe fn syscall2(nr: usize, a0: usize, a1: usize) -> usize {
    let r0;
    core::arch::asm!(
        "syscall",
        inlateout("rax") nr => r0,
        in("rdi") a0,
        in("rsi") a1,
        lateout("rcx") _,
        lateout("r11") _,
        options(nostack, preserves_flags)
    );
    r0
}

#[inline]
unsafe fn syscall6(nr: usize, a0: usize, a1: usize, a2: usize, a3: usize, a4: usize, a5: usize) -> usize {
    let r0;
    core::arch::asm!(
        "syscall",
        inlateout("rax") nr => r0,
        in("rdi") a0,
        in("rsi") a1,
        in("rdx") a2,
        in("r10") a3,
        in("r8") a4,
        in("r9") a5,
        lateout("rcx") _,
        lateout("r11") _,
        options(nostack, preserves_flags)
    );
    r0
}

impl Env for System {
    #[inline]
    unsafe fn allocate_address_space(&mut self, size: Size) -> *mut u8 {
        const SYS_MMAP: usize = 9;
        const PROT_READ: usize = 1;
        const PROT_WRITE: usize = 2;
        const MAP_PRIVATE: usize = 2;
        const MAP_ANONYMOUS: usize = 32;
        unsafe {
            let pointer = abort_on_fail(syscall6(
                SYS_MMAP,
                0,
                size.bytes() as usize,
                PROT_READ | PROT_WRITE,
                MAP_ANONYMOUS | MAP_PRIVATE,
                usize::MAX,
                0,
            ));

            core::ptr::with_exposed_provenance_mut(pointer)
        }
    }

    #[inline]
    unsafe fn expand_memory_until(&mut self, _base: *mut u8, _size: Size) -> bool {
        true
    }

    #[inline]
    unsafe fn free_address_space(&mut self, base: *mut u8, size: Size) {
        const SYS_MUNMAP: usize = 11;
        unsafe {
            abort_on_fail(syscall2(SYS_MUNMAP, base.expose_provenance(), size.bytes() as usize));
        }
    }
}
