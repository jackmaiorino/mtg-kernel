//! Card programs for the eternal catalog's wave E5a
//! (`docs/design/eternal_formats_support_v1.md`): Vintage Raker Shops,
//! Oath of Druids, Paradoxical Outcome, Blue Tinker and Beseech Storm, and
//! Legacy Mystic Forge.
//!
//! `eternal_cards_v1` forwards its effect leaves, trigger conditions and
//! state here, and the engine calls the hooks below only in
//! `eternal-fixtures` builds. Every table is keyed by a printed name that
//! occurs only in the eternal registry, so the Pauper, FDN and Standard
//! catalogs never reach it. Behavior was read from each card's current
//! Oracle text and, where present, its XMage source.
//!
//! Primitives added here:
//! - casting from the top of the library (Mystic Forge, Bolas's Citadel's
//!   life payment) through `SpellCastRouteV4::LibraryPermissionV1`, and
//!   Citadel's land play from the top;
//! - Yawgmoth's Will's graveyard play permission and exile replacement;
//! - phasing (Talon Gates of Madara): a phased-out permanent leaves every
//!   zone list and waits in the command-zone marker until its controller's
//!   next untap step, with the same incarnation;
//! - protection from a chosen card type for a player and their creatures
//!   (Serra's Emissary);
//! - "loses all abilities and becomes a 3/3 Elk" (Oko, Thief of Crowns);
//! - charge counters entering with X (Chalice of the Void).
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

pub mod choices;
mod execute;
pub(crate) mod permissions;
pub mod phasing;
pub(crate) mod statics;
mod triggers;

pub(crate) use execute::execute;
pub(crate) use triggers::{
    begin_turn, delayed_effect_matches, delayed_triggers, note_land_played, note_tapped_for_mana,
    trigger_matches, trigger_target_spec, triggers_for,
};

use crate::card_def::{card_id_by_name, CardDef, CardType, TargetSpec, CARD_DEFS};
use crate::effect::{EffectObjectBinding, EffectOp, PlayerRef};
use crate::event::CommittedEvent;
use crate::ids::{ObjectId, PlayerId};
use crate::mana::ManaColor;
use crate::standard_cards_v1::{StandardOpV1, StandardTargetV1};
use crate::state::{AbilitySourceContractV4, GameState, ObjectLinkV4, Zone};
use serde::{Deserialize, Serialize};

pub use choices::E5aChoiceV1;

/// Per-game state for this module's cards, kept inside
/// `eternal_cards_v1::EternalStateV1`. Default (and absent from snapshots)
/// until one of these cards needs it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct E5aStateV1 {
    /// Printed triggers detected by engine hooks (Forbidden Orchard's mana,
    /// The Endstone's land plays), waiting for the next trigger collection.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub queued_triggers: Vec<QueuedTriggerV1>,
    /// Library incarnations announced as spells through Mystic Forge or
    /// Bolas's Citadel, so the cast keeps its permission if the permanent
    /// leaves while the spell is on the stack.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub library_casts: Vec<LibraryCastV1>,
    /// Yawgmoth's Will effects, each for the turn it resolved.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub wills: Vec<TurnEffectV1>,
    /// Creatures exiled instead if they would die during the turn with
    /// this key and active player (Suplex).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exile_if_dies: Vec<(ObjectLinkV4, TurnEffectV1)>,
    /// Serra's Emissaries and the card type each chose, as an index into
    /// `PROTECTION_TYPES`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub emissary_types: Vec<(ObjectLinkV4, u8)>,
    /// Permanents Oko, Thief of Crowns's +1 made green 3/3 Elk creatures
    /// without abilities, with the effect's timestamp.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub elks: Vec<(ObjectLinkV4, u64)>,
    /// Phased-out permanents (Talon Gates of Madara).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub phased_out: Vec<phasing::PhasedOutV1>,
    /// The Fantasticars that are artifact creatures until end of turn.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub animated: Vec<(ObjectLinkV4, TurnEffectV1)>,
}

impl E5aStateV1 {
    pub(crate) fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// An effect for one turn: the player it belongs to, the turn key and the
/// active player of that turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TurnEffectV1 {
    pub player: PlayerId,
    pub turn: u32,
    pub active: PlayerId,
}

impl TurnEffectV1 {
    fn now(state: &GameState, player: PlayerId) -> Self {
        Self {
            player,
            turn: state.turn_key_v1(),
            active: state.active_player,
        }
    }

    fn current(&self, state: &GameState) -> bool {
        self.turn == state.turn_key_v1() && self.active == state.active_player
    }
}

/// One spell announced from the top of its owner's library.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LibraryCastV1 {
    /// The library incarnation the spell was cast from.
    pub card: ObjectLinkV4,
    pub holder: PlayerId,
    /// Bolas's Citadel: life equal to its mana value rather than its mana
    /// cost.
    pub pay_life: bool,
}

/// A triggered ability queued by an engine hook.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueuedTriggerV1 {
    pub source: AbilitySourceContractV4,
    pub controller: PlayerId,
    pub op: E5aOpV1,
}

/// Trigger conditions owned by this module, carried as
/// `eternal_cards_v1::EternalTriggerV1::E5aV1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum E5aTriggerV1 {
    /// "Whenever a player casts a spell with mana value equal to the number
    /// of charge counters on this artifact" (Chalice of the Void).
    SpellCastManaValueEqualsChargeCountersV1,
    /// "Whenever you cast a colorless spell" (Glaring Fleshraker, Ugin).
    ControllerCastsColorlessSpellV1,
    /// "Whenever another colorless creature you control enters" (Glaring
    /// Fleshraker).
    OtherColorlessControlledCreatureEntersV1,
    /// "At the beginning of combat on your turn, if you've cast a
    /// noncreature spell this turn" (Council of Reeds).
    ControllerCombatIfCastNoncreatureSpellV1,
    /// "At the beginning of your upkeep, if you control three or more
    /// artifacts" (Inventors' Fair).
    ControllerUpkeepIfThreeArtifactsV1,
    /// "Whenever you cast your fourth noncreature spell each turn" (The
    /// Fantasticar).
    ControllerCastsFourthNoncreatureSpellV1,
}

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::E5aV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum E5aOpV1 {
    /// Sensei's Divining Top: "Draw a card, then put this artifact on top
    /// of its owner's library."
    DrawThenSourceOnTopV1,
    /// Mystic Forge: "Exile the top card of your library."
    ExileTopOfLibraryV1,
    /// Chalice of the Void's resolution: it enters with X charge counters.
    EnterWithXChargeCountersV1,
    /// Inventors' Fair: "if you control three or more artifacts, you gain 1
    /// life", rechecked on resolution.
    GainLifeIfThreeArtifactsV1,
    /// The Fantasticar becomes an artifact creature until end of turn.
    AnimateSourceUntilEndOfTurnV1,
    /// The Fantasticar: sacrifice it, and if you do, create four 4/4
    /// Construct tokens.
    SacrificeSourceForConstructsV1,
    /// Talon Gates of Madara: target 0, if any, phases out.
    PhaseOutTargetV1,
    /// Talon Gates of Madara: "Put this card from your hand onto the
    /// battlefield."
    PutSourceOntoBattlefieldV1,
    /// Kozilek's Command's two chosen modes, each with the target slot it
    /// uses. `cards` is the first slot of the graveyard cards.
    KozilekV1 {
        spawn: Option<u8>,
        scry: Option<u8>,
        creature: Option<u8>,
        cards: Option<u8>,
        spec: TargetSpec,
    },
    /// Kozilek's Command: target player `slot` scries X.
    ScryTargetXV1 { slot: u8 },
    /// Kozilek's Command: exile target `slot` creature if its mana value is
    /// at most X.
    ExileCreatureManaValueAtMostXV1 { slot: u8, spec: TargetSpec },
    /// Kozilek's Command: exile the still-legal graveyard targets from
    /// `first` on.
    ExileGraveyardTargetsV1 { first: u8, spec: TargetSpec },
    /// Ugin: exile target 0, if any, if it is still a colored permanent.
    ExileColoredTargetV1,
    /// Ugin's -11: search for colorless nonland cards to exile and cast.
    UginSearchV1,
    /// Oath of Druids for the upkeep of `player`: the target check and the
    /// player's choice to reveal.
    OathV1 { player: PlayerId },
    /// Oath of Druids: `player` reveals until a creature card.
    OathRevealV1 { player: PlayerId },
    /// Forbidden Orchard: target opponent `player` creates a Spirit.
    OrchardSpiritV1 { player: PlayerId },
    /// Flash: put up to one creature card from hand onto the battlefield.
    FlashV1,
    /// Flash's payment: pay the creature's mana cost reduced by {2}, or
    /// sacrifice it.
    FlashPayV1 { creature: EffectObjectBinding },
    /// Flash's sacrifice when the cost is not paid.
    FlashSacrificeV1 { creature: EffectObjectBinding },
    /// Oko's +1 on target 0.
    OkoElkV1,
    /// Oko's -5: exchange control of targets 0 and 1.
    OkoExchangeV1,
    /// Suplex: 3 damage to target 0; it is exiled instead if it would die
    /// this turn.
    SuplexDamageV1,
    /// The Endstone: "your life total becomes half your starting life
    /// total, rounded up."
    LifeBecomesHalfStartingV1,
    /// Yawgmoth's Will's effect for the controller this turn.
    YawgmothsWillV1,
    /// Beseech the Mirror's search and cast.
    BeseechV1,
    /// Vampiric Tutor's search.
    VampiricV1,
    /// Council of Reeds: create a token copy, the intervening condition
    /// rechecked on resolution.
    CouncilCopyV1,
    /// The Endstone's land-play trigger: draw a card.
    EndstoneDrawV1,
}

fn e5a(op: E5aOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(
        crate::eternal_cards_v1::EternalOpV1::E5aV1(op),
    ))
}

impl E5aOpV1 {
    /// Whether this leaf can yield a player decision, so its program must use
    /// the resumable interpreter.
    pub(crate) fn contains_player_choice(&self) -> bool {
        matches!(
            self,
            E5aOpV1::KozilekV1 { .. }
                | E5aOpV1::ScryTargetXV1 { .. }
                | E5aOpV1::UginSearchV1
                | E5aOpV1::OathV1 { .. }
                | E5aOpV1::FlashV1
                | E5aOpV1::BeseechV1
                | E5aOpV1::VampiricV1
        )
    }
}

pub(crate) const BESEECH_THE_MIRROR: &str = "Beseech the Mirror";
pub(crate) const BOLAS_CITADEL: &str = "Bolas's Citadel";
pub(crate) const CHALICE_OF_THE_VOID: &str = "Chalice of the Void";
pub(crate) const COUNCIL_OF_REEDS: &str = "Council of Reeds";
pub(crate) const COUNCIL_OF_REEDS_TOKEN: &str = "Council of Reeds Token";
pub(crate) const FORBIDDEN_ORCHARD: &str = "Forbidden Orchard";
pub(crate) const GLARING_FLESHRAKER: &str = "Glaring Fleshraker";
pub(crate) const GRIM_MONOLITH: &str = "Grim Monolith";
pub(crate) const INVENTORS_FAIR: &str = "Inventors' Fair";
pub(crate) const KOZILEKS_COMMAND: &str = "Kozilek's Command";
pub(crate) const MYSTIC_FORGE: &str = "Mystic Forge";
pub(crate) const OATH_OF_DRUIDS: &str = "Oath of Druids";
pub(crate) const OKO: &str = "Oko, Thief of Crowns";
pub(crate) const SCRAWLING_CRAWLER: &str = "Scrawling Crawler";
pub(crate) const SERRAS_EMISSARY: &str = "Serra's Emissary";
pub(crate) const SUPLEX: &str = "Suplex";
pub(crate) const TALON_GATES: &str = "Talon Gates of Madara";
pub(crate) const TENDRILS_OF_AGONY: &str = "Tendrils of Agony";
pub(crate) const THE_ENDSTONE: &str = "The Endstone";
pub(crate) const THE_FANTASTICAR: &str = "The Fantasticar";
pub(crate) const FANTASTICAR_CONSTRUCT_TOKEN: &str = "Fantasticar Construct Token";
pub(crate) const UGIN: &str = "Ugin, Eye of the Storms";
pub(crate) const URZAS_WORKSHOP: &str = "Urza's Workshop";
pub(crate) const YAWGMOTHS_WILL: &str = "Yawgmoth's Will";
const ELDRAZI_SPAWN_TOKEN: &str = "Eldrazi Spawn Token";
const FOOD_TOKEN: &str = "Food Token";
const COLORLESS_SPIRIT_TOKEN: &str = "Colorless Spirit Token";

fn token(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name} in CARD_DEFS"))
}

pub(crate) fn e5a_state(state: &mut GameState) -> &mut E5aStateV1 {
    &mut state.eternal_v1.get_or_insert_with(Default::default).e5a
}

pub(crate) fn read_state(state: &GameState) -> Option<&E5aStateV1> {
    state.eternal_v1.as_ref().map(|eternal| &eternal.e5a)
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

pub(crate) fn bind(state: &GameState, object: ObjectId) -> EffectObjectBinding {
    let live = state.objects.get(object);
    EffectObjectBinding {
        object,
        expected_zone: live.zone,
        expected_zone_change_count: live.zone_change_count,
    }
}

pub(crate) fn binding_live(state: &GameState, binding: EffectObjectBinding) -> bool {
    state.objects.try_get(binding.object).is_some_and(|live| {
        live.zone == binding.expected_zone
            && live.zone_change_count == binding.expected_zone_change_count
    })
}

fn definition(state: &GameState, object: ObjectId) -> Option<&'static CardDef> {
    let live = state.objects.try_get(object)?;
    CARD_DEFS.get(live.card_def as usize)
}

/// Whether `object` is a front-face battlefield permanent named `name`
/// (or, for Council of Reeds, its token copy) whose printed abilities
/// apply.
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

/// The battlefield permanents named `name` whose printed abilities apply,
/// optionally only those `controller` controls.
pub(crate) fn active_permanents<'a>(
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

fn controls_active(state: &GameState, player: PlayerId, name: &str) -> bool {
    active_permanents(state, name, Some(player))
        .next()
        .is_some()
}

/// The permanents `player` controls.
fn controlled(state: &GameState, player: PlayerId) -> impl Iterator<Item = ObjectId> + '_ {
    [PlayerId::P0, PlayerId::P1]
        .into_iter()
        .flat_map(move |seat| state.players[seat.index()].battlefield.iter().copied())
        .filter(move |&object| {
            let live = state.objects.get(object);
            live.zone == Zone::Battlefield && live.controller == player
        })
}

fn controlled_with_type(state: &GameState, player: PlayerId, card_type: CardType) -> usize {
    controlled(state, player)
        .filter(|&object| crate::engine::object_has_type(state, object, card_type))
        .count()
}

/// Metalcraft and Inventors' Fair: three or more artifacts.
pub(crate) fn controls_three_artifacts(state: &GameState, player: PlayerId) -> bool {
    controlled_with_type(state, player, CardType::Artifact) >= 3
}

const WUBRG_MASK: u8 = (1 << 5) - 1;

/// Whether `object` is colorless (202.2b): no colored mana in its color.
pub(crate) fn is_colorless(state: &GameState, object: ObjectId) -> bool {
    crate::engine::object_color_mask(state, object) & WUBRG_MASK == 0
}

/// How many noncreature spells `player` has cast this turn, including one
/// whose cast was just committed. The turn begins at its `UpkeepBegan`
/// marker; no spell is cast earlier.
fn noncreature_spells_cast_this_turn(state: &GameState, player: PlayerId) -> usize {
    let mut count = 0;
    for event in state.engine.event_history.iter().rev() {
        match event {
            CommittedEvent::UpkeepBegan { .. } => break,
            CommittedEvent::SpellCast { spell, controller } if *controller == player => {
                if definition(state, *spell).is_some_and(|def| !def.has_type(CardType::Creature)) {
                    count += 1;
                }
            }
            _ => {}
        }
    }
    count
}

// ---- Programs named by the card database -------------------------------------------

/// Grim Monolith: "{4}: Untap this artifact."
pub fn untap_source() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(
        crate::eternal_cards_v1::EternalOpV1::VintageV1(
            crate::eternal_vintage_v1::VintageOpV1::UntapSourceV1,
        ),
    ))
}

/// Sensei's Divining Top: "{1}: Look at the top three cards of your
/// library, then put them back in any order."
pub fn top_look() -> EffectOp {
    EffectOp::LookAtLibraryTopAndReorder {
        player: PlayerRef::Controller,
        count: 3,
    }
}

/// Sensei's Divining Top: "{T}: Draw a card, then put this artifact on top
/// of its owner's library."
pub fn top_draw() -> EffectOp {
    e5a(E5aOpV1::DrawThenSourceOnTopV1)
}

/// Inventors' Fair: "Search your library for an artifact card, reveal it,
/// put it into your hand, then shuffle."
pub fn inventors_fair_search() -> EffectOp {
    EffectOp::SearchLibraryToHand {
        player: PlayerRef::Controller,
        filter: crate::effect::LibraryCardFilter::CardWithTypeV1(CardType::Artifact),
    }
}

/// Mystic Forge: "{T}, Pay 1 life: Exile the top card of your library."
pub fn exile_top_card() -> EffectOp {
    e5a(E5aOpV1::ExileTopOfLibraryV1)
}

/// Bolas's Citadel: "Each opponent loses 10 life."
pub fn citadel_drain() -> EffectOp {
    EffectOp::LoseLife {
        player: PlayerRef::Opponent,
        amount: 10,
    }
}

/// Oko, Thief of Crowns's +2: "Create a Food token."
pub fn oko_food() -> EffectOp {
    EffectOp::CreateToken {
        token_def: token(FOOD_TOKEN),
        controller: PlayerRef::Controller,
    }
}

/// Oko's +1: "Target artifact or creature loses all abilities and becomes
/// a green Elk creature with base power and toughness 3/3."
pub fn oko_elk() -> EffectOp {
    e5a(E5aOpV1::OkoElkV1)
}

/// Oko's -5: "Exchange control of target artifact or creature you control
/// and target creature an opponent controls with power 3 or less."
pub fn oko_exchange() -> EffectOp {
    e5a(E5aOpV1::OkoExchangeV1)
}

/// Ugin, Eye of the Storms's +2: "You gain 3 life and draw a card."
pub fn ugin_plus_two() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::GainLife {
            player: PlayerRef::Controller,
            amount: 3,
        },
        EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 1,
        },
    ])
}

/// Ugin's 0: "Add {C}{C}{C}." A loyalty ability, so not a mana ability
/// (605.1a): it uses the stack.
pub fn ugin_zero() -> EffectOp {
    EffectOp::AddMana {
        player: PlayerRef::Controller,
        colors: vec![ManaColor::C, ManaColor::C, ManaColor::C],
    }
}

/// Ugin's -11: "Search your library for any number of colorless nonland
/// cards, exile them, then shuffle. Until end of turn, you may cast those
/// cards without paying their mana costs."
pub fn ugin_minus_eleven() -> EffectOp {
    e5a(E5aOpV1::UginSearchV1)
}

/// Talon Gates of Madara: "{4}: Put this card from your hand onto the
/// battlefield."
pub fn talon_gates_put() -> EffectOp {
    e5a(E5aOpV1::PutSourceOntoBattlefieldV1)
}

/// Beseech the Mirror: "Search your library for a card, exile it face down,
/// then shuffle. If this spell was bargained, you may cast the exiled card
/// without paying its mana cost if that spell's mana value is 4 or less.
/// Put the exiled card into your hand if it wasn't cast this way." The
/// found card waits in its owner's hand, hidden from the opponent as it is
/// face down in exile, and is cast from there during the resolution; the
/// card ends in the hand exactly when it was not cast.
pub fn beseech_the_mirror() -> EffectOp {
    e5a(E5aOpV1::BeseechV1)
}

/// Chalice of the Void: "This artifact enters with X charge counters on
/// it."
pub fn chalice_of_the_void() -> EffectOp {
    e5a(E5aOpV1::EnterWithXChargeCountersV1)
}

/// Extrapolate the Impossible: "You may reveal exactly two cards you own
/// with different names from outside the game." A best-of-one mainboard
/// game has no cards outside the game, so the spell does nothing.
pub fn extrapolate_the_impossible() -> EffectOp {
    EffectOp::Sequence(Vec::new())
}

/// Flash: "You may put a creature card from your hand onto the
/// battlefield. If you do, sacrifice it unless you pay its mana cost
/// reduced by {2}."
pub fn flash() -> EffectOp {
    e5a(E5aOpV1::FlashV1)
}

fn kozilek(
    spawn: Option<u8>,
    scry: Option<u8>,
    creature: Option<u8>,
    cards: Option<u8>,
    spec: StandardTargetV1,
) -> EffectOp {
    e5a(E5aOpV1::KozilekV1 {
        spawn,
        scry,
        creature,
        cards,
        spec: TargetSpec::StandardV1(spec),
    })
}

/// Kozilek's Command's six pairs of modes ("Choose two"): Spawn tokens and
/// scry, Spawn tokens and exile a creature, Spawn tokens and exile cards,
/// scry and exile a creature, scry and exile cards, and exile a creature
/// and cards.
pub fn kozilek_spawn_scry() -> EffectOp {
    kozilek(
        Some(0),
        Some(1),
        None,
        None,
        StandardTargetV1::PlayerThenPlayerV1,
    )
}

pub fn kozilek_spawn_creature() -> EffectOp {
    kozilek(
        Some(0),
        None,
        Some(1),
        None,
        StandardTargetV1::PlayerThenCreatureV1,
    )
}

pub fn kozilek_spawn_cards() -> EffectOp {
    kozilek(
        Some(0),
        None,
        None,
        Some(1),
        StandardTargetV1::PlayerThenGraveyardCardsV1,
    )
}

pub fn kozilek_scry_creature() -> EffectOp {
    kozilek(
        None,
        Some(0),
        Some(1),
        None,
        StandardTargetV1::PlayerThenCreatureV1,
    )
}

pub fn kozilek_scry_cards() -> EffectOp {
    kozilek(
        None,
        Some(0),
        None,
        Some(1),
        StandardTargetV1::PlayerThenGraveyardCardsV1,
    )
}

pub fn kozilek_creature_cards() -> EffectOp {
    kozilek(
        None,
        None,
        Some(0),
        Some(1),
        StandardTargetV1::CreatureThenGraveyardCardsV1,
    )
}

/// Suplex: "Suplex deals 3 damage to target creature. If that creature
/// would die this turn, exile it instead."
pub fn suplex_damage() -> EffectOp {
    e5a(E5aOpV1::SuplexDamageV1)
}

/// Suplex: "Exile target artifact."
pub fn suplex_exile() -> EffectOp {
    EffectOp::MoveAllTargets {
        to_zone: Zone::Exile,
    }
}

/// Tendrils of Agony: "Target player loses 2 life and you gain 2 life."
/// Storm is its cast trigger (`eternal_cards_v1::e3`).
pub fn tendrils_of_agony() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::LoseLife {
            player: PlayerRef::Target(0),
            amount: 2,
        },
        EffectOp::GainLife {
            player: PlayerRef::Controller,
            amount: 2,
        },
    ])
}

/// Vampiric Tutor: "Search your library for a card, then shuffle and put
/// that card on top. You lose 2 life." Only the searcher learns the card.
pub fn vampiric_tutor() -> EffectOp {
    EffectOp::Sequence(vec![
        e5a(E5aOpV1::VampiricV1),
        EffectOp::LoseLife {
            player: PlayerRef::Controller,
            amount: 2,
        },
    ])
}

/// Yawgmoth's Will: "Until end of turn, you may play lands and cast spells
/// from your graveyard. If a card would be put into your graveyard from
/// anywhere this turn, exile that card instead."
pub fn yawgmoths_will() -> EffectOp {
    e5a(E5aOpV1::YawgmothsWillV1)
}
