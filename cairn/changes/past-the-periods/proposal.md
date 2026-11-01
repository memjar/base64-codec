---
cairn: change
id: past-the-periods
status: draft
created: 2026-11-01
---

# Past the periods

The handoff of the session of 2026-11-01 (log 2026-11-01-a-closer-look), written for a fresh context to take over the optimisation without its history: where things stand, how to measure without fooling yourself, where the time goes now, what was tried, and what is left.

## Why

The user's goal: base64-codec should beat encodify 1.0.2 in every section of benches/compare.rs with the most concise code, and each pass should do better than the one before it in speed and in size. On the development machine (AMD Ryzen AI Max+ 395, Zen 5, AVX-512 VBMI detected) it is ahead in every section; on AVX2 alone, encodify's instructions, it is ahead on decoding and on wrapped encoding (1.39x at 4 KiB) and level on unwrapped encoding only, where both run the same 11-operation kernel. This pass made wrapped input, the MIME case, faster (decoding on AVX-512 and AVX2, encoding on AVX2, SSSE3 and scalar), but grew the code by about 11%: the next pass owes the size back.

Settled with the user on 2026-10-31: the SSSE3 backend stays (encodify has SSSE3 kernels), and CPU detection stays the application's (`Base64Cpu`).

## Where things stand

Cycles per call (`perf stat`, core 13, its sibling idle, averaged over four output offsets); the log of 2026-11-01-a-closer-look has the tree before that session and the instruction counts:

| workload | AVX-512 | AVX2 | SSSE3 | scalar | encodify |
| --- | --- | --- | --- | --- | --- |
| encode unwrapped, 64 B | 37.6 | 42.8 | 57.8 | 104.3 | 44.1 |
| encode unwrapped, 4 KiB | 162.1 | 577.6 | 1,176 | 3,100 | 578.6 |
| encode wrapped, 64 B | 45.2 | 59.7 | 62.4 | 95.5 | 109.2 |
| encode wrapped, 4 KiB | 313.3 | 670.3 | 1,218 | 3,057 | 931.0 |
| decode unwrapped, 64 B | 37.9 | 43.0 | 55.1 | 100.1 | 45.5 |
| decode unwrapped, 4 KiB | 195.1 | 605.9 | 1,233 | 3,813 | 693.2 |
| decode wrapped, 64 B | 86.1 | 90.7 | 92.2 | 127.3 | 118.2 |
| decode wrapped, 4 KiB | 387.2 | 714.7 | 1,341 | 3,828 | 1,064.6 |

Criterion medians are in benches/README.md.

The code: src/driver.rs stamps per backend `decode_run` and `encode_run` (dispatchers over `decode_blocks` and `encode_blocks`, unwrapped runs), `decode_lines` and `encode_lines` (dispatchers on the line ending over `decode_lines_of` and `encode_lines_of`, line by line, which call `decode_periods` and `encode_periods` when there are lines enough for a period; each period function calls the line function back for the lines past its whole periods). `base64_period!` writes a period's 19 blocks out with their index a constant. The decode kernels take `&[u8; C]` and a `split` (characters past it come `C` less a block further, a line ending skipped), the encode kernels `&mut [_; N]` and a `split` (characters past it written `N` less a block further); `stitch` is the byte mask table of the AVX2, SSSE3 and NEON decode kernels, `spread` the quantum mover of every encode kernel but AVX-512's.

## How to measure

- Cycles and instructions per call, not time: `perf stat -e cycles:u,instructions:u` (from `nix-shell -p perf`) over a binary running one call N times, at two values of N, the difference over the difference of N, pinned with `taskset` to a core whose SMT sibling is idle (cpu N and N+16 are siblings; check with `ps -eo psr,pcpu,comm` that no long job, Miri or a fuzzer, sits on either).
- The harness: a scratch crate depending on the tree by path, on a copy of the tree before the change (package renamed `base64-codec-before`, from `git archive HEAD src Cargo.toml`) and on encodify `=1.0.2`, with a `count <lib> <op> <wrapped> <size> <iters> <src offset> <dst offset>` mode, buffers at those offsets from a 4096-byte boundary, the output checked against a reference after the loop. Build it with `RUSTFLAGS='-C llvm-args=-align-all-functions=6'` (code placement alone once doubled a 64-byte decode). Average every workload over several output offsets (0, 1056, 2112, 3104 here): wrapped workloads move by up to 30% with the output's offset alone, and in a different direction for each tree, so one offset can show a gain that is not there.
- Instruction attribution: `perf record -e cycles:u` and `perf annotate` work on every backend; the AMD counters `de_dispatch_stall_cycle_dynamic_tokens_part1.store_queue_rsrc_stall`, `de_dispatch_stall_cycle_dynamic_tokens_part2.retq`, `ex_no_retire.load_not_complete` and `ls_bad_status2.stli_other` told stores, retirement and load stalls apart this session; Valgrind still has no AVX-512. The assembly with `objdump -d -M intel -C`, each stamped function a symbol of its own (`base64_codec::avx512::decode_periods::<66, 1248>`).
- Checks off the dev shell: aarch64 clippy and tests under qemu need fenix's aarch64-unknown-linux-musl rust-std with qemu-aarch64 as runner and rust-lld as linker, and Miri needs fenix's nightly with miri and rust-src; both from the flake.lock revisions of fenix and nixpkgs through `builtins.getFlake`, in a throwaway shell.nix. Miri with `+avx2` on the kernels takes about 40 minutes.

## Where the time goes now

- AVX2 unwrapped encoding: 11 vector operations per 24-byte block, as encodify, 3.15 cycles per block against 2.75 for four pipes. Untouched this session.
- AVX-512 wrapped encoding: bound by its stores (the store queue full 40% of its cycles at 4 KiB), not its 3 permutes per block; a `vpermb` and one store per crossing block, or two plain stores, measured the same as the masked store.
- AVX-512 and AVX2 wrapped decoding: periods run near 20 instructions a line against 42 before; the line by line tail after the whole periods and the period's 16 or 8 line-ending checks are what is left.
- Scalar unwrapped decoding: load-bound at 3,770 cycles when its codegen is good, 4,466 when LLVM hoists a group's lookups above its stores and spills them, which depends on how rustc splits the crate into codegen units.

## Levers left

Ordered by expected gain, each to be measured before it is kept:

1. Size: the period code, the kernels' stitch and split and `spread` cost about 340 lines. Candidates: the SSSE3 and scalar decode stitch only exists to compile `decode_periods`, which they never run (a macro parameter naming whether a backend has periods would drop it); the SSSE3 and AVX2 twins (`sextets`, `ascii`, the decode tables); `decode_lines_of` and `encode_lines_of` against the old per-line loops they replaced.
2. Scalar decoding robust to codegen units: lookups whose stores LLVM cannot reorder, or the 16 bytes of a block loaded as two words and extracted by shifts (fewer loads on a load-bound loop).
3. AVX2 unwrapped encoding below encodify's kernel cost: a translation or split one operation shorter, or two blocks ordered by hand.
4. AVX-512 wrapped encoding: fewer store bytes per line (stores are the bound), for instance a crossing block's characters written once by a `vpermt2b` with the line ending, and its two pushed-out characters by the next block.
5. Small inputs: the one-shot decode's glue, and the encoder's fixed cost per call (base64-simd 0.8 encodes 64 unwrapped bytes faster than the AVX-512 backend in benches/README.md).
6. NEON measured on hardware: never done; it now decodes in periods, stitching with four `vbsl` per block, and encodes them with `spread`, all under qemu only.

## Traps

- Instruction count is the bound of wrapped decoding on Zen 5 (IPC near 8 in the loops of this session): position bookkeeping per block or per line costs more than the blocks it saves, which is why the period's places are compile-time constants.
- LLVM rolls 19 identical block bodies back into a loop, and leaves a large `#[inline]` function called from two places out of line (the scalar line loop did, 26 instructions a call): write blocks out with `base64_period!`, give a large helper one call site.
- A dispatcher dividing by a length chosen at run time costs a `div`: keep the line's length a constant generic (`SPAN`) in the function that divides.
- A function that calls out and returns afterwards saves registers on every path: make the call a tail call, as `decode_lines_of` and `encode_lines_of` call the periods, so that a call too short for a period saves none.
- AMD's masked stores before Zen 4 (`vpmaskmovd`) are microcoded: AVX2 splits its crossing block with `spread`, 6% slower on Zen 5 than `vpmaskmovd`, as AVX2 is the backend of Zen 2 and Zen 3.
- An overlapping last period redoes up to a period less a line: wasteful just above one period (AVX2 decoding at 600 bytes, 31%).
- The traps of a-closer-look and further-speed-levers still hold: masked AVX-512 accesses whose masked-off bytes cross into the next page, `encode_groups` not inlined into a 3-byte caller, runtime-offset `fill` and `copy_within` becoming library calls, the eager AVX2 test per block, multipliers by powers of two hidden behind `opaque`, Valgrind without AVX-512, the fuzz crate's nightly, aarch64 under qemu.
