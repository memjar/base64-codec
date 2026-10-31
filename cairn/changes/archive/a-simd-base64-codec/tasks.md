---
cairn: tasks
change: a-simd-base64-codec
---

- [ ] Settle the name, then git init and the GitHub repository (user)
- [x] Scaffold from mime-rs: Cargo.toml (edition 2024, rust-version 1.88, no dependencies, `alloc` and `std` features), nix files, licences, ATTRIBUTION.md, deny.toml, lib.rs with `#![no_std]` and the lint denials
- [x] `Base64Cpu` and `Base64Backend` with `BASELINE`, `assume`, `detect` and `backend`
- [x] Scalar encoder and decoder as `Base64Encode` and `Base64Decode` with `resume` (and `finish`, see the log), the lenient rules, the one-shot `encode` and `decode` over them, the length helpers and the `alloc` helpers
- [x] Port mime-rs's src/codec/base64.rs tests, the RFC 4648 vectors and the delta's scenarios; the split-at-every-position test
- [x] Driver macro: unwrapped blocks, line shape detection, line runs with the overlapping narrow block, wrapped encoding, hand-over to scalar and back
- [x] SSSE3 and AVX2 block kernels after encodify's x86.rs (two short kernel sets, AVX2 reusing the SSSE3 narrow ones, rather than one macro over the vector type: see the log)
- [x] NEON block kernels after encodify's neon.rs, tested under qemu-aarch64 user mode
- [x] Backend equivalence tests at every length up to 1,024; fuzz targets (backends against scalar, round trip, random chunking; the mime-rs oracle in tests/mime.rs); Miri on scalar and the x86 kernels
- [ ] CI on x86_64 and aarch64: .github/workflows/tests.yml is written, its first run waits for the repository
- [x] Benchmarks against base64, base64-simd and encodify; the gate is met on unwrapped decoding and wrapped encoding and missed on the rest, carried to the-last-speed-levers
- [ ] A mime-rs change switching its `base64` feature to the crate, deciding how the token reaches its leaf accessors, and rewriting D4: not started, mime-rs has another session's uncommitted work in progress in the files it touches
