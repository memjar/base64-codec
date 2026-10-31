---
cairn: tasks
change: the-last-speed-levers
---

- [x] Each run loop its own `#[inline(never)]` function in the driver macro
- [x] Decode kernels returning their error vector, tested once per line, the last block of a line wide and overlapping (tested once per unrolled group: tried, left out)
- [x] Run lengths that fit `src` and `dst` computed once, not checked per block
- [x] Encode multipliers hidden from the compiler (behind an empty `asm!`, not `black_box`, so that they hoist)
- [x] AVX2 encode loading from 4 bytes before the group, behind a lead the driver knows, unrolled, with an exact kernel for the first and last blocks
- [x] A scalar step of whole quanta before `decode_bytes`, and fixed-size carry moves in `encode_bytes`
- [x] Small inputs: line breaks skipped by the driver, the fast lenient tail, the one-shot encode
- [x] The encode line path with the same levers
- [x] The bench: unwrapped decoding against encodify's lenient engine too, not only its strict one
- [ ] Scalar decoding two quanta per step: moved to further-speed-levers
- [ ] AVX-512 VBMI kernels, if the bench shows them ahead of AVX2: moved to further-speed-levers
