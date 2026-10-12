//! Definition-owned static and captured ETB roots for Arahbo, the First Fang.
//! This unregistered source is not capability or consumer admission.
use crate::card_def::{TargetSpec, CARD_DEFS};
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
        .is_some_and(|def| def.name == "Arahbo, the First Fang" && def.is_executable())
}

pub(crate) fn cat_token_definition() -> Result<u16, String> {
    let id = crate::card_def::card_id_by_name("Cat Token")
        .ok_or("Arahbo requires the admitted Cat Token")?;
    let def = &CARD_DEFS[usize::from(id)];
    if !def.is_executable()
        || !def.is_token
        || def.is_land
        || def.object_name != def.name
        || def.types != [crate::card_def::CardType::Creature]
        || def.subtypes != [crate::card_def::Subtype::Cat]
        || !def.supertypes.is_empty()
        || def.power != Some(1)
        || def.toughness != Some(1)
        || def.colors != [crate::mana::ManaColor::W]
        || def.keywords != crate::card_def::Keywords::NONE
        || def.mana_value != 0
        || def.cost.generic != 0
        || def.cost.x_count != 0
        || !def.cost.pips.is_empty()
        || !def.activated_abilities.is_empty()
        || !def.produces_mana.is_empty()
        || !crate::trigger::triggers_for(id).is_empty()
        || def.changeling
        || def.transform_face.is_some()
        || def.animation.is_some()
    {
        return Err(
            "Arahbo Cat token blueprint differs from its actual printed characteristics".into(),
        );
    }
    Ok(id)
}
pub(crate) fn active(state: &GameState, source: ObjectId) -> bool {
    state.objects.try_get(source).is_some_and(|live| {
        live.zone == Zone::Battlefield
            && live.v4.face_index == 0
            && selected_definition(live.card_def)
            && crate::continuous_characteristics_v1::printed_abilities_active(state, source)
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RootV1 {
    OwnEntry,
    OtherNontokenCatEntry,
}
impl RootV1 {
    pub(crate) fn template(self) -> EffectOp {
        match self {
            Self::OwnEntry | Self::OtherNontokenCatEntry => EffectOp::BindArahboCatEntryV1,
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
/// later collection. Effective subtype and observers are sampled only after
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
            let mut snapshot = state.objects.get(object).clone();
            if matches!(
                state.engine.event_history.get(index),
                Some(CommittedEvent::CreateToken { .. })
            ) {
                snapshot.v4.is_token = true;
            }
            self.entries.push(EntryV1 {
                entry_history_index: index as u64,
                object,
                snapshot,
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
                let original_token = entry.snapshot.v4.is_token;
                entry.snapshot = live.clone();
                entry.snapshot.controller = original_controller;
                entry.snapshot.v4.is_token |= original_token;
            }
            record_entry(state, entry);
        }
    }
}
pub(crate) fn enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        CARD_DEFS.iter().enumerate().any(|(id, _)| {
            selected_definition(id as u16) && crate::trigger::triggers_for(id as u16).len() == 1
        })
    })
}

pub(crate) fn record_single_entry(state: &mut GameState, index: usize) {
    let Some(mut batch) = EntryBatchV1::capture() else {
        return;
    };
    batch.record(state, index);
    batch.finish(state);
}
fn record_entry(state: &mut GameState, entry: EntryV1) {
    let live = state.objects.get(entry.object);
    if live.zone != Zone::Battlefield || live.zone_change_count != entry.snapshot.zone_change_count
    {
        return;
    }
    let cat = crate::card_def::Subtype::Cat
        .is_in_subtype_ids(&crate::engine::effective_subtype_ids(state, entry.object));
    let token = entry.snapshot.v4.is_token;
    let sources = state
        .objects
        .iter()
        .filter(|(source, live)| {
            active(state, *source)
                && (*source == entry.object
                    || (!token && cat && live.controller == entry.snapshot.controller))
        })
        .map(|(source, _)| {
            let mut captured = AbilitySourceContractV4::capture(state, source);
            if source == entry.object {
                captured.controller = entry.snapshot.controller;
            }
            (
                captured,
                if source == entry.object {
                    RootV1::OwnEntry
                } else {
                    RootV1::OtherNontokenCatEntry
                },
            )
        })
        .collect::<Vec<_>>();
    let observation = state.engine.event_history.len() as u64;
    state
        .engine
        .event_history
        .push(CommittedEvent::ArahboEntryObservationV1 {
            entry: Box::new(entry),
            effective_cat: cat,
            was_token: token,
            sources: sources.clone(),
        });
    for (source, root) in sources {
        let capture = CaptureV1 {
            history_index: state.engine.event_history.len() as u64,
            observation_history_index: observation,
            source,
            root,
        };
        let marker = CommittedEvent::ArahboTokenCaptureV1 { capture };
        state.engine.event_history.push(marker.clone());
        state.engine.event_log.push(marker);
    }
}
pub(crate) fn capture_at(state: &GameState, index: u64) -> Result<&CaptureV1, String> {
    let at = usize::try_from(index).map_err(|_| "Arahbo capture index overflow")?;
    let Some(CommittedEvent::ArahboTokenCaptureV1 { capture }) = state.engine.event_history.get(at)
    else {
        return Err("Arahbo token lost its original capture".into());
    };
    let observation = usize::try_from(capture.observation_history_index)
        .map_err(|_| "Arahbo observation index overflow")?;
    let Some(CommittedEvent::ArahboEntryObservationV1 {
        entry,
        effective_cat,
        was_token,
        sources,
    }) = state.engine.event_history.get(observation)
    else {
        return Err("Arahbo token lost original event-time facts".into());
    };
    let event =
        usize::try_from(entry.entry_history_index).map_err(|_| "Arahbo entry index overflow")?;
    if capture.history_index != index
        || event >= observation
        || observation >= at
        || crate::trigger::battlefield_entry_object(
            state
                .engine
                .event_history
                .get(event)
                .ok_or("Arahbo entry disappeared")?,
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
        || (capture.root == RootV1::OwnEntry
            && (capture.source.source != entry.object
                || capture.source.zone_change_count != entry.snapshot.zone_change_count
                || capture.source.controller != entry.snapshot.controller))
        || (capture.root == RootV1::OtherNontokenCatEntry
            && (!*effective_cat
                || *was_token
                || capture.source.source == entry.object
                || capture.source.controller != entry.snapshot.controller))
    {
        return Err("Arahbo token changed its original entrant/source/power/controller".into());
    }
    crate::engine::validate_historical_ability_source_contract(state, capture.source)?;
    let abilities = crate::trigger::triggers_for(capture.source.card_def);
    let position = 0;
    if abilities
        .get(position)
        .is_none_or(|ability| (ability.effect)() != capture.root.template())
    {
        return Err("Arahbo capture changed the actual printed root".into());
    }
    Ok(capture)
}
fn capture_index(effect: &EffectOp) -> Option<u64> {
    match effect {
        EffectOp::ArahboTokenFromCaptureV1 {
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
    if matches!(effect, EffectOp::BindArahboCatEntryV1) {
        return Err("Arahbo printed template lost its event-time capture".into());
    }
    let Some(index) = capture_index(effect) else {
        return Ok(());
    };
    let capture = capture_at(state, index)?;
    if source!=Some(capture.source) || state.engine.event_history.iter().filter(|event|matches!(event,
        CommittedEvent::ArahboTokenCollectedV1 {capture_history_index} if *capture_history_index==index)).count()!=1 {
        return Err("Arahbo source changed its capture or collection".into())
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
        let CommittedEvent::ArahboTokenCaptureV1 { capture } = event else {
            continue;
        };
        let index = capture.history_index;
        if capture_at(state,index).ok()!=Some(capture) || !used.insert(index)
            || state.engine.event_history.iter().any(|event|matches!(event,
                CommittedEvent::ArahboTokenCollectedV1 {capture_history_index} if *capture_history_index==index)) {
            return Err(capture.source.source)
        }
        pending.push(PendingTrigger {
            source: capture.source.source,
            controller: capture.source.controller,
            source_contract: Some(capture.source),
            granted_by: None,
            effect: EffectOp::ArahboTokenFromCaptureV1 {
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
        if let CommittedEvent::ArahboTokenCaptureV1 { capture } = event {
            let collected=state.engine.event_history.iter().any(|event|matches!(event,
                CommittedEvent::ArahboTokenCollectedV1 {capture_history_index} if *capture_history_index==capture.history_index));
            if !collected && !used.contains(&capture.history_index) {
                return Err(capture.source.source);
            }
        }
    }
    for capture_history_index in used {
        state
            .engine
            .event_history
            .push(CommittedEvent::ArahboTokenCollectedV1 {
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
            CommittedEvent::ArahboTokenPublishedV1 {
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
        &EffectOp::ArahboTokenFromCaptureV1 {
            capture_history_index: index,
        },
        item.v4.ability_source_contract,
    )?;
    if item.kind != crate::state::StackItemKind::TriggeredAbility
        || item.source != capture.source.source
        || item.controller != capture.source.controller
        || item.inline_effect
            != Some(EffectOp::ArahboTokenFromCaptureV1 {
                capture_history_index: index,
            })
        || item.v4.granted_by.is_some()
        || item.v4.source_contract.is_some()
        || item.v4.activated_ability_index.is_some()
        || item.v4.target_spec != Some(TargetSpec::None)
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
        return Err("Arahbo finalized token changed its original root/source/metadata".into());
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
        return Err("Arahbo token repeated or fabricated its actual publication".into());
    }
    state
        .engine
        .event_history
        .push(CommittedEvent::ArahboTokenPublishedV1 {
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
        return Err("Arahbo removal lost its original published live owner".into());
    }
    Ok(Some(RemovalOwnerV1 {
        item: item.clone(),
        index,
    }))
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RemovalKindV1 {
    Resolving,
    Countered,
}
fn record_removal(
    state: &mut GameState,
    owner: &RemovalOwnerV1,
    kind: RemovalKindV1,
) -> Result<usize, String> {
    if state.stack.iter().any(|item|item.v4.stack_item_id==owner.item.v4.stack_item_id)
        || state.engine.event_history.iter().any(|event|matches!(event,
            CommittedEvent::ArahboTokenRemovedV1 {capture_history_index,..} if *capture_history_index==owner.index)) {
        return Err("Arahbo private removed owner still exists or was consumed".into())
    }
    let at = state.engine.event_history.len();
    state
        .engine
        .event_history
        .push(CommittedEvent::ArahboTokenRemovedV1 {
            capture_history_index: owner.index,
            item: Box::new(owner.item.clone()),
            kind,
        });
    Ok(at)
}
pub(crate) fn record_counter_input(
    state: &mut GameState,
    owner: &RemovalOwnerV1,
) -> Result<(), String> {
    if state
        .stack
        .iter()
        .any(|row| row.v4.stack_item_id == owner.item.v4.stack_item_id)
    {
        return Err("Arahbo counter input precedes removal".into());
    }
    state
        .engine
        .event_history
        .push(CommittedEvent::ArahboCounterInputV1 {
            capture_history_index: owner.index,
            item: Box::new(owner.item.clone()),
        });
    Ok(())
}
pub(crate) fn counter_removed(state: &mut GameState, owner: RemovalOwnerV1) -> Result<(), String> {
    let removal = record_removal(state, &owner, RemovalKindV1::Countered)?;
    state
        .engine
        .event_history
        .push(CommittedEvent::ArahboTokenCompletedV1 {
            capture_history_index: owner.index,
            removal_history_index: removal as u64,
            token_history_index: None,
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
        return Err("Arahbo resolution changed the genuine removed row".into());
    }
    let token_def = cat_token_definition()?;
    let removal = record_removal(state, &owner, RemovalKindV1::Resolving)?;
    let start = state.engine.event_history.len();
    crate::event::propose_and_commit(
        state,
        ProposedEvent::create_token(token_def, ctx.controller),
    );
    let token = Some(state.engine.event_history[start..].iter().position(|event|
        matches!(event, CommittedEvent::CreateToken { controller, token_def: created_def, .. } if *controller == ctx.controller && *created_def == token_def)
    ).map(|offset| start+offset).ok_or("Arahbo token produced no actual creation occurrence")? as u64);
    state
        .engine
        .event_history
        .push(CommittedEvent::ArahboTokenCompletedV1 {
            capture_history_index: owner.index,
            removal_history_index: removal as u64,
            token_history_index: token,
        });
    Ok(())
}

pub(crate) fn is_history(event: &CommittedEvent) -> bool {
    matches!(
        event,
        CommittedEvent::ArahboEntryObservationV1 { .. }
            | CommittedEvent::ArahboTokenCollectedV1 { .. }
            | CommittedEvent::ArahboTokenPublishedV1 { .. }
            | CommittedEvent::ArahboTokenRemovedV1 { .. }
            | CommittedEvent::ArahboTokenCompletedV1 { .. }
            | CommittedEvent::ArahboCounterInputV1 { .. }
    )
}
pub(crate) fn validate_state(state: &GameState) -> Result<(), String> {
    if state.engine.event_log.iter().any(is_history) {
        return Err("Arahbo private journal entered mutable collection log".into());
    }
    if enabled() {
        for (index, event) in state.engine.event_history.iter().enumerate() {
            if crate::trigger::battlefield_entry_object(event).is_some()
                && state.engine.event_history.iter().filter(|event|matches!(event,
                    CommittedEvent::ArahboEntryObservationV1 {entry,..} if entry.entry_history_index==index as u64)).count()!=1 {
                return Err("Arahbo catalog entry lost its independent original observation".into())
            }
        }
    }
    for (observation, event) in state.engine.event_history.iter().enumerate() {
        if let CommittedEvent::ArahboEntryObservationV1 { entry, sources, .. } = event {
            let original = usize::try_from(entry.entry_history_index)
                .map_err(|_| "Arahbo entry index overflow")?;
            if original >= observation
                || state
                    .engine
                    .event_history
                    .get(original)
                    .and_then(crate::trigger::battlefield_entry_object)
                    != Some(entry.object)
            {
                return Err("Arahbo observation lost its actual entry".into());
            }
            for (source, root) in sources {
                if state.engine.event_history.iter().filter(|event|matches!(event,
                    CommittedEvent::ArahboTokenCaptureV1 {capture} if capture.observation_history_index==observation as u64
                        && capture.source==*source && capture.root==*root)).count()!=1 {
                    return Err("Arahbo original observer lost its mandatory token capture".into())
                }
            }
        }
    }
    for (at, event) in state.engine.event_history.iter().enumerate() {
        let CommittedEvent::ArahboTokenCaptureV1 { capture } = event else {
            continue;
        };
        capture_at(state, capture.history_index)?;
        if capture.history_index != at as u64 {
            return Err("Arahbo capture moved in its original journal".into());
        }
        let index = capture.history_index;
        let collections: Vec<_> = state.engine.event_history.iter().enumerate().filter_map(|(at,event)|
            matches!(event,CommittedEvent::ArahboTokenCollectedV1 {capture_history_index} if *capture_history_index==index).then_some(at)).collect();
        let collected = collections.len();
        if collections.iter().any(|collection| *collection <= at) {
            return Err("Arahbo collection preceded its original capture".into());
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
                return Err("Arahbo pending token changed its original captured owner".into());
            }
        }
        let publications = publications(state, index);
        if collected == 0 {
            if logged != 1 || !pending.is_empty() || !publications.is_empty() {
                return Err("Arahbo uncollected capture lost its sole log owner".into());
            }
            continue;
        }
        if collected != 1 || logged != 0 || publications.len() > 1 {
            return Err("Arahbo capture was duplicated or recollected".into());
        }
        let Some((published_at, item)) = publications.first().copied() else {
            if pending.len() != 1 {
                return Err("Arahbo collected token lost its mandatory pending owner".into());
            }
            continue;
        };
        if collections[0] >= published_at {
            return Err("Arahbo publication preceded collection".into());
        }
        validate_item(state, item, index)?;
        if !pending.is_empty() {
            return Err("Arahbo published token retained its pending owner".into());
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
                CommittedEvent::ArahboTokenCompletedV1 {
                    capture_history_index,
                    removal_history_index,
                    token_history_index,
                } if *capture_history_index == index => {
                    Some((at, *removal_history_index, *token_history_index))
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
                CommittedEvent::ArahboTokenRemovedV1 {
                    capture_history_index,
                    item,
                    kind,
                } if *capture_history_index == index => Some((at, item.as_ref(), kind)),
                _ => None,
            })
            .collect();
        if completions.is_empty() {
            if live.as_slice() != [item] || !removals.is_empty() {
                return Err("Arahbo live token was dropped or changed".into());
            }
        } else {
            let [(completed, removal, token)] = completions.as_slice() else {
                return Err("Arahbo token completed more than once".into());
            };
            let [(removed, original, kind)] = removals.as_slice() else {
                return Err("Arahbo completion has no unique actual removal".into());
            };
            if !live.is_empty()
                || *original != item
                || *removal != *removed as u64
                || published_at >= *removed
                || *removed >= *completed
            {
                return Err("Arahbo completion changed original removal/publication".into());
            }
            let counter_inputs = state
                .engine
                .event_history
                .iter()
                .enumerate()
                .filter_map(|(at, event)| match event {
                    CommittedEvent::ArahboCounterInputV1 {
                        capture_history_index,
                        item: original,
                    } if *capture_history_index == index => Some((at, original.as_ref())),
                    _ => None,
                })
                .collect::<Vec<_>>();
            match kind {
                RemovalKindV1::Countered if matches!(counter_inputs.as_slice(), [(at, original)] if published_at < *at && *at < *removed && *original == item) =>
                    {}
                RemovalKindV1::Resolving if counter_inputs.is_empty() => {}
                _ => {
                    return Err(
                        "Arahbo closure lost its independent actual counter occurrence".into(),
                    )
                }
            }
            let should_token = *kind == RemovalKindV1::Resolving;
            match (should_token, token) {
                (true, Some(token))
                    if *token > *removal
                        && *token < *completed as u64
                        && matches!(state.engine.event_history.get(*token as usize),Some(CommittedEvent::CreateToken {controller,token_def,..})
                        if *controller==capture.source.controller && crate::card_def::card_id_by_name("Cat Token")==Some(*token_def)) =>
                    {}
                (false, None) => {}
                _ => return Err("Arahbo completion skipped or forged the actual token".into()),
            }
        }
    }
    for event in &state.engine.event_log {
        if let CommittedEvent::ArahboTokenCaptureV1 { capture } = event {
            if capture_at(state, capture.history_index).ok() != Some(capture) {
                return Err("Arahbo mutable log contains a forged capture".into());
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
        || state.engine.event_history.iter().any(|event| {
            is_history(event) || matches!(event, CommittedEvent::ArahboTokenCaptureV1 { .. })
        })
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
pub(crate) mod future_tests;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    fn registered_changeling_materializes_avatar_through_entry_and_restore_both_seats() {
        use crate::card_def::Subtype;
        for actor in [crate::ids::PlayerId::P0, crate::ids::PlayerId::P1] {
            let mut state = crate::policy_observation_v6::tests::ready_state();
            let source = crate::policy_observation_v6::tests::put(
                &mut state,
                actor,
                "Webweaver Changeling",
                Zone::Hand,
            );
            crate::event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(source, Zone::Battlefield),
            );
            for snapshot in [
                state.clone(),
                serde_json::from_slice::<GameState>(&serde_json::to_vec(&state).unwrap()).unwrap(),
            ] {
                let live = snapshot.objects.get(source);
                assert_eq!(live.zone, Zone::Battlefield);
                assert_eq!(live.controller, actor);
                assert!(Subtype::Avatar.is_in_subtype_ids(&live.v4.effective_subtype_ids));
                assert!(Subtype::Cat.is_in_subtype_ids(&live.v4.effective_subtype_ids));
                assert!(!Subtype::Aura.is_in_subtype_ids(&live.v4.effective_subtype_ids));
            }
        }
    }

    #[test]
    fn registered_absent_source_hooks_preserve_bytes_and_hash_both_seats() {
        if enabled() {
            return;
        }
        for actor in [crate::ids::PlayerId::P0, crate::ids::PlayerId::P1] {
            let mut state = crate::policy_observation_v6::tests::ready_state();
            state.active_player = actor;
            let before = serde_json::to_vec(&state).unwrap();
            let hash = state.diagnostic_state_hash();
            record_single_entry(&mut state, 0);
            validate_state(&state).unwrap();
            assert_eq!(serde_json::to_vec(&state).unwrap(), before);
            assert_eq!(state.diagnostic_state_hash(), hash);
        }
    }
}
