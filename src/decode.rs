//! # Decoding
//!
//! Lenient base64 decoding, as mail needs it: line breaks and blanks are
//! skipped, every other byte outside the alphabet is skipped and reported,
//! missing padding and non-zero trailing bits are accepted, symbols after
//! padding are decoded as a new run and reported, and a lone trailing symbol
//! is dropped and reported. Decoding never fails: it returns the bytes, and
//! whether the input was clean.
//!
//! [`Base64Decode`] takes the input chunk by chunk; the one-shot
//! [`Base64Codec::decode`] and `Base64Codec::decode_vec` decode a whole
//! input. Encoding is the sibling module [`encode`](crate::encode).

use core::mem::MaybeUninit;

#[cfg(feature = "alloc")]
use alloc::vec::Vec;

use crate::{
    codec::Base64Codec,
    cpu::Base64Cpu,
    driver,
    scalar::{self, DecodeState, LINE, LINE_BYTES},
};

/// I/O-free base64 decoder, resumed with the caller's input and output
/// buffers.
///
/// The input can be cut anywhere, mid-quantum included: the decoder carries
/// at most three symbols from one call to the next.
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
/// Refs: <https://datatracker.ietf.org/doc/html/rfc4648#section-4>
#[derive(Clone, Debug)]
pub struct Base64Decode {
    cpu: Base64Cpu,
    state: DecodeState,
}

impl Base64Decode {
    /// Builds a decoder running the backend of `codec`, whatever its line
    /// ending: decoding takes any.
    pub fn new(codec: Base64Codec) -> Self {
        Self {
            cpu: codec.cpu,
            state: DecodeState::new(),
        }
    }

    /// Decodes as much of `src` as fits in `dst`.
    ///
    /// The caller drains `dst` and resumes with the rest of `src` or the
    /// next chunk; three bytes of room always let the decoder progress. The
    /// bytes of `dst` past the ones written may be overwritten.
    pub fn resume(&mut self, src: &[u8], dst: &mut [u8]) -> Base64DecodeOutput {
        let (read, written) = self.run(src, driver::uninit(dst));

        Base64DecodeOutput {
            read,
            written,
            clean: self.state.clean,
        }
    }

    /// Ends the input: writes the last partial quantum, at most two bytes,
    /// and drops and reports a lone trailing symbol.
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

    /// Decodes as much of `src` as fits in `dst`, returning the bytes read
    /// and written: whole lines and runs of whole quanta, the line breaks
    /// between them, and anything else byte by byte until the runs can take
    /// over.
    fn run(&mut self, src: &[u8], dst: &mut [MaybeUninit<u8>]) -> (usize, usize) {
        let state = &mut self.state;
        let (end, padded) = scalar::trimmed(src);
        let mut read = 0;
        let mut written = 0;

        loop {
            if state.pending == 0 && !state.padded {
                let end = end.max(read);
                let (more_read, more_written) =
                    lines_and_run(self.cpu, &src[read..end], &mut dst[written..]);
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
    /// Decodes the whole of `src` into `dst`.
    ///
    /// Returns none, writing nothing, when `dst` is shorter than
    /// [`decoded_len_max`](Self::decoded_len_max) asks. Within that length,
    /// the bytes of `dst` past the ones written may be overwritten.
    #[inline]
    pub fn decode(self, src: &[u8], dst: &mut [u8]) -> Option<Base64DecodeOutput> {
        let dst = dst.get_mut(..self.decoded_len_max(src.len()))?;
        Some(self.decode_all(src, driver::uninit(dst)))
    }

    /// Decodes the whole of `src` at the end of `out`, returning whether it
    /// was well-formed.
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

    /// The most bytes [`decode`](Self::decode) writes for an input of `len`
    /// bytes, whatever they hold.
    pub const fn decoded_len_max(self, len: usize) -> usize {
        len - len.div_ceil(4)
    }

    /// [`decode`](Self::decode) into a `dst` sized by
    /// [`decoded_len_max`](Self::decoded_len_max): whole lines and one run
    /// when the input ends as usual, anything else by a decoder from where
    /// they stopped.
    fn decode_all(self, src: &[u8], dst: &mut [MaybeUninit<u8>]) -> Base64DecodeOutput {
        let (end, _) = scalar::trimmed(src);
        let (read, mut written) = lines_and_run(self.cpu, &src[..end], dst);
        let mut state = DecodeState::new();

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

/// Decodes whole lines when `src` starts with a line of `LINE` characters,
/// then one run, returning the bytes read and written.
#[inline(always)]
fn lines_and_run(cpu: Base64Cpu, src: &[u8], dst: &mut [MaybeUninit<u8>]) -> (usize, usize) {
    let loops = cpu.loops();

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

    use crate::{cpu::available, decode::*};

    /// The bytes and flag every backend decodes `input` to, checked equal.
    fn decoded(input: &[u8]) -> (Vec<u8>, bool) {
        let results: Vec<(Vec<u8>, bool)> = available()
            .into_iter()
            .map(|cpu| {
                let codec = Base64Codec { cpu, wrap: None };
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
            let mut decoder = Base64Decode::new(Base64Codec { cpu, wrap: None });
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
            let mut decoder = Base64Decode::new(Base64Codec { cpu, wrap: None });
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
            let codec = Base64Codec { cpu, wrap: None };
            let mut out = b"> ".to_vec();
            assert!(!codec.decode_vec(input, &mut out));
            assert_eq!(out, b"> foobarfoobarfoobarfoobarfoobarfoobarfoo");
        }
    }
}
