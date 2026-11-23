//! Card programs for the eternal catalog's Vintage Cube wave C7c
//! (`docs/design/eternal_formats_support_v1.md`): evoke, linked exile/duration,
//! persist, backup and Sanctum Lurker.
//!
//! `eternal_cards_v1` forwards this module's effect leaves, triggers, entry
//! counters and turn bookkeeping here, and the recipe tables of
//! `build_eternal_c7c_v1.rs` name its programs. Every table is keyed by a
//! printed name that occurs only in the eternal registry, so the Pauper, FDN
//! and Standard catalogs never reach it. Behavior was read from each card's
//! current Oracle text and, where present, its XMage source.
//!
//! Where each card lives:
//!
//! - **Vibrance** and **Wistfulness**: hybrid Evoke alternative costs reuse
//!   wave E7b's `EvokedEntersV1` sacrifice trigger, and per-color mana spent
//!   (`SpellManaByColorV1`, recorded at cast payment finalization) powers the
//!   intervening-if color-spent ETB triggers (`EntersIfColorSpentAtLeastV1`).
//! - **Tidehollow Sculler**: reuses Mesmeric Fiend's exact linked hand-exile
//!   machinery (`LinkedHandExileKind::ReturnTrigger` + `ReturnLinkedExiledCardToOwnersHand`),
//!   so leaving before the ETB resolves exiles the chosen nonland card
//!   permanently.
//! - **The Wondrous Wasp**: ETB taps up to one target creature and records a
//!   linked ability-loss entry (`WaspStingV1`) read by
//!   `continuous_characteristics_v1::removal_timestamp_with` while both the
//!   Wasp's exact battlefield incarnation and the target remain on the
//!   battlefield.
//! - **Woodfall Primus**: ETB destroys target noncreature permanent
//!   (`StandardTargetV1::NoncreaturePermanentV1`); Persist checks LKI for zero
//!   -1/-1 counters (`DiesWithoutMinusOneCountersV1`) and returns Primus under
//!   its owner's control with a -1/-1 counter applied as it enters
//!   (`apply_entry_counters`).
//! - **Sanctum Lurker**: ETB reuses wave E7b's `EmpowerJaceV1 { amount: 1 }`;
//!   exempts controlled planeswalkers from the 0-loyalty SBA
//!   (`planeswalker_survives_zero_loyalty`); and grants controlled planeswalkers
//!   the `[+2]` loyalty ability at dedicated index `LURKER_GRANTED_INDEX`.
//! - **Guardian Scalelord**: Backup 1 (`ScalelordBackupV1`) puts a +1/+1
//!   counter on target creature and, if it is another creature, grants Flying
//!   and the attack reanimation trigger until end of turn (`ScalelordBackupV1`
//!   record collected on `CommittedEvent::DeclaredAttacker`), using the
//!   attacking creature's power as X via `targets`.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

pub(crate) mod targets;

use crate::card_def::{ActivatedAbilityDef, CardType, Keywords, TargetSpec, CARD_DEFS};
use crate::effect::{
    EffectCond, EffectContinuation, EffectOp, ExecCtx, LibraryCardFilter, ObjectRef, PlayerRef,
    TargetRef,
};
use crate::eternal_cards_v1::{EternalOpV1, EternalTriggerV1};
use crate::eternal_legacy_e9a_v1::TurnV1;
use crate::eternal_vintage_e7b_v1::{E7bOpV1, E7bTriggerV1};
use crate::event::{self, CommittedEvent, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::mana::ManaColor;
use crate::standard_cards_v1::{StandardOpV1, StandardTargetV1, StandardTriggerV1};
use crate::state::{
    AbilitySourceContractV4, CastMethodV4, GameState, ObjectLinkV4, StackItemKind, Target, Zone,
};
use crate::trigger::{PendingTrigger, TriggerCondition, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};

pub(crate) const VIBRANCE: &str = "Vibrance";
pub(crate) const WISTFULNESS: &str = "Wistfulness";
pub(crate) const TIDEHOLLOW_SCULLER: &str = "Tidehollow Sculler";
pub(crate) const THE_WONDROUS_WASP: &str = "The Wondrous Wasp";
pub(crate) const WOODFALL_PRIMUS: &str = "Woodfall Primus";
pub(crate) const SANCTUM_LURKER: &str = "Sanctum Lurker";
pub(crate) const GUARDIAN_SCALELORD: &str = "Guardian Scalelord";

/// Dedicated activation index for Sanctum Lurker's granted `[+2]` loyalty
/// ability on controlled planeswalkers, avoiding collisions with printed
/// abilities (`0..len`), Avatar of Burgeoning Echoes (`len`), Cauldron grants
/// (`len + 1 + i`), and turn-face-up (`255`).
pub(crate) const LURKER_GRANTED_INDEX: u8 = 254;

/// Guardian Scalelord's attack-trigger target spec: a nonland permanent card
/// in your graveyard with mana value at most the attacking creature's power.
pub(crate) const SCALELORD_TARGET: TargetSpec = TargetSpec::StandardV1(
    StandardTargetV1::NonlandPermanentCardInOwnGraveyardManaValueAtMostSourcePowerV1,
);

// ---- State --------------------------------------------------------------------------

/// Per-color mana spent to cast one spell incarnation on the stack.
/// `counts` is indexed by `ManaColor::pool_index()` (W, U, B, R, G, C).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpellManaByColorV1 {
    pub spell: ObjectLinkV4,
    pub counts: [u8; 6],
}

/// One active The Wondrous Wasp ability-loss effect: `target` loses all
/// abilities at `timestamp` for as long as `wasp` remains on the battlefield.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WaspStingV1 {
    pub wasp: ObjectLinkV4,
    pub target: ObjectLinkV4,
    pub timestamp: u64,
}

/// One Guardian Scalelord Backup 1 attack-trigger grant to another creature
/// `host` for the rest of `turn`, with layer-6 timestamp `timestamp`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScalelordBackupV1 {
    pub host: ObjectLinkV4,
    pub turn: TurnV1,
    pub timestamp: u64,
}

/// Per-game state owned by wave C7c, kept inside
/// `eternal_cards_v1::EternalStateV1`. Absent from serialization and hashes to
/// zero bytes while empty so every game without these cards keeps its exact
/// snapshot and hash.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct C7cStateV1 {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spell_mana_by_color: Vec<SpellManaByColorV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub wasp_stings: Vec<WaspStingV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pending_persist_minus1: Vec<ObjectLinkV4>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scalelord_backups: Vec<ScalelordBackupV1>,
}

impl C7cStateV1 {
    pub(crate) fn is_empty(&self) -> bool {
        self.spell_mana_by_color.is_empty()
            && self.wasp_stings.is_empty()
            && self.pending_persist_minus1.is_empty()
            && self.scalelord_backups.is_empty()
    }
}

/// An empty state hashes to nothing, so every game without these cards keeps
/// its state hash.
impl std::hash::Hash for C7cStateV1 {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        if !self.is_empty() {
            b"eternal-cube-c7c/v1".hash(state);
            self.spell_mana_by_color.hash(state);
            self.wasp_stings.hash(state);
            self.pending_persist_minus1.hash(state);
            self.scalelord_backups.hash(state);
        }
    }
}

fn c7c_state(state: &mut GameState) -> &mut C7cStateV1 {
    &mut state.eternal_v1.get_or_insert_with(Default::default).c7c
}

fn read_state(state: &GameState) -> Option<&C7cStateV1> {
    state.eternal_v1.as_ref().map(|eternal| &eternal.c7c)
}

// ---- Triggers and ops ---------------------------------------------------------------

/// Trigger conditions owned by wave C7c, carried as
/// `eternal_cards_v1::EternalTriggerV1::C7cV1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum C7cTriggerV1 {
    /// Intervening-if: "When this creature enters, if `{color}` x `amount` was
    /// spent to cast it" (Vibrance, Wistfulness).
    EntersIfColorSpentAtLeastV1 { color: ManaColor, amount: u8 },
    /// Persist (702.79a): "When this creature dies, if it had no -1/-1
    /// counters on it" (Woodfall Primus).
    DiesWithoutMinusOneCountersV1,
}

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::C7cV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum C7cOpV1 {
    /// The Wondrous Wasp: tap up to one target creature; it loses all
    /// abilities for as long as The Wondrous Wasp remains on the battlefield.
    WaspStingV1,
    /// Persist: return this card from its owner's graveyard to the battlefield
    /// under its owner's control with a -1/-1 counter on it.
    PersistReturnV1,
    /// Guardian Scalelord's Backup 1: put a +1/+1 counter on target creature;
    /// if that is another creature, it gains flying and the attack reanimation
    /// trigger until end of turn.
    ScalelordBackupV1,
}

impl C7cOpV1 {
    /// Whether the leaf may stop for a player's choice (or a draw that a
    /// replacement can interrupt), so it runs in the resumable interpreter.
    pub(crate) fn contains_player_choice(&self) -> bool {
        match self {
            Self::WaspStingV1 | Self::PersistReturnV1 | Self::ScalelordBackupV1 => false,
        }
    }
}

fn c7c(op: C7cOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::C7cV1(op)))
}

fn definition_name(state: &GameState, object: ObjectId) -> Option<&'static str> {
    let live = state.objects.try_get(object)?;
    CARD_DEFS
        .get(usize::from(live.card_def))
        .filter(|def| def.is_executable())
        .map(|def| def.name)
}

fn live_link(state: &GameState, object: ObjectId) -> ObjectLinkV4 {
    ObjectLinkV4 {
        object,
        zone_change_count: state.objects.get(object).zone_change_count,
    }
}

fn battlefield_link(state: &GameState, object: ObjectId) -> Option<ObjectLinkV4> {
    let live = state.objects.try_get(object)?;
    (live.zone == Zone::Battlefield).then_some(ObjectLinkV4 {
        object,
        zone_change_count: live.zone_change_count,
    })
}

fn link_live_in(state: &GameState, link: ObjectLinkV4, zone: Zone) -> bool {
    state
        .objects
        .try_get(link.object)
        .is_some_and(|live| live.zone == zone && live.zone_change_count == link.zone_change_count)
}

fn active_named(state: &GameState, object: ObjectId, name: &str) -> bool {
    state.objects.try_get(object).is_some_and(|live| {
        live.zone == Zone::Battlefield
            && live.v4.face_index == 0
            && live.v4.face_down_v1.is_none()
            && CARD_DEFS
                .get(usize::from(live.card_def))
                .is_some_and(|def| def.name == name && def.is_executable())
            && crate::continuous_characteristics_v1::printed_abilities_active(state, object)
    })
}

fn active_permanents<'a>(
    state: &'a GameState,
    name: &'static str,
    controller: Option<PlayerId>,
) -> impl Iterator<Item = ObjectId> + 'a {
    [PlayerId::P0, PlayerId::P1]
        .into_iter()
        .flat_map(move |seat| state.players[seat.index()].battlefield.iter().copied())
        .filter(move |&object| {
            active_named(state, object, name)
                && controller.is_none_or(|player| state.objects.get(object).controller == player)
        })
}

// ---- Programs -----------------------------------------------------------------------

/// Vibrance: "this creature deals 3 damage to any target."
fn vibrance_red() -> EffectOp {
    EffectOp::Conditional {
        cond: EffectCond::TargetIsLegalForAbility {
            index: 0,
            spec: TargetSpec::AnyTarget,
        },
        then: Box::new(EffectOp::DealDamage {
            target: TargetRef::Target(0),
            amount: 3,
        }),
        else_: Box::new(EffectOp::Sequence(Vec::new())),
    }
}

/// Vibrance: "search your library for a land card, reveal it, put it into your
/// hand, then shuffle. You gain 2 life."
fn vibrance_green() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::SearchLibraryToHand {
            player: PlayerRef::Controller,
            filter: LibraryCardFilter::AnyLand,
        },
        EffectOp::GainLife {
            player: PlayerRef::Controller,
            amount: 2,
        },
    ])
}

/// Evoke (702.74a): sacrifice the evoked permanent when it enters.
fn sacrifice_source() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::SacrificeSourceV1))
}

/// Wistfulness: "exile target artifact or enchantment an opponent controls."
fn wistfulness_green() -> EffectOp {
    EffectOp::Conditional {
        cond: EffectCond::TargetIsLegalForAbility {
            index: 0,
            spec: TargetSpec::OpponentArtifactOrEnchantmentPermanent,
        },
        then: Box::new(EffectOp::MoveAllTargets {
            to_zone: Zone::Exile,
        }),
        else_: Box::new(EffectOp::Sequence(Vec::new())),
    }
}

/// Wistfulness: "draw two cards, then discard a card." The draw is a direct
/// element of the root `Sequence` so competing draw replacements can choose;
/// `DiscardCards` is the final instruction.
fn wistfulness_blue() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 2,
        },
        crate::eternal_cube_c1a_v1::stop_if_won(),
        EffectOp::DiscardCards {
            player: PlayerRef::Controller,
            count: 1,
        },
    ])
}

/// Tidehollow Sculler ETB: Mesmeric Fiend's exact linked hand-exile op.
fn tidehollow_exile() -> EffectOp {
    EffectOp::RevealHandChooseNonlandToLinkedExile {
        player: PlayerRef::Target(0),
    }
}

/// Tidehollow Sculler LTB: return the linked exiled card to its owner's hand.
fn tidehollow_return() -> EffectOp {
    EffectOp::ReturnLinkedExiledCardToOwnersHand
}

/// The Wondrous Wasp ETB: tap up to one target creature; it loses all
/// abilities while The Wondrous Wasp remains on the battlefield.
fn wasp_sting() -> EffectOp {
    c7c(C7cOpV1::WaspStingV1)
}

/// Woodfall Primus ETB: destroy target noncreature permanent.
fn primus_destroy() -> EffectOp {
    EffectOp::Conditional {
        cond: EffectCond::TargetIsLegalForAbility {
            index: 0,
            spec: TargetSpec::StandardV1(StandardTargetV1::NoncreaturePermanentV1),
        },
        then: Box::new(EffectOp::DestroyObject {
            object: ObjectRef::Target(0),
        }),
        else_: Box::new(EffectOp::Sequence(Vec::new())),
    }
}

/// Woodfall Primus Persist: return from graveyard under owner's control with a
/// -1/-1 counter on it.
fn primus_persist() -> EffectOp {
    c7c(C7cOpV1::PersistReturnV1)
}

/// Sanctum Lurker ETB: "empower Jace 1" (reusing wave E7b's `EmpowerJaceV1`).
fn lurker_empower() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::E7bV1(
        E7bOpV1::EmpowerJaceV1 { amount: 1 },
    )))
}

/// Sanctum Lurker's granted loyalty ability:
/// "[+2]: This planeswalker deals 1 damage to each opponent and you gain 1 life."
pub fn lurker_plus_two() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::DealDamage {
            target: TargetRef::Opponent,
            amount: 1,
        },
        EffectOp::GainLife {
            player: PlayerRef::Controller,
            amount: 1,
        },
    ])
}

/// Guardian Scalelord Backup 1 ETB trigger.
fn scalelord_backup() -> EffectOp {
    c7c(C7cOpV1::ScalelordBackupV1)
}

/// Guardian Scalelord attack trigger (both printed on Scalelord and granted
/// until end of turn to a backed-up creature): return target nonland permanent
/// card with mana value X or less from your graveyard to the battlefield, where
/// X is this creature's power.
pub(crate) fn scalelord_attack() -> EffectOp {
    EffectOp::Conditional {
        cond: EffectCond::TargetIsLegalForAbility {
            index: 0,
            spec: SCALELORD_TARGET,
        },
        then: Box::new(EffectOp::ReturnTargetPermanentToBattlefield { target_index: 0 }),
        else_: Box::new(EffectOp::Sequence(Vec::new())),
    }
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

const fn c7c_condition(condition: C7cTriggerV1) -> TriggerCondition {
    TriggerCondition::StandardV1(StandardTriggerV1::EternalV1(EternalTriggerV1::C7cV1(
        condition,
    )))
}

const fn evoked_condition() -> TriggerCondition {
    TriggerCondition::StandardV1(StandardTriggerV1::EternalV1(EternalTriggerV1::E7bV1(
        E7bTriggerV1::EvokedEntersV1,
    )))
}

const VIBRANCE_TRIGGERS: [TriggeredAbilityDef; 3] = [
    trigger(
        c7c_condition(C7cTriggerV1::EntersIfColorSpentAtLeastV1 {
            color: ManaColor::R,
            amount: 2,
        }),
        vibrance_red,
    ),
    trigger(
        c7c_condition(C7cTriggerV1::EntersIfColorSpentAtLeastV1 {
            color: ManaColor::G,
            amount: 2,
        }),
        vibrance_green,
    ),
    trigger(evoked_condition(), sacrifice_source),
];

const WISTFULNESS_TRIGGERS: [TriggeredAbilityDef; 3] = [
    trigger(
        c7c_condition(C7cTriggerV1::EntersIfColorSpentAtLeastV1 {
            color: ManaColor::G,
            amount: 2,
        }),
        wistfulness_green,
    ),
    trigger(
        c7c_condition(C7cTriggerV1::EntersIfColorSpentAtLeastV1 {
            color: ManaColor::U,
            amount: 2,
        }),
        wistfulness_blue,
    ),
    trigger(evoked_condition(), sacrifice_source),
];

const TIDEHOLLOW_SCULLER_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(TriggerCondition::Etb, tidehollow_exile),
    TriggeredAbilityDef {
        condition: TriggerCondition::LeftBattlefield,
        // Leave triggers are matched from battlefield last-known information
        // and therefore ignore the source's post-event zone.
        home_zone: Zone::Graveyard,
        intervening_if_kicked: false,
        intervening_if_controls_another_source_card: false,
        face_index: 0,
        effect: tidehollow_return,
    },
];

const WASP_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(TriggerCondition::Etb, wasp_sting)];

const WOODFALL_PRIMUS_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(TriggerCondition::Etb, primus_destroy),
    TriggeredAbilityDef {
        condition: c7c_condition(C7cTriggerV1::DiesWithoutMinusOneCountersV1),
        home_zone: Zone::Graveyard,
        intervening_if_kicked: false,
        intervening_if_controls_another_source_card: false,
        face_index: 0,
        effect: primus_persist,
    },
];

const SANCTUM_LURKER_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, lurker_empower)];

const GUARDIAN_SCALELORD_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(TriggerCondition::Etb, scalelord_backup),
    trigger(TriggerCondition::Attacks, scalelord_attack),
];

/// Triggered abilities of this module's cards, the tail of the trigger tables.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        VIBRANCE => &VIBRANCE_TRIGGERS,
        WISTFULNESS => &WISTFULNESS_TRIGGERS,
        TIDEHOLLOW_SCULLER => &TIDEHOLLOW_SCULLER_TRIGGERS,
        THE_WONDROUS_WASP => &WASP_TRIGGERS,
        WOODFALL_PRIMUS => &WOODFALL_PRIMUS_TRIGGERS,
        SANCTUM_LURKER => &SANCTUM_LURKER_TRIGGERS,
        GUARDIAN_SCALELORD => &GUARDIAN_SCALELORD_TRIGGERS,
        _ => &[],
    }
}

fn own_trigger_effect(name: &str, effect: &EffectOp) -> bool {
    triggers_for(name)
        .iter()
        .any(|def| (def.effect)() == *effect)
        || delayed_effect_matches(name, effect)
}

/// Target specs of this module's printed and queued triggered abilities.
pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    if !own_trigger_effect(name, effect) {
        return None;
    }
    Some(match name {
        VIBRANCE if *effect == vibrance_red() => TargetSpec::AnyTarget,
        WISTFULNESS if *effect == wistfulness_green() => {
            TargetSpec::OpponentArtifactOrEnchantmentPermanent
        }
        TIDEHOLLOW_SCULLER if *effect == tidehollow_exile() => TargetSpec::TargetOpponent,
        THE_WONDROUS_WASP if *effect == wasp_sting() => {
            TargetSpec::StandardV1(StandardTargetV1::UpToOneCreature)
        }
        WOODFALL_PRIMUS if *effect == primus_destroy() => {
            TargetSpec::StandardV1(StandardTargetV1::NoncreaturePermanentV1)
        }
        GUARDIAN_SCALELORD if *effect == scalelord_backup() => TargetSpec::Creature,
        GUARDIAN_SCALELORD if *effect == scalelord_attack() => SCALELORD_TARGET,
        _ => TargetSpec::None,
    })
}

/// Queued triggers owned by this module's cards.
pub(crate) fn delayed_effect_matches(_name: &str, _effect: &EffectOp) -> bool {
    false
}

/// Guardian Scalelord's attack trigger may be granted until end of turn to any
/// other creature via Backup 1.
pub(crate) fn granted_trigger_effect(effect: &EffectOp) -> bool {
    *effect == scalelord_attack()
}

/// Target spec for Guardian Scalelord's granted attack trigger, whatever
/// creature has it.
pub(crate) fn granted_trigger_target_spec(effect: &EffectOp) -> Option<TargetSpec> {
    granted_trigger_effect(effect).then_some(SCALELORD_TARGET)
}

/// Whether `condition` triggers on its source's arrival on the battlefield, so
/// the trigger's source contract captures the new battlefield incarnation.
pub(crate) fn arrival_condition(condition: C7cTriggerV1) -> bool {
    matches!(condition, C7cTriggerV1::EntersIfColorSpentAtLeastV1 { .. })
}

pub(crate) fn trigger_matches(
    condition: C7cTriggerV1,
    events: &[CommittedEvent],
    index: usize,
    source: ObjectId,
    state: &GameState,
) -> bool {
    let Some(live) = state.objects.try_get(source) else {
        return false;
    };
    match condition {
        C7cTriggerV1::EntersIfColorSpentAtLeastV1 { color, amount } => {
            let Some(name) = definition_name(state, source) else {
                return false;
            };
            if !active_named(state, source, name) {
                return false;
            }
            let entered_from_stack = matches!(
                events[index],
                CommittedEvent::ZoneChange {
                    object,
                    from: Zone::Stack,
                    to: Zone::Battlefield,
                    ..
                } if object == source
            );
            if !entered_from_stack {
                return false;
            }
            let Some(stack_zcc) = live.zone_change_count.checked_sub(1) else {
                return false;
            };
            let stack_link = ObjectLinkV4 {
                object: source,
                zone_change_count: stack_zcc,
            };
            read_state(state).is_some_and(|c7c| {
                c7c.spell_mana_by_color.iter().any(|entry| {
                    entry.spell == stack_link && entry.counts[color.pool_index()] >= amount
                })
            })
        }
        C7cTriggerV1::DiesWithoutMinusOneCountersV1 => {
            let died = matches!(
                events[index],
                CommittedEvent::ZoneChange {
                    object,
                    from: Zone::Battlefield,
                    to: Zone::Graveyard,
                    ..
                } if object == source
            );
            if !died
                || crate::standard_cards_v1::departure_face(events, index, source).unwrap_or(0) != 0
            {
                return false;
            }
            live.zone_change_count
                .checked_sub(1)
                .is_some_and(|departed| {
                    state
                        .counter_lki_for(source, departed)
                        .is_none_or(|counters| counters.minus1_minus1 == 0)
                })
        }
    }
}

// ---- Engine hooks -------------------------------------------------------------------

/// Records per-color mana spent to cast Vibrance or Wistfulness on the stack.
pub(crate) fn note_mana_by_color(state: &mut GameState, spell: ObjectId, counts: [u8; 6]) {
    if !matches!(definition_name(state, spell), Some(VIBRANCE | WISTFULNESS)) {
        return;
    }
    let Some(live) = state.objects.try_get(spell) else {
        return;
    };
    if live.zone != Zone::Stack {
        return;
    }
    let link = live_link(state, spell);
    let entries = &mut c7c_state(state).spell_mana_by_color;
    if let Some(existing) = entries.iter_mut().find(|entry| entry.spell == link) {
        existing.counts = counts;
    } else {
        entries.push(SpellManaByColorV1 {
            spell: link,
            counts,
        });
    }
}

/// Whether `name` is Woodfall Primus, whose Persist trigger reads the -1/-1
/// counters it had as it left the battlefield.
pub(crate) fn reads_counter_lki(name: &str) -> bool {
    name == WOODFALL_PRIMUS
}

/// Applies the -1/-1 counter to a creature returning to the battlefield via
/// Persist as it enters (CR 702.79a), respecting prevention (Darksteel Angel)
/// and counter-entry modifiers.
pub(crate) fn apply_entry_counters(
    state: &mut GameState,
    object: ObjectId,
    input: Option<&crate::doubling_season_v48::ReplacementInputV48>,
) {
    if read_state(state).is_none_or(|c7c| c7c.pending_persist_minus1.is_empty()) {
        return;
    }
    let Some(live) = state.objects.try_get(object) else {
        return;
    };
    if live.zone != Zone::Battlefield || live.v4.face_index != 0 {
        return;
    }
    let (controller, zone_change_count) = (live.controller, live.zone_change_count);
    let position = c7c_state(state)
        .pending_persist_minus1
        .iter()
        .position(|graveyard| {
            graveyard.object == object
                && graveyard.zone_change_count.checked_add(1) == Some(zone_change_count)
        });
    let Some(position) = position else {
        return;
    };
    c7c_state(state).pending_persist_minus1.remove(position);
    if input.map_or_else(
        || crate::eternal_sideboard_s1n_v1::controls_darksteel_angel(state, controller),
        |captured| captured.entry_prevents_minus1_minus1_v1,
    ) {
        return;
    }
    match crate::doubling_season_v48::entry_amount(state, input, controller, 1)
        .and_then(|count| i16::try_from(count).map_err(|error| error.to_string()))
        .and_then(|count| {
            state
                .objects
                .get(object)
                .counters
                .minus1_minus1
                .checked_add(count)
                .ok_or_else(|| "persist entry counter overflow".to_string())
        }) {
        Ok(total) => state.objects.get_mut(object).counters.minus1_minus1 = total,
        Err(_) => {
            state.engine.halted = Some((
                crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                object,
            ));
        }
    }
}

/// The latest layer-6 ability-removal timestamp from The Wondrous Wasp on
/// `object`'s current battlefield incarnation, active only while that Wasp's
/// exact battlefield incarnation remains on the battlefield (and phased in).
pub(crate) fn removal_timestamp(state: &GameState, object: ObjectId) -> Option<u64> {
    if !cfg!(feature = "eternal-fixtures") {
        return None;
    }
    let c7c = read_state(state)?;
    if c7c.wasp_stings.is_empty() {
        return None;
    }
    let host = battlefield_link(state, object)?;
    c7c.wasp_stings
        .iter()
        .filter(|entry| {
            entry.target == host
                && link_live_in(state, entry.wasp, Zone::Battlefield)
                && !crate::eternal_e5a_v1::phasing::phased_out(state, entry.wasp.object)
        })
        .map(|entry| entry.timestamp)
        .max()
}

/// Sanctum Lurker: "Planeswalkers you control aren't put into their owners'
/// graveyards for having 0 loyalty."
pub(crate) fn planeswalker_survives_zero_loyalty(state: &GameState, object: ObjectId) -> bool {
    if !cfg!(feature = "eternal-fixtures") {
        return false;
    }
    let Some(live) = state.objects.try_get(object) else {
        return false;
    };
    live.zone == Zone::Battlefield
        && active_permanents(state, SANCTUM_LURKER, Some(live.controller))
            .next()
            .is_some()
}

fn lurker_ability() -> Option<ActivatedAbilityDef> {
    let lurker = crate::card_def::card_id_by_name(SANCTUM_LURKER)?;
    CARD_DEFS
        .get(usize::from(lurker))?
        .activated_abilities
        .first()
        .copied()
}

/// Sanctum Lurker: 'Planeswalkers you control have "[+2]: This planeswalker
/// deals 1 damage to each opponent and you gain 1 life."' Offered on each
/// planeswalker its controller controls (respecting layer-6 timestamp order
/// against ability removals on that planeswalker).
pub(crate) fn granted_lurker_ability(
    state: &GameState,
    object: ObjectId,
) -> Option<ActivatedAbilityDef> {
    let live = state.objects.try_get(object)?;
    if live.zone != Zone::Battlefield
        || !crate::engine::object_has_type(state, object, CardType::Planeswalker)
    {
        return None;
    }
    let granted = active_permanents(state, SANCTUM_LURKER, Some(live.controller)).any(|lurker| {
        let timestamp = state.objects.get(lurker).v4.layer_timestamp.unwrap_or(0);
        crate::continuous_characteristics_v1::granted_ability_survives(state, object, timestamp)
    });
    if !granted {
        return None;
    }
    lurker_ability()
}

/// Resolves Sanctum Lurker's granted loyalty ability on the stack regardless
/// of what happened to Sanctum Lurker since activation (CR 113.7a).
pub(crate) fn granted_lurker_ability_on_stack(
    host: AbilitySourceContractV4,
    ability_index: u8,
) -> Option<ActivatedAbilityDef> {
    if ability_index != LURKER_GRANTED_INDEX {
        return None;
    }
    let def = CARD_DEFS.get(usize::from(host.card_def))?;
    let is_planeswalker_def = def.has_type(CardType::Planeswalker)
        || def
            .transform_face
            .as_ref()
            .is_some_and(|back| back.types.contains(&CardType::Planeswalker));
    if !is_planeswalker_def {
        return None;
    }
    lurker_ability()
}

/// Sanctum Lurker's own stored copy of `[+2]` is never offered on the Horror
/// itself unless it is somehow a planeswalker.
pub(crate) fn activation_allowed(state: &GameState, source: ObjectId, name: &str) -> bool {
    name != SANCTUM_LURKER || crate::engine::object_has_type(state, source, CardType::Planeswalker)
}

/// Whether any active Backup 1 grant from Guardian Scalelord needs
/// `CommittedEvent::DeclaredAttacker` to be emitted for attackers this turn.
pub(crate) fn observes_attacks(state: &GameState) -> bool {
    if !cfg!(feature = "eternal-fixtures") {
        return false;
    }
    let turn = TurnV1::now(state);
    read_state(state).is_some_and(|c7c| {
        c7c.scalelord_backups
            .iter()
            .any(|grant| grant.turn == turn && link_live_in(state, grant.host, Zone::Battlefield))
    })
}

/// Bookkeeping of evoke casts (Vibrance, Wistfulness) and Guardian Scalelord's
/// Backup-granted attack triggers, followed by pruning expired records.
pub(crate) fn delayed_triggers(
    state: &mut GameState,
    events: &[CommittedEvent],
) -> Vec<PendingTrigger> {
    if !cfg!(feature = "eternal-fixtures") {
        return Vec::new();
    }
    note_evoked_casts(state, events);
    let triggers = scalelord_granted_attack_triggers(state, events);
    prune(state);
    triggers
}

/// Records Vibrance and Wistfulness spells cast for their Evoke alternative
/// cost in wave E7b's shared `evoked_casts` table.
fn note_evoked_casts(state: &mut GameState, events: &[CommittedEvent]) {
    for event in events {
        let CommittedEvent::SpellCast { spell, .. } = *event else {
            continue;
        };
        if !matches!(definition_name(state, spell), Some(VIBRANCE | WISTFULNESS)) {
            continue;
        }
        let evoked = state.stack.iter().any(|item| {
            item.kind == StackItemKind::Spell
                && item.source == spell
                && !item.is_copy
                && item.v4.cast_method == Some(CastMethodV4::Alternative)
        });
        if evoked && state.objects.get(spell).zone == Zone::Stack {
            let link = live_link(state, spell);
            let casts = &mut crate::eternal_vintage_e7b_v1::e7b_state(state).evoked_casts;
            if !casts.contains(&link) {
                casts.push(link);
            }
        }
    }
}

/// Emits Guardian Scalelord's granted attack trigger for each declared
/// attacker carrying an active Backup 1 grant this turn whose layer-6 grant
/// survived any later ability removals.
fn scalelord_granted_attack_triggers(
    state: &GameState,
    events: &[CommittedEvent],
) -> Vec<PendingTrigger> {
    let Some(c7c) = read_state(state) else {
        return Vec::new();
    };
    if c7c.scalelord_backups.is_empty() {
        return Vec::new();
    }
    let turn = TurnV1::now(state);
    let mut triggers = Vec::new();
    for event in events {
        let CommittedEvent::DeclaredAttacker {
            source,
            source_zone_change_count,
            controller,
        } = *event
        else {
            continue;
        };
        let attacker = ObjectLinkV4 {
            object: source,
            zone_change_count: source_zone_change_count,
        };
        if !link_live_in(state, attacker, Zone::Battlefield) {
            continue;
        }
        for grant in &c7c.scalelord_backups {
            if grant.host == attacker
                && grant.turn == turn
                && crate::continuous_characteristics_v1::granted_ability_survives(
                    state,
                    source,
                    grant.timestamp,
                )
            {
                let mut contract = AbilitySourceContractV4::capture(state, source);
                contract.controller = controller;
                let mut pending = crate::eternal_cards_v1::delayed_trigger(
                    contract,
                    controller,
                    scalelord_attack(),
                );
                pending.target_spec = SCALELORD_TARGET;
                triggers.push(pending);
            }
        }
    }
    triggers
}

/// Drops records whose objects moved on or whose turn ended.
fn prune(state: &mut GameState) {
    let Some(c7c) = read_state(state) else {
        return;
    };
    if c7c.is_empty() {
        return;
    }
    let now = TurnV1::now(state);
    let spell_mana = c7c
        .spell_mana_by_color
        .iter()
        .copied()
        .filter(|entry| link_live_in(state, entry.spell, Zone::Stack))
        .collect::<Vec<_>>();
    let wasp_stings = c7c
        .wasp_stings
        .iter()
        .copied()
        .filter(|entry| {
            let target_present = link_live_in(state, entry.target, Zone::Battlefield)
                || (link_live_in(state, entry.target, Zone::Command)
                    && crate::eternal_e5a_v1::phasing::phased_out(state, entry.target.object));
            link_live_in(state, entry.wasp, Zone::Battlefield) && target_present
        })
        .collect::<Vec<_>>();
    let backups = c7c
        .scalelord_backups
        .iter()
        .copied()
        .filter(|grant| grant.turn == now && link_live_in(state, grant.host, Zone::Battlefield))
        .collect::<Vec<_>>();
    let changed = spell_mana.len() != c7c.spell_mana_by_color.len()
        || wasp_stings.len() != c7c.wasp_stings.len()
        || backups.len() != c7c.scalelord_backups.len();
    if changed {
        let c7c = c7c_state(state);
        c7c.spell_mana_by_color = spell_mana;
        c7c.wasp_stings = wasp_stings;
        c7c.scalelord_backups = backups;
    }
}

/// Drops turn-scoped Backup grants and stale records as a new turn begins.
pub(crate) fn begin_turn(state: &mut GameState, _player: PlayerId) {
    if read_state(state).is_none_or(C7cStateV1::is_empty) {
        return;
    }
    prune(state);
}

// ---- Execution ----------------------------------------------------------------------

/// Runs one of this module's leaves in synchronous execution.
pub(crate) fn execute(op: &C7cOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        C7cOpV1::WaspStingV1 => {
            let Some(Target::Object(target)) = ctx.targets.first().copied() else {
                return;
            };
            let Some(source) = ctx.ability_source_contract else {
                return;
            };
            let spec = TargetSpec::StandardV1(StandardTargetV1::UpToOneCreature);
            if !ctx.target_incarnation_matches(0, state)
                || !crate::engine::effect_target_is_legal_from_ability_source(
                    state,
                    source,
                    ctx.controller,
                    spec,
                    &ctx.targets,
                    0,
                )
            {
                return;
            }
            if !state.objects.get(target).tapped {
                event::propose_and_commit(state, ProposedEvent::tap(target));
            }
            let wasp = ObjectLinkV4 {
                object: source.source,
                zone_change_count: source.zone_change_count,
            };
            let Some(target_link) = battlefield_link(state, target) else {
                return;
            };
            // CR 611.2b: if The Wondrous Wasp has already left the battlefield
            // before its ETB resolves, the duration has already ended and the
            // ability-loss effect does not begin (though the creature was still
            // tapped).
            if link_live_in(state, wasp, Zone::Battlefield)
                && !crate::eternal_e5a_v1::phasing::phased_out(state, wasp.object)
            {
                let timestamp = crate::engine::next_timestamp(state);
                c7c_state(state).wasp_stings.push(WaspStingV1 {
                    wasp,
                    target: target_link,
                    timestamp,
                });
            }
        }
        C7cOpV1::PersistReturnV1 => {
            let Some(source) = ctx.ability_source_contract else {
                return;
            };
            if source.source != ctx.source || source.zone != Zone::Battlefield {
                return;
            }
            let Some(graveyard_zcc) = source.zone_change_count.checked_add(1) else {
                return;
            };
            let Some(live) = state.objects.try_get(source.source) else {
                return;
            };
            if live.zone != Zone::Graveyard
                || live.zone_change_count != graveyard_zcc
                || live.v4.is_token
                || !state.players[live.owner.index()]
                    .graveyard
                    .contains(&source.source)
            {
                return;
            }
            let owner = live.owner;
            let graveyard_link = ObjectLinkV4 {
                object: source.source,
                zone_change_count: graveyard_zcc,
            };
            c7c_state(state).pending_persist_minus1.push(graveyard_link);
            event::propose_and_commit(
                state,
                ProposedEvent::zone_change_to_battlefield_under_controller(source.source, owner),
            );
            // Clear any leftover marker if a replacement prevented entry.
            if let Some(eternal) = state.eternal_v1.as_mut() {
                eternal
                    .c7c
                    .pending_persist_minus1
                    .retain(|entry| *entry != graveyard_link);
            }
        }
        C7cOpV1::ScalelordBackupV1 => {
            let Some(Target::Object(target)) = ctx.targets.first().copied() else {
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
                    TargetSpec::Creature,
                    &ctx.targets,
                    0,
                )
            {
                return;
            }
            crate::effect::execute(
                &EffectOp::AddCountersToTarget {
                    target_index: 0,
                    optional: false,
                    plus1_plus1: 1,
                    lifelink: 0,
                    stun: 0,
                },
                ctx,
                state,
            );
            if state.engine.halted.is_some() {
                return;
            }
            // Backup 1 grants the following abilities until end of turn only
            // when the target is ANOTHER creature.
            if target != ctx.source {
                let Some(host) = battlefield_link(state, target) else {
                    return;
                };
                let timestamp = crate::engine::next_timestamp(state);
                state.engine.until_end_of_turn.push(
                    crate::engine::UntilEndOfTurnEffect::ResolvedObjectKeywordEffect {
                        object_id: host.object,
                        object_zone_change_count: host.zone_change_count,
                        layer: crate::engine::Layers::ABILITY_ADDING,
                        timestamp,
                        duration: crate::engine::EffectDuration::EndOfTurn,
                        keywords: Keywords::FLYING,
                    },
                );
                let turn = TurnV1::now(state);
                c7c_state(state).scalelord_backups.push(ScalelordBackupV1 {
                    host,
                    turn,
                    timestamp,
                });
            }
        }
    }
}

/// Runs one of this module's leaves in the resumable interpreter.
pub(crate) fn drive(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    op: &C7cOpV1,
    _path: Vec<u16>,
) -> Result<bool, String> {
    let ctx = continuation.ctx.clone();
    execute(op, &ctx, state);
    Ok(false)
}
