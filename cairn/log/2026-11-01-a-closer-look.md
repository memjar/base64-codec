---
cairn: log
change: a-closer-look
landed: 2026-11-01
---

# A closer look at the speed

Took over the optimisation from the handoff of a-fresh-look, with its measuring method: `perf stat` cycles and instructions per call over two iteration counts, against a copy of the tree before the session and encodify 1.0.2, the harness built with `-C llvm-args=-align-all-functions=6`. One trap was new: where the output buffer sits against the input moves wrapped workloads by up to 30% (the tree before encoded 4 KiB wrapped with AVX-512 in 250 to 364 cycles depending on the output's offset alone), so every number below is averaged over four output offsets (0, 1056, 2112 and 3104 bytes past a page, the input at 0). Wrapped input, the MIME case, now decodes faster on AVX-512 and AVX2 and encodes faster on AVX2, SSSE3 and scalar; everything else is level with the tree before.

What landed:

- Lines of 76 characters in periods: as 76 is 4 times 19, the characters of `$chars / 4` lines make exactly 19 blocks (16 lines for AVX-512 and NEON, 8 for AVX2, 4 for SSSE3 and scalar), and a period's 19 blocks are written out by `base64_period!`, each block's offset and split constants folded at compile time. A block across a line break decodes from its two lines stitched (two loads and a blend: `vpblendmb`, `vpblendvb`, and/or for SSSE3, `vbsl` per deinterleaved vector, a quantum's source for scalar), and encodes with its characters past the line written past the line ending (an AVX-512 masked store, quanta moved by `spread` elsewhere). AVX-512 now runs 1.19 blocks per line where it ran 2, AVX2 2.375 where it ran 3.
- The lines past the whole periods, and calls with fewer lines than a period, go line by line: each period function ends by calling the line function on the rest, so that the line loop is inlined once, at its only call site, and a short call saves no register.
- Decoding in periods only with blocks of 32 characters or more (AVX2, AVX-512, NEON); SSSE3 and scalar decode line by line as before (see below). Encoding is in periods on every backend: they read windows where they fit, where the line loop read groups alone, which is most of the SSSE3 and scalar gain.
- Lines decode as lines only when 76 characters wide, as MIME writes them: the decoder learns the line ending of a whole 76-character line (`DecodeState::lines` is now `Option<bool>`, `LINE_MIN` gone, `decode_lines` takes the ending alone); other widths go through runs and line breaks, as encodify decodes every width.
- AVX2's multipliers come from the SSSE3 `opaque`, broadcast: its own `opaque` and Miri twin gone, level.
- The decode kernels take the characters as `&[u8; C]` with a `split`, the encode kernels their output as `&mut [_; N]` with a `split`; the kernel tests check every split, with CRLF stitched in or the characters spread around a gap, and the every-byte-at-every-position test now runs on CRLF lines too.

Tried and left out:

- One stream of blocks over the lines, each block's column and offset tracked: 45 scalar instructions per block, AVX-512 wrapped decoding at 640 cycles against 410; the decode at IPC near 8, so instruction count is the cost there.
- A stitched block per line with the column carried from line to line: 90 instructions per line.
- The 19 blocks of a period as a loop: LLVM rolls them back into a loop working out each block's place at run time (545 cycles); written out by the macro: 330. A table of places in a loop: rolled too.
- A shorter last period with a runtime line count: small inputs paid 20 to 60 instructions. The last period moved back over lines done: AVX2 decoding at 600 bytes 31% slower (a period redoing 7 of 9 lines), AVX-512 encoding at 4 KiB 11% slower.
- The encode split by `copy_within`: a `memmove` call per block, 2,200 cycles at 4 KiB; by a fixed-size copy of the block's characters: store-to-load forwarding stalls, 1,460. On AVX2, by `vpmaskmovd`: 633 cycles at 4 KiB against 672 with `spread` on Zen 5, left out as AMD's masked stores before Zen 4 are microcoded, and AVX2 is the backend of those CPUs. On AVX-512, the masked store against a `vpermb` and one plain store, or two plain stores: level, the loop bound by its stores (the store queue full 40% of its cycles).
- Decoding periods for SSSE3 and scalar: SSSE3 level, scalar about 200 cycles a call slower at every size from 600 bytes, with as many instructions, loads waiting (`ex_no_retire.load_not_complete`); cause not found. Scalar decode blocks of 8 characters: steady across codegen units but slower.
- The line functions folded into their dispatchers: the line's length becomes a runtime divisor, a `div` per call, 7% on 64-byte wrapped encoding. A compiler fence after each scalar block: wrapped decoding slower.

Found and left as is: the scalar unwrapped decode at 4 KiB measures 3,770 or 4,466 cycles depending on how rustc splits the crate into codegen units, before and after this session alike (with one codegen unit, the tree before also reads 4,455): when the lookup tables' initialiser is visible, LLVM hoists a group's 16 lookups above its stores and spills them.

Cycles per call, `perf stat` on core 13 with its sibling idle, averaged over the four output offsets, the tree before the session in brackets:

| workload | AVX-512 | AVX2 | SSSE3 | scalar | encodify |
| --- | --- | --- | --- | --- | --- |
| encode unwrapped, 64 B | 37.6 (36.7) | 42.8 (42.4) | 57.8 (58.0) | 104.3 (103.6) | 44.1 |
| encode unwrapped, 4 KiB | 162.1 (159.4) | 577.6 (575.2) | 1,176 (1,176) | 3,100 (3,103) | 578.6 |
| encode wrapped, 64 B | 45.2 (44.7) | 59.7 (58.4) | 62.4 (63.1) | 95.5 (92.2) | 109.2 |
| encode wrapped, 4 KiB | 313.3 (335.8) | 670.3 (817.4) | 1,218 (1,392) | 3,057 (3,289) | 931.0 |
| decode unwrapped, 64 B | 37.9 (36.6) | 43.0 (42.6) | 55.1 (54.3) | 100.1 (97.2) | 45.5 |
| decode unwrapped, 4 KiB | 195.1 (195.7) | 605.9 (615.1) | 1,233 (1,233) | 3,813 (3,781) | 693.2 |
| decode wrapped, 64 B | 86.1 (87.3) | 90.7 (91.7) | 92.2 (95.4) | 127.3 (127.9) | 118.2 |
| decode wrapped, 4 KiB | 387.2 (451.4) | 714.7 (830.0) | 1,341 (1,360) | 3,828 (3,834) | 1,064.6 |

At 64 KiB, AVX-512 wrapped encoding reads 4,821 cycles against 5,132 and wrapped decoding 5,530 against 6,329. The rows within 3% moved by one or two cycles of noise on this loaded machine (load average 3 to 12 during the session), with instruction counts within four of the tree before.

The library grew: 2,895 lines before the tests against 2,618, 1,813 without comments and blank lines against 1,630. The periods, the stitch and split in every kernel and `spread` added about 340 lines; dropping `places` (the arithmetic folds once the blocks are written out), the line-width plumbing and the AVX2 `opaque` took about 60 out.

Checked: the test suite in release, in debug with all features, without default features and with `-C target-cpu=native` (AVX-512 as `BASELINE`); clippy with `-D warnings` for all targets on x86_64 and aarch64, and for the library without default features; rustdoc with `-D warnings`, private items included; the NEON backend under qemu-aarch64, built for aarch64-unknown-linux-musl and linked with rust-lld, not on hardware; Miri on the library and, with `-C target-feature=+avx2`, on the kernels; the decode and encode fuzz targets for 3 minutes each (0.55 and 1.33 million runs), then 2.5 minutes each on the final code (0.45 and 1.03 million runs), clean.

The levers left, with the traps met this time, went to past-the-periods.

Capabilities moved: none, speed only.
