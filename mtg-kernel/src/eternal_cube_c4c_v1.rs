//! Card programs for the eternal catalog's Vintage Cube wave C4c
//! (`docs/design/eternal_formats_support_v1.md`): Equipment and Vehicles.
//!
//! `eternal_cards_v1` forwards this module's effect leaves here, and the
//! recipe tables of `build_eternal_c4c_v1.rs` name its programs. Every
//! table is keyed by a printed name that occurs only in the eternal registry,
//! so the Pauper, FDN and Standard catalogs never reach it. Behavior was read
//! from each card's current Oracle text and, where present, its XMage source.
//!
//! Where each card lives:
//!
//! - Equipment statics are `EquipmentDef` rows (wave E9a's `equipment_for`):
//!   Skullclamp +1/-1, Sword of the Meek +1/+2, Kaldra Compleat +5/+5 with
//!   four keywords, Iron Man Armor +2/+1 flying, Astrologian's Planisphere's
//!   Wizard type. Nettlecyst's "+1/+1 for each artifact and/or enchantment
//!   you control" is `controlled_boost` (the engine calls it with Lion Sash's
//!   per-counter bonus).
//! - Living weapon (Nettlecyst, Kaldra Compleat) and For Mirrodin! (Glimmer
//!   Lens) are the entry trigger of Black Mage's Rod's job select,
//!   `EffectOp::CreateTokenAndAttachSource`, with this wave's 0/0 black
//!   Phyrexian Germ and 2/2 red Rebel token rows. Iron Man Armor's entry
//!   attach is Mirran Banesplitter's.
//! - Skullclamp's death trigger needs the Equipment's host as it died:
//!   `note_departure` (called from the zone-change choke point before the
//!   move) records every Skullclamp attached to a creature that leaves, and
//!   `delayed_triggers` turns a record into a trigger when the creature then
//!   goes to the graveyard (leaves-the-battlefield look-back, 603.10a). A
//!   Skullclamp that leaves first in the same collection records its host
//!   itself, so both leaving together still draws.
//! - Umezawa's Jitte's and Kaldra Compleat's combat damage triggers are queued
//!   by `note_combat_damage` (the engine calls it once per combat damage wave,
//!   as it does Psychic Frog's), one Jitte trigger per damage wave of its
//!   equipped creature and one Kaldra trigger per creature that creature
//!   damaged. The Jitte's three modes are three activated abilities, each
//!   removing one charge counter, as the research map directs.
//! - Glimmer Lens, Astrologian's Planisphere and Sword of the Meek read the
//!   collection's events in `delayed_triggers`: the attackers declared, the
//!   spells cast and cards drawn (with the draw ordinal of
//!   `trigger::draws_this_turn_snapshot`), the creatures that entered. A
//!   declaration only logs every attacker while something observes attacks
//!   (`observes_attacks`). The Planisphere's and Kaldra Compleat's abilities
//!   belong to the equipped creature, so those triggers have the creature as
//!   their source (the Ring's pattern in wave E8b) and are checked against
//!   `continuous_characteristics_v1::grant_survives`.
//! - Sword of the Meek's trigger works from the graveyard: its source is the
//!   graveyard incarnation, and the "may return it, then attach it" is a
//!   choice the resolution asks only while the Sword is still there.
//! - Smuggler's Copter's attack trigger is a printed one; its block trigger
//!   is placed by `note_blockers_declared` (declaring blockers commits no
//!   event). Esika's Chariot's copy is
//!   `standard_cards_v1::create_token_copy` over this wave's "token you
//!   control" target filter (`targets`).
//! - Iron Man Armor's animation is `CardDef::animation` (Mishra's Factory's
//!   mechanism, row in `build_support/standard_lands_v1.rs`); its "+1/+1 for
//!   each artifact you control" is `controlled_boost` and its being unable to
//!   stay attached while it is a creature (301.5c) is
//!   `unattach_animated_equipment`, run with the other attachment checks of
//!   the state-based actions.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

use crate::card_def::{CardType, DynamicValueDef, TargetSpec, CARD_DEFS};
use crate::effect::{EffectContinuation, EffectFrame, EffectOp, ExecCtx, ObjectRef, PlayerRef};
use crate::eternal_cards_v1::EternalOpV1;
use crate::event::{self, CommittedEvent, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::standard_cards_v1::{StandardOpV1, StandardTargetV1};
use crate::state::{AbilitySourceContractV4, GameState, ObjectLinkV4, Target, Zone};
use crate::trigger::{PendingTrigger, TriggerCondition, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};

pub mod targets;

pub(crate) const SKULLCLAMP: &str = "Skullclamp";
pub(crate) const SWORD_OF_THE_MEEK: &str = "Sword of the Meek";
pub(crate) const UMEZAWAS_JITTE: &str = "Umezawa's Jitte";
pub(crate) const NETTLECYST: &str = "Nettlecyst";
pub(crate) const KALDRA_COMPLEAT: &str = "Kaldra Compleat";
pub(crate) const GLIMMER_LENS: &str = "Glimmer Lens";
pub(crate) const IRON_MAN_ARMOR: &str = "Iron Man Armor";
pub(crate) const ASTROLOGIANS_PLANISPHERE: &str = "Astrologian's Planisphere";
pub(crate) const SMUGGLERS_COPTER: &str = "Smuggler's Copter";
pub(crate) const ESIKAS_CHARIOT: &str = "Esika's Chariot";

const GERM_TOKEN: &str = "Phyrexian Germ Token";
const REBEL_TOKEN: &str = "Rebel Token";
const GREEN_CAT_TOKEN: &str = "Green Cat Token";

/// One triggered ability a hook queued, waiting for the next trigger
/// collection.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueuedTriggerV1 {
    pub source: AbilitySourceContractV4,
    pub controller: PlayerId,
    pub kind: QueuedKindV1,
}

/// Which ability a queued trigger is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum QueuedKindV1 {
    /// Umezawa's Jitte: two charge counters on the Jitte.
    JitteCountersV1,
    /// Kaldra Compleat's granted trigger: exile this creature it damaged.
    KaldraExileV1 { victim: ObjectLinkV4 },
}

/// A Skullclamp attached to a creature that is leaving the battlefield.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClampRecordV1 {
    /// The equipped creature's incarnation before it leaves.
    pub host: ObjectLinkV4,
    /// The Skullclamp as it was, with its controller.
    pub clamp: AbilitySourceContractV4,
    /// Whether the Skullclamp itself left first: it then only draws if the
    /// host leaves for the graveyard in the same collection.
    pub clamp_left: bool,
}

/// Per-game state for this module's cards, kept inside
/// `eternal_cards_v1::EternalStateV1`. Default (and absent from snapshots)
/// until a card of this wave needs it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct C4cStateV1 {
    /// Triggers detected by hooks, waiting for the next trigger collection.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub queued_triggers: Vec<QueuedTriggerV1>,
    /// Skullclamps whose host is leaving the battlefield.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub clamp_records: Vec<ClampRecordV1>,
}

impl C4cStateV1 {
    pub(crate) fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

pub(crate) fn c4c_state(state: &mut GameState) -> &mut C4cStateV1 {
    &mut state.eternal_v1.get_or_insert_with(Default::default).c4c
}

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::C4cV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum C4cOpV1 {
    /// Kaldra Compleat: "exile that creature", the creature its host damaged,
    /// if it is still the permanent that was damaged.
    KaldraExileV1 {
        victim: ObjectId,
        zone_change_count: u32,
    },
    /// Astrologian's Planisphere: "put a +1/+1 counter on this creature" for
    /// the equipped creature the granted trigger belongs to.
    PlanisphereCounterV1,
    /// Sword of the Meek: "you may return this card from your graveyard to
    /// the battlefield, then attach it to that creature". Asks the controller
    /// only while the Sword is still in the graveyard it triggered from.
    SwordMayReturnV1 {
        creature: ObjectId,
        zone_change_count: u32,
    },
    /// The accepted half of Sword of the Meek's ability.
    SwordReturnV1 {
        creature: ObjectId,
        zone_change_count: u32,
    },
    /// Esika's Chariot: "create a token that's a copy of target token you
    /// control" for target 0.
    ChariotCopyV1,
    /// Iron Man Armor's "Equip {2}": attach to target 0, unless the Armor is a
    /// creature (301.5c), when the ability does nothing.
    IronManEquipV1,
}

impl C4cOpV1 {
    /// Whether the leaf may stop for a player's choice (or a draw that a
    /// replacement can interrupt), so it runs in the resumable interpreter.
    pub(crate) fn contains_player_choice(&self) -> bool {
        matches!(self, Self::SwordMayReturnV1 { .. })
    }
}

fn c4c(op: C4cOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::C4cV1(op)))
}

fn token_id(name: &str) -> u16 {
    crate::card_def::card_id_by_name(name).unwrap_or_else(|| panic!("{name} in CARD_DEFS"))
}

// ---- Programs -----------------------------------------------------------------------

/// Nettlecyst's and Kaldra Compleat's living weapon: "When this Equipment
/// enters, create a 0/0 black Phyrexian Germ creature token, then attach this
/// to it." Created and attached in one resolution, so no state-based action
/// sees the 0/0 Germ unequipped.
fn living_weapon() -> EffectOp {
    EffectOp::CreateTokenAndAttachSource {
        token_def: token_id(GERM_TOKEN),
    }
}

/// Glimmer Lens's For Mirrodin!: "When this Equipment enters, create a 2/2
/// red Rebel creature token, then attach this to it."
fn for_mirrodin() -> EffectOp {
    EffectOp::CreateTokenAndAttachSource {
        token_def: token_id(REBEL_TOKEN),
    }
}

/// Iron Man Armor: "When this Equipment enters, attach it to target creature
/// you control" (Mirran Banesplitter's).
fn attach_to_target() -> EffectOp {
    EffectOp::AttachSourceToTarget {
        object: ObjectRef::Target(0),
    }
}

/// Skullclamp: "Whenever equipped creature dies, draw two cards."
fn skullclamp_draw() -> EffectOp {
    EffectOp::Sequence(vec![EffectOp::DrawCards {
        player: PlayerRef::Controller,
        count: 2,
    }])
}

/// Glimmer Lens: "Whenever equipped creature and at least one other creature
/// attack, draw a card."
fn lens_draw() -> EffectOp {
    EffectOp::Sequence(vec![EffectOp::DrawCards {
        player: PlayerRef::Controller,
        count: 1,
    }])
}

/// Umezawa's Jitte: "put two charge counters on Umezawa's Jitte" (Blast
/// Zone's counter leaf, which reads the live ability source).
fn jitte_counters() -> EffectOp {
    crate::eternal_legacy_e7a_v1::blast_zone_charge_2()
}

/// Umezawa's Jitte's first mode: "Equipped creature gets +2/+2 until end of
/// turn." Read at resolution from the Jitte's attachment (Adventuring Gear's
/// boost leaf).
pub fn jitte_boost() -> EffectOp {
    EffectOp::BoostAttachedCreatureUntilEndOfTurn {
        power: 2,
        toughness: 2,
    }
}

/// Smuggler's Copter: "you may draw a card. If you do, discard a card"
/// (Moon-Circuit Hacker's shape: decline first).
fn copter_loot() -> EffectOp {
    EffectOp::Choice {
        controller: PlayerRef::Controller,
        options: vec![
            EffectOp::Sequence(Vec::new()),
            EffectOp::Sequence(vec![
                EffectOp::DrawCards {
                    player: PlayerRef::Controller,
                    count: 1,
                },
                EffectOp::DiscardCards {
                    player: PlayerRef::Controller,
                    count: 1,
                },
            ]),
        ],
    }
}

/// Esika's Chariot: "When Esika's Chariot enters, create two 2/2 green Cat
/// creature tokens."
fn chariot_cats() -> EffectOp {
    EffectOp::CreateTokensDynamic {
        token_def: token_id(GREEN_CAT_TOKEN),
        controller: PlayerRef::Controller,
        count: DynamicValueDef::Fixed(2),
        tapped: false,
    }
}

/// Esika's Chariot: "Whenever Esika's Chariot attacks, create a token that's
/// a copy of target token you control."
fn chariot_copy() -> EffectOp {
    c4c(C4cOpV1::ChariotCopyV1)
}

/// Iron Man Armor's "Equip {2}" (702.6a), written as its own program because
/// the Armor can be a creature, which then may name itself as the target
/// creature, and 301.5c keeps an Equipment that is a creature from equipping.
pub fn iron_man_equip() -> EffectOp {
    c4c(C4cOpV1::IronManEquipV1)
}

/// Astrologian's Planisphere's granted "put a +1/+1 counter on this
/// creature".
fn planisphere_counter() -> EffectOp {
    c4c(C4cOpV1::PlanisphereCounterV1)
}

fn sword_may_return(creature: ObjectLinkV4) -> EffectOp {
    c4c(C4cOpV1::SwordMayReturnV1 {
        creature: creature.object,
        zone_change_count: creature.zone_change_count,
    })
}

fn queued_effect(kind: QueuedKindV1) -> EffectOp {
    match kind {
        QueuedKindV1::JitteCountersV1 => jitte_counters(),
        QueuedKindV1::KaldraExileV1 { victim } => c4c(C4cOpV1::KaldraExileV1 {
            victim: victim.object,
            zone_change_count: victim.zone_change_count,
        }),
    }
}

// ---- Helpers ------------------------------------------------------------------------

fn def_name(state: &GameState, object: ObjectId) -> Option<&'static str> {
    let live = state.objects.try_get(object)?;
    CARD_DEFS
        .get(usize::from(live.card_def))
        .map(|def| def.name)
}

/// Whether `object` is a battlefield permanent named `name` whose printed
/// abilities apply.
fn active_named(state: &GameState, object: ObjectId, name: &str) -> bool {
    state
        .objects
        .try_get(object)
        .is_some_and(|live| live.zone == Zone::Battlefield)
        && def_name(state, object) == Some(name)
        && crate::continuous_characteristics_v1::printed_abilities_active(state, object)
}

fn link_of(state: &GameState, object: ObjectId) -> ObjectLinkV4 {
    ObjectLinkV4 {
        object,
        zone_change_count: state.objects.get(object).zone_change_count,
    }
}

/// The active Equipment named `name` attached to exactly `host`.
fn attached_equipment(state: &GameState, host: ObjectId, name: &str) -> Vec<ObjectId> {
    let Some(host_live) = state.objects.try_get(host) else {
        return Vec::new();
    };
    if host_live.attachments.is_empty() {
        return Vec::new();
    }
    let host_link = link_of(state, host);
    host_live
        .attachments
        .iter()
        .copied()
        .filter(|&equipment| {
            active_named(state, equipment, name)
                && state.objects.get(equipment).v4.attached_to == Some(host_link)
        })
        .collect()
}

/// Every battlefield permanent `player` controls.
fn controlled(state: &GameState, player: PlayerId) -> Vec<ObjectId> {
    [PlayerId::P0, PlayerId::P1]
        .into_iter()
        .flat_map(|seat| state.players[seat.index()].battlefield.iter().copied())
        .filter(|&object| {
            let live = state.objects.get(object);
            live.zone == Zone::Battlefield && live.controller == player
        })
        .collect()
}

fn is_creature(state: &GameState, object: ObjectId) -> bool {
    crate::engine::object_has_type(state, object, CardType::Creature)
}

fn push_queued(state: &mut GameState, source: AbilitySourceContractV4, kind: QueuedKindV1) {
    let controller = source.controller;
    c4c_state(state).queued_triggers.push(QueuedTriggerV1 {
        source,
        controller,
        kind,
    });
}

// ---- Hooks: engine ------------------------------------------------------------------

/// A permanent is moving from `from` to `to`, called by the zone-change choke
/// point before the move. Records the Skullclamps that equip a creature
/// leaving the battlefield (the attachment ends with the move), and a
/// Skullclamp that leaves while it equips a creature.
pub(crate) fn note_departure(state: &mut GameState, object: ObjectId, from: Zone, to: Zone) {
    if from != Zone::Battlefield || to == Zone::Battlefield {
        return;
    }
    let Some(live) = state.objects.try_get(object) else {
        return;
    };
    if live.attachments.is_empty() && live.v4.attached_to.is_none() {
        return;
    }
    let mut records = Vec::new();
    if let Some(host) = live.v4.attached_to {
        // A Skullclamp leaving while it equips a creature that is still there.
        let host_there = state.objects.try_get(host.object).is_some_and(|host_live| {
            host_live.zone == Zone::Battlefield
                && host_live.zone_change_count == host.zone_change_count
        });
        if host_there && active_named(state, object, SKULLCLAMP) && is_creature(state, host.object)
        {
            records.push(ClampRecordV1 {
                host,
                clamp: AbilitySourceContractV4::capture(state, object),
                clamp_left: true,
            });
        }
    }
    if !live.attachments.is_empty() && is_creature(state, object) {
        let host = link_of(state, object);
        for clamp in attached_equipment(state, object, SKULLCLAMP) {
            records.push(ClampRecordV1 {
                host,
                clamp: AbilitySourceContractV4::capture(state, clamp),
                clamp_left: false,
            });
        }
    }
    if records.is_empty() {
        return;
    }
    let held = &mut c4c_state(state).clamp_records;
    for record in records {
        held.retain(|old| !(old.host == record.host && old.clamp.source == record.clamp.source));
        held.push(record);
    }
}

/// A combat damage wave was dealt (`engine::finish_combat_damage_events_v38`):
/// Umezawa's Jitte's "Whenever equipped creature deals combat damage" (once
/// per equipped creature per wave, however many creatures and players it
/// damaged) and Kaldra Compleat's "Whenever this creature deals combat damage
/// to a creature" (once per creature damaged).
pub(crate) fn note_combat_damage(state: &mut GameState, events: &[CommittedEvent]) {
    let mut dealers: Vec<ObjectId> = Vec::new();
    let mut damaged: Vec<(ObjectId, ObjectId)> = Vec::new();
    for event in events {
        let CommittedEvent::Damage {
            source,
            target,
            amount,
        } = *event
        else {
            continue;
        };
        if amount <= 0 {
            continue;
        }
        if !dealers.contains(&source) {
            dealers.push(source);
        }
        if let Target::Object(victim) = target {
            if is_creature(state, victim) && !damaged.contains(&(source, victim)) {
                damaged.push((source, victim));
            }
        }
    }
    let mut queued: Vec<(AbilitySourceContractV4, QueuedKindV1)> = Vec::new();
    for dealer in dealers {
        let Some(live) = state.objects.try_get(dealer) else {
            continue;
        };
        if live.zone != Zone::Battlefield || live.attachments.is_empty() {
            continue;
        }
        for jitte in attached_equipment(state, dealer, UMEZAWAS_JITTE) {
            queued.push((
                AbilitySourceContractV4::capture(state, jitte),
                QueuedKindV1::JitteCountersV1,
            ));
        }
        let granted = attached_equipment(state, dealer, KALDRA_COMPLEAT)
            .into_iter()
            .any(|kaldra| {
                crate::continuous_characteristics_v1::grant_survives(
                    state,
                    dealer,
                    state.objects.get(kaldra).v4.layer_timestamp.unwrap_or(0),
                )
            });
        if !granted {
            continue;
        }
        for &(source, victim) in &damaged {
            if source == dealer {
                queued.push((
                    AbilitySourceContractV4::capture(state, dealer),
                    QueuedKindV1::KaldraExileV1 {
                        victim: link_of(state, victim),
                    },
                ));
            }
        }
    }
    for (source, kind) in queued {
        push_queued(state, source, kind);
    }
}

/// Blockers were declared: Smuggler's Copter's "Whenever this Vehicle ...
/// blocks". Declaring blockers commits no event, and a trigger collection
/// without events does not read this module's queue, so the trigger is placed
/// directly with the pending triggers (the Ring's block trigger, wave E8b).
pub(crate) fn note_blockers_declared(state: &mut GameState) {
    let blockers: Vec<ObjectId> = state
        .engine
        .combat
        .blocked_by
        .iter()
        .flat_map(|(_, blockers)| blockers.iter().copied())
        .collect();
    for blocker in blockers {
        if active_named(state, blocker, SMUGGLERS_COPTER) {
            let source = AbilitySourceContractV4::capture(state, blocker);
            let pending =
                crate::eternal_cards_v1::delayed_trigger(source, source.controller, copter_loot());
            state.engine.pending_triggers.push(pending);
        }
    }
}

/// Whether a permanent on the battlefield needs every declared attacker
/// logged (`CommittedEvent::DeclaredAttacker`): Glimmer Lens.
pub(crate) fn observes_attacks(state: &GameState) -> bool {
    crate::eternal_e5a_v1::active_permanents(state, GLIMMER_LENS, None)
        .next()
        .is_some()
}

/// The "+1/+1 for each artifact and/or enchantment you control" of a
/// Nettlecyst attached to `object`, and the "+1/+1 for each artifact you
/// control" of an animated Iron Man Armor.
pub(crate) fn controlled_boost(state: &GameState, object: ObjectId) -> (i128, i128) {
    let Some(live) = state.objects.try_get(object) else {
        return (0, 0);
    };
    let animated = live.v4.animation_timestamp.is_some();
    if live.attachments.is_empty() && !animated {
        return (0, 0);
    }
    let mut total = 0_i128;
    for cyst in attached_equipment(state, object, NETTLECYST) {
        let controller = state.objects.get(cyst).controller;
        let count = controlled(state, controller)
            .into_iter()
            .filter(|&permanent| {
                crate::engine::object_has_type(state, permanent, CardType::Artifact)
                    || crate::engine::object_has_type(state, permanent, CardType::Enchantment)
            })
            .count();
        total += i128::try_from(count).unwrap_or(0);
    }
    if animated
        && active_named(state, object, IRON_MAN_ARMOR)
        && crate::continuous_characteristics_v1::animation(state, object).is_some()
    {
        let count = controlled(state, live.controller)
            .into_iter()
            .filter(|&permanent| {
                crate::engine::object_has_type(state, permanent, CardType::Artifact)
            })
            .count();
        total += i128::try_from(count).unwrap_or(0);
    }
    (total, total)
}

/// 301.5c and 704.5n: an Equipment that is also a creature can't be attached
/// to a creature (Iron Man Armor once its ability animates it). Run with the
/// other attachment checks of the state-based actions; whether it changed
/// anything.
pub(crate) fn unattach_animated_equipment(state: &mut GameState) -> bool {
    let detach: Vec<(ObjectId, ObjectLinkV4)> = state
        .objects
        .iter()
        .filter_map(|(id, live)| {
            let link = live.v4.attached_to?;
            (live.zone == Zone::Battlefield
                && def_name(state, id) == Some(IRON_MAN_ARMOR)
                && is_creature(state, id))
            .then_some((id, link))
        })
        .collect();
    for &(id, link) in &detach {
        if state.objects.try_get(link.object).is_some() {
            state
                .objects
                .get_mut(link.object)
                .attachments
                .retain(|attachment| *attachment != id);
        }
        state.objects.get_mut(id).v4.attached_to = None;
    }
    !detach.is_empty()
}

// ---- Delayed triggers ---------------------------------------------------------------

/// The triggers this collection puts on the stack that no printed trigger
/// condition matches: Skullclamp's draws, the queued combat damage and block
/// triggers, Glimmer Lens's draw, Astrologian's Planisphere's counters and
/// Sword of the Meek's return.
pub(crate) fn delayed_triggers(
    state: &mut GameState,
    events: &[CommittedEvent],
) -> Vec<PendingTrigger> {
    let mut triggers = Vec::new();
    clamp_triggers(state, events, &mut triggers);
    lens_triggers(state, events, &mut triggers);
    planisphere_triggers(state, events, &mut triggers);
    sword_triggers(state, events, &mut triggers);
    let queued = match state.eternal_v1.as_mut() {
        Some(eternal) if !eternal.c4c.queued_triggers.is_empty() => {
            std::mem::take(&mut eternal.c4c.queued_triggers)
        }
        _ => Vec::new(),
    };
    for queued in queued {
        triggers.push(crate::eternal_cards_v1::delayed_trigger(
            queued.source,
            queued.controller,
            queued_effect(queued.kind),
        ));
    }
    triggers
}

/// Skullclamp: "Whenever equipped creature dies, draw two cards." Every record
/// is consumed by the collection it was made for.
fn clamp_triggers(state: &mut GameState, events: &[CommittedEvent], out: &mut Vec<PendingTrigger>) {
    let records = match state.eternal_v1.as_mut() {
        Some(eternal) if !eternal.c4c.clamp_records.is_empty() => {
            std::mem::take(&mut eternal.c4c.clamp_records)
        }
        _ => return,
    };
    for record in records {
        let went_to_graveyard = events.iter().any(|event| {
            matches!(
                event,
                CommittedEvent::ZoneChange {
                    object,
                    from: Zone::Battlefield,
                    to: Zone::Graveyard,
                    ..
                } if *object == record.host.object
            )
        });
        let moved = state
            .objects
            .try_get(record.host.object)
            .is_some_and(|live| live.zone_change_count > record.host.zone_change_count);
        if went_to_graveyard && moved {
            out.push(crate::eternal_cards_v1::delayed_trigger(
                record.clamp,
                record.clamp.controller,
                skullclamp_draw(),
            ));
        }
    }
}

/// Glimmer Lens: "Whenever equipped creature and at least one other creature
/// attack, draw a card." One trigger per Lens whose creature is among at
/// least two declared attackers.
fn lens_triggers(state: &GameState, events: &[CommittedEvent], out: &mut Vec<PendingTrigger>) {
    let attackers: Vec<ObjectLinkV4> = events
        .iter()
        .filter_map(|event| match *event {
            CommittedEvent::DeclaredAttacker {
                source,
                source_zone_change_count,
                ..
            } => Some(ObjectLinkV4 {
                object: source,
                zone_change_count: source_zone_change_count,
            }),
            _ => None,
        })
        .collect();
    if attackers.len() < 2 {
        return;
    }
    for lens in crate::eternal_e5a_v1::active_permanents(state, GLIMMER_LENS, None) {
        let live = state.objects.get(lens);
        if live
            .v4
            .attached_to
            .is_some_and(|host| attackers.contains(&host))
        {
            out.push(crate::eternal_cards_v1::delayed_trigger(
                AbilitySourceContractV4::capture(state, lens),
                live.controller,
                lens_draw(),
            ));
        }
    }
}

/// How many cards each player had drawn this turn as of each event of the
/// batch (`trigger::draws_this_turn_snapshot`'s count), zero for any other
/// event.
fn draw_ordinals(state: &GameState, events: &[CommittedEvent]) -> Vec<u32> {
    let batch_draws = |player: PlayerId| {
        let count = events
            .iter()
            .filter(|event| {
                matches!(
                    event,
                    CommittedEvent::Draw { player: drawer, object: Some(_) } if *drawer == player
                )
            })
            .count();
        u32::try_from(count).unwrap_or(u32::MAX)
    };
    let mut running = [
        state.players[0]
            .draws_this_turn
            .saturating_sub(batch_draws(PlayerId::P0)),
        state.players[1]
            .draws_this_turn
            .saturating_sub(batch_draws(PlayerId::P1)),
    ];
    events
        .iter()
        .map(|event| match event {
            CommittedEvent::Draw {
                player,
                object: Some(_),
            } => {
                running[player.index()] += 1;
                running[player.index()]
            }
            _ => 0,
        })
        .collect()
}

/// Astrologian's Planisphere: the equipped creature has "Whenever you cast a
/// noncreature spell and whenever you draw your third card each turn, put a
/// +1/+1 counter on this creature." Each Planisphere on a creature its caster
/// or drawer controls grants one trigger per event, a granted ability that
/// belongs to the creature.
fn planisphere_triggers(
    state: &GameState,
    events: &[CommittedEvent],
    out: &mut Vec<PendingTrigger>,
) {
    if !events.iter().any(|event| {
        matches!(
            event,
            CommittedEvent::SpellCast { .. }
                | CommittedEvent::Draw {
                    object: Some(_),
                    ..
                }
        )
    }) {
        return;
    }
    let ordinals = draw_ordinals(state, events);
    for (index, event) in events.iter().enumerate() {
        let player = match *event {
            CommittedEvent::SpellCast {
                spell,
                controller: caster,
            } if !crate::trigger::selected_spell_types(state, spell)
                .contains(&CardType::Creature) =>
            {
                caster
            }
            CommittedEvent::Draw {
                player: drawer,
                object: Some(_),
            } if ordinals[index] == 3 => drawer,
            _ => continue,
        };
        for host in controlled(state, player) {
            if !is_creature(state, host) {
                continue;
            }
            for planisphere in attached_equipment(state, host, ASTROLOGIANS_PLANISPHERE) {
                let granted_at = state
                    .objects
                    .get(planisphere)
                    .v4
                    .layer_timestamp
                    .unwrap_or(0);
                if crate::continuous_characteristics_v1::grant_survives(state, host, granted_at) {
                    out.push(crate::eternal_cards_v1::delayed_trigger(
                        AbilitySourceContractV4::capture(state, host),
                        player,
                        planisphere_counter(),
                    ));
                }
            }
        }
    }
}

/// Sword of the Meek: "Whenever a 1/1 creature you control enters, you may
/// return this card from your graveyard to the battlefield, then attach it to
/// that creature." One trigger per such creature for each Sword in its
/// controller's graveyard, looking at the creature's power and toughness as
/// the collection finds it.
fn sword_triggers(state: &GameState, events: &[CommittedEvent], out: &mut Vec<PendingTrigger>) {
    let mut entrants: Vec<ObjectId> = Vec::new();
    for event in events {
        match *event {
            CommittedEvent::ZoneChange {
                object,
                to: Zone::Battlefield,
                ..
            }
            | CommittedEvent::CreateToken { object, .. } => {
                if !entrants.contains(&object) {
                    entrants.push(object);
                }
            }
            _ => {}
        }
    }
    for entrant in entrants {
        let Some(live) = state.objects.try_get(entrant) else {
            continue;
        };
        if live.zone != Zone::Battlefield
            || !is_creature(state, entrant)
            || crate::engine::checked_effective_pt(state, entrant).ok() != Some((1, 1))
        {
            continue;
        }
        let player = live.controller;
        let creature = link_of(state, entrant);
        for &card in &state.players[player.index()].graveyard {
            let sword = state.objects.get(card);
            if sword.zone == Zone::Graveyard
                && sword.owner == player
                && def_name(state, card) == Some(SWORD_OF_THE_MEEK)
            {
                out.push(crate::eternal_cards_v1::delayed_trigger(
                    AbilitySourceContractV4::capture(state, card),
                    player,
                    sword_may_return(creature),
                ));
            }
        }
    }
}

// ---- Triggered abilities ------------------------------------------------------------

const fn trigger(condition: TriggerCondition, effect: fn() -> EffectOp) -> TriggeredAbilityDef {
    TriggeredAbilityDef {
        condition,
        home_zone: Zone::Battlefield,
        intervening_if_kicked: false,
        intervening_if_controls_another_source_card: false,
        face_index: 0,
        effect,
    }
}

const LIVING_WEAPON_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, living_weapon)];
const FOR_MIRRODIN_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, for_mirrodin)];
const IRON_MAN_ARMOR_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, attach_to_target)];
const COPTER_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(TriggerCondition::Attacks, copter_loot)];
const CHARIOT_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(TriggerCondition::Etb, chariot_cats),
    trigger(TriggerCondition::Attacks, chariot_copy),
];

/// Triggered abilities of this module's cards, the tail of the trigger
/// tables. Skullclamp's, Umezawa's Jitte's, Sword of the Meek's and
/// Astrologian's Planisphere's abilities, Glimmer Lens's attack trigger and
/// Smuggler's Copter's block trigger are delayed triggers.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        NETTLECYST | KALDRA_COMPLEAT => &LIVING_WEAPON_TRIGGERS,
        GLIMMER_LENS => &FOR_MIRRODIN_TRIGGERS,
        IRON_MAN_ARMOR => &IRON_MAN_ARMOR_TRIGGERS,
        SMUGGLERS_COPTER => &COPTER_TRIGGERS,
        ESIKAS_CHARIOT => &CHARIOT_TRIGGERS,
        _ => &[],
    }
}

/// Target specs of this module's printed and queued triggered abilities.
pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    if name == IRON_MAN_ARMOR && *effect == attach_to_target() {
        return Some(TargetSpec::ControlledCreature);
    }
    if name == ESIKAS_CHARIOT && *effect == chariot_copy() {
        return Some(TargetSpec::StandardV1(StandardTargetV1::C4cV1(
            targets::C4cTargetV1::ControlledTokenV1,
        )));
    }
    delayed_effect_matches(name, effect).then_some(TargetSpec::None)
}

/// Queued and granted triggers owned by this module's cards. The Planisphere's
/// and Kaldra Compleat's belong to whichever creature has them.
pub(crate) fn delayed_effect_matches(name: &str, effect: &EffectOp) -> bool {
    if matches!(
        effect,
        EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::C4cV1(
            C4cOpV1::KaldraExileV1 { .. } | C4cOpV1::PlanisphereCounterV1
        )))
    ) {
        return true;
    }
    match name {
        SKULLCLAMP => *effect == skullclamp_draw(),
        UMEZAWAS_JITTE => *effect == jitte_counters(),
        GLIMMER_LENS => *effect == lens_draw(),
        SMUGGLERS_COPTER => *effect == copter_loot(),
        SWORD_OF_THE_MEEK => matches!(
            effect,
            EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::C4cV1(
                C4cOpV1::SwordMayReturnV1 { .. }
            )))
        ),
        _ => false,
    }
}

// ---- Execution ----------------------------------------------------------------------

/// Target `index`, if it is still the incarnation chosen and still legal for
/// the resolving ability under `spec`.
fn legal_target(
    ctx: &ExecCtx,
    state: &GameState,
    index: usize,
    spec: TargetSpec,
) -> Option<Target> {
    let target = *ctx.targets.get(index)?;
    (ctx.target_incarnation_matches(index, state)
        && crate::engine::effect_target_is_legal(
            state,
            ctx.source,
            ctx.controller,
            spec,
            &ctx.targets,
            index,
        ))
    .then_some(target)
}

/// Whether `object` is still the battlefield incarnation `zone_change_count`
/// names.
fn on_battlefield(state: &GameState, object: ObjectId, zone_change_count: u32) -> bool {
    state.objects.try_get(object).is_some_and(|live| {
        live.zone == Zone::Battlefield && live.zone_change_count == zone_change_count
    })
}

/// The resolving ability's source, if it is still the graveyard incarnation
/// its contract names.
fn source_in_graveyard(ctx: &ExecCtx, state: &GameState) -> Option<ObjectId> {
    let contract = ctx.ability_source_contract?;
    let live = state.objects.try_get(contract.source)?;
    (contract.zone == Zone::Graveyard
        && live.zone == Zone::Graveyard
        && live.zone_change_count == contract.zone_change_count)
        .then_some(contract.source)
}

/// Sword of the Meek: return the Sword from the graveyard, then attach it to
/// the creature if that is still the permanent that entered.
fn run_sword_return(ctx: &ExecCtx, state: &mut GameState, creature: ObjectId, count: u32) {
    let Some(sword) = source_in_graveyard(ctx, state) else {
        return;
    };
    event::propose_and_commit(state, ProposedEvent::zone_change(sword, Zone::Battlefield));
    let Some(returned) = state.objects.try_get(sword) else {
        return;
    };
    if returned.zone != Zone::Battlefield
        || !on_battlefield(state, creature, count)
        || !is_creature(state, creature)
    {
        return;
    }
    let sword_count = returned.zone_change_count;
    // Attachment can refuse (protection from the Sword); the Sword then stays
    // where it entered, unattached.
    let _ = state.attach_object_exact(sword, sword_count, creature, count);
}

/// Iron Man Armor's "Equip {2}": attach the Armor to target 0 if it is still
/// the Armor that was activated, not a creature, and the target is still a
/// legal creature you control other than itself.
fn run_iron_man_equip(ctx: &ExecCtx, state: &mut GameState) {
    let Some(contract) = ctx.ability_source_contract else {
        return;
    };
    let Some(Target::Object(target)) = legal_target(ctx, state, 0, TargetSpec::ControlledCreature)
    else {
        return;
    };
    if contract.source != ctx.source
        || target == ctx.source
        || !on_battlefield(state, ctx.source, contract.zone_change_count)
        || is_creature(state, ctx.source)
        || !on_battlefield(state, target, state.objects.get(target).zone_change_count)
        || !is_creature(state, target)
    {
        return;
    }
    let target_count = state.objects.get(target).zone_change_count;
    // Attachment can refuse (protection from the Armor): the old attachment
    // then stays.
    let _ = state.attach_object_exact(ctx.source, contract.zone_change_count, target, target_count);
}

/// Runs one of this module's leaves in synchronous execution.
pub(crate) fn execute(op: &C4cOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        C4cOpV1::KaldraExileV1 {
            victim,
            zone_change_count,
        } => {
            if on_battlefield(state, *victim, *zone_change_count) {
                event::propose_and_commit(state, ProposedEvent::zone_change(*victim, Zone::Exile));
            }
        }
        C4cOpV1::PlanisphereCounterV1 => {
            crate::effect::execute(&EffectOp::AddPlusOneCounterToAbilitySource, ctx, state);
        }
        // Asks a player, so it only runs in `drive`.
        C4cOpV1::SwordMayReturnV1 { .. } => {}
        C4cOpV1::SwordReturnV1 {
            creature,
            zone_change_count,
        } => run_sword_return(ctx, state, *creature, *zone_change_count),
        C4cOpV1::IronManEquipV1 => run_iron_man_equip(ctx, state),
        C4cOpV1::ChariotCopyV1 => {
            let spec = TargetSpec::StandardV1(StandardTargetV1::C4cV1(
                targets::C4cTargetV1::ControlledTokenV1,
            ));
            if let Some(Target::Object(token)) = legal_target(ctx, state, 0, spec) {
                crate::standard_cards_v1::create_token_copy(state, token, ctx.controller);
            }
        }
    }
}

/// Runs one of this module's leaves in the resumable interpreter.
pub(crate) fn drive(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    op: &C4cOpV1,
    path: Vec<u16>,
) -> Result<bool, String> {
    match op {
        C4cOpV1::SwordMayReturnV1 {
            creature,
            zone_change_count,
        } => {
            // A Sword that already returned (an earlier trigger of the same
            // collection) is not offered again.
            if source_in_graveyard(&continuation.ctx, state).is_some() {
                continuation.frames.push(EffectFrame::Program {
                    op: EffectOp::Choice {
                        controller: PlayerRef::Controller,
                        options: vec![
                            EffectOp::Sequence(Vec::new()),
                            c4c(C4cOpV1::SwordReturnV1 {
                                creature: *creature,
                                zone_change_count: *zone_change_count,
                            }),
                        ],
                    },
                    path,
                });
            }
            Ok(false)
        }
        other => {
            execute(other, &continuation.ctx, state);
            Ok(false)
        }
    }
}
