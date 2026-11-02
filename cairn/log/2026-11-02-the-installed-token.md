---
cairn: log
change: the-installed-token
landed: 2026-11-02
---

# The installed token

Landed default codecs on the token the application installs: `Base64Cpu::default()` is `Base64Cpu::new(SimdCpu::current())`, the installed token or `SimdCpu::BASELINE`, and `Base64Codec::default()` takes it with the rest of `BASELINE`. An explicit token stays the `cpu` field; `Base64Codec::BASELINE` and `Base64Cpu::new` are unchanged and `const`. The token is read once when the value is built, never by an encode or decode call.

simd-cpu is pinned at f77308a, the rev holding the install, in Cargo.toml and fuzz/Cargo.toml. tests/installed.rs, alone in its binary since the token is the process's, checks a default codec before and after an install; it fails on the old `const` default. The release assembly of the library is identical to HEAD once crate hashes are normalized, nothing inside the crate calling `Default`, so no bench row moves. Miri was not run: no unsafe code or kernel changed.

Capabilities moved: backends (MODIFIED: CPU detection is the caller's).
