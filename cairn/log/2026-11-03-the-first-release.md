---
cairn: log
change: the-first-release
landed: 2026-11-03
---

# The first release

Prepared base64-codec 0.1.0, the first crates.io release. simd-cpu moved from its git rev pin to `simd-cpu = { version = "0.1", default-features = false }` from the registry, in the manifest and fuzz/Cargo.toml, as crates.io refuses a git dependency: simd-cpu 0.1.0 was published first, and the lockfiles resolve it from the registry. mime-rs and vcard-rs, which pin this crate and simd-cpu by git rev, move to the registry versions together.

The manifest, package.nix and default.nix read 0.1.0, CHANGELOG.md has its 0.1.0 section and SECURITY.md supports 0.1.x. Dev-dependencies took `default-features = false` with the features the bench needs, and the benchmark tables moved from the README to benches/README.md, per the Pimalaya guidelines (readme-007). Doc comments and module headers were compressed to the guideline ceilings, with no code behaviour changed.

Capabilities moved: none.
