//! Definition-owned static and captured ETB roots for Garruk's Uprising.
//! This unregistered source is not capability or consumer admission.
use crate::card_def::{CardType, Keywords, TargetSpec, CARD_DEFS};
use crate::effect::{EffectOp, ExecCtx};
use crate::event::{CommittedEvent, ProposedEvent};
use crate::ids::ObjectId;
use crate::state::{AbilitySourceContractV4, GameObject, GameState, StackItem, Zone};
use crate::trigger::PendingTrigger;
use serde::{Deserialize, Serialize};

pub(crate) fn selected_definition(id: u16) -> bool {
    cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) && CARD_DEFS
        .get(usize::from(id))
        .is_some_and(|def| def.name == "Garruk's Uprising" && def.is_executable())
}

pub(crate) fn active(state: &GameState, source: ObjectId) -> bool {
    state.objects.try_get(source).is_some_and(|live| {
        live.zone == Zone::Battlefield
            && live.v4.face_index == 0
            && selected_definition(live.card_def)
            && crate::continuous_characteristics_v1::printed_abilities_active(state, source)
    })
}

/// A continuously granted keyword uses the source's current controller and
/// ordinary layer timestamp. Losing abilities after that timestamp removes
/// the grant; a later source's grant can still survive the same removal.
pub(crate) fn grants_trample(state: &GameState, creature: ObjectId) -> bool {
    if !enabled() {
        return false;
    }
    let Some(live) = state.objects.try_get(creature) else {
        return false;
    };
    live.zone == Zone::Battlefield
        && crate::engine::object_has_type(state, creature, CardType::Creature)
        && state.objects.iter().any(|(source, observer)| {
            observer.controller == live.controller
                && active(state, source)
                && crate::continuous_characteristics_v1::grant_survives(
                    state,
                    creature,
                    observer.v4.layer_timestamp.unwrap_or(0),
                )
        })
}

pub(crate) fn grants_keyword(state: &GameState, creature: ObjectId, keyword: Keywords) -> bool {
    keyword == Keywords::TRAMPLE && grants_trample(state, creature)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RootV1 {
    OwnEtbFerocious,
    ControlledCreaturePowerEntry,
}
impl RootV1 {
    pub(crate) fn template(self) -> EffectOp {
        match self {
            Self::OwnEtbFerocious => EffectOp::BindGarrukEtbDrawV1,
            Self::ControlledCreaturePowerEntry => EffectOp::BindGarrukPowerEntryDrawV1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EntryV1 {
    pub entry_history_index: u64,
    pub object: ObjectId,
    pub snapshot: GameObject,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CaptureV1 {
    pub history_index: u64,
    pub observation_history_index: u64,
    pub source: AbilitySourceContractV4,
    pub root: RootV1,
}

/// Genuine batch ownership retains entry-time controller/incarnation before
/// later collection. Effective power and observers are sampled only after
/// the complete simultaneous entry board, counters and attachments exist.
pub(crate) struct EntryBatchV1 {
    entries: Vec<EntryV1>,
}
impl EntryBatchV1 {
    pub(crate) fn capture() -> Option<Self> {
        enabled().then(|| Self {
            entries: Vec::new(),
        })
    }
    pub(crate) fn record(&mut self, state: &GameState, index: usize) {
        if let Some(object) = state
            .engine
            .event_history
            .get(index)
            .and_then(crate::trigger::battlefield_entry_object)
        {
            self.entries.push(EntryV1 {
                entry_history_index: index as u64,
                object,
                snapshot: state.objects.get(object).clone(),
            });
        }
    }
    pub(crate) fn finish(self, state: &mut GameState) {
        for mut entry in self.entries {
            let live = state.objects.get(entry.object);
            if live.zone == Zone::Battlefield
                && live.zone_change_count == entry.snapshot.zone_change_count
            {
                let original_controller = entry.snapshot.controller;
                entry.snapshot = live.clone();
                entry.snapshot.controller = original_controller;
            }
            record_entry(state, entry);
        }
    }
}
fn enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        CARD_DEFS.iter().enumerate().any(|(id, _)| {
            selected_definition(id as u16) && crate::trigger::triggers_for(id as u16).len() == 2
        })
    })
}

/// Exact supported as-entry forms, constructed from a genuine finalized
/// permanent spell before removal. No serialized continuation constructs this loan.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct EntryCharacteristicsV1 {
    pub face_down: Option<crate::state::FaceDownV1>,
    pub x_counters: Option<i32>,
    pub attached_to: Option<crate::state::ObjectLinkV4>,
    pub bestowed: bool,
}
pub(crate) struct PermanentEntryOwnerV1 {
    item: StackItem,
    original: GameObject,
    characteristics: EntryCharacteristicsV1,
}
pub(crate) fn acquire_permanent_entry(
    state: &GameState,
    item: &StackItem,
    targets_legal: bool,
) -> Result<Option<PermanentEntryOwnerV1>, String> {
    if !(enabled() || crate::arahbo_v1::enabled())
        || item.kind != crate::state::StackItemKind::Spell
        || item.inline_effect.is_some()
        || item.v4.cast_method == Some(crate::state::CastMethodV4::Omen)
    {
        return Ok(None);
    }
    let live = state.objects.get(item.source);
    let def = &CARD_DEFS[usize::from(live.card_def)];
    let bestowed = item.v4.cast_method == Some(crate::state::CastMethodV4::Bestow);
    let disguised = item.v4.cast_method == Some(crate::state::CastMethodV4::Alternative)
        && def.alt_cost.is_some_and(|alt| {
            alt.condition == crate::card_def::AltCostCondition::DisguiseFromHand
        });
    let program = if bestowed {
        if def.bestow.is_none() {
            return Err("Garruk entry lost actual Bestow form".into());
        }
        if targets_legal {
            Some(
                EffectOp::PutSourceOntoBattlefieldAttachedToTargetWithXPlusOneCounters {
                    target: crate::effect::ObjectRef::Target(0),
                },
            )
        } else {
            Some(EffectOp::PutSourceOntoBattlefieldWithXPlusOneCounters)
        }
    } else {
        def.printed_mode_effect(item.mode_chosen)
    };
    let (x_counters, target) = match program {
        Some(EffectOp::MoveObject {
            object: crate::effect::ObjectRef::ThisSource,
            to_zone: Zone::Battlefield,
        }) if disguised => (None, None),
        Some(EffectOp::PutSourceOntoBattlefieldWithXPlusOneCounters) => {
            (Some(i32::from(item.v4.x_value)), None)
        }
        Some(EffectOp::PutSourceOntoBattlefieldAttachedToTargetWithXPlusOneCounters { target })
            if targets_legal =>
        {
            (Some(i32::from(item.v4.x_value)), Some(target))
        }
        Some(EffectOp::PutSourceOntoBattlefieldAttachedToTarget { target }) if targets_legal => {
            (None, Some(target))
        }
        _ => return Ok(None),
    };
    if live.zone != Zone::Stack
        || state
            .stack
            .iter()
            .filter(|row| row.v4.stack_item_id == item.v4.stack_item_id)
            .collect::<Vec<_>>()
            != [item]
    {
        return Err("Garruk entry lost the unique genuine permanent spell".into());
    }
    crate::engine::validated_stack_item_target_spec(item, state)?;
    let attached_to = if let Some(crate::effect::ObjectRef::Target(index)) = target {
        let at = usize::from(index);
        let Some(crate::state::Target::Object(object)) = item.targets.get(at) else {
            return Err("Garruk entry attachment lost its actual target".into());
        };
        let contract = *item
            .v4
            .target_contracts
            .get(at)
            .ok_or("Garruk entry attachment lost its contract")?;
        if !crate::engine::target_contract_matches_live(state, item.targets[at], contract) {
            return Err("Garruk entry attachment target incarnation changed".into());
        }
        Some(crate::state::ObjectLinkV4 {
            object: *object,
            zone_change_count: state.objects.get(*object).zone_change_count,
        })
    } else if target.is_some() {
        return Err("Garruk entry attachment is not a definition-owned spell target".into());
    } else {
        None
    };
    let face_down = disguised.then_some(crate::state::FaceDownV1 {
        disguised: true,
        lookers: 1 << item.controller.index(),
        hidden_by: None,
        hidden_by_source: None,
    });
    Ok(Some(PermanentEntryOwnerV1 {
        item: item.clone(),
        original: live.clone(),
        characteristics: EntryCharacteristicsV1 {
            face_down,
            x_counters,
            attached_to,
            bestowed: bestowed && targets_legal,
        },
    }))
}
impl PermanentEntryOwnerV1 {
    pub(crate) fn entering_characteristics(
        &self,
        state: &GameState,
    ) -> Result<(ObjectId, EntryCharacteristicsV1), String> {
        if state.objects.get(self.item.source) != &self.original
            || self.original.zone != Zone::Stack
            || state
                .stack
                .iter()
                .any(|row| row.v4.stack_item_id == self.item.v4.stack_item_id)
        {
            return Err(
                "Garruk entry loan was not genuinely removed or changed its physical source".into(),
            );
        }
        if self.characteristics.attached_to.is_some_and(|host| {
            state.objects.try_get(host.object).is_none_or(|live| {
                live.zone != Zone::Battlefield || live.zone_change_count != host.zone_change_count
            })
        }) {
            return Err("Garruk entering attachment changed its original host".into());
        }
        Ok((self.item.source, self.characteristics))
    }
}
pub(crate) fn record_single_entry(state: &mut GameState, index: usize) {
    let Some(mut batch) = EntryBatchV1::capture() else {
        return;
    };
    batch.record(state, index);
    batch.finish(state);
}
pub(crate) fn ferocious(state: &GameState, controller: crate::ids::PlayerId) -> bool {
    state.players[controller.index()]
        .battlefield
        .iter()
        .copied()
        .any(|object| {
            crate::engine::object_has_type(state, object, CardType::Creature)
                && crate::engine::effective_power(state, object) >= 4
        })
}
fn record_entry(state: &mut GameState, entry: EntryV1) {
    let live = state.objects.get(entry.object);
    if live.zone != Zone::Battlefield || live.zone_change_count != entry.snapshot.zone_change_count
    {
        return;
    }
    let creature = crate::engine::object_has_type(state, entry.object, CardType::Creature);
    let power = creature.then(|| crate::engine::effective_power(state, entry.object));
    let mut sources = Vec::new();
    if active(state, entry.object) && ferocious(state, entry.snapshot.controller) {
        let mut source = AbilitySourceContractV4::capture(state, entry.object);
        source.controller = entry.snapshot.controller;
        sources.push((source, RootV1::OwnEtbFerocious));
    }
    if power.is_some_and(|power| power >= 4) {
        sources.extend(
            state
                .objects
                .iter()
                .filter(|(source, live)| {
                    live.controller == entry.snapshot.controller && active(state, *source)
                })
                .map(|(source, _)| {
                    (
                        AbilitySourceContractV4::capture(state, source),
                        RootV1::ControlledCreaturePowerEntry,
                    )
                }),
        );
    }
    let observation = state.engine.event_history.len() as u64;
    state
        .engine
        .event_history
        .push(CommittedEvent::GarrukEntryObservationV1 {
            entry: Box::new(entry),
            effective_creature: creature,
            effective_power: power,
            sources: sources.clone(),
        });
    for (source, root) in sources {
        let capture = CaptureV1 {
            history_index: state.engine.event_history.len() as u64,
            observation_history_index: observation,
            source,
            root,
        };
        let marker = CommittedEvent::GarrukDrawCaptureV1 { capture };
        state.engine.event_history.push(marker.clone());
        state.engine.event_log.push(marker);
    }
}
pub(crate) fn capture_at(state: &GameState, index: u64) -> Result<&CaptureV1, String> {
    let at = usize::try_from(index).map_err(|_| "Garruk capture index overflow")?;
    let Some(CommittedEvent::GarrukDrawCaptureV1 { capture }) = state.engine.event_history.get(at)
    else {
        return Err("Garruk draw lost its original capture".into());
    };
    let observation = usize::try_from(capture.observation_history_index)
        .map_err(|_| "Garruk observation index overflow")?;
    let Some(CommittedEvent::GarrukEntryObservationV1 {
        entry,
        effective_creature,
        effective_power,
        sources,
    }) = state.engine.event_history.get(observation)
    else {
        return Err("Garruk draw lost original event-time facts".into());
    };
    let event =
        usize::try_from(entry.entry_history_index).map_err(|_| "Garruk entry index overflow")?;
    if capture.history_index != index
        || event >= observation
        || observation >= at
        || crate::trigger::battlefield_entry_object(
            state
                .engine
                .event_history
                .get(event)
                .ok_or("Garruk entry disappeared")?,
        ) != Some(entry.object)
        || entry.snapshot.zone != Zone::Battlefield
        || !sources.contains(&(capture.source, capture.root))
        || sources
            .iter()
            .filter(|source| **source == (capture.source, capture.root))
            .count()
            != 1
        || !selected_definition(capture.source.card_def)
        || capture.source.zone != Zone::Battlefield
        || capture.source.attached_to.is_some()
        || (capture.root == RootV1::OwnEtbFerocious
            && (capture.source.source != entry.object
                || capture.source.zone_change_count != entry.snapshot.zone_change_count
                || capture.source.controller != entry.snapshot.controller))
        || (capture.root == RootV1::ControlledCreaturePowerEntry
            && (!*effective_creature
                || effective_power.is_none_or(|power| power < 4)
                || capture.source.controller != entry.snapshot.controller))
    {
        return Err("Garruk draw changed its original entrant/source/power/controller".into());
    }
    crate::engine::validate_historical_ability_source_contract(state, capture.source)?;
    let abilities = crate::trigger::triggers_for(capture.source.card_def);
    let position = match capture.root {
        RootV1::OwnEtbFerocious => 0,
        RootV1::ControlledCreaturePowerEntry => 1,
    };
    if abilities
        .get(position)
        .is_none_or(|ability| (ability.effect)() != capture.root.template())
    {
        return Err("Garruk capture changed the actual printed root".into());
    }
    Ok(capture)
}
fn capture_index(effect: &EffectOp) -> Option<u64> {
    match effect {
        EffectOp::GarrukDrawFromCaptureV1 {
            capture_history_index,
        } => Some(*capture_history_index),
        _ => None,
    }
}
pub(crate) fn validate_source(
    state: &GameState,
    effect: &EffectOp,
    source: Option<AbilitySourceContractV4>,
) -> Result<(), String> {
    if matches!(
        effect,
        EffectOp::BindGarrukEtbDrawV1 | EffectOp::BindGarrukPowerEntryDrawV1
    ) {
        return Err("Garruk printed template lost its event-time capture".into());
    }
    let Some(index) = capture_index(effect) else {
        return Ok(());
    };
    let capture = capture_at(state, index)?;
    if source!=Some(capture.source) || state.engine.event_history.iter().filter(|event|matches!(event,
        CommittedEvent::GarrukDrawCollectedV1 {capture_history_index} if *capture_history_index==index)).count()!=1 {
        return Err("Garruk source changed its capture or collection".into())
    }
    Ok(())
}
pub(crate) fn collect(
    state: &mut GameState,
    events: &[CommittedEvent],
) -> Result<Vec<PendingTrigger>, ObjectId> {
    let mut pending = Vec::new();
    let mut used = std::collections::BTreeSet::new();
    for event in events {
        let CommittedEvent::GarrukDrawCaptureV1 { capture } = event else {
            continue;
        };
        let index = capture.history_index;
        if capture_at(state,index).ok()!=Some(capture) || !used.insert(index)
            || state.engine.event_history.iter().any(|event|matches!(event,
                CommittedEvent::GarrukDrawCollectedV1 {capture_history_index} if *capture_history_index==index)) {
            return Err(capture.source.source)
        }
        pending.push(PendingTrigger {
            source: capture.source.source,
            controller: capture.source.controller,
            source_contract: Some(capture.source),
            granted_by: None,
            effect: EffectOp::GarrukDrawFromCaptureV1 {
                capture_history_index: index,
            },
            is_madness_offer: false,
            kicked: false,
            target_spec: TargetSpec::None,
            targets: Vec::new(),
            target_contracts: Vec::new(),
            placement_ordered: false,
            optional_additional_cost_paid: None,
            paid_cost_refs: Vec::new(),
        });
    }
    for event in &state.engine.event_history {
        if let CommittedEvent::GarrukDrawCaptureV1 { capture } = event {
            let collected=state.engine.event_history.iter().any(|event|matches!(event,
                CommittedEvent::GarrukDrawCollectedV1 {capture_history_index} if *capture_history_index==capture.history_index));
            if !collected && !used.contains(&capture.history_index) {
                return Err(capture.source.source);
            }
        }
    }
    for capture_history_index in used {
        state
            .engine
            .event_history
            .push(CommittedEvent::GarrukDrawCollectedV1 {
                capture_history_index,
            });
    }
    Ok(pending)
}

fn publications(state: &GameState, index: u64) -> Vec<(usize, &StackItem)> {
    state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(at, event)| match event {
            CommittedEvent::GarrukDrawPublishedV1 {
                capture_history_index,
                item,
            } if *capture_history_index == index => Some((at, item.as_ref())),
            _ => None,
        })
        .collect()
}
fn validate_item(state: &GameState, item: &StackItem, index: u64) -> Result<(), String> {
    let capture = capture_at(state, index)?;
    validate_source(
        state,
        &EffectOp::GarrukDrawFromCaptureV1 {
            capture_history_index: index,
        },
        item.v4.ability_source_contract,
    )?;
    if item.kind != crate::state::StackItemKind::TriggeredAbility
        || item.source != capture.source.source
        || item.controller != capture.source.controller
        || item.inline_effect
            != Some(EffectOp::GarrukDrawFromCaptureV1 {
                capture_history_index: index,
            })
        || item.v4.granted_by.is_some()
        || item.v4.source_contract.is_some()
        || item.v4.activated_ability_index.is_some()
        || item.v4.target_spec.is_some()
        || crate::engine::validated_stack_item_target_spec(item, state)? != TargetSpec::None
        || !item.targets.is_empty()
        || !item.v4.target_contracts.is_empty()
        || !item.v4.paid_cost_refs.is_empty()
        || item.v4.optional_additional_cost_paid.is_some()
        || item.is_copy
        || item.is_flashback
        || item.madness_offer
        || item.kicked
        || item.v4.stack_item_id == crate::ids::StackItemId::default()
    {
        return Err("Garruk finalized draw changed its original root/source/metadata".into());
    }
    Ok(())
}
pub(crate) fn publish(state: &mut GameState, item: &StackItem) -> Result<(), String> {
    let Some(index) = item.inline_effect.as_ref().and_then(capture_index) else {
        return Ok(());
    };
    validate_item(state, item, index)?;
    if !publications(state, index).is_empty()
        || state.stack.iter().filter(|live| *live == item).count() != 1
    {
        return Err("Garruk draw repeated or fabricated its actual publication".into());
    }
    state
        .engine
        .event_history
        .push(CommittedEvent::GarrukDrawPublishedV1 {
            capture_history_index: index,
            item: Box::new(item.clone()),
        });
    Ok(())
}

/// Private authority acquired from the unique actual live row before pop or
/// counter. It cannot be serialized, forged through a restore, or constructed
/// by a generic direct effect invocation.
pub(crate) struct RemovalOwnerV1 {
    item: StackItem,
    index: u64,
    ferocious_at_resolution: bool,
}
pub(crate) fn before_remove(
    state: &GameState,
    item: &StackItem,
) -> Result<Option<RemovalOwnerV1>, String> {
    let Some(index) = item.inline_effect.as_ref().and_then(capture_index) else {
        return Ok(None);
    };
    validate_state(state)?;
    validate_item(state, item, index)?;
    let rows = publications(state, index);
    if !matches!(rows.as_slice(),[(_,original)] if *original==item)
        || state
            .stack
            .iter()
            .filter(|live| live.v4.stack_item_id == item.v4.stack_item_id && *live == item)
            .count()
            != 1
    {
        return Err("Garruk removal lost its original published live owner".into());
    }
    Ok(Some(RemovalOwnerV1 {
        item: item.clone(),
        index,
        ferocious_at_resolution: ferocious(state, item.controller),
    }))
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RemovalKindV1 {
    Resolving { ferocious: bool },
    Countered,
}
fn record_removal(
    state: &mut GameState,
    owner: &RemovalOwnerV1,
    kind: RemovalKindV1,
) -> Result<usize, String> {
    if state.stack.iter().any(|item|item.v4.stack_item_id==owner.item.v4.stack_item_id)
        || state.engine.event_history.iter().any(|event|matches!(event,
            CommittedEvent::GarrukDrawRemovedV1 {capture_history_index,..} if *capture_history_index==owner.index)) {
        return Err("Garruk private removed owner still exists or was consumed".into())
    }
    let at = state.engine.event_history.len();
    state
        .engine
        .event_history
        .push(CommittedEvent::GarrukDrawRemovedV1 {
            capture_history_index: owner.index,
            item: Box::new(owner.item.clone()),
            kind,
        });
    Ok(at)
}
pub(crate) fn counter_removed(state: &mut GameState, owner: RemovalOwnerV1) -> Result<(), String> {
    let removal = record_removal(state, &owner, RemovalKindV1::Countered)?;
    state
        .engine
        .event_history
        .push(CommittedEvent::GarrukDrawCompletedV1 {
            capture_history_index: owner.index,
            removal_history_index: removal as u64,
            draw_history_index: None,
        });
    Ok(())
}
pub(crate) fn resolve_removed(
    state: &mut GameState,
    owner: RemovalOwnerV1,
    ctx: &ExecCtx,
) -> Result<(), String> {
    if Some(owner.item.v4.stack_item_id) != ctx.stack_item_id
        || owner.item.source != ctx.source
        || owner.item.controller != ctx.controller
        || owner.item.v4.ability_source_contract != ctx.ability_source_contract
        || !ctx.targets.is_empty()
        || !ctx.target_contracts.is_empty()
    {
        return Err("Garruk resolution changed the genuine removed row".into());
    }
    let capture = capture_at(state, owner.index)?;
    let should_draw =
        capture.root == RootV1::ControlledCreaturePowerEntry || owner.ferocious_at_resolution;
    let removal = record_removal(
        state,
        &owner,
        RemovalKindV1::Resolving {
            ferocious: owner.ferocious_at_resolution,
        },
    )?;
    let draw = if should_draw {
        let start = state.engine.event_history.len();
        crate::event::propose_and_commit(state, ProposedEvent::draw(ctx.controller));
        Some(
            state.engine.event_history[start..]
                .iter()
                .position(|event| {
                    matches!(event,
            CommittedEvent::Draw {player,..} if *player==ctx.controller)
                })
                .map(|offset| start + offset)
                .ok_or("Garruk draw attempt produced no actual Draw occurrence")?
                as u64,
        )
    } else {
        None
    };
    state
        .engine
        .event_history
        .push(CommittedEvent::GarrukDrawCompletedV1 {
            capture_history_index: owner.index,
            removal_history_index: removal as u64,
            draw_history_index: draw,
        });
    Ok(())
}

pub(crate) fn is_history(event: &CommittedEvent) -> bool {
    matches!(
        event,
        CommittedEvent::GarrukEntryObservationV1 { .. }
            | CommittedEvent::GarrukDrawCollectedV1 { .. }
            | CommittedEvent::GarrukDrawPublishedV1 { .. }
            | CommittedEvent::GarrukDrawRemovedV1 { .. }
            | CommittedEvent::GarrukDrawCompletedV1 { .. }
    )
}
pub(crate) fn validate_state(state: &GameState) -> Result<(), String> {
    if state.engine.event_log.iter().any(is_history) {
        return Err("Garruk private journal entered mutable collection log".into());
    }
    if enabled() {
        for (index, event) in state.engine.event_history.iter().enumerate() {
            if crate::trigger::battlefield_entry_object(event).is_some()
                && state.engine.event_history.iter().filter(|event|matches!(event,
                    CommittedEvent::GarrukEntryObservationV1 {entry,..} if entry.entry_history_index==index as u64)).count()!=1 {
                return Err("Garruk catalog entry lost its independent original observation".into())
            }
        }
    }
    for (observation, event) in state.engine.event_history.iter().enumerate() {
        if let CommittedEvent::GarrukEntryObservationV1 { entry, sources, .. } = event {
            let original = usize::try_from(entry.entry_history_index)
                .map_err(|_| "Garruk entry index overflow")?;
            if original >= observation
                || state
                    .engine
                    .event_history
                    .get(original)
                    .and_then(crate::trigger::battlefield_entry_object)
                    != Some(entry.object)
            {
                return Err("Garruk observation lost its actual entry".into());
            }
            for (source, root) in sources {
                if state.engine.event_history.iter().filter(|event|matches!(event,
                    CommittedEvent::GarrukDrawCaptureV1 {capture} if capture.observation_history_index==observation as u64
                        && capture.source==*source && capture.root==*root)).count()!=1 {
                    return Err("Garruk original observer lost its mandatory draw capture".into())
                }
            }
        }
    }
    for (at, event) in state.engine.event_history.iter().enumerate() {
        let CommittedEvent::GarrukDrawCaptureV1 { capture } = event else {
            continue;
        };
        capture_at(state, capture.history_index)?;
        if capture.history_index != at as u64 {
            return Err("Garruk capture moved in its original journal".into());
        }
        let index = capture.history_index;
        let collections: Vec<_> = state.engine.event_history.iter().enumerate().filter_map(|(at,event)|
            matches!(event,CommittedEvent::GarrukDrawCollectedV1 {capture_history_index} if *capture_history_index==index).then_some(at)).collect();
        let collected = collections.len();
        if collections.iter().any(|collection| *collection <= at) {
            return Err("Garruk collection preceded its original capture".into());
        }
        let logged = state
            .engine
            .event_log
            .iter()
            .filter(|logged| *logged == event)
            .count();
        let pending: Vec<_> = state
            .engine
            .pending_triggers
            .iter()
            .chain(state.legend_pending_v1.iter().flatten())
            .chain(
                state
                    .pending_legend_rule_v1
                    .iter()
                    .flat_map(|pending| pending.waiting_triggers()),
            )
            .chain(
                state
                    .standard_v1
                    .iter()
                    .filter_map(|s| s.resolution_play.as_ref())
                    .flat_map(|p| p.deferred_triggers.iter()),
            )
            .filter(|pending| capture_index(&pending.effect) == Some(index))
            .collect();
        for row in &pending {
            if row.source != capture.source.source
                || row.controller != capture.source.controller
                || row.source_contract != Some(capture.source)
                || row.granted_by.is_some()
                || row.target_spec != TargetSpec::None
                || !row.targets.is_empty()
                || !row.target_contracts.is_empty()
                || row.is_madness_offer
                || row.kicked
                || row.optional_additional_cost_paid.is_some()
                || !row.paid_cost_refs.is_empty()
            {
                return Err("Garruk pending draw changed its original captured owner".into());
            }
        }
        let publications = publications(state, index);
        if collected == 0 {
            if logged != 1 || !pending.is_empty() || !publications.is_empty() {
                return Err("Garruk uncollected capture lost its sole log owner".into());
            }
            continue;
        }
        if collected != 1 || logged != 0 || publications.len() > 1 {
            return Err("Garruk capture was duplicated or recollected".into());
        }
        let Some((published_at, item)) = publications.first().copied() else {
            if pending.len() != 1 {
                return Err("Garruk collected draw lost its mandatory pending owner".into());
            }
            continue;
        };
        if collections[0] >= published_at {
            return Err("Garruk publication preceded collection".into());
        }
        validate_item(state, item, index)?;
        if !pending.is_empty() {
            return Err("Garruk published draw retained its pending owner".into());
        }
        let live: Vec<_> = state
            .stack
            .iter()
            .filter(|row| row.v4.stack_item_id == item.v4.stack_item_id)
            .collect();
        let completions: Vec<_> = state
            .engine
            .event_history
            .iter()
            .enumerate()
            .filter_map(|(at, event)| match event {
                CommittedEvent::GarrukDrawCompletedV1 {
                    capture_history_index,
                    removal_history_index,
                    draw_history_index,
                } if *capture_history_index == index => {
                    Some((at, *removal_history_index, *draw_history_index))
                }
                _ => None,
            })
            .collect();
        let removals: Vec<_> = state
            .engine
            .event_history
            .iter()
            .enumerate()
            .filter_map(|(at, event)| match event {
                CommittedEvent::GarrukDrawRemovedV1 {
                    capture_history_index,
                    item,
                    kind,
                } if *capture_history_index == index => Some((at, item.as_ref(), kind)),
                _ => None,
            })
            .collect();
        if completions.is_empty() {
            if live.as_slice() != [item] || !removals.is_empty() {
                return Err("Garruk live draw was dropped or changed".into());
            }
        } else {
            let [(completed, removal, draw)] = completions.as_slice() else {
                return Err("Garruk draw completed more than once".into());
            };
            let [(removed, original, kind)] = removals.as_slice() else {
                return Err("Garruk completion has no unique actual removal".into());
            };
            if !live.is_empty()
                || *original != item
                || *removal != *removed as u64
                || published_at >= *removed
                || *removed >= *completed
            {
                return Err("Garruk completion changed original removal/publication".into());
            }
            let should_draw = match kind {
                RemovalKindV1::Countered => false,
                RemovalKindV1::Resolving { ferocious } => {
                    capture.root == RootV1::ControlledCreaturePowerEntry || *ferocious
                }
            };
            match (should_draw, draw) {
                (true, Some(draw))
                    if *draw > *removal
                        && *draw < *completed as u64
                        && matches!(state.engine.event_history.get(*draw as usize),Some(CommittedEvent::Draw {player,..})
                        if *player==capture.source.controller) => {}
                (false, None) => {}
                _ => return Err("Garruk completion skipped or forged the actual draw".into()),
            }
        }
    }
    for event in &state.engine.event_log {
        if let CommittedEvent::GarrukDrawCaptureV1 { capture } = event {
            if capture_at(state, capture.history_index).ok() != Some(capture) {
                return Err("Garruk mutable log contains a forged capture".into());
            }
        }
    }
    Ok(())
}

pub(crate) fn requires_successor(state: &GameState) -> bool {
    state
        .objects
        .iter()
        .any(|(source, _)| active(state, source))
        || state
            .engine
            .event_history
            .iter()
            .any(|event| matches!(event, CommittedEvent::GarrukDrawCaptureV1 { .. }))
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
pub(crate) mod attachment_tests;
#[cfg(test)]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[path = "garruks_uprising_v1/future_tests.rs"]
mod future_tests;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn registered_predecessor_catalog_does_not_record_or_change_state_bytes() {
        if enabled() {
            return;
        }
        for actor in [crate::ids::PlayerId::P0, crate::ids::PlayerId::P1] {
            let mut state = crate::policy_observation_v6::tests::ready_state();
            state.active_player = actor;
            let creature = crate::policy_observation_v6::tests::put(
                &mut state,
                actor,
                "Llanowar Elves",
                Zone::Battlefield,
            );
            let before = serde_json::to_vec(&state).unwrap();
            let hash = state.diagnostic_state_hash();
            record_single_entry(&mut state, 0);
            assert!(!grants_trample(&state, creature));
            validate_state(&state).unwrap();
            assert_eq!(serde_json::to_vec(&state).unwrap(), before);
            assert_eq!(state.diagnostic_state_hash(), hash);
        }
    }
}
