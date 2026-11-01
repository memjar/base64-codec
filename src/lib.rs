//! # base64-codec
//!
//! Base64 with the standard alphabet (RFC 4648 4), decoded leniently as mail
//! needs it, with SIMD, `no_std`, no dependency and no I/O.
//!
//! A [`Base64Codec`] holds a [`Base64Cpu`] backend token, which the
//! application gets once, usually from `Base64Cpu::detect`, and the line
//! ending of encoded output. Its methods take a whole input, [`Base64Encode`]
//! and [`Base64Decode`] chunk by chunk.
//!
//! The scalar steps alone handle what is not whole quanta: padding, stray
//! bytes, partial quanta, the carry between calls. Every backend has block
//! kernels that one driver, stamped into it, runs over runs and lines, all
//! writing the same bytes; ATTRIBUTION.md credits the methods.
//!
//! ```
//! use base64_codec::{
//!     codec::{Base64Codec, Base64Eol},
//!     cpu::Base64Cpu,
//! };
//!
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
