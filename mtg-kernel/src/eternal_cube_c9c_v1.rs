//! Card programs for the eternal catalog's Vintage Cube wave C9c
//! (`docs/design/eternal_formats_support_v1.md`): Elspeth, Storm Slayer;
//! Grist, the Hunger Tide (+ Black Green Insect Token); Nissa, Who Shakes the
//! World; Tamiyo, Collector of Tales; and Through the Breach.
//!
//! `eternal_cards_v1` forwards this module's effect leaves, CDA, delayed
//! triggers, and turn bookkeeping here, and the recipe tables of
//! `build_eternal_c9c_v1.rs` name its programs. Every table is keyed by a
//! printed name that occurs only in the eternal registry, so the Pauper, FDN,
//! and Standard catalogs never reach it. Behavior was read from each card's
//! current Oracle text.
//!
//! Where each card lives:
//!
//! - **Elspeth, Storm Slayer**: token-doubling replacement
//!   (`token_doubler_count` / `expand_doubled_tokens` hooked in `event.rs`),
//!   `+1` Soldier token (`ElspethPlusOneV1`), `0` +1/+1 counters and flying
//!   until controller's next turn (`ElspethZeroV1` via
//!   `until_next_turn_keywords`), and `-3` destroy target opponent creature
//!   with mana value 3 or greater (`ElspethMinusThreeV1` +
//!   `C9cTargetV1::OpponentCreatureManaValueThreeOrGreaterV1`).
//! - **Grist, the Hunger Tide** & **Black Green Insect Token**: CR 604.3
//!   non-battlefield CDA (`is_nonbattlefield_grist` /
//!   `is_nonbattlefield_creature_def` / `characteristic_defining_power` /
//!   `characteristic_defining_toughness`), `+1` repeating Insect-token +
//!   mill-and-loyalty loop (`GristPlusOneV1`), `-2` optional creature
//!   sacrifice with reflexive destroy trigger (`GristMinusTwoV1` +
//!   `C9cChoiceV1::GristSacrificeV1` + `GristMinusTwoDestroyV1`), and `-5`
//!   opponent life loss per creature card in controller's graveyard
//!   (`GristMinusFiveV1`).
//! - **Nissa, Who Shakes the World**: triggered mana ability on tapping any
//!   controlled Forest for mana (`after_tapped_for_mana`), `+1` three +1/+1
//!   counters, untap, and permanent 0/0 Elemental Vigilance Haste land
//!   animation (`NissaPlusOneV1` + `C9cTargetV1::UpToOneControlledNoncreatureLandV1`),
//!   and `-8` indestructible-lands emblem + resumable library search for any
//!   number of Forest cards onto the battlefield tapped (`NissaMinusEightV1` +
//!   `C9cChoiceV1::NissaSearchForestsV1`).
//! - **Tamiyo, Collector of Tales**: opponent-effect discard/sacrifice shield
//!   (`opponent_effect_cant_cause_discard_or_sacrifice`), `+1` nonland
//!   card-name choice and reveal top 4 (`tamiyo_plus_one` +
//!   `TamiyoPlusOneNamedV1`), and `-3` return target card from own graveyard
//!   to hand (`TamiyoMinusThreeV1`).
//! - **Through the Breach**: optional creature from hand onto the battlefield
//!   with haste and delayed end-step sacrifice (`ThroughTheBreachV1` +
//!   `C9cChoiceV1::ThroughTheBreachV1` + `BreachSacrificeV1`), plus
//!   generalized Splice onto Arcane `{2}{R}{R}` coordinated with
//!   `eternal_pauper_e8b_v1`.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

pub mod targets;

use crate::card_def::{
    card_id_by_name, CardDef, CardType, Keywords, Subtype, TargetSpec, CARD_DEFS,
};
use crate::effect::{
    EffectContinuation, EffectObjectBinding, EffectOp, ExecCtx, ObjectRef, PlayerRef,
};
use crate::engine::UntilNextTurnKeywordEffectV1;
use crate::eternal_cards_v1::e3::{E3ChoiceKindV1, E3ChoiceV1};
use crate::eternal_cards_v1::EternalOpV1;
use crate::event::{self, CommittedEvent, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::mana::ManaColor;
use crate::standard_cards_v1::{StandardOpV1, StandardTargetV1};
use crate::state::{AbilitySourceContractV4, GameState, ObjectLinkV4, Target, Zone};
use crate::trigger::{PendingTrigger, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};
use targets::C9cTargetV1;

pub(crate) const ELSPETH: &str = "Elspeth, Storm Slayer";
pub(crate) const GRIST: &str = "Grist, the Hunger Tide";
pub(crate) const NISSA: &str = "Nissa, Who Shakes the World";
pub(crate) const TAMIYO: &str = "Tamiyo, Collector of Tales";
pub(crate) const BLACK_GREEN_INSECT_TOKEN: &str = "Black Green Insect Token";
pub(crate) const SOLDIER_TOKEN: &str = "Soldier Token";

pub(crate) const ELSPETH_MINUS_THREE_TARGET: TargetSpec = TargetSpec::StandardV1(
    StandardTargetV1::C9cV1(C9cTargetV1::OpponentCreatureManaValueThreeOrGreaterV1),
);

pub(crate) const NISSA_PLUS_ONE_TARGET: TargetSpec = TargetSpec::StandardV1(
    StandardTargetV1::C9cV1(C9cTargetV1::UpToOneControlledNoncreatureLandV1),
);

const TAMIYO_MINUS_THREE_TARGET: TargetSpec =
    TargetSpec::StandardV1(StandardTargetV1::CardInOwnGraveyardV1);

const MAX_DOUBLED_TOKENS_PER_EVENT: usize = 1024;
const MAX_GRIST_LOOP_ITERATIONS: usize = 1024;

// ---- State --------------------------------------------------------------------------

/// A land permanently animated by Nissa, Who Shakes the World's `+1` into a
/// 0/0 Elemental creature with vigilance and haste that is still a land.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NissaAnimatedLandV1 {
    pub land: ObjectLinkV4,
    pub timestamp: u64,
}

fn is_no_nissa_emblem(emblems: &[Option<u64>; 2]) -> bool {
    emblems[0].is_none() && emblems[1].is_none()
}

/// Per-game state owned by wave C9c, kept inside
/// `eternal_cards_v1::EternalStateV1`. Absent from serialization and hashes to
/// zero bytes while empty so every game without these cards keeps its exact
/// snapshot and hash.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct C9cStateV1 {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nissa_animated_lands: Vec<NissaAnimatedLandV1>,
    #[serde(default, skip_serializing_if = "is_no_nissa_emblem")]
    pub nissa_emblems: [Option<u64>; 2],
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub breach_haste: Vec<(ObjectLinkV4, u64)>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub breach_sacrifices: Vec<(ObjectLinkV4, PlayerId)>,
}

impl C9cStateV1 {
    pub fn is_empty(&self) -> bool {
        self.nissa_animated_lands.is_empty()
            && is_no_nissa_emblem(&self.nissa_emblems)
            && self.breach_haste.is_empty()
            && self.breach_sacrifices.is_empty()
    }
}

/// An empty state hashes to zero bytes so every game without these cards keeps
/// its exact state hash.
impl std::hash::Hash for C9cStateV1 {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        if !self.is_empty() {
            b"eternal-cube-c9c/v1".hash(state);
            self.nissa_animated_lands.hash(state);
            self.nissa_emblems.hash(state);
            self.breach_haste.hash(state);
            self.breach_sacrifices.hash(state);
        }
    }
}

fn c9c_state(state: &mut GameState) -> &mut C9cStateV1 {
    &mut state.eternal_v1.get_or_insert_with(Default::default).c9c
}

fn read_state(state: &GameState) -> Option<&C9cStateV1> {
    state.eternal_v1.as_ref().map(|eternal| &eternal.c9c)
}

// ---- Ops and choices ----------------------------------------------------------------

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::C9cV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum C9cOpV1 {
    /// Elspeth, Storm Slayer (+1): Create a 1/1 white Soldier creature token.
    ElspethPlusOneV1,
    /// Elspeth, Storm Slayer (0): Put a +1/+1 counter on each creature you
    /// control. Those creatures gain flying until your next turn.
    ElspethZeroV1,
    /// Elspeth, Storm Slayer (-3): Destroy target creature an opponent
    /// controls with mana value 3 or greater.
    ElspethMinusThreeV1,
    /// Grist, the Hunger Tide (+1): Create a 1/1 black and green Insect
    /// creature token, then mill a card. If an Insect card was milled this
    /// way, put a loyalty counter on Grist and repeat this process.
    GristPlusOneV1,
    /// Grist, the Hunger Tide (-2): You may sacrifice a creature. When you do,
    /// destroy target creature or planeswalker.
    GristMinusTwoV1,
    /// Grist, the Hunger Tide (-2) reflexive trigger: Destroy target creature
    /// or planeswalker.
    GristMinusTwoDestroyV1,
    /// Grist, the Hunger Tide (-5): Each opponent loses life equal to the
    /// number of creature cards in your graveyard.
    GristMinusFiveV1,
    /// Nissa, Who Shakes the World (+1): Put three +1/+1 counters on up to one
    /// target noncreature land you control. Untap it. It becomes a 0/0
    /// Elemental creature with vigilance and haste that's still a land.
    NissaPlusOneV1,
    /// Nissa, Who Shakes the World (-8): You get an emblem with "Lands you
    /// control have indestructible." Search your library for any number of
    /// Forest cards, put them onto the battlefield tapped, then shuffle.
    NissaMinusEightV1,
    /// Tamiyo, Collector of Tales (+1): Reveal the top four cards of your
    /// library after choosing nonland card name `named`. Put all cards with
    /// the chosen name from among them into your hand and the rest into your
    /// graveyard.
    TamiyoPlusOneNamedV1 { named: u16 },
    /// Tamiyo, Collector of Tales (-3): Return target card from your graveyard
    /// to your hand.
    TamiyoMinusThreeV1,
    /// Through the Breach: You may put a creature card from your hand onto the
    /// battlefield. That creature gains haste. Sacrifice that creature at the
    /// beginning of the next end step.
    ThroughTheBreachV1,
    /// Through the Breach delayed end-step trigger: Sacrifice `creature`.
    BreachSacrificeV1 { creature: ObjectLinkV4 },
}

impl C9cOpV1 {
    /// Whether the leaf may stop for a player's choice (or a draw that a
    /// replacement can interrupt), so it runs in the resumable interpreter.
    pub(crate) fn contains_player_choice(&self) -> bool {
        matches!(
            self,
            Self::GristMinusTwoV1 | Self::NissaMinusEightV1 | Self::ThroughTheBreachV1
        )
    }
}

/// Resolution choices owned by this module, carried as
/// `E3ChoiceKindV1::C9cV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum C9cChoiceV1 {
    /// Grist, the Hunger Tide (-2): optionally sacrifice a controlled creature.
    GristSacrificeV1,
    /// Nissa, Who Shakes the World (-8): search library for any number of
    /// Forest cards to put onto the battlefield tapped.
    NissaSearchForestsV1,
    /// Through the Breach: optionally put a creature card from hand onto the
    /// battlefield.
    ThroughTheBreachV1,
}

impl C9cChoiceV1 {
    pub(crate) fn chooser_private(&self) -> bool {
        match self {
            Self::GristSacrificeV1 => false,
            Self::NissaSearchForestsV1 | Self::ThroughTheBreachV1 => true,
        }
    }

    pub(crate) fn library_search(&self) -> bool {
        matches!(self, Self::NissaSearchForestsV1)
    }

    pub(crate) fn public_purpose(&self) -> crate::rl::TargetSelectionPurposeV4 {
        match self {
            Self::GristSacrificeV1 => crate::rl::TargetSelectionPurposeV4::PermanentSelection,
            Self::NissaSearchForestsV1 | Self::ThroughTheBreachV1 => {
                crate::rl::TargetSelectionPurposeV4::CardSelection
            }
        }
    }

    pub(crate) fn referenced_bindings(&self) -> Vec<EffectObjectBinding> {
        Vec::new()
    }
}

fn c9c(op: C9cOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::C9cV1(op)))
}

// ---- Card programs ------------------------------------------------------------------

/// Standard planeswalker permanent entry program.
pub fn planeswalker_enter() -> EffectOp {
    EffectOp::MoveObject {
        object: ObjectRef::ThisSource,
        to_zone: Zone::Battlefield,
    }
}

pub fn elspeth_plus_one() -> EffectOp {
    c9c(C9cOpV1::ElspethPlusOneV1)
}

pub fn elspeth_zero() -> EffectOp {
    c9c(C9cOpV1::ElspethZeroV1)
}

pub fn elspeth_minus_three() -> EffectOp {
    c9c(C9cOpV1::ElspethMinusThreeV1)
}

pub fn grist_plus_one() -> EffectOp {
    c9c(C9cOpV1::GristPlusOneV1)
}

pub fn grist_minus_two() -> EffectOp {
    c9c(C9cOpV1::GristMinusTwoV1)
}

fn grist_minus_two_destroy() -> EffectOp {
    c9c(C9cOpV1::GristMinusTwoDestroyV1)
}

pub fn grist_minus_five() -> EffectOp {
    c9c(C9cOpV1::GristMinusFiveV1)
}

pub fn nissa_plus_one() -> EffectOp {
    c9c(C9cOpV1::NissaPlusOneV1)
}

pub fn nissa_minus_eight() -> EffectOp {
    c9c(C9cOpV1::NissaMinusEightV1)
}

/// Tamiyo, Collector of Tales (+1): "Choose a nonland card name, then reveal
/// the top four cards of your library. Put all cards with the chosen name from
/// among them into your hand and the rest into your graveyard."
pub fn tamiyo_plus_one() -> EffectOp {
    EffectOp::Choice {
        controller: PlayerRef::Controller,
        options: crate::eternal_sideboard_s1b_v1::nameable_nonland()
            .iter()
            .map(|ids| EffectOp::Choice {
                controller: PlayerRef::Controller,
                options: ids
                    .iter()
                    .map(|&named| c9c(C9cOpV1::TamiyoPlusOneNamedV1 { named }))
                    .collect(),
            })
            .collect(),
    }
}

pub fn tamiyo_minus_three() -> EffectOp {
    c9c(C9cOpV1::TamiyoMinusThreeV1)
}

/// Through the Breach: "You may put a creature card from your hand onto the
/// battlefield. That creature gains haste. Sacrifice that creature at the
/// beginning of the next end step." Followed by spliced Arcane effects.
pub fn through_the_breach() -> EffectOp {
    EffectOp::Sequence(vec![
        c9c(C9cOpV1::ThroughTheBreachV1),
        crate::eternal_pauper_e8b_v1::spliced_effects(),
    ])
}

/// One spliced Through the Breach effect added onto another Arcane spell.
pub(crate) fn spliced_through_the_breach() -> EffectOp {
    c9c(C9cOpV1::ThroughTheBreachV1)
}

fn breach_sacrifice(creature: ObjectLinkV4) -> EffectOp {
    c9c(C9cOpV1::BreachSacrificeV1 { creature })
}

// ---- Triggered abilities ------------------------------------------------------------

/// Triggered abilities of this module's cards, falling through to wave Pf2a.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    crate::eternal_pauper_pf2a_v1::triggers_for(name)
}

/// Target specs of this module's printed and queued triggered abilities.
pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    if name == GRIST && *effect == grist_minus_two_destroy() {
        return Some(TargetSpec::CreatureOrPlaneswalker);
    }
    delayed_effect_matches(name, effect).then_some(TargetSpec::None)
}

/// Queued triggers owned by this module's cards: Grist's `-2` reflexive
/// destroy trigger and Through the Breach's delayed end-step sacrifice (whose
/// source is the creature that entered, whatever its name).
pub(crate) fn delayed_effect_matches(name: &str, effect: &EffectOp) -> bool {
    if matches!(
        effect,
        EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::C9cV1(
            C9cOpV1::BreachSacrificeV1 { .. }
        )))
    ) {
        return true;
    }
    name == GRIST && *effect == grist_minus_two_destroy()
}

/// Delayed end-step sacrifice triggers for Through the Breach.
pub(crate) fn delayed_triggers(
    state: &mut GameState,
    events: &[CommittedEvent],
) -> Vec<PendingTrigger> {
    if !cfg!(feature = "eternal-fixtures") {
        return Vec::new();
    }
    let mut triggers = Vec::new();
    if events
        .iter()
        .any(|event| matches!(event, CommittedEvent::BeginningEndStep { .. }))
    {
        let due = state
            .eternal_v1
            .as_mut()
            .map(|eternal| std::mem::take(&mut eternal.c9c.breach_sacrifices))
            .unwrap_or_default();
        for (creature, controller) in due {
            if !link_live_in(state, creature, Zone::Battlefield) {
                continue;
            }
            let mut source = AbilitySourceContractV4::capture(state, creature.object);
            source.controller = controller;
            triggers.push(crate::eternal_cards_v1::delayed_trigger(
                source,
                controller,
                breach_sacrifice(creature),
            ));
        }
    }
    prune(state);
    triggers
}

/// Drops records whose permanents left the battlefield.
pub(crate) fn begin_turn(state: &mut GameState, _player: PlayerId) {
    prune(state);
}

fn prune(state: &mut GameState) {
    let Some(c9c) = read_state(state) else {
        return;
    };
    if c9c.nissa_animated_lands.is_empty() && c9c.breach_haste.is_empty() {
        return;
    }
    let live_nissa: Vec<NissaAnimatedLandV1> = c9c
        .nissa_animated_lands
        .iter()
        .copied()
        .filter(|record| link_live_in(state, record.land, Zone::Battlefield))
        .collect();
    let live_haste: Vec<(ObjectLinkV4, u64)> = c9c
        .breach_haste
        .iter()
        .copied()
        .filter(|(link, _)| link_live_in(state, *link, Zone::Battlefield))
        .collect();
    if let Some(eternal) = state.eternal_v1.as_mut() {
        eternal.c9c.nissa_animated_lands = live_nissa;
        eternal.c9c.breach_haste = live_haste;
    }
}

// ---- Shared helpers -----------------------------------------------------------------

fn halt(state: &mut GameState, source: ObjectId) {
    state.engine.halted = Some((
        crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
        source,
    ));
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

fn live_ability_source(ctx: &ExecCtx, state: &GameState) -> Option<ObjectId> {
    let contract = ctx.ability_source_contract?;
    let live = state.objects.try_get(contract.source)?;
    (live.zone == Zone::Battlefield && live.zone_change_count == contract.zone_change_count)
        .then_some(contract.source)
}

fn battlefield_objects(state: &GameState) -> Vec<ObjectId> {
    [PlayerId::P0, PlayerId::P1]
        .into_iter()
        .flat_map(|player| state.players[player.index()].battlefield.iter().copied())
        .collect()
}

/// Whether `object` is a front-face battlefield permanent with printed name
/// `name` whose static/printed abilities apply.
fn active_named(state: &GameState, object: ObjectId, name: &str) -> bool {
    let Some(live) = state.objects.try_get(object) else {
        return false;
    };
    if live.zone != Zone::Battlefield
        || live.v4.face_index != 0
        || live.v4.face_down_v1.is_some()
        || !crate::continuous_characteristics_v1::printed_abilities_active(state, object)
    {
        return false;
    }
    if let Some(def) = crate::full_text_v1::active_definition(state, object) {
        return def.name == name && def.is_executable();
    }
    CARD_DEFS
        .get(live.card_def as usize)
        .is_some_and(|def| def.name == name && def.is_executable())
}

fn count_active_controlled(state: &GameState, controller: PlayerId, name: &str) -> usize {
    battlefield_objects(state)
        .into_iter()
        .filter(|&object| {
            state.objects.get(object).controller == controller && active_named(state, object, name)
        })
        .count()
}

fn controls_active_named(state: &GameState, controller: PlayerId, name: &str) -> bool {
    count_active_controlled(state, controller, name) > 0
}

fn until_players_next_turn_expiry(state: &GameState, player: PlayerId) -> Option<u32> {
    if state.active_player == player {
        state.turn.checked_add(1)
    } else {
        Some(state.turn_key_v1())
    }
}

// ---- Elspeth, Storm Slayer: token doubling ------------------------------------------

/// Number of active Elspeth, Storm Slayer permanents controlled by `controller`.
pub(crate) fn token_doubler_count(state: &GameState, controller: PlayerId) -> usize {
    if !cfg!(feature = "eternal-fixtures") {
        return 0;
    }
    count_active_controlled(state, controller, ELSPETH)
}

/// Expands every `ProposedEvent::CreateToken` in `events` by `2^n`, where `n`
/// is the number of active Elspeth, Storm Slayer permanents controlled by that
/// token's controller on the pre-event battlefield (CR 614.1, 614.12).
pub(crate) fn expand_doubled_tokens(
    state: &GameState,
    events: Vec<ProposedEvent>,
) -> Result<Vec<ProposedEvent>, String> {
    if !cfg!(feature = "eternal-fixtures") {
        return Ok(events);
    }
    let doublers = [
        token_doubler_count(state, PlayerId::P0),
        token_doubler_count(state, PlayerId::P1),
    ];
    if doublers[0] == 0 && doublers[1] == 0 {
        return Ok(events);
    }
    let mut expanded = Vec::with_capacity(events.len());
    for event in events {
        if let ProposedEvent::CreateToken(ref token) = event {
            let n = doublers[token.controller.index()];
            let mut copies = 1usize;
            for _ in 0..n {
                copies = copies
                    .checked_mul(2)
                    .filter(|&c| c <= MAX_DOUBLED_TOKENS_PER_EVENT)
                    .ok_or_else(|| "Elspeth token doubling overflow".to_string())?;
            }
            expanded
                .try_reserve(copies)
                .map_err(|_| "Elspeth token roster allocation failed".to_string())?;
            for _ in 0..copies {
                expanded.push(event.clone());
            }
        } else {
            expanded.push(event);
        }
    }
    Ok(expanded)
}

// ---- Grist, the Hunger Tide: CR 604.3 non-battlefield CDA ---------------------------

/// Whether `name` is Grist, the Hunger Tide.
pub(crate) fn is_grist_card(name: &str) -> bool {
    cfg!(feature = "eternal-fixtures") && name == GRIST
}

/// Whether `def` is a creature card in any non-battlefield zone (printed
/// Creature or Grist, the Hunger Tide under CR 604.3).
pub(crate) fn is_nonbattlefield_creature_def(def: &CardDef) -> bool {
    def.has_type(CardType::Creature) || is_grist_card(def.name)
}

/// Whether `id` is a live object of Grist, the Hunger Tide outside the
/// battlefield (Hand, Library, Graveyard, Exile, Stack, Command), where its
/// CDA makes it a 1/1 Insect creature in addition to its other types.
pub(crate) fn is_nonbattlefield_grist(state: &GameState, id: ObjectId) -> bool {
    if !cfg!(feature = "eternal-fixtures") {
        return false;
    }
    state.objects.try_get(id).is_some_and(|live| {
        live.zone != Zone::Battlefield
            && live.v4.face_down_v1.is_none()
            && CARD_DEFS
                .get(live.card_def as usize)
                .is_some_and(|def| def.name == GRIST && def.is_executable())
    })
}

/// Whether non-battlefield object `id` is a creature card (printed Creature or
/// Grist's non-battlefield CDA).
pub(crate) fn is_creature_card_outside_battlefield(state: &GameState, id: ObjectId) -> bool {
    state.objects.try_get(id).is_some_and(|live| {
        live.zone != Zone::Battlefield
            && live.v4.face_down_v1.is_none()
            && CARD_DEFS
                .get(live.card_def as usize)
                .is_some_and(is_nonbattlefield_creature_def)
    })
}

/// Grist's CR 604.3 CDA power (1 outside the battlefield).
pub(crate) fn characteristic_defining_power(state: &GameState, id: ObjectId) -> Option<i32> {
    is_nonbattlefield_grist(state, id).then_some(1)
}

/// Grist's CR 604.3 CDA toughness (1 outside the battlefield).
pub(crate) fn characteristic_defining_toughness(state: &GameState, id: ObjectId) -> Option<i32> {
    is_nonbattlefield_grist(state, id).then_some(1)
}

// ---- Nissa, Who Shakes the World: Forest mana trigger, animation & emblem -----------

/// Nissa, Who Shakes the World: "Whenever you tap a Forest for mana, add an
/// additional {G}." Triggered mana ability (CR 605.1b, resolves without the
/// stack). Called from `eternal_pauper_e6b_v1::after_tapped_for_mana` on both
/// manual mana activations and automatic mana payments.
pub(crate) fn after_tapped_for_mana(state: &mut GameState, land: ObjectId) {
    if !cfg!(feature = "eternal-fixtures") {
        return;
    }
    let Some(live) = state.objects.try_get(land) else {
        return;
    };
    if live.zone != Zone::Battlefield
        || !crate::engine::has_effective_subtype(state, land, Subtype::Forest)
    {
        return;
    }
    let controller = live.controller;
    let nissas = count_active_controlled(state, controller, NISSA);
    if nissas == 0 {
        return;
    }
    event::propose_and_commit(
        state,
        ProposedEvent::mana_add(controller, vec![ManaColor::G; nissas]),
    );
}

/// Latest timestamp at which `id`'s current battlefield incarnation was
/// animated by Nissa, Who Shakes the World's `+1`.
pub(crate) fn nissa_animated_timestamp(state: &GameState, id: ObjectId) -> Option<u64> {
    if !cfg!(feature = "eternal-fixtures") {
        return None;
    }
    let c9c = read_state(state)?;
    if c9c.nissa_animated_lands.is_empty() {
        return None;
    }
    let live = state.objects.try_get(id)?;
    if live.zone != Zone::Battlefield {
        return None;
    }
    let link = ObjectLinkV4 {
        object: id,
        zone_change_count: live.zone_change_count,
    };
    c9c.nissa_animated_lands
        .iter()
        .filter(|record| record.land == link)
        .map(|record| record.timestamp)
        .max()
}

/// Layer 7b base power/toughness (0/0) for Nissa-animated lands.
pub(crate) fn base_power_toughness(state: &GameState, id: ObjectId) -> Option<(i16, i16, u64)> {
    nissa_animated_timestamp(state, id).map(|timestamp| (0, 0, timestamp))
}

/// Layer 6 keywords granted by wave C9c effects:
/// - Nissa `+1`: Vigilance and Haste on animated lands.
/// - Nissa `-8` emblem: Indestructible on controlled lands.
/// - Through the Breach: Haste on the creature put onto the battlefield.
pub(crate) fn granted_keywords(state: &GameState, id: ObjectId) -> Keywords {
    if !cfg!(feature = "eternal-fixtures") {
        return Keywords::NONE;
    }
    let Some(c9c) = read_state(state) else {
        return Keywords::NONE;
    };
    if c9c.is_empty() {
        return Keywords::NONE;
    }
    let Some(live) = state.objects.try_get(id) else {
        return Keywords::NONE;
    };
    if live.zone != Zone::Battlefield {
        return Keywords::NONE;
    }
    let mut granted = Keywords::NONE;
    if let Some(timestamp) = nissa_animated_timestamp(state, id) {
        if crate::continuous_characteristics_v1::granted_ability_survives(state, id, timestamp) {
            granted = granted | Keywords::VIGILANCE | Keywords::HASTE;
        }
    }
    if let Some(timestamp) = c9c.nissa_emblems[live.controller.index()] {
        if crate::engine::object_has_type(state, id, CardType::Land)
            && crate::continuous_characteristics_v1::granted_ability_survives(state, id, timestamp)
        {
            granted = granted | Keywords::INDESTRUCTIBLE;
        }
    }
    let link = ObjectLinkV4 {
        object: id,
        zone_change_count: live.zone_change_count,
    };
    if c9c.breach_haste.iter().any(|&(entry, timestamp)| {
        entry == link
            && crate::continuous_characteristics_v1::granted_ability_survives(state, id, timestamp)
    }) {
        granted = granted | Keywords::HASTE;
    }
    granted
}

// ---- Tamiyo, Collector of Tales: opponent discard/sacrifice shield ------------------

/// Whether a resolving spell or ability controlled by `effect_controller` is
/// prevented from causing `affected_player` to discard cards or sacrifice
/// permanents by an active Tamiyo, Collector of Tales.
pub(crate) fn opponent_effect_cant_cause_discard_or_sacrifice(
    state: &GameState,
    effect_controller: PlayerId,
    affected_player: PlayerId,
) -> bool {
    cfg!(feature = "eternal-fixtures")
        && effect_controller != affected_player
        && controls_active_named(state, affected_player, TAMIYO)
}

// ---- Execution ----------------------------------------------------------------------

/// Runs one of this module's leaves in synchronous execution.
pub(crate) fn execute(op: &C9cOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        C9cOpV1::ElspethPlusOneV1 => resolve_elspeth_plus_one(ctx, state),
        C9cOpV1::ElspethZeroV1 => resolve_elspeth_zero(ctx, state),
        C9cOpV1::ElspethMinusThreeV1 => resolve_elspeth_minus_three(ctx, state),
        C9cOpV1::GristPlusOneV1 => resolve_grist_plus_one(ctx, state),
        C9cOpV1::GristMinusTwoDestroyV1 => resolve_grist_minus_two_destroy(ctx, state),
        C9cOpV1::GristMinusFiveV1 => resolve_grist_minus_five(ctx, state),
        C9cOpV1::NissaPlusOneV1 => resolve_nissa_plus_one(ctx, state),
        C9cOpV1::TamiyoPlusOneNamedV1 { named } => resolve_tamiyo_plus_one(ctx, state, *named),
        C9cOpV1::TamiyoMinusThreeV1 => resolve_tamiyo_minus_three(ctx, state),
        C9cOpV1::BreachSacrificeV1 { creature } => resolve_breach_sacrifice(ctx, state, *creature),
        C9cOpV1::GristMinusTwoV1 | C9cOpV1::NissaMinusEightV1 | C9cOpV1::ThroughTheBreachV1 => {
            halt(state, ctx.source);
        }
    }
}

fn offer(
    continuation: &mut EffectContinuation,
    kind: C9cChoiceV1,
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
            kind: E3ChoiceKindV1::C9cV1(kind),
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

/// Runs one of this module's leaves in the resumable interpreter.
pub(crate) fn drive(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    op: &C9cOpV1,
    path: Vec<u16>,
) -> Result<bool, String> {
    match op {
        C9cOpV1::GristMinusTwoV1 => {
            let player = continuation.ctx.controller;
            let creatures: Vec<EffectObjectBinding> = battlefield_objects(state)
                .into_iter()
                .filter(|&object| {
                    state.objects.get(object).controller == player
                        && crate::engine::object_has_type(state, object, CardType::Creature)
                })
                .map(|object| bind(state, object))
                .collect();
            if creatures.is_empty() {
                return Ok(false);
            }
            offer(
                continuation,
                C9cChoiceV1::GristSacrificeV1,
                player,
                creatures,
                0,
                1,
                path,
            );
            Ok(true)
        }
        C9cOpV1::NissaMinusEightV1 => {
            let player = continuation.ctx.controller;
            let timestamp = crate::engine::next_timestamp(state);
            c9c_state(state).nissa_emblems[player.index()] = Some(timestamp);
            let forests: Vec<EffectObjectBinding> = state.players[player.index()]
                .library
                .iter()
                .copied()
                .filter(|&card| crate::engine::has_effective_subtype(state, card, Subtype::Forest))
                .map(|card| bind(state, card))
                .collect();
            if forests.is_empty() {
                state
                    .shuffle_library(player)
                    .map_err(|error| format!("{error:?}"))?;
                crate::eternal_cube_c8c_v1::note_library_searched(state, player);
                return Ok(false);
            }
            let max = u16::try_from(forests.len()).unwrap_or(u16::MAX);
            offer(
                continuation,
                C9cChoiceV1::NissaSearchForestsV1,
                player,
                forests,
                0,
                max,
                path,
            );
            Ok(true)
        }
        C9cOpV1::ThroughTheBreachV1 => {
            let player = continuation.ctx.controller;
            let creatures: Vec<EffectObjectBinding> = state.players[player.index()]
                .hand
                .iter()
                .copied()
                .filter(|&card| is_creature_card_outside_battlefield(state, card))
                .map(|card| bind(state, card))
                .collect();
            if creatures.is_empty() {
                return Ok(false);
            }
            offer(
                continuation,
                C9cChoiceV1::ThroughTheBreachV1,
                player,
                creatures,
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

/// Applies an answered choice of this module.
pub(crate) fn finish_selection(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    choice: &E3ChoiceV1,
    kind: &C9cChoiceV1,
    bindings: Vec<EffectObjectBinding>,
) -> Result<bool, String> {
    match kind {
        C9cChoiceV1::GristSacrificeV1 => {
            let Some(&chosen) = bindings.first() else {
                return Ok(false);
            };
            if !binding_live(state, chosen)
                || state.objects.get(chosen.object).controller != choice.player
                || !crate::engine::object_has_type(state, chosen.object, CardType::Creature)
            {
                return Ok(false);
            }
            let zcc_before = state.objects.get(chosen.object).zone_change_count;
            event::log_sacrifice(state, chosen.object);
            event::propose_and_commit(
                state,
                ProposedEvent::zone_change(chosen.object, Zone::Graveyard),
            );
            if state.engine.halted.is_some() {
                return Ok(false);
            }
            let sacrificed = state
                .objects
                .try_get(chosen.object)
                .is_some_and(|live| live.zone_change_count > zcc_before);
            if sacrificed {
                if let Some(source) = continuation.ctx.ability_source_contract {
                    let mut pending = crate::eternal_cards_v1::delayed_trigger(
                        source,
                        choice.player,
                        grist_minus_two_destroy(),
                    );
                    pending.target_spec = TargetSpec::CreatureOrPlaneswalker;
                    state.engine.pending_triggers.push(pending);
                }
            }
            Ok(false)
        }
        C9cChoiceV1::NissaSearchForestsV1 => {
            let player = choice.player;
            let chosen: Vec<ObjectId> = bindings
                .into_iter()
                .filter(|binding| binding_live(state, *binding))
                .map(|binding| binding.object)
                .collect();
            if !chosen.is_empty() {
                let events: Vec<ProposedEvent> = chosen
                    .into_iter()
                    .map(ProposedEvent::zone_change_to_battlefield_tapped)
                    .collect();
                event::propose_and_commit_batch(state, events);
                if state.engine.halted.is_some() {
                    return Ok(false);
                }
            }
            state
                .shuffle_library(player)
                .map_err(|error| format!("{error:?}"))?;
            crate::eternal_cube_c8c_v1::note_library_searched(state, player);
            Ok(false)
        }
        C9cChoiceV1::ThroughTheBreachV1 => {
            let Some(&chosen) = bindings.first() else {
                return Ok(false);
            };
            if !binding_live(state, chosen) {
                return Ok(false);
            }
            let player = choice.player;
            event::propose_and_commit(
                state,
                ProposedEvent::zone_change_to_battlefield_under_controller(chosen.object, player),
            );
            if crate::linked_aura_operation_v1::entry_checkpoint_in_flight_v1(state)
                || state.engine.halted.is_some()
            {
                return Ok(false);
            }
            if state
                .objects
                .try_get(chosen.object)
                .is_some_and(|live| live.zone == Zone::Battlefield)
            {
                let link = live_link(state, chosen.object);
                let timestamp = crate::engine::next_timestamp(state);
                let c9c = c9c_state(state);
                c9c.breach_haste.push((link, timestamp));
                c9c.breach_sacrifices.push((link, player));
            }
            Ok(false)
        }
    }
}

// ---- Individual resolution helpers --------------------------------------------------

fn resolve_elspeth_plus_one(ctx: &ExecCtx, state: &mut GameState) {
    let Some(soldier) = card_id_by_name(SOLDIER_TOKEN) else {
        halt(state, ctx.source);
        return;
    };
    event::propose_and_commit(state, ProposedEvent::create_token(soldier, ctx.controller));
}

fn resolve_elspeth_zero(ctx: &ExecCtx, state: &mut GameState) {
    let creatures: Vec<ObjectLinkV4> = battlefield_objects(state)
        .into_iter()
        .filter(|&object| {
            state.objects.get(object).controller == ctx.controller
                && crate::engine::object_has_type(state, object, CardType::Creature)
        })
        .map(|object| live_link(state, object))
        .collect();
    if creatures.is_empty() {
        return;
    }
    for &link in &creatures {
        if !link_live_in(state, link, Zone::Battlefield) {
            continue;
        }
        if event::add_plus_one_counters(state, link.object, ctx.controller, 1).is_err() {
            halt(state, ctx.source);
            return;
        }
        if state.engine.halted.is_some() {
            return;
        }
    }
    let Some(expires_at_turn) = until_players_next_turn_expiry(state, ctx.controller) else {
        halt(state, ctx.source);
        return;
    };
    let timestamp = crate::engine::next_timestamp(state);
    for link in creatures {
        if !link_live_in(state, link, Zone::Battlefield) {
            continue;
        }
        state
            .engine
            .until_next_turn_keywords
            .push(UntilNextTurnKeywordEffectV1 {
                object_id: link.object,
                object_zone_change_count: link.zone_change_count,
                holder: ctx.controller,
                expires_at_turn,
                keywords: Keywords::FLYING,
                timestamp: Some(timestamp),
            });
    }
    state.engine.until_next_turn_keywords.sort_by_key(|effect| {
        (
            effect.holder,
            effect.expires_at_turn,
            effect.object_id,
            effect.object_zone_change_count,
            effect.keywords.0,
        )
    });
}

fn resolve_elspeth_minus_three(ctx: &ExecCtx, state: &mut GameState) {
    let Some(source_contract) = ctx.ability_source_contract else {
        return;
    };
    let Some(Target::Object(target)) = ctx.targets.first().copied() else {
        return;
    };
    if !ctx.target_incarnation_matches(0, state)
        || !crate::engine::effect_target_is_legal_from_ability_source(
            state,
            source_contract,
            ctx.controller,
            ELSPETH_MINUS_THREE_TARGET,
            &ctx.targets,
            0,
        )
    {
        return;
    }
    if crate::engine::has_effective_keyword(state, target, Keywords::INDESTRUCTIBLE) {
        return;
    }
    let destroyed = crate::effect::without_regenerated_v1(state, vec![target]);
    if let Some(victim) = destroyed.into_iter().next() {
        event::propose_and_commit(state, ProposedEvent::zone_change(victim, Zone::Graveyard));
    }
}

fn resolve_grist_plus_one(ctx: &ExecCtx, state: &mut GameState) {
    let Some(insect_token) = card_id_by_name(BLACK_GREEN_INSECT_TOKEN) else {
        halt(state, ctx.source);
        return;
    };
    for _ in 0..MAX_GRIST_LOOP_ITERATIONS {
        event::propose_and_commit(
            state,
            ProposedEvent::create_token(insect_token, ctx.controller),
        );
        if state.engine.halted.is_some() {
            return;
        }
        let Some(top_card) = state.players[ctx.controller.index()]
            .library
            .first()
            .copied()
        else {
            break;
        };
        let zcc_before = state.objects.get(top_card).zone_change_count;
        event::propose_and_commit(state, ProposedEvent::zone_change(top_card, Zone::Graveyard));
        if state.engine.halted.is_some() {
            return;
        }
        let milled_insect = state.objects.try_get(top_card).is_some_and(|live| {
            live.zone_change_count > zcc_before
                && matches!(live.zone, Zone::Graveyard | Zone::Exile)
                && !live.v4.is_token
                && live.spell_copy_origin.is_none()
                && crate::engine::has_effective_subtype(state, top_card, Subtype::Insect)
        });
        if !milled_insect {
            break;
        }
        if let Some(source) = live_ability_source(ctx, state) {
            crate::planeswalker_v1::add_loyalty_counters(state, source, 1);
            if state.engine.halted.is_some() {
                return;
            }
        }
    }
}

fn resolve_grist_minus_two_destroy(ctx: &ExecCtx, state: &mut GameState) {
    let Some(source_contract) = ctx.ability_source_contract else {
        return;
    };
    let Some(Target::Object(target)) = ctx.targets.first().copied() else {
        return;
    };
    if !ctx.target_incarnation_matches(0, state)
        || !crate::engine::effect_target_is_legal_from_ability_source(
            state,
            source_contract,
            ctx.controller,
            TargetSpec::CreatureOrPlaneswalker,
            &ctx.targets,
            0,
        )
    {
        return;
    }
    if crate::engine::has_effective_keyword(state, target, Keywords::INDESTRUCTIBLE) {
        return;
    }
    let destroyed = crate::effect::without_regenerated_v1(state, vec![target]);
    if let Some(victim) = destroyed.into_iter().next() {
        event::propose_and_commit(state, ProposedEvent::zone_change(victim, Zone::Graveyard));
    }
}

fn resolve_grist_minus_five(ctx: &ExecCtx, state: &mut GameState) {
    let count = state.players[ctx.controller.index()]
        .graveyard
        .iter()
        .copied()
        .filter(|&card| {
            let live = state.objects.get(card);
            !live.v4.is_token
                && live.spell_copy_origin.is_none()
                && is_creature_card_outside_battlefield(state, card)
        })
        .count();
    let amount = i32::try_from(count).unwrap_or(i32::MAX);
    if amount > 0 {
        event::propose_and_commit(
            state,
            ProposedEvent::life_loss(ctx.controller.opponent(), amount),
        );
    }
}

fn resolve_nissa_plus_one(ctx: &ExecCtx, state: &mut GameState) {
    let Some(source_contract) = ctx.ability_source_contract else {
        return;
    };
    let Some(Target::Object(land)) = ctx.targets.first().copied() else {
        return;
    };
    if !ctx.target_incarnation_matches(0, state)
        || !crate::engine::effect_target_is_legal_from_ability_source(
            state,
            source_contract,
            ctx.controller,
            NISSA_PLUS_ONE_TARGET,
            &ctx.targets,
            0,
        )
    {
        return;
    }
    let link = live_link(state, land);
    if event::add_plus_one_counters(state, land, ctx.controller, 3).is_err() {
        halt(state, ctx.source);
        return;
    }
    if state.engine.halted.is_some() || !link_live_in(state, link, Zone::Battlefield) {
        return;
    }
    if state.objects.get(land).tapped {
        let _ = crate::engine::attempt_untap(state, land);
        if state.engine.halted.is_some() || !link_live_in(state, link, Zone::Battlefield) {
            return;
        }
    }
    let timestamp = crate::engine::next_timestamp(state);
    c9c_state(state)
        .nissa_animated_lands
        .push(NissaAnimatedLandV1 {
            land: link,
            timestamp,
        });
}

fn resolve_tamiyo_plus_one(ctx: &ExecCtx, state: &mut GameState, named: u16) {
    let Some(chosen_name) = CARD_DEFS.get(usize::from(named)).map(|def| def.object_name) else {
        halt(state, ctx.source);
        return;
    };
    let controller = ctx.controller;
    let top: Vec<ObjectId> = state.players[controller.index()]
        .library
        .iter()
        .take(4)
        .copied()
        .collect();
    if top.is_empty() {
        return;
    }
    for observer in [PlayerId::P0, PlayerId::P1] {
        // spellbench-public-reveal: public reveal to both players.
        state.reveal_library_top(observer, controller, top.len());
    }
    let mut to_hand = Vec::new();
    let mut to_graveyard = Vec::new();
    for card in top {
        let matches = state
            .objects
            .try_get(card)
            .and_then(|live| CARD_DEFS.get(usize::from(live.card_def)))
            .is_some_and(|def| def.object_name == chosen_name);
        if matches {
            to_hand.push(card);
        } else {
            to_graveyard.push(card);
        }
    }
    if !to_hand.is_empty() {
        let events: Vec<ProposedEvent> = to_hand
            .into_iter()
            .filter(|&card| state.objects.get(card).zone == Zone::Library)
            .map(|card| ProposedEvent::zone_change_preserving_known_identity(card, Zone::Hand))
            .collect();
        if !events.is_empty() {
            event::propose_and_commit_batch(state, events);
            if state.engine.halted.is_some() {
                return;
            }
        }
    }
    if !to_graveyard.is_empty() {
        let events: Vec<ProposedEvent> = to_graveyard
            .into_iter()
            .filter(|&card| state.objects.get(card).zone == Zone::Library)
            .map(|card| ProposedEvent::zone_change(card, Zone::Graveyard))
            .collect();
        if !events.is_empty() {
            event::propose_and_commit_batch(state, events);
        }
    }
}

fn resolve_tamiyo_minus_three(ctx: &ExecCtx, state: &mut GameState) {
    let Some(source_contract) = ctx.ability_source_contract else {
        return;
    };
    let Some(Target::Object(card)) = ctx.targets.first().copied() else {
        return;
    };
    if !ctx.target_incarnation_matches(0, state)
        || !crate::engine::effect_target_is_legal_from_ability_source(
            state,
            source_contract,
            ctx.controller,
            TAMIYO_MINUS_THREE_TARGET,
            &ctx.targets,
            0,
        )
    {
        return;
    }
    event::propose_and_commit(state, ProposedEvent::zone_change(card, Zone::Hand));
}

fn resolve_breach_sacrifice(ctx: &ExecCtx, state: &mut GameState, creature: ObjectLinkV4) {
    if !link_live_in(state, creature, Zone::Battlefield) {
        return;
    }
    if state.objects.get(creature.object).controller != ctx.controller {
        return;
    }
    event::log_sacrifice(state, creature.object);
    event::propose_and_commit(
        state,
        ProposedEvent::zone_change(creature.object, Zone::Graveyard),
    );
}
