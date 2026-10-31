---
cairn: tasks
change: a-fresh-look
---

- [x] Re-measure on a quiet machine with the method of the proposal, and decide which levers are worth their lines
- [x] Unwrapped AVX2 at 4 KiB and up: decoding below encodify's kernel cost (the clamp of `/` and `vptest` per block, 692.6 to 610.3 cycles); encoding still level, moved to a-closer-look
- [x] Small inputs: the one-shot decode's usual end without a decoder, AVX-512 short runs in one masked block
- [x] Scalar decoding and encoding on tables, 1.6x to 1.8x the tree before
- [ ] NEON measured on hardware: moved to a-closer-look
- [x] Lines of code: the wide and exact kernels merged, the shorter tables and detection; level overall, the SSSE3 twins moved to a-closer-look, SSSE3 kept by the user
