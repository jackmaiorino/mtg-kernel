use crate::{
    CheckedUntrustedMtgoFirstMainReconstructionRefinementV1, MtgoContractErrorV1,
    MtgoObservationReconstructionGroupV1,
};
use mtg_kernel::policy_surface_v5::PolicySurfaceStageV5;
use mtg_kernel::rl::{
    EngineContextV2, EngineDecisionStageV2, HarnessSurfaceContextV2, PolicySurfaceContextV5,
    SurfaceDecisionStageV2,
};
use sha2::{Digest, Sha256};

const FIRST_MAIN_KERNEL_CONTEXT_DOMAIN_V1: &[u8] = b"mtgo-first-main-kernel-context-v1";
const SOLITAIRE_FIRST_MAIN_HAND_COUNT_V1: u8 = 8;
const KERNEL_STARTING_PLAYER_FIRST_MAIN_HAND_COUNT_V1: u8 = 7;

const EXPECTED_INPUT_BLOCKERS_V1: [MtgoObservationReconstructionGroupV1; 4] = [
    MtgoObservationReconstructionGroupV1::DuelParticipants,
    MtgoObservationReconstructionGroupV1::PlayerPublicState,
    MtgoObservationReconstructionGroupV1::PublicObjectsAndZones,
    MtgoObservationReconstructionGroupV1::KernelDecisionHistoryContext,
];

const REMAINING_BLOCKERS_V1: [MtgoObservationReconstructionGroupV1; 3] = [
    MtgoObservationReconstructionGroupV1::DuelParticipants,
    MtgoObservationReconstructionGroupV1::PlayerPublicState,
    MtgoObservationReconstructionGroupV1::PublicObjectsAndZones,
];

/// Kernel-version-bound reset context for the exact supported Turn 1
/// first-main Solitaire slice.
///
/// The template is tested against the kernel's own first policy decision. It
/// records the reset engine, harness-surface, and policy-surface commitments
/// without exposing those structs. Only the context shape transfers. MTGO
/// Solitaire reaches this screen with eight cards, while the kernel's trained
/// two-player starting player correctly skips the first-turn draw and has
/// seven. Distinct opponent state also remains absent, so the result cannot
/// create an `ObservationV5`, a model request, or input.
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoFirstMainKernelContextV1;
/// fn cannot_extract_context(value: &CheckedUntrustedMtgoFirstMainKernelContextV1) {
///     let _ = value.engine_context();
/// }
/// ```
pub struct CheckedUntrustedMtgoFirstMainKernelContextV1 {
    source_manifest_sha256: String,
    source_frame_sha256: String,
    source_reconstruction_commitment_sha256: String,
    engine_context_sha256: String,
    surface_context_sha256: String,
    policy_surface_context_sha256: String,
    context_commitment_sha256: String,
}

impl CheckedUntrustedMtgoFirstMainKernelContextV1 {
    pub fn source_manifest_sha256(&self) -> &str {
        &self.source_manifest_sha256
    }

    pub fn source_frame_sha256(&self) -> &str {
        &self.source_frame_sha256
    }

    pub fn source_reconstruction_commitment_sha256(&self) -> &str {
        &self.source_reconstruction_commitment_sha256
    }

    pub fn engine_context_sha256(&self) -> &str {
        &self.engine_context_sha256
    }

    pub fn surface_context_sha256(&self) -> &str {
        &self.surface_context_sha256
    }

    pub fn policy_surface_context_sha256(&self) -> &str {
        &self.policy_surface_context_sha256
    }

    pub fn resolved_group(&self) -> MtgoObservationReconstructionGroupV1 {
        MtgoObservationReconstructionGroupV1::KernelDecisionHistoryContext
    }

    pub fn remaining_blocking_groups(&self) -> &[MtgoObservationReconstructionGroupV1] {
        &REMAINING_BLOCKERS_V1
    }

    pub fn observation_complete(&self) -> bool {
        false
    }

    pub fn ready_for_model_scoring(&self) -> bool {
        false
    }

    pub fn context_commitment_sha256(&self) -> &str {
        &self.context_commitment_sha256
    }

    pub fn solitaire_source_hand_count(&self) -> u8 {
        SOLITAIRE_FIRST_MAIN_HAND_COUNT_V1
    }

    pub fn kernel_starting_player_first_main_hand_count(&self) -> u8 {
        KERNEL_STARTING_PLAYER_FIRST_MAIN_HAND_COUNT_V1
    }

    pub fn full_decision_compatible_with_two_player_starting_first_main(&self) -> bool {
        false
    }

    pub fn safe_for_input(&self) -> bool {
        false
    }
}

pub fn derive_checked_untrusted_first_main_kernel_context_v1(
    reconstruction: &CheckedUntrustedMtgoFirstMainReconstructionRefinementV1,
) -> Result<CheckedUntrustedMtgoFirstMainKernelContextV1, MtgoContractErrorV1> {
    if reconstruction.remaining_blocking_groups() != EXPECTED_INPUT_BLOCKERS_V1
        || !reconstruction.supported_first_main_action_set_complete()
        || reconstruction.observation_complete()
        || reconstruction.ready_for_model_scoring()
    {
        return Err(MtgoContractErrorV1::new(
            "first_main_kernel_context_source_mismatch",
            "the exact four-blocker supported first-main refinement is required",
        ));
    }

    let engine_context = initial_engine_context_v1();
    let surface_context = initial_surface_context_v1();
    let policy_surface_context = initial_policy_surface_context_v1();
    let engine_context_sha256 = serialized_commitment_v1(b"engine_context", &engine_context)?;
    let surface_context_sha256 = serialized_commitment_v1(b"surface_context", &surface_context)?;
    let policy_surface_context_sha256 =
        serialized_commitment_v1(b"policy_surface_context", &policy_surface_context)?;

    let mut hasher = Sha256::new();
    hasher.update(FIRST_MAIN_KERNEL_CONTEXT_DOMAIN_V1);
    for part in [
        reconstruction.source_manifest_sha256().as_bytes(),
        reconstruction.source_frame_sha256().as_bytes(),
        reconstruction.refinement_commitment_sha256().as_bytes(),
        engine_context_sha256.as_bytes(),
        surface_context_sha256.as_bytes(),
        policy_surface_context_sha256.as_bytes(),
    ] {
        hasher.update((part.len() as u64).to_le_bytes());
        hasher.update(part);
    }

    Ok(CheckedUntrustedMtgoFirstMainKernelContextV1 {
        source_manifest_sha256: reconstruction.source_manifest_sha256().to_owned(),
        source_frame_sha256: reconstruction.source_frame_sha256().to_owned(),
        source_reconstruction_commitment_sha256: reconstruction
            .refinement_commitment_sha256()
            .to_owned(),
        engine_context_sha256,
        surface_context_sha256,
        policy_surface_context_sha256,
        context_commitment_sha256: format!("{:x}", hasher.finalize()),
    })
}

pub(crate) fn initial_engine_context_v1() -> EngineContextV2 {
    EngineContextV2 {
        priority_passes: [false; 2],
        stack_nonempty: false,
        stack_activity_since_priority_boundary: false,
        mana_activity_since_priority_boundary: false,
        last_mana_ability_activator_since_priority_boundary: None,
        current_stage: EngineDecisionStageV2::Priority,
        pending_cast: None,
        pending_activation: None,
        pending_discard: None,
        pending_optional_cost: None,
        pending_optional_cost_sacrifice: None,
        pending_spell_copy: None,
        pending_effect: None,
        pending_triggers: Vec::new(),
    }
}

pub(crate) fn initial_surface_context_v1() -> HarnessSurfaceContextV2 {
    HarnessSurfaceContextV2 {
        current_stage: SurfaceDecisionStageV2::Priority,
        combat_priority_spent: [false; 2],
        combat_priority_rearmed_by_stack_activity: false,
        combat_priority_rearmed_by_mana_activity: false,
        stack_grew_since_round_open: false,
        mana_activity_since_round_open: false,
        stack_length_changed_since_observed: Some(false),
        mana_activity_since_last_stack_change: false,
        madness_cast_reprompt_source: None,
        private_blockers: None,
        private_discard: None,
        private_optional_cost: None,
    }
}

pub(crate) fn initial_policy_surface_context_v1() -> PolicySurfaceContextV5 {
    PolicySurfaceContextV5 {
        current_stage: PolicySurfaceStageV5::Surface,
        private_combat_selection: None,
    }
}

fn serialized_commitment_v1<T: serde::Serialize>(
    label: &[u8],
    value: &T,
) -> Result<String, MtgoContractErrorV1> {
    let encoded = serde_json::to_vec(value).map_err(|error| {
        MtgoContractErrorV1::new("first_main_kernel_context_serialization", error.to_string())
    })?;
    let mut hasher = Sha256::new();
    hasher.update(FIRST_MAIN_KERNEL_CONTEXT_DOMAIN_V1);
    hasher.update((label.len() as u64).to_le_bytes());
    hasher.update(label);
    hasher.update((encoded.len() as u64).to_le_bytes());
    hasher.update(encoded);
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::complete_first_main_reconstruction_for_test_v1;
    use mtg_kernel::rl::{ActionSemanticV1, ZoneIndependentStepV1};
    use mtg_kernel::rl_session::{RlEpisodeSessionV1, RlSessionResponseV1};

    #[test]
    fn reset_context_template_matches_current_kernel_first_decision() {
        let observation = (1..=128)
            .find_map(|seed| {
                let session = RlEpisodeSessionV1::reset_with_limits(7, seed, 128, 16_384);
                let RlSessionResponseV1::Decision(decision) = session.current_response() else {
                    return None;
                };
                let has_pass = decision
                    .legal_actions
                    .iter()
                    .any(|action| matches!(action.semantic, ActionSemanticV1::Pass { .. }));
                let has_land = decision
                    .legal_actions
                    .iter()
                    .any(|action| matches!(action.semantic, ActionSemanticV1::PlayLand { .. }));
                (decision.observation.projection.surface.phase == ZoneIndependentStepV1::Main1
                    && has_pass
                    && has_land)
                    .then(|| decision.observation.clone())
            })
            .expect("a deterministic first decision must expose Main1 Pass and PlayLand");

        assert_eq!(
            observation.projection.surface.engine_context,
            initial_engine_context_v1()
        );
        assert_eq!(
            observation.projection.surface.surface_context,
            initial_surface_context_v1()
        );
        assert_eq!(
            observation.projection.policy_surface_context,
            initial_policy_surface_context_v1()
        );
        assert_eq!(
            observation.own_hand.len(),
            usize::from(KERNEL_STARTING_PLAYER_FIRST_MAIN_HAND_COUNT_V1)
        );
        assert_eq!(
            observation.projection.surface.hand_counts[0],
            usize::from(KERNEL_STARTING_PLAYER_FIRST_MAIN_HAND_COUNT_V1)
        );
    }

    #[test]
    fn exact_refinement_closes_history_only_and_keeps_model_blocked() {
        let reconstruction = complete_first_main_reconstruction_for_test_v1();
        let context =
            derive_checked_untrusted_first_main_kernel_context_v1(&reconstruction).unwrap();

        assert_eq!(
            context.resolved_group(),
            MtgoObservationReconstructionGroupV1::KernelDecisionHistoryContext
        );
        assert_eq!(context.remaining_blocking_groups(), REMAINING_BLOCKERS_V1);
        assert!(!context.observation_complete());
        assert!(!context.ready_for_model_scoring());
        assert!(!context.safe_for_input());
        assert_eq!(context.solitaire_source_hand_count(), 8);
        assert_eq!(context.kernel_starting_player_first_main_hand_count(), 7);
        assert!(!context.full_decision_compatible_with_two_player_starting_first_main());
        assert_eq!(context.context_commitment_sha256().len(), 64);
    }
}
