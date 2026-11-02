---
cairn: tasks
change: the-installed-token
---

- [x] simd-cpu's an-installed-token committed and pushed (f77308a); its rev pinned in Cargo.toml and fuzz/Cargo.toml
- [x] `Base64Cpu::default()` and `Base64Codec::default()` on `SimdCpu::current()`
- [x] tests/installed.rs: a default codec runs the baseline backend before an install and the installed token's after, and a codec built before the install keeps its backend
- [x] Release assembly diffed against HEAD: identical once crate hashes are normalized, since nothing inside the crate calls `Default`; the coroutine constructors and one-shot calls are untouched, so no bench row moves
- [x] Crate header, `Base64Cpu` and `Base64Codec` docs, README, CHANGELOG.md
- [x] Tests, clippy (aarch64 too), docs; Miri skipped, no unsafe code or kernel touched
- [x] Spec folded, log entry, change archived
