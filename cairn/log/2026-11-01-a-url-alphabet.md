---
cairn: log
change: a-url-alphabet
landed: 2026-11-01
---

# A URL alphabet

A PIM client needs base64url (RFC 4648 5) beside the standard alphabet: OAuth PKCE code challenges and the JWTs of Google service accounts, both unpadded, and the Gmail API `raw` message. The user asked for it, pointing at encodify's alphabet tables for inspiration.

What landed:

- `codec::Base64Alphabet`, `Standard` (the default) or `Url`, and the codec fields `alphabet` and `pad`. `Default` is now written out as `BASELINE`, since a derived one would leave `pad` false.
- Encoding writes the codec's alphabet; without `pad`, a last partial quantum ends after its 2 or 3 characters, and `encoded_len` counts `len + len.div_ceil(3)` characters. Decoding takes the codec's alphabet alone, the other one's two symbols being stray bytes; `pad` changes nothing when decoding, missing padding being clean already.
- Every backend's `LOOPS` is now `[Loops; 2]`, the standard loops then the URL ones, `Base64Cpu::loops` picking by alphabet: the driver functions take a `const URL: bool`, and the kernels and narrow loops a `url` argument that folds once they inline. The scalar backend, AVX-512 and NEON index a table per alphabet (`ALPHABETS`, `CLASSES`, `QUANTA`, `PAIRS`, the VBMI and NEON decode tables). SSSE3 and AVX2 swap five low-nibble classes, the class of the high nibble 7, the offset of the high nibble 2 and the encode offsets of 62 and 63, the URL classes being encodify 1.0.2's; `_` (0x5F) shares the high nibble 5 with `P` to `Z`, so the URL kernels OR in a compare with `_` before the `min` with 63 that saturates it.
- AVX2 hands short inputs to SSSE3's stamped loops through `decode_short` and `encode_short`, matching on the alphabet. Inlined, they pulled SSSE3's `encode_run` into AVX2's and cost a `push` and `pop` on every call, so the stamped `decode_run` and `encode_run` are `#[inline(never)]`, as they were in effect before: only AVX2 calls them directly.
- `encode_tail` keeps its one 4-byte store when padding and writes 2 or 3 bytes otherwise; `EncodeState` holds the alphabet and padding, `DecodeState` the alphabet.
- Tests: the URL symbols and a standard symbol under `Url` decoded, 62 and 63 encoded in both alphabets, the RFC 4648 vectors unpadded, a wrapped unpadded tail; the driver tests over both alphabets and paddings (every length, every split) and both alphabets (every byte at every position, every short end, `_` and `+` taking the place of `z` and `*` under `Url`); every kernel checked against scalar under both alphabets. The fuzz targets take the alphabet from the room byte (decode) and the alphabet and padding from the options byte (encode).

Instructions per call on the standard alphabet, padded, `perf stat` on core 13, the tree before this change in brackets, built with rustc 1.99.1, loops and functions aligned, buffers at one offset pair only:

| workload | AVX-512 | AVX2 | SSSE3 | scalar |
| --- | --- | --- | --- | --- |
| encode unwrapped, 64 B | 226 (206) | 278 (258) | 336 (326) | 529 (509) |
| encode wrapped, 64 B | 257 (233) | 365 (333) | 395 (363) | 566 (539) |
| decode unwrapped, 64 B | 262 (254) | 311 (304) | 415 (405) | 564 (554) |
| decode wrapped, 64 B | 376 (370) | 355 (338) | 389 (381) | 523 (515) |
| encode unwrapped, 4 KiB | 794 (774) | 2,893 (2,873) | 6,888 (6,878) | 18,505 (18,485) |
| encode wrapped, 4 KiB | 1,095 (1,071) | 2,991 (2,967) | 6,546 (6,532) | 18,355 (18,331) |
| decode unwrapped, 4 KiB | 1,297 (1,289) | 3,406 (3,399) | 7,975 (7,965) | 17,280 (17,270) |
| decode wrapped, 4 KiB | 1,542 (1,536) | 3,370 (3,365) | 7,914 (7,906) | 17,153 (17,145) |

Each 4 KiB row moves by about the constant of its 64-byte row: the kernels and loops did not change, the cost is per call, in the glue reading the wider codec (the line ending, the alphabet and the padding packed in one register, the loops picked by backend and alphabet, the alphabet and padding in `encode_tail`). Cycles per call at 64 bytes, the least of five runs at one offset pair: encode 1 to 3.5 cycles above (AVX2 unwrapped 41.3 against 38.2, still ahead of encodify's 43.1 of the-store-queue), decode within the layout noise this harness does not average out. The URL alphabet runs the standard one's instructions except on SSSE3 and AVX2 decoding: about 2 instructions a block more on AVX2 (3,755 against 3,406 unwrapped at 4 KiB) and 6 on SSSE3 (10,024 against 7,975), its two-operand forms copying the characters before the compare.

The library, counted up to each file's first `#[cfg(test)]`: 2,659 lines against 2,435, 1,879 without comments and blank lines against 1,716.

Checked: the suite in release and debug with all features and in release with `-C target-cpu=native`; the library without default features, depending on simd-cpu alone; clippy for all targets with `-D warnings` on x86_64 and on aarch64; rustdoc with `-D warnings`; the bench built; the aarch64 suite under qemu-aarch64 on neoverse-n1, release and debug, built for aarch64-unknown-linux-musl, not on hardware; Miri on the library; the decode and encode fuzz targets for 2 minutes each, clean (297,730 and 605,736 runs). Miri was not run on the kernels with `+avx2` (40 minutes before this change, twice the cases now): their only change is constants and one compare and OR, no unsafe code.

Capabilities moved: encoding (the URL alphabet, unpadded encoding, the standard alphabet scoped), decoding (the codec's alphabet alone, the mime-rs equivalence scoped to the standard alphabet), calls (the codec's fields), backends (both alphabets and paddings matching scalar).
