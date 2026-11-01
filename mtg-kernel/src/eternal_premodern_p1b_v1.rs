//! Card programs for the eternal catalog's Premodern wave P1b
//! (`docs/design/eternal_formats_support_v1.md`): Enchantress, Replenish,
//! Oath of Druids, Landstill, Psychatog and Grow.
//!
//! `eternal_cards_v1` forwards its effect leaves and state here, and the
//! engine calls the hooks in `statics` only in `eternal-fixtures` builds.
//! Every table is keyed by a printed name that occurs only in the eternal
//! registry, so the Pauper, FDN and Standard catalogs never reach it.
//! Behavior follows each card's current Oracle text, checked against its
//! XMage source.
//!
//! Primitives added here:
//! - Opalescence's type and base power/toughness change for the other
//!   non-Aura enchantments (layers 4 and 7b, with its timestamp);
//! - fading and fade counters, Parallax Wave's linked exile and its
//!   leave-the-battlefield return; treasure and depletion counters;
//! - costs paid as an ability is put on the stack: returning Attunement to
//!   its owner's hand, removing a fade counter; cycling triggers ("When you
//!   cycle this card") queued as the cycling ability is paid;
//! - "whenever you cast an enchantment spell", "when a player casts a
//!   spell" (Standstill) and "whenever an opponent casts a spell, counter
//!   that spell" (Decree of Silence), City of Brass's "becomes tapped";
//! - opponent choices during resolution: Fact or Fiction's piles and
//!   Intuition's pick;
//! - Replenish returning Auras attached to hosts chosen as they enter;
//! - Sickening Dreams' "discard X cards" announced like X; Solitary
//!   Confinement's skipped draw step, player shroud and damage prevention;
//!   Sterling Grove's granted shroud; Tsabo's Web's untap restriction;
//!   Aura of Silence's tax on opponents' artifact and enchantment spells.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

pub mod statics;
pub(crate) mod targets;

use crate::card_def::{card_id_by_name, CardDef, CardType, Subtype, TargetSpec, CARD_DEFS};
use crate::effect::{
    EffectContinuation, EffectFrame, EffectObjectBinding, EffectOp, ExecCtx, ObjectRef, PlayerRef,
};
use crate::eternal_cards_v1::e3::{E3ChoiceKindV1, E3ChoiceV1};
use crate::eternal_cards_v1::EternalOpV1;
use crate::event::{self, CommittedEvent, ProposedEvent};
use crate::ids::{ObjectId, PlayerId, StackItemId};
use crate::mana::ManaColor;
use crate::standard_cards_v1::StandardOpV1;
use crate::state::{AbilitySourceContractV4, GameState, ObjectLinkV4, StackItemKind, Target, Zone};
use crate::trigger::{PendingTrigger, TriggerCondition, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};

// ---- State ------------------------------------------------------------------------

/// Per-game state for this module's cards, kept inside
/// `eternal_cards_v1::EternalStateV1`. Default (and absent from snapshots)
/// until one of these cards needs it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct P1bStateV1 {
    /// Triggers detected by hooks, waiting for the next trigger collection.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub queued_triggers: Vec<QueuedTriggerV1>,
    /// Opalescence incarnations and the timestamp each entered with.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub opalescences: Vec<(ObjectLinkV4, u64)>,
    /// Fade counters on each Parallax Wave incarnation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fade_counters: Vec<(ObjectLinkV4, u8)>,
    /// Cards exiled with each Parallax Wave incarnation: the Wave's
    /// battlefield incarnation and the exiled card's exile incarnation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub wave_exiles: Vec<(ObjectLinkV4, ObjectLinkV4)>,
    /// Treasure counters on each Legacy's Allure incarnation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub treasure_counters: Vec<(ObjectLinkV4, u16)>,
    /// The treasure counters each sacrificed Legacy's Allure had, by its
    /// battlefield incarnation, for its ability's resolution.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sacrificed_allures: Vec<(ObjectLinkV4, u16)>,
    /// Depletion counters on each Decree of Silence incarnation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depletion_counters: Vec<(ObjectLinkV4, u8)>,
}

impl P1bStateV1 {
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

pub(crate) fn p1b_state(state: &mut GameState) -> &mut P1bStateV1 {
    &mut state.eternal_v1.get_or_insert_with(Default::default).p1b
}

pub(crate) fn read_state(state: &GameState) -> Option<&P1bStateV1> {
    state.eternal_v1.as_ref().map(|eternal| &eternal.p1b)
}

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::P1bV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum P1bOpV1 {
    /// "Destroy all creatures. They can't be regenerated."
    WrathOfGodV1,
    /// Decree of Justice: create X 4/4 Angels.
    DecreeAngelsV1,
    /// Decree of Justice's cycling trigger: "you may pay {X}". A choice of
    /// X among the amounts its controller can pay.
    DecreeSoldiersOfferV1,
    /// Pay {X} and create X 1/1 Soldiers.
    DecreeSoldiersV1 { x: u16 },
    /// Decree of Silence: counter the spell `spell` an opponent cast and
    /// put a depletion counter on the Decree; with three or more, sacrifice
    /// it.
    DecreeCounterV1 { spell: StackItemId },
    /// Fact or Fiction: reveal the top five; an opponent separates them.
    /// A choice.
    FactOrFictionV1,
    /// Intuition: search for three cards. A choice.
    IntuitionV1,
    /// Sterling Grove's search. A choice.
    SterlingGroveV1,
    /// Replenish. A choice for each Aura with a legal host.
    ReplenishV1,
    /// Sickening Dreams: X damage to each creature and each player.
    SickeningDreamsV1,
    /// Legacy's Allure: gain control of target 0.
    AllureControlV1,
    /// Legacy's Allure's upkeep counter.
    AllureCounterV1,
    /// Parallax Wave: exile target 0.
    WaveExileV1,
    /// Fading: remove a fade counter, or sacrifice the source.
    FadingUpkeepV1,
    /// Parallax Wave's return of the cards exiled with `wave`.
    WaveReturnV1 { wave: ObjectLinkV4 },
    /// Phyrexian Furnace: exile the bottom card of target player 0's
    /// graveyard.
    FurnaceBottomV1,
    /// Phyrexian Furnace: exile target card 0 from a graveyard.
    FurnaceExileTargetV1,
    /// Solitary Confinement's upkeep: sacrifice it unless its controller
    /// discards a card. May be a choice.
    ConfinementUpkeepV1,
    /// Sacrifice the resolving ability's source.
    SacrificeSourceV1,
    /// Standstill: sacrifice it; if sacrificed, each opponent of `caster`
    /// draws three cards.
    StandstillV1 { caster: PlayerId },
    /// City of Brass: 1 damage to its controller.
    CityDamageV1,
}

impl P1bOpV1 {
    /// Whether this leaf can yield a player decision or pushes frames, so
    /// its program must use the resumable interpreter.
    pub(crate) fn contains_player_choice(&self) -> bool {
        matches!(
            self,
            Self::DecreeSoldiersOfferV1
                | Self::FactOrFictionV1
                | Self::IntuitionV1
                | Self::SterlingGroveV1
                | Self::ReplenishV1
                | Self::ConfinementUpkeepV1
                | Self::StandstillV1 { .. }
        )
    }
}

/// Resolution choices owned by this module, carried as
/// `E3ChoiceKindV1::P1bV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum P1bChoiceV1 {
    /// Fact or Fiction: the opponent chooses the cards of one pile; the
    /// rest form the other.
    FactOrFictionSplitV1,
    /// Fact or Fiction: the controller picks a card; its pile goes to hand
    /// and the other to the graveyard. `pile` is the opponent's chosen pile,
    /// `rest` the other. When one pile is empty, choosing no card takes the
    /// empty pile.
    FactOrFictionPileV1 {
        pile: Vec<EffectObjectBinding>,
        rest: Vec<EffectObjectBinding>,
    },
    /// Intuition: the controller's three cards.
    IntuitionSearchV1,
    /// Intuition: the opponent's pick among `found`.
    IntuitionPickV1 { found: Vec<EffectObjectBinding> },
    /// Sterling Grove: up to one enchantment card.
    SterlingGroveSearchV1,
    /// Replenish: the host of `aura`. `auras` wait for theirs; `chosen`
    /// holds the hosts chosen so far; `rest` is every other enchantment card
    /// returning.
    ReplenishHostV1 {
        aura: EffectObjectBinding,
        auras: Vec<(EffectObjectBinding, Vec<EffectObjectBinding>)>,
        chosen: Vec<(EffectObjectBinding, EffectObjectBinding)>,
        rest: Vec<EffectObjectBinding>,
    },
}

impl P1bChoiceV1 {
    /// Whether the candidates are hidden from the other player.
    pub(crate) fn chooser_private(&self) -> bool {
        matches!(self, Self::IntuitionSearchV1 | Self::SterlingGroveSearchV1)
    }

    /// Whether the chooser sees library cards it has not looked at before.
    pub(crate) fn library_search(&self) -> bool {
        matches!(self, Self::IntuitionSearchV1 | Self::SterlingGroveSearchV1)
    }

    pub(crate) fn public_purpose(&self) -> crate::rl::TargetSelectionPurposeV4 {
        match self {
            Self::ReplenishHostV1 { .. } => crate::rl::TargetSelectionPurposeV4::PermanentSelection,
            _ => crate::rl::TargetSelectionPurposeV4::CardSelection,
        }
    }

    /// Object incarnations this choice refers to beyond its candidates.
    pub(crate) fn referenced_bindings(&self) -> Vec<EffectObjectBinding> {
        match self {
            Self::FactOrFictionPileV1 { pile, rest } => pile.iter().chain(rest).copied().collect(),
            Self::IntuitionPickV1 { found } => found.clone(),
            Self::ReplenishHostV1 {
                aura,
                auras,
                chosen,
                rest,
            } => {
                std::iter::once(*aura)
                    .chain(auras.iter().flat_map(|(aura, hosts)| {
                        std::iter::once(*aura).chain(hosts.iter().copied())
                    }))
                    .chain(chosen.iter().flat_map(|(aura, host)| [*aura, *host]))
                    .chain(rest.iter().copied())
                    .collect()
            }
            _ => Vec::new(),
        }
    }
}

// ---- Names and helpers --------------------------------------------------------------

pub(crate) const ARGOTHIAN_ENCHANTRESS: &str = "Argothian Enchantress";
pub(crate) const ATTUNEMENT: &str = "Attunement";
pub(crate) const AURA_OF_SILENCE: &str = "Aura of Silence";
pub(crate) const CITY_OF_BRASS: &str = "City of Brass";
pub(crate) const DECREE_OF_JUSTICE: &str = "Decree of Justice";
pub(crate) const DECREE_OF_SILENCE: &str = "Decree of Silence";
pub(crate) const ENCHANTRESSS_PRESENCE: &str = "Enchantress's Presence";
const ACCUMULATED_KNOWLEDGE: &str = "Accumulated Knowledge";
pub(crate) const LEGACYS_ALLURE: &str = "Legacy's Allure";
const MIRRIS_GUILE: &str = "Mirri's Guile";
pub(crate) const OPALESCENCE: &str = "Opalescence";
pub(crate) const PARALLAX_WAVE: &str = "Parallax Wave";
const QUIRION_DRYAD: &str = "Quirion Dryad";
pub(crate) const SICKENING_DREAMS: &str = "Sickening Dreams";
pub(crate) const SOLITARY_CONFINEMENT: &str = "Solitary Confinement";
pub(crate) const STANDSTILL: &str = "Standstill";
pub(crate) const STERLING_GROVE: &str = "Sterling Grove";
pub(crate) const TSABOS_WEB: &str = "Tsabo's Web";
const ANGEL_TOKEN: &str = "Angel Token";
const SOLDIER_TOKEN: &str = "Soldier Token";

/// Parallax Wave: "Fading 5".
const FADE_COUNTERS: u8 = 5;
/// The most X Decree of Justice's cycling trigger offers.
const DECREE_MAX_X: u16 = 20;

fn p1b(op: P1bOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::P1bV1(op)))
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

/// The resolving ability's source, if it is still the incarnation that
/// created the ability, on the battlefield.
fn live_ability_source(ctx: &ExecCtx, state: &GameState) -> Option<ObjectId> {
    let contract = ctx.ability_source_contract?;
    let live = state.objects.try_get(contract.source)?;
    (live.zone == Zone::Battlefield && live.zone_change_count == contract.zone_change_count)
        .then_some(contract.source)
}

/// The battlefield incarnation that created the resolving ability.
fn ability_source_link(ctx: &ExecCtx) -> Option<ObjectLinkV4> {
    let contract = ctx.ability_source_contract?;
    (contract.zone == Zone::Battlefield).then_some(ObjectLinkV4 {
        object: contract.source,
        zone_change_count: contract.zone_change_count,
    })
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
    p1b_state(state).queued_triggers.push(QueuedTriggerV1 {
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

/// Creates `count` tokens named `name` for the ability's controller.
fn create_tokens(ctx: &ExecCtx, state: &mut GameState, name: &str, count: u16) {
    let token = token_id(name);
    for _ in 0..count {
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

fn push_frame(continuation: &mut EffectContinuation, op: EffectOp, path: &[u16]) {
    continuation.frames.push(EffectFrame::Program {
        op,
        path: path.to_vec(),
    });
}

/// Counters `item` without targeting it, if it is still a spell on the
/// stack that can be countered.
fn counter_spell_item(state: &mut GameState, item: StackItemId, source: ObjectId) {
    let Some(spell) = state
        .stack
        .iter()
        .find(|entry| entry.v4.stack_item_id == item && entry.kind == StackItemKind::Spell)
        .map(|entry| entry.source)
    else {
        return;
    };
    let uncounterable = definition(state, spell).is_some_and(|def| def.spell_cannot_be_countered)
        || crate::eternal_cards_v1::e3::spell_cannot_be_countered(state, spell);
    if uncounterable {
        return;
    }
    if crate::engine::counter_stack_item_by_id(state, item).is_err() {
        halt(state, source);
    }
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

fn draw(count: u32) -> EffectOp {
    EffectOp::DrawCards {
        player: PlayerRef::Controller,
        count,
    }
}

// ---- Spells -------------------------------------------------------------------------

/// Accumulated Knowledge: "Draw a card, then draw cards equal to the number
/// of cards named Accumulated Knowledge in all graveyards." Both draws are
/// draw leaves, so dredge can replace them.
pub fn accumulated_knowledge() -> EffectOp {
    EffectOp::Sequence(vec![
        draw(1),
        EffectOp::DrawCardsDynamic {
            player: PlayerRef::Controller,
            count: crate::card_def::DynamicValueDef::GraveyardCardsNamedV1(token_id(
                ACCUMULATED_KNOWLEDGE,
            )),
        },
    ])
}

/// Decree of Justice: "Create X 4/4 white Angel creature tokens with
/// flying."
pub fn decree_of_justice() -> EffectOp {
    p1b(P1bOpV1::DecreeAngelsV1)
}

/// Fact or Fiction: "Reveal the top five cards of your library. An opponent
/// separates those cards into two piles. Put one pile into your hand and
/// the other into your graveyard."
pub fn fact_or_fiction() -> EffectOp {
    p1b(P1bOpV1::FactOrFictionV1)
}

/// Frantic Search: "Draw two cards, then discard two cards. Untap up to
/// three lands." The lands are chosen before the discard: the two choices
/// are independent and neither reveals anything the other could use, and
/// the discard stays the terminal leaf.
pub fn frantic_search() -> EffectOp {
    EffectOp::Sequence(vec![
        draw(2),
        EffectOp::UntapUpToLands {
            chooser: PlayerRef::Controller,
            max_targets: 3,
        },
        EffectOp::DiscardCards {
            player: PlayerRef::Controller,
            count: 2,
        },
    ])
}

/// Intuition: "Search your library for three cards and reveal them. Target
/// opponent chooses one of them. Put the chosen card into your hand and the
/// rest into your graveyard. Then shuffle."
pub fn intuition() -> EffectOp {
    p1b(P1bOpV1::IntuitionV1)
}

/// Replenish: "Return all enchantment cards from your graveyard to the
/// battlefield. (Auras with nothing to enchant remain in your graveyard.)"
pub fn replenish() -> EffectOp {
    p1b(P1bOpV1::ReplenishV1)
}

/// Rewind: "Counter target spell. Untap up to four lands."
pub fn rewind() -> EffectOp {
    EffectOp::Sequence(vec![
        counter_target_spell(),
        EffectOp::UntapUpToLands {
            chooser: PlayerRef::Controller,
            max_targets: 4,
        },
    ])
}

/// Sickening Dreams: "deals X damage to each creature and each player." X is
/// the number of cards discarded for its additional cost.
pub fn sickening_dreams() -> EffectOp {
    p1b(P1bOpV1::SickeningDreamsV1)
}

/// Wrath of God: "Destroy all creatures. They can't be regenerated."
pub fn wrath_of_god() -> EffectOp {
    p1b(P1bOpV1::WrathOfGodV1)
}

// ---- Activated abilities --------------------------------------------------------------

/// Attunement: "Draw three cards, then discard four cards." The return to
/// hand is its cost.
pub fn attunement() -> EffectOp {
    EffectOp::Sequence(vec![
        draw(3),
        EffectOp::DiscardCards {
            player: PlayerRef::Controller,
            count: 4,
        },
    ])
}

/// Legacy's Allure: "Gain control of target creature with power less than
/// or equal to the number of treasure counters on this enchantment."
pub fn legacys_allure() -> EffectOp {
    p1b(P1bOpV1::AllureControlV1)
}

/// Parallax Wave: "Exile target creature."
pub fn parallax_wave_exile() -> EffectOp {
    p1b(P1bOpV1::WaveExileV1)
}

/// Phyrexian Furnace: "Exile the bottom card of target player's graveyard."
pub fn furnace_exile_bottom() -> EffectOp {
    p1b(P1bOpV1::FurnaceBottomV1)
}

/// Phyrexian Furnace: "Exile target card from a graveyard. Draw a card."
pub fn furnace_exile_and_draw() -> EffectOp {
    EffectOp::Sequence(vec![p1b(P1bOpV1::FurnaceExileTargetV1), draw(1)])
}

/// Sterling Grove: "Search your library for an enchantment card and reveal
/// that card. Shuffle, then put the card on top."
pub fn sterling_grove_search() -> EffectOp {
    p1b(P1bOpV1::SterlingGroveV1)
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

const YOUR_UPKEEP: TriggerCondition = TriggerCondition::BeginningOfUpkeep {
    controller_only: true,
};

/// Quirion Dryad: "Whenever you cast a white, blue, black, or red spell".
const WHITE_BLUE_BLACK_OR_RED: u8 = crate::card_def::mana_color_mask(ManaColor::W)
    | crate::card_def::mana_color_mask(ManaColor::U)
    | crate::card_def::mana_color_mask(ManaColor::B)
    | crate::card_def::mana_color_mask(ManaColor::R);

/// "put a +1/+1 counter on this creature."
fn dryad_counter() -> EffectOp {
    EffectOp::AddPlusOnePlusOneCounters {
        object: ObjectRef::ThisSource,
        count: 1,
    }
}

/// Legacy's Allure: "you may put a treasure counter on this enchantment."
fn allure_upkeep() -> EffectOp {
    EffectOp::Choice {
        controller: PlayerRef::Controller,
        options: vec![EffectOp::Sequence(vec![]), p1b(P1bOpV1::AllureCounterV1)],
    }
}

/// Mirri's Guile: "you may look at the top three cards of your library,
/// then put them back in any order."
fn mirris_guile_upkeep() -> EffectOp {
    EffectOp::Choice {
        controller: PlayerRef::Controller,
        options: vec![
            EffectOp::Sequence(vec![]),
            EffectOp::LookAtLibraryTopAndReorder {
                player: PlayerRef::Controller,
                count: 3,
            },
        ],
    }
}

/// Fading: "remove a fade counter from it. If you can't, sacrifice it."
fn fading_upkeep() -> EffectOp {
    p1b(P1bOpV1::FadingUpkeepV1)
}

/// Solitary Confinement: "sacrifice this enchantment unless you discard a
/// card."
fn confinement_upkeep() -> EffectOp {
    p1b(P1bOpV1::ConfinementUpkeepV1)
}

/// Tsabo's Web: "draw a card."
fn draw_one() -> EffectOp {
    draw(1)
}

const DRYAD_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(
    TriggerCondition::CastSpellWithAnyColor(WHITE_BLUE_BLACK_OR_RED),
    dryad_counter,
)];
const ALLURE_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(YOUR_UPKEEP, allure_upkeep)];
const MIRRIS_GUILE_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(YOUR_UPKEEP, mirris_guile_upkeep)];
const WAVE_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(YOUR_UPKEEP, fading_upkeep)];
const CONFINEMENT_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(YOUR_UPKEEP, confinement_upkeep)];
const TSABOS_WEB_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(TriggerCondition::Etb, draw_one)];

/// Triggered abilities of this module's cards.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        QUIRION_DRYAD => &DRYAD_TRIGGERS,
        LEGACYS_ALLURE => &ALLURE_TRIGGERS,
        MIRRIS_GUILE => &MIRRIS_GUILE_TRIGGERS,
        PARALLAX_WAVE => &WAVE_TRIGGERS,
        SOLITARY_CONFINEMENT => &CONFINEMENT_TRIGGERS,
        TSABOS_WEB => &TSABOS_WEB_TRIGGERS,
        _ => &[],
    }
}

fn own_trigger_effect(name: &str, effect: &EffectOp) -> bool {
    triggers_for(name)
        .iter()
        .any(|trigger| (trigger.effect)() == *effect)
        || delayed_effect_matches(name, effect)
}

/// Decree of Silence's cycling trigger: "you may counter target spell."
fn decree_of_silence_cycled() -> EffectOp {
    EffectOp::Choice {
        controller: PlayerRef::Controller,
        options: vec![EffectOp::Sequence(vec![]), counter_target_spell()],
    }
}

/// Target specs of this module's triggered abilities, per ability.
pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    if !own_trigger_effect(name, effect) {
        return None;
    }
    Some(match name {
        DECREE_OF_SILENCE if *effect == decree_of_silence_cycled() => TargetSpec::AnySpellOnStack,
        _ => TargetSpec::None,
    })
}

/// Queued triggers owned by this module's cards.
pub(crate) fn delayed_effect_matches(name: &str, effect: &EffectOp) -> bool {
    if name == ARGOTHIAN_ENCHANTRESS || name == ENCHANTRESSS_PRESENCE {
        return *effect == draw(1);
    }
    if name == DECREE_OF_SILENCE && *effect == decree_of_silence_cycled() {
        return true;
    }
    let EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::P1bV1(op))) = effect else {
        return false;
    };
    match name {
        DECREE_OF_JUSTICE => matches!(op, P1bOpV1::DecreeSoldiersOfferV1),
        DECREE_OF_SILENCE => matches!(op, P1bOpV1::DecreeCounterV1 { .. }),
        PARALLAX_WAVE => matches!(op, P1bOpV1::WaveReturnV1 { .. }),
        STANDSTILL => matches!(op, P1bOpV1::StandstillV1 { .. }),
        CITY_OF_BRASS => matches!(op, P1bOpV1::CityDamageV1),
        _ => false,
    }
}

// ---- Effect leaves ------------------------------------------------------------------

pub(crate) fn execute(op: &P1bOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        P1bOpV1::WrathOfGodV1 => {
            // "They can't be regenerated": no regeneration shield applies.
            let creatures = battlefield(state)
                .filter(|&object| has_type(state, object, CardType::Creature))
                .filter(|&object| {
                    !crate::engine::has_effective_keyword(
                        state,
                        object,
                        crate::card_def::Keywords::INDESTRUCTIBLE,
                    )
                })
                .collect::<Vec<_>>();
            event::propose_and_commit_batch(
                state,
                creatures
                    .into_iter()
                    .map(|object| ProposedEvent::zone_change(object, Zone::Graveyard))
                    .collect(),
            );
        }
        P1bOpV1::DecreeAngelsV1 => {
            create_tokens(ctx, state, ANGEL_TOKEN, ctx.x_value);
        }
        P1bOpV1::DecreeSoldiersV1 { x } => {
            let cost = crate::mana::Cost {
                pips: &[],
                generic: u8::try_from(*x).unwrap_or(u8::MAX),
                x_count: 0,
            };
            let Some(plan) = crate::mana::can_pay(&cost, 0, ctx.controller, state) else {
                return;
            };
            crate::engine::pay_plan(state, ctx.controller, &plan);
            create_tokens(ctx, state, SOLDIER_TOKEN, *x);
        }
        P1bOpV1::DecreeCounterV1 { spell } => {
            counter_spell_item(state, *spell, ctx.source);
            // "and put a depletion counter on this enchantment. If there are
            // three or more depletion counters on it, sacrifice it."
            let Some(decree) = live_ability_source(ctx, state) else {
                return;
            };
            let link = live_link(state, decree);
            let counters = &mut p1b_state(state).depletion_counters;
            let count = match counters.iter_mut().find(|(host, _)| *host == link) {
                Some(entry) => {
                    entry.1 = entry.1.saturating_add(1);
                    entry.1
                }
                None => {
                    counters.push((link, 1));
                    1
                }
            };
            if count >= 3 {
                sacrifice(state, decree, ctx.controller);
            }
        }
        P1bOpV1::SickeningDreamsV1 => {
            let amount = i32::from(ctx.x_value);
            if amount <= 0 {
                return;
            }
            let mut damage = battlefield(state)
                .filter(|&object| has_type(state, object, CardType::Creature))
                .map(|object| event::damage_from_exec_ctx_v1(ctx, Target::Object(object), amount))
                .collect::<Vec<_>>();
            for player in [ctx.controller, ctx.controller.opponent()] {
                damage.push(event::damage_from_exec_ctx_v1(
                    ctx,
                    Target::Player(player),
                    amount,
                ));
            }
            event::propose_and_commit_batch(state, damage);
        }
        P1bOpV1::AllureControlV1 => {
            let spec = TargetSpec::StandardV1(
                crate::standard_cards_v1::StandardTargetV1::CreaturePowerAtMostTreasureCountersV1,
            );
            let Some(creature) = legal_object_target(ctx, state, 0, spec) else {
                return;
            };
            let counters = ability_source_link(ctx)
                .map_or(0, |allure| statics::allure_counters_for_link(state, allure));
            if crate::engine::effective_power(state, creature) > i32::from(counters) {
                return;
            }
            crate::standard_cards_v1::gain_control(state, creature, ctx.controller);
        }
        P1bOpV1::AllureCounterV1 => {
            let Some(allure) = live_ability_source(ctx, state) else {
                return;
            };
            let link = live_link(state, allure);
            let counters = &mut p1b_state(state).treasure_counters;
            match counters.iter_mut().find(|(host, _)| *host == link) {
                Some(entry) => entry.1 = entry.1.saturating_add(1),
                None => counters.push((link, 1)),
            }
        }
        P1bOpV1::WaveExileV1 => {
            let Some(creature) = legal_object_target(ctx, state, 0, TargetSpec::Creature) else {
                return;
            };
            let Some(wave) = ability_source_link(ctx) else {
                return;
            };
            event::propose_and_commit(state, ProposedEvent::zone_change(creature, Zone::Exile));
            let live = state.objects.get(creature);
            // A token ceases to exist in exile and never returns.
            if live.zone != Zone::Exile || live.v4.is_token {
                return;
            }
            let exiled = live_link(state, creature);
            p1b_state(state).wave_exiles.push((wave, exiled));
        }
        P1bOpV1::FadingUpkeepV1 => {
            let Some(wave) = live_ability_source(ctx, state) else {
                return;
            };
            let link = live_link(state, wave);
            let removed = p1b_state(state)
                .fade_counters
                .iter_mut()
                .find(|(host, count)| *host == link && *count > 0)
                .map(|entry| entry.1 -= 1)
                .is_some();
            if !removed {
                sacrifice(state, wave, ctx.controller);
            }
        }
        P1bOpV1::WaveReturnV1 { wave } => {
            // "each player returns to the battlefield all cards they own
            // exiled with this enchantment."
            let Some(p1b) = state.eternal_v1.as_mut().map(|eternal| &mut eternal.p1b) else {
                return;
            };
            let (mine, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut p1b.wave_exiles)
                .into_iter()
                .partition(|(host, _)| host == wave);
            p1b.wave_exiles = rest;
            let returning = mine
                .into_iter()
                .map(|(_, card)| card)
                .filter(|&card| link_live_in(state, card, Zone::Exile))
                .map(|card| {
                    let owner = state.objects.get(card.object).owner;
                    ProposedEvent::zone_change_to_battlefield_under_controller(card.object, owner)
                })
                .collect::<Vec<_>>();
            event::propose_and_commit_batch(state, returning);
        }
        P1bOpV1::FurnaceBottomV1 => {
            let Some(player) = legal_player_target(ctx, state, 0, TargetSpec::AnyPlayer) else {
                return;
            };
            let Some(&bottom) = state.players[player.index()].graveyard.first() else {
                return;
            };
            event::propose_and_commit(state, ProposedEvent::zone_change(bottom, Zone::Exile));
        }
        P1bOpV1::FurnaceExileTargetV1 => {
            let spec = TargetSpec::StandardV1(
                crate::standard_cards_v1::StandardTargetV1::CardInAGraveyard,
            );
            if let Some(card) = legal_object_target(ctx, state, 0, spec) {
                event::propose_and_commit(state, ProposedEvent::zone_change(card, Zone::Exile));
            }
        }
        P1bOpV1::SacrificeSourceV1 => {
            if let Some(source) = live_ability_source(ctx, state) {
                sacrifice(state, source, ctx.controller);
            }
        }
        P1bOpV1::CityDamageV1 => {
            event::propose_and_commit(
                state,
                event::damage_from_exec_ctx_v1(ctx, Target::Player(ctx.controller), 1),
            );
        }
        // Choices and frame-pushing leaves run in `drive`.
        P1bOpV1::DecreeSoldiersOfferV1
        | P1bOpV1::FactOrFictionV1
        | P1bOpV1::IntuitionV1
        | P1bOpV1::SterlingGroveV1
        | P1bOpV1::ReplenishV1
        | P1bOpV1::ConfinementUpkeepV1
        | P1bOpV1::StandstillV1 { .. } => halt(state, ctx.source),
    }
}

fn targets_of(bindings: &[EffectObjectBinding]) -> Vec<Target> {
    bindings
        .iter()
        .map(|binding| Target::Object(binding.object))
        .collect()
}

/// Stages one of this module's selections over `candidates`.
#[allow(clippy::too_many_arguments)]
fn offer(
    continuation: &mut EffectContinuation,
    kind: P1bChoiceV1,
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
            kind: E3ChoiceKindV1::P1bV1(kind),
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

fn count_u16(count: usize) -> u16 {
    u16::try_from(count).unwrap_or(u16::MAX)
}

/// Whether `def` is an Aura card.
fn is_aura(def: &CardDef) -> bool {
    def.subtypes.contains(&Subtype::Aura)
}

/// The battlefield permanents the Aura card `aura` (in its owner's
/// graveyard) could enchant as it enters under `controller`'s control,
/// without targeting (303.4a). An Aura with no enchant rule this catalog
/// models has none, so it stays in the graveyard.
fn replenish_hosts(
    state: &GameState,
    aura: EffectObjectBinding,
    controller: PlayerId,
) -> Vec<EffectObjectBinding> {
    let Some(def) = definition(state, aura.object) else {
        return Vec::new();
    };
    if def.attachment.is_some() {
        return crate::effect::returning_aura_hosts(state, aura).unwrap_or_default();
    }
    if !crate::standard_cards_v1::is_aura(def) {
        return Vec::new();
    }
    battlefield(state)
        .filter(|&host| {
            crate::standard_cards_v1::aura_host_legal(def, controller, host, state) == Some(true)
        })
        .map(|host| bind(state, host))
        .collect()
}

/// Puts Replenish's cards onto the battlefield: every non-Aura card
/// together, then each Aura attached to its chosen host.
fn replenish_return(
    state: &mut GameState,
    rest: &[EffectObjectBinding],
    chosen: &[(EffectObjectBinding, EffectObjectBinding)],
) -> Result<(), String> {
    event::propose_and_commit_batch(
        state,
        rest.iter()
            .filter(|&&card| binding_live(state, card))
            .map(|card| ProposedEvent::zone_change(card.object, Zone::Battlefield))
            .collect(),
    );
    for &(aura, host) in chosen {
        if !binding_live(state, aura) || !binding_live(state, host) {
            continue;
        }
        event::propose_and_commit(
            state,
            ProposedEvent::zone_change(aura.object, Zone::Battlefield),
        );
        let live = state.objects.get(aura.object);
        if live.zone != Zone::Battlefield {
            continue;
        }
        let incarnation = live.zone_change_count;
        state
            .attach_object_exact(
                aura.object,
                incarnation,
                host.object,
                host.expected_zone_change_count,
            )
            .map_err(|_| "Replenish's Aura could not attach to its chosen host".to_string())?;
    }
    Ok(())
}

/// Offers the host of the next Aura in `auras`, or returns everything.
fn replenish_next(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    player: PlayerId,
    mut auras: Vec<(EffectObjectBinding, Vec<EffectObjectBinding>)>,
    chosen: Vec<(EffectObjectBinding, EffectObjectBinding)>,
    rest: Vec<EffectObjectBinding>,
    path: Vec<u16>,
) -> Result<bool, String> {
    while !auras.is_empty() {
        let (aura, hosts) = auras.remove(0);
        let hosts = hosts
            .into_iter()
            .filter(|&host| binding_live(state, host))
            .collect::<Vec<_>>();
        if hosts.is_empty() || !binding_live(state, aura) {
            continue;
        }
        offer(
            continuation,
            P1bChoiceV1::ReplenishHostV1 {
                aura,
                auras,
                chosen,
                rest,
            },
            player,
            hosts,
            1,
            1,
            path,
        );
        return Ok(true);
    }
    replenish_return(state, &rest, &chosen)?;
    Ok(false)
}

/// The most X `player` can pay for Decree of Justice's cycling trigger.
fn payable_x(state: &GameState, player: PlayerId) -> u16 {
    (1..=DECREE_MAX_X)
        .take_while(|&x| {
            let cost = crate::mana::Cost {
                pips: &[],
                generic: u8::try_from(x).unwrap_or(u8::MAX),
                x_count: 0,
            };
            crate::mana::can_pay(&cost, 0, player, state).is_some()
        })
        .last()
        .unwrap_or(0)
}

/// Runs a leaf inside the resumable interpreter. `Ok(true)` when it staged a
/// choice and the continuation must suspend.
pub(crate) fn drive(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    op: &P1bOpV1,
    path: Vec<u16>,
) -> Result<bool, String> {
    let controller = continuation.ctx.controller;
    match op {
        P1bOpV1::DecreeSoldiersOfferV1 => {
            let maximum = payable_x(state, controller);
            if maximum == 0 {
                return Ok(false);
            }
            let mut options = vec![EffectOp::Sequence(vec![])];
            options.extend((1..=maximum).map(|x| p1b(P1bOpV1::DecreeSoldiersV1 { x })));
            push_frame(
                continuation,
                EffectOp::Choice {
                    controller: PlayerRef::Controller,
                    options,
                },
                &path,
            );
            Ok(false)
        }
        P1bOpV1::StandstillV1 { caster } => {
            let Some(standstill) = live_ability_source(&continuation.ctx, state) else {
                return Ok(false);
            };
            if !sacrifice(state, standstill, controller) {
                return Ok(false);
            }
            // "each of that player's opponents draws three cards."
            let player = if caster.opponent() == controller {
                PlayerRef::Controller
            } else {
                PlayerRef::Opponent
            };
            push_frame(
                continuation,
                EffectOp::DrawCards { player, count: 3 },
                &path,
            );
            Ok(false)
        }
        P1bOpV1::ConfinementUpkeepV1 => {
            let Some(confinement) = live_ability_source(&continuation.ctx, state) else {
                return Ok(false);
            };
            if state.players[controller.index()].hand.is_empty() {
                sacrifice(state, confinement, controller);
                return Ok(false);
            }
            push_frame(
                continuation,
                EffectOp::Choice {
                    controller: PlayerRef::Controller,
                    options: vec![
                        p1b(P1bOpV1::SacrificeSourceV1),
                        EffectOp::DiscardCards {
                            player: PlayerRef::Controller,
                            count: 1,
                        },
                    ],
                },
                &path,
            );
            Ok(false)
        }
        P1bOpV1::FactOrFictionV1 => {
            let top = state.players[controller.index()]
                .library
                .iter()
                .copied()
                .take(5)
                .collect::<Vec<_>>();
            if top.is_empty() {
                return Ok(false);
            }
            for observer in [PlayerId::P0, PlayerId::P1] {
                state.reveal_library_top(observer, controller, top.len());
            }
            let bindings = top
                .iter()
                .map(|&card| bind(state, card))
                .collect::<Vec<_>>();
            let max = count_u16(bindings.len());
            offer(
                continuation,
                P1bChoiceV1::FactOrFictionSplitV1,
                controller.opponent(),
                bindings,
                0,
                max,
                path,
            );
            Ok(true)
        }
        P1bOpV1::IntuitionV1 => {
            let ctx = continuation.ctx.clone();
            if legal_player_target(&ctx, state, 0, TargetSpec::TargetOpponent).is_none() {
                return Ok(false);
            }
            let library = state.players[controller.index()]
                .library
                .iter()
                .map(|&card| bind(state, card))
                .collect::<Vec<_>>();
            if library.is_empty() {
                shuffle(state, controller)?;
                return Ok(false);
            }
            // A search for cards with no stated quality finds as many as it
            // can (701.19b).
            let need = count_u16(library.len().min(3));
            offer(
                continuation,
                P1bChoiceV1::IntuitionSearchV1,
                controller,
                library,
                need,
                need,
                path,
            );
            Ok(true)
        }
        P1bOpV1::SterlingGroveV1 => {
            let cards = state.players[controller.index()]
                .library
                .iter()
                .copied()
                .filter(|&card| {
                    definition(state, card).is_some_and(|def| def.has_type(CardType::Enchantment))
                })
                .map(|card| bind(state, card))
                .collect::<Vec<_>>();
            if cards.is_empty() {
                shuffle(state, controller)?;
                return Ok(false);
            }
            offer(
                continuation,
                P1bChoiceV1::SterlingGroveSearchV1,
                controller,
                cards,
                0,
                1,
                path,
            );
            Ok(true)
        }
        P1bOpV1::ReplenishV1 => {
            let cards = state.players[controller.index()]
                .graveyard
                .iter()
                .copied()
                .filter(|&card| {
                    let live = state.objects.get(card);
                    live.owner == controller
                        && !live.v4.is_token
                        && definition(state, card).is_some_and(|def| {
                            def.is_executable() && def.has_type(CardType::Enchantment)
                        })
                })
                .map(|card| bind(state, card))
                .collect::<Vec<_>>();
            let (auras, rest): (Vec<_>, Vec<_>) = cards
                .into_iter()
                .partition(|card| definition(state, card.object).is_some_and(is_aura));
            // Hosts are chosen among the permanents already on the
            // battlefield, before the cards enter together.
            let auras = auras
                .into_iter()
                .map(|aura| (aura, replenish_hosts(state, aura, controller)))
                .collect::<Vec<_>>();
            replenish_next(
                state,
                continuation,
                controller,
                auras,
                Vec::new(),
                rest,
                path,
            )
        }
        other => {
            execute(other, &continuation.ctx, state);
            Ok(false)
        }
    }
}

/// Moves `cards` still in their library incarnation to `zone`, as one batch.
fn move_cards(state: &mut GameState, cards: &[EffectObjectBinding], zone: Zone) {
    let events = cards
        .iter()
        .filter(|&&card| binding_live(state, card))
        .map(|card| ProposedEvent::zone_change(card.object, zone))
        .collect::<Vec<_>>();
    event::propose_and_commit_batch(state, events);
}

/// Applies an answered choice of this module.
pub(crate) fn finish_selection(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    choice: &E3ChoiceV1,
    kind: &P1bChoiceV1,
    bindings: Vec<EffectObjectBinding>,
) -> Result<bool, String> {
    let controller = continuation.ctx.controller;
    match kind {
        P1bChoiceV1::FactOrFictionSplitV1 => {
            let rest = choice
                .bindings
                .iter()
                .copied()
                .filter(|card| !bindings.contains(card))
                .collect::<Vec<_>>();
            // The controller points at a card of the pile they take. With an
            // empty pile, choosing no card takes it.
            let (candidates, min) = if bindings.is_empty() {
                (rest.clone(), 0)
            } else if rest.is_empty() {
                (bindings.clone(), 0)
            } else {
                (choice.bindings.clone(), 1)
            };
            offer(
                continuation,
                P1bChoiceV1::FactOrFictionPileV1 {
                    pile: bindings,
                    rest,
                },
                controller,
                candidates,
                min,
                1,
                choice.path.clone(),
            );
            Ok(true)
        }
        P1bChoiceV1::FactOrFictionPileV1 { pile, rest } => {
            let (to_hand, to_graveyard) = match bindings.first() {
                Some(card) if pile.contains(card) => (pile, rest),
                Some(_) => (rest, pile),
                // No card chosen: the empty pile goes to hand.
                None if pile.is_empty() => (pile, rest),
                None => (rest, pile),
            };
            move_cards(state, to_hand, Zone::Hand);
            move_cards(state, to_graveyard, Zone::Graveyard);
            Ok(false)
        }
        P1bChoiceV1::IntuitionSearchV1 => {
            let opponent = controller.opponent();
            // "reveal them": each found card's library position is known to
            // the opponent.
            for card in &bindings {
                if let Some(position) = state.players[controller.index()]
                    .library
                    .iter()
                    .position(|&object| object == card.object)
                {
                    state.reveal_library_position(opponent, controller, position);
                }
            }
            offer(
                continuation,
                P1bChoiceV1::IntuitionPickV1 {
                    found: bindings.clone(),
                },
                opponent,
                bindings,
                1,
                1,
                choice.path.clone(),
            );
            Ok(true)
        }
        P1bChoiceV1::IntuitionPickV1 { found } => {
            let picked = bindings.first().copied();
            let rest = found
                .iter()
                .copied()
                .filter(|card| Some(*card) != picked)
                .collect::<Vec<_>>();
            if let Some(card) = picked {
                move_cards(state, &[card], Zone::Hand);
            }
            move_cards(state, &rest, Zone::Graveyard);
            shuffle(state, controller)?;
            Ok(false)
        }
        P1bChoiceV1::SterlingGroveSearchV1 => {
            shuffle(state, controller)?;
            let Some(card) = bindings.first().copied() else {
                return Ok(false);
            };
            // "Shuffle, then put the card on top." The revealed card stays a
            // library card; its new position is public.
            let library = &mut state.players[controller.index()].library;
            let Some(position) = library.iter().position(|&object| object == card.object) else {
                return Ok(false);
            };
            let object = library.remove(position);
            library.insert(0, object);
            for observer in [PlayerId::P0, PlayerId::P1] {
                state.reveal_library_position(observer, controller, 0);
            }
            Ok(false)
        }
        P1bChoiceV1::ReplenishHostV1 {
            aura,
            auras,
            chosen,
            rest,
        } => {
            let mut chosen = chosen.clone();
            if let Some(host) = bindings.first().copied() {
                chosen.push((*aura, host));
            }
            replenish_next(
                state,
                continuation,
                controller,
                auras.clone(),
                chosen,
                rest.clone(),
                choice.path.clone(),
            )
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
    note_departures(state, events);
    note_taps(state, events);
    prune(state);
    let Some(eternal) = state.eternal_v1.as_mut() else {
        return Vec::new();
    };
    if eternal.p1b.queued_triggers.is_empty() {
        return Vec::new();
    }
    std::mem::take(&mut eternal.p1b.queued_triggers)
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

/// Spells cast: the enchantresses' draws, Standstill and Decree of
/// Silence.
fn note_spell_casts(state: &mut GameState, events: &[CommittedEvent]) {
    for event in events {
        let CommittedEvent::SpellCast { spell, controller } = *event else {
            continue;
        };
        let Some(item) = state
            .stack
            .iter()
            .find(|item| item.kind == StackItemKind::Spell && item.source == spell)
            .map(|item| item.v4.stack_item_id)
        else {
            continue;
        };
        // "Whenever you cast an enchantment spell, draw a card."
        if crate::trigger::selected_spell_types(state, spell).contains(&CardType::Enchantment) {
            let drawers = active_permanents(state, ARGOTHIAN_ENCHANTRESS, Some(controller))
                .chain(active_permanents(
                    state,
                    ENCHANTRESSS_PRESENCE,
                    Some(controller),
                ))
                .collect::<Vec<_>>();
            for drawer in drawers {
                let source = AbilitySourceContractV4::capture(state, drawer);
                queue_trigger(state, source, controller, draw(1), None);
            }
        }
        // Standstill: "When a player casts a spell".
        let standstills = active_permanents(state, STANDSTILL, None).collect::<Vec<_>>();
        for standstill in standstills {
            let source = AbilitySourceContractV4::capture(state, standstill);
            let owner = state.objects.get(standstill).controller;
            queue_trigger(
                state,
                source,
                owner,
                p1b(P1bOpV1::StandstillV1 { caster: controller }),
                None,
            );
        }
        // Decree of Silence: "Whenever an opponent casts a spell".
        let decrees = active_permanents(state, DECREE_OF_SILENCE, Some(controller.opponent()))
            .collect::<Vec<_>>();
        for decree in decrees {
            let source = AbilitySourceContractV4::capture(state, decree);
            queue_trigger(
                state,
                source,
                controller.opponent(),
                p1b(P1bOpV1::DecreeCounterV1 { spell: item }),
                None,
            );
        }
    }
}

/// Permanents entering: Opalescence's timestamp.
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
            || definition_name(state, object) != Some(OPALESCENCE)
        {
            continue;
        }
        let link = live_link(state, object);
        if read_state(state)
            .is_some_and(|p1b| p1b.opalescences.iter().any(|(host, _)| *host == link))
        {
            continue;
        }
        let timestamp = crate::engine::next_timestamp(state);
        p1b_state(state).opalescences.push((link, timestamp));
    }
}

/// Permanents leaving the battlefield: Parallax Wave's return.
fn note_departures(state: &mut GameState, events: &[CommittedEvent]) {
    for event in events {
        let CommittedEvent::ZoneChange {
            object,
            from: Zone::Battlefield,
            controller_before,
            ..
        } = *event
        else {
            continue;
        };
        let Some(live) = state.objects.try_get(object) else {
            continue;
        };
        let departed = ObjectLinkV4 {
            object,
            zone_change_count: live.zone_change_count.saturating_sub(1),
        };
        // "When this enchantment leaves the battlefield, each player returns
        // to the battlefield all cards they own exiled with it."
        if definition_name(state, object) == Some(PARALLAX_WAVE) && live.v4.face_index == 0 {
            let source = AbilitySourceContractV4::capture(state, object);
            queue_trigger(
                state,
                source,
                controller_before,
                p1b(P1bOpV1::WaveReturnV1 { wave: departed }),
                None,
            );
        }
    }
}

/// City of Brass: "Whenever this land becomes tapped, it deals 1 damage to
/// you."
fn note_taps(state: &mut GameState, events: &[CommittedEvent]) {
    for event in events {
        let CommittedEvent::Tap { object } = *event else {
            continue;
        };
        if !active_named(state, object, CITY_OF_BRASS) {
            continue;
        }
        let source = AbilitySourceContractV4::capture(state, object);
        let controller = state.objects.get(object).controller;
        queue_trigger(state, source, controller, p1b(P1bOpV1::CityDamageV1), None);
    }
}

/// Drops records whose objects moved on.
fn prune(state: &mut GameState) {
    let Some(p1b) = read_state(state) else {
        return;
    };
    if p1b.is_empty() {
        return;
    }
    let on_battlefield = |link: &ObjectLinkV4| link_live_in(state, *link, Zone::Battlefield);
    let opalescences = p1b
        .opalescences
        .iter()
        .filter(|(link, _)| on_battlefield(link))
        .copied()
        .collect::<Vec<_>>();
    let fade_counters = p1b
        .fade_counters
        .iter()
        .filter(|(link, _)| on_battlefield(link))
        .copied()
        .collect::<Vec<_>>();
    let treasure_counters = p1b
        .treasure_counters
        .iter()
        .filter(|(link, _)| on_battlefield(link))
        .copied()
        .collect::<Vec<_>>();
    let depletion_counters = p1b
        .depletion_counters
        .iter()
        .filter(|(link, _)| on_battlefield(link))
        .copied()
        .collect::<Vec<_>>();
    // Exiled cards stay linked while they remain in exile.
    let wave_exiles = p1b
        .wave_exiles
        .iter()
        .filter(|(_, card)| link_live_in(state, *card, Zone::Exile))
        .copied()
        .collect::<Vec<_>>();
    let p1b = p1b_state(state);
    p1b.opalescences = opalescences;
    p1b.fade_counters = fade_counters;
    p1b.treasure_counters = treasure_counters;
    p1b.depletion_counters = depletion_counters;
    p1b.wave_exiles = wave_exiles;
}
