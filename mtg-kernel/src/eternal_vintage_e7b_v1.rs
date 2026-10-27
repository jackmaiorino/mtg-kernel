//! Card programs for the eternal catalog's wave E7b
//! (`docs/design/eternal_formats_support_v1.md`): Vintage Dredge,
//! Counter-Vine, Blue Tempo, Lurrus DRS, Doomsday and RUG Control, and the
//! artifact cards of Stiflenought, Sphere Shops and Sultai Psychic Frog.
//!
//! `eternal_cards_v1` forwards its effect leaves, trigger conditions and
//! state here, and the engine calls the hooks below only in
//! `eternal-fixtures` builds. Every table is keyed by a printed name that
//! occurs only in the eternal registry, so the Pauper, FDN and Standard
//! catalogs never reach it. Behavior was read from each card's current
//! Oracle text and, where present, its XMage source.
//!
//! Primitives added here:
//! - dredge (702.52): a draw may be replaced by milling N and returning the
//!   dredge card, offered for every draw an effect makes inside the
//!   resumable interpreter and for the draw step's draw (`dredge`);
//! - triggers of cards milled from the library (Narcomoeba, Creeping
//!   Chill), graveyard upkeep and end-step returns (Ichorid, Squee, Master
//!   of Death, Silversmote Ghoul), Prized Amalgam's and Vengevine's
//!   graveyard triggers and Kishla Skimmer's once-each-turn draw;
//! - evoke by exiling a colored card (Grief, Subtlety), dash (Ragavan),
//!   Commandeer's two-card exile, Mindbreak Trap's free cast;
//! - spell control and retargeting (Commandeer), exiling spells (Mindbreak
//!   Trap), a spell to its owner's library top or bottom (Subtlety), copies
//!   of the resolving spell for another player (Chain of Vapor);
//! - statics (`statics`): spell taxes by type (Thorn of Amethyst, Lodestone
//!   Golem), entering permanents that cause no triggers (Torpor Orb, Karn,
//!   Argent Defender), Collector Ouphe's and Phyrexian Revoker's activation
//!   prohibitions, Crucible of Worlds' graveyard land plays, protection
//!   from multicolored (Stonecoil Serpent), regeneration (Golgari
//!   Grave-Troll) and Hollow One's discard count;
//! - planeswalkers (Dack Fayden, Wrenn and Six, Minsc & Boo), emblems,
//!   retrace from Wrenn and Six's emblem, empower Jace's planeswalker token
//!   and the loyalty ability Avatar of Burgeoning Echoes grants;
//! - Serum Powder's exile-and-redraw before each London mulligan choice.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

pub(crate) mod dredge;
pub mod statics;
pub(crate) mod targets;

use crate::card_def::{card_id_by_name, CardDef, CardType, Subtype, TargetSpec, CARD_DEFS};
use crate::effect::{
    EffectContinuation, EffectFrame, EffectObjectBinding, EffectOp, ExecCtx, LibraryCardFilter,
    LibrarySearchDestinationV1, ObjectRef, PlayerRef, TargetRef,
};
use crate::eternal_cards_v1::e3::{E3ChoiceKindV1, E3ChoiceV1};
use crate::eternal_cards_v1::EternalOpV1;
use crate::event::{self, CommittedEvent, ProposedEvent};
use crate::ids::{ObjectId, PlayerId, StackItemId};
use crate::mana::ManaColor;
use crate::standard_cards_v1::{StandardOpV1, StandardTargetV1, StandardTriggerV1};
use crate::state::{
    AbilitySourceContractV4, CastMethodV4, GameState, ObjectLinkV4, SpellCastRouteV4,
    StackItemKind, Step, Target, Zone,
};
use crate::trigger::{PendingTrigger, TriggerCondition, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};

// ---- State ------------------------------------------------------------------------

/// Per-game state for this module's cards, kept inside
/// `eternal_cards_v1::EternalStateV1`. Default (and absent from snapshots)
/// until one of these cards needs it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct E7bStateV1 {
    /// Triggers detected by hooks, waiting for the next trigger collection.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub queued_triggers: Vec<QueuedTriggerV1>,
    /// Grief and Subtlety spells cast for their evoke cost, by stack
    /// incarnation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evoked_casts: Vec<ObjectLinkV4>,
    /// Ragavan spells cast for their dash cost, by stack incarnation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dash_casts: Vec<ObjectLinkV4>,
    /// Battlefield incarnations that entered from a dashed spell: they have
    /// haste and return to hand at the beginning of the next end step.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dashed: Vec<ObjectLinkV4>,
    /// Uro spells cast with escape, by stack incarnation, kept while that
    /// spell or the permanent it became remains.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub escaped_casts: Vec<ObjectLinkV4>,
    /// Creature spells cast from their owner's graveyard, by stack
    /// incarnation, in games with a Prized Amalgam.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub graveyard_casts: Vec<ObjectLinkV4>,
    /// "At the beginning of the next end step" returns waiting for that end
    /// step (Prized Amalgam, dash).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub end_step_returns: Vec<EndStepReturnV1>,
    /// Phyrexian Revokers and the definition each one's name chose.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub revoker_names: Vec<(ObjectLinkV4, u16)>,
    /// Regeneration shields: the creature incarnation and the turn the
    /// shield lasts through.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shields: Vec<(ObjectLinkV4, TurnCountV1)>,
    /// Cards each player cycled or discarded this turn (Hollow One).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub discards: Vec<TurnCountV1>,
    /// Creature spells each player cast this turn (Vengevine).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub creature_spells: Vec<TurnCountV1>,
    /// Kishla Skimmers that triggered this turn.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skimmer_uses: Vec<(ObjectLinkV4, TurnCountV1)>,
    /// Dack Fayden emblems each player has.
    #[serde(default, skip_serializing_if = "no_emblems")]
    pub dack_emblems: [u8; 2],
    /// Wrenn and Six emblems each player has.
    #[serde(default, skip_serializing_if = "no_emblems")]
    pub wrenn_emblems: [u8; 2],
    /// The player whose draw-step draw waits for their dredge choice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dredge_draw: Option<PlayerId>,
    /// Spells whose control Commandeer changed, with the player who cast
    /// them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub casters: Vec<(StackItemId, PlayerId)>,
    /// Opening hands whose Serum Powder choice the player declined, cleared
    /// once the mulligans end.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub powder_declined: Vec<(PlayerId, Vec<ObjectLinkV4>)>,
}

fn no_emblems(emblems: &[u8; 2]) -> bool {
    emblems[0] == 0 && emblems[1] == 0
}

impl E7bStateV1 {
    pub(crate) fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// A count for one player during one turn (the turn key and its active
/// player).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TurnCountV1 {
    pub player: PlayerId,
    pub turn: u32,
    pub active: PlayerId,
    pub count: u32,
}

impl TurnCountV1 {
    fn now(state: &GameState, player: PlayerId) -> Self {
        Self {
            player,
            turn: state.turn_key_v1(),
            active: state.active_player,
            count: 0,
        }
    }

    fn current(&self, state: &GameState) -> bool {
        self.turn == state.turn_key_v1() && self.active == state.active_player
    }
}

/// A delayed "at the beginning of the next end step" return.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EndStepReturnV1 {
    pub source: AbilitySourceContractV4,
    pub controller: PlayerId,
    /// The incarnation that returns.
    pub card: ObjectLinkV4,
    /// `true` returns a graveyard card to the battlefield tapped (Prized
    /// Amalgam); `false` returns a permanent to its owner's hand (dash).
    pub to_battlefield: bool,
    /// The turn this was created during its end step, whose end step has
    /// already begun; it waits for a later one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_in_end_step: Option<(u32, PlayerId)>,
}

/// A triggered ability queued by a hook.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueuedTriggerV1 {
    pub source: AbilitySourceContractV4,
    pub controller: PlayerId,
    pub op: E7bOpV1,
    /// The target the ability's controller announces, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_spec: Option<TargetSpec>,
}

pub(crate) fn e7b_state(state: &mut GameState) -> &mut E7bStateV1 {
    &mut state.eternal_v1.get_or_insert_with(Default::default).e7b
}

pub(crate) fn read_state(state: &GameState) -> Option<&E7bStateV1> {
    state.eternal_v1.as_ref().map(|eternal| &eternal.e7b)
}

/// Trigger conditions owned by this module, carried as
/// `eternal_cards_v1::EternalTriggerV1::E7bV1`. Each checks its own source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum E7bTriggerV1 {
    /// "When this card is put into your graveyard from your library"
    /// (Narcomoeba, Creeping Chill). Functions from the graveyard.
    PutIntoGraveyardFromLibraryV1,
    /// Evoke (702.74a): "When this enters, if its evoke cost was paid, its
    /// controller sacrifices it" (Grief, Subtlety).
    EvokedEntersV1,
    /// "At the beginning of the end step" (Ichorid), on the battlefield.
    EachEndStepV1,
    /// "At the beginning of your end step, if you gained 3 or more life
    /// this turn" (Silversmote Ghoul), from the graveyard.
    YourEndStepIfGainedThreeLifeV1,
}

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::E7bV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum E7bOpV1 {
    /// Destroy target 0 if it is still legal (Abrupt Decay).
    DestroyTargetV1,
    /// Chain of Vapor's resolution. A choice.
    ChainOfVaporV1,
    /// Commandeer's resolution. A choice.
    CommandeerV1,
    /// Demonic Consultation with its chosen name.
    ConsultationV1 { named: u16 },
    /// Exile each target spell still legal (Mindbreak Trap).
    ExileTargetSpellsV1,
    /// Put target 0 from a graveyard on top of its owner's library (Noxious
    /// Revival, Golgari Thug).
    TargetCardToLibraryTopV1,
    /// Creeping Chill's graveyard trigger: exile the card; if it was
    /// exiled, 3 damage to each opponent and gain 3 life.
    ExileSourceThenDrainV1,
    /// Put this ability's source card from its graveyard onto the
    /// battlefield under its owner's control (Narcomoeba, Vengevine).
    ReturnSourceToBattlefieldV1 { tapped: bool },
    /// Silversmote Ghoul: recheck the life gained, then return it tapped.
    SilversmoteReturnV1,
    /// Return this ability's source card from its graveyard to its owner's
    /// hand (Squee, Goblin Nabob).
    ReturnSourceToHandV1,
    /// Master of Death: pay 1 life, then return it to hand.
    MasterOfDeathV1,
    /// Ichorid's upkeep trigger. A choice.
    IchoridV1,
    /// Prized Amalgam's trigger: schedule its return at the next end step.
    AmalgamScheduleV1,
    /// The delayed return of `card` (Prized Amalgam to the battlefield
    /// tapped, a dashed permanent to its owner's hand).
    EndStepReturnV1 {
        card: ObjectLinkV4,
        to_battlefield: bool,
    },
    /// Sacrifice this ability's source incarnation if its controller still
    /// controls it (Ichorid's end step, evoke).
    SacrificeSourceV1,
    /// Stinkweed Imp: destroy the creature it dealt combat damage to.
    DestroyDamagedCreatureV1 { creature: ObjectLinkV4 },
    /// Ragavan: create a Treasure, exile the top card of the damaged
    /// player's library; Ragavan's controller may cast it this turn.
    RagavanV1,
    /// Draw a card (Kishla Skimmer).
    DrawOneV1,
    /// Subtlety's entry trigger. A choice.
    SubtletyV1,
    /// The chosen branch of Subtlety: `spell` to its owner's library.
    SpellToLibraryV1 { spell: ObjectLinkV4, bottom: bool },
    /// Uro: sacrifice it unless it escaped.
    UroSacrificeUnlessEscapedV1,
    /// Uro: "you may put a land card from your hand onto the battlefield".
    /// A choice.
    UroPutLandV1,
    /// Golos: exile the top three cards; play them this turn free.
    GolosExileV1,
    /// Deathrite Shaman: one mana of `color`.
    AddManaV1 { color: ManaColor },
    /// Dack Fayden's -2: gain control of target artifact.
    GainControlOfTargetV1,
    /// Dack Fayden's -6 emblem.
    DackEmblemV1,
    /// A Dack Fayden emblem's trigger: gain control of `permanents`.
    GainControlOfPermanentsV1 { permanents: Vec<ObjectLinkV4> },
    /// Wrenn and Six's -7 emblem.
    WrennEmblemV1,
    /// Minsc & Boo's +1: three +1/+1 counters on target 0, if any.
    MinscCountersV1,
    /// Minsc & Boo's -2. A choice.
    MinscSacrificeV1,
    /// Minsc & Boo's reflexive trigger.
    MinscDamageV1 { power: i32, hamster: bool },
    /// Empower Jace `amount`. May be a choice.
    EmpowerJaceV1 { amount: u8 },
    /// The -10 ability Avatar of Burgeoning Echoes grants.
    AvatarCountersV1,
    /// Golgari Grave-Troll: regenerate this ability's source.
    RegenerateSourceV1,
    /// "You may [the leaf]" for a hook-queued trigger (Vengevine). A
    /// choice.
    MayV1(Box<E7bOpV1>),
}

impl E7bOpV1 {
    /// Whether this leaf can yield a player decision, so its program must
    /// use the resumable interpreter.
    pub(crate) fn contains_player_choice(&self) -> bool {
        matches!(
            self,
            Self::ChainOfVaporV1
                | Self::CommandeerV1
                | Self::IchoridV1
                | Self::SubtletyV1
                | Self::UroPutLandV1
                | Self::MinscSacrificeV1
                | Self::EmpowerJaceV1 { .. }
                | Self::MayV1(_)
        )
    }
}

/// Resolution choices owned by this module, carried as
/// `E3ChoiceKindV1::E7bV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum E7bChoiceV1 {
    /// Dredge: none draws the card, one dredge card is dredged instead.
    DredgeV1,
    /// Chain of Vapor: up to one land to sacrifice.
    ChainSacrificeV1,
    /// Chain of Vapor: up to one new target for a copy.
    ChainCopyTargetV1,
    /// Commandeer: the new target of `slot` of `spell`.
    RetargetSlotV1 {
        spell: StackItemId,
        slot: u8,
        chosen: Vec<Target>,
    },
    /// Ichorid: up to one other black creature card to exile.
    IchoridExileV1,
    /// Uro: up to one land card from hand.
    UroLandV1,
    /// Minsc & Boo: the creature to sacrifice.
    MinscSacrificeV1,
    /// Empower Jace: the Jace token that gets the counters.
    JaceTokenV1 { amount: u8 },
}

impl E7bChoiceV1 {
    /// Whether the candidates are hidden from the other player.
    pub(crate) fn chooser_private(&self) -> bool {
        matches!(self, Self::UroLandV1)
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

const AVATAR_OF_BURGEONING_ECHOES: &str = "Avatar of Burgeoning Echoes";
const CARNIVOROUS_CULTIVATOR: &str = "Carnivorous Cultivator";
const CREEPING_CHILL: &str = "Creeping Chill";
const DACK_FAYDEN: &str = "Dack Fayden";
const GOLGARI_GRAVE_TROLL: &str = "Golgari Grave-Troll";
const GOLGARI_THUG: &str = "Golgari Thug";
const GOLOS: &str = "Golos, Tireless Pilgrim";
const GRIEF: &str = "Grief";
const ICHORID: &str = "Ichorid";
const KISHLA_SKIMMER: &str = "Kishla Skimmer";
const MASTER_OF_DEATH: &str = "Master of Death";
const MINSC_AND_BOO: &str = "Minsc & Boo, Timeless Heroes";
const NARCOMOEBA: &str = "Narcomoeba";
const PRIZED_AMALGAM: &str = "Prized Amalgam";
const RAGAVAN: &str = "Ragavan, Nimble Pilferer";
const SHAMBLING_SHELL: &str = "Shambling Shell";
const SILVERSMOTE_GHOUL: &str = "Silversmote Ghoul";
const SQUEE: &str = "Squee, Goblin Nabob";
const STINKWEED_IMP: &str = "Stinkweed Imp";
const SUBTLETY: &str = "Subtlety";
const URO: &str = "Uro, Titan of Nature's Wrath";
const VENGEVINE: &str = "Vengevine";
const BOO_TOKEN: &str = "Boo Token";
const JACE_TOKEN: &str = "Jace Token";
const TREASURE_TOKEN: &str = "Treasure Token";

const COLORS: [ManaColor; 5] = [
    ManaColor::W,
    ManaColor::U,
    ManaColor::B,
    ManaColor::R,
    ManaColor::G,
];

fn e7b(op: E7bOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::E7bV1(op)))
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

fn binding_link(binding: &EffectObjectBinding) -> ObjectLinkV4 {
    ObjectLinkV4 {
        object: binding.object,
        zone_change_count: binding.expected_zone_change_count,
    }
}

fn definition(state: &GameState, object: ObjectId) -> Option<&'static CardDef> {
    let live = state.objects.try_get(object)?;
    CARD_DEFS.get(live.card_def as usize)
}

fn definition_name(state: &GameState, object: ObjectId) -> Option<&'static str> {
    definition(state, object).map(|def| def.name)
}

/// Whether `object` is a front-face battlefield permanent named `name`
/// whose printed abilities apply.
pub(crate) fn active_named(state: &GameState, object: ObjectId, name: &str) -> bool {
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

/// The cards (not tokens or copies) in `player`'s graveyard, in order.
fn graveyard_cards(state: &GameState, player: PlayerId) -> Vec<ObjectId> {
    state.players[player.index()]
        .graveyard
        .iter()
        .copied()
        .filter(|&card| {
            let live = state.objects.get(card);
            !live.v4.is_token && live.spell_copy_origin.is_none()
        })
        .collect()
}

/// The resolving ability's source, if it is still the incarnation that
/// created the ability, on the battlefield.
fn live_ability_source(ctx: &ExecCtx, state: &GameState) -> Option<ObjectId> {
    let contract = ctx.ability_source_contract?;
    let live = state.objects.try_get(contract.source)?;
    (live.zone == Zone::Battlefield && live.zone_change_count == contract.zone_change_count)
        .then_some(contract.source)
}

/// The resolving ability's source card, if it is still the graveyard
/// incarnation the ability was created from (or that its source left the
/// battlefield to become, for a dies trigger).
fn graveyard_ability_source(ctx: &ExecCtx, state: &GameState) -> Option<ObjectId> {
    let contract = ctx.ability_source_contract?;
    let zone_change_count = if contract.zone == Zone::Graveyard {
        contract.zone_change_count
    } else {
        contract.zone_change_count.checked_add(1)?
    };
    link_live_in(
        state,
        ObjectLinkV4 {
            object: contract.source,
            zone_change_count,
        },
        Zone::Graveyard,
    )
    .then_some(contract.source)
}

/// Target `index` if it still names its announced incarnation, in `zone`.
fn bound_target_in(ctx: &ExecCtx, state: &GameState, index: usize, zone: Zone) -> Option<ObjectId> {
    let Some(Target::Object(object)) = ctx.targets.get(index).copied() else {
        return None;
    };
    (ctx.target_incarnation_matches(index, state) && state.objects.get(object).zone == zone)
        .then_some(object)
}

fn queue_trigger(
    state: &mut GameState,
    source: AbilitySourceContractV4,
    controller: PlayerId,
    op: E7bOpV1,
    target_spec: Option<TargetSpec>,
) {
    e7b_state(state).queued_triggers.push(QueuedTriggerV1 {
        source,
        controller,
        op,
        target_spec,
    });
}

fn is_creature(state: &GameState, object: ObjectId) -> bool {
    crate::engine::object_has_type(state, object, CardType::Creature)
}

fn is_land(state: &GameState, object: ObjectId) -> bool {
    crate::engine::object_has_type(state, object, CardType::Land)
}

/// Whether any object in the game is the card named `name`.
fn in_game(state: &GameState, name: &str) -> bool {
    card_id_by_name(name)
        .is_some_and(|id| state.objects.iter().any(|(_, live)| live.card_def == id))
}

/// A spell or permanent's colors, as a count of its color bits.
fn color_count(state: &GameState, object: ObjectId) -> u32 {
    (crate::engine::object_color_mask(state, object) & 0b1_1111).count_ones()
}

fn draw(player: PlayerRef, count: u32) -> EffectOp {
    EffectOp::DrawCards { player, count }
}

/// "You may [effect]": a choice to do nothing or `effect`.
fn may(effect: EffectOp) -> EffectOp {
    EffectOp::Choice {
        controller: PlayerRef::Controller,
        options: vec![EffectOp::Sequence(Vec::new()), effect],
    }
}

// ---- Spells -------------------------------------------------------------------------

/// Abrupt Decay: "Destroy target nonland permanent with mana value 3 or
/// less." Its "can't be countered" is the card's printed flag.
pub fn abrupt_decay() -> EffectOp {
    e7b(E7bOpV1::DestroyTargetV1)
}

/// Assassin's Trophy: "Destroy target permanent an opponent controls. Its
/// controller may search their library for a basic land card, put it onto
/// the battlefield, then shuffle."
pub fn assassins_trophy() -> EffectOp {
    EffectOp::Sequence(vec![
        e7b(E7bOpV1::DestroyTargetV1),
        // The permanent's controller is the caster's opponent: the spell
        // targets a permanent an opponent controls, so it would have no
        // legal target otherwise. The search is private to that player.
        EffectOp::SearchLibraryCardsToDestination {
            player: PlayerRef::Opponent,
            filter: LibraryCardFilter::BasicLand,
            max_targets: 1,
            destination: LibrarySearchDestinationV1::Battlefield { tapped: false },
        },
    ])
}

/// Chain of Vapor: "Return target nonland permanent to its owner's hand.
/// Then that permanent's controller may sacrifice a land of their choice.
/// If the player does, they may copy this spell and may choose a new target
/// for that copy."
pub fn chain_of_vapor() -> EffectOp {
    e7b(E7bOpV1::ChainOfVaporV1)
}

/// Commandeer: "Gain control of target noncreature spell. You may choose
/// new targets for it."
pub fn commandeer() -> EffectOp {
    e7b(E7bOpV1::CommandeerV1)
}

/// Creeping Chill: "Creeping Chill deals 3 damage to each opponent and you
/// gain 3 life."
pub fn creeping_chill() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::DealDamage {
            target: TargetRef::Opponent,
            amount: 3,
        },
        EffectOp::GainLife {
            player: PlayerRef::Controller,
            amount: 3,
        },
    ])
}

/// Demonic Consultation: "Choose a card name." The name is chosen by first
/// letter, then from the names with that letter, among every card the
/// registry defines (`eternal_vintage_v1::nameable_definitions`).
pub fn demonic_consultation() -> EffectOp {
    EffectOp::Choice {
        controller: PlayerRef::Controller,
        options: crate::eternal_vintage_v1::nameable_definitions()
            .iter()
            .map(|ids| EffectOp::Choice {
                controller: PlayerRef::Controller,
                options: ids
                    .iter()
                    .map(|&named| e7b(E7bOpV1::ConsultationV1 { named }))
                    .collect(),
            })
            .collect(),
    }
}

/// Mindbreak Trap: "Exile any number of target spells."
pub fn mindbreak_trap() -> EffectOp {
    e7b(E7bOpV1::ExileTargetSpellsV1)
}

/// Noxious Revival: "Put target card from a graveyard on top of its
/// owner's library."
pub fn noxious_revival() -> EffectOp {
    e7b(E7bOpV1::TargetCardToLibraryTopV1)
}

/// Stonecoil Serpent: "This creature enters with X +1/+1 counters on it."
pub fn stonecoil_serpent() -> EffectOp {
    EffectOp::PutSourceOntoBattlefieldWithXPlusOneCounters
}

// ---- Activated abilities --------------------------------------------------------------

/// Bazaar of Baghdad: "Draw two cards, then discard three cards."
pub fn bazaar_of_baghdad() -> EffectOp {
    EffectOp::Sequence(vec![
        draw(PlayerRef::Controller, 2),
        EffectOp::DiscardCards {
            player: PlayerRef::Controller,
            count: 3,
        },
    ])
}

/// Golgari Grave-Troll: "Regenerate this creature."
pub fn regenerate_source() -> EffectOp {
    e7b(E7bOpV1::RegenerateSourceV1)
}

/// The mana of Deathrite Shaman's first ability: one mana of any color.
fn any_color_mana() -> EffectOp {
    EffectOp::Choice {
        controller: PlayerRef::Controller,
        options: COLORS
            .into_iter()
            .map(|color| e7b(E7bOpV1::AddManaV1 { color }))
            .collect(),
    }
}

fn exile_target() -> EffectOp {
    EffectOp::MoveObject {
        object: ObjectRef::Target(0),
        to_zone: Zone::Exile,
    }
}

/// Deathrite Shaman: "Exile target land card from a graveyard. Add one
/// mana of any color."
pub fn deathrite_mana() -> EffectOp {
    EffectOp::Sequence(vec![exile_target(), any_color_mana()])
}

/// Deathrite Shaman: "Exile target instant or sorcery card from a
/// graveyard. Each opponent loses 2 life."
pub fn deathrite_drain() -> EffectOp {
    EffectOp::Sequence(vec![
        exile_target(),
        EffectOp::LoseLife {
            player: PlayerRef::Opponent,
            amount: 2,
        },
    ])
}

/// Deathrite Shaman: "Exile target creature card from a graveyard. You gain
/// 2 life."
pub fn deathrite_gain() -> EffectOp {
    EffectOp::Sequence(vec![
        exile_target(),
        EffectOp::GainLife {
            player: PlayerRef::Controller,
            amount: 2,
        },
    ])
}

/// Golos, Tireless Pilgrim: "Exile the top three cards of your library. You
/// may play them this turn without paying their mana costs."
pub fn golos_exile() -> EffectOp {
    e7b(E7bOpV1::GolosExileV1)
}

/// Dack Fayden's +1: "Target player draws two cards, then discards two
/// cards."
pub fn dack_plus_one() -> EffectOp {
    EffectOp::Sequence(vec![
        draw(PlayerRef::Target(0), 2),
        EffectOp::DiscardCards {
            player: PlayerRef::Target(0),
            count: 2,
        },
    ])
}

/// Dack Fayden's -2: "Gain control of target artifact."
pub fn dack_minus_two() -> EffectOp {
    e7b(E7bOpV1::GainControlOfTargetV1)
}

/// Dack Fayden's -6: "You get an emblem with 'Whenever you cast a spell that
/// targets one or more permanents, gain control of those permanents.'"
pub fn dack_minus_six() -> EffectOp {
    e7b(E7bOpV1::DackEmblemV1)
}

/// Wrenn and Six's +1: "Return up to one target land card from your
/// graveyard to your hand."
pub fn wrenn_plus_one() -> EffectOp {
    EffectOp::MoveAllTargets {
        to_zone: Zone::Hand,
    }
}

/// Wrenn and Six's -1: "Wrenn and Six deals 1 damage to any target."
pub fn wrenn_minus_one() -> EffectOp {
    EffectOp::DealDamage {
        target: TargetRef::Target(0),
        amount: 1,
    }
}

/// Wrenn and Six's -7: "You get an emblem with 'Instant and sorcery cards in
/// your graveyard have retrace.'"
pub fn wrenn_minus_seven() -> EffectOp {
    e7b(E7bOpV1::WrennEmblemV1)
}

/// Retrace (702.81), from Wrenn and Six's emblem: "You may cast this card
/// from your graveyard by discarding a land card in addition to paying its
/// other costs." Whether `holder` may cast `object` from their graveyard
/// that way: an instant or sorcery card they own while they have the
/// emblem. Callers check the graveyard incarnation.
pub(crate) fn retrace_permission(state: &GameState, object: ObjectId, holder: PlayerId) -> bool {
    read_state(state).is_some_and(|e7b| e7b.wrenn_emblems[holder.index()] > 0)
        && state.objects.try_get(object).is_some_and(|live| {
            live.owner == holder
                && !live.v4.is_token
                && live.spell_copy_origin.is_none()
                && CARD_DEFS
                    .get(usize::from(live.card_def))
                    .is_some_and(|def| {
                        def.has_type(CardType::Instant) || def.has_type(CardType::Sorcery)
                    })
        })
}

/// Whether casting the graveyard incarnation (`object`, `zone_change_count`)
/// is a retrace cast: retrace allows it and no other permission does (a
/// card another permission allows is cast without the land discard).
fn retrace_cast(
    state: &GameState,
    object: ObjectId,
    holder: PlayerId,
    zone_change_count: u32,
) -> bool {
    retrace_permission(state, object, holder)
        && !crate::standard_cards_v1::graveyard_cast_granted_without_retrace(
            state,
            object,
            holder,
            zone_change_count,
        )
}

/// Whether the pending cast is a retrace cast.
pub(crate) fn pending_retrace(state: &GameState, pending: &crate::engine::PendingCast) -> bool {
    pending.origin_zone == Zone::Graveyard
        && pending
            .source_contract
            .spell_cast_origin
            .is_some_and(|origin| {
                origin.origin_zone == Zone::Graveyard
                    && matches!(
                        origin.route,
                        crate::state::SpellCastRouteV4::GraveyardPermissionV1 { holder, .. }
                            if holder == pending.controller
                    )
                    && retrace_cast(
                        state,
                        pending.spell,
                        pending.controller,
                        origin.origin_zone_change_count,
                    )
            })
}

/// Whether a quote for `player` casting `source` includes retrace's land
/// discard: the pending cast of `source`, or `source` in a graveyard.
pub(crate) fn retrace_quote(state: &GameState, source: ObjectId, player: PlayerId) -> bool {
    if let Some(pending) = state
        .engine
        .pending_cast
        .as_ref()
        .filter(|pending| pending.spell == source)
    {
        return pending.controller == player && pending_retrace(state, pending);
    }
    state.objects.try_get(source).is_some_and(|live| {
        live.zone == Zone::Graveyard && retrace_cast(state, source, player, live.zone_change_count)
    })
}

/// Minsc & Boo's +1: "Put three +1/+1 counters on up to one target creature
/// with trample or haste."
pub fn minsc_plus_one() -> EffectOp {
    e7b(E7bOpV1::MinscCountersV1)
}

/// Minsc & Boo's -2: "Sacrifice a creature. When you do, Minsc & Boo deals
/// X damage to any target, where X is that creature's power. If the
/// sacrificed creature was a Hamster, draw X cards."
pub fn minsc_minus_two() -> EffectOp {
    e7b(E7bOpV1::MinscSacrificeV1)
}

/// The Jace token's "[-1]: Surveil 1."
pub fn jace_token_surveil() -> EffectOp {
    EffectOp::Surveil {
        player: PlayerRef::Controller,
        count: 1,
    }
}

/// The Jace token's "[-3]: Draw a card."
pub fn jace_token_draw() -> EffectOp {
    draw(PlayerRef::Controller, 1)
}

/// The ability Avatar of Burgeoning Echoes grants: "Put a +1/+1 counter on
/// target creature for each land you control."
pub fn avatar_granted_counters() -> EffectOp {
    e7b(E7bOpV1::AvatarCountersV1)
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

const fn e7b_condition(condition: E7bTriggerV1) -> TriggerCondition {
    TriggerCondition::StandardV1(StandardTriggerV1::EternalV1(
        crate::eternal_cards_v1::EternalTriggerV1::E7bV1(condition),
    ))
}

const UPKEEP: TriggerCondition = TriggerCondition::BeginningOfUpkeep {
    controller_only: true,
};

fn sacrifice_source() -> EffectOp {
    e7b(E7bOpV1::SacrificeSourceV1)
}

/// Narcomoeba: "you may put it onto the battlefield."
fn narcomoeba_enters() -> EffectOp {
    may(e7b(E7bOpV1::ReturnSourceToBattlefieldV1 { tapped: false }))
}

/// Creeping Chill: "you may exile it. If you do, Creeping Chill deals 3
/// damage to each opponent and you gain 3 life."
fn creeping_chill_milled() -> EffectOp {
    may(e7b(E7bOpV1::ExileSourceThenDrainV1))
}

/// Golgari Thug: "put target creature card from your graveyard on top of
/// your library."
fn thug_dies() -> EffectOp {
    e7b(E7bOpV1::TargetCardToLibraryTopV1)
}

fn ichorid_upkeep() -> EffectOp {
    e7b(E7bOpV1::IchoridV1)
}

/// Squee, Goblin Nabob: "you may return this card from your graveyard to
/// your hand."
fn squee_upkeep() -> EffectOp {
    may(e7b(E7bOpV1::ReturnSourceToHandV1))
}

/// Master of Death: "When this creature enters, surveil 2."
fn master_surveil() -> EffectOp {
    EffectOp::Surveil {
        player: PlayerRef::Controller,
        count: 2,
    }
}

/// Master of Death: "you may pay 1 life. If you do, return it to your
/// hand."
fn master_upkeep() -> EffectOp {
    may(e7b(E7bOpV1::MasterOfDeathV1))
}

fn silversmote_return() -> EffectOp {
    e7b(E7bOpV1::SilversmoteReturnV1)
}

/// Grief: "target opponent reveals their hand. You choose a nonland card
/// from it. That player discards that card."
fn grief_discard() -> EffectOp {
    EffectOp::RevealTargetHandChooseNonlandDiscard {
        player: PlayerRef::Target(0),
    }
}

fn subtlety_enters() -> EffectOp {
    e7b(E7bOpV1::SubtletyV1)
}

fn uro_sacrifice() -> EffectOp {
    e7b(E7bOpV1::UroSacrificeUnlessEscapedV1)
}

/// Uro: "you gain 3 life and draw a card, then you may put a land card from
/// your hand onto the battlefield."
fn uro_enters_or_attacks() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::GainLife {
            player: PlayerRef::Controller,
            amount: 3,
        },
        draw(PlayerRef::Controller, 1),
        e7b(E7bOpV1::UroPutLandV1),
    ])
}

/// Golos: "you may search your library for a land card, put that card onto
/// the battlefield tapped, then shuffle."
fn golos_search() -> EffectOp {
    EffectOp::SearchLibraryToBattlefieldTapped {
        player: PlayerRef::Controller,
        filter: LibraryCardFilter::AnyLand,
    }
}

/// Minsc & Boo: "you may create Boo, a legendary 1/1 red Hamster creature
/// token with trample and haste."
fn minsc_boo() -> EffectOp {
    may(EffectOp::CreateToken {
        token_def: token_id(BOO_TOKEN),
        controller: PlayerRef::Controller,
    })
}

/// Avatar of Burgeoning Echoes: "empower Jace 2."
fn avatar_landfall() -> EffectOp {
    e7b(E7bOpV1::EmpowerJaceV1 { amount: 2 })
}

/// Ragavan: "create a Treasure token and exile the top card of that
/// player's library. Until end of turn, you may cast that card." In a
/// two-player game the damaged player is the controller's opponent.
fn ragavan_hit() -> EffectOp {
    e7b(E7bOpV1::RagavanV1)
}

/// Carnivorous Cultivator: "return target land card from your graveyard to
/// your hand."
fn cultivator_hit() -> EffectOp {
    EffectOp::MoveObject {
        object: ObjectRef::Target(0),
        to_zone: Zone::Hand,
    }
}

fn draw_one() -> EffectOp {
    draw(PlayerRef::Controller, 1)
}

const NARCOMOEBA_TRIGGERS: [TriggeredAbilityDef; 1] = [graveyard_trigger(
    e7b_condition(E7bTriggerV1::PutIntoGraveyardFromLibraryV1),
    narcomoeba_enters,
)];
const CREEPING_CHILL_TRIGGERS: [TriggeredAbilityDef; 1] = [graveyard_trigger(
    e7b_condition(E7bTriggerV1::PutIntoGraveyardFromLibraryV1),
    creeping_chill_milled,
)];
const GOLGARI_THUG_TRIGGERS: [TriggeredAbilityDef; 1] = [graveyard_trigger(
    TriggerCondition::LeftBattlefieldToGraveyard,
    thug_dies,
)];
const ICHORID_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(e7b_condition(E7bTriggerV1::EachEndStepV1), sacrifice_source),
    graveyard_trigger(UPKEEP, ichorid_upkeep),
];
const SQUEE_TRIGGERS: [TriggeredAbilityDef; 1] = [graveyard_trigger(UPKEEP, squee_upkeep)];
const MASTER_OF_DEATH_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(TriggerCondition::Etb, master_surveil),
    graveyard_trigger(UPKEEP, master_upkeep),
];
const SILVERSMOTE_TRIGGERS: [TriggeredAbilityDef; 1] = [graveyard_trigger(
    e7b_condition(E7bTriggerV1::YourEndStepIfGainedThreeLifeV1),
    silversmote_return,
)];
const GRIEF_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(TriggerCondition::Etb, grief_discard),
    trigger(
        e7b_condition(E7bTriggerV1::EvokedEntersV1),
        sacrifice_source,
    ),
];
const SUBTLETY_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(TriggerCondition::Etb, subtlety_enters),
    trigger(
        e7b_condition(E7bTriggerV1::EvokedEntersV1),
        sacrifice_source,
    ),
];
const URO_TRIGGERS: [TriggeredAbilityDef; 3] = [
    trigger(TriggerCondition::Etb, uro_sacrifice),
    trigger(TriggerCondition::Etb, uro_enters_or_attacks),
    trigger(TriggerCondition::Attacks, uro_enters_or_attacks),
];
const GOLOS_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(TriggerCondition::Etb, golos_search)];
const MINSC_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(TriggerCondition::Etb, minsc_boo),
    trigger(UPKEEP, minsc_boo),
];
const AVATAR_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(
    TriggerCondition::ControlledLandEnters,
    avatar_landfall,
)];
const RAGAVAN_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(
    TriggerCondition::DealsCombatDamageToPlayer,
    ragavan_hit,
)];
const CULTIVATOR_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(
    TriggerCondition::DealsCombatDamageToPlayer,
    cultivator_hit,
)];

/// Triggered abilities of this module's cards.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        NARCOMOEBA => &NARCOMOEBA_TRIGGERS,
        CREEPING_CHILL => &CREEPING_CHILL_TRIGGERS,
        GOLGARI_THUG => &GOLGARI_THUG_TRIGGERS,
        ICHORID => &ICHORID_TRIGGERS,
        SQUEE => &SQUEE_TRIGGERS,
        MASTER_OF_DEATH => &MASTER_OF_DEATH_TRIGGERS,
        SILVERSMOTE_GHOUL => &SILVERSMOTE_TRIGGERS,
        GRIEF => &GRIEF_TRIGGERS,
        SUBTLETY => &SUBTLETY_TRIGGERS,
        URO => &URO_TRIGGERS,
        GOLOS => &GOLOS_TRIGGERS,
        MINSC_AND_BOO => &MINSC_TRIGGERS,
        AVATAR_OF_BURGEONING_ECHOES => &AVATAR_TRIGGERS,
        RAGAVAN => &RAGAVAN_TRIGGERS,
        CARNIVOROUS_CULTIVATOR => &CULTIVATOR_TRIGGERS,
        _ => &[],
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
    if !own_trigger_effect(name, effect) {
        return None;
    }
    Some(match name {
        GOLGARI_THUG if *effect == thug_dies() => TargetSpec::CreatureCardInOwnGraveyard,
        GRIEF if *effect == grief_discard() => TargetSpec::TargetOpponent,
        SUBTLETY if *effect == subtlety_enters() => {
            TargetSpec::StandardV1(StandardTargetV1::UpToOneCreatureOrPlaneswalkerSpellV1)
        }
        CARNIVOROUS_CULTIVATOR if *effect == cultivator_hit() => {
            TargetSpec::StandardV1(StandardTargetV1::LandCardInOwnGraveyardV1)
        }
        MINSC_AND_BOO
            if matches!(
                effect,
                EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::E7bV1(
                    E7bOpV1::MinscDamageV1 { .. }
                )))
            ) =>
        {
            TargetSpec::AnyTarget
        }
        _ => TargetSpec::None,
    })
}

/// Queued and delayed triggers owned by this module's cards.
pub(crate) fn delayed_effect_matches(name: &str, effect: &EffectOp) -> bool {
    let EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::E7bV1(op))) = effect else {
        return false;
    };
    match name {
        PRIZED_AMALGAM => matches!(
            op,
            E7bOpV1::AmalgamScheduleV1 | E7bOpV1::EndStepReturnV1 { .. }
        ),
        RAGAVAN => matches!(op, E7bOpV1::EndStepReturnV1 { .. }),
        VENGEVINE => matches!(op, E7bOpV1::MayV1(_)),
        KISHLA_SKIMMER => false,
        STINKWEED_IMP => matches!(op, E7bOpV1::DestroyDamagedCreatureV1 { .. }),
        MINSC_AND_BOO => matches!(op, E7bOpV1::MinscDamageV1 { .. }),
        DACK_FAYDEN => matches!(op, E7bOpV1::GainControlOfPermanentsV1 { .. }),
        _ => false,
    }
}

/// Whether `condition` triggers on its source's arrival in the zone it now
/// occupies.
pub(crate) fn arrival_condition(condition: E7bTriggerV1) -> bool {
    matches!(
        condition,
        E7bTriggerV1::PutIntoGraveyardFromLibraryV1 | E7bTriggerV1::EvokedEntersV1
    )
}

pub(crate) fn trigger_matches(
    condition: E7bTriggerV1,
    events: &[CommittedEvent],
    index: usize,
    source: ObjectId,
    state: &GameState,
) -> bool {
    let Some(live) = state.objects.try_get(source) else {
        return false;
    };
    match condition {
        E7bTriggerV1::PutIntoGraveyardFromLibraryV1 => {
            matches!(
                events[index],
                CommittedEvent::ZoneChange {
                    object,
                    from: Zone::Library,
                    to: Zone::Graveyard,
                    ..
                } if object == source
            ) && live.zone == Zone::Graveyard
                && state.players[live.owner.index()]
                    .graveyard
                    .contains(&source)
        }
        E7bTriggerV1::EvokedEntersV1 => {
            let entered_from_stack = matches!(
                events[index],
                CommittedEvent::ZoneChange {
                    object,
                    from: Zone::Stack,
                    to: Zone::Battlefield,
                    ..
                } if object == source
            );
            let Some(stack) = live.zone_change_count.checked_sub(1) else {
                return false;
            };
            entered_from_stack
                && live.zone == Zone::Battlefield
                && read_state(state).is_some_and(|e7b| {
                    e7b.evoked_casts.contains(&ObjectLinkV4 {
                        object: source,
                        zone_change_count: stack,
                    })
                })
        }
        E7bTriggerV1::EachEndStepV1 => {
            matches!(events[index], CommittedEvent::BeginningEndStep { .. })
                && definition_name(state, source)
                    .is_some_and(|name| active_named(state, source, name))
        }
        E7bTriggerV1::YourEndStepIfGainedThreeLifeV1 => {
            matches!(
                events[index],
                CommittedEvent::BeginningEndStep { active_player, .. } if active_player == live.owner
            ) && live.zone == Zone::Graveyard
                && state.players[live.owner.index()]
                    .graveyard
                    .contains(&source)
                && crate::standard_cards_v1::life_gained_this_turn(state, live.owner) >= 3
        }
    }
}

// ---- Effect leaves ------------------------------------------------------------------

pub(crate) fn execute(op: &E7bOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        E7bOpV1::DestroyTargetV1 => {
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
        E7bOpV1::ConsultationV1 { named } => consultation(state, ctx.controller, *named),
        E7bOpV1::ExileTargetSpellsV1 => {
            for index in 0..ctx.targets.len() {
                let Some(spell) = bound_target_in(ctx, state, index, Zone::Stack) else {
                    continue;
                };
                if crate::engine::apply_live_stack_spell_departure(state, spell, Zone::Exile)
                    .is_err()
                {
                    halt(state, ctx.source);
                    return;
                }
            }
        }
        E7bOpV1::TargetCardToLibraryTopV1 => {
            if let Some(card) = bound_target_in(ctx, state, 0, Zone::Graveyard) {
                event::propose_and_commit(
                    state,
                    ProposedEvent::public_library_insert(card, event::LibraryPlacement::Top),
                );
            }
        }
        E7bOpV1::ExileSourceThenDrainV1 => {
            let Some(card) = graveyard_ability_source(ctx, state) else {
                return;
            };
            event::propose_and_commit(state, ProposedEvent::zone_change(card, Zone::Exile));
            if state.objects.get(card).zone == Zone::Exile {
                crate::effect::execute(&creeping_chill(), ctx, state);
            }
        }
        E7bOpV1::ReturnSourceToBattlefieldV1 { tapped } => {
            let Some(card) = graveyard_ability_source(ctx, state) else {
                return;
            };
            return_to_battlefield(state, card, *tapped);
        }
        E7bOpV1::SilversmoteReturnV1 => {
            let Some(card) = graveyard_ability_source(ctx, state) else {
                return;
            };
            let owner = state.objects.get(card).owner;
            if crate::standard_cards_v1::life_gained_this_turn(state, owner) >= 3 {
                return_to_battlefield(state, card, true);
            }
        }
        E7bOpV1::ReturnSourceToHandV1 => {
            if let Some(card) = graveyard_ability_source(ctx, state) {
                event::propose_and_commit(state, ProposedEvent::zone_change(card, Zone::Hand));
            }
        }
        E7bOpV1::MasterOfDeathV1 => {
            let Some(card) = graveyard_ability_source(ctx, state) else {
                return;
            };
            // "You may pay 1 life": a player with less than 1 life cannot.
            if state.players[ctx.controller.index()].life < 1 {
                return;
            }
            event::propose_and_commit(state, ProposedEvent::life_payment(ctx.controller, 1));
            event::propose_and_commit(state, ProposedEvent::zone_change(card, Zone::Hand));
        }
        E7bOpV1::AmalgamScheduleV1 => {
            let Some(contract) = ctx.ability_source_contract else {
                return;
            };
            let card = ObjectLinkV4 {
                object: contract.source,
                zone_change_count: contract.zone_change_count,
            };
            schedule_end_step_return(state, contract, ctx.controller, card, true);
        }
        E7bOpV1::EndStepReturnV1 {
            card,
            to_battlefield,
        } => {
            if *to_battlefield {
                if link_live_in(state, *card, Zone::Graveyard) {
                    return_to_battlefield(state, card.object, true);
                }
            } else if link_live_in(state, *card, Zone::Battlefield) {
                event::propose_and_commit(
                    state,
                    ProposedEvent::zone_change(card.object, Zone::Hand),
                );
            }
        }
        E7bOpV1::SacrificeSourceV1 => {
            if let Some(source) = live_ability_source(ctx, state) {
                if state.objects.get(source).controller == ctx.controller {
                    crate::effect::execute(
                        &EffectOp::Sacrifice {
                            object: ObjectRef::ThisSource,
                        },
                        &ExecCtx::no_targets(source, ctx.controller),
                        state,
                    );
                }
            }
        }
        E7bOpV1::DestroyDamagedCreatureV1 { creature } => {
            if link_live_in(state, *creature, Zone::Battlefield) {
                crate::effect::execute(
                    &EffectOp::DestroyObject {
                        object: ObjectRef::ThisSource,
                    },
                    &ExecCtx::no_targets(creature.object, ctx.controller),
                    state,
                );
            }
        }
        E7bOpV1::RagavanV1 => ragavan(state, ctx),
        E7bOpV1::DrawOneV1 => crate::effect::execute(&draw_one(), ctx, state),
        E7bOpV1::SpellToLibraryV1 { spell, bottom } => {
            if !link_live_in(state, *spell, Zone::Stack) {
                return;
            }
            let placement = if *bottom {
                event::LibraryPlacement::Bottom
            } else {
                event::LibraryPlacement::Top
            };
            if crate::engine::apply_live_stack_spell_departure_to_library_v1(
                state,
                spell.object,
                placement,
            )
            .is_err()
            {
                halt(state, ctx.source);
            }
        }
        E7bOpV1::UroSacrificeUnlessEscapedV1 => {
            let Some(contract) = ctx.ability_source_contract else {
                return;
            };
            let escaped = contract
                .zone_change_count
                .checked_sub(1)
                .is_some_and(|stack| {
                    read_state(state).is_some_and(|e7b| {
                        e7b.escaped_casts.contains(&ObjectLinkV4 {
                            object: contract.source,
                            zone_change_count: stack,
                        })
                    })
                });
            if !escaped {
                execute(&E7bOpV1::SacrificeSourceV1, ctx, state);
            }
        }
        E7bOpV1::GolosExileV1 => golos(state, ctx.controller),
        E7bOpV1::AddManaV1 { color } => {
            event::propose_and_commit(state, ProposedEvent::mana_add(ctx.controller, vec![*color]));
        }
        E7bOpV1::GainControlOfTargetV1 => {
            if let Some(artifact) = bound_target_in(ctx, state, 0, Zone::Battlefield) {
                if crate::engine::object_has_type(state, artifact, CardType::Artifact) {
                    crate::standard_cards_v1::gain_control(state, artifact, ctx.controller);
                }
            }
        }
        E7bOpV1::DackEmblemV1 => {
            let emblems = &mut e7b_state(state).dack_emblems[ctx.controller.index()];
            *emblems = emblems.saturating_add(1);
        }
        E7bOpV1::GainControlOfPermanentsV1 { permanents } => {
            for permanent in permanents {
                if link_live_in(state, *permanent, Zone::Battlefield) {
                    crate::standard_cards_v1::gain_control(state, permanent.object, ctx.controller);
                }
            }
        }
        E7bOpV1::WrennEmblemV1 => {
            let emblems = &mut e7b_state(state).wrenn_emblems[ctx.controller.index()];
            *emblems = emblems.saturating_add(1);
        }
        E7bOpV1::MinscCountersV1 => {
            if let Some(creature) = bound_target_in(ctx, state, 0, Zone::Battlefield) {
                if crate::engine::effect_target_is_legal(
                    state,
                    ctx.source,
                    ctx.controller,
                    TargetSpec::StandardV1(StandardTargetV1::UpToOneCreatureWithTrampleOrHasteV1),
                    &ctx.targets,
                    0,
                ) {
                    let _ = event::add_plus_one_counters(state, creature, ctx.controller, 3);
                }
            }
        }
        E7bOpV1::MinscDamageV1 { power, hamster } => {
            let amount = (*power).max(0);
            if amount > 0 {
                crate::effect::execute(
                    &EffectOp::DealDamage {
                        target: TargetRef::Target(0),
                        amount,
                    },
                    ctx,
                    state,
                );
            }
            if *hamster && amount > 0 {
                crate::effect::execute(
                    &draw(PlayerRef::Controller, u32::try_from(amount).unwrap_or(0)),
                    ctx,
                    state,
                );
            }
        }
        E7bOpV1::AvatarCountersV1 => {
            let Some(creature) = bound_target_in(ctx, state, 0, Zone::Battlefield) else {
                return;
            };
            if !is_creature(state, creature) {
                return;
            }
            let lands = battlefield(state)
                .filter(|&object| {
                    state.objects.get(object).controller == ctx.controller && is_land(state, object)
                })
                .count();
            if let Ok(count) = i32::try_from(lands) {
                if count > 0 {
                    let _ = event::add_plus_one_counters(state, creature, ctx.controller, count);
                }
            }
        }
        E7bOpV1::RegenerateSourceV1 => {
            if let Some(source) = live_ability_source(ctx, state) {
                statics::add_regeneration_shield(state, source);
            }
        }
        // Choices run in `drive`; the plain executor never reaches them.
        E7bOpV1::ChainOfVaporV1
        | E7bOpV1::CommandeerV1
        | E7bOpV1::IchoridV1
        | E7bOpV1::SubtletyV1
        | E7bOpV1::UroPutLandV1
        | E7bOpV1::MinscSacrificeV1
        | E7bOpV1::EmpowerJaceV1 { .. }
        | E7bOpV1::MayV1(_) => halt(state, ctx.source),
    }
}

/// Puts the graveyard card `card` onto the battlefield under its owner's
/// control.
fn return_to_battlefield(state: &mut GameState, card: ObjectId, tapped: bool) {
    let owner = state.objects.get(card).owner;
    let proposed = ProposedEvent::zone_change_to_battlefield_under_controller(card, owner);
    let proposed = if tapped {
        let ProposedEvent::ZoneChange(mut change) = proposed else {
            return;
        };
        change.force_battlefield_tapped = true;
        ProposedEvent::ZoneChange(change)
    } else {
        proposed
    };
    event::propose_and_commit(state, proposed);
}

fn schedule_end_step_return(
    state: &mut GameState,
    source: AbilitySourceContractV4,
    controller: PlayerId,
    card: ObjectLinkV4,
    to_battlefield: bool,
) {
    let created_in_end_step =
        (state.step == Step::End).then_some((state.turn_key_v1(), state.active_player));
    e7b_state(state).end_step_returns.push(EndStepReturnV1 {
        source,
        controller,
        card,
        to_battlefield,
        created_in_end_step,
    });
}

/// Demonic Consultation: "Exile the top six cards of your library, then
/// reveal cards from the top of your library until you reveal a card with
/// the chosen name. Put that card into your hand and exile all other cards
/// revealed this way."
fn consultation(state: &mut GameState, player: PlayerId, named: u16) {
    let Some(named_def) = CARD_DEFS.get(usize::from(named)) else {
        return;
    };
    let top_six = state.players[player.index()]
        .library
        .iter()
        .take(6)
        .copied()
        .collect::<Vec<_>>();
    if !top_six.is_empty() {
        event::propose_and_commit_batch(
            state,
            top_six
                .into_iter()
                .map(|card| ProposedEvent::zone_change(card, Zone::Exile))
                .collect(),
        );
    }
    let library = state.players[player.index()].library.clone();
    let found = library.iter().position(|&card| {
        definition(state, card).is_some_and(|def| def.object_name == named_def.object_name)
    });
    let revealed = found.map_or(library.len(), |position| position + 1);
    for observer in [PlayerId::P0, PlayerId::P1] {
        state.reveal_library_top(observer, player, revealed);
    }
    let exiled = library
        .iter()
        .take(revealed)
        .copied()
        .filter(|&card| Some(card) != found.map(|position| library[position]))
        .collect::<Vec<_>>();
    if let Some(position) = found {
        event::propose_and_commit(
            state,
            ProposedEvent::zone_change_preserving_known_identity(library[position], Zone::Hand),
        );
    }
    if !exiled.is_empty() {
        event::propose_and_commit_batch(
            state,
            exiled
                .into_iter()
                .map(|card| ProposedEvent::zone_change(card, Zone::Exile))
                .collect(),
        );
    }
}

/// Ragavan: a Treasure token, then the top card of the opponent's library
/// exiled; Ragavan's controller may cast it until end of turn.
fn ragavan(state: &mut GameState, ctx: &ExecCtx) {
    crate::effect::execute(
        &EffectOp::CreateToken {
            token_def: token_id(TREASURE_TOKEN),
            controller: PlayerRef::Controller,
        },
        ctx,
        state,
    );
    let player = ctx.controller.opponent();
    let Some(&top) = state.players[player.index()].library.first() else {
        return;
    };
    event::propose_and_commit(state, ProposedEvent::zone_change(top, Zone::Exile));
    let live = state.objects.get(top);
    if live.zone != Zone::Exile {
        return;
    }
    let zone_change_generation = live.zone_change_count;
    state
        .engine
        .exile_play_permissions
        .push(crate::engine::PlayPermission {
            without_mana_cost: crate::engine::FreeCastV1(false),
            object: top,
            holder: ctx.controller,
            zone_change_generation,
            play_or_cast: crate::engine::PlayOrCast::Cast,
            expiry: crate::engine::PlayPermissionExpiry::EndOfTurn,
        });
}

/// Golos: "Exile the top three cards of your library. You may play them
/// this turn without paying their mana costs."
fn golos(state: &mut GameState, player: PlayerId) {
    let top = state.players[player.index()]
        .library
        .iter()
        .take(3)
        .copied()
        .collect::<Vec<_>>();
    if top.is_empty() {
        return;
    }
    event::propose_and_commit_batch(
        state,
        top.iter()
            .map(|&card| ProposedEvent::zone_change(card, Zone::Exile))
            .collect(),
    );
    for card in top {
        let live = state.objects.get(card);
        if live.zone != Zone::Exile {
            continue;
        }
        let zone_change_generation = live.zone_change_count;
        state
            .engine
            .exile_play_permissions
            .push(crate::engine::PlayPermission {
                without_mana_cost: crate::engine::FreeCastV1(true),
                object: card,
                holder: player,
                zone_change_generation,
                play_or_cast: crate::engine::PlayOrCast::Play,
                expiry: crate::engine::PlayPermissionExpiry::EndOfTurn,
            });
    }
}

// ---- Choices ------------------------------------------------------------------------

/// Offers `choice` to `player` over the objects `candidates`.
#[allow(clippy::too_many_arguments)]
fn offer_objects(
    state: &GameState,
    continuation: &mut EffectContinuation,
    kind: E7bChoiceV1,
    player: PlayerId,
    candidates: &[ObjectId],
    min: u16,
    max: u16,
    path: Vec<u16>,
) {
    let bindings = candidates
        .iter()
        .map(|&object| bind(state, object))
        .collect::<Vec<_>>();
    let remaining = continuation.frames.clone();
    crate::eternal_cards_v1::e3::offer(
        continuation,
        E3ChoiceV1 {
            kind: E3ChoiceKindV1::E7bV1(kind),
            player,
            candidates: candidates
                .iter()
                .map(|&object| Target::Object(object))
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

/// Runs a leaf inside the resumable interpreter. `Ok(true)` when it staged a
/// choice and the continuation must suspend.
pub(crate) fn drive(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    op: &E7bOpV1,
    path: Vec<u16>,
) -> Result<bool, String> {
    let controller = continuation.ctx.controller;
    match op {
        E7bOpV1::ChainOfVaporV1 => {
            let ctx = continuation.ctx.clone();
            let Some(target) = bound_target_in(&ctx, state, 0, Zone::Battlefield) else {
                return Ok(false);
            };
            let returner = state.objects.get(target).controller;
            crate::effect::execute(
                &EffectOp::MoveObject {
                    object: ObjectRef::Target(0),
                    to_zone: Zone::Hand,
                },
                &ctx,
                state,
            );
            let lands = state.players[returner.index()]
                .battlefield
                .iter()
                .copied()
                .filter(|&object| {
                    state.objects.get(object).controller == returner && is_land(state, object)
                })
                .collect::<Vec<_>>();
            if lands.is_empty() {
                return Ok(false);
            }
            offer_objects(
                state,
                continuation,
                E7bChoiceV1::ChainSacrificeV1,
                returner,
                &lands,
                0,
                1,
                path,
            );
            Ok(true)
        }
        E7bOpV1::CommandeerV1 => {
            let ctx = continuation.ctx.clone();
            let Some(spell) = bound_target_in(&ctx, state, 0, Zone::Stack) else {
                return Ok(false);
            };
            let Some(item) = crate::engine::gain_control_of_spell_v1(state, spell, controller)
            else {
                return Ok(false);
            };
            stage_retarget_slot(state, continuation, item, 0, Vec::new(), path)
        }
        E7bOpV1::IchoridV1 => {
            let Some(card) = graveyard_ability_source(&continuation.ctx, state) else {
                return Ok(false);
            };
            let owner = state.objects.get(card).owner;
            let black = crate::card_def::mana_color_mask(ManaColor::B);
            let candidates = graveyard_cards(state, owner)
                .into_iter()
                .filter(|&other| {
                    other != card
                        && definition(state, other).is_some_and(|def| {
                            def.has_type(CardType::Creature)
                                && def.colors.iter().any(|&color| {
                                    crate::card_def::mana_color_mask(color) & black != 0
                                })
                        })
                })
                .collect::<Vec<_>>();
            if candidates.is_empty() {
                return Ok(false);
            }
            offer_objects(
                state,
                continuation,
                E7bChoiceV1::IchoridExileV1,
                controller,
                &candidates,
                0,
                1,
                path,
            );
            Ok(true)
        }
        E7bOpV1::SubtletyV1 => {
            let ctx = continuation.ctx.clone();
            let Some(spell) = bound_target_in(&ctx, state, 0, Zone::Stack) else {
                return Ok(false);
            };
            if !crate::engine::effect_target_is_legal(
                state,
                ctx.source,
                ctx.controller,
                TargetSpec::StandardV1(StandardTargetV1::UpToOneCreatureOrPlaneswalkerSpellV1),
                &ctx.targets,
                0,
            ) {
                return Ok(false);
            }
            // "Its owner puts it on their choice of the top or bottom of
            // their library."
            let owner = state.objects.get(spell).owner;
            let chooser = if owner == controller {
                PlayerRef::Controller
            } else {
                PlayerRef::Opponent
            };
            let link = live_link(state, spell);
            continuation.frames.push(EffectFrame::Program {
                op: EffectOp::Choice {
                    controller: chooser,
                    options: vec![
                        e7b(E7bOpV1::SpellToLibraryV1 {
                            spell: link,
                            bottom: false,
                        }),
                        e7b(E7bOpV1::SpellToLibraryV1 {
                            spell: link,
                            bottom: true,
                        }),
                    ],
                },
                path,
            });
            Ok(false)
        }
        E7bOpV1::UroPutLandV1 => {
            let lands = state.players[controller.index()]
                .hand
                .iter()
                .copied()
                .filter(|&card| {
                    definition(state, card).is_some_and(|def| def.has_type(CardType::Land))
                })
                .collect::<Vec<_>>();
            if lands.is_empty() {
                return Ok(false);
            }
            offer_objects(
                state,
                continuation,
                E7bChoiceV1::UroLandV1,
                controller,
                &lands,
                0,
                1,
                path,
            );
            Ok(true)
        }
        E7bOpV1::MinscSacrificeV1 => {
            let creatures = state.players[controller.index()]
                .battlefield
                .iter()
                .copied()
                .filter(|&object| {
                    state.objects.get(object).controller == controller && is_creature(state, object)
                })
                .collect::<Vec<_>>();
            if creatures.is_empty() {
                return Ok(false);
            }
            offer_objects(
                state,
                continuation,
                E7bChoiceV1::MinscSacrificeV1,
                controller,
                &creatures,
                1,
                1,
                path,
            );
            Ok(true)
        }
        E7bOpV1::EmpowerJaceV1 { amount } => {
            let tokens = jace_tokens(state, controller);
            match tokens.as_slice() {
                [] => {
                    let start = state.engine.event_history.len();
                    crate::effect::execute(
                        &EffectOp::CreateToken {
                            token_def: token_id(JACE_TOKEN),
                            controller: PlayerRef::Controller,
                        },
                        &continuation.ctx,
                        state,
                    );
                    let created = state.engine.event_history[start..]
                        .iter()
                        .find_map(|event| match event {
                            CommittedEvent::CreateToken { object, .. } => Some(*object),
                            _ => None,
                        });
                    if let Some(token) = created {
                        empower(state, token, *amount);
                    }
                    Ok(false)
                }
                [token] => {
                    empower(state, *token, *amount);
                    Ok(false)
                }
                _ => {
                    offer_objects(
                        state,
                        continuation,
                        E7bChoiceV1::JaceTokenV1 { amount: *amount },
                        controller,
                        &tokens,
                        1,
                        1,
                        path,
                    );
                    Ok(true)
                }
            }
        }
        E7bOpV1::MayV1(inner) => {
            continuation.frames.push(EffectFrame::Program {
                op: may(e7b((**inner).clone())),
                path,
            });
            Ok(false)
        }
        other => {
            execute(other, &continuation.ctx, state);
            Ok(false)
        }
    }
}

/// The Jace planeswalker tokens `player` controls.
fn jace_tokens(state: &GameState, player: PlayerId) -> Vec<ObjectId> {
    let Some(jace) = card_id_by_name(JACE_TOKEN) else {
        return Vec::new();
    };
    battlefield(state)
        .filter(|&object| {
            let live = state.objects.get(object);
            live.controller == player
                && live.v4.is_token
                && crate::engine::object_has_type(state, object, CardType::Planeswalker)
                && (live.card_def == jace
                    || crate::engine::has_effective_subtype(state, object, Subtype::Jace))
        })
        .collect()
}

/// Puts `amount` loyalty counters on the Jace token `token`.
fn empower(state: &mut GameState, token: ObjectId, amount: u8) {
    if state.objects.get(token).zone == Zone::Battlefield {
        crate::planeswalker_v1::change_loyalty(state, token, i32::from(amount));
    }
}

/// Commandeer: offers a new target for slot `slot` of the spell
/// `spell`, after `chosen` for the earlier slots. The current target stays
/// a candidate while it is still legal, so keeping it is a choice too.
fn stage_retarget_slot(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    spell: StackItemId,
    slot: u8,
    chosen: Vec<Target>,
    path: Vec<u16>,
) -> Result<bool, String> {
    let Some(candidates) = crate::engine::spell_slot_retarget_candidates_v1(state, spell, &chosen)
    else {
        // The spell left the stack or has no more slots.
        if !chosen.is_empty() {
            crate::engine::retarget_spell_targets_v1(state, spell, &chosen)?;
        }
        return Ok(false);
    };
    let bindings = candidates
        .iter()
        .filter_map(|target| match *target {
            Target::Object(object) => Some(bind(state, object)),
            _ => None,
        })
        .collect();
    let remaining = continuation.frames.clone();
    crate::eternal_cards_v1::e3::offer(
        continuation,
        E3ChoiceV1 {
            kind: E3ChoiceKindV1::E7bV1(E7bChoiceV1::RetargetSlotV1 {
                spell,
                slot,
                chosen,
            }),
            player: continuation.ctx.controller,
            candidates,
            bindings,
            min: 1,
            max: 1,
            ordered: false,
            path,
            remaining,
        },
    );
    Ok(true)
}

/// Applies an answered choice of this module.
pub(crate) fn finish_selection(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    choice: &E3ChoiceV1,
    kind: &E7bChoiceV1,
    bindings: Vec<EffectObjectBinding>,
    selected: &[Target],
) -> Result<bool, String> {
    let player = choice.player;
    let chosen = bindings.first().copied();
    if let Some(binding) = chosen {
        if !choice.bindings.contains(&binding) {
            return Err("the eternal choice picked an object it did not offer".to_string());
        }
    }
    match kind {
        E7bChoiceV1::DredgeV1 => {
            dredge::finish(state, player, chosen);
            Ok(false)
        }
        E7bChoiceV1::ChainSacrificeV1 => {
            let Some(land) = chosen else {
                return Ok(false);
            };
            if !link_live_in(state, binding_link(&land), Zone::Battlefield)
                || state.objects.get(land.object).controller != player
            {
                return Err("Chain of Vapor's land left its controller".to_string());
            }
            event::log_sacrifice(state, land.object);
            event::propose_and_commit(
                state,
                ProposedEvent::zone_change(land.object, Zone::Graveyard),
            );
            // "If the player does, they may copy this spell and may choose a
            // new target for that copy": choosing no target declines the
            // copy, whose only target (the returned permanent) is gone.
            let candidates = crate::engine::legal_spell_targets_for_v1(
                state,
                TargetSpec::NonlandPermanent,
                player,
                continuation.ctx.source,
            );
            if candidates.is_empty() {
                return Ok(false);
            }
            let objects = candidates
                .iter()
                .filter_map(|target| match *target {
                    Target::Object(object) => Some(object),
                    _ => None,
                })
                .collect::<Vec<_>>();
            offer_objects(
                state,
                continuation,
                E7bChoiceV1::ChainCopyTargetV1,
                player,
                &objects,
                0,
                1,
                choice.path.clone(),
            );
            Ok(true)
        }
        E7bChoiceV1::ChainCopyTargetV1 => {
            let Some(target) = chosen else {
                return Ok(false);
            };
            crate::engine::copy_resolving_spell_for_v1(
                state,
                continuation.ctx.source,
                player,
                Target::Object(target.object),
                TargetSpec::NonlandPermanent,
            )?;
            Ok(false)
        }
        E7bChoiceV1::RetargetSlotV1 {
            spell,
            slot,
            chosen: earlier,
        } => {
            let Some(&target) = selected.first() else {
                return Err("Commandeer's new target is missing".to_string());
            };
            let mut so_far = earlier.clone();
            so_far.push(target);
            stage_retarget_slot(
                state,
                continuation,
                *spell,
                slot.saturating_add(1),
                so_far,
                choice.path.clone(),
            )
        }
        E7bChoiceV1::IchoridExileV1 => {
            let Some(card) = chosen else {
                return Ok(false);
            };
            let Some(ichorid) = graveyard_ability_source(&continuation.ctx, state) else {
                return Ok(false);
            };
            if !link_live_in(state, binding_link(&card), Zone::Graveyard) {
                return Ok(false);
            }
            event::propose_and_commit(state, ProposedEvent::zone_change(card.object, Zone::Exile));
            if state.objects.get(card.object).zone == Zone::Exile {
                return_to_battlefield(state, ichorid, false);
            }
            Ok(false)
        }
        E7bChoiceV1::UroLandV1 => {
            if let Some(land) = chosen {
                if link_live_in(state, binding_link(&land), Zone::Hand) {
                    event::propose_and_commit(
                        state,
                        ProposedEvent::zone_change_to_battlefield_under_controller(
                            land.object,
                            player,
                        ),
                    );
                }
            }
            Ok(false)
        }
        E7bChoiceV1::MinscSacrificeV1 => {
            let Some(creature) = chosen else {
                return Err("Minsc & Boo sacrifices a creature".to_string());
            };
            if !link_live_in(state, binding_link(&creature), Zone::Battlefield) {
                return Err("Minsc & Boo's sacrifice left the battlefield".to_string());
            }
            // The power and Hamster type are read as it is sacrificed.
            let power = crate::engine::effective_power(state, creature.object);
            let hamster =
                crate::engine::has_effective_subtype(state, creature.object, Subtype::Hamster);
            event::log_sacrifice(state, creature.object);
            event::propose_and_commit(
                state,
                ProposedEvent::zone_change(creature.object, Zone::Graveyard),
            );
            // "When you do": a reflexive trigger, which targets.
            if let Some(source) = continuation.ctx.ability_source_contract {
                queue_trigger(
                    state,
                    source,
                    player,
                    E7bOpV1::MinscDamageV1 { power, hamster },
                    Some(TargetSpec::AnyTarget),
                );
            }
            Ok(false)
        }
        E7bChoiceV1::JaceTokenV1 { amount } => {
            if let Some(token) = chosen {
                if link_live_in(state, binding_link(&token), Zone::Battlefield) {
                    empower(state, token.object, *amount);
                }
            }
            Ok(false)
        }
    }
}

// ---- Hooks: triggers ----------------------------------------------------------------

/// The triggers this module's hooks set off in this collection, and the
/// bookkeeping of its casts.
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
    graveyard_watch_triggers(state, events);
    emblem_triggers(state, events);
    prune(state);
    let Some(eternal) = state.eternal_v1.as_mut() else {
        return Vec::new();
    };
    if eternal.e7b.queued_triggers.is_empty() {
        return Vec::new();
    }
    std::mem::take(&mut eternal.e7b.queued_triggers)
        .into_iter()
        .map(|queued| {
            let mut pending = crate::eternal_cards_v1::delayed_trigger(
                queued.source,
                queued.controller,
                e7b(queued.op),
            );
            if let Some(spec) = queued.target_spec {
                pending.target_spec = spec;
            }
            pending
        })
        .collect()
}

/// Spells cast this turn: evoke and dash casts, Uro's escape, the creature
/// spells Vengevine counts, Vengevine's triggers and the Dack Fayden
/// emblem's.
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
        let cast_origin = live.v4.spell_cast_origin;
        let Some(item) = state
            .stack
            .iter()
            .find(|item| item.kind == StackItemKind::Spell && item.source == spell)
            .cloned()
        else {
            continue;
        };
        let link = live_link(state, spell);
        let name = definition_name(state, spell);
        if !item.is_copy {
            let alternative = item.v4.cast_method == Some(CastMethodV4::Alternative);
            match name {
                Some(GRIEF | SUBTLETY) if alternative => {
                    let casts = &mut e7b_state(state).evoked_casts;
                    if !casts.contains(&link) {
                        casts.push(link);
                    }
                }
                Some(RAGAVAN) if alternative => {
                    let casts = &mut e7b_state(state).dash_casts;
                    if !casts.contains(&link) {
                        casts.push(link);
                    }
                }
                Some(URO)
                    if cast_origin.is_some_and(|origin| {
                        origin.route == SpellCastRouteV4::GraveyardEscape
                    }) =>
                {
                    let casts = &mut e7b_state(state).escaped_casts;
                    if !casts.contains(&link) {
                        casts.push(link);
                    }
                }
                _ => {}
            }
            if cast_origin.is_some_and(|origin| origin.origin_zone == Zone::Graveyard)
                && in_game(state, PRIZED_AMALGAM)
            {
                let casts = &mut e7b_state(state).graveyard_casts;
                if !casts.contains(&link) {
                    casts.push(link);
                }
            }
        }
        if in_game(state, VENGEVINE)
            && crate::engine::stack_spell_has_type(state, &item, CardType::Creature)
        {
            let turn = state_turn(state);
            let count = bump(&mut e7b_state(state).creature_spells, turn, controller);
            if count == 2 {
                vengevine_triggers(state, controller);
            }
        }
        dack_emblem_trigger(state, controller, &item);
    }
}

/// The turn key and active player, for `bump`.
fn state_turn(state: &GameState) -> (u32, PlayerId) {
    (state.turn_key_v1(), state.active_player)
}

/// Adds one to `player`'s count this turn and returns the new count.
fn bump(counts: &mut Vec<TurnCountV1>, turn: (u32, PlayerId), player: PlayerId) -> u32 {
    counts.retain(|count| count.turn == turn.0 && count.active == turn.1);
    match counts.iter_mut().find(|count| count.player == player) {
        Some(count) => {
            count.count = count.count.saturating_add(1);
            count.count
        }
        None => {
            counts.push(TurnCountV1 {
                player,
                turn: turn.0,
                active: turn.1,
                count: 1,
            });
            1
        }
    }
}

/// `player`'s count this turn.
pub(crate) fn count_this_turn(state: &GameState, counts: &[TurnCountV1], player: PlayerId) -> u32 {
    counts
        .iter()
        .find(|count| count.player == player && count.current(state))
        .map_or(0, |count| count.count)
}

/// Vengevine: "Whenever you cast a spell, if it's the second creature spell
/// you cast this turn, you may return this card from your graveyard to the
/// battlefield."
fn vengevine_triggers(state: &mut GameState, player: PlayerId) {
    let vengevines = graveyard_cards(state, player)
        .into_iter()
        .filter(|&card| definition_name(state, card) == Some(VENGEVINE))
        .collect::<Vec<_>>();
    for card in vengevines {
        let source = AbilitySourceContractV4::capture(state, card);
        queue_trigger(
            state,
            source,
            player,
            E7bOpV1::MayV1(Box::new(E7bOpV1::ReturnSourceToBattlefieldV1 {
                tapped: false,
            })),
            None,
        );
    }
}

/// A Dack Fayden emblem: "Whenever you cast a spell that targets one or more
/// permanents, gain control of those permanents." One trigger per emblem.
fn dack_emblem_trigger(state: &mut GameState, player: PlayerId, item: &crate::state::StackItem) {
    let emblems = read_state(state).map_or(0, |e7b| e7b.dack_emblems[player.index()]);
    if emblems == 0 {
        return;
    }
    let permanents = item
        .targets
        .iter()
        .filter_map(|target| match *target {
            Target::Object(object) if state.objects.get(object).zone == Zone::Battlefield => {
                Some(live_link(state, object))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    if permanents.is_empty() {
        return;
    }
    let source = AbilitySourceContractV4::capture(state, item.source);
    for _ in 0..emblems {
        queue_trigger(
            state,
            source,
            player,
            E7bOpV1::GainControlOfPermanentsV1 {
                permanents: permanents.clone(),
            },
            None,
        );
    }
}

/// Spells that just resolved into permanents: a dashed Ragavan gains haste
/// and returns at the next end step; a Carnivorous Cultivator's entry is
/// its own trigger. Also feeds Prized Amalgam.
fn note_entries(state: &mut GameState, events: &[CommittedEvent]) {
    for (index, event) in events.iter().enumerate() {
        let CommittedEvent::ZoneChange {
            object,
            from,
            to: Zone::Battlefield,
            ..
        } = *event
        else {
            continue;
        };
        let Some(live) = state.objects.try_get(object) else {
            continue;
        };
        if live.zone != Zone::Battlefield {
            continue;
        }
        if from == Zone::Stack {
            if let Some(stack) = live.zone_change_count.checked_sub(1) {
                let cast = ObjectLinkV4 {
                    object,
                    zone_change_count: stack,
                };
                if read_state(state).is_some_and(|e7b| e7b.dash_casts.contains(&cast)) {
                    let link = live_link(state, object);
                    let controller = live.controller;
                    let source = AbilitySourceContractV4::capture(state, object);
                    e7b_state(state).dashed.push(link);
                    schedule_end_step_return(state, source, controller, link, false);
                }
            }
        }
        if statics::entry_triggers_suppressed(state, object) {
            continue;
        }
        amalgam_triggers(state, events, index, object, from);
    }
}

/// Prized Amalgam: "Whenever a creature enters, if it entered from your
/// graveyard or you cast it from your graveyard, return this card from your
/// graveyard to the battlefield tapped at the beginning of the next end
/// step."
fn amalgam_triggers(
    state: &mut GameState,
    _events: &[CommittedEvent],
    _index: usize,
    object: ObjectId,
    from: Zone,
) {
    if !is_creature(state, object) {
        return;
    }
    let live = state.objects.get(object);
    let player = if from == Zone::Graveyard {
        live.owner
    } else if from == Zone::Stack {
        // "You cast it from your graveyard": the caster is its controller,
        // and the graveyard was the caster's own.
        let Some(stack) = live.zone_change_count.checked_sub(1) else {
            return;
        };
        let cast = ObjectLinkV4 {
            object,
            zone_change_count: stack,
        };
        if !read_state(state).is_some_and(|e7b| e7b.graveyard_casts.contains(&cast))
            || live.owner != live.controller
        {
            return;
        }
        live.controller
    } else {
        return;
    };
    let amalgams = graveyard_cards(state, player)
        .into_iter()
        .filter(|&card| card != object && definition_name(state, card) == Some(PRIZED_AMALGAM))
        .collect::<Vec<_>>();
    for amalgam in amalgams {
        let source = AbilitySourceContractV4::capture(state, amalgam);
        queue_trigger(state, source, player, E7bOpV1::AmalgamScheduleV1, None);
    }
}

/// "At the beginning of the next end step" returns.
fn end_step_triggers(state: &mut GameState, events: &[CommittedEvent]) {
    if !events
        .iter()
        .any(|event| matches!(event, CommittedEvent::BeginningEndStep { .. }))
    {
        return;
    }
    let Some(e7b) = read_state(state) else {
        return;
    };
    if e7b.end_step_returns.is_empty() {
        return;
    }
    let now = state_turn(state);
    let (due, waiting): (Vec<_>, Vec<_>) = e7b
        .end_step_returns
        .iter()
        .copied()
        .partition(|pending| pending.created_in_end_step != Some(now));
    e7b_state(state).end_step_returns = waiting;
    for pending in due {
        queue_trigger(
            state,
            pending.source,
            pending.controller,
            E7bOpV1::EndStepReturnV1 {
                card: pending.card,
                to_battlefield: pending.to_battlefield,
            },
            None,
        );
    }
}

/// Kishla Skimmer: "Whenever a card leaves your graveyard during your
/// turn, draw a card. This ability triggers only once each turn."
fn graveyard_watch_triggers(state: &mut GameState, events: &[CommittedEvent]) {
    for event in events {
        let CommittedEvent::ZoneChange {
            object,
            from: Zone::Graveyard,
            ..
        } = *event
        else {
            continue;
        };
        let Some(live) = state.objects.try_get(object) else {
            continue;
        };
        if live.v4.is_token || live.spell_copy_origin.is_some() {
            continue;
        }
        let owner = live.owner;
        if state.active_player != owner {
            continue;
        }
        let skimmers = active_permanents(state, KISHLA_SKIMMER, Some(owner)).collect::<Vec<_>>();
        for skimmer in skimmers {
            let link = live_link(state, skimmer);
            let used = read_state(state).is_some_and(|e7b| {
                e7b.skimmer_uses
                    .iter()
                    .any(|(used, turn)| *used == link && turn.current(state))
            });
            if used {
                continue;
            }
            let now = TurnCountV1::now(state, owner);
            let uses = &mut e7b_state(state).skimmer_uses;
            uses.retain(|(_, turn)| turn.turn == now.turn && turn.active == now.active);
            uses.push((link, now));
            let source = AbilitySourceContractV4::capture(state, skimmer);
            queue_trigger(state, source, owner, E7bOpV1::DrawOneV1, None);
        }
    }
}

/// Emblem triggers that read events other than casts: none yet.
fn emblem_triggers(_state: &mut GameState, _events: &[CommittedEvent]) {}

/// Drops cast records whose spells left the stack, dashed incarnations that
/// left the battlefield, expired shields and prepared records whose
/// creature left.
fn prune(state: &mut GameState) {
    let Some(e7b) = read_state(state) else {
        return;
    };
    let on_stack = |links: &[ObjectLinkV4]| {
        links
            .iter()
            .copied()
            .filter(|&link| link_live_in(state, link, Zone::Stack))
            .collect::<Vec<_>>()
    };
    let evoked = on_stack(&e7b.evoked_casts);
    let dash = on_stack(&e7b.dash_casts);
    let escaped = e7b
        .escaped_casts
        .iter()
        .copied()
        .filter(|&link| {
            link_live_in(state, link, Zone::Stack)
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
                            Zone::Battlefield,
                        )
                    })
        })
        .collect::<Vec<_>>();
    let graveyard_casts = on_stack(&e7b.graveyard_casts);
    let dashed = e7b
        .dashed
        .iter()
        .copied()
        .filter(|&link| link_live_in(state, link, Zone::Battlefield))
        .collect::<Vec<_>>();
    let shields = e7b
        .shields
        .iter()
        .copied()
        .filter(|(link, turn)| link_live_in(state, *link, Zone::Battlefield) && turn.current(state))
        .collect::<Vec<_>>();
    let casters = e7b
        .casters
        .iter()
        .copied()
        .filter(|(id, _)| state.stack.iter().any(|item| item.v4.stack_item_id == *id))
        .collect::<Vec<_>>();
    let changed = casters.len() != e7b.casters.len()
        || graveyard_casts.len() != e7b.graveyard_casts.len()
        || evoked.len() != e7b.evoked_casts.len()
        || dash.len() != e7b.dash_casts.len()
        || escaped.len() != e7b.escaped_casts.len()
        || dashed.len() != e7b.dashed.len()
        || shields.len() != e7b.shields.len();
    if changed {
        let e7b = e7b_state(state);
        e7b.casters = casters;
        e7b.graveyard_casts = graveyard_casts;
        e7b.evoked_casts = evoked;
        e7b.dash_casts = dash;
        e7b.escaped_casts = escaped;
        e7b.dashed = dashed;
        e7b.shields = shields;
    }
}

/// The player who cast `item`, if Commandeer gave it another controller.
pub(crate) fn spell_caster(state: &GameState, item: &crate::state::StackItem) -> Option<PlayerId> {
    read_state(state)?
        .casters
        .iter()
        .find(|(id, _)| *id == item.v4.stack_item_id)
        .map(|(_, caster)| *caster)
}

/// Records the caster of the spell `item` before its control changes.
pub(crate) fn note_spell_caster(state: &mut GameState, item: StackItemId, caster: PlayerId) {
    let casters = &mut e7b_state(state).casters;
    if !casters.iter().any(|(id, _)| *id == item) {
        casters.push((item, caster));
    }
}

/// Whether `object` is a battlefield incarnation that entered from a dashed
/// spell (it has haste).
pub(crate) fn dashed(state: &GameState, object: ObjectId) -> bool {
    state
        .objects
        .try_get(object)
        .is_some_and(|live| live.zone == Zone::Battlefield)
        && read_state(state).is_some_and(|e7b| e7b.dashed.contains(&live_link(state, object)))
}

// ---- Hooks: combat ------------------------------------------------------------------

/// Stinkweed Imp: "Whenever this creature deals combat damage to a
/// creature, destroy that creature." Read from a combat damage batch.
pub(crate) fn note_combat_damage(state: &mut GameState, events: &[CommittedEvent]) {
    let mut hits: Vec<(ObjectId, ObjectLinkV4)> = Vec::new();
    for event in events {
        let CommittedEvent::Damage {
            source,
            target: Target::Object(creature),
            amount,
        } = *event
        else {
            continue;
        };
        if amount <= 0 || !active_named(state, source, STINKWEED_IMP) {
            continue;
        }
        let Some(live) = state.objects.try_get(creature) else {
            continue;
        };
        if live.zone != Zone::Battlefield || !is_creature(state, creature) {
            continue;
        }
        let link = live_link(state, creature);
        if !hits.contains(&(source, link)) {
            hits.push((source, link));
        }
    }
    for (imp, creature) in hits {
        let controller = state.objects.get(imp).controller;
        let source = AbilitySourceContractV4::capture(state, imp);
        queue_trigger(
            state,
            source,
            controller,
            E7bOpV1::DestroyDamagedCreatureV1 { creature },
            None,
        );
    }
}
