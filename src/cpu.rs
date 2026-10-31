//! # CPU token
//!
//! [`Base64Cpu`] names the backend every encoding and decoding call runs.
//! The library never detects CPU features itself: the application does it
//! once, with `Base64Cpu::detect` or by its own means, and passes the token
//! down, while [`Base64Cpu::BASELINE`] needs no detection at all.

use crate::driver::Loops;

/// The backend an encoding or decoding call runs.
///
/// A token only ever names a backend the running CPU supports, since running
/// an instruction the CPU lacks is undefined behaviour: it comes from
/// [`BASELINE`](Self::BASELINE), `detect` (behind the `std` feature) or the
/// unsafe [`assume`](Self::assume).
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Base64Cpu {
    backend: Base64Backend,
}

impl Base64Cpu {
    /// The best backend the build target guarantees, read from its target
    /// features: scalar on a default x86_64 build, AVX2 or AVX-512 on a build
    /// for a CPU that has it (`-C target-cpu`), NEON on aarch64.
    pub const BASELINE: Self = Self {
        backend: BASELINE_BACKEND,
    };

    /// Names `backend`, trusting the caller that the running CPU supports
    /// it.
    ///
    /// # Safety
    ///
    /// The running CPU must support the instructions of `backend`: SSSE3 for
    /// `Base64Backend::Ssse3`, AVX2 for `Base64Backend::Avx2`, AVX-512 VBMI
    /// for `Base64Backend::Avx512`.
    /// [`Base64Backend::Scalar`] and `Base64Backend::Neon`, which exists only
    /// on targets guaranteeing NEON, need nothing.
    pub const unsafe fn assume(backend: Base64Backend) -> Self {
        Self { backend }
    }

    /// The best backend the running CPU supports, detected at run time.
    ///
    /// Meant to be called once by the application, which then passes the
    /// token down to every call.
    #[cfg(feature = "std")]
    #[cfg_attr(docsrs, doc(cfg(feature = "std")))]
    pub fn detect() -> Self {
        Self {
            backend: detect_backend(),
        }
    }

    /// The backend this token runs.
    pub const fn backend(self) -> Base64Backend {
        self.backend
    }

    /// The loops of the backend this token runs.
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

/// The widest x86 backend of the features given: AVX-512 VBMI, AVX2, SSSE3.
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
const fn widest(avx512: bool, avx2: bool, ssse3: bool) -> Base64Backend {
    match (avx512, avx2, ssse3) {
        (true, _, _) => Base64Backend::Avx512,
        (_, true, _) => Base64Backend::Avx2,
        (_, _, true) => Base64Backend::Ssse3,
        _ => Base64Backend::Scalar,
    }
}

/// Every token the running CPU supports, the detected one first.
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
