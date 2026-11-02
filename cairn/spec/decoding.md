---
cairn: spec
capability: decoding
status: current
---

# Decoding

Lenient base64 decoding, as mail needs it, in the codec's alphabet, with the exact semantics of the base64 wrapper mime-rs used before this crate: it never fails, and returns the bytes and whether the input was clean.

### Requirement: Lenient decoding

The decoder SHALL skip CR, LF, SP and HTAB silently, skip every other byte outside the alphabet and report it, accept missing padding and non-zero trailing bits, decode symbols after padding as a new run and report it, and drop a lone trailing symbol and report it. For every input under the standard alphabet it SHALL return the same bytes and the same clean flag as mime-rs's src/codec/base64.rs as of 2026-10-31, and it SHALL never fail or panic.

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

### Requirement: The codec's alphabet alone

The decoder SHALL take the symbols of the codec's alphabet alone: under `Base64Alphabet::Url`, `-` and `_` as the sextets 62 and 63 and `+` and `/` as bytes outside the alphabet, and the reverse under `Base64Alphabet::Standard`, every other lenient rule unchanged.

#### Scenario: A standard symbol under the URL alphabet
- GIVEN `aGk+aGk` and a codec with the URL alphabet
- WHEN it is decoded
- THEN `+` is skipped, the bytes are those of `aGkaGk` and the input is reported unclean
