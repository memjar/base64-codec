---
cairn: change
id: a-closer-look
status: draft
created: 2026-11-01
---

# A closer look at the speed

The handoff of the session of the night of 2026-10-31 (log 2026-11-01-a-fresh-look), written for a fresh context to take over the optimisation without its history: where things stand, how to measure them without fooling yourself, where the time goes now, what was tried, and what is left.

## Why

The user's goal: base64-codec should beat encodify 1.0.2 in every section of benches/compare.rs with the most concise code, and each pass should do better than the one before it in speed and in size. On the development machine (AMD Ryzen AI Max+ 395, Zen 5, AVX-512 VBMI detected) it is ahead in every section; on AVX2 alone, encodify's instructions, it is now ahead on decoding (11% fewer cycles at 4 KiB unwrapped, 25% wrapped) and wrapped encoding, and level on unwrapped encoding only, where both run the same 11-operation kernel. The size did not move: 2,618 lines before the tests, 1,630 of code, against 2,578 and 1,629.

Settled with the user on 2026-10-31: the SSSE3 backend stays (encodify has SSSE3 kernels; dropping it would cost SSSE3-only CPUs 2.6x to 3.1x), and CPU detection stays the application's (`Base64Cpu`).

## Where things stand

Cycles per call (`perf stat`, core 13, its sibling idle); the log of 2026-11-01-a-fresh-look has the tree before that session and the instruction counts:

| workload | AVX-512 | AVX2 | SSSE3 | scalar | encodify |
| --- | --- | --- | --- | --- | --- |
| encode unwrapped, 64 B | 35.5 | 42.0 | 57.0 | 99.8 | 47.1 |
| encode unwrapped, 4 KiB | 126.9 | 572.2 | 1,174 | 3,082 | 573.7 |
| encode wrapped, 64 B | 44.2 | 57.4 | 60.8 | 88.7 | 107.2 |
| encode wrapped, 4 KiB | 284.4 | 814.9 | 1,386 | 3,270 | 921.7 |
| decode unwrapped, 64 B | 44.0 | 49.0 | 60.0 | 97.4 | 46.9 |
| decode unwrapped, 4 KiB | 187.7 | 610.3 | 1,229 | 3,776 | 685.7 |
| decode wrapped, 64 B | 84.3 | 90.7 | 91.5 | 126.7 | 110.5 |
| decode wrapped, 4 KiB | 415.5 | 814.8 | 1,352 | 3,794 | 1,092.5 |

Criterion medians are in benches/README.md.

The code: src/driver.rs stamps the loops of a backend from one decode and one encode kernel, generic over the length of their output or window (`decode_run` and `encode_run` dispatching to `decode_blocks` and `encode_blocks`, `decode_lines`, `encode_lines`); `Bad` tells whether a block holds a character outside the alphabet (`any`), where (`first`), and joins with others unless the backend tests each block apart (`EAGER`, AVX2 only); src/avx512.rs adds `decode_short` and `encode_short`, one masked block for runs shorter than a block; src/scalar.rs holds the lenient steps and the table kernels (`QUANTA`, `PAIRS`); src/decode.rs the decoder's loop and the one-shot fast path.

## How to measure

- Cycles and instructions per call, not time, the machine being shared and its clock moving: `perf stat -e cycles:u,instructions:u` (from `nix-shell -p perf`) over a binary running one call N times, at two values of N, the difference over the difference of N, pinned with `taskset` to a core whose SMT sibling is idle (cpu N and N+16 are siblings; sample /proc/stat first).
- The harness, a scratch crate depending on the tree by path, on a copy of the tree before the change (package renamed `base64-codec-before`) and on encodify `=1.0.2`, with a `count <lib> <op> <wrapped> <size> <iters>` mode and every buffer at a fixed offset from a 4096-byte boundary. Build it with `RUSTFLAGS='-C llvm-args=-align-all-functions=6'`: without it, the copy of the tree before ran its AVX2 64-byte decode at 110 cycles against 52 for the identical code of the tree, from op-cache misses (`de_src_op_disp.x86_decoder` and `de_no_dispatch_per_slot.no_ops_from_frontend` show it); with it both read 52. A per-backend table of every workload at 64 B and 4 KiB takes about two minutes.
- Instruction attribution: `valgrind --tool=callgrind` on the AVX2 or SSSE3 path (Valgrind has no AVX-512), `callgrind_annotate --inclusive=no`, summed per function; the assembly with `nix-shell -p binutils --run 'objdump -d -M intel -C --disassemble=<symbol>'`, each stamped loop a symbol of its own (`base64_codec::avx2::decode_blocks`).
- Zen 5 ports from uops.info, measured: `vpshufb`, `vpermd`, `vpermb`, `vpermi2b` and `vpmovb2m` on FP1 and FP2; shifts by an immediate on FP2 and FP3; `vpmaddubsw`, `vpmaddwd` and `vpmullw` on FP0 and FP1; simple integer operations on all four; `vptest` and `kortestq` take an FP45 slot too; masked 512-bit stores 1 per cycle. llvm-mca has no Zen 5 model (`znver5` uses Zen 4's tables and predicts 3.8 cycles for an AVX2 encode block measured at 3.15).
- /tmp is a tmpfs filled to the last megabyte by other sessions' scratchpads: put cross, fuzz and Miri target directories under the repository's ignored target/, and point `TMPDIR` there too, as rustdoc links the doctests in it and a full /tmp fails them with a linker bus error.

## Where the time goes now

- AVX2 unwrapped encoding: 11 vector operations per 24-byte block, the spread (1), the split (5: two `vpmullw`, a shift, an `and`, an `or`) and the translation (5: `vpsubusb`, `vpcmpgtb`, `vpsubb`, `vpshufb`, `vpaddb`), 3.15 cycles per block against 2.75 for four pipes. encodify runs the same.
- AVX2 decoding: 12 vector operations per 32-character block and its `vptest`, near the 3 cycles four pipes allow.
- AVX-512: decoding 4 shuffles and multiplies per 64 characters on FP0 to FP2, encoding 3 permutes per 48 bytes on FP1 and FP2, both near their pipes' bound from 4 KiB up. Wrapped decoding spends 5.8 cycles per 78-byte line against about 2.7 for its two blocks.
- Small inputs: the one-shot decode keeps about 110 instructions of glue in `decode_all` (the trim, the call to the run and the registers it saves, the tail and its finish), the run's dispatcher and blocks about 150 more with AVX2.

## Levers left

Ordered by expected gain, each to be measured before it is kept:

1. AVX2 unwrapped encoding, the one section level with encodify: a translation or split one operation shorter (none found: the `vpmulhuw` split lost the loop's schedule, 1,008 cycles), or two blocks' operations ordered by hand.
2. AVX-512 wrapped decoding: two lines in three blocks, the 12 last characters of one line and the first 52 of the next gathered by `vpermt2b`, or the line break taken out by `vpcompressb` (VBMI2).
3. Wrapped encoding on AVX2 and SSSE3: one window per line, checked once per line, the exact kernel for the first and last lines only, saving an operation per block (the window checked per block against the whole input does not fold: AVX-512 wrapped 4 KiB went 284 to 352 cycles).
4. Small inputs: the trim, the run and the tail of a one-shot decode in one stamped function, so that `decode_all` saves no register around the call. base64-simd 0.8 encodes 64 unwrapped bytes faster than the AVX-512 backend (7.74 against 7.03 GiB/s in benches/README.md), and the base64 crate faster than the scalar one (3.00 against 2.68): the encoder's fixed cost per call is the target there.
5. NEON measured on hardware: never done; it has the merged kernels and `first` again, its short runs go to the scalar table kernels.
6. Lines: the SSSE3 and AVX2 twins (`split`, `ascii`, `opaque`, the decode tables, about 60 lines each) are the largest duplication left; a macro needs every intrinsic as a parameter without `paste`, which the first session found longer than what it shared.

## Traps

- A masked AVX-512 load or store whose masked-off bytes cross into the next page costs about ten cycles: a block ending exactly at a buffer's end does, so exact blocks keep plain 32 and 16-byte accesses and only the short blocks are masked. A store whose mask waits on the block's validity serialises a small input: the short decode branches to the scalar quanta on a bad character instead.
- LLVM does not inline `encode_groups` into a 3-byte caller (`encode_tail` through it cost 20 cycles per call), and a `fill` of 1 or 2 bytes at a runtime offset becomes a `memset` call. When `join` keeps the last block for an eager backend, LLVM tests it a second time after the group: break out of the group instead.
- Testing each block costs AVX-512 (`vpmovb2m` and `kortestq` on its busiest pipes) and scalar; AVX2's `vptest` takes the place of two vector operations. Keep the per-backend `EAGER`.
- The traps of further-speed-levers still hold: multipliers by powers of two hidden behind `opaque`, kernels `#[inline]`, the dispatchers reaching the narrower loop before saving registers, `decode_bytes` handing back past the trimmed end, Valgrind without AVX-512, the fuzz crate's nightly, aarch64 checks under qemu-aarch64 linked with rust-lld.
