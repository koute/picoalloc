#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use std::collections::BTreeSet;
use core::ptr::NonNull;

#[derive(Arbitrary, Debug)]
enum Op {
    Alloc { align: u16, size: u16 },
    Free { index: usize },
}

use picoalloc::{Allocator, Size, UnsafeSystem};

fn fill_slice(seed: u128, slice: &mut [u8]) {
    let mut rng = oorandom::Rand64::new(seed);
    let mut offset = 0;
    while offset + 8 < slice.len() {
        slice[offset..offset + 8].copy_from_slice(&rng.rand_u64().to_le_bytes());
        offset += 8;
    }

    while offset < slice.len() {
        slice[offset] = rng.rand_u64() as u8;
        offset += 1;
    }
}

fuzz_target!(|ops: Vec<Op>| {
    let mut allocator = Allocator::new(UnsafeSystem, Size::from_bytes_usize(32 * 1024 * 1024).unwrap());
    let mut allocations: Vec<(NonNull<u8>, Vec<u8>)> = vec![];
    let mut alive_addresses = BTreeSet::new();

    for method in ops {
        match method {
            Op::Alloc { align, size } => {
                let align = core::cmp::max(1, (align as usize).next_power_of_two());
                let size = core::cmp::max(1, size as usize);
                let pointer = allocator
                    .alloc(Size::from_bytes_usize(align).unwrap(), Size::from_bytes_usize(size).unwrap())
                    .unwrap();

                assert_eq!(pointer.as_ptr().addr() % align, 0);

                let usable_size = unsafe { Allocator::<UnsafeSystem>::usable_size(pointer) };
                assert!(usable_size >= size);

                let data = {
                    let slice: &mut [u8] = unsafe { core::slice::from_raw_parts_mut(pointer.as_ptr(), usable_size) };

                    fill_slice(pointer.as_ptr().addr() as u128, slice);
                    slice.to_vec()
                };

                alive_addresses.insert(pointer);
                allocations.push((pointer, data));
            }
            Op::Free { index } => {
                if !allocations.is_empty() {
                    let index = index % allocations.len();
                    let (pointer, expected_data) = allocations.swap_remove(index);
                    let slice = unsafe { core::slice::from_raw_parts(pointer.as_ptr(), expected_data.len()) };

                    assert!(alive_addresses.remove(&pointer));

                    // Make sure the data wasn't corrupted.
                    assert!(slice == expected_data);
                    unsafe { allocator.free(pointer) };
                }
            }
        }
    }
});
