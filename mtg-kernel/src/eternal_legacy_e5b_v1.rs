//! Card programs for the eternal catalog's Legacy aggro, Eldrazi and control
//! lists (wave E5b, `docs/design/eternal_formats_support_v1.md`): Boros
//! Energy, Eldrazi, Tron, Beanstalk Control and the Azorius, Jeskai and
//! Esper tempo and control lists.
//!
//! `eternal_cards_v1` forwards its effect leaves, trigger conditions and
//! state here, and the engine calls the hooks below only in
//! `eternal-fixtures` builds. Every table is keyed by a printed name that
//! occurs only in the eternal registry, so the Pauper, FDN and Standard
//! catalogs never reach it. Behavior was read from each card's current
//! Oracle text and, where present, its XMage source.
//!
//! Resolution choices use E3's typed selection (`E3ChoiceKindV1::E5bV1`)
//! or a generated option list (`EffectOp::Choice`). A spell cast during a
//! resolution (miracle, Amped Raptor) pays its alternative cost first, then
//! uses the free resolution cast that Mad Wizard's Lair introduced.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

use crate::card_def::{
    card_id_by_name, CardDef, CardType, Keywords, Subtype, TargetSpec, CARD_DEFS,
};
use crate::effect::{
    EffectContinuation, EffectFrame, EffectObjectBinding, EffectOp, ExecCtx, ObjectRef, PlayerRef,
    TargetRef,
};
use crate::eternal_cards_v1::e3::{E3ChoiceKindV1, E3ChoiceV1};
use crate::event::{self, CommittedEvent, ProposedEvent};
use crate::ids::{ObjectId, PlayerId, StackItemId};
use crate::mana::ManaColor;
use crate::standard_cards_v1::{StandardOpV1, StandardTargetV1};
use crate::state::{
    AbilitySourceContractV4, GameState, ObjectLinkV4, SpellCastRouteV4, StackItemKind, Step,
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
pub struct E5bStateV1 {
    /// Amped Raptor spells cast from their owner's hand, by stack
    /// incarnation ("Then if you cast it from your hand").
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hand_casts: Vec<ObjectLinkV4>,
    /// Phlage spells cast with escape, by stack incarnation ("unless it
    /// escaped").
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub escaped_casts: Vec<ObjectLinkV4>,
    /// Miracle cards drawn as their owner's first card this turn, waiting
    /// for the owner's choice whether to reveal them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub miracle_offers: Vec<ObjectLinkV4>,
    /// Triggers detected by hooks, waiting for the next trigger collection.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub queued_triggers: Vec<QueuedTriggerV1>,
    /// Phelia's exiles, returning at the beginning of the next end step.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub phelia_returns: Vec<PheliaReturnV1>,
    /// Mobilize tokens, sacrificed at the beginning of the next end step.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mobilized: Vec<MobilizedV1>,
    /// Sand Scouts whose "triggers only once each turn" ability has
    /// triggered, with the turn key.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sand_scout_turns: Vec<(ObjectLinkV4, u32)>,
    /// Players with the city's blessing (ascend, 702.131).
    #[serde(default, skip_serializing_if = "no_blessing")]
    pub city_blessing: [bool; 2],
    /// Petrified Hamlets and the land definition whose name each chose.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hamlet_names: Vec<(ObjectLinkV4, u16)>,
}

fn no_blessing(blessing: &[bool; 2]) -> bool {
    !blessing[0] && !blessing[1]
}

impl E5bStateV1 {
    pub(crate) fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// A triggered ability queued by a hook, with its target specification.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueuedTriggerV1 {
    pub source: AbilitySourceContractV4,
    pub controller: PlayerId,
    pub op: E5bOpV1,
}

/// "At the beginning of the next end step, return that card to the
/// battlefield under its owner's control" (Phelia, Exuberant Shepherd).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PheliaReturnV1 {
    pub source: AbilitySourceContractV4,
    pub controller: PlayerId,
    pub card: ObjectLinkV4,
}

/// Mobilize's tokens: "Sacrifice them at the beginning of the next end
/// step."
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MobilizedV1 {
    pub source: AbilitySourceContractV4,
    pub controller: PlayerId,
    pub tokens: Vec<ObjectLinkV4>,
}

fn e5b_state(state: &mut GameState) -> &mut E5bStateV1 {
    &mut state.eternal_v1.get_or_insert_with(Default::default).e5b
}

fn read_state(state: &GameState) -> Option<&E5bStateV1> {
    state.eternal_v1.as_ref().map(|eternal| &eternal.e5b)
}

/// Trigger conditions owned by this module, carried as
/// `eternal_cards_v1::EternalTriggerV1::E5bV1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum E5bTriggerV1 {
    /// "Whenever one or more other Cats you control die" (Ajani, Nacatl
    /// Pariah). One trigger per collected event batch.
    OtherCatsYouControlDieV1,
    /// "At the beginning of your end step, if you gained life this turn"
    /// (Ocelot Pride).
    YourEndStepIfGainedLifeV1,
    /// "When this creature enters, if an opponent controls more lands than
    /// you" (Sand Scout).
    EntersIfOpponentHasMoreLandsV1,
    /// "When you cast this spell, if it was kicked with its {G} kicker"
    /// (Wastescape Battlemage). Functions on the stack.
    CastSelfIfKickedV1,
    /// "When you cast this spell, if it was kicked with its {1}{U} kicker":
    /// cast for its alternative cost (Wastescape Battlemage). Functions on
    /// the stack.
    CastSelfWithAlternativeCostV1,
}

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::E5bV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum E5bOpV1 {
    /// Stifle: counter target 0, an activated or triggered ability.
    CounterTargetAbilityV1,
    /// Consign to Memory: counter target 0, a triggered ability or a
    /// colorless spell.
    CounterTriggeredAbilityOrColorlessSpellV1,
    /// Galvanic Discharge: get three energy, then choose how much to pay.
    GalvanicDischargeV1,
    /// Galvanic Discharge's answer: pay `amount` energy, deal that much
    /// damage to target 0.
    PayEnergyDamageTargetV1 { amount: u32 },
    /// Wrath of the Skies: get X energy, then choose how much to pay.
    WrathOfTheSkiesV1,
    /// Wrath of the Skies' answer: pay `amount` energy, destroy each
    /// artifact, creature and enchantment with mana value at most that.
    PayEnergyDestroyV1 { amount: u32 },
    /// Terminus: every creature goes to the bottom of its owner's library,
    /// in an order its owner chooses.
    TerminusV1,
    /// A revealed miracle card's trigger: its owner may cast it for its
    /// miracle cost.
    MiracleV1,
    /// The miracle answer: pay the miracle cost and cast the card.
    MiracleCastV1,
    /// Amped Raptor's entry: get two energy, then (if cast from hand)
    /// exile cards until a nonland card and offer to cast it for energy.
    AmpedRaptorV1,
    /// Amped Raptor's answer: pay energy equal to `card`'s mana value and
    /// cast it.
    RaptorCastV1 { card: ObjectLinkV4 },
    /// Reality Smasher: counter `spell` unless its controller discards a
    /// card.
    SmasherV1 { spell: StackItemId },
    /// Ajani, Nacatl Pariah: "you may exile Ajani, then return him to the
    /// battlefield transformed under his owner's control."
    AjaniMayTransformV1,
    /// Ajani, Nacatl Avenger's +2: a +1/+1 counter on each Cat you control.
    AjaniCountersV1,
    /// Ajani, Nacatl Avenger's 0: a Cat Warrior, then the reflexive damage
    /// trigger if the controller controls another red permanent.
    AjaniTokenThenDamageV1,
    /// The reflexive trigger: damage equal to the number of creatures the
    /// controller controls to target 0.
    AjaniDamageV1,
    /// Ajani, Nacatl Avenger's -4. A choice.
    AjaniSacrificeRestV1,
    /// Guide of Souls' attack trigger: may pay {E}{E}{E}.
    GuideMayPayV1,
    /// Guide of Souls' payment: pay {E}{E}{E} and queue the reflexive
    /// trigger.
    GuidePayV1,
    /// The reflexive trigger: two +1/+1 counters and a flying counter on
    /// target 0, which becomes an Angel in addition to its other types.
    GuideAngelV1,
    /// Eldrazi Linebreaker: target 0 gains haste and gets +X/+0, X the
    /// number of Eldrazi the controller controls.
    LinebreakerV1,
    /// Ocelot Pride's end step trigger.
    OcelotPrideV1,
    /// Phelia's attack trigger: exile up to one target, return it at the
    /// next end step.
    PheliaExileV1,
    /// Phelia's delayed trigger: return `card`.
    PheliaReturnV1 { card: ObjectLinkV4 },
    /// Phlage: sacrifice it unless it escaped.
    PhlageSacrificeUnlessEscapedV1,
    /// Static Prison: sacrifice it unless its controller pays {E}. A choice.
    StaticPrisonUpkeepV1,
    /// Static Prison's payment: pay {E}.
    PayOneEnergyV1,
    /// Mobilize N: create N tapped and attacking 1/1 red Warriors.
    MobilizeV1 { count: u8 },
    /// Mobilize's delayed trigger: sacrifice `tokens`.
    SacrificeMobilizedV1 { tokens: Vec<ObjectLinkV4> },
    /// Sand Scout's entry search, rechecking its condition.
    SandScoutSearchV1,
    /// Sand Scout's land trigger: create a Sand Warrior.
    CreateSandWarriorV1,
    /// Petrified Hamlet: choose a land card name. A choice.
    HamletChooseNameV1,
    /// Petrified Hamlet's answer: the chosen land definition.
    HamletNameV1 { card_def: u16 },
    /// Lazotep Quarry: exile target 0 and create a 4/4 black Zombie copy.
    LazotepZombieV1,
    /// Eldrazi Confluence: the three chosen modes in printed order (0: +3/-3,
    /// 1: flicker, 2: Scion), each with its own target slot.
    ConfluenceV1 { modes: [u8; 3] },
    /// Wastescape Battlemage's {G} kicker trigger: exile target 0.
    ExileTargetV1,
    /// Wastescape Battlemage's {1}{U} kicker trigger: return target 0 to its
    /// owner's hand.
    BounceTargetV1,
}

impl E5bOpV1 {
    /// Whether this leaf can yield a player decision, so its program must
    /// use the resumable interpreter.
    pub(crate) fn contains_player_choice(&self) -> bool {
        matches!(
            self,
            Self::GalvanicDischargeV1
                | Self::WrathOfTheSkiesV1
                | Self::TerminusV1
                | Self::MiracleV1
                | Self::MiracleCastV1
                | Self::AmpedRaptorV1
                | Self::RaptorCastV1 { .. }
                | Self::SmasherV1 { .. }
                | Self::AjaniMayTransformV1
                | Self::AjaniSacrificeRestV1
                | Self::GuideMayPayV1
                | Self::StaticPrisonUpkeepV1
                | Self::HamletChooseNameV1
                | Self::SandScoutSearchV1
        )
    }
}

fn e5b(op: E5bOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(
        crate::eternal_cards_v1::EternalOpV1::E5bV1(op),
    ))
}

fn eternal(op: crate::eternal_cards_v1::EternalOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(op))
}

pub(crate) const AJANI: &str = "Ajani, Nacatl Pariah";
const AMPED_RAPTOR: &str = "Amped Raptor";
const ELDRAZI_LINEBREAKER: &str = "Eldrazi Linebreaker";
const ELDRAZI_TEMPLE: &str = "Eldrazi Temple";
const EYE_OF_UGIN: &str = "Eye of Ugin";
const GUIDE_OF_SOULS: &str = "Guide of Souls";
const IT_THAT_HERALDS: &str = "It That Heralds the End";
const LEYLINE_BINDING: &str = "Leyline Binding";
const OCELOT_PRIDE: &str = "Ocelot Pride";
const PETRIFIED_HAMLET: &str = "Petrified Hamlet";
const PHELIA: &str = "Phelia, Exuberant Shepherd";
const PHLAGE: &str = "Phlage, Titan of Fire's Fury";
const QUANTUM_RIDDLER: &str = "Quantum Riddler";
const REALITY_SMASHER: &str = "Reality Smasher";
const SAND_SCOUT: &str = "Sand Scout";
const STATIC_PRISON: &str = "Static Prison";
const TERMINUS: &str = "Terminus";
const THOUGHT_KNOT_SEER: &str = "Thought-Knot Seer";
const UP_THE_BEANSTALK: &str = "Up the Beanstalk";
const VOICE_OF_VICTORY: &str = "Voice of Victory";
const WASTESCAPE_BATTLEMAGE: &str = "Wastescape Battlemage";

const CAT_WARRIOR_TOKEN: &str = "Cat Warrior Token";
const CAT_TOKEN: &str = "Cat Token";
const WARRIOR_TOKEN: &str = "Warrior Token";
const SAND_WARRIOR_TOKEN: &str = "Sand Warrior Token";
const ELDRAZI_SCION_TOKEN: &str = "Eldrazi Scion Token";

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

fn printed(state: &GameState, object: ObjectId) -> &'static CardDef {
    &CARD_DEFS[state.objects.get(object).card_def as usize]
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

/// The battlefield permanents `player` controls.
fn controlled_permanents(state: &GameState, player: PlayerId) -> Vec<ObjectId> {
    [PlayerId::P0, PlayerId::P1]
        .into_iter()
        .flat_map(|seat| state.players[seat.index()].battlefield.iter().copied())
        .filter(|&object| {
            let live = state.objects.get(object);
            live.zone == Zone::Battlefield && live.controller == player
        })
        .collect()
}

fn is_colorless(state: &GameState, object: ObjectId) -> bool {
    // Painter's Servant colors every card, spell and permanent (wave E9a).
    state.objects.get(object).v4.effective_color_mask == 0
        && crate::eternal_legacy_e9a_v1::statics::painter_mask(state) == 0
}

/// The resolving ability's source, if it is still the incarnation that
/// created the ability, on the battlefield.
fn live_ability_source(ctx: &ExecCtx, state: &GameState) -> Option<ObjectId> {
    let contract = ctx.ability_source_contract?;
    let live = state.objects.try_get(contract.source)?;
    (live.zone == Zone::Battlefield && live.zone_change_count == contract.zone_change_count)
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
        && state.objects.get(object).zone == Zone::Battlefield
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

fn queue_trigger(
    state: &mut GameState,
    source: AbilitySourceContractV4,
    controller: PlayerId,
    op: E5bOpV1,
) {
    e5b_state(state).queued_triggers.push(QueuedTriggerV1 {
        source,
        controller,
        op,
    });
}

/// "Sacrifice it" for a resolving ability's own source incarnation.
fn sacrifice_source() -> EffectOp {
    eternal(crate::eternal_cards_v1::EternalOpV1::SacrificeSourceV1)
}

fn nothing() -> EffectOp {
    EffectOp::Sequence(Vec::new())
}

// ---- Spells -------------------------------------------------------------------------

/// Stifle: "Counter target activated or triggered ability. (Mana abilities
/// can't be targeted.)"
pub fn stifle() -> EffectOp {
    e5b(E5bOpV1::CounterTargetAbilityV1)
}

/// Consign to Memory: "Counter target triggered ability or colorless
/// spell." Replicate {1} is `replicate_cost` below.
pub fn consign_to_memory() -> EffectOp {
    e5b(E5bOpV1::CounterTriggeredAbilityOrColorlessSpellV1)
}

/// Replicate costs of this module's cards (702.56).
pub fn replicate_cost(name: &str) -> Option<crate::mana::Cost> {
    match name {
        "Consign to Memory" => Some(crate::mana::Cost {
            pips: &[],
            generic: 1,
            x_count: 0,
        }),
        _ => crate::eternal_pauper_e6a_v1::replicate_cost(name),
    }
}

/// Galvanic Discharge: "Choose target creature or planeswalker. You get
/// {E}{E}{E}, then you may pay any amount of {E}. Galvanic Discharge deals
/// that much damage to that permanent."
pub fn galvanic_discharge() -> EffectOp {
    e5b(E5bOpV1::GalvanicDischargeV1)
}

/// Wrath of the Skies: "You get X {E}, then you may pay any amount of {E}.
/// Destroy each artifact, creature, and enchantment with mana value less
/// than or equal to the amount of {E} paid this way."
pub fn wrath_of_the_skies() -> EffectOp {
    e5b(E5bOpV1::WrathOfTheSkiesV1)
}

/// Terminus: "Put all creatures on the bottom of their owners' libraries."
/// Miracle {W} is its draw trigger (`miracle_cost`).
pub fn terminus() -> EffectOp {
    e5b(E5bOpV1::TerminusV1)
}

/// Eldrazi Confluence's ten mode choices: "Choose three. You may choose the
/// same mode more than once." Index `n` names the multiset of modes
/// (0: +3/-3, 1: flicker, 2: Scion) with `n` the position in this list.
pub(crate) const CONFLUENCE_MODES: [[u8; 3]; 10] = [
    [0, 0, 0],
    [0, 0, 1],
    [0, 0, 2],
    [0, 1, 1],
    [0, 1, 2],
    [0, 2, 2],
    [1, 1, 1],
    [1, 1, 2],
    [1, 2, 2],
    [2, 2, 2],
];

fn confluence(index: usize) -> EffectOp {
    e5b(E5bOpV1::ConfluenceV1 {
        modes: CONFLUENCE_MODES[index],
    })
}

pub fn eldrazi_confluence_0() -> EffectOp {
    confluence(0)
}
pub fn eldrazi_confluence_1() -> EffectOp {
    confluence(1)
}
pub fn eldrazi_confluence_2() -> EffectOp {
    confluence(2)
}
pub fn eldrazi_confluence_3() -> EffectOp {
    confluence(3)
}
pub fn eldrazi_confluence_4() -> EffectOp {
    confluence(4)
}
pub fn eldrazi_confluence_5() -> EffectOp {
    confluence(5)
}
pub fn eldrazi_confluence_6() -> EffectOp {
    confluence(6)
}
pub fn eldrazi_confluence_7() -> EffectOp {
    confluence(7)
}
pub fn eldrazi_confluence_8() -> EffectOp {
    confluence(8)
}
pub fn eldrazi_confluence_9() -> EffectOp {
    confluence(9)
}

/// The target filter of Eldrazi Confluence's mode multiset `modes`: one
/// creature slot per +3/-3, then one nonland permanent slot per flicker.
pub(crate) fn confluence_target(modes: [u8; 3]) -> TargetSpec {
    let creatures = modes.iter().filter(|&&mode| mode == 0).count() as u8;
    let permanents = modes.iter().filter(|&&mode| mode == 1).count() as u8;
    if creatures + permanents == 0 {
        TargetSpec::None
    } else {
        TargetSpec::StandardV1(StandardTargetV1::CreaturesThenNonlandPermanentsV1 {
            creatures,
            permanents,
        })
    }
}

// ---- Activated ability programs -----------------------------------------------------

/// Ajani, Nacatl Avenger's +2: "Put a +1/+1 counter on each Cat you
/// control."
pub fn ajani_plus_two() -> EffectOp {
    e5b(E5bOpV1::AjaniCountersV1)
}

/// Ajani, Nacatl Avenger's 0: "Create a 2/1 white Cat Warrior creature
/// token. When you do, if you control a red permanent other than Ajani, he
/// deals damage equal to the number of creatures you control to any
/// target."
pub fn ajani_zero() -> EffectOp {
    e5b(E5bOpV1::AjaniTokenThenDamageV1)
}

/// Ajani, Nacatl Avenger's -4: "Each opponent chooses an artifact, a
/// creature, an enchantment, and a planeswalker from among the nonland
/// permanents they control, then sacrifices the rest."
pub fn ajani_minus_four() -> EffectOp {
    e5b(E5bOpV1::AjaniSacrificeRestV1)
}

/// Lazotep Quarry: "Exile target creature card with mana value X from your
/// graveyard. Create a token that's a copy of it, except it's a 4/4 black
/// Zombie."
pub fn lazotep_zombie() -> EffectOp {
    e5b(E5bOpV1::LazotepZombieV1)
}

/// Eye of Ugin: "Search your library for a colorless creature card, reveal
/// it, put it into your hand, then shuffle."
pub fn eye_of_ugin_search() -> EffectOp {
    EffectOp::SearchLibraryToHand {
        player: PlayerRef::Controller,
        filter: crate::effect::LibraryCardFilter::ColorlessCreatureV1,
    }
}

/// Abundant Countryside: "Create a 1/1 colorless Shapeshifter creature token
/// with changeling."
pub fn abundant_countryside_token() -> EffectOp {
    EffectOp::CreateToken {
        token_def: token_id("Shapeshifter Token"),
        controller: PlayerRef::Controller,
    }
}

// ---- Triggered abilities -----------------------------------------------------------

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

const fn e5b_trigger(condition: E5bTriggerV1, effect: fn() -> EffectOp) -> TriggeredAbilityDef {
    trigger(
        TriggerCondition::StandardV1(crate::standard_cards_v1::StandardTriggerV1::EternalV1(
            crate::eternal_cards_v1::EternalTriggerV1::E5bV1(condition),
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

fn create_cat_warrior() -> EffectOp {
    EffectOp::CreateToken {
        token_def: token_id(CAT_WARRIOR_TOKEN),
        controller: PlayerRef::Controller,
    }
}

fn ajani_may_transform() -> EffectOp {
    e5b(E5bOpV1::AjaniMayTransformV1)
}

fn amped_raptor_enters() -> EffectOp {
    e5b(E5bOpV1::AmpedRaptorV1)
}

/// Exile target ... until this leaves the battlefield (Leyline Binding).
fn exile_until_leaves() -> EffectOp {
    EffectOp::ExileTargetLinkedToSource {
        object: ObjectRef::Target(0),
    }
}

/// Static Prison: "exile target nonland permanent an opponent controls
/// until this enchantment leaves the battlefield. You get {E}{E}."
fn static_prison_enters() -> EffectOp {
    EffectOp::Sequence(vec![
        exile_until_leaves(),
        eternal(crate::eternal_cards_v1::EternalOpV1::GainEnergy { amount: 2 }),
    ])
}

/// Guide of Souls: "Whenever another creature you control enters, you gain
/// 1 life and get {E}."
fn guide_life_and_energy() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::GainLife {
            player: PlayerRef::Controller,
            amount: 1,
        },
        eternal(crate::eternal_cards_v1::EternalOpV1::GainEnergy { amount: 1 }),
    ])
}

fn guide_may_pay() -> EffectOp {
    e5b(E5bOpV1::GuideMayPayV1)
}

fn linebreaker_pump() -> EffectOp {
    e5b(E5bOpV1::LinebreakerV1)
}

fn ocelot_end_step() -> EffectOp {
    e5b(E5bOpV1::OcelotPrideV1)
}

fn phelia_exile() -> EffectOp {
    e5b(E5bOpV1::PheliaExileV1)
}

fn phlage_sacrifice() -> EffectOp {
    e5b(E5bOpV1::PhlageSacrificeUnlessEscapedV1)
}

/// Phlage: "it deals 3 damage to any target and you gain 3 life."
fn phlage_damage() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::DealDamage {
            target: TargetRef::Target(0),
            amount: 3,
        },
        EffectOp::GainLife {
            player: PlayerRef::Controller,
            amount: 3,
        },
    ])
}

fn sand_scout_search() -> EffectOp {
    e5b(E5bOpV1::SandScoutSearchV1)
}

fn sand_warrior() -> EffectOp {
    EffectOp::CreateToken {
        token_def: token_id(SAND_WARRIOR_TOKEN),
        controller: PlayerRef::Controller,
    }
}

/// Thought-Knot Seer: "target opponent reveals their hand. You choose a
/// nonland card from it and exile that card."
fn thought_knot_exile() -> EffectOp {
    EffectOp::RevealHandChooseNonlandToLinkedExile {
        player: PlayerRef::Target(0),
    }
}

/// Thought-Knot Seer: "target opponent draws a card."
fn thought_knot_draw() -> EffectOp {
    EffectOp::DrawCards {
        player: PlayerRef::Target(0),
        count: 1,
    }
}

fn hamlet_choose_name() -> EffectOp {
    e5b(E5bOpV1::HamletChooseNameV1)
}

fn mobilize_two() -> EffectOp {
    e5b(E5bOpV1::MobilizeV1 { count: 2 })
}

fn wastescape_exile() -> EffectOp {
    e5b(E5bOpV1::ExileTargetV1)
}

fn wastescape_bounce() -> EffectOp {
    e5b(E5bOpV1::BounceTargetV1)
}

/// Warp's "at the beginning of the next end step, exile this creature"
/// (the engine's warp machinery, as on Nova Hellkite).
fn warp_exile() -> EffectOp {
    EffectOp::BindWarpExileToTriggerSource
}

const UP_THE_BEANSTALK_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(TriggerCondition::Etb, draw_one),
    trigger(TriggerCondition::CastSpellManaValueAtLeast(5), draw_one),
];
const AJANI_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(TriggerCondition::Etb, create_cat_warrior),
    e5b_trigger(E5bTriggerV1::OtherCatsYouControlDieV1, ajani_may_transform),
];
const AMPED_RAPTOR_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, amped_raptor_enters)];
const LEYLINE_BINDING_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, exile_until_leaves)];
const STATIC_PRISON_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, static_prison_enters)];
const GUIDE_OF_SOULS_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(
        TriggerCondition::OtherControlledCreatureEnters { subtype: None },
        guide_life_and_energy,
    ),
    trigger(TriggerCondition::ControllerAttacks, guide_may_pay),
];
const LINEBREAKER_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(
    TriggerCondition::BeginningOfControllerCombat,
    linebreaker_pump,
)];
const OCELOT_PRIDE_TRIGGERS: [TriggeredAbilityDef; 1] = [e5b_trigger(
    E5bTriggerV1::YourEndStepIfGainedLifeV1,
    ocelot_end_step,
)];
const PHELIA_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Attacks, phelia_exile)];
const PHLAGE_TRIGGERS: [TriggeredAbilityDef; 3] = [
    trigger(TriggerCondition::Etb, phlage_sacrifice),
    trigger(TriggerCondition::Etb, phlage_damage),
    trigger(TriggerCondition::Attacks, phlage_damage),
];
const QUANTUM_RIDDLER_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(TriggerCondition::Etb, draw_one),
    trigger(TriggerCondition::BeginningEndStepAfterWarp, warp_exile),
];
const SAND_SCOUT_TRIGGERS: [TriggeredAbilityDef; 1] = [e5b_trigger(
    E5bTriggerV1::EntersIfOpponentHasMoreLandsV1,
    sand_scout_search,
)];
const THOUGHT_KNOT_SEER_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(TriggerCondition::Etb, thought_knot_exile),
    trigger(TriggerCondition::LeftBattlefield, thought_knot_draw),
];
const PETRIFIED_HAMLET_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, hamlet_choose_name)];
const VOICE_OF_VICTORY_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Attacks, mobilize_two)];
const WASTESCAPE_TRIGGERS: [TriggeredAbilityDef; 2] = [
    TriggeredAbilityDef {
        home_zone: Zone::Stack,
        ..e5b_trigger(E5bTriggerV1::CastSelfIfKickedV1, wastescape_exile)
    },
    TriggeredAbilityDef {
        home_zone: Zone::Stack,
        ..e5b_trigger(
            E5bTriggerV1::CastSelfWithAlternativeCostV1,
            wastescape_bounce,
        )
    },
];

/// Triggered abilities of this module's cards.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        UP_THE_BEANSTALK => &UP_THE_BEANSTALK_TRIGGERS,
        AJANI => &AJANI_TRIGGERS,
        AMPED_RAPTOR => &AMPED_RAPTOR_TRIGGERS,
        LEYLINE_BINDING => &LEYLINE_BINDING_TRIGGERS,
        STATIC_PRISON => &STATIC_PRISON_TRIGGERS,
        GUIDE_OF_SOULS => &GUIDE_OF_SOULS_TRIGGERS,
        ELDRAZI_LINEBREAKER => &LINEBREAKER_TRIGGERS,
        OCELOT_PRIDE => &OCELOT_PRIDE_TRIGGERS,
        PHELIA => &PHELIA_TRIGGERS,
        PHLAGE => &PHLAGE_TRIGGERS,
        QUANTUM_RIDDLER => &QUANTUM_RIDDLER_TRIGGERS,
        SAND_SCOUT => &SAND_SCOUT_TRIGGERS,
        THOUGHT_KNOT_SEER => &THOUGHT_KNOT_SEER_TRIGGERS,
        PETRIFIED_HAMLET => &PETRIFIED_HAMLET_TRIGGERS,
        VOICE_OF_VICTORY => &VOICE_OF_VICTORY_TRIGGERS,
        WASTESCAPE_BATTLEMAGE => &WASTESCAPE_TRIGGERS,
        _ => crate::eternal_pauper_e6a_v1::triggers_for(name),
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
    // `triggers_for` falls through to E6a's cards, so they answer first.
    if let Some(spec) = crate::eternal_pauper_e6a_v1::trigger_target_spec(name, effect) {
        return Some(spec);
    }
    if !own_trigger_effect(name, effect) {
        return None;
    }
    let opponent_nonland = TargetSpec::StandardV1(StandardTargetV1::OpponentNonlandPermanent);
    Some(match name {
        LEYLINE_BINDING | STATIC_PRISON
            if *effect == exile_until_leaves() || *effect == static_prison_enters() =>
        {
            opponent_nonland
        }
        ELDRAZI_LINEBREAKER if *effect == linebreaker_pump() => TargetSpec::ControlledCreature,
        PHELIA if *effect == phelia_exile() => {
            TargetSpec::StandardV1(StandardTargetV1::UpToOneOtherNonlandPermanentV1)
        }
        PHLAGE if *effect == phlage_damage() => TargetSpec::AnyTarget,
        THOUGHT_KNOT_SEER => TargetSpec::TargetOpponent,
        WASTESCAPE_BATTLEMAGE if *effect == wastescape_exile() => {
            TargetSpec::OpponentArtifactOrEnchantmentPermanent
        }
        WASTESCAPE_BATTLEMAGE if *effect == wastescape_bounce() => {
            TargetSpec::OpponentControlledCreature
        }
        AJANI if *effect == e5b(E5bOpV1::AjaniDamageV1) => TargetSpec::AnyTarget,
        GUIDE_OF_SOULS if *effect == e5b(E5bOpV1::GuideAngelV1) => TargetSpec::AttackingCreature,
        _ => TargetSpec::None,
    })
}

/// Queued and delayed triggers owned by this module's cards.
pub(crate) fn delayed_effect_matches(name: &str, effect: &EffectOp) -> bool {
    if crate::eternal_pauper_e6a_v1::delayed_effect_matches(name, effect) {
        return true;
    }
    let EffectOp::StandardV1(StandardOpV1::EternalV1(crate::eternal_cards_v1::EternalOpV1::E5bV1(
        op,
    ))) = effect
    else {
        return name == STATIC_PRISON && *effect == sacrifice_source();
    };
    match name {
        AJANI => *op == E5bOpV1::AjaniDamageV1,
        GUIDE_OF_SOULS => *op == E5bOpV1::GuideAngelV1,
        REALITY_SMASHER => matches!(op, E5bOpV1::SmasherV1 { .. }),
        TERMINUS => *op == E5bOpV1::MiracleV1,
        PHELIA => matches!(op, E5bOpV1::PheliaReturnV1 { .. }),
        VOICE_OF_VICTORY => matches!(op, E5bOpV1::SacrificeMobilizedV1 { .. }),
        SAND_SCOUT => *op == E5bOpV1::CreateSandWarriorV1,
        STATIC_PRISON => *op == E5bOpV1::StaticPrisonUpkeepV1,
        _ => false,
    }
}

/// Whether `condition` triggers on its source's own arrival, so the
/// trigger's source is the arriving incarnation.
pub(crate) fn arrival_condition(condition: E5bTriggerV1) -> bool {
    condition == E5bTriggerV1::EntersIfOpponentHasMoreLandsV1
}

/// Whether `condition` functions while its source is a spell on the stack.
pub(crate) fn functions_on_stack(condition: E5bTriggerV1) -> bool {
    matches!(
        condition,
        E5bTriggerV1::CastSelfIfKickedV1 | E5bTriggerV1::CastSelfWithAlternativeCostV1
    )
}

/// The stack item of the spell `source` just cast, if it was cast by its
/// controller as `events[index]`.
fn own_cast_item<'a>(
    state: &'a GameState,
    events: &[CommittedEvent],
    index: usize,
    source: ObjectId,
) -> Option<&'a crate::state::StackItem> {
    let CommittedEvent::SpellCast { spell, controller } = events[index] else {
        return None;
    };
    if spell != source {
        return None;
    }
    state.stack.iter().find(|item| {
        item.kind == StackItemKind::Spell
            && item.source == source
            && item.controller == controller
            && !item.is_copy
    })
}

fn lands_controlled(state: &GameState, player: PlayerId) -> usize {
    controlled_permanents(state, player)
        .into_iter()
        .filter(|&object| crate::engine::object_has_type(state, object, CardType::Land))
        .count()
}

fn opponent_has_more_lands(state: &GameState, player: PlayerId) -> bool {
    lands_controlled(state, player.opponent()) > lands_controlled(state, player)
}

/// Whether `player` gained life this turn. The turn begins at its
/// `UpkeepBegan` marker; nobody gains life during an untap step.
fn gained_life_this_turn(state: &GameState, player: PlayerId) -> bool {
    for event in state.engine.event_history.iter().rev() {
        match event {
            CommittedEvent::UpkeepBegan { .. } => return false,
            CommittedEvent::LifeGain {
                player: gainer,
                amount,
            } if *gainer == player && *amount > 0 => return true,
            _ => {}
        }
    }
    false
}

/// Whether the object a committed zone change moved was a Cat creature card
/// or token, read from its printed characteristics (changelings included).
fn was_cat_creature(state: &GameState, object: ObjectId) -> bool {
    state.objects.try_get(object).is_some_and(|live| {
        let def = &CARD_DEFS[live.card_def as usize];
        def.has_type(CardType::Creature) && (def.changeling || def.subtypes.contains(&Subtype::Cat))
    })
}

fn other_cat_died(
    state: &GameState,
    event: &CommittedEvent,
    source: ObjectId,
    controller: PlayerId,
) -> bool {
    matches!(
        *event,
        CommittedEvent::ZoneChange {
            object,
            from: Zone::Battlefield,
            to: Zone::Graveyard,
            controller_before,
        } if object != source && controller_before == controller && was_cat_creature(state, object)
    )
}

pub(crate) fn trigger_matches(
    condition: E5bTriggerV1,
    events: &[CommittedEvent],
    index: usize,
    source: ObjectId,
    state: &GameState,
) -> bool {
    let controller = state.objects.get(source).controller;
    match condition {
        E5bTriggerV1::OtherCatsYouControlDieV1 => {
            other_cat_died(state, &events[index], source, controller)
                && !events[..index]
                    .iter()
                    .any(|event| other_cat_died(state, event, source, controller))
        }
        E5bTriggerV1::YourEndStepIfGainedLifeV1 => {
            matches!(
                events[index],
                CommittedEvent::BeginningEndStep { active_player, .. } if active_player == controller
            ) && gained_life_this_turn(state, controller)
        }
        E5bTriggerV1::EntersIfOpponentHasMoreLandsV1 => {
            matches!(
                events[index],
                CommittedEvent::ZoneChange {
                    object,
                    to: Zone::Battlefield,
                    ..
                } if object == source
            ) && opponent_has_more_lands(state, controller)
        }
        E5bTriggerV1::CastSelfIfKickedV1 => {
            own_cast_item(state, events, index, source).is_some_and(|item| item.kicked)
        }
        E5bTriggerV1::CastSelfWithAlternativeCostV1 => own_cast_item(state, events, index, source)
            .is_some_and(|item| {
                item.v4.cast_method == Some(crate::state::CastMethodV4::Alternative)
            }),
    }
}

// ---- Hooks: queued and delayed triggers ---------------------------------------------

fn queued_trigger(queued: QueuedTriggerV1) -> PendingTrigger {
    let target_spec = definition_target_spec(queued.source.card_def, &e5b(queued.op.clone()));
    let mut pending =
        crate::eternal_cards_v1::delayed_trigger(queued.source, queued.controller, e5b(queued.op));
    pending.target_spec = target_spec;
    pending
}

fn definition_target_spec(card_def: u16, effect: &EffectOp) -> TargetSpec {
    CARD_DEFS
        .get(usize::from(card_def))
        .and_then(|def| trigger_target_spec(def.name, effect))
        .unwrap_or(TargetSpec::None)
}

/// Spell casts this module remembers: Amped Raptor cast from hand, Phlage
/// cast with escape.
fn note_spell_casts(state: &mut GameState, events: &[CommittedEvent]) {
    for event in events {
        let CommittedEvent::SpellCast { spell, .. } = *event else {
            continue;
        };
        let Some(live) = state.objects.try_get(spell) else {
            continue;
        };
        let Some(origin) = live.v4.spell_cast_origin else {
            continue;
        };
        if live.spell_copy_origin.is_some() {
            continue;
        }
        let link = live_link(state, spell);
        match definition_name(state, spell) {
            Some(AMPED_RAPTOR) if origin.origin_zone == Zone::Hand => {
                let casts = &mut e5b_state(state).hand_casts;
                casts.retain(|cast| cast.object != spell);
                casts.push(link);
            }
            Some(PHLAGE) if origin.route == SpellCastRouteV4::GraveyardEscape => {
                let casts = &mut e5b_state(state).escaped_casts;
                casts.retain(|cast| cast.object != spell);
                casts.push(link);
            }
            _ => {}
        }
    }
}

/// Miracle (702.94a): a card with miracle drawn as its owner's first card
/// this turn may be revealed as it is drawn. The owner answers the reveal
/// before anything else happens (`pending_decision`).
fn note_miracle_draws(state: &mut GameState, events: &[CommittedEvent]) {
    let mut offers = Vec::new();
    for (index, event) in events.iter().enumerate() {
        let CommittedEvent::Draw {
            player,
            object: Some(card),
        } = *event
        else {
            continue;
        };
        if miracle_cost(state, card).is_none()
            || draw_ordinal_at(events, index, player, state) != 1
            || !state.players[player.index()].hand.contains(&card)
        {
            continue;
        }
        offers.push(live_link(state, card));
    }
    if !offers.is_empty() {
        e5b_state(state).miracle_offers.extend(offers);
    }
}

/// `player`'s draws this turn up to and including `events[index]`.
fn draw_ordinal_at(
    events: &[CommittedEvent],
    index: usize,
    player: PlayerId,
    state: &GameState,
) -> u32 {
    let is_draw = |event: &CommittedEvent| matches!(event, CommittedEvent::Draw { player: drawer, object: Some(_) } if *drawer == player);
    let in_batch = events.iter().filter(|event| is_draw(event)).count() as u32;
    let through = events[..=index]
        .iter()
        .filter(|event| is_draw(event))
        .count() as u32;
    state.players[player.index()]
        .draws_this_turn
        .saturating_sub(in_batch)
        .saturating_add(through)
}

/// Reality Smasher: "Whenever this creature becomes the target of a spell
/// an opponent controls, counter that spell unless its controller discards
/// a card."
fn note_smasher_targets(state: &mut GameState, events: &[CommittedEvent]) {
    for event in events {
        let CommittedEvent::Targeted {
            target,
            target_zone_change_count,
            targeting_stack_item,
            targeting_controller,
        } = *event
        else {
            continue;
        };
        if !active_named(state, target, REALITY_SMASHER) {
            continue;
        }
        let live = state.objects.get(target);
        let controller = live.controller;
        if live.zone_change_count != target_zone_change_count || targeting_controller == controller
        {
            continue;
        }
        let is_spell = state.stack.iter().any(|item| {
            item.v4.stack_item_id == targeting_stack_item && item.kind == StackItemKind::Spell
        });
        if !is_spell {
            continue;
        }
        let source = AbilitySourceContractV4::capture(state, target);
        queue_trigger(
            state,
            source,
            controller,
            E5bOpV1::SmasherV1 {
                spell: targeting_stack_item,
            },
        );
    }
}

/// Sand Scout: "Whenever one or more land cards are put into your
/// graveyard from anywhere, create a 1/1 red, green, and white Sand Warrior
/// creature token. This ability triggers only once each turn."
fn note_land_cards_to_graveyard(state: &mut GameState, events: &[CommittedEvent]) {
    let owners = events
        .iter()
        .filter_map(|event| match *event {
            CommittedEvent::ZoneChange {
                object,
                to: Zone::Graveyard,
                ..
            } => state.objects.try_get(object).and_then(|live| {
                let def = &CARD_DEFS[live.card_def as usize];
                (def.has_type(CardType::Land)
                    && !live.v4.is_token
                    && !def.is_token
                    && live.zone == Zone::Graveyard)
                    .then_some(live.owner)
            }),
            _ => None,
        })
        .collect::<Vec<_>>();
    if owners.is_empty() {
        return;
    }
    let turn = state.turn_key_v1();
    let scouts = active_permanents(state, SAND_SCOUT, None)
        .filter(|&scout| owners.contains(&state.objects.get(scout).controller))
        .collect::<Vec<_>>();
    for scout in scouts {
        let link = live_link(state, scout);
        if read_state(state).is_some_and(|e5b| {
            e5b.sand_scout_turns
                .iter()
                .any(|(used, used_turn)| *used == link && *used_turn == turn)
        }) {
            continue;
        }
        let controller = state.objects.get(scout).controller;
        let source = AbilitySourceContractV4::capture(state, scout);
        let e5b = e5b_state(state);
        e5b.sand_scout_turns
            .retain(|(_, used_turn)| *used_turn == turn);
        e5b.sand_scout_turns.push((link, turn));
        e5b.queued_triggers.push(QueuedTriggerV1 {
            source,
            controller,
            op: E5bOpV1::CreateSandWarriorV1,
        });
    }
}

/// Ascend (702.131): a player who controls ten or more permanents while
/// they control an Ocelot Pride gets the city's blessing for the rest of
/// the game.
fn note_city_blessing(state: &mut GameState) {
    for player in [PlayerId::P0, PlayerId::P1] {
        if read_state(state).is_some_and(|e5b| e5b.city_blessing[player.index()]) {
            continue;
        }
        if active_permanents(state, OCELOT_PRIDE, Some(player))
            .next()
            .is_some()
            && controlled_permanents(state, player).len() >= 10
        {
            e5b_state(state).city_blessing[player.index()] = true;
        }
    }
}

fn has_city_blessing(state: &GameState, player: PlayerId) -> bool {
    read_state(state).is_some_and(|e5b| e5b.city_blessing[player.index()])
}

/// Phelia's and mobilize's "at the beginning of the next end step" delayed
/// triggers.
fn end_step_triggers(state: &mut GameState, events: &[CommittedEvent]) {
    if !events
        .iter()
        .any(|event| matches!(event, CommittedEvent::BeginningEndStep { .. }))
    {
        return;
    }
    let Some(eternal) = state.eternal_v1.as_mut() else {
        return;
    };
    let returns = std::mem::take(&mut eternal.e5b.phelia_returns);
    let mobilized = std::mem::take(&mut eternal.e5b.mobilized);
    for record in returns {
        queue_trigger(
            state,
            record.source,
            record.controller,
            E5bOpV1::PheliaReturnV1 { card: record.card },
        );
    }
    for record in mobilized {
        queue_trigger(
            state,
            record.source,
            record.controller,
            E5bOpV1::SacrificeMobilizedV1 {
                tokens: record.tokens,
            },
        );
    }
}

/// The triggers this collection puts on the stack that no printed trigger
/// condition matches.
pub(crate) fn delayed_triggers(
    state: &mut GameState,
    events: &[CommittedEvent],
) -> Vec<PendingTrigger> {
    if !cfg!(feature = "eternal-fixtures") {
        return Vec::new();
    }
    note_spell_casts(state, events);
    note_miracle_draws(state, events);
    note_smasher_targets(state, events);
    note_land_cards_to_graveyard(state, events);
    note_city_blessing(state);
    end_step_triggers(state, events);
    let Some(eternal) = state.eternal_v1.as_mut() else {
        return Vec::new();
    };
    if eternal.e5b.queued_triggers.is_empty() {
        return Vec::new();
    }
    std::mem::take(&mut eternal.e5b.queued_triggers)
        .into_iter()
        .map(queued_trigger)
        .collect()
}

/// A main phase began for the active player: Static Prison's "At the
/// beginning of your first main phase, sacrifice this enchantment unless
/// you pay {E}."
pub(crate) fn begin_main_phase(state: &mut GameState) -> Vec<PendingTrigger> {
    if state.step != Step::Main1 {
        return Vec::new();
    }
    let player = state.active_player;
    let prisons = active_permanents(state, STATIC_PRISON, Some(player)).collect::<Vec<_>>();
    prisons
        .into_iter()
        .map(|prison| {
            let source = AbilitySourceContractV4::capture(state, prison);
            crate::eternal_cards_v1::delayed_trigger(
                source,
                player,
                e5b(E5bOpV1::StaticPrisonUpkeepV1),
            )
        })
        .collect()
}

/// The beginning of `active`'s turn ends this module's turn-bound records.
pub(crate) fn begin_turn(state: &mut GameState, _active: PlayerId) {
    let turn = state.turn_key_v1();
    if let Some(eternal) = state.eternal_v1.as_mut() {
        eternal
            .e5b
            .sand_scout_turns
            .retain(|(_, used_turn)| *used_turn == turn);
    }
}

// ---- Hooks: miracle reveal ----------------------------------------------------------

/// A card's miracle cost (702.94), if it has one: Terminus, Miracle {W}.
fn miracle_cost(state: &GameState, card: ObjectId) -> Option<&'static [ManaColor]> {
    (definition_name(state, card)? == TERMINUS).then_some(&[ManaColor::W][..])
}

/// The first live miracle offer, dropping offers whose card left its hand.
fn live_miracle_offer(state: &GameState) -> Option<ObjectLinkV4> {
    read_state(state)?
        .miracle_offers
        .iter()
        .copied()
        .find(|&offer| link_live_in(state, offer, Zone::Hand))
}

/// The pending miracle reveal, as a decision for the card's owner: option
/// 0 keeps the card hidden, option 1 reveals it.
pub(crate) fn pending_decision(state: &GameState) -> Option<crate::engine::Decision> {
    let offer = live_miracle_offer(state)?;
    Some(crate::engine::Decision::ChooseEffectOption {
        player: state.objects.get(offer.object).owner,
        source: offer.object,
        option_count: 2,
    })
}

/// Answers the pending miracle reveal. `Ok(false)` when none is pending.
pub(crate) fn answer_pending_decision(state: &mut GameState, option: u16) -> Result<bool, String> {
    let Some(offer) = live_miracle_offer(state) else {
        if let Some(eternal) = state.eternal_v1.as_mut() {
            eternal.e5b.miracle_offers.clear();
        }
        return Ok(false);
    };
    if option > 1 {
        return Err("a miracle reveal has two options".to_string());
    }
    let player = state.objects.get(offer.object).owner;
    let remaining = read_state(state)
        .map(|e5b| {
            e5b.miracle_offers
                .iter()
                .copied()
                .filter(|&other| other != offer && link_live_in(state, other, Zone::Hand))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    e5b_state(state).miracle_offers = remaining;
    if option == 1 {
        // spellbench-public-reveal: hand card revealed by the card rules.
        state.reveal_hand_card(player.opponent(), player, offer.object)?;
        // The reveal is answered outside any event batch, so its trigger
        // waits for the next placement directly.
        let source = AbilitySourceContractV4::capture(state, offer.object);
        let trigger = queued_trigger(QueuedTriggerV1 {
            source,
            controller: player,
            op: E5bOpV1::MiracleV1,
        });
        state.engine.pending_triggers.push(trigger);
    }
    Ok(true)
}

// ---- Hooks: statics -------------------------------------------------------------------

/// Voice of Victory: "Your opponents can't cast spells during your turn."
pub(crate) fn cast_prohibited(state: &GameState, player: PlayerId) -> bool {
    let opponent = player.opponent();
    state.active_player == opponent
        && active_permanents(state, VOICE_OF_VICTORY, Some(opponent))
            .next()
            .is_some()
}

/// The generic reduction `player` gets for casting `source`: Eye of Ugin's
/// "Colorless Eldrazi spells you cast cost {2} less to cast.", It That
/// Heralds the End's "Colorless spells you cast with mana value 7 or
/// greater cost {1} less to cast." and Leyline Binding's domain.
pub(crate) fn spell_cost_reduction(
    state: &GameState,
    definition: &CardDef,
    player: PlayerId,
) -> u32 {
    // Painter's Servant colors every spell (wave E9a).
    let colorless = definition.colors.is_empty()
        && crate::eternal_legacy_e9a_v1::statics::painter_mask(state) == 0;
    let mut reduction = 0;
    if colorless && definition.subtypes.contains(&Subtype::Eldrazi) {
        reduction += 2 * active_permanents(state, EYE_OF_UGIN, Some(player)).count() as u32;
    }
    if colorless && definition.mana_value >= 7 {
        reduction += active_permanents(state, IT_THAT_HERALDS, Some(player)).count() as u32;
    }
    if definition.name == LEYLINE_BINDING {
        reduction += domain(state, player);
    }
    reduction
}

/// Domain: the number of basic land types among lands `player` controls.
fn domain(state: &GameState, player: PlayerId) -> u32 {
    let lands = controlled_permanents(state, player)
        .into_iter()
        .filter(|&object| crate::engine::object_has_type(state, object, CardType::Land))
        .collect::<Vec<_>>();
    [
        Subtype::Plains,
        Subtype::Island,
        Subtype::Swamp,
        Subtype::Mountain,
        Subtype::Forest,
    ]
    .into_iter()
    .filter(|&subtype| {
        lands
            .iter()
            .any(|&land| crate::engine::has_effective_subtype(state, land, subtype))
    })
    .count() as u32
}

/// It That Heralds the End: "Other colorless creatures you control get
/// +1/+1." The power and toughness bonus for `object`.
pub(crate) fn controlled_boost(state: &GameState, object: ObjectId) -> (i128, i128) {
    let Some(live) = state.objects.try_get(object) else {
        return (0, 0);
    };
    if live.zone != Zone::Battlefield
        || !crate::engine::object_has_type(state, object, CardType::Creature)
        || !is_colorless(state, object)
    {
        return (0, 0);
    }
    let lords = active_permanents(state, IT_THAT_HERALDS, Some(live.controller))
        .filter(|&lord| lord != object)
        .count() as i128;
    (lords, lords)
}

/// Petrified Hamlet: "Activated abilities of sources with the chosen name
/// can't be activated unless they're mana abilities." Whether a non-mana
/// ability of `source` may be activated.
pub(crate) fn activation_allowed(state: &GameState, source: ObjectId) -> bool {
    let Some(e5b) = read_state(state) else {
        return true;
    };
    if e5b.hamlet_names.is_empty() {
        return true;
    }
    let name = crate::engine::effective_name(state, source);
    !e5b.hamlet_names.iter().any(|(hamlet, named)| {
        link_live_in(state, *hamlet, Zone::Battlefield)
            && active_named(state, hamlet.object, PETRIFIED_HAMLET)
            && CARD_DEFS[usize::from(*named)].object_name == name
    })
}

/// Petrified Hamlet: 'Lands with the chosen name have "{T}: Add {C}."'
pub(crate) fn granted_mana_colors(state: &GameState, object: ObjectId) -> Vec<ManaColor> {
    let Some(e5b) = read_state(state) else {
        return Vec::new();
    };
    if e5b.hamlet_names.is_empty()
        || !state
            .objects
            .try_get(object)
            .is_some_and(|live| live.zone == Zone::Battlefield)
        || !crate::engine::object_has_type(state, object, CardType::Land)
    {
        return Vec::new();
    }
    // A land whose own mana abilities already add {C} keeps them; the
    // granted ability adds nothing they lack.
    let def = printed(state, object);
    if def.mana_ability_def.is_some()
        || def
            .primary_mana_ability_choices(state.objects.get(object).v4.chosen_color)
            .contains(&ManaColor::C)
    {
        return Vec::new();
    }
    let name = crate::engine::effective_name(state, object);
    let granted = e5b.hamlet_names.iter().any(|(hamlet, named)| {
        link_live_in(state, *hamlet, Zone::Battlefield)
            && active_named(state, hamlet.object, PETRIFIED_HAMLET)
            && CARD_DEFS[usize::from(*named)].object_name == name
    });
    if granted {
        vec![ManaColor::C]
    } else {
        Vec::new()
    }
}

/// The land card names a Petrified Hamlet may choose: every land
/// definition's name, one per name, in definition order.
pub fn hamlet_name_options() -> &'static [u16] {
    static OPTIONS: std::sync::OnceLock<Vec<u16>> = std::sync::OnceLock::new();
    OPTIONS.get_or_init(|| {
        let mut names: Vec<&str> = Vec::new();
        let mut options = Vec::new();
        for (index, def) in CARD_DEFS.iter().enumerate() {
            if !def.is_land
                || def.is_token
                || def.object_name.is_empty()
                || !def.has_full_support()
                || names.contains(&def.object_name)
            {
                continue;
            }
            names.push(def.object_name);
            options.push(u16::try_from(index).expect("card ids fit u16"));
        }
        options
    })
}

/// The definition whose name a Petrified Hamlet chose.
pub fn hamlet_name(state: &GameState, hamlet: ObjectId) -> Option<u16> {
    let link = live_link(state, hamlet);
    read_state(state)?
        .hamlet_names
        .iter()
        .find(|(host, _)| *host == link)
        .map(|(_, named)| *named)
}

/// Quantum Riddler: "As long as you have one or fewer cards in hand, if you
/// would draw one or more cards, you draw that many cards plus one
/// instead." The extra draws one draw event gets, one per Riddler.
pub(crate) fn riddler_extra_draws(state: &GameState, player: PlayerId) -> usize {
    static RIDDLER: std::sync::OnceLock<Option<u16>> = std::sync::OnceLock::new();
    let Some(riddler) = *RIDDLER.get_or_init(|| card_id_by_name(QUANTUM_RIDDLER)) else {
        return 0;
    };
    if state.players[player.index()].hand.len() > 1 {
        return 0;
    }
    state.players[player.index()]
        .battlefield
        .iter()
        .chain(state.players[player.opponent().index()].battlefield.iter())
        .filter(|&&object| {
            state.objects.get(object).card_def == riddler
                && state.objects.get(object).controller == player
                && active_named(state, object, QUANTUM_RIDDLER)
        })
        .count()
}

/// Eldrazi Temple: "{T}: Add {C}{C}. Spend this mana only to cast colorless
/// Eldrazi spells or activate abilities of colorless Eldrazi." While the
/// payment planner pays for a colorless Eldrazi spell, a Temple's tap
/// yields two.
pub(crate) fn eldrazi_spell_yield(
    state: &GameState,
    source: ObjectId,
    colorless_eldrazi_spell: bool,
) -> Option<u8> {
    (colorless_eldrazi_spell && definition_name(state, source) == Some(ELDRAZI_TEMPLE)).then_some(2)
}

/// Eldrazi Temple's mana that a colorless Eldrazi spell's payment left
/// over stays restricted: up to one unit per Temple the plan tapped becomes
/// a restricted unit instead of floating freely.
pub(crate) fn restrict_temple_surplus(
    state: &mut GameState,
    player: PlayerId,
    plan: &crate::mana::PaymentPlan,
) {
    let colorless = ManaColor::C.pool_index();
    let leftover = plan.surplus[colorless].saturating_sub(plan.pool_used[colorless]);
    if leftover == 0 {
        return;
    }
    let temples = plan
        .taps
        .iter()
        .filter(|(source, _)| definition_name(state, *source) == Some(ELDRAZI_TEMPLE))
        .map(|(source, _)| *source)
        .collect::<Vec<_>>();
    for temple in temples.into_iter().take(usize::from(leftover)) {
        let seat = &mut state.players[player.index()];
        if seat.mana_pool[colorless] == 0 {
            break;
        }
        seat.mana_pool[colorless] -= 1;
        let unit = crate::mana::RestrictedManaUnitV1 {
            color: ManaColor::C,
            restriction: crate::card_def::ManaSpendRestrictionDef::ColorlessEldraziSpellV1,
            source: live_link(state, temple),
            source_card_def: state.objects.get(temple).card_def,
        };
        state.players[player.index()]
            .restricted_mana_pool
            .0
            .push(unit);
    }
}

// ---- Resolution choices ---------------------------------------------------------------

/// What one of this module's E3 resolution choices decides, carried as
/// `E3ChoiceKindV1::E5bV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum E5bChoiceV1 {
    /// Reality Smasher: the targeting spell's controller may discard a card
    /// from their hand; otherwise `spell` is countered.
    SmasherDiscardV1 { spell: StackItemId },
    /// Terminus: the order of the chooser's creatures on the bottom of
    /// their library, the first chosen highest. `later` are the owners
    /// still to choose.
    TerminusOrderV1 { later: Vec<PlayerId> },
    /// Ajani, Nacatl Avenger's -4: the permanent the opponent keeps for
    /// card type `stage` (artifact, creature, enchantment, planeswalker),
    /// with the ones kept so far.
    AjaniKeepV1 {
        stage: u8,
        kept: Vec<EffectObjectBinding>,
    },
    /// The card a resolution casts after its alternative cost was paid
    /// (miracle, Amped Raptor).
    PaidCastV1,
}

impl E5bChoiceV1 {
    /// Whether the candidates are hidden from the other player.
    pub(crate) fn chooser_private(&self) -> bool {
        matches!(self, Self::SmasherDiscardV1 { .. })
    }

    pub(crate) fn public_purpose(&self) -> crate::rl::TargetSelectionPurposeV4 {
        match self {
            Self::SmasherDiscardV1 { .. } | Self::PaidCastV1 => {
                crate::rl::TargetSelectionPurposeV4::CardSelection
            }
            Self::TerminusOrderV1 { .. } | Self::AjaniKeepV1 { .. } => {
                crate::rl::TargetSelectionPurposeV4::PermanentSelection
            }
        }
    }

    /// Object incarnations this choice refers to beyond its candidates.
    pub(crate) fn referenced_bindings(&self) -> Vec<EffectObjectBinding> {
        match self {
            Self::AjaniKeepV1 { kept, .. } => kept.clone(),
            _ => Vec::new(),
        }
    }
}

fn targets_of(bindings: &[EffectObjectBinding]) -> Vec<Target> {
    bindings
        .iter()
        .map(|binding| Target::Object(binding.object))
        .collect()
}

/// One of this module's selections and its shape.
struct OfferV1 {
    kind: E5bChoiceV1,
    player: PlayerId,
    bindings: Vec<EffectObjectBinding>,
    min: u16,
    max: u16,
    ordered: bool,
}

fn offer(continuation: &mut EffectContinuation, offer: OfferV1, path: Vec<u16>) {
    let remaining = continuation.frames.clone();
    crate::eternal_cards_v1::e3::offer(
        continuation,
        E3ChoiceV1 {
            kind: E3ChoiceKindV1::E5bV1(offer.kind),
            player: offer.player,
            candidates: targets_of(&offer.bindings),
            bindings: offer.bindings,
            min: offer.min,
            max: offer.max,
            ordered: offer.ordered,
            path,
            remaining,
        },
    );
}

/// The card a paid resolution cast answer casts, if any.
pub(crate) fn resolution_cast_card(
    choice: &E3ChoiceV1,
    selected: &[Target],
) -> Option<EffectObjectBinding> {
    let E3ChoiceKindV1::E5bV1(E5bChoiceV1::PaidCastV1) = choice.kind else {
        return None;
    };
    let &Target::Object(object) = selected.first()? else {
        return None;
    };
    choice
        .bindings
        .iter()
        .copied()
        .find(|binding| binding.object == object)
}

/// Authenticates the cast a paid resolution cast answer authorized.
pub(crate) fn validate_resolution_cast(
    suspended: &EffectContinuation,
    choice: &E3ChoiceV1,
    selected: &[Target],
    permission: crate::engine::resolution_cast_v1::ResolutionPermissionV1,
) -> Result<(), String> {
    if resolution_cast_card(choice, selected) != Some(permission.card)
        || selected.len() != 1
        || !matches!(permission.card.expected_zone, Zone::Hand | Zone::Exile)
        || permission.land
        || permission.maximum_mana_value.is_some()
        || permission.controller != choice.player
        || suspended.ctx.controller != choice.player
        || suspended.frames != choice.remaining
    {
        return Err("the paid resolution cast answer does not authorize this cast".to_string());
    }
    Ok(())
}

/// Applies an answered choice. `Ok(true)` when it staged a further choice.
pub(crate) fn finish_selection(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    choice: &E3ChoiceV1,
    kind: &E5bChoiceV1,
    bindings: Vec<EffectObjectBinding>,
) -> Result<bool, String> {
    match kind {
        E5bChoiceV1::SmasherDiscardV1 { spell } => {
            if let Some(card) = bindings.first() {
                crate::engine::commit_discarded_card(state, card.object);
            } else {
                counter_spell_item(state, *spell, continuation.ctx.source);
            }
            Ok(false)
        }
        E5bChoiceV1::TerminusOrderV1 { later } => {
            let order = bindings
                .iter()
                .map(|binding| binding.object)
                .collect::<Vec<_>>();
            put_on_bottom(state, &order);
            stage_terminus(state, continuation, later, choice.path.clone())
        }
        E5bChoiceV1::AjaniKeepV1 { stage, kept } => {
            let mut kept = kept.clone();
            kept.extend(bindings.first().copied());
            stage_ajani_keep(
                state,
                continuation,
                choice.player,
                stage + 1,
                kept,
                choice.path.clone(),
            )
        }
        // A cast answer is staged by the interpreter before it gets here.
        E5bChoiceV1::PaidCastV1 => Ok(false),
    }
}

/// Counters the spell with stack id `item`, if it is still on the stack.
fn counter_spell_item(state: &mut GameState, item: StackItemId, source: ObjectId) {
    let present = state
        .stack
        .iter()
        .any(|entry| entry.v4.stack_item_id == item && entry.kind == StackItemKind::Spell);
    if present && crate::engine::counter_stack_item_by_id(state, item).is_err() {
        halt(state, source);
    }
}

/// Puts `objects` on the bottom of their owners' libraries, the first
/// highest. Their order is public: they were on the battlefield.
fn put_on_bottom(state: &mut GameState, objects: &[ObjectId]) {
    for &object in objects {
        if state.objects.get(object).zone == Zone::Battlefield {
            event::propose_and_commit(
                state,
                ProposedEvent::public_library_insert(object, event::LibraryPlacement::Bottom),
            );
        }
    }
}

/// Terminus: each owner, in turn order, orders their creatures on the
/// bottom of their library.
fn stage_terminus(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    players: &[PlayerId],
    path: Vec<u16>,
) -> Result<bool, String> {
    for (index, &player) in players.iter().enumerate() {
        let creatures = [PlayerId::P0, PlayerId::P1]
            .into_iter()
            .flat_map(|seat| state.players[seat.index()].battlefield.iter().copied())
            .filter(|&object| {
                state.objects.get(object).owner == player
                    && crate::engine::object_has_type(state, object, CardType::Creature)
            })
            .collect::<Vec<_>>();
        if creatures.len() <= 1 {
            put_on_bottom(state, &creatures);
            continue;
        }
        let bindings = creatures
            .iter()
            .map(|&object| bind(state, object))
            .collect::<Vec<_>>();
        let count = u16::try_from(bindings.len()).unwrap_or(u16::MAX);
        offer(
            continuation,
            OfferV1 {
                kind: E5bChoiceV1::TerminusOrderV1 {
                    later: players[index + 1..].to_vec(),
                },
                player,
                bindings,
                min: count,
                max: count,
                ordered: true,
            },
            path,
        );
        return Ok(true);
    }
    Ok(false)
}

const AJANI_KEPT_TYPES: [CardType; 4] = [
    CardType::Artifact,
    CardType::Creature,
    CardType::Enchantment,
    CardType::Planeswalker,
];

/// Ajani, Nacatl Avenger's -4: the opponent picks one nonland permanent of
/// each card type in turn (one permanent may answer several types), then
/// sacrifices the rest of their nonland permanents.
fn stage_ajani_keep(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    opponent: PlayerId,
    stage: u8,
    kept: Vec<EffectObjectBinding>,
    path: Vec<u16>,
) -> Result<bool, String> {
    let nonland = controlled_permanents(state, opponent)
        .into_iter()
        .filter(|&object| !crate::engine::object_has_type(state, object, CardType::Land))
        .collect::<Vec<_>>();
    for next in stage..4 {
        let card_type = AJANI_KEPT_TYPES[usize::from(next)];
        let candidates = nonland
            .iter()
            .copied()
            .filter(|&object| crate::engine::object_has_type(state, object, card_type))
            .map(|object| bind(state, object))
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            continue;
        }
        offer(
            continuation,
            OfferV1 {
                kind: E5bChoiceV1::AjaniKeepV1 { stage: next, kept },
                player: opponent,
                bindings: candidates,
                min: 1,
                max: 1,
                ordered: false,
            },
            path,
        );
        return Ok(true);
    }
    let doomed = nonland
        .into_iter()
        .filter(|&object| !kept.iter().any(|binding| binding.object == object))
        .collect::<Vec<_>>();
    sacrifice_all(state, &doomed);
    Ok(false)
}

fn sacrifice_all(state: &mut GameState, objects: &[ObjectId]) {
    let objects = objects
        .iter()
        .copied()
        .filter(|&object| state.objects.get(object).zone == Zone::Battlefield)
        .collect::<Vec<_>>();
    if objects.is_empty() {
        return;
    }
    for &object in &objects {
        event::log_sacrifice(state, object);
    }
    event::propose_and_commit_batch(
        state,
        objects
            .iter()
            .map(|&object| ProposedEvent::zone_change(object, Zone::Graveyard))
            .collect(),
    );
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

/// Offers casting `card` during this resolution, its cost already paid.
/// Rebound's free cast (`eternal_pauper_e6a_v1`) uses it too.
pub(crate) fn offer_paid_cast(
    state: &GameState,
    continuation: &mut EffectContinuation,
    card: EffectObjectBinding,
    path: Vec<u16>,
) -> bool {
    let player = continuation.ctx.controller;
    if !crate::engine::resolution_cast_v1::can_cast_exiled_without_mana(state, player, card, None) {
        return false;
    }
    offer(
        continuation,
        OfferV1 {
            kind: E5bChoiceV1::PaidCastV1,
            player,
            bindings: vec![card],
            min: 1,
            max: 1,
            ordered: false,
        },
        path,
    );
    true
}

/// The hand incarnation a miracle trigger revealed, if still in its hand.
fn miracle_card(ctx: &ExecCtx, state: &GameState) -> Option<EffectObjectBinding> {
    let contract = ctx.ability_source_contract?;
    let card = ObjectLinkV4 {
        object: contract.source,
        zone_change_count: contract.zone_change_count,
    };
    (contract.zone == Zone::Hand
        && link_live_in(state, card, Zone::Hand)
        && state.players[ctx.controller.index()]
            .hand
            .contains(&card.object))
    .then(|| bind(state, card.object))
}

fn energy_options(available: u32, op: fn(u32) -> E5bOpV1) -> Vec<EffectOp> {
    (0..=available.min(255))
        .map(|amount| e5b(op(amount)))
        .collect()
}

fn pay_energy_damage(amount: u32) -> E5bOpV1 {
    E5bOpV1::PayEnergyDamageTargetV1 { amount }
}

fn pay_energy_destroy(amount: u32) -> E5bOpV1 {
    E5bOpV1::PayEnergyDestroyV1 { amount }
}

/// Runs a leaf inside the resumable interpreter. `Ok(true)` when it staged a
/// choice and the continuation must suspend.
pub(crate) fn drive(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    op: &E5bOpV1,
    path: Vec<u16>,
) -> Result<bool, String> {
    let controller = continuation.ctx.controller;
    match op {
        E5bOpV1::GalvanicDischargeV1 => {
            crate::effect::execute(
                &eternal(crate::eternal_cards_v1::EternalOpV1::GainEnergy { amount: 3 }),
                &continuation.ctx,
                state,
            );
            let available = crate::eternal_cards_v1::energy(state, controller);
            push_choice(
                continuation,
                energy_options(available, pay_energy_damage),
                path,
            );
            Ok(false)
        }
        E5bOpV1::WrathOfTheSkiesV1 => {
            let gained = u8::try_from(continuation.ctx.x_value).unwrap_or(u8::MAX);
            crate::effect::execute(
                &eternal(crate::eternal_cards_v1::EternalOpV1::GainEnergy { amount: gained }),
                &continuation.ctx,
                state,
            );
            let available = crate::eternal_cards_v1::energy(state, controller);
            push_choice(
                continuation,
                energy_options(available, pay_energy_destroy),
                path,
            );
            Ok(false)
        }
        E5bOpV1::TerminusV1 => {
            let first = state.active_player;
            stage_terminus(state, continuation, &[first, first.opponent()], path)
        }
        E5bOpV1::MiracleV1 => {
            let Some(card) = miracle_card(&continuation.ctx, state) else {
                return Ok(false);
            };
            let Some(cost) = miracle_cost(state, card.object) else {
                return Ok(false);
            };
            if crate::engine::can_pay_effect_mana(controller, cost, 0, state)
                && crate::engine::resolution_cast_v1::can_cast_exiled_without_mana(
                    state, controller, card, None,
                )
            {
                push_choice(
                    continuation,
                    vec![e5b(E5bOpV1::MiracleCastV1), nothing()],
                    path,
                );
            }
            Ok(false)
        }
        E5bOpV1::MiracleCastV1 => {
            let Some(card) = miracle_card(&continuation.ctx, state) else {
                return Ok(false);
            };
            let Some(cost) = miracle_cost(state, card.object) else {
                return Ok(false);
            };
            if !crate::engine::pay_effect_mana(controller, cost, 0, state) {
                return Ok(false);
            }
            Ok(offer_paid_cast(state, continuation, card, path))
        }
        E5bOpV1::AmpedRaptorV1 => {
            crate::effect::execute(
                &eternal(crate::eternal_cards_v1::EternalOpV1::GainEnergy { amount: 2 }),
                &continuation.ctx,
                state,
            );
            let Some(card) = raptor_exile(state, &continuation.ctx) else {
                return Ok(false);
            };
            let cost = u32::from(crate::engine::object_mana_value(state, card.object));
            if crate::eternal_cards_v1::energy(state, controller) >= cost
                && crate::engine::resolution_cast_v1::can_cast_exiled_without_mana(
                    state, controller, card, None,
                )
            {
                let card = ObjectLinkV4 {
                    object: card.object,
                    zone_change_count: card.expected_zone_change_count,
                };
                push_choice(
                    continuation,
                    vec![e5b(E5bOpV1::RaptorCastV1 { card }), nothing()],
                    path,
                );
            }
            Ok(false)
        }
        E5bOpV1::RaptorCastV1 { card } => {
            if !link_live_in(state, *card, Zone::Exile) {
                return Ok(false);
            }
            let cost = crate::engine::object_mana_value(state, card.object);
            let Ok(cost) = u8::try_from(cost) else {
                return Ok(false);
            };
            if crate::eternal_cards_v1::pay_energy(state, controller, cost).is_none() {
                return Ok(false);
            }
            let binding = bind(state, card.object);
            Ok(offer_paid_cast(state, continuation, binding, path))
        }
        E5bOpV1::SmasherV1 { spell } => {
            let Some(item) = state
                .stack
                .iter()
                .find(|item| item.v4.stack_item_id == *spell && item.kind == StackItemKind::Spell)
            else {
                return Ok(false);
            };
            let payer = item.controller;
            let hand = state.players[payer.index()]
                .hand
                .iter()
                .map(|&card| bind(state, card))
                .collect::<Vec<_>>();
            if hand.is_empty() {
                counter_spell_item(state, *spell, continuation.ctx.source);
                return Ok(false);
            }
            offer(
                continuation,
                OfferV1 {
                    kind: E5bChoiceV1::SmasherDiscardV1 { spell: *spell },
                    player: payer,
                    bindings: hand,
                    min: 0,
                    max: 1,
                    ordered: false,
                },
                path,
            );
            Ok(true)
        }
        E5bOpV1::AjaniMayTransformV1 => {
            push_choice(
                continuation,
                vec![
                    eternal(crate::eternal_cards_v1::EternalOpV1::ExileThenReturnTransformedV1),
                    nothing(),
                ],
                path,
            );
            Ok(false)
        }
        E5bOpV1::AjaniSacrificeRestV1 => stage_ajani_keep(
            state,
            continuation,
            controller.opponent(),
            0,
            Vec::new(),
            path,
        ),
        E5bOpV1::GuideMayPayV1 => {
            if crate::eternal_cards_v1::energy(state, controller) >= 3 {
                push_choice(
                    continuation,
                    vec![e5b(E5bOpV1::GuidePayV1), nothing()],
                    path,
                );
            }
            Ok(false)
        }
        E5bOpV1::StaticPrisonUpkeepV1 => {
            if crate::eternal_cards_v1::energy(state, controller) >= 1 {
                push_choice(
                    continuation,
                    vec![e5b(E5bOpV1::PayOneEnergyV1), sacrifice_source()],
                    path,
                );
            } else {
                continuation.frames.push(EffectFrame::Program {
                    op: sacrifice_source(),
                    path,
                });
            }
            Ok(false)
        }
        E5bOpV1::HamletChooseNameV1 => {
            if live_ability_source(&continuation.ctx, state).is_some() {
                let options = hamlet_name_options()
                    .iter()
                    .map(|&card_def| e5b(E5bOpV1::HamletNameV1 { card_def }))
                    .collect();
                push_choice(continuation, options, path);
            }
            Ok(false)
        }
        E5bOpV1::SandScoutSearchV1 => {
            if opponent_has_more_lands(state, controller) {
                continuation.frames.push(EffectFrame::Program {
                    op: EffectOp::SearchLibraryCardsToDestination {
                        player: PlayerRef::Controller,
                        filter: crate::effect::LibraryCardFilter::CardWithAnySubtypeV1([
                            Subtype::Desert,
                            Subtype::Desert,
                        ]),
                        max_targets: 1,
                        destination: crate::effect::LibrarySearchDestinationV1::Battlefield {
                            tapped: true,
                        },
                    },
                    path,
                });
            }
            Ok(false)
        }
        other => {
            execute(other, &continuation.ctx, state);
            Ok(false)
        }
    }
}

/// Amped Raptor: "Then if you cast it from your hand, exile cards from the
/// top of your library until you exile a nonland card." The nonland card
/// exiled, if the condition held and one was found.
fn raptor_exile(state: &mut GameState, ctx: &ExecCtx) -> Option<EffectObjectBinding> {
    let contract = ctx.ability_source_contract?;
    let stack = ObjectLinkV4 {
        object: contract.source,
        zone_change_count: contract.zone_change_count.checked_sub(1)?,
    };
    let cast_from_hand = read_state(state).is_some_and(|e5b| e5b.hand_casts.contains(&stack));
    if let Some(eternal) = state.eternal_v1.as_mut() {
        eternal
            .e5b
            .hand_casts
            .retain(|cast| cast.object != contract.source);
    }
    if !cast_from_hand {
        return None;
    }
    let player = ctx.controller;
    while let Some(&top) = state.players[player.index()].library.first() {
        event::propose_and_commit(state, ProposedEvent::zone_change(top, Zone::Exile));
        let live = state.objects.get(top);
        if live.zone != Zone::Exile {
            return None;
        }
        if !CARD_DEFS[live.card_def as usize].has_type(CardType::Land) {
            return Some(bind(state, top));
        }
    }
    None
}

/// Whether `player` controls a red permanent other than `source`.
fn controls_other_red_permanent(state: &GameState, player: PlayerId, source: ObjectId) -> bool {
    let red = crate::card_def::mana_colors_mask(&[ManaColor::R]);
    controlled_permanents(state, player)
        .into_iter()
        .any(|object| {
            object != source
                && (state.objects.get(object).v4.effective_color_mask
                    | crate::eternal_legacy_e9a_v1::statics::painter_mask(state))
                    & red
                    != 0
        })
}

/// The creatures `player` controls.
fn creature_count(state: &GameState, player: PlayerId) -> usize {
    controlled_permanents(state, player)
        .into_iter()
        .filter(|&object| crate::engine::object_has_type(state, object, CardType::Creature))
        .count()
}

fn add_plus_one_counters(state: &mut GameState, ctx: &ExecCtx, object: ObjectId, count: u8) {
    let binding = EffectObjectBinding {
        object,
        expected_zone: Zone::Battlefield,
        expected_zone_change_count: state.objects.get(object).zone_change_count,
    };
    for _ in 0..count {
        crate::effect::execute(
            &EffectOp::PutPlusOnePlusOneCounterOnBoundObject { object: binding },
            ctx,
            state,
        );
    }
}

/// Exile `object`, then return it to the battlefield tapped under its
/// owner's control (Eldrazi Confluence).
fn flicker_tapped(state: &mut GameState, object: ObjectId) {
    event::propose_and_commit(state, ProposedEvent::zone_change(object, Zone::Exile));
    let live = state.objects.get(object);
    if live.zone != Zone::Exile || live.v4.is_token {
        return;
    }
    let owner = live.owner;
    let mut proposal = ProposedEvent::zone_change_to_battlefield_under_controller(object, owner);
    if let ProposedEvent::ZoneChange(change) = &mut proposal {
        change.force_battlefield_tapped = true;
    }
    event::propose_and_commit(state, proposal);
}

fn counter_target_ability(ctx: &ExecCtx, state: &mut GameState) {
    let Some(Target::StackItem(item)) = ctx.targets.first().copied() else {
        return;
    };
    if !ctx.target_incarnation_matches(0, state) {
        return;
    }
    let is_ability = state.stack.iter().any(|entry| {
        entry.v4.stack_item_id == item
            && matches!(
                entry.kind,
                StackItemKind::ActivatedAbility | StackItemKind::TriggeredAbility
            )
    });
    if is_ability && crate::engine::counter_stack_item_by_id(state, item).is_err() {
        halt(state, ctx.source);
    }
}

fn counter_triggered_ability_or_colorless_spell(ctx: &ExecCtx, state: &mut GameState) {
    if !ctx.target_incarnation_matches(0, state) {
        return;
    }
    let item = match ctx.targets.first().copied() {
        Some(Target::StackItem(item)) => state
            .stack
            .iter()
            .find(|entry| {
                entry.v4.stack_item_id == item && entry.kind == StackItemKind::TriggeredAbility
            })
            .map(|entry| entry.v4.stack_item_id),
        Some(Target::Object(spell)) => state
            .stack
            .iter()
            .find(|entry| entry.kind == StackItemKind::Spell && entry.source == spell)
            .filter(|_| is_colorless(state, spell))
            .map(|entry| entry.v4.stack_item_id),
        _ => None,
    };
    if let Some(item) = item {
        if crate::engine::counter_stack_item_by_id(state, item).is_err() {
            halt(state, ctx.source);
        }
    }
}

fn pay_energy_then_damage(ctx: &ExecCtx, state: &mut GameState, amount: u32) {
    let Ok(paid) = u8::try_from(amount) else {
        return;
    };
    if crate::eternal_cards_v1::pay_energy(state, ctx.controller, paid).is_none() || paid == 0 {
        return;
    }
    if legal_object_target(ctx, state, 0, TargetSpec::CreatureOrPlaneswalker).is_some() {
        crate::effect::execute(
            &EffectOp::DealDamage {
                target: TargetRef::Target(0),
                amount: i32::from(paid),
            },
            ctx,
            state,
        );
    }
}

fn pay_energy_then_destroy(ctx: &ExecCtx, state: &mut GameState, amount: u32) {
    let Ok(paid) = u8::try_from(amount) else {
        return;
    };
    if crate::eternal_cards_v1::pay_energy(state, ctx.controller, paid).is_none() {
        return;
    }
    let doomed = [PlayerId::P0, PlayerId::P1]
        .into_iter()
        .flat_map(|player| state.players[player.index()].battlefield.iter().copied())
        .filter(|&object| {
            [
                CardType::Artifact,
                CardType::Creature,
                CardType::Enchantment,
            ]
            .into_iter()
            .any(|card_type| crate::engine::object_has_type(state, object, card_type))
                && crate::engine::object_mana_value(state, object) <= u16::from(paid)
                && !crate::engine::has_effective_keyword(state, object, Keywords::INDESTRUCTIBLE)
        })
        .map(|object| ProposedEvent::zone_change(object, Zone::Graveyard))
        .collect::<Vec<_>>();
    if !doomed.is_empty() {
        event::propose_and_commit_batch(state, doomed);
    }
}

fn guide_angel(ctx: &ExecCtx, state: &mut GameState) {
    let Some(object) = legal_object_target(ctx, state, 0, TargetSpec::AttackingCreature) else {
        return;
    };
    add_plus_one_counters(state, ctx, object, 2);
    if state.objects.get(object).zone != Zone::Battlefield {
        return;
    }
    // The flying counter grants flying to this incarnation, which loses it
    // with its counters when it leaves the battlefield.
    let timestamp = crate::engine::next_timestamp(state);
    state
        .objects
        .get_mut(object)
        .v4
        .creature_upgrade
        .get_or_insert_with(Default::default)
        .keyword_grants
        .push((Keywords::FLYING, timestamp));
    let angel = Subtype::Angel.stable_id();
    let subtypes = &mut state.objects.get_mut(object).v4.effective_subtype_ids;
    if let Err(position) = subtypes.binary_search(&angel) {
        subtypes.insert(position, angel);
    }
}

fn linebreaker_pump_target(ctx: &ExecCtx, state: &mut GameState) {
    let Some(object) = legal_object_target(ctx, state, 0, TargetSpec::ControlledCreature) else {
        return;
    };
    let eldrazi = controlled_permanents(state, ctx.controller)
        .into_iter()
        .filter(|&permanent| {
            crate::engine::has_effective_subtype(state, permanent, Subtype::Eldrazi)
        })
        .count();
    let binding = EffectObjectBinding {
        object,
        expected_zone: Zone::Battlefield,
        expected_zone_change_count: state.objects.get(object).zone_change_count,
    };
    crate::effect::install_temporary_boost(
        state,
        binding,
        i32::try_from(eldrazi).unwrap_or(i32::MAX),
        0,
        Keywords::HASTE,
    );
}

fn ocelot_pride(ctx: &ExecCtx, state: &mut GameState) {
    let controller = ctx.controller;
    if !gained_life_this_turn(state, controller) {
        return;
    }
    crate::effect::execute(
        &EffectOp::CreateToken {
            token_def: token_id(CAT_TOKEN),
            controller: PlayerRef::Controller,
        },
        ctx,
        state,
    );
    note_city_blessing(state);
    if !has_city_blessing(state, controller) {
        return;
    }
    let tokens = controlled_permanents(state, controller)
        .into_iter()
        .filter(|&object| {
            let live = state.objects.get(object);
            live.v4.is_token && live.v4.entered_battlefield_this_turn
        })
        .collect::<Vec<_>>();
    for token in tokens {
        crate::standard_cards_v1::create_token_copy(state, token, controller);
    }
}

fn phelia_exile_target(ctx: &ExecCtx, state: &mut GameState) {
    let Some(object) = legal_object_target(
        ctx,
        state,
        0,
        TargetSpec::StandardV1(StandardTargetV1::UpToOneOtherNonlandPermanentV1),
    ) else {
        return;
    };
    event::propose_and_commit(state, ProposedEvent::zone_change(object, Zone::Exile));
    let live = state.objects.get(object);
    if live.zone != Zone::Exile || live.v4.is_token {
        return;
    }
    let Some(source) = ctx.ability_source_contract else {
        return;
    };
    let card = live_link(state, object);
    e5b_state(state).phelia_returns.push(PheliaReturnV1 {
        source,
        controller: ctx.controller,
        card,
    });
}

fn phelia_return(ctx: &ExecCtx, state: &mut GameState, card: ObjectLinkV4) {
    if !link_live_in(state, card, Zone::Exile) {
        return;
    }
    let owner = state.objects.get(card.object).owner;
    event::propose_and_commit(
        state,
        ProposedEvent::zone_change_to_battlefield_under_controller(card.object, owner),
    );
    let returned = state.objects.get(card.object);
    if returned.zone == Zone::Battlefield && returned.controller == ctx.controller {
        if let Some(phelia) = live_ability_source(ctx, state) {
            add_plus_one_counters(state, ctx, phelia, 1);
        }
    }
}

fn phlage_sacrifice_unless_escaped(ctx: &ExecCtx, state: &mut GameState) {
    let Some(contract) = ctx.ability_source_contract else {
        return;
    };
    let escaped = contract
        .zone_change_count
        .checked_sub(1)
        .is_some_and(|stack| {
            read_state(state).is_some_and(|e5b| {
                e5b.escaped_casts.contains(&ObjectLinkV4 {
                    object: contract.source,
                    zone_change_count: stack,
                })
            })
        });
    if !escaped {
        crate::effect::execute(&sacrifice_source(), ctx, state);
    }
}

fn mobilize(ctx: &ExecCtx, state: &mut GameState, count: u8) {
    let warrior = token_id(WARRIOR_TOKEN);
    let mut tokens = Vec::new();
    for _ in 0..count {
        let start = state.engine.event_history.len();
        crate::effect::execute(
            &EffectOp::CreateTokenTappedAndAttacking { token_def: warrior },
            ctx,
            state,
        );
        let created = state.engine.event_history[start..]
            .iter()
            .filter_map(|event| match event {
                CommittedEvent::CreateToken { object, .. } => Some(*object),
                _ => None,
            })
            .collect::<Vec<_>>();
        for token in created {
            tokens.push(live_link(state, token));
        }
    }
    if let (Some(source), false) = (ctx.ability_source_contract, tokens.is_empty()) {
        e5b_state(state).mobilized.push(MobilizedV1 {
            source,
            controller: ctx.controller,
            tokens,
        });
    }
}

fn lazotep_zombie_copy(ctx: &ExecCtx, state: &mut GameState) {
    let Some(Target::Object(card)) = ctx.targets.first().copied() else {
        return;
    };
    let live = state.objects.get(card);
    if !ctx.target_incarnation_matches(0, state)
        || live.zone != Zone::Graveyard
        || live.owner != ctx.controller
        || !CARD_DEFS[live.card_def as usize].has_type(CardType::Creature)
        || crate::engine::object_mana_value(state, card) != ctx.x_value
    {
        return;
    }
    event::propose_and_commit(state, ProposedEvent::zone_change(card, Zone::Exile));
    if state.objects.get(card).zone != Zone::Exile {
        return;
    }
    let Some(token) = crate::standard_cards_v1::create_token_copy(state, card, ctx.controller)
    else {
        return;
    };
    if state.objects.get(token).zone != Zone::Battlefield {
        return;
    }
    // "except it's a 4/4 black Zombie".
    let timestamp = crate::engine::next_timestamp(state);
    let upgrade = state
        .objects
        .get_mut(token)
        .v4
        .creature_upgrade
        .get_or_insert_with(Default::default);
    upgrade.base_stats = Some((4, 4, timestamp));
    upgrade.color = Some((
        crate::card_def::mana_colors_mask(&[ManaColor::B]),
        timestamp,
    ));
    upgrade.creature_types = Some((vec![Subtype::Zombie.stable_id()], timestamp));
}

fn eldrazi_confluence(ctx: &ExecCtx, state: &mut GameState, modes: [u8; 3]) {
    let spec = confluence_target(modes);
    let mut creature_slot = 0usize;
    let mut permanent_slot = modes.iter().filter(|&&mode| mode == 0).count();
    for mode in modes {
        match mode {
            0 => {
                let slot = creature_slot;
                creature_slot += 1;
                if let Some(object) = legal_object_target(ctx, state, slot, spec) {
                    let binding = EffectObjectBinding {
                        object,
                        expected_zone: Zone::Battlefield,
                        expected_zone_change_count: state.objects.get(object).zone_change_count,
                    };
                    crate::effect::install_temporary_boost(state, binding, 3, -3, Keywords::NONE);
                }
            }
            1 => {
                let slot = permanent_slot;
                permanent_slot += 1;
                if let Some(object) = legal_object_target(ctx, state, slot, spec) {
                    flicker_tapped(state, object);
                }
            }
            _ => crate::effect::execute(
                &EffectOp::CreateToken {
                    token_def: token_id(ELDRAZI_SCION_TOKEN),
                    controller: PlayerRef::Controller,
                },
                ctx,
                state,
            ),
        }
    }
}

pub(crate) fn execute(op: &E5bOpV1, ctx: &ExecCtx, state: &mut GameState) {
    let controller = ctx.controller;
    match op {
        E5bOpV1::CounterTargetAbilityV1 => counter_target_ability(ctx, state),
        E5bOpV1::CounterTriggeredAbilityOrColorlessSpellV1 => {
            counter_triggered_ability_or_colorless_spell(ctx, state)
        }
        E5bOpV1::PayEnergyDamageTargetV1 { amount } => pay_energy_then_damage(ctx, state, *amount),
        E5bOpV1::PayEnergyDestroyV1 { amount } => pay_energy_then_destroy(ctx, state, *amount),
        E5bOpV1::AjaniCountersV1 => {
            let cats = controlled_permanents(state, controller)
                .into_iter()
                .filter(|&object| crate::engine::has_effective_subtype(state, object, Subtype::Cat))
                .collect::<Vec<_>>();
            for cat in cats {
                add_plus_one_counters(state, ctx, cat, 1);
            }
        }
        E5bOpV1::AjaniTokenThenDamageV1 => {
            crate::effect::execute(&create_cat_warrior(), ctx, state);
            if controls_other_red_permanent(state, controller, ctx.source) {
                if let Some(source) = ctx.ability_source_contract {
                    queue_trigger(state, source, controller, E5bOpV1::AjaniDamageV1);
                }
            }
        }
        E5bOpV1::AjaniDamageV1 => {
            if controls_other_red_permanent(state, controller, ctx.source) {
                let amount = i32::try_from(creature_count(state, controller)).unwrap_or(i32::MAX);
                crate::effect::execute(
                    &EffectOp::DealDamage {
                        target: TargetRef::Target(0),
                        amount,
                    },
                    ctx,
                    state,
                );
            }
        }
        E5bOpV1::GuidePayV1 => {
            if crate::eternal_cards_v1::pay_energy(state, controller, 3).is_some() {
                if let Some(source) = ctx.ability_source_contract {
                    queue_trigger(state, source, controller, E5bOpV1::GuideAngelV1);
                }
            }
        }
        E5bOpV1::GuideAngelV1 => guide_angel(ctx, state),
        E5bOpV1::LinebreakerV1 => linebreaker_pump_target(ctx, state),
        E5bOpV1::OcelotPrideV1 => ocelot_pride(ctx, state),
        E5bOpV1::PheliaExileV1 => phelia_exile_target(ctx, state),
        E5bOpV1::PheliaReturnV1 { card } => phelia_return(ctx, state, *card),
        E5bOpV1::PhlageSacrificeUnlessEscapedV1 => phlage_sacrifice_unless_escaped(ctx, state),
        E5bOpV1::PayOneEnergyV1 => {
            if crate::eternal_cards_v1::pay_energy(state, controller, 1).is_none() {
                crate::effect::execute(&sacrifice_source(), ctx, state);
            }
        }
        E5bOpV1::MobilizeV1 { count } => mobilize(ctx, state, *count),
        E5bOpV1::SacrificeMobilizedV1 { tokens } => {
            let doomed = tokens
                .iter()
                .filter(|&&token| {
                    link_live_in(state, token, Zone::Battlefield)
                        && state.objects.get(token.object).controller == controller
                })
                .map(|token| token.object)
                .collect::<Vec<_>>();
            sacrifice_all(state, &doomed);
        }
        E5bOpV1::CreateSandWarriorV1 => crate::effect::execute(&sand_warrior(), ctx, state),
        E5bOpV1::HamletNameV1 { card_def } => {
            if let Some(hamlet) = live_ability_source(ctx, state) {
                let link = live_link(state, hamlet);
                let names = &mut e5b_state(state).hamlet_names;
                names.retain(|(host, _)| host.object != hamlet);
                names.push((link, *card_def));
            }
        }
        E5bOpV1::LazotepZombieV1 => lazotep_zombie_copy(ctx, state),
        E5bOpV1::ConfluenceV1 { modes } => eldrazi_confluence(ctx, state, *modes),
        E5bOpV1::ExileTargetV1 => {
            if let Some(object) = bound_battlefield_target(ctx, state, 0) {
                event::propose_and_commit(state, ProposedEvent::zone_change(object, Zone::Exile));
            }
        }
        E5bOpV1::BounceTargetV1 => {
            if let Some(object) = bound_battlefield_target(ctx, state, 0) {
                event::propose_and_commit(state, ProposedEvent::zone_change(object, Zone::Hand));
            }
        }
        // Choice-bearing leaves run in the resumable interpreter (`drive`).
        E5bOpV1::GalvanicDischargeV1
        | E5bOpV1::WrathOfTheSkiesV1
        | E5bOpV1::TerminusV1
        | E5bOpV1::MiracleV1
        | E5bOpV1::MiracleCastV1
        | E5bOpV1::AmpedRaptorV1
        | E5bOpV1::RaptorCastV1 { .. }
        | E5bOpV1::SmasherV1 { .. }
        | E5bOpV1::AjaniMayTransformV1
        | E5bOpV1::AjaniSacrificeRestV1
        | E5bOpV1::GuideMayPayV1
        | E5bOpV1::StaticPrisonUpkeepV1
        | E5bOpV1::HamletChooseNameV1
        | E5bOpV1::SandScoutSearchV1 => halt(state, ctx.source),
    }
}
