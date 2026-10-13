//! Separate event-time Arahbo transport. This is not V8/V9 or model admission.
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

pub const SCHEMA: &str = "mtg-kernel/arahbo-public/v1";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntryOccurrenceV1 {
    pub entrant_at_entry: CardStableRefV1,
    pub entrant_face_index: u8,
    pub root: crate::arahbo_v1::RootV1,
    pub entrant_was_token: bool,
    pub created_token: Option<CardStableRefV1>,
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
    TokenCreated,
    Countered,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActiveSourceV1 {
    pub source: CardStableRefV1,
    pub controller: PlayerSeatV1,
    pub other_controlled_cat_creatures: Vec<CardStableRefV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RootV1 {
    pub card_id: u16,
    pub ability_index: Option<u16>,
    pub root: crate::rules_meaning_arahbo_v1::RootV1,
    pub root_sha256: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LinkedExileFactsV1 {
    pub triggers: Vec<crate::banishing_public_v1::TriggerV1>,
    pub links: Vec<crate::banishing_public_v1::LinkV1>,
    pub attachment_choice: Option<crate::banishing_public_v1::AttachmentChoiceV1>,
    pub confirmed_attachments: Vec<crate::banishing_public_v1::ConfirmedAttachmentV1>,
    pub roots: Vec<crate::banishing_public_v1::RootV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArahboObservationV1 {
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
    pub linked_exile: LinkedExileFactsV1,
    pub garruk_occurrences: Vec<crate::garruk_public_v1::EntryOccurrenceV1>,
    pub garruk_active_sources: Vec<crate::garruk_public_v1::ActiveSourceV1>,
    pub garruk_roots: Vec<crate::garruk_public_v1::RootV1>,
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
        include_str!("arahbo_public_v1.rs"),
        include_str!("garruk_public_v1.rs"),
        include_str!("garruks_uprising_v1.rs"),
        include_str!("rules_meaning_garruk_v1.rs"),
        include_str!("arahbo_v1.rs"),
        include_str!("banishing_public_v1.rs"),
        include_str!("banishing_light_v1.rs"),
        include_str!("linked_aura_operation_v1.rs"),
        include_str!("linked_aura_return_v1.rs"),
        include_str!("rules_meaning_banishing_v1.rs"),
        include_str!("rules_meaning_arahbo_v1.rs"),
        include_str!("sower_restriction_capture_v1.rs"),
        include_str!("rl.rs"),
        include_str!("rl_session/arahbo_v31.rs"),
        include_str!("human_bo3_v1/arahbo_v31.rs"),
        include_str!("human_bo3_v1/garruk_v30.rs"),
        include_str!("human_bo3_v1/banishing_v26.rs"),
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
        include_str!("standard_statics_v1.rs"),
        include_str!("standard_legends_v1.rs"),
        include_str!("standard_keywords_v1.rs"),
        include_str!("fdn_raid_v1.rs"),
        include_str!("mana.rs"),
        include_str!("policy_surface_v5.rs"),
        include_str!("surface_v2.rs"),
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
fn commitment(observation: &ArahboObservationV1) -> Result<String, RlContractError> {
    let mut value = serde_json::to_value(observation)?;
    value
        .as_object_mut()
        .ok_or_else(|| RlContractError("Arahbo projection is not an object".into()))?
        .remove("projection_sha256");
    Ok(digest(&serde_json::to_vec(&value)?))
}
impl ArahboObservationV1 {
    /// Checks transport identity only. Execution always rederives the entire
    /// observation and private ordered candidate binding from the owning session.
    pub fn validate_artifact(&self) -> Result<(), RlContractError> {
        if self.schema != SCHEMA
            || self.card_db_hash != crate::card_def::KERNEL_CARDDB_HASH
            || self.producer_sha256 != producer_sha256_v1()
            || self.projection_sha256 != commitment(self)?
        {
            return Err(RlContractError("wrong Arahbo artifact identity".into()));
        }
        let expected = roots()?;
        if self.roots != expected
            || self.linked_exile.roots != crate::banishing_public_v1::roots()?
            || self.garruk_roots != crate::garruk_public_v1::roots()?
        {
            return Err(RlContractError("wrong Arahbo printed root".into()));
        }
        Ok(())
    }
}
pub(crate) fn roots() -> Result<Vec<RootV1>, RlContractError> {
    // Ordinary Default/Standard sessions still have a real successor identity,
    // but never fabricate the missing Limited definition or claim its meaning.
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) {
        return Ok(Vec::new());
    }
    let Some(id) = crate::card_def::card_id_by_name("Arahbo, the First Fang") else {
        return Ok(Vec::new());
    };
    let selector = crate::rules_meaning_arahbo_v1::live_selector_v1()
        .map_err(|_| RlContractError("Arahbo catalog not supported".into()))?;
    [
        crate::rules_meaning_arahbo_v1::RootV1::OtherControlledCatCreaturesGetPlusOne,
        crate::rules_meaning_arahbo_v1::RootV1::ThisOrOtherControlledNontokenCatPermanentCreatesCat,
    ]
    .into_iter()
    .map(|root| {
        let meaning = crate::rules_meaning_arahbo_v1::meaning_v1(&selector, id, root)
            .map_err(|_| RlContractError("Arahbo root not supported".into()))?;
        Ok(RootV1 {
            card_id: id,
            root,
            ability_index: meaning.trigger_index,
            root_sha256: meaning.root_sha256,
        })
    })
    .collect()
}

pub(crate) fn facts(
    state: &GameState,
) -> Result<(Vec<EntryOccurrenceV1>, Vec<ActiveSourceV1>), RlContractError> {
    crate::arahbo_v1::validate_state(state).map_err(RlContractError)?;
    let mut entry_occurrences = Vec::new();
    let mut event_order = 0_u64;
    let mut previous_entry = None;
    for event in &state.engine.event_history {
        let crate::event::CommittedEvent::ArahboTokenCaptureV1 { capture } = event else {
            continue;
        };
        crate::arahbo_v1::capture_at(state, capture.history_index).map_err(RlContractError)?;
        let Some(crate::event::CommittedEvent::ArahboEntryObservationV1 {
            entry, was_token, ..
        }) = state
            .engine
            .event_history
            .get(capture.observation_history_index as usize)
        else {
            return Err(RlContractError(
                "Arahbo original entry facts missing".into(),
            ));
        };
        if previous_entry.is_some_and(|old| old != entry.entry_history_index) {
            event_order = event_order
                .checked_add(1)
                .ok_or_else(|| RlContractError("Arahbo event ordinal overflow".into()))?;
        }
        previous_entry = Some(entry.entry_history_index);
        let index = capture.history_index;
        let owns = |pending: &crate::trigger::PendingTrigger| {
            matches!(pending.effect,
            crate::effect::EffectOp::ArahboTokenFromCaptureV1 { capture_history_index } if capture_history_index == index)
        };
        let stack_index = state.stack.iter().position(|item| matches!(item.inline_effect,
            Some(crate::effect::EffectOp::ArahboTokenFromCaptureV1 { capture_history_index }) if capture_history_index == index))
            .map(u32::try_from).transpose().map_err(|_| RlContractError("Arahbo stack ordinal overflow".into()))?;
        let pending_order_index = state
            .engine
            .pending_triggers
            .iter()
            .position(owns)
            .map(u32::try_from)
            .transpose()
            .map_err(|_| RlContractError("Arahbo pending ordinal overflow".into()))?;
        let completed = state
            .engine
            .event_history
            .iter()
            .find_map(|event| match event {
                crate::event::CommittedEvent::ArahboTokenCompletedV1 {
                    capture_history_index,
                    removal_history_index,
                    token_history_index,
                } if *capture_history_index == index => {
                    Some((*removal_history_index, *token_history_index))
                }
                _ => None,
            });
        let created_token = completed
            .and_then(|(_, token)| token)
            .map(|index| {
                let Some(crate::event::CommittedEvent::CreateToken {
                    object,
                    token_def,
                    controller,
                }) = state.engine.event_history.get(index as usize)
                else {
                    return Err(RlContractError(
                        "Arahbo actual token occurrence missing".into(),
                    ));
                };
                let snapshot = state
                    .engine
                    .event_history
                    .iter()
                    .find_map(|event| match event {
                        crate::event::CommittedEvent::ArahboEntryObservationV1 {
                            entry, ..
                        } if entry.entry_history_index == index => Some(&entry.snapshot),
                        _ => None,
                    })
                    .ok_or_else(|| {
                        RlContractError("Arahbo created token entry snapshot missing".into())
                    })?;
                Ok(CardStableRefV1 {
                    arena_id: object.0,
                    card_db_id: *token_def,
                    owner: (*controller).into(),
                    controller: (*controller).into(),
                    zone: Zone::Battlefield,
                    zone_change_count: snapshot.zone_change_count,
                })
            })
            .transpose()?;
        let lifecycle = if let Some((removal, _token)) = completed {
            let Some(crate::event::CommittedEvent::ArahboTokenRemovedV1 { kind, .. }) =
                state.engine.event_history.get(removal as usize)
            else {
                return Err(RlContractError(
                    "Arahbo actual removal facts missing".into(),
                ));
            };
            match kind {
                crate::arahbo_v1::RemovalKindV1::Countered => EntryLifecycleV1::Countered,
                crate::arahbo_v1::RemovalKindV1::Resolving => EntryLifecycleV1::TokenCreated,
            }
        } else if stack_index.is_some() {
            EntryLifecycleV1::Published
        } else if state.engine.pending_triggers.iter().any(owns) {
            EntryLifecycleV1::Pending
        } else if state.legend_pending_v1.iter().flatten().any(owns)
            || state
                .pending_legend_rule_v1
                .iter()
                .flat_map(|p| p.waiting_triggers())
                .any(owns)
        {
            EntryLifecycleV1::LegendWaiting
        } else if state
            .standard_v1
            .iter()
            .filter_map(|s| s.resolution_play.as_ref())
            .flat_map(|p| p.deferred_triggers.iter())
            .any(owns)
        {
            EntryLifecycleV1::ResolutionDeferred
        } else {
            EntryLifecycleV1::Uncollected
        };
        let source = capture.source;
        entry_occurrences.push(EntryOccurrenceV1 {
            entrant_at_entry: crate::sower_restriction_capture_v1::masked_ref(
                entry.object,
                &entry.snapshot,
            )?,
            entrant_face_index: if entry.snapshot.v4.face_down_v1.is_some() {
                0
            } else {
                entry.snapshot.v4.face_index
            },
            root: capture.root,
            entrant_was_token: *was_token,
            created_token,
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
        .filter(|(source, _)| crate::arahbo_v1::active(state, *source))
        .map(|(source, live)| {
            let recipients = state.players[live.controller.index()]
                .battlefield
                .iter()
                .copied()
                .filter(|id| {
                    crate::engine::object_has_type(state, *id, crate::card_def::CardType::Creature)
                        && *id != source
                        && crate::card_def::Subtype::Cat
                            .is_in_subtype_ids(&crate::engine::effective_subtype_ids(state, *id))
                })
                .map(|id| {
                    crate::sower_restriction_capture_v1::masked_ref(id, state.objects.get(id))
                })
                .collect::<Result<Vec<_>, RlContractError>>()?;
            Ok(ActiveSourceV1 {
                source: crate::sower_restriction_capture_v1::masked_ref(source, live)?,
                controller: live.controller.into(),
                other_controlled_cat_creatures: recipients,
            })
        })
        .collect::<Result<Vec<_>, RlContractError>>()?;
    Ok((entry_occurrences, active_sources))
}

// V40 consumes these private typed parts; standalone predecessors retain their refusal path.
pub(crate) fn parts_for_akroma_v40(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor: PlayerId,
    step_index: u64,
    physical_decision_id: u64,
    substep: (u32, u32),
) -> Result<ArahboObservationV1, RlContractError> {
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
) -> Result<ArahboObservationV1, RlContractError> {
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
) -> Result<ArahboObservationV1, RlContractError> {
    let (substep_index, substep_count) = substep;

    crate::arahbo_v1::validate_state(state).map_err(RlContractError)?;
    let (triggers, links, attachment_choice, confirmed_attachments) =
        crate::banishing_public_v1::facts(state)?;
    let linked_exile = LinkedExileFactsV1 {
        triggers,
        links,
        attachment_choice,
        confirmed_attachments,
        roots: crate::banishing_public_v1::roots()?,
    };
    let (entry_occurrences, active_sources) = facts(state)?;
    let (garruk_occurrences, garruk_active_sources) = crate::garruk_public_v1::facts(state)?;
    let base = crate::rl::common_family_observation_storage_scoped(
        (scope, state),
        surface,
        actor,
        step_index,
        physical_decision_id,
        (substep_index, substep_count),
    )?;
    let storage = crate::policy_observation_v8::observation_storage_v8(state, actor, base)?;
    let mut base = storage.base;
    if linked_exile.attachment_choice.is_some() {
        base.projection.surface.engine_context.current_stage =
            crate::rl::EngineDecisionStageV2::PendingEffect;
    }
    for row in &entry_occurrences {
        if let Some(position) = row.pending_order_index {
            let pending = base
                .projection
                .surface
                .engine_context
                .pending_triggers
                .get_mut(position as usize)
                .ok_or_else(|| {
                    RlContractError("Arahbo original pending position missing".into())
                })?;
            pending.source = Some(row.observer_at_entry.clone());
        }
    }
    for row in &garruk_occurrences {
        if let Some(position) = row.pending_order_index {
            let pending = base
                .projection
                .surface
                .engine_context
                .pending_triggers
                .get_mut(position as usize)
                .ok_or_else(|| RlContractError("Arahbo/Garruk pending position missing".into()))?;
            pending.source = Some(row.observer_at_entry.clone());
        }
    }
    let observation = ArahboObservationV1 {
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
        entry_occurrences,
        active_sources,
        roots: roots()?,
        linked_exile,
        garruk_occurrences,
        garruk_active_sources,
        garruk_roots: crate::garruk_public_v1::roots()?,
        projection_sha256: String::new(),
    };
    Ok(observation)
}
pub fn observe_v1(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor: PlayerId,
    step_index: u64,
    physical_decision_id: u64,
    substep_index: u32,
    substep_count: u32,
) -> Result<ArahboObservationV1, RlContractError> {
    crate::arahbo_v1::validate_state(state).map_err(RlContractError)?;
    let (triggers, links, attachment_choice, confirmed_attachments) =
        crate::banishing_public_v1::facts(state)?;
    let linked_exile = LinkedExileFactsV1 {
        triggers,
        links,
        attachment_choice,
        confirmed_attachments,
        roots: crate::banishing_public_v1::roots()?,
    };
    let (entry_occurrences, active_sources) = facts(state)?;
    let (garruk_occurrences, garruk_active_sources) = crate::garruk_public_v1::facts(state)?;
    let base = crate::rl::arahbo_observation_storage_v1(
        state,
        surface,
        actor,
        step_index,
        physical_decision_id,
        (substep_index, substep_count),
    )?;
    let storage = crate::policy_observation_v8::observation_storage_v8(state, actor, base)?;
    let mut base = storage.base;
    if linked_exile.attachment_choice.is_some() {
        base.projection.surface.engine_context.current_stage =
            crate::rl::EngineDecisionStageV2::PendingEffect;
    }
    for row in &entry_occurrences {
        if let Some(position) = row.pending_order_index {
            let pending = base
                .projection
                .surface
                .engine_context
                .pending_triggers
                .get_mut(position as usize)
                .ok_or_else(|| {
                    RlContractError("Arahbo original pending position missing".into())
                })?;
            pending.source = Some(row.observer_at_entry.clone());
        }
    }
    for row in &garruk_occurrences {
        if let Some(position) = row.pending_order_index {
            let pending = base
                .projection
                .surface
                .engine_context
                .pending_triggers
                .get_mut(position as usize)
                .ok_or_else(|| RlContractError("Arahbo/Garruk pending position missing".into()))?;
            pending.source = Some(row.observer_at_entry.clone());
        }
    }
    let mut observation = ArahboObservationV1 {
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
        linked_exile,
        garruk_occurrences,
        garruk_active_sources,
        garruk_roots: crate::garruk_public_v1::roots()?,
        projection_sha256: String::new(),
    };
    observation.projection_sha256 = commitment(&observation)?;
    Ok(observation)
}
