//! Card programs for the eternal catalog's Vintage Cube wave C8c
//! (`docs/design/eternal_formats_support_v1.md`): Pyrogoyf, Elite Spellbinder,
//! Wan Shi Tong (All-Knowing & Librarian), Thieving Skydiver, Zirda, the
//! Dawnwaker, and Absorbing Man.
//!
//! `eternal_cards_v1` forwards this module's effect leaves, triggers, CDA,
//! combat restrictions, and turn bookkeeping here, and the recipe tables of
//! `build_eternal_c8c_v1.rs` name its programs. Every table is keyed by a
//! printed name that occurs only in the eternal registry, so the Pauper, FDN,
//! and Standard catalogs never reach it. Behavior was read from each card's
//! current Oracle text.
//!
//! Where each card lives:
//!
//! - **Pyrogoyf**: characteristic-defining power/toughness (`*/1+*`) counts
//!   card types across all graveyards (`characteristic_defining_power` /
//!   `characteristic_defining_toughness`). Entering Lhurgoyf creatures under
//!   its controller's control emit `PyrogoyfDamageV1 { entering }` triggers
//!   via `delayed_triggers`; the entering Lhurgoyf deals damage equal to its
//!   power (or last-known power/keywords/color captured by `note_departure` /
//!   `before_batch_departures`) to any target, while Pyrogoyf remains the
//!   ability source for target legality.
//! - **Elite Spellbinder**: ETB (`EliteSpellbinderEtbV1`) privately reveals
//!   target opponent's hand to its controller and offers an optional nonland
//!   card exile (`C8cChoiceV1::SpellbinderExileV1`). An exiled card stays
//!   exiled permanently with a `PlayPermissionExpiry::AsLongAsExiled` play
//!   permission for its owner and a `+{2}` generic spell-cast tax tracked by
//!   exact exile incarnation (`spellbinder_spell_tax`).
//! - **Wan Shi Tong, All-Knowing** & **Wan Shi Tong Spirit Token**: ETB uses
//!   native `PutObjectInOwnersLibrarySecondOrBottom`; batched trigger
//!   `CardsPutIntoLibraryFromAnywhereV1` creates two 1/1 colorless Spirit
//!   tokens whose combat blocking restriction ("can't block or be blocked by
//!   non-Spirit creatures") is enforced in `blocker_allowed`.
//! - **Wan Shi Tong, Librarian**: spell resolution (`LibrarianEnterV1`)
//!   records cast-time `X` on its entering battlefield incarnation so ETB
//!   (`LibrarianEtbV1`) puts `X` +1/+1 counters if live and performs resumable
//!   draws of `X / 2` cards via `dredge::drive_draws`. Library search sites
//!   notify `note_library_searched`, queueing `LibrarianSearchTriggerV1` for
//!   each opponent-controlled active Librarian.
//! - **Thieving Skydiver**: `Kicker {X}` with `X >= 1` (`minimum_x` + engine
//!   kicker-X support). Kicked resolution (`SkydiverEnterV1`) records `X` for
//!   the intervening-if kicked ETB trigger (`SkydiverEtbV1`, targeting
//!   `C8cTargetV1::SkydiverArtifactV1`), which permanently gains control of
//!   the target artifact and, if it is a noncreature Equipment and Skydiver is
//!   still a legal host creature, attaches it to Skydiver.
//! - **Zirda, the Dawnwaker**: Companion deck check
//!   (`zirda_companion_condition_met`), `{2}` generic reduction on non-mana
//!   activated abilities across all zones floored at one mana
//!   (`reduce_nonmana_activation`), and `{1}, {T}: Target creature can't block
//!   this turn` (`ZirdaCantBlockV1` + `blocker_allowed`).
//! - **Absorbing Man**: `ControllerPrecombatMain` trigger (`AbsorbingManCopyV1`,
//!   targeting `C8cTargetV1::UpToOneArtifactNonAuraEnchantmentOrLandV1`)
//!   records an `AbsorbingCopyV1` lasting until its controller's next turn
//!   (`begin_turn`) or until Absorbing Man leaves the battlefield
//!   (`end_copy_before_departure`), keeping name `"Absorbing Man"`, supertype
//!   `Legendary`, type `Creature`, subtypes `Human` and `Villain`, base P/T
//!   `4/4`, and `Vigilance`.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

pub mod targets;

use crate::card_def::{
    card_id_by_name, mana_colors_mask, CardDef, CardType, CostComponent, DynamicValueDef, Keywords,
    Subtype, Supertype, TargetSpec, CARD_DEFS,
};
use crate::effect::{
    EffectContinuation, EffectObjectBinding, EffectOp, ExecCtx, ObjectRef, PlayerRef,
};
use crate::engine::{AnyManaTypeV1, FreeCastV1, PlayOrCast, PlayPermission, PlayPermissionExpiry};
use crate::eternal_cards_v1::e3::{E3ChoiceKindV1, E3ChoiceV1};
use crate::eternal_cards_v1::{EternalOpV1, EternalTriggerV1};
use crate::eternal_legacy_e9a_v1::TurnV1;
use crate::event::{self, CommittedEvent, HistoricalDamageSourceV1, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::mana::Cost;
use crate::standard_cards_v1::{StandardOpV1, StandardTargetV1, StandardTriggerV1};
use crate::state::{
    AbilitySourceContractV4, GameState, ObjectLinkV4, ObjectStateV4, SpellCastRouteV4, Target, Zone,
};
use crate::trigger::{PendingTrigger, TriggerCondition, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};
use targets::C8cTargetV1;

pub(crate) const PYROGOYF: &str = "Pyrogoyf";
pub(crate) const ELITE_SPELLBINDER: &str = "Elite Spellbinder";
pub(crate) const WAN_SHI_TONG_ALL_KNOWING: &str = "Wan Shi Tong, All-Knowing";
pub(crate) const WAN_SHI_TONG_SPIRIT_TOKEN: &str = "Wan Shi Tong Spirit Token";
pub(crate) const WAN_SHI_TONG_LIBRARIAN: &str = "Wan Shi Tong, Librarian";
pub(crate) const THIEVING_SKYDIVER: &str = "Thieving Skydiver";
pub(crate) const ZIRDA: &str = "Zirda, the Dawnwaker";
pub(crate) const ABSORBING_MAN: &str = "Absorbing Man";

pub(crate) const SKYDIVER_TARGET: TargetSpec =
    TargetSpec::StandardV1(StandardTargetV1::C8cV1(C8cTargetV1::SkydiverArtifactV1));

pub(crate) const ABSORBING_MAN_TARGET: TargetSpec = TargetSpec::StandardV1(
    StandardTargetV1::C8cV1(C8cTargetV1::UpToOneArtifactNonAuraEnchantmentOrLandV1),
);

// ---- State --------------------------------------------------------------------------

/// Last-known power and damage-source facts of a Lhurgoyf that left the
/// battlefield while a Pyrogoyf ETB trigger could still reference it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DepartedLhurgoyfV1 {
    pub link: ObjectLinkV4,
    pub turn: TurnV1,
    pub power: i32,
    pub facts: HistoricalDamageSourceV1,
}

/// Queued Wan Shi Tong, Librarian opponent-search trigger captured at the
/// exact moment the opponent searched their library.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LibrarianSearchTriggerV1 {
    pub source: AbilitySourceContractV4,
    pub controller: PlayerId,
}

/// Active Absorbing Man copy record, reverting at the start of `until_turn_of`'s
/// next turn or when `host` leaves the battlefield.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AbsorbingCopyV1 {
    pub host: ObjectLinkV4,
    pub until_turn_of: PlayerId,
    pub orig_card_def: u16,
    pub orig_name: String,
    pub orig_color_mask: u8,
    pub orig_subtype_ids: Vec<u16>,
    pub orig_face_index: u8,
    pub orig_ward_generic: u16,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub added_artifact: bool,
}

/// Per-game state owned by wave C8c, kept inside
/// `eternal_cards_v1::EternalStateV1`. Absent from serialization and hashes to
/// zero bytes while empty so every game without these cards keeps its exact
/// snapshot and hash.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct C8cStateV1 {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub departed_lhurgoyfs: Vec<DepartedLhurgoyfV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spellbinder_exiled: Vec<ObjectLinkV4>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cast_x_entries: Vec<(ObjectLinkV4, u16)>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub librarian_search_triggers: Vec<LibrarianSearchTriggerV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cant_block_creatures: Vec<(ObjectLinkV4, TurnV1)>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub absorbing_copies: Vec<AbsorbingCopyV1>,
}

impl C8cStateV1 {
    pub fn is_empty(&self) -> bool {
        self.departed_lhurgoyfs.is_empty()
            && self.spellbinder_exiled.is_empty()
            && self.cast_x_entries.is_empty()
            && self.librarian_search_triggers.is_empty()
            && self.cant_block_creatures.is_empty()
            && self.absorbing_copies.is_empty()
    }
}

/// An empty state hashes to zero bytes so every game without these cards keeps
/// its exact state hash.
impl std::hash::Hash for C8cStateV1 {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        if !self.is_empty() {
            b"eternal-cube-c8c/v1".hash(state);
            self.departed_lhurgoyfs.hash(state);
            self.spellbinder_exiled.hash(state);
            self.cast_x_entries.hash(state);
            self.librarian_search_triggers.hash(state);
            self.cant_block_creatures.hash(state);
            self.absorbing_copies.hash(state);
        }
    }
}

fn c8c_state(state: &mut GameState) -> &mut C8cStateV1 {
    &mut state.eternal_v1.get_or_insert_with(Default::default).c8c
}

fn read_state(state: &GameState) -> Option<&C8cStateV1> {
    state.eternal_v1.as_ref().map(|eternal| &eternal.c8c)
}

// ---- Triggers, ops, and choices -----------------------------------------------------

/// Trigger conditions owned by wave C8c, carried as
/// `eternal_cards_v1::EternalTriggerV1::C8cV1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum C8cTriggerV1 {
    /// Wan Shi Tong, All-Knowing: "Whenever one or more cards are put into a
    /// library from anywhere". Fires once per collected event batch.
    CardsPutIntoLibraryFromAnywhereV1,
}

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::C8cV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum C8cOpV1 {
    /// Pyrogoyf: the entering Lhurgoyf deals damage equal to its power to any
    /// target.
    PyrogoyfDamageV1 { entering: ObjectLinkV4 },
    /// Elite Spellbinder: look at target opponent's hand and optionally exile
    /// a nonland card from it with permanent play permission and a +{2} tax.
    EliteSpellbinderEtbV1,
    /// Wan Shi Tong, Librarian spell resolution: move to battlefield and
    /// preserve cast-time X onto the entering incarnation.
    LibrarianEnterV1,
    /// Wan Shi Tong, Librarian ETB: put X +1/+1 counters on him, then draw
    /// X / 2 cards rounded down.
    LibrarianEtbV1,
    /// Wan Shi Tong, Librarian opponent-search trigger: put a +1/+1 counter on
    /// him and draw a card.
    LibrarianSearchTriggerV1,
    /// Thieving Skydiver spell resolution: move to battlefield and preserve
    /// kicked X onto the entering incarnation.
    SkydiverEnterV1,
    /// Thieving Skydiver kicked ETB: gain control of target artifact with mana
    /// value X or less; if it is an Equipment, attach it to Skydiver.
    SkydiverEtbV1,
    /// Zirda, the Dawnwaker: target creature can't block this turn.
    ZirdaCantBlockV1,
    /// Absorbing Man: until your next turn, become a copy of up to one target
    /// artifact, non-Aura enchantment, or land with Absorbing Man's copy
    /// exceptions.
    AbsorbingManCopyV1,
}

impl C8cOpV1 {
    /// Whether the leaf may stop for a player's choice (or a draw that a
    /// replacement can interrupt), so it runs in the resumable interpreter.
    pub(crate) fn contains_player_choice(&self) -> bool {
        matches!(
            self,
            Self::EliteSpellbinderEtbV1 | Self::LibrarianEtbV1 | Self::LibrarianSearchTriggerV1
        )
    }
}

/// Resolution choices owned by this module, carried as
/// `E3ChoiceKindV1::C8cV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum C8cChoiceV1 {
    /// Elite Spellbinder: optional exile of a nonland card from target
    /// opponent's looked-at hand.
    SpellbinderExileV1,
}

impl C8cChoiceV1 {
    pub(crate) fn chooser_private(&self) -> bool {
        match self {
            Self::SpellbinderExileV1 => true,
        }
    }

    pub(crate) fn public_purpose(&self) -> crate::rl::TargetSelectionPurposeV4 {
        match self {
            Self::SpellbinderExileV1 => crate::rl::TargetSelectionPurposeV4::CardSelection,
        }
    }

    pub(crate) fn referenced_bindings(&self) -> Vec<EffectObjectBinding> {
        Vec::new()
    }
}

fn c8c(op: C8cOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::C8cV1(op)))
}

// ---- Card programs ------------------------------------------------------------------

/// Wan Shi Tong, Librarian spell resolution.
pub fn librarian_enter() -> EffectOp {
    c8c(C8cOpV1::LibrarianEnterV1)
}

/// Thieving Skydiver spell resolution.
pub fn skydiver_enter() -> EffectOp {
    c8c(C8cOpV1::SkydiverEnterV1)
}

/// Zirda, the Dawnwaker's activated ability: "Target creature can't block this
/// turn."
pub fn zirda_cant_block() -> EffectOp {
    c8c(C8cOpV1::ZirdaCantBlockV1)
}

fn pyrogoyf_damage(entering: ObjectLinkV4) -> EffectOp {
    c8c(C8cOpV1::PyrogoyfDamageV1 { entering })
}

fn elite_spellbinder_etb() -> EffectOp {
    c8c(C8cOpV1::EliteSpellbinderEtbV1)
}

fn wan_shi_tong_tuck() -> EffectOp {
    EffectOp::PutObjectInOwnersLibrarySecondOrBottom {
        object: ObjectRef::Target(0),
    }
}

fn wan_shi_tong_spirits() -> EffectOp {
    EffectOp::CreateTokensDynamic {
        token_def: card_id_by_name(WAN_SHI_TONG_SPIRIT_TOKEN)
            .expect("Wan Shi Tong Spirit Token in CARD_DEFS"),
        controller: PlayerRef::Controller,
        count: DynamicValueDef::Fixed(2),
        tapped: false,
    }
}

fn librarian_etb() -> EffectOp {
    c8c(C8cOpV1::LibrarianEtbV1)
}

fn librarian_search_trigger() -> EffectOp {
    c8c(C8cOpV1::LibrarianSearchTriggerV1)
}

fn skydiver_etb() -> EffectOp {
    c8c(C8cOpV1::SkydiverEtbV1)
}

fn absorbing_man_copy() -> EffectOp {
    c8c(C8cOpV1::AbsorbingManCopyV1)
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

const ELITE_SPELLBINDER_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, elite_spellbinder_etb)];

const WAN_SHI_TONG_ALL_KNOWING_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(TriggerCondition::Etb, wan_shi_tong_tuck),
    trigger(
        TriggerCondition::StandardV1(StandardTriggerV1::EternalV1(EternalTriggerV1::C8cV1(
            C8cTriggerV1::CardsPutIntoLibraryFromAnywhereV1,
        ))),
        wan_shi_tong_spirits,
    ),
];

const WAN_SHI_TONG_LIBRARIAN_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, librarian_etb)];

const THIEVING_SKYDIVER_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    intervening_if_kicked: true,
    ..trigger(TriggerCondition::Etb, skydiver_etb)
}];

const ABSORBING_MAN_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(
    TriggerCondition::StandardV1(StandardTriggerV1::ControllerPrecombatMain),
    absorbing_man_copy,
)];

/// Triggered abilities of this module's cards, the tail of the trigger tables.
/// Pyrogoyf's Lhurgoyf-entry trigger and Wan Shi Tong, Librarian's opponent
/// library-search trigger are queued via `delayed_triggers`.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        ELITE_SPELLBINDER => &ELITE_SPELLBINDER_TRIGGERS,
        WAN_SHI_TONG_ALL_KNOWING => &WAN_SHI_TONG_ALL_KNOWING_TRIGGERS,
        WAN_SHI_TONG_LIBRARIAN => &WAN_SHI_TONG_LIBRARIAN_TRIGGERS,
        THIEVING_SKYDIVER => &THIEVING_SKYDIVER_TRIGGERS,
        ABSORBING_MAN => &ABSORBING_MAN_TRIGGERS,
        _ => crate::eternal_cube_c9a_v1::triggers_for(name),
    }
}

/// Target specs of this module's printed and queued triggered abilities.
pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    match name {
        PYROGOYF
            if matches!(
                effect,
                EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::C8cV1(
                    C8cOpV1::PyrogoyfDamageV1 { .. }
                )))
            ) =>
        {
            Some(TargetSpec::AnyTarget)
        }
        ELITE_SPELLBINDER if *effect == elite_spellbinder_etb() => Some(TargetSpec::TargetOpponent),
        WAN_SHI_TONG_ALL_KNOWING if *effect == wan_shi_tong_tuck() => {
            Some(TargetSpec::NonlandPermanent)
        }
        THIEVING_SKYDIVER if *effect == skydiver_etb() => Some(SKYDIVER_TARGET),
        ABSORBING_MAN if *effect == absorbing_man_copy() => Some(ABSORBING_MAN_TARGET),
        _ => delayed_effect_matches(name, effect).then_some(TargetSpec::None),
    }
}

/// Queued triggers owned by this module's cards.
pub(crate) fn delayed_effect_matches(name: &str, effect: &EffectOp) -> bool {
    (name == PYROGOYF
        && matches!(
            effect,
            EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::C8cV1(
                C8cOpV1::PyrogoyfDamageV1 { .. }
            )))
        ))
        || (name == WAN_SHI_TONG_LIBRARIAN && *effect == librarian_search_trigger())
}

fn card_put_into_library(state: &GameState, event: &CommittedEvent) -> bool {
    let CommittedEvent::ZoneChange {
        object,
        from,
        to: Zone::Library,
        ..
    } = *event
    else {
        return false;
    };
    if from == Zone::Library {
        return false;
    }
    state.objects.try_get(object).is_some_and(|card| {
        !card.v4.is_token
            && card.spell_copy_origin.is_none()
            && CARD_DEFS
                .get(usize::from(card.card_def))
                .is_some_and(|def| !def.is_token)
    })
}

/// Evaluates C8c printed trigger conditions.
pub(crate) fn trigger_matches(
    condition: C8cTriggerV1,
    events: &[CommittedEvent],
    index: usize,
    _source: ObjectId,
    state: &GameState,
) -> bool {
    match condition {
        C8cTriggerV1::CardsPutIntoLibraryFromAnywhereV1 => {
            card_put_into_library(state, &events[index])
                && !events[..index]
                    .iter()
                    .any(|event| card_put_into_library(state, event))
        }
    }
}

// ---- Helpers ------------------------------------------------------------------------

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
            && !crate::eternal_e5a_v1::phasing::phased_out(state, object)
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
        .flat_map(|seat| state.players[seat.index()].battlefield.iter().copied())
        .filter(move |&object| {
            active_named(state, object, name)
                && controller.is_none_or(|who| state.objects.get(object).controller == who)
        })
}

fn live_ability_source(ctx: &ExecCtx, state: &GameState) -> Option<ObjectId> {
    let contract = ctx.ability_source_contract?;
    let live = state.objects.try_get(contract.source)?;
    (live.zone == Zone::Battlefield && live.zone_change_count == contract.zone_change_count)
        .then_some(contract.source)
}

pub(crate) fn cast_x_for(state: &GameState, link: ObjectLinkV4) -> Option<u16> {
    read_state(state)?
        .cast_x_entries
        .iter()
        .find(|(entry, _)| *entry == link)
        .map(|(_, x)| *x)
}

fn record_cast_x(state: &mut GameState, link: ObjectLinkV4, x: u16) {
    let entries = &mut c8c_state(state).cast_x_entries;
    entries.retain(|(entry, _)| *entry != link);
    entries.push((link, x));
}

fn bind(state: &GameState, object: ObjectId) -> EffectObjectBinding {
    let live = state.objects.get(object);
    EffectObjectBinding {
        object,
        expected_zone: live.zone,
        expected_zone_change_count: live.zone_change_count,
    }
}

fn offer(
    continuation: &mut EffectContinuation,
    kind: C8cChoiceV1,
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
            kind: E3ChoiceKindV1::C8cV1(kind),
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

// ---- Hooks: Pyrogoyf CDA and LKI ----------------------------------------------------

/// Pyrogoyf: "Pyrogoyf's power is equal to the number of card types among
/// cards in all graveyards and its toughness is equal to that number plus 1."
/// A characteristic-defining ability functions in every zone (CR 604.3).
pub(crate) fn characteristic_defining_power(state: &GameState, id: ObjectId) -> Option<i32> {
    if definition_name(state, id)? != PYROGOYF {
        return None;
    }
    let object = state.objects.try_get(id)?;
    if object.zone == Zone::Battlefield
        && !crate::continuous_characteristics_v1::printed_abilities_active(state, id)
    {
        return None;
    }
    let cards = [PlayerId::P0, PlayerId::P1]
        .into_iter()
        .flat_map(|player| state.players[player.index()].graveyard.iter().copied())
        .collect::<Vec<_>>();
    i32::try_from(crate::eternal_cards_v1::card_type_count(state, &cards)).ok()
}

pub(crate) fn characteristic_defining_toughness(state: &GameState, id: ObjectId) -> Option<i32> {
    characteristic_defining_power(state, id).map(|power| power + 1)
}

fn pyrogoyf_watches(state: &GameState, link: ObjectLinkV4) -> bool {
    let matches_op = |op: &EffectOp| {
        matches!(
            op,
            EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::C8cV1(
                C8cOpV1::PyrogoyfDamageV1 { entering }
            ))) if *entering == link
        )
    };
    active_permanents(state, PYROGOYF, None).next().is_some()
        || state
            .stack
            .iter()
            .any(|item| item.inline_effect.as_ref().is_some_and(matches_op))
        || state
            .engine
            .pending_triggers
            .iter()
            .any(|pending| matches_op(&pending.effect))
        || state.engine.pending_effect.as_ref().is_some_and(|pending| {
            pending
                .resolving_item
                .inline_effect
                .as_ref()
                .is_some_and(matches_op)
        })
}

fn capture_lhurgoyf_departure(state: &mut GameState, object: ObjectId) {
    let Some(live) = state.objects.try_get(object) else {
        return;
    };
    if live.zone != Zone::Battlefield
        || !crate::engine::object_has_type(state, object, CardType::Creature)
        || !crate::engine::has_effective_subtype(state, object, Subtype::Lhurgoyf)
    {
        return;
    }
    let link = ObjectLinkV4 {
        object,
        zone_change_count: live.zone_change_count,
    };
    if !pyrogoyf_watches(state, link) {
        return;
    }
    let entry = DepartedLhurgoyfV1 {
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
    let departed = &mut c8c_state(state).departed_lhurgoyfs;
    if !departed.iter().any(|existing| existing.link == entry.link) {
        departed.push(entry);
    }
}

/// Captures LKI for departing Lhurgoyfs in a simultaneous zone-change batch
/// before any permanent in the batch leaves the battlefield.
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
        capture_lhurgoyf_departure(state, object);
    }
}

/// Captures LKI for a single departing Lhurgoyf before it leaves the
/// battlefield.
pub(crate) fn note_departure(state: &mut GameState, object: ObjectId) {
    if !cfg!(feature = "eternal-fixtures") {
        return;
    }
    capture_lhurgoyf_departure(state, object);
}

// ---- Hooks: Elite Spellbinder tax ---------------------------------------------------

fn is_spellbinder_exiled_cast(state: &GameState, source: ObjectId, player: PlayerId) -> bool {
    let Some(c8c) = read_state(state) else {
        return false;
    };
    if c8c.spellbinder_exiled.is_empty() {
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
    c8c.spellbinder_exiled.contains(&exile_link)
}

/// Elite Spellbinder: "A spell cast this way costs {2} more to cast."
pub(crate) fn spellbinder_spell_tax(state: &GameState, source: ObjectId, player: PlayerId) -> u32 {
    if cfg!(feature = "eternal-fixtures") && is_spellbinder_exiled_cast(state, source, player) {
        2
    } else {
        0
    }
}

// ---- Hooks: Wan Shi Tong, Librarian library searches --------------------------------

/// Records that `searcher` just searched their own library. Queues one
/// trigger per active Wan Shi Tong, Librarian controlled by `searcher`'s
/// opponent. Does nothing (and leaves `C8cStateV1` untouched) when no opponent
/// controls an active Librarian.
pub(crate) fn note_library_searched(state: &mut GameState, searcher: PlayerId) {
    if !cfg!(feature = "eternal-fixtures") {
        return;
    }
    let opponent = searcher.opponent();
    let librarians: Vec<ObjectId> =
        active_permanents(state, WAN_SHI_TONG_LIBRARIAN, Some(opponent)).collect();
    if librarians.is_empty() {
        return;
    }
    let queued: Vec<LibrarianSearchTriggerV1> = librarians
        .into_iter()
        .map(|librarian| LibrarianSearchTriggerV1 {
            source: AbilitySourceContractV4::capture(state, librarian),
            controller: opponent,
        })
        .collect();
    c8c_state(state).librarian_search_triggers.extend(queued);
}

// ---- Hooks: Thieving Skydiver Kicker {X} --------------------------------------------

/// Thieving Skydiver: "Kicker {X}. X can't be 0." When kicked, X must be at
/// least 1.
pub(crate) fn minimum_x(name: &str, state: &GameState) -> Option<u8> {
    if name != THIEVING_SKYDIVER {
        return None;
    }
    let kicked = state.engine.pending_cast.as_ref().is_some_and(|pending| {
        state
            .objects
            .try_get(pending.spell)
            .and_then(|object| CARD_DEFS.get(usize::from(object.card_def)))
            .is_some_and(|def| def.name == THIEVING_SKYDIVER)
            && pending.kicked == Some(true)
    });
    kicked.then_some(1)
}

// ---- Hooks: Zirda, the Dawnwaker ----------------------------------------------------

fn is_permanent_card(card: &CardDef) -> bool {
    [
        CardType::Artifact,
        CardType::Creature,
        CardType::Enchantment,
        CardType::Land,
        CardType::Planeswalker,
    ]
    .into_iter()
    .any(|card_type| card.has_type(card_type))
}

fn permanent_has_activated_ability(card: &CardDef) -> bool {
    // Sanctum Lurker and Avatar of Burgeoning Echoes store the loyalty
    // abilities they grant to planeswalkers in `activated_abilities`;
    // Urza's Saga gains its mana and Construct abilities from chapters I
    // and II rather than having them on the card.
    if matches!(
        card.name,
        crate::eternal_cube_c7c_v1::SANCTUM_LURKER | "Avatar of Burgeoning Echoes" | "Urza's Saga"
    ) {
        return false;
    }
    // Outside the battlefield, a double-faced card has only its front face's
    // characteristics (CR 712.8a). Companion and Foretell special-action slots
    // use `EffectOp::Sequence(Vec::new())` and are not activated abilities.
    if card.activated_abilities.iter().any(|ability| {
        ability.face.is_none_or(|face| face == 0)
            && ability.activation_zone != Zone::Exile
            && (ability.effect)() != EffectOp::Sequence(Vec::new())
    }) {
        return true;
    }
    let back_face_mana_only = matches!(
        card.name,
        "Ojer Axonil, Deepest Might" | "Thousand Moons Smithy"
    ) || crate::eternal_cards_v1::e3::has_land_back_face(card.name);
    if !back_face_mana_only
        && (card.has_mana_ability()
            || !card.mana_ability_choices.is_empty()
            || card.mana_ability_def.is_some()
            || !card.additional_mana_abilities.is_empty()
            || !card.restricted_mana_abilities.is_empty()
            || card.mana_ability_includes_chosen_color
            || (card.mana_ability)().is_some())
    {
        return true;
    }
    // Basic land subtypes grant intrinsic mana abilities (CR 305.6).
    card.subtypes.iter().any(|subtype| {
        matches!(
            subtype,
            Subtype::Plains
                | Subtype::Island
                | Subtype::Swamp
                | Subtype::Mountain
                | Subtype::Forest
        )
    })
}

/// Zirda, the Dawnwaker: "Companion -- Each permanent card in your starting
/// deck has an activated ability."
pub(crate) fn zirda_companion_condition_met(deck: &[u16]) -> bool {
    deck.iter().all(|&id| {
        CARD_DEFS
            .get(usize::from(id))
            .is_some_and(|card| !is_permanent_card(card) || permanent_has_activated_ability(card))
    })
}

fn is_special_or_mana_activation(
    state: &GameState,
    source: ObjectId,
    components: &[CostComponent],
) -> bool {
    if crate::eternal_lurrus_v1::is_companion_source(state, source) {
        return true;
    }
    let Some(live) = state.objects.try_get(source) else {
        return false;
    };
    let Some(def) = CARD_DEFS.get(usize::from(live.card_def)) else {
        return false;
    };
    def.activated_abilities
        .iter()
        .enumerate()
        .any(|(idx, ability)| {
            let Ok(index) = u8::try_from(idx) else {
                return false;
            };
            ability.activation_zone == live.zone
                && ability.cost == components
                && (crate::eternal_pauper_e6a_v1::is_foretell_action(state, source, index)
                    || crate::eternal_legacy_e7a_v1::is_special_activation(state, source, index)
                    || crate::eternal_legacy_e8a_v1::statics::is_special_activation(
                        state, source, index,
                    )
                    || (!ability.is_loyalty_ability()
                        && matches!(ability.target_spec, TargetSpec::None)
                        && matches!((ability.effect)(), EffectOp::AddMana { .. })))
        })
}

/// Zirda, the Dawnwaker: "Abilities you activate that aren't mana abilities
/// cost {2} less to activate. This effect can't reduce the mana in that cost
/// to less than one mana." Applies to non-mana activated abilities activated
/// by `player` in any zone (battlefield, hand, graveyard, exile), reducing
/// generic mana (including concrete X mana) by up to 2 per active Zirda while
/// never reducing total mana (colored/hybrid pips plus generic) below 1.
/// Skips special actions (Foretell, Companion, Suspend) and mana abilities
/// housed in `activated_abilities`.
pub(crate) fn reduce_nonmana_activation(
    state: &GameState,
    player: PlayerId,
    source: ObjectId,
    components: &[CostComponent],
    cost: Cost,
    x_value: u8,
) -> Cost {
    if !cfg!(feature = "eternal-fixtures")
        || is_special_or_mana_activation(state, source, components)
    {
        return cost;
    }
    let zirdas = active_permanents(state, ZIRDA, Some(player)).count();
    if zirdas == 0 {
        return cost;
    }
    let x_generic = u32::from(cost.x_count).saturating_mul(u32::from(x_value));
    let generic = u32::from(cost.generic).saturating_add(x_generic);
    let pips = u32::try_from(cost.pips.len()).unwrap_or(u32::MAX);
    let total = generic.saturating_add(pips);
    if generic == 0 || total <= 1 {
        return cost;
    }
    let reduction = u32::try_from(zirdas).unwrap_or(u32::MAX).saturating_mul(2);
    let max_reducible = (total - 1).min(generic);
    let reduced_generic = generic - reduction.min(max_reducible);
    if cost.x_count == 0 {
        Cost {
            pips: cost.pips,
            generic: u8::try_from(reduced_generic).unwrap_or(u8::MAX),
            x_count: 0,
        }
    } else if let Ok(concrete) = u8::try_from(reduced_generic) {
        Cost {
            pips: cost.pips,
            generic: concrete,
            x_count: 0,
        }
    } else {
        cost
    }
}

// ---- Hooks: Combat blocking restrictions --------------------------------------------

fn is_wan_shi_tong_spirit(state: &GameState, object: ObjectId) -> bool {
    active_named(state, object, WAN_SHI_TONG_SPIRIT_TOKEN)
}

/// Enforces Wave C8c combat blocking restrictions:
/// - Wan Shi Tong Spirit Token: "This token can't block or be blocked by
///   non-Spirit creatures."
/// - Zirda, the Dawnwaker: "Target creature can't block this turn."
pub(crate) fn blocker_allowed(state: &GameState, attacker: ObjectId, blocker: ObjectId) -> bool {
    if !cfg!(feature = "eternal-fixtures") {
        return true;
    }
    if is_wan_shi_tong_spirit(state, attacker)
        && !crate::engine::has_effective_subtype(state, blocker, Subtype::Spirit)
    {
        return false;
    }
    if is_wan_shi_tong_spirit(state, blocker)
        && !crate::engine::has_effective_subtype(state, attacker, Subtype::Spirit)
    {
        return false;
    }
    if let Some(c8c) = read_state(state) {
        if !c8c.cant_block_creatures.is_empty() {
            let turn = TurnV1::now(state);
            let blocker_link = live_link(state, blocker);
            if c8c
                .cant_block_creatures
                .iter()
                .any(|&(link, banned_turn)| link == blocker_link && banned_turn == turn)
            {
                return false;
            }
        }
    }
    true
}

// ---- Hooks: Absorbing Man copy layer & lifecycle ------------------------------------

fn absorbing_copy_index(state: &GameState, object: ObjectId) -> Option<usize> {
    let live = state.objects.try_get(object)?;
    if live.zone != Zone::Battlefield {
        return None;
    }
    let link = ObjectLinkV4 {
        object,
        zone_change_count: live.zone_change_count,
    };
    read_state(state)?
        .absorbing_copies
        .iter()
        .position(|copy| copy.host == link)
}

/// Whether `object` is currently on the battlefield under Absorbing Man's
/// copy effect (not overwritten by a newer temporary Saheeli copy).
pub(crate) fn is_absorbing_copy(state: &GameState, object: ObjectId) -> bool {
    cfg!(feature = "eternal-fixtures")
        && absorbing_copy_index(state, object).is_some()
        && !crate::eternal_cube_c8b_v1::has_saheeli_copy(state, object)
}

/// Whether `object` is an Absorbing Man copy that inherited a Layer 1 added
/// Artifact type from its copied target (e.g., Vaultborn Tyrant artifact
/// token, Saheeli -2 copy, or Phyrexian Metamorph).
pub(crate) fn absorbing_copy_adds_artifact(state: &GameState, object: ObjectId) -> bool {
    if !is_absorbing_copy(state, object) {
        return false;
    }
    let Some(index) = absorbing_copy_index(state, object) else {
        return false;
    };
    read_state(state).is_some_and(|c8c| c8c.absorbing_copies[index].added_artifact)
}

/// Alias used by `event.rs` departure handling.
pub(crate) fn active_copy(state: &GameState, object: ObjectId) -> bool {
    cfg!(feature = "eternal-fixtures") && absorbing_copy_index(state, object).is_some()
}

/// Absorbing Man copy exception: base power and toughness are 4/4.
pub(crate) fn absorbing_copy_base_pt(state: &GameState, object: ObjectId) -> Option<(i32, i32)> {
    is_absorbing_copy(state, object).then_some((4, 4))
}

/// Absorbing Man copy exception: Legendary in addition to copied supertypes.
pub(crate) fn absorbing_copy_supertypes(
    state: &GameState,
    object: ObjectId,
) -> Option<&'static [Supertype]> {
    if !is_absorbing_copy(state, object) {
        return None;
    }
    let live = state.objects.try_get(object)?;
    if live.v4.face_down_v1.is_some() {
        return None;
    }
    let def = CARD_DEFS.get(usize::from(live.card_def))?;
    let printed = if live.v4.face_index == 1 && def.name == "Ojer Axonil, Deepest Might" {
        &[][..]
    } else {
        def.supertypes
    };
    let has_basic = printed.contains(&Supertype::Basic);
    let has_snow = printed.contains(&Supertype::Snow);
    Some(match (has_basic, has_snow) {
        (true, true) => &[Supertype::Legendary, Supertype::Basic, Supertype::Snow],
        (true, false) => &[Supertype::Legendary, Supertype::Basic],
        (false, true) => &[Supertype::Legendary, Supertype::Snow],
        (false, false) => &[Supertype::Legendary],
    })
}

fn become_absorbing_copy(
    state: &mut GameState,
    host: ObjectId,
    target: ObjectId,
    until_turn_of: PlayerId,
) {
    let target_live = state.objects.get(target);
    if target_live.v4.face_down_v1.is_some() {
        return;
    }
    let copied_def = target_live.card_def;
    let copied_face = target_live.v4.face_index;
    let copied_card = &CARD_DEFS[usize::from(copied_def)];
    let copied_color_mask = mana_colors_mask(copied_card.colors_for_face(copied_face));
    let copied_adds_artifact = crate::eternal_cube_c8b_v1::adds_artifact(state, target)
        || crate::eternal_vintage_v1::active_copy(state, target)
        || absorbing_copy_adds_artifact(state, target);
    let mut copied_subtypes: Vec<u16> = copied_card
        .subtypes_for_face(copied_face)
        .iter()
        .map(|subtype| subtype.stable_id())
        .collect();
    if copied_card.changeling && copied_face == 0 {
        copied_subtypes.extend(
            Subtype::CREATURE_TYPES
                .iter()
                .map(|subtype| subtype.stable_id()),
        );
    }
    copied_subtypes.extend(crate::eternal_cube_c8b_v1::copy_added_subtypes(
        state, target,
    ));
    copied_subtypes.push(Subtype::Human.stable_id());
    copied_subtypes.push(Subtype::Villain.stable_id());
    copied_subtypes.sort_unstable();
    copied_subtypes.dedup();
    let copied_ward = ObjectStateV4::from_card_def(copied_def).ward_generic;

    let link = live_link(state, host);
    let own_def = state.objects.get(host).card_def;
    let existing = absorbing_copy_index(state, host);
    if let Some(index) = existing {
        let entry = &mut c8c_state(state).absorbing_copies[index];
        entry.until_turn_of = until_turn_of;
        entry.added_artifact = copied_adds_artifact;
    } else {
        let live = state.objects.get_mut(host);
        let record = AbsorbingCopyV1 {
            host: link,
            until_turn_of,
            orig_card_def: live.card_def,
            orig_name: std::mem::take(&mut live.name),
            orig_color_mask: live.v4.effective_color_mask,
            orig_subtype_ids: std::mem::take(&mut live.v4.effective_subtype_ids),
            orig_face_index: live.v4.face_index,
            orig_ward_generic: live.v4.ward_generic,
            added_artifact: copied_adds_artifact,
        };
        c8c_state(state).absorbing_copies.push(record);
    }
    let live = state.objects.get_mut(host);
    live.card_def = copied_def;
    live.name = ABSORBING_MAN.to_owned();
    live.v4.face_index = copied_face;
    live.v4.effective_color_mask = copied_color_mask;
    live.v4.effective_subtype_ids = copied_subtypes;
    live.v4.ward_generic = copied_ward;
    crate::standard_cards_v1::note_copied_definitions(
        state,
        host,
        link.zone_change_count,
        &[own_def, copied_def],
    );
}

fn restore_absorbing_copy(state: &mut GameState, record: AbsorbingCopyV1) {
    if state
        .objects
        .try_get(record.host.object)
        .is_none_or(|live| {
            live.zone != Zone::Battlefield
                || live.zone_change_count != record.host.zone_change_count
        })
    {
        return;
    }
    let live = state.objects.get_mut(record.host.object);
    live.card_def = record.orig_card_def;
    live.name = record.orig_name;
    live.v4.effective_color_mask = record.orig_color_mask;
    live.v4.effective_subtype_ids = record.orig_subtype_ids;
    live.v4.face_index = record.orig_face_index;
    live.v4.ward_generic = record.orig_ward_generic;
}

/// Ends an active Absorbing Man copy immediately before it leaves the
/// battlefield so it arrives in its destination zone as Absorbing Man.
pub(crate) fn end_copy_before_departure(state: &mut GameState, object: ObjectId) {
    if !cfg!(feature = "eternal-fixtures") {
        return;
    }
    let Some(index) = absorbing_copy_index(state, object) else {
        return;
    };
    let record = c8c_state(state).absorbing_copies.remove(index);
    restore_absorbing_copy(state, record);
}

/// Ends Absorbing Man copies lasting "until your next turn" at the start of
/// `player`'s turn (Untap step, before Main1 so Absorbing Man's own
/// precombat main trigger fires again).
pub(crate) fn begin_turn(state: &mut GameState, player: PlayerId) {
    if !cfg!(feature = "eternal-fixtures") {
        return;
    }
    let Some(c8c) = read_state(state) else {
        return;
    };
    if c8c.is_empty() {
        return;
    }
    if !c8c.absorbing_copies.is_empty() {
        let expiring: Vec<AbsorbingCopyV1> = {
            let copies = &mut c8c_state(state).absorbing_copies;
            let mut out = Vec::new();
            let mut i = 0;
            while i < copies.len() {
                if copies[i].until_turn_of == player {
                    out.push(copies.remove(i));
                } else {
                    i += 1;
                }
            }
            out
        };
        for record in expiring {
            restore_absorbing_copy(state, record);
        }
    }
    prune(state);
}

// ---- Hooks: Delayed triggers & pruning ----------------------------------------------

/// Queues Pyrogoyf's Lhurgoyf-entry triggers and drains Wan Shi Tong,
/// Librarian's opponent-search triggers, then prunes expired state.
pub(crate) fn delayed_triggers(
    state: &mut GameState,
    events: &[CommittedEvent],
) -> Vec<PendingTrigger> {
    if !cfg!(feature = "eternal-fixtures") {
        return Vec::new();
    }
    let mut triggers = pyrogoyf_triggers(state, events);
    if read_state(state).is_some_and(|c8c| !c8c.librarian_search_triggers.is_empty()) {
        let pending = std::mem::take(&mut c8c_state(state).librarian_search_triggers);
        for entry in pending {
            triggers.push(crate::eternal_cards_v1::delayed_trigger(
                entry.source,
                entry.controller,
                librarian_search_trigger(),
            ));
        }
    }
    prune(state);
    triggers
}

fn pyrogoyf_triggers(state: &GameState, events: &[CommittedEvent]) -> Vec<PendingTrigger> {
    if active_permanents(state, PYROGOYF, None).next().is_none() {
        return Vec::new();
    }
    let mut triggers = Vec::new();
    for event in events {
        let entering = match *event {
            CommittedEvent::ZoneChange {
                object,
                to: Zone::Battlefield,
                ..
            } => object,
            CommittedEvent::CreateToken { object, .. } => object,
            _ => continue,
        };
        if crate::eternal_vintage_e7b_v1::statics::entry_event_suppressed(state, event) {
            continue;
        }
        let Some((entering_link, entering_controller)) = entering_lhurgoyf_info(state, entering)
        else {
            continue;
        };
        for pyrogoyf in active_permanents(state, PYROGOYF, Some(entering_controller)) {
            let mut pending = crate::eternal_cards_v1::delayed_trigger(
                AbilitySourceContractV4::capture(state, pyrogoyf),
                entering_controller,
                pyrogoyf_damage(entering_link),
            );
            pending.target_spec = TargetSpec::AnyTarget;
            triggers.push(pending);
        }
    }
    triggers
}

fn entering_lhurgoyf_info(
    state: &GameState,
    entering: ObjectId,
) -> Option<(ObjectLinkV4, PlayerId)> {
    let live = state.objects.try_get(entering)?;
    if live.zone == Zone::Battlefield {
        if crate::engine::object_has_type(state, entering, CardType::Creature)
            && crate::engine::has_effective_subtype(state, entering, Subtype::Lhurgoyf)
        {
            return Some((
                ObjectLinkV4 {
                    object: entering,
                    zone_change_count: live.zone_change_count,
                },
                live.controller,
            ));
        }
        return None;
    }
    read_state(state)?
        .departed_lhurgoyfs
        .iter()
        .rev()
        .find(|entry| entry.link.object == entering)
        .map(|entry| (entry.link, entry.facts.source.controller))
}

fn prune(state: &mut GameState) {
    let Some(c8c) = read_state(state) else {
        return;
    };
    if c8c.is_empty() {
        return;
    }
    let turn = TurnV1::now(state);
    let keep_departed: Vec<bool> = c8c
        .departed_lhurgoyfs
        .iter()
        .map(|entry| entry.turn == turn || pyrogoyf_watches(state, entry.link))
        .collect();
    let keep_spellbinder: Vec<bool> = c8c
        .spellbinder_exiled
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
    let keep_cast_x: Vec<bool> = c8c
        .cast_x_entries
        .iter()
        .map(|&(link, _)| link_live_in(state, link, Zone::Battlefield) || source_referenced(link))
        .collect();
    let keep_cant_block: Vec<bool> = c8c
        .cant_block_creatures
        .iter()
        .map(|&(link, banned_turn)| {
            banned_turn == turn && link_live_in(state, link, Zone::Battlefield)
        })
        .collect();
    let keep_absorbing: Vec<bool> = c8c
        .absorbing_copies
        .iter()
        .map(|copy| link_live_in(state, copy.host, Zone::Battlefield))
        .collect();

    let c8c = c8c_state(state);
    let mut idx = 0;
    c8c.departed_lhurgoyfs.retain(|_| {
        let keep = keep_departed[idx];
        idx += 1;
        keep
    });
    idx = 0;
    c8c.spellbinder_exiled.retain(|_| {
        let keep = keep_spellbinder[idx];
        idx += 1;
        keep
    });
    idx = 0;
    c8c.cast_x_entries.retain(|_| {
        let keep = keep_cast_x[idx];
        idx += 1;
        keep
    });
    idx = 0;
    c8c.cant_block_creatures.retain(|_| {
        let keep = keep_cant_block[idx];
        idx += 1;
        keep
    });
    idx = 0;
    c8c.absorbing_copies.retain(|_| {
        let keep = keep_absorbing[idx];
        idx += 1;
        keep
    });
}

// ---- Execution ----------------------------------------------------------------------

/// Runs one of this module's synchronous leaves.
pub(crate) fn execute(op: &C8cOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        C8cOpV1::PyrogoyfDamageV1 { entering } => {
            let Some(target) = ctx.targets.first().copied() else {
                return;
            };
            if !ctx.target_incarnation_matches(0, state)
                || !ctx.ability_source_contract.is_some_and(|source| {
                    crate::engine::effect_target_is_legal_from_ability_source(
                        state,
                        source,
                        ctx.controller,
                        TargetSpec::AnyTarget,
                        &ctx.targets,
                        0,
                    )
                })
            {
                return;
            }
            let (amount, historical_facts) = if link_live_in(state, *entering, Zone::Battlefield) {
                (crate::engine::effective_power(state, entering.object), None)
            } else {
                let Some(lki) = read_state(state).and_then(|c8c| {
                    c8c.departed_lhurgoyfs
                        .iter()
                        .find(|entry| entry.link == *entering)
                        .copied()
                }) else {
                    return;
                };
                (lki.power, Some(lki.facts))
            };
            if amount <= 0 {
                return;
            }
            let mut proposal = ProposedEvent::damage(entering.object, target, amount);
            if let (ProposedEvent::Damage(damage), Some(facts)) = (&mut proposal, historical_facts)
            {
                damage.eternal_damage_lki_v1 =
                    Some(crate::effect::EternalDamageLkiV1::for_c8c(facts));
            }
            event::propose_and_commit(state, proposal);
        }
        C8cOpV1::LibrarianEnterV1 => {
            if state
                .objects
                .try_get(ctx.source)
                .is_none_or(|live| live.zone != Zone::Stack)
            {
                return;
            }
            crate::effect::execute(
                &EffectOp::MoveObject {
                    object: ObjectRef::ThisSource,
                    to_zone: Zone::Battlefield,
                },
                ctx,
                state,
            );
            if ctx.x_value > 0
                && state
                    .objects
                    .try_get(ctx.source)
                    .is_some_and(|live| live.zone == Zone::Battlefield)
            {
                let link = live_link(state, ctx.source);
                record_cast_x(state, link, ctx.x_value);
            }
        }
        C8cOpV1::SkydiverEnterV1 => {
            if state
                .objects
                .try_get(ctx.source)
                .is_none_or(|live| live.zone != Zone::Stack)
            {
                return;
            }
            crate::effect::execute(
                &EffectOp::MoveObject {
                    object: ObjectRef::ThisSource,
                    to_zone: Zone::Battlefield,
                },
                ctx,
                state,
            );
            if ctx.kicked
                && ctx.x_value > 0
                && state
                    .objects
                    .try_get(ctx.source)
                    .is_some_and(|live| live.zone == Zone::Battlefield)
            {
                let link = live_link(state, ctx.source);
                record_cast_x(state, link, ctx.x_value);
            }
        }
        C8cOpV1::SkydiverEtbV1 => {
            let Some(Target::Object(artifact)) = ctx.targets.first().copied() else {
                return;
            };
            if !ctx.target_incarnation_matches(0, state)
                || !ctx.ability_source_contract.is_some_and(|source| {
                    crate::engine::effect_target_is_legal_from_ability_source(
                        state,
                        source,
                        ctx.controller,
                        SKYDIVER_TARGET,
                        &ctx.targets,
                        0,
                    )
                })
            {
                return;
            }
            crate::standard_cards_v1::gain_control(state, artifact, ctx.controller);
            if crate::engine::has_effective_subtype(state, artifact, Subtype::Equipment)
                && !crate::engine::object_has_type(state, artifact, CardType::Creature)
            {
                if let Some(skydiver) = live_ability_source(ctx, state) {
                    if crate::engine::object_has_type(state, skydiver, CardType::Creature)
                        && !crate::engine::damage_is_prevented_by_protection(
                            state,
                            artifact,
                            Target::Object(skydiver),
                        )
                        && !crate::progenitus_v1::attachment_forbidden(state, artifact, skydiver)
                    {
                        let artifact_zcc = state.objects.get(artifact).zone_change_count;
                        let skydiver_zcc = state.objects.get(skydiver).zone_change_count;
                        let _ = state.attach_object_exact(
                            artifact,
                            artifact_zcc,
                            skydiver,
                            skydiver_zcc,
                        );
                    }
                }
            }
        }
        C8cOpV1::ZirdaCantBlockV1 => {
            let Some(Target::Object(creature)) = ctx.targets.first().copied() else {
                return;
            };
            if !ctx.target_incarnation_matches(0, state)
                || !ctx.ability_source_contract.is_some_and(|source| {
                    crate::engine::effect_target_is_legal_from_ability_source(
                        state,
                        source,
                        ctx.controller,
                        TargetSpec::Creature,
                        &ctx.targets,
                        0,
                    )
                })
            {
                return;
            }
            let entry = (live_link(state, creature), TurnV1::now(state));
            let list = &mut c8c_state(state).cant_block_creatures;
            if !list.contains(&entry) {
                list.push(entry);
            }
        }
        C8cOpV1::AbsorbingManCopyV1 => {
            let Some(Target::Object(target)) = ctx.targets.first().copied() else {
                return;
            };
            if !ctx.target_incarnation_matches(0, state)
                || !ctx.ability_source_contract.is_some_and(|source| {
                    crate::engine::effect_target_is_legal_from_ability_source(
                        state,
                        source,
                        ctx.controller,
                        ABSORBING_MAN_TARGET,
                        &ctx.targets,
                        0,
                    )
                })
            {
                return;
            }
            let Some(host) = live_ability_source(ctx, state) else {
                return;
            };
            become_absorbing_copy(state, host, target, ctx.controller);
        }
        C8cOpV1::EliteSpellbinderEtbV1
        | C8cOpV1::LibrarianEtbV1
        | C8cOpV1::LibrarianSearchTriggerV1 => {
            debug_assert!(false, "choice/draw C8c leaf reached synchronous execute");
        }
    }
}

/// Runs one of this module's leaves in the resumable interpreter.
pub(crate) fn drive(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    op: &C8cOpV1,
    path: Vec<u16>,
) -> Result<bool, String> {
    match op {
        C8cOpV1::EliteSpellbinderEtbV1 => {
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
            let hand = state.players[opponent.index()].hand.clone();
            for &card in &hand {
                // spellbench-reviewed-look: Elite Spellbinder lets its
                // controller look at target opponent's hand.
                state.reveal_distinct_hand_card(controller, opponent, card)?;
            }
            let cards: Vec<EffectObjectBinding> = hand
                .into_iter()
                .filter(|&card| {
                    state
                        .objects
                        .try_get(card)
                        .and_then(|live| CARD_DEFS.get(usize::from(live.card_def)))
                        .is_some_and(|def| !def.is_land)
                })
                .map(|card| bind(state, card))
                .collect();
            if cards.is_empty() {
                return Ok(false);
            }
            offer(
                continuation,
                C8cChoiceV1::SpellbinderExileV1,
                controller,
                cards,
                0,
                1,
                path,
            );
            Ok(true)
        }
        C8cOpV1::LibrarianEtbV1 => {
            let x = continuation
                .ctx
                .ability_source_contract
                .and_then(|contract| {
                    cast_x_for(
                        state,
                        ObjectLinkV4 {
                            object: contract.source,
                            zone_change_count: contract.zone_change_count,
                        },
                    )
                })
                .unwrap_or(0);
            if x > 0 {
                if let Some(librarian) = live_ability_source(&continuation.ctx, state) {
                    event::add_plus_one_counters(
                        state,
                        librarian,
                        continuation.ctx.controller,
                        i32::from(x),
                    )
                    .map_err(|error| error.to_string())?;
                }
            }
            let draws = u32::from(x / 2);
            if draws == 0 {
                return Ok(false);
            }
            crate::eternal_vintage_e7b_v1::dredge::drive_draws(
                state,
                continuation,
                PlayerRef::Controller,
                draws,
                path,
            )
        }
        C8cOpV1::LibrarianSearchTriggerV1 => {
            if let Some(librarian) = live_ability_source(&continuation.ctx, state) {
                event::add_plus_one_counters(state, librarian, continuation.ctx.controller, 1)
                    .map_err(|error| error.to_string())?;
            }
            crate::eternal_vintage_e7b_v1::dredge::drive_draws(
                state,
                continuation,
                PlayerRef::Controller,
                1,
                path,
            )
        }
        _ => {
            execute(op, &continuation.ctx, state);
            Ok(false)
        }
    }
}

/// Applies an answered C8c resolution choice.
pub(crate) fn finish_selection(
    state: &mut GameState,
    _continuation: &mut EffectContinuation,
    _choice: &E3ChoiceV1,
    kind: &C8cChoiceV1,
    bindings: Vec<EffectObjectBinding>,
) -> Result<bool, String> {
    match kind {
        C8cChoiceV1::SpellbinderExileV1 => {
            let Some(chosen) = bindings.first().copied() else {
                return Ok(false);
            };
            let Some(live) = state.objects.try_get(chosen.object) else {
                return Ok(false);
            };
            if live.zone != Zone::Hand
                || live.zone_change_count != chosen.expected_zone_change_count
            {
                return Ok(false);
            }
            let owner = live.owner;
            event::propose_and_commit(
                state,
                ProposedEvent::zone_change(chosen.object, Zone::Exile),
            );
            let Some(after) = state.objects.try_get(chosen.object) else {
                return Ok(false);
            };
            if after.zone != Zone::Exile {
                return Ok(false);
            }
            let def = &CARD_DEFS[usize::from(after.card_def)];
            let play_or_cast = if def.is_playable_land() {
                Some(PlayOrCast::Play)
            } else if def.is_castable() {
                Some(PlayOrCast::Cast)
            } else {
                None
            };
            if let Some(play_or_cast) = play_or_cast {
                let exile_link = ObjectLinkV4 {
                    object: chosen.object,
                    zone_change_count: after.zone_change_count,
                };
                state.engine.exile_play_permissions.push(PlayPermission {
                    without_mana_cost: FreeCastV1(false),
                    any_mana_type: AnyManaTypeV1(false),
                    object: chosen.object,
                    holder: owner,
                    zone_change_generation: exile_link.zone_change_count,
                    play_or_cast,
                    expiry: PlayPermissionExpiry::AsLongAsExiled,
                });
                let list = &mut c8c_state(state).spellbinder_exiled;
                if !list.contains(&exile_link) {
                    list.push(exile_link);
                }
            }
            Ok(false)
        }
    }
}
