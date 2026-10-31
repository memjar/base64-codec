---
cairn: spec
capability: calls
status: current
---

# Calls

How encoding and decoding are called: I/O-free, over the caller's buffers, chunk by chunk through `Base64Encode` and `Base64Decode` or at once through the one-shot functions built on them, with no dependency and no allocation outside the `alloc` helpers.

### Requirement: Lengths and short buffers

`encoded_len` SHALL return exactly what `encode` writes, or none when that overflows `usize`. `decoded_len_max` SHALL bound what `decode` writes for any input of that length. `encode` and `decode` SHALL return none and write nothing when `dst` is shorter than their length helper asks; `resume` SHALL consume only what fits in `dst`. Decoding MAY overwrite the bytes of `dst` past the ones it reports written, within the length it was given.

### Requirement: Resumable without I/O

`Base64Encode` and `Base64Decode` SHALL give, for any split of the input into chunks and any output space offered, the same bytes and the same clean flag as the one-shot call, `finish` ending the input; an empty chunk SHALL change nothing. `resume` SHALL always progress with three bytes of room when decoding and six when encoding, and `finish` SHALL write nothing and return none when its output does not fit. The crate SHALL perform no I/O, and SHALL allocate only in the `alloc` helpers.

#### Scenario: A decode split mid-quantum and after padding
- GIVEN `aGk=aGk=` resumed as `aG`, `k=aG`, `k=`, then finished
- WHEN the outputs are joined
- THEN the bytes are `hihi` and the input is reported unclean

### Requirement: No dependency, no std

The library SHALL build `#![no_std]` with no dependency under every feature combination, `std` serving only `Base64Cpu::detect` and `alloc` only `encode_vec` and `decode_vec`.
