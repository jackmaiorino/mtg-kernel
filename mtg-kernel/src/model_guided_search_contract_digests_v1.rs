//! Content-bound contract digests for the model-guided (test-time search)
//! wrapper.
//!
//! `LEAD_TEST_TIME_SEARCH_DESIGN_SKETCH_V2.md` Section 5 (S0 Engineering)
//! requires "quantization and deterministic-build digests bound to content".
//! Before this module, `ModelGuidedSearchAuthorityV1` validated its
//! `puct_prior_quantization_contract_sha256`,
//! `value_quantization_contract_sha256`, and
//! `forward_determinism_build_identity` fields STRUCTURALLY only (lower-hex,
//! 64 characters), by that module's own explicit and honest admission: the
//! contracts it needed to bind to "are being built concurrently in a sibling
//! worktree and are not available here". They are available now, in this
//! crate, so the placeholder discipline is discharged rather than carried
//! forward.
//!
//! # What "bound to content" means here, and what it deliberately does not
//!
//! The obvious implementation is a SHA-256 of each contract module's own
//! source file (`include_str!("model_guided_search_prior_quantization_v1.rs")`).
//! This module does NOT do that, for one reason: a source-file digest
//! changes when a doc typo is fixed and does not change when the same
//! numbers are produced by a differently-spelled but behaviorally identical
//! implementation. It binds the wrong thing. Every authority record in the
//! repo would then need re-minting for a comment edit, which trains
//! reviewers to treat digest churn as noise, which is precisely how a real
//! contract change slips through.
//!
//! Each digest here is instead computed over a canonical byte stream that
//! commits to:
//!
//! 1. a frozen contract-identity label (so two contracts can never collide
//!    even if their constants and probe outputs happened to agree);
//! 2. every pinned constant the contract declares, in a fixed order, by
//!    exact bit pattern (never by decimal formatting);
//! 3. the EXACT OUTPUT the contract's own live functions produce for a
//!    frozen probe battery, byte for byte.
//!
//! Item 3 is what makes these behavioral, not cosmetic: the digest moves if
//! and only if the contract's numeric behavior moves. A rewritten
//! implementation that computes the identical numbers keeps its digest,
//! correctly; a one-ULP change to any probe's result breaks it, correctly.
//!
//! The probe batteries are deliberately small and hand-chosen (boundaries,
//! ties, sign flips, saturation) rather than large and random: their job is
//! to be a fingerprint that a human can re-derive, not a test suite. The
//! real test suites live in each contract's own module and are unchanged.
//!
//! # The pinned literals and how they are kept honest
//!
//! Each digest has a pinned lower-hex constant next to its computing
//! function, and a test in this module asserts the two are equal. The
//! pinned literal is what
//! `model_guided_search_authority_v1::ModelGuidedSearchAuthorityV1::validate`
//! compares a record's field against, so a record minted against a
//! different contract fails closed at validation rather than silently
//! searching under a contract nobody registered. Recomputation happens once
//! per process (`OnceLock`), not once per `validate` call, so binding the
//! authority to live content costs nothing on the per-decision path.
//!
//! # The forward-determinism build identity
//!
//! `docs/audits/model_guided_forward_determinism_audit_v1.md` Section 6,
//! recommendation 5 asks for the Section 1.4 "forward-determinism build
//! identity" field to be wired into every registered authority record, and
//! recommendation 2 asks for the FMA/target-feature contract to be pinned
//! "for the record". This module's build identity commits to exactly the
//! things that decide whether two builds' `forward_search_deterministic_v1`
//! calls can differ:
//!
//! - the kernel-owned, libm-free `tanh_f32_v1` the deterministic forward
//!   substitutes for `f32::tanh()` (probed by bit pattern, so a change to
//!   the polynomial, the saturation threshold, or the operation order moves
//!   the digest);
//! - the `softmax_legal_action_weights_v1` primitive the prior conversion
//!   runs on the forward's own logits;
//! - the pinned MXCSR FTZ/DAZ/rounding-control contract;
//! - the target architecture, pointer width, and the two floating-point
//!   target features (`fma`, `avx`) whose presence would license the
//!   compiler to contract or vectorize this crate's arithmetic differently;
//! - the BUILD-FLAG CONTRACT itself
//!   (`MODEL_GUIDED_SEARCH_BUILD_FLAG_CONTRACT_V1`) together with the exact
//!   list of build-override variables it rejects and the exact list of
//!   forbidden floating-point fragments, so quietly narrowing the contract
//!   moves the identity instead of silently widening the escape hatch.
//!
//! That last bullet closes a gap the earlier version of this module had.
//! Recording only arch, pointer width, `fma`, `avx`, and the probe outputs
//! is not sufficient, because a build under
//! `RUSTFLAGS=-C llvm-args=-fp-contract=fast` matches every one of those
//! and may still perform different arithmetic: the flag changes neither
//! the source, nor the target features, nor (necessarily) any probe this
//! module evaluates at the same optimization site. The audit names exactly
//! this escape hatch. `build_flag_violation_v1` therefore fails closed on
//! any override in force at build time, and
//! `ModelGuidedSearchAuthorityV1::validate` refuses to mint an authority
//! when one is present, so a binary built under a forbidden flag cannot
//! certify itself no matter where it later runs.
//!
//! ## Two observation points, because one is not enough
//!
//! `option_env!` inside this crate sees only what was in rustc's own
//! environment. Flags configured in a `.cargo/config.toml` `[build]
//! rustflags` key or a `[target.<triple>] rustflags` table are applied by
//! Cargo WITHOUT passing through any such variable, so a configured
//! `-C llvm-args=-fp-contract=fast` used to pass certification untouched:
//! the crate simply could not see it. That is a real hole, not a
//! theoretical one, since a config file is the ordinary way a project or a
//! CI image sets flags.
//!
//! Cargo does export `CARGO_ENCODED_RUSTFLAGS` to BUILD SCRIPTS, with
//! config-derived flags already folded in. `build.rs` therefore reads that
//! (and `RUSTFLAGS`, `CARGO_BUILD_RUSTFLAGS`, the target-specific table,
//! `RUSTC_WRAPPER`, `RUSTC_WORKSPACE_WRAPPER`, and `CARGO_BUILD_TARGET`)
//! and re-exports each through `cargo:rustc-env=MTG_KERNEL_EFFECTIVE_*`,
//! with a `cargo:rerun-if-env-changed` for each so a changed flag
//! re-runs the capture. `EFFECTIVE_BUILD_FLAG_VARIABLES_V1` reads those.
//! `BUILD_OVERRIDE_VARIABLES_V1` keeps the crate-local `option_env!` view
//! as a second, independent observation point.
//!
//! A variable the build script did not report at all counts as a
//! violation, not as an absence of one: a certification that silently
//! degrades to "no evidence" is not a certification.
//!
//! It deliberately does NOT commit to `MTG_KERNEL_BUILD_GIT_HEAD`. The
//! authority record already carries `engine_commit` as its own separately
//! validated field, and folding the commit in here would force this
//! module's pinned literal to be re-minted on every commit to the
//! repository, which would destroy the signal the literal exists to carry.
//!
//! What it does not, and cannot, establish: the audit's finding that the
//! externally linked `tanhf` makes property 3 (no FMA contraction across
//! the complete call graph) VIOLATED applies to `forward_v1`, not to
//! `forward_search_deterministic_v1`, which routes every activation through
//! the kernel's own `tanh_f32_v1`. Cross-HOST bit identity (audit property
//! 6) remains unestablished either way, and this digest does not claim
//! otherwise: it pins WHICH build is in use, so a mismatch is detectable,
//! not that two matching builds necessarily agree bit for bit.

use crate::deterministic_math_v1::{softmax_legal_action_weights_v1, tanh_f32_v1};
use crate::model_guided_search_prior_quantization_v1::{
    prior_expansion_order_v1, puct_bonus_v1, quantize_prior_v1,
    MODEL_GUIDED_SEARCH_PRIOR_QUANTIZATION_FIXED_POINT_SCALE_BITS_V1,
    MODEL_GUIDED_SEARCH_PRIOR_QUANTIZATION_FIXED_POINT_SCALE_V1,
    MODEL_GUIDED_SEARCH_PRIOR_QUANTIZATION_SCALE_V1,
};
use crate::model_guided_search_value_quantization_v1::{
    quantize_value_v1, ModelGuidedSearchValueHeadDomainV1,
    MODEL_GUIDED_SEARCH_VALUE_QUANTIZATION_CLAMP_MAX_V1,
    MODEL_GUIDED_SEARCH_VALUE_QUANTIZATION_CLAMP_MIN_V1,
    MODEL_GUIDED_SEARCH_VALUE_QUANTIZATION_SCALE_V1,
};
use sha2::{Digest, Sha256};
use std::sync::OnceLock;

/// Frozen label for the PUCT prior-quantization contract. Distinct from
/// every other identity in this module by construction, so no two digests
/// can collide even under identical constants and probe outputs.
pub const MODEL_GUIDED_SEARCH_PRIOR_QUANTIZATION_CONTRACT_IDENTITY_V1: &str =
    "model-guided-search-prior-quantization-f32-fixed-point-hamilton-largest-remainder/v1";

/// Frozen label for the value-quantization contract.
pub const MODEL_GUIDED_SEARCH_VALUE_QUANTIZATION_CONTRACT_IDENTITY_V1: &str =
    "model-guided-search-value-quantization-unit-domain-root-perspective-round-ties-even-clamp/v1";

/// Frozen label for the deterministic-forward build identity.
pub const MODEL_GUIDED_SEARCH_FORWARD_DETERMINISM_BUILD_IDENTITY_V1: &str =
    "model-guided-search-forward-determinism-kernel-tanh-softmax-mxcsr-target-buildflags/v2";

/// The build-flag contract `docs/audits/model_guided_forward_determinism_
/// audit_v1.md` Section 6 item 2 requires: an explicit `target-feature`
/// and `RUSTFLAGS` pin matching today's default (no `+fma`, no `+avx`),
/// enforced by rejecting build-override environment variables the way
/// `examples/bench_fast_sampler.rs` already rejects them, and explicitly
/// forbidding `-C llvm-args=-fp-contract=fast` or any other
/// contraction-enabling flag by that same mechanism.
///
/// Why this is checked with `option_env!` and not `std::env::var`: the
/// flags that matter were consumed by `rustc` when this crate was
/// compiled, so the only faithful reading is the COMPILE-time one.
/// `option_env!` bakes the value that was in force during the build into
/// the binary, so a scorer built under
/// `RUSTFLAGS=-C llvm-args=-fp-contract=fast` carries that fact with it
/// and fails closed wherever it later runs. A run-time read would see the
/// operator's current shell, which says nothing about how the binary was
/// produced.
pub const MODEL_GUIDED_SEARCH_BUILD_FLAG_CONTRACT_V1: &str =
    "no RUSTFLAGS, CARGO_ENCODED_RUSTFLAGS, \
     CARGO_BUILD_RUSTFLAGS, CARGO_TARGET_<TRIPLE>_RUSTFLAGS, RUSTC, RUSTC_WRAPPER, \
     RUSTC_WORKSPACE_WRAPPER, or CARGO_BUILD_TARGET override; target features pinned to the \
     crate default (no +fma, no +avx); floating-point contraction and fast-math flags \
     (-ffast-math, -C llvm-args=-fp-contract=fast, -C llvm-args=-enable-unsafe-fp-math, \
     -Z fp-contract, -C target-feature=+fma) explicitly forbidden/v1";

/// The EFFECTIVE build-flag environment, captured by `build.rs` and
/// re-exported through `cargo:rustc-env`.
///
/// This is the layer that closes the `.cargo/config.toml` hole. Flags
/// configured in a `[build] rustflags` key or a `[target.<triple>]
/// rustflags` table are applied by Cargo to the rustc invocation without
/// ever appearing in an environment variable the compiled crate can see,
/// so a `option_env!("RUSTFLAGS")` check alone certified a build that a
/// configured `-C llvm-args=-fp-contract=fast` had already changed.
/// Cargo does set `CARGO_ENCODED_RUSTFLAGS` for BUILD SCRIPTS, and that
/// value already has the config-derived flags folded in, so `build.rs`
/// reads it there and hands it back to the crate.
///
/// `None` means the build script did not report the variable at all,
/// which is treated as a violation rather than as an absence of one: a
/// certification that silently degrades to "no evidence" is not a
/// certification. In a normal build every entry is `Some`, and empty when
/// the variable is unset.
const EFFECTIVE_BUILD_FLAG_VARIABLES_V1: &[(&str, Option<&str>)] = &[
    (
        "CARGO_ENCODED_RUSTFLAGS (effective, includes .cargo/config.toml)",
        option_env!("MTG_KERNEL_EFFECTIVE_ENCODED_RUSTFLAGS"),
    ),
    (
        "RUSTFLAGS (effective)",
        option_env!("MTG_KERNEL_EFFECTIVE_RUSTFLAGS"),
    ),
    (
        "CARGO_BUILD_RUSTFLAGS (effective)",
        option_env!("MTG_KERNEL_EFFECTIVE_BUILD_RUSTFLAGS"),
    ),
    (
        "CARGO_TARGET_<TRIPLE>_RUSTFLAGS (effective)",
        option_env!("MTG_KERNEL_EFFECTIVE_TARGET_RUSTFLAGS"),
    ),
    (
        "RUSTC_WRAPPER (effective)",
        option_env!("MTG_KERNEL_EFFECTIVE_RUSTC_WRAPPER"),
    ),
    (
        "RUSTC_WORKSPACE_WRAPPER (effective)",
        option_env!("MTG_KERNEL_EFFECTIVE_RUSTC_WORKSPACE_WRAPPER"),
    ),
    (
        "CARGO_BUILD_TARGET (effective)",
        option_env!("MTG_KERNEL_EFFECTIVE_BUILD_TARGET"),
    ),
];

/// The build-override variables visible in the CRATE's own compile-time
/// environment. Kept alongside the build-script capture above as a second,
/// independent observation point rather than replaced by it: the two see
/// different things (this one sees what rustc itself was handed; the other
/// sees what Cargo resolved), and a discrepancy between them is exactly
/// the sort of thing a certification should not have to reason about.
///
/// `RUSTC` appears here and NOT in the build-script list, because Cargo
/// always sets `RUSTC` for build scripts (to the rustc it is driving) and
/// so its presence there means nothing, whereas at crate-compile time it
/// is unset in a default build.
const BUILD_OVERRIDE_VARIABLES_V1: &[(&str, Option<&str>)] = &[
    ("RUSTFLAGS", option_env!("RUSTFLAGS")),
    (
        "CARGO_ENCODED_RUSTFLAGS",
        option_env!("CARGO_ENCODED_RUSTFLAGS"),
    ),
    (
        "CARGO_BUILD_RUSTFLAGS",
        option_env!("CARGO_BUILD_RUSTFLAGS"),
    ),
    (
        "CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS",
        option_env!("CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS"),
    ),
    (
        "CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS",
        option_env!("CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS"),
    ),
    ("RUSTC", option_env!("RUSTC")),
    ("RUSTC_WRAPPER", option_env!("RUSTC_WRAPPER")),
    (
        "RUSTC_WORKSPACE_WRAPPER",
        option_env!("RUSTC_WORKSPACE_WRAPPER"),
    ),
    ("CARGO_BUILD_TARGET", option_env!("CARGO_BUILD_TARGET")),
];

/// Flag fragments that enable floating-point contraction or otherwise
/// relax floating-point semantics. Named explicitly so a rejection says
/// WHICH forbidden flag was found rather than only that some override was
/// set, and so the prohibition is testable without an actually-overridden
/// build.
const FORBIDDEN_FLOATING_POINT_FRAGMENTS_V1: &[&str] = &[
    "fp-contract",
    "ffast-math",
    "fast-math",
    "enable-unsafe-fp-math",
    "unsafe-fp-math",
    "reassociate-fp",
    "+fma",
    "fp-model",
];

/// A build-flag contract violation: which variable, and (when it is one of
/// the explicitly forbidden floating-point flags) which fragment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BuildFlagViolationV1 {
    /// The environment variable that was set at build time.
    pub variable: &'static str,
    /// The forbidden floating-point fragment found in its value, when the
    /// value names one. `None` means the variable was merely set to
    /// something unrepresented, which is rejected all the same: an
    /// override this contract cannot interpret must never be treated as
    /// benign.
    pub forbidden_fragment: Option<&'static str>,
}

/// Returns the forbidden floating-point fragment a build-flag value names,
/// if any. Case-insensitive because `rustc` accepts the flags in either
/// case through some shells and a lowercase-only match would be a trivial
/// bypass.
pub fn forbidden_floating_point_fragment_v1(value: &str) -> Option<&'static str> {
    let lowered = value.to_ascii_lowercase();
    FORBIDDEN_FLOATING_POINT_FRAGMENTS_V1
        .iter()
        .copied()
        .find(|fragment| lowered.contains(fragment))
}

/// Classifies one captured build-flag value. Factored out so the
/// build-script capture and the crate's own compile-time capture cannot
/// drift apart, and so a test can drive it with a synthetic value that no
/// real build in this repository produces.
///
/// `reported` is `None` when the observation point produced nothing at
/// all, which is a violation in the build-script list (the script always
/// reports) and merely an absence in the `option_env!` list (an unset
/// variable is genuinely absent there).
fn classify_build_flag_value_v1(
    variable: &'static str,
    reported: Option<&str>,
    missing_is_violation: bool,
) -> Option<BuildFlagViolationV1> {
    match reported {
        None if missing_is_violation => Some(BuildFlagViolationV1 {
            variable,
            forbidden_fragment: None,
        }),
        None => None,
        // Empty is unset: Cargo and many CI shells export these names with
        // an empty value, which changes nothing about the build.
        Some("") => None,
        Some(value) => Some(BuildFlagViolationV1 {
            variable,
            forbidden_fragment: forbidden_floating_point_fragment_v1(value),
        }),
    }
}

/// Fails closed on any build-override that was in force when this crate
/// was compiled, from either observation point. Returns the FIRST
/// violation in the contract's declared order, so the error is
/// deterministic.
///
/// The build-script capture is checked first because it is the one that
/// sees `.cargo/config.toml`-derived flags, which is the case a
/// crate-local `option_env!` cannot see at all.
pub fn build_flag_violation_v1() -> Option<BuildFlagViolationV1> {
    for &(variable, value) in EFFECTIVE_BUILD_FLAG_VARIABLES_V1 {
        if let Some(violation) = classify_build_flag_value_v1(variable, value, true) {
            return Some(violation);
        }
    }
    for &(variable, value) in BUILD_OVERRIDE_VARIABLES_V1 {
        if let Some(violation) = classify_build_flag_value_v1(variable, value, false) {
            return Some(violation);
        }
    }
    None
}

/// The wrapper's pre-registered value-head input domain, folded into the
/// value-quantization contract digest so the mapping is, as the design
/// requires, "pinned in the authority record" even though the record has no
/// separate domain field.
///
/// DEVIATION FROM THE SKETCH, reported rather than taken silently.
/// `LEAD_TEST_TIME_SEARCH_DESIGN_SKETCH_V2.md` Section 3 item 3 and Section
/// 4 both pre-register this as `Tanh`. That value is not implementable
/// against this checkpoint architecture: the Net8 value head is
/// `linear_rows_v1(&self.value_second, &value_hidden, 1)[0]`, a RAW LINEAR
/// output layer (`native_policy_value_net_v1.rs`), so its range is
/// unbounded. `ModelGuidedSearchValueHeadDomainV1::Tanh` treats any
/// `v_raw` outside `[-1.0, 1.0]` as a hard `RawValueOutOfAnalyticDomain`
/// error, correctly, because for a genuinely tanh-activated head such a
/// value could only mean the registered activation was wrong. Under a raw
/// linear head an out-of-range value is instead an ordinary, expected tail:
/// a network trained to regress +/-1 terminal returns routinely overshoots
/// slightly. Pinning `Tanh` would therefore make the wrapper abort live
/// episodes on ordinary model outputs.
///
/// This is not an inference from reading the layer definition alone.
/// `model_guided_search_core_v1`'s own test suite reaches the same
/// conclusion independently and in the other direction: every one of its
/// tests that drives the REAL forward evaluator
/// (`ModelGuidedSearchRealForwardValueEvaluatorV1`) uses
/// `Calibrated { lower: -8.0, upper: 8.0 }`, while only the mock-evaluator
/// tests use `Tanh`. That module's authors had to widen the domain to make
/// the real net's outputs admissible, before this sketch existed.
///
/// `Calibrated { lower: -1.0, upper: 1.0 }` is pinned instead. It applies
/// the IDENTICAL affine map on `[-1.0, 1.0]` (`2*(v+1)/2 - 1 == v`, proven
/// by `calibrated_unit_domain_matches_tanh_domain_exactly_v1` below), so
/// the pre-registered MAPPING is preserved exactly everywhere the
/// pre-registered domain was defined; outside it, the contract's own
/// saturating `+/-9,000` clamp applies instead of a hard error. This is a
/// substantive change to a pre-registered constant and needs the owner's
/// ratification before any CP7 panel. The alternative resolution -- keep
/// `Tanh` and pre-register an empirical per-checkpoint calibration, which
/// is what `model_guided_search_value_quantization_v1`'s own module docs
/// anticipate for "the population-v2 Net8 architecture" -- is a strictly
/// better answer that S1 can measure and this diff cannot: it needs a
/// value-head output distribution from a real checkpoint, which S0 (no
/// games) does not have.
pub const MODEL_GUIDED_SEARCH_WRAPPER_VALUE_DOMAIN_V1: ModelGuidedSearchValueHeadDomainV1 =
    ModelGuidedSearchValueHeadDomainV1::Calibrated {
        lower: -1.0,
        upper: 1.0,
    };

/// Pinned digest of the PUCT prior-quantization contract. Equality with
/// [`prior_quantization_contract_digest_v1`] is asserted by this module's
/// own test; `ModelGuidedSearchAuthorityV1::validate` compares a record's
/// `puct_prior_quantization_contract_sha256` against this literal.
pub const MODEL_GUIDED_SEARCH_PRIOR_QUANTIZATION_CONTRACT_SHA256_V1: &str =
    "cd0b3bb345e7f413f38b7cc6cbad3571f0166d32a0eab009bfd0e7185d9bb20f";

/// Pinned digest of the value-quantization contract, including the
/// wrapper's pre-registered value-head domain.
pub const MODEL_GUIDED_SEARCH_VALUE_QUANTIZATION_CONTRACT_SHA256_V1: &str =
    "d06c8b302df558254646881bac682c64a6151c9d582cc7d30141e04fabc43663";

/// Pinned digest of the deterministic-forward build identity.
pub const MODEL_GUIDED_SEARCH_FORWARD_DETERMINISM_BUILD_SHA256_V1: &str =
    "093e978cacef906db300fd683349ed66f676d500c32ca5c6fb8d419b990b97d3";

/// The frozen prior-contract probe battery: masked, not-necessarily-
/// normalized legal-action weight vectors. Chosen for coverage of the
/// apportionment's own edges rather than for volume: a singleton, an exact
/// tie (residual distribution by ascending index), an unrepresentable
/// three-way split, a dominant-action vector, and a vector containing an
/// exact zero alongside nonzero weights.
const PRIOR_WEIGHT_PROBES_V1: &[&[f32]] = &[
    &[1.0],
    &[0.5, 0.5],
    &[1.0, 1.0, 1.0],
    &[0.999_999, 0.000_001],
    &[0.0, 0.25, 0.75],
    &[0.1, 0.2, 0.3, 0.4],
    &[1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
];

/// The frozen `puct_bonus_v1` probe battery, as `(bonus, p_int)` pairs:
/// zero prior, whole scale, the design's own hand-worked example
/// (`1_414 * 250_000 / 1_000_000 == 353`), and a large bonus that exercises
/// the `u128` intermediate.
const PUCT_BONUS_PROBES_V1: &[(u64, u32)] = &[
    (0, 0),
    (1_414, 250_000),
    (1_414, 1_000_000),
    (1, 1),
    (u32::MAX as u64, 999_999),
];

/// The frozen expansion-order probe battery: descending prior with ties
/// broken by ascending flat-action index.
const PRIOR_EXPANSION_ORDER_PROBES_V1: &[&[u32]] = &[
    &[100_000, 500_000, 500_000, 100_000, 200_000],
    &[1_000_000],
    &[0, 0, 0],
    &[333_334, 333_333, 333_333],
];

/// The frozen value-contract probe battery: `(domain, v_raw,
/// leaf_acting_player_is_root)`. Covers both analytic domains at their
/// exact endpoints and interior, the wrapper's own pinned calibrated
/// domain, the perspective flip in both directions, a round-half-to-even
/// tie, and both saturation tails.
fn value_probes_v1() -> Vec<(ModelGuidedSearchValueHeadDomainV1, f32, bool)> {
    use ModelGuidedSearchValueHeadDomainV1 as Domain;
    vec![
        (Domain::Tanh, 0.0, true),
        (Domain::Tanh, 1.0, true),
        (Domain::Tanh, -1.0, true),
        (Domain::Tanh, 0.5, true),
        (Domain::Tanh, 0.5, false),
        (Domain::SigmoidFamily, 0.0, true),
        (Domain::SigmoidFamily, 0.5, true),
        (Domain::SigmoidFamily, 1.0, false),
        (MODEL_GUIDED_SEARCH_WRAPPER_VALUE_DOMAIN_V1, 0.0, true),
        (MODEL_GUIDED_SEARCH_WRAPPER_VALUE_DOMAIN_V1, 0.5, true),
        (MODEL_GUIDED_SEARCH_WRAPPER_VALUE_DOMAIN_V1, 0.5, false),
        (MODEL_GUIDED_SEARCH_WRAPPER_VALUE_DOMAIN_V1, 2.5, true),
        (MODEL_GUIDED_SEARCH_WRAPPER_VALUE_DOMAIN_V1, -2.5, true),
        (
            Domain::Calibrated {
                lower: -3.5,
                upper: 2.25,
            },
            0.125,
            true,
        ),
    ]
}

/// The frozen `tanh_f32_v1` probe battery, by input bit pattern: zero,
/// negative zero, the small-linear passthrough threshold, a subnormal, one
/// and minus one, the saturation threshold, and a large magnitude.
const TANH_PROBE_BITS_V1: &[u32] = &[
    0x0000_0000,
    0x8000_0000,
    0x3980_0000,
    0x0000_0001,
    0x3f80_0000,
    0xbf80_0000,
    0x4110_2cb4,
    0x4180_0000,
    0x40a0_0000,
];

/// The frozen softmax probe battery. `softmax_legal_action_weights_v1`
/// returns UNNORMALIZED weights, so these probe the exponential itself and
/// the max-subtraction, not a normalization.
const SOFTMAX_PROBES_V1: &[&[f32]] = &[
    &[0.0],
    &[0.0, 0.0],
    &[10.0, 0.0, 0.0, 0.0],
    &[-1.5, 2.25, 0.0],
    &[-200.0, 0.0],
];

fn update_str_v1(hasher: &mut Sha256, value: &str) {
    hasher.update((value.len() as u64).to_le_bytes());
    hasher.update(value.as_bytes());
}

fn update_f32_slice_v1(hasher: &mut Sha256, values: &[f32]) {
    hasher.update((values.len() as u64).to_le_bytes());
    for value in values {
        hasher.update(value.to_bits().to_le_bytes());
    }
}

/// Encodes a value-head domain into the digest by tag plus, for
/// `Calibrated`, both bounds' exact bit patterns. The tags are literal
/// bytes rather than the enum's discriminant so that reordering the enum's
/// variants cannot silently change a digest.
fn update_value_domain_v1(hasher: &mut Sha256, domain: &ModelGuidedSearchValueHeadDomainV1) {
    match domain {
        ModelGuidedSearchValueHeadDomainV1::Tanh => update_str_v1(hasher, "tanh"),
        ModelGuidedSearchValueHeadDomainV1::SigmoidFamily => {
            update_str_v1(hasher, "sigmoid_family");
        }
        ModelGuidedSearchValueHeadDomainV1::Calibrated { lower, upper } => {
            update_str_v1(hasher, "calibrated");
            hasher.update(lower.to_bits().to_le_bytes());
            hasher.update(upper.to_bits().to_le_bytes());
        }
    }
}

/// Computes the PUCT prior-quantization contract digest from live content.
/// See the module docs for exactly what is committed and why a source-file
/// digest was rejected.
pub fn prior_quantization_contract_digest_v1() -> [u8; 32] {
    static DIGEST: OnceLock<[u8; 32]> = OnceLock::new();
    *DIGEST.get_or_init(|| {
        let mut hasher = Sha256::new();
        update_str_v1(
            &mut hasher,
            MODEL_GUIDED_SEARCH_PRIOR_QUANTIZATION_CONTRACT_IDENTITY_V1,
        );
        hasher.update(MODEL_GUIDED_SEARCH_PRIOR_QUANTIZATION_SCALE_V1.to_le_bytes());
        hasher
            .update(MODEL_GUIDED_SEARCH_PRIOR_QUANTIZATION_FIXED_POINT_SCALE_BITS_V1.to_le_bytes());
        hasher.update(
            MODEL_GUIDED_SEARCH_PRIOR_QUANTIZATION_FIXED_POINT_SCALE_V1
                .to_bits()
                .to_le_bytes(),
        );
        update_str_v1(&mut hasher, "quantize_prior_v1");
        for probe in PRIOR_WEIGHT_PROBES_V1 {
            update_f32_slice_v1(&mut hasher, probe);
            match quantize_prior_v1(probe) {
                Ok(prior) => {
                    update_str_v1(&mut hasher, "ok");
                    hasher.update((prior.len() as u64).to_le_bytes());
                    for value in prior {
                        hasher.update(value.to_le_bytes());
                    }
                }
                // A probe that errors is still contract content: the digest
                // commits to WHICH error, so a contract that started
                // accepting a previously rejected input moves the digest.
                Err(error) => {
                    update_str_v1(&mut hasher, "err");
                    update_str_v1(&mut hasher, &format!("{error:?}"));
                }
            }
        }
        update_str_v1(&mut hasher, "puct_bonus_v1");
        for &(bonus, p_int) in PUCT_BONUS_PROBES_V1 {
            hasher.update(bonus.to_le_bytes());
            hasher.update(p_int.to_le_bytes());
            match puct_bonus_v1(bonus, p_int) {
                Ok(value) => {
                    update_str_v1(&mut hasher, "ok");
                    hasher.update(value.to_le_bytes());
                }
                Err(error) => {
                    update_str_v1(&mut hasher, "err");
                    update_str_v1(&mut hasher, &format!("{error:?}"));
                }
            }
        }
        update_str_v1(&mut hasher, "prior_expansion_order_v1");
        for probe in PRIOR_EXPANSION_ORDER_PROBES_V1 {
            hasher.update((probe.len() as u64).to_le_bytes());
            for value in *probe {
                hasher.update(value.to_le_bytes());
            }
            for index in prior_expansion_order_v1(probe) {
                hasher.update((index as u64).to_le_bytes());
            }
        }
        hasher.finalize().into()
    })
}

/// Computes the value-quantization contract digest from live content,
/// including the wrapper's pre-registered
/// [`MODEL_GUIDED_SEARCH_WRAPPER_VALUE_DOMAIN_V1`].
pub fn value_quantization_contract_digest_v1() -> [u8; 32] {
    static DIGEST: OnceLock<[u8; 32]> = OnceLock::new();
    *DIGEST.get_or_init(|| {
        let mut hasher = Sha256::new();
        update_str_v1(
            &mut hasher,
            MODEL_GUIDED_SEARCH_VALUE_QUANTIZATION_CONTRACT_IDENTITY_V1,
        );
        hasher.update(
            MODEL_GUIDED_SEARCH_VALUE_QUANTIZATION_SCALE_V1
                .to_bits()
                .to_le_bytes(),
        );
        hasher.update(MODEL_GUIDED_SEARCH_VALUE_QUANTIZATION_CLAMP_MIN_V1.to_le_bytes());
        hasher.update(MODEL_GUIDED_SEARCH_VALUE_QUANTIZATION_CLAMP_MAX_V1.to_le_bytes());
        update_str_v1(&mut hasher, "wrapper_value_domain");
        update_value_domain_v1(&mut hasher, &MODEL_GUIDED_SEARCH_WRAPPER_VALUE_DOMAIN_V1);
        update_str_v1(&mut hasher, "quantize_value_v1");
        for (domain, v_raw, leaf_acting_player_is_root) in value_probes_v1() {
            update_value_domain_v1(&mut hasher, &domain);
            hasher.update(v_raw.to_bits().to_le_bytes());
            hasher.update([u8::from(leaf_acting_player_is_root)]);
            match quantize_value_v1(&domain, v_raw, leaf_acting_player_is_root) {
                Ok(value) => {
                    update_str_v1(&mut hasher, "ok");
                    hasher.update(value.to_le_bytes());
                }
                Err(error) => {
                    update_str_v1(&mut hasher, "err");
                    update_str_v1(&mut hasher, &format!("{error:?}"));
                }
            }
        }
        hasher.finalize().into()
    })
}

/// Computes the deterministic-forward build identity from live content.
/// See the module docs for the four things this commits to and for the one
/// thing (`MTG_KERNEL_BUILD_GIT_HEAD`) it deliberately excludes.
pub fn forward_determinism_build_digest_v1() -> [u8; 32] {
    static DIGEST: OnceLock<[u8; 32]> = OnceLock::new();
    *DIGEST.get_or_init(|| {
        let mut hasher = Sha256::new();
        update_str_v1(
            &mut hasher,
            MODEL_GUIDED_SEARCH_FORWARD_DETERMINISM_BUILD_IDENTITY_V1,
        );
        update_str_v1(&mut hasher, "tanh_f32_v1");
        for &bits in TANH_PROBE_BITS_V1 {
            hasher.update(bits.to_le_bytes());
            hasher.update(tanh_f32_v1(f32::from_bits(bits)).to_bits().to_le_bytes());
        }
        update_str_v1(&mut hasher, "softmax_legal_action_weights_v1");
        for probe in SOFTMAX_PROBES_V1 {
            update_f32_slice_v1(&mut hasher, probe);
            update_f32_slice_v1(&mut hasher, &softmax_legal_action_weights_v1(probe));
        }
        // The pinned floating-point control state, spelled out rather than
        // referenced, so a change to which bits are pinned moves the digest
        // even though the pinning code itself lives in another module.
        update_str_v1(&mut hasher, "mxcsr:ftz=0,daz=0,rc=round-to-nearest-even");
        update_str_v1(&mut hasher, "target_arch");
        update_str_v1(&mut hasher, std::env::consts::ARCH);
        hasher.update((usize::BITS).to_le_bytes());
        update_str_v1(&mut hasher, "target_feature_fma");
        hasher.update([u8::from(cfg!(target_feature = "fma"))]);
        update_str_v1(&mut hasher, "target_feature_avx");
        hasher.update([u8::from(cfg!(target_feature = "avx"))]);
        // The build-flag contract itself, plus the exact list of override
        // variables it rejects and the exact list of forbidden
        // floating-point fragments. Binding the LISTS, not just the prose,
        // is what makes the identity move when someone quietly drops a
        // variable or a fragment from the contract: the digest is then the
        // audit trail for the contract's own coverage, not only for the
        // arithmetic it protects.
        update_str_v1(&mut hasher, MODEL_GUIDED_SEARCH_BUILD_FLAG_CONTRACT_V1);
        update_str_v1(&mut hasher, "build_override_variables");
        for &(variable, _) in BUILD_OVERRIDE_VARIABLES_V1 {
            update_str_v1(&mut hasher, variable);
        }
        // The build-script observation point is part of the contract's
        // coverage, so removing it (and with it the only way to see
        // `.cargo/config.toml` flags) moves the identity.
        update_str_v1(&mut hasher, "effective_build_flag_variables");
        for &(variable, _) in EFFECTIVE_BUILD_FLAG_VARIABLES_V1 {
            update_str_v1(&mut hasher, variable);
        }
        update_str_v1(&mut hasher, "forbidden_floating_point_fragments");
        for fragment in FORBIDDEN_FLOATING_POINT_FRAGMENTS_V1 {
            update_str_v1(&mut hasher, fragment);
        }
        hasher.finalize().into()
    })
}

/// Lower-hex of a raw 32-byte digest. Local rather than imported from
/// `native_training_store_digest_v1` so this module has no dependency on
/// the Store layer, which it is otherwise entirely independent of.
pub fn lower_hex_digest_v1(digest: [u8; 32]) -> String {
    let mut out = String::with_capacity(64);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinned_prior_contract_digest_matches_live_content_v1() {
        assert_eq!(
            lower_hex_digest_v1(prior_quantization_contract_digest_v1()),
            MODEL_GUIDED_SEARCH_PRIOR_QUANTIZATION_CONTRACT_SHA256_V1,
            "the prior-quantization contract's behavior changed; re-pin the literal deliberately"
        );
    }

    #[test]
    fn pinned_value_contract_digest_matches_live_content_v1() {
        assert_eq!(
            lower_hex_digest_v1(value_quantization_contract_digest_v1()),
            MODEL_GUIDED_SEARCH_VALUE_QUANTIZATION_CONTRACT_SHA256_V1,
            "the value-quantization contract's behavior or pinned domain changed; re-pin deliberately"
        );
    }

    /// The audit (Section 6 item 2) requires rejecting
    /// contraction-enabling overrides. The classifier is tested directly
    /// because the only other way to exercise it is to actually build the
    /// crate under a forbidden flag, which no test can do from inside the
    /// build it would need to change.
    #[test]
    fn contraction_enabling_flags_are_named_and_rejected_v1() {
        for (value, expected) in [
            ("-C llvm-args=-fp-contract=fast", Some("fp-contract")),
            ("-Zfp-contract=fast", Some("fp-contract")),
            ("-C llvm-args=-ffast-math", Some("ffast-math")),
            (
                "-C llvm-args=-enable-unsafe-fp-math",
                Some("enable-unsafe-fp-math"),
            ),
            ("-C target-feature=+fma", Some("+fma")),
            ("-C target-feature=+avx2,+fma", Some("+fma")),
            ("-C llvm-args=-fp-model=fast", Some("fp-model")),
            ("-C opt-level=3", None),
            ("-C debuginfo=0", None),
            ("", None),
        ] {
            let found = forbidden_floating_point_fragment_v1(value);
            assert_eq!(
                found.is_some(),
                expected.is_some(),
                "{value:?} classification"
            );
        }
        // Case folding is not a bypass.
        assert!(forbidden_floating_point_fragment_v1("-C LLVM-ARGS=-FP-CONTRACT=FAST").is_some());
        // An override this contract cannot interpret is still rejected:
        // "not a known-bad flag" must never read as "safe".
        assert!(forbidden_floating_point_fragment_v1("-C some-future-flag=yes").is_none());
    }

    /// This build must itself be clean, which is also what makes every
    /// other test in the crate a valid witness for the pinned digests.
    #[test]
    fn this_build_carries_no_forbidden_build_flag_override_v1() {
        assert_eq!(
            build_flag_violation_v1(),
            None,
            "this crate was compiled with a build-override environment variable set; \
             the pinned forward-determinism identity cannot describe its arithmetic"
        );
    }

    /// The build script must actually have reported, or the whole
    /// `.cargo/config.toml` defence is inert. A missing report is a
    /// violation, not an absence of one.
    #[test]
    fn the_build_script_reported_the_effective_flag_environment_v1() {
        for &(variable, value) in EFFECTIVE_BUILD_FLAG_VARIABLES_V1 {
            assert!(
                value.is_some(),
                "build.rs did not re-export {variable}; the config.toml capture is not running"
            );
        }
        // Missing is a violation; present-and-empty is clean.
        assert_eq!(
            classify_build_flag_value_v1("probe", None, true),
            Some(BuildFlagViolationV1 {
                variable: "probe",
                forbidden_fragment: None,
            })
        );
        assert_eq!(classify_build_flag_value_v1("probe", Some(""), true), None);
        assert_eq!(classify_build_flag_value_v1("probe", None, false), None);
    }

    /// A `.cargo/config.toml`-configured contraction flag reaches the
    /// crate as an ENCODED rustflags string (unit-separated, flattened to
    /// spaces by `build.rs`). Driving the classifier with that synthetic
    /// shape is the only way to exercise the path without rebuilding the
    /// crate under a poisoned config, and it proves the flattening did not
    /// destroy the substring the scan depends on.
    #[test]
    fn a_synthetic_encoded_rustflags_string_with_a_contraction_flag_is_detected_v1() {
        // What Cargo hands a build script for
        // `rustflags = ["-C", "llvm-args=-fp-contract=fast"]`, after
        // build.rs maps the \x1f separators to spaces.
        let encoded = "-C llvm-args=-fp-contract=fast";
        let violation = classify_build_flag_value_v1(
            "CARGO_ENCODED_RUSTFLAGS (effective, includes .cargo/config.toml)",
            Some(encoded),
            true,
        )
        .expect("a configured contraction flag must be a violation");
        assert_eq!(violation.forbidden_fragment, Some("fp-contract"));

        // The raw unit-separated form must not smuggle a flag past the
        // scan either, in case the flattening is ever changed.
        assert_eq!(
            forbidden_floating_point_fragment_v1("-C\u{1f}llvm-args=-fp-contract=fast"),
            Some("fp-contract")
        );
        // A multi-flag config where only the last entry is forbidden.
        assert_eq!(
            forbidden_floating_point_fragment_v1("-C opt-level=3 -C target-feature=+fma"),
            Some("+fma")
        );
        // A configured flag set with nothing forbidden in it is STILL a
        // violation, just an unnamed one. This is deliberate and is the
        // same rule the crate-local capture already applied: the contract
        // pins the default build, so any configured rustflags value at all
        // means the arithmetic was produced under conditions the pinned
        // identity does not describe. "Not a known-bad flag" must never
        // read as "safe".
        assert_eq!(
            classify_build_flag_value_v1("probe", Some("-C opt-level=3 -C debuginfo=2"), true),
            Some(BuildFlagViolationV1 {
                variable: "probe",
                forbidden_fragment: None,
            })
        );
        // Only genuinely empty is clean.
        assert_eq!(classify_build_flag_value_v1("probe", Some(""), true), None);
    }

    /// The contract's coverage is part of the identity: dropping a
    /// variable or a fragment must move the pinned digest rather than
    /// silently widen the escape hatch.
    #[test]
    fn build_flag_contract_coverage_is_bound_into_the_identity_v1() {
        for required in [
            "RUSTFLAGS",
            "CARGO_ENCODED_RUSTFLAGS",
            "CARGO_BUILD_RUSTFLAGS",
            "RUSTC_WRAPPER",
            "CARGO_BUILD_TARGET",
        ] {
            assert!(
                BUILD_OVERRIDE_VARIABLES_V1
                    .iter()
                    .any(|(name, _)| *name == required),
                "the audit's contract names {required}"
            );
        }
        assert!(BUILD_OVERRIDE_VARIABLES_V1
            .iter()
            .any(|(name, _)| name.starts_with("CARGO_TARGET_") && name.ends_with("_RUSTFLAGS")));
        for required in ["fp-contract", "ffast-math", "+fma"] {
            assert!(
                FORBIDDEN_FLOATING_POINT_FRAGMENTS_V1.contains(&required),
                "the audit's contract forbids {required}"
            );
        }
        // The identity label moved when the contract was added, so a
        // reader cannot confuse a v1-identity record with a v2 one.
        assert!(
            MODEL_GUIDED_SEARCH_FORWARD_DETERMINISM_BUILD_IDENTITY_V1.ends_with("buildflags/v2")
        );
    }

    #[test]
    fn pinned_forward_determinism_build_digest_matches_live_content_v1() {
        assert_eq!(
            lower_hex_digest_v1(forward_determinism_build_digest_v1()),
            MODEL_GUIDED_SEARCH_FORWARD_DETERMINISM_BUILD_SHA256_V1,
            "the deterministic-forward build identity changed (tanh, softmax, MXCSR contract, or target features)"
        );
    }

    /// The three digests are mutually distinct. A copy-paste that pointed
    /// two authority fields at the same contract would otherwise validate.
    #[test]
    fn the_three_contract_digests_are_mutually_distinct_v1() {
        let prior = prior_quantization_contract_digest_v1();
        let value = value_quantization_contract_digest_v1();
        let build = forward_determinism_build_digest_v1();
        assert_ne!(prior, value);
        assert_ne!(prior, build);
        assert_ne!(value, build);
    }

    /// The digests are content-BOUND, not merely content-derived: perturbing
    /// one probe's input moves the digest. Recomputed inline here (the
    /// public functions memoize, so this cannot call them twice with
    /// different content).
    #[test]
    fn a_perturbed_probe_moves_the_prior_digest_v1() {
        let mut hasher = Sha256::new();
        update_str_v1(
            &mut hasher,
            MODEL_GUIDED_SEARCH_PRIOR_QUANTIZATION_CONTRACT_IDENTITY_V1,
        );
        // Same identity, one different pinned constant.
        hasher.update((MODEL_GUIDED_SEARCH_PRIOR_QUANTIZATION_SCALE_V1 + 1).to_le_bytes());
        let perturbed: [u8; 32] = hasher.finalize().into();
        assert_ne!(perturbed, prior_quantization_contract_digest_v1());
    }

    /// The pre-registered mapping is preserved exactly. The sketch
    /// pre-registers `Tanh`; this build pins
    /// `Calibrated { lower: -1.0, upper: 1.0 }` (see that constant's own
    /// doc comment for the full reason). On `[-1.0, 1.0]`, the domain the
    /// pre-registered constant actually defines, the two must agree on
    /// every quantized output, in both perspectives.
    #[test]
    fn calibrated_unit_domain_matches_tanh_domain_exactly_v1() {
        let probes = [
            -1.0_f32, -0.75, -0.5, -0.25, -0.000_001, 0.0, 0.000_001, 0.25, 0.5, 0.75, 1.0,
        ];
        for v_raw in probes {
            for is_root in [true, false] {
                let tanh_side =
                    quantize_value_v1(&ModelGuidedSearchValueHeadDomainV1::Tanh, v_raw, is_root)
                        .expect("Tanh domain accepts [-1, 1]");
                let calibrated_side =
                    quantize_value_v1(&MODEL_GUIDED_SEARCH_WRAPPER_VALUE_DOMAIN_V1, v_raw, is_root)
                        .expect("the pinned calibrated unit domain accepts [-1, 1]");
                assert_eq!(
                    tanh_side, calibrated_side,
                    "mapping diverged at v_raw={v_raw} is_root={is_root}"
                );
            }
        }
    }

    /// Outside the pre-registered domain the two differ exactly as
    /// documented: `Tanh` is a hard error, the pinned domain saturates at
    /// the contract's own clamp. This is the whole substance of the
    /// deviation, asserted rather than only described.
    #[test]
    fn outside_the_unit_domain_tanh_errors_and_the_pinned_domain_saturates_v1() {
        for (v_raw, expected) in [
            (1.5_f32, MODEL_GUIDED_SEARCH_VALUE_QUANTIZATION_CLAMP_MAX_V1),
            (
                -1.5_f32,
                MODEL_GUIDED_SEARCH_VALUE_QUANTIZATION_CLAMP_MIN_V1,
            ),
        ] {
            assert!(
                quantize_value_v1(&ModelGuidedSearchValueHeadDomainV1::Tanh, v_raw, true).is_err(),
                "the pre-registered Tanh domain must reject {v_raw}"
            );
            assert_eq!(
                quantize_value_v1(&MODEL_GUIDED_SEARCH_WRAPPER_VALUE_DOMAIN_V1, v_raw, true),
                Ok(expected)
            );
        }
    }
}
