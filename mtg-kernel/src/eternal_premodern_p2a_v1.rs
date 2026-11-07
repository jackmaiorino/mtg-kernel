//! Card programs for the eternal catalog's Premodern wave P2a
//! (`docs/design/eternal_formats_support_v1.md`): The Rock, Mono-Black
//! Control, BW Control, Stasis and Other Control.
//!
//! `eternal_cards_v1` forwards its effect leaves and state here, and the
//! engine calls the hooks in `statics` only in `eternal-fixtures` builds.
//! Every table is keyed by a printed name that occurs only in the eternal
//! registry, so the Pauper, FDN and Standard catalogs never reach it.
//! Behavior follows each card's current Oracle text, checked against its
//! XMage source.
//!
//! Primitives added here:
//! - morph with an X cost (Bane of the Living): turning it face up pays
//!   {B}{B}, then its controller announces X (an as-enters style choice) and
//!   pays it before the "turned face up" trigger is queued;
//! - Contamination's replacement of the mana a land produces, Stasis's and
//!   Forsaken City's skipped untaps, Root Maze's tapped entries, Future
//!   Sight's revealed and playable library top;
//! - abilities of cards in the graveyard: Genesis's and Nether Spirit's
//!   upkeep triggers (queued by a hook), Eternal Dragon's upkeep-only return,
//!   Gaea's Blessing's trigger when it is milled;
//! - upkeep payments and sacrifices: Stasis, Contamination, Forsaken City;
//! - Black Vise's chosen opponent, Pernicious Deed's X as one ability per X.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

pub mod statics;
pub(crate) mod targets;

use crate::card_def::{CardDef, CardType, DynamicValueDef, TargetSpec, CARD_DEFS};
use crate::effect::{
    EffectContinuation, EffectFrame, EffectObjectBinding, EffectOp, ExecCtx, ObjectRef, PlayerRef,
    TargetRef,
};
use crate::eternal_cards_v1::e3::{E3ChoiceKindV1, E3ChoiceV1};
use crate::eternal_cards_v1::EternalOpV1;
use crate::event::{self, CommittedEvent, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::mana::{Cost, Pip};
use crate::standard_cards_v1::StandardOpV1;
use crate::state::{AbilitySourceContractV4, GameState, ObjectLinkV4, Target, Zone};
use crate::trigger::{PendingTrigger, TriggerCondition, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};

// ---- State ------------------------------------------------------------------------

/// Per-game state for this module's cards, kept inside
/// `eternal_cards_v1::EternalStateV1`. Default (and absent from snapshots)
/// until one of these cards needs it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct P2aStateV1 {
    /// Triggers detected by hooks, waiting for the next trigger collection.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub queued_triggers: Vec<QueuedTriggerV1>,
    /// Black Vise incarnations and the opponent each chose as it entered.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub vises: Vec<(ObjectLinkV4, PlayerId)>,
    /// A Bane of the Living turned face up whose controller still owes its
    /// X.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bane_x: Option<ObjectLinkV4>,
}

impl P2aStateV1 {
    pub(crate) fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// A triggered ability queued by a hook.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueuedTriggerV1 {
    pub source: AbilitySourceContractV4,
    pub controller: PlayerId,
    pub effect: EffectOp,
    /// The target the ability's controller announces, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_spec: Option<TargetSpec>,
}

pub(crate) fn p2a_state(state: &mut GameState) -> &mut P2aStateV1 {
    &mut state.eternal_v1.get_or_insert_with(Default::default).p2a
}

pub fn read_state(state: &GameState) -> Option<&P2aStateV1> {
    state.eternal_v1.as_ref().map(|eternal| &eternal.p2a)
}

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::P2aV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum P2aOpV1 {
    /// Pernicious Deed: destroy each artifact, creature and enchantment with
    /// mana value `x` or less.
    DeedV1 { x: u8 },
    /// Nevinyrral's Disk: destroy all artifacts, creatures and enchantments.
    DiskV1,
    /// Eternal Dragon: return the ability's source from its owner's
    /// graveyard to their hand.
    DragonReturnV1,
    /// Vindicate: destroy target permanent 0.
    VindicateV1,
    /// Gerrard's Verdict: target player 0 discards two cards. A choice.
    VerdictV1,
    /// Mana Short: tap all lands target player 0 controls and empty their
    /// mana pool.
    ManaShortV1,
    /// Upheaval: return all permanents to their owners' hands.
    UpheavalV1,
    /// Gaea's Blessing: the target player shuffles the target cards from
    /// their graveyard into their library.
    BlessingShuffleV1,
    /// Gaea's Blessing's trigger: shuffle the controller's graveyard into
    /// their library.
    BlessingMilledV1,
    /// Black Vise: damage to `player` for the cards in their hand beyond
    /// four.
    ViseDamageV1 { player: PlayerId },
    /// Bane of the Living: all creatures get -`x`/-`x` until end of turn.
    BaneShrinkV1 { x: u16 },
    /// Sacrifice the resolving ability's source.
    SacrificeSourceV1,
    /// Stasis's upkeep: sacrifice it unless its controller pays {U}. May be
    /// a choice.
    StasisUpkeepV1,
    /// Stasis's payment of {U}.
    StasisPayV1,
    /// Forsaken City's upkeep: its controller may exile a card from hand to
    /// untap it. A choice.
    CityUpkeepV1,
    /// Genesis's upkeep trigger: its controller may pay {2}{G} to return
    /// target creature card from their graveyard to their hand. May be a
    /// choice.
    GenesisOfferV1,
    /// Genesis's payment and return.
    GenesisPayV1,
    /// Nether Spirit's upkeep trigger: its controller may return it to the
    /// battlefield if it is the only creature card in their graveyard. May
    /// be a choice.
    NetherSpiritOfferV1,
    /// Nether Spirit's return.
    NetherSpiritReturnV1,
    /// Academy Rector's dies trigger: its controller may exile it from the
    /// graveyard, and if they do search for an enchantment. May be a choice.
    RectorOfferV1,
    /// Academy Rector: exile the ability's source from the graveyard.
    RectorExileV1,
}

impl P2aOpV1 {
    /// Whether this leaf can yield a player decision or pushes frames, so
    /// its program must use the resumable interpreter.
    pub(crate) fn contains_player_choice(&self) -> bool {
        matches!(
            self,
            Self::VerdictV1
                | Self::StasisUpkeepV1
                | Self::CityUpkeepV1
                | Self::GenesisOfferV1
                | Self::NetherSpiritOfferV1
                | Self::RectorOfferV1
        )
    }
}

/// Resolution choices owned by this module, carried as
/// `E3ChoiceKindV1::P2aV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum P2aChoiceV1 {
    /// Gerrard's Verdict: the cards the target player discards.
    VerdictDiscardV1,
    /// Forsaken City: the card its controller may exile from hand.
    CityExileV1,
}

impl P2aChoiceV1 {
    /// Whether the candidates are hidden from the other player.
    pub(crate) fn chooser_private(&self) -> bool {
        matches!(self, Self::VerdictDiscardV1 | Self::CityExileV1)
    }

    /// Whether the chooser sees library cards it has not looked at before.
    pub(crate) fn library_search(&self) -> bool {
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

pub(crate) const ACADEMY_RECTOR: &str = "Academy Rector";
pub(crate) const BANE_OF_THE_LIVING: &str = "Bane of the Living";
pub(crate) const BLACK_VISE: &str = "Black Vise";
pub(crate) const CONTAMINATION: &str = "Contamination";
pub(crate) const ETERNAL_DRAGON: &str = "Eternal Dragon";
pub(crate) const FORSAKEN_CITY: &str = "Forsaken City";
pub(crate) const FUTURE_SIGHT: &str = "Future Sight";
pub(crate) const GAEAS_BLESSING: &str = "Gaea's Blessing";
pub(crate) const GENESIS: &str = "Genesis";
pub(crate) const KROSAN_TUSKER: &str = "Krosan Tusker";
pub(crate) const NETHER_SPIRIT: &str = "Nether Spirit";
pub(crate) const ROOT_MAZE: &str = "Root Maze";
pub(crate) const STASIS: &str = "Stasis";
pub(crate) const TAINTED_FIELD: &str = "Tainted Field";
pub(crate) const TAINTED_WOOD: &str = "Tainted Wood";
pub(crate) const VETERAN_EXPLORER: &str = "Veteran Explorer";
pub(crate) const RAVENOUS_RATS: &str = "Ravenous Rats";
pub(crate) const WALL_OF_BLOSSOMS: &str = "Wall of Blossoms";

/// Stasis: "unless you pay {U}".
const BLUE: Cost = Cost {
    pips: &[Pip::Colored(crate::mana::ManaColor::U)],
    generic: 0,
    x_count: 0,
};
/// Genesis: "pay {2}{G}".
const TWO_GREEN: Cost = Cost {
    pips: &[Pip::Colored(crate::mana::ManaColor::G)],
    generic: 2,
    x_count: 0,
};

fn p2a(op: P2aOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::P2aV1(op)))
}

fn halt(state: &mut GameState, source: ObjectId) {
    state.engine.halted = Some((
        crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
        source,
    ));
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

fn binding_live(state: &GameState, binding: EffectObjectBinding) -> bool {
    state.objects.try_get(binding.object).is_some_and(|live| {
        live.zone == binding.expected_zone
            && live.zone_change_count == binding.expected_zone_change_count
    })
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
pub(crate) fn battlefield(state: &GameState) -> impl Iterator<Item = ObjectId> + '_ {
    [PlayerId::P0, PlayerId::P1]
        .into_iter()
        .flat_map(move |player| state.players[player.index()].battlefield.iter().copied())
        .filter(move |&object| state.objects.get(object).zone == Zone::Battlefield)
}

/// The resolving ability's source, if it is still the incarnation that
/// created the ability, on the battlefield.
fn live_ability_source(ctx: &ExecCtx, state: &GameState) -> Option<ObjectId> {
    let contract = ctx.ability_source_contract?;
    let live = state.objects.try_get(contract.source)?;
    (live.zone == Zone::Battlefield && live.zone_change_count == contract.zone_change_count)
        .then_some(contract.source)
}

/// The resolving ability's source, if it is still the incarnation that
/// created the ability, in its owner's graveyard.
fn graveyard_ability_source(ctx: &ExecCtx, state: &GameState) -> Option<ObjectId> {
    let contract = ctx.ability_source_contract?;
    let live = state.objects.try_get(contract.source)?;
    // A graveyard ability is frozen with its graveyard incarnation; a dies
    // trigger, with the battlefield incarnation that just died.
    let incarnation = if contract.zone == Zone::Battlefield {
        contract.zone_change_count.checked_add(1)?
    } else {
        contract.zone_change_count
    };
    (live.zone == Zone::Graveyard
        && live.zone_change_count == incarnation
        && state.players[live.owner.index()]
            .graveyard
            .contains(&contract.source))
    .then_some(contract.source)
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

/// Target `index`, if it is a player still legal under `spec`.
fn legal_player_target(
    ctx: &ExecCtx,
    state: &GameState,
    index: usize,
    spec: TargetSpec,
) -> Option<PlayerId> {
    let Some(Target::Player(player)) = ctx.targets.get(index).copied() else {
        return None;
    };
    crate::engine::effect_target_is_legal(
        state,
        ctx.source,
        ctx.controller,
        spec,
        &ctx.targets,
        index,
    )
    .then_some(player)
}

fn queue_trigger(
    state: &mut GameState,
    source: AbilitySourceContractV4,
    controller: PlayerId,
    effect: EffectOp,
    target_spec: Option<TargetSpec>,
) {
    p2a_state(state).queued_triggers.push(QueuedTriggerV1 {
        source,
        controller,
        effect,
        target_spec,
    });
}

fn has_type(state: &GameState, object: ObjectId, card_type: CardType) -> bool {
    crate::engine::object_has_type(state, object, card_type)
}

fn shuffle(state: &mut GameState, player: PlayerId) -> Result<(), String> {
    state
        .shuffle_library(player)
        .map_err(|error| format!("{error:?}"))
}

/// `player` sacrifices `object` if they control it on the battlefield.
fn sacrifice(state: &mut GameState, object: ObjectId, player: PlayerId) -> bool {
    let live = state.objects.get(object);
    if live.zone != Zone::Battlefield || live.controller != player {
        return false;
    }
    event::log_sacrifice(state, object);
    event::propose_and_commit(state, ProposedEvent::zone_change(object, Zone::Graveyard));
    state.objects.get(object).zone != Zone::Battlefield
}

fn push_frame(continuation: &mut EffectContinuation, op: EffectOp, path: &[u16]) {
    continuation.frames.push(EffectFrame::Program {
        op,
        path: path.to_vec(),
    });
}

fn draw(count: u32) -> EffectOp {
    EffectOp::DrawCards {
        player: PlayerRef::Controller,
        count,
    }
}

fn nothing() -> EffectOp {
    EffectOp::Sequence(vec![])
}

/// "You may [op]": the controller picks nothing or the op.
fn may(op: EffectOp) -> EffectOp {
    EffectOp::Choice {
        controller: PlayerRef::Controller,
        options: vec![nothing(), op],
    }
}

/// Destroys `objects` as one simultaneous event: indestructible permanents
/// stay, and a regeneration shield replaces a destruction.
fn destroy_batch(state: &mut GameState, objects: Vec<ObjectId>) {
    let events = objects
        .into_iter()
        .filter(|&id| {
            state.objects.get(id).zone == Zone::Battlefield
                && !crate::engine::has_effective_keyword(
                    state,
                    id,
                    crate::card_def::Keywords::INDESTRUCTIBLE,
                )
        })
        .collect::<Vec<_>>()
        .into_iter()
        .filter(|&id| !crate::eternal_vintage_e7b_v1::statics::regenerate_instead(state, id))
        .map(|id| ProposedEvent::zone_change(id, Zone::Graveyard))
        .collect::<Vec<_>>();
    if !events.is_empty() {
        event::propose_and_commit_batch(state, events);
    }
}

/// Destroys `object` through the engine's destroy leaf (indestructible and
/// regeneration apply).
fn destroy(ctx: &ExecCtx, state: &mut GameState, object: ObjectId) {
    let mut ctx = ctx.clone();
    ctx.targets = vec![Target::Object(object)];
    ctx.target_contracts = Vec::new();
    crate::effect::execute(
        &EffectOp::DestroyObject {
            object: ObjectRef::Target(0),
        },
        &ctx,
        state,
    );
}

/// An until-end-of-turn power and toughness change on `object`'s current
/// incarnation.
fn pump_until_end_of_turn(state: &mut GameState, object: ObjectId, power: i32, toughness: i32) {
    let timestamp = crate::engine::next_timestamp(state);
    let object_zone_change_count = state.objects.get(object).zone_change_count;
    state.engine.until_end_of_turn.push(
        crate::engine::UntilEndOfTurnEffect::ResolvedObjectEffect {
            object_id: object,
            object_zone_change_count,
            layer: crate::engine::Layers::POWER_TOUGHNESS,
            timestamp,
            duration: crate::engine::EffectDuration::EndOfTurn,
            power,
            toughness,
            grant_haste: false,
        },
    );
}

/// Pays `cost` from `player`'s mana if they can; whether they did.
fn pay(state: &mut GameState, player: PlayerId, cost: &Cost) -> bool {
    let Some(plan) = crate::mana::can_pay(cost, 0, player, state) else {
        return false;
    };
    crate::engine::pay_plan(state, player, &plan);
    true
}

fn can_pay(state: &GameState, player: PlayerId, cost: &Cost) -> bool {
    crate::mana::can_pay(cost, 0, player, state).is_some()
}

/// "Counter target spell": the countering half of Counterspell's program.
fn counter_target_spell() -> EffectOp {
    EffectOp::Conditional {
        cond: crate::effect::EffectCond::TargetSpellCanBeCountered(0),
        then: Box::new(EffectOp::MoveObject {
            object: ObjectRef::Target(0),
            to_zone: Zone::Graveyard,
        }),
        else_: Box::new(EffectOp::Sequence(vec![])),
    }
}

// ---- Spells and abilities ------------------------------------------------------------

/// Dromar's Charm's first mode: "You gain 5 life."
pub fn dromars_charm_life() -> EffectOp {
    EffectOp::GainLife {
        player: PlayerRef::Controller,
        amount: 5,
    }
}

/// Dromar's Charm's second mode: "Counter target spell."
pub fn dromars_charm_counter() -> EffectOp {
    counter_target_spell()
}

/// Dromar's Charm's third mode: "Target creature gets -2/-2 until end of
/// turn."
pub fn dromars_charm_shrink() -> EffectOp {
    EffectOp::PumpTargetUntilEndOfTurnDynamic {
        target: TargetRef::Target(0),
        power: DynamicValueDef::Fixed(-2),
        toughness: DynamicValueDef::Fixed(-2),
    }
}

/// Eternal Dragon: "Return this card from your graveyard to your hand."
pub fn eternal_dragon_return() -> EffectOp {
    p2a(P2aOpV1::DragonReturnV1)
}

/// Gaea's Blessing: "Target player shuffles up to three target cards from
/// their graveyard into their library. Draw a card."
pub fn gaeas_blessing() -> EffectOp {
    EffectOp::Sequence(vec![p2a(P2aOpV1::BlessingShuffleV1), draw(1)])
}

/// Gerrard's Verdict: "Target player discards two cards. You gain 3 life
/// for each land card discarded this way."
pub fn gerrards_verdict() -> EffectOp {
    p2a(P2aOpV1::VerdictV1)
}

/// Innocent Blood: "Each player sacrifices a creature." The active player
/// chooses first.
pub fn innocent_blood() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::SacrificeCreature {
            player: PlayerRef::Controller,
            filter: crate::effect::CreatureSacrificeFilter::Any,
        },
        EffectOp::SacrificeCreature {
            player: PlayerRef::Opponent,
            filter: crate::effect::CreatureSacrificeFilter::Any,
        },
    ])
}

/// Mana Short: "Tap all lands target player controls and empty their mana
/// pool."
pub fn mana_short() -> EffectOp {
    p2a(P2aOpV1::ManaShortV1)
}

/// Nevinyrral's Disk: "Destroy all artifacts, creatures, and enchantments."
pub fn nevinyrrals_disk() -> EffectOp {
    p2a(P2aOpV1::DiskV1)
}

/// Upheaval: "Return all permanents to their owners' hands."
pub fn upheaval() -> EffectOp {
    p2a(P2aOpV1::UpheavalV1)
}

/// Vindicate: "Destroy target permanent."
pub fn vindicate() -> EffectOp {
    p2a(P2aOpV1::VindicateV1)
}

macro_rules! pernicious_deed {
    ($($name:ident => $x:literal),* $(,)?) => {
        $(
            /// Pernicious Deed's ability for this X.
            pub fn $name() -> EffectOp {
                p2a(P2aOpV1::DeedV1 { x: $x })
            }
        )*
    };
}

pernicious_deed! {
    pernicious_deed_0 => 0, pernicious_deed_1 => 1, pernicious_deed_2 => 2,
    pernicious_deed_3 => 3, pernicious_deed_4 => 4, pernicious_deed_5 => 5,
    pernicious_deed_6 => 6, pernicious_deed_7 => 7, pernicious_deed_8 => 8,
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

const YOUR_UPKEEP: TriggerCondition = TriggerCondition::BeginningOfUpkeep {
    controller_only: true,
};

/// Ravenous Rats: "target opponent discards a card."
fn rats_enters() -> EffectOp {
    EffectOp::DiscardCards {
        player: PlayerRef::Target(0),
        count: 1,
    }
}

/// Wall of Blossoms: "draw a card."
fn blossoms_enters() -> EffectOp {
    draw(1)
}

/// Contamination: "sacrifice Contamination unless you sacrifice a creature."
/// A controller with no creature sacrifices it outright. The choice is a
/// branch of the trigger's own program, which a sacrifice choice requires.
fn contamination_upkeep() -> EffectOp {
    EffectOp::Conditional {
        cond: crate::effect::EffectCond::PlayerControlsPermanentType {
            player: PlayerRef::Controller,
            card_type: CardType::Creature,
        },
        then: Box::new(EffectOp::Choice {
            controller: PlayerRef::Controller,
            options: vec![
                p2a(P2aOpV1::SacrificeSourceV1),
                EffectOp::SacrificeCreature {
                    player: PlayerRef::Controller,
                    filter: crate::effect::CreatureSacrificeFilter::Any,
                },
            ],
        }),
        else_: Box::new(p2a(P2aOpV1::SacrificeSourceV1)),
    }
}

/// Stasis: "sacrifice Stasis unless you pay {U}."
fn stasis_upkeep() -> EffectOp {
    p2a(P2aOpV1::StasisUpkeepV1)
}

/// Forsaken City: "you may exile a card from your hand. If you do, untap
/// Forsaken City."
fn city_upkeep() -> EffectOp {
    p2a(P2aOpV1::CityUpkeepV1)
}

/// Academy Rector: "you may exile it. If you do, search your library for an
/// enchantment card, put that card onto the battlefield, then shuffle."
fn rector_dies() -> EffectOp {
    p2a(P2aOpV1::RectorOfferV1)
}

/// The exile and the search it pays for.
fn rector_exile_and_search() -> EffectOp {
    EffectOp::Sequence(vec![
        p2a(P2aOpV1::RectorExileV1),
        EffectOp::SearchLibraryCardsToDestination {
            player: PlayerRef::Controller,
            filter: crate::effect::LibraryCardFilter::CardWithTypeV1(CardType::Enchantment),
            max_targets: 1,
            destination: crate::effect::LibrarySearchDestinationV1::Battlefield { tapped: false },
        },
    ])
}

/// One player's "may search your library for up to two basic land cards and
/// put them onto the battlefield" (the shuffle follows the search).
fn explorer_search(player: PlayerRef) -> EffectOp {
    EffectOp::SearchLibraryCardsToDestination {
        player,
        filter: crate::effect::LibraryCardFilter::BasicLand,
        max_targets: 2,
        destination: crate::effect::LibrarySearchDestinationV1::Battlefield { tapped: false },
    }
}

/// Veteran Explorer: "each player may search their library for up to two
/// basic land cards and put them onto the battlefield. Then each player who
/// searched their library this way shuffles."
fn explorer_dies() -> EffectOp {
    EffectOp::Sequence(vec![
        explorer_search(PlayerRef::Controller),
        explorer_search(PlayerRef::Opponent),
    ])
}

/// Krosan Tusker: "you may search your library for a basic land card,
/// reveal that card, put it into your hand, then shuffle."
fn tusker_cycled() -> EffectOp {
    may(EffectOp::SearchLibraryToHand {
        player: PlayerRef::Controller,
        filter: crate::effect::LibraryCardFilter::BasicLand,
    })
}

const RATS_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(TriggerCondition::Etb, rats_enters)];
const BLOSSOMS_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, blossoms_enters)];
const CONTAMINATION_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(YOUR_UPKEEP, contamination_upkeep)];
const STASIS_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(YOUR_UPKEEP, stasis_upkeep)];
const CITY_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(YOUR_UPKEEP, city_upkeep)];
const RECTOR_TRIGGERS: [TriggeredAbilityDef; 1] = [graveyard_trigger(
    TriggerCondition::LeftBattlefieldToGraveyard,
    rector_dies,
)];
const EXPLORER_TRIGGERS: [TriggeredAbilityDef; 1] = [graveyard_trigger(
    TriggerCondition::LeftBattlefieldToGraveyard,
    explorer_dies,
)];

/// The triggers of this module's cards that use the engine's own trigger
/// conditions.
fn own_triggers(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        RAVENOUS_RATS => &RATS_TRIGGERS,
        WALL_OF_BLOSSOMS => &BLOSSOMS_TRIGGERS,
        CONTAMINATION => &CONTAMINATION_TRIGGERS,
        STASIS => &STASIS_TRIGGERS,
        FORSAKEN_CITY => &CITY_TRIGGERS,
        ACADEMY_RECTOR => &RECTOR_TRIGGERS,
        VETERAN_EXPLORER => &EXPLORER_TRIGGERS,
        _ => &[],
    }
}

/// Triggered abilities of this module's cards.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match own_triggers(name) {
        [] => crate::eternal_premodern_p1b_v1::triggers_for(name),
        own => own,
    }
}

fn own_trigger_effect(name: &str, effect: &EffectOp) -> bool {
    own_triggers(name)
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
        RAVENOUS_RATS => TargetSpec::TargetOpponent,
        GENESIS => TargetSpec::CreatureCardInOwnGraveyard,
        _ => TargetSpec::None,
    })
}

/// Queued triggers owned by this module's cards.
pub(crate) fn delayed_effect_matches(name: &str, effect: &EffectOp) -> bool {
    if name == KROSAN_TUSKER {
        return *effect == tusker_cycled();
    }
    let EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::P2aV1(op))) = effect else {
        return false;
    };
    match name {
        GENESIS => matches!(op, P2aOpV1::GenesisOfferV1),
        NETHER_SPIRIT => matches!(op, P2aOpV1::NetherSpiritOfferV1),
        BLACK_VISE => matches!(op, P2aOpV1::ViseDamageV1 { .. }),
        GAEAS_BLESSING => matches!(op, P2aOpV1::BlessingMilledV1),
        BANE_OF_THE_LIVING => matches!(op, P2aOpV1::BaneShrinkV1 { .. }),
        _ => false,
    }
}

// ---- Effect leaves ------------------------------------------------------------------

/// Whether `object` is an artifact, creature or enchantment with mana value
/// `x` or less.
fn deed_victim(state: &GameState, object: ObjectId, x: u8) -> bool {
    (has_type(state, object, CardType::Artifact)
        || has_type(state, object, CardType::Creature)
        || has_type(state, object, CardType::Enchantment))
        && crate::engine::object_mana_value(state, object) <= u16::from(x)
}

pub(crate) fn execute(op: &P2aOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        P2aOpV1::DeedV1 { x } => {
            let victims = battlefield(state)
                .filter(|&object| deed_victim(state, object, *x))
                .collect::<Vec<_>>();
            destroy_batch(state, victims);
        }
        P2aOpV1::DiskV1 => {
            let victims = battlefield(state)
                .filter(|&object| deed_victim(state, object, u8::MAX))
                .collect::<Vec<_>>();
            destroy_batch(state, victims);
        }
        P2aOpV1::DragonReturnV1 => {
            if let Some(card) = graveyard_ability_source(ctx, state) {
                event::propose_and_commit(state, ProposedEvent::zone_change(card, Zone::Hand));
            }
        }
        P2aOpV1::VindicateV1 => {
            if let Some(object) = legal_object_target(ctx, state, 0, TargetSpec::AnyPermanent) {
                destroy(ctx, state, object);
            }
        }
        P2aOpV1::ManaShortV1 => {
            let Some(player) = legal_player_target(ctx, state, 0, TargetSpec::AnyPlayer) else {
                return;
            };
            let lands = state.players[player.index()]
                .battlefield
                .iter()
                .copied()
                .filter(|&land| {
                    state.objects.get(land).zone == Zone::Battlefield
                        && state.objects.get(land).controller == player
                        && has_type(state, land, CardType::Land)
                })
                .collect::<Vec<_>>();
            for land in lands {
                event::propose_and_commit(state, ProposedEvent::tap(land));
            }
            let seat = &mut state.players[player.index()];
            seat.mana_pool = [0; 6];
            seat.restricted_mana_pool.0.clear();
        }
        P2aOpV1::UpheavalV1 => {
            let permanents = battlefield(state)
                .map(|object| ProposedEvent::zone_change(object, Zone::Hand))
                .collect::<Vec<_>>();
            event::propose_and_commit_batch(state, permanents);
        }
        P2aOpV1::BlessingShuffleV1 => blessing_shuffle(ctx, state),
        P2aOpV1::BlessingMilledV1 => {
            // "When this card is put into your graveyard from your library":
            // checked again as the ability resolves.
            let Some(card) = graveyard_ability_source(ctx, state) else {
                return;
            };
            let owner = state.objects.get(card).owner;
            let cards = state.players[owner.index()]
                .graveyard
                .iter()
                .copied()
                .map(|card| ProposedEvent::zone_change(card, Zone::Library))
                .collect::<Vec<_>>();
            event::propose_and_commit_batch(state, cards);
            if shuffle(state, owner).is_err() {
                halt(state, ctx.source);
            }
        }
        P2aOpV1::ViseDamageV1 { player } => {
            let cards = state.players[player.index()].hand.len();
            let amount = i32::try_from(cards).unwrap_or(i32::MAX) - 4;
            if amount > 0 {
                event::propose_and_commit(
                    state,
                    event::damage_from_exec_ctx_v1(ctx, Target::Player(*player), amount),
                );
            }
        }
        P2aOpV1::BaneShrinkV1 { x } => {
            let creatures = battlefield(state)
                .filter(|&object| has_type(state, object, CardType::Creature))
                .collect::<Vec<_>>();
            for creature in creatures {
                pump_until_end_of_turn(state, creature, -i32::from(*x), -i32::from(*x));
            }
        }
        P2aOpV1::SacrificeSourceV1 => {
            if let Some(source) = live_ability_source(ctx, state) {
                sacrifice(state, source, ctx.controller);
            }
        }
        P2aOpV1::StasisPayV1 => {
            // "Unless you pay {U}": paying keeps Stasis; failing sacrifices it.
            let Some(stasis) = live_ability_source(ctx, state) else {
                return;
            };
            if !pay(state, ctx.controller, &BLUE) {
                sacrifice(state, stasis, ctx.controller);
            }
        }
        P2aOpV1::GenesisPayV1 => {
            let Some(card) =
                legal_object_target(ctx, state, 0, TargetSpec::CreatureCardInOwnGraveyard)
            else {
                return;
            };
            if graveyard_ability_source(ctx, state).is_some()
                && pay(state, ctx.controller, &TWO_GREEN)
            {
                event::propose_and_commit(state, ProposedEvent::zone_change(card, Zone::Hand));
            }
        }
        P2aOpV1::NetherSpiritReturnV1 => {
            if let Some(card) = nether_spirit_returnable(ctx, state) {
                let owner = state.objects.get(card).owner;
                event::propose_and_commit(
                    state,
                    ProposedEvent::zone_change_to_battlefield_under_controller(card, owner),
                );
            }
        }
        P2aOpV1::RectorExileV1 => {
            if let Some(card) = graveyard_ability_source(ctx, state) {
                event::propose_and_commit(state, ProposedEvent::zone_change(card, Zone::Exile));
            }
        }
        // Choices and frame-pushing leaves run in `drive`.
        P2aOpV1::VerdictV1
        | P2aOpV1::RectorOfferV1
        | P2aOpV1::StasisUpkeepV1
        | P2aOpV1::CityUpkeepV1
        | P2aOpV1::GenesisOfferV1
        | P2aOpV1::NetherSpiritOfferV1 => halt(state, ctx.source),
    }
}

/// Gaea's Blessing: the target player (slot 0) shuffles the target cards
/// (slots 1 to 3) still in their graveyard into their library.
fn blessing_shuffle(ctx: &ExecCtx, state: &mut GameState) {
    let spec = TargetSpec::StandardV1(
        crate::standard_cards_v1::StandardTargetV1::PlayerThenUpToThreeOfTheirGraveyardCardsV1,
    );
    let Some(Target::Player(player)) = ctx.targets.first().copied() else {
        return;
    };
    if !crate::engine::effect_target_is_legal(
        state,
        ctx.source,
        ctx.controller,
        spec,
        &ctx.targets,
        0,
    ) {
        return;
    }
    let mut cards = Vec::new();
    for index in 1..ctx.targets.len() {
        let Some(Target::Object(card)) = ctx.targets.get(index).copied() else {
            continue;
        };
        if ctx.target_incarnation_matches(index, state)
            && state.objects.get(card).zone == Zone::Graveyard
            && state.objects.get(card).owner == player
            && !cards.contains(&card)
        {
            cards.push(card);
        }
    }
    let moves = cards
        .into_iter()
        .map(|card| ProposedEvent::zone_change(card, Zone::Library))
        .collect::<Vec<_>>();
    event::propose_and_commit_batch(state, moves);
    if shuffle(state, player).is_err() {
        halt(state, ctx.source);
    }
}

/// The creature cards in `player`'s graveyard (cards, not tokens).
fn graveyard_creature_cards(state: &GameState, player: PlayerId) -> Vec<ObjectId> {
    state.players[player.index()]
        .graveyard
        .iter()
        .copied()
        .filter(|&card| {
            let live = state.objects.get(card);
            live.owner == player
                && !live.v4.is_token
                && definition(state, card).is_some_and(|def| def.has_type(CardType::Creature))
        })
        .collect()
}

/// Whether the resolving ability's source, a Nether Spirit in its owner's
/// graveyard, is the only creature card there.
fn nether_spirit_returnable(ctx: &ExecCtx, state: &GameState) -> Option<ObjectId> {
    let card = graveyard_ability_source(ctx, state)?;
    let owner = state.objects.get(card).owner;
    (graveyard_creature_cards(state, owner) == [card]).then_some(card)
}

/// Stages one of this module's selections over `candidates`.
#[allow(clippy::too_many_arguments)]
fn offer(
    continuation: &mut EffectContinuation,
    kind: P2aChoiceV1,
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
            kind: E3ChoiceKindV1::P2aV1(kind),
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

fn count_u16(count: usize) -> u16 {
    u16::try_from(count).unwrap_or(u16::MAX)
}

/// Runs a leaf inside the resumable interpreter. `Ok(true)` when it staged a
/// choice and the continuation must suspend.
pub(crate) fn drive(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    op: &P2aOpV1,
    path: Vec<u16>,
) -> Result<bool, String> {
    let controller = continuation.ctx.controller;
    match op {
        P2aOpV1::VerdictV1 => {
            let ctx = continuation.ctx.clone();
            let Some(player) = legal_player_target(&ctx, state, 0, TargetSpec::AnyPlayer) else {
                return Ok(false);
            };
            let hand = state.players[player.index()]
                .hand
                .iter()
                .map(|&card| bind(state, card))
                .collect::<Vec<_>>();
            let need = count_u16(hand.len().min(2));
            if need == 0 {
                return Ok(false);
            }
            if usize::from(need) == hand.len() {
                // The whole hand goes: nothing to choose.
                verdict_discard(state, controller, &hand);
                return Ok(false);
            }
            offer(
                continuation,
                P2aChoiceV1::VerdictDiscardV1,
                player,
                hand,
                need,
                need,
                path,
            );
            Ok(true)
        }
        P2aOpV1::StasisUpkeepV1 => {
            let Some(stasis) = live_ability_source(&continuation.ctx, state) else {
                return Ok(false);
            };
            if !can_pay(state, controller, &BLUE) {
                sacrifice(state, stasis, controller);
                return Ok(false);
            }
            push_frame(
                continuation,
                EffectOp::Choice {
                    controller: PlayerRef::Controller,
                    options: vec![p2a(P2aOpV1::StasisPayV1), p2a(P2aOpV1::SacrificeSourceV1)],
                },
                &path,
            );
            Ok(false)
        }
        P2aOpV1::CityUpkeepV1 => {
            if live_ability_source(&continuation.ctx, state).is_none() {
                return Ok(false);
            }
            let hand = state.players[controller.index()]
                .hand
                .iter()
                .map(|&card| bind(state, card))
                .collect::<Vec<_>>();
            if hand.is_empty() {
                return Ok(false);
            }
            offer(
                continuation,
                P2aChoiceV1::CityExileV1,
                controller,
                hand,
                0,
                1,
                path,
            );
            Ok(true)
        }
        P2aOpV1::GenesisOfferV1 => {
            let ctx = continuation.ctx.clone();
            let affordable = can_pay(state, controller, &TWO_GREEN);
            if !affordable
                || graveyard_ability_source(&ctx, state).is_none()
                || legal_object_target(&ctx, state, 0, TargetSpec::CreatureCardInOwnGraveyard)
                    .is_none()
            {
                return Ok(false);
            }
            push_frame(continuation, may(p2a(P2aOpV1::GenesisPayV1)), &path);
            Ok(false)
        }
        P2aOpV1::RectorOfferV1 => {
            // "When this creature dies, you may exile it": it has to be
            // there still.
            if graveyard_ability_source(&continuation.ctx, state).is_none() {
                return Ok(false);
            }
            push_frame(continuation, may(rector_exile_and_search()), &path);
            Ok(false)
        }
        P2aOpV1::NetherSpiritOfferV1 => {
            if nether_spirit_returnable(&continuation.ctx, state).is_none() {
                return Ok(false);
            }
            push_frame(continuation, may(p2a(P2aOpV1::NetherSpiritReturnV1)), &path);
            Ok(false)
        }
        other => {
            execute(other, &continuation.ctx, state);
            Ok(false)
        }
    }
}

/// Gerrard's Verdict: the discarding player puts `cards` into their
/// graveyard, and `caster` gains 3 life for each land card among them.
fn verdict_discard(state: &mut GameState, caster: PlayerId, cards: &[EffectObjectBinding]) {
    let mut lands = 0_i32;
    for card in cards {
        if !binding_live(state, *card) {
            continue;
        }
        if definition(state, card.object).is_some_and(|def| def.has_type(CardType::Land)) {
            lands += 1;
        }
        crate::engine::commit_discarded_card(state, card.object);
    }
    if lands > 0 {
        event::propose_and_commit(state, ProposedEvent::life_gain(caster, lands * 3));
    }
}

/// Applies an answered choice of this module.
pub(crate) fn finish_selection(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    _choice: &E3ChoiceV1,
    kind: &P2aChoiceV1,
    bindings: Vec<EffectObjectBinding>,
) -> Result<bool, String> {
    let controller = continuation.ctx.controller;
    match kind {
        P2aChoiceV1::VerdictDiscardV1 => {
            verdict_discard(state, controller, &bindings);
            Ok(false)
        }
        P2aChoiceV1::CityExileV1 => {
            let Some(card) = bindings.first().copied() else {
                return Ok(false);
            };
            if !binding_live(state, card) {
                return Ok(false);
            }
            event::propose_and_commit(state, ProposedEvent::zone_change(card.object, Zone::Exile));
            if state.objects.get(card.object).zone != Zone::Exile {
                return Ok(false);
            }
            // "If you do, untap Forsaken City."
            if let Some(city) = live_ability_source(&continuation.ctx, state) {
                crate::engine::attempt_untap(state, city);
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
    note_entries(state, events);
    note_upkeep(state, events);
    note_milled(state, events);
    prune(state);
    let Some(eternal) = state.eternal_v1.as_mut() else {
        return Vec::new();
    };
    if eternal.p2a.queued_triggers.is_empty() {
        return Vec::new();
    }
    std::mem::take(&mut eternal.p2a.queued_triggers)
        .into_iter()
        .map(|queued| {
            let mut pending = crate::eternal_cards_v1::delayed_trigger(
                queued.source,
                queued.controller,
                queued.effect,
            );
            if let Some(spec) = queued.target_spec {
                pending.target_spec = spec;
            }
            pending
        })
        .collect()
}

/// Records the opponent an entering Black Vise chooses, once: "As Black Vise
/// enters, choose an opponent" (the one opponent of this two-player kernel).
pub(crate) fn record_vise(state: &mut GameState, link: ObjectLinkV4) {
    if read_state(state).is_some_and(|p2a| p2a.vises.iter().any(|(host, _)| *host == link)) {
        return;
    }
    let chosen = state.objects.get(link.object).controller.opponent();
    p2a_state(state).vises.push((link, chosen));
}

/// Permanents entering: Black Vise, for an entry that did not pass the
/// entry-counter hook (which records it as it enters).
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
        if state.objects.get(object).zone == Zone::Battlefield
            && definition_name(state, object) == Some(BLACK_VISE)
        {
            let link = live_link(state, object);
            record_vise(state, link);
        }
    }
}

/// Upkeeps: the triggers of Genesis and Nether Spirit from their owner
/// graveyard, and Black Vise damage at its chosen player upkeep.
fn note_upkeep(state: &mut GameState, events: &[CommittedEvent]) {
    for event in events {
        let CommittedEvent::UpkeepBegan { player } = *event else {
            continue;
        };
        let cards = state.players[player.index()]
            .graveyard
            .iter()
            .copied()
            .filter(|&card| {
                let live = state.objects.get(card);
                live.owner == player && !live.v4.is_token
            })
            .collect::<Vec<_>>();
        for card in cards {
            match definition_name(state, card) {
                // "if Genesis is in your graveyard"
                Some(GENESIS) => {
                    let source = AbilitySourceContractV4::capture(state, card);
                    queue_trigger(
                        state,
                        source,
                        player,
                        p2a(P2aOpV1::GenesisOfferV1),
                        Some(TargetSpec::CreatureCardInOwnGraveyard),
                    );
                }
                // "if Nether Spirit is the only creature card in your graveyard"
                Some(NETHER_SPIRIT) if graveyard_creature_cards(state, player) == [card] => {
                    let source = AbilitySourceContractV4::capture(state, card);
                    queue_trigger(
                        state,
                        source,
                        player,
                        p2a(P2aOpV1::NetherSpiritOfferV1),
                        None,
                    );
                }
                _ => {}
            }
        }
        let vises = read_state(state)
            .map(|p2a| p2a.vises.clone())
            .unwrap_or_default();
        for (link, chosen) in vises {
            if chosen != player
                || !link_live_in(state, link, Zone::Battlefield)
                || !active_named(state, link.object, BLACK_VISE)
            {
                continue;
            }
            let source = AbilitySourceContractV4::capture(state, link.object);
            let owner = state.objects.get(link.object).controller;
            queue_trigger(
                state,
                source,
                owner,
                p2a(P2aOpV1::ViseDamageV1 { player }),
                None,
            );
        }
    }
}

/// Gaea Blessing: "When this card is put into your graveyard from your
/// library".
fn note_milled(state: &mut GameState, events: &[CommittedEvent]) {
    for event in events {
        let CommittedEvent::ZoneChange {
            object,
            from: Zone::Library,
            to: Zone::Graveyard,
            ..
        } = *event
        else {
            continue;
        };
        let live = state.objects.get(object);
        if live.zone != Zone::Graveyard
            || live.v4.is_token
            || definition_name(state, object) != Some(GAEAS_BLESSING)
        {
            continue;
        }
        let owner = live.owner;
        let source = AbilitySourceContractV4::capture(state, object);
        queue_trigger(state, source, owner, p2a(P2aOpV1::BlessingMilledV1), None);
    }
}

/// Drops records whose objects moved on.
fn prune(state: &mut GameState) {
    let Some(p2a) = read_state(state) else {
        return;
    };
    if p2a.is_empty() {
        return;
    }
    let vises = p2a
        .vises
        .iter()
        .filter(|(link, _)| link_live_in(state, *link, Zone::Battlefield))
        .copied()
        .collect::<Vec<_>>();
    let bane_x = p2a
        .bane_x
        .filter(|link| link_live_in(state, *link, Zone::Battlefield));
    let p2a = p2a_state(state);
    p2a.vises = vises;
    p2a.bane_x = bane_x;
}
