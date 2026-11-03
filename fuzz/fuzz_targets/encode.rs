//! # Encode fuzz target
//!
//! Every backend encodes any input in either alphabet, padded or not,
//! one-shot or resumed over any chunks and room, to the characters of
//! scalar, which decode back clean.

#![no_main]

use base64_codec::{
    codec::{Base64Alphabet, Base64Codec, Base64Eol},
    cpu::Base64Cpu,
    encode::Base64Encode,
};
use libfuzzer_sys::fuzz_target;
use simd_cpu::{SimdCpu, SimdFeature};

fuzz_target!(|data: &[u8]| {
    let [options, chunk, room, input @ ..] = data else {
        return;
    };

    let wrap = match options % 3 {
        0 => None,
        1 => Some(Base64Eol::Crlf),
        _ => Some(Base64Eol::Lf),
    };

    let alphabet = match options / 3 % 2 {
        0 => Base64Alphabet::Standard,
        _ => Base64Alphabet::Url,
    };

    let pad = options / 6 % 2 == 0;

    let chunk = *chunk as usize + 1;
    let room = *room as usize % 64 + 6;

    let scalar = Base64Codec {
        cpu: Base64Cpu::new(SimdCpu::NONE),
        alphabet,
        pad,
        wrap,
    };

    let expected = one_shot(input, scalar);

    let mut decoded = vec![0; scalar.decoded_len_max(expected.len())];
    let output = scalar.decode(&expected, &mut decoded).unwrap();
    assert_eq!(&decoded[..output.written], input);
    assert!(output.clean);

    for cpu in cpus() {
        let codec = Base64Codec { cpu, ..scalar };
        assert_eq!(one_shot(input, codec), expected, "{codec:?}");

        let mut encoder = Base64Encode::new(codec);
        let mut out = Vec::new();
        let mut buffer = vec![0; room];

        for mut src in input.chunks(chunk) {
            while !src.is_empty() {
                let output = encoder.resume(src, &mut buffer);
                out.extend_from_slice(&buffer[..output.written]);
                src = &src[output.read..];
            }
        }

        let end = encoder.finish(&mut buffer).unwrap();
        out.extend_from_slice(&buffer[..end]);
        assert_eq!(out, expected, "{codec:?} resumed");
    }
});

fn one_shot(input: &[u8], codec: Base64Codec) -> Vec<u8> {
    let mut out = vec![0; codec.encoded_len(input.len()).unwrap()];
    let written = codec.encode(input, &mut out).unwrap();
    assert_eq!(written, out.len());
    out
}

fn cpus() -> Vec<Base64Cpu> {
    let mut cpu = SimdCpu::detect();
    let mut cpus = Vec::new();

    for feature in [
        SimdFeature::Avx512Vbmi,
        SimdFeature::Avx2,
        SimdFeature::Ssse3,
    ] {
        let next = Base64Cpu::new(cpu);

        if !cpus.contains(&next) {
            cpus.push(next);
        }

        cpu = cpu.without(feature);
    }

    cpus
}
