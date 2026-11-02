//! # CPU token
//!
//! [`Base64Cpu`], the backend a [`Base64Codec`](crate::codec::Base64Codec)
//! runs, picked from a [`SimdCpu`]: one the caller hands over, or by default
//! the one the application installed. The library never detects CPU
//! features.

use simd_cpu::SimdCpu;

use crate::driver::Loops;

/// The backend a codec runs, by construction one the running CPU supports.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Base64Cpu {
    backend: Base64Backend,
}

impl Base64Cpu {
    /// The widest backend `cpu` has the features of: on x86 AVX-512 VBMI,
    /// then AVX2, then SSSE3; NEON on aarch64; scalar otherwise.
    pub const fn new(cpu: SimdCpu) -> Self {
        Self {
            backend: widest(cpu),
        }
    }

    /// The backend this token names.
    pub const fn backend(self) -> Base64Backend {
        self.backend
    }

    /// The loops of this backend for the URL alphabet or the standard one.
    pub(crate) fn loops(self, url: bool) -> &'static Loops {
        let loops = match self.backend {
            Base64Backend::Scalar => &crate::scalar::LOOPS,
            #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
            Base64Backend::Ssse3 => &crate::ssse3::LOOPS,
            #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
            Base64Backend::Avx2 => &crate::avx2::LOOPS,
            #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
            Base64Backend::Avx512 => &crate::avx512::LOOPS,
            #[cfg(all(target_arch = "aarch64", target_feature = "neon"))]
            Base64Backend::Neon => &crate::neon::LOOPS,
        };

        &loops[url as usize]
    }
}

impl Default for Base64Cpu {
    /// The backend of [`SimdCpu::current`]: the token the application
    /// installed, or [`SimdCpu::BASELINE`] when it installed none.
    #[inline]
    fn default() -> Self {
        Self::new(SimdCpu::current())
    }
}

/// A backend of the codec.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Base64Backend {
    /// Table lookups, on every target.
    Scalar,
    /// SSSE3 kernels, 16 characters per step.
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    Ssse3,
    /// AVX2 kernels, 32 characters per step.
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    Avx2,
    /// AVX-512 kernels with the VBMI byte permutes, 64 characters per step.
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    Avx512,
    /// NEON kernels, 64 characters per step.
    #[cfg(all(target_arch = "aarch64", target_feature = "neon"))]
    Neon,
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
const fn widest(cpu: SimdCpu) -> Base64Backend {
    use simd_cpu::SimdFeature::{Avx2, Avx512Vbmi, Ssse3};

    if cpu.has(Avx512Vbmi) {
        Base64Backend::Avx512
    } else if cpu.has(Avx2) {
        Base64Backend::Avx2
    } else if cpu.has(Ssse3) {
        Base64Backend::Ssse3
    } else {
        Base64Backend::Scalar
    }
}

#[cfg(all(target_arch = "aarch64", target_feature = "neon"))]
const fn widest(cpu: SimdCpu) -> Base64Backend {
    use simd_cpu::SimdFeature::Neon;

    if cpu.has(Neon) {
        Base64Backend::Neon
    } else {
        Base64Backend::Scalar
    }
}

#[cfg(not(any(
    target_arch = "x86",
    target_arch = "x86_64",
    all(target_arch = "aarch64", target_feature = "neon")
)))]
const fn widest(_: SimdCpu) -> Base64Backend {
    Base64Backend::Scalar
}

/// The backends the running CPU supports, widest first.
#[cfg(test)]
pub(crate) fn available() -> alloc::vec::Vec<Base64Cpu> {
    use simd_cpu::SimdFeature::{Avx2, Avx512Vbmi, Neon, Ssse3};

    let mut cpu = SimdCpu::detect();
    let mut cpus = alloc::vec![Base64Cpu::new(cpu)];

    for feature in [Avx512Vbmi, Avx2, Ssse3, Neon] {
        cpu = cpu.without(feature);
        let next = Base64Cpu::new(cpu);

        if !cpus.contains(&next) {
            cpus.push(next);
        }
    }

    cpus
}
