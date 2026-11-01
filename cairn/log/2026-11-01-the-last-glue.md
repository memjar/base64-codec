---
cairn: log
change: the-last-glue
landed: 2026-11-01
---

# The last glue

Took over the optimisation from the handoff of past-the-periods with its measuring method, now averaged over eight source and output offset pairs (see below) against a copy of the tree before the session and encodify 1.0.2. The user asked for more speed and less code, searching for new methods where needed. Unwrapped encoding got faster on AVX-512, AVX2 and SSSE3, small encoding calls on SSSE3 and scalar, wrapped decoding on every backend at every size; the library lost 38 lines, 34 of them code.

What landed:

- The encode kernels take `K` groups lying a group apart in one byte array, their size checked by a const assert, and an output each; the steps of `encode_blocks` give them four blocks, where they took eight blocks one call each. AVX2 runs each of its 11 operations across the four blocks before the next: one block's operations form one chain of dependencies, which fills Zen 5's FP scheduler (43% of the cycles of unwrapped encoding), and AVX2 unwrapped encoding at 4 KiB went from 571 to 552 cycles, ahead of encodify's 579. Steps of four blocks also fit a 63-byte run: SSSE3 and scalar unwrapped encoding at 64 bytes 13% and 12% faster.
- AVX-512 unwrapped encoding stores whole cache lines aligned: after the first block, its steps start at the first 64-byte boundary of the output, redoing up to 48 bytes, when the run has more than a step. 13% to 14% faster at 1 KiB and 4 KiB averaged over the offset pairs, from 154 to 133 cycles at 4 KiB.
- The one-shot decode takes wrapped input's first pass itself, whole lines, one run and the tail, through `lines_and_run`, which the decoder's loop shares; it went through that loop, building a decoder, for any input with a line ending at character 76. Wrapped decoding at 64 bytes went from 72 to 57 cycles on AVX-512 and from 69 to 55 on AVX2, 16% to 21% on every backend, 1% to 3% at 4 KiB.
- The decoder's `line_start` state is gone: it only gated the attempt at whole lines, which is correct anywhere, as `decode_lines` decodes 76 alphabet characters and a line ending or nothing, and `decode_bytes` never read it before its first byte.
- `decoded_len_max` is `len - len.div_ceil(4)`, the same values with 4 instructions fewer, which pays for the look at character 76 on unwrapped input: unwrapped decoding at 64 bytes level with the tree before on every backend.
- Size: helpers with one caller folded into it, as the user's rule has it (the SSSE3 and AVX2 `sextets` and `ascii` into their encode kernels, the six table builders of AVX-512 and scalar into const blocks of their statics, the two `opaque` into one); the line loops walk `chunks_exact` and their block starts, the decode line loop returns its line index.

Tried and left out:

- AVX-512 decode groups packed by three `vpermt2b` into three 64-byte stores, aligned when the output allows, where four `vpermb` and four overlapping stores pack them: level at 4 KiB, 5% at 16 KiB. A hand-written loop with no bounds check runs the same 2 cycles a block: loads alone run 1.41, stores alone 1.51, both 2.1, whatever the alignment. The decode loop is at the machine's limit for its operations.
- AVX2 decoding two blocks a step at a time: 2.5% slower; four: 9% slower, its 16 live vectors and 8 constants spill. AVX2 decoding with a joined `Bad`, one test per group: 9% slower at 4 KiB, `Bad::EAGER` stays. The SSSE3 encode kernel a step at a time: 0.5% faster than group by group, kept group by group, 5 lines shorter.
- The one-shot decode through the decoder's loop alone, inlined: 69 instructions more at 64 bytes. `lines_and_run` out of line: 27 more.
- Scalar runs through `decode_quanta` and `encode_groups`, without the driver: 5% slower decoding and 15% slower encoding at 4 KiB.
- AVX2 encoding indexing its offsets by `max(s, 51) - (s > 25)`, as `vpsubusb` has 2 pipes on Zen 5 where `vpmaxub` has 4: the same 3.23 cycles a block in registers.
- `encode_tail` through `encode_groups` on a zero-padded group: 55 instructions more a call. Single blocks walked by `step_by` over a run's length: 9 to 20 more on small calls. Exact chunks and a separate last block: fewer instructions on AVX2, more cycles.
- AVX-512 wrapped encoding's crossing block written by a plain store of a `vpermb`-shifted vector, where a masked store writes it: 155 instructions fewer per 64 lines, the same cycles.

Found: Zen 5 runs `vpermb`, `vpermt2b`, `vpermi2b` and `vpshufb` on 2 pipes with a latency of 5 cycles on 512 bits, `vpmaddubsw`, `vpmaddwd`, `vpmultishiftqb` and `vpmullw` on 2 pipes with a latency of 3, the two pairs sharing one pipe (3 between them); `vpsrlw`, `vpsubusb`, `vpmulhuw` and `vpmaddubsw` on 256 bits on 2 pipes, `vpand`, `vpor`, `vpaddb`, `vpsubb`, `vpcmpgtb`, `vpminub` and `vpmaxub` on 4; 64-byte stores one a cycle, aligned, off a line or masked, 32-byte stores half a cycle aligned and three quarters off a line. Measured with `asm!` loops in a scratch crate. The page distance between source and output moves a 4 KiB workload by up to 30%, as their streams alias on the low 12 address bits: AVX-512 wrapped decoding reads 284 cycles with both at the same page offset, 363 to 370 with them 2 KiB apart, unwrapped encoding about 125 to 290 cycles over 60 pairs; four output offsets from a source on a page boundary, the method before, could favour one tree by layout. AVX-512 wrapped encoding costs 55 cycles a period of 16 lines, 47 to 60 over eight page distances, against 21 for the same bytes unwrapped: the store queue is full 32% of its cycles and loads wait 23%, with 35 stores a period.

Cycles per call, `perf stat` on core 13 with its sibling idle, averaged over the source and output offset pairs 0:0, 0:1056, 0:2112, 0:3104, 528:2096, 1552:3600, 2576:624 and 3600:1616, the tree before the session in brackets:

| workload | AVX-512 | AVX2 | SSSE3 | scalar | encodify |
| --- | --- | --- | --- | --- | --- |
| encode unwrapped, 64 B | 32.3 (32.3) | 37.3 (37.4) | 46.7 (53.7) | 85.1 (96.5) | 43.1 |
| encode unwrapped, 4 KiB | 133.3 (153.5) | 551.7 (571.1) | 1,162 (1,172) | 3,108 (3,090) | 579.0 |
| encode wrapped, 64 B | 43.0 (43.4) | 55.5 (55.6) | 61.1 (60.9) | 91.8 (91.7) | 110.7 |
| encode wrapped, 4 KiB | 289.4 (279.9) | 669.1 (668.9) | 1,217 (1,218) | 3,070 (3,070) | 940.7 |
| decode unwrapped, 64 B | 38.4 (38.1) | 43.4 (43.4) | 55.7 (55.6) | 98.7 (98.4) | 43.9 |
| decode unwrapped, 4 KiB | 201.9 (201.8) | 609.5 (608.3) | 1,230 (1,230) | 3,819 (3,816) | 693.6 |
| decode wrapped, 64 B | 56.9 (71.8) | 55.2 (68.9) | 59.2 (74.4) | 89.0 (106.3) | 115.0 |
| decode wrapped, 4 KiB | 337.6 (349.2) | 653.6 (666.8) | 1,288 (1,310) | 3,758 (3,806) | 1,063.1 |

AVX-512 wrapped encoding at 4 KiB, 3% above the tree before in this run with 7 instructions more (the alignment test in its last line's run), reads 280.9 against 283.7 over the same pairs at five repeats each, its spread within one pair up to 100 cycles; it read 281.9 against 280.7 in another run. Scalar unwrapped encoding at 4 KiB is 0.6% above the tree before (199 instructions more): its steps of four blocks pay a loop's overhead twice as often, for 12% at 64 bytes. Instructions per call against the tree before: unwrapped encoding at 4 KiB 760 against 731 on AVX-512 and 2,885 against 2,675 on AVX2 (steps of four blocks), at 64 bytes 338 against 397 on SSSE3; wrapped decoding at 64 bytes 376 against 485 on AVX-512 and 344 against 451 on AVX2; unwrapped decoding at 64 bytes one fewer on every backend.

The library: 2,712 lines before the tests against 2,750, 1,742 without comments and blank lines against 1,776.

Checked: the test suite in release, in debug with all features, without default features and with `-C target-cpu=native` (AVX-512 as `BASELINE`); clippy with `-D warnings` for all targets on x86_64 and aarch64, and for the library without default features; missing docs and rustdoc with `-D warnings`, private items included; the NEON backend under qemu-aarch64, built for aarch64-unknown-linux-musl, not on hardware; Miri on the library and, with `-C target-feature=+avx2`, on the kernels; the decode and encode fuzz targets for 3 minutes each, clean.

The levers left, with the traps met this time, went to the-store-queue.

Capabilities moved: none, speed only.
