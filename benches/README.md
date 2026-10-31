# Benchmarks

[criterion](https://crates.io/crates/criterion) medians of `cargo bench --features std --bench compare` (source in [compare.rs](./compare.rs)), in GiB/s of raw bytes on both sides, on pseudo-random input. Measured on 2026-10-31, rustc 1.99, Linux x86_64 on an AMD Ryzen AI Max+ 395 (Zen 5, AVX-512 backend detected), pinned to one core with `taskset -c 12`; the decoding tables were re-run once the machine, shared with other work, was quiet. A dependency or compiler bump moves them, so re-run the bench after one; for differences of a few percent, compare cycles per call instead (cairn/log/2026-10-31-further-speed-levers.md).

- base64-codec: the detected backend (AVX-512 VBMI here); avx2: the AVX2 backend, the instructions encodify uses; scalar: the scalar backend, what `Base64Cpu::BASELINE` runs on a default x86_64 build.
- base64: the base64 crate 0.23, the backend of mime-rs's wrapper; wrapped, it runs line by line as that wrapper does.
- base64-simd 0.8: its forgiving decoder for wrapped input; it does not wrap when encoding.
- encodify 1.0.2: `STANDARD` unwrapped, which is strict; `MIME` wrapped (lenient, 76 characters, CRLF). encodify lenient: `MIME` on unwrapped input, the engine whose rules are base64-codec's.

Wrapped means lines of 76 characters ended by CRLF.

## Decoding

| input | base64-codec | avx2 | scalar | base64 | base64-simd | encodify | encodify lenient |
| --- | --- | --- | --- | --- | --- | --- | --- |
| unwrapped, 64 B | 5.96 | 5.47 | 1.73 | 2.39 | 5.91 | 5.92 | 4.41 |
| unwrapped, 4 KiB | 69.19 | 24.65 | 2.78 | 3.49 | 11.23 | 25.26 | 23.60 |
| unwrapped, 1 MiB | 49.87 | 26.29 | 2.82 | 3.52 | 11.59 | 26.43 | 26.35 |
| unwrapped, 10 MiB | 41.88 | 23.63 | 2.81 | 3.52 | 11.51 | 24.04 | 23.94 |
| wrapped, 64 B | 2.59 | 2.65 | 1.53 | 1.03 | 2.74 | 2.54 | |
| wrapped, 4 KiB | 35.93 | 18.28 | 2.80 | 1.33 | 2.41 | 17.10 | |
| wrapped, 1 MiB | 40.56 | 21.38 | 2.89 | 1.33 | 2.30 | 19.63 | |
| wrapped, 10 MiB | 30.60 | 20.30 | 2.91 | 1.33 | 2.31 | 18.82 | |

## Encoding

| input | base64-codec | avx2 | scalar | base64 | base64-simd | encodify |
| --- | --- | --- | --- | --- | --- | --- |
| unwrapped, 64 B | 7.07 | 5.86 | 1.85 | 3.05 | 7.87 | 6.14 |
| unwrapped, 4 KiB | 79.65 | 29.31 | 3.20 | 4.09 | 14.76 | 29.73 |
| unwrapped, 1 MiB | 50.97 | 32.05 | 3.23 | 4.16 | 15.68 | 31.99 |
| unwrapped, 10 MiB | 45.62 | 29.86 | 3.23 | 4.15 | 15.38 | 29.82 |
| wrapped, 64 B | 3.96 | 3.99 | 2.07 | 2.67 | | 2.68 |
| wrapped, 4 KiB | 49.75 | 21.72 | 3.01 | 3.23 | | 18.78 |
| wrapped, 1 MiB | 47.95 | 22.30 | 3.03 | 3.19 | | 21.06 |
| wrapped, 10 MiB | 40.12 | 21.72 | 3.03 | 3.25 | | 20.52 |

## Against encodify and the base64 crate

With the backend detected here, AVX-512, base64-codec is ahead of encodify 1.0.2 in every section but unwrapped decoding at 64 bytes, where it is level with encodify's strict `STANDARD` (5.96 against 5.92 GiB/s, 47 cycles per call each) and ahead of its lenient engine (4.41). From 4 KiB it runs 1.5x to 2.7x encodify on unwrapped input and 1.6x to 2.7x on wrapped input; at 64 bytes, 1.15x on unwrapped encoding, 1.5x on wrapped encoding, 1.02x on wrapped decoding. On AVX2 alone, encodify's instructions, the kernels are the same: base64-codec is ahead on wrapped input (1.04x to 1.5x), level on unwrapped encoding (0.95x at 64 bytes here, but 41 against 46 cycles per call), and behind on unwrapped decoding by up to 2% from 4 KiB and 8% at 64 bytes.

Against what mime-rs runs today, the base64 crate line by line, at 1 MiB: wrapped decoding 30x, wrapped encoding 15x; unwrapped, 14x decoding and 12x encoding. The scalar backend alone decodes wrapped input 2.2x faster than it, but unwrapped input at 0.8x, so an application should pass `Base64Cpu::detect()`.

## What moved the numbers

Read from per-call cycle and instruction counts (`perf stat`), callgrind and the generated assembly, and kept only when the counts showed it:

- The AVX-512 VBMI backend: one `vpermi2b` validates and translates 64 characters, three permutes encode 48 bytes, about 7 and 5 instructions per block against 2 x 17 and 2 x 13 for AVX2.
- A run stops at its first character outside the alphabet, the kernels telling where (movemask and tzcnt), which replaced the narrow and quantum-by-quantum loops the run fell back to: the 64-byte unwrapped decode went from 490 instructions per call to 281 with AVX-512 and 322 with AVX2.
- The end of an input is found by trimming its padding and blanks, so that the last block of a run succeeds, and the one-shot decode is one run and that end when nothing else is left.
- The decoder's and encoder's loops are plain code over four stamped loops per backend, each run a dispatcher reaching the narrower backend before any register is saved.
- The scalar backend runs the same driver with 16-character table-lookup blocks, its kernels inlined: called per block, they cost it 17%.
- Tried and left out: an exact store closing each decode group (fewer instructions, more cycles on Zen 5's shuffle pipes), and the decode's high nibbles through a multiply rather than a shift (no gain).

The scalar backend encodes wrapped input about 20% slower than before this round, and unwrapped input 4% to 8% slower; what is left is in cairn/changes/a-fresh-look.
