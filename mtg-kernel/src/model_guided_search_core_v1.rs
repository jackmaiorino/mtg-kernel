//! Model-guided searcher: the core search-loop change.
//!
//! Authority: `CLAUDE-MODEL-GUIDED-SEARCHER-DESIGN-V1.md` Sections 1.2
//! ("New: the net's policy as a PUCT-style expansion prior"), 1.3 ("New: the
//! net's value at leaf cutoffs, replacing v1's static evaluator"), 1.4 (the
//! "third dispatch shape"), 5.3 item 5 ("Core search-loop change: prior-
//! ordered expansion and value-head leaf dispatch (Sections 1.2-1.3),
//! replacing v1's ascending-index expansion and static evaluator on THIS
//! DESIGN'S OWN ALGORITHM PATH ONLY, v1's own path is untouched, Section
//! 1.1"), and 5.3 item 6's own live-session encoder bridge sub-scope
//! ("item 6a" here), sequenced, per item 6's own text, "to extend v1's own
//! stage-2 dispatch surface rather than duplicate it."
//!
//! Sequencing ruling (collab `CLAUDE #252`, binding): this diff wires the
//! search loop to the already-merged quantization contracts
//! (`model_guided_search_prior_quantization_v1`,
//! `model_guided_search_value_quantization_v1`) through an evaluator TRAIT
//! seam ([`ModelGuidedSearchLeafEvaluatorV1`]) with a deterministic MOCK
//! implementation ([`MockLeafEvaluatorV1`]) for tests. The REAL forward
//! (`NativePolicyValueNetV1::forward_search_deterministic_v1`, already on
//! `main`) now also has a full trait implementation
//! ([`ModelGuidedSearchRealForwardValueEvaluatorV1`], item 6a, see "Item 6a:
//! the live-session encoder bridge" below for the encode-path map). What
//! remains unwired by this diff is any launcher, CLI, panel, script, or
//! eval-path CONSUMER of that evaluator (native evaluation, population
//! selection, analyzer, scorer-bridge integration): none of those call it
//! yet, matching item 6's own broader stage-2 scope, of which this is one
//! piece. `kernel_native_search_opponent_v1` (v1) is not modified in
//! behavior anywhere by this module; v1's own tests are re-verified green,
//! unchanged, alongside this module's own suite.
//!
//! # Sharing architecture against v1 (Section 1.1: inherit verbatim)
//!
//! Section 1.1 lists tree mechanics, seed domains, redetermination, and
//! depth caps as unchanged "byte-for-byte, in behavior and in the exact
//! constants that define them." This module honors that by REUSE, not
//! re-derivation, wherever the underlying function is pure and has no new
//! data to carry:
//!
//! - [`crate::kernel_native_search_opponent_v1::search_node_key_v1`][]: the
//!   tree-key computation. Called directly, unchanged.
//! - [`crate::kernel_native_search_opponent_v1::derive_simulation_seed_v1`][]:
//!   the seed-domain-separation formula. Called directly with this design's
//!   own [`ModelGuidedSearchAuthorityV1::digest`] in place of v1's digest;
//!   the seed-domain STRING is the identical constant either way
//!   (`model_guided_search_authority_v1` already requires this by
//!   `validate`).
//! - [`crate::kernel_native_search_opponent_v1::integer_ucb_bonus_v1`][]: v1's
//!   exact UCB bonus core. Called directly, then multiplied by the cached
//!   PUCT prior fraction via
//!   [`crate::model_guided_search_prior_quantization_v1::puct_bonus_v1`]
//!   (Section 1.2's own formula, `bonus_PUCT(a) = floor(bonus(a) * P_int(a) /
//!   1,000,000)`), exactly as specified: "This formula is fixed by this
//!   design... Every other term (the isqrt/ilog2 core...) is v1's,
//!   untouched."
//! - [`crate::kernel_native_search_opponent_v1::natural_terminal_value_v1`][]:
//!   v1's exact terminal-constant scoring (+10,000/0/-10,000). Called
//!   directly for `TerminalClassificationV1::Natural`.
//! - [`crate::kernel_native_search_opponent_v1::player_id_v1`][]: the
//!   `PlayerSeatV1` -> `PlayerId` mapping. Called directly.
//! - [`crate::kernel_native_search_opponent_v1::select_final_root_action_v1`][]:
//!   v1's final-selection rule (most visits, then higher mean, then lower
//!   index), unchanged by this design. Its signature was narrowed from
//!   `&SearchNodeV1` to `&[SearchActionStatV1]` (v1's own call site updated
//!   to `select_final_root_action_v1(&root.actions)`, behavior identical) so
//!   both algorithms' node types -- v1's `SearchNodeV1` and this module's own
//!   [`ModelGuidedSearchNodeV1`], which additionally carries a cached prior
//!   array v1's node has no field for -- can call the identical function.
//! - [`crate::kernel_native_search_opponent_v1::SearchActionStatV1`][]: v1's
//!   own action-statistics record (`visits`, `value_sum`, `child_nodes`,
//!   `mean()`) is reused DIRECTLY as the element type of this module's own
//!   node's `actions` array, not reimplemented. `model_guided_search_core_v1`
//!   adds only a parallel `prior: Vec<u32>` array alongside it.
//! - [`crate::kernel_native_search_opponent_v1::KernelNativeSearchActionStatV1`]
//!   (the public, serializable per-action summary v1's decision record uses)
//!   is reused directly as this module's own decision record's
//!   `root_action_stats` element type.
//! - `FastActorSessionV1::kernel_search_redeterminized_clone_v1` (already
//!   `pub(crate)` on `rl_session.rs`, no change needed): the per-simulation
//!   redetermination boundary itself. Called directly, identically to v1.
//!
//! Every one of these visibility/signature changes to v1's own file is
//! accompanied by a doc comment there citing this design and is proven
//! behavior-preserving by re-running v1's own `#[cfg(test)]` module
//! unchanged (see this crate's CI and this change's own commit history);
//! v1's tests were not edited.
//!
//! What is genuinely NEW (not shared, because v1 has no equivalent): the
//! node/tree types carrying a cached prior
//! ([`ModelGuidedSearchNodeV1`]/`ModelGuidedSearchTreeV1`), the
//! prior-ordered expansion and PUCT selection function
//! (`select_tree_action_puct_v1`), the evaluator seam itself, and the
//! per-simulation traversal loop (`run_simulation_puct_v1`) that dispatches
//! to the value head at Section 1.3's sites instead of v1's static
//! evaluator. The traversal loop's CONTROL FLOW (which branch fires when) is
//! a deliberate line-for-line mirror of
//! `kernel_native_search_opponent_v1::run_simulation_v1`, so a reviewer can
//! diff the two side by side; only the expansion-order/selection call and the
//! leaf-value production are swapped.
//!
//! # The evaluator seam
//!
//! [`ModelGuidedSearchLeafEvaluatorV1::evaluate_leaf_v1`] takes a tree key
//! (the same `[u8; 32]` `search_node_key_v1` already computes), a legal-action
//! count, and a [`ModelGuidedSearchLeafSiteV1`] tag, and returns raw,
//! not-yet-quantized net outputs
//! ([`ModelGuidedSearchLeafForwardV1`]: masked-but-unrenormalized
//! per-legal-action policy weights in `[0.0, 1.0]`, and a raw value scalar
//! from the leaf's own acting player's perspective). The search loop itself
//! -- not the evaluator -- owns turning those raw outputs into this design's
//! quantized contract values, by calling
//! [`crate::model_guided_search_prior_quantization_v1::quantize_prior_v1`]
//! and
//! [`crate::model_guided_search_value_quantization_v1::quantize_value_v1`]
//! directly. This mirrors the quantization modules' own documented scope
//! boundary verbatim: `model_guided_search_prior_quantization_v1`'s docs
//! state plainly that evaluating the policy head and performing legal-action
//! masking "remain the search-loop wiring's job (design item 5)", and
//! `model_guided_search_value_quantization_v1`'s docs state the same for
//! "the perspective determination itself... all of that is search-loop
//! wiring (design item 5)."
//!
//! # Resolved design point: the root's own prior
//!
//! Section 1.2's formula is used at every PUCT selection step, including
//! selection over the ROOT's own children -- but the root pre-exists the
//! simulation loop (`SearchTreeV1::new` in v1; `ModelGuidedSearchTreeV1::new`
//! here), so it is never itself reached as one of Section 1.3's three
//! per-simulation leaf-value events. Its own `P_int` must therefore be
//! computed once, before any simulation runs, or no simulation could ever
//! compute `bonus_PUCT` for a root child. This module resolves that by
//! calling the evaluator once, tagged [`ModelGuidedSearchLeafSiteV1::RootPrior`],
//! immediately after the root key is known and before the simulation budget
//! loop starts; the accompanying `v_raw` this call also returns (both heads
//! always arrive together per Section 1.3's shared-trunk note) is computed
//! and discarded, since the root is never itself backed up as a leaf value.
//! This is additive to, not a substitute for, Section 1.3's three-site
//! enumeration (which is specifically about events during a simulation's
//! OWN traversal); it does not change, and this module's own dispatch-site
//! test counters do not conflate it with, sites 2 or 3.
//!
//! # Resolved design point: what routes to site 3
//!
//! Section 1.3 enumerates three per-simulation leaf-value sites; v1's own
//! `run_simulation_v1` implementation, read literally, has FOUR textual call
//! sites for its static evaluator: (a) the top-of-loop check; (b) the
//! immediate post-transition depth-cap check (the genuine site-3 case); (c)
//! newly-expanded-node creation (site 2); and (d) the coverage-guarantee
//! early exit for an EXISTING next node during the initial "every root
//! action gets one visit" phase.
//!
//! **(a) is unreachable under any well-formed call, not merely rare in
//! practice.** Its condition (`remaining_depth == 0 || *transitions_used >=
//! transition_budget`) is checked, and found false, by branch (b)
//! immediately before the only path that can loop back around to (a) (the
//! "existing node found" arm, taken when `forced_root_action` is `None`);
//! nothing on that path changes `remaining_depth` or `transitions_used`
//! between the two checks, so (a)'s condition cannot flip from false to true
//! in between. On the very first iteration, the caller's own preconditions
//! make it false there too: the outer budget loop only starts a new
//! simulation while `transitions_used < transition_budget`, and `depth_cap`
//! is always positive in every real call. This is defense-in-depth dead
//! code, structurally identical to v1's own equally-unreachable top-of-loop
//! branch -- this module's traversal loop deliberately mirrors v1's shape
//! line for line, per the inherit-verbatim mandate (Section 1.1) -- and is
//! kept here for structural symmetry with that inherited traversal, not
//! because it fires in practice. (An earlier revision of this comment
//! claimed it "fires only on mid-tree budget exhaustion in practice"; that
//! claim was checked against the actual control flow and found false, and
//! is retracted here rather than left standing.)
//!
//! **(b) and (d) route to the same tag as a matter of event semantics, not
//! tree-lookup structure.** Both are an evaluation of a search-frontier
//! state that does not expand a new tree node this simulation (genuine site
//! 3, and v1's own coverage-guarantee early exit, respectively). An earlier
//! revision of this comment justified the grouping by claiming both
//! "evaluate an existing node without creating a new one," calling that
//! "structurally the defining property of site 3." That justification does
//! not hold as written: (b) itself never looks up or touches an existing
//! tree node before evaluating -- it short-circuits on the depth-cap/budget
//! check immediately after a transition, before `tree.find_node` is ever
//! called -- so there is no shared tree-lookup property between (b) and (d)
//! to appeal to. What actually licenses folding (a) into this same tag,
//! despite its own separate unreachability, is the design's own text:
//! "'Budget exhaustion'... is a decision-level condition... not itself a
//! fourth per-simulation leaf trigger, and this design does not treat it as
//! one." And because per-simulation redetermination is inherited verbatim
//! (Section 1.1), the concrete state underlying a given tree key differs on
//! every simulation even when the key itself does not: a first-ever
//! encounter with a given depth-cap-frontier key and a genuine later
//! revisit of that same key are therefore INDISTINGUISHABLE at this site by
//! design, deliberately -- the evaluator is invoked fresh either way,
//! exactly as Section 1.3 requires ("the evaluator is therefore invoked
//! fresh on every such revisit, never cached from a prior simulation's
//! result"). This module routes (a) (dead), (b), and (d) all through the
//! same [`ModelGuidedSearchLeafSiteV1::RevisitedDepthCapLeaf`] tag and mock
//! counter. Three site TAGS exist here (`RootPrior` is pre-loop and not a
//! per-simulation site at all), matching the design's three-site count
//! exactly once `RootPrior` is set aside.
//!
//! # Resolved design point: `TerminalClassificationV1::Truncated`
//!
//! v1's own `terminal_value_v1` falls back to its static evaluator for a
//! truncated (decision-cap-reached) terminal, a case Section 1.3's own
//! three-site enumeration does not separately name (it enumerates ways a
//! SIMULATION's traversal ends, and frames the design as comprehensively
//! "replacing v1's static evaluator" with "no site... silently left on the
//! static evaluator"). Leaving this one case on v1's evaluator would
//! contradict that framing, so this module routes it to the value head too,
//! tagged `RevisitedDepthCapLeaf` (no new tree node is at stake here either).
//! `RlSessionTerminalV1` carries no per-actor perspective field (unlike a
//! live decision), so this case is evaluated as if the leaf actor were the
//! root player (`leaf_acting_player_is_root = true`, no negation) -- the
//! same root-relative convention v1's own static evaluator already used
//! unconditionally for every value it ever produced (`evaluate_state_v1(state,
//! root_player)` has no acting-player parameter at all). `Halted` is
//! unaffected: v1 already errors there (`NonNaturalTerminal`), unchanged.
//!
//! # Item 6a: the live-session encoder bridge (supersedes the former
//! "Resolved scope boundary" note below)
//!
//! An earlier revision of this module's docs stated that
//! [`ModelGuidedSearchRealForwardValueEvaluatorV1`] does not implement
//! [`ModelGuidedSearchLeafEvaluatorV1`], and that "no construction site
//! anywhere in this crate builds a `FlatScoringDecisionViewV2` from a live
//! `&FastActorSessionV1`." That second claim does not survive a full trace
//! and is retracted here rather than left standing (the same discipline
//! this module's other "Resolved design point" notes already apply to
//! their own earlier, corrected claims). One real construction site exists
//! today: the checkpoint-opponent branch of the async flat-scored rollout
//! (`async_flat_scored_rollout_v1.rs`, the `(Some(_), None, false) |
//! (None, Some(_), false)` arm around `F::encode_packet`/
//! `F::ladder_scoring_view`) builds a `FlatScoringDecisionViewV2` from
//! `self.session.as_ref()` -- a live, in-flight rollout session, not a
//! staged trajectory record -- every time a ladder or population opponent
//! scores a decision. It is layered three deep behind the
//! `FlatScoredFamilyCore` trait's generic dispatch
//! (`F::encode_packet` -> an owned, validated packet
//! (`ValidatedOwnedFlatScoringDecisionV2`) -> `.scorer_view_v1()`), which is
//! almost certainly why the earlier grep-shaped search that produced the
//! original claim missed it: the literal `FlatScoringDecisionViewV2::new`
//! call is inside `OwnedFlatScoringDecisionV2::scorer_view` in
//! `async_flat_scored_rollout_v2.rs`, reached only through that indirection,
//! not through any direct construction visible from this module's own
//! neighborhood.
//!
//! That checkpoint-opponent encode path -- `FlatScoredFamilyV2::encode_packet`
//! (an unmodified, existing `pub(crate)` trait method,
//! `async_flat_scored_rollout_v2.rs`) producing a
//! `ValidatedOwnedFlatScoringDecisionV2` whose `.scorer_view_v1()` is a
//! `FlatScoringDecisionViewV2`, then `NativeFlatTensorizerV2::fill` (already
//! `pub(crate)`, `native_flat_tensorizer_v2.rs`) tensorizing that view into a
//! `NativeFlatDecisionTensorV2`, then `encoded_decision_view_v1` (bumped from
//! private to `pub(crate)` by this change, `native_checkpoint_inference_v1.rs`,
//! zero behavior change to its body) converting that tensor into the
//! `NativeEncodedDecisionViewV1` [`NativePolicyValueNetV1::forward_search_deterministic_v1`]
//! consumes -- is the EXACT machinery
//! `NativeCheckpointInferenceV1::score_decision_search_deterministic_v1`
//! already wraps for a full, checkpoint-manifest-backed model handle. This
//! evaluator reuses the same three pure, unmodified pieces directly against
//! a bare `&NativePolicyValueNetV1` instead (matching this file's own
//! existing test precedent of constructing `NativePolicyValueNetV1::
//! runner_fixed_v1` in-memory, with no checkpoint-manifest/Store dependency),
//! because `NativeCheckpointInferenceV1::score_decision_search_deterministic_v1`
//! itself is not reused directly: doing so would route search leaves through
//! `NativeCheckpointInferenceV1`, the exact checkpoint-manifest-backed type
//! the calibration runner's own doc draws the eval-only/production boundary
//! around, and would require a full `ValidatedTrainRunV2`/`CheckpointManifestV3`
//! construction this evaluator's own tests (and item 5's precedent before it)
//! deliberately avoid. No existing function's body changes; the only
//! reachability change anywhere in this diff is the one visibility bump
//! above.
//!
//! [`ModelGuidedSearchLeafEvaluatorV1`] is now implemented for
//! [`ModelGuidedSearchRealForwardValueEvaluatorV1`] (below), calling the
//! real, MXCSR-gated `forward_search_deterministic_v1` exactly once per
//! leaf-evaluation event and reading BOTH heads (`output.logits` for the
//! prior, `output.value` for `v_raw`) off that one call's return value, per
//! Section 1.3's `heads_per_leaf` economy. `value_domain` is taken as a
//! construction parameter and used as-is; this evaluator does not decide
//! what it should be for any real checkpoint (item 6b, elsewhere).
//!
//! # Determination: the real-forward evaluator's live-decision requirement
//!
//! `evaluate_leaf_v1`'s trait signature receives `session` and `leaf_key`,
//! not a `FastActorDecisionV1` directly. This evaluator recovers the
//! decision to encode from `session.current_response()`, which is sound
//! for every one of `run_simulation_puct_v1`'s actual call sites: `session`
//! there is always the per-simulation redeterminized clone, freshly
//! positioned by `consume_current_flat_action_slice_v2` at exactly the leaf
//! this call is evaluating, so `session.current_response()` at the moment
//! of the call is that same leaf's live `Decision` -- for `RootPrior`
//! (session unmodified, verified equal to `expected` before the loop
//! starts), for `NewlyExpandedNode` (site 2, just transitioned to), and for
//! the ordinary `RevisitedDepthCapLeaf` cases (site 3 proper, v1's own
//! coverage-guarantee early exit, and the structurally-dead top-of-loop
//! branch). The one exception, documented in code as
//! [`ModelGuidedSearchCoreErrorV1::NoLiveDecisionToEncode`]: the
//! `TerminalClassificationV1::Truncated` synthetic-key dispatch (module
//! docs above) calls the evaluator with `session.current_response()` still
//! `Terminal` (the transition that reached it already consumed the last
//! decision), carrying no live decision to encode at all. This evaluator
//! fails closed there rather than fabricating an encoding; a construction
//! that never reaches a decision-cap truncation within its own depth cap
//! (every test in this diff) never exercises that path.
//!
//! # Determination: the real-forward PRIOR conversion (item 6's own open
//! question; sigmoid RETRACTED, softmax adopted -- countersigning panel
//! ruling, 2026-08-16)
//!
//! Item 1's own module (`model_guided_search_prior_quantization_v1`) states
//! its input contract as "per-legal-action weights that need not already
//! sum to 1... each required to lie in `[0.0, 1.0]`" and explicitly declines
//! to reuse `fast_sampler.rs`'s softmax/exponential-weighting step, leaving
//! the real conversion for item 6. This evaluator's first revision resolved
//! it with a per-action sigmoid, documented honestly as an open
//! implementation choice with a defensible (if debatable) rationale. A
//! countersigning panel reviewed that choice on the merits, not only for
//! documentation honesty, and RULED it wrong: the panel's own
//! recomputation, for legal logits `{10.0, 0.0 x9}`, found softmax puts
//! `~99.96%` of the prior mass on the dominant action, while
//! sigmoid-plus-Hamilton-apportionment puts only `~18%` there -- sigmoid
//! near-flattens the net's own guidance, and the policy head is trained as
//! a softmax distribution (cross-entropy against a normalized target), so
//! the prior slot must carry the object the net actually learned, not a
//! differently-shaped one. This also matters for mode (c): a future
//! visit-count training target is downstream of whichever prior shapes
//! expansion order and the PUCT bonus, so a wrongly-shaped prior would bias
//! that target's distribution too, not only today's search quality. The
//! ruling is a merits correction, not a fidelity one: the design left this
//! conversion open, and the sigmoid documentation was itself accurate about
//! what it did and why; the panel simply found a better answer to the
//! question the design deliberately left unanswered.
//!
//! This evaluator now calls
//! [`softmax_legal_action_weights_v1`](crate::deterministic_math_v1::softmax_legal_action_weights_v1)
//! directly on `output.logits` (already exactly the node's live ordered
//! legal-action set: `NativeFlatTensorizerV2` only ever tensorizes the
//! decision's actual candidate rows, so no separate masking step is needed
//! here at all -- the encode path IS the mask, unchanged from the
//! sigmoid-era note this superseded). The sigmoid function
//! (`sigmoid_v1`) and its `tanh_f32_v1` import are REMOVED outright by this
//! revision, not deprecated or left as a dead alternative surface, per the
//! ruling's own instruction. See
//! [`crate::deterministic_math_v1`]'s own module doc, "Panel ruling
//! extension (2026-08-16)" section, for the full softmax algorithm (the
//! new `exp_f64_v1` primitive's bit-pinned operation order, the clamp
//! floor, and why this design never performs a floating-point summation
//! reduction at all: `softmax_legal_action_weights_v1` returns
//! UNNORMALIZED per-action weights, and `quantize_prior_v1`'s own exact
//! `u128` largest-remainder apportionment is what completes the softmax
//! semantics losslessly, exactly as it already did for the withdrawn
//! sigmoid's own not-summing-to-1 weights).

use crate::async_flat_scored_rollout_v1::FlatScoredFamilyCore;
use crate::async_flat_scored_rollout_v2::{FlatScoredFamilyV2, OwnedFlatScoringDecisionV2};
use crate::deterministic_math_v1::softmax_legal_action_weights_v1;
use crate::flat_policy_v2::FlatDecisionEncoderV2;
use crate::ids::PlayerId;
use crate::kernel_native_search_opponent_v1::{
    derive_simulation_seed_v1, integer_ucb_bonus_v1, natural_terminal_value_v1, player_id_v1,
    search_node_key_v1, select_final_root_action_v1, KernelNativeSearchActionStatV1,
    KernelNativeSearchErrorV1, SearchActionStatV1,
};
use crate::model_guided_search_authority_v1::{
    ModelGuidedSearchAuthorityError, ModelGuidedSearchAuthorityV1,
};
use crate::model_guided_search_prior_quantization_v1::{
    prior_expansion_order_v1, puct_bonus_v1, quantize_prior_v1,
    ModelGuidedSearchPriorQuantizationErrorV1,
};
use crate::model_guided_search_value_quantization_v1::{
    quantize_value_v1, ModelGuidedSearchValueHeadDomainV1,
    ModelGuidedSearchValueQuantizationErrorV1,
};
use crate::native_checkpoint_inference_v1::encoded_decision_view_v1;
use crate::native_flat_tensorizer_v2::{NativeFlatDecisionTensorV2, NativeFlatTensorizerV2};
use crate::native_policy_value_net_v1::{
    NativeEncodedDecisionViewV1, NativePolicyValueErrorV1, NativePolicyValueNetV1,
};
use crate::rl::TerminalClassificationV1;
use crate::rl_session::{
    FastActorDecisionV1, FastActorResponseV1, FastActorSessionV1, FlatActionDecisionSliceErrorV1,
    RlSessionError, RlSessionTerminalV1,
};
use crate::state::SplitMix64;
use serde::{Deserialize, Serialize};
use std::cell::Cell;
use std::fmt;

/// Fail-closed error vocabulary for this module. `Tree` reuses v1's own
/// error type directly (Section 1.1: the tree/session/redetermination
/// mechanics are inherited verbatim, so their failure modes are too);
/// the remaining variants are genuinely new to this design.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelGuidedSearchCoreErrorV1 {
    /// Inherited-verbatim tree/session/redetermination failure.
    Tree(KernelNativeSearchErrorV1),
    Authority(ModelGuidedSearchAuthorityError),
    Prior(ModelGuidedSearchPriorQuantizationErrorV1),
    Value(ModelGuidedSearchValueQuantizationErrorV1),
    /// A real-forward call failed (`NativePolicyValueErrorV1`, `pub(crate)`
    /// in its own module, so stored here as its `Display` text rather than
    /// the concrete type, to avoid leaking a `pub(crate)` type through this
    /// enum's own `pub` visibility).
    Forward(String),
    /// The evaluator seam returned a `legal_action_weights` vector whose
    /// length did not match the node's own live legal-action count: a
    /// contract violation of the evaluator implementation, not a tree or
    /// session defect.
    EvaluatorContract,
    /// Item 6a: the real-forward evaluator was invoked while
    /// `session.current_response()` was not a live `Decision`. This is a
    /// known, documented scope boundary, not a bug: it is reachable exactly
    /// once, at the `TerminalClassificationV1::Truncated` synthetic-key
    /// dispatch (module docs, "Resolved design point:
    /// `TerminalClassificationV1::Truncated`"), where v1's own
    /// `RlSessionTerminalV1` carries no live decision to encode into a
    /// `FlatScoringDecisionViewV2` at all. The mock evaluator has no such
    /// gap, since it derives its output purely from `leaf_key` bytes, never
    /// from session content. See this module's own "Determination: the
    /// real-forward evaluator's live-decision requirement" note.
    NoLiveDecisionToEncode,
    /// Item 6a: `FlatScoredFamilyCore::encode_packet` rejected the live
    /// decision. The trait boundary's error type is an opaque `()` (see
    /// that trait's own doc comment), so no further detail is available to
    /// preserve here.
    Encode,
    /// Item 6a: tensorizing an encoded `FlatScoringDecisionViewV2` into the
    /// net's thirteen-tensor input failed (`NativeFlatTensorErrorV2`,
    /// `pub(crate)` in its own module, stored as `Display` text for the
    /// identical reason `Forward` stores `NativePolicyValueErrorV1` as
    /// text: avoids leaking a `pub(crate)` type through this enum's own
    /// `pub` visibility).
    Tensorize(String),
}

impl fmt::Display for ModelGuidedSearchCoreErrorV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for ModelGuidedSearchCoreErrorV1 {}

impl From<KernelNativeSearchErrorV1> for ModelGuidedSearchCoreErrorV1 {
    fn from(error: KernelNativeSearchErrorV1) -> Self {
        Self::Tree(error)
    }
}

impl From<FlatActionDecisionSliceErrorV1> for ModelGuidedSearchCoreErrorV1 {
    fn from(error: FlatActionDecisionSliceErrorV1) -> Self {
        Self::Tree(error.into())
    }
}

impl From<RlSessionError> for ModelGuidedSearchCoreErrorV1 {
    fn from(error: RlSessionError) -> Self {
        Self::Tree(error.into())
    }
}

impl From<ModelGuidedSearchAuthorityError> for ModelGuidedSearchCoreErrorV1 {
    fn from(error: ModelGuidedSearchAuthorityError) -> Self {
        Self::Authority(error)
    }
}

impl From<ModelGuidedSearchPriorQuantizationErrorV1> for ModelGuidedSearchCoreErrorV1 {
    fn from(error: ModelGuidedSearchPriorQuantizationErrorV1) -> Self {
        Self::Prior(error)
    }
}

impl From<ModelGuidedSearchValueQuantizationErrorV1> for ModelGuidedSearchCoreErrorV1 {
    fn from(error: ModelGuidedSearchValueQuantizationErrorV1) -> Self {
        Self::Value(error)
    }
}

impl From<NativePolicyValueErrorV1> for ModelGuidedSearchCoreErrorV1 {
    fn from(error: NativePolicyValueErrorV1) -> Self {
        Self::Forward(error.to_string())
    }
}

/// Tags exactly which of Section 1.3's dispatch events an
/// [`ModelGuidedSearchLeafEvaluatorV1::evaluate_leaf_v1`] call corresponds
/// to. See the module docs' two "Resolved design point" notes for exactly
/// which v1-textual call sites map to which tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelGuidedSearchLeafSiteV1 {
    /// One-time, pre-simulation-loop evaluation of the root node's own
    /// prior. Not one of Section 1.3's three per-simulation leaf-value
    /// sites; see module docs.
    RootPrior,
    /// Section 1.3, site 2: the single new successor a simulation is
    /// entitled to create.
    NewlyExpandedNode,
    /// Section 1.3, site 3, plus this module's resolved routing of budget
    /// exhaustion, v1's coverage-guarantee early exit, and truncated
    /// terminals; see module docs.
    RevisitedDepthCapLeaf,
}

/// Raw, not-yet-quantized net outputs for one leaf-evaluation event.
/// `legal_action_weights[i]` is the masked (not-necessarily-renormalized)
/// policy weight for the legal action at flat-action index `i`, each in
/// `[0.0, 1.0]` (the exact precondition
/// [`crate::model_guided_search_prior_quantization_v1::quantize_prior_v1`]
/// requires); meaningful only at
/// [`ModelGuidedSearchLeafSiteV1::RootPrior`]/`NewlyExpandedNode` (no tree
/// node is cached at `RevisitedDepthCapLeaf`, so the weights there are
/// computed -- both heads always arrive together -- but unused). `v_raw` is
/// the value head's raw scalar from the LEAF'S OWN ACTING PLAYER's
/// perspective, not yet perspective-flipped (Section 1.3, pipeline step 3
/// is the search loop's own job, applied after this call returns).
#[derive(Debug, Clone)]
pub struct ModelGuidedSearchLeafForwardV1 {
    pub legal_action_weights: Vec<f32>,
    pub v_raw: f32,
}

/// The evaluator seam (binding sequencing ruling, collab `CLAUDE #252`).
/// Implementors produce raw net outputs for one leaf; the search loop itself
/// owns all quantization (see module docs, "The evaluator seam").
pub trait ModelGuidedSearchLeafEvaluatorV1 {
    fn evaluate_leaf_v1(
        &self,
        session: &FastActorSessionV1,
        leaf_key: [u8; 32],
        legal_action_count: u32,
        site: ModelGuidedSearchLeafSiteV1,
    ) -> Result<ModelGuidedSearchLeafForwardV1, ModelGuidedSearchCoreErrorV1>;
}

/// Deterministic mock leaf evaluator for this module's own tests. Every
/// output is a pure, integer-derived function of `leaf_key` (the exact tree
/// node key `search_node_key_v1` computes, reused unchanged from v1) and
/// `legal_action_count`: two runs over the same seed/state reach the same
/// `leaf_key` at every dispatch (redetermination and node-keying are
/// inherited verbatim), hence produce byte-identical mock forwards, hence
/// byte-identical trees (Section 1.5's byte-reproducibility requirement,
/// exercised end to end against this seam; the real forward's own
/// determinism is design item 3's audit, already sealed on `main`).
///
/// Counts calls per site (`root_prior_calls`/`site2_calls`/`site3_calls`),
/// so tests can assert each dispatch site actually fires.
#[derive(Debug, Default)]
pub struct MockLeafEvaluatorV1 {
    pub root_prior_calls: Cell<u32>,
    pub site2_calls: Cell<u32>,
    pub site3_calls: Cell<u32>,
}

impl MockLeafEvaluatorV1 {
    pub fn new() -> Self {
        Self::default()
    }
}

impl ModelGuidedSearchLeafEvaluatorV1 for MockLeafEvaluatorV1 {
    fn evaluate_leaf_v1(
        &self,
        _session: &FastActorSessionV1,
        leaf_key: [u8; 32],
        legal_action_count: u32,
        site: ModelGuidedSearchLeafSiteV1,
    ) -> Result<ModelGuidedSearchLeafForwardV1, ModelGuidedSearchCoreErrorV1> {
        match site {
            ModelGuidedSearchLeafSiteV1::RootPrior => {
                self.root_prior_calls.set(self.root_prior_calls.get() + 1);
            }
            ModelGuidedSearchLeafSiteV1::NewlyExpandedNode => {
                self.site2_calls.set(self.site2_calls.get() + 1);
            }
            ModelGuidedSearchLeafSiteV1::RevisitedDepthCapLeaf => {
                self.site3_calls.set(self.site3_calls.get() + 1);
            }
        }
        if legal_action_count == 0 {
            return Err(ModelGuidedSearchCoreErrorV1::EvaluatorContract);
        }
        let seed = u64::from_le_bytes(
            leaf_key[0..8]
                .try_into()
                .expect("leaf_key is exactly 32 bytes"),
        );
        let mut rng = SplitMix64::seed(seed);
        // v_raw in [-1.0, 1.0]: integer-derived, then a single division (no
        // reduction-order ambiguity to reason about).
        let v_units = (rng.next_u64() % 2_001) as i64 - 1_000;
        let v_raw = v_units as f32 / 1_000.0;
        let mut legal_action_weights = Vec::with_capacity(legal_action_count as usize);
        for _ in 0..legal_action_count {
            // (0.0, 1.0], nonzero by construction: quantize_prior_v1 treats
            // an all-zero weight vector as a hard error (see that module's
            // own docs, "all-zero weight is a hard error").
            let w_units = rng.next_u64() % 1_000 + 1;
            legal_action_weights.push(w_units as f32 / 1_000.0);
        }
        Ok(ModelGuidedSearchLeafForwardV1 {
            legal_action_weights,
            v_raw,
        })
    }
}

/// REAL-FORWARD SEAM. See the module docs' "Item 6a: the live-session
/// encoder bridge" and "Determination" notes for exactly what this type
/// does and does not do, and why.
///
/// `pub(crate)`, not `pub`: it inherently carries
/// `NativePolicyValueNetV1`/`NativeEncodedDecisionViewV1`, both `pub(crate)`
/// in their own module, so it cannot be part of this library's external
/// `pub` surface without leaking a private type. Its only callers today are
/// this module's own tests (item 6a implements the trait; wiring a real
/// launcher/eval CONSUMER of it is not this diff's scope, see module docs),
/// hence the dead-code allowance outside `cfg(test)` builds, matching the
/// task's own "platform-scoped [or, here, feature-scoped] dead-code
/// allowances only if genuinely needed" instruction.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct ModelGuidedSearchRealForwardValueEvaluatorV1<'a> {
    model: &'a NativePolicyValueNetV1,
    value_domain: ModelGuidedSearchValueHeadDomainV1,
    /// Item 6a instrumentation, not production state: counts
    /// `evaluate_leaf_v1` invocations. Compared against `forward_calls`
    /// (below) so a test can prove the single-forward-per-leaf economy
    /// (design Section 1.3: both heads "computed together in one forward
    /// call per leaf-evaluation event... when the checkpoint architecture
    /// shares a trunk between policy and value") empirically, not only by
    /// code inspection: the two counters must stay equal, call for call.
    leaf_calls: Cell<u32>,
    /// Item 6a instrumentation: counts calls to
    /// `forward_search_deterministic_v1` made from `evaluate_leaf_v1`. See
    /// `leaf_calls` above.
    forward_calls: Cell<u32>,
}

#[cfg_attr(not(test), allow(dead_code))]
impl<'a> ModelGuidedSearchRealForwardValueEvaluatorV1<'a> {
    pub(crate) fn new(
        model: &'a NativePolicyValueNetV1,
        value_domain: ModelGuidedSearchValueHeadDomainV1,
    ) -> Self {
        Self {
            model,
            value_domain,
            leaf_calls: Cell::new(0),
            forward_calls: Cell::new(0),
        }
    }

    /// Runs the real, MXCSR-gated deterministic forward pass
    /// (`forward_search_deterministic_v1`'s own gate is not duplicated
    /// here -- see that function's doc comment) on an already-encoded
    /// decision view, then this design's Section 1.3 value-quantization
    /// pipeline. Returns the quantized, root-perspective value in
    /// `[-9_000, 9_000]`. Unchanged by item 6a: still item 5's own direct,
    /// already-encoded-view entry point, independent of
    /// [`ModelGuidedSearchLeafEvaluatorV1::evaluate_leaf_v1`] below (which
    /// does not call this method, and does not touch the counters below
    /// itself; each has its own accounting).
    pub(crate) fn evaluate_encoded_value_v1(
        &self,
        encoded: NativeEncodedDecisionViewV1<'_>,
        leaf_acting_player_is_root: bool,
    ) -> Result<i32, ModelGuidedSearchCoreErrorV1> {
        let output = self.model.forward_search_deterministic_v1(encoded)?;
        Ok(quantize_value_v1(
            &self.value_domain,
            output.value,
            leaf_acting_player_is_root,
        )?)
    }

    /// Item 6a test instrumentation accessors (see `leaf_calls`/
    /// `forward_calls` field docs). `pub(crate)` so this module's own test
    /// suite can read them; not part of any production decision.
    #[cfg(test)]
    pub(crate) fn leaf_calls_v1(&self) -> u32 {
        self.leaf_calls.get()
    }

    #[cfg(test)]
    pub(crate) fn forward_calls_v1(&self) -> u32 {
        self.forward_calls.get()
    }
}

/// Item 6a: the live-session encoder bridge itself. See module docs, "Item
/// 6a: the live-session encoder bridge" for the encode-path map and
/// "Determination: the real-forward evaluator's live-decision requirement"
/// / "Determination: the real-forward PRIOR conversion" for the two design
/// decisions this implementation makes.
impl ModelGuidedSearchLeafEvaluatorV1 for ModelGuidedSearchRealForwardValueEvaluatorV1<'_> {
    fn evaluate_leaf_v1(
        &self,
        session: &FastActorSessionV1,
        _leaf_key: [u8; 32],
        legal_action_count: u32,
        _site: ModelGuidedSearchLeafSiteV1,
    ) -> Result<ModelGuidedSearchLeafForwardV1, ModelGuidedSearchCoreErrorV1> {
        self.leaf_calls.set(self.leaf_calls.get() + 1);
        let FastActorResponseV1::Decision(decision) = session.current_response() else {
            return Err(ModelGuidedSearchCoreErrorV1::NoLiveDecisionToEncode);
        };
        if decision.legal_action_count != legal_action_count {
            return Err(ModelGuidedSearchCoreErrorV1::EvaluatorContract);
        }

        let tensor = encode_live_decision_tensor_v1(session, decision)?;
        let encoded = encoded_decision_view_v1(&tensor);

        // Single forward call, both heads (Section 1.3's `heads_per_leaf`
        // economy): `output.logits` is the prior side, `output.value` is
        // the raw value side, from the one call below.
        let output = self.model.forward_search_deterministic_v1(encoded)?;
        self.forward_calls.set(self.forward_calls.get() + 1);
        if output.logits.len() != legal_action_count as usize
            || output.logits.iter().any(|value| !value.is_finite())
            || !output.value.is_finite()
        {
            return Err(ModelGuidedSearchCoreErrorV1::EvaluatorContract);
        }

        // `output.logits` already corresponds, index for index, to the
        // node's live ordered legal-action set: `NativeFlatTensorizerV2`
        // only ever tensorizes the decision's actual candidate rows, so
        // the encode path IS the mask (module docs, "Determination: the
        // real-forward PRIOR conversion"). No separate masking step.
        // Softmax (panel ruling, 2026-08-16): unnormalized per-action
        // weights, positionally index-matched to `output.logits`;
        // `quantize_prior_v1` renormalizes.
        let legal_action_weights = softmax_legal_action_weights_v1(&output.logits);

        Ok(ModelGuidedSearchLeafForwardV1 {
            legal_action_weights,
            v_raw: output.value,
        })
    }
}

/// Item 6a: encodes one live decision into the net's own thirteen-tensor
/// input, using the exact, unmodified checkpoint-opponent encode path
/// (module docs, "Item 6a: the live-session encoder bridge") -- fresh
/// scratch every call, no caching. Factored out of
/// [`ModelGuidedSearchLeafEvaluatorV1::evaluate_leaf_v1`] above so this
/// module's own encode-determinism test (below) exercises the identical
/// code path production uses, not a re-derived copy that could drift from
/// it. `session: &FastActorSessionV1` (immutable): this function cannot
/// mutate the session, or consume any RNG state stored mutably on it, by
/// construction -- the borrow checker enforces this structurally, not
/// merely as an empirical observation about today's implementation. That
/// is the exact "if the existing encode path consumes RNG or mutable
/// session state, isolate that" concern the task's own instructions name;
/// this module's answer is that it provably cannot, and the
/// encode-determinism test below checks the resulting claim (byte-identical
/// output for the same live leaf) empirically, on top of that structural
/// argument.
fn encode_live_decision_tensor_v1(
    session: &FastActorSessionV1,
    decision: FastActorDecisionV1,
) -> Result<NativeFlatDecisionTensorV2, ModelGuidedSearchCoreErrorV1> {
    let mut encoder = FlatDecisionEncoderV2::default();
    let owned = OwnedFlatScoringDecisionV2::default();
    let validated = FlatScoredFamilyV2::encode_packet(session, decision, &mut encoder, owned)
        .map_err(|()| ModelGuidedSearchCoreErrorV1::Encode)?;
    let view = validated.scorer_view_v1();
    let mut tensorizer = NativeFlatTensorizerV2::new();
    let mut tensor = NativeFlatDecisionTensorV2::default();
    tensorizer
        .fill(view, &mut tensor)
        .map_err(|error| ModelGuidedSearchCoreErrorV1::Tensorize(error.to_string()))?;
    Ok(tensor)
}

/// A node in the model-guided search tree. Structurally v1's own
/// `SearchNodeV1` (`key`, `actor`, `visits`, `actions: Vec<SearchActionStatV1>`,
/// the last type reused directly, not reimplemented) plus exactly one new
/// field: `prior`, the cached PUCT prior (`P_int`, Section 1.2), computed
/// once at node creation (or, for the root, once before the first
/// simulation -- see module docs) and never recomputed during selection.
#[derive(Debug, Clone)]
struct ModelGuidedSearchNodeV1 {
    key: [u8; 32],
    actor: PlayerId,
    visits: u32,
    actions: Vec<SearchActionStatV1>,
    /// `prior[i]` is `P_int` for the action at flat-action index `i`;
    /// `prior.len() == actions.len()`, enforced at construction.
    prior: Vec<u32>,
}

impl ModelGuidedSearchNodeV1 {
    fn new(
        key: [u8; 32],
        actor: PlayerId,
        prior: Vec<u32>,
    ) -> Result<Self, ModelGuidedSearchCoreErrorV1> {
        if prior.is_empty() {
            return Err(KernelNativeSearchErrorV1::CorruptTree.into());
        }
        let action_count = prior.len();
        Ok(Self {
            key,
            actor,
            visits: 0,
            actions: (0..action_count)
                .map(|_| SearchActionStatV1 {
                    visits: 0,
                    value_sum: 0,
                    child_nodes: Vec::new(),
                })
                .collect(),
            prior,
        })
    }
}

#[derive(Debug, Clone)]
struct ModelGuidedSearchTreeV1 {
    /// Vec-only storage, ordered linear-scan lookup: the identical
    /// cross-host-determinism property v1's own `SearchTreeV1` states for
    /// itself ("a cross-host determinism property, not an optimization
    /// choice").
    nodes: Vec<ModelGuidedSearchNodeV1>,
}

impl ModelGuidedSearchTreeV1 {
    fn new(
        root_key: [u8; 32],
        root_actor: PlayerId,
        root_prior: Vec<u32>,
    ) -> Result<Self, ModelGuidedSearchCoreErrorV1> {
        Ok(Self {
            nodes: vec![ModelGuidedSearchNodeV1::new(
                root_key, root_actor, root_prior,
            )?],
        })
    }

    fn find_node(&self, key: [u8; 32]) -> Option<usize> {
        self.nodes.iter().position(|node| node.key == key)
    }
}

/// Prior-ordered expansion (Section 1.2, "Expansion order": unvisited
/// actions expand in descending `P_int(a)`, ties broken by ascending
/// flat-action index) plus, once every action has at least one visit, PUCT
/// selection (Section 1.2, "Selection bonus":
/// `bonus_PUCT(a) = floor(bonus(a) * P_int(a) / 1,000,000)`, root-player
/// nodes maximize `mean + bonus_PUCT`, opponent nodes minimize
/// `mean - bonus_PUCT`). Mirrors v1's own `select_tree_action_v1` line for
/// line except for these two swaps; the tie-break rule (strict `>`/`<`,
/// first-found/lowest-index wins) is identical.
fn select_tree_action_puct_v1(
    node: &ModelGuidedSearchNodeV1,
    root_player: PlayerId,
) -> Result<usize, ModelGuidedSearchCoreErrorV1> {
    for &index in &prior_expansion_order_v1(&node.prior) {
        if node.actions[index].visits == 0 {
            return Ok(index);
        }
    }
    let maximizing = node.actor == root_player;
    let mut best_index = 0usize;
    let mut best_score = if maximizing { i64::MIN } else { i64::MAX };
    for (index, action) in node.actions.iter().enumerate() {
        let bonus = integer_ucb_bonus_v1(node.visits, action.visits);
        let bonus_puct = puct_bonus_v1(bonus, node.prior[index])?;
        let bonus_puct =
            i64::try_from(bonus_puct).map_err(|_| KernelNativeSearchErrorV1::CorruptTree)?;
        let score = if maximizing {
            action.mean().saturating_add(bonus_puct)
        } else {
            action.mean().saturating_sub(bonus_puct)
        };
        let better = if maximizing {
            score > best_score
        } else {
            score < best_score
        };
        if better {
            best_index = index;
            best_score = score;
        }
    }
    Ok(best_index)
}

/// Dispatches one `RevisitedDepthCapLeaf`-style leaf value: calls the
/// evaluator, applies the perspective flip, and quantizes. Shared by every
/// call site that resolved to `RevisitedDepthCapLeaf` in `run_simulation_puct_v1`
/// (see module docs, "Resolved design point: what routes to site 3").
fn dispatch_revisited_leaf_value_v1<E: ModelGuidedSearchLeafEvaluatorV1>(
    evaluator: &E,
    value_domain: &ModelGuidedSearchValueHeadDomainV1,
    session: &FastActorSessionV1,
    leaf_key: [u8; 32],
    legal_action_count: u32,
    leaf_acting_player_is_root: bool,
) -> Result<i32, ModelGuidedSearchCoreErrorV1> {
    let forward = evaluator.evaluate_leaf_v1(
        session,
        leaf_key,
        legal_action_count,
        ModelGuidedSearchLeafSiteV1::RevisitedDepthCapLeaf,
    )?;
    Ok(quantize_value_v1(
        value_domain,
        forward.v_raw,
        leaf_acting_player_is_root,
    )?)
}

/// `TerminalClassificationV1` dispatch. `Natural` reuses v1's exact literal
/// terminal constants (Section 1.3, site 1: "the value head is never invoked
/// here"). `Truncated` and `Halted`: see module docs' "Resolved design
/// point: `TerminalClassificationV1::Truncated`".
fn terminal_value_puct_v1<E: ModelGuidedSearchLeafEvaluatorV1>(
    evaluator: &E,
    value_domain: &ModelGuidedSearchValueHeadDomainV1,
    session: &FastActorSessionV1,
    terminal: &RlSessionTerminalV1,
    root_player: PlayerId,
    census: &mut ModelGuidedSearchLeafCensusV1,
) -> Result<i32, ModelGuidedSearchCoreErrorV1> {
    match terminal.terminal_classification {
        TerminalClassificationV1::Natural => {
            census.natural_terminal_leaves = census.natural_terminal_leaves.saturating_add(1);
            Ok(natural_terminal_value_v1(
                terminal.terminal_outcome,
                root_player,
            )?)
        }
        TerminalClassificationV1::Truncated => {
            census.truncated_terminal_leaves = census.truncated_terminal_leaves.saturating_add(1);
            let mut leaf_key = [0u8; 32];
            leaf_key[..8]
                .copy_from_slice(&session.privileged_core_environment_hash().to_le_bytes());
            dispatch_revisited_leaf_value_v1(evaluator, value_domain, session, leaf_key, 1, true)
        }
        TerminalClassificationV1::Halted => {
            Err(KernelNativeSearchErrorV1::NonNaturalTerminal.into())
        }
    }
}

/// Runs one simulation: descend existing tree structure (prior-ordered
/// expansion / PUCT selection), expand at most one new node, and back up the
/// resulting leaf value. Mirrors
/// `kernel_native_search_opponent_v1::run_simulation_v1`'s control flow
/// line for line; only the action-selection call and the leaf-value
/// production are swapped (see module docs for the full sharing inventory).
#[allow(clippy::too_many_arguments)]
fn run_simulation_puct_v1<E: ModelGuidedSearchLeafEvaluatorV1>(
    tree: &mut ModelGuidedSearchTreeV1,
    session: &mut FastActorSessionV1,
    root_player: PlayerId,
    depth_cap: u16,
    transition_budget: u32,
    transitions_used: &mut u32,
    forced_root_action: Option<u32>,
    evaluator: &E,
    value_domain: &ModelGuidedSearchValueHeadDomainV1,
    census: &mut ModelGuidedSearchLeafCensusV1,
) -> Result<(), ModelGuidedSearchCoreErrorV1> {
    let mut node_index = 0usize;
    let mut remaining_depth = depth_cap;
    let mut node_path = vec![0usize];
    let mut edge_path: Vec<(usize, usize)> = Vec::new();
    let value: i32;

    loop {
        if remaining_depth == 0 || *transitions_used >= transition_budget {
            let node = tree
                .nodes
                .get(node_index)
                .ok_or(KernelNativeSearchErrorV1::CorruptTree)?;
            let leaf_acting_player_is_root = node.actor == root_player;
            census.depth_cap_leaves = census.depth_cap_leaves.saturating_add(1);
            value = dispatch_revisited_leaf_value_v1(
                evaluator,
                value_domain,
                session,
                node.key,
                node.actions.len() as u32,
                leaf_acting_player_is_root,
            )?;
            break;
        }
        let FastActorResponseV1::Decision(decision) = session.current_response() else {
            return Err(KernelNativeSearchErrorV1::CorruptTree.into());
        };
        let node = tree
            .nodes
            .get(node_index)
            .ok_or(KernelNativeSearchErrorV1::CorruptTree)?;
        let expected_key = search_node_key_v1(session, decision, remaining_depth)?;
        if node.key != expected_key || node.actor != player_id_v1(decision.acting_player) {
            return Err(KernelNativeSearchErrorV1::CorruptTree.into());
        }

        let action_index = if node_index == 0 {
            if let Some(forced) = forced_root_action {
                usize::try_from(forced).map_err(|_| KernelNativeSearchErrorV1::CorruptTree)?
            } else {
                select_tree_action_puct_v1(node, root_player)?
            }
        } else {
            select_tree_action_puct_v1(node, root_player)?
        };
        if action_index >= node.actions.len() {
            return Err(KernelNativeSearchErrorV1::CorruptTree.into());
        }
        let binding = session.native_full_trajectory_current_binding_v2(decision)?;
        let response =
            session.consume_current_flat_action_slice_v2(binding, action_index as u32)?;
        *transitions_used = transitions_used
            .checked_add(1)
            .ok_or(KernelNativeSearchErrorV1::CorruptTree)?;
        edge_path.push((node_index, action_index));
        remaining_depth -= 1;

        match response {
            FastActorResponseV1::Terminal(terminal) => {
                value = terminal_value_puct_v1(
                    evaluator,
                    value_domain,
                    session,
                    &terminal,
                    root_player,
                    census,
                )?;
                break;
            }
            FastActorResponseV1::Decision(next_decision) => {
                let next_actor = player_id_v1(next_decision.acting_player);
                if remaining_depth == 0 || *transitions_used >= transition_budget {
                    let leaf_key = search_node_key_v1(session, next_decision, remaining_depth)?;
                    let leaf_acting_player_is_root = next_actor == root_player;
                    census.depth_cap_leaves = census.depth_cap_leaves.saturating_add(1);
                    value = dispatch_revisited_leaf_value_v1(
                        evaluator,
                        value_domain,
                        session,
                        leaf_key,
                        next_decision.legal_action_count,
                        leaf_acting_player_is_root,
                    )?;
                    break;
                }
                let next_key = search_node_key_v1(session, next_decision, remaining_depth)?;
                let next_index = if let Some(existing) = tree.find_node(next_key) {
                    existing
                } else {
                    // Site 2: the single new successor this simulation is
                    // entitled to create. One evaluator call produces both
                    // heads together (Section 1.3): the prior is cached on
                    // the new node; the value backs up this simulation.
                    let forward = evaluator.evaluate_leaf_v1(
                        session,
                        next_key,
                        next_decision.legal_action_count,
                        ModelGuidedSearchLeafSiteV1::NewlyExpandedNode,
                    )?;
                    if forward.legal_action_weights.len()
                        != next_decision.legal_action_count as usize
                    {
                        return Err(ModelGuidedSearchCoreErrorV1::EvaluatorContract);
                    }
                    census.newly_expanded_leaves = census.newly_expanded_leaves.saturating_add(1);
                    let prior = quantize_prior_v1(&forward.legal_action_weights)?;
                    let created = tree.nodes.len();
                    tree.nodes
                        .push(ModelGuidedSearchNodeV1::new(next_key, next_actor, prior)?);
                    tree.nodes[node_index].actions[action_index]
                        .child_nodes
                        .push(created);
                    node_path.push(created);
                    let leaf_acting_player_is_root = next_actor == root_player;
                    value =
                        quantize_value_v1(value_domain, forward.v_raw, leaf_acting_player_is_root)?;
                    break;
                };
                if !tree.nodes[node_index].actions[action_index]
                    .child_nodes
                    .contains(&next_index)
                {
                    tree.nodes[node_index].actions[action_index]
                        .child_nodes
                        .push(next_index);
                }
                node_index = next_index;
                node_path.push(next_index);

                if forced_root_action.is_some() {
                    let node = &tree.nodes[next_index];
                    let leaf_acting_player_is_root = node.actor == root_player;
                    census.depth_cap_leaves = census.depth_cap_leaves.saturating_add(1);
                    value = dispatch_revisited_leaf_value_v1(
                        evaluator,
                        value_domain,
                        session,
                        node.key,
                        node.actions.len() as u32,
                        leaf_acting_player_is_root,
                    )?;
                    break;
                }
            }
        }
    }

    // Instrumentation only, after every decision the traversal makes: the
    // simulation's own depth is exactly the number of transitions it
    // consumed from the root.
    let simulation_depth = u16::try_from(edge_path.len())
        .map_err(|_| ModelGuidedSearchCoreErrorV1::from(KernelNativeSearchErrorV1::CorruptTree))?;
    census.max_simulation_depth = census.max_simulation_depth.max(simulation_depth);
    census.summed_simulation_depth = census
        .summed_simulation_depth
        .saturating_add(u64::from(simulation_depth));

    for index in node_path {
        let node = tree
            .nodes
            .get_mut(index)
            .ok_or(KernelNativeSearchErrorV1::CorruptTree)?;
        node.visits = node
            .visits
            .checked_add(1)
            .ok_or(KernelNativeSearchErrorV1::CorruptTree)?;
    }
    for (node_index, action_index) in edge_path {
        let action = tree
            .nodes
            .get_mut(node_index)
            .and_then(|node| node.actions.get_mut(action_index))
            .ok_or(KernelNativeSearchErrorV1::CorruptTree)?;
        action.visits = action
            .visits
            .checked_add(1)
            .ok_or(KernelNativeSearchErrorV1::CorruptTree)?;
        action.value_sum = action
            .value_sum
            .checked_add(i64::from(value))
            .ok_or(KernelNativeSearchErrorV1::CorruptTree)?;
    }
    Ok(())
}

/// Domain label for the diagnostic stability halves. Distinct from
/// `KERNEL_NATIVE_SEARCH_SEED_DOMAIN_V1`, which stays the label for the
/// per-simulation seed derivation itself: this one separates one seed
/// SOURCE from another, one level above.
pub const MODEL_GUIDED_SEARCH_SEED_HALF_DOMAIN_V1: &str =
    "model-guided-search-chosen-action-stability-seed-half/v1";

/// Which independent half of the simulation-seed space a diagnostic
/// stability run draws from
/// (`LEAD_TEST_TIME_SEARCH_DESIGN_SKETCH_V2.md` Section 5, S0: "chosen-
/// action stability across two independent simulation-seed halves").
///
/// "Independent" is enforced by construction, not asserted: each half's
/// seed digest is `SHA-256(domain || authority digest || half tag)`, so
/// the two halves and the full-budget run all draw from
/// computationally-unrelated digests, and the seeds one produces can never
/// be reached by another.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelGuidedSearchSeedHalfV1 {
    A,
    B,
}

impl ModelGuidedSearchSeedHalfV1 {
    /// The stable, serializable tag for this half. Literal bytes, not the
    /// enum discriminant, so reordering the variants cannot silently
    /// change a derived seed.
    pub const fn tag_v1(self) -> &'static str {
        match self {
            Self::A => "half-a",
            Self::B => "half-b",
        }
    }

    /// Domain-separated seed digest for this half, derived from the
    /// authority digest. Never equal to the authority digest itself (the
    /// domain label is prepended), so the full-budget run's seeds and this
    /// half's seeds are disjoint sequences.
    pub(crate) fn simulation_seed_digest_v1(self, authority_digest: [u8; 32]) -> [u8; 32] {
        use sha2::Digest as _;
        let mut hasher = sha2::Sha256::new();
        hasher.update(MODEL_GUIDED_SEARCH_SEED_HALF_DOMAIN_V1.as_bytes());
        hasher.update(authority_digest);
        hasher.update(self.tag_v1().as_bytes());
        hasher.finalize().into()
    }
}

/// How each simulation's traversal ENDED, counted across a whole decision.
/// Added for the test-time-search wrapper's outcome schema (V4 on the wire), which
/// records "depth and terminal-leaf counts" per decision
/// (`LEAD_TEST_TIME_SEARCH_DESIGN_SKETCH_V2.md` Section 5, S0).
///
/// The four leaf counts partition the simulations exactly: every
/// simulation ends at exactly one of them, so they always sum to
/// [`ModelGuidedSearchDecisionV1::simulations`]. That is an invariant a
/// consumer can check, and this module's own tests do.
///
/// This is pure instrumentation. Nothing in the search reads it, so it
/// cannot influence the chosen action; it is filled on the way out.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelGuidedSearchLeafCensusV1 {
    /// Section 1.3 site 1: a natural terminal (+/-10,000 constants, the
    /// value head never invoked).
    pub natural_terminal_leaves: u32,
    /// A decision-cap truncation, routed to the value head by this
    /// module's own resolved design point.
    pub truncated_terminal_leaves: u32,
    /// Section 1.3 site 2: the one new tree node a simulation may create.
    pub newly_expanded_leaves: u32,
    /// Section 1.3 site 3 plus this module's documented routing of budget
    /// exhaustion and v1's coverage-guarantee early exit.
    pub depth_cap_leaves: u32,
    /// Deepest simulation, in transitions from the root. Never exceeds the
    /// authority's `policy_step_depth_cap`.
    pub max_simulation_depth: u16,
    /// Sum of every simulation's depth, so a consumer can form the mean
    /// cutoff depth exactly (integer division on its own terms) rather
    /// than receiving a lossy float from here.
    pub summed_simulation_depth: u64,
}

/// Per-root-action visit/value summary and the search's overall verdict.
/// Structurally identical to v1's own `KernelNativeSearchDecisionV1` plus
/// the leaf census; reuses v1's own `KernelNativeSearchActionStatV1`
/// directly as the element type of `root_action_stats`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelGuidedSearchDecisionV1 {
    pub selected_index: u32,
    pub transitions_used: u32,
    pub simulations: u32,
    pub tree_node_count: u32,
    pub root_action_stats: Vec<KernelNativeSearchActionStatV1>,
    #[serde(default)]
    pub leaf_census: ModelGuidedSearchLeafCensusV1,
}

/// The model-guided searcher's own algorithm path (design items 1-2, 5):
/// IS-MCTS with per-simulation redetermination inherited verbatim from v1,
/// prior-ordered expansion (Section 1.2), and value-head leaf dispatch
/// (Section 1.3) via the evaluator seam. v1's own
/// `KernelNativeSearchOpponentV1` is untouched and remains independently
/// usable; this type is its sibling, not its replacement.
#[derive(Debug, Clone)]
pub struct ModelGuidedSearchCoreV1 {
    authority: ModelGuidedSearchAuthorityV1,
}

impl ModelGuidedSearchCoreV1 {
    pub fn new(
        authority: ModelGuidedSearchAuthorityV1,
    ) -> Result<Self, ModelGuidedSearchCoreErrorV1> {
        authority.validate()?;
        Ok(Self { authority })
    }

    pub fn authority(&self) -> &ModelGuidedSearchAuthorityV1 {
        &self.authority
    }

    /// Runs the full tiered search (this authority's own tier budget and
    /// depth cap) and returns the selected root action plus its full
    /// visit/value record.
    pub fn select_action_v1<E: ModelGuidedSearchLeafEvaluatorV1>(
        &self,
        session: &FastActorSessionV1,
        expected: FastActorDecisionV1,
        evaluator: &E,
        value_domain: &ModelGuidedSearchValueHeadDomainV1,
    ) -> Result<ModelGuidedSearchDecisionV1, ModelGuidedSearchCoreErrorV1> {
        self.select_action_with_budget_v1(
            session,
            expected,
            self.authority.transition_budget,
            self.authority.policy_step_depth_cap,
            evaluator,
            value_domain,
        )
    }

    /// DIAGNOSTIC ONLY. Runs the identical search over a domain-separated
    /// HALF of the simulation-seed space, at half the tier budget, for the
    /// outcome-schema-V3 "chosen-action stability across two independent
    /// simulation-seed halves" record
    /// (`LEAD_TEST_TIME_SEARCH_DESIGN_SKETCH_V2.md` Section 5, S0).
    ///
    /// Its result is recorded and never consulted: the chosen action is,
    /// per the sketch's own wording, "the full-budget result" from
    /// [`Self::select_action_v1`]. Two properties make that structurally
    /// true rather than a convention the caller must honor. First, each
    /// call builds its own tree over its own freshly redeterminized
    /// per-simulation clones, so a half run cannot perturb any state the
    /// full run reads. Second, the half's seeds come from a digest that is
    /// domain-separated from the authority digest itself
    /// (`SHA-256(MODEL_GUIDED_SEARCH_SEED_HALF_DOMAIN_V1 || authority
    /// digest || half tag)`), so no simulation seed drawn by a half can
    /// ever collide with one the full-budget run draws, and the two halves
    /// cannot collide with each other.
    ///
    /// The half budget is `max(tier_budget / 2, root_action_count)`: the
    /// search's own coverage rule requires at least one transition per
    /// root action, so a naive halving would turn a wide decision into a
    /// `BudgetSmallerThanRootActionCount` error and lose the diagnostic
    /// entirely. The `max` is deterministic and depends only on the
    /// decision's own legal-action count, so it cannot make the record a
    /// function of anything but (checkpoint, seed, decision, authority).
    pub(crate) fn select_action_seed_half_v1<E: ModelGuidedSearchLeafEvaluatorV1>(
        &self,
        session: &FastActorSessionV1,
        expected: FastActorDecisionV1,
        evaluator: &E,
        value_domain: &ModelGuidedSearchValueHeadDomainV1,
        half: ModelGuidedSearchSeedHalfV1,
    ) -> Result<ModelGuidedSearchDecisionV1, ModelGuidedSearchCoreErrorV1> {
        let half_budget = (self.authority.transition_budget / 2).max(expected.legal_action_count);
        self.select_action_with_seed_digest_and_budget_v1(
            session,
            expected,
            half.simulation_seed_digest_v1(self.authority.digest()?),
            half_budget,
            self.authority.policy_step_depth_cap,
            evaluator,
            value_domain,
        )
    }

    /// Mirrors `KernelNativeSearchOpponentV1::select_action_with_budget_v1`
    /// line for line (root/coverage preflight, per-simulation redetermination
    /// loop, post-loop hidden-state re-verification, final selection); see
    /// module docs for the one addition (the pre-loop root-prior evaluation)
    /// and the leaf-dispatch swap inside `run_simulation_puct_v1`.
    fn select_action_with_budget_v1<E: ModelGuidedSearchLeafEvaluatorV1>(
        &self,
        session: &FastActorSessionV1,
        expected: FastActorDecisionV1,
        transition_budget: u32,
        depth_cap: u16,
        evaluator: &E,
        value_domain: &ModelGuidedSearchValueHeadDomainV1,
    ) -> Result<ModelGuidedSearchDecisionV1, ModelGuidedSearchCoreErrorV1> {
        let authority_digest = self.authority.digest()?;
        self.select_action_with_seed_digest_and_budget_v1(
            session,
            expected,
            authority_digest,
            transition_budget,
            depth_cap,
            evaluator,
            value_domain,
        )
    }

    /// The one implementation both the full-budget entry point and the
    /// diagnostic seed-half entry point run. `simulation_seed_digest` is
    /// the first input to
    /// [`derive_simulation_seed_v1`]; the full-budget path passes the
    /// authority digest itself (unchanged behavior, byte for byte), and
    /// only the diagnostic path passes a domain-separated derivative.
    /// Factored out rather than duplicated so the two can never drift.
    #[allow(clippy::too_many_arguments)]
    fn select_action_with_seed_digest_and_budget_v1<E: ModelGuidedSearchLeafEvaluatorV1>(
        &self,
        session: &FastActorSessionV1,
        expected: FastActorDecisionV1,
        simulation_seed_digest: [u8; 32],
        transition_budget: u32,
        depth_cap: u16,
        evaluator: &E,
        value_domain: &ModelGuidedSearchValueHeadDomainV1,
    ) -> Result<ModelGuidedSearchDecisionV1, ModelGuidedSearchCoreErrorV1> {
        self.authority.validate()?;
        let FastActorResponseV1::Decision(live) = session.current_response() else {
            return Err(KernelNativeSearchErrorV1::InvalidDecision.into());
        };
        if live != expected
            || session.kernel_search_private_diagnostic_identity_v1()
                != self.authority.private_diagnostic_identity
        {
            return Err(KernelNativeSearchErrorV1::InvalidDecision.into());
        }
        if transition_budget < expected.legal_action_count {
            return Err(
                KernelNativeSearchErrorV1::BudgetSmallerThanRootActionCount {
                    budget: transition_budget,
                    root_actions: expected.legal_action_count,
                }
                .into(),
            );
        }

        let root_player = player_id_v1(expected.acting_player);
        let root_key = search_node_key_v1(session, expected, depth_cap)?;

        // Root prior: one-time, pre-loop evaluation (module docs, "Resolved
        // design point: the root's own prior"). `v_raw` is computed
        // (both heads always arrive together) and discarded.
        let root_forward = evaluator.evaluate_leaf_v1(
            session,
            root_key,
            expected.legal_action_count,
            ModelGuidedSearchLeafSiteV1::RootPrior,
        )?;
        if root_forward.legal_action_weights.len() != expected.legal_action_count as usize {
            return Err(ModelGuidedSearchCoreErrorV1::EvaluatorContract);
        }
        let root_prior = quantize_prior_v1(&root_forward.legal_action_weights)?;

        let mut tree = ModelGuidedSearchTreeV1::new(root_key, root_player, root_prior)?;
        let authoritative_hash_before = session.privileged_core_environment_hash();
        let authoritative_response_before = session.current_response();
        let mut transitions_used = 0u32;
        let mut simulations = 0u32;
        let mut leaf_census = ModelGuidedSearchLeafCensusV1::default();

        while transitions_used < transition_budget {
            let simulation_seed = derive_simulation_seed_v1(
                simulation_seed_digest,
                expected,
                u64::from(simulations),
                root_player,
            );
            let mut sampled = session.kernel_search_redeterminized_clone_v1(simulation_seed)?;
            if search_node_key_v1(&sampled, expected, depth_cap)? != root_key {
                return Err(KernelNativeSearchErrorV1::HiddenStateContract.into());
            }
            let forced_root_action =
                (simulations < expected.legal_action_count).then_some(simulations);
            run_simulation_puct_v1(
                &mut tree,
                &mut sampled,
                root_player,
                depth_cap,
                transition_budget,
                &mut transitions_used,
                forced_root_action,
                evaluator,
                value_domain,
                &mut leaf_census,
            )?;
            simulations = simulations
                .checked_add(1)
                .ok_or(KernelNativeSearchErrorV1::CorruptTree)?;
        }

        if session.privileged_core_environment_hash() != authoritative_hash_before
            || session.current_response() != authoritative_response_before
        {
            return Err(KernelNativeSearchErrorV1::HiddenStateContract.into());
        }
        let root = tree
            .nodes
            .first()
            .ok_or(KernelNativeSearchErrorV1::CorruptTree)?;
        if root.actions.iter().any(|action| action.visits == 0) {
            return Err(KernelNativeSearchErrorV1::CorruptTree.into());
        }
        let selected_index = select_final_root_action_v1(&root.actions)?;
        let root_action_stats = root
            .actions
            .iter()
            .enumerate()
            .map(|(index, stat)| KernelNativeSearchActionStatV1 {
                flat_action_index: index as u32,
                visits: stat.visits,
                value_sum: stat.value_sum,
                mean_value: stat.mean(),
            })
            .collect();
        Ok(ModelGuidedSearchDecisionV1 {
            selected_index,
            transitions_used,
            simulations,
            tree_node_count: u32::try_from(tree.nodes.len())
                .map_err(|_| KernelNativeSearchErrorV1::CorruptTree)?,
            root_action_stats,
            leaf_census,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::PlayerId;
    use crate::kernel_native_search_opponent_v1::{
        KernelNativeSearchTierV1, KERNEL_NATIVE_SEARCH_DEPTH_CAP_V1,
    };
    use crate::model_guided_search_authority_v1::{
        ModelGuidedSearchConsumptionModeV1, MODEL_GUIDED_SEARCH_AUTHORIZED_SEED_BLOCKS_V1,
    };
    use crate::native_policy_value_net_v1::{
        NativeEncodedDecisionSchemaV1, NativePolicyValueModelConfigV1,
    };
    use crate::rl_session::FastActorSessionV1;
    use sha2::{Digest, Sha256};

    fn v2_session_v1(p0: &str, p1: &str, seed: u64) -> FastActorSessionV1 {
        FastActorSessionV1::reset_with_decks_and_limits_flat_action_v2(
            50_101,
            seed,
            512,
            65_536,
            [p0.to_string(), p1.to_string()],
        )
        .unwrap()
    }

    fn authority_v1(tier: KernelNativeSearchTierV1) -> ModelGuidedSearchAuthorityV1 {
        ModelGuidedSearchAuthorityV1::new(
            tier,
            // Was `KERNEL_NATIVE_SEARCH_AUTHORIZED_SEEDS_V1[0]` (v1's own
            // calibration seed), which validated only because this schema
            // had no allowlist of its own. It does now, and the two bands
            // are deliberately disjoint, so this fixture names a
            // model-guided block.
            MODEL_GUIDED_SEARCH_AUTHORIZED_SEED_BLOCKS_V1[0],
            crate::state::DIAGNOSTIC_STATE_HASH_ALGORITHM,
            "D:/mtg-kernel-store/test-lineage",
            0,
            &"1".repeat(64),
            "net8-family/test-v1",
            ModelGuidedSearchConsumptionModeV1::SearchAsOpponent,
        )
        .unwrap()
    }

    fn tree_digest_v1(tree: &ModelGuidedSearchTreeV1) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update((tree.nodes.len() as u64).to_le_bytes());
        for node in &tree.nodes {
            hasher.update(node.key);
            hasher.update([node.actor.0]);
            hasher.update(node.visits.to_le_bytes());
            hasher.update((node.actions.len() as u64).to_le_bytes());
            for action in &node.actions {
                hasher.update(action.visits.to_le_bytes());
                hasher.update(action.value_sum.to_le_bytes());
                hasher.update((action.child_nodes.len() as u64).to_le_bytes());
                for &child in &action.child_nodes {
                    hasher.update((child as u64).to_le_bytes());
                }
            }
            hasher.update((node.prior.len() as u64).to_le_bytes());
            for &p in &node.prior {
                hasher.update(p.to_le_bytes());
            }
        }
        hasher.finalize().into()
    }

    // ---- expansion ordering (synthetic prior set with ties) ----

    #[test]
    fn expansion_prefers_descending_prior_then_ascending_index_among_unvisited_actions() {
        let key = [3u8; 32];
        // Ties at (index 1, index 2) both 500_000, and (index 0, index 3)
        // both 100_000; expected order: 1, 2 (tie -> ascending index), then
        // 4 (200_000), then 0, 3 (tie -> ascending index).
        let prior = vec![100_000, 500_000, 500_000, 100_000, 200_000];
        let mut node = ModelGuidedSearchNodeV1::new(key, PlayerId::P0, prior).expect("node builds");
        let root_player = PlayerId::P0;
        let mut visited_order = Vec::new();
        for _ in 0..5 {
            let index = select_tree_action_puct_v1(&node, root_player).unwrap();
            visited_order.push(index);
            node.actions[index].visits = 1;
            node.visits += 1;
        }
        assert_eq!(visited_order, vec![1, 2, 4, 0, 3]);
    }

    #[test]
    fn puct_selection_favors_higher_prior_action_when_visited_means_and_visits_tie() {
        // Two actions, equal visits and equal mean (so v1's own bonus(a) term
        // is identical for both), but different cached priors. Without the
        // PUCT multiplier this would be an exact tie broken by index (action
        // 0 would win); with it, the higher-prior action 1 must win.
        let key = [9u8; 32];
        let prior = vec![100_000, 900_000];
        let mut node = ModelGuidedSearchNodeV1::new(key, PlayerId::P0, prior).expect("node builds");
        for action in &mut node.actions {
            action.visits = 4;
            action.value_sum = 0;
        }
        node.visits = 8;
        assert_eq!(
            select_tree_action_puct_v1(&node, PlayerId::P0).unwrap(),
            1,
            "higher-prior action must win an otherwise-tied PUCT comparison"
        );
        // With priors swapped, action 0 must win instead: proves the result
        // is driven by the prior, not by some other index-dependent bias.
        node.prior = vec![900_000, 100_000];
        assert_eq!(select_tree_action_puct_v1(&node, PlayerId::P0).unwrap(), 0);
    }

    #[test]
    fn puct_selection_ties_choose_the_lower_index() {
        // Three actions, IDENTICAL priors and identical visits/means, so
        // every score is exactly tied: the lower flat-action index must win
        // (v1's own tie rule, Section 1.1, inherited unchanged), for both a
        // maximizing (root) and a minimizing (opponent) node.
        let key = [11u8; 32];
        let prior = vec![333_334, 333_333, 333_333];
        let mut node = ModelGuidedSearchNodeV1::new(key, PlayerId::P0, prior).expect("node builds");
        for action in &mut node.actions {
            action.visits = 5;
            action.value_sum = 10;
        }
        node.visits = 15;
        assert_eq!(
            select_tree_action_puct_v1(&node, PlayerId::P0).unwrap(),
            0,
            "maximizing node: exact tie must choose the lowest index"
        );
        node.actor = PlayerId::P1;
        assert_eq!(
            select_tree_action_puct_v1(&node, PlayerId::P0).unwrap(),
            0,
            "minimizing node: exact tie must also choose the lowest index"
        );
    }

    #[test]
    fn puct_bonus_hand_worked_example_matches_the_design_formula() {
        // parent_visits=8, child_visits=4 -> bonus(a) via v1's own formula;
        // cross-checked against the item-1 module's own hand-worked value.
        let bonus = integer_ucb_bonus_v1(8, 4);
        // floor(1414*isqrt(1_000_000*(ilog2(9)+1)/4)/1000); ilog2(9)=3, so
        // radicand = 1_000_000*4/4 = 1_000_000, isqrt=1000, bonus=floor(1414)=1414.
        assert_eq!(bonus, 1_414);
        assert_eq!(puct_bonus_v1(bonus, 250_000).unwrap(), 353);
        assert_eq!(puct_bonus_v1(bonus, 1_000_000).unwrap(), bonus);
        assert_eq!(puct_bonus_v1(bonus, 0).unwrap(), 0);
    }

    // ---- leaf dispatch at all three (plus root-prior) sites ----

    #[test]
    fn every_dispatch_site_fires_root_prior_new_node_and_revisited_leaf() {
        let session = v2_session_v1("Rally", "Burn", 41_001);
        let FastActorResponseV1::Decision(decision) = session.current_response() else {
            panic!("reset terminated")
        };
        let searcher = ModelGuidedSearchCoreV1::new(authority_v1(KernelNativeSearchTierV1::T512))
            .expect("authority validates");
        let evaluator = MockLeafEvaluatorV1::new();
        let value_domain = ModelGuidedSearchValueHeadDomainV1::Tanh;
        // A generous budget at a shallow depth cap forces many simulations
        // to revisit existing depth-cap-frontier positions after the first
        // few expand new nodes, exercising all three sites.
        let result = searcher
            .select_action_with_budget_v1(&session, decision, 300, 3, &evaluator, &value_domain)
            .expect("search completes");
        assert!(result.transitions_used > 0);
        assert_eq!(
            evaluator.root_prior_calls.get(),
            1,
            "root prior must be evaluated exactly once, before any simulation"
        );
        assert!(
            evaluator.site2_calls.get() >= 1,
            "at least one newly-expanded-node event must fire"
        );
        assert!(
            evaluator.site3_calls.get() >= 1,
            "at least one revisited-depth-cap-leaf event must fire"
        );
        // Newly-expanded-node count must equal the tree's own node count
        // minus the root (every non-root node is created by exactly one
        // site-2 event).
        assert_eq!(evaluator.site2_calls.get(), result.tree_node_count - 1);
    }

    #[test]
    fn natural_terminal_never_invokes_the_evaluator() {
        // depth_cap=1 with the coverage-forced root action reproduces v1's
        // own `fixed_budget_search_is_repeatable_and_never_mutates_the_episode`
        // shape: shallow enough that no genuine terminal is reachable in one
        // step for this fixture, so this test instead pins the CONTRACT
        // directly: `natural_terminal_value_v1` (reused unchanged from v1)
        // is a pure function with no evaluator parameter at all, so a
        // natural terminal structurally cannot invoke the evaluator ---
        // proven by construction, not by observation.
        let outcome_win = crate::rl::TerminalOutcomeV1::P0Win;
        assert_eq!(
            natural_terminal_value_v1(outcome_win, PlayerId::P0),
            Ok(10_000)
        );
        assert_eq!(
            natural_terminal_value_v1(outcome_win, PlayerId::P1),
            Ok(-10_000)
        );
    }

    // ---- byte-reproducibility ----

    /// Rebuilds a tree by hand (identical seeds/root/loop shape to
    /// `select_action_with_budget_v1`) so its full digest can be inspected,
    /// not just the public `ModelGuidedSearchDecisionV1` projection of it.
    fn build_tree_for_test_v1<E: ModelGuidedSearchLeafEvaluatorV1>(
        session: &FastActorSessionV1,
        decision: FastActorDecisionV1,
        depth_cap: u16,
        transition_budget: u32,
        authority_digest: [u8; 32],
        evaluator: &E,
        value_domain: &ModelGuidedSearchValueHeadDomainV1,
    ) -> ModelGuidedSearchTreeV1 {
        let root_player = player_id_v1(decision.acting_player);
        let root_key = search_node_key_v1(session, decision, depth_cap).unwrap();
        let root_forward = evaluator
            .evaluate_leaf_v1(
                session,
                root_key,
                decision.legal_action_count,
                ModelGuidedSearchLeafSiteV1::RootPrior,
            )
            .unwrap();
        let root_prior = quantize_prior_v1(&root_forward.legal_action_weights).unwrap();
        let mut tree = ModelGuidedSearchTreeV1::new(root_key, root_player, root_prior).unwrap();
        let mut transitions_used = 0u32;
        let mut simulations = 0u32;
        while transitions_used < transition_budget {
            let simulation_seed = derive_simulation_seed_v1(
                authority_digest,
                decision,
                u64::from(simulations),
                root_player,
            );
            let mut sampled = session
                .kernel_search_redeterminized_clone_v1(simulation_seed)
                .unwrap();
            let forced_root_action =
                (simulations < decision.legal_action_count).then_some(simulations);
            run_simulation_puct_v1(
                &mut tree,
                &mut sampled,
                root_player,
                depth_cap,
                transition_budget,
                &mut transitions_used,
                forced_root_action,
                evaluator,
                value_domain,
                &mut ModelGuidedSearchLeafCensusV1::default(),
            )
            .unwrap();
            simulations += 1;
        }
        tree
    }

    #[test]
    fn same_seed_state_two_runs_are_byte_identical_in_decision_and_tree_digest() {
        let session = v2_session_v1("Rally", "Burn", 41_002);
        let FastActorResponseV1::Decision(decision) = session.current_response() else {
            panic!("reset terminated")
        };
        let searcher = ModelGuidedSearchCoreV1::new(authority_v1(KernelNativeSearchTierV1::T512))
            .expect("authority validates");
        let value_domain = ModelGuidedSearchValueHeadDomainV1::Tanh;
        let depth_cap = 8u16;
        let transition_budget = 128u32;
        let authority_digest = searcher.authority().digest().unwrap();

        let decision_a = searcher
            .select_action_with_budget_v1(
                &session,
                decision,
                transition_budget,
                depth_cap,
                &MockLeafEvaluatorV1::new(),
                &value_domain,
            )
            .unwrap();
        let decision_b = searcher
            .select_action_with_budget_v1(
                &session,
                decision,
                transition_budget,
                depth_cap,
                &MockLeafEvaluatorV1::new(),
                &value_domain,
            )
            .unwrap();
        assert_eq!(
            decision_a, decision_b,
            "selected action and full visit distribution must be byte-identical"
        );

        let tree_a = build_tree_for_test_v1(
            &session,
            decision,
            depth_cap,
            transition_budget,
            authority_digest,
            &MockLeafEvaluatorV1::new(),
            &value_domain,
        );
        let tree_b = build_tree_for_test_v1(
            &session,
            decision,
            depth_cap,
            transition_budget,
            authority_digest,
            &MockLeafEvaluatorV1::new(),
            &value_domain,
        );
        assert_eq!(
            tree_digest_v1(&tree_a),
            tree_digest_v1(&tree_b),
            "internal tree digest must be byte-identical"
        );
    }

    // ---- v1's inherited redetermination behavior, mirrored ----

    #[test]
    fn redetermination_canonicalizes_a_surface_arrangement_difference_before_search() {
        // Mirrors kernel_native_search_opponent_v1's own
        // `unknown_arrangement_is_canonicalized_before_sampling_and_known_cards_lock`
        // test shape: two sessions differing only in an UNKNOWN hidden-zone
        // arrangement swap must search identically, because
        // `redeterminize_hidden_zones_v1` (inherited unchanged, called via
        // `kernel_search_redeterminized_clone_v1`, itself called unchanged
        // by this module) canonicalizes unknown definitions before sampling.
        let a = v2_session_v1("Rally", "Burn", 81_101);
        let mut b = a.clone();
        let actor = player_id_v1(match a.current_response() {
            FastActorResponseV1::Decision(decision) => decision.acting_player,
            FastActorResponseV1::Terminal(_) => panic!("reset terminated"),
        });
        let opponent = actor.opponent();
        let unknown_hand = a.kernel_search_state_v1().players[opponent.index()].hand[0];
        let unknown_library = a.kernel_search_state_v1().players[opponent.index()].library[0];
        let a_hand_def = a
            .kernel_search_state_v1()
            .objects
            .get(unknown_hand)
            .card_def;
        let a_library_def = a
            .kernel_search_state_v1()
            .objects
            .get(unknown_library)
            .card_def;
        assert_ne!(
            a_hand_def, a_library_def,
            "the arrangement-independence witness must swap distinct definitions"
        );
        {
            let state = b.kernel_search_state_mut_for_test_v1();
            state.objects.get_mut(unknown_hand).card_def = a_library_def;
            state.objects.get_mut(unknown_hand).name = crate::card_def::CARD_DEFS
                [a_library_def as usize]
                .name
                .to_string();
            state.objects.get_mut(unknown_hand).v4 =
                crate::state::ObjectStateV4::from_card_def(a_library_def);
            state.objects.get_mut(unknown_library).card_def = a_hand_def;
            state.objects.get_mut(unknown_library).name = crate::card_def::CARD_DEFS
                [a_hand_def as usize]
                .name
                .to_string();
            state.objects.get_mut(unknown_library).v4 =
                crate::state::ObjectStateV4::from_card_def(a_hand_def);
        }
        let FastActorResponseV1::Decision(decision_a) = a.current_response() else {
            panic!("reset terminated")
        };
        let FastActorResponseV1::Decision(decision_b) = b.current_response() else {
            panic!("reset terminated")
        };
        assert_eq!(decision_a, decision_b);

        let searcher = ModelGuidedSearchCoreV1::new(authority_v1(KernelNativeSearchTierV1::T512))
            .expect("authority validates");
        let value_domain = ModelGuidedSearchValueHeadDomainV1::Tanh;
        let evaluator_a = MockLeafEvaluatorV1::new();
        let evaluator_b = MockLeafEvaluatorV1::new();
        let result_a = searcher
            .select_action_with_budget_v1(&a, decision_a, 64, 6, &evaluator_a, &value_domain)
            .unwrap();
        let result_b = searcher
            .select_action_with_budget_v1(&b, decision_b, 64, 6, &evaluator_b, &value_domain)
            .unwrap();
        assert_eq!(
            result_a, result_b,
            "an unknown-arrangement-only difference must not change the search result"
        );
    }

    #[test]
    fn tampered_post_redetermination_hand_corruption_fails_closed_through_this_search_loop() {
        // Mirrors v1's own
        // `redeterminized_clone_rejects_a_genuine_post_redetermination_hand_corruption`:
        // proves the SAME rejection is reachable from this module's own
        // search entry point, not merely trusted to exist elsewhere. Uses
        // the cfg(test)-only corruption hook one simulation would encounter.
        let session = v2_session_v1("Rally", "Burn", 81_102);
        let FastActorResponseV1::Decision(decision) = session.current_response() else {
            panic!("reset terminated")
        };
        let corrupted =
            session.kernel_search_redeterminized_clone_for_test_v1(1, |state, actor| {
                let &hand_object = state.players[actor.index()]
                    .hand
                    .first()
                    .expect("acting player holds at least one card at game start");
                let object = state.objects.get_mut(hand_object);
                let corrupted_def = (object.card_def + 1) % crate::card_def::CARD_DEFS.len() as u16;
                object.card_def = corrupted_def;
                object.name = crate::card_def::CARD_DEFS[corrupted_def as usize]
                    .name
                    .to_string();
                object.v4 = crate::state::ObjectStateV4::from_card_def(corrupted_def);
            });
        match corrupted {
            Err(err) => assert_eq!(err, KernelNativeSearchErrorV1::HiddenStateContract),
            Ok(_) => {
                panic!("a post-redetermination hand corruption must be rejected, not admitted")
            }
        }
        // decision is still usable afterward: the authoritative episode was
        // never touched by the rejected sample.
        assert_eq!(
            session.current_response(),
            FastActorResponseV1::Decision(decision)
        );
    }

    // ---- end-to-end search on a real, definition-owned game state ----

    #[test]
    fn end_to_end_search_over_real_runtime_decks_selects_a_valid_root_action() {
        let session = v2_session_v1("Rally", "Burn", 41_003);
        let FastActorResponseV1::Decision(decision) = session.current_response() else {
            panic!("reset terminated")
        };
        let searcher = ModelGuidedSearchCoreV1::new(authority_v1(KernelNativeSearchTierV1::T512))
            .expect("authority validates");
        let evaluator = MockLeafEvaluatorV1::new();
        let value_domain = ModelGuidedSearchValueHeadDomainV1::Tanh;
        let result = searcher
            .select_action_v1(&session, decision, &evaluator, &value_domain)
            .expect("full-tier search completes");
        assert_eq!(result.transitions_used, 512);
        assert!(result.selected_index < decision.legal_action_count);
        assert_eq!(
            result.root_action_stats.len(),
            decision.legal_action_count as usize
        );
        assert!(result.root_action_stats.iter().all(|stat| stat.visits > 0));
        // Session must remain untouched (no-mutation guarantee, inherited).
        assert_eq!(
            session.current_response(),
            FastActorResponseV1::Decision(decision)
        );
    }

    // ---- outcome-schema-V3 instrumentation (test-time-search S0) ----

    /// The four leaf counts partition the simulations exactly: every
    /// simulation ends at one and only one of them. A consumer of the
    /// diagnostics record relies on this to read "terminal-leaf fraction"
    /// off the census without a second source of truth.
    #[test]
    fn leaf_census_partitions_the_simulation_count_exactly() {
        let session = v2_session_v1("Rally", "Burn", 41_501);
        let FastActorResponseV1::Decision(decision) = session.current_response() else {
            panic!("reset terminated")
        };
        let searcher = ModelGuidedSearchCoreV1::new(authority_v1(KernelNativeSearchTierV1::T512))
            .expect("authority validates");
        let evaluator = MockLeafEvaluatorV1::new();
        let value_domain = ModelGuidedSearchValueHeadDomainV1::Tanh;
        let result = searcher
            .select_action_v1(&session, decision, &evaluator, &value_domain)
            .expect("full-tier search completes");
        let census = result.leaf_census;
        assert_eq!(
            census.natural_terminal_leaves
                + census.truncated_terminal_leaves
                + census.newly_expanded_leaves
                + census.depth_cap_leaves,
            result.simulations,
            "every simulation must end at exactly one leaf class"
        );
        assert!(census.max_simulation_depth <= KERNEL_NATIVE_SEARCH_DEPTH_CAP_V1);
        assert!(
            census.summed_simulation_depth >= u64::from(census.max_simulation_depth),
            "the depth sum must cover at least the deepest simulation"
        );
        // The census is instrumentation: it cannot have changed the
        // verdict, so a search with the same inputs still agrees with
        // itself.
        let again = searcher
            .select_action_v1(
                &session,
                decision,
                &MockLeafEvaluatorV1::new(),
                &value_domain,
            )
            .expect("full-tier search completes");
        assert_eq!(result, again);
    }

    /// The diagnostic stability halves must be genuinely independent of
    /// the full-budget run and of each other, and must never disturb it.
    #[test]
    fn stability_halves_are_domain_separated_and_do_not_perturb_the_full_run() {
        let authority = authority_v1(KernelNativeSearchTierV1::T512);
        let authority_digest = authority.digest().expect("authority digests");
        let half_a = ModelGuidedSearchSeedHalfV1::A.simulation_seed_digest_v1(authority_digest);
        let half_b = ModelGuidedSearchSeedHalfV1::B.simulation_seed_digest_v1(authority_digest);
        assert_ne!(
            half_a, authority_digest,
            "half A must not reuse the authority digest"
        );
        assert_ne!(
            half_b, authority_digest,
            "half B must not reuse the authority digest"
        );
        assert_ne!(half_a, half_b, "the two halves must be independent");
        assert_eq!(ModelGuidedSearchSeedHalfV1::A.tag_v1(), "half-a");
        assert_eq!(ModelGuidedSearchSeedHalfV1::B.tag_v1(), "half-b");

        let session = v2_session_v1("Rally", "Burn", 41_502);
        let FastActorResponseV1::Decision(decision) = session.current_response() else {
            panic!("reset terminated")
        };
        let searcher = ModelGuidedSearchCoreV1::new(authority).expect("authority validates");
        let value_domain = ModelGuidedSearchValueHeadDomainV1::Tanh;
        let full_before = searcher
            .select_action_v1(
                &session,
                decision,
                &MockLeafEvaluatorV1::new(),
                &value_domain,
            )
            .expect("full search completes");
        let a = searcher
            .select_action_seed_half_v1(
                &session,
                decision,
                &MockLeafEvaluatorV1::new(),
                &value_domain,
                ModelGuidedSearchSeedHalfV1::A,
            )
            .expect("half A completes");
        let b = searcher
            .select_action_seed_half_v1(
                &session,
                decision,
                &MockLeafEvaluatorV1::new(),
                &value_domain,
                ModelGuidedSearchSeedHalfV1::B,
            )
            .expect("half B completes");
        // Half budget, and never below the root action count (the search's
        // own coverage rule would otherwise reject the decision).
        assert!(a.transitions_used >= decision.legal_action_count);
        assert_eq!(a.transitions_used, 256.max(decision.legal_action_count));
        assert_eq!(b.transitions_used, a.transitions_used);
        // Different seed sources really do produce different searches.
        assert_ne!(a.root_action_stats, b.root_action_stats);
        let full_after = searcher
            .select_action_v1(
                &session,
                decision,
                &MockLeafEvaluatorV1::new(),
                &value_domain,
            )
            .expect("full search completes");
        assert_eq!(
            full_before, full_after,
            "running the halves must not perturb the full-budget result"
        );
    }

    // ---- v1's own error type is reused, not re-derived ----

    #[test]
    fn budget_smaller_than_root_count_fails_closed_via_reused_v1_error_variant() {
        let session = v2_session_v1("Rally", "Burn", 41_004);
        let FastActorResponseV1::Decision(decision) = session.current_response() else {
            panic!("reset terminated")
        };
        let searcher = ModelGuidedSearchCoreV1::new(authority_v1(KernelNativeSearchTierV1::T512))
            .expect("authority validates");
        let evaluator = MockLeafEvaluatorV1::new();
        let value_domain = ModelGuidedSearchValueHeadDomainV1::Tanh;
        assert_eq!(
            searcher.select_action_with_budget_v1(
                &session,
                decision,
                decision.legal_action_count - 1,
                8,
                &evaluator,
                &value_domain,
            ),
            Err(ModelGuidedSearchCoreErrorV1::Tree(
                KernelNativeSearchErrorV1::BudgetSmallerThanRootActionCount {
                    budget: decision.legal_action_count - 1,
                    root_actions: decision.legal_action_count,
                }
            ))
        );
    }

    // ---- real-forward seam: minimal, direct tests (item 5's own scope; see
    // module docs "Resolved scope boundary") ----

    fn minimal_encoded_view_v1<'a>(
        state: &'a [f32],
        object_features: &'a [f32],
        object_card_ids: &'a [i64],
        object_groups: &'a [i64],
        object_node_ids: &'a [i64],
        action_features: &'a [f32],
    ) -> NativeEncodedDecisionViewV1<'a> {
        NativeEncodedDecisionViewV1::from_slices_unvalidated(
            NativeEncodedDecisionSchemaV1::contract_v1(),
            state,
            object_features,
            object_card_ids,
            object_groups,
            object_node_ids,
            &[],
            &[],
            &[],
            action_features,
            &[],
            &[],
            &[],
            &[],
        )
    }

    #[test]
    fn real_forward_seam_is_deterministic_and_value_quantization_is_antisymmetric() {
        let config = NativePolicyValueModelConfigV1::contract_v1();
        let model = NativePolicyValueNetV1::runner_fixed_v1(config).expect("model builds");
        let state = vec![0.0f32; config.state_dim];
        let object_features = vec![0.0f32; config.object_feature_dim];
        let object_card_ids = vec![0i64];
        let object_groups = vec![0i64];
        let object_node_ids = vec![0i64];
        let action_features = vec![0.0f32; config.action_feature_dim * 2];
        let encoded = minimal_encoded_view_v1(
            &state,
            &object_features,
            &object_card_ids,
            &object_groups,
            &object_node_ids,
            &action_features,
        );

        // The runner-fixed test model's raw value output is not guaranteed
        // to lie in Tanh's analytic [-1.0, 1.0] domain (it has no final
        // squashing activation on this path either -- see the
        // value-quantization module's own TBD-AT-IMPLEMENTATION note); this
        // test's own goal is only to prove the SEAM (real forward -> value
        // quantization) is wired and deterministic, so it uses the
        // `Calibrated` domain sized generously around the model's own raw
        // output rather than asserting a production domain choice.
        let raw = model
            .forward_search_deterministic_v1(encoded)
            .expect("forward succeeds")
            .value;
        let bound = raw.abs().max(1.0) * 4.0;
        let domain = ModelGuidedSearchValueHeadDomainV1::Calibrated {
            lower: -bound,
            upper: bound,
        };
        let evaluator = ModelGuidedSearchRealForwardValueEvaluatorV1::new(&model, domain);
        let root_value = evaluator
            .evaluate_encoded_value_v1(encoded, true)
            .expect("value quantizes");
        let opponent_value = evaluator
            .evaluate_encoded_value_v1(encoded, false)
            .expect("value quantizes");
        assert_eq!(
            root_value, -opponent_value,
            "perspective flip must be an exact negation through the real forward"
        );
        let root_value_again = evaluator
            .evaluate_encoded_value_v1(encoded, true)
            .expect("value quantizes");
        assert_eq!(
            root_value, root_value_again,
            "two calls over the same encoded view must be byte-identical"
        );
    }

    // ---- item 6a: the live-session encoder bridge ----

    fn real_model_for_test_v1() -> NativePolicyValueNetV1 {
        NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
            .expect("runner-fixed model builds")
    }

    #[test]
    fn encoding_the_same_live_leaf_twice_is_byte_identical() {
        let session = v2_session_v1("Rally", "Burn", 41_101);
        let FastActorResponseV1::Decision(decision) = session.current_response() else {
            panic!("reset terminated")
        };
        let tensor_a = encode_live_decision_tensor_v1(&session, decision).expect("encodes");
        let tensor_b = encode_live_decision_tensor_v1(&session, decision).expect("encodes");
        assert_eq!(
            tensor_a, tensor_b,
            "encoding the same live leaf twice must be byte-identical"
        );
    }

    #[test]
    fn real_forward_evaluator_makes_exactly_one_forward_call_per_leaf_event() {
        let session = v2_session_v1("Rally", "Burn", 41_102);
        let FastActorResponseV1::Decision(decision) = session.current_response() else {
            panic!("reset terminated")
        };
        let searcher = ModelGuidedSearchCoreV1::new(authority_v1(KernelNativeSearchTierV1::T512))
            .expect("authority validates");
        let model = real_model_for_test_v1();
        let value_domain = ModelGuidedSearchValueHeadDomainV1::Calibrated {
            lower: -8.0,
            upper: 8.0,
        };
        let evaluator = ModelGuidedSearchRealForwardValueEvaluatorV1::new(&model, value_domain);
        let transition_budget = decision
            .legal_action_count
            .saturating_mul(3)
            .saturating_add(8);
        let result = searcher
            .select_action_with_budget_v1(
                &session,
                decision,
                transition_budget,
                3,
                &evaluator,
                &value_domain,
            )
            .expect("real-model search completes");
        assert!(result.transitions_used > 0);
        assert!(
            evaluator.leaf_calls_v1() > 1,
            "a real, multi-simulation search must dispatch more than the root-prior call alone"
        );
        assert_eq!(
            evaluator.leaf_calls_v1(),
            evaluator.forward_calls_v1(),
            "every leaf-evaluation event must cost exactly one forward call, both heads \
             together (Section 1.3's heads_per_leaf economy), never zero and never two"
        );
    }

    #[test]
    fn end_to_end_search_with_real_model_is_byte_reproducible_and_selects_a_legal_action() {
        let session = v2_session_v1("Rally", "Burn", 41_103);
        let FastActorResponseV1::Decision(decision) = session.current_response() else {
            panic!("reset terminated")
        };
        let searcher = ModelGuidedSearchCoreV1::new(authority_v1(KernelNativeSearchTierV1::T512))
            .expect("authority validates");
        let model = real_model_for_test_v1();
        let value_domain = ModelGuidedSearchValueHeadDomainV1::Calibrated {
            lower: -8.0,
            upper: 8.0,
        };
        let transition_budget = decision
            .legal_action_count
            .saturating_mul(3)
            .saturating_add(8);
        let depth_cap = 3u16;

        let evaluator_a = ModelGuidedSearchRealForwardValueEvaluatorV1::new(&model, value_domain);
        let result_a = searcher
            .select_action_with_budget_v1(
                &session,
                decision,
                transition_budget,
                depth_cap,
                &evaluator_a,
                &value_domain,
            )
            .expect("real-model search completes");

        let evaluator_b = ModelGuidedSearchRealForwardValueEvaluatorV1::new(&model, value_domain);
        let result_b = searcher
            .select_action_with_budget_v1(
                &session,
                decision,
                transition_budget,
                depth_cap,
                &evaluator_b,
                &value_domain,
            )
            .expect("real-model search completes");

        assert_eq!(
            result_a, result_b,
            "two runs of the real-model search over the same seed/state must be byte-identical"
        );
        assert!(result_a.selected_index < decision.legal_action_count);
        // No-mutation guarantee, inherited: the original session must remain
        // untouched by either run.
        assert_eq!(
            session.current_response(),
            FastActorResponseV1::Decision(decision)
        );
    }

    #[test]
    fn encoding_two_different_live_leaves_produces_different_tensors() {
        // Item 6a's own "encode-view binding" mutation boundary: proves the
        // encode genuinely reflects the specific live decision passed in
        // (not, say, a stale or constant placeholder that would happen to
        // pass the byte-identical-reencode test above vacuously). Two
        // distinct seeds' opening decisions must not encode identically.
        let session_a = v2_session_v1("Rally", "Burn", 41_105);
        let FastActorResponseV1::Decision(decision_a) = session_a.current_response() else {
            panic!("reset terminated")
        };
        let session_b = v2_session_v1("Rally", "Burn", 41_106);
        let FastActorResponseV1::Decision(decision_b) = session_b.current_response() else {
            panic!("reset terminated")
        };
        let tensor_a = encode_live_decision_tensor_v1(&session_a, decision_a).expect("encodes");
        let tensor_b = encode_live_decision_tensor_v1(&session_b, decision_b).expect("encodes");
        assert_ne!(
            tensor_a, tensor_b,
            "two different live leaves must not encode to the same tensor"
        );
    }

    #[test]
    #[cfg(target_arch = "x86_64")]
    fn real_forward_evaluator_panics_through_the_real_mxcsr_gate_when_dirty() {
        // Item 6a's own "gate presence" mutation boundary: proves THIS
        // evaluator's own dispatch genuinely reaches
        // `forward_search_deterministic_v1`'s MXCSR gate (not, say, a
        // mistaken call to the ungated production `forward_v1`), mirroring
        // `native_policy_value_net_v1`'s own item-4 proof that the gate is
        // wired into the real entry point, but exercised through item 6a's
        // own evaluator and a real live session/decision instead of that
        // file's synthetic fixture.
        let handle = std::thread::spawn(|| {
            let original = crate::deterministic_math_v1::read_mxcsr_v1();
            crate::deterministic_math_v1::write_mxcsr_v1(original | (1 << 15)); // dirty FTZ
            let result = std::panic::catch_unwind(|| {
                let session = v2_session_v1("Rally", "Burn", 41_107);
                let FastActorResponseV1::Decision(decision) = session.current_response() else {
                    panic!("reset terminated")
                };
                let model = real_model_for_test_v1();
                let value_domain = ModelGuidedSearchValueHeadDomainV1::Calibrated {
                    lower: -8.0,
                    upper: 8.0,
                };
                let evaluator =
                    ModelGuidedSearchRealForwardValueEvaluatorV1::new(&model, value_domain);
                evaluator.evaluate_leaf_v1(
                    &session,
                    [0u8; 32],
                    decision.legal_action_count,
                    ModelGuidedSearchLeafSiteV1::RootPrior,
                )
            });
            crate::deterministic_math_v1::write_mxcsr_v1(original);
            assert!(
                result.is_err(),
                "expected the real-forward evaluator to panic through the MXCSR gate \
                 when FTZ is dirty"
            );
        });
        handle
            .join()
            .expect("evaluator mxcsr gate thread must not panic");
    }

    #[test]
    fn real_forward_evaluator_fails_closed_with_no_live_decision_to_encode() {
        // Module docs, "Determination: the real-forward evaluator's
        // live-decision requirement": the one case the real-forward
        // evaluator cannot encode is a session whose `current_response()`
        // is `Terminal`, not `Decision` (the `TerminalClassificationV1::
        // Truncated` synthetic-key dispatch). This test proves the
        // documented failure mode directly, on a session actually at a
        // terminal, rather than only asserting it in prose.
        let mut session = v2_session_v1("Rally", "Burn", 41_104);
        let model = real_model_for_test_v1();
        let value_domain = ModelGuidedSearchValueHeadDomainV1::Calibrated {
            lower: -8.0,
            upper: 8.0,
        };
        let evaluator = ModelGuidedSearchRealForwardValueEvaluatorV1::new(&model, value_domain);
        loop {
            match session.current_response() {
                FastActorResponseV1::Terminal(_) => break,
                FastActorResponseV1::Decision(decision) => {
                    let binding = session
                        .native_full_trajectory_current_binding_v2(decision)
                        .expect("binding available");
                    session
                        .consume_current_flat_action_slice_v2(binding, 0)
                        .expect("action consumes");
                }
            }
        }
        let result = evaluator.evaluate_leaf_v1(
            &session,
            [0u8; 32],
            1,
            ModelGuidedSearchLeafSiteV1::RevisitedDepthCapLeaf,
        );
        match result {
            Err(ModelGuidedSearchCoreErrorV1::NoLiveDecisionToEncode) => {}
            other => panic!("expected NoLiveDecisionToEncode, got {other:?}"),
        }
    }
}
