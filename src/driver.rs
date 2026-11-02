//! # Driver
//!
//! The loops of every backend, written once as `base64_driver!` and stamped
//! into each backend under its target features so that its kernels inline,
//! called through [`Loops`]. Runs end with a block overlapping the one
//! before; lines of 76 characters go a period at a time, the lines whose
//! characters make 19 blocks.

use core::mem::MaybeUninit;

/// A backend loop, unsafe on a CPU without the backend's features.
pub(crate) type Run = unsafe fn(&[u8], &mut [MaybeUninit<u8>]) -> usize;

/// The loops of a backend over the start of `src` and what fits in `dst`,
/// the line ones for LF then CRLF.
pub(crate) struct Loops {
    /// Decodes whole quanta of alphabet characters, returning their length.
    pub(crate) decode_run: Run,
    /// Decodes whole well-formed lines, returning how many.
    pub(crate) decode_lines: [Run; 2],
    /// Encodes whole groups, returning their length.
    pub(crate) encode_run: Run,
    /// Encodes whole lines, returning how many.
    pub(crate) encode_lines: [Run; 2],
}

pub(crate) fn uninit(dst: &mut [u8]) -> &mut [MaybeUninit<u8>] {
    // SAFETY: same layout, and only initialised bytes are written through it.
    unsafe { &mut *(dst as *mut [u8] as *mut [MaybeUninit<u8>]) }
}

/// A mask whose bytes from `split` on are set, `split` at most 32.
pub(crate) fn stitch(split: usize) -> &'static [u8] {
    static MASKS: [[u8; 32]; 2] = [[0; 32], [0xff; 32]];
    &MASKS.as_flattened()[32 - split..]
}

/// Writes a line ending at `split` as long as the room past `chars`, moving
/// the characters after it, when `split` is not past `chars`.
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

/// Expands `$body` once per block of a period, `$block` a constant from 0
/// to 18: as a loop, LLVM rolls the blocks back up.
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

/// Stamps a backend's `LOOPS` from its `Bad` and kernels: `decode` takes a
/// block from the first `split` characters and the last ones, skipping a
/// line break; `encode` takes `K` groups, alone or `lead` bytes into a
/// `window`, and writes a line ending at `split` when given room.
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
            narrow: $encode_narrow:path
            $(, periods: $encode_periods:ident)? $(,)?
        ) $(,)?
    ) => {
        /// The loops of the standard alphabet, then of the URL one.
        pub(crate) const LOOPS: [crate::driver::Loops; 2] = [loops::<false>(), loops::<true>()];

        const fn loops<const URL: bool>() -> crate::driver::Loops {
            crate::driver::Loops {
                decode_run: decode_run::<URL>,
                decode_lines: [
                    decode_lines::<URL, { $chars + 1 }, { $chars / 4 * 77 }>,
                    decode_lines::<URL, { $chars + 2 }, { $chars / 4 * 78 }>,
                ],
                encode_run: encode_run::<URL>,
                encode_lines: [
                    encode_lines::<URL, { $bytes / 3 * 4 + 1 }, { $bytes / 3 * 77 }>,
                    encode_lines::<URL, { $bytes / 3 * 4 + 2 }, { $bytes / 3 * 78 }>,
                ],
            }
        }

        $(#[$attr])*
        #[inline(never)]
        pub(crate) fn decode_run<const URL: bool>(src: &[u8], dst: &mut [core::mem::MaybeUninit<u8>]) -> usize {
            let chars = (src.len() / 4).min(dst.len() / 3) * 4;

            if chars < $chars {
                return $decode_narrow(src, dst, URL);
            }

            decode_blocks::<URL>(&src[..chars], dst)
        }

        /// `decode_run` from a block on: four-block groups, then single blocks.
        $(#[$attr])*
        #[inline(never)]
        fn decode_blocks<const URL: bool>(src: &[u8], dst: &mut [core::mem::MaybeUninit<u8>]) -> usize {
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

                    let found = $decode(chars, WIDE, bytes, URL);

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

                let bad = $decode(block, WIDE, bytes, URL);

                if bad.any() {
                    return start + bad.first();
                }

                at = start + WIDE;
            }

            chars
        }

        /// `Loops::decode_lines` with `C - $chars` bytes of line ending and
        /// `SPAN` bytes a period.
        $(#[$attr])*
        #[inline(never)]
        fn decode_lines<const URL: bool, const C: usize, const SPAN: usize>(
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
                return decode_periods::<URL, C, SPAN>(src, dst, lines);
            }

            for (line, chars) in src.chunks_exact(span).take(lines).enumerate() {
                if chars[LINE..] != *eol {
                    return line;
                }

                let Some(out) = dst.get_mut(line * LEN..line * LEN + LEN + $spare) else {
                    return line + (decode_blocks::<URL>(&chars[..LINE], &mut dst[line * LEN..]) == LINE) as usize;
                };

                let mut bad = Bad::none();

                for start in (0..LINE).step_by(WIDE).map(|at| at.min(LINE - WIDE)) {
                    let (Some(block), Some(bytes)) = (
                        chars[start..].first_chunk::<WIDE>(),
                        out[start / 4 * 3..].first_chunk_mut::<{ WIDE / 4 * 3 + $spare }>(),
                    ) else {
                        return line;
                    };

                    let found = $decode(block, WIDE, bytes, URL);

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

        /// `decode_lines` over the first `lines` lines, whole periods first.
        $(#[$attr])*
        #[inline(never)]
        fn decode_periods<const URL: bool, const C: usize, const SPAN: usize>(
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

                    let found = $decode(window, split, bytes, URL);

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
            done + decode_lines::<URL, C, SPAN>(&src[done * span..(done + rest) * span], &mut dst[done * LEN..])
        }

        $(#[$attr])*
        #[inline(never)]
        pub(crate) fn encode_run<const URL: bool>(src: &[u8], dst: &mut [core::mem::MaybeUninit<u8>]) -> usize {
            let bytes = (src.len() / 3).min(dst.len() / 4) * 3;

            if bytes < $bytes {
                return $encode_narrow(src, dst, URL);
            }

            encode_blocks::<URL>(&src[..bytes], dst)
        }

        /// `encode_run` from a block on: steps of four blocks, from the first
        /// cache line for 64-character blocks, then single ones.
        $(#[$attr])*
        #[inline(never)]
        fn encode_blocks<const URL: bool>(src: &[u8], dst: &mut [core::mem::MaybeUninit<u8>]) -> usize {
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

            $encode(group, CHARS, [chars], URL);

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

                $encode(windows, CHARS, outs.each_mut(), URL);
                at += STEP;
            }

            while at < bytes {
                let start = at.min(bytes - BYTES);
                let window = start.checked_sub($lead).and_then(|lead| src[lead..].first_chunk::<$window>());

                match (window, src[start..].first_chunk::<BYTES>(), dst[start / 3 * 4..].first_chunk_mut::<CHARS>()) {
                    (Some(window), _, Some(chars)) => $encode(window, CHARS, [chars], URL),
                    (None, Some(group), Some(chars)) => $encode(group, CHARS, [chars], URL),
                    _ => return at,
                }

                at = start + BYTES;
            }

            bytes
        }

        /// `Loops::encode_lines` with `N - CHARS` bytes of line ending and
        /// `SPAN` bytes a period.
        $(#[$attr])*
        #[inline(never)]
        fn encode_lines<const URL: bool, const N: usize, const SPAN: usize>(
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
                $(use self::$encode_periods as encode_periods;)?
                return encode_periods::<URL, N, SPAN>(src, dst, lines);
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

                    $encode(group, CHARS, [chars], URL);
                }

                crate::scalar::write_eol(&mut out[LINE..], eol);
            }

            lines
        }

        /// `encode_lines` over the first `lines` lines, whole periods first.
        $(#[$attr])*
        #[inline(never)]
        fn encode_periods<const URL: bool, const N: usize, const SPAN: usize>(
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
                        Some(window) => $encode(window, split, [chars], URL),
                        None => $encode(group, split, [chars], URL),
                    }
                });
            }

            let done = periods.len() * LINES;
            done + encode_lines::<URL, N, SPAN>(&src[done * LEN..lines * LEN], &mut dst[done * span..])
        }
    };
}

#[cfg(test)]
mod tests {
    use alloc::{vec, vec::Vec};

    use crate::{
        codec::{Base64Alphabet, Base64Codec, Base64Eol},
        cpu::available,
        decode::Base64Decode,
        driver::uninit,
        encode::Base64Encode,
        scalar::{self, DecodeState, EncodeState},
    };

    const WRAPS: [Option<Base64Eol>; 3] = [None, Some(Base64Eol::Crlf), Some(Base64Eol::Lf)];

    /// Every alphabet and padding, with the scalar backend.
    const STYLES: [Base64Codec; 4] = {
        let standard = Base64Codec::BASELINE;
        let url = Base64Codec {
            alphabet: Base64Alphabet::Url,
            ..standard
        };

        [
            standard,
            Base64Codec {
                pad: false,
                ..standard
            },
            url,
            Base64Codec { pad: false, ..url },
        ]
    };

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

    fn reference_decode(src: &[u8], codec: Base64Codec) -> (Vec<u8>, bool) {
        let mut state = DecodeState::new(codec.url());
        let mut out = vec![0; codec.decoded_len_max(src.len())];
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

    fn reference_encode(src: &[u8], codec: Base64Codec) -> Vec<u8> {
        let mut state = EncodeState::new(codec);
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

    fn one_shot_encode(src: &[u8], codec: Base64Codec) -> Vec<u8> {
        let mut out = vec![0; codec.encoded_len(src.len()).unwrap()];
        let written = codec.encode(src, &mut out).unwrap();
        assert_eq!(written, out.len());
        out
    }

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
                for style in STYLES {
                    let reference = Base64Codec { wrap, ..style };
                    let expected = reference_encode(&input, reference);
                    assert_eq!(
                        reference_decode(&expected, reference),
                        (input.clone(), true)
                    );

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
                        let codec = Base64Codec { cpu, ..reference };
                        assert_eq!(one_shot_encode(&input, codec), expected, "{codec:?} {len}");

                        for variant in &variants {
                            assert_eq!(
                                one_shot_decode(variant, codec),
                                reference_decode(variant, reference),
                                "{codec:?} {len}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn every_backend_matches_the_reference_around_whole_cycles() {
        let mut rng = Rng(11);

        // Around one and two cycles of 32 lines ending with CRLF and of 64
        // ending with LF, whose output fills whole vectors.
        for len in [1824, 3648, 7296]
            .into_iter()
            .flat_map(|end| end - 64..end + 64)
        {
            let input = rng.bytes(len);

            for wrap in WRAPS {
                for style in STYLES {
                    let reference = Base64Codec { wrap, ..style };
                    let expected = reference_encode(&input, reference);

                    for cpu in available() {
                        let codec = Base64Codec { cpu, ..reference };
                        assert_eq!(one_shot_encode(&input, codec), expected, "{codec:?} {len}");

                        let mut encoder = Base64Encode::new(codec);
                        let mut out = vec![0; expected.len()];
                        let first = encoder.resume(&input[..len / 3], &mut out);
                        let second = encoder.resume(&input[len / 3..], &mut out[first.written..]);
                        let written = first.written + second.written;
                        encoder.finish(&mut out[written..]).unwrap();
                        assert_eq!(out, expected, "{codec:?} {len}");
                    }
                }
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn every_byte_at_every_position_matches_the_reference() {
        for style in [STYLES[0], STYLES[2]] {
            let crlf = Base64Codec {
                wrap: Some(Base64Eol::Crlf),
                ..style
            };

            let run = reference_encode(&Rng(7).bytes(120), style);
            let lines = reference_encode(&Rng(7).bytes(171), crlf);

            for line in [run, lines] {
                for byte in 0..=255 {
                    for at in 0..line.len() {
                        let mut input = line.clone();
                        input[at] = byte;
                        let expected = reference_decode(&input, style);

                        for cpu in available() {
                            let codec = Base64Codec { cpu, ..style };
                            assert_eq!(
                                one_shot_decode(&input, codec),
                                expected,
                                "{codec:?} {byte} {at}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn every_short_end_matches_the_reference() {
        for (style, bytes) in [(STYLES[0], b"Qz=\r\n *"), (STYLES[2], b"Q_=\r\n +")] {
            for len in 0..=6_u32 {
                for mut code in 0..bytes.len().pow(len) {
                    let end: Vec<u8> = (0..len)
                        .map(|_| {
                            let byte = bytes[code % bytes.len()];
                            code /= bytes.len();
                            byte
                        })
                        .collect();

                    for prefix in [&b""[..], b"QUJD", b"QUJD\r\n"] {
                        let input = [prefix, &end].concat();
                        let expected = reference_decode(&input, style);

                        for cpu in available() {
                            let codec = Base64Codec { cpu, ..style };
                            assert_eq!(
                                one_shot_decode(&input, codec),
                                expected,
                                "{codec:?} {input:?}"
                            );

                            let mut decoder = Base64Decode::new(codec);
                            let mut out = vec![0; codec.decoded_len_max(input.len())];
                            let first = decoder.resume(prefix, &mut out);
                            let second = decoder.resume(&end, &mut out[first.written..]);
                            let written = first.written + second.written;
                            let last = decoder.finish(&mut out[written..]).unwrap();
                            out.truncate(written + last.written);
                            assert_eq!((out, last.clean), expected, "{codec:?} {input:?} resumed");
                        }
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
            for style in STYLES {
                let reference = Base64Codec { wrap, ..style };
                let encoded = reference_encode(&input, reference);
                let mut dirty = encoded.clone();
                dirty.insert(100, b'*');
                dirty.extend_from_slice(b"=Zg");

                for cpu in available() {
                    let codec = Base64Codec { cpu, ..reference };

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
                        let expected = reference_decode(source, reference);

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
}
