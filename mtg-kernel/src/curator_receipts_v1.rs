//! Native Curator pile reveal and actual replacement-aware departures.
use crate::{
    effect::EffectObjectBinding,
    event::CommittedEvent as E,
    event_history_v1::EventKindV1,
    ids::{ObjectId, PlayerId},
    state::{GameState, Zone},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MoveOwnerV1 {
    pub entry: u64,
    pub controller: PlayerId,
    pub planned: Zone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShownV1 {
    pub original: EffectObjectBinding,
    pub printing: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MovedV1 {
    pub event_at: u64,
    pub original: EffectObjectBinding,
    pub owner: PlayerId,
    pub planned: Zone,
    pub destination: Zone,
    pub public_printing: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum JournalV1 {
    Revealed { entry: u64, controller: PlayerId, shown: Vec<ShownV1> },
    Moved { entry: u64, move_row: MovedV1 },
}

fn rows(state: &GameState) -> impl Iterator<Item = (usize, &JournalV1)> {
    state.engine.event_history.scan_for_v1(&[EventKindV1::CuratorReceiptV1])
        .iter().enumerate().filter_map(|(at, event)| match event {
            E::CuratorReceiptV1 { journal } => Some((at, journal.as_ref())),
            _ => None,
        })
}

fn record(state: &mut GameState, journal: JournalV1) {
    state.engine.event_history.push(E::CuratorReceiptV1 { journal: Box::new(journal) });
}

pub(crate) fn record_reveal(
    state: &mut GameState,
    entry: u64,
    controller: PlayerId,
    shown_cards: impl IntoIterator<Item = EffectObjectBinding>,
) -> Result<(), String> {
    let shown = shown_cards.into_iter().map(|original| {
        let live = state.objects.get(original.object);
        if live.owner != controller || live.zone != Zone::Library
            || live.zone_change_count != original.expected_zone_change_count
        {
            return Err("Curator reveal lost its original looked incarnation".into());
        }
        Ok(ShownV1 { original, printing: live.card_def })
    }).collect::<Result<Vec<_>, String>>()?;
    record(state, JournalV1::Revealed { entry, controller, shown });
    Ok(())
}

fn reveal(state: &GameState, entry: u64) -> Result<(usize, PlayerId, &[ShownV1]), String> {
    let mut receipts = rows(state).filter_map(|(at, journal)| match journal {
        JournalV1::Revealed { entry: owner, controller, shown } if *owner == entry =>
            Some((at, *controller, shown.as_slice())),
        _ => None,
    });
    let only = receipts.next().ok_or("Curator face-up pile lacks its native reveal")?;
    if receipts.next().is_some() {
        return Err("Curator native pile reveal was duplicated".into());
    }
    Ok(only)
}

pub(crate) fn validate_reveal<'a>(
    state: &'a GameState,
    choice: &crate::effect::curator_v1::ChoiceV1,
    live: bool,
) -> Result<(usize, &'a [ShownV1]), String> {
    let (at, controller, shown) = reveal(state, choice.entry)?;
    let expected = choice.prefix.iter().filter(|binding| !choice.down.contains(binding));
    if controller != choice.player || shown.iter().map(|row| &row.original).ne(expected)
        || shown.iter().any(|row| {
            crate::card_def::CARD_DEFS.get(usize::from(row.printing)).is_none()
                || live && state.objects.get(row.original.object).card_def != row.printing
        })
    {
        return Err("Curator face-up reveal changed its exact original pile".into());
    }
    Ok((at, shown))
}

pub(crate) fn record_move(
    state: &mut GameState,
    cause: MoveOwnerV1,
    original: EffectObjectBinding,
    event_at: usize,
) -> Result<(), String> {
    let Some(E::ZoneChange { object, from: Zone::Library, to, .. }) =
        state.engine.event_history.get(event_at)
    else {
        return Err("Curator move lacks an actual library departure".into());
    };
    let destination = *to;
    let live = state.objects.get(original.object);
    let (_, controller, shown) = reveal(state, cause.entry)?;
    let revealed = shown.iter().find(|row| row.original == original);
    if *object != original.object || controller != cause.controller || live.owner != controller
        || original.expected_zone != Zone::Library || live.zone != destination
        || original.expected_zone_change_count.checked_add(1) != Some(live.zone_change_count)
    {
        return Err("Curator native move changed the original library incarnation".into());
    }
    let revealed_hand = destination == Zone::Hand
        && revealed.is_some_and(|shown| shown.printing == live.card_def)
        && live.v4.face_down_v1.is_none();
    let public_destination = matches!(destination, Zone::Battlefield | Zone::Graveyard
        | Zone::Exile | Zone::Stack | Zone::Command) && live.v4.face_down_v1.is_none();
    let move_row = MovedV1 {
        event_at: u64::try_from(event_at).map_err(|_| "Curator history index width")?,
        original, owner: controller, planned: cause.planned, destination,
        public_printing: (revealed_hand || public_destination).then_some(live.card_def),
    };
    record(state, JournalV1::Moved { entry: cause.entry, move_row });
    if revealed_hand {
        // spellbench-public-reveal: the originally face-up pile arrived in hand.
        state.reveal_distinct_hand_card_with_meaning_v3(
            controller.opponent(), controller, original.object,
            crate::reveal_meaning_v3::RevealV3::CuratorPileArrival,
        )?;
    }
    Ok(())
}

pub(crate) fn resolved_moves(
    state: &GameState,
    choice: &crate::effect::curator_v1::ChoiceV1,
    phase_at: usize,
    finished_at: usize,
) -> Result<Vec<MovedV1>, String> {
    let history = &state.engine.event_history;
    let Some(choose_down) = choice.choose_down else {
        return Err("Curator resolved without the actual opponent answer".into());
    };
    let (reveal_at, shown) = validate_reveal(state, choice, false)?;
    if reveal_at >= phase_at || phase_at >= finished_at {
        return Err("Curator resolution markers changed their native order".into());
    }
    let mut seen = BTreeSet::new();
    let mut actual = Vec::new();
    for (at, journal) in rows(state) {
        let JournalV1::Moved { entry, move_row: row } = journal else { continue };
        if *entry != choice.entry {
            continue;
        }
        let event_at = usize::try_from(row.event_at).map_err(|_| "Curator history index width")?;
        let private_hand = row.destination == Zone::Hand && !shown.iter().any(|shown|
            shown.original == row.original && Some(shown.printing) == row.public_printing);
        let expected = if choice.down.contains(&row.original) == choose_down {
            Zone::Hand
        } else {
            Zone::Graveyard
        };
        if event_at <= phase_at || at >= finished_at || event_at.checked_add(1) != Some(at)
            || !choice.prefix.contains(&row.original) || !seen.insert(row.original.object)
            || row.owner != choice.player || row.planned != expected
            || row.public_printing.is_some_and(|printing| {
                private_hand || row.destination == Zone::Library
                    || crate::card_def::CARD_DEFS.get(usize::from(printing)).is_none()
            })
            || !matches!(history.get(event_at), Some(E::ZoneChange {
                object, from: Zone::Library, to, ..
            }) if *object == row.original.object && *to == row.destination)
        {
            return Err("Curator move receipt no longer binds an original committed move".into());
        }
        actual.push(*row);
    }
    let mut departures = BTreeSet::<ObjectId>::new();
    for event in &history[phase_at + 1..finished_at] {
        if let E::ZoneChange { object, from: Zone::Library, .. } = event {
            if choice.prefix.iter().any(|binding| binding.object == *object) {
                departures.insert(*object);
            }
        }
    }
    if seen != departures {
        return Err("Curator completed outcome lost an actual native departure".into());
    }
    Ok(actual)
}
