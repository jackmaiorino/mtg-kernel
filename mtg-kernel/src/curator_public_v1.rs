//! Actor-visible Curator printing, entry lifecycle and private choice payloads.
use crate::{
    card_def::{self, CardType, Subtype},
    curator_receipts_v1 as receipts,
    effect::{
        curator_v1 as native, EffectObjectBinding, EffectOp, EffectTargetCandidate,
        EffectTargetSelectionPurpose, PendingEffectChoice,
    },
    event::CommittedEvent as E,
    event_history_v1::EventKindV1,
    ids::PlayerId,
    optional_trigger_public_v1::{self as public, IdentityV1, ReferenceV1, RootV1},
    rl::PlayerSeatV1,
    state::{GameState, StackItem, Target, Zone},
    tinybones_public_v72::IncarnationV72,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub const PRINTING: u16 = 472;
pub const NAME: &str = "Curator of Destinies";
pub const SCHEMA: &str = "mtg-kernel/curator-public/v1";
const MEANING_SCHEMA: &str = "rules-meaning/curator-of-destinies/v1";

pub(crate) fn selected_id() -> Result<u16, String> {
    let triggers = crate::trigger::triggers_for(PRINTING);
    if card_def::card_id_by_name(NAME) != Some(PRINTING)
        || !native::definition(PRINTING)
        || triggers.len() != 1
        || triggers[0].condition != crate::trigger::TriggerCondition::Etb
        || triggers[0].home_zone != Zone::Battlefield
        || triggers[0].face_index != 0
        || triggers[0].intervening_if_kicked
        || triggers[0].intervening_if_controls_another_source_card
        || (triggers[0].effect)() != EffectOp::BindCuratorDestiniesV1
    {
        return Err("Curator requires its exact pure Limited printing and entry trigger".into());
    }
    Ok(PRINTING)
}

pub fn producer_sha256() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut hash = Sha256::new();
        for source in [
            SCHEMA,
            MEANING_SCHEMA,
            include_str!("curator_public_v1.rs"),
            include_str!("effect/curator_v1.rs"),
            include_str!("curator_receipts_v1.rs"),
            include_str!("event.rs"),
            include_str!("engine.rs"),
            include_str!("state.rs"),
            include_str!("reveal_meaning_v3.rs"),
            include_str!("../build.rs"),
        ] {
            hash.update((source.len() as u64).to_le_bytes());
            hash.update(source.as_bytes());
        }
        format!("{:x}", hash.finalize())
    })
    .clone()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeaningV1 {
    pub schema: String,
    pub producer_sha256: String,
    pub card_db_hash: u64,
    pub card_id: u16,
    pub card_type: CardType,
    pub subtype: Subtype,
    pub printed_generic: u8,
    pub printed_blue: u8,
    pub mana_value: u16,
    pub power: i16,
    pub toughness: i16,
    pub flying: bool,
    pub spell_cannot_be_countered: bool,
    pub private_look_up_to: u8,
    pub opponent_selects_face_up_or_face_down: bool,
    pub chosen_pile_to_hand_other_to_graveyard: bool,
}

pub fn printed_meaning() -> Result<MeaningV1, String> {
    Ok(MeaningV1 {
        schema: MEANING_SCHEMA.into(),
        producer_sha256: producer_sha256(),
        card_db_hash: card_def::KERNEL_CARDDB_HASH,
        card_id: selected_id()?,
        card_type: CardType::Creature,
        subtype: Subtype::Sphinx,
        printed_generic: 4,
        printed_blue: 2,
        mana_value: 6,
        power: 5,
        toughness: 5,
        flying: true,
        spell_cannot_be_countered: true,
        private_look_up_to: 5,
        opponent_selects_face_up_or_face_down: true,
        chosen_pile_to_hand_other_to_graveyard: true,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum PileV1 {
    FaceUp,
    FaceDown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NameCountV1 {
    pub card_id: u16,
    pub copies: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PilesV1 {
    pub face_down_count: u8,
    pub face_up: Vec<NameCountV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicMoveV1 {
    pub original: IncarnationV72,
    pub destination: Zone,
    pub card_id: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutcomeV1 {
    pub chosen: PileV1,
    pub shown_piles: PilesV1,
    pub actual_hand_arrivals: u8,
    pub revealed_hand_arrivals: Vec<NameCountV1>,
    pub public_moves: Vec<PublicMoveV1>,
    pub hidden_library_moves: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum StageV1 {
    Entered,
    Held {
        position: u32,
    },
    Published {
        stack_position: u32,
    },
    Resolving {
        shown_piles: Option<PilesV1>,
        chosen: Option<PileV1>,
    },
    Resolved(OutcomeV1),
    Countered,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntryV1 {
    pub source: RootV1,
    pub stage: StageV1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum CardsStageV1 {
    SetFaceDownPile,
    OrderGraveyard,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum ChoiceV1 {
    Cards {
        source: ReferenceV1,
        chooser: PlayerSeatV1,
        stage: CardsStageV1,
        looked: Vec<ReferenceV1>,
        selected: Vec<ReferenceV1>,
        legal: Vec<ReferenceV1>,
        required: u16,
        limit: u16,
        can_finish: bool,
    },
    Pile {
        source: ReferenceV1,
        chooser: PlayerSeatV1,
        piles: PilesV1,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactsV1 {
    pub schema: String,
    pub viewer: PlayerSeatV1,
    pub meaning: MeaningV1,
    pub entries: Vec<EntryV1>,
    pub chooser: Option<ChoiceV1>,
}

fn names(cards: impl Iterator<Item = u16>) -> Vec<NameCountV1> {
    let mut counts = BTreeMap::new();
    for card_id in cards {
        *counts.entry(card_id).or_insert(0) += 1;
    }
    counts
        .into_iter()
        .map(|(card_id, copies)| NameCountV1 { card_id, copies })
        .collect()
}

fn piles(choice: &native::ChoiceV1, shown: &[receipts::ShownV1]) -> PilesV1 {
    PilesV1 {
        face_down_count: u8::try_from(choice.down.len()).expect("Curator looks at up to five"),
        face_up: names(shown.iter().map(|row| row.printing)),
    }
}

fn pile(chose_down: bool) -> PileV1 {
    if chose_down {
        PileV1::FaceDown
    } else {
        PileV1::FaceUp
    }
}

#[derive(Default)]
struct Journal<'a> {
    entry_at: Option<usize>,
    collected_at: Option<usize>,
    published: Option<(usize, &'a StackItem)>,
    phases: Vec<(usize, &'a native::ChoiceV1)>,
    finished_at: Option<usize>,
}

fn occurrence_rows(state: &GameState) -> Result<BTreeMap<u64, Journal<'_>>, String> {
    let mut found = BTreeMap::<u64, Journal>::new();
    let mut auxiliary = Vec::new();
    for (at, event) in state
        .engine
        .event_history
        .scan_for_v1(&[
            EventKindV1::CuratorEnteredV1,
            EventKindV1::CuratorCollectedV1,
            EventKindV1::CuratorPublishedV1,
            EventKindV1::CuratorProgressV1,
            EventKindV1::CuratorFinishedV1,
            EventKindV1::CuratorReceiptV1,
        ])
        .iter()
        .enumerate()
    {
        let (E::CuratorEnteredV1 { .. }
        | E::CuratorCollectedV1 { .. }
        | E::CuratorPublishedV1 { .. }
        | E::CuratorProgressV1 { .. }
        | E::CuratorFinishedV1 { .. }
        | E::CuratorReceiptV1 { .. }) = event
        else {
            continue;
        };
        let duplicated = match event {
            E::CuratorEnteredV1 {
                entry_history_index: entry,
                ..
            } => found
                .entry(*entry)
                .or_default()
                .entry_at
                .replace(at)
                .is_some(),
            E::CuratorCollectedV1 {
                entry_history_index: entry,
            } => found
                .entry(*entry)
                .or_default()
                .collected_at
                .replace(at)
                .is_some(),
            E::CuratorPublishedV1 {
                entry_history_index: entry,
                item,
            } => found
                .entry(*entry)
                .or_default()
                .published
                .replace((at, item.as_ref()))
                .is_some(),
            E::CuratorFinishedV1 {
                entry_history_index: entry,
            } => found
                .entry(*entry)
                .or_default()
                .finished_at
                .replace(at)
                .is_some(),
            E::CuratorProgressV1 {
                entry_history_index: entry,
                choice,
            } => {
                found
                    .entry(*entry)
                    .or_default()
                    .phases
                    .push((at, choice.as_ref()));
                false
            }
            E::CuratorReceiptV1 { journal } => {
                auxiliary.push((at, journal.as_ref()));
                false
            }
            _ => false,
        };
        if duplicated {
            return Err("Curator lifecycle occurrence duplicated".into());
        }
    }
    for (at, row) in auxiliary {
        let ok = match row {
            receipts::JournalV1::Revealed { entry, .. } => found
                .get(entry)
                .and_then(|row| row.phases.get(1))
                .is_some_and(|(phase, _)| at.checked_add(1) == Some(*phase)),
            receipts::JournalV1::Moved { entry, .. }
            | receipts::JournalV1::ArrivalVisibility { entry, .. } => found
                .get(entry)
                .and_then(|row| row.phases.get(2).zip(row.finished_at))
                .is_some_and(|((phase, _), end)| *phase < at && at < end),
        };
        if !ok {
            return Err("Curator native receipt lost its original lifecycle stage".into());
        }
    }
    Ok(found)
}

fn validate_phases(row: &Journal<'_>, entry: u64, controller: PlayerId) -> Result<(), String> {
    let Some(&(first, original)) = row.phases.first() else {
        return Ok(());
    };
    let Some((published, _)) = row.published else {
        return Err("Curator progress lacks original trigger publication".into());
    };
    if first <= published || row.phases.len() > 3 || original.prefix.len() > 5 {
        return Err("Curator native progress changed range".into());
    }
    let mut prefix = BTreeSet::new();
    if original
        .prefix
        .iter()
        .any(|binding| binding.expected_zone != Zone::Library || !prefix.insert(binding.object))
    {
        return Err("Curator original library prefix is malformed".into());
    }
    for (phase, (at, choice)) in row.phases.iter().enumerate() {
        if choice.entry != entry
            || choice.player != controller
            || usize::from(choice.phase) != phase
            || choice.prefix != original.prefix
            || row.finished_at.is_some_and(|end| *at >= end)
            || (phase == 0 && !choice.down.is_empty())
            || (phase == 2) != choice.choose_down.is_some()
            || (phase == 2 && row.phases[1].1.down != choice.down)
            || choice
                .prefix
                .iter()
                .filter(|card| choice.down.contains(card))
                .ne(choice.down.iter())
        {
            return Err("Curator private native progress changed original pile".into());
        }
    }
    Ok(())
}

fn entries(state: &GameState, viewer: PlayerId) -> Result<Vec<EntryV1>, String> {
    let held = public::held(state);
    let mut pending_by_entry = BTreeMap::new();
    for (position, pending) in held.into_iter().enumerate() {
        let EffectOp::CuratorDestiniesV1 {
            entry_history_index: entry,
        } = pending.effect
        else {
            continue;
        };
        crate::engine::validate_pending_trigger_identity(state, pending)?;
        if pending_by_entry
            .insert(entry, (position, pending))
            .is_some()
        {
            return Err("Curator native held trigger duplicated".into());
        }
    }
    let mut published_live = BTreeMap::new();
    for (position, item) in state.stack.iter().enumerate() {
        if !native::selected(state, item) {
            continue;
        }
        let entry = native::validate_trigger(state, item)?;
        if published_live.insert(entry, (position, item)).is_some() {
            return Err("Curator native live publication duplicated".into());
        }
    }
    let resolving = state
        .engine
        .pending_effect
        .as_ref()
        .filter(|pending| native::pending_selected(state, pending));
    if let Some(pending) = resolving {
        native::validate(state, pending)?;
    }
    let mut result = Vec::new();
    for (entry, row) in occurrence_rows(state)? {
        let source = native::entry_source(state, entry)?;
        let entered = row.entry_at.ok_or("Curator native entry receipt missing")?;
        if row.collected_at.is_some_and(|at| at <= entered)
            || row.published.is_some_and(|(at, item)| {
                row.collected_at.is_none_or(|start| start >= at)
                    || native::validate_archived_trigger(state, item) != Ok(entry)
            })
            || row
                .finished_at
                .is_some_and(|at| row.published.is_none_or(|(start, _)| start >= at))
        {
            return Err("Curator native lifecycle reordered or publication changed".into());
        }
        validate_phases(&row, entry, source.controller)?;
        let shown_piles = row
            .phases
            .get(1)
            .map(|(_, choice)| {
                let (_, shown) =
                    receipts::validate_reveal(state, choice, row.finished_at.is_none())?;
                Ok::<_, String>(piles(choice, shown))
            })
            .transpose()?;
        let active = resolving.is_some_and(|pending| matches!(pending.resolving_item.inline_effect,
            Some(EffectOp::CuratorDestiniesV1 { entry_history_index }) if entry_history_index == entry));
        let live = published_live.remove(&entry);
        let held = pending_by_entry.remove(&entry);
        let stage = if let Some(finished) = row.finished_at {
            if live.is_some() || held.is_some() || active {
                return Err("Curator finished occurrence is still live".into());
            }
            match row.phases.as_slice() {
                [] => StageV1::Countered,
                [_, _, (phase_at, choice)] => {
                    let actual = receipts::resolved_moves(state, choice, *phase_at, finished)?;
                    let hand = actual
                        .iter()
                        .filter(|move_row| move_row.destination == Zone::Hand);
                    let public_moves = actual
                        .iter()
                        .filter(|move_row| {
                            !matches!(move_row.destination, Zone::Hand | Zone::Library)
                        })
                        .map(|move_row| PublicMoveV1 {
                            original: IncarnationV72 {
                                object: move_row.original.object.0,
                                generation: move_row.original.expected_zone_change_count,
                                owner: move_row.owner.into(),
                                controller: move_row.owner.into(),
                                zone: Zone::Library,
                            },
                            destination: move_row.destination,
                            card_id: move_row.public_printing,
                        })
                        .collect();
                    StageV1::Resolved(OutcomeV1 {
                        chosen: pile(choice.choose_down.unwrap()),
                        shown_piles: shown_piles.ok_or("Curator terminal lost its face-up pile")?,
                        actual_hand_arrivals: u8::try_from(hand.clone().count())
                            .map_err(|_| "Curator hand width")?,
                        revealed_hand_arrivals: names(hand.filter_map(|row| row.public_printing)),
                        public_moves,
                        hidden_library_moves: u8::try_from(
                            actual
                                .iter()
                                .filter(|row| row.destination == Zone::Library)
                                .count(),
                        )
                        .map_err(|_| "Curator hidden movement width")?,
                    })
                }
                _ => return Err("Curator terminal lost a required native progress stage".into()),
            }
        } else if active {
            StageV1::Resolving {
                shown_piles,
                chosen: row
                    .phases
                    .get(2)
                    .and_then(|(_, choice)| choice.choose_down)
                    .map(pile),
            }
        } else if let Some((position, _)) = live {
            if !row.phases.is_empty() || held.is_some() {
                return Err("Curator waiting publication carries impossible later progress".into());
            }
            StageV1::Published {
                stack_position: u32::try_from(position).map_err(|_| "Curator stack width")?,
            }
        } else if let Some((position, pending)) = held {
            if row.collected_at.is_none()
                || row.published.is_some()
                || pending.source_contract != Some(source)
            {
                return Err("Curator held trigger lost original entry/controller".into());
            }
            StageV1::Held {
                position: u32::try_from(position).map_err(|_| "Curator held width")?,
            }
        } else if row.collected_at.is_none() && row.published.is_none() {
            StageV1::Entered
        } else {
            return Err("Curator original occurrence lost its native lifetime".into());
        };
        let mut root = public::archived_root(state, entered, source, Some(viewer))?;
        root.original_source.identity = IdentityV1::Known { card_id: PRINTING };
        result.push(EntryV1 {
            source: root,
            stage,
        });
    }
    if !published_live.is_empty() || !pending_by_entry.is_empty() {
        return Err("Curator live trigger lacks its original entry".into());
    }
    Ok(result)
}

fn looked(
    state: &GameState,
    viewer: PlayerId,
    binding: EffectObjectBinding,
) -> Result<ReferenceV1, String> {
    let card = public::reference(state, binding.object, Some(viewer))
        .ok_or("Curator chooser lacks its genuine private look")?;
    if card.zone != Zone::Library
        || card.generation != binding.expected_zone_change_count
        || binding.expected_zone != card.zone
        || card.owner != viewer.into()
    {
        return Err("Curator chooser's original private incarnation changed".into());
    }
    Ok(card)
}

fn menu_cards(
    state: &GameState,
    viewer: PlayerId,
    cards: &[EffectTargetCandidate],
) -> Result<Vec<ReferenceV1>, String> {
    cards
        .iter()
        .map(|candidate| {
            let binding = candidate
                .expected_object
                .ok_or("Curator private action lacks native binding")?;
            if candidate.target != Target::Object(binding.object) {
                return Err("Curator action changed original library target".into());
            }
            looked(state, viewer, binding)
        })
        .collect()
}

pub(crate) fn capture(state: &GameState, viewer: PlayerId) -> Result<FactsV1, String> {
    if !state.curator_unordered_reveal_v1 {
        return Err("Curator owner requires prospective unordered-pile state".into());
    }
    let entries = entries(state, viewer)?;
    let chooser = state
        .engine
        .pending_effect
        .as_ref()
        .filter(|pending| native::pending_selected(state, pending))
        .and_then(|pending| {
            let choice = pending.choice.as_ref()?;
            Some((pending, choice))
        })
        .map(|(pending, choice)| -> Result<Option<ChoiceV1>, String> {
            let source = pending
                .resolving_item
                .v4
                .ability_source_contract
                .ok_or("Curator choice lost original public entry source")?;
            let mut source = public::root(state, source, Some(viewer))?.original_source;
            source.identity = IdentityV1::Known { card_id: PRINTING };
            match choice {
                PendingEffectChoice::SelectTargets {
                    player,
                    purpose: EffectTargetSelectionPurpose::CuratorCardsV1 { choice },
                    selected,
                    legal,
                    min_targets,
                    max_targets,
                    ordered,
                    ..
                } if *player == viewer => {
                    let looked = if *ordered {
                        vec![]
                    } else {
                        choice
                            .prefix
                            .iter()
                            .map(|&binding| looked(state, viewer, binding))
                            .collect::<Result<_, _>>()?
                    };
                    Ok(Some(ChoiceV1::Cards {
                        source,
                        chooser: viewer.into(),
                        looked,
                        stage: if *ordered {
                            CardsStageV1::OrderGraveyard
                        } else {
                            CardsStageV1::SetFaceDownPile
                        },
                        selected: menu_cards(state, viewer, selected)?,
                        legal: menu_cards(state, viewer, legal)?,
                        required: *min_targets,
                        limit: *max_targets,
                        can_finish: selected.len() >= usize::from(*min_targets),
                    }))
                }
                PendingEffectChoice::ChooseBoolean {
                    player,
                    purpose:
                        crate::effect::EffectBooleanChoicePurpose::CuratorPileV1 {
                            face_down_count,
                            face_up_count,
                        },
                    ..
                } if *player == viewer => {
                    let choice = native::prepare_pile_answer(
                        state,
                        pending,
                        *face_down_count,
                        *face_up_count,
                    )?;
                    let (_, shown) = receipts::validate_reveal(state, &choice, true)?;
                    Ok(Some(ChoiceV1::Pile {
                        source,
                        chooser: viewer.into(),
                        piles: piles(&choice, shown),
                    }))
                }
                _ => Ok(None),
            }
        })
        .transpose()?
        .flatten();
    Ok(FactsV1 {
        schema: SCHEMA.into(),
        viewer: viewer.into(),
        meaning: printed_meaning()?,
        entries,
        chooser,
    })
}

pub(crate) fn validate_native(state: &GameState) -> Result<(), String> {
    if !state.curator_unordered_reveal_v1 {
        return Err("Curator owner requires prospective unordered-pile state".into());
    }
    entries(state, PlayerId::P0)?;
    Ok(())
}
