//! Every backend decodes any input, one-shot or resumed over any chunks and
//! room, to the bytes and flag of scalar, without panicking.

#![no_main]

use base64_codec::{
    codec::Base64Codec,
    cpu::{Base64Backend, Base64Cpu},
    decode::Base64Decode,
};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let [chunk, room, input @ ..] = data else {
        return;
    };

    let chunk = *chunk as usize + 1;
    let room = *room as usize % 64 + 3;

    // SAFETY: the scalar backend needs no CPU feature.
    let scalar = unsafe { Base64Cpu::assume(Base64Backend::Scalar) };
    let expected = one_shot(
        input,
        Base64Codec {
            cpu: scalar,
            wrap: None,
        },
    );

    for cpu in cpus() {
        let codec = Base64Codec { cpu, wrap: None };
        assert_eq!(one_shot(input, codec), expected, "{cpu:?}");

        let mut decoder = Base64Decode::new(codec);
        let mut out = Vec::new();
        let mut buffer = vec![0; room];

        for mut src in input.chunks(chunk) {
            while !src.is_empty() {
                let output = decoder.resume(src, &mut buffer);
                out.extend_from_slice(&buffer[..output.written]);
                src = &src[output.read..];
            }
        }

        let end = decoder.finish(&mut buffer).unwrap();
        out.extend_from_slice(&buffer[..end.written]);
        assert_eq!((out, end.clean), expected, "{cpu:?} resumed");
    }
});

fn one_shot(input: &[u8], codec: Base64Codec) -> (Vec<u8>, bool) {
    let mut out = vec![0; codec.decoded_len_max(input.len())];
    let output = codec.decode(input, &mut out).unwrap();
    out.truncate(output.written);
    (out, output.clean)
}

fn cpus() -> Vec<Base64Cpu> {
    let detected = Base64Cpu::detect();
    let mut cpus = vec![detected];
    cpus.extend(below(detected.backend()));
    cpus
}

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
