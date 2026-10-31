---
cairn: change
id: a-fresh-look
status: draft
created: 2026-10-31
---

# A fresh look at the speed

The handoff of the 2026-10-31 evening session (log 2026-10-31-further-speed-levers), written for a fresh context to take over the optimisation without its history. It says where things stand, how to measure them without fooling yourself, where the time goes now, what was tried and why it stayed or went, and what is left.

## Why

The goal set by the user: base64-codec should beat encodify 1.0.2 in every section of benches/compare.rs (encode and decode, unwrapped and CRLF-wrapped, 64 B to 10 MiB), with the most concise code. On the development machine (AMD Ryzen AI Max+ 395, Zen 5, with AVX-512 VBMI) it now does in every section, through the AVX-512 backend `Base64Cpu::detect` picks there. On AVX2 alone, the instructions encodify has, it is ahead on wrapped input and level on unwrapped input, both crates running the same kernels. The library did not get shorter: 2,576 lines before the tests against 2,377 for the tree before the session, of which the AVX-512 backend is 216.

## Where things stand

Cycles per call, `perf stat` on core 12 (its SMT sibling idle), interleaved runs; the log of 2026-10-31 has the instruction counts and the tree before the session:

| workload | AVX-512 | AVX2 | encodify 1.0.2 |
| --- | --- | --- | --- |
| encode unwrapped, 64 B | 35 | 41 | 46 |
| encode unwrapped, 4 KiB | 130 | 573 | 572 |
| encode wrapped, 64 B | 60 | 61 | 105 |
| encode wrapped, 4 KiB | 284 | 815 | 920 |
| decode unwrapped, 64 B | 47 | 51 to 52 | 45 to 47 |
| decode unwrapped, 4 KiB | 191 | 693 to 697 | 684 to 698 |
| decode wrapped, 64 B | 103 | 100 | 110 |
| decode wrapped, 4 KiB | 419 | 916 to 919 | 1,049 to 1,065 |

Criterion medians are in benches/README.md: with AVX-512 the crate is ahead in every section but unwrapped decoding at 64 bytes, level there with encodify's strict engine (5.96 against 5.92 GiB/s); with AVX2 alone it is ahead on wrapped input, level on unwrapped encoding and 2% to 8% behind on unwrapped decoding. The scalar backend decodes level with the tree before the session and encodes 4% to 8% slower unwrapped, 20% slower wrapped.

The code: src/driver.rs stamps four loops per backend (`decode_run` and `encode_run` as dispatchers over `decode_blocks` and `encode_blocks`, `decode_lines`, `encode_lines`) behind `Loops`; src/decode.rs holds the decoder's loop and the one-shot fast path; src/encode.rs the encoder's loops; src/scalar.rs the lenient byte steps, the end of input (`trimmed`, `decode_tail`), the scalar kernels and the quantum and group loops; one module per SIMD backend holds its kernels and `Bad`.

## How to measure

- Cycles and instructions per call, not time. The machine is shared: its clock moved between 3.7 and 4.8 GHz within minutes as other sessions loaded it, which shifts every time-based number. Count with `perf stat -e cycles:u,instructions:u` (from `nix-shell -p perf`; `perf_event_paranoid` is 2, user-space counting works) over a binary that runs one call N times, at two values of N, and take the difference over the difference of N: that cancels the setup. Pin with `taskset` to a core whose SMT sibling is idle (cpu N and N+16 are siblings; sample /proc/stat first, cores 8 to 15 were usually quiet).
- A harness is a scratch crate depending on the tree by path, on a copy of the tree before the change (package renamed), and on encodify `=1.0.2`, with a `count <lib> <op> <wrapped> <size> <iters>` mode, which runs `black_box(call)` that many times, and a timing mode. Its traps, both seen: the first workload of a process measures slow unless each measurement warms up first (200 ms); and buffer placement moves 64-byte results by up to 2x, so put every buffer at a fixed offset from a 4096-byte boundary, the same for every library (an offset of 48 slowed every library alike, an address artifact). Per call, with `$B` the harness binary:

  ```sh
  stat() { taskset -c 12 perf stat -x, -e instructions:u,cycles:u "$B" count "$@" 2>&1 >/dev/null | cut -d, -f1 | paste -sd' '; }
  read i1 c1 <<<"$(stat avx2 decode 0 64 100000)"
  read i2 c2 <<<"$(stat avx2 decode 0 64 1100000)"
  echo "$(( (i2 - i1) / 1000000 )) instructions, $(( (c2 - c1) / 1000000 )) cycles"
  ```
- Code placement alone moves a hot loop by up to 8% between two builds of the same code (the 4 KiB unwrapped decode measured 702 and 758 cycles from two binaries). Repeat runs, compare instruction counts too, and distrust a 2% win from one build.
- Instruction attribution: `valgrind --tool=callgrind` (from `nix-shell -p valgrind`), then `callgrind_annotate --inclusive=yes`, summed per function. Valgrind has no AVX-512: profile the shared code with `Base64Cpu::assume(Base64Backend::Avx2)`. Read the assembly with `nix-shell -p binutils --run 'objdump -d -M intel -C --disassemble=<symbol>'`; the four stamped loops of each backend are separate symbols (`base64_codec::avx2::decode_run` and so on).
- The final numbers come from `cargo bench --features std --bench compare` pinned to one core (benches/README.md), which now also runs the AVX2 backend apart when AVX-512 is detected, to compare with encodify on the same instructions.

## Where the time goes now

- Unwrapped input from 4 KiB up, AVX2: the kernels, identical to encodify's. The decode block costs 4 `vpshufb`, a shift and a `vextracti128` on Zen 5's two shuffle pipes, about 3 cycles per 32 characters; the encode block 11 vector operations on four pipes, measured 3.35 cycles per 24 bytes against 2.75 at best. Both crates sit at the same cost; only a cheaper kernel moves this.
- The same with AVX-512: about 7 instructions per 64 characters decoded, 5 per 48 bytes encoded. From 1 MiB up the loops run at about 50 GiB/s, near what the caches give; the decode measured a lower clock than the encode once (4.3 against 4.8 GHz), possibly the 512-bit multiplies, not confirmed on a quiet machine.
- Small inputs: fixed cost per call. A 64-byte unwrapped one-shot decode is 281 instructions with AVX-512 and 322 with AVX2: `decode` and `decode_all` about 50, the trim of the end about 20, the run's dispatcher and blocks (the dispatcher divides by 3 for the room in `dst`), the tail and `decode_finish` about 30. Encodify's strict decoder takes 296 for the same input, its lenient one 452.
- Wrapped input: the line kernel runs a line of 76 characters in 3 AVX2 blocks (2 with AVX-512), tested once; what is left is the first line, read by the run to learn the width, and the last one, which goes down the narrow chain (AVX-512, AVX2, SSSE3, scalar quanta for 12 characters).

## What was tried

Kept, with the numbers in the log: the four stamped loops behind `Loops`; each run a dispatcher reaching the narrower backend before any register is saved, its blocks a function of their own (LLVM saved 5 to 7 registers before the size check otherwise); the run stopped at its first bad quantum; groups of 4 wide blocks tested once (as encodify), counted from the run's length alone; the line width learnt from a line read whole; the end of the input trimmed of padding and blanks so that the last block of a run succeeds; the one-shot decode in one run when only a tail is left; `decode_bytes` on locals; AVX-512 VBMI; the scalar backend through the driver again, with 16-character blocks.

Left out or reverted:

- The last block of each group stored exactly (`vpermd`) so that groups map onto `as_chunks`: 11% fewer instructions, 4% more cycles, the extra shuffle on the bound pipes.
- The tail check before the line-break check for every rest, and a one-shot fast path redoing the first line on wrapped input: both cost wrapped decoding up to 12%.
- An all-exact single-block loop with `min` for the overlap: the same cost as `as_chunks` blocks plus one overlapping block, longer code.
- The AVX2 decode's high nibbles by `vpmulhuw`: no gain.
- Scalar runs of their own beside the driver, two quanta or four per step: 17% slower than the old tree at 4 KiB until the scalar kernels went back through the driver; the kernels must carry `#[inline]` like the SIMD ones, or LLVM calls them per block. Scalar encoding group by group in its block: 10% slower than two groups per 64-bit word.

## Levers left

Ordered by expected gain on the bench, each to be measured before it is kept:

1. The 64-byte unwrapped decode, the closest race: AVX-512 at 47 cycles against encodify's strict 47 (it was 45 before the narrow dispatchers, which won about 30 instructions on wrapped 64-byte input), AVX2 at 51 to 54. The fixed cost per call is the target: `decode` and `decode_all` (about 50 instructions), the trim (20), the dispatcher's division by 3 for the room in `dst` (avoidable when `dst` is sized by `decoded_len_max`, the one-shot case), `decode_finish`.
2. AVX2 unwrapped from 4 KiB up, level with encodify on the same kernel. Moving the high-nibble shift of the decode to the multiply pipes (`vpmulhuw` by 2^12 behind `opaque`) was tried and gave nothing (700 cycles against 693 to 697 at 4 KiB), so the shuffle pipes are not the whole bound; look at the loop with a pipeline analyser (uiCA, llvm-mca with `-mcpu=znver5`) before guessing again.
3. Scalar encoding: 4% slower than the tree before the session unwrapped at 4 KiB and 23% slower wrapped (6,039 against 4,899 cycles), both still several times the base64 crate. The old one encoded 24-byte blocks, 8 groups each, two per line plus a 12-byte one; this one encodes 12-byte blocks of two 64-bit words. Scalar decoding is back level with the old tree (16-character blocks, a byte of joined classes per quantum in its `Bad`).
4. AVX2 wrapped encoding: lead windows in `encode_lines` save one instruction per block where the window fits.
5. NEON: its 16-character kernels were dropped with the narrow loops, so runs below 64 characters are scalar on aarch64. It passes the suite under qemu-aarch64 and has never been measured on hardware.
6. Lines of code, none tried: drop the SSSE3 backend (254 lines; only CPUs from before 2013 lack AVX2, and AVX2's short runs would go scalar), which is the user's call; trim the docs of the stamped loops; share the 128 and 256-bit kernels (tried by the first session, it took more lines than it shared).

## Traps

- LLVM rewrites multiplies by a power of two into shifts (and blends, for `mullo`): hide the multiplier behind `opaque`, an empty `asm!` with `pure, nomem` that it can still hoist. Miri needs the `cfg(miri)` fallback.
- A safe `#[target_feature]` fn coerces to an `unsafe fn` pointer, which is what `Loops` holds; `avx512vbmi` implies AVX-512BW, AVX2 and SSSE3, so the narrower loops are callable without `unsafe` from wider ones.
- AVX-512 intrinsics need Rust 1.89, the crate's `rust-version` since this session.
- `decode_bytes` hands back to the runs at a line start, which can be past the trimmed end, inside the trailing blanks: the decoder clamps the end to where it is (the exhaustive short-end test caught it).
- Valgrind has no AVX-512; the fuzz crate needs the nightly of fuzz/shell.nix; aarch64 checks need a fenix toolchain with the aarch64 rust-std, and the tests run under `qemu-aarch64` linked with `rust-lld` for aarch64-unknown-linux-musl.
