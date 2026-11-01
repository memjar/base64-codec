---
cairn: tasks
change: the-last-glue
---

- [x] Re-measure with the method of the proposal, loops and functions aligned; offsets now averaged over eight source and output pairs, as the page distance between the two moves a workload by up to 30%, see the log
- [x] AVX-512 stores aligned to the output's cache lines: unwrapped encoding's steps start at the first 64-byte boundary (14% at 4 KiB); decoding left out, an ideal loop of three aligned stores per four blocks ran no faster
- [x] Small calls: wrapped decoding's first pass moved into the one-shot decode (20% at 64 bytes), unwrapped 64-byte decoding level with the tree before; one or two blocks without `decode_blocks`' fixed costs not reached, moved to the-store-queue
- [ ] AVX-512 wrapped encoding: output assembled at its own 64-byte positions, not attempted (about 60 lines of AVX-512 code), moved to the-store-queue with the new measurements
- [x] AVX2 unwrapped encoding: still the 11-operation kernel, its steps run across four blocks at a time, 3% faster than the tree before and encodify
- [ ] Scalar decoding robust to codegen units: not attempted, moved to the-store-queue
- [x] Size: single-caller helpers folded, the line loops shortened, the `line_start` state gone; the twins and the stitch left, see the log
- [ ] NEON measured on hardware: still qemu only, moved to the-store-queue
