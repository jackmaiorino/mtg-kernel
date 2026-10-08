//! Card programs for the MageZero Standard catalog's non-creature permanent,
//! planeswalker and transforming-legend families (inventory families E and F).
//!
//! Every program here is selected by printed card name from the registry, so
//! catalogs without these names never reach it. Behavior was read from each
//! card's XMage source (`magefree/mage` master).

use crate::card_def::{CardDef, CardType, Keywords, Subtype, CARD_DEFS};
use crate::effect::{EffectObjectBinding, EffectOp, ExecCtx, PlayerRef};
use crate::event::{self, CommittedEvent, DamageProposed, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::mana::ManaColor;
use crate::state::{GameState, Target, Zone};
use crate::trigger::{TriggerCondition, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};

/// Per-game state for this module's cards. Absent until one of them needs
/// it, so other catalogs keep their snapshots and hashes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StandardStateV1 {
    /// Turn number and, per player, the noncombat damage dealt that turn by
    /// red sources they controlled (Temple of Power's activation condition).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    red_noncombat_damage: Option<(u32, [u32; 2])>,
    /// Unlocked doors of Room permanents, per exact battlefield incarnation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    rooms: Vec<RoomDoorsV1>,
    /// Turn number and, per player, the life gained and the life lost that
    /// turn (Lunar Convocation). Paying life is losing it (119.4).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    life_this_turn: Option<(u32, [u32; 2], [u32; 2])>,
    /// Turn number and the creatures declared as attackers that turn (Case
    /// of the Gateway Express).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    attackers_this_turn: Option<(u32, u32)>,
    /// Solved Case permanents, per exact battlefield incarnation (719.3).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    solved_cases: Vec<(ObjectId, u32)>,
    /// Class levels above 1, per exact battlefield incarnation (716.2).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    class_levels: Vec<(ObjectId, u32, u8)>,
    /// "You may cast this card from your graveyard" grants: card, its
    /// graveyard incarnation, the player who may cast it, and the turn the
    /// grant ends with.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    graveyard_casts: Vec<(ObjectId, u32, PlayerId, u32)>,
}

/// The doors of one Room permanent incarnation: bit 0 is the left door,
/// bit 1 the right door. A Room absent from the list has both doors locked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RoomDoorsV1 {
    object: ObjectId,
    zone_change_count: u32,
    unlocked: u8,
}

/// One-object battlefield target filters owned by this module
/// (`TargetSpec::StandardV1`). "Opponent" is relative to the targeting
/// controller.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StandardTargetV1 {
    /// A nonland permanent an opponent controls with mana value at most N.
    OpponentNonlandPermanentManaValueAtMost(u8),
    /// A nonland permanent an opponent controls.
    OpponentNonlandPermanent,
    /// An artifact or creature an opponent controls.
    OpponentArtifactOrCreature,
    /// An artifact the targeting controller controls.
    ControlledArtifact,
    /// An instant or sorcery card in the targeting controller's graveyard.
    InstantOrSorceryCardInOwnGraveyard,
}

impl StandardTargetV1 {
    fn in_graveyard(self) -> bool {
        matches!(self, Self::InstantOrSorceryCardInOwnGraveyard)
    }
}

/// The zone a `TargetSpec::StandardV1` target is chosen in.
pub(crate) fn target_zone(target: StandardTargetV1) -> Zone {
    if target.in_graveyard() {
        Zone::Graveyard
    } else {
        Zone::Battlefield
    }
}

/// The legal choices for `target`, in battlefield or graveyard order.
pub(crate) fn legal_targets(
    target: StandardTargetV1,
    controller: PlayerId,
    state: &GameState,
) -> Vec<Target> {
    let candidates: Vec<ObjectId> = if target.in_graveyard() {
        state.players[controller.index()].graveyard.clone()
    } else {
        [PlayerId::P0, PlayerId::P1]
            .iter()
            .flat_map(|player| state.players[player.index()].battlefield.iter().copied())
            .collect()
    };
    candidates
        .into_iter()
        .filter(|&id| target_matches(target, controller, id, state))
        .map(Target::Object)
        .collect()
}

/// Whether `object` satisfies `target` for `controller`.
pub(crate) fn target_matches(
    target: StandardTargetV1,
    controller: PlayerId,
    object: ObjectId,
    state: &GameState,
) -> bool {
    let Some(live) = state.objects.try_get(object) else {
        return false;
    };
    if target.in_graveyard() {
        let def = &CARD_DEFS[live.card_def as usize];
        return live.zone == Zone::Graveyard
            && live.owner == controller
            && !live.v4.is_token
            && (def.has_type(CardType::Instant) || def.has_type(CardType::Sorcery));
    }
    if live.zone != Zone::Battlefield {
        return false;
    }
    let has = |card_type| crate::engine::object_has_type(state, object, card_type);
    match target {
        StandardTargetV1::OpponentNonlandPermanentManaValueAtMost(maximum) => {
            live.controller != controller
                && !has(CardType::Land)
                && mana_value(state, object) <= u16::from(maximum)
        }
        StandardTargetV1::OpponentNonlandPermanent => {
            live.controller != controller && !has(CardType::Land)
        }
        StandardTargetV1::OpponentArtifactOrCreature => {
            live.controller != controller && (has(CardType::Artifact) || has(CardType::Creature))
        }
        StandardTargetV1::ControlledArtifact => {
            live.controller == controller && has(CardType::Artifact)
        }
        StandardTargetV1::InstantOrSorceryCardInOwnGraveyard => false,
    }
}

/// Effect leaves owned by this module. Appended to `EffectOp` as one variant
/// so earlier serialized programs keep their shapes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StandardOpV1 {
    /// Put loyalty counters on the resolving ability's own source, if it is
    /// still the same battlefield planeswalker incarnation.
    AddLoyaltyToSource { amount: u8 },
    /// `player` chooses one permanent they control matching `filter` (a
    /// public choice, not targeted) and `action` applies to it. With one
    /// candidate the choice is automatic; with none nothing happens.
    PlayerChoosesControlledPermanent {
        player: PlayerRef,
        filter: StandardPermanentFilterV1,
        action: StandardChosenActionV1,
    },
    /// The answered half of `PlayerChoosesControlledPermanent`: interpreter
    /// owned, never part of a generated program.
    ApplyChosenPermanent {
        chosen: EffectObjectBinding,
        action: StandardChosenActionV1,
    },
    /// Each nonland permanent `player` controls goes into its owner's
    /// library, then every affected library is shuffled.
    ShuffleNonlandPermanentsIntoLibraries { player: PlayerRef },
    /// Transform the resolving ability's source to its other face, if it is
    /// still the same battlefield incarnation.
    TransformSource,
    /// Trigger-time template for "you lose that much life": materialized to
    /// `CecilDarkness` with the damage event's amount.
    BindCecilDarkness,
    /// Cecil, Dark Knight: its controller loses `amount` life. Then if their
    /// life total is at most half their starting life total, untap the
    /// source and transform it.
    CecilDarkness { amount: u32 },
    /// Other attacking creatures the controller controls gain `keywords`
    /// until end of turn.
    OtherAttackersGainUntilEndOfTurn { keywords: Keywords },
    /// The source, put into a graveyard from the battlefield by the event
    /// that triggered this ability, returns to the battlefield tapped and
    /// transformed under its owner's control.
    ReturnSourceTappedAndTransformed,
    /// Blue Sun's Twilight: gain control of target creature (mana value at
    /// most X); if X is 5 or more, create a token that's a copy of it.
    GainControlOfTargetCreatureCopyAtXFive,
    /// A Room spell resolves: it enters the battlefield, and if it was cast,
    /// the door of the half that was cast becomes unlocked (709.5).
    RoomEnters,
    /// Unlock `door` of the ability's source Room, if it is still the same
    /// battlefield incarnation and that door is locked.
    UnlockSourceDoor { door: u8 },
    /// A resolving Aura spell enters the battlefield attached to its target
    /// (303.4f), for this module's Auras whose enchant restriction is not
    /// "enchant creature".
    AuraEnters,
    /// Lunar Convocation: "if you gained life this turn, each opponent loses
    /// `amount` life", the intervening condition rechecked on resolution
    /// (603.4).
    OpponentLosesLifeIfYouGainedLife { amount: u8 },
    /// Lunar Convocation: "if you gained and lost life this turn, create"
    /// the token, the intervening condition rechecked on resolution.
    CreateTokenIfYouGainedAndLostLife { token_def: u16 },
    /// Case of the Gateway Express: each creature you control deals 1
    /// damage to the target creature, simultaneously.
    EachControlledCreatureDealsOneDamageToTarget,
    /// A Case's "to solve" end-step ability: the source becomes solved if
    /// it is still the same unsolved incarnation and its condition holds.
    SolveSourceCase,
    /// A Class's level-up ability resolves: the source gains `level` if it
    /// is still the same incarnation at the level before it (716.2a).
    GainClassLevel { level: u8 },
    /// Case of the Uneaten Feast: creature cards now in your graveyard gain
    /// "You may cast this card from your graveyard" until end of turn.
    GraveyardCreaturesCastableThisTurn,
}

impl StandardOpV1 {
    /// Whether this leaf can yield a player decision, so its program must use
    /// the resumable interpreter.
    pub(crate) fn contains_player_choice(&self) -> bool {
        matches!(self, Self::PlayerChoosesControlledPermanent { .. })
    }

    /// The exact object incarnation this leaf is bound to, if any.
    pub(crate) fn bound_object(&self) -> Option<EffectObjectBinding> {
        match self {
            Self::ApplyChosenPermanent { chosen, .. } => Some(*chosen),
            _ => None,
        }
    }

    /// Interpreter-owned leaves that a generated program may never contain.
    pub(crate) fn is_bound_continuation(&self) -> bool {
        matches!(self, Self::ApplyChosenPermanent { .. })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StandardPermanentFilterV1 {
    Any,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StandardChosenActionV1 {
    ReturnToOwnersHand,
}

/// The exact battlefield permanents `player` controls that match `filter`,
/// in battlefield order.
pub(crate) fn controlled_permanent_candidates(
    state: &GameState,
    player: PlayerId,
    filter: StandardPermanentFilterV1,
) -> Vec<EffectObjectBinding> {
    state.players[player.index()]
        .battlefield
        .iter()
        .copied()
        .filter(|&object| {
            let live = state.objects.get(object);
            live.zone == Zone::Battlefield
                && live.controller == player
                && match filter {
                    StandardPermanentFilterV1::Any => true,
                }
        })
        .map(|object| EffectObjectBinding {
            object,
            expected_zone: Zone::Battlefield,
            expected_zone_change_count: state.objects.get(object).zone_change_count,
        })
        .collect()
}

/// Applies a chosen-permanent action to its exact bound incarnation.
pub(crate) fn apply_chosen_permanent(
    state: &mut GameState,
    chosen: EffectObjectBinding,
    action: StandardChosenActionV1,
) {
    let live = state.objects.get(chosen.object);
    if live.zone != chosen.expected_zone
        || live.zone_change_count != chosen.expected_zone_change_count
    {
        return;
    }
    match action {
        StandardChosenActionV1::ReturnToOwnersHand => event::propose_and_commit(
            state,
            ProposedEvent::zone_change(chosen.object, Zone::Hand),
        ),
    }
}

/// True iff the resolving ability's source is still the incarnation that
/// created it, on the battlefield.
fn source_incarnation_live(ctx: &ExecCtx, state: &GameState) -> bool {
    let Some(contract) = ctx.ability_source_contract else {
        return false;
    };
    state.objects.try_get(ctx.source).is_some_and(|live| {
        live.zone == Zone::Battlefield
            && live.card_def == contract.card_def
            && live.zone_change_count == contract.zone_change_count
    })
}

/// True iff the ability source is the same battlefield incarnation with a
/// second face.
fn transformable_source(ctx: &ExecCtx, state: &GameState) -> bool {
    source_incarnation_live(ctx, state)
        && CARD_DEFS[state.objects.get(ctx.source).card_def as usize]
            .transform_face
            .is_some()
}

fn transform_resolving_source(ctx: &ExecCtx, state: &mut GameState) {
    if transformable_source(ctx, state) {
        let face = 1 - state.objects.get(ctx.source).v4.face_index.min(1);
        event::propose_and_commit(state, ProposedEvent::transform_in_place(ctx.source, face));
    }
}

pub(crate) fn execute(op: &StandardOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        StandardOpV1::TransformSource => transform_resolving_source(ctx, state),
        StandardOpV1::RoomEnters => room_enters(ctx, state),
        StandardOpV1::AuraEnters => aura_enters_battlefield(ctx, state),
        StandardOpV1::OpponentLosesLifeIfYouGainedLife { amount } => {
            if life_changed_this_turn(state, ctx.controller, false) {
                event::propose_and_commit(
                    state,
                    ProposedEvent::life_loss(ctx.controller.opponent(), i32::from(*amount)),
                );
            }
        }
        StandardOpV1::CreateTokenIfYouGainedAndLostLife { token_def } => {
            if life_changed_this_turn(state, ctx.controller, true) {
                event::propose_and_commit(
                    state,
                    ProposedEvent::create_token(*token_def, ctx.controller),
                );
            }
        }
        StandardOpV1::EachControlledCreatureDealsOneDamageToTarget => {
            let Some(Target::Object(target)) = ctx.targets.first().copied() else {
                return;
            };
            let live = state.objects.get(target);
            if !ctx.target_incarnation_matches(0, state)
                || live.zone != Zone::Battlefield
                || live.controller == ctx.controller
                || !crate::engine::object_has_type(state, target, CardType::Creature)
            {
                return;
            }
            let damage = state.players[ctx.controller.index()]
                .battlefield
                .iter()
                .copied()
                .filter(|&id| crate::engine::object_has_type(state, id, CardType::Creature))
                .map(|id| ProposedEvent::damage(id, Target::Object(target), 1))
                .collect::<Vec<_>>();
            if !damage.is_empty() {
                event::propose_and_commit_batch(state, damage);
            }
        }
        StandardOpV1::GainClassLevel { level } => {
            if source_incarnation_live(ctx, state)
                && class_level(state, ctx.source).checked_add(1) == Some(*level)
            {
                set_class_level(state, ctx.source, *level);
            }
        }
        StandardOpV1::GraveyardCreaturesCastableThisTurn => {
            let turn = state.turn;
            let grants = state.players[ctx.controller.index()]
                .graveyard
                .iter()
                .copied()
                .filter(|&id| {
                    CARD_DEFS[state.objects.get(id).card_def as usize].has_type(CardType::Creature)
                })
                .map(|id| (id, state.objects.get(id).zone_change_count, ctx.controller, turn))
                .collect::<Vec<_>>();
            let standard = state.standard_v1.get_or_insert_with(Default::default);
            standard.graveyard_casts.retain(|grant| grant.3 == turn);
            standard.graveyard_casts.extend(grants);
        }
        StandardOpV1::SolveSourceCase => {
            if source_incarnation_live(ctx, state)
                && !case_solved(state, ctx.source)
                && case_condition_met(state, ctx.source)
            {
                let zone_change_count = state.objects.get(ctx.source).zone_change_count;
                let standard = state.standard_v1.get_or_insert_with(Default::default);
                standard.solved_cases.push((ctx.source, zone_change_count));
            }
        }
        StandardOpV1::UnlockSourceDoor { door } => {
            if source_incarnation_live(ctx, state) {
                unlock_door(state, ctx.source, *door);
            }
        }
        StandardOpV1::BindCecilDarkness => {}
        StandardOpV1::CecilDarkness { amount } => {
            let amount = i32::try_from(*amount).unwrap_or(i32::MAX);
            if amount > 0 {
                event::propose_and_commit(state, ProposedEvent::life_loss(ctx.controller, amount));
            }
            if state.players[ctx.controller.index()].life <= crate::state::STARTING_LIFE / 2
                && source_incarnation_live(ctx, state)
                && state.objects.get(ctx.source).v4.face_index == 0
            {
                state.objects.get_mut(ctx.source).tapped = false;
                transform_resolving_source(ctx, state);
            }
        }
        StandardOpV1::OtherAttackersGainUntilEndOfTurn { keywords } => {
            let attackers = state
                .engine
                .combat
                .attackers
                .iter()
                .copied()
                .filter(|&id| {
                    let live = state.objects.get(id);
                    id != ctx.source
                        && live.zone == Zone::Battlefield
                        && live.controller == ctx.controller
                })
                .collect::<Vec<_>>();
            for object_id in attackers {
                let timestamp = crate::engine::next_timestamp(state);
                state.engine.until_end_of_turn.push(
                    crate::engine::UntilEndOfTurnEffect::ResolvedObjectKeywordEffect {
                        object_id,
                        object_zone_change_count: state.objects.get(object_id).zone_change_count,
                        layer: crate::engine::Layers::ABILITY_ADDING,
                        timestamp,
                        duration: crate::engine::EffectDuration::EndOfTurn,
                        keywords: *keywords,
                    },
                );
            }
        }
        StandardOpV1::GainControlOfTargetCreatureCopyAtXFive => {
            let Some(Target::Object(object)) = ctx.targets.first().copied() else {
                return;
            };
            let live = state.objects.get(object);
            if live.zone != Zone::Battlefield
                || !crate::engine::object_has_type(state, object, CardType::Creature)
                || mana_value(state, object) > ctx.x_value
            {
                return;
            }
            gain_control(state, object, ctx.controller);
            if ctx.x_value >= 5 {
                create_token_copy(state, object, ctx.controller);
            }
        }
        StandardOpV1::ReturnSourceTappedAndTransformed => {
            let Some(contract) = ctx.ability_source_contract else {
                return;
            };
            let Some(card) = state.objects.try_get(ctx.source) else {
                return;
            };
            // Only the card the dying permanent became, still in the
            // graveyard it went to (400.7).
            if card.card_def != contract.card_def
                || card.zone != Zone::Graveyard
                || card.zone_change_count != contract.zone_change_count + 1
            {
                return;
            }
            let owner = card.owner;
            let mut entry = ProposedEvent::transformed_battlefield_return(ctx.source, 1, owner);
            if let ProposedEvent::ZoneChange(change) = &mut entry {
                change.force_battlefield_tapped = true;
            }
            event::propose_and_commit(state, entry);
        }
        StandardOpV1::AddLoyaltyToSource { amount } => {
            if source_incarnation_live(ctx, state) {
                crate::planeswalker_v1::change_loyalty(state, ctx.source, i32::from(*amount));
            }
        }
        StandardOpV1::ShuffleNonlandPermanentsIntoLibraries { player } => {
            let player = ctx.resolve_player(*player, state);
            let moving = state.players[player.index()]
                .battlefield
                .iter()
                .copied()
                .filter(|&object| !crate::engine::object_has_type(state, object, CardType::Land))
                .collect::<Vec<_>>();
            if moving.is_empty() {
                return;
            }
            let mut owners = moving
                .iter()
                .map(|&object| state.objects.get(object).owner)
                .collect::<Vec<_>>();
            owners.sort_unstable();
            owners.dedup();
            event::propose_and_commit_batch(
                state,
                moving
                    .into_iter()
                    .map(|object| ProposedEvent::zone_change(object, Zone::Library))
                    .collect(),
            );
            for owner in owners {
                if state.shuffle_library(owner).is_err() {
                    state.engine.halted = Some((
                        crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                        ctx.source,
                    ));
                    return;
                }
            }
        }
        StandardOpV1::PlayerChoosesControlledPermanent { .. }
        | StandardOpV1::ApplyChosenPermanent { .. } => {
            panic!("choice-bearing effects must use the resumable interpreter")
        }
    }
}

/// A permanent's mana value (202.3), from its card's mana cost.
fn mana_value(state: &GameState, object: ObjectId) -> u16 {
    CARD_DEFS[state.objects.get(object).card_def as usize].mana_value
}

/// `player` gains control of `object` indefinitely (until it leaves the
/// battlefield). It is summoning sick for its new controller, and leaves
/// combat (506.4).
pub(crate) fn gain_control(state: &mut GameState, object: ObjectId, player: PlayerId) {
    let live = state.objects.get(object);
    if live.zone != Zone::Battlefield || live.controller == player {
        return;
    }
    let previous = live.controller;
    state.players[previous.index()]
        .battlefield
        .retain(|&id| id != object);
    state.players[player.index()].battlefield.push(object);
    let live = state.objects.get_mut(object);
    live.controller = player;
    live.summoning_sick = true;
    state.engine.combat.remove_from_combat(object);
}

/// Creates a token that's a copy of `original` (707.2): the same card
/// definition and shown face, without counters, damage or other state.
pub(crate) fn create_token_copy(state: &mut GameState, original: ObjectId, controller: PlayerId) {
    let live = state.objects.get(original);
    let (card_def, face) = (live.card_def, live.v4.face_index);
    let start = state.engine.event_log.len();
    event::propose_and_commit(state, ProposedEvent::create_token(card_def, controller));
    let created = state.engine.event_log[start..]
        .iter()
        .find_map(|event| match event {
            CommittedEvent::CreateToken { object, .. } => Some(*object),
            _ => None,
        });
    if let Some(token) = created {
        state.objects.get_mut(token).v4.is_token = true;
        if face != 0 && CARD_DEFS[card_def as usize].transform_face.is_some() {
            event::propose_and_commit(state, ProposedEvent::transform_in_place(token, face));
        }
    }
}

const fn trigger(condition: TriggerCondition, effect: fn() -> EffectOp) -> TriggeredAbilityDef {
    TriggeredAbilityDef {
        condition,
        home_zone: Zone::Battlefield,
        intervening_if_kicked: false,
        intervening_if_controls_another_source_card: false,
        effect,
    }
}

// ---- Teferi, Temporal Pilgrim -------------------------------------------

fn teferi_draw_loyalty() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::AddLoyaltyToSource { amount: 1 })
}

const TEFERI_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::ControllerDraws, teferi_draw_loyalty)];

/// "-2: Create a 2/2 blue Spirit creature token with vigilance and 'Whenever
/// you draw a card, put a +1/+1 counter on this creature.'"
pub fn teferi_create_spirit() -> EffectOp {
    EffectOp::CreateToken {
        token_def: crate::card_def::card_id_by_name("Teferi Spirit Token")
            .expect("Teferi Spirit Token in CARD_DEFS"),
        controller: PlayerRef::Controller,
    }
}

/// "-12: Target opponent chooses a permanent they control and returns it to
/// its owner's hand. Then they shuffle each nonland permanent they control
/// into its owner's library."
pub fn teferi_ultimate() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::StandardV1(StandardOpV1::PlayerChoosesControlledPermanent {
            player: PlayerRef::Target(0),
            filter: StandardPermanentFilterV1::Any,
            action: StandardChosenActionV1::ReturnToOwnersHand,
        }),
        EffectOp::StandardV1(StandardOpV1::ShuffleNonlandPermanentsIntoLibraries {
            player: PlayerRef::Target(0),
        }),
    ])
}

fn spirit_draw_counter() -> EffectOp {
    EffectOp::BindPlusOnePlusOneCounterToTriggerSource
}

const TEFERI_SPIRIT_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::ControllerDraws, spirit_draw_counter)];

// ---- Cecil, Dark Knight // Cecil, Redeemed Paladin ----------------------

fn cecil_darkness() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::BindCecilDarkness)
}

fn cecil_protect() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::OtherAttackersGainUntilEndOfTurn {
        keywords: Keywords::INDESTRUCTIBLE,
    })
}

const CECIL_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(TriggerCondition::DealsDamage, cecil_darkness),
    trigger(TriggerCondition::Attacks, cecil_protect),
];

/// Replaces a trigger-time template with the event's data, if `effect` is
/// one of this module's templates.
pub(crate) fn materialize_event(effect: &EffectOp, event: &CommittedEvent) -> Option<EffectOp> {
    match (effect, event) {
        (
            EffectOp::StandardV1(StandardOpV1::BindCecilDarkness),
            CommittedEvent::Damage { amount, .. },
        ) => Some(EffectOp::StandardV1(StandardOpV1::CecilDarkness {
            amount: u32::try_from(*amount).unwrap_or(0),
        })),
        _ => None,
    }
}

/// Whether `effect` is a trigger-time materialization of `template`.
pub(crate) fn template_matches(template: &EffectOp, effect: &EffectOp) -> bool {
    matches!(
        (template, effect),
        (
            EffectOp::StandardV1(StandardOpV1::BindCecilDarkness),
            EffectOp::StandardV1(StandardOpV1::CecilDarkness { .. }),
        )
    )
}

// ---- Polukranos Reborn // Polukranos, Engine of Ruin ----------------------

/// "{6}{W/P}: Transform Polukranos Reborn. Activate only as a sorcery." and
/// Temple of Power's transform ability.
pub fn transform_source() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::TransformSource)
}

fn polukranos_hydras() -> EffectOp {
    let token = |name| crate::card_def::card_id_by_name(name).expect("Phyrexian Hydra token");
    EffectOp::Sequence(vec![
        EffectOp::CreateToken {
            token_def: token("Phyrexian Hydra Reach Token"),
            controller: PlayerRef::Controller,
        },
        EffectOp::CreateToken {
            token_def: token("Phyrexian Hydra Lifelink Token"),
            controller: PlayerRef::Controller,
        },
    ])
}

/// Trigger conditions owned by this module. Appended to `TriggerCondition`
/// as one variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StandardTriggerV1 {
    /// "Whenever this or another nontoken Hydra you control dies", printed on
    /// the back face.
    ThisOrAnotherNontokenHydraYouControlDies,
    /// "At the beginning of your end step", printed on a Room door, which
    /// works only while that door is unlocked.
    ControllerEndStepWhileDoorUnlocked { door: u8 },
    /// "When you unlock this door."
    YouUnlockThisDoor { door: u8 },
    /// "At the beginning of your end step, if you gained life this turn"
    /// (and, with `require_lost`, "and lost life").
    ControllerEndStepIfLifeChanged { require_lost: bool },
    /// "Whenever another artifact you control with mana value N or greater
    /// enters."
    AnotherControlledArtifactEntersManaValueAtLeast(u8),
    /// A Case's "to solve": at the beginning of your end step, if this Case
    /// is unsolved and its condition holds (719.4).
    ControllerEndStepSolveCase,
    /// "At the beginning of combat on your turn."
    ControllerBeginningOfCombat,
    /// "When this Class becomes level N" (716.2c).
    ClassBecomesLevel { level: u8 },
    /// "Whenever you cast an instant or sorcery spell", printed at a Class
    /// level, so it works only from that level on.
    YouCastInstantOrSorceryAtClassLevel { level: u8 },
}

const POLUKRANOS_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(
    TriggerCondition::StandardV1(StandardTriggerV1::ThisOrAnotherNontokenHydraYouControlDies),
    polukranos_hydras,
)];

/// The face `object` showed when the zone change at `events[index]` took
/// it off the battlefield, or `None` when that event is not its departure.
pub(crate) fn departure_face(events: &[CommittedEvent], index: usize, object: ObjectId) -> Option<u8> {
    let CommittedEvent::ZoneChange {
        object: moved,
        from: Zone::Battlefield,
        ..
    } = events.get(index)?
    else {
        return None;
    };
    if *moved != object {
        return None;
    }
    Some(
        events[..index]
            .iter()
            .rev()
            .take(2)
            .find_map(|event| match event {
                CommittedEvent::LeftBattlefieldFaceV1 {
                    object: departed,
                    face_index,
                    ..
                } if *departed == object => Some(*face_index),
                _ => None,
            })
            .unwrap_or(0),
    )
}

/// Whether a departure from the battlefield records its face, so leave
/// triggers printed on one face see the face that left.
pub(crate) fn records_departure_face(def: &CardDef) -> bool {
    def.transform_face.is_some() && matches!(def.name, POLUKRANOS | OJER | CECIL)
}

pub(crate) fn trigger_matches(
    condition: StandardTriggerV1,
    events: &[CommittedEvent],
    index: usize,
    source: ObjectId,
    state: &GameState,
) -> bool {
    match condition {
        StandardTriggerV1::ControllerBeginningOfCombat => {
            let live = state.objects.get(source);
            matches!(
                events[index],
                CommittedEvent::BeginningCombatV1 { active_player }
                    if active_player == live.controller
            ) && live.zone == Zone::Battlefield
                && crate::continuous_characteristics_v1::printed_abilities_active(state, source)
        }
        StandardTriggerV1::ClassBecomesLevel { level } => matches!(
            events[index],
            CommittedEvent::ClassLevelGainedV1 { object, zone_change_count, level: gained }
                if object == source
                    && gained == level
                    && zone_change_count == state.objects.get(source).zone_change_count
        ),
        StandardTriggerV1::YouCastInstantOrSorceryAtClassLevel { level } => {
            let CommittedEvent::SpellCast { spell, controller } = events[index] else {
                return false;
            };
            let live = state.objects.get(source);
            let def = &CARD_DEFS[state.objects.get(spell).card_def as usize];
            controller == live.controller
                && live.zone == Zone::Battlefield
                && crate::continuous_characteristics_v1::printed_abilities_active(state, source)
                && class_level(state, source) >= level
                && (def.has_type(CardType::Instant) || def.has_type(CardType::Sorcery))
        }
        StandardTriggerV1::ControllerEndStepIfLifeChanged { require_lost } => {
            let live = state.objects.get(source);
            controller_end_step(events, index, source, state)
                && life_changed_this_turn(state, live.controller, require_lost)
        }
        StandardTriggerV1::ControllerEndStepSolveCase => {
            controller_end_step(events, index, source, state)
                && !case_solved(state, source)
                && case_condition_met(state, source)
        }
        StandardTriggerV1::AnotherControlledArtifactEntersManaValueAtLeast(minimum) => {
            let Some(object) = crate::trigger::battlefield_entry_object(&events[index]) else {
                return false;
            };
            let live = state.objects.get(source);
            live.zone == Zone::Battlefield
                && crate::continuous_characteristics_v1::printed_abilities_active(state, source)
                && object != source
                && state.objects.get(object).controller == live.controller
                && crate::engine::object_has_type(state, object, CardType::Artifact)
                && mana_value(state, object) >= u16::from(minimum)
        }
        StandardTriggerV1::ControllerEndStepWhileDoorUnlocked { door } => {
            let live = state.objects.get(source);
            matches!(
                events[index],
                CommittedEvent::BeginningEndStep { active_player, .. }
                    if active_player == live.controller
            ) && live.zone == Zone::Battlefield
                && door_unlocked(state, source, door)
        }
        StandardTriggerV1::YouUnlockThisDoor { door } => matches!(
            events[index],
            CommittedEvent::RoomDoorUnlockedV1 { object, zone_change_count, door: unlocked }
                if object == source
                    && unlocked == door
                    && zone_change_count == state.objects.get(source).zone_change_count
        ),
        StandardTriggerV1::ThisOrAnotherNontokenHydraYouControlDies => {
            let CommittedEvent::ZoneChange {
                object,
                from: Zone::Battlefield,
                to: Zone::Graveyard,
                controller_before,
            } = &events[index]
            else {
                return false;
            };
            if *object == source {
                return departure_face(events, index, source) == Some(1);
            }
            // The source sees another Hydra die while it is on the
            // battlefield showing this face, or as it leaves alongside it.
            let source_live = state.objects.get(source);
            let watching = (source_live.zone == Zone::Battlefield
                && source_live.v4.face_index == 1
                && crate::continuous_characteristics_v1::printed_abilities_active(state, source))
                || events.iter().enumerate().any(|(i, _)| {
                    departure_face(events, i, source) == Some(1)
                        && matches!(events[i], CommittedEvent::ZoneChange { to: Zone::Graveyard, .. })
                });
            let source_controller = if source_live.zone == Zone::Battlefield {
                source_live.controller
            } else {
                events
                    .iter()
                    .find_map(|event| match event {
                        CommittedEvent::ZoneChange {
                            object,
                            from: Zone::Battlefield,
                            controller_before,
                            ..
                        } if *object == source => Some(*controller_before),
                        _ => None,
                    })
                    .unwrap_or(source_live.controller)
            };
            let died = state.objects.get(*object);
            let def = &CARD_DEFS[died.card_def as usize];
            watching
                && *controller_before == source_controller
                && !died.v4.is_token
                && def.has_type(CardType::Creature)
                && def.subtypes.contains(&Subtype::Hydra)
        }
    }
}

// ---- Ojer Axonil, Deepest Might // Temple of Power -------------------------

const CECIL: &str = "Cecil, Dark Knight";
const POLUKRANOS: &str = "Polukranos Reborn";
const OJER: &str = "Ojer Axonil, Deepest Might";

fn ojer_returns() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::ReturnSourceTappedAndTransformed)
}

const OJER_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::LeftBattlefieldToGraveyard, ojer_returns)];

/// "If a red source you control would deal an amount of noncombat damage
/// less than Ojer Axonil's power to an opponent, that source deals damage
/// equal to Ojer Axonil's power instead."
pub(crate) fn replace_damage(state: &GameState, damage: &mut DamageProposed) {
    if damage.is_combat || damage.amount <= 0 {
        return;
    }
    let Target::Player(damaged) = damage.target else {
        return;
    };
    let Some(source) = state.objects.try_get(damage.source) else {
        return;
    };
    let controller = source.controller;
    if damaged != controller.opponent()
        || crate::engine::object_color_mask(state, damage.source)
            & crate::card_def::mana_colors_mask(&[ManaColor::R])
            == 0
    {
        return;
    }
    let power = state.players[controller.index()]
        .battlefield
        .iter()
        .copied()
        .filter(|&id| {
            let live = state.objects.get(id);
            live.v4.face_index == 0
                && CARD_DEFS[live.card_def as usize].name == OJER
                && crate::continuous_characteristics_v1::printed_abilities_active(state, id)
        })
        .map(|id| crate::engine::effective_power(state, id))
        .max();
    if let Some(power) = power {
        if damage.amount < power {
            damage.amount = power;
        }
    }
}

/// Records noncombat damage dealt by red sources for Temple of Power.
#[cfg(feature = "standard-magezero-fixtures")]
pub(crate) fn after_damage(state: &mut GameState, source: ObjectId, amount: i32, is_combat: bool) {
    if is_combat || amount <= 0 {
        return;
    }
    let Some(live) = state.objects.try_get(source) else {
        return;
    };
    if crate::engine::object_color_mask(state, source)
        & crate::card_def::mana_colors_mask(&[ManaColor::R])
        == 0
    {
        return;
    }
    let controller = live.controller;
    let turn = state.turn;
    let standard = state.standard_v1.get_or_insert_with(Default::default);
    let (recorded_turn, totals) = standard.red_noncombat_damage.get_or_insert((turn, [0, 0]));
    if *recorded_turn != turn {
        *recorded_turn = turn;
        *totals = [0, 0];
    }
    let total = &mut totals[controller.index()];
    *total = total.saturating_add(amount.unsigned_abs());
}

fn red_noncombat_damage_this_turn(state: &GameState, player: PlayerId) -> u32 {
    state
        .standard_v1
        .as_ref()
        .and_then(|standard| standard.red_noncombat_damage)
        .filter(|(turn, _)| *turn == state.turn)
        .map_or(0, |(_, totals)| totals[player.index()])
}

/// Extra activation restrictions printed on this module's abilities.
pub(crate) fn activation_allowed(
    state: &GameState,
    source: ObjectId,
    def: &CardDef,
    ability_index: usize,
) -> bool {
    if def.name == ROOM {
        // A door can be unlocked only while it is locked.
        return u8::try_from(ability_index)
            .is_ok_and(|door| door < 2 && !door_unlocked(state, source, door));
    }
    if def.name == CASE_OF_THE_UNEATEN_FEAST {
        // "Solved -- Sacrifice this Case: ..."
        return case_solved(state, source);
    }
    if is_class(def.name) {
        // Level N+1 can be gained only at level N (716.2a).
        return usize::from(class_level(state, source)) == ability_index + 1;
    }
    if def.transform_face.is_none() {
        return true;
    }
    match (def.name, ability_index) {
        // "Activate only if red sources you controlled dealt 4 or more
        // noncombat damage this turn."
        (OJER, 0) => {
            red_noncombat_damage_this_turn(state, state.objects.get(source).controller) >= 4
        }
        _ => true,
    }
}

/// Whether `object`'s mana abilities exist on the face it shows.
pub(crate) fn mana_abilities_active(state: &GameState, object: ObjectId, def: &CardDef) -> bool {
    def.transform_face.is_none()
        || def.name != OJER
        || state.objects.get(object).v4.face_index == 1
}

/// Triggered abilities of this module's cards, by registry name.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        "Teferi, Temporal Pilgrim" => &TEFERI_TRIGGERS,
        "Teferi Spirit Token" => &TEFERI_SPIRIT_TRIGGERS,
        CECIL => &CECIL_TRIGGERS,
        POLUKRANOS => &POLUKRANOS_TRIGGERS,
        OJER => &OJER_TRIGGERS,
        ROOM => &ROOM_TRIGGERS,
        CANDY_TRAIL => &SCRY_TWO_ON_ENTRY_TRIGGERS,
        WARLEADERS_CALL => &WARLEADERS_CALL_TRIGGERS,
        LUNAR_CONVOCATION => &LUNAR_CONVOCATION_TRIGGERS,
        SIMULACRUM_SYNTHESIZER => &SIMULACRUM_SYNTHESIZER_TRIGGERS,
        CASE_OF_THE_GATEWAY_EXPRESS => &GATEWAY_EXPRESS_TRIGGERS,
        CASE_OF_THE_UNEATEN_FEAST => &UNEATEN_FEAST_TRIGGERS,
        INNKEEPERS_TALENT => &INNKEEPERS_TALENT_TRIGGERS,
        STORMCHASERS_TALENT => &STORMCHASERS_TALENT_TRIGGERS,
        "Otter Prowess Token" => &PROWESS_TRIGGERS,
        SEAM_RIP | DUSK_ROSE_RELIQUARY | SHELTERED_BY_GHOSTS | HARDLIGHT_CONTAINMENT => {
            &EXILE_UNTIL_LEAVES_TRIGGERS
        }
        _ => &[],
    }
}

/// The transforming-card face each triggered ability is printed on. Abilities
/// of single-faced cards report face 0.
pub(crate) fn trigger_face(name: &str, ability_index: usize) -> u8 {
    match (name, ability_index) {
        (CECIL, 1) | (POLUKRANOS, 0) => 1,
        _ => 0,
    }
}

// ---- Blue Sun's Twilight ---------------------------------------------------

const BLUE_SUNS_TWILIGHT: &str = "Blue Sun's Twilight";

pub fn blue_suns_twilight() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::GainControlOfTargetCreatureCopyAtXFive)
}

/// Whether this card's spell targets depend on more than its target spec.
pub(crate) fn filters_cast_targets(def: &CardDef) -> bool {
    def.cost.x_count != 0 && def.name == BLUE_SUNS_TWILIGHT
}

/// "Target creature with mana value X or less": the kernel announces X after
/// targets, so a creature is a legal choice when some payable X reaches its
/// mana value, and X is then at least that mana value (`minimum_x`).
pub(crate) fn cast_target_allowed(
    def: &CardDef,
    controller: PlayerId,
    target: Target,
    state: &GameState,
) -> bool {
    match target {
        Target::Object(object) if def.name == BLUE_SUNS_TWILIGHT => {
            crate::engine::maximum_payable_x(&def.cost, controller, state)
                .is_some_and(|maximum| mana_value(state, object) <= u16::from(maximum))
        }
        _ => true,
    }
}

/// The least X this spell may announce with these targets.
pub(crate) fn minimum_x(def: &CardDef, targets: &[Target], state: &GameState) -> u8 {
    match (def.name, targets.first()) {
        (BLUE_SUNS_TWILIGHT, Some(Target::Object(object))) if def.cost.x_count != 0 => {
            u8::try_from(mana_value(state, *object)).unwrap_or(u8::MAX)
        }
        _ => 0,
    }
}

// ---- Rooms: Unholy Annex // Ritual Chamber ---------------------------------

const ROOM: &str = "Unholy Annex // Ritual Chamber";

/// Whether `door` (0 left, 1 right) of this exact Room incarnation is
/// unlocked.
pub(crate) fn door_unlocked(state: &GameState, object: ObjectId, door: u8) -> bool {
    let live = state.objects.get(object);
    live.zone == Zone::Battlefield
        && state.standard_v1.as_ref().is_some_and(|standard| {
            standard.rooms.iter().any(|room| {
                room.object == object
                    && room.zone_change_count == live.zone_change_count
                    && room.unlocked & (1 << door) != 0
            })
        })
}

/// Unlocks `door` of a battlefield Room and records the unlock event that
/// "when you unlock this door" abilities trigger on. Unlocking an unlocked
/// door does nothing.
fn unlock_door(state: &mut GameState, object: ObjectId, door: u8) {
    if door > 1 || door_unlocked(state, object, door) {
        return;
    }
    let live = state.objects.get(object);
    if live.zone != Zone::Battlefield {
        return;
    }
    let zone_change_count = live.zone_change_count;
    let standard = state.standard_v1.get_or_insert_with(Default::default);
    // Forget Rooms that have since left the battlefield.
    let objects = &state.objects;
    standard.rooms.retain(|room| {
        objects.try_get(room.object).is_some_and(|live| {
            live.zone == Zone::Battlefield && live.zone_change_count == room.zone_change_count
        })
    });
    match standard
        .rooms
        .iter_mut()
        .find(|room| room.object == object && room.zone_change_count == zone_change_count)
    {
        Some(room) => room.unlocked |= 1 << door,
        None => standard.rooms.push(RoomDoorsV1 {
            object,
            zone_change_count,
            unlocked: 1 << door,
        }),
    }
    state.engine.event_log.push(CommittedEvent::RoomDoorUnlockedV1 {
        object,
        zone_change_count,
        door,
    });
}

/// A resolving Room spell enters the battlefield. The half that was cast is
/// the one whose door unlocks: the left half is the card's normal cost and
/// the right half its alternative cost.
fn room_enters(ctx: &ExecCtx, state: &mut GameState) {
    let door = match state
        .objects
        .get(ctx.source)
        .v4
        .spell_cast_origin
        .and_then(|origin| origin.finalized_method)
    {
        Some(crate::state::CastMethodV4::Normal) => Some(0),
        Some(crate::state::CastMethodV4::Alternative) => Some(1),
        _ => None,
    };
    crate::effect::execute(
        &EffectOp::MoveObject {
            object: crate::effect::ObjectRef::ThisSource,
            to_zone: Zone::Battlefield,
        },
        ctx,
        state,
    );
    if let Some(door) = door {
        if state.objects.get(ctx.source).zone == Zone::Battlefield {
            unlock_door(state, ctx.source, door);
        }
    }
}

pub fn room_enters_program() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::RoomEnters)
}

/// Unlocking a door is a special action (709.5e): it doesn't use the stack.
pub fn unlock_left_door() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::UnlockSourceDoor { door: 0 })
}

pub fn unlock_right_door() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::UnlockSourceDoor { door: 1 })
}

/// Whether this activated ability is a special action that takes effect
/// as soon as its cost is paid instead of using the stack.
pub(crate) fn is_special_action(def: &CardDef, ability_index: u8) -> bool {
    def.name == ROOM && ability_index < 2
}

/// Performs a paid special action of `source`.
pub(crate) fn special_action(state: &mut GameState, source: ObjectId, ability_index: u8) {
    unlock_door(state, source, ability_index);
}

/// Unholy Annex: "At the beginning of your end step, draw a card. If you
/// control a Demon, each opponent loses 2 life and you gain 2 life.
/// Otherwise, you lose 2 life."
fn unholy_annex_end_step() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 1,
        },
        EffectOp::Conditional {
            cond: crate::effect::EffectCond::ControlsOtherSubtypeCount {
                subtype: Subtype::Demon,
                minimum_count: 1,
            },
            then: Box::new(EffectOp::Sequence(vec![
                EffectOp::LoseLife {
                    player: PlayerRef::Opponent,
                    amount: 2,
                },
                EffectOp::GainLife {
                    player: PlayerRef::Controller,
                    amount: 2,
                },
            ])),
            else_: Box::new(EffectOp::LoseLife {
                player: PlayerRef::Controller,
                amount: 2,
            }),
        },
    ])
}

/// Ritual Chamber: "When you unlock this door, create a 6/6 black Demon
/// creature token with flying."
fn ritual_chamber_demon() -> EffectOp {
    EffectOp::CreateToken {
        token_def: crate::card_def::card_id_by_name("Demon Flying Token")
            .expect("Demon Flying Token in CARD_DEFS"),
        controller: PlayerRef::Controller,
    }
}

const ROOM_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(
        TriggerCondition::StandardV1(StandardTriggerV1::ControllerEndStepWhileDoorUnlocked {
            door: 0,
        }),
        unholy_annex_end_step,
    ),
    trigger(
        TriggerCondition::StandardV1(StandardTriggerV1::YouUnlockThisDoor { door: 1 }),
        ritual_chamber_demon,
    ),
];

// ---- Exile until this leaves, and Auras -------------------------------------

const SEAM_RIP: &str = "Seam Rip";
const DUSK_ROSE_RELIQUARY: &str = "Dusk Rose Reliquary";
const SHELTERED_BY_GHOSTS: &str = "Sheltered by Ghosts";
const HARDLIGHT_CONTAINMENT: &str = "Hardlight Containment";

fn exile_target_until_source_leaves() -> EffectOp {
    EffectOp::ExileTargetLinkedToSource {
        object: crate::effect::ObjectRef::Target(0),
    }
}

fn return_exiled_by_source() -> EffectOp {
    EffectOp::ReturnObjectsExiledBySource
}

/// "When this enters, exile target ... until this leaves the battlefield."
const EXILE_UNTIL_LEAVES_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(TriggerCondition::Etb, exile_target_until_source_leaves),
    TriggeredAbilityDef {
        condition: TriggerCondition::LeftBattlefield,
        // Leave triggers use the departed battlefield incarnation.
        home_zone: Zone::Graveyard,
        intervening_if_kicked: false,
        intervening_if_controls_another_source_card: false,
        effect: return_exiled_by_source,
    },
];

/// The target of a triggered ability of this module's cards, by printed
/// name and the trigger's effect; `None` when the module doesn't own it.
pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<crate::card_def::TargetSpec> {
    use crate::card_def::TargetSpec;
    if name == CASE_OF_THE_GATEWAY_EXPRESS && *effect == gateway_express_damage() {
        return Some(TargetSpec::OpponentControlledCreature);
    }
    if name == INNKEEPERS_TALENT && *effect == innkeeper_counter() {
        return Some(TargetSpec::ControlledCreature);
    }
    if name == STORMCHASERS_TALENT && *effect == stormchaser_regrowth() {
        return Some(TargetSpec::StandardV1(
            StandardTargetV1::InstantOrSorceryCardInOwnGraveyard,
        ));
    }
    if *effect != exile_target_until_source_leaves() {
        return None;
    }
    Some(match name {
        SEAM_RIP => TargetSpec::StandardV1(
            StandardTargetV1::OpponentNonlandPermanentManaValueAtMost(2),
        ),
        DUSK_ROSE_RELIQUARY => TargetSpec::StandardV1(StandardTargetV1::OpponentArtifactOrCreature),
        SHELTERED_BY_GHOSTS => TargetSpec::StandardV1(StandardTargetV1::OpponentNonlandPermanent),
        HARDLIGHT_CONTAINMENT => TargetSpec::OpponentControlledCreature,
        _ => return None,
    })
}

pub fn aura_enters() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::AuraEnters)
}

/// Whether `def` is one of this module's Auras.
pub(crate) fn is_aura(def: &CardDef) -> bool {
    matches!(def.name, SHELTERED_BY_GHOSTS | HARDLIGHT_CONTAINMENT)
}

/// Whether this module's Aura `def`, controlled by `controller`, may
/// enchant `host`: Sheltered by Ghosts "enchant creature you control",
/// Hardlight Containment "enchant artifact you control". `None` for every
/// other definition.
pub(crate) fn aura_host_legal(
    def: &CardDef,
    controller: PlayerId,
    host: ObjectId,
    state: &GameState,
) -> Option<bool> {
    let live = state.objects.get(host);
    let type_ok = |card_type| crate::engine::object_has_type(state, host, card_type);
    let card_type = match def.name {
        SHELTERED_BY_GHOSTS => CardType::Creature,
        HARDLIGHT_CONTAINMENT => CardType::Artifact,
        _ => return None,
    };
    Some(live.zone == Zone::Battlefield && live.controller == controller && type_ok(card_type))
}

fn aura_enters_battlefield(ctx: &ExecCtx, state: &mut GameState) {
    let Some(Target::Object(host)) = ctx.targets.first().copied() else {
        return;
    };
    let def = &CARD_DEFS[state.objects.get(ctx.source).card_def as usize];
    let legal = state.objects.get(ctx.source).zone == Zone::Stack
        && ctx.target_incarnation_matches(0, state)
        && aura_host_legal(def, ctx.controller, host, state) == Some(true);
    if !legal {
        state.engine.halted = Some((
            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            ctx.source,
        ));
        return;
    }
    event::propose_and_commit(state, ProposedEvent::zone_change(ctx.source, Zone::Battlefield));
    let aura = state.objects.get(ctx.source);
    if aura.zone != Zone::Battlefield {
        return;
    }
    let aura_generation = aura.zone_change_count;
    let host_generation = state.objects.get(host).zone_change_count;
    if state
        .attach_object_exact(ctx.source, aura_generation, host, host_generation)
        .is_err()
    {
        state.engine.halted = Some((
            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            ctx.source,
        ));
    }
}

/// Ward amounts `host` gains from this module's Auras attached to it
/// ("Enchanted permanent has ward {N}").
pub(crate) fn granted_wards(state: &GameState, host: ObjectId) -> Vec<u8> {
    let Some(host_live) = state.objects.try_get(host) else {
        return Vec::new();
    };
    let link = crate::state::ObjectLinkV4 {
        object: host,
        zone_change_count: host_live.zone_change_count,
    };
    let mut wards = host_live
        .attachments
        .iter()
        .filter_map(|&attached| {
            let aura = state.objects.try_get(attached)?;
            if aura.zone != Zone::Battlefield || aura.v4.attached_to != Some(link) {
                return None;
            }
            granted_ward_of(CARD_DEFS[aura.card_def as usize].name)
        })
        .collect::<Vec<_>>();
    // Innkeeper's Talent level 2: "Permanents you control with counters on
    // them have ward {1}."
    if host_live.zone == Zone::Battlefield && has_counters(state, host) {
        let classes = controlled_classes_at(state, host_live.controller, INNKEEPERS_TALENT, 2);
        wards.extend((0..classes).map(|_| 1));
    }
    wards
}

fn granted_ward_of(name: &str) -> Option<u8> {
    match name {
        SHELTERED_BY_GHOSTS => Some(2),
        HARDLIGHT_CONTAINMENT => Some(1),
        _ => None,
    }
}

/// Whether some definition in this catalog grants ward `generic`, so a
/// ward trigger restored after its Aura left can still be recognized.
pub(crate) fn ward_grant_exists(generic: u8) -> bool {
    CARD_DEFS
        .iter()
        .any(|def| granted_ward_of(def.name) == Some(generic))
}

// ---- Life, attack and anthem watchers --------------------------------------

/// Records a life total change for this turn's "gained life"/"lost life"
/// conditions.
#[cfg(feature = "standard-magezero-fixtures")]
pub(crate) fn record_life(state: &mut GameState, player: PlayerId, gained: i32, lost: i32) {
    if gained <= 0 && lost <= 0 {
        return;
    }
    let turn = state.turn;
    let standard = state.standard_v1.get_or_insert_with(Default::default);
    let (recorded_turn, gains, losses) = standard.life_this_turn.get_or_insert((turn, [0, 0], [0, 0]));
    if *recorded_turn != turn {
        *recorded_turn = turn;
        *gains = [0, 0];
        *losses = [0, 0];
    }
    let index = player.index();
    gains[index] = gains[index].saturating_add(gained.max(0).unsigned_abs());
    losses[index] = losses[index].saturating_add(lost.max(0).unsigned_abs());
}

fn life_gained_this_turn(state: &GameState, player: PlayerId) -> u32 {
    state
        .standard_v1
        .as_ref()
        .and_then(|standard| standard.life_this_turn)
        .filter(|(turn, _, _)| *turn == state.turn)
        .map_or(0, |(_, gains, _)| gains[player.index()])
}

fn life_changed_this_turn(state: &GameState, player: PlayerId, require_lost: bool) -> bool {
    state
        .standard_v1
        .as_ref()
        .and_then(|standard| standard.life_this_turn)
        .filter(|(turn, _, _)| *turn == state.turn)
        .is_some_and(|(_, gains, losses)| {
            gains[player.index()] > 0 && (!require_lost || losses[player.index()] > 0)
        })
}

/// Records declared attackers for this turn's "creatures attacked this
/// turn" count.
#[cfg(feature = "standard-magezero-fixtures")]
pub(crate) fn record_attackers(state: &mut GameState, count: usize) {
    if count == 0 {
        return;
    }
    let turn = state.turn;
    let standard = state.standard_v1.get_or_insert_with(Default::default);
    let (recorded_turn, total) = standard.attackers_this_turn.get_or_insert((turn, 0));
    if *recorded_turn != turn {
        *recorded_turn = turn;
        *total = 0;
    }
    *total = total.saturating_add(u32::try_from(count).unwrap_or(u32::MAX));
}

fn attackers_this_turn(state: &GameState) -> u32 {
    state
        .standard_v1
        .as_ref()
        .and_then(|standard| standard.attackers_this_turn)
        .filter(|(turn, _)| *turn == state.turn)
        .map_or(0, |(_, total)| total)
}

/// "At the beginning of your end step", for a source on the battlefield
/// whose printed abilities function.
fn controller_end_step(
    events: &[CommittedEvent],
    index: usize,
    source: ObjectId,
    state: &GameState,
) -> bool {
    let live = state.objects.get(source);
    matches!(
        events[index],
        CommittedEvent::BeginningEndStep { active_player, .. } if active_player == live.controller
    ) && live.zone == Zone::Battlefield
        && crate::continuous_characteristics_v1::printed_abilities_active(state, source)
}

/// Power and toughness this module's static abilities give `object`:
/// Warleader's Call's and a solved Case of the Gateway Express's anthems,
/// and the Construct token's "+1/+1 for each artifact you control".
#[cfg(feature = "standard-magezero-fixtures")]
pub(crate) fn controlled_boost(state: &GameState, object: ObjectId) -> (i32, i32) {
    let live = state.objects.get(object);
    if live.zone != Zone::Battlefield
        || !crate::engine::object_has_type(state, object, CardType::Creature)
    {
        return (0, 0);
    }
    let controller = live.controller;
    let mut boost = (0, 0);
    let mut artifacts = 0;
    for &id in &state.players[controller.index()].battlefield {
        if crate::engine::object_has_type(state, id, CardType::Artifact) {
            artifacts += 1;
        }
        let name = CARD_DEFS[state.objects.get(id).card_def as usize].name;
        if !matches!(name, WARLEADERS_CALL | CASE_OF_THE_GATEWAY_EXPRESS)
            || !crate::continuous_characteristics_v1::printed_abilities_active(state, id)
        {
            continue;
        }
        if name == WARLEADERS_CALL {
            boost.0 += 1;
            boost.1 += 1;
        } else if case_solved(state, id) {
            boost.0 += 1;
        }
    }
    if CARD_DEFS[live.card_def as usize].name == KARN_CONSTRUCT_TOKEN
        && crate::continuous_characteristics_v1::printed_abilities_active(state, object)
    {
        boost.0 += artifacts;
        boost.1 += artifacts;
    }
    boost
}

// ---- Candy Trail, Warleader's Call, Lunar Convocation, Simulacrum ----------

const CANDY_TRAIL: &str = "Candy Trail";
const WARLEADERS_CALL: &str = "Warleader's Call";
const LUNAR_CONVOCATION: &str = "Lunar Convocation";
const SIMULACRUM_SYNTHESIZER: &str = "Simulacrum Synthesizer";
const KARN_CONSTRUCT_TOKEN: &str = "Karn Construct Token";

fn scry_two() -> EffectOp {
    EffectOp::Scry {
        player: PlayerRef::Controller,
        count: 2,
    }
}

/// "When this enters, scry 2."
const SCRY_TWO_ON_ENTRY_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, scry_two)];

/// Candy Trail: "{2}, {T}, Sacrifice this: You gain 3 life and draw a card."
pub fn candy_trail_sacrifice() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::GainLife {
            player: PlayerRef::Controller,
            amount: 3,
        },
        EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 1,
        },
    ])
}

fn warleaders_call_damage() -> EffectOp {
    EffectOp::DealDamage {
        target: crate::effect::TargetRef::Opponent,
        amount: 1,
    }
}

/// "Whenever a creature you control enters, this deals 1 damage to each
/// opponent." Its anthem is `controlled_boost`.
const WARLEADERS_CALL_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(
    TriggerCondition::OtherControlledCreatureEnters { subtype: None },
    warleaders_call_damage,
)];

fn lunar_convocation_drain() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::OpponentLosesLifeIfYouGainedLife { amount: 1 })
}

fn lunar_convocation_bat() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::CreateTokenIfYouGainedAndLostLife {
        token_def: crate::card_def::card_id_by_name("Bat Flying Token")
            .expect("Bat Flying Token in CARD_DEFS"),
    })
}

const LUNAR_CONVOCATION_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(
        TriggerCondition::StandardV1(StandardTriggerV1::ControllerEndStepIfLifeChanged {
            require_lost: false,
        }),
        lunar_convocation_drain,
    ),
    trigger(
        TriggerCondition::StandardV1(StandardTriggerV1::ControllerEndStepIfLifeChanged {
            require_lost: true,
        }),
        lunar_convocation_bat,
    ),
];

fn simulacrum_construct() -> EffectOp {
    EffectOp::CreateToken {
        token_def: crate::card_def::card_id_by_name(KARN_CONSTRUCT_TOKEN)
            .expect("Karn Construct Token in CARD_DEFS"),
        controller: PlayerRef::Controller,
    }
}

const SIMULACRUM_SYNTHESIZER_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(TriggerCondition::Etb, scry_two),
    trigger(
        TriggerCondition::StandardV1(
            StandardTriggerV1::AnotherControlledArtifactEntersManaValueAtLeast(3),
        ),
        simulacrum_construct,
    ),
];

// ---- Cases -------------------------------------------------------------------

const CASE_OF_THE_GATEWAY_EXPRESS: &str = "Case of the Gateway Express";

/// Whether `object`'s current battlefield incarnation is a solved Case.
pub(crate) fn case_solved(state: &GameState, object: ObjectId) -> bool {
    let zone_change_count = state.objects.get(object).zone_change_count;
    state.standard_v1.as_ref().is_some_and(|standard| {
        standard
            .solved_cases
            .contains(&(object, zone_change_count))
    })
}

/// A Case's "to solve" condition.
fn case_condition_met(state: &GameState, object: ObjectId) -> bool {
    match CARD_DEFS[state.objects.get(object).card_def as usize].name {
        // "Three or more creatures attacked this turn."
        CASE_OF_THE_GATEWAY_EXPRESS => attackers_this_turn(state) >= 3,
        // "You've gained 5 or more life this turn."
        CASE_OF_THE_UNEATEN_FEAST => {
            life_gained_this_turn(state, state.objects.get(object).controller) >= 5
        }
        _ => false,
    }
}

fn gateway_express_damage() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EachControlledCreatureDealsOneDamageToTarget)
}

fn solve_source_case() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::SolveSourceCase)
}

/// "When this Case enters, choose target creature you don't control. Each
/// creature you control deals 1 damage to that creature." To solve: three
/// or more creatures attacked this turn. Solved: creatures you control get
/// +1/+0 (`controlled_boost`).
const GATEWAY_EXPRESS_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(TriggerCondition::Etb, gateway_express_damage),
    trigger(
        TriggerCondition::StandardV1(StandardTriggerV1::ControllerEndStepSolveCase),
        solve_source_case,
    ),
];

// ---- Classes -----------------------------------------------------------------

const INNKEEPERS_TALENT: &str = "Innkeeper's Talent";
const STORMCHASERS_TALENT: &str = "Stormchaser's Talent";

fn is_class(name: &str) -> bool {
    matches!(name, INNKEEPERS_TALENT | STORMCHASERS_TALENT)
}

/// A Class permanent's level; a Class enters at level 1 (716.3).
pub(crate) fn class_level(state: &GameState, object: ObjectId) -> u8 {
    let zone_change_count = state.objects.get(object).zone_change_count;
    state
        .standard_v1
        .as_ref()
        .and_then(|standard| {
            standard
                .class_levels
                .iter()
                .find(|(id, count, _)| *id == object && *count == zone_change_count)
        })
        .map_or(1, |(_, _, level)| *level)
}

fn set_class_level(state: &mut GameState, object: ObjectId, level: u8) {
    let zone_change_count = state.objects.get(object).zone_change_count;
    let live = |entry: &(ObjectId, u32, u8)| {
        state
            .objects
            .try_get(entry.0)
            .is_some_and(|live| live.zone == Zone::Battlefield && live.zone_change_count == entry.1)
    };
    let mut levels = state
        .standard_v1
        .as_ref()
        .map(|standard| standard.class_levels.clone())
        .unwrap_or_default();
    levels.retain(|entry| live(entry) && entry.0 != object);
    levels.push((object, zone_change_count, level));
    state.standard_v1.get_or_insert_with(Default::default).class_levels = levels;
    let committed = CommittedEvent::ClassLevelGainedV1 {
        object,
        zone_change_count,
        level,
    };
    state.engine.event_log.push(committed.clone());
    state.engine.event_history.push(committed);
}

pub fn gain_level_two() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::GainClassLevel { level: 2 })
}

pub fn gain_level_three() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::GainClassLevel { level: 3 })
}

/// Whether `player` controls a permanent showing a Class at `level` or
/// higher with functioning abilities, counted once per such Class.
fn controlled_classes_at(state: &GameState, player: PlayerId, name: &str, level: u8) -> u32 {
    let count = state.players[player.index()]
        .battlefield
        .iter()
        .filter(|&&id| {
            CARD_DEFS[state.objects.get(id).card_def as usize].name == name
                && class_level(state, id) >= level
                && crate::continuous_characteristics_v1::printed_abilities_active(state, id)
        })
        .count();
    u32::try_from(count).unwrap_or(u32::MAX)
}

/// Innkeeper's Talent level 3: "If you would put one or more counters on a
/// permanent or player, put twice that many of each of those kinds of
/// counters on that permanent or player instead." Each such Class doubles
/// again (616.1).
#[cfg(feature = "standard-magezero-fixtures")]
pub(crate) fn scale_counters(state: &GameState, player: PlayerId, count: i32) -> i32 {
    if count <= 0 {
        return count;
    }
    let doublings = controlled_classes_at(state, player, INNKEEPERS_TALENT, 3);
    (0..doublings).fold(count, |count, _| count.saturating_mul(2))
}

/// Whether `object` has any counters on it.
fn has_counters(state: &GameState, object: ObjectId) -> bool {
    let live = state.objects.get(object);
    let counters = &live.counters;
    counters.plus1_plus1 > 0
        || counters.minus1_minus1 > 0
        || counters.minus0_minus1 > 0
        || counters.stun > 0
        || counters.lore > 0
        || live.v4.lifelink_keyword_counters > 0
        || crate::planeswalker_v1::loyalty(state, object).is_some_and(|loyalty| loyalty > 0)
}

fn innkeeper_counter() -> EffectOp {
    EffectOp::AddCountersToTarget {
        target_index: 0,
        optional: false,
        plus1_plus1: 1,
        lifelink: 0,
        stun: 0,
    }
}

/// Level 1: "At the beginning of combat on your turn, put a +1/+1 counter
/// on target creature you control." Level 2's ward is `granted_wards`;
/// level 3's doubling is `scale_counters`.
const INNKEEPERS_TALENT_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(
    TriggerCondition::StandardV1(StandardTriggerV1::ControllerBeginningOfCombat),
    innkeeper_counter,
)];

fn otter_token() -> EffectOp {
    EffectOp::CreateToken {
        token_def: crate::card_def::card_id_by_name("Otter Prowess Token")
            .expect("Otter Prowess Token in CARD_DEFS"),
        controller: PlayerRef::Controller,
    }
}

fn stormchaser_regrowth() -> EffectOp {
    EffectOp::MoveObject {
        object: crate::effect::ObjectRef::Target(0),
        to_zone: Zone::Hand,
    }
}

/// Level 1: an Otter on entry. Level 2: "When this Class becomes level 2,
/// return target instant or sorcery card from your graveyard to your hand."
/// Level 3: "Whenever you cast an instant or sorcery spell, create" an
/// Otter.
const STORMCHASERS_TALENT_TRIGGERS: [TriggeredAbilityDef; 3] = [
    trigger(TriggerCondition::Etb, otter_token),
    trigger(
        TriggerCondition::StandardV1(StandardTriggerV1::ClassBecomesLevel { level: 2 }),
        stormchaser_regrowth,
    ),
    trigger(
        TriggerCondition::StandardV1(StandardTriggerV1::YouCastInstantOrSorceryAtClassLevel {
            level: 3,
        }),
        otter_token,
    ),
];

fn prowess_boost() -> EffectOp {
    EffectOp::BindTemporaryBoostToTriggerSource {
        power: 1,
        toughness: 1,
    }
}

/// Prowess (702.108): "Whenever you cast a noncreature spell, this creature
/// gets +1/+1 until end of turn."
const PROWESS_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::CastNoncreatureSpell, prowess_boost)];

// ---- Case of the Uneaten Feast ----------------------------------------------

const CASE_OF_THE_UNEATEN_FEAST: &str = "Case of the Uneaten Feast";

/// Whether `holder` may cast `object` from its graveyard incarnation
/// `zone_change_count` this turn.
pub(crate) fn graveyard_cast_granted(
    state: &GameState,
    object: ObjectId,
    holder: PlayerId,
    zone_change_count: u32,
) -> bool {
    state.standard_v1.as_ref().is_some_and(|standard| {
        standard
            .graveyard_casts
            .contains(&(object, zone_change_count, holder, state.turn))
    })
}

fn gain_one_life() -> EffectOp {
    EffectOp::GainLife {
        player: PlayerRef::Controller,
        amount: 1,
    }
}

/// "Solved -- Sacrifice this Case: Creature cards in your graveyard gain
/// 'You may cast this card from your graveyard' until end of turn." Usable
/// only while solved (`activation_allowed`).
pub fn uneaten_feast_grant() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::GraveyardCreaturesCastableThisTurn)
}

/// "Whenever a creature you control enters, you gain 1 life." To solve:
/// you've gained 5 or more life this turn.
const UNEATEN_FEAST_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(
        TriggerCondition::OtherControlledCreatureEnters { subtype: None },
        gain_one_life,
    ),
    trigger(
        TriggerCondition::StandardV1(StandardTriggerV1::ControllerEndStepSolveCase),
        solve_source_case,
    ),
];
