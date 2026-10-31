//! base64-codec against the base64 wrapper of mime-rs as of 2026-10-31
//! (src/codec/base64.rs), which it replaces: the same bytes and the same
//! clean flag for every input, the same encodings for every input.

use base64_codec::{
    cpu::{Base64Backend, Base64Cpu},
    decode::{decode, decoded_len_max},
    encode::{Base64EncodeOptions, Base64Eol, encode, encoded_len},
};

/// The bytes inputs are drawn from, base64 characters most often.
const POOL: &[u8] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/AbCd0123+/====\r\n\r\n \t*\x00\x0c\xff-_.";

#[test]
fn decoding_matches_the_wrapper() {
    let mut rng = Rng(0x2545_f491_4f6c_dd1d);
    let mut inputs: Vec<Vec<u8>> = (0..20_000)
        .map(|_| {
            let len = rng.next() as usize % 300;
            (0..len)
                .map(|_| POOL[rng.next() as usize % POOL.len()])
                .collect()
        })
        .collect();

    for len in [57, 300, 4096] {
        let bytes: Vec<u8> = (0..len).map(|_| rng.next() as u8).collect();
        let mut encoded = Vec::new();
        wrapper::encode(&bytes, b"\r\n", &mut encoded);

        for _ in 0..50 {
            let mut mutated = encoded.clone();
            let at = rng.next() as usize % mutated.len();
            mutated[at] = POOL[rng.next() as usize % POOL.len()];
            inputs.push(mutated);
        }

        inputs.push(encoded);
    }

    for input in &inputs {
        let mut expected = Vec::new();
        let clean = wrapper::decode(input, &mut expected);

        for cpu in cpus() {
            let mut out = vec![0; decoded_len_max(input.len())];
            let output = decode(input, &mut out, cpu).unwrap();
            out.truncate(output.written);
            assert_eq!(
                (out, output.clean),
                (expected.clone(), clean),
                "{cpu:?} {input:?}"
            );
        }
    }
}

#[test]
fn encoding_matches_the_wrapper() {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);

    for len in 0..=600 {
        let bytes: Vec<u8> = (0..len).map(|_| rng.next() as u8).collect();

        for (wrap, eol) in [
            (None, &b""[..]),
            (Some(Base64Eol::Crlf), b"\r\n"),
            (Some(Base64Eol::Lf), b"\n"),
        ] {
            let mut expected = Vec::new();

            match wrap {
                None => wrapper::encode_b(&bytes, &mut expected),
                Some(_) => wrapper::encode(&bytes, eol, &mut expected),
            }

            let opts = Base64EncodeOptions { wrap };

            for cpu in cpus() {
                let mut out = vec![0; encoded_len(len, opts).unwrap()];
                let written = encode(&bytes, &mut out, cpu, opts).unwrap();
                out.truncate(written);
                assert_eq!(out, expected, "{cpu:?} {opts:?} {len}");
            }
        }
    }
}

/// Every token the running CPU supports.
fn cpus() -> Vec<Base64Cpu> {
    // SAFETY: the scalar backend needs no CPU feature.
    let scalar = unsafe { Base64Cpu::assume(Base64Backend::Scalar) };
    let mut cpus = vec![scalar, Base64Cpu::BASELINE];
    cpus.extend(simd());
    cpus
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn simd() -> Vec<Base64Cpu> {
    let mut cpus = Vec::new();

    if std::arch::is_x86_feature_detected!("ssse3") {
        // SAFETY: SSSE3 was detected just above.
        cpus.push(unsafe { Base64Cpu::assume(Base64Backend::Ssse3) });
    }

    if std::arch::is_x86_feature_detected!("avx2") {
        // SAFETY: AVX2 was detected just above.
        cpus.push(unsafe { Base64Cpu::assume(Base64Backend::Avx2) });
    }

    if std::arch::is_x86_feature_detected!("avx512vbmi") {
        // SAFETY: AVX-512 VBMI was detected just above.
        cpus.push(unsafe { Base64Cpu::assume(Base64Backend::Avx512) });
    }

    cpus
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
fn simd() -> Vec<Base64Cpu> {
    Vec::new()
}

/// A xorshift generator, so that every run tests the same inputs.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

/// A frozen copy of mime-rs's src/codec/base64.rs as of 2026-10-31, over the
/// base64 crate, kept as the oracle of the switch.
mod wrapper {
    use base64::{
        Engine,
        alphabet::STANDARD,
        engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig},
    };

    const ENGINE: GeneralPurpose = GeneralPurpose::new(
        &STANDARD,
        GeneralPurposeConfig::new()
            .with_decode_padding_mode(DecodePaddingMode::Indifferent)
            .with_decode_allow_trailing_bits(true),
    );

    pub fn decode(input: &[u8], out: &mut Vec<u8>) -> bool {
        let start = out.len();

        if decode_lines(input, out) {
            return true;
        }

        out.truncate(start);
        decode_filtered(input, out)
    }

    pub fn encode(input: &[u8], eol: &[u8], out: &mut Vec<u8>) {
        out.reserve(input.len().div_ceil(57) * (76 + eol.len()));

        for chunk in input.chunks(57) {
            let start = out.len();
            out.resize(start + chunk.len().div_ceil(3) * 4, 0);

            let written = ENGINE
                .encode_slice(chunk, &mut out[start..])
                .unwrap_or_default();

            out.truncate(start + written);
            out.extend_from_slice(eol);
        }
    }

    pub fn encode_b(input: &[u8], out: &mut Vec<u8>) {
        let start = out.len();
        out.resize(start + input.len().div_ceil(3) * 4, 0);

        let written = ENGINE
            .encode_slice(input, &mut out[start..])
            .unwrap_or_default();

        out.truncate(start + written);
    }

    fn decode_lines(input: &[u8], out: &mut Vec<u8>) -> bool {
        let mut padded = false;

        for line in input.split(|byte| *byte == b'\n') {
            let line = trim_blanks(line);

            if line.is_empty() {
                continue;
            }

            if padded || !line.len().is_multiple_of(4) {
                return false;
            }

            padded = line.ends_with(b"=");

            let start = out.len();
            out.resize(start + line.len() / 4 * 3, 0);

            match ENGINE.decode_slice(line, &mut out[start..]) {
                Ok(written) => out.truncate(start + written),
                Err(_) => return false,
            }
        }

        true
    }

    fn decode_filtered(input: &[u8], out: &mut Vec<u8>) -> bool {
        let mut clean = true;
        let mut symbols = Vec::with_capacity(input.len());
        let mut padding = false;

        for &byte in input {
            match byte {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'+' | b'/' => {
                    if padding {
                        clean = false;
                        padding = false;
                        flush(&mut symbols, out);
                    }

                    symbols.push(byte);
                }
                b'=' => padding = true,
                b' ' | b'\t' | b'\r' | b'\n' => {}
                _ => clean = false,
            }
        }

        if symbols.len() % 4 == 1 {
            clean = false;
            symbols.pop();
        }

        flush(&mut symbols, out);
        clean
    }

    fn flush(symbols: &mut Vec<u8>, out: &mut Vec<u8>) {
        if symbols.len() % 4 == 1 {
            symbols.pop();
        }

        let start = out.len();
        out.resize(start + symbols.len().div_ceil(4) * 3, 0);

        let written = ENGINE
            .decode_slice(symbols.as_slice(), &mut out[start..])
            .unwrap_or_default();

        out.truncate(start + written);
        symbols.clear();
    }

    fn trim_blanks(line: &[u8]) -> &[u8] {
        let blank = |byte: &u8| matches!(byte, b' ' | b'\t' | b'\r');
        let start = line
            .iter()
            .position(|byte| !blank(byte))
            .unwrap_or(line.len());
        let end = line
            .iter()
            .rposition(|byte| !blank(byte))
            .map_or(start, |end| end + 1);

        &line[start..end]
    }
}
