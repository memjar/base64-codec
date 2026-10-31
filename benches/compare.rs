//! Throughput of base64-codec against the base64 crate, base64-simd and
//! encodify, encoding and decoding, unwrapped and in CRLF lines of 76
//! characters, from 64 bytes to 10 MiB. Throughput counts the raw bytes, on
//! both sides.

use std::{hint::black_box, time::Duration};

use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use base64_codec::{
    cpu::{Base64Backend, Base64Cpu},
    decode::decode,
    encode::{Base64EncodeOptions, Base64Eol, encode, encoded_len},
};
use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use encodify::base64::{MIME as ENCODIFY_MIME, STANDARD as ENCODIFY};

const SIZES: [usize; 4] = [64, 4 * 1024, 1024 * 1024, 10 * 1024 * 1024];

const UNWRAPPED: Base64EncodeOptions = Base64EncodeOptions { wrap: None };

const WRAPPED: Base64EncodeOptions = Base64EncodeOptions {
    wrap: Some(Base64Eol::Crlf),
};

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
    // SAFETY: the scalar backend needs no CPU feature.
    let scalar = unsafe { Base64Cpu::assume(Base64Backend::Scalar) };
    let mut cpus = vec![
        ("base64-codec", Base64Cpu::detect()),
        ("base64-codec scalar", scalar),
    ];

    cpus.extend(avx2());
    cpus
}

/// AVX2 apart when AVX-512 is detected, to compare it with encodify on the
/// same instructions.
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn avx2() -> Option<(&'static str, Base64Cpu)> {
    let avx512 = Base64Cpu::detect().backend() == Base64Backend::Avx512;

    // SAFETY: a CPU with AVX-512 VBMI has AVX2.
    avx512.then(|| {
        ("base64-codec avx2", unsafe {
            Base64Cpu::assume(Base64Backend::Avx2)
        })
    })
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
fn avx2() -> Option<(&'static str, Base64Cpu)> {
    None
}

fn encodes(c: &mut Criterion) {
    for (name, opts) in [("unwrapped", UNWRAPPED), ("wrapped", WRAPPED)] {
        let mut group = c.benchmark_group(format!("encode {name}"));
        group.warm_up_time(Duration::from_secs(1));
        group.measurement_time(Duration::from_secs(3));

        for size in SIZES {
            let src = input(size);
            let mut out = vec![0; encoded_len(size, opts).unwrap() + 64];
            group.throughput(Throughput::Bytes(size as u64));

            for (label, cpu) in cpus() {
                group.bench_with_input(BenchmarkId::new(label, size), &src, |b, src| {
                    b.iter(|| encode(black_box(src), &mut out, cpu, opts))
                });
            }

            match opts.wrap {
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
                    // NOTE: the base64 crate has no wrapping: lines of 57
                    // bytes, as mime-rs's wrapper writes them.
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
    for (name, opts) in [("unwrapped", UNWRAPPED), ("wrapped", WRAPPED)] {
        let mut group = c.benchmark_group(format!("decode {name}"));
        group.warm_up_time(Duration::from_secs(1));
        group.measurement_time(Duration::from_secs(3));

        for size in SIZES {
            let raw = input(size);
            let mut src = vec![0; encoded_len(size, opts).unwrap()];
            encode(&raw, &mut src, Base64Cpu::BASELINE, opts).unwrap();

            // NOTE: as long as the input, which base64-simd's forgiving
            // decoder asks for.
            let mut out = vec![0; src.len() + 64];
            group.throughput(Throughput::Bytes(size as u64));

            for (label, cpu) in cpus() {
                group.bench_with_input(BenchmarkId::new(label, size), &src, |b, src| {
                    b.iter(|| decode(black_box(src), &mut out, cpu))
                });
            }

            match opts.wrap {
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

                    // NOTE: encodify's lenient engine, the one whose rules
                    // are base64-codec's, on the same input.
                    group.bench_with_input(
                        BenchmarkId::new("encodify lenient", size),
                        &src,
                        |b, src| b.iter(|| ENCODIFY_MIME.decode_slice(black_box(src), &mut out)),
                    );
                }
                Some(_) => {
                    // NOTE: the base64 crate rejects line breaks: line by
                    // line, as mime-rs's wrapper reads them.
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
