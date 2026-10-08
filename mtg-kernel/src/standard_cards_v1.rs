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
