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
        kernel: decode,
        narrow: crate::ssse3::decode_run,
    ),
    encode(
        bytes: 24,
        window: 32,
        lead: 4,
        kernel: encode,
        narrow: crate::ssse3::encode_run,
    ),
);

/// The classes the low nibbles of a block's characters allow, and the classes
/// of their high nibbles: a character whose class its low nibble does not
/// allow is outside the alphabet.
#[derive(Clone, Copy)]
struct Bad(__m256i, __m256i);

impl Bad {
    /// One `vptest` per block costs less than joining the blocks of a group.
    const EAGER: bool = true;

    #[target_feature(enable = "avx2")]
    fn none() -> Self {
        Self(_mm256_set1_epi8(-1), _mm256_setzero_si256())
    }

    fn join(self, _: Self) -> Self {
        self
    }

    #[target_feature(enable = "avx2")]
    fn any(self) -> bool {
        _mm256_testc_si256(self.0, self.1) == 0
    }

    /// The characters of the whole quanta before the first one outside the
    /// alphabet.
    #[target_feature(enable = "avx2")]
    fn first(self) -> usize {
        let bad = _mm256_cmpeq_epi8(_mm256_andnot_si256(self.0, self.1), _mm256_setzero_si256());
        (_mm256_movemask_epi8(bad) as u32).trailing_ones() as usize / 4 * 4
    }
}

/// Decodes 32 characters into the first 24 bytes of `out` by the SSSE3 method
/// over both lanes, plus 4 spare ones when it holds 28: the first 32 of
/// `chars`, or its first `split` and its last ones.
///
/// The low-nibble table holds the classes each low nibble allows, the
/// complement of the SSSE3 one.
#[inline]
#[target_feature(enable = "avx2")]
fn decode<const C: usize, const N: usize>(
    chars: &[u8; C],
    split: usize,
    out: &mut [MaybeUninit<u8>; N],
) -> Bad {
    const { assert!(C >= 32 && (N == 24 || N == 28)) };

    // SAFETY: the loads read 32 bytes of `chars` and of the masks.
    let chars = unsafe {
        let first = _mm256_loadu_si256(chars.as_ptr().cast());
        let last = _mm256_loadu_si256(chars.as_ptr().add(C - 32).cast());
        let mask = crate::driver::stitch(split);
        _mm256_blendv_epi8(first, last, _mm256_loadu_si256(mask.as_ptr().cast()))
    };
    let high = _mm256_and_si256(_mm256_srli_epi32::<4>(chars), _mm256_set1_epi8(0x0f));

    let allowed = _mm256_shuffle_epi8(
        _mm256_broadcastsi128_si256(_mm_andnot_si128(
            _mm_setr_epi8(
                0x15, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x13, 0x1a, 0x1b, 0x1b,
                0x1b, 0x1a,
            ),
            _mm_set1_epi8(-1),
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

    let offsets = _mm256_shuffle_epi8(
        _mm256_broadcastsi128_si256(_mm_setr_epi8(
            0, 0, 19, 4, -65, -65, -71, -71, 0, 0, 0, 0, 0, 0, 0, 0,
        )),
        high,
    );

    let sextets = _mm256_min_epu8(_mm256_add_epi8(chars, offsets), _mm256_set1_epi8(63));
    let pairs = _mm256_maddubs_epi16(sextets, _mm256_set1_epi32(0x0140_0140));
    let quads = _mm256_madd_epi16(pairs, _mm256_set1_epi32(0x0001_1000));

    let bytes = _mm256_shuffle_epi8(
        quads,
        _mm256_broadcastsi128_si256(_mm_setr_epi8(
            2, 1, 0, 6, 5, 4, 10, 9, 8, 14, 13, 12, -1, -1, -1, -1,
        )),
    );

    if N < 28 {
        let bytes = _mm256_permutevar8x32_epi32(bytes, _mm256_setr_epi32(0, 1, 2, 4, 5, 6, 7, 7));

        // SAFETY: the stores write the 24 bytes of `out`.
        unsafe {
            _mm_storeu_si128(out.as_mut_ptr().cast(), _mm256_castsi256_si128(bytes));
            _mm_storel_epi64(
                out.as_mut_ptr().add(16).cast(),
                _mm256_extracti128_si256::<1>(bytes),
            );
        }
    } else {
        // SAFETY: the stores write the 28 bytes of `out`, the second over the
        // first lane's spare ones.
        unsafe {
            _mm_storeu_si128(out.as_mut_ptr().cast(), _mm256_castsi256_si128(bytes));
            _mm_storeu_si128(
                out.as_mut_ptr().add(12).cast(),
                _mm256_extracti128_si256::<1>(bytes),
            );
        }
    }

    Bad(allowed, class)
}

/// Encodes 24 bytes into 32 characters, from a window of 32 holding them from
/// its fifth byte, in one load landing each lane's 12 bytes in place, or from
/// the group alone, its halves loaded into their lanes; given room beyond 32,
/// a line ending at `split`, as `spread` writes it.
#[inline]
#[target_feature(enable = "avx2")]
fn encode<const W: usize, const N: usize>(
    window: &[u8; W],
    split: usize,
    out: &mut [MaybeUninit<u8>; N],
) {
    const { assert!((W == 24 || W == 32) && N >= 32) };

    let spread = if W == 32 {
        // SAFETY: the load reads the 32 bytes of `window`.
        let bytes = unsafe { _mm256_loadu_si256(window.as_ptr().cast()) };

        _mm256_shuffle_epi8(
            bytes,
            _mm256_setr_epi8(
                5, 4, 6, 5, 8, 7, 9, 8, 11, 10, 12, 11, 14, 13, 15, 14, 1, 0, 2, 1, 4, 3, 5, 4, 7,
                6, 8, 7, 10, 9, 11, 10,
            ),
        )
    } else {
        // SAFETY: the loads read the 24 bytes of `window`.
        let (low, high) = unsafe {
            (
                _mm_loadu_si128(window.as_ptr().cast()),
                _mm_loadu_si128(window.as_ptr().add(8).cast()),
            )
        };

        _mm256_shuffle_epi8(
            _mm256_set_m128i(high, low),
            _mm256_setr_epi8(
                1, 0, 2, 1, 4, 3, 5, 4, 7, 6, 8, 7, 10, 9, 11, 10, 5, 4, 6, 5, 8, 7, 9, 8, 11, 10,
                12, 11, 14, 13, 15, 14,
            ),
        )
    };

    let chars = ascii(sextets(spread));

    // SAFETY: the store writes 32 bytes of `out`.
    unsafe { _mm256_storeu_si256(out.as_mut_ptr().cast(), chars) };

    if N > 32 {
        crate::driver::spread(out, split, 32);
    }
}

/// The SSSE3 `sextets` over 256 bits, its multipliers hidden by the SSSE3
/// `opaque`.
#[inline]
#[target_feature(enable = "avx2")]
fn sextets(spread: __m256i) -> __m256i {
    let low = _mm256_broadcastsi128_si256(crate::ssse3::opaque(_mm_set1_epi32(0x0010_0001)));
    let high = _mm256_broadcastsi128_si256(crate::ssse3::opaque(_mm_set1_epi32(0x0100_0010)));
    let first = _mm256_srli_epi16::<10>(_mm256_mullo_epi16(spread, low));
    let second = _mm256_mullo_epi16(
        _mm256_and_si256(spread, _mm256_set1_epi32(0x003f_03f0)),
        high,
    );

    _mm256_or_si256(first, second)
}

/// The SSSE3 `ascii` over 256 bits.
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

#[cfg(test)]
mod tests {
    use crate::{avx2::*, scalar::*};

    #[test]
    fn kernels_match_scalar() {
        if !std::arch::is_x86_feature_detected!("avx2") {
            return;
        }

        // SAFETY: AVX2 was detected above.
        check_decode_kernel(|chars, split, out| unsafe {
            run(decode::<34, 24>(chars, split, out))
        });
        // SAFETY: AVX2 was detected above.
        check_decode_kernel(|chars, split, out: &mut [MaybeUninit<u8>; 24]| unsafe {
            let mut wide = [MaybeUninit::new(0); 28];
            let read = run(decode::<34, 28>(chars, split, &mut wide));
            out.copy_from_slice(&wide[..24]);
            read
        });
        // SAFETY: AVX2 was detected above.
        check_encode_kernel(
            4,
            |window: &[u8; 32], split, out: &mut [MaybeUninit<u8>; 34]| unsafe {
                encode(window, split, out)
            },
        );
        // SAFETY: AVX2 was detected above.
        check_encode_kernel(
            0,
            |bytes: &[u8; 24], split, out: &mut [MaybeUninit<u8>; 34]| unsafe {
                encode(bytes, split, out)
            },
        );
    }

    #[target_feature(enable = "avx2")]
    fn run(bad: Bad) -> usize {
        if bad.any() { bad.first() } else { 32 }
    }
}
