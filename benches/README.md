# Benchmarks

[criterion](https://crates.io/crates/criterion) medians of `cargo bench --features std --bench compare` (source in [compare.rs](./compare.rs)), in GiB/s of raw bytes on both sides, on pseudo-random input. Measured on 2026-11-01, rustc 1.99, Linux x86_64 on an AMD Ryzen AI Max+ 395 (Zen 5, AVX-512 backend detected), pinned to one core with `taskset -c 13`, all groups in one run. The machine is shared with other work and its clock moves with the load: the 1 MiB and 10 MiB sizes, bound by memory, move by up to 40% between runs (the same decode read 23.8 and 44.6 GiB/s at 10 MiB minutes apart). The 4 KiB rows of the AVX-512 backend move by up to 20% with the alignment of the output buffer, which the allocator picks anew every run: its stores run 15% to 20% slower on an output 16 bytes off a cache line (unwrapped encoding at 4 KiB 151 cycles per call aligned, 175 to 185 off by 16), so the same tree read 84 and 104 GiB/s on that row in two runs of this session. A dependency or compiler bump moves them too, so re-run the bench after one; for differences of a few percent, compare cycles per call instead (cairn/log/2026-11-01-past-the-periods.md).

- base64-codec: the detected backend (AVX-512 VBMI here); avx2: the AVX2 backend, the instructions encodify uses; scalar: the scalar backend, what `Base64Cpu::BASELINE` runs on a default x86_64 build.
- base64: the base64 crate 0.23, the backend of mime-rs's wrapper; wrapped, it runs line by line as that wrapper does.
- base64-simd 0.8: its forgiving decoder for wrapped input; it does not wrap when encoding.
- encodify 1.0.2: `STANDARD` unwrapped, which is strict; `MIME` wrapped (lenient, 76 characters, CRLF). encodify lenient: `MIME` on unwrapped input, the engine whose rules are base64-codec's.

Wrapped means lines of 76 characters ended by CRLF.

## Decoding

| input | base64-codec | avx2 | scalar | base64 | base64-simd | encodify | encodify lenient |
| --- | --- | --- | --- | --- | --- | --- | --- |
| unwrapped, 64 B | 6.37 | 5.82 | 2.70 | 2.39 | 6.46 | 5.93 | 3.92 |
| unwrapped, 4 KiB | 75.87 | 29.73 | 4.72 | 3.52 | 11.49 | 25.86 | 24.00 |
| unwrapped, 1 MiB | 49.25 | 29.79 | 4.70 | 3.50 | 11.58 | 26.32 | 26.24 |
| unwrapped, 10 MiB | 44.41 | 28.12 | 4.71 | 3.51 | 11.55 | 25.36 | 25.35 |
| wrapped, 64 B | 3.79 | 3.97 | 2.51 | 1.02 | 2.74 | 2.27 | |
| wrapped, 4 KiB | 45.18 | 26.77 | 4.74 | 1.32 | 2.06 | 17.05 | |
| wrapped, 1 MiB | 38.16 | 28.33 | 4.77 | 1.33 | 2.31 | 19.45 | |
| wrapped, 10 MiB | 33.96 | 26.14 | 4.76 | 1.32 | 2.33 | 18.92 | |

## Encoding

| input | base64-codec | avx2 | scalar | base64 | base64-simd | encodify |
| --- | --- | --- | --- | --- | --- | --- |
| unwrapped, 64 B | 7.89 | 6.71 | 2.79 | 3.06 | 7.87 | 6.15 |
| unwrapped, 4 KiB | 84.25 | 31.16 | 5.88 | 4.08 | 15.10 | 31.07 |
| unwrapped, 1 MiB | 49.94 | 31.59 | 5.85 | 4.12 | 15.54 | 31.60 |
| unwrapped, 10 MiB | 47.87 | 30.11 | 5.87 | 4.12 | 15.42 | 30.10 |
| wrapped, 64 B | 6.05 | 4.38 | 3.00 | 2.64 | | 2.66 |
| wrapped, 4 KiB | 63.39 | 26.69 | 5.85 | 3.22 | | 19.08 |
| wrapped, 1 MiB | 48.24 | 27.88 | 5.92 | 3.22 | | 21.07 |
| wrapped, 10 MiB | 40.46 | 26.36 | 5.94 | 3.21 | | 20.44 |

## Against encodify and the base64 crate

With the backend detected here, AVX-512, base64-codec is ahead of encodify 1.0.2 in every section: at 64 bytes 1.07x on unwrapped decoding, 1.67x on wrapped decoding, 1.28x on unwrapped encoding and 2.27x on wrapped encoding; from 4 KiB, 1.6x to 3.3x. On AVX2 alone, encodify's instructions, it is ahead on decoding from 4 KiB (1.15x unwrapped, by its decode kernel one operation shorter and tested by `vptest` per block, 1.57x wrapped, by its periods), on wrapped input at 64 bytes (1.75x decoding, 1.65x encoding), on wrapped encoding (1.40x at 4 KiB) and on unwrapped encoding at 64 bytes (1.09x); it is level on unwrapped encoding from 4 KiB, where both run the same kernel (1.00x, 572 against 578 cycles per call), and on unwrapped decoding at 64 bytes (0.98x here, 43.2 against 44.7 cycles per call). base64-simd 0.8 decodes 64 unwrapped bytes a little faster (6.46 GiB/s) and encodes them level (7.87).

Against what mime-rs runs today, the base64 crate line by line, at 1 MiB: wrapped decoding 29x, wrapped encoding 15x; unwrapped, 14x decoding and 12x encoding. The scalar backend alone is ahead of it in every section but unwrapped encoding at 64 bytes (2.79 against 3.06 GiB/s): 1.3x decoding and 1.4x encoding unwrapped, 3.6x decoding and 1.8x encoding wrapped at 1 MiB; an application should still pass `Base64Cpu::detect()`.

## What moved the numbers

Read from per-call cycle and instruction counts (`perf stat`), callgrind and the generated assembly, and kept only when the counts showed it:

- The AVX-512 VBMI backend: one `vpermi2b` validates and translates 64 characters, three permutes encode 48 bytes, about 7 and 5 instructions per block against 2 x 17 and 2 x 13 for AVX2.
- A run stops at its first character outside the alphabet, the kernels telling where (movemask and tzcnt), which replaced the narrow and quantum-by-quantum loops the run fell back to.
- The x86 decode kernels clamp the sextets to 63, so that `/` needs no offset of its own: one operation fewer per block than encodify's; AVX2 tests each block with one `vptest`, which costs less than joining the blocks of a group (the other backends still join them).
- AVX-512 runs shorter than one block in one masked block, and the one-shot decode ends the usual input without building a decoder: wrapped encoding at 64 bytes went from 60.6 to 44.2 cycles per call, wrapped decoding from 103.2 to 84.3.
- The scalar kernels on tables: four lookups of bytes already in place per quantum decoded, one lookup per pair of sextets encoded, 1.6x to 1.8x the scalar backend before.
- Lines of 76 characters a period at a time: the characters of 16, 8 or 4 lines make exactly 19 blocks of 64, 32 or 16, written out with each block's place known at compile time, a block across a line break stitched from its two lines when decoding and split around the line ending when encoding. AVX-512 decodes 1.19 blocks per line where it decoded 2, AVX2 2.375 where it decoded 3: wrapped decoding at 4 KiB went from 451 to 387 cycles per call on AVX-512 and from 830 to 715 on AVX2, wrapped encoding from 817 to 672 on AVX2 and 7% to 13% faster on scalar and SSSE3, level on AVX-512, whose stores bound it.
- The end of an input is found by trimming its padding and blanks, so that the last block of a run succeeds; the decoder's and encoder's loops are plain code over four stamped loops per backend, each run a dispatcher reaching the narrower backend before any register is saved.
- Wrapped input sent to the decoder's line loops from its first line, by a look at character 76, where the first line went as a run stopping at the line break; the input's end trimmed once, by byte compares; the one-shot encode's unwrapped path apart; the line endings written by the encode kernels, the AVX-512 one inside its masked store, 16 stores fewer per period of 16 lines: wrapped decoding at 4 KiB went from 380 to 331 cycles per call on AVX-512 and from 707 to 667 on AVX2, at 64 bytes 14% to 25% faster on every backend; unwrapped encoding at 64 bytes 12% to 13% faster on AVX-512 and AVX2, 6% to 8% on SSSE3 and scalar; AVX-512 wrapped encoding at 4 KiB from 317 to 291. Unwrapped decoding at 64 bytes pays a cycle for the look at character 76.
- Tried and left out: masked AVX-512 loads and stores for every block (their masked-off bytes cross into the next page at a buffer's end), each block tested apart on AVX-512 and scalar, windows across lines for wrapped encoding (the cairn log of 2026-11-01-a-fresh-look has the numbers); lines as one stream with each block's place tracked at run time, periods looped rather than written out, a last period moved back over lines done, and periods for decoding with 16-character blocks (the cairn log of 2026-11-01-a-closer-look); the one-shot decode through the decoder's loop alone, wrapped input detected from where the first run stops, spare or exact stores chosen per block, scalar quanta loaded as 32-bit words (the cairn log of 2026-11-01-past-the-periods).
