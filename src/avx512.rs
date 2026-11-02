//! # AVX-512 backend
//!
//! Muła and Lemire's VBMI kernels: one 128-byte table lookup decodes 64
//! characters, permutes and a multishift encode 48 bytes. What is shorter
//! than a block is one masked block.

#[cfg(target_arch = "x86")]
use core::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use core::arch::x86_64::*;
use core::mem::MaybeUninit;

use crate::scalar::{ALPHABETS, LINE, LINE_BYTES};

/// Per alphabet, the sextets of the characters below 128, 128 outside it.
static DECODE: [[[u8; 64]; 2]; 2] = [decode_table(ALPHABETS[0]), decode_table(ALPHABETS[1])];

/// The byte of `quads` each decoded byte comes from.
static PACK: [u8; 64] = {
    let mut table = [0; 64];
    let mut at = 0;

    while at < 48 {
        table[at] = (at / 3 * 4 + 2 - at % 3) as u8;
        at += 1;
    }

    table
};

/// Each group's 3 bytes spread over a 32-bit lane as `[b, a, c, b]`.
static SPREAD: [u8; 64] = {
    let mut table = [0; 64];
    let mut at = 0;

    while at < 64 {
        table[at] = (at / 4 * 3 + [1, 0, 2, 1][at % 4]) as u8;
        at += 1;
    }

    table
};

/// Per 64 bytes of output in a cycle of lines whose output fills whole
/// vectors: the byte of the two encoded blocks the vector draws from that
/// each lane takes, bit 7 set, or the lane's line ending byte.
#[derive(Clone, Copy)]
#[repr(align(64))]
struct Wrap([u8; 64]);

/// The cycles of 32 lines ending with CRLF and of 64 ending with LF.
static CRLF: [Wrap; 39] = wraps::<78, 39>();
static LF: [Wrap; 77] = wraps::<77, 77>();

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
        periods: encode_wrapped,
    ),
);

/// The characters outside the alphabet blocks met, their high bit set.
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

    /// The characters of the whole quanta before the first bad one.
    #[target_feature(enable = "avx512vbmi")]
    fn first(self) -> usize {
        _mm512_movepi8_mask(self.0).trailing_zeros() as usize / 4 * 4
    }
}

/// Decodes a block of 64 characters into 48 bytes, and 16 spare ones when
/// `out` holds 64.
#[inline]
#[target_feature(enable = "avx512vbmi")]
fn decode<const C: usize, const N: usize>(
    chars: &[u8; C],
    split: usize,
    out: &mut [MaybeUninit<u8>; N],
    url: bool,
) -> Bad {
    const { assert!(C >= 64 && (N == 48 || N == 64)) };

    // SAFETY: the loads read 64 bytes of `chars`.
    let chars = unsafe {
        let first = _mm512_loadu_si512(chars.as_ptr().cast());
        let last = _mm512_loadu_si512(chars.as_ptr().add(C - 64).cast());
        _mm512_mask_blend_epi8(!(u64::MAX >> (64 - split)), first, last)
    };

    let (bytes, bad) = decode_vector(chars, url);

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

/// Encodes `K` groups of 48 bytes into 64 characters each, read alone or
/// from windows of 64, a line ending blended into a masked store.
#[inline]
#[target_feature(enable = "avx512vbmi")]
fn encode<const S: usize, const N: usize, const K: usize>(
    bytes: &[u8; S],
    split: usize,
    outs: [&mut [MaybeUninit<u8>; N]; K],
    url: bool,
) {
    const { assert!(matches!(S - 48 * (K - 1), 48 | 64) && N >= 64) };

    for (group, out) in outs.into_iter().enumerate() {
        let at = 48 * group;

        let bytes = if S - 48 * (K - 1) == 48 {
            // SAFETY: the loads read 48 bytes of `bytes`, as asserted above.
            unsafe {
                _mm512_inserti32x4::<2>(
                    _mm512_castsi256_si512(_mm256_loadu_si256(bytes.as_ptr().add(at).cast())),
                    _mm_loadu_si128(bytes.as_ptr().add(at + 32).cast()),
                )
            }
        } else {
            // SAFETY: the load reads 64 bytes of `bytes`, as asserted above.
            unsafe { _mm512_loadu_si512(bytes.as_ptr().add(at).cast()) }
        };

        let chars = encode_vector(bytes, 0, url);

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
}

/// `Loops::decode_run` below a block in one masked block, by `decode_quanta`
/// when a character is outside the alphabet.
#[target_feature(enable = "avx512vbmi")]
fn decode_short(src: &[u8], dst: &mut [MaybeUninit<u8>], url: bool) -> usize {
    let chars = (src.len() / 4).min(dst.len() / 3).min(15) * 4;

    // SAFETY: the load reads the first `chars` bytes of `src`.
    let (bytes, bad) = decode_vector(
        unsafe { _mm512_maskz_loadu_epi8(mask(chars), src.as_ptr().cast()) },
        url,
    );

    if _mm512_movepi8_mask(bad.0) & mask(chars) != 0 {
        return crate::scalar::decode_quanta(src, dst, url);
    }

    // SAFETY: the store writes the first `chars / 4 * 3` bytes of `dst`.
    unsafe { _mm512_mask_storeu_epi8(dst.as_mut_ptr().cast(), mask(chars / 4 * 3), bytes) };

    chars
}

/// `Loops::encode_run` below a block in one masked block.
#[target_feature(enable = "avx512vbmi")]
fn encode_short(src: &[u8], dst: &mut [MaybeUninit<u8>], url: bool) -> usize {
    let bytes = (src.len() / 3).min(dst.len() / 4).min(15) * 3;

    // SAFETY: the load reads the first `bytes` bytes of `src`.
    let chars = encode_vector(
        unsafe { _mm512_maskz_loadu_epi8(mask(bytes), src.as_ptr().cast()) },
        0,
        url,
    );

    // SAFETY: the store writes the first `bytes / 3 * 4` bytes of `dst`.
    unsafe { _mm512_mask_storeu_epi8(dst.as_mut_ptr().cast(), mask(bytes / 3 * 4), chars) };

    bytes
}

/// `encode_periods`, by `encode_cycles` from 32 lines on.
#[inline]
#[target_feature(enable = "avx512vbmi")]
fn encode_wrapped<const URL: bool, const N: usize, const SPAN: usize>(
    src: &[u8],
    dst: &mut [MaybeUninit<u8>],
    lines: usize,
) -> usize {
    match lines {
        ..32 => encode_periods::<URL, N, SPAN>(src, dst, lines),
        _ => encode_cycles::<URL, N, SPAN>(src, dst, lines),
    }
}

/// `encode_periods` with each 64 bytes of output picked from the two encoded
/// blocks it draws from by the `Wrap` of their place, line endings included,
/// as long as whole vectors fit the lines.
#[target_feature(enable = "avx512vbmi")]
#[inline(never)]
fn encode_cycles<const URL: bool, const N: usize, const SPAN: usize>(
    src: &[u8],
    dst: &mut [MaybeUninit<u8>],
    lines: usize,
) -> usize {
    let span = SPAN / 16;
    let wraps: &[Wrap] = if span == 78 { &CRLF } else { &LF };
    // The one vector of a cycle drawing from the blocks of the one before.
    let stay = const { stay(SPAN / 16) };
    let (outs, _) = dst[..lines * span / 64 * 64].as_chunks_mut::<64>();
    let last = src.len().saturating_sub(48);
    let mut done = 0;

    let Some(first) = src.first_chunk::<64>() else {
        return encode_periods::<URL, N, SPAN>(src, dst, lines);
    };

    // The encoded blocks the vector draws from, and where the second starts.
    let (mut low, mut high, mut from) = (
        _mm512_setzero_si512(),
        encode_vector(vector(first), 0, URL),
        0,
    );

    'cycles: for outs in outs.chunks_mut(wraps.len()) {
        for (at, (wrap, out)) in wraps.iter().zip(outs).enumerate() {
            if at != stay {
                from += 48;

                if from > last {
                    break 'cycles;
                }

                // SAFETY: the load reads the 64 bytes of `src` ending the block.
                let bytes = unsafe { _mm512_loadu_si512(src.as_ptr().add(from - 16).cast()) };
                (low, high) = (high, encode_vector(bytes, 16, URL));
            }

            let index = vector(&wrap.0);
            let mask = _mm512_movepi8_mask(index);
            let chars = _mm512_mask2_permutex2var_epi8(low, index, mask, high);

            // SAFETY: the store writes the 64 bytes of `out`.
            unsafe { _mm512_storeu_si512(out.as_mut_ptr().cast(), chars) };
            done += 1;
        }
    }

    let whole = done * 64 / span;
    let (src, dst) = (
        &src[whole * LINE_BYTES..lines * LINE_BYTES],
        &mut dst[whole * span..],
    );
    whole + encode_lines::<URL, N, SPAN>(src, dst)
}

/// The 48 bytes of 64 characters, first in the vector, and their `Bad`.
#[inline]
#[target_feature(enable = "avx512vbmi")]
fn decode_vector(chars: __m512i, url: bool) -> (__m512i, Bad) {
    let [low, high] = &DECODE[url as usize];
    let sextets = _mm512_permutex2var_epi8(vector(low), chars, vector(high));
    let pairs = _mm512_maddubs_epi16(sextets, _mm512_set1_epi32(0x0140_0140));
    let quads = _mm512_madd_epi16(pairs, _mm512_set1_epi32(0x0001_1000));
    let bytes = _mm512_permutexvar_epi8(vector(&PACK), quads);

    (bytes, Bad(_mm512_or_si512(chars, sextets)))
}

/// The 64 characters of the 48 bytes `lead` bytes into `bytes`.
#[inline]
#[target_feature(enable = "avx512vbmi")]
fn encode_vector(bytes: __m512i, lead: i8, url: bool) -> __m512i {
    let spread = _mm512_add_epi8(vector(&SPREAD), _mm512_set1_epi8(lead));
    let spread = _mm512_permutexvar_epi8(spread, bytes);
    let sextets = _mm512_multishift_epi64_epi8(_mm512_set1_epi64(0x3036_242a_1016_040a), spread);
    _mm512_permutexvar_epi8(sextets, vector(ALPHABETS[url as usize]))
}

#[target_feature(enable = "avx512vbmi")]
fn vector(bytes: &[u8; 64]) -> __m512i {
    // SAFETY: the load reads the 64 bytes of `bytes`.
    unsafe { _mm512_loadu_si512(bytes.as_ptr().cast()) }
}

const fn decode_table(alphabet: &[u8; 64]) -> [[u8; 64]; 2] {
    let mut table = [[128; 64]; 2];
    let mut sextet = 0;

    while sextet < 64 {
        let ch = alphabet[sextet] as usize;
        table[ch / 64][ch % 64] = sextet as u8;
        sextet += 1;
    }

    table
}

/// The `Wrap` of each of the `V` vectors of a cycle of lines `W` bytes long.
const fn wraps<const W: usize, const V: usize>() -> [Wrap; V] {
    let mut table = [Wrap([0; 64]); V];
    let mut at = 0;

    while at < 64 * V {
        // The first character of the two blocks the vector draws from.
        let first = chars(at / 64 * 64, W) / 64 * 64;

        table[at / 64].0[at % 64] = match at % W {
            col if col < LINE => {
                assert!(chars(at, W) - first < 128);
                (chars(at, W) - first) as u8 | 0x80
            }
            col if col == LINE && W == LINE + 2 => b'\r',
            _ => b'\n',
        };

        at += 1;
    }

    table
}

/// The vector of a cycle of lines `span` bytes long drawing from the blocks
/// of the vector before it: a cycle has a block fewer than vectors.
const fn stay(span: usize) -> usize {
    let mut at = 64;

    while chars(at, span) / 64 != chars(at - 64, span) / 64 {
        at += 64;
    }

    at / 64
}

/// The characters before output byte `at`, lines `span` bytes long.
const fn chars(at: usize, span: usize) -> usize {
    at / span * LINE + if at % span < LINE { at % span } else { LINE }
}

/// The mask of a vector's first `len` bytes, `len` below 64.
fn mask(len: usize) -> u64 {
    (1 << len) - 1
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
        check_decode_kernel(|chars, split, out, url| unsafe {
            run(decode::<66, 48>(chars, split, out, url))
        });
        // SAFETY: AVX-512 VBMI was detected above.
        check_decode_kernel(
            |chars, split, out: &mut [MaybeUninit<u8>; 48], url| unsafe {
                let mut wide = [MaybeUninit::new(0); 64];
                let read = run(decode::<66, 64>(chars, split, &mut wide, url));
                out.copy_from_slice(&wide[..48]);
                read
            },
        );
        // SAFETY: AVX-512 VBMI was detected above.
        check_encode_kernel(
            0,
            |window: &[u8; 64], split, out: &mut [MaybeUninit<u8>; 66], url| unsafe {
                encode(window, split, [out], url)
            },
        );
        // SAFETY: AVX-512 VBMI was detected above.
        check_encode_kernel(
            0,
            |bytes: &[u8; 48], split, out: &mut [MaybeUninit<u8>; 66], url| unsafe {
                encode(bytes, split, [out], url)
            },
        );
    }

    #[target_feature(enable = "avx512vbmi")]
    fn run(bad: Bad) -> usize {
        if bad.any() { bad.first() } else { 64 }
    }
}
