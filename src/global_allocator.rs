extern crate alloc;

use crate::{Allocator, Env, Mutex, Size};

unsafe impl<E: Env> alloc::alloc::GlobalAlloc for Mutex<Allocator<E>> {
    unsafe fn alloc(&self, layout: alloc::alloc::Layout) -> *mut u8 {
        let Some(align) = Size::from_bytes_usize(layout.align()) else {
            return core::ptr::null_mut();
        };

        let Some(size) = Size::from_bytes_usize(layout.size()) else {
            return core::ptr::null_mut();
        };

        self.lock().alloc(align, size).unwrap_or(core::ptr::null_mut())
    }

    unsafe fn dealloc(&self, pointer: *mut u8, _layout: alloc::alloc::Layout) {
        self.lock().free(pointer);
    }
}
