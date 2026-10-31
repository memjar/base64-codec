//! # AVX2 backend
//!
//! The SSSE3 method over 256-bit vectors, 32 characters into 24 bytes per
//! decode step and 24 bytes into 32 characters per encode step, as encodify
//! 1.0.2 implements it (src/base64/kernel/x86.rs). The shuffles stay within
//! each 128-bit lane, so the 12 bytes each lane packs are stored apart, or
//! moved together first for an exact store, and an encode step loads its 24
//! bytes from 4 bytes before them, which lands the 12 bytes of each lane in
//! place. What is shorter than one block goes to the SSSE3 loops, which AVX2
//! implies.

#[cfg(target_arch = "x86")]
use core::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use core::arch::x86_64::*;
use core::mem::MaybeUninit;

base64_driver!(
    #[target_feature(enable = "avx2")]
    decode(
        chars: 32,
        spare: 4,
        wide: decode_wide,
        exact: decode_exact,
        narrow: crate::ssse3::decode_run,
    ),
    encode(
        bytes: 24,
        window: 32,
        lead: 4,
        wide: encode_wide,
        exact: encode_exact,
        narrow: crate::ssse3::encode_run,
    ),
);

/// The characters outside the alphabet that blocks met, as non-zero bytes.
#[derive(Clone, Copy)]
struct Bad(__m256i);

impl Bad {
    #[inline]
    #[target_feature(enable = "avx2")]
    fn none() -> Self {
        Self(_mm256_setzero_si256())
    }

    #[inline]
    #[target_feature(enable = "avx2")]
    fn join(self, other: Self) -> Self {
        Self(_mm256_or_si256(self.0, other.0))
    }

    #[inline]
    #[target_feature(enable = "avx2")]
    fn any(self) -> bool {
        _mm256_testz_si256(self.0, self.0) == 0
    }

    /// The characters of the whole quanta before the first character
    /// outside the alphabet.
    #[inline]
    #[target_feature(enable = "avx2")]
    fn first(self) -> usize {
        let valid = _mm256_movemask_epi8(_mm256_cmpeq_epi8(self.0, _mm256_setzero_si256()));
        (valid as u32).trailing_ones() as usize / 4 * 4
    }
}

/// Decodes 32 characters into the first 24 bytes of `out`, its last 4 left
/// spare.
#[inline]
#[target_feature(enable = "avx2")]
fn decode_wide(chars: &[u8; 32], out: &mut [MaybeUninit<u8>; 28]) -> Bad {
    let (bytes, bad) = decode(chars);

    // SAFETY: `out` holds 28 bytes: the first store writes bytes 0 to 15,
    // the second bytes 12 to 27, over the first lane's 4 spare ones.
    unsafe {
        _mm_storeu_si128(out.as_mut_ptr().cast(), _mm256_castsi256_si128(bytes));
        _mm_storeu_si128(
            out.as_mut_ptr().add(12).cast(),
            _mm256_extracti128_si256::<1>(bytes),
        );
    }

    bad
}

/// Decodes 32 characters into 24 bytes, moved together before they are
/// stored.
#[inline]
#[target_feature(enable = "avx2")]
fn decode_exact(chars: &[u8; 32], out: &mut [MaybeUninit<u8>; 24]) -> Bad {
    let (bytes, bad) = decode(chars);
    let bytes = _mm256_permutevar8x32_epi32(bytes, _mm256_setr_epi32(0, 1, 2, 4, 5, 6, 7, 7));

    // SAFETY: `out` holds 24 bytes, the first 16 for the first store and
    // the last 8 for the second.
    unsafe {
        _mm_storeu_si128(out.as_mut_ptr().cast(), _mm256_castsi256_si128(bytes));
        _mm_storel_epi64(
            out.as_mut_ptr().add(16).cast(),
            _mm256_extracti128_si256::<1>(bytes),
        );
    }

    bad
}

/// Encodes the 24 bytes of `window` from its fifth into 32 characters, its
/// first 4 and last 4 bytes only loaded along.
#[inline]
#[target_feature(enable = "avx2")]
fn encode_wide(window: &[u8; 32], out: &mut [MaybeUninit<u8>; 32]) {
    // SAFETY: `window` holds the 32 bytes the load reads.
    let bytes = unsafe { _mm256_loadu_si256(window.as_ptr().cast()) };

    // NOTE: the low lane holds its groups from its fifth byte on, the high
    // lane from its first.
    let spread = _mm256_shuffle_epi8(
        bytes,
        _mm256_setr_epi8(
            5, 4, 6, 5, 8, 7, 9, 8, 11, 10, 12, 11, 14, 13, 15, 14, 1, 0, 2, 1, 4, 3, 5, 4, 7, 6,
            8, 7, 10, 9, 11, 10,
        ),
    );

    // SAFETY: `out` holds the 32 bytes the store writes.
    unsafe { _mm256_storeu_si256(out.as_mut_ptr().cast(), ascii(split(spread))) };
}

/// Encodes 24 bytes into 32 characters, reading no byte around them: the
/// two halves are loaded into their lanes apart.
#[inline]
#[target_feature(enable = "avx2")]
fn encode_exact(bytes: &[u8; 24], out: &mut [MaybeUninit<u8>; 32]) {
    // SAFETY: `bytes` holds 24 bytes, the loads read the first 16 and the
    // last 16.
    let (low, high) = unsafe {
        (
            _mm_loadu_si128(bytes.as_ptr().cast()),
            _mm_loadu_si128(bytes.as_ptr().add(8).cast()),
        )
    };

    // NOTE: the high lane holds bytes 8 to 23, its groups from its fifth
    // byte on.
    let spread = _mm256_shuffle_epi8(
        _mm256_set_m128i(high, low),
        _mm256_setr_epi8(
            1, 0, 2, 1, 4, 3, 5, 4, 7, 6, 8, 7, 10, 9, 11, 10, 5, 4, 6, 5, 8, 7, 9, 8, 11, 10, 12,
            11, 14, 13, 15, 14,
        ),
    );

    // SAFETY: `out` holds the 32 bytes the store writes.
    unsafe { _mm256_storeu_si256(out.as_mut_ptr().cast(), ascii(split(spread))) };
}

/// The 256-bit `decode` of the SSSE3 backend, twelve bytes first in each
/// lane.
///
/// The low-nibble table holds the classes each low nibble allows, the
/// complement of the SSSE3 one, and is looked up with the characters as they
/// are: a character from 128 up looks up zero, which allows nothing. A
/// character whose class its nibble does not allow is then a non-zero byte.
#[inline]
#[target_feature(enable = "avx2")]
fn decode(chars: &[u8; 32]) -> (__m256i, Bad) {
    // SAFETY: `chars` holds the 32 bytes the load reads.
    let chars = unsafe { _mm256_loadu_si256(chars.as_ptr().cast()) };
    let high = _mm256_and_si256(_mm256_srli_epi32::<4>(chars), _mm256_set1_epi8(0x0f));

    let allowed = _mm256_shuffle_epi8(
        _mm256_broadcastsi128_si256(_mm_setr_epi8(
            !0x15_u8 as i8,
            !0x11_u8 as i8,
            !0x11_u8 as i8,
            !0x11_u8 as i8,
            !0x11_u8 as i8,
            !0x11_u8 as i8,
            !0x11_u8 as i8,
            !0x11_u8 as i8,
            !0x11_u8 as i8,
            !0x11_u8 as i8,
            !0x13_u8 as i8,
            !0x1a_u8 as i8,
            !0x1b_u8 as i8,
            !0x1b_u8 as i8,
            !0x1b_u8 as i8,
            !0x1a_u8 as i8,
        )),
        chars,
    );

    let class = _mm256_shuffle_epi8(
        _mm256_broadcastsi128_si256(_mm_setr_epi8(
            0x10, 0x10, 0x01, 0x02, 0x04, 0x08, 0x04, 0x08, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10,
            0x10, 0x10,
        )),
        high,
    );

    let slash = _mm256_cmpeq_epi8(chars, _mm256_set1_epi8(b'/' as i8));

    let offsets = _mm256_shuffle_epi8(
        _mm256_broadcastsi128_si256(_mm_setr_epi8(
            0, 16, 19, 4, -65, -65, -71, -71, 0, 0, 0, 0, 0, 0, 0, 0,
        )),
        _mm256_add_epi8(slash, high),
    );

    let sextets = _mm256_add_epi8(chars, offsets);
    let pairs = _mm256_maddubs_epi16(sextets, _mm256_set1_epi32(0x0140_0140));
    let quads = _mm256_madd_epi16(pairs, _mm256_set1_epi32(0x0001_1000));

    let bytes = _mm256_shuffle_epi8(
        quads,
        _mm256_broadcastsi128_si256(_mm_setr_epi8(
            2, 1, 0, 6, 5, 4, 10, 9, 8, 14, 13, 12, -1, -1, -1, -1,
        )),
    );

    (bytes, Bad(_mm256_andnot_si256(allowed, class)))
}

/// The 256-bit `split` of the SSSE3 backend.
#[inline]
#[target_feature(enable = "avx2")]
fn split(spread: __m256i) -> __m256i {
    let first = _mm256_srli_epi16::<10>(_mm256_mullo_epi16(
        spread,
        opaque(_mm256_set1_epi32(0x0010_0001)),
    ));

    let second = _mm256_mullo_epi16(
        _mm256_and_si256(spread, _mm256_set1_epi32(0x003f_03f0)),
        opaque(_mm256_set1_epi32(0x0100_0010)),
    );

    _mm256_or_si256(first, second)
}

/// The 256-bit `ascii` of the SSSE3 backend.
#[inline]
#[target_feature(enable = "avx2")]
fn ascii(sextets: __m256i) -> __m256i {
    let ranges = _mm256_sub_epi8(
        _mm256_subs_epu8(sextets, _mm256_set1_epi8(51)),
        _mm256_cmpgt_epi8(sextets, _mm256_set1_epi8(25)),
    );

    let offsets = _mm256_shuffle_epi8(
        _mm256_broadcastsi128_si256(_mm_setr_epi8(
            65, 71, -4, -4, -4, -4, -4, -4, -4, -4, -4, -4, -19, -16, 0, 0,
        )),
        ranges,
    );

    _mm256_add_epi8(sextets, offsets)
}

/// The 256-bit `opaque` of the SSSE3 backend.
#[cfg(not(miri))]
#[inline]
#[target_feature(enable = "avx2")]
fn opaque(mut value: __m256i) -> __m256i {
    // SAFETY: the template is empty: it leaves its register as it is, and
    // touches no other register, no flag and no memory.
    unsafe {
        core::arch::asm!(
            "/* {0} */",
            inout(ymm_reg) value,
            options(pure, nomem, nostack, preserves_flags),
        )
    };

    value
}

/// `value` as it is, since Miri runs no assembly.
#[cfg(miri)]
#[inline]
#[target_feature(enable = "avx2")]
fn opaque(value: __m256i) -> __m256i {
    value
}

#[cfg(test)]
mod tests {
    use crate::{avx2::*, scalar::*};

    #[test]
    fn kernels_match_scalar() {
        if !std::arch::is_x86_feature_detected!("avx2") {
            return;
        }

        // SAFETY: AVX2 was detected above.
        check_decode_kernel(|chars, out| unsafe { run(decode_exact(chars, out)) });
        // SAFETY: AVX2 was detected above.
        check_decode_kernel(|chars, out: &mut [MaybeUninit<u8>; 24]| unsafe {
            let mut wide = [MaybeUninit::new(0); 28];
            let read = run(decode_wide(chars, &mut wide));
            out.copy_from_slice(&wide[..24]);
            read
        });
        // SAFETY: AVX2 was detected above.
        check_encode_kernel(4, |window, out| unsafe { encode_wide(window, out) });
        // SAFETY: AVX2 was detected above.
        check_encode_kernel(0, |bytes, out| unsafe { encode_exact(bytes, out) });
    }

    #[target_feature(enable = "avx2")]
    fn run(bad: Bad) -> usize {
        if bad.any() { bad.first() } else { 32 }
    }
}
