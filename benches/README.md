# Benchmarks

[criterion](https://crates.io/crates/criterion) medians of `cargo bench --features std --bench compare` (source in [compare.rs](./compare.rs)), in GiB/s of raw bytes on both sides, on pseudo-random input. Measured on 2026-10-31, rustc 1.99, Linux x86_64 on an AMD Ryzen AI Max+ 395 (Zen 5, AVX-512 backend detected), pinned to one core with `taskset -c 13`, all groups in one run. The machine is shared with other work and its clock moves with the load: the 1 MiB and 10 MiB sizes, bound by memory, move by up to 40% between runs (the same decode read 23.8 and 44.6 GiB/s at 10 MiB minutes apart). A dependency or compiler bump moves them too, so re-run the bench after one; for differences of a few percent, compare cycles per call instead (cairn/log/2026-11-01-a-fresh-look.md).

- base64-codec: the detected backend (AVX-512 VBMI here); avx2: the AVX2 backend, the instructions encodify uses; scalar: the scalar backend, what `Base64Cpu::BASELINE` runs on a default x86_64 build.
- base64: the base64 crate 0.23, the backend of mime-rs's wrapper; wrapped, it runs line by line as that wrapper does.
- base64-simd 0.8: its forgiving decoder for wrapped input; it does not wrap when encoding.
- encodify 1.0.2: `STANDARD` unwrapped, which is strict; `MIME` wrapped (lenient, 76 characters, CRLF). encodify lenient: `MIME` on unwrapped input, the engine whose rules are base64-codec's.

Wrapped means lines of 76 characters ended by CRLF.

## Decoding

| input | base64-codec | avx2 | scalar | base64 | base64-simd | encodify | encodify lenient |
| --- | --- | --- | --- | --- | --- | --- | --- |
| unwrapped, 64 B | 6.49 | 5.90 | 2.80 | 2.34 | 6.56 | 5.98 | 4.39 |
| unwrapped, 4 KiB | 73.15 | 29.50 | 4.74 | 3.51 | 11.53 | 26.38 | 24.35 |
| unwrapped, 1 MiB | 49.59 | 30.11 | 4.78 | 3.52 | 11.66 | 26.65 | 26.62 |
| unwrapped, 10 MiB | 35.87 | 27.61 | 4.76 | 3.50 | 11.55 | 25.12 | 25.12 |
| wrapped, 64 B | 2.98 | 2.95 | 2.20 | 0.93 | 2.74 | 2.53 | |
| wrapped, 4 KiB | 30.53 | 20.92 | 4.75 | 1.30 | 2.33 | 17.28 | |
| wrapped, 1 MiB | 41.02 | 23.64 | 4.65 | 1.31 | 2.17 | 19.58 | |
| wrapped, 10 MiB | 35.41 | 22.55 | 4.64 | 1.30 | 2.30 | 18.89 | |

## Encoding

| input | base64-codec | avx2 | scalar | base64 | base64-simd | encodify |
| --- | --- | --- | --- | --- | --- | --- |
| unwrapped, 64 B | 7.03 | 5.82 | 2.68 | 3.00 | 7.74 | 6.10 |
| unwrapped, 4 KiB | 98.13 | 30.80 | 5.81 | 4.05 | 15.16 | 31.10 |
| unwrapped, 1 MiB | 49.14 | 31.45 | 5.80 | 4.09 | 15.48 | 31.54 |
| unwrapped, 10 MiB | 46.71 | 29.89 | 5.84 | 4.09 | 15.39 | 29.86 |
| wrapped, 64 B | 5.77 | 4.30 | 3.02 | 2.65 | | 2.63 |
| wrapped, 4 KiB | 51.40 | 21.58 | 5.46 | 3.23 | | 18.72 |
| wrapped, 1 MiB | 46.85 | 22.15 | 5.47 | 3.20 | | 20.94 |
| wrapped, 10 MiB | 40.16 | 21.55 | 5.52 | 3.22 | | 20.43 |

## Against encodify and the base64 crate

With the backend detected here, AVX-512, base64-codec is ahead of encodify 1.0.2 in every section: at 64 bytes 1.09x on unwrapped decoding, 1.18x on wrapped decoding, 1.15x on unwrapped encoding and 2.19x on wrapped encoding; from 4 KiB, 1.4x to 3.2x. On AVX2 alone, encodify's instructions, it is ahead on decoding (1.12x at 4 KiB unwrapped, 1.21x wrapped, by its decode kernel one operation shorter and tested by `vptest` per block) and on wrapped encoding (1.15x at 4 KiB), and level on unwrapped encoding, where both run the same kernel (0.99x at 4 KiB, 572 cycles per call each). base64-simd 0.8 encodes 64 unwrapped bytes faster still (7.74 GiB/s).

Against what mime-rs runs today, the base64 crate line by line, at 1 MiB: wrapped decoding 31x, wrapped encoding 15x; unwrapped, 14x decoding and 12x encoding. The scalar backend alone is now ahead of it in every section but unwrapped encoding at 64 bytes (2.68 against 3.00 GiB/s): 1.4x decoding and 1.4x encoding unwrapped, 3.6x decoding and 1.7x encoding wrapped at 1 MiB; an application should still pass `Base64Cpu::detect()`.

## What moved the numbers

Read from per-call cycle and instruction counts (`perf stat`), callgrind and the generated assembly, and kept only when the counts showed it:

- The AVX-512 VBMI backend: one `vpermi2b` validates and translates 64 characters, three permutes encode 48 bytes, about 7 and 5 instructions per block against 2 x 17 and 2 x 13 for AVX2.
- A run stops at its first character outside the alphabet, the kernels telling where (movemask and tzcnt), which replaced the narrow and quantum-by-quantum loops the run fell back to.
- The x86 decode kernels clamp the sextets to 63, so that `/` needs no offset of its own: one operation fewer per block than encodify's; AVX2 tests each block with one `vptest`, which costs less than joining the blocks of a group (the other backends still join them).
- AVX-512 runs shorter than one block in one masked block, and the one-shot decode ends the usual input without building a decoder: wrapped encoding at 64 bytes went from 60.6 to 44.2 cycles per call, wrapped decoding from 103.2 to 84.3.
- The scalar kernels on tables: four lookups of bytes already in place per quantum decoded, one lookup per pair of sextets encoded, 1.6x to 1.8x the scalar backend before.
- The end of an input is found by trimming its padding and blanks, so that the last block of a run succeeds; the decoder's and encoder's loops are plain code over four stamped loops per backend, each run a dispatcher reaching the narrower backend before any register is saved.
- Tried and left out: masked AVX-512 loads and stores for every block (their masked-off bytes cross into the next page at a buffer's end), each block tested apart on AVX-512 and scalar, windows across lines for wrapped encoding; the cairn log of 2026-11-01-a-fresh-look has the numbers.
