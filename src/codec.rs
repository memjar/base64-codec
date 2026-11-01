//! # Codec
//!
//! [`Base64Codec`], the backend and line ending every call runs with; its
//! one-shot methods are in [`encode`](crate::encode) and
//! [`decode`](crate::decode).

use crate::cpu::Base64Cpu;

/// The backend and line ending every call runs with, a stateless value the
/// application builds once.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Base64Codec {
    /// The backend every call runs.
    pub cpu: Base64Cpu,
    /// The ending of 76-character encoded lines, or none for one unbroken
    /// run; decoding ignores it.
    ///
    /// Refs: <https://datatracker.ietf.org/doc/html/rfc2045#section-6.8>
    pub wrap: Option<Base64Eol>,
}

impl Base64Codec {
    /// The build target's backend, unwrapped, needing no detection.
    pub const BASELINE: Self = Self {
        cpu: Base64Cpu::BASELINE,
        wrap: None,
    };

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
    /// CR LF, as mail on the wire.
    Crlf,
    /// LF alone, common in mail stored on disk.
    Lf,
}
