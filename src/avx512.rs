//! # AVX-512 backend
//!
//! Base64 over 512-bit vectors with the VBMI byte permutes, by Wojciech Muła
//! and Daniel Lemire's method: 64 characters into 48 bytes per decode step,
//! validated and translated by one lookup in a 128-byte table, and 48 bytes
//! into 64 characters per encode step, spread by a permute, split by a
//! multishift and translated by a lookup in the alphabet. What is shorter
//! than one block goes to the AVX2 loops, which AVX-512 implies.

#[cfg(target_arch = "x86")]
use core::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use core::arch::x86_64::*;
use core::mem::MaybeUninit;

use crate::scalar::ALPHABET;

/// Every character below 128 mapped to its sextet, or to 128 outside the
/// alphabet, in two halves of 64.
static DECODE: [[u8; 64]; 2] = decode_table();

/// The byte of `quads` each of the 48 decoded bytes comes from: the three
/// low bytes of each 32-bit lane, highest first.
static PACK: [u8; 64] = pack_table();

/// The byte of the input each byte of the spread comes from: the three bytes
/// of each group spread over a 32-bit lane as `[b, a, c, b]`.
static SPREAD: [u8; 64] = spread_table();

base64_driver!(
    #[target_feature(enable = "avx512vbmi")]
    decode(
        chars: 64,
        spare: 16,
        wide: decode_wide,
        exact: decode_exact,
        narrow: crate::avx2::decode_run,
    ),
    encode(
        bytes: 48,
        window: 64,
        lead: 0,
        wide: encode_wide,
        exact: encode_exact,
        narrow: crate::avx2::encode_run,
    ),
);

/// The characters outside the alphabet that blocks met, as bytes with their
/// highest bit set.
#[derive(Clone, Copy)]
struct Bad(__m512i);

impl Bad {
    #[inline]
    #[target_feature(enable = "avx512vbmi")]
    fn none() -> Self {
        Self(_mm512_setzero_si512())
    }

    #[inline]
    #[target_feature(enable = "avx512vbmi")]
    fn join(self, other: Self) -> Self {
        Self(_mm512_or_si512(self.0, other.0))
    }

    #[inline]
    #[target_feature(enable = "avx512vbmi")]
    fn any(self) -> bool {
        _mm512_movepi8_mask(self.0) != 0
    }

    /// The characters of the whole quanta before the first character
    /// outside the alphabet.
    #[inline]
    #[target_feature(enable = "avx512vbmi")]
    fn first(self) -> usize {
        _mm512_movepi8_mask(self.0).trailing_zeros() as usize / 4 * 4
    }
}

/// Decodes 64 characters into the first 48 bytes of `out`, its last 16
/// left spare.
#[inline]
#[target_feature(enable = "avx512vbmi")]
fn decode_wide(chars: &[u8; 64], out: &mut [MaybeUninit<u8>; 64]) -> Bad {
    let (bytes, bad) = decode(chars);

    // SAFETY: `out` holds the 64 bytes the store writes.
    unsafe { _mm512_storeu_si512(out.as_mut_ptr().cast(), bytes) };

    bad
}

/// Decodes 64 characters into 48 bytes.
#[inline]
#[target_feature(enable = "avx512vbmi")]
fn decode_exact(chars: &[u8; 64], out: &mut [MaybeUninit<u8>; 48]) -> Bad {
    let (bytes, bad) = decode(chars);

    // SAFETY: `out` holds 48 bytes, the first 32 for the first store and
    // the last 16 for the second.
    unsafe {
        _mm256_storeu_si256(out.as_mut_ptr().cast(), _mm512_castsi512_si256(bytes));
        _mm_storeu_si128(
            out.as_mut_ptr().add(32).cast(),
            _mm512_extracti32x4_epi32::<2>(bytes),
        );
    }

    bad
}

/// Encodes the first 48 bytes of `window` into 64 characters, its last 16
/// only loaded along.
#[inline]
#[target_feature(enable = "avx512vbmi")]
fn encode_wide(window: &[u8; 64], out: &mut [MaybeUninit<u8>; 64]) {
    // SAFETY: `window` holds the 64 bytes the load reads.
    let bytes = unsafe { _mm512_loadu_si512(window.as_ptr().cast()) };

    // SAFETY: `out` holds the 64 bytes the store writes.
    unsafe { _mm512_storeu_si512(out.as_mut_ptr().cast(), encode(bytes)) };
}

/// Encodes 48 bytes into 64 characters, reading no byte around them.
#[inline]
#[target_feature(enable = "avx512vbmi")]
fn encode_exact(bytes: &[u8; 48], out: &mut [MaybeUninit<u8>; 64]) {
    // SAFETY: `bytes` holds 48 bytes, the loads read the first 32 and the
    // last 16.
    let bytes = unsafe {
        _mm512_inserti32x4::<2>(
            _mm512_castsi256_si512(_mm256_loadu_si256(bytes.as_ptr().cast())),
            _mm_loadu_si128(bytes.as_ptr().add(32).cast()),
        )
    };

    // SAFETY: `out` holds the 64 bytes the store writes.
    unsafe { _mm512_storeu_si512(out.as_mut_ptr().cast(), encode(bytes)) };
}

/// The 48 bytes of 64 characters, first in the vector, and the characters
/// outside the alphabet: a character from 128 up, or one the table maps to
/// 128, has its highest bit set once joined with its sextet.
#[inline]
#[target_feature(enable = "avx512vbmi")]
fn decode(chars: &[u8; 64]) -> (__m512i, Bad) {
    // SAFETY: `chars` holds the 64 bytes the load reads.
    let chars = unsafe { _mm512_loadu_si512(chars.as_ptr().cast()) };
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
fn encode(bytes: __m512i) -> __m512i {
    let spread = _mm512_permutexvar_epi8(vector(&SPREAD), bytes);
    let sextets = _mm512_multishift_epi64_epi8(_mm512_set1_epi64(0x3036_242a_1016_040a), spread);
    _mm512_permutexvar_epi8(sextets, vector(ALPHABET))
}

/// The 64 bytes of `bytes` as a vector.
#[inline]
#[target_feature(enable = "avx512vbmi")]
fn vector(bytes: &[u8; 64]) -> __m512i {
    // SAFETY: `bytes` holds the 64 bytes the load reads.
    unsafe { _mm512_loadu_si512(bytes.as_ptr().cast()) }
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
        check_decode_kernel(|chars, out| unsafe { run(decode_exact(chars, out)) });
        // SAFETY: AVX-512 VBMI was detected above.
        check_decode_kernel(|chars, out: &mut [MaybeUninit<u8>; 48]| unsafe {
            let mut wide = [MaybeUninit::new(0); 64];
            let read = run(decode_wide(chars, &mut wide));
            out.copy_from_slice(&wide[..48]);
            read
        });
        // SAFETY: AVX-512 VBMI was detected above.
        check_encode_kernel(0, |window, out| unsafe { encode_wide(window, out) });
        // SAFETY: AVX-512 VBMI was detected above.
        check_encode_kernel(0, |bytes, out| unsafe { encode_exact(bytes, out) });
    }

    #[target_feature(enable = "avx512vbmi")]
    fn run(bad: Bad) -> usize {
        if bad.any() { bad.first() } else { 64 }
    }
}
