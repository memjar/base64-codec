---
cairn: delta
change: the-installed-token
---

## ADDED Requirements

## MODIFIED Requirements

### Requirement: CPU detection is the caller's

No encoding or decoding function SHALL detect CPU features. A backend SHALL run only from a `Base64Cpu` that `Base64Cpu::new` built from a `simd_cpu::SimdCpu`: one the caller gives, or, for `Base64Cpu::default()` and `Base64Codec::default()`, `SimdCpu::current()`, the token the application installed or `SimdCpu::BASELINE`, read once when the value is built.

#### Scenario: An aarch64 build without std
- GIVEN the crate built for aarch64 with no feature
- WHEN `Base64Codec::BASELINE` decodes an input
- THEN it runs the NEON backend, and no detection code is compiled

#### Scenario: A default x86_64 build without std
- GIVEN the crate built for x86_64 with no feature and no `-C target-feature`
- WHEN `Base64Codec::BASELINE` decodes an input
- THEN it runs the scalar backend

#### Scenario: A default codec and the installed token
- GIVEN a process on a CPU with AVX2, and a `Base64Codec::default()` built before any install
- WHEN the application calls `SimdCpu::install_detected()`, then builds another `Base64Codec::default()`
- THEN the first codec runs the backend of `SimdCpu::BASELINE`, the second the AVX2 backend or a wider one

## REMOVED Requirements
