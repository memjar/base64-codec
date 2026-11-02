---
cairn: change
id: the-installed-token
status: landed
created: 2026-11-02
---

# The installed token

## Why

simd-cpu gained a process-wide token on 2026-11-02 (its change an-installed-token): the application calls `SimdCpu::install_detected()` once, first thing in `main`, and libraries read `SimdCpu::current()`, the installed token or `SimdCpu::BASELINE`. Today a `Base64Codec` runs SIMD only when every layer between the application and the codec hands a `SimdCpu` down; mime-rs grew a `cpu` field on its options and on every entity for that alone. A codec built with its defaults should run the installed token instead.

## What

Settled with the user on 2026-11-02: an explicit token wins, and without one the codec takes the installed token, or the baseline when none is installed, with no error and no log.

- `Base64Cpu::default()` returns `Base64Cpu::new(SimdCpu::current())` instead of the baseline backend, and `Base64Codec::default()` takes it, the standard alphabet, padded and unwrapped, as `BASELINE` otherwise.
- An explicit token stays a struct field: `Base64Codec { cpu: Base64Cpu::new(token), ..Base64Codec::default() }`. The codec value holds the resolved backend, never an `Option`, so the token is read once, when the value is built, and no encode or decode call touches the static: per-call resolution would add an atomic load and a backend match to every call, and lose the backend being known to the compiler on small inputs, which the passes tune at a few cycles. An install after a codec value was built does not reach that value.
- `Base64Codec::BASELINE` and `Base64Cpu::new` stay `const` and unchanged: `BASELINE` is the deterministic codec, for consts and for tests that need the build target's backend.
- The crate header, the docs of `Base64Cpu` and `Base64Codec`, and the README say the application installs the token once and libraries take the defaults, or pass a token they were given.
- simd-cpu's pin moves to the rev that holds `install`, in Cargo.toml (dependency and dev-dependency) and fuzz/Cargo.toml; quoted-printable-codec and mime-rs move to the same rev, so one static serves them all.

## Risks

- `Default` now calls a run-time function: a run-time `new` inside `Default` once pushed quoted-printable-codec's decoder constructor past rustc's cross-crate inline threshold. The release assembly of `Base64Encode::new`, `Base64Decode::new` and the one-shot calls is diffed function by function against HEAD, and the bench's small rows compared, as the-store-queue's method says.
- Tests share the process's static: the install is tested in its own integration test binary, tests/installed.rs, so no unit test sees an installed token, and the unit tests keep passing explicit tokens.
