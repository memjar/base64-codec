---
cairn: change
id: a-codec-value
status: landed
created: 2026-11-01
---

# A codec value

## Why

The one-shot functions stood apart from the encoder and decoder: free functions a reader of `Base64Encode` never saw on docs.rs, called as `encode::encode`, taking the CPU token and options `Base64Encode::new` takes too. Moving them onto the coroutines instead would let a caller run one in the middle of a resumed input, dropping what the coroutine carried.

## What

`codec::Base64Codec`, a plain `Copy` value holding the backend (`cpu`) and the line ending of encoded output (`wrap`), with the one-shot methods `encode`, `encode_vec`, `encoded_len`, `decode`, `decode_vec` and `decoded_len_max`. An application builds one, usually at start from `Base64Cpu::detect`, and uses it everywhere; `Base64Codec::BASELINE` needs no detection. `Base64Encode::new` and `Base64Decode::new` take a codec. `Base64EncodeOptions` and the free functions go, `Base64Eol` moves to the codec module. Resuming and finishing do not change.

Settled with the user on 2026-11-01: the coroutines keep their verb names (naming-006), the configuration is a codec value rather than a builder, and public changes are free while the crate is unpublished.
