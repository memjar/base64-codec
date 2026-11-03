//! # Decoding
//!
//! Lenient decoding that never fails, as mail needs it, chunk by chunk with
//! [`Base64Decode`] or at once with [`Base64Codec::decode`]. Encoding is the
//! sibling [`encode`](crate::encode).
//!
//! Blanks, line breaks, missing padding and trailing bits are clean; a stray
//! byte (skipped), data after padding (decoded) and a lone last symbol
//! (dropped) are not.

use core::mem::MaybeUninit;

#[cfg(feature = "alloc")]
use alloc::vec::Vec;

use crate::{
    codec::Base64Codec,
    cpu::Base64Cpu,
    driver,
    scalar::{self, DecodeState, LINE, LINE_BYTES},
};

/// I/O-free decoder over the caller's buffers, the input cut anywhere.
///
/// ```
/// use base64_codec::{codec::Base64Codec, decode::Base64Decode};
///
/// let mut decoder = Base64Decode::new(Base64Codec::BASELINE);
/// let mut out = [0; 16];
///
/// let first = decoder.resume(b"aGVs", &mut out);
/// let second = decoder.resume(b"bG8=\r\n", &mut out[first.written..]);
/// let written = first.written + second.written;
/// let end = decoder.finish(&mut out[written..]).unwrap();
///
/// assert_eq!(&out[..written + end.written], b"hello");
/// assert!(end.clean);
/// ```
///
/// Refs: <https://datatracker.ietf.org/doc/html/rfc4648#section-4>,
/// <https://datatracker.ietf.org/doc/html/rfc4648#section-5>
#[derive(Clone, Debug)]
pub struct Base64Decode {
    cpu: Base64Cpu,
    state: DecodeState,
}

impl Base64Decode {
    /// A decoder with the backend and alphabet of `codec`.
    pub fn new(codec: Base64Codec) -> Self {
        Self {
            cpu: codec.cpu,
            state: DecodeState::new(codec.url()),
        }
    }

    /// Decodes as much of `src` as fits in `dst`.
    ///
    /// It always progresses with three bytes of room. Bytes of `dst` past the
    /// written ones may change.
    pub fn resume(&mut self, src: &[u8], dst: &mut [u8]) -> Base64DecodeOutput {
        let (read, written) = self.run(src, driver::uninit(dst));

        Base64DecodeOutput {
            read,
            written,
            clean: self.state.clean,
        }
    }

    /// Ends the input with the last partial quantum, at most two bytes.
    ///
    /// Returns none, writing nothing, when `dst` is too short.
    pub fn finish(&mut self, dst: &mut [u8]) -> Option<Base64DecodeOutput> {
        let written = scalar::decode_finish(&mut self.state, driver::uninit(dst))?;

        Some(Base64DecodeOutput {
            read: 0,
            written,
            clean: self.state.clean,
        })
    }

    fn run(&mut self, src: &[u8], dst: &mut [MaybeUninit<u8>]) -> (usize, usize) {
        let state = &mut self.state;
        let (end, padded) = scalar::trimmed(src);
        let mut read = 0;
        let mut written = 0;

        loop {
            if state.pending == 0 && !state.padded {
                let end = end.max(read);
                let (more_read, more_written) =
                    lines_and_run(self.cpu, state.url, &src[read..end], &mut dst[written..]);
                read += more_read;
                written += more_written;

                if end - read < 4 && scalar::decode_tail(state, &src[read..end], padded) {
                    return (src.len(), written);
                }

                if let eol @ 1.. = scalar::eol(&src[read..]) {
                    read += eol;
                    continue;
                }
            }

            let (more_read, more_written, done) =
                scalar::decode_bytes(state, &src[read..], &mut dst[written..]);

            read += more_read;
            written += more_written;

            if done {
                return (read, written);
            }
        }
    }
}

/// What a decoding call did.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Base64DecodeOutput {
    /// The input bytes consumed.
    pub read: usize,
    /// The output bytes written.
    pub written: usize,
    /// Whether everything read so far was well-formed.
    pub clean: bool,
}

/// One-shot decoding.
impl Base64Codec {
    /// Decodes all of `src` into `dst`.
    ///
    /// Any of the first [`decoded_len_max`](Self::decoded_len_max) bytes of
    /// `dst` may change. Returns none, writing nothing, when `dst` is shorter.
    #[inline]
    pub fn decode(self, src: &[u8], dst: &mut [u8]) -> Option<Base64DecodeOutput> {
        let dst = dst.get_mut(..self.decoded_len_max(src.len()))?;
        Some(self.decode_all(src, driver::uninit(dst)))
    }

    /// Appends the decoding of `src` to `out`, returning whether it was clean.
    #[cfg(feature = "alloc")]
    #[cfg_attr(docsrs, doc(cfg(feature = "alloc")))]
    pub fn decode_vec(self, src: &[u8], out: &mut Vec<u8>) -> bool {
        let start = out.len();
        out.reserve(self.decoded_len_max(src.len()));

        let decoded = self.decode_all(src, out.spare_capacity_mut());

        // SAFETY: the decoder initialised the first `decoded.written` bytes.
        unsafe { out.set_len(start + decoded.written) };

        decoded.clean
    }

    /// The most bytes [`decode`](Self::decode) writes for `len` input bytes.
    pub const fn decoded_len_max(self, len: usize) -> usize {
        len - len.div_ceil(4)
    }

    /// `decode` into a `dst` of `decoded_len_max` bytes or more.
    fn decode_all(self, src: &[u8], dst: &mut [MaybeUninit<u8>]) -> Base64DecodeOutput {
        let (end, _) = scalar::trimmed(src);
        let (read, mut written) = lines_and_run(self.cpu, self.url(), &src[..end], dst);
        let mut state = DecodeState::new(self.url());

        if end - read < 4 && scalar::decode_tail(&mut state, &src[read..end], false) {
            let tail = scalar::decode_finish(&mut state, &mut dst[written..]).unwrap_or(0);

            return Base64DecodeOutput {
                read: src.len(),
                written: written + tail,
                clean: state.clean,
            };
        }

        let mut decoder = Base64Decode {
            cpu: self.cpu,
            state,
        };

        let (more_read, more_written) = decoder.run(&src[read..], &mut dst[written..]);
        written += more_written;
        let end = scalar::decode_finish(&mut decoder.state, &mut dst[written..]).unwrap_or(0);

        Base64DecodeOutput {
            read: read + more_read,
            written: written + end,
            clean: decoder.state.clean,
        }
    }
}

/// Decodes whole lines when `src` starts with one, then one run.
#[inline(always)]
fn lines_and_run(
    cpu: Base64Cpu,
    url: bool,
    src: &[u8],
    dst: &mut [MaybeUninit<u8>],
) -> (usize, usize) {
    let loops = cpu.loops(url);

    if let Some(b'\r' | b'\n') = src.get(LINE)
        && let eol @ 1.. = scalar::eol(&src[LINE..])
    {
        // SAFETY: the token names a backend the CPU supports.
        let lines = unsafe { (loops.decode_lines[eol - 1])(src, dst) };
        let (read, written) = (lines * (LINE + eol), lines * LINE_BYTES);
        // SAFETY: as above.
        let chars = unsafe { (loops.decode_run)(&src[read..], &mut dst[written..]) };
        return (read + chars, written + chars / 4 * 3);
    }

    // SAFETY: as above.
    let chars = unsafe { (loops.decode_run)(src, dst) };
    (chars, chars / 4 * 3)
}

#[cfg(test)]
mod tests {
    use alloc::{vec, vec::Vec};

    use crate::{codec::Base64Alphabet, cpu::available, decode::*};

    const URL: Base64Codec = Base64Codec {
        alphabet: Base64Alphabet::Url,
        ..Base64Codec::BASELINE
    };

    fn decoded(input: &[u8]) -> (Vec<u8>, bool) {
        decoded_as(input, Base64Codec::BASELINE)
    }

    fn decoded_as(input: &[u8], style: Base64Codec) -> (Vec<u8>, bool) {
        let results: Vec<(Vec<u8>, bool)> = available()
            .into_iter()
            .map(|cpu| {
                let codec = Base64Codec { cpu, ..style };
                let mut out = vec![0; codec.decoded_len_max(input.len())];
                let output = codec.decode(input, &mut out).unwrap();
                out.truncate(output.written);
                (out, output.clean)
            })
            .collect();

        assert!(results.windows(2).all(|pair| pair[0] == pair[1]));
        results.into_iter().next().unwrap()
    }

    #[test]
    fn rfc4648_vectors_decode() {
        for (encoded, decoded_bytes) in [
            (&b""[..], &b""[..]),
            (b"Zg==", b"f"),
            (b"Zm8=", b"fo"),
            (b"Zm9v", b"foo"),
            (b"Zm9vYg==", b"foob"),
            (b"Zm9vYmE=", b"fooba"),
            (b"Zm9vYmFy", b"foobar"),
        ] {
            assert_eq!(decoded(encoded), (decoded_bytes.to_vec(), true));
        }
    }

    #[test]
    fn the_url_alphabet_takes_its_own_symbols_only() {
        let bytes = vec![0xfb, 0xff, 0xbf];
        assert_eq!(decoded_as(b"-_-_", URL), (bytes.clone(), true));
        assert_eq!(decoded(b"+/+/"), (bytes, true));
        assert_eq!(decoded_as(b"+/+/", URL), (vec![], false));
        assert_eq!(decoded(b"-_-_"), (vec![], false));
    }

    #[test]
    fn a_standard_symbol_is_stray_under_the_url_alphabet() {
        let (bytes, clean) = decoded_as(b"aGkaGk", URL);
        assert!(clean);
        assert_eq!(decoded_as(b"aGk+aGk", URL), (bytes, false));
    }

    #[test]
    fn a_stray_byte_is_skipped_and_reported() {
        assert_eq!(decoded(b"AAEC*AwQF"), (vec![0, 1, 2, 3, 4, 5], false));
    }

    #[test]
    fn wrapped_lines_are_clean() {
        assert_eq!(decoded(b"AAEC\r\nAwQF\r\n"), (vec![0, 1, 2, 3, 4, 5], true));
        assert_eq!(decoded(b"aG k=\t\n"), (b"hi".to_vec(), true));
    }

    #[test]
    fn missing_padding_and_trailing_bits_are_clean() {
        assert_eq!(decoded(b"aGk"), (b"hi".to_vec(), true));
        assert_eq!(decoded(b"aGl="), (b"hi".to_vec(), true));
        assert_eq!(decoded(b"aG\nk=\n"), (b"hi".to_vec(), true));
        assert_eq!(decoded(b"aGk=="), (b"hi".to_vec(), true));
    }

    #[test]
    fn data_after_padding_is_a_second_run() {
        assert_eq!(decoded(b"aGk=aGk="), (b"hihi".to_vec(), false));
        assert_eq!(decoded(b"aGkxY=aGk="), (b"hi1hi".to_vec(), false));
        assert_eq!(decoded(b"=aGk"), (b"hi".to_vec(), false));
    }

    #[test]
    fn a_lone_trailing_symbol_is_dropped_and_reported() {
        assert_eq!(decoded(b"aGkxY"), (b"hi1".to_vec(), false));
    }

    #[test]
    fn a_form_feed_is_reported() {
        assert_eq!(decoded(b"aGk=\x0c"), (b"hi".to_vec(), false));
    }

    #[test]
    fn a_resumed_decode_joins_its_chunks() {
        for cpu in available() {
            let mut decoder = Base64Decode::new(Base64Codec {
                cpu,
                ..Base64Codec::BASELINE
            });
            let mut out = [0; 8];
            let mut written = 0;

            for chunk in [&b"aG"[..], b"k=aG", b"k="] {
                let output = decoder.resume(chunk, &mut out[written..]);
                assert_eq!(output.read, chunk.len());
                written += output.written;
            }

            let end = decoder.finish(&mut out[written..]).unwrap();
            assert_eq!(&out[..written + end.written], b"hihi");
            assert!(!end.clean);
        }
    }

    #[test]
    fn three_bytes_of_room_always_progress() {
        let input = b"Zm9vYmFy\r\nZm9vYmFy\r\nZm9vYmE=\r\n";

        for cpu in available() {
            let mut decoder = Base64Decode::new(Base64Codec {
                cpu,
                ..Base64Codec::BASELINE
            });
            let mut src = &input[..];
            let mut out = Vec::new();
            let mut room = [0; 3];

            while !src.is_empty() {
                let output = decoder.resume(src, &mut room);
                assert!(output.read > 0 || output.written > 0);
                out.extend_from_slice(&room[..output.written]);
                src = &src[output.read..];
            }

            let end = decoder.finish(&mut room).unwrap();
            out.extend_from_slice(&room[..end.written]);
            assert_eq!(out, b"foobarfoobarfooba");
        }
    }

    #[test]
    fn a_short_buffer_writes_nothing() {
        let codec = Base64Codec::BASELINE;
        let mut out = [0xaa; 2];
        assert_eq!(codec.decode(b"Zm9v", &mut out), None);
        assert_eq!(out, [0xaa; 2]);

        let mut decoder = Base64Decode::new(codec);
        decoder.resume(b"Zm9", &mut []);
        assert_eq!(decoder.finish(&mut out[..1]), None);
        assert_eq!(decoder.finish(&mut out).map(|end| end.written), Some(2));
    }

    #[test]
    #[cfg(feature = "alloc")]
    fn a_vec_is_appended_to() {
        let input = b"Zm9vYmFyZm9vYmFyZm9vYmFyZm9vYmFyZm9vYmFyZm9vYmFy\r\nZm9v*\r\n";

        for cpu in available() {
            let codec = Base64Codec {
                cpu,
                ..Base64Codec::BASELINE
            };
            let mut out = b"> ".to_vec();
            assert!(!codec.decode_vec(input, &mut out));
            assert_eq!(out, b"> foobarfoobarfoobarfoobarfoobarfoobarfoo");
        }
    }
}
