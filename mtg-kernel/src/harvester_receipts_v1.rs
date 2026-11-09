//! Exact Harvester payment, actual moves and token births from the native owner.
use crate::{
    effect::{harvester_v1 as native, EffectObjectBinding},
    event::{CommittedEvent as E, ProposedEvent},
    event_history_v1::EventKindV1,
    ids::{ObjectId, PlayerId, StackItemId},
    state::{
        AbilitySourceContractV4, GameState, StackItem, StackItemKind, StackTargetContractV4,
        Target, Zone,
    },
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublishedV1 {
    pub item: Box<StackItem>,
    pub tap_at: u64,
    pub target_entry: u64,
    pub targeted_at: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MoveV1 {
    pub original: AbilitySourceContractV4,
    pub public_printing: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_face_down: Option<bool>,
    pub event_at: u64,
    pub destination: Zone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatedV1 {
    pub source: AbilitySourceContractV4,
    pub event_at: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum OutcomeV1 {
    Resolved,
    NoLegalTarget,
    Countered,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum JournalV1 {
    Published(PublishedV1),
    Moved {
        id: StackItemId,
        movement: MoveV1,
    },
    Created {
        id: StackItemId,
        token: CreatedV1,
    },
    Finished {
        id: StackItemId,
        outcome: OutcomeV1,
    },
    Dispatched {
        id: StackItemId,
        outcome: OutcomeV1,
    },
    TargetAnnounced {
        id: StackItemId,
        target: ObjectId,
        generation: u32,
        controller: PlayerId,
    },
}

fn push(state: &mut GameState, journal: JournalV1) {
    state
        .engine
        .event_history
        .push(E::HarvesterCommitReceiptV1 {
            journal: Box::new(journal),
        });
}

pub(crate) fn selected(item: &StackItem) -> bool {
    item.kind == StackItemKind::ActivatedAbility
        && item.inline_effect.as_ref().is_some_and(native::recipe)
}

fn shape(item: &StackItem) -> Result<(AbilitySourceContractV4, StackTargetContractV4), String> {
    let source = item
        .v4
        .ability_source_contract
        .ok_or("Harvester original ability source absent")?;
    let [target] = item.v4.target_contracts.as_slice() else {
        return Err("Harvester original target contract absent".into());
    };
    let StackTargetContractV4::Object {
        object,
        zone: Zone::Graveyard,
        spell_copy_origin: None,
        ..
    } = target
    else {
        return Err("Harvester original target is not a physical graveyard card".into());
    };
    if !selected(item)
        || item.v4.stack_item_id == StackItemId::default()
        || item.v4.activated_ability_index != Some(0)
        || item.v4.granted_by.is_some()
        || item.v4.target_spec != Some(native::target_spec())
        || item.targets != [Target::Object(*object)]
        || !native::definition(source.card_def)
        || source.zone != Zone::Battlefield
        || source.source != item.source
        || source.controller != item.controller
    {
        return Err("Harvester paid ability changed exact printing, target or controller".into());
    }
    Ok((source, *target))
}

pub(crate) fn record_published(state: &mut GameState, payment_start: usize) -> Result<(), String> {
    if !state.harvester_commit_receipts_v1 {
        return Ok(());
    }
    let item = state.stack.last().filter(|item| selected(item)).cloned();
    let Some(item) = item else { return Ok(()) };
    let (source, target) = shape(&item)?;
    crate::engine::validate_historical_ability_source_contract(state, source)?;
    let StackTargetContractV4::Object {
        object,
        card_def,
        owner,
        zone_change_count,
        ..
    } = target
    else {
        unreachable!("validated Harvester target is an object")
    };
    let history = &state.engine.event_history;
    let mut taps = history
        .iter()
        .enumerate()
        .skip(payment_start)
        .filter_map(|(at, event)| {
            matches!(event, E::Tap { object } if *object == source.source).then_some(at)
        });
    let tap_at = taps.next().ok_or("Harvester native tap payment absent")?;
    let mut entries = history.iter().enumerate().take(tap_at).filter_map(|(at, event)| {
        matches!(event, E::HarvesterGraveyardEntryV1 { entry }
            if entry.card.object == object && entry.card.expected_zone_change_count == zone_change_count
                && entry.card_def == card_def && entry.owner == owner).then_some(at)
    });
    let target_entry = entries
        .next()
        .ok_or("Harvester chosen card lacks actual this-turn entry")?;
    if taps.next().is_some() || entries.next().is_some() || !native::eligible(state, object) {
        return Err("Harvester native tap, entry or target announcement changed".into());
    }
    let receipt = PublishedV1 {
        item: Box::new(item),
        tap_at: u64::try_from(tap_at).map_err(|_| "Harvester tap history width")?,
        target_entry: u64::try_from(target_entry)
            .map_err(|_| "Harvester target entry history width")?,
        targeted_at: u64::try_from(history.len())
            .map_err(|_| "Harvester targeted history width")?,
    };
    push(
        state,
        JournalV1::TargetAnnounced {
            id: receipt.item.v4.stack_item_id,
            target: object,
            generation: zone_change_count,
            controller: source.controller,
        },
    );
    push(state, JournalV1::Published(receipt));
    Ok(())
}

pub(crate) fn record_finished(state: &mut GameState, item: &StackItem, outcome: OutcomeV1) {
    if state.harvester_commit_receipts_v1 && selected(item) {
        push(
            state,
            JournalV1::Finished {
                id: item.v4.stack_item_id,
                outcome,
            },
        );
    }
}

pub(crate) fn record_dispatch(state: &mut GameState, item: &StackItem, outcome: OutcomeV1) {
    if state.harvester_commit_receipts_v1 && selected(item) {
        push(
            state,
            JournalV1::Dispatched {
                id: item.v4.stack_item_id,
                outcome,
            },
        );
    }
}

pub(crate) fn move_proposal(object: ObjectId, owner: Option<StackItemId>) -> ProposedEvent {
    let ProposedEvent::ZoneChange(mut change) = ProposedEvent::zone_change(object, Zone::Exile)
    else {
        unreachable!("native constructor builds zone changes");
    };
    change.harvester_owner = owner;
    ProposedEvent::ZoneChange(change)
}

pub(crate) enum OperationV1 {
    Move {
        id: StackItemId,
        original: AbilitySourceContractV4,
        public_printing: u16,
        original_face_down: Option<bool>,
    },
    Create {
        id: StackItemId,
    },
}

pub(crate) fn capture_operation(
    state: &GameState,
    proposal: &ProposedEvent,
) -> Option<OperationV1> {
    match proposal {
        ProposedEvent::ZoneChange(change) => change.harvester_owner.map(|id| {
            let original = AbilitySourceContractV4::capture(state, change.object);
            OperationV1::Move {
                id,
                original,
                public_printing: crate::rl::projected_card_def(
                    state,
                    change.object,
                    original.zone_change_count,
                ),
                original_face_down: state
                    .objects
                    .get(change.object)
                    .v4
                    .face_down_v1
                    .map(|face| face.disguised),
            }
        }),
        ProposedEvent::CreateToken(token) => {
            token.harvester_owner.map(|id| OperationV1::Create { id })
        }
        _ => None,
    }
}

impl OperationV1 {
    pub(crate) fn record(self, state: &mut GameState, event_at: usize) -> Result<(), String> {
        let original_event = state
            .engine
            .event_history
            .get(event_at)
            .ok_or("Harvester committed operation absent")?;
        let event_at =
            u64::try_from(event_at).map_err(|_| "Harvester actual operation history width")?;
        let journal = match (self, original_event) {
            (
                Self::Move {
                    id,
                    original,
                    public_printing,
                    original_face_down,
                },
                E::ZoneChange {
                    object,
                    from,
                    to,
                    controller_before,
                },
            ) if *object == original.source
                && *from == original.zone
                && *controller_before == original.controller =>
            {
                JournalV1::Moved {
                    id,
                    movement: MoveV1 {
                        original,
                        public_printing,
                        original_face_down,
                        event_at,
                        destination: *to,
                    },
                }
            }
            (
                Self::Create { id },
                E::CreateToken {
                    object,
                    token_def,
                    controller,
                    ..
                },
            ) => {
                let source = AbilitySourceContractV4::capture(state, *object);
                if source.card_def != *token_def
                    || source.controller != *controller
                    || !state.objects.get(*object).v4.copied_nightmare_v1
                {
                    return Err(
                        "Harvester actual token lost its copiable Nightmare exception".into(),
                    );
                }
                JournalV1::Created {
                    id,
                    token: CreatedV1 { source, event_at },
                }
            }
            _ => return Err("Harvester actual commit changed native operation kind".into()),
        };
        push(state, journal);
        Ok(())
    }
}

pub(crate) struct OperationSet<'a> {
    pub publication_at: usize,
    pub publication: &'a PublishedV1,
    pub target_entry: native::GraveyardEntryV1,
    pub moves: Vec<MoveV1>,
    pub created: Vec<CreatedV1>,
    pub outcome: Option<OutcomeV1>,
}

#[derive(Default)]
struct Rows<'a> {
    publication: Option<(usize, &'a PublishedV1)>,
    moves: Vec<(usize, MoveV1)>,
    created: Vec<(usize, CreatedV1)>,
    outcome: Option<(usize, OutcomeV1)>,
    dispatch: Option<(usize, OutcomeV1)>,
    announced: Option<usize>,
    legacy_resolution: Option<(usize, &'a native::ResolutionV1)>,
}

pub(crate) fn operations(state: &GameState) -> Result<Vec<OperationSet<'_>>, String> {
    let history = &state.engine.event_history;
    let mut rows = BTreeMap::<StackItemId, Rows>::new();
    for (at, event) in state.engine.event_history.scan_for_v1(&[
        EventKindV1::HarvesterCommitReceiptV1,
        EventKindV1::HarvesterResolvedV1,
    ]).iter().enumerate() {
        let (E::HarvesterCommitReceiptV1 { .. } | E::HarvesterResolvedV1 { .. }) = event else { continue; };
        let duplicate = match event {
            E::HarvesterResolvedV1 { resolution } => rows
                .entry(resolution.item.v4.stack_item_id)
                .or_default()
                .legacy_resolution
                .replace((at, resolution))
                .is_some(),
            E::HarvesterCommitReceiptV1 { journal } => match journal.as_ref() {
                JournalV1::Published(publication) => rows
                    .entry(publication.item.v4.stack_item_id)
                    .or_default()
                    .publication
                    .replace((at, publication))
                    .is_some(),
                JournalV1::Moved { id, movement } => {
                    rows.entry(*id).or_default().moves.push((at, *movement));
                    false
                }
                JournalV1::Created { id, token } => {
                    rows.entry(*id).or_default().created.push((at, *token));
                    false
                }
                JournalV1::Finished { id, outcome } => rows
                    .entry(*id)
                    .or_default()
                    .outcome
                    .replace((at, *outcome))
                    .is_some(),
                JournalV1::Dispatched { id, outcome } => rows
                    .entry(*id)
                    .or_default()
                    .dispatch
                    .replace((at, *outcome))
                    .is_some(),
                JournalV1::TargetAnnounced { id, .. } => {
                    rows.entry(*id).or_default().announced.replace(at).is_some()
                }
            },
            _ => false,
        };
        if duplicate {
            return Err("Harvester native activation operation duplicated".into());
        }
    }
    let mut output = Vec::new();
    for (id, row) in rows {
        let (published_at, publication) = row
            .publication
            .ok_or("Harvester operation lacks native publication")?;
        let (source, target) = shape(&publication.item)?;
        crate::sampled_history_v1::validate_source(state, published_at, source)?;
        let StackTargetContractV4::Object {
            object,
            owner,
            card_def,
            zone_change_count,
            ..
        } = target
        else {
            unreachable!("validated Harvester target is an object")
        };
        let tap = usize::try_from(publication.tap_at).map_err(|_| "Harvester tap width")?;
        let announced =
            usize::try_from(publication.targeted_at).map_err(|_| "Harvester targeted width")?;
        let target_at = usize::try_from(publication.target_entry)
            .map_err(|_| "Harvester target receipt width")?;
        let target_entry = native::graveyard_entry(state, target_at)?;
        if !(target_at < tap && tap < announced && announced.checked_add(1) == Some(published_at))
            || row.announced != Some(announced)
            || !matches!(history.get(tap), Some(E::Tap { object }) if *object == source.source)
            || history.iter().take(announced).skip(tap + 1).any(|event| {
                matches!(event, E::Tap { object } | E::ZoneChange { object, .. }
                    if *object == source.source)
            })
            || !matches!(history.get(announced), Some(E::HarvesterCommitReceiptV1 { journal })
                if matches!(journal.as_ref(), JournalV1::TargetAnnounced { id: actual, target, generation, controller }
                    if *actual == id && *target == object && *generation == zone_change_count
                        && *controller == source.controller))
            || target_entry.card.object != object
            || target_entry.card.expected_zone_change_count != zone_change_count
            || target_entry.owner != owner
            || target_entry.card_def != card_def
        {
            return Err("Harvester publication changed native cost, target or entry".into());
        }
        let live: Vec<_> = state
            .stack
            .iter()
            .filter(|item| item.v4.stack_item_id == id)
            .collect();
        if row.outcome.is_none() && live.as_slice() != [publication.item.as_ref()]
            || row.outcome.is_some() && !live.is_empty()
        {
            return Err("Harvester native activation lost its live/finished lifetime".into());
        }
        let outcome = row.outcome.map(|(_, outcome)| outcome);
        let dispatch_at = match (row.dispatch, row.outcome) {
            (None, None) => None,
            (Some((start, operation)), Some((end, claimed)))
                if published_at < start && start < end && operation == claimed =>
            {
                Some(start)
            }
            _ => return Err("Harvester terminal changed actual native dispatch".into()),
        };
        let mut moves = Vec::new();
        let mut created = Vec::new();
        if let (Some((end, OutcomeV1::Resolved)), Some((legacy_at, resolution))) =
            (row.outcome, row.legacy_resolution)
        {
            let dispatched = dispatch_at.ok_or("Harvester native resolution dispatch absent")?;
            if dispatched >= legacy_at
                || legacy_at.checked_add(1) != Some(end)
                || resolution.item.as_ref() != publication.item.as_ref()
            {
                return Err("Harvester resolution lost its original native activation".into());
            }
            let mut moved = BTreeSet::<ObjectId>::new();
            for (at, movement) in row.moves {
                let original = movement.original;
                let event_at =
                    usize::try_from(movement.event_at).map_err(|_| "Harvester move width")?;
                let target_move = original.zone == Zone::Graveyard
                    && original.source == object
                    && original.owner == owner
                    && original.card_def == card_def
                    && original.zone_change_count == zone_change_count
                    && movement.public_printing == card_def;
                let cleanup = original.zone == Zone::Battlefield
                    && original.controller == source.controller
                    && resolution.exiled_others.iter().any(|binding| {
                        binding.object == original.source
                            && binding.expected_zone == original.zone
                            && binding.expected_zone_change_count == original.zone_change_count
                    });
                let public_printing = match movement.original_face_down {
                    Some(disguised) => crate::card_def::card_id_by_name(if disguised {
                        "Face-down creature"
                    } else {
                        "Face-down card"
                    }),
                    None => Some(original.card_def),
                };
                if !(target_move || cleanup)
                    || event_at <= dispatched
                    || at >= legacy_at
                    || !moved.insert(original.source)
                    || public_printing != Some(movement.public_printing)
                    || event_at.checked_add(1) != Some(at)
                    || !matches!(history.get(event_at), Some(E::ZoneChange { object, from, to, controller_before })
                        if *object == original.source && *from == original.zone
                            && *to == movement.destination && *controller_before == original.controller)
                    || crate::card_def::CARD_DEFS
                        .get(usize::from(movement.public_printing))
                        .is_none()
                {
                    return Err("Harvester actual movement changed native occurrence".into());
                }
                moves.push(movement);
            }
            for (at, token) in row.created {
                let original = token.source;
                let event_at =
                    usize::try_from(token.event_at).map_err(|_| "Harvester token history width")?;
                if event_at <= dispatched
                    || at >= legacy_at
                    || event_at.checked_add(1) != Some(at)
                    || original.card_def != card_def
                    || original.owner != source.controller
                    || original.controller != source.controller
                    || original.zone != Zone::Battlefield
                    || !matches!(history.get(event_at), Some(E::CreateToken { object, token_def, controller, .. })
                        if *object == original.source && *token_def == card_def && *controller == source.controller)
                    || !native::historical_nightmare(state, original, legacy_at)?
                {
                    return Err(
                        "Harvester actual token lost native copy, owner or Nightmare layer".into(),
                    );
                }
                created.push(token);
            }
            if created
                .iter()
                .map(|token| EffectObjectBinding {
                    object: token.source.source,
                    expected_zone: token.source.zone,
                    expected_zone_change_count: token.source.zone_change_count,
                })
                .ne(resolution.created.iter().copied())
            {
                return Err("Harvester native resolution lost actual token roster".into());
            }
            let committed_moves = history[dispatched + 1..legacy_at]
                .iter()
                .filter_map(|event| match event {
                    E::ZoneChange {
                        object: actual,
                        from: Zone::Graveyard,
                        ..
                    } if *actual == object => Some(*actual),
                    E::ZoneChange {
                        object: actual,
                        from: Zone::Battlefield,
                        ..
                    } => Some(*actual),
                    _ => None,
                })
                .collect::<BTreeSet<_>>();
            if moved != committed_moves {
                return Err("Harvester resolution lost an actual committed departure".into());
            }
            let actual_births = history[dispatched + 1..legacy_at]
                .iter()
                .filter_map(|event| match event {
                    E::NightmareCopyEnteredV1 { entry } => Some(entry.source),
                    _ => None,
                });
            if created.iter().map(|token| token.source).ne(actual_births) {
                return Err("Harvester resolution lost an actual Nightmare copy birth".into());
            }
        } else {
            let previous = if let Some((legacy_at, resolution)) = row.legacy_resolution {
                if outcome != Some(OutcomeV1::NoLegalTarget)
                    || dispatch_at.and_then(|start| start.checked_add(1)) != Some(legacy_at)
                    || resolution.item.as_ref() != publication.item.as_ref()
                    || !resolution.created.is_empty()
                    || !resolution.exiled_others.is_empty()
                {
                    return Err("Harvester fizzle fabricated a resolution".into());
                }
                Some(legacy_at)
            } else {
                dispatch_at
            };
            if !row.moves.is_empty()
                || !row.created.is_empty()
                || row.outcome.is_some_and(|(at, outcome)| {
                    outcome == OutcomeV1::Resolved
                        || previous.and_then(|start| start.checked_add(1)) != Some(at)
                })
            {
                return Err("Harvester nonresolving native occurrence fabricated outcomes".into());
            }
        }
        output.push(OperationSet {
            publication_at: published_at,
            publication,
            target_entry,
            moves,
            created,
            outcome,
        });
    }
    for item in state.stack.iter().filter(|item| selected(item)) {
        if !output
            .iter()
            .any(|row| row.publication.item.as_ref() == item)
        {
            return Err("Harvester live stack row lacks its actual paid publication".into());
        }
    }
    Ok(output)
}
