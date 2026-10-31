//! # Encoding
//!
//! Base64 encoding with padding, unwrapped (as an RFC 2047 B encoded word
//! needs) or in lines of 76 characters, each ended by CRLF or LF, the last
//! one included (as a MIME body needs).
//!
//! [`Base64Encode`] takes the input chunk by chunk; [`encode`] and
//! `encode_vec` encode a whole input in one pass, with no carry to keep.
//! Decoding is the sibling module [`decode`](crate::decode).

use core::mem::MaybeUninit;

#[cfg(feature = "alloc")]
use alloc::vec::Vec;

use crate::{
    cpu::Base64Cpu,
    driver,
    scalar::{self, EncodeState, LINE, LINE_BYTES},
};

/// I/O-free base64 encoder, resumed with the caller's input and output
/// buffers.
///
/// The input can be cut anywhere: the encoder carries at most two bytes from
/// one call to the next.
///
/// ```
/// use base64_codec::{
///     cpu::Base64Cpu,
///     encode::{Base64Encode, Base64EncodeOptions, Base64Eol},
/// };
///
/// let opts = Base64EncodeOptions {
///     wrap: Some(Base64Eol::Crlf),
/// };
///
/// let mut encoder = Base64Encode::new(Base64Cpu::BASELINE, opts);
/// let mut out = [0; 16];
///
/// let first = encoder.resume(b"hel", &mut out);
/// let second = encoder.resume(b"lo", &mut out[first.written..]);
/// let written = first.written + second.written;
/// let end = encoder.finish(&mut out[written..]).unwrap();
///
/// assert_eq!(&out[..written + end], b"aGVsbG8=\r\n");
/// ```
///
/// Refs: <https://datatracker.ietf.org/doc/html/rfc4648#section-4>
#[derive(Clone, Debug)]
pub struct Base64Encode {
    cpu: Base64Cpu,
    state: EncodeState,
}

impl Base64Encode {
    /// Builds an encoder running the backend `cpu` names.
    pub fn new(cpu: Base64Cpu, opts: Base64EncodeOptions) -> Self {
        Self {
            cpu,
            state: EncodeState::new(opts.eol()),
        }
    }

    /// Encodes as much of `src` as fits in `dst`.
    ///
    /// The caller drains `dst` and resumes with the rest of `src` or the
    /// next chunk; six bytes of room always let the encoder progress.
    pub fn resume(&mut self, src: &[u8], dst: &mut [u8]) -> Base64EncodeOutput {
        let (read, written) = self.run(src, driver::uninit(dst));
        Base64EncodeOutput { read, written }
    }

    /// Ends the input: writes the last group, padded, and the last line
    /// ending when wrapped, at most six bytes.
    ///
    /// Returns the bytes written, or none, writing nothing, when `dst` is
    /// too short.
    pub fn finish(&mut self, dst: &mut [u8]) -> Option<usize> {
        scalar::encode_finish(&mut self.state, driver::uninit(dst))
    }

    /// Encodes as much of `src` as fits in `dst`, returning the bytes read
    /// and written: group by group up to the next line, or the next group
    /// when unwrapped, then whole lines or a whole run, then group by group
    /// again, carrying what is left.
    fn run(&mut self, src: &[u8], dst: &mut [MaybeUninit<u8>]) -> (usize, usize) {
        let state = &mut self.state;
        let lead = state.lead().min(src.len());
        let (mut read, mut written) = scalar::encode_bytes(state, &src[..lead], dst);

        if read == lead && state.carried == 0 && state.column == 0 {
            let loops = self.cpu.loops();

            if state.eol.is_empty() {
                // SAFETY: the token names a backend the CPU supports.
                let bytes = unsafe { (loops.encode_run)(&src[read..], &mut dst[written..]) };
                read += bytes;
                written += bytes / 3 * 4;
            } else {
                // SAFETY: as above.
                let lines =
                    unsafe { (loops.encode_lines)(&src[read..], &mut dst[written..], state.eol) };

                read += lines * LINE_BYTES;
                written += lines * (LINE + state.eol.len());
            }
        }

        let (more_read, more_written) =
            scalar::encode_bytes(state, &src[read..], &mut dst[written..]);

        (read + more_read, written + more_written)
    }
}

/// Options of [`Base64Encode`] and [`encode`].
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Base64EncodeOptions {
    /// The line ending of 76-character lines, or none for a single
    /// unbroken run. Defaults to none.
    ///
    /// Refs: <https://datatracker.ietf.org/doc/html/rfc2045#section-6.8>
    pub wrap: Option<Base64Eol>,
}

impl Base64EncodeOptions {
    fn eol(self) -> &'static [u8] {
        match self.wrap {
            None => b"",
            Some(Base64Eol::Crlf) => b"\r\n",
            Some(Base64Eol::Lf) => b"\n",
        }
    }
}

/// The line ending of wrapped output.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Base64Eol {
    /// CR LF, as the wire format of mail has it.
    Crlf,
    /// LF alone, as mail stored on disk often has it.
    Lf,
}

/// What an encoding call did.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Base64EncodeOutput {
    /// The input bytes consumed.
    pub read: usize,
    /// The output bytes written.
    pub written: usize,
}

/// Encodes the whole of `src` into `dst`, returning the bytes written.
///
/// Returns none, writing nothing, when `dst` is shorter than
/// [`encoded_len`] asks.
pub fn encode(
    src: &[u8],
    dst: &mut [u8],
    cpu: Base64Cpu,
    opts: Base64EncodeOptions,
) -> Option<usize> {
    let len = encoded_len(src.len(), opts)?;
    let dst = dst.get_mut(..len)?;
    Some(encode_all(src, driver::uninit(dst), cpu, opts))
}

/// Encodes the whole of `src` at the end of `out`.
#[cfg(feature = "alloc")]
#[cfg_attr(docsrs, doc(cfg(feature = "alloc")))]
pub fn encode_vec(src: &[u8], out: &mut Vec<u8>, cpu: Base64Cpu, opts: Base64EncodeOptions) {
    let start = out.len();
    out.reserve(encoded_len(src.len(), opts).unwrap_or(usize::MAX));

    let written = encode_all(src, out.spare_capacity_mut(), cpu, opts);

    // SAFETY: the encoder wrote `written` initialised bytes at the start of
    // the spare capacity.
    unsafe { out.set_len(start + written) };
}

/// The bytes [`encode`] writes for an input of `len` bytes, or none when
/// that overflows `usize`.
pub fn encoded_len(len: usize, opts: Base64EncodeOptions) -> Option<usize> {
    let chars = len.div_ceil(3).checked_mul(4)?;
    let lines = chars.div_ceil(LINE);
    chars.checked_add(lines.checked_mul(opts.eol().len())?)
}

/// [`encode`] into a `dst` sized by [`encoded_len`], in one pass with no
/// [`Base64Encode`] state: whole lines when wrapped, the whole groups left,
/// the padded tail and the last line ending.
fn encode_all(
    src: &[u8],
    dst: &mut [MaybeUninit<u8>],
    cpu: Base64Cpu,
    opts: Base64EncodeOptions,
) -> usize {
    let loops = cpu.loops();
    let eol = opts.eol();
    let mut read = 0;
    let mut written = 0;

    if !eol.is_empty() {
        // SAFETY: the token names a backend the CPU supports.
        let lines = unsafe { (loops.encode_lines)(src, dst, eol) };
        read = lines * LINE_BYTES;
        written = lines * (LINE + eol.len());
    }

    let start = written;

    // SAFETY: as above.
    let bytes = unsafe { (loops.encode_run)(&src[read..], &mut dst[written..]) };
    written += bytes / 3 * 4;
    written += scalar::encode_tail(&src[read + bytes..], &mut dst[written..]);

    if written == start {
        return written;
    }

    scalar::write_eol(&mut dst[written..], eol);
    written + eol.len()
}

#[cfg(test)]
mod tests {
    use alloc::{vec, vec::Vec};

    use crate::{cpu::available, encode::*};

    const CRLF: Base64EncodeOptions = Base64EncodeOptions {
        wrap: Some(Base64Eol::Crlf),
    };

    /// What every backend encodes `input` to, checked equal.
    fn encoded(input: &[u8], opts: Base64EncodeOptions) -> Vec<u8> {
        let results: Vec<Vec<u8>> = available()
            .into_iter()
            .map(|cpu| {
                let mut out = vec![0; encoded_len(input.len(), opts).unwrap()];
                let written = encode(input, &mut out, cpu, opts).unwrap();
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
            assert_eq!(
                encoded(decoded, Base64EncodeOptions::default()),
                encoded_chars
            );
        }
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
        assert_eq!(
            encoded_len(usize::MAX, Base64EncodeOptions::default()),
            None
        );
        assert_eq!(encoded_len(usize::MAX / 4 * 3, CRLF), None);
    }

    #[test]
    fn six_bytes_of_room_always_progress() {
        let input: Vec<u8> = (0..200).collect();

        for cpu in available() {
            let mut encoder = Base64Encode::new(cpu, CRLF);
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
        let mut out = [0xaa; 3];
        assert_eq!(encode(b"f", &mut out, Base64Cpu::BASELINE, CRLF), None);
        assert_eq!(out, [0xaa; 3]);
    }

    #[test]
    #[cfg(feature = "alloc")]
    fn a_vec_is_appended_to() {
        let input: Vec<u8> = (0..100).collect();

        for cpu in available() {
            let mut out = b"> ".to_vec();
            encode_vec(&input, &mut out, cpu, CRLF);
            assert_eq!(&out[..2], b"> ");
            assert_eq!(out[2..], encoded(&input, CRLF));
        }
    }
}
