//! Card programs for the eternal catalog's Legacy sideboard cards, part 2
//! (wave S1b, issue #365, `docs/design/eternal_formats_support_v1.md`).
//!
//! `eternal_cards_v1` forwards this module's effect leaves, and the engine
//! calls the hooks below only in `eternal-fixtures` builds. Every table is
//! keyed by a printed name that occurs only in the eternal registry, so the
//! Pauper, FDN and Standard catalogs never reach it. Behavior was read from
//! each card's current Oracle text and its XMage source.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

use crate::card_def::{card_id_by_name, CardDef, CardType, Keywords, TargetSpec, CARD_DEFS};
use crate::effect::{
    EffectContinuation, EffectFrame, EffectObjectBinding, EffectOp, ExecCtx, ObjectRef, PlayerRef,
    TargetRef,
};
use crate::eternal_cards_v1::EternalOpV1;
use crate::event::{self, CommittedEvent, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::mana::ManaColor;
use crate::standard_cards_v1::StandardOpV1;
use crate::state::{GameState, LinkedExileRecordV4, ObjectLinkV4, Target, Zone};
use crate::trigger::{PendingTrigger, TriggerCondition, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};

/// Per-game state for this module's cards, kept inside
/// `eternal_cards_v1::EternalStateV1`. Default (and absent from snapshots)
/// until one of these cards needs it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct S1bStateV1 {
    /// The turn (its key and active player) the noncreature spell counts
    /// below belong to (Deafening Silence).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cast_turn: Option<(u32, PlayerId)>,
    /// Noncreature spells each player has cast this turn.
    #[serde(default, skip_serializing_if = "is_zero_pair")]
    pub noncreature_casts: [u8; 2],
    /// Meddling Mages and the definition whose name each chose.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mage_names: Vec<(ObjectLinkV4, u16)>,
}

fn is_zero_pair(pair: &[u8; 2]) -> bool {
    *pair == [0, 0]
}

impl S1bStateV1 {
    pub(crate) fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

fn s1b_state(state: &mut GameState) -> &mut S1bStateV1 {
    &mut state.eternal_v1.get_or_insert_with(Default::default).s1b
}

fn read_state(state: &GameState) -> Option<&S1bStateV1> {
    state.eternal_v1.as_ref().map(|eternal| &eternal.s1b)
}

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::S1bV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum S1bOpV1 {
    /// Debt to the Deathless: each opponent loses two times X life; the
    /// controller gains life equal to the life lost this way.
    DebtV1,
    /// Peer into the Abyss: target player draws cards equal to half the
    /// cards in their library and loses half their life, rounding up each
    /// time. The draws are frames, so dredge may replace them.
    PeerV1,
    /// Whipflare: 2 damage to each nonartifact creature.
    WhipflareV1,
    /// Sudden Demise: X damage to each creature of `color`.
    SuddenDemiseV1 { color: ManaColor },
    /// Deafening Clarion, first mode: 3 damage to each creature.
    ClarionDamageV1,
    /// Deafening Clarion, second mode: creatures the controller controls
    /// gain lifelink until end of turn.
    ClarionLifelinkV1,
    /// Paradox Engine: untap all nonland permanents the controller controls.
    ParadoxUntapV1,
    /// Temporary Lockdown: exile each nonland permanent with mana value 2 or
    /// less until this leaves the battlefield.
    LockdownV1,
    /// Toxic Deluge: all creatures get -X/-X until end of turn.
    DelugeV1,
    /// Meddling Mage enters, choosing the name of definition `named`.
    MageNamesV1 { named: u16 },
}

impl S1bOpV1 {
    /// Whether this leaf can yield a player decision, so its program must
    /// use the resumable interpreter.
    pub(crate) fn contains_player_choice(&self) -> bool {
        matches!(self, Self::PeerV1)
    }
}

// ---- Names and helpers --------------------------------------------------------------

const GOBLIN_TOKEN: &str = "Goblin Token";
pub(crate) const PARADOX_ENGINE: &str = "Paradox Engine";
pub(crate) const STEEL_WRECKING_BALL: &str = "Steel Wrecking Ball";
pub(crate) const STONY_SILENCE: &str = "Stony Silence";
pub(crate) const TEMPORARY_LOCKDOWN: &str = "Temporary Lockdown";
const DEAFENING_SILENCE: &str = "Deafening Silence";
const GADDOCK_TEEG: &str = "Gaddock Teeg";
const MEDDLING_MAGE: &str = "Meddling Mage";
const SAILORS_BANE: &str = "Sailors' Bane";
const TOXIC_DELUGE: &str = "Toxic Deluge";

fn s1b(op: S1bOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::S1bV1(op)))
}

fn token_id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name} in CARD_DEFS"))
}

fn bind(state: &GameState, object: ObjectId) -> EffectObjectBinding {
    let live = state.objects.get(object);
    EffectObjectBinding {
        object,
        expected_zone: live.zone,
        expected_zone_change_count: live.zone_change_count,
    }
}

/// Every battlefield object, in each player's battlefield order.
fn battlefield(state: &GameState) -> impl Iterator<Item = ObjectId> + '_ {
    [PlayerId::P0, PlayerId::P1]
        .into_iter()
        .flat_map(move |player| state.players[player.index()].battlefield.iter().copied())
}

fn is_creature(state: &GameState, object: ObjectId) -> bool {
    crate::engine::object_has_type(state, object, CardType::Creature)
}

fn push_frame(continuation: &mut EffectContinuation, op: EffectOp, path: &[u16]) {
    continuation.frames.push(EffectFrame::Program {
        op,
        path: path.to_vec(),
    });
}

fn damage_batch(state: &mut GameState, source: ObjectId, targets: Vec<ObjectId>, amount: i32) {
    if amount <= 0 || targets.is_empty() {
        return;
    }
    let events = targets
        .into_iter()
        .map(|object| ProposedEvent::damage(source, Target::Object(object), amount))
        .collect::<Vec<_>>();
    event::propose_and_commit_batch(state, events);
}

// ---- Spells -------------------------------------------------------------------------

/// Empty the Warrens: "Create two 1/1 red Goblin creature tokens." Storm is
/// `eternal_cards_v1::e3`'s.
pub fn empty_the_warrens() -> EffectOp {
    let goblin = EffectOp::CreateToken {
        token_def: token_id(GOBLIN_TOKEN),
        controller: PlayerRef::Controller,
    };
    EffectOp::Sequence(vec![goblin.clone(), goblin])
}

/// Debt to the Deathless.
pub fn debt_to_the_deathless() -> EffectOp {
    s1b(S1bOpV1::DebtV1)
}

/// Peer into the Abyss.
pub fn peer_into_the_abyss() -> EffectOp {
    s1b(S1bOpV1::PeerV1)
}

/// Whipflare.
pub fn whipflare() -> EffectOp {
    s1b(S1bOpV1::WhipflareV1)
}

/// Destroy target artifact (Shenanigans, Steel Wrecking Ball's discard
/// ability).
pub fn destroy_target() -> EffectOp {
    EffectOp::DestroyObject {
        object: ObjectRef::Target(0),
    }
}

/// Deafening Clarion's three printed choices of "one or both".
pub fn clarion_damage() -> EffectOp {
    s1b(S1bOpV1::ClarionDamageV1)
}
pub fn clarion_lifelink() -> EffectOp {
    s1b(S1bOpV1::ClarionLifelinkV1)
}
pub fn clarion_both() -> EffectOp {
    EffectOp::Sequence(vec![clarion_damage(), clarion_lifelink()])
}

/// Sudden Demise: "Choose a color. Sudden Demise deals X damage to each
/// creature of the chosen color."
pub fn sudden_demise() -> EffectOp {
    EffectOp::Choice {
        controller: PlayerRef::Controller,
        options: [
            ManaColor::W,
            ManaColor::U,
            ManaColor::B,
            ManaColor::R,
            ManaColor::G,
        ]
        .into_iter()
        .map(|color| s1b(S1bOpV1::SuddenDemiseV1 { color }))
        .collect(),
    }
}

// ---- Triggered abilities -------------------------------------------------------------

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

/// Steel Wrecking Ball: "it deals 5 damage to target creature."
fn wrecking_ball_damage() -> EffectOp {
    EffectOp::DealDamage {
        target: TargetRef::Target(0),
        amount: 5,
    }
}

fn paradox_untap() -> EffectOp {
    s1b(S1bOpV1::ParadoxUntapV1)
}

fn lockdown() -> EffectOp {
    s1b(S1bOpV1::LockdownV1)
}

const STEEL_WRECKING_BALL_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, wrecking_ball_damage)];
const PARADOX_ENGINE_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::CastSpell, paradox_untap)];
const TEMPORARY_LOCKDOWN_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, lockdown)];

/// Triggered abilities of this module's cards.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        STEEL_WRECKING_BALL => &STEEL_WRECKING_BALL_TRIGGERS,
        PARADOX_ENGINE => &PARADOX_ENGINE_TRIGGERS,
        TEMPORARY_LOCKDOWN => &TEMPORARY_LOCKDOWN_TRIGGERS,
        _ => &[],
    }
}

/// Target specs of this module's triggered abilities. `None` for every
/// other card.
pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    let own = triggers_for(name)
        .iter()
        .any(|trigger| (trigger.effect)() == *effect);
    if !own {
        return None;
    }
    Some(match name {
        STEEL_WRECKING_BALL => TargetSpec::Creature,
        _ => TargetSpec::None,
    })
}

// ---- Effect leaves ------------------------------------------------------------------

pub(crate) fn execute(op: &S1bOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        S1bOpV1::DebtV1 => {
            let x = i32::from(ctx.x_value);
            if x == 0 {
                return;
            }
            let opponent = ctx.controller.opponent();
            let start = state.engine.event_history.len();
            event::propose_and_commit(state, ProposedEvent::life_loss(opponent, 2 * x));
            let lost: i32 = state.engine.event_history[start..]
                .iter()
                .filter_map(|event| match event {
                    CommittedEvent::LifeLoss { player, amount } if *player == opponent => {
                        Some((*amount).max(0))
                    }
                    _ => None,
                })
                .sum();
            event::propose_and_commit(state, ProposedEvent::life_gain(ctx.controller, lost));
        }
        S1bOpV1::PeerV1 => panic!("Peer into the Abyss requires resumable resolution"),
        S1bOpV1::WhipflareV1 => {
            let targets = battlefield(state)
                .filter(|&object| {
                    is_creature(state, object)
                        && !crate::engine::object_has_type(state, object, CardType::Artifact)
                })
                .collect();
            damage_batch(state, ctx.source, targets, 2);
        }
        S1bOpV1::SuddenDemiseV1 { color } => {
            let bit = 1_u8 << color.pool_index();
            let targets = battlefield(state)
                .filter(|&object| {
                    is_creature(state, object)
                        && crate::engine::object_color_mask(state, object) & bit != 0
                })
                .collect();
            damage_batch(state, ctx.source, targets, i32::from(ctx.x_value));
        }
        S1bOpV1::ClarionDamageV1 => {
            let targets = battlefield(state)
                .filter(|&object| is_creature(state, object))
                .collect();
            damage_batch(state, ctx.source, targets, 3);
        }
        S1bOpV1::ClarionLifelinkV1 => {
            let creatures = battlefield(state)
                .filter(|&object| {
                    state.objects.get(object).controller == ctx.controller
                        && is_creature(state, object)
                })
                .collect::<Vec<_>>();
            for creature in creatures {
                let binding = bind(state, creature);
                crate::effect::install_temporary_boost(state, binding, 0, 0, Keywords::LIFELINK);
            }
        }
        S1bOpV1::ParadoxUntapV1 => {
            let permanents = battlefield(state)
                .filter(|&object| {
                    state.objects.get(object).controller == ctx.controller
                        && !crate::engine::object_has_type(state, object, CardType::Land)
                })
                .collect::<Vec<_>>();
            for permanent in permanents {
                crate::engine::attempt_untap(state, permanent);
            }
        }
        S1bOpV1::LockdownV1 => lockdown_exile(ctx, state),
        S1bOpV1::DelugeV1 => {
            let x = i32::from(ctx.x_value);
            if x == 0 {
                return;
            }
            let creatures = battlefield(state)
                .filter(|&object| is_creature(state, object))
                .collect::<Vec<_>>();
            for creature in creatures {
                let binding = bind(state, creature);
                crate::effect::install_temporary_boost(state, binding, -x, -x, Keywords::NONE);
            }
        }
        S1bOpV1::MageNamesV1 { named } => {
            if state
                .objects
                .try_get(ctx.source)
                .is_some_and(|live| live.zone == Zone::Stack)
            {
                crate::effect::execute(
                    &EffectOp::MoveObject {
                        object: ObjectRef::ThisSource,
                        to_zone: Zone::Battlefield,
                    },
                    ctx,
                    state,
                );
            }
            let Some(live) = state.objects.try_get(ctx.source) else {
                return;
            };
            if live.zone != Zone::Battlefield {
                return;
            }
            let link = ObjectLinkV4 {
                object: ctx.source,
                zone_change_count: live.zone_change_count,
            };
            s1b_state(state).mage_names.push((link, *named));
        }
    }
}

/// Temporary Lockdown: exile each nonland permanent with mana value 2 or
/// less, linked to the source, which returns them when it leaves the
/// battlefield. Nothing happens if the source has already left.
fn lockdown_exile(ctx: &ExecCtx, state: &mut GameState) {
    let Some(contract) = ctx.ability_source_contract else {
        return;
    };
    let live = state.objects.try_get(ctx.source).is_some_and(|source| {
        source.zone == Zone::Battlefield
            && source.zone_change_count == contract.zone_change_count
            && source.card_def == contract.card_def
    });
    if !live {
        return;
    }
    let victims = battlefield(state)
        .filter(|&object| {
            !crate::engine::object_has_type(state, object, CardType::Land)
                && crate::engine::object_mana_value(state, object) <= 2
        })
        .collect::<Vec<_>>();
    let link = ObjectLinkV4 {
        object: ctx.source,
        zone_change_count: contract.zone_change_count,
    };
    for victim in victims {
        if state.objects.get(victim).zone != Zone::Battlefield {
            continue;
        }
        let owner = state.objects.get(victim).owner;
        let is_token = state.objects.get(victim).v4.is_token;
        event::propose_and_commit(state, ProposedEvent::zone_change(victim, Zone::Exile));
        let after = state.objects.get(victim);
        if after.zone != Zone::Exile || is_token {
            continue;
        }
        let record = LinkedExileRecordV4 {
            source: contract,
            exiled: victim,
            exiled_card_def: after.card_def,
            exiled_owner: owner,
            exiled_zone_change_count: after.zone_change_count,
        };
        state.engine.linked_exile_records.push(record);
        state.objects.get_mut(victim).v4.exiled_by = Some(link);
    }
}

/// Runs a leaf inside the resumable interpreter. `Ok(true)` when it staged a
/// choice and the continuation must suspend.
pub(crate) fn drive(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    op: &S1bOpV1,
    path: Vec<u16>,
) -> Result<bool, String> {
    match op {
        S1bOpV1::PeerV1 => {
            let Some(Target::Player(player)) = continuation.ctx.targets.first().copied() else {
                return Ok(false);
            };
            let cards = state.players[player.index()].library.len();
            let life = state.players[player.index()].life;
            let draw = u32::try_from(cards.div_ceil(2)).unwrap_or(u32::MAX);
            let lose = if life > 0 { (life + 1) / 2 } else { 0 };
            // Frames run last-in first-out: the draws, then the life loss.
            if lose > 0 {
                push_frame(
                    continuation,
                    EffectOp::LoseLife {
                        player: PlayerRef::Target(0),
                        amount: lose,
                    },
                    &path,
                );
            }
            if draw > 0 {
                push_frame(
                    continuation,
                    EffectOp::DrawCards {
                        player: PlayerRef::Target(0),
                        count: draw,
                    },
                    &path,
                );
            }
            Ok(false)
        }
        other => {
            execute(other, &continuation.ctx, state);
            Ok(false)
        }
    }
}

/// Whether the card named `name` exiles permanents until it leaves the
/// battlefield (see `effect::battlefield_exile_until_source_leaves`).
pub(crate) fn exiles_until_source_leaves(name: &str) -> bool {
    name == TEMPORARY_LOCKDOWN
}

// ---- Spells: Toxic Deluge and Meddling Mage -------------------------------------------

/// Toxic Deluge: "All creatures get -X/-X until end of turn." X is the life
/// paid as the additional cost (`pays_life_for_x`).
pub fn toxic_deluge() -> EffectOp {
    s1b(S1bOpV1::DelugeV1)
}

/// Every name a player may choose for Meddling Mage: each executable
/// nontoken, nonland definition's name once, grouped by first letter for a
/// two-step choice (as Anointed Peacekeeper's names are).
pub fn nameable_nonland() -> &'static [Vec<u16>] {
    static TABLE: std::sync::OnceLock<Vec<Vec<u16>>> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        crate::eternal_vintage_v1::nameable_definitions()
            .iter()
            .map(|group| {
                group
                    .iter()
                    .copied()
                    .filter(|&id| !CARD_DEFS[usize::from(id)].types.contains(&CardType::Land))
                    .collect::<Vec<_>>()
            })
            .filter(|group| !group.is_empty())
            .collect()
    })
}

/// Meddling Mage: "As this creature enters, choose a nonland card name."
/// The resolving creature spell enters and records the name together, so
/// no player acts between the two.
pub fn meddling_mage() -> EffectOp {
    EffectOp::Choice {
        controller: PlayerRef::Controller,
        options: nameable_nonland()
            .iter()
            .map(|ids| EffectOp::Choice {
                controller: PlayerRef::Controller,
                options: ids
                    .iter()
                    .map(|&named| s1b(S1bOpV1::MageNamesV1 { named }))
                    .collect(),
            })
            .collect(),
    }
}

// ---- Statics and hooks --------------------------------------------------------------

fn definition(state: &GameState, object: ObjectId) -> Option<&'static CardDef> {
    let live = state.objects.try_get(object)?;
    CARD_DEFS.get(live.card_def as usize)
}

/// Whether `object` is a battlefield permanent named `name` showing its
/// front face, whose printed abilities apply.
fn active_named(state: &GameState, object: ObjectId, name: &str) -> bool {
    state.objects.try_get(object).is_some_and(|live| {
        live.zone == Zone::Battlefield
            && live.v4.face_index == 0
            && CARD_DEFS
                .get(live.card_def as usize)
                .is_some_and(|def| def.name == name && def.is_executable())
            && crate::continuous_characteristics_v1::printed_abilities_active(state, object)
    })
}

fn any_active(state: &GameState, name: &str) -> bool {
    battlefield(state).any(|object| active_named(state, object, name))
}

fn turn_now(state: &GameState) -> (u32, PlayerId) {
    (state.turn_key_v1(), state.active_player)
}

/// Noncreature spells `player` has cast this turn.
fn noncreature_casts(state: &GameState, player: PlayerId) -> u8 {
    read_state(state)
        .filter(|s1b| s1b.cast_turn == Some(turn_now(state)))
        .map_or(0, |s1b| s1b.noncreature_casts[player.index()])
}

/// Cast prohibitions of this module: Deafening Silence ("Each player can't
/// cast more than one noncreature spell each turn."), Gaddock Teeg
/// ("Noncreature spells with mana value 4 or greater can't be cast.
/// Noncreature spells with {X} in their mana costs can't be cast.") and
/// Meddling Mage ("Spells with the chosen name can't be cast."). Whether
/// `player` can't cast `card` now.
pub(crate) fn cast_prohibited(state: &GameState, player: PlayerId, card: ObjectId) -> bool {
    let Some(def) = definition(state, card) else {
        return false;
    };
    let noncreature = !def.types.contains(&CardType::Creature);
    if noncreature
        && noncreature_casts(state, player) >= 1
        && any_active(state, DEAFENING_SILENCE)
    {
        return true;
    }
    if noncreature
        && (def.mana_value >= 4 || def.cost.x_count > 0)
        && any_active(state, GADDOCK_TEEG)
    {
        return true;
    }
    read_state(state).is_some_and(|s1b| {
        s1b.mage_names.iter().any(|(mage, named)| {
            state.objects.try_get(mage.object).is_some_and(|live| {
                live.zone_change_count == mage.zone_change_count
                    && active_named(state, mage.object, MEDDLING_MAGE)
            }) && CARD_DEFS[usize::from(*named)].object_name == def.object_name
        })
    })
}

/// Counts noncreature spells as they are cast (Deafening Silence counts
/// those cast before it entered, too).
pub(crate) fn delayed_triggers(
    state: &mut GameState,
    events: &[CommittedEvent],
) -> Vec<PendingTrigger> {
    if !cfg!(feature = "eternal-fixtures") {
        return Vec::new();
    }
    for event in events {
        let CommittedEvent::SpellCast { spell, controller } = *event else {
            continue;
        };
        if state
            .objects
            .try_get(spell)
            .is_none_or(|live| live.zone != Zone::Stack)
            || is_creature(state, spell)
        {
            continue;
        }
        let now = turn_now(state);
        let s1b = s1b_state(state);
        if s1b.cast_turn != Some(now) {
            s1b.cast_turn = Some(now);
            s1b.noncreature_casts = [0, 0];
        }
        let count = &mut s1b.noncreature_casts[controller.index()];
        *count = count.saturating_add(1);
    }
    Vec::new()
}

/// Sailors' Bane: "This spell costs {1} less to cast for each card you own
/// in exile and in your graveyard that's an instant card, a sorcery card, or
/// a card that has an Adventure."
pub(crate) fn spell_cost_reduction(state: &GameState, definition: &CardDef, player: PlayerId) -> u32 {
    if definition.name != SAILORS_BANE {
        return 0;
    }
    let counts = |object: &ObjectId| {
        let Some(live) = state.objects.try_get(*object) else {
            return false;
        };
        let Some(def) = CARD_DEFS.get(live.card_def as usize) else {
            return false;
        };
        live.owner == player
            && !live.v4.is_token
            && !def.is_token
            && (def.types.contains(&CardType::Instant)
                || def.types.contains(&CardType::Sorcery)
                || def.adventure.is_some())
    };
    let cards = state.players[player.index()]
        .graveyard
        .iter()
        .chain(state.exile.iter())
        .filter(|object| counts(object))
        .count();
    u32::try_from(cards).unwrap_or(u32::MAX)
}

/// Toxic Deluge: "As an additional cost to cast this spell, pay X life."
/// Its X is announced like Sickening Dreams' discard count, up to the life
/// the caster has, and the life is paid as the cast completes.
pub(crate) fn pays_life_for_x(def: &CardDef) -> bool {
    cfg!(feature = "eternal-fixtures") && def.name == TOXIC_DELUGE
}

/// The most X Toxic Deluge may announce: its caster's life total.
pub(crate) fn life_x_cap(def: &CardDef, player: PlayerId, state: &GameState) -> Option<u8> {
    pays_life_for_x(def)
        .then(|| u8::try_from(state.players[player.index()].life.max(0)).unwrap_or(u8::MAX))
}
