//! # Encoding
//!
//! Base64 in either alphabet, padded or not, unwrapped as RFC 2047 B words
//! and URL tokens need, or in lines of 76 characters each ended by CRLF or
//! LF, the last one included, as MIME bodies need: chunk by chunk with
//! [`Base64Encode`], at once with [`Base64Codec::encode`]. Decoding is the
//! sibling [`decode`](crate::decode).

use core::mem::MaybeUninit;

#[cfg(feature = "alloc")]
use alloc::vec::Vec;

use crate::{
    codec::Base64Codec,
    cpu::Base64Cpu,
    driver,
    scalar::{self, EncodeState, LINE, LINE_BYTES},
};

/// I/O-free encoder over the caller's buffers, the input cut anywhere.
///
/// ```
/// use base64_codec::{codec::Base64Codec, encode::Base64Encode};
///
/// let mut encoder = Base64Encode::new(Base64Codec::BASELINE);
/// let mut out = [0; 16];
///
/// let first = encoder.resume(b"hel", &mut out);
/// let second = encoder.resume(b"lo", &mut out[first.written..]);
/// let written = first.written + second.written;
/// let end = encoder.finish(&mut out[written..]).unwrap();
///
/// assert_eq!(&out[..written + end], b"aGVsbG8=");
/// ```
///
/// Refs: <https://datatracker.ietf.org/doc/html/rfc4648#section-4>
#[derive(Clone, Debug)]
pub struct Base64Encode {
    cpu: Base64Cpu,
    state: EncodeState,
}

impl Base64Encode {
    /// An encoder with the backend, alphabet, padding and line ending of
    /// `codec`.
    pub fn new(codec: Base64Codec) -> Self {
        Self {
            cpu: codec.cpu,
            state: EncodeState::new(codec),
        }
    }

    /// Encodes as much of `src` as fits in `dst`, always progressing with six
    /// bytes of room.
    pub fn resume(&mut self, src: &[u8], dst: &mut [u8]) -> Base64EncodeOutput {
        let (read, written) = self.run(src, driver::uninit(dst));
        Base64EncodeOutput { read, written }
    }

    /// Ends the input with the last group and line ending, at most
    /// six bytes, or returns none, writing nothing, when `dst` is too short.
    pub fn finish(&mut self, dst: &mut [u8]) -> Option<usize> {
        scalar::encode_finish(&mut self.state, driver::uninit(dst))
    }

    fn run(&mut self, src: &[u8], dst: &mut [MaybeUninit<u8>]) -> (usize, usize) {
        let state = &mut self.state;
        let lead = state.lead().min(src.len());
        let (mut read, mut written) = scalar::encode_bytes(state, &src[..lead], dst);

        if read == lead && state.carried == 0 && state.column == 0 {
            let loops = self.cpu.loops(state.url);

            if state.eol.is_empty() {
                // SAFETY: the token names a backend the CPU supports.
                let bytes = unsafe { (loops.encode_run)(&src[read..], &mut dst[written..]) };
                read += bytes;
                written += bytes / 3 * 4;
            } else {
                // SAFETY: as above.
                let lines = unsafe {
                    (loops.encode_lines[state.eol.len() - 1])(&src[read..], &mut dst[written..])
                };

                read += lines * LINE_BYTES;
                written += lines * (LINE + state.eol.len());
            }
        }

        let (more_read, more_written) =
            scalar::encode_bytes(state, &src[read..], &mut dst[written..]);

        (read + more_read, written + more_written)
    }
}

/// What an encoding call did.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Base64EncodeOutput {
    /// The input bytes consumed.
    pub read: usize,
    /// The output bytes written.
    pub written: usize,
}

/// One-shot encoding.
impl Base64Codec {
    /// Encodes all of `src` into `dst`, or returns none, writing nothing, when
    /// it is shorter than [`encoded_len`](Self::encoded_len).
    #[inline]
    pub fn encode(self, src: &[u8], dst: &mut [u8]) -> Option<usize> {
        let len = self.encoded_len(src.len())?;
        let dst = dst.get_mut(..len)?;
        Some(self.encode_all(src, driver::uninit(dst)))
    }

    /// Appends the encoding of `src` to `out`.
    #[cfg(feature = "alloc")]
    #[cfg_attr(docsrs, doc(cfg(feature = "alloc")))]
    pub fn encode_vec(self, src: &[u8], out: &mut Vec<u8>) {
        let start = out.len();
        out.reserve(self.encoded_len(src.len()).unwrap_or(usize::MAX));

        let written = self.encode_all(src, out.spare_capacity_mut());

        // SAFETY: the encoder initialised the first `written` bytes.
        unsafe { out.set_len(start + written) };
    }

    /// The bytes [`encode`](Self::encode) writes for an input of `len` bytes,
    /// or none when that overflows `usize`.
    pub fn encoded_len(self, len: usize) -> Option<usize> {
        let groups = len.div_ceil(3);

        let chars = match self.pad {
            true => groups.checked_mul(4)?,
            false => len.checked_add(groups)?,
        };

        let lines = chars.div_ceil(LINE);
        chars.checked_add(lines.checked_mul(self.eol().len())?)
    }

    /// `encode` into a `dst` of `encoded_len` bytes or more.
    fn encode_all(self, src: &[u8], dst: &mut [MaybeUninit<u8>]) -> usize {
        let loops = self.cpu.loops(self.url());
        let eol = self.eol();

        if eol.is_empty() {
            // SAFETY: the token names a backend the CPU supports.
            let bytes = unsafe { (loops.encode_run)(src, dst) };
            let written = bytes / 3 * 4;
            return written
                + scalar::encode_tail(&src[bytes..], &mut dst[written..], self.url(), self.pad);
        }

        // SAFETY: as above.
        let lines = unsafe { (loops.encode_lines[eol.len() - 1])(src, dst) };
        let read = lines * LINE_BYTES;
        let start = lines * (LINE + eol.len());

        // SAFETY: as above.
        let bytes = unsafe { (loops.encode_run)(&src[read..], &mut dst[start..]) };
        let mut written = start + bytes / 3 * 4;
        written += scalar::encode_tail(
            &src[read + bytes..],
            &mut dst[written..],
            self.url(),
            self.pad,
        );

        if written == start {
            return written;
        }

        scalar::write_eol(&mut dst[written..], eol);
        written + eol.len()
    }
}

#[cfg(test)]
mod tests {
    use alloc::{vec, vec::Vec};

    use crate::{
        codec::{Base64Alphabet, Base64Eol},
        cpu::available,
        encode::*,
    };

    const CRLF: Option<Base64Eol> = Some(Base64Eol::Crlf);

    const UNPADDED: Base64Codec = Base64Codec {
        pad: false,
        ..Base64Codec::BASELINE
    };

    fn encoded(input: &[u8], wrap: Option<Base64Eol>) -> Vec<u8> {
        encoded_as(
            input,
            Base64Codec {
                wrap,
                ..Base64Codec::BASELINE
            },
        )
    }

    fn encoded_as(input: &[u8], style: Base64Codec) -> Vec<u8> {
        let results: Vec<Vec<u8>> = available()
            .into_iter()
            .map(|cpu| {
                let codec = Base64Codec { cpu, ..style };
                let mut out = vec![0; codec.encoded_len(input.len()).unwrap()];
                let written = codec.encode(input, &mut out).unwrap();
                assert_eq!(written, out.len());
                out
            })
            .collect();

        assert!(results.windows(2).all(|pair| pair[0] == pair[1]));
        results.into_iter().next().unwrap()
    }

    #[test]
    fn rfc4648_vectors_encode() {
        for (decoded, encoded_chars) in [
            (&b""[..], &b""[..]),
            (b"f", b"Zg=="),
            (b"fo", b"Zm8="),
            (b"foo", b"Zm9v"),
            (b"foob", b"Zm9vYg=="),
            (b"fooba", b"Zm9vYmE="),
            (b"foobar", b"Zm9vYmFy"),
        ] {
            assert_eq!(encoded(decoded, None), encoded_chars);
        }
    }

    #[test]
    fn the_url_alphabet_writes_62_and_63() {
        let url = Base64Codec {
            alphabet: Base64Alphabet::Url,
            ..Base64Codec::BASELINE
        };

        assert_eq!(encoded(&[0xfb, 0xff, 0xbf], None), b"+/+/");
        assert_eq!(encoded_as(&[0xfb, 0xff, 0xbf], url), b"-_-_");
        assert_eq!(
            encoded_as(&[0xfb, 0xff, 0xbf].repeat(40), url),
            b"-_-_".repeat(40)
        );
    }

    #[test]
    fn rfc4648_vectors_encode_unpadded() {
        for (decoded, encoded_chars) in [
            (&b"f"[..], &b"Zg"[..]),
            (b"fo", b"Zm8"),
            (b"foo", b"Zm9v"),
            (b"foob", b"Zm9vYg"),
            (b"fooba", b"Zm9vYmE"),
        ] {
            assert_eq!(encoded_as(decoded, UNPADDED), encoded_chars);
        }
    }

    #[test]
    fn one_byte_past_a_full_line_unpadded() {
        let out = encoded_as(
            &[b'x'; 58],
            Base64Codec {
                wrap: CRLF,
                ..UNPADDED
            },
        );
        assert_eq!(&out[76..], b"\r\neA\r\n");
    }

    #[test]
    fn one_byte_past_a_full_line() {
        let out = encoded(&[b'x'; 58], CRLF);

        assert_eq!(out.len(), 84);
        assert_eq!(&out[76..], b"\r\neA==\r\n");
        assert!(out[..76].iter().all(u8::is_ascii_alphanumeric));
    }

    #[test]
    fn a_full_line_ends_once() {
        let out = encoded(&[0; 57], CRLF);
        assert_eq!(out.len(), 78);
        assert_eq!(&out[76..], b"\r\n");
    }

    #[test]
    fn an_overflowing_length_is_none() {
        let codec = Base64Codec::BASELINE;
        assert_eq!(codec.encoded_len(usize::MAX), None);

        let codec = Base64Codec {
            wrap: CRLF,
            ..codec
        };
        assert_eq!(codec.encoded_len(usize::MAX / 4 * 3), None);
    }

    #[test]
    fn six_bytes_of_room_always_progress() {
        let input: Vec<u8> = (0..200).collect();

        for cpu in available() {
            let mut encoder = Base64Encode::new(Base64Codec {
                cpu,
                wrap: CRLF,
                ..Base64Codec::BASELINE
            });
            let mut src = &input[..];
            let mut out = Vec::new();
            let mut room = [0; 6];

            while !src.is_empty() {
                let output = encoder.resume(src, &mut room);
                assert!(output.read > 0 || output.written > 0);
                out.extend_from_slice(&room[..output.written]);
                src = &src[output.read..];
            }

            let end = encoder.finish(&mut room).unwrap();
            out.extend_from_slice(&room[..end]);
            assert_eq!(out, encoded(&input, CRLF));
        }
    }

    #[test]
    fn a_short_buffer_writes_nothing() {
        let codec = Base64Codec {
            wrap: CRLF,
            ..Base64Codec::BASELINE
        };

        let mut out = [0xaa; 3];
        assert_eq!(codec.encode(b"f", &mut out), None);
        assert_eq!(out, [0xaa; 3]);
    }

    #[test]
    #[cfg(feature = "alloc")]
    fn a_vec_is_appended_to() {
        let input: Vec<u8> = (0..100).collect();

        for cpu in available() {
            let mut out = b"> ".to_vec();
            Base64Codec {
                cpu,
                wrap: CRLF,
                ..Base64Codec::BASELINE
            }
            .encode_vec(&input, &mut out);
            assert_eq!(&out[..2], b"> ");
            assert_eq!(out[2..], encoded(&input, CRLF));
        }
    }
}
