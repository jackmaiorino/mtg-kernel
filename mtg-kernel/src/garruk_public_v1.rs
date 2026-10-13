//! Separate event-time Garruk transport. This is not V8/V9 or model admission.
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

pub const SCHEMA: &str = "mtg-kernel/garruk-public/v1";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntryOccurrenceV1 {
    pub entrant_at_entry: CardStableRefV1,
    pub entrant_face_index: u8,
    pub root: crate::garruks_uprising_v1::RootV1,
    pub event_time_power: Option<i32>,
    pub card_was_drawn: Option<bool>,
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
    DrawAttempted,
    ConditionFalse,
    Countered,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActiveSourceV1 {
    pub source: CardStableRefV1,
    pub controller: PlayerSeatV1,
    pub controlled_creatures_have_trample: Vec<CardStableRefV1>,
    pub ferocious_now: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RootV1 {
    pub card_id: u16,
    pub ability_index: Option<u16>,
    pub root: crate::rules_meaning_garruk_v1::RootV1,
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
pub struct GarrukObservationV1 {
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
        include_str!("garruk_public_v1.rs"),
        include_str!("garruks_uprising_v1.rs"),
        include_str!("banishing_public_v1.rs"),
        include_str!("banishing_light_v1.rs"),
        include_str!("linked_aura_operation_v1.rs"),
        include_str!("linked_aura_return_v1.rs"),
        include_str!("rules_meaning_banishing_v1.rs"),
        include_str!("rules_meaning_garruk_v1.rs"),
        include_str!("sower_restriction_capture_v1.rs"),
        include_str!("rl.rs"),
        include_str!("rl_session/garruk_v30.rs"),
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
fn commitment(observation: &GarrukObservationV1) -> Result<String, RlContractError> {
    let mut value = serde_json::to_value(observation)?;
    value
        .as_object_mut()
        .ok_or_else(|| RlContractError("Garruk projection is not an object".into()))?
        .remove("projection_sha256");
    Ok(digest(&serde_json::to_vec(&value)?))
}
impl GarrukObservationV1 {
    /// Checks transport identity only. Execution always rederives the entire
    /// observation and private ordered candidate binding from the owning session.
    pub fn validate_artifact(&self) -> Result<(), RlContractError> {
        if self.schema != SCHEMA
            || self.card_db_hash != crate::card_def::KERNEL_CARDDB_HASH
            || self.producer_sha256 != producer_sha256_v1()
            || self.projection_sha256 != commitment(self)?
        {
            return Err(RlContractError("wrong Garruk artifact identity".into()));
        }
        let expected = roots()?;
        if self.roots != expected || self.linked_exile.roots != crate::banishing_public_v1::roots()?
        {
            return Err(RlContractError("wrong Garruk printed root".into()));
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
    let Some(id) = crate::card_def::card_id_by_name("Garruk's Uprising") else {
        return Ok(Vec::new());
    };
    let selector = crate::rules_meaning_garruk_v1::live_selector_v1()
        .map_err(|_| RlContractError("Garruk catalog not supported".into()))?;
    [
        crate::rules_meaning_garruk_v1::RootV1::OwnEtbInterveningFerociousDraw,
        crate::rules_meaning_garruk_v1::RootV1::ControlledCreaturesHaveTrample,
        crate::rules_meaning_garruk_v1::RootV1::ControlledCreaturePowerEntryDraw,
    ]
    .into_iter()
    .map(|root| {
        let meaning = crate::rules_meaning_garruk_v1::meaning_v1(&selector, id, root)
            .map_err(|_| RlContractError("Garruk root not supported".into()))?;
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
    crate::garruks_uprising_v1::validate_state(state).map_err(RlContractError)?;
    let mut entry_occurrences = Vec::new();
    let mut event_order = 0;
    let mut previous_entry = None;
    for event in &state.engine.event_history {
        let crate::event::CommittedEvent::GarrukDrawCaptureV1 { capture } = event else {
            continue;
        };
        crate::garruks_uprising_v1::capture_at(state, capture.history_index)
            .map_err(RlContractError)?;
        let Some(crate::event::CommittedEvent::GarrukEntryObservationV1 {
            entry,
            effective_power,
            ..
        }) = state
            .engine
            .event_history
            .get(capture.observation_history_index as usize)
        else {
            return Err(RlContractError(
                "Garruk original entry facts missing".into(),
            ));
        };
        if previous_entry.is_some_and(|old| old != entry.entry_history_index) {
            event_order += 1;
        }
        previous_entry = Some(entry.entry_history_index);
        let index = capture.history_index;
        let owns = |pending: &crate::trigger::PendingTrigger| {
            matches!(pending.effect,
            crate::effect::EffectOp::GarrukDrawFromCaptureV1 { capture_history_index } if capture_history_index == index)
        };
        let stack_index = state.stack.iter().position(|item| matches!(item.inline_effect,
            Some(crate::effect::EffectOp::GarrukDrawFromCaptureV1 { capture_history_index }) if capture_history_index == index))
            .map(u32::try_from).transpose().map_err(|_| RlContractError("Garruk stack ordinal overflow".into()))?;
        let pending_order_index = state
            .engine
            .pending_triggers
            .iter()
            .position(owns)
            .map(u32::try_from)
            .transpose()
            .map_err(|_| RlContractError("Garruk pending ordinal overflow".into()))?;
        let completed = state
            .engine
            .event_history
            .iter()
            .find_map(|event| match event {
                crate::event::CommittedEvent::GarrukDrawCompletedV1 {
                    capture_history_index,
                    removal_history_index,
                    draw_history_index,
                } if *capture_history_index == index => {
                    Some((*removal_history_index, *draw_history_index))
                }
                _ => None,
            });
        let card_was_drawn = completed
            .and_then(|(_, draw)| draw)
            .map(
                |index| match state.engine.event_history.get(index as usize) {
                    Some(crate::event::CommittedEvent::Draw { object, .. }) => Ok(object.is_some()),
                    _ => Err(RlContractError(
                        "Garruk draw attempt occurrence missing".into(),
                    )),
                },
            )
            .transpose()?;
        let lifecycle = if let Some((removal, draw)) = completed {
            let Some(crate::event::CommittedEvent::GarrukDrawRemovedV1 { kind, .. }) =
                state.engine.event_history.get(removal as usize)
            else {
                return Err(RlContractError(
                    "Garruk actual removal facts missing".into(),
                ));
            };
            match kind {
                crate::garruks_uprising_v1::RemovalKindV1::Countered => EntryLifecycleV1::Countered,
                crate::garruks_uprising_v1::RemovalKindV1::Resolving { .. } if draw.is_some() => {
                    EntryLifecycleV1::DrawAttempted
                }
                crate::garruks_uprising_v1::RemovalKindV1::Resolving { .. } => {
                    EntryLifecycleV1::ConditionFalse
                }
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
            event_time_power: *effective_power,
            card_was_drawn,
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
        .filter(|(source, _)| crate::garruks_uprising_v1::active(state, *source))
        .map(|(source, live)| {
            let recipients = state.players[live.controller.index()]
                .battlefield
                .iter()
                .copied()
                .filter(|id| {
                    crate::engine::object_has_type(state, *id, crate::card_def::CardType::Creature)
                        && crate::continuous_characteristics_v1::grant_survives(
                            state,
                            *id,
                            live.v4.layer_timestamp.unwrap_or(0),
                        )
                })
                .map(|id| {
                    crate::sower_restriction_capture_v1::masked_ref(id, state.objects.get(id))
                })
                .collect::<Result<Vec<_>, RlContractError>>()?;
            Ok(ActiveSourceV1 {
                source: crate::sower_restriction_capture_v1::masked_ref(source, live)?,
                controller: live.controller.into(),
                controlled_creatures_have_trample: recipients,
                ferocious_now: crate::garruks_uprising_v1::ferocious(state, live.controller),
            })
        })
        .collect::<Result<Vec<_>, RlContractError>>()?;
    Ok((entry_occurrences, active_sources))
}

/// This successor retains already completed public owner choices. The generic
/// Banishing producer intentionally exposes only answers to its active choice.
/// Historical Aura identities come from the original public exile capture;
/// the chosen host is shown only for its still-public exact BF incarnation.
fn confirmed_linked_attachments(
    state: &GameState,
    pending: Vec<crate::banishing_public_v1::ConfirmedAttachmentV1>,
) -> Result<Vec<crate::banishing_public_v1::ConfirmedAttachmentV1>, RlContractError> {
    crate::banishing_light_v1::validate_state(state).map_err(RlContractError)?;
    let mut confirmed = Vec::new();
    for event in &state.engine.event_history {
        let crate::event::CommittedEvent::BanishingAuraHostChosenV1 {
            capture_history_index,
            host,
            ..
        } = event
        else {
            continue;
        };
        let at = usize::try_from(*capture_history_index)
            .map_err(|_| RlContractError("Aura capture index exceeds usize".into()))?;
        let Some(crate::event::CommittedEvent::BanishingAuraHostsCapturedV1 { choice }) =
            state.engine.event_history.get(at)
        else {
            return Err(RlContractError(
                "completed Aura answer lost original capture".into(),
            ));
        };
        let current_chosen_host = state
            .objects
            .try_get(host.object)
            .filter(|live| {
                live.zone == Zone::Battlefield
                    && live.zone_change_count == host.expected_zone_change_count
            })
            .map(|live| crate::sower_restriction_capture_v1::masked_ref(host.object, live))
            .transpose()?;
        confirmed.push(crate::banishing_public_v1::ConfirmedAttachmentV1 {
            aura: CardStableRefV1 {
                arena_id: choice.aura.object.0,
                card_db_id: choice.link.exiled_card_def,
                owner: choice.owner.into(),
                controller: choice.owner.into(),
                zone: choice.aura.expected_zone,
                zone_change_count: choice.aura.expected_zone_change_count,
            },
            owner: choice.owner.into(),
            current_chosen_host,
        });
    }
    for row in pending {
        if let Some(original) = confirmed.iter().find(|original| original.aura == row.aura) {
            if original != &row {
                return Err(RlContractError(
                    "pending Aura answer disagrees with committed confirmation".into(),
                ));
            }
        } else {
            confirmed.push(row);
        }
    }
    Ok(confirmed)
}

pub fn observe_v1(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor: PlayerId,
    step_index: u64,
    physical_decision_id: u64,
    substep_index: u32,
    substep_count: u32,
) -> Result<GarrukObservationV1, RlContractError> {
    crate::garruks_uprising_v1::validate_state(state).map_err(RlContractError)?;
    let (triggers, links, attachment_choice, confirmed_attachments) =
        crate::banishing_public_v1::facts(state)?;
    let linked_exile = LinkedExileFactsV1 {
        triggers,
        links,
        attachment_choice,
        confirmed_attachments: confirmed_linked_attachments(state, confirmed_attachments)?,
        roots: crate::banishing_public_v1::roots()?,
    };
    let (entry_occurrences, active_sources) = facts(state)?;
    let base = crate::rl::garruk_observation_storage_v1(
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
                    RlContractError("Garruk original pending position missing".into())
                })?;
            pending.source = Some(row.observer_at_entry.clone());
        }
    }
    let mut observation = GarrukObservationV1 {
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
        projection_sha256: String::new(),
    };
    observation.projection_sha256 = commitment(&observation)?;
    Ok(observation)
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod attachment_history_tests {
    use super::*;
    use crate::engine::{self, Action};
    use crate::event::{self, CommittedEvent, ProposedEvent};
    use crate::state::Target;

    #[test]
    fn genuine_sequential_returns_merge_committed_and_retained_answer_once_on_restore() {
        use crate::effect::{EffectOp, ExecCtx, ObjectRef};
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (pending, _, _, own_aura, other_aura, host) =
                crate::garruks_uprising_v1::attachment_tests::pending_returns(actor, false, false);
            // Reuse the genuine pre-operation state, then execute a distinct
            // accepted physical Sequence, not an invented printed spell.
            let mut state = *pending
                .engine
                .linked_aura_operation_v1
                .pending
                .as_ref()
                .unwrap()
                .original
                .clone();
            let sources = [own_aura, other_aura].map(|aura| {
                state
                    .engine
                    .linked_exile_records
                    .iter()
                    .find(|link| link.exiled == aura)
                    .unwrap()
                    .source
                    .source
            });
            let controller = state.objects.get(sources[0]).controller;
            let mut ctx = ExecCtx::no_targets(sources[0], controller);
            ctx.targets = vec![Target::Object(sources[1])];
            ctx.target_contracts = ctx
                .targets
                .iter()
                .map(|target| crate::state::StackTargetContractV4::capture(&state, *target))
                .collect();
            let sequence = EffectOp::Sequence(vec![
                EffectOp::MoveObject {
                    object: ObjectRef::ThisSource,
                    to_zone: Zone::Hand,
                },
                EffectOp::MoveObject {
                    object: ObjectRef::Target(0),
                    to_zone: Zone::Hand,
                },
            ]);
            crate::effect::execute(&sequence, &ctx, &mut state);
            assert!(crate::linked_aura_operation_v1::has_pending(&state));
            engine::step(&mut state, Action::ChooseEffectTarget(Target::Object(host))).unwrap();
            assert_eq!(state.objects.get(own_aura).zone, Zone::Battlefield);
            assert_eq!(state.objects.get(other_aura).zone, Zone::Exile);
            assert_eq!(
                state
                    .engine
                    .linked_aura_operation_v1
                    .pending
                    .as_ref()
                    .unwrap()
                    .choice
                    .owner,
                actor.opponent()
            );
            assert_eq!(
                state
                    .engine
                    .event_history
                    .iter()
                    .filter(|event| matches!(
                        event,
                        CommittedEvent::BanishingAuraHostChosenV1 { .. }
                    ))
                    .count(),
                1
            );
            let mut restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            for branch in [&mut state, &mut restored] {
                let (triggers, links, choice, previous) =
                    crate::banishing_public_v1::facts(branch).unwrap();
                assert_eq!(previous.len(), 1);
                let unique = confirmed_linked_attachments(branch, previous).unwrap();
                assert_eq!(
                    unique.len(),
                    1,
                    "one real completed answer remains one confirmation"
                );
                assert_eq!(unique[0].aura.arena_id, own_aura.0);
                let public = observe_v1(
                    branch,
                    &PolicySurfaceV5::new_for_session(),
                    actor.opponent(),
                    0,
                    0,
                    0,
                    1,
                )
                .unwrap();
                assert_eq!(public.linked_exile.confirmed_attachments, unique);
                assert_eq!(public.linked_exile.triggers, triggers);
                assert_eq!(public.linked_exile.links, links);
                assert_eq!(public.linked_exile.attachment_choice, choice);
                engine::step(branch, Action::ChooseEffectTarget(Target::Object(host))).unwrap();
                assert_eq!(branch.objects.get(other_aura).zone, Zone::Battlefield);
                assert_eq!(
                    confirmed_linked_attachments(branch, Vec::new())
                        .unwrap()
                        .len(),
                    2
                );
            }
            assert_eq!(state, restored);
        }
    }

    #[test]
    fn completed_apnap_attachments_preserve_original_aura_and_mask_departed_host_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (mut state, _, _, own_aura, other_aura, host) =
                crate::garruks_uprising_v1::attachment_tests::pending_returns(actor, false, true);
            let captures = [own_aura, other_aura].map(|id| {
                state
                    .engine
                    .event_history
                    .iter()
                    .find_map(|event| match event {
                        CommittedEvent::BanishingAuraHostsCapturedV1 { choice }
                            if choice.aura.object == id =>
                        {
                            Some(choice.clone())
                        }
                        _ => None,
                    })
                    .unwrap()
            });
            engine::step(&mut state, Action::ChooseEffectTarget(Target::Object(host))).unwrap();
            let facts = confirmed_linked_attachments(&state, Vec::new()).unwrap();
            assert_eq!(facts.len(), 2);
            for (row, capture) in facts.iter().zip(&captures) {
                assert_eq!(row.aura.arena_id, capture.aura.object.0);
                assert_eq!(row.aura.card_db_id, capture.link.exiled_card_def);
                assert_eq!(
                    row.aura.zone_change_count,
                    capture.aura.expected_zone_change_count
                );
                assert_eq!(row.aura.zone, Zone::Exile);
                assert_eq!(row.owner, capture.owner.into());
                assert_eq!(row.current_chosen_host.as_ref().unwrap().arena_id, host.0);
            }
            let restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            assert_eq!(
                confirmed_linked_attachments(&restored, Vec::new()).unwrap(),
                facts
            );
            let mut erased = restored;
            let answer = erased
                .engine
                .event_history
                .iter()
                .position(|event| matches!(event, CommittedEvent::BanishingAuraHostChosenV1 { .. }))
                .unwrap();
            erased.engine.event_history[answer] = CommittedEvent::LifeLoss {
                player: actor,
                amount: 0,
            };
            let before = serde_json::to_vec(&erased).unwrap();
            assert!(confirmed_linked_attachments(&erased, Vec::new()).is_err());
            assert_eq!(serde_json::to_vec(&erased).unwrap(), before);
            event::propose_and_commit(&mut state, ProposedEvent::zone_change(host, Zone::Hand));
            let departed = confirmed_linked_attachments(&state, Vec::new()).unwrap();
            assert_eq!(departed.len(), 2);
            assert!(departed.iter().all(|row| row.current_chosen_host.is_none()));
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(host, Zone::Battlefield),
            );
            let returned = confirmed_linked_attachments(&state, Vec::new()).unwrap();
            assert!(returned.iter().all(|row| row.current_chosen_host.is_none()));
            for (original, later) in facts.iter().zip(&returned) {
                assert_eq!(original.aura, later.aura);
                assert_eq!(original.owner, later.owner);
            }
        }
    }
}
