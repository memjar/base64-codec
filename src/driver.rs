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
//! - The encode kernel reads its input: a window of the length the invocation
//!   names, its group starting at the lead, so that it can load bytes around
//!   it, or the group alone, for the blocks no window fits around.
//!
//! A run or a line ends with a block ending at its end and overlapping the one
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
    /// Decodes whole lines of the given width, ended by CRLF or by LF alone,
    /// while they are well formed and fit in `dst`, returning how many.
    pub(crate) decode_lines: unsafe fn(&[u8], &mut [MaybeUninit<u8>], usize, bool) -> usize,
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

                    let found = $decode(chars, bytes);

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
                    src[start..].first_chunk(),
                    dst[start / 4 * 3..].first_chunk_mut::<BYTES>(),
                ) else {
                    return at;
                };

                let bad = $decode(block, bytes);

                if bad.any() {
                    return start + bad.first();
                }

                at = start + WIDE;
            }

            chars
        }

        /// Decodes whole lines of `width` characters, ended by CRLF or by LF
        /// when `crlf` is false, while they are well formed and `dst` has
        /// room for the spare bytes past them, returning how many.
        $(#[$attr])*
        #[inline(never)]
        fn decode_lines(
            src: &[u8],
            dst: &mut [core::mem::MaybeUninit<u8>],
            width: usize,
            crlf: bool,
        ) -> usize {
            match width {
                crate::scalar::LINE => decode_lines_of::<{ crate::scalar::LINE }>(src, dst, width, crlf),
                $chars.. => decode_lines_of::<0>(src, dst, width, crlf),
                _ => 0,
            }
        }

        /// `decode_lines` in blocks writing spare bytes, the last one ending
        /// at the line's end, tested once per line unless the backend tests
        /// each block apart.
        ///
        /// NOTE: `WIDTH` is the width when it is known at compile time, 0
        /// otherwise, so that the common width compiles with its blocks
        /// unrolled and their bounds checked away.
        $(#[$attr])*
        #[inline]
        fn decode_lines_of<const WIDTH: usize>(
            src: &[u8],
            dst: &mut [core::mem::MaybeUninit<u8>],
            width: usize,
            crlf: bool,
        ) -> usize {
            const WIDE: usize = $chars;

            let width = if WIDTH == 0 { width } else { WIDTH };
            let len = width / 4 * 3;
            let mut done = 0;

            for line in src.chunks_exact(width + 1 + crlf as usize) {
                let (chars, end) = line.split_at(width);

                let Some(out) = dst.get_mut(done * len..done * len + len + $spare) else {
                    break;
                };

                if end.last() != Some(&b'\n') || crlf && end[0] != b'\r' {
                    break;
                }

                let mut bad = Bad::none();
                let mut at = 0;

                while at < width {
                    let start = at.min(width - WIDE);

                    let (Some(block), Some(bytes)) = (
                        chars[start..].first_chunk(),
                        out[start / 4 * 3..].first_chunk_mut::<{ WIDE / 4 * 3 + $spare }>(),
                    ) else {
                        return done;
                    };

                    let found = $decode(block, bytes);

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

            let (Some(group), Some(chars)) = (src.first_chunk::<BYTES>(), dst.first_chunk_mut()) else {
                return 0;
            };

            $encode(group, chars);
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

                    $encode(window, chars);
                }

                at += STEP;
            }

            while at < bytes {
                let start = at.min(bytes - BYTES);
                let window = start.checked_sub($lead).and_then(|lead| src[lead..].first_chunk::<$window>());

                match (window, src[start..].first_chunk::<BYTES>(), dst[start / 3 * 4..].first_chunk_mut()) {
                    (Some(window), _, Some(chars)) => $encode(window, chars),
                    (None, Some(group), Some(chars)) => $encode(group, chars),
                    _ => return at,
                }

                at = start + BYTES;
            }

            bytes
        }

        /// Encodes the whole lines of `src` that fit in `dst`, each ended by
        /// `eol`, returning how many.
        $(#[$attr])*
        #[inline(never)]
        fn encode_lines(
            src: &[u8],
            dst: &mut [core::mem::MaybeUninit<u8>],
            eol: &[u8],
        ) -> usize {
            match eol.len() {
                2 => encode_lines_of::<2>(src, dst),
                _ => encode_lines_of::<1>(src, dst),
            }
        }

        /// `encode_lines` in blocks, the last one ending at the line's end,
        /// each line ended by CRLF when `EOL` is 2 and by LF when it is 1.
        ///
        /// NOTE: the ending's length is a constant, so that it is written
        /// with plain stores and not a copy call per line.
        $(#[$attr])*
        #[inline]
        fn encode_lines_of<const EOL: usize>(
            src: &[u8],
            dst: &mut [core::mem::MaybeUninit<u8>],
        ) -> usize {
            const BYTES: usize = $bytes;
            const LINE: usize = crate::scalar::LINE;
            const LINE_BYTES: usize = crate::scalar::LINE_BYTES;

            let (lines, _) = src.as_chunks::<LINE_BYTES>();
            let mut done = 0;

            for (line, out) in lines.iter().zip(dst.chunks_exact_mut(LINE + EOL)) {
                let mut at = 0;

                while at < LINE_BYTES {
                    let start = at.min(LINE_BYTES - BYTES);

                    let (Some(group), Some(chars)) = (
                        line[start..].first_chunk::<BYTES>(),
                        out[start / 3 * 4..].first_chunk_mut(),
                    ) else {
                        return done;
                    };

                    $encode(group, chars);
                    at = start + BYTES;
                }

                if EOL == 2 {
                    out[LINE].write(b'\r');
                }

                out[LINE + EOL - 1].write(b'\n');
                done += 1;
            }

            done
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
        let line = reference_encode(&Rng(7).bytes(120), OPTIONS[0]);

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
