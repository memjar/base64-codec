//! Every backend decodes any input, in one call or resumed over chunks of
//! any size into a buffer of any size, to the bytes and flag of the scalar
//! backend; and never panics.

#![no_main]

use base64_codec::{
    cpu::{Base64Backend, Base64Cpu},
    decode::{Base64Decode, decode, decoded_len_max},
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
    let expected = one_shot(input, scalar);

    for cpu in cpus() {
        assert_eq!(one_shot(input, cpu), expected, "{cpu:?}");

        let mut decoder = Base64Decode::new(cpu);
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

fn one_shot(input: &[u8], cpu: Base64Cpu) -> (Vec<u8>, bool) {
    let mut out = vec![0; decoded_len_max(input.len())];
    let output = decode(input, &mut out, cpu).unwrap();
    out.truncate(output.written);
    (out, output.clean)
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
