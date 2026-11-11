//! Card programs for the eternal catalog's Premodern sideboard wave P3b
//! (`docs/design/eternal_formats_support_v1.md`): the sideboard cards of
//! Terrageddon, Mono-Black Aggro, Survival, The Rock and Mono-Red Control.
//!
//! `eternal_cards_v1` forwards its effect leaves here, wave P2b's triggered
//! ability table falls through to `triggers_for` below,
//! the recipe tables of waves P3a and P2b fall through to
//! `build_eternal_p3b_v1.rs`, and the engine calls the hooks in `statics`
//! and `targets` only in `eternal-fixtures` builds. Every table is keyed by
//! a printed name that occurs only in the eternal registry. Behavior follows
//! each card's current Oracle text, checked against its XMage source.
//!
//! Primitives added here:
//! - an artifact that deals damage equal to its mana value to the ability's
//!   source after a destruction it may survive (Goblin Tinkerer);
//! - an attack trigger that bans the defending player's spells for the turn
//!   (Xantid Swarm), as Orim's Chant's record of wave E9a;
//! - cumulative upkeep with age counters and a life payment (Dystopia),
//!   and an edict at each upkeep on the player whose upkeep it is, with a
//!   color filter;
//! - exile a graveyard but its basic land cards, then every library card
//!   with an exiled card's name (Haunting Echoes);
//! - in `statics`: Gloom's taxes on white spells and on activated
//!   abilities of white enchantments, Engineered Plague's creature type and
//!   -1/-1, Mogg Salvage's free cast condition, and Dystopia's age counters;
//! - in `targets`: a nonblack creature with mana value exactly X
//!   (Plaguebearer).
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

pub mod statics;
pub mod targets;

use crate::card_def::{Subtype, Supertype, TargetSpec, CARD_DEFS};
use crate::effect::{
    CreatureSacrificeFilter, EffectCond, EffectContinuation, EffectFrame, EffectOp, ExecCtx,
    PlayerRef,
};
use crate::eternal_cards_v1::EternalOpV1;
use crate::eternal_legacy_e9a_v1::{link_live_in, live_link, TurnV1};
use crate::eternal_premodern_p1a_v1::{destroy, halt, legal_object_target, live_ability_source};
use crate::event::{self, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::mana::ManaColor;
use crate::standard_cards_v1::StandardOpV1;
use crate::state::{GameState, ObjectLinkV4, Target, Zone};
use crate::trigger::{TriggerCondition, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};

pub(crate) const DYSTOPIA: &str = "Dystopia";
pub(crate) const ENGINEERED_PLAGUE: &str = "Engineered Plague";
pub(crate) const GLOOM: &str = "Gloom";
pub(crate) const XANTID_SWARM: &str = "Xantid Swarm";

/// Per-game state of this module's cards. Absent from every snapshot until
/// one of them needs it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct P3bStateV1 {
    /// Age counters on each Dystopia incarnation (702.24a).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub age_counters: Vec<(ObjectLinkV4, u16)>,
    /// The creature type each Engineered Plague incarnation chose as it
    /// entered.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub plague_types: Vec<(ObjectLinkV4, Subtype)>,
}

impl P3bStateV1 {
    pub(crate) fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

pub(crate) fn p3b_state(state: &mut GameState) -> &mut P3bStateV1 {
    &mut state.eternal_v1.get_or_insert_with(Default::default).p3b
}

pub fn read_state(state: &GameState) -> Option<&P3bStateV1> {
    state.eternal_v1.as_ref().map(|eternal| &eternal.p3b)
}

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::P3bV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum P3bOpV1 {
    /// Goblin Tinkerer: destroy target artifact 0, which then deals damage
    /// equal to its mana value to the ability's source.
    TinkererV1,
    /// Xantid Swarm: the defending player can't cast spells this turn.
    XantidBanV1,
    /// Dystopia's cumulative upkeep: an age counter on the source.
    AddAgeCounterV1,
    /// Dystopia's cumulative upkeep: pay 1 life for each age counter or
    /// sacrifice the source. May be a choice.
    CumulativeUpkeepV1,
    /// Pay 1 life for each age counter on the source (the paying option).
    PayAgeLifeV1,
    /// Sacrifice the source (the other option).
    SacrificeSourceV1,
    /// Haunting Echoes on target player 0.
    HauntingEchoesV1,
}

impl P3bOpV1 {
    /// Whether this leaf can yield a player decision or pushes frames, so its
    /// program must use the resumable interpreter.
    pub(crate) fn contains_player_choice(&self) -> bool {
        matches!(self, Self::CumulativeUpkeepV1)
    }
}

fn p3b(op: P3bOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::P3bV1(op)))
}

fn push_frame(continuation: &mut EffectContinuation, op: EffectOp, path: &[u16]) {
    continuation.frames.push(EffectFrame::Program {
        op,
        path: path.to_vec(),
    });
}

/// `player` sacrifices `object` if they control it on the battlefield.
fn sacrifice(state: &mut GameState, object: ObjectId, player: PlayerId) {
    let live = state.objects.get(object);
    if live.zone != Zone::Battlefield || live.controller != player {
        return;
    }
    event::log_sacrifice(state, object);
    event::propose_and_commit(state, ProposedEvent::zone_change(object, Zone::Graveyard));
}

/// The printed name every copy of `object`'s card shares (201.2).
fn card_name(state: &GameState, object: ObjectId) -> &'static str {
    CARD_DEFS[usize::from(state.objects.get(object).card_def)].object_name
}

// ---- Programs -----------------------------------------------------------------------

/// Goblin Tinkerer: "{R}, {T}: Destroy target artifact. That artifact deals
/// damage equal to its mana value to this creature."
pub fn goblin_tinkerer() -> EffectOp {
    p3b(P3bOpV1::TinkererV1)
}

/// Haunting Echoes: "Exile all cards from target player's graveyard other
/// than basic land cards. For each card exiled this way, search that
/// player's library for all cards with the same name as that card and exile
/// them. Then that player shuffles."
pub fn haunting_echoes() -> EffectOp {
    p3b(P3bOpV1::HauntingEchoesV1)
}

// ---- Triggered abilities ----------------------------------------------------------------

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

/// Xantid Swarm: "Whenever this creature attacks, defending player can't
/// cast spells this turn."
fn xantid_attacks() -> EffectOp {
    p3b(P3bOpV1::XantidBanV1)
}

/// Dystopia: "Cumulative upkeep—Pay 1 life." (702.24a: "At the beginning of
/// your upkeep, if this permanent is on the battlefield, put an age counter
/// on this permanent. Then you may pay [cost] for each age counter on it. If
/// you don't, sacrifice it.")
fn dystopia_upkeep() -> EffectOp {
    EffectOp::Sequence(vec![
        p3b(P3bOpV1::AddAgeCounterV1),
        p3b(P3bOpV1::CumulativeUpkeepV1),
    ])
}

/// Dystopia: "At the beginning of each player's upkeep, that player
/// sacrifices a green or white permanent." The trigger is controlled by
/// Dystopia's controller, so that player is the controller on their own turn
/// and the opponent otherwise. The sacrifice choice is a branch of the
/// trigger's own program, as the engine's sacrifice choice requires.
fn dystopia_edict() -> EffectOp {
    EffectOp::Conditional {
        cond: EffectCond::ControllersTurnV1,
        then: Box::new(dystopia_sacrifice(PlayerRef::Controller)),
        else_: Box::new(dystopia_sacrifice(PlayerRef::Opponent)),
    }
}

/// `player` sacrifices a permanent that is green or white now.
fn dystopia_sacrifice(player: PlayerRef) -> EffectOp {
    EffectOp::SacrificeCreature {
        player,
        filter: CreatureSacrificeFilter::PermanentColorsV1(
            crate::card_def::mana_color_mask(ManaColor::G)
                | crate::card_def::mana_color_mask(ManaColor::W),
        ),
    }
}

const XANTID_SWARM_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Attacks, xantid_attacks)];
const DYSTOPIA_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(
        TriggerCondition::BeginningOfUpkeep {
            controller_only: true,
        },
        dystopia_upkeep,
    ),
    trigger(
        TriggerCondition::BeginningOfUpkeep {
            controller_only: false,
        },
        dystopia_edict,
    ),
];

/// Triggered abilities of this module's cards.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        DYSTOPIA => &DYSTOPIA_TRIGGERS,
        XANTID_SWARM => &XANTID_SWARM_TRIGGERS,
        _ => &[],
    }
}

/// Target specs of this module's triggered abilities: none of them targets.
pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    triggers_for(name)
        .iter()
        .any(|trigger| (trigger.effect)() == *effect)
        .then_some(TargetSpec::None)
}

// ---- Effect leaves ------------------------------------------------------------------

pub(crate) fn execute(op: &P3bOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        P3bOpV1::TinkererV1 => {
            let Some(artifact) = legal_object_target(ctx, state, 0, TargetSpec::ArtifactPermanent)
            else {
                return;
            };
            // "Its mana value" as it was destroyed: read before the
            // destruction, which it may survive (indestructible, regenerated).
            let mana_value = crate::engine::object_mana_value(state, artifact);
            destroy(ctx, state, artifact);
            let Some(tinkerer) = live_ability_source(ctx, state) else {
                return;
            };
            if mana_value > 0 {
                event::propose_and_commit(
                    state,
                    ProposedEvent::damage(
                        artifact,
                        Target::Object(tinkerer),
                        i32::from(mana_value),
                    ),
                );
            }
        }
        P3bOpV1::XantidBanV1 => {
            let player = ctx.controller.opponent();
            let turn = TurnV1::now(state);
            let e9a = crate::eternal_legacy_e9a_v1::e9a_state(state);
            if !e9a.cast_bans.contains(&(player, turn)) {
                e9a.cast_bans.push((player, turn));
            }
        }
        P3bOpV1::AddAgeCounterV1 => {
            let Some(source) = live_ability_source(ctx, state) else {
                return;
            };
            let link = live_link(state, source);
            let counters = &mut p3b_state(state).age_counters;
            match counters.iter_mut().find(|(host, _)| *host == link) {
                Some((_, count)) => *count = count.saturating_add(1),
                None => counters.push((link, 1)),
            }
        }
        P3bOpV1::PayAgeLifeV1 => {
            let Some(source) = live_ability_source(ctx, state) else {
                return;
            };
            let owed = i32::from(statics::age_counters(state, source));
            // 119.4: life can be paid only up to the life total.
            if state.players[ctx.controller.index()].life < owed {
                sacrifice(state, source, ctx.controller);
            } else if owed > 0 {
                event::propose_and_commit(state, ProposedEvent::life_payment(ctx.controller, owed));
            }
        }
        P3bOpV1::SacrificeSourceV1 => {
            if let Some(source) = live_ability_source(ctx, state) {
                sacrifice(state, source, ctx.controller);
            }
        }
        P3bOpV1::HauntingEchoesV1 => haunting_echoes_resolve(ctx, state),
        // Its choice runs in `drive`.
        P3bOpV1::CumulativeUpkeepV1 => halt(state, ctx.source),
    }
}

/// Haunting Echoes's resolution. Every matching library card is exiled: the
/// searching player doesn't choose to find fewer (701.19b).
fn haunting_echoes_resolve(ctx: &ExecCtx, state: &mut GameState) {
    let Some(Target::Player(player)) = ctx.targets.first().copied() else {
        return;
    };
    if !crate::engine::effect_target_is_legal(
        state,
        ctx.source,
        ctx.controller,
        TargetSpec::AnyPlayer,
        &ctx.targets,
        0,
    ) {
        return;
    }
    let basic_land = |state: &GameState, card: ObjectId| {
        let definition = &CARD_DEFS[usize::from(state.objects.get(card).card_def)];
        definition.is_land && definition.supertypes.contains(&Supertype::Basic)
    };
    let graveyard = state.players[player.index()]
        .graveyard
        .iter()
        .copied()
        .filter(|&card| !state.objects.get(card).v4.is_token && !basic_land(state, card))
        .collect::<Vec<_>>();
    exile_all(state, &graveyard);
    // "For each card exiled this way": the names of those now in exile.
    let mut names = graveyard
        .iter()
        .filter(|&&card| state.objects.get(card).zone == Zone::Exile)
        .map(|&card| card_name(state, card))
        .collect::<Vec<_>>();
    names.sort_unstable();
    names.dedup();
    let matching = state.players[player.index()]
        .library
        .iter()
        .copied()
        .filter(|&card| names.binary_search(&card_name(state, card)).is_ok())
        .collect::<Vec<_>>();
    exile_all(state, &matching);
    if state.shuffle_library(player).is_err() {
        halt(state, ctx.source);
    }
}

/// Exiles `cards` as one simultaneous event.
fn exile_all(state: &mut GameState, cards: &[ObjectId]) {
    if cards.is_empty() {
        return;
    }
    event::propose_and_commit_batch(
        state,
        cards
            .iter()
            .map(|&card| ProposedEvent::zone_change(card, Zone::Exile))
            .collect(),
    );
}

/// Drives this module's choice-bearing leaves in the resumable interpreter.
pub(crate) fn drive(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    op: &P3bOpV1,
    path: Vec<u16>,
) -> Result<bool, String> {
    let controller = continuation.ctx.controller;
    match op {
        P3bOpV1::CumulativeUpkeepV1 => {
            let ctx = continuation.ctx.clone();
            let Some(source) = live_ability_source(&ctx, state) else {
                return Ok(false);
            };
            let owed = i32::from(statics::age_counters(state, source));
            // 119.4: a player who can't pay the life sacrifices the source.
            if state.players[controller.index()].life < owed {
                sacrifice(state, source, controller);
                return Ok(false);
            }
            // "you may pay [cost] for each age counter": paying is the first
            // option.
            push_frame(
                continuation,
                EffectOp::Choice {
                    controller: PlayerRef::Controller,
                    options: vec![p3b(P3bOpV1::PayAgeLifeV1), p3b(P3bOpV1::SacrificeSourceV1)],
                },
                &path,
            );
            Ok(false)
        }
        other => {
            execute(other, &continuation.ctx, state);
            Ok(false)
        }
    }
}

// ---- Hooks ----------------------------------------------------------------------------

/// Drops the records of permanents that left the battlefield.
fn prune(state: &mut GameState) {
    let Some(p3b) = read_state(state) else {
        return;
    };
    if p3b.is_empty() {
        return;
    }
    let on_battlefield = |link: &ObjectLinkV4| link_live_in(state, *link, Zone::Battlefield);
    let age_counters = p3b
        .age_counters
        .iter()
        .copied()
        .filter(|(link, _)| on_battlefield(link))
        .collect::<Vec<_>>();
    let plague_types = p3b
        .plague_types
        .iter()
        .copied()
        .filter(|(link, _)| on_battlefield(link))
        .collect::<Vec<_>>();
    let p3b = p3b_state(state);
    p3b.age_counters = age_counters;
    p3b.plague_types = plague_types;
}

pub(crate) fn begin_turn(state: &mut GameState, _player: PlayerId) {
    prune(state);
}
