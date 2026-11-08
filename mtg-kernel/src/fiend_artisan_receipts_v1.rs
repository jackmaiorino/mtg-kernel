//! Public evidence captured at Fiend Artisan's actual sacrifice, move and counter commits.
use crate::{
    card_def::CostComponent,
    effect::{fiend_artisan_v1 as native, EffectObjectBinding},
    event::{CommittedEvent as E, ProposedEvent},
    event_history_v1::EventKindV1 as K,
    ids::{ObjectId, StackItemId},
    optional_trigger_public_v1::{self as public, IdentityV1, ReferenceV1},
    state::{AbilitySourceContractV4, GameState, StackItem, Zone},
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum JournalV1 {
    SacrificeFace {
        input: u32,
        observed: ReferenceV1,
    },
    ArrivalVisibility {
        id: StackItemId,
        object: ObjectId,
        observed: Option<ReferenceV1>,
    },
    Sacrificed {
        input: u32,
        original: ReferenceV1,
        source: AbilitySourceContractV4,
        face_down: Option<bool>,
    },
    Moved {
        id: StackItemId,
        original: EffectObjectBinding,
        destination: Zone,
        arrival: Option<ReferenceV1>,
        arrival_face_down: Option<bool>,
    },
    Countered { id: StackItemId },
}

const KINDS: &[K] = &[
    K::FiendArtisanInputV1,
    K::FiendArtisanPaidV1,
    K::FiendArtisanSearchStartedV1,
    K::FiendArtisanSearchFinishedV1,
    K::FiendArtisanConsumerReceiptV1,
];

pub(crate) fn refuse_legacy(state: &GameState) -> Result<(), String> {
    if state.fiend_artisan_receipts_v1
        || !state.engine.event_history.scan_for_v1(KINDS).is_empty()
        || state.stack.iter().any(|item| native::selected(state, item))
        || state.objects.iter().any(|(_, live)| public_destination(live.zone)
            && live.v4.face_down_v1.is_none() && native::definition(live.card_def))
    {
        return Err("Fiend Artisan requires its distinct complete consumer".into());
    }
    Ok(())
}

fn push(state: &mut GameState, journal: JournalV1) {
    state.engine.event_history.push(E::FiendArtisanConsumerReceiptV1 {
        journal: Box::new(journal),
    });
}

pub(crate) fn payment_owner(
    state: &GameState,
    source: ObjectId,
    components: &[CostComponent],
) -> Option<u32> {
    if !state.fiend_artisan_receipts_v1 || components != native::COST {
        return None;
    }
    let history = &state.engine.event_history;
    let at = history.len().checked_sub(1)?;
    let E::FiendArtisanInputV1 { input } = history.get(at)? else { return None };
    (input.source.source == source).then_some(u32::try_from(at).ok()?)
}

pub(crate) fn observe_sacrifice_face(state: &mut GameState, input: u32, object: ObjectId) {
    if let Some(observed) = public::reference(state, object, None) {
        push(state, JournalV1::SacrificeFace { input, observed });
    }
}

pub(crate) fn record_sacrifice(state: &mut GameState, input: u32, object: ObjectId) {
    if let Some(original) = public::reference(state, object, None) {
        let source = AbilitySourceContractV4::capture(state, object);
        let face_down = state.objects.get(object).v4.face_down_v1.map(|face| face.disguised);
        push(state, JournalV1::Sacrificed { input, original, source, face_down });
    }
}

pub(crate) fn owned_move(
    mut proposal: ProposedEvent,
    owner: Option<StackItemId>,
) -> ProposedEvent {
    if let ProposedEvent::ZoneChange(change) = &mut proposal {
        change.fiend_search_owner = owner;
    }
    proposal
}

fn public_destination(zone: Zone) -> bool {
    matches!(zone, Zone::Battlefield | Zone::Graveyard | Zone::Exile | Zone::Stack | Zone::Command)
}

pub(crate) fn observe_arrival_visibility(
    state: &mut GameState,
    id: StackItemId,
    source: AbilitySourceContractV4,
    event_at: usize,
) {
    let Some(E::ZoneChange { object, from: Zone::Library, to, .. }) =
        state.engine.event_history.get(event_at)
    else { return };
    if *object != source.source || source.zone != Zone::Library {
        return;
    }
    let object = *object;
    let observed = public_destination(*to)
        .then(|| public::reference(state, object, None))
        .flatten();
    push(state, JournalV1::ArrivalVisibility { id, object, observed });
}

pub(crate) fn record_move(
    state: &mut GameState,
    id: StackItemId,
    source: AbilitySourceContractV4,
    event_at: usize,
) {
    let Some(E::ZoneChange { object, from: Zone::Library, to, .. }) =
        state.engine.event_history.get(event_at)
    else { return };
    if *object != source.source || source.zone != Zone::Library {
        return;
    }
    let original = EffectObjectBinding {
        object: source.source,
        expected_zone: source.zone,
        expected_zone_change_count: source.zone_change_count,
    };
    let destination = *to;
    let arrival = public_destination(destination)
        .then(|| public::reference(state, source.source, None))
        .flatten();
    let arrival_face_down = state.objects.get(source.source).v4.face_down_v1
        .map(|face| face.disguised);
    push(state, JournalV1::Moved { id, original, destination, arrival, arrival_face_down });
}

pub(crate) fn record_countered(state: &mut GameState, item: &StackItem) {
    if state.fiend_artisan_receipts_v1 && native::selected(state, item) {
        push(state, JournalV1::Countered { id: item.v4.stack_item_id });
    }
}

#[derive(Default)]
struct Row<'a> {
    paid: Option<(usize, u32, &'a StackItem)>,
    started: Option<(usize, &'a [EffectObjectBinding])>,
    moved: Option<Movement<'a>>,
    arrival_visibility: Option<(usize, ObjectId, Option<&'a ReferenceV1>)>,
    finished: Option<usize>,
    countered: Option<usize>,
}

struct Movement<'a> {
    at: usize,
    original: EffectObjectBinding,
    destination: Zone,
    arrival: Option<&'a ReferenceV1>,
    face_down: Option<bool>,
}

pub(crate) struct Activation<'a> {
    pub item: &'a StackItem,
    pub x: u8,
    pub sacrifice: &'a ReferenceV1,
    pub stack_position: Option<u32>,
    pub searching: bool,
    pub shuffled: bool,
    pub countered: bool,
    pub arrival: Option<&'a ReferenceV1>,
}

pub(crate) fn activations(state: &GameState) -> Result<Vec<Activation<'_>>, String> {
    if !state.fiend_artisan_receipts_v1 {
        return Err("Fiend Artisan requires its scoped native receipts".into());
    }
    if state.engine.pending_effect.as_ref()
        .is_some_and(|pending| native::selected(state, &pending.resolving_item))
    {
        crate::effect::validate_pending_effect_choice(state)?;
    }
    let history = &state.engine.event_history;
    let mut rows = BTreeMap::<StackItemId, Row<'_>>::new();
    let mut sacrifices = BTreeMap::new();
    let mut sacrifice_faces = BTreeMap::new();
    for (at, event) in history.iter().enumerate() {
        let duplicate = match event {
            E::FiendArtisanPaidV1 { input_index, item } =>
                rows.entry(item.v4.stack_item_id).or_default()
                    .paid.replace((at, *input_index, item.as_ref())).is_some(),
            E::FiendArtisanSearchStartedV1 { stack_item_id, library } =>
                rows.entry(*stack_item_id).or_default()
                    .started.replace((at, library.as_slice())).is_some(),
            E::FiendArtisanSearchFinishedV1 { stack_item_id } =>
                rows.entry(*stack_item_id).or_default().finished.replace(at).is_some(),
            E::FiendArtisanConsumerReceiptV1 { journal } => match journal.as_ref() {
                JournalV1::SacrificeFace { input, observed } =>
                    sacrifice_faces.insert(*input, (at, observed)).is_some(),
                JournalV1::ArrivalVisibility { id, object, observed } =>
                    rows.entry(*id).or_default().arrival_visibility
                        .replace((at, *object, observed.as_ref())).is_some(),
                JournalV1::Sacrificed { input, original, source, face_down } =>
                    sacrifices.insert(*input, (at, original, *source, *face_down)).is_some(),
                JournalV1::Moved { id, original, destination, arrival, arrival_face_down } =>
                    rows.entry(*id).or_default().moved.replace(Movement {
                        at, original: *original, destination: *destination,
                        arrival: arrival.as_ref(), face_down: *arrival_face_down,
                    }).is_some(),
                JournalV1::Countered { id } =>
                    rows.entry(*id).or_default().countered.replace(at).is_some(),
            },
            _ => false,
        };
        if duplicate {
            return Err("Fiend Artisan duplicate native occurrence".into());
        }
    }
    let mut result = Vec::with_capacity(rows.len());
    for (id, row) in rows {
        let (paid_at, input_index, item) = row.paid.ok_or("Fiend Artisan native paid row absent")?;
        let x = native::authenticated_paid_x(state, item)?;
        let (sacrifice_at, sacrifice, paid_source, face_down) = sacrifices.remove(&input_index)
            .ok_or("Fiend Artisan actual sacrifice receipt absent")?;
        let (face_at, observed) = sacrifice_faces.remove(&input_index)
            .ok_or("Fiend Artisan committed sacrifice visibility absent")?;
        let Some(E::FiendArtisanInputV1 { input }) = history.get(input_index as usize)
        else { return Err("Fiend Artisan original payment absent".into()) };
        let [binding] = input.pending.object_cost_chosen.as_slice()
        else { return Err("Fiend Artisan original sacrifice absent".into()) };
        crate::engine::validate_historical_ability_source_contract(state, paid_source)?;
        let paid_identity = face_down.map_or(
            IdentityV1::Known { card_id: paid_source.card_def },
            |disguised| IdentityV1::FaceDown { disguised },
        );
        if !(input_index as usize + 1 < face_at && face_at < sacrifice_at && sacrifice_at < paid_at)
            || face_at + 1 != sacrifice_at
            || observed != sacrifice
            || paid_source.source != binding.object
            || paid_source.zone != Zone::Battlefield
            || paid_source.zone_change_count != binding.expected_zone_change_count
            || paid_source.controller != item.controller
            || sacrifice.identity != paid_identity
            || sacrifice.object != binding.object.0
            || sacrifice.generation != binding.expected_zone_change_count
            || sacrifice.zone != Zone::Battlefield
            || sacrifice.controller != item.controller.into()
            || !matches!(history.get(face_at - 1),
                Some(E::Sacrificed { object, controller_before, .. })
                if *object == binding.object && *controller_before == item.controller)
            || state.objects.try_get(binding.object).map(|o| crate::rl::PlayerSeatV1::from(o.owner))
                != Some(sacrifice.owner)
        {
            return Err("Fiend Artisan sacrifice differs from actual original payment".into());
        }

        let mut live = state.stack.iter().enumerate().filter(|(_, item)| item.v4.stack_item_id == id);
        let stack_position = match live.next() {
            Some((at, original)) if original == item && live.next().is_none() =>
                Some(u32::try_from(at).map_err(|_| "Fiend Artisan stack position width")?),
            None => None,
            _ => return Err("Fiend Artisan original live row changed".into()),
        };
        let pending = state.engine.pending_effect.as_ref()
            .is_some_and(|pending| pending.resolving_item.v4.stack_item_id == id);
        if row.moved.is_none() != row.arrival_visibility.is_none() {
            return Err("Fiend Artisan native search move visibility incomplete".into());
        }
        let arrival = match (row.started, row.finished, row.countered) {
            (None, None, None) if stack_position.is_some() && !pending && row.moved.is_none() => None,
            (None, None, Some(at)) if stack_position.is_none() && paid_at < at
                && !pending && row.moved.is_none() => None,
            (Some((start, _)), None, None) if stack_position.is_some() && paid_at < start
                && pending && row.moved.is_none() => None,
            (Some((start, original)), Some(end), None) if stack_position.is_none() && !pending
                && paid_at < start && start < end => {
                let ids = original.iter().map(|binding| binding.object).collect::<BTreeSet<_>>();
                if ids.len() != original.len() || original.iter().any(|b| b.expected_zone != Zone::Library) {
                    return Err("Fiend Artisan original search roster changed".into());
                }
                let actual = history.iter().enumerate().take(end).skip(start + 1)
                    .filter(|(_, event)| matches!(event,
                        E::ZoneChange { object, from: Zone::Library, .. } if ids.contains(object)))
                    .collect::<Vec<_>>();
                match (actual.as_slice(), row.moved) {
                    ([], None) => None,
                    ([(event_at, E::ZoneChange { object, to, .. })],
                        Some(Movement { at, original: original_binding, destination, arrival, face_down }))
                        if *event_at + 2 == at && at < end && *object == original_binding.object
                            && original.contains(&original_binding) && *to == destination =>
                    {
                        if row.arrival_visibility != Some((*event_at + 1, *object, arrival)) {
                            return Err("Fiend Artisan arrival disagrees with committed visibility".into());
                        }
                        if let Some(card) = arrival {
                            let live = state.objects.try_get(*object);
                            let identity_matches_face = match (&card.identity, face_down) {
                                (IdentityV1::FaceDown { disguised }, Some(captured)) => *disguised == captured,
                                (IdentityV1::Known { card_id }, None) =>
                                    crate::card_def::CARD_DEFS.get(usize::from(*card_id)).is_some(),
                                _ => false,
                            };
                            if !identity_matches_face || !public_destination(destination) || card.object != object.0
                                || card.zone != destination
                                || original_binding.expected_zone_change_count.checked_add(1)
                                    != Some(card.generation)
                                || live.map(|o| crate::rl::PlayerSeatV1::from(o.owner)) != Some(card.owner)
                                || live.filter(|o| o.zone_change_count == card.generation
                                    && o.controller.index() == card.controller.index()
                                    && !history.iter().skip(at + 1).any(|event| matches!(event,
                                        E::TurnedFaceUp { object: turned, zone_change_count }
                                            if *turned == *object && *zone_change_count == card.generation)))
                                    .is_some_and(|_| public::reference(state, *object, None).as_ref() != Some(card))
                            {
                                return Err("Fiend Artisan arrival changed actual public incarnation".into());
                            }
                        } else if public_destination(destination) {
                            return Err("Fiend Artisan public arrival receipt absent".into());
                        }
                        arrival
                    }
                    _ => return Err("Fiend Artisan actual search movement changed".into()),
                }
            }
            _ => return Err("Fiend Artisan original native lifecycle changed".into()),
        };
        result.push(Activation {
            item, x, sacrifice, stack_position, searching: pending,
            shuffled: row.finished.is_some(), countered: row.countered.is_some(), arrival,
        });
    }
    if !sacrifices.is_empty() || !sacrifice_faces.is_empty() || state.stack.iter().any(|item|
        native::selected(state, item) && !result.iter().any(|row| row.item == item))
    {
        return Err("Fiend Artisan original native payment custody incomplete".into());
    }
    Ok(result)
}
