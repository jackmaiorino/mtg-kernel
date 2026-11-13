//! Native Curator pile reveal and actual replacement-aware departures.
use crate::{
    effect::EffectObjectBinding,
    event::CommittedEvent as E,
    event_history_v1::EventKindV1,
    ids::{ObjectId, PlayerId},
    state::{GameState, Zone},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    hash::{Hash, Hasher},
};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct SampledHandArrivalsV1(BTreeSet<u64>);

impl SampledHandArrivalsV1 {
    pub(crate) fn hash_if_sampled(&self, state: &mut impl Hasher) {
        if !self.0.is_empty() {
            "curator-sampled-hand-arrivals/v1".hash(state);
            self.0.hash(state);
        }
    }
}

#[cfg(feature = "limited-fdn-fixtures")]
pub(crate) fn mark_sampled_hand_arrivals(
    state: &mut GameState,
    actor: PlayerId,
    opponent_slots: &[ObjectId],
) -> Result<(), String> {
    let arrivals = rows(state)
        .filter_map(|(_, row)| {
            let JournalV1::Moved { move_row, .. } = row else {
                return None;
            };
            let original = move_row.original;
            let live = state.objects.try_get(original.object)?;
            (move_row.owner == actor.opponent()
                && move_row.destination == Zone::Hand
                && move_row.public_printing.is_some()
                && live.zone == Zone::Hand
                && original.expected_zone_change_count.checked_add(1)
                    == Some(live.zone_change_count)
                && opponent_slots.contains(&original.object))
            .then_some(move_row.event_at)
        })
        .collect::<Vec<_>>();
    if !arrivals.is_empty() {
        // Validate the original native history before detaching any hand slot.
        crate::curator_public_v1::validate_native(state)?;
        state.curator_sampled_hand_arrivals_v1.0.extend(arrivals);
    }
    Ok(())
}

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
    Revealed {
        entry: u64,
        controller: PlayerId,
        shown: Vec<ShownV1>,
    },
    Moved {
        entry: u64,
        move_row: MovedV1,
    },
    ArrivalVisibility {
        entry: u64,
        event_at: u64,
        original: EffectObjectBinding,
        mask: u8,
        disclosed_printing: Option<u16>,
    },
}

fn rows(state: &GameState) -> impl Iterator<Item = (usize, &JournalV1)> {
    state
        .engine
        .event_history
        .scan_for_v1(&[EventKindV1::CuratorReceiptV1])
        .iter()
        .enumerate()
        .filter_map(|(at, event)| match event {
            E::CuratorReceiptV1 { journal } => Some((at, journal.as_ref())),
            _ => None,
        })
}

fn record(state: &mut GameState, journal: JournalV1) {
    state.engine.event_history.push(E::CuratorReceiptV1 {
        journal: Box::new(journal),
    });
}

pub(crate) fn record_reveal(
    state: &mut GameState,
    entry: u64,
    controller: PlayerId,
    shown_cards: impl IntoIterator<Item = EffectObjectBinding>,
) -> Result<(), String> {
    let shown = shown_cards
        .into_iter()
        .map(|original| {
            let live = state.objects.get(original.object);
            if live.owner != controller
                || live.zone != Zone::Library
                || live.zone_change_count != original.expected_zone_change_count
            {
                return Err("Curator reveal lost its original looked incarnation".into());
            }
            Ok(ShownV1 {
                original,
                printing: live.card_def,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    record(
        state,
        JournalV1::Revealed {
            entry,
            controller,
            shown,
        },
    );
    Ok(())
}

fn reveal(state: &GameState, entry: u64) -> Result<(usize, PlayerId, &[ShownV1]), String> {
    let mut receipts = rows(state).filter_map(|(at, journal)| match journal {
        JournalV1::Revealed {
            entry: owner,
            controller,
            shown,
        } if *owner == entry => Some((at, *controller, shown.as_slice())),
        _ => None,
    });
    let only = receipts
        .next()
        .ok_or("Curator face-up pile lacks its native reveal")?;
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
    let expected = choice
        .prefix
        .iter()
        .filter(|binding| !choice.down.contains(binding));
    if controller != choice.player
        || shown.iter().map(|row| &row.original).ne(expected)
        || shown.iter().any(|row| {
            crate::card_def::CARD_DEFS
                .get(usize::from(row.printing))
                .is_none()
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
    let Some(E::ZoneChange {
        object,
        from: Zone::Library,
        to,
        ..
    }) = state.engine.event_history.get(event_at)
    else {
        return Err("Curator move lacks an actual library departure".into());
    };
    let destination = *to;
    let live = state.objects.get(original.object);
    let (_, controller, shown) = reveal(state, cause.entry)?;
    let revealed = shown.iter().find(|row| row.original == original);
    if *object != original.object
        || controller != cause.controller
        || live.owner != controller
        || original.expected_zone != Zone::Library
        || live.zone != destination
        || original.expected_zone_change_count.checked_add(1) != Some(live.zone_change_count)
    {
        return Err("Curator native move changed the original library incarnation".into());
    }
    let revealed_hand = destination == Zone::Hand
        && revealed.is_some_and(|shown| shown.printing == live.card_def)
        && live.v4.face_down_v1.is_none();
    let public_destination = matches!(
        destination,
        Zone::Battlefield | Zone::Graveyard | Zone::Exile | Zone::Stack | Zone::Command
    ) && live.v4.face_down_v1.is_none();
    let mask = state
        .observation_journal
        .visibility_v2(event_at, original.object)
        .ok_or("Curator actual committed move lacks event-time visibility")?;
    let disclosed_printing =
        (mask == 3 && (revealed_hand || public_destination)).then_some(live.card_def);
    let event_at = u64::try_from(event_at).map_err(|_| "Curator history index width")?;
    let move_row = MovedV1 {
        event_at,
        original,
        owner: controller,
        planned: cause.planned,
        destination,
        public_printing: disclosed_printing,
    };
    record(
        state,
        JournalV1::ArrivalVisibility {
            entry: cause.entry,
            event_at,
            original,
            mask,
            disclosed_printing,
        },
    );
    record(
        state,
        JournalV1::Moved {
            entry: cause.entry,
            move_row,
        },
    );
    if revealed_hand && mask == 3 {
        // spellbench-public-reveal: the originally face-up pile arrived in hand.
        state.reveal_distinct_hand_card_with_meaning_v3(
            controller.opponent(),
            controller,
            original.object,
            crate::reveal_meaning_v3::RevealV3::CuratorPileArrival,
        )?;
    }
    Ok(())
}

fn validate_move_identity(
    state: &GameState,
    row: &MovedV1,
    event_at: usize,
    shown: Option<u16>,
    mask: u8,
    disclosed_printing: Option<u16>,
) -> Result<(), String> {
    let object = row.original.object;
    let arrival = row
        .original
        .expected_zone_change_count
        .checked_add(1)
        .ok_or("Curator arrival generation overflow")?;
    let live = state
        .objects
        .try_get(object)
        .ok_or("Curator arrival object absent")?;
    if live.owner != row.owner || live.zone_change_count < arrival {
        return Err("Curator arrival lost its original physical owner or incarnation".into());
    }
    let may_name = match row.destination {
        Zone::Library => false,
        Zone::Hand => shown.is_some(),
        _ => true,
    };
    if mask > 3
        || row.public_printing != disclosed_printing
        || state
            .observation_journal
            .visibility_v2(event_at, object)
            .is_some_and(|observed| observed != mask)
    {
        return Err("Curator arrival differs from its committed event-time visibility".into());
    }
    if let Some(printing) = row.public_printing {
        if !may_name
            || mask != 3
            || shown.is_some_and(|original| original != printing)
            || crate::card_def::CARD_DEFS
                .get(usize::from(printing))
                .is_none()
        {
            return Err(
                "Curator arrival name was not publicly disclosed by the native move".into(),
            );
        }
    }
    if live.zone_change_count == arrival {
        if live.zone != row.destination {
            return Err("Curator exact arrival changed its native destination".into());
        }
        let sampled_hand = row.destination == Zone::Hand
            && state
                .curator_sampled_hand_arrivals_v1
                .0
                .contains(&row.event_at);
        if may_name && live.v4.face_down_v1.is_none() && mask == 3 {
            let printing = row
                .public_printing
                .ok_or("Curator public arrival lost its original name")?;
            let original = crate::state::AbilitySourceContractV4 {
                source: object,
                card_def: printing,
                owner: row.owner,
                controller: row.owner,
                zone: row.destination,
                zone_change_count: arrival,
                attached_to: None,
            };
            if !sampled_hand
                && !crate::engine::source_card_def_matches(state, original, live.card_def)
            {
                return Err(
                    "Curator public arrival name differs from the exact committed incarnation"
                        .into(),
                );
            }
        }
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
        let JournalV1::Moved {
            entry,
            move_row: row,
        } = journal
        else {
            continue;
        };
        if *entry != choice.entry {
            continue;
        }
        let event_at = usize::try_from(row.event_at).map_err(|_| "Curator history index width")?;
        let shown_name = shown
            .iter()
            .find(|shown| shown.original == row.original)
            .map(|shown| shown.printing);
        let expected = if choice.down.contains(&row.original) == choose_down {
            Zone::Hand
        } else {
            Zone::Graveyard
        };
        if event_at <= phase_at
            || at >= finished_at
            || event_at.checked_add(2) != Some(at)
            || !choice.prefix.contains(&row.original)
            || !seen.insert(row.original.object)
            || row.owner != choice.player
            || row.planned != expected
            || !matches!(history.get(event_at), Some(E::ZoneChange {
                object, from: Zone::Library, to, ..
            }) if *object == row.original.object && *to == row.destination)
        {
            return Err("Curator move receipt no longer binds an original committed move".into());
        }
        let Some(E::CuratorReceiptV1 { journal }) = history.get(event_at + 1) else {
            return Err("Curator committed move lost serialized event-time visibility".into());
        };
        let JournalV1::ArrivalVisibility {
            entry,
            event_at: observed,
            original,
            mask,
            disclosed_printing,
        } = journal.as_ref()
        else {
            return Err("Curator committed move has a foreign visibility record".into());
        };
        if *entry != choice.entry || *observed != row.event_at || *original != row.original {
            return Err("Curator serialized visibility changed original move occurrence".into());
        }
        validate_move_identity(state, row, event_at, shown_name, *mask, *disclosed_printing)?;
        actual.push(*row);
    }
    let mut departures = BTreeSet::<ObjectId>::new();
    for event in &history[phase_at + 1..finished_at] {
        if let E::ZoneChange {
            object,
            from: Zone::Library,
            ..
        } = event
        {
            if choice
                .prefix
                .iter()
                .any(|binding| binding.object == *object)
            {
                departures.insert(*object);
            }
        }
    }
    if seen != departures {
        return Err("Curator completed outcome lost an actual native departure".into());
    }
    if rows(state)
        .filter(|(_, journal)| {
            matches!(journal,
        JournalV1::ArrivalVisibility { entry, .. } if *entry == choice.entry)
        })
        .count()
        != actual.len()
    {
        return Err("Curator native move has duplicated or detached visibility evidence".into());
    }
    Ok(actual)
}
