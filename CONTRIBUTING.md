# Contributing guide

Thank you for investing your time in contributing to base64-codec.

Whether you are a human or an AI agent, read these in order before touching the code:

1. the [Pimalaya README](https://github.com/pimalaya) for what the project is and how its repositories stack;
2. the [Pimalaya CONTRIBUTING](https://github.com/pimalaya/.github/blob/master/CONTRIBUTING.md) guide, which chains to the shared architecture and guidelines;
3. the inline header documentation, starting with src/lib.rs: it is the architecture document of this crate;
4. the [cairn/](./cairn) folder for the development history and living plans (the Cairn convention: spec/, changes/, log/).

Everything below documents only what differs from the Pimalaya standards.

## Deviation: no io- prefix

base64-codec is not an I/O library: its encoder and decoder are coroutines over the caller's buffers that never yield I/O. It has no client and no TLS layer, hence no io- prefix and no layered features.

## Deviation: unsafe SIMD kernels

The backends call CPU intrinsics and load and store through raw pointers, which takes unsafe code. Every unsafe block carries a one-line SAFETY comment, as the crate denies `clippy::undocumented_unsafe_blocks`.

The kernels are tested byte for byte against the scalar steps, and Miri runs on the library and on the kernels.

## Cairn

This repository follows [Cairn](https://github.com/pimalaya/cairn): a living spec, reviewable change proposals and a dated log, kept next to the code.

Non-trivial work starts with a change folder under cairn/changes, and nothing behavioural is done until its delta is folded into cairn/spec and an entry is appended to cairn/log. The activation stanza is [AGENTS.md](./AGENTS.md).

## Build

The crate is no_std and dependency-free under every feature: `alloc` adds the Vec helpers, `std` adds CPU detection. The tests run every backend the machine has against the scalar steps:

```sh
cargo build --no-default-features
cargo test --release --all-features
RUSTFLAGS="-C target-cpu=native" cargo test --release --all-features
```

The NEON backend builds for aarch64 and runs under qemu, with `QEMU_CPU=neoverse-n1`: qemu's default model crashes the release tests, as [cairn/log/2026-11-01-a-codec-value.md](./cairn/log/2026-11-01-a-codec-value.md) tells. Miri needs a nightly toolchain, and so do the fuzz targets, described in [fuzz/README.md](./fuzz/README.md).

## Benchmarks

The comparative benchmark runs with cargo bench --features std --bench compare, and its numbers and history are in [benches/README.md](./benches/README.md).

Criterion moves by several percent between runs, so speed work is measured in cycles per call, by the method of the latest change under cairn/changes.

## Attribution

The kernels and the driver follow encodify, simdutf8 and published methods, credited in [ATTRIBUTION.md](./ATTRIBUTION.md).
