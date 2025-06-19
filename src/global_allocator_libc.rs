use crate::allocator::Size;
use crate::GLOBAL_ALLOCATOR;

use core::ffi::{c_int, c_void};
use core::ptr::NonNull;

const ENOMEM: c_int = 12;
const EINVAL: c_int = 22;

extern "C" {
    fn __errno_location() -> *mut c_int;
}

#[inline]
fn set_errno(value: c_int) {
    unsafe {
        *__errno_location() = value;
    }
}

#[no_mangle]
pub extern "C" fn __libc_malloc(size: usize) -> *mut c_void {
    malloc(size)
}

#[no_mangle]
pub extern "C" fn __libc_calloc(count: usize, size: usize) -> *mut c_void {
    calloc(count, size)
}

#[no_mangle]
pub unsafe extern "C" fn __libc_realloc(pointer: *mut c_void, size: usize) -> *mut c_void {
    realloc(pointer, size)
}

#[no_mangle]
pub unsafe extern "C" fn __libc_free(pointer: *mut c_void) {
    free(pointer)
}

#[no_mangle]
pub extern "C" fn calloc(count: usize, size: usize) -> *mut c_void {
    let Some(total_size) = count.checked_mul(size) else {
        set_errno(ENOMEM);
        return core::ptr::null_mut();
    };

    let Some(total_size) = Size::from_bytes_usize(total_size) else {
        set_errno(ENOMEM);
        return core::ptr::null_mut();
    };

    let pointer = {
        let mut allocator = GLOBAL_ALLOCATOR.lock();
        allocator.alloc_zeroed(const { Size::from_bytes_usize(16).unwrap() }, total_size)
    };

    if let Some(pointer) = pointer {
        pointer.as_ptr().cast()
    } else {
        set_errno(ENOMEM);
        core::ptr::null_mut()
    }
}

#[no_mangle]
pub extern "C" fn memalign(align: usize, size: usize) -> *mut c_void {
    aligned_alloc(align, size)
}

#[no_mangle]
pub extern "C" fn aligned_alloc(align: usize, size: usize) -> *mut c_void {
    let mut result = core::ptr::null_mut();
    let errno = unsafe { posix_memalign(&mut result, align, size) };
    if errno != 0 {
        set_errno(errno);
    }
    result
}

#[no_mangle]
pub unsafe extern "C" fn reallocarray(pointer: *mut c_void, count: usize, size: usize) -> *mut c_void {
    let Some(total_size) = count.checked_mul(size) else {
        set_errno(ENOMEM);
        return core::ptr::null_mut();
    };

    realloc(pointer, total_size)
}

#[no_mangle]
pub extern "C" fn malloc(size: usize) -> *mut c_void {
    aligned_alloc(core::mem::size_of::<*mut c_void>(), size)
}

#[no_mangle]
pub unsafe extern "C" fn free(pointer: *mut c_void) {
    let Some(pointer) = NonNull::new(pointer.cast::<u8>()) else {
        return;
    };

    let mut allocator = GLOBAL_ALLOCATOR.lock();
    allocator.free(pointer);
}

#[no_mangle]
pub unsafe extern "C" fn posix_memalign(result: *mut *mut c_void, align: usize, size: usize) -> c_int {
    if !align.is_power_of_two() || align < core::mem::size_of::<*mut c_void>() {
        return EINVAL;
    }

    let Some(size) = Size::from_bytes_usize(size) else {
        return ENOMEM;
    };

    let Some(align) = Size::from_bytes_usize(align) else {
        return ENOMEM;
    };

    let mut allocator = GLOBAL_ALLOCATOR.lock();
    if let Some(pointer) = allocator.alloc(align, size) {
        unsafe { *result = pointer.as_ptr().cast() }

        0
    } else {
        ENOMEM
    }
}

#[no_mangle]
pub unsafe extern "C" fn realloc(pointer: *mut c_void, size: usize) -> *mut c_void {
    let Some(pointer) = NonNull::new(pointer) else {
        return malloc(size);
    };

    if size == 0 {
        free(pointer.as_ptr());
        return core::ptr::null_mut();
    }

    let Some(size) = Size::from_bytes_usize(size) else {
        set_errno(ENOMEM);
        return core::ptr::null_mut();
    };

    let mut allocator = GLOBAL_ALLOCATOR.lock();
    if let Some(pointer) = allocator.realloc(pointer.cast::<u8>(), const { Size::from_bytes_usize(1).unwrap() }, size) {
        pointer.as_ptr().cast()
    } else {
        set_errno(ENOMEM);
        core::ptr::null_mut()
    }
}

#[no_mangle]
pub unsafe extern "C" fn malloc_usable_size(pointer: *mut c_void) -> usize {
    let Some(pointer) = NonNull::new(pointer) else {
        return 0;
    };
    crate::SystemAllocator::usable_size(pointer.cast())
}
