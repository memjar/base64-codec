---
cairn: tasks
change: further-speed-levers
---

- [x] Kernels reporting their first character outside the alphabet: the run stops at its first bad quantum, the narrow and quantum-by-quantum loops gone
- [x] The line shape read without a byte scan: learnt from a line read whole
- [x] SSSE3 encoding with a lead
- [x] AVX-512 VBMI kernels, the bench showing them ahead of AVX2 (3x to 4x at 4 KiB)
- [x] The driver cut to four stamped loops behind a `Loops` table, the decoder and encoder loops plain code
- [x] The end of an input found by trimming its padding and blanks, and the one-shot decode in one run when nothing else is left
- [x] `decode_bytes` with its state in locals
- [ ] Scalar decoding two quanta per step: moved to a-fresh-look
- [ ] Scalar encoding two groups per step: moved to a-fresh-look
- [ ] NEON measured on hardware: moved to a-fresh-look
