//! # Driver
//!
//! The loops of a backend, written once as `base64_driver!` and stamped into
//! each backend module, under its `#[target_feature]` when it has one, so
//! that its block kernels inline into them, as simdutf8 0.1.5 stamps its
//! algorithm. Each loop is a function of its own, so that it keeps its
//! constants in registers; the decoder and the encoder reach them through the
//! backend's [`Loops`].
//!
//! A backend module defines its two kernels and `Bad`, the characters outside
//! the alphabet a decode kernel met, which tells whether there were `any` and
//! how many characters came `first`, and `join`s with others unless the
//! backend tests each block apart (`EAGER`):
//!
//! - The decode kernel fills its output: the bytes of its block, then as many
//!   spare bytes as the invocation names when the output has room for them.
//!   Given more characters than a block, it stitches its block from the
//!   characters before `split` and those as far past them as it was given
//!   more, which leaves out a line break.
//! - The encode kernel reads its input: a window of the length the invocation
//!   names, its group starting at the lead, so that it can load bytes around
//!   it, or the group alone, for the blocks no window fits around. Given more
//!   room than its characters, it writes those from `split` on as far past as
//!   it was given more, which leaves room for a line ending.
//!
//! Lines of 76 characters go a period at a time: the lines whose characters
//! make 19 blocks, 76 being 4 times 19, each block's place known at compile
//! time, a block across a line break decoded from its two lines stitched and
//! encoded with its characters past the line moved past the line break. Fewer
//! lines than a period, and those past the whole periods, go line by line. A
//! run or a line ends with a block ending at its end and overlapping the one
//! before, which is harmless since a quantum decodes to the same bytes
//! wherever it falls; a run shorter than one block goes to the narrower loop
//! the invocation names.

use core::mem::MaybeUninit;

/// Wide blocks a decoding run takes per group, tested once unless the backend
/// tests each block apart.
pub(crate) const DECODE_UNROLL: usize = 4;

/// Wide blocks an encoding run writes per step.
pub(crate) const ENCODE_UNROLL: usize = 8;

/// The loops of a backend, which the decoder and the encoder run.
///
/// NOTE: unsafe to call, as a SIMD backend's loops run only on a CPU that
/// has its features, which a `Base64Cpu` token guarantees.
#[allow(clippy::type_complexity)]
pub(crate) struct Loops {
    /// Decodes the whole quanta of alphabet characters at the start of `src`
    /// that fit in `dst`, up to the first other byte, returning the
    /// characters read.
    pub(crate) decode_run: unsafe fn(&[u8], &mut [MaybeUninit<u8>]) -> usize,
    /// Decodes whole lines of `LINE` characters, ended by CRLF or by LF
    /// alone, while they are well formed and fit in `dst`, returning how
    /// many.
    pub(crate) decode_lines: unsafe fn(&[u8], &mut [MaybeUninit<u8>], bool) -> usize,
    /// Encodes the whole groups at the start of `src` that fit in `dst`,
    /// returning the bytes read.
    pub(crate) encode_run: unsafe fn(&[u8], &mut [MaybeUninit<u8>]) -> usize,
    /// Encodes the whole lines of `src` that fit in `dst`, each ended by the
    /// given line ending, returning how many.
    pub(crate) encode_lines: unsafe fn(&[u8], &mut [MaybeUninit<u8>], &[u8]) -> usize,
}

/// `dst` as the uninitialised bytes the kernels write into.
pub(crate) fn uninit(dst: &mut [u8]) -> &mut [MaybeUninit<u8>] {
    // SAFETY: `MaybeUninit<u8>` has the layout of `u8`, and the codec only
    // ever writes initialised bytes through the result, so `dst` stays
    // initialised.
    unsafe { &mut *(dst as *mut [u8] as *mut [MaybeUninit<u8>]) }
}

/// The mask of a block stitched at `split`, at most 32: its bytes from the
/// split on are set.
pub(crate) fn stitch(split: usize) -> &'static [u8] {
    static MASKS: [[u8; 32]; 2] = [[0; 32], [0xff; 32]];
    &MASKS.as_flattened()[32 - split..]
}

/// Moves the characters of `out` from `split` to `chars` past the line
/// ending that fills the rest of `out`, a quantum at a time from the last,
/// for the kernels with no masked store.
#[inline]
pub(crate) fn spread(out: &mut [MaybeUninit<u8>], split: usize, chars: usize) {
    let gap = out.len() - chars;

    for at in (split..chars).step_by(4).rev() {
        let Some(&quantum) = out[at..].first_chunk::<4>() else {
            return;
        };

        if let Some(slot) = out[at + gap..].first_chunk_mut() {
            *slot = quantum;
        }
    }
}

/// Expands `$body` once per block of a period, its index `$block` a constant
/// from 0 to 18.
///
/// NOTE: written out rather than looped, as LLVM rolls the 19 blocks back
/// into a loop, which then works each block's place out at run time.
macro_rules! base64_period {
    ($block:ident => $body:block) => {
        base64_period!(@ $block $body 0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18)
    };
    (@ $block:ident $body:block $($index:literal)*) => {
        $({
            const $block: usize = $index;
            $body
        })*
    };
}

/// Stamps a backend's `LOOPS` from its kernels: for decoding, the characters
/// a block takes, the spare bytes its kernel writes when it has room, the
/// kernel and the narrower run; for encoding, the bytes a block takes, the
/// window its kernel reads and the lead of the group in it, the kernel and
/// the narrower run.
macro_rules! base64_driver {
    (
        $(#[$attr:meta])*
        decode(
            chars: $chars:literal,
            spare: $spare:literal,
            kernel: $decode:ident,
            narrow: $decode_narrow:path $(,)?
        ),
        encode(
            bytes: $bytes:literal,
            window: $window:literal,
            lead: $lead:literal,
            kernel: $encode:ident,
            narrow: $encode_narrow:path $(,)?
        ) $(,)?
    ) => {
        /// The loops of the backend.
        pub(crate) const LOOPS: crate::driver::Loops = crate::driver::Loops {
            decode_run,
            decode_lines,
            encode_run,
            encode_lines,
        };

        /// Decodes the whole quanta of alphabet characters at the start of
        /// `src` that fit in `dst`, returning the characters read: in blocks,
        /// or by the narrower loop below one block.
        ///
        /// NOTE: the blocks are a function of their own, so that a short run
        /// reaches the narrower loop before any register is saved.
        $(#[$attr])*
        pub(crate) fn decode_run(src: &[u8], dst: &mut [core::mem::MaybeUninit<u8>]) -> usize {
            let chars = (src.len() / 4).min(dst.len() / 3) * 4;

            if chars < $chars {
                return $decode_narrow(src, dst);
            }

            decode_blocks(&src[..chars], dst)
        }

        /// Decodes `src`, whole quanta at least one block long, up to its
        /// first character outside the alphabet, returning the characters
        /// read: groups of blocks writing spare bytes while those fall within
        /// the run's output, then blocks writing their own alone, the last
        /// one ending at the run's end.
        $(#[$attr])*
        #[inline(never)]
        fn decode_blocks(src: &[u8], dst: &mut [core::mem::MaybeUninit<u8>]) -> usize {
            const WIDE: usize = $chars;
            const BYTES: usize = WIDE / 4 * 3;
            const UNROLL: usize = crate::driver::DECODE_UNROLL;

            let chars = src.len();

            // NOTE: a group ending 2 spare lengths of characters before the
            // run's end has its spare bytes within the run's output.
            let groups = chars.saturating_sub(2 * $spare) / (UNROLL * WIDE);
            let mut at = 0;

            'groups: for group in 0..groups {
                let (Some(group), Some(out)) = (
                    src[group * UNROLL * WIDE..].first_chunk::<{ UNROLL * WIDE }>(),
                    dst[group * UNROLL * BYTES..].first_chunk_mut::<{ UNROLL * BYTES + $spare }>(),
                ) else {
                    return at;
                };

                let (blocks, _) = group.as_chunks::<WIDE>();
                let mut bad = Bad::none();

                for (block, chars) in blocks.iter().enumerate() {
                    let Some(bytes) = out[block * BYTES..].first_chunk_mut::<{ BYTES + $spare }>() else {
                        return at;
                    };

                    let found = $decode(chars, WIDE, bytes);

                    if Bad::EAGER && found.any() {
                        break 'groups;
                    }

                    bad = bad.join(found);
                }

                if !Bad::EAGER && bad.any() {
                    break;
                }

                at += UNROLL * WIDE;
            }

            while at < chars {
                let start = at.min(chars - WIDE);

                let (Some(block), Some(bytes)) = (
                    src[start..].first_chunk::<WIDE>(),
                    dst[start / 4 * 3..].first_chunk_mut::<BYTES>(),
                ) else {
                    return at;
                };

                let bad = $decode(block, WIDE, bytes);

                if bad.any() {
                    return start + bad.first();
                }

                at = start + WIDE;
            }

            chars
        }

        /// Decodes whole lines of `LINE` characters, ended by CRLF or by LF
        /// when `crlf` is false, while they are well formed and `dst` has
        /// room for the spare bytes past them, returning how many.
        $(#[$attr])*
        fn decode_lines(src: &[u8], dst: &mut [core::mem::MaybeUninit<u8>], crlf: bool) -> usize {
            const LINES: usize = $chars / 4;

            match crlf {
                true => decode_lines_of::<{ $chars + 2 }, { LINES * 78 }>(src, dst),
                false => decode_lines_of::<{ $chars + 1 }, { LINES * 77 }>(src, dst),
            }
        }

        /// `decode_lines` with the `C - $chars` bytes of CRLF or LF ending
        /// each line, `SPAN` bytes a period: by `decode_periods` when there
        /// are lines enough for a period, with blocks of 32 characters or
        /// more, line by line otherwise, blocks writing spare bytes, the last
        /// one ending at the line's end, tested once per line unless the
        /// backend tests each block apart.
        ///
        /// NOTE: with blocks of 16 characters, the periods measured no faster
        /// than the lines for SSSE3 and 200 cycles a call slower for scalar.
        $(#[$attr])*
        #[inline(never)]
        fn decode_lines_of<const C: usize, const SPAN: usize>(
            src: &[u8],
            dst: &mut [core::mem::MaybeUninit<u8>],
        ) -> usize {
            const WIDE: usize = $chars;
            const LINE: usize = crate::scalar::LINE;
            const LINES: usize = WIDE / 4;
            const LEN: usize = crate::scalar::LINE_BYTES;

            let span = SPAN / LINES;
            let eol = &b"\r\n"[2 + WIDE - C..];
            let lines = (src.len() / span).min(dst.len().saturating_sub($spare) / LEN);

            if WIDE >= 32 && lines >= LINES {
                return decode_periods::<C, SPAN>(src, dst, lines);
            }

            let mut done = 0;

            for line in 0..lines {
                let (Some(chars), Some(out)) = (
                    src.get(line * span..line * span + span),
                    dst.get_mut(line * LEN..line * LEN + LEN + $spare),
                ) else {
                    break;
                };

                if chars.get(LINE..span) != Some(eol) {
                    break;
                }

                let mut bad = Bad::none();
                let mut at = 0;

                while at < LINE {
                    let start = at.min(LINE - WIDE);

                    let (Some(block), Some(bytes)) = (
                        chars[start..].first_chunk::<WIDE>(),
                        out[start / 4 * 3..].first_chunk_mut::<{ WIDE / 4 * 3 + $spare }>(),
                    ) else {
                        return done;
                    };

                    let found = $decode(block, WIDE, bytes);

                    if Bad::EAGER && found.any() {
                        return done;
                    }

                    bad = bad.join(found);
                    at = start + WIDE;
                }

                if !Bad::EAGER && bad.any() {
                    break;
                }

                done += 1;
            }

            done
        }

        /// `decode_lines_of` over the first `lines` lines of `src`, in whole
        /// periods of `$chars / 4` lines, `SPAN` bytes, each the 19 blocks of
        /// its characters, a block across a line break stitched from its two
        /// lines, tested once per period unless the backend tests each block
        /// apart, then by `decode_lines_of` the lines past them, or those of
        /// a period that failed.
        $(#[$attr])*
        #[inline(never)]
        fn decode_periods<const C: usize, const SPAN: usize>(
            src: &[u8],
            dst: &mut [core::mem::MaybeUninit<u8>],
            lines: usize,
        ) -> usize {
            const WIDE: usize = $chars;
            const BYTES: usize = WIDE / 4 * 3;
            const LINE: usize = crate::scalar::LINE;
            const LINES: usize = WIDE / 4;
            const LEN: usize = crate::scalar::LINE_BYTES;

            let span = SPAN / LINES;
            let eol = &b"\r\n"[2 + WIDE - C..];
            let (periods, _) = src[..lines * span].as_chunks::<SPAN>();
            let mut done = 0;

            'periods: for chars in periods {
                let Some(out) = dst[done * LEN..].first_chunk_mut::<{ LINES * LEN + $spare }>() else {
                    break 'periods;
                };

                for line in 0..LINES {
                    if chars[line * span + LINE..][..eol.len()] != *eol {
                        break 'periods;
                    }
                }

                let mut bad = Bad::none();

                base64_period!(BLOCK => {
                    let at = BLOCK * WIDE;
                    let (from, split) = (at + at / LINE * eol.len(), (LINE - at % LINE).min(WIDE));

                    let (Some(window), Some(bytes)) = (
                        chars[from..].first_chunk::<C>(),
                        out[BLOCK * BYTES..].first_chunk_mut::<{ BYTES + $spare }>(),
                    ) else {
                        break 'periods;
                    };

                    let found = $decode(window, split, bytes);

                    if Bad::EAGER && found.any() {
                        break 'periods;
                    }

                    bad = bad.join(found);
                });

                if !Bad::EAGER && bad.any() {
                    break;
                }

                done += LINES;
            }

            // NOTE: the lines of a period that failed but its last go line by
            // line, which stops at the line at fault.
            let rest = (lines - done).min(LINES - 1);
            done + decode_lines_of::<C, SPAN>(&src[done * span..(done + rest) * span], &mut dst[done * LEN..])
        }

        /// Encodes the whole groups at the start of `src` that fit in `dst`,
        /// returning the bytes read: in blocks, or by the narrower loop below
        /// one block, as `decode_run` does.
        $(#[$attr])*
        pub(crate) fn encode_run(src: &[u8], dst: &mut [core::mem::MaybeUninit<u8>]) -> usize {
            let bytes = (src.len() / 3).min(dst.len() / 4) * 3;

            if bytes < $bytes {
                return $encode_narrow(src, dst);
            }

            encode_blocks(&src[..bytes], dst)
        }

        /// Encodes `src`, whole groups at least one block long, returning
        /// the bytes read: steps of blocks reading windows around their
        /// groups, then single blocks, reading their group alone where no
        /// window fits, the last one ending at the run's end.
        $(#[$attr])*
        #[inline(never)]
        fn encode_blocks(src: &[u8], dst: &mut [core::mem::MaybeUninit<u8>]) -> usize {
            const BYTES: usize = $bytes;
            const CHARS: usize = BYTES / 3 * 4;
            const STEP: usize = crate::driver::ENCODE_UNROLL * BYTES;
            const WINDOWS: usize = STEP - BYTES + $window;

            let bytes = src.len();
            let dst = &mut dst[..bytes / 3 * 4];

            let (Some(group), Some(chars)) = (src.first_chunk::<BYTES>(), dst.first_chunk_mut::<CHARS>()) else {
                return 0;
            };

            $encode(group, CHARS, chars);
            let mut at = BYTES;
            let (steps, _) = dst[CHARS..].as_chunks_mut::<{ crate::driver::ENCODE_UNROLL * CHARS }>();

            for outs in steps {
                let Some(windows) = src[at - $lead..].first_chunk::<WINDOWS>() else {
                    break;
                };

                let (outs, _) = outs.as_chunks_mut::<CHARS>();

                for (block, chars) in outs.iter_mut().enumerate() {
                    let Some(window) = windows[block * BYTES..].first_chunk::<$window>() else {
                        return at;
                    };

                    $encode(window, CHARS, chars);
                }

                at += STEP;
            }

            while at < bytes {
                let start = at.min(bytes - BYTES);
                let window = start.checked_sub($lead).and_then(|lead| src[lead..].first_chunk::<$window>());

                match (window, src[start..].first_chunk::<BYTES>(), dst[start / 3 * 4..].first_chunk_mut::<CHARS>()) {
                    (Some(window), _, Some(chars)) => $encode(window, CHARS, chars),
                    (None, Some(group), Some(chars)) => $encode(group, CHARS, chars),
                    _ => return at,
                }

                at = start + BYTES;
            }

            bytes
        }

        /// Encodes the whole lines of `src` that fit in `dst`, each ended by
        /// `eol`, returning how many.
        $(#[$attr])*
        fn encode_lines(src: &[u8], dst: &mut [core::mem::MaybeUninit<u8>], eol: &[u8]) -> usize {
            const LINES: usize = $bytes / 3;
            const CHARS: usize = $bytes / 3 * 4;

            match eol.len() {
                2 => encode_lines_of::<{ CHARS + 2 }, { LINES * 78 }>(src, dst),
                _ => encode_lines_of::<{ CHARS + 1 }, { LINES * 77 }>(src, dst),
            }
        }

        /// `encode_lines` with the `N` less a block's characters bytes of
        /// CRLF or LF ending each line, `SPAN` bytes a period: by
        /// `encode_periods` when there are lines enough for a period, line by
        /// line otherwise, each line's last block ending at its end.
        ///
        /// NOTE: the ending's length is a constant, so that it is written
        /// with plain stores and not a copy call per line.
        $(#[$attr])*
        #[inline(never)]
        fn encode_lines_of<const N: usize, const SPAN: usize>(
            src: &[u8],
            dst: &mut [core::mem::MaybeUninit<u8>],
        ) -> usize {
            const BYTES: usize = $bytes;
            const CHARS: usize = BYTES / 3 * 4;
            const LINE: usize = crate::scalar::LINE;
            const LINES: usize = BYTES / 3;
            const LEN: usize = crate::scalar::LINE_BYTES;

            let span = SPAN / LINES;
            let eol = &b"\r\n"[2 + CHARS - N..];
            let lines = (src.len() / LEN).min(dst.len() / span);

            if lines >= LINES {
                return encode_periods::<N, SPAN>(src, dst, lines);
            }

            let (whole, _) = src.as_chunks::<LEN>();

            for (line, out) in whole.iter().zip(dst.chunks_exact_mut(span)).take(lines) {
                let mut at = 0;

                while at < LEN {
                    let start = at.min(LEN - BYTES);

                    let (Some(group), Some(chars)) = (
                        line[start..].first_chunk::<BYTES>(),
                        out[start / 3 * 4..].first_chunk_mut::<CHARS>(),
                    ) else {
                        return 0;
                    };

                    $encode(group, CHARS, chars);
                    at = start + BYTES;
                }

                crate::scalar::write_eol(&mut out[LINE..], eol);
            }

            lines
        }

        /// `encode_lines_of` over the first `lines` lines of `src`, in whole
        /// periods of `$bytes / 3` lines, `SPAN` bytes, each the 19 blocks of
        /// its bytes, reading windows where they fit, the characters of a
        /// block across a line break past the line moved past it, then the
        /// lines past them by `encode_lines_of`.
        $(#[$attr])*
        #[inline(never)]
        fn encode_periods<const N: usize, const SPAN: usize>(
            src: &[u8],
            dst: &mut [core::mem::MaybeUninit<u8>],
            lines: usize,
        ) -> usize {
            const BYTES: usize = $bytes;
            const CHARS: usize = BYTES / 3 * 4;
            const LINE: usize = crate::scalar::LINE;
            const LINES: usize = BYTES / 3;
            const LEN: usize = crate::scalar::LINE_BYTES;

            let span = SPAN / LINES;
            let eol = &b"\r\n"[2 + CHARS - N..];
            let (periods, _) = src[..lines * LEN].as_chunks::<{ LINES * LEN }>();
            let (outs, _) = dst.as_chunks_mut::<SPAN>();

            for (bytes, out) in periods.iter().zip(outs) {
                base64_period!(BLOCK => {
                    let at = BLOCK * CHARS;
                    let (to, split) = (at + at / LINE * eol.len(), (LINE - at % LINE).min(CHARS));
                    let from = BLOCK * BYTES;
                    let window = from.checked_sub($lead).and_then(|lead| bytes[lead..].first_chunk::<$window>());

                    let (Some(chars), Some(group)) = (
                        out[to..].first_chunk_mut::<N>(),
                        bytes[from..].first_chunk::<BYTES>(),
                    ) else {
                        return 0;
                    };

                    match window {
                        Some(window) => $encode(window, split, chars),
                        None => $encode(group, split, chars),
                    }
                });

                for line in 0..LINES {
                    crate::scalar::write_eol(&mut out[line * span + LINE..], eol);
                }
            }

            let done = periods.len() * LINES;
            done + encode_lines_of::<N, SPAN>(&src[done * LEN..lines * LEN], &mut dst[done * span..])
        }
    };
}

#[cfg(test)]
mod tests {
    use alloc::{vec, vec::Vec};

    use crate::{
        cpu::{Base64Cpu, available},
        decode::{Base64Decode, decode, decoded_len_max},
        driver::uninit,
        encode::{Base64Encode, Base64EncodeOptions, Base64Eol, encode, encoded_len},
        scalar::{self, DecodeState, EncodeState},
    };

    const OPTIONS: [Base64EncodeOptions; 3] = [
        Base64EncodeOptions { wrap: None },
        Base64EncodeOptions {
            wrap: Some(Base64Eol::Crlf),
        },
        Base64EncodeOptions {
            wrap: Some(Base64Eol::Lf),
        },
    ];

    /// A xorshift generator, so that every run tests the same inputs.
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }

        fn bytes(&mut self, len: usize) -> Vec<u8> {
            (0..len).map(|_| self.next() as u8).collect()
        }
    }

    /// Decoding through the scalar steps alone, byte by byte.
    fn reference_decode(src: &[u8]) -> (Vec<u8>, bool) {
        let mut state = DecodeState::new();
        let mut out = vec![0; decoded_len_max(src.len())];
        let (mut read, mut written) = (0, 0);

        loop {
            let (more_read, more_written, done) =
                scalar::decode_bytes(&mut state, &src[read..], uninit(&mut out[written..]));

            read += more_read;
            written += more_written;

            if done {
                break;
            }
        }

        let end = scalar::decode_finish(&mut state, uninit(&mut out[written..])).unwrap();
        out.truncate(written + end);
        (out, state.clean)
    }

    /// Encoding through the scalar steps alone, group by group.
    fn reference_encode(src: &[u8], opts: Base64EncodeOptions) -> Vec<u8> {
        let eol: &'static [u8] = match opts.wrap {
            None => b"",
            Some(Base64Eol::Crlf) => b"\r\n",
            Some(Base64Eol::Lf) => b"\n",
        };

        let mut state = EncodeState::new(eol);
        let mut out = vec![0; encoded_len(src.len(), opts).unwrap()];
        let (_, written) = scalar::encode_bytes(&mut state, src, uninit(&mut out));
        let end = scalar::encode_finish(&mut state, uninit(&mut out[written..])).unwrap();
        out.truncate(written + end);
        out
    }

    fn one_shot_decode(src: &[u8], cpu: Base64Cpu) -> (Vec<u8>, bool) {
        let mut out = vec![0; decoded_len_max(src.len())];
        let output = decode(src, &mut out, cpu).unwrap();
        assert_eq!(output.read, src.len());
        out.truncate(output.written);
        (out, output.clean)
    }

    fn one_shot_encode(src: &[u8], cpu: Base64Cpu, opts: Base64EncodeOptions) -> Vec<u8> {
        let mut out = vec![0; encoded_len(src.len(), opts).unwrap()];
        let written = encode(src, &mut out, cpu, opts).unwrap();
        out.truncate(written);
        out
    }

    /// `encoded` rewrapped in lines of `width` characters ended by `eol`.
    fn rewrapped(encoded: &[u8], width: usize, eol: &[u8]) -> Vec<u8> {
        let chars: Vec<u8> = encoded
            .iter()
            .copied()
            .filter(|byte| !byte.is_ascii_whitespace())
            .collect();

        let mut out = Vec::new();

        for line in chars.chunks(width) {
            out.extend_from_slice(line);
            out.extend_from_slice(eol);
        }

        out
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn every_backend_matches_the_reference_at_every_length() {
        let mut rng = Rng(0x9e37_79b9_7f4a_7c15);

        for len in 0..=1024 {
            let input = rng.bytes(len);

            for opts in OPTIONS {
                let expected = reference_encode(&input, opts);
                assert_eq!(expected.len(), encoded_len(len, opts).unwrap());
                assert_eq!(reference_decode(&expected), (input.clone(), true));

                let mut variants = vec![
                    expected.clone(),
                    rewrapped(&expected, 64, b"\r\n"),
                    rewrapped(&expected, 60, b"\n"),
                    rewrapped(&expected, 72, b" \r\n"),
                ];

                if !expected.is_empty() {
                    let mut stray = expected.clone();
                    let at = rng.next() as usize % stray.len();
                    stray.insert(at, rng.next() as u8);
                    variants.push(stray);
                }

                for cpu in available() {
                    assert_eq!(
                        one_shot_encode(&input, cpu, opts),
                        expected,
                        "{cpu:?} {opts:?} {len}"
                    );

                    for variant in &variants {
                        assert_eq!(
                            one_shot_decode(variant, cpu),
                            reference_decode(variant),
                            "{cpu:?} {opts:?} {len}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn every_byte_at_every_position_matches_the_reference() {
        let run = reference_encode(&Rng(7).bytes(120), OPTIONS[0]);
        let lines = reference_encode(&Rng(7).bytes(171), OPTIONS[1]);

        for line in [run, lines] {
            for byte in 0..=255 {
                for at in 0..line.len() {
                    let mut input = line.clone();
                    input[at] = byte;
                    let expected = reference_decode(&input);

                    for cpu in available() {
                        assert_eq!(
                            one_shot_decode(&input, cpu),
                            expected,
                            "{cpu:?} {byte} {at}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn every_short_end_matches_the_reference() {
        const BYTES: &[u8; 7] = b"Qz=\r\n *";

        for len in 0..=6_u32 {
            for mut code in 0..BYTES.len().pow(len) {
                let end: Vec<u8> = (0..len)
                    .map(|_| {
                        let byte = BYTES[code % BYTES.len()];
                        code /= BYTES.len();
                        byte
                    })
                    .collect();

                for prefix in [&b""[..], b"QUJD", b"QUJD\r\n"] {
                    let input = [prefix, &end].concat();
                    let expected = reference_decode(&input);

                    for cpu in available() {
                        assert_eq!(one_shot_decode(&input, cpu), expected, "{cpu:?} {input:?}");

                        let mut decoder = Base64Decode::new(cpu);
                        let mut out = vec![0; decoded_len_max(input.len())];
                        let first = decoder.resume(prefix, &mut out);
                        let second = decoder.resume(&end, &mut out[first.written..]);
                        let written = first.written + second.written;
                        let last = decoder.finish(&mut out[written..]).unwrap();
                        out.truncate(written + last.written);
                        assert_eq!((out, last.clean), expected, "{cpu:?} {input:?} resumed");
                    }
                }
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn every_split_matches_the_one_shot() {
        let mut rng = Rng(42);
        let input = rng.bytes(300);

        for opts in OPTIONS {
            let encoded = reference_encode(&input, opts);
            let mut dirty = encoded.clone();
            dirty.insert(100, b'*');
            dirty.extend_from_slice(b"=Zg");

            for cpu in available() {
                for at in 0..=input.len() {
                    let mut encoder = Base64Encode::new(cpu, opts);
                    let mut out = vec![0; encoded.len()];
                    let first = encoder.resume(&input[..at], &mut out);
                    let second = encoder.resume(&input[at..], &mut out[first.written..]);
                    let written = first.written + second.written;
                    let end = encoder.finish(&mut out[written..]).unwrap();
                    assert_eq!(written + end, encoded.len(), "{cpu:?} {opts:?} {at}");
                    assert_eq!(out, encoded, "{cpu:?} {opts:?} {at}");
                }

                for source in [&encoded, &dirty] {
                    let expected = reference_decode(source);

                    for at in 0..=source.len() {
                        let mut decoder = Base64Decode::new(cpu);
                        let mut out = vec![0; decoded_len_max(source.len())];
                        let first = decoder.resume(&source[..at], &mut out);
                        let second = decoder.resume(&source[at..], &mut out[first.written..]);
                        let written = first.written + second.written;
                        let end = decoder.finish(&mut out[written..]).unwrap();
                        out.truncate(written + end.written);
                        assert_eq!((out, end.clean), expected, "{cpu:?} {opts:?} {at}");
                    }
                }
            }
        }
    }
}
