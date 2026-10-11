//! Separate event-time Wildwood transport. This is not V8/V9 or model admission.
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

pub const SCHEMA: &str = "mtg-kernel/wildwood-public/v1";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CounterOccurrenceV1 {
    pub recipient_at_event: CardStableRefV1,
    pub observer_at_event: CardStableRefV1,
    pub placing_player: PlayerSeatV1,
    pub amount: i32,
    pub event_order: u64,
    pub recipient_was_creature: bool,
    pub recipient_was_hydra: bool,
    pub captured_controller: PlayerSeatV1,
    /// Bound source may have left or changed abilities; neither cancels capture.
    pub original_source_present: bool,
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
pub struct WildwoodObservationV1 {
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
    pub counter_occurrences: Vec<CounterOccurrenceV1>,
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
        include_str!("wildwood_public_v1.rs"),
        include_str!("wildwood_capture_v1.rs"),
        include_str!("wildwood_scourge_v1.rs"),
        include_str!("rules_meaning_wildwood_v1.rs"),
        include_str!("rl.rs"),
        include_str!("rl_session/wildwood_v16.rs"),
        include_str!("human_bo3_v1/wildwood_v16.rs"),
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
fn commitment(observation: &WildwoodObservationV1) -> Result<String, RlContractError> {
    let mut value = serde_json::to_value(observation)?;
    value
        .as_object_mut()
        .ok_or_else(|| RlContractError("Wildwood projection is not an object".into()))?
        .remove("projection_sha256");
    Ok(digest(&serde_json::to_vec(&value)?))
}
impl WildwoodObservationV1 {
    /// Checks transport identity only. Execution always rederives the entire
    /// observation and private ordered candidate binding from the owning session.
    pub fn validate_artifact(&self) -> Result<(), RlContractError> {
        if self.schema != SCHEMA
            || self.card_db_hash != crate::card_def::KERNEL_CARDDB_HASH
            || self.producer_sha256 != producer_sha256_v1()
            || self.projection_sha256 != commitment(self)?
        {
            return Err(RlContractError("wrong Wildwood artifact identity".into()));
        }
        let expected = roots()?;
        if self.roots != expected {
            return Err(RlContractError("wrong Wildwood printed root".into()));
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
    let Some(id) = crate::card_def::card_id_by_name("Wildwood Scourge") else {
        return Ok(Vec::new());
    };
    let selector = crate::rules_meaning_wildwood_v1::live_selector_v1()
        .map_err(|_| RlContractError("Wildwood catalog not supported".into()))?;
    let meaning = crate::rules_meaning_wildwood_v1::meaning_v1(&selector, id)
        .map_err(|_| RlContractError("Wildwood root not supported".into()))?;
    Ok(vec![RootV1 {
        card_id: id,
        ability_index: 0,
        root_sha256: meaning.counter_trigger.trigger_root_sha256,
    }])
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
) -> Result<WildwoodObservationV1, RlContractError> {
    let active = crate::wildwood_capture_v1::active(state).map_err(RlContractError)?;
    let mut counter_occurrences = Vec::new();
    let mut last_capture = None;
    let mut event_order = 0_u64;
    for (index, observer_index, capture) in active {
        if last_capture.is_some_and(|previous| previous != index) {
            event_order += 1;
        }
        last_capture = Some(index);
        let crate::state::StackTargetContractV4::Object {
            object,
            card_def,
            owner,
            controller,
            zone,
            zone_change_count,
            ..
        } = capture.recipient
        else {
            return Err(RlContractError("invalid captured recipient".into()));
        };
        let public_definition = if capture.recipient_face_down {
            crate::card_def::card_id_by_name("Face-down creature")
                .ok_or_else(|| RlContractError("public face-down sentinel missing".into()))?
        } else {
            card_def
        };
        let observer = capture.observers[observer_index];
        let crate::event::CommittedEvent::PlusOneCountersAdded { player, count, .. } =
            capture.event.as_ref()
        else {
            return Err(RlContractError("invalid captured counter".into()));
        };
        counter_occurrences.push(CounterOccurrenceV1 {
            recipient_at_event: CardStableRefV1 {
                arena_id: object.0,
                card_db_id: public_definition,
                owner: owner.into(),
                controller: controller.into(),
                zone,
                zone_change_count,
            },
            observer_at_event: CardStableRefV1 {
                arena_id: observer.source.0,
                card_db_id: observer.card_def,
                owner: observer.owner.into(),
                controller: observer.controller.into(),
                zone: observer.zone,
                zone_change_count: observer.zone_change_count,
            },
            placing_player: (*player).into(),
            amount: *count,
            event_order,
            recipient_was_creature: capture.recipient_is_creature,
            recipient_was_hydra: capture.recipient_is_hydra,
            captured_controller: observer.controller.into(),
            original_source_present: state
                .objects
                .try_get(observer.source)
                .is_some_and(|object| {
                    object.owner == observer.owner
                        && object.zone == Zone::Battlefield
                        && object.zone_change_count == observer.zone_change_count
                }),
        });
    }
    let base = crate::rl::wildwood_observation_storage_v1(
        state,
        surface,
        actor,
        step_index,
        physical_decision_id,
        (substep_index, substep_count),
    )?;
    let storage = crate::policy_observation_v8::observation_storage_v8(state, actor, base)?;
    let base = storage.base;
    let mut observation = WildwoodObservationV1 {
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
        counter_occurrences,
        roots: roots()?,
        projection_sha256: String::new(),
    };
    observation.projection_sha256 = commitment(&observation)?;
    Ok(observation)
}
