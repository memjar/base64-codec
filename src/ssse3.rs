//! # SSSE3 backend
//!
//! Base64 over 128-bit vectors, by Wojciech Muła and Daniel Lemire's method
//! as encodify 1.0.2 implements it (src/base64/kernel/x86.rs): 16 characters
//! into 12 bytes per decode step, 12 bytes into 16 characters per encode
//! step, loaded from 4 bytes before them. Its loops also take what is shorter
//! than one AVX2 block.

#[cfg(target_arch = "x86")]
use core::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use core::arch::x86_64::*;
use core::mem::MaybeUninit;

base64_driver!(
    #[target_feature(enable = "ssse3")]
    decode(
        chars: 16,
        spare: 4,
        wide: decode_wide,
        exact: decode_exact,
        narrow: crate::scalar::decode_quanta,
    ),
    encode(
        bytes: 12,
        window: 16,
        lead: 4,
        wide: encode_wide,
        exact: encode_exact,
        narrow: crate::scalar::encode_groups,
    ),
);

/// The characters outside the alphabet that blocks met, as non-zero bytes.
#[derive(Clone, Copy)]
struct Bad(__m128i);

impl Bad {
    #[inline]
    #[target_feature(enable = "ssse3")]
    fn none() -> Self {
        Self(_mm_setzero_si128())
    }

    #[inline]
    #[target_feature(enable = "ssse3")]
    fn join(self, other: Self) -> Self {
        Self(_mm_or_si128(self.0, other.0))
    }

    #[inline]
    #[target_feature(enable = "ssse3")]
    fn any(self) -> bool {
        self.valid() != 0xffff
    }

    /// The characters of the whole quanta before the first character
    /// outside the alphabet.
    #[inline]
    #[target_feature(enable = "ssse3")]
    fn first(self) -> usize {
        self.valid().trailing_ones() as usize / 4 * 4
    }

    /// A bit per character, set when it is in the alphabet.
    #[inline]
    #[target_feature(enable = "ssse3")]
    fn valid(self) -> u32 {
        _mm_movemask_epi8(_mm_cmpeq_epi8(self.0, _mm_setzero_si128())) as u32
    }
}

/// Decodes 16 characters into the first 12 bytes of `out`, its last 4 left
/// spare.
#[inline]
#[target_feature(enable = "ssse3")]
fn decode_wide(chars: &[u8; 16], out: &mut [MaybeUninit<u8>; 16]) -> Bad {
    let (bytes, bad) = decode(chars);

    // SAFETY: `out` holds the 16 bytes the store writes.
    unsafe { _mm_storeu_si128(out.as_mut_ptr().cast(), bytes) };

    bad
}

/// Decodes 16 characters into 12 bytes.
#[inline]
#[target_feature(enable = "ssse3")]
fn decode_exact(chars: &[u8; 16], out: &mut [MaybeUninit<u8>; 12]) -> Bad {
    let (bytes, bad) = decode(chars);
    let tail = _mm_cvtsi128_si32(_mm_srli_si128::<8>(bytes));

    // SAFETY: `out` holds 12 bytes, the first 8 for the first store and the
    // last 4 for the second.
    unsafe {
        _mm_storel_epi64(out.as_mut_ptr().cast(), bytes);
        out.as_mut_ptr().add(8).cast::<i32>().write_unaligned(tail);
    }

    bad
}

/// Encodes the 12 bytes of `window` from its fifth into 16 characters, its
/// first 4 bytes only loaded along.
#[inline]
#[target_feature(enable = "ssse3")]
fn encode_wide(window: &[u8; 16], out: &mut [MaybeUninit<u8>; 16]) {
    // SAFETY: `window` holds the 16 bytes the load reads.
    let bytes = unsafe { _mm_loadu_si128(window.as_ptr().cast()) };
    let spread = _mm_shuffle_epi8(
        bytes,
        _mm_setr_epi8(5, 4, 6, 5, 8, 7, 9, 8, 11, 10, 12, 11, 14, 13, 15, 14),
    );

    // SAFETY: `out` holds the 16 bytes the store writes.
    unsafe { _mm_storeu_si128(out.as_mut_ptr().cast(), ascii(split(spread))) };
}

/// Encodes 12 bytes into 16 characters, reading no byte around them.
#[inline]
#[target_feature(enable = "ssse3")]
fn encode_exact(bytes: &[u8; 12], out: &mut [MaybeUninit<u8>; 16]) {
    let [low @ .., h0, h1, h2, h3] = *bytes;
    let high = i64::from(u32::from_le_bytes([h0, h1, h2, h3]));
    let bytes = _mm_set_epi64x(high, i64::from_le_bytes(low));

    let spread = _mm_shuffle_epi8(
        bytes,
        _mm_setr_epi8(1, 0, 2, 1, 4, 3, 5, 4, 7, 6, 8, 7, 10, 9, 11, 10),
    );

    // SAFETY: `out` holds the 16 bytes the store writes.
    unsafe { _mm_storeu_si128(out.as_mut_ptr().cast(), ascii(split(spread))) };
}

/// The 12 bytes of 16 characters, first in the vector, and a non-zero byte
/// for each character outside the alphabet.
///
/// Its high nibble picks a class of characters and its low nibble a bit in
/// that class, set when the pair is outside the alphabet; the high nibble,
/// with `/` set apart, also picks the offset to the sextet. The four sextets
/// of each 32-bit lane are then packed into three bytes.
#[inline]
#[target_feature(enable = "ssse3")]
fn decode(chars: &[u8; 16]) -> (__m128i, Bad) {
    // SAFETY: `chars` holds the 16 bytes the load reads.
    let chars = unsafe { _mm_loadu_si128(chars.as_ptr().cast()) };
    let nibble = _mm_set1_epi8(0x0f);
    let low = _mm_and_si128(chars, nibble);
    let high = _mm_and_si128(_mm_srli_epi32::<4>(chars), nibble);

    let bad = _mm_and_si128(
        _mm_shuffle_epi8(
            _mm_setr_epi8(
                0x15, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x13, 0x1a, 0x1b, 0x1b,
                0x1b, 0x1a,
            ),
            low,
        ),
        _mm_shuffle_epi8(
            _mm_setr_epi8(
                0x10, 0x10, 0x01, 0x02, 0x04, 0x08, 0x04, 0x08, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10,
                0x10, 0x10,
            ),
            high,
        ),
    );

    let slash = _mm_cmpeq_epi8(chars, _mm_set1_epi8(b'/' as i8));

    let offsets = _mm_shuffle_epi8(
        _mm_setr_epi8(0, 16, 19, 4, -65, -65, -71, -71, 0, 0, 0, 0, 0, 0, 0, 0),
        _mm_add_epi8(slash, high),
    );

    let sextets = _mm_add_epi8(chars, offsets);
    let pairs = _mm_maddubs_epi16(sextets, _mm_set1_epi32(0x0140_0140));
    let quads = _mm_madd_epi16(pairs, _mm_set1_epi32(0x0001_1000));

    let bytes = _mm_shuffle_epi8(
        quads,
        _mm_setr_epi8(2, 1, 0, 6, 5, 4, 10, 9, 8, 14, 13, 12, -1, -1, -1, -1),
    );

    (bytes, Bad(bad))
}

/// The four sextets of each 32-bit lane, from the three bytes spread over
/// it as `[b, a, c, b]`: each 16-bit half moves its two fields into place
/// with one low multiply, the first field shifted out of its high bits.
///
/// NOTE: the multipliers are powers of two hidden from the compiler, which
/// otherwise rewrites the multiplies into shifts and blends, several times
/// the cost.
#[inline]
#[target_feature(enable = "ssse3")]
fn split(spread: __m128i) -> __m128i {
    let first = _mm_srli_epi16::<10>(_mm_mullo_epi16(spread, opaque(_mm_set1_epi32(0x0010_0001))));

    let second = _mm_mullo_epi16(
        _mm_and_si128(spread, _mm_set1_epi32(0x003f_03f0)),
        opaque(_mm_set1_epi32(0x0100_0010)),
    );

    _mm_or_si128(first, second)
}

/// The characters of 16 sextets: each range of the alphabet, numbered by
/// saturating arithmetic, gets its offset from one lookup.
#[inline]
#[target_feature(enable = "ssse3")]
fn ascii(sextets: __m128i) -> __m128i {
    let ranges = _mm_sub_epi8(
        _mm_subs_epu8(sextets, _mm_set1_epi8(51)),
        _mm_cmpgt_epi8(sextets, _mm_set1_epi8(25)),
    );

    let offsets = _mm_shuffle_epi8(
        _mm_setr_epi8(
            65, 71, -4, -4, -4, -4, -4, -4, -4, -4, -4, -4, -19, -16, 0, 0,
        ),
        ranges,
    );

    _mm_add_epi8(sextets, offsets)
}

/// `value`, through an empty assembly block the compiler cannot see into,
/// yet free to hoist out of a loop since it is pure.
#[cfg(not(miri))]
#[inline]
#[target_feature(enable = "ssse3")]
fn opaque(mut value: __m128i) -> __m128i {
    // SAFETY: the template is empty: it leaves its register as it is, and
    // touches no other register, no flag and no memory.
    unsafe {
        core::arch::asm!(
            "/* {0} */",
            inout(xmm_reg) value,
            options(pure, nomem, nostack, preserves_flags),
        )
    };

    value
}

/// `value` as it is, since Miri runs no assembly.
#[cfg(miri)]
#[inline]
#[target_feature(enable = "ssse3")]
fn opaque(value: __m128i) -> __m128i {
    value
}

#[cfg(test)]
mod tests {
    use crate::{scalar::*, ssse3::*};

    #[test]
    fn kernels_match_scalar() {
        if !std::arch::is_x86_feature_detected!("ssse3") {
            return;
        }

        // SAFETY: SSSE3 was detected above.
        check_decode_kernel(|chars, out| unsafe { run(decode_exact(chars, out)) });
        // SAFETY: SSSE3 was detected above.
        check_decode_kernel(|chars, out: &mut [MaybeUninit<u8>; 12]| unsafe {
            let mut wide = [MaybeUninit::new(0); 16];
            let read = run(decode_wide(chars, &mut wide));
            out.copy_from_slice(&wide[..12]);
            read
        });
        // SAFETY: SSSE3 was detected above.
        check_encode_kernel(4, |window, out| unsafe { encode_wide(window, out) });
        // SAFETY: SSSE3 was detected above.
        check_encode_kernel(0, |bytes, out| unsafe { encode_exact(bytes, out) });
    }

    #[target_feature(enable = "ssse3")]
    fn run(bad: Bad) -> usize {
        if bad.any() { bad.first() } else { 16 }
    }
}
