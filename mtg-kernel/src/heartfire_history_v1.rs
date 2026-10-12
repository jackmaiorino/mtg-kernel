//! Independent captured-source input. No frozen observation is constructed.
use crate::ids::PlayerId;
use crate::policy_observation_v8::PolicyObservationExtensionsV8;
use crate::policy_surface_v5::PolicySurfaceV5;
use crate::rl::{
    CardPrivateV1, KnownLibraryCardV4, PlayerSeatV1, PublicObservationProjectionV5, RlContractError,
};
use crate::state::GameState;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub const SCHEMA: &str = "mtg-kernel/heartfire-history-observation/v1";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeartfireHistoryObservationV1 {
    pub schema: String,
    pub source_sha256: String,
    pub kernel_version: String,
    pub surface_version: u32,
    pub policy_surface_version: u32,
    pub card_db_hash: u64,
    pub acting_player: PlayerSeatV1,
    pub step_index: u64,
    pub physical_decision_id: u64,
    pub substep_index: u32,
    pub substep_count: u32,
    pub projection: PublicObservationProjectionV5,
    pub own_hand: Vec<CardPrivateV1>,
    pub known_library_cards: [Vec<KnownLibraryCardV4>; 2],
    pub known_hand_cards: [Vec<CardPrivateV1>; 2],
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub known_face_down_cards: Vec<crate::rl::FaceDownCardKnowledgeV1>,
    pub extensions: PolicyObservationExtensionsV8,
    pub attacked_this_turn: [bool; 2],
    pub saved_damage: Vec<crate::heartfire_public_v1::HeartfireSavedDamageV1>,
    pub visible_projection_hash: u64,
}

pub fn source_sha256() -> String {
    static SOURCE: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    SOURCE
        .get_or_init(|| {
            let mut hash = Sha256::new();
            for text in [
                include_str!("heartfire_history_v1.rs"),
                include_str!("rl/heartfire_history_v1.rs"),
                include_str!("rl_session/heartfire_history_v1.rs"),
                include_str!("human_bo3_v1/heartfire_history_v1.rs"),
                include_str!("source_power_activation_v1.rs"),
                include_str!("creature_stats_lki_v1.rs"),
                include_str!("rl.rs"),
                include_str!("rl_session.rs"),
                include_str!("human_bo3_v1/labels.rs"),
                include_str!("human_bo3_v1/visible.rs"),
                include_str!("policy_observation_v8.rs"),
                include_str!("policy_observation_v7.rs"),
                include_str!("engine.rs"),
                include_str!("standard_cards_v1.rs"),
                include_str!("card_def.rs"),
                include_str!("../build.rs"),
            ] {
                hash.update([0]);
                hash.update(text.as_bytes());
            }
            hash.finalize()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect()
        })
        .clone()
}
/// Artifact identity/shape only. Execution requires a sealed freshly derived binding.
pub fn validate(observation: &HeartfireHistoryObservationV1) -> Result<(), RlContractError> {
    crate::rl::heartfire_history_v1::validate_artifact(observation)
}
#[allow(clippy::too_many_arguments)]
pub fn observe(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor: PlayerId,
    step: u64,
    physical: u64,
    substep: u32,
    count: u32,
) -> Result<HeartfireHistoryObservationV1, RlContractError> {
    crate::rl::heartfire_history_v1::observe(state, surface, actor, step, physical, substep, count)
}
