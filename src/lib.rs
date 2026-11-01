//! # base64-codec
//!
//! Base64 encoding and decoding with the standard alphabet (RFC 4648 4),
//! lenient as mail needs it, with SIMD, `no_std`, no dependency and no I/O.
//!
//! A [`Base64Codec`], from [`codec`], holds what every call runs: the backend
//! a [`Base64Cpu`] token from [`cpu`] names, and the line ending of encoded
//! output. Its one-shot methods encode a whole input in a pass of their own,
//! with no carry to keep, and decode it in whole lines and one run when it
//! ends as usual. [`Base64Encode`] and [`Base64Decode`], from [`encode`] and
//! [`decode`], take a codec, then the caller's input and output buffers
//! chunk by chunk, as any I/O-free coroutine does. The library never detects
//! CPU features itself: the application does it once, builds its codec and
//! passes it down.
//!
//! The scalar steps are the reference, and the only code that handles
//! anything but whole quanta of alphabet characters: padding, skipped and
//! reported bytes, partial quanta and the carry between two calls. Every
//! backend supplies block kernels, table lookups for scalar and vectors for
//! SSSE3, AVX2 and AVX-512 on x86 and NEON on aarch64, which one driver,
//! written once and stamped into each backend under its target features,
//! runs over runs of whole quanta and over lines of 76 characters, a period
//! of lines at a time, its blocks crossing the line breaks. Every backend
//! writes the same bytes and reports the same flag.
//!
//! The SIMD kernels follow encodify 1.0.2 and Wojciech Muła and Daniel
//! Lemire's methods, the driver's structure simdutf8 0.1.5, as
//! ATTRIBUTION.md details.
//!
//! ```
//! use base64_codec::{
//!     codec::{Base64Codec, Base64Eol},
//!     cpu::Base64Cpu,
//! };
//!
//! // NOTE: with the std feature, Base64Cpu::detect() at application start.
//! let codec = Base64Codec {
//!     cpu: Base64Cpu::BASELINE,
//!     wrap: Some(Base64Eol::Crlf),
//! };
//!
//! let mut encoded = [0; 6];
//! let len = codec.encode(b"hi!", &mut encoded).unwrap();
//! assert_eq!(&encoded[..len], b"aGkh\r\n");
//!
//! let mut decoded = [0; 6];
//! let output = codec.decode(b"aGkh\r\n", &mut decoded).unwrap();
//! assert_eq!(&decoded[..output.written], b"hi!");
//! assert!(output.clean);
//! ```
//!
//! [`Base64Codec`]: codec::Base64Codec
//! [`Base64Encode`]: encode::Base64Encode
//! [`Base64Decode`]: decode::Base64Decode
//! [`Base64Cpu`]: cpu::Base64Cpu

#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![deny(unsafe_op_in_unsafe_fn, clippy::undocumented_unsafe_blocks)]

#[cfg(any(feature = "alloc", test))]
extern crate alloc;
#[cfg(any(feature = "std", test))]
extern crate std;

#[macro_use]
mod driver;

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
mod avx2;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
mod avx512;
pub mod codec;
pub mod cpu;
pub mod decode;
pub mod encode;
#[cfg(all(target_arch = "aarch64", target_feature = "neon"))]
mod neon;
mod scalar;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
mod ssse3;
