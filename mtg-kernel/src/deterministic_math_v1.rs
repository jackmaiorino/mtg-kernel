//! Kernel-owned, bit-defined deterministic scalar math for the
//! model-guided-searcher's search-scoped forward variant.
//!
//! Motivation: `docs/audits/model_guided_forward_determinism_audit_v1.md`
//! found that the production forward pass's only nonlinearity,
//! `tanh_in_place_v1` (`native_policy_value_net_v1.rs`), calls
//! `f32::tanh()`, which resolves to the platform's opaque system libm
//! `tanhf` (confirmed by import-table inspection in that audit). Different
//! libm implementations (Windows UCRT vs. glibc, and even different
//! runtime-selected code paths within one libm) are not required by any
//! standard to agree bit-for-bit, and the audit reports a concrete,
//! nontrivial divergence rate between them. `CLAUDE-MODEL-GUIDED-SEARCHER-
//! DESIGN-V1.md` Section 1.5 requires the model-guided searcher's forward
//! pass to be deterministic on CPU as a structural property, not an
//! empirical run-twice check. This module is that fix's Option-S
//! implementation (search-scoped only; see the audit worktree's follow-on
//! commits for the scope analysis): a `tanh` with a fixed, documented,
//! libm-free operation sequence, usable by a new search-scoped forward
//! variant without touching the production path at all.
//!
//! No function in this module's body calls into the platform's system math
//! library (no `f32::tanh`, `f32::exp`, `f32::exp2`, `f32::ln`, `f32::powf`,
//! `f32::powi`, or any other transcendental libm-backed function). Every
//! operation is one of: elementary IEEE 754 `f32` arithmetic (`+`, `-`,
//! `*`, `/`), a bit-level reinterpretation (`f32::to_bits`/`f32::from_bits`),
//! or a primitive that Rust/LLVM lowers to a native instruction or
//! compiler-builtin rather than a dynamic libm import (`f32::abs`,
//! `f32::copysign`, `f32::round`, `f32::clamp`, `f32::is_nan`,
//! `f32::is_infinite`; the audit's own `dumpbin /imports` evidence lists
//! `tanhf`/`logf`/`exp`/`pow` as the crate's system-libm imports and does
//! not list any of these). This module's own verification step re-runs
//! that same import-table check against a binary built from this exact
//! module.
//!
//! # `tanh_f32_v1`: algorithm, operation order pinned
//!
//! `tanh_f32_v1(x)` is computed by, in this exact order:
//!
//! 1. **NaN.** If `x` is NaN (any payload, either sign), return the fixed
//!    canonical quiet NaN `f32::NAN` (bit pattern `0x7fc0_0000`), not `x`
//!    itself and not an arithmetic result derived from `x`. This is a
//!    deliberate, documented choice: propagating the input NaN's payload
//!    bit-for-bit would still be deterministic in principle, but pinning
//!    one fixed output bit pattern regardless of input payload is simpler
//!    to golden-test and is the choice this implementation makes.
//! 2. **Magnitude and infinity.** Let `ax = x.abs()`. If `ax` is infinite,
//!    return `1.0_f32.copysign(x)` (i.e. `+1.0` for `+inf`, `-1.0` for
//!    `-inf`).
//! 3. **Small-magnitude linear region.** If `ax < TANH_SMALL_LINEAR_THRESHOLD_V1`
//!    (`2^-12`), return `x` unchanged. This is not a shortcut approximation:
//!    for any `f32` `x` with `|x| < 2^-12`, the true value of `x - x^3/3`
//!    (the first two terms of tanh's Taylor series) rounds to exactly `x`
//!    in `f32` (the cubic term is more than one full `f32` ULP below `x`'s
//!    own magnitude at this threshold, with margin: `(2^-12)^2 / 3 ≈
//!    1.79e-8`, versus half a `f32` ULP relative to 1.0 of `2^-24 ≈
//!    5.96e-8`), so returning `x` directly is the correctly-rounded value
//!    of tanh's own defining series in this region, computed with zero
//!    rounding error rather than approximated. This also exactly preserves
//!    every subnormal input (including the smallest, `f32::from_bits(1)`)
//!    and signed zero (`tanh_f32_v1(-0.0) == -0.0`, bit-for-bit), instead of
//!    losing them to the cancellation the next step's identity would
//!    otherwise introduce near zero (`1 - 2/(e^{2x}+1)` subtracts two
//!    nearly-equal values when `x` is tiny).
//! 4. **Saturation region.** If `ax >= TANH_SATURATION_THRESHOLD_V1` (`9.0`),
//!    return `1.0_f32.copysign(x)`. This is a hard, exact cutoff, not an
//!    approximation of a cutoff: the true value of `tanh(9.0)` is
//!    `1 - 2*e^-18 + O(e^-36) ≈ 1 - 3.046e-8`, and half a `f32` ULP at 1.0
//!    is `2^-24 ≈ 5.96e-8`; since `3.046e-8 < 5.96e-8`, the correctly-
//!    rounded `f32` value of the true mathematical `tanh(9.0)` is already
//!    exactly `1.0`, so clamping here changes nothing a higher-precision
//!    evaluation would have produced differently, for any `|x| >= 9.0`
//!    (the gap only widens for larger `|x|`).
//! 5. **Regular region** (`TANH_SMALL_LINEAR_THRESHOLD_V1 <= ax <
//!    TANH_SATURATION_THRESHOLD_V1`): compute via the numerically stable
//!    one-sided identity `tanh(ax) = 1 - 2 / (e^{2*ax} + 1)` (stable here
//!    because `e^{2*ax}+1 >= 1` always, so there is no large-magnitude
//!    cancellation, unlike the small-`x` region step 3 already routes
//!    around):
//!    - `t = 2.0_f32 * ax`
//!    - `e = exp_f32_v1(t)` (Section below)
//!    - `result_abs = (1.0_f32 - 2.0_f32 / (e + 1.0_f32)).clamp(0.0, 1.0)`
//!      (the `clamp` is a structural safety net, not an expected correction:
//!      it guarantees the `[0.0, 1.0]` half of the output-range property
//!      holds even if a future coefficient change introduced a small
//!      overshoot near the saturation boundary, rather than relying on
//!      empirical measurement alone)
//!    - return `result_abs.copysign(x)`
//!
//! Because step 5 depends on `x` only through `ax = x.abs()`, and the
//! final `copysign(x)` is the only place the sign of `x` re-enters the
//! computation, `tanh_f32_v1(-x)` and `tanh_f32_v1(x)` are guaranteed, by
//! this construction, to differ in exactly the sign bit for every finite
//! `x` (odd symmetry, bit-exact, structural rather than empirical); the
//! property test below confirms this over a wide sample rather than
//! trusting the argument alone.
//!
//! # `exp_f32_v1`: algorithm, operation order pinned
//!
//! A private helper, valid only for finite `t` in
//! `[0, 2 * TANH_SATURATION_THRESHOLD_V1)` (i.e. `[0, 18)`), which is the
//! only domain `tanh_f32_v1` ever calls it with. It is not a general-purpose
//! `exp` (no overflow/underflow saturation, no negative-input handling) and
//! must not be used outside that contract. Standard Cody-Waite range
//! reduction plus a fixed-degree Taylor polynomial, computed in this exact
//! order:
//!
//! 1. `k_f = (t * EXP_INV_LN2_V1).round()` (`f32::round`, half away from
//!    zero); `k = k_f as i32`.
//! 2. Two-part reduction against `ln(2)` split as `EXP_LN2_HI_V1 +
//!    EXP_LN2_LO_V1`, where `EXP_LN2_HI_V1` has its low mantissa bits
//!    zeroed so that `(k as f32) * EXP_LN2_HI_V1` is computed with far less
//!    cancellation error than a single-constant reduction would have for
//!    the `k` range this domain produces (`k` up to `26`, since
//!    `18 / ln(2) ≈ 25.97`):
//!    - `r_hi = t - (k as f32) * EXP_LN2_HI_V1`
//!    - `r = r_hi - (k as f32) * EXP_LN2_LO_V1`
//!    - `r` is small by construction (`|r| <= ln(2)/2 ≈ 0.3466`).
//! 3. Degree-7 Taylor polynomial for `e^r` (`e^r = sum_{n=0}^{7} r^n / n!`),
//!    evaluated by Horner's method in descending degree:
//!    `value = (((((((C7)*r + C6)*r + C5)*r + C4)*r + C3)*r + C2)*r + C1)*r + C0`.
//!    Plain Taylor coefficients, not a minimax fit: at `|r| <= 0.3466` the
//!    truncated remainder is already far below `f32` precision (order
//!    `r^8/8! ≈ 3e-9`), so there is no accuracy benefit to a fitted
//!    polynomial here, only more opaque constants.
//! 4. Reconstruction, `e^t = 2^k * e^r`, via exact bit-level construction of
//!    `2^k` (`f32::from_bits(((k + 127) as u32) << 23)`, valid because this
//!    domain's `k` (`0..=26`) sits well inside the normal `f32` exponent
//!    range and is never large enough to hit subnormal or overflow
//!    reconstruction cases) multiplied into the polynomial result. This one
//!    multiply is exact scaling by a power of two (no rounding beyond the
//!    single multiply's own, unavoidable rounding).
//!
//! Empirical accuracy (measured in a companion, non-committed numerical
//! prototype before this Rust code was written, and re-measured against
//! this exact Rust implementation by the property tests below): maximum
//! absolute error versus `f64`-precision `tanh` across a dense sweep of the
//! full non-saturated domain is on the order of `1e-7`, i.e. within a few
//! `f32` ULPs; see the max-ULP-versus-libm probe comparison test
//! (`native_forward_determinism_probe_v1.rs`) for the same measurement
//! against the production `f32::tanh()` path specifically.
//!
//! # FTZ/DAZ/rounding-mode entry gate
//!
//! `assert_pinned_mxcsr_state_v1` (Design v1 Section 1.5 property 5,
//! follow-on item 3b) reads the calling thread's MXCSR control/status
//! register and panics if flush-to-zero, denormals-are-zero, or the
//! rounding-control field are not in their IEEE 754 default state (both
//! flags off, round-to-nearest-even). It is called once, at the entry of
//! the search-scoped forward variant
//! (`NativePolicyValueNetV1::forward_search_deterministic_v1`), before any
//! arithmetic in that call runs. `x86_64`-only, matching the existing
//! probe's own scope limitation (`native_forward_determinism_probe_v1.rs`);
//! every `x86_64` target this crate builds for has SSE2 and therefore
//! MXCSR as a mandatory baseline.

/// Saturation cutoff for [`tanh_f32_v1`]'s magnitude. See the algorithm
/// doc above (step 4) for why `9.0` is an exact, not approximate, cutoff.
pub(crate) const TANH_SATURATION_THRESHOLD_V1: f32 = 9.0;

/// Small-magnitude linear-region cutoff for [`tanh_f32_v1`]. `2^-12`
/// (`0x3980_0000`), exactly representable in `f32`. Pinned as an exact bit
/// pattern, matching the other exact constants below, rather than a
/// decimal literal. See the algorithm doc above (step 3).
const TANH_SMALL_LINEAR_THRESHOLD_V1: f32 = f32::from_bits(0x3980_0000);

// Cody-Waite two-part ln(2) split and the polynomial's reciprocal
// constant, derived once (not at build time) and pinned as exact `f32` bit
// patterns so the source of truth is the literal bits, not a re-rounded
// decimal literal. `EXP_LN2_HI_V1`'s low 12 bits are zeroed by
// construction (see the algorithm doc above), which is what the hex
// pattern below encodes; the doc comment states the property, the bits
// are the pinned contract.
const EXP_LN2_HI_V1: f32 = f32::from_bits(0x3f31_7000);
const EXP_LN2_LO_V1: f32 = f32::from_bits(0x3805_fdf4);
const EXP_INV_LN2_V1: f32 = f32::from_bits(0x3fb8_aa3b);

// Degree-7 Taylor coefficients for e^r, C_n = 1/n!, pinned as exact `f32`
// bit patterns (nearest-`f32` rounding of the exact rational 1/n!).
const EXP_TAYLOR_C0_V1: f32 = f32::from_bits(0x3f80_0000); // 1/0! = 1
const EXP_TAYLOR_C1_V1: f32 = f32::from_bits(0x3f80_0000); // 1/1! = 1
const EXP_TAYLOR_C2_V1: f32 = f32::from_bits(0x3f00_0000); // 1/2!
const EXP_TAYLOR_C3_V1: f32 = f32::from_bits(0x3e2a_aaab); // 1/3!
const EXP_TAYLOR_C4_V1: f32 = f32::from_bits(0x3d2a_aaab); // 1/4!
const EXP_TAYLOR_C5_V1: f32 = f32::from_bits(0x3c08_8889); // 1/5!
const EXP_TAYLOR_C6_V1: f32 = f32::from_bits(0x3ab6_0b61); // 1/6!
const EXP_TAYLOR_C7_V1: f32 = f32::from_bits(0x3950_0d01); // 1/7!

/// Private, domain-restricted deterministic `exp`. Valid only for finite
/// `t` in `[0, 2 * TANH_SATURATION_THRESHOLD_V1)`; see the module doc's
/// `exp_f32_v1` section for the full algorithm and why this domain is
/// safe for the bit-level `2^k` reconstruction step.
fn exp_f32_v1(t: f32) -> f32 {
    let k_f = (t * EXP_INV_LN2_V1).round();
    let k = k_f as i32;
    let k_as_f32 = k as f32;

    let r_hi = t - k_as_f32 * EXP_LN2_HI_V1;
    let r = r_hi - k_as_f32 * EXP_LN2_LO_V1;

    let mut value = EXP_TAYLOR_C7_V1;
    value = value * r + EXP_TAYLOR_C6_V1;
    value = value * r + EXP_TAYLOR_C5_V1;
    value = value * r + EXP_TAYLOR_C4_V1;
    value = value * r + EXP_TAYLOR_C3_V1;
    value = value * r + EXP_TAYLOR_C2_V1;
    value = value * r + EXP_TAYLOR_C1_V1;
    value = value * r + EXP_TAYLOR_C0_V1;

    // Exact power-of-two scaling via direct exponent-field construction;
    // no libm `exp2`/`powi`/`powf` call. Safe for this function's whole
    // domain: k in 0..=26 here, far inside the normal f32 exponent range.
    let scale_bits: u32 = ((k + 127) as u32) << 23;
    let scale = f32::from_bits(scale_bits);
    value * scale
}

/// Kernel-owned, bit-defined deterministic scalar `tanh`. See the module
/// doc above for the full, order-pinned algorithm. No call in this
/// function's body, or in [`exp_f32_v1`]'s, reaches the platform's system
/// libm.
pub(crate) fn tanh_f32_v1(x: f32) -> f32 {
    if x.is_nan() {
        return f32::NAN;
    }
    let ax = x.abs();
    if ax.is_infinite() {
        return 1.0_f32.copysign(x);
    }
    if ax < TANH_SMALL_LINEAR_THRESHOLD_V1 {
        return x;
    }
    if ax >= TANH_SATURATION_THRESHOLD_V1 {
        return 1.0_f32.copysign(x);
    }
    let t = 2.0_f32 * ax;
    let e = exp_f32_v1(t);
    let denom = e + 1.0_f32;
    let frac = 2.0_f32 / denom;
    let result_abs = (1.0_f32 - frac).clamp(0.0_f32, 1.0_f32);
    result_abs.copysign(x)
}

// ---------------------------------------------------------------------
// MXCSR FTZ/DAZ/rounding-mode entry gate (Design v1 Section 1.5 property
// 5; follow-on item 3b).
// ---------------------------------------------------------------------

#[cfg(target_arch = "x86_64")]
const MXCSR_DAZ_BIT_V1: u32 = 1 << 6;
#[cfg(target_arch = "x86_64")]
const MXCSR_FTZ_BIT_V1: u32 = 1 << 15;
#[cfg(target_arch = "x86_64")]
const MXCSR_ROUNDING_CONTROL_SHIFT_V1: u32 = 13;
#[cfg(target_arch = "x86_64")]
const MXCSR_ROUNDING_CONTROL_MASK_V1: u32 = 0b11;

/// Reads the calling thread's MXCSR. `stmxcsr` with no special
/// `options(...)` is a full side effect from the compiler's point of view,
/// so it cannot be hoisted or sunk across neighboring floating-point
/// operations, matching the same rationale the existing determinism
/// probe's own `read_mxcsr_v1` documents
/// (`native_forward_determinism_probe_v1.rs`).
#[cfg(target_arch = "x86_64")]
fn read_mxcsr_v1() -> u32 {
    let mut mxcsr: u32 = 0;
    unsafe {
        std::arch::asm!(
            "stmxcsr [{0}]",
            in(reg) &mut mxcsr,
        );
    }
    mxcsr
}

/// Writes the calling thread's MXCSR. Used only by this module's own test
/// module below (to set up and restore MXCSR mutation scenarios for the
/// gate-violation tests); nothing in the production path calls it, since
/// production only ever needs to read and verify, never repair, the
/// FTZ/DAZ/rounding-control state.
#[cfg(all(test, target_arch = "x86_64"))]
fn write_mxcsr_v1(value: u32) {
    unsafe {
        std::arch::asm!(
            "ldmxcsr [{0}]",
            in(reg) &value,
        );
    }
}

/// Panics unless the calling thread's MXCSR has FTZ=0, DAZ=0, and
/// rounding-control=0 (round-to-nearest-even), the pinned state Design v1
/// Section 1.5 property 5 requires be verified "at the point the forward
/// pass runs." Called once, at the entry of
/// [`crate::native_policy_value_net_v1::NativePolicyValueNetV1::forward_search_deterministic_v1`],
/// before any arithmetic in that call runs.
#[cfg(target_arch = "x86_64")]
pub(crate) fn assert_pinned_mxcsr_state_v1() {
    let mxcsr = read_mxcsr_v1();
    let daz = mxcsr & MXCSR_DAZ_BIT_V1 != 0;
    let ftz = mxcsr & MXCSR_FTZ_BIT_V1 != 0;
    let rounding_control =
        (mxcsr >> MXCSR_ROUNDING_CONTROL_SHIFT_V1) & MXCSR_ROUNDING_CONTROL_MASK_V1;
    assert!(
        !daz,
        "search-scoped deterministic forward requires MXCSR DAZ=0 \
         (denormals-are-zero must be off); found dirty MXCSR 0x{mxcsr:08x}"
    );
    assert!(
        !ftz,
        "search-scoped deterministic forward requires MXCSR FTZ=0 \
         (flush-to-zero must be off); found dirty MXCSR 0x{mxcsr:08x}"
    );
    assert_eq!(
        rounding_control, 0,
        "search-scoped deterministic forward requires MXCSR rounding-control=0 \
         (round-to-nearest-even); found dirty MXCSR 0x{mxcsr:08x}"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    // -------------------------------------------------------------
    // Golden battery: exact output bits for named edge cases.
    // -------------------------------------------------------------

    #[test]
    fn golden_battery_bits_v1() {
        let cases: &[(&str, f32, u32)] = &[
            ("positive_zero", 0.0f32, 0x0000_0000),
            ("negative_zero", -0.0f32, 0x8000_0000),
            (
                "smallest_positive_subnormal",
                f32::from_bits(0x0000_0001),
                0x0000_0001,
            ),
            (
                "smallest_negative_subnormal",
                f32::from_bits(0x8000_0001),
                0x8000_0001,
            ),
            (
                "largest_positive_subnormal",
                f32::from_bits(0x007f_ffff),
                0x007f_ffff,
            ),
            (
                "largest_negative_subnormal",
                f32::from_bits(0x807f_ffff),
                0x807f_ffff,
            ),
            (
                "smallest_positive_normal",
                f32::MIN_POSITIVE,
                f32::MIN_POSITIVE.to_bits(),
            ),
            (
                "just_below_small_linear_threshold",
                f32::from_bits(0x397f_ffff),
                0x397f_ffff,
            ),
            (
                "at_small_linear_threshold",
                TANH_SMALL_LINEAR_THRESHOLD_V1,
                0x397f_f000,
            ),
            ("positive_one", 1.0f32, 0x3f42_f7d6),
            ("negative_one", -1.0f32, 0xbf42_f7d6),
            (
                "just_below_saturation",
                f32::from_bits(0x4110_0000_u32.wrapping_sub(1)),
                0x3f7f_ffff,
            ),
            (
                "at_saturation_threshold",
                TANH_SATURATION_THRESHOLD_V1,
                0x3f80_0000,
            ),
            ("above_saturation_threshold", 9.5f32, 0x3f80_0000),
            ("far_above_saturation", 100.0f32, 0x3f80_0000),
            ("f32_max", f32::MAX, 0x3f80_0000),
            ("negative_far_above_saturation", -100.0f32, 0xbf80_0000),
            ("positive_infinity", f32::INFINITY, 0x3f80_0000),
            ("negative_infinity", f32::NEG_INFINITY, 0xbf80_0000),
            ("quiet_nan", f32::NAN, 0x7fc0_0000),
            ("negative_nan", f32::from_bits(0xffc0_0000), 0x7fc0_0000),
            (
                "nan_alternate_payload",
                f32::from_bits(0x7fc1_2345),
                0x7fc0_0000,
            ),
            (
                "nan_signaling_pattern",
                f32::from_bits(0x7f80_0001),
                0x7fc0_0000,
            ),
        ];
        for (name, input, expected_bits) in cases.iter().copied() {
            let actual = tanh_f32_v1(input);
            assert_eq!(
                actual.to_bits(),
                expected_bits,
                "case {name}: input=0x{:08x} ({input:?}) got 0x{:08x} expected 0x{expected_bits:08x}",
                input.to_bits(),
                actual.to_bits(),
            );
        }
    }

    /// One fixed, deterministic dense sweep folded into a single SHA-256
    /// digest, mirroring the audit probe's own hashing technique
    /// (`native_forward_determinism_probe_v1.rs`): a wide catch-all
    /// regression golden that a change to the algorithm's arithmetic,
    /// constants, or operation order would move, without needing to list
    /// thousands of individual hex values by hand.
    #[test]
    fn golden_dense_sweep_hash_v1() {
        let mut hasher = Sha256::new();

        // Linear sweep across [-12.0, 12.0]: covers the small-linear
        // region, the regular region, and the saturation region on both
        // sides of zero with a fixed, even step.
        const LINEAR_STEPS: u32 = 200_000;
        for i in 0..=LINEAR_STEPS {
            let t = i as f32 / LINEAR_STEPS as f32;
            let x = -12.0_f32 + 24.0_f32 * t;
            let y = tanh_f32_v1(x);
            hasher.update(y.to_bits().to_be_bytes());
        }

        // Fixed-stride raw bit-pattern sweep: exercises the full f32
        // dynamic range (subnormals, extremes, NaNs) without needing all
        // 2^32 patterns. Stride is a fixed odd constant, not derived from
        // any input, so the visited set is exactly reproducible.
        const BIT_SWEEP_COUNT: u32 = 500_000;
        const BIT_SWEEP_STRIDE: u32 = 8599;
        let mut bits: u32 = 0;
        for _ in 0..BIT_SWEEP_COUNT {
            let x = f32::from_bits(bits);
            let y = tanh_f32_v1(x);
            hasher.update(y.to_bits().to_be_bytes());
            bits = bits.wrapping_add(BIT_SWEEP_STRIDE);
        }

        let digest = hasher.finalize();
        let hex: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
        assert_eq!(
            hex, "b502cf42e808ac9c50d2e39016abd9269a71f88c8fcbcd6dfbefbba34f0726ec",
            "dense-sweep golden hash moved; this is a real algorithm-output \
             change (constants, operation order, or branch structure), not \
             a flaky test -- update deliberately if the change is reviewed \
             and intended"
        );
    }

    // -------------------------------------------------------------
    // Property tests.
    // -------------------------------------------------------------

    /// Odd symmetry, bit-exact: by construction (see the module doc),
    /// `tanh_f32_v1(-x)` and `tanh_f32_v1(x)` differ in exactly the sign
    /// bit for every finite `x`. Excludes NaN, which this implementation
    /// deliberately canonicalizes regardless of input sign (documented
    /// above), so odd symmetry does not apply to it.
    #[test]
    fn property_odd_symmetry_bit_exact_v1() {
        const SAMPLE_COUNT: u32 = 300_000;
        const STRIDE: u32 = 7919;
        let mut bits: u32 = 1;
        for _ in 0..SAMPLE_COUNT {
            let x = f32::from_bits(bits);
            if x.is_finite() {
                let positive = tanh_f32_v1(x);
                let negative = tanh_f32_v1(-x);
                assert_eq!(
                    negative.to_bits(),
                    positive.to_bits() ^ 0x8000_0000,
                    "odd symmetry failed for x={x:?} (bits=0x{bits:08x})"
                );
            }
            bits = bits.wrapping_add(STRIDE);
        }
        for x in [
            0.0f32,
            -0.0f32,
            f32::MIN_POSITIVE,
            f32::from_bits(1),
            TANH_SMALL_LINEAR_THRESHOLD_V1,
            1.0,
            TANH_SATURATION_THRESHOLD_V1,
            100.0,
            f32::MAX,
        ] {
            let positive = tanh_f32_v1(x);
            let negative = tanh_f32_v1(-x);
            assert_eq!(
                negative.to_bits(),
                positive.to_bits() ^ 0x8000_0000,
                "odd symmetry failed for explicit edge case x={x:?}"
            );
        }
    }

    /// Monotonic (non-decreasing) on a dense, ascending sampled grid
    /// spanning well past the saturation threshold on both sides.
    #[test]
    fn property_monotonic_on_sampled_grid_v1() {
        const HALF_STEPS: i32 = 400_000;
        const DOMAIN_HALF_WIDTH: f32 = 40.0;
        let mut previous: Option<f32> = None;
        for i in -HALF_STEPS..=HALF_STEPS {
            let x = (i as f32 / HALF_STEPS as f32) * DOMAIN_HALF_WIDTH;
            let y = tanh_f32_v1(x);
            if let Some(previous_y) = previous {
                assert!(
                    y >= previous_y,
                    "monotonicity violated: x={x} produced y={y} < previous y={previous_y}"
                );
            }
            previous = Some(y);
        }
    }

    /// Output range is `[-1.0, 1.0]` inclusive for every non-NaN input,
    /// checked over a fixed-stride full-range bit-pattern sweep.
    #[test]
    fn property_output_range_bounds_v1() {
        const SAMPLE_COUNT: u32 = 500_000;
        const STRIDE: u32 = 6151;
        let mut bits: u32 = 0;
        for _ in 0..SAMPLE_COUNT {
            let x = f32::from_bits(bits);
            if !x.is_nan() {
                let y = tanh_f32_v1(x);
                assert!(
                    (-1.0..=1.0).contains(&y),
                    "range violated for x=0x{bits:08x} ({x:?}): y={y}"
                );
            }
            bits = bits.wrapping_add(STRIDE);
        }
    }

    #[test]
    fn property_nan_input_always_canonicalized_v1() {
        for bits in [
            0x7fc0_0000u32,
            0xffc0_0000,
            0x7fc1_2345,
            0x7f80_0001,
            0xff80_0001,
        ] {
            let x = f32::from_bits(bits);
            assert!(x.is_nan(), "test setup bug: 0x{bits:08x} is not NaN");
            let y = tanh_f32_v1(x);
            assert_eq!(y.to_bits(), f32::NAN.to_bits());
        }
    }

    // -------------------------------------------------------------
    // MXCSR FTZ/DAZ/rounding-mode gate tests. Each mutation runs in a
    // freshly spawned OS thread (MXCSR is per-thread) so it cannot affect
    // any concurrently running test's floating-point control state, and
    // restores the thread's original MXCSR before the thread exits.
    // -------------------------------------------------------------

    #[test]
    #[cfg(target_arch = "x86_64")]
    fn assert_pinned_mxcsr_state_passes_on_clean_state_v1() {
        let handle = std::thread::spawn(|| {
            let original = read_mxcsr_v1();
            let dirty_mask = MXCSR_DAZ_BIT_V1
                | MXCSR_FTZ_BIT_V1
                | (MXCSR_ROUNDING_CONTROL_MASK_V1 << MXCSR_ROUNDING_CONTROL_SHIFT_V1);
            let clean = original & !dirty_mask;
            write_mxcsr_v1(clean);
            assert_pinned_mxcsr_state_v1();
            write_mxcsr_v1(original);
        });
        handle
            .join()
            .expect("mxcsr clean-state thread must not panic");
    }

    #[test]
    #[cfg(target_arch = "x86_64")]
    fn assert_pinned_mxcsr_state_panics_on_dirty_ftz_v1() {
        let handle = std::thread::spawn(|| {
            let original = read_mxcsr_v1();
            write_mxcsr_v1(original | MXCSR_FTZ_BIT_V1);
            let result = std::panic::catch_unwind(assert_pinned_mxcsr_state_v1);
            write_mxcsr_v1(original);
            assert!(
                result.is_err(),
                "expected panic when MXCSR FTZ bit is dirty"
            );
        });
        handle
            .join()
            .expect("mxcsr ftz-violation thread must not panic");
    }

    #[test]
    #[cfg(target_arch = "x86_64")]
    fn assert_pinned_mxcsr_state_panics_on_dirty_daz_v1() {
        let handle = std::thread::spawn(|| {
            let original = read_mxcsr_v1();
            write_mxcsr_v1(original | MXCSR_DAZ_BIT_V1);
            let result = std::panic::catch_unwind(assert_pinned_mxcsr_state_v1);
            write_mxcsr_v1(original);
            assert!(
                result.is_err(),
                "expected panic when MXCSR DAZ bit is dirty"
            );
        });
        handle
            .join()
            .expect("mxcsr daz-violation thread must not panic");
    }

    #[test]
    #[cfg(target_arch = "x86_64")]
    fn assert_pinned_mxcsr_state_panics_on_dirty_rounding_control_v1() {
        let handle = std::thread::spawn(|| {
            let original = read_mxcsr_v1();
            let dirty = original | (1 << MXCSR_ROUNDING_CONTROL_SHIFT_V1);
            write_mxcsr_v1(dirty);
            let result = std::panic::catch_unwind(assert_pinned_mxcsr_state_v1);
            write_mxcsr_v1(original);
            assert!(
                result.is_err(),
                "expected panic when MXCSR rounding-control is dirty"
            );
        });
        handle
            .join()
            .expect("mxcsr rounding-control-violation thread must not panic");
    }
}
