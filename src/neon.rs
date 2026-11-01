//! # NEON backend
//!
//! Base64 over 128-bit NEON vectors, by the method encodify 1.0.2 implements
//! (src/base64/kernel/neon.rs), which is not the x86 one: a step loads 64
//! characters deinterleaved into four vectors, one per position in the
//! quantum, so that packing is three shifts and an interleaving store of 48
//! bytes, and translates them with 64-byte table lookups. What is shorter
//! than one block goes to the scalar loops.

use core::{arch::aarch64::*, mem::MaybeUninit};

use crate::scalar::ALPHABET;

/// The characters below 64, each mapped to its sextet plus one, or to zero
/// outside the alphabet.
static DECODE_LOW: [u8; 64] = decode_table(0);

/// The characters from 64 to 127, less 64, mapped as in `DECODE_LOW`.
static DECODE_HIGH: [u8; 64] = decode_table(64);

base64_driver!(
    #[target_feature(enable = "neon")]
    decode(
        chars: 64,
        spare: 0,
        kernel: decode,
        narrow: crate::scalar::decode_quanta,
    ),
    encode(
        bytes: 48,
        window: 48,
        lead: 0,
        kernel: encode,
        narrow: crate::scalar::encode_groups,
    ),
);

/// The least translated character of each quantum that blocks met: a zero
/// lane means a character outside the alphabet.
#[derive(Clone, Copy)]
struct Bad(uint8x16_t);

impl Bad {
    const EAGER: bool = false;

    #[target_feature(enable = "neon")]
    fn none() -> Self {
        Self(vdupq_n_u8(1))
    }

    #[target_feature(enable = "neon")]
    fn join(self, other: Self) -> Self {
        Self(vminq_u8(self.0, other.0))
    }

    #[target_feature(enable = "neon")]
    fn any(self) -> bool {
        vminvq_u8(self.0) == 0
    }

    /// The characters of the whole quanta before the first one outside the
    /// alphabet: the zero lanes, narrowed to 4 bits each, count 4 trailing
    /// zeros per quantum before them.
    #[target_feature(enable = "neon")]
    fn first(self) -> usize {
        let zero = vreinterpretq_u16_u8(vceqzq_u8(self.0));
        let mask = vget_lane_u64::<0>(vreinterpret_u64_u8(vshrn_n_u16::<4>(zero)));
        mask.trailing_zeros() as usize
    }
}

/// Decodes 64 characters into 48 bytes: the first 64 of `chars`, or its first
/// `split` and its last ones, picked quantum by quantum from the
/// deinterleaved vectors.
#[inline]
#[target_feature(enable = "neon")]
fn decode<const C: usize>(chars: &[u8; C], split: usize, out: &mut [MaybeUninit<u8>; 48]) -> Bad {
    const { assert!(C >= 64) };

    // SAFETY: the loads read 64 bytes of `chars` and 16 of the masks.
    let chars = unsafe {
        let (first, last) = (
            vld4q_u8(chars.as_ptr()),
            vld4q_u8(chars.as_ptr().add(C - 64)),
        );
        let mask = vld1q_u8(crate::driver::stitch(split / 4).as_ptr());

        uint8x16x4_t(
            vbslq_u8(mask, last.0, first.0),
            vbslq_u8(mask, last.1, first.1),
            vbslq_u8(mask, last.2, first.2),
            vbslq_u8(mask, last.3, first.3),
        )
    };

    let (low, high) = (table(&DECODE_LOW), table(&DECODE_HIGH));

    let a = translate(chars.0, low, high);
    let b = translate(chars.1, low, high);
    let c = translate(chars.2, low, high);
    let d = translate(chars.3, low, high);
    let bad = Bad(vminq_u8(vminq_u8(a, b), vminq_u8(c, d)));

    let one = vdupq_n_u8(1);
    let (a, b, c, d) = (
        vsubq_u8(a, one),
        vsubq_u8(b, one),
        vsubq_u8(c, one),
        vsubq_u8(d, one),
    );

    let bytes = uint8x16x3_t(
        vorrq_u8(vshlq_n_u8::<2>(a), vshrq_n_u8::<4>(b)),
        vorrq_u8(vshlq_n_u8::<4>(b), vshrq_n_u8::<2>(c)),
        vorrq_u8(vshlq_n_u8::<6>(c), d),
    );

    // SAFETY: the store writes the 48 bytes of `out`.
    unsafe { vst3q_u8(out.as_mut_ptr().cast(), bytes) };

    bad
}

/// Encodes 48 bytes into 64 characters; given room beyond 64, a line ending
/// at `split`, as `spread` writes it.
#[inline]
#[target_feature(enable = "neon")]
fn encode<const W: usize, const N: usize>(
    bytes: &[u8; W],
    split: usize,
    out: &mut [MaybeUninit<u8>; N],
) {
    const { assert!(W == 48 && N >= 64) };

    // SAFETY: the load reads the 48 bytes of `bytes`.
    let bytes = unsafe { vld3q_u8(bytes.as_ptr()) };
    let mask = vdupq_n_u8(0x3f);
    let alphabet = table(ALPHABET);

    let a = vshrq_n_u8::<2>(bytes.0);
    let b = vandq_u8(
        vorrq_u8(vshlq_n_u8::<4>(bytes.0), vshrq_n_u8::<4>(bytes.1)),
        mask,
    );
    let c = vandq_u8(
        vorrq_u8(vshlq_n_u8::<2>(bytes.1), vshrq_n_u8::<6>(bytes.2)),
        mask,
    );
    let d = vandq_u8(bytes.2, mask);

    let chars = uint8x16x4_t(
        vqtbl4q_u8(alphabet, a),
        vqtbl4q_u8(alphabet, b),
        vqtbl4q_u8(alphabet, c),
        vqtbl4q_u8(alphabet, d),
    );

    // SAFETY: the store writes 64 bytes of `out`.
    unsafe { vst4q_u8(out.as_mut_ptr().cast(), chars) };

    if N > 64 {
        crate::driver::spread(out, split, 64);
    }
}

/// Each character's sextet plus one, or zero outside the alphabet: the two
/// tables cover the characters below 128 and give zero for any other.
#[inline]
#[target_feature(enable = "neon")]
fn translate(chars: uint8x16_t, low: uint8x16x4_t, high: uint8x16x4_t) -> uint8x16_t {
    let shifted = veorq_u8(chars, vdupq_n_u8(0x40));
    vorrq_u8(vqtbl4q_u8(low, chars), vqtbl4q_u8(high, shifted))
}

#[target_feature(enable = "neon")]
fn table(bytes: &[u8; 64]) -> uint8x16x4_t {
    // SAFETY: the load reads the 64 bytes of `bytes`.
    unsafe { vld1q_u8_x4(bytes.as_ptr()) }
}

const fn decode_table(base: u8) -> [u8; 64] {
    let mut table = [0; 64];
    let mut sextet = 0;

    while sextet < 64 {
        let ch = ALPHABET[sextet];

        if ch >= base && ch - base < 64 {
            table[(ch - base) as usize] = sextet as u8 + 1;
        }

        sextet += 1;
    }

    table
}

#[cfg(test)]
mod tests {
    use crate::{neon::*, scalar::*};

    #[test]
    fn kernels_match_scalar() {
        // SAFETY: the module only exists on targets guaranteeing NEON.
        check_decode_kernel(|chars, split, out| unsafe { run(decode::<66>(chars, split, out)) });
        // SAFETY: as above.
        check_encode_kernel(0, |bytes, split, out| unsafe {
            encode::<48, 66>(bytes, split, out)
        });
    }

    #[target_feature(enable = "neon")]
    fn run(bad: Bad) -> usize {
        if bad.any() { bad.first() } else { 64 }
    }
}
