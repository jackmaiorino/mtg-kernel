//! Separate event-time Cerberus transport. This is not V8/V9 or model admission.
use crate::ids::PlayerId;
use crate::policy_observation_v8::PolicyObservationExtensionsV8;
use crate::policy_surface_v5::PolicySurfaceV5;
use crate::rl::{
    CardPrivateV1, CardStableRefV1, KnownLibraryCardV4, PlayerSeatV1,
    PublicObservationProjectionV5, RlContractError,
};
use crate::state::{GameState, Zone};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub use crate::cerberus_capture_v1::CerberusOperationKindV1 as OutcomeV1;

pub const SCHEMA: &str = "mtg-kernel/cerberus-public/v1";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TriggerV1 {
    pub stack_ordinal: Option<u32>,
    pub phase: String,
    pub end_step_player: PlayerSeatV1,
    pub morbid_at_capture: bool,
    pub outcome: Option<OutcomeV1>,
    pub source_at_trigger: CardStableRefV1,
    pub captured_controller: PlayerSeatV1,
    pub original_source_present: bool,
    pub currently_tapped: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UntapRestrictionV1 {
    pub source: CardStableRefV1,
    pub current_controller: PlayerSeatV1,
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
pub struct CerberusObservationV1 {
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
    pub untap_restrictions: Vec<UntapRestrictionV1>,
    pub triggers: Vec<TriggerV1>,
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
        include_str!("cerberus_public_v1.rs"),
        include_str!("sower_restriction_capture_v1.rs"),
        include_str!("slumbering_cerberus_v1.rs"),
        include_str!("cerberus_capture_v1.rs"),
        include_str!("standard_cards_v1.rs"),
        include_str!("rules_meaning_cerberus_v1.rs"),
        include_str!("effect.rs"),
        include_str!("card_def.rs"),
        include_str!("../build.rs"),
        include_str!("rl.rs"),
        include_str!("rl_session/cerberus_v19.rs"),
        include_str!("human_bo3_v1/cerberus_v19.rs"),
        include_str!("human_bo3_v1/labels.rs"),
        include_str!("human_bo3_v1/visible.rs"),
        include_str!("human_bo3_v1/life_ward_v8.rs"),
        include_str!("engine.rs"),
        include_str!("event.rs"),
        include_str!("effect.rs"),
        include_str!("trigger.rs"),
        include_str!("card_def.rs"),
        include_str!("continuous_characteristics_v1.rs"),
        include_str!("standard_cards_v1.rs"),
        include_str!("../build.rs"),
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
fn commitment(observation: &CerberusObservationV1) -> Result<String, RlContractError> {
    let mut value = serde_json::to_value(observation)?;
    value
        .as_object_mut()
        .ok_or_else(|| RlContractError("Cerberus projection is not an object".into()))?
        .remove("projection_sha256");
    Ok(digest(&serde_json::to_vec(&value)?))
}
impl CerberusObservationV1 {
    /// Checks transport identity only. Execution always rederives the entire
    /// observation and private ordered candidate binding from the owning session.
    pub fn validate_artifact(&self) -> Result<(), RlContractError> {
        if self.schema != SCHEMA
            || self.card_db_hash != crate::card_def::KERNEL_CARDDB_HASH
            || self.producer_sha256 != producer_sha256_v1()
            || self.projection_sha256 != commitment(self)?
        {
            return Err(RlContractError("wrong Cerberus artifact identity".into()));
        }
        let expected = roots()?;
        if self.roots != expected {
            return Err(RlContractError("wrong Cerberus printed root".into()));
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
    let Some(id) = crate::card_def::card_id_by_name("Slumbering Cerberus") else {
        return Ok(Vec::new());
    };
    let selector = crate::rules_meaning_cerberus_v1::live_selector()
        .map_err(|_| RlContractError("Cerberus catalog not supported".into()))?;
    [
        crate::rules_meaning_cerberus_v1::RootV1::ControllerUntapStepRestriction,
        crate::rules_meaning_cerberus_v1::RootV1::PrintedEndStepTrigger { trigger_index: 0 },
    ]
    .into_iter()
    .enumerate()
    .map(|(index, root)| {
        let meaning = crate::rules_meaning_cerberus_v1::meaning(&selector, id, root)
            .map_err(|_| RlContractError("Cerberus root not supported".into()))?;
        Ok(RootV1 {
            card_id: id,
            ability_index: index as u16,
            root_sha256: meaning.root_sha256,
        })
    })
    .collect()
}
/// Authenticates selected roots and the independent event-time capture lifecycle.
pub(crate) fn validate_state(state: &GameState) -> Result<(), String> {
    crate::cerberus_capture_v1::validate_state(state)?;
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
fn trigger_rows(state: &GameState) -> Result<Vec<TriggerV1>, RlContractError> {
    use crate::event::CommittedEvent;
    let mut rows = Vec::new();
    for event in &state.engine.event_history {
        let CommittedEvent::CerberusCaptureV1 { capture } = event else {
            continue;
        };
        let contract = capture.source;
        let index = usize::try_from(capture.begin_history_index)
            .map_err(|_| RlContractError("captured end-step index exceeds usize".into()))?;
        let Some(CommittedEvent::BeginningEndStep {
            active_player,
            creature_died_this_turn,
        }) = state.engine.event_history.get(index)
        else {
            return Err(RlContractError(
                "captured end-step occurrence missing".into(),
            ));
        };
        let stack_ordinal = state
            .stack
            .iter()
            .position(|item| {
                item.inline_effect
                    == Some(crate::effect::EffectOp::CerberusUntapV1 {
                        capture_history_index: capture.history_index,
                    })
            })
            .map(|index| {
                u32::try_from(index).map_err(|_| RlContractError("stack ordinal overflow".into()))
            })
            .transpose()?;
        let outcome = state
            .engine
            .event_history
            .iter()
            .find_map(|event| match event {
                CommittedEvent::CerberusOperationV1 {
                    capture_history_index,
                    kind,
                    ..
                } if *capture_history_index == capture.history_index => Some(kind.clone()),
                _ => None,
            });
        let phase = if outcome.is_some() {
            "completed"
        } else if stack_ordinal.is_some() {
            "published"
        } else if state.engine.event_history.iter().any(|event| {
            matches!(event,
            CommittedEvent::CerberusCollectedV1 { capture_history_index }
                if *capture_history_index == capture.history_index)
        }) {
            "waiting"
        } else {
            "captured"
        };
        let live = state.objects.try_get(contract.source).filter(|object| {
            object.owner == contract.owner
                && object.zone == Zone::Battlefield
                && object.zone_change_count == contract.zone_change_count
        });
        rows.push(TriggerV1 {
            stack_ordinal,
            phase: phase.into(),
            end_step_player: (*active_player).into(),
            morbid_at_capture: *creature_died_this_turn,
            outcome,
            source_at_trigger: CardStableRefV1 {
                arena_id: contract.source.0,
                card_db_id: contract.card_def,
                owner: contract.owner.into(),
                controller: contract.controller.into(),
                zone: contract.zone,
                zone_change_count: contract.zone_change_count,
            },
            captured_controller: contract.controller.into(),
            original_source_present: live.is_some(),
            currently_tapped: live.is_some_and(|object| object.tapped),
        });
    }
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
) -> Result<CerberusObservationV1, RlContractError> {
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
) -> Result<CerberusObservationV1, RlContractError> {
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
pub(crate) fn parts_for_faebloom_v43(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor: PlayerId,
    step_index: u64,
    physical_decision_id: u64,
    substep: (u32, u32),
) -> Result<CerberusObservationV1, RlContractError> {
    parts_for_common_scope(
        state,
        surface,
        actor,
        step_index,
        physical_decision_id,
        substep,
        crate::rl::CommonProjectionScopeV42::FullFaebloomV43,
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
) -> Result<CerberusObservationV1, RlContractError> {
    let (substep_index, substep_count) = substep;

    validate_state(state).map_err(RlContractError)?;
    let triggers = trigger_rows(state)?;
    let mut untap_restrictions = Vec::new();
    for (id, object) in state.objects.iter() {
        if crate::slumbering_cerberus_v1::prevents_untap_step(state, id) {
            untap_restrictions.push(UntapRestrictionV1 {
                source: crate::sower_restriction_capture_v1::masked_ref(id, object)?,
                current_controller: object.controller.into(),
            });
        }
    }
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
    let observation = CerberusObservationV1 {
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
        untap_restrictions,
        triggers,
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
) -> Result<CerberusObservationV1, RlContractError> {
    validate_state(state).map_err(RlContractError)?;
    let triggers = trigger_rows(state)?;
    let mut untap_restrictions = Vec::new();
    for (id, object) in state.objects.iter() {
        if crate::slumbering_cerberus_v1::prevents_untap_step(state, id) {
            untap_restrictions.push(UntapRestrictionV1 {
                source: crate::sower_restriction_capture_v1::masked_ref(id, object)?,
                current_controller: object.controller.into(),
            });
        }
    }
    let base = crate::rl::cerberus_observation_storage_v1(
        state,
        surface,
        actor,
        step_index,
        physical_decision_id,
        (substep_index, substep_count),
    )?;
    let storage = crate::policy_observation_v8::observation_storage_v8(state, actor, base)?;
    let base = storage.base;
    let mut observation = CerberusObservationV1 {
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
        untap_restrictions,
        triggers,
        roots: roots()?,
        projection_sha256: String::new(),
    };
    observation.projection_sha256 = commitment(&observation)?;
    Ok(observation)
}
#[cfg(test)]
mod tests;
