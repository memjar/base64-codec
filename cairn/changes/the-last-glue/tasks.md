---
cairn: tasks
change: the-last-glue
---

- [ ] Re-measure with the method of the proposal, loops and functions aligned, output offsets averaged and source offsets swept for wrapped decoding
- [ ] AVX-512 stores aligned to the output's cache lines, for an output 16 bytes off one
- [ ] Small calls: one or two blocks without `decode_blocks`' fixed costs, unwrapped 64-byte decoding back ahead of past-the-periods' tree before
- [ ] AVX-512 wrapped encoding: output assembled at its own 64-byte positions, one store per 64 bytes
- [ ] AVX2 unwrapped encoding below encodify's kernel cost
- [ ] Scalar decoding robust to codegen units, with fewer operations around its loads
- [ ] Size: the SSSE3 and AVX2 twins, the stitch of SSSE3 and scalar, `decode_all`'s fast path, each cut measured
- [ ] NEON measured on hardware
