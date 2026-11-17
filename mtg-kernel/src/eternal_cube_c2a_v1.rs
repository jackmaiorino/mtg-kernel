//! Card programs for the eternal catalog's Vintage Cube wave C2a
//! (`docs/design/eternal_formats_support_v1.md`): lands with an entry choice or a typed tap.
//!
//! `eternal_cards_v1` forwards this module's effect leaves here, and the
//! recipe tables of `build_eternal_c2a_v1.rs` name its programs. Every
//! table is keyed by a printed name that occurs only in the eternal registry,
//! so the Pauper, FDN and Standard catalogs never reach it. Behavior was read
//! from each card's current Oracle text and, where present, its XMage source.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

use crate::card_def::{Subtype, TargetSpec, CARD_DEFS};
use crate::effect::{EffectContinuation, EffectOp, ExecCtx};
use crate::engine::Decision;
use crate::ids::ObjectId;
use crate::mana::ManaColor;
use crate::state::{GameState, ObjectLinkV4, Zone};
use crate::trigger::TriggeredAbilityDef;
use serde::{Deserialize, Serialize};

// ---- Shocklands and Multiversal Passage ----------------------------------------------
//
// "As this land enters, you may pay 2 life. If you don't, it enters tapped."
// reuses the as-enters life payment that Soporific Springs introduced
// (`eternal_cards_v1::e3`, its `springs_entering` list, decision and answer),
// parameterized by the life it costs. Every other piece of these lands (their
// basic land types, their two mana abilities) is registry data.

/// The ten shocklands.
pub(crate) const SHOCKLANDS: [&str; 10] = [
    "Blood Crypt",
    "Breeding Pool",
    "Godless Shrine",
    "Hallowed Fountain",
    "Overgrown Tomb",
    "Sacred Foundry",
    "Steam Vents",
    "Stomping Ground",
    "Temple Garden",
    "Watery Grave",
];

pub(crate) const MULTIVERSAL_PASSAGE: &str = "Multiversal Passage";

/// The basic land types in the order of Multiversal Passage's five-way
/// choice (WUBRG), with the color of the mana ability each one gives.
const BASIC_TYPES: [(Subtype, ManaColor); 5] = [
    (Subtype::Plains, ManaColor::W),
    (Subtype::Island, ManaColor::U),
    (Subtype::Swamp, ManaColor::B),
    (Subtype::Mountain, ManaColor::R),
    (Subtype::Forest, ManaColor::G),
];

/// The life an as-enters "you may pay N life. If you don't, it enters
/// tapped" costs for a C2a land, `None` for every other card.
pub(crate) fn as_enters_life_cost(name: &str) -> Option<i32> {
    (name == MULTIVERSAL_PASSAGE || SHOCKLANDS.contains(&name)).then_some(2)
}

/// Per-game state for this module's cards, kept inside
/// `eternal_cards_v1::EternalStateV1`. Absent from every snapshot until one
/// of them needs it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct C2aStateV1 {
    /// Multiversal Passage incarnations that have entered and still owe
    /// their basic land type choice.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub passage_pending: Vec<ObjectLinkV4>,
    /// The basic land type chosen as each Multiversal Passage incarnation
    /// entered, with the timestamp of the choice. It applies for as long as
    /// that incarnation is on the battlefield.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub passage_types: Vec<PassageTypeV1>,
}

impl C2aStateV1 {
    pub(crate) fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// One Multiversal Passage's chosen basic land type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PassageTypeV1 {
    pub land: ObjectLinkV4,
    pub subtype: Subtype,
    pub timestamp: u64,
}

fn read_state(state: &GameState) -> Option<&C2aStateV1> {
    state.eternal_v1.as_ref().map(|eternal| &eternal.c2a)
}

fn c2a_state(state: &mut GameState) -> &mut C2aStateV1 {
    &mut state.eternal_v1.get_or_insert_with(Default::default).c2a
}

fn link_live(state: &GameState, link: ObjectLinkV4) -> bool {
    state.objects.try_get(link.object).is_some_and(|live| {
        live.zone == Zone::Battlefield && live.zone_change_count == link.zone_change_count
    })
}

/// Entry hook, called for every permanent that enters the battlefield
/// (however it got there): a shockland starts its life payment, a
/// Multiversal Passage its land type choice.
pub(crate) fn apply_entry(state: &mut GameState, object: ObjectId) {
    let live = state.objects.get(object);
    if live.zone != Zone::Battlefield || live.v4.face_index != 0 {
        return;
    }
    let Some(def) = CARD_DEFS.get(live.card_def as usize) else {
        return;
    };
    // A land that a Moon effect already makes a Mountain or an Island as it
    // enters has lost its abilities, the entry ability among them (614.12):
    // it enters untapped with no choice.
    if crate::eternal_legacy_e8a_v1::statics::removal_timestamp(state, object).is_some() {
        return;
    }
    if def.name == MULTIVERSAL_PASSAGE {
        let entering = ObjectLinkV4 {
            object,
            zone_change_count: live.zone_change_count,
        };
        let pending = &mut c2a_state(state).passage_pending;
        if !pending.contains(&entering) {
            pending.push(entering);
        }
    } else if SHOCKLANDS.contains(&def.name) {
        crate::eternal_cards_v1::e3::note_life_entry(state, object);
    }
}

fn live_passage_pending(state: &GameState) -> Option<ObjectLinkV4> {
    read_state(state)?
        .passage_pending
        .iter()
        .copied()
        .find(|&entering| link_live(state, entering))
}

/// "As this land enters, choose a basic land type": the pending choice, one
/// option per basic land type in WUBRG order.
pub(crate) fn entry_choice_decision(state: &GameState) -> Option<Decision> {
    let entering = live_passage_pending(state)?;
    Some(Decision::ChooseEffectOption {
        player: state.objects.get(entering.object).controller,
        source: entering.object,
        option_count: u16::try_from(BASIC_TYPES.len()).unwrap_or(u16::MAX),
    })
}

/// Answers the pending Multiversal Passage land type, then starts its life
/// payment. `Ok(false)` when no choice is pending.
pub(crate) fn answer_entry_choice(state: &mut GameState, option: u16) -> Result<bool, String> {
    let Some(entering) = live_passage_pending(state) else {
        return Ok(false);
    };
    let (subtype, _) = *BASIC_TYPES
        .get(usize::from(option))
        .ok_or("Multiversal Passage's land type option is out of range")?;
    let timestamp = crate::engine::next_timestamp(state);
    let owed = c2a_state(state);
    owed.passage_pending.retain(|&other| other != entering);
    owed.passage_types.push(PassageTypeV1 {
        land: entering,
        subtype,
        timestamp,
    });
    crate::eternal_cards_v1::e3::note_life_entry(state, entering.object);
    Ok(true)
}

/// Multiversal Passage: "This land is the chosen type." The latest land type
/// change on `object`, its timestamp and the color of the mana ability the
/// new basic land type gives. Wave E8a's land-type change applies it with
/// Harbinger of the Seas and the moons (305.7).
pub(crate) fn land_type_setter(
    state: &GameState,
    object: ObjectId,
) -> Option<(u64, Subtype, ManaColor)> {
    let c2a = read_state(state)?;
    if c2a.passage_types.is_empty() {
        return None;
    }
    let live = state.objects.try_get(object)?;
    if live.zone != Zone::Battlefield {
        return None;
    }
    let record = c2a
        .passage_types
        .iter()
        .filter(|record| {
            record.land.object == object && record.land.zone_change_count == live.zone_change_count
        })
        .max_by_key(|record| record.timestamp)?;
    let (_, color) = BASIC_TYPES
        .iter()
        .find(|(subtype, _)| *subtype == record.subtype)?;
    Some((record.timestamp, record.subtype, *color))
}

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::C2aV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum C2aOpV1 {}

impl C2aOpV1 {
    /// Whether the leaf may stop for a player's choice (or a draw that a
    /// replacement can interrupt), so it runs in the resumable interpreter.
    pub(crate) fn contains_player_choice(&self) -> bool {
        match *self {}
    }
}

// ---- Triggered abilities ------------------------------------------------------------

/// Triggered abilities of this module's cards, falling through to wave
/// C2b.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    crate::eternal_cube_c2b_v1::triggers_for(name)
}

/// Target specs of this module's printed and queued triggered abilities.
pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    delayed_effect_matches(name, effect).then_some(TargetSpec::None)
}

/// Queued triggers owned by this module's cards.
pub(crate) fn delayed_effect_matches(_name: &str, _effect: &EffectOp) -> bool {
    false
}

// ---- Execution ----------------------------------------------------------------------

/// Runs one of this module's leaves in synchronous execution.
pub(crate) fn execute(op: &C2aOpV1, _ctx: &ExecCtx, _state: &mut GameState) {
    match *op {}
}

/// Runs one of this module's leaves in the resumable interpreter.
pub(crate) fn drive(
    _state: &mut GameState,
    _continuation: &mut EffectContinuation,
    op: &C2aOpV1,
    _path: Vec<u16>,
) -> Result<bool, String> {
    match *op {}
}
