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
use crate::state::{
    AbilitySourceContractV4, GameState, LinkedExileRecordV4, ObjectLinkV4, Target, Zone,
};
use crate::trigger::{PendingTrigger, TriggerCondition, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};

pub(crate) mod targets;

/// Per-game state for this module's cards, kept inside
/// `eternal_cards_v1::EternalStateV1`. Default (and absent from snapshots)
/// until one of these cards needs it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
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
    /// Whether Leyline of Sanctity's opening-hand offer has been made.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub leyline_offered: bool,
    /// Owners of cards discarded while a Tourach faced them, waiting for
    /// the next trigger collection.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub discards: Vec<PlayerId>,
    /// Engineered Explosives and the charge counters each entered with,
    /// kept for the ability of the incarnation that was sacrificed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub explosives: Vec<(ObjectLinkV4, u8)>,
    /// Permanents Liquimetal Coating made artifacts, with the turn.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifact_until: Vec<(ObjectLinkV4, (u32, PlayerId))>,
}

impl Hash for S1bStateV1 {
    fn hash<H: Hasher>(&self, state: &mut H) {
        if !self.is_empty() {
            b"eternal-sideboard-s1b/v1".hash(state);
            self.cast_turn.hash(state);
            self.noncreature_casts.hash(state);
            self.mage_names.hash(state);
            self.leyline_offered.hash(state);
            self.discards.hash(state);
            self.explosives.hash(state);
            self.artifact_until.hash(state);
        }
    }
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
    /// Pyrokinesis: 4 damage divided among its creature targets.
    PyrokinesisV1,
    /// Extirpate: exile every card with the target's name from its owner's
    /// graveyard, hand and library, then shuffle.
    ExtirpateV1,
    /// Tourach, kicked: target opponent discards two cards at random.
    TourachDiscardV1,
    /// Leyline of Sanctity begins the game on the battlefield.
    LeylineBeginsV1,
    /// Engineered Explosives enters with its sunburst charge counters.
    ExplosivesEnterV1,
    /// Engineered Explosives' destruction.
    ExplosivesDestroyV1,
    /// Liquimetal Coating: target 0 is an artifact until end of turn.
    ArtifactUntilEndOfTurnV1,
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
        TOURACH => &TOURACH_TRIGGERS,
        PERILOUS_SNARE => &PERILOUS_SNARE_TRIGGERS,
        _ => &[],
    }
}

/// Target specs of this module's triggered abilities. `None` for every
/// other card.
pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    let own = triggers_for(name)
        .iter()
        .any(|trigger| (trigger.effect)() == *effect)
        || delayed_effect_matches(name, effect);
    if !own {
        return None;
    }
    Some(match name {
        STEEL_WRECKING_BALL => TargetSpec::Creature,
        TOURACH if *effect == tourach_discard() => TargetSpec::TargetOpponent,
        PERILOUS_SNARE => TargetSpec::StandardV1(
            crate::standard_cards_v1::StandardTargetV1::OpponentNonlandPermanent,
        ),
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
        S1bOpV1::PyrokinesisV1 => run_pyrokinesis(ctx, state),
        S1bOpV1::ExtirpateV1 => run_extirpate(ctx, state),
        S1bOpV1::TourachDiscardV1 => run_tourach_discard(ctx, state),
        S1bOpV1::LeylineBeginsV1 => run_leyline_begins(ctx, state),
        S1bOpV1::ExplosivesEnterV1 => run_explosives_enter(ctx, state),
        S1bOpV1::ExplosivesDestroyV1 => run_explosives_destroy(ctx, state),
        S1bOpV1::ArtifactUntilEndOfTurnV1 => run_artifact_until_eot(ctx, state),
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
    if noncreature && noncreature_casts(state, player) >= 1 && any_active(state, DEAFENING_SILENCE)
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
        let Some(silence) = card_id_by_name(DEAFENING_SILENCE) else {
            continue;
        };
        let can_enter = state.objects.iter().any(|(_, object)| object.card_def == silence)
            || [PlayerId::P0, PlayerId::P1].into_iter().any(|player| {
                crate::eternal_outside_game_v1::outside_the_game(state, player).contains(&silence)
            });
        if !can_enter {
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
    stage3_triggers(state, events)
}

/// Sailors' Bane: "This spell costs {1} less to cast for each card you own
/// in exile and in your graveyard that's an instant card, a sorcery card, or
/// a card that has an Adventure."
pub(crate) fn spell_cost_reduction(
    state: &GameState,
    definition: &CardDef,
    player: PlayerId,
) -> u32 {
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

// ---- Stage 3: Pyrokinesis, Stern Dismissal, Extirpate, Tourach, Leyline ------------------

const TOURACH: &str = "Tourach, Dread Cantor";
const LEYLINE_OF_SANCTITY: &str = "Leyline of Sanctity";
const EXTIRPATE: &str = "Extirpate";

/// Pyrokinesis: "deals 4 damage divided as you choose among any number of
/// target creatures." Divided as it is announced: four picks of one point.
pub fn pyrokinesis() -> EffectOp {
    s1b(S1bOpV1::PyrokinesisV1)
}

/// Stern Dismissal: "Return target creature or enchantment an opponent
/// controls to its owner's hand."
pub fn stern_dismissal() -> EffectOp {
    EffectOp::MoveAllTargets {
        to_zone: Zone::Hand,
    }
}

/// Extirpate: "Choose target card in a graveyard other than a basic land
/// card. Search its owner's graveyard, hand, and library for all cards with
/// the same name as that card and exile them. Then that player shuffles."
pub fn extirpate() -> EffectOp {
    s1b(S1bOpV1::ExtirpateV1)
}

fn tourach_discard() -> EffectOp {
    s1b(S1bOpV1::TourachDiscardV1)
}

/// Tourach: "Whenever an opponent discards a card, put a +1/+1 counter on
/// Tourach."
fn tourach_counter() -> EffectOp {
    EffectOp::AddPlusOnePlusOneCounters {
        object: ObjectRef::ThisSource,
        count: 1,
    }
}

/// Leyline of Sanctity: the opening-hand offer, as the first upkeep begins.
fn leyline_choice() -> EffectOp {
    EffectOp::Choice {
        controller: PlayerRef::Controller,
        options: vec![
            s1b(S1bOpV1::LeylineBeginsV1),
            EffectOp::Sequence(Vec::new()),
        ],
    }
}

const PERILOUS_SNARE: &str = "Perilous Snare";

/// Perilous Snare: "exile target nonland permanent an opponent controls
/// until this artifact leaves the battlefield."
fn snare_exile() -> EffectOp {
    EffectOp::ExileTargetLinkedToSource {
        object: ObjectRef::Target(0),
    }
}

/// Perilous Snare's max speed ability: "Put a +1/+1 counter on target
/// creature or Vehicle you control."
pub fn snare_counter() -> EffectOp {
    EffectOp::AddPlusOnePlusOneCounters {
        object: ObjectRef::Target(0),
        count: 1,
    }
}

const PERILOUS_SNARE_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, snare_exile)];

/// "Start your engines!" (Perilous Snare), read by the Standard speed
/// designation.
pub(crate) fn starts_engines(name: &str) -> bool {
    cfg!(feature = "eternal-fixtures") && name == PERILOUS_SNARE
}

/// "Max speed" (Perilous Snare): whether the controller of `source` has
/// speed 4.
pub(crate) fn max_speed(state: &GameState, source: ObjectId) -> bool {
    let controller = state.objects.get(source).controller;
    state
        .speed_v1
        .is_some_and(|speed| speed.speeds[controller.index()] >= 4)
}

const TOURACH_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    intervening_if_kicked: true,
    ..trigger(TriggerCondition::Etb, tourach_discard)
}];

/// Triggers created outside the printed tables (hook-queued).
pub(crate) fn delayed_effect_matches(name: &str, effect: &EffectOp) -> bool {
    match name {
        TOURACH => *effect == tourach_counter(),
        LEYLINE_OF_SANCTITY => *effect == leyline_choice(),
        _ => false,
    }
}

fn run_pyrokinesis(ctx: &ExecCtx, state: &mut GameState) {
    let spec = TargetSpec::StandardV1(
        crate::standard_cards_v1::StandardTargetV1::DividedFourDamageCreaturesV1,
    );
    let damage = targets::division(&ctx.targets)
        .into_iter()
        .filter(|&(target, amount)| {
            amount > 0
                && ctx.targets.iter().enumerate().any(|(index, &slot)| {
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
                })
        })
        .map(|(target, amount)| event::damage_from_exec_ctx_v1(ctx, target, amount))
        .collect::<Vec<_>>();
    if !damage.is_empty() {
        event::propose_and_commit_batch(state, damage);
    }
}

fn halt(state: &mut GameState, source: ObjectId) {
    state.engine.halted = Some((
        crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
        source,
    ));
}

fn run_extirpate(ctx: &ExecCtx, state: &mut GameState) {
    let Some(Target::Object(target)) = ctx.targets.first().copied() else {
        return;
    };
    if !ctx.target_incarnation_matches(0, state)
        || state.objects.get(target).zone != Zone::Graveyard
    {
        return;
    }
    let owner = state.objects.get(target).owner;
    let name = CARD_DEFS[usize::from(state.objects.get(target).card_def)].object_name;
    let seat = &state.players[owner.index()];
    let cards = seat
        .graveyard
        .iter()
        .chain(seat.hand.iter())
        .chain(seat.library.iter())
        .copied()
        .filter(|&card| {
            let live = state.objects.get(card);
            !live.v4.is_token && CARD_DEFS[usize::from(live.card_def)].object_name == name
        })
        .collect::<Vec<_>>();
    for card in cards {
        event::propose_and_commit(state, ProposedEvent::zone_change(card, Zone::Exile));
    }
    if state.shuffle_library(owner).is_err() {
        halt(state, ctx.source);
    }
}

fn run_tourach_discard(ctx: &ExecCtx, state: &mut GameState) {
    let Some(Target::Player(player)) = ctx.targets.first().copied() else {
        return;
    };
    for _ in 0..2 {
        match state.random_hand_card_v1(player) {
            Ok(Some(card)) => crate::engine::commit_discarded_card(state, card),
            Ok(None) => break,
            Err(_) => {
                halt(state, ctx.source);
                return;
            }
        }
    }
}

fn run_leyline_begins(ctx: &ExecCtx, state: &mut GameState) {
    let Some(contract) = ctx.ability_source_contract else {
        return;
    };
    let in_hand = state.objects.try_get(contract.source).is_some_and(|live| {
        live.zone == Zone::Hand && live.zone_change_count == contract.zone_change_count
    });
    if in_hand {
        crate::effect::execute(
            &EffectOp::MoveObject {
                object: ObjectRef::ThisSource,
                to_zone: Zone::Battlefield,
            },
            ctx,
            state,
        );
    }
}

/// Whether `player` has hexproof from Leyline of Sanctity ("You have
/// hexproof.").
pub(crate) fn player_hexproof(state: &GameState, player: PlayerId) -> bool {
    battlefield(state).any(|object| {
        state.objects.get(object).controller == player
            && active_named(state, object, LEYLINE_OF_SANCTITY)
    })
}

/// Extirpate has split second: while it is on the stack, nobody casts spells
/// or activates abilities that are not mana abilities.
pub(crate) fn extirpate_on_stack(state: &GameState) -> bool {
    static ID: std::sync::OnceLock<Option<u16>> = std::sync::OnceLock::new();
    let Some(id) = *ID.get_or_init(|| card_id_by_name(EXTIRPATE)) else {
        return false;
    };
    state.stack.iter().any(|item| {
        item.kind == crate::state::StackItemKind::Spell
            && state
                .objects
                .try_get(item.source)
                .is_some_and(|live| live.zone == Zone::Stack && live.card_def == id)
    })
}

/// Records a discard by `card`'s owner while a Tourach faces them.
pub(crate) fn note_discard(state: &mut GameState, card: ObjectId) {
    let Some(live) = state.objects.try_get(card) else {
        return;
    };
    if live.zone != Zone::Hand {
        return;
    }
    let owner = live.owner;
    let tourach = battlefield(state).any(|object| {
        state.objects.get(object).controller == owner.opponent()
            && active_named(state, object, TOURACH)
    });
    if tourach {
        s1b_state(state).discards.push(owner);
    }
}

fn definition_name(state: &GameState, object: ObjectId) -> Option<&'static str> {
    definition(state, object).map(|def| def.name)
}

/// Opponent discards (Tourach) and Leyline of Sanctity's opening-hand offer,
/// as the game's first upkeep begins.
fn stage3_triggers(state: &mut GameState, events: &[CommittedEvent]) -> Vec<PendingTrigger> {
    let mut out = Vec::new();
    let discards = match state.eternal_v1.as_mut() {
        Some(eternal) => std::mem::take(&mut eternal.s1b.discards),
        None => Vec::new(),
    };
    for owner in discards {
        let tourachs = battlefield(state)
            .filter(|&object| {
                state.objects.get(object).controller == owner.opponent()
                    && active_named(state, object, TOURACH)
            })
            .collect::<Vec<_>>();
        for tourach in tourachs {
            let source = AbilitySourceContractV4::capture(state, tourach);
            out.push(crate::eternal_cards_v1::delayed_trigger(
                source,
                source.controller,
                tourach_counter(),
            ));
        }
    }
    let first_upkeep = state.turn == 1
        && state.active_player == state.starting_player
        && state.extra_turn_v1.is_none()
        && events
            .iter()
            .any(|event| matches!(event, CommittedEvent::UpkeepBegan { .. }));
    if first_upkeep && !read_state(state).is_some_and(|s1b| s1b.leyline_offered) {
        let leylines = [PlayerId::P0, PlayerId::P1]
            .into_iter()
            .flat_map(|player| state.players[player.index()].hand.iter().copied())
            .filter(|&card| definition_name(state, card) == Some(LEYLINE_OF_SANCTITY))
            .collect::<Vec<_>>();
        if !leylines.is_empty() {
            s1b_state(state).leyline_offered = true;
        }
        for card in leylines {
            let source = AbilitySourceContractV4::capture(state, card);
            out.push(crate::eternal_cards_v1::delayed_trigger(
                source,
                source.owner,
                leyline_choice(),
            ));
        }
    }
    out
}

// ---- Stage 4: Engineered Explosives and Liquimetal Coating ----------------------------------

/// Engineered Explosives: "Sunburst" for a noncreature permanent, so it
/// enters with a charge counter for each color of mana spent to cast it.
pub fn engineered_explosives() -> EffectOp {
    s1b(S1bOpV1::ExplosivesEnterV1)
}

/// Engineered Explosives: "{2}, Sacrifice this artifact: Destroy each
/// nonland permanent with mana value equal to the number of charge counters
/// on this artifact."
pub fn explosives_destroy() -> EffectOp {
    s1b(S1bOpV1::ExplosivesDestroyV1)
}

/// Liquimetal Coating: "{T}: Target permanent becomes an artifact in
/// addition to its other types until end of turn."
pub fn liquimetal_coating() -> EffectOp {
    s1b(S1bOpV1::ArtifactUntilEndOfTurnV1)
}

fn run_explosives_enter(ctx: &ExecCtx, state: &mut GameState) {
    if state
        .objects
        .try_get(ctx.source)
        .is_none_or(|live| live.zone != Zone::Stack)
    {
        return;
    }
    // Sunburst counts the colors spent on the spell, recorded while it is on
    // the stack.
    let colors = crate::eternal_lurrus_v1::colors_spent_on(state, ctx.source);
    crate::effect::execute(
        &EffectOp::MoveObject {
            object: ObjectRef::ThisSource,
            to_zone: Zone::Battlefield,
        },
        ctx,
        state,
    );
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
    state.objects.get_mut(ctx.source).counters.charge = i32::from(colors);
    let s1b = s1b_state(state);
    s1b.explosives.retain(|(host, _)| host.object != ctx.source);
    s1b.explosives.push((link, colors));
}

fn destroy_batch(state: &mut GameState, objects: Vec<ObjectId>) {
    let mut events = Vec::new();
    for id in objects {
        if state.objects.get(id).zone != Zone::Battlefield
            || crate::engine::has_effective_keyword(state, id, Keywords::INDESTRUCTIBLE)
            || crate::eternal_vintage_e7b_v1::statics::regenerate_instead(state, id)
        {
            continue;
        }
        events.push(ProposedEvent::zone_change(id, Zone::Graveyard));
    }
    if !events.is_empty() {
        event::propose_and_commit_batch(state, events);
    }
}

fn run_explosives_destroy(ctx: &ExecCtx, state: &mut GameState) {
    let Some(contract) = ctx.ability_source_contract else {
        return;
    };
    let counters = read_state(state)
        .and_then(|s1b| {
            s1b.explosives.iter().find(|(host, _)| {
                host.object == contract.source
                    && host.zone_change_count == contract.zone_change_count
            })
        })
        .map_or(0, |(_, colors)| *colors);
    let doomed = battlefield(state)
        .filter(|&object| {
            !crate::engine::object_has_type(state, object, CardType::Land)
                && crate::engine::object_mana_value(state, object) == u16::from(counters)
        })
        .collect::<Vec<_>>();
    destroy_batch(state, doomed);
}

fn run_artifact_until_eot(ctx: &ExecCtx, state: &mut GameState) {
    let Some(Target::Object(target)) = ctx.targets.first().copied() else {
        return;
    };
    if !ctx.target_incarnation_matches(0, state)
        || state.objects.get(target).zone != Zone::Battlefield
    {
        return;
    }
    let link = ObjectLinkV4 {
        object: target,
        zone_change_count: state.objects.get(target).zone_change_count,
    };
    let now = turn_now(state);
    let s1b = s1b_state(state);
    s1b.artifact_until.retain(|(_, turn)| *turn == now);
    s1b.artifact_until.push((link, now));
}

/// Liquimetal Coating: whether `object` is an artifact in addition to its
/// other types this turn.
pub(crate) fn artifact_added(state: &GameState, object: ObjectId) -> bool {
    let Some(s1b) = read_state(state) else {
        return false;
    };
    if s1b.artifact_until.is_empty() {
        return false;
    }
    let now = turn_now(state);
    let Some(live) = state.objects.try_get(object) else {
        return false;
    };
    live.zone == Zone::Battlefield
        && s1b.artifact_until.iter().any(|(host, turn)| {
            host.object == object
                && host.zone_change_count == live.zone_change_count
                && *turn == now
        })
}
