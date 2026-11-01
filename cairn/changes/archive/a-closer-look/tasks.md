---
cairn: tasks
change: a-closer-look
---

- [x] Re-measure with the method of the proposal, the harness built with aligned functions; output offsets averaged, as placement moves wrapped workloads by up to 30%
- [ ] AVX2 unwrapped encoding below encodify's kernel cost: not attempted, moved to past-the-periods
- [x] AVX-512 wrapped decoding: fewer than two blocks per line, 1.19 in periods of 16 lines, a block across a line break stitched from its two lines
- [x] Wrapped encoding: periods whose blocks cross the line breaks, reading windows, rather than windows checked once per line
- [ ] The one-shot decode's glue: not attempted, moved to past-the-periods
- [ ] NEON measured on hardware: still qemu only, moved to past-the-periods
- [x] Lines: the AVX2 `opaque` dropped for the SSSE3 one broadcast; the periods added more than any cut took out, see the log
