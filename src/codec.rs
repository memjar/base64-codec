//! # Codec
//!
//! [`Base64Codec`], the configuration every call runs: the backend and the
//! line ending of encoded output. Its one-shot methods, in
//! [`encode`](crate::encode) and [`decode`](crate::decode), encode and decode
//! a whole input; the coroutines of those modules take it to work chunk by
//! chunk.

use crate::cpu::Base64Cpu;

/// The backend every call runs and the line ending of encoded output.
///
/// A plain value holding no state: an application builds one, usually at
/// start from `Base64Cpu::detect`, and uses it for every call, its one-shot
/// methods as the encoders and decoders built from it.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Base64Codec {
    /// The backend every call runs.
    pub cpu: Base64Cpu,
    /// The line ending of 76-character lines of encoded output, or none for
    /// a single unbroken run. Decoding takes either.
    ///
    /// Refs: <https://datatracker.ietf.org/doc/html/rfc2045#section-6.8>
    pub wrap: Option<Base64Eol>,
}

impl Base64Codec {
    /// The backend the build target guarantees, unwrapped: a codec needing
    /// no detection, usable in a `static`.
    pub const BASELINE: Self = Self {
        cpu: Base64Cpu::BASELINE,
        wrap: None,
    };

    /// The line ending of encoded output, empty when unwrapped.
    pub(crate) fn eol(self) -> &'static [u8] {
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
