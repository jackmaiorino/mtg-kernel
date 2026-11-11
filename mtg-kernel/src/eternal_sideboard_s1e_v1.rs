//! Card programs for the eternal catalog's Vintage sideboard wave S1e
//! (`docs/design/eternal_formats_support_v1.md`): the sideboard cards that
//! complete Jewel Shops, Oath of Druids, Dredge, Doomsday and Underworld
//! Breach.
//!
//! `eternal_cards_v1` forwards its effect leaves, triggers and turn
//! bookkeeping here, wave P3f's recipe tables fall through to
//! `build_eternal_s1e_v1.rs`, and the engine calls the hooks below only in
//! `eternal-fixtures` builds. Every table is keyed by a printed name that
//! occurs only in the eternal registry. Behavior follows each card's current
//! Oracle text and rulings, checked against its XMage source.
//!
//! Primitives added here:
//! - a permanent that loses all abilities until a player's next turn, with
//!   base power and toughness 2/2 if it was a creature (Azure Beastbinder),
//!   with `targets`' filter;
//! - a creature exiled with time counters that gains suspend (Suspend), and
//!   the haste of a creature spell cast from suspend (702.62a);
//! - an alternative cost whose exiled card announces X (Sickening Shoal);
//! - an Aura whose host loses all abilities and gains types and a base
//!   power and toughness (Unable to Scream);
//! - a sacrifice of three creatures of the opponent's choice at once, and a
//!   reanimation that adds a subtype as the card enters (Portal to
//!   Phyrexia).
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

pub(crate) mod targets;

use crate::card_def::{CardDef, CardType, Keywords, Subtype, TargetSpec, CARD_DEFS};
use crate::effect::{EffectContinuation, EffectObjectBinding, EffectOp, ExecCtx};
use crate::eternal_cards_v1::e3::{E3ChoiceKindV1, E3ChoiceV1};
use crate::eternal_cards_v1::EternalOpV1;
use crate::event::{self, CommittedEvent, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::standard_cards_v1::{StandardOpV1, StandardTargetV1};
use crate::state::{GameState, ObjectLinkV4, Target, Zone};
use crate::trigger::{PendingTrigger, TriggerCondition, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};

pub(crate) const AZURE_BEASTBINDER: &str = "Azure Beastbinder";
pub(crate) const SICKENING_SHOAL: &str = "Sickening Shoal";
pub(crate) const UNABLE_TO_SCREAM: &str = "Unable to Scream";
pub(crate) const PORTAL_TO_PHYREXIA: &str = "Portal to Phyrexia";

/// Suspend: "put two time counters on it".
const SUSPEND_TIME_COUNTERS: u8 = 2;
/// Portal to Phyrexia: "each opponent sacrifices three creatures".
const PORTAL_SACRIFICES: u16 = 3;

/// Per-game state of this module's cards, kept inside
/// `eternal_cards_v1::EternalStateV1`. Absent from every snapshot until one
/// of them needs it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct S1eStateV1 {
    /// Azure Beastbinder's effects, one per resolved trigger with a target.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bound: Vec<BoundV1>,
    /// Suspended cards whose last time counter was removed and whose owner
    /// chose to cast them, by exile incarnation, with that owner.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suspend_casts: Vec<(ObjectLinkV4, PlayerId)>,
    /// Creature spells cast from suspend, by stack incarnation, with their
    /// caster and the timestamp of their haste (702.62a).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hasty_spells: Vec<(ObjectLinkV4, PlayerId, u64)>,
    /// The permanents those spells became, by battlefield incarnation, with
    /// the caster and the timestamp of their haste.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hasty_permanents: Vec<(ObjectLinkV4, PlayerId, u64)>,
}

impl S1eStateV1 {
    pub(crate) fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// One Azure Beastbinder effect: its target's battlefield incarnation, the
/// player whose next turn ends it, its timestamp, and whether the target
/// was a creature as the ability resolved (the base 2/2 part).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoundV1 {
    pub host: ObjectLinkV4,
    pub holder: PlayerId,
    pub timestamp: u64,
    pub creature: bool,
}

pub(crate) fn s1e_state(state: &mut GameState) -> &mut S1eStateV1 {
    &mut state.eternal_v1.get_or_insert_with(Default::default).s1e
}

pub fn read_state(state: &GameState) -> Option<&S1eStateV1> {
    state.eternal_v1.as_ref().map(|eternal| &eternal.s1e)
}

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::S1eV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum S1eOpV1 {
    /// Azure Beastbinder's attack trigger: up to one target 0 loses all
    /// abilities until its controller's next turn, and has base power and
    /// toughness 2/2 if it is a creature.
    BeastbinderV1,
    /// Suspend: exile target creature 0 with two time counters; it gains
    /// suspend.
    SuspendV1,
    /// Sickening Shoal: target creature 0 gets -X/-X until end of turn.
    ShoalV1,
    /// Portal to Phyrexia's entry trigger: each opponent sacrifices three
    /// creatures of their choice. A choice.
    PortalSacrificeV1,
    /// Portal to Phyrexia's upkeep trigger: put target creature card 0 from
    /// a graveyard onto the battlefield under its controller's control, a
    /// Phyrexian in addition to its other types.
    PortalReanimateV1,
}

impl S1eOpV1 {
    /// Whether this leaf can yield a player decision, so its program must
    /// use the resumable interpreter.
    pub(crate) fn contains_player_choice(&self) -> bool {
        matches!(self, Self::PortalSacrificeV1)
    }
}

fn s1e(op: S1eOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::S1eV1(op)))
}

fn link(state: &GameState, object: ObjectId) -> ObjectLinkV4 {
    ObjectLinkV4 {
        object,
        zone_change_count: state.objects.get(object).zone_change_count,
    }
}

/// `object`'s battlefield incarnation, if it is on the battlefield.
fn battlefield_link(state: &GameState, object: ObjectId) -> Option<ObjectLinkV4> {
    let live = state.objects.try_get(object)?;
    (live.zone == Zone::Battlefield).then_some(ObjectLinkV4 {
        object,
        zone_change_count: live.zone_change_count,
    })
}

fn link_live_in(state: &GameState, link: ObjectLinkV4, zone: Zone) -> bool {
    state
        .objects
        .try_get(link.object)
        .is_some_and(|live| live.zone == zone && live.zone_change_count == link.zone_change_count)
}

fn bind(state: &GameState, object: ObjectId) -> EffectObjectBinding {
    let live = state.objects.get(object);
    EffectObjectBinding {
        object,
        expected_zone: live.zone,
        expected_zone_change_count: live.zone_change_count,
    }
}

fn halt(state: &mut GameState, source: ObjectId) {
    state.engine.halted = Some((
        crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
        source,
    ));
}

/// Target `index` of the resolving spell or ability if it is still the
/// announced incarnation and still legal for `spec` (608.2b).
fn legal_object_target(
    ctx: &ExecCtx,
    state: &GameState,
    index: usize,
    spec: TargetSpec,
) -> Option<ObjectId> {
    let Some(Target::Object(object)) = ctx.targets.get(index).copied() else {
        return None;
    };
    (ctx.target_incarnation_matches(index, state)
        && crate::engine::effect_target_is_legal(
            state,
            ctx.source,
            ctx.controller,
            spec,
            &ctx.targets,
            index,
        ))
    .then_some(object)
}

/// Every battlefield object, in each player's battlefield order.
fn battlefield(state: &GameState) -> impl Iterator<Item = ObjectId> + '_ {
    [PlayerId::P0, PlayerId::P1]
        .into_iter()
        .flat_map(move |player| state.players[player.index()].battlefield.iter().copied())
        .filter(move |&object| state.objects.get(object).zone == Zone::Battlefield)
}

/// Whether `object` is a front-face battlefield permanent named `name`
/// whose printed abilities apply.
fn active_named(state: &GameState, object: ObjectId, name: &str) -> bool {
    state.objects.try_get(object).is_some_and(|live| {
        live.zone == Zone::Battlefield
            && live.v4.face_index == 0
            && CARD_DEFS
                .get(usize::from(live.card_def))
                .is_some_and(|def| def.name == name && def.is_executable())
            && crate::continuous_characteristics_v1::printed_abilities_active(state, object)
    })
}

// ---- Programs -----------------------------------------------------------------------

/// Suspend: "Exile target creature and put two time counters on it. If it
/// doesn't have suspend, it gains suspend."
pub fn suspend() -> EffectOp {
    s1e(S1eOpV1::SuspendV1)
}

/// Sickening Shoal: "You may exile a black card with mana value X from your
/// hand rather than pay this spell's mana cost. Target creature gets -X/-X
/// until end of turn." Choosing the exiled card announces X
/// (`shoal_forced_x`).
pub fn sickening_shoal() -> EffectOp {
    s1e(S1eOpV1::ShoalV1)
}

fn beastbinder_trigger() -> EffectOp {
    s1e(S1eOpV1::BeastbinderV1)
}

fn portal_sacrifice() -> EffectOp {
    s1e(S1eOpV1::PortalSacrificeV1)
}

fn portal_reanimate() -> EffectOp {
    s1e(S1eOpV1::PortalReanimateV1)
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

/// Azure Beastbinder: "Whenever this creature attacks, up to one target
/// artifact, creature, or planeswalker an opponent controls loses all
/// abilities until your next turn. If it's a creature, it also has base
/// power and toughness 2/2 until your next turn."
const BEASTBINDER_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Attacks, beastbinder_trigger)];

/// Portal to Phyrexia: "When this artifact enters, each opponent sacrifices
/// three creatures of their choice. At the beginning of your upkeep, put
/// target creature card from a graveyard onto the battlefield under your
/// control. It's a Phyrexian in addition to its other types."
const PORTAL_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(TriggerCondition::Etb, portal_sacrifice),
    trigger(
        TriggerCondition::BeginningOfUpkeep {
            controller_only: true,
        },
        portal_reanimate,
    ),
];

/// Triggered abilities of this module's cards, then of wave S1g's, the
/// tail of the trigger tables.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        AZURE_BEASTBINDER => &BEASTBINDER_TRIGGERS,
        PORTAL_TO_PHYREXIA => &PORTAL_TRIGGERS,
        _ => crate::eternal_sideboard_s1g_v1::triggers_for(name),
    }
}

/// Target specs of this module's triggered abilities, and of suspend's
/// time-counter trigger on whatever card has suspend. `None` for every
/// other card and effect.
pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    if suspend_tick(effect) {
        return Some(TargetSpec::None);
    }
    if !triggers_for(name)
        .iter()
        .any(|trigger| (trigger.effect)() == *effect)
    {
        return None;
    }
    Some(match name {
        AZURE_BEASTBINDER => TargetSpec::StandardV1(
            StandardTargetV1::UpToOneOpponentArtifactCreatureOrPlaneswalkerV1,
        ),
        PORTAL_TO_PHYREXIA if *effect == portal_reanimate() => {
            TargetSpec::StandardV1(StandardTargetV1::CreatureCardInAGraveyardV1)
        }
        _ => TargetSpec::None,
    })
}

/// Suspend's "At the beginning of your upkeep, if this card is suspended,
/// remove a time counter from it" (wave E7a's trigger).
fn suspend_tick(effect: &EffectOp) -> bool {
    matches!(
        effect,
        EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::E7aV1(
            crate::eternal_legacy_e7a_v1::E7aOpV1::SuspendTickV1 { .. }
        )))
    )
}

/// Queued triggers owned by this module's cards: suspend's time-counter
/// trigger belongs to whatever card Suspend exiled, as well as to Gaea's
/// Will's own suspend (wave E7a).
pub(crate) fn delayed_effect_matches(_name: &str, effect: &EffectOp) -> bool {
    cfg!(feature = "eternal-fixtures") && suspend_tick(effect)
}

// ---- Statics: Azure Beastbinder and Unable to Scream --------------------------------

/// The timestamp of the latest Azure Beastbinder effect on `object`'s
/// battlefield incarnation, and whether one of them sets its base power
/// and toughness (the latest such one's timestamp).
fn bound_timestamps(state: &GameState, object: ObjectId) -> Option<(u64, Option<u64>)> {
    let s1e = read_state(state)?;
    if s1e.bound.is_empty() {
        return None;
    }
    let host = battlefield_link(state, object)?;
    let records = s1e.bound.iter().filter(|record| record.host == host);
    let latest = records.clone().map(|record| record.timestamp).max()?;
    let base = records
        .filter(|record| record.creature)
        .map(|record| record.timestamp)
        .max();
    Some((latest, base))
}

/// The timestamp of the Unable to Scream attached to `host` whose
/// abilities apply, the latest if several are.
fn scream_timestamp(state: &GameState, host: ObjectId) -> Option<u64> {
    let object = state.objects.try_get(host)?;
    if object.zone != Zone::Battlefield || object.attachments.is_empty() {
        return None;
    }
    let host_link = Some(ObjectLinkV4 {
        object: host,
        zone_change_count: object.zone_change_count,
    });
    object
        .attachments
        .iter()
        .filter_map(|&aura| {
            let source = state.objects.try_get(aura)?;
            (aura != host
                && source.v4.attached_to == host_link
                && active_named(state, aura, UNABLE_TO_SCREAM))
            .then(|| source.v4.layer_timestamp.unwrap_or(0))
        })
        .max()
}

/// Layer 6 removals from this module: Azure Beastbinder's "loses all
/// abilities until your next turn" and Unable to Scream's "Enchanted
/// creature loses all abilities", at their timestamps.
pub(crate) fn removal_timestamp(state: &GameState, object: ObjectId) -> Option<u64> {
    if !cfg!(feature = "eternal-fixtures") {
        return None;
    }
    let bound = bound_timestamps(state, object).map(|(latest, _)| latest);
    bound
        .into_iter()
        .chain(scream_timestamp(state, object))
        .max()
}

/// Layer 7b settings from this module: Azure Beastbinder's base 2/2 for a
/// creature and Unable to Scream's base 0/2, at their timestamps (a later
/// setting overwrites an earlier one).
pub(crate) fn base_power_toughness(state: &GameState, object: ObjectId) -> Option<(i16, i16, u64)> {
    if !cfg!(feature = "eternal-fixtures") {
        return None;
    }
    let bound = bound_timestamps(state, object)
        .and_then(|(_, base)| base)
        .map(|timestamp| (2, 2, timestamp));
    let scream = scream_timestamp(state, object).map(|timestamp| (0, 2, timestamp));
    bound
        .into_iter()
        .chain(scream)
        .max_by_key(|(_, _, timestamp)| *timestamp)
}

/// Unable to Scream: "is a Toy artifact creature ... in addition to its
/// other types" (layer 4). Whether `object` is an artifact and a creature
/// because of it.
pub(crate) fn adds_artifact_creature(state: &GameState, object: ObjectId) -> bool {
    cfg!(feature = "eternal-fixtures") && scream_timestamp(state, object).is_some()
}

/// The Toy creature type Unable to Scream adds with those types.
pub(crate) fn is_toy(state: &GameState, object: ObjectId) -> bool {
    adds_artifact_creature(state, object)
}

/// Unable to Scream: "As long as enchanted creature is face down, it can't
/// be turned face up."
pub(crate) fn cant_turn_face_up(state: &GameState, object: ObjectId) -> bool {
    adds_artifact_creature(state, object)
        && state
            .objects
            .try_get(object)
            .is_some_and(|live| live.v4.face_down_v1.is_some())
}

/// Azure Beastbinder: "This creature can't be blocked by creatures with
/// power 2 or greater." Read as blockers are declared, so a later power
/// change doesn't remove a block (its 2024-07-26 ruling).
pub(crate) fn blocker_allowed(state: &GameState, attacker: ObjectId, blocker: ObjectId) -> bool {
    !cfg!(feature = "eternal-fixtures")
        || !active_named(state, attacker, AZURE_BEASTBINDER)
        || crate::engine::effective_power(state, blocker) < 2
}

// ---- Statics: suspend's haste -------------------------------------------------------

/// The haste a creature spell cast from suspend gives the permanent it
/// becomes, while its caster controls it and no later effect removed its
/// abilities (702.62a).
pub(crate) fn granted_keywords(state: &GameState, object: ObjectId) -> Keywords {
    if !cfg!(feature = "eternal-fixtures") {
        return Keywords::NONE;
    }
    let Some(s1e) = read_state(state) else {
        return Keywords::NONE;
    };
    if s1e.hasty_permanents.is_empty() {
        return Keywords::NONE;
    }
    let Some(host) = battlefield_link(state, object) else {
        return Keywords::NONE;
    };
    let controller = state.objects.get(object).controller;
    let hasty = s1e
        .hasty_permanents
        .iter()
        .any(|&(entry, caster, timestamp)| {
            entry == host
                && caster == controller
                && crate::continuous_characteristics_v1::grant_survives(state, object, timestamp)
        });
    if hasty {
        Keywords::HASTE
    } else {
        Keywords::NONE
    }
}

/// Wave E7a's suspend offers `card` to its owner `player` as the last time
/// counter is removed. A creature spell they cast from it gains haste.
pub(crate) fn note_suspend_cast(state: &mut GameState, card: ObjectLinkV4, player: PlayerId) {
    if !cfg!(feature = "eternal-fixtures") {
        return;
    }
    let casts = &mut s1e_state(state).suspend_casts;
    if !casts.contains(&(card, player)) {
        casts.push((card, player));
    }
}

/// "It gains haste until you lose control of the spell or the permanent it
/// becomes": a control change ends the haste.
pub(crate) fn control_changed(state: &mut GameState, object: ObjectId) {
    if read_state(state).is_none_or(|s1e| {
        s1e.hasty_permanents
            .iter()
            .all(|(entry, _, _)| entry.object != object)
    }) {
        return;
    }
    s1e_state(state)
        .hasty_permanents
        .retain(|(entry, _, _)| entry.object != object);
}

/// Follows a suspend cast from its exile incarnation to the stack, and from
/// the stack to the battlefield.
fn note_suspend_spells(state: &mut GameState, events: &[CommittedEvent]) {
    let Some(s1e) = read_state(state) else {
        return;
    };
    if s1e.suspend_casts.is_empty() && s1e.hasty_spells.is_empty() {
        return;
    }
    for event in events {
        match *event {
            CommittedEvent::SpellCast { spell, controller } => {
                let Some(live) = state.objects.try_get(spell) else {
                    continue;
                };
                if live.zone != Zone::Stack {
                    continue;
                }
                let Some(exiled) = live.zone_change_count.checked_sub(1) else {
                    continue;
                };
                let card = ObjectLinkV4 {
                    object: spell,
                    zone_change_count: exiled,
                };
                let Some(index) = read_state(state).and_then(|s1e| {
                    s1e.suspend_casts
                        .iter()
                        .position(|&(entry, owner)| entry == card && owner == controller)
                }) else {
                    continue;
                };
                s1e_state(state).suspend_casts.remove(index);
                if !crate::engine::object_has_type(state, spell, CardType::Creature) {
                    continue;
                }
                let record = (
                    link(state, spell),
                    controller,
                    crate::engine::next_timestamp(state),
                );
                s1e_state(state).hasty_spells.push(record);
            }
            CommittedEvent::ZoneChange {
                object,
                from: Zone::Stack,
                to: Zone::Battlefield,
                ..
            } => {
                let Some(live) = state.objects.try_get(object) else {
                    continue;
                };
                let Some(stack) = live.zone_change_count.checked_sub(1) else {
                    continue;
                };
                let spell = ObjectLinkV4 {
                    object,
                    zone_change_count: stack,
                };
                let Some(index) = read_state(state).and_then(|s1e| {
                    s1e.hasty_spells
                        .iter()
                        .position(|(entry, _, _)| *entry == spell)
                }) else {
                    continue;
                };
                let (_, caster, timestamp) = s1e_state(state).hasty_spells.remove(index);
                if state.objects.get(object).zone == Zone::Battlefield {
                    let host = link(state, object);
                    s1e_state(state)
                        .hasty_permanents
                        .push((host, caster, timestamp));
                }
            }
            _ => {}
        }
    }
}

// ---- Shoal's X ----------------------------------------------------------------------

/// Sickening Shoal cast for its alternative cost: X is the mana value of
/// the black card chosen to exile, announced as it is chosen (601.2b), so
/// the cast asks for no X. `None` for every other cast.
pub(crate) fn shoal_forced_x(
    pending: &crate::engine::PendingCast,
    def: &CardDef,
    state: &GameState,
) -> Option<u16> {
    if !cfg!(feature = "eternal-fixtures")
        || def.name != SICKENING_SHOAL
        || pending.cast_mode != Some(crate::engine::CastMode::Alternative)
    {
        return None;
    }
    let &card = pending.sacrifice_chosen.first()?;
    state
        .objects
        .try_get(card)
        .filter(|live| live.zone == Zone::Hand)?;
    Some(crate::engine::object_mana_value(state, card))
}

// ---- Queued triggers and turn bookkeeping -------------------------------------------

/// The triggers this module's hooks set off in this collection: none. The
/// hook follows suspend casts.
pub(crate) fn delayed_triggers(
    state: &mut GameState,
    events: &[CommittedEvent],
) -> Vec<PendingTrigger> {
    if cfg!(feature = "eternal-fixtures") {
        note_suspend_spells(state, events);
    }
    Vec::new()
}

/// As `player`'s turn begins their Azure Beastbinder effects end ("until
/// your next turn"). The stack is empty, so suspend casts are over; the
/// records of incarnations that left their zone are dropped.
pub(crate) fn begin_turn(state: &mut GameState, player: PlayerId) {
    let Some(s1e) = read_state(state) else {
        return;
    };
    if s1e.is_empty() {
        return;
    }
    let bound = s1e
        .bound
        .iter()
        .copied()
        .filter(|record| {
            record.holder != player && link_live_in(state, record.host, Zone::Battlefield)
        })
        .collect::<Vec<_>>();
    let hasty_permanents = s1e
        .hasty_permanents
        .iter()
        .copied()
        .filter(|&(host, caster, _)| {
            link_live_in(state, host, Zone::Battlefield)
                && state.objects.get(host.object).controller == caster
        })
        .collect::<Vec<_>>();
    let s1e = s1e_state(state);
    s1e.bound = bound;
    s1e.suspend_casts.clear();
    s1e.hasty_spells.clear();
    s1e.hasty_permanents = hasty_permanents;
}

// ---- Effect leaves ------------------------------------------------------------------

/// The opponent's creatures, as incarnation bindings.
fn opponent_creatures(state: &GameState, controller: PlayerId) -> Vec<EffectObjectBinding> {
    let opponent = controller.opponent();
    battlefield(state)
        .filter(|&object| {
            state.objects.get(object).controller == opponent
                && crate::engine::object_has_type(state, object, CardType::Creature)
        })
        .map(|object| bind(state, object))
        .collect()
}

/// Sacrifices every still-live permanent in `chosen` that `player`
/// controls, at once.
fn sacrifice_all(state: &mut GameState, player: PlayerId, chosen: &[EffectObjectBinding]) {
    let live = chosen
        .iter()
        .copied()
        .filter(|binding| {
            state.objects.try_get(binding.object).is_some_and(|object| {
                object.zone == binding.expected_zone
                    && object.zone_change_count == binding.expected_zone_change_count
                    && object.controller == player
            })
        })
        .collect::<Vec<_>>();
    for binding in &live {
        event::log_sacrifice(state, binding.object);
    }
    if !live.is_empty() {
        event::propose_and_commit_batch(
            state,
            live.iter()
                .map(|binding| ProposedEvent::zone_change(binding.object, Zone::Graveyard))
                .collect(),
        );
    }
}

/// Drives this module's choice-bearing leaves in the resumable interpreter.
pub(crate) fn drive(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    op: &S1eOpV1,
    path: Vec<u16>,
) -> Result<bool, String> {
    match op {
        S1eOpV1::PortalSacrificeV1 => {
            // "Each opponent sacrifices three creatures of their choice": the
            // opponent chooses three (all of them with three or fewer), and
            // they are sacrificed at once, as annihilator's are.
            let opponent = continuation.ctx.controller.opponent();
            let creatures = opponent_creatures(state, continuation.ctx.controller);
            if creatures.len() <= usize::from(PORTAL_SACRIFICES) {
                sacrifice_all(state, opponent, &creatures);
                return Ok(false);
            }
            let remaining = continuation.frames.clone();
            crate::eternal_cards_v1::e3::offer(
                continuation,
                E3ChoiceV1 {
                    kind: E3ChoiceKindV1::AnnihilatorV1,
                    player: opponent,
                    candidates: creatures
                        .iter()
                        .map(|binding| Target::Object(binding.object))
                        .collect(),
                    bindings: creatures,
                    min: PORTAL_SACRIFICES,
                    max: PORTAL_SACRIFICES,
                    ordered: false,
                    path,
                    remaining,
                },
            );
            Ok(true)
        }
        _ => {
            let ctx = continuation.ctx.clone();
            execute(op, &ctx, state);
            Ok(false)
        }
    }
}

pub(crate) fn execute(op: &S1eOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        S1eOpV1::BeastbinderV1 => {
            let spec = TargetSpec::StandardV1(
                StandardTargetV1::UpToOneOpponentArtifactCreatureOrPlaneswalkerV1,
            );
            let Some(object) = legal_object_target(ctx, state, 0, spec) else {
                return;
            };
            // "If it's a creature" as the ability resolves; the 2/2 then lasts
            // as long as the ability loss (XMage's resolution-time condition).
            let creature = crate::engine::object_has_type(state, object, CardType::Creature);
            let host = link(state, object);
            let timestamp = crate::engine::next_timestamp(state);
            s1e_state(state).bound.push(BoundV1 {
                host,
                holder: ctx.controller,
                timestamp,
                creature,
            });
        }
        S1eOpV1::SuspendV1 => {
            let Some(creature) = legal_object_target(ctx, state, 0, TargetSpec::Creature) else {
                return;
            };
            event::propose_and_commit(state, ProposedEvent::zone_change(creature, Zone::Exile));
            // A token ceases to exist in exile (704.5d). No catalog creature has
            // printed suspend, so the card always gains it.
            let exiled = state
                .objects
                .try_get(creature)
                .is_some_and(|live| live.zone == Zone::Exile && !live.v4.is_token);
            if exiled {
                crate::eternal_legacy_e7a_v1::add_suspended(state, creature, SUSPEND_TIME_COUNTERS);
            }
        }
        S1eOpV1::ShoalV1 => {
            let Some(creature) = legal_object_target(ctx, state, 0, TargetSpec::Creature) else {
                return;
            };
            let x = i32::from(ctx.x_value);
            if x == 0 {
                return;
            }
            let binding = bind(state, creature);
            crate::effect::install_temporary_boost(state, binding, -x, -x, Keywords::NONE);
        }
        // Staged by `drive`; the program always runs in the resumable
        // interpreter.
        S1eOpV1::PortalSacrificeV1 => halt(state, ctx.source),
        S1eOpV1::PortalReanimateV1 => {
            let spec = TargetSpec::StandardV1(StandardTargetV1::CreatureCardInAGraveyardV1);
            let Some(card) = legal_object_target(ctx, state, 0, spec) else {
                return;
            };
            // "It's a Phyrexian in addition to its other types" applies as it
            // enters (611.2e), as Valkyrie's Call's Angel does.
            let mut proposed =
                ProposedEvent::zone_change_to_battlefield_under_controller(card, ctx.controller);
            if let ProposedEvent::ZoneChange(change) = &mut proposed {
                change.battlefield_added_subtype_v1 = Some(Subtype::Phyrexian);
            }
            event::propose_and_commit(state, proposed);
        }
    }
}
