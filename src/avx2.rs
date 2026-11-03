//! # AVX2 backend
//!
//! The SSSE3 kernels over 256-bit vectors, as encodify 1.0.2 has them; what
//! is shorter than a block goes to the SSSE3 loops.

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
        narrow: decode_short,
    ),
    encode(
        bytes: 24,
        window: 32,
        lead: 4,
        kernel: encode,
        narrow: encode_short,
    ),
);

/// `Loops::decode_run` below a block: the SSSE3 loop of the alphabet.
#[target_feature(enable = "avx2")]
fn decode_short(src: &[u8], dst: &mut [MaybeUninit<u8>], url: bool) -> usize {
    match url {
        false => crate::ssse3::decode_run::<false>(src, dst),
        true => crate::ssse3::decode_run::<true>(src, dst),
    }
}

/// `Loops::encode_run` below a block: the SSSE3 loop of the alphabet.
#[target_feature(enable = "avx2")]
fn encode_short(src: &[u8], dst: &mut [MaybeUninit<u8>], url: bool) -> usize {
    match url {
        false => crate::ssse3::encode_run::<false>(src, dst),
        true => crate::ssse3::encode_run::<true>(src, dst),
    }
}

/// Per character, the classes its low nibble allows and its high nibble's
/// class, not allowed outside the alphabet.
#[derive(Clone, Copy)]
struct Bad(__m256i, __m256i);

impl Bad {
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

    /// The characters of the whole quanta before the first bad one.
    #[target_feature(enable = "avx2")]
    fn first(self) -> usize {
        let bad = _mm256_cmpeq_epi8(_mm256_andnot_si256(self.0, self.1), _mm256_setzero_si256());
        (_mm256_movemask_epi8(bad) as u32).trailing_ones() as usize / 4 * 4
    }
}

/// Decodes a block of 32 characters into 24 bytes, and 4 spare ones when
/// `out` holds 28.
#[inline]
#[target_feature(enable = "avx2")]
fn decode<const C: usize, const N: usize>(
    chars: &[u8; C],
    split: usize,
    out: &mut [MaybeUninit<u8>; N],
    url: bool,
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

    let ([low_b, low_c, low_d, low_e, low_f], high_7, offset_2) = if url {
        ([0x3b, 0x3b, 0x3a, 0x3b, 0x33], 0x20, 17)
    } else {
        ([0x1a, 0x1b, 0x1b, 0x1b, 0x1a], 0x08, 19)
    };

    let allowed = _mm256_shuffle_epi8(
        _mm256_broadcastsi128_si256(_mm_andnot_si128(
            _mm_setr_epi8(
                0x15, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x13, low_b, low_c,
                low_d, low_e, low_f,
            ),
            _mm_set1_epi8(-1),
        )),
        chars,
    );

    let class = _mm256_shuffle_epi8(
        _mm256_broadcastsi128_si256(_mm_setr_epi8(
            0x10, 0x10, 0x01, 0x02, 0x04, 0x08, 0x04, high_7, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10,
            0x10, 0x10,
        )),
        high,
    );

    let offsets = _mm256_shuffle_epi8(
        _mm256_broadcastsi128_si256(_mm_setr_epi8(
            0, 0, offset_2, 4, -65, -65, -71, -71, 0, 0, 0, 0, 0, 0, 0, 0,
        )),
        high,
    );

    let mut sextets = _mm256_add_epi8(chars, offsets);

    if url {
        // NOTE: `_` shares its high nibble with `P` to `Z`: saturated to 63.
        let underscore = _mm256_cmpeq_epi8(chars, _mm256_set1_epi8(b'_' as i8));
        sextets = _mm256_or_si256(sextets, underscore);
    }

    let sextets = _mm256_min_epu8(sextets, _mm256_set1_epi8(63));
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
        // SAFETY: the stores write the 28 bytes of `out`.
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

/// Encodes `K` groups of 24 bytes into 32 characters each, read alone or
/// from windows of 32 holding them from their fifth byte.
#[inline]
#[target_feature(enable = "avx2")]
fn encode<const S: usize, const N: usize, const K: usize>(
    bytes: &[u8; S],
    split: usize,
    outs: [&mut [MaybeUninit<u8>; N]; K],
    url: bool,
) {
    const { assert!(matches!(S - 24 * (K - 1), 24 | 32) && N >= 32) };

    let spread: [__m256i; K] = core::array::from_fn(|group| {
        let at = 24 * group;

        if S - 24 * (K - 1) == 32 {
            // SAFETY: the load reads 32 bytes of `bytes`, as asserted above.
            let window = unsafe { _mm256_loadu_si256(bytes.as_ptr().add(at).cast()) };

            _mm256_shuffle_epi8(
                window,
                _mm256_setr_epi8(
                    5, 4, 6, 5, 8, 7, 9, 8, 11, 10, 12, 11, 14, 13, 15, 14, 1, 0, 2, 1, 4, 3, 5, 4,
                    7, 6, 8, 7, 10, 9, 11, 10,
                ),
            )
        } else {
            // SAFETY: the loads read 24 bytes of `bytes`, as asserted above.
            let (low, high) = unsafe {
                (
                    _mm_loadu_si128(bytes.as_ptr().add(at).cast()),
                    _mm_loadu_si128(bytes.as_ptr().add(at + 8).cast()),
                )
            };

            _mm256_shuffle_epi8(
                _mm256_set_m128i(high, low),
                _mm256_setr_epi8(
                    1, 0, 2, 1, 4, 3, 5, 4, 7, 6, 8, 7, 10, 9, 11, 10, 5, 4, 6, 5, 8, 7, 9, 8, 11,
                    10, 12, 11, 14, 13, 15, 14,
                ),
            )
        }
    });

    let low = _mm256_broadcastsi128_si256(crate::ssse3::opaque(_mm_set1_epi32(0x0010_0001)));
    let high = _mm256_broadcastsi128_si256(crate::ssse3::opaque(_mm_set1_epi32(0x0100_0010)));
    let first = spread.map(|spread| _mm256_srli_epi16::<10>(_mm256_mullo_epi16(spread, low)));

    let second = spread.map(|spread| {
        _mm256_mullo_epi16(
            _mm256_and_si256(spread, _mm256_set1_epi32(0x003f_03f0)),
            high,
        )
    });

    let sextets: [__m256i; K] =
        core::array::from_fn(|group| _mm256_or_si256(first[group], second[group]));

    let ranges = sextets.map(|sextets| {
        _mm256_sub_epi8(
            _mm256_subs_epu8(sextets, _mm256_set1_epi8(51)),
            _mm256_cmpgt_epi8(sextets, _mm256_set1_epi8(25)),
        )
    });

    let (sixty_two, sixty_three) = if url { (-17, 32) } else { (-19, -16) };

    let offsets = _mm256_broadcastsi128_si256(_mm_setr_epi8(
        65,
        71,
        -4,
        -4,
        -4,
        -4,
        -4,
        -4,
        -4,
        -4,
        -4,
        -4,
        sixty_two,
        sixty_three,
        0,
        0,
    ));

    for (group, out) in outs.into_iter().enumerate() {
        let chars = _mm256_add_epi8(sextets[group], _mm256_shuffle_epi8(offsets, ranges[group]));

        // SAFETY: the store writes 32 bytes of `out`.
        unsafe { _mm256_storeu_si256(out.as_mut_ptr().cast(), chars) };

        if N > 32 {
            crate::driver::spread(out, split, 32);
        }
    }
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
        check_decode_kernel(|chars, split, out, url| unsafe {
            run(decode::<34, 24>(chars, split, out, url))
        });
        // SAFETY: AVX2 was detected above.
        check_decode_kernel(
            |chars, split, out: &mut [MaybeUninit<u8>; 24], url| unsafe {
                let mut wide = [MaybeUninit::new(0); 28];
                let read = run(decode::<34, 28>(chars, split, &mut wide, url));
                out.copy_from_slice(&wide[..24]);
                read
            },
        );
        // SAFETY: AVX2 was detected above.
        check_encode_kernel(
            4,
            |window: &[u8; 32], split, out: &mut [MaybeUninit<u8>; 34], url| unsafe {
                encode(window, split, [out], url)
            },
        );
        // SAFETY: AVX2 was detected above.
        check_encode_kernel(
            0,
            |bytes: &[u8; 24], split, out: &mut [MaybeUninit<u8>; 34], url| unsafe {
                encode(bytes, split, [out], url)
            },
        );
    }

    #[target_feature(enable = "avx2")]
    fn run(bad: Bad) -> usize {
        if bad.any() { bad.first() } else { 32 }
    }
}
