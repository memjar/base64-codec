---
cairn: log
change: the-last-speed-levers
landed: 2026-10-31
---

# The last speed levers

Profiled the gap to encodify 1.0.2 (perf on the bench, exact instruction counts per call with perf stat and callgrind, the generated assembly read) and found it in the shared driver and the kernel contract, not in per-architecture specialisation; prototyped the levers on a throwaway copy, then applied them in the driver, every backend running the same loops.

What landed:

- The kernel contract: the wide decode kernel returns a `Bad`, each backend's error vector (`none`, `join`, `any`; the least translated byte on NEON); the invocation names the characters a wide decode takes and its spare bytes, and the bytes a wide encode takes, the window it reads, the lead of its group in it and an exact kernel reading the group alone (the AVX2 two-load one; the wide kernel itself for the other backends).
- One `#[inline(never)]` function per run: `decode_run` (wide blocks, narrow blocks, then quantum by quantum), `decode_lines`, `encode_run` and `encode_lines`, the line encoder small enough to inline into the line loop.
- Decoded lines in wide blocks whose errors are joined and tested once, the last one ending at the line's end and writing its spare bytes into the next line's output; the last line without room for them goes to the run.
- The AVX2 encode loading its 24 bytes from 4 bytes before them, 8 blocks per step taken as one fixed-size window array and one output array, its first and last blocks through the exact kernel; the SSSE3 and AVX2 encode split multiplying by exact powers of two hidden behind an empty `asm!` with `pure, nomem` (a `cfg(miri)` fallback returns the value as is).
- Line breaks between runs skipped by the driver, and `decode_tail`: the rest of an input that is at most three symbols followed by padding and blanks folded into the decoder state in one pass, leaving exactly the state `decode_bytes` would, which a new exhaustive test checks.
- `encode_once` per backend, which `encode` and `encode_vec` run with no `EncodeState`; `encode_tail` and `write_eol` write the padded quantum and the line ending as fixed-size values, in the one-shot path and in `encode_finish`, and the resumable carry is moved the same way.
- The scalar backend decoding in 16-character blocks, an overlapping block costing it per character.
- The bench comparing unwrapped decoding with encodify's lenient engine too, and benches/README.md re-measured, pinned to one core.

What moved from the plan:

- `black_box` gave way to an empty `asm!`: `black_box` cannot be hoisted out of a loop, a pure assembly block can, so the kernels keep their signatures with nothing passed in.
- Testing 4 decode blocks at once, the one lever applied before it was measured, cost 6% at 1 MiB on AVX2 (memory-bound, fewer instructions but more cycles) and gained nothing at 4 KiB: left out.
- The generic span function first used per line was not inlined into the line loop and cost about 175 instructions per line, twice the old wrapped encoding: replaced by `encode_line`. The narrow first and last blocks of an encode run cost a quarter more at 64 bytes than encodify's: the exact kernel replaced them.
- The scalar backend encodes unwrapped input about 7% slower than before, its table loop scheduled worse in the shared run while every other scalar workload is faster; two groups per step, in further-speed-levers, is the lever for it.

Checked: the test suite in release, debug, without default features and with `-C target-feature=+avx2`; clippy with `-D warnings` for all targets and for the library without default features, on x86_64 and aarch64; Miri on the library and on the SSSE3 and AVX2 kernels; the NEON backend under qemu-aarch64 user mode, cross-built for aarch64-unknown-linux-musl, not on hardware; the decode and encode fuzz targets for 5 minutes each (2.2 and 2.9 million runs), then 3 minutes each on the final code (1.6 and 2.0 million runs), clean.

Against encodify 1.0.2 (benches/README.md), from 4 KiB up: unwrapped encoding level (1.00x to 1.01x), wrapped encoding 1.12x to 1.13x, wrapped decoding 1.06x to 1.12x, unwrapped decoding 0.98x to 1.05x its strict engine and 1.04x to 1.05x its lenient one. At 64 bytes: wrapped encoding 1.46x, unwrapped encoding 0.91x, wrapped decoding 0.85x, unwrapped decoding 0.93x the lenient engine and 0.68x the strict one. Against the base64 crate line by line at 1 MiB, what mime-rs runs today: wrapped decoding 15.9x, wrapped encoding 7.3x. The levers left went to further-speed-levers.

Capabilities moved: none, speed only.
