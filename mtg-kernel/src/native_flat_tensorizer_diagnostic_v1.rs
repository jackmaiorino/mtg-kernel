//! Environment-only microprofile seam for the production V2 tensorizer.
//!
//! The fixture is the production-generated `burn-mirror-combat` decision from
//! the Python-authoritative full-feature golden. This module deliberately
//! exposes only an opaque repeated-run harness behind an opt-in feature.

use crate::flat_policy_v2::{
    FlatCompletedDungeonV2, FlatContextPathElementV2, FlatDecisionEncoderV2,
    FlatEffectSubtypeChangeV2, FlatGlobalsV2, FlatObjectAbilityUseV2, FlatObjectCoreV2,
    FlatObjectGoadV2, FlatObjectSubtypeV2, FlatRelationV2, FlatScorerActionCoreV2,
    FlatScorerActionRefV2, FlatScoringDecisionViewV2, FlatScoringOwnedBuffersV2,
};
use crate::native_flat_tensorizer_v2::{NativeFlatDecisionTensorV2, NativeFlatTensorizerV2};
use crate::rl_session::{FastActorResponseV1, FastActorSessionV1};
use serde::Deserialize;
use std::hint::black_box;
use std::time::Instant;

const FULL_GOLDEN: &str = include_str!("../../data/flat_policy_v2/python_full_features_v2.json");

#[derive(Deserialize)]
struct GoldenDocument {
    cases: Vec<GoldenCase>,
}

#[derive(Deserialize)]
struct GoldenCase {
    name: String,
    rust_fixture: GoldenFixture,
}

#[derive(Deserialize)]
struct GoldenFixture {
    episode_id: u64,
    environment_seed: u64,
    deck_ids: [String; 2],
    replay_selected_indices: Vec<u32>,
}

struct OwnedScoringDecisionV2 {
    globals: FlatGlobalsV2,
    objects: Vec<FlatObjectCoreV2>,
    relations: Vec<FlatRelationV2>,
    object_subtypes: Vec<FlatObjectSubtypeV2>,
    ability_uses: Vec<FlatObjectAbilityUseV2>,
    goads: Vec<FlatObjectGoadV2>,
    completed_dungeons: Vec<FlatCompletedDungeonV2>,
    effect_subtype_changes: Vec<FlatEffectSubtypeChangeV2>,
    context_path_elements: Vec<FlatContextPathElementV2>,
    actions: Vec<FlatScorerActionCoreV2>,
    action_refs: Vec<FlatScorerActionRefV2>,
}

impl OwnedScoringDecisionV2 {
    fn from_session(session: &FastActorSessionV1) -> Result<Self, String> {
        let FastActorResponseV1::Decision(expected) = session.current_response() else {
            return Err("diagnostic fixture terminated before tensorization".into());
        };
        let mut encoder = FlatDecisionEncoderV2::default();
        let mut owned = Self {
            globals: FlatGlobalsV2::default(),
            objects: Vec::new(),
            relations: Vec::new(),
            object_subtypes: Vec::new(),
            ability_uses: Vec::new(),
            goads: Vec::new(),
            completed_dungeons: Vec::new(),
            effect_subtype_changes: Vec::new(),
            context_path_elements: Vec::new(),
            actions: Vec::new(),
            action_refs: Vec::new(),
        };
        let encoded = session
            .encode_current_flat_scoring_decision_owned_v2(
                expected,
                &mut encoder,
                &mut FlatScoringOwnedBuffersV2 {
                    objects: &mut owned.objects,
                    relations: &mut owned.relations,
                    object_subtypes: &mut owned.object_subtypes,
                    ability_uses: &mut owned.ability_uses,
                    goads: &mut owned.goads,
                    completed_dungeons: &mut owned.completed_dungeons,
                    effect_subtype_changes: &mut owned.effect_subtype_changes,
                    context_path_elements: &mut owned.context_path_elements,
                    actions: &mut owned.actions,
                    action_refs: &mut owned.action_refs,
                },
            )
            .map_err(|error| format!("diagnostic fixture encode failed: {error:?}"))?;
        owned.globals = encoded.globals;
        Ok(owned)
    }

    fn view(&self) -> FlatScoringDecisionViewV2<'_> {
        FlatScoringDecisionViewV2::new(
            &self.globals,
            &self.objects,
            &self.relations,
            &self.object_subtypes,
            &self.ability_uses,
            &self.goads,
            &self.completed_dungeons,
            &self.effect_subtype_changes,
            &self.context_path_elements,
            &self.actions,
            &self.action_refs,
        )
    }
}

/// Opaque, reusable microprofile fixture. It executes the same stateful
/// tensorizer API used by the native trainer scorer.
pub struct NativeFlatTensorizerDiagnosticFixtureV1 {
    decision: OwnedScoringDecisionV2,
    tensorizer: NativeFlatTensorizerV2,
    output: NativeFlatDecisionTensorV2,
}

#[derive(Clone, Copy, Debug)]
pub struct NativeFlatTensorizerDiagnosticRunV1 {
    pub iterations: u64,
    pub elapsed_nanoseconds: u128,
    pub checksum: u64,
    pub object_rows: usize,
    pub edge_rows: usize,
    pub action_rows: usize,
    pub action_ref_rows: usize,
}

impl NativeFlatTensorizerDiagnosticFixtureV1 {
    pub fn production_burn_combat_v1() -> Result<Self, String> {
        let document: GoldenDocument =
            serde_json::from_str(FULL_GOLDEN).map_err(|error| error.to_string())?;
        let fixture = document
            .cases
            .into_iter()
            .find(|case| case.name == "burn-mirror-combat")
            .ok_or_else(|| "burn-mirror-combat fixture is missing".to_string())?
            .rust_fixture;
        let mut session = FastActorSessionV1::reset_with_decks_and_limits_flat_action_v2(
            fixture.episode_id,
            fixture.environment_seed,
            256,
            32_768,
            fixture.deck_ids,
        )
        .map_err(|error| format!("diagnostic session reset failed: {error:?}"))?;
        for selected_index in fixture.replay_selected_indices {
            let FastActorResponseV1::Decision(expected) = session.current_response() else {
                return Err("diagnostic replay terminated early".into());
            };
            session
                .step(expected.episode_id, expected.step, selected_index)
                .map_err(|error| format!("diagnostic replay step failed: {error:?}"))?;
        }
        Ok(Self {
            decision: OwnedScoringDecisionV2::from_session(&session)?,
            tensorizer: NativeFlatTensorizerV2::new(),
            output: NativeFlatDecisionTensorV2::default(),
        })
    }

    pub fn run_v1(
        &mut self,
        iterations: u64,
    ) -> Result<NativeFlatTensorizerDiagnosticRunV1, String> {
        if iterations == 0 {
            return Err("diagnostic iterations must be positive".into());
        }
        let started = Instant::now();
        for _ in 0..iterations {
            self.tensorizer
                .fill(self.decision.view(), &mut self.output)
                .map_err(|error| format!("production tensorizer failed: {error:?}"))?;
            black_box(&self.output);
        }
        let elapsed_nanoseconds = started.elapsed().as_nanos();
        let checksum = self
            .output
            .state
            .iter()
            .chain(&self.output.object_features)
            .chain(&self.output.edge_features)
            .chain(&self.output.action_features)
            .chain(&self.output.action_ref_features)
            .fold(0xcbf29ce484222325_u64, |hash, value| {
                (hash ^ u64::from(value.to_bits())).wrapping_mul(0x100000001b3)
            });
        Ok(NativeFlatTensorizerDiagnosticRunV1 {
            iterations,
            elapsed_nanoseconds,
            checksum,
            object_rows: self.output.object_card_ids.len(),
            edge_rows: self.output.edge_source_indices.len(),
            action_rows: self.output.action_features.len() / 195,
            action_ref_rows: self.output.action_ref_card_ids.len(),
        })
    }
}
