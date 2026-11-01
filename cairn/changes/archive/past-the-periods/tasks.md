---
cairn: tasks
change: past-the-periods
---

- [x] Re-measure with the method of the proposal, every workload averaged over several output offsets; the harness now also aligns loops to 64 bytes, see the log
- [x] Lines: the decode stitch of SSSE3 and scalar and the SSSE3 and AVX2 twins left as they are (a macro switch would cost what it saves); the line dispatchers, the learnt line ending and most inline comments gone instead, see the log
- [ ] Scalar decoding robust to codegen units, and faster on its loads: 32-bit loads per quantum measured 7% slower, moved to the-last-glue
- [ ] AVX2 unwrapped encoding below encodify's kernel cost: not attempted (no shorter split or translation found), moved to the-last-glue
- [x] AVX-512 wrapped encoding: fewer store bytes per line, the line endings written by the kernels' masked stores (16 stores fewer per period, 8% at 4 KiB)
- [x] Small inputs: the one-shot decode's glue (one trim, wrapped input sent to the lines from its first line) and the encoder's fixed cost (the unwrapped path apart)
- [ ] NEON measured on hardware: still qemu only, moved to the-last-glue
