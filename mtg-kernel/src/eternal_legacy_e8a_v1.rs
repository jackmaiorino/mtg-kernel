//! Card programs for the eternal catalog's wave E8a
//! (`docs/design/eternal_formats_support_v1.md`): Legacy Death and Taxes
//! with Yorion, Hogaak, the Jeskai, Azorius and Dimir control lists, Beseech
//! and Saga Storm, Reanimator and 8-Cast.
//!
//! `eternal_cards_v1` forwards its effect leaves, trigger conditions and
//! state here, and the engine calls the hooks below only in
//! `eternal-fixtures` builds. Every table is keyed by a printed name that
//! occurs only in the eternal registry, so the Pauper, FDN and Standard
//! catalogs never reach it. Behavior was read from each card's current
//! Oracle text and, where present, its XMage source.
//!
//! Primitives added here:
//! - Yorion's companion condition and its exile of any number of your
//!   nonland permanents, returned at the next end step (as is
//!   Flickerwisp's target); the return waits for a later end step when it
//!   is created during one;
//! - Equipment entry triggers (Meteor Sword, Pre-War Formalwear's return
//!   and attach, Lich's Relic's paid reflexive trigger), Stoneforge
//!   Mystic's search and put, and reconfigure (Lion Sash: attach, unattach,
//!   not a creature while attached);
//! - Hogaak's mana-free payment: convoked creatures pay its hybrid pips and
//!   generic mana, delve pays the rest (`statics::hogaak_payment`), and its
//!   graveyard cast permission;
//! - Teferi, Time Raveler's sorcery-speed restriction and sorcery flash,
//!   Harbinger of the Seas' land type change, Back to Basics' untap
//!   prohibition, Rain of Filth's granted mana ability;
//! - Chandra's surveil return, restricted mana for noncreature spells,
//!   stun counters and spell-cast emblem;
//! - Emperor of Bones' linked exile, adapt and finality reanimation, Cloak
//!   and Dagger's hand-or-creature exile until it leaves, Skyclave
//!   Apparition's Illusion of the exiled card's mana value;
//! - Wishclaw Talisman's wish counters and control change, Aether Vial's
//!   charge counters, Once Upon a Time's first-spell free cast, Valgavoth's
//!   Lair's chosen color.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

pub mod statics;
pub(crate) mod targets;

use crate::card_def::{
    card_id_by_name, CardDef, CardType, Keywords, Subtype, TargetSpec, CARD_DEFS,
};
use crate::effect::{
    EffectContinuation, EffectFrame, EffectObjectBinding, EffectOp, ExecCtx, LibraryLookCount,
    LibraryPickFilterV1, LibraryPickRule, LibraryPickSelectionV1, ObjectRef, PlayerRef,
};
use crate::eternal_cards_v1::e3::{E3ChoiceKindV1, E3ChoiceV1};
use crate::eternal_cards_v1::EternalOpV1;
use crate::event::{self, CommittedEvent, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::mana::ManaColor;
use crate::standard_cards_v1::{StandardOpV1, StandardTargetV1, StandardTriggerV1};
use crate::state::{
    AbilitySourceContractV4, CastMethodV4, GameState, ObjectLinkV4, StackItemKind, Step, Target,
    Zone,
};
use crate::trigger::{PendingTrigger, TriggerCondition, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};

// ---- State ------------------------------------------------------------------------

/// Per-game state for this module's cards, kept inside
/// `eternal_cards_v1::EternalStateV1`. Default (and absent from snapshots)
/// until one of these cards needs it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct E8aStateV1 {
    /// Triggers detected by hooks, waiting for the next trigger collection.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub queued_triggers: Vec<QueuedTriggerV1>,
    /// Exiled cards returning at the beginning of the next end step
    /// (Flickerwisp, Yorion).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub end_step_returns: Vec<EndStepActionV1>,
    /// Creatures Emperor of Bones put onto the battlefield, sacrificed at
    /// the beginning of the next end step.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub end_step_sacrifices: Vec<EndStepActionV1>,
    /// Cards exiled with each Emperor of Bones incarnation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub emperor_exiles: Vec<(ObjectLinkV4, ObjectLinkV4)>,
    /// Skyclave Apparition incarnations and the mana value and owner of the
    /// card each exiled.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skyclave_exiles: Vec<(ObjectLinkV4, u16, PlayerId)>,
    /// Illusion tokens and their power and toughness.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub illusions: Vec<(ObjectLinkV4, u16)>,
    /// Cards Cloak and Dagger exiled until it leaves the battlefield.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cloak_exiles: Vec<CloakExileV1>,
    /// Wish counters on each Wishclaw Talisman incarnation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub wish_counters: Vec<(ObjectLinkV4, u8)>,
    /// Lands that have Rain of Filth's ability, and the turn they have it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rain_lands: Vec<(ObjectLinkV4, TurnV1)>,
    /// Harbinger of the Seas incarnations and the timestamp each entered
    /// with.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub harbingers: Vec<(ObjectLinkV4, u64)>,
    /// Players who may cast sorceries as though they had flash until their
    /// next turn (Teferi, Time Raveler's +1).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sorcery_flash: Vec<PlayerId>,
    /// Chandra, Chill of Compliance emblems: their owner and the
    /// planeswalker that made each one.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chandra_emblems: Vec<(PlayerId, AbilitySourceContractV4)>,
    /// Whether each player has cast a spell this game, in games with a Once
    /// Upon a Time.
    #[serde(default, skip_serializing_if = "no_spells")]
    pub spells_cast: [bool; 2],
}

fn no_spells(cast: &[bool; 2]) -> bool {
    !cast[0] && !cast[1]
}

impl E8aStateV1 {
    pub(crate) fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// One turn: its key and active player.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TurnV1 {
    pub turn: u32,
    pub active: PlayerId,
}

impl TurnV1 {
    fn now(state: &GameState) -> Self {
        Self {
            turn: state.turn_key_v1(),
            active: state.active_player,
        }
    }
}

/// A delayed "at the beginning of the next end step" action on `cards`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EndStepActionV1 {
    pub source: AbilitySourceContractV4,
    pub controller: PlayerId,
    pub cards: Vec<ObjectLinkV4>,
    /// The turn this was created during its end step, whose end step has
    /// already begun; it waits for a later one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_in_end_step: Option<TurnV1>,
}

/// A card Cloak and Dagger exiled until it leaves the battlefield.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CloakExileV1 {
    /// The Cloak and Dagger incarnation.
    pub source: ObjectLinkV4,
    /// The exiled card's exile incarnation.
    pub card: ObjectLinkV4,
    /// Whether it returns to the battlefield (it was a creature) rather than
    /// to its owner's hand.
    pub to_battlefield: bool,
}

/// A triggered ability queued by a hook.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueuedTriggerV1 {
    pub source: AbilitySourceContractV4,
    pub controller: PlayerId,
    pub op: E8aOpV1,
    /// The target the ability's controller announces, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_spec: Option<TargetSpec>,
}

pub(crate) fn e8a_state(state: &mut GameState) -> &mut E8aStateV1 {
    &mut state.eternal_v1.get_or_insert_with(Default::default).e8a
}

pub(crate) fn read_state(state: &GameState) -> Option<&E8aStateV1> {
    state.eternal_v1.as_ref().map(|eternal| &eternal.e8a)
}

/// Trigger conditions owned by this module, carried as
/// `eternal_cards_v1::EternalTriggerV1::E8aV1`. Each checks its own source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum E8aTriggerV1 {
    /// "When this land enters untapped" (Mystic Sanctuary).
    EntersUntappedV1,
    /// "Whenever one or more +1/+1 counters are put on this creature"
    /// (Emperor of Bones).
    PlusOneCountersPutOnSelfV1,
}

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::E8aV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum E8aOpV1 {
    /// Ad Nauseam: reveal the top card, put it into hand, lose life equal to
    /// its mana value, then the controller may repeat. A choice.
    AdNauseamV1,
    /// Cabal Ritual: add {B}{B}{B}, or {B}{B}{B}{B}{B} with threshold.
    CabalRitualV1,
    /// Canoptek Scarab Swarm: exile target player's graveyard, then an
    /// Insect for each artifact or land card exiled.
    CanoptekV1,
    /// Chandra's first +1: surveil 1, then the return. A choice.
    ChandraSurveilV1,
    /// The return of Chandra's surveil: `card` (the top card it looked at),
    /// to hand if it went to the graveyard and is noncreature, nonland.
    ChandraReturnV1 { card: ObjectLinkV4 },
    /// Chandra's second +1: {U} only noncreature spells spend.
    ChandraManaV1,
    /// Chandra's -X: tap target 0, X stun counters on it.
    ChandraStunV1,
    /// Chandra's -6 emblem.
    ChandraEmblemV1,
    /// Draw a card (a Chandra emblem's trigger).
    DrawOneV1,
    /// Cloak and Dagger's entry trigger. A choice.
    CloakAndDaggerV1,
    /// Emperor of Bones: exile up to one target card from a graveyard,
    /// linked to the source.
    EmperorExileV1,
    /// "Adapt `count`" on the ability's source.
    AdaptV1 { count: u8 },
    /// Emperor of Bones' counters trigger. A choice.
    EmperorReturnV1,
    /// Sacrifice `creature` if its controller still controls it (Emperor of
    /// Bones' delayed trigger).
    SacrificeCreatureV1 { creature: ObjectLinkV4 },
    /// Destroy target 0, if it still names its incarnation.
    DestroyTargetV1,
    /// Destroy target 0; its controller may search for a basic land card
    /// and put it onto the battlefield tapped (Erode, White Orchid
    /// Phantom). A choice.
    DestroyThenSearchBasicTappedV1,
    /// Flickerwisp: exile target 0; it returns at the next end step.
    FlickerwispV1,
    /// The delayed return of `cards` from exile under their owners'
    /// control.
    ReturnExiledV1 { cards: Vec<ObjectLinkV4> },
    /// Yorion's entry trigger. A choice.
    YorionV1,
    /// Infernal Tutor. A choice.
    InfernalTutorV1,
    /// Lich's Relic's entry trigger: may pay {2}. A choice.
    LichsRelicV1,
    /// Lich's Relic: pay {2}, then its reflexive trigger.
    LichsRelicPayV1,
    /// Lion Sash's {W} ability on target 0.
    LionSashExileV1,
    /// Lion Sash's reconfigure: unattach the source.
    LionSashUnattachV1,
    /// Pre-War Formalwear: return target 0 and attach the source to it.
    FormalwearV1,
    /// Mystic Sanctuary: may put target 0 on top of its owner's library. A
    /// choice.
    MayTargetToLibraryTopV1,
    /// Put target 0 from a graveyard on top of its owner's library.
    TargetToLibraryTopV1,
    /// Overlord of the Balemurk's return to hand. A choice.
    OverlordReturnV1,
    /// Rain of Filth: the controller's lands gain its ability this turn.
    RainOfFilthV1,
    /// The printed slot of a mana ability the engine performs without the
    /// stack (Rain of Filth's granted ability); never resolves.
    SpecialActionV1,
    /// Recruiter of the Guard's search. A choice.
    RecruiterSearchV1,
    /// Stoneforge Mystic's search. A choice.
    StoneforgeSearchV1,
    /// Stoneforge Mystic's put. A choice.
    StoneforgePutV1,
    /// Aether Vial's put. A choice.
    AetherVialPutV1,
    /// A charge counter on the ability's source (Aether Vial).
    AddChargeCounterV1,
    /// Skyclave Apparition's entry exile of target 0.
    SkyclaveExileV1,
    /// Skyclave Apparition's leave trigger: the Illusion.
    SkyclaveTokenV1,
    /// Solitude's entry trigger on target 0.
    SolitudeV1,
    /// Teferi's +1.
    TeferiPlusOneV1,
    /// Wishclaw Talisman's search and control change. A choice.
    WishclawSearchV1,
}

impl E8aOpV1 {
    /// Whether this leaf can yield a player decision, so its program must
    /// use the resumable interpreter.
    pub(crate) fn contains_player_choice(&self) -> bool {
        matches!(
            self,
            Self::AdNauseamV1
                | Self::ChandraSurveilV1
                | Self::CloakAndDaggerV1
                | Self::EmperorReturnV1
                | Self::DestroyThenSearchBasicTappedV1
                | Self::YorionV1
                | Self::InfernalTutorV1
                | Self::LichsRelicV1
                | Self::MayTargetToLibraryTopV1
                | Self::OverlordReturnV1
                | Self::RecruiterSearchV1
                | Self::StoneforgeSearchV1
                | Self::StoneforgePutV1
                | Self::AetherVialPutV1
                | Self::WishclawSearchV1
        )
    }
}

/// Resolution choices owned by this module, carried as
/// `E3ChoiceKindV1::E8aV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum E8aChoiceV1 {
    /// Cloak and Dagger: up to one revealed nonland card or the chosen
    /// creature to exile until it leaves.
    CloakExileV1,
    /// Emperor of Bones: a creature card exiled with it.
    EmperorReturnV1,
    /// Up to one basic land card to put onto the battlefield tapped.
    BasicLandSearchV1,
    /// Yorion: any number of other nonland permanents to exile.
    YorionExileV1,
    /// Infernal Tutor: the card in hand to reveal.
    InfernalRevealV1,
    /// Infernal Tutor: up to one card with the revealed card's name.
    InfernalSearchV1,
    /// A card, put into hand without revealing it (Infernal Tutor's
    /// hellbent, Wishclaw Talisman), then `talisman` changes control.
    AnyCardSearchV1 { talisman: Option<ObjectLinkV4> },
    /// Overlord of the Balemurk: up to one non-Avatar creature card or
    /// planeswalker card to return to hand.
    OverlordReturnV1,
    /// Up to one card to reveal and put into hand (Recruiter of the Guard,
    /// Stoneforge Mystic).
    LibraryToHandRevealV1,
    /// Up to one card from hand to put onto the battlefield (Stoneforge
    /// Mystic, Aether Vial).
    HandToBattlefieldV1,
}

impl E8aChoiceV1 {
    /// Whether the candidates are hidden from the other player.
    pub(crate) fn chooser_private(&self) -> bool {
        matches!(
            self,
            Self::BasicLandSearchV1
                | Self::InfernalRevealV1
                | Self::InfernalSearchV1
                | Self::AnyCardSearchV1 { .. }
                | Self::LibraryToHandRevealV1
                | Self::HandToBattlefieldV1
        )
    }

    /// Whether the chooser sees library cards it has not looked at before.
    pub(crate) fn library_search(&self) -> bool {
        matches!(
            self,
            Self::BasicLandSearchV1
                | Self::InfernalSearchV1
                | Self::AnyCardSearchV1 { .. }
                | Self::LibraryToHandRevealV1
        )
    }

    pub(crate) fn public_purpose(&self) -> crate::rl::TargetSelectionPurposeV4 {
        match self {
            Self::YorionExileV1 => crate::rl::TargetSelectionPurposeV4::PermanentSelection,
            _ => crate::rl::TargetSelectionPurposeV4::CardSelection,
        }
    }

    /// Object incarnations this choice refers to beyond its candidates.
    pub(crate) fn referenced_bindings(&self) -> Vec<EffectObjectBinding> {
        Vec::new()
    }
}

// ---- Names and helpers --------------------------------------------------------------

pub(crate) const AETHER_VIAL: &str = "Aether Vial";
pub(crate) const BACK_TO_BASICS: &str = "Back to Basics";
const CANOPTEK_SCARAB_SWARM: &str = "Canoptek Scarab Swarm";
const CHANDRA: &str = "Chandra, Chill of Compliance";
const CLOAK_AND_DAGGER: &str = "Cloak and Dagger, Entwined";
pub(crate) const DARK_MATTER_MANIPULATOR: &str = "Dark Matter Manipulator";
const EMPEROR_OF_BONES: &str = "Emperor of Bones";
const FLAGSTONES_OF_TROKAIR: &str = "Flagstones of Trokair";
const FLICKERWISP: &str = "Flickerwisp";
pub(crate) const HARBINGER_OF_THE_SEAS: &str = "Harbinger of the Seas";
pub(crate) const HOGAAK: &str = "Hogaak, Arisen Necropolis";
const LICHS_RELIC: &str = "Lich's Relic";
pub(crate) const LION_SASH: &str = "Lion Sash";
const LORAN: &str = "Loran of the Third Path";
const METEOR_SWORD: &str = "Meteor Sword";
const MYSTIC_SANCTUARY: &str = "Mystic Sanctuary";
const ONCE_UPON_A_TIME: &str = "Once Upon a Time";
const OVERLORD_OF_THE_BALEMURK: &str = "Overlord of the Balemurk";
const PRE_WAR_FORMALWEAR: &str = "Pre-War Formalwear";
pub(crate) const RAIN_OF_FILTH: &str = "Rain of Filth";
const RECRUITER_OF_THE_GUARD: &str = "Recruiter of the Guard";
const SKYCLAVE_APPARITION: &str = "Skyclave Apparition";
const SOLITUDE: &str = "Solitude";
const STITCHERS_SUPPLIER: &str = "Stitcher's Supplier";
const STONEFORGE_MYSTIC: &str = "Stoneforge Mystic";
pub(crate) const TEFERI: &str = "Teferi, Time Raveler";
const THOUGHT_MONITOR: &str = "Thought Monitor";
pub(crate) const VALGAVOTHS_LAIR: &str = "Valgavoth's Lair";
const WHITE_ORCHID_PHANTOM: &str = "White Orchid Phantom";
pub(crate) const WIGHT_OF_THE_RELIQUARY: &str = "Wight of the Reliquary";
pub(crate) const WISHCLAW_TALISMAN: &str = "Wishclaw Talisman";
const YORION: &str = "Yorion, Sky Nomad";
const INSECT_ARTIFACT_TOKEN: &str = "Insect Artifact Token";
pub(crate) const ILLUSION_TOKEN: &str = "Illusion Token";

fn e8a(op: E8aOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::E8aV1(op)))
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

pub(crate) fn live_link(state: &GameState, object: ObjectId) -> ObjectLinkV4 {
    ObjectLinkV4 {
        object,
        zone_change_count: state.objects.get(object).zone_change_count,
    }
}

pub(crate) fn link_live_in(state: &GameState, link: ObjectLinkV4, zone: Zone) -> bool {
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

fn binding_link(binding: &EffectObjectBinding) -> ObjectLinkV4 {
    ObjectLinkV4 {
        object: binding.object,
        zone_change_count: binding.expected_zone_change_count,
    }
}

pub(crate) fn definition(state: &GameState, object: ObjectId) -> Option<&'static CardDef> {
    let live = state.objects.try_get(object)?;
    CARD_DEFS.get(usize::from(live.card_def))
}

pub(crate) fn definition_name(state: &GameState, object: ObjectId) -> Option<&'static str> {
    definition(state, object).map(|def| def.name)
}

/// Whether `object` is a front-face battlefield permanent named `name`
/// whose printed abilities apply.
pub(crate) fn active_named(state: &GameState, object: ObjectId, name: &str) -> bool {
    state.objects.try_get(object).is_some_and(|live| {
        live.zone == Zone::Battlefield
            && live.v4.face_index == 0
            && CARD_DEFS
                .get(usize::from(live.card_def))
                .is_some_and(|def| def.name == name && def.is_executable())
            && crate::continuous_characteristics_v1::printed_abilities_active(state, object)
    })
}

/// Every battlefield object, in each player's battlefield order.
fn battlefield(state: &GameState) -> impl Iterator<Item = ObjectId> + '_ {
    [PlayerId::P0, PlayerId::P1]
        .into_iter()
        .flat_map(move |player| state.players[player.index()].battlefield.iter().copied())
        .filter(move |&object| state.objects.get(object).zone == Zone::Battlefield)
}

/// The battlefield permanents named `name` whose printed abilities apply,
/// controlled by `controller` if given.
pub(crate) fn active_permanents<'a>(
    state: &'a GameState,
    name: &'a str,
    controller: Option<PlayerId>,
) -> impl Iterator<Item = ObjectId> + 'a {
    battlefield(state).filter(move |&object| {
        active_named(state, object, name)
            && controller.is_none_or(|player| state.objects.get(object).controller == player)
    })
}

/// Whether any object in the game is the card named `name`.
fn in_game(state: &GameState, name: &str) -> bool {
    card_id_by_name(name)
        .is_some_and(|id| state.objects.iter().any(|(_, live)| live.card_def == id))
}

/// The resolving ability's source, if it is still the incarnation that
/// created the ability, on the battlefield.
fn live_ability_source(ctx: &ExecCtx, state: &GameState) -> Option<ObjectId> {
    let contract = ctx.ability_source_contract?;
    let live = state.objects.try_get(contract.source)?;
    (live.zone == Zone::Battlefield && live.zone_change_count == contract.zone_change_count)
        .then_some(contract.source)
}

/// The battlefield incarnation that created the resolving ability.
fn source_link(ctx: &ExecCtx) -> Option<ObjectLinkV4> {
    ctx.ability_source_contract.map(|contract| ObjectLinkV4 {
        object: contract.source,
        zone_change_count: contract.zone_change_count,
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

/// Target `index`, if it is an object still legal for the resolving spell
/// or ability under `spec`.
fn legal_object_target(
    ctx: &ExecCtx,
    state: &GameState,
    index: usize,
    spec: TargetSpec,
) -> Option<ObjectId> {
    let Some(Target::Object(object)) = ctx.targets.get(index).copied() else {
        return None;
    };
    (ctx.target_incarnation_matches(index, state)
        && crate::engine::effect_target_is_legal(
            state,
            ctx.source,
            ctx.controller,
            spec,
            &ctx.targets,
            index,
        ))
    .then_some(object)
}

fn queue_trigger(
    state: &mut GameState,
    source: AbilitySourceContractV4,
    controller: PlayerId,
    op: E8aOpV1,
    target_spec: Option<TargetSpec>,
) {
    e8a_state(state).queued_triggers.push(QueuedTriggerV1 {
        source,
        controller,
        op,
        target_spec,
    });
}

fn has_type(state: &GameState, object: ObjectId, card_type: CardType) -> bool {
    crate::engine::object_has_type(state, object, card_type)
}

fn nothing() -> EffectOp {
    EffectOp::Sequence(Vec::new())
}

fn draw(player: PlayerRef, count: u32) -> EffectOp {
    EffectOp::DrawCards { player, count }
}

fn mill(count: u8) -> EffectOp {
    EffectOp::MillCards {
        player: PlayerRef::Controller,
        count,
    }
}

/// `player` as seen by an ability `controller` controls.
fn player_ref(controller: PlayerId, player: PlayerId) -> PlayerRef {
    if player == controller {
        PlayerRef::Controller
    } else {
        PlayerRef::Opponent
    }
}

/// The cards in `player`'s library matching `keep`, bound to their library
/// incarnations.
fn library_cards(
    state: &GameState,
    player: PlayerId,
    keep: impl Fn(&CardDef) -> bool,
) -> Vec<EffectObjectBinding> {
    state.players[player.index()]
        .library
        .iter()
        .copied()
        .filter(|&card| definition(state, card).is_some_and(&keep))
        .map(|card| bind(state, card))
        .collect()
}

/// The cards in `player`'s hand matching `keep`.
fn hand_cards(
    state: &GameState,
    player: PlayerId,
    keep: impl Fn(&CardDef) -> bool,
) -> Vec<EffectObjectBinding> {
    state.players[player.index()]
        .hand
        .iter()
        .copied()
        .filter(|&card| {
            state.objects.get(card).owner == player && definition(state, card).is_some_and(&keep)
        })
        .map(|card| bind(state, card))
        .collect()
}

fn shuffle(state: &mut GameState, player: PlayerId) -> Result<(), String> {
    state
        .shuffle_library(player)
        .map_err(|error| format!("{error:?}"))
}

/// Puts `card` onto the battlefield under `controller`'s control, tapped
/// if `tapped`.
fn put_onto_battlefield(state: &mut GameState, card: ObjectId, controller: PlayerId, tapped: bool) {
    let proposed = ProposedEvent::zone_change_to_battlefield_under_controller(card, controller);
    let proposed = match proposed {
        ProposedEvent::ZoneChange(mut change) if tapped => {
            change.force_battlefield_tapped = true;
            ProposedEvent::ZoneChange(change)
        }
        other => other,
    };
    event::propose_and_commit(state, proposed);
}

/// Sacrifices `object` if it is on the battlefield.
fn sacrifice(state: &mut GameState, object: ObjectId) {
    if state.objects.get(object).zone != Zone::Battlefield {
        return;
    }
    event::log_sacrifice(state, object);
    event::propose_and_commit(state, ProposedEvent::zone_change(object, Zone::Graveyard));
}

/// Moves `card` to its owner's hand and reveals it to the other player.
fn to_hand_revealed(state: &mut GameState, card: ObjectId) -> Result<(), String> {
    event::propose_and_commit(state, ProposedEvent::zone_change(card, Zone::Hand));
    let live = state.objects.get(card);
    if live.zone == Zone::Hand {
        let owner = live.owner;
        // spellbench-public-reveal: hand card revealed by the card rules.
        state.reveal_hand_card(owner.opponent(), owner, card)?;
    }
    Ok(())
}

/// A permanent card: one with a permanent card type (110.4a).
fn permanent_card(def: &CardDef) -> bool {
    [
        CardType::Artifact,
        CardType::Creature,
        CardType::Enchantment,
        CardType::Land,
        CardType::Planeswalker,
    ]
    .into_iter()
    .any(|card_type| def.has_type(card_type))
}

/// Records a delayed "at the beginning of the next end step" action.
fn next_end_step(
    state: &GameState,
    source: AbilitySourceContractV4,
    controller: PlayerId,
    cards: Vec<ObjectLinkV4>,
) -> EndStepActionV1 {
    EndStepActionV1 {
        source,
        controller,
        cards,
        created_in_end_step: (state.step == Step::End).then(|| TurnV1::now(state)),
    }
}

// ---- Spells -------------------------------------------------------------------------

/// Ad Nauseam: "Reveal the top card of your library and put that card into
/// your hand. You lose life equal to its mana value. You may repeat this
/// process any number of times."
pub fn ad_nauseam() -> EffectOp {
    e8a(E8aOpV1::AdNauseamV1)
}

/// Bitter Triumph: "Destroy target creature or planeswalker." Meteor
/// Sword's and Loran of the Third Path's entry triggers also destroy their
/// target.
pub fn destroy_target() -> EffectOp {
    e8a(E8aOpV1::DestroyTargetV1)
}

/// Cabal Ritual: "Add {B}{B}{B}. Threshold -- Add {B}{B}{B}{B}{B} instead
/// if there are seven or more cards in your graveyard."
pub fn cabal_ritual() -> EffectOp {
    e8a(E8aOpV1::CabalRitualV1)
}

/// Erode: "Destroy target creature or planeswalker. Its controller may
/// search their library for a basic land card, put it onto the battlefield
/// tapped, then shuffle."
pub fn destroy_then_search_basic_tapped() -> EffectOp {
    e8a(E8aOpV1::DestroyThenSearchBasicTappedV1)
}

/// Infernal Tutor: "Reveal a card from your hand. Search your library for a
/// card with the same name as that card, reveal it, put it into your hand,
/// then shuffle. Hellbent -- If you have no cards in hand, instead search
/// your library for a card, put it into your hand, then shuffle."
pub fn infernal_tutor() -> EffectOp {
    e8a(E8aOpV1::InfernalTutorV1)
}

/// Once Upon a Time: "Look at the top five cards of your library. You may
/// reveal a creature or land card from among them and put it into your
/// hand. Put the rest on the bottom of your library in a random order."
pub fn once_upon_a_time() -> EffectOp {
    EffectOp::LookTopSelectV1 {
        player: PlayerRef::Controller,
        count: LibraryLookCount::Fixed(5),
        rule: LibraryPickRule {
            pick: 1,
            choose_rest_order: false,
            selection: Some(LibraryPickSelectionV1 {
                without_mana_cost: crate::engine::FreeCastV1(false),
                filter: LibraryPickFilterV1::CreatureOrLandCardV1,
                optional: true,
                destination: Zone::Hand,
                reveal_selected: true,
            }),
        },
        pick_x: false,
    }
}

/// Rain of Filth: "Until end of turn, lands you control gain 'Sacrifice
/// this land: Add {B}.'"
pub fn rain_of_filth() -> EffectOp {
    e8a(E8aOpV1::RainOfFilthV1)
}

/// Supreme Verdict: "Destroy all creatures." Its "This spell can't be
/// countered" is the card's printed flag.
pub fn supreme_verdict() -> EffectOp {
    EffectOp::DestroyAllCreatures
}

// ---- Activated abilities --------------------------------------------------------------

/// Aether Vial's ability.
pub fn aether_vial_put() -> EffectOp {
    e8a(E8aOpV1::AetherVialPutV1)
}

/// Chandra's first +1.
pub fn chandra_surveil() -> EffectOp {
    e8a(E8aOpV1::ChandraSurveilV1)
}

/// Chandra's second +1: "Add {U}. Spend this mana only to cast a
/// noncreature spell."
pub fn chandra_mana() -> EffectOp {
    e8a(E8aOpV1::ChandraManaV1)
}

/// Chandra's -X: "Tap target artifact or creature. Put X stun counters on
/// it."
pub fn chandra_stun() -> EffectOp {
    e8a(E8aOpV1::ChandraStunV1)
}

/// Chandra's -6: "You get an emblem with 'Whenever you cast a spell, draw a
/// card.'"
pub fn chandra_emblem() -> EffectOp {
    e8a(E8aOpV1::ChandraEmblemV1)
}

/// Emperor of Bones: "Adapt 2." (If this creature has no +1/+1 counters on
/// it, put two +1/+1 counters on it.)
pub fn adapt_two() -> EffectOp {
    e8a(E8aOpV1::AdaptV1 { count: 2 })
}

/// Lion Sash: "Exile target card from a graveyard. If it was a permanent
/// card, put a +1/+1 counter on this permanent."
pub fn lion_sash_exile() -> EffectOp {
    e8a(E8aOpV1::LionSashExileV1)
}

/// Lion Sash's reconfigure: "unattach from a creature".
pub fn lion_sash_unattach() -> EffectOp {
    e8a(E8aOpV1::LionSashUnattachV1)
}

/// Loran of the Third Path: "You and target opponent each draw a card."
pub fn loran_draw() -> EffectOp {
    EffectOp::Sequence(vec![
        draw(PlayerRef::Controller, 1),
        draw(PlayerRef::Target(0), 1),
    ])
}

/// Rain of Filth's granted mana ability, performed without the stack.
pub fn rain_of_filth_mana() -> EffectOp {
    e8a(E8aOpV1::SpecialActionV1)
}

/// Stoneforge Mystic: "You may put an Equipment card from your hand onto
/// the battlefield."
pub fn stoneforge_put() -> EffectOp {
    e8a(E8aOpV1::StoneforgePutV1)
}

/// Teferi's +1: "Until your next turn, you may cast sorcery spells as
/// though they had flash."
pub fn teferi_plus_one() -> EffectOp {
    e8a(E8aOpV1::TeferiPlusOneV1)
}

/// Teferi's -3: "Return up to one target artifact, creature, or enchantment
/// to its owner's hand. Draw a card."
pub fn teferi_minus_three() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::MoveAllTargets {
            to_zone: Zone::Hand,
        },
        draw(PlayerRef::Controller, 1),
    ])
}

/// Wishclaw Talisman's ability.
pub fn wishclaw_search() -> EffectOp {
    e8a(E8aOpV1::WishclawSearchV1)
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

const fn graveyard_trigger(
    condition: TriggerCondition,
    effect: fn() -> EffectOp,
) -> TriggeredAbilityDef {
    TriggeredAbilityDef {
        home_zone: Zone::Graveyard,
        ..trigger(condition, effect)
    }
}

const fn e8a_condition(condition: E8aTriggerV1) -> TriggerCondition {
    TriggerCondition::StandardV1(StandardTriggerV1::EternalV1(
        crate::eternal_cards_v1::EternalTriggerV1::E8aV1(condition),
    ))
}

/// Wave E7b's evoke condition: "When this enters, if its evoke cost was
/// paid, its controller sacrifices it."
const EVOKED: TriggerCondition = TriggerCondition::StandardV1(StandardTriggerV1::EternalV1(
    crate::eternal_cards_v1::EternalTriggerV1::E7bV1(
        crate::eternal_vintage_e7b_v1::E7bTriggerV1::EvokedEntersV1,
    ),
));

const UPKEEP: TriggerCondition = TriggerCondition::BeginningOfUpkeep {
    controller_only: true,
};

/// Aether Vial: "you may put a charge counter on this artifact."
fn vial_upkeep() -> EffectOp {
    EffectOp::Choice {
        controller: PlayerRef::Controller,
        options: vec![nothing(), e8a(E8aOpV1::AddChargeCounterV1)],
    }
}

/// Canoptek Scarab Swarm: "exile target player's graveyard. For each
/// artifact or land card exiled this way, create a 1/1 colorless Insect
/// artifact creature token with flying."
fn canoptek_enters() -> EffectOp {
    e8a(E8aOpV1::CanoptekV1)
}

/// Cloak and Dagger: "choose target opponent and up to one target creature
/// they control. They reveal their hand. You may exile a nonland card from
/// their hand or the chosen creature until Cloak and Dagger leave the
/// battlefield."
fn cloak_enters() -> EffectOp {
    e8a(E8aOpV1::CloakAndDaggerV1)
}

/// "Mill three cards."
fn mill_three() -> EffectOp {
    mill(3)
}

/// Emperor of Bones: "exile up to one target card from a graveyard."
fn emperor_combat() -> EffectOp {
    e8a(E8aOpV1::EmperorExileV1)
}

/// Emperor of Bones: "put a creature card exiled with this creature onto
/// the battlefield under your control with a finality counter on it. It
/// gains haste. Sacrifice it at the beginning of the next end step."
fn emperor_counters() -> EffectOp {
    e8a(E8aOpV1::EmperorReturnV1)
}

/// Flagstones of Trokair: "you may search your library for a Plains card,
/// put it onto the battlefield tapped, then shuffle."
fn flagstones_search() -> EffectOp {
    EffectOp::SearchLibraryCardsToDestination {
        player: PlayerRef::Controller,
        filter: crate::effect::LibraryCardFilter::LandWithSubtype(Subtype::Plains),
        max_targets: 1,
        destination: crate::effect::LibrarySearchDestinationV1::Battlefield { tapped: true },
    }
}

/// Flickerwisp: "exile another target permanent. Return that card to the
/// battlefield under its owner's control at the beginning of the next end
/// step."
fn flickerwisp_enters() -> EffectOp {
    e8a(E8aOpV1::FlickerwispV1)
}

/// Lich's Relic: "you may pay {2}. When you do, for each opponent, destroy
/// up to one target creature or planeswalker that player controls."
fn lichs_relic_enters() -> EffectOp {
    e8a(E8aOpV1::LichsRelicV1)
}

/// Mystic Sanctuary: "you may put target instant or sorcery card from your
/// graveyard on top of your library."
fn sanctuary_enters() -> EffectOp {
    e8a(E8aOpV1::MayTargetToLibraryTopV1)
}

/// Overlord of the Balemurk: "mill four cards, then you may return a
/// non-Avatar creature card or a planeswalker card from your graveyard to
/// your hand."
fn overlord_enters_or_attacks() -> EffectOp {
    EffectOp::Sequence(vec![mill(4), e8a(E8aOpV1::OverlordReturnV1)])
}

/// Impending: "At the beginning of your end step, remove a time counter from
/// it."
fn remove_time_counter() -> EffectOp {
    EffectOp::RemoveTimeCounterFromSource
}

/// Pre-War Formalwear: "return target creature card with mana value 3 or
/// less from your graveyard to the battlefield and attach this Equipment to
/// it."
fn formalwear_enters() -> EffectOp {
    e8a(E8aOpV1::FormalwearV1)
}

/// Recruiter of the Guard: "you may search your library for a creature card
/// with toughness 2 or less, reveal it, put it into your hand, then
/// shuffle."
fn recruiter_enters() -> EffectOp {
    e8a(E8aOpV1::RecruiterSearchV1)
}

/// Skyclave Apparition: "exile up to one target nonland, nontoken permanent
/// you don't control with mana value 4 or less."
fn skyclave_enters() -> EffectOp {
    e8a(E8aOpV1::SkyclaveExileV1)
}

/// Skyclave Apparition: "the exiled card's owner creates an X/X blue
/// Illusion creature token, where X is the mana value of the exiled card."
fn skyclave_leaves() -> EffectOp {
    e8a(E8aOpV1::SkyclaveTokenV1)
}

/// Solitude: "exile up to one other target creature. That creature's
/// controller gains life equal to its power."
fn solitude_enters() -> EffectOp {
    e8a(E8aOpV1::SolitudeV1)
}

/// Evoke's sacrifice (wave E7b's leaf).
fn evoked_sacrifice() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::E7bV1(
        crate::eternal_vintage_e7b_v1::E7bOpV1::SacrificeSourceV1,
    )))
}

/// Stoneforge Mystic: "you may search your library for an Equipment card,
/// reveal it, put it into your hand, then shuffle."
fn stoneforge_enters() -> EffectOp {
    e8a(E8aOpV1::StoneforgeSearchV1)
}

/// Thought Monitor: "draw two cards."
fn draw_two() -> EffectOp {
    draw(PlayerRef::Controller, 2)
}

/// Yorion: "exile any number of other nonland permanents you own and
/// control. Return those cards to the battlefield at the beginning of the
/// next end step."
fn yorion_enters() -> EffectOp {
    e8a(E8aOpV1::YorionV1)
}

const AETHER_VIAL_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(UPKEEP, vial_upkeep)];
const CANOPTEK_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, canoptek_enters)];
const CLOAK_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(TriggerCondition::Etb, cloak_enters)];
const MILL_THREE_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(TriggerCondition::Etb, mill_three)];
const EMPEROR_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(
        TriggerCondition::BeginningOfControllerCombat,
        emperor_combat,
    ),
    trigger(
        e8a_condition(E8aTriggerV1::PlusOneCountersPutOnSelfV1),
        emperor_counters,
    ),
];
const FLAGSTONES_TRIGGERS: [TriggeredAbilityDef; 1] = [graveyard_trigger(
    TriggerCondition::LeftBattlefieldToGraveyard,
    flagstones_search,
)];
const FLICKERWISP_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, flickerwisp_enters)];
const LICHS_RELIC_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, lichs_relic_enters)];
const DESTROY_ON_ENTRY_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, destroy_target)];
const MYSTIC_SANCTUARY_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(
    e8a_condition(E8aTriggerV1::EntersUntappedV1),
    sanctuary_enters,
)];
const OVERLORD_TRIGGERS: [TriggeredAbilityDef; 3] = [
    trigger(TriggerCondition::Etb, overlord_enters_or_attacks),
    trigger(TriggerCondition::Attacks, overlord_enters_or_attacks),
    trigger(
        TriggerCondition::BeginningControllerEndStepWithTimeCounter,
        remove_time_counter,
    ),
];
const FORMALWEAR_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, formalwear_enters)];
const RECRUITER_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, recruiter_enters)];
const SKYCLAVE_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(TriggerCondition::Etb, skyclave_enters),
    trigger(TriggerCondition::LeftBattlefield, skyclave_leaves),
];
const SOLITUDE_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(TriggerCondition::Etb, solitude_enters),
    trigger(EVOKED, evoked_sacrifice),
];
const STITCHERS_SUPPLIER_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(TriggerCondition::Etb, mill_three),
    graveyard_trigger(TriggerCondition::LeftBattlefieldToGraveyard, mill_three),
];
const STONEFORGE_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, stoneforge_enters)];
const THOUGHT_MONITOR_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, draw_two)];
const WHITE_ORCHID_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(
    TriggerCondition::Etb,
    destroy_then_search_basic_tapped,
)];
const YORION_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(TriggerCondition::Etb, yorion_enters)];

/// Triggered abilities of this module's cards.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        AETHER_VIAL => &AETHER_VIAL_TRIGGERS,
        CANOPTEK_SCARAB_SWARM => &CANOPTEK_TRIGGERS,
        CLOAK_AND_DAGGER => &CLOAK_TRIGGERS,
        DARK_MATTER_MANIPULATOR => &MILL_THREE_TRIGGERS,
        EMPEROR_OF_BONES => &EMPEROR_TRIGGERS,
        FLAGSTONES_OF_TROKAIR => &FLAGSTONES_TRIGGERS,
        FLICKERWISP => &FLICKERWISP_TRIGGERS,
        LICHS_RELIC => &LICHS_RELIC_TRIGGERS,
        LORAN | METEOR_SWORD => &DESTROY_ON_ENTRY_TRIGGERS,
        MYSTIC_SANCTUARY => &MYSTIC_SANCTUARY_TRIGGERS,
        OVERLORD_OF_THE_BALEMURK => &OVERLORD_TRIGGERS,
        PRE_WAR_FORMALWEAR => &FORMALWEAR_TRIGGERS,
        RECRUITER_OF_THE_GUARD => &RECRUITER_TRIGGERS,
        SKYCLAVE_APPARITION => &SKYCLAVE_TRIGGERS,
        SOLITUDE => &SOLITUDE_TRIGGERS,
        STITCHERS_SUPPLIER => &STITCHERS_SUPPLIER_TRIGGERS,
        STONEFORGE_MYSTIC => &STONEFORGE_TRIGGERS,
        THOUGHT_MONITOR => &THOUGHT_MONITOR_TRIGGERS,
        WHITE_ORCHID_PHANTOM => &WHITE_ORCHID_TRIGGERS,
        YORION => &YORION_TRIGGERS,
        // Wave E9a names disjoint cards.
        _ => crate::eternal_legacy_e9a_v1::triggers_for(name),
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
    // `triggers_for` falls through to E9a's cards, so they answer first.
    if let Some(spec) = crate::eternal_legacy_e9a_v1::trigger_target_spec(name, effect) {
        return Some(spec);
    }
    // And through E9a's to Premodern wave P1b's.
    if let Some(spec) = crate::eternal_premodern_p1b_v1::trigger_target_spec(name, effect) {
        return Some(spec);
    }
    if !own_trigger_effect(name, effect) {
        return None;
    }
    let destroy = *effect == destroy_target();
    Some(match name {
        CANOPTEK_SCARAB_SWARM if *effect == canoptek_enters() => TargetSpec::AnyPlayer,
        CLOAK_AND_DAGGER if *effect == cloak_enters() => {
            TargetSpec::StandardV1(StandardTargetV1::OpponentThenUpToOneTheirCreatureV1)
        }
        EMPEROR_OF_BONES if *effect == emperor_combat() => TargetSpec::UpToOneCardInGraveyards,
        FLICKERWISP if *effect == flickerwisp_enters() => {
            TargetSpec::StandardV1(StandardTargetV1::AnotherPermanentV1)
        }
        LICHS_RELIC if destroy => {
            TargetSpec::StandardV1(StandardTargetV1::UpToOneOpponentCreatureOrPlaneswalkerV1)
        }
        LORAN if destroy => {
            TargetSpec::StandardV1(StandardTargetV1::UpToOneArtifactOrEnchantmentV1)
        }
        METEOR_SWORD if destroy => TargetSpec::AnyPermanent,
        MYSTIC_SANCTUARY if *effect == sanctuary_enters() => {
            TargetSpec::StandardV1(StandardTargetV1::InstantOrSorceryCardInOwnGraveyard)
        }
        PRE_WAR_FORMALWEAR if *effect == formalwear_enters() => {
            TargetSpec::CreatureCardInOwnGraveyardManaValueAtMost(3)
        }
        SKYCLAVE_APPARITION if *effect == skyclave_enters() => TargetSpec::StandardV1(
            StandardTargetV1::UpToOneOpponentNonlandNontokenManaValueAtMostV1(4),
        ),
        SOLITUDE if *effect == solitude_enters() => TargetSpec::UpToOneOtherCreature,
        WHITE_ORCHID_PHANTOM if *effect == destroy_then_search_basic_tapped() => {
            TargetSpec::StandardV1(StandardTargetV1::UpToOneNonbasicLandV1)
        }
        _ => TargetSpec::None,
    })
}

/// Queued and delayed triggers owned by this module's cards.
pub(crate) fn delayed_effect_matches(name: &str, effect: &EffectOp) -> bool {
    let EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::E8aV1(op))) = effect else {
        return false;
    };
    match name {
        CHANDRA => matches!(op, E8aOpV1::DrawOneV1),
        LICHS_RELIC => matches!(op, E8aOpV1::DestroyTargetV1),
        FLICKERWISP | YORION => matches!(op, E8aOpV1::ReturnExiledV1 { .. }),
        EMPEROR_OF_BONES => matches!(op, E8aOpV1::SacrificeCreatureV1 { .. }),
        _ => false,
    }
}

/// Whether `condition` triggers on its source's arrival in the zone it now
/// occupies.
pub(crate) fn arrival_condition(condition: E8aTriggerV1) -> bool {
    condition == E8aTriggerV1::EntersUntappedV1
}

pub(crate) fn trigger_matches(
    condition: E8aTriggerV1,
    events: &[CommittedEvent],
    index: usize,
    source: ObjectId,
    state: &GameState,
) -> bool {
    let Some(live) = state.objects.try_get(source) else {
        return false;
    };
    match condition {
        E8aTriggerV1::EntersUntappedV1 => {
            matches!(
                events[index],
                CommittedEvent::ZoneChange {
                    object,
                    to: Zone::Battlefield,
                    ..
                } if object == source
            ) && active_named(state, source, MYSTIC_SANCTUARY)
                && !live.tapped
        }
        E8aTriggerV1::PlusOneCountersPutOnSelfV1 => {
            matches!(
                events[index],
                CommittedEvent::PlusOneCountersAdded {
                    object,
                    zone_change_count,
                    count,
                    ..
                } if object == source && zone_change_count == live.zone_change_count && count > 0
            ) && active_named(state, source, EMPEROR_OF_BONES)
        }
    }
}

// ---- Effect leaves ------------------------------------------------------------------

pub(crate) fn execute(op: &E8aOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        E8aOpV1::CabalRitualV1 => {
            let threshold =
                crate::effect::controller_graveyard_card_count(state, ctx.controller) >= 7;
            let count = if threshold { 5 } else { 3 };
            crate::effect::execute(
                &EffectOp::AddMana {
                    player: PlayerRef::Controller,
                    colors: vec![ManaColor::B; count],
                },
                ctx,
                state,
            );
        }
        E8aOpV1::CanoptekV1 => canoptek(ctx, state),
        E8aOpV1::ChandraReturnV1 { card } => {
            let milled = ObjectLinkV4 {
                object: card.object,
                zone_change_count: card.zone_change_count.saturating_add(1),
            };
            if !link_live_in(state, milled, Zone::Graveyard)
                || state.objects.get(card.object).owner != ctx.controller
            {
                return;
            }
            let returns = definition(state, card.object).is_some_and(|def| {
                !def.has_type(CardType::Creature) && !def.has_type(CardType::Land)
            });
            if returns {
                event::propose_and_commit(
                    state,
                    ProposedEvent::zone_change(card.object, Zone::Hand),
                );
            }
        }
        E8aOpV1::ChandraManaV1 => {
            let Some(contract) = ctx.ability_source_contract else {
                halt(state, ctx.source);
                return;
            };
            let unit = crate::mana::RestrictedManaUnitV1 {
                color: ManaColor::U,
                restriction: crate::card_def::ManaSpendRestrictionDef::NoncreatureSpellV1,
                source: ObjectLinkV4 {
                    object: contract.source,
                    zone_change_count: contract.zone_change_count,
                },
                source_card_def: contract.card_def,
            };
            state.players[ctx.controller.index()]
                .restricted_mana_pool
                .0
                .push(unit);
        }
        E8aOpV1::ChandraStunV1 => {
            let spec = TargetSpec::StandardV1(StandardTargetV1::ArtifactOrCreatureV1);
            let Some(object) = legal_object_target(ctx, state, 0, spec) else {
                return;
            };
            if !state.objects.get(object).tapped {
                event::propose_and_commit(state, ProposedEvent::tap(object));
            }
            let stun = i16::try_from(ctx.x_value).unwrap_or(i16::MAX);
            if stun > 0 && state.objects.get(object).zone == Zone::Battlefield {
                let counters = &mut state.objects.get_mut(object).counters.stun;
                *counters = counters.saturating_add(stun);
            }
        }
        E8aOpV1::ChandraEmblemV1 => {
            let Some(source) = ctx.ability_source_contract else {
                halt(state, ctx.source);
                return;
            };
            e8a_state(state)
                .chandra_emblems
                .push((ctx.controller, source));
        }
        E8aOpV1::DrawOneV1 => {
            crate::effect::execute(&draw(PlayerRef::Controller, 1), ctx, state);
        }
        E8aOpV1::EmperorExileV1 => {
            let Some(card) = bound_target_in(ctx, state, 0, Zone::Graveyard) else {
                return;
            };
            let Some(emperor) = source_link(ctx) else {
                return;
            };
            event::propose_and_commit(state, ProposedEvent::zone_change(card, Zone::Exile));
            if state.objects.get(card).zone == Zone::Exile {
                let exiled = live_link(state, card);
                e8a_state(state).emperor_exiles.push((emperor, exiled));
            }
        }
        E8aOpV1::AdaptV1 { count } => {
            let Some(source) = live_ability_source(ctx, state) else {
                return;
            };
            if state.objects.get(source).counters.plus1_plus1 == 0 {
                let _ =
                    event::add_plus_one_counters(state, source, ctx.controller, i32::from(*count));
            }
        }
        E8aOpV1::SacrificeCreatureV1 { creature } => {
            if link_live_in(state, *creature, Zone::Battlefield)
                && state.objects.get(creature.object).controller == ctx.controller
            {
                sacrifice(state, creature.object);
            }
        }
        E8aOpV1::DestroyTargetV1 => {
            if bound_target_in(ctx, state, 0, Zone::Battlefield).is_some() {
                crate::effect::execute(
                    &EffectOp::DestroyObject {
                        object: ObjectRef::Target(0),
                    },
                    ctx,
                    state,
                );
            }
        }
        E8aOpV1::FlickerwispV1 => {
            let spec = TargetSpec::StandardV1(StandardTargetV1::AnotherPermanentV1);
            let Some(object) = legal_object_target(ctx, state, 0, spec) else {
                return;
            };
            let Some(source) = ctx.ability_source_contract else {
                return;
            };
            event::propose_and_commit(state, ProposedEvent::zone_change(object, Zone::Exile));
            let live = state.objects.get(object);
            if live.zone != Zone::Exile || live.v4.is_token {
                return;
            }
            let card = live_link(state, object);
            let record = next_end_step(state, source, ctx.controller, vec![card]);
            e8a_state(state).end_step_returns.push(record);
        }
        E8aOpV1::ReturnExiledV1 { cards } => {
            let returns = cards
                .iter()
                .filter(|&&card| link_live_in(state, card, Zone::Exile))
                .map(|card| {
                    let owner = state.objects.get(card.object).owner;
                    ProposedEvent::zone_change_to_battlefield_under_controller(card.object, owner)
                })
                .collect::<Vec<_>>();
            if !returns.is_empty() {
                event::propose_and_commit_batch(state, returns);
            }
        }
        E8aOpV1::LichsRelicPayV1 => {
            if !crate::engine::pay_effect_mana(ctx.controller, &[], 2, state) {
                // The option is offered only while the payment is possible.
                halt(state, ctx.source);
                return;
            }
            // "When you do": a reflexive trigger, which targets. Paying
            // commits no event that would make the next trigger collection
            // read the queue, so it goes straight to the pending triggers.
            if let Some(source) = ctx.ability_source_contract {
                let mut pending = crate::eternal_cards_v1::delayed_trigger(
                    source,
                    ctx.controller,
                    e8a(E8aOpV1::DestroyTargetV1),
                );
                pending.target_spec = TargetSpec::StandardV1(
                    StandardTargetV1::UpToOneOpponentCreatureOrPlaneswalkerV1,
                );
                state.engine.pending_triggers.push(pending);
            }
        }
        E8aOpV1::LionSashExileV1 => {
            let Some(card) = bound_target_in(ctx, state, 0, Zone::Graveyard) else {
                return;
            };
            let permanent = definition(state, card).is_some_and(permanent_card);
            event::propose_and_commit(state, ProposedEvent::zone_change(card, Zone::Exile));
            if permanent && state.objects.get(card).zone == Zone::Exile {
                if let Some(source) = live_ability_source(ctx, state) {
                    let _ = event::add_plus_one_counters(state, source, ctx.controller, 1);
                }
            }
        }
        E8aOpV1::LionSashUnattachV1 => {
            if let Some(source) = live_ability_source(ctx, state) {
                unattach(state, source);
            }
        }
        E8aOpV1::FormalwearV1 => {
            let spec = TargetSpec::CreatureCardInOwnGraveyardManaValueAtMost(3);
            let Some(card) = legal_object_target(ctx, state, 0, spec) else {
                return;
            };
            put_onto_battlefield(state, card, ctx.controller, false);
            if state.objects.get(card).zone != Zone::Battlefield
                || !has_type(state, card, CardType::Creature)
            {
                return;
            }
            let Some(formalwear) = live_ability_source(ctx, state) else {
                return;
            };
            let formalwear_count = state.objects.get(formalwear).zone_change_count;
            let card_count = state.objects.get(card).zone_change_count;
            if state
                .attach_object_exact(formalwear, formalwear_count, card, card_count)
                .is_err()
            {
                halt(state, formalwear);
            }
        }
        E8aOpV1::TargetToLibraryTopV1 => {
            if let Some(card) = bound_target_in(ctx, state, 0, Zone::Graveyard) {
                event::propose_and_commit(
                    state,
                    ProposedEvent::public_library_insert(card, event::LibraryPlacement::Top),
                );
            }
        }
        E8aOpV1::RainOfFilthV1 => {
            let turn = TurnV1::now(state);
            let lands = battlefield(state)
                .filter(|&object| {
                    state.objects.get(object).controller == ctx.controller
                        && has_type(state, object, CardType::Land)
                })
                .map(|object| (live_link(state, object), turn))
                .collect::<Vec<_>>();
            let granted = &mut e8a_state(state).rain_lands;
            for land in lands {
                if !granted.contains(&land) {
                    granted.push(land);
                }
            }
        }
        E8aOpV1::SpecialActionV1 => {}
        E8aOpV1::AddChargeCounterV1 => {
            if let Some(source) = live_ability_source(ctx, state) {
                let counters = &mut state.objects.get_mut(source).counters.charge;
                *counters = counters.saturating_add(1);
            }
        }
        E8aOpV1::SkyclaveExileV1 => {
            let spec = TargetSpec::StandardV1(
                StandardTargetV1::UpToOneOpponentNonlandNontokenManaValueAtMostV1(4),
            );
            let Some(object) = legal_object_target(ctx, state, 0, spec) else {
                return;
            };
            let Some(skyclave) = source_link(ctx) else {
                return;
            };
            let owner = state.objects.get(object).owner;
            event::propose_and_commit(state, ProposedEvent::zone_change(object, Zone::Exile));
            let live = state.objects.get(object);
            if live.zone == Zone::Exile {
                let mana_value = CARD_DEFS
                    .get(usize::from(live.card_def))
                    .map_or(0, |def| def.mana_value);
                e8a_state(state)
                    .skyclave_exiles
                    .push((skyclave, mana_value, owner));
            }
        }
        E8aOpV1::SkyclaveTokenV1 => skyclave_token(ctx, state),
        E8aOpV1::SolitudeV1 => {
            let Some(creature) =
                legal_object_target(ctx, state, 0, TargetSpec::UpToOneOtherCreature)
            else {
                return;
            };
            let controller = state.objects.get(creature).controller;
            let power = crate::engine::effective_power(state, creature);
            event::propose_and_commit(state, ProposedEvent::zone_change(creature, Zone::Exile));
            if power > 0 {
                crate::effect::execute(
                    &EffectOp::GainLife {
                        player: player_ref(ctx.controller, controller),
                        amount: power,
                    },
                    ctx,
                    state,
                );
            }
        }
        E8aOpV1::TeferiPlusOneV1 => {
            let flash = &mut e8a_state(state).sorcery_flash;
            if !flash.contains(&ctx.controller) {
                flash.push(ctx.controller);
            }
        }
        // Choices run in `drive`; the plain executor never reaches them.
        E8aOpV1::AdNauseamV1
        | E8aOpV1::ChandraSurveilV1
        | E8aOpV1::CloakAndDaggerV1
        | E8aOpV1::EmperorReturnV1
        | E8aOpV1::DestroyThenSearchBasicTappedV1
        | E8aOpV1::YorionV1
        | E8aOpV1::InfernalTutorV1
        | E8aOpV1::LichsRelicV1
        | E8aOpV1::MayTargetToLibraryTopV1
        | E8aOpV1::OverlordReturnV1
        | E8aOpV1::RecruiterSearchV1
        | E8aOpV1::StoneforgeSearchV1
        | E8aOpV1::StoneforgePutV1
        | E8aOpV1::AetherVialPutV1
        | E8aOpV1::WishclawSearchV1 => halt(state, ctx.source),
    }
}

/// Unattaches `source` from whatever it is attached to.
fn unattach(state: &mut GameState, source: ObjectId) {
    if state.objects.get(source).v4.attached_to.is_none() {
        return;
    }
    for (_, candidate) in state.objects.iter_mut() {
        candidate.attachments.retain(|&attached| attached != source);
    }
    state.objects.get_mut(source).v4.attached_to = None;
}

/// Canoptek Scarab Swarm's entry trigger.
fn canoptek(ctx: &ExecCtx, state: &mut GameState) {
    let Some(Target::Player(player)) = ctx.targets.first().copied() else {
        return;
    };
    if !ctx.target_incarnation_matches(0, state) {
        return;
    }
    let cards = state.players[player.index()].graveyard.clone();
    if cards.is_empty() {
        return;
    }
    let counted = cards
        .iter()
        .copied()
        .filter(|&card| {
            definition(state, card)
                .is_some_and(|def| def.has_type(CardType::Artifact) || def.has_type(CardType::Land))
        })
        .collect::<Vec<_>>();
    event::propose_and_commit_batch(
        state,
        cards
            .iter()
            .map(|&card| ProposedEvent::zone_change(card, Zone::Exile))
            .collect(),
    );
    let exiled = counted
        .into_iter()
        .filter(|&card| state.objects.get(card).zone == Zone::Exile)
        .count();
    let token = token_id(INSECT_ARTIFACT_TOKEN);
    for _ in 0..exiled {
        crate::effect::execute(
            &EffectOp::CreateToken {
                token_def: token,
                controller: PlayerRef::Controller,
            },
            ctx,
            state,
        );
    }
}

/// Skyclave Apparition's leave trigger: the owner of the card this
/// incarnation exiled creates an Illusion of that card's mana value.
fn skyclave_token(ctx: &ExecCtx, state: &mut GameState) {
    let Some(skyclave) = source_link(ctx) else {
        return;
    };
    let Some(position) = read_state(state).and_then(|e8a| {
        e8a.skyclave_exiles
            .iter()
            .position(|(host, _, _)| *host == skyclave)
    }) else {
        return;
    };
    let (_, mana_value, owner) = e8a_state(state).skyclave_exiles.remove(position);
    let start = state.engine.event_history.len();
    crate::effect::execute(
        &EffectOp::CreateToken {
            token_def: token_id(ILLUSION_TOKEN),
            controller: player_ref(ctx.controller, owner),
        },
        ctx,
        state,
    );
    let created = state.engine.event_history[start..]
        .iter()
        .find_map(|event| match event {
            CommittedEvent::CreateToken { object, .. } => Some(*object),
            _ => None,
        });
    if let Some(token) = created {
        if state.objects.get(token).zone == Zone::Battlefield {
            let link = live_link(state, token);
            e8a_state(state).illusions.push((link, mana_value));
        }
    }
}

// ---- Choices ------------------------------------------------------------------------

fn targets_of(bindings: &[EffectObjectBinding]) -> Vec<Target> {
    bindings
        .iter()
        .map(|binding| Target::Object(binding.object))
        .collect()
}

/// Stages one of this module's selections.
fn offer(
    continuation: &mut EffectContinuation,
    kind: E8aChoiceV1,
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
            kind: E3ChoiceKindV1::E8aV1(kind),
            player,
            candidates: targets_of(&bindings),
            bindings,
            min,
            max,
            ordered: false,
            path,
            remaining,
        },
    );
}

fn push(continuation: &mut EffectContinuation, op: EffectOp, path: &[u16]) {
    continuation.frames.push(EffectFrame::Program {
        op,
        path: path.to_vec(),
    });
}

/// "You may [effect]": a choice to do nothing or `effect`, pushed for the
/// interpreter.
fn push_may(continuation: &mut EffectContinuation, effect: EffectOp, path: &[u16]) {
    push(
        continuation,
        EffectOp::Choice {
            controller: PlayerRef::Controller,
            options: vec![nothing(), effect],
        },
        path,
    );
}

/// The count of `bindings` as a choice maximum.
fn count_of(bindings: &[EffectObjectBinding]) -> u16 {
    u16::try_from(bindings.len()).unwrap_or(u16::MAX)
}

/// Runs a leaf inside the resumable interpreter. `Ok(true)` when it staged a
/// choice and the continuation must suspend.
pub(crate) fn drive(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    op: &E8aOpV1,
    path: Vec<u16>,
) -> Result<bool, String> {
    let controller = continuation.ctx.controller;
    match op {
        E8aOpV1::AdNauseamV1 => {
            if state.players[controller.index()].library.is_empty() {
                return Ok(false);
            }
            let ctx = continuation.ctx.clone();
            crate::effect::execute(
                &EffectOp::RevealTopCardToHandLoseLifeEqualToManaValue,
                &ctx,
                state,
            );
            // "You may repeat this process any number of times."
            if !state.players[controller.index()].library.is_empty() {
                push_may(continuation, e8a(E8aOpV1::AdNauseamV1), &path);
            }
            Ok(false)
        }
        E8aOpV1::ChandraSurveilV1 => {
            let Some(&top) = state.players[controller.index()].library.first() else {
                return Ok(false);
            };
            let card = live_link(state, top);
            // The return runs after the surveil.
            push(continuation, e8a(E8aOpV1::ChandraReturnV1 { card }), &path);
            push(
                continuation,
                EffectOp::Surveil {
                    player: PlayerRef::Controller,
                    count: 1,
                },
                &path,
            );
            Ok(false)
        }
        E8aOpV1::CloakAndDaggerV1 => {
            let ctx = continuation.ctx.clone();
            let Some(Target::Player(opponent)) = ctx.targets.first().copied() else {
                return Ok(false);
            };
            if !ctx.target_incarnation_matches(0, state) {
                return Ok(false);
            }
            // "Until Cloak and Dagger leave the battlefield": nothing is
            // exiled once it has left.
            if live_ability_source(&ctx, state).is_none() {
                return Ok(false);
            }
            // "They reveal their hand."
            let hand = state.players[opponent.index()].hand.clone();
            for &card in &hand {
                // spellbench-public-reveal: the opponent reveals their hand (Cloak and Dagger).
                state.reveal_hand_card(controller, opponent, card)?;
            }
            let mut candidates = hand_cards(state, opponent, |def| !def.has_type(CardType::Land));
            let spec = TargetSpec::StandardV1(StandardTargetV1::OpponentThenUpToOneTheirCreatureV1);
            if let Some(creature) = legal_object_target(&ctx, state, 1, spec) {
                candidates.push(bind(state, creature));
            }
            if candidates.is_empty() {
                return Ok(false);
            }
            offer(
                continuation,
                E8aChoiceV1::CloakExileV1,
                controller,
                candidates,
                0,
                1,
                path,
            );
            Ok(true)
        }
        E8aOpV1::EmperorReturnV1 => {
            let Some(emperor) = source_link(&continuation.ctx) else {
                return Ok(false);
            };
            let cards = read_state(state)
                .map(|e8a| e8a.emperor_exiles.clone())
                .unwrap_or_default()
                .into_iter()
                .filter(|(host, card)| {
                    *host == emperor
                        && link_live_in(state, *card, Zone::Exile)
                        && definition(state, card.object)
                            .is_some_and(|def| def.has_type(CardType::Creature))
                })
                .map(|(_, card)| bind(state, card.object))
                .collect::<Vec<_>>();
            if cards.is_empty() {
                return Ok(false);
            }
            offer(
                continuation,
                E8aChoiceV1::EmperorReturnV1,
                controller,
                cards,
                1,
                1,
                path,
            );
            Ok(true)
        }
        E8aOpV1::DestroyThenSearchBasicTappedV1 => {
            let ctx = continuation.ctx.clone();
            let Some(target) = bound_target_in(&ctx, state, 0, Zone::Battlefield) else {
                return Ok(false);
            };
            let player = state.objects.get(target).controller;
            crate::effect::execute(
                &EffectOp::DestroyObject {
                    object: ObjectRef::Target(0),
                },
                &ctx,
                state,
            );
            let cards = library_cards(state, player, |def| {
                def.is_land && def.supertypes.contains(&crate::card_def::Supertype::Basic)
            });
            if cards.is_empty() {
                // Searching is optional; a player who searches finds
                // nothing.
                return Ok(false);
            }
            offer(
                continuation,
                E8aChoiceV1::BasicLandSearchV1,
                player,
                cards,
                0,
                1,
                path,
            );
            Ok(true)
        }
        E8aOpV1::YorionV1 => {
            let own = continuation
                .ctx
                .ability_source_contract
                .map(|contract| contract.source);
            let candidates = battlefield(state)
                .filter(|&object| {
                    let live = state.objects.get(object);
                    Some(object) != own
                        && live.controller == controller
                        && live.owner == controller
                        && !has_type(state, object, CardType::Land)
                })
                .map(|object| bind(state, object))
                .collect::<Vec<_>>();
            if candidates.is_empty() {
                return Ok(false);
            }
            let max = count_of(&candidates);
            offer(
                continuation,
                E8aChoiceV1::YorionExileV1,
                controller,
                candidates,
                0,
                max,
                path,
            );
            Ok(true)
        }
        E8aOpV1::InfernalTutorV1 => {
            let hand = hand_cards(state, controller, |_| true);
            if hand.is_empty() {
                // Hellbent: "search your library for a card".
                let cards = library_cards(state, controller, |_| true);
                if cards.is_empty() {
                    shuffle(state, controller)?;
                    return Ok(false);
                }
                offer(
                    continuation,
                    E8aChoiceV1::AnyCardSearchV1 { talisman: None },
                    controller,
                    cards,
                    1,
                    1,
                    path,
                );
                return Ok(true);
            }
            offer(
                continuation,
                E8aChoiceV1::InfernalRevealV1,
                controller,
                hand,
                1,
                1,
                path,
            );
            Ok(true)
        }
        E8aOpV1::LichsRelicV1 => {
            if crate::engine::can_pay_effect_mana(controller, &[], 2, state) {
                push_may(continuation, e8a(E8aOpV1::LichsRelicPayV1), &path);
            }
            Ok(false)
        }
        E8aOpV1::MayTargetToLibraryTopV1 => {
            if bound_target_in(&continuation.ctx, state, 0, Zone::Graveyard).is_some() {
                push_may(continuation, e8a(E8aOpV1::TargetToLibraryTopV1), &path);
            }
            Ok(false)
        }
        E8aOpV1::OverlordReturnV1 => {
            let cards = state.players[controller.index()]
                .graveyard
                .iter()
                .copied()
                .filter(|&card| {
                    let live = state.objects.get(card);
                    live.owner == controller
                        && !live.v4.is_token
                        && definition(state, card).is_some_and(|def| {
                            let avatar = def.changeling || def.subtypes.contains(&Subtype::Avatar);
                            (def.has_type(CardType::Creature) && !avatar)
                                || def.has_type(CardType::Planeswalker)
                        })
                })
                .map(|card| bind(state, card))
                .collect::<Vec<_>>();
            if cards.is_empty() {
                return Ok(false);
            }
            offer(
                continuation,
                E8aChoiceV1::OverlordReturnV1,
                controller,
                cards,
                0,
                1,
                path,
            );
            Ok(true)
        }
        E8aOpV1::RecruiterSearchV1 | E8aOpV1::StoneforgeSearchV1 => {
            let recruiter = *op == E8aOpV1::RecruiterSearchV1;
            let cards = library_cards(state, controller, |def| {
                if recruiter {
                    def.has_type(CardType::Creature)
                        && def.toughness.is_some_and(|toughness| toughness <= 2)
                } else {
                    def.subtypes.contains(&Subtype::Equipment)
                }
            });
            if cards.is_empty() {
                shuffle(state, controller)?;
                return Ok(false);
            }
            offer(
                continuation,
                E8aChoiceV1::LibraryToHandRevealV1,
                controller,
                cards,
                0,
                1,
                path,
            );
            Ok(true)
        }
        E8aOpV1::StoneforgePutV1 | E8aOpV1::AetherVialPutV1 => {
            let cards = if *op == E8aOpV1::StoneforgePutV1 {
                hand_cards(state, controller, |def| {
                    def.subtypes.contains(&Subtype::Equipment)
                })
            } else {
                let Some(source) = continuation.ctx.ability_source_contract else {
                    return Ok(false);
                };
                let charge = statics::vial_charge(state, source);
                hand_cards(state, controller, |def| {
                    def.has_type(CardType::Creature) && i32::from(def.mana_value) == charge
                })
            };
            if cards.is_empty() {
                return Ok(false);
            }
            offer(
                continuation,
                E8aChoiceV1::HandToBattlefieldV1,
                controller,
                cards,
                0,
                1,
                path,
            );
            Ok(true)
        }
        E8aOpV1::WishclawSearchV1 => {
            let talisman = live_ability_source(&continuation.ctx, state)
                .map(|object| live_link(state, object));
            let cards = library_cards(state, controller, |_| true);
            if cards.is_empty() {
                shuffle(state, controller)?;
                give_talisman(state, talisman, controller);
                return Ok(false);
            }
            offer(
                continuation,
                E8aChoiceV1::AnyCardSearchV1 { talisman },
                controller,
                cards,
                1,
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

/// Wishclaw Talisman: "An opponent gains control of this artifact."
fn give_talisman(state: &mut GameState, talisman: Option<ObjectLinkV4>, player: PlayerId) {
    if let Some(talisman) = talisman {
        if link_live_in(state, talisman, Zone::Battlefield) {
            crate::standard_cards_v1::gain_control(state, talisman.object, player.opponent());
        }
    }
}

/// Applies an answered choice of this module.
pub(crate) fn finish_selection(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    choice: &E3ChoiceV1,
    kind: &E8aChoiceV1,
    bindings: Vec<EffectObjectBinding>,
) -> Result<bool, String> {
    let player = choice.player;
    let chosen = bindings.first().copied();
    match kind {
        E8aChoiceV1::CloakExileV1 => {
            let Some(card) = chosen else {
                return Ok(false);
            };
            let Some(cloak) = source_link(&continuation.ctx) else {
                return Ok(false);
            };
            if !link_live_in(state, cloak, Zone::Battlefield) {
                return Ok(false);
            }
            let to_battlefield = card.expected_zone == Zone::Battlefield;
            event::propose_and_commit(state, ProposedEvent::zone_change(card.object, Zone::Exile));
            let live = state.objects.get(card.object);
            if live.zone == Zone::Exile && !live.v4.is_token {
                let exiled = live_link(state, card.object);
                e8a_state(state).cloak_exiles.push(CloakExileV1 {
                    source: cloak,
                    card: exiled,
                    to_battlefield,
                });
            }
            Ok(false)
        }
        E8aChoiceV1::EmperorReturnV1 => {
            let Some(card) = chosen else {
                return Ok(false);
            };
            let Some(source) = continuation.ctx.ability_source_contract else {
                return Ok(false);
            };
            put_onto_battlefield(state, card.object, player, false);
            if state.objects.get(card.object).zone != Zone::Battlefield {
                return Ok(false);
            }
            // "With a finality counter on it. It gains haste."
            let timestamp = crate::engine::next_timestamp(state);
            let upgrade = state
                .objects
                .get_mut(card.object)
                .v4
                .creature_upgrade
                .get_or_insert_with(Default::default);
            upgrade.finality = upgrade.finality.saturating_add(1);
            upgrade.keyword_grants.push((Keywords::HASTE, timestamp));
            let creature = live_link(state, card.object);
            let record = next_end_step(state, source, player, vec![creature]);
            e8a_state(state).end_step_sacrifices.push(record);
            Ok(false)
        }
        E8aChoiceV1::BasicLandSearchV1 => {
            if let Some(card) = chosen {
                put_onto_battlefield(state, card.object, player, true);
            }
            shuffle(state, player)?;
            Ok(false)
        }
        E8aChoiceV1::YorionExileV1 => {
            let objects = bindings
                .iter()
                .map(|binding| binding.object)
                .filter(|&object| state.objects.get(object).zone == Zone::Battlefield)
                .collect::<Vec<_>>();
            if objects.is_empty() {
                return Ok(false);
            }
            event::propose_and_commit_batch(
                state,
                objects
                    .iter()
                    .map(|&object| ProposedEvent::zone_change(object, Zone::Exile))
                    .collect(),
            );
            let cards = objects
                .into_iter()
                .filter(|&object| {
                    let live = state.objects.get(object);
                    live.zone == Zone::Exile && !live.v4.is_token
                })
                .map(|object| live_link(state, object))
                .collect::<Vec<_>>();
            let Some(source) = continuation.ctx.ability_source_contract else {
                return Ok(false);
            };
            if !cards.is_empty() {
                let record = next_end_step(state, source, player, cards);
                e8a_state(state).end_step_returns.push(record);
            }
            Ok(false)
        }
        E8aChoiceV1::InfernalRevealV1 => {
            let Some(card) = chosen else {
                return Err("Infernal Tutor reveals a card".to_string());
            };
            // spellbench-public-reveal: hand card revealed by the card rules.
            state.reveal_hand_card(player.opponent(), player, card.object)?;
            let Some(name) = definition(state, card.object).map(|def| def.object_name) else {
                return Ok(false);
            };
            let cards = library_cards(state, player, |def| def.object_name == name);
            if cards.is_empty() {
                shuffle(state, player)?;
                return Ok(false);
            }
            offer(
                continuation,
                E8aChoiceV1::InfernalSearchV1,
                player,
                cards,
                0,
                1,
                choice.path.clone(),
            );
            Ok(true)
        }
        E8aChoiceV1::InfernalSearchV1 | E8aChoiceV1::LibraryToHandRevealV1 => {
            if let Some(card) = chosen {
                to_hand_revealed(state, card.object)?;
            }
            shuffle(state, player)?;
            Ok(false)
        }
        E8aChoiceV1::AnyCardSearchV1 { talisman } => {
            if let Some(card) = chosen {
                event::propose_and_commit(
                    state,
                    ProposedEvent::zone_change(card.object, Zone::Hand),
                );
            }
            shuffle(state, player)?;
            give_talisman(state, *talisman, player);
            Ok(false)
        }
        E8aChoiceV1::OverlordReturnV1 => {
            if let Some(card) = chosen {
                event::propose_and_commit(
                    state,
                    ProposedEvent::zone_change(card.object, Zone::Hand),
                );
            }
            Ok(false)
        }
        E8aChoiceV1::HandToBattlefieldV1 => {
            if let Some(card) = chosen {
                if link_live_in(state, binding_link(&card), Zone::Hand) {
                    put_onto_battlefield(state, card.object, player, false);
                }
            }
            Ok(false)
        }
    }
}

// ---- Hooks: triggers ----------------------------------------------------------------

/// The triggers this module's hooks set off in this collection, and the
/// bookkeeping of its cards.
pub(crate) fn delayed_triggers(
    state: &mut GameState,
    events: &[CommittedEvent],
) -> Vec<PendingTrigger> {
    if !cfg!(feature = "eternal-fixtures") {
        return Vec::new();
    }
    note_spell_casts(state, events);
    note_entries(state, events);
    end_step_triggers(state, events);
    prune(state);
    let Some(eternal) = state.eternal_v1.as_mut() else {
        return Vec::new();
    };
    if eternal.e8a.queued_triggers.is_empty() {
        return Vec::new();
    }
    std::mem::take(&mut eternal.e8a.queued_triggers)
        .into_iter()
        .map(|queued| {
            let mut pending = crate::eternal_cards_v1::delayed_trigger(
                queued.source,
                queued.controller,
                e8a(queued.op),
            );
            if let Some(spec) = queued.target_spec {
                pending.target_spec = spec;
            }
            pending
        })
        .collect()
}

/// Spells cast: Solitude's evoke (recorded for wave E7b's evoke trigger),
/// the first spell each player casts (Once Upon a Time) and the Chandra
/// emblems' draws.
fn note_spell_casts(state: &mut GameState, events: &[CommittedEvent]) {
    for event in events {
        let CommittedEvent::SpellCast { spell, controller } = *event else {
            continue;
        };
        let Some(live) = state.objects.try_get(spell) else {
            continue;
        };
        if live.zone != Zone::Stack {
            continue;
        }
        let Some(item) = state
            .stack
            .iter()
            .find(|item| item.kind == StackItemKind::Spell && item.source == spell)
            .cloned()
        else {
            continue;
        };
        if !item.is_copy
            && item.v4.cast_method == Some(CastMethodV4::Alternative)
            && definition_name(state, spell) == Some(SOLITUDE)
        {
            let link = live_link(state, spell);
            let casts = &mut crate::eternal_vintage_e7b_v1::e7b_state(state).evoked_casts;
            if !casts.contains(&link) {
                casts.push(link);
            }
        }
        if !read_state(state).is_some_and(|e8a| e8a.spells_cast[controller.index()])
            && in_game(state, ONCE_UPON_A_TIME)
        {
            e8a_state(state).spells_cast[controller.index()] = true;
        }
        let emblems = read_state(state).map_or_else(Vec::new, |e8a| {
            e8a.chandra_emblems
                .iter()
                .filter(|(owner, _)| *owner == controller)
                .map(|(_, source)| *source)
                .collect::<Vec<_>>()
        });
        for source in emblems {
            queue_trigger(state, source, controller, E8aOpV1::DrawOneV1, None);
        }
    }
}

/// Permanents entering: Harbinger of the Seas' timestamp.
fn note_entries(state: &mut GameState, events: &[CommittedEvent]) {
    for event in events {
        let CommittedEvent::ZoneChange {
            object,
            to: Zone::Battlefield,
            ..
        } = *event
        else {
            continue;
        };
        if state.objects.get(object).zone != Zone::Battlefield
            || definition_name(state, object) != Some(HARBINGER_OF_THE_SEAS)
        {
            continue;
        }
        let link = live_link(state, object);
        if read_state(state).is_some_and(|e8a| e8a.harbingers.iter().any(|(host, _)| *host == link))
        {
            continue;
        }
        let timestamp = crate::engine::next_timestamp(state);
        e8a_state(state).harbingers.push((link, timestamp));
    }
}

/// "At the beginning of the next end step": Flickerwisp's and Yorion's
/// returns, Emperor of Bones' sacrifices.
fn end_step_triggers(state: &mut GameState, events: &[CommittedEvent]) {
    if !events
        .iter()
        .any(|event| matches!(event, CommittedEvent::BeginningEndStep { .. }))
    {
        return;
    }
    let Some(e8a) = read_state(state) else {
        return;
    };
    if e8a.end_step_returns.is_empty() && e8a.end_step_sacrifices.is_empty() {
        return;
    }
    let now = TurnV1::now(state);
    let (returns, waiting_returns): (Vec<_>, Vec<_>) = e8a
        .end_step_returns
        .iter()
        .cloned()
        .partition(|pending| pending.created_in_end_step != Some(now));
    let (sacrifices, waiting_sacrifices): (Vec<_>, Vec<_>) = e8a
        .end_step_sacrifices
        .iter()
        .cloned()
        .partition(|pending| pending.created_in_end_step != Some(now));
    let e8a = e8a_state(state);
    e8a.end_step_returns = waiting_returns;
    e8a.end_step_sacrifices = waiting_sacrifices;
    for pending in returns {
        queue_trigger(
            state,
            pending.source,
            pending.controller,
            E8aOpV1::ReturnExiledV1 {
                cards: pending.cards,
            },
            None,
        );
    }
    for pending in sacrifices {
        for creature in pending.cards {
            queue_trigger(
                state,
                pending.source,
                pending.controller,
                E8aOpV1::SacrificeCreatureV1 { creature },
                None,
            );
        }
    }
}

/// Drops records whose objects moved on.
fn prune(state: &mut GameState) {
    let Some(e8a) = read_state(state) else {
        return;
    };
    if e8a.is_empty() {
        return;
    }
    let now = TurnV1::now(state);
    let emperor_exiles = e8a
        .emperor_exiles
        .iter()
        .copied()
        .filter(|(_, card)| link_live_in(state, *card, Zone::Exile))
        .collect::<Vec<_>>();
    let illusions = e8a
        .illusions
        .iter()
        .copied()
        .filter(|(token, _)| link_live_in(state, *token, Zone::Battlefield))
        .collect::<Vec<_>>();
    let cloak_exiles = e8a
        .cloak_exiles
        .iter()
        .copied()
        .filter(|record| link_live_in(state, record.card, Zone::Exile))
        .collect::<Vec<_>>();
    let wish_counters = e8a
        .wish_counters
        .iter()
        .copied()
        .filter(|(host, _)| link_live_in(state, *host, Zone::Battlefield))
        .collect::<Vec<_>>();
    let rain_lands = e8a
        .rain_lands
        .iter()
        .copied()
        .filter(|(land, turn)| *turn == now && link_live_in(state, *land, Zone::Battlefield))
        .collect::<Vec<_>>();
    let harbingers = e8a
        .harbingers
        .iter()
        .copied()
        .filter(|(host, _)| link_live_in(state, *host, Zone::Battlefield))
        .collect::<Vec<_>>();
    let changed = emperor_exiles.len() != e8a.emperor_exiles.len()
        || illusions.len() != e8a.illusions.len()
        || cloak_exiles.len() != e8a.cloak_exiles.len()
        || wish_counters.len() != e8a.wish_counters.len()
        || rain_lands.len() != e8a.rain_lands.len()
        || harbingers.len() != e8a.harbingers.len();
    if changed {
        let e8a = e8a_state(state);
        e8a.emperor_exiles = emperor_exiles;
        e8a.illusions = illusions;
        e8a.cloak_exiles = cloak_exiles;
        e8a.wish_counters = wish_counters;
        e8a.rain_lands = rain_lands;
        e8a.harbingers = harbingers;
    }
}

/// The beginning of `player`'s turn ends their Teferi +1, and drops the
/// Skyclave Apparition records whose leave trigger can no longer come.
pub(crate) fn begin_turn(state: &mut GameState, player: PlayerId) {
    let Some(e8a) = read_state(state) else {
        return;
    };
    let stale = e8a
        .skyclave_exiles
        .iter()
        .any(|(host, _, _)| !link_live_in(state, *host, Zone::Battlefield));
    if !stale && !e8a.sorcery_flash.contains(&player) {
        return;
    }
    let kept = e8a
        .skyclave_exiles
        .iter()
        .copied()
        .filter(|(host, _, _)| link_live_in(state, *host, Zone::Battlefield))
        .collect::<Vec<_>>();
    let e8a = e8a_state(state);
    e8a.skyclave_exiles = kept;
    e8a.sorcery_flash.retain(|&flash| flash != player);
}

// ---- Hooks: engine ------------------------------------------------------------------

/// Once Upon a Time: "If this spell is the first spell you've cast this
/// game".
pub(crate) fn first_spell_this_game(state: &GameState, player: PlayerId) -> bool {
    !read_state(state).is_some_and(|e8a| e8a.spells_cast[player.index()])
}

/// Bitter Triumph's "pay 3 life" branch is its alternative cost, which
/// replaces the discard.
pub(crate) fn alternative_skips_additional_cost(definition: &CardDef, alternative: bool) -> bool {
    alternative && definition.name == "Bitter Triumph"
}

/// Cloak and Dagger: the cards it exiled return once it leaves the
/// battlefield (`left_zone_change_count` is its battlefield incarnation).
/// Called from the zone-change commit, so the return is immediate.
pub(crate) fn source_left_battlefield(
    state: &mut GameState,
    source: ObjectId,
    left_zone_change_count: u32,
) {
    let Some(e8a) = read_state(state) else {
        return;
    };
    if e8a.cloak_exiles.is_empty() {
        return;
    }
    let left = ObjectLinkV4 {
        object: source,
        zone_change_count: left_zone_change_count,
    };
    let (returning, kept): (Vec<_>, Vec<_>) = e8a
        .cloak_exiles
        .iter()
        .copied()
        .partition(|record| record.source == left);
    if returning.is_empty() {
        return;
    }
    e8a_state(state).cloak_exiles = kept;
    for record in returning {
        if !link_live_in(state, record.card, Zone::Exile) {
            continue;
        }
        if record.to_battlefield {
            let owner = state.objects.get(record.card.object).owner;
            put_onto_battlefield(state, record.card.object, owner, false);
        } else {
            event::propose_and_commit(
                state,
                ProposedEvent::zone_change(record.card.object, Zone::Hand),
            );
        }
    }
}
