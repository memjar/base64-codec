---
cairn: tasks
change: the-gathered-blocks
---

- [ ] Rebuild the harness of the proposal, re-measure against HEAD over the offset pairs
- [ ] Size: win back the 94 lines of the-store-queue, each cut measured
- [ ] AVX-512 wrapped decoding by gathered blocks, the lookup per input vector, ahead of the periods at every size from 32 lines
- [ ] AVX-512 wrapped encoding toward 1.55 cycles a vector
- [ ] NEON wrapped encoding by cycles, under qemu
- [ ] The levers of the-store-queue not taken: small calls, AVX2 unwrapped decoding at 64 bytes, scalar decoding, NEON on hardware
