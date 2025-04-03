#![no_std]

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    picoalloc::abort();
}

#[test]
fn test_allocator() {
    extern crate alloc;

    let mut vec = alloc::vec::Vec::new();
    for n in 0..1024 {
        vec.push(core::hint::black_box(n));
    }

    let mut vec2 = alloc::vec::Vec::new();
    for n in 0..1024 {
        vec2.push(core::hint::black_box(n));
    }

    for n in 1024..2048 {
        vec.push(core::hint::black_box(n));
    }

    #[allow(clippy::needless_range_loop)]
    for n in 0..2048 {
        assert_eq!(vec[n], n);
    }
}
