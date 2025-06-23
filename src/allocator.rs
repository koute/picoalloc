#![allow(clippy::unnecessary_cast)]

use crate::Env;
use core::ptr::NonNull;

#[cfg(any(test, feature = "paranoid"))]
macro_rules! paranoid_assert {
    ($e:expr) => {
        if !$e {
            panic!(concat!("Assertion failed: ", stringify!($e)));
        }
    };
}

#[cfg(any(test, feature = "paranoid"))]
macro_rules! paranoid_assert_eq {
    ($lhs:expr, $rhs:expr) => {
        if $lhs != $rhs {
            panic!(concat!("Assertion failed: ", stringify!($lhs), " == ", stringify!($rhs)));
        }
    };
}

#[cfg(not(any(test, feature = "paranoid")))]
macro_rules! paranoid_assert {
    ($e:expr) => {};
}

#[cfg(not(any(test, feature = "paranoid")))]
macro_rules! paranoid_assert_eq {
    ($lhs:expr, $rhs:expr) => {};
}

const MAX_ALLOCATION_SIZE: Size = Size::from_bytes_usize(1024 * 1024 * 1024).unwrap();
const MAX_BINS: u32 = 4096;

type SizeT = u32;
const ALLOCATION_GRANULARITY: SizeT = 32;
const ALLOCATION_SIZE_SHIFT: u32 = ALLOCATION_GRANULARITY.ilog2();

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
#[repr(transparent)]
pub struct Size(SizeT);

impl Size {
    #[inline]
    pub const fn from_bytes_usize(bytes: usize) -> Option<Self> {
        let Some(size) = bytes.checked_add(ALLOCATION_GRANULARITY as usize - 1) else {
            return None;
        };

        let size = size >> ALLOCATION_SIZE_SHIFT;
        if size > (SizeT::MAX as usize) {
            return None;
        }

        Some(Self(size as SizeT))
    }

    fn from_pointer_and_base_unchecked<T, U>(pointer: Pointer<U>, base: Pointer<T>) -> Size {
        Self(((pointer.address() - base.address()) >> ALLOCATION_SIZE_SHIFT) as SizeT)
    }

    #[inline]
    pub const fn bytes(self) -> SizeT {
        self.0 << ALLOCATION_SIZE_SHIFT
    }

    #[inline]
    fn checked_add(self, rhs: Size) -> Option<Self> {
        self.0.checked_add(rhs.0).map(Size)
    }

    #[inline]
    fn unchecked_add(self, rhs: Size) -> Self {
        Self(self.0 + rhs.0)
    }

    #[inline]
    fn unchecked_sub(self, rhs: Size) -> Self {
        Self(self.0 - rhs.0)
    }

    #[inline]
    fn is_empty(self) -> bool {
        self.0 == 0
    }
}

#[inline]
fn align_offset(x: SizeT, a: SizeT, b: usize) -> SizeT {
    let mask = (a - 1) as usize;
    let x = x as usize;
    let b = b >> ALLOCATION_SIZE_SHIFT;
    (((b + x + mask) & !mask) - b) as SizeT
}

#[cfg(any(
    target_arch = "x86",
    target_arch = "arm",
    target_arch = "wasm32",
    target_arch = "riscv32",
    target_env = "polkavm",
))]
pub type Address = u32;

#[cfg(any(
    target_arch = "x86_64",
    target_arch = "aarch64",
    target_arch = "wasm64",
    all(target_arch = "riscv64", not(target_env = "polkavm")),
    target_arch = "sbf"
))]
pub type Address = u64;

#[cfg(any(target_arch = "x86", target_arch = "riscv32"))]
type Mask = u32;

#[cfg(any(
    target_arch = "x86_64",
    target_arch = "aarch64",
    target_family = "wasm",
    target_arch = "riscv64",
    target_arch = "sbf"
))]
type Mask = u64;

const fn lowest_set_bit_after(value: Mask, bit_index: u32) -> u32 {
    let mask_before = (1 << bit_index) - 1;
    let mask_after = !mask_before;
    let bits_after = value & mask_after;
    if bits_after == 0 {
        u32::MAX
    } else {
        bits_after.trailing_zeros()
    }
}

// This is based on: https://github.com/sebbbi/OffsetAllocator/blob/main/offsetAllocator.cpp
#[inline]
const fn to_bin_index_generic<const MANTISSA_BITS: u32, const ROUND_UP: bool>(size: SizeT) -> u32 {
    if size == 0 {
        return 0;
    }

    let mantissa_value = 1 << MANTISSA_BITS;
    if size < mantissa_value {
        // The first 2^MANTISSA_BITS buckets contain only a single element.
        return (size - 1) as u32;
    }

    let mantissa_start_bit: u32 = (::core::mem::size_of::<Size>() as u32 * 8 - 1 - size.leading_zeros()) - MANTISSA_BITS;
    let exponent = mantissa_start_bit + 1;
    let mut mantissa = (size >> mantissa_start_bit) & (mantissa_value - 1);

    if ROUND_UP {
        let low_bits_mask: SizeT = (1 << mantissa_start_bit) - 1;
        if (size & low_bits_mask) != 0 {
            mantissa += 1;
        }
    }

    let out = exponent << MANTISSA_BITS;
    let mantissa = mantissa as u32;
    if ROUND_UP {
        out + mantissa - 1
    } else {
        (out | mantissa) - 1
    }
}

struct AllocatorBinConfig {
    mantissa_bits: u32,
    bin_count: u32,
}

const fn calculate_optimal_bin_config(max_allocation_size: Size, mut requested_max_bins: u32) -> AllocatorBinConfig {
    let true_max_bins = (::core::mem::size_of::<Mask>() * 8 * ::core::mem::size_of::<Mask>() * 8) as u32;
    if true_max_bins < requested_max_bins {
        requested_max_bins = true_max_bins
    }

    macro_rules! try_all {
            ($($mantissa_bits:expr),+) => {
                $(
                    let highest_bin_index = to_bin_index_generic::<$mantissa_bits, true>(max_allocation_size.0);
                    if highest_bin_index < requested_max_bins {
                        return AllocatorBinConfig {
                            mantissa_bits: $mantissa_bits,
                            bin_count: highest_bin_index + 1
                        };
                    }
                )+
            }
        }

    try_all! {
        8, 7, 6, 5, 4, 3, 2, 1
    }

    panic!("failed to calculate optimal configuration for the allocator");
}

const BIN_CONFIG: AllocatorBinConfig = calculate_optimal_bin_config(MAX_ALLOCATION_SIZE, MAX_BINS);

#[inline(always)]
const fn to_bin_index<const ROUND_UP: bool>(size: Size) -> u32 {
    const MANTISSA_BITS: u32 = BIN_CONFIG.mantissa_bits;
    to_bin_index_generic::<MANTISSA_BITS, ROUND_UP>(size.0)
}

const SECONDARY_LENGTH: usize = {
    let bits = BIN_CONFIG.bin_count as usize;
    let bits_per_item = ::core::mem::size_of::<Mask>() * 8;
    let mut items = bits / bits_per_item;
    if bits % bits_per_item != 0 {
        items += 1;
    }

    items
};

// Define slice access helpers to make sure no panicking code is included.
#[inline]
const unsafe fn get_unchecked<T>(slice: &[T], index: usize) -> &T {
    if cfg!(debug_assertions) || cfg!(test) || cfg!(feature = "paranoid") {
        &slice[index as usize]
    } else {
        unsafe { &*slice.as_ptr().add(index) }
    }
}

#[inline]
const unsafe fn get_mut_unchecked<T>(slice: &mut [T], index: usize) -> &mut T {
    if cfg!(debug_assertions) || cfg!(test) || cfg!(feature = "paranoid") {
        &mut slice[index as usize]
    } else {
        unsafe { &mut *slice.as_mut_ptr().add(index) }
    }
}

#[derive(Copy, Clone, Eq)]
struct BitIndex {
    index: u32,
    primary: u32,
    secondary: u32,
}

impl PartialEq for BitIndex {
    fn eq(&self, rhs: &Self) -> bool {
        let is_equal = self.index == rhs.index;
        paranoid_assert_eq!(is_equal, self.primary == rhs.primary);
        paranoid_assert_eq!(is_equal, self.secondary == rhs.secondary);
        is_equal
    }
}

impl BitIndex {
    #[inline]
    pub const fn index(&self) -> usize {
        self.index as usize
    }
}

/// A constant-length two-level bitmask.
struct BitMask {
    /// The primary mask. This mask is used to mark which secondary masks are non-empty.
    primary_mask: Mask,

    /// Secondary masks. These contain the actual bits that the `BitMask` stores.
    secondary_masks: [Mask; SECONDARY_LENGTH],
}

impl BitMask {
    const PRIMARY_BIN_SHIFT: u32 = (::core::mem::size_of::<Mask>() * 8).ilog2();
    const SECONDARY_BIN_MASK: u32 = (1 << Self::PRIMARY_BIN_SHIFT) - 1;

    const ASSERT_TYPES_ARE_BIG_ENOUGH_FOR_THE_DESIRED_BIT_WIDTH: () = {
        if SECONDARY_LENGTH > ::core::mem::size_of::<Mask>() * 8 {
            panic!("the given raw mask types are too narrow to fit a bit mask of the the desired bit length");
        }
    };

    /// Creates a new empty bitmask.
    #[inline]
    const fn new() -> Self {
        let () = Self::ASSERT_TYPES_ARE_BIG_ENOUGH_FOR_THE_DESIRED_BIT_WIDTH;

        Self {
            primary_mask: 0,
            secondary_masks: [0; SECONDARY_LENGTH],
        }
    }

    /// Converts a raw `index` into a `BitIndex`.
    #[inline]
    const fn index(index: u32) -> BitIndex {
        let primary = index >> Self::PRIMARY_BIN_SHIFT;
        let secondary = index & Self::SECONDARY_BIN_MASK;

        BitIndex { index, primary, secondary }
    }

    /// Sets the bit at `index`.
    #[inline]
    const fn set(&mut self, index: BitIndex) {
        unsafe {
            *get_mut_unchecked(&mut self.secondary_masks, index.primary as usize) |= 1 << index.secondary;
        }
        self.primary_mask |= 1 << index.primary;
    }

    /// Clears the bit at `index`.
    #[inline]
    const fn unset(&mut self, index: BitIndex) {
        let secondary = unsafe { get_mut_unchecked(&mut self.secondary_masks, index.primary as usize) };
        *secondary &= !(1 << index.secondary);
        if *secondary == 0 {
            self.primary_mask &= !(1 << index.primary);
        }
    }

    /// Finds the first set bit, starting at `min_index`.
    #[inline]
    const fn find_first(&self, min_index: BitIndex) -> Option<BitIndex> {
        let mut primary = min_index.primary;
        let mut secondary = u32::MAX;

        if (self.primary_mask & (1 << primary)) != 0 {
            let mask = unsafe { *get_unchecked(&self.secondary_masks, primary as usize) };
            secondary = lowest_set_bit_after(mask, min_index.secondary);
        }

        if secondary == u32::MAX {
            primary = lowest_set_bit_after(self.primary_mask, min_index.primary + 1);
            if primary == u32::MAX {
                return None;
            }

            secondary = unsafe { get_unchecked(&self.secondary_masks, primary as usize) }.trailing_zeros();
        }

        Some(BitIndex {
            index: (primary << Self::PRIMARY_BIN_SHIFT) | secondary,
            primary,
            secondary,
        })
    }
}

#[repr(transparent)]
pub struct Pointer<T> {
    raw: Address,
    _phantom: core::marker::PhantomData<*mut T>,
}

impl<T> Copy for Pointer<T> {}
impl<T> Clone for Pointer<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> PartialEq for Pointer<T> {
    fn eq(&self, rhs: &Pointer<T>) -> bool {
        self.raw == rhs.raw
    }
}

impl<T> Eq for Pointer<T> {}

impl<T> PartialOrd for Pointer<T> {
    fn partial_cmp(&self, rhs: &Pointer<T>) -> Option<core::cmp::Ordering> {
        Some(self.cmp(rhs))
    }
}

impl<T> Ord for Pointer<T> {
    fn cmp(&self, rhs: &Pointer<T>) -> core::cmp::Ordering {
        self.raw.cmp(&rhs.raw)
    }
}

impl<T> core::hash::Hash for Pointer<T> {
    fn hash<H>(&self, hasher: &mut H)
    where
        H: core::hash::Hasher,
    {
        self.raw.hash(hasher)
    }
}

impl<T> core::fmt::Debug for Pointer<T> {
    fn fmt(&self, fmt: &mut core::fmt::Formatter) -> core::fmt::Result {
        self.raw.fmt(fmt)
    }
}

unsafe impl<T> Sync for Pointer<T> {}
unsafe impl<T> Send for Pointer<T> {}

impl<T> Pointer<T> {
    const NULL: Self = Self::from_address(0);

    #[inline]
    const fn from_address(address: Address) -> Self {
        Pointer {
            raw: address,
            _phantom: core::marker::PhantomData,
        }
    }

    #[inline]
    const fn address(self) -> Address {
        self.raw
    }

    #[inline]
    fn is_null(self) -> bool {
        self.raw == 0
    }

    #[inline]
    fn cast<U>(self) -> Pointer<U> {
        Pointer {
            raw: self.raw,
            _phantom: core::marker::PhantomData,
        }
    }

    #[inline]
    fn unchecked_add(self, offset: Size) -> Self {
        Pointer {
            raw: self.raw.wrapping_add(offset.bytes() as Address),
            _phantom: core::marker::PhantomData,
        }
    }

    #[inline]
    fn unchecked_sub(self, offset: Size) -> Self {
        Pointer {
            raw: self.raw.wrapping_sub(offset.bytes() as Address),
            _phantom: core::marker::PhantomData,
        }
    }

    #[cfg(feature = "strict_provenance")]
    #[inline]
    fn from_pointer(pointer: *const T) -> Self {
        Self::from_address(pointer.addr() as Address)
    }

    #[cfg(feature = "strict_provenance")]
    #[inline]
    fn from_pointer_mut(pointer: *mut T) -> Self {
        Self::from_address(pointer.addr() as Address)
    }

    #[cfg(feature = "strict_provenance")]
    #[inline]
    fn raw_pointer(self, provenance: *const u8) -> *const T {
        provenance.cast::<T>().with_addr(self.raw as usize)
    }

    #[cfg(feature = "strict_provenance")]
    #[inline]
    fn raw_pointer_mut(self, provenance: *mut u8) -> *mut T {
        provenance.cast::<T>().with_addr(self.raw as usize)
    }

    #[cfg(not(feature = "strict_provenance"))]
    #[inline]
    fn from_pointer(pointer: *const T) -> Self {
        Self::from_address(pointer.expose_provenance() as Address)
    }

    #[cfg(not(feature = "strict_provenance"))]
    #[inline]
    fn from_pointer_mut(pointer: *mut T) -> Self {
        Self::from_address(pointer.expose_provenance() as Address)
    }

    #[cfg(not(feature = "strict_provenance"))]
    #[inline]
    fn raw_pointer(self, _provenance: *const u8) -> *const T {
        core::ptr::with_exposed_provenance(self.raw as usize)
    }

    #[cfg(not(feature = "strict_provenance"))]
    #[inline]
    fn raw_pointer_mut(self, _provenance: *mut u8) -> *mut T {
        core::ptr::with_exposed_provenance_mut(self.raw as usize)
    }

    #[inline]
    unsafe fn write_no_drop(self, provenance: *mut u8, value: T) {
        paranoid_assert!(!self.is_null());
        core::ptr::write(self.raw_pointer_mut(provenance), value);
    }

    #[inline]
    unsafe fn get_unchecked(self, provenance: *const u8) -> &'static T {
        paranoid_assert!(!self.is_null());
        unsafe { &*self.raw_pointer(provenance) }
    }

    #[inline]
    unsafe fn get_mut_unchecked(self, provenance: *mut u8) -> &'static mut T {
        paranoid_assert!(!self.is_null());
        unsafe { &mut *self.raw_pointer_mut(provenance) }
    }
}

#[derive(Copy, Clone, Debug)]
struct ChunkSize(SizeT);

impl ChunkSize {
    #[inline]
    fn new_allocated(size: Size) -> Self {
        Self(size.0 << 1 | 1)
    }

    #[inline]
    fn new_unallocated(size: Size) -> Self {
        Self(size.0 << 1)
    }

    #[inline]
    fn size(self) -> Size {
        Size(self.0 >> 1)
    }

    #[inline]
    fn is_allocated(self) -> bool {
        (self.0 & 1) == 1
    }
}

#[derive(Copy, Clone, Debug)]
#[repr(C)]
struct ChunkHeader {
    prev_chunk_size: Size,
    size: ChunkSize,
}

#[derive(Copy, Clone)]
#[repr(C)]
struct FreeChunkHeader {
    prev_chunk_size: Size,
    size: ChunkSize,
    next_in_list: Pointer<FreeChunkHeader>,
    prev_in_list: Pointer<FreeChunkHeader>,
}

const HEADER_SIZE: Size = Size::from_bytes_usize(core::mem::size_of::<ChunkHeader>()).unwrap();
const FREE_CHUNK_HEADER_SIZE: Size = Size::from_bytes_usize(core::mem::size_of::<FreeChunkHeader>()).unwrap();

const _: () = {
    assert!(core::mem::size_of::<ChunkHeader>() <= ALLOCATION_GRANULARITY as usize);
    assert!(core::mem::size_of::<FreeChunkHeader>() <= ALLOCATION_GRANULARITY as usize);
    assert!(HEADER_SIZE.0 == 1);
};

pub struct Allocator<E: Env> {
    allocated_space: Size,
    base_address: *mut u8,
    free_lists_with_unallocated_memory: BitMask,
    first_in_free_list: [Pointer<FreeChunkHeader>; BIN_CONFIG.bin_count as usize],
    env: E,
}

unsafe impl<E: Env> Sync for Allocator<E> where E: Sync {}
unsafe impl<E: Env> Send for Allocator<E> where E: Send {}

impl<E: Env> Drop for Allocator<E> {
    fn drop(&mut self) {
        unsafe {
            self.env.free_address_space(self.base_address);
        }
    }
}

impl<E: Env> Allocator<E> {
    pub const fn new(env: E) -> Self {
        Allocator {
            allocated_space: const { Size::from_bytes_usize(0).unwrap() },
            base_address: core::ptr::null_mut(),
            free_lists_with_unallocated_memory: BitMask::new(),
            first_in_free_list: [Pointer::NULL; BIN_CONFIG.bin_count as usize],
            env,
        }
    }

    #[inline(always)]
    fn initialize(&mut self) -> bool {
        if self.base_address.is_null() {
            self.initialize_impl()
        } else {
            true
        }
    }

    #[inline(never)]
    #[cold]
    fn initialize_impl(&mut self) -> bool {
        let base_address = unsafe { self.env.allocate_address_space() };
        if base_address.is_null() {
            return false;
        }

        paranoid_assert_eq!(base_address.addr() % ALLOCATION_GRANULARITY as usize, 0);

        let chunk = base_address.cast::<FreeChunkHeader>();
        paranoid_assert!(chunk.is_aligned());

        let is_ok = unsafe { self.env.expand_memory_until(base_address, FREE_CHUNK_HEADER_SIZE) };
        if !is_ok {
            unsafe {
                self.env.free_address_space(base_address);
            }

            return false;
        }

        self.base_address = base_address;
        self.allocated_space = FREE_CHUNK_HEADER_SIZE;
        self.paranoid_check_access(Pointer::from_pointer(chunk));

        let total_space = self.env.total_space();
        let bin = Self::size_to_bin_round_down(total_space);
        self.free_lists_with_unallocated_memory.set(bin);

        let chunk_header = FreeChunkHeader {
            prev_chunk_size: Size(0),
            size: ChunkSize::new_unallocated(total_space),
            next_in_list: Pointer::NULL,
            prev_in_list: Pointer::NULL,
        };

        unsafe {
            chunk.write(chunk_header);
            *get_mut_unchecked(&mut self.first_in_free_list, bin.index()) = Pointer::from_pointer(chunk);
        }

        self.paranoid_check_chunk(Pointer::from_pointer(chunk).cast());
        true
    }

    #[inline]
    const fn size_to_bin_round_down(mut size: Size) -> BitIndex {
        if size.0 > MAX_ALLOCATION_SIZE.0 {
            size = MAX_ALLOCATION_SIZE;
        }
        BitMask::index(to_bin_index::<false>(size))
    }

    #[inline]
    const fn size_to_bin_round_up(mut size: Size) -> BitIndex {
        if size.0 > MAX_ALLOCATION_SIZE.0 {
            size = MAX_ALLOCATION_SIZE;
        }
        BitMask::index(to_bin_index::<true>(size))
    }

    #[inline(always)]
    fn unregister_free_space_first_chunk(&mut self, chunk: Pointer<FreeChunkHeader>, bin: BitIndex) {
        self.paranoid_check_access(chunk);
        paranoid_assert_eq!(self.first_in_free_list[bin.index()], chunk);

        unsafe {
            paranoid_assert!(!chunk.get_unchecked(self.base_address).size.is_allocated());
            paranoid_assert!(chunk.get_unchecked(self.base_address).prev_in_list.is_null());
            let next_in_list = chunk.get_unchecked(self.base_address).next_in_list;
            paranoid_assert!(next_in_list != chunk);

            *get_mut_unchecked(&mut self.first_in_free_list, bin.index()) = next_in_list;
            if next_in_list.is_null() {
                self.free_lists_with_unallocated_memory.unset(bin);
            } else {
                next_in_list.get_mut_unchecked(self.base_address).prev_in_list = Pointer::NULL;
            }
        }
    }

    #[inline(always)]
    fn unregister_free_space(&mut self, chunk: Pointer<FreeChunkHeader>, bin: BitIndex) {
        self.paranoid_check_access(chunk);

        if unsafe { *get_unchecked(&self.first_in_free_list, bin.index()) } == chunk {
            self.unregister_free_space_first_chunk(chunk, bin);
        } else {
            let chunk_ref = unsafe { chunk.get_unchecked(self.base_address) };
            paranoid_assert!(!chunk_ref.size.is_allocated());
            let next_in_list = chunk_ref.next_in_list;
            let prev_in_list = chunk_ref.prev_in_list;
            paranoid_assert!(next_in_list != chunk);
            paranoid_assert!(prev_in_list != chunk);
            paranoid_assert!(!prev_in_list.is_null());

            unsafe {
                prev_in_list.get_mut_unchecked(self.base_address).next_in_list = next_in_list;
                if !next_in_list.is_null() {
                    next_in_list.get_mut_unchecked(self.base_address).prev_in_list = prev_in_list;
                }
            }
        }
    }

    #[inline(always)]
    fn register_free_space(&mut self, chunk: Pointer<FreeChunkHeader>, prev_chunk_size: Size, size: Size) -> Size {
        if size.is_empty() {
            return prev_chunk_size;
        }

        self.paranoid_check_access(chunk);

        let bin = Self::size_to_bin_round_down(size);
        unsafe {
            let next_in_list = core::mem::replace(get_mut_unchecked(&mut self.first_in_free_list, bin.index()), chunk);
            chunk.write_no_drop(
                self.base_address,
                FreeChunkHeader {
                    prev_chunk_size,
                    size: ChunkSize::new_unallocated(size),
                    next_in_list,
                    prev_in_list: Pointer::NULL,
                },
            );

            if !next_in_list.is_null() {
                next_in_list.get_mut_unchecked(self.base_address).prev_in_list = chunk;
            }
        }

        self.free_lists_with_unallocated_memory.set(bin);
        size
    }

    #[inline(always)]
    fn register_allocation(&mut self, chunk: Pointer<ChunkHeader>, prev_chunk_size: Size, size: Size) {
        self.paranoid_check_access(chunk);

        unsafe {
            chunk.write_no_drop(
                self.base_address,
                ChunkHeader {
                    prev_chunk_size,
                    size: ChunkSize::new_allocated(size),
                },
            );
        }
    }

    /// Allocates zeroed memory.
    #[inline(always)]
    pub fn alloc_zeroed(&mut self, align: Size, requested_size: Size) -> Option<NonNull<u8>> {
        self.alloc_impl(align, requested_size, true)
    }

    /// Allocates memory.
    #[inline(always)]
    pub fn alloc(&mut self, align: Size, requested_size: Size) -> Option<NonNull<u8>> {
        self.alloc_impl(align, requested_size, false)
    }

    fn alloc_impl(&mut self, align: Size, requested_size: Size, is_calloc: bool) -> Option<NonNull<u8>> {
        if align.0 == 0 || !align.0.is_power_of_two() {
            return None;
        }

        if !self.initialize() {
            return None;
        }

        let min_size = requested_size.checked_add(HEADER_SIZE)?.checked_add(align.unchecked_sub(Size(1)))?;
        if min_size.0 > MAX_ALLOCATION_SIZE.0 {
            return None;
        }

        // Find a bin with enough free space.
        //
        // First calculate the minimum bin to fit this allocation; round up in case the size doesn't match the bin size exactly.
        // If this doesn't work then try rounding down and see if maybe we can find an oversized region in the previous bin.
        let min_size_round_up = Self::size_to_bin_round_up(min_size);
        let min_size_round_down = Self::size_to_bin_round_down(min_size);
        let mut bin = self.free_lists_with_unallocated_memory.find_first(min_size_round_up);

        if bin.is_none() {
            bin = self.free_lists_with_unallocated_memory.find_first(min_size_round_down);
        }

        let bin = bin?;

        let chunk = unsafe { *get_unchecked(&self.first_in_free_list, bin.index()) };
        self.paranoid_check_chunk(chunk.cast::<ChunkHeader>());

        let chunk_size = unsafe { chunk.get_unchecked(self.base_address).size };
        paranoid_assert!(!chunk_size.is_allocated());

        let chunk_size = chunk_size.size();
        paranoid_assert_eq!(Self::size_to_bin_round_down(chunk_size), bin);

        if chunk_size < min_size {
            return None;
        }

        let chunk_offset = Size::from_pointer_and_base_unchecked(chunk, Pointer::from_pointer_mut(self.base_address));
        let data_offset = Size(align_offset(chunk_offset.0 + HEADER_SIZE.0, align.0, self.base_address.addr()));
        let header_offset = data_offset.unchecked_sub(HEADER_SIZE);
        let allocation_chunk = Pointer::from_pointer_mut(self.base_address)
            .unchecked_add(header_offset)
            .cast::<ChunkHeader>();

        paranoid_assert!(header_offset >= chunk_offset);

        let free_space_lhs = header_offset.unchecked_sub(chunk_offset);
        let free_space_rhs = chunk_size
            .unchecked_sub(requested_size)
            .unchecked_sub(free_space_lhs)
            .unchecked_sub(HEADER_SIZE);

        let mut end_offset = data_offset.unchecked_add(requested_size);
        if !free_space_rhs.is_empty() {
            end_offset = end_offset.unchecked_add(FREE_CHUNK_HEADER_SIZE);
        }

        let zero_memory = self.allocated_space > data_offset && is_calloc;
        if self.allocated_space < end_offset {
            if !unsafe { self.env.expand_memory_until(self.base_address, end_offset) } {
                return None;
            }

            self.allocated_space = end_offset;
        }

        unsafe {
            let mut prev_chunk_size = chunk.get_unchecked(self.base_address).prev_chunk_size;
            self.unregister_free_space_first_chunk(chunk, bin);

            prev_chunk_size = self.register_free_space(chunk, prev_chunk_size, free_space_lhs);
            self.register_allocation(allocation_chunk, prev_chunk_size, requested_size.unchecked_add(HEADER_SIZE));

            prev_chunk_size = requested_size.unchecked_add(HEADER_SIZE);
            let next_chunk = allocation_chunk
                .unchecked_add(HEADER_SIZE)
                .unchecked_add(requested_size)
                .cast::<FreeChunkHeader>();

            prev_chunk_size = self.register_free_space(next_chunk, prev_chunk_size, free_space_rhs);
            let final_chunk = next_chunk.unchecked_add(free_space_rhs);

            if final_chunk.cast() < Pointer::from_pointer(self.base_address).unchecked_add(self.env.total_space()) {
                self.paranoid_check_access(final_chunk);
                final_chunk.get_mut_unchecked(self.base_address).prev_chunk_size = prev_chunk_size;
            }

            self.paranoid_check_chunk(allocation_chunk);
            self.paranoid_check_chunk(allocation_chunk.unchecked_sub(free_space_lhs));
            self.paranoid_check_chunk(allocation_chunk.unchecked_add(HEADER_SIZE).unchecked_add(requested_size));
            paranoid_assert_eq!(
                allocation_chunk.get_unchecked(self.base_address).size.size(),
                requested_size.unchecked_add(HEADER_SIZE)
            );
        }

        let data: Pointer<u8> = allocation_chunk.unchecked_add(HEADER_SIZE).cast();
        paranoid_assert_eq!(data.address() % align.bytes() as Address, 0);

        let output = data.raw_pointer_mut(self.base_address);
        self.paranoid_check_chunk(Pointer::from_pointer(output).unchecked_sub(HEADER_SIZE).cast::<ChunkHeader>());

        if zero_memory {
            unsafe {
                output.write_bytes(0, requested_size.bytes() as usize);
            }
        }

        Some(unsafe { NonNull::new_unchecked(output) })
    }

    #[cfg(any(test, feature = "paranoid"))]
    #[inline(never)]
    #[track_caller]
    fn paranoid_check_access<T>(&self, pointer: Pointer<T>) {
        paranoid_assert!(!pointer.is_null());
        paranoid_assert!(
            pointer
                .address()
                .wrapping_sub(self.base_address.addr() as Address)
                .wrapping_add(core::mem::size_of::<T>() as Address)
                <= Address::from(self.allocated_space.bytes())
        );
    }

    #[cfg(not(any(test, feature = "paranoid")))]
    #[inline(always)]
    fn paranoid_check_access<T>(&self, _pointer: Pointer<T>) {}

    #[cfg(any(test, feature = "paranoid"))]
    #[inline(never)]
    #[track_caller]
    fn paranoid_check_chunk(&self, chunk: Pointer<ChunkHeader>) {
        paranoid_assert!(!chunk.is_null());
        paranoid_assert!(!self.base_address.is_null());

        unsafe {
            let base_address = Pointer::from_pointer(self.base_address);
            paranoid_assert!(chunk.cast() >= base_address);

            let end_of_address_space = base_address.unchecked_add(self.env.total_space());
            if chunk.cast() == end_of_address_space {
                return;
            }

            paranoid_assert!(chunk.address() < end_of_address_space.address());
            paranoid_assert!(
                chunk.address().wrapping_add(core::mem::size_of::<ChunkHeader>() as Address) <= end_of_address_space.address()
            );

            self.paranoid_check_access(chunk);
            let is_allocated = chunk.get_unchecked(self.base_address).size.is_allocated();

            let prev_chunk_size = chunk.get_unchecked(self.base_address).prev_chunk_size;
            if prev_chunk_size.is_empty() {
                paranoid_assert_eq!(chunk.cast(), base_address);
            } else {
                let prev_chunk = chunk.unchecked_sub(prev_chunk_size);
                paranoid_assert!(prev_chunk.cast() >= base_address);
                paranoid_assert_eq!(prev_chunk.get_unchecked(self.base_address).size.size(), prev_chunk_size);
                if !is_allocated {
                    paranoid_assert!(prev_chunk.get_unchecked(self.base_address).size.is_allocated());
                }
            }

            let size = chunk.get_unchecked(self.base_address).size.size();
            let next_chunk = chunk.unchecked_add(size);
            if next_chunk.cast() < end_of_address_space {
                paranoid_assert_eq!(next_chunk.get_unchecked(self.base_address).prev_chunk_size, size);
                if !is_allocated {
                    paranoid_assert!(next_chunk.get_unchecked(self.base_address).size.is_allocated());
                }
            } else {
                paranoid_assert_eq!(next_chunk.cast(), end_of_address_space);
            }
        }
    }

    #[cfg(not(any(test, feature = "paranoid")))]
    fn paranoid_check_chunk(&self, _chunk: Pointer<ChunkHeader>) {}

    /// Shrinks the memory allocation to at most the given size.
    ///
    /// # Safety
    ///
    /// The `pointer` must have come from [`Allocator::alloc`](Allocator::alloc), and must not have been passed to [`Allocator::free`](Allocator::free) beforehand.
    pub unsafe fn shrink_inplace(&mut self, pointer: NonNull<u8>, new_size: Size) {
        if new_size.is_empty() {
            self.free(pointer);
            return;
        }

        let pointer = pointer.as_ptr();
        let new_size = new_size.unchecked_add(HEADER_SIZE);

        let chunk = Pointer::from_pointer(pointer).unchecked_sub(HEADER_SIZE).cast::<ChunkHeader>();
        self.paranoid_check_chunk(chunk);

        let current_size = unsafe { chunk.get_unchecked(self.base_address).size };
        paranoid_assert!(current_size.is_allocated());

        let current_size = current_size.size();
        if new_size >= current_size {
            return;
        }

        let mut free_space = current_size.unchecked_sub(new_size);
        chunk.get_mut_unchecked(self.base_address).size = ChunkSize::new_allocated(new_size);

        let end_of_address_space = Pointer::from_pointer(self.base_address).unchecked_add(self.env.total_space());
        {
            let next_chunk = chunk.unchecked_add(current_size);
            if next_chunk.cast() < end_of_address_space {
                self.paranoid_check_access(next_chunk);
                let next_size = unsafe { next_chunk.get_unchecked(self.base_address).size };
                if !next_size.is_allocated() {
                    let next_size = next_size.size();
                    self.unregister_free_space(next_chunk.cast::<FreeChunkHeader>(), Self::size_to_bin_round_down(next_size));
                    free_space = free_space.unchecked_add(next_size);
                }
            }
        }

        let next_chunk = chunk.unchecked_add(new_size);
        self.register_free_space(next_chunk.cast::<FreeChunkHeader>(), new_size, free_space);

        let final_chunk = next_chunk.unchecked_add(free_space);
        if final_chunk.cast() < end_of_address_space {
            self.paranoid_check_access(final_chunk);
            final_chunk.get_mut_unchecked(self.base_address).prev_chunk_size = free_space;
        }

        self.paranoid_check_chunk(chunk);
        self.paranoid_check_chunk(next_chunk);
        self.paranoid_check_chunk(final_chunk);
    }

    /// Tries to grow the memory allocation to at least the given size.
    ///
    /// # Safety
    ///
    /// The `pointer` must have come from [`Allocator::alloc`](Allocator::alloc), and must not have been passed to [`Allocator::free`](Allocator::free) beforehand.
    pub unsafe fn grow_inplace(&mut self, pointer: NonNull<u8>, new_size: Size) -> Option<Size> {
        let new_size = new_size.checked_add(HEADER_SIZE)?;

        let pointer = pointer.as_ptr();
        let chunk = Pointer::from_pointer(pointer).unchecked_sub(HEADER_SIZE).cast::<ChunkHeader>();
        self.paranoid_check_chunk(chunk);

        let current_size = unsafe { chunk.get_unchecked(self.base_address).size };
        paranoid_assert!(current_size.is_allocated());

        let current_size = current_size.size();
        if current_size >= new_size {
            return Some(current_size.unchecked_sub(HEADER_SIZE));
        }

        let end_of_address_space = Pointer::from_pointer(self.base_address).unchecked_add(self.env.total_space());
        let old_next_chunk = chunk.unchecked_add(current_size);
        if old_next_chunk.cast() >= end_of_address_space {
            return None;
        }

        self.paranoid_check_chunk(old_next_chunk);
        let old_next_size = unsafe { old_next_chunk.get_unchecked(self.base_address).size };
        if old_next_size.is_allocated() {
            return None;
        }

        let old_next_size = old_next_size.size();
        let available_space = current_size.unchecked_add(old_next_size);
        if available_space < new_size {
            return None;
        }

        let remaining_free_space = available_space.unchecked_sub(new_size);
        let new_next_chunk = chunk.unchecked_add(new_size);

        let mut end_offset = Size::from_pointer_and_base_unchecked(new_next_chunk, Pointer::from_pointer_mut(self.base_address));
        if !remaining_free_space.is_empty() {
            end_offset = end_offset.unchecked_add(FREE_CHUNK_HEADER_SIZE);
        }

        if self.allocated_space < end_offset {
            if !unsafe { self.env.expand_memory_until(self.base_address, end_offset) } {
                return None;
            }

            self.allocated_space = end_offset;
        }

        self.unregister_free_space(
            old_next_chunk.cast::<FreeChunkHeader>(),
            Self::size_to_bin_round_down(old_next_size),
        );
        chunk.get_mut_unchecked(self.base_address).size = ChunkSize::new_allocated(new_size);

        let chunk_size = self.register_free_space(new_next_chunk.cast::<FreeChunkHeader>(), new_size, remaining_free_space);
        let final_chunk = new_next_chunk.unchecked_add(remaining_free_space);
        if final_chunk.cast() < end_of_address_space {
            self.paranoid_check_access(final_chunk);
            final_chunk.get_mut_unchecked(self.base_address).prev_chunk_size = chunk_size;
        }

        self.paranoid_check_chunk(chunk);
        self.paranoid_check_chunk(new_next_chunk);
        self.paranoid_check_chunk(final_chunk);
        Some(new_size.unchecked_sub(HEADER_SIZE))
    }

    /// Reallocates the memory pointed by `pointer`.
    ///
    /// # Safety
    ///
    /// The `pointer` must have come from [`Allocator::alloc`](Allocator::alloc), and must not have been passed to [`Allocator::free`](Allocator::free) beforehand.
    pub unsafe fn realloc(&mut self, pointer: NonNull<u8>, align: Size, new_size: Size) -> Option<NonNull<u8>> {
        let current_size = Self::usable_size_impl(pointer);
        if new_size == current_size {
            return Some(pointer);
        }

        if new_size.is_empty() {
            self.free(pointer);
            return None;
        }

        if cfg!(feature = "realloc_inplace") {
            if new_size < current_size {
                self.shrink_inplace(pointer, new_size);
                return Some(pointer);
            }

            if self.grow_inplace(pointer, new_size).is_some() {
                return Some(pointer);
            }
        }

        let new_pointer = self.alloc(align, new_size)?;
        core::ptr::copy_nonoverlapping(pointer.as_ptr(), new_pointer.as_ptr(), current_size.bytes() as usize);
        self.free(pointer);

        Some(new_pointer)
    }

    /// Frees the memory pointed by `pointer`.
    ///
    /// # Safety
    ///
    /// The `pointer` must have come from [`Allocator::alloc`](Allocator::alloc), and must not have been passed to [`Allocator::free`](Allocator::free) beforehand.
    pub unsafe fn free(&mut self, pointer: NonNull<u8>) {
        let pointer = pointer.as_ptr();

        paranoid_assert!(!self.base_address.is_null());

        let mut chunk = Pointer::from_pointer(pointer).unchecked_sub(HEADER_SIZE).cast::<ChunkHeader>();
        self.paranoid_check_chunk(chunk);

        let size = unsafe { chunk.get_unchecked(self.base_address).size };
        let mut prev_chunk_size = unsafe { chunk.get_unchecked(self.base_address).prev_chunk_size };

        paranoid_assert!(size.is_allocated());
        let mut size = size.size();

        // Try to merge with the previous free chunk.
        if !Size::from_pointer_and_base_unchecked(chunk, Pointer::from_pointer_mut(self.base_address)).is_empty() {
            let prev_chunk = chunk.unchecked_sub(prev_chunk_size);
            self.paranoid_check_access(prev_chunk);

            let prev_size = unsafe { prev_chunk.get_unchecked(self.base_address).size };
            paranoid_assert_eq!(prev_size.size(), prev_chunk_size);

            if !prev_size.is_allocated() {
                prev_chunk_size = unsafe { prev_chunk.get_unchecked(self.base_address).prev_chunk_size };
                let prev_size = prev_size.size();
                self.unregister_free_space(prev_chunk.cast::<FreeChunkHeader>(), Self::size_to_bin_round_down(prev_size));
                size = size.unchecked_add(prev_size);
                chunk = chunk.unchecked_sub(prev_size);
            }
        }

        // Try to merge with the next free chunk.
        let end_of_address_space = Pointer::from_pointer(self.base_address).unchecked_add(self.env.total_space());
        {
            let next_chunk = chunk.unchecked_add(size);
            if next_chunk.cast() < end_of_address_space {
                self.paranoid_check_access(next_chunk);

                let next_size = unsafe { next_chunk.get_unchecked(self.base_address).size };
                if !next_size.is_allocated() {
                    let next_size = next_size.size();
                    self.unregister_free_space(next_chunk.cast::<FreeChunkHeader>(), Self::size_to_bin_round_down(next_size));
                    size = size.unchecked_add(next_size);
                }
            }
        }

        let chunk = chunk.cast::<FreeChunkHeader>();
        self.register_free_space(chunk, prev_chunk_size, size);

        let next_chunk = chunk.unchecked_add(size);
        if next_chunk.cast() < end_of_address_space {
            self.paranoid_check_access(next_chunk);
            unsafe {
                next_chunk.get_mut_unchecked(self.base_address).prev_chunk_size = size;
            };
        }

        self.paranoid_check_chunk(chunk.cast());
    }

    /// Returns the amount of usable space in the memory pointed by `pointer`.
    ///
    /// # Safety
    ///
    /// The `pointer` must have come from [`Allocator::alloc`](Allocator::alloc), and must not have been passed to [`Allocator::free`](Allocator::free) beforehand.
    #[inline]
    pub unsafe fn usable_size(pointer: NonNull<u8>) -> usize {
        Self::usable_size_impl(pointer).bytes() as usize
    }

    #[inline]
    unsafe fn usable_size_impl(pointer: NonNull<u8>) -> Size {
        Self::header_for_pointer(pointer.as_ptr()).size.size().unchecked_sub(HEADER_SIZE)
    }

    #[inline]
    unsafe fn header_for_pointer<'a>(pointer: *mut u8) -> &'a ChunkHeader {
        unsafe { &*pointer.byte_sub(HEADER_SIZE.bytes() as usize).cast::<ChunkHeader>() }
    }
}

#[cfg(target_has_atomic = "8")]
unsafe impl<E: crate::Env> core::alloc::GlobalAlloc for crate::Mutex<Allocator<E>> {
    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        let Some(align) = Size::from_bytes_usize(layout.align()) else {
            return core::ptr::null_mut();
        };

        let Some(size) = Size::from_bytes_usize(layout.size()) else {
            return core::ptr::null_mut();
        };

        if let Some(pointer) = self.lock().alloc(align, size) {
            pointer.as_ptr()
        } else {
            core::ptr::null_mut()
        }
    }

    unsafe fn alloc_zeroed(&self, layout: core::alloc::Layout) -> *mut u8 {
        let Some(align) = Size::from_bytes_usize(layout.align()) else {
            return core::ptr::null_mut();
        };

        let Some(size) = Size::from_bytes_usize(layout.size()) else {
            return core::ptr::null_mut();
        };

        if let Some(pointer) = self.lock().alloc_zeroed(align, size) {
            pointer.as_ptr()
        } else {
            core::ptr::null_mut()
        }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: core::alloc::Layout, new_size: usize) -> *mut u8 {
        let Some(pointer) = NonNull::new(pointer) else {
            return core::ptr::null_mut();
        };

        let Some(align) = Size::from_bytes_usize(layout.align()) else {
            return core::ptr::null_mut();
        };

        let Some(new_size) = Size::from_bytes_usize(new_size) else {
            return core::ptr::null_mut();
        };

        if let Some(pointer) = self.lock().realloc(pointer, align, new_size) {
            pointer.as_ptr()
        } else {
            core::ptr::null_mut()
        }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, _layout: core::alloc::Layout) {
        if let Some(pointer) = NonNull::new(pointer) {
            self.lock().free(pointer);
        }
    }
}
