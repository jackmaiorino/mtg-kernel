//! Distinct authenticated Full-Ajani/common-family transport.
use crate::ids::PlayerId;
use crate::policy_observation_v8::PolicyObservationExtensionsV8;
use crate::policy_surface_v5::PolicySurfaceV5;
use crate::rl::{
    CardPrivateV1, KnownLibraryCardV4, PlayerSeatV1, PublicObservationProjectionV5, RlContractError,
};
use crate::state::GameState;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub const SCHEMA: &str = "mtg-kernel/faebloom-observation/v43";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FaebloomObservationV43 {
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
    pub grants: Vec<crate::akroma_public_v1::ActiveGrantV1>,
    pub families: Vec<crate::akroma_family_public_v40::FamilyFactsV40>,
    pub color_departures: Vec<crate::akroma_public_v1::ColorDepartureV1>,
    pub roots: Vec<crate::akroma_public_v1::RootV1>,
    pub projection_sha256: String,
    pub ajani: crate::ajani_public_v42::FactsV42,
    pub faebloom: crate::faebloom_public_v43::FactsV43,
    pub creature_target_traits: Vec<CreatureTargetTraitsV43>,
}
pub(crate) type FaebloomPublicPartsV43 = (
    FaebloomObservationV43,
    Vec<crate::akroma_family_public_v40::FamilyProjectionV40>,
);
pub fn producer_sha256() -> String {
    let mut h = Sha256::new();
    for s in [
        SCHEMA,
        include_str!("faebloom_observation_v43.rs"),
        include_str!("rl.rs"),
        include_str!("akroma_family_public_v40.rs"),
        include_str!("planeswalker_v1.rs"),
        include_str!("rl_session/faebloom_v43.rs"),
        include_str!("human_bo3_v1/faebloom_v43.rs"),
        include_str!("rl_session/flat_action_v3.rs"),
        include_str!("rl_session/flat_action_v4.rs"),
        include_str!("policy_surface_v5.rs"),
        include_str!("human_bo3_v1/visible.rs"),
        include_str!("human_bo3_v1/labels.rs"),
    ] {
        h.update([0]);
        h.update(s.as_bytes());
    }
    for s in [
        crate::ajani_public_v42::producer_sha256(),
        crate::faebloom_public_v43::producer_sha256(),
        crate::akroma_public_v1::producer_sha256_v1(),
    ] {
        h.update([1]);
        h.update(s.as_bytes());
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}
pub fn commitment(value: &FaebloomObservationV43) -> Result<String, RlContractError> {
    let mut v = value.clone();
    v.projection_sha256.clear();
    let b = serde_json::to_vec(&v).map_err(|e| RlContractError(e.to_string()))?;
    Ok(Sha256::digest(b)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}
pub(crate) fn validate_state(state: &GameState) -> Result<(), String> {
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) {
        return Err("Faebloom V43 requires the Foundations catalog".into());
    }
    crate::controlled_creature_grants_v1::validate_color_history(state)?;
    crate::akroma_public_v1::roots().map_err(|e| e.0)?;
    crate::akroma_family_public_v40::validate_state_for_ajani_v42(state)?;
    crate::ajani_public_v42::facts(state).map_err(|e| e.0)?;
    crate::faebloom_trick_v43::validate_state(state)?;
    Ok(())
}
pub fn observe_v43(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor: PlayerId,
    step: u64,
    physical: u64,
    substep: (u32, u32),
) -> Result<FaebloomObservationV43, RlContractError> {
    observe_parts_v43(state, surface, actor, step, physical, substep).map(|(o, _)| o)
}
pub(crate) fn observe_parts_v43(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor: PlayerId,
    step: u64,
    physical: u64,
    substep: (u32, u32),
) -> Result<FaebloomPublicPartsV43, RlContractError> {
    validate_state(state).map_err(RlContractError)?;
    let (substep_index, substep_count) = substep;
    let roots = crate::akroma_public_v1::roots()?;
    let (grants, color_departures) = crate::akroma_public_v1::facts(state)?;
    let base = crate::rl::common_family_observation_storage_scoped(
        (crate::rl::CommonProjectionScopeV42::FullFaebloomV43, state),
        surface,
        actor,
        step,
        physical,
        (substep_index, substep_count),
    )?;
    let storage = crate::policy_observation_v8::observation_storage_v8(state, actor, base)?;
    let mut base = storage.base;
    let mut parts = crate::akroma_family_public_v40::build_for_faebloom_v43(
        state, surface, actor, step, physical, substep,
    )?;
    base.projection = crate::akroma_family_public_v40::merge_projection(&base.projection, &parts)?;
    let mut observation = FaebloomObservationV43 {
        schema: SCHEMA.into(),
        producer_sha256: producer_sha256(),
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
        grants,
        families: parts.iter().map(|part| part.facts()).collect(),
        color_departures,
        roots,
        projection_sha256: String::new(),
        ajani: crate::ajani_public_v42::facts(state)?,
        faebloom: crate::faebloom_public_v43::capture(state)?,
        creature_target_traits: target_traits(state)?,
    };
    observation.projection_sha256 = commitment(&observation)?;
    for part in &mut parts {
        part.set_projection(&observation.projection);
    }
    Ok((observation, parts))
}

impl FaebloomObservationV43 {
    pub fn validate_artifact(&self) -> Result<(), RlContractError> {
        if self.schema != SCHEMA
            || self.producer_sha256 != producer_sha256()
            || self.kernel_version != crate::KERNEL_VERSION
            || self.roots != crate::akroma_public_v1::roots()?
            || self.ajani.meanings != crate::ajani_public_v42::meanings()?
            || self.faebloom.schema != crate::faebloom_public_v43::SCHEMA
            || self.faebloom.producer_sha256 != crate::faebloom_public_v43::producer_sha256()
            || self.faebloom.card_db_hash != self.card_db_hash
            || self.substep_count == 0
            || self.substep_index >= self.substep_count
            || self.card_db_hash != crate::card_def::KERNEL_CARDDB_HASH
            || self.projection_sha256 != commitment(self)?
        {
            return Err(RlContractError(
                "Ajani observation identity mismatch".into(),
            ));
        }
        for meaning in &self.ajani.meanings {
            crate::rules_meaning_ajani_v42::validate(meaning)
                .map_err(|_| RlContractError("Ajani precise root differs".into()))?;
        }
        let selector =
            crate::rules_meaning_faebloom_v43::live_selector().map_err(RlContractError)?;
        for meaning in &self.faebloom.roots {
            crate::rules_meaning_faebloom_v43::validate(&selector, meaning)
                .map_err(RlContractError)?;
        }
        Ok(())
    }
}

/// Extra targeting facts absent from the frozen keyword DTO, bound in the whole observation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatureTargetTraitsV43 {
    pub creature: crate::rl::CardStableRefV1,
    pub shroud: bool,
    pub hexproof: bool,
    pub protection_from_monocolored: bool,
}
fn target_traits(state: &GameState) -> Result<Vec<CreatureTargetTraitsV43>, RlContractError> {
    use crate::card_def::{CardType, Keywords};
    state
        .objects
        .iter()
        .filter(|(id, object)| {
            object.zone == crate::state::Zone::Battlefield
                && crate::engine::object_has_type(state, *id, CardType::Creature)
        })
        .map(|(id, object)| {
            Ok(CreatureTargetTraitsV43 {
                creature: crate::sower_restriction_capture_v1::masked_ref(id, object)?,
                shroud: crate::engine::has_effective_keyword(state, id, Keywords::SHROUD),
                hexproof: crate::engine::has_effective_keyword(state, id, Keywords::HEXPROOF),
                protection_from_monocolored: crate::engine::has_effective_keyword(
                    state,
                    id,
                    Keywords::PROTECTION_FROM_MONOCOLORED,
                ),
            })
        })
        .collect()
}
