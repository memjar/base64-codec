//! Every backend decodes any input in either alphabet, one-shot or resumed
//! over any chunks and room, to the bytes and flag of scalar, without
//! panicking.

#![no_main]

use base64_codec::{
    codec::{Base64Alphabet, Base64Codec},
    cpu::Base64Cpu,
    decode::Base64Decode,
};
use libfuzzer_sys::fuzz_target;
use simd_cpu::{SimdCpu, SimdFeature};

fuzz_target!(|data: &[u8]| {
    let [chunk, room, input @ ..] = data else {
        return;
    };

    let chunk = *chunk as usize + 1;

    let alphabet = match room / 64 % 2 {
        0 => Base64Alphabet::Standard,
        _ => Base64Alphabet::Url,
    };

    let room = *room as usize % 64 + 3;

    let scalar = Base64Codec {
        cpu: Base64Cpu::new(SimdCpu::NONE),
        alphabet,
        ..Base64Codec::BASELINE
    };

    let expected = one_shot(input, scalar);

    for cpu in cpus() {
        let codec = Base64Codec { cpu, ..scalar };
        assert_eq!(one_shot(input, codec), expected, "{codec:?}");

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
        assert_eq!((out, end.clean), expected, "{codec:?} resumed");
    }
});

fn one_shot(input: &[u8], codec: Base64Codec) -> (Vec<u8>, bool) {
    let mut out = vec![0; codec.decoded_len_max(input.len())];
    let output = codec.decode(input, &mut out).unwrap();
    out.truncate(output.written);
    (out, output.clean)
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
