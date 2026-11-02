# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Added `codec::Base64Codec`, the `Copy` value every call runs from: the backend (`cpu`), the alphabet (`alphabet`), whether encoding pads (`pad`) and the line ending of encoded output (`wrap`, unwrapped or lines of 76 characters ended by CRLF or LF), with `BASELINE` needing no detection.

- Added `codec::Base64Alphabet`: the standard alphabet, or the URL one of RFC 4648 5 (`-` and `_`) for OAuth and JWT tokens, on every backend. Decoding takes the codec's alphabet alone, the other one's two symbols being stray bytes.

- Added unpadded encoding, without `pad`; decoding accepts input with or without padding alike.

- Added the one-shot `Base64Codec::encode` and `Base64Codec::decode`, and `encode_vec` and `decode_vec` behind the `alloc` feature.

- Added `Base64Codec::encoded_len` and `Base64Codec::decoded_len_max`, the output lengths to allocate.

- Added `encode::Base64Encode` and `decode::Base64Decode`, the I/O-free encoder and decoder built from a codec and resumed over the caller's buffers, the input cut anywhere.

- Added lenient decoding with the rules of mime-rs's base64 wrapper: line breaks and blanks skipped, other stray bytes skipped and reported, missing padding accepted, never failing.

- Added `cpu::Base64Cpu`, the backend token a codec holds, the widest backend a `simd_cpu::SimdCpu` has the features of. `Base64Cpu::default()` and `Base64Codec::default()` take the token the application installed with `SimdCpu::install_detected`, the baseline when none is.

- Added the scalar, SSSE3, AVX2, AVX-512 VBMI and NEON backends, all writing the same bytes.
