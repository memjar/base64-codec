# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-11-03

### Added

- Added `codec::Base64Codec`, the `Copy` value every call runs from.

  It holds the backend (`cpu`), the alphabet (`alphabet`), whether encoding pads (`pad`) and the line ending of encoded output (`wrap`). `BASELINE` needs no CPU detection.

- Added `codec::Base64Alphabet`: the standard alphabet and the URL one of RFC 4648 section 5.
- Added `codec::Base64Eol`, ending encoded lines of 76 characters with CRLF or LF.
- Added `Base64Codec::encode` and `Base64Codec::decode`, the one-shot calls.
- Added `Base64Codec::encode_vec` and `Base64Codec::decode_vec`, behind the `alloc` feature.
- Added `Base64Codec::encoded_len` and `Base64Codec::decoded_len_max`, the output lengths to allocate.
- Added `encode::Base64Encode`, the I/O-free encoder over the caller's buffers, the input cut anywhere.
- Added `decode::Base64Decode`, the I/O-free decoder over the caller's buffers, the input cut anywhere.

  Decoding never fails: line breaks, blanks and missing padding are accepted, other stray bytes skipped and reported through `clean`.

- Added `cpu::Base64Cpu`, the widest backend a `simd_cpu::SimdCpu` token has the features of.

  `Base64Cpu::default()` and `Base64Codec::default()` run the token the application installed with `SimdCpu::install_detected`, the build target's baseline when none is.

- Added the scalar, SSSE3, AVX2, AVX-512 VBMI and NEON backends, all writing the same bytes.

[Unreleased]: https://github.com/pimalaya/base64-codec/compare/v0.1.0..HEAD
[0.1.0]: https://github.com/pimalaya/base64-codec/compare/root..v0.1.0
