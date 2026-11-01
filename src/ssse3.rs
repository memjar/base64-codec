//! # SSSE3 backend
//!
//! Muła and Lemire's 128-bit kernels, as encodify 1.0.2 has them; its loops
//! also take what is shorter than an AVX2 block.

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
        kernel: decode,
        narrow: crate::scalar::decode_quanta,
    ),
    encode(
        bytes: 12,
        window: 16,
        lead: 4,
        kernel: encode,
        narrow: crate::scalar::encode_groups,
    ),
);

/// The characters outside the alphabet that blocks met, as non-zero bytes.
#[derive(Clone, Copy)]
struct Bad(__m128i);

impl Bad {
    const EAGER: bool = false;

    #[target_feature(enable = "ssse3")]
    fn none() -> Self {
        Self(_mm_setzero_si128())
    }

    #[target_feature(enable = "ssse3")]
    fn join(self, other: Self) -> Self {
        Self(_mm_or_si128(self.0, other.0))
    }

    #[target_feature(enable = "ssse3")]
    fn any(self) -> bool {
        self.first() < 16
    }

    /// The characters of the whole quanta before the first bad one, 16 if
    /// none.
    #[target_feature(enable = "ssse3")]
    fn first(self) -> usize {
        let valid = _mm_movemask_epi8(_mm_cmpeq_epi8(self.0, _mm_setzero_si128()));
        (valid as u32).trailing_ones() as usize / 4 * 4
    }
}

/// Decodes a block of 16 characters into 12 bytes, and 4 spare ones when
/// `out` holds 16.
#[inline]
#[target_feature(enable = "ssse3")]
fn decode<const C: usize, const N: usize>(
    chars: &[u8; C],
    split: usize,
    out: &mut [MaybeUninit<u8>; N],
) -> Bad {
    const { assert!(C >= 16 && (N == 12 || N == 16)) };

    // SAFETY: the loads read 16 bytes of `chars` and of the masks.
    let chars = unsafe {
        let first = _mm_loadu_si128(chars.as_ptr().cast());
        let last = _mm_loadu_si128(chars.as_ptr().add(C - 16).cast());
        let mask = _mm_loadu_si128(crate::driver::stitch(split).as_ptr().cast());
        _mm_or_si128(_mm_andnot_si128(mask, first), _mm_and_si128(mask, last))
    };
    let high = _mm_and_si128(_mm_srli_epi32::<4>(chars), _mm_set1_epi8(0x0f));

    let allowed = _mm_shuffle_epi8(
        _mm_andnot_si128(
            _mm_setr_epi8(
                0x15, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x13, 0x1a, 0x1b, 0x1b,
                0x1b, 0x1a,
            ),
            _mm_set1_epi8(-1),
        ),
        chars,
    );

    let class = _mm_shuffle_epi8(
        _mm_setr_epi8(
            0x10, 0x10, 0x01, 0x02, 0x04, 0x08, 0x04, 0x08, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10,
            0x10, 0x10,
        ),
        high,
    );

    let offsets = _mm_shuffle_epi8(
        _mm_setr_epi8(0, 0, 19, 4, -65, -65, -71, -71, 0, 0, 0, 0, 0, 0, 0, 0),
        high,
    );

    let sextets = _mm_min_epu8(_mm_add_epi8(chars, offsets), _mm_set1_epi8(63));
    let pairs = _mm_maddubs_epi16(sextets, _mm_set1_epi32(0x0140_0140));
    let quads = _mm_madd_epi16(pairs, _mm_set1_epi32(0x0001_1000));

    let bytes = _mm_shuffle_epi8(
        quads,
        _mm_setr_epi8(2, 1, 0, 6, 5, 4, 10, 9, 8, 14, 13, 12, -1, -1, -1, -1),
    );

    if N < 16 {
        let tail = _mm_cvtsi128_si32(_mm_srli_si128::<8>(bytes));

        // SAFETY: the stores write the 12 bytes of `out`.
        unsafe {
            _mm_storel_epi64(out.as_mut_ptr().cast(), bytes);
            out.as_mut_ptr().add(8).cast::<i32>().write_unaligned(tail);
        }
    } else {
        // SAFETY: the store writes the 16 bytes of `out`.
        unsafe { _mm_storeu_si128(out.as_mut_ptr().cast(), bytes) };
    }

    Bad(_mm_andnot_si128(allowed, class))
}

/// Encodes `K` groups of 12 bytes into 16 characters each, read alone or
/// from windows of 16 holding them from their fifth byte.
#[inline]
#[target_feature(enable = "ssse3")]
fn encode<const S: usize, const N: usize, const K: usize>(
    bytes: &[u8; S],
    split: usize,
    outs: [&mut [MaybeUninit<u8>; N]; K],
) {
    const { assert!(matches!(S - 12 * (K - 1), 12 | 16) && N >= 16) };

    for (group, out) in outs.into_iter().enumerate() {
        let at = 12 * group;

        let spread = if S - 12 * (K - 1) == 16 {
            // SAFETY: the load reads 16 bytes of `bytes`, as asserted above.
            let window = unsafe { _mm_loadu_si128(bytes.as_ptr().add(at).cast()) };

            _mm_shuffle_epi8(
                window,
                _mm_setr_epi8(5, 4, 6, 5, 8, 7, 9, 8, 11, 10, 12, 11, 14, 13, 15, 14),
            )
        } else {
            // SAFETY: the loads read 12 bytes of `bytes`, as asserted above.
            let (low, high) = unsafe {
                (
                    bytes.as_ptr().add(at).cast::<i64>().read_unaligned(),
                    bytes.as_ptr().add(at + 8).cast::<u32>().read_unaligned(),
                )
            };

            _mm_shuffle_epi8(
                _mm_set_epi64x(i64::from(high), low),
                _mm_setr_epi8(1, 0, 2, 1, 4, 3, 5, 4, 7, 6, 8, 7, 10, 9, 11, 10),
            )
        };

        let first =
            _mm_srli_epi16::<10>(_mm_mullo_epi16(spread, opaque(_mm_set1_epi32(0x0010_0001))));

        let second = _mm_mullo_epi16(
            _mm_and_si128(spread, _mm_set1_epi32(0x003f_03f0)),
            opaque(_mm_set1_epi32(0x0100_0010)),
        );

        let sextets = _mm_or_si128(first, second);

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

        // SAFETY: the store writes 16 bytes of `out`.
        unsafe { _mm_storeu_si128(out.as_mut_ptr().cast(), _mm_add_epi8(sextets, offsets)) };

        if N > 16 {
            crate::driver::spread(out, split, 16);
        }
    }
}

/// `value` hidden from the compiler, which would rewrite the multiplies by it
/// into slower shifts and blends.
#[inline]
#[target_feature(enable = "ssse3")]
#[cfg_attr(miri, allow(unused_mut))]
pub(crate) fn opaque(mut value: __m128i) -> __m128i {
    #[cfg(not(miri))]
    // SAFETY: the empty template touches nothing but its register.
    unsafe {
        core::arch::asm!(
            "/* {0} */",
            inout(xmm_reg) value,
            options(pure, nomem, nostack, preserves_flags),
        )
    };

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
        check_decode_kernel(|chars, split, out| unsafe {
            run(decode::<18, 12>(chars, split, out))
        });
        // SAFETY: SSSE3 was detected above.
        check_decode_kernel(|chars, split, out: &mut [MaybeUninit<u8>; 12]| unsafe {
            let mut wide = [MaybeUninit::new(0); 16];
            let read = run(decode::<18, 16>(chars, split, &mut wide));
            out.copy_from_slice(&wide[..12]);
            read
        });
        // SAFETY: SSSE3 was detected above.
        check_encode_kernel(
            4,
            |window: &[u8; 16], split, out: &mut [MaybeUninit<u8>; 18]| unsafe {
                encode(window, split, [out])
            },
        );
        // SAFETY: SSSE3 was detected above.
        check_encode_kernel(
            0,
            |bytes: &[u8; 12], split, out: &mut [MaybeUninit<u8>; 18]| unsafe {
                encode(bytes, split, [out])
            },
        );
    }

    #[target_feature(enable = "ssse3")]
    fn run(bad: Bad) -> usize {
        if bad.any() { bad.first() } else { 16 }
    }
}
