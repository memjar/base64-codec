---
cairn: spec
capability: encoding
status: current
---

# Encoding

Base64 encoding with the standard or the URL alphabet, padded or not, either unwrapped, as an RFC 2047 B encoded word or a URL token needs, or in lines of 76 characters, as a MIME body needs. The line width is not a parameter: MIME fixes it.

### Requirement: Standard padded encoding

With `Base64Alphabet::Standard`, the encoder SHALL write the RFC 4648 4 standard alphabet, padded with `=` to a whole quantum when the codec pads, and nothing for an empty input.

#### Scenario: The RFC 4648 vectors
- GIVEN `f`, `fo`, `foo` and `foobar`
- WHEN each is encoded unwrapped
- THEN the output is `Zg==`, `Zm8=`, `Zm9v` and `Zm9vYmFy`

### Requirement: MIME line wrapping

With a line ending set, the encoder SHALL write lines of 76 characters, the last one possibly shorter, each ended by that line ending, the last one included (RFC 2045 6.8). Without one, it SHALL write a single unbroken run, as an RFC 2047 B encoded word needs.

#### Scenario: One byte past a full line
- GIVEN 58 bytes `x` and a codec with CRLF
- WHEN they are encoded
- THEN the output is 76 characters, CRLF, `eA==`, CRLF: 84 bytes, which `Base64Codec::encoded_len` also returns

### Requirement: The URL alphabet

With `Base64Alphabet::Url`, the encoder SHALL write `-` and `_` for the sextets 62 and 63 (RFC 4648 5).

#### Scenario: The last two sextets
- GIVEN the bytes FB FF BF
- WHEN they are encoded with the standard and the URL alphabet
- THEN the outputs are `+/+/` and `-_-_`, and each decodes back clean with its own alphabet

### Requirement: Unpadded encoding

Without `pad`, the encoder SHALL end a last partial quantum after its 2 or 3 characters, with no `=`, and `Base64Codec::encoded_len` SHALL count that; decoding SHALL not depend on `pad`.

#### Scenario: The RFC 4648 vectors unpadded
- GIVEN `f`, `fo` and `foo`
- WHEN each is encoded without padding
- THEN the output is `Zg`, `Zm8` and `Zm9v`
