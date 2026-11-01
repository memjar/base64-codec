# Benchmarks

[criterion](https://crates.io/crates/criterion) medians of `cargo bench --features std --bench compare` (source in [compare.rs](./compare.rs)), in GiB/s of raw bytes on both sides, on pseudo-random input. Measured on 2026-11-01, rustc 1.99, Linux x86_64 on an AMD Ryzen AI Max+ 395 (Zen 5, AVX-512 backend detected), pinned to one core with `taskset -c 13`, all groups in one run. The machine is shared with other work and its clock moves with the load: the 1 MiB and 10 MiB sizes, bound by memory, move by up to 40% between runs (the same decode read 23.8 and 44.6 GiB/s at 10 MiB minutes apart). A dependency or compiler bump moves them too, so re-run the bench after one; for differences of a few percent, compare cycles per call instead (cairn/log/2026-11-01-a-closer-look.md).

- base64-codec: the detected backend (AVX-512 VBMI here); avx2: the AVX2 backend, the instructions encodify uses; scalar: the scalar backend, what `Base64Cpu::BASELINE` runs on a default x86_64 build.
- base64: the base64 crate 0.23, the backend of mime-rs's wrapper; wrapped, it runs line by line as that wrapper does.
- base64-simd 0.8: its forgiving decoder for wrapped input; it does not wrap when encoding.
- encodify 1.0.2: `STANDARD` unwrapped, which is strict; `MIME` wrapped (lenient, 76 characters, CRLF). encodify lenient: `MIME` on unwrapped input, the engine whose rules are base64-codec's.

Wrapped means lines of 76 characters ended by CRLF.

## Decoding

| input | base64-codec | avx2 | scalar | base64 | base64-simd | encodify | encodify lenient |
| --- | --- | --- | --- | --- | --- | --- | --- |
| unwrapped, 64 B | 6.46 | 5.81 | 2.73 | 2.34 | 6.37 | 5.87 | 4.09 |
| unwrapped, 4 KiB | 69.22 | 29.34 | 4.60 | 3.39 | 11.24 | 25.81 | 23.78 |
| unwrapped, 1 MiB | 48.72 | 29.37 | 4.63 | 3.45 | 10.46 | 23.57 | 25.85 |
| unwrapped, 10 MiB | 44.60 | 27.59 | 4.61 | 3.43 | 11.30 | 24.93 | 24.90 |
| wrapped, 64 B | 2.67 | 2.68 | 2.11 | 1.00 | 2.57 | 2.35 | |
| wrapped, 4 KiB | 40.27 | 24.64 | 4.65 | 1.29 | 1.40 | 15.64 | |
| wrapped, 1 MiB | 39.47 | 27.89 | 4.72 | 1.30 | 1.39 | 19.06 | |
| wrapped, 10 MiB | 33.38 | 25.34 | 4.72 | 1.29 | 1.39 | 18.55 | |

## Encoding

| input | base64-codec | avx2 | scalar | base64 | base64-simd | encodify |
| --- | --- | --- | --- | --- | --- | --- |
| unwrapped, 64 B | 6.93 | 5.83 | 2.68 | 2.99 | 7.65 | 6.02 |
| unwrapped, 4 KiB | 117.16 | 30.89 | 5.76 | 4.02 | 15.17 | 30.99 |
| unwrapped, 1 MiB | 48.91 | 30.95 | 5.73 | 4.04 | 15.21 | 30.87 |
| unwrapped, 10 MiB | 46.13 | 29.23 | 5.70 | 4.04 | 15.09 | 29.26 |
| wrapped, 64 B | 5.70 | 4.29 | 2.93 | 2.49 | | 2.65 |
| wrapped, 4 KiB | 60.89 | 26.55 | 5.88 | 3.19 | | 18.70 |
| wrapped, 1 MiB | 44.87 | 27.44 | 5.82 | 3.16 | | 20.63 |
| wrapped, 10 MiB | 37.74 | 25.85 | 5.87 | 3.17 | | 20.18 |

## Against encodify and the base64 crate

With the backend detected here, AVX-512, base64-codec is ahead of encodify 1.0.2 in every section: at 64 bytes 1.10x on unwrapped decoding, 1.14x on wrapped decoding, 1.15x on unwrapped encoding and 2.15x on wrapped encoding; from 4 KiB, 1.6x to 3.8x. On AVX2 alone, encodify's instructions, it is ahead on decoding (1.14x at 4 KiB unwrapped, by its decode kernel one operation shorter and tested by `vptest` per block, 1.58x wrapped, by its periods) and on wrapped encoding (1.42x at 4 KiB), and level on unwrapped encoding, where both run the same kernel (1.00x at 4 KiB, 578 cycles per call each). base64-simd 0.8 encodes 64 unwrapped bytes faster still (7.65 GiB/s). Against the run of 2026-10-31, the 64-byte and memory-bound rows moved with the clock of a loaded machine, encodify's as much as ours; the cycle counts of cairn/log/2026-11-01-a-closer-look.md have them level.

Against what mime-rs runs today, the base64 crate line by line, at 1 MiB: wrapped decoding 30x, wrapped encoding 14x; unwrapped, 14x decoding and 12x encoding. The scalar backend alone is ahead of it in every section but unwrapped encoding at 64 bytes (2.68 against 2.99 GiB/s): 1.3x decoding and 1.4x encoding unwrapped, 3.6x decoding and 1.8x encoding wrapped at 1 MiB; an application should still pass `Base64Cpu::detect()`.

## What moved the numbers

Read from per-call cycle and instruction counts (`perf stat`), callgrind and the generated assembly, and kept only when the counts showed it:

- The AVX-512 VBMI backend: one `vpermi2b` validates and translates 64 characters, three permutes encode 48 bytes, about 7 and 5 instructions per block against 2 x 17 and 2 x 13 for AVX2.
- A run stops at its first character outside the alphabet, the kernels telling where (movemask and tzcnt), which replaced the narrow and quantum-by-quantum loops the run fell back to.
- The x86 decode kernels clamp the sextets to 63, so that `/` needs no offset of its own: one operation fewer per block than encodify's; AVX2 tests each block with one `vptest`, which costs less than joining the blocks of a group (the other backends still join them).
- AVX-512 runs shorter than one block in one masked block, and the one-shot decode ends the usual input without building a decoder: wrapped encoding at 64 bytes went from 60.6 to 44.2 cycles per call, wrapped decoding from 103.2 to 84.3.
- The scalar kernels on tables: four lookups of bytes already in place per quantum decoded, one lookup per pair of sextets encoded, 1.6x to 1.8x the scalar backend before.
- Lines of 76 characters a period at a time: the characters of 16, 8 or 4 lines make exactly 19 blocks of 64, 32 or 16, written out with each block's place known at compile time, a block across a line break stitched from its two lines when decoding and split around the line ending when encoding. AVX-512 decodes 1.19 blocks per line where it decoded 2, AVX2 2.375 where it decoded 3: wrapped decoding at 4 KiB went from 451 to 387 cycles per call on AVX-512 and from 830 to 715 on AVX2, wrapped encoding from 817 to 672 on AVX2 and 7% to 13% faster on scalar and SSSE3, level on AVX-512, whose stores bound it.
- The end of an input is found by trimming its padding and blanks, so that the last block of a run succeeds; the decoder's and encoder's loops are plain code over four stamped loops per backend, each run a dispatcher reaching the narrower backend before any register is saved.
- Tried and left out: masked AVX-512 loads and stores for every block (their masked-off bytes cross into the next page at a buffer's end), each block tested apart on AVX-512 and scalar, windows across lines for wrapped encoding (the cairn log of 2026-11-01-a-fresh-look has the numbers); lines as one stream with each block's place tracked at run time, periods looped rather than written out, a last period moved back over lines done, and periods for decoding with 16-character blocks (the cairn log of 2026-11-01-a-closer-look).
