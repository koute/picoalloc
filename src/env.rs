use crate::allocator::Size;

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

#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
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

#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
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

#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
#[inline]
fn abort_on_fail(result: usize) -> usize {
    if (result as isize) >= -4095 && (result as isize) < 0 {
        abort();
    }

    result
}

#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
#[inline]
pub fn allocate_address_space(size: Size) -> *mut u8 {
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

#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
#[inline]
pub fn expand_memory_until(_end: *mut u8) -> bool {
    true
}

#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
#[inline]
pub fn free_address_space(base: *mut u8, size: Size) {
    const SYS_MUNMAP: usize = 11;
    unsafe {
        abort_on_fail(syscall2(SYS_MUNMAP, base.expose_provenance(), size.bytes() as usize));
    }
}

#[cfg(target_env = "polkavm")]
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

#[cfg(target_env = "polkavm")]
#[inline]
pub fn allocate_address_space(_size: Size) -> *mut u8 {
    unsafe {
        let mut output;
        core::arch::asm!(
            ".insn r 0xb, 3, 0, {dst}, zero, zero",
            dst = out(reg) output,
        );
        output
    }
}

#[cfg(target_env = "polkavm")]
#[inline]
pub fn expand_memory_until(end: *mut u8) -> bool {
    let heap_end = sbrk(0);
    if heap_end.addr() >= end.addr() {
        return true;
    }

    !sbrk(end.addr() - heap_end.addr()).is_null()
}

#[cfg(target_env = "polkavm")]
#[inline]
pub fn free_address_space(_base: *mut u8, _size: Size) {}
