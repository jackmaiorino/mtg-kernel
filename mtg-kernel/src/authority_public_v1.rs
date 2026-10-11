//! Separate event-time Authority transport. This is not V8/V9 or model admission.
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

pub const SCHEMA: &str = "mtg-kernel/authority-public/v1";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntryOccurrenceV1 {
    pub entrant_at_entry: CardStableRefV1,
    pub entrant_face_index: u8,
    pub observer_at_entry: CardStableRefV1,
    pub beneficiary: PlayerSeatV1,
    pub event_order: u64,
    pub lifecycle: EntryLifecycleV1,
    pub stack_index: Option<u32>,
    pub pending_order_index: Option<u32>,
    pub original_source_present: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntryLifecycleV1 {
    Uncollected,
    Pending,
    LegendWaiting,
    ResolutionDeferred,
    Published,
    Gained,
    Countered,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActiveSourceV1 {
    pub source: CardStableRefV1,
    pub affected_entry_controller: PlayerSeatV1,
    pub opposing_creatures_enter_tapped: bool,
    pub mandatory_gain_per_entry: u8,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RootV1 {
    pub card_id: u16,
    pub ability_index: Option<u16>,
    pub root_sha256: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorityObservationV1 {
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
    pub entry_occurrences: Vec<EntryOccurrenceV1>,
    pub active_sources: Vec<ActiveSourceV1>,
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
        include_str!("authority_public_v1.rs"),
        include_str!("authority_public_capture_v1.rs"),
        include_str!("authority_of_the_consuls_v1.rs"),
        include_str!("rules_meaning_authority_v1.rs"),
        include_str!("sower_restriction_capture_v1.rs"),
        include_str!("rl.rs"),
        include_str!("rl_session/authority_v17.rs"),
        include_str!("human_bo3_v1/authority_v17.rs"),
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
fn commitment(observation: &AuthorityObservationV1) -> Result<String, RlContractError> {
    let mut value = serde_json::to_value(observation)?;
    value
        .as_object_mut()
        .ok_or_else(|| RlContractError("Authority projection is not an object".into()))?
        .remove("projection_sha256");
    Ok(digest(&serde_json::to_vec(&value)?))
}
impl AuthorityObservationV1 {
    /// Checks transport identity only. Execution always rederives the entire
    /// observation and private ordered candidate binding from the owning session.
    pub fn validate_artifact(&self) -> Result<(), RlContractError> {
        if self.schema != SCHEMA
            || self.card_db_hash != crate::card_def::KERNEL_CARDDB_HASH
            || self.producer_sha256 != producer_sha256_v1()
            || self.projection_sha256 != commitment(self)?
        {
            return Err(RlContractError("wrong Authority artifact identity".into()));
        }
        let expected = roots()?;
        if self.roots != expected {
            return Err(RlContractError("wrong Authority printed root".into()));
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
    let Some(id) = crate::card_def::card_id_by_name("Authority of the Consuls") else {
        return Ok(Vec::new());
    };
    let selector = crate::rules_meaning_authority_v1::live_selector_v1()
        .map_err(|_| RlContractError("Authority catalog not supported".into()))?;
    [
        crate::rules_meaning_authority_v1::RootV1::OpponentCreatureEntryTappedReplacement,
        crate::rules_meaning_authority_v1::RootV1::OpponentCreatureEntryGainLifeTrigger,
    ]
    .into_iter()
    .map(|root| {
        let meaning = crate::rules_meaning_authority_v1::meaning_v1(&selector, id, root)
            .map_err(|_| RlContractError("Authority root not supported".into()))?;
        Ok(RootV1 {
            card_id: id,
            ability_index: meaning.trigger_index,
            root_sha256: meaning.root_sha256,
        })
    })
    .collect()
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
) -> Result<AuthorityObservationV1, RlContractError> {
    crate::authority_public_capture_v1::validate_state(state).map_err(RlContractError)?;
    let mut entry_occurrences = Vec::new();
    let mut event_order = 0;
    let mut previous_entry = None;
    for event in &state.engine.event_history {
        let crate::event::CommittedEvent::AuthorityEntryCaptureV1 { capture } = event else {
            continue;
        };
        if previous_entry.is_some_and(|previous| previous != capture.observer_authority_index) {
            event_order += 1;
        }
        previous_entry = Some(capture.observer_authority_index);
        let facts =
            crate::authority_public_capture_v1::facts_at(state, capture.observer_authority_index)
                .map_err(RlContractError)?;
        let entrant_at_entry = facts
            .entrant
            .clone()
            .ok_or_else(|| RlContractError("public entrant sentinel missing".into()))?;
        let index = capture.history_index;
        let owns = |pending: &crate::trigger::PendingTrigger| {
            matches!(pending.effect,
            crate::effect::EffectOp::AuthorityEntryGainLifeV1 { capture_history_index } if capture_history_index == index)
        };
        let stack_index = state.stack.iter().position(|item| matches!(item.inline_effect,
            Some(crate::effect::EffectOp::AuthorityEntryGainLifeV1 { capture_history_index }) if capture_history_index == index))
            .map(u32::try_from).transpose().map_err(|_| RlContractError("public stack ordinal overflow".into()))?;
        let pending_order_index = state
            .engine
            .pending_triggers
            .iter()
            .position(owns)
            .map(u32::try_from)
            .transpose()
            .map_err(|_| RlContractError("public trigger ordinal overflow".into()))?;
        let lifecycle = if let Some(kind) =
            state
                .engine
                .event_history
                .iter()
                .find_map(|event| match event {
                    crate::event::CommittedEvent::AuthorityEntryOperationV1 {
                        capture_history_index,
                        kind,
                        ..
                    } if *capture_history_index == index => Some(kind),
                    _ => None,
                }) {
            match kind { crate::authority_of_the_consuls_v1::AuthorityEntryOperationKindV1::GainCommitted { .. } => EntryLifecycleV1::Gained,
                crate::authority_of_the_consuls_v1::AuthorityEntryOperationKindV1::CounterCommitted => EntryLifecycleV1::Countered }
        } else if stack_index.is_some() {
            EntryLifecycleV1::Published
        } else if state.engine.pending_triggers.iter().any(owns) {
            EntryLifecycleV1::Pending
        } else if state.legend_pending_v1.iter().flatten().any(owns)
            || state
                .pending_legend_rule_v1
                .iter()
                .flat_map(|pending| pending.waiting_triggers())
                .any(owns)
        {
            EntryLifecycleV1::LegendWaiting
        } else if state
            .standard_v1
            .iter()
            .filter_map(|standard| standard.resolution_play.as_ref())
            .flat_map(|resolution| resolution.deferred_triggers.iter())
            .any(owns)
        {
            EntryLifecycleV1::ResolutionDeferred
        } else {
            EntryLifecycleV1::Uncollected
        };
        let source = capture.source;
        entry_occurrences.push(EntryOccurrenceV1 {
            entrant_at_entry,
            entrant_face_index: facts.entrant_face_index,
            observer_at_entry: CardStableRefV1 {
                arena_id: source.source.0,
                card_db_id: source.card_def,
                owner: source.owner.into(),
                controller: source.controller.into(),
                zone: source.zone,
                zone_change_count: source.zone_change_count,
            },
            beneficiary: source.controller.into(),
            event_order,
            lifecycle,
            stack_index,
            pending_order_index,
            original_source_present: state.objects.try_get(source.source).is_some_and(|live| {
                live.owner == source.owner
                    && live.zone == Zone::Battlefield
                    && live.zone_change_count == source.zone_change_count
            }),
        });
    }
    let active_sources = state
        .objects
        .iter()
        .filter(|(source, _)| crate::authority_of_the_consuls_v1::active(state, *source))
        .map(|(source, live)| {
            Ok(ActiveSourceV1 {
                source: crate::sower_restriction_capture_v1::masked_ref(source, live)?,
                affected_entry_controller: live.controller.opponent().into(),
                opposing_creatures_enter_tapped: true,
                mandatory_gain_per_entry: 1,
            })
        })
        .collect::<Result<Vec<_>, RlContractError>>()?;
    let base = crate::rl::authority_observation_storage_v1(
        state,
        surface,
        actor,
        step_index,
        physical_decision_id,
        (substep_index, substep_count),
    )?;
    let storage = crate::policy_observation_v8::observation_storage_v8(state, actor, base)?;
    let mut base = storage.base;
    // Pending Authority rows always name the captured observer, including a
    // visible source whose controller changed. Ordinary rows remain untouched.
    for row in &entry_occurrences {
        if let Some(position) = row.pending_order_index {
            let pending = base
                .projection
                .surface
                .engine_context
                .pending_triggers
                .get_mut(position as usize)
                .ok_or_else(|| {
                    RlContractError("Authority public pending position missing".into())
                })?;
            pending.source = Some(row.observer_at_entry.clone());
        }
    }
    let mut observation = AuthorityObservationV1 {
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
        entry_occurrences,
        active_sources,
        roots: roots()?,
        projection_sha256: String::new(),
    };
    observation.projection_sha256 = commitment(&observation)?;
    Ok(observation)
}
