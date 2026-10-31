---
cairn: delta
change: a-simd-base64-codec
---

## ADDED Requirements

### Requirement: Standard padded encoding

The encoder SHALL write the RFC 4648 4 standard alphabet, padded with `=` to a whole quantum, and nothing for an empty input.

#### Scenario: The RFC 4648 vectors
- GIVEN `f`, `fo`, `foo` and `foobar`
- WHEN each is encoded unwrapped
- THEN the output is `Zg==`, `Zm8=`, `Zm9v` and `Zm9vYmFy`

### Requirement: MIME line wrapping

With a line ending set, the encoder SHALL write lines of 76 characters, the last one possibly shorter, each ended by that line ending, the last one included (RFC 2045 6.8). Without one, it SHALL write a single unbroken run, as an RFC 2047 B encoded word needs.

#### Scenario: One byte past a full line
- GIVEN 58 bytes `x` and CRLF
- WHEN they are encoded
- THEN the output is 76 characters, CRLF, `eA==`, CRLF: 84 bytes, which `encoded_len` also returns

### Requirement: Lengths and short buffers

`encoded_len` SHALL return exactly what `encode` writes, or none when that overflows `usize`. `decoded_len_max` SHALL bound what `decode` writes for any input of that length. `encode` and `decode` SHALL return none and write nothing when `dst` is shorter than their length helper asks; `resume` SHALL consume only what fits in `dst`. Decoding MAY overwrite the bytes of `dst` past the ones it reports written, within the length it was given.

### Requirement: Lenient decoding

The decoder SHALL skip CR, LF, SP and HTAB silently, skip every other byte outside the alphabet and report it, accept missing padding and non-zero trailing bits, decode symbols after padding as a new run and report it, and drop a lone trailing symbol and report it. For every input it SHALL return the same bytes and the same clean flag as mime-rs's src/codec/base64.rs as of 2026-10-31, and it SHALL never fail or panic.

#### Scenario: A stray byte
- GIVEN `AAEC*AwQF`
- WHEN it is decoded
- THEN the bytes are 00 01 02 03 04 05 and the input is reported unclean

#### Scenario: Wrapped lines
- GIVEN `AAEC` CRLF `AwQF` CRLF
- WHEN it is decoded
- THEN the bytes are 00 01 02 03 04 05 and the input is clean

#### Scenario: Missing padding and trailing bits
- GIVEN `aGk` and `aGl=`
- WHEN each is decoded
- THEN both give `hi` and are clean

#### Scenario: Data after padding
- GIVEN `aGk=aGk=`
- WHEN it is decoded
- THEN the bytes are `hihi` and the input is reported unclean

#### Scenario: A lone trailing symbol
- GIVEN `aGkxY`
- WHEN it is decoded
- THEN the bytes are `hi1` and the input is reported unclean

#### Scenario: A form feed
- GIVEN `aGk=` followed by FF
- WHEN it is decoded
- THEN the bytes are `hi` and the input is reported unclean

### Requirement: Resumable without I/O

`Base64Encode` and `Base64Decode` SHALL give, for any split of the input into chunks and any output space offered, the same bytes and the same clean flag as the one-shot call, `finish` ending the input; an empty chunk SHALL change nothing. `resume` SHALL always progress with three bytes of room when decoding and six when encoding, and `finish` SHALL write nothing and return none when its output does not fit. The crate SHALL perform no I/O, and SHALL allocate only in the `alloc` helpers.

#### Scenario: A decode split mid-quantum and after padding
- GIVEN `aGk=aGk=` resumed as `aG`, `k=aG`, `k=`, then finished
- WHEN the outputs are joined
- THEN the bytes are `hihi` and the input is reported unclean

### Requirement: CPU detection is the caller's

No encoding or decoding function SHALL detect CPU features. A backend SHALL run only from a `Base64Cpu` token: `BASELINE`, the best backend the build target guarantees through `cfg(target_feature)`; `assume`, unsafe; or `detect`, behind the `std` feature.

#### Scenario: An aarch64 build without std
- GIVEN the crate built for aarch64 with no feature
- WHEN `decode` runs with `Base64Cpu::BASELINE`
- THEN it runs the NEON backend, and no detection code is compiled

#### Scenario: A default x86_64 build without std
- GIVEN the crate built for x86_64 with no feature and no `-C target-feature`
- WHEN `decode` runs with `Base64Cpu::BASELINE`
- THEN it runs the scalar backend

### Requirement: Every backend matches scalar

For every input and options, every backend SHALL write the same bytes and return the same clean flag as the scalar backend, in one-shot and resumed calls alike.

#### Scenario: Every length on an AVX2 machine
- GIVEN pseudo-random inputs of every length from 0 to 1,024, encoded unwrapped, with CRLF and with LF, and the same encodings with a stray byte inserted
- WHEN they are encoded and decoded under SSSE3, AVX2 and scalar
- THEN the three backends write identical bytes and flags

### Requirement: No dependency, no std

The library SHALL build `#![no_std]` with no dependency under every feature combination, `std` serving only `Base64Cpu::detect` and `alloc` only `encode_vec` and `decode_vec`.
