//! # CPU token
//!
//! [`Base64Cpu`], the backend a [`Base64Codec`](crate::codec::Base64Codec)
//! runs: the application picks it, the library never detects CPU features.

use crate::driver::Loops;

/// The backend a codec runs, by construction one the running CPU supports.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Base64Cpu {
    backend: Base64Backend,
}

impl Base64Cpu {
    /// The best backend the build's target features guarantee: scalar on a
    /// default x86_64 build, NEON on aarch64.
    pub const BASELINE: Self = Self {
        backend: BASELINE_BACKEND,
    };

    /// Names `backend` without checking the running CPU.
    ///
    /// # Safety
    ///
    /// The running CPU must support the instructions of `backend`.
    pub const unsafe fn assume(backend: Base64Backend) -> Self {
        Self { backend }
    }

    /// The best backend the running CPU supports, detected at run time.
    #[cfg(feature = "std")]
    #[cfg_attr(docsrs, doc(cfg(feature = "std")))]
    pub fn detect() -> Self {
        Self {
            backend: detect_backend(),
        }
    }

    /// The backend this token names.
    pub const fn backend(self) -> Base64Backend {
        self.backend
    }

    pub(crate) fn loops(self) -> &'static Loops {
        match self.backend {
            Base64Backend::Scalar => &crate::scalar::LOOPS,
            #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
            Base64Backend::Ssse3 => &crate::ssse3::LOOPS,
            #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
            Base64Backend::Avx2 => &crate::avx2::LOOPS,
            #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
            Base64Backend::Avx512 => &crate::avx512::LOOPS,
            #[cfg(all(target_arch = "aarch64", target_feature = "neon"))]
            Base64Backend::Neon => &crate::neon::LOOPS,
        }
    }
}

impl Default for Base64Cpu {
    fn default() -> Self {
        Self::BASELINE
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
const BASELINE_BACKEND: Base64Backend = widest(
    cfg!(target_feature = "avx512vbmi"),
    cfg!(target_feature = "avx2"),
    cfg!(target_feature = "ssse3"),
);

#[cfg(all(target_arch = "aarch64", target_feature = "neon"))]
const BASELINE_BACKEND: Base64Backend = Base64Backend::Neon;

#[cfg(not(any(
    target_arch = "x86",
    target_arch = "x86_64",
    all(target_arch = "aarch64", target_feature = "neon")
)))]
const BASELINE_BACKEND: Base64Backend = Base64Backend::Scalar;

#[cfg(all(
    any(feature = "std", test),
    any(target_arch = "x86", target_arch = "x86_64")
))]
fn detect_backend() -> Base64Backend {
    widest(
        std::arch::is_x86_feature_detected!("avx512vbmi"),
        std::arch::is_x86_feature_detected!("avx2"),
        std::arch::is_x86_feature_detected!("ssse3"),
    )
}

#[cfg(all(
    any(feature = "std", test),
    not(any(target_arch = "x86", target_arch = "x86_64"))
))]
fn detect_backend() -> Base64Backend {
    BASELINE_BACKEND
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
const fn widest(avx512: bool, avx2: bool, ssse3: bool) -> Base64Backend {
    match (avx512, avx2, ssse3) {
        (true, _, _) => Base64Backend::Avx512,
        (_, true, _) => Base64Backend::Avx2,
        (_, _, true) => Base64Backend::Ssse3,
        _ => Base64Backend::Scalar,
    }
}

#[cfg(test)]
pub(crate) fn available() -> alloc::vec::Vec<Base64Cpu> {
    let mut cpus = alloc::vec::Vec::new();
    let mut backend = Some(detect_backend());

    while let Some(next) = backend {
        cpus.push(Base64Cpu { backend: next });

        backend = match next {
            Base64Backend::Scalar => None,
            #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
            Base64Backend::Ssse3 => Some(Base64Backend::Scalar),
            #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
            Base64Backend::Avx2 => Some(Base64Backend::Ssse3),
            #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
            Base64Backend::Avx512 => Some(Base64Backend::Avx2),
            #[cfg(all(target_arch = "aarch64", target_feature = "neon"))]
            Base64Backend::Neon => Some(Base64Backend::Scalar),
        };
    }

    cpus
}
