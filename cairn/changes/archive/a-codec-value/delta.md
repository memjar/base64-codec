---
cairn: delta
change: a-codec-value
---

## ADDED Requirements

### Requirement: One codec for every call

Every call SHALL run from a `Base64Codec`, a `Copy` value holding the backend and the line ending of encoded output: its one-shot methods encode or decode a whole input, and `Base64Encode::new` and `Base64Decode::new` take it to encode or decode chunk by chunk. The codec SHALL hold no state between calls, so that no one-shot call can run in the middle of a resumed input.

#### Scenario: One codec for the whole application
- GIVEN a codec built once with CRLF line endings
- WHEN it encodes an input in one call, and a `Base64Encode` built from it encodes the same input chunk by chunk, then finishes
- THEN both write the same bytes

## MODIFIED Requirements

### Requirement: Lengths and short buffers

`Base64Codec::encoded_len` SHALL return exactly what `Base64Codec::encode` writes, or none when that overflows `usize`. `Base64Codec::decoded_len_max` SHALL bound what `Base64Codec::decode` writes for any input of that length. `encode` and `decode` SHALL return none and write nothing when `dst` is shorter than their length helper asks; `resume` SHALL consume only what fits in `dst`. Decoding MAY overwrite the bytes of `dst` past the ones it reports written, within the length it was given.

### Requirement: No dependency, no std

The library SHALL build `#![no_std]` with no dependency under every feature combination, `std` serving only `Base64Cpu::detect` and `alloc` only `Base64Codec::encode_vec` and `Base64Codec::decode_vec`.

### Requirement: CPU detection is the caller's

No encoding or decoding function SHALL detect CPU features. A backend SHALL run only from a `Base64Cpu` token: `BASELINE`, the best backend the build target guarantees through `cfg(target_feature)`; `assume`, unsafe; or `detect`, behind the `std` feature.

#### Scenario: An aarch64 build without std
- GIVEN the crate built for aarch64 with no feature
- WHEN `Base64Codec::BASELINE` decodes an input
- THEN it runs the NEON backend, and no detection code is compiled

#### Scenario: A default x86_64 build without std
- GIVEN the crate built for x86_64 with no feature and no `-C target-feature`
- WHEN `Base64Codec::BASELINE` decodes an input
- THEN it runs the scalar backend

### Requirement: Every backend matches scalar

For every input and line ending, every backend SHALL write the same bytes and return the same clean flag as the scalar steps, in one-shot and resumed calls alike.

#### Scenario: Every length on an AVX-512 machine
- GIVEN pseudo-random inputs of every length from 0 to 1,024, encoded unwrapped, with CRLF and with LF, and the same encodings rewrapped or with a stray byte inserted
- WHEN they are encoded and decoded under AVX-512, AVX2, SSSE3 and scalar
- THEN the four backends write identical bytes and flags

#### Scenario: Every short end
- GIVEN every input of up to 6 bytes drawn from `Q`, `z`, `=`, CR, LF, SP and `*`, alone and after `QUJD` or `QUJD` CRLF
- WHEN it is decoded in one call, and resumed with the join between the two parts, under every backend the CPU supports
- THEN every backend writes the bytes and flag of the scalar steps

### Requirement: MIME line wrapping

With a line ending set, the encoder SHALL write lines of 76 characters, the last one possibly shorter, each ended by that line ending, the last one included (RFC 2045 6.8). Without one, it SHALL write a single unbroken run, as an RFC 2047 B encoded word needs.

#### Scenario: One byte past a full line
- GIVEN 58 bytes `x` and a codec with CRLF
- WHEN they are encoded
- THEN the output is 76 characters, CRLF, `eA==`, CRLF: 84 bytes, which `Base64Codec::encoded_len` also returns
