---
cairn: delta
change: a-shared-cpu-token
---

## ADDED Requirements

### Requirement: One dependency, no std

The library SHALL build `#![no_std]` under every feature combination with simd-cpu, without its `std` feature, as its only dependency, `alloc` serving only `Base64Codec::encode_vec` and `Base64Codec::decode_vec`.

## MODIFIED Requirements

### Requirement: CPU detection is the caller's

No encoding or decoding function SHALL detect CPU features. A backend SHALL run only from a `Base64Cpu` that `Base64Cpu::new` built from a `simd_cpu::SimdCpu`, the application's token of the features the running CPU has.

#### Scenario: An aarch64 build without std
- GIVEN the crate built for aarch64 with no feature
- WHEN `Base64Codec::BASELINE` decodes an input
- THEN it runs the NEON backend, and no detection code is compiled

#### Scenario: A default x86_64 build without std
- GIVEN the crate built for x86_64 with no feature and no `-C target-feature`
- WHEN `Base64Codec::BASELINE` decodes an input
- THEN it runs the scalar backend

### Requirement: The widest backend the CPU has

`Base64Cpu::new` SHALL name the widest backend whose features its `SimdCpu` holds: on x86 AVX-512 with the VBMI byte permutes, then AVX2, then SSSE3; NEON on aarch64; scalar otherwise.

#### Scenario: A CPU with AVX-512 VBMI
- GIVEN a `SimdCpu` detected on an x86_64 CPU with AVX-512 VBMI
- WHEN the application builds a `Base64Cpu` from it
- THEN the token names the AVX-512 backend, and from the same `SimdCpu` without AVX-512 VBMI, the AVX2 backend

## REMOVED Requirements

### Requirement: No dependency, no std
