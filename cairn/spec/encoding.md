---
cairn: spec
capability: encoding
status: current
---

# Encoding

Base64 encoding with the standard alphabet and padding, either unwrapped, as an RFC 2047 B encoded word needs, or in lines of 76 characters, as a MIME body needs. The line width is not a parameter: MIME fixes it.

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
