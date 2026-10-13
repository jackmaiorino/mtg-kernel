//! Separate public Sower transport. This is not V8/V9 or tensor/model admission.
use crate::ids::{ObjectId, PlayerId};
use crate::policy_observation_v8::PolicyObservationExtensionsV8;
use crate::policy_surface_v5::PolicySurfaceV5;
use crate::rl::{
    CardPrivateV1, CardStableRefV1, KnownLibraryCardV4, PlayerSeatV1,
    PublicObservationProjectionV5, RlContractError,
};
use crate::state::{GameState, Zone};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA: &str = "mtg-kernel/sower-public/v1";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RestrictionV1 {
    /// Public identity at installation, masked if it was face down.
    pub recipient: CardStableRefV1,
    pub timestamp: u64,
    pub current_recipient: Option<CardStableRefV1>,
    /// True only while this exact incarnation remains on the battlefield.
    /// Later control or ability removal does not remove the restriction.
    pub effective: bool,
    pub duration: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RootV1 {
    pub card_id: u16,
    pub ability_index: u16,
    pub root_sha256: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SowerObservationV1 {
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
    pub restrictions: Vec<RestrictionV1>,
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
        include_str!("sower_public_v1.rs"),
        include_str!("sower_restriction_capture_v1.rs"),
        include_str!("sower_of_chaos_v1.rs"),
        include_str!("rules_meaning_sower_v1.rs"),
        include_str!("rl.rs"),
        include_str!("rl_session/sower_v14.rs"),
        include_str!("human_bo3_v1/sower_v14.rs"),
        include_str!("human_bo3_v1/labels.rs"),
        include_str!("human_bo3_v1/visible.rs"),
        include_str!("human_bo3_v1/life_ward_v8.rs"),
        include_str!("engine.rs"),
        include_str!("event.rs"),
        include_str!("rl_session.rs"),
        include_str!("state.rs"),
        include_str!("policy_observation_v8.rs"),
        include_str!("policy_observation_v7.rs"),
        include_str!("rl_session/flat_action_v3.rs"),
        include_str!("rl_session/flat_action_v4.rs"),
    ] {
        hash.update([0]);
        hash.update(source.as_bytes());
    }
    hash.finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn commitment(observation: &SowerObservationV1) -> Result<String, RlContractError> {
    let mut value = serde_json::to_value(observation)?;
    value
        .as_object_mut()
        .ok_or_else(|| RlContractError("Sower projection is not an object".into()))?
        .remove("projection_sha256");
    Ok(digest(&serde_json::to_vec(&value)?))
}
impl SowerObservationV1 {
    /// Checks transport identity only. Execution always rederives the entire
    /// observation and private ordered candidate binding from the owning session.
    pub fn validate_artifact(&self) -> Result<(), RlContractError> {
        if self.schema != SCHEMA
            || self.card_db_hash != crate::card_def::KERNEL_CARDDB_HASH
            || self.producer_sha256 != producer_sha256_v1()
            || self.projection_sha256 != commitment(self)?
        {
            return Err(RlContractError("wrong Sower artifact identity".into()));
        }
        let expected = roots()?;
        if self.roots != expected {
            return Err(RlContractError("wrong Sower printed root".into()));
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
    let Some(id) = crate::card_def::card_id_by_name("Sower of Chaos") else {
        return Ok(Vec::new());
    };
    let selector = crate::rules_meaning_sower_v1::live_selector_v1()
        .map_err(|_| RlContractError("Sower catalog not supported".into()))?;
    let meaning = crate::rules_meaning_sower_v1::meaning_v1(&selector, id, 0)
        .map_err(|_| RlContractError("Sower root not supported".into()))?;
    Ok(vec![RootV1 {
        card_id: id,
        ability_index: 0,
        root_sha256: meaning.root_sha256,
    }])
}
// V40 consumes these private typed parts; standalone predecessors retain their refusal path.
pub(crate) fn parts_for_akroma_v40(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor: PlayerId,
    step_index: u64,
    physical_decision_id: u64,
    substep: (u32, u32),
) -> Result<SowerObservationV1, RlContractError> {
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
) -> Result<SowerObservationV1, RlContractError> {
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
) -> Result<SowerObservationV1, RlContractError> {
    let (substep_index, substep_count) = substep;

    let captures = crate::sower_restriction_capture_v1::active(state)?;
    let mut restrictions: Vec<_> = captures
        .into_iter()
        .map(|capture| {
            let id = ObjectId(capture.recipient.arena_id);
            let current = state
                .objects
                .try_get(id)
                .filter(|object| {
                    object.zone == Zone::Battlefield
                        && object.zone_change_count == capture.recipient.zone_change_count
                })
                .map(|object| crate::sower_restriction_capture_v1::masked_ref(id, object))
                .transpose()?;
            Ok(RestrictionV1 {
                recipient: capture.recipient.clone(),
                timestamp: capture.timestamp,
                effective: current.is_some(),
                current_recipient: current,
                duration: "until_next_cleanup_including_repeated_cleanup".into(),
            })
        })
        .collect::<Result<Vec<_>, RlContractError>>()?;
    restrictions.sort_by_key(|row| row.timestamp);
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
    let observation = SowerObservationV1 {
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
        restrictions,
        roots: roots()?,
        projection_sha256: String::new(),
    };
    Ok(observation)
}
#[allow(clippy::too_many_arguments)]
pub fn observe_v1(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor: PlayerId,
    step_index: u64,
    physical_decision_id: u64,
    substep_index: u32,
    substep_count: u32,
) -> Result<SowerObservationV1, RlContractError> {
    let captures = crate::sower_restriction_capture_v1::active(state)?;
    let mut restrictions: Vec<_> = captures
        .into_iter()
        .map(|capture| {
            let id = ObjectId(capture.recipient.arena_id);
            let current = state
                .objects
                .try_get(id)
                .filter(|object| {
                    object.zone == Zone::Battlefield
                        && object.zone_change_count == capture.recipient.zone_change_count
                })
                .map(|object| crate::sower_restriction_capture_v1::masked_ref(id, object))
                .transpose()?;
            Ok(RestrictionV1 {
                recipient: capture.recipient.clone(),
                timestamp: capture.timestamp,
                effective: current.is_some(),
                current_recipient: current,
                duration: "until_next_cleanup_including_repeated_cleanup".into(),
            })
        })
        .collect::<Result<Vec<_>, RlContractError>>()?;
    restrictions.sort_by_key(|row| row.timestamp);
    let base = crate::rl::sower_observation_storage_v1(
        state,
        surface,
        actor,
        step_index,
        physical_decision_id,
        (substep_index, substep_count),
    )?;
    let storage = crate::policy_observation_v8::observation_storage_v8(state, actor, base)?;
    let base = storage.base;
    let mut observation = SowerObservationV1 {
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
        restrictions,
        roots: roots()?,
        projection_sha256: String::new(),
    };
    observation.projection_sha256 = commitment(&observation)?;
    Ok(observation)
}

#[cfg(test)]
pub(crate) mod tests;
