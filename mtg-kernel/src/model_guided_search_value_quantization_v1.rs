//! Model-guided searcher: value-quantization contract.
//!
//! Authority: `CLAUDE-MODEL-GUIDED-SEARCHER-DESIGN-V1.md` Section 1.3 ("New:
//! the net's value at leaf cutoffs, replacing v1's static evaluator"),
//! implementation item 2 of Section 5.3. This module implements exactly the
//! seven-step value-quantization pipeline of that section, steps 1-7,
//! including step 1's TBD-AT-IMPLEMENTATION domain-resolution procedure
//! (resolved below, with rationale). It does not implement the leaf-site
//! dispatch (natural terminal vs. newly-expanded node vs. revisited
//! depth-cap leaf), the perspective determination itself (which acting
//! player is at the leaf, versus who the root player is), or any caching:
//! all of that is search-loop wiring (design item 5), out of scope here.
//! Terminal nodes (site one of Section 1.3's three-site enumeration) never
//! enter this pipeline; the design states they "use v1's exact literal
//! terminal constants directly" with no scaling, rounding, or clamping.
//! This module has no terminal-value function at all, matching that
//! non-claim structurally rather than only in prose.
//!
//! # TBD-AT-IMPLEMENTATION resolution: the input domain (step 1)
//!
//! The design fixes the *procedure*, not the answer: "read the value head's
//! final activation function from the checkpoint architecture's own
//! source." That source is `native_policy_value_net_v1.rs`'s `forward_v1`
//! (the population-v2 lineage's Net8 family, the architecture Section 1.3
//! itself names as the shared-trunk case). The relevant lines:
//!
//! ```text
//! let mut value_hidden = linear_rows_v1(&self.value_first, &state_hidden, 1);
//! tanh_in_place_v1(&mut value_hidden);
//! let value = linear_rows_v1(&self.value_second, &value_hidden, 1)[0];
//! ```
//!
//! `tanh` is applied only to the *hidden* layer (`value_hidden`), between
//! `value_first` and `value_second`. The head's own final layer
//! (`value_second`) is a plain linear projection with **no** trailing
//! activation applied to its output (`value`). Per the design's own
//! procedure, this is unambiguously the "unbounded (a raw linear head with
//! no final squashing activation)" case, which the design routes to the
//! empirical, pre-registered calibration path, not the analytic-bound path.
//!
//! **Consequence for this module.** The empirical calibration itself
//! ("evaluate the value head over a large, fixed sample of reachable
//! nonterminal states drawn under the specific checkpoint being
//! registered... freeze the two resulting constants into that checkpoint's
//! registration record") is explicitly *not* buildable now: it requires a
//! live checkpoint and real game states, is checkpoint-specific by
//! construction, and is its own pre-registered procedure the design places
//! outside this module's scope (Section 4.2 lists this module as buildable
//! now precisely because it does *not* depend on a checkpoint). This module
//! therefore does not compute, hardcode, or guess any calibrated bound. It
//! instead exposes [`ModelGuidedSearchValueHeadDomainV1`] as a small, closed
//! contract type with three variants matching the design's own trichotomy
//! (`Tanh`, `SigmoidFamily`, and `Calibrated { lower, upper }`), so that a
//! future registration step supplies whichever domain applies to a given
//! checkpoint (for Net8 as it exists today: `Calibrated`, with `lower`/
//! `upper` frozen by that future calibration run) and this module's pure
//! pipeline functions work identically regardless of which variant is
//! supplied. The `Tanh`/`SigmoidFamily` variants are retained even though
//! *today's* concrete checkpoint architecture does not use them, because the
//! design's own procedure is written generically ("read... from the
//! checkpoint architecture's own source," not "from Net8's source"
//! specifically) and a future architecture could plausibly use either.
//!
//! # Determination: pipeline arithmetic width is `f32`
//!
//! The design's steps 2-5 do not name a float width. `v_raw` is the
//! checkpoint's own value-head scalar, which is `f32`
//! (`native_policy_value_net_v1::NativePolicyValueOutputV1::value: f32`).
//! With no explicit instruction to widen, this module carries `f32`
//! throughout (`v_unit`, `v_signed`, `product` are all `f32`), matching the
//! net's own native output type and introducing no extra width the design
//! does not call for.
//!
//! # Determination: round-half-to-even without floating-point arithmetic
//! (revised; supersedes an earlier, verified-false claim)
//!
//! Step 5 requires round-half-to-even "performed with the floating-point
//! unit's rounding-mode register verified and pinned to its IEEE 754
//! default... not left to whatever mode an unrelated prior computation on
//! the same thread may have set."
//!
//! An earlier revision of this module claimed `f32::round_ties_even`
//! satisfied this by itself, because it lowers to an
//! explicit-immediate-encoded instruction (`ROUNDSS`) independent of the
//! dynamic rounding-control register. **That claim was checked against this
//! crate's actual build target and found false, and is retracted here
//! rather than left standing.** `ROUNDSS`/`ROUNDPS` require SSE4.1. This
//! crate has no `.cargo/config.toml` and no `RUSTFLAGS` setting
//! `target-feature`, so it builds at the plain `x86_64-pc-windows-msvc`
//! baseline, whose default feature set is exactly `sse,sse2,sse3,
//! cmpxchg16b,fxsr` -- **no SSE4.1**. Verified directly: compiling a
//! `#[no_mangle] fn(x: f32) -> f32 { x.round_ties_even() }` probe with
//! `rustc -C opt-level=3 --emit asm` under this exact default-target
//! configuration (rustc 1.94.1, LLVM 21.1.8) produces
//!
//! ```text
//! probe_round_ties_even_f32:
//!     jmp     rintf
//! ```
//!
//! a tail call to the C library's `rintf`, not an inlined `ROUNDSS`.
//! `rintf`/`rint` are specified (C99, and the LLVM `roundeven` lowering
//! that falls back to them) to round according to the **current dynamic
//! rounding direction**, not unconditionally to ties-to-even; LLVM's
//! fallback lowering is correct *only* because the dynamic mode is assumed
//! to already be at its IEEE-754 default (round-to-nearest-ties-to-even)
//! wherever this runs. In other words: on this actual target,
//! `f32::round_ties_even` is exactly as exposed to the dynamic
//! rounding-mode-register hazard as any other floating-point operation in
//! this pipeline (steps 2 and 4's remap and scale multiply included), not
//! immune to it. The prior claim conflated "what `round_ties_even` is
//! specified to do" with "what it lowers to on every target," which do not
//! coincide here.
//!
//! **The fix, definitive rather than assumed:** [`round_ties_even_f32_bits_v1`]
//! implements round-half-to-even directly on `x.to_bits()` -- sign,
//! exponent, and mantissa extraction, integer shifts and masks, and one
//! integer comparison for the tie decision -- with **no floating-point
//! arithmetic anywhere in its body**. This is the correct, and now genuine,
//! basis for an immunity claim: there is no floating-point operation of any
//! kind for the dynamic rounding-mode register or FTZ/DAZ to influence, so
//! the result is bit-identical under every rounding-mode and FTZ/DAZ state,
//! by construction, independent of target, instruction-set level, or
//! optimizer choices, full stop. This is exhaustively verified against
//! `f32::round_ties_even` itself (under this thread's default, unmodified
//! rounding-mode state, where the two must and do agree exactly) by a
//! large deterministic sweep plus explicit bit-level tie-pattern goldens
//! (module tests), and is the sole rounding primitive both this module and
//! `model_guided_search_prior_quantization_v1` now use; `f32::round_ties_even`
//! itself survives in this module only as that verification's comparison
//! oracle, never in a production code path.
//!
//! This module still does **not** perform FPU dynamic-rounding-mode or
//! FTZ/DAZ register introspection for the *other* floating-point
//! operations in this pipeline: step 2's affine remap and step 4's scale
//! multiply remain ordinary IEEE-754 operations (a genuine reduction is
//! nowhere in either, so there is no operation-*order* ambiguity, but both
//! still consult the dynamic rounding-control register like any other basic
//! float op). That verification belongs to design item 3 (the
//! deterministic-CPU-forward audit "for the target checkpoint
//! architecture"), which covers the whole thread this pipeline eventually
//! runs on; it is out of scope for a pure, standalone contract module. The
//! split is therefore: steps 2 and 4 depend on the dynamic mode and are
//! covered by item 3's future audit; step 5 (this determination) depends on
//! nothing dynamic at all, because it now contains no floating-point
//! arithmetic to be dynamic about.
//!
//! # Determination: step 2's affine remap is not a second clamp
//!
//! For the `Calibrated` domain, step 2 affinely remaps `v_raw` to
//! `[-1.0, 1.0]` using the checkpoint's frozen `(lower, upper)` bounds,
//! *without* first clamping `v_raw` to `[lower, upper]`. This is
//! deliberate: those bounds come from a "wide, symmetric, percentile-based
//! bound (for example the 0.1st to 99.9th percentile)," which by
//! construction admits some raw values outside it; the design's own safety
//! clamp is step 6, applied to the *final scaled integer* range
//! `[-9,000, 9,000]`, not to `v_unit`. [`canonicalize_to_unit_domain_v1`]
//! therefore extrapolates the affine map linearly for out-of-bound
//! `Calibrated` inputs rather than pre-clamping, and lets
//! [`round_and_clamp_value_v1`] absorb the overflow exactly as the design's
//! step ordering specifies. For the `Tanh`/`SigmoidFamily` analytic
//! domains, by contrast, out-of-range `v_raw` is rejected outright
//! (`RawValueOutOfAnalyticDomain`): those bounds are exact mathematical
//! guarantees of the stated activation function, so a value outside them
//! indicates a genuine defect (a wrong domain tag, or the activation
//! function was not what was registered), not a legitimate tail value.

use core::fmt;

/// Scaling factor for step 4 (`product = v_signed * 9000.0`), and the
/// magnitude of v1's own exact nonterminal value range.
pub const MODEL_GUIDED_SEARCH_VALUE_QUANTIZATION_SCALE_V1: f32 = 9_000.0;

/// Step 6's saturating clamp lower bound: v1's exact nonterminal range.
pub const MODEL_GUIDED_SEARCH_VALUE_QUANTIZATION_CLAMP_MIN_V1: i32 = -9_000;

/// Step 6's saturating clamp upper bound: v1's exact nonterminal range.
pub const MODEL_GUIDED_SEARCH_VALUE_QUANTIZATION_CLAMP_MAX_V1: i32 = 9_000;

/// Step 1's resolved input domain, one instance of which is fixed per
/// checkpoint (never per search; design: "the resolved domain is part of
/// the value-quantization contract digest... and is fixed per checkpoint,
/// not per search"). See module docs for the TBD-AT-IMPLEMENTATION
/// resolution and why all three variants are retained.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ModelGuidedSearchValueHeadDomainV1 {
    /// Final activation is `tanh`; analytic domain `[-1.0, 1.0]`, no
    /// empirical step, no remap in step 2.
    Tanh,
    /// Final activation is a sigmoid-family function; analytic domain
    /// `[0.0, 1.0]`.
    SigmoidFamily,
    /// Unbounded (raw linear) head; domain fixed by an external,
    /// checkpoint-specific, pre-registered empirical calibration this
    /// module does not compute. This is the case that applies to the
    /// population-v2 Net8 architecture as of this writing (see module
    /// docs).
    Calibrated { lower: f32, upper: f32 },
}

impl ModelGuidedSearchValueHeadDomainV1 {
    /// Validates the domain descriptor itself (not any particular raw
    /// value). `Tanh`/`SigmoidFamily` carry no free parameters and always
    /// validate; `Calibrated` requires both bounds finite and `lower <
    /// upper` (a zero- or negative-width domain cannot be affinely remapped
    /// to `[-1.0, 1.0]`).
    pub fn validate(&self) -> Result<(), ModelGuidedSearchValueQuantizationErrorV1> {
        match self {
            Self::Tanh | Self::SigmoidFamily => Ok(()),
            Self::Calibrated { lower, upper } => {
                if !lower.is_finite() || !upper.is_finite() || lower >= upper {
                    return Err(
                        ModelGuidedSearchValueQuantizationErrorV1::InvalidCalibratedDomain {
                            lower_bits: lower.to_bits(),
                            upper_bits: upper.to_bits(),
                        },
                    );
                }
                Ok(())
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelGuidedSearchValueQuantizationErrorV1 {
    /// `v_raw` was not finite (NaN or infinite). Rejected regardless of
    /// domain: a non-finite value-head output is a genuine defect, not a
    /// legitimate tail value under any domain.
    NonFiniteRawValue { bits: u32 },
    /// `v_raw` fell outside the exact analytic domain (`Tanh` or
    /// `SigmoidFamily`); see module docs, "step 2's affine remap is not a
    /// second clamp," for why this is a hard error only for the analytic
    /// domains.
    RawValueOutOfAnalyticDomain {
        bits: u32,
        lower_bits: u32,
        upper_bits: u32,
    },
    /// The `Calibrated` domain's own bounds are malformed (non-finite, or
    /// `lower >= upper`).
    InvalidCalibratedDomain { lower_bits: u32, upper_bits: u32 },
    /// `product` (post-scaling, step 4) was not finite. Reachable only from
    /// a pathologically extreme `Calibrated` domain combined with an
    /// extreme out-of-bound `v_raw`; checked defensively before rounding.
    NonFiniteProduct { bits: u32 },
}

impl fmt::Display for ModelGuidedSearchValueQuantizationErrorV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for ModelGuidedSearchValueQuantizationErrorV1 {}

/// Steps 1-2: resolves `v_raw`'s domain and, if it is not already
/// `[-1.0, 1.0]`, affinely remaps it there. Returns `v_unit`.
pub fn canonicalize_to_unit_domain_v1(
    domain: &ModelGuidedSearchValueHeadDomainV1,
    v_raw: f32,
) -> Result<f32, ModelGuidedSearchValueQuantizationErrorV1> {
    domain.validate()?;
    if !v_raw.is_finite() {
        return Err(
            ModelGuidedSearchValueQuantizationErrorV1::NonFiniteRawValue {
                bits: v_raw.to_bits(),
            },
        );
    }
    match domain {
        ModelGuidedSearchValueHeadDomainV1::Tanh => {
            if !(-1.0..=1.0).contains(&v_raw) {
                return Err(
                    ModelGuidedSearchValueQuantizationErrorV1::RawValueOutOfAnalyticDomain {
                        bits: v_raw.to_bits(),
                        lower_bits: (-1.0_f32).to_bits(),
                        upper_bits: (1.0_f32).to_bits(),
                    },
                );
            }
            // Already [-1.0, 1.0]; step 2's remap is a no-op.
            Ok(v_raw)
        }
        ModelGuidedSearchValueHeadDomainV1::SigmoidFamily => {
            if !(0.0..=1.0).contains(&v_raw) {
                return Err(
                    ModelGuidedSearchValueQuantizationErrorV1::RawValueOutOfAnalyticDomain {
                        bits: v_raw.to_bits(),
                        lower_bits: (0.0_f32).to_bits(),
                        upper_bits: (1.0_f32).to_bits(),
                    },
                );
            }
            Ok(2.0 * v_raw - 1.0)
        }
        ModelGuidedSearchValueHeadDomainV1::Calibrated { lower, upper } => {
            Ok(2.0 * (v_raw - lower) / (upper - lower) - 1.0)
        }
    }
}

/// Step 3: negates `v_unit` when the leaf's acting player is not the root
/// player, so the result stays root-player-relative (v1's storage
/// convention). Returns `v_signed`.
pub fn apply_root_perspective_v1(v_unit: f32, leaf_acting_player_is_root: bool) -> f32 {
    if leaf_acting_player_is_root {
        v_unit
    } else {
        -v_unit
    }
}

/// Step 4: `product = v_signed * 9000.0`, one IEEE-754 multiply of two
/// already-fixed operands.
pub fn scale_signed_value_v1(v_signed: f32) -> f32 {
    v_signed * MODEL_GUIDED_SEARCH_VALUE_QUANTIZATION_SCALE_V1
}

/// Bit-exact round-half-to-even ("round to nearest, ties to even") from a
/// finite `f32` to its mathematically exact rounded integer value, returned
/// as `i64` (wide enough to hold this module's own `i32` range and, exactly,
/// `model_guided_search_prior_quantization_v1`'s `u64` fixed-point weight
/// scale up to `2**32`; each caller narrows/casts to its own domain). See
/// module docs, "Determination: round-half-to-even without floating-point
/// arithmetic," for the verified target-specific finding that motivated
/// this function and for the immunity argument it establishes.
///
/// Operates entirely on `x.to_bits()` -- sign, exponent, and mantissa
/// extraction, integer shifts, integer masks, and one integer comparison
/// for the tie decision -- with **no floating-point arithmetic anywhere in
/// its body**. That is what makes it immune to the dynamic FPU
/// rounding-mode register (`MXCSR.RC` / `FPCR.RMode`) and to FTZ/DAZ: there
/// is no floating-point operation for either to influence, full stop, not
/// an argument about which machine instruction some floating-point
/// operation happens to lower to on some particular target.
///
/// **Precondition:** `x` is finite. NaN and infinity have no "nearest
/// integer" and are not given defined behavior here; every call site in
/// this crate checks finiteness before calling this function.
///
/// **Algorithm.** Decompose `x`'s bits into `sign`, the 8-bit biased
/// `raw_exponent`, and the 23-bit `raw_mantissa`. If `raw_exponent == 0`
/// (zero or subnormal), `|x| < 2**-126`, unconditionally below the 0.5
/// rounding threshold: return 0. Otherwise let `unbiased_exponent =
/// raw_exponent - 127` and `significand = (1 << 23) | raw_mantissa` (the
/// exact 24-bit integer the implicit-leading-one encoding represents).
/// Three cases, exhaustive over every remaining `unbiased_exponent`:
///
/// - `unbiased_exponent < -1`: `|x| < 0.5` always (even the supremum, just
///   under `2 * 2**-2`, is strictly less), so the result is 0.
/// - `unbiased_exponent == -1`: `0.5 <= |x| < 1.0` (`|x| == significand /
///   2**24` here). Exactly `0.5` iff `raw_mantissa == 0`, which is the tie
///   and rounds to the even integer 0; any nonzero mantissa means `|x| >
///   0.5`, which always rounds away from zero to magnitude 1 (never a tie
///   in this case).
/// - `unbiased_exponent >= 23`: no fractional bits remain; `|x|` is already
///   an exact integer equal to `significand << (unbiased_exponent - 23)`.
/// - Otherwise (`0 <= unbiased_exponent <= 22`): split `significand` at
///   `frac_bits = 23 - unbiased_exponent` into an integer part (the high
///   bits) and a fractional part (the low `frac_bits` bits, in units of
///   `2**-frac_bits`). Compare the fractional part against `half = 1 <<
///   (frac_bits - 1)`: below rounds down, above rounds up, and an exact
///   match is the tie, resolved by the integer part's own parity (round to
///   even) -- textbook round-to-nearest-even, performed with only integer
///   shifts, masks, and one comparison.
///
/// Magnitudes whose exact value this function does not need to compute
/// (because no caller in this crate narrows to anything wider than `u64`'s
/// legitimate `2**32` maximum) short-circuit to `i64::MIN`/`i64::MAX`
/// (matching Rust's own saturating `f32 as i64` cast exactly, which is what
/// this function's own exhaustive sweep test uses as its oracle) rather
/// than risking an oversized shift; the threshold is far above every
/// legitimate caller's range, so this never affects a real result (see the
/// module's own tests for the exact threshold and why it cannot be reached
/// by any validated caller).
pub(crate) fn round_ties_even_f32_bits_v1(x: f32) -> i64 {
    let bits = x.to_bits();
    let sign_negative = (bits >> 31) & 1 == 1;
    let raw_exponent = (bits >> 23) & 0xFF;
    let raw_mantissa = bits & 0x007F_FFFF;

    if raw_exponent == 0 {
        return 0;
    }

    let unbiased_exponent = raw_exponent as i32 - 127;

    if unbiased_exponent < -1 {
        return 0;
    }

    let significand: u64 = (1u64 << 23) | u64::from(raw_mantissa);

    // Boundary derivation (exact, not a margin-of-safety guess): significand
    // ranges over [2**23, 2**24 - 1] (the implicit leading bit is always
    // set). At shift == 39, the largest possible magnitude is
    // (2**24 - 1) << 39 == 2**63 - 2**39, strictly less than
    // i64::MAX == 2**63 - 1, so every significand still fits exactly. At
    // shift == 40, even the *smallest* possible magnitude,
    // 2**23 << 40 == 2**63, already exceeds i64::MAX by construction
    // (2**63 > 2**63 - 1) -- so shift >= 40 can never be represented
    // exactly in i64, for any significand, and must short-circuit to the
    // same saturated endpoints Rust's own `as i64` float-to-int cast
    // produces (asymmetric, since i64::MIN's magnitude, 2**63, has no
    // positive i64 counterpart), matching the sweep test's oracle
    // (`x.round_ties_even() as i64`) exactly rather than merely "some large
    // value" of the wrong sign-symmetric shape. This is also far beyond
    // both of this crate's legitimate callers' maximum needed shift (9, for
    // the prior-quantization module's largest legal weight, 1.0, scaled by
    // 2**32), so the exact value is irrelevant to any real caller either
    // way.
    if unbiased_exponent - 23 > 39 {
        return if sign_negative { i64::MIN } else { i64::MAX };
    }

    let magnitude: i64 = if unbiased_exponent == -1 {
        i64::from(raw_mantissa != 0)
    } else if unbiased_exponent >= 23 {
        let shift = unbiased_exponent - 23;
        // shift <= 39 here (the shift >= 40 case already returned above),
        // which the boundary derivation above proves always fits in i64
        // without overflow, for every possible significand.
        (significand as i64) << shift
    } else {
        let frac_bits = 23 - unbiased_exponent;
        let integer_part = significand >> frac_bits;
        let frac_mask = (1u64 << frac_bits) - 1;
        let frac = significand & frac_mask;
        let half = 1u64 << (frac_bits - 1);
        let rounded = match frac.cmp(&half) {
            core::cmp::Ordering::Less => integer_part,
            core::cmp::Ordering::Greater => integer_part + 1,
            core::cmp::Ordering::Equal => {
                if integer_part & 1 == 0 {
                    integer_part
                } else {
                    integer_part + 1
                }
            }
        };
        rounded as i64
    };

    if sign_negative {
        -magnitude
    } else {
        magnitude
    }
}

/// Steps 5-7: rounds `product` to the nearest integer with ties-to-even
/// (via [`round_ties_even_f32_bits_v1`]; see module docs for why that
/// function, not `f32::round_ties_even`, is what satisfies step 5's
/// rounding-mode requirement on this crate's actual build target), then
/// saturating-clamps to `[-9,000, 9,000]` (step 6), returning the result in
/// v1's own signed integer type (step 7: "this design introduces no new
/// integer width or representation").
pub fn round_and_clamp_value_v1(
    product: f32,
) -> Result<i32, ModelGuidedSearchValueQuantizationErrorV1> {
    if !product.is_finite() {
        return Err(
            ModelGuidedSearchValueQuantizationErrorV1::NonFiniteProduct {
                bits: product.to_bits(),
            },
        );
    }
    let rounded = round_ties_even_f32_bits_v1(product);
    // Saturating narrow to i32 (pure integer clamp, no float cast
    // involved): out-of-range magnitudes clamp to i32::MIN/MAX first, then
    // to the design's own [-9_000, 9_000] range below.
    let saturated = rounded.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
    Ok(saturated.clamp(
        MODEL_GUIDED_SEARCH_VALUE_QUANTIZATION_CLAMP_MIN_V1,
        MODEL_GUIDED_SEARCH_VALUE_QUANTIZATION_CLAMP_MAX_V1,
    ))
}

/// The full Section 1.3 pipeline (steps 1-7) for one nonterminal
/// leaf-evaluation event (sites 2 or 3; never site 1, natural terminals,
/// which never enter this pipeline). `leaf_acting_player_is_root` is
/// `leaf_actor == root_player`, computed by the caller from live session
/// state this module does not have access to.
pub fn quantize_value_v1(
    domain: &ModelGuidedSearchValueHeadDomainV1,
    v_raw: f32,
    leaf_acting_player_is_root: bool,
) -> Result<i32, ModelGuidedSearchValueQuantizationErrorV1> {
    let v_unit = canonicalize_to_unit_domain_v1(domain, v_raw)?;
    let v_signed = apply_root_perspective_v1(v_unit, leaf_acting_player_is_root);
    let product = scale_signed_value_v1(v_signed);
    round_and_clamp_value_v1(product)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::SplitMix64;

    #[test]
    fn constants_are_pinned() {
        assert_eq!(MODEL_GUIDED_SEARCH_VALUE_QUANTIZATION_SCALE_V1, 9_000.0);
        assert_eq!(MODEL_GUIDED_SEARCH_VALUE_QUANTIZATION_CLAMP_MIN_V1, -9_000);
        assert_eq!(MODEL_GUIDED_SEARCH_VALUE_QUANTIZATION_CLAMP_MAX_V1, 9_000);
    }

    #[test]
    fn calibrated_domain_validation_rejects_non_finite_and_non_increasing_bounds() {
        assert!(ModelGuidedSearchValueHeadDomainV1::Tanh.validate().is_ok());
        assert!(ModelGuidedSearchValueHeadDomainV1::SigmoidFamily
            .validate()
            .is_ok());
        assert!(ModelGuidedSearchValueHeadDomainV1::Calibrated {
            lower: -3.0,
            upper: 3.0
        }
        .validate()
        .is_ok());
        assert_eq!(
            ModelGuidedSearchValueHeadDomainV1::Calibrated {
                lower: 3.0,
                upper: 3.0
            }
            .validate(),
            Err(
                ModelGuidedSearchValueQuantizationErrorV1::InvalidCalibratedDomain {
                    lower_bits: 3.0_f32.to_bits(),
                    upper_bits: 3.0_f32.to_bits(),
                }
            )
        );
        assert_eq!(
            ModelGuidedSearchValueHeadDomainV1::Calibrated {
                lower: 3.0,
                upper: -3.0
            }
            .validate(),
            Err(
                ModelGuidedSearchValueQuantizationErrorV1::InvalidCalibratedDomain {
                    lower_bits: 3.0_f32.to_bits(),
                    upper_bits: (-3.0_f32).to_bits(),
                }
            )
        );
        assert_eq!(
            ModelGuidedSearchValueHeadDomainV1::Calibrated {
                lower: f32::NAN,
                upper: 3.0
            }
            .validate(),
            Err(
                ModelGuidedSearchValueQuantizationErrorV1::InvalidCalibratedDomain {
                    lower_bits: f32::NAN.to_bits(),
                    upper_bits: 3.0_f32.to_bits(),
                }
            )
        );
    }

    #[test]
    fn tanh_domain_is_identity_within_range_and_rejects_outside() {
        assert_eq!(
            canonicalize_to_unit_domain_v1(&ModelGuidedSearchValueHeadDomainV1::Tanh, 0.0).unwrap(),
            0.0
        );
        assert_eq!(
            canonicalize_to_unit_domain_v1(&ModelGuidedSearchValueHeadDomainV1::Tanh, -1.0)
                .unwrap(),
            -1.0
        );
        assert_eq!(
            canonicalize_to_unit_domain_v1(&ModelGuidedSearchValueHeadDomainV1::Tanh, 1.0).unwrap(),
            1.0
        );
        assert_eq!(
            canonicalize_to_unit_domain_v1(&ModelGuidedSearchValueHeadDomainV1::Tanh, 1.5),
            Err(
                ModelGuidedSearchValueQuantizationErrorV1::RawValueOutOfAnalyticDomain {
                    bits: 1.5_f32.to_bits(),
                    lower_bits: (-1.0_f32).to_bits(),
                    upper_bits: 1.0_f32.to_bits(),
                }
            )
        );
        assert_eq!(
            canonicalize_to_unit_domain_v1(&ModelGuidedSearchValueHeadDomainV1::Tanh, f32::NAN),
            Err(
                ModelGuidedSearchValueQuantizationErrorV1::NonFiniteRawValue {
                    bits: f32::NAN.to_bits(),
                }
            )
        );
    }

    #[test]
    fn sigmoid_family_domain_remaps_zero_one_to_minus_one_one() {
        assert_eq!(
            canonicalize_to_unit_domain_v1(&ModelGuidedSearchValueHeadDomainV1::SigmoidFamily, 0.0)
                .unwrap(),
            -1.0
        );
        assert_eq!(
            canonicalize_to_unit_domain_v1(&ModelGuidedSearchValueHeadDomainV1::SigmoidFamily, 1.0)
                .unwrap(),
            1.0
        );
        assert_eq!(
            canonicalize_to_unit_domain_v1(&ModelGuidedSearchValueHeadDomainV1::SigmoidFamily, 0.5)
                .unwrap(),
            0.0
        );
        assert_eq!(
            canonicalize_to_unit_domain_v1(&ModelGuidedSearchValueHeadDomainV1::SigmoidFamily, 1.5),
            Err(
                ModelGuidedSearchValueQuantizationErrorV1::RawValueOutOfAnalyticDomain {
                    bits: 1.5_f32.to_bits(),
                    lower_bits: 0.0_f32.to_bits(),
                    upper_bits: 1.0_f32.to_bits(),
                }
            )
        );
    }

    #[test]
    fn calibrated_domain_remaps_bounds_to_minus_one_one_and_extrapolates_outside() {
        let domain = ModelGuidedSearchValueHeadDomainV1::Calibrated {
            lower: -4.0,
            upper: 4.0,
        };
        assert_eq!(canonicalize_to_unit_domain_v1(&domain, -4.0).unwrap(), -1.0);
        assert_eq!(canonicalize_to_unit_domain_v1(&domain, 4.0).unwrap(), 1.0);
        assert_eq!(canonicalize_to_unit_domain_v1(&domain, 0.0).unwrap(), 0.0);
        // Outside [lower, upper]: extrapolated, not rejected or clamped here
        // (see module docs, "step 2's affine remap is not a second clamp").
        // v_unit = 2*(v_raw-lower)/(upper-lower) - 1 = v_raw/4 for this
        // domain (center 0, half-width 4), so v_raw=8 -> v_unit=2.0.
        assert_eq!(canonicalize_to_unit_domain_v1(&domain, 8.0).unwrap(), 2.0);
        assert_eq!(canonicalize_to_unit_domain_v1(&domain, -8.0).unwrap(), -2.0);
    }

    #[test]
    fn perspective_flip_is_an_exact_involution_and_identity_on_root_perspective() {
        for raw in [0.0_f32, 1.0, -1.0, 0.37, -0.37, f32::MIN_POSITIVE] {
            assert_eq!(apply_root_perspective_v1(raw, true), raw);
            assert_eq!(apply_root_perspective_v1(raw, false), -raw);
            // Involution: flipping twice returns the exact original value.
            assert_eq!(
                apply_root_perspective_v1(apply_root_perspective_v1(raw, false), false),
                raw
            );
        }
    }

    #[test]
    fn scaling_matches_the_design_literal() {
        assert_eq!(scale_signed_value_v1(1.0), 9_000.0);
        assert_eq!(scale_signed_value_v1(-1.0), -9_000.0);
        assert_eq!(scale_signed_value_v1(0.0), 0.0);
    }

    #[test]
    fn rounding_is_ties_to_even_at_every_half_boundary() {
        // Ties-to-even, not away-from-zero: 0.5 -> 0, 1.5 -> 2, 2.5 -> 2,
        // -0.5 -> 0 (sign of zero not asserted), -1.5 -> -2, -2.5 -> -2.
        assert_eq!(round_and_clamp_value_v1(0.5).unwrap(), 0);
        assert_eq!(round_and_clamp_value_v1(1.5).unwrap(), 2);
        assert_eq!(round_and_clamp_value_v1(2.5).unwrap(), 2);
        assert_eq!(round_and_clamp_value_v1(-0.5).unwrap(), 0);
        assert_eq!(round_and_clamp_value_v1(-1.5).unwrap(), -2);
        assert_eq!(round_and_clamp_value_v1(-2.5).unwrap(), -2);
        // Non-half values round to nearest as usual.
        assert_eq!(round_and_clamp_value_v1(0.49).unwrap(), 0);
        assert_eq!(round_and_clamp_value_v1(0.51).unwrap(), 1);
    }

    #[test]
    fn rounding_is_idempotent_once_integer_valued() {
        // Idempotence of the actual production rounding primitive
        // (round_ties_even_f32_bits_v1), not std's round_ties_even: once a
        // value is already integer-valued, rounding it again (by feeding
        // the result back in as an f32, which is exact for every value
        // tested here) must return the identical integer.
        for value in [0.0_f32, 5.0, -5.0, 8_999.0, -8_999.0] {
            let once = round_ties_even_f32_bits_v1(value);
            let twice = round_ties_even_f32_bits_v1(once as f32);
            assert_eq!(once, twice);
        }
    }

    #[test]
    fn bit_exact_rounding_matches_std_round_ties_even_at_every_golden_half_boundary() {
        // The same values rounding_is_ties_to_even_at_every_half_boundary
        // checks through the full pipeline, checked directly against the
        // bit-exact primitive and cross-checked against
        // f32::round_ties_even (the oracle, valid here because this thread
        // runs under the default, unmodified rounding-mode state).
        let cases: [(f32, i64); 12] = [
            (0.5, 0),
            (1.5, 2),
            (2.5, 2),
            (3.5, 4),
            (4.5, 4),
            (-0.5, 0),
            (-1.5, -2),
            (-2.5, -2),
            (-3.5, -4),
            (-4.5, -4),
            (0.49, 0),
            (0.51, 1),
        ];
        for (input, expected) in cases {
            assert_eq!(
                round_ties_even_f32_bits_v1(input),
                expected,
                "input={input}"
            );
            assert_eq!(
                round_ties_even_f32_bits_v1(input),
                input.round_ties_even() as i64,
                "disagreement with std oracle at input={input}"
            );
        }
    }

    #[test]
    fn bit_exact_rounding_handles_zero_subnormal_and_sign() {
        assert_eq!(round_ties_even_f32_bits_v1(0.0), 0);
        assert_eq!(round_ties_even_f32_bits_v1(-0.0), 0);
        assert_eq!(round_ties_even_f32_bits_v1(f32::from_bits(1)), 0); // smallest subnormal
        assert_eq!(round_ties_even_f32_bits_v1(-f32::from_bits(1)), 0);
        assert_eq!(
            round_ties_even_f32_bits_v1(f32::from_bits(0x007F_FFFF)), // largest subnormal
            0
        );
    }

    #[test]
    fn bit_exact_rounding_is_exact_at_the_integer_exponent_boundary() {
        // unbiased_exponent == 23 is the frac_bits == 0 boundary: the value
        // is already an exact integer with no rounding decision to make.
        // 2**23 == 8_388_608.0 is exactly representable in f32.
        assert_eq!(round_ties_even_f32_bits_v1(8_388_608.0), 8_388_608);
        assert_eq!(round_ties_even_f32_bits_v1(-8_388_608.0), -8_388_608);
        // One step below the boundary (unbiased_exponent == 22) still has
        // one fractional bit; exercise a non-tie and a tie there.
        assert_eq!(round_ties_even_f32_bits_v1(4_194_304.0), 4_194_304);
    }

    #[test]
    fn bit_exact_rounding_is_exact_at_the_prior_modules_legitimate_maximum() {
        // model_guided_search_prior_quantization_v1's weight quantization
        // scales by 2**32 and its weight domain is validated to [0.0, 1.0],
        // so 2**32 exactly (weight == 1.0) is the largest input this
        // crate's other call site ever legitimately passes. Confirms the
        // shift > 39 sentinel threshold is nowhere near this range (shift
        // here is 32 - 23 == 9).
        assert_eq!(round_ties_even_f32_bits_v1(4_294_967_296.0), 4_294_967_296);
    }

    #[test]
    fn bit_exact_rounding_saturates_without_overflow_far_beyond_any_caller() {
        // Values far beyond both callers' legitimate ranges must not panic
        // or wrap, and must match Rust's own saturating f32-as-i64 cast
        // exactly (i64::MAX / i64::MIN, asymmetric: i64::MIN's magnitude
        // 2**63 has no positive i64 counterpart), the same oracle the
        // exhaustive sweep test below uses.
        assert_eq!(round_ties_even_f32_bits_v1(f32::MAX), i64::MAX);
        assert_eq!(round_ties_even_f32_bits_v1(f32::MIN), i64::MIN);
        assert_eq!(
            round_ties_even_f32_bits_v1(f32::MAX),
            f32::MAX.round_ties_even() as i64
        );
        assert_eq!(
            round_ties_even_f32_bits_v1(f32::MIN),
            f32::MIN.round_ties_even() as i64
        );
    }

    #[test]
    fn bit_exact_rounding_matches_std_oracle_across_a_dense_deterministic_sweep() {
        // Large deterministic sweep (SplitMix64-seeded, same generator
        // already used elsewhere in this crate) over finite f32 bit
        // patterns spanning the full exponent range (subnormal through
        // near-overflow, both signs), verified to agree exactly with
        // f32::round_ties_even under this thread's default rounding-mode
        // state -- the two must and do coincide there, which is exactly
        // what licenses using f32::round_ties_even as this sweep's oracle.
        let mut rng = SplitMix64::seed(0xC0FF_EE15_BEEF_1234);
        let mut checked = 0u32;
        while checked < 2_000_000 {
            let bits = rng.next_u64() as u32;
            let candidate = f32::from_bits(bits);
            if !candidate.is_finite() {
                continue;
            }
            let expected = candidate.round_ties_even() as i64;
            let actual = round_ties_even_f32_bits_v1(candidate);
            assert_eq!(
                actual, expected,
                "disagreement at bits=0x{bits:08x} value={candidate}"
            );
            checked += 1;
        }
    }

    #[test]
    fn bit_exact_rounding_matches_std_oracle_across_every_half_integer_in_a_wide_exhaustive_range()
    {
        // Exhaustive (not sampled) walk over every exact half-integer tie
        // point from -10_000.5 to 10_000.5, the densest possible
        // tie-pattern battery: every one of these is a genuine tie under
        // round-half-to-even, alternating which side is "even" as the
        // integer part increments.
        let mut n = -10_000i32;
        while n <= 10_000 {
            let value = n as f32 + 0.5;
            let expected = value.round_ties_even() as i64;
            let actual = round_ties_even_f32_bits_v1(value);
            assert_eq!(actual, expected, "tie at value={value}");
            n += 1;
        }
    }

    #[test]
    fn clamp_saturates_at_exactly_9000_and_is_idempotent() {
        assert_eq!(round_and_clamp_value_v1(9_000.0).unwrap(), 9_000);
        assert_eq!(round_and_clamp_value_v1(9_000.4).unwrap(), 9_000);
        assert_eq!(round_and_clamp_value_v1(9_001.0).unwrap(), 9_000);
        assert_eq!(round_and_clamp_value_v1(1_000_000.0).unwrap(), 9_000);
        assert_eq!(round_and_clamp_value_v1(f32::MAX).unwrap(), 9_000);
        assert_eq!(round_and_clamp_value_v1(-9_000.0).unwrap(), -9_000);
        assert_eq!(round_and_clamp_value_v1(-9_001.0).unwrap(), -9_000);
        assert_eq!(round_and_clamp_value_v1(-1_000_000.0).unwrap(), -9_000);
        assert_eq!(round_and_clamp_value_v1(f32::MIN).unwrap(), -9_000);
        // Idempotence: clamping an already-clamped boundary value again
        // gives the same value.
        let clamped = round_and_clamp_value_v1(f32::MAX).unwrap();
        assert_eq!(clamped.clamp(-9_000, 9_000), clamped);
    }

    #[test]
    fn round_and_clamp_rejects_non_finite_product() {
        assert_eq!(
            round_and_clamp_value_v1(f32::NAN),
            Err(
                ModelGuidedSearchValueQuantizationErrorV1::NonFiniteProduct {
                    bits: f32::NAN.to_bits(),
                }
            )
        );
        assert_eq!(
            round_and_clamp_value_v1(f32::INFINITY),
            Err(
                ModelGuidedSearchValueQuantizationErrorV1::NonFiniteProduct {
                    bits: f32::INFINITY.to_bits(),
                }
            )
        );
    }

    #[test]
    fn full_pipeline_matches_hand_worked_tanh_example() {
        // v_raw = 0.5 under Tanh (already [-1,1], no remap); root
        // perspective; product = 0.5 * 9000 = 4500; rounds exactly.
        assert_eq!(
            quantize_value_v1(&ModelGuidedSearchValueHeadDomainV1::Tanh, 0.5, true).unwrap(),
            4_500
        );
        // Same raw value, non-root perspective: negated before scaling.
        assert_eq!(
            quantize_value_v1(&ModelGuidedSearchValueHeadDomainV1::Tanh, 0.5, false).unwrap(),
            -4_500
        );
    }

    #[test]
    fn full_pipeline_matches_hand_worked_calibrated_example() {
        // Calibrated domain [0.0, 8.0]; v_raw = 6.0 -> v_unit = 2*(6/8)-1 =
        // 0.5; product = 0.5*9000 = 4500.
        let domain = ModelGuidedSearchValueHeadDomainV1::Calibrated {
            lower: 0.0,
            upper: 8.0,
        };
        assert_eq!(quantize_value_v1(&domain, 6.0, true).unwrap(), 4_500);
        assert_eq!(quantize_value_v1(&domain, 6.0, false).unwrap(), -4_500);
    }

    #[test]
    fn full_pipeline_propagates_invalid_domain_before_touching_v_raw() {
        let domain = ModelGuidedSearchValueHeadDomainV1::Calibrated {
            lower: 1.0,
            upper: 1.0,
        };
        assert_eq!(
            quantize_value_v1(&domain, 0.0, true),
            Err(
                ModelGuidedSearchValueQuantizationErrorV1::InvalidCalibratedDomain {
                    lower_bits: 1.0_f32.to_bits(),
                    upper_bits: 1.0_f32.to_bits(),
                }
            )
        );
    }

    #[test]
    fn negation_symmetry_holds_end_to_end_through_the_full_pipeline() {
        let domain = ModelGuidedSearchValueHeadDomainV1::Tanh;
        for raw in [0.0_f32, 0.1, -0.1, 0.999, -0.999, 1.0, -1.0] {
            let root = quantize_value_v1(&domain, raw, true).unwrap();
            let opponent = quantize_value_v1(&domain, raw, false).unwrap();
            assert_eq!(root, -opponent, "raw={raw}");
        }
    }
}
