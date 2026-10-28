//! Card programs for the eternal catalog's Pauper and Vintage long tail
//! (wave E9b, `docs/design/eternal_formats_support_v1.md`): Pauper Turbo Fog,
//! Gruul Ponza, Gruul Ramp, Food Gardens, 4c Pestilence, Esper Faeries and 4c
//! Ephemerate, and Vintage Scam, Eldrazi, Red Prison, Grixis Lurrus, Lurrus
//! Vault Key and Underworld Breach.
//!
//! `eternal_cards_v1` forwards its effect leaves, choices and state here, and
//! the engine calls the hooks below only in `eternal-fixtures` builds. Every
//! table is keyed by a printed name that occurs only in the eternal registry,
//! so the Pauper, FDN and Standard catalogs never reach it. Behavior was read
//! from each card's current Oracle text and, where present, its XMage source.
//!
//! New rules here: cascade (exile until a cheaper nonland card, a free cast
//! during the trigger's resolution, the rest to the bottom in a random
//! order), shadow, Dauthi Voidwalker's void counters and its free play, a
//! mana ability from the hand (Elvish Spirit Guide, without the stack like
//! Arena of Glory's), Wild Growth's mana trigger, a granted leaves trigger
//! (Thief of Existence), "whenever a player casts" (Mai), Barrowgoyf's
//! characteristic-defining stats and its combat damage mill, Pestilence's
//! end-step sacrifice, and Arcane Denial's next-upkeep draws. Momentum
//! Breaker's "Start your engines!" is the Standard speed designation.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

use crate::card_def::{
    card_id_by_name, CardDef, CardType, Subtype, Supertype, TargetSpec, CARD_DEFS,
};
use crate::effect::{
    EffectCond, EffectContinuation, EffectFrame, EffectObjectBinding, EffectOp, ExecCtx,
    LibraryCardFilter, LibrarySearchDestinationV1, ObjectRef, PlayerRef, TargetRef,
};
use crate::eternal_cards_v1::e3::{E3ChoiceKindV1, E3ChoiceV1};
use crate::eternal_cards_v1::EternalOpV1;
use crate::event::{self, CommittedEvent, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::mana::ManaColor;
use crate::standard_cards_v1::{
    StandardChosenActionV1, StandardOpV1, StandardPermanentFilterV1, StandardTargetV1,
};
use crate::state::{AbilitySourceContractV4, GameState, ObjectLinkV4, StackItemKind, Target, Zone};
use crate::trigger::{PendingTrigger, TriggerCondition, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};

// ---- State ------------------------------------------------------------------------

/// Per-game state for this module's cards, kept inside
/// `eternal_cards_v1::EternalStateV1`. Default (and absent from snapshots)
/// until one of these cards needs it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct E9bStateV1 {
    /// Exiled cards with a void counter (Dauthi Voidwalker), by exile
    /// incarnation. An exiled card without an entry has none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub void_cards: Vec<ObjectLinkV4>,
    /// Thief of Existence battlefield incarnations that gained "When this
    /// creature leaves the battlefield, target opponent draws a card."
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub thief_grants: Vec<ObjectLinkV4>,
    /// Arcane Denial's draws, waiting for the next turn's upkeep.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub denial_draws: Vec<DenialDrawV1>,
    /// Triggers detected by hooks, waiting for the next trigger collection.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub queued_triggers: Vec<QueuedTriggerV1>,
}

impl E9bStateV1 {
    pub(crate) fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// One resolved Arcane Denial: its card, its controller, the controller of
/// the spell it targeted, and the turn it resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DenialDrawV1 {
    pub card: ObjectId,
    pub controller: PlayerId,
    pub player: PlayerId,
    pub turn: u32,
}

/// A triggered ability queued by a hook, with its target specification.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueuedTriggerV1 {
    pub source: AbilitySourceContractV4,
    pub controller: PlayerId,
    pub effect: EffectOp,
    /// Whether the ability targets an opponent (Thief of Existence's gained
    /// trigger).
    pub targets_opponent: bool,
}

fn e9b_state(state: &mut GameState) -> &mut E9bStateV1 {
    &mut state.eternal_v1.get_or_insert_with(Default::default).e9b
}

fn read_state(state: &GameState) -> Option<&E9bStateV1> {
    state.eternal_v1.as_ref().map(|eternal| &eternal.e9b)
}

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::E9bV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum E9bOpV1 {
    /// Cascade (702.85a) from a cast trigger. May be a choice.
    CascadeV1,
    /// Cascade's free cast of `card`. A choice.
    CascadeCastV1 { card: ObjectLinkV4 },
    /// Cascade's last step: `cards` still exiled go to the bottom of
    /// `player`'s library in a random order.
    CascadeBottomV1 {
        player: PlayerId,
        cards: Vec<ObjectLinkV4>,
    },
    /// Respite: gain 1 life for each attacking creature.
    RespiteLifeV1,
    /// Echoing Truth: return target 0 and every other permanent with its
    /// name to their owners' hands.
    EchoingTruthV1,
    /// Boomerang Basics: return target 0 to its owner's hand; draw a card if
    /// the controller controlled it. Its draw runs in the interpreter, so
    /// dredge can replace it.
    BoomerangBasicsV1,
    /// Arcane Denial: counter target 0 and register the next upkeep's draws.
    ArcaneDenialV1,
    /// Arcane Denial's draw for the countered spell's controller, `player`:
    /// up to two cards. A choice.
    DenialMayDrawV1 { player: PlayerId },
    /// Castigate: target opponent reveals their hand; the controller exiles
    /// a nonland card from it. A choice.
    CastigateV1,
    /// Growth Spiral: may put a land card from the hand onto the
    /// battlefield. A choice.
    GrowthSpiralLandV1,
    /// Thermokarst: destroy target land; gain 1 life if it was snow.
    ThermokarstV1,
    /// Ram Through's damage, with trample's excess to the controller.
    RamThroughV1,
    /// Pieces of the Puzzle: reveal five, up to two instants and sorceries
    /// to the hand, the rest to the graveyard. A choice.
    PiecesV1,
    /// You Meet in a Tavern's first mode. A choice.
    TavernPartyV1,
    /// Barrowgoyf's combat damage trigger: may mill `amount`. A choice.
    BarrowgoyfV1 { amount: i32 },
    /// Barrowgoyf's mill and its creature card to hand. A choice.
    BarrowgoyfMillV1 { amount: i32 },
    /// Momentum Breaker's entry: each opponent sacrifices a creature or
    /// Vehicle, or discards. A choice.
    MomentumEdictV1,
    /// Momentum Breaker's ability: gain life equal to the controller's
    /// speed.
    MomentumLifeV1,
    /// Pestilence's end step trigger: sacrifice it if no creatures are on
    /// the battlefield.
    PestilenceEndV1,
    /// Dauthi Voidwalker's ability: choose a void card an opponent owns and
    /// play it this turn without paying its mana cost. A choice.
    DauthiChooseV1,
    /// Oswald Fiddlebender's search, keyed by the sacrificed artifact. A
    /// choice.
    OswaldSearchV1,
    /// Mai, Scornful Striker's trigger: `player` loses 2 life.
    MaiLoseLifeV1 { player: PlayerId },
    /// Thief of Existence's cast trigger: exile up to one target and grant
    /// the leaves trigger.
    ThiefExileV1,
    /// Sowing Mycospawn's kicked cast trigger: exile target land.
    ExileTargetV1,
}

impl E9bOpV1 {
    /// Whether this leaf can yield a player decision, so its program must
    /// use the resumable interpreter.
    pub(crate) fn contains_player_choice(&self) -> bool {
        matches!(
            self,
            Self::CascadeV1
                | Self::CascadeCastV1 { .. }
                | Self::BoomerangBasicsV1
                | Self::DenialMayDrawV1 { .. }
                | Self::CastigateV1
                | Self::GrowthSpiralLandV1
                | Self::PiecesV1
                | Self::TavernPartyV1
                | Self::BarrowgoyfV1 { .. }
                | Self::BarrowgoyfMillV1 { .. }
                | Self::MomentumEdictV1
                | Self::DauthiChooseV1
                | Self::OswaldSearchV1
        )
    }
}

/// Resolution choices owned by this module, carried as
/// `E3ChoiceKindV1::E9bV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum E9bChoiceV1 {
    /// Castigate: the nonland card to exile from the revealed hand.
    CastigateV1,
    /// Growth Spiral: up to one land card from the hand.
    GrowthSpiralV1,
    /// Pieces of the Puzzle: up to two of the `revealed` instants and
    /// sorceries.
    PiecesV1 { revealed: Vec<EffectObjectBinding> },
    /// You Meet in a Tavern: any number of the `looked` creature cards.
    TavernV1 { looked: Vec<EffectObjectBinding> },
    /// Barrowgoyf: up to one milled creature card.
    BarrowgoyfV1,
    /// Momentum Breaker: the opponent's creature or Vehicle to sacrifice.
    MomentumSacrificeV1,
    /// Dauthi Voidwalker: the void card to play.
    DauthiV1,
}

impl E9bChoiceV1 {
    /// Whether the candidates are hidden from the other player.
    pub(crate) fn chooser_private(&self) -> bool {
        matches!(self, Self::GrowthSpiralV1 | Self::TavernV1 { .. })
    }

    pub(crate) fn public_purpose(&self) -> crate::rl::TargetSelectionPurposeV4 {
        match self {
            Self::MomentumSacrificeV1 => crate::rl::TargetSelectionPurposeV4::PermanentSelection,
            _ => crate::rl::TargetSelectionPurposeV4::CardSelection,
        }
    }

    /// Object incarnations this choice refers to beyond its candidates.
    pub(crate) fn referenced_bindings(&self) -> Vec<EffectObjectBinding> {
        match self {
            Self::PiecesV1 { revealed } => revealed.clone(),
            Self::TavernV1 { looked } => looked.clone(),
            _ => Vec::new(),
        }
    }
}

// ---- Names and helpers --------------------------------------------------------------

const ANNOYED_ALTISAUR: &str = "Annoyed Altisaur";
const ARCANE_DENIAL: &str = "Arcane Denial";
const BARROWGOYF: &str = "Barrowgoyf";
const BOARDING_PARTY: &str = "Boarding Party";
const BOGGART_TRAWLER: &str = "Boggart Trawler";
const BRIDGEWORKS_BATTLE: &str = "Bridgeworks Battle";
const CAULDRON_FAMILIAR: &str = "Cauldron Familiar";
const DAUTHI_VOIDWALKER: &str = "Dauthi Voidwalker";
const ELDRAZI_REPURPOSER: &str = "Eldrazi Repurposer";
const ELVISH_SPIRIT_GUIDE: &str = "Elvish Spirit Guide";
const JEWEL_THIEF: &str = "Jewel Thief";
const MAI: &str = "Mai, Scornful Striker";
const MOMENTUM_BREAKER: &str = "Momentum Breaker";
const NUTRIENT_BLOCK: &str = "Nutrient Block";
const PESTILENCE: &str = "Pestilence";
const RADIANT_FOUNTAIN: &str = "Radiant Fountain";
const SIMIC_GROWTH_CHAMBER: &str = "Simic Growth Chamber";
const SOWING_MYCOSPAWN: &str = "Sowing Mycospawn";
const THIEF_OF_EXISTENCE: &str = "Thief of Existence";
const VAMPIRE_SOVEREIGN: &str = "Vampire Sovereign";
const WILD_GROWTH: &str = "Wild Growth";
const ELDRAZI_SPAWN_TOKEN: &str = "Eldrazi Spawn Token";
const TREASURE_TOKEN: &str = "Treasure Token";

fn e9b(op: E9bOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::E9bV1(op)))
}

fn nothing() -> EffectOp {
    EffectOp::Sequence(Vec::new())
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

fn binding_live(state: &GameState, binding: EffectObjectBinding) -> bool {
    state.objects.try_get(binding.object).is_some_and(|live| {
        live.zone == binding.expected_zone
            && live.zone_change_count == binding.expected_zone_change_count
    })
}

fn definition(state: &GameState, object: ObjectId) -> Option<&'static CardDef> {
    let live = state.objects.try_get(object)?;
    CARD_DEFS.get(live.card_def as usize)
}

fn definition_name(state: &GameState, object: ObjectId) -> Option<&'static str> {
    definition(state, object).map(|def| def.name)
}

/// Whether `object` is a battlefield permanent named `name` showing its
/// front face, whose printed abilities apply.
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

/// Every battlefield object, in each player's battlefield order.
fn battlefield(state: &GameState) -> impl Iterator<Item = ObjectId> + '_ {
    [PlayerId::P0, PlayerId::P1]
        .into_iter()
        .flat_map(move |player| state.players[player.index()].battlefield.iter().copied())
}

fn is_creature(state: &GameState, object: ObjectId) -> bool {
    crate::engine::object_has_type(state, object, CardType::Creature)
}

/// Whether `object` is a physical card rather than a token or a spell copy.
fn is_card(state: &GameState, object: ObjectId) -> bool {
    state.objects.try_get(object).is_some_and(|live| {
        !live.v4.is_token
            && live.spell_copy_origin.is_none()
            && CARD_DEFS
                .get(live.card_def as usize)
                .is_some_and(|def| !def.is_token)
    })
}

/// Target `index` if it still names its announced incarnation, in `zone`.
fn bound_target_in(ctx: &ExecCtx, state: &GameState, index: usize, zone: Zone) -> Option<ObjectId> {
    let Some(Target::Object(object)) = ctx.targets.get(index).copied() else {
        return None;
    };
    (ctx.target_incarnation_matches(index, state) && state.objects.get(object).zone == zone)
        .then_some(object)
}

fn push_frame(continuation: &mut EffectContinuation, op: EffectOp, path: &[u16]) {
    continuation.frames.push(EffectFrame::Program {
        op,
        path: path.to_vec(),
    });
}

fn push_choice(continuation: &mut EffectContinuation, options: Vec<EffectOp>, path: &[u16]) {
    push_frame(
        continuation,
        EffectOp::Choice {
            controller: PlayerRef::Controller,
            options,
        },
        path,
    );
}

/// Offers one of this module's selections; the frames still to run after
/// it are the continuation's current frames.
fn offer(
    continuation: &mut EffectContinuation,
    kind: E9bChoiceV1,
    player: PlayerId,
    bindings: Vec<EffectObjectBinding>,
    (min, max): (u16, u16),
    path: Vec<u16>,
) {
    let remaining = continuation.frames.clone();
    crate::eternal_cards_v1::e3::offer(
        continuation,
        E3ChoiceV1 {
            kind: E3ChoiceKindV1::E9bV1(kind),
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

fn halt(state: &mut GameState, source: ObjectId) {
    state.engine.halted = Some((
        crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
        source,
    ));
}

fn sacrifice(state: &mut GameState, object: ObjectId) {
    event::log_sacrifice(state, object);
    event::propose_and_commit(state, ProposedEvent::zone_change(object, Zone::Graveyard));
}

/// The controller of the spell `spell` on the stack.
fn spell_controller(state: &GameState, spell: ObjectId) -> Option<PlayerId> {
    state
        .stack
        .iter()
        .find(|item| item.kind == StackItemKind::Spell && item.source == spell)
        .map(|item| item.controller)
}

/// The printed name a permanent shows: its land face's name for a modal
/// double-faced card showing it. Face-down permanents have none.
fn shown_name(state: &GameState, object: ObjectId) -> Option<&'static str> {
    let live = state.objects.try_get(object)?;
    if live.v4.face_down_v1.is_some() {
        return None;
    }
    let def = CARD_DEFS.get(live.card_def as usize)?;
    if live.v4.face_index == 1 {
        if let Some(face) = &def.transform_face {
            return Some(face.name);
        }
    }
    Some(def.name)
}

/// Puts the still-exiled `cards` on the bottom of `player`'s library in a
/// random order.
fn exiled_to_bottom_in_random_order(
    state: &mut GameState,
    player: PlayerId,
    cards: &[ObjectLinkV4],
) {
    let exiled = cards
        .iter()
        .copied()
        .filter(|card| link_live_in(state, *card, Zone::Exile))
        .collect::<Vec<_>>();
    if exiled.is_empty() {
        return;
    }
    for card in &exiled {
        event::propose_and_commit(
            state,
            ProposedEvent::public_library_insert(card.object, event::LibraryPlacement::Bottom),
        );
    }
    let moved = exiled
        .iter()
        .filter(|card| state.objects.get(card.object).zone == Zone::Library)
        .count();
    if state.randomize_library_bottom_v1(player, moved).is_err() {
        halt(state, cards[0].object);
    }
}

/// Puts the library's current top cards `cards`, in order, on its bottom in
/// a random order (e3's Atraxa shape).
fn top_to_bottom_in_random_order(
    state: &mut GameState,
    player: PlayerId,
    cards: &[ObjectLinkV4],
) -> Result<(), String> {
    if cards.is_empty() {
        return Ok(());
    }
    let objects = cards.iter().map(|card| card.object).collect::<Vec<_>>();
    state.apply_scry_result(player, cards, &[], &objects)?;
    state
        .randomize_library_bottom_v1(player, objects.len())
        .map_err(|error| error.to_string())
}

// ---- Spells -------------------------------------------------------------------------

/// Arcane Denial: "Counter target spell. Its controller may draw up to two
/// cards at the beginning of the next turn's upkeep. You draw a card at the
/// beginning of the next turn's upkeep."
pub fn arcane_denial() -> EffectOp {
    e9b(E9bOpV1::ArcaneDenialV1)
}

/// Boomerang Basics: "Return target nonland permanent to its owner's hand.
/// If you controlled that permanent, draw a card."
pub fn boomerang_basics() -> EffectOp {
    e9b(E9bOpV1::BoomerangBasicsV1)
}

/// Bridgeworks Battle: "Target creature you control gets +2/+2 until end of
/// turn. It fights up to one target creature you don't control."
pub fn bridgeworks_battle() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::PumpTargetUntilEndOfTurnDynamic {
            target: TargetRef::Target(0),
            power: crate::card_def::DynamicValueDef::Fixed(2),
            toughness: crate::card_def::DynamicValueDef::Fixed(2),
        },
        EffectOp::FightObjects {
            first: ObjectRef::Target(0),
            second: ObjectRef::Target(1),
            target_spec: TargetSpec::StandardV1(
                StandardTargetV1::ControlledCreatureThenUpToOneOpponentCreatureV1,
            ),
        },
    ])
}

/// Castigate: "Target opponent reveals their hand. You choose a nonland card
/// from it and exile that card."
pub fn castigate() -> EffectOp {
    e9b(E9bOpV1::CastigateV1)
}

/// Echoing Truth: "Return target nonland permanent and all other permanents
/// with the same name as that permanent to their owners' hands."
pub fn echoing_truth() -> EffectOp {
    e9b(E9bOpV1::EchoingTruthV1)
}

/// Grapple with Death: "Destroy target artifact or creature. You gain 1
/// life."
pub fn grapple_with_death() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::DestroyObject {
            object: ObjectRef::Target(0),
        },
        EffectOp::GainLife {
            player: PlayerRef::Controller,
            amount: 1,
        },
    ])
}

/// Growth Spiral: "Draw a card. You may put a land card from your hand onto
/// the battlefield."
pub fn growth_spiral() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 1,
        },
        e9b(E9bOpV1::GrowthSpiralLandV1),
    ])
}

/// "Destroy target artifact" or "Destroy target enchantment" (Heritage
/// Reclamation's first two modes).
pub fn destroy_target() -> EffectOp {
    EffectOp::DestroyObject {
        object: ObjectRef::Target(0),
    }
}

/// Heritage Reclamation's third mode: "Exile up to one target card from a
/// graveyard. Draw a card."
pub fn heritage_reclamation_exile() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::MoveAllTargets {
            to_zone: Zone::Exile,
        },
        EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 1,
        },
    ])
}

/// Mwonvuli Acid-Moss: "Destroy target land. Search your library for a
/// Forest card, put that card onto the battlefield tapped, then shuffle."
pub fn mwonvuli_acid_moss() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::DestroyObject {
            object: ObjectRef::Target(0),
        },
        EffectOp::SearchLibraryCardsToDestination {
            player: PlayerRef::Controller,
            filter: LibraryCardFilter::CardWithAnySubtypeV1([Subtype::Forest, Subtype::Forest]),
            max_targets: 1,
            destination: LibrarySearchDestinationV1::Battlefield { tapped: true },
        },
    ])
}

/// Mystical Tutor: "Search your library for an instant or sorcery card,
/// reveal it, then shuffle and put that card on top."
pub fn mystical_tutor() -> EffectOp {
    EffectOp::SearchLibraryCardsToDestination {
        player: PlayerRef::Controller,
        filter: LibraryCardFilter::InstantOrSorceryCardV1,
        max_targets: 1,
        destination: LibrarySearchDestinationV1::LibraryTopAfterShuffle,
    }
}

/// Pieces of the Puzzle: "Reveal the top five cards of your library. Put up
/// to two instant and/or sorcery cards from among them into your hand and
/// the rest into your graveyard."
pub fn pieces_of_the_puzzle() -> EffectOp {
    e9b(E9bOpV1::PiecesV1)
}

/// Ram Through: "Target creature you control deals damage equal to its
/// power to target creature you don't control. If the creature you control
/// has trample, excess damage is dealt to that creature's controller
/// instead."
pub fn ram_through() -> EffectOp {
    e9b(E9bOpV1::RamThroughV1)
}

/// Respite: "Prevent all combat damage that would be dealt this turn. You
/// gain 1 life for each attacking creature."
pub fn respite() -> EffectOp {
    EffectOp::Sequence(vec![
        crate::eternal_pauper_e6a_v1::moments_peace(),
        e9b(E9bOpV1::RespiteLifeV1),
    ])
}

/// Step Through: "Return two target creatures to their owners' hands."
pub fn step_through() -> EffectOp {
    EffectOp::MoveAllTargets {
        to_zone: Zone::Hand,
    }
}

/// Step Through's "Wizardcycling {2}": "Search your library for a Wizard
/// card, reveal it, put it into your hand, then shuffle."
pub fn wizardcycling() -> EffectOp {
    EffectOp::SearchLibraryToHand {
        player: PlayerRef::Controller,
        filter: LibraryCardFilter::CardWithSubtypeV1(Subtype::Wizard),
    }
}

/// Thermokarst: "Destroy target land. If that land was a snow land, you gain
/// 1 life."
pub fn thermokarst() -> EffectOp {
    e9b(E9bOpV1::ThermokarstV1)
}

/// Vraska's Final Mercy's first mode: "You lose 2 life. Destroy target
/// creature or planeswalker."
pub fn vraskas_final_mercy_destroy() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::LoseLife {
            player: PlayerRef::Controller,
            amount: 2,
        },
        EffectOp::DestroyObject {
            object: ObjectRef::Target(0),
        },
    ])
}

/// Vraska's Final Mercy's second mode: "You lose 2 life. Empower Jace 6."
/// Empower is wave E7b's (`eternal_vintage_e7b_v1`).
pub fn vraskas_final_mercy_empower() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::LoseLife {
            player: PlayerRef::Controller,
            amount: 2,
        },
        EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::E7bV1(
            crate::eternal_vintage_e7b_v1::E7bOpV1::EmpowerJaceV1 { amount: 6 },
        ))),
    ])
}

/// You Meet in a Tavern's first mode, "Form a Party": "Look at the top five
/// cards of your library. You may reveal any number of creature cards from
/// among them and put them into your hand. Put the rest on the bottom of
/// your library in a random order."
pub fn you_meet_in_a_tavern_party() -> EffectOp {
    e9b(E9bOpV1::TavernPartyV1)
}

/// You Meet in a Tavern's second mode, "Start a Brawl": "Creatures you
/// control get +2/+2 until end of turn."
pub fn you_meet_in_a_tavern_brawl() -> EffectOp {
    EffectOp::BoostControlledCreaturesUntilEndOfTurn {
        power: 2,
        toughness: 2,
        keywords: crate::card_def::Keywords::NONE,
    }
}

// ---- Activated abilities ------------------------------------------------------------

/// Dauthi Voidwalker's ability.
pub fn dauthi_voidwalker() -> EffectOp {
    e9b(E9bOpV1::DauthiChooseV1)
}

/// Elvish Spirit Guide's ability, performed without the stack by
/// `special_activation`; the program itself is never resolved.
pub fn spirit_guide_mana() -> EffectOp {
    EffectOp::AddMana {
        player: PlayerRef::Controller,
        colors: vec![ManaColor::G],
    }
}

/// Momentum Breaker's ability.
pub fn momentum_breaker_life() -> EffectOp {
    e9b(E9bOpV1::MomentumLifeV1)
}

/// Oswald Fiddlebender's ability.
pub fn oswald_fiddlebender() -> EffectOp {
    e9b(E9bOpV1::OswaldSearchV1)
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

/// A cast trigger, which functions while its source is a spell.
const fn cast_trigger(effect: fn() -> EffectOp) -> TriggeredAbilityDef {
    TriggeredAbilityDef {
        home_zone: Zone::Stack,
        ..trigger(TriggerCondition::CastSelf, effect)
    }
}

/// A dies trigger, which functions from the graveyard.
const fn dies_trigger(effect: fn() -> EffectOp) -> TriggeredAbilityDef {
    TriggeredAbilityDef {
        home_zone: Zone::Graveyard,
        ..trigger(TriggerCondition::LeftBattlefieldToGraveyard, effect)
    }
}

fn cascade() -> EffectOp {
    e9b(E9bOpV1::CascadeV1)
}

fn create_spawn() -> EffectOp {
    EffectOp::CreateToken {
        token_def: token_id(ELDRAZI_SPAWN_TOKEN),
        controller: PlayerRef::Controller,
    }
}

fn create_treasure() -> EffectOp {
    EffectOp::CreateToken {
        token_def: token_id(TREASURE_TOKEN),
        controller: PlayerRef::Controller,
    }
}

fn draw_one() -> EffectOp {
    EffectOp::DrawCards {
        player: PlayerRef::Controller,
        count: 1,
    }
}

/// Boggart Trawler: "exile target player's graveyard."
fn exile_target_graveyard() -> EffectOp {
    EffectOp::ExilePlayersGraveyard {
        player: PlayerRef::Target(0),
    }
}

/// Cauldron Familiar: "each opponent loses 1 life and you gain 1 life."
fn familiar_drain() -> EffectOp {
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

/// Vampire Sovereign: "target opponent loses 3 life and you gain 3 life."
fn sovereign_drain() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::LoseLife {
            player: PlayerRef::Target(0),
            amount: 3,
        },
        EffectOp::GainLife {
            player: PlayerRef::Controller,
            amount: 3,
        },
    ])
}

fn gain_two() -> EffectOp {
    EffectOp::GainLife {
        player: PlayerRef::Controller,
        amount: 2,
    }
}

/// Simic Growth Chamber: "return a land you control to its owner's hand."
/// Not targeted; chosen on resolution, and it may be this land.
fn return_controlled_land() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::PlayerChoosesControlledPermanent {
        player: PlayerRef::Controller,
        filter: StandardPermanentFilterV1::Land,
        action: StandardChosenActionV1::ReturnToOwnersHand,
    })
}

fn momentum_edict() -> EffectOp {
    e9b(E9bOpV1::MomentumEdictV1)
}

/// Sowing Mycospawn: "search your library for a land card, put it onto the
/// battlefield, then shuffle."
fn mycospawn_search() -> EffectOp {
    EffectOp::SearchLibraryCardsToDestination {
        player: PlayerRef::Controller,
        filter: LibraryCardFilter::AnyLand,
        max_targets: 1,
        destination: LibrarySearchDestinationV1::Battlefield { tapped: false },
    }
}

fn exile_target() -> EffectOp {
    e9b(E9bOpV1::ExileTargetV1)
}

fn thief_exile() -> EffectOp {
    e9b(E9bOpV1::ThiefExileV1)
}

/// Thief of Existence's gained "target opponent draws a card."
fn thief_draw() -> EffectOp {
    EffectOp::DrawCards {
        player: PlayerRef::Target(0),
        count: 1,
    }
}

const CASCADE_TRIGGERS: [TriggeredAbilityDef; 1] = [cast_trigger(cascade)];
const BOGGART_TRAWLER_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, exile_target_graveyard)];
const CAULDRON_FAMILIAR_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, familiar_drain)];
const ELDRAZI_REPURPOSER_TRIGGERS: [TriggeredAbilityDef; 2] =
    [cast_trigger(create_spawn), dies_trigger(create_spawn)];
const JEWEL_THIEF_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, create_treasure)];
const MOMENTUM_BREAKER_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, momentum_edict)];
const NUTRIENT_BLOCK_TRIGGERS: [TriggeredAbilityDef; 1] = [dies_trigger(draw_one)];
const RADIANT_FOUNTAIN_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, gain_two)];
const SIMIC_GROWTH_CHAMBER_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, return_controlled_land)];
/// "When you cast this spell, if it was kicked" is wave E5b's kicked cast
/// condition (Wastescape Battlemage).
const SOWING_MYCOSPAWN_TRIGGERS: [TriggeredAbilityDef; 2] = [
    cast_trigger(mycospawn_search),
    TriggeredAbilityDef {
        home_zone: Zone::Stack,
        ..trigger(
            TriggerCondition::StandardV1(crate::standard_cards_v1::StandardTriggerV1::EternalV1(
                crate::eternal_cards_v1::EternalTriggerV1::E5bV1(
                    crate::eternal_legacy_e5b_v1::E5bTriggerV1::CastSelfIfKickedV1,
                ),
            )),
            exile_target,
        )
    },
];
const THIEF_OF_EXISTENCE_TRIGGERS: [TriggeredAbilityDef; 1] = [cast_trigger(thief_exile)];
const VAMPIRE_SOVEREIGN_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, sovereign_drain)];

/// Triggered abilities of this module's cards. Mai's, Barrowgoyf's,
/// Pestilence's, Thief of Existence's gained trigger and Arcane Denial's are
/// queued by hooks (`delayed_triggers`).
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        ANNOYED_ALTISAUR | BOARDING_PARTY => &CASCADE_TRIGGERS,
        BOGGART_TRAWLER => &BOGGART_TRAWLER_TRIGGERS,
        CAULDRON_FAMILIAR => &CAULDRON_FAMILIAR_TRIGGERS,
        ELDRAZI_REPURPOSER => &ELDRAZI_REPURPOSER_TRIGGERS,
        JEWEL_THIEF => &JEWEL_THIEF_TRIGGERS,
        MOMENTUM_BREAKER => &MOMENTUM_BREAKER_TRIGGERS,
        NUTRIENT_BLOCK => &NUTRIENT_BLOCK_TRIGGERS,
        RADIANT_FOUNTAIN => &RADIANT_FOUNTAIN_TRIGGERS,
        SIMIC_GROWTH_CHAMBER => &SIMIC_GROWTH_CHAMBER_TRIGGERS,
        SOWING_MYCOSPAWN => &SOWING_MYCOSPAWN_TRIGGERS,
        THIEF_OF_EXISTENCE => &THIEF_OF_EXISTENCE_TRIGGERS,
        VAMPIRE_SOVEREIGN => &VAMPIRE_SOVEREIGN_TRIGGERS,
        _ => &[],
    }
}

/// Target specs of this module's triggered abilities, per ability. `None`
/// for every other card.
pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    let own = triggers_for(name)
        .iter()
        .any(|trigger| (trigger.effect)() == *effect)
        || delayed_effect_matches(name, effect);
    if !own {
        return None;
    }
    Some(match name {
        BOGGART_TRAWLER if *effect == exile_target_graveyard() => TargetSpec::AnyPlayer,
        SOWING_MYCOSPAWN if *effect == exile_target() => TargetSpec::Land,
        THIEF_OF_EXISTENCE if *effect == thief_exile() => TargetSpec::StandardV1(
            StandardTargetV1::UpToOneOpponentNoncreatureNonlandPermanentManaValueAtMostV1(4),
        ),
        THIEF_OF_EXISTENCE if *effect == thief_draw() => TargetSpec::TargetOpponent,
        VAMPIRE_SOVEREIGN if *effect == sovereign_drain() => TargetSpec::TargetOpponent,
        _ => TargetSpec::None,
    })
}

/// Queued triggers owned by this module's cards.
pub(crate) fn delayed_effect_matches(name: &str, effect: &EffectOp) -> bool {
    match name {
        ARCANE_DENIAL => {
            *effect == draw_one()
                || matches!(
                    effect,
                    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::E9bV1(
                        E9bOpV1::DenialMayDrawV1 { .. }
                    )))
                )
        }
        THIEF_OF_EXISTENCE => *effect == thief_draw(),
        MAI => matches!(
            effect,
            EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::E9bV1(
                E9bOpV1::MaiLoseLifeV1 { .. }
            )))
        ),
        BARROWGOYF => matches!(
            effect,
            EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::E9bV1(
                E9bOpV1::BarrowgoyfV1 { .. }
            )))
        ),
        PESTILENCE => *effect == e9b(E9bOpV1::PestilenceEndV1),
        _ => false,
    }
}

// ---- Effect leaves ------------------------------------------------------------------

pub(crate) fn execute(op: &E9bOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        E9bOpV1::CascadeBottomV1 { player, cards } => {
            exiled_to_bottom_in_random_order(state, *player, cards);
        }
        E9bOpV1::RespiteLifeV1 => {
            let attackers = state
                .engine
                .combat
                .attackers
                .iter()
                .filter(|&&object| {
                    state
                        .objects
                        .try_get(object)
                        .is_some_and(|live| live.zone == Zone::Battlefield)
                        && is_creature(state, object)
                })
                .count();
            let amount = i32::try_from(attackers).unwrap_or(i32::MAX);
            if amount > 0 {
                event::propose_and_commit(state, ProposedEvent::life_gain(ctx.controller, amount));
            }
        }
        E9bOpV1::EchoingTruthV1 => {
            let Some(target) = bound_target_in(ctx, state, 0, Zone::Battlefield) else {
                return;
            };
            let name = shown_name(state, target);
            let mut objects = vec![target];
            if let Some(name) = name {
                objects.extend(
                    battlefield(state).filter(|&object| {
                        object != target && shown_name(state, object) == Some(name)
                    }),
                );
            }
            event::propose_and_commit_batch(
                state,
                objects
                    .into_iter()
                    .map(|object| ProposedEvent::zone_change(object, Zone::Hand))
                    .collect(),
            );
        }
        E9bOpV1::ArcaneDenialV1 => {
            let Some(spell) = bound_target_in(ctx, state, 0, Zone::Stack) else {
                return;
            };
            let Some(player) = spell_controller(state, spell) else {
                return;
            };
            crate::effect::execute(
                &EffectOp::Conditional {
                    cond: EffectCond::TargetSpellCanBeCountered(0),
                    then: Box::new(EffectOp::MoveObject {
                        object: ObjectRef::Target(0),
                        to_zone: Zone::Graveyard,
                    }),
                    else_: Box::new(nothing()),
                },
                ctx,
                state,
            );
            let record = DenialDrawV1 {
                card: ctx.source,
                controller: ctx.controller,
                player,
                turn: state.turn_key_v1(),
            };
            e9b_state(state).denial_draws.push(record);
        }
        E9bOpV1::ThermokarstV1 => {
            let Some(target) = bound_target_in(ctx, state, 0, Zone::Battlefield) else {
                return;
            };
            let snow =
                crate::engine::effective_supertypes(state, target).contains(&Supertype::Snow);
            crate::effect::execute(&destroy_target(), ctx, state);
            if snow {
                event::propose_and_commit(state, ProposedEvent::life_gain(ctx.controller, 1));
            }
        }
        E9bOpV1::RamThroughV1 => ram_through_damage(ctx, state),
        E9bOpV1::MomentumLifeV1 => {
            // "Start your engines!" is the Standard speed designation.
            let speed = state
                .speed_v1
                .map_or(0, |speed| speed.speeds[ctx.controller.index()]);
            if speed > 0 {
                event::propose_and_commit(
                    state,
                    ProposedEvent::life_gain(ctx.controller, i32::from(speed)),
                );
            }
        }
        E9bOpV1::PestilenceEndV1 => {
            let Some(contract) = ctx.ability_source_contract else {
                return;
            };
            let pestilence = ObjectLinkV4 {
                object: contract.source,
                zone_change_count: contract.zone_change_count,
            };
            // The intervening "if", checked again on resolution.
            if battlefield(state).any(|object| is_creature(state, object))
                || !link_live_in(state, pestilence, Zone::Battlefield)
            {
                return;
            }
            sacrifice(state, pestilence.object);
        }
        E9bOpV1::MaiLoseLifeV1 { player } => {
            event::propose_and_commit(state, ProposedEvent::life_loss(*player, 2));
        }
        E9bOpV1::ThiefExileV1 => {
            let Some(target) = bound_target_in(ctx, state, 0, Zone::Battlefield) else {
                return;
            };
            event::propose_and_commit(state, ProposedEvent::zone_change(target, Zone::Exile));
            if state.objects.get(target).zone != Zone::Exile {
                return;
            }
            // "If you do, Thief of Existence gains ...": the Thief is still a
            // spell, and the ability belongs to the permanent it becomes.
            let Some(contract) = ctx.ability_source_contract else {
                return;
            };
            let grant = ObjectLinkV4 {
                object: contract.source,
                zone_change_count: contract.zone_change_count.saturating_add(1),
            };
            e9b_state(state).thief_grants.push(grant);
        }
        E9bOpV1::ExileTargetV1 => {
            if let Some(target) = bound_target_in(ctx, state, 0, Zone::Battlefield) {
                event::propose_and_commit(state, ProposedEvent::zone_change(target, Zone::Exile));
            }
        }
        // Choices run in `drive`; the plain executor never reaches them.
        E9bOpV1::CascadeV1
        | E9bOpV1::CascadeCastV1 { .. }
        | E9bOpV1::BoomerangBasicsV1
        | E9bOpV1::DenialMayDrawV1 { .. }
        | E9bOpV1::CastigateV1
        | E9bOpV1::GrowthSpiralLandV1
        | E9bOpV1::PiecesV1
        | E9bOpV1::TavernPartyV1
        | E9bOpV1::BarrowgoyfV1 { .. }
        | E9bOpV1::BarrowgoyfMillV1 { .. }
        | E9bOpV1::MomentumEdictV1
        | E9bOpV1::DauthiChooseV1
        | E9bOpV1::OswaldSearchV1 => halt(state, ctx.source),
    }
}

/// Ram Through: the creature you control deals damage equal to its power;
/// with trample, only lethal damage goes to the creature and the rest to its
/// controller.
fn ram_through_damage(ctx: &ExecCtx, state: &mut GameState) {
    let Some(source) = bound_target_in(ctx, state, 0, Zone::Battlefield) else {
        return;
    };
    let Some(victim) = bound_target_in(ctx, state, 1, Zone::Battlefield) else {
        return;
    };
    if !is_creature(state, source) || !is_creature(state, victim) {
        return;
    }
    let power = crate::engine::effective_power(state, source).max(0);
    if power == 0 {
        return;
    }
    let trample =
        crate::engine::has_effective_keyword(state, source, crate::card_def::Keywords::TRAMPLE);
    if !trample {
        event::propose_and_commit(
            state,
            ProposedEvent::damage(source, Target::Object(victim), power),
        );
        return;
    }
    // 702.19c: lethal damage accounts for damage already marked and for
    // deathtouch.
    let deathtouch =
        crate::engine::has_effective_keyword(state, source, crate::card_def::Keywords::DEATHTOUCH);
    let marked = i32::try_from(state.objects.get(victim).damage).unwrap_or(i32::MAX);
    let lethal = if deathtouch {
        1
    } else {
        (crate::engine::effective_toughness(state, victim) - marked).max(0)
    };
    let to_creature = power.min(lethal);
    let excess = power - to_creature;
    let controller = state.objects.get(victim).controller;
    let mut events = Vec::new();
    if to_creature > 0 {
        events.push(ProposedEvent::damage(
            source,
            Target::Object(victim),
            to_creature,
        ));
    }
    if excess > 0 {
        events.push(ProposedEvent::damage(
            source,
            Target::Player(controller),
            excess,
        ));
    }
    event::propose_and_commit_batch(state, events);
}

/// Runs a leaf inside the resumable interpreter. `Ok(true)` when it staged a
/// choice and the continuation must suspend.
pub(crate) fn drive(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    op: &E9bOpV1,
    path: Vec<u16>,
) -> Result<bool, String> {
    let controller = continuation.ctx.controller;
    match op {
        E9bOpV1::CascadeV1 => {
            cascade_exile(state, continuation, &path);
            Ok(false)
        }
        E9bOpV1::CascadeCastV1 { card } => {
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
        E9bOpV1::BoomerangBasicsV1 => {
            let Some(target) = bound_target_in(&continuation.ctx, state, 0, Zone::Battlefield)
            else {
                return Ok(false);
            };
            let controlled = state.objects.get(target).controller == controller;
            event::propose_and_commit(state, ProposedEvent::zone_change(target, Zone::Hand));
            if controlled {
                push_frame(continuation, draw_one(), &path);
            }
            Ok(false)
        }
        E9bOpV1::DenialMayDrawV1 { player } => {
            let (who, chooser) = if *player == controller {
                (PlayerRef::Controller, PlayerRef::Controller)
            } else {
                (PlayerRef::Opponent, PlayerRef::Opponent)
            };
            push_frame(
                continuation,
                EffectOp::Choice {
                    controller: chooser,
                    options: vec![
                        nothing(),
                        EffectOp::DrawCards {
                            player: who,
                            count: 1,
                        },
                        EffectOp::DrawCards {
                            player: who,
                            count: 2,
                        },
                    ],
                },
                &path,
            );
            Ok(false)
        }
        E9bOpV1::CastigateV1 => {
            let Some(&Target::Player(player)) = continuation.ctx.targets.first() else {
                return Ok(false);
            };
            if !continuation.ctx.target_incarnation_matches(0, state) {
                return Ok(false);
            }
            let hand = state.players[player.index()].hand.clone();
            for &card in &hand {
                state.reveal_hand_card(player.opponent(), player, card)?;
            }
            let cards = hand
                .into_iter()
                .filter(|&card| definition(state, card).is_some_and(|def| !def.is_land))
                .map(|card| bind(state, card))
                .collect::<Vec<_>>();
            if cards.is_empty() {
                return Ok(false);
            }
            offer(
                continuation,
                E9bChoiceV1::CastigateV1,
                controller,
                cards,
                (1, 1),
                path,
            );
            Ok(true)
        }
        E9bOpV1::GrowthSpiralLandV1 => {
            let lands = state.players[controller.index()]
                .hand
                .iter()
                .copied()
                .filter(|&card| definition(state, card).is_some_and(|def| def.is_land))
                .map(|card| bind(state, card))
                .collect::<Vec<_>>();
            if lands.is_empty() {
                return Ok(false);
            }
            offer(
                continuation,
                E9bChoiceV1::GrowthSpiralV1,
                controller,
                lands,
                (0, 1),
                path,
            );
            Ok(true)
        }
        E9bOpV1::PiecesV1 => {
            let revealed = state.players[controller.index()]
                .library
                .iter()
                .take(5)
                .map(|&card| bind(state, card))
                .collect::<Vec<_>>();
            for observer in [PlayerId::P0, PlayerId::P1] {
                state.reveal_library_top(observer, controller, revealed.len());
            }
            let spells = revealed
                .iter()
                .copied()
                .filter(|card| {
                    definition(state, card.object).is_some_and(|def| {
                        def.has_type(CardType::Instant) || def.has_type(CardType::Sorcery)
                    })
                })
                .collect::<Vec<_>>();
            if spells.is_empty() {
                finish_pieces(state, &revealed, &[]);
                return Ok(false);
            }
            let max = u16::try_from(spells.len().min(2)).unwrap_or(2);
            offer(
                continuation,
                E9bChoiceV1::PiecesV1 { revealed },
                controller,
                spells,
                (0, max),
                path,
            );
            Ok(true)
        }
        E9bOpV1::TavernPartyV1 => {
            let looked = state.players[controller.index()]
                .library
                .iter()
                .take(5)
                .map(|&card| bind(state, card))
                .collect::<Vec<_>>();
            state.reveal_library_top(controller, controller, looked.len());
            let creatures = looked
                .iter()
                .copied()
                .filter(|card| {
                    definition(state, card.object)
                        .is_some_and(|def| def.has_type(CardType::Creature))
                })
                .collect::<Vec<_>>();
            if creatures.is_empty() {
                finish_tavern(state, controller, &looked, &[])?;
                return Ok(false);
            }
            let max = u16::try_from(creatures.len()).unwrap_or(u16::MAX);
            offer(
                continuation,
                E9bChoiceV1::TavernV1 { looked },
                controller,
                creatures,
                (0, max),
                path,
            );
            Ok(true)
        }
        E9bOpV1::BarrowgoyfV1 { amount } => {
            if *amount > 0 && !state.players[controller.index()].library.is_empty() {
                push_choice(
                    continuation,
                    vec![
                        e9b(E9bOpV1::BarrowgoyfMillV1 { amount: *amount }),
                        nothing(),
                    ],
                    &path,
                );
            }
            Ok(false)
        }
        E9bOpV1::BarrowgoyfMillV1 { amount } => {
            let count = usize::try_from(*amount).unwrap_or(0);
            let milled = state.players[controller.index()]
                .library
                .iter()
                .take(count)
                .copied()
                .collect::<Vec<_>>();
            event::propose_and_commit_batch(
                state,
                milled
                    .iter()
                    .map(|&card| ProposedEvent::zone_change(card, Zone::Graveyard))
                    .collect(),
            );
            let creatures = milled
                .into_iter()
                .filter(|&card| {
                    state.objects.get(card).zone == Zone::Graveyard
                        && definition(state, card)
                            .is_some_and(|def| def.has_type(CardType::Creature))
                })
                .map(|card| bind(state, card))
                .collect::<Vec<_>>();
            if creatures.is_empty() {
                return Ok(false);
            }
            offer(
                continuation,
                E9bChoiceV1::BarrowgoyfV1,
                controller,
                creatures,
                (0, 1),
                path,
            );
            Ok(true)
        }
        E9bOpV1::MomentumEdictV1 => {
            let opponent = controller.opponent();
            let victims = battlefield(state)
                .filter(|&object| {
                    state.objects.get(object).controller == opponent
                        && (is_creature(state, object)
                            || crate::engine::has_effective_subtype(
                                state,
                                object,
                                Subtype::Vehicle,
                            ))
                })
                .map(|object| bind(state, object))
                .collect::<Vec<_>>();
            if victims.is_empty() {
                // "Each opponent who can't discards a card."
                push_frame(
                    continuation,
                    EffectOp::DiscardCards {
                        player: PlayerRef::Opponent,
                        count: 1,
                    },
                    &path,
                );
                return Ok(false);
            }
            offer(
                continuation,
                E9bChoiceV1::MomentumSacrificeV1,
                opponent,
                victims,
                (1, 1),
                path,
            );
            Ok(true)
        }
        E9bOpV1::DauthiChooseV1 => {
            let opponent = controller.opponent();
            let cards = read_state(state)
                .map(|e9b| e9b.void_cards.clone())
                .unwrap_or_default()
                .into_iter()
                .filter(|card| {
                    link_live_in(state, *card, Zone::Exile)
                        && state.objects.get(card.object).owner == opponent
                })
                .map(|card| bind(state, card.object))
                .collect::<Vec<_>>();
            if cards.is_empty() {
                return Ok(false);
            }
            offer(
                continuation,
                E9bChoiceV1::DauthiV1,
                controller,
                cards,
                (1, 1),
                path,
            );
            Ok(true)
        }
        E9bOpV1::OswaldSearchV1 => {
            let paid = continuation
                .ctx
                .paid_cost_refs
                .iter()
                .find(|paid| {
                    CARD_DEFS
                        .get(paid.card_def as usize)
                        .is_some_and(|def| def.has_type(CardType::Artifact))
                })
                .and_then(|paid| CARD_DEFS.get(paid.card_def as usize))
                .map(|def| def.mana_value);
            let Some(mana_value) = paid else {
                return Ok(false);
            };
            push_frame(
                continuation,
                EffectOp::SearchLibraryCardsToDestination {
                    player: PlayerRef::Controller,
                    filter: LibraryCardFilter::ArtifactWithManaValue(mana_value.saturating_add(1)),
                    max_targets: 1,
                    destination: LibrarySearchDestinationV1::Battlefield { tapped: false },
                },
                &path,
            );
            Ok(false)
        }
        other => {
            execute(other, &continuation.ctx, state);
            Ok(false)
        }
    }
}

/// Cascade: "exile cards from the top of your library until you exile a
/// nonland card that costs less. You may cast it without paying its mana
/// cost. Put the exiled cards on the bottom in a random order." The spell's
/// mana value is its last known one if it already left the stack.
fn cascade_exile(state: &mut GameState, continuation: &mut EffectContinuation, path: &[u16]) {
    let player = continuation.ctx.controller;
    let source = continuation.ctx.source;
    let bound = match state.objects.try_get(source) {
        Some(live) if live.zone == Zone::Stack => crate::engine::object_mana_value(state, source),
        _ => continuation
            .ctx
            .ability_source_contract
            .and_then(|contract| CARD_DEFS.get(contract.card_def as usize))
            .map_or(0, |def| def.mana_value),
    };
    let mut exiled = Vec::new();
    let mut hit = None;
    while let Some(&top) = state.players[player.index()].library.first() {
        event::propose_and_commit(state, ProposedEvent::zone_change(top, Zone::Exile));
        let live = state.objects.get(top);
        if live.zone != Zone::Exile {
            break;
        }
        exiled.push(live_link(state, top));
        let def = &CARD_DEFS[live.card_def as usize];
        if !def.has_type(CardType::Land) && def.mana_value < bound {
            hit = Some(top);
            break;
        }
    }
    push_frame(
        continuation,
        e9b(E9bOpV1::CascadeBottomV1 {
            player,
            cards: exiled,
        }),
        path,
    );
    let Some(card) = hit else {
        return;
    };
    let binding = bind(state, card);
    if crate::engine::resolution_cast_v1::can_cast_exiled_without_mana(state, player, binding, None)
    {
        push_choice(
            continuation,
            vec![
                e9b(E9bOpV1::CascadeCastV1 {
                    card: live_link(state, card),
                }),
                nothing(),
            ],
            path,
        );
    }
}

/// Pieces of the Puzzle's end: `chosen` to the hand, the rest of `revealed`
/// to the graveyard.
fn finish_pieces(
    state: &mut GameState,
    revealed: &[EffectObjectBinding],
    chosen: &[EffectObjectBinding],
) {
    for card in chosen {
        if binding_live(state, *card) {
            event::propose_and_commit(state, ProposedEvent::zone_change(card.object, Zone::Hand));
        }
    }
    let rest = revealed
        .iter()
        .copied()
        .filter(|card| !chosen.contains(card) && binding_live(state, *card))
        .map(|card| ProposedEvent::zone_change(card.object, Zone::Graveyard))
        .collect::<Vec<_>>();
    event::propose_and_commit_batch(state, rest);
}

/// You Meet in a Tavern's end: `chosen` revealed and put into the hand, the
/// rest of `looked` on the bottom in a random order.
fn finish_tavern(
    state: &mut GameState,
    player: PlayerId,
    looked: &[EffectObjectBinding],
    chosen: &[EffectObjectBinding],
) -> Result<(), String> {
    for card in chosen {
        if !binding_live(state, *card) {
            return Err("a You Meet in a Tavern card left the library".into());
        }
        event::propose_and_commit(state, ProposedEvent::zone_change(card.object, Zone::Hand));
        if state.objects.get(card.object).zone == Zone::Hand {
            state.reveal_hand_card(player.opponent(), player, card.object)?;
        }
    }
    let rest = looked
        .iter()
        .filter(|card| !chosen.contains(card) && binding_live(state, **card))
        .map(|card| ObjectLinkV4 {
            object: card.object,
            zone_change_count: card.expected_zone_change_count,
        })
        .collect::<Vec<_>>();
    top_to_bottom_in_random_order(state, player, &rest)
}

/// Applies an answered choice of this module.
pub(crate) fn finish_selection(
    state: &mut GameState,
    _continuation: &mut EffectContinuation,
    choice: &E3ChoiceV1,
    kind: &E9bChoiceV1,
    bindings: Vec<EffectObjectBinding>,
) -> Result<bool, String> {
    match kind {
        E9bChoiceV1::CastigateV1 => {
            if let Some(card) = bindings.first() {
                event::propose_and_commit(
                    state,
                    ProposedEvent::zone_change(card.object, Zone::Exile),
                );
            }
            Ok(false)
        }
        E9bChoiceV1::GrowthSpiralV1 => {
            if let Some(card) = bindings.first() {
                event::propose_and_commit(
                    state,
                    ProposedEvent::zone_change_to_battlefield_under_controller(
                        card.object,
                        choice.player,
                    ),
                );
            }
            Ok(false)
        }
        E9bChoiceV1::PiecesV1 { revealed } => {
            finish_pieces(state, revealed, &bindings);
            Ok(false)
        }
        E9bChoiceV1::TavernV1 { looked } => {
            finish_tavern(state, choice.player, looked, &bindings)?;
            Ok(false)
        }
        E9bChoiceV1::BarrowgoyfV1 => {
            if let Some(card) = bindings.first() {
                event::propose_and_commit(
                    state,
                    ProposedEvent::zone_change(card.object, Zone::Hand),
                );
            }
            Ok(false)
        }
        E9bChoiceV1::MomentumSacrificeV1 => {
            let [victim] = bindings.as_slice() else {
                return Err("Momentum Breaker's opponent sacrifices exactly one permanent".into());
            };
            if state.objects.get(victim.object).controller != choice.player {
                return Err("the sacrificed permanent changed controller".into());
            }
            sacrifice(state, victim.object);
            Ok(false)
        }
        E9bChoiceV1::DauthiV1 => {
            let [card] = bindings.as_slice() else {
                return Err("Dauthi Voidwalker chooses exactly one card".into());
            };
            let live = state.objects.get(card.object);
            let land = CARD_DEFS
                .get(live.card_def as usize)
                .is_some_and(|def| def.has_type(CardType::Land));
            let permission = crate::engine::PlayPermission {
                without_mana_cost: crate::engine::FreeCastV1(!land),
                object: card.object,
                holder: choice.player,
                zone_change_generation: live.zone_change_count,
                play_or_cast: if land {
                    crate::engine::PlayOrCast::Play
                } else {
                    crate::engine::PlayOrCast::Cast
                },
                expiry: crate::engine::PlayPermissionExpiry::EndOfTurn,
            };
            state.engine.exile_play_permissions.push(permission);
            Ok(false)
        }
    }
}

// ---- Hooks: triggers ----------------------------------------------------------------

/// The triggers this module's hooks set off in this collection, and its
/// bookkeeping.
pub(crate) fn delayed_triggers(
    state: &mut GameState,
    events: &[CommittedEvent],
) -> Vec<PendingTrigger> {
    if !cfg!(feature = "eternal-fixtures") {
        return Vec::new();
    }
    for event in events {
        match *event {
            CommittedEvent::SpellCast { spell, controller } => {
                mai_triggers(state, spell, controller)
            }
            CommittedEvent::CombatDamageToPlayer {
                source,
                source_zone_change_count,
                amount,
                ..
            } => barrowgoyf_trigger(state, source, source_zone_change_count, amount),
            CommittedEvent::BeginningEndStep { .. } => pestilence_triggers(state),
            CommittedEvent::UpkeepBegan { .. } => denial_triggers(state),
            CommittedEvent::ZoneChange {
                object,
                from: Zone::Battlefield,
                controller_before,
                ..
            } => thief_trigger(state, object, controller_before),
            _ => {}
        }
    }
    prune(state);
    let Some(eternal) = state.eternal_v1.as_mut() else {
        return Vec::new();
    };
    if eternal.e9b.queued_triggers.is_empty() {
        return Vec::new();
    }
    std::mem::take(&mut eternal.e9b.queued_triggers)
        .into_iter()
        .map(|queued| {
            let mut pending = crate::eternal_cards_v1::delayed_trigger(
                queued.source,
                queued.controller,
                queued.effect,
            );
            if queued.targets_opponent {
                pending.target_spec = TargetSpec::TargetOpponent;
            }
            pending
        })
        .collect()
}

fn queue_trigger(
    state: &mut GameState,
    source: AbilitySourceContractV4,
    controller: PlayerId,
    effect: EffectOp,
    targets_opponent: bool,
) {
    e9b_state(state).queued_triggers.push(QueuedTriggerV1 {
        source,
        controller,
        effect,
        targets_opponent,
    });
}

/// Mai, Scornful Striker: "Whenever a player casts a noncreature spell, they
/// lose 2 life."
fn mai_triggers(state: &mut GameState, spell: ObjectId, caster: PlayerId) {
    if state
        .objects
        .try_get(spell)
        .is_none_or(|live| live.zone != Zone::Stack)
        || is_creature(state, spell)
    {
        return;
    }
    let mais = battlefield(state)
        .filter(|&object| active_named(state, object, MAI))
        .collect::<Vec<_>>();
    for mai in mais {
        let source = AbilitySourceContractV4::capture(state, mai);
        let controller = source.controller;
        queue_trigger(
            state,
            source,
            controller,
            e9b(E9bOpV1::MaiLoseLifeV1 { player: caster }),
            false,
        );
    }
}

/// Barrowgoyf: "Whenever this creature deals combat damage to a player".
fn barrowgoyf_trigger(state: &mut GameState, source: ObjectId, generation: u32, amount: i32) {
    if amount <= 0
        || !active_named(state, source, BARROWGOYF)
        || state.objects.get(source).zone_change_count != generation
    {
        return;
    }
    let contract = AbilitySourceContractV4::capture(state, source);
    let controller = contract.controller;
    queue_trigger(
        state,
        contract,
        controller,
        e9b(E9bOpV1::BarrowgoyfV1 { amount }),
        false,
    );
}

/// Pestilence: "At the beginning of the end step, if no creatures are on the
/// battlefield, sacrifice this enchantment."
fn pestilence_triggers(state: &mut GameState) {
    if battlefield(state).any(|object| is_creature(state, object)) {
        return;
    }
    let pestilences = battlefield(state)
        .filter(|&object| active_named(state, object, PESTILENCE))
        .collect::<Vec<_>>();
    for pestilence in pestilences {
        let source = AbilitySourceContractV4::capture(state, pestilence);
        let controller = source.controller;
        queue_trigger(
            state,
            source,
            controller,
            e9b(E9bOpV1::PestilenceEndV1),
            false,
        );
    }
}

/// Arcane Denial: "at the beginning of the next turn's upkeep".
fn denial_triggers(state: &mut GameState) {
    let turn = state.turn_key_v1();
    let Some(records) = read_state(state).map(|e9b| e9b.denial_draws.clone()) else {
        return;
    };
    if records.is_empty() {
        return;
    }
    let (due, waiting): (Vec<_>, Vec<_>) =
        records.into_iter().partition(|record| record.turn != turn);
    e9b_state(state).denial_draws = waiting;
    for record in due {
        if state.objects.try_get(record.card).is_none() {
            continue;
        }
        let mut source = AbilitySourceContractV4::capture(state, record.card);
        source.controller = record.controller;
        queue_trigger(
            state,
            source,
            record.controller,
            e9b(E9bOpV1::DenialMayDrawV1 {
                player: record.player,
            }),
            false,
        );
        queue_trigger(state, source, record.controller, draw_one(), false);
    }
}

/// Thief of Existence's gained "When this creature leaves the battlefield,
/// target opponent draws a card."
fn thief_trigger(state: &mut GameState, object: ObjectId, controller: PlayerId) {
    let Some(live) = state.objects.try_get(object) else {
        return;
    };
    let left = ObjectLinkV4 {
        object,
        zone_change_count: live.zone_change_count.saturating_sub(1),
    };
    let granted = read_state(state).is_some_and(|e9b| e9b.thief_grants.contains(&left));
    if !granted {
        return;
    }
    e9b_state(state).thief_grants.retain(|grant| *grant != left);
    let mut source = AbilitySourceContractV4::capture(state, object);
    source.controller = controller;
    queue_trigger(state, source, controller, thief_draw(), true);
}

/// Drops records whose objects are gone.
fn prune(state: &mut GameState) {
    let Some(e9b) = read_state(state) else {
        return;
    };
    let void_cards = e9b
        .void_cards
        .iter()
        .copied()
        .filter(|card| link_live_in(state, *card, Zone::Exile))
        .collect::<Vec<_>>();
    // A grant waits for the Thief's next incarnation: on the stack it is
    // one generation behind, on the battlefield it is that generation.
    let thief_grants = e9b
        .thief_grants
        .iter()
        .copied()
        .filter(|grant| {
            state.objects.try_get(grant.object).is_some_and(|live| {
                (live.zone == Zone::Stack
                    && live.zone_change_count.saturating_add(1) == grant.zone_change_count)
                    || (live.zone == Zone::Battlefield
                        && live.zone_change_count == grant.zone_change_count)
            })
        })
        .collect::<Vec<_>>();
    if void_cards.len() != e9b.void_cards.len() || thief_grants.len() != e9b.thief_grants.len() {
        let e9b = e9b_state(state);
        e9b.void_cards = void_cards;
        e9b.thief_grants = thief_grants;
    }
}

// ---- Hooks: engine ------------------------------------------------------------------

/// Barrowgoyf: "Barrowgoyf's power is equal to the number of card types
/// among cards in all graveyards and its toughness is equal to that number
/// plus 1." A characteristic-defining ability works in every zone. `None`
/// for every other object.
pub(crate) fn characteristic_defining_power(state: &GameState, id: ObjectId) -> Option<i32> {
    if definition_name(state, id)? != BARROWGOYF {
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

/// Whether `object` has shadow (Dauthi Voidwalker).
fn has_shadow(state: &GameState, object: ObjectId) -> bool {
    active_named(state, object, DAUTHI_VOIDWALKER)
}

/// Shadow (702.28b): "This creature can block or be blocked by only
/// creatures with shadow."
pub(crate) fn blocker_allowed(state: &GameState, attacker: ObjectId, blocker: ObjectId) -> bool {
    has_shadow(state, attacker) == has_shadow(state, blocker)
}

/// Dauthi Voidwalker: "If a card would be put into an opponent's graveyard
/// from anywhere, instead exile it with a void counter on it." `record` is
/// false for a projected change, which must not leave a counter behind.
pub(crate) fn replace_zone_change(
    state: &mut GameState,
    change: &mut event::ZoneChangeProposed,
    record: bool,
) {
    if change.to_zone != Zone::Graveyard || !is_card(state, change.object) {
        return;
    }
    let Some(live) = state.objects.try_get(change.object) else {
        return;
    };
    let owner = live.owner;
    let generation = live.zone_change_count;
    let voided = battlefield(state).any(|object| {
        active_named(state, object, DAUTHI_VOIDWALKER)
            && state.objects.get(object).controller != owner
            && object != change.object
    });
    if !voided {
        return;
    }
    change.to_zone = Zone::Exile;
    if record {
        let link = ObjectLinkV4 {
            object: change.object,
            zone_change_count: generation.saturating_add(1),
        };
        e9b_state(state).void_cards.push(link);
    }
}

/// Whether `card` is an exiled card with a void counter on it.
pub fn has_void_counter(state: &GameState, card: ObjectId) -> bool {
    let Some(live) = state.objects.try_get(card) else {
        return false;
    };
    live.zone == Zone::Exile
        && read_state(state).is_some_and(|e9b| {
            e9b.void_cards.contains(&ObjectLinkV4 {
                object: card,
                zone_change_count: live.zone_change_count,
            })
        })
}

/// Whether activating `source`'s ability `index` is one this module performs
/// without the stack: Elvish Spirit Guide's mana ability.
pub(crate) fn is_special_activation(state: &GameState, source: ObjectId, index: u8) -> bool {
    index == 0
        && state
            .objects
            .try_get(source)
            .is_some_and(|live| live.zone == Zone::Hand)
        && definition_name(state, source) == Some(ELVISH_SPIRIT_GUIDE)
}

/// Elvish Spirit Guide: "Exile this card from your hand: Add {G}." A mana
/// ability (605.1a), so it never uses the stack.
pub(crate) fn special_activation(
    state: &mut GameState,
    player: PlayerId,
    source: ObjectId,
) -> Result<(), String> {
    if !is_special_activation(state, source, 0)
        || !state.players[player.index()].hand.contains(&source)
    {
        return Err("not a special activation".to_string());
    }
    event::propose_and_commit(state, ProposedEvent::zone_change(source, Zone::Exile));
    if state.objects.get(source).zone != Zone::Exile {
        return Err("Elvish Spirit Guide stayed in its owner's hand".to_string());
    }
    event::propose_and_commit(state, ProposedEvent::mana_add(player, vec![ManaColor::G]));
    Ok(())
}

/// This module's Auras that the Standard Aura entry attaches: Wild Growth
/// ("Enchant land").
pub(crate) fn is_aura(def: &CardDef) -> bool {
    def.name == WILD_GROWTH
}

/// Whether this module's Aura `def` may enchant `host`. `None` for every
/// other definition.
pub(crate) fn aura_host_legal(def: &CardDef, host: ObjectId, state: &GameState) -> Option<bool> {
    (def.name == WILD_GROWTH).then(|| {
        state.objects.get(host).zone == Zone::Battlefield
            && crate::engine::object_has_type(state, host, CardType::Land)
    })
}

/// Wild Growth: "Whenever enchanted land is tapped for mana, its controller
/// adds an additional {G}." A triggered mana ability (605.1b): it resolves at
/// once. Called after `land` was tapped for mana.
pub(crate) fn after_tapped_for_mana(state: &mut GameState, land: ObjectId) {
    if state.objects.get(land).zone != Zone::Battlefield {
        return;
    }
    let host = Some(live_link(state, land));
    let growths = battlefield(state)
        .filter(|&aura| {
            active_named(state, aura, WILD_GROWTH) && state.objects.get(aura).v4.attached_to == host
        })
        .count();
    if growths == 0 {
        return;
    }
    let controller = state.objects.get(land).controller;
    event::propose_and_commit(
        state,
        ProposedEvent::mana_add(controller, vec![ManaColor::G; growths]),
    );
}

/// Modal double-faced cards whose back face is a land played from hand.
pub(crate) fn has_land_back_face(name: &str) -> bool {
    matches!(name, BOGGART_TRAWLER | BRIDGEWORKS_BATTLE)
}

/// A modal double-faced card's mana ability belongs to its land face.
pub(crate) fn mana_abilities_active(
    state: &GameState,
    object: ObjectId,
    name: &str,
) -> Option<bool> {
    has_land_back_face(name).then(|| state.objects.get(object).v4.face_index == 1)
}

/// "Start your engines!" (Momentum Breaker), read by the Standard speed
/// designation.
pub(crate) fn starts_engines(name: &str) -> bool {
    cfg!(feature = "eternal-fixtures") && name == MOMENTUM_BREAKER
}

// ---- Targets ------------------------------------------------------------------------

/// Target filters of this module's cards, carried as `StandardTargetV1`
/// variants. `standard_cards_v1::legal_targets` and `target_matches` ask
/// these first.
pub(crate) mod targets {
    use crate::card_def::CardType;
    use crate::ids::{ObjectId, PlayerId};
    use crate::standard_cards_v1::StandardTargetV1;
    use crate::state::{GameState, Target, Zone};

    /// Whether `target` is one of this module's filters.
    pub(crate) fn owns(target: StandardTargetV1) -> bool {
        matches!(
            target,
            StandardTargetV1::ControlledCreatureThenUpToOneOpponentCreatureV1
                | StandardTargetV1::TwoDistinctCreaturesV1
                | StandardTargetV1::UpToOneOpponentNoncreatureNonlandPermanentManaValueAtMostV1(_)
        )
    }

    fn creature(state: &GameState, object: ObjectId) -> bool {
        crate::engine::object_has_type(state, object, CardType::Creature)
    }

    /// Whether `object` satisfies this module's filter `target` for
    /// `controller`, in any slot, or `None` for every other filter.
    pub(crate) fn target_matches(
        target: StandardTargetV1,
        controller: PlayerId,
        object: ObjectId,
        state: &GameState,
    ) -> Option<bool> {
        if !owns(target) {
            return None;
        }
        let Some(live) = state.objects.try_get(object) else {
            return Some(false);
        };
        let permanent = live.zone == Zone::Battlefield;
        Some(match target {
            StandardTargetV1::ControlledCreatureThenUpToOneOpponentCreatureV1
            | StandardTargetV1::TwoDistinctCreaturesV1 => permanent && creature(state, object),
            StandardTargetV1::UpToOneOpponentNoncreatureNonlandPermanentManaValueAtMostV1(
                maximum,
            ) => {
                permanent
                    && live.controller != controller
                    && !creature(state, object)
                    && !crate::engine::object_has_type(state, object, CardType::Land)
                    && crate::engine::object_mana_value(state, object) <= u16::from(maximum)
            }
            _ => return None,
        })
    }

    /// The legal choices for this module's filter `target` after the already
    /// chosen `prefix`, or `None` for every other filter.
    pub(crate) fn legal_targets(
        target: StandardTargetV1,
        controller: PlayerId,
        prefix: &[Target],
        state: &GameState,
    ) -> Option<Vec<Target>> {
        if !owns(target) {
            return None;
        }
        let permanents = [PlayerId::P0, PlayerId::P1]
            .into_iter()
            .flat_map(|player| state.players[player.index()].battlefield.iter().copied());
        Some(match target {
            // "Target creature you control", then "up to one target creature
            // you don't control".
            StandardTargetV1::ControlledCreatureThenUpToOneOpponentCreatureV1 => {
                let yours = match prefix.len() {
                    0 => true,
                    1 => false,
                    _ => return Some(Vec::new()),
                };
                permanents
                    .filter(|&object| {
                        creature(state, object)
                            && (state.objects.get(object).controller == controller) == yours
                    })
                    .map(Target::Object)
                    .collect()
            }
            // "Two target creatures": two different creatures (115.3).
            StandardTargetV1::TwoDistinctCreaturesV1 => {
                if prefix.len() >= 2 {
                    return Some(Vec::new());
                }
                permanents
                    .filter(|&object| creature(state, object))
                    .map(Target::Object)
                    .filter(|candidate| !prefix.contains(candidate))
                    .collect()
            }
            _ => {
                if !prefix.is_empty() {
                    return Some(Vec::new());
                }
                permanents
                    .filter(|&object| {
                        target_matches(target, controller, object, state) == Some(true)
                    })
                    .map(Target::Object)
                    .collect()
            }
        })
    }
}
