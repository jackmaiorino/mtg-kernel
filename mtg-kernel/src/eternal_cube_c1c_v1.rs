//! Card programs for the eternal catalog's Vintage Cube wave C1c
//! (`docs/design/eternal_formats_support_v1.md`): red and multicolor instants and sorceries.
//!
//! `eternal_cards_v1` forwards this module's effect leaves here, and the
//! recipe tables of `build_eternal_c1c_v1.rs` name its programs. Every
//! table is keyed by a printed name that occurs only in the eternal registry,
//! so the Pauper, FDN and Standard catalogs never reach it. Behavior was read
//! from each card's current Oracle text and, where present, its XMage source.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

pub(crate) mod targets;

use crate::card_def::{card_id_by_name, CardDef, CardType, DynamicValueDef, TargetSpec, CARD_DEFS};
use crate::effect::{
    EffectContinuation, EffectFrame, EffectObjectBinding, EffectOp, ExecCtx, ImpulseDuration,
    LibraryLookCount, LibraryPickFilterV1, LibraryPickRule, LibraryPickSelectionV1, ObjectRef,
    PlayerRef, TargetRef,
};
use crate::eternal_cards_v1::e3::{E3ChoiceKindV1, E3ChoiceV1};
use crate::eternal_cards_v1::EternalOpV1;
use crate::event::{self, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::standard_cards_v1::{StandardOpV1, StandardTargetV1};
use crate::state::{GameState, ObjectLinkV4, Target, Zone};
use crate::trigger::TriggeredAbilityDef;
use serde::{Deserialize, Serialize};
use targets::{C1cTargetV1, KolaghanModeV1};

// ---- State ------------------------------------------------------------------------

/// Per-game state for this module's cards, kept inside
/// `eternal_cards_v1::EternalStateV1`. Absent from every snapshot until one
/// of them needs it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct C1cStateV1 {
    /// Escape to the Wilds: the players who may play an additional land on
    /// the turn (`GameState::turn_key_v1`) recorded with them. Only the
    /// active player plays lands, so a record is used on its own player's
    /// turn and is stale afterwards.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra_land_plays: Vec<ExtraLandPlayV1>,
}

impl C1cStateV1 {
    pub(crate) fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// One "you may play an additional land this turn" (Escape to the Wilds).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtraLandPlayV1 {
    pub player: PlayerId,
    pub turn: u32,
}

pub(crate) fn c1c_state(state: &mut GameState) -> &mut C1cStateV1 {
    &mut state.eternal_v1.get_or_insert_with(Default::default).c1c
}

/// How many additional lands `player` may play this turn through Escape to
/// the Wilds (the engine adds it to the land-play limit).
pub(crate) fn extra_land_plays(state: &GameState, player: PlayerId) -> u8 {
    let turn = state.turn_key_v1();
    let count = state.eternal_v1.as_ref().map_or(0, |eternal| {
        eternal
            .c1c
            .extra_land_plays
            .iter()
            .filter(|record| record.player == player && record.turn == turn)
            .count()
    });
    u8::try_from(count).unwrap_or(u8::MAX)
}

// ---- Life paid for X --------------------------------------------------------------

/// "As an additional cost to cast this spell, pay X life" (Toxic Deluge's
/// shape, `eternal_sideboard_s1b_v1::pays_life_for_x`): Fire Covenant and
/// Vicious Rivalry.
pub(crate) fn pays_life_for_x(name: &str) -> bool {
    matches!(name, FIRE_COVENANT | VICIOUS_RIVALRY)
}

/// Fire Covenant's X: the life it pays is one point per damage, and each
/// target pick is one point of damage, so X is the number of picks. `None`
/// for every other card.
pub(crate) fn covenant_forced_x(
    pending: &crate::engine::PendingCast,
    def: &CardDef,
) -> Option<u16> {
    (cfg!(feature = "eternal-fixtures") && def.name == FIRE_COVENANT)
        .then(|| u16::try_from(pending.targets_chosen.len()).unwrap_or(u16::MAX))
}

/// Whether `controller` is in the middle of casting Fire Covenant, whose
/// picks are limited by the life it will pay.
pub(crate) fn covenant_cast_in_progress(state: &GameState, controller: PlayerId) -> bool {
    state.engine.pending_cast.as_ref().is_some_and(|pending| {
        pending.controller == controller
            && state
                .objects
                .try_get(pending.spell)
                .is_some_and(|live| CARD_DEFS[live.card_def as usize].name == FIRE_COVENANT)
    })
}

const FIRE_COVENANT: &str = "Fire Covenant";
const VICIOUS_RIVALRY: &str = "Vicious Rivalry";

/// Lingering Souls' token: a 1/1 white Spirit creature token with flying.
const WHITE_FLYING_SPIRIT_TOKEN: &str = "White Flying Spirit Token";

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::C1cV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum C1cOpV1 {
    /// Wheel of Fortune's discard: each player discards their hand.
    DiscardEachHandV1,
    /// Traumatic Critique: it deals X damage to its target.
    DamageTargetXV1,
    /// Kolaghan's Command: one chosen mode, acting on its own target number
    /// `slot` if that target is still legal under `spec` (608.2b).
    KolaghanV1 {
        mode: KolaghanModeV1,
        slot: u8,
        spec: TargetSpec,
    },
    /// Delayed Blast Fireball: 2 damage to each opponent and each creature
    /// they control, or 5 if this spell was cast from exile.
    DelayedBlastV1,
    /// Impractical Joke: damage can't be prevented this turn, then 3 damage
    /// to its target, if it has one.
    JokeV1,
    /// Escape to the Wilds: its controller may play an additional land this
    /// turn.
    ExtraLandV1,
    /// Expressive Iteration: look at the top three cards, one to the hand,
    /// one to the bottom, one exiled and playable this turn. A choice.
    IterationV1,
    /// Fire Covenant: each target pick is dealt one damage, as one batch.
    CovenantV1,
    /// Vicious Rivalry: destroy each artifact and creature with mana value X
    /// or less.
    RivalryV1,
}

/// Resolution choices owned by this module, carried as
/// `E3ChoiceKindV1::C1cV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum C1cChoiceV1 {
    /// Expressive Iteration: the one of the `looked` cards to put into the
    /// hand.
    IterationHandV1 { looked: Vec<EffectObjectBinding> },
    /// Expressive Iteration: the one of the `looked` cards (those still in
    /// the library) to put on the bottom; the other is exiled.
    IterationBottomV1 { looked: Vec<EffectObjectBinding> },
    /// Kolaghan's Command: the card its target player discards.
    KolaghanDiscardV1,
}

impl C1cChoiceV1 {
    /// The candidates are hidden cards: library cards, or the hand a player
    /// discards from.
    pub(crate) fn chooser_private(&self) -> bool {
        true
    }

    pub(crate) fn public_purpose(&self) -> crate::rl::TargetSelectionPurposeV4 {
        crate::rl::TargetSelectionPurposeV4::CardSelection
    }
}

impl C1cOpV1 {
    /// Whether the leaf may stop for a player's choice (or a draw that a
    /// replacement can interrupt), so it runs in the resumable interpreter.
    /// Wheel of Fortune's discard runs in the resumable interpreter, so the
    /// draws after it (their own `EffectOp::DrawCards`) can offer Dredge for
    /// a card the discard just put into a graveyard; the X damage is
    /// synchronous; a targeted player's discard is their choice.
    pub(crate) fn contains_player_choice(&self) -> bool {
        match self {
            Self::DiscardEachHandV1 => true,
            Self::DamageTargetXV1 => false,
            Self::KolaghanV1 { mode, .. } => *mode == KolaghanModeV1::DiscardV1,
            Self::IterationV1 => true,
            Self::DelayedBlastV1
            | Self::JokeV1
            | Self::ExtraLandV1
            | Self::CovenantV1
            | Self::RivalryV1 => false,
        }
    }
}

fn c1c(op: C1cOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::C1cV1(op)))
}

fn damage_target(amount: i32) -> EffectOp {
    EffectOp::DealDamage {
        target: TargetRef::Target(0),
        amount,
    }
}

// ---- Programs -----------------------------------------------------------------------

/// Flame Slash: "Flame Slash deals 4 damage to target creature."
pub fn flame_slash() -> EffectOp {
    damage_target(4)
}

/// Sear: "Sear deals 4 damage to target creature or planeswalker."
pub fn sear() -> EffectOp {
    damage_target(4)
}

/// Mine Collapse: "Mine Collapse deals 5 damage to target creature or
/// planeswalker." Its alternative cost is `alt_cost_for` in the recipes.
pub fn mine_collapse() -> EffectOp {
    damage_target(5)
}

/// Wheel of Fortune: "Each player discards their hand, then draws seven
/// cards." The draws are their own ops, so Dredge and other draw
/// replacements can interrupt them (Ox of Agonas' shape, for both players).
pub fn wheel_of_fortune() -> EffectOp {
    EffectOp::Sequence(vec![
        c1c(C1cOpV1::DiscardEachHandV1),
        EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 7,
        },
        // A draw that wins the game ends it before the other player draws.
        crate::eternal_cube_c1a_v1::stop_if_won(),
        EffectOp::DrawCards {
            player: PlayerRef::Opponent,
            count: 7,
        },
    ])
}

/// Lingering Souls: "Create two 1/1 white Spirit creature tokens with
/// flying." Its flashback is `flashback_for` in the recipes.
pub fn lingering_souls() -> EffectOp {
    EffectOp::CreateTokensDynamic {
        token_def: card_id_by_name(WHITE_FLYING_SPIRIT_TOKEN)
            .unwrap_or_else(|| panic!("{WHITE_FLYING_SPIRIT_TOKEN} in CARD_DEFS")),
        controller: PlayerRef::Controller,
        count: DynamicValueDef::Fixed(2),
        tapped: false,
    }
}

/// Traumatic Critique: "Traumatic Critique deals X damage to any target.
/// Draw two cards, then discard a card."
pub fn traumatic_critique() -> EffectOp {
    EffectOp::Sequence(vec![
        c1c(C1cOpV1::DamageTargetXV1),
        EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 2,
        },
        EffectOp::DiscardCards {
            player: PlayerRef::Controller,
            count: 1,
        },
    ])
}

/// Kolaghan's Command, a pair of its modes ("Choose two"), in printed
/// order: each chosen mode acts on its own target if that is still legal
/// (608.2b). The target player's discard is this module's own choice, not
/// `EffectOp::DiscardCards`, whose resumable form must be a program's last
/// instruction.
fn kolaghans_command(first: KolaghanModeV1, second: KolaghanModeV1) -> EffectOp {
    let spec = TargetSpec::StandardV1(StandardTargetV1::C1cV1(C1cTargetV1::KolaghanPairV1(
        first, second,
    )));
    EffectOp::Sequence(vec![
        c1c(C1cOpV1::KolaghanV1 {
            mode: first,
            slot: 0,
            spec,
        }),
        c1c(C1cOpV1::KolaghanV1 {
            mode: second,
            slot: 1,
            spec,
        }),
    ])
}

/// The six pairs of Kolaghan's Command's four modes: return a creature
/// card, target player discards, destroy an artifact, 2 damage. Pair `n` is
/// mode `n` of the card (`mode2_for`, `mode3_for`, `additional_modes_for`).
pub fn kolaghans_command_return_discard() -> EffectOp {
    kolaghans_command(
        KolaghanModeV1::ReturnCreatureCardV1,
        KolaghanModeV1::DiscardV1,
    )
}

pub fn kolaghans_command_return_destroy() -> EffectOp {
    kolaghans_command(
        KolaghanModeV1::ReturnCreatureCardV1,
        KolaghanModeV1::DestroyArtifactV1,
    )
}

pub fn kolaghans_command_return_damage() -> EffectOp {
    kolaghans_command(
        KolaghanModeV1::ReturnCreatureCardV1,
        KolaghanModeV1::DamageV1,
    )
}

pub fn kolaghans_command_discard_destroy() -> EffectOp {
    kolaghans_command(KolaghanModeV1::DiscardV1, KolaghanModeV1::DestroyArtifactV1)
}

pub fn kolaghans_command_discard_damage() -> EffectOp {
    kolaghans_command(KolaghanModeV1::DiscardV1, KolaghanModeV1::DamageV1)
}

pub fn kolaghans_command_destroy_damage() -> EffectOp {
    kolaghans_command(KolaghanModeV1::DestroyArtifactV1, KolaghanModeV1::DamageV1)
}

// ---- Programs of the second half -----------------------------------------------------

/// Dead (Dead // Gone): "Dead deals 2 damage to target creature."
pub fn dead() -> EffectOp {
    damage_target(2)
}

/// Gone (Dead // Gone): "Return target creature you don't control to its
/// owner's hand."
pub fn gone() -> EffectOp {
    EffectOp::MoveAllTargets {
        to_zone: Zone::Hand,
    }
}

/// Delayed Blast Fireball: "Delayed Blast Fireball deals 2 damage to each
/// opponent and each creature they control. If this spell was cast from
/// exile, it deals 5 damage to each opponent and each creature they control
/// instead." Its foretell is `alt_cost_for` and the hand ability in the
/// recipes.
pub fn delayed_blast_fireball() -> EffectOp {
    c1c(C1cOpV1::DelayedBlastV1)
}

/// Flashback: "Target instant or sorcery card in your graveyard gains
/// flashback until end of turn. The flashback cost is equal to its mana
/// cost." Snapcaster Mage's grant.
pub fn flashback() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::LurrusV1(
        crate::eternal_lurrus_v1::LurrusOpV1::GrantFlashbackV1,
    )))
}

/// Impractical Joke: "Damage can't be prevented this turn. Impractical Joke
/// deals 3 damage to up to one target creature or planeswalker."
pub fn impractical_joke() -> EffectOp {
    c1c(C1cOpV1::JokeV1)
}

/// Escape to the Wilds: "Exile the top five cards of your library. You may
/// play cards exiled this way until the end of your next turn. You may play
/// an additional land this turn." Reckless Impulse's exile, five cards.
pub fn escape_to_the_wilds() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::ImpulseDraw {
            count: 5,
            duration: ImpulseDuration::UntilOwnersNextTurn,
        },
        c1c(C1cOpV1::ExtraLandV1),
    ])
}

/// Expressive Iteration: "Look at the top three cards of your library. Put
/// one of them into your hand, put one of them on the bottom of your
/// library, and exile one of them. You may play the exiled card this turn."
pub fn expressive_iteration() -> EffectOp {
    c1c(C1cOpV1::IterationV1)
}

/// Fire Covenant: "Fire Covenant deals X damage divided as you choose among
/// any number of target creatures." Its additional cost, pay X life, is
/// `pays_life_for_x`.
pub fn fire_covenant() -> EffectOp {
    c1c(C1cOpV1::CovenantV1)
}

/// Pillage the Bog: "Look at the top X cards of your library, where X is
/// twice the number of lands you control. Put one of them into your hand and
/// the rest on the bottom of your library in a random order." Its plot is
/// the recipes' plot cost.
pub fn pillage_the_bog() -> EffectOp {
    EffectOp::LookTopSelectV1 {
        player: PlayerRef::Controller,
        count: LibraryLookCount::ControllerLandsTwiceV1,
        rule: LibraryPickRule {
            pick: 1,
            choose_rest_order: false,
            selection: Some(LibraryPickSelectionV1 {
                without_mana_cost: crate::engine::FreeCastV1(false),
                filter: LibraryPickFilterV1::Any,
                optional: false,
                destination: Zone::Hand,
                reveal_selected: false,
            }),
        },
        pick_x: false,
    }
}

/// Vicious Rivalry: "Destroy all artifacts and creatures with mana value X
/// or less." Its additional cost, pay X life, is `pays_life_for_x`.
pub fn vicious_rivalry() -> EffectOp {
    c1c(C1cOpV1::RivalryV1)
}

// ---- Counter destination --------------------------------------------------------------

/// Where a spell countered by `counter_source`'s counter-unless-pays effect
/// goes. No More Lies: "If that spell is countered this way, exile it
/// instead of putting it into its owner's graveyard."
pub(crate) fn counter_destination(state: &GameState, counter_source: ObjectId) -> Zone {
    let exiles = state
        .objects
        .try_get(counter_source)
        .is_some_and(|live| CARD_DEFS[live.card_def as usize].name == "No More Lies");
    if exiles {
        Zone::Exile
    } else {
        Zone::Graveyard
    }
}

// ---- Triggered abilities ------------------------------------------------------------

/// Triggered abilities of this module's cards, falling through to wave
/// C2a.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    crate::eternal_cube_c2a_v1::triggers_for(name)
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
pub(crate) fn execute(op: &C1cOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        C1cOpV1::DiscardEachHandV1 => {
            // Both hands, the active player's first (101.4); each discard is
            // an ordinary discard event (Ox of Agonas' shape).
            let first = state.active_player;
            for player in [first, first.opponent()] {
                crate::eternal_legacy_e7a_v1::discard_controller_hand(state, player);
            }
        }
        C1cOpV1::DamageTargetXV1 => {
            let amount = i32::from(ctx.x_value);
            if amount > 0 && ctx.target_incarnation_matches(0, state) {
                let target = ctx.resolve_target(TargetRef::Target(0));
                event::propose_and_commit(
                    state,
                    event::damage_from_exec_ctx_v1(ctx, target, amount),
                );
            }
        }
        C1cOpV1::KolaghanV1 { mode, slot, spec } => {
            if let Some(follow_up) = kolaghan_follow_up(ctx, state, *mode, *slot, *spec) {
                crate::effect::execute(&follow_up, ctx, state);
            }
        }
        C1cOpV1::DelayedBlastV1 => delayed_blast(ctx, state),
        C1cOpV1::JokeV1 => joke(ctx, state),
        C1cOpV1::ExtraLandV1 => {
            let record = ExtraLandPlayV1 {
                player: ctx.controller,
                turn: state.turn_key_v1(),
            };
            let turn = record.turn;
            let records = &mut c1c_state(state).extra_land_plays;
            records.retain(|old| old.turn >= turn);
            records.push(record);
        }
        C1cOpV1::CovenantV1 => covenant(ctx, state),
        C1cOpV1::RivalryV1 => rivalry(ctx, state),
        // A choice runs in `drive`; the plain executor never reaches it.
        C1cOpV1::IterationV1 => {
            state.engine.halted = Some((
                crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                ctx.source,
            ));
        }
    }
}

/// Whether the resolving spell `source` was cast from exile (any cast from
/// exile, a foretold card or not). A copy was never cast.
fn cast_from_exile(state: &GameState, source: ObjectId) -> bool {
    state.objects.try_get(source).is_some_and(|live| {
        live.zone == Zone::Stack
            && live
                .v4
                .spell_cast_origin
                .is_some_and(|origin| origin.origin_zone == Zone::Exile)
    })
}

/// Delayed Blast Fireball's damage, to each opponent and each creature they
/// control at once.
fn delayed_blast(ctx: &ExecCtx, state: &mut GameState) {
    let amount = if cast_from_exile(state, ctx.source) {
        5
    } else {
        2
    };
    let opponent = ctx.controller.opponent();
    let mut events = vec![event::damage_from_exec_ctx_v1(
        ctx,
        Target::Player(opponent),
        amount,
    )];
    for object in battlefield_objects(state) {
        let live = state.objects.get(object);
        if live.controller != ctx.controller
            && crate::engine::object_has_type(state, object, CardType::Creature)
        {
            events.push(event::damage_from_exec_ctx_v1(
                ctx,
                Target::Object(object),
                amount,
            ));
        }
    }
    event::propose_and_commit_batch(state, events);
}

fn battlefield_objects(state: &GameState) -> Vec<ObjectId> {
    [PlayerId::P0, PlayerId::P1]
        .into_iter()
        .flat_map(|player| state.players[player.index()].battlefield.iter().copied())
        .collect()
}

/// Impractical Joke: a spell whose only target is illegal on resolution does
/// not resolve at all (608.2b), so it does not stop prevention either.
fn joke(ctx: &ExecCtx, state: &mut GameState) {
    let spec = TargetSpec::StandardV1(StandardTargetV1::C1cV1(
        C1cTargetV1::UpToOneCreatureOrPlaneswalkerV1,
    ));
    let target = legal_target(ctx, state, 0, spec);
    if !ctx.targets.is_empty() && target.is_none() {
        return;
    }
    crate::effect::execute(&EffectOp::DamageCannotBePreventedThisTurn, ctx, state);
    if target.is_some() {
        crate::effect::execute(&damage_target(3), ctx, state);
    }
}

/// Fire Covenant's division: each pick is one point, and each creature still
/// legal on resolution is dealt its picks as one simultaneous batch (an
/// illegal target's share is not dealt, 608.2b).
fn covenant(ctx: &ExecCtx, state: &mut GameState) {
    let spec = TargetSpec::StandardV1(StandardTargetV1::C1cV1(C1cTargetV1::CreaturePicksPerLifeV1));
    let mut shares: Vec<(Target, i32)> = Vec::new();
    for index in 0..ctx.targets.len() {
        let Some(target) = legal_target(ctx, state, index, spec) else {
            continue;
        };
        match shares.iter_mut().find(|(known, _)| *known == target) {
            Some((_, amount)) => *amount = amount.saturating_add(1),
            None => shares.push((target, 1)),
        }
    }
    if shares.is_empty() {
        return;
    }
    event::propose_and_commit_batch(
        state,
        shares
            .into_iter()
            .map(|(target, amount)| event::damage_from_exec_ctx_v1(ctx, target, amount))
            .collect(),
    );
}

/// Vicious Rivalry: destroy each artifact and creature with mana value X or
/// less together (regeneration shields apply).
fn rivalry(ctx: &ExecCtx, state: &mut GameState) {
    let limit = ctx.x_value;
    let victims = battlefield_objects(state)
        .into_iter()
        .filter(|&object| {
            (crate::engine::object_has_type(state, object, CardType::Artifact)
                || crate::engine::object_has_type(state, object, CardType::Creature))
                && crate::engine::object_mana_value(state, object) <= limit
                && !crate::engine::has_effective_keyword(
                    state,
                    object,
                    crate::card_def::Keywords::INDESTRUCTIBLE,
                )
        })
        .collect::<Vec<_>>();
    let destroyed = crate::effect::without_regenerated_v1(state, victims);
    event::propose_and_commit_batch(
        state,
        destroyed
            .into_iter()
            .map(|object| ProposedEvent::zone_change(object, Zone::Graveyard))
            .collect(),
    );
}

/// Target `index` of the resolving spell, if it is still the incarnation
/// chosen and still legal under `spec`.
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

/// What a chosen mode of Kolaghan's Command does, or `None` when its target
/// is no longer legal (the other mode still happens).
fn kolaghan_follow_up(
    ctx: &ExecCtx,
    state: &GameState,
    mode: KolaghanModeV1,
    slot: u8,
    spec: TargetSpec,
) -> Option<EffectOp> {
    let target = legal_target(ctx, state, usize::from(slot), spec)?;
    match (mode, target) {
        (KolaghanModeV1::ReturnCreatureCardV1, Target::Object(card))
            if state.objects.get(card).zone == Zone::Graveyard =>
        {
            Some(EffectOp::MoveObject {
                object: ObjectRef::Target(slot),
                to_zone: Zone::Hand,
            })
        }
        (KolaghanModeV1::DiscardV1, Target::Player(_)) => Some(EffectOp::DiscardCards {
            player: PlayerRef::Target(slot),
            count: 1,
        }),
        (KolaghanModeV1::DestroyArtifactV1, Target::Object(_)) => Some(EffectOp::DestroyObject {
            object: ObjectRef::Target(slot),
        }),
        (KolaghanModeV1::DamageV1, _) => Some(EffectOp::DealDamage {
            target: TargetRef::Target(slot),
            amount: 2,
        }),
        _ => None,
    }
}

/// Runs one of this module's leaves in the resumable interpreter.
pub(crate) fn drive(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    op: &C1cOpV1,
    path: Vec<u16>,
) -> Result<bool, String> {
    if let C1cOpV1::KolaghanV1 {
        mode: KolaghanModeV1::DiscardV1,
        slot,
        spec,
    } = op
    {
        // "Target player discards a card": their choice among two or more
        // cards, with the pair's other mode still to run after it.
        let Some(Target::Player(player)) =
            legal_target(&continuation.ctx, state, usize::from(*slot), *spec)
        else {
            return Ok(false);
        };
        let hand = state.players[player.index()].hand.clone();
        return match hand.as_slice() {
            [] => Ok(false),
            [card] => {
                crate::engine::commit_discarded_card(state, *card);
                Ok(false)
            }
            _ => {
                let bindings = hand.iter().map(|&card| bind(state, card)).collect();
                offer(
                    continuation,
                    C1cChoiceV1::KolaghanDiscardV1,
                    player,
                    bindings,
                    path,
                );
                Ok(true)
            }
        };
    }
    if let C1cOpV1::KolaghanV1 { mode, slot, spec } = op {
        // The other modes run as the next frame of the resumable
        // interpreter.
        if let Some(follow_up) = kolaghan_follow_up(&continuation.ctx, state, *mode, *slot, *spec) {
            continuation.frames.push(EffectFrame::Program {
                op: follow_up,
                path,
            });
        }
        return Ok(false);
    }
    if let C1cOpV1::IterationV1 = op {
        return iteration(state, continuation, path);
    }
    execute(op, &continuation.ctx, state);
    Ok(false)
}

// ---- Expressive Iteration -------------------------------------------------------------

fn bind(state: &GameState, object: ObjectId) -> EffectObjectBinding {
    let live = state.objects.get(object);
    EffectObjectBinding {
        object,
        expected_zone: live.zone,
        expected_zone_change_count: live.zone_change_count,
    }
}

fn binding_live(state: &GameState, binding: EffectObjectBinding) -> bool {
    state.objects.try_get(binding.object).is_some_and(|live| {
        live.zone == binding.expected_zone
            && live.zone_change_count == binding.expected_zone_change_count
    })
}

/// Offers one of this module's selections; the frames still to run after it
/// are the continuation's current frames.
fn offer(
    continuation: &mut EffectContinuation,
    kind: C1cChoiceV1,
    player: PlayerId,
    bindings: Vec<EffectObjectBinding>,
    path: Vec<u16>,
) {
    let remaining = continuation.frames.clone();
    crate::eternal_cards_v1::e3::offer(
        continuation,
        E3ChoiceV1 {
            kind: E3ChoiceKindV1::C1cV1(kind),
            player,
            candidates: bindings
                .iter()
                .map(|binding| Target::Object(binding.object))
                .collect(),
            bindings,
            min: 1,
            max: 1,
            ordered: false,
            path,
            remaining,
        },
    );
}

/// Expressive Iteration's start: the top three cards are looked at, then the
/// controller chooses the card for the hand (the cards left take their
/// places in `finish_selection`).
fn iteration(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    path: Vec<u16>,
) -> Result<bool, String> {
    let controller = continuation.ctx.controller;
    let looked = state.players[controller.index()]
        .library
        .iter()
        .take(3)
        .map(|&card| bind(state, card))
        .collect::<Vec<_>>();
    state.reveal_library_top(controller, controller, looked.len());
    match looked.len() {
        0 => Ok(false),
        1 => {
            event::propose_and_commit(
                state,
                ProposedEvent::zone_change(looked[0].object, Zone::Hand),
            );
            Ok(false)
        }
        _ => {
            offer(
                continuation,
                C1cChoiceV1::IterationHandV1 {
                    looked: looked.clone(),
                },
                controller,
                looked,
                path,
            );
            Ok(true)
        }
    }
}

/// Puts the library's top card `card` on the bottom.
fn put_on_bottom(
    state: &mut GameState,
    player: PlayerId,
    card: EffectObjectBinding,
) -> Result<(), String> {
    let link = ObjectLinkV4 {
        object: card.object,
        zone_change_count: card.expected_zone_change_count,
    };
    state.apply_scry_result(player, &[link], &[], &[card.object])
}

/// Exiles `card` and lets `player` play it this turn: a land through a land
/// play, anything else cast with its mana cost (Reckless Impulse's
/// permission, until end of turn).
fn exile_playable_this_turn(state: &mut GameState, player: PlayerId, card: ObjectId) {
    event::propose_and_commit(state, ProposedEvent::zone_change(card, Zone::Exile));
    let live = state.objects.get(card);
    if live.zone != Zone::Exile {
        return;
    }
    let def = &CARD_DEFS[live.card_def as usize];
    let play_or_cast = if def.is_playable_land() {
        crate::engine::PlayOrCast::Play
    } else if def.is_castable() {
        crate::engine::PlayOrCast::Cast
    } else {
        return;
    };
    let zone_change_generation = live.zone_change_count;
    state
        .engine
        .exile_play_permissions
        .push(crate::engine::PlayPermission {
            without_mana_cost: crate::engine::FreeCastV1::default(),
            object: card,
            holder: player,
            zone_change_generation,
            play_or_cast,
            expiry: crate::engine::PlayPermissionExpiry::EndOfTurn,
        });
}

/// Applies an answered choice of this module.
pub(crate) fn finish_selection(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    choice: &E3ChoiceV1,
    kind: &C1cChoiceV1,
    bindings: Vec<EffectObjectBinding>,
) -> Result<bool, String> {
    let player = choice.player;
    let [chosen] = bindings.as_slice() else {
        return Err("this module's selections choose exactly one card".into());
    };
    let chosen = *chosen;
    match kind {
        C1cChoiceV1::KolaghanDiscardV1 => {
            if state.objects.get(chosen.object).zone == Zone::Hand {
                crate::engine::commit_discarded_card(state, chosen.object);
            }
            Ok(false)
        }
        C1cChoiceV1::IterationHandV1 { looked } => {
            event::propose_and_commit(state, ProposedEvent::zone_change(chosen.object, Zone::Hand));
            let rest = looked
                .iter()
                .copied()
                .filter(|&card| card != chosen && binding_live(state, card))
                .collect::<Vec<_>>();
            match rest.len() {
                0 => Ok(false),
                // Two cards looked at: the other goes to the bottom, and no
                // card is left to exile.
                1 => {
                    put_on_bottom(state, player, rest[0])?;
                    Ok(false)
                }
                _ => {
                    offer(
                        continuation,
                        C1cChoiceV1::IterationBottomV1 {
                            looked: rest.clone(),
                        },
                        player,
                        rest,
                        choice.path.clone(),
                    );
                    Ok(true)
                }
            }
        }
        C1cChoiceV1::IterationBottomV1 { looked } => {
            let others = looked
                .iter()
                .copied()
                .filter(|&card| card != chosen && binding_live(state, card))
                .collect::<Vec<_>>();
            // The card left over is exiled first, so the chosen card is on
            // top of the library for the bottom placement.
            for other in others {
                exile_playable_this_turn(state, player, other.object);
            }
            put_on_bottom(state, player, chosen)?;
            Ok(false)
        }
    }
}
