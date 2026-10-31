---
cairn: change
id: further-speed-levers
status: landed
created: 2026-10-31
---

# Further speed levers

What the-last-speed-levers left, each measured before it is kept, and kept only if it pays without per-architecture line kernels. The numbers are in benches/README.md and the log of 2026-10-31.

## Why

After the-last-speed-levers, base64-codec is ahead of encodify 1.0.2 or level with it on every workload from 4 KiB up, and ahead on wrapped encoding at 64 bytes (1.46x). At 64 bytes it still trails on unwrapped encoding (0.91x), on wrapped decoding (0.85x), and on unwrapped decoding (0.93x encodify's lenient engine, 0.68x its strict `STANDARD`). The scalar backend, which `Base64Cpu::BASELINE` runs on a default x86_64 build, decodes unwrapped input at 0.82x the base64 crate, and encodes unwrapped input about 7% slower than before the-last-speed-levers, its table loop scheduled worse in the shared run.

## What

- Scalar decoding: two quanta per step into one 64-bit word, as the base64 crate's `decode_chunk_8` and encodify's `decode_quads_scalar` do.
- Scalar encoding: two groups per step, 6 bytes into 8 characters through one 64-bit word, as encodify's `encode_groups_scalar` does, which would also win back the 7% lost.
- Kernels reporting their first character outside the alphabet (encodify's `Valid::stop`), so that a block commits its whole quanta before a line break and the quantum step has less to take.
- The line shape read without a byte scan of the second line, which most of what is left of a 64-byte wrapped decode is.
- SSSE3 encoding loading its 12 bytes from 4 bytes before them, as the AVX2 kernel does, measured on a CPU without AVX2 or with `Base64Cpu::assume`.
- NEON measured on hardware: the backend passes the suite under qemu only.
- AVX-512 VBMI: 64 characters per step through `vpermb`, if the bench shows it ahead of AVX2.

The levers are speed only, but the AVX-512 backend moves the backends capability: detection picks it first, and every backend matches scalar under it too (delta.md).
