---
cairn: tasks
change: the-store-queue
---

- [x] Re-measure with the method of the proposal, loops and functions aligned, every workload averaged over source and output offset pairs
- [x] AVX-512 wrapped encoding: output assembled at its own 64-byte positions, one store per 64 bytes (cycles of `vpermt2b` over encoded blocks, 14% to 16% faster from 2 KiB)
- [ ] Small calls: one or two blocks without `decode_blocks`' fixed costs, single blocks with fewer bounds checks (moved to the-gathered-blocks)
- [ ] AVX2 unwrapped decoding at 64 bytes clearly ahead of encodify (moved to the-gathered-blocks)
- [ ] Scalar decoding robust to codegen units, with fewer operations around its loads (moved to the-gathered-blocks)
- [ ] Size: the SSSE3 and AVX2 twins, the stitch of SSSE3 and scalar, `encode_all` against the encoder's run, each cut measured (moved to the-gathered-blocks)
- [ ] NEON measured on hardware (moved to the-gathered-blocks)
