---
cairn: log
change: further-speed-levers
landed: 2026-10-31
---

# Further speed levers

Asked to find where the time went against encodify 1.0.2 and to get ahead of it in every bench section with the most concise code, profiled on x86_64 (perf stat cycles and instructions per call, callgrind, the assembly read): the kernels were encodify's, cycle for cycle, and the driver was heavy. A 64-byte unwrapped decode spent 473 instructions, about 60 of them in kernels: `decode_run` 245 (wide, narrow and quantum-by-quantum loops), `decode_tail` 76, the stamped decode loop 67, the entry 74. Rewrote the driver around that and added an AVX-512 VBMI backend.

What landed:

- The driver stamps four loops per backend, `decode_run`, `decode_lines`, `encode_run` and `encode_lines`, the runs as a dispatcher handing what is shorter than a block to the narrower backend before any register is saved, and an `#[inline(never)]` block loop. `Loops` holds them as `unsafe fn` pointers, `Base64Cpu::loops` picks them. The resumable and one-shot loops are plain code in decode.rs and encode.rs, no longer stamped per backend.
- A run stops at its first character outside the alphabet, as encodify's `Valid::stop`: the decode kernels return a `Bad` telling how many characters came `first` (movemask and tzcnt on x86, the zero lanes of the per-quantum minimum on NEON, a byte of joined classes per quantum in scalar). The narrow and quantum-by-quantum loops of a run went.
- Groups of 4 wide decode blocks tested once, counted from the run's length alone; encode steps of 8 blocks over one window array; SSSE3 encoding loading its 12 bytes from 4 bytes before them.
- The line width learnt from a line read whole, with no byte scan (`DecodeState::line_break`).
- The end of an input: its padding and blanks trimmed once, the runs stopping there so their last block succeeds, the at most three symbols left folded into the state by `decode_tail`. The one-shot decode is one run and that tail when nothing else is left, the decoder's loop from where the run stopped otherwise.
- `decode_bytes` works on locals and writes fixed-size outputs; it stored every field per byte and called `memcpy` in `flush`.
- The AVX-512 VBMI backend, src/avx512.rs, detected first on x86: one `vpermi2b` validates and translates 64 characters, three permutes encode 48 bytes. `rust-version` is 1.89, for its intrinsics.
- The scalar backend runs the driver with 16-character table-lookup blocks; its quantum and group loops take what is shorter than any backend's block.
- NEON lost its 16-character kernels: runs below 64 characters are scalar on aarch64.
- benches/compare.rs runs the AVX2 backend apart when AVX-512 is detected, to compare with encodify on the same instructions.

Tried and left out: the last block of each decode group stored exactly so that groups map onto `as_chunks` (11% fewer instructions, 4% more cycles: one more shuffle on Zen 5's two shuffle pipes, which bound the AVX2 decode); the high nibbles by `vpmulhuw` on the multiply pipes (no gain); the tail check before the line-break check, and a one-shot fast path redoing the first line (wrapped decoding up to 12% slower); scalar runs beside the driver, two or four quanta per step (17% slower than the old tree until the scalar kernels went back through the driver with `#[inline]`); scalar encoding group by group in its block (10% slower than two groups per 64-bit word).

Cycles per call, `perf stat` on one core with an idle SMT sibling, interleaved runs (instructions in brackets):

| workload | AVX-512 | AVX2 | tree before | encodify |
| --- | --- | --- | --- | --- |
| encode unwrapped, 64 B | 35 (237) | 41 (293) | 43 (344) | 46 (255) |
| encode unwrapped, 4 KiB | 130 (758) | 573 (2,697) | 573 (2,969) | 572 (2,531) |
| encode wrapped, 64 B | 60 (358) | 61 (387) | 60 (434) | 105 (640) |
| encode wrapped, 4 KiB | 284 (1,467) | 815 (3,652) | 798 (5,704) | 920 (5,566) |
| decode unwrapped, 64 B | 47 (281) | 51 to 52 (322) | 69 (490) | 45 to 47 (294) |
| decode unwrapped, 4 KiB | 191 (1,316) | 693 to 697 (3,669) | 702 to 758 (4,859) | 684 to 698 (3,366) |
| decode wrapped, 64 B | 103 (616) | 100 (648) | 130 (918) | 110 (728) |
| decode wrapped, 4 KiB | 419 (3,061) | 916 to 919 (5,412) | 1,013 (5,909) | 1,049 to 1,065 (7,118) |

The scalar backend against the tree before, at 4 KiB: unwrapped decoding level (6,464 against 6,480 cycles), wrapped decoding 2% slower, unwrapped encoding 4% slower, wrapped encoding 23% slower (6,039 against 4,899); at 64 bytes wrapped decoding is 25% faster.

Criterion medians, in benches/README.md (the decoding groups re-run once the shared machine was quiet): with AVX-512, base64-codec is ahead of encodify in every section but unwrapped decoding at 64 bytes, level there with encodify's strict `STANDARD` (5.96 against 5.92 GiB/s) and ahead of its lenient engine (4.41); from 4 KiB it runs 1.5x to 2.7x encodify. On AVX2 alone it is ahead on wrapped input, level on unwrapped encoding and behind on unwrapped decoding by up to 2% from 4 KiB and 8% at 64 bytes.

The library did not get shorter: 2,576 lines before the tests against 2,377, 1,629 without comments and blank lines against 1,534. The driver lost about 190 lines, AVX-512 added 216, and the exact kernels and the loops moved to decode.rs and encode.rs the rest.

Checked: the test suite in release, in debug with all features, without default features and with `-C target-cpu=native` (AVX-512 as `BASELINE`), with a new exhaustive test of every short end, which caught `decode_bytes` handing back past the trimmed end in the first version; clippy with `-D warnings` for all targets on x86_64 and aarch64; missing docs and rustdoc links; the NEON backend under qemu-aarch64, built for aarch64-unknown-linux-musl and linked with rust-lld, 23 unit tests and tests/mime.rs, not on hardware; the decode and encode fuzz targets 5 minutes each on the first AVX-512 code (1.18 and 1.49 million runs) and 3 minutes each on the final code (0.83 and 1.68 million runs), clean. Miri did not run: no shell with it was at hand.

The levers left, with the measuring method and the traps met, went to a-fresh-look.

Capabilities moved: backends (ADDED: The widest backend the CPU has; MODIFIED: Every backend matches scalar).
