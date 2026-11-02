---
cairn: change
id: the-gathered-blocks
status: draft
created: 2026-11-02
---

# The gathered blocks

The handoff of the session of 2026-11-02 (log 2026-11-02-the-store-queue), written for a fresh context to take over the optimisation without its history: where things stand, how to measure, where the time goes, what was tried, and what is left.

## Why

The user's goal: base64-codec should beat encodify 1.0.2 in every section of benches/compare.rs with the most concise code, each pass doing better than the one before in speed and in size, with inline comments kept to the important ones (the `SAFETY` comments stay: the crate denies `clippy::undocumented_unsafe_blocks`), searching for new methods where needed. On the development machine (AMD Ryzen AI Max+ 395, Zen 5, AVX-512 VBMI detected) it is ahead in every section; on AVX2 alone, encodify's instructions, it is ahead everywhere, unwrapped encoding at 4 KiB by 5%, unwrapped decoding at 64 bytes by 1% only. The pass of the-store-queue bought speed with size (1,978 code lines against 1,884): the next pass owes the size back.

Settled with the user: the SSSE3 backend stays; CPU detection stays the application's; decoding keeps one entry point with no wrapped option; helpers with one caller are folded into it; encoding may not write past what it reports (only decoding may, within the length it was given: cairn/spec/calls.md).

## Where things stand

Cycles per call (`perf stat`, core 13, its sibling idle, averaged over eight source and output offset pairs, the least of three runs each), AVX-512 against encodify; the other backends and every row but wrapped encoding run the code of the-last-glue, whose log has their numbers:

| workload | AVX-512 | encodify |
| --- | --- | --- |
| encode unwrapped, 64 B | 33.7 | 43.9 |
| encode unwrapped, 4 KiB | 134.6 | 574.8 |
| encode wrapped, 64 B | 42.5 | 109.4 |
| encode wrapped, 4 KiB | 230.6 | 934.1 |
| decode unwrapped, 64 B | 37.9 | 43.9 |
| decode unwrapped, 4 KiB | 197.1 | 692.2 |
| decode wrapped, 64 B | 57.3 | 113.6 |
| decode wrapped, 4 KiB | 336.5 | 1,037.5 |

The code: src/avx512.rs `encode_wrapped` (inlined into the stamped `encode_lines` through the driver's `periods:` hook) sends 32 lines or more to `encode_cycles`: per cycle of 32 CRLF or 64 LF lines, whose output is 39 or 77 whole vectors drawn from 38 or 76 encoded blocks, each vector one masked `vpermt2b` of the two blocks it draws from by its `Wrap` index (line ending bytes where bit 7 is clear), one store each. `wraps` builds the indices, `stay` the one vector a cycle that draws from the same blocks as the one before. Everything else is the-last-glue's: periods of 19 blocks written out by `base64_period!`, decode stitches, `lines_and_run`.

## How to measure

The method of the-store-queue (archived), unchanged, with these additions:

- The harness of this pass: a scratch crate with the tree, a copy of HEAD renamed `base64-codec-before` (`git archive HEAD src Cargo.toml`, `[[bench]]` and dev-dependencies dropped) and encodify `=1.0.2`, `count <lib>:<backend> <enc|dec> <0|1|2> <size> <iters> <src offset> <dst offset>`, 1 for CRLF and 2 for LF, the backends picked by removing features from `SimdCpu::detect`, the output checked against the scalar reference after the loop. Its copy lives in /tmp/claude-1000/-home-soywod-code-pimalaya/b47fd4e0-aa89-429c-bcec-dd781659bb13/scratchpad while /tmp lasts: build.sh (aligned build), m.sh (rows over the eight pairs, `REP`, `PAIRS`), loc.sh (lines), ops2/ (the `asm!` timings).
- An `asm!` throughput loop needs about ten independent destinations, never a destination that is also a source: four chains of a 5-cycle instruction read 0.8 a cycle, which is how quoted-printable-codec misread `vpermt2b`. Its buffers must not share a page offset, or 4K aliasing between its loads and stores reads as a port bound.
- Check that untouched code is untouched: diff the release assembly of the tree and HEAD function by function, addresses and `rip` offsets normalized (the log of the-store-queue did it for every function but one).
- The test suite's lengths stop at 1,024 bytes: a change to paths that start at 32 lines needs the around-cycles test, and a deliberate break to prove a test reaches it.

## Where the time goes now

- AVX-512 wrapped encoding: about 2 cycles a vector (39 a cycle), against 1.55 for an `asm!` replica of its operations (a table load, an unaligned 64-byte input load, two `vpermb`, a multishift, a `vpmovb2m`, a `vpermt2b`, an unaligned store). Stores off a cache line cost a slot more; the output vectors sit at fixed places from the output's start, so they cannot be aligned without tables per output alignment.
- AVX-512 wrapped decoding: 336 cycles at 4 KiB against 197 unwrapped, the periods' stitched blocks (two loads and a blend for each block across a line break) and the line ending checks.
- AVX-512 unwrapped decoding: at the machine's limit for its operations (the-last-glue).
- Small calls and scalar: as in the-store-queue (archived).

## Levers left

Ordered by expected gain, each to be measured before it is kept:

1. Size: the pass of the-store-queue added 94 code lines. Candidates: the SSSE3 and AVX2 twins (decode tables), the decode stitch of SSSE3 and scalar compiled for periods they never run, `encode_all` against the encoder's run; whether AVX-512 still needs the stamped encode periods below 32 lines (16 to 31 lines run them; the cycle loop there was 7% to 13% slower, from the line it leaves apart).
2. AVX-512 wrapped decoding by gathered blocks: block b of a cycle lies in input vectors b and b + 1, so one `vpermt2b` by a per-block index gathers its 64 characters, each input vector loaded once and its line endings compared with the encoder's `Wrap` of the same place (the decoder's input is the encoder's output). Tried this pass: 405 to 451 cycles at 4 KiB against 285 to 381, the FP scheduler full 21% of the cycles (6% for HEAD), the chain per block about 30 cycles of eleven vector operations. To try: translate each input vector once (the 128-entry lookup on vectors, not blocks, invalid characters marked in bit 7), then gather sextets, which moves the lookup off the per-block chain; check line endings with one compare a vector against a constant per place rather than two mask operations; test the joined `Bad` once per cycle and redo a failing cycle through `decode_periods`. A good line count must leave out a line whose last character ends the last block, its line ending possibly in a vector not yet checked. The code of the attempt is avx512.v10-decode.rs in the scratchpad above.
3. AVX-512 wrapped encoding toward its replica's 1.55 cycles a vector: the loop's bookkeeping (two counters, the `stay` compare), and the lines left apart at the end (one line through `encode_lines`, 8% of a 4 KiB call in the first version).
4. NEON wrapped encoding by the same cycles: `vqtbl2q_u8` of two 16-byte character vectors per 16 bytes of output, its line endings from `vqtbx`; under qemu only, so instruction counts at best.
5. The levers of the-store-queue not taken: small calls without `decode_blocks`' fixed costs, AVX2 unwrapped decoding at 64 bytes, scalar decoding robust to codegen units, NEON measured on hardware.

## Traps

- A loop whose load address comes from a table load runs 1.4x slower than the same loop with the address computed (the encoder's first version): compute addresses, and keep only the data in tables.
- A runtime division by a constant on `usize` can compile to a 128-bit `mul` with fix-ups (by 39): a 32-bit division is a multiply and a shift.
- LLVM folds a static table into the first iterations of a loop over it (it peeled three vectors of `CRLF` into `vpermt2w` with constants); read the steady loop, not the head, when counting instructions.
- A branch added inside the hot loop's advance path, or slices with runtime lengths in place of fixed-size chunks, broke the loop's shape (2,504 and 7,628 instructions where 1,670 and 5,615 ran).
- The traps of the-store-queue still hold: page distance, a step at a time helping only chains that fill the scheduler, the small paths' tightness, `encode` and `decode` staying `#[inline]`, qemu's `max` model, AVX2's eager `Bad`, layout moving results by up to 2x.
