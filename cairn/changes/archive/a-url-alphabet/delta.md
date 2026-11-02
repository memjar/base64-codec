---
cairn: delta
change: a-url-alphabet
---

## ADDED Requirements

### Requirement: The URL alphabet

With `Base64Alphabet::Url`, the encoder SHALL write `-` and `_` for the sextets 62 and 63 (RFC 4648 5).

#### Scenario: The last two sextets
- GIVEN the bytes FB FF BF
- WHEN they are encoded with the standard and the URL alphabet
- THEN the outputs are `+/+/` and `-_-_`, and each decodes back clean with its own alphabet

### Requirement: The codec's alphabet alone

The decoder SHALL take the symbols of the codec's alphabet alone: under `Base64Alphabet::Url`, `-` and `_` as the sextets 62 and 63 and `+` and `/` as bytes outside the alphabet, and the reverse under `Base64Alphabet::Standard`, every other lenient rule unchanged.

#### Scenario: A standard symbol under the URL alphabet
- GIVEN `aGk+aGk` and a codec with the URL alphabet
- WHEN it is decoded
- THEN `+` is skipped, the bytes are those of `aGkaGk` and the input is reported unclean

### Requirement: Unpadded encoding

Without `pad`, the encoder SHALL end a last partial quantum after its 2 or 3 characters, with no `=`, and `Base64Codec::encoded_len` SHALL count that; decoding SHALL not depend on `pad`.

#### Scenario: The RFC 4648 vectors unpadded
- GIVEN `f`, `fo` and `foo`
- WHEN each is encoded without padding
- THEN the output is `Zg`, `Zm8` and `Zm9v`

## MODIFIED Requirements

### Requirement: Lenient decoding

The decoder SHALL skip CR, LF, SP and HTAB silently, skip every other byte outside the alphabet and report it, accept missing padding and non-zero trailing bits, decode symbols after padding as a new run and report it, and drop a lone trailing symbol and report it. For every input under the standard alphabet it SHALL return the same bytes and the same clean flag as mime-rs's src/codec/base64.rs as of 2026-10-31, and it SHALL never fail or panic.

### Requirement: Standard padded encoding

With `Base64Alphabet::Standard`, the encoder SHALL write the RFC 4648 4 standard alphabet, padded with `=` to a whole quantum when the codec pads, and nothing for an empty input.

#### Scenario: The RFC 4648 vectors
- GIVEN `f`, `fo`, `foo` and `foobar`
- WHEN each is encoded unwrapped
- THEN the output is `Zg==`, `Zm8=`, `Zm9v` and `Zm9vYmFy`

### Requirement: One codec for every call

Every call SHALL run from a `Base64Codec`, a `Copy` value holding the backend, the alphabet, whether encoding pads and the line ending of encoded output: its one-shot methods encode or decode a whole input, and `Base64Encode::new` and `Base64Decode::new` take it to encode or decode chunk by chunk. The codec SHALL hold no state between calls, so that no one-shot call can run in the middle of a resumed input.

#### Scenario: One codec for the whole application
- GIVEN a codec built once with CRLF line endings
- WHEN it encodes an input in one call, and a `Base64Encode` built from it encodes the same input chunk by chunk, then finishes
- THEN both write the same bytes

### Requirement: Every backend matches scalar

For every input, alphabet, padding and line ending, every backend SHALL write the same bytes and return the same clean flag as the scalar steps, in one-shot and resumed calls alike.

#### Scenario: Every length on an AVX-512 machine
- GIVEN pseudo-random inputs of every length from 0 to 1,024, encoded unwrapped, with CRLF and with LF, under both alphabets, padded and not, and the same encodings rewrapped or with a stray byte inserted
- WHEN they are encoded and decoded under AVX-512, AVX2, SSSE3 and scalar
- THEN the four backends write identical bytes and flags

#### Scenario: Every short end
- GIVEN every input of up to 6 bytes drawn from `Q`, `z`, `=`, CR, LF, SP and `*`, alone and after `QUJD` or `QUJD` CRLF
- WHEN it is decoded in one call, and resumed with the join between the two parts, under every backend the CPU supports
- THEN every backend writes the bytes and flag of the scalar steps
