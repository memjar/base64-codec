//! Every backend encodes any input, in one call or resumed over chunks of
//! any size into a buffer of any size, to the characters of the scalar
//! backend, which decode back to the input, clean.

#![no_main]

use base64_codec::{
    cpu::{Base64Backend, Base64Cpu},
    decode::{decode, decoded_len_max},
    encode::{Base64Encode, Base64EncodeOptions, Base64Eol, encode, encoded_len},
};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let [options, chunk, room, input @ ..] = data else {
        return;
    };

    let opts = Base64EncodeOptions {
        wrap: match options % 3 {
            0 => None,
            1 => Some(Base64Eol::Crlf),
            _ => Some(Base64Eol::Lf),
        },
    };

    let chunk = *chunk as usize + 1;
    let room = *room as usize % 64 + 6;

    // SAFETY: the scalar backend needs no CPU feature.
    let scalar = unsafe { Base64Cpu::assume(Base64Backend::Scalar) };
    let expected = one_shot(input, scalar, opts);

    let mut decoded = vec![0; decoded_len_max(expected.len())];
    let output = decode(&expected, &mut decoded, scalar).unwrap();
    assert_eq!(&decoded[..output.written], input);
    assert!(output.clean);

    for cpu in cpus() {
        assert_eq!(one_shot(input, cpu, opts), expected, "{cpu:?}");

        let mut encoder = Base64Encode::new(cpu, opts);
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
        assert_eq!(out, expected, "{cpu:?} resumed");
    }
});

fn one_shot(input: &[u8], cpu: Base64Cpu, opts: Base64EncodeOptions) -> Vec<u8> {
    let mut out = vec![0; encoded_len(input.len(), opts).unwrap()];
    let written = encode(input, &mut out, cpu, opts).unwrap();
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
