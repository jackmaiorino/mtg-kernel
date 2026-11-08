//! Card programs for the eternal catalog's Premodern wave P2c
//! (`docs/design/eternal_formats_support_v1.md`): the Reanimator and Full
//! English Breakfast cards.
//!
//! Every table is keyed by a printed name that occurs only in the eternal
//! registry, so the Pauper, FDN and Standard catalogs never reach it.
//! `eternal_cards_v1` asks this module first, then falls through to the
//! earlier waves; the engine calls the hooks in `statics` only in
//! `eternal-fixtures` builds. State stays absent (`state.eternal_v1` is not
//! created) until one of these cards needs it.
//!
//! Primitives added here:
//! - Undiscovered Paradise's return to hand during its controller's next
//!   untap step, Reflecting Pool's colors (the types a land its controller
//!   controls could produce), Hermit Druid's reveal until a basic land;
//! - Exhume's choice by each player from their own graveyard (the
//!   Show and Tell shape), Death's reanimation with life loss, Life's
//!   animation of every land its controller controls;
//! - Petradon's linked exile of two lands and its return when it leaves,
//!   Phantom Nishoba's damage prevention that removes a counter, Phyrexian
//!   Devourer's state trigger and its cost that exiles the top card of the
//!   library, Battlefield Scrounger's cost that moves graveyard cards to the
//!   library bottom, Putrid Imp's threshold and Avatar of Woe's fear and
//!   conditional cost reduction;
//! - echo (Karmic Guide) generalizes wave P2d's.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

pub mod statics;
pub(crate) mod targets;

use crate::card_def::{card_id_by_name, CardType, Keywords, Supertype, TargetSpec, CARD_DEFS};
use crate::effect::{
    EffectContinuation, EffectObjectBinding, EffectOp, ExecCtx, ObjectRef, PlayerRef,
};
use crate::eternal_cards_v1::e3::{E3ChoiceKindV1, E3ChoiceV1};
use crate::eternal_cards_v1::EternalOpV1;
use crate::eternal_legacy_e9a_v1::{live_link, TurnV1};
use crate::eternal_premodern_p1a_v1::{
    destroy, forbid_regeneration, halt, legal_object_target, live_ability_source, queue_trigger,
};
use crate::event::{self, CommittedEvent, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::standard_cards_v1::{StandardOpV1, StandardTargetV1};
use crate::state::{AbilitySourceContractV4, GameState, ObjectLinkV4, Target, Zone};
use crate::trigger::{TriggerCondition, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};

pub(crate) const PALINCHRON: &str = "Palinchron";
pub(crate) const TREVAS_RUINS: &str = "Treva's Ruins";
pub(crate) const VERDANT_FORCE: &str = "Verdant Force";
pub(crate) const AKROMA: &str = "Akroma, Angel of Wrath";
pub(crate) const TRISKELION: &str = "Triskelion";
pub(crate) const KARMIC_GUIDE: &str = "Karmic Guide";
pub(crate) const PETRADON: &str = "Petradon";
pub(crate) const PHANTOM_NISHOBA: &str = "Phantom Nishoba";
pub(crate) const PHYREXIAN_DEVOURER: &str = "Phyrexian Devourer";
pub(crate) const UNDISCOVERED_PARADISE: &str = "Undiscovered Paradise";
pub(crate) const REFLECTING_POOL: &str = "Reflecting Pool";
pub(crate) const AVATAR_OF_WOE: &str = "Avatar of Woe";
pub(crate) const PUTRID_IMP: &str = "Putrid Imp";
pub(crate) const BATTLEFIELD_SCROUNGER: &str = "Battlefield Scrounger";
pub(crate) const LIFE_DEATH: &str = "Life // Death";
const SAPROLING_TOKEN: &str = "Saproling Token";

// ---- State ------------------------------------------------------------------------

/// Per-game state for this module's cards, kept inside
/// `eternal_cards_v1::EternalStateV1`. Default (and absent from snapshots)
/// until one of these cards needs it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct P2cStateV1 {
    /// Undiscovered Paradise incarnations tapped for mana that return to
    /// their owner's hand in their controller's next untap step.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub paradise_returns: Vec<ObjectLinkV4>,
    /// Each Petradon incarnation with the lands it exiled (their exile
    /// incarnations).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub petradon_exiled: Vec<(ObjectLinkV4, Vec<ObjectLinkV4>)>,
    /// Lands animated by Life this turn, with the layer timestamp.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub life_lands: Vec<(ObjectLinkV4, TurnV1, u64)>,
    /// Phyrexian Devourer incarnations whose state trigger is on the stack
    /// or queued.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub devourer_triggered: Vec<ObjectLinkV4>,
    /// While a batch of simultaneous damage is being replaced: the Phantom
    /// Nishoba incarnations that already lost their counter for it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nishoba_batch: Option<Vec<ObjectLinkV4>>,
}

impl P2cStateV1 {
    pub(crate) fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

pub(crate) fn p2c_state(state: &mut GameState) -> &mut P2cStateV1 {
    &mut state.eternal_v1.get_or_insert_with(Default::default).p2c
}

pub(crate) fn read_state(state: &GameState) -> Option<&P2cStateV1> {
    state.eternal_v1.as_ref().map(|eternal| &eternal.p2c)
}

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::P2cV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum P2cOpV1 {
    /// Exhume: each player puts a creature card from their graveyard onto
    /// the battlefield. A choice.
    ExhumeV1,
    /// Death: return target creature card 0 from your graveyard to the
    /// battlefield; you lose life equal to its mana value.
    DeathV1,
    /// Life: all lands you control become 1/1 creatures until end of turn.
    LifeV1,
    /// Avatar of Woe: destroy target creature 0; it can't be regenerated.
    DestroyNoRegenV1,
    /// Hermit Druid: reveal until a basic land; it goes to hand, the rest to
    /// the graveyard.
    HermitDruidV1,
    /// Krosan Reclamation: target player 0 shuffles up to two target cards
    /// (slots 1 and 2) from their graveyard into their library.
    ReclamationV1,
    /// Putrid Imp: it gains flying until end of turn.
    ImpFlyingV1,
    /// Petradon: exile target lands 0 and 1.
    PetradonExileV1,
    /// Petradon: return the exiled cards to the battlefield.
    PetradonReturnV1,
    /// Phyrexian Devourer: put X +1/+1 counters on it, X the mana value of
    /// the card its cost exiled.
    DevourerCountersV1,
    /// Sacrifice the resolving ability's source.
    SacrificeSourceV1,
    /// Palinchron: return the resolving ability's source to its owner's hand
    /// if that incarnation is still on the battlefield.
    ReturnSourceV1,
    /// Phantom Nishoba: gain `amount` life.
    NishobaLifeV1 { amount: i32 },
}

impl P2cOpV1 {
    /// Whether this leaf can yield a player decision by itself, so its
    /// program must use the resumable interpreter.
    pub(crate) fn contains_player_choice(&self) -> bool {
        matches!(self, Self::ExhumeV1)
    }
}

/// Resolution choices owned by this module, carried as
/// `E3ChoiceKindV1::P2cV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum P2cChoiceV1 {
    /// Exhume: the creature card the chooser puts onto the battlefield.
    /// `chosen` are the earlier players' cards, `later` the players still to
    /// choose.
    ExhumeV1 {
        chosen: Vec<EffectObjectBinding>,
        later: Vec<PlayerId>,
    },
}

impl P2cChoiceV1 {
    /// Whether the candidates are hidden from the other player: graveyards
    /// are public.
    pub(crate) fn chooser_private(&self) -> bool {
        false
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
        match self {
            Self::ExhumeV1 { chosen, .. } => chosen.clone(),
        }
    }
}

fn p2c(op: P2cOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::P2cV1(op)))
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
// ---- Programs named by the card database ----------------------------------------------

/// Unearth: "Return target creature card with mana value 3 or less from your
/// graveyard to the battlefield."
pub fn unearth() -> EffectOp {
    EffectOp::MoveObject {
        object: ObjectRef::Target(0),
        to_zone: Zone::Battlefield,
    }
}

/// Careful Study: "Draw two cards, then discard two cards."
pub fn careful_study() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 2,
        },
        EffectOp::DiscardCards {
            player: PlayerRef::Controller,
            count: 2,
        },
    ])
}

/// Palinchron: "Return Palinchron to its owner's hand."
pub fn return_source_to_hand() -> EffectOp {
    p2c(P2cOpV1::ReturnSourceV1)
}

/// Exhume: "Each player puts a creature card from their graveyard onto the
/// battlefield."
pub fn exhume() -> EffectOp {
    p2c(P2cOpV1::ExhumeV1)
}

/// Death (Life // Death): "Return target creature card from your graveyard
/// to the battlefield. You lose life equal to its mana value."
pub fn death() -> EffectOp {
    p2c(P2cOpV1::DeathV1)
}

/// Life (Life // Death): "All lands you control become 1/1 creatures until
/// end of turn. They're still lands."
pub fn life() -> EffectOp {
    p2c(P2cOpV1::LifeV1)
}

/// Avatar of Woe: "{T}: Destroy target creature. It can't be regenerated."
pub fn destroy_target_creature_no_regeneration() -> EffectOp {
    p2c(P2cOpV1::DestroyNoRegenV1)
}

/// Hermit Druid: "{G}, {T}: Reveal cards from the top of your library until
/// you reveal a basic land card. Put that card into your hand and all other
/// cards revealed this way into your graveyard."
pub fn hermit_druid() -> EffectOp {
    p2c(P2cOpV1::HermitDruidV1)
}

/// Krosan Reclamation: "Target player shuffles up to two target cards from
/// their graveyard into their library."
pub fn reclamation() -> EffectOp {
    p2c(P2cOpV1::ReclamationV1)
}

/// Putrid Imp: "Discard a card: Putrid Imp gains flying until end of turn."
pub fn imp_flying() -> EffectOp {
    p2c(P2cOpV1::ImpFlyingV1)
}

/// Phyrexian Devourer: "Exile the top card of your library: Put X +1/+1
/// counters on Phyrexian Devourer, where X is the exiled card's mana value."
pub fn devourer_counters() -> EffectOp {
    p2c(P2cOpV1::DevourerCountersV1)
}

// ---- Triggered abilities ----------------------------------------------------------------

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

/// Palinchron: "When this creature enters, untap up to seven lands."
fn palinchron_enters() -> EffectOp {
    EffectOp::UntapUpToLands {
        chooser: PlayerRef::Controller,
        max_targets: 7,
    }
}

/// Treva's Ruins: "When this land enters, sacrifice it unless you return a
/// non-Lair land you control to its owner's hand." Rith's Grove's entry
/// (wave P1a) is not specific to that card.
fn ruins_enters() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::P1aV1(
        crate::eternal_premodern_p1a_v1::P1aOpV1::GroveV1,
    )))
}

/// Verdant Force: "At the beginning of each upkeep, create a 1/1 green
/// Saproling creature token."
fn force_upkeep() -> EffectOp {
    EffectOp::CreateToken {
        token_def: card_id_by_name(SAPROLING_TOKEN)
            .unwrap_or_else(|| panic!("{SAPROLING_TOKEN} in CARD_DEFS")),
        controller: PlayerRef::Controller,
    }
}

/// Karmic Guide: "When this creature enters, return target creature card
/// from your graveyard to the battlefield."
fn guide_enters() -> EffectOp {
    EffectOp::MoveObject {
        object: ObjectRef::Target(0),
        to_zone: Zone::Battlefield,
    }
}

/// Petradon: "When this creature enters, exile two target lands."
fn petradon_enters() -> EffectOp {
    p2c(P2cOpV1::PetradonExileV1)
}

/// Petradon: "When this creature leaves the battlefield, return the exiled
/// cards to the battlefield under their owners' control."
fn petradon_leaves() -> EffectOp {
    p2c(P2cOpV1::PetradonReturnV1)
}

/// Phyrexian Devourer's state trigger: "When this creature's power is 7 or
/// greater, sacrifice it."
fn devourer_sacrifice() -> EffectOp {
    p2c(P2cOpV1::SacrificeSourceV1)
}

const PALINCHRON_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, palinchron_enters)];
const RUINS_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(TriggerCondition::Etb, ruins_enters)];
const FORCE_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(
    TriggerCondition::BeginningOfUpkeep {
        controller_only: false,
    },
    force_upkeep,
)];
const GUIDE_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(TriggerCondition::Etb, guide_enters)];
const PETRADON_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(TriggerCondition::Etb, petradon_enters),
    trigger(TriggerCondition::LeftBattlefield, petradon_leaves),
];

/// Triggered abilities of this module's cards; empty for every other name.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        PALINCHRON => &PALINCHRON_TRIGGERS,
        TREVAS_RUINS => &RUINS_TRIGGERS,
        VERDANT_FORCE => &FORCE_TRIGGERS,
        KARMIC_GUIDE => &GUIDE_TRIGGERS,
        PETRADON => &PETRADON_TRIGGERS,
        _ => &[],
    }
}

/// Target specs of this module's triggered abilities, and of the triggers
/// its hooks queue.
pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    if name == KARMIC_GUIDE && *effect == guide_enters() {
        return Some(TargetSpec::CreatureCardInOwnGraveyard);
    }
    if name == PETRADON && *effect == petradon_enters() {
        return Some(TargetSpec::StandardV1(StandardTargetV1::TwoLandsV1));
    }
    if delayed_effect_matches(name, effect) {
        return Some(TargetSpec::None);
    }
    triggers_for(name)
        .iter()
        .any(|trigger| (trigger.effect)() == *effect)
        .then_some(TargetSpec::None)
}

/// Queued triggers owned by this module's cards.
pub(crate) fn delayed_effect_matches(name: &str, effect: &EffectOp) -> bool {
    match name {
        PHANTOM_NISHOBA => matches!(
            effect,
            EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::P2cV1(
                P2cOpV1::NishobaLifeV1 { .. }
            )))
        ),
        PHYREXIAN_DEVOURER => *effect == devourer_sacrifice(),
        _ => false,
    }
}
// ---- Effect leaves ------------------------------------------------------------------

pub(crate) fn execute(op: &P2cOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        P2cOpV1::DeathV1 => {
            let Some(card) =
                legal_object_target(ctx, state, 0, TargetSpec::CreatureCardInOwnGraveyard)
            else {
                return;
            };
            let value = CARD_DEFS
                .get(usize::from(state.objects.get(card).card_def))
                .map_or(0, |def| i32::from(def.mana_value));
            event::propose_and_commit(state, ProposedEvent::zone_change(card, Zone::Battlefield));
            if value > 0 {
                event::propose_and_commit(state, ProposedEvent::life_loss(ctx.controller, value));
            }
        }
        P2cOpV1::LifeV1 => {
            let lands = state.players[ctx.controller.index()]
                .battlefield
                .iter()
                .copied()
                .filter(|&object| {
                    state.objects.get(object).zone == Zone::Battlefield
                        && state.objects.get(object).controller == ctx.controller
                        && crate::engine::object_has_type(state, object, CardType::Land)
                })
                .collect::<Vec<_>>();
            if lands.is_empty() {
                return;
            }
            let timestamp = crate::engine::next_timestamp(state);
            let now = TurnV1::now(state);
            let records = lands
                .into_iter()
                .map(|object| (live_link(state, object), now, timestamp))
                .collect::<Vec<_>>();
            let life_lands = &mut p2c_state(state).life_lands;
            for record in records {
                life_lands.retain(|(link, _, _)| *link != record.0);
                life_lands.push(record);
            }
        }
        P2cOpV1::DestroyNoRegenV1 => {
            let Some(object) = legal_object_target(ctx, state, 0, TargetSpec::Creature) else {
                return;
            };
            // "It can't be regenerated" for this destruction only.
            let record = (live_link(state, object), TurnV1::now(state));
            let added =
                !crate::eternal_premodern_p1a_v1::statics::cant_be_regenerated(state, object);
            if added {
                forbid_regeneration(state, object);
            }
            destroy(ctx, state, object);
            if added {
                crate::eternal_premodern_p1a_v1::p1a_state(state)
                    .no_regeneration
                    .retain(|entry| *entry != record);
            }
        }
        P2cOpV1::HermitDruidV1 => hermit_druid_reveal(ctx, state),
        P2cOpV1::ReclamationV1 => {
            crate::eternal_premodern_p2a_v1::shuffle_target_cards(
                ctx,
                state,
                TargetSpec::StandardV1(StandardTargetV1::PlayerThenUpToTwoOfTheirGraveyardCardsV1),
            );
        }
        P2cOpV1::ImpFlyingV1 => {
            if let Some(imp) = live_ability_source(ctx, state) {
                let binding = bind(state, imp);
                crate::effect::install_temporary_boost(state, binding, 0, 0, Keywords::FLYING);
            }
        }
        P2cOpV1::PetradonExileV1 => petradon_exile(ctx, state),
        P2cOpV1::PetradonReturnV1 => petradon_return(ctx, state),
        P2cOpV1::DevourerCountersV1 => {
            let Some(devourer) = live_ability_source(ctx, state) else {
                return;
            };
            let value = ctx
                .paid_cost_refs
                .first()
                .and_then(|paid| CARD_DEFS.get(usize::from(paid.card_def)))
                .map_or(0, |def| i32::from(def.mana_value));
            if value > 0
                && event::add_plus_one_counters(state, devourer, ctx.controller, value).is_err()
            {
                halt(state, ctx.source);
            }
        }
        P2cOpV1::SacrificeSourceV1 => {
            let Some(source) = live_ability_source(ctx, state) else {
                return;
            };
            if state.objects.get(source).controller != ctx.controller {
                return;
            }
            event::log_sacrifice(state, source);
            event::propose_and_commit(state, ProposedEvent::zone_change(source, Zone::Graveyard));
        }
        P2cOpV1::ReturnSourceV1 => {
            if let Some(source) = live_ability_source(ctx, state) {
                event::propose_and_commit(state, ProposedEvent::zone_change(source, Zone::Hand));
            }
        }
        P2cOpV1::NishobaLifeV1 { amount } => {
            if *amount > 0 {
                event::propose_and_commit(state, ProposedEvent::life_gain(ctx.controller, *amount));
            }
        }
        // Choices run in `drive`.
        P2cOpV1::ExhumeV1 => halt(state, ctx.source),
    }
}

/// Hermit Druid: reveal cards from the top of the library until a basic land
/// card is revealed (or the library ends); the land goes to hand, the other
/// revealed cards to the graveyard.
fn hermit_druid_reveal(ctx: &ExecCtx, state: &mut GameState) {
    let controller = ctx.controller;
    let library = state.players[controller.index()].library.clone();
    let mut land = None;
    let mut count = 0;
    for (index, &card) in library.iter().enumerate() {
        count = index + 1;
        let basic_land = CARD_DEFS
            .get(usize::from(state.objects.get(card).card_def))
            .is_some_and(|def| {
                def.has_type(CardType::Land) && def.supertypes.contains(&Supertype::Basic)
            });
        if basic_land {
            land = Some(card);
            break;
        }
    }
    if count == 0 {
        return;
    }
    // spellbench-public-reveal: public reveal to both players (Hermit Druid).
    for observer in [PlayerId::P0, PlayerId::P1] {
        state.reveal_library_top(observer, controller, count);
    }
    let events = library
        .iter()
        .take(count)
        .map(|&card| {
            if Some(card) == land {
                // Revealed: both players keep its identity in hand.
                ProposedEvent::zone_change_preserving_known_identity(card, Zone::Hand)
            } else {
                ProposedEvent::zone_change(card, Zone::Graveyard)
            }
        })
        .collect::<Vec<_>>();
    event::propose_and_commit_batch(state, events);
}

/// Petradon's enters trigger: exile the two target lands, and remember them
/// with this Petradon incarnation if it is still on the battlefield.
fn petradon_exile(ctx: &ExecCtx, state: &mut GameState) {
    let spec = TargetSpec::StandardV1(StandardTargetV1::TwoLandsV1);
    let mut lands = Vec::new();
    for index in 0..2 {
        if let Some(land) = legal_object_target(ctx, state, index, spec) {
            if !lands.contains(&land) {
                lands.push(land);
            }
        }
    }
    if lands.is_empty() {
        return;
    }
    event::propose_and_commit_batch(
        state,
        lands
            .iter()
            .map(|&land| ProposedEvent::zone_change(land, Zone::Exile))
            .collect(),
    );
    // If Petradon already left, nothing will return the lands.
    let Some(petradon) = live_ability_source(ctx, state) else {
        return;
    };
    let exiled = lands
        .into_iter()
        .filter(|&land| state.objects.get(land).zone == Zone::Exile)
        .map(|land| live_link(state, land))
        .collect::<Vec<_>>();
    if !exiled.is_empty() {
        let link = live_link(state, petradon);
        p2c_state(state).petradon_exiled.push((link, exiled));
    }
}

/// Petradon's leaves trigger: the exiled cards return under their owners'
/// control.
fn petradon_return(ctx: &ExecCtx, state: &mut GameState) {
    let Some(contract) = ctx.ability_source_contract else {
        return;
    };
    let link = ObjectLinkV4 {
        object: contract.source,
        zone_change_count: contract.zone_change_count,
    };
    let Some(p2c) = read_state(state) else {
        return;
    };
    let Some(position) = p2c
        .petradon_exiled
        .iter()
        .position(|(source, _)| *source == link)
    else {
        return;
    };
    let (_, exiled) = p2c_state(state).petradon_exiled.remove(position);
    let events = exiled
        .into_iter()
        .filter(|&land| crate::eternal_legacy_e9a_v1::link_live_in(state, land, Zone::Exile))
        .map(|land| {
            let owner = state.objects.get(land.object).owner;
            ProposedEvent::zone_change_to_battlefield_under_controller(land.object, owner)
        })
        .collect::<Vec<_>>();
    if !events.is_empty() {
        event::propose_and_commit_batch(state, events);
    }
}
// ---- Choices ----------------------------------------------------------------------------

/// The creature cards in `player`'s graveyard (cards, not tokens).
fn graveyard_creature_cards(state: &GameState, player: PlayerId) -> Vec<EffectObjectBinding> {
    state.players[player.index()]
        .graveyard
        .iter()
        .copied()
        .filter(|&card| {
            let live = state.objects.get(card);
            live.zone == Zone::Graveyard
                && live.owner == player
                && !live.v4.is_token
                && CARD_DEFS
                    .get(usize::from(live.card_def))
                    .is_some_and(|def| def.has_type(CardType::Creature))
        })
        .map(|card| bind(state, card))
        .collect()
}

/// Stages one of this module's selections over `bindings`.
fn offer(
    continuation: &mut EffectContinuation,
    kind: P2cChoiceV1,
    player: PlayerId,
    bindings: Vec<EffectObjectBinding>,
    path: Vec<u16>,
) {
    let remaining = continuation.frames.clone();
    crate::eternal_cards_v1::e3::offer(
        continuation,
        E3ChoiceV1 {
            kind: E3ChoiceKindV1::P2cV1(kind),
            player,
            candidates: bindings
                .iter()
                .map(|binding| Target::Object(binding.object))
                .collect(),
            bindings,
            min: 1,
            max: 1,
            ordered: false,
            path,
            remaining,
        },
    );
}

/// Exhume (101.4): the active player chooses first, then the other player;
/// a graveyard with exactly one creature card has nothing to choose. Then
/// the chosen cards enter at once.
fn stage_exhume(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    players: &[PlayerId],
    mut chosen: Vec<EffectObjectBinding>,
    path: Vec<u16>,
) -> Result<bool, String> {
    for (index, &player) in players.iter().enumerate() {
        let cards = graveyard_creature_cards(state, player);
        match cards.len() {
            0 => {}
            1 => chosen.push(cards[0]),
            _ => {
                offer(
                    continuation,
                    P2cChoiceV1::ExhumeV1 {
                        chosen,
                        later: players[index + 1..].to_vec(),
                    },
                    player,
                    cards,
                    path,
                );
                return Ok(true);
            }
        }
    }
    let events = chosen
        .into_iter()
        .filter(|&binding| binding_live(state, binding))
        .map(|binding| ProposedEvent::zone_change(binding.object, Zone::Battlefield))
        .collect::<Vec<_>>();
    if !events.is_empty() {
        event::propose_and_commit_batch(state, events);
    }
    Ok(false)
}

/// Runs a leaf inside the resumable interpreter. `Ok(true)` when it staged a
/// choice and the continuation must suspend.
pub(crate) fn drive(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    op: &P2cOpV1,
    path: Vec<u16>,
) -> Result<bool, String> {
    match op {
        P2cOpV1::ExhumeV1 => {
            let first = state.active_player;
            stage_exhume(
                state,
                continuation,
                &[first, first.opponent()],
                Vec::new(),
                path,
            )
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
    continuation: &mut EffectContinuation,
    choice: &E3ChoiceV1,
    kind: &P2cChoiceV1,
    bindings: Vec<EffectObjectBinding>,
) -> Result<bool, String> {
    match kind {
        P2cChoiceV1::ExhumeV1 { chosen, later } => {
            let mut chosen = chosen.clone();
            chosen.extend(bindings.first().copied());
            stage_exhume(state, continuation, later, chosen, choice.path.clone())
        }
    }
}

// ---- Hooks: triggers ----------------------------------------------------------------

/// The triggers this module's hooks queue in this collection: Phantom
/// Nishoba's life gain for the damage it dealt, and Phyrexian Devourer's
/// state trigger. They are collected with wave P1a's queue.
pub(crate) fn delayed_triggers(state: &mut GameState, events: &[CommittedEvent]) {
    if !cfg!(feature = "eternal-fixtures") {
        return;
    }
    note_nishoba_damage(state, events);
    note_devourer_power(state);
}

/// "Whenever this creature deals damage, you gain that much life": one
/// trigger per Nishoba for the damage it dealt in these events.
fn note_nishoba_damage(state: &mut GameState, events: &[CommittedEvent]) {
    let mut dealt: Vec<(ObjectId, i32)> = Vec::new();
    for event in events {
        let CommittedEvent::Damage { source, amount, .. } = *event else {
            continue;
        };
        if amount <= 0 || !statics::is_nishoba(state, source) {
            continue;
        }
        match dealt.iter_mut().find(|(seen, _)| *seen == source) {
            Some((_, total)) => *total = total.saturating_add(amount),
            None => dealt.push((source, amount)),
        }
    }
    for (nishoba, amount) in dealt {
        let controller = state.objects.get(nishoba).controller;
        let contract = AbilitySourceContractV4::capture(state, nishoba);
        queue_trigger(
            state,
            contract,
            controller,
            p2c(P2cOpV1::NishobaLifeV1 { amount }),
            None,
        );
    }
}

/// Phyrexian Devourer: "When this creature's power is 7 or greater,
/// sacrifice it." A state trigger (603.8) triggers once until it leaves the
/// stack.
fn note_devourer_power(state: &mut GameState) {
    let Some(devourer) = statics::devourer_id() else {
        return;
    };
    let candidates = [PlayerId::P0, PlayerId::P1]
        .into_iter()
        .flat_map(|player| state.players[player.index()].battlefield.iter().copied())
        .filter(|&object| {
            let live = state.objects.get(object);
            live.zone == Zone::Battlefield
                && live.card_def == devourer
                && crate::eternal_legacy_e9a_v1::active_named(state, object, PHYREXIAN_DEVOURER)
        })
        .collect::<Vec<_>>();
    if candidates.is_empty()
        && read_state(state).is_none_or(|p2c| p2c.devourer_triggered.is_empty())
    {
        return;
    }
    for object in candidates {
        let link = live_link(state, object);
        let big = crate::engine::effective_power(state, object) >= 7;
        let known = read_state(state).is_some_and(|p2c| p2c.devourer_triggered.contains(&link));
        let outstanding = known && devourer_trigger_outstanding(state, object);
        if known && !outstanding {
            // 603.8: the previous instance left the stack (resolved, or
            // countered), so the trigger may trigger again.
            p2c_state(state)
                .devourer_triggered
                .retain(|entry| *entry != link);
        }
        if big && !outstanding {
            p2c_state(state).devourer_triggered.push(link);
            let controller = state.objects.get(object).controller;
            let contract = AbilitySourceContractV4::capture(state, object);
            queue_trigger(state, contract, controller, devourer_sacrifice(), None);
        }
    }
    // Forget incarnations that left the battlefield.
    let live = read_state(state)
        .map(|p2c| p2c.devourer_triggered.clone())
        .unwrap_or_default()
        .into_iter()
        .filter(|link| crate::eternal_legacy_e9a_v1::link_live_in(state, *link, Zone::Battlefield))
        .collect::<Vec<_>>();
    if read_state(state).is_some_and(|p2c| p2c.devourer_triggered != live) {
        p2c_state(state).devourer_triggered = live;
    }
}

/// Whether an instance of Phyrexian Devourer's state trigger for `object` is
/// queued, waiting to be put on the stack, or on the stack.
fn devourer_trigger_outstanding(state: &GameState, object: ObjectId) -> bool {
    let effect = devourer_sacrifice();
    state.stack.iter().any(|item| {
        item.kind == crate::state::StackItemKind::TriggeredAbility
            && item.source == object
            && item.inline_effect.as_ref() == Some(&effect)
    }) || state
        .engine
        .pending_triggers
        .iter()
        .any(|pending| pending.source == object && pending.effect == effect)
        || state.eternal_v1.as_ref().is_some_and(|eternal| {
            eternal
                .p1a
                .queued_triggers
                .iter()
                .any(|queued| queued.source.source == object && queued.effect == effect)
        })
}

/// The beginning of a turn ends the last turn's records.
pub(crate) fn begin_turn(state: &mut GameState, _player: PlayerId) {
    let Some(p2c) = read_state(state) else {
        return;
    };
    if p2c.life_lands.is_empty() {
        return;
    }
    let now = TurnV1::now(state);
    let life_lands = p2c
        .life_lands
        .iter()
        .copied()
        .filter(|(link, turn, _)| {
            *turn == now
                && crate::eternal_legacy_e9a_v1::link_live_in(state, *link, Zone::Battlefield)
        })
        .collect::<Vec<_>>();
    if let Some(p2c) = state.eternal_v1.as_mut().map(|eternal| &mut eternal.p2c) {
        p2c.life_lands = life_lands;
    }
}
