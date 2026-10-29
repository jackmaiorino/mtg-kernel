//! Card programs for the eternal catalog's Pauper Bogles, white aggro,
//! Rakdos Madness, Boros Moxite and Gates lists (wave E6b,
//! `docs/design/eternal_formats_support_v1.md`).
//!
//! `eternal_cards_v1` forwards its effect leaves, trigger conditions and
//! state here, and the engine calls the hooks below only in
//! `eternal-fixtures` builds. Every table is keyed by a printed name that
//! occurs only in the eternal registry, so the Pauper, FDN and Standard
//! catalogs never reach it. Behavior was read from each card's current
//! Oracle text and, where present, its XMage source.
//!
//! New rules here: Auras that count enchantments, give their controller life
//! for the damage their creature deals, return to hand, enchant lands
//! (granting a mana ability or adding mana when the land is tapped for
//! mana) or give protection from a chosen color; "can't be blocked except
//! by creatures with flying"; sneak, web-slinging and disturb casts;
//! connive; endure; an equipped-creature attack trigger; and a modal entry
//! trigger. Resolution choices use E3's typed selection
//! (`E3ChoiceKindV1::E6bV1`) or a generated option list (`EffectOp::Choice`).
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

use crate::card_def::{
    card_id_by_name, AltCostCondition, CardDef, CardType, Keywords, Subtype, TargetSpec, CARD_DEFS,
};
use crate::effect::{
    EffectContinuation, EffectObjectBinding, EffectOp, ExecCtx, LibraryCardFilter, ObjectRef,
    PlayerRef, TargetRef,
};
use crate::eternal_cards_v1::e3::{E3ChoiceKindV1, E3ChoiceV1};
use crate::eternal_cards_v1::EternalOpV1;
use crate::event::{self, CommittedEvent, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::mana::ManaColor;
use crate::standard_cards_v1::{
    StandardChosenActionV1, StandardOpV1, StandardPermanentFilterV1, StandardTargetV1,
    StandardTriggerV1,
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
pub struct E6bStateV1 {
    /// Spells cast for their sneak cost, by stack incarnation: they enter
    /// tapped and attacking.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sneak_casts: Vec<ObjectLinkV4>,
    /// Spells cast for their disturb cost, by stack incarnation: they enter
    /// transformed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub disturb_casts: Vec<ObjectLinkV4>,
    /// Triggers detected by hooks, waiting for the next trigger collection.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub queued_triggers: Vec<QueuedTriggerV1>,
}

impl E6bStateV1 {
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
    pub op: E6bOpV1,
}

fn e6b_state(state: &mut GameState) -> &mut E6bStateV1 {
    &mut state.eternal_v1.get_or_insert_with(Default::default).e6b
}

fn read_state(state: &GameState) -> Option<&E6bStateV1> {
    state.eternal_v1.as_ref().map(|eternal| &eternal.e6b)
}

/// Trigger conditions owned by this module, carried as
/// `eternal_cards_v1::EternalTriggerV1::E6bV1`. Each checks its own source,
/// so a back face's trigger (Luminous Phantom) works too.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum E6bTriggerV1 {
    /// "When this land enters untapped" (Idyllic Grange).
    EntersUntappedV1,
    /// "When this creature enters, if you control no tapped lands"
    /// (Martyr's Soul). The resolution rechecks it.
    EntersWithNoTappedLandsV1,
    /// "Whenever equipped creature attacks" (Greatsword of Tyr).
    EquippedCreatureAttacksV1,
    /// "Whenever another creature you control leaves the battlefield"
    /// (Luminous Phantom, Lunarch Veteran's back face).
    AnotherControlledCreatureLeavesV1,
}

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::E6bV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum E6bOpV1 {
    /// The controller gains `amount` life (Spirit Link, Armadillo Cloak).
    GainLifeV1 { amount: i32 },
    /// Rancor: return this card from its graveyard to its owner's hand.
    ReturnSourceToHandV1,
    /// `count` +1/+1 counters on the ability's source incarnation (endure).
    SourceCountersV1 { count: u8 },
    /// Martyr's Soul: "if you control no tapped lands, put two +1/+1
    /// counters on it", rechecked on resolution.
    MartyrCountersV1,
    /// Connive's discard: discard a card, then a +1/+1 counter on the
    /// conniving creature if it was a nonland card. A choice.
    ConniveDiscardV1,
    /// Malevolent Rumble's reveal: a permanent card among the top four to
    /// hand, the rest to the graveyard. A choice.
    RumbleRevealV1,
    /// Guardians' Pledge: white creatures you control get +2/+2 until end
    /// of turn.
    WhiteCreaturesPumpV1,
    /// Greatsword of Tyr: a +1/+1 counter on the equipped creature, then
    /// tap up to one target creature defending player controls.
    GreatswordV1,
    /// An Aura enters attached to target 0 with `color` chosen (Benevolent
    /// Blessing).
    CreatureAuraEntersWithColorV1 { color: ManaColor },
    /// An Aura that enchants a land enters attached to target 0 with
    /// `color` chosen (Utopia Sprawl).
    LandAuraEntersWithColorV1 { color: ManaColor },
}

impl E6bOpV1 {
    /// Whether this leaf can yield a player decision, so its program must
    /// use the resumable interpreter.
    pub(crate) fn contains_player_choice(&self) -> bool {
        matches!(self, Self::ConniveDiscardV1 | Self::RumbleRevealV1)
    }
}

/// Resolution choices owned by this module, carried as
/// `E3ChoiceKindV1::E6bV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum E6bChoiceV1 {
    /// Connive: the card to discard, from the chooser's hand.
    ConniveDiscardV1,
    /// Malevolent Rumble: up to one permanent card among `revealed` to put
    /// into hand.
    RumblePickV1 { revealed: Vec<EffectObjectBinding> },
}

impl E6bChoiceV1 {
    /// Whether the candidates are hidden from the other player.
    pub(crate) fn chooser_private(&self) -> bool {
        matches!(self, Self::ConniveDiscardV1)
    }

    pub(crate) fn public_purpose(&self) -> crate::rl::TargetSelectionPurposeV4 {
        crate::rl::TargetSelectionPurposeV4::CardSelection
    }

    /// Object incarnations this choice refers to beyond its candidates.
    pub(crate) fn referenced_bindings(&self) -> Vec<EffectObjectBinding> {
        match self {
            Self::ConniveDiscardV1 => Vec::new(),
            Self::RumblePickV1 { revealed } => revealed.clone(),
        }
    }
}

// ---- Names and helpers --------------------------------------------------------------

const ABUNDANT_GROWTH: &str = "Abundant Growth";
const ANCESTRAL_MASK: &str = "Ancestral Mask";
const ANGELIC_GIFT: &str = "Angelic Gift";
const ARMADILLO_CLOAK: &str = "Armadillo Cloak";
const ASGARDIAN_CITADEL: &str = "Asgardian Citadel";
const BENEVOLENT_BLESSING: &str = "Benevolent Blessing";
const BOROS_GARRISON: &str = "Boros Garrison";
const CARTOUCHE_OF_SOLIDARITY: &str = "Cartouche of Solidarity";
const DAWNBRINGER_CLERIC: &str = "Dawnbringer Cleric";
const DEMAND_ANSWERS: &str = "Demand Answers";
const EAGLES_OF_THE_NORTH: &str = "Eagles of the North";
const ETHEREAL_ARMOR: &str = "Ethereal Armor";
const FORTRESS_KIN_GUARD: &str = "Fortress Kin-Guard";
const GREATSWORD_OF_TYR: &str = "Greatsword of Tyr";
const HELIODS_PILGRIM: &str = "Heliod's Pilgrim";
const IDYLLIC_GRANGE: &str = "Idyllic Grange";
const JAGGED_BARRENS: &str = "Jagged Barrens";
const LEONARDO: &str = "Leonardo, Big Brother";
const LUNARCH_VETERAN: &str = "Lunarch Veteran";
const MARTYRS_SOUL: &str = "Martyr's Soul";
const RAFFINES_INFORMANT: &str = "Raffine's Informant";
const RAGGED_SHORT_SPEAR: &str = "Ragged Short Spear";
const RAKDOS_CARNARIUM: &str = "Rakdos Carnarium";
const RANCOR: &str = "Rancor";
const SALT_ROAD_PACKBEAST: &str = "Salt Road Packbeast";
const SEARCH_PARTY_CAPTAIN: &str = "Search Party Captain";
const SILHANA_LEDGEWALKER: &str = "Silhana Ledgewalker";
const SPIRIT_LINK: &str = "Spirit Link";
const THRABEN_INSPECTOR: &str = "Thraben Inspector";
const UTOPIA_SPRAWL: &str = "Utopia Sprawl";
const BLOOD_TOKEN: &str = "Blood Token";
const CLUE_TOKEN: &str = "Clue Token";
const ELDRAZI_SPAWN_TOKEN: &str = "Eldrazi Spawn Token";
const WHITE_WARRIOR_TOKEN: &str = "White Warrior Token";
const WHITE_SPIRIT_TOKEN: &str = "White Spirit Token";

const COLORS: [ManaColor; 5] = [
    ManaColor::W,
    ManaColor::U,
    ManaColor::B,
    ManaColor::R,
    ManaColor::G,
];

fn e6b(op: E6bOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::E6bV1(op)))
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

/// Whether `object` is a battlefield permanent named `name` showing face
/// `face` whose printed abilities apply.
fn active_face(state: &GameState, object: ObjectId, name: &str, face: u8) -> bool {
    state.objects.try_get(object).is_some_and(|live| {
        live.zone == Zone::Battlefield
            && live.v4.face_index == face
            && CARD_DEFS
                .get(live.card_def as usize)
                .is_some_and(|def| def.name == name && def.is_executable())
            && crate::continuous_characteristics_v1::printed_abilities_active(state, object)
    })
}

fn active_named(state: &GameState, object: ObjectId, name: &str) -> bool {
    active_face(state, object, name, 0)
}

/// Every battlefield object, in each player's battlefield order.
fn battlefield(state: &GameState) -> impl Iterator<Item = ObjectId> + '_ {
    [PlayerId::P0, PlayerId::P1]
        .into_iter()
        .flat_map(move |player| state.players[player.index()].battlefield.iter().copied())
}

/// The battlefield permanents named `name` whose printed abilities apply.
fn active_permanents<'a>(
    state: &'a GameState,
    name: &'a str,
) -> impl Iterator<Item = ObjectId> + 'a {
    battlefield(state).filter(move |&object| active_named(state, object, name))
}

/// The active Auras named `name` attached to `host`'s current incarnation.
fn attached_named<'a>(
    state: &'a GameState,
    host: ObjectId,
    name: &'a str,
) -> impl Iterator<Item = ObjectId> + 'a {
    let host_link = state
        .objects
        .try_get(host)
        .filter(|live| live.zone == Zone::Battlefield)
        .map(|_| live_link(state, host));
    active_permanents(state, name).filter(move |&aura| {
        host_link.is_some() && state.objects.get(aura).v4.attached_to == host_link
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

fn queue_trigger(
    state: &mut GameState,
    source: AbilitySourceContractV4,
    controller: PlayerId,
    op: E6bOpV1,
) {
    e6b_state(state).queued_triggers.push(QueuedTriggerV1 {
        source,
        controller,
        op,
    });
}

fn is_creature(state: &GameState, object: ObjectId) -> bool {
    crate::engine::object_has_type(state, object, CardType::Creature)
}

/// A permanent card (110.4a): an artifact, creature, enchantment, land or
/// planeswalker card.
fn is_permanent_card(def: &CardDef) -> bool {
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

fn controls_tapped_land(state: &GameState, player: PlayerId) -> bool {
    battlefield(state).any(|object| {
        let live = state.objects.get(object);
        live.controller == player
            && live.tapped
            && crate::engine::object_has_type(state, object, CardType::Land)
    })
}

// ---- Spells -------------------------------------------------------------------------

/// Alms of the Vein: "Target opponent loses 3 life and you gain 3 life."
pub fn alms_of_the_vein() -> EffectOp {
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

/// Vampire's Kiss: "Target player loses 2 life and you gain 2 life. Create
/// two Blood tokens."
pub fn vampires_kiss() -> EffectOp {
    let blood = || EffectOp::CreateToken {
        token_def: token_id(BLOOD_TOKEN),
        controller: PlayerRef::Controller,
    };
    EffectOp::Sequence(vec![
        EffectOp::LoseLife {
            player: PlayerRef::Target(0),
            amount: 2,
        },
        EffectOp::GainLife {
            player: PlayerRef::Controller,
            amount: 2,
        },
        blood(),
        blood(),
    ])
}

/// Guardians' Pledge: "White creatures you control get +2/+2 until end of
/// turn."
pub fn guardians_pledge() -> EffectOp {
    e6b(E6bOpV1::WhiteCreaturesPumpV1)
}

/// Malevolent Rumble: "Reveal the top four cards of your library. You may
/// put a permanent card from among them into your hand. Put the rest into
/// your graveyard. Create a 0/1 colorless Eldrazi Spawn creature token with
/// 'Sacrifice this token: Add {C}.'"
pub fn malevolent_rumble() -> EffectOp {
    EffectOp::Sequence(vec![
        e6b(E6bOpV1::RumbleRevealV1),
        EffectOp::CreateToken {
            token_def: token_id(ELDRAZI_SPAWN_TOKEN),
            controller: PlayerRef::Controller,
        },
    ])
}

/// "You may discard a card. If you do, draw two cards."
fn may_discard_then_draw_two() -> EffectOp {
    EffectOp::MayPayCostThen {
        discard: 1,
        sacrifice_lands: 0,
        return_permanent: None,
        then: Box::new(EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 2,
        }),
        otherwise: None,
    }
}

/// Pursue the Past: "You gain 2 life. You may discard a card. If you do,
/// draw two cards." Flashback is the card's own `CardDef::flashback`.
pub fn pursue_the_past() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::GainLife {
            player: PlayerRef::Controller,
            amount: 2,
        },
        may_discard_then_draw_two(),
    ])
}

/// Benevolent Blessing: "As this Aura enters, choose a color." It then
/// enters attached to its target.
pub fn benevolent_blessing() -> EffectOp {
    EffectOp::Choice {
        controller: PlayerRef::Controller,
        options: COLORS
            .into_iter()
            .map(|color| e6b(E6bOpV1::CreatureAuraEntersWithColorV1 { color }))
            .collect(),
    }
}

/// Utopia Sprawl: "As this Aura enters, choose a color." It then enters
/// attached to its target Forest.
pub fn utopia_sprawl() -> EffectOp {
    EffectOp::Choice {
        controller: PlayerRef::Controller,
        options: COLORS
            .into_iter()
            .map(|color| e6b(E6bOpV1::LandAuraEntersWithColorV1 { color }))
            .collect(),
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

const fn e6b_trigger(condition: E6bTriggerV1, effect: fn() -> EffectOp) -> TriggeredAbilityDef {
    trigger(
        TriggerCondition::StandardV1(StandardTriggerV1::EternalV1(
            crate::eternal_cards_v1::EternalTriggerV1::E6bV1(condition),
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

fn gain_one() -> EffectOp {
    EffectOp::GainLife {
        player: PlayerRef::Controller,
        amount: 1,
    }
}

/// Jagged Barrens: "it deals 1 damage to target opponent."
fn damage_target_opponent() -> EffectOp {
    EffectOp::DealDamage {
        target: TargetRef::Target(0),
        amount: 1,
    }
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

/// Idyllic Grange: "put a +1/+1 counter on target creature you control."
fn counter_on_target() -> EffectOp {
    EffectOp::AddPlusOnePlusOneCounters {
        object: ObjectRef::Target(0),
        count: 1,
    }
}

/// Thraben Inspector: "investigate" (create a Clue token).
fn investigate() -> EffectOp {
    EffectOp::CreateToken {
        token_def: token_id(CLUE_TOKEN),
        controller: PlayerRef::Controller,
    }
}

/// Cartouche of Solidarity: "create a 1/1 white Warrior creature token with
/// vigilance."
fn create_warrior() -> EffectOp {
    EffectOp::CreateToken {
        token_def: token_id(WHITE_WARRIOR_TOKEN),
        controller: PlayerRef::Controller,
    }
}

/// Rancor: "When this Aura is put into a graveyard from the battlefield,
/// return it to its owner's hand."
fn return_to_hand() -> EffectOp {
    e6b(E6bOpV1::ReturnSourceToHandV1)
}

/// Dawnbringer Cleric's three modes.
fn dawnbringer_modes() -> Vec<(TargetSpec, EffectOp)> {
    vec![
        // Cure Wounds: "You gain 2 life."
        (
            TargetSpec::None,
            EffectOp::GainLife {
                player: PlayerRef::Controller,
                amount: 2,
            },
        ),
        // Dispel Magic: "Destroy target enchantment."
        (
            TargetSpec::EnchantmentPermanent,
            EffectOp::DestroyObject {
                object: ObjectRef::Target(0),
            },
        ),
        // Gentle Repose: "Exile target card from a graveyard."
        (
            TargetSpec::CardInGraveyards,
            EffectOp::MoveObject {
                object: ObjectRef::Target(0),
                to_zone: Zone::Exile,
            },
        ),
    ]
}

/// The mode marker of Dawnbringer Cleric's entry trigger; choosing a mode
/// replaces it with that mode's program.
fn dawnbringer_entry() -> EffectOp {
    EffectOp::Choice {
        controller: PlayerRef::Controller,
        options: dawnbringer_modes()
            .into_iter()
            .map(|(_, effect)| effect)
            .collect(),
    }
}

/// Eagles of the North: "creatures you control get +1/+0 and gain first
/// strike until end of turn."
fn eagles_boost() -> EffectOp {
    EffectOp::BoostControlledCreaturesUntilEndOfTurn {
        power: 1,
        toughness: 0,
        keywords: Keywords::FIRST_STRIKE,
    }
}

/// Fortress Kin-Guard: "it endures 1. (Put a +1/+1 counter on it or create
/// a 1/1 white Spirit creature token.)"
fn endure_one() -> EffectOp {
    EffectOp::Choice {
        controller: PlayerRef::Controller,
        options: vec![
            e6b(E6bOpV1::SourceCountersV1 { count: 1 }),
            EffectOp::CreateToken {
                token_def: token_id(WHITE_SPIRIT_TOKEN),
                controller: PlayerRef::Controller,
            },
        ],
    }
}

/// Heliod's Pilgrim: "you may search your library for an Aura card, reveal
/// it, put it into your hand, then shuffle."
fn search_aura() -> EffectOp {
    EffectOp::SearchLibraryToHand {
        player: PlayerRef::Controller,
        filter: LibraryCardFilter::CardWithAnySubtypeV1([Subtype::Aura, Subtype::Aura]),
    }
}

fn martyr_counters() -> EffectOp {
    e6b(E6bOpV1::MartyrCountersV1)
}

/// Raffine's Informant: "it connives. (Draw a card, then discard a card. If
/// you discarded a nonland card, put a +1/+1 counter on this creature.)"
fn connive() -> EffectOp {
    EffectOp::Sequence(vec![draw_one(), e6b(E6bOpV1::ConniveDiscardV1)])
}

fn greatsword_attack() -> EffectOp {
    e6b(E6bOpV1::GreatswordV1)
}

const GAIN_ONE_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(TriggerCondition::Etb, gain_one)];
const JAGGED_BARRENS_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, damage_target_opponent)];
const KAROO_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, return_controlled_land)];
const IDYLLIC_GRANGE_TRIGGERS: [TriggeredAbilityDef; 1] = [e6b_trigger(
    E6bTriggerV1::EntersUntappedV1,
    counter_on_target,
)];
const DRAW_ONE_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(TriggerCondition::Etb, draw_one)];
const CARTOUCHE_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, create_warrior)];
const THRABEN_INSPECTOR_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, investigate)];
const RANCOR_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    home_zone: Zone::Graveyard,
    ..trigger(TriggerCondition::LeftBattlefieldToGraveyard, return_to_hand)
}];
const DAWNBRINGER_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, dawnbringer_entry)];
const EAGLES_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(TriggerCondition::Etb, eagles_boost)];
const FORTRESS_KIN_GUARD_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, endure_one)];
const HELIODS_PILGRIM_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, search_aura)];
const LUNARCH_VETERAN_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(
        TriggerCondition::OtherControlledCreatureEnters { subtype: None },
        gain_one,
    ),
    TriggeredAbilityDef {
        face_index: 1,
        ..e6b_trigger(E6bTriggerV1::AnotherControlledCreatureLeavesV1, gain_one)
    },
];
const MARTYRS_SOUL_TRIGGERS: [TriggeredAbilityDef; 1] = [e6b_trigger(
    E6bTriggerV1::EntersWithNoTappedLandsV1,
    martyr_counters,
)];
const RAFFINES_INFORMANT_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, connive)];
const RAGGED_SHORT_SPEAR_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, may_discard_then_draw_two)];
const GREATSWORD_TRIGGERS: [TriggeredAbilityDef; 1] = [e6b_trigger(
    E6bTriggerV1::EquippedCreatureAttacksV1,
    greatsword_attack,
)];

/// Triggered abilities of this module's cards.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        ASGARDIAN_CITADEL => &GAIN_ONE_TRIGGERS,
        JAGGED_BARRENS => &JAGGED_BARRENS_TRIGGERS,
        BOROS_GARRISON | RAKDOS_CARNARIUM => &KAROO_TRIGGERS,
        IDYLLIC_GRANGE => &IDYLLIC_GRANGE_TRIGGERS,
        ANGELIC_GIFT | ABUNDANT_GROWTH | SALT_ROAD_PACKBEAST | SEARCH_PARTY_CAPTAIN => {
            &DRAW_ONE_TRIGGERS
        }
        CARTOUCHE_OF_SOLIDARITY => &CARTOUCHE_TRIGGERS,
        THRABEN_INSPECTOR => &THRABEN_INSPECTOR_TRIGGERS,
        RANCOR => &RANCOR_TRIGGERS,
        DAWNBRINGER_CLERIC => &DAWNBRINGER_TRIGGERS,
        EAGLES_OF_THE_NORTH => &EAGLES_TRIGGERS,
        FORTRESS_KIN_GUARD => &FORTRESS_KIN_GUARD_TRIGGERS,
        HELIODS_PILGRIM => &HELIODS_PILGRIM_TRIGGERS,
        LUNARCH_VETERAN => &LUNARCH_VETERAN_TRIGGERS,
        MARTYRS_SOUL => &MARTYRS_SOUL_TRIGGERS,
        RAFFINES_INFORMANT => &RAFFINES_INFORMANT_TRIGGERS,
        RAGGED_SHORT_SPEAR => &RAGGED_SHORT_SPEAR_TRIGGERS,
        GREATSWORD_OF_TYR => &GREATSWORD_TRIGGERS,
        // Waves E7a, E7b and E8a name disjoint cards.
        _ => match crate::eternal_legacy_e7a_v1::triggers_for(name) {
            [] => match crate::eternal_vintage_e7b_v1::triggers_for(name) {
                [] => crate::eternal_legacy_e8a_v1::triggers_for(name),
                triggers => triggers,
            },
            triggers => triggers,
        },
    }
}

/// Dawnbringer Cleric's modes, for its unselected entry trigger.
pub(crate) fn trigger_modes(name: &str, effect: &EffectOp) -> Option<Vec<(TargetSpec, EffectOp)>> {
    (name == DAWNBRINGER_CLERIC && *effect == dawnbringer_entry()).then(dawnbringer_modes)
}

fn own_trigger_effect(name: &str, effect: &EffectOp) -> bool {
    triggers_for(name)
        .iter()
        .any(|trigger| (trigger.effect)() == *effect)
        || delayed_effect_matches(name, effect)
}

/// Target specs of this module's triggered abilities, per ability.
pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    // `triggers_for` falls through to E7a's and E7b's cards, so they answer
    // first.
    if let Some(spec) = crate::eternal_legacy_e7a_v1::trigger_target_spec(name, effect) {
        return Some(spec);
    }
    if let Some(spec) = crate::eternal_vintage_e7b_v1::trigger_target_spec(name, effect) {
        return Some(spec);
    }
    if let Some(spec) = crate::eternal_legacy_e8a_v1::trigger_target_spec(name, effect) {
        return Some(spec);
    }
    if !own_trigger_effect(name, effect) {
        return None;
    }
    if name == DAWNBRINGER_CLERIC {
        return Some(
            dawnbringer_modes()
                .into_iter()
                .find(|(_, branch)| branch == effect)
                .map_or(TargetSpec::None, |(spec, _)| spec),
        );
    }
    Some(match name {
        JAGGED_BARRENS if *effect == damage_target_opponent() => TargetSpec::TargetOpponent,
        IDYLLIC_GRANGE if *effect == counter_on_target() => TargetSpec::ControlledCreature,
        GREATSWORD_OF_TYR if *effect == greatsword_attack() => {
            TargetSpec::StandardV1(StandardTargetV1::UpToOneOpponentCreatureV1)
        }
        _ => TargetSpec::None,
    })
}

/// Queued triggers and mode branches owned by this module's cards.
pub(crate) fn delayed_effect_matches(name: &str, effect: &EffectOp) -> bool {
    if crate::eternal_legacy_e7a_v1::delayed_effect_matches(name, effect)
        || crate::eternal_vintage_e7b_v1::delayed_effect_matches(name, effect)
        || crate::eternal_legacy_e8a_v1::delayed_effect_matches(name, effect)
        || crate::eternal_legacy_e9a_v1::delayed_effect_matches(name, effect)
    {
        return true;
    }
    if name == DAWNBRINGER_CLERIC {
        return dawnbringer_modes()
            .iter()
            .any(|(_, branch)| branch == effect);
    }
    let EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::E6bV1(op))) = effect else {
        return false;
    };
    match name {
        SPIRIT_LINK | ARMADILLO_CLOAK => matches!(op, E6bOpV1::GainLifeV1 { .. }),
        _ => false,
    }
}

/// Whether `condition` triggers on its source's arrival on the battlefield.
pub(crate) fn arrival_condition(condition: E6bTriggerV1) -> bool {
    matches!(
        condition,
        E6bTriggerV1::EntersUntappedV1 | E6bTriggerV1::EntersWithNoTappedLandsV1
    )
}

/// Whether `events[index]` is `source`'s own arrival on the battlefield.
fn arrived(events: &[CommittedEvent], index: usize, source: ObjectId) -> bool {
    matches!(
        events[index],
        CommittedEvent::ZoneChange {
            object,
            to: Zone::Battlefield,
            ..
        } if object == source
    )
}

pub(crate) fn trigger_matches(
    condition: E6bTriggerV1,
    events: &[CommittedEvent],
    index: usize,
    source: ObjectId,
    state: &GameState,
) -> bool {
    match condition {
        E6bTriggerV1::EntersUntappedV1 => {
            arrived(events, index, source)
                && active_named(state, source, IDYLLIC_GRANGE)
                && !state.objects.get(source).tapped
        }
        E6bTriggerV1::EntersWithNoTappedLandsV1 => {
            arrived(events, index, source)
                && active_named(state, source, MARTYRS_SOUL)
                && !controls_tapped_land(state, state.objects.get(source).controller)
        }
        E6bTriggerV1::EquippedCreatureAttacksV1 => {
            let CommittedEvent::DeclaredAttacker {
                source: attacker,
                source_zone_change_count,
                ..
            } = events[index]
            else {
                return false;
            };
            active_named(state, source, GREATSWORD_OF_TYR)
                && state.objects.get(source).v4.attached_to
                    == Some(ObjectLinkV4 {
                        object: attacker,
                        zone_change_count: source_zone_change_count,
                    })
        }
        E6bTriggerV1::AnotherControlledCreatureLeavesV1 => {
            let CommittedEvent::ZoneChange {
                object,
                from: Zone::Battlefield,
                controller_before,
                ..
            } = events[index]
            else {
                return false;
            };
            if object == source {
                return false;
            }
            let Some(def) = definition(state, object) else {
                return false;
            };
            if !def.has_type(CardType::Creature)
                && !events.iter().any(|event| {
                    matches!(
                        event,
                        CommittedEvent::WasCreatureBeforeLeavingBattlefield { object: left, .. }
                            if *left == object
                    )
                })
            {
                return false;
            }
            // The Phantom on the battlefield, or leaving it in this same batch
            // (leaves-the-battlefield abilities look back in time).
            let phantom_here = active_face(state, source, LUNARCH_VETERAN, 1)
                && state.objects.get(source).controller == controller_before;
            let phantom_left_together = events.iter().any(|event| {
                matches!(
                    event,
                    CommittedEvent::LeftBattlefieldFaceV1 {
                        object: left,
                        face_index: 1,
                        ..
                    } if *left == source
                )
            }) && events.iter().any(|event| {
                matches!(
                    event,
                    CommittedEvent::ZoneChange {
                        object: left,
                        from: Zone::Battlefield,
                        controller_before: phantom_controller,
                        ..
                    } if *left == source && *phantom_controller == controller_before
                )
            });
            phantom_here || phantom_left_together
        }
    }
}

// ---- Effect leaves ------------------------------------------------------------------

pub(crate) fn execute(op: &E6bOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        E6bOpV1::GainLifeV1 { amount } => {
            if *amount > 0 {
                crate::effect::execute(
                    &EffectOp::GainLife {
                        player: PlayerRef::Controller,
                        amount: *amount,
                    },
                    ctx,
                    state,
                );
            }
        }
        E6bOpV1::ReturnSourceToHandV1 => {
            let Some(contract) = ctx.ability_source_contract else {
                return;
            };
            // The trigger's source is the Aura that left the battlefield;
            // the card it returns is the one that went to the graveyard.
            let graveyard_count = if contract.zone == Zone::Battlefield {
                contract.zone_change_count.checked_add(1)
            } else {
                Some(contract.zone_change_count)
            };
            let Some(zone_change_count) = graveyard_count else {
                return;
            };
            let card = ObjectLinkV4 {
                object: contract.source,
                zone_change_count,
            };
            if link_live_in(state, card, Zone::Graveyard) {
                event::propose_and_commit(
                    state,
                    ProposedEvent::zone_change(card.object, Zone::Hand),
                );
            }
        }
        E6bOpV1::SourceCountersV1 { count } => {
            if let Some(source) = live_ability_source(ctx, state) {
                let _ =
                    event::add_plus_one_counters(state, source, ctx.controller, i32::from(*count));
            }
        }
        E6bOpV1::MartyrCountersV1 => {
            if let Some(source) = live_ability_source(ctx, state) {
                if !controls_tapped_land(state, ctx.controller) {
                    let _ = event::add_plus_one_counters(state, source, ctx.controller, 2);
                }
            }
        }
        E6bOpV1::WhiteCreaturesPumpV1 => {
            let white = crate::card_def::mana_color_mask(ManaColor::W);
            let creatures = battlefield(state)
                .filter(|&object| {
                    state.objects.get(object).controller == ctx.controller
                        && is_creature(state, object)
                        && crate::engine::object_color_mask(state, object) & white != 0
                })
                .collect::<Vec<_>>();
            for creature in creatures {
                let binding = bind(state, creature);
                crate::effect::install_temporary_boost(state, binding, 2, 2, Keywords::NONE);
            }
        }
        E6bOpV1::GreatswordV1 => {
            // "it": the creature this Equipment was attached to as the
            // ability triggered.
            let equipped = live_ability_source(ctx, state)
                .and_then(|source| state.objects.get(source).v4.attached_to)
                .or_else(|| {
                    ctx.ability_source_contract
                        .and_then(|contract| contract.attached_to)
                });
            if let Some(creature) = equipped {
                if link_live_in(state, creature, Zone::Battlefield)
                    && is_creature(state, creature.object)
                {
                    let _ = event::add_plus_one_counters(state, creature.object, ctx.controller, 1);
                }
            }
            if let Some(target) = bound_battlefield_target(ctx, state, 0) {
                if crate::engine::effect_target_is_legal(
                    state,
                    ctx.source,
                    ctx.controller,
                    TargetSpec::StandardV1(StandardTargetV1::UpToOneOpponentCreatureV1),
                    &ctx.targets,
                    0,
                ) {
                    event::propose_and_commit(state, ProposedEvent::tap(target));
                }
            }
        }
        E6bOpV1::CreatureAuraEntersWithColorV1 { color } => {
            crate::effect::execute(
                &EffectOp::PutSourceOntoBattlefieldAttachedToTarget {
                    target: ObjectRef::Target(0),
                },
                ctx,
                state,
            );
            set_entered_color(state, ctx.source, *color);
        }
        E6bOpV1::LandAuraEntersWithColorV1 { color } => {
            crate::effect::execute(&crate::standard_cards_v1::aura_enters(), ctx, state);
            set_entered_color(state, ctx.source, *color);
        }
        // Choices run in `drive`; the plain executor never reaches them.
        E6bOpV1::ConniveDiscardV1 | E6bOpV1::RumbleRevealV1 => halt(state, ctx.source),
    }
}

/// "As this Aura enters, choose a color": records the color on the Aura's
/// battlefield incarnation once it has entered.
fn set_entered_color(state: &mut GameState, aura: ObjectId, color: ManaColor) {
    if state.engine.halted.is_some() {
        return;
    }
    if let Some(live) = state.objects.try_get(aura) {
        if live.zone == Zone::Battlefield {
            state.objects.get_mut(aura).v4.chosen_color = Some(color);
        }
    }
}

/// Runs a leaf inside the resumable interpreter. `Ok(true)` when it staged a
/// choice and the continuation must suspend.
pub(crate) fn drive(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    op: &E6bOpV1,
    path: Vec<u16>,
) -> Result<bool, String> {
    let controller = continuation.ctx.controller;
    match op {
        E6bOpV1::ConniveDiscardV1 => {
            let hand = state.players[controller.index()]
                .hand
                .iter()
                .map(|&card| bind(state, card))
                .collect::<Vec<_>>();
            if hand.is_empty() {
                return Ok(false);
            }
            let remaining = continuation.frames.clone();
            crate::eternal_cards_v1::e3::offer(
                continuation,
                E3ChoiceV1 {
                    kind: E3ChoiceKindV1::E6bV1(E6bChoiceV1::ConniveDiscardV1),
                    player: controller,
                    candidates: hand
                        .iter()
                        .map(|binding| Target::Object(binding.object))
                        .collect(),
                    bindings: hand,
                    min: 1,
                    max: 1,
                    ordered: false,
                    path,
                    remaining,
                },
            );
            Ok(true)
        }
        E6bOpV1::RumbleRevealV1 => {
            let top = state.players[controller.index()]
                .library
                .iter()
                .take(4)
                .copied()
                .collect::<Vec<_>>();
            if top.is_empty() {
                return Ok(false);
            }
            for observer in [PlayerId::P0, PlayerId::P1] {
                state.reveal_library_top(observer, controller, top.len());
            }
            let revealed = top
                .iter()
                .map(|&card| bind(state, card))
                .collect::<Vec<_>>();
            let permanents = revealed
                .iter()
                .copied()
                .filter(|binding| definition(state, binding.object).is_some_and(is_permanent_card))
                .collect::<Vec<_>>();
            if permanents.is_empty() {
                rest_to_graveyard(state, &revealed, &[]);
                return Ok(false);
            }
            let remaining = continuation.frames.clone();
            crate::eternal_cards_v1::e3::offer(
                continuation,
                E3ChoiceV1 {
                    kind: E3ChoiceKindV1::E6bV1(E6bChoiceV1::RumblePickV1 { revealed }),
                    player: controller,
                    candidates: permanents
                        .iter()
                        .map(|binding| Target::Object(binding.object))
                        .collect(),
                    bindings: permanents,
                    min: 0,
                    max: 1,
                    ordered: false,
                    path,
                    remaining,
                },
            );
            Ok(true)
        }
        other => {
            execute(other, &continuation.ctx, state);
            Ok(false)
        }
    }
}

/// Moves the revealed cards still in their library, other than `kept`, to
/// their owner's graveyard together.
fn rest_to_graveyard(
    state: &mut GameState,
    revealed: &[EffectObjectBinding],
    kept: &[EffectObjectBinding],
) {
    let rest = revealed
        .iter()
        .filter(|binding| !kept.contains(binding))
        .filter(|binding| {
            link_live_in(
                state,
                ObjectLinkV4 {
                    object: binding.object,
                    zone_change_count: binding.expected_zone_change_count,
                },
                Zone::Library,
            )
        })
        .map(|binding| ProposedEvent::zone_change(binding.object, Zone::Graveyard))
        .collect::<Vec<_>>();
    if !rest.is_empty() {
        event::propose_and_commit_batch(state, rest);
    }
}

/// Applies an answered choice of this module.
pub(crate) fn finish_selection(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    choice: &E3ChoiceV1,
    kind: &E6bChoiceV1,
    bindings: Vec<EffectObjectBinding>,
) -> Result<bool, String> {
    let player = choice.player;
    match kind {
        E6bChoiceV1::ConniveDiscardV1 => {
            let [card] = bindings.as_slice() else {
                return Err("connive discards exactly one card".to_string());
            };
            let live = state.objects.get(card.object);
            if live.zone != Zone::Hand
                || live.zone_change_count != card.expected_zone_change_count
                || !state.players[player.index()].hand.contains(&card.object)
            {
                return Err("connive chose a card no longer in hand".to_string());
            }
            let nonland =
                definition(state, card.object).is_some_and(|def| !def.has_type(CardType::Land));
            crate::engine::commit_discarded_card(state, card.object);
            if nonland {
                if let Some(creature) = live_ability_source(&continuation.ctx, state) {
                    let _ = event::add_plus_one_counters(state, creature, player, 1);
                }
            }
            Ok(false)
        }
        E6bChoiceV1::RumblePickV1 { revealed } => {
            if bindings.len() > 1 || bindings.iter().any(|binding| !revealed.contains(binding)) {
                return Err("Malevolent Rumble chose a card it did not reveal".to_string());
            }
            for binding in &bindings {
                let card = ObjectLinkV4 {
                    object: binding.object,
                    zone_change_count: binding.expected_zone_change_count,
                };
                if !link_live_in(state, card, Zone::Library) {
                    return Err("Malevolent Rumble's card left the library".to_string());
                }
                event::propose_and_commit(
                    state,
                    ProposedEvent::zone_change(binding.object, Zone::Hand),
                );
            }
            rest_to_graveyard(state, revealed, &bindings);
            Ok(false)
        }
    }
}

// ---- Hooks: triggers ----------------------------------------------------------------

/// The triggers this module's hooks set off in this collection, and the
/// bookkeeping of sneak and disturb casts.
pub(crate) fn delayed_triggers(
    state: &mut GameState,
    events: &[CommittedEvent],
) -> Vec<PendingTrigger> {
    if !cfg!(feature = "eternal-fixtures") {
        return Vec::new();
    }
    note_alternative_casts(state, events);
    sneaked_creatures_attack(state, events);
    damage_life_triggers(state, events);
    prune(state);
    let Some(eternal) = state.eternal_v1.as_mut() else {
        return Vec::new();
    };
    if eternal.e6b.queued_triggers.is_empty() {
        return Vec::new();
    }
    std::mem::take(&mut eternal.e6b.queued_triggers)
        .into_iter()
        .map(|queued| {
            crate::eternal_cards_v1::delayed_trigger(
                queued.source,
                queued.controller,
                e6b(queued.op),
            )
        })
        .collect()
}

/// Sneak (Leonardo) and disturb (Lunarch Veteran) casts are their cards'
/// alternative costs.
fn note_alternative_casts(state: &mut GameState, events: &[CommittedEvent]) {
    for event in events {
        let CommittedEvent::SpellCast { spell, .. } = *event else {
            continue;
        };
        let Some(def) = definition(state, spell) else {
            continue;
        };
        let Some(condition) = def.alt_cost.map(|alt| alt.condition) else {
            continue;
        };
        if !matches!(
            condition,
            AltCostCondition::SneakV1 | AltCostCondition::DisturbV1
        ) {
            continue;
        }
        let alternative = state.stack.iter().any(|item| {
            item.kind == StackItemKind::Spell
                && item.source == spell
                && !item.is_copy
                && item.v4.cast_method == Some(CastMethodV4::Alternative)
        });
        if !alternative || state.objects.get(spell).zone != Zone::Stack {
            continue;
        }
        let link = live_link(state, spell);
        let e6b = e6b_state(state);
        let casts = if condition == AltCostCondition::SneakV1 {
            &mut e6b.sneak_casts
        } else {
            &mut e6b.disturb_casts
        };
        if !casts.contains(&link) {
            casts.push(link);
        }
    }
}

/// Whether `object` is on the stack as a spell cast for its sneak cost.
fn sneak_cast(state: &GameState, object: ObjectId) -> bool {
    state
        .objects
        .try_get(object)
        .is_some_and(|live| live.zone == Zone::Stack)
        && read_state(state).is_some_and(|e6b| e6b.sneak_casts.contains(&live_link(state, object)))
}

/// Whether `object` is on the stack as a spell cast for its disturb cost.
fn disturb_cast(state: &GameState, object: ObjectId) -> bool {
    state
        .objects
        .try_get(object)
        .is_some_and(|live| live.zone == Zone::Stack)
        && read_state(state)
            .is_some_and(|e6b| e6b.disturb_casts.contains(&live_link(state, object)))
}

/// Sneak (702.190b): a permanent spell whose sneak cost was paid enters
/// tapped (`adjust_zone_change`) and attacking.
fn sneaked_creatures_attack(state: &mut GameState, events: &[CommittedEvent]) {
    for event in events {
        let CommittedEvent::ZoneChange {
            object,
            from: Zone::Stack,
            to: Zone::Battlefield,
            ..
        } = *event
        else {
            continue;
        };
        let Some(live) = state.objects.try_get(object) else {
            continue;
        };
        let Some(stack) = live.zone_change_count.checked_sub(1) else {
            continue;
        };
        let cast = ObjectLinkV4 {
            object,
            zone_change_count: stack,
        };
        if live.zone != Zone::Battlefield
            || !read_state(state).is_some_and(|e6b| e6b.sneak_casts.contains(&cast))
        {
            continue;
        }
        let controller = live.controller;
        let combat = &mut state.engine.combat;
        if combat.attackers_declared
            && state.active_player == controller
            && matches!(state.step, Step::DeclareBlockers | Step::CombatDamage)
            && !combat.attackers.contains(&object)
        {
            combat.attackers.push(object);
        }
    }
}

/// Spirit Link and Armadillo Cloak: "Whenever enchanted creature deals
/// damage, you gain that much life." One trigger per Aura for the damage its
/// creature dealt in this batch (simultaneous combat damage is one event).
fn damage_life_triggers(state: &mut GameState, events: &[CommittedEvent]) {
    let auras = active_permanents(state, SPIRIT_LINK)
        .chain(active_permanents(state, ARMADILLO_CLOAK))
        .collect::<Vec<_>>();
    if auras.is_empty() {
        return;
    }
    let mut totals: Vec<(ObjectId, i32)> = Vec::new();
    for event in events {
        let CommittedEvent::Damage { source, amount, .. } = *event else {
            continue;
        };
        if amount <= 0
            || !state
                .objects
                .try_get(source)
                .is_some_and(|live| live.zone == Zone::Battlefield)
        {
            continue;
        }
        let creature = live_link(state, source);
        for &aura in &auras {
            if state.objects.get(aura).v4.attached_to != Some(creature) {
                continue;
            }
            match totals.iter_mut().find(|(seen, _)| *seen == aura) {
                Some((_, total)) => *total = total.saturating_add(amount),
                None => totals.push((aura, amount)),
            }
        }
    }
    for (aura, amount) in totals {
        let source = AbilitySourceContractV4::capture(state, aura);
        let controller = state.objects.get(aura).controller;
        queue_trigger(state, source, controller, E6bOpV1::GainLifeV1 { amount });
    }
}

/// Drops cast records whose spells left the stack, unless the spell just
/// entered the battlefield in this collection (handled above first).
fn prune(state: &mut GameState) {
    let Some(e6b) = read_state(state) else {
        return;
    };
    let keep = |links: &[ObjectLinkV4]| {
        links
            .iter()
            .copied()
            .filter(|&link| link_live_in(state, link, Zone::Stack))
            .collect::<Vec<_>>()
    };
    let sneak = keep(&e6b.sneak_casts);
    let disturb = keep(&e6b.disturb_casts);
    if sneak.len() != e6b.sneak_casts.len() || disturb.len() != e6b.disturb_casts.len() {
        let e6b = e6b_state(state);
        e6b.sneak_casts = sneak;
        e6b.disturb_casts = disturb;
    }
}

// ---- Hooks: engine ------------------------------------------------------------------

/// A zone change's replacements and entry characteristics: a disturbed
/// spell enters transformed and, like Luminous Phantom, is exiled instead
/// of going to a graveyard from anywhere; a sneaked spell enters tapped.
pub(crate) fn adjust_zone_change(state: &GameState, change: &mut event::ZoneChangeProposed) {
    let Some(live) = state.objects.try_get(change.object) else {
        return;
    };
    let veteran = definition_name(state, change.object) == Some(LUNARCH_VETERAN);
    if veteran && change.to_zone == Zone::Graveyard {
        let phantom = (live.zone == Zone::Battlefield && live.v4.face_index == 1)
            || (live.zone == Zone::Stack && disturb_cast(state, change.object));
        if phantom {
            change.to_zone = Zone::Exile;
        }
        return;
    }
    if change.to_zone != Zone::Battlefield || live.zone != Zone::Stack {
        return;
    }
    if veteran && disturb_cast(state, change.object) {
        change.battlefield_face_index = Some(1);
    }
    if sneak_cast(state, change.object) {
        change.force_battlefield_tapped = true;
    }
}

/// Whether a permanent observes declared attackers (Greatsword of Tyr's
/// "Whenever equipped creature attacks"), so their declarations are logged.
pub(crate) fn observes_attacks(state: &GameState) -> bool {
    active_permanents(state, GREATSWORD_OF_TYR).next().is_some()
        || crate::eternal_pauper_e8b_v1::observes_attacks(state)
}

/// Ancestral Mask: "+2/+2 for each other enchantment on the battlefield."
/// Ethereal Armor: "+1/+1 for each enchantment you control." The multiplier
/// of the Aura's printed bonus, or `None` for every other Aura.
pub(crate) fn aura_multiplier(state: &GameState, aura: ObjectId) -> Option<i128> {
    let name = definition_name(state, aura)?;
    let enchantment =
        |object: ObjectId| crate::engine::object_has_type(state, object, CardType::Enchantment);
    match name {
        ANCESTRAL_MASK => Some(
            battlefield(state)
                .filter(|&object| object != aura && enchantment(object))
                .count() as i128,
        ),
        ETHEREAL_ARMOR => {
            let controller = state.objects.get(aura).controller;
            Some(
                battlefield(state)
                    .filter(|&object| {
                        state.objects.get(object).controller == controller && enchantment(object)
                    })
                    .count() as i128,
            )
        }
        _ => None,
    }
}

/// Leonardo, Big Brother: "Leonardo gets +1/+0 for each other creature you
/// control."
pub(crate) fn controlled_boost(state: &GameState, object: ObjectId) -> (i128, i128) {
    if !active_named(state, object, LEONARDO) || !is_creature(state, object) {
        return (0, 0);
    }
    let controller = state.objects.get(object).controller;
    let others = battlefield(state)
        .filter(|&other| {
            other != object
                && state.objects.get(other).controller == controller
                && is_creature(state, other)
        })
        .count() as i128;
    (others, 0)
}

/// The colors Benevolent Blessings attached to `host` give it protection
/// from, as a color mask.
fn protection_mask(state: &GameState, host: ObjectId) -> u8 {
    // Sejiri Steppe's protection until end of turn (wave E9a).
    let granted = crate::eternal_legacy_e9a_v1::statics::protection_mask(state, host);
    attached_named(state, host, BENEVOLENT_BLESSING).fold(granted, |mask, blessing| {
        mask | state
            .objects
            .get(blessing)
            .v4
            .chosen_color
            .map_or(0, crate::card_def::mana_color_mask)
    })
}

/// Benevolent Blessing: whether `object` has protection from `source`'s
/// colors (it can't be targeted, blocked, enchanted or dealt damage by a
/// source of the chosen color).
pub(crate) fn protected_from_source(state: &GameState, object: ObjectId, source: ObjectId) -> bool {
    protected_from_colors(
        state,
        object,
        crate::engine::object_color_mask(state, source),
    )
}

fn protected_from_colors(state: &GameState, object: ObjectId, colors: u8) -> bool {
    colors & 0x1f != 0 && protection_mask(state, object) & colors != 0
}

/// Blocking restrictions: Silhana Ledgewalker "can't be blocked except by
/// creatures with flying", and Benevolent Blessing's protection.
pub(crate) fn blocker_allowed(state: &GameState, attacker: ObjectId, blocker: ObjectId) -> bool {
    (!active_named(state, attacker, SILHANA_LEDGEWALKER)
        || crate::engine::has_effective_keyword(state, blocker, Keywords::FLYING))
        && !protected_from_source(state, attacker, blocker)
}

/// 704.5m with Benevolent Blessing: an Aura of a color its host has
/// protection from is put into its owner's graveyard, unless the player who
/// controls that Blessing controls the Aura ("This effect doesn't remove
/// Auras and Equipment you control that are already attached to it").
pub(crate) fn protection_removes_attachment(
    state: &GameState,
    attachment: ObjectId,
    host: ObjectId,
) -> bool {
    let colors = crate::engine::object_color_mask(state, attachment);
    let controller = state.objects.get(attachment).controller;
    // Sejiri Steppe's protection removes every attachment of that color.
    if crate::eternal_legacy_e9a_v1::statics::protection_mask(state, host) & colors & 0x1f != 0 {
        return true;
    }
    attached_named(state, host, BENEVOLENT_BLESSING).any(|blessing| {
        let live = state.objects.get(blessing);
        live.controller != controller
            && live
                .v4
                .chosen_color
                .map_or(0, crate::card_def::mana_color_mask)
                & colors
                != 0
    })
}

/// The same rule for Equipment: unattaches those Benevolent Blessing's
/// protection no longer allows. Returns whether any moved.
pub(crate) fn unattach_protected_equipment(state: &mut GameState) -> bool {
    let detach = state
        .objects
        .iter()
        .filter_map(|(id, live)| {
            let definition = CARD_DEFS.get(usize::from(live.card_def))?;
            let link = live.v4.attached_to?;
            (live.zone == Zone::Battlefield
                && definition.equipment.is_some()
                && definition.subtypes.contains(&Subtype::Equipment)
                && link_live_in(state, link, Zone::Battlefield)
                && protection_removes_attachment(state, id, link.object))
            .then_some((id, link))
        })
        .collect::<Vec<_>>();
    for &(id, link) in &detach {
        state
            .objects
            .get_mut(link.object)
            .attachments
            .retain(|attachment| *attachment != id);
        state.objects.get_mut(id).v4.attached_to = None;
    }
    !detach.is_empty()
}

/// This module's Auras that the Standard Aura entry attaches: Cartouche of
/// Solidarity ("Enchant creature you control"), Abundant Growth ("Enchant
/// land") and Utopia Sprawl ("Enchant Forest").
pub(crate) fn is_aura(def: &CardDef) -> bool {
    matches!(
        def.name,
        CARTOUCHE_OF_SOLIDARITY | ABUNDANT_GROWTH | UTOPIA_SPRAWL
    ) || crate::eternal_e9b_v1::is_aura(def)
}

/// Whether this module's Aura `def`, controlled by `controller`, may enchant
/// `host`. `None` for every other definition.
pub(crate) fn aura_host_legal(
    def: &CardDef,
    controller: PlayerId,
    host: ObjectId,
    state: &GameState,
) -> Option<bool> {
    let live = state.objects.get(host);
    let on_battlefield = live.zone == Zone::Battlefield;
    let land = || crate::engine::object_has_type(state, host, CardType::Land);
    Some(match def.name {
        CARTOUCHE_OF_SOLIDARITY => {
            on_battlefield && live.controller == controller && is_creature(state, host)
        }
        ABUNDANT_GROWTH => on_battlefield && land(),
        UTOPIA_SPRAWL => {
            on_battlefield
                && land()
                && crate::engine::has_effective_subtype(state, host, Subtype::Forest)
        }
        _ => return crate::eternal_e9b_v1::aura_host_legal(def, host, state),
    })
}

/// Abundant Growth: 'Enchanted land has "{T}: Add one mana of any color."'
/// The colors the granted ability adds beyond the land's own primary mana
/// ability, which keeps adding its colors.
pub(crate) fn granted_mana_colors(state: &GameState, object: ObjectId) -> Vec<ManaColor> {
    if attached_named(state, object, ABUNDANT_GROWTH)
        .next()
        .is_none()
        || !crate::engine::object_has_type(state, object, CardType::Land)
    {
        return Vec::new();
    }
    let live = state.objects.get(object);
    let own = definition(state, object)
        .map(|def| {
            if crate::continuous_characteristics_v1::printed_abilities_active(state, object) {
                def.primary_mana_ability_choices(live.v4.chosen_color)
            } else {
                Vec::new()
            }
        })
        .unwrap_or_default();
    COLORS
        .into_iter()
        .filter(|color| !own.contains(color))
        .collect()
}

/// Utopia Sprawl: "Whenever enchanted Forest is tapped for mana, its
/// controller adds an additional one mana of the chosen color." A triggered
/// mana ability (605.1b): it resolves at once. Called after `land` was
/// tapped for mana.
pub(crate) fn after_tapped_for_mana(state: &mut GameState, land: ObjectId) {
    // Hickory Woodlot and Sandstone Needle remove a depletion counter.
    crate::eternal_pauper_e8b_v1::after_tapped_for_mana(state, land);
    // Wild Growth adds {G}.
    crate::eternal_e9b_v1::after_tapped_for_mana(state, land);
    // Eumidian Hatchery gets a hatchling counter (wave E9a).
    crate::eternal_legacy_e9a_v1::statics::after_tapped_for_mana(state, land);
    if state.objects.get(land).zone != Zone::Battlefield {
        return;
    }
    let colors = attached_named(state, land, UTOPIA_SPRAWL)
        .filter_map(|sprawl| state.objects.get(sprawl).v4.chosen_color)
        .collect::<Vec<_>>();
    if colors.is_empty() {
        return;
    }
    let controller = state.objects.get(land).controller;
    event::propose_and_commit(state, ProposedEvent::mana_add(controller, colors));
}

/// Whether `player` may cast the hand card `card` for its sneak cost now:
/// any time they have priority during the declare blockers step of their
/// own turn.
pub(crate) fn sneak_window(state: &GameState, card: ObjectId, player: PlayerId) -> bool {
    definition(state, card).is_some_and(|def| {
        def.alt_cost
            .is_some_and(|alt| alt.condition == AltCostCondition::SneakV1)
    }) && state.step == Step::DeclareBlockers
        && state.active_player == player
        && state.objects.get(card).owner == player
}

/// Sneak's cast timing for a cast already announced from hand.
pub(crate) fn sneak_pending_window(
    state: &GameState,
    pending: &crate::engine::PendingCast,
) -> bool {
    pending.origin_zone == Zone::Hand && sneak_window(state, pending.spell, pending.controller)
}

/// Disturb (702.146a): the owner of a Lunarch Veteran in their graveyard
/// may cast it transformed for its disturb cost. `zone_change_count` names
/// the graveyard incarnation; the permission also covers that card while
/// it is being cast and once it is on the stack.
pub(crate) fn disturb_permission(
    state: &GameState,
    object: ObjectId,
    holder: PlayerId,
    zone_change_count: u32,
) -> bool {
    let Some(live) = state.objects.try_get(object) else {
        return false;
    };
    let Some(def) = CARD_DEFS.get(live.card_def as usize) else {
        return false;
    };
    let disturb = def
        .alt_cost
        .is_some_and(|alt| alt.condition == AltCostCondition::DisturbV1);
    disturb
        && live.owner == holder
        && !live.v4.is_token
        && live.spell_copy_origin.is_none()
        && ((live.zone == Zone::Graveyard && live.zone_change_count == zone_change_count)
            || (live.zone == Zone::Stack
                && zone_change_count.checked_add(1) == Some(live.zone_change_count)))
}

/// The cost rules of sneak and disturb cards: during the declare blockers
/// step a sneak card is cast only for its sneak cost, and from the
/// graveyard a disturb card is cast only for its disturb cost unless
/// another permission (Lurrus of the Dream-Den) lets it be cast normally.
pub(crate) fn quote_allowed(
    state: &GameState,
    definition: &CardDef,
    source: ObjectId,
    origin_zone: Zone,
    method: CastMethodV4,
) -> bool {
    let Some(condition) = definition.alt_cost.map(|alt| alt.condition) else {
        return true;
    };
    if method == CastMethodV4::Alternative {
        return true;
    }
    match condition {
        AltCostCondition::SneakV1 => state.step != Step::DeclareBlockers,
        AltCostCondition::DisturbV1 if origin_zone == Zone::Graveyard => {
            let Some(live) = state.objects.try_get(source) else {
                return false;
            };
            let zone_change_count = if live.zone == Zone::Stack {
                live.zone_change_count.saturating_sub(1)
            } else {
                live.zone_change_count
            };
            crate::standard_cards_v1::standard_graveyard_cast_granted(
                state,
                source,
                live.owner,
                zone_change_count,
            ) || crate::eternal_lurrus_v1::lurrus_permission(
                state,
                source,
                live.owner,
                zone_change_count,
            )
        }
        _ => true,
    }
}

/// Demand Answers' sacrifice branch is its alternative cost, which replaces
/// the discard: whether a cast of `definition` (for its alternative cost if
/// `alternative`) skips its additional cost.
pub(crate) fn alternative_skips_additional_cost(definition: &CardDef, alternative: bool) -> bool {
    (alternative && definition.name == DEMAND_ANSWERS)
        || crate::eternal_legacy_e8a_v1::alternative_skips_additional_cost(definition, alternative)
}
