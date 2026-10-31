---
cairn: change
id: a-simd-base64-codec
status: landed
created: 2026-10-31
---

# A SIMD base64 codec

The plan for a small crate, working name base64-codec (free on crates.io on 2026-10-31), that encodes and decodes base64 with SIMD, depends on nothing, does no I/O and keeps the strict minimum MIME needs. An agent starting on it reads this file, then delta.md (the behaviour to reach), tasks.md and the references below. Capabilities: encoding, decoding, calls and backends, all ADDED, folded into cairn/spec/encoding.md, decoding.md, calls.md and backends.md when the change lands.

## Why

mime-rs decodes base64 through the scalar base64 crate, line by line (src/codec/base64.rs), and its `full` workload trails mail-parser 1.0 (0.75x on the vendored corpus, worse expected on large attachments), whose codec, encodify, uses SIMD. encodify cannot replace it as is: it is about 18,000 lines (base32, PEM, hex, percent and xtext escapes, UTF-7, DKIM, streaming), depends on memchr, and needs std for its `io::Write` streaming and for `is_x86_feature_detected!`, which it runs on every call. What MIME needs is its base64 kernels, about 2,500 of its 4,500 non-test base64 lines, and much of those is line handling written again for each backend.

## References to read first

- encodify 1.0.2 (Stalwart Labs LLC, Apache-2.0 OR MIT), in ~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/encodify-1.0.2/src/base64: kernel/x86.rs (SSSE3 and AVX2: `decode` with its nibble-table validation, `roll` translation and `maddubs` then `madd` packing, the AVX2 `permutevar8x32` compaction in `decode_exact`, `encode` with its shuffle spread, two `mullo` and the saturating-subtract offset lookup, `load12` and `load_pair`), kernel/neon.rs (`decode_block` and `encode_block` over `vld4q_u8`, `vqtbl4q_u8` and `vst3q_u8`, and the 16-character `decode_quarter` and `encode_quarter`), kernel/mod.rs (`LineShape::detect`, `decode_lines`, `encode_lines`), decode/lenient.rs (`Quantum`, the carry a resumable decoder keeps).
- simdutf8 0.1.5 (Hans Kratz, MIT OR Apache-2.0), in the same registry: implementation/algorithm.rs (`algorithm_simd!`, the algorithm written once as a macro and stamped into each backend module so it compiles under that backend's `#[target_feature]` and its primitives inline), implementation/x86/avx2.rs (the primitives), implementation/x86/mod.rs (dispatch: an `AtomicPtr` cache with std, `cfg(target_feature)` without), implementation/helpers.rs (the scalar tail).
- mime-rs src/codec/base64.rs: the lenient semantics to reproduce and the tests to port.
- Wojciech Muła and Daniel Lemire, "Faster Base64 Encoding and Decoding using AVX2 Instructions" (ACM Transactions on the Web, 2018): the x86 method.

## What

### Scope, and only this

- The RFC 4648 4 standard alphabet, nothing else: no URL-safe alphabet, no base32, no other escape.
- Encoding: padded output, unwrapped (RFC 2047 B encoded words) or in lines of 76 characters ended by CRLF or LF (RFC 2045 6.8), the last line ended too, as mime-rs writes it today. The width is not a parameter: MIME fixes it, and nothing else needs another yet.
- Decoding, lenient exactly as mime-rs's wrapper is today, so its tests carry over: CR, LF, SP and HTAB skipped silently; any other byte outside the alphabet (VT and FF included, which encodify skips silently) skipped and reported; missing padding and non-zero trailing bits accepted; data after padding decoded as a second run and reported; a lone trailing symbol dropped and reported. The result is the bytes and a clean flag.
- Length helpers: the exact encoded length and a decoded length every lenient reading fits in.

### Nothing is read or written but the caller's buffers

The crate does no I/O and owns no buffer. encodify's `io::Write` streaming becomes the shape io-imap's streaming FETCH already has: the caller holds the bytes, hands them over, and resumes. A resumable encoder and decoder carry at most one partial quantum between calls, so a body can be decoded as it arrives (from an `ImapMessageFetchStreamYield::BodyChunk`, say) or encoded straight into a socket buffer, with no `std::io` and no copy of the whole content.

### API

- `Base64Cpu`, the CPU token (below).
- `Base64Encode::new(cpu, opts)` with `Base64EncodeOptions { wrap: Option<Base64Eol> }` (unwrapped by default) and `Base64Eol::{Crlf, Lf}`; `Base64Decode::new(cpu)`. Each `resume(src, dst)` consumes what fits in `dst` and returns the bytes read and written. The state holds at most three symbols, the padding seen and the clean flag when decoding, at most two bytes and the line column when encoding. An empty `src` ends the input, as an empty read ends one in io-imap: the decoder flushes its last quantum and returns the clean flag, the encoder writes its padding and last line ending.
- One-shot `encode(src, dst, cpu, opts) -> Option<usize>` and `decode(src, dst, cpu) -> Option<Base64DecodeOutput>` (`written`, `clean`): that loop run once, `None` without writing anything when `dst` is shorter than the length helper asks.
- `encoded_len(len, opts) -> Option<usize>`, exact, `None` on overflow; `decoded_len_max(len) -> usize`, `len / 4 * 3 + len % 4 * 3 / 4`, which no lenient reading exceeds.
- Behind an `alloc` feature: `encode_vec` and `decode_vec`, appending to a `Vec` through its spare capacity. Inside, the kernels write into `&mut [MaybeUninit<u8>]` as encodify's do, so nothing is zeroed first, and the `&mut [u8]` entry points cast once into that form (sound, since the codec only ever writes initialised bytes).

### CPU detection is the caller's

`Base64Cpu` holds the backend to run, a `Base64Backend`: `Scalar`, plus `Ssse3` and `Avx2` on x86 and x86_64 and `Neon` on aarch64, each variant gated on its architecture as core::arch is. The token cannot be built from a plain value, since running an AVX2 instruction on a CPU without it is undefined behaviour:

- `Base64Cpu::BASELINE`, a `const`: the best backend the build target guarantees, read from `cfg(target_feature)` as simdutf8's no-std path does (scalar on a default x86_64 build, AVX2 under `-C target-cpu=native` on an AVX2 machine, NEON on aarch64);
- `unsafe fn Base64Cpu::assume(backend)`: for a caller that knows its hardware, and for tests forcing each backend;
- `Base64Cpu::detect()`, behind a `std` feature off by default, for the application to call once and pass down;
- `backend()`, to read back which one runs.

Each call matches on the token once and runs its whole loop inside the chosen backend's `#[target_feature]` function. No function checks the CPU itself, so no call pays for detection.

### Backends

- Scalar: table-driven, safe code, the reference every other backend matches byte for byte and flag for flag. It is the only code that handles anything but whole quads of alphabet characters: padding, skipped and reported bytes, irregular lines, the carry across `resume` calls and every tail shorter than a block.
- SSSE3 and AVX2: encodify's x86 method, the same steps at 128 and 256 bits, so written once as a macro over the vector type; the steps that differ (AVX2's cross-lane compaction after packing, its 24-byte load before spreading) are per-backend primitives.
- NEON: encodify's NEON method, which is not the x86 one. `vld4q_u8` deinterleaves 64 characters into four registers, `vqtbl4q_u8` and `vqtbx4q_u8` lookups validate and translate them, shifts pack them and `vst3q_u8` stores 48 bytes, the reverse to encode. Forcing NEON through x86's `maddubs` packing would cost it that speed, so its block kernels are its own.
- Not now: AVX-512 VBMI, WASM simd128.

### One driver over block kernels

What is written once for every backend is the driver: the loops, the line handling and the hand-over to scalar. Following simdutf8, it is a macro instantiated inside each backend module, under that backend's `#[target_feature]`, so the block kernels inline into it; a generic function or a trait would not inline across the feature boundary, and a call per block would cost more than the block. Each backend supplies four block kernels: decode and encode, wide (16 characters for SSSE3, 32 for AVX2, 64 for NEON) and narrow (16 characters, 12 bytes). They take fixed-size array references, so the driver checks a block's bounds once and each load's or store's SAFETY is the array's length. A decode block also says whether all its characters were in the alphabet.

- Unwrapped input: wide blocks, then narrow ones, while the input lasts and the blocks are valid.
- Line runs: the driver reads the line shape from the first two lines, a width that is a multiple of 4 and at least 16 characters and a line ending, as encodify's `LineShape::detect` does. Each line of that shape is decoded with wide then narrow blocks, its remainder with one narrow block ending exactly at the line's end, overlapping characters already decoded: a quad decodes to the same bytes wherever it falls, so rewriting them is harmless. A 76-character line costs two AVX2 blocks and one narrow block (80 characters of work) and no line kernel of its own, where encodify writes three (direct, batched and compact) per architecture.
- Wrapped encoding: the same, 57 input bytes per line with the last block overlapping, then the line ending.
- An invalid block or a line off the shape goes to scalar, and the driver goes back to blocks at the next point where scalar holds no partial quantum: the next line start in a line run, the next block otherwise. Padding makes its block invalid, so it always reaches scalar.

The goal is about 1,500 lines of library code in all, tests apart.

### Safety and quality

- `#![no_std]`, no dependency at all, `alloc` and `std` features off by default; edition 2024 and rust-version 1.88 as mime-rs (safe `#[target_feature]` functions need 1.86, safe intrinsics inside them 1.87).
- `unsafe` only for the loads and stores in the block kernels, the one `MaybeUninit` cast and the call into a backend after the token's match, each block with its SAFETY comment; `unsafe_op_in_unsafe_fn` and `clippy::undocumented_unsafe_blocks` denied.
- Ported code keeps its origin: an ATTRIBUTION.md at the root names encodify 1.0.2 and simdutf8 0.1.5, their authors, their licences and the files derived from each, as mime-rs's vendored corpora do; no per-file licence header.
- Tests: the RFC 4648 10 vectors; every length from 0 to 1,024, wrapped and not, on every backend the machine has, against scalar; the lenient cases of mime-rs's wrapper; every input split at every position into two `resume` calls, against the one-shot result.
- Fuzz targets: every backend against scalar, for encode and decode; decode against mime-rs's current wrapper (copied into the fuzz crate over the base64 crate), identical bytes and flag, which proves the switch changes nothing for mime-rs; encode then decode returns the input; `resume` over random chunk sizes against the one-shot result.
- Miri on scalar, and on the x86 kernels under the matching `-C target-feature` as far as Miri implements their intrinsics; CI on x86_64 and on an aarch64 runner.
- Benchmarks against the base64 crate, base64-simd and encodify, in MiB/s, at 64 bytes, 4 KiB, 1 MiB and 10 MiB, wrapped and not. The gate: at 1 MiB and 10 MiB, at least encodify's throughput on this machine's AVX2 and on the aarch64 runner; a miss is profiled before mime-rs switches.

### How mime-rs uses it

mime-rs's `base64` feature switches from the base64 crate to this one, as a mime-rs change of its own, which also rewrites D4 in its a-mime-library proposal (it still names encodify as the codec backend). mime-rs has no decoding options yet (src/tree/entity.rs calls the wrapper directly), so how a caller's token reaches the leaf accessors is that change's decision; `Base64Cpu::BASELINE` is the default either way, so a caller who never passes a token still decodes correctly.

## Open questions

- The name: base64-codec, or a broader name if the crate is to grow quoted-printable and the search kernels later.
- Whether the decoder reports the first bad position, which encodify does and mime-rs does not need today.
