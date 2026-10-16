//! Which `tanh`, `exp`, `ln` and integer power a native training run
//! computes with.
//!
//! [`TrainingMathV1::PlatformLibm`], the default, is unchanged: every call
//! goes to the platform's own libm (`tanhf`, `expf`, `logf`, `exp`, `log`),
//! whose last bits differ between glibc and the Windows UCRT, so the same
//! run trains different bits on the two systems. Adam's bias corrections
//! use `f64::powi`, which LLVM lowers to compiler-builtins' square-and-
//! multiply on Linux and to the CRT's `pow` on Windows.
//!
//! [`TrainingMathV1::PortableV1`] computes `tanh` with
//! [`deterministic_math_v1::tanh_f32_v1`] (four elements at a time where
//! AVX2 is available, with the same bits), `exp` and `ln` with the pinned
//! pure-Rust `libm` crate (0.2.16, a port of FreeBSD's msun), and integer
//! powers by square-and-multiply in compiler-builtins' order. All of them
//! use only IEEE-754 arithmetic and exactly specified operations, so they
//! give the same bits on every x86-64 system.
//!
//! A run uses one of them for all of its computation: the policy forwards
//! of collection (learner and opponents alike), the update's replay checks
//! and learner tape, and the learner's loss, its gradient, the scorer-bias
//! gauge and Adam's bias corrections. Records name the choice only when it
//! is not the default, so every existing config, command, trajectory,
//! checkpoint and receipt keeps its bytes.

use crate::deterministic_math_v1;
use crate::native_policy_value_net_v1::ForwardActivationModeV1;
use serde::{Deserialize, Serialize};

/// The record value of [`TrainingMathV1::PortableV1`].
pub(crate) const PORTABLE_MATH_IDENTITY_V1: &str = "portable-math-v1";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrainingMathV1 {
    #[default]
    #[serde(rename = "platform-libm")]
    PlatformLibm,
    #[serde(rename = "portable-math-v1")]
    PortableV1,
}

impl TrainingMathV1 {
    pub(crate) fn is_platform_libm(&self) -> bool {
        *self == Self::PlatformLibm
    }

    /// The value records carry; `None` for the default, which they omit.
    pub(crate) fn identity_v1(&self) -> Option<&'static str> {
        match self {
            Self::PlatformLibm => None,
            Self::PortableV1 => Some(PORTABLE_MATH_IDENTITY_V1),
        }
    }

    /// Writes the math into a receipt exactly when it is not the default.
    pub(crate) fn record_v1(&self, document: &mut serde_json::Value) {
        if let Some(identity) = self.identity_v1() {
            document["training_math"] = serde_json::json!(identity);
        }
    }

    /// The forward pass's activation for this math. `PortableV1` is the
    /// existing bit-defined `KernelDeterministicTanh`.
    pub(crate) fn forward_activation_v1(self) -> ForwardActivationModeV1 {
        match self {
            Self::PlatformLibm => ForwardActivationModeV1::LibmTanh,
            Self::PortableV1 => ForwardActivationModeV1::KernelDeterministicTanh,
        }
    }

    pub(crate) fn tanh_in_place_v1(self, values: &mut [f32]) {
        match self {
            Self::PlatformLibm => {
                for value in values {
                    *value = value.tanh();
                }
            }
            Self::PortableV1 => deterministic_math_v1::tanh_f32_in_place_v1(values),
        }
    }

    #[inline]
    pub(crate) fn exp_f32_v1(self, x: f32) -> f32 {
        match self {
            Self::PlatformLibm => x.exp(),
            Self::PortableV1 => libm::expf(x),
        }
    }

    #[inline]
    pub(crate) fn ln_f32_v1(self, x: f32) -> f32 {
        match self {
            Self::PlatformLibm => x.ln(),
            Self::PortableV1 => libm::logf(x),
        }
    }

    #[inline]
    pub(crate) fn exp_f64_v1(self, x: f64) -> f64 {
        match self {
            Self::PlatformLibm => x.exp(),
            Self::PortableV1 => libm::exp(x),
        }
    }

    #[inline]
    pub(crate) fn ln_f64_v1(self, x: f64) -> f64 {
        match self {
            Self::PlatformLibm => x.ln(),
            Self::PortableV1 => libm::log(x),
        }
    }

    /// `base` to an integer power (Adam's bias corrections). The portable
    /// math gives Linux's `powi` bits on every system.
    #[inline]
    pub(crate) fn powi_f64_v1(self, base: f64, exponent: i32) -> f64 {
        match self {
            Self::PlatformLibm => base.powi(exponent),
            Self::PortableV1 => powi_f64_portable_v1(base, exponent),
        }
    }
}

/// compiler-builtins' `__powidf2`, which `f64::powi` calls on Linux:
/// square-and-multiply from the exponent's lowest bit, then the reciprocal
/// for a negative exponent.
fn powi_f64_portable_v1(mut base: f64, exponent: i32) -> f64 {
    let mut remaining = exponent.unsigned_abs();
    let mut power = 1.0f64;
    loop {
        if remaining & 1 != 0 {
            power *= base;
        }
        remaining >>= 1;
        if remaining == 0 {
            break;
        }
        base *= base;
    }
    if exponent < 0 {
        1.0 / power
    } else {
        power
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The wire names, and the default's omission from records.
    #[test]
    fn records_name_only_the_portable_math() {
        assert_eq!(
            serde_json::to_string(&TrainingMathV1::PortableV1).unwrap(),
            "\"portable-math-v1\""
        );
        assert_eq!(
            serde_json::from_str::<TrainingMathV1>("\"platform-libm\"").unwrap(),
            TrainingMathV1::PlatformLibm
        );
        assert!(serde_json::from_str::<TrainingMathV1>("\"portable\"").is_err());
        assert_eq!(TrainingMathV1::default().identity_v1(), None);
        assert_eq!(
            TrainingMathV1::PortableV1.identity_v1(),
            Some(PORTABLE_MATH_IDENTITY_V1)
        );
        let mut document = serde_json::json!({"schema": "receipt"});
        TrainingMathV1::PlatformLibm.record_v1(&mut document);
        assert_eq!(document, serde_json::json!({"schema": "receipt"}));
        TrainingMathV1::PortableV1.record_v1(&mut document);
        assert_eq!(
            document,
            serde_json::json!({"schema": "receipt", "training_math": "portable-math-v1"})
        );
    }

    /// Pinned bits of the portable functions on a few inputs. The same
    /// constants hold on every system, which the Windows CI job checks.
    /// `ln(3.5)` is one ulp above glibc's `logf` (0x3fa0_5a89): the portable
    /// functions are bit-defined, not glibc's.
    #[test]
    fn portable_math_bits_are_pinned() {
        type Case<T, B> = (fn(TrainingMathV1, T) -> T, T, B);
        let math = TrainingMathV1::PortableV1;
        let f32_cases: [Case<f32, u32>; 6] = [
            (TrainingMathV1::exp_f32_v1, -0.75, 0x3ef1_da07),
            (TrainingMathV1::exp_f32_v1, -13.25, 0x35ec_450a),
            (TrainingMathV1::exp_f32_v1, 0.0, 0x3f80_0000),
            (TrainingMathV1::ln_f32_v1, 1.0, 0x0000_0000),
            (TrainingMathV1::ln_f32_v1, 3.5, 0x3fa0_5a8a),
            (TrainingMathV1::ln_f32_v1, 1.000_001, 0x357f_fff8),
        ];
        for (function, x, bits) in f32_cases {
            assert_eq!(function(math, x).to_bits(), bits, "f32 {x}");
        }
        let f64_cases: [Case<f64, u64>; 4] = [
            (TrainingMathV1::exp_f64_v1, -0.75, 0x3fde_3b40_ebef_cd7e),
            (TrainingMathV1::exp_f64_v1, -13.25, 0x3ebd_88a1_3d37_8348),
            (TrainingMathV1::ln_f64_v1, 3.5, 0x3ff4_0b51_2eb5_3d60),
            (TrainingMathV1::ln_f64_v1, 1.0, 0x0000_0000_0000_0000),
        ];
        for (function, x, bits) in f64_cases {
            assert_eq!(function(math, x).to_bits(), bits, "f64 {x}");
        }
        let inputs = [0.5f32, -2.0, 1e-5, 20.0, f32::NAN];
        let mut values = inputs;
        math.tanh_in_place_v1(&mut values);
        assert_eq!(
            values.map(f32::to_bits),
            inputs.map(|x| deterministic_math_v1::tanh_f32_v1(x).to_bits())
        );
    }

    /// Adam's bias-correction powers under the portable math, pinned for
    /// every system. From the third step on they need not equal the CRT's
    /// `pow`, which `f64::powi` becomes on Windows.
    #[test]
    fn portable_adam_powers_are_pinned() {
        let math = TrainingMathV1::PortableV1;
        let beta1 = f64::from(crate::native_policy_train_step_v1::ADAM_BETA1_V1);
        let beta2 = f64::from(crate::native_policy_train_step_v1::ADAM_BETA2_V1);
        let cases: [(f64, i32, u64); 10] = [
            (beta1, 0, 0x3ff0_0000_0000_0000),
            (beta1, 1, 0x3fec_cccc_c000_0000),
            (beta1, 3, 0x3fe7_53f7_afbe_76d7),
            (beta1, 7, 0x3fde_9c6a_3a95_d7ed),
            (beta1, 1000, 0x366f_ee3c_a448_6bc3),
            (beta2, 3, 0x3fef_e772_ea08_cf5d),
            (beta2, 7, 0x3fef_c6d4_173a_55d1),
            (beta2, 1000, 0x3fd7_8866_4436_6271),
            (beta2, 4096, 0x3f91_0123_cdf1_ed50),
            (beta2, 40_000, 0x3c53_36b9_cf15_cc68),
        ];
        for (base, exponent, bits) in cases {
            assert_eq!(
                math.powi_f64_v1(base, exponent).to_bits(),
                bits,
                "{base}^{exponent}"
            );
        }
        assert_eq!(math.powi_f64_v1(2.0, -3), 0.125);
        assert_eq!(math.powi_f64_v1(-1.5, 3), -3.375);
    }

    /// Where `f64::powi` is compiler-builtins' `__powidf2` (everywhere but
    /// Windows), the portable power gives its bits, so portable runs on
    /// those systems train exactly what they did with `powi`.
    #[cfg(not(windows))]
    #[test]
    fn portable_powers_equal_compiler_builtins_powi() {
        let beta1 = f64::from(crate::native_policy_train_step_v1::ADAM_BETA1_V1);
        let beta2 = f64::from(crate::native_policy_train_step_v1::ADAM_BETA2_V1);
        for base in [beta1, beta2, 0.5, 1.000_000_1, 3.7, -1.25] {
            for exponent in (-200..=50_000).chain([i32::MAX, i32::MIN + 1]) {
                assert_eq!(
                    TrainingMathV1::PortableV1
                        .powi_f64_v1(base, exponent)
                        .to_bits(),
                    std::hint::black_box(base)
                        .powi(std::hint::black_box(exponent))
                        .to_bits(),
                    "{base}^{exponent}"
                );
            }
        }
    }
}
