//! # Driver
//!
//! The loops of a backend, written once as `base64_driver!` and stamped into
//! each backend module under its target features, so that its kernels inline
//! into them, as simdutf8 0.1.5 stamps its algorithm. The decoder and the
//! encoder call them through the backend's [`Loops`].
//!
//! A backend defines `Bad`, the characters outside the alphabet a decode
//! kernel met, and two kernels. The decode kernel decodes a block, then as
//! many spare bytes as its output has room for; given more characters than a
//! block, it takes those before `split` and the last ones, which leaves out a
//! line break. The encode kernel encodes `K` groups a group apart, each read
//! from a window around it, or a lone group; given more room than a block's
//! characters, it writes those from `split` on that much further and a line
//! ending of that length at `split`, when `split` is not past them.
//!
//! A run ends with a block overlapping the one before, as a quantum decodes
//! to the same bytes wherever it falls. Lines of 76 characters go a period at
//! a time: the lines whose characters make 19 blocks, 76 being 4 times 19,
//! each block's place a constant.

use core::mem::MaybeUninit;

/// A loop of a backend over `src` into `dst`, unsafe to call on a CPU without
/// the backend's features.
pub(crate) type Run = unsafe fn(&[u8], &mut [MaybeUninit<u8>]) -> usize;

/// The loops of a backend.
pub(crate) struct Loops {
    /// Decodes the whole quanta of alphabet characters at the start of `src`
    /// that fit in `dst`, returning the characters read.
    pub(crate) decode_run: Run,
    /// Decodes whole lines of `LINE` characters, ended by LF then by CRLF,
    /// while well formed and fitting in `dst`, returning how many.
    pub(crate) decode_lines: [Run; 2],
    /// Encodes the whole groups at the start of `src` that fit in `dst`,
    /// returning the bytes read.
    pub(crate) encode_run: Run,
    /// Encodes the whole lines of `src` that fit in `dst`, ended by LF then
    /// by CRLF, returning how many.
    pub(crate) encode_lines: [Run; 2],
}

/// `dst` as the uninitialised bytes the kernels write into.
pub(crate) fn uninit(dst: &mut [u8]) -> &mut [MaybeUninit<u8>] {
    // SAFETY: same layout, and only initialised bytes are written through it.
    unsafe { &mut *(dst as *mut [u8] as *mut [MaybeUninit<u8>]) }
}

/// The mask of a block stitched at `split`, at most 32: its bytes from the
/// split on are set.
pub(crate) fn stitch(split: usize) -> &'static [u8] {
    static MASKS: [[u8; 32]; 2] = [[0; 32], [0xff; 32]];
    &MASKS.as_flattened()[32 - split..]
}

/// Moves the characters of `out` from `split` to `chars` past the room left at
/// its end, a quantum at a time from the last, and writes the line ending of
/// that length at `split` when it is not past `chars`.
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

    if split <= chars {
        crate::scalar::write_eol(&mut out[split..], &b"\r\n"[2 - gap..]);
    }
}

/// Expands `$body` once per block of a period, `$block` a constant from 0 to
/// 18.
///
/// NOTE: written out, as LLVM rolls the blocks of a loop back into one that
/// works each block's place out at run time.
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
/// of a block, the spare bytes its kernel writes and the narrower run; for
/// encoding, the bytes of a block, the window its kernel reads, the lead of
/// the group in it and the narrower run.
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
            decode_lines: [
                decode_lines::<{ $chars + 1 }, { $chars / 4 * 77 }>,
                decode_lines::<{ $chars + 2 }, { $chars / 4 * 78 }>,
            ],
            encode_run,
            encode_lines: [
                encode_lines::<{ $bytes / 3 * 4 + 1 }, { $bytes / 3 * 77 }>,
                encode_lines::<{ $bytes / 3 * 4 + 2 }, { $bytes / 3 * 78 }>,
            ],
        };

        /// `Loops::decode_run`: in blocks, or by the narrower run below one.
        ///
        /// NOTE: the blocks are a function of their own, so that a short run
        /// saves no register.
        $(#[$attr])*
        pub(crate) fn decode_run(src: &[u8], dst: &mut [core::mem::MaybeUninit<u8>]) -> usize {
            let chars = (src.len() / 4).min(dst.len() / 3) * 4;

            if chars < $chars {
                return $decode_narrow(src, dst);
            }

            decode_blocks(&src[..chars], dst)
        }

        /// Decodes `src`, whole quanta of a block or more, up to its first
        /// character outside the alphabet: groups of four blocks writing spare
        /// bytes while those stay within the run's output, tested once unless
        /// `Bad::EAGER`, then single blocks, the last one ending at the run's
        /// end.
        $(#[$attr])*
        #[inline(never)]
        fn decode_blocks(src: &[u8], dst: &mut [core::mem::MaybeUninit<u8>]) -> usize {
            const WIDE: usize = $chars;
            const BYTES: usize = WIDE / 4 * 3;
            const UNROLL: usize = 4;

            let chars = src.len();
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

        /// `Loops::decode_lines` with `C - $chars` bytes of line ending and
        /// `SPAN` bytes a period: in periods when there are lines enough and
        /// blocks of 32 characters or more, else line by line, the last line by
        /// `decode_blocks` when its spare bytes do not fit.
        ///
        /// NOTE: periods of 16-character blocks measured level for SSSE3 and
        /// slower for scalar.
        $(#[$attr])*
        #[inline(never)]
        fn decode_lines<const C: usize, const SPAN: usize>(
            src: &[u8],
            dst: &mut [core::mem::MaybeUninit<u8>],
        ) -> usize {
            const WIDE: usize = $chars;
            const LINE: usize = crate::scalar::LINE;
            const LINES: usize = WIDE / 4;
            const LEN: usize = crate::scalar::LINE_BYTES;

            let span = SPAN / LINES;
            let eol = &b"\r\n"[2 + WIDE - C..];
            let lines = (src.len() / span).min(dst.len() / LEN);

            if WIDE >= 32 && lines >= LINES {
                return decode_periods::<C, SPAN>(src, dst, lines);
            }

            for (line, chars) in src.chunks_exact(span).take(lines).enumerate() {
                if chars[LINE..] != *eol {
                    return line;
                }

                let Some(out) = dst.get_mut(line * LEN..line * LEN + LEN + $spare) else {
                    return line + (decode_blocks(&chars[..LINE], &mut dst[line * LEN..]) == LINE) as usize;
                };

                let mut bad = Bad::none();

                for start in (0..LINE).step_by(WIDE).map(|at| at.min(LINE - WIDE)) {
                    let (Some(block), Some(bytes)) = (
                        chars[start..].first_chunk::<WIDE>(),
                        out[start / 4 * 3..].first_chunk_mut::<{ WIDE / 4 * 3 + $spare }>(),
                    ) else {
                        return line;
                    };

                    let found = $decode(block, WIDE, bytes);

                    if Bad::EAGER && found.any() {
                        return line;
                    }

                    bad = bad.join(found);
                }

                if !Bad::EAGER && bad.any() {
                    return line;
                }
            }

            lines
        }

        /// `decode_lines` over the first `lines` lines of `src`, in whole
        /// periods, a block across a line break stitched from its two lines,
        /// then line by line over the lines left or those of a period that
        /// failed.
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

                if chars.chunks_exact(span).any(|line| line[LINE..] != *eol) {
                    break;
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

            let rest = (lines - done).min(LINES - 1);
            done + decode_lines::<C, SPAN>(&src[done * span..(done + rest) * span], &mut dst[done * LEN..])
        }

        /// `Loops::encode_run`: in blocks, or by the narrower run below one.
        $(#[$attr])*
        pub(crate) fn encode_run(src: &[u8], dst: &mut [core::mem::MaybeUninit<u8>]) -> usize {
            let bytes = (src.len() / 3).min(dst.len() / 4) * 3;

            if bytes < $bytes {
                return $encode_narrow(src, dst);
            }

            encode_blocks(&src[..bytes], dst)
        }

        /// Encodes `src`, whole groups of a block or more: the first block,
        /// then steps of four blocks reading windows around their groups, then
        /// single blocks reading their group alone where no window fits, the
        /// last one ending at the run's end.
        ///
        /// NOTE: blocks of 64 characters store whole cache lines, up to 15%
        /// faster aligned, so their steps start at the first aligned one.
        $(#[$attr])*
        #[inline(never)]
        fn encode_blocks(src: &[u8], dst: &mut [core::mem::MaybeUninit<u8>]) -> usize {
            const BYTES: usize = $bytes;
            const CHARS: usize = BYTES / 3 * 4;
            const UNROLL: usize = 4;
            const STEP: usize = UNROLL * BYTES;
            const WINDOWS: usize = STEP - BYTES + $window;

            let bytes = src.len();
            let dst = &mut dst[..bytes / 3 * 4];

            let (Some(group), Some(chars)) = (src.first_chunk::<BYTES>(), dst.first_chunk_mut::<CHARS>()) else {
                return 0;
            };

            $encode(group, CHARS, [chars]);

            let lag = match dst.as_ptr().addr().wrapping_neg() % 64 {
                lag if CHARS == 64 && bytes > STEP && lag % 4 == 0 && lag > 0 => lag,
                _ => CHARS,
            };

            let mut at = lag / 4 * 3;
            let (steps, _) = dst[lag..].as_chunks_mut::<{ UNROLL * CHARS }>();

            for outs in steps {
                let (Some(windows), Some(outs)) = (
                    src[at - $lead..].first_chunk::<WINDOWS>(),
                    outs.as_chunks_mut::<CHARS>().0.first_chunk_mut::<UNROLL>(),
                ) else {
                    break;
                };

                $encode(windows, CHARS, outs.each_mut());
                at += STEP;
            }

            while at < bytes {
                let start = at.min(bytes - BYTES);
                let window = start.checked_sub($lead).and_then(|lead| src[lead..].first_chunk::<$window>());

                match (window, src[start..].first_chunk::<BYTES>(), dst[start / 3 * 4..].first_chunk_mut::<CHARS>()) {
                    (Some(window), _, Some(chars)) => $encode(window, CHARS, [chars]),
                    (None, Some(group), Some(chars)) => $encode(group, CHARS, [chars]),
                    _ => return at,
                }

                at = start + BYTES;
            }

            bytes
        }

        /// `Loops::encode_lines` with `N` less a block's characters bytes of
        /// line ending, written by plain stores, and `SPAN` bytes a period: in
        /// periods when there are lines enough, else line by line.
        $(#[$attr])*
        #[inline(never)]
        fn encode_lines<const N: usize, const SPAN: usize>(
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
                for start in (0..LEN).step_by(BYTES).map(|at| at.min(LEN - BYTES)) {
                    let (Some(group), Some(chars)) = (
                        line[start..].first_chunk::<BYTES>(),
                        out[start / 3 * 4..].first_chunk_mut::<CHARS>(),
                    ) else {
                        return 0;
                    };

                    $encode(group, CHARS, [chars]);
                }

                crate::scalar::write_eol(&mut out[LINE..], eol);
            }

            lines
        }

        /// `encode_lines` over the first `lines` lines of `src`, in whole
        /// periods reading windows where they fit, a block across a line break
        /// split around it, then line by line over the lines left.
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
                    let (to, split) = (at + at / LINE * eol.len(), LINE - at % LINE);
                    let from = BLOCK * BYTES;
                    let window = from.checked_sub($lead).and_then(|lead| bytes[lead..].first_chunk::<$window>());

                    let (Some(chars), Some(group)) = (
                        out[to..].first_chunk_mut::<N>(),
                        bytes[from..].first_chunk::<BYTES>(),
                    ) else {
                        return 0;
                    };

                    match window {
                        Some(window) => $encode(window, split, [chars]),
                        None => $encode(group, split, [chars]),
                    }
                });
            }

            let done = periods.len() * LINES;
            done + encode_lines::<N, SPAN>(&src[done * LEN..lines * LEN], &mut dst[done * span..])
        }
    };
}

#[cfg(test)]
mod tests {
    use alloc::{vec, vec::Vec};

    use crate::{
        codec::{Base64Codec, Base64Eol},
        cpu::available,
        decode::Base64Decode,
        driver::uninit,
        encode::Base64Encode,
        scalar::{self, DecodeState, EncodeState},
    };

    const WRAPS: [Option<Base64Eol>; 3] = [None, Some(Base64Eol::Crlf), Some(Base64Eol::Lf)];

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
        let mut out = vec![0; Base64Codec::BASELINE.decoded_len_max(src.len())];
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
    fn reference_encode(src: &[u8], wrap: Option<Base64Eol>) -> Vec<u8> {
        let eol: &'static [u8] = match wrap {
            None => b"",
            Some(Base64Eol::Crlf) => b"\r\n",
            Some(Base64Eol::Lf) => b"\n",
        };

        let codec = Base64Codec {
            wrap,
            ..Base64Codec::BASELINE
        };

        let mut state = EncodeState::new(eol);
        let mut out = vec![0; codec.encoded_len(src.len()).unwrap()];
        let (_, written) = scalar::encode_bytes(&mut state, src, uninit(&mut out));
        let end = scalar::encode_finish(&mut state, uninit(&mut out[written..])).unwrap();
        out.truncate(written + end);
        out
    }

    fn one_shot_decode(src: &[u8], codec: Base64Codec) -> (Vec<u8>, bool) {
        let mut out = vec![0; codec.decoded_len_max(src.len())];
        let output = codec.decode(src, &mut out).unwrap();
        assert_eq!(output.read, src.len());
        out.truncate(output.written);
        (out, output.clean)
    }

    /// The one-shot encoding, checked to write the length it asks.
    fn one_shot_encode(src: &[u8], codec: Base64Codec) -> Vec<u8> {
        let mut out = vec![0; codec.encoded_len(src.len()).unwrap()];
        let written = codec.encode(src, &mut out).unwrap();
        assert_eq!(written, out.len());
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

            for wrap in WRAPS {
                let expected = reference_encode(&input, wrap);
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
                    let codec = Base64Codec { cpu, wrap };
                    assert_eq!(one_shot_encode(&input, codec), expected, "{codec:?} {len}");

                    for variant in &variants {
                        assert_eq!(
                            one_shot_decode(variant, codec),
                            reference_decode(variant),
                            "{codec:?} {len}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn every_byte_at_every_position_matches_the_reference() {
        let run = reference_encode(&Rng(7).bytes(120), None);
        let lines = reference_encode(&Rng(7).bytes(171), Some(Base64Eol::Crlf));

        for line in [run, lines] {
            for byte in 0..=255 {
                for at in 0..line.len() {
                    let mut input = line.clone();
                    input[at] = byte;
                    let expected = reference_decode(&input);

                    for cpu in available() {
                        let codec = Base64Codec { cpu, wrap: None };
                        assert_eq!(
                            one_shot_decode(&input, codec),
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
                        let codec = Base64Codec { cpu, wrap: None };
                        assert_eq!(
                            one_shot_decode(&input, codec),
                            expected,
                            "{cpu:?} {input:?}"
                        );

                        let mut decoder = Base64Decode::new(codec);
                        let mut out = vec![0; codec.decoded_len_max(input.len())];
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

        for wrap in WRAPS {
            let encoded = reference_encode(&input, wrap);
            let mut dirty = encoded.clone();
            dirty.insert(100, b'*');
            dirty.extend_from_slice(b"=Zg");

            for cpu in available() {
                let codec = Base64Codec { cpu, wrap };

                for at in 0..=input.len() {
                    let mut encoder = Base64Encode::new(codec);
                    let mut out = vec![0; encoded.len()];
                    let first = encoder.resume(&input[..at], &mut out);
                    let second = encoder.resume(&input[at..], &mut out[first.written..]);
                    let written = first.written + second.written;
                    let end = encoder.finish(&mut out[written..]).unwrap();
                    assert_eq!(written + end, encoded.len(), "{codec:?} {at}");
                    assert_eq!(out, encoded, "{codec:?} {at}");
                }

                for source in [&encoded, &dirty] {
                    let expected = reference_decode(source);

                    for at in 0..=source.len() {
                        let mut decoder = Base64Decode::new(codec);
                        let mut out = vec![0; codec.decoded_len_max(source.len())];
                        let first = decoder.resume(&source[..at], &mut out);
                        let second = decoder.resume(&source[at..], &mut out[first.written..]);
                        let written = first.written + second.written;
                        let end = decoder.finish(&mut out[written..]).unwrap();
                        out.truncate(written + end.written);
                        assert_eq!((out, end.clean), expected, "{codec:?} {at}");
                    }
                }
            }
        }
    }
}
