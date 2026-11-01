# Benchmarks

[criterion](https://crates.io/crates/criterion) medians of `cargo bench --features std --bench compare` (source in [compare.rs](./compare.rs)), in GiB/s of raw bytes on both sides, on pseudo-random input. Measured on 2026-11-01, rustc 1.99, Linux x86_64 on an AMD Ryzen AI Max+ 395 (Zen 5, AVX-512 backend detected), pinned to one core with `taskset -c 13`, all groups in one run. The machine is shared with other work and its clock moves with the load: the 1 MiB and 10 MiB sizes, bound by memory, move by up to 40% between runs (the same decode read 23.8 and 44.6 GiB/s at 10 MiB minutes apart). The 4 KiB rows move by up to 30% with where the allocator, anew every run, places the input and output buffers: their streams alias on the low 12 address bits (AVX-512 wrapped decoding at 4 KiB reads 284 cycles per call with both at the same page offset, 363 to 370 with them 2 KiB apart), so the same tree read 56.7 and 64.8 GiB/s on AVX-512 wrapped encoding at 4 KiB in two runs of this session. A dependency or compiler bump moves them too, so re-run the bench after one; for differences of a few percent, compare cycles per call instead (cairn/log/2026-11-01-the-last-glue.md).

- base64-codec: the detected backend (AVX-512 VBMI here); avx2: the AVX2 backend, the instructions encodify uses; scalar: the scalar backend, what `Base64Cpu::BASELINE` runs on a default x86_64 build.
- base64: the base64 crate 0.23, the backend of mime-rs's wrapper; wrapped, it runs line by line as that wrapper does.
- base64-simd 0.8: its forgiving decoder for wrapped input; it does not wrap when encoding.
- encodify 1.0.2: `STANDARD` unwrapped, which is strict; `MIME` wrapped (lenient, 76 characters, CRLF). encodify lenient: `MIME` on unwrapped input, the engine whose rules are base64-codec's.

Wrapped means lines of 76 characters ended by CRLF.

## Decoding

| input | base64-codec | avx2 | scalar | base64 | base64-simd | encodify | encodify lenient |
| --- | --- | --- | --- | --- | --- | --- | --- |
| unwrapped, 64 B | 6.51 | 5.91 | 2.71 | 2.36 | 6.44 | 5.90 | 4.31 |
| unwrapped, 4 KiB | 74.59 | 29.42 | 4.63 | 3.46 | 11.39 | 25.99 | 24.10 |
| unwrapped, 1 MiB | 48.31 | 29.54 | 4.67 | 3.46 | 11.44 | 26.08 | 26.02 |
| unwrapped, 10 MiB | 43.49 | 27.66 | 4.66 | 3.45 | 11.37 | 24.87 | 24.88 |
| wrapped, 64 B | 4.82 | 4.99 | 2.94 | 1.01 | 2.64 | 2.47 | |
| wrapped, 4 KiB | 44.70 | 26.43 | 4.74 | 1.30 | 1.41 | 16.55 | |
| wrapped, 1 MiB | 38.36 | 28.19 | 4.74 | 1.31 | 1.41 | 19.24 | |
| wrapped, 10 MiB | 32.56 | 25.37 | 4.73 | 1.31 | 1.40 | 18.63 | |

## Encoding

| input | base64-codec | avx2 | scalar | base64 | base64-simd | encodify |
| --- | --- | --- | --- | --- | --- | --- |
| unwrapped, 64 B | 7.77 | 6.68 | 3.23 | 3.06 | 7.86 | 6.12 |
| unwrapped, 4 KiB | 128.55 | 32.75 | 5.87 | 4.08 | 15.49 | 31.45 |
| unwrapped, 1 MiB | 49.71 | 32.97 | 5.80 | 4.12 | 15.48 | 31.56 |
| unwrapped, 10 MiB | 44.59 | 31.17 | 5.77 | 4.11 | 15.26 | 29.73 |
| wrapped, 64 B | 6.04 | 4.39 | 3.01 | 2.64 | | 2.65 |
| wrapped, 4 KiB | 56.73 | 26.19 | 5.85 | 3.22 | | 19.10 |
| wrapped, 1 MiB | 48.32 | 27.69 | 5.86 | 3.19 | | 20.85 |
| wrapped, 10 MiB | 39.30 | 26.25 | 5.90 | 3.19 | | 20.22 |

## Against encodify and the base64 crate

With the backend detected here, AVX-512, base64-codec is ahead of encodify 1.0.2 in every section: at 64 bytes 1.10x on unwrapped decoding, 1.95x on wrapped decoding, 1.27x on unwrapped encoding and 2.28x on wrapped encoding; from 4 KiB, 1.5x to 4.1x. On AVX2 alone, encodify's instructions, it is ahead on decoding from 4 KiB (1.11x to 1.13x unwrapped, by its decode kernel one operation shorter and tested by `vptest` per block, 1.36x to 1.60x wrapped, by its periods), on wrapped input at 64 bytes (2.02x decoding, 1.65x encoding), on wrapped encoding (1.37x at 4 KiB), on unwrapped encoding at 64 bytes (1.09x) and from 4 KiB (1.04x to 1.05x, 552 against 579 cycles per call at 4 KiB: the same 11-operation kernel, its steps run across four blocks at a time); it is level on unwrapped decoding at 64 bytes (1.00x here, 43.4 against 43.9 cycles per call). base64-simd 0.8 decodes 64 unwrapped bytes level (6.44 against 6.51 GiB/s) and encodes them a little faster (7.86 against 7.77).

Against what mime-rs runs today, the base64 crate line by line, at 1 MiB: wrapped decoding 29x, wrapped encoding 15x; unwrapped, 14x decoding and 12x encoding. The scalar backend alone is ahead of it in every section: 1.35x decoding and 1.41x encoding unwrapped, 3.6x decoding and 1.8x encoding wrapped at 1 MiB, unwrapped encoding at 64 bytes 3.23 against 3.06 GiB/s; an application should still pass `Base64Cpu::detect()`.

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
- Encode kernels given four blocks a step, AVX2 running each operation across the four before the next, as one block's chain of operations fills the scheduler; AVX-512 unwrapped encoding storing whole cache lines aligned; the one-shot decode taking wrapped input's whole lines, run and tail itself, without building a decoder: unwrapped encoding at 4 KiB went from 154 to 133 cycles per call on AVX-512 and from 571 to 552 on AVX2, at 64 bytes 13% and 12% faster on SSSE3 and scalar; wrapped decoding at 64 bytes from 72 to 57 cycles on AVX-512 and from 69 to 55 on AVX2, 16% to 21% faster on every backend.
- Tried and left out: masked AVX-512 loads and stores for every block (their masked-off bytes cross into the next page at a buffer's end), each block tested apart on AVX-512 and scalar, windows across lines for wrapped encoding (the cairn log of 2026-11-01-a-fresh-look has the numbers); lines as one stream with each block's place tracked at run time, periods looped rather than written out, a last period moved back over lines done, and periods for decoding with 16-character blocks (the cairn log of 2026-11-01-a-closer-look); the one-shot decode through the decoder's loop alone, wrapped input detected from where the first run stops, spare or exact stores chosen per block, scalar quanta loaded as 32-bit words (the cairn log of 2026-11-01-past-the-periods); AVX-512 decode groups packed into three aligned stores, AVX2 decoding several blocks a step at a time, a joined AVX2 error vector, scalar runs without the driver (the cairn log of 2026-11-01-the-last-glue).
