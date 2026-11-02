---
cairn: log
change: the-store-queue
landed: 2026-11-02
---

# The store queue

Took over the optimisation from the handoff of the-last-glue with its measuring method (`perf stat` cycles and instructions per call over two iteration counts, core 13 with its sibling idle, loops and functions aligned, every workload averaged over the eight source and output offset pairs, here the least of three runs per pair), against a copy of HEAD (a478087) and encodify 1.0.2. The user asked for more speed with what quoted-printable-codec had taught, after a review of that crate found its note that `vpermt2b` runs 0.8 a cycle on Zen 5 to be a measuring artifact: its `asm!` loop chained each `vpermt2b` on its own destination, four chains of latency 5. Re-measured, `vpermt2b` and `vpermi2b` run 2 a cycle as `vpermb` does; `vpcompressb` and `vpexpandb` run 1 a cycle with a false dependency on their destination. That reopened lever 1 of the handoff. AVX-512 wrapped encoding now runs 14% to 16% faster from 2 KiB up with CRLF and 10% to 15% with LF; every other row runs the same code as HEAD.

What landed:

- AVX-512 wrapped encoding of 32 lines or more in cycles: 32 lines ending with CRLF (2,496 bytes) or 64 ending with LF (4,928 bytes) are whole 64-byte vectors, 39 and 77, drawn from 38 and 76 encoded blocks. Each output vector is one `vpermt2b` of the two consecutive encoded blocks it draws from, masked by bit 7 of its index (the mask2 form), its line ending bytes taken from the index where the mask is clear: one index vector a vector, built at compile time (`wraps`), and one store per 64 bytes where periods took 35 a period of 19.5. Blocks are encoded in order and carried in two registers; exactly one vector a cycle draws from the same two blocks as the one before (`stay`, found at compile time), so the loop compares an index and steps 48 bytes. Each block is loaded as the 64 bytes ending it, its spread indices 16 on (`encode_vector` takes the lead), so that the last block of an input ending on a line still loads. Lines past the whole vectors go to `encode_lines`.
- `base64_driver!` takes an optional `periods:` function for encoding, which the line loop calls in place of the stamped `encode_periods`; AVX-512 names `encode_wrapped`, an inlined dispatch sending fewer than 32 lines to `encode_periods` and more to `encode_cycles`, so small calls keep their path and their instruction counts.
- A test of inputs ending around one and two cycles of each line ending, encoded at once and resumed in two parts on every backend; the suite's lengths stop at 1,024 bytes, below a cycle. A deliberate break of the CR byte in the index and one of the `stay` position each fail it.

Tried and left out:

- Each output vector assembled straight from a 64-byte input window by a per-vector spread and multishift table, no encoded blocks: two table loads and an unaligned load a vector, level with HEAD at 4 KiB and up to 11% slower from 1 to 2 KiB. Its window offsets read from the table made it 1.4x slower again, as quoted-printable-codec's data-dependent cursor did, and a division by 39 on `usize` compiled to a 128-bit `mul` with fix-ups: a 32-bit division is a multiply and a shift.
- The vector's block pair worked out per vector from the character count: 29 instructions a vector, twice HEAD's cycles.
- Masked loads of the 48 bytes of each block: 5% slower at 16 KiB; plain 64-byte loads with the masked one at the input's end only: the branch in the loop broke its shape, 7,628 instructions at 16 KiB against 5,615.
- The last partial vector of the lines stored masked from slices of the output: 2,504 instructions at 4 KiB against 1,670.
- `encode_cycles` inlined into the line loop: small calls 5% to 15% slower; the cycle loop below 32 lines: 7% to 13% slower at 16 and 17 lines, which end in a line done apart.
- Wrapped decoding the same way, each block of 64 characters one `vpermt2b` of the two input vectors it lies in (always vectors b and b + 1 of the cycle), each vector's line endings compared with the encoder's index of its place: 405 to 451 cycles at 4 KiB against 285 to 381 for HEAD. Its chain per block (load, gather, table lookup, the multiplies and the pack) is about 30 cycles of eleven vector operations, and the FP scheduler is full 21% of the cycles against 6% for HEAD; checking the line endings as the periods do, in scalar code per cycle, compiled to 5,549 instructions. The handoff the-gathered-blocks keeps the numbers.

Cycles per call, `perf stat`, the tree before (HEAD) in brackets; the rows not listed run identical code (every function of every backend but the AVX-512 `encode_lines` is the same in the release assembly once addresses are normalized):

| workload | AVX-512 | encodify |
| --- | --- | --- |
| encode wrapped, 64 B | 42.5 (42.1) | 109.4 |
| encode wrapped, 600 B | 61.2 (64.4) | |
| encode wrapped, 1 KiB | 99.0 (100.9) | |
| encode wrapped, 1,824 B | 128.6 (141.4) | |
| encode wrapped, 2 KiB | 148.4 (161.9) | |
| encode wrapped, 4 KiB | 230.6 (273.2) | 934.1 |
| encode wrapped, 16 KiB | 849.1 (983.1) | |
| encode wrapped, 64 KiB | 3,631 (4,236) | |
| encode wrapped LF, 2 KiB | 148.6 (158.2) | |
| encode wrapped LF, 4 KiB | 235.8 (278.7) | |
| encode wrapped LF, 16 KiB | 846.6 (979.1) | |
| encode wrapped LF, 64 KiB | 3,690 (4,113) | |

The unchanged rows read within 4% of HEAD in the same runs (unwrapped encoding at 4 KiB 134.6 against 130.0, wrapped decoding 336.5 against 328.3), with the same instructions: layout. Instructions per call grew from 32 lines on (1,670 against 1,087 at 4 KiB), the vectors being fewer but each loop step heavier; the cycles are what moved. The loop runs about 2 cycles a vector against an `asm!` replica's 1.55 for one table load and an unaligned input load a vector.

Criterion, the median of three runs of the wrapped encoding group (README.md and benches/README.md): base64-codec at 64 bytes, 4 KiB, 1 MiB and 10 MiB 5.67, 60.71, 47.33 and 38.32 GiB/s against the published 6.04, 56.73, 48.32 and 39.30; 4 KiB 7% faster, the memory-bound sizes level, and 64 bytes level by cycle counts, every column of the run reading about 5% under the published one at 64 bytes, encodify included.

The library: 2,806 lines before the tests against 2,674, 1,978 without comments and blank lines against 1,884: the cycle encoder, its tables and their builder.

Checked: the suite in release, in debug with all features, without default features (simd-cpu alone, without `std`) and with `-C target-cpu=native`; clippy with `-D warnings` for all targets, and for the library without default features; rustdoc with `-D warnings`, private items included; the aarch64 suite in release under qemu-aarch64 on neoverse-n1 and clippy with `-D warnings` for aarch64-unknown-linux-musl; the encode fuzz target for 3 minutes with inputs up to 8 KiB, 1,258,906 runs, clean; every wrapped length around the cycles against the harness's reference at three offset pairs. Miri was not run: it has no AVX-512, and every other backend compiles to the same code.

Capabilities moved: none, speed only.
