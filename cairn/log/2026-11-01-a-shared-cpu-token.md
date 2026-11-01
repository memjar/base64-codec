---
cairn: log
change: a-shared-cpu-token
landed: 2026-11-01
---

# A shared CPU token

The user asked whether the CPU detection base64-codec and quoted-printable-codec each carried behind `std` should move to a small crate that base32, UTF-7 and later codecs would share. Settled with them: yes, as simd-cpu, holding features rather than backends, since each codec needs a different set (base64-codec AVX-512 VBMI, AVX2, SSSE3 or NEON; quoted-printable-codec AVX-512 VBMI2 with VBMI, BMI1, BMI2, LZCNT and POPCNT).

What landed:

- simd-cpu 0.0.1, a new repository at ~/code/pimalaya/simd-cpu: `SimdCpu` with `NONE`, `BASELINE`, `detect` behind `std`, `has`, the unsafe `with` and the safe `without`, and the `#[non_exhaustive]` `SimdFeature`.
- `Base64Cpu::new(SimdCpu)`, a `const fn` picking the widest backend; `Default`, in a `const` block, and `Base64Codec::BASELINE` build it from `SimdCpu::BASELINE`: called at run time in `Default`, `new` pushed quoted-printable-codec's decoder constructor past rustc's cross-crate inlining threshold (its log of the same day). `Base64Cpu::BASELINE`, `detect`, `assume` and the `std` feature are gone; `backend` and `Base64Backend` stay.
- The unit tests, tests/mime.rs, the bench and both fuzz targets list their backends by removing features from `SimdCpu::detect` in turn, with no unsafe code and no per-architecture twin; the bench needs no feature any more.
- The dependency is pinned by git revision to simd-cpu 4cde2e9 on GitHub, with its version for publishing; the fuzz crate pins the same revision.

The release assembly of the library is identical to the committed tree once symbol hashes are normalized (18,838 lines, no difference), so the cycle counts of past passes stand.

Checked: the test suite in release with all features, without default features and with `-C target-cpu=native`; the backends listed on this machine, AVX-512, AVX2, SSSE3 and scalar; clippy with `-D warnings` for all targets and for the library without default features, on x86_64 and on aarch64-unknown-linux-musl; the aarch64 suite in release under qemu-aarch64 on neoverse-n1; missing docs and rustdoc with `-D warnings`; the bench and the fuzz targets built, not run. Miri and fuzzing were not rerun, no unsafe code or kernel having changed.

Capabilities moved: backends (the token built from a `SimdCpu`), calls (one dependency, no std).
