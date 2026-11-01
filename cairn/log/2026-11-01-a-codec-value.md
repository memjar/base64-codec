---
cairn: log
change: a-codec-value
landed: 2026-11-01
---

# A codec value

The user found the one-shot functions hidden and awkward: free functions a reader of `Base64Encode` never met on docs.rs, called as `encode::encode`, taking the CPU token and the options `Base64Encode::new` takes too. Settled with them: the coroutines keep their verb names (naming-006), the configuration becomes a codec value rather than a builder, and public changes are free while the crate is unpublished.

What landed:

- `codec::Base64Codec`, a `Copy` value with the public fields `cpu` and `wrap`, and `BASELINE`, the build target's backend unwrapped, which `Default` also gives; `Base64Eol` moved next to it. An application builds one, usually at start from `Base64Cpu::detect`, and uses it for every call.
- The one-shot calls are its methods, taking `self`: `encode`, `encode_vec` and `encoded_len` in encode.rs, `decode`, `decode_vec` and `decoded_len_max` in decode.rs. The free functions and `Base64EncodeOptions` are gone.
- `Base64Encode::new` and `Base64Decode::new` take a codec, the decoder using its backend alone. Resuming and finishing did not change.
- `encode` and `decode` are `#[inline]`: out of line, `self` takes two argument registers, which pushed the length of `dst` to the stack and shifted every argument on the way to `decode_all`, 5 instructions more a decode call than the free function had. Inlined, the caller goes straight to `encode_all` or `decode_all`.
- The tests, tests/mime.rs, the bench, both fuzz targets, the crate and module docs and the README on the new calls; the driver tests loop over the line endings and build a codec per backend.

Cycles and instructions per call at 64 bytes, `perf stat` on core 13 with its sibling idle, loops and functions aligned, averaged over the eight source and output offset pairs of the-last-glue, the committed tree of the-last-glue in brackets:

| workload | AVX-512 | AVX2 | SSSE3 | scalar |
| --- | --- | --- | --- | --- |
| encode unwrapped | 31.9 (32.0), 200 (218) | 38.1 (37.5), 252 (270) | 44.8 (47.2), 320 (338) | 84.7 (85.4), 503 (521) |
| encode wrapped | 41.4 (43.6), 227 (242) | 53.0 (55.1), 327 (342) | 56.9 (60.4), 357 (372) | 89.2 (91.6), 533 (548) |
| decode unwrapped | 37.7 (38.8), 255 (262) | 42.9 (43.5), 305 (312) | 55.4 (55.8), 406 (413) | 97.7 (98.3), 555 (562) |
| decode wrapped | 58.0 (57.1), 371 (376) | 55.9 (56.0), 339 (344) | 60.7 (59.5), 382 (387) | 89.1 (89.1), 516 (521) |

Fewer instructions on every row; wrapped decoding reads up to 1.2 cycles above on AVX-512 and SSSE3 with 5 instructions fewer, within the layout noise the-last-glue measured. The kernels and loops did not change, so larger inputs were not measured. The library: 2,746 lines before the tests against 2,712, 1,750 without comments and blank lines against 1,742, the codec module and its docs.

Found: qemu-aarch64 10.1.5's default CPU model, `max`, crashed two driver tests of the release build with SIGSEGV, at an unaligned `stp` into mapped, writable memory in the scalar `encode_periods`, which is safe code, the store before it on the same page having gone through. The same binary passed on the cortex-a72, neoverse-n1 and a64fx models and with `-one-insn-per-tb`, and crashed on neoverse-v1 and neoverse-n2: every model tried with FEAT_LSE2 crashed, none without it. The committed tree passed on `max` by its layout. The aarch64 tests run with `QEMU_CPU=neoverse-n1` since.

Checked: the test suite in release with all features, with default features and with `-C target-cpu=native`; the library without default features; clippy for all targets on x86_64, and with `-D warnings` on aarch64; rustdoc with `-D warnings`; the aarch64 suite under qemu-aarch64 on neoverse-n1, release and debug, built for aarch64-unknown-linux-musl, not on hardware; Miri on the library; the decode and encode fuzz targets for 2 minutes each, clean (310,518 and 928,243 runs), before the two `#[inline]` attributes, which changed nothing else.

Capabilities moved: calls (one codec for every call, the length helpers and the `alloc` helpers on it), backends (the token held by the codec), encoding (the scenario's length helper).
