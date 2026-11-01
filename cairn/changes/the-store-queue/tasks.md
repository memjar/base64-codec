---
cairn: tasks
change: the-store-queue
---

- [ ] Re-measure with the method of the proposal, loops and functions aligned, every workload averaged over source and output offset pairs
- [ ] AVX-512 wrapped encoding: what holds its 35 stores a period and the loads behind them, or output assembled at its own 64-byte positions, one store per 64 bytes
- [ ] Small calls: one or two blocks without `decode_blocks`' fixed costs, single blocks with fewer bounds checks
- [ ] AVX2 unwrapped decoding at 64 bytes clearly ahead of encodify
- [ ] Scalar decoding robust to codegen units, with fewer operations around its loads
- [ ] Size: the SSSE3 and AVX2 twins, the stitch of SSSE3 and scalar, `encode_all` against the encoder's run, each cut measured
- [ ] NEON measured on hardware
