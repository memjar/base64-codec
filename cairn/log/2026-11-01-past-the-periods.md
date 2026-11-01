---
cairn: log
change: past-the-periods
landed: 2026-11-01
---

# Past the periods

Took over the optimisation from the handoff of a-closer-look with its measuring method, the harness now also built with `-C llvm-args=-align-loops=64` (see below), every workload averaged over four output offsets against a copy of the tree before the session and encodify 1.0.2. The user asked for more speed, less code and most inline comments gone. Small calls got faster on every backend, wrapped decoding at every size, AVX-512 wrapped encoding at 4 KiB; the library lost 141 lines, 37 of them code.

What landed:

- The decoder finds wrapped input itself: at a line start with more than a line left, a line ending at character 76 sends the input to the line loops, so a wrapped input decodes in lines from its first line, where it decoded its first line as a run stopping at the line break (a failing group of blocks, then single blocks) and learnt the line ending from it. `DecodeState::lines` and `line_break` are gone. The one-shot decode keeps its single run for an input with no line ending at character 76; the source offset does not change the gain (AVX-512 wrapped decoding at 4 KiB 314 to 374 cycles against 381 to 446 over six source offsets).
- `trimmed` compares bytes instead of looking up their class, and reports whether the trimmed end holds padding: a wrapped decode trimmed its input twice, then scanned the end a third time for padding, about 18 of its 86 cycles at 64 bytes on AVX-512.
- A line loop writes its last line's blocks exactly when the spare bytes past it do not fit, through `decode_blocks`: within `decoded_len_max`, an input of fewer than 11 lines had no room for AVX-512's 16 spare bytes, so its lines went through runs and line breaks.
- The one-shot encode returns early when unwrapped, one run and the padded tail: 34 instructions fewer per call, 12% to 13% at 64 bytes on AVX-512 and AVX2, 6% to 8% on SSSE3 and scalar.
- The encode kernels write the line ending of a block that crosses or ends a line, `split` no longer clamped to the block: AVX-512 blends it into the vector of its masked store, the others write it in `spread`. Periods no longer write their line endings apart: 16 stores fewer per AVX-512 period, its wrapped encoding at 4 KiB 8% faster.
- `Loops::decode_lines` and `encode_lines` are pairs of loops indexed by the line ending, which drops the two dispatchers and a call level per wrapped call; `Run` names the loops' type.
- Inline comments from 93 lines to 57, all of them `SAFETY` comments, which the crate's lint requires, each on one line but one; private docs shortened; `#[inline]` dropped from the one-line `Bad` methods and helpers, instruction counts unchanged; the unroll counts local to the two loops using them.

Tried and left out:

- The one-shot decode through the decoder's loop alone, no single run first: 77 instructions more per unwrapped call, 10 cycles at 64 bytes; `#[inline(always)]` on the loop took 9 of them back.
- Wrapped input detected lazily, from where the first run stops, with no look at character 76: unwrapped decoding at 64 bytes level with the tree before (36.9 cycles against 36.5), wrapped decoding back to it (388 cycles at 4 KiB on AVX-512, 92 at 64 bytes on AVX2). Kept the look, which costs unwrapped 64-byte decoding about a cycle and 4 instructions.
- The line loop choosing spare or exact stores per block at run time: SSSE3 wrapped decoding 11% slower, scalar 25%, both line-bound. Every last line through `decode_blocks`, room or not: AVX2 wrapped decoding at 64 bytes 82 cycles against 71.
- Scalar quanta loaded as 32-bit words, their bytes extracted by shifts, half the loads: 7% slower at 4 KiB. Zen 5 has load ports to spare, and the second byte comes from `ch`, which adds latency.
- `decode_all` inlined into `decode`: no change, as `decode_vec` calls it too.

Found: a code layout change alone moved SSSE3 wrapped decoding at 64 bytes from 77 to 142 cycles (`ex_no_retire.load_not_complete` 8 times higher) and scalar wrapped encoding at 4 KiB by 150 cycles, with no code change in either; aligning loops to 64 bytes in the harness removed both, and is now part of the method. The tree before's AVX-512 wrapped encoding at 64 bytes reads 54 to 56 cycles in some builds and 44.7 to 45.9 in others, for the same reason. AVX-512 stores also run 15% to 20% slower on an output 16 bytes off a cache line (unwrapped encoding at 4 KiB 151 cycles aligned, 175 to 185 off by 16, the tree before alike), which the harness's output offsets, all multiples of 32, never showed: criterion's buffers, placed anew by the allocator each run, read that row at 84 and 104 GiB/s in two runs of this session, 117 in the run of a-closer-look.

Cycles per call, `perf stat` on core 13 with its sibling idle, averaged over the four output offsets, the tree before the session in brackets:

| workload | AVX-512 | AVX2 | SSSE3 | scalar | encodify |
| --- | --- | --- | --- | --- | --- |
| encode unwrapped, 64 B | 31.9 (36.7) | 37.5 (42.8) | 53.7 (58.1) | 97.9 (103.9) | 42.9 |
| encode unwrapped, 4 KiB | 158.2 (157.5) | 572.3 (574.8) | 1,175 (1,176) | 3,091 (3,105) | 578.1 |
| encode wrapped, 64 B | 43.6 (55.6) | 55.7 (59.4) | 60.9 (63.5) | 92.8 (94.9) | 110.9 |
| encode wrapped, 4 KiB | 291.4 (317.0) | 678.8 (671.4) | 1,222 (1,219) | 3,077 (3,056) | 941.5 |
| decode unwrapped, 64 B | 37.8 (36.7) | 43.2 (41.9) | 56.2 (54.7) | 98.2 (96.9) | 44.7 |
| decode unwrapped, 4 KiB | 194.5 (196.0) | 608.2 (606.0) | 1,234 (1,225) | 3,820 (3,819) | 695.5 |
| decode wrapped, 64 B | 71.7 (85.7) | 69.4 (91.9) | 74.6 (92.9) | 108.3 (125.6) | 115.3 |
| decode wrapped, 4 KiB | 330.5 (380.1) | 667.4 (707.0) | 1,301 (1,344) | 3,804 (3,815) | 1,043.5 |

The tree before's AVX-512 wrapped encoding at 64 bytes reads 55.6 in this build against 45.2 in the log of a-closer-look (see above). AVX2 and scalar wrapped encoding at 4 KiB, 1% above the tree before here with 25 instructions fewer, read 668.5 against 670.5 and 3,064 against 3,074 in other runs. Over eleven output offsets, AVX-512 wrapped encoding at 4 KiB reads 259 to 327 cycles against 298 to 370 for the tree before. Unwrapped decoding at 64 bytes is a cycle behind on every backend: the look at character 76.

The library: 2,750 lines before the tests against 2,891, 1,776 without comments and blank lines against 1,813.

Checked: the test suite in release, in debug with all features, without default features and with `-C target-cpu=native` (AVX-512 as `BASELINE`); clippy with `-D warnings` for all targets on x86_64 and aarch64, and for the library without default features; missing docs and rustdoc with `-D warnings`, private items included; the NEON backend under qemu-aarch64, built for aarch64-unknown-linux-musl, not on hardware; Miri on the library and, with `-C target-feature=+avx2`, on the kernels; the decode and encode fuzz targets for 3 minutes each, clean. The encode kernel tests now also check the line ending written at the split.

The levers left, with the traps met this time, went to the-last-glue.

Capabilities moved: none, speed only.
