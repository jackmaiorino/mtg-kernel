//! Card programs for the eternal catalog's Pauper Affinity, Tron,
//! Ephemerate and Faeries lists (wave E6a,
//! `docs/design/eternal_formats_support_v1.md`).
//!
//! `eternal_cards_v1` forwards its effect leaves, trigger conditions and
//! state here, and the engine calls the hooks below only in
//! `eternal-fixtures` builds. Every table is keyed by a printed name that
//! occurs only in the eternal registry, so the Pauper, FDN and Standard
//! catalogs never reach it. Behavior was read from each card's current
//! Oracle text and, where present, its XMage source.
//!
//! New rules here: fogs (all combat damage, or all damage creatures would
//! deal, this turn), flicker, rebound, evoke, foretell (a special action
//! that exiles the card face down with a later-turn cast permission),
//! untapping during another player's untap step, a creature craft
//! material, and an Aura trigger on its creature becoming tapped or being
//! dealt damage. Resolution choices use E3's typed selection
//! (`E3ChoiceKindV1::E6aV1`) or a generated option list
//! (`EffectOp::Choice`); a rebound cast reuses E5b's paid resolution cast.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

use crate::card_def::{
    card_id_by_name, AltCostCondition, CardDef, CardType, Keywords, TargetSpec, CARD_DEFS,
};
use crate::effect::{
    CreatureSacrificeFilter, EffectContinuation, EffectFrame, EffectObjectBinding, EffectOp,
    ExecCtx, ObjectRef, PlayerRef,
};
use crate::eternal_cards_v1::e3::{E3ChoiceKindV1, E3ChoiceV1};
use crate::eternal_cards_v1::EternalOpV1;
use crate::event::{self, CommittedEvent, DamageProposed, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::mana::{Cost, ManaColor, Pip};
use crate::standard_cards_v1::{
    StandardChosenActionV1, StandardOpV1, StandardPermanentFilterV1, StandardTargetV1,
    StandardTriggerV1,
};
use crate::state::{
    AbilitySourceContractV4, CastMethodV4, GameState, ObjectLinkV4, StackItem, StackItemKind,
    Target, Zone,
};
use crate::trigger::{PendingTrigger, TriggerCondition, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};

// ---- State ------------------------------------------------------------------------

/// Per-game state for this module's cards, kept inside
/// `eternal_cards_v1::EternalStateV1`. Default (and absent from snapshots)
/// until one of these cards needs it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct E6aStateV1 {
    /// Damage prevention shields lasting this turn (Moment's Peace,
    /// Ethereal Haze, Tangle).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fogs: Vec<FogV1>,
    /// Mulldrifter spells cast for their evoke cost, by stack incarnation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evoked_casts: Vec<ObjectLinkV4>,
    /// Resolving Ephemerate spells cast from their owner's hand, by stack
    /// incarnation: rebound exiles them as they resolve.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rebounding_spells: Vec<ObjectLinkV4>,
    /// Cards rebound exiled, waiting for their controller's next upkeep.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rebounds: Vec<ReboundV1>,
    /// Foretold cards, by face-down exile incarnation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub foretold: Vec<ObjectLinkV4>,
    /// Triggers detected by hooks, waiting for the next trigger collection.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub queued_triggers: Vec<QueuedTriggerV1>,
}

impl E6aStateV1 {
    pub(crate) fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// What a fog prevents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FogKindV1 {
    /// "Prevent all combat damage that would be dealt this turn."
    CombatDamageV1,
    /// "Prevent all damage that would be dealt by creatures this turn."
    DamageByCreaturesV1,
}

/// One fog, for the turn identified by its round and active player.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FogV1 {
    pub turn: u32,
    pub active_player: PlayerId,
    pub kind: FogKindV1,
}

/// A rebound card in exile and the player who cast it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReboundV1 {
    pub card: ObjectLinkV4,
    pub controller: PlayerId,
}

/// A triggered ability queued by a hook.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueuedTriggerV1 {
    pub source: AbilitySourceContractV4,
    pub controller: PlayerId,
    pub op: E6aOpV1,
}

fn e6a_state(state: &mut GameState) -> &mut E6aStateV1 {
    &mut state.eternal_v1.get_or_insert_with(Default::default).e6a
}

fn read_state(state: &GameState) -> Option<&E6aStateV1> {
    state.eternal_v1.as_ref().map(|eternal| &eternal.e6a)
}

/// Trigger conditions owned by this module, carried as
/// `eternal_cards_v1::EternalTriggerV1::E6aV1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum E6aTriggerV1 {
    /// Evoke (702.74a): "When this enters, if its evoke cost was paid, its
    /// controller sacrifices it" (Mulldrifter).
    EvokedEntersV1,
}

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::E6aV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum E6aOpV1 {
    /// Target 0 gets +`power`/+`toughness` until end of turn (Brinebarrow
    /// Intruder, Grim Bauble).
    PumpTargetV1 { power: i32, toughness: i32 },
    /// Tap target 0 (Sewer-veillance Cam).
    TapTargetV1,
    /// Untap target 0 (Sewer-veillance Cam).
    UntapTargetV1,
    /// Dispatch: tap target 0, then exile it with metalcraft.
    DispatchV1,
    /// Ephemerate: flicker target 0 under its owner's control, then mark
    /// the spell for rebound if it was cast from hand.
    EphemerateV1,
    /// Ghostly Flicker: flicker targets 0 and 1 under the controller's
    /// control.
    GhostlyFlickerV1,
    /// A fog for this turn.
    FogV1 { kind: FogKindV1 },
    /// Tangle: a combat damage fog, and each attacking creature doesn't
    /// untap during its controller's next untap step.
    TangleV1,
    /// "You gain life equal to the number of cards in your hand" (Union of
    /// the Third Path).
    GainLifePerHandCardV1,
    /// Stream of Thought: "You shuffle up to four cards from your graveyard
    /// into your library." A choice.
    StreamShuffleV1,
    /// Rebound's upkeep trigger: the controller may cast `card` from exile
    /// without paying its mana cost. A choice.
    ReboundV1 { card: ObjectLinkV4 },
    /// The rebound answer: cast `card`. A choice.
    ReboundCastV1 { card: ObjectLinkV4 },
    /// Cryoshatter's trigger: destroy `creature`.
    DestroyCreatureV1 { creature: ObjectLinkV4 },
    /// "This creature can't be blocked this turn" for the ability's source
    /// incarnation (Gearseeker Serpent).
    SourceCantBeBlockedV1,
    /// Foretell's printed slot. The engine performs it as a special action
    /// (`foretell`) and never resolves this program.
    ForetellV1,
}

impl E6aOpV1 {
    /// Whether this leaf can yield a player decision, so its program must
    /// use the resumable interpreter.
    pub(crate) fn contains_player_choice(&self) -> bool {
        matches!(
            self,
            Self::StreamShuffleV1 | Self::ReboundV1 { .. } | Self::ReboundCastV1 { .. }
        )
    }
}

/// Resolution choices owned by this module, carried as
/// `E3ChoiceKindV1::E6aV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum E6aChoiceV1 {
    /// Stream of Thought: up to four cards from the chooser's graveyard to
    /// shuffle into their library.
    StreamShuffleV1,
}

impl E6aChoiceV1 {
    /// Whether the candidates are hidden from the other player.
    pub(crate) fn chooser_private(&self) -> bool {
        false
    }

    pub(crate) fn public_purpose(&self) -> crate::rl::TargetSelectionPurposeV4 {
        crate::rl::TargetSelectionPurposeV4::CardSelection
    }

    /// Object incarnations this choice refers to beyond its candidates.
    pub(crate) fn referenced_bindings(&self) -> Vec<EffectObjectBinding> {
        Vec::new()
    }
}

// ---- Names and helpers --------------------------------------------------------------

const ARCHAEOMANCER: &str = "Archaeomancer";
const BEHOLD_THE_MULTIVERSE: &str = "Behold the Multiverse";
const BENDERS_WATERSKIN: &str = "Bender's Waterskin";
const BRAMBLE_WURM: &str = "Bramble Wurm";
const BRINEBARROW_INTRUDER: &str = "Brinebarrow Intruder";
const CRYOSHATTER: &str = "Cryoshatter";
const ENERGY_REFRACTOR: &str = "Energy Refractor";
const EPHEMERATE: &str = "Ephemerate";
const GRIM_BAUBLE: &str = "Grim Bauble";
const KOR_SKYFISHER: &str = "Kor Skyfisher";
const MULLDRIFTER: &str = "Mulldrifter";
const OMEN_OF_THE_DEAD: &str = "Omen of the Dead";
const PROPHETIC_PRISM: &str = "Prophetic Prism";
const SEWER_VEILLANCE_CAM: &str = "Sewer-veillance Cam";
const TITHING_BLADE: &str = "Tithing Blade";
const CRAB_TOKEN: &str = "Crab Token";
const CLUE_TOKEN: &str = "Clue Token";

fn e6a(op: E6aOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::E6aV1(op)))
}

fn halt(state: &mut GameState, source: ObjectId) {
    state.engine.halted = Some((
        crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
        source,
    ));
}

fn token_id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name} in CARD_DEFS"))
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

fn definition_name(state: &GameState, object: ObjectId) -> Option<&'static str> {
    let live = state.objects.try_get(object)?;
    CARD_DEFS.get(live.card_def as usize).map(|def| def.name)
}

/// Whether `object` is a front-face battlefield permanent named `name`
/// whose printed abilities apply.
fn active_named(state: &GameState, object: ObjectId, name: &str) -> bool {
    state.objects.try_get(object).is_some_and(|live| {
        live.zone == Zone::Battlefield
            && live.v4.face_index == 0
            && CARD_DEFS
                .get(live.card_def as usize)
                .is_some_and(|def| def.name == name && def.is_executable())
            && crate::continuous_characteristics_v1::printed_abilities_active(state, object)
    })
}

/// The battlefield permanents named `name` whose printed abilities apply,
/// optionally only those `controller` controls.
fn active_permanents<'a>(
    state: &'a GameState,
    name: &'a str,
    controller: Option<PlayerId>,
) -> impl Iterator<Item = ObjectId> + 'a {
    [PlayerId::P0, PlayerId::P1]
        .into_iter()
        .flat_map(move |player| state.players[player.index()].battlefield.iter().copied())
        .filter(move |&object| {
            active_named(state, object, name)
                && controller.is_none_or(|player| state.objects.get(object).controller == player)
        })
}

/// The resolving ability's source, if it is still the incarnation that
/// created the ability, on the battlefield.
fn live_ability_source(ctx: &ExecCtx, state: &GameState) -> Option<ObjectId> {
    let contract = ctx.ability_source_contract?;
    let live = state.objects.try_get(contract.source)?;
    (live.zone == Zone::Battlefield && live.zone_change_count == contract.zone_change_count)
        .then_some(contract.source)
}

/// Target `index` if it still names its announced incarnation on the
/// battlefield.
fn bound_battlefield_target(ctx: &ExecCtx, state: &GameState, index: usize) -> Option<ObjectId> {
    let Some(Target::Object(object)) = ctx.targets.get(index).copied() else {
        return None;
    };
    (ctx.target_incarnation_matches(index, state)
        && state.objects.get(object).zone == Zone::Battlefield)
        .then_some(object)
}

/// Target `index`, if it is an object still legal for the resolving spell
/// or ability under `spec`.
fn legal_object_target(
    ctx: &ExecCtx,
    state: &GameState,
    index: usize,
    spec: TargetSpec,
) -> Option<ObjectId> {
    let object = bound_battlefield_target(ctx, state, index)?;
    crate::engine::effect_target_is_legal(
        state,
        ctx.source,
        ctx.controller,
        spec,
        &ctx.targets,
        index,
    )
    .then_some(object)
}

fn queue_trigger(
    state: &mut GameState,
    source: AbilitySourceContractV4,
    controller: PlayerId,
    op: E6aOpV1,
) {
    e6a_state(state).queued_triggers.push(QueuedTriggerV1 {
        source,
        controller,
        op,
    });
}

fn nothing() -> EffectOp {
    EffectOp::Sequence(Vec::new())
}

/// "Sacrifice it" for a resolving ability's own source incarnation.
fn sacrifice_source() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::SacrificeSourceV1))
}

/// "Counter target spell" (the shared generated counter program).
fn counter_target_spell() -> EffectOp {
    EffectOp::Conditional {
        cond: crate::effect::EffectCond::TargetSpellCanBeCountered(0),
        then: Box::new(EffectOp::MoveObject {
            object: ObjectRef::Target(0),
            to_zone: Zone::Graveyard,
        }),
        else_: Box::new(nothing()),
    }
}

/// Exile each of `objects`, then return the cards to the battlefield under
/// `controller` (or each card's owner). Tokens cease to exist in exile.
fn flicker(state: &mut GameState, objects: &[ObjectId], controller: Option<PlayerId>) {
    let mut exiled = Vec::new();
    for &object in objects {
        event::propose_and_commit(state, ProposedEvent::zone_change(object, Zone::Exile));
        let live = state.objects.get(object);
        if live.zone == Zone::Exile && !live.v4.is_token {
            exiled.push(live_link(state, object));
        }
    }
    for card in exiled {
        if !link_live_in(state, card, Zone::Exile) {
            continue;
        }
        let owner = state.objects.get(card.object).owner;
        event::propose_and_commit(
            state,
            ProposedEvent::zone_change_to_battlefield_under_controller(
                card.object,
                controller.unwrap_or(owner),
            ),
        );
    }
}

/// Destroy `object` unless it is indestructible.
fn destroy(state: &mut GameState, object: ObjectId) {
    if state.objects.get(object).zone == Zone::Battlefield
        && !crate::engine::has_effective_keyword(state, object, Keywords::INDESTRUCTIBLE)
    {
        event::propose_and_commit(state, ProposedEvent::zone_change(object, Zone::Graveyard));
    }
}

// ---- Spells -------------------------------------------------------------------------

/// Crop Rotation: "Search your library for a land card, put that card onto
/// the battlefield, then shuffle." Its land sacrifice is the card's
/// additional cost.
pub fn crop_rotation() -> EffectOp {
    EffectOp::SearchLibraryCardsToDestination {
        player: PlayerRef::Controller,
        filter: crate::effect::LibraryCardFilter::AnyLand,
        max_targets: 1,
        destination: crate::effect::LibrarySearchDestinationV1::Battlefield { tapped: false },
    }
}

/// Dispatch: "Tap target creature. Metalcraft -- If you control three or
/// more artifacts, exile that creature."
pub fn dispatch() -> EffectOp {
    e6a(E6aOpV1::DispatchV1)
}

/// Ephemerate: "Exile target creature you control, then return it to the
/// battlefield under its owner's control. Rebound."
pub fn ephemerate() -> EffectOp {
    e6a(E6aOpV1::EphemerateV1)
}

/// Ethereal Haze: "Prevent all damage that would be dealt by creatures this
/// turn."
pub fn ethereal_haze() -> EffectOp {
    // An Arcane instant: spliced Desperate Rituals add their text.
    EffectOp::Sequence(vec![
        e6a(E6aOpV1::FogV1 {
            kind: FogKindV1::DamageByCreaturesV1,
        }),
        crate::eternal_pauper_e8b_v1::spliced_effects(),
    ])
}

/// Ghostly Flicker: "Exile two target artifacts, creatures, and/or lands
/// you control, then return those cards to the battlefield under your
/// control."
pub fn ghostly_flicker() -> EffectOp {
    e6a(E6aOpV1::GhostlyFlickerV1)
}

/// Hard Evidence: "Create a 0/3 blue Crab creature token. Investigate."
pub fn hard_evidence() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::CreateToken {
            token_def: token_id(CRAB_TOKEN),
            controller: PlayerRef::Controller,
        },
        EffectOp::CreateToken {
            token_def: token_id(CLUE_TOKEN),
            controller: PlayerRef::Controller,
        },
    ])
}

/// Moment's Peace: "Prevent all combat damage that would be dealt this
/// turn." Flashback is the card's own `CardDef::flashback`.
pub fn moments_peace() -> EffectOp {
    e6a(E6aOpV1::FogV1 {
        kind: FogKindV1::CombatDamageV1,
    })
}

/// Mystical Teachings: "Search your library for an instant card or a card
/// with flash, reveal it, put it into your hand, then shuffle."
pub fn mystical_teachings() -> EffectOp {
    EffectOp::SearchLibraryToHand {
        player: PlayerRef::Controller,
        filter: crate::effect::LibraryCardFilter::InstantOrFlashV1,
    }
}

/// Prohibit: "Counter target spell if its mana value is 2 or less. If this
/// spell was kicked, counter that spell if its mana value is 4 or less
/// instead." Both checks happen on resolution.
pub fn prohibit() -> EffectOp {
    let counter_at_most = |maximum: u16| EffectOp::Conditional {
        cond: crate::effect::EffectCond::TargetManaValueAtMost(0, maximum),
        then: Box::new(counter_target_spell()),
        else_: Box::new(nothing()),
    };
    EffectOp::Conditional {
        cond: crate::effect::EffectCond::WasKicked,
        then: Box::new(counter_at_most(4)),
        else_: Box::new(counter_at_most(2)),
    }
}

/// Stream of Thought: "Target player mills four cards. You shuffle up to
/// four cards from your graveyard into your library." Replicate is
/// `replicate_cost`.
pub fn stream_of_thought() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::MillCards {
            player: PlayerRef::Target(0),
            count: 4,
        },
        e6a(E6aOpV1::StreamShuffleV1),
    ])
}

/// Tangle: "Prevent all combat damage that would be dealt this turn. Each
/// attacking creature doesn't untap during its controller's next untap
/// step."
pub fn tangle() -> EffectOp {
    e6a(E6aOpV1::TangleV1)
}

/// Union of the Third Path: "Draw a card, then you gain life equal to the
/// number of cards in your hand."
pub fn union_of_the_third_path() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 1,
        },
        e6a(E6aOpV1::GainLifePerHandCardV1),
    ])
}

/// Gearseeker Serpent's ability.
pub fn gearseeker_unblockable() -> EffectOp {
    e6a(E6aOpV1::SourceCantBeBlockedV1)
}

/// Behold the Multiverse's foretell slot (a special action, never put on
/// the stack).
pub fn foretell_action() -> EffectOp {
    e6a(E6aOpV1::ForetellV1)
}

/// Replicate costs (702.56): Stream of Thought's {2}{U}{U}.
pub fn replicate_cost(name: &str) -> Option<Cost> {
    match name {
        "Stream of Thought" => Some(Cost {
            pips: &[Pip::Colored(ManaColor::U), Pip::Colored(ManaColor::U)],
            generic: 2,
            x_count: 0,
        }),
        _ => None,
    }
}

// ---- Triggered abilities -------------------------------------------------------------

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

const fn e6a_trigger(condition: E6aTriggerV1, effect: fn() -> EffectOp) -> TriggeredAbilityDef {
    trigger(
        TriggerCondition::StandardV1(StandardTriggerV1::EternalV1(
            crate::eternal_cards_v1::EternalTriggerV1::E6aV1(condition),
        )),
        effect,
    )
}

fn draw_one() -> EffectOp {
    EffectOp::DrawCards {
        player: PlayerRef::Controller,
        count: 1,
    }
}

fn draw_two() -> EffectOp {
    EffectOp::DrawCards {
        player: PlayerRef::Controller,
        count: 2,
    }
}

/// "Return target ... card from your graveyard to your hand" (Archaeomancer,
/// Omen of the Dead). The target filter is per card.
fn return_target_card_to_hand() -> EffectOp {
    EffectOp::MoveAllTargets {
        to_zone: Zone::Hand,
    }
}

fn gain_five() -> EffectOp {
    EffectOp::GainLife {
        player: PlayerRef::Controller,
        amount: 5,
    }
}

/// Brinebarrow Intruder: "target creature an opponent controls gets -2/-0
/// until end of turn."
fn intruder_shrink() -> EffectOp {
    e6a(E6aOpV1::PumpTargetV1 {
        power: -2,
        toughness: 0,
    })
}

/// Grim Bauble: "target creature an opponent controls gets -2/-2 until end
/// of turn."
fn bauble_shrink() -> EffectOp {
    e6a(E6aOpV1::PumpTargetV1 {
        power: -2,
        toughness: -2,
    })
}

/// Kor Skyfisher: "return a permanent you control to its owner's hand." Not
/// targeted; chosen on resolution, and it may be the Skyfisher.
fn return_controlled_permanent() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::PlayerChoosesControlledPermanent {
        player: PlayerRef::Controller,
        filter: StandardPermanentFilterV1::Any,
        action: StandardChosenActionV1::ReturnToOwnersHand,
    })
}

/// Sewer-veillance Cam: "you may tap or untap target creature."
fn cam_tap_or_untap() -> EffectOp {
    EffectOp::Choice {
        controller: PlayerRef::Controller,
        options: vec![
            e6a(E6aOpV1::TapTargetV1),
            e6a(E6aOpV1::UntapTargetV1),
            nothing(),
        ],
    }
}

/// Tithing Blade: "each opponent sacrifices a creature of their choice."
fn opponent_sacrifices_creature() -> EffectOp {
    EffectOp::SacrificeCreature {
        player: PlayerRef::Opponent,
        filter: CreatureSacrificeFilter::Any,
    }
}

/// Consuming Sepulcher: "each opponent loses 1 life and you gain 1 life."
fn sepulcher_drain() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::LoseLife {
            player: PlayerRef::Opponent,
            amount: 1,
        },
        EffectOp::GainLife {
            player: PlayerRef::Controller,
            amount: 1,
        },
    ])
}

const ARCHAEOMANCER_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, return_target_card_to_hand)];
const BRAMBLE_WURM_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(TriggerCondition::Etb, gain_five)];
const BRINEBARROW_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, intruder_shrink)];
const DRAW_ONE_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(TriggerCondition::Etb, draw_one)];
const GRIM_BAUBLE_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, bauble_shrink)];
const KOR_SKYFISHER_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, return_controlled_permanent)];
const MULLDRIFTER_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(TriggerCondition::Etb, draw_two),
    e6a_trigger(E6aTriggerV1::EvokedEntersV1, sacrifice_source),
];
const OMEN_OF_THE_DEAD_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, return_target_card_to_hand)];
const SEWER_VEILLANCE_CAM_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(TriggerCondition::Etb, cam_tap_or_untap),
    trigger(TriggerCondition::LeftBattlefield, cam_tap_or_untap),
];
const TITHING_BLADE_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(TriggerCondition::Etb, opponent_sacrifices_creature),
    TriggeredAbilityDef {
        face_index: 1,
        ..trigger(
            TriggerCondition::BeginningOfUpkeep {
                controller_only: true,
            },
            sepulcher_drain,
        )
    },
];

/// Triggered abilities of this module's cards.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        ARCHAEOMANCER => &ARCHAEOMANCER_TRIGGERS,
        BRAMBLE_WURM => &BRAMBLE_WURM_TRIGGERS,
        BRINEBARROW_INTRUDER => &BRINEBARROW_TRIGGERS,
        ENERGY_REFRACTOR | PROPHETIC_PRISM => &DRAW_ONE_TRIGGERS,
        GRIM_BAUBLE => &GRIM_BAUBLE_TRIGGERS,
        KOR_SKYFISHER => &KOR_SKYFISHER_TRIGGERS,
        MULLDRIFTER => &MULLDRIFTER_TRIGGERS,
        OMEN_OF_THE_DEAD => &OMEN_OF_THE_DEAD_TRIGGERS,
        SEWER_VEILLANCE_CAM => &SEWER_VEILLANCE_CAM_TRIGGERS,
        TITHING_BLADE => &TITHING_BLADE_TRIGGERS,
        _ => crate::eternal_pauper_e6b_v1::triggers_for(name),
    }
}

fn own_trigger_effect(name: &str, effect: &EffectOp) -> bool {
    triggers_for(name)
        .iter()
        .any(|trigger| (trigger.effect)() == *effect)
        || delayed_effect_matches(name, effect)
}

/// Target specs of this module's triggered abilities, per ability.
pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    // `triggers_for` falls through to E6b's cards, so they answer first.
    if let Some(spec) = crate::eternal_pauper_e6b_v1::trigger_target_spec(name, effect) {
        return Some(spec);
    }
    if !own_trigger_effect(name, effect) {
        return None;
    }
    Some(match name {
        ARCHAEOMANCER if *effect == return_target_card_to_hand() => {
            TargetSpec::StandardV1(StandardTargetV1::InstantOrSorceryCardInOwnGraveyard)
        }
        OMEN_OF_THE_DEAD if *effect == return_target_card_to_hand() => {
            TargetSpec::CreatureCardInOwnGraveyard
        }
        BRINEBARROW_INTRUDER if *effect == intruder_shrink() => {
            TargetSpec::OpponentControlledCreature
        }
        GRIM_BAUBLE if *effect == bauble_shrink() => TargetSpec::OpponentControlledCreature,
        SEWER_VEILLANCE_CAM if *effect == cam_tap_or_untap() => TargetSpec::Creature,
        _ => TargetSpec::None,
    })
}

/// Queued and delayed triggers owned by this module's cards.
pub(crate) fn delayed_effect_matches(name: &str, effect: &EffectOp) -> bool {
    if crate::eternal_pauper_e6b_v1::delayed_effect_matches(name, effect) {
        return true;
    }
    let EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::E6aV1(op))) = effect else {
        return false;
    };
    match name {
        EPHEMERATE => matches!(op, E6aOpV1::ReboundV1 { .. }),
        CRYOSHATTER => matches!(op, E6aOpV1::DestroyCreatureV1 { .. }),
        _ => false,
    }
}

/// Whether `condition` triggers on its source's arrival on the battlefield.
pub(crate) fn arrival_condition(condition: E6aTriggerV1) -> bool {
    condition == E6aTriggerV1::EvokedEntersV1
}

pub(crate) fn trigger_matches(
    condition: E6aTriggerV1,
    events: &[CommittedEvent],
    index: usize,
    source: ObjectId,
    state: &GameState,
) -> bool {
    match condition {
        E6aTriggerV1::EvokedEntersV1 => {
            let entered_from_stack = matches!(
                events[index],
                CommittedEvent::ZoneChange {
                    object,
                    from: Zone::Stack,
                    to: Zone::Battlefield,
                    ..
                } if object == source
            );
            let Some(stack) = state.objects.get(source).zone_change_count.checked_sub(1) else {
                return false;
            };
            entered_from_stack
                && read_state(state).is_some_and(|e6a| {
                    e6a.evoked_casts.contains(&ObjectLinkV4 {
                        object: source,
                        zone_change_count: stack,
                    })
                })
        }
    }
}

// ---- Effect leaves ------------------------------------------------------------------

pub(crate) fn execute(op: &E6aOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        E6aOpV1::PumpTargetV1 { power, toughness } => {
            let Some(object) = bound_battlefield_target(ctx, state, 0) else {
                return;
            };
            if !crate::engine::object_has_type(state, object, CardType::Creature) {
                return;
            }
            let binding = bind(state, object);
            crate::effect::install_temporary_boost(
                state,
                binding,
                *power,
                *toughness,
                Keywords::NONE,
            );
        }
        E6aOpV1::TapTargetV1 => {
            if let Some(object) = bound_battlefield_target(ctx, state, 0) {
                event::propose_and_commit(state, ProposedEvent::tap(object));
            }
        }
        E6aOpV1::UntapTargetV1 => {
            if let Some(object) = bound_battlefield_target(ctx, state, 0) {
                crate::engine::attempt_untap(state, object);
            }
        }
        E6aOpV1::DispatchV1 => {
            let Some(object) = legal_object_target(ctx, state, 0, TargetSpec::Creature) else {
                return;
            };
            event::propose_and_commit(state, ProposedEvent::tap(object));
            let artifacts = state.players[ctx.controller.index()]
                .battlefield
                .iter()
                .filter(|&&id| crate::engine::object_has_type(state, id, CardType::Artifact))
                .count();
            if artifacts >= 3 && state.objects.get(object).zone == Zone::Battlefield {
                event::propose_and_commit(state, ProposedEvent::zone_change(object, Zone::Exile));
            }
        }
        E6aOpV1::EphemerateV1 => {
            if let Some(object) = legal_object_target(ctx, state, 0, TargetSpec::ControlledCreature)
            {
                flicker(state, &[object], None);
            }
            mark_rebound(ctx, state);
        }
        E6aOpV1::GhostlyFlickerV1 => {
            let spec =
                TargetSpec::StandardV1(StandardTargetV1::TwoControlledArtifactsCreaturesOrLandsV1);
            let objects = (0..ctx.targets.len().min(2))
                .filter_map(|index| legal_object_target(ctx, state, index, spec))
                .collect::<Vec<_>>();
            flicker(state, &objects, Some(ctx.controller));
        }
        E6aOpV1::FogV1 { kind } => add_fog(state, *kind),
        E6aOpV1::TangleV1 => {
            add_fog(state, FogKindV1::CombatDamageV1);
            let attackers = state.engine.combat.attackers.clone();
            for attacker in attackers {
                if state
                    .objects
                    .try_get(attacker)
                    .is_some_and(|live| live.zone == Zone::Battlefield)
                {
                    state.objects.get_mut(attacker).v4.skip_next_untap = true;
                }
            }
        }
        E6aOpV1::GainLifePerHandCardV1 => {
            let cards = state.players[ctx.controller.index()].hand.len();
            let amount = i32::try_from(cards).unwrap_or(i32::MAX);
            if amount > 0 {
                crate::effect::execute(
                    &EffectOp::GainLife {
                        player: PlayerRef::Controller,
                        amount,
                    },
                    ctx,
                    state,
                );
            }
        }
        E6aOpV1::DestroyCreatureV1 { creature } => {
            if link_live_in(state, *creature, Zone::Battlefield) {
                destroy(state, creature.object);
            }
        }
        E6aOpV1::SourceCantBeBlockedV1 => {
            if live_ability_source(ctx, state).is_some() {
                crate::effect::execute(
                    &EffectOp::GrantKeywordTargetUntilEndOfTurn {
                        object: ObjectRef::ThisSource,
                        keyword: Keywords::CANT_BE_BLOCKED,
                    },
                    ctx,
                    state,
                );
            }
        }
        E6aOpV1::ForetellV1 => {}
        // Choices run in `drive`; the plain executor never reaches them.
        E6aOpV1::StreamShuffleV1 | E6aOpV1::ReboundV1 { .. } | E6aOpV1::ReboundCastV1 { .. } => {
            halt(state, ctx.source)
        }
    }
}

fn add_fog(state: &mut GameState, kind: FogKindV1) {
    let fog = FogV1 {
        turn: state.turn_key_v1(),
        active_player: state.active_player,
        kind,
    };
    let fogs = &mut e6a_state(state).fogs;
    if !fogs.contains(&fog) {
        fogs.push(fog);
    }
}

/// Rebound (702.88a): "If you cast this spell from your hand, exile it as it
/// resolves." Marks the resolving spell's stack incarnation; the departure
/// hook `rebounds_on_departure` then exiles it.
fn mark_rebound(ctx: &ExecCtx, state: &mut GameState) {
    let Some(live) = state.objects.try_get(ctx.source) else {
        return;
    };
    let from_hand = live
        .v4
        .spell_cast_origin
        .is_some_and(|origin| origin.origin_zone == Zone::Hand);
    if live.zone != Zone::Stack || live.spell_copy_origin.is_some() || !from_hand {
        return;
    }
    let link = live_link(state, ctx.source);
    let spells = &mut e6a_state(state).rebounding_spells;
    if !spells.contains(&link) {
        spells.push(link);
    }
}

/// A choice among `options`, as a frame pushed for the interpreter.
fn push_choice(continuation: &mut EffectContinuation, options: Vec<EffectOp>, path: Vec<u16>) {
    continuation.frames.push(EffectFrame::Program {
        op: EffectOp::Choice {
            controller: PlayerRef::Controller,
            options,
        },
        path,
    });
}

/// Runs a leaf inside the resumable interpreter. `Ok(true)` when it staged a
/// choice and the continuation must suspend.
pub(crate) fn drive(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    op: &E6aOpV1,
    path: Vec<u16>,
) -> Result<bool, String> {
    let controller = continuation.ctx.controller;
    match op {
        E6aOpV1::StreamShuffleV1 => {
            let cards = state.players[controller.index()]
                .graveyard
                .iter()
                .copied()
                .filter(|&card| {
                    let live = state.objects.get(card);
                    live.owner == controller
                        && !live.v4.is_token
                        && live.spell_copy_origin.is_none()
                })
                .map(|card| bind(state, card))
                .collect::<Vec<_>>();
            if cards.is_empty() {
                return Ok(false);
            }
            let max = u16::try_from(cards.len().min(4)).unwrap_or(4);
            let remaining = continuation.frames.clone();
            crate::eternal_cards_v1::e3::offer(
                continuation,
                E3ChoiceV1 {
                    kind: E3ChoiceKindV1::E6aV1(E6aChoiceV1::StreamShuffleV1),
                    player: controller,
                    candidates: cards
                        .iter()
                        .map(|binding| Target::Object(binding.object))
                        .collect(),
                    bindings: cards,
                    min: 0,
                    max,
                    ordered: false,
                    path,
                    remaining,
                },
            );
            Ok(true)
        }
        E6aOpV1::ReboundV1 { card } => {
            if !link_live_in(state, *card, Zone::Exile) {
                return Ok(false);
            }
            let binding = bind(state, card.object);
            if crate::engine::resolution_cast_v1::can_cast_exiled_without_mana(
                state, controller, binding, None,
            ) {
                push_choice(
                    continuation,
                    vec![e6a(E6aOpV1::ReboundCastV1 { card: *card }), nothing()],
                    path,
                );
            }
            Ok(false)
        }
        E6aOpV1::ReboundCastV1 { card } => {
            if !link_live_in(state, *card, Zone::Exile) {
                return Ok(false);
            }
            let binding = bind(state, card.object);
            Ok(crate::eternal_legacy_e5b_v1::offer_paid_cast(
                state,
                continuation,
                binding,
                path,
            ))
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
    _continuation: &mut EffectContinuation,
    choice: &E3ChoiceV1,
    kind: &E6aChoiceV1,
    bindings: Vec<EffectObjectBinding>,
) -> Result<bool, String> {
    match kind {
        E6aChoiceV1::StreamShuffleV1 => {
            let player = choice.player;
            if bindings.iter().any(|binding| {
                let live = state.objects.get(binding.object);
                live.zone != Zone::Graveyard
                    || live.owner != player
                    || live.zone_change_count != binding.expected_zone_change_count
            }) {
                return Err("Stream of Thought chose a card no longer in the graveyard".to_string());
            }
            if bindings.is_empty() {
                return Ok(false);
            }
            event::propose_and_commit_batch(
                state,
                bindings
                    .iter()
                    .map(|binding| ProposedEvent::zone_change(binding.object, Zone::Library))
                    .collect(),
            );
            state
                .shuffle_library(player)
                .map_err(|_| "Stream of Thought could not shuffle the library".to_string())?;
            Ok(false)
        }
    }
}

// ---- Hooks: triggers ----------------------------------------------------------------

/// The triggers this module's hooks set off in this collection, and the
/// bookkeeping of evoke, rebound and foretell.
pub(crate) fn delayed_triggers(
    state: &mut GameState,
    events: &[CommittedEvent],
) -> Vec<PendingTrigger> {
    if !cfg!(feature = "eternal-fixtures") {
        return Vec::new();
    }
    note_evoked_casts(state, events);
    note_rebound_exiles(state, events);
    rebound_upkeep_triggers(state, events);
    cryoshatter_triggers(state, events);
    prune(state);
    let Some(eternal) = state.eternal_v1.as_mut() else {
        return Vec::new();
    };
    if eternal.e6a.queued_triggers.is_empty() {
        return Vec::new();
    }
    std::mem::take(&mut eternal.e6a.queued_triggers)
        .into_iter()
        .map(|queued| {
            crate::eternal_cards_v1::delayed_trigger(
                queued.source,
                queued.controller,
                e6a(queued.op),
            )
        })
        .collect()
}

/// Evoke: Mulldrifter spells cast for their alternative cost.
fn note_evoked_casts(state: &mut GameState, events: &[CommittedEvent]) {
    for event in events {
        let CommittedEvent::SpellCast { spell, .. } = *event else {
            continue;
        };
        if definition_name(state, spell) != Some(MULLDRIFTER) {
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
            let casts = &mut e6a_state(state).evoked_casts;
            if !casts.contains(&link) {
                casts.push(link);
            }
        }
    }
}

/// Rebound: a marked spell exiled as it resolved waits for its controller's
/// next upkeep.
fn note_rebound_exiles(state: &mut GameState, events: &[CommittedEvent]) {
    for event in events {
        let CommittedEvent::ZoneChange {
            object,
            from: Zone::Stack,
            to: Zone::Exile,
            controller_before,
        } = *event
        else {
            continue;
        };
        let Some(live) = state.objects.try_get(object) else {
            continue;
        };
        if live.zone != Zone::Exile {
            continue;
        }
        let Some(stack) = live.zone_change_count.checked_sub(1) else {
            continue;
        };
        let card = live_link(state, object);
        let marked = ObjectLinkV4 {
            object,
            zone_change_count: stack,
        };
        let e6a = e6a_state(state);
        if let Some(position) = e6a
            .rebounding_spells
            .iter()
            .position(|link| *link == marked)
        {
            e6a.rebounding_spells.remove(position);
            e6a.rebounds.push(ReboundV1 {
                card,
                controller: controller_before,
            });
        }
    }
}

/// "At the beginning of your next upkeep, you may cast this card from
/// exile without paying its mana cost."
fn rebound_upkeep_triggers(state: &mut GameState, events: &[CommittedEvent]) {
    for event in events {
        let CommittedEvent::UpkeepBegan { player } = *event else {
            continue;
        };
        let Some(e6a) = state.eternal_v1.as_mut().map(|eternal| &mut eternal.e6a) else {
            return;
        };
        let (due, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut e6a.rebounds)
            .into_iter()
            .partition(|rebound| rebound.controller == player);
        e6a.rebounds = rest;
        for rebound in due {
            if !link_live_in(state, rebound.card, Zone::Exile) {
                continue;
            }
            let source = AbilitySourceContractV4::capture(state, rebound.card.object);
            queue_trigger(
                state,
                source,
                player,
                E6aOpV1::ReboundV1 { card: rebound.card },
            );
        }
    }
}

/// Cryoshatter: "When enchanted creature becomes tapped or is dealt damage,
/// destroy it." One trigger per Cryoshatter for each kind of event in a
/// collected batch (simultaneous combat damage is one event).
fn cryoshatter_triggers(state: &mut GameState, events: &[CommittedEvent]) {
    let auras = active_permanents(state, CRYOSHATTER, None).collect::<Vec<_>>();
    if auras.is_empty() {
        return;
    }
    let mut fired: Vec<(ObjectId, bool)> = Vec::new();
    for event in events {
        let (creature, damaged) = match *event {
            CommittedEvent::Tap { object } => (object, false),
            CommittedEvent::Damage {
                target: Target::Object(object),
                amount,
                ..
            } if amount > 0 => (object, true),
            _ => continue,
        };
        let Some(live) = state.objects.try_get(creature) else {
            continue;
        };
        if live.zone != Zone::Battlefield {
            continue;
        }
        let link = live_link(state, creature);
        for &aura in &auras {
            if state.objects.get(aura).v4.attached_to != Some(link)
                || fired.contains(&(aura, damaged))
            {
                continue;
            }
            fired.push((aura, damaged));
            let source = AbilitySourceContractV4::capture(state, aura);
            let controller = state.objects.get(aura).controller;
            queue_trigger(
                state,
                source,
                controller,
                E6aOpV1::DestroyCreatureV1 { creature: link },
            );
        }
    }
}

/// Drops records whose objects moved on.
fn prune(state: &mut GameState) {
    let Some(e6a) = read_state(state) else {
        return;
    };
    let keep_stack = |links: &[ObjectLinkV4]| {
        links
            .iter()
            .copied()
            .filter(|&link| link_live_in(state, link, Zone::Stack))
            .collect::<Vec<_>>()
    };
    let evoked = keep_stack(&e6a.evoked_casts);
    let rebounding = keep_stack(&e6a.rebounding_spells);
    // A foretold card being cast keeps its record until it leaves the stack.
    let foretold = e6a
        .foretold
        .iter()
        .copied()
        .filter(|&link| {
            link_live_in(state, link, Zone::Exile)
                || link
                    .zone_change_count
                    .checked_add(1)
                    .is_some_and(|zone_change_count| {
                        link_live_in(
                            state,
                            ObjectLinkV4 {
                                object: link.object,
                                zone_change_count,
                            },
                            Zone::Stack,
                        )
                    })
        })
        .collect::<Vec<_>>();
    let changed = evoked.len() != e6a.evoked_casts.len()
        || rebounding.len() != e6a.rebounding_spells.len()
        || foretold.len() != e6a.foretold.len();
    if changed {
        let e6a = e6a_state(state);
        e6a.evoked_casts = evoked;
        e6a.rebounding_spells = rebounding;
        e6a.foretold = foretold;
    }
}

/// The beginning of a turn ends the last turn's fogs.
pub(crate) fn begin_turn(state: &mut GameState, _active: PlayerId) {
    if let Some(eternal) = state.eternal_v1.as_mut() {
        eternal.e6a.fogs.clear();
    }
}

// ---- Hooks: engine ------------------------------------------------------------------

/// Fogs: whether a fog this turn prevents `damage`.
pub(crate) fn prevents_damage(state: &GameState, damage: &DamageProposed) -> bool {
    // Maze of Ith (wave E7a).
    if crate::eternal_legacy_e7a_v1::prevents_damage(state, damage) {
        return true;
    }
    // Cephalid Illusionist (wave E9a).
    if crate::eternal_legacy_e9a_v1::statics::prevents_damage(state, damage) {
        return true;
    }
    let Some(e6a) = read_state(state) else {
        return false;
    };
    if e6a.fogs.is_empty() {
        return false;
    }
    let turn = state.turn_key_v1();
    e6a.fogs.iter().any(|fog| {
        fog.turn == turn
            && fog.active_player == state.active_player
            && match fog.kind {
                FogKindV1::CombatDamageV1 => damage.is_combat,
                FogKindV1::DamageByCreaturesV1 => {
                    crate::eternal_vintage_v1::source_is_creature(state, damage.source)
                }
            }
    })
}

/// Bender's Waterskin: "Untap this artifact during each other player's
/// untap step." Called during `active`'s untap step.
pub(crate) fn untap_during_other_untap_steps(state: &mut GameState, active: PlayerId) {
    let waterskins =
        active_permanents(state, BENDERS_WATERSKIN, Some(active.opponent())).collect::<Vec<_>>();
    for waterskin in waterskins {
        crate::engine::attempt_untap(state, waterskin);
    }
}

/// Rebound's replacement: a marked spell is exiled instead of going to
/// its owner's graveyard as it resolves.
pub(crate) fn rebounds_on_departure(state: &GameState, item: &StackItem) -> bool {
    if item.is_copy {
        return false;
    }
    let Some(live) = state.objects.try_get(item.source) else {
        return false;
    };
    let link = ObjectLinkV4 {
        object: item.source,
        zone_change_count: live.zone_change_count,
    };
    live.zone == Zone::Stack
        && read_state(state).is_some_and(|e6a| e6a.rebounding_spells.contains(&link))
}

/// Tithing Blade's craft material is a creature (craft with creature).
pub(crate) fn craft_material_type(state: &GameState, source: ObjectId) -> Option<CardType> {
    (definition_name(state, source) == Some(TITHING_BLADE)).then_some(CardType::Creature)
}

/// Activated abilities this module gates: foretelling only during its
/// owner's turn.
pub(crate) fn activation_allowed(
    state: &GameState,
    source: ObjectId,
    name: &str,
    ability_index: usize,
) -> bool {
    if name != BEHOLD_THE_MULTIVERSE || ability_index != 0 {
        return true;
    }
    state
        .objects
        .try_get(source)
        .is_some_and(|live| live.zone == Zone::Hand && live.owner == state.active_player)
}

/// Whether activating `source`'s ability `index` is the foretell special
/// action.
pub(crate) fn is_foretell_action(state: &GameState, source: ObjectId, index: u8) -> bool {
    index == 0
        && definition_name(state, source) == Some(BEHOLD_THE_MULTIVERSE)
        && state
            .objects
            .try_get(source)
            .is_some_and(|live| live.zone == Zone::Hand)
}

/// Foretell (702.143a): pays {2} and exiles `source` from `player`'s hand
/// face down. Only its owner may look at it; it may be cast on a later turn
/// for its foretell cost. No stack, no priority pass.
pub(crate) fn foretell(
    state: &mut GameState,
    player: PlayerId,
    source: ObjectId,
) -> Result<(), String> {
    if !is_foretell_action(state, source, 0)
        || state.objects.get(source).owner != player
        || state.active_player != player
    {
        return Err("the foretell special action is not available".to_string());
    }
    if !crate::engine::pay_effect_mana(player, &[], 2, state) {
        return Err("foretell's {2} is not payable".to_string());
    }
    event::propose_and_commit(state, ProposedEvent::zone_change(source, Zone::Exile));
    if state.objects.get(source).zone != Zone::Exile {
        return Ok(());
    }
    let link = live_link(state, source);
    state.objects.get_mut(source).v4.face_down_v1 = Some(crate::state::FaceDownV1 {
        disguised: false,
        lookers: 1 << player.index(),
        hidden_by: None,
        hidden_by_source: None,
    });
    let permission = crate::engine::PlayPermission {
        without_mana_cost: crate::engine::FreeCastV1::default(),
        object: source,
        holder: player,
        zone_change_generation: link.zone_change_count,
        play_or_cast: crate::engine::PlayOrCast::Cast,
        expiry: crate::engine::PlayPermissionExpiry::LaterTurn {
            granted_turn: state.turn_key_v1(),
            granted_active_player: state.active_player,
        },
    };
    state.engine.exile_play_permissions.push(permission);
    e6a_state(state).foretold.push(link);
    Ok(())
}

/// An aborted cast of a foretold card returns it to exile as a new object:
/// it stays foretold, face down, with its cast permission.
pub(crate) fn restore_aborted_foretell(state: &mut GameState, card: ObjectId) {
    let live_count = state.objects.get(card).zone_change_count;
    let Some(exile_count) = live_count.checked_sub(2) else {
        return;
    };
    let old = ObjectLinkV4 {
        object: card,
        zone_change_count: exile_count,
    };
    let Some(e6a) = state.eternal_v1.as_mut().map(|eternal| &mut eternal.e6a) else {
        return;
    };
    let Some(position) = e6a.foretold.iter().position(|link| *link == old) else {
        return;
    };
    e6a.foretold[position] = ObjectLinkV4 {
        object: card,
        zone_change_count: live_count,
    };
    let owner = state.objects.get(card).owner;
    state.objects.get_mut(card).v4.face_down_v1 = Some(crate::state::FaceDownV1 {
        disguised: false,
        lookers: 1 << owner.index(),
        hidden_by: None,
        hidden_by_source: None,
    });
    for permission in &mut state.engine.exile_play_permissions {
        if permission.object == card && permission.zone_change_generation == exile_count {
            permission.zone_change_generation = live_count;
        }
    }
}

/// Whether `object` is a foretold card, in exile or as the spell cast from
/// that exile incarnation.
fn foretold_incarnation(state: &GameState, object: ObjectId) -> bool {
    let Some(live) = state.objects.try_get(object) else {
        return false;
    };
    let exile_count = match live.zone {
        Zone::Exile => Some(live.zone_change_count),
        Zone::Stack => live.zone_change_count.checked_sub(1),
        _ => None,
    };
    exile_count.is_some_and(|zone_change_count| {
        read_state(state).is_some_and(|e6a| {
            e6a.foretold.contains(&ObjectLinkV4 {
                object,
                zone_change_count,
            })
        })
    })
}

/// Foretell's cost rule: a foretold card is cast from exile only for its
/// foretell cost, and the foretell cost pays only for a foretold card.
pub(crate) fn foretell_quote_allowed(
    state: &GameState,
    definition: &CardDef,
    source: ObjectId,
    origin_zone: Zone,
    method: CastMethodV4,
) -> bool {
    let foretell = definition
        .alt_cost
        .is_some_and(|alt| alt.condition == AltCostCondition::ForetoldV1);
    if !foretell {
        return true;
    }
    let foretold = origin_zone == Zone::Exile && foretold_incarnation(state, source);
    if method == CastMethodV4::Alternative {
        foretold
    } else {
        !foretold
    }
}
