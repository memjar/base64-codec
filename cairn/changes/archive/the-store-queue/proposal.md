---
cairn: change
id: the-store-queue
status: landed
created: 2026-11-01
---

# The store queue

The handoff of the session of 2026-11-01 (log 2026-11-01-the-last-glue), written for a fresh context to take over the optimisation without its history: where things stand, how to measure without fooling yourself, where the time goes now, what was tried, and what is left.

## Why

The user's goal: base64-codec should beat encodify 1.0.2 in every section of benches/compare.rs with the most concise code, each pass doing better than the one before in speed and in size, with inline comments kept to the important ones (the `SAFETY` comments stay: the crate denies `clippy::undocumented_unsafe_blocks`), searching for new methods where needed. On the development machine (AMD Ryzen AI Max+ 395, Zen 5, AVX-512 VBMI detected) it is ahead in every section; on AVX2 alone, encodify's instructions, it is ahead everywhere, unwrapped encoding at 4 KiB by 5% (both run the same 11 operations a block), unwrapped decoding at 64 bytes by 1% only.

Settled with the user: the SSSE3 backend stays (encodify has SSSE3 kernels); CPU detection stays the application's (`Base64Cpu`); decoding keeps one entry point with no wrapped option: the one-shot decode looks at character 76 and decodes whole lines from the first one, the decoder's loop tries whole lines wherever its runs stop. Helpers with one caller are folded into it (the user's rule).

## Where things stand

Cycles per call (`perf stat`, core 13, its sibling idle, averaged over eight source and output offset pairs); the log of 2026-11-01-the-last-glue has the tree before that session and the instruction counts:

| workload | AVX-512 | AVX2 | SSSE3 | scalar | encodify |
| --- | --- | --- | --- | --- | --- |
| encode unwrapped, 64 B | 32.3 | 37.3 | 46.7 | 85.1 | 43.1 |
| encode unwrapped, 4 KiB | 133.3 | 551.7 | 1,162 | 3,108 | 579.0 |
| encode wrapped, 64 B | 43.0 | 55.5 | 61.1 | 91.8 | 110.7 |
| encode wrapped, 4 KiB | 289.4 | 669.1 | 1,217 | 3,070 | 940.7 |
| decode unwrapped, 64 B | 38.4 | 43.4 | 55.7 | 98.7 | 43.9 |
| decode unwrapped, 4 KiB | 201.9 | 609.5 | 1,230 | 3,819 | 693.6 |
| decode wrapped, 64 B | 56.9 | 55.2 | 59.2 | 89.0 | 115.0 |
| decode wrapped, 4 KiB | 337.6 | 653.6 | 1,288 | 3,758 | 1,063.1 |

AVX-512 wrapped encoding at 4 KiB spreads by up to 100 cycles within one offset pair: 281 in a run of five repeats per pair.

Criterion medians are in benches/README.md.

The code: src/decode.rs `decode_all` trims the input's end, then `lines_and_run` decodes whole lines when a line ending sits at character 76 and one run after them; the tail ends the usual input, anything else goes to the decoder's loop, which calls `lines_and_run` wherever its runs stop. src/driver.rs stamps per backend `decode_run` and `encode_run` (dispatchers over `decode_blocks` and `encode_blocks`), `decode_lines` and `encode_lines` (line by line, calling `decode_periods` and `encode_periods` when there are lines enough for a period of 19 blocks, which call them back for the lines past their whole periods). The encode kernels take `K` groups a group apart and an output each: `encode_blocks` gives them four blocks a step, from the first 64-byte boundary of the output when a block's 64 characters fill a cache line (AVX-512, NEON); AVX2 runs each of its operations across the four blocks before the next. Every other call gives one group.

## How to measure

- Cycles and instructions per call, not time: `perf stat -e cycles:u,instructions:u` (perf from `nix-shell -p perf`) over a binary running one call N times, at two values of N, the difference over the difference of N, pinned with `taskset` to a core whose SMT sibling is idle (cpu N and N+16 are siblings). Check with `ps -eo psr,pcpu,comm` that no long job sits on either, and start Miri and the fuzzers pinned away from it (`taskset -c 2-11 nix-shell ...`), as a process pinned afterwards leaves its children free.
- The harness: a scratch crate depending on the tree by path, on a copy of the tree before the change (package renamed `base64-codec-before` from `git archive HEAD src Cargo.toml`, its `[[bench]]` section dropped) and on encodify `=1.0.2`, with a `count <lib> <op> <wrapped> <size> <iters> <src offset> <dst offset>` mode, buffers at those offsets from a 4096-byte boundary, the output checked against a reference after the loop. Build it with `RUSTFLAGS='-C llvm-args=-align-all-functions=6 -C llvm-args=-align-loops=64'`. Average every workload over source and output offset pairs, 0:0, 0:1056, 0:2112, 0:3104, 528:2096, 1552:3600, 2576:624 and 3600:1616 here: the page distance between source and output moves a 4 KiB workload by up to 30% as their streams alias on the low 12 address bits (AVX-512 wrapped decoding 284 cycles with both at the same page offset, 363 to 370 with them 2 KiB apart), and AVX-512 stores run 15% slower 16 bytes off a cache line.
- Instruction attribution: callgrind (`nix-shell -p valgrind`) gives exact counts per function and per source line on AVX2 and below; `perf record -e cycles:u` and `perf annotate` on every backend; the AMD counters `de_dispatch_stall_cycle_dynamic_tokens_part1.fp_sch_rsrc_stall` (the FP scheduler full), `de_dispatch_stall_cycle_dynamic_tokens_part1.store_queue_rsrc_stall`, `ex_no_retire.load_not_complete` and `ls_bad_status2.stli_other` (a load blocked by an older store) told the bounds apart this session. The assembly with `objdump -d -M intel -C`, each stamped function a symbol of its own.
- Port bounds: an operation mix run in `asm!` loops in a scratch crate (`-C target-cpu=native`), independent operations against the real chains, tells a port bound from a scheduling one; the log has Zen 5's numbers.
- Checks off the dev shell: aarch64 clippy and tests under qemu need fenix's aarch64-unknown-linux-musl rust-std with qemu-aarch64 as runner and rust-lld as linker, and Miri needs fenix's nightly with miri and rust-src; both from the flake.lock revisions of fenix and nixpkgs through `builtins.getFlake`, in a throwaway shell.nix. Miri takes about 7 minutes on the library and 40 with `+avx2` on the kernels (`cargo miri test --lib kernels`).

## Where the time goes now

- AVX-512 wrapped encoding: 55 cycles a period of 16 lines (47 to 60 over eight page distances) against 21 for the same bytes unwrapped. 35 stores a period, 19 plain and 16 masked, the masked ones 2 bytes past a plain one; the store queue is full 32% of the cycles, loads wait 23% and conflict with older stores 14 times a call. A plain store of a `vpermb`-shifted vector in place of the masked one measured level.
- AVX-512 unwrapped decoding: at the machine's limit for its operations, 2 cycles a block for a hand-written loop as for the driver's; loads alone 1.41, stores alone 1.51.
- AVX2 unwrapped encoding: 11 operations a block, 3.0 cycles a block in registers a step at a time across four blocks, against 2.75 for its ports (`vpsrlw`, `vpsubusb`, the shuffles and the multiplies have 2 pipes each on Zen 5).
- Small calls: about 262 instructions to decode 64 bytes on AVX-512: the one-shot glue, `decode_blocks`' fixed costs (three saved registers, up to nine constant loads on AVX2) and about 14 instructions of bounds checks per single block.
- Scalar decoding: 8 loads per quantum and 2.5 operations a quantum placing its bad flag, unchanged.

## Levers left

Ordered by expected gain, each to be measured before it is kept:

1. AVX-512 wrapped encoding: find what holds the stores (35 a period for 1,248 bytes) and the loads behind them; or output assembled at its own 64-byte positions, each vector a `vpermt2b` of the two encoded blocks it draws from with the line ending taken from the index (the mask2 form), 20 stores a period instead of 35, 20 index vectors per period and line ending, about 60 lines of AVX-512 code. quoted-printable-codec's note that `vpermt2b` runs 0.8 a cycle on Zen 5 was a latency artifact of its loop: re-measured on 2026-11-02, `vpermt2b` and `vpermi2b` run 2 a cycle, as `vpermb` does. The cost model: 38 `vpermb` and 20 `vpermt2b` a period on the two permute pipes, about 29 cycles, the 19 multishifts beside them, 20 stores, against 55 cycles today and 21 unwrapped. 1,248 bytes are 19.5 cache lines (1,232 with LF are 19.25), so either a double period (CRLF) and a quadruple one (LF) to stay on cache lines, or a masked last vector and the output's alignment drifting by period: weigh table size against the 15% of stores off a cache line. A cheaper first try in the same spirit (quoted-printable-codec's read-ahead): issue the next blocks' loads before this block's stores, aimed at the loads that wait (23%) and the conflicts with older stores.
2. Small calls: one or two blocks without `decode_blocks`' fixed costs, and single blocks with fewer bounds checks (exact chunks and a separate last block measured fewer instructions but more cycles on AVX2). Since a-url-alphabet (log 2026-11-01-a-url-alphabet), the glue reading the codec costs about 20 instructions more a padded encode call and 8 a decode call (the line ending, alphabet and padding unpacked from one register, the loops picked by backend and alphabet, the padding branch of `encode_tail`), 1 to 3.5 cycles at 64 bytes: measure against that tree, not the table above.
3. AVX2 unwrapped decoding at 64 bytes, level with encodify (43.4 against 43.9 cycles).
4. Scalar decoding robust to codegen units, with fewer operations around its loads.
5. Size: the SSSE3 and AVX2 twins (decode tables), the decode stitch of SSSE3 and scalar compiled only for periods they never run, `encode_all` against the encoder's run.
6. NEON measured on hardware: never done; it decodes and encodes in periods and takes four groups a step, under qemu only.

## Traps

- Page distance: source and output streams alias on the low 12 address bits, moving a 4 KiB workload by up to 30% with no code change; average over source and output offset pairs, not output offsets from a page-aligned source.
- A step at a time across blocks helps chains of dependent operations that fill the scheduler (AVX2 encoding), not the decode kernels, whose lookups run in parallel already: AVX2 decoding two blocks at a time ran 2.5% slower, four spilled.
- The small paths are tight: a shared pass out of line cost 27 instructions (`#[inline(always)]` on `lines_and_run`), a runtime `step_by` 9 to 20, `encode_groups` for a 3-byte tail 55, a slice from a start the compiler cannot see is 0 about 8; check instruction counts at 64 bytes after any change near them.
- The one-shot calls are `Base64Codec` methods since a-codec-value; `encode` and `decode` stay `#[inline]`, as out of line the codec takes two argument registers and shifts every argument on the way to `encode_all` and `decode_all` (5 instructions a decode call). A measuring harness calls `Base64Codec { cpu, wrap }.encode` and `.decode` on this tree, the free functions on trees before it.
- qemu-aarch64's default `max` model crashed the release tests in safe scalar code on 2026-11-01, as did every model with FEAT_LSE2 tried: run the aarch64 tests with `QEMU_CPU=neoverse-n1`.
- AVX2's eager `Bad`, one `vptest` per block, beats a joined one by 9% at 4 KiB.
- The traps of the-last-glue still hold: layout moves results by up to 2x (align loops and functions, compare instruction counts, suspect layout when cycles move against them); a decode line loop writes spare bytes past its last line only when they fit; the one-shot decode needs its single pass; scalar 32-bit loads with byte extraction lose to byte loads on Zen 5; instruction count bounds wrapped decoding; LLVM rolls the 19 blocks of a period back into a loop unless `base64_period!` writes them out; a large `#[inline]` function with two callers stays out of line; a runtime divisor costs a `div`; a function calling out and returning saves registers on every path; AMD's masked stores before Zen 4 are microcoded; an overlapping last period redoes lines; masked AVX-512 accesses crossing into the next page cost about ten cycles; Valgrind has no AVX-512; the fuzz crate needs nightly; aarch64 runs under qemu.
