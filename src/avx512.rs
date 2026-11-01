//! # AVX-512 backend
//!
//! Base64 over 512-bit vectors with the VBMI byte permutes, by Wojciech Muła
//! and Daniel Lemire's method: 64 characters into 48 bytes per decode step,
//! validated and translated by one lookup in a 128-byte table, and 48 bytes
//! into 64 characters per encode step, spread by a permute, split by a
//! multishift and translated by a lookup in the alphabet. What is shorter
//! than one block is one block too, its load and store masked to it; whole
//! blocks are not, as a masked access crossing into the next page costs about
//! ten cycles.

#[cfg(target_arch = "x86")]
use core::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use core::arch::x86_64::*;
use core::mem::MaybeUninit;

use crate::scalar::ALPHABET;

/// Every character below 128 mapped to its sextet, or to 128 outside the
/// alphabet, in two halves of 64.
static DECODE: [[u8; 64]; 2] = decode_table();

/// The byte of `quads` each of the 48 decoded bytes comes from: the three low
/// bytes of each 32-bit lane, highest first.
static PACK: [u8; 64] = pack_table();

/// The byte of the input each byte of the spread comes from: the three bytes
/// of each group spread over a 32-bit lane as `[b, a, c, b]`.
static SPREAD: [u8; 64] = spread_table();

base64_driver!(
    #[target_feature(enable = "avx512vbmi")]
    decode(
        chars: 64,
        spare: 16,
        kernel: decode,
        narrow: decode_short,
    ),
    encode(
        bytes: 48,
        window: 64,
        lead: 0,
        kernel: encode,
        narrow: encode_short,
    ),
);

/// The characters outside the alphabet that blocks met, as bytes with their
/// highest bit set.
#[derive(Clone, Copy)]
struct Bad(__m512i);

impl Bad {
    const EAGER: bool = false;

    #[target_feature(enable = "avx512vbmi")]
    fn none() -> Self {
        Self(_mm512_setzero_si512())
    }

    #[target_feature(enable = "avx512vbmi")]
    fn join(self, other: Self) -> Self {
        Self(_mm512_or_si512(self.0, other.0))
    }

    #[target_feature(enable = "avx512vbmi")]
    fn any(self) -> bool {
        _mm512_movepi8_mask(self.0) != 0
    }

    /// The characters of the whole quanta before the first one outside the
    /// alphabet.
    #[target_feature(enable = "avx512vbmi")]
    fn first(self) -> usize {
        _mm512_movepi8_mask(self.0).trailing_zeros() as usize / 4 * 4
    }
}

/// Decodes 64 characters into the first 48 bytes of `out`, plus 16 spare ones
/// when it holds 64: the first 64 of `chars`, or its first `split` and its
/// last ones.
#[inline]
#[target_feature(enable = "avx512vbmi")]
fn decode<const C: usize, const N: usize>(
    chars: &[u8; C],
    split: usize,
    out: &mut [MaybeUninit<u8>; N],
) -> Bad {
    const { assert!(C >= 64 && (N == 48 || N == 64)) };

    // SAFETY: the loads read 64 bytes of `chars`.
    let chars = unsafe {
        let first = _mm512_loadu_si512(chars.as_ptr().cast());
        let last = _mm512_loadu_si512(chars.as_ptr().add(C - 64).cast());
        _mm512_mask_blend_epi8(!(u64::MAX >> (64 - split)), first, last)
    };

    let (bytes, bad) = decode_vector(chars);

    if N < 64 {
        // SAFETY: the stores write the 48 bytes of `out`.
        unsafe {
            _mm256_storeu_si256(out.as_mut_ptr().cast(), _mm512_castsi512_si256(bytes));
            _mm_storeu_si128(
                out.as_mut_ptr().add(32).cast(),
                _mm512_extracti32x4_epi32::<2>(bytes),
            );
        }
    } else {
        // SAFETY: the store writes the 64 bytes of `out`.
        unsafe { _mm512_storeu_si512(out.as_mut_ptr().cast(), bytes) };
    }

    bad
}

/// Encodes 48 bytes into 64 characters, from the first 48 of a window of 64
/// or from the group alone; given room beyond 64, a line ending fills it at
/// `split` when not past 64, the characters from `split` on moved past it.
#[inline]
#[target_feature(enable = "avx512vbmi")]
fn encode<const W: usize, const N: usize>(
    window: &[u8; W],
    split: usize,
    out: &mut [MaybeUninit<u8>; N],
) {
    const { assert!((W == 48 || W == 64) && N >= 64) };

    let bytes = if W < 64 {
        // SAFETY: the loads read the 48 bytes of `window`.
        unsafe {
            _mm512_inserti32x4::<2>(
                _mm512_castsi256_si512(_mm256_loadu_si256(window.as_ptr().cast())),
                _mm_loadu_si128(window.as_ptr().add(32).cast()),
            )
        }
    } else {
        // SAFETY: the load reads the 64 bytes of `window`.
        unsafe { _mm512_loadu_si512(window.as_ptr().cast()) }
    };

    let chars = encode_vector(bytes);

    // SAFETY: the stores write the first and the last 64 bytes of `out`.
    unsafe {
        _mm512_storeu_si512(out.as_mut_ptr().cast(), chars);

        if N > 64 && split <= 64 {
            let from = u64::MAX << (split - (N - 64));
            let ends = _mm512_set1_epi16(if N == 66 { 0x0a0d } else { 0x0a0a });
            let chars = _mm512_mask_blend_epi8(from & u64::MAX >> (64 - split), chars, ends);
            _mm512_mask_storeu_epi8(out.as_mut_ptr().add(N - 64).cast(), from, chars);
        }
    }
}

/// Decodes the whole quanta of alphabet characters at the start of `src` that
/// fit in `dst`, fewer than a block, returning the characters read; quantum
/// by quantum when one is outside the alphabet, so that the length waits on
/// no test.
#[target_feature(enable = "avx512vbmi")]
fn decode_short(src: &[u8], dst: &mut [MaybeUninit<u8>]) -> usize {
    let chars = (src.len() / 4).min(dst.len() / 3).min(15) * 4;

    // SAFETY: the load reads the first `chars` bytes of `src`.
    let (bytes, bad) =
        decode_vector(unsafe { _mm512_maskz_loadu_epi8(mask(chars), src.as_ptr().cast()) });

    if _mm512_movepi8_mask(bad.0) & mask(chars) != 0 {
        return crate::scalar::decode_quanta(src, dst);
    }

    // SAFETY: the store writes the first `chars / 4 * 3` bytes of `dst`.
    unsafe { _mm512_mask_storeu_epi8(dst.as_mut_ptr().cast(), mask(chars / 4 * 3), bytes) };

    chars
}

/// Encodes the whole groups at the start of `src` that fit in `dst`, fewer
/// than a block, returning the bytes read.
#[target_feature(enable = "avx512vbmi")]
fn encode_short(src: &[u8], dst: &mut [MaybeUninit<u8>]) -> usize {
    let bytes = (src.len() / 3).min(dst.len() / 4).min(15) * 3;

    // SAFETY: the load reads the first `bytes` bytes of `src`.
    let chars = encode_vector(unsafe { _mm512_maskz_loadu_epi8(mask(bytes), src.as_ptr().cast()) });

    // SAFETY: the store writes the first `bytes / 3 * 4` bytes of `dst`.
    unsafe { _mm512_mask_storeu_epi8(dst.as_mut_ptr().cast(), mask(bytes / 3 * 4), chars) };

    bytes
}

/// The 48 bytes of 64 characters, first in the vector, and the characters
/// outside the alphabet: one from 128 up, or one the table maps to 128, has
/// its highest bit set once joined with its sextet.
#[inline]
#[target_feature(enable = "avx512vbmi")]
fn decode_vector(chars: __m512i) -> (__m512i, Bad) {
    let [low, high] = &DECODE;
    let sextets = _mm512_permutex2var_epi8(vector(low), chars, vector(high));
    let pairs = _mm512_maddubs_epi16(sextets, _mm512_set1_epi32(0x0140_0140));
    let quads = _mm512_madd_epi16(pairs, _mm512_set1_epi32(0x0001_1000));
    let bytes = _mm512_permutexvar_epi8(vector(&PACK), quads);

    (bytes, Bad(_mm512_or_si512(chars, sextets)))
}

/// The 64 characters of 48 bytes: each 32-bit lane spread as `[b, a, c, b]`
/// holds the four sextets of its group at bits 10, 4, 22 and 16, which one
/// multishift moves into its bytes; the alphabet lookup ignores the two bits
/// above each sextet.
#[inline]
#[target_feature(enable = "avx512vbmi")]
fn encode_vector(bytes: __m512i) -> __m512i {
    let spread = _mm512_permutexvar_epi8(vector(&SPREAD), bytes);
    let sextets = _mm512_multishift_epi64_epi8(_mm512_set1_epi64(0x3036_242a_1016_040a), spread);
    _mm512_permutexvar_epi8(sextets, vector(ALPHABET))
}

#[target_feature(enable = "avx512vbmi")]
fn vector(bytes: &[u8; 64]) -> __m512i {
    // SAFETY: the load reads the 64 bytes of `bytes`.
    unsafe { _mm512_loadu_si512(bytes.as_ptr().cast()) }
}

/// The mask of the first `len` bytes of a vector, `len` below 64.
fn mask(len: usize) -> u64 {
    (1 << len) - 1
}

const fn decode_table() -> [[u8; 64]; 2] {
    let mut table = [[128; 64]; 2];
    let mut sextet = 0;

    while sextet < 64 {
        let ch = ALPHABET[sextet] as usize;
        table[ch / 64][ch % 64] = sextet as u8;
        sextet += 1;
    }

    table
}

const fn pack_table() -> [u8; 64] {
    let mut table = [0; 64];
    let mut at = 0;

    while at < 48 {
        table[at] = (at / 3 * 4 + 2 - at % 3) as u8;
        at += 1;
    }

    table
}

const fn spread_table() -> [u8; 64] {
    let mut table = [0; 64];
    let mut at = 0;

    while at < 64 {
        table[at] = (at / 4 * 3 + [1, 0, 2, 1][at % 4]) as u8;
        at += 1;
    }

    table
}

#[cfg(test)]
mod tests {
    use crate::{avx512::*, scalar::*};

    #[test]
    #[cfg_attr(miri, ignore)]
    fn kernels_match_scalar() {
        if !std::arch::is_x86_feature_detected!("avx512vbmi") {
            return;
        }

        // SAFETY: AVX-512 VBMI was detected above.
        check_decode_kernel(|chars, split, out| unsafe {
            run(decode::<66, 48>(chars, split, out))
        });
        // SAFETY: AVX-512 VBMI was detected above.
        check_decode_kernel(|chars, split, out: &mut [MaybeUninit<u8>; 48]| unsafe {
            let mut wide = [MaybeUninit::new(0); 64];
            let read = run(decode::<66, 64>(chars, split, &mut wide));
            out.copy_from_slice(&wide[..48]);
            read
        });
        // SAFETY: AVX-512 VBMI was detected above.
        check_encode_kernel(
            0,
            |window: &[u8; 64], split, out: &mut [MaybeUninit<u8>; 66]| unsafe {
                encode(window, split, out)
            },
        );
        // SAFETY: AVX-512 VBMI was detected above.
        check_encode_kernel(
            0,
            |bytes: &[u8; 48], split, out: &mut [MaybeUninit<u8>; 66]| unsafe {
                encode(bytes, split, out)
            },
        );
    }

    #[target_feature(enable = "avx512vbmi")]
    fn run(bad: Bad) -> usize {
        if bad.any() { bad.first() } else { 64 }
    }
}
