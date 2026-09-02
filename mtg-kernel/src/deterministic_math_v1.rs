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
//! or a primitive whose result is exactly specified by IEEE 754 for every
//! input, leaving no implementation latitude (`f32::abs`, `f32::copysign`,
//! `f32::round`, `f32::clamp`, `f32::is_nan`, `f32::is_infinite`).
//! Honesty note (round-2 countersign): on this Windows/MSVC target
//! `f64::round` DOES lower to a CRT `round` import rather than a compiler
//! builtin (verified by an isolated dumpbin probe); that is acceptable
//! here, and only here, because `round` is an exactly-specified operation
//! with a unique correct answer per input, unlike the transcendental
//! functions this module exists to avoid. The determinism contract
//! therefore rests on exact-specification, not on import-freedom; the
//! transcendental imports (`tanhf`/`logf`/`exp`/`pow`) remain banned from
//! this module's bodies.
//!
//! # Panel-driven revision (2026-08-15)
//!
//! An adversarial math/determinism review of the first version of this
//! module (the version pinned by commits `156ce88`/`8e9d54f` on this
//! branch) found one BLOCKER and one DEFECT in the algorithm itself, fixed
//! in this revision:
//!
//! - **BLOCKER, the small-linear seam.** The passthrough guard was
//!   `ax < TANH_SMALL_LINEAR_THRESHOLD_V1` (strict), so the exact boundary
//!   value fell into the general formula, which at that magnitude suffered
//!   from the cancellation defect below (thousands of ULPs off, and
//!   non-monotone against the passthrough branch immediately below it).
//!   Fixed by making the guard `<=` (inclusive) -- direct computation
//!   (mpmath, 50-digit precision, verified in this revision's own test
//!   suite below) confirms passthrough is exactly correctly-rounded both
//!   at the boundary and one ULP above it, so this alone would have closed
//!   the immediate blocker, but the deeper defect below needed its own fix
//!   regardless, since the general formula remained wrong for a wide
//!   region above the seam.
//! - **DEFECT, the `1 - 2/(e^{2x}+1)` formula's cancellation.** This
//!   one-sided identity computes tanh as a *subtraction of two O(1)
//!   quantities* (`1.0` and a fraction close to `1.0`), so for any `x`
//!   whose true `tanh(x)` is small, the result loses relative precision in
//!   proportion to how small it is: measured against an independent
//!   mpmath oracle, this revision's own investigation found errors up to
//!   ~4096 ULP near the small-linear seam and multi-hundred-ULP errors
//!   persisting for a wide region above it (an octave-by-octave scan
//!   showed the defect only fell below single-digit ULP error around
//!   `x ~ 2^-4`). Increasing the Taylor degree does not fix this: the
//!   error is a cancellation artifact of the outer subtraction, not a
//!   truncation artifact of the inner polynomial. Fixed by reformulating
//!   as `tanh(x) = expm1(2x) / (expm1(2x) + 2)` (mathematically identical:
//!   `(e^{2x}-1)/(e^{2x}+1)`), computing `expm1` directly rather than via
//!   `e^{2x} - 1`, and additionally performing the whole "regular region"
//!   computation internally in `f64` (still zero libm calls: `f64` `+`,
//!   `-`, `*`, `/` carry the exact same Rust-language strict-IEEE-754,
//!   no-implicit-FMA guarantees the audit already established for `f32`),
//!   rounding to `f32` exactly once at the very end via `as f32` (a
//!   correctly-rounded narrowing cast, not a libm call). This closes the
//!   cancellation structurally rather than patching it: measured against
//!   the same independent oracle, the fixed algorithm's maximum deviation
//!   across a several-hundred-thousand-point scan (dense coverage of both
//!   seams plus random interior sampling) is 1 ULP, with the seams and the
//!   saturation region exact (0 ULP); see the oracle-comparison test below
//!   for the committed, generation-script-provenanced version of this
//!   claim.
//!
//! The saturation threshold was also DEFECTIVE (not just imprecise): its
//! original justification used the half-ULP gap *above* `1.0` (`2^-24`)
//! where the correct governing gap, for rounding *up into* `1.0` from
//! below, is half the gap to `1.0`'s *predecessor* (`2^-25`). The true
//! crossover -- found by direct binary search against the mpmath oracle
//! over representable `f32` inputs, not by formula -- is `x =
//! f32::from_bits(0x4110_2cb4)` (`9.010913848876953...`): the previous
//! `9.0` cutoff was short by about `0.0109`, a region of `11,444`
//! representable `f32` inputs the original algorithm saturated early
//! instead of computing (exhaustively verified in this revision's test
//! suite: every one of those 11,444 inputs has correctly-rounded
//! `tanh(x) == f32::from_bits(0x3f7f_ffff)`, one ULP below `1.0`, and the
//! fixed general-formula path reproduces that exactly for every one of
//! them). See the module's own oracle-comparison test for the mpmath
//! evidence this revision's own verification pass produced.
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
//! 3. **Small-magnitude linear region.** If `ax <= TANH_SMALL_LINEAR_THRESHOLD_V1`
//!    (`2^-12`, **inclusive** -- see the panel-driven-revision note above
//!    for why this must not be a strict `<`), return `x` unchanged. This
//!    is not a shortcut approximation: for any `f32` `x` with
//!    `|x| <= 2^-12`, the true value of `x - x^3/3` (the first two terms
//!    of tanh's Taylor series) rounds to exactly `x` in `f32` (the cubic
//!    term is more than one full `f32` ULP below `x`'s own magnitude at
//!    this threshold, with margin: `(2^-12)^2 / 3 ≈ 1.79e-8`, versus half
//!    a `f32` ULP relative to 1.0 of `2^-24 ≈ 5.96e-8`), so returning `x`
//!    directly is the correctly-rounded value of tanh's own defining
//!    series in this region, computed with zero rounding error rather
//!    than approximated (confirmed directly against the mpmath oracle at
//!    and around this exact boundary in the golden battery below, not
//!    just argued analytically). This also exactly preserves every
//!    subnormal input (including the smallest, `f32::from_bits(1)`) and
//!    signed zero (`tanh_f32_v1(-0.0) == -0.0`, bit-for-bit), immune even
//!    to a dirtied DAZ flag (denormals-are-zero affects hardware FPU
//!    *arithmetic* on subnormal operands; this branch performs no
//!    arithmetic at all on `x`, just returns it, so DAZ has nothing to
//!    act on -- see the dedicated DAZ-immunity test below).
//! 4. **Saturation region.** If `ax >= TANH_SATURATION_THRESHOLD_V1`
//!    (`f32::from_bits(0x4110_2cb4)`, `≈9.010914`, the true crossover
//!    derived above), return `1.0_f32.copysign(x)`.
//! 5. **Regular region** (`TANH_SMALL_LINEAR_THRESHOLD_V1 < ax <
//!    TANH_SATURATION_THRESHOLD_V1`): compute via
//!    `tanh(ax) = expm1(2*ax) / (expm1(2*ax) + 2)`, entirely in `f64`:
//!    - `ax64 = ax as f64` (exact widening, no rounding)
//!    - `t = 2.0_f64 * ax64`
//!    - `em1 = expm1_f64_v1(t)` (Section below)
//!    - `result_abs64 = (em1 / (em1 + 2.0_f64)).clamp(0.0, 1.0)` (the
//!      `clamp` is a structural safety net, not an expected correction: it
//!      guarantees the `[0.0, 1.0]` half of the output-range property
//!      holds even if a future coefficient change introduced a small
//!      overshoot, rather than relying on empirical measurement alone)
//!    - `result_abs = result_abs64 as f32` (the one, single, correctly-
//!      rounded narrowing conversion in this whole branch)
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
//! # `expm1_f64_v1`: algorithm, operation order pinned
//!
//! A private helper, valid only for finite `t >= 0` up to
//! `2 * TANH_SATURATION_THRESHOLD_V1` (`≈18.0218`), which is the only
//! domain `tanh_f32_v1` ever calls it with. It is not a general-purpose
//! `expm1` (no overflow/underflow saturation, no negative-input handling)
//! and must not be used outside that contract. Standard range reduction
//! plus a fixed-degree Taylor polynomial for `e^r - 1` directly (not
//! `e^r` followed by a subtraction, which would reintroduce exactly the
//! cancellation this function exists to avoid), computed in this exact
//! order, entirely in `f64`:
//!
//! 1. `k_f = (t * EXPM1_INV_LN2_F64_V1).round()` (`f64::round`, half away
//!    from zero); `k = k_f as i32`.
//! 2. Single-constant reduction against `ln(2)`:
//!    `r = t - (k as f64) * EXPM1_LN2_F64_V1`. A `f64`-precision `ln(2)`
//!    constant (52 mantissa bits) is sufficient here without a Cody-Waite
//!    two-part split (needed in the earlier, withdrawn `f32`-only
//!    version): the cancellation error from reducing against a single
//!    full-precision constant is bounded by `k * 2^-52`, which for this
//!    domain's `k` (up to `26`, since `18.03 / ln(2) ≈ 26.01`) is at most
//!    `~5.8e-15` -- utterly negligible against the `f32`-precision target
//!    (`~6e-8`) this function's caller ultimately narrows to. `r` is small
//!    by construction (`|r| <= ln(2)/2 ≈ 0.3466`).
//! 3. Degree-7 Taylor polynomial for `e^r - 1`
//!    (`e^r - 1 = r * sum_{n=1}^{7} r^(n-1) / n!`), evaluated by Horner's
//!    method in descending degree on the `n=2..=7` coefficients and then
//!    one final multiply by `r` (the `n=1` coefficient, exactly `1.0`, is
//!    the implicit leading term of that multiply, so it is never written
//!    down or added as a separate `+ 1.0` step -- this is what avoids
//!    forming `1.0 + tiny - 1.0`, the cancellation pattern this whole
//!    function exists to avoid): `value = (((((C7)*r + C6)*r + C5)*r +
//!    C4)*r + C3)*r + C2`; `eminus1_r = (value * r + C1) * r`. Plain
//!    Taylor coefficients, not a minimax fit: at `|r| <= 0.3466` in `f64`
//!    precision the truncated remainder (order `r^8/8! ≈ 3e-9`) is many
//!    orders of magnitude below the `f32`-precision target, so there is no
//!    accuracy benefit to a fitted polynomial or a higher degree here (an
//!    adversarial-review candidate of adding an eighth term was tested
//!    against the oracle and made no measurable difference once the
//!    cancellation itself was fixed, confirming the remaining few-ULP
//!    error before the `f64` rewrite was accumulated rounding through the
//!    computation chain, not Taylor truncation).
//! 4. Reconstruction, `e^t - 1 = (2^k - 1) + 2^k * (e^r - 1)`, via exact
//!    bit-level construction of `2^k` (`f64::from_bits(((k + 1023) as
//!    u64) << 52)`, valid because this domain's `k` (`0..=26`) sits well
//!    inside the normal `f64` exponent range) multiplied into the
//!    polynomial result and combined with the exactly-representable (for
//!    this `k` range) integer `2^k - 1`. This is the same algebraic
//!    identity the withdrawn `f32`-only version used for `e^t` itself,
//!    adapted to `expm1`'s `-1` so the outer cancellation never happens.
//!
//! Empirical accuracy: see the module's own independent-oracle comparison
//! test, `oracle_correctly_rounded_comparison_v1`, for the committed,
//! generation-script-provenanced measurement (maximum deviation 1 ULP
//! across several hundred oracle-table entries spanning both seams
//! densely, subnormals, the passthrough region, the polynomial interior
//! at varied exponents, and the saturation region; the seams and
//! saturation region are exact, 0 ULP). See the max-ULP-versus-libm probe
//! comparison test (`native_forward_determinism_probe_v1.rs`) for the
//! separate measurement against the production `f32::tanh()` path.
//!
//! # Panel ruling extension (2026-08-16): `softmax_legal_action_weights_v1`
//! and `exp_f64_v1`
//!
//! `CLAUDE-MODEL-GUIDED-SEARCHER-DESIGN-V1.md` item 6a's own review left one
//! open question unresolved by design (Section 1.2 states only "masked...
//! renormalized... quantized," not which function converts a raw policy
//! logit into a legal-action weight) and item 6a's first revision closed it
//! with a per-action sigmoid, documented honestly as a resolved-but-open
//! implementation choice. A countersigning panel's own recomputation found
//! the sigmoid choice materially wrong on the merits, not merely
//! undocumented: for the worked example legal logits `{10.0, 0.0 x9}`,
//! softmax puts `~99.96%` of the prior mass on the dominant action, while
//! sigmoid-plus-Hamilton-apportionment puts only `~18%` there -- sigmoid
//! near-flattens the trained policy head's own guidance, because the net's
//! policy head is trained as a softmax distribution (cross-entropy against
//! a normalized target), so only a softmax-shaped prior carries the object
//! the net actually learned. This also matters beyond mode (b): mode (c)'s
//! visit-count training targets are downstream of whichever prior shapes
//! expansion order and the PUCT bonus, so a badly-shaped prior would bias
//! that future target's distribution too, not just today's search quality.
//! This ruling replaces the sigmoid with a deterministic softmax; the
//! sigmoid function is removed outright (not deprecated or left as a dead
//! alternative surface).
//!
//! **What lives here versus at the call site.** This module owns the new
//! bit-defined primitive ([`exp_f64_v1`]) and the softmax wrapper
//! ([`softmax_legal_action_weights_v1`]) that the model-guided search's
//! evaluator calls directly, exactly mirroring this module's own existing
//! split for `tanh_f32_v1`/`expm1_f64_v1`: one clean, oracle-verified
//! `pub(crate)` entry point built from a private, domain-restricted
//! primitive, all of it libm-free and bit-order-pinned. Masking is not this
//! module's concern (as with `expm1_f64_v1`, "no session/observation state
//! is available here"): the caller already hands this function exactly the
//! node's live ordered legal-action logits (see
//! `model_guided_search_core_v1.rs`'s own docs for why the encode path
//! upstream already performs the masking, so no separate mask step is
//! needed by the time logits reach this module).
//!
//! **What this function returns, and why it never divides by a sum.**
//! [`softmax_legal_action_weights_v1`] returns UNNORMALIZED per-action
//! weights, `e^(logit_i - max_logit)`, in the caller's own index order, not
//! a normalized probability vector. This is deliberate, not a shortcut:
//! `model_guided_search_prior_quantization_v1::quantize_prior_v1`'s own
//! contract already "accepts per-legal-action weights that need not
//! already sum to 1... it renormalizes implicitly by dividing by their
//! actual sum," via exact `u128` largest-remainder apportionment, not
//! floating-point division. Handing it `e^(logit_i - max_logit)` directly
//! therefore reproduces the exact softmax RATIOS (`e^(logit_i-max) /
//! sum_j e^(logit_j-max)` is mathematically identical to
//! `e^(logit_i)/sum_j e^(logit_j)`, the standard softmax, since the
//! `e^-max` factor cancels in every ratio) while never performing a
//! floating-point summation-then-division at all in this module: the exact-
//! integer apportionment downstream is what completes the softmax
//! semantics, losslessly. This is a genuine determinism improvement over a
//! conventional softmax implementation (which normally divides by a
//! floating-point sum, a reduction whose order could in principle matter),
//! not merely a convenience: there is no order-sensitive floating-point
//! reduction anywhere in this function.
//!
//! **Max-subtraction is exact, not approximate.** `max_logit` is found by a
//! single explicit ascending-index scan (fixed order, documented, not an
//! iterator-adapter chain whose evaluation order is left implicit), using
//! strict `>` so ties keep the first-seen maximum value -- though since
//! `f32::max`-style comparison is associative/commutative exactly (unlike
//! `+`), which occurrence of a tied maximum "wins" cannot change the
//! computed max VALUE itself, only which index a human reader might think
//! of as "the" argmax; every tied action still receives `logit_i ==
//! max_logit` and therefore `x_i == 0.0` exactly. Each `x_i = f64::from
//! (logit_i) - f64::from(max_logit)` is computed independently per action
//! (never accumulated across actions), so `x_i` does not depend on which
//! other actions were scanned before or after it, and the max action's own
//! `x_i` is bit-exact `0.0` (both operands are exact, lossless `f32`-to-
//! `f64` widenings of the identical value, so the subtraction commits no
//! rounding error at all for that action): [`exp_f64_v1`] of exactly `0.0`
//! is, by this function's own algorithm (see below), exactly `1.0`, so the
//! dominant action's weight is always bit-exact `1.0`, never an
//! approximation of it.
//!
//! **The clamp floor.** By construction, every `x_i <= 0.0` (since
//! `max_logit` is the true maximum). [`exp_f64_v1`] is a private,
//! domain-restricted primitive, valid only for finite `x` in
//! `[EXP_DOMAIN_FLOOR_V1, 0.0]`, mirroring `expm1_f64_v1`'s own bounded,
//! not-general-purpose contract; [`softmax_legal_action_weights_v1`] clamps
//! every `x_i` to that floor (`x_i.max(EXP_DOMAIN_FLOOR_V1)`) before
//! calling it. See [`EXP_DOMAIN_FLOOR_V1`]'s own doc comment for the exact
//! floor value and why it was chosen with wide margin below the boundary
//! where `e^x` first rounds to exactly `0.0f32`, rather than sitting close
//! to that boundary. A weight of exactly `0.0f32` for a legal action is not
//! itself an error: `quantize_legal_action_weight_v1`'s own precondition is
//! the closed range `[0.0, 1.0]`, and only an ALL-zero weight vector (every
//! legal action's weight quantizing to zero) is `quantize_prior_v1`'s own
//! hard `ZeroTotalWeight` error -- unreachable here since the dominant
//! action's weight is always exactly `1.0`.
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
//! MXCSR as a mandatory baseline. Cost: one `stmxcsr` (a single,
//! non-serializing store-from-register instruction) per search-scoped
//! forward call, immeasurably small next to that call's own network
//! forward pass; it stays permanently rather than becoming a debug-only or
//! feature-gated check because its entire purpose is catching a *future*
//! in-process contamination source (a CUDA context, a different numerical
//! library) that does not exist in this crate's dependency graph today --
//! removing the gate once it has never fired yet would defeat exactly the
//! guarantee it exists to provide.

/// Saturation cutoff for [`tanh_f32_v1`]'s magnitude: the true crossover
/// where correctly-rounded `tanh` first equals exactly `1.0` in `f32`,
/// found by direct binary search against an independent mpmath oracle
/// (panel-driven revision; see the module doc above and
/// `oracle_correctly_rounded_comparison_v1` below). NOT `9.0`: that was
/// the withdrawn first version's defect.
use std::cell::Cell;
use std::fmt;

pub(crate) const TANH_SATURATION_THRESHOLD_V1: f32 = f32::from_bits(0x4110_2cb4);

/// Small-magnitude linear-region cutoff for [`tanh_f32_v1`]. `2^-12`
/// (`0x3980_0000`), exactly representable in `f32`. Pinned as an exact bit
/// pattern, matching the other exact constants below, rather than a
/// decimal literal. See the algorithm doc above (step 3); the comparison
/// against this constant is `<=` (inclusive), not `<` -- the panel-driven
/// revision's BLOCKER fix.
const TANH_SMALL_LINEAR_THRESHOLD_V1: f32 = f32::from_bits(0x3980_0000);

// ln(2) and its reciprocal, `f64` precision, pinned as exact bit patterns
// (nearest-`f64` rounding of the exact irrational/rational values) so the
// source of truth is the literal bits, not a re-rounded decimal literal.
// See the module doc's `expm1_f64_v1` section for why a single
// full-precision `f64` constant (no Cody-Waite two-part split) is
// sufficient at this precision.
const EXPM1_LN2_F64_V1: f64 = f64::from_bits(0x3fe6_2e42_fefa_39ef);
const EXPM1_INV_LN2_F64_V1: f64 = f64::from_bits(0x3ff7_1547_652b_82fe);

// Taylor coefficients for e^r - 1 = r*(C1 + C2*r + C3*r^2 + ... + C7*r^6),
// C_n = 1/n!, pinned as exact `f64` bit patterns (nearest-`f64` rounding
// of the exact rational 1/n!). C1 (= 1/1! = 1.0 exactly) is the implicit
// leading coefficient of the final `* r` in `expm1_f64_v1` and is not a
// separate named constant; see that function's Horner structure.
const EXPM1_TAYLOR_C2_V1: f64 = f64::from_bits(0x3fe0_0000_0000_0000); // 1/2!
const EXPM1_TAYLOR_C3_V1: f64 = f64::from_bits(0x3fc5_5555_5555_5555); // 1/3!
const EXPM1_TAYLOR_C4_V1: f64 = f64::from_bits(0x3fa5_5555_5555_5555); // 1/4!
const EXPM1_TAYLOR_C5_V1: f64 = f64::from_bits(0x3f81_1111_1111_1111); // 1/5!
const EXPM1_TAYLOR_C6_V1: f64 = f64::from_bits(0x3f56_c16c_16c1_6c17); // 1/6!
const EXPM1_TAYLOR_C7_V1: f64 = f64::from_bits(0x3f2a_01a0_1a01_a01a); // 1/7!

/// Private, domain-restricted deterministic `expm1` (`e^t - 1`). Valid
/// only for finite `t >= 0` up to `2 * TANH_SATURATION_THRESHOLD_V1`
/// (`≈18.0218`); see the module doc's `expm1_f64_v1` section for the full
/// algorithm and why computing `e^r - 1` directly (never `e^r` followed by
/// a subtraction) is what makes this function safe against the
/// cancellation the panel-driven revision found in the withdrawn `e^t`-
/// then-subtract formulation.
fn expm1_f64_v1(t: f64) -> f64 {
    let k_f = (t * EXPM1_INV_LN2_F64_V1).round();
    let k = k_f as i32;
    let k_as_f64 = f64::from(k);

    let r = t - k_as_f64 * EXPM1_LN2_F64_V1;

    let mut value = EXPM1_TAYLOR_C7_V1;
    value = value * r + EXPM1_TAYLOR_C6_V1;
    value = value * r + EXPM1_TAYLOR_C5_V1;
    value = value * r + EXPM1_TAYLOR_C4_V1;
    value = value * r + EXPM1_TAYLOR_C3_V1;
    value = value * r + EXPM1_TAYLOR_C2_V1;
    // Implicit C1 = 1.0: `value * r + 1.0`, then the outer `* r` below,
    // i.e. `eminus1_r = (value * r + 1.0) * r`, is exactly
    // `r + C2*r^2 + ... + C7*r^7`: never forms `1.0 + tiny - 1.0`.
    let eminus1_r = (value * r + 1.0_f64) * r;

    // Exact power-of-two scaling via direct exponent-field construction;
    // no libm `exp2`/`powi`/`powf`/`exp_m1` call. Safe for this function's
    // whole domain: k in 0..=26 here, far inside the normal f64 exponent
    // range, and 2^k - 1 is exactly representable in f64 for this k range
    // (f64 has 52 mantissa bits).
    let scale_bits: u64 = ((k + 1023) as u64) << 52;
    let scale = f64::from_bits(scale_bits);
    (scale - 1.0_f64) + scale * eminus1_r
}

/// Kernel-owned, bit-defined deterministic scalar `tanh`. See the module
/// doc above for the full, order-pinned algorithm. No call in this
/// function's body, or in [`expm1_f64_v1`]'s, reaches the platform's
/// system libm.
pub(crate) fn tanh_f32_v1(x: f32) -> f32 {
    if x.is_nan() {
        return f32::NAN;
    }
    let ax = x.abs();
    if ax.is_infinite() {
        return 1.0_f32.copysign(x);
    }
    if ax <= TANH_SMALL_LINEAR_THRESHOLD_V1 {
        return x;
    }
    if ax >= TANH_SATURATION_THRESHOLD_V1 {
        return 1.0_f32.copysign(x);
    }
    let ax64 = f64::from(ax);
    let t = 2.0_f64 * ax64;
    let em1 = expm1_f64_v1(t);
    let denom = em1 + 2.0_f64;
    let result_abs64 = (em1 / denom).clamp(0.0_f64, 1.0_f64);
    let result_abs = result_abs64 as f32;
    result_abs.copysign(x)
}

// ---------------------------------------------------------------------
// Panel ruling extension (2026-08-16): softmax_legal_action_weights_v1 and
// its private exp_f64_v1 primitive. See the module doc's own "Panel ruling
// extension" section above for the full rationale (why softmax replaces
// the withdrawn sigmoid, why this function never divides by a sum, the
// max-subtraction exactness argument, and the clamp-floor argument).
// ---------------------------------------------------------------------

/// Domain floor for [`exp_f64_v1`]: inputs are clamped to this value before
/// being passed to it, which is valid only for `x` in
/// `[EXP_DOMAIN_FLOOR_V1, 0.0]` (mirroring [`expm1_f64_v1`]'s own bounded,
/// not-general-purpose contract, not an accident of shared style). Chosen
/// with comfortable margin below the exact IEEE-754 round-to-nearest-even
/// boundary where `e^x` first rounds to exactly `0.0f32` rather than the
/// smallest subnormal (`ln(2^-150) ~= -103.97212`, the halfway point
/// between `0.0` and f32's smallest subnormal `2^-149 ~= 1.401e-45`, which
/// is itself the tie-break floor since round-to-nearest-even favors the
/// "more even" `0.0` at an exact tie): at `x = -120.0`, `e^-120 ~=
/// 7.67e-53`, more than seven orders of magnitude below even the smallest
/// f32 subnormal, so every clamped input narrows to bit-exact `0.0f32`
/// unambiguously, with wide margin, rather than this module needing to
/// argue precisely about a tight boundary. `k` (this function's own
/// range-reduction integer, see [`exp_f64_v1`]'s doc) stays in `-174..=0`
/// for this floor, still far inside `f64`'s normal exponent range.
const EXP_DOMAIN_FLOOR_V1: f64 = -120.0;

/// Private, domain-restricted deterministic `exp` (`e^x`). Valid only for
/// finite `x` in `[EXP_DOMAIN_FLOOR_V1, 0.0]`; its only caller
/// ([`softmax_legal_action_weights_v1`]) clamps to this domain before
/// calling, exactly as [`expm1_f64_v1`]'s own contract requires its own
/// caller (`tanh_f32_v1`) to pre-guard its domain via branch structure
/// rather than a runtime assertion. Not a general-purpose `exp` (no
/// positive-`x` handling, no overflow saturation): reusing it outside this
/// exact contract is a bug, not a feature, identically to `expm1_f64_v1`'s
/// own warning.
///
/// Same range-reduction technique as [`expm1_f64_v1`] (this module's own
/// established, panel-reviewed precedent for exactly this class of
/// problem): reduce against a single full-precision `f64` `ln(2)`
/// constant (no Cody-Waite two-part split needed, for the same reason
/// `expm1_f64_v1` gives, extended to this function's own wider `k` range
/// below), evaluate a degree-7 Taylor polynomial for `e^r` by Horner's
/// method (reusing the identical [`EXPM1_TAYLOR_C2_V1`]..
/// [`EXPM1_TAYLOR_C7_V1`] coefficients [`expm1_f64_v1`] already defines --
/// the same `1/n!` values power a plain `e^r` series exactly as they power
/// `e^r - 1`'s; deliberately not renamed or duplicated as literals, since
/// `expm1_f64_v1`/`tanh_f32_v1` are already independently countersigned
/// and this addition does not change their behavior, only reads their
/// already-fixed constants a second time), then reconstruct via exact
/// bit-level `2^k` scaling. Unlike `expm1_f64_v1`, there is no
/// cancellation hazard here to guard against with an "expm1-style"
/// reformulation: this function's result is used directly as one softmax
/// numerator among several, never subtracted from an `O(1)` quantity
/// afterward, so a plain Taylor series for `e^r` itself (leading
/// coefficient `1.0`, written out rather than omitted) is safe as written,
/// with no cancellation-avoidance trick needed.
///
/// Computed, in this exact order:
///
/// 1. `k_f = (x * EXPM1_INV_LN2_F64_V1).round()` (`f64::round`, half away
///    from zero, identical primitive to `expm1_f64_v1`'s); `k = k_f as
///    i32`.
/// 2. `r = x - (k as f64) * EXPM1_LN2_F64_V1`; `|r| <= ln(2)/2 ~= 0.3466`
///    by construction, the identical bound `expm1_f64_v1` derives. For
///    this function's domain (`x` in `[-120.0, 0.0]`), `k` ranges over
///    `-174..=0` (`-120.0 / ln(2) ~= -173.13`), still utterly negligible
///    reduction error against a single full-precision constant (`|k| *
///    2^-52 <= 174 * 2^-52 ~= 3.9e-14`, far below the `f32`-precision
///    target (`~6e-8`) this function's caller ultimately narrows to), and
///    still well inside `f64`'s normal exponent range for the
///    reconstruction step below (`k + 1023` in `849..=1023`, nowhere near
///    the field's `0..=2046` boundary).
/// 3. Degree-7 Taylor polynomial, Horner's method, descending degree,
///    reusing `EXPM1_TAYLOR_C2_V1..C7_V1` plus explicit `1.0` coefficients
///    for the `r^1` and `r^0` terms (both written out, unlike
///    `expm1_f64_v1`'s implicit-leading-term trick, since there is no
///    cancellation to avoid here): `e^r = ((((((C7*r + C6)*r + C5)*r +
///    C4)*r + C3)*r + C2)*r + 1.0)*r + 1.0`.
/// 4. Reconstruction, `e^x = 2^k * e^r`, via the identical exact bit-level
///    `2^k` construction `expm1_f64_v1` uses (`f64::from_bits(((k + 1023)
///    as u64) << 52)`).
///
/// Accuracy: the Taylor truncation bound (`~3e-9` relative, at `|r| <=
/// 0.3466`) and the reduction-constant error bound (`~3.9e-14` at this
/// function's widest `k`) are both independent of how negative `x` is
/// (range reduction always brings `r` into the same fixed small interval
/// regardless of `x`'s magnitude), so this function's `f64`-precision
/// accuracy stays uniform across its whole documented domain; narrowing to
/// `f32` (the caller's one rounding per weight) is therefore correctly
/// rounded to within the oracle-verified contract goal below. See
/// `exp_f64_v1_oracle_correctly_rounded_comparison_v1` for the committed,
/// generation-script-provenanced measurement.
fn exp_f64_v1(x: f64) -> f64 {
    let k_f = (x * EXPM1_INV_LN2_F64_V1).round();
    let k = k_f as i32;
    let k_as_f64 = f64::from(k);

    let r = x - k_as_f64 * EXPM1_LN2_F64_V1;

    let mut value = EXPM1_TAYLOR_C7_V1;
    value = value * r + EXPM1_TAYLOR_C6_V1;
    value = value * r + EXPM1_TAYLOR_C5_V1;
    value = value * r + EXPM1_TAYLOR_C4_V1;
    value = value * r + EXPM1_TAYLOR_C3_V1;
    value = value * r + EXPM1_TAYLOR_C2_V1;
    value = value * r + 1.0_f64; // C1 = 1/1! = 1.0, written out (see doc).
    let exp_r = value * r + 1.0_f64; // C0 = 1/0! = 1.0, written out.

    // Exact power-of-two scaling via direct exponent-field construction;
    // no libm `exp2`/`powi`/`powf`/`exp` call. Safe for this function's
    // whole documented domain: k in -174..=0 here, far inside the normal
    // f64 exponent range.
    let scale_bits: u64 = ((k + 1023) as u64) << 52;
    let scale = f64::from_bits(scale_bits);
    scale * exp_r
}

/// Deterministic softmax-numerator conversion over one node's legal-action
/// logits, in the caller's own ascending index order (`weights[i]`
/// corresponds to `logits[i]`, positionally, always -- this function
/// performs no reordering of its own). Returns UNNORMALIZED per-action
/// weights (`e^(logit_i - max_logit)`, each in `[0.0, 1.0]`, the dominant
/// action(s) always exactly `1.0`); see the module doc's own "Panel ruling
/// extension" section for why the caller (`quantize_prior_v1`) is meant to
/// renormalize these, and why that design has no floating-point summation
/// reduction anywhere in this function at all. Callers must ensure every
/// `logit` is finite (this function does not itself validate that,
/// identically to `expm1_f64_v1`/`exp_f64_v1`'s own "caller's job"
/// contract); `model_guided_search_core_v1`'s own caller already validates
/// this upstream before calling.
pub(crate) fn softmax_legal_action_weights_v1(logits: &[f32]) -> Vec<f32> {
    let mut max_logit = f32::NEG_INFINITY;
    for &logit in logits {
        if logit > max_logit {
            max_logit = logit;
        }
    }
    let max_logit64 = f64::from(max_logit);

    let mut weights = Vec::with_capacity(logits.len());
    for &logit in logits {
        // Exact: both operands are lossless f32-to-f64 widenings of the
        // identical bit pattern for the dominant action, so this
        // subtraction commits no rounding error there (module doc,
        // "Max-subtraction is exact, not approximate").
        let diff = f64::from(logit) - max_logit64;
        let clamped = diff.max(EXP_DOMAIN_FLOOR_V1);
        let exp_value = exp_f64_v1(clamped);
        weights.push(exp_value as f32); // the one rounding to f32 per weight
    }
    weights
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
/// (`native_forward_determinism_probe_v1.rs`). `pub(crate)` so other
/// modules' test code (e.g.
/// `native_policy_value_net_v1`'s production-path gate coverage test) can
/// reuse it instead of a fourth copy of this same inline-asm block.
#[cfg(target_arch = "x86_64")]
pub(crate) fn read_mxcsr_v1() -> u32 {
    let mut mxcsr: u32 = 0;
    unsafe {
        std::arch::asm!(
            "stmxcsr [{0}]",
            in(reg) &mut mxcsr,
        );
    }
    mxcsr
}

/// Writes the calling thread's MXCSR.
///
/// This was `#[cfg(test)]` until the test-time-search wrapper's S0
/// engineering item ("MXCSR normalization at scorer process startup and on
/// every worker thread that runs a search, plus fail-closed verification
/// before the first search forward"). An earlier revision of this doc
/// comment stated that "production only ever needs to read and verify,
/// never repair" the FTZ/DAZ/rounding-control state; that is retracted
/// here rather than left standing. The forward-determinism audit
/// (`docs/audits/model_guided_forward_determinism_audit_v1.md`, Section 6
/// recommendation 3) asks for exactly the opposite: "an explicit MXCSR
/// read (and, if ever found dirty, a set, or a fail-closed error)". A
/// read-and-assert gate defends the forward but leaves the process with no
/// way to reach a pinned state at all: a thread whose MXCSR was dirtied by
/// an in-process CUDA context or a third-party library would simply panic
/// on every search, forever. Normalization is the repair half the audit
/// named, and it belongs in production.
///
/// `ldmxcsr` with no `options(...)` is a full compiler side effect, so it
/// cannot be reordered across neighboring floating-point work, the same
/// property [`read_mxcsr_v1`] documents for `stmxcsr`.
#[cfg(target_arch = "x86_64")]
pub(crate) fn write_mxcsr_v1(value: u32) {
    unsafe {
        std::arch::asm!(
            "ldmxcsr [{0}]",
            in(reg) &value,
        );
    }
}

/// The observed MXCSR state failed the pinned FTZ=0 / DAZ=0 /
/// rounding-control=0 contract. Carries the raw observed register value so
/// a caller can report exactly which bits were dirty without this module
/// having to enumerate them; nothing else is retained.
///
/// This is the fail-closed sibling of [`assert_pinned_mxcsr_state_v1`]:
/// the assert form stays as the forward pass's own last-line panic gate
/// (a `Result` there would change `forward_search_deterministic_v1`'s
/// error type and force every existing caller to widen), while this form
/// is what a scorer, launcher, or selector calls when it must refuse a
/// decision instead of aborting a live episode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MxcsrPinnedStateErrorV1 {
    observed: u32,
}

impl MxcsrPinnedStateErrorV1 {
    pub(crate) const fn observed_v1(self) -> u32 {
        self.observed
    }

    pub(crate) const fn code(self) -> &'static str {
        "mxcsr_pinned_state_mismatch"
    }
}

impl fmt::Display for MxcsrPinnedStateErrorV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} 0x{:08x}", self.code(), self.observed)
    }
}

impl std::error::Error for MxcsrPinnedStateErrorV1 {}

/// `true` when `mxcsr` has FTZ=0, DAZ=0, and rounding-control=0. The single
/// predicate [`assert_pinned_mxcsr_state_v1`], [`verify_pinned_mxcsr_state_v1`],
/// and [`normalize_pinned_mxcsr_state_v1`] all agree on, so the assert form
/// and the fail-closed form can never drift apart.
#[cfg(target_arch = "x86_64")]
const fn mxcsr_state_is_pinned_v1(mxcsr: u32) -> bool {
    mxcsr & MXCSR_DAZ_BIT_V1 == 0
        && mxcsr & MXCSR_FTZ_BIT_V1 == 0
        && (mxcsr >> MXCSR_ROUNDING_CONTROL_SHIFT_V1) & MXCSR_ROUNDING_CONTROL_MASK_V1 == 0
}

/// Fail-closed read-and-verify of the calling thread's pinned MXCSR state.
/// Never repairs, never panics; the caller decides what a mismatch means.
#[cfg(target_arch = "x86_64")]
pub(crate) fn verify_pinned_mxcsr_state_v1() -> Result<(), MxcsrPinnedStateErrorV1> {
    let observed = read_mxcsr_v1();
    if mxcsr_state_is_pinned_v1(observed) {
        Ok(())
    } else {
        Err(MxcsrPinnedStateErrorV1 { observed })
    }
}

/// Clears FTZ and DAZ and forces rounding-control to round-to-nearest-even
/// on the calling thread, leaving every other MXCSR field (the exception
/// masks and the sticky exception flags) exactly as found, then re-reads
/// and verifies. A post-write read that still fails the predicate is a hard
/// error, not a silent success: the write is a plain `ldmxcsr`, so the only
/// ways it can fail to take are ones this module must not paper over.
#[cfg(target_arch = "x86_64")]
pub(crate) fn normalize_pinned_mxcsr_state_v1() -> Result<(), MxcsrPinnedStateErrorV1> {
    let observed = read_mxcsr_v1();
    if !mxcsr_state_is_pinned_v1(observed) {
        let normalized = observed
            & !MXCSR_DAZ_BIT_V1
            & !MXCSR_FTZ_BIT_V1
            & !(MXCSR_ROUNDING_CONTROL_MASK_V1 << MXCSR_ROUNDING_CONTROL_SHIFT_V1);
        write_mxcsr_v1(normalized);
    }
    verify_pinned_mxcsr_state_v1()
}

/// Non-`x86_64` build: there is no MXCSR to verify, so the pinned-state
/// contract is vacuously satisfied. This mirrors the pre-existing scope
/// limitation of [`assert_pinned_mxcsr_state_v1`] and its single call site
/// in `native_policy_value_net_v1`, both of which are already
/// `x86_64`-only. Returning `Ok` here is not a silent fallback on a
/// supported target: it is the correct answer on a target where the
/// register does not exist. Cross-target byte-identity remains out of
/// scope, exactly as the forward-determinism audit records.
#[cfg(not(target_arch = "x86_64"))]
pub(crate) fn verify_pinned_mxcsr_state_v1() -> Result<(), MxcsrPinnedStateErrorV1> {
    Ok(())
}

/// Non-`x86_64` counterpart of [`normalize_pinned_mxcsr_state_v1`]; see
/// the non-`x86_64` [`verify_pinned_mxcsr_state_v1`] for why this is `Ok`.
#[cfg(not(target_arch = "x86_64"))]
pub(crate) fn normalize_pinned_mxcsr_state_v1() -> Result<(), MxcsrPinnedStateErrorV1> {
    Ok(())
}

thread_local! {
    /// One-shot-per-thread normalization latch. `Cell<bool>`, not a
    /// `Once`: the state is per-thread, so a process-wide `Once` would
    /// normalize exactly one thread and leave every other worker
    /// unpinned, which is the precise failure this latch exists to
    /// prevent.
    static MXCSR_THREAD_NORMALIZED_V1: Cell<bool> = const { Cell::new(false) };
}

/// Normalizes the calling thread's MXCSR the first time this thread asks,
/// then verifies on every call. Call it at the entry of any thread that is
/// about to run a search: a worker pool's threads each get their own
/// normalization, and the per-call verify means a register dirtied AFTER
/// the first normalization (an in-process CUDA context initializing on
/// that thread, say) is still caught, fail-closed, rather than trusted
/// because the latch is already set.
pub(crate) fn ensure_thread_mxcsr_normalized_v1() -> Result<(), MxcsrPinnedStateErrorV1> {
    let already = MXCSR_THREAD_NORMALIZED_V1.with(Cell::get);
    if already {
        return verify_pinned_mxcsr_state_v1();
    }
    normalize_pinned_mxcsr_state_v1()?;
    MXCSR_THREAD_NORMALIZED_V1.with(|flag| flag.set(true));
    Ok(())
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

    /// Re-pinned after the panel-driven revision (2026-08-15): the
    /// small-linear seam's guard changed from `<` to `<=` (BLOCKER fix,
    /// item 1) and the saturation cutoff moved from `9.0` to the true
    /// mpmath-verified crossover `f32::from_bits(0x4110_2cb4)` (DEFECT
    /// fix, item 2), so every case at or near either seam is either new or
    /// re-verified against the independent oracle (see
    /// `oracle_correctly_rounded_comparison_v1` for the generation-script-
    /// provenanced version of this same claim over a much larger table).
    /// Every bit pattern below at the two seams was independently checked
    /// against a 50-digit-precision mpmath reference before being pinned
    /// here, not just accepted from a single implementation run.
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
            // --- Small-linear seam (item 1: the guard is now `<=`). ---
            (
                "one_below_small_linear_seam",
                f32::from_bits(0x397f_ffff),
                0x397f_ffff, // passthrough; oracle-confirmed correctly-rounded
            ),
            (
                "at_small_linear_seam",
                TANH_SMALL_LINEAR_THRESHOLD_V1,
                0x3980_0000, // now passthrough (the BLOCKER fix): input == output
            ),
            (
                "one_above_small_linear_seam",
                f32::from_bits(0x3980_0001),
                0x3980_0001, // general formula; oracle-confirmed correctly-rounded
            ),
            ("positive_one", 1.0f32, 0x3f42_f7d6),
            ("negative_one", -1.0f32, 0xbf42_f7d6),
            // --- Saturation seam (item 2: cutoff moved to the true crossover). ---
            (
                "old_9_0_now_interior",
                f32::from_bits(0x4110_0000), // 9.0 exactly: no longer the cutoff
                0x3f7f_ffff,                 // oracle-confirmed: correctly rounds one ULP below 1.0
            ),
            (
                "last_input_below_1_0",
                f32::from_bits(0x4110_2cb3), // one ULP below the new threshold
                0x3f7f_ffff,
            ),
            (
                "at_saturation_threshold",
                TANH_SATURATION_THRESHOLD_V1, // f32::from_bits(0x4110_2cb4)
                0x3f80_0000,
            ),
            (
                "one_above_saturation_threshold",
                f32::from_bits(0x4110_2cb5),
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
            // Re-pinned 2026-08-15: panel-driven revision fixed the small-
            // linear seam guard (`<` -> `<=`) and moved the saturation
            // cutoff to the true mpmath-verified crossover; both are real,
            // reviewed algorithm-output changes this sweep is expected to
            // move on. See the module doc's "Panel-driven revision" note.
            hex,
            "71f3cc4fe4d7e028a0093456eef385f55527908bfedb411563a17d0838b82e71",
            "dense-sweep golden hash moved; this is a real algorithm-output \
             change (constants, operation order, or branch structure), not \
             a flaky test -- update deliberately if the change is reviewed \
             and intended"
        );
    }

    // -------------------------------------------------------------
    // Independent oracle comparison (item 3, panel-driven revision):
    // `tanh_f32_v1` compared against a table computed independently of
    // this crate's own implementation, by `mpmath` at 50 decimal digits
    // of precision, rounded to `f32` by a single safe double-rounding
    // through `f64`. Regenerate with:
    //   python python/tools/generate_deterministic_math_v1_oracle_goldens.py
    // (requires `mpmath`; see that script's own module doc for why it is
    // not a project dependency). Covers, per input_bits ascending: both
    // seams densely (64 consecutive bit patterns each), subnormals, the
    // passthrough region, the polynomial interior at varied exponents
    // (both signs), and the saturation region including extremes and the
    // infinities.
    // -------------------------------------------------------------

    include!("deterministic_math_v1_oracle_table_v1.rs");

    /// Signed-magnitude-to-ordered-integer key: adjacent representable
    /// `f32` values map to keys exactly 1 apart, matching `f32`'s own
    /// total order (ignoring NaN). Standard technique (Bruce Dawson's
    /// "comparing floating point numbers").
    fn ordered_key_v1(bits: u32) -> i64 {
        let signed = bits as i32;
        let ordered = if signed < 0 {
            i32::MIN.wrapping_sub(signed)
        } else {
            signed
        };
        ordered as i64
    }

    fn ulp_distance_v1(bits_a: u32, bits_b: u32) -> u64 {
        (ordered_key_v1(bits_a) - ordered_key_v1(bits_b)).unsigned_abs()
    }

    /// Bit ranges, inclusive, where the oracle table's own construction
    /// (see the generation script) guarantees the entries sit at or
    /// immediately adjacent to one of the two seams, or in the saturation
    /// region: these must be *exact* (0 ULP), not merely within the
    /// general 1-ULP contract goal, since seam and saturation correctness
    /// is exactly what items 1 and 2 of the panel-driven revision fixed.
    fn must_be_exact_v1(input_bits: u32) -> bool {
        let magnitude = input_bits & 0x7fff_ffff;
        let near_small_seam = ((0x3980_0000 - 32)..=(0x3980_0000 + 31)).contains(&magnitude);
        let near_sat_seam = ((0x4110_2cb4 - 32)..=(0x4110_2cb4 + 31)).contains(&magnitude);
        let saturated = magnitude >= 0x4110_2cb4;
        near_small_seam || near_sat_seam || saturated
    }

    #[test]
    fn oracle_correctly_rounded_comparison_v1() {
        let mut max_ulp = 0u64;
        let mut max_ulp_bits = 0u32;
        let mut exact_count = 0u32;
        for &(input_bits, expected_bits) in ORACLE_TABLE_V1.iter() {
            let input = f32::from_bits(input_bits);
            let actual_bits = tanh_f32_v1(input).to_bits();
            let distance = ulp_distance_v1(actual_bits, expected_bits);
            if distance == 0 {
                exact_count += 1;
            }
            if distance > max_ulp {
                max_ulp = distance;
                max_ulp_bits = input_bits;
            }
            if must_be_exact_v1(input_bits) {
                assert_eq!(
                    actual_bits, expected_bits,
                    "seam/saturation entry must be exact: input=0x{input_bits:08x} \
                     ({input:?}) got=0x{actual_bits:08x} expected=0x{expected_bits:08x}"
                );
            } else {
                assert!(
                    distance <= 1,
                    "contract goal is at most 1 ULP: input=0x{input_bits:08x} ({input:?}) \
                     got=0x{actual_bits:08x} expected=0x{expected_bits:08x} ulp={distance}"
                );
            }
        }
        println!(
            "oracle_correctly_rounded_comparison_v1: {} entries, {exact_count} exact (0 ULP), \
             max_ulp={max_ulp} at input_bits=0x{max_ulp_bits:08x} ({:?})",
            ORACLE_TABLE_V1.len(),
            f32::from_bits(max_ulp_bits),
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

    /// S0 normalization: each of the three dirty states is repaired in
    /// place, the repair is verified by a fresh read, and every OTHER
    /// MXCSR field (here the six exception masks, bits 7..=12) survives
    /// untouched. Normalization that silently reset the exception masks
    /// would be a different floating-point environment, not a pinned one.
    #[test]
    #[cfg(target_arch = "x86_64")]
    fn normalize_pinned_mxcsr_state_repairs_each_dirty_field_v1() {
        let handle = std::thread::spawn(|| {
            let original = read_mxcsr_v1();
            const EXCEPTION_MASK_FIELD_V1: u32 = 0b111_111 << 7;
            for dirty_bits in [
                MXCSR_DAZ_BIT_V1,
                MXCSR_FTZ_BIT_V1,
                1 << MXCSR_ROUNDING_CONTROL_SHIFT_V1,
                0b10 << MXCSR_ROUNDING_CONTROL_SHIFT_V1,
                MXCSR_DAZ_BIT_V1
                    | MXCSR_FTZ_BIT_V1
                    | (MXCSR_ROUNDING_CONTROL_MASK_V1 << MXCSR_ROUNDING_CONTROL_SHIFT_V1),
            ] {
                let dirty = original | dirty_bits;
                write_mxcsr_v1(dirty);
                assert_eq!(
                    verify_pinned_mxcsr_state_v1(),
                    Err(MxcsrPinnedStateErrorV1 { observed: dirty }),
                    "verify must fail closed on 0x{dirty:08x}"
                );
                normalize_pinned_mxcsr_state_v1().expect("normalization repairs a dirty MXCSR");
                assert_eq!(verify_pinned_mxcsr_state_v1(), Ok(()));
                assert_eq!(
                    read_mxcsr_v1() & EXCEPTION_MASK_FIELD_V1,
                    dirty & EXCEPTION_MASK_FIELD_V1,
                    "normalization must leave the exception masks alone"
                );
            }
            write_mxcsr_v1(original);
        });
        handle
            .join()
            .expect("mxcsr normalization thread must not panic");
    }

    /// The per-thread latch is per-thread: a second thread that has never
    /// normalized still repairs its own dirty register, and the verify on
    /// the already-latched path still catches a register dirtied AFTER the
    /// first normalization.
    #[test]
    #[cfg(target_arch = "x86_64")]
    fn ensure_thread_mxcsr_normalized_is_per_thread_and_reverifies_v1() {
        let handle = std::thread::spawn(|| {
            let original = read_mxcsr_v1();
            write_mxcsr_v1(original | MXCSR_FTZ_BIT_V1 | MXCSR_DAZ_BIT_V1);
            ensure_thread_mxcsr_normalized_v1().expect("first call normalizes this thread");
            assert_eq!(verify_pinned_mxcsr_state_v1(), Ok(()));
            // Latch is set; a register dirtied afterwards must still be
            // caught rather than trusted.
            let latched = read_mxcsr_v1();
            write_mxcsr_v1(latched | MXCSR_FTZ_BIT_V1);
            assert!(
                ensure_thread_mxcsr_normalized_v1().is_err(),
                "the latched path must re-verify, not trust the latch"
            );
            write_mxcsr_v1(original);
        });
        handle
            .join()
            .expect("mxcsr per-thread latch thread must not panic");
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

    /// Note 1, panel-driven revision: subnormal DAZ-immunity. Denormals-
    /// are-zero (DAZ) affects the hardware FPU's own *arithmetic* on
    /// subnormal operands (flushing them to zero before/during an add,
    /// multiply, etc.); `tanh_f32_v1`'s passthrough branch for a subnormal
    /// input performs no arithmetic on it at all (`ax <=
    /// TANH_SMALL_LINEAR_THRESHOLD_V1` is a comparison, and the branch
    /// simply returns `x`, its own bit pattern, unmodified), so a dirty
    /// DAZ flag has nothing to act on: this test proves that directly,
    /// with DAZ actually dirtied in a scoped thread (MXCSR is per-thread),
    /// not merely argued from the source.
    #[test]
    #[cfg(target_arch = "x86_64")]
    fn subnormal_passthrough_is_daz_immune_v1() {
        let handle = std::thread::spawn(|| {
            let original = read_mxcsr_v1();
            write_mxcsr_v1(original | MXCSR_DAZ_BIT_V1);
            for bits in [0x0000_0001u32, 0x0040_0000, 0x007f_ffff, 0x8000_0001] {
                let x = f32::from_bits(bits);
                assert!(
                    x.is_subnormal(),
                    "test setup bug: 0x{bits:08x} is not subnormal"
                );
                let y = tanh_f32_v1(x);
                assert_eq!(
                    y.to_bits(),
                    bits,
                    "subnormal passthrough must be bit-exact even with DAZ dirty: \
                     input=0x{bits:08x} got=0x{:08x}",
                    y.to_bits()
                );
            }
            write_mxcsr_v1(original);
        });
        handle
            .join()
            .expect("subnormal daz-immunity thread must not panic");
    }

    // -------------------------------------------------------------
    // Panel ruling extension (2026-08-16): exp_f64_v1 and
    // softmax_legal_action_weights_v1. Mirrors the tanh_f32_v1 test
    // discipline above (golden battery, dense-sweep hash, independent
    // oracle comparison, property tests), adapted to exp_f64_v1's own
    // `f64 -> f64` signature by comparing the one-rounding-to-f32 result
    // its only caller (softmax_legal_action_weights_v1) actually produces,
    // exactly as tanh_f32_v1's own tests validate the f32-visible surface
    // rather than expm1_f64_v1's raw f64 internals directly.
    // -------------------------------------------------------------

    fn exp_f64_v1_f32_bits(x: f64) -> u32 {
        (exp_f64_v1(x) as f32).to_bits()
    }

    /// Every case independently checked against a 50-digit-precision
    /// mpmath reference before being pinned here (see
    /// `generate_deterministic_math_v1_exp_oracle_goldens.py`'s own golden-
    /// case cross-check), not accepted from a single implementation run.
    #[test]
    fn exp_f64_v1_golden_battery_bits_v1() {
        let cases: &[(&str, f64, u32)] = &[
            ("zero", 0.0, 0x3f80_0000), // the dominant action's own x: exactly 1.0
            ("neg_zero", -0.0, 0x3f80_0000),
            ("floor_exact", EXP_DOMAIN_FLOOR_V1, 0x0000_0000),
            ("near_floor", -119.0, 0x0000_0000),
            ("neg_one", -1.0, 0x3ebc_5ab2),
            ("neg_half", -0.5, 0x3f1b_4598),
            ("worked_example_gap", -10.0, 0x383e_6bce), // coordinator's own {10,0} example
            ("expm1_domain_edge", -18.0218, 0x3280_00e0), // 2*TANH_SATURATION_THRESHOLD_V1
            ("neg_twenty", -20.0, 0x310d_a433),
            ("neg_fifty", -50.0, 0x1b69_2beb),
            ("normal_subnormal_boundary", -87.336544, 0x0080_0006),
            ("zero_round_boundary_below", -103.97212, 0x0000_0000),
            ("zero_round_boundary_above", -103.9, 0x0000_0001),
            ("neg_hundred_three", -103.0, 0x0000_0001),
            ("neg_hundred", -100.0, 0x0000_001b),
            // Exact reduction seam: r = -ln(2)/2, k = -1 (k*ln2 = -ln2, r =
            // x - k*ln2 = -ln2/2 - (-ln2) = ln2/2... constructed so |r| sits
            // at its own documented bound).
            ("reduction_seam", -0.346_573_590_279_972_64, 0x3f35_04f3),
            ("small_eps", -1.0e-10, 0x3f80_0000),
        ];
        for (name, input, expected_bits) in cases.iter().copied() {
            let actual_bits = exp_f64_v1_f32_bits(input);
            assert_eq!(
                actual_bits, expected_bits,
                "case {name}: input={input:?} got=0x{actual_bits:08x} expected=0x{expected_bits:08x}"
            );
        }
    }

    #[test]
    fn exp_f64_v1_golden_dense_sweep_hash_v1() {
        let mut hasher = Sha256::new();
        const STEPS: u32 = 200_000;
        for i in 0..=STEPS {
            let t = f64::from(i) / f64::from(STEPS);
            let x = EXP_DOMAIN_FLOOR_V1 * (1.0 - t); // linear sweep [-120.0, 0.0]
            hasher.update(exp_f64_v1_f32_bits(x).to_be_bytes());
        }
        let digest = hasher.finalize();
        let hex: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
        assert_eq!(
            hex, "880eca125c1b4d01342d61f4b14103a0a828c84c494610430af830d7b873ba6b",
            "dense-sweep golden hash moved; this is a real algorithm-output \
             change (constants, operation order, or branch structure), not \
             a flaky test -- update deliberately if the change is reviewed \
             and intended"
        );
    }

    // Independent oracle comparison for exp_f64_v1 (panel ruling,
    // 2026-08-16), mirroring tanh_f32_v1's own oracle discipline above.
    // Regenerate with:
    //   python python/tools/generate_deterministic_math_v1_exp_oracle_goldens.py
    // Covers, per input_bits ascending: the clamp-floor boundary and its
    // near neighbors, the zero-rounding boundary (ln(2^-150) ~= -103.972)
    // densely, the normal/subnormal boundary (ln(2^-126) ~= -87.337)
    // densely, zero itself (both signs), a dense octave-by-octave interior
    // sweep, and named reference points including the countersigning
    // panel's own worked-example gap (-10.0).
    include!("deterministic_math_v1_exp_oracle_table_v1.rs");

    /// Regions where this function's own correctness argument (module doc,
    /// clamp-floor note) says the result must be EXACT, not merely within
    /// the general 1-ULP goal: comfortably inside the zero-rounding region
    /// (`x <= -104.0`, deep past the exact tie boundary `~-103.9721` with
    /// margin), where every entry in the oracle table already independently
    /// rounds to `0.0f32`.
    fn exp_f64_v1_must_be_exact(input_bits: u32) -> bool {
        let x = f32::from_bits(input_bits);
        x <= -104.0
    }

    #[test]
    fn exp_f64_v1_oracle_correctly_rounded_comparison_v1() {
        let mut max_ulp = 0u64;
        let mut max_ulp_bits = 0u32;
        let mut exact_count = 0u32;
        for &(input_bits, expected_bits) in EXP_ORACLE_TABLE_V1.iter() {
            let input = f64::from(f32::from_bits(input_bits));
            let actual_bits = exp_f64_v1_f32_bits(input);
            let distance = ulp_distance_v1(actual_bits, expected_bits);
            if distance == 0 {
                exact_count += 1;
            }
            if distance > max_ulp {
                max_ulp = distance;
                max_ulp_bits = input_bits;
            }
            if exp_f64_v1_must_be_exact(input_bits) {
                assert_eq!(
                    actual_bits,
                    expected_bits,
                    "zero-rounding-region entry must be exact: input_bits=0x{input_bits:08x} \
                     ({:?}) got=0x{actual_bits:08x} expected=0x{expected_bits:08x}",
                    f32::from_bits(input_bits),
                );
            } else {
                assert!(
                    distance <= 1,
                    "contract goal is at most 1 ULP: input_bits=0x{input_bits:08x} \
                     ({:?}) got=0x{actual_bits:08x} expected=0x{expected_bits:08x} ulp={distance}",
                    f32::from_bits(input_bits),
                );
            }
        }
        println!(
            "exp_f64_v1_oracle_correctly_rounded_comparison_v1: {} entries, {exact_count} exact \
             (0 ULP), max_ulp={max_ulp} at input_bits=0x{max_ulp_bits:08x}",
            EXP_ORACLE_TABLE_V1.len(),
        );
    }

    #[test]
    fn exp_f64_v1_property_monotonic_on_sampled_grid_v1() {
        const STEPS: i32 = 400_000;
        let mut previous: Option<f64> = None;
        for i in 0..=STEPS {
            let t = f64::from(i) / f64::from(STEPS);
            let x = EXP_DOMAIN_FLOOR_V1 * (1.0 - t);
            let y = exp_f64_v1(x);
            if let Some(previous_y) = previous {
                assert!(
                    y >= previous_y,
                    "monotonicity violated: x={x} produced y={y} < previous y={previous_y}"
                );
            }
            previous = Some(y);
        }
    }

    #[test]
    fn exp_f64_v1_property_output_range_bounds_v1() {
        const STEPS: i32 = 400_000;
        for i in 0..=STEPS {
            let t = f64::from(i) / f64::from(STEPS);
            let x = EXP_DOMAIN_FLOOR_V1 * (1.0 - t);
            let y = exp_f64_v1(x);
            assert!((0.0..=1.0).contains(&y), "range violated for x={x}: y={y}");
        }
    }

    // -------------------------------------------------------------
    // softmax_legal_action_weights_v1: golden battery over the wrapper
    // itself (masking-free, index-order-sensitive behavior that
    // exp_f64_v1's own tests above do not exercise).
    // -------------------------------------------------------------

    /// Max-subtraction mutation boundary: the dominant action's own weight
    /// must be bit-exact `1.0` for any legal logit set, since its own
    /// `x_i` is exact `0.0` by construction (module doc, "Max-subtraction
    /// is exact, not approximate"). A mutation that skipped or corrupted
    /// max-subtraction would not, in general, produce exactly `1.0` here.
    #[test]
    fn softmax_dominant_action_weight_is_exactly_one_v1() {
        for logits in [
            vec![10.0_f32, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            vec![-5.0_f32, -3.0, 7.5, 2.0],
            vec![0.0_f32],
            vec![-1000.0_f32, 1000.0],
        ] {
            let weights = softmax_legal_action_weights_v1(&logits);
            let max = logits.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            for (index, (&logit, &weight)) in logits.iter().zip(weights.iter()).enumerate() {
                if logit == max {
                    assert_eq!(
                        weight.to_bits(),
                        1.0_f32.to_bits(),
                        "dominant action at index {index} must have weight exactly 1.0, \
                         got {weight} for logits {logits:?}"
                    );
                }
            }
        }
    }

    /// Two equal max logits: both must receive bit-exact equal (`1.0`)
    /// weight, not merely approximately equal.
    #[test]
    fn softmax_two_equal_max_logits_tie_case_v1() {
        let logits = vec![5.0_f32, 5.0, 1.0, -3.0];
        let weights = softmax_legal_action_weights_v1(&logits);
        assert_eq!(weights[0].to_bits(), 1.0_f32.to_bits());
        assert_eq!(weights[1].to_bits(), 1.0_f32.to_bits());
        assert_eq!(weights[0].to_bits(), weights[1].to_bits());
        assert!(weights[2] < weights[0]);
        assert!(weights[3] < weights[2]);
    }

    /// All-equal logits give bit-exact equal weights (every `x_i == 0.0`).
    #[test]
    fn softmax_all_equal_logits_give_equal_weights_exactly_v1() {
        let logits = vec![3.5_f32; 6];
        let weights = softmax_legal_action_weights_v1(&logits);
        for &weight in &weights {
            assert_eq!(weight.to_bits(), 1.0_f32.to_bits());
        }
    }

    /// A single legal action: its own weight must be exactly 1.0 (it is
    /// trivially the dominant action).
    #[test]
    fn softmax_single_legal_action_v1() {
        for logit in [0.0_f32, 123.456, -999.0, f32::MIN, f32::MAX] {
            let weights = softmax_legal_action_weights_v1(&[logit]);
            assert_eq!(weights.len(), 1);
            assert_eq!(weights[0].to_bits(), 1.0_f32.to_bits());
        }
    }

    /// Clamp-floor mutation boundary: an extreme spread (the losing action
    /// far below the winner) must still return a well-formed, finite,
    /// in-range weight for every action -- not NaN, not a garbage bit
    /// pattern from an out-of-contract `exp_f64_v1` call. This is the
    /// scenario an unclamped call could corrupt (module doc, "The clamp
    /// floor").
    #[test]
    fn softmax_extreme_spread_clamps_and_stays_well_formed_v1() {
        for spread in [200.0_f32, 1_000.0, 1.0e6, f32::MAX / 2.0] {
            let logits = vec![0.0_f32, -spread];
            let weights = softmax_legal_action_weights_v1(&logits);
            assert_eq!(weights[0].to_bits(), 1.0_f32.to_bits());
            assert!(
                weights[1].is_finite() && (0.0..=1.0).contains(&weights[1]),
                "extreme spread {spread} produced an out-of-contract weight: {:?}",
                weights[1]
            );
        }
    }

    /// Index-order boundary: `weights[i]` must correspond POSITIONALLY to
    /// `logits[i]`, not merely contain the right multiset of values in some
    /// order. Distinct, individually identifiable logits per index make a
    /// transposition bug detectable.
    #[test]
    fn softmax_output_is_positionally_index_matched_v1() {
        let logits = vec![1.0_f32, 2.0, 3.0, 4.0, 5.0];
        let weights = softmax_legal_action_weights_v1(&logits);
        assert_eq!(weights.len(), logits.len());
        // Strictly increasing input logits must produce strictly
        // increasing weights at the SAME indices (softmax is monotonic in
        // its own input), so any positional shuffle is detectable here.
        for window in weights.windows(2) {
            assert!(
                window[1] > window[0],
                "weights must be strictly increasing at increasing indices \
                 for strictly increasing logits: {weights:?}"
            );
        }
        // The last (highest-logit) action must be the unique dominant one.
        assert_eq!(*weights.last().unwrap(), 1.0_f32);
    }

    /// Sanity cross-check against the countersigning panel's own worked
    /// example (module doc, "Panel ruling extension"): legal logits
    /// `{10.0, 0.0 x9}` must put approximately 99.96% of the (Hamilton-
    /// apportioned, normalized) mass on the dominant action, reproducing
    /// the panel's own recomputation, not merely this implementation's own
    /// self-consistency.
    #[test]
    fn softmax_matches_panels_worked_example_v1() {
        let mut logits = vec![0.0_f32; 10];
        logits[0] = 10.0;
        let weights = softmax_legal_action_weights_v1(&logits);
        let sum: f64 = weights.iter().map(|&w| f64::from(w)).sum();
        let dominant_share = f64::from(weights[0]) / sum;
        assert!(
            (0.999_0..=0.999_9).contains(&dominant_share),
            "expected the dominant action's share to match the panel's own \
             ~99.96% recomputation, got {:.6}",
            dominant_share * 100.0
        );
    }
}
