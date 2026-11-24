//! Card programs and engine hooks for the eternal catalog's Pauper field wave
//! Pf2c (`docs/design/eternal_formats_support_v1.md`): the three cards that
//! each add a new rules piece.
//!
//! - **Benevolent Unicorn**: "If a spell would deal damage to a permanent or
//!   player, it deals that much damage minus 1 to that permanent or player
//!   instead." A CR 614.1a replacement effect (NOT a prevention effect, so
//!   "damage can't be prevented" never suppresses it) applying to spell damage
//!   to any permanent or player regardless of controller. Participates in both
//!   the ordinary damage replacement pass (`event.rs`) and affected-player
//!   ordering (`eternal_pauper_pf1c_v1::ordinary_damage` /
//!   `eternal_prevention_order_v1`).
//! - **Boulderbranch Golem**: "Prototype {3}{G} -- 3/3. When this creature
//!   enters, you gain life equal to its power." Implements CR 718 prototype
//!   casting (`AltCostCondition::PrototypeV1`): on the stack and on the
//!   battlefield (including copies of a prototyped permanent), a prototyped
//!   permanent has its prototype mana cost, mana value (4), color (green) and
//!   base power/toughness (3/3); in all other zones and when entering without
//!   having been cast prototyped (e.g. blinked/flickered or reanimated), it
//!   uses its normal colorless 6/5 MV 7 profile. Its ETB trigger reads
//!   effective power on resolution or last known power if it departed early.
//! - **Inside Out**: "Switch target creature's power and toughness until end
//!   of turn. Draw a card." Implements CR 613.4d Layer 7d P/T switching:
//!   applied after all Layer 7a/7b/7c modifiers (including counters and buffs
//!   applied later in the same turn), with parity tracking so two switches
//!   cancel.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

use crate::card_def::{card_id_by_name, AltCostCondition, CardDef, TargetSpec, CARD_DEFS};
use crate::effect::{EffectOp, ExecCtx, PlayerRef};
use crate::eternal_cards_v1::EternalOpV1;
use crate::eternal_legacy_e9a_v1::{live_link, TurnV1};
use crate::eternal_premodern_p3a_v1::statics::active_with_id;
use crate::event::{DamageProposed, ProposedEvent};
use crate::ids::ObjectId;
use crate::mana::ManaColor;
use crate::standard_cards_v1::StandardOpV1;
use crate::state::{CastMethodV4, GameState, ObjectLinkV4, StackItemKind, Target, Zone};
use crate::trigger::{TriggerCondition, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};

pub(crate) const BENEVOLENT_UNICORN: &str = "Benevolent Unicorn";
pub(crate) const BOULDERBRANCH_GOLEM: &str = "Boulderbranch Golem";

/// Alternate Layer-1 characteristics of a prototyped spell or permanent
/// (CR 718.3b-c).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PrototypeProfileV1 {
    pub mana_value: u16,
    pub color_mask: u8,
    pub power: i32,
    pub toughness: i32,
}

/// Prototype profile of `def` when cast prototyped, if `def` is a prototype
/// card.
pub(crate) fn prototype_profile(def: &CardDef) -> Option<PrototypeProfileV1> {
    if !def.is_executable() {
        return None;
    }
    match def.name {
        BOULDERBRANCH_GOLEM => Some(PrototypeProfileV1 {
            mana_value: 4,
            color_mask: crate::card_def::mana_color_mask(ManaColor::G),
            power: 3,
            toughness: 3,
        }),
        _ => None,
    }
}

/// Per-game state for this module's cards, kept inside
/// `eternal_cards_v1::EternalStateV1`. Default (and absent from snapshots and
/// state hashes) until one of these cards needs it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pf2cStateV1 {
    /// Battlefield incarnations that entered as prototyped spells (or copied
    /// a prototyped permanent).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prototyped_permanents: Vec<ObjectLinkV4>,
    /// Last-known power captured when a Boulderbranch Golem leaves the
    /// battlefield, read by its ETB trigger if it departed before resolution.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub departed_power: Vec<(ObjectLinkV4, i32)>,
    /// Active "switch power and toughness until end of turn" effects (CR
    /// 613.4d). One entry per resolved switch effect so even counts cancel.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pt_switches: Vec<(ObjectLinkV4, TurnV1)>,
    /// Prototyped permanents temporarily overwritten by Saheeli, Sublime
    /// Artificer's -2 copy effect, restored when that temporary copy ends.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub saheeli_prev_prototyped: Vec<ObjectLinkV4>,
    /// Transient flag set during `create_token_copy` when copying a prototyped
    /// permanent so the new token receives prototype status before ETB
    /// triggers collect.
    #[serde(skip)]
    pub pending_token_prototyped: bool,
}

impl Pf2cStateV1 {
    pub(crate) fn is_empty(&self) -> bool {
        self.prototyped_permanents.is_empty()
            && self.departed_power.is_empty()
            && self.pt_switches.is_empty()
            && self.saheeli_prev_prototyped.is_empty()
    }
}

impl std::hash::Hash for Pf2cStateV1 {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        if !self.is_empty() {
            b"eternal-pauper-pf2c/v1".hash(state);
            self.prototyped_permanents.hash(state);
            self.departed_power.hash(state);
            self.pt_switches.hash(state);
            self.saheeli_prev_prototyped.hash(state);
        }
    }
}

fn pf2c_state(state: &mut GameState) -> &mut Pf2cStateV1 {
    &mut state.eternal_v1.get_or_insert_with(Default::default).pf2c
}

fn read_state(state: &GameState) -> Option<&Pf2cStateV1> {
    state.eternal_v1.as_ref().map(|eternal| &eternal.pf2c)
}

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::Pf2cV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Pf2cOpV1 {
    /// Boulderbranch Golem ETB: gain life equal to this creature's power (or
    /// last-known power if it left the battlefield before resolution).
    BoulderbranchEtbLifeV1,
    /// Inside Out: switch target creature's power and toughness until end of
    /// turn (CR 613.4d).
    SwitchTargetPtUntilEndOfTurnV1,
}

impl Pf2cOpV1 {
    pub(crate) fn contains_player_choice(&self) -> bool {
        false
    }
}

fn pf2c(op: Pf2cOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::Pf2cV1(op)))
}

fn definition(state: &GameState, object: ObjectId) -> Option<&'static CardDef> {
    let live = state.objects.try_get(object)?;
    CARD_DEFS.get(live.card_def as usize)
}

fn unicorn_id() -> Option<u16> {
    static ID: std::sync::OnceLock<Option<u16>> = std::sync::OnceLock::new();
    *ID.get_or_init(|| card_id_by_name(BENEVOLENT_UNICORN))
}

// ---- Programs -----------------------------------------------------------------------

/// Inside Out: "Switch target creature's power and toughness until end of
/// turn. Draw a card."
pub fn inside_out() -> EffectOp {
    EffectOp::Sequence(vec![
        pf2c(Pf2cOpV1::SwitchTargetPtUntilEndOfTurnV1),
        EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 1,
        },
    ])
}

/// Boulderbranch Golem ETB: "you gain life equal to its power."
fn boulderbranch_etb_life() -> EffectOp {
    pf2c(Pf2cOpV1::BoulderbranchEtbLifeV1)
}

// ---- Triggered abilities ------------------------------------------------------------

const BOULDERBRANCH_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::Etb,
    home_zone: Zone::Battlefield,
    intervening_if_kicked: false,
    intervening_if_controls_another_source_card: false,
    face_index: 0,
    effect: boulderbranch_etb_life,
}];

/// Triggered abilities of this module's cards, the tail of the trigger chain.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        BOULDERBRANCH_GOLEM => &BOULDERBRANCH_TRIGGERS,
        _ => &[],
    }
}

/// Target specs of this module's printed and queued triggered abilities.
pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    if name == BOULDERBRANCH_GOLEM && *effect == boulderbranch_etb_life() {
        return Some(TargetSpec::None);
    }
    None
}

/// Queued triggers owned by this module's cards: none.
pub(crate) fn delayed_effect_matches(_name: &str, _effect: &EffectOp) -> bool {
    false
}

// ---- Benevolent Unicorn (CR 614.1a spell damage replacement) ------------------------

/// Whether `damage` is proposed damage from a spell (not combat damage and not
/// an activated or triggered ability).
pub(crate) fn is_spell_damage(state: &GameState, damage: &DamageProposed) -> bool {
    if damage.is_combat
        || damage.amount <= 0
        || damage.historical_source.is_some()
        || damage.eternal_damage_lki_v1.is_some()
        || crate::eternal_premodern_p3d_v1::resolving_ability_source(state).is_some()
    {
        return false;
    }
    let valid_recipient = match damage.target {
        Target::Player(_) => true,
        Target::Object(id) => state
            .objects
            .try_get(id)
            .is_some_and(|live| live.zone == Zone::Battlefield),
        Target::StackItem(_) => false,
    };
    valid_recipient
        && state
            .objects
            .try_get(damage.source)
            .is_some_and(|live| live.zone == Zone::Stack)
}

/// Active Benevolent Unicorns on either battlefield whose replacement applies
/// to `damage`. Because Benevolent Unicorn is a replacement effect (CR 614.1a)
/// rather than a prevention effect (CR 615.1a), "damage can't be prevented"
/// does not suppress it.
pub(crate) fn applicable_unicorns<'a>(
    state: &'a GameState,
    damage: &DamageProposed,
) -> impl Iterator<Item = ObjectId> + 'a {
    let applies = is_spell_damage(state, damage);
    applies
        .then_some(())
        .into_iter()
        .flat_map(move |()| active_with_id(state, unicorn_id(), BENEVOLENT_UNICORN, None))
}

/// Coarse check for replacement-ordering preflight: whether any active
/// Benevolent Unicorn is on either battlefield.
pub(crate) fn has_benevolent_unicorn(state: &GameState) -> bool {
    active_with_id(state, unicorn_id(), BENEVOLENT_UNICORN, None)
        .next()
        .is_some()
}

/// Direct replacement pass for `event.rs` when no affected-player choice is
/// needed: reduces spell damage by 1 for each active Benevolent Unicorn.
/// Returns `true` if the damage is reduced to 0 (no damage event occurs).
pub(crate) fn unicorn_replaces_damage(state: &GameState, damage: &mut DamageProposed) -> bool {
    let unicorns = applicable_unicorns(state, damage).count();
    if unicorns == 0 {
        return false;
    }
    let reduced = i32::try_from(unicorns).unwrap_or(i32::MAX);
    damage.amount = damage.amount.saturating_sub(reduced).max(0);
    damage.amount == 0
}

// ---- Boulderbranch Golem (CR 718 Prototype) -----------------------------------------

/// Whether `object` on the stack is currently cast as a prototyped spell.
pub(crate) fn is_prototyped_stack_spell(state: &GameState, object: ObjectId) -> bool {
    let Some(live) = state.objects.try_get(object) else {
        return false;
    };
    if live.zone != Zone::Stack {
        return false;
    }
    let Some(def) = CARD_DEFS.get(usize::from(live.card_def)) else {
        return false;
    };
    if prototype_profile(def).is_none()
        || !def
            .alt_cost
            .is_some_and(|alt| alt.condition == AltCostCondition::PrototypeV1)
    {
        return false;
    }
    live.v4
        .spell_cast_origin
        .and_then(|origin| origin.finalized_method)
        == Some(CastMethodV4::Alternative)
        || state.stack.iter().any(|item| {
            item.kind == StackItemKind::Spell
                && item.source == object
                && item.v4.cast_method == Some(CastMethodV4::Alternative)
        })
}

/// Whether `object` on the battlefield is currently a prototyped permanent.
pub(crate) fn is_prototyped_permanent(state: &GameState, object: ObjectId) -> bool {
    let Some(pf2c) = read_state(state) else {
        return false;
    };
    if pf2c.prototyped_permanents.is_empty() {
        return false;
    }
    let Some(live) = state.objects.try_get(object) else {
        return false;
    };
    live.zone == Zone::Battlefield
        && pf2c
            .prototyped_permanents
            .contains(&live_link(state, object))
}

/// Records that `object` just entered the battlefield as a prototyped spell
/// (called right after `materialize_zone_change_v1`, before ETB triggers
/// collect).
pub(crate) fn note_prototyped_entry(state: &mut GameState, object: ObjectId) {
    let Some(live) = state.objects.try_get(object) else {
        return;
    };
    if live.zone != Zone::Battlefield {
        return;
    }
    let Some(profile) = definition(state, object).and_then(prototype_profile) else {
        return;
    };
    let link = live_link(state, object);
    state.objects.get_mut(object).v4.effective_color_mask = profile.color_mask;
    let list = &mut pf2c_state(state).prototyped_permanents;
    list.retain(|entry| entry.object != object);
    list.push(link);
}

/// Removes any prototype marker on `object` (e.g. when a copy reverts or
/// copies a non-prototyped permanent).
pub(crate) fn clear_prototyped(state: &mut GameState, object: ObjectId) {
    if let Some(eternal) = state.eternal_v1.as_mut() {
        eternal
            .pf2c
            .prototyped_permanents
            .retain(|entry| entry.object != object);
    }
}

/// Saves `object`'s current prototype status before a temporary Saheeli copy
/// overwrites it (only when not already saved for this incarnation).
pub(crate) fn save_saheeli_prototype(state: &mut GameState, object: ObjectId) {
    if !is_prototyped_permanent(state, object) {
        return;
    }
    let link = live_link(state, object);
    let saved = &mut pf2c_state(state).saheeli_prev_prototyped;
    if !saved.contains(&link) {
        saved.push(link);
    }
}

/// Restores `object`'s pre-Saheeli prototype status when a temporary Saheeli
/// copy ends at cleanup or on departure.
pub(crate) fn restore_saheeli_prototype(state: &mut GameState, object: ObjectId) {
    let Some(live) = state.objects.try_get(object) else {
        return;
    };
    let link = ObjectLinkV4 {
        object,
        zone_change_count: live.zone_change_count,
    };
    let was_prototyped = state.eternal_v1.as_mut().is_some_and(|eternal| {
        if let Some(pos) = eternal
            .pf2c
            .saheeli_prev_prototyped
            .iter()
            .position(|entry| *entry == link)
        {
            eternal.pf2c.saheeli_prev_prototyped.remove(pos);
            true
        } else {
            false
        }
    });
    if was_prototyped {
        note_prototyped_entry(state, object);
    } else {
        clear_prototyped(state, object);
    }
}

/// Stages or clears the transient prototype flag for a token copy being
/// created by `eternal_cube_c8b_v1::create_token_copy`.
pub(crate) fn set_pending_token_prototyped(state: &mut GameState, prototyped: bool) {
    if prototyped {
        pf2c_state(state).pending_token_prototyped = true;
    } else if let Some(eternal) = state.eternal_v1.as_mut() {
        eternal.pf2c.pending_token_prototyped = false;
    }
}

/// Marks a newly created token copy as prototyped during token entry in
/// `event.rs`, before entry observers and ETB triggers run.
pub(crate) fn apply_pending_token_prototyped(state: &mut GameState, object: ObjectId) {
    if read_state(state).is_some_and(|pf2c| pf2c.pending_token_prototyped) {
        note_prototyped_entry(state, object);
    }
}

/// Prototype mana value of a battlefield permanent (4 for a prototyped
/// Boulderbranch Golem). Stack spells read `prototype_stack_mana_value`.
pub(crate) fn prototype_battlefield_mana_value(state: &GameState, object: ObjectId) -> Option<u16> {
    if !is_prototyped_permanent(state, object) {
        return None;
    }
    definition(state, object)
        .and_then(prototype_profile)
        .map(|profile| profile.mana_value)
}

/// Prototype mana value of a spell on the stack cast via Prototype.
pub(crate) fn prototype_stack_mana_value(
    def: &CardDef,
    cast_method: Option<CastMethodV4>,
) -> Option<u16> {
    if cast_method != Some(CastMethodV4::Alternative)
        || !def
            .alt_cost
            .is_some_and(|alt| alt.condition == AltCostCondition::PrototypeV1)
    {
        return None;
    }
    prototype_profile(def).map(|profile| profile.mana_value)
}

/// Prototype color mask of a spell on the stack or a permanent on the
/// battlefield (CR 718.3b-c).
pub(crate) fn prototype_color_mask(state: &GameState, object: ObjectId) -> Option<u8> {
    if !is_prototyped_stack_spell(state, object) && !is_prototyped_permanent(state, object) {
        return None;
    }
    definition(state, object)
        .and_then(prototype_profile)
        .map(|profile| profile.color_mask)
}

/// Prototype base power and toughness of a permanent on the battlefield (or a
/// spell on the stack, if queried).
pub(crate) fn prototype_base_pt(state: &GameState, object: ObjectId) -> Option<(i32, i32)> {
    if !is_prototyped_permanent(state, object) && !is_prototyped_stack_spell(state, object) {
        return None;
    }
    definition(state, object)
        .and_then(prototype_profile)
        .map(|profile| (profile.power, profile.toughness))
}

/// Freeze Boulderbranch Golem's power before any simultaneous departure removes
/// boosts (CR 608.2h LKI for its ETB life gain).
pub(crate) fn before_batch_departures(state: &mut GameState, events: &[ProposedEvent]) {
    let departing: Vec<(ObjectLinkV4, i32)> = events
        .iter()
        .filter_map(|event| {
            let ProposedEvent::ZoneChange(change) = event else {
                return None;
            };
            let live = state.objects.try_get(change.object)?;
            if live.zone != Zone::Battlefield
                || change.to_zone == Zone::Battlefield
                || CARD_DEFS
                    .get(usize::from(live.card_def))
                    .is_none_or(|def| def.name != BOULDERBRANCH_GOLEM)
            {
                return None;
            }
            Some((
                live_link(state, change.object),
                crate::engine::effective_power(state, change.object),
            ))
        })
        .collect();
    if departing.is_empty() {
        return;
    }
    let entries = &mut pf2c_state(state).departed_power;
    for (link, power) in departing {
        if !entries.iter().any(|(old, _)| *old == link) {
            entries.push((link, power));
        }
    }
}

/// Capture Boulderbranch Golem's last-known power on single departure
/// (before `materialize_zone_change_v1` clears prototype state).
pub(crate) fn note_departure(
    state: &mut GameState,
    object: ObjectId,
    from: Zone,
    to_zone: Zone,
    frozen_power: Option<i32>,
) {
    if from != Zone::Battlefield || to_zone == Zone::Battlefield {
        return;
    }
    let Some(live) = state.objects.try_get(object) else {
        return;
    };
    if CARD_DEFS
        .get(usize::from(live.card_def))
        .is_some_and(|def| def.name == BOULDERBRANCH_GOLEM)
    {
        let link = live_link(state, object);
        let power = frozen_power.unwrap_or_else(|| crate::engine::effective_power(state, object));
        let entries = &mut pf2c_state(state).departed_power;
        if !entries.iter().any(|(old, _)| *old == link) {
            entries.push((link, power));
        }
    }
}

// ---- Inside Out (CR 613.4d Layer 7d P/T switch) -------------------------------------

/// Whether `object` currently has an odd number of active power/toughness
/// switch effects this turn (CR 613.4d).
pub(crate) fn is_pt_switched(state: &GameState, object: ObjectId) -> bool {
    let Some(pf2c) = read_state(state) else {
        return false;
    };
    if pf2c.pt_switches.is_empty() {
        return false;
    }
    let Some(live) = state.objects.try_get(object) else {
        return false;
    };
    if live.zone != Zone::Battlefield {
        return false;
    }
    let link = live_link(state, object);
    let now = TurnV1::now(state);
    let count = pf2c
        .pt_switches
        .iter()
        .filter(|&&(entry, turn)| entry == link && turn == now)
        .count();
    count % 2 == 1
}

/// Clears end-of-turn P/T switches during the cleanup step.
pub(crate) fn cleanup_end_of_turn(state: &mut GameState) {
    if let Some(eternal) = state.eternal_v1.as_mut() {
        eternal.pf2c.pt_switches.clear();
    }
}

/// Prunes dead prototype markers, expired switches, and departed LKI power
/// records at turn boundaries.
pub(crate) fn begin_turn(state: &mut GameState, _player: crate::ids::PlayerId) {
    let Some(pf2c) = read_state(state) else {
        return;
    };
    if pf2c.is_empty() {
        return;
    }
    let now = TurnV1::now(state);
    let live_proto: Vec<ObjectLinkV4> = pf2c
        .prototyped_permanents
        .iter()
        .copied()
        .filter(|link| {
            state.objects.try_get(link.object).is_some_and(|live| {
                live.zone == Zone::Battlefield && live.zone_change_count == link.zone_change_count
            })
        })
        .collect();
    let live_switches: Vec<(ObjectLinkV4, TurnV1)> = pf2c
        .pt_switches
        .iter()
        .copied()
        .filter(|&(link, turn)| {
            turn == now
                && state.objects.try_get(link.object).is_some_and(|live| {
                    live.zone == Zone::Battlefield
                        && live.zone_change_count == link.zone_change_count
                })
        })
        .collect();
    let live_saheeli_proto: Vec<ObjectLinkV4> = pf2c
        .saheeli_prev_prototyped
        .iter()
        .copied()
        .filter(|link| {
            state.objects.try_get(link.object).is_some_and(|live| {
                live.zone == Zone::Battlefield && live.zone_change_count == link.zone_change_count
            })
        })
        .collect();
    let has_pending_golem_trigger = state.stack.iter().any(|item| {
        item.kind == StackItemKind::TriggeredAbility
            && item.inline_effect.as_ref() == Some(&boulderbranch_etb_life())
    });
    let eternal = state.eternal_v1.as_mut().expect("pf2c state present");
    eternal.pf2c.prototyped_permanents = live_proto;
    eternal.pf2c.pt_switches = live_switches;
    eternal.pf2c.saheeli_prev_prototyped = live_saheeli_proto;
    if !has_pending_golem_trigger {
        eternal.pf2c.departed_power.clear();
    }
}

// ---- Execution ----------------------------------------------------------------------

pub(crate) fn execute(op: &Pf2cOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        Pf2cOpV1::BoulderbranchEtbLifeV1 => {
            let link = if let Some(source) = ctx.ability_source_contract {
                ObjectLinkV4 {
                    object: source.source,
                    zone_change_count: source.zone_change_count,
                }
            } else {
                live_link(state, ctx.source)
            };
            let on_battlefield = state.objects.try_get(link.object).is_some_and(|live| {
                live.zone == Zone::Battlefield && live.zone_change_count == link.zone_change_count
            });
            let power = if on_battlefield {
                crate::engine::effective_power(state, link.object)
            } else {
                read_state(state)
                    .and_then(|pf2c| {
                        pf2c.departed_power
                            .iter()
                            .find(|(old, _)| *old == link)
                            .map(|(_, power)| *power)
                    })
                    .unwrap_or(0)
            };
            if power > 0 {
                crate::effect::execute(
                    &EffectOp::GainLife {
                        player: PlayerRef::Controller,
                        amount: power,
                    },
                    ctx,
                    state,
                );
            }
        }
        Pf2cOpV1::SwitchTargetPtUntilEndOfTurnV1 => {
            if !ctx.target_incarnation_matches(0, state)
                || !crate::engine::effect_target_is_legal(
                    state,
                    ctx.source,
                    ctx.controller,
                    TargetSpec::Creature,
                    &ctx.targets,
                    0,
                )
            {
                return;
            }
            let Some(Target::Object(creature)) = ctx.targets.first().copied() else {
                return;
            };
            let entry = (live_link(state, creature), TurnV1::now(state));
            pf2c_state(state).pt_switches.push(entry);
        }
    }
}
