//! Vintage Cube wave C6a: discard, exile and impulse-draw trigger creatures.
//! Inti remains unsupported until all multi-card discard callers preserve
//! instruction boundaries: ordinary cleanup, Time Stop cleanup, Breakthrough,
//! and Cabal Therapy's legacy fallback still loop over single-card commits.
//!
//! Headliner Scarlett and Gandalf, Goblins' Bane remain Partial. Do not
//! attach their triggers or Flameshape until concealed play permissions
//! have a private projection. `rl::exile_play_permissions_public_v2` has
//! no observer argument and publishes `play_or_cast` for every permission:
//! a land and a spell exiled face down produce distinguishable opponent
//! observations, even when their object references use a neutral sentinel.
//! Shelldock Isle / Collector's Cage hideaway does not grant an ordinary
//! play permission. Its linked-card projection accepts only those sources,
//! and its lookers follow the source's controllers, unlike these cards.
//! `standard_exile::finish_hideaway` also sets the face-down marker after
//! the move records public event visibility. Memory Jar's explicit private
//! journal mask is needed for a concealed move, but does not fix the play
//! permission projection. Gandalf additionally needs an unbounded grant
//! conditional on controlling a Wizard at each play attempt. No ability
//! implementation is registered for either Partial card; the catalog's
//! ordinary-permanent fallback is not a claim of printed-text support.
//!
//! Black Widow, Super Spy remains NoEffect. PlayPermission has no mana
//! spending rider. The any-type payment grant in engine::tinybones_v72
//! requires that Partial card's definition, stash counters and live source;
//! it is not a reusable spell-scoped grant. Agatha's Soul Cauldron only
//! changes activation payments and excludes colorless symbols. Also, the
//! staged registry omits Spy, which has no native Subtype entry. Do not
//! register a reduced version of Black Widow's printed abilities.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

use crate::card_def::{card_id_by_name, CardType, TargetSpec, CARD_DEFS};
use crate::effect::{
    EffectContinuation, EffectFrame, EffectObjectBinding, EffectOp, ExecCtx, ImpulseDuration,
    ObjectRef, PlayerRef, TargetRef,
};
use crate::eternal_cards_v1::e3::{E3ChoiceKindV1, E3ChoiceV1};
use crate::eternal_cards_v1::{EternalOpV1, EternalTriggerV1};
use crate::eternal_legacy_e9a_v1::TurnV1;
use crate::event::{CommittedEvent, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::standard_cards_v1::{StandardOpV1, StandardTriggerV1};
use crate::state::{AbilitySourceContractV4, GameState, ObjectLinkV4, Target, Zone};
use crate::trigger::{TriggerCondition, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};

/// Wave-specific resolution leaves.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum C6aOpV1 {
    GauDamageV1,
    FallenShinobiHitV1,
    PlundererCreateV1,
    PlundererGiftV1,
    PlundererDamageV1,
    GutSacrificeV1,
}

impl C6aOpV1 {
    pub(crate) fn contains_player_choice(&self) -> bool {
        matches!(self, Self::GutSacrificeV1)
    }
}

const IVORA: &str = "Ivora, Insatiable Heir";
const GAU: &str = "Gau, Feral Youth";
const LAELIA: &str = "Laelia, the Blade Reforged";
const FALLEN_SHINOBI: &str = "Fallen Shinobi";
const PLUNDERER: &str = "Generous Plunderer";
const GUT: &str = "Gut, True Soul Zealot";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum C6aTriggerV1 {
    EachEndStepIfCardLeftYourGraveyardV1,
    CardsExiledFromYourLibraryOrGraveyardV1,
}

/// Graveyard departures must be remembered even before Gau enters.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct C6aStateV1 {
    pub graveyard_departures: [Option<TurnV1>; 2],
    /// Ouroboroid's incarnation-scoped last-known-power convention.
    pub departed_power: Vec<(ObjectLinkV4, i32)>,
    /// Event-log indices of additional exiles for an owner in one simultaneous batch.
    /// Cleared when that log is collected, rather than at the end of a resolution.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exile_batch_followers: Vec<usize>,
}

impl C6aStateV1 {
    pub(crate) fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Track only the batch's zone-change members, not incidental nested events.
pub(crate) struct ExileBatchV1 {
    start: usize,
    objects: Vec<ObjectId>,
}

impl ExileBatchV1 {
    pub(crate) fn capture(state: &GameState, events: &[ProposedEvent]) -> Self {
        let objects = events
            .iter()
            .filter_map(|event| {
                let ProposedEvent::ZoneChange(change) = event else {
                    return None;
                };
                let card = state.objects.get(change.object);
                (matches!(card.zone, Zone::Library | Zone::Graveyard)
                    && !card.v4.is_token
                    && card.spell_copy_origin.is_none())
                .then_some(change.object)
            })
            .collect();
        Self {
            start: state.engine.event_log.len(),
            objects,
        }
    }

    pub(crate) fn finish(self, state: &mut GameState) {
        if self.objects.len() < 2 {
            return;
        }
        let mut seen = [false; 2];
        let mut remaining = self.objects;
        let mut followers = Vec::new();
        for (index, event) in state.engine.event_log.iter().enumerate().skip(self.start) {
            let CommittedEvent::ZoneChange { object, .. } = event else {
                continue;
            };
            let Some(member) = remaining.iter().position(|candidate| candidate == object) else {
                continue;
            };
            // Only the member's first move belongs to this simultaneous instruction.
            remaining.swap_remove(member);
            let owner = state.objects.get(*object).owner;
            if qualifying_exile(state, event, owner) {
                if seen[owner.index()] {
                    followers.push(index);
                }
                seen[owner.index()] = true;
            }
        }
        if !followers.is_empty() {
            state
                .eternal_v1
                .get_or_insert_with(Default::default)
                .c6a
                .exile_batch_followers
                .extend(followers);
        }
    }
}

pub(crate) fn clear_collected_exile_batches(state: &mut GameState) {
    if let Some(eternal) = state.eternal_v1.as_mut() {
        eternal.c6a.exile_batch_followers.clear();
    }
}

/// Called after replacement selection, before the committed zone change.
/// `frozen_power` preserves the pre-batch value for simultaneous departures.
pub(crate) fn note_zone_change(
    state: &mut GameState,
    object: ObjectId,
    from: Zone,
    to: Zone,
    frozen_power: Option<i32>,
) {
    if from == to || !matches!(from, Zone::Graveyard | Zone::Battlefield) {
        return;
    }
    let live = state.objects.get(object);
    if from == Zone::Graveyard && !live.v4.is_token && live.spell_copy_origin.is_none() {
        let owner = live.owner;
        // A Gau anywhere in the game can enter after this departure.
        if state.objects.iter().any(|(_, card)| {
            let def = &CARD_DEFS[usize::from(card.card_def)];
            def.name == GAU && def.is_executable()
        }) {
            let now = TurnV1::now(state);
            state
                .eternal_v1
                .get_or_insert_with(Default::default)
                .c6a
                .graveyard_departures[owner.index()] = Some(now);
        }
    } else if from == Zone::Battlefield && CARD_DEFS[usize::from(live.card_def)].name == GAU {
        let link = ObjectLinkV4 {
            object,
            zone_change_count: live.zone_change_count,
        };
        let power = frozen_power.unwrap_or_else(|| crate::engine::effective_power(state, object));
        let entries = &mut state
            .eternal_v1
            .get_or_insert_with(Default::default)
            .c6a
            .departed_power;
        if !entries.iter().any(|(old, _)| *old == link) {
            entries.push((link, power));
        }
    }
}

fn card_left_graveyard(state: &GameState, player: PlayerId) -> bool {
    state.eternal_v1.as_ref().is_some_and(|eternal| {
        eternal.c6a.graveyard_departures[player.index()] == Some(TurnV1::now(state))
    })
}

/// Freeze Gau before any other simultaneous departure removes its boosts.
pub(crate) fn before_batch_departures(state: &mut GameState, events: &[ProposedEvent]) {
    let departing = events
        .iter()
        .filter_map(|event| {
            let ProposedEvent::ZoneChange(change) = event else {
                return None;
            };
            let live = state.objects.get(change.object);
            if live.zone != Zone::Battlefield
                || change.to_zone == Zone::Battlefield
                || CARD_DEFS[usize::from(live.card_def)].name != GAU
            {
                return None;
            }
            Some((
                change.object,
                change.to_zone,
                crate::engine::effective_power(state, change.object),
            ))
        })
        .collect::<Vec<_>>();
    for (object, to, power) in departing {
        note_zone_change(state, object, Zone::Battlefield, to, Some(power));
    }
}

fn gau_damage() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::C6aV1(
        C6aOpV1::GauDamageV1,
    )))
}

fn fallen_shinobi_hit() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::C6aV1(
        C6aOpV1::FallenShinobiHitV1,
    )))
}

fn c6a(op: C6aOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::C6aV1(op)))
}

/// Guide of Souls' optional action followed by a separately targeted
/// reflexive trigger. The opponent is not targeted until after creation.
fn plunderer_upkeep() -> EffectOp {
    EffectOp::Choice {
        controller: PlayerRef::Controller,
        options: vec![
            c6a(C6aOpV1::PlundererCreateV1),
            EffectOp::Sequence(Vec::new()),
        ],
    }
}

fn plunderer_damage() -> EffectOp {
    c6a(C6aOpV1::PlundererDamageV1)
}

fn gut_sacrifice() -> EffectOp {
    c6a(C6aOpV1::GutSacrificeV1)
}

/// "Another" qualifies creature only: an artifact Gut can sacrifice itself.
/// A returned source is a new permanent, so it also counts as another creature.
fn gut_candidate(state: &GameState, ctx: &ExecCtx, object: ObjectId) -> bool {
    let live = state.objects.get(object);
    let is_source = ctx.ability_source_contract.is_some_and(|source| {
        source.source == object && source.zone_change_count == live.zone_change_count
    });
    live.zone == Zone::Battlefield
        && live.controller == ctx.controller
        && (crate::engine::object_has_type(state, object, CardType::Artifact)
            || (!is_source && crate::engine::object_has_type(state, object, CardType::Creature)))
}

/// Balance's public E3 permanent selection, with zero meaning decline.
fn offer_gut_sacrifice(
    state: &GameState,
    continuation: &mut EffectContinuation,
    path: Vec<u16>,
) -> bool {
    let bindings = [PlayerId::P0, PlayerId::P1]
        .into_iter()
        .flat_map(|seat| state.players[seat.index()].battlefield.iter().copied())
        .filter(|&object| gut_candidate(state, &continuation.ctx, object))
        .map(|object| EffectObjectBinding {
            object,
            expected_zone: Zone::Battlefield,
            expected_zone_change_count: state.objects.get(object).zone_change_count,
        })
        .collect::<Vec<_>>();
    if bindings.is_empty() {
        return false;
    }
    let choice = E3ChoiceV1 {
        kind: E3ChoiceKindV1::C6aGutSacrificeV1,
        player: continuation.ctx.controller,
        candidates: bindings
            .iter()
            .map(|card| Target::Object(card.object))
            .collect(),
        bindings,
        min: 0,
        max: 1,
        ordered: false,
        path,
        remaining: continuation.frames.clone(),
    };
    crate::eternal_cards_v1::e3::offer(continuation, choice);
    true
}

pub(crate) fn finish_gut_sacrifice(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    choice: &E3ChoiceV1,
    bindings: Vec<EffectObjectBinding>,
) -> Result<bool, String> {
    let Some(card) = bindings.first() else {
        return Ok(false);
    };
    if state.objects.get(card.object).zone_change_count != card.expected_zone_change_count
        || !gut_candidate(state, &continuation.ctx, card.object)
    {
        return Ok(false);
    }
    // Aang's Iceberg's sacrifice convention: replacement destinations still
    // pay the cost, but a permanent that never leaves has not been sacrificed.
    crate::event::log_sacrifice(state, card.object);
    crate::event::propose_and_commit(
        state,
        ProposedEvent::zone_change(card.object, Zone::Graveyard),
    );
    if state.objects.get(card.object).zone != Zone::Battlefield {
        // Adeline's Full-card convention: this shared op enters attacking
        // the defending player, without a separate planeswalker choice.
        continuation.frames.push(EffectFrame::Program {
            op: EffectOp::CreateTokenTappedAndAttacking {
                token_def: card_id_by_name("Skeleton Token").expect("registered Skeleton Token"),
            },
            path: choice.path.clone(),
        });
    }
    Ok(false)
}

/// Ragavan's damaged opponent convention in this two-player engine, with
/// Golos's batch exile and ordinary-timing free-play permission. As with
/// Golos, the permission chooses Play for a playable land, otherwise Cast.
fn exile_opponents_top_two(state: &mut GameState, ctx: &ExecCtx) {
    let top = state.players[ctx.controller.opponent().index()]
        .library
        .iter()
        .take(2)
        .copied()
        .collect::<Vec<_>>();
    if top.is_empty() {
        return;
    }
    crate::event::propose_and_commit_batch(
        state,
        top.iter()
            .map(|&object| ProposedEvent::zone_change(object, Zone::Exile))
            .collect(),
    );
    for object in top {
        let live = state.objects.get(object);
        if live.zone != Zone::Exile {
            continue;
        }
        let def = &CARD_DEFS[usize::from(live.card_def)];
        let play_or_cast = if def.is_playable_land() {
            crate::engine::PlayOrCast::Play
        } else if def.is_castable() {
            crate::engine::PlayOrCast::Cast
        } else {
            continue;
        };
        let zone_change_generation = live.zone_change_count;
        state
            .engine
            .exile_play_permissions
            .push(crate::engine::PlayPermission {
                without_mana_cost: crate::engine::FreeCastV1(true),
                any_mana_type: Default::default(),
                object,
                holder: ctx.controller,
                zone_change_generation,
                play_or_cast,
                expiry: crate::engine::PlayPermissionExpiry::EndOfTurn,
            });
    }
}

/// Experimental Synthesizer's identical impulse permission.
fn laelia_impulse() -> EffectOp {
    EffectOp::ImpulseDraw {
        count: 1,
        duration: ImpulseDuration::EndOfTurn,
    }
}

const fn condition(kind: C6aTriggerV1) -> TriggerCondition {
    TriggerCondition::StandardV1(StandardTriggerV1::EternalV1(EternalTriggerV1::C6aV1(kind)))
}

fn qualifying_exile(state: &GameState, event: &CommittedEvent, player: PlayerId) -> bool {
    let CommittedEvent::ZoneChange {
        object,
        from: Zone::Library | Zone::Graveyard,
        to: Zone::Exile,
        ..
    } = event
    else {
        return false;
    };
    state.objects.try_get(*object).is_some_and(|card| {
        card.owner == player && !card.v4.is_token && card.spell_copy_origin.is_none()
    })
}

pub(crate) fn trigger_matches(
    kind: C6aTriggerV1,
    events: &[CommittedEvent],
    index: usize,
    source: ObjectId,
    state: &GameState,
) -> bool {
    // The Eternal trigger dispatcher already checks executable battlefield
    // presence and that the source's printed abilities remain active.
    let controller = state.objects.get(source).controller;
    match kind {
        C6aTriggerV1::EachEndStepIfCardLeftYourGraveyardV1 => {
            matches!(events[index], CommittedEvent::BeginningEndStep { .. })
                && card_left_graveyard(state, controller)
        }
        C6aTriggerV1::CardsExiledFromYourLibraryOrGraveyardV1 => {
            // Collection can include several separate instructions. Only actual
            // simultaneous batch members share a trigger, across both source zones.
            qualifying_exile(state, &events[index], controller)
                && !state
                    .eternal_v1
                    .as_ref()
                    .is_some_and(|eternal| eternal.c6a.exile_batch_followers.contains(&index))
        }
    }
}

/// Put the resolving creature spell onto the battlefield.
pub fn permanent() -> EffectOp {
    EffectOp::MoveObject {
        object: ObjectRef::ThisSource,
        to_zone: Zone::Battlefield,
    }
}

/// Bloodtithe Harvester's entry effect, also used for combat player damage.
fn blood() -> EffectOp {
    EffectOp::CreateToken {
        token_def: card_id_by_name("Blood Token").expect("registered Blood Token"),
        controller: PlayerRef::Controller,
    }
}

fn ivora_counter() -> EffectOp {
    EffectOp::AddPlusOneCounterToAbilitySource
}

/// Alesha's attack-counter binding also works if the source stops being a
/// creature before resolution, and never follows it through a zone change.
fn source_counter() -> EffectOp {
    EffectOp::BindPlusOnePlusOneCounterToTriggerSource
}

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

const IVORA_TRIGGERS: [TriggeredAbilityDef; 3] = [
    trigger(TriggerCondition::Etb, blood),
    trigger(TriggerCondition::DealsCombatDamageToPlayer, blood),
    trigger(TriggerCondition::ControllerDiscardsCardV1, ivora_counter),
];

const GAU_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(TriggerCondition::Attacks, source_counter),
    trigger(
        condition(C6aTriggerV1::EachEndStepIfCardLeftYourGraveyardV1),
        gau_damage,
    ),
];

const LAELIA_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(TriggerCondition::Attacks, laelia_impulse),
    trigger(
        condition(C6aTriggerV1::CardsExiledFromYourLibraryOrGraveyardV1),
        source_counter,
    ),
];

const FALLEN_SHINOBI_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(
    TriggerCondition::DealsCombatDamageToPlayer,
    fallen_shinobi_hit,
)];

const PLUNDERER_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(
        TriggerCondition::BeginningOfUpkeep {
            controller_only: true,
        },
        plunderer_upkeep,
    ),
    trigger(TriggerCondition::Attacks, plunderer_damage),
];

const GUT_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::ControllerAttacks, gut_sacrifice)];

/// Capture observers before the discard moves any card, as Currency Converter
/// does. This counts cards regardless of their replacement destination and
/// retains each observer's original controller and battlefield incarnation.
/// These triggers disclose no identity from the discarded hand.
pub(crate) fn note_discards(state: &mut GameState, cards: &[ObjectId]) {
    for player in [PlayerId::P0, PlayerId::P1] {
        let sources = crate::eternal_e5a_v1::active_permanents(state, IVORA, Some(player))
            .map(|object| AbilitySourceContractV4::capture(state, object))
            .collect::<Vec<_>>();
        if sources.is_empty() {
            continue;
        }
        let count = cards
            .iter()
            .filter(|&&card| {
                state.players[player.index()].hand.contains(&card)
                    && state.objects.get(card).zone == Zone::Hand
            })
            .count();
        for source in sources {
            for _ in 0..count {
                state
                    .engine
                    .pending_triggers
                    .push(crate::eternal_cards_v1::delayed_trigger(
                        source,
                        player,
                        ivora_counter(),
                    ));
            }
        }
    }
}

pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        IVORA => &IVORA_TRIGGERS,
        GAU => &GAU_TRIGGERS,
        LAELIA => &LAELIA_TRIGGERS,
        FALLEN_SHINOBI => &FALLEN_SHINOBI_TRIGGERS,
        PLUNDERER => &PLUNDERER_TRIGGERS,
        GUT => &GUT_TRIGGERS,
        _ => crate::eternal_cube_c6b_v1::triggers_for(name),
    }
}

pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    if name == PLUNDERER && *effect == c6a(C6aOpV1::PlundererGiftV1) {
        return Some(TargetSpec::TargetOpponent);
    }
    let known = match name {
        IVORA => *effect == blood() || *effect == ivora_counter(),
        FALLEN_SHINOBI => *effect == fallen_shinobi_hit(),
        PLUNDERER => *effect == plunderer_upkeep() || *effect == plunderer_damage(),
        GUT => *effect == gut_sacrifice(),
        GAU | LAELIA => {
            (name == GAU && *effect == gau_damage())
                || (name == LAELIA && *effect == laelia_impulse())
                || matches!(
                    effect,
                    EffectOp::BindPlusOnePlusOneCounterToTriggerSource
                        | EffectOp::PutPlusOnePlusOneCounterOnBoundObject { .. }
                )
        }
        _ => false,
    };
    known.then_some(TargetSpec::None)
}

pub(crate) fn delayed_effect_matches(name: &str, effect: &EffectOp) -> bool {
    (name == IVORA && *effect == ivora_counter())
        || (name == PLUNDERER && *effect == c6a(C6aOpV1::PlundererGiftV1))
}

pub(crate) fn execute(op: &C6aOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        C6aOpV1::PlundererCreateV1 => {
            crate::effect::execute(
                &EffectOp::CreateToken {
                    token_def: card_id_by_name("Treasure Token")
                        .expect("registered Treasure Token"),
                    controller: PlayerRef::Controller,
                },
                ctx,
                state,
            );
            if let Some(source) = ctx.ability_source_contract {
                let mut pending = crate::eternal_cards_v1::delayed_trigger(
                    source,
                    ctx.controller,
                    c6a(C6aOpV1::PlundererGiftV1),
                );
                pending.target_spec = TargetSpec::TargetOpponent;
                state.engine.pending_triggers.push(pending);
            }
        }
        C6aOpV1::PlundererGiftV1 => {
            let Some(Target::Player(player)) = ctx.targets.first().copied() else {
                return;
            };
            let Some(source) = ctx.ability_source_contract else {
                return;
            };
            if !ctx.target_incarnation_matches(0, state)
                || !crate::engine::effect_target_is_legal_from_ability_source(
                    state,
                    source,
                    ctx.controller,
                    TargetSpec::TargetOpponent,
                    &ctx.targets,
                    0,
                )
            {
                return;
            }
            let mut event = ProposedEvent::create_token(
                card_id_by_name("Treasure Token").expect("registered Treasure Token"),
                player,
            );
            if let ProposedEvent::CreateToken(token) = &mut event {
                token.entry_tapped = true;
            }
            crate::event::propose_and_commit(state, event);
        }
        C6aOpV1::PlundererDamageV1 => {
            // Two-player defending-player convention, including when the
            // attacker attacks that opponent's planeswalker (Adeline).
            let count = [PlayerId::P0, PlayerId::P1]
                .into_iter()
                .flat_map(|seat| state.players[seat.index()].battlefield.iter().copied())
                .filter(|&object| {
                    let live = state.objects.get(object);
                    live.zone == Zone::Battlefield
                        && live.controller == ctx.controller.opponent()
                        && crate::engine::object_has_type(state, object, CardType::Artifact)
                })
                .count();
            crate::effect::execute(
                &EffectOp::DealDamage {
                    target: TargetRef::Opponent,
                    amount: i32::try_from(count).unwrap_or(i32::MAX),
                },
                ctx,
                state,
            );
        }
        // The resumable interpreter owns this leaf's selection.
        C6aOpV1::GutSacrificeV1 => {}
        C6aOpV1::FallenShinobiHitV1 => exile_opponents_top_two(state, ctx),
        C6aOpV1::GauDamageV1 => {
            if !card_left_graveyard(state, ctx.controller) {
                return;
            }
            let Some(source) = ctx.ability_source_contract else {
                return;
            };
            let link = ObjectLinkV4 {
                object: source.source,
                zone_change_count: source.zone_change_count,
            };
            let current = state.objects.try_get(link.object).is_some_and(|live| {
                live.zone == Zone::Battlefield && live.zone_change_count == link.zone_change_count
            });
            let power = if current {
                crate::engine::effective_power(state, link.object)
            } else {
                state
                    .eternal_v1
                    .as_ref()
                    .and_then(|eternal| {
                        eternal
                            .c6a
                            .departed_power
                            .iter()
                            .find(|(old, _)| *old == link)
                    })
                    .map_or(0, |(_, power)| *power)
            };
            if power > 0 {
                crate::effect::execute(
                    &EffectOp::DealDamage {
                        target: TargetRef::Opponent,
                        amount: power,
                    },
                    ctx,
                    state,
                );
            }
        }
    }
}

pub(crate) fn drive(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    op: &C6aOpV1,
    path: Vec<u16>,
) -> Result<bool, String> {
    if *op == C6aOpV1::GutSacrificeV1 {
        return Ok(offer_gut_sacrifice(state, continuation, path));
    }
    execute(op, &continuation.ctx, state);
    Ok(false)
}
