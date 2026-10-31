---
cairn: change
id: the-last-speed-levers
status: landed
created: 2026-10-31
---

# The last speed levers

What is left between base64-codec and encodify 1.0.2 after a-simd-base64-codec landed, each lever measured on the bench before it is kept, and kept only if it pays without bringing back encodify's per-architecture line kernels. The numbers are in benches/README.md.

## Why

The first version met encodify at 1 MiB on unwrapped decoding and wrapped encoding, and missed it by 10 to 20% on the rest: unwrapped encoding at 0.8x, wrapped decoding at 0.9x, every workload at 64 bytes at about 0.5x. The scalar backend, which `Base64Cpu::BASELINE` runs on a default x86_64 build, decodes unwrapped input at 0.66x the base64 crate.

Profiled on 2026-10-31 (perf on the bench, exact instruction counts per call from perf stat and callgrind, the generated assembly read), the gap is in the driver and the kernel contract, not in per-architecture specialisation:

- Unwrapped encoding: the AVX2 loop runs 19 instructions per 24-byte block against encodify's 13.5. It loads with two loads and a `vinserti128` where encodify loads 32 bytes from 4 bytes before the group; it needs a `vpand` for its junk-bit multipliers where encodify hides exact ones from LLVM behind `black_box`; it spends 4 loop instructions per block where encodify unrolls 8 times. 13.5/19 is the 0.72x measured at 4 KiB.
- Wrapped decoding: about 111 instructions per 78-byte line against encodify's 83. Each block tests its own validity (`vpcmpeqq`, `vmovmskpd`, `sete`: LLVM's rewrite of the `vptest`) where encodify ORs the error vectors of a line and tests once; the narrow block for the line's last 12 characters runs as a loop with bounds checks and reloads its constants from memory; 7 driver variables are spilled and reloaded per line.
- Unwrapped decoding: 26 instructions per 32-character block, 4 of them re-checking the room in `dst` and 6 updating counters, and a branch on validity per block, where encodify unrolls 4 blocks and tests once. It wins at 1 MiB, which is memory-bound, and loses at 4 KiB (0.88x).
- Small inputs: at 64 bytes base64-codec runs 685 instructions per unwrapped decode (encodify 323), 1,735 per wrapped decode (756) and 566 per unwrapped encode (275). `decode_bytes` costs about 45 instructions per byte, and it gets the 8-character tail of an unwrapped input and about 28 bytes of a wrapped one, since a kernel's `bool` drops a whole block at the first line break. `encode_bytes` copies its carry with runtime-length `copy_from_slice`: 60 instructions of `memcpy` per call.
- The bench compares the lenient decoder with encodify's strict `STANDARD` on unwrapped input. Against encodify's lenient `MIME` on the same input the 64-byte gap is 98 against 67 cycles per call (0.69x), not 0.5x.
- Every driver loop is inlined into one `decode` (or `encode`) per backend, so register allocation is shared: a change to the line path alone moved 5 constants of the unwrapped block loop to memory operands, -6% at 1 MiB.

## What

Prototyped on a throwaway copy of the crate on 2026-10-31, the whole test suite passing. GiB/s, criterion medians on the machine of benches/README.md; before is this session's run of the current tree, except the 4 KiB encodings and wrapped decoding, taken from benches/README.md; encodify from the same runs as the prototype:

| workload | before | prototype | encodify |
| --- | --- | --- | --- |
| encode unwrapped, 64 B | 3.16 | 4.05 | 6.33 |
| encode unwrapped, 4 KiB | 23.44 | 31.24 | 33.09 |
| encode unwrapped, 1 MiB | 26.19 | 33.27 | 33.10 |
| encode wrapped, 64 B | 2.77 | 3.37 | 2.87 |
| encode wrapped, 4 KiB | 17.68 | 18.03 | 20.49 |
| encode wrapped, 1 MiB | 21.52 | 21.85 | 22.20 |
| decode unwrapped, 64 B | 3.23 | 3.49 | 6.36 |
| decode unwrapped, 4 KiB | 24.41 | 26.3 | 27.99 |
| decode unwrapped, 1 MiB | 28.82 | 29.1 | 28.04 |
| decode wrapped, 64 B | 1.29 | 1.89 | 2.66 |
| decode wrapped, 4 KiB | 14.43 | 18.5 | 15.84 |
| decode wrapped, 1 MiB | 18.16 | 22.6 | 20.48 |

The prototype took shortcuts (an AVX2-only encode run and 76-character line run beside the driver, raw pointers); the levers themselves are generic over the backends, the driver and the kernel contract:

- Each run loop its own `#[inline(never)]` function in the driver macro (block runs, line runs, encode runs), so that a loop keeps its constants in registers whatever the rest of the driver does. Measured on the unwrapped decode loop: back to its registers, from 27.2 to 29.1 GiB/s at 1 MiB.
- Encode kernels built once per run, their multipliers passed through `core::hint::black_box` outside the loop, so that LLVM keeps one `vpmullw` per field and the mask goes: +10% on unwrapped encoding. `black_box` is best effort, which is harmless here since the bytes are the same whatever LLVM does; an empty `asm!` with `pure, nomem` is the guaranteed way to hide a value, if it ever stops hiding it.
- AVX2 encode loading 32 bytes from 4 bytes before the group (a lead the driver knows, zero for the backends that need none), the first block permuted from a load at the start and the last one overlapping the one before: +13%. With an unroll by 8 (+6% at 4 KiB, +1% at 1 MiB), unwrapped encoding reaches encodify.
- Decode kernels returning their error vector rather than a bool, OR'd over a line or an unrolled group and tested once, as simdutf8 and encodify do; a group that fails is decoded again block by block, off the fast path. The last block of a line is a wide one overlapping the one before when `dst` has its 4 spare bytes past the line, the narrow kernel kept for the last line. Wrapped decoding passes encodify.
- The block counts that fit `src` and `dst` computed once before a run, not checked per block.
- A scalar step of whole quanta (four `CLASSES` lookups, one test) before `decode_bytes`, so that only padding, line breaks and stray bytes go byte by byte: wrapped decoding at 64 bytes from 1.29 to 1.89 GiB/s.
- The carry of `encode_bytes` moved as fixed-size values: wrapped encoding at 64 bytes from 2.77 to 3.37 GiB/s, ahead of encodify.
- A line break between runs skipped by the driver (CRLF or LF, the next line started), and a fast lenient tail: when the rest of the input is at most three symbols followed only by padding and blanks, as nearly every input ends, it is folded into the decoder state in one pass, leaving exactly the state `decode_bytes` would (bits, pending, padded, line start), then `decode_finish` flushes it. `decode_bytes` then runs only on irregular input.
- A one-shot encode, `encode_once` per backend, which `encode` and `encode_vec` run with no `EncodeState`: whole lines, the whole groups left (the wide run, narrow blocks with the last one overlapping, groups below one narrow block), the padded tail and the last line ending, written as fixed-size values (a runtime-length `write` there cost a `memcpy` call per call). The resumable `Base64Encode` keeps the carry. A one-shot decode would save at most the 74 instructions of `decode_all` and `decode` around the driver, since lenient decoding needs its state anyway: not tried.
- Not measured yet: kernels reporting their first bad character, so that a block commits its quanta before a line break (encodify's `Valid::stop`); the encode line path with the hidden multipliers and an overlapping last block; the unroll and deferred test on unwrapped decoding; the line shape detected without a byte scan of the second line, which most of the 64-byte wrapped decode left is; the same contract on SSSE3 and NEON.

With the tail and the one-shot encode on top of the levers above, per call on the same machine (loaded, so cycles are the best of 5 runs pinned to a core with an idle sibling; instructions are exact), before, prototype, then encodify:

| workload | before | prototype | encodify |
| --- | --- | --- | --- |
| decode unwrapped, 64 B | 96 cycles, 684 instr | 73, 518 | 48, 323 (strict); 76, 452 (lenient `MIME`) |
| decode unwrapped, 4 KiB | 751, 5,051 | 767, 4,886 | 698, 3,395 (strict); 750, 3,713 (lenient) |
| decode wrapped, 64 B | 240, 1,734 | 141, 944 | 127, 756 |
| decode wrapped, 4 KiB | 1,293, 9,777 | 1,028, 5,518 | 1,121, 7,147 |
| encode unwrapped, 64 B | 96, 566 | 52, 326 | 45, 275 |
| encode unwrapped, 4 KiB | 811, 3,758 | 585, 2,601 | 581, 2,551 |
| encode wrapped, 64 B | 110, 616 | 75, 392 | 110, 659 |
| encode wrapped, 4 KiB | 1,074, 4,956 | 891, 3,955 | 950, 5,616 |

Against encodify's own lenient decoder, the one that matches base64-codec's semantics, the prototype is level on unwrapped decoding at 64 bytes (73 against 76 cycles in this run, 75 against 70 in another) and at 4 KiB, with more instructions; the strict `STANDARD` the bench compares it with stays 35% ahead at 64 bytes and 9% at 4 KiB, having no state at all. Encoding at 64 bytes is 0.87x unwrapped and 1.47x wrapped, wrapped decoding at 64 bytes 0.90x, and every 4 KiB workload but unwrapped decoding is level or ahead.
- Scalar unwrapped decoding: two quanta per step into one 64-bit word, as the base64 crate's `decode_chunk_8` does.
- AVX-512 VBMI, which this machine has: 64 characters per step through `vpermb`, out of scope for the first version.

Nothing in the spec moves: these are speed only, so there is no delta.
