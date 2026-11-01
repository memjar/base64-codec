---
cairn: tasks
change: a-codec-value
---

- [x] `codec::Base64Codec` with `cpu` and `wrap`, `BASELINE`, and `Base64Eol` moved next to it
- [x] The one-shot methods on the codec, the free functions and `Base64EncodeOptions` removed
- [x] `Base64Encode::new` and `Base64Decode::new` taking a codec
- [x] Docs, tests, the bench, the fuzz targets and the README on the new calls
- [x] Instruction counts of the one-shot calls at 64 bytes unchanged: 5 to 18 fewer, with `encode` and `decode` inlined
- [x] Spec folded, log entry written
