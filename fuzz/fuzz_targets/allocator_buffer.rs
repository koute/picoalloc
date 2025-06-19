#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use std::collections::BTreeSet;
use core::ptr::NonNull;

#[derive(Arbitrary, Debug)]
enum Op {
    Alloc { align: u16, size: u16 },
    Free { index: usize },
    Shrink { index: usize, size: u16 },
    Grow { index: usize, size: u16 },
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
    #[inline]
    fn total_space(&self) -> Size {
        const { Size::from_bytes_usize(SIZE * 16).unwrap() }
    }

    unsafe fn allocate_address_space(&mut self) -> *mut u8 {
        self.buffer.as_mut_ptr()
    }

    unsafe fn expand_memory_until(&mut self, _base: *mut u8, size: Size) -> bool {
        size <= const { Size::from_bytes_usize(LIMIT).unwrap() }
    }

    unsafe fn free_address_space(&mut self, _base: *mut u8) {}
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
    let mut allocator = Allocator::new(env);
    let mut allocations: Vec<(NonNull<u8>, Vec<u8>)> = vec![];
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

                assert_eq!(pointer.as_ptr().addr() % align, 0);

                let usable_size = unsafe { Allocator::<DefaultEnv>::usable_size(pointer) };
                assert!(usable_size >= size);

                let data = {
                    let slice: &mut [u8] = unsafe { core::slice::from_raw_parts_mut(pointer.as_ptr(), usable_size) };
                    assert!(slice.iter().copied().all(|byte| byte == 0));

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
            Op::Shrink { index, size } => {
                let size = size as usize + 1;
                let size = Size::from_bytes_usize(size).unwrap();

                if allocations.is_empty() {
                    continue;
                }

                let index = index % allocations.len();
                {
                    let &(pointer, ref expected_data) = &allocations[index];
                    let slice = unsafe { core::slice::from_raw_parts(pointer.as_ptr(), expected_data.len()) };
                    assert!(slice == expected_data);
                }
                allocations[index].1.truncate(size.bytes().try_into().unwrap());
                unsafe { allocator.shrink_inplace(allocations[index].0, size) }

                let &(pointer, ref expected_data) = &allocations[index];
                let slice = unsafe { core::slice::from_raw_parts(pointer.as_ptr(), expected_data.len()) };
                assert!(slice == expected_data);
            }
            Op::Grow { index, size } => {
                let size = size as usize + 1;
                let size = Size::from_bytes_usize(size).unwrap();

                if allocations.is_empty() {
                    continue;
                }

                let index = index % allocations.len();
                if let Some(new_size) = unsafe { allocator.grow_inplace(allocations[index].0, size) } {
                    let old_size = allocations[index].1.len();
                    let new_size = new_size.bytes().try_into().unwrap();
                    if old_size == new_size {
                        continue;
                    }
                    allocations[index].1.resize(new_size, 0);
                    {
                        let &mut (pointer, ref mut expected_data) = &mut allocations[index];
                        let slice_actual = unsafe { core::slice::from_raw_parts_mut(pointer.add(old_size).as_ptr(), new_size - old_size) };
                        let slice_expected = &mut expected_data[old_size..new_size];
                        fill_slice((pointer.as_ptr().addr() ^ 0b10101010) as u128, slice_expected);
                        slice_actual.copy_from_slice(slice_expected);
                    }

                    let &(pointer, ref expected_data) = &allocations[index];
                    let slice = unsafe { core::slice::from_raw_parts(pointer.as_ptr(), expected_data.len()) };
                    assert!(slice == expected_data);
                }
            }
        }
    }
});
