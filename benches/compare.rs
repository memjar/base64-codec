//! base64-codec against the base64 crate, base64-simd and encodify, from 64
//! bytes to 10 MiB, unwrapped and in CRLF lines, as benches/README.md tells.

use std::{hint::black_box, time::Duration};

use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use base64_codec::{
    codec::{Base64Codec, Base64Eol},
    cpu::Base64Cpu,
};
use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use encodify::base64::{MIME as ENCODIFY_MIME, STANDARD as ENCODIFY};
use simd_cpu::{SimdCpu, SimdFeature};

const SIZES: [usize; 4] = [64, 4 * 1024, 1024 * 1024, 10 * 1024 * 1024];

const WRAPS: [(&str, Option<Base64Eol>); 2] =
    [("unwrapped", None), ("wrapped", Some(Base64Eol::Crlf))];

fn input(len: usize) -> Vec<u8> {
    let mut state = 0x9e37_79b9_7f4a_7c15_u64;

    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state as u8
        })
        .collect()
}

fn cpus() -> Vec<(&'static str, Base64Cpu)> {
    let detected = SimdCpu::detect();
    let mut cpus = vec![
        ("base64-codec", Base64Cpu::new(detected)),
        ("base64-codec scalar", Base64Cpu::new(SimdCpu::NONE)),
    ];

    if detected.has(SimdFeature::Avx512Vbmi) {
        let avx2 = detected.without(SimdFeature::Avx512Vbmi);
        cpus.push(("base64-codec avx2", Base64Cpu::new(avx2)));
    }

    cpus
}

fn encodes(c: &mut Criterion) {
    for (name, wrap) in WRAPS {
        let baseline = Base64Codec {
            wrap,
            ..Base64Codec::BASELINE
        };

        let mut group = c.benchmark_group(format!("encode {name}"));
        group.warm_up_time(Duration::from_secs(1));
        group.measurement_time(Duration::from_secs(3));

        for size in SIZES {
            let src = input(size);
            let mut out = vec![0; baseline.encoded_len(size).unwrap() + 64];
            group.throughput(Throughput::Bytes(size as u64));

            for (label, cpu) in cpus() {
                let codec = Base64Codec { cpu, wrap };

                group.bench_with_input(BenchmarkId::new(label, size), &src, |b, src| {
                    b.iter(|| codec.encode(black_box(src), &mut out))
                });
            }

            match wrap {
                None => {
                    group.bench_with_input(BenchmarkId::new("base64", size), &src, |b, src| {
                        b.iter(|| BASE64.encode_slice(black_box(src), &mut out))
                    });

                    group.bench_with_input(
                        BenchmarkId::new("base64-simd", size),
                        &src,
                        |b, src| {
                            b.iter(|| {
                                base64_simd::STANDARD
                                    .encode(black_box(src), base64_simd::Out::from_slice(&mut out))
                                    .len()
                            })
                        },
                    );

                    group.bench_with_input(BenchmarkId::new("encodify", size), &src, |b, src| {
                        b.iter(|| ENCODIFY.encode_slice(black_box(src), &mut out))
                    });
                }
                Some(_) => {
                    group.bench_with_input(BenchmarkId::new("base64", size), &src, |b, src| {
                        b.iter(|| {
                            let mut written = 0;

                            for chunk in black_box(src).chunks(57) {
                                written += BASE64.encode_slice(chunk, &mut out[written..]).unwrap();
                                out[written..written + 2].copy_from_slice(b"\r\n");
                                written += 2;
                            }

                            written
                        })
                    });

                    group.bench_with_input(BenchmarkId::new("encodify", size), &src, |b, src| {
                        b.iter(|| ENCODIFY_MIME.encode_slice(black_box(src), &mut out))
                    });
                }
            }
        }

        group.finish();
    }
}

fn decodes(c: &mut Criterion) {
    for (name, wrap) in WRAPS {
        let baseline = Base64Codec {
            wrap,
            ..Base64Codec::BASELINE
        };

        let mut group = c.benchmark_group(format!("decode {name}"));
        group.warm_up_time(Duration::from_secs(1));
        group.measurement_time(Duration::from_secs(3));

        for size in SIZES {
            let raw = input(size);
            let mut src = vec![0; baseline.encoded_len(size).unwrap()];
            baseline.encode(&raw, &mut src).unwrap();

            let mut out = vec![0; src.len() + 64];
            group.throughput(Throughput::Bytes(size as u64));

            for (label, cpu) in cpus() {
                let codec = Base64Codec { cpu, wrap };

                group.bench_with_input(BenchmarkId::new(label, size), &src, |b, src| {
                    b.iter(|| codec.decode(black_box(src), &mut out))
                });
            }

            match wrap {
                None => {
                    group.bench_with_input(BenchmarkId::new("base64", size), &src, |b, src| {
                        b.iter(|| BASE64.decode_slice(black_box(src), &mut out))
                    });

                    group.bench_with_input(
                        BenchmarkId::new("base64-simd", size),
                        &src,
                        |b, src| {
                            b.iter(|| {
                                base64_simd::STANDARD
                                    .decode(black_box(src), base64_simd::Out::from_slice(&mut out))
                                    .map(|out| out.len())
                            })
                        },
                    );

                    group.bench_with_input(BenchmarkId::new("encodify", size), &src, |b, src| {
                        b.iter(|| ENCODIFY.decode_slice(black_box(src), &mut out))
                    });

                    group.bench_with_input(
                        BenchmarkId::new("encodify lenient", size),
                        &src,
                        |b, src| b.iter(|| ENCODIFY_MIME.decode_slice(black_box(src), &mut out)),
                    );
                }
                Some(_) => {
                    group.bench_with_input(BenchmarkId::new("base64", size), &src, |b, src| {
                        b.iter(|| {
                            let mut written = 0;

                            for line in black_box(src).split(|byte| *byte == b'\n') {
                                let line = line.strip_suffix(b"\r").unwrap_or(line);
                                written += BASE64.decode_slice(line, &mut out[written..]).unwrap();
                            }

                            written
                        })
                    });

                    group.bench_with_input(
                        BenchmarkId::new("base64-simd", size),
                        &src,
                        |b, src| {
                            b.iter(|| {
                                base64_simd::forgiving_decode(
                                    black_box(src),
                                    base64_simd::Out::from_slice(&mut out),
                                )
                                .map(|out| out.len())
                            })
                        },
                    );

                    group.bench_with_input(BenchmarkId::new("encodify", size), &src, |b, src| {
                        b.iter(|| ENCODIFY_MIME.decode_slice(black_box(src), &mut out))
                    });
                }
            }
        }

        group.finish();
    }
}

criterion_group!(benches, encodes, decodes);
criterion_main!(benches);
