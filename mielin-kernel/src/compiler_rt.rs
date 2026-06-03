//! Compiler Runtime Intrinsics
//!
//! Optimized low-level operations for the MielinOS kernel:
//! - Bulk memory operations (memcpy, memmove, memset, memcmp)
//! - Software arithmetic (64-bit division on 32-bit targets)
//! - Bit manipulation intrinsics (clz, ctz, popcount, bswap, rotl, rotr)
//! - Alignment helpers
//! - Bit-field extraction and insertion
//! - Constant-time buffer operations (for cryptographic use)
//! - Optional call statistics for profiling

#![allow(dead_code)]

use core::sync::atomic::{AtomicU64, Ordering};

// ---------------------------------------------------------------------------
// Bulk memory operations
// ---------------------------------------------------------------------------

/// Copy `n` bytes from `src` to `dst`. Regions must not overlap.
///
/// Uses an 8-byte-aligned fast path (word-at-a-time) when both pointers
/// are naturally aligned, then finishes with a byte-by-byte tail.
///
/// # Safety
/// - `dst` and `src` must be valid for `n` bytes.
/// - The regions must not overlap. Use [`mielin_memmove`] for overlapping regions.
#[inline]
pub unsafe fn mielin_memcpy(dst: *mut u8, src: *const u8, n: usize) {
    RT_STATS.memcpy_calls.fetch_add(1, Ordering::Relaxed);
    RT_STATS
        .total_bytes_copied
        .fetch_add(n as u64, Ordering::Relaxed);

    let mut i = 0usize;

    // 8-byte aligned fast path: copy 64-bit words at a time.
    if n >= 8 && (dst as usize).is_multiple_of(8) && (src as usize).is_multiple_of(8) {
        while i + 8 <= n {
            // SAFETY: both pointers are 8-byte aligned and within [0, n).
            let word = core::ptr::read_unaligned(src.add(i) as *const u64);
            core::ptr::write_unaligned(dst.add(i) as *mut u64, word);
            i += 8;
        }
    }

    // Byte-by-byte tail: handles the residual and the fully-unaligned case.
    while i < n {
        // SAFETY: i < n ensures both pointers are within their respective buffers.
        *dst.add(i) = *src.add(i);
        i += 1;
    }
}

/// Move `n` bytes from `src` to `dst`, correctly handling overlapping regions.
///
/// When `dst > src` and the regions overlap the copy proceeds backwards;
/// otherwise it proceeds forwards (same as `mielin_memcpy` in structure).
///
/// # Safety
/// - `dst` and `src` must each be valid for `n` bytes.
#[inline]
pub unsafe fn mielin_memmove(dst: *mut u8, src: *const u8, n: usize) {
    RT_STATS.memmove_calls.fetch_add(1, Ordering::Relaxed);

    if n == 0 {
        return;
    }

    let dst_addr = dst as usize;
    let src_addr = src as usize;

    if dst_addr == src_addr {
        // Same pointer — no work needed.
        return;
    }

    if dst_addr < src_addr || dst_addr >= src_addr + n {
        // Forward copy is safe: dst is entirely before the source window or
        // there is no overlap at all.
        let mut i = 0usize;
        if n >= 8 && dst_addr.is_multiple_of(8) && src_addr.is_multiple_of(8) {
            while i + 8 <= n {
                // SAFETY: aligned, in-bounds.
                let word = core::ptr::read_unaligned(src.add(i) as *const u64);
                core::ptr::write_unaligned(dst.add(i) as *mut u64, word);
                i += 8;
            }
        }
        while i < n {
            // SAFETY: i < n.
            *dst.add(i) = *src.add(i);
            i += 1;
        }
    } else {
        // Backward copy: dst is inside [src, src+n), so a forward copy would
        // overwrite source bytes before they are read.
        let mut i = n;

        // 8-byte aligned backward fast path.
        if n >= 8 && dst_addr.is_multiple_of(8) && src_addr.is_multiple_of(8) {
            while i >= 8 {
                i -= 8;
                // SAFETY: aligned, i < n, in-bounds.
                let word = core::ptr::read_unaligned(src.add(i) as *const u64);
                core::ptr::write_unaligned(dst.add(i) as *mut u64, word);
            }
        }
        while i > 0 {
            i -= 1;
            // SAFETY: i < n.
            *dst.add(i) = *src.add(i);
        }
    }
}

/// Fill `n` bytes at `dst` with `val`.
///
/// Uses a broadcast 64-bit word for an aligned fast path, then fills the
/// remaining bytes one at a time.
///
/// # Safety
/// - `dst` must be valid for `n` bytes.
#[inline]
pub unsafe fn mielin_memset(dst: *mut u8, val: u8, n: usize) {
    RT_STATS.memset_calls.fetch_add(1, Ordering::Relaxed);
    RT_STATS
        .total_bytes_set
        .fetch_add(n as u64, Ordering::Relaxed);

    let mut i = 0usize;

    // Broadcast `val` across all 8 bytes of a u64.
    let v = val as u64;
    let broadcast =
        v | (v << 8) | (v << 16) | (v << 24) | (v << 32) | (v << 40) | (v << 48) | (v << 56);

    // 8-byte aligned fast path.
    if n >= 8 && (dst as usize).is_multiple_of(8) {
        while i + 8 <= n {
            // SAFETY: aligned, in-bounds.
            core::ptr::write_unaligned(dst.add(i) as *mut u64, broadcast);
            i += 8;
        }
    }

    // Byte-by-byte tail.
    while i < n {
        // SAFETY: i < n.
        *dst.add(i) = val;
        i += 1;
    }
}

/// Compare `n` bytes at `a` and `b`.
///
/// Returns `0` if the regions are equal, a negative value if the first
/// differing byte of `a` is less than the corresponding byte of `b`,
/// or a positive value otherwise.
///
/// # Safety
/// - `a` and `b` must each be valid for `n` bytes.
#[inline]
pub unsafe fn mielin_memcmp(a: *const u8, b: *const u8, n: usize) -> i32 {
    RT_STATS.memcmp_calls.fetch_add(1, Ordering::Relaxed);

    for i in 0..n {
        // SAFETY: i < n.
        let av = *a.add(i) as i32;
        let bv = *b.add(i) as i32;
        let diff = av - bv;
        if diff != 0 {
            return diff;
        }
    }
    0
}

// ---------------------------------------------------------------------------
// Bit manipulation — 32-bit
// ---------------------------------------------------------------------------

/// Count leading zeros in a `u32`. Returns `32` when the input is `0`.
#[inline]
pub fn mielin_clz32(x: u32) -> u32 {
    x.leading_zeros()
}

/// Count trailing zeros in a `u32`. Returns `32` when the input is `0`.
#[inline]
pub fn mielin_ctz32(x: u32) -> u32 {
    x.trailing_zeros()
}

/// Population count: number of set bits in a `u32`.
#[inline]
pub fn mielin_popcount32(x: u32) -> u32 {
    x.count_ones()
}

/// Reverse the byte order of a `u32` (`0x12345678` → `0x78563412`).
#[inline]
pub fn mielin_bswap32(x: u32) -> u32 {
    x.swap_bytes()
}

/// Rotate `x` left by `n` bits (mod 32).
#[inline]
pub fn mielin_rotl32(x: u32, n: u32) -> u32 {
    x.rotate_left(n)
}

/// Rotate `x` right by `n` bits (mod 32).
#[inline]
pub fn mielin_rotr32(x: u32, n: u32) -> u32 {
    x.rotate_right(n)
}

// ---------------------------------------------------------------------------
// Bit manipulation — 64-bit
// ---------------------------------------------------------------------------

/// Count leading zeros in a `u64`. Returns `64` when the input is `0`.
#[inline]
pub fn mielin_clz64(x: u64) -> u32 {
    x.leading_zeros()
}

/// Count trailing zeros in a `u64`. Returns `64` when the input is `0`.
#[inline]
pub fn mielin_ctz64(x: u64) -> u32 {
    x.trailing_zeros()
}

/// Population count: number of set bits in a `u64`.
#[inline]
pub fn mielin_popcount64(x: u64) -> u32 {
    x.count_ones()
}

/// Reverse the byte order of a `u64`
/// (`0x0102030405060708` → `0x0807060504030201`).
#[inline]
pub fn mielin_bswap64(x: u64) -> u64 {
    x.swap_bytes()
}

/// Rotate `x` left by `n` bits (mod 64).
#[inline]
pub fn mielin_rotl64(x: u64, n: u32) -> u64 {
    x.rotate_left(n)
}

/// Rotate `x` right by `n` bits (mod 64).
#[inline]
pub fn mielin_rotr64(x: u64, n: u32) -> u64 {
    x.rotate_right(n)
}

// ---------------------------------------------------------------------------
// Software 64-bit integer arithmetic
// ---------------------------------------------------------------------------

/// 64-bit unsigned division using a portable bit-shift long-division algorithm.
///
/// Computes and returns `(quotient, remainder)`.  The algorithm is a classic
/// non-restoring binary long division: it processes one bit of the dividend per
/// iteration (MSB first), building up the partial remainder and quotient.
///
/// This is suitable as a software fallback on 32-bit targets that lack a
/// hardware 64-bit divider.
///
/// # Panics
/// Panics if `divisor == 0`.
#[inline]
pub fn mielin_udivmod64(dividend: u64, divisor: u64) -> (u64, u64) {
    assert!(divisor != 0, "mielin_udivmod64: division by zero");

    RT_STATS.udivmod64_calls.fetch_add(1, Ordering::Relaxed);

    // Fast paths for common cases.
    if divisor > dividend {
        return (0, dividend);
    }
    if divisor == dividend {
        return (1, 0);
    }

    // Non-restoring binary long division: each iteration shifts one bit of
    // the dividend into the partial remainder from the most significant end.
    let mut quotient: u64 = 0;
    let mut remainder: u64 = 0;

    for i in (0..64u32).rev() {
        // Shift the next dividend bit into the LSB of the partial remainder.
        remainder = (remainder << 1) | ((dividend >> i) & 1);
        if remainder >= divisor {
            remainder -= divisor;
            quotient |= 1u64 << i;
        }
    }

    (quotient, remainder)
}

/// 64-bit signed division.
///
/// Returns `(quotient, remainder)` obeying the invariant
/// `quotient * divisor + remainder == dividend`, where the remainder carries
/// the sign of the dividend (truncated-toward-zero semantics, matching Rust's
/// built-in `/` and `%` operators).
///
/// Delegates to [`mielin_udivmod64`] after normalising signs.
///
/// # Panics
/// Panics if `divisor == 0`.
#[inline]
pub fn mielin_sdivmod64(dividend: i64, divisor: i64) -> (i64, i64) {
    assert!(divisor != 0, "mielin_sdivmod64: division by zero");

    RT_STATS.sdivmod64_calls.fetch_add(1, Ordering::Relaxed);

    let neg_dividend = dividend < 0;
    let neg_divisor = divisor < 0;

    // Compute absolute values via i128 to handle i64::MIN without overflow.
    let abs_dividend = (dividend as i128).unsigned_abs() as u64;
    let abs_divisor = (divisor as i128).unsigned_abs() as u64;

    let (q, r) = mielin_udivmod64(abs_dividend, abs_divisor);

    // Quotient is negative iff exactly one operand is negative.
    let quotient = if neg_dividend ^ neg_divisor {
        // Negate, guarding against the (unlikely) case where q == i64::MAX + 1.
        (q as i128).wrapping_neg() as i64
    } else {
        q as i64
    };

    // Remainder carries the sign of the dividend.
    let remainder = if neg_dividend {
        (r as i128).wrapping_neg() as i64
    } else {
        r as i64
    };

    (quotient, remainder)
}

// ---------------------------------------------------------------------------
// Saturating arithmetic
// ---------------------------------------------------------------------------

/// Saturating unsigned addition for `u32`.
#[inline]
pub fn mielin_saturating_add_u32(a: u32, b: u32) -> u32 {
    a.saturating_add(b)
}

/// Saturating unsigned subtraction for `u32`.
#[inline]
pub fn mielin_saturating_sub_u32(a: u32, b: u32) -> u32 {
    a.saturating_sub(b)
}

/// Saturating unsigned addition for `u64`.
#[inline]
pub fn mielin_saturating_add_u64(a: u64, b: u64) -> u64 {
    a.saturating_add(b)
}

/// Saturating unsigned subtraction for `u64`.
#[inline]
pub fn mielin_saturating_sub_u64(a: u64, b: u64) -> u64 {
    a.saturating_sub(b)
}

// ---------------------------------------------------------------------------
// Alignment helpers
// ---------------------------------------------------------------------------

/// Round `addr` up to the nearest multiple of `align` (must be a power of two).
#[inline]
pub fn mielin_align_up(addr: usize, align: usize) -> usize {
    debug_assert!(align.is_power_of_two(), "align must be a power of two");
    (addr + align - 1) & !(align - 1)
}

/// Round `addr` down to the nearest multiple of `align` (must be a power of two).
#[inline]
pub fn mielin_align_down(addr: usize, align: usize) -> usize {
    debug_assert!(align.is_power_of_two(), "align must be a power of two");
    addr & !(align - 1)
}

/// Return `true` if `addr` is a multiple of `align` (must be a power of two).
#[inline]
pub fn mielin_is_aligned(addr: usize, align: usize) -> bool {
    debug_assert!(align.is_power_of_two(), "align must be a power of two");
    addr & (align - 1) == 0
}

// ---------------------------------------------------------------------------
// Bit-field extraction and insertion
// ---------------------------------------------------------------------------

/// Extract a `width`-bit field from `val` starting at bit `offset` (LSB = 0).
///
/// `width` must be in `[1, 64]` and `offset + width` must be `<= 64`.
#[inline]
pub fn mielin_extract_bits(val: u64, offset: u32, width: u32) -> u64 {
    debug_assert!((1..=64).contains(&width), "width out of range [1, 64]");
    debug_assert!(offset + width <= 64, "offset + width exceeds 64 bits");

    let mask = if width == 64 {
        u64::MAX
    } else {
        (1u64 << width) - 1
    };
    (val >> offset) & mask
}

/// Replace a `width`-bit field at bit `offset` in `val` with `field`.
///
/// Bits of `val` outside the field are preserved unchanged.  `field` is
/// masked to `width` bits before insertion so that only the low `width` bits
/// of `field` are used.
///
/// `width` must be in `[1, 64]` and `offset + width` must be `<= 64`.
#[inline]
pub fn mielin_insert_bits(val: u64, field: u64, offset: u32, width: u32) -> u64 {
    debug_assert!((1..=64).contains(&width), "width out of range [1, 64]");
    debug_assert!(offset + width <= 64, "offset + width exceeds 64 bits");

    let mask = if width == 64 {
        u64::MAX
    } else {
        (1u64 << width) - 1
    };
    let cleared = val & !(mask << offset);
    cleared | ((field & mask) << offset)
}

// ---------------------------------------------------------------------------
// Constant-time and security helpers
// ---------------------------------------------------------------------------

/// Compare two byte slices in constant time (no early exit on first mismatch).
///
/// Returns `true` iff both slices have the same length and identical contents.
/// Execution time depends only on `a.len()`, making this suitable for
/// comparing secrets such as MACs or password hashes.
#[inline]
pub fn mielin_const_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    // Accumulate all byte differences via bitwise-OR — every byte pair is
    // always examined regardless of where the first mismatch occurs.
    let mut diff: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Zero-fill `buf` using volatile writes so the compiler cannot elide the stores.
///
/// Primarily used to erase cryptographic key material from memory.
#[inline]
pub fn mielin_zeroize(buf: &mut [u8]) {
    for byte in buf.iter_mut() {
        // SAFETY: `byte` is a valid mutable reference obtained from a slice
        // iterator.  Volatile write prevents the compiler from optimising away
        // stores that appear "dead" because the buffer is not read afterwards.
        unsafe { core::ptr::write_volatile(byte as *mut u8, 0u8) };
    }
}

// ---------------------------------------------------------------------------
// String / encoding helpers
// ---------------------------------------------------------------------------

/// Return the index of the first `0x00` byte in `buf`, or `buf.len()` if none
/// exists (analogous to C `strlen`, but bounded by `buf.len()`).
#[inline]
pub fn mielin_strlen(buf: &[u8]) -> usize {
    buf.iter().position(|&b| b == 0).unwrap_or(buf.len())
}

/// Hex-encode `src` bytes into `dst` using lowercase digits (`0–9`, `a–f`).
///
/// `dst` must be at least `2 * src.len()` bytes long.
///
/// # Panics
/// Panics if `dst.len() < 2 * src.len()`.
#[inline]
pub fn mielin_hex_encode(src: &[u8], dst: &mut [u8]) {
    assert!(
        dst.len() >= 2 * src.len(),
        "mielin_hex_encode: dst buffer too small ({} < {})",
        dst.len(),
        2 * src.len()
    );

    const HEX: &[u8; 16] = b"0123456789abcdef";
    for (i, &byte) in src.iter().enumerate() {
        dst[2 * i] = HEX[(byte >> 4) as usize];
        dst[2 * i + 1] = HEX[(byte & 0x0F) as usize];
    }
}

// ---------------------------------------------------------------------------
// Runtime call statistics
// ---------------------------------------------------------------------------

/// Global atomic counters for compiler-runtime call statistics.
static RT_STATS: RtStatsAtomic = RtStatsAtomic::new();

struct RtStatsAtomic {
    memcpy_calls: AtomicU64,
    memmove_calls: AtomicU64,
    memset_calls: AtomicU64,
    memcmp_calls: AtomicU64,
    udivmod64_calls: AtomicU64,
    sdivmod64_calls: AtomicU64,
    total_bytes_copied: AtomicU64,
    total_bytes_set: AtomicU64,
}

impl RtStatsAtomic {
    const fn new() -> Self {
        Self {
            memcpy_calls: AtomicU64::new(0),
            memmove_calls: AtomicU64::new(0),
            memset_calls: AtomicU64::new(0),
            memcmp_calls: AtomicU64::new(0),
            udivmod64_calls: AtomicU64::new(0),
            sdivmod64_calls: AtomicU64::new(0),
            total_bytes_copied: AtomicU64::new(0),
            total_bytes_set: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> CompilerRtStats {
        CompilerRtStats {
            memcpy_calls: self.memcpy_calls.load(Ordering::Relaxed),
            memmove_calls: self.memmove_calls.load(Ordering::Relaxed),
            memset_calls: self.memset_calls.load(Ordering::Relaxed),
            memcmp_calls: self.memcmp_calls.load(Ordering::Relaxed),
            udivmod64_calls: self.udivmod64_calls.load(Ordering::Relaxed),
            sdivmod64_calls: self.sdivmod64_calls.load(Ordering::Relaxed),
            total_bytes_copied: self.total_bytes_copied.load(Ordering::Relaxed),
            total_bytes_set: self.total_bytes_set.load(Ordering::Relaxed),
        }
    }

    fn reset(&self) {
        self.memcpy_calls.store(0, Ordering::Relaxed);
        self.memmove_calls.store(0, Ordering::Relaxed);
        self.memset_calls.store(0, Ordering::Relaxed);
        self.memcmp_calls.store(0, Ordering::Relaxed);
        self.udivmod64_calls.store(0, Ordering::Relaxed);
        self.sdivmod64_calls.store(0, Ordering::Relaxed);
        self.total_bytes_copied.store(0, Ordering::Relaxed);
        self.total_bytes_set.store(0, Ordering::Relaxed);
    }
}

/// Snapshot of compiler-runtime call and byte-volume statistics.
///
/// Obtain a current snapshot with [`compiler_rt_stats`] and reset the global
/// counters with [`reset_compiler_rt_stats`].
#[derive(Debug, Default, Clone)]
pub struct CompilerRtStats {
    pub memcpy_calls: u64,
    pub memmove_calls: u64,
    pub memset_calls: u64,
    pub memcmp_calls: u64,
    pub udivmod64_calls: u64,
    pub sdivmod64_calls: u64,
    pub total_bytes_copied: u64,
    pub total_bytes_set: u64,
}

impl CompilerRtStats {
    /// Create a zeroed statistics snapshot.
    pub fn new() -> Self {
        Self::default()
    }

    /// Return the sum of all per-function call counters.
    pub fn total_calls(&self) -> u64 {
        self.memcpy_calls
            + self.memmove_calls
            + self.memset_calls
            + self.memcmp_calls
            + self.udivmod64_calls
            + self.sdivmod64_calls
    }

    /// Reset all fields to zero.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// Take a snapshot of the global runtime statistics counters.
pub fn compiler_rt_stats() -> CompilerRtStats {
    RT_STATS.snapshot()
}

/// Reset all global runtime statistics counters to zero.
pub fn reset_compiler_rt_stats() {
    RT_STATS.reset();
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // memcpy (5 tests)
    // -----------------------------------------------------------------------

    #[test]
    fn test_memcpy_aligned_8byte() {
        let src: Vec<u8> = (0u8..64).collect();
        let mut dst = vec![0u8; 64];

        // SAFETY: src and dst are valid, non-overlapping, 64-byte heap buffers.
        unsafe { mielin_memcpy(dst.as_mut_ptr(), src.as_ptr(), 64) };

        assert_eq!(dst, src, "64-byte copy must match the source exactly");
    }

    #[test]
    fn test_memcpy_unaligned() {
        // Use a 1-byte offset pointer to force the byte-by-byte path.
        let src: Vec<u8> = vec![0xAAu8, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, 0x11];
        let mut dst = vec![0u8; 8];

        // SAFETY: src.len() == 7, dst has 8 bytes — valid, non-overlapping.
        unsafe { mielin_memcpy(dst.as_mut_ptr().add(1), src.as_ptr(), 7) };

        assert_eq!(&dst[1..8], src.as_slice());
    }

    #[test]
    fn test_memcpy_zero_len() {
        let src = [0xFFu8; 8];
        let mut dst = [0u8; 8];

        // SAFETY: n == 0, no bytes are accessed.
        unsafe { mielin_memcpy(dst.as_mut_ptr(), src.as_ptr(), 0) };

        assert!(
            dst.iter().all(|&b| b == 0),
            "zero-length copy must not modify dst"
        );
    }

    #[test]
    fn test_memcpy_single_byte() {
        let src = [0x42u8];
        let mut dst = [0u8];

        // SAFETY: valid, non-overlapping, 1-byte buffers.
        unsafe { mielin_memcpy(dst.as_mut_ptr(), src.as_ptr(), 1) };

        assert_eq!(dst[0], 0x42);
    }

    #[test]
    fn test_memcpy_large() {
        let src: Vec<u8> = (0u8..=255).cycle().take(1024).collect();
        let mut dst = vec![0u8; 1024];

        // SAFETY: valid, non-overlapping, 1024-byte buffers.
        unsafe { mielin_memcpy(dst.as_mut_ptr(), src.as_ptr(), 1024) };

        assert_eq!(dst, src, "1024-byte copy must match the source exactly");
    }

    // -----------------------------------------------------------------------
    // memmove (4 tests)
    // -----------------------------------------------------------------------

    #[test]
    fn test_memmove_no_overlap() {
        let src: Vec<u8> = (1u8..=16).collect();
        let mut dst = vec![0u8; 16];

        // SAFETY: disjoint heap buffers, both valid for 16 bytes.
        unsafe { mielin_memmove(dst.as_mut_ptr(), src.as_ptr(), 16) };

        assert_eq!(dst, src);
    }

    #[test]
    fn test_memmove_overlap_fwd() {
        // src = buf[0..6], dst = buf[2..8] — dst > src, regions overlap.
        // A forward copy would corrupt the unread tail; backward copy is required.
        let mut buf = vec![0u8, 1, 2, 3, 4, 5, 0xFF, 0xFF];

        // SAFETY: overlapping regions within the same valid allocation.
        unsafe { mielin_memmove(buf.as_mut_ptr().add(2), buf.as_ptr(), 6) };

        assert_eq!(&buf[2..8], &[0u8, 1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_memmove_overlap_bck() {
        // src = buf[2..8], dst = buf[0..6] — dst < src, regions overlap.
        // A backward copy would corrupt the unread head; forward copy is required.
        let mut buf = vec![0xFFu8, 0xFF, 1, 2, 3, 4, 5, 6];

        // SAFETY: overlapping regions within the same valid allocation.
        unsafe { mielin_memmove(buf.as_mut_ptr(), buf.as_ptr().add(2), 6) };

        assert_eq!(&buf[0..6], &[1u8, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn test_memmove_same_ptr() {
        let mut buf = vec![0xABu8; 8];
        let expected = buf.clone();

        // SAFETY: dst == src, n == 8 — the implementation must not corrupt data.
        unsafe {
            let ptr = buf.as_mut_ptr();
            mielin_memmove(ptr, ptr as *const u8, 8);
        }

        assert_eq!(
            buf, expected,
            "same-pointer memmove must leave data unchanged"
        );
    }

    // -----------------------------------------------------------------------
    // memset / memcmp (3 tests)
    // -----------------------------------------------------------------------

    #[test]
    fn test_memset_fills_pattern() {
        let mut buf = vec![0u8; 32];

        // SAFETY: valid 32-byte buffer.
        unsafe { mielin_memset(buf.as_mut_ptr(), 0xAB, 32) };

        assert!(
            buf.iter().all(|&b| b == 0xAB),
            "every byte must be 0xAB after memset"
        );
    }

    #[test]
    fn test_memcmp_equal() {
        let a = [1u8, 2, 3, 4, 5];
        let b = [1u8, 2, 3, 4, 5];

        // SAFETY: valid, 5-byte buffers.
        let result = unsafe { mielin_memcmp(a.as_ptr(), b.as_ptr(), 5) };

        assert_eq!(result, 0, "identical buffers must compare equal (0)");
    }

    #[test]
    fn test_memcmp_different() {
        let a = [1u8, 2, 3, 4, 6];
        let b = [1u8, 2, 3, 4, 5];

        // SAFETY: valid, 5-byte buffers.
        let pos = unsafe { mielin_memcmp(a.as_ptr(), b.as_ptr(), 5) };
        assert!(pos > 0, "a[4]=6 > b[4]=5, result must be positive");

        // SAFETY: same buffers, swapped roles.
        let neg = unsafe { mielin_memcmp(b.as_ptr(), a.as_ptr(), 5) };
        assert!(neg < 0, "b[4]=5 < a[4]=6, result must be negative");
    }

    // -----------------------------------------------------------------------
    // Bit manipulation (8 tests)
    // -----------------------------------------------------------------------

    #[test]
    fn test_clz32_zero() {
        assert_eq!(mielin_clz32(0), 32, "clz32(0) must return 32");
    }

    #[test]
    fn test_clz32_one() {
        assert_eq!(mielin_clz32(1), 31, "clz32(1) must return 31");
    }

    #[test]
    fn test_ctz32() {
        assert_eq!(mielin_ctz32(8), 3, "ctz32(8) == 3  (bit 3 is set)");
        assert_eq!(mielin_ctz32(1), 0, "ctz32(1) == 0");
        assert_eq!(mielin_ctz32(0), 32, "ctz32(0) == 32");
    }

    #[test]
    fn test_popcount32() {
        assert_eq!(mielin_popcount32(0xFF), 8, "popcount32(0xFF) == 8");
        assert_eq!(mielin_popcount32(0), 0);
        assert_eq!(mielin_popcount32(u32::MAX), 32);
        assert_eq!(mielin_popcount32(0b1010_1010), 4);
    }

    #[test]
    fn test_bswap32() {
        assert_eq!(
            mielin_bswap32(0x12345678),
            0x78563412,
            "bswap32 must reverse all four bytes"
        );
        assert_eq!(mielin_bswap32(0x00000001), 0x01000000);
        assert_eq!(mielin_bswap32(0), 0);
    }

    #[test]
    fn test_rotl32() {
        assert_eq!(mielin_rotl32(1, 1), 2);
        assert_eq!(
            mielin_rotl32(0x8000_0000, 1),
            1,
            "rotating MSB left wraps to bit 0"
        );
        assert_eq!(
            mielin_rotl32(0xDEAD_BEEF, 0),
            0xDEAD_BEEF,
            "rotate by 0 is identity"
        );
        assert_eq!(
            mielin_rotl32(0xDEAD_BEEF, 32),
            0xDEAD_BEEF,
            "rotate by 32 is identity"
        );
    }

    #[test]
    fn test_popcount64() {
        assert_eq!(
            mielin_popcount64(u64::MAX),
            64,
            "popcount64(u64::MAX) == 64"
        );
        assert_eq!(mielin_popcount64(0), 0);
        assert_eq!(mielin_popcount64(0xFF), 8);
        assert_eq!(mielin_popcount64(0xFF00_FF00_FF00_FF00u64), 32);
    }

    #[test]
    fn test_bswap64() {
        assert_eq!(
            mielin_bswap64(0x0102_0304_0506_0708u64),
            0x0807_0605_0403_0201u64,
            "bswap64 must reverse all eight bytes"
        );
        assert_eq!(mielin_bswap64(0), 0);
        assert_eq!(mielin_bswap64(u64::MAX), u64::MAX);
    }

    // -----------------------------------------------------------------------
    // Arithmetic (5 tests)
    // -----------------------------------------------------------------------

    #[test]
    fn test_udivmod64_basic() {
        let (q, r) = mielin_udivmod64(100, 7);
        assert_eq!(q, 14, "100 / 7 == 14");
        assert_eq!(r, 2, "100 % 7 == 2");
    }

    #[test]
    fn test_udivmod64_large() {
        let dividend = 1u64 << 63; // 2^63
        let divisor = 3u64;
        let (q, r) = mielin_udivmod64(dividend, divisor);
        // Verify the fundamental invariant: q * d + r == n.
        assert_eq!(
            q.wrapping_mul(divisor).wrapping_add(r),
            dividend,
            "q * d + r must equal the original dividend"
        );
        assert!(
            r < divisor,
            "remainder must be strictly less than the divisor"
        );
    }

    #[test]
    fn test_udivmod64_exact() {
        let (q, r) = mielin_udivmod64(100, 5);
        assert_eq!(q, 20, "100 / 5 == 20");
        assert_eq!(r, 0, "100 % 5 == 0 (exact division)");
    }

    #[test]
    fn test_sdivmod64_negative() {
        // Truncated-toward-zero: -100 / 7 == -14 remainder -2.
        let (q, r) = mielin_sdivmod64(-100, 7);
        assert_eq!(q, -14);
        assert_eq!(r, -2);
        assert_eq!(q * 7 + r, -100, "invariant q*d+r==n must hold");
    }

    #[test]
    fn test_sdivmod64_both_neg() {
        // -100 / -7 == 14 remainder -2.
        let (q, r) = mielin_sdivmod64(-100, -7);
        assert_eq!(q, 14);
        assert_eq!(r, -2);
        assert_eq!(q * (-7) + r, -100, "invariant q*d+r==n must hold");
    }

    // -----------------------------------------------------------------------
    // Alignment + helpers (6 tests)
    // -----------------------------------------------------------------------

    #[test]
    fn test_align_up() {
        assert_eq!(mielin_align_up(13, 8), 16);
        assert_eq!(
            mielin_align_up(16, 8),
            16,
            "already-aligned value is unchanged"
        );
        assert_eq!(mielin_align_up(0, 8), 0);
        assert_eq!(mielin_align_up(1, 4096), 4096);
    }

    #[test]
    fn test_align_down() {
        assert_eq!(mielin_align_down(15, 8), 8);
        assert_eq!(
            mielin_align_down(16, 8),
            16,
            "already-aligned value is unchanged"
        );
        assert_eq!(mielin_align_down(0, 8), 0);
        assert_eq!(mielin_align_down(4095, 4096), 0);
    }

    #[test]
    fn test_extract_bits() {
        // 0xFF00 = 0b1111_1111_0000_0000
        // bits [11:8] == 0xF
        assert_eq!(mielin_extract_bits(0xFF00, 8, 4), 0xF);
        // 0xFF00 = ...1111_1111_0000_0000; bits [7:4] == 0x0 (lower byte is 0x00)
        assert_eq!(mielin_extract_bits(0xFF00, 4, 4), 0x0);
        // bits [7:0] == 0x00
        assert_eq!(mielin_extract_bits(0xFF00, 0, 8), 0x00);
        // full 64-bit extract
        assert_eq!(mielin_extract_bits(u64::MAX, 0, 64), u64::MAX);
    }

    #[test]
    fn test_insert_bits() {
        // Insert 0xA (4 bits wide) at offset 4 into 0x0000_00FF.
        // Bits [7:4] become 0xA; bits [3:0] stay 0xF → 0x0000_00AF.
        let base = 0x0000_00FFu64;
        let modified = mielin_insert_bits(base, 0xA, 4, 4);

        let extracted = mielin_extract_bits(modified, 4, 4);
        assert_eq!(
            extracted, 0xA,
            "round-trip: extracted value must equal the inserted value"
        );

        // Bits outside the field must be preserved.
        assert_eq!(
            mielin_extract_bits(modified, 0, 4),
            0xF,
            "lower nibble must be preserved"
        );
        assert_eq!(
            mielin_extract_bits(modified, 8, 56),
            0,
            "upper bits must be zero"
        );
    }

    #[test]
    fn test_const_time_eq() {
        let a = b"hello world";
        let b = b"hello world";
        let c = b"hello worle";
        let d = b"short";

        assert!(mielin_const_time_eq(a, b), "identical slices must be equal");
        assert!(
            !mielin_const_time_eq(a, c),
            "one differing byte must return false"
        );
        assert!(
            !mielin_const_time_eq(a, d),
            "different lengths must return false"
        );
        assert!(mielin_const_time_eq(&[], &[]), "empty slices are equal");
    }

    #[test]
    fn test_hex_encode() {
        let src = [0xDEu8, 0xAD];
        let mut dst = [0u8; 4];
        mielin_hex_encode(&src, &mut dst);
        assert_eq!(&dst, b"dead");

        let src2 = [0x00u8, 0xFF, 0xA5];
        let mut dst2 = [0u8; 6];
        mielin_hex_encode(&src2, &mut dst2);
        assert_eq!(&dst2, b"00ffa5");
    }

    #[test]
    fn test_strlen() {
        assert_eq!(mielin_strlen(b"hello\0garbage"), 5, "stop at first NUL");
        assert_eq!(mielin_strlen(b"abcde"), 5, "no NUL → return slice length");
        assert_eq!(mielin_strlen(b""), 0, "empty slice → 0");
        assert_eq!(mielin_strlen(b"\0hello"), 0, "leading NUL → 0");
    }

    // -----------------------------------------------------------------------
    // Additional tests (saturating, rotation inverse, clz/ctz 64-bit, etc.)
    // -----------------------------------------------------------------------

    #[test]
    fn test_saturating_arithmetic() {
        assert_eq!(mielin_saturating_add_u32(u32::MAX, 1), u32::MAX);
        assert_eq!(mielin_saturating_sub_u32(0, 1), 0);
        assert_eq!(mielin_saturating_add_u64(u64::MAX, 1), u64::MAX);
        assert_eq!(mielin_saturating_sub_u64(0, 1), 0);
        assert_eq!(mielin_saturating_add_u32(100, 200), 300);
        assert_eq!(mielin_saturating_sub_u64(1000, 400), 600);
    }

    #[test]
    fn test_rotr32_and_rotl32_inverse() {
        let x = 0x1234_5678u32;
        for n in 0u32..=32 {
            assert_eq!(
                mielin_rotr32(mielin_rotl32(x, n), n),
                x,
                "rotr(rotl(x, n), n) must be the identity for n={n}"
            );
        }
    }

    #[test]
    fn test_clz64_and_ctz64() {
        assert_eq!(mielin_clz64(0), 64);
        assert_eq!(mielin_clz64(1), 63);
        assert_eq!(mielin_clz64(u64::MAX), 0);
        assert_eq!(mielin_ctz64(0), 64);
        assert_eq!(mielin_ctz64(1), 0);
        assert_eq!(mielin_ctz64(8), 3);
        assert_eq!(mielin_ctz64(u64::MAX), 0);
    }

    #[test]
    fn test_udivmod64_divisor_one() {
        let n = 12_345_678_901_234_567_890u64;
        let (q, r) = mielin_udivmod64(n, 1);
        assert_eq!(q, n);
        assert_eq!(r, 0);
    }

    #[test]
    fn test_is_aligned() {
        assert!(mielin_is_aligned(0, 8));
        assert!(mielin_is_aligned(8, 8));
        assert!(!mielin_is_aligned(9, 8));
        assert!(mielin_is_aligned(4096, 4096));
        assert!(!mielin_is_aligned(4097, 4096));
    }

    #[test]
    fn test_stats_accumulate() {
        reset_compiler_rt_stats();

        let src = [1u8, 2, 3, 4];
        let mut dst = [0u8; 4];

        // SAFETY: valid, non-overlapping, 4-byte stack buffers.
        unsafe { mielin_memcpy(dst.as_mut_ptr(), src.as_ptr(), 4) };

        let stats = compiler_rt_stats();
        assert!(
            stats.memcpy_calls >= 1,
            "at least one memcpy call must be recorded"
        );
        assert!(
            stats.total_bytes_copied >= 4,
            "at least 4 bytes copied must be recorded"
        );
        assert!(stats.total_calls() >= 1);
    }

    #[test]
    fn test_zeroize() {
        let mut secret = vec![0xFFu8; 32];
        mielin_zeroize(&mut secret);
        assert!(
            secret.iter().all(|&b| b == 0),
            "zeroize must clear every byte to 0"
        );
    }
}
