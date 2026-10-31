---
cairn: tasks
change: a-fresh-look
---

- [ ] Re-measure on a quiet machine with the method of the proposal, and decide which levers are worth their lines
- [ ] Unwrapped AVX2 at 4 KiB and up: anything below encodify's kernel cost (both crates run the same kernel)
- [ ] Small inputs: the decoder's loop and the narrow chain overhead, and the one-shot fast path's cost on wrapped input
- [ ] Scalar decoding two quanta per step, scalar encoding two groups per step
- [ ] NEON measured on hardware, and its 16-character kernels back if short runs need them
- [ ] Lines of code: the cuts listed in the proposal, each measured
