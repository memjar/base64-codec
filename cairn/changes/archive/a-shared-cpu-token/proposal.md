---
cairn: change
id: a-shared-cpu-token
status: landed
created: 2026-11-01
---

# A shared CPU token

## Why

base64-codec and quoted-printable-codec each carried their own CPU detection behind a `std` feature, and base32, UTF-7 and other codecs would carry a third and a fourth. An application using several codecs detected the CPU once per codec and held one token per codec, and every codec needed a `std` feature for detection alone.

## What

The new crate simd-cpu (~/code/pimalaya/simd-cpu) holds `SimdCpu`, the SIMD features of the running CPU as a token that cannot be forged: `NONE`, `BASELINE` from `cfg(target_feature)`, `detect` behind its `std` feature, the unsafe `with` and the safe `without`. It names features, not backends, since every codec needs a different set.

base64-codec keeps `Base64Cpu` as its resolved backend, built once by the `const fn` `Base64Cpu::new(SimdCpu)`, which picks the widest backend the features allow, so the hot path still matches on the same enum. `Base64Cpu::BASELINE`, `Base64Cpu::detect`, `Base64Cpu::assume` and the `std` feature go. The tests, the bench and the fuzz targets build narrower backends with `SimdCpu::without`, which is safe, instead of `assume`.

Settled with the user on 2026-11-01: the crate is named simd-cpu, and the codecs give up "dependency-free" for this one dependency.
