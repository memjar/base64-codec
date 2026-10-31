# Attribution

base64-codec follows the code and design of two crates, both under the licences of this one (MIT OR Apache-2.0, see LICENSE-MIT and LICENSE-APACHE), and the published method of its AVX-512 backend.

## encodify 1.0.2

Copyright 2026 Stalwart Labs LLC <hello@stalw.art>, Apache-2.0 OR MIT, https://github.com/stalwartlabs/encodify

- [src/ssse3.rs](src/ssse3.rs) and [src/avx2.rs](src/avx2.rs): the x86 kernels of src/base64/kernel/x86.rs (nibble-table validation and translation, `maddubs` and `madd` packing, the encode spread, the class lookup of the encode offsets, the encode loading its group from 4 bytes before it, the AVX2 exact decode moving its bytes together with `vpermd`, the multipliers of the encode split kept from the compiler, exact ones with no mask).
- [src/neon.rs](src/neon.rs): the NEON kernels of src/base64/kernel/neon.rs (deinterleaving loads and stores, 64-byte table lookups).
- [src/scalar.rs](src/scalar.rs): the quantum a lenient decoder carries between calls, from src/base64/decode/lenient.rs, and the end of an input found by trimming its padding and blanks, from the strict layout of src/base64/decode/strict.rs.
- [src/driver.rs](src/driver.rs) and [src/decode.rs](src/decode.rs): the line breaks skipped between runs (`Break::skip`) and the runs of src/base64/kernel/mod.rs and x86.rs: errors of several blocks joined and tested once, a run that fails decoded again block by block and stopped at its first character outside the alphabet (`Valid::stop`), the last block of a line or a run overlapping the one before, encode runs unrolled.

The SSSE3, AVX2 and NEON kernels implement Wojciech Muła and Daniel Lemire, "Faster Base64 Encoding and Decoding using AVX2 Instructions", ACM Transactions on the Web 12(3), 2018.

## simdutf8 0.1.5

Hans Kratz <hans@appfour.com>, MIT OR Apache-2.0, https://github.com/rusticstuff/simdutf8

- [src/driver.rs](src/driver.rs): the loops written once as a macro and stamped into each backend module under its target features, from src/implementation/algorithm.rs, and the error of several blocks joined into one vector and tested once.
- [src/cpu.rs](src/cpu.rs): the backend picked from the build's target features when nothing is detected, from src/implementation/x86/mod.rs.

## AVX-512 VBMI method

[src/avx512.rs](src/avx512.rs) implements Wojciech Muła and Daniel Lemire, "Base64 encoding and decoding at almost the speed of a memory copy", Software: Practice and Experience 50(2), 2020: decoding translated and validated by one `vpermi2b` lookup in a 128-byte table and packed with `maddubs`, `madd` and `vpermb`; encoding spread by `vpermb`, split by `vpmultishiftqb` and translated by `vpermb` in the alphabet.
