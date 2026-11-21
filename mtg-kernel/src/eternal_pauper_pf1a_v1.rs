//! Card programs for the eternal catalog's Pauper field wave Pf1a
//! (`docs/design/eternal_formats_support_v1.md`): permanents with simple text.
//!
//! `eternal_cards_v1` forwards this module's effect leaves here, and the
//! recipe tables of `build_eternal_pf1a_v1.rs` name its programs. Every
//! table is keyed by a printed name that occurs only in the eternal registry,
//! so the Pauper, FDN and Standard catalogs never reach it. Behavior was read
//! from each card's current Oracle text and, where present, its XMage source.
//!
//! Cards: Wooded Ridgeline and Kabira Crossroads (lands), Jaspera Sentinel,
//! Kor Sanctifiers, Vulshok Sorcerer, Primordial Pachyderm and K'un-Lun
//! Warrior (creatures), Lifelink and Mask of Law and Grace (Auras) and
//! Mycosynth Wellspring (artifact). Most are compositions of existing
//! effects; the new rules here are Mask of Law and Grace's fixed two-color
//! protection (read through wave E9a's protection mask) and K'un-Lun
//! Warrior's "sacrifice an artifact or discard a card" choice.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

use crate::card_def::{CardType, TargetSpec, CARD_DEFS};
use crate::effect::{
    EffectCond, EffectContinuation, EffectFrame, EffectObjectBinding, EffectOp, ExecCtx,
    LibraryCardFilter, ObjectRef, PlayerRef,
};
use crate::eternal_cards_v1::e3::{E3ChoiceKindV1, E3ChoiceV1};
use crate::eternal_cards_v1::EternalOpV1;
use crate::event::ProposedEvent;
use crate::ids::ObjectId;
use crate::mana::ManaColor;
use crate::standard_cards_v1::StandardOpV1;
use crate::state::{GameState, ObjectLinkV4, Target, Zone};
use crate::trigger::{TriggerCondition, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};

const KABIRA_CROSSROADS: &str = "Kabira Crossroads";
const KOR_SANCTIFIERS: &str = "Kor Sanctifiers";
const PRIMORDIAL_PACHYDERM: &str = "Primordial Pachyderm";
const MYCOSYNTH_WELLSPRING: &str = "Mycosynth Wellspring";
const KUN_LUN_WARRIOR: &str = "K'un-Lun Warrior";
const MASK_OF_LAW_AND_GRACE: &str = "Mask of Law and Grace";

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::Pf1aV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Pf1aOpV1 {
    /// K'un-Lun Warrior: "you may sacrifice an artifact or discard a card.
    /// If you do, draw a card."
    KunLunV1,
}

impl Pf1aOpV1 {
    /// Whether the leaf may stop for a player's choice (or a draw that a
    /// replacement can interrupt), so it runs in the resumable interpreter.
    pub(crate) fn contains_player_choice(&self) -> bool {
        match self {
            Self::KunLunV1 => true,
        }
    }
}

/// Resolution choices owned by this module, carried as
/// `E3ChoiceKindV1::Pf1aV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Pf1aChoiceV1 {
    /// K'un-Lun Warrior: up to one artifact the controller controls (to
    /// sacrifice) or card in the controller's hand (to discard).
    KunLunV1,
}

impl Pf1aChoiceV1 {
    /// The menu includes cards in the chooser's hand.
    pub(crate) fn chooser_private(&self) -> bool {
        match self {
            Self::KunLunV1 => true,
        }
    }

    pub(crate) fn public_purpose(&self) -> crate::rl::TargetSelectionPurposeV4 {
        match self {
            Self::KunLunV1 => crate::rl::TargetSelectionPurposeV4::CardSelection,
        }
    }
}

fn pf1a(op: Pf1aOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::Pf1aV1(op)))
}

fn halt(state: &mut GameState, source: ObjectId) {
    state.engine.halted = Some((
        crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
        source,
    ));
}

// ---- Programs -------------------------------------------------------------------------

/// Kabira Crossroads and Primordial Pachyderm: "you gain 2 life."
fn gain_two() -> EffectOp {
    EffectOp::GainLife {
        player: PlayerRef::Controller,
        amount: 2,
    }
}

/// Kor Sanctifiers: "destroy target artifact or enchantment." The target is
/// chosen when the trigger is put on the stack (its kicked condition is the
/// trigger's intervening "if"); the destruction is Meteor Golem's shape, which
/// checks that the target is still on the battlefield.
fn destroy_target_artifact_or_enchantment() -> EffectOp {
    EffectOp::Conditional {
        cond: EffectCond::TargetInZone(0, Zone::Battlefield),
        then: Box::new(EffectOp::DestroyObject {
            object: ObjectRef::Target(0),
        }),
        else_: Box::new(EffectOp::Sequence(vec![])),
    }
}

/// Mycosynth Wellspring: "you may search your library for a basic land card,
/// reveal it, put it into your hand, then shuffle." Krosan Tusker's search.
fn may_search_basic_land() -> EffectOp {
    crate::eternal_cards_v1::may_search(
        PlayerRef::Controller,
        EffectOp::SearchLibraryToHand {
            player: PlayerRef::Controller,
            filter: LibraryCardFilter::BasicLand,
        },
    )
}

/// K'un-Lun Warrior: "you may sacrifice an artifact or discard a card. If you
/// do, draw a card."
fn kun_lun_choice() -> EffectOp {
    pf1a(Pf1aOpV1::KunLunV1)
}

// ---- Triggered abilities ------------------------------------------------------------

const fn trigger(effect: fn() -> EffectOp) -> TriggeredAbilityDef {
    TriggeredAbilityDef {
        condition: TriggerCondition::Etb,
        home_zone: Zone::Battlefield,
        intervening_if_kicked: false,
        intervening_if_controls_another_source_card: false,
        face_index: 0,
        effect,
    }
}

const GAIN_TWO_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(gain_two)];
const KOR_SANCTIFIERS_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    intervening_if_kicked: true,
    ..trigger(destroy_target_artifact_or_enchantment)
}];
const WELLSPRING_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(may_search_basic_land),
    TriggeredAbilityDef {
        condition: TriggerCondition::LeftBattlefieldToGraveyard,
        home_zone: Zone::Graveyard,
        ..trigger(may_search_basic_land)
    },
];
const KUN_LUN_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(kun_lun_choice)];

/// This module's own triggered abilities.
fn own_triggers(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        KABIRA_CROSSROADS | PRIMORDIAL_PACHYDERM => &GAIN_TWO_TRIGGERS,
        KOR_SANCTIFIERS => &KOR_SANCTIFIERS_TRIGGERS,
        MYCOSYNTH_WELLSPRING => &WELLSPRING_TRIGGERS,
        KUN_LUN_WARRIOR => &KUN_LUN_TRIGGERS,
        _ => &[],
    }
}

/// Triggered abilities of this module's cards, falling through to wave
/// Pf1b.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match own_triggers(name) {
        [] => crate::eternal_pauper_pf1b_v1::triggers_for(name),
        triggers => triggers,
    }
}

/// Target specs of this module's printed and queued triggered abilities.
pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    let own = own_triggers(name)
        .iter()
        .any(|trigger| (trigger.effect)() == *effect)
        || delayed_effect_matches(name, effect);
    if !own {
        return None;
    }
    Some(match name {
        KOR_SANCTIFIERS => TargetSpec::ArtifactOrEnchantmentPermanent,
        _ => TargetSpec::None,
    })
}

/// Queued triggers owned by this module's cards.
pub(crate) fn delayed_effect_matches(_name: &str, _effect: &EffectOp) -> bool {
    false
}

// ---- Statics: Mask of Law and Grace --------------------------------------------------

/// Mask of Law and Grace: "Enchanted creature has protection from black and
/// from red." The color mask of the Masks attached to `host` whose grant still
/// applies (a later ability removal on the host takes it away, 613.7). Wave
/// E9a's mask chain includes it, so the engine's targeting, blocking, damage
/// and attachment checks and the state-based removal of black and red Auras
/// and Equipment apply it.
pub(crate) fn protection_mask(state: &GameState, host: ObjectId) -> u8 {
    let Some(live) = state.objects.try_get(host) else {
        return 0;
    };
    if live.zone != Zone::Battlefield || live.attachments.is_empty() {
        return 0;
    }
    let link = ObjectLinkV4 {
        object: host,
        zone_change_count: live.zone_change_count,
    };
    let mut mask = 0;
    for &aura in &live.attachments {
        let Some(mask_live) = state.objects.try_get(aura) else {
            continue;
        };
        if mask_live.zone == Zone::Battlefield
            && mask_live.v4.face_index == 0
            && mask_live.v4.attached_to == Some(link)
            && CARD_DEFS
                .get(usize::from(mask_live.card_def))
                .is_some_and(|def| def.name == MASK_OF_LAW_AND_GRACE && def.is_executable())
            && crate::continuous_characteristics_v1::printed_abilities_active(state, aura)
            && crate::continuous_characteristics_v1::granted_ability_survives(
                state,
                host,
                mask_live.v4.layer_timestamp.unwrap_or(0),
            )
        {
            mask |= crate::card_def::mana_color_mask(ManaColor::B)
                | crate::card_def::mana_color_mask(ManaColor::R);
        }
    }
    mask
}

// ---- Execution ----------------------------------------------------------------------

/// Runs one of this module's leaves in synchronous execution.
pub(crate) fn execute(op: &Pf1aOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        // The choice runs in `drive`.
        Pf1aOpV1::KunLunV1 => halt(state, ctx.source),
    }
}

/// Runs one of this module's leaves in the resumable interpreter.
pub(crate) fn drive(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    op: &Pf1aOpV1,
    path: Vec<u16>,
) -> Result<bool, String> {
    match op {
        Pf1aOpV1::KunLunV1 => Ok(offer_kun_lun(state, continuation, path)),
    }
}

/// Whether `object` is an artifact `player` controls on the battlefield.
fn sacrificeable_artifact(
    state: &GameState,
    player: crate::ids::PlayerId,
    object: ObjectId,
) -> bool {
    state.objects.try_get(object).is_some_and(|live| {
        live.zone == Zone::Battlefield
            && live.controller == player
            && crate::engine::object_has_type(state, object, CardType::Artifact)
    })
}

/// Stages the choice: any artifact the controller controls or any card in the
/// controller's hand, or none (the controller may do neither). `false` when
/// there is nothing to choose from.
fn offer_kun_lun(state: &GameState, continuation: &mut EffectContinuation, path: Vec<u16>) -> bool {
    let player = continuation.ctx.controller;
    let mut objects: Vec<ObjectId> = [crate::ids::PlayerId::P0, crate::ids::PlayerId::P1]
        .into_iter()
        .flat_map(|seat| state.players[seat.index()].battlefield.iter().copied())
        .filter(|&object| sacrificeable_artifact(state, player, object))
        .collect();
    objects.extend(state.players[player.index()].hand.iter().copied());
    if objects.is_empty() {
        return false;
    }
    let bindings = objects
        .iter()
        .map(|&object| {
            let live = state.objects.get(object);
            EffectObjectBinding {
                object,
                expected_zone: live.zone,
                expected_zone_change_count: live.zone_change_count,
            }
        })
        .collect::<Vec<_>>();
    let choice = E3ChoiceV1 {
        kind: E3ChoiceKindV1::Pf1aV1(Pf1aChoiceV1::KunLunV1),
        player,
        candidates: objects
            .iter()
            .map(|&object| Target::Object(object))
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

/// Applies an answered choice of this module.
pub(crate) fn finish_selection(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    choice: &E3ChoiceV1,
    kind: &Pf1aChoiceV1,
    bindings: Vec<EffectObjectBinding>,
) -> Result<bool, String> {
    match kind {
        Pf1aChoiceV1::KunLunV1 => {
            let Some(card) = bindings.first() else {
                // Declined: neither done, so nothing is drawn.
                return Ok(false);
            };
            let player = choice.player;
            let done = match card.expected_zone {
                Zone::Hand => {
                    if state.objects.get(card.object).owner != player {
                        return Err("K'un-Lun Warrior's discard left its owner's hand".to_string());
                    }
                    // The genuine discard path (madness included).
                    crate::engine::commit_discarded_card(state, card.object);
                    true
                }
                Zone::Battlefield => {
                    if !sacrificeable_artifact(state, player, card.object) {
                        return Err(
                            "K'un-Lun Warrior's artifact is no longer sacrificable".to_string()
                        );
                    }
                    // A permanent that a replacement keeps on the battlefield
                    // has not been sacrificed (Gut's convention).
                    crate::event::log_sacrifice(state, card.object);
                    crate::event::propose_and_commit(
                        state,
                        ProposedEvent::zone_change(card.object, Zone::Graveyard),
                    );
                    state
                        .objects
                        .try_get(card.object)
                        .is_none_or(|live| live.zone != Zone::Battlefield)
                }
                _ => return Err("K'un-Lun Warrior's choice named another zone".to_string()),
            };
            if done {
                // "If you do, draw a card": the next instruction at this
                // leaf's path, where the draw replacements are offered as for
                // any draw (wave E7b's may-discard-then-draw shape).
                continuation.frames.push(EffectFrame::Program {
                    op: EffectOp::DrawCards {
                        player: PlayerRef::Controller,
                        count: 1,
                    },
                    path: choice.path.clone(),
                });
            }
            Ok(false)
        }
    }
}
