---
cairn: change
id: a-url-alphabet
status: landed
created: 2026-11-01
---

# A URL alphabet

## Why

A PIM client needs base64url (RFC 4648 5) beside the standard alphabet: OAuth PKCE code challenges and the JWTs of Google service accounts, both unpadded, and the Gmail API `raw` message, a whole message where speed matters. The crate only had the standard alphabet and always padded, so these callers had to reach for another crate.

## What

- `codec::Base64Alphabet`, `Standard` (`+/`) or `Url` (`-_`), and the codec fields `alphabet` and `pad`. `BASELINE` and `Default` stay the standard alphabet, padded.
- Encoding writes the codec's alphabet; without `pad`, a last partial quantum ends after its 2 or 3 characters, and `encoded_len` counts that.
- Decoding takes the codec's alphabet alone: under `Url`, `+` and `/` are bytes outside the alphabet, skipped and reported, as `-` and `_` are under `Standard`. Every other lenient rule holds; missing padding was already clean, so `pad` changes nothing when decoding.
- Every backend stamps its loops once per alphabet, a `const URL: bool` on the driver functions, the kernels taking it as an argument they inline: the standard path keeps its exact kernels and loops. The table-driven backends (scalar, AVX-512, NEON) index a table per alphabet. SSSE3 and AVX2 swap their nibble and offset constants; decoding `_` (0x5F, high nibble 5 with `P` to `Z`) takes one compare and one OR more per block, only on the URL path.

Not in scope: a decoder accepting both alphabets at once, and strict decoding.
