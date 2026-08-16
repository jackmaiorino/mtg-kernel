//! Model-guided searcher: PUCT prior-quantization contract.
//!
//! Authority: `CLAUDE-MODEL-GUIDED-SEARCHER-DESIGN-V1.md` Section 1.2 ("New:
//! the net's policy as a PUCT-style expansion prior"), implementation item 1
//! of Section 5.3. That document's own COUNTERSIGN states plainly: "No claim
//! that this design's PUCT formula (Section 1.2)... is calibrated, tuned, or
//! strength-validated... the stage-1 review checks implementation
//! correctness only." This module makes the identical non-claim: it is an
//! arithmetic contract, not a strength claim.
//!
//! # Scope
//!
//! This module is disjoint and self-contained, exactly like
//! `kernel_native_search_opponent_v1`'s own stage-1 core. It does not touch
//! the search tree, the checkpoint forward pass, node caching, or any
//! existing module. It supplies three pure pieces of Section 1.2's contract:
//!
//! 1. **Apportionment**: `P_int(a)`, the integer PUCT prior for each legal
//!    action, via deterministic largest-remainder (Hamilton) apportionment
//!    to a fixed scale of 1,000,000.
//! 2. **Expansion order**: the descending-`P_int`, ascending-index order
//!    unvisited actions expand in (Section 1.2, "Expansion order").
//! 3. **Selection bonus**: `bonus_PUCT(a) = floor(bonus(a) * P_int(a) /
//!    1,000,000)` (Section 1.2, "Selection bonus"). `bonus(a)` itself is
//!    v1's own, unchanged, integer UCB bonus
//!    (`kernel_native_search_opponent_v1::integer_ucb_bonus_v1`); this
//!    module does not recompute it, only the new multiply/divide/floor the
//!    design adds on top of it.
//!
//! This module does **not** evaluate the checkpoint's policy head, does not
//! perform the legal-action masking (that requires live session/observation
//! state this pure module has no access to), and does not cache anything on
//! a search node. Those remain the search-loop wiring's job (design item 5),
//! explicitly out of scope here per the task's instructions.
//!
//! # Determination: what "masked, renormalized" input this module expects
//!
//! Section 1.2 describes one continuous pipeline -- "masked to exactly the
//! node's live ordered legal-action set..., renormalized over that set, and
//! quantized... via deterministic largest-remainder (Hamilton)
//! apportionment" -- without drawing a module boundary inside it. This
//! module's boundary is: **the caller performs masking; this module performs
//! renormalization and apportionment together, as one exact-integer
//! operation.**
//!
//! This is not a free choice; it follows from what a pure, checkpoint-free,
//! session-free module can and cannot do. Masking requires knowing which
//! flat-action indices are legal for a live decision, which requires session
//! state this module has no access to (and item 1's own scope, "apportionment
//! algorithm, scale, tie rule," names none of that). Renormalization and
//! apportionment, by contrast, are a matter of pure arithmetic once the
//! per-legal-action weights are in hand, and largest-remainder apportionment
//! subsumes explicit renormalization for free: `floor(w_i * SCALE /
//! sum(w))` is exactly "renormalize `w_i` to a distribution over the legal
//! set, then scale and floor," computed as one exact-integer expression
//! rather than as a separate floating-point division step that would need
//! its own rounding-hazard analysis. [`quantize_prior_v1`] therefore accepts
//! per-legal-action weights that need not already sum to 1 (or to anything
//! in particular): it renormalizes implicitly by dividing by their actual
//! sum. Whether the caller passes weights before or after an explicit
//! renormalization division makes no difference to the result.
//!
//! Each weight is required to lie in `[0.0, 1.0]`. This is not an arbitrary
//! restriction: every weight this module is designed to consume is one
//! masked component of a probability distribution that summed to exactly 1
//! over the *full* flat-action space before masking (Section 1.2: "The raw
//! output is masked to exactly the node's live ordered legal-action set");
//! no single nonnegative component of a distribution summing to 1 can itself
//! exceed 1, in either the pre- or post-renormalization reading.
//!
//! # Determination: exact-integer arithmetic, not floating-point division
//!
//! v1's own countersigned Amendment 1 requires "genuine integer
//! `isqrt`/`ilog2` algorithms, not float-cast shortcuts," inherited verbatim
//! by this design (Section 1.1). This module applies the identical
//! discipline to apportionment: the largest-remainder computation
//! ([`hamilton_apportion_prior_v1`]) uses only exact `u128` integer
//! arithmetic (multiply, then integer divide/modulo) once weights are in
//! fixed-point integer form, never a floating-point division followed by a
//! float `floor()`. This matters structurally, not just stylistically: a
//! naive `w_i / sum * SCALE` computed in floating point could, in principle,
//! round a per-action share fractionally above its true rational value at
//! exactly the wrong point, pushing `floor()` one unit too high and breaking
//! the invariant this module's own tests assert (`sum(P_int) ==
//! 1,000,000`, exactly, every time). Exact-integer division has no such
//! failure mode: `floor_total = sum(floor(w_i * SCALE / total))` is
//! *provably* `<= SCALE` and `SCALE - floor_total` is *provably* `<
//! legal_action_count`, from the exact-rational identity `sum(w_i) ==
//! total`, with no dependence on floating-point rounding anywhere in the
//! apportionment step itself. This mirrors the established, precedented
//! technique already in this crate for exactly this class of problem
//! (`fast_sampler.rs`'s `FastCategoricalScratch::apportion`, whose contract
//! JSON literally names the same rule: "floor(weight*2**64/sum), then
//! residual units by descending exact integer remainder and ascending
//! legal-action index" -- this module reuses that rule verbatim, at scale
//! 1,000,000 instead of 2**64, without reusing `fast_sampler`'s
//! exponential-weighting step, since this module's input is already
//! per-action weight, not a logit requiring softmax).
//!
//! # Determination: `f32` -> exact-integer conversion via power-of-two
//! scaling plus `round_ties_even`
//!
//! The task's own float-in-contract-surface rule requires any `f32`-to-
//! integer conversion to be bit-defined ("via `to_bits` ordering or
//! explicitly specified rounding"). [`quantize_legal_action_weight_v1`]
//! converts each `f32` weight (known to lie in `[0.0, 1.0]`) to a `u64`
//! fixed-point integer by multiplying by `2**32`
//! (`MODEL_GUIDED_SEARCH_PRIOR_QUANTIZATION_FIXED_POINT_SCALE_V1`) and
//! rounding with `f32::round_ties_even`, an IEEE-754-specified
//! `roundToIntegralTiesToEven` operation, not an implementation-defined
//! library shortcut (the same standard Section 1.3 step 5 states for its own
//! rounding step; see that module's docs for why this specific operation is
//! also immune to the dynamic-rounding-mode-register hazard).
//!
//! The multiply-by-`2**32` step is itself *exactly* lossless for every
//! finite `f32` in this domain: multiplying an IEEE-754 float by a power of
//! two changes only its exponent field, never its mantissa bits, so the
//! product carries exactly the same, and no more, significant bits as the
//! input `weight` (provably true here because the exponent never leaves
//! `f32`'s valid range for inputs in `[0.0, 1.0]` scaled by `2**32`). The
//! rounding step therefore only ever discards bits genuinely below `f32`'s
//! own 24-bit significand precision, never introduces error beyond what the
//! `f32` input itself already carries, and its result is always itself
//! exactly representable in `f32` before the final `as u64` cast (proved in
//! the module's own test suite: for `weight >= 2**-9`, the scaled product is
//! already an exact integer at that magnitude with no bits below the
//! binary point; for smaller `weight`, the scaled product's integer part is
//! `<= 2**23`, within `f32`'s exact-integer range).
//!
//! The fixed-point scale is `2**32`, deliberately far finer than the
//! `2**20`-ish resolution of the final 1,000,000 apportionment scale, so
//! this intermediate quantization step is never the binding precision
//! constraint ahead of apportionment's own largest-remainder redistribution
//! -- it captures the *full* precision an `f32` input can carry (24
//! significant bits) with 8 bits of headroom to spare.
//!
//! # Determination: all-zero weight is a hard error, not a fallback
//!
//! The design does not name a fallback for a legal-action set whose weights
//! all quantize to zero. A real checkpoint's masked-but-unrenormalized
//! output can only produce this if every legal action's true probability
//! underflowed to exactly zero in `f32` -- an extreme, and arguably
//! malformed, degenerate case the design's mode (a)/(b)/(c) gates would want
//! surfaced, not silently patched over by an invented uniform fallback this
//! document never authorizes. [`hamilton_apportion_prior_v1`] therefore
//! fails closed (`ZeroTotalWeight`) rather than inventing behavior.

use core::fmt;

/// Fixed apportionment/selection-bonus-divisor scale (design Section 1.2:
/// "a fixed scale of 1,000,000, the same scale v1's own bonus formula
/// already uses internally," referring to
/// `kernel_native_search_opponent_v1::integer_ucb_bonus_v1`'s own internal
/// `1_000_000u64` radicand constant). Used both as the apportionment total
/// for [`hamilton_apportion_prior_v1`] and as the divisor in
/// [`puct_bonus_v1`].
pub const MODEL_GUIDED_SEARCH_PRIOR_QUANTIZATION_SCALE_V1: u64 = 1_000_000;

/// Precision, in bits, of the fixed-point integer each legal-action `f32`
/// weight is quantized to before exact-integer apportionment. See the
/// module docs ("Determination: `f32` -> exact-integer conversion") for why
/// 32 bits was chosen: strictly more than `f32`'s 24-bit significand, with
/// headroom, so this step never becomes the binding precision constraint.
pub const MODEL_GUIDED_SEARCH_PRIOR_QUANTIZATION_FIXED_POINT_SCALE_BITS_V1: u32 = 32;

/// `2 ** MODEL_GUIDED_SEARCH_PRIOR_QUANTIZATION_FIXED_POINT_SCALE_BITS_V1`,
/// as the exact `f32` value used to scale each input weight before rounding.
/// Exact because multiplying an IEEE-754 float by a power of two never
/// rounds (see module docs).
pub const MODEL_GUIDED_SEARCH_PRIOR_QUANTIZATION_FIXED_POINT_SCALE_V1: f32 =
    (1u64 << MODEL_GUIDED_SEARCH_PRIOR_QUANTIZATION_FIXED_POINT_SCALE_BITS_V1) as f32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelGuidedSearchPriorQuantizationErrorV1 {
    /// [`quantize_prior_v1`] / [`hamilton_apportion_prior_v1`] called with no
    /// legal actions. v1's own contract requires "every root action must
    /// receive one visit," so a real caller should never reach this with an
    /// empty set; a pure contract module still fails closed rather than
    /// silently returning an empty result.
    EmptyLegalActionSet,
    /// A legal-action weight at `index` was not a finite value in
    /// `[0.0, 1.0]`. `bits` is the raw `f32::to_bits()` pattern, for exact
    /// diagnosis (distinguishes negative zero, NaN payloads, and infinities).
    WeightOutOfDomain { index: usize, bits: u32 },
    /// Every legal-action weight quantized to exactly zero; see module docs,
    /// "Determination: all-zero weight is a hard error."
    ZeroTotalWeight,
    /// [`puct_bonus_v1`]'s `bonus * p_int` product, divided by the prior
    /// scale, does not fit in `u64`. Reachable only for `bonus` values far
    /// outside what `kernel_native_search_opponent_v1::integer_ucb_bonus_v1`
    /// can actually produce; still checked, since this function accepts any
    /// `u64` at the type level.
    PuctBonusOverflow { bonus: u64, p_int: u32 },
    /// A defensive, structurally-proven-unreachable invariant failed (see
    /// module docs, "exact-integer arithmetic" section, for the proof each
    /// `code` corresponds to). Mirrors
    /// `fast_sampler::FastCategoricalError::InternalInvariant`.
    InternalInvariant { code: &'static str },
}

impl fmt::Display for ModelGuidedSearchPriorQuantizationErrorV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for ModelGuidedSearchPriorQuantizationErrorV1 {}

/// Converts one legal action's masked (not-necessarily-renormalized)
/// probability weight into an exact `u64` fixed-point integer. `index` is
/// used only for error attribution. See module docs for the exactness
/// argument.
pub fn quantize_legal_action_weight_v1(
    index: usize,
    weight: f32,
) -> Result<u64, ModelGuidedSearchPriorQuantizationErrorV1> {
    // `(0.0..=1.0).contains(&weight)` is `false` for NaN (all comparisons
    // with NaN are false under IEEE-754), so this single range check also
    // rejects NaN without a separate `is_finite()` call; it likewise
    // rejects both infinities, since neither lies in `[0.0, 1.0]`.
    if !(0.0..=1.0).contains(&weight) {
        return Err(
            ModelGuidedSearchPriorQuantizationErrorV1::WeightOutOfDomain {
                index,
                bits: weight.to_bits(),
            },
        );
    }
    let scaled = weight * MODEL_GUIDED_SEARCH_PRIOR_QUANTIZATION_FIXED_POINT_SCALE_V1;
    let rounded = scaled.round_ties_even();
    // `rounded` is a nonnegative, finite, exactly-integer-valued `f32` no
    // larger than `2**32`; the cast is exact.
    Ok(rounded as u64)
}

/// Deterministic largest-remainder (Hamilton) apportionment of already
/// fixed-point-quantized legal-action weights to
/// [`MODEL_GUIDED_SEARCH_PRIOR_QUANTIZATION_SCALE_V1`], in ascending
/// flat-action-index order (i.e. `fixed_point_weights[i]` is action `i`).
/// Ties in the apportionment remainder break by ascending flat-action index
/// (design Section 1.2). `sum(result) ==
/// MODEL_GUIDED_SEARCH_PRIOR_QUANTIZATION_SCALE_V1` exactly, for every
/// non-error input (see module docs for the exact-rational proof).
pub fn hamilton_apportion_prior_v1(
    fixed_point_weights: &[u64],
) -> Result<Vec<u32>, ModelGuidedSearchPriorQuantizationErrorV1> {
    if fixed_point_weights.is_empty() {
        return Err(ModelGuidedSearchPriorQuantizationErrorV1::EmptyLegalActionSet);
    }
    let total: u128 = fixed_point_weights.iter().map(|&w| u128::from(w)).sum();
    if total == 0 {
        return Err(ModelGuidedSearchPriorQuantizationErrorV1::ZeroTotalWeight);
    }
    let scale = u128::from(MODEL_GUIDED_SEARCH_PRIOR_QUANTIZATION_SCALE_V1);

    let mut quotients: Vec<u32> = Vec::with_capacity(fixed_point_weights.len());
    let mut remainders: Vec<u128> = Vec::with_capacity(fixed_point_weights.len());
    let mut floor_total: u128 = 0;
    for &weight in fixed_point_weights {
        let numerator = u128::from(weight) * scale;
        let quotient = numerator / total;
        let remainder = numerator % total;
        floor_total += quotient;
        let quotient = u32::try_from(quotient).map_err(|_| {
            ModelGuidedSearchPriorQuantizationErrorV1::InternalInvariant {
                code: "prior-quotient-exceeds-scale",
            }
        })?;
        quotients.push(quotient);
        remainders.push(remainder);
    }

    let residual = scale.checked_sub(floor_total).ok_or(
        ModelGuidedSearchPriorQuantizationErrorV1::InternalInvariant {
            code: "prior-floor-total-exceeds-scale",
        },
    )?;
    if residual >= fixed_point_weights.len() as u128 {
        return Err(
            ModelGuidedSearchPriorQuantizationErrorV1::InternalInvariant {
                code: "prior-residual-exceeds-legal-action-count",
            },
        );
    }
    let residual = residual as usize;

    // Total order over (remainder, index): remainder descending, then index
    // ascending. No two elements ever compare equal (index is unique), so
    // the result is fully determined regardless of sort algorithm/stability.
    let mut order: Vec<usize> = (0..fixed_point_weights.len()).collect();
    order.sort_by(|&a, &b| remainders[b].cmp(&remainders[a]).then(a.cmp(&b)));
    for &index in order.iter().take(residual) {
        quotients[index] += 1;
    }
    Ok(quotients)
}

/// Full Section 1.2 apportionment pipeline: validate and quantize each
/// masked legal-action weight, then Hamilton-apportion to
/// [`MODEL_GUIDED_SEARCH_PRIOR_QUANTIZATION_SCALE_V1`]. `legal_action_weights[i]`
/// is read as the (masked, not-necessarily-renormalized) probability weight
/// for the legal action at flat-action index `i`.
pub fn quantize_prior_v1(
    legal_action_weights: &[f32],
) -> Result<Vec<u32>, ModelGuidedSearchPriorQuantizationErrorV1> {
    if legal_action_weights.is_empty() {
        return Err(ModelGuidedSearchPriorQuantizationErrorV1::EmptyLegalActionSet);
    }
    let mut fixed_point = Vec::with_capacity(legal_action_weights.len());
    for (index, &weight) in legal_action_weights.iter().enumerate() {
        fixed_point.push(quantize_legal_action_weight_v1(index, weight)?);
    }
    hamilton_apportion_prior_v1(&fixed_point)
}

/// Descending-`P_int` expansion order (design Section 1.2, "Expansion
/// order"), ties broken by ascending flat-action index. `p_int[i]` is read
/// as the prior for the legal action at flat-action index `i`. Always
/// succeeds: any `u32` slice, including empty, has a well-defined order.
pub fn prior_expansion_order_v1(p_int: &[u32]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..p_int.len()).collect();
    order.sort_by(|&a, &b| p_int[b].cmp(&p_int[a]).then(a.cmp(&b)));
    order
}

/// `bonus_PUCT(a) = floor(bonus(a) * P_int(a) / 1,000,000)` (design Section
/// 1.2, "Selection bonus"). `bonus` is v1's own, unchanged, integer UCB
/// bonus value; this function owns only the new multiply/divide/floor the
/// design adds on top of it.
pub fn puct_bonus_v1(
    bonus: u64,
    p_int: u32,
) -> Result<u64, ModelGuidedSearchPriorQuantizationErrorV1> {
    let product = u128::from(bonus) * u128::from(p_int);
    let quotient = product / u128::from(MODEL_GUIDED_SEARCH_PRIOR_QUANTIZATION_SCALE_V1);
    u64::try_from(quotient)
        .map_err(|_| ModelGuidedSearchPriorQuantizationErrorV1::PuctBonusOverflow { bonus, p_int })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::SplitMix64;

    #[test]
    fn constants_are_pinned() {
        assert_eq!(MODEL_GUIDED_SEARCH_PRIOR_QUANTIZATION_SCALE_V1, 1_000_000);
        assert_eq!(
            MODEL_GUIDED_SEARCH_PRIOR_QUANTIZATION_FIXED_POINT_SCALE_BITS_V1,
            32
        );
        assert_eq!(
            MODEL_GUIDED_SEARCH_PRIOR_QUANTIZATION_FIXED_POINT_SCALE_V1,
            4_294_967_296.0
        );
    }

    #[test]
    fn weight_domain_rejects_negative_over_one_and_non_finite() {
        assert_eq!(
            quantize_legal_action_weight_v1(3, -0.000_001),
            Err(
                ModelGuidedSearchPriorQuantizationErrorV1::WeightOutOfDomain {
                    index: 3,
                    bits: (-0.000_001_f32).to_bits(),
                }
            )
        );
        assert_eq!(
            quantize_legal_action_weight_v1(5, 1.000_001),
            Err(
                ModelGuidedSearchPriorQuantizationErrorV1::WeightOutOfDomain {
                    index: 5,
                    bits: (1.000_001_f32).to_bits(),
                }
            )
        );
        assert_eq!(
            quantize_legal_action_weight_v1(0, f32::NAN),
            Err(
                ModelGuidedSearchPriorQuantizationErrorV1::WeightOutOfDomain {
                    index: 0,
                    bits: f32::NAN.to_bits(),
                }
            )
        );
        assert_eq!(
            quantize_legal_action_weight_v1(0, f32::INFINITY),
            Err(
                ModelGuidedSearchPriorQuantizationErrorV1::WeightOutOfDomain {
                    index: 0,
                    bits: f32::INFINITY.to_bits(),
                }
            )
        );
        assert_eq!(
            quantize_legal_action_weight_v1(0, f32::NEG_INFINITY),
            Err(
                ModelGuidedSearchPriorQuantizationErrorV1::WeightOutOfDomain {
                    index: 0,
                    bits: f32::NEG_INFINITY.to_bits(),
                }
            )
        );
        // Boundary values are admitted, not rejected.
        assert!(quantize_legal_action_weight_v1(0, 0.0).is_ok());
        assert!(quantize_legal_action_weight_v1(0, 1.0).is_ok());
    }

    #[test]
    fn weight_quantization_is_exact_at_power_of_two_boundaries() {
        assert_eq!(quantize_legal_action_weight_v1(0, 0.0).unwrap(), 0);
        assert_eq!(quantize_legal_action_weight_v1(0, 1.0).unwrap(), 1u64 << 32);
        assert_eq!(quantize_legal_action_weight_v1(0, 0.5).unwrap(), 1u64 << 31);
        assert_eq!(
            quantize_legal_action_weight_v1(0, 0.25).unwrap(),
            1u64 << 30
        );
    }

    #[test]
    fn weight_quantization_round_trip_never_exceeds_f32_exact_integer_range_below_the_boundary() {
        // For weight < 2**-9, the scaled product's magnitude is <= 2**23,
        // within f32's exact-integer range, and round_ties_even is exact
        // IEEE-754 rounding of the true mathematical product (itself exact,
        // since scaling by a power of two never rounds). This test pins that
        // structural argument against a concrete, exactly-constructed value
        // (division by a power of two is exact absent underflow) rather than
        // only asserting it in prose.
        let tiny = 1.0_f32 / 16_384.0; // == 2**-14 exactly, well below 2**-9
        let fixed = quantize_legal_action_weight_v1(0, tiny).unwrap();
        assert_eq!(fixed, 1u64 << 18); // 2**-14 * 2**32 == 2**18 exactly
    }

    #[test]
    fn apportionment_rejects_empty_and_all_zero() {
        assert_eq!(
            hamilton_apportion_prior_v1(&[]),
            Err(ModelGuidedSearchPriorQuantizationErrorV1::EmptyLegalActionSet)
        );
        assert_eq!(
            hamilton_apportion_prior_v1(&[0, 0, 0]),
            Err(ModelGuidedSearchPriorQuantizationErrorV1::ZeroTotalWeight)
        );
        assert_eq!(
            quantize_prior_v1(&[]),
            Err(ModelGuidedSearchPriorQuantizationErrorV1::EmptyLegalActionSet)
        );
    }

    #[test]
    fn single_legal_action_receives_the_entire_scale() {
        assert_eq!(quantize_prior_v1(&[1.0]).unwrap(), vec![1_000_000]);
        assert_eq!(quantize_prior_v1(&[0.000_001]).unwrap(), vec![1_000_000]);
    }

    #[test]
    fn two_equal_weights_split_evenly_with_no_residual() {
        assert_eq!(
            quantize_prior_v1(&[0.5, 0.5]).unwrap(),
            vec![500_000, 500_000]
        );
    }

    #[test]
    fn three_exactly_tied_weights_assign_the_residual_to_the_lowest_index() {
        // Any three IDENTICAL positive fixed-point weights w produce
        // identical quotient (floor(SCALE/3) = 333_333) and identical
        // remainder for every action, by exact-rational cancellation:
        // floor(w*SCALE / (3*w)) == floor(SCALE/3) and
        // (w*SCALE) mod (3*w) == w * (SCALE mod 3) == w, for any w > 0. The
        // single unit of residual (1,000,000 - 3*333_333 == 1) therefore
        // resolves purely by the ascending-index tie rule.
        let result = quantize_prior_v1(&[0.3, 0.3, 0.3]).unwrap();
        assert_eq!(result, vec![333_334, 333_333, 333_333]);
        assert_eq!(result.iter().sum::<u32>(), 1_000_000);
    }

    #[test]
    fn residual_distribution_follows_descending_remainder_then_ascending_index() {
        // Four legal actions with distinct fixed-point weights, hand-worked
        // against MODEL_GUIDED_SEARCH_PRIOR_QUANTIZATION_SCALE_V1 = 1e6.
        // weights (fixed-point, arbitrary units): 1, 2, 3, 4 -> total 10.
        // numerator_i = weight_i * 1_000_000; quotient_i = floor(numerator/10);
        // remainder_i = numerator_i mod 10.
        // i=0: numerator=1_000_000, quotient=100_000, remainder=0
        // i=1: numerator=2_000_000, quotient=200_000, remainder=0
        // i=2: numerator=3_000_000, quotient=300_000, remainder=0
        // i=3: numerator=4_000_000, quotient=400_000, remainder=0
        // floor_total = 1_000_000, residual = 0: exact split, no ties to
        // break. Use a case with a genuine, non-degenerate residual instead:
        // weights 1, 1, 1, 4 (total 7).
        // i=0: numerator=1_000_000, quotient=142_857, remainder=1_000_000 - 142_857*7=1
        // i=1: identical to i=0: quotient=142_857, remainder=1
        // i=2: identical to i=0: quotient=142_857, remainder=1
        // i=3: numerator=4_000_000, quotient=571_428, remainder=4_000_000-571_428*7=4
        // floor_total = 142_857*3 + 571_428 = 999_999, residual = 1.
        // Highest remainder is index 3 (remainder 4), so it receives the
        // residual unit despite not being the lowest index: proves the tie
        // rule is remainder-first, index-second, not index-only.
        let result = hamilton_apportion_prior_v1(&[1, 1, 1, 4]).unwrap();
        assert_eq!(result, vec![142_857, 142_857, 142_857, 571_429]);
        assert_eq!(result.iter().sum::<u32>(), 1_000_000);
    }

    #[test]
    fn sum_is_preserved_exactly_across_many_pseudo_random_weight_vectors() {
        let mut rng = SplitMix64::seed(0x9E37_79B9_7F4A_7C15);
        for trial in 0..2_000u32 {
            let width = 1 + (rng.next_u64() % 32) as usize;
            let mut weights = Vec::with_capacity(width);
            for _ in 0..width {
                // Draw a uniform u32 and scale to [0.0, 1.0] as f64 first
                // (exact widening) to avoid the test's own RNG-to-f32 step
                // needing separate exactness scrutiny; the value under test
                // is quantize_prior_v1's own arithmetic, not this draw.
                let raw = (rng.next_u64() >> 40) as f64 / f64::from(1u32 << 24);
                weights.push(raw as f32);
            }
            let result = quantize_prior_v1(&weights).unwrap();
            assert_eq!(
                result.iter().map(|&v| u64::from(v)).sum::<u64>(),
                MODEL_GUIDED_SEARCH_PRIOR_QUANTIZATION_SCALE_V1,
                "trial {trial} width {width} weights {weights:?}"
            );
        }
    }

    #[test]
    fn apportionment_is_order_invariant_up_to_the_explicit_index_tie_rule() {
        // Operate on hamilton_apportion_prior_v1 directly, with hand-chosen
        // fixed-point weights guaranteed to have pairwise-distinct
        // remainders, so this test isolates apportionment's own
        // permutation-commutation property from f32 quantization (covered
        // separately above). weights [100,150,200,250,297] sum to 997, a
        // prime coprime to the 1_000_000 scale; since multiplication by
        // 1_000_000 mod 997 is then a bijection on Z/997Z, five distinct
        // weights (each < 997) produce five pairwise-distinct remainders,
        // so no tie-break rule is ever exercised and the permutation must
        // commute unconditionally.
        let original: [u64; 5] = [100, 150, 200, 250, 297];
        assert_eq!(original.iter().sum::<u64>(), 997);
        let permutation = [3usize, 0, 4, 1, 2];
        let permuted: Vec<u64> = permutation.iter().map(|&i| original[i]).collect();

        let result_original = hamilton_apportion_prior_v1(&original).unwrap();
        let result_permuted = hamilton_apportion_prior_v1(&permuted).unwrap();

        // Confirm the no-ties precondition actually holds for this input,
        // rather than merely asserting it in a comment.
        let remainder_of = |weight: u64| -> u128 {
            let numerator =
                u128::from(weight) * u128::from(MODEL_GUIDED_SEARCH_PRIOR_QUANTIZATION_SCALE_V1);
            numerator % 997
        };
        let mut remainders: Vec<u128> = original.iter().map(|&w| remainder_of(w)).collect();
        remainders.sort_unstable();
        remainders.dedup();
        assert_eq!(remainders.len(), original.len(), "precondition: no ties");

        for (new_position, &original_index) in permutation.iter().enumerate() {
            assert_eq!(
                result_permuted[new_position], result_original[original_index],
                "position {new_position} (originally index {original_index})"
            );
        }
    }

    #[test]
    fn expansion_order_is_descending_p_int_then_ascending_index() {
        assert_eq!(prior_expansion_order_v1(&[]), Vec::<usize>::new());
        assert_eq!(prior_expansion_order_v1(&[5]), vec![0]);
        assert_eq!(prior_expansion_order_v1(&[10, 30, 20]), vec![1, 2, 0]);
        // Exact ties resolve by ascending index.
        assert_eq!(
            prior_expansion_order_v1(&[7, 9, 9, 7, 9]),
            vec![1, 2, 4, 0, 3]
        );
    }

    #[test]
    fn puct_bonus_matches_the_design_formula_and_floors() {
        // floor(1_414 * 250_000 / 1_000_000) = floor(353.5) = 353
        assert_eq!(puct_bonus_v1(1_414, 250_000).unwrap(), 353);
        // Exact division: floor(1_000 * 500_000 / 1_000_000) = 500
        assert_eq!(puct_bonus_v1(1_000, 500_000).unwrap(), 500);
        // Zero prior means zero bonus regardless of the UCB term.
        assert_eq!(puct_bonus_v1(999_999, 0).unwrap(), 0);
        // Full prior mass passes the UCB term through unchanged.
        assert_eq!(puct_bonus_v1(8_123, 1_000_000).unwrap(), 8_123);
    }

    #[test]
    fn puct_bonus_overflow_is_reachable_and_reported() {
        // p_int == MODEL_GUIDED_SEARCH_PRIOR_QUANTIZATION_SCALE_V1 never
        // overflows regardless of bonus (the divide exactly cancels the
        // multiply: bonus * 1_000_000 / 1_000_000 == bonus, which always
        // fits in u64 since bonus already is one). A genuine P_int from
        // quantize_prior_v1 is always <= the scale, so this function's
        // realistic callers never trigger this path; it is still checked,
        // since the type signature accepts any u32, and a p_int far outside
        // that realistic range (u32::MAX) combined with bonus == u64::MAX
        // does overflow u64.
        assert_eq!(puct_bonus_v1(u64::MAX, 1_000_000).unwrap(), u64::MAX);
        assert_eq!(
            puct_bonus_v1(u64::MAX, u32::MAX),
            Err(
                ModelGuidedSearchPriorQuantizationErrorV1::PuctBonusOverflow {
                    bonus: u64::MAX,
                    p_int: u32::MAX,
                }
            )
        );
    }

    #[test]
    fn realistic_masked_legal_action_distribution_apportions_and_orders_consistently() {
        // A plausible masked (pre-renormalization) policy slice: five legal
        // actions out of a much larger full action space, summing to well
        // under 1.0.
        let weights = [0.02_f32, 0.001, 0.15, 0.004, 0.003];
        let p_int = quantize_prior_v1(&weights).unwrap();
        assert_eq!(p_int.iter().sum::<u32>(), 1_000_000);
        // Highest-weight action (index 2) must expand first.
        assert_eq!(prior_expansion_order_v1(&p_int)[0], 2);
    }
}
