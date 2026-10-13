//! Separate event-time ExtraTurn transport. This is not V8/V9 or model admission.
use crate::ids::PlayerId;
use crate::policy_observation_v8::PolicyObservationExtensionsV8;
use crate::policy_surface_v5::PolicySurfaceV5;
use crate::rl::{
    CardPrivateV1, KnownLibraryCardV4, PlayerSeatV1, PublicObservationProjectionV5, RlContractError,
};
use crate::state::GameState;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA: &str = "mtg-kernel/extra-turn-public/v1";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RootV1 {
    pub card_id: u16,
    pub ability_index: u16,
    pub root_sha256: String,
}
/// Public scheduling provenance, with no arena or executable stack identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScheduledTurnV1 {
    pub creation_ordinal: u64,
    pub captured_controller: PlayerSeatV1,
    pub physical_owner: PlayerSeatV1,
    pub spell_copy: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TurnScheduleV1 {
    pub occurrence: u32,
    pub legacy_round: u32,
    pub own_turns_started: [u32; 2],
    pub previous_player: PlayerSeatV1,
    pub next_regular_player: PlayerSeatV1,
    pub current_extra: Option<ScheduledTurnV1>,
    /// Next turn first, including extra turns created during an extra turn.
    pub queued_next_first: Vec<ScheduledTurnV1>,
}
fn schedule(state: &GameState) -> Result<Option<TurnScheduleV1>, RlContractError> {
    let Some(turn) = &state.extra_turn_v1 else {
        return Ok(None);
    };
    let scheduled: Vec<_> = state
        .engine
        .event_history
        .iter()
        .filter_map(|event| {
            if let crate::event::CommittedEvent::ExtraTurnScheduledV1 { item } = event {
                Some(item.as_ref())
            } else {
                None
            }
        })
        .collect();
    let row = |id: crate::ids::StackItemId| -> Result<ScheduledTurnV1, RlContractError> {
        let (ordinal, item) = scheduled
            .iter()
            .enumerate()
            .find(|(_, item)| item.v4.stack_item_id == id)
            .ok_or_else(|| {
                RlContractError("extra turn has no original scheduling occurrence".into())
            })?;
        let source = item
            .v4
            .source_contract
            .ok_or_else(|| RlContractError("extra turn source capture missing".into()))?;
        Ok(ScheduledTurnV1 {
            creation_ordinal: u64::try_from(ordinal)
                .map_err(|_| RlContractError("schedule ordinal overflow".into()))?,
            captured_controller: item.controller.into(),
            physical_owner: source.owner.into(),
            spell_copy: item.is_copy,
        })
    };
    Ok(Some(TurnScheduleV1 {
        occurrence: turn.occurrence,
        legacy_round: state.turn,
        own_turns_started: turn.own_turns_started,
        previous_player: turn.previous_player.into(),
        next_regular_player: turn.next_regular_player.into(),
        current_extra: turn.current_extra.map(row).transpose()?,
        queued_next_first: turn
            .queued
            .iter()
            .rev()
            .map(|item| row(item.item.v4.stack_item_id))
            .collect::<Result<Vec<_>, _>>()?,
    }))
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtraTurnObservationV1 {
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
    pub creature_died_this_turn: bool,
    pub turn_schedule: Option<TurnScheduleV1>,
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
        include_str!("extra_turn_public_v1.rs"),
        include_str!("extra_turn_v1.rs"),
        include_str!("rules_meaning_extra_turn_v1.rs"),
        include_str!("rl_session/extra_turn_v23.rs"),
        include_str!("human_bo3_v1/extra_turn_v23.rs"),
        include_str!("human_bo3_v1/labels.rs"),
        include_str!("human_bo3_v1/visible.rs"),
        include_str!("human_bo3_v1/life_ward_v8.rs"),
        include_str!("engine.rs"),
        include_str!("event.rs"),
        include_str!("effect.rs"),
        include_str!("trigger.rs"),
        include_str!("standard_cards_v1.rs"),
        include_str!("standard_spell_copy.rs"),
        include_str!("effect/standard_copy.rs"),
        include_str!("continuous_characteristics_v1.rs"),
        include_str!("standard_keywords_v1.rs"),
        include_str!("standard_creatures_v1.rs"),
        include_str!("life_gain_turn_v1.rs"),
        include_str!("card_def.rs"),
        include_str!("../build.rs"),
        include_str!("rl.rs"),
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
fn commitment(observation: &ExtraTurnObservationV1) -> Result<String, RlContractError> {
    let mut value = serde_json::to_value(observation)?;
    value
        .as_object_mut()
        .ok_or_else(|| RlContractError("ExtraTurn projection is not an object".into()))?
        .remove("projection_sha256");
    Ok(digest(&serde_json::to_vec(&value)?))
}
impl ExtraTurnObservationV1 {
    /// Checks transport identity only. Execution always rederives the entire
    /// observation and private ordered candidate binding from the owning session.
    pub fn validate_artifact(&self) -> Result<(), RlContractError> {
        if self.schema != SCHEMA
            || self.card_db_hash != crate::card_def::KERNEL_CARDDB_HASH
            || self.producer_sha256 != producer_sha256_v1()
            || self.projection_sha256 != commitment(self)?
        {
            return Err(RlContractError("wrong ExtraTurn artifact identity".into()));
        }
        let expected = roots()?;
        if self.roots != expected {
            return Err(RlContractError("wrong ExtraTurn printed root".into()));
        }
        Ok(())
    }
}
fn roots() -> Result<Vec<RootV1>, RlContractError> {
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) {
        return Ok(Vec::new());
    }
    let Some(id) = crate::card_def::card_id_by_name("Temporal Manipulation") else {
        return Ok(Vec::new());
    };
    let selector = crate::rules_meaning_extra_turn_v1::live_selector()
        .map_err(|_| RlContractError("ExtraTurn catalog not supported".into()))?;
    let meaning = crate::rules_meaning_extra_turn_v1::meaning(
        &selector,
        id,
        crate::rules_meaning_extra_turn_v1::RootV1::PrintedSpell,
    )
    .map_err(|_| RlContractError("Temporal printed root not supported".into()))?;
    Ok(vec![RootV1 {
        card_id: id,
        ability_index: 0,
        root_sha256: meaning.root_sha256,
    }])
}
/// Authenticates selected roots and the independent event-time capture lifecycle.
pub(crate) fn validate_state(state: &GameState) -> Result<(), String> {
    crate::effect::validate_pending_effect_choice(state)?;
    for pending in &state.engine.pending_triggers {
        crate::engine::validate_pending_trigger_identity(state, pending)?;
        if pending.placement_ordered {
            crate::engine::validate_pending_trigger(state, pending)?;
        }
    }
    for item in &state.stack {
        crate::engine::validated_stack_item_target_spec(item, state)?;
    }
    Ok(())
}
// V40 consumes these private typed parts; standalone predecessors retain their refusal path.
pub(crate) fn parts_for_akroma_v40(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor: PlayerId,
    step_index: u64,
    physical_decision_id: u64,
    substep: (u32, u32),
) -> Result<ExtraTurnObservationV1, RlContractError> {
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
) -> Result<ExtraTurnObservationV1, RlContractError> {
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
) -> Result<ExtraTurnObservationV1, RlContractError> {
    let (substep_index, substep_count) = substep;

    validate_state(state).map_err(RlContractError)?;
    let turn_schedule = schedule(state)?;
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
    let observation = ExtraTurnObservationV1 {
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
        creature_died_this_turn: state.creature_died_this_turn_v1(),
        turn_schedule,
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
) -> Result<ExtraTurnObservationV1, RlContractError> {
    validate_state(state).map_err(RlContractError)?;
    let turn_schedule = schedule(state)?;
    let base = crate::rl::extra_turn_observation_storage_v1(
        state,
        surface,
        actor,
        step_index,
        physical_decision_id,
        (substep_index, substep_count),
    )?;
    let storage = crate::policy_observation_v8::observation_storage_v8(state, actor, base)?;
    let base = storage.base;
    let mut observation = ExtraTurnObservationV1 {
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
        creature_died_this_turn: state.creature_died_this_turn_v1(),
        turn_schedule,
        roots: roots()?,
        projection_sha256: String::new(),
    };
    observation.projection_sha256 = commitment(&observation)?;
    Ok(observation)
}
