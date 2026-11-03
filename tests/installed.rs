//! # Installed token
//!
//! A default codec and the token the application installs, alone in its
//! binary since the token is the process's.

use base64_codec::{codec::Base64Codec, cpu::Base64Cpu};
use simd_cpu::SimdCpu;

#[test]
fn a_default_codec_runs_the_installed_token() {
    let before = Base64Codec::default();
    assert_eq!(before, Base64Codec::BASELINE);

    let cpu = SimdCpu::install_detected().unwrap();
    let after = Base64Codec::default();
    let explicit = Base64Codec {
        cpu: Base64Cpu::new(SimdCpu::NONE),
        ..Base64Codec::default()
    };

    assert_eq!(after.cpu, Base64Cpu::new(cpu));
    assert_eq!(explicit.cpu, Base64Cpu::new(SimdCpu::NONE));
    assert_eq!(before, Base64Codec::BASELINE);

    let encoded = [before, after].map(|codec| {
        let mut out = [0; 8];
        codec.encode(b"hi!?", &mut out).unwrap();
        out
    });
    assert_eq!(encoded, [*b"aGkhPw==", *b"aGkhPw=="]);
}
