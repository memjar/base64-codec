# 🔣 base64-codec [![Coverage](https://img.shields.io/codecov/c/github/pimalaya/base64-codec/master?style=flat&logo=codecov&logoColor=white)](https://codecov.io/gh/pimalaya/base64-codec) [![Matrix](https://img.shields.io/badge/chat-%23pimalaya-blue?style=flat&logo=matrix&logoColor=white)](https://matrix.to/#/#pimalaya:matrix.org) [![Mastodon](https://img.shields.io/badge/news-%40pimalaya-blue?style=flat&logo=mastodon&logoColor=white)](https://fosstodon.org/@pimalaya) [![Sponsor](https://img.shields.io/badge/sponsor-pink?style=flat&logo=github-sponsors&logoColor=white)](https://pimalaya.org/sponsor/)

Base64 encoder and decoder with SIMD, no_std and dependency-free

## Table of contents

- [Features](#features)
- [RFC coverage](#rfc-coverage)
- [Usage](#usage)
- [Examples](#examples)
- [Benchmarks](#benchmarks)
- [AI policy](https://github.com/pimalaya/.github/blob/master/AI_POLICY.md)
- [License](#license)
- [Social](#social)
- [Contributing](./CONTRIBUTING.md)
- [Sponsoring](#sponsoring)

## Features

- **SIMD**: AVX-512, AVX2 and SSSE3 on x86, NEON on aarch64, scalar anywhere else.
- **Fast**: up to 4x faster than encodify and 34x faster than the base64 crate, see the [benchmarks](#benchmarks).
- **MIME lines**: encodes in lines of 76 characters ended by CRLF or LF, or in one unbroken run.
- **Lenient decoding**: never fails, skips line breaks and blanks, reports other stray bytes.
- **I/O-free**: encode and decode chunk by chunk into your own buffers.
- **Portable**: no_std, dependency-free, the CPU picked once by the application.

> [!TIP]
> base64-codec uses [cargo features](https://doc.rust-lang.org/cargo/reference/features.html) to gate optional support. The default feature set is declared in [Cargo.toml](./Cargo.toml) or on [docs.rs](https://docs.rs/crate/base64-codec/latest/features).

## RFC coverage

| RFC    | What is covered                                                                                                      |
|--------|----------------------------------------------------------------------------------------------------------------------|
| [4648] | The base64 encoding: the standard alphabet and its padding                                                           |
| [2045] | Base64 as a MIME transfer encoding: lines of 76 characters, line breaks and foreign characters skipped when decoding |
| [2047] | The B encoding of encoded words: one unbroken run of characters                                                      |

[4648]: https://www.rfc-editor.org/rfc/rfc4648
[2045]: https://www.rfc-editor.org/rfc/rfc2045
[2047]: https://www.rfc-editor.org/rfc/rfc2047

## Usage

The whole API is documented on [docs.rs](https://docs.rs/base64-codec/latest/base64_codec), with runnable snippets for the encoder, the decoder and the one-shot functions.

## Examples

The crate ships no examples folder, a whole program adding nothing to a two-call API. The crate root, the encoder and the decoder each open with a runnable snippet, and [tests/mime.rs](./tests/mime.rs) checks every backend against the base64 wrapper of mime-rs it is meant to replace.

## Benchmarks

[criterion](https://crates.io/crates/criterion) medians of `cargo bench --features std --bench compare` on 2026-11-01, on an AMD Ryzen AI Max+ 395 (Zen 5), in GiB/s of raw bytes. Each italic delta compares its row with base64-codec.

### Unwrapped decoding

| GiB/s | 64 B | 4 KiB | 1 MiB | 10 MiB |
| --- | ---: | ---: | ---: | ---: |
| **base64-codec** | **6.51** | **74.59** | **48.31** | **43.49** |
| base64-codec, AVX2 | *(-9%)* **5.91** | *(-61%)* **29.42** | *(-39%)* **29.54** | *(-36%)* **27.66** |
| base64-codec, scalar | *(-58%)* **2.71** | *(-94%)* **4.63** | *(-90%)* **4.67** | *(-89%)* **4.66** |
| [encodify](https://crates.io/crates/encodify) | *(-9%)* **5.90** | *(-65%)* **25.99** | *(-46%)* **26.08** | *(-43%)* **24.87** |
| [encodify](https://crates.io/crates/encodify), lenient | *(-34%)* **4.31** | *(-68%)* **24.10** | *(-46%)* **26.02** | *(-43%)* **24.88** |
| [base64-simd](https://crates.io/crates/base64-simd) | *(-1%)* **6.44** | *(-85%)* **11.39** | *(-76%)* **11.44** | *(-74%)* **11.37** |
| [base64](https://crates.io/crates/base64) | *(-64%)* **2.36** | *(-95%)* **3.46** | *(-93%)* **3.46** | *(-92%)* **3.45** |

### Wrapped decoding

| GiB/s | 64 B | 4 KiB | 1 MiB | 10 MiB |
| --- | ---: | ---: | ---: | ---: |
| **base64-codec** | **4.82** | **44.70** | **38.36** | **32.56** |
| base64-codec, AVX2 | *(+3%)* **4.99** | *(-41%)* **26.43** | *(-27%)* **28.19** | *(-22%)* **25.37** |
| base64-codec, scalar | *(-39%)* **2.94** | *(-89%)* **4.74** | *(-88%)* **4.74** | *(-85%)* **4.73** |
| [encodify](https://crates.io/crates/encodify) | *(-49%)* **2.47** | *(-63%)* **16.55** | *(-50%)* **19.24** | *(-43%)* **18.63** |
| [base64-simd](https://crates.io/crates/base64-simd) | *(-45%)* **2.64** | *(-97%)* **1.41** | *(-96%)* **1.41** | *(-96%)* **1.40** |
| [base64](https://crates.io/crates/base64) | *(-79%)* **1.01** | *(-97%)* **1.30** | *(-97%)* **1.31** | *(-96%)* **1.31** |

### Unwrapped encoding

| GiB/s | 64 B | 4 KiB | 1 MiB | 10 MiB |
| --- | ---: | ---: | ---: | ---: |
| **base64-codec** | **7.77** | **128.55** | **49.71** | **44.59** |
| base64-codec, AVX2 | *(-14%)* **6.68** | *(-75%)* **32.75** | *(-34%)* **32.97** | *(-30%)* **31.17** |
| base64-codec, scalar | *(-58%)* **3.23** | *(-95%)* **5.87** | *(-88%)* **5.80** | *(-87%)* **5.77** |
| [encodify](https://crates.io/crates/encodify) | *(-21%)* **6.12** | *(-76%)* **31.45** | *(-37%)* **31.56** | *(-33%)* **29.73** |
| [base64-simd](https://crates.io/crates/base64-simd) | *(+1%)* **7.86** | *(-88%)* **15.49** | *(-69%)* **15.48** | *(-66%)* **15.26** |
| [base64](https://crates.io/crates/base64) | *(-61%)* **3.06** | *(-97%)* **4.08** | *(-92%)* **4.12** | *(-91%)* **4.11** |

### Wrapped encoding

| GiB/s | 64 B | 4 KiB | 1 MiB | 10 MiB |
| --- | ---: | ---: | ---: | ---: |
| **base64-codec** | **6.04** | **56.73** | **48.32** | **39.30** |
| base64-codec, AVX2 | *(-27%)* **4.39** | *(-54%)* **26.19** | *(-43%)* **27.69** | *(-33%)* **26.25** |
| base64-codec, scalar | *(-50%)* **3.01** | *(-90%)* **5.85** | *(-88%)* **5.86** | *(-85%)* **5.90** |
| [encodify](https://crates.io/crates/encodify) | *(-56%)* **2.65** | *(-66%)* **19.10** | *(-57%)* **20.85** | *(-49%)* **20.22** |
| [base64](https://crates.io/crates/base64) | *(-56%)* **2.64** | *(-94%)* **3.22** | *(-93%)* **3.19** | *(-92%)* **3.19** |

### Reading the numbers

- base64-codec runs AVX-512 here. Its AVX2 row uses the instructions encodify uses.
- encodify decodes unwrapped input strictly, which is less work. Its lenient row follows base64-codec's rules.
- base64 decodes wrapped input line by line, as mime-rs does. base64-simd has no wrapped encoding.
- The 4 KiB columns move by up to 30% between runs, the memory-bound 1 MiB and 10 MiB ones by up to 40%.

History and cycle counts are in [benches/README.md](./benches/README.md).

## License

This project is licensed under either of:

- [MIT license](LICENSE-MIT)
- [Apache License, Version 2.0](LICENSE-APACHE)

## Social

- Chat on [Matrix](https://matrix.to/#/#pimalaya:matrix.org)
- News on [Mastodon](https://fosstodon.org/@pimalaya) or [RSS](https://fosstodon.org/@pimalaya.rss)
- Mail at [pimalaya.org@posteo.net](mailto:pimalaya.org@posteo.net)

## Sponsoring

[![nlnet](https://nlnet.nl/logo/banner-160x60.png)](https://nlnet.nl/)

Special thanks to the [NLnet foundation](https://nlnet.nl/) and the [European Commission](https://www.ngi.eu/) that have been financially supporting the project for years:

- 2022 → 2023: [NGI Assure](https://nlnet.nl/project/Himalaya/)
- 2023 → 2024: [NGI Zero Entrust](https://nlnet.nl/project/Pimalaya/)
- 2024 → 2026: [NGI Zero Core](https://nlnet.nl/project/Pimalaya-PIM/)
- 2026 → 2027: [NGI Zero Commons Fund](https://nlnet.nl/project/Pimalaya-pimdir/)

This program is part of Pimalaya, free software funded entirely by grants and donations. If you find it useful, consider [sponsoring](https://pimalaya.org/sponsor/) its development:

[![GitHub](https://img.shields.io/badge/-GitHub%20Sponsors-fafbfc?logo=GitHub%20Sponsors)](https://github.com/sponsors/soywod)
[![Ko-fi](https://img.shields.io/badge/-Ko--fi-ff5e5a?logo=Ko-fi&logoColor=ffffff)](https://ko-fi.com/pimalaya)
[![Buy Me a Coffee](https://img.shields.io/badge/-Buy%20Me%20a%20Coffee-ffdd00?logo=Buy%20Me%20A%20Coffee&logoColor=000000)](https://www.buymeacoffee.com/pimalaya)
[![Liberapay](https://img.shields.io/badge/-Liberapay-f6c915?logo=Liberapay&logoColor=222222)](https://liberapay.com/pimalaya)
[![thanks.dev](https://img.shields.io/badge/-thanks.dev-000000?logo=data:image/svg+xml;base64,PHN2ZyB3aWR0aD0iMjQuMDk3IiBoZWlnaHQ9IjE3LjU5NyIgY2xhc3M9InctMzYgbWwtMiBsZzpteC0wIHByaW50Om14LTAgcHJpbnQ6aW52ZXJ0IiB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciPjxwYXRoIGQ9Ik05Ljc4MyAxNy41OTdINy4zOThjLTEuMTY4IDAtMi4wOTItLjI5Ny0yLjc3My0uODktLjY4LS41OTMtMS4wMi0xLjQ2Mi0xLjAyLTIuNjA2di0xLjM0NmMwLTEuMDE4LS4yMjctMS43NS0uNjc4LTIuMTk1LS40NTItLjQ0Ni0xLjIzMi0uNjY5LTIuMzQtLjY2OUgwVjcuNzA1aC41ODdjMS4xMDggMCAxLjg4OC0uMjIyIDIuMzQtLjY2OC40NTEtLjQ0Ni42NzctMS4xNzcuNjc3LTIuMTk1VjMuNDk2YzAtMS4xNDQuMzQtMi4wMTMgMS4wMjEtMi42MDZDNS4zMDUuMjk3IDYuMjMgMCA3LjM5OCAwaDIuMzg1djEuOTg3aC0uOTg1Yy0uMzYxIDAtLjY4OC4wMjctLjk4LjA4MmExLjcxOSAxLjcxOSAwIDAgMC0uNzM2LjMwN2MtLjIwNS4xNTYtLjM1OC4zODQtLjQ2LjY4Mi0uMTAzLjI5OC0uMTU0LjY4Mi0uMTU0IDEuMTUxVjUuMjNjMCAuODY3LS4yNDkgMS41ODYtLjc0NSAyLjE1NS0uNDk3LjU2OS0xLjE1OCAxLjAwNC0xLjk4MyAxLjMwNXYuMjE3Yy44MjUuMyAxLjQ4Ni43MzYgMS45ODMgMS4zMDUuNDk2LjU3Ljc0NSAxLjI4Ny43NDUgMi4xNTR2MS4wMjFjMCAuNDcuMDUxLjg1NC4xNTMgMS4xNTIuMTAzLjI5OC4yNTYuNTI1LjQ2MS42ODIuMTkzLjE1Ny40MzcuMjYuNzMyLjMxMi4yOTUuMDUuNjIzLjA3Ni45ODQuMDc2aC45ODVabTE0LjMxNC03LjcwNmgtLjU4OGMtMS4xMDggMC0xLjg4OC4yMjMtMi4zNC42NjktLjQ1LjQ0NS0uNjc3IDEuMTc3LS42NzcgMi4xOTVWMTQuMWMwIDEuMTQ0LS4zNCAyLjAxMy0xLjAyIDIuNjA2LS42OC41OTMtMS42MDUuODktMi43NzQuODloLTIuMzg0di0xLjk4OGguOTg0Yy4zNjIgMCAuNjg4LS4wMjcuOTgtLjA4LjI5Mi0uMDU1LjUzOC0uMTU3LjczNy0uMzA4LjIwNC0uMTU3LjM1OC0uMzg0LjQ2LS42ODIuMTAzLS4yOTguMTU0LS42ODIuMTU0LTEuMTUydi0xLjAyYzAtLjg2OC4yNDgtMS41ODYuNzQ1LTIuMTU1LjQ5Ny0uNTcgMS4xNTgtMS4wMDQgMS45ODMtMS4zMDV2LS4yMTdjLS44MjUtLjMwMS0xLjQ4Ni0uNzM2LTEuOTgzLTEuMzA1LS40OTctLjU3LS43NDUtMS4yODgtLjc0NS0yLjE1NXYtMS4wMmMwLS40Ny0uMDUxLS44NTQtLjE1NC0xLjE1Mi0uMTAyLS4yOTgtLjI1Ni0uNTI2LS40Ni0uNjgyYTEuNzE5IDEuNzE5IDAgMCAwLS43MzctLjMwNyA1LjM5NSA1LjM5NSAwIDAgMC0uOTgtLjA4MmgtLjk4NFYwaDIuMzg0YzEuMTY5IDAgMi4wOTMuMjk3IDIuNzc0Ljg5LjY4LjU5MyAxLjAyIDEuNDYyIDEuMDIgMi42MDZ2MS4zNDZjMCAxLjAxOC4yMjYgMS43NS42NzggMi4xOTUuNDUxLjQ0NiAxLjIzMS42NjggMi4zNC42NjhoLjU4N3oiIGZpbGw9IiNmZmYiLz48L3N2Zz4=)](https://thanks.dev/u/gh/soywod)
[![PayPal](https://img.shields.io/badge/-PayPal-0079c1?logo=PayPal&logoColor=ffffff)](https://www.paypal.com/paypalme/soywod)
