#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
mod linux;

#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
pub(crate) use self::linux::*;

#[cfg(all(target_env = "polkavm", not(feature = "corevm")))]
mod polkavm;

#[cfg(all(target_env = "polkavm", not(feature = "corevm")))]
pub(crate) use self::polkavm::*;

#[cfg(all(target_env = "polkavm", feature = "corevm"))]
mod corevm;

#[cfg(all(target_env = "polkavm", feature = "corevm"))]
pub(crate) use self::corevm::*;

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
