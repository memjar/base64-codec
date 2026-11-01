---
cairn: change
id: the-last-glue
status: draft
created: 2026-11-01
---

# The last glue

The handoff of the session of 2026-11-01 (log 2026-11-01-past-the-periods), written for a fresh context to take over the optimisation without its history: where things stand, how to measure without fooling yourself, where the time goes now, what was tried, and what is left.

## Why

The user's goal: base64-codec should beat encodify 1.0.2 in every section of benches/compare.rs with the most concise code, each pass doing better than the one before in speed and in size, with inline comments kept to the important ones (the `SAFETY` comments stay: the crate denies `clippy::undocumented_unsafe_blocks`). On the development machine (AMD Ryzen AI Max+ 395, Zen 5, AVX-512 VBMI detected) it is ahead in every section; on AVX2 alone, encodify's instructions, it is ahead everywhere but unwrapped encoding at 4 KiB, where both run the same 11-operation kernel (1.01x), and unwrapped decoding at 64 bytes leads by 3% only. This pass made small calls and wrapped input faster and the code 5% shorter, 2% without comments.

Settled with the user: the SSSE3 backend stays (encodify has SSSE3 kernels); CPU detection stays the application's (`Base64Cpu`); decoding keeps one entry point with no wrapped option (asked on 2026-11-01): the decoder finds wrapped input itself by a look at character 76, which costs a cycle, whereas an option could only be a hint, both paths staying for inputs that do not match it. Encoding has its option already, with a path of its own for each.

## Where things stand

Cycles per call (`perf stat`, core 13, its sibling idle, averaged over four output offsets); the log of 2026-11-01-past-the-periods has the tree before that session and the instruction counts:

| workload | AVX-512 | AVX2 | SSSE3 | scalar | encodify |
| --- | --- | --- | --- | --- | --- |
| encode unwrapped, 64 B | 31.9 | 37.5 | 53.7 | 97.9 | 42.9 |
| encode unwrapped, 4 KiB | 158.2 | 572.3 | 1,175 | 3,091 | 578.1 |
| encode wrapped, 64 B | 43.6 | 55.7 | 60.9 | 92.8 | 110.9 |
| encode wrapped, 4 KiB | 291.4 | 678.8 | 1,222 | 3,077 | 941.5 |
| decode unwrapped, 64 B | 37.8 | 43.2 | 56.2 | 98.2 | 44.7 |
| decode unwrapped, 4 KiB | 194.5 | 608.2 | 1,234 | 3,820 | 695.5 |
| decode wrapped, 64 B | 71.7 | 69.4 | 74.6 | 108.3 | 115.3 |
| decode wrapped, 4 KiB | 330.5 | 667.4 | 1,301 | 3,804 | 1,043.5 |

Criterion medians are in benches/README.md.

The code: src/decode.rs `decode_all` looks at character 76: with no line ending there, one run over the trimmed input and the tail, the usual unwrapped input; otherwise the decoder's loop, which at a line start with a line ending at character 76 calls `Loops::decode_lines[eol - 1]`, then runs, line breaks and the byte-by-byte rules. src/driver.rs stamps per backend `decode_run` and `encode_run` (dispatchers over `decode_blocks` and `encode_blocks`), `decode_lines` and `encode_lines` (line by line, calling `decode_periods` and `encode_periods` when there are lines enough for a period of 19 blocks, which call them back for the lines past their whole periods). A decode line loop writes spare bytes past each block, but past its last line only when they fit, else that line goes through `decode_blocks`' exact blocks. The encode kernels write the line ending at `split` when given room, `split` past the block meaning no line ends in it: AVX-512 with the line ending blended into its masked store, the others in `spread`.

## How to measure

- Cycles and instructions per call, not time: `perf stat -e cycles:u,instructions:u` (perf from `nix-shell -p perf`) over a binary running one call N times, at two values of N, the difference over the difference of N, pinned with `taskset` to a core whose SMT sibling is idle (cpu N and N+16 are siblings). Check with `ps -eo psr,pcpu,comm` that no long job sits on either, and start Miri and the fuzzers pinned away from it (`taskset -c 2-11 nix-shell ...`), as a process pinned afterwards leaves its children free.
- The harness: a scratch crate depending on the tree by path, on a copy of the tree before the change (package renamed `base64-codec-before` from `git archive HEAD src Cargo.toml`, its `[[bench]]` section dropped) and on encodify `=1.0.2`, with a `count <lib> <op> <wrapped> <size> <iters> <src offset> <dst offset>` mode, buffers at those offsets from a 4096-byte boundary, the output checked against a reference after the loop. Build it with `RUSTFLAGS='-C llvm-args=-align-all-functions=6 -C llvm-args=-align-loops=64'`: with functions aligned alone, a layout change moved SSSE3 wrapped decoding at 64 bytes from 77 to 142 cycles with no code change in it. Average every workload over several output offsets (0, 1056, 2112, 3104 here, all multiples of 32, which hides the cost of an output 16 bytes off a cache line: add 16 and 48), and sweep source offsets for wrapped decoding: AVX-512 at 4 KiB reads 314 to 374 cycles with the source's offset alone.
- Instruction attribution: callgrind (`nix-shell -p valgrind`) gives exact counts per function and per source line on AVX2 and below (`callgrind_annotate --inclusive=yes`, then `--auto=yes` on a file); `perf record -e cycles:u` and `perf annotate` on every backend; the AMD counter `ex_no_retire.load_not_complete` flags the layout-dependent stalls. The assembly with `objdump -d -M intel -C` (from `nix-shell -p binutils`), each stamped function a symbol of its own (`base64_codec::avx512::decode_periods::<66, 1248>`).
- Checks off the dev shell: aarch64 clippy and tests under qemu need fenix's aarch64-unknown-linux-musl rust-std with qemu-aarch64 as runner and rust-lld as linker, and Miri needs fenix's nightly with miri and rust-src; both from the flake.lock revisions of fenix and nixpkgs through `builtins.getFlake`, in a throwaway shell.nix. Miri takes about 7 minutes on the library and 40 with `+avx2` on the kernels.

## Where the time goes now

- Small unwrapped decoding (AVX-512 about 38 cycles at 64 bytes, 263 instructions): `decode_blocks` entered for two blocks pays its fixed costs (three saved registers, five constant loads, the group count, bounds checks per block), and the look at character 76 a cycle, which leaves this row a cycle behind the tree before.
- AVX-512 wrapped decoding at 4 KiB: the periods take 63% of the cycles, 2.75 cycles a block against 2.25 for an unwrapped block (the second load and the blend); the line loop over the lines past the last period 13%.
- AVX-512 wrapped encoding at 4 KiB: still its stores, 19 plain and 19 masked per period of 16 lines, 3 of the masked ones writing a line ending the next block overwrites.
- AVX2 unwrapped encoding: 11 vector operations per block, as encodify, 3.35 cycles per block against 2.75 for four pipes.
- Scalar decoding: 8 loads per quantum (4 characters, 4 lookups) and 2.5 operations a quantum placing its bad flag in the block's mask.

## Levers left

Ordered by expected gain, each to be measured before it is kept:

1. AVX-512 stores aligned to the output's cache lines: unwrapped encoding at 4 KiB reads 151 cycles on an output aligned to 64 bytes and 175 to 185 on one 16 bytes off, as malloc often returns it; a first block overlapping the next could bring the following stores to the output's 64-byte boundaries, for decoding as for encoding.
2. Small calls: one or two blocks without `decode_blocks`' fixed costs (a dispatcher case, or a narrow run taking up to two blocks), which would also pay for the look at character 76.
3. AVX-512 wrapped encoding: output assembled at its own 64-byte positions, each vector a `vpermt2b` of two encoded blocks with the line ending blended in, one store per 64 bytes of output (19.5 per period against 38); it needs 39 index vectors per two periods, which costs size.
4. AVX2 unwrapped encoding below encodify's kernel cost: no shorter split or translation is known; two blocks scheduled by hand, or a translation sharing work between two blocks.
5. Scalar decoding: robust to codegen units, and fewer operations around the loads, for instance a bad flag joined without placing it per quantum, its position found again on failure.
6. Size: the SSSE3 and AVX2 twins (decode tables, `sextets`, `ascii`), the decode stitch of SSSE3 and scalar compiled only for periods they never run, `decode_all`'s fast path against the decoder's loop.
7. NEON measured on hardware: never done; it decodes and encodes in periods, under qemu only.

## Traps

- Code layout moves results by up to 2x with no code change: align loops as well as functions in the harness, compare instruction counts, and suspect layout when cycles move against the instructions (`ex_no_retire.load_not_complete` up). The tree before's AVX-512 wrapped encoding at 64 bytes reads 54 to 56 cycles in some builds, 44.7 to 45.9 in others.
- The spare bytes of a decode line loop: within `decoded_len_max`, an input of fewer than 11 lines has no room for AVX-512's 16 spare bytes past its last line. A choice of spare or exact per block at run time costs SSSE3 and scalar 11% and 25%, and sending every last line through `decode_blocks` costs small AVX2 inputs 15%: only the last line, and only when the spare bytes do not fit.
- The one-shot decode needs its single run: through the decoder's loop alone it costs 77 instructions a call. Detecting wrapped input from where that run stops, rather than by the look at character 76, gives back the wrapped gains.
- Scalar 32-bit loads with byte extraction lose to byte loads on Zen 5.
- The traps of past-the-periods still hold: instruction count bounds wrapped decoding (IPC near 8); LLVM rolls the 19 blocks of a period back into a loop unless `base64_period!` writes them out; a large `#[inline]` function with two callers stays out of line; a runtime divisor costs a `div`; a function calling out and returning saves registers on every path, hence the tail calls to the periods; AMD's masked stores before Zen 4 are microcoded; an overlapping last period redoes lines; masked AVX-512 accesses crossing into the next page cost about ten cycles; Valgrind has no AVX-512; the fuzz crate needs nightly; aarch64 runs under qemu.
