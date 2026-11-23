//! Card programs for the eternal catalog's Vintage Cube wave C9a
//! (`docs/design/eternal_formats_support_v1.md`): Aang, Swift Savior // Aang
//! and La, Ocean's Fury; Badgermole Cub; Batroc the Leaper; Blightsteel
//! Colossus; Crabomination; and Fear of Missing Out.
//!
//! `eternal_cards_v1` forwards this module's effect leaves, triggers, entry
//! counters, and turn bookkeeping here, and the recipe tables of
//! `build_eternal_c9a_v1.rs` name its programs. Every table is keyed by a
//! printed name that occurs only in the eternal registry, so the Pauper, FDN,
//! and Standard catalogs never reach it. Behavior was read from each card's
//! current Oracle text.
//!
//! Where each card lives:
//!
//! - **Aang, Swift Savior // Aang and La, Ocean's Fury**: front face has
//!   `Flash`, `Flying`, ETB `AangAirbendV1` targeting up to one other creature
//!   or spell (exiling spells off the stack without countering them; non-copy,
//!   non-token castable cards get a permanent `AsLongAsExiled` play permission
//!   for their owner casting them for `{2}` rather than their mana cost), and
//!   `Waterbend {8}: Transform Aang.` (9 instant-speed payments on face 0,
//!   reusing wave C8a's `transform_front_to_back` with waterbend cost/tap
//!   planning). Back face (`Aang and La, Ocean's Fury`, 5/5 colorless, `Reach`,
//!   `Trample`) triggers on attack (`AangBackAttackV1`, `face_index: 1`) to
//!   put a +1/+1 counter on each tapped creature its controller controls.
//! - **Badgermole Cub**: ETB `BadgermoleEarthbendV1` animates target land you
//!   control permanently as a 0/0 creature with haste that is still a land
//!   (layers 4, 6, 7b), puts a +1/+1 counter on it, and links a delayed trigger
//!   returning it to the battlefield tapped under its owner's control when it
//!   dies or is exiled (`before_departure` + `EarthbendReturnV1`). Triggered
//!   mana ability (`after_tapped_for_mana`) adds `{G}` whenever its controller
//!   taps a creature for mana.
//! - **Batroc the Leaper**: `Multikicker {2}` (`multikicker_cost`); enters
//!   with one +1/+1 counter per kick (`BatrocEnterV1` + `apply_entry_counters`
//!   respecting counter replacements); ETB (`BatrocEtbDamageV1`) deals damage
//!   equal to his power (or LKI if departed) simultaneously to each of up to X
//!   distinct any-targets (0 kicks emits no target prompt).
//! - **Blightsteel Colossus**: `Trample`, `Infect` (`Keywords::INFECT`,
//!   CR 702.90 in `event.rs` damage commit), `Indestructible`, and mandatory
//!   reveal-and-shuffle replacement from any zone to graveyard
//!   (`plan_blightsteel_shuffle` / `finish_blightsteel_shuffles`).
//! - **Crabomination**: `Emerge from artifact {5}{B}{B}` (alternative cast
//!   cost sacrificing a controlled artifact and reducing generic mana in
//!   `{5}{B}{B}` by the sacrificed artifact's mana value, minimum `{B}{B}`);
//!   ETB (`CrabominationEtbV1`) exiles target opponent's top library card, a
//!   random graveyard card, and a random hand card, then optionally casts one
//!   castable spell from among them without paying its mana cost during
//!   resolution (`C9aChoiceV1::CrabominationCastV1` + `resolution_cast_v1`).
//! - **Fear of Missing Out**: ETB discards a card then draws a card; Delirium
//!   first-attack-each-turn trigger (`observes_attacks` + `delayed_triggers` +
//!   `FomoAttackTriggerV1`) tracks first attack per incarnation even without
//!   Delirium, checks Delirium at trigger and resolution, untaps target
//!   creature, and queues an additional combat phase (`extra_combat_phases`,
//!   consumed at `Step::EndCombat` in `engine.rs`).
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

pub mod targets;

use crate::card_def::{CardType, CostComponent, Keywords, Subtype, TargetSpec, CARD_DEFS};
use crate::effect::{
    EffectContinuation, EffectObjectBinding, EffectOp, ExecCtx, ObjectRef, PlayerRef,
};
use crate::engine::{AnyManaTypeV1, FreeCastV1, PlayOrCast, PlayPermission, PlayPermissionExpiry};
use crate::eternal_cards_v1::e3::{E3ChoiceKindV1, E3ChoiceV1};
use crate::eternal_cards_v1::EternalOpV1;
use crate::eternal_legacy_e9a_v1::TurnV1;
use crate::event::{self, CommittedEvent, HistoricalDamageSourceV1, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::mana::{Cost, ManaColor};
use crate::standard_cards_v1::{StandardOpV1, StandardTargetV1};
use crate::state::{
    AbilitySourceContractV4, GameState, ObjectLinkV4, SpellCastRouteV4, Target, Zone,
};
use crate::trigger::{PendingTrigger, TriggerCondition, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};
use targets::C9aTargetV1;

pub(crate) const AANG_SWIFT_SAVIOR: &str = "Aang, Swift Savior";
pub(crate) const BADGERMOLE_CUB: &str = "Badgermole Cub";
pub(crate) const BATROC_THE_LEAPER: &str = "Batroc the Leaper";
pub(crate) const BLIGHTSTEEL_COLOSSUS: &str = "Blightsteel Colossus";
pub(crate) const CRABOMINATION: &str = "Crabomination";
pub(crate) const FEAR_OF_MISSING_OUT: &str = "Fear of Missing Out";

/// Generic mana in Aang, Swift Savior's printed "Waterbend {8}".
const AANG_WATERBEND_GENERIC: u8 = 8;

/// Alternative cast cost granted by airbend: `{2}` rather than its mana cost.
pub(crate) const AIRBEND_CAST_COST: Cost = Cost {
    pips: &[],
    generic: 2,
    x_count: 0,
};

pub(crate) const AANG_ETB_TARGET: TargetSpec = TargetSpec::StandardV1(StandardTargetV1::C9aV1(
    C9aTargetV1::UpToOneOtherCreatureOrSpellV1,
));

pub(crate) const BADGERMOLE_ETB_TARGET: TargetSpec =
    TargetSpec::StandardV1(StandardTargetV1::C9aV1(C9aTargetV1::ControlledLandV1));

// ---- State --------------------------------------------------------------------------

/// One permanent earthbend animation and its delayed return link.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EarthbentLandV1 {
    pub land: ObjectLinkV4,
    pub timestamp: u64,
    pub controller: PlayerId,
    pub source: AbilitySourceContractV4,
}

/// An earthbent land that just died or was exiled, waiting to queue its
/// delayed return trigger at the next trigger collection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PendingEarthbendReturnV1 {
    pub source: AbilitySourceContractV4,
    pub controller: PlayerId,
    pub land: ObjectId,
    pub expected_zone: Zone,
    pub expected_zcc: u32,
}

/// Last-known power and damage-source facts for a Batroc the Leaper that left
/// the battlefield while his ETB damage trigger could still reference him.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DepartedBatrocV1 {
    pub link: ObjectLinkV4,
    pub turn: TurnV1,
    pub power: i32,
    pub facts: HistoricalDamageSourceV1,
}

fn is_zero_u8(value: &u8) -> bool {
    *value == 0
}

/// Per-game state owned by wave C9a, kept inside
/// `eternal_cards_v1::EternalStateV1`. Absent from serialization and hashes to
/// zero bytes while empty so every game without these cards keeps its exact
/// snapshot and hash.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct C9aStateV1 {
    /// Cards exiled by airbend whose owner may cast them for `{2}` rather
    /// than their mana cost while they remain exiled at this incarnation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub airbent_exiled: Vec<ObjectLinkV4>,
    /// Lands animated by Badgermole Cub's earthbend (0/0 creature with haste,
    /// still a land, with delayed return when dying or exiled).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub earthbent_lands: Vec<EarthbentLandV1>,
    /// Earthbent lands that just left to graveyard or exile, queued for
    /// delayed trigger collection.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pending_earthbend_returns: Vec<PendingEarthbendReturnV1>,
    /// Batroc the Leaper spells entering from stack with their kick count,
    /// consumed by `apply_entry_counters`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pending_batroc_kicks: Vec<(ObjectLinkV4, u16)>,
    /// Battlefield incarnations of Batroc the Leaper and how many times they
    /// were kicked, read by his ETB trigger.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub batroc_kicks: Vec<(ObjectLinkV4, u16)>,
    /// LKI snapshots for Batroc the Leaper incarnations that departed while
    /// their ETB damage trigger was pending or on the stack.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub departed_batrocs: Vec<DepartedBatrocV1>,
    /// Fear of Missing Out incarnations that have already attacked this turn.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fomo_attacked_this_turn: Vec<(ObjectLinkV4, TurnV1)>,
    /// Additional combat phases queued for the current turn (Fear of Missing
    /// Out).
    #[serde(default, skip_serializing_if = "is_zero_u8")]
    pub extra_combat_phases: u8,
}

impl C9aStateV1 {
    pub fn is_empty(&self) -> bool {
        self.airbent_exiled.is_empty()
            && self.earthbent_lands.is_empty()
            && self.pending_earthbend_returns.is_empty()
            && self.pending_batroc_kicks.is_empty()
            && self.batroc_kicks.is_empty()
            && self.departed_batrocs.is_empty()
            && self.fomo_attacked_this_turn.is_empty()
            && self.extra_combat_phases == 0
    }
}

/// An empty state hashes to zero bytes so every game without these cards keeps
/// its exact state hash.
impl std::hash::Hash for C9aStateV1 {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        if !self.is_empty() {
            b"eternal-cube-c9a/v1".hash(state);
            self.airbent_exiled.hash(state);
            self.earthbent_lands.hash(state);
            self.pending_earthbend_returns.hash(state);
            self.pending_batroc_kicks.hash(state);
            self.batroc_kicks.hash(state);
            self.departed_batrocs.hash(state);
            self.fomo_attacked_this_turn.hash(state);
            self.extra_combat_phases.hash(state);
        }
    }
}

fn c9a_state(state: &mut GameState) -> &mut C9aStateV1 {
    &mut state.eternal_v1.get_or_insert_with(Default::default).c9a
}

fn read_state(state: &GameState) -> Option<&C9aStateV1> {
    state.eternal_v1.as_ref().map(|eternal| &eternal.c9a)
}

// ---- Ops and choices ----------------------------------------------------------------

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::C9aV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum C9aOpV1 {
    /// Aang, Swift Savior ETB: airbend up to one other target creature or
    /// spell (exile it; while exiled, its owner may cast it for `{2}`).
    AangAirbendV1,
    /// Aang and La, Ocean's Fury attack trigger: put a +1/+1 counter on each
    /// tapped creature you control.
    AangBackAttackV1,
    /// Badgermole Cub ETB: earthbend 1 target land you control.
    BadgermoleEarthbendV1,
    /// Earthbend delayed trigger: return `land` to the battlefield tapped
    /// under its owner's control if still in `expected_zone` at `expected_zcc`.
    EarthbendReturnV1 {
        land: ObjectId,
        expected_zone: Zone,
        expected_zcc: u32,
    },
    /// Batroc the Leaper spell resolution: record multikicker count and move
    /// to battlefield so entry counters apply as he enters.
    BatrocEnterV1,
    /// Batroc the Leaper ETB: deal damage equal to his power (or LKI) to each
    /// of up to `max_targets` targets simultaneously.
    BatrocEtbDamageV1 {
        source_link: ObjectLinkV4,
        max_targets: u8,
    },
    /// Crabomination ETB: target opponent exiles top of library, random
    /// graveyard card, and random hand card; optionally cast one spell from
    /// among them without paying its mana cost. A choice.
    CrabominationEtbV1,
    /// Fear of Missing Out Delirium attack trigger: untap target creature and
    /// add an additional combat phase after this phase.
    FomoAttackTriggerV1,
}

impl C9aOpV1 {
    /// Whether the leaf may stop for a player's choice (or a draw that a
    /// replacement can interrupt), so it runs in the resumable interpreter.
    pub(crate) fn contains_player_choice(&self) -> bool {
        matches!(self, Self::CrabominationEtbV1)
    }
}

/// Resolution choices owned by this module, carried as
/// `E3ChoiceKindV1::C9aV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum C9aChoiceV1 {
    /// Crabomination: optionally cast one spell from among the exiled cards
    /// without paying its mana cost during resolution.
    CrabominationCastV1,
}

impl C9aChoiceV1 {
    pub(crate) fn chooser_private(&self) -> bool {
        match self {
            Self::CrabominationCastV1 => false,
        }
    }

    pub(crate) fn public_purpose(&self) -> crate::rl::TargetSelectionPurposeV4 {
        match self {
            Self::CrabominationCastV1 => crate::rl::TargetSelectionPurposeV4::CardSelection,
        }
    }

    pub(crate) fn referenced_bindings(&self) -> Vec<EffectObjectBinding> {
        Vec::new()
    }
}

fn c9a(op: C9aOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::C9aV1(op)))
}

// ---- Card programs ------------------------------------------------------------------

/// Batroc the Leaper spell resolution.
pub fn batroc_enter() -> EffectOp {
    c9a(C9aOpV1::BatrocEnterV1)
}

fn aang_airbend() -> EffectOp {
    c9a(C9aOpV1::AangAirbendV1)
}

fn aang_back_attack() -> EffectOp {
    c9a(C9aOpV1::AangBackAttackV1)
}

fn badgermole_earthbend() -> EffectOp {
    c9a(C9aOpV1::BadgermoleEarthbendV1)
}

fn crabomination_etb() -> EffectOp {
    c9a(C9aOpV1::CrabominationEtbV1)
}

/// Fear of Missing Out ETB: "When this creature enters, discard a card, then
/// draw a card." Non-terminal `DiscardCards` runs in the resumable interpreter
/// and `DrawCards` uses native resumable draws (Dredge, Narset, replacements).
fn fomo_etb() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::DiscardCards {
            player: PlayerRef::Controller,
            count: 1,
        },
        EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 1,
        },
    ])
}

fn fomo_attack_trigger() -> EffectOp {
    c9a(C9aOpV1::FomoAttackTriggerV1)
}

// ---- Triggered abilities ------------------------------------------------------------

const fn trigger(
    condition: TriggerCondition,
    face_index: u8,
    effect: fn() -> EffectOp,
) -> TriggeredAbilityDef {
    TriggeredAbilityDef {
        condition,
        home_zone: Zone::Battlefield,
        intervening_if_kicked: false,
        intervening_if_controls_another_source_card: false,
        face_index,
        effect,
    }
}

const AANG_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(TriggerCondition::Etb, 0, aang_airbend),
    trigger(TriggerCondition::Attacks, 1, aang_back_attack),
];

const BADGERMOLE_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, 0, badgermole_earthbend)];

const CRABOMINATION_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, 0, crabomination_etb)];

const FOMO_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(TriggerCondition::Etb, 0, fomo_etb)];

/// Triggered abilities of this module's cards, falling through to wave C9b.
/// Batroc's ETB trigger (variable up-to-X targets), earthbend's delayed return
/// trigger, and Fear of Missing Out's Delirium first-attack trigger are queued
/// via `delayed_triggers`.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        AANG_SWIFT_SAVIOR => &AANG_TRIGGERS,
        BADGERMOLE_CUB => &BADGERMOLE_TRIGGERS,
        CRABOMINATION => &CRABOMINATION_TRIGGERS,
        FEAR_OF_MISSING_OUT => &FOMO_TRIGGERS,
        _ => crate::eternal_cube_c9b_v1::triggers_for(name),
    }
}

/// Target specs of this module's printed and queued triggered abilities.
pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    match name {
        AANG_SWIFT_SAVIOR if *effect == aang_airbend() => Some(AANG_ETB_TARGET),
        AANG_SWIFT_SAVIOR if *effect == aang_back_attack() => Some(TargetSpec::None),
        BADGERMOLE_CUB if *effect == badgermole_earthbend() => Some(BADGERMOLE_ETB_TARGET),
        BATROC_THE_LEAPER => match effect {
            EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::C9aV1(
                C9aOpV1::BatrocEtbDamageV1 { max_targets, .. },
            ))) => Some(if *max_targets == 0 {
                TargetSpec::None
            } else {
                TargetSpec::StandardV1(StandardTargetV1::C9aV1(
                    C9aTargetV1::BatrocUpToAnyTargetsV1(*max_targets),
                ))
            }),
            _ => None,
        },
        CRABOMINATION if *effect == crabomination_etb() => Some(TargetSpec::TargetOpponent),
        FEAR_OF_MISSING_OUT if *effect == fomo_etb() => Some(TargetSpec::None),
        FEAR_OF_MISSING_OUT if *effect == fomo_attack_trigger() => Some(TargetSpec::Creature),
        _ => delayed_effect_matches(name, effect).then_some(TargetSpec::None),
    }
}

/// Queued triggers owned by this module's cards.
pub(crate) fn delayed_effect_matches(name: &str, effect: &EffectOp) -> bool {
    (name == BADGERMOLE_CUB
        && matches!(
            effect,
            EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::C9aV1(
                C9aOpV1::EarthbendReturnV1 { .. }
            )))
        ))
        || (name == BATROC_THE_LEAPER
            && matches!(
                effect,
                EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::C9aV1(
                    C9aOpV1::BatrocEtbDamageV1 { .. }
                )))
            ))
        || (name == FEAR_OF_MISSING_OUT && *effect == fomo_attack_trigger())
}

// ---- Helpers ------------------------------------------------------------------------

fn definition_name(state: &GameState, object: ObjectId) -> Option<&'static str> {
    let live = state.objects.try_get(object)?;
    CARD_DEFS
        .get(usize::from(live.card_def))
        .map(|def| def.name)
}

fn named_face(state: &GameState, object: ObjectId, name: &str, face_index: u8) -> bool {
    state.objects.try_get(object).is_some_and(|live| {
        live.zone == Zone::Battlefield
            && live.v4.face_down_v1.is_none()
            && live.v4.face_index == face_index
            && definition_name(state, object) == Some(name)
    })
}

fn active_named(state: &GameState, object: ObjectId, name: &str) -> bool {
    named_face(state, object, name, 0)
        && crate::continuous_characteristics_v1::printed_abilities_active(state, object)
}

fn live_link(state: &GameState, object: ObjectId) -> ObjectLinkV4 {
    ObjectLinkV4 {
        object,
        zone_change_count: state.objects.get(object).zone_change_count,
    }
}

fn link_live_in(state: &GameState, link: ObjectLinkV4, zone: Zone) -> bool {
    state
        .objects
        .try_get(link.object)
        .is_some_and(|live| live.zone == zone && live.zone_change_count == link.zone_change_count)
}

fn battlefield(state: &GameState) -> impl Iterator<Item = ObjectId> + '_ {
    [PlayerId::P0, PlayerId::P1]
        .into_iter()
        .flat_map(|player| state.players[player.index()].battlefield.iter().copied())
}

fn bind(state: &GameState, object: ObjectId) -> EffectObjectBinding {
    let live = state.objects.get(object);
    EffectObjectBinding {
        object,
        expected_zone: live.zone,
        expected_zone_change_count: live.zone_change_count,
    }
}

fn halt(state: &mut GameState, source: ObjectId) {
    state.engine.halted = Some((
        crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
        source,
    ));
}

fn offer(
    continuation: &mut EffectContinuation,
    kind: C9aChoiceV1,
    player: PlayerId,
    bindings: Vec<EffectObjectBinding>,
    min: u16,
    max: u16,
    path: Vec<u16>,
) {
    let remaining = continuation.frames.clone();
    let candidates = bindings
        .iter()
        .map(|binding| Target::Object(binding.object))
        .collect();
    crate::eternal_cards_v1::e3::offer(
        continuation,
        E3ChoiceV1 {
            kind: E3ChoiceKindV1::C9aV1(kind),
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

// ---- Hooks: Aang, Swift Savior (airbend & waterbend) --------------------------------

/// Whether casting `source` as `player` is using an active airbend exile
/// permission (`{2}` rather than its mana cost).
pub(crate) fn is_airbent_exile_cast(state: &GameState, source: ObjectId, player: PlayerId) -> bool {
    if !cfg!(feature = "eternal-fixtures") {
        return false;
    }
    let Some(c9a) = read_state(state) else {
        return false;
    };
    if c9a.airbent_exiled.is_empty() {
        return false;
    }
    let Some(live) = state.objects.try_get(source) else {
        return false;
    };
    let exile_link = match live.zone {
        Zone::Exile => {
            if live.owner != player {
                return false;
            }
            ObjectLinkV4 {
                object: source,
                zone_change_count: live.zone_change_count,
            }
        }
        Zone::Stack => {
            let Some(origin) = live.v4.spell_cast_origin else {
                return false;
            };
            if origin.origin_zone != Zone::Exile
                || origin.origin_zone_change_count.checked_add(1) != Some(live.zone_change_count)
            {
                return false;
            }
            let SpellCastRouteV4::ExilePermission {
                holder,
                permission_zone_change_count,
            } = origin.route
            else {
                return false;
            };
            if holder != player || permission_zone_change_count != origin.origin_zone_change_count {
                return false;
            }
            ObjectLinkV4 {
                object: source,
                zone_change_count: permission_zone_change_count,
            }
        }
        _ => return false,
    };
    c9a.airbent_exiled.contains(&exile_link)
}

fn waterbend_taps(components: &[CostComponent]) -> u8 {
    components
        .iter()
        .find_map(|component| match component {
            CostComponent::TapControlled { count, .. } => Some(*count),
            _ => None,
        })
        .unwrap_or(0)
}

/// The printed cost every one of Aang, Swift Savior's Waterbend {8} payments
/// starts from, or `None` for every other ability.
pub(crate) fn waterbend_printed(
    state: &GameState,
    source: ObjectId,
    components: &[CostComponent],
) -> Option<Cost> {
    (cfg!(feature = "eternal-fixtures")
        && named_face(state, source, AANG_SWIFT_SAVIOR, 0)
        && components
            .iter()
            .any(|component| matches!(component, CostComponent::Mana(_))))
    .then_some(Cost {
        pips: &[],
        generic: AANG_WATERBEND_GENERIC,
        x_count: 0,
    })
}

/// The mana one of Aang's waterbend payments still needs once its waterbend
/// taps paid their part of `total`.
pub(crate) fn waterbend_taps_paid(
    state: &GameState,
    source: ObjectId,
    components: &[CostComponent],
    total: Cost,
) -> Cost {
    if waterbend_printed(state, source, components).is_none() {
        return total;
    }
    Cost {
        generic: total.generic.saturating_sub(waterbend_taps(components)),
        ..total
    }
}

fn waterbend_mana(state: &GameState, source: ObjectId, ability_index: usize) -> Option<Cost> {
    let live = state.objects.try_get(source)?;
    let ability = CARD_DEFS[usize::from(live.card_def)]
        .activated_abilities
        .get(ability_index)?;
    crate::engine::activation_mana_payment(ability.cost, live.controller, source, state)
}

fn waterbend_cap(state: &GameState, source: ObjectId) -> u8 {
    waterbend_mana(state, source, 0).map_or(AANG_WATERBEND_GENERIC, |total| {
        total.generic.min(AANG_WATERBEND_GENERIC)
    })
}

fn waterbend_tappable(state: &GameState, player: PlayerId, chosen: &[ObjectId]) -> Vec<ObjectId> {
    state.players[player.index()]
        .battlefield
        .iter()
        .copied()
        .filter(|object| !chosen.contains(object))
        .filter(|&object| {
            let live = state.objects.get(object);
            live.zone == Zone::Battlefield
                && live.controller == player
                && !live.tapped
                && (crate::engine::object_has_type(state, object, CardType::Artifact)
                    || crate::engine::object_has_type(state, object, CardType::Creature))
        })
        .collect()
}

fn waterbend_completion_exists(
    state: &GameState,
    player: PlayerId,
    source: ObjectId,
    cost: &Cost,
    chosen: &[ObjectId],
    more: usize,
) -> bool {
    let Some(plan) = crate::mana::plan_activation_mana_v1(cost, 0, player, state, source, chosen)
    else {
        return false;
    };
    let rest = waterbend_tappable(state, player, chosen);
    if rest.len() < more {
        return false;
    }
    let outside_plan = rest
        .iter()
        .filter(|&&object| !plan.taps.iter().any(|&(tapped, _)| tapped == object))
        .count();
    outside_plan >= more
        || waterbend_completion_among(state, player, source, cost, chosen, &rest, more)
}

fn waterbend_completion_among(
    state: &GameState,
    player: PlayerId,
    source: ObjectId,
    cost: &Cost,
    chosen: &[ObjectId],
    among: &[ObjectId],
    more: usize,
) -> bool {
    if more == 0 {
        return crate::mana::plan_activation_mana_v1(cost, 0, player, state, source, chosen)
            .is_some();
    }
    if among.len() < more {
        return false;
    }
    among.iter().enumerate().any(|(index, &next)| {
        let mut with = chosen.to_vec();
        with.push(next);
        waterbend_completion_among(
            state,
            player,
            source,
            cost,
            &with,
            &among[index + 1..],
            more - 1,
        )
    })
}

/// Aang, Swift Savior's Waterbend {8}: only payments within `waterbend_cap`
/// whose chosen taps leave the remaining mana payable are offered.
pub(crate) fn activation_allowed(
    state: &GameState,
    source: ObjectId,
    name: &str,
    ability_index: usize,
) -> bool {
    if !cfg!(feature = "eternal-fixtures") || name != AANG_SWIFT_SAVIOR {
        return true;
    }
    let Some(live) = state.objects.try_get(source) else {
        return true;
    };
    if live.zone != Zone::Battlefield || live.v4.face_index != 0 {
        return false;
    }
    let Some(ability) = CARD_DEFS[usize::from(live.card_def)]
        .activated_abilities
        .get(ability_index)
    else {
        return true;
    };
    let taps = waterbend_taps(ability.cost);
    if taps > waterbend_cap(state, source) {
        return false;
    }
    let Some(cost) = waterbend_mana(state, source, ability_index) else {
        return true;
    };
    waterbend_completion_exists(
        state,
        live.controller,
        source,
        &cost,
        &[],
        usize::from(taps),
    )
}

/// Keeps only the waterbend taps after which the rest of Aang's pending
/// Waterbend {8} payment can still be completed.
pub(crate) fn retain_payable_waterbend_taps(
    state: &GameState,
    source: ObjectId,
    already_chosen: &[EffectObjectBinding],
    candidates: &mut Vec<EffectObjectBinding>,
) {
    if !cfg!(feature = "eternal-fixtures") {
        return;
    }
    let Some(pending) = state
        .engine
        .pending_activation
        .as_ref()
        .filter(|pending| pending.source == source)
    else {
        return;
    };
    if !named_face(state, source, AANG_SWIFT_SAVIOR, 0) {
        return;
    }
    let ability_index = usize::from(pending.ability_index);
    let Some(ability) = CARD_DEFS[usize::from(state.objects.get(source).card_def)]
        .activated_abilities
        .get(ability_index)
    else {
        return;
    };
    let Some(cost) = waterbend_mana(state, source, ability_index) else {
        return;
    };
    let taps = usize::from(waterbend_taps(ability.cost));
    let chosen: Vec<ObjectId> = already_chosen
        .iter()
        .map(|binding| binding.object)
        .collect();
    let more = taps.saturating_sub(chosen.len() + 1);
    candidates.retain(|candidate| {
        let mut with = chosen.clone();
        with.push(candidate.object);
        waterbend_completion_exists(state, pending.controller, source, &cost, &with, more)
    });
}

// ---- Hooks: Badgermole Cub (earthbend & triggered mana ability) ---------------------

/// Whether `object` is currently animated by an earthbend effect.
pub(crate) fn is_earthbent(state: &GameState, object: ObjectId) -> bool {
    earthbent_timestamp(state, object).is_some()
}

/// Latest earthbend layer timestamp for `object`'s current battlefield
/// incarnation, if any.
pub(crate) fn earthbent_timestamp(state: &GameState, object: ObjectId) -> Option<u64> {
    if !cfg!(feature = "eternal-fixtures") {
        return None;
    }
    let c9a = read_state(state)?;
    if c9a.earthbent_lands.is_empty() {
        return None;
    }
    let live = state.objects.try_get(object)?;
    if live.zone != Zone::Battlefield {
        return None;
    }
    let link = ObjectLinkV4 {
        object,
        zone_change_count: live.zone_change_count,
    };
    c9a.earthbent_lands
        .iter()
        .filter(|entry| entry.land == link)
        .map(|entry| entry.timestamp)
        .max()
}

/// Layer-6 keywords granted by earthbend (Haste, subject to later ability
/// removal).
pub(crate) fn granted_keywords(state: &GameState, object: ObjectId) -> Keywords {
    let Some(timestamp) = earthbent_timestamp(state, object) else {
        return Keywords::NONE;
    };
    if crate::continuous_characteristics_v1::granted_ability_survives(state, object, timestamp) {
        Keywords::HASTE
    } else {
        Keywords::NONE
    }
}

/// Layer-7b base power/toughness (0/0) for an earthbent land.
pub(crate) fn base_power_toughness(state: &GameState, object: ObjectId) -> Option<(i16, i16, u64)> {
    earthbent_timestamp(state, object).map(|timestamp| (0, 0, timestamp))
}

/// Badgermole Cub: "Whenever you tap a creature for mana, add an additional
/// {G}." A triggered mana ability (CR 605.1b): resolves immediately without
/// using the stack. Called after `source` was tapped for mana (both manual
/// mana activation and automatic payment).
pub(crate) fn after_tapped_for_mana(state: &mut GameState, source: ObjectId) {
    if !cfg!(feature = "eternal-fixtures") {
        return;
    }
    let Some(live) = state.objects.try_get(source) else {
        return;
    };
    if live.zone != Zone::Battlefield
        || !crate::engine::object_has_type(state, source, CardType::Creature)
    {
        return;
    }
    let controller = live.controller;
    let cubs = state.players[controller.index()]
        .battlefield
        .iter()
        .copied()
        .filter(|&id| active_named(state, id, BADGERMOLE_CUB))
        .count();
    if cubs == 0 {
        return;
    }
    event::propose_and_commit(
        state,
        ProposedEvent::mana_add(controller, vec![ManaColor::G; cubs]),
    );
}

// ---- Hooks: Batroc the Leaper (multikicker, entry counters, LKI) --------------------

/// Multikicker cost for wave C9a cards (Batroc the Leaper: `Multikicker {2}`).
pub(crate) fn multikicker_cost(name: &str) -> Option<Cost> {
    match name {
        BATROC_THE_LEAPER => Some(Cost {
            pips: &[],
            generic: 2,
            x_count: 0,
        }),
        _ => None,
    }
}

/// Batroc the Leaper enters with a +1/+1 counter on him for each time he was
/// kicked (CR 614.1c), respecting counter replacement effects.
pub(crate) fn apply_entry_counters(
    state: &mut GameState,
    object: ObjectId,
    input: Option<&crate::doubling_season_v48::ReplacementInputV48>,
) {
    if !cfg!(feature = "eternal-fixtures") {
        return;
    }
    if read_state(state).is_none_or(|c9a| c9a.pending_batroc_kicks.is_empty()) {
        return;
    }
    let Some(live) = state.objects.try_get(object) else {
        return;
    };
    if live.zone != Zone::Battlefield
        || live.v4.face_index != 0
        || definition_name(state, object) != Some(BATROC_THE_LEAPER)
    {
        return;
    }
    let (controller, zcc) = (live.controller, live.zone_change_count);
    let position = c9a_state(state)
        .pending_batroc_kicks
        .iter()
        .position(|(spell, _)| {
            spell.object == object && spell.zone_change_count.checked_add(1) == Some(zcc)
        });
    let Some(position) = position else {
        return;
    };
    let (_, kicks) = c9a_state(state).pending_batroc_kicks.remove(position);
    let link = ObjectLinkV4 {
        object,
        zone_change_count: zcc,
    };
    if kicks == 0 {
        return;
    }
    let entries = &mut c9a_state(state).batroc_kicks;
    entries.retain(|(recorded, _)| *recorded != link);
    entries.push((link, kicks));
    let count = i32::from(kicks);
    match crate::doubling_season_v48::entry_amount(state, input, controller, count).and_then(
        |amount| {
            state
                .objects
                .get(object)
                .counters
                .plus1_plus1
                .checked_add(amount)
                .ok_or_else(|| "Batroc entry counter overflow".to_string())
        },
    ) {
        Ok(total) => state.objects.get_mut(object).counters.plus1_plus1 = total,
        Err(_) => halt(state, object),
    }
}

fn batroc_kicks_for(state: &GameState, link: ObjectLinkV4) -> u16 {
    read_state(state)
        .and_then(|c9a| {
            c9a.batroc_kicks
                .iter()
                .find(|&&(recorded, _)| recorded == link)
                .map(|&(_, kicks)| kicks)
        })
        .unwrap_or(0)
}

fn batroc_etb_watches(state: &GameState, link: ObjectLinkV4) -> bool {
    let matches_effect = |effect: &EffectOp| {
        matches!(
            effect,
            EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::C9aV1(
                C9aOpV1::BatrocEtbDamageV1 { source_link, .. }
            ))) if *source_link == link
        )
    };
    state
        .engine
        .pending_triggers
        .iter()
        .any(|pending| matches_effect(&pending.effect))
        || state
            .stack
            .iter()
            .any(|item| item.inline_effect.as_ref().is_some_and(matches_effect))
        || state.engine.pending_effect.as_ref().is_some_and(|pending| {
            pending
                .resolving_item
                .inline_effect
                .as_ref()
                .is_some_and(matches_effect)
        })
        || batroc_kicks_for(state, link) > 0
}

fn capture_batroc_departure(state: &mut GameState, object: ObjectId) {
    let Some(live) = state.objects.try_get(object) else {
        return;
    };
    if live.zone != Zone::Battlefield || definition_name(state, object) != Some(BATROC_THE_LEAPER) {
        return;
    }
    let link = ObjectLinkV4 {
        object,
        zone_change_count: live.zone_change_count,
    };
    if !batroc_etb_watches(state, link) {
        return;
    }
    let entry = DepartedBatrocV1 {
        link,
        turn: TurnV1::now(state),
        power: crate::engine::effective_power(state, object),
        facts: HistoricalDamageSourceV1 {
            source: AbilitySourceContractV4::capture(state, object),
            color_mask: crate::engine::object_color_mask(state, object),
            lifelink: crate::engine::has_effective_keyword(state, object, Keywords::LIFELINK),
            deathtouch: crate::engine::has_effective_keyword(state, object, Keywords::DEATHTOUCH),
            werewolf: crate::engine::has_effective_subtype(state, object, Subtype::Werewolf),
        },
    };
    let departed = &mut c9a_state(state).departed_batrocs;
    if !departed.iter().any(|existing| existing.link == entry.link) {
        departed.push(entry);
    }
}

/// Captures Batroc LKI before any permanent in a simultaneous zone-change
/// batch leaves the battlefield.
pub(crate) fn before_batch_departures(state: &mut GameState, survivors: &[ProposedEvent]) {
    if !cfg!(feature = "eternal-fixtures") {
        return;
    }
    let departing: Vec<ObjectId> = survivors
        .iter()
        .filter_map(|event| match event {
            ProposedEvent::ZoneChange(change)
                if state
                    .objects
                    .try_get(change.object)
                    .is_some_and(|live| live.zone == Zone::Battlefield) =>
            {
                Some(change.object)
            }
            _ => None,
        })
        .collect();
    for object in departing {
        capture_batroc_departure(state, object);
    }
}

/// Captures Batroc LKI and queues earthbend delayed return triggers when a
/// battlefield permanent is about to leave to `to_zone`. Only queues delayed
/// return triggers in non-scene mode (`queue_returns`).
pub(crate) fn before_departure(
    state: &mut GameState,
    object: ObjectId,
    to_zone: Zone,
    queue_returns: bool,
) {
    if !cfg!(feature = "eternal-fixtures") {
        return;
    }
    capture_batroc_departure(state, object);
    if !queue_returns || !matches!(to_zone, Zone::Graveyard | Zone::Exile) {
        return;
    }
    let Some(live) = state.objects.try_get(object) else {
        return;
    };
    if live.zone != Zone::Battlefield {
        return;
    }
    let link = ObjectLinkV4 {
        object,
        zone_change_count: live.zone_change_count,
    };
    let Some(expected_zcc) = live.zone_change_count.checked_add(1) else {
        return;
    };
    let matches: Vec<EarthbentLandV1> = read_state(state)
        .map(|c9a| {
            c9a.earthbent_lands
                .iter()
                .copied()
                .filter(|entry| entry.land == link)
                .collect()
        })
        .unwrap_or_default();
    if matches.is_empty() {
        return;
    }
    let queue = &mut c9a_state(state).pending_earthbend_returns;
    for entry in matches {
        queue.push(PendingEarthbendReturnV1 {
            source: entry.source,
            controller: entry.controller,
            land: object,
            expected_zone: to_zone,
            expected_zcc,
        });
    }
}

// ---- Hooks: Blightsteel Colossus (graveyard replacement) ----------------------------

/// Whether `card_def` is Blightsteel Colossus.
pub(crate) fn is_blightsteel(card_def: u16) -> bool {
    CARD_DEFS
        .get(usize::from(card_def))
        .is_some_and(|def| def.name == BLIGHTSTEEL_COLOSSUS)
}

/// Blightsteel Colossus: "If Blightsteel Colossus would be put into a
/// graveyard from anywhere, reveal Blightsteel Colossus and shuffle it into
/// its owner's library instead."
///
/// Returns `Some((owner, object, same_library))` when the replacement applies.
pub(crate) fn plan_blightsteel_shuffle(
    state: &GameState,
    event: &ProposedEvent,
    abilities_removed_before: Option<bool>,
) -> Option<(PlayerId, ObjectId, bool)> {
    if !cfg!(feature = "eternal-fixtures") {
        return None;
    }
    let ProposedEvent::ZoneChange(change) = event else {
        return None;
    };
    if change.to_zone != Zone::Graveyard {
        return None;
    }
    let live = state.objects.try_get(change.object)?;
    if !is_blightsteel(live.card_def) || live.v4.face_down_v1.is_some() || live.v4.face_index != 0 {
        return None;
    }
    if live.zone == Zone::Battlefield
        && abilities_removed_before.unwrap_or_else(|| {
            !crate::continuous_characteristics_v1::printed_abilities_active(state, change.object)
        })
    {
        return None;
    }
    Some((live.owner, change.object, live.zone == Zone::Library))
}

/// Performs one checked library shuffle per Blightsteel Colossus replacement
/// in original batch order.
pub(crate) fn finish_blightsteel_shuffles(
    state: &mut GameState,
    shuffles: Vec<(PlayerId, ObjectId)>,
) -> Result<(), ObjectId> {
    for (owner, source) in shuffles {
        let token = state.preflight_library_shuffle(owner).map_err(|_| source)?;
        state
            .commit_library_shuffle(owner, token)
            .map_err(|_| source)?;
    }
    Ok(())
}

// ---- Hooks: Crabomination (Emerge from artifact & resolution cast) ------------------

/// Whether `name` has "Emerge from artifact" (Crabomination).
pub(crate) fn is_emerge_artifact(name: &str) -> bool {
    cfg!(feature = "eternal-fixtures") && name == CRABOMINATION
}

/// The card a Crabomination ETB choice casts during resolution, if any.
pub(crate) fn resolution_cast_card(
    choice: &E3ChoiceV1,
    selected: &[Target],
) -> Option<EffectObjectBinding> {
    if !matches!(
        choice.kind,
        E3ChoiceKindV1::C9aV1(C9aChoiceV1::CrabominationCastV1)
    ) {
        return None;
    }
    let &Target::Object(object) = selected.first()? else {
        return None;
    };
    choice
        .bindings
        .iter()
        .copied()
        .find(|binding| binding.object == object)
}

/// Authenticates the free resolution cast authorized by Crabomination's ETB.
pub(crate) fn validate_resolution_cast(
    suspended: &EffectContinuation,
    choice: &E3ChoiceV1,
    selected: &[Target],
    permission: crate::engine::resolution_cast_v1::ResolutionPermissionV1,
) -> Result<(), String> {
    if !matches!(
        choice.kind,
        E3ChoiceKindV1::C9aV1(C9aChoiceV1::CrabominationCastV1)
    ) {
        return Err("C9a choice does not authorize a resolution cast".into());
    }
    if resolution_cast_card(choice, selected) != Some(permission.card)
        || selected.len() != 1
        || permission.card.expected_zone != Zone::Exile
        || permission.land
        || permission.maximum_mana_value.is_some()
        || permission.controller != choice.player
        || suspended.ctx.controller != choice.player
        || suspended.frames != choice.remaining
    {
        return Err("Crabomination resolution cast answer does not authorize this cast".into());
    }
    Ok(())
}

// ---- Hooks: Fear of Missing Out (attacks & additional combat phase) -----------------

/// Whether `CommittedEvent::DeclaredAttacker` should be emitted for Fear of
/// Missing Out's first-attack-each-turn tracking (even when Delirium is not
/// currently met, since attacking without Delirium still counts as its first
/// attack this turn).
pub(crate) fn observes_attacks(state: &GameState) -> bool {
    cfg!(feature = "eternal-fixtures")
        && battlefield(state).any(|object| active_named(state, object, FEAR_OF_MISSING_OUT))
}

/// Consumes one queued additional combat phase at the end of `Step::EndCombat`.
pub(crate) fn take_extra_combat_phase(state: &mut GameState) -> bool {
    if !cfg!(feature = "eternal-fixtures") {
        return false;
    }
    if read_state(state).is_none_or(|c9a| c9a.extra_combat_phases == 0) {
        return false;
    }
    let c9a = c9a_state(state);
    c9a.extra_combat_phases -= 1;
    true
}

/// Clears any unconsumed extra combat phases when a turn ends cleanly or
/// prematurely (e.g. Time Stop).
pub(crate) fn clear_extra_combat_phases(state: &mut GameState) {
    if !cfg!(feature = "eternal-fixtures") {
        return;
    }
    if let Some(eternal) = state.eternal_v1.as_mut() {
        eternal.c9a.extra_combat_phases = 0;
    }
}

// ---- Hooks: Delayed triggers, turn start, and pruning -------------------------------

/// Queues earthbend delayed returns, Batroc ETB triggers, and Fear of Missing
/// Out Delirium first-attack triggers, then prunes expired state.
pub(crate) fn delayed_triggers(
    state: &mut GameState,
    events: &[CommittedEvent],
) -> Vec<PendingTrigger> {
    if !cfg!(feature = "eternal-fixtures") {
        return Vec::new();
    }
    let mut triggers = Vec::new();

    // 1. Earthbend delayed return triggers queued when earthbent lands died or
    //    were exiled.
    if read_state(state).is_some_and(|c9a| !c9a.pending_earthbend_returns.is_empty()) {
        let queued = std::mem::take(&mut c9a_state(state).pending_earthbend_returns);
        for entry in queued {
            triggers.push(crate::eternal_cards_v1::delayed_trigger(
                entry.source,
                entry.controller,
                c9a(C9aOpV1::EarthbendReturnV1 {
                    land: entry.land,
                    expected_zone: entry.expected_zone,
                    expected_zcc: entry.expected_zcc,
                }),
            ));
        }
    }

    // 2. Batroc the Leaper ETB: deals damage equal to his power to each of up
    //    to X targets, where X is the number of times he was kicked.
    for event in events {
        let CommittedEvent::ZoneChange {
            object,
            to: Zone::Battlefield,
            ..
        } = *event
        else {
            continue;
        };
        if crate::eternal_vintage_e7b_v1::statics::entry_event_suppressed(state, event) {
            continue;
        }
        if !active_named(state, object, BATROC_THE_LEAPER) {
            continue;
        }
        let link = live_link(state, object);
        let kicks = batroc_kicks_for(state, link);
        let max_targets = u8::try_from(kicks).unwrap_or(u8::MAX);
        let controller = state.objects.get(object).controller;
        let mut trigger = crate::eternal_cards_v1::delayed_trigger(
            AbilitySourceContractV4::capture(state, object),
            controller,
            c9a(C9aOpV1::BatrocEtbDamageV1 {
                source_link: link,
                max_targets,
            }),
        );
        trigger.target_spec = if max_targets == 0 {
            TargetSpec::None
        } else {
            TargetSpec::StandardV1(StandardTargetV1::C9aV1(
                C9aTargetV1::BatrocUpToAnyTargetsV1(max_targets),
            ))
        };
        triggers.push(trigger);
    }

    // 3. Fear of Missing Out: "Delirium - Whenever this creature attacks for
    //    the first time each turn, if there are four or more card types among
    //    cards in your graveyard, untap target creature. After this phase,
    //    there is an additional combat phase."
    let now = TurnV1::now(state);
    for event in events {
        let CommittedEvent::DeclaredAttacker {
            source,
            source_zone_change_count,
            controller,
        } = *event
        else {
            continue;
        };
        if !active_named(state, source, FEAR_OF_MISSING_OUT)
            || state.objects.get(source).zone_change_count != source_zone_change_count
        {
            continue;
        }
        let link = ObjectLinkV4 {
            object: source,
            zone_change_count: source_zone_change_count,
        };
        let already_attacked = read_state(state).is_some_and(|c9a| {
            c9a.fomo_attacked_this_turn
                .iter()
                .any(|&(recorded, turn)| recorded == link && turn == now)
        });
        if already_attacked {
            continue;
        }
        // Record the first attack this turn even when Delirium is not met: a
        // later combat phase this turn is still not its first attack.
        c9a_state(state).fomo_attacked_this_turn.push((link, now));
        if crate::eternal_cards_v1::controller_has_delirium(controller, state) {
            let mut trigger = crate::eternal_cards_v1::delayed_trigger(
                AbilitySourceContractV4::capture(state, source),
                controller,
                fomo_attack_trigger(),
            );
            trigger.target_spec = TargetSpec::Creature;
            triggers.push(trigger);
        }
    }

    prune(state);
    triggers
}

/// Clears turn-scoped Fear of Missing Out state at the beginning of a turn
/// and prunes expired wave C9a records.
pub(crate) fn begin_turn(state: &mut GameState, _player: PlayerId) {
    if !cfg!(feature = "eternal-fixtures") {
        return;
    }
    let now = TurnV1::now(state);
    if let Some(eternal) = state.eternal_v1.as_mut() {
        eternal.c9a.extra_combat_phases = 0;
        eternal
            .c9a
            .fomo_attacked_this_turn
            .retain(|&(_, turn)| turn == now);
    }
    prune(state);
}

fn prune(state: &mut GameState) {
    let Some(c9a) = read_state(state) else {
        return;
    };
    if c9a.is_empty() {
        return;
    }
    let turn = TurnV1::now(state);
    let keep_airbent: Vec<bool> = c9a
        .airbent_exiled
        .iter()
        .map(|&link| {
            link_live_in(state, link, Zone::Exile)
                || state.objects.try_get(link.object).is_some_and(|live| {
                    live.zone == Zone::Stack
                        && live.v4.spell_cast_origin.is_some_and(|origin| {
                            origin.origin_zone == Zone::Exile
                                && matches!(
                                    origin.route,
                                    SpellCastRouteV4::ExilePermission {
                                        permission_zone_change_count,
                                        ..
                                    } if permission_zone_change_count == link.zone_change_count
                                )
                        })
                })
        })
        .collect();
    let keep_earthbent: Vec<bool> = c9a
        .earthbent_lands
        .iter()
        .map(|entry| link_live_in(state, entry.land, Zone::Battlefield))
        .collect();
    let keep_pending_batroc: Vec<bool> = c9a
        .pending_batroc_kicks
        .iter()
        .map(|&(link, _)| link_live_in(state, link, Zone::Stack))
        .collect();
    let source_referenced = |link: ObjectLinkV4| {
        state.stack.iter().any(|item| {
            item.v4.ability_source_contract.is_some_and(|c| {
                c.source == link.object && c.zone_change_count == link.zone_change_count
            })
        }) || state.engine.pending_triggers.iter().any(|pending| {
            pending.source_contract.is_some_and(|c| {
                c.source == link.object && c.zone_change_count == link.zone_change_count
            })
        }) || state.engine.pending_effect.as_ref().is_some_and(|pending| {
            pending.ctx.ability_source_contract.is_some_and(|c| {
                c.source == link.object && c.zone_change_count == link.zone_change_count
            })
        })
    };
    let keep_batroc_kicks: Vec<bool> = c9a
        .batroc_kicks
        .iter()
        .map(|&(link, _)| link_live_in(state, link, Zone::Battlefield) || source_referenced(link))
        .collect();
    let keep_departed_batroc: Vec<bool> = c9a
        .departed_batrocs
        .iter()
        .map(|entry| entry.turn == turn || source_referenced(entry.link))
        .collect();
    let keep_fomo_attacked: Vec<bool> = c9a
        .fomo_attacked_this_turn
        .iter()
        .map(|&(link, recorded_turn)| {
            recorded_turn == turn && link_live_in(state, link, Zone::Battlefield)
        })
        .collect();

    let c9a = c9a_state(state);
    let mut idx = 0;
    c9a.airbent_exiled.retain(|_| {
        let keep = keep_airbent[idx];
        idx += 1;
        keep
    });
    idx = 0;
    c9a.earthbent_lands.retain(|_| {
        let keep = keep_earthbent[idx];
        idx += 1;
        keep
    });
    idx = 0;
    c9a.pending_batroc_kicks.retain(|_| {
        let keep = keep_pending_batroc[idx];
        idx += 1;
        keep
    });
    idx = 0;
    c9a.batroc_kicks.retain(|_| {
        let keep = keep_batroc_kicks[idx];
        idx += 1;
        keep
    });
    idx = 0;
    c9a.departed_batrocs.retain(|_| {
        let keep = keep_departed_batroc[idx];
        idx += 1;
        keep
    });
    idx = 0;
    c9a.fomo_attacked_this_turn.retain(|_| {
        let keep = keep_fomo_attacked[idx];
        idx += 1;
        keep
    });
}

// ---- Execution ----------------------------------------------------------------------

/// Runs one of this module's leaves in synchronous execution.
pub(crate) fn execute(op: &C9aOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        C9aOpV1::AangAirbendV1 => {
            let Some(Target::Object(target)) = ctx.targets.first().copied() else {
                return;
            };
            if !ctx.target_incarnation_matches(0, state)
                || !ctx.ability_source_contract.is_some_and(|source| {
                    crate::engine::effect_target_is_legal_from_ability_source(
                        state,
                        source,
                        ctx.controller,
                        AANG_ETB_TARGET,
                        &ctx.targets,
                        0,
                    )
                })
            {
                return;
            }
            let Some(live) = state.objects.try_get(target) else {
                return;
            };
            match live.zone {
                Zone::Stack => {
                    if crate::engine::apply_live_stack_spell_departure(state, target, Zone::Exile)
                        .is_err()
                    {
                        halt(state, ctx.source);
                        return;
                    }
                }
                Zone::Battlefield => {
                    event::propose_and_commit(
                        state,
                        ProposedEvent::zone_change(target, Zone::Exile),
                    );
                }
                _ => return,
            }
            let Some(after) = state.objects.try_get(target) else {
                return;
            };
            if after.zone != Zone::Exile || !state.exile.contains(&target) || after.v4.is_token {
                return;
            }
            let def = &CARD_DEFS[usize::from(after.card_def)];
            if def.is_land || def.is_playable_land() || !def.is_castable() {
                return;
            }
            let owner = after.owner;
            let exile_link = ObjectLinkV4 {
                object: target,
                zone_change_count: after.zone_change_count,
            };
            state.engine.exile_play_permissions.push(PlayPermission {
                without_mana_cost: FreeCastV1(false),
                any_mana_type: AnyManaTypeV1(false),
                object: target,
                holder: owner,
                zone_change_generation: exile_link.zone_change_count,
                play_or_cast: PlayOrCast::Cast,
                expiry: PlayPermissionExpiry::AsLongAsExiled,
            });
            let list = &mut c9a_state(state).airbent_exiled;
            if !list.contains(&exile_link) {
                list.push(exile_link);
            }
        }
        C9aOpV1::AangBackAttackV1 => {
            let creatures: Vec<ObjectId> = state.players[ctx.controller.index()]
                .battlefield
                .iter()
                .copied()
                .filter(|&id| {
                    let live = state.objects.get(id);
                    live.zone == Zone::Battlefield
                        && live.controller == ctx.controller
                        && live.tapped
                        && crate::engine::object_has_type(state, id, CardType::Creature)
                })
                .collect();
            for creature in creatures {
                if event::add_plus_one_counters(state, creature, ctx.controller, 1).is_err() {
                    halt(state, ctx.source);
                    return;
                }
            }
        }
        C9aOpV1::BadgermoleEarthbendV1 => {
            let Some(Target::Object(land)) = ctx.targets.first().copied() else {
                return;
            };
            let Some(contract) = ctx.ability_source_contract else {
                halt(state, ctx.source);
                return;
            };
            if !ctx.target_incarnation_matches(0, state)
                || !crate::engine::effect_target_is_legal_from_ability_source(
                    state,
                    contract,
                    ctx.controller,
                    BADGERMOLE_ETB_TARGET,
                    &ctx.targets,
                    0,
                )
            {
                return;
            }
            let link = live_link(state, land);
            let timestamp = crate::engine::next_timestamp(state);
            c9a_state(state).earthbent_lands.push(EarthbentLandV1 {
                land: link,
                timestamp,
                controller: ctx.controller,
                source: contract,
            });
            if event::add_plus_one_counters(state, land, ctx.controller, 1).is_err() {
                halt(state, ctx.source);
            }
        }
        C9aOpV1::EarthbendReturnV1 {
            land,
            expected_zone,
            expected_zcc,
        } => {
            let Some(live) = state.objects.try_get(*land) else {
                return;
            };
            if live.v4.is_token
                || live.zone != *expected_zone
                || live.zone_change_count != *expected_zcc
            {
                return;
            }
            let owner = live.owner;
            let present = match *expected_zone {
                Zone::Graveyard => state.players[owner.index()].graveyard.contains(land),
                Zone::Exile => state.exile.contains(land),
                _ => false,
            };
            if !present {
                return;
            }
            let mut move_event = ProposedEvent::zone_change(*land, Zone::Battlefield);
            if let ProposedEvent::ZoneChange(ref mut change) = move_event {
                change.force_battlefield_tapped = true;
                change.battlefield_controller = Some(owner);
            }
            event::propose_and_commit(state, move_event);
        }
        C9aOpV1::BatrocEnterV1 => {
            if state
                .objects
                .try_get(ctx.source)
                .is_none_or(|live| live.zone != Zone::Stack)
            {
                return;
            }
            if ctx.x_value > 0 {
                let stack_link = live_link(state, ctx.source);
                let pending = &mut c9a_state(state).pending_batroc_kicks;
                pending.retain(|(recorded, _)| *recorded != stack_link);
                pending.push((stack_link, ctx.x_value));
            }
            crate::effect::execute(
                &EffectOp::MoveObject {
                    object: ObjectRef::ThisSource,
                    to_zone: Zone::Battlefield,
                },
                ctx,
                state,
            );
        }
        C9aOpV1::BatrocEtbDamageV1 {
            source_link,
            max_targets,
        } => {
            if *max_targets == 0 || ctx.targets.is_empty() {
                return;
            }
            let Some(contract) = ctx.ability_source_contract else {
                halt(state, ctx.source);
                return;
            };
            let (amount, historical_facts) = if link_live_in(state, *source_link, Zone::Battlefield)
            {
                (
                    crate::engine::effective_power(state, source_link.object),
                    None,
                )
            } else {
                let Some(lki) = read_state(state).and_then(|c9a| {
                    c9a.departed_batrocs
                        .iter()
                        .find(|entry| entry.link == *source_link)
                        .copied()
                }) else {
                    return;
                };
                (lki.power, Some(lki.facts))
            };
            if amount <= 0 {
                return;
            }
            let spec = TargetSpec::StandardV1(StandardTargetV1::C9aV1(
                C9aTargetV1::BatrocUpToAnyTargetsV1(*max_targets),
            ));
            let mut events = Vec::new();
            for index in 0..ctx.targets.len() {
                if !ctx.target_incarnation_matches(index, state)
                    || !crate::engine::effect_target_is_legal_from_ability_source(
                        state,
                        contract,
                        ctx.controller,
                        spec,
                        &ctx.targets,
                        index,
                    )
                {
                    continue;
                }
                let mut proposal =
                    ProposedEvent::damage(source_link.object, ctx.targets[index], amount);
                if let (ProposedEvent::Damage(damage), Some(facts)) =
                    (&mut proposal, historical_facts)
                {
                    damage.eternal_damage_lki_v1 =
                        Some(crate::effect::EternalDamageLkiV1::for_c8c(facts));
                }
                events.push(proposal);
            }
            if !events.is_empty() {
                event::propose_and_commit_batch(state, events);
            }
        }
        C9aOpV1::FomoAttackTriggerV1 => {
            // Intervening-if Delirium recheck on resolution (CR 603.4).
            if !crate::eternal_cards_v1::controller_has_delirium(ctx.controller, state) {
                return;
            }
            let Some(Target::Object(creature)) = ctx.targets.first().copied() else {
                return;
            };
            let Some(contract) = ctx.ability_source_contract else {
                halt(state, ctx.source);
                return;
            };
            if !ctx.target_incarnation_matches(0, state)
                || !crate::engine::effect_target_is_legal_from_ability_source(
                    state,
                    contract,
                    ctx.controller,
                    TargetSpec::Creature,
                    &ctx.targets,
                    0,
                )
            {
                return;
            }
            if state.objects.get(creature).tapped {
                crate::engine::attempt_untap(state, creature);
            }
            let c9a = c9a_state(state);
            c9a.extra_combat_phases = c9a.extra_combat_phases.saturating_add(1);
        }
        C9aOpV1::CrabominationEtbV1 => {
            debug_assert!(false, "choice C9a leaf reached synchronous execute");
        }
    }
}

/// Runs one of this module's leaves in the resumable interpreter.
pub(crate) fn drive(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    op: &C9aOpV1,
    path: Vec<u16>,
) -> Result<bool, String> {
    match op {
        C9aOpV1::CrabominationEtbV1 => {
            let Some(Target::Player(opponent)) = continuation.ctx.targets.first().copied() else {
                return Ok(false);
            };
            if !continuation.ctx.target_incarnation_matches(0, state)
                || !continuation
                    .ctx
                    .ability_source_contract
                    .is_some_and(|source| {
                        crate::engine::effect_target_is_legal_from_ability_source(
                            state,
                            source,
                            continuation.ctx.controller,
                            TargetSpec::TargetOpponent,
                            &continuation.ctx.targets,
                            0,
                        )
                    })
            {
                return Ok(false);
            }
            let controller = continuation.ctx.controller;
            let mut to_exile = Vec::new();
            if let Some(top) = state.players[opponent.index()].library.first().copied() {
                to_exile.push(top);
            }
            if let Some(gy_card) = state.random_graveyard_card_v1(opponent).map_err(|error| {
                format!("Crabomination graveyard random choice failed: {error:?}")
            })? {
                to_exile.push(gy_card);
            }
            if let Some(hand_card) = state
                .random_hand_card_v1(opponent)
                .map_err(|error| format!("Crabomination hand random choice failed: {error:?}"))?
            {
                to_exile.push(hand_card);
            }
            if to_exile.is_empty() {
                return Ok(false);
            }
            let events: Vec<ProposedEvent> = to_exile
                .iter()
                .map(|&card| ProposedEvent::zone_change(card, Zone::Exile))
                .collect();
            event::propose_and_commit_batch(state, events);
            let castable: Vec<EffectObjectBinding> = to_exile
                .into_iter()
                .filter(|&card| {
                    state.exile.contains(&card)
                        && state
                            .objects
                            .try_get(card)
                            .is_some_and(|live| live.zone == Zone::Exile && !live.v4.is_token)
                })
                .map(|card| bind(state, card))
                .filter(|&binding| {
                    crate::engine::resolution_cast_v1::can_cast_exiled_without_mana(
                        state, controller, binding, None,
                    )
                })
                .collect();
            if castable.is_empty() {
                return Ok(false);
            }
            offer(
                continuation,
                C9aChoiceV1::CrabominationCastV1,
                controller,
                castable,
                0,
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

/// Applies an answered C9a resolution choice.
pub(crate) fn finish_selection(
    _state: &mut GameState,
    _continuation: &mut EffectContinuation,
    _choice: &E3ChoiceV1,
    kind: &C9aChoiceV1,
    _bindings: Vec<EffectObjectBinding>,
) -> Result<bool, String> {
    match kind {
        // An affirmative pick is intercepted by `dungeons::resolution_cast_card`
        // and cast during resolution; declining leaves all exiled cards in exile.
        C9aChoiceV1::CrabominationCastV1 => Ok(false),
    }
}
