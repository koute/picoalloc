use crate::Size;

#[polkavm_derive::polkavm_import]
extern "C" {
    pub fn alloc(size: u64) -> u64;
    pub fn free(address: u64, size: u64);
}

#[inline]
pub fn allocate_address_space(size: Size) -> *mut u8 {
    // SAFETY: `alloc` always returns a valid pointer or zero on error.
    let address = unsafe { alloc(u64::from(size.bytes())) };
    core::ptr::with_exposed_provenance_mut(address as usize)
}

#[inline]
pub fn expand_memory_until(_end: *mut u8) -> bool {
    true
}

#[inline]
pub fn free_address_space(base: *mut u8, size: Size) {
    // SAFETY: `free` always succeeds.
    unsafe { free(base.expose_provenance() as u64, u64::from(size.bytes())) }
}
