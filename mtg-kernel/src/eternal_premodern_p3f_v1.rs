//! Card programs for the eternal catalog's Premodern sideboard wave P3f
//! (`docs/design/eternal_formats_support_v1.md`): the sideboard cards that
//! complete Mono-Black Control.
//!
//! `eternal_cards_v1` forwards its effect leaves, queued triggers and turn
//! bookkeeping here, wave P3e's recipe tables fall through to
//! `build_eternal_p3f_v1.rs`, and the engine calls the hooks below only in
//! `eternal-fixtures` builds. Every table is keyed by a printed name that
//! occurs only in the eternal registry. Behavior follows each card's current
//! Oracle text and rulings, checked against its XMage source.
//!
//! Primitives added here:
//! - -2/-1 counters divided among one or two target creatures as the spell
//!   is announced (Contagion), with `targets`' filter;
//! - a trigger on creatures put into an opponent's graveyard from the
//!   battlefield, and a permanent that becomes a creature and stops being an
//!   enchantment (Lurking Skirge);
//! - an as-enters life payment recorded per incarnation and read by an X/X
//!   token ability (Phyrexian Processor).
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

pub(crate) mod targets;

use crate::card_def::{CardType, Keywords, Subtype, TargetSpec, CARD_DEFS};
use crate::effect::{EffectOp, ExecCtx, PlayerRef};
use crate::eternal_cards_v1::EternalOpV1;
use crate::eternal_legacy_e9a_v1::{
    active_permanents, battlefield, left_as_creature, link_live_in,
};
use crate::eternal_premodern_p1a_v1::{live_ability_source, queue_trigger};
use crate::event::{self, CommittedEvent, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::standard_cards_v1::{StandardOpV1, StandardTargetV1};
use crate::state::{AbilitySourceContractV4, GameState, ObjectLinkV4, Target, Zone};
use crate::trigger::PendingTrigger;
use serde::{Deserialize, Serialize};

pub(crate) const LURKING_SKIRGE: &str = "Lurking Skirge";
pub(crate) const PHYREXIAN_PROCESSOR: &str = "Phyrexian Processor";
pub(crate) const MINION_TOKEN: &str = "Phyrexian Minion Token";

/// Per-game state of this module's cards, kept inside
/// `eternal_cards_v1::EternalStateV1`. Absent from every snapshot until one
/// of them needs it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct P3fStateV1 {
    /// The -2/-1 counters on each battlefield incarnation that has any
    /// (Contagion).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub minus_two_minus_one: Vec<(ObjectLinkV4, u16)>,
    /// Each Lurking Skirge that became a creature, as its battlefield
    /// incarnation, with the change's timestamp.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skirges: Vec<(ObjectLinkV4, u64)>,
    /// The life each Phyrexian Processor incarnation paid as it entered.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub payments: Vec<(ObjectLinkV4, i32)>,
    /// Each Phyrexian Minion token's power and toughness, its X.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub minions: Vec<(ObjectLinkV4, i32)>,
}

impl P3fStateV1 {
    pub(crate) fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

pub(crate) fn p3f_state(state: &mut GameState) -> &mut P3fStateV1 {
    &mut state.eternal_v1.get_or_insert_with(Default::default).p3f
}

pub fn read_state(state: &GameState) -> Option<&P3fStateV1> {
    state.eternal_v1.as_ref().map(|eternal| &eternal.p3f)
}

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::P3fV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum P3fOpV1 {
    /// Contagion: put its divided -2/-1 counters on each target still legal.
    ContagionV1,
    /// Lurking Skirge's trigger: if it is still an enchantment, it becomes a
    /// 3/2 Phyrexian Imp creature with flying.
    SkirgeAnimateV1,
    /// Phyrexian Processor: create an X/X black Phyrexian Minion, X being the
    /// life the activating incarnation paid as it entered.
    ProcessorTokenV1,
}

impl P3fOpV1 {
    /// Whether this leaf can yield a player decision or pushes frames. None
    /// of this module's leaves does.
    pub(crate) fn contains_player_choice(&self) -> bool {
        false
    }
}

fn p3f(op: P3fOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::P3fV1(op)))
}

fn link(state: &GameState, object: ObjectId) -> ObjectLinkV4 {
    ObjectLinkV4 {
        object,
        zone_change_count: state.objects.get(object).zone_change_count,
    }
}

fn count_for<T: Copy>(entries: &[(ObjectLinkV4, T)], host: ObjectLinkV4) -> Option<T> {
    entries
        .iter()
        .find(|(entry, _)| *entry == host)
        .map(|(_, value)| *value)
}

/// `object`'s battlefield incarnation, if it is on the battlefield.
fn battlefield_link(state: &GameState, object: ObjectId) -> Option<ObjectLinkV4> {
    let live = state.objects.try_get(object)?;
    (live.zone == Zone::Battlefield).then_some(ObjectLinkV4 {
        object,
        zone_change_count: live.zone_change_count,
    })
}

// ---- Programs -----------------------------------------------------------------------

/// Contagion: "You may pay 1 life and exile a black card from your hand
/// rather than pay this spell's mana cost. Distribute two -2/-1 counters
/// among one or two target creatures." Divided as it is announced: two picks
/// of one counter (`targets`).
pub fn contagion() -> EffectOp {
    p3f(P3fOpV1::ContagionV1)
}

/// Phyrexian Processor: "{4}, {T}: Create an X/X black Phyrexian Minion
/// creature token, where X is the life paid as this artifact entered."
pub fn processor_token() -> EffectOp {
    p3f(P3fOpV1::ProcessorTokenV1)
}

fn skirge_trigger() -> EffectOp {
    p3f(P3fOpV1::SkirgeAnimateV1)
}

// ---- Statics: Contagion's counters --------------------------------------------------

/// The -2/-1 counters on `object`'s battlefield incarnation.
pub fn minus_two_minus_one_counters(state: &GameState, object: ObjectId) -> u16 {
    let Some(host) = battlefield_link(state, object) else {
        return 0;
    };
    read_state(state)
        .and_then(|p3f| count_for(&p3f.minus_two_minus_one, host))
        .unwrap_or(0)
}

/// The change to `object`'s power and toughness from its -2/-1 counters
/// (layer 7c, with the other modifications).
pub(crate) fn counter_boost(state: &GameState, object: ObjectId) -> (i128, i128) {
    if read_state(state).is_none_or(|p3f| p3f.minus_two_minus_one.is_empty()) {
        return (0, 0);
    }
    let counters = i128::from(minus_two_minus_one_counters(state, object));
    (-2 * counters, -counters)
}

fn add_counters(state: &mut GameState, object: ObjectId, count: u16) {
    let Some(host) = battlefield_link(state, object) else {
        return;
    };
    let counters = &mut p3f_state(state).minus_two_minus_one;
    match counters.iter_mut().find(|(entry, _)| *entry == host) {
        Some((_, existing)) => *existing = existing.saturating_add(count),
        None => counters.push((host, count)),
    }
}

// ---- Statics: Lurking Skirge --------------------------------------------------------

/// The timestamp at which `object`'s battlefield incarnation became Lurking
/// Skirge's 3/2 Phyrexian Imp, if it has.
fn skirge_timestamp(state: &GameState, object: ObjectId) -> Option<u64> {
    let p3f = read_state(state)?;
    if p3f.skirges.is_empty() {
        return None;
    }
    count_for(&p3f.skirges, battlefield_link(state, object)?)
}

/// Card-type changes from this module: a Lurking Skirge that became a
/// creature is a creature and nothing else ("becomes a 3/2 Phyrexian Imp
/// creature" replaces its card types, 205.1a), as Oko's Elks are.
pub(crate) fn has_type_override(
    state: &GameState,
    object: ObjectId,
    card_type: CardType,
) -> Option<bool> {
    skirge_timestamp(state, object).map(|_| card_type == CardType::Creature)
}

/// The Skirge's base power and toughness 3/2, a layer 7b setting with the
/// change's timestamp.
pub(crate) fn base_power_toughness(state: &GameState, object: ObjectId) -> Option<(i16, i16, u64)> {
    skirge_timestamp(state, object).map(|timestamp| (3, 2, timestamp))
}

/// The Skirge's flying, unless an effect with a later timestamp removed
/// its abilities (layer 6 in timestamp order: an Elk, Dress Down).
pub(crate) fn granted_keywords(state: &GameState, object: ObjectId) -> Keywords {
    let Some(timestamp) = skirge_timestamp(state, object) else {
        return Keywords::NONE;
    };
    if crate::continuous_characteristics_v1::removal_timestamp(state, object)
        .is_some_and(|removal| removal > timestamp)
    {
        return Keywords::NONE;
    }
    Keywords::FLYING
}

/// Lurking Skirge: "When a creature is put into an opponent's graveyard from
/// the battlefield, if this permanent is an enchantment, ..." Each permanent
/// that left the battlefield for a graveyard as a creature, tokens included,
/// triggers every Lurking Skirge whose abilities apply, that is an
/// enchantment now, and whose controller is an opponent of the creature's
/// owner (the graveyard is its owner's).
fn note_deaths(state: &mut GameState, events: &[CommittedEvent]) {
    for event in events {
        let CommittedEvent::ZoneChange {
            object,
            from: Zone::Battlefield,
            to: Zone::Graveyard,
            ..
        } = *event
        else {
            continue;
        };
        let Some(live) = state.objects.try_get(object) else {
            continue;
        };
        if !left_as_creature(state, object, live.zone_change_count.saturating_sub(1)) {
            continue;
        }
        let owner = live.owner;
        let skirges = active_permanents(state, LURKING_SKIRGE, Some(owner.opponent()))
            .filter(|&skirge| crate::engine::object_has_type(state, skirge, CardType::Enchantment))
            .collect::<Vec<_>>();
        for skirge in skirges {
            let source = AbilitySourceContractV4::capture(state, skirge);
            let controller = state.objects.get(skirge).controller;
            queue_trigger(state, source, controller, skirge_trigger(), None);
        }
    }
}

// ---- Statics: Phyrexian Processor and its Minions -----------------------------------

/// The Phyrexian Processor that has just entered and has not paid its life.
fn processor_needing_payment(state: &GameState) -> Option<ObjectId> {
    let p3f = read_state(state);
    battlefield(state).find(|&object| {
        let live = state.objects.get(object);
        live.v4.face_index == 0
            && CARD_DEFS
                .get(usize::from(live.card_def))
                .is_some_and(|def| def.name == PHYREXIAN_PROCESSOR && def.is_executable())
            && p3f.is_none_or(|p3f| count_for(&p3f.payments, link(state, object)).is_none())
    })
}

/// The most life `player` can pay: their life total, or 0 (119.4).
fn payable_life(state: &GameState, player: PlayerId) -> i32 {
    state.players[player.index()]
        .life
        .clamp(0, i32::from(u16::MAX - 1))
}

/// "As this artifact enters, pay any amount of life": the pending payment,
/// option `n` paying `n` life, from 0 to the controller's life total.
pub(crate) fn entry_choice_decision(state: &GameState) -> Option<crate::engine::Decision> {
    let processor = processor_needing_payment(state)?;
    let player = state.objects.get(processor).controller;
    Some(crate::engine::Decision::ChooseEffectOption {
        player,
        source: processor,
        option_count: u16::try_from(payable_life(state, player) + 1).unwrap_or(u16::MAX),
    })
}

/// Answers the pending Phyrexian Processor payment. `Ok(false)` when none is
/// paying.
pub(crate) fn answer_entry_choice(state: &mut GameState, option: u16) -> Result<bool, String> {
    let Some(processor) = processor_needing_payment(state) else {
        return Ok(false);
    };
    let player = state.objects.get(processor).controller;
    let amount = i32::from(option);
    if amount > payable_life(state, player) {
        return Err("the life payment is more than the player's life".to_string());
    }
    let host = link(state, processor);
    p3f_state(state).payments.push((host, amount));
    if amount > 0 {
        event::propose_and_commit(state, ProposedEvent::life_payment(player, amount));
    }
    Ok(true)
}

/// The life `processor`'s battlefield incarnation paid as it entered.
pub fn processor_payment(state: &GameState, processor: ObjectId) -> Option<i32> {
    let host = battlefield_link(state, processor)?;
    count_for(&read_state(state)?.payments, host)
}

/// A Phyrexian Minion token's power and toughness: the X its Processor's
/// ability recorded. Both stats.
pub(crate) fn characteristic_defining_power(state: &GameState, object: ObjectId) -> Option<i32> {
    let live = state.objects.try_get(object)?;
    if live.zone != Zone::Battlefield
        || CARD_DEFS
            .get(usize::from(live.card_def))
            .is_none_or(|def| def.name != MINION_TOKEN)
    {
        return None;
    }
    Some(
        read_state(state)
            .and_then(|p3f| count_for(&p3f.minions, link(state, object)))
            .unwrap_or(0),
    )
}

// ---- Queued triggers ----------------------------------------------------------------

/// The triggers this module's cards set off in this collection, queued into
/// wave P1a's queue.
pub(crate) fn delayed_triggers(
    state: &mut GameState,
    events: &[CommittedEvent],
) -> Vec<PendingTrigger> {
    if cfg!(feature = "eternal-fixtures") {
        note_deaths(state, events);
    }
    Vec::new()
}

/// Queued triggers owned by this module's cards.
pub(crate) fn delayed_effect_matches(name: &str, effect: &EffectOp) -> bool {
    name == LURKING_SKIRGE && *effect == skirge_trigger()
}

/// Target specs of this module's queued triggers. `None` for every other
/// card and effect.
pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    delayed_effect_matches(name, effect).then_some(TargetSpec::None)
}

// ---- Turn bookkeeping ---------------------------------------------------------------

/// As a turn begins the stack is empty, so no ability still needs the
/// records of incarnations that left the battlefield: their counters,
/// changes, payments and Minion sizes are dropped.
pub(crate) fn begin_turn(state: &mut GameState, _player: PlayerId) {
    let Some(p3f) = read_state(state) else {
        return;
    };
    let live = |host: &ObjectLinkV4| link_live_in(state, *host, Zone::Battlefield);
    let stale = p3f.minus_two_minus_one.iter().any(|(host, _)| !live(host))
        || p3f.skirges.iter().any(|(host, _)| !live(host))
        || p3f.payments.iter().any(|(host, _)| !live(host))
        || p3f.minions.iter().any(|(host, _)| !live(host));
    if !stale {
        return;
    }
    let keep = |entries: &[(ObjectLinkV4, i32)]| {
        entries
            .iter()
            .copied()
            .filter(|(host, _)| live(host))
            .collect::<Vec<_>>()
    };
    let counters = p3f
        .minus_two_minus_one
        .iter()
        .copied()
        .filter(|(host, _)| live(host))
        .collect::<Vec<_>>();
    let skirges = p3f
        .skirges
        .iter()
        .copied()
        .filter(|(host, _)| live(host))
        .collect::<Vec<_>>();
    let payments = keep(&p3f.payments);
    let minions = keep(&p3f.minions);
    let p3f = p3f_state(state);
    p3f.minus_two_minus_one = counters;
    p3f.skirges = skirges;
    p3f.payments = payments;
    p3f.minions = minions;
}

// ---- Effect leaves ------------------------------------------------------------------

pub(crate) fn execute(op: &P3fOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        P3fOpV1::ContagionV1 => {
            let spec = TargetSpec::StandardV1(StandardTargetV1::DividedTwoCountersCreaturesV1);
            let placed = crate::eternal_sideboard_s1b_v1::targets::division(&ctx.targets)
                .into_iter()
                .filter_map(|(target, count)| {
                    let Target::Object(object) = target else {
                        return None;
                    };
                    let legal = ctx.targets.iter().enumerate().any(|(index, &slot)| {
                        slot == target
                            && ctx.target_incarnation_matches(index, state)
                            && crate::engine::effect_target_is_legal(
                                state,
                                ctx.source,
                                ctx.controller,
                                spec,
                                &ctx.targets,
                                index,
                            )
                    });
                    (legal && count > 0).then(|| (object, u16::try_from(count).unwrap_or(0)))
                })
                .collect::<Vec<_>>();
            for (object, count) in placed {
                add_counters(state, object, count);
            }
        }
        P3fOpV1::SkirgeAnimateV1 => {
            // "if this permanent is an enchantment" again on resolution (603.4).
            let Some(skirge) = live_ability_source(ctx, state) else {
                return;
            };
            if !crate::engine::object_has_type(state, skirge, CardType::Enchantment) {
                return;
            }
            let host = link(state, skirge);
            let timestamp = crate::engine::next_timestamp(state);
            let mut subtypes = vec![Subtype::Phyrexian.stable_id(), Subtype::Imp.stable_id()];
            subtypes.sort_unstable();
            state.objects.get_mut(skirge).v4.effective_subtype_ids = subtypes;
            let skirges = &mut p3f_state(state).skirges;
            skirges.retain(|(entry, _)| *entry != host);
            skirges.push((host, timestamp));
        }
        P3fOpV1::ProcessorTokenV1 => {
            // X is the life the activating incarnation paid, read even if it
            // has left the battlefield since.
            let Some(contract) = ctx.ability_source_contract else {
                return;
            };
            let host = ObjectLinkV4 {
                object: contract.source,
                zone_change_count: contract.zone_change_count,
            };
            let x = read_state(state)
                .and_then(|p3f| count_for(&p3f.payments, host))
                .unwrap_or(0);
            let Some(token_def) = crate::card_def::card_id_by_name(MINION_TOKEN) else {
                return;
            };
            let start = state.engine.event_history.len();
            crate::effect::execute(
                &EffectOp::CreateToken {
                    token_def,
                    controller: PlayerRef::Controller,
                },
                ctx,
                state,
            );
            let created = state
                .engine
                .event_history
                .range_v2(start..)
                .iter()
                .filter_map(|event| match event {
                    CommittedEvent::CreateToken { object, .. } => Some(*object),
                    _ => None,
                })
                .collect::<Vec<_>>();
            for token in created {
                if state.objects.get(token).zone == Zone::Battlefield {
                    let host = link(state, token);
                    p3f_state(state).minions.push((host, x));
                }
            }
        }
    }
}
