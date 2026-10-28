//! Card programs for the eternal catalog's Pauper Ruby Storm, Golgari
//! Gardens, Familiars, Jeskai Ephemerate and smaller lists (wave E8b,
//! `docs/design/eternal_formats_support_v1.md`).
//!
//! `eternal_cards_v1` forwards its effect leaves, trigger conditions and
//! state here, and the engine calls the hooks below only in
//! `eternal-fixtures` builds. Every table is keyed by a printed name that
//! occurs only in the eternal registry, so the Pauper, FDN and Standard
//! catalogs never reach it. Behavior was read from each card's current
//! Oracle text and, where present, its XMage source.
//!
//! New rules here: splice onto Arcane (announced like replicate's count),
//! teamwork (an alternative cost tapping creatures through the convoke
//! selection), static cost reductions by spell color, depletion counters,
//! "the Ring tempts you" with the Ring's four abilities, an "enters this
//! turn" delayed trigger (First Day of Class), a next-end-step delayed
//! trigger over exiled cards (Glimpse the Impossible), "when you sacrifice
//! it", a characteristic-defining token (Seize the Storm), an activation
//! window (Desert) and "if this spell wasn't cast from your hand".
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

use crate::card_def::{
    card_id_by_name, CardDef, CardType, Keywords, Subtype, Supertype, TargetSpec, CARD_DEFS,
};
use crate::effect::{
    CreatureSacrificeFilter, EffectCond, EffectContinuation, EffectObjectBinding, EffectOp,
    ExecCtx, ImpulseDuration, LibraryCardFilter, LibraryLookCount, ObjectRef, PlayerRef, TargetRef,
};
use crate::eternal_cards_v1::e3::{E3ChoiceKindV1, E3ChoiceV1};
use crate::eternal_cards_v1::EternalOpV1;
use crate::event::{self, CommittedEvent, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::mana::{Cost, ManaColor, Pip};
use crate::standard_cards_v1::{
    StandardChosenActionV1, StandardOpV1, StandardPermanentFilterV1, StandardTriggerV1,
};
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
pub struct E8bStateV1 {
    /// Depletion counters on each Hickory Woodlot or Sandstone Needle
    /// incarnation. A battlefield depletion land without an entry has none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depletion: Vec<(ObjectLinkV4, u8)>,
    /// First Day of Class: "Whenever a creature you control enters this
    /// turn", one record per resolution.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub first_days: Vec<FirstDayV1>,
    /// Glimpse the Impossible's exiled cards, waiting for the next end step.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub glimpses: Vec<GlimpseV1>,
    /// How many times the Ring has tempted each player (capped at four, its
    /// last ability), indexed by `PlayerId::index`.
    #[serde(default, skip_serializing_if = "no_ring")]
    pub ring_levels: [u8; 2],
    /// Each player's Ring-bearer, by battlefield incarnation.
    #[serde(default, skip_serializing_if = "no_bearer")]
    pub ring_bearers: [Option<ObjectLinkV4>; 2],
    /// Creatures that blocked a Ring-bearer, sacrificed at end of combat.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ring_sacrifices: Vec<RingSacrificeV1>,
    /// Triggers detected by hooks, waiting for the next trigger collection.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub queued_triggers: Vec<QueuedTriggerV1>,
}

fn no_ring(levels: &[u8; 2]) -> bool {
    *levels == [0, 0]
}

fn no_bearer(bearers: &[Option<ObjectLinkV4>; 2]) -> bool {
    bearers.iter().all(Option::is_none)
}

impl E8bStateV1 {
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

    fn current(self, state: &GameState) -> bool {
        self == Self::now(state)
    }
}

/// A First Day of Class resolution: its spell source, its controller and
/// the turn it lasts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FirstDayV1 {
    pub source: AbilitySourceContractV4,
    pub controller: PlayerId,
    pub turn: TurnV1,
}

/// A Glimpse the Impossible resolution: the cards it exiled.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GlimpseV1 {
    pub source: AbilitySourceContractV4,
    pub controller: PlayerId,
    pub cards: Vec<ObjectLinkV4>,
}

/// "That creature's controller sacrifices it at end of combat."
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RingSacrificeV1 {
    pub controller: PlayerId,
    pub blocker: ObjectLinkV4,
    /// The Ring-bearer the blocked trigger came from.
    pub source: AbilitySourceContractV4,
}

/// A triggered ability queued by a hook.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueuedTriggerV1 {
    pub source: AbilitySourceContractV4,
    pub controller: PlayerId,
    pub effect: EffectOp,
}

fn e8b_state(state: &mut GameState) -> &mut E8bStateV1 {
    &mut state.eternal_v1.get_or_insert_with(Default::default).e8b
}

fn read_state(state: &GameState) -> Option<&E8bStateV1> {
    state.eternal_v1.as_ref().map(|eternal| &eternal.e8b)
}

/// Trigger conditions owned by this module, carried as
/// `eternal_cards_v1::EternalTriggerV1::E8bV1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum E8bTriggerV1 {
    /// "When this land enters untapped" (Witch's Cottage).
    EntersUntappedV1,
}

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::E8bV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum E8bOpV1 {
    /// Crypt Rats: `amount` damage to each creature and each player, one
    /// simultaneous batch.
    DamageEachCreatureAndPlayerV1 { amount: i32 },
    /// Defile: target creature gets -1/-1 until end of turn for each Swamp
    /// the controller controls.
    DefileV1,
    /// Skred: damage to target creature equal to the number of snow
    /// permanents the controller controls.
    SkredV1,
    /// The spliced Desperate Rituals' "Add {R}{R}{R}", once per splice (the
    /// spell's announced X).
    SplicedRitualsV1,
    /// First Day of Class: registers its "enters this turn" trigger.
    FirstDayOfClassV1,
    /// First Day of Class's trigger: a +1/+1 counter on `creature`, which
    /// gains haste until end of turn.
    FirstDayCounterV1 { creature: ObjectLinkV4 },
    /// Glimpse the Impossible: exile the top three cards, playable this
    /// turn, and register the end-step trigger.
    GlimpseV1,
    /// Glimpse the Impossible's end-step trigger: `cards` still in exile go
    /// to the graveyard, then an Eldrazi Spawn for each.
    GlimpseEndV1 { cards: Vec<ObjectLinkV4> },
    /// Glorious Gale: counter target creature spell; if it was legendary,
    /// the Ring tempts the controller. May be a choice.
    GloriousGaleV1,
    /// Crossover Collaboration: a Treasure if it was cast for its teamwork
    /// cost.
    TeamworkTreasureV1,
    /// Rancid Earth: destroy target land; with threshold, then 1 damage to
    /// each creature and each player.
    RancidEarthV1,
    /// Ride's End: exile target creature or Vehicle.
    ExileTargetV1,
    /// Twinned Vision: draw a card, or two if it wasn't cast from a hand.
    TwinnedVisionV1,
    /// Mortuary Mire, Witch's Cottage: put target creature card from the
    /// controller's graveyard on top of their library.
    PutTargetOnLibraryTopV1,
    /// The Ring's second ability: draw a card, then discard a card.
    RingLootV1,
    /// The Ring's third ability: `blocker`'s controller sacrifices it at end
    /// of combat (registers that delayed trigger).
    RingBlockedV1 { blocker: ObjectLinkV4 },
    /// The delayed trigger itself: `blocker`'s controller sacrifices it.
    RingSacrificeV1 { blocker: ObjectLinkV4 },
    /// The Ring's fourth ability: each opponent loses 3 life.
    RingDrainV1,
}

impl E8bOpV1 {
    /// Whether this leaf can yield a player decision, so its program must
    /// use the resumable interpreter.
    pub(crate) fn contains_player_choice(&self) -> bool {
        matches!(self, Self::GloriousGaleV1)
    }
}

/// Resolution choices owned by this module, carried as
/// `E3ChoiceKindV1::E8bV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum E8bChoiceV1 {
    /// "As the Ring tempts you, you choose a creature you control": the new
    /// Ring-bearer.
    RingBearerV1,
}

impl E8bChoiceV1 {
    /// Whether the candidates are hidden from the other player.
    pub(crate) fn chooser_private(&self) -> bool {
        false
    }

    pub(crate) fn public_purpose(&self) -> crate::rl::TargetSelectionPurposeV4 {
        crate::rl::TargetSelectionPurposeV4::PermanentSelection
    }

    /// Object incarnations this choice refers to beyond its candidates.
    pub(crate) fn referenced_bindings(&self) -> Vec<EffectObjectBinding> {
        Vec::new()
    }
}

// ---- Names and helpers --------------------------------------------------------------

const ACCURSED_MARAUDER: &str = "Accursed Marauder";
const AZORIUS_CHANCERY: &str = "Azorius Chancery";
const CRIMSON_FLEET_COMMODORE: &str = "Crimson Fleet Commodore";
const DESERT: &str = "Desert";
const DESPERATE_RITUAL: &str = "Desperate Ritual";
const FIRST_DAY_OF_CLASS: &str = "First Day of Class";
const GLIMPSE_THE_IMPOSSIBLE: &str = "Glimpse the Impossible";
const GOBLIN_ANARCHOMANCER: &str = "Goblin Anarchomancer";
const GOD_PHARAOHS_FAITHFUL: &str = "God-Pharaoh's Faithful";
const GOLGARI_ROT_FARM: &str = "Golgari Rot Farm";
const GOLIATH_PALADIN: &str = "Goliath Paladin";
const HEAPED_HARVEST: &str = "Heaped Harvest";
const HICKORY_WOODLOT: &str = "Hickory Woodlot";
const KHALNI_GARDEN: &str = "Khalni Garden";
const LONELY_ARROYO: &str = "Lonely Arroyo";
const MIRRAN_BANESPLITTER: &str = "Mirran Banesplitter";
const MORTUARY_MIRE: &str = "Mortuary Mire";
const OROKU_SAKI: &str = "Oroku Saki, Shredder Rising";
const ORZHOV_BASILICA: &str = "Orzhov Basilica";
const RED_ELEMENTAL_TOKEN: &str = "Red Elemental Token";
const SANDSTONE_NEEDLE: &str = "Sandstone Needle";
const SEA_GATE_ORACLE: &str = "Sea Gate Oracle";
const SUNSCAPE_FAMILIAR: &str = "Sunscape Familiar";
const THORNSCAPE_FAMILIAR: &str = "Thornscape Familiar";
const WITCHS_COTTAGE: &str = "Witch's Cottage";
const ELDRAZI_SPAWN_TOKEN: &str = "Eldrazi Spawn Token";
const PLANT_TOKEN: &str = "Plant Token";
const TREASURE_TOKEN: &str = "Treasure Token";

/// A depletion land "enters tapped with two depletion counters on it".
const DEPLETION_COUNTERS: u8 = 2;

/// Splice onto Arcane {1}{R} (Desperate Ritual).
const SPLICE_COST: Cost = Cost {
    pips: &[Pip::Colored(ManaColor::R)],
    generic: 1,
    x_count: 0,
};

/// The Ring's abilities: "Your Ring-bearer is legendary and can't be
/// blocked by creatures with greater power", then the attack loot, the
/// blocker sacrifice and the combat damage drain.
const RING_LEGENDARY: u8 = 1;
const RING_LOOT: u8 = 2;
const RING_SACRIFICE: u8 = 3;
const RING_DRAIN: u8 = 4;

const COLORS: [ManaColor; 5] = [
    ManaColor::W,
    ManaColor::U,
    ManaColor::B,
    ManaColor::R,
    ManaColor::G,
];

fn e8b(op: E8bOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::E8bV1(op)))
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

fn definition(state: &GameState, object: ObjectId) -> Option<&'static CardDef> {
    let live = state.objects.try_get(object)?;
    CARD_DEFS.get(live.card_def as usize)
}

fn definition_name(state: &GameState, object: ObjectId) -> Option<&'static str> {
    definition(state, object).map(|def| def.name)
}

/// Whether `object` is a battlefield permanent named `name` whose printed
/// abilities apply.
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

/// Target `index` if it still names its announced incarnation, in `zone`.
fn bound_target_in(ctx: &ExecCtx, state: &GameState, index: usize, zone: Zone) -> Option<ObjectId> {
    let Some(Target::Object(object)) = ctx.targets.get(index).copied() else {
        return None;
    };
    (ctx.target_incarnation_matches(index, state) && state.objects.get(object).zone == zone)
        .then_some(object)
}

/// A trigger set off outside any event batch (declaring blockers, the
/// beginning of the end of combat step), placed directly with the pending
/// triggers: an empty trigger collection does not read the queue.
fn push_trigger(
    state: &mut GameState,
    source: AbilitySourceContractV4,
    controller: PlayerId,
    effect: EffectOp,
) {
    let pending = crate::eternal_cards_v1::delayed_trigger(source, controller, effect);
    state.engine.pending_triggers.push(pending);
}

fn queue_trigger(
    state: &mut GameState,
    source: AbilitySourceContractV4,
    controller: PlayerId,
    effect: EffectOp,
) {
    e8b_state(state).queued_triggers.push(QueuedTriggerV1 {
        source,
        controller,
        effect,
    });
}

/// The current source contract of the spell card `recorded` captured on the
/// stack, controlled by `controller`: the spell has left the stack, so a
/// delayed trigger's source is the card where it is now.
fn spell_delayed_source(
    state: &GameState,
    recorded: AbilitySourceContractV4,
    controller: PlayerId,
) -> Option<AbilitySourceContractV4> {
    let live = state.objects.try_get(recorded.source)?;
    if live.zone == Zone::Stack
        || live.card_def != recorded.card_def
        || live.owner != recorded.owner
        || live.zone_change_count < recorded.zone_change_count
    {
        return None;
    }
    let mut contract = AbilitySourceContractV4::capture(state, recorded.source);
    contract.controller = controller;
    Some(contract)
}

/// One simultaneous batch of `amount` damage from `source` to each creature
/// and each player (Crypt Rats, Rancid Earth with threshold).
fn damage_each_creature_and_player(state: &mut GameState, source: ObjectId, amount: i32) {
    if amount <= 0 {
        return;
    }
    let mut events = battlefield(state)
        .filter(|&object| is_creature(state, object))
        .map(|object| ProposedEvent::damage(source, Target::Object(object), amount))
        .collect::<Vec<_>>();
    let active = state.active_player;
    for player in [active, active.opponent()] {
        events.push(ProposedEvent::damage(
            source,
            Target::Player(player),
            amount,
        ));
    }
    event::propose_and_commit_batch(state, events);
}

/// Battlefield permanents `player` controls.
fn controlled(state: &GameState, player: PlayerId) -> impl Iterator<Item = ObjectId> + '_ {
    battlefield(state).filter(move |&object| state.objects.get(object).controller == player)
}

// ---- Spells -------------------------------------------------------------------------

/// Crossover Collaboration: "Exile the top two cards of your library. Until
/// the end of your next turn, you may play those cards. If this spell was
/// cast using teamwork, create a Treasure token."
pub fn crossover_collaboration() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::ImpulseDraw {
            count: 2,
            duration: ImpulseDuration::UntilOwnersNextTurn,
        },
        e8b(E8bOpV1::TeamworkTreasureV1),
    ])
}

/// Defile: "Target creature gets -1/-1 until end of turn for each Swamp you
/// control."
pub fn defile() -> EffectOp {
    e8b(E8bOpV1::DefileV1)
}

/// Desperate Ritual: "Add {R}{R}{R}." Each Desperate Ritual spliced onto it
/// adds {R}{R}{R} more.
pub fn desperate_ritual() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::AddMana {
            player: PlayerRef::Controller,
            colors: vec![ManaColor::R; 3],
        },
        spliced_effects(),
    ])
}

/// The text spliced Desperate Rituals add to an Arcane spell (702.47b):
/// "Add {R}{R}{R}" for each. Every Arcane spell's program ends with it.
pub fn spliced_effects() -> EffectOp {
    e8b(E8bOpV1::SplicedRitualsV1)
}

/// First Day of Class: "Whenever a creature you control enters this turn,
/// put a +1/+1 counter on it and it gains haste until end of turn. Learn."
/// Learn finds no Lesson outside the game in a best-of-one mainboard game,
/// so it is "you may discard a card. If you do, draw a card."
pub fn first_day_of_class() -> EffectOp {
    EffectOp::Sequence(vec![
        e8b(E8bOpV1::FirstDayOfClassV1),
        EffectOp::MayPayCostThen {
            discard: 1,
            sacrifice_lands: 0,
            return_permanent: None,
            then: Box::new(EffectOp::DrawCards {
                player: PlayerRef::Controller,
                count: 1,
            }),
            otherwise: None,
        },
    ])
}

/// Glimpse the Impossible: "Exile the top three cards of your library. You
/// may play those cards this turn. At the beginning of the next end step,
/// if any of those cards remain exiled, put them into your graveyard, then
/// create a 0/1 colorless Eldrazi Spawn creature token for each card put
/// into your graveyard this way."
pub fn glimpse_the_impossible() -> EffectOp {
    e8b(E8bOpV1::GlimpseV1)
}

/// Glorious Gale: "Counter target creature spell. If it was a legendary
/// spell, the Ring tempts you."
pub fn glorious_gale() -> EffectOp {
    e8b(E8bOpV1::GloriousGaleV1)
}

/// Manamorphose: "Add two mana in any combination of colors. Draw a card."
pub fn manamorphose() -> EffectOp {
    let mut options = Vec::new();
    for (index, &first) in COLORS.iter().enumerate() {
        for &second in &COLORS[index..] {
            options.push(EffectOp::AddMana {
                player: PlayerRef::Controller,
                colors: vec![first, second],
            });
        }
    }
    EffectOp::Sequence(vec![
        EffectOp::Choice {
            controller: PlayerRef::Controller,
            options,
        },
        EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 1,
        },
    ])
}

/// Rancid Earth: "Destroy target land. Threshold -- If there are seven or
/// more cards in your graveyard, instead destroy that land and Rancid Earth
/// deals 1 damage to each creature and each player."
pub fn rancid_earth() -> EffectOp {
    e8b(E8bOpV1::RancidEarthV1)
}

/// Ride's End: "Exile target creature or Vehicle." Its cost reduction is
/// the card's `GenericCostReductionDef`.
pub fn rides_end() -> EffectOp {
    e8b(E8bOpV1::ExileTargetV1)
}

/// Seething Song: "Add {R}{R}{R}{R}{R}."
pub fn seething_song() -> EffectOp {
    EffectOp::AddMana {
        player: PlayerRef::Controller,
        colors: vec![ManaColor::R; 5],
    }
}

/// Skred: "Skred deals damage to target creature equal to the number of
/// snow permanents you control."
pub fn skred() -> EffectOp {
    e8b(E8bOpV1::SkredV1)
}

/// Twinned Vision: "Draw a card. If this spell wasn't cast from your hand,
/// draw two cards instead."
pub fn twinned_vision() -> EffectOp {
    e8b(E8bOpV1::TwinnedVisionV1)
}

/// Crypt Rats' ability for each X.
fn crypt_rats(amount: i32) -> EffectOp {
    e8b(E8bOpV1::DamageEachCreatureAndPlayerV1 { amount })
}

pub fn crypt_rats_1() -> EffectOp {
    crypt_rats(1)
}
pub fn crypt_rats_2() -> EffectOp {
    crypt_rats(2)
}
pub fn crypt_rats_3() -> EffectOp {
    crypt_rats(3)
}
pub fn crypt_rats_4() -> EffectOp {
    crypt_rats(4)
}
pub fn crypt_rats_5() -> EffectOp {
    crypt_rats(5)
}
pub fn crypt_rats_6() -> EffectOp {
    crypt_rats(6)
}
pub fn crypt_rats_7() -> EffectOp {
    crypt_rats(7)
}
pub fn crypt_rats_8() -> EffectOp {
    crypt_rats(8)
}
pub fn crypt_rats_9() -> EffectOp {
    crypt_rats(9)
}
pub fn crypt_rats_10() -> EffectOp {
    crypt_rats(10)
}

/// Reckless Lackey: "Draw a card and create a Treasure token."
pub fn reckless_lackey() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 1,
        },
        EffectOp::CreateToken {
            token_def: token_id(TREASURE_TOKEN),
            controller: PlayerRef::Controller,
        },
    ])
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

const fn e8b_trigger(condition: E8bTriggerV1, effect: fn() -> EffectOp) -> TriggeredAbilityDef {
    trigger(
        TriggerCondition::StandardV1(StandardTriggerV1::EternalV1(
            crate::eternal_cards_v1::EternalTriggerV1::E8bV1(condition),
        )),
        effect,
    )
}

/// Accursed Marauder: "each player sacrifices a nontoken creature of their
/// choice", the active player first.
fn each_player_sacrifices() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::SacrificeCreature {
            player: PlayerRef::Controller,
            filter: CreatureSacrificeFilter::Nontoken,
        },
        EffectOp::SacrificeCreature {
            player: PlayerRef::Opponent,
            filter: CreatureSacrificeFilter::Nontoken,
        },
    ])
}

/// The karoos: "return a land you control to its owner's hand." Not
/// targeted; chosen on resolution, and it may be this land.
fn return_controlled_land() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::PlayerChoosesControlledPermanent {
        player: PlayerRef::Controller,
        filter: StandardPermanentFilterV1::Land,
        action: StandardChosenActionV1::ReturnToOwnersHand,
    })
}

fn become_monarch() -> EffectOp {
    EffectOp::BecomeMonarch
}

fn gain_one() -> EffectOp {
    EffectOp::GainLife {
        player: PlayerRef::Controller,
        amount: 1,
    }
}

fn take_initiative() -> EffectOp {
    EffectOp::TakeInitiative {
        player: PlayerRef::Controller,
    }
}

/// Heaped Harvest: "you may search your library for a basic land card, put
/// it onto the battlefield tapped, then shuffle."
fn search_basic_land_tapped() -> EffectOp {
    EffectOp::SearchLibraryToBattlefieldTapped {
        player: PlayerRef::Controller,
        filter: LibraryCardFilter::BasicLand,
    }
}

fn create_plant() -> EffectOp {
    EffectOp::CreateToken {
        token_def: token_id(PLANT_TOKEN),
        controller: PlayerRef::Controller,
    }
}

/// Lonely Arroyo: "it deals 1 damage to target opponent."
fn damage_target_opponent() -> EffectOp {
    EffectOp::DealDamage {
        target: TargetRef::Target(0),
        amount: 1,
    }
}

/// Mirran Banesplitter: "attach it to target creature you control."
fn attach_to_target() -> EffectOp {
    EffectOp::AttachSourceToTarget {
        object: ObjectRef::Target(0),
    }
}

/// Mortuary Mire, Witch's Cottage: "you may put target creature card from
/// your graveyard on top of your library."
fn may_put_on_library_top() -> EffectOp {
    EffectOp::Choice {
        controller: PlayerRef::Controller,
        options: vec![
            e8b(E8bOpV1::PutTargetOnLibraryTopV1),
            EffectOp::Sequence(Vec::new()),
        ],
    }
}

/// Oroku Saki: "you draw a card and lose 1 life."
fn draw_and_lose_one() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 1,
        },
        EffectOp::LoseLife {
            player: PlayerRef::Controller,
            amount: 1,
        },
    ])
}

/// Sea Gate Oracle: "look at the top two cards of your library. Put one of
/// them into your hand and the other on the bottom of your library."
fn sea_gate_look() -> EffectOp {
    EffectOp::LookTopPickToHandBottomRest {
        player: PlayerRef::Controller,
        count: LibraryLookCount::Fixed(2),
        pick: 1,
        choose_rest_order: false,
    }
}

const ACCURSED_MARAUDER_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, each_player_sacrifices)];
const KAROO_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, return_controlled_land)];
const MONARCH_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(TriggerCondition::Etb, become_monarch)];
/// God-Pharaoh's Faithful: "Whenever you cast a blue, black, or red spell,
/// you gain 1 life."
const GOD_PHARAOHS_FAITHFUL_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(
    TriggerCondition::CastSpellWithAnyColor(
        crate::card_def::mana_color_mask(ManaColor::U)
            | crate::card_def::mana_color_mask(ManaColor::B)
            | crate::card_def::mana_color_mask(ManaColor::R),
    ),
    gain_one,
)];
const INITIATIVE_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, take_initiative)];
/// Heaped Harvest's "when you sacrifice it" half is queued by a hook.
const HEAPED_HARVEST_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, search_basic_land_tapped)];
const KHALNI_GARDEN_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, create_plant)];
const LONELY_ARROYO_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, damage_target_opponent)];
const MIRRAN_BANESPLITTER_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, attach_to_target)];
const MORTUARY_MIRE_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, may_put_on_library_top)];
const WITCHS_COTTAGE_TRIGGERS: [TriggeredAbilityDef; 1] = [e8b_trigger(
    E8bTriggerV1::EntersUntappedV1,
    may_put_on_library_top,
)];
const OROKU_SAKI_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(
    TriggerCondition::DealsCombatDamageToPlayer,
    draw_and_lose_one,
)];
const SEA_GATE_ORACLE_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, sea_gate_look)];

/// Triggered abilities of this module's cards.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        ACCURSED_MARAUDER => &ACCURSED_MARAUDER_TRIGGERS,
        AZORIUS_CHANCERY | GOLGARI_ROT_FARM | ORZHOV_BASILICA => &KAROO_TRIGGERS,
        CRIMSON_FLEET_COMMODORE => &MONARCH_TRIGGERS,
        GOD_PHARAOHS_FAITHFUL => &GOD_PHARAOHS_FAITHFUL_TRIGGERS,
        GOLIATH_PALADIN => &INITIATIVE_TRIGGERS,
        HEAPED_HARVEST => &HEAPED_HARVEST_TRIGGERS,
        KHALNI_GARDEN => &KHALNI_GARDEN_TRIGGERS,
        LONELY_ARROYO => &LONELY_ARROYO_TRIGGERS,
        MIRRAN_BANESPLITTER => &MIRRAN_BANESPLITTER_TRIGGERS,
        MORTUARY_MIRE => &MORTUARY_MIRE_TRIGGERS,
        WITCHS_COTTAGE => &WITCHS_COTTAGE_TRIGGERS,
        OROKU_SAKI => &OROKU_SAKI_TRIGGERS,
        SEA_GATE_ORACLE => &SEA_GATE_ORACLE_TRIGGERS,
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
        LONELY_ARROYO if *effect == damage_target_opponent() => TargetSpec::TargetOpponent,
        MIRRAN_BANESPLITTER if *effect == attach_to_target() => TargetSpec::ControlledCreature,
        MORTUARY_MIRE | WITCHS_COTTAGE if *effect == may_put_on_library_top() => {
            TargetSpec::CreatureCardInOwnGraveyard
        }
        _ => TargetSpec::None,
    })
}

/// Queued and delayed triggers owned by this module's cards. The Ring's
/// triggers belong to whatever creature is the Ring-bearer (or, for the
/// end-of-combat sacrifice, the blocker).
pub(crate) fn delayed_effect_matches(name: &str, effect: &EffectOp) -> bool {
    if granted_trigger_effect(effect) {
        return true;
    }
    if name == HEAPED_HARVEST && *effect == search_basic_land_tapped() {
        return true;
    }
    let EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::E8bV1(op))) = effect else {
        return false;
    };
    match op {
        E8bOpV1::FirstDayCounterV1 { .. } => name == FIRST_DAY_OF_CLASS,
        E8bOpV1::GlimpseEndV1 { .. } => name == GLIMPSE_THE_IMPOSSIBLE,
        _ => false,
    }
}

/// The Ring's triggers, which belong to whatever creature has them.
pub(crate) fn granted_trigger_effect(effect: &EffectOp) -> bool {
    matches!(
        effect,
        EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::E8bV1(
            E8bOpV1::RingLootV1
                | E8bOpV1::RingBlockedV1 { .. }
                | E8bOpV1::RingSacrificeV1 { .. }
                | E8bOpV1::RingDrainV1
        )))
    )
}

/// Whether `condition` triggers on its source's arrival on the battlefield.
pub(crate) fn arrival_condition(condition: E8bTriggerV1) -> bool {
    condition == E8bTriggerV1::EntersUntappedV1
}

pub(crate) fn trigger_matches(
    condition: E8bTriggerV1,
    events: &[CommittedEvent],
    index: usize,
    source: ObjectId,
    state: &GameState,
) -> bool {
    match condition {
        E8bTriggerV1::EntersUntappedV1 => {
            matches!(
                events[index],
                CommittedEvent::ZoneChange {
                    object,
                    to: Zone::Battlefield,
                    ..
                } if object == source
            ) && active_named(state, source, WITCHS_COTTAGE)
                && !state.objects.get(source).tapped
        }
    }
}

// ---- Effect leaves ------------------------------------------------------------------

pub(crate) fn execute(op: &E8bOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        E8bOpV1::DamageEachCreatureAndPlayerV1 { amount } => {
            damage_each_creature_and_player(state, ctx.source, *amount);
        }
        E8bOpV1::DefileV1 => {
            let Some(target) = bound_target_in(ctx, state, 0, Zone::Battlefield) else {
                return;
            };
            let swamps = controlled(state, ctx.controller)
                .filter(|&object| {
                    crate::engine::has_effective_subtype(state, object, Subtype::Swamp)
                })
                .count();
            let amount = i32::try_from(swamps).unwrap_or(i32::MAX);
            if amount > 0 && is_creature(state, target) {
                let binding = bind(state, target);
                crate::effect::install_temporary_boost(
                    state,
                    binding,
                    -amount,
                    -amount,
                    Keywords::NONE,
                );
            }
        }
        E8bOpV1::SkredV1 => {
            let Some(target) = bound_target_in(ctx, state, 0, Zone::Battlefield) else {
                return;
            };
            let snow = controlled(state, ctx.controller)
                .filter(|&object| {
                    crate::engine::effective_supertypes(state, object).contains(&Supertype::Snow)
                })
                .count();
            let amount = i32::try_from(snow).unwrap_or(i32::MAX);
            if amount > 0 {
                event::propose_and_commit(
                    state,
                    ProposedEvent::damage(ctx.source, Target::Object(target), amount),
                );
            }
        }
        E8bOpV1::SplicedRitualsV1 => {
            let count = usize::from(ctx.x_value).saturating_mul(3);
            if count > 0 {
                event::propose_and_commit(
                    state,
                    ProposedEvent::mana_add(ctx.controller, vec![ManaColor::R; count]),
                );
            }
        }
        E8bOpV1::FirstDayOfClassV1 => {
            let record = FirstDayV1 {
                source: AbilitySourceContractV4::capture(state, ctx.source),
                controller: ctx.controller,
                turn: TurnV1::now(state),
            };
            e8b_state(state).first_days.push(record);
        }
        E8bOpV1::FirstDayCounterV1 { creature } => {
            if !link_live_in(state, *creature, Zone::Battlefield)
                || !is_creature(state, creature.object)
            {
                return;
            }
            let _ = event::add_plus_one_counters(state, creature.object, ctx.controller, 1);
            if link_live_in(state, *creature, Zone::Battlefield) {
                let binding = bind(state, creature.object);
                crate::effect::install_temporary_boost(state, binding, 0, 0, Keywords::HASTE);
            }
        }
        E8bOpV1::GlimpseV1 => glimpse(ctx, state),
        E8bOpV1::GlimpseEndV1 { cards } => {
            let remaining = cards
                .iter()
                .copied()
                .filter(|&card| link_live_in(state, card, Zone::Exile))
                .collect::<Vec<_>>();
            if remaining.is_empty() {
                return;
            }
            event::propose_and_commit_batch(
                state,
                remaining
                    .iter()
                    .map(|card| ProposedEvent::zone_change(card.object, Zone::Graveyard))
                    .collect(),
            );
            let moved = remaining
                .iter()
                .filter(|card| {
                    state.objects.try_get(card.object).is_some_and(|live| {
                        live.zone == Zone::Graveyard
                            && live.zone_change_count > card.zone_change_count
                    })
                })
                .count();
            for _ in 0..moved {
                crate::effect::execute(
                    &EffectOp::CreateToken {
                        token_def: token_id(ELDRAZI_SPAWN_TOKEN),
                        controller: PlayerRef::Controller,
                    },
                    ctx,
                    state,
                );
            }
        }
        E8bOpV1::TeamworkTreasureV1 => {
            let teamwork = state.objects.try_get(ctx.source).is_some_and(|live| {
                live.zone == Zone::Stack
                    && live.spell_copy_origin.is_none()
                    && live
                        .v4
                        .spell_cast_origin
                        .and_then(|origin| origin.finalized_method)
                        == Some(CastMethodV4::Alternative)
            });
            if teamwork {
                crate::effect::execute(
                    &EffectOp::CreateToken {
                        token_def: token_id(TREASURE_TOKEN),
                        controller: PlayerRef::Controller,
                    },
                    ctx,
                    state,
                );
            }
        }
        E8bOpV1::RancidEarthV1 => {
            let threshold = state.players[ctx.controller.index()].graveyard.len() >= 7;
            if bound_target_in(ctx, state, 0, Zone::Battlefield).is_some() {
                crate::effect::execute(
                    &EffectOp::DestroyObject {
                        object: ObjectRef::Target(0),
                    },
                    ctx,
                    state,
                );
            }
            if threshold {
                damage_each_creature_and_player(state, ctx.source, 1);
            }
        }
        E8bOpV1::ExileTargetV1 => {
            if let Some(target) = bound_target_in(ctx, state, 0, Zone::Battlefield) {
                event::propose_and_commit(state, ProposedEvent::zone_change(target, Zone::Exile));
            }
        }
        E8bOpV1::TwinnedVisionV1 => {
            let from_hand = state.objects.try_get(ctx.source).is_some_and(|live| {
                live.spell_copy_origin.is_none()
                    && live
                        .v4
                        .spell_cast_origin
                        .is_some_and(|origin| origin.origin_zone == Zone::Hand)
            });
            crate::effect::execute(
                &EffectOp::DrawCards {
                    player: PlayerRef::Controller,
                    count: if from_hand { 1 } else { 2 },
                },
                ctx,
                state,
            );
        }
        E8bOpV1::PutTargetOnLibraryTopV1 => {
            let Some(card) = bound_target_in(ctx, state, 0, Zone::Graveyard) else {
                return;
            };
            let live = state.objects.get(card);
            if live.owner != ctx.controller
                || !definition(state, card).is_some_and(|def| def.has_type(CardType::Creature))
            {
                return;
            }
            event::propose_and_commit(
                state,
                ProposedEvent::public_library_insert(card, event::LibraryPlacement::Top),
            );
        }
        E8bOpV1::RingLootV1 => crate::effect::execute(
            &EffectOp::Sequence(vec![
                EffectOp::DrawCards {
                    player: PlayerRef::Controller,
                    count: 1,
                },
                EffectOp::DiscardCards {
                    player: PlayerRef::Controller,
                    count: 1,
                },
            ]),
            ctx,
            state,
        ),
        E8bOpV1::RingBlockedV1 { blocker } => {
            let record = RingSacrificeV1 {
                controller: ctx.controller,
                blocker: *blocker,
                source: ctx
                    .ability_source_contract
                    .unwrap_or_else(|| AbilitySourceContractV4::capture(state, ctx.source)),
            };
            e8b_state(state).ring_sacrifices.push(record);
        }
        E8bOpV1::RingSacrificeV1 { blocker } => {
            if link_live_in(state, *blocker, Zone::Battlefield) {
                event::log_sacrifice(state, blocker.object);
                event::propose_and_commit(
                    state,
                    ProposedEvent::zone_change(blocker.object, Zone::Graveyard),
                );
            }
        }
        E8bOpV1::RingDrainV1 => {
            event::propose_and_commit(
                state,
                ProposedEvent::life_loss(ctx.controller.opponent(), 3),
            );
        }
        // Choices run in `drive`; the plain executor never reaches them.
        E8bOpV1::GloriousGaleV1 => halt(state, ctx.source),
    }
}

/// Glimpse the Impossible's exile, and the record its end-step trigger
/// reads.
fn glimpse(ctx: &ExecCtx, state: &mut GameState) {
    let top = state.players[ctx.controller.index()]
        .library
        .iter()
        .take(3)
        .copied()
        .collect::<Vec<_>>();
    crate::effect::execute(
        &EffectOp::ImpulseDraw {
            count: 3,
            duration: ImpulseDuration::EndOfTurn,
        },
        ctx,
        state,
    );
    if state.engine.halted.is_some() {
        return;
    }
    let cards = top
        .into_iter()
        .filter(|&card| state.objects.get(card).zone == Zone::Exile)
        .map(|card| live_link(state, card))
        .collect::<Vec<_>>();
    let record = GlimpseV1 {
        source: AbilitySourceContractV4::capture(state, ctx.source),
        controller: ctx.controller,
        cards,
    };
    e8b_state(state).glimpses.push(record);
}

/// Glorious Gale's counter, as the printed counterspells resolve it.
fn counter_target_spell() -> EffectOp {
    EffectOp::Conditional {
        cond: EffectCond::TargetSpellCanBeCountered(0),
        then: Box::new(EffectOp::MoveObject {
            object: ObjectRef::Target(0),
            to_zone: Zone::Graveyard,
        }),
        else_: Box::new(EffectOp::Sequence(Vec::new())),
    }
}

/// Runs a leaf inside the resumable interpreter. `Ok(true)` when it staged a
/// choice and the continuation must suspend.
pub(crate) fn drive(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    op: &E8bOpV1,
    path: Vec<u16>,
) -> Result<bool, String> {
    match op {
        E8bOpV1::GloriousGaleV1 => {
            let legendary = bound_target_in(&continuation.ctx, state, 0, Zone::Stack)
                .and_then(|spell| definition(state, spell))
                .is_some_and(|def| def.supertypes.contains(&Supertype::Legendary));
            crate::effect::execute(&counter_target_spell(), &continuation.ctx, state);
            if !legendary || state.engine.halted.is_some() {
                return Ok(false);
            }
            Ok(ring_tempts(state, continuation, path))
        }
        other => {
            execute(other, &continuation.ctx, state);
            Ok(false)
        }
    }
}

/// "The Ring tempts you" (701.54): the Ring gains its next ability, and the
/// controller chooses a creature they control as their Ring-bearer. `true`
/// when that choice was staged.
pub(crate) fn ring_tempts(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    path: Vec<u16>,
) -> bool {
    let player = continuation.ctx.controller;
    {
        let level = &mut e8b_state(state).ring_levels[player.index()];
        *level = level.saturating_add(1).min(RING_DRAIN);
    }
    let creatures = controlled(state, player)
        .filter(|&object| is_creature(state, object))
        .map(|object| bind(state, object))
        .collect::<Vec<_>>();
    if creatures.is_empty() {
        return false;
    }
    let remaining = continuation.frames.clone();
    crate::eternal_cards_v1::e3::offer(
        continuation,
        E3ChoiceV1 {
            kind: E3ChoiceKindV1::E8bV1(E8bChoiceV1::RingBearerV1),
            player,
            candidates: creatures
                .iter()
                .map(|binding| Target::Object(binding.object))
                .collect(),
            bindings: creatures,
            min: 1,
            max: 1,
            ordered: false,
            path,
            remaining,
        },
    );
    true
}

/// Applies an answered choice of this module.
pub(crate) fn finish_selection(
    state: &mut GameState,
    _continuation: &mut EffectContinuation,
    choice: &E3ChoiceV1,
    kind: &E8bChoiceV1,
    bindings: Vec<EffectObjectBinding>,
) -> Result<bool, String> {
    match kind {
        E8bChoiceV1::RingBearerV1 => {
            let [bearer] = bindings.as_slice() else {
                return Err("the Ring tempts its player to choose exactly one creature".into());
            };
            let link = ObjectLinkV4 {
                object: bearer.object,
                zone_change_count: bearer.expected_zone_change_count,
            };
            if !link_live_in(state, link, Zone::Battlefield)
                || state.objects.get(bearer.object).controller != choice.player
            {
                return Err("the chosen Ring-bearer left its controller's battlefield".into());
            }
            let bearers = &mut e8b_state(state).ring_bearers;
            // Each creature is only one player's Ring-bearer.
            for slot in bearers.iter_mut() {
                if *slot == Some(link) {
                    *slot = None;
                }
            }
            bearers[choice.player.index()] = Some(link);
            Ok(false)
        }
    }
}

// ---- The Ring ---------------------------------------------------------------------

/// The number of the Ring's abilities `player` has.
pub fn ring_level(state: &GameState, player: PlayerId) -> u8 {
    read_state(state).map_or(0, |e8b| e8b.ring_levels[player.index()])
}

/// `player`'s Ring-bearer: a creature they control, still the incarnation
/// they chose.
pub fn ring_bearer(state: &GameState, player: PlayerId) -> Option<ObjectId> {
    let link = read_state(state)?.ring_bearers[player.index()]?;
    (link_live_in(state, link, Zone::Battlefield)
        && state.objects.get(link.object).controller == player)
        .then_some(link.object)
}

/// The player whose Ring-bearer `object` is, with their Ring's level.
fn bearer_owner(state: &GameState, object: ObjectId) -> Option<(PlayerId, u8)> {
    let controller = state.objects.try_get(object)?.controller;
    let level = ring_level(state, controller);
    (ring_bearer(state, controller) == Some(object)).then_some((controller, level))
}

/// "Your Ring-bearer is legendary": its supertypes with Legendary added.
pub(crate) fn ring_bearer_supertypes(
    state: &GameState,
    object: ObjectId,
) -> Option<&'static [Supertype]> {
    let (_, level) = bearer_owner(state, object)?;
    if level < RING_LEGENDARY {
        return None;
    }
    let printed = definition(state, object)?.supertypes;
    Some(if printed.contains(&Supertype::Legendary) {
        printed
    } else if printed.contains(&Supertype::Snow) {
        &[Supertype::Legendary, Supertype::Snow]
    } else {
        &[Supertype::Legendary]
    })
}

/// "Your Ring-bearer can't be blocked by creatures with greater power."
pub(crate) fn blocker_allowed(state: &GameState, attacker: ObjectId, blocker: ObjectId) -> bool {
    bearer_owner(state, attacker).is_none_or(|(_, level)| {
        level < RING_LEGENDARY
            || crate::engine::effective_power(state, blocker)
                <= crate::engine::effective_power(state, attacker)
    })
}

/// Whether a Ring-bearer's attack is observed, so declarations are logged.
pub(crate) fn observes_attacks(state: &GameState) -> bool {
    [PlayerId::P0, PlayerId::P1].into_iter().any(|player| {
        ring_level(state, player) >= RING_LOOT && ring_bearer(state, player).is_some()
    })
}

/// The Ring's third ability: "Whenever your Ring-bearer becomes blocked by a
/// creature", one trigger for each creature blocking it.
pub(crate) fn note_blockers_declared(state: &mut GameState) {
    let blocks = state.engine.combat.blocked_by.clone();
    for (attacker, blockers) in blocks {
        let Some((player, level)) = bearer_owner(state, attacker) else {
            continue;
        };
        if level < RING_SACRIFICE {
            continue;
        }
        let source = AbilitySourceContractV4::capture(state, attacker);
        for blocker in blockers {
            let link = live_link(state, blocker);
            push_trigger(
                state,
                source,
                player,
                e8b(E8bOpV1::RingBlockedV1 { blocker: link }),
            );
        }
    }
}

/// "At end of combat": the sacrifices the Ring's third ability set up.
/// Whether any trigger was queued.
pub(crate) fn begin_end_of_combat(state: &mut GameState) -> bool {
    let records = state
        .eternal_v1
        .as_mut()
        .map(|eternal| std::mem::take(&mut eternal.e8b.ring_sacrifices))
        .unwrap_or_default();
    let mut queued = false;
    for record in records {
        if !link_live_in(state, record.blocker, Zone::Battlefield) {
            continue;
        }
        // The delayed trigger belongs to the Ring-bearer, wherever it is now.
        let mut source = AbilitySourceContractV4::capture(state, record.source.source);
        source.controller = record.controller;
        push_trigger(
            state,
            source,
            record.controller,
            e8b(E8bOpV1::RingSacrificeV1 {
                blocker: record.blocker,
            }),
        );
        queued = true;
    }
    queued
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
    reveal_splices(state, events);
    first_day_triggers(state, events);
    glimpse_triggers(state, events);
    sacrifice_triggers(state, events);
    ring_triggers(state, events);
    prune(state);
    let Some(eternal) = state.eternal_v1.as_mut() else {
        return Vec::new();
    };
    if eternal.e8b.queued_triggers.is_empty() {
        return Vec::new();
    }
    std::mem::take(&mut eternal.e8b.queued_triggers)
        .into_iter()
        .map(|queued| {
            crate::eternal_cards_v1::delayed_trigger(
                queued.source,
                queued.controller,
                queued.effect,
            )
        })
        .collect()
}

/// Splice (702.47a): "reveal this card from your hand". The opponent sees
/// as many splice cards from the caster's hand as the cast spliced.
fn reveal_splices(state: &mut GameState, events: &[CommittedEvent]) {
    for event in events {
        let CommittedEvent::SpellCast { spell, controller } = *event else {
            continue;
        };
        let Some(def) = definition(state, spell) else {
            continue;
        };
        if splice_cost(def).is_none() || crate::eternal_cards_v1::replicate_cost(def.name).is_some()
        {
            continue;
        }
        let spliced = state
            .stack
            .iter()
            .find(|item| item.kind == StackItemKind::Spell && item.source == spell && !item.is_copy)
            .map_or(0, |item| usize::from(item.v4.x_value));
        let cards = splice_cards_in_hand(state, controller, spell)
            .into_iter()
            .take(spliced)
            .collect::<Vec<_>>();
        for card in cards {
            for observer in [PlayerId::P0, PlayerId::P1] {
                let _ = state.reveal_hand_card(observer, controller, card);
            }
        }
    }
}

/// First Day of Class: each creature entering under a recorded player's
/// control this turn.
fn first_day_triggers(state: &mut GameState, events: &[CommittedEvent]) {
    let Some(records) = read_state(state).map(|e8b| e8b.first_days.clone()) else {
        return;
    };
    if records.is_empty() {
        return;
    }
    for event in events {
        // A permanent card's entry or a token's creation.
        let Some(object) = crate::trigger::battlefield_entry_object(event) else {
            continue;
        };
        let Some(live) = state.objects.try_get(object) else {
            continue;
        };
        if live.zone != Zone::Battlefield || !is_creature(state, object) {
            continue;
        }
        let controller = live.controller;
        let creature = live_link(state, object);
        for record in &records {
            if record.controller != controller || !record.turn.current(state) {
                continue;
            }
            let Some(source) = spell_delayed_source(state, record.source, record.controller) else {
                continue;
            };
            queue_trigger(
                state,
                source,
                record.controller,
                e8b(E8bOpV1::FirstDayCounterV1 { creature }),
            );
        }
    }
}

/// Glimpse the Impossible: "At the beginning of the next end step".
fn glimpse_triggers(state: &mut GameState, events: &[CommittedEvent]) {
    if !events
        .iter()
        .any(|event| matches!(event, CommittedEvent::BeginningEndStep { .. }))
    {
        return;
    }
    let records = state
        .eternal_v1
        .as_mut()
        .map(|eternal| std::mem::take(&mut eternal.e8b.glimpses))
        .unwrap_or_default();
    for record in records {
        let Some(source) = spell_delayed_source(state, record.source, record.controller) else {
            continue;
        };
        queue_trigger(
            state,
            source,
            record.controller,
            e8b(E8bOpV1::GlimpseEndV1 {
                cards: record.cards,
            }),
        );
    }
}

/// Heaped Harvest: "when you sacrifice it".
fn sacrifice_triggers(state: &mut GameState, events: &[CommittedEvent]) {
    for event in events {
        let CommittedEvent::Sacrificed {
            object,
            controller_before,
            ..
        } = *event
        else {
            continue;
        };
        if definition_name(state, object) != Some(HEAPED_HARVEST)
            || state
                .objects
                .try_get(object)
                .is_some_and(|live| live.zone == Zone::Battlefield)
        {
            continue;
        }
        let mut source = AbilitySourceContractV4::capture(state, object);
        source.controller = controller_before;
        queue_trigger(state, source, controller_before, search_basic_land_tapped());
    }
}

/// The Ring's attack and combat damage triggers.
fn ring_triggers(state: &mut GameState, events: &[CommittedEvent]) {
    if read_state(state).is_none_or(|e8b| no_bearer(&e8b.ring_bearers)) {
        return;
    }
    for event in events {
        match *event {
            CommittedEvent::DeclaredAttacker {
                source,
                source_zone_change_count,
                ..
            } => {
                let Some((player, level)) = bearer_owner(state, source) else {
                    continue;
                };
                if level < RING_LOOT
                    || state.objects.get(source).zone_change_count != source_zone_change_count
                {
                    continue;
                }
                let contract = AbilitySourceContractV4::capture(state, source);
                queue_trigger(state, contract, player, e8b(E8bOpV1::RingLootV1));
            }
            CommittedEvent::CombatDamageToPlayer {
                source,
                source_zone_change_count,
                amount,
                ..
            } => {
                if amount <= 0 {
                    continue;
                }
                let bearer = [PlayerId::P0, PlayerId::P1].into_iter().find(|&player| {
                    read_state(state).is_some_and(|e8b| {
                        e8b.ring_bearers[player.index()]
                            == Some(ObjectLinkV4 {
                                object: source,
                                zone_change_count: source_zone_change_count,
                            })
                    }) && ring_level(state, player) >= RING_DRAIN
                });
                let Some(player) = bearer else {
                    continue;
                };
                let contract = AbilitySourceContractV4::capture(state, source);
                queue_trigger(state, contract, player, e8b(E8bOpV1::RingDrainV1));
            }
            _ => {}
        }
    }
}

/// Drops records whose objects or turns are gone.
fn prune(state: &mut GameState) {
    let Some(e8b) = read_state(state) else {
        return;
    };
    let depletion = e8b
        .depletion
        .iter()
        .copied()
        .filter(|(land, _)| link_live_in(state, *land, Zone::Battlefield))
        .collect::<Vec<_>>();
    let first_days = e8b
        .first_days
        .iter()
        .copied()
        .filter(|record| record.turn.current(state))
        .collect::<Vec<_>>();
    // A Ring-bearer stops being one when it leaves or changes controller.
    let bearers = [PlayerId::P0, PlayerId::P1].map(|player| {
        e8b.ring_bearers[player.index()].filter(|link| {
            link_live_in(state, *link, Zone::Battlefield)
                && state.objects.get(link.object).controller == player
        })
    });
    if depletion.len() != e8b.depletion.len()
        || first_days.len() != e8b.first_days.len()
        || bearers != e8b.ring_bearers
    {
        let e8b = e8b_state(state);
        e8b.depletion = depletion;
        e8b.first_days = first_days;
        e8b.ring_bearers = bearers;
    }
}

// ---- Hooks: engine ------------------------------------------------------------------

/// "Enters with" counters: a depletion land's two depletion counters.
pub(crate) fn apply_entry_counters(state: &mut GameState, object: ObjectId) {
    if !matches!(
        definition_name(state, object),
        Some(HICKORY_WOODLOT | SANDSTONE_NEEDLE)
    ) {
        return;
    }
    let link = live_link(state, object);
    let depletion = &mut e8b_state(state).depletion;
    depletion.retain(|(land, _)| land.object != object);
    depletion.push((link, DEPLETION_COUNTERS));
}

/// The depletion counters on `land`'s current incarnation.
pub fn depletion_counters(state: &GameState, land: ObjectId) -> u8 {
    let Some(live) = state.objects.try_get(land) else {
        return 0;
    };
    read_state(state).map_or(0, |e8b| {
        e8b.depletion
            .iter()
            .find(|(link, _)| {
                link.object == land && link.zone_change_count == live.zone_change_count
            })
            .map_or(0, |(_, count)| *count)
    })
}

/// A depletion land's mana ability costs "Remove a depletion counter from
/// this land": without one it can't be activated.
pub(crate) fn mana_ability_blocked(state: &GameState, source: ObjectId) -> bool {
    matches!(
        definition_name(state, source),
        Some(HICKORY_WOODLOT | SANDSTONE_NEEDLE)
    ) && depletion_counters(state, source) == 0
}

/// After a depletion land was tapped for mana: remove a depletion counter,
/// then "If there are no depletion counters on this land, sacrifice it."
pub(crate) fn after_tapped_for_mana(state: &mut GameState, land: ObjectId) {
    if !matches!(
        definition_name(state, land),
        Some(HICKORY_WOODLOT | SANDSTONE_NEEDLE)
    ) || state.objects.get(land).zone != Zone::Battlefield
    {
        return;
    }
    let link = live_link(state, land);
    let mut remaining = 0;
    if let Some(entry) = e8b_state(state)
        .depletion
        .iter_mut()
        .find(|(counted, _)| *counted == link)
    {
        entry.1 = entry.1.saturating_sub(1);
        remaining = entry.1;
    }
    if remaining == 0 {
        event::log_sacrifice(state, land);
        event::propose_and_commit(state, ProposedEvent::zone_change(land, Zone::Graveyard));
    }
}

/// Goliath Paladin: "When this creature enters, you take the initiative."
pub(crate) fn initiative_granter(name: &str) -> bool {
    name == GOLIATH_PALADIN
}

/// Desert: "Activate only during the end of combat step."
pub(crate) fn activation_allowed(state: &GameState, name: &str, ability_index: usize) -> bool {
    name != DESERT || ability_index != 0 || state.step == Step::EndCombat
}

/// Goblin Anarchomancer ("Each spell you cast that's red or green costs {1}
/// less to cast."), Sunscape Familiar ("Green spells and blue spells") and
/// Thornscape Familiar ("Red spells and white spells"): the generic mana
/// `player`'s spell of `definition` costs less.
pub(crate) fn spell_cost_reduction(
    state: &GameState,
    definition: &CardDef,
    player: PlayerId,
) -> u32 {
    let colors = crate::card_def::mana_colors_mask(definition.colors);
    if colors == 0 {
        return 0;
    }
    let mask = |first: ManaColor, second: ManaColor| {
        crate::card_def::mana_color_mask(first) | crate::card_def::mana_color_mask(second)
    };
    let reducers = [
        (GOBLIN_ANARCHOMANCER, mask(ManaColor::R, ManaColor::G)),
        (SUNSCAPE_FAMILIAR, mask(ManaColor::G, ManaColor::U)),
        (THORNSCAPE_FAMILIAR, mask(ManaColor::R, ManaColor::W)),
    ];
    let count = controlled(state, player)
        .filter_map(|object| {
            let name = definition_name(state, object)?;
            reducers
                .iter()
                .find(|(reducer, _)| *reducer == name)
                .filter(|(reducer, mask)| {
                    colors & mask != 0 && active_named(state, object, reducer)
                })
        })
        .count();
    u32::try_from(count).unwrap_or(u32::MAX)
}

/// Splice onto Arcane's cost for an Arcane spell: the only splice card in
/// the catalog is Desperate Ritual ({1}{R}), so the spliced cards are the
/// Desperate Rituals in the caster's hand and their count is announced like
/// X. `None` for every other spell and outside eternal builds.
pub(crate) fn splice_cost(definition: &CardDef) -> Option<Cost> {
    (cfg!(feature = "eternal-fixtures") && definition.subtypes.contains(&Subtype::Arcane))
        .then_some(SPLICE_COST)
}

/// The splice cards `player` holds other than `spell`.
fn splice_cards_in_hand(state: &GameState, player: PlayerId, spell: ObjectId) -> Vec<ObjectId> {
    state.players[player.index()]
        .hand
        .iter()
        .copied()
        .filter(|&card| card != spell && definition_name(state, card) == Some(DESPERATE_RITUAL))
        .collect()
}

/// The most cards a cast of `definition` may splice: the splice cards in
/// `player`'s hand. `None` when the spell can't be spliced onto.
pub(crate) fn splice_cap(
    definition: &CardDef,
    spell: ObjectId,
    player: PlayerId,
    state: &GameState,
) -> Option<u8> {
    if splice_cost(definition).is_none()
        || crate::eternal_cards_v1::replicate_cost(definition.name).is_some()
        || definition.cost.x_count != 0
    {
        return None;
    }
    Some(u8::try_from(splice_cards_in_hand(state, player, spell).len()).unwrap_or(u8::MAX))
}

/// Twinned Vision's "Discard a card" belongs to its flashback cost: whether
/// a cast of `definition` (by flashback if `flashback`) skips it.
pub(crate) fn skips_additional_cost(definition: &CardDef, flashback: bool) -> bool {
    !flashback && definition.name == "Twinned Vision"
}

/// Seize the Storm's Elemental: "This token's power and toughness are each
/// equal to the number of instant and sorcery cards in your graveyard plus
/// the number of cards with flashback you own in exile." `None` for every
/// other object.
pub(crate) fn characteristic_defining_power(state: &GameState, id: ObjectId) -> Option<i32> {
    if definition_name(state, id)? != RED_ELEMENTAL_TOKEN {
        return None;
    }
    let controller = state.objects.get(id).controller;
    let graveyard = state.players[controller.index()]
        .graveyard
        .iter()
        .filter(|&&card| {
            definition(state, card).is_some_and(|def| {
                def.has_type(CardType::Instant) || def.has_type(CardType::Sorcery)
            })
        })
        .count();
    let exiled = state
        .exile
        .iter()
        .filter(|&&card| {
            let live = state.objects.get(card);
            live.owner == controller
                && live.zone == Zone::Exile
                && !live.v4.is_token
                && definition(state, card).is_some_and(|def| def.flashback.is_some())
        })
        .count();
    i32::try_from(graveyard + exiled).ok()
}
