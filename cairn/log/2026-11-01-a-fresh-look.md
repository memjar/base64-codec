---
cairn: log
change: a-fresh-look
landed: 2026-11-01
---

# A fresh look at the speed

Took over the optimisation from the handoff of further-speed-levers, with its measuring method: `perf stat` cycles and instructions per call over two iteration counts, interleaved, against a copy of the tree before the session and encodify 1.0.2, the harness now built with `-C llvm-args=-align-all-functions=6` (see the traps of a-closer-look). Every workload is now ahead of the tree before the session or level with it, and ahead of encodify on every one of its sections but unwrapped encoding on AVX2, where both crates run the same 11 vector operations per block.

What landed:

- The x86 decode kernels one operation shorter than encodify's: `/`, the one character whose offset its high nibble does not give, now takes the offset of `+` and lands on 66, and the sextets are clamped to 63 with `vpminub`, which replaces the `vpcmpeqb` and the `vpaddb` that picked its offset. The SSSE3 kernel also looks its low-nibble table up with the characters as they are, as the AVX2 one does, an `and` fewer.
- AVX2 tests each decode block with one `vptest` of its two class vectors (`Bad::EAGER`), where joining the blocks of a group cost an `andnot` and an `or` per block; on Zen 5 the vector pipes bound that loop, so an operation per block is 7%. The other backends still join a group's blocks and test them once: testing each block cost AVX-512 31% (191 against 251 cycles at 4 KiB) and scalar 3%.
- One decode and one encode kernel per backend, generic over the length of the output or of the window they take and checked by a `const` assertion, in place of the wide and exact pairs; the exact blocks of a run and its overlapping last block in one loop.
- AVX-512 runs shorter than one block in one block whose load and store are masked to them (`decode_short`, `encode_short`), in place of the AVX2, SSSE3 and scalar chain; a short decode that meets a character outside the alphabet goes quantum by quantum, so that the length it returns waits on no mask. Exact blocks keep plain loads and stores.
- The one-shot decode's usual end, at most three symbols before padding and blanks, folded into a local state and finished without building a decoder; `trimmed` no longer tracks the padding, which the resumable decoder reads from what it trimmed when it folds a tail.
- The scalar kernels on tables: decoding by four 256-entry tables of each byte's sextet already in place for its position in a quantum, as modp_b64 does, four lookups joined giving the quantum's three bytes and flagging a byte outside the alphabet in the fourth, its block marking each bad quantum in a byte of its own; encoding by a 4,096-entry table of character pairs, two lookups per group.
- `BASELINE` and `detect` picking the widest x86 backend through one `widest` function, and the low-nibble tables written as the complement of SSSE3's bad classes, which LLVM folds.

Tried and left out:

- Every block tested in a loop of its own, without groups: AVX2 710 cycles at 4 KiB, its bounds checks 15 scalar instructions per block; groups whose blocks are tested each, 607.
- AVX-512 blocks with masked stores and loads throughout (spare 0, window 48): 4 KiB decoding 189 to 251 cycles, encoding 129 to 139. Masked exact blocks alone still cost 14 and 5 cycles per call: the masked-off bytes of the block ending a buffer cross into the next page.
- `Bad::first` dropped, a failing block decoded again by the narrower loop: the first line of wrapped input then went quantum by quantum, wrapped decoding at 64 bytes 88 to 125 cycles. Kept, the scalar block marking each quantum, which costs nothing measurable.
- AVX2 encoding split by `vpmulhuw` as Muła writes it rather than by `vpmullw` and a shift: 572 to 1,008 cycles, the loop no longer scheduled.
- Encode steps of 4 or 16 blocks rather than 8: level at 4 KiB, 4 lost 25 cycles at 64 bytes.
- Line encoding through windows of the whole input, lever 4 of a-fresh-look: the window checks per block do not fold, wrapped AVX-512 encoding at 4 KiB 284 to 352 cycles.
- `encode_tail` through `encode_groups`, 10 lines fewer: LLVM calls `encode_groups` rather than inlining it, 20 cycles more per call at 64 bytes; a `fill` of the 1 or 2 padding bytes at a runtime offset became a `memset` call.
- The SSSE3 backend dropped, 254 lines: the user chose to keep it, encodify 1.0.2 having SSSE3 kernels too, so that SSSE3-only CPUs keep their 2.6x to 3.1x over scalar.

Cycles per call, `perf stat` on core 13 with its sibling idle, the tree before the session in brackets:

| workload | AVX-512 | AVX2 | SSSE3 | scalar | encodify |
| --- | --- | --- | --- | --- | --- |
| encode unwrapped, 64 B | 35.5 (35.5) | 42.0 (41.3) | 57.0 (57.7) | 99.8 (148.8) | 47.1 |
| encode unwrapped, 4 KiB | 126.9 (131.2) | 572.2 (572.5) | 1,174 (1,174) | 3,082 (5,700) | 573.7 |
| encode wrapped, 64 B | 44.2 (60.6) | 57.4 (61.1) | 60.8 (66.3) | 88.7 (133.8) | 107.2 |
| encode wrapped, 4 KiB | 284.4 (284.0) | 814.9 (815.0) | 1,386 (1,386) | 3,270 (6,048) | 921.7 |
| decode unwrapped, 64 B | 44.0 (47.0) | 49.0 (51.9) | 60.0 (67.0) | 97.4 (156.9) | 46.9 |
| decode unwrapped, 4 KiB | 187.7 (190.0) | 610.3 (692.6) | 1,229 (1,415) | 3,776 (6,454) | 685.7 |
| decode wrapped, 64 B | 84.3 (103.2) | 90.7 (100.5) | 91.5 (108.9) | 126.7 (179.0) | 110.5 |
| decode wrapped, 4 KiB | 415.5 (421.4) | 814.8 (921.5) | 1,352 (1,501) | 3,794 (6,359) | 1,092.5 |

The library is level in size: 2,618 lines before the tests against 2,578, 1,630 without comments and blank lines against 1,629. The scalar tables, the masked AVX-512 blocks and the eager tests added about what the merged kernels and the shorter tables took out.

Checked: the test suite in release, in debug with all features, without default features and with `-C target-cpu=native` (AVX-512 as `BASELINE`); clippy with `-D warnings` for all targets on x86_64 and aarch64; missing docs and rustdoc links; the NEON backend under qemu-aarch64, built for aarch64-unknown-linux-musl, not on hardware; Miri on the library and, with `-C target-feature=+avx2`, on the kernels; the decode and encode fuzz targets for 5 minutes each.

The levers left, with the traps met this time, went to a-closer-look.

Capabilities moved: none, speed only.
