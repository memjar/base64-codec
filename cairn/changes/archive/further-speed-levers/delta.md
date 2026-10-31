---
cairn: delta
change: further-speed-levers
---

## ADDED Requirements

### Requirement: The widest backend the CPU has

`Base64Cpu::detect` SHALL name the widest backend the running CPU supports, on x86 AVX-512 with the VBMI byte permutes, then AVX2, then SSSE3; `Base64Cpu::BASELINE` SHALL name the widest one the build target's features guarantee.

#### Scenario: A CPU with AVX-512 VBMI
- GIVEN an x86_64 CPU with AVX-512 VBMI
- WHEN the application calls `Base64Cpu::detect`
- THEN the token names the AVX-512 backend

## MODIFIED Requirements

### Requirement: Every backend matches scalar

For every input and options, every backend SHALL write the same bytes and return the same clean flag as the scalar steps, in one-shot and resumed calls alike.

#### Scenario: Every length on an AVX-512 machine
- GIVEN pseudo-random inputs of every length from 0 to 1,024, encoded unwrapped, with CRLF and with LF, and the same encodings rewrapped or with a stray byte inserted
- WHEN they are encoded and decoded under AVX-512, AVX2, SSSE3 and scalar
- THEN the four backends write identical bytes and flags

#### Scenario: Every short end
- GIVEN every input of up to 6 bytes drawn from `Q`, `z`, `=`, CR, LF, SP and `*`, alone and after `QUJD` or `QUJD` CRLF
- WHEN it is decoded in one call, and resumed with the join between the two parts, under every backend the CPU supports
- THEN every backend writes the bytes and flag of the scalar steps
