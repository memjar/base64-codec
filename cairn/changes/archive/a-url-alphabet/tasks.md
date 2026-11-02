---
cairn: tasks
change: a-url-alphabet
---

- [x] `Base64Alphabet` and the codec fields `alphabet` and `pad`, `Default` kept as `BASELINE`
- [x] Scalar tables and steps per alphabet, unpadded tails and `encoded_len`
- [x] Driver stamped per alphabet, `Loops` picked by backend and alphabet
- [x] SSSE3, AVX2, AVX-512 and NEON kernels per alphabet
- [x] Tests: URL vectors, unpadded vectors, a standard symbol under `Url`, every backend against scalar under both alphabets and paddings, kernel checks per alphabet
- [x] Fuzz targets, bench, tests/mime.rs and docs on the new fields
- [x] Standard path instruction counts measured at 64 bytes and 4 KiB: kernels unchanged, about 20 instructions a padded encode call and 8 a decode call of glue (see the log)
- [x] Fold the delta, log, CHANGELOG
