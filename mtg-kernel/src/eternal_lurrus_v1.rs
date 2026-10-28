//! Card programs for the eternal catalog's Vintage Lurrus lists (wave E4b,
//! `docs/design/eternal_formats_support_v1.md`): companions, Lurrus of the
//! Dream-Den, and the cards that complete the Lurrus decks.
//!
//! `eternal_cards_v1` forwards its effect leaves, trigger conditions and
//! state here, and the engine calls the hooks below only in
//! `eternal-fixtures` builds. Every table is keyed by a printed name that
//! occurs only in the eternal registry, so the Pauper, FDN and Standard
//! catalogs never reach it. Behavior was read from each card's current
//! Oracle text and, where present, its XMage source.
//!
//! A companion (702.139) waits face up in exile, the public zone the policy
//! observation shows, until its owner pays {3} to put it into their hand.
//! No card in the catalog interacts with it there: Karn, the Great Creator's
//! -2 finds only artifacts, and linked exiles name their own cards.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

use crate::card_def::{CardDef, CardType, CostComponent, Keywords, Subtype, TargetSpec, CARD_DEFS};
use crate::effect::{
    EffectCond, EffectOp, ExecCtx, LibraryLookCount, LibraryPickFilterV1, LibraryPickRule,
    LibraryPickSelectionV1, ObjectRef, PlayerRef,
};
use crate::event::{self, CommittedEvent, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::mana::ManaColor;
use crate::standard_cards_v1::{StandardOpV1, StandardTargetV1};
use crate::state::{
    AbilitySourceContractV4, GameObject, GameState, ObjectLinkV4, ObjectStateV4, Step, Target, Zone,
};
use crate::trigger::{PendingTrigger, TriggerCondition, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};

/// Per-game state for this module's cards, kept inside
/// `eternal_cards_v1::EternalStateV1`. Default (and absent from snapshots)
/// until one of these cards needs it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LurrusStateV1 {
    /// Each player's companion, by the exile incarnation it began the game
    /// as.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub companions: Vec<CompanionV1>,
    /// Lurrus of the Dream-Den's graveyard casts: the Lurrus whose
    /// permission each used, the turn, and the cast card's graveyard
    /// incarnation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lurrus_casts: Vec<LurrusCastV1>,
    /// Graveyard cards Snapcaster Mage gave flashback until end of turn.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub flashback_grants: Vec<TurnGrantV1>,
    /// Graveyard incarnations announced as a spell through the escape
    /// Underworld Breach grants, so the spell keeps its escape on the stack
    /// even if the Breach leaves.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub breach_escapes: Vec<ObjectLinkV4>,
    /// Mana Drain: "At the beginning of your next main phase, add an amount
    /// of {C} equal to that spell's mana value."
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mana_drains: Vec<ManaDrainV1>,
    /// Forth Eorlingas!: "Whenever one or more creatures you control deal
    /// combat damage to one or more players this turn, you become the
    /// monarch."
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub monarch_watches: Vec<MonarchWatchV1>,
    /// Printed and delayed triggers detected by engine hooks, waiting for
    /// the next trigger collection.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub queued_triggers: Vec<QueuedTriggerV1>,
    /// Pithing Needles and the definition whose name each chose.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub needle_names: Vec<(ObjectLinkV4, u16)>,
    /// How many colors of mana were spent to cast each Prismatic Ending
    /// still on the stack (converge).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub colors_spent: Vec<(ObjectLinkV4, u8)>,
}

impl LurrusStateV1 {
    pub(crate) fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// A player's companion card.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompanionV1 {
    pub player: PlayerId,
    pub card: ObjectLinkV4,
}

/// One use of a Lurrus of the Dream-Den's graveyard permission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LurrusCastV1 {
    pub lurrus: ObjectLinkV4,
    pub turn: u32,
    pub card: ObjectLinkV4,
}

/// A permission on one card incarnation for the turn with key `turn`
/// and active player `active`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TurnGrantV1 {
    pub card: ObjectLinkV4,
    pub turn: u32,
    pub active: PlayerId,
}

/// A Mana Drain waiting for its controller's next main phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManaDrainV1 {
    pub controller: PlayerId,
    pub source: AbilitySourceContractV4,
    pub amount: u16,
}

/// A Forth Eorlingas! monarch trigger for one turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MonarchWatchV1 {
    pub controller: PlayerId,
    pub turn: u32,
    pub active: PlayerId,
    pub source: AbilitySourceContractV4,
}

/// A triggered ability queued by an engine hook.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueuedTriggerV1 {
    pub source: AbilitySourceContractV4,
    pub controller: PlayerId,
    pub op: LurrusOpV1,
}

/// Trigger conditions owned by this module, carried as
/// `eternal_cards_v1::EternalTriggerV1::LurrusV1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LurrusTriggerV1 {
    /// "At the beginning of the end step" (Underworld Breach): every end
    /// step, whoever's turn it is.
    BeginningOfEachEndStepV1,
}

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::LurrusV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LurrusOpV1 {
    /// Gitaxian Probe: the controller looks at target 0's hand.
    LookAtTargetHandV1,
    /// Swords to Plowshares: exile target 0; its controller gains life
    /// equal to its power.
    ExileTargetCreatureGainPowerV1,
    /// Flame of Anor's chosen modes, each with the target slot it uses.
    FlameOfAnorV1 {
        draw: Option<u8>,
        artifact: Option<u8>,
        creature: Option<u8>,
        spec: TargetSpec,
    },
    /// Forth Eorlingas!: create X `token_def` tokens and watch this turn's
    /// combat damage for the monarch trigger.
    ForthEorlingasV1 { token_def: u16 },
    /// Mana Drain: counter target 0 and schedule the mana.
    ManaDrainV1,
    /// Mana Drain's delayed trigger: the controller adds `amount` {C}.
    AddColorlessV1 { amount: u16 },
    /// Prismatic Ending: exile target 0 if its mana value is at most the
    /// number of colors spent to cast this spell.
    ConvergeExileTargetV1,
    /// Snapcaster Mage: target 0 gains flashback until end of turn.
    GrantFlashbackV1,
    /// "This creature gains flying until end of turn" for this ability's
    /// source incarnation (Psychic Frog).
    SourceGainsFlyingV1,
    /// Psychic Frog's combat damage trigger: draw a card.
    FrogDrawV1,
    /// Forth Eorlingas!'s trigger: the controller becomes the monarch.
    BecomeMonarchV1,
}

fn lurrus(op: LurrusOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(
        crate::eternal_cards_v1::EternalOpV1::LurrusV1(op),
    ))
}

pub(crate) const LURRUS: &str = "Lurrus of the Dream-Den";
const SNAPCASTER_MAGE: &str = "Snapcaster Mage";
const THUNDERTRAP_TRAINER: &str = "Thundertrap Trainer";
const THUNDERTRAP_TOKEN: &str = "Thundertrap Trainer Offspring Token";
const PORTABLE_HOLE: &str = "Portable Hole";
const SOUL_GUIDE_LANTERN: &str = "Soul-Guide Lantern";
const UNDERWORLD_BREACH: &str = "Underworld Breach";
const PSYCHIC_FROG: &str = "Psychic Frog";
const HEXING_SQUELCHER: &str = "Hexing Squelcher";
const NULL_ROD: &str = "Null Rod";
const PITHING_NEEDLE: &str = "Pithing Needle";
const MOX_OPAL: &str = "Mox Opal";
const FLAME_OF_ANOR: &str = "Flame of Anor";
const MANA_DRAIN: &str = "Mana Drain";
const FORTH_EORLINGAS: &str = "Forth Eorlingas!";
const PRISMATIC_ENDING: &str = "Prismatic Ending";

fn lurrus_state(state: &mut GameState) -> &mut LurrusStateV1 {
    &mut state.eternal_v1.get_or_insert_with(Default::default).lurrus
}

fn read_state(state: &GameState) -> Option<&LurrusStateV1> {
    state.eternal_v1.as_ref().map(|eternal| &eternal.lurrus)
}

fn halt(state: &mut GameState, source: ObjectId) {
    state.engine.halted = Some((
        crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
        source,
    ));
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

fn definition_name(state: &GameState, object: ObjectId) -> Option<&'static str> {
    let live = state.objects.try_get(object)?;
    CARD_DEFS.get(live.card_def as usize).map(|def| def.name)
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

/// Whether `player` holds priority at sorcery speed (307.1).
fn sorcery_timing(state: &GameState, player: PlayerId) -> bool {
    state.priority_player == player
        && state.active_player == player
        && state.stack.is_empty()
        && matches!(state.step, Step::Main1 | Step::Main2)
        && state.engine.pending_cast.is_none()
        && state.engine.pending_activation.is_none()
        && state.engine.pending_effect.is_none()
}

// ---- Companions ----------------------------------------------------------------

/// Whether `deck` meets companion `companion`'s deck condition, or `None`
/// when `companion` is not a supported companion. Lurrus of the Dream-Den:
/// "Each permanent card in your starting deck has mana value 2 or less."
pub fn companion_condition_met(companion: u16, deck: &[u16]) -> Option<bool> {
    let def = CARD_DEFS.get(usize::from(companion))?;
    if !def.has_full_support() || def.is_token {
        return None;
    }
    match def.name {
        LURRUS => Some(deck.iter().all(|&id| {
            CARD_DEFS
                .get(usize::from(id))
                .is_some_and(|card| !is_permanent_card(card) || card.mana_value <= 2)
        })),
        // Yorion, Sky Nomad (wave E8a): "Your starting deck contains at
        // least twenty cards more than the minimum deck size", sixty for
        // these constructed formats.
        "Yorion, Sky Nomad" => Some(deck.len() >= 80),
        _ => None,
    }
}

/// Puts each player's companion, if any, face up in exile before the game
/// begins.
pub fn add_companions(state: &mut GameState, companions: [Option<u16>; 2]) {
    for player in [PlayerId::P0, PlayerId::P1] {
        let Some(card_def) = companions[player.index()] else {
            continue;
        };
        let id = state.objects.push(GameObject {
            card_def,
            name: CARD_DEFS[usize::from(card_def)].object_name.into(),
            owner: player,
            controller: player,
            zone: Zone::Exile,
            tapped: false,
            summoning_sick: false,
            damage: 0,
            counters: Default::default(),
            attachments: Vec::new(),
            v4: ObjectStateV4::from_card_def(card_def),
            spell_copy_origin: None,
            plotted_turn: None,
            zone_change_count: 0,
        });
        state.exile.push(id);
        lurrus_state(state).companions.push(CompanionV1 {
            player,
            card: ObjectLinkV4 {
                object: id,
                zone_change_count: 0,
            },
        });
    }
}

/// Whether `source` is a companion still waiting in exile.
pub(crate) fn is_companion_source(state: &GameState, source: ObjectId) -> bool {
    read_state(state).is_some_and(|lurrus| {
        lurrus.companions.iter().any(|companion| {
            companion.card.object == source && link_live_in(state, companion.card, Zone::Exile)
        })
    })
}

/// The companion `player` may put into their hand right now: "you may put
/// it into your hand from outside the game for {3} as a sorcery" (702.139c),
/// a special action offered as the card's ability 0.
pub(crate) fn companion_action(state: &GameState, player: PlayerId) -> Option<ObjectId> {
    let companion = read_state(state)?
        .companions
        .iter()
        .find(|companion| companion.player == player)?;
    (link_live_in(state, companion.card, Zone::Exile)
        && state.exile.contains(&companion.card.object)
        && state.objects.get(companion.card.object).owner == player
        && sorcery_timing(state, player)
        && crate::engine::can_pay_effect_mana(player, &[], 3, state))
    .then_some(companion.card.object)
}

/// Pays {3} and puts `player`'s companion into their hand. The card stays
/// known to the opponent, who saw it in exile. No stack, no priority pass.
pub(crate) fn put_companion_into_hand(
    state: &mut GameState,
    player: PlayerId,
    source: ObjectId,
) -> Result<(), String> {
    if companion_action(state, player) != Some(source) {
        return Err("the companion special action is not available".to_string());
    }
    if !crate::engine::pay_effect_mana(player, &[], 3, state) {
        return Err("the companion's {3} is not payable".to_string());
    }
    event::propose_and_commit(state, ProposedEvent::zone_change(source, Zone::Hand));
    if state.objects.get(source).zone == Zone::Hand {
        state.reveal_hand_card(player.opponent(), player, source)?;
    }
    Ok(())
}

// ---- Lurrus of the Dream-Den -------------------------------------------------------

/// The Lurrus of the Dream-Den `holder` controls whose permission is unused
/// this turn.
fn unused_lurrus(state: &GameState, holder: PlayerId) -> Option<ObjectLinkV4> {
    let turn = state.turn_key_v1();
    active_permanents(state, LURRUS, Some(holder))
        .map(|object| live_link(state, object))
        .find(|link| {
            read_state(state).is_none_or(|lurrus| {
                !lurrus
                    .lurrus_casts
                    .iter()
                    .any(|cast| cast.lurrus == *link && cast.turn == turn)
            })
        })
}

/// Lurrus of the Dream-Den: "Once during each of your turns, you may cast a
/// permanent spell with mana value 2 or less from your graveyard." Whether
/// `holder` may cast the graveyard incarnation (`object`, `zone_change_count`)
/// this way. A card already cast this way this turn keeps its permission
/// while it is on the stack.
pub(crate) fn lurrus_permission(
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
    if live.owner != holder
        || live.v4.is_token
        || live.spell_copy_origin.is_some()
        || def.is_token
        || def.is_land
        || !is_permanent_card(def)
        || def.mana_value > 2
    {
        return false;
    }
    let card = ObjectLinkV4 {
        object,
        zone_change_count,
    };
    let turn = state.turn_key_v1();
    if read_state(state).is_some_and(|lurrus| {
        lurrus
            .lurrus_casts
            .iter()
            .any(|cast| cast.card == card && cast.turn == turn)
    }) {
        return true;
    }
    let in_graveyard_or_announcing = (live.zone == Zone::Graveyard
        && live.zone_change_count == zone_change_count)
        || state.engine.pending_cast.as_ref().is_some_and(|pending| {
            pending.spell == object && pending.origin_zone == Zone::Graveyard
        });
    in_graveyard_or_announcing
        && state.active_player == holder
        && unused_lurrus(state, holder).is_some()
}

/// Records the Lurrus permission a graveyard cast just used, unless another
/// permission (Case of the Uneaten Feast) covered it.
pub(crate) fn note_graveyard_permission_cast(
    state: &mut GameState,
    spell: ObjectId,
    origin_zone_change_count: u32,
    controller: PlayerId,
) {
    if crate::standard_cards_v1::standard_graveyard_cast_granted(
        state,
        spell,
        controller,
        origin_zone_change_count,
    ) || crate::eternal_e5a_v1::permissions::will_permission(state, spell, controller)
    {
        return;
    }
    let card = ObjectLinkV4 {
        object: spell,
        zone_change_count: origin_zone_change_count,
    };
    let turn = state.turn_key_v1();
    if read_state(state).is_some_and(|lurrus| {
        lurrus
            .lurrus_casts
            .iter()
            .any(|cast| cast.card == card && cast.turn == turn)
    }) {
        return;
    }
    let Some(lurrus) = unused_lurrus(state, controller) else {
        return;
    };
    let casts = &mut lurrus_state(state).lurrus_casts;
    casts.retain(|cast| cast.turn == turn);
    casts.push(LurrusCastV1 { lurrus, turn, card });
}

// ---- Granted flashback and escape -------------------------------------------------

/// Index-aligned with `CARD_DEFS`: the flashback cost "equal to its mana
/// cost" and Underworld Breach's escape cost "equal to the card's mana cost
/// plus exile three other cards from your graveyard".
fn granted_costs() -> &'static [([CostComponent; 1], [CostComponent; 2])] {
    static TABLE: std::sync::OnceLock<Vec<([CostComponent; 1], [CostComponent; 2])>> =
        std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        CARD_DEFS
            .iter()
            .map(|def| {
                (
                    [CostComponent::Mana(def.cost)],
                    [
                        CostComponent::Mana(def.cost),
                        CostComponent::ExileOtherCardsFromOwnGraveyard(3),
                    ],
                )
            })
            .collect()
    })
}

fn definition_index(definition: &CardDef) -> Option<usize> {
    CARD_DEFS
        .iter()
        .position(|candidate| std::ptr::eq(candidate, definition))
}

/// The flashback cost Snapcaster Mage gives an instant or sorcery card.
pub(crate) fn granted_flashback_cost(definition: &CardDef) -> Option<&'static [CostComponent]> {
    if !cfg!(feature = "eternal-fixtures")
        || definition.flashback.is_some()
        || !(definition.has_type(CardType::Instant) || definition.has_type(CardType::Sorcery))
    {
        return None;
    }
    let index = definition_index(definition)?;
    Some(granted_costs()[index].0.as_slice())
}

/// The escape cost Underworld Breach gives a nonland card.
pub(crate) fn granted_escape_cost(definition: &CardDef) -> Option<&'static [CostComponent]> {
    if !cfg!(feature = "eternal-fixtures")
        || definition.escape.is_some()
        || definition.is_land
        || definition.is_token
    {
        return None;
    }
    let index = definition_index(definition)?;
    Some(granted_costs()[index].1.as_slice())
}

fn turn_grant_active(state: &GameState, grant: &TurnGrantV1) -> bool {
    grant.turn == state.turn_key_v1() && grant.active == state.active_player
}

/// Whether the graveyard incarnation (`object`, `zone_change_count`) has
/// flashback granted by Snapcaster Mage this turn.
pub(crate) fn flashback_granted(
    state: &GameState,
    object: ObjectId,
    zone_change_count: u32,
) -> bool {
    let card = ObjectLinkV4 {
        object,
        zone_change_count,
    };
    read_state(state).is_some_and(|lurrus| {
        lurrus
            .flashback_grants
            .iter()
            .any(|grant| grant.card == card && turn_grant_active(state, grant))
    })
}

/// Whether the graveyard incarnation (`object`, `zone_change_count`) has
/// the escape Underworld Breach grants: "Each nonland card in your
/// graveyard has escape." A spell announced through it keeps it.
pub(crate) fn escape_granted(state: &GameState, object: ObjectId, zone_change_count: u32) -> bool {
    let card = ObjectLinkV4 {
        object,
        zone_change_count,
    };
    if read_state(state).is_some_and(|lurrus| lurrus.breach_escapes.contains(&card)) {
        return true;
    }
    let Some(live) = state.objects.try_get(object) else {
        return false;
    };
    live.zone == Zone::Graveyard
        && live.zone_change_count == zone_change_count
        && !live.v4.is_token
        && live.spell_copy_origin.is_none()
        && CARD_DEFS
            .get(live.card_def as usize)
            .is_some_and(|def| granted_escape_cost(def).is_some())
        && active_permanents(state, UNDERWORLD_BREACH, Some(live.owner))
            .next()
            .is_some()
}

/// Records a spell announced through Underworld Breach's escape.
pub(crate) fn note_breach_escape(state: &mut GameState, object: ObjectId, zone_change_count: u32) {
    let card = ObjectLinkV4 {
        object,
        zone_change_count,
    };
    let escapes = &mut lurrus_state(state).breach_escapes;
    if !escapes.contains(&card) {
        escapes.push(card);
    }
}

// ---- Spells --------------------------------------------------------------------------

/// Demonic Tutor: "Search your library for a card, put that card into your
/// hand, then shuffle." Hidden from the opponent.
pub fn demonic_tutor() -> EffectOp {
    EffectOp::SearchLibraryToHand {
        player: PlayerRef::Controller,
        filter: crate::effect::LibraryCardFilter::AnyCard,
    }
}

/// Merchant Scroll: "Search your library for a blue instant card, reveal
/// that card, put it into your hand, then shuffle."
pub fn merchant_scroll() -> EffectOp {
    EffectOp::SearchLibraryToHand {
        player: PlayerRef::Controller,
        filter: crate::effect::LibraryCardFilter::BlueInstantV1,
    }
}

fn look_top_pick(count: u8, pick: u8) -> EffectOp {
    EffectOp::LookTopPickToHandBottomRest {
        player: PlayerRef::Controller,
        count: LibraryLookCount::Fixed(count),
        pick,
        choose_rest_order: true,
    }
}

/// Flow State: "Look at the top three cards of your library. Put one of
/// them into your hand and the rest on the bottom of your library in any
/// order. If there is an instant card and a sorcery card in your graveyard,
/// instead put two of them into your hand and the rest on the bottom of
/// your library in any order."
pub fn flow_state() -> EffectOp {
    EffectOp::Conditional {
        cond: EffectCond::And(
            Box::new(EffectCond::ControllerGraveyardHasType(CardType::Instant)),
            Box::new(EffectCond::ControllerGraveyardHasType(CardType::Sorcery)),
        ),
        then: Box::new(look_top_pick(3, 2)),
        else_: Box::new(look_top_pick(3, 1)),
    }
}

/// Dig Through Time: "Look at the top seven cards of your library. Put two
/// of them into your hand and the rest on the bottom of your library in any
/// order." Delve is the card's own `CardDef::delve`.
pub fn dig_through_time() -> EffectOp {
    look_top_pick(7, 2)
}

/// Force of Negation: "Counter target noncreature spell. If that spell is
/// countered this way, exile it instead of putting it into its owner's
/// graveyard." Its alternative cost is the card's own `CardDef::alt_cost`.
pub fn force_of_negation() -> EffectOp {
    EffectOp::Conditional {
        cond: EffectCond::TargetSpellCanBeCountered(0),
        then: Box::new(EffectOp::MoveObject {
            object: ObjectRef::Target(0),
            to_zone: Zone::Exile,
        }),
        else_: Box::new(EffectOp::Sequence(Vec::new())),
    }
}

/// Gitaxian Probe: "Look at target player's hand. Draw a card."
pub fn gitaxian_probe() -> EffectOp {
    EffectOp::Sequence(vec![
        lurrus(LurrusOpV1::LookAtTargetHandV1),
        EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 1,
        },
    ])
}

/// Swords to Plowshares: "Exile target creature. Its controller gains life
/// equal to its power."
pub fn swords_to_plowshares() -> EffectOp {
    lurrus(LurrusOpV1::ExileTargetCreatureGainPowerV1)
}

/// Prismatic Ending: "Converge -- Exile target nonland permanent if its
/// mana value is less than or equal to the number of colors of mana spent
/// to cast this spell."
pub fn prismatic_ending() -> EffectOp {
    lurrus(LurrusOpV1::ConvergeExileTargetV1)
}

/// Brain Freeze: "Target player mills three cards." Storm is its cast
/// trigger (`eternal_cards_v1::e3`).
pub fn brain_freeze() -> EffectOp {
    EffectOp::MillCards {
        player: PlayerRef::Target(0),
        count: 3,
    }
}

/// Mana Drain: "Counter target spell. At the beginning of your next main
/// phase, add an amount of {C} equal to that spell's mana value." The mana
/// comes even if the spell can't be countered (XMage).
pub fn mana_drain() -> EffectOp {
    lurrus(LurrusOpV1::ManaDrainV1)
}

/// Forth Eorlingas!: "Create X 2/2 red Human Knight creature tokens with
/// trample and haste. Whenever one or more creatures you control deal
/// combat damage to one or more players this turn, you become the monarch."
pub fn forth_eorlingas() -> EffectOp {
    lurrus(LurrusOpV1::ForthEorlingasV1 {
        token_def: crate::card_def::card_id_by_name("Human Knight Token")
            .expect("Human Knight Token in CARD_DEFS"),
    })
}

/// Flame of Anor's modes: "Target player draws two cards.", "Destroy
/// target artifact." and "Flame of Anor deals 5 damage to target
/// creature." Modes 3 to 5 are the pairs a Wizard's controller may choose
/// (`spell_mode_allowed`).
fn flame(
    draw: Option<u8>,
    artifact: Option<u8>,
    creature: Option<u8>,
    spec: TargetSpec,
) -> EffectOp {
    lurrus(LurrusOpV1::FlameOfAnorV1 {
        draw,
        artifact,
        creature,
        spec,
    })
}

pub fn flame_of_anor_draw() -> EffectOp {
    flame(Some(0), None, None, TargetSpec::AnyPlayer)
}

pub fn flame_of_anor_artifact() -> EffectOp {
    flame(None, Some(0), None, TargetSpec::ArtifactPermanent)
}

pub fn flame_of_anor_creature() -> EffectOp {
    flame(None, None, Some(0), TargetSpec::Creature)
}

pub fn flame_of_anor_draw_artifact() -> EffectOp {
    flame(
        Some(0),
        Some(1),
        None,
        TargetSpec::StandardV1(StandardTargetV1::PlayerThenArtifactV1),
    )
}

pub fn flame_of_anor_draw_creature() -> EffectOp {
    flame(
        Some(0),
        None,
        Some(1),
        TargetSpec::StandardV1(StandardTargetV1::PlayerThenCreatureV1),
    )
}

pub fn flame_of_anor_artifact_creature() -> EffectOp {
    flame(
        None,
        Some(0),
        Some(1),
        TargetSpec::StandardV1(StandardTargetV1::ArtifactThenCreatureV1),
    )
}

/// Flame of Anor: "Choose one. If you control a Wizard as you cast this
/// spell, you may choose two instead." Whether `controller` may choose
/// printed form `mode` of the card named `name`.
pub(crate) fn spell_mode_allowed(
    state: &GameState,
    name: &str,
    mode: u8,
    controller: PlayerId,
) -> bool {
    name != FLAME_OF_ANOR
        || mode < 3
        || state.players[controller.index()]
            .battlefield
            .iter()
            .any(|&object| {
                state.objects.get(object).controller == controller
                    && crate::engine::has_effective_subtype(state, object, Subtype::Wizard)
            })
}

// ---- Activated ability programs ------------------------------------------------------

/// Soul-Guide Lantern: "Exile each opponent's graveyard."
pub fn exile_opponents_graveyard() -> EffectOp {
    EffectOp::ExilePlayersGraveyard {
        player: PlayerRef::Opponent,
    }
}

/// Psychic Frog: "This creature gains flying until end of turn."
pub fn psychic_frog_flying() -> EffectOp {
    lurrus(LurrusOpV1::SourceGainsFlyingV1)
}

/// The companion special action's printed slot. The engine performs it as a
/// special action (`put_companion_into_hand`) and never puts this program
/// on the stack.
pub fn companion_to_hand() -> EffectOp {
    EffectOp::Sequence(Vec::new())
}

// ---- Triggered abilities ------------------------------------------------------------

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

const fn lurrus_trigger(
    condition: LurrusTriggerV1,
    effect: fn() -> EffectOp,
) -> TriggeredAbilityDef {
    trigger(
        TriggerCondition::StandardV1(crate::standard_cards_v1::StandardTriggerV1::EternalV1(
            crate::eternal_cards_v1::EternalTriggerV1::LurrusV1(condition),
        )),
        effect,
    )
}

/// Offspring {4}: "When this creature enters, if its offspring cost was
/// paid, create a 1/1 token copy of it."
fn thundertrap_offspring() -> EffectOp {
    EffectOp::Conditional {
        cond: EffectCond::WasKicked,
        then: Box::new(EffectOp::CreateToken {
            token_def: crate::card_def::card_id_by_name(THUNDERTRAP_TOKEN)
                .expect("Thundertrap Trainer Offspring Token in CARD_DEFS"),
            controller: PlayerRef::Controller,
        }),
        else_: Box::new(EffectOp::Sequence(Vec::new())),
    }
}

/// Thundertrap Trainer: "When this creature enters, look at the top four
/// cards of your library. You may reveal a noncreature, nonland card from
/// among them and put it into your hand. Put the rest on the bottom of your
/// library in a random order."
fn thundertrap_look() -> EffectOp {
    EffectOp::LookTopSelectV1 {
        player: PlayerRef::Controller,
        count: LibraryLookCount::Fixed(4),
        rule: LibraryPickRule {
            pick: 1,
            choose_rest_order: false,
            selection: Some(LibraryPickSelectionV1 {
                without_mana_cost: crate::engine::FreeCastV1(false),
                filter: LibraryPickFilterV1::NoncreatureNonlandV1,
                optional: true,
                destination: Zone::Hand,
                reveal_selected: true,
            }),
        },
        pick_x: false,
    }
}

/// Snapcaster Mage: "When this creature enters, target instant or sorcery
/// card in your graveyard gains flashback until end of turn. The flashback
/// cost is equal to its mana cost."
fn snapcaster_grant() -> EffectOp {
    lurrus(LurrusOpV1::GrantFlashbackV1)
}

/// Portable Hole: "When this artifact enters, exile target nonland
/// permanent an opponent controls with mana value 2 or less until this
/// artifact leaves the battlefield."
fn portable_hole_exile() -> EffectOp {
    EffectOp::ExileTargetLinkedToSource {
        object: ObjectRef::Target(0),
    }
}

/// Soul-Guide Lantern: "When this artifact enters, exile target card from a
/// graveyard."
fn lantern_exile() -> EffectOp {
    EffectOp::MoveAllTargets {
        to_zone: Zone::Exile,
    }
}

/// Underworld Breach: "At the beginning of the end step, sacrifice this
/// enchantment."
fn breach_sacrifice() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(
        crate::eternal_cards_v1::EternalOpV1::SacrificeSourceV1,
    ))
}

const THUNDERTRAP_TRIGGERS: [TriggeredAbilityDef; 2] = [
    TriggeredAbilityDef {
        intervening_if_kicked: true,
        ..trigger(TriggerCondition::Etb, thundertrap_offspring)
    },
    trigger(TriggerCondition::Etb, thundertrap_look),
];
const SNAPCASTER_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, snapcaster_grant)];
const PORTABLE_HOLE_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, portable_hole_exile)];
const LANTERN_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(TriggerCondition::Etb, lantern_exile)];
const BREACH_TRIGGERS: [TriggeredAbilityDef; 1] = [lurrus_trigger(
    LurrusTriggerV1::BeginningOfEachEndStepV1,
    breach_sacrifice,
)];

/// Triggered abilities of this module's cards.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        THUNDERTRAP_TRAINER | THUNDERTRAP_TOKEN => &THUNDERTRAP_TRIGGERS,
        SNAPCASTER_MAGE => &SNAPCASTER_TRIGGERS,
        PORTABLE_HOLE => &PORTABLE_HOLE_TRIGGERS,
        SOUL_GUIDE_LANTERN => &LANTERN_TRIGGERS,
        UNDERWORLD_BREACH => &BREACH_TRIGGERS,
        // Waves E5a and E5b own disjoint card names.
        _ => match crate::eternal_e5a_v1::triggers_for(name) {
            [] => crate::eternal_legacy_e5b_v1::triggers_for(name),
            e5a => e5a,
        },
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
    // `triggers_for` falls through to waves E5a and E5b, so they answer first.
    if let Some(spec) = crate::eternal_e5a_v1::trigger_target_spec(name, effect)
        .or_else(|| crate::eternal_legacy_e5b_v1::trigger_target_spec(name, effect))
    {
        return Some(spec);
    }
    if !own_trigger_effect(name, effect) {
        return None;
    }
    Some(
        if name == SNAPCASTER_MAGE && *effect == snapcaster_grant() {
            TargetSpec::StandardV1(StandardTargetV1::InstantOrSorceryCardInOwnGraveyard)
        } else if name == PORTABLE_HOLE && *effect == portable_hole_exile() {
            TargetSpec::StandardV1(StandardTargetV1::OpponentNonlandPermanentManaValueAtMost(2))
        } else if name == SOUL_GUIDE_LANTERN && *effect == lantern_exile() {
            TargetSpec::StandardV1(StandardTargetV1::CardInAGraveyard)
        } else {
            TargetSpec::None
        },
    )
}

/// Queued and delayed triggers owned by this module's cards.
pub(crate) fn delayed_effect_matches(name: &str, effect: &EffectOp) -> bool {
    match name {
        PSYCHIC_FROG => *effect == lurrus(LurrusOpV1::FrogDrawV1),
        MANA_DRAIN => matches!(
            effect,
            EffectOp::StandardV1(StandardOpV1::EternalV1(
                crate::eternal_cards_v1::EternalOpV1::LurrusV1(LurrusOpV1::AddColorlessV1 { .. })
            ))
        ),
        FORTH_EORLINGAS => *effect == lurrus(LurrusOpV1::BecomeMonarchV1),
        _ => {
            crate::eternal_e5a_v1::delayed_effect_matches(name, effect)
                || crate::eternal_legacy_e5b_v1::delayed_effect_matches(name, effect)
        }
    }
}

pub(crate) fn trigger_matches(
    condition: LurrusTriggerV1,
    events: &[CommittedEvent],
    index: usize,
    _source: ObjectId,
    _state: &GameState,
) -> bool {
    match condition {
        LurrusTriggerV1::BeginningOfEachEndStepV1 => {
            matches!(events[index], CommittedEvent::BeginningEndStep { .. })
        }
    }
}

/// The source contract of a delayed trigger created by a resolving spell
/// (Mana Drain, Forth Eorlingas!). An ability's source contract can't name
/// the stack, so the trigger is bound to the card's current incarnation
/// when it triggers, after the spell has left the stack.
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

fn queue_trigger(
    state: &mut GameState,
    source: AbilitySourceContractV4,
    controller: PlayerId,
    op: LurrusOpV1,
) {
    lurrus_state(state).queued_triggers.push(QueuedTriggerV1 {
        source,
        controller,
        op,
    });
}

/// The triggers this collection puts on the stack that no printed trigger
/// condition matches.
pub(crate) fn delayed_triggers(
    state: &mut GameState,
    _events: &[CommittedEvent],
) -> Vec<PendingTrigger> {
    let Some(eternal) = state.eternal_v1.as_mut() else {
        return Vec::new();
    };
    if eternal.lurrus.queued_triggers.is_empty() {
        return Vec::new();
    }
    std::mem::take(&mut eternal.lurrus.queued_triggers)
        .into_iter()
        .map(|queued| {
            crate::eternal_cards_v1::delayed_trigger(
                queued.source,
                queued.controller,
                lurrus(queued.op),
            )
        })
        .collect()
}

/// A combat damage batch was dealt (`engine::deal_combat_damage`): Psychic
/// Frog's "Whenever this creature deals combat damage to a player or
/// planeswalker, draw a card." and Forth Eorlingas!'s monarch trigger.
pub(crate) fn note_combat_damage(state: &mut GameState, events: &[CommittedEvent]) {
    let mut frogs: Vec<ObjectId> = Vec::new();
    let mut damaged_players_by: Vec<PlayerId> = Vec::new();
    for event in events {
        match *event {
            CommittedEvent::Damage {
                source,
                target,
                amount,
            } if amount > 0 => {
                let to_player_or_planeswalker = match target {
                    Target::Player(_) => true,
                    Target::Object(object) => {
                        crate::engine::object_has_type(state, object, CardType::Planeswalker)
                    }
                    Target::StackItem(_) => false,
                };
                if to_player_or_planeswalker
                    && definition_name(state, source) == Some(PSYCHIC_FROG)
                    && !frogs.contains(&source)
                {
                    frogs.push(source);
                }
            }
            CommittedEvent::CombatDamageToPlayer { source, amount, .. } if amount > 0 => {
                if let Some(live) = state.objects.try_get(source) {
                    if !damaged_players_by.contains(&live.controller) {
                        damaged_players_by.push(live.controller);
                    }
                }
            }
            _ => {}
        }
    }
    for frog in frogs {
        let live = state.objects.get(frog);
        let controller = live.controller;
        let printed = live.v4.face_index == 0
            && (live.zone != Zone::Battlefield
                || crate::continuous_characteristics_v1::printed_abilities_active(state, frog));
        if !printed {
            continue;
        }
        let source = AbilitySourceContractV4::capture(state, frog);
        queue_trigger(state, source, controller, LurrusOpV1::FrogDrawV1);
    }
    let turn = state.turn_key_v1();
    let active = state.active_player;
    let watches = read_state(state)
        .map(|lurrus| {
            lurrus
                .monarch_watches
                .iter()
                .copied()
                .filter(|watch| {
                    watch.turn == turn
                        && watch.active == active
                        && damaged_players_by.contains(&watch.controller)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    for watch in watches {
        if let Some(source) = spell_delayed_source(state, watch.source, watch.controller) {
            queue_trigger(state, source, watch.controller, LurrusOpV1::BecomeMonarchV1);
        }
    }
}

/// A main phase began for the active player: Mana Drain's delayed triggers
/// for that player, and Static Prison's first-main-phase trigger.
pub(crate) fn begin_main_phase(state: &mut GameState) -> Vec<PendingTrigger> {
    let mut triggers = crate::eternal_legacy_e5b_v1::begin_main_phase(state);
    triggers.extend(mana_drains_due(state));
    triggers
}

fn mana_drains_due(state: &mut GameState) -> Vec<PendingTrigger> {
    let player = state.active_player;
    let Some(eternal) = state.eternal_v1.as_mut() else {
        return Vec::new();
    };
    if eternal.lurrus.mana_drains.is_empty() {
        return Vec::new();
    }
    let (due, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut eternal.lurrus.mana_drains)
        .into_iter()
        .partition(|drain| drain.controller == player);
    eternal.lurrus.mana_drains = rest;
    for drain in due {
        if let Some(source) = spell_delayed_source(state, drain.source, drain.controller) {
            queue_trigger(
                state,
                source,
                drain.controller,
                LurrusOpV1::AddColorlessV1 {
                    amount: drain.amount,
                },
            );
        }
    }
    // A main phase begins without trigger events, so the event-driven
    // collection would not pick these up: they go straight to the pending
    // triggers.
    delayed_triggers(state, &[])
}

/// The beginning of `active`'s turn ends this module's turn-bound records.
pub(crate) fn begin_turn(state: &mut GameState, active: PlayerId) {
    let turn = state.turn_key_v1();
    if let Some(eternal) = state.eternal_v1.as_mut() {
        let lurrus = &mut eternal.lurrus;
        lurrus
            .flashback_grants
            .retain(|grant| grant.turn == turn && grant.active == active);
        lurrus
            .monarch_watches
            .retain(|watch| watch.turn == turn && watch.active == active);
        lurrus.lurrus_casts.retain(|cast| cast.turn == turn);
    }
}

// ---- Statics -----------------------------------------------------------------------

/// Hexing Squelcher: "Spells you control can't be countered."
pub(crate) fn spell_cannot_be_countered(state: &GameState, object: ObjectId) -> bool {
    let Some(live) = state.objects.try_get(object) else {
        return false;
    };
    live.zone == Zone::Stack
        && active_permanents(state, HEXING_SQUELCHER, Some(live.controller))
            .next()
            .is_some()
}

/// Hexing Squelcher: 'Other creatures you control have "Ward--Pay 2
/// life."' One ward ability per Squelcher granting it to `object`.
pub(crate) fn granted_life_wards(state: &GameState, object: ObjectId) -> usize {
    let Some(live) = state.objects.try_get(object) else {
        return 0;
    };
    if live.zone != Zone::Battlefield
        || !crate::engine::object_has_type(state, object, CardType::Creature)
    {
        return 0;
    }
    active_permanents(state, HEXING_SQUELCHER, Some(live.controller))
        .filter(|&squelcher| squelcher != object)
        .count()
}

/// Whether any Hexing Squelcher could be granting life ward now.
pub(crate) fn life_ward_grant_exists(state: &GameState) -> bool {
    active_permanents(state, HEXING_SQUELCHER, None)
        .next()
        .is_some()
}

/// Null Rod: "Activated abilities of artifacts can't be activated."
pub(crate) fn activations_prohibited(state: &GameState, object: ObjectId) -> bool {
    state
        .objects
        .try_get(object)
        .is_some_and(|live| live.zone == Zone::Battlefield)
        && crate::engine::object_has_type(state, object, CardType::Artifact)
        && active_permanents(state, NULL_ROD, None).next().is_some()
}

/// Pithing Needle: "Activated abilities of sources with the chosen name
/// can't be activated unless they're mana abilities." Whether a non-mana
/// ability of `source` may be activated.
pub(crate) fn activation_allowed(state: &GameState, source: ObjectId) -> bool {
    let Some(lurrus) = read_state(state) else {
        return true;
    };
    if lurrus.needle_names.is_empty() {
        return true;
    }
    let name = crate::engine::effective_name(state, source);
    !lurrus.needle_names.iter().any(|(needle, named)| {
        link_live_in(state, *needle, Zone::Battlefield)
            && active_named(state, needle.object, PITHING_NEEDLE)
            && CARD_DEFS[usize::from(*named)].object_name == name
    })
}

/// Mox Opal: "Metalcraft -- {T}: Add one mana of any color. Activate only
/// if you control three or more artifacts."
pub(crate) fn mana_abilities_active(
    state: &GameState,
    object: ObjectId,
    name: &str,
) -> Option<bool> {
    if name != MOX_OPAL {
        return None;
    }
    let controller = state.objects.get(object).controller;
    let artifacts = state.players[controller.index()]
        .battlefield
        .iter()
        .filter(|&&permanent| {
            state.objects.get(permanent).controller == controller
                && crate::engine::object_has_type(state, permanent, CardType::Artifact)
        })
        .count();
    Some(artifacts >= 3)
}

/// Records how many colors of mana were spent to cast `spell` (converge),
/// for Prismatic Ending only.
pub(crate) fn note_colors_spent(state: &mut GameState, spell: ObjectId, colors: u8) {
    if definition_name(state, spell) != Some(PRISMATIC_ENDING) {
        return;
    }
    let link = live_link(state, spell);
    let on_stack = state
        .stack
        .iter()
        .map(|item| item.source)
        .collect::<Vec<_>>();
    let records = &mut lurrus_state(state).colors_spent;
    records.retain(|(record, _)| record.object != spell && on_stack.contains(&record.object));
    records.push((link, colors));
}

fn colors_spent(state: &GameState, spell: ObjectId) -> u8 {
    let Some(live) = state.objects.try_get(spell) else {
        return 0;
    };
    read_state(state)
        .and_then(|lurrus| {
            lurrus.colors_spent.iter().find(|(record, _)| {
                record.object == spell && record.zone_change_count == live.zone_change_count
            })
        })
        .map_or(0, |(_, colors)| *colors)
}

// ---- Pithing Needle's name ------------------------------------------------------------

/// The card names `player` may choose for a Pithing Needle: the names of
/// the objects visible to them, as definition ids in ascending order, one
/// per name. Visible objects are those on the battlefield, in graveyards,
/// in exile, on the stack and in `player`'s hand. Naming a card nobody can
/// see would have no effect this game, so the choice is limited to these
/// (a documented simplification of "choose a card name").
pub fn needle_name_options(state: &GameState, player: PlayerId) -> Vec<u16> {
    let mut objects: Vec<ObjectId> = Vec::new();
    for seat in [PlayerId::P0, PlayerId::P1] {
        objects.extend(state.players[seat.index()].battlefield.iter().copied());
        objects.extend(state.players[seat.index()].graveyard.iter().copied());
    }
    objects.extend(state.exile.iter().copied());
    objects.extend(state.stack.iter().map(|item| item.source));
    objects.extend(state.players[player.index()].hand.iter().copied());
    let mut options: Vec<u16> = Vec::new();
    let mut names: Vec<&str> = Vec::new();
    for object in objects {
        let Some(live) = state.objects.try_get(object) else {
            continue;
        };
        if live.v4.face_down_v1.is_some() {
            continue;
        }
        let Some(def) = CARD_DEFS.get(live.card_def as usize) else {
            continue;
        };
        if def.object_name.is_empty() || names.contains(&def.object_name) {
            continue;
        }
        names.push(def.object_name);
        options.push(live.card_def);
    }
    options.sort_unstable();
    options
}

/// The Pithing Needle that just entered and has not chosen a name.
fn needle_needing_name(state: &GameState) -> Option<ObjectId> {
    let id = crate::card_def::card_id_by_name(PITHING_NEEDLE)?;
    [PlayerId::P0, PlayerId::P1]
        .into_iter()
        .flat_map(|player| state.players[player.index()].battlefield.iter().copied())
        .find(|&object| {
            let live = state.objects.get(object);
            live.card_def == id
                && live.v4.face_index == 0
                && !read_state(state).is_some_and(|lurrus| {
                    lurrus
                        .needle_names
                        .iter()
                        .any(|(needle, _)| *needle == live_link(state, object))
                })
        })
}

/// "As this artifact enters, choose a card name": the pending choice, as an
/// option per visible name.
pub(crate) fn entry_choice_decision(state: &GameState) -> Option<crate::engine::Decision> {
    let needle = needle_needing_name(state)?;
    let player = state.objects.get(needle).controller;
    Some(crate::engine::Decision::ChooseEffectOption {
        player,
        source: needle,
        option_count: u16::try_from(needle_name_options(state, player).len())
            .unwrap_or(u16::MAX)
            .max(1),
    })
}

/// Answers the pending Pithing Needle name choice. `Ok(false)` when no
/// Needle is choosing.
pub(crate) fn answer_entry_choice(state: &mut GameState, option: u16) -> Result<bool, String> {
    let Some(needle) = needle_needing_name(state) else {
        return Ok(false);
    };
    let player = state.objects.get(needle).controller;
    let options = needle_name_options(state, player);
    let named = options
        .get(usize::from(option))
        .copied()
        .or_else(|| (option == 0).then(|| state.objects.get(needle).card_def))
        .ok_or("card name option is out of range")?;
    let link = live_link(state, needle);
    let names = &mut lurrus_state(state).needle_names;
    names.retain(|(host, _)| host.object != needle);
    names.push((link, named));
    Ok(true)
}

/// The definition whose name a Pithing Needle chose.
pub fn needle_name(state: &GameState, needle: ObjectId) -> Option<u16> {
    let link = live_link(state, needle);
    read_state(state)?
        .needle_names
        .iter()
        .find(|(host, _)| *host == link)
        .map(|(_, named)| *named)
}

// ---- Execution ----------------------------------------------------------------------

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

/// The resolving ability's source, if it is still the incarnation that
/// created the ability, on the battlefield.
fn live_ability_source(ctx: &ExecCtx, state: &GameState) -> Option<ObjectId> {
    let contract = ctx.ability_source_contract?;
    let live = state.objects.try_get(contract.source)?;
    (live.zone == Zone::Battlefield && live.zone_change_count == contract.zone_change_count)
        .then_some(contract.source)
}

pub(crate) fn execute(op: &LurrusOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        LurrusOpV1::LookAtTargetHandV1 => {
            let Some(Target::Player(player)) = ctx.targets.first().copied() else {
                return;
            };
            if player == ctx.controller || !ctx.target_incarnation_matches(0, state) {
                return;
            }
            for card in state.players[player.index()].hand.clone() {
                if state
                    .reveal_hand_card(ctx.controller, player, card)
                    .is_err()
                {
                    halt(state, ctx.source);
                    return;
                }
            }
        }
        LurrusOpV1::ExileTargetCreatureGainPowerV1 => {
            let Some(object) = legal_object_target(ctx, state, 0, TargetSpec::Creature) else {
                return;
            };
            let controller = state.objects.get(object).controller;
            let power = crate::engine::effective_power(state, object);
            event::propose_and_commit(state, ProposedEvent::zone_change(object, Zone::Exile));
            if power > 0 {
                event::propose_and_commit(state, ProposedEvent::life_gain(controller, power));
            }
        }
        LurrusOpV1::FlameOfAnorV1 {
            draw,
            artifact,
            creature,
            spec,
        } => {
            // Each chosen mode acts on its own target if that target is
            // still legal (608.2b); the others still happen.
            if let Some(slot) = *draw {
                let index = usize::from(slot);
                if let Some(Target::Player(_)) = ctx.targets.get(index) {
                    if ctx.target_incarnation_matches(index, state)
                        && crate::engine::effect_target_is_legal(
                            state,
                            ctx.source,
                            ctx.controller,
                            *spec,
                            &ctx.targets,
                            index,
                        )
                    {
                        crate::effect::execute(
                            &EffectOp::DrawCards {
                                player: PlayerRef::Target(slot),
                                count: 2,
                            },
                            ctx,
                            state,
                        );
                    }
                }
            }
            if let Some(slot) = *artifact {
                if legal_object_target(ctx, state, usize::from(slot), *spec).is_some() {
                    crate::effect::execute(
                        &EffectOp::DestroyObject {
                            object: ObjectRef::Target(slot),
                        },
                        ctx,
                        state,
                    );
                }
            }
            if let Some(slot) = *creature {
                if legal_object_target(ctx, state, usize::from(slot), *spec).is_some() {
                    crate::effect::execute(
                        &EffectOp::DealDamage {
                            target: crate::effect::TargetRef::Target(slot),
                            amount: 5,
                        },
                        ctx,
                        state,
                    );
                }
            }
        }
        LurrusOpV1::ForthEorlingasV1 { token_def } => {
            if ctx.x_value > 0 {
                crate::effect::execute(
                    &EffectOp::CreateTokensDynamic {
                        token_def: *token_def,
                        controller: PlayerRef::Controller,
                        count: crate::card_def::DynamicValueDef::Fixed(i32::from(ctx.x_value)),
                        tapped: false,
                    },
                    ctx,
                    state,
                );
            }
            let source = AbilitySourceContractV4::capture(state, ctx.source);
            let watch = MonarchWatchV1 {
                controller: ctx.controller,
                turn: state.turn_key_v1(),
                active: state.active_player,
                source,
            };
            lurrus_state(state).monarch_watches.push(watch);
        }
        LurrusOpV1::ManaDrainV1 => {
            let Some(Target::Object(spell)) = ctx.targets.first().copied() else {
                return;
            };
            if !ctx.target_incarnation_matches(0, state) {
                return;
            }
            let Some(item) = state
                .stack
                .iter()
                .find(|item| {
                    item.kind == crate::state::StackItemKind::Spell && item.source == spell
                })
                .cloned()
            else {
                return;
            };
            let amount = crate::engine::stack_spell_mana_value(state, &item);
            if crate::engine::counter_stack_item_by_id(state, item.v4.stack_item_id).is_err() {
                halt(state, ctx.source);
                return;
            }
            let source = AbilitySourceContractV4::capture(state, ctx.source);
            lurrus_state(state).mana_drains.push(ManaDrainV1 {
                controller: ctx.controller,
                source,
                amount,
            });
        }
        LurrusOpV1::AddColorlessV1 { amount } => {
            if *amount > 0 {
                event::propose_and_commit(
                    state,
                    ProposedEvent::mana_add(
                        ctx.controller,
                        vec![ManaColor::C; usize::from(*amount)],
                    ),
                );
            }
        }
        LurrusOpV1::ConvergeExileTargetV1 => {
            let Some(object) = legal_object_target(ctx, state, 0, TargetSpec::NonlandPermanent)
            else {
                return;
            };
            let colors = colors_spent(state, ctx.source);
            if crate::engine::object_mana_value(state, object) <= u16::from(colors) {
                event::propose_and_commit(state, ProposedEvent::zone_change(object, Zone::Exile));
            }
        }
        LurrusOpV1::GrantFlashbackV1 => {
            let Some(Target::Object(card)) = ctx.targets.first().copied() else {
                return;
            };
            let live = state.objects.get(card);
            if !ctx.target_incarnation_matches(0, state)
                || live.zone != Zone::Graveyard
                || live.owner != ctx.controller
            {
                return;
            }
            let grant = TurnGrantV1 {
                card: live_link(state, card),
                turn: state.turn_key_v1(),
                active: state.active_player,
            };
            let grants = &mut lurrus_state(state).flashback_grants;
            if !grants.contains(&grant) {
                grants.push(grant);
            }
        }
        LurrusOpV1::SourceGainsFlyingV1 => {
            let Some(source) = live_ability_source(ctx, state) else {
                return;
            };
            let binding = crate::effect::EffectObjectBinding {
                object: source,
                expected_zone: Zone::Battlefield,
                expected_zone_change_count: state.objects.get(source).zone_change_count,
            };
            crate::effect::install_temporary_boost(state, binding, 0, 0, Keywords::FLYING);
        }
        LurrusOpV1::FrogDrawV1 => crate::effect::execute(
            &EffectOp::DrawCards {
                player: PlayerRef::Controller,
                count: 1,
            },
            ctx,
            state,
        ),
        LurrusOpV1::BecomeMonarchV1 => crate::effect::execute(&EffectOp::BecomeMonarch, ctx, state),
    }
}
