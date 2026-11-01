---
cairn: spec
capability: backends
status: current
---

# Backends

Every call runs the backend the `Base64Cpu` token of its codec names: scalar everywhere, SSSE3, AVX2 and AVX-512 on x86, NEON on aarch64. The scalar steps alone handle anything but whole quanta of alphabet characters; the SIMD backends supply block kernels that one shared driver runs, and hand everything else back to scalar.

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

### Requirement: Every backend matches scalar

For every input and line ending, every backend SHALL write the same bytes and return the same clean flag as the scalar steps, in one-shot and resumed calls alike.

#### Scenario: Every length on an AVX-512 machine
- GIVEN pseudo-random inputs of every length from 0 to 1,024, encoded unwrapped, with CRLF and with LF, and the same encodings rewrapped or with a stray byte inserted
- WHEN they are encoded and decoded under AVX-512, AVX2, SSSE3 and scalar
- THEN the four backends write identical bytes and flags

#### Scenario: Every short end
- GIVEN every input of up to 6 bytes drawn from `Q`, `z`, `=`, CR, LF, SP and `*`, alone and after `QUJD` or `QUJD` CRLF
- WHEN it is decoded in one call, and resumed with the join between the two parts, under every backend the CPU supports
- THEN every backend writes the bytes and flag of the scalar steps
