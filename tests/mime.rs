//! Every backend against the base64 wrapper of mime-rs it replaces.

use base64_codec::{
    codec::{Base64Codec, Base64Eol},
    cpu::Base64Cpu,
};
use simd_cpu::{SimdCpu, SimdFeature};

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
            let codec = Base64Codec {
                cpu,
                ..Base64Codec::BASELINE
            };
            let mut out = vec![0; codec.decoded_len_max(input.len())];
            let output = codec.decode(input, &mut out).unwrap();
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

            for cpu in cpus() {
                let codec = Base64Codec {
                    cpu,
                    wrap,
                    ..Base64Codec::BASELINE
                };
                let mut out = vec![0; codec.encoded_len(len).unwrap()];
                let written = codec.encode(&bytes, &mut out).unwrap();
                out.truncate(written);
                assert_eq!(out, expected, "{codec:?} {len}");
            }
        }
    }
}

fn cpus() -> Vec<Base64Cpu> {
    let mut cpu = SimdCpu::detect();
    let mut cpus = vec![
        Base64Cpu::new(SimdCpu::NONE),
        Base64Cpu::new(SimdCpu::BASELINE),
    ];

    for feature in [
        SimdFeature::Avx512Vbmi,
        SimdFeature::Avx2,
        SimdFeature::Ssse3,
        SimdFeature::Neon,
    ] {
        let next = Base64Cpu::new(cpu);

        if !cpus.contains(&next) {
            cpus.push(next);
        }

        cpu = cpu.without(feature);
    }

    cpus
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

/// mime-rs's src/codec/base64.rs as of 2026-10-31, frozen as the oracle.
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
