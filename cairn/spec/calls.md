---
cairn: spec
capability: calls
status: current
---

# Calls

How encoding and decoding are called: from one `Base64Codec` value, I/O-free, over the caller's buffers, at once through its one-shot methods or chunk by chunk through the `Base64Encode` and `Base64Decode` built from it, with no dependency and no allocation outside the `alloc` helpers.

### Requirement: Lengths and short buffers

`Base64Codec::encoded_len` SHALL return exactly what `Base64Codec::encode` writes, or none when that overflows `usize`. `Base64Codec::decoded_len_max` SHALL bound what `Base64Codec::decode` writes for any input of that length. `encode` and `decode` SHALL return none and write nothing when `dst` is shorter than their length helper asks; `resume` SHALL consume only what fits in `dst`. Decoding MAY overwrite the bytes of `dst` past the ones it reports written, within the length it was given.

### Requirement: Resumable without I/O

`Base64Encode` and `Base64Decode` SHALL give, for any split of the input into chunks and any output space offered, the same bytes and the same clean flag as the one-shot call, `finish` ending the input; an empty chunk SHALL change nothing. `resume` SHALL always progress with three bytes of room when decoding and six when encoding, and `finish` SHALL write nothing and return none when its output does not fit. The crate SHALL perform no I/O, and SHALL allocate only in the `alloc` helpers.

#### Scenario: A decode split mid-quantum and after padding
- GIVEN `aGk=aGk=` resumed as `aG`, `k=aG`, `k=`, then finished
- WHEN the outputs are joined
- THEN the bytes are `hihi` and the input is reported unclean

### Requirement: No dependency, no std

The library SHALL build `#![no_std]` with no dependency under every feature combination, `std` serving only `Base64Cpu::detect` and `alloc` only `Base64Codec::encode_vec` and `Base64Codec::decode_vec`.

### Requirement: One codec for every call

Every call SHALL run from a `Base64Codec`, a `Copy` value holding the backend and the line ending of encoded output: its one-shot methods encode or decode a whole input, and `Base64Encode::new` and `Base64Decode::new` take it to encode or decode chunk by chunk. The codec SHALL hold no state between calls, so that no one-shot call can run in the middle of a resumed input.

#### Scenario: One codec for the whole application
- GIVEN a codec built once with CRLF line endings
- WHEN it encodes an input in one call, and a `Base64Encode` built from it encodes the same input chunk by chunk, then finishes
- THEN both write the same bytes
