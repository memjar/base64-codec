//! Every backend encodes any input, in one call or resumed over chunks of
//! any size into a buffer of any size, to the characters of the scalar
//! backend, which decode back to the input, clean.

#![no_main]

use base64_codec::{
    codec::{Base64Codec, Base64Eol},
    cpu::{Base64Backend, Base64Cpu},
    encode::Base64Encode,
};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let [options, chunk, room, input @ ..] = data else {
        return;
    };

    let wrap = match options % 3 {
        0 => None,
        1 => Some(Base64Eol::Crlf),
        _ => Some(Base64Eol::Lf),
    };

    let chunk = *chunk as usize + 1;
    let room = *room as usize % 64 + 6;

    // SAFETY: the scalar backend needs no CPU feature.
    let cpu = unsafe { Base64Cpu::assume(Base64Backend::Scalar) };
    let scalar = Base64Codec { cpu, wrap };
    let expected = one_shot(input, scalar);

    let mut decoded = vec![0; scalar.decoded_len_max(expected.len())];
    let output = scalar.decode(&expected, &mut decoded).unwrap();
    assert_eq!(&decoded[..output.written], input);
    assert!(output.clean);

    for cpu in cpus() {
        let codec = Base64Codec { cpu, wrap };
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

/// Every token the running CPU supports.
fn cpus() -> Vec<Base64Cpu> {
    let detected = Base64Cpu::detect();
    let mut cpus = vec![detected];
    cpus.extend(below(detected.backend()));
    cpus
}

/// The tokens below `detected` that the running CPU supports too.
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn below(detected: Base64Backend) -> Vec<Base64Cpu> {
    let backends: &[Base64Backend] = match detected {
        Base64Backend::Avx512 => &[Base64Backend::Avx2, Base64Backend::Ssse3],
        Base64Backend::Avx2 => &[Base64Backend::Ssse3],
        _ => &[],
    };

    // SAFETY: a CPU with AVX-512 has AVX2 and SSSE3, one with AVX2 has SSSE3.
    backends
        .iter()
        .map(|backend| unsafe { Base64Cpu::assume(*backend) })
        .collect()
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
fn below(_: Base64Backend) -> Vec<Base64Cpu> {
    Vec::new()
}
