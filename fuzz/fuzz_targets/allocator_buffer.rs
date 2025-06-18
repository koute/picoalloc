#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use std::collections::BTreeSet;

#[derive(Arbitrary, Debug)]
enum Op {
    Alloc { align: u16, size: u16 },
    Free { index: usize },
}

use picoalloc::{Allocator, Env, Size};

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

#[repr(align(32))]
pub struct TestEnv<const SIZE: usize, const LIMIT: usize> {
    buffer: [u8; SIZE],
}

impl<const SIZE: usize, const LIMIT: usize> Drop for TestEnv<SIZE, LIMIT> {
    fn drop(&mut self) {
        // Make sure there were no out-of-bounds writes.
        assert!(self.buffer[LIMIT..].iter().all(|&byte| byte == 0b10101010));
    }
}

impl<const SIZE: usize, const LIMIT: usize> Env for TestEnv<SIZE, LIMIT> {
    unsafe fn allocate_address_space(&mut self, _size: Size) -> *mut u8 {
        self.buffer.as_mut_ptr()
    }

    unsafe fn expand_memory_until(&mut self, _base: *mut u8, size: Size) -> bool {
        size <= const { Size::from_bytes_usize(LIMIT).unwrap() }
    }

    unsafe fn free_address_space(&mut self, _base: *mut u8, _size: Size) {}
}

impl<const SIZE: usize, const LIMIT: usize> Default for TestEnv<SIZE, LIMIT> {
    fn default() -> Self {
        let mut buffer = [0b10101010; SIZE];
        buffer[..LIMIT].fill(0);

        Self { buffer }
    }
}

type DefaultEnv = TestEnv<16384, 2048>;

fuzz_target!(|ops: Vec<Op>| {
    let env = DefaultEnv::default();
    let mut allocator = Allocator::new(env, Size::from_bytes_usize(1024 * 1024).unwrap());
    let mut allocations: Vec<(*mut u8, Vec<u8>)> = vec![];
    let mut alive_addresses = BTreeSet::new();

    for method in ops {
        match method {
            Op::Alloc { align, size } => {
                let align = core::cmp::max(1, (align as usize).next_power_of_two());
                let size = core::cmp::max(1, size as usize);
                let Some(pointer) = allocator.alloc_zeroed(Size::from_bytes_usize(align).unwrap(), Size::from_bytes_usize(size).unwrap())
                else {
                    continue;
                };

                assert_eq!(pointer.addr() % align, 0);

                let usable_size = unsafe { Allocator::<DefaultEnv>::usable_size(pointer) };
                assert!(usable_size >= size);

                let data = {
                    let slice: &mut [u8] = unsafe { core::slice::from_raw_parts_mut(pointer, usable_size) };
                    assert!(slice.iter().copied().all(|byte| byte == 0));

                    fill_slice(pointer.addr() as u128, slice);
                    slice.to_vec()
                };

                alive_addresses.insert(pointer);
                allocations.push((pointer, data));
            }
            Op::Free { index } => {
                if !allocations.is_empty() {
                    let index = index % allocations.len();
                    let (pointer, expected_data) = allocations.swap_remove(index);
                    let slice = unsafe { core::slice::from_raw_parts(pointer, expected_data.len()) };

                    assert!(alive_addresses.remove(&pointer));

                    // Make sure the data wasn't corrupted.
                    assert!(slice == expected_data);
                    unsafe { allocator.free(pointer) };
                }
            }
        }
    }
});
