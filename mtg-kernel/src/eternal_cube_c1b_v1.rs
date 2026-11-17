//! Card programs for the eternal catalog's Vintage Cube wave C1b
//! (`docs/design/eternal_formats_support_v1.md`): black and green instants and sorceries.
//!
//! `eternal_cards_v1` forwards this module's effect leaves here, and the
//! recipe tables of `build_eternal_c1b_v1.rs` name its programs. Every
//! table is keyed by a printed name that occurs only in the eternal registry,
//! so the Pauper, FDN and Standard catalogs never reach it. Behavior was read
//! from each card's current Oracle text and, where present, its XMage source.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

pub(crate) mod targets;

use crate::card_def::{CardDef, CardType, DynamicValueDef, Keywords, TargetSpec, CARD_DEFS};
use crate::effect::{
    EffectContinuation, EffectObjectBinding, EffectOp, ExecCtx, LibraryCardFilter,
    LibrarySearchDestinationV1, ObjectRef, PlayerRef, TargetRef,
};
use crate::eternal_cards_v1::e3::{E3ChoiceKindV1, E3ChoiceV1};
use crate::eternal_cards_v1::EternalOpV1;
use crate::ids::{ObjectId, PlayerId};
use crate::standard_cards_v1::{StandardOpV1, StandardTargetV1};
use crate::state::{GameState, Target, Zone};
use crate::trigger::{TriggerCondition, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};

const COLLECTIVE_BRUTALITY: &str = "Collective Brutality";
const PEST_INFESTATION: &str = "Pest Infestation";
const PEST_TOKEN: &str = "Pest Token";

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::C1bV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum C1bOpV1 {
    /// Hymn to Tourach: the target player discards two cards at random.
    HymnV1,
    /// Mind Twist: the target player discards X cards at random.
    MindTwistV1,
    /// Chain of Smog: the target player discards two cards of their choice,
    /// then may copy the spell and choose a new target for the copy. A
    /// choice.
    ChainSmogV1,
    /// Collective Brutality's first mode: the opponent in target slot
    /// `slot` reveals their hand, and the caster chooses an instant or
    /// sorcery card from it to be discarded. A choice. `mask` names the
    /// chosen modes, for the target filter.
    BrutalityRevealV1 { slot: u8, mask: u8 },
    /// Collective Brutality's second mode: the creature in target slot
    /// `slot` gets -2/-2 until end of turn.
    BrutalityShrinkV1 { slot: u8, mask: u8 },
    /// Collective Brutality's third mode: the opponent in target slot `slot`
    /// loses 2 life and the caster gains 2 life.
    BrutalityDrainV1 { slot: u8, mask: u8 },
    /// Pest Infestation: destroy each target artifact or enchantment, then
    /// create twice X Pest tokens.
    PestInfestationV1,
}

impl C1bOpV1 {
    /// Whether the leaf may stop for a player's choice (or a draw that a
    /// replacement can interrupt), so it runs in the resumable interpreter.
    /// Random discards ask nobody anything.
    pub(crate) fn contains_player_choice(&self) -> bool {
        matches!(self, Self::ChainSmogV1 | Self::BrutalityRevealV1 { .. })
    }
}

/// A choice of this module, carried as `E3ChoiceKindV1::C1bV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum C1bChoiceV1 {
    /// Chain of Smog: the two cards its target player discards.
    ChainSmogDiscardV1,
    /// Chain of Smog: the new target player of the copy, or none to decline
    /// the copy.
    ChainSmogCopyTargetV1,
    /// Collective Brutality: the instant or sorcery card to discard from the
    /// revealed hand.
    BrutalityDiscardV1,
}

impl C1bChoiceV1 {
    /// Whether the candidates are hidden from the other player: a hand the
    /// chooser discards from. The copy's players are public, and the
    /// revealed hand is shown to both.
    pub(crate) fn chooser_private(&self) -> bool {
        matches!(self, Self::ChainSmogDiscardV1)
    }

    pub(crate) fn public_purpose(&self) -> crate::rl::TargetSelectionPurposeV4 {
        match self {
            Self::ChainSmogCopyTargetV1 => crate::rl::TargetSelectionPurposeV4::PlayerSelection,
            Self::ChainSmogDiscardV1 | Self::BrutalityDiscardV1 => {
                crate::rl::TargetSelectionPurposeV4::CardSelection
            }
        }
    }

    /// Object incarnations this choice refers to beyond its candidates.
    pub(crate) fn referenced_bindings(&self) -> Vec<EffectObjectBinding> {
        Vec::new()
    }
}

fn c1b(op: C1bOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::C1bV1(op)))
}

fn halt(state: &mut GameState, source: ObjectId) {
    state.engine.halted = Some((
        crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
        source,
    ));
}

fn bind(state: &GameState, object: ObjectId) -> EffectObjectBinding {
    let live = state.objects.get(object);
    EffectObjectBinding {
        object,
        expected_zone: live.zone,
        expected_zone_change_count: live.zone_change_count,
    }
}

fn definition(state: &GameState, object: ObjectId) -> Option<&'static CardDef> {
    let live = state.objects.try_get(object)?;
    CARD_DEFS.get(usize::from(live.card_def))
}

// ---- Programs -----------------------------------------------------------------------

/// Damnation: "Destroy all creatures. They can't be regenerated." The same
/// text and program as Wrath of God (wave P1b).
pub fn damnation() -> EffectOp {
    crate::eternal_premodern_p1b_v1::wrath_of_god()
}

/// Entomb: "Search your library for a card, put that card into your
/// graveyard, then shuffle." The search is private to its controller (the
/// same destination as Enroot's).
pub fn entomb() -> EffectOp {
    EffectOp::SearchLibraryCardsToDestination {
        player: PlayerRef::Controller,
        filter: LibraryCardFilter::AnyCard,
        max_targets: 1,
        destination: LibrarySearchDestinationV1::GraveyardV1,
    }
}

/// Hymn to Tourach: "Target player discards two cards at random."
pub fn hymn_to_tourach() -> EffectOp {
    c1b(C1bOpV1::HymnV1)
}

/// Imperial Seal: "Search your library for a card, then shuffle and put that
/// card on top. You lose 2 life." The same text and program as Vampiric
/// Tutor (wave E5a).
pub fn imperial_seal() -> EffectOp {
    crate::eternal_e5a_v1::vampiric_tutor()
}

/// Infernal Grasp: "Destroy target creature. You lose 2 life."
pub fn infernal_grasp() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::DestroyObject {
            object: ObjectRef::Target(0),
        },
        EffectOp::LoseLife {
            player: PlayerRef::Controller,
            amount: 2,
        },
    ])
}

/// Mind Twist: "Target player discards X cards at random."
pub fn mind_twist() -> EffectOp {
    c1b(C1bOpV1::MindTwistV1)
}

/// Night's Whisper: "You draw two cards and lose 2 life." The draw is its
/// own op, so Dredge and other draw replacements can interrupt it.
pub fn nights_whisper() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 2,
        },
        EffectOp::LoseLife {
            player: PlayerRef::Controller,
            amount: 2,
        },
    ])
}

fn destroy_target() -> EffectOp {
    EffectOp::DestroyObject {
        object: ObjectRef::Target(0),
    }
}

/// Bloodchief's Thirst: "Destroy target creature or planeswalker with mana
/// value 2 or less. If this spell was kicked, instead destroy target
/// creature or planeswalker." The kicked spell's wider target is its
/// `kicked_target_spec`, so the one program destroys target 0 either way.
pub fn bloodchiefs_thirst() -> EffectOp {
    destroy_target()
}

/// Bone Shards: "Destroy target creature or planeswalker." Its choice
/// between two additional costs is its two modes (`eternal_cost_choice_v1`).
pub fn bone_shards() -> EffectOp {
    destroy_target()
}

/// Chain of Smog: "Target player discards two cards. That player may copy
/// this spell and may choose a new target for that copy." The player picks
/// the two cards (a hand of two or fewer is discarded whole), then the copy
/// is Chain of Vapor's. The discard is this module's own choice, not
/// `EffectOp::DiscardCards`, whose resumable form cannot be followed by
/// another choice of the same spell.
pub fn chain_of_smog() -> EffectOp {
    c1b(C1bOpV1::ChainSmogV1)
}

/// Collective Brutality's mode `mask` (bit 0 reveal, bit 1 shrink, bit 2
/// drain), run in printed order.
fn brutality(mask: u8) -> EffectOp {
    let mut ops = Vec::new();
    let mut slot = 0;
    if mask & 1 != 0 {
        ops.push(c1b(C1bOpV1::BrutalityRevealV1 { slot, mask }));
        slot += 1;
    }
    if mask & 2 != 0 {
        ops.push(c1b(C1bOpV1::BrutalityShrinkV1 { slot, mask }));
        slot += 1;
    }
    if mask & 4 != 0 {
        ops.push(c1b(C1bOpV1::BrutalityDrainV1 { slot, mask }));
    }
    EffectOp::Sequence(ops)
}

/// Collective Brutality, mode 0: "Target opponent reveals their hand. You
/// choose an instant or sorcery card from it. That player discards that
/// card."
pub fn collective_brutality() -> EffectOp {
    brutality(1)
}

/// Mode 1: "Target creature gets -2/-2 until end of turn."
pub fn collective_brutality_1() -> EffectOp {
    brutality(2)
}

/// Mode 2: the first two modes.
pub fn collective_brutality_2() -> EffectOp {
    brutality(3)
}

/// Mode 3: "Target opponent loses 2 life and you gain 2 life."
pub fn collective_brutality_3() -> EffectOp {
    brutality(4)
}

/// Mode 4: the first and third modes.
pub fn collective_brutality_4() -> EffectOp {
    brutality(5)
}

/// Mode 5: the second and third modes.
pub fn collective_brutality_5() -> EffectOp {
    brutality(6)
}

/// Mode 6: all three modes.
pub fn collective_brutality_6() -> EffectOp {
    brutality(7)
}

/// Natural Order: "Search your library for a green creature card, put it
/// onto the battlefield, then shuffle." Its sacrifice is the additional
/// cost.
pub fn natural_order() -> EffectOp {
    EffectOp::SearchLibraryCardsToDestination {
        player: PlayerRef::Controller,
        filter: LibraryCardFilter::GreenCreatureV1,
        max_targets: 1,
        destination: LibrarySearchDestinationV1::Battlefield { tapped: false },
    }
}

/// Pest Infestation: "Destroy up to X target artifacts and/or enchantments.
/// Create twice X 1/1 black and green Pest creature tokens with 'When this
/// token dies, you gain 1 life.'"
pub fn pest_infestation() -> EffectOp {
    c1b(C1bOpV1::PestInfestationV1)
}

/// Tamiyo's Safekeeping: "Target permanent you control gains hexproof and
/// indestructible until end of turn. You gain 2 life."
pub fn tamiyos_safekeeping() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::GrantKeywordTargetUntilEndOfTurn {
            object: ObjectRef::Target(0),
            keyword: Keywords::HEXPROOF,
        },
        EffectOp::GrantKeywordTargetUntilEndOfTurn {
            object: ObjectRef::Target(0),
            keyword: Keywords::INDESTRUCTIBLE,
        },
        EffectOp::GainLife {
            player: PlayerRef::Controller,
            amount: 2,
        },
    ])
}

// ---- Costs ----------------------------------------------------------------------------

/// The cards escalate makes `mode` of Collective Brutality discard: one for
/// each chosen mode beyond the first. Mode `m` chooses the modes of the
/// bits of `m + 1`.
fn escalate_discards(mode: u8) -> u8 {
    let chosen = u32::from(mode).saturating_add(1).count_ones();
    u8::try_from(chosen.saturating_sub(1)).unwrap_or(u8::MAX)
}

/// Collective Brutality's additional discard: its escalate count for the
/// chosen mode, none for a single mode. `None` for every other card.
pub(crate) fn additional_discard_count(def: &CardDef, mode: Option<u8>) -> Option<Option<u8>> {
    (def.name == COLLECTIVE_BRUTALITY)
        .then(|| mode.map(escalate_discards).filter(|&discards| discards > 0))
}

/// Pest Infestation: X is at least the number of targets ("up to X").
pub(crate) fn minimum_x(name: &str, targets: &[Target]) -> Option<u8> {
    (name == PEST_INFESTATION).then(|| u8::try_from(targets.len()).unwrap_or(u8::MAX))
}

/// Whether the cast targets of `def` are limited by the X they leave payable.
pub(crate) fn filters_cast_targets(def: &CardDef) -> bool {
    def.name == PEST_INFESTATION
}

/// Pest Infestation: one more target is legal only when an X covering all
/// the targets so far and this one is payable.
pub(crate) fn cast_target_allowed(
    def: &CardDef,
    source: ObjectId,
    controller: PlayerId,
    target: Target,
    state: &GameState,
) -> bool {
    let chosen = state
        .engine
        .pending_cast
        .as_ref()
        .filter(|pending| pending.spell == source)
        .map_or(0, |pending| pending.targets_chosen.len());
    let minimum = u16::try_from(chosen.saturating_add(1)).unwrap_or(u16::MAX);
    crate::engine::x_target_is_payable(def, source, controller, target, minimum, state)
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

/// The Pest token: "When this token dies, you gain 1 life."
fn pest_dies() -> EffectOp {
    EffectOp::GainLife {
        player: PlayerRef::Controller,
        amount: 1,
    }
}

const PEST_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(
    TriggerCondition::LeftBattlefieldToGraveyard,
    pest_dies,
)];

/// Triggered abilities of this module's cards, falling through to wave
/// C1c.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        PEST_TOKEN => &PEST_TRIGGERS,
        _ => crate::eternal_cube_c1c_v1::triggers_for(name),
    }
}

/// Target specs of this module's printed and queued triggered abilities.
pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    let own = name == PEST_TOKEN && *effect == pest_dies();
    (own || delayed_effect_matches(name, effect)).then_some(TargetSpec::None)
}

/// Queued triggers owned by this module's cards.
pub(crate) fn delayed_effect_matches(_name: &str, _effect: &EffectOp) -> bool {
    false
}

// ---- Execution ----------------------------------------------------------------------

/// Collective Brutality's target filter for the modes `mask`.
fn brutality_spec(mask: u8) -> TargetSpec {
    match mask {
        1 | 4 => TargetSpec::TargetOpponent,
        2 => TargetSpec::Creature,
        mask => TargetSpec::StandardV1(StandardTargetV1::C1bV1(
            targets::C1bTargetV1::BrutalitySlotsV1(mask),
        )),
    }
}

/// Whether target `slot` of a Collective Brutality mode is still the
/// incarnation announced and still legal (608.2b).
fn slot_legal(ctx: &ExecCtx, state: &GameState, slot: u8, mask: u8) -> bool {
    let index = usize::from(slot);
    ctx.target_incarnation_matches(index, state)
        && crate::engine::effect_target_is_legal(
            state,
            ctx.source,
            ctx.controller,
            brutality_spec(mask),
            &ctx.targets,
            index,
        )
}

/// Runs one of this module's leaves in synchronous execution.
pub(crate) fn execute(op: &C1bOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        C1bOpV1::HymnV1 => discard_at_random(ctx, state, 2),
        C1bOpV1::MindTwistV1 => discard_at_random(ctx, state, usize::from(ctx.x_value)),
        C1bOpV1::BrutalityShrinkV1 { slot, mask } => {
            if slot_legal(ctx, state, *slot, *mask) {
                crate::effect::execute(
                    &EffectOp::PumpTargetUntilEndOfTurnDynamic {
                        target: TargetRef::Target(*slot),
                        power: DynamicValueDef::Fixed(-2),
                        toughness: DynamicValueDef::Fixed(-2),
                    },
                    ctx,
                    state,
                );
            }
        }
        C1bOpV1::BrutalityDrainV1 { slot, mask } => {
            if slot_legal(ctx, state, *slot, *mask) {
                let drain = EffectOp::Sequence(vec![
                    EffectOp::LoseLife {
                        player: PlayerRef::Target(*slot),
                        amount: 2,
                    },
                    EffectOp::GainLife {
                        player: PlayerRef::Controller,
                        amount: 2,
                    },
                ]);
                crate::effect::execute(&drain, ctx, state);
            }
        }
        C1bOpV1::PestInfestationV1 => pest_infestation_resolves(ctx, state),
        // Choices run in `drive`; the plain executor never reaches them.
        C1bOpV1::ChainSmogV1 | C1bOpV1::BrutalityRevealV1 { .. } => halt(state, ctx.source),
    }
}

/// The target player discards `count` cards at random, all at once (701.8).
/// A hand with `count` cards or fewer is discarded whole. The spell does
/// nothing when its only target is no longer a legal player (608.2b).
fn discard_at_random(ctx: &ExecCtx, state: &mut GameState, count: usize) {
    let Some(Target::Player(player)) = ctx.targets.first().copied() else {
        return;
    };
    if count == 0 {
        return;
    }
    match state.random_hand_cards_v1(player, count) {
        Ok(cards) => crate::engine::commit_discarded_cards(state, &cards),
        Err(_) => {
            state.engine.halted = Some((
                crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                ctx.source,
            ));
        }
    }
}

/// Pest Infestation: the target artifacts and enchantments that are still
/// the announced incarnations and still legal are destroyed as one batch,
/// then twice X Pest tokens are created whether or not anything was
/// destroyed.
fn pest_infestation_resolves(ctx: &ExecCtx, state: &mut GameState) {
    let spec = TargetSpec::StandardV1(StandardTargetV1::C1bV1(
        targets::C1bTargetV1::ArtifactsAndEnchantmentsV1,
    ));
    let mut victims = Vec::new();
    for (index, target) in ctx.targets.iter().enumerate() {
        let Target::Object(object) = *target else {
            continue;
        };
        if !victims.contains(&object)
            && ctx.target_incarnation_matches(index, state)
            && crate::engine::effect_target_is_legal(
                state,
                ctx.source,
                ctx.controller,
                spec,
                &ctx.targets,
                index,
            )
            && !crate::engine::has_effective_keyword(state, object, Keywords::INDESTRUCTIBLE)
        {
            victims.push(object);
        }
    }
    let destroyed = crate::effect::without_regenerated_v1(state, victims);
    if !destroyed.is_empty() {
        crate::event::propose_and_commit_batch(
            state,
            destroyed
                .into_iter()
                .map(|object| crate::event::ProposedEvent::zone_change(object, Zone::Graveyard))
                .collect(),
        );
    }
    let Some(token_def) = crate::card_def::card_id_by_name(PEST_TOKEN) else {
        halt(state, ctx.source);
        return;
    };
    for _ in 0..ctx.x_value.saturating_mul(2) {
        crate::effect::execute(
            &EffectOp::CreateToken {
                token_def,
                controller: PlayerRef::Controller,
            },
            ctx,
            state,
        );
    }
}

/// Offers `kind` to `player` over `candidates`.
#[allow(clippy::too_many_arguments)]
fn offer(
    continuation: &mut EffectContinuation,
    kind: C1bChoiceV1,
    player: PlayerId,
    candidates: Vec<Target>,
    bindings: Vec<EffectObjectBinding>,
    min: u16,
    max: u16,
    path: Vec<u16>,
) {
    let remaining = continuation.frames.clone();
    crate::eternal_cards_v1::e3::offer(
        continuation,
        E3ChoiceV1 {
            kind: E3ChoiceKindV1::C1bV1(kind),
            player,
            candidates,
            bindings,
            min,
            max,
            ordered: false,
            path,
            remaining,
        },
    );
}

/// "That player may copy this spell and may choose a new target for that
/// copy": offers `player` the players as the copy's new target, where
/// choosing none declines the copy. `false` when no target is legal.
fn offer_chain_copy(
    state: &GameState,
    continuation: &mut EffectContinuation,
    player: PlayerId,
    path: Vec<u16>,
) -> bool {
    let candidates = crate::engine::legal_spell_targets_for_v1(
        state,
        TargetSpec::AnyPlayer,
        player,
        continuation.ctx.source,
    );
    if candidates.is_empty() {
        return false;
    }
    offer(
        continuation,
        C1bChoiceV1::ChainSmogCopyTargetV1,
        player,
        candidates,
        Vec::new(),
        0,
        1,
        path,
    );
    true
}

/// Runs one of this module's leaves in the resumable interpreter.
pub(crate) fn drive(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    op: &C1bOpV1,
    path: Vec<u16>,
) -> Result<bool, String> {
    match op {
        C1bOpV1::ChainSmogV1 => {
            let ctx = continuation.ctx.clone();
            let Some(Target::Player(player)) = ctx.targets.first().copied() else {
                return Ok(false);
            };
            if !ctx.target_incarnation_matches(0, state) {
                return Ok(false);
            }
            let hand = state.players[player.index()].hand.clone();
            if hand.len() > 2 {
                // "Target player discards two cards": their choice.
                let bindings = hand
                    .iter()
                    .map(|&card| bind(state, card))
                    .collect::<Vec<_>>();
                offer(
                    continuation,
                    C1bChoiceV1::ChainSmogDiscardV1,
                    player,
                    hand.into_iter().map(Target::Object).collect(),
                    bindings,
                    2,
                    2,
                    path,
                );
                return Ok(true);
            }
            if !hand.is_empty() {
                crate::engine::commit_discarded_cards(state, &hand);
            }
            Ok(offer_chain_copy(state, continuation, player, path))
        }
        C1bOpV1::BrutalityRevealV1 { slot, mask } => {
            let ctx = continuation.ctx.clone();
            let Some(&Target::Player(opponent)) = ctx.targets.get(usize::from(*slot)) else {
                return Ok(false);
            };
            if !slot_legal(&ctx, state, *slot, *mask) {
                return Ok(false);
            }
            // "Target opponent reveals their hand."
            let hand = state.players[opponent.index()].hand.clone();
            for &card in &hand {
                // spellbench-public-reveal: hand card revealed by the card rules.
                state.reveal_distinct_hand_card(opponent.opponent(), opponent, card)?;
            }
            let cards = hand
                .into_iter()
                .filter(|&card| {
                    definition(state, card).is_some_and(|def| {
                        def.has_type(CardType::Instant) || def.has_type(CardType::Sorcery)
                    })
                })
                .collect::<Vec<_>>();
            if cards.is_empty() {
                return Ok(false);
            }
            let bindings = cards
                .iter()
                .map(|&card| bind(state, card))
                .collect::<Vec<_>>();
            offer(
                continuation,
                C1bChoiceV1::BrutalityDiscardV1,
                ctx.controller,
                cards.into_iter().map(Target::Object).collect(),
                bindings,
                1,
                1,
                path,
            );
            Ok(true)
        }
        _ => {
            execute(op, &continuation.ctx, state);
            Ok(false)
        }
    }
}

/// Applies an answered choice of this module. `Ok(true)` when it staged a
/// further choice.
pub(crate) fn finish_selection(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    choice: &E3ChoiceV1,
    kind: &C1bChoiceV1,
    bindings: Vec<EffectObjectBinding>,
    selected: &[Target],
) -> Result<bool, String> {
    match kind {
        C1bChoiceV1::ChainSmogDiscardV1 => {
            let cards = bindings
                .iter()
                .map(|binding| binding.object)
                .filter(|&card| state.objects.get(card).zone == Zone::Hand)
                .collect::<Vec<_>>();
            crate::engine::commit_discarded_cards(state, &cards);
            Ok(offer_chain_copy(
                state,
                continuation,
                choice.player,
                choice.path.clone(),
            ))
        }
        C1bChoiceV1::ChainSmogCopyTargetV1 => {
            let Some(&Target::Player(target)) = selected.first() else {
                return Ok(false);
            };
            crate::engine::copy_resolving_spell_for_v1(
                state,
                continuation.ctx.source,
                choice.player,
                Target::Player(target),
                TargetSpec::AnyPlayer,
            )?;
            Ok(false)
        }
        C1bChoiceV1::BrutalityDiscardV1 => {
            let Some(card) = bindings.first().copied() else {
                return Err("Collective Brutality chooses a card".to_string());
            };
            if state.objects.get(card.object).zone == Zone::Hand {
                crate::engine::commit_discarded_card(state, card.object);
            }
            Ok(false)
        }
    }
}
