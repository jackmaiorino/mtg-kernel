//! Separate public FishingPole transport. This is not V8/V9 or tensor/model admission.
use crate::ids::PlayerId;
use crate::policy_observation_v8::PolicyObservationExtensionsV8;
use crate::policy_surface_v5::PolicySurfaceV5;
use crate::rl::{
    CardPrivateV1, KnownLibraryCardV4, PlayerSeatV1, PublicObservationProjectionV5, RlContractError,
};
use crate::state::GameState;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA: &str = "mtg-kernel/fishing-pole-observation/v39";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RootV1 {
    pub card_id: u16,
    pub root: crate::rules_meaning_fishing_pole_v39::RootV39,
    pub root_sha256: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FishingPoleObservationV1 {
    pub schema: String,
    pub producer_sha256: String,
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
    pub facts: crate::fishing_pole_public_v39::FactsV39,
    pub roots: Vec<RootV1>,
    pub projection_sha256: String,
}

fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
pub fn producer_sha256_v1() -> String {
    let mut hash = Sha256::new();
    for source in [
        SCHEMA,
        include_str!("fishing_pole_observation_v39.rs"),
        include_str!("rules_meaning_fishing_pole_v39.rs"),
        include_str!("fishing_pole_public_v39.rs"),
        include_str!("fishing_pole_v39.rs"),
        include_str!("fishing_pole_v39/lifecycle.rs"),
        include_str!("effect/crystal_prevention_v1.rs"),
        include_str!("source_power_activation_v1.rs"),
        include_str!("gorehorn_damage_v1.rs"),
        include_str!("effect/squad_rallier_resolution_v1.rs"),
        include_str!("raid_entry_v1.rs"),
        include_str!("raid_entry_custody_v1.rs"),
        include_str!("card_def.rs"),
        include_str!("../build.rs"),
        include_str!("trigger.rs"),
        include_str!("effect.rs"),
        include_str!("engine.rs"),
        include_str!("event.rs"),
        include_str!("state.rs"),
        include_str!("extra_turn_v1.rs"),
        include_str!("continuous_characteristics_v1.rs"),
        include_str!("standard_cards_v1.rs"),
        include_str!("rl.rs"),
        include_str!("rl_session.rs"),
        include_str!("rl_session/fishing_pole_v39.rs"),
        include_str!("human_bo3_v1/fishing_pole_v39.rs"),
        include_str!("human_bo3_v1/labels.rs"),
        include_str!("human_bo3_v1/visible.rs"),
        include_str!("human_bo3_v1/life_ward_v8.rs"),
        include_str!("policy_observation_v8.rs"),
        include_str!("policy_observation_v7.rs"),
        include_str!("rl_session/flat_action_v3.rs"),
        include_str!("rl_session/flat_action_v4.rs"),
        include_str!("policy_surface_v5.rs"),
        include_str!("surface_v2.rs"),
        include_str!("standard_creatures_v1.rs"),
        include_str!("standard_statics_v1.rs"),
        include_str!("standard_keywords_v1.rs"),
        include_str!("standard_legends_v1.rs"),
    ] {
        hash.update([0]);
        hash.update(source.as_bytes());
    }
    hash.finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn commitment(observation: &FishingPoleObservationV1) -> Result<String, RlContractError> {
    let mut value = serde_json::to_value(observation)?;
    value
        .as_object_mut()
        .ok_or_else(|| RlContractError("FishingPole projection is not an object".into()))?
        .remove("projection_sha256");
    Ok(digest(&serde_json::to_vec(&value)?))
}
impl FishingPoleObservationV1 {
    /// Checks transport identity only. Execution always rederives the entire
    /// observation and private ordered candidate binding from the owning session.
    pub fn validate_artifact(&self) -> Result<(), RlContractError> {
        if self.schema != SCHEMA
            || self.card_db_hash != crate::card_def::KERNEL_CARDDB_HASH
            || self.producer_sha256 != producer_sha256_v1()
            || self.projection_sha256 != commitment(self)?
            || self.facts.schema != crate::fishing_pole_public_v39::SCHEMA
            || self.facts.card_db_hash != self.card_db_hash
            || self.facts.producer_sha256 != crate::fishing_pole_public_v39::producer_sha256_v39()
        {
            return Err(RlContractError(
                "wrong FishingPole artifact identity".into(),
            ));
        }
        let expected = roots()?;
        if self.roots != expected {
            return Err(RlContractError("wrong FishingPole printed root".into()));
        }
        Ok(())
    }
}
fn roots() -> Result<Vec<RootV1>, RlContractError> {
    // Ordinary Default/Standard sessions still have a real successor identity,
    // but never fabricate the missing Limited definition or claim its meaning.
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) {
        return Ok(Vec::new());
    }
    let Some(id) = crate::card_def::card_id_by_name("Fishing Pole") else {
        return Ok(Vec::new());
    };
    let selector = crate::rules_meaning_fishing_pole_v39::live_selector()
        .map_err(|_| RlContractError("FishingPole catalog not supported".into()))?;
    [
        crate::rules_meaning_fishing_pole_v39::RootV39::PrintedArtifactSpell,
        crate::rules_meaning_fishing_pole_v39::RootV39::PrintedEquip { ability_index: 0 },
        crate::rules_meaning_fishing_pole_v39::RootV39::GrantedBaitActivation,
        crate::rules_meaning_fishing_pole_v39::RootV39::EquippedCreatureUntaps,
    ]
    .into_iter()
    .map(|root| {
        let meaning = crate::rules_meaning_fishing_pole_v39::meaning(&selector, id, root)
            .map_err(|_| RlContractError("FishingPole root not supported".into()))?;
        Ok(RootV1 {
            card_id: id,
            root,
            root_sha256: meaning.root_sha256,
        })
    })
    .collect()
}
pub fn observe_v1(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor: PlayerId,
    step_index: u64,
    physical_decision_id: u64,
    substep_index: u32,
    substep_count: u32,
) -> Result<FishingPoleObservationV1, RlContractError> {
    validate_projection_authority(state)?;
    let base = crate::rl::fishing_pole_observation_storage_v39(
        state,
        surface,
        actor,
        step_index,
        physical_decision_id,
        (substep_index, substep_count),
    )?;
    let storage = crate::policy_observation_v8::observation_storage_v8(state, actor, base)?;
    let base = storage.base;
    let mut observation = FishingPoleObservationV1 {
        schema: SCHEMA.into(),
        producer_sha256: producer_sha256_v1(),
        kernel_version: base.kernel_version,
        surface_version: base.surface_version,
        policy_surface_version: base.policy_surface_version,
        card_db_hash: base.card_db_hash,
        acting_player: base.acting_player,
        step_index: base.step_index,
        physical_decision_id: base.physical_decision_id,
        substep_index: base.substep_index,
        substep_count: base.substep_count,
        projection: base.projection,
        own_hand: base.own_hand,
        known_library_cards: base.known_library_cards,
        known_hand_cards: base.known_hand_cards,
        known_face_down_cards: base.known_face_down_cards,
        extensions: storage.extensions,
        attacked_this_turn: [
            state.player_attacked_this_turn_v1(PlayerId::P0),
            state.player_attacked_this_turn_v1(PlayerId::P1),
        ],
        facts: crate::fishing_pole_public_v39::facts_v39(state)?,
        roots: roots()?,
        projection_sha256: String::new(),
    };
    observation.projection_sha256 = commitment(&observation)?;
    Ok(observation)
}

pub(crate) fn parts_for_akroma_v40(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor: PlayerId,
    step_index: u64,
    physical_decision_id: u64,
    substep: (u32, u32),
) -> Result<FishingPoleObservationV1, RlContractError> {
    parts_for_common_scope(
        state,
        surface,
        actor,
        step_index,
        physical_decision_id,
        substep,
        crate::rl::CommonProjectionScopeV42::ExistingV40,
    )
}
pub(crate) fn parts_for_ajani_v42(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor: PlayerId,
    step_index: u64,
    physical_decision_id: u64,
    substep: (u32, u32),
) -> Result<FishingPoleObservationV1, RlContractError> {
    parts_for_common_scope(
        state,
        surface,
        actor,
        step_index,
        physical_decision_id,
        substep,
        crate::rl::CommonProjectionScopeV42::FullAjaniV42,
    )
}
pub(crate) fn parts_for_common_scope(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor: PlayerId,
    step_index: u64,
    physical_decision_id: u64,
    substep: (u32, u32),
    scope: crate::rl::CommonProjectionScopeV42,
) -> Result<FishingPoleObservationV1, RlContractError> {
    let (substep_index, substep_count) = substep;
    validate_projection_authority(state)?;
    let base = crate::rl::common_family_observation_storage_scoped(
        (scope, state),
        surface,
        actor,
        step_index,
        physical_decision_id,
        (substep_index, substep_count),
    )?;
    let storage = crate::policy_observation_v8::observation_storage_v8(state, actor, base)?;
    let base = storage.base;
    let observation = FishingPoleObservationV1 {
        schema: String::new(),
        producer_sha256: String::new(),
        kernel_version: base.kernel_version,
        surface_version: base.surface_version,
        policy_surface_version: base.policy_surface_version,
        card_db_hash: base.card_db_hash,
        acting_player: base.acting_player,
        step_index: base.step_index,
        physical_decision_id: base.physical_decision_id,
        substep_index: base.substep_index,
        substep_count: base.substep_count,
        projection: base.projection,
        own_hand: base.own_hand,
        known_library_cards: base.known_library_cards,
        known_hand_cards: base.known_hand_cards,
        known_face_down_cards: base.known_face_down_cards,
        extensions: storage.extensions,
        attacked_this_turn: [
            state.player_attacked_this_turn_v1(PlayerId::P0),
            state.player_attacked_this_turn_v1(PlayerId::P1),
        ],
        facts: crate::fishing_pole_public_v39::facts_v39(state)?,
        roots: roots()?,
        projection_sha256: String::new(),
    };
    Ok(observation)
}

pub(crate) fn validate_projection_authority(state: &GameState) -> Result<(), RlContractError> {
    crate::effect::validate_pending_effect_choice(state).map_err(RlContractError)?;
    crate::fishing_pole_public_v39::facts_v39(state)?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn registered_absence_keeps_genuine_predecessor_payload_bytes_and_state_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let state = crate::policy_observation_v6::tests::ready_state_for(actor);
            let surface = PolicySurfaceV5::new_for_session();
            let before = serde_json::to_vec(&state).unwrap();
            let old = crate::rl::observe_policy_v8(&state, &surface, actor, 0, 0, 0, 1).unwrap();
            let new = observe_v1(&state, &surface, actor, 0, 0, 0, 1).unwrap();
            assert_eq!(
                serde_json::to_vec(&new.projection).unwrap(),
                serde_json::to_vec(&old.projection).unwrap()
            );
            assert_eq!(new.own_hand, old.own_hand);
            assert_eq!(new.known_library_cards, old.known_library_cards);
            assert_eq!(new.known_hand_cards, old.known_hand_cards);
            assert_eq!(new.known_face_down_cards, old.known_face_down_cards);
            assert_eq!(new.extensions, old.extensions);
            assert!(new.facts.poles.is_empty() && new.facts.operations.is_empty());
            new.validate_artifact().unwrap();
            assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        }
    }
}
