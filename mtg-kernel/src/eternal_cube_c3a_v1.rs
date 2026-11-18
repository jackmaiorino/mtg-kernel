//! Card programs for the eternal catalog's Vintage Cube wave C3a
//! (`docs/design/eternal_formats_support_v1.md`): cheap creatures with existing analogs.
//!
//! `eternal_cards_v1` forwards this module's effect leaves here, and the
//! recipe tables of `build_eternal_c3a_v1.rs` name its programs. Every
//! table is keyed by a printed name that occurs only in the eternal registry,
//! so the Pauper, FDN and Standard catalogs never reach it. Behavior was read
//! from each card's current Oracle text and, where present, its XMage source.
//!
//! Cards that reuse existing recipes only (their rows are in
//! `build_eternal_c3a_v1.rs` or another wave's table):
//!
//! - Auriok Salvagers: Goblin Engineer's graveyard filter with the bound 1
//!   and the plain return to hand;
//! - Bloodbraid Challenger: Boarding Party's cascade (`eternal_e9b_v1`),
//!   haste, and Uro's escape shape with three cards exiled;
//! - Death-Greeter's Champion: Ragavan's dash (wave E7b recognizes the card
//!   by name when it is cast for its dash cost) and Saiba Cryptomancer's
//!   backup effect granting double strike;
//! - Embereth Shieldbreaker: a plain 2/1 creature with a sorcery Adventure
//!   destroying target artifact;
//! - Formidable Speaker's untap ability: Voltaic Key's untap effect with the
//!   "another permanent" filter of wave E8a.
//!
//! New in this module: Iron-Shield Elf's indestructible and tap, Giver of
//! Runes' protection from colorless (one more record of wave E9a's
//! protection list, read by wave E6b's protection checks), Harvester of
//! Misery's enters trigger, Formidable Speaker's discard-then-search enters
//! trigger and Devoted Druid's -1/-1 counter cost.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

use crate::card_def::{CardType, Keywords, TargetSpec, CARD_DEFS};
use crate::effect::{
    EffectContinuation, EffectFrame, EffectObjectBinding, EffectOp, ExecCtx, LibraryCardFilter,
    ObjectRef, PlayerRef,
};
use crate::eternal_cards_v1::e3::{E3ChoiceKindV1, E3ChoiceV1};
use crate::eternal_cards_v1::EternalOpV1;
use crate::ids::{ObjectId, PlayerId};
use crate::mana::ManaColor;
use crate::standard_cards_v1::StandardOpV1;
use crate::state::{GameState, ObjectLinkV4, Target, Zone};
use crate::trigger::{TriggerCondition, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};

const BLOODBRAID_CHALLENGER: &str = "Bloodbraid Challenger";
const DEATH_GREETERS_CHAMPION: &str = "Death-Greeter's Champion";
const DEVOTED_DRUID: &str = "Devoted Druid";
const FORMIDABLE_SPEAKER: &str = "Formidable Speaker";
const HARVESTER_OF_MISERY: &str = "Harvester of Misery";
const LUMINARCH_ASPIRANT: &str = "Luminarch Aspirant";

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::C3aV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum C3aOpV1 {
    /// Iron-Shield Elf: this creature gains indestructible until end of
    /// turn, then taps.
    IronShieldV1,
    /// Giver of Runes: target 0 gains protection from colorless until end
    /// of turn (the other options are wave E9a's protection from a color).
    GiverColorlessV1,
    /// Harvester of Misery's enters trigger: each other creature gets -2/-2
    /// until end of turn.
    HarvesterEntersV1,
    /// Formidable Speaker's enters trigger: its controller may discard a
    /// card, and if they do, searches for a creature card. A choice.
    SpeakerEntersV1,
}

impl C3aOpV1 {
    /// Whether the leaf may stop for a player's choice (or a draw that a
    /// replacement can interrupt), so it runs in the resumable interpreter.
    pub(crate) fn contains_player_choice(&self) -> bool {
        matches!(self, Self::SpeakerEntersV1)
    }
}

/// A choice of this module, carried as `E3ChoiceKindV1::C3aV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum C3aChoiceV1 {
    /// Formidable Speaker: up to one card of the controller's hand to
    /// discard.
    SpeakerDiscardV1,
}

impl C3aChoiceV1 {
    /// Whether the candidates are hidden from the other player.
    pub(crate) fn chooser_private(&self) -> bool {
        true
    }

    pub(crate) fn public_purpose(&self) -> crate::rl::TargetSelectionPurposeV4 {
        crate::rl::TargetSelectionPurposeV4::CardSelection
    }

    /// Object incarnations this choice refers to beyond its candidates.
    pub(crate) fn referenced_bindings(&self) -> Vec<EffectObjectBinding> {
        Vec::new()
    }
}

fn c3a(op: C3aOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::C3aV1(op)))
}

fn halt(state: &mut GameState, source: ObjectId) {
    state.engine.halted = Some((
        crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
        source,
    ));
}

// ---- Programs -----------------------------------------------------------------------

/// Iron-Shield Elf: "This creature gains indestructible until end of turn.
/// Tap it." Tapping is part of the effect, not a cost.
pub fn iron_shield_elf() -> EffectOp {
    c3a(C3aOpV1::IronShieldV1)
}

/// Giver of Runes: "Another target creature you control gains protection
/// from colorless or from the color of your choice until end of turn." One
/// option per color in WUBRG order (Mother of Runes' options, wave E9a's
/// protection record), then colorless.
pub fn giver_of_runes() -> EffectOp {
    let mut options = crate::eternal_legacy_e9a_v1::statics::COLORS
        .into_iter()
        .map(|color| {
            EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::E9aV1(
                crate::eternal_legacy_e9a_v1::E9aOpV1::ProtectionFromV1 { color },
            )))
        })
        .collect::<Vec<_>>();
    options.push(c3a(C3aOpV1::GiverColorlessV1));
    EffectOp::Choice {
        controller: PlayerRef::Controller,
        options,
    }
}

// ---- Helpers ------------------------------------------------------------------------

/// Target `index`, if it is an object still legal for the resolving ability
/// under `spec` (608.2b).
fn legal_target(
    ctx: &ExecCtx,
    state: &GameState,
    spec: TargetSpec,
    index: usize,
) -> Option<ObjectId> {
    let Target::Object(object) = ctx.targets.get(index).copied()? else {
        return None;
    };
    (ctx.target_incarnation_matches(index, state)
        && state.objects.get(object).zone == Zone::Battlefield
        && crate::engine::effect_target_is_legal(
            state,
            ctx.source,
            ctx.controller,
            spec,
            &ctx.targets[index..=index],
            0,
        ))
    .then_some(object)
}

/// The resolving ability's source, if it is still the incarnation that
/// created the ability, on the battlefield.
fn live_ability_source(ctx: &ExecCtx, state: &GameState) -> Option<ObjectId> {
    let contract = ctx.ability_source_contract?;
    let live = state.objects.try_get(contract.source)?;
    (live.zone == Zone::Battlefield && live.zone_change_count == contract.zone_change_count)
        .then_some(contract.source)
}

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

fn push(continuation: &mut EffectContinuation, op: EffectOp, path: &[u16]) {
    continuation.frames.push(EffectFrame::Program {
        op,
        path: path.to_vec(),
    });
}

/// Offers `player` the choice of `kind` among `bindings`, `min` to `max`
/// cards.
fn offer(
    continuation: &mut EffectContinuation,
    kind: C3aChoiceV1,
    player: PlayerId,
    bindings: Vec<EffectObjectBinding>,
    min: u16,
    max: u16,
    path: Vec<u16>,
) {
    let remaining = continuation.frames.clone();
    crate::eternal_cards_v1::e3::offer(
        continuation,
        E3ChoiceV1 {
            kind: E3ChoiceKindV1::C3aV1(kind),
            player,
            candidates: bindings
                .iter()
                .map(|binding| Target::Object(binding.object))
                .collect(),
            bindings,
            min,
            max,
            ordered: false,
            path,
            remaining,
        },
    );
}

/// Puts one -1/-1 counter on `object` as a cost (122.1, 614.16): the amount
/// always goes through the shared placement replacements, so one that names
/// any counter being put (Innkeeper's Talent) applies, while Doubling
/// Season's "an effect" leaves a cost alone (`counter_amount`).
fn put_minus_one_counter_as_cost(state: &mut GameState, object: ObjectId, actor: PlayerId) {
    let placed = match crate::doubling_season_v48::counter_amount(
        state,
        object,
        actor,
        crate::doubling_season_v48::CounterOriginV48::Cost,
        1,
    )
    .ok()
    .and_then(|count| i16::try_from(count).ok())
    {
        Some(count) => count,
        None => {
            halt(state, object);
            return;
        }
    };
    let Some(next) = state
        .objects
        .get(object)
        .counters
        .minus1_minus1
        .checked_add(placed)
    else {
        halt(state, object);
        return;
    };
    state.objects.get_mut(object).counters.minus1_minus1 = next;
}

/// Records that `creature` has protection from colorless until end of turn:
/// wave E9a's protection record (Sejiri Steppe's) with the colorless
/// "color", which wave E6b's protection checks read as protection from
/// colorless sources.
fn grant_colorless_protection(state: &mut GameState, creature: ObjectId) {
    let link = ObjectLinkV4 {
        object: creature,
        zone_change_count: state.objects.get(creature).zone_change_count,
    };
    let record = (
        link,
        ManaColor::C,
        crate::eternal_legacy_e9a_v1::TurnV1::now(state),
    );
    let protections = &mut crate::eternal_legacy_e9a_v1::e9a_state(state).protections;
    if !protections.contains(&record) {
        protections.push(record);
    }
}

// ---- Activation costs -----------------------------------------------------------------

/// Called as an activated ability's costs have been paid: Devoted Druid's
/// "Put a -1/-1 counter on this creature" is paid here, as the ability goes
/// on the stack. The counter stays when the ability is countered, and when
/// it kills the Druid the ability still resolves (with nothing to untap).
/// When Agatha's Soul Cauldron grants the ability, the creature that
/// activated it (`source`) pays the counter: the donor card and its local
/// ability index identify the ability.
pub(crate) fn activation_paid(
    state: &mut GameState,
    source: ObjectId,
    ability_index: u8,
    cauldron: Option<crate::cauldron_grants_v1::CauldronGrantV1>,
) {
    let Some(live) = state.objects.try_get(source) else {
        return;
    };
    if live.zone != Zone::Battlefield {
        return;
    }
    let (card_def, index) = match cauldron {
        Some(grant) => (grant.donor.card_def, grant.local_index),
        None => (live.card_def, u16::from(ability_index)),
    };
    if index != 0
        || CARD_DEFS
            .get(usize::from(card_def))
            .is_none_or(|def| def.name != DEVOTED_DRUID)
    {
        return;
    }
    let controller = live.controller;
    put_minus_one_counter_as_cost(state, source, controller);
}

// ---- Colorless protection ---------------------------------------------------------------

/// Giver of Runes' protection from colorless, as a test the E6b protection
/// checks ask: whether `host` has it until end of turn.
pub(crate) fn has_colorless_protection(state: &GameState, host: ObjectId) -> bool {
    // Free when no Giver of Runes has used its ability.
    let Some(e9a) = crate::eternal_legacy_e9a_v1::read_state(state) else {
        return false;
    };
    if e9a.protections.is_empty() {
        return false;
    }
    let Some(live) = state.objects.try_get(host) else {
        return false;
    };
    let now = crate::eternal_legacy_e9a_v1::TurnV1::now(state);
    e9a.protections.iter().any(|(link, color, turn)| {
        *color == ManaColor::C
            && link.object == host
            && link.zone_change_count == live.zone_change_count
            && *turn == now
    })
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

/// Luminarch Aspirant: "At the beginning of combat on your turn, put a +1/+1
/// counter on target creature you control."
fn aspirant_counter() -> EffectOp {
    EffectOp::PutPlusOnePlusOneCounter {
        object: ObjectRef::Target(0),
    }
}

/// Death-Greeter's Champion's "Backup 1": a +1/+1 counter on target creature
/// and, if it is another creature, double strike until end of turn (Saiba
/// Cryptomancer's effect with the keyword).
fn death_greeters_backup() -> EffectOp {
    EffectOp::BackupTarget {
        target: ObjectRef::Target(0),
        keyword: Keywords::DOUBLE_STRIKE,
    }
}

fn harvester_enters() -> EffectOp {
    c3a(C3aOpV1::HarvesterEntersV1)
}

fn speaker_enters() -> EffectOp {
    c3a(C3aOpV1::SpeakerEntersV1)
}

const ASPIRANT_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(
    TriggerCondition::BeginningOfControllerCombat,
    aspirant_counter,
)];
const DEATH_GREETERS_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, death_greeters_backup)];
const HARVESTER_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, harvester_enters)];
const SPEAKER_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(TriggerCondition::Etb, speaker_enters)];

/// Triggered abilities of this module's cards, falling through to wave
/// C3b. Bloodbraid Challenger's cascade is Boarding Party's.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        LUMINARCH_ASPIRANT => &ASPIRANT_TRIGGERS,
        DEATH_GREETERS_CHAMPION => &DEATH_GREETERS_TRIGGERS,
        HARVESTER_OF_MISERY => &HARVESTER_TRIGGERS,
        FORMIDABLE_SPEAKER => &SPEAKER_TRIGGERS,
        BLOODBRAID_CHALLENGER => crate::eternal_e9b_v1::triggers_for("Boarding Party"),
        _ => crate::eternal_cube_c3b_v1::triggers_for(name),
    }
}

/// Target specs of this module's printed and queued triggered abilities.
pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    match name {
        LUMINARCH_ASPIRANT if *effect == aspirant_counter() => Some(TargetSpec::ControlledCreature),
        DEATH_GREETERS_CHAMPION if *effect == death_greeters_backup() => Some(TargetSpec::Creature),
        HARVESTER_OF_MISERY if *effect == harvester_enters() => Some(TargetSpec::None),
        FORMIDABLE_SPEAKER if *effect == speaker_enters() => Some(TargetSpec::None),
        _ => delayed_effect_matches(name, effect).then_some(TargetSpec::None),
    }
}

/// Queued triggers owned by this module's cards: a dashed Death-Greeter's
/// Champion's return to its owner's hand at the next end step (wave E7b's
/// queued return).
pub(crate) fn delayed_effect_matches(name: &str, effect: &EffectOp) -> bool {
    name == DEATH_GREETERS_CHAMPION
        && matches!(
            effect,
            EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::E7bV1(
                crate::eternal_vintage_e7b_v1::E7bOpV1::EndStepReturnV1 { .. }
            )))
        )
}

// ---- Execution ----------------------------------------------------------------------

/// Runs one of this module's leaves in synchronous execution.
pub(crate) fn execute(op: &C3aOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        C3aOpV1::IronShieldV1 => {
            // The source must still be the incarnation that was activated.
            if live_ability_source(ctx, state).is_none() {
                return;
            }
            crate::effect::execute(
                &EffectOp::GrantKeywordTargetUntilEndOfTurn {
                    object: ObjectRef::ThisSource,
                    keyword: Keywords::INDESTRUCTIBLE,
                },
                ctx,
                state,
            );
            crate::effect::execute(
                &EffectOp::TapObject {
                    object: ObjectRef::ThisSource,
                },
                ctx,
                state,
            );
        }
        C3aOpV1::GiverColorlessV1 => {
            if let Some(creature) =
                legal_target(ctx, state, TargetSpec::AnotherControlledCreature, 0)
            {
                grant_colorless_protection(state, creature);
            }
        }
        C3aOpV1::HarvesterEntersV1 => {
            // "Other creatures": every creature on the battlefield, either
            // player's, but not the incarnation that entered. The set is
            // fixed as the trigger resolves (611.2c).
            let creatures = [PlayerId::P0, PlayerId::P1]
                .into_iter()
                .flat_map(|player| state.players[player.index()].battlefield.iter().copied())
                .filter(|&object| crate::engine::object_has_type(state, object, CardType::Creature))
                .filter(|&object| {
                    !ctx.ability_source_contract.is_some_and(|source| {
                        source.source == object
                            && source.zone_change_count
                                == state.objects.get(object).zone_change_count
                    })
                })
                .map(|object| bind(state, object))
                .collect::<Vec<_>>();
            for creature in creatures {
                crate::effect::install_temporary_boost(state, creature, -2, -2, Keywords::NONE);
            }
        }
        // Choices run in `drive`.
        C3aOpV1::SpeakerEntersV1 => halt(state, ctx.source),
    }
}

/// Runs one of this module's leaves in the resumable interpreter.
pub(crate) fn drive(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    op: &C3aOpV1,
    path: Vec<u16>,
) -> Result<bool, String> {
    match op {
        C3aOpV1::SpeakerEntersV1 => {
            // "You may discard a card": with an empty hand there is nothing
            // to discard and so nothing to search for.
            let controller = continuation.ctx.controller;
            let hand = state.players[controller.index()]
                .hand
                .iter()
                .map(|&card| bind(state, card))
                .collect::<Vec<_>>();
            if hand.is_empty() {
                return Ok(false);
            }
            offer(
                continuation,
                C3aChoiceV1::SpeakerDiscardV1,
                controller,
                hand,
                0,
                1,
                path,
            );
            Ok(true)
        }
        other => {
            execute(other, &continuation.ctx, state);
            Ok(false)
        }
    }
}

/// Applies an answered choice of this module.
pub(crate) fn finish_selection(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    choice: &E3ChoiceV1,
    kind: &C3aChoiceV1,
    bindings: Vec<EffectObjectBinding>,
) -> Result<bool, String> {
    match kind {
        C3aChoiceV1::SpeakerDiscardV1 => {
            let cards = bindings
                .iter()
                .copied()
                .filter(|&card| binding_live(state, card))
                .map(|card| card.object)
                .collect::<Vec<_>>();
            if cards.is_empty() {
                return Ok(false);
            }
            crate::engine::commit_discarded_cards(state, &cards);
            if state.engine.halted.is_some() {
                return Ok(false);
            }
            // "If you do, search your library for a creature card, reveal
            // it, put it into your hand, then shuffle."
            push(
                continuation,
                EffectOp::SearchLibraryToHand {
                    player: PlayerRef::Controller,
                    filter: LibraryCardFilter::CardWithTypeV1(CardType::Creature),
                },
                &choice.path,
            );
            Ok(false)
        }
    }
}
