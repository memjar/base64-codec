---
cairn: tasks
change: past-the-periods
---

- [ ] Re-measure with the method of the proposal, every workload averaged over several output offsets
- [ ] Lines: the decode stitch of SSSE3 and scalar, the SSSE3 and AVX2 twins, each cut measured
- [ ] Scalar decoding robust to codegen units, and faster on its loads
- [ ] AVX2 unwrapped encoding below encodify's kernel cost
- [ ] AVX-512 wrapped encoding: fewer store bytes per line
- [ ] Small inputs: the one-shot decode's glue and the encoder's fixed cost
- [ ] NEON measured on hardware
