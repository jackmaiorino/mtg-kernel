//! Card programs for the eternal catalog's Premodern wave P1a
//! (`docs/design/eternal_formats_support_v1.md`): Stiflenought, Sligh, RG
//! Aggro, Goblins, Terrageddon and Mono-Black Aggro, with the cards they
//! share with wave P1b.
//!
//! `eternal_cards_v1` forwards its effect leaves and state here, wave E9a's
//! tables fall through to this module's, and the engine calls the hooks
//! below only in `eternal-fixtures` builds. Every table is keyed by a printed
//! name that occurs only in the eternal registry, so the Pauper, FDN and
//! Standard catalogs never reach it. Behavior follows each card's current
//! Oracle text (no mana burn, no damage on the stack), checked against its
//! XMage source where one exists.
//!
//! Primitives added here:
//! - split cards (Fire // Ice): the second half is the Adventure form, and
//!   it goes to the graveyard, not exile; a half on the stack has only its
//!   own mana value and colors;
//! - morph (Exalted Angel): disguise's face-down cast without its ward,
//!   turned face up for the morph cost;
//! - "exile X cards from your graveyard" costs (Skeletal Scrying, Flash of
//!   Insight's flashback) and Foil's "discard an Island card and another
//!   card";
//! - printed protection from a color, swampwalk until end of turn, "can't be
//!   regenerated", "assigns no combat damage", "players gain no life";
//! - mining and fuse counters, Lair's return, the cycle trigger, damage
//!   triggers (dealt by and dealt to a creature), "whenever a creature dies"
//!   untaps, a land type change chosen on resolution (Vision Charm);
//! - naming a card (Cursed Scroll) and a random hand reveal, "cards drawn
//!   this turn" (Sylvan Library), reordering another player's library
//!   (Portent).
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

pub mod statics;
pub(crate) mod targets;

use crate::card_def::{card_id_by_name, CardType, Keywords, Subtype, TargetSpec, CARD_DEFS};
use crate::effect::{
    EffectContinuation, EffectObjectBinding, EffectOp, ExecCtx, ObjectRef, PlayerRef, TargetRef,
};
use crate::eternal_cards_v1::e3::{E3ChoiceKindV1, E3ChoiceV1};
use crate::eternal_cards_v1::EternalOpV1;
use crate::eternal_legacy_e9a_v1::{
    active_named, active_permanents, battlefield, definition, definition_name, left_as_creature,
    link_live_in, live_link, TurnV1,
};
use crate::event::{self, CommittedEvent, LibraryPlacement, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::mana::ManaColor;
use crate::standard_cards_v1::{StandardOpV1, StandardTargetV1};
use crate::state::{AbilitySourceContractV4, GameState, ObjectLinkV4, Target, Zone};
use crate::trigger::{PendingTrigger, TriggerCondition, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};

// ---- State ------------------------------------------------------------------------

/// Per-game state for this module's cards, kept inside
/// `eternal_cards_v1::EternalStateV1`. Default (and absent from snapshots)
/// until one of these cards needs it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct P1aStateV1 {
    /// Triggers detected by hooks, waiting for the next trigger collection.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub queued_triggers: Vec<QueuedTriggerV1>,
    /// Gempalm Incinerators just cycled, by their owner, waiting for the
    /// next trigger collection.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cycled: Vec<(ObjectId, PlayerId)>,
    /// Anurid Brushhoppers exiled until the beginning of the next end step.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub end_step_returns: Vec<EndStepReturnV1>,
    /// Mining counters on each Gemstone Mine incarnation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mining: Vec<(ObjectLinkV4, u8)>,
    /// Fuse counters on each Powder Keg incarnation, kept after it leaves
    /// while its ability may still read them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fuse: Vec<(ObjectLinkV4, u16)>,
    /// Creatures that can't be regenerated this turn (Incinerate), or during
    /// one destruction (Smother).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub no_regeneration: Vec<(ObjectLinkV4, TurnV1)>,
    /// Creatures with swampwalk until end of turn (Funeral Charm).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub swampwalk: Vec<(ObjectLinkV4, TurnV1)>,
    /// Creatures that became a color until end of turn (Wild Mongrel), with
    /// the effect's timestamp.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub colors: Vec<ColorChangeV1>,
    /// Lands whose land type Vision Charm changed until end of turn.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub land_types: Vec<LandTypeChangeV1>,
    /// Creatures that assign no combat damage this turn (Goblin Vandal).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub no_combat_damage: Vec<(ObjectLinkV4, TurnV1)>,
}

impl P1aStateV1 {
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

/// Anurid Brushhopper: "Return it to the battlefield under its owner's
/// control at the beginning of the next end step."
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EndStepReturnV1 {
    pub source: AbilitySourceContractV4,
    pub controller: PlayerId,
    /// The exiled card's exile incarnation.
    pub card: ObjectLinkV4,
    /// The turn whose end step had already begun when the card was exiled;
    /// it waits for a later end step.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_in_end_step: Option<TurnV1>,
}

/// Wild Mongrel: "becomes the color of your choice until end of turn."
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColorChangeV1 {
    pub object: ObjectLinkV4,
    pub color: ManaColor,
    pub timestamp: u64,
    pub turn: TurnV1,
}

/// Vision Charm: one land that became `to` until end of turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LandTypeChangeV1 {
    pub land: ObjectLinkV4,
    pub to: Subtype,
    pub timestamp: u64,
    pub turn: TurnV1,
}

pub(crate) fn p1a_state(state: &mut GameState) -> &mut P1aStateV1 {
    &mut state.eternal_v1.get_or_insert_with(Default::default).p1a
}

pub(crate) fn read_state(state: &GameState) -> Option<&P1aStateV1> {
    state.eternal_v1.as_ref().map(|eternal| &eternal.p1a)
}

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::P1aV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum P1aOpV1 {
    /// Armageddon: destroy all lands.
    ArmageddonV1,
    /// Destroy target 0 (Disenchant, Naturalize).
    DestroyTargetV1,
    /// Fire: its damage divided among its targets.
    FireV1,
    /// Ice: tap target 0, then draw a card.
    IceV1,
    /// Flash of Insight: look at the top X cards. A choice.
    FlashOfInsightV1,
    /// Funeral Charm: target creature 0 gains swampwalk until end of turn.
    SwampwalkV1,
    /// Incinerate: 3 damage to target 0, which can't be regenerated.
    IncinerateV1,
    /// Portent's look and reorder of target player 0's library. A choice.
    PortentV1,
    /// "You may have that player shuffle": shuffle target player 0's
    /// library.
    ShuffleTargetV1,
    /// Portent: draw a card at the beginning of the next turn's upkeep.
    PortentDrawV1,
    /// Price of Progress.
    PriceOfProgressV1,
    /// Skeletal Scrying: "You draw X cards and you lose X life."
    SkeletalScryingV1,
    /// Smother: destroy target 0, which can't be regenerated.
    SmotherV1,
    /// Vision Charm's land type change from `from` to the basic land type
    /// `to`.
    VisionLandsV1 { from: Subtype, to: Subtype },
    /// Vision Charm: target artifact 0 phases out.
    PhaseOutV1,
    /// Anurid Brushhopper exiles itself until the next end step.
    BrushhopperExileV1,
    /// The return: `card` comes back under its owner's control.
    BrushhopperReturnV1 { card: ObjectLinkV4 },
    /// Cursed Scroll with `named` (a definition id) as the named card.
    CursedScrollV1 { named: u16 },
    /// Knight of Stromgald: the source gains first strike until end of turn.
    SourceFirstStrikeV1,
    /// Powder Keg's destruction for the fuse counters on its source.
    PowderKegV1,
    /// Powder Keg: a fuse counter on the source.
    FuseCounterV1,
    /// Wild Mongrel: +1/+1 until end of turn.
    MongrelPumpV1,
    /// Wild Mongrel: becomes `color` until end of turn.
    MongrelColorV1 { color: ManaColor },
    /// "Sacrifice this permanent" from a triggered ability (Ball
    /// Lightning).
    SacrificeSourceV1,
    /// Gempalm Incinerator's cycle trigger on target creature 0.
    GempalmDamageV1,
    /// Goblin Lackey's damage trigger. A choice.
    LackeyPutV1,
    /// Goblin Matron's search. A choice.
    MatronSearchV1,
    /// Goblin Piledriver's attack trigger.
    PiledriverPumpV1,
    /// Goblin Ringleader's reveal. May be a choice (the bottom order).
    RingleaderV1,
    /// Goblin Sharpshooter untaps.
    UntapSourceV1,
    /// Goblin Vandal's paid destruction of target artifact 0.
    VandalDestroyV1,
    /// Graveborn Muse's upkeep draw.
    GravebornMuseV1,
    /// Hypnotic Specter: `player` discards a card at random.
    RandomDiscardV1 { player: PlayerId },
    /// Jackal Pup deals `amount` damage to its controller.
    PupDamageV1 { amount: i32 },
    /// Rith's Grove's entry. A choice.
    GroveV1,
    /// Siege-Gang Commander's three Goblins.
    SiegeGangTokensV1,
    /// Sulfuric Vortex: 2 damage to the player whose upkeep it is.
    VortexDamageV1 { player: PlayerId },
    /// Sylvan Library's trigger. A choice.
    SylvanLibraryV1,
    /// Sylvan Library's draw of two additional cards, then its choice.
    SylvanDrawV1,
    /// A declined "you may" option.
    NoEffectV1,
}

impl P1aOpV1 {
    /// Whether this leaf can yield a player decision, so its program must
    /// use the resumable interpreter.
    pub(crate) fn contains_player_choice(&self) -> bool {
        matches!(
            self,
            Self::FlashOfInsightV1
                | Self::PortentV1
                | Self::LackeyPutV1
                | Self::MatronSearchV1
                | Self::RingleaderV1
                | Self::GroveV1
                | Self::SylvanLibraryV1
                | Self::SylvanDrawV1
        )
    }
}

/// Resolution choices owned by this module, carried as
/// `E3ChoiceKindV1::P1aV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum P1aChoiceV1 {
    /// Portent: the order of target player's top cards, top first.
    PortentOrderV1 { player: PlayerId },
    /// Goblin Lackey: up to one Goblin permanent card in hand.
    LackeyPutV1,
    /// Goblin Ringleader: the order of the other revealed cards on the
    /// bottom, the first chosen highest.
    RingleaderBottomV1,
    /// Rith's Grove: up to one non-Lair land to return.
    GroveReturnV1,
    /// Sylvan Library: two cards in hand drawn this turn, the first chosen
    /// put back first.
    SylvanChooseV1,
    /// Sylvan Library: which of the `chosen` cards to pay 4 life for.
    SylvanKeepV1 { chosen: Vec<EffectObjectBinding> },
}

impl P1aChoiceV1 {
    /// Whether the candidates are hidden from the other player.
    pub(crate) fn chooser_private(&self) -> bool {
        matches!(
            self,
            Self::PortentOrderV1 { .. }
                | Self::LackeyPutV1
                | Self::SylvanChooseV1
                | Self::SylvanKeepV1 { .. }
        )
    }

    /// Whether the chooser sees library cards it has not looked at before.
    pub(crate) fn library_search(&self) -> bool {
        matches!(self, Self::PortentOrderV1 { .. })
    }

    pub(crate) fn public_purpose(&self) -> crate::rl::TargetSelectionPurposeV4 {
        match self {
            Self::GroveReturnV1 => crate::rl::TargetSelectionPurposeV4::PermanentSelection,
            _ => crate::rl::TargetSelectionPurposeV4::CardSelection,
        }
    }

    /// Object incarnations this choice refers to beyond its candidates.
    pub(crate) fn referenced_bindings(&self) -> Vec<EffectObjectBinding> {
        match self {
            Self::SylvanKeepV1 { chosen } => chosen.clone(),
            _ => Vec::new(),
        }
    }
}

// ---- Names and helpers --------------------------------------------------------------

pub(crate) const ANURID_BRUSHHOPPER: &str = "Anurid Brushhopper";
pub(crate) const BALL_LIGHTNING: &str = "Ball Lightning";
pub(crate) const BARBARIAN_RING: &str = "Barbarian Ring";
pub(crate) const EXALTED_ANGEL: &str = "Exalted Angel";
pub(crate) const FIRE_ICE: &str = "Fire // Ice";
pub(crate) const GEMPALM_INCINERATOR: &str = "Gempalm Incinerator";
pub(crate) const GEMSTONE_MINE: &str = "Gemstone Mine";
pub(crate) const GOBLIN_GOON: &str = "Goblin Goon";
const GOBLIN_LACKEY: &str = "Goblin Lackey";
const GOBLIN_MATRON: &str = "Goblin Matron";
pub(crate) const GOBLIN_PILEDRIVER: &str = "Goblin Piledriver";
const GOBLIN_RINGLEADER: &str = "Goblin Ringleader";
pub(crate) const GOBLIN_SHARPSHOOTER: &str = "Goblin Sharpshooter";
pub(crate) const GOBLIN_VANDAL: &str = "Goblin Vandal";
pub(crate) const GOBLIN_WARCHIEF: &str = "Goblin Warchief";
const GRAVEBORN_MUSE: &str = "Graveborn Muse";
const HYPNOTIC_SPECTER: &str = "Hypnotic Specter";
const JACKAL_PUP: &str = "Jackal Pup";
pub(crate) const KNIGHT_OF_STROMGALD: &str = "Knight of Stromgald";
pub(crate) const MYSTIC_ENFORCER: &str = "Mystic Enforcer";
const PORTENT: &str = "Portent";
pub(crate) const POWDER_KEG: &str = "Powder Keg";
const RITHS_GROVE: &str = "Rith's Grove";
const SIEGE_GANG_COMMANDER: &str = "Siege-Gang Commander";
const SULFURIC_VORTEX: &str = "Sulfuric Vortex";
pub(crate) const SYLVAN_LIBRARY: &str = "Sylvan Library";
pub(crate) const TERRAVORE: &str = "Terravore";
pub(crate) const WEATHERED_WAYFARER: &str = "Weathered Wayfarer";
const ELEPHANT_TOKEN: &str = "Elephant Token";
const GOBLIN_TOKEN: &str = "Goblin Token";

fn p1a(op: P1aOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::P1aV1(op)))
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

fn bind(state: &GameState, object: ObjectId) -> EffectObjectBinding {
    let live = state.objects.get(object);
    EffectObjectBinding {
        object,
        expected_zone: live.zone,
        expected_zone_change_count: live.zone_change_count,
    }
}

fn has_type(state: &GameState, object: ObjectId, card_type: CardType) -> bool {
    crate::engine::object_has_type(state, object, card_type)
}

fn is_goblin(state: &GameState, object: ObjectId) -> bool {
    crate::engine::has_effective_subtype(state, object, Subtype::Goblin)
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

/// Target 0 if it is still a legal target of any kind under `spec`.
fn legal_any_target(ctx: &ExecCtx, state: &GameState, spec: TargetSpec) -> Option<Target> {
    let target = ctx.targets.first().copied()?;
    (ctx.target_incarnation_matches(0, state)
        && crate::engine::effect_target_is_legal(
            state,
            ctx.source,
            ctx.controller,
            spec,
            &ctx.targets,
            0,
        ))
    .then_some(target)
}

fn queue_trigger(
    state: &mut GameState,
    source: AbilitySourceContractV4,
    controller: PlayerId,
    effect: EffectOp,
    target_spec: Option<TargetSpec>,
) {
    p1a_state(state).queued_triggers.push(QueuedTriggerV1 {
        source,
        controller,
        effect,
        target_spec,
    });
}

fn shuffle(state: &mut GameState, player: PlayerId) -> Result<(), String> {
    state
        .shuffle_library(player)
        .map_err(|error| format!("{error:?}"))
}

/// Destroys `objects` as one simultaneous event: indestructible permanents
/// stay, and a regeneration shield replaces a destruction.
fn destroy_batch(state: &mut GameState, objects: Vec<ObjectId>) {
    let destroyed = objects
        .into_iter()
        .filter(|&id| {
            state.objects.get(id).zone == Zone::Battlefield
                && !crate::engine::has_effective_keyword(state, id, Keywords::INDESTRUCTIBLE)
        })
        .collect::<Vec<_>>();
    let events = destroyed
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

/// Records that `creature`'s current incarnation can't be regenerated this
/// turn.
fn forbid_regeneration(state: &mut GameState, creature: ObjectId) {
    let record = (live_link(state, creature), TurnV1::now(state));
    let records = &mut p1a_state(state).no_regeneration;
    if !records.contains(&record) {
        records.push(record);
    }
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

/// The cards `player` owns in their hand.
fn hand_cards(state: &GameState, player: PlayerId) -> Vec<ObjectId> {
    state.players[player.index()]
        .hand
        .iter()
        .copied()
        .filter(|&card| state.objects.get(card).owner == player)
        .collect()
}

/// The other attacking Goblins than `source`.
fn other_attacking_goblins(state: &GameState, source: ObjectId) -> usize {
    state
        .engine
        .combat
        .attackers
        .iter()
        .filter(|&&attacker| {
            attacker != source
                && state.objects.get(attacker).zone == Zone::Battlefield
                && has_type(state, attacker, CardType::Creature)
                && is_goblin(state, attacker)
        })
        .count()
}

/// The Goblins on the battlefield.
fn goblins_on_battlefield(state: &GameState) -> usize {
    battlefield(state)
        .filter(|&object| is_goblin(state, object))
        .count()
}

// ---- Spells -------------------------------------------------------------------------

/// Armageddon: "Destroy all lands."
pub fn armageddon() -> EffectOp {
    p1a(P1aOpV1::ArmageddonV1)
}

/// Call of the Herd: "Create a 3/3 green Elephant creature token." Its
/// flashback is the card's own `CardDef::flashback`.
pub fn call_of_the_herd() -> EffectOp {
    EffectOp::CreateToken {
        token_def: token_id(ELEPHANT_TOKEN),
        controller: PlayerRef::Controller,
    }
}

/// Diabolic Edict: "Target player sacrifices a creature."
pub fn diabolic_edict() -> EffectOp {
    EffectOp::SacrificeCreature {
        player: PlayerRef::Target(0),
        filter: crate::effect::CreatureSacrificeFilter::Any,
    }
}

/// Disenchant and Naturalize: "Destroy target artifact or enchantment."
pub fn destroy_target() -> EffectOp {
    p1a(P1aOpV1::DestroyTargetV1)
}

/// Fire: "Fire deals 2 damage divided as you choose among one or two
/// targets."
pub fn fire() -> EffectOp {
    p1a(P1aOpV1::FireV1)
}

/// Ice: "Tap target permanent. Draw a card."
pub fn ice() -> EffectOp {
    p1a(P1aOpV1::IceV1)
}

/// Flash of Insight: "Look at the top X cards of your library. Put one of
/// them into your hand and the rest on the bottom of your library in any
/// order." Its flashback's X is the number of blue cards it exiled.
pub fn flash_of_insight() -> EffectOp {
    p1a(P1aOpV1::FlashOfInsightV1)
}

/// Funeral Charm's first mode: "Target player discards a card."
pub fn funeral_charm_discard() -> EffectOp {
    EffectOp::DiscardCards {
        player: PlayerRef::Target(0),
        count: 1,
    }
}

/// Funeral Charm's second mode: "Target creature gets +2/-1 until end of
/// turn."
pub fn funeral_charm_pump() -> EffectOp {
    EffectOp::PumpTargetUntilEndOfTurnDynamic {
        target: TargetRef::Target(0),
        power: crate::card_def::DynamicValueDef::Fixed(2),
        toughness: crate::card_def::DynamicValueDef::Fixed(-1),
    }
}

/// Funeral Charm's third mode: "Target creature gains swampwalk until end
/// of turn."
pub fn funeral_charm_swampwalk() -> EffectOp {
    p1a(P1aOpV1::SwampwalkV1)
}

/// Incinerate: "Incinerate deals 3 damage to any target. A creature dealt
/// damage this way can't be regenerated this turn."
pub fn incinerate() -> EffectOp {
    p1a(P1aOpV1::IncinerateV1)
}

/// Portent: "Look at the top three cards of target player's library, then
/// put them back in any order. You may have that player shuffle. Draw a
/// card at the beginning of the next turn's upkeep."
pub fn portent() -> EffectOp {
    EffectOp::Sequence(vec![
        p1a(P1aOpV1::PortentV1),
        EffectOp::Choice {
            controller: PlayerRef::Controller,
            options: vec![p1a(P1aOpV1::ShuffleTargetV1), p1a(P1aOpV1::NoEffectV1)],
        },
        p1a(P1aOpV1::PortentDrawV1),
    ])
}

/// Price of Progress: "Price of Progress deals damage to each player equal
/// to twice the number of nonbasic lands that player controls."
pub fn price_of_progress() -> EffectOp {
    p1a(P1aOpV1::PriceOfProgressV1)
}

/// Skeletal Scrying: "You draw X cards and you lose X life." Its exile is
/// the card's additional cost. The draws are their own leaf, so dredge can
/// replace them.
pub fn skeletal_scrying() -> EffectOp {
    p1a(P1aOpV1::SkeletalScryingV1)
}

/// Smother: "Destroy target creature with mana value 3 or less. It can't
/// be regenerated."
pub fn smother() -> EffectOp {
    p1a(P1aOpV1::SmotherV1)
}

/// Vision Charm's first mode: "Target player mills four cards."
pub fn vision_charm_mill() -> EffectOp {
    EffectOp::MillCards {
        player: PlayerRef::Target(0),
        count: 4,
    }
}

/// Vision Charm's second mode: "Choose a land type and a basic land type.
/// Each land of the first chosen type becomes the second chosen type until
/// end of turn." The land type, then the basic land type, are chosen as it
/// resolves.
pub fn vision_charm_lands() -> EffectOp {
    EffectOp::Choice {
        controller: PlayerRef::Controller,
        options: crate::eternal_legacy_e8a_v1::statics::LAND_TYPES
            .iter()
            .map(|&from| EffectOp::Choice {
                controller: PlayerRef::Controller,
                options: statics::BASIC_LAND_TYPES
                    .iter()
                    .map(|&(to, _)| p1a(P1aOpV1::VisionLandsV1 { from, to }))
                    .collect(),
            })
            .collect(),
    }
}

/// Vision Charm's third mode: "Target artifact phases out."
pub fn vision_charm_phase() -> EffectOp {
    p1a(P1aOpV1::PhaseOutV1)
}

// ---- Activated abilities --------------------------------------------------------------

/// Anurid Brushhopper's ability.
pub fn brushhopper_exile() -> EffectOp {
    p1a(P1aOpV1::BrushhopperExileV1)
}

/// Cursed Scroll: "Name a card. Reveal a card at random from your hand. If
/// it's the named card, Cursed Scroll deals 2 damage to any target." The
/// name is chosen by first letter, then from the names with that letter.
pub fn cursed_scroll() -> EffectOp {
    EffectOp::Choice {
        controller: PlayerRef::Controller,
        options: card_names()
            .iter()
            .map(|ids| EffectOp::Choice {
                controller: PlayerRef::Controller,
                options: ids
                    .iter()
                    .map(|&named| p1a(P1aOpV1::CursedScrollV1 { named }))
                    .collect(),
            })
            .collect(),
    }
}

/// Every card name, once, grouped by first letter.
fn card_names() -> &'static [Vec<u16>] {
    static TABLE: std::sync::OnceLock<Vec<Vec<u16>>> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        let mut seen: Vec<&str> = Vec::new();
        let mut groups: Vec<(char, Vec<u16>)> = Vec::new();
        for (index, def) in CARD_DEFS.iter().enumerate() {
            if def.is_token
                || !def.is_executable()
                || def.object_name.is_empty()
                || seen.contains(&def.object_name)
            {
                continue;
            }
            let Ok(id) = u16::try_from(index) else {
                continue;
            };
            seen.push(def.object_name);
            let letter = def
                .object_name
                .chars()
                .next()
                .map_or(' ', |letter| letter.to_ascii_uppercase());
            match groups.iter_mut().find(|(group, _)| *group == letter) {
                Some((_, ids)) => ids.push(id),
                None => groups.push((letter, vec![id])),
            }
        }
        groups.sort_by_key(|(letter, _)| *letter);
        groups.into_iter().map(|(_, ids)| ids).collect()
    })
}

/// Knight of Stromgald's first strike.
pub fn knight_first_strike() -> EffectOp {
    p1a(P1aOpV1::SourceFirstStrikeV1)
}

/// Powder Keg's sacrifice ability.
pub fn powder_keg() -> EffectOp {
    p1a(P1aOpV1::PowderKegV1)
}

/// Wild Mongrel: "This creature gets +1/+1 and becomes the color of your
/// choice until end of turn." One color option per color, in WUBRG order.
pub fn wild_mongrel() -> EffectOp {
    EffectOp::Sequence(vec![
        p1a(P1aOpV1::MongrelPumpV1),
        EffectOp::Choice {
            controller: PlayerRef::Controller,
            options: crate::eternal_legacy_e9a_v1::statics::COLORS
                .into_iter()
                .map(|color| p1a(P1aOpV1::MongrelColorV1 { color }))
                .collect(),
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

/// Goblin Matron: "When this creature enters, you may search your library
/// for a Goblin card, reveal it, put it into your hand, then shuffle."
fn matron_enters() -> EffectOp {
    p1a(P1aOpV1::MatronSearchV1)
}

/// Goblin Piledriver: "Whenever this creature attacks, it gets +2/+0 until
/// end of turn for each other attacking Goblin."
fn piledriver_attacks() -> EffectOp {
    p1a(P1aOpV1::PiledriverPumpV1)
}

/// Goblin Ringleader: "When this creature enters, reveal the top four cards
/// of your library. Put all Goblin cards revealed this way into your hand
/// and the rest on the bottom of your library in any order."
fn ringleader_enters() -> EffectOp {
    p1a(P1aOpV1::RingleaderV1)
}

/// Graveborn Muse: "At the beginning of your upkeep, you draw X cards and
/// you lose X life, where X is the number of Zombies you control."
fn graveborn_muse_upkeep() -> EffectOp {
    p1a(P1aOpV1::GravebornMuseV1)
}

/// Powder Keg: "At the beginning of your upkeep, you may put a fuse counter
/// on this artifact."
fn powder_keg_upkeep() -> EffectOp {
    EffectOp::Choice {
        controller: PlayerRef::Controller,
        options: vec![p1a(P1aOpV1::FuseCounterV1), p1a(P1aOpV1::NoEffectV1)],
    }
}

/// Rith's Grove: "When this land enters, sacrifice it unless you return a
/// non-Lair land you control to its owner's hand."
fn grove_enters() -> EffectOp {
    p1a(P1aOpV1::GroveV1)
}

/// Siege-Gang Commander: "When this creature enters, create three 1/1 red
/// Goblin creature tokens."
fn siege_gang_enters() -> EffectOp {
    p1a(P1aOpV1::SiegeGangTokensV1)
}

const GOBLIN_MATRON_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, matron_enters)];
const GOBLIN_PILEDRIVER_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Attacks, piledriver_attacks)];
const GOBLIN_RINGLEADER_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, ringleader_enters)];
const GRAVEBORN_MUSE_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(
    TriggerCondition::BeginningOfUpkeep {
        controller_only: true,
    },
    graveborn_muse_upkeep,
)];
const POWDER_KEG_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(
    TriggerCondition::BeginningOfUpkeep {
        controller_only: true,
    },
    powder_keg_upkeep,
)];
const RITHS_GROVE_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, grove_enters)];
const SIEGE_GANG_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, siege_gang_enters)];

/// Triggered abilities of this module's cards that use the engine's own
/// trigger conditions. The rest are queued by the hooks below.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        GOBLIN_MATRON => &GOBLIN_MATRON_TRIGGERS,
        GOBLIN_PILEDRIVER => &GOBLIN_PILEDRIVER_TRIGGERS,
        GOBLIN_RINGLEADER => &GOBLIN_RINGLEADER_TRIGGERS,
        GRAVEBORN_MUSE => &GRAVEBORN_MUSE_TRIGGERS,
        POWDER_KEG => &POWDER_KEG_TRIGGERS,
        RITHS_GROVE => &RITHS_GROVE_TRIGGERS,
        SIEGE_GANG_COMMANDER => &SIEGE_GANG_TRIGGERS,
        _ => &[],
    }
}

/// Target specs of this module's triggered abilities, per ability.
pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    if !triggers_for(name)
        .iter()
        .any(|trigger| (trigger.effect)() == *effect)
        && !delayed_effect_matches(name, effect)
    {
        return None;
    }
    Some(match name {
        GEMPALM_INCINERATOR if *effect == p1a(P1aOpV1::GempalmDamageV1) => TargetSpec::Creature,
        GOBLIN_VANDAL => vandal_target_spec(),
        _ => TargetSpec::None,
    })
}

/// "Target artifact defending player controls."
fn vandal_target_spec() -> TargetSpec {
    TargetSpec::StandardV1(StandardTargetV1::OpponentArtifactV1)
}

/// Goblin Vandal's trigger: "you may pay {R}. If you do, destroy target
/// artifact defending player controls and this creature assigns no combat
/// damage this turn."
fn vandal_trigger() -> EffectOp {
    EffectOp::MayPayManaThen {
        player: PlayerRef::Controller,
        colored: vec![ManaColor::R],
        generic: 0,
        then: Box::new(p1a(P1aOpV1::VandalDestroyV1)),
    }
}

/// Gempalm Incinerator's trigger: "you may have it deal X damage to target
/// creature".
fn gempalm_trigger() -> EffectOp {
    EffectOp::Choice {
        controller: PlayerRef::Controller,
        options: vec![p1a(P1aOpV1::GempalmDamageV1), p1a(P1aOpV1::NoEffectV1)],
    }
}

/// Queued and delayed triggers owned by this module's cards.
pub(crate) fn delayed_effect_matches(name: &str, effect: &EffectOp) -> bool {
    if name == GEMPALM_INCINERATOR && *effect == gempalm_trigger() {
        return true;
    }
    if name == GOBLIN_VANDAL && *effect == vandal_trigger() {
        return true;
    }
    if name == SYLVAN_LIBRARY && *effect == sylvan_trigger() {
        return true;
    }
    if name == PORTENT
        && *effect
            == EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::DelayedUpkeepDrawV1))
    {
        return true;
    }
    let EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::P1aV1(op))) = effect else {
        return false;
    };
    match name {
        ANURID_BRUSHHOPPER => matches!(op, P1aOpV1::BrushhopperReturnV1 { .. }),
        BALL_LIGHTNING => matches!(op, P1aOpV1::SacrificeSourceV1),
        GOBLIN_LACKEY => matches!(op, P1aOpV1::LackeyPutV1),
        GOBLIN_SHARPSHOOTER => matches!(op, P1aOpV1::UntapSourceV1),
        HYPNOTIC_SPECTER => matches!(op, P1aOpV1::RandomDiscardV1 { .. }),
        JACKAL_PUP => matches!(op, P1aOpV1::PupDamageV1 { .. }),
        SULFURIC_VORTEX => matches!(op, P1aOpV1::VortexDamageV1 { .. }),
        _ => false,
    }
}

/// Sylvan Library: "At the beginning of your draw step, you may draw two
/// additional cards. If you do, choose two cards in your hand drawn this
/// turn. For each of those cards, pay 4 life or put the card on top of your
/// library."
fn sylvan_trigger() -> EffectOp {
    EffectOp::Choice {
        controller: PlayerRef::Controller,
        options: vec![p1a(P1aOpV1::SylvanDrawV1), p1a(P1aOpV1::NoEffectV1)],
    }
}

// ---- Effect leaves ------------------------------------------------------------------

pub(crate) fn execute(op: &P1aOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        P1aOpV1::ArmageddonV1 => {
            let lands = battlefield(state)
                .filter(|&object| has_type(state, object, CardType::Land))
                .collect::<Vec<_>>();
            destroy_batch(state, lands);
        }
        P1aOpV1::DestroyTargetV1 => {
            if let Some(object) =
                legal_object_target(ctx, state, 0, TargetSpec::ArtifactOrEnchantmentPermanent)
            {
                destroy(ctx, state, object);
            }
        }
        P1aOpV1::FireV1 => fire_damage(ctx, state),
        P1aOpV1::IceV1 => {
            if let Some(object) = legal_object_target(ctx, state, 0, TargetSpec::AnyPermanent) {
                event::propose_and_commit(state, ProposedEvent::tap(object));
            }
            crate::effect::execute(
                &EffectOp::DrawCards {
                    player: PlayerRef::Controller,
                    count: 1,
                },
                ctx,
                state,
            );
        }
        P1aOpV1::SwampwalkV1 => {
            if let Some(creature) = legal_object_target(ctx, state, 0, TargetSpec::Creature) {
                let record = (live_link(state, creature), TurnV1::now(state));
                let records = &mut p1a_state(state).swampwalk;
                if !records.contains(&record) {
                    records.push(record);
                }
            }
        }
        P1aOpV1::IncinerateV1 => {
            let Some(target) = legal_any_target(ctx, state, TargetSpec::AnyTarget) else {
                return;
            };
            let before = state.engine.event_history.len();
            event::propose_and_commit(state, event::damage_from_exec_ctx_v1(ctx, target, 3));
            if let Target::Object(creature) = target {
                let dealt = state.engine.event_history.iter().skip(before).any(|event| {
                    matches!(
                        event,
                        CommittedEvent::Damage { source, target: Target::Object(dealt), amount }
                            if *source == ctx.source && *dealt == creature && *amount > 0
                    )
                });
                if dealt
                    && state.objects.get(creature).zone == Zone::Battlefield
                    && has_type(state, creature, CardType::Creature)
                {
                    forbid_regeneration(state, creature);
                }
            }
        }
        P1aOpV1::ShuffleTargetV1 => {
            if let Some(player) = legal_player_target(ctx, state, 0, TargetSpec::AnyPlayer) {
                if shuffle(state, player).is_err() {
                    halt(state, ctx.source);
                }
            }
        }
        P1aOpV1::PortentDrawV1 => {
            let source = AbilitySourceContractV4::capture(state, ctx.source);
            state
                .eternal_v1
                .get_or_insert_with(Default::default)
                .upkeep_draws
                .push(crate::eternal_cards_v1::DelayedUpkeepDrawV1 {
                    controller: ctx.controller,
                    source,
                });
        }
        P1aOpV1::PriceOfProgressV1 => {
            let damage = [PlayerId::P0, PlayerId::P1]
                .into_iter()
                .filter_map(|player| {
                    let nonbasic = battlefield(state)
                        .filter(|&object| {
                            state.objects.get(object).controller == player
                                && has_type(state, object, CardType::Land)
                                && !crate::engine::effective_supertypes(state, object)
                                    .contains(&crate::card_def::Supertype::Basic)
                        })
                        .count();
                    let amount = i32::try_from(nonbasic.saturating_mul(2)).unwrap_or(i32::MAX);
                    (amount > 0).then(|| {
                        event::damage_from_exec_ctx_v1(ctx, Target::Player(player), amount)
                    })
                })
                .collect::<Vec<_>>();
            if !damage.is_empty() {
                event::propose_and_commit_batch(state, damage);
            }
        }
        P1aOpV1::SkeletalScryingV1 => {
            let x = ctx.x_value;
            crate::effect::execute(
                &EffectOp::DrawCards {
                    player: PlayerRef::Controller,
                    count: u32::from(x),
                },
                ctx,
                state,
            );
            if x > 0 {
                crate::effect::execute(
                    &EffectOp::LoseLife {
                        player: PlayerRef::Controller,
                        amount: i32::from(x),
                    },
                    ctx,
                    state,
                );
            }
        }
        P1aOpV1::SmotherV1 => {
            let spec = TargetSpec::StandardV1(StandardTargetV1::CreatureManaValueAtMostV1(3));
            let Some(creature) = legal_object_target(ctx, state, 0, spec) else {
                return;
            };
            let record = (live_link(state, creature), TurnV1::now(state));
            let added = {
                let records = &mut p1a_state(state).no_regeneration;
                let added = !records.contains(&record);
                if added {
                    records.push(record);
                }
                added
            };
            destroy(ctx, state, creature);
            if added {
                p1a_state(state)
                    .no_regeneration
                    .retain(|entry| *entry != record);
            }
        }
        P1aOpV1::VisionLandsV1 { from, to } => {
            let lands = battlefield(state)
                .filter(|&object| {
                    has_type(state, object, CardType::Land)
                        && crate::engine::has_effective_subtype(state, object, *from)
                })
                .collect::<Vec<_>>();
            if lands.is_empty() {
                return;
            }
            let timestamp = crate::engine::next_timestamp(state);
            let turn = TurnV1::now(state);
            for land in lands {
                let link = live_link(state, land);
                p1a_state(state).land_types.push(LandTypeChangeV1 {
                    land: link,
                    to: *to,
                    timestamp,
                    turn,
                });
            }
        }
        P1aOpV1::PhaseOutV1 => {
            if let Some(artifact) =
                legal_object_target(ctx, state, 0, TargetSpec::ArtifactPermanent)
            {
                crate::eternal_e5a_v1::phasing::phase_out(state, artifact);
            }
        }
        P1aOpV1::BrushhopperExileV1 => {
            let Some(source) = live_ability_source(ctx, state) else {
                return;
            };
            let Some(contract) = ctx.ability_source_contract else {
                return;
            };
            event::propose_and_commit(state, ProposedEvent::zone_change(source, Zone::Exile));
            let live = state.objects.get(source);
            if live.zone != Zone::Exile || live.v4.is_token {
                return;
            }
            let card = live_link(state, source);
            let created_in_end_step =
                (state.step == crate::state::Step::End).then(|| TurnV1::now(state));
            p1a_state(state).end_step_returns.push(EndStepReturnV1 {
                source: contract,
                controller: ctx.controller,
                card,
                created_in_end_step,
            });
        }
        P1aOpV1::BrushhopperReturnV1 { card } => {
            if link_live_in(state, *card, Zone::Exile) {
                let owner = state.objects.get(card.object).owner;
                event::propose_and_commit(
                    state,
                    ProposedEvent::zone_change_to_battlefield_under_controller(card.object, owner),
                );
            }
        }
        P1aOpV1::CursedScrollV1 { named } => cursed_scroll_reveal(ctx, state, *named),
        P1aOpV1::SourceFirstStrikeV1 => {
            if live_ability_source(ctx, state).is_some() {
                crate::effect::execute(
                    &EffectOp::GrantKeywordTargetUntilEndOfTurn {
                        object: ObjectRef::ThisSource,
                        keyword: Keywords::FIRST_STRIKE,
                    },
                    ctx,
                    state,
                );
            }
        }
        P1aOpV1::PowderKegV1 => {
            let Some(contract) = ctx.ability_source_contract else {
                return;
            };
            let link = ObjectLinkV4 {
                object: contract.source,
                zone_change_count: contract.zone_change_count,
            };
            let fuse = read_state(state).map_or(0, |p1a| {
                p1a.fuse
                    .iter()
                    .find(|(keg, _)| *keg == link)
                    .map_or(0, |(_, count)| *count)
            });
            let doomed = battlefield(state)
                .filter(|&object| {
                    (has_type(state, object, CardType::Artifact)
                        || has_type(state, object, CardType::Creature))
                        && crate::engine::object_mana_value(state, object) == fuse
                })
                .collect::<Vec<_>>();
            destroy_batch(state, doomed);
        }
        P1aOpV1::FuseCounterV1 => {
            if let Some(keg) = live_ability_source(ctx, state) {
                let link = live_link(state, keg);
                let counters = &mut p1a_state(state).fuse;
                match counters.iter_mut().find(|(host, _)| *host == link) {
                    Some(entry) => entry.1 = entry.1.saturating_add(1),
                    None => counters.push((link, 1)),
                }
            }
        }
        P1aOpV1::MongrelPumpV1 => {
            if let Some(mongrel) = live_ability_source(ctx, state) {
                pump_until_end_of_turn(state, mongrel, 1, 1);
            }
        }
        P1aOpV1::MongrelColorV1 { color } => {
            if let Some(mongrel) = live_ability_source(ctx, state) {
                let timestamp = crate::engine::next_timestamp(state);
                let record = ColorChangeV1 {
                    object: live_link(state, mongrel),
                    color: *color,
                    timestamp,
                    turn: TurnV1::now(state),
                };
                p1a_state(state).colors.push(record);
            }
        }
        P1aOpV1::SacrificeSourceV1 => {
            if let Some(source) = live_ability_source(ctx, state) {
                if state.objects.get(source).controller == ctx.controller {
                    event::log_sacrifice(state, source);
                    event::propose_and_commit(
                        state,
                        ProposedEvent::zone_change(source, Zone::Graveyard),
                    );
                }
            }
        }
        P1aOpV1::GempalmDamageV1 => {
            let Some(creature) = legal_object_target(ctx, state, 0, TargetSpec::Creature) else {
                return;
            };
            let amount = i32::try_from(goblins_on_battlefield(state)).unwrap_or(i32::MAX);
            if amount > 0 {
                event::propose_and_commit(
                    state,
                    event::damage_from_exec_ctx_v1(ctx, Target::Object(creature), amount),
                );
            }
        }
        P1aOpV1::PiledriverPumpV1 => {
            let Some(source) = live_ability_source(ctx, state) else {
                return;
            };
            let others = other_attacking_goblins(state, source);
            if others > 0 {
                let power = i32::try_from(others.saturating_mul(2)).unwrap_or(i32::MAX);
                pump_until_end_of_turn(state, source, power, 0);
            }
        }
        P1aOpV1::UntapSourceV1 => {
            if let Some(source) = live_ability_source(ctx, state) {
                crate::engine::attempt_untap(state, source);
            }
        }
        P1aOpV1::VandalDestroyV1 => {
            if let Some(artifact) = legal_object_target(ctx, state, 0, vandal_target_spec()) {
                destroy(ctx, state, artifact);
            }
            if let Some(vandal) = live_ability_source(ctx, state) {
                let record = (live_link(state, vandal), TurnV1::now(state));
                let records = &mut p1a_state(state).no_combat_damage;
                if !records.contains(&record) {
                    records.push(record);
                }
            }
        }
        P1aOpV1::GravebornMuseV1 => {
            let zombies = battlefield(state)
                .filter(|&object| {
                    state.objects.get(object).controller == ctx.controller
                        && crate::engine::has_effective_subtype(state, object, Subtype::Zombie)
                })
                .count();
            if zombies == 0 {
                return;
            }
            crate::effect::execute(
                &EffectOp::DrawCards {
                    player: PlayerRef::Controller,
                    count: u32::try_from(zombies).unwrap_or(u32::MAX),
                },
                ctx,
                state,
            );
            crate::effect::execute(
                &EffectOp::LoseLife {
                    player: PlayerRef::Controller,
                    amount: i32::try_from(zombies).unwrap_or(i32::MAX),
                },
                ctx,
                state,
            );
        }
        P1aOpV1::RandomDiscardV1 { player } => match state.random_hand_card_v1(*player) {
            Ok(Some(card)) => crate::engine::commit_discarded_card(state, card),
            Ok(None) => {}
            Err(_) => halt(state, ctx.source),
        },
        P1aOpV1::PupDamageV1 { amount } => {
            if *amount > 0 {
                event::propose_and_commit(
                    state,
                    event::damage_from_exec_ctx_v1(ctx, Target::Player(ctx.controller), *amount),
                );
            }
        }
        P1aOpV1::SiegeGangTokensV1 => {
            let token = token_id(GOBLIN_TOKEN);
            for _ in 0..3 {
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
        P1aOpV1::VortexDamageV1 { player } => {
            event::propose_and_commit(
                state,
                event::damage_from_exec_ctx_v1(ctx, Target::Player(*player), 2),
            );
        }
        P1aOpV1::NoEffectV1 => {}
        // Choices run in `drive`; the plain executor never reaches them.
        P1aOpV1::FlashOfInsightV1
        | P1aOpV1::PortentV1
        | P1aOpV1::LackeyPutV1
        | P1aOpV1::MatronSearchV1
        | P1aOpV1::RingleaderV1
        | P1aOpV1::GroveV1
        | P1aOpV1::SylvanLibraryV1
        | P1aOpV1::SylvanDrawV1 => halt(state, ctx.source),
    }
}

/// Fire's divided damage: each target still legal on resolution is dealt
/// its share, as one simultaneous batch; an illegal target's share is not
/// dealt (608.2b).
fn fire_damage(ctx: &ExecCtx, state: &mut GameState) {
    let filter = StandardTargetV1::DividedTwoDamageAnyTargetsV1;
    let spec = TargetSpec::StandardV1(filter);
    let damage = crate::eternal_legacy_e9a_v1::targets::division(filter, &ctx.targets, 2)
        .into_iter()
        .filter(|&(target, amount)| {
            amount > 0
                && ctx.targets.iter().enumerate().any(|(index, &slot)| {
                    slot == target
                        && ctx.target_incarnation_matches(index, state)
                        && crate::engine::effect_target_is_legal(
                            state,
                            ctx.source,
                            ctx.controller,
                            spec,
                            &ctx.targets,
                            index,
                        )
                })
        })
        .map(|(target, amount)| event::damage_from_exec_ctx_v1(ctx, target, amount))
        .collect::<Vec<_>>();
    if !damage.is_empty() {
        event::propose_and_commit_batch(state, damage);
    }
}

/// Cursed Scroll's reveal: a card at random from its controller's hand is
/// revealed; if it has the named card's name, 2 damage to target 0.
fn cursed_scroll_reveal(ctx: &ExecCtx, state: &mut GameState, named: u16) {
    let player = ctx.controller;
    let card = match state.random_hand_card_v1(player) {
        Ok(Some(card)) => card,
        Ok(None) => return,
        Err(_) => {
            halt(state, ctx.source);
            return;
        }
    };
    if state
        .reveal_hand_card(player.opponent(), player, card)
        .is_err()
    {
        halt(state, ctx.source);
        return;
    }
    let named_name = CARD_DEFS.get(usize::from(named)).map(|def| def.object_name);
    let revealed_name = definition(state, card).map(|def| def.object_name);
    if named_name.is_none() || named_name != revealed_name {
        return;
    }
    if let Some(target) = legal_any_target(ctx, state, TargetSpec::AnyTarget) {
        event::propose_and_commit(state, event::damage_from_exec_ctx_v1(ctx, target, 2));
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
#[allow(clippy::too_many_arguments)]
fn offer(
    continuation: &mut EffectContinuation,
    kind: P1aChoiceV1,
    player: PlayerId,
    bindings: Vec<EffectObjectBinding>,
    min: u16,
    max: u16,
    ordered: bool,
    path: Vec<u16>,
) {
    let remaining = continuation.frames.clone();
    crate::eternal_cards_v1::e3::offer(
        continuation,
        E3ChoiceV1 {
            kind: E3ChoiceKindV1::P1aV1(kind),
            player,
            candidates: targets_of(&bindings),
            bindings,
            min,
            max,
            ordered,
            path,
            remaining,
        },
    );
}

/// The cards in `player`'s hand that they drew this turn: the turn begins
/// at its `UpkeepBegan` marker (nothing is drawn in an untap step), and a
/// card counts when its last move this turn was that draw.
fn drawn_this_turn(state: &GameState, player: PlayerId) -> Vec<ObjectId> {
    let history = &state.engine.event_history;
    let start = history
        .iter()
        .rposition(|event| matches!(event, CommittedEvent::UpkeepBegan { .. }))
        .map_or(0, |index| index + 1);
    let mut drawn: Vec<ObjectId> = Vec::new();
    for event in history.iter().skip(start) {
        match event {
            CommittedEvent::Draw {
                player: drawer,
                object: Some(card),
            } if *drawer == player => {
                if !drawn.contains(card) {
                    drawn.push(*card);
                }
            }
            CommittedEvent::ZoneChange { object, .. } => drawn.retain(|card| card != object),
            _ => {}
        }
    }
    drawn
        .into_iter()
        .filter(|&card| {
            state.players[player.index()].hand.contains(&card)
                && state.objects.get(card).zone == Zone::Hand
        })
        .collect()
}

/// Puts `cards` on the bottom of their owner's library in order, the first
/// highest.
fn put_on_bottom(state: &mut GameState, cards: &[ObjectId]) {
    for &card in cards {
        event::propose_and_commit(
            state,
            ProposedEvent::public_library_insert(card, LibraryPlacement::Bottom),
        );
    }
}

/// Runs a leaf inside the resumable interpreter. `Ok(true)` when it staged a
/// choice and the continuation must suspend.
pub(crate) fn drive(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    op: &P1aOpV1,
    path: Vec<u16>,
) -> Result<bool, String> {
    let controller = continuation.ctx.controller;
    match op {
        P1aOpV1::FlashOfInsightV1 => {
            let count = u8::try_from(continuation.ctx.x_value).unwrap_or(u8::MAX);
            if count == 0 || state.players[controller.index()].library.is_empty() {
                return Ok(false);
            }
            continuation
                .frames
                .push(crate::effect::EffectFrame::Program {
                    op: EffectOp::LookTopPickToHandBottomRest {
                        player: PlayerRef::Controller,
                        count: crate::effect::LibraryLookCount::Fixed(count),
                        pick: 1,
                        choose_rest_order: true,
                    },
                    path,
                });
            Ok(false)
        }
        P1aOpV1::PortentV1 => {
            let ctx = continuation.ctx.clone();
            let Some(player) = legal_player_target(&ctx, state, 0, TargetSpec::AnyPlayer) else {
                return Ok(false);
            };
            let top = state.players[player.index()]
                .library
                .iter()
                .copied()
                .take(3)
                .map(|card| bind(state, card))
                .collect::<Vec<_>>();
            if top.len() < 2 {
                // One card or none: looking at it changes no order, and the
                // controller learns it.
                if let Some(card) = top.first() {
                    let ordered = [card.object];
                    state.reorder_library_top(player, &ordered, &[controller, player])?;
                }
                return Ok(false);
            }
            let count = u16::try_from(top.len()).unwrap_or(u16::MAX);
            offer(
                continuation,
                P1aChoiceV1::PortentOrderV1 { player },
                controller,
                top,
                count,
                count,
                true,
                path,
            );
            Ok(true)
        }
        P1aOpV1::LackeyPutV1 => {
            let cards = hand_cards(state, controller)
                .into_iter()
                .filter(|&card| {
                    definition(state, card).is_some_and(|def| {
                        def.subtypes.contains(&Subtype::Goblin)
                            && [
                                CardType::Artifact,
                                CardType::Creature,
                                CardType::Enchantment,
                                CardType::Land,
                                CardType::Planeswalker,
                            ]
                            .into_iter()
                            .any(|card_type| def.has_type(card_type))
                    })
                })
                .map(|card| bind(state, card))
                .collect::<Vec<_>>();
            if cards.is_empty() {
                return Ok(false);
            }
            offer(
                continuation,
                P1aChoiceV1::LackeyPutV1,
                controller,
                cards,
                0,
                1,
                false,
                path,
            );
            Ok(true)
        }
        P1aOpV1::MatronSearchV1 => {
            continuation
                .frames
                .push(crate::effect::EffectFrame::Program {
                    op: EffectOp::SearchLibraryToHand {
                        player: PlayerRef::Controller,
                        filter: crate::effect::LibraryCardFilter::CardWithSubtypeV1(
                            Subtype::Goblin,
                        ),
                    },
                    path,
                });
            Ok(false)
        }
        P1aOpV1::RingleaderV1 => {
            let top = state.players[controller.index()]
                .library
                .iter()
                .copied()
                .take(4)
                .collect::<Vec<_>>();
            if top.is_empty() {
                return Ok(false);
            }
            for observer in [PlayerId::P0, PlayerId::P1] {
                state.reveal_library_top(observer, controller, top.len());
            }
            let goblins = top
                .iter()
                .copied()
                .filter(|&card| {
                    definition(state, card)
                        .is_some_and(|def| def.subtypes.contains(&Subtype::Goblin))
                })
                .collect::<Vec<_>>();
            if !goblins.is_empty() {
                event::propose_and_commit_batch(
                    state,
                    goblins
                        .iter()
                        .map(|&card| ProposedEvent::zone_change(card, Zone::Hand))
                        .collect(),
                );
            }
            let rest = top
                .into_iter()
                .filter(|&card| state.objects.get(card).zone == Zone::Library)
                .collect::<Vec<_>>();
            if rest.len() < 2 {
                put_on_bottom(state, &rest);
                return Ok(false);
            }
            let bindings = rest.iter().map(|&card| bind(state, card)).collect();
            let count = u16::try_from(rest.len()).unwrap_or(u16::MAX);
            offer(
                continuation,
                P1aChoiceV1::RingleaderBottomV1,
                controller,
                bindings,
                count,
                count,
                true,
                path,
            );
            Ok(true)
        }
        P1aOpV1::GroveV1 => {
            let source = continuation
                .ctx
                .ability_source_contract
                .map(|contract| contract.source);
            let lands = battlefield(state)
                .filter(|&object| {
                    state.objects.get(object).controller == controller
                        && Some(object) != source
                        && has_type(state, object, CardType::Land)
                        && !crate::engine::has_effective_subtype(state, object, Subtype::Lair)
                })
                .map(|object| bind(state, object))
                .collect::<Vec<_>>();
            if lands.is_empty() {
                grove_sacrifice(state, &continuation.ctx);
                return Ok(false);
            }
            offer(
                continuation,
                P1aChoiceV1::GroveReturnV1,
                controller,
                lands,
                0,
                1,
                false,
                path,
            );
            Ok(true)
        }
        P1aOpV1::SylvanDrawV1 => {
            continuation
                .frames
                .push(crate::effect::EffectFrame::Program {
                    op: p1a(P1aOpV1::SylvanLibraryV1),
                    path: path.clone(),
                });
            continuation
                .frames
                .push(crate::effect::EffectFrame::Program {
                    op: EffectOp::DrawCards {
                        player: PlayerRef::Controller,
                        count: 2,
                    },
                    path,
                });
            Ok(false)
        }
        P1aOpV1::SylvanLibraryV1 => {
            let drawn = drawn_this_turn(state, controller);
            if drawn.is_empty() {
                return Ok(false);
            }
            let count = u16::try_from(drawn.len().min(2)).unwrap_or(2);
            let bindings = drawn.iter().map(|&card| bind(state, card)).collect();
            offer(
                continuation,
                P1aChoiceV1::SylvanChooseV1,
                controller,
                bindings,
                count,
                count,
                true,
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

/// Rith's Grove: "sacrifice it".
fn grove_sacrifice(state: &mut GameState, ctx: &ExecCtx) {
    let Some(grove) = live_ability_source(ctx, state) else {
        return;
    };
    if state.objects.get(grove).controller != ctx.controller {
        return;
    }
    event::log_sacrifice(state, grove);
    event::propose_and_commit(state, ProposedEvent::zone_change(grove, Zone::Graveyard));
}

/// Applies an answered choice of this module.
pub(crate) fn finish_selection(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    choice: &E3ChoiceV1,
    kind: &P1aChoiceV1,
    bindings: Vec<EffectObjectBinding>,
) -> Result<bool, String> {
    let player = choice.player;
    match kind {
        P1aChoiceV1::PortentOrderV1 { player: owner } => {
            let ordered = bindings
                .iter()
                .map(|binding| binding.object)
                .collect::<Vec<_>>();
            let library = &state.players[owner.index()].library;
            if library.len() < ordered.len()
                || library[..ordered.len()]
                    .iter()
                    .any(|card| !ordered.contains(card))
            {
                return Err("Portent's cards left the top of the library".to_string());
            }
            state.reorder_library_top(*owner, &ordered, &[player])?;
            Ok(false)
        }
        P1aChoiceV1::LackeyPutV1 => {
            if let Some(card) = bindings.first() {
                if state.objects.get(card.object).zone == Zone::Hand {
                    event::propose_and_commit(
                        state,
                        ProposedEvent::zone_change_to_battlefield_under_controller(
                            card.object,
                            player,
                        ),
                    );
                }
            }
            Ok(false)
        }
        P1aChoiceV1::RingleaderBottomV1 => {
            let cards = bindings
                .iter()
                .map(|binding| binding.object)
                .filter(|&card| state.objects.get(card).zone == Zone::Library)
                .collect::<Vec<_>>();
            put_on_bottom(state, &cards);
            Ok(false)
        }
        P1aChoiceV1::GroveReturnV1 => {
            match bindings.first() {
                Some(land) if state.objects.get(land.object).zone == Zone::Battlefield => {
                    event::propose_and_commit(
                        state,
                        ProposedEvent::zone_change(land.object, Zone::Hand),
                    );
                }
                _ => {
                    let ctx = continuation.ctx.clone();
                    grove_sacrifice(state, &ctx);
                }
            }
            Ok(false)
        }
        P1aChoiceV1::SylvanChooseV1 => {
            let life = state.players[player.index()].life;
            let affordable = usize::try_from((life / 4).max(0)).unwrap_or(0);
            let keep = bindings.len().min(affordable);
            if keep == 0 {
                sylvan_finish(state, player, &bindings, &[]);
                return Ok(false);
            }
            let keep = u16::try_from(keep).unwrap_or(u16::MAX);
            let path = choice.path.clone();
            offer(
                continuation,
                P1aChoiceV1::SylvanKeepV1 {
                    chosen: bindings.clone(),
                },
                player,
                bindings,
                0,
                keep,
                false,
                path,
            );
            Ok(true)
        }
        P1aChoiceV1::SylvanKeepV1 { chosen } => {
            let kept = bindings
                .iter()
                .map(|binding| binding.object)
                .collect::<Vec<_>>();
            sylvan_finish(state, player, chosen, &kept);
            Ok(false)
        }
    }
}

/// Sylvan Library's payment: 4 life for each kept card, the others go on
/// top of the library in the order they were chosen (the last on top).
fn sylvan_finish(
    state: &mut GameState,
    player: PlayerId,
    chosen: &[EffectObjectBinding],
    kept: &[ObjectId],
) {
    for card in chosen {
        if state.objects.get(card.object).zone != Zone::Hand
            || state.objects.get(card.object).zone_change_count != card.expected_zone_change_count
        {
            continue;
        }
        if kept.contains(&card.object) {
            event::propose_and_commit(state, ProposedEvent::life_loss(player, 4));
        } else {
            event::propose_and_commit(
                state,
                ProposedEvent::private_top_library_insert(card.object),
            );
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
    note_cycles(state);
    note_damage(state, events);
    note_deaths(state, events);
    note_end_step(state, events);
    note_upkeep(state, events);
    prune(state);
    let Some(eternal) = state.eternal_v1.as_mut() else {
        return Vec::new();
    };
    if eternal.p1a.queued_triggers.is_empty() {
        return Vec::new();
    }
    std::mem::take(&mut eternal.p1a.queued_triggers)
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

/// Gempalm Incinerator: "When you cycle this card". Called as a cycling
/// cost discards `card`.
pub(crate) fn note_cycled(state: &mut GameState, card: ObjectId) {
    if definition_name(state, card) != Some(GEMPALM_INCINERATOR) {
        return;
    }
    let owner = state.objects.get(card).owner;
    p1a_state(state).cycled.push((card, owner));
}

fn note_cycles(state: &mut GameState) {
    let Some(p1a) = read_state(state) else {
        return;
    };
    if p1a.cycled.is_empty() {
        return;
    }
    let cycled = std::mem::take(&mut p1a_state(state).cycled);
    for (card, owner) in cycled {
        let source = AbilitySourceContractV4::capture(state, card);
        queue_trigger(
            state,
            source,
            owner,
            gempalm_trigger(),
            Some(TargetSpec::Creature),
        );
    }
}

/// Damage: Goblin Lackey ("Whenever this creature deals damage to a
/// player"), Hypnotic Specter ("Whenever this creature deals damage to an
/// opponent, that player discards a card at random.") and Jackal Pup
/// ("Whenever this creature is dealt damage, it deals that much damage to
/// you.").
fn note_damage(state: &mut GameState, events: &[CommittedEvent]) {
    for event in events {
        let CommittedEvent::Damage {
            source,
            target,
            amount,
        } = *event
        else {
            continue;
        };
        if amount <= 0 {
            continue;
        }
        match target {
            Target::Player(player) => {
                let Some(name) = definition_name(state, source) else {
                    continue;
                };
                if !printed_source(state, source, name) {
                    continue;
                }
                let controller = state.objects.get(source).controller;
                if name == GOBLIN_LACKEY {
                    let contract = AbilitySourceContractV4::capture(state, source);
                    queue_trigger(state, contract, controller, p1a(P1aOpV1::LackeyPutV1), None);
                } else if name == HYPNOTIC_SPECTER && player != controller {
                    let contract = AbilitySourceContractV4::capture(state, source);
                    queue_trigger(
                        state,
                        contract,
                        controller,
                        p1a(P1aOpV1::RandomDiscardV1 { player }),
                        None,
                    );
                }
            }
            Target::Object(object) => {
                if definition_name(state, object) != Some(JACKAL_PUP)
                    || !printed_source(state, object, JACKAL_PUP)
                {
                    continue;
                }
                let controller = state.objects.get(object).controller;
                let contract = AbilitySourceContractV4::capture(state, object);
                queue_trigger(
                    state,
                    contract,
                    controller,
                    p1a(P1aOpV1::PupDamageV1 { amount }),
                    None,
                );
            }
            Target::StackItem(_) => {}
        }
    }
}

/// Whether `object` is a face-up `name` whose printed abilities applied: on
/// the battlefield now, or a card that left it in the same event batch (a
/// creature dealt lethal damage still triggers; 603.10a).
fn printed_source(state: &GameState, object: ObjectId, name: &str) -> bool {
    let Some(live) = state.objects.try_get(object) else {
        return false;
    };
    if live.zone == Zone::Battlefield {
        return active_named(state, object, name);
    }
    live.v4.face_down_v1.is_none()
        && !live.v4.is_token
        && definition_name(state, object) == Some(name)
}

/// Goblin Sharpshooter: "Whenever a creature dies, untap this creature."
fn note_deaths(state: &mut GameState, events: &[CommittedEvent]) {
    let died = events
        .iter()
        .filter(|event| {
            let CommittedEvent::ZoneChange {
                object,
                from: Zone::Battlefield,
                to: Zone::Graveyard,
                ..
            } = **event
            else {
                return false;
            };
            state.objects.try_get(object).is_some_and(|live| {
                left_as_creature(state, object, live.zone_change_count.saturating_sub(1))
            })
        })
        .count();
    if died == 0 {
        return;
    }
    let sharpshooters = active_permanents(state, GOBLIN_SHARPSHOOTER, None).collect::<Vec<_>>();
    for _ in 0..died {
        for &sharpshooter in &sharpshooters {
            let source = AbilitySourceContractV4::capture(state, sharpshooter);
            let controller = state.objects.get(sharpshooter).controller;
            queue_trigger(state, source, controller, p1a(P1aOpV1::UntapSourceV1), None);
        }
    }
}

/// The beginning of the end step: Ball Lightning ("At the beginning of the
/// end step, sacrifice this creature.") and Anurid Brushhopper's return.
fn note_end_step(state: &mut GameState, events: &[CommittedEvent]) {
    if !events
        .iter()
        .any(|event| matches!(event, CommittedEvent::BeginningEndStep { .. }))
    {
        return;
    }
    let lightning = active_permanents(state, BALL_LIGHTNING, None).collect::<Vec<_>>();
    for ball in lightning {
        let source = AbilitySourceContractV4::capture(state, ball);
        let controller = state.objects.get(ball).controller;
        queue_trigger(
            state,
            source,
            controller,
            p1a(P1aOpV1::SacrificeSourceV1),
            None,
        );
    }
    let Some(records) = read_state(state) else {
        return;
    };
    if records.end_step_returns.is_empty() {
        return;
    }
    let now = TurnV1::now(state);
    let (returns, waiting): (Vec<_>, Vec<_>) = records
        .end_step_returns
        .iter()
        .copied()
        .partition(|pending| pending.created_in_end_step != Some(now));
    p1a_state(state).end_step_returns = waiting;
    for pending in returns {
        queue_trigger(
            state,
            pending.source,
            pending.controller,
            p1a(P1aOpV1::BrushhopperReturnV1 { card: pending.card }),
            None,
        );
    }
}

/// Sulfuric Vortex: "At the beginning of each player's upkeep, this
/// enchantment deals 2 damage to that player."
fn note_upkeep(state: &mut GameState, events: &[CommittedEvent]) {
    for event in events {
        let CommittedEvent::UpkeepBegan { player } = *event else {
            continue;
        };
        let vortexes = active_permanents(state, SULFURIC_VORTEX, None).collect::<Vec<_>>();
        for vortex in vortexes {
            let source = AbilitySourceContractV4::capture(state, vortex);
            let controller = state.objects.get(vortex).controller;
            queue_trigger(
                state,
                source,
                controller,
                p1a(P1aOpV1::VortexDamageV1 { player }),
                None,
            );
        }
    }
}

/// Sylvan Library's trigger, queued as `player`'s draw step begins (it is
/// put on the stack after the draw).
pub(crate) fn begin_draw_step(state: &mut GameState, player: PlayerId) {
    if !cfg!(feature = "eternal-fixtures") {
        return;
    }
    let libraries = active_permanents(state, SYLVAN_LIBRARY, Some(player)).collect::<Vec<_>>();
    for library in libraries {
        let source = AbilitySourceContractV4::capture(state, library);
        queue_trigger(state, source, player, sylvan_trigger(), None);
    }
}

/// Goblin Vandal: "Whenever this creature attacks and isn't blocked".
/// Called as blockers are declared.
pub(crate) fn note_blockers_declared(state: &mut GameState) {
    if !cfg!(feature = "eternal-fixtures") {
        return;
    }
    let unblocked = state
        .engine
        .combat
        .attackers
        .iter()
        .copied()
        .filter(|&attacker| {
            active_named(state, attacker, GOBLIN_VANDAL)
                && !state
                    .engine
                    .combat
                    .blocked_by
                    .iter()
                    .any(|(blocked, blockers)| *blocked == attacker && !blockers.is_empty())
        })
        .collect::<Vec<_>>();
    for vandal in unblocked {
        let source = AbilitySourceContractV4::capture(state, vandal);
        let controller = state.objects.get(vandal).controller;
        queue_trigger(
            state,
            source,
            controller,
            vandal_trigger(),
            Some(vandal_target_spec()),
        );
    }
}

/// Drops records whose objects moved on or whose turn ended.
fn prune(state: &mut GameState) {
    let Some(p1a) = read_state(state) else {
        return;
    };
    if p1a.is_empty() {
        return;
    }
    let now = TurnV1::now(state);
    let on_battlefield = |link: &ObjectLinkV4| link_live_in(state, *link, Zone::Battlefield);
    let mining = p1a
        .mining
        .iter()
        .copied()
        .filter(|(host, _)| on_battlefield(host))
        .collect::<Vec<_>>();
    // A Powder Keg's counters stay while an ability of that incarnation is
    // on the stack.
    let fuse = p1a
        .fuse
        .iter()
        .copied()
        .filter(|(host, _)| {
            on_battlefield(host)
                || state.stack.iter().any(|item| {
                    item.v4.ability_source_contract.is_some_and(|contract| {
                        contract.source == host.object
                            && contract.zone_change_count == host.zone_change_count
                    })
                })
        })
        .collect::<Vec<_>>();
    let no_regeneration = p1a
        .no_regeneration
        .iter()
        .copied()
        .filter(|(host, turn)| *turn == now && on_battlefield(host))
        .collect::<Vec<_>>();
    let swampwalk = p1a
        .swampwalk
        .iter()
        .copied()
        .filter(|(host, turn)| *turn == now && on_battlefield(host))
        .collect::<Vec<_>>();
    let colors = p1a
        .colors
        .iter()
        .copied()
        .filter(|record| record.turn == now && on_battlefield(&record.object))
        .collect::<Vec<_>>();
    let land_types = p1a
        .land_types
        .iter()
        .copied()
        .filter(|record| record.turn == now && on_battlefield(&record.land))
        .collect::<Vec<_>>();
    let no_combat_damage = p1a
        .no_combat_damage
        .iter()
        .copied()
        .filter(|(host, turn)| *turn == now && on_battlefield(host))
        .collect::<Vec<_>>();
    let end_step_returns = p1a
        .end_step_returns
        .iter()
        .copied()
        .filter(|pending| link_live_in(state, pending.card, Zone::Exile))
        .collect::<Vec<_>>();
    let changed = mining.len() != p1a.mining.len()
        || fuse.len() != p1a.fuse.len()
        || no_regeneration.len() != p1a.no_regeneration.len()
        || swampwalk.len() != p1a.swampwalk.len()
        || colors.len() != p1a.colors.len()
        || land_types.len() != p1a.land_types.len()
        || no_combat_damage.len() != p1a.no_combat_damage.len()
        || end_step_returns.len() != p1a.end_step_returns.len();
    if changed {
        let p1a = p1a_state(state);
        p1a.mining = mining;
        p1a.fuse = fuse;
        p1a.no_regeneration = no_regeneration;
        p1a.swampwalk = swampwalk;
        p1a.colors = colors;
        p1a.land_types = land_types;
        p1a.no_combat_damage = no_combat_damage;
        p1a.end_step_returns = end_step_returns;
    }
}

/// The beginning of a turn ends the last turn's records.
pub(crate) fn begin_turn(state: &mut GameState, _player: PlayerId) {
    prune(state);
}
