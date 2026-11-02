//! # Codec
//!
//! [`Base64Codec`], the backend, alphabet, padding and line ending every
//! call runs with; its one-shot methods are in [`encode`](crate::encode) and
//! [`decode`](crate::decode).

use simd_cpu::SimdCpu;

use crate::cpu::Base64Cpu;

/// The backend, alphabet, padding and line ending every call runs with, a
/// stateless value built once.
///
/// [`Base64Codec::default()`] runs the backend of the token the application
/// installed, read when the value is built; an explicit token goes in
/// [`cpu`](Self::cpu): `Base64Codec { cpu: Base64Cpu::new(token),
/// ..Base64Codec::default() }`.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Base64Codec {
    /// The backend every call runs.
    pub cpu: Base64Cpu,
    /// The alphabet encoding writes and decoding takes.
    pub alphabet: Base64Alphabet,
    /// Whether encoding ends a partial quantum with `=`; decoding accepts
    /// input with or without padding alike.
    ///
    /// Refs: <https://datatracker.ietf.org/doc/html/rfc4648#section-3.2>
    pub pad: bool,
    /// The ending of 76-character encoded lines, or none for one unbroken
    /// run; decoding ignores it.
    ///
    /// Refs: <https://datatracker.ietf.org/doc/html/rfc2045#section-6.8>
    pub wrap: Option<Base64Eol>,
}

impl Base64Codec {
    /// The build target's backend, whatever the application installed, the
    /// standard alphabet, padded and unwrapped.
    pub const BASELINE: Self = Self {
        cpu: Base64Cpu::new(SimdCpu::BASELINE),
        alphabet: Base64Alphabet::Standard,
        pad: true,
        wrap: None,
    };

    pub(crate) fn eol(self) -> &'static [u8] {
        match self.wrap {
            None => b"",
            Some(Base64Eol::Crlf) => b"\r\n",
            Some(Base64Eol::Lf) => b"\n",
        }
    }

    pub(crate) fn url(self) -> bool {
        self.alphabet == Base64Alphabet::Url
    }
}

impl Default for Base64Codec {
    /// [`BASELINE`](Self::BASELINE) on the backend of the token the
    /// application installed.
    #[inline]
    fn default() -> Self {
        Self {
            cpu: Base64Cpu::default(),
            ..Self::BASELINE
        }
    }
}

/// The characters of the sextets 62 and 63, the others being `A` to `Z`,
/// `a` to `z` and `0` to `9`.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum Base64Alphabet {
    /// `+` and `/`, as mail needs.
    ///
    /// Refs: <https://datatracker.ietf.org/doc/html/rfc4648#section-4>
    #[default]
    Standard,
    /// `-` and `_`, safe in URLs and file names, as OAuth and JWT need.
    ///
    /// Refs: <https://datatracker.ietf.org/doc/html/rfc4648#section-5>
    Url,
}

/// The line ending of wrapped output.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Base64Eol {
    /// CR LF, as mail on the wire.
    Crlf,
    /// LF alone, common in mail stored on disk.
    Lf,
}
