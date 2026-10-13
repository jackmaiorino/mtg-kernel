//! Explicit temporary-control transport. Frozen V8/V9 remain refused.
use crate::effect::temporary_control_v1::authority::{self, InstallationOriginV1};
use crate::event::CommittedEvent;
use crate::ids::PlayerId;
use crate::policy_observation_v8::PolicyObservationExtensionsV8;
use crate::policy_surface_v5::PolicySurfaceV5;
use crate::rl::{
    CardPrivateV1, CardStableRefV1, KnownLibraryCardV4, PlayerSeatV1,
    PublicObservationProjectionV5, RlContractError,
};
use crate::state::{GameObject, GameState, StackTargetContractV4, Zone};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA: &str = "mtg-kernel/employment-public/v1";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum OriginV1 {
    PrintedSpell,
    RegisteredPrimitive,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstallationV1 {
    pub origin: OriginV1,
    /// A hand-source primitive discloses its source only to its physical owner.
    pub source: Option<CardStableRefV1>,
    pub recipient: CardStableRefV1,
    pub original_controller: PlayerSeatV1,
    pub captured_controller: PlayerSeatV1,
    pub timestamp: u64,
    pub installation_turn: u32,
    pub current_recipient: Option<CardStableRefV1>,
    pub current_tapped: Option<bool>,
    pub current_has_haste: Option<bool>,
    pub expired: bool,
    pub cleanup_turn: Option<u32>,
    pub duration: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RootV1 {
    pub card_id: u16,
    pub root_sha256: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmploymentObservationV1 {
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
    pub installations: Vec<InstallationV1>,
    pub roots: Vec<RootV1>,
    pub projection_sha256: String,
}
fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
pub fn producer_sha256_v1() -> String {
    let mut hash = Sha256::new();
    for source in [
        SCHEMA,
        include_str!("employment_public_v1.rs"),
        include_str!("effect/temporary_control_v1/authority.rs"),
        include_str!("effect/temporary_control_v1.rs"),
        include_str!("rules_meaning_employment_v1.rs"),
        include_str!("rl.rs"),
        include_str!("rl_session/employment_v18.rs"),
        include_str!("human_bo3_v1/employment_v18.rs"),
        include_str!("human_bo3_v1/labels.rs"),
        include_str!("human_bo3_v1/visible.rs"),
        include_str!("human_bo3_v1/life_ward_v8.rs"),
        include_str!("sower_restriction_capture_v1.rs"),
        include_str!("engine.rs"),
        include_str!("continuous_characteristics_v1.rs"),
        include_str!("standard_cards_v1.rs"),
        include_str!("event.rs"),
        include_str!("effect.rs"),
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
    hash.finalize().iter().map(|b| format!("{b:02x}")).collect()
}
fn roots() -> Result<Vec<RootV1>, RlContractError> {
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) {
        return Ok(Vec::new());
    }
    let Some(id) = crate::card_def::card_id_by_name("Involuntary Employment") else {
        return Ok(Vec::new());
    };
    let selector = crate::rules_meaning_employment_v1::live_selector()
        .map_err(|_| RlContractError("Employment catalog unsupported".into()))?;
    let meaning = crate::rules_meaning_employment_v1::meaning(
        &selector,
        id,
        crate::rules_meaning_employment_v1::RootV1::PrintedSpell,
    )
    .map_err(|_| RlContractError("Employment root unsupported".into()))?;
    Ok(vec![RootV1 {
        card_id: id,
        root_sha256: meaning.root_sha256,
    }])
}
fn commitment(observation: &EmploymentObservationV1) -> Result<String, RlContractError> {
    let mut value = serde_json::to_value(observation)?;
    value
        .as_object_mut()
        .ok_or_else(|| RlContractError("Employment object absent".into()))?
        .remove("projection_sha256");
    Ok(digest(&serde_json::to_vec(&value)?))
}
impl EmploymentObservationV1 {
    /// Artifact identity never authorizes execution; the owner rederives all facts.
    pub fn validate_artifact(&self) -> Result<(), RlContractError> {
        if self.schema != SCHEMA
            || self.card_db_hash != crate::card_def::KERNEL_CARDDB_HASH
            || self.producer_sha256 != producer_sha256_v1()
            || self.roots != roots()?
            || self.projection_sha256 != commitment(self)?
        {
            return Err(RlContractError("wrong Employment artifact identity".into()));
        }
        Ok(())
    }
}
pub(crate) fn installations(
    state: &GameState,
    actor: PlayerId,
) -> Result<Vec<InstallationV1>, RlContractError> {
    authority::validate(state).map_err(RlContractError)?;
    let mut rows = Vec::new();
    for (index, event) in state.engine.event_history.iter().enumerate() {
        let CommittedEvent::TemporaryControlInstalledV1 {
            installation: capture,
        } = event
        else {
            continue;
        };
        let StackTargetContractV4::Object {
            object,
            zone_change_count,
            ..
        } = capture.target
        else {
            return Err(RlContractError("Employment recipient absent".into()));
        };
        let snapshot: GameObject = serde_json::from_str(&capture.recipient_snapshot)?;
        let recipient = crate::sower_restriction_capture_v1::masked_ref(object, &snapshot)?;
        let current = state.objects.try_get(object).filter(|live| {
            live.zone == Zone::Battlefield && live.zone_change_count == zone_change_count
        });
        let current_recipient = current
            .map(|live| crate::sower_restriction_capture_v1::masked_ref(object, live))
            .transpose()?;
        let expiry = state
            .engine
            .event_history
            .iter()
            .find_map(|event| match event {
                CommittedEvent::TemporaryControlExpiredV1 {
                    installation_history_index,
                    cleanup_history_index,
                } if *installation_history_index == index => Some(*cleanup_history_index),
                _ => None,
            });
        let cleanup_turn = expiry.and_then(|i| match state.engine.event_history.get(i) {
            Some(CommittedEvent::TemporaryControlCleanupV1 { turn, .. }) => Some(*turn),
            _ => None,
        });
        let (origin, source) = match capture.origin {
            InstallationOriginV1::PrintedSpell { .. } => (
                OriginV1::PrintedSpell,
                Some(CardStableRefV1 {
                    arena_id: capture.source.source.0,
                    card_db_id: capture.source.card_def,
                    owner: capture.source.owner.into(),
                    controller: capture.controller.into(),
                    zone: Zone::Stack,
                    zone_change_count: capture.source.zone_change_count,
                }),
            ),
            InstallationOriginV1::RegisteredPrimitive => (
                OriginV1::RegisteredPrimitive,
                (capture.source.owner == actor).then_some(CardStableRefV1 {
                    arena_id: capture.source.source.0,
                    card_db_id: capture.source.card_def,
                    owner: capture.source.owner.into(),
                    controller: capture.source.controller.into(),
                    zone: capture.source.zone,
                    zone_change_count: capture.source.zone_change_count,
                }),
            ),
        };
        rows.push(InstallationV1 {
            origin,
            source,
            recipient,
            original_controller: capture.previous_controller.into(),
            captured_controller: capture.controller.into(),
            timestamp: capture.timestamp,
            installation_turn: capture.turn,
            current_recipient,
            current_tapped: current.map(|live| live.tapped),
            current_has_haste: current.map(|_| {
                crate::engine::has_effective_keyword(
                    state,
                    object,
                    crate::card_def::Keywords::HASTE,
                )
            }),
            expired: expiry.is_some(),
            cleanup_turn,
            duration: "until_next_cleanup_including_repeated_cleanup".into(),
        });
    }
    rows.sort_by_key(|row| row.timestamp);
    Ok(rows)
}
// V40 consumes these private typed parts; standalone predecessors retain their refusal path.
pub(crate) fn parts_for_akroma_v40(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor: PlayerId,
    step_index: u64,
    physical_decision_id: u64,
    substep: (u32, u32),
) -> Result<EmploymentObservationV1, RlContractError> {
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
) -> Result<EmploymentObservationV1, RlContractError> {
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
fn parts_for_common_scope(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor: PlayerId,
    step_index: u64,
    physical_decision_id: u64,
    substep: (u32, u32),
    scope: crate::rl::CommonProjectionScopeV42,
) -> Result<EmploymentObservationV1, RlContractError> {
    let (substep_index, substep_count) = substep;

    let installations = installations(state, actor)?;
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
    let observation = EmploymentObservationV1 {
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
        installations,
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
) -> Result<EmploymentObservationV1, RlContractError> {
    let installations = installations(state, actor)?;
    let base = crate::rl::employment_observation_storage_v1(
        state,
        surface,
        actor,
        step_index,
        physical_decision_id,
        (substep_index, substep_count),
    )?;
    let storage = crate::policy_observation_v8::observation_storage_v8(state, actor, base)?;
    let base = storage.base;
    let mut observation = EmploymentObservationV1 {
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
        installations,
        roots: roots()?,
        projection_sha256: String::new(),
    };
    observation.projection_sha256 = commitment(&observation)?;
    Ok(observation)
}
#[cfg(test)]
pub(crate) mod tests;
