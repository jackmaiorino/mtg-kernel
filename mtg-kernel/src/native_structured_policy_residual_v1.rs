//! Trimmed port (`fable/shadow-scorer-on-main-v1`) of the composed-factorial-v1
//! lineage's `native_structured_policy_residual_v1.rs`. That module (1934
//! lines on the source lineage) is a strict live-inference loader for a
//! structured-object-action-attention policy/value residual family: parsing
//! and validating a candidate manifest, loading its weight file, and
//! constructing a `NativeStructuredPolicyResidualInferenceV1` handle. None of
//! that loading/inference machinery is reachable from this bin's four used
//! entry points (`run_checkpoint_shadow_stdio_v1` and the three
//! `..._with_xmage_cp7_*_jsonl_v1` wrappers), which all resolve through the
//! standard `ShadowScorerServiceV1::load_v1` -> `load_checkpoint_v1` path,
//! never through the structured-residual candidate loader.
//!
//! What IS load-bearing on the kept path: `NativeStructuredHistoryEntryV1`
//! itself is threaded through the crate-wide `ShadowModelScorerV1::score_v1`
//! trait signature (`history: &[NativeStructuredHistoryEntryV1]`) and is
//! genuinely constructed by `StructuredHistoryStateV1` inside
//! `native_checkpoint_shadow_stdio_v1.rs` for every session regardless of
//! which model scorer is active (the default scorer simply ignores the
//! parameter). This file ports exactly that struct, its `new_v1` and
//! `actor_relative_features_v1` methods, and the handful of constants they
//! depend on, byte-identical to the source lineage's definitions. Everything
//! else in the original module (`CandidateV1` parsing, `TensorV1`,
//! `NativeStructuredPolicyResidualInferenceV1`,
//! `load_native_structured_policy_residual_inference_v1`, and
//! `PARENT_NATIVE_STATE_SHA256_V1`) is intentionally omitted: it was used
//! only by `ShadowScorerServiceV1::load_v1`'s `XmageCp7OutcomeDerivative`
//! bounded-value-search branch, which this port removes (see the scorer
//! module's own doc comment and the branch's replacement with
//! `load_checkpoint_v1`'s existing unconditional rejection of that
//! authority variant).

use crate::native_flat_tensorizer_v2::NATIVE_FLAT_ACTION_EXPLICIT_FEATURE_DIM_V2;

pub(crate) const CARD_VOCAB_V1: usize = 136;
pub(crate) const HISTORY_LENGTH_V1: usize = 16;
pub(crate) const HISTORY_ROLE_DIM_V1: usize = 2;
pub(crate) const HISTORY_FEATURE_DIM_V1: usize =
    NATIVE_FLAT_ACTION_EXPLICIT_FEATURE_DIM_V2 + HISTORY_ROLE_DIM_V1 + CARD_VOCAB_V1;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NativeStructuredHistoryEntryV1 {
    acting_player: u8,
    action_explicit_features: [f32; NATIVE_FLAT_ACTION_EXPLICIT_FEATURE_DIM_V2],
    public_card_histogram: [f32; CARD_VOCAB_V1],
}

impl NativeStructuredHistoryEntryV1 {
    pub(crate) fn new_v1(
        acting_player: u8,
        action_explicit_features: [f32; NATIVE_FLAT_ACTION_EXPLICIT_FEATURE_DIM_V2],
        public_card_histogram: [f32; CARD_VOCAB_V1],
    ) -> Result<Self, ()> {
        if acting_player > 1
            || action_explicit_features
                .iter()
                .any(|value| !value.is_finite())
            || public_card_histogram.iter().any(|value| !value.is_finite())
        {
            return Err(());
        }
        Ok(Self {
            acting_player,
            action_explicit_features,
            public_card_histogram,
        })
    }

    pub(crate) fn actor_relative_features_v1(&self, acting_player: u8) -> Result<Vec<f32>, ()> {
        if acting_player > 1 {
            return Err(());
        }
        let mut features = Vec::with_capacity(HISTORY_FEATURE_DIM_V1);
        features.extend_from_slice(&self.action_explicit_features);
        features.push(f32::from(self.acting_player == acting_player));
        features.push(f32::from(self.acting_player != acting_player));
        features.extend_from_slice(&self.public_card_histogram);
        if features.len() != HISTORY_FEATURE_DIM_V1 {
            return Err(());
        }
        Ok(features)
    }
}
