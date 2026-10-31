---
cairn: log
change: a-simd-base64-codec
landed: 2026-10-31
---

# A SIMD base64 codec

Built the crate: `Base64Cpu` and `Base64Backend`, the resumable `Base64Encode` and `Base64Decode` with the one-shot `encode` and `decode` over them, the length helpers and the `alloc` helpers; the scalar steps with mime-rs's lenient rules; one driver macro stamped into the scalar, SSSE3, AVX2 and NEON modules; the tests, the fuzz crate, the bench, the CI workflow and ATTRIBUTION.md. About 1,900 lines of library code with their docs, against encodify's 4,500 for base64, and no dependency.

Checked: the unit tests compare every backend with the scalar steps at every length up to 1,024, wrapped, rewrapped and with a stray byte, every byte value at every position of a block, and every split of an input into two calls; tests/mime.rs compares every backend with a frozen copy of mime-rs's wrapper over 20,000 generated and mutated inputs, identical bytes and flags. Miri passed on the scalar backend and, with `-C target-feature=+avx2`, on the SSSE3 and AVX2 kernels. The decode and encode fuzz targets ran five minutes each, about 2.4 million runs, clean. The NEON backend passed the whole suite cross-built for aarch64-unknown-linux-musl under qemu-aarch64 user mode, not on hardware.

What moved from the plan:

- The input ends with `finish`, not with an empty chunk: an empty chunk is a normal event mid-stream (an empty `BodyChunk`), and it must not end a body by accident.
- The SSSE3 and AVX2 kernels are two short sets, AVX2 reusing the SSSE3 narrow kernels, rather than one macro over the vector type, which took more lines than the few it would have shared.
- The scalar backend also runs the driver, with table-lookup block kernels, so wrapped input gets its line runs on every target.
- The NEON kernels carry `#[target_feature(enable = "neon")]` and their dispatch is `unsafe`: an intrinsic is safe only inside a function with the attribute, even when the target's baseline has the feature.
- A wide kernel may write spare bytes past its own (AVX2 decodes 24 bytes with two overlapping 16-byte stores), so it is handed the rest of the output and the narrow kernels finish where that has no room; decoding may overwrite `dst` past the bytes it reports written.
- The line shape is read after the first line feed rather than from the start of every input, which a scan of the first kilobyte cost on every unwrapped input.
- `Base64Cpu` implements `Default` as `BASELINE`, for option structs that derive it.
- The spec splits into four capabilities rather than three: encoding, decoding, calls and backends.

Against encodify 1.0.2 at 1 MiB and 10 MiB (benches/README.md), the gate is met on unwrapped decoding (1.01x, 1.03x) and wrapped encoding (0.99x, 0.98x) and missed on wrapped decoding (0.89x, 0.91x) and unwrapped encoding (0.80x, 0.82x); every workload runs at about half its speed at 64 bytes. Against the base64 crate line by line, mime-rs's backend today, wrapped decoding is 12.9x and wrapped encoding 6.4x. The levers left went to the-last-speed-levers.

Left for the user: git init, the GitHub repository and the first CI run, which the workflow waits for. Not done: the mime-rs change switching its `base64` feature to this crate, since another session had uncommitted work in the mime-rs files it touches.

Capabilities moved: encoding, decoding, calls, backends (all ADDED).
