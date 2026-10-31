# Fuzzing

Two [cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz) targets, run from the dedicated nightly shell:

```sh
nix-shell fuzz/shell.nix --run "cargo fuzz run decode"
nix-shell fuzz/shell.nix --run "cargo fuzz run encode"
```

- `decode`: every backend the machine has decodes any input, in one call and resumed over chunks of any size into a buffer of any size, to the bytes and clean flag of the scalar backend, without panicking.
- `encode`: the same for encoding, and the scalar encoding decodes back to the input, clean.

The scalar backend itself is checked against the frozen copy of mime-rs's base64 wrapper in tests/mime.rs.
