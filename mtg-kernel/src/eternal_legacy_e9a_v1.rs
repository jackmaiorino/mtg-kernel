//! Card programs for the eternal catalog's wave E9a
//! (`docs/design/eternal_formats_support_v1.md`), the Legacy long tail:
//! Selesnya Depths, Red Stompy, LED Dredge, Painter and Cephalid Breakfast,
//! and the Initiative, Mono-Black and Death and Taxes cards of the decks
//! that also need wave E9b.
//!
//! `eternal_cards_v1` forwards its effect leaves, trigger conditions and
//! state here, and the engine calls the hooks below only in
//! `eternal-fixtures` builds. Every table is keyed by a printed name that
//! occurs only in the eternal registry, so the Pauper, FDN and Standard
//! catalogs never reach it. Behavior was read from each card's current
//! Oracle text and, where present, its XMage source.
//!
//! Primitives added here:
//! - divided damage announced with the targets, one point per pick
//!   (`targets`: Fury, Shatterskull Smashing);
//! - Blood Moon and Magus of the Moon, through wave E8a's land-type change;
//! - Painter's Servant's added color for every card, spell and permanent,
//!   read wherever the engine reads colors;
//! - split second (Legolas's Quick Reflexes) and a granted "becomes tapped"
//!   trigger; "becomes the target" triggers for players and permanents
//!   (Cephalid Illusionist, Loki);
//! - Nomads en-Kor's damage redirection shield, Cephalid Illusionist's
//!   combat damage prevention, Sejiri Steppe's protection from a color;
//! - Ensnaring Bridge's and Orim's Chant's attack and cast restrictions,
//!   Goblin Rabblemaster's attack requirement, Sundering Eruption's block
//!   restriction, Anger's haste from the graveyard;
//! - Echoing Deeps entering as a copy of a graveyard land (its triggers wait
//!   for the choice), Eumidian Hatchery's hatchling counters, Ox of Agonas'
//!   escape counter, Green Sun's Zenith's shuffle into the library, Simian
//!   Spirit Guide's mana from the hand.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

pub mod statics;
pub(crate) mod targets;

use crate::card_def::{
    card_id_by_name, CardDef, CardType, Keywords, Subtype, TargetSpec, CARD_DEFS,
};
use crate::effect::{
    EffectContinuation, EffectObjectBinding, EffectOp, ExecCtx, ObjectRef, PlayerRef,
};
use crate::eternal_cards_v1::e3::{E3ChoiceKindV1, E3ChoiceV1};
use crate::eternal_cards_v1::EternalOpV1;
use crate::event::{self, CommittedEvent, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::mana::ManaColor;
use crate::standard_cards_v1::{StandardOpV1, StandardTargetV1, StandardTriggerV1};
use crate::state::{
    AbilitySourceContractV4, CastMethodV4, GameState, ObjectLinkV4, StackItem, StackItemKind,
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
pub struct E9aStateV1 {
    /// Triggers detected by hooks, waiting for the next trigger collection.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub queued_triggers: Vec<QueuedTriggerV1>,
    /// Blood Moon and Magus of the Moon incarnations and the timestamp each
    /// entered with.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub moons: Vec<(ObjectLinkV4, u64)>,
    /// Ox of Agonas spells cast for their escape cost, by stack incarnation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub escape_casts: Vec<ObjectLinkV4>,
    /// Players who can't cast spells this turn (Orim's Chant).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cast_bans: Vec<(PlayerId, TurnV1)>,
    /// Turns in which creatures can't attack (Orim's Chant, kicked).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attack_bans: Vec<TurnV1>,
    /// Turns in which creatures without flying can't block (Sundering
    /// Eruption).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub block_bans: Vec<TurnV1>,
    /// Creatures with protection from a color until end of turn (Sejiri
    /// Steppe).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub protections: Vec<(ObjectLinkV4, ManaColor, TurnV1)>,
    /// Creatures whose combat damage, dealt and received, is prevented this
    /// turn (Cephalid Illusionist).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prevented: Vec<(ObjectLinkV4, TurnV1)>,
    /// Nomads en-Kor's redirection shields.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub redirects: Vec<RedirectV1>,
    /// Creatures with Legolas's Quick Reflexes' granted trigger this turn.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reflexes: Vec<(ObjectLinkV4, TurnV1)>,
    /// Loki incarnations whose trigger already triggered this turn.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub loki_triggered: Vec<(ObjectLinkV4, TurnV1)>,
    /// Hatchling counters on each Eumidian Hatchery incarnation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hatchlings: Vec<(ObjectLinkV4, u16)>,
    /// Echoing Deeps incarnations whose entry choice was made.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deeps_answered: Vec<ObjectLinkV4>,
    /// Green Sun's Zenith spells, by stack incarnation, that shuffle into
    /// their owner's library as they finish resolving.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub zenith_shuffles: Vec<ObjectLinkV4>,
}

impl E9aStateV1 {
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
    pub(crate) fn now(state: &GameState) -> Self {
        Self {
            turn: state.turn_key_v1(),
            active: state.active_player,
        }
    }
}

/// Nomads en-Kor: "The next 1 damage that would be dealt to this creature
/// this turn is dealt to target creature you control instead."
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RedirectV1 {
    /// The Nomads en-Kor incarnation the damage would be dealt to.
    pub from: ObjectLinkV4,
    /// The creature dealt the damage instead.
    pub to: ObjectLinkV4,
    pub turn: TurnV1,
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

pub(crate) fn e9a_state(state: &mut GameState) -> &mut E9aStateV1 {
    &mut state.eternal_v1.get_or_insert_with(Default::default).e9a
}

pub(crate) fn read_state(state: &GameState) -> Option<&E9aStateV1> {
    state.eternal_v1.as_ref().map(|eternal| &eternal.e9a)
}

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::E9aV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum E9aOpV1 {
    /// Breakthrough's keep: choose X cards in hand, discard the rest. A
    /// choice.
    BreakthroughKeepV1,
    /// Green Sun's Zenith's search. A choice.
    GreenSunsZenithV1,
    /// Inquisition of Kozilek. A choice.
    InquisitionV1,
    /// Legolas's Quick Reflexes on target 0.
    LegolasV1,
    /// The granted trigger: `creature` deals damage equal to its power to
    /// target 0.
    ReflexesDamageV1 { creature: ObjectLinkV4 },
    /// Orim's Chant on target player 0.
    OrimsChantV1,
    /// Divided damage among the targets: `total`, or Shatterskull
    /// Smashing's total for X when `total` is `None`.
    DividedDamageV1 { total: Option<i32> },
    /// "Creatures without flying can't block this turn."
    NonflyersCantBlockV1,
    /// Broadside Bombardiers' boast on target 0.
    BombardiersBoastV1,
    /// Cephalid Illusionist's prevention on target 0.
    IllusionistPreventV1,
    /// Grindstone on target player 0: mill two, then `GrindstoneCheckV1`.
    /// May be a choice (the milled cards' order).
    GrindstoneV1,
    /// Grindstone's repeat: if the two `milled` cards (library
    /// incarnations) are in the graveyard and share a color, mill again.
    GrindstoneCheckV1 { milled: Vec<ObjectLinkV4> },
    /// Mishra's Research Desk's exile. A choice.
    ResearchDeskV1,
    /// Nomads en-Kor's shield toward target 0.
    NomadsRedirectV1,
    /// Tolaria's banding loss, which changes nothing here.
    NoEffectV1,
    /// Exile target 0 (Ashen Rider).
    ExileTargetV1,
    /// Caves of Chaos Adventurer's attack trigger.
    CavesExileTopV1,
    /// Chaos Defiler's trigger. A choice.
    ChaosDefilerV1,
    /// Endurance's trigger on up to one target player 0.
    EnduranceV1,
    /// Goblin Rabblemaster's attack trigger.
    RabblemasterPumpV1,
    /// Samwise the Stouthearted: return target 0 to hand.
    SamwiseReturnV1,
    /// "Then the Ring tempts you." A choice.
    RingTemptsV1,
    /// Sejiri Steppe: target 0 gains protection from `color`.
    ProtectionFromV1 { color: ManaColor },
    /// Poxwalkers' return from the graveyard.
    PoxwalkersReturnV1,
    /// Bridge from Below's Zombie.
    BridgeZombieV1,
    /// Bridge from Below exiles itself.
    BridgeExileV1,
    /// A +1/+1 counter on Super Shredder.
    ShredderCounterV1,
    /// Eumidian Hatchery's Insects.
    HatcheryInsectsV1 { count: u16 },
    /// The printed slot of a mana ability the engine performs without the
    /// stack (Simian Spirit Guide); never resolves.
    SpecialActionV1,
}

impl E9aOpV1 {
    /// Whether this leaf can yield a player decision, so its program must
    /// use the resumable interpreter.
    pub(crate) fn contains_player_choice(&self) -> bool {
        matches!(
            self,
            Self::BreakthroughKeepV1
                | Self::GrindstoneV1
                | Self::GreenSunsZenithV1
                | Self::InquisitionV1
                | Self::ResearchDeskV1
                | Self::ChaosDefilerV1
                | Self::RingTemptsV1
        )
    }
}

/// Resolution choices owned by this module, carried as
/// `E3ChoiceKindV1::E9aV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum E9aChoiceV1 {
    /// Breakthrough: the cards in hand to keep.
    BreakthroughKeepV1,
    /// Green Sun's Zenith: up to one green creature card.
    ZenithSearchV1,
    /// Inquisition of Kozilek: the card the target player discards.
    InquisitionDiscardV1,
    /// Mishra's Research Desk: the exiled card to play.
    ResearchDeskPlayV1,
    /// Chaos Defiler: the permanent to destroy.
    ChaosDefilerDestroyV1,
}

impl E9aChoiceV1 {
    /// Whether the candidates are hidden from the other player.
    pub(crate) fn chooser_private(&self) -> bool {
        matches!(self, Self::BreakthroughKeepV1 | Self::ZenithSearchV1)
    }

    /// Whether the chooser sees library cards it has not looked at before.
    pub(crate) fn library_search(&self) -> bool {
        matches!(self, Self::ZenithSearchV1)
    }

    pub(crate) fn public_purpose(&self) -> crate::rl::TargetSelectionPurposeV4 {
        match self {
            Self::ChaosDefilerDestroyV1 => crate::rl::TargetSelectionPurposeV4::PermanentSelection,
            _ => crate::rl::TargetSelectionPurposeV4::CardSelection,
        }
    }

    /// Object incarnations this choice refers to beyond its candidates.
    pub(crate) fn referenced_bindings(&self) -> Vec<EffectObjectBinding> {
        Vec::new()
    }
}

// ---- Names and helpers --------------------------------------------------------------

pub(crate) const ANGER: &str = "Anger";
const ASHEN_RIDER: &str = "Ashen Rider";
pub(crate) const BLOOD_MOON: &str = "Blood Moon";
const BRIDGE_FROM_BELOW: &str = "Bridge from Below";
pub(crate) const BROADSIDE_BOMBARDIERS: &str = "Broadside Bombardiers";
const CAVES_OF_CHAOS_ADVENTURER: &str = "Caves of Chaos Adventurer";
pub(crate) const CEPHALID_COLISEUM: &str = "Cephalid Coliseum";
const CEPHALID_ILLUSIONIST: &str = "Cephalid Illusionist";
const CHAOS_DEFILER: &str = "Chaos Defiler";
pub(crate) const ECHOING_DEEPS: &str = "Echoing Deeps";
pub(crate) const ELVISH_RECLAIMER: &str = "Elvish Reclaimer";
const ENDURANCE: &str = "Endurance";
pub(crate) const ENSNARING_BRIDGE: &str = "Ensnaring Bridge";
pub(crate) const EUMIDIAN_HATCHERY: &str = "Eumidian Hatchery";
const FURY: &str = "Fury";
pub(crate) const GOBLIN_RABBLEMASTER: &str = "Goblin Rabblemaster";
const GREEN_SUNS_ZENITH: &str = "Green Sun's Zenith";
pub(crate) const KNIGHT_OF_THE_RELIQUARY: &str = "Knight of the Reliquary";
pub(crate) const LEGOLAS: &str = "Legolas's Quick Reflexes";
const LOKI: &str = "Loki, God of Mischief";
pub(crate) const MAGUS_OF_THE_MOON: &str = "Magus of the Moon";
const OX_OF_AGONAS: &str = "Ox of Agonas";
pub(crate) const PAINTERS_SERVANT: &str = "Painter's Servant";
const POXWALKERS: &str = "Poxwalkers";
const SAMWISE: &str = "Samwise the Stouthearted";
const SEJIRI_STEPPE: &str = "Sejiri Steppe";
pub(crate) const SHATTERSKULL_SMASHING: &str = "Shatterskull Smashing";
pub(crate) const SIMIAN_SPIRIT_GUIDE: &str = "Simian Spirit Guide";
pub(crate) const SUNDERING_ERUPTION: &str = "Sundering Eruption";
const SUPER_SHREDDER: &str = "Super Shredder";
pub(crate) const TOLARIA: &str = "Tolaria";
const UNDERMOUNTAIN_ADVENTURER: &str = "Undermountain Adventurer";
const GOBLIN_HASTE_TOKEN: &str = "Goblin Haste Token";
const BLACK_INSECT_TOKEN: &str = "Black Insect Token";
const ZOMBIE_TOKEN: &str = "Zombie Token";

fn e9a(op: E9aOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::E9aV1(op)))
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

/// The resolving ability's source, if it is still the incarnation that
/// created the ability, in its owner's graveyard.
fn graveyard_ability_source(ctx: &ExecCtx, state: &GameState) -> Option<ObjectId> {
    let contract = ctx.ability_source_contract?;
    let live = state.objects.try_get(contract.source)?;
    (live.zone == Zone::Graveyard
        && live.zone_change_count == contract.zone_change_count
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
    e9a_state(state).queued_triggers.push(QueuedTriggerV1 {
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

/// Puts `card` onto the battlefield under `controller`'s control, tapped
/// if `tapped`.
fn put_onto_battlefield(state: &mut GameState, card: ObjectId, controller: PlayerId, tapped: bool) {
    let proposed = ProposedEvent::zone_change_to_battlefield_under_controller(card, controller);
    let proposed = match proposed {
        ProposedEvent::ZoneChange(mut change) if tapped => {
            change.force_battlefield_tapped = true;
            ProposedEvent::ZoneChange(change)
        }
        other => other,
    };
    event::propose_and_commit(state, proposed);
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

/// Whether `object`'s battlefield incarnation `zone_change_count` was a
/// creature as it left (its stats were recorded as last-known information).
pub(crate) fn left_as_creature(
    state: &GameState,
    object: ObjectId,
    zone_change_count: u32,
) -> bool {
    state.creature_stats_lki_v1.as_ref().is_some_and(|entries| {
        entries.iter().any(|entry| {
            entry.source.object == object && entry.source.zone_change_count == zone_change_count
        })
    })
}

// ---- Spells -------------------------------------------------------------------------

/// Breakthrough: "Draw four cards, then choose X cards in your hand and
/// discard the rest." The draws are their own leaf, so dredge can replace
/// them.
pub fn breakthrough() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 4,
        },
        e9a(E9aOpV1::BreakthroughKeepV1),
    ])
}

/// Green Sun's Zenith: "Search your library for a green creature card with
/// mana value X or less, put it onto the battlefield, then shuffle. Shuffle
/// Green Sun's Zenith into its owner's library."
pub fn green_suns_zenith() -> EffectOp {
    e9a(E9aOpV1::GreenSunsZenithV1)
}

/// Inquisition of Kozilek: "Target player reveals their hand. You choose a
/// nonland card from it with mana value 3 or less. That player discards
/// that card."
pub fn inquisition_of_kozilek() -> EffectOp {
    e9a(E9aOpV1::InquisitionV1)
}

/// Legolas's Quick Reflexes: "Untap target creature. Until end of turn, it
/// gains reach, hexproof, and 'Whenever this creature becomes tapped, it
/// deals damage equal to its power to up to one target creature.'" Split
/// second is `statics::split_second_active`.
pub fn legolas_quick_reflexes() -> EffectOp {
    e9a(E9aOpV1::LegolasV1)
}

/// Orim's Chant: "Target player can't cast spells this turn. If this spell
/// was kicked, creatures can't attack this turn."
pub fn orims_chant() -> EffectOp {
    e9a(E9aOpV1::OrimsChantV1)
}

/// Otherworldly Gaze: "Surveil 3." Its flashback is the card's own
/// `CardDef::flashback`.
pub fn otherworldly_gaze() -> EffectOp {
    EffectOp::Surveil {
        player: PlayerRef::Controller,
        count: 3,
    }
}

/// Shatterskull Smashing: "deals X damage divided as you choose among up to
/// two target creatures and/or planeswalkers. If X is 6 or more, it deals
/// twice X damage divided as you choose among them instead."
pub fn shatterskull_smashing() -> EffectOp {
    e9a(E9aOpV1::DividedDamageV1 { total: None })
}

/// Sundering Eruption: "Destroy target land. Its controller may search
/// their library for a basic land card, put it onto the battlefield tapped,
/// then shuffle. Creatures without flying can't block this turn." The
/// destruction and search are wave E8a's leaf (Erode).
pub fn sundering_eruption() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::E8aV1(
            crate::eternal_legacy_e8a_v1::E8aOpV1::DestroyThenSearchBasicTappedV1,
        ))),
        e9a(E9aOpV1::NonflyersCantBlockV1),
    ])
}

// ---- Activated abilities --------------------------------------------------------------

/// Broadside Bombardiers' boast.
pub fn bombardiers_boast() -> EffectOp {
    e9a(E9aOpV1::BombardiersBoastV1)
}

/// Cephalid Coliseum: "Target player draws three cards, then discards three
/// cards."
pub fn coliseum_threshold() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::DrawCards {
            player: PlayerRef::Target(0),
            count: 3,
        },
        EffectOp::DiscardCards {
            player: PlayerRef::Target(0),
            count: 3,
        },
    ])
}

/// Cephalid Illusionist: "Prevent all combat damage that would be dealt to
/// and dealt by target creature you control this turn."
pub fn illusionist_prevent() -> EffectOp {
    e9a(E9aOpV1::IllusionistPreventV1)
}

/// Grindstone: "Target player mills two cards. If two cards that share a
/// color were milled this way, repeat this process."
pub fn grindstone() -> EffectOp {
    e9a(E9aOpV1::GrindstoneV1)
}

/// Mishra's Research Desk's sacrifice ability.
pub fn research_desk() -> EffectOp {
    e9a(E9aOpV1::ResearchDeskV1)
}

/// Nomads en-Kor's ability.
pub fn nomads_redirect() -> EffectOp {
    e9a(E9aOpV1::NomadsRedirectV1)
}

/// Simian Spirit Guide's ability, performed without the stack by
/// `statics::special_activation`; the program itself is never resolved.
pub fn spirit_guide_mana() -> EffectOp {
    e9a(E9aOpV1::SpecialActionV1)
}

/// Tolaria: "Target creature loses banding and all 'bands with other'
/// abilities until end of turn." No card here has banding.
pub fn tolaria_banding() -> EffectOp {
    e9a(E9aOpV1::NoEffectV1)
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

/// Wave E7b's evoke condition: "When this enters, if its evoke cost was
/// paid, its controller sacrifices it."
const EVOKED: TriggerCondition = TriggerCondition::StandardV1(StandardTriggerV1::EternalV1(
    crate::eternal_cards_v1::EternalTriggerV1::E7bV1(
        crate::eternal_vintage_e7b_v1::E7bTriggerV1::EvokedEntersV1,
    ),
));

/// Evoke's sacrifice (wave E7b's leaf).
fn evoked_sacrifice() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::E7bV1(
        crate::eternal_vintage_e7b_v1::E7bOpV1::SacrificeSourceV1,
    )))
}

/// Ashen Rider: "exile target permanent."
fn exile_target() -> EffectOp {
    e9a(E9aOpV1::ExileTargetV1)
}

/// "When this creature enters, you take the initiative."
fn take_initiative() -> EffectOp {
    EffectOp::TakeInitiative {
        player: PlayerRef::Controller,
    }
}

/// Caves of Chaos Adventurer: "Whenever this creature attacks, exile the
/// top card of your library. If you've completed a dungeon, you may play
/// that card this turn without paying its mana cost. Otherwise, you may
/// play that card this turn."
fn caves_attacks() -> EffectOp {
    e9a(E9aOpV1::CavesExileTopV1)
}

/// Chaos Defiler: "for each opponent, choose a nonland permanent that player
/// controls. Destroy one of them chosen at random."
fn chaos_defiler() -> EffectOp {
    e9a(E9aOpV1::ChaosDefilerV1)
}

/// Endurance: "up to one target player puts all the cards from their
/// graveyard on the bottom of their library in a random order."
fn endurance_enters() -> EffectOp {
    e9a(E9aOpV1::EnduranceV1)
}

/// Fury: "it deals 4 damage divided as you choose among any number of target
/// creatures and/or planeswalkers."
fn fury_enters() -> EffectOp {
    e9a(E9aOpV1::DividedDamageV1 { total: Some(4) })
}

/// Goblin Rabblemaster: "create a 1/1 red Goblin creature token with
/// haste."
fn rabblemaster_token() -> EffectOp {
    EffectOp::CreateToken {
        token_def: token_id(GOBLIN_HASTE_TOKEN),
        controller: PlayerRef::Controller,
    }
}

/// Goblin Rabblemaster: "it gets +1/+0 until end of turn for each other
/// attacking Goblin."
fn rabblemaster_attacks() -> EffectOp {
    e9a(E9aOpV1::RabblemasterPumpV1)
}

/// Ox of Agonas: "discard your hand, then draw three cards." The discard is
/// wave E7a's leaf; the draws are their own leaf, so dredge can replace
/// them.
fn ox_enters() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::E7aV1(
            crate::eternal_legacy_e7a_v1::E7aOpV1::DiscardHandV1,
        ))),
        EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 3,
        },
    ])
}

/// Samwise the Stouthearted: "choose up to one target permanent card in
/// your graveyard that was put there from the battlefield this turn. Return
/// it to your hand. Then the Ring tempts you."
fn samwise_enters() -> EffectOp {
    EffectOp::Sequence(vec![
        e9a(E9aOpV1::SamwiseReturnV1),
        e9a(E9aOpV1::RingTemptsV1),
    ])
}

/// Sejiri Steppe: "target creature you control gains protection from the
/// color of your choice until end of turn." One option per color in WUBRG
/// order.
fn sejiri_enters() -> EffectOp {
    EffectOp::Choice {
        controller: PlayerRef::Controller,
        options: statics::COLORS
            .into_iter()
            .map(|color| e9a(E9aOpV1::ProtectionFromV1 { color }))
            .collect(),
    }
}

const ASHEN_RIDER_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(TriggerCondition::Etb, exile_target),
    graveyard_trigger(TriggerCondition::LeftBattlefieldToGraveyard, exile_target),
];
const CAVES_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(TriggerCondition::Etb, take_initiative),
    trigger(TriggerCondition::Attacks, caves_attacks),
];
const CHAOS_DEFILER_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(TriggerCondition::Etb, chaos_defiler),
    graveyard_trigger(TriggerCondition::LeftBattlefieldToGraveyard, chaos_defiler),
];
const ENDURANCE_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(TriggerCondition::Etb, endurance_enters),
    trigger(EVOKED, evoked_sacrifice),
];
const FURY_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(TriggerCondition::Etb, fury_enters),
    trigger(EVOKED, evoked_sacrifice),
];
const RABBLEMASTER_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(
        TriggerCondition::BeginningOfControllerCombat,
        rabblemaster_token,
    ),
    trigger(TriggerCondition::Attacks, rabblemaster_attacks),
];
const OX_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(TriggerCondition::Etb, ox_enters)];
const SAMWISE_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(TriggerCondition::Etb, samwise_enters)];
const SEJIRI_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(TriggerCondition::Etb, sejiri_enters)];
const INITIATIVE_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, take_initiative)];

/// Triggered abilities of this module's cards.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        ASHEN_RIDER => &ASHEN_RIDER_TRIGGERS,
        CAVES_OF_CHAOS_ADVENTURER => &CAVES_TRIGGERS,
        CHAOS_DEFILER => &CHAOS_DEFILER_TRIGGERS,
        ENDURANCE => &ENDURANCE_TRIGGERS,
        FURY => &FURY_TRIGGERS,
        GOBLIN_RABBLEMASTER => &RABBLEMASTER_TRIGGERS,
        OX_OF_AGONAS => &OX_TRIGGERS,
        SAMWISE => &SAMWISE_TRIGGERS,
        SEJIRI_STEPPE => &SEJIRI_TRIGGERS,
        UNDERMOUNTAIN_ADVENTURER => &INITIATIVE_TRIGGERS,
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
        ASHEN_RIDER if *effect == exile_target() => TargetSpec::AnyPermanent,
        ENDURANCE if *effect == endurance_enters() => {
            TargetSpec::StandardV1(StandardTargetV1::UpToOnePlayerV1)
        }
        FURY if *effect == fury_enters() => {
            TargetSpec::StandardV1(StandardTargetV1::DividedFourDamageCreaturesOrPlaneswalkersV1)
        }
        SAMWISE if *effect == samwise_enters() => {
            TargetSpec::StandardV1(StandardTargetV1::UpToOnePermanentCardDiedThisTurnV1)
        }
        SEJIRI_STEPPE if *effect == sejiri_enters() => TargetSpec::ControlledCreature,
        _ => TargetSpec::None,
    })
}

/// Queued and delayed triggers owned by this module's cards.
pub(crate) fn delayed_effect_matches(name: &str, effect: &EffectOp) -> bool {
    if name == LOKI {
        return *effect
            == EffectOp::DrawCards {
                player: PlayerRef::Controller,
                count: 1,
            };
    }
    if name == CEPHALID_ILLUSIONIST {
        return *effect == illusionist_mill();
    }
    let EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::E9aV1(op))) = effect else {
        return false;
    };
    match name {
        POXWALKERS => matches!(op, E9aOpV1::PoxwalkersReturnV1),
        BRIDGE_FROM_BELOW => matches!(op, E9aOpV1::BridgeZombieV1 | E9aOpV1::BridgeExileV1),
        SUPER_SHREDDER => matches!(op, E9aOpV1::ShredderCounterV1),
        EUMIDIAN_HATCHERY => matches!(op, E9aOpV1::HatcheryInsectsV1 { .. }),
        _ => false,
    }
}

/// Legolas's Quick Reflexes grants its trigger to any creature.
pub(crate) fn granted_trigger_effect(effect: &EffectOp) -> bool {
    matches!(
        effect,
        EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::E9aV1(
            E9aOpV1::ReflexesDamageV1 { .. }
        )))
    )
}

/// The target of a granted trigger, whatever creature has it.
pub(crate) fn granted_trigger_target_spec(effect: &EffectOp) -> Option<TargetSpec> {
    granted_trigger_effect(effect)
        .then_some(TargetSpec::StandardV1(StandardTargetV1::UpToOneCreature))
}

// ---- Effect leaves ------------------------------------------------------------------

pub(crate) fn execute(op: &E9aOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        E9aOpV1::LegolasV1 => {
            let Some(creature) = legal_object_target(ctx, state, 0, TargetSpec::Creature) else {
                return;
            };
            crate::engine::attempt_untap(state, creature);
            crate::effect::execute(
                &EffectOp::GrantKeywordTargetUntilEndOfTurn {
                    object: ObjectRef::Target(0),
                    keyword: Keywords(Keywords::REACH.0 | Keywords::HEXPROOF.0),
                },
                ctx,
                state,
            );
            if state.objects.get(creature).zone != Zone::Battlefield {
                return;
            }
            let record = (live_link(state, creature), TurnV1::now(state));
            let reflexes = &mut e9a_state(state).reflexes;
            if !reflexes.contains(&record) {
                reflexes.push(record);
            }
        }
        E9aOpV1::ReflexesDamageV1 { creature } => {
            let binding = EffectObjectBinding {
                object: creature.object,
                expected_zone: Zone::Battlefield,
                expected_zone_change_count: creature.zone_change_count,
            };
            let power =
                crate::creature_stats_lki_v1::current_or_last_creature_stats(state, binding)
                    .map_or(0, |(power, _)| power);
            let spec = TargetSpec::StandardV1(StandardTargetV1::UpToOneCreature);
            let Some(target) = legal_object_target(ctx, state, 0, spec) else {
                return;
            };
            if power > 0 {
                event::propose_and_commit(
                    state,
                    event::damage_from_exec_ctx_v1(ctx, Target::Object(target), power),
                );
            }
        }
        E9aOpV1::OrimsChantV1 => {
            let Some(player) = legal_player_target(ctx, state, 0, TargetSpec::AnyPlayer) else {
                return;
            };
            let turn = TurnV1::now(state);
            let e9a = e9a_state(state);
            if !e9a.cast_bans.contains(&(player, turn)) {
                e9a.cast_bans.push((player, turn));
            }
            if ctx.kicked && !e9a.attack_bans.contains(&turn) {
                e9a.attack_bans.push(turn);
            }
        }
        E9aOpV1::DividedDamageV1 { total } => divided_damage(ctx, state, *total),
        E9aOpV1::NonflyersCantBlockV1 => {
            let turn = TurnV1::now(state);
            let bans = &mut e9a_state(state).block_bans;
            if !bans.contains(&turn) {
                bans.push(turn);
            }
        }
        E9aOpV1::BombardiersBoastV1 => {
            let Some(paid) = ctx.paid_cost_refs.first() else {
                halt(state, ctx.source);
                return;
            };
            let mana_value = CARD_DEFS
                .get(usize::from(paid.card_def))
                .map_or(0, |def| i32::from(def.mana_value));
            let Some(target) = ctx.targets.first().copied() else {
                return;
            };
            if !ctx.target_incarnation_matches(0, state)
                || !crate::engine::effect_target_is_legal(
                    state,
                    ctx.source,
                    ctx.controller,
                    TargetSpec::AnyTarget,
                    &ctx.targets,
                    0,
                )
            {
                return;
            }
            event::propose_and_commit(
                state,
                event::damage_from_exec_ctx_v1(ctx, target, mana_value.saturating_add(2)),
            );
        }
        E9aOpV1::IllusionistPreventV1 => {
            let Some(creature) = legal_object_target(ctx, state, 0, TargetSpec::ControlledCreature)
            else {
                return;
            };
            let record = (live_link(state, creature), TurnV1::now(state));
            let prevented = &mut e9a_state(state).prevented;
            if !prevented.contains(&record) {
                prevented.push(record);
            }
        }
        E9aOpV1::NomadsRedirectV1 => {
            let Some(nomads) = live_ability_source(ctx, state) else {
                return;
            };
            let Some(creature) = legal_object_target(ctx, state, 0, TargetSpec::ControlledCreature)
            else {
                return;
            };
            let record = RedirectV1 {
                from: live_link(state, nomads),
                to: live_link(state, creature),
                turn: TurnV1::now(state),
            };
            e9a_state(state).redirects.push(record);
        }
        E9aOpV1::NoEffectV1 | E9aOpV1::SpecialActionV1 => {}
        E9aOpV1::ExileTargetV1 => {
            if let Some(permanent) = legal_object_target(ctx, state, 0, TargetSpec::AnyPermanent) {
                event::propose_and_commit(
                    state,
                    ProposedEvent::zone_change(permanent, Zone::Exile),
                );
            }
        }
        E9aOpV1::CavesExileTopV1 => caves_exile_top(ctx, state),
        E9aOpV1::EnduranceV1 => {
            let spec = TargetSpec::StandardV1(StandardTargetV1::UpToOnePlayerV1);
            let Some(player) = legal_player_target(ctx, state, 0, spec) else {
                return;
            };
            let cards = state.players[player.index()].graveyard.clone();
            if cards.is_empty() {
                return;
            }
            event::propose_and_commit_batch(
                state,
                cards
                    .iter()
                    .map(|&card| {
                        ProposedEvent::public_library_insert(card, event::LibraryPlacement::Bottom)
                    })
                    .collect(),
            );
            let moved = cards
                .iter()
                .filter(|&&card| state.objects.get(card).zone == Zone::Library)
                .count();
            if state.randomize_library_bottom_v1(player, moved).is_err() {
                halt(state, ctx.source);
            }
        }
        E9aOpV1::RabblemasterPumpV1 => {
            let Some(source) = live_ability_source(ctx, state) else {
                return;
            };
            let others = state
                .engine
                .combat
                .attackers
                .iter()
                .filter(|&&attacker| {
                    attacker != source
                        && state.objects.get(attacker).zone == Zone::Battlefield
                        && has_type(state, attacker, CardType::Creature)
                        && crate::engine::has_effective_subtype(state, attacker, Subtype::Goblin)
                })
                .count();
            if others == 0 {
                return;
            }
            let timestamp = crate::engine::next_timestamp(state);
            let object_zone_change_count = state.objects.get(source).zone_change_count;
            state.engine.until_end_of_turn.push(
                crate::engine::UntilEndOfTurnEffect::ResolvedObjectEffect {
                    object_id: source,
                    object_zone_change_count,
                    layer: crate::engine::Layers::POWER_TOUGHNESS,
                    timestamp,
                    duration: crate::engine::EffectDuration::EndOfTurn,
                    power: i32::try_from(others).unwrap_or(i32::MAX),
                    toughness: 0,
                    grant_haste: false,
                },
            );
        }
        E9aOpV1::SamwiseReturnV1 => {
            let spec = TargetSpec::StandardV1(StandardTargetV1::UpToOnePermanentCardDiedThisTurnV1);
            if let Some(card) = legal_object_target(ctx, state, 0, spec) {
                event::propose_and_commit(state, ProposedEvent::zone_change(card, Zone::Hand));
            }
        }
        E9aOpV1::ProtectionFromV1 { color } => {
            let Some(creature) = legal_object_target(ctx, state, 0, TargetSpec::ControlledCreature)
            else {
                return;
            };
            let record = (live_link(state, creature), *color, TurnV1::now(state));
            let protections = &mut e9a_state(state).protections;
            if !protections.contains(&record) {
                protections.push(record);
            }
        }
        E9aOpV1::PoxwalkersReturnV1 => {
            if let Some(card) = graveyard_ability_source(ctx, state) {
                let owner = state.objects.get(card).owner;
                put_onto_battlefield(state, card, owner, true);
            }
        }
        E9aOpV1::BridgeZombieV1 => {
            // "If this card is in your graveyard" is checked again as the
            // ability resolves.
            if graveyard_ability_source(ctx, state).is_some() {
                create_tokens(ctx, state, ZOMBIE_TOKEN, 1);
            }
        }
        E9aOpV1::BridgeExileV1 => {
            if let Some(card) = graveyard_ability_source(ctx, state) {
                event::propose_and_commit(state, ProposedEvent::zone_change(card, Zone::Exile));
            }
        }
        E9aOpV1::ShredderCounterV1 => {
            if live_ability_source(ctx, state).is_some() {
                crate::effect::execute(&EffectOp::AddPlusOneCounterToAbilitySource, ctx, state);
            }
        }
        E9aOpV1::HatcheryInsectsV1 { count } => {
            create_tokens(ctx, state, BLACK_INSECT_TOKEN, *count);
        }
        // Choices run in `drive`; the plain executor never reaches them.
        E9aOpV1::BreakthroughKeepV1
        | E9aOpV1::GreenSunsZenithV1
        | E9aOpV1::InquisitionV1
        | E9aOpV1::ResearchDeskV1
        | E9aOpV1::ChaosDefilerV1
        | E9aOpV1::RingTemptsV1
        | E9aOpV1::GrindstoneV1
        | E9aOpV1::GrindstoneCheckV1 { .. } => halt(state, ctx.source),
    }
}

/// Divided damage: Fury's fixed `total`, or Shatterskull Smashing's damage
/// for X. Each target still legal on resolution is dealt its share, as one
/// simultaneous batch; an illegal target's share is not dealt (608.2b).
fn divided_damage(ctx: &ExecCtx, state: &mut GameState, total: Option<i32>) {
    let (filter, total) = match total {
        Some(total) => (
            StandardTargetV1::DividedFourDamageCreaturesOrPlaneswalkersV1,
            total,
        ),
        None => (
            StandardTargetV1::DividedXDamageUpToTwoV1,
            targets::shatterskull_total(ctx.x_value),
        ),
    };
    let spec = TargetSpec::StandardV1(filter);
    let damage = targets::division(filter, &ctx.targets, total)
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

/// Caves of Chaos Adventurer's attack trigger.
fn caves_exile_top(ctx: &ExecCtx, state: &mut GameState) {
    let player = ctx.controller;
    let Some(&top) = state.players[player.index()].library.first() else {
        return;
    };
    event::propose_and_commit(state, ProposedEvent::zone_change(top, Zone::Exile));
    let live = state.objects.get(top);
    if live.zone != Zone::Exile {
        return;
    }
    let zone_change_generation = live.zone_change_count;
    let def = &CARD_DEFS[usize::from(live.card_def)];
    let play_or_cast = if def.is_playable_land() {
        crate::engine::PlayOrCast::Play
    } else if def.is_castable() {
        crate::engine::PlayOrCast::Cast
    } else {
        return;
    };
    let completed = !state.players[player.index()]
        .dungeon
        .completed_dungeons
        .is_empty();
    state
        .engine
        .exile_play_permissions
        .push(crate::engine::PlayPermission {
            without_mana_cost: crate::engine::FreeCastV1(
                completed && play_or_cast == crate::engine::PlayOrCast::Cast,
            ),
            object: top,
            holder: player,
            zone_change_generation,
            play_or_cast,
            expiry: crate::engine::PlayPermissionExpiry::EndOfTurn,
        });
}

/// Cephalid Illusionist: "Whenever this creature becomes the target of a
/// spell or ability, mill three cards."
fn illusionist_mill() -> EffectOp {
    EffectOp::MillCards {
        player: PlayerRef::Controller,
        count: 3,
    }
}

/// Grindstone: "Target player mills two cards. If two cards that share a
/// color were milled this way, repeat this process." Pushes the mill and,
/// after it, the check of the two milled cards.
fn grindstone_mill(
    state: &GameState,
    continuation: &mut EffectContinuation,
    player: PlayerId,
    path: &[u16],
) {
    let library = &state.players[player.index()].library;
    if library.is_empty() {
        return;
    }
    let milled = library
        .iter()
        .take(2)
        .map(|&card| live_link(state, card))
        .collect::<Vec<_>>();
    if milled.len() == 2 {
        continuation
            .frames
            .push(crate::effect::EffectFrame::Program {
                op: e9a(E9aOpV1::GrindstoneCheckV1 { milled }),
                path: path.to_vec(),
            });
    }
    continuation
        .frames
        .push(crate::effect::EffectFrame::Program {
            op: EffectOp::MillCards {
                player: PlayerRef::Target(0),
                count: 2,
            },
            path: path.to_vec(),
        });
}

// ---- Choices ------------------------------------------------------------------------

fn targets_of(bindings: &[EffectObjectBinding]) -> Vec<Target> {
    bindings
        .iter()
        .map(|binding| Target::Object(binding.object))
        .collect()
}

/// Stages one of this module's selections.
fn offer(
    continuation: &mut EffectContinuation,
    kind: E9aChoiceV1,
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
            kind: E3ChoiceKindV1::E9aV1(kind),
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

/// The cards `player` owns in their hand.
fn hand_cards(state: &GameState, player: PlayerId) -> Vec<ObjectId> {
    state.players[player.index()]
        .hand
        .iter()
        .copied()
        .filter(|&card| state.objects.get(card).owner == player)
        .collect()
}

/// Records a resolving Green Sun's Zenith, which shuffles into its owner's
/// library as it finishes resolving.
fn note_zenith(state: &mut GameState, source: ObjectId) {
    let Some(live) = state.objects.try_get(source) else {
        return;
    };
    if live.zone != Zone::Stack
        || live.spell_copy_origin.is_some()
        || definition_name(state, source) != Some(GREEN_SUNS_ZENITH)
    {
        return;
    }
    let link = live_link(state, source);
    let shuffles = &mut e9a_state(state).zenith_shuffles;
    if !shuffles.contains(&link) {
        shuffles.push(link);
    }
}

/// Runs a leaf inside the resumable interpreter. `Ok(true)` when it staged a
/// choice and the continuation must suspend.
pub(crate) fn drive(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    op: &E9aOpV1,
    path: Vec<u16>,
) -> Result<bool, String> {
    let controller = continuation.ctx.controller;
    match op {
        E9aOpV1::BreakthroughKeepV1 => {
            let hand = hand_cards(state, controller);
            let keep = usize::from(continuation.ctx.x_value).min(hand.len());
            if keep >= hand.len() {
                return Ok(false);
            }
            if keep == 0 {
                for card in hand {
                    crate::engine::commit_discarded_card(state, card);
                }
                return Ok(false);
            }
            let bindings = hand.iter().map(|&card| bind(state, card)).collect();
            let keep = u16::try_from(keep).unwrap_or(u16::MAX);
            offer(
                continuation,
                E9aChoiceV1::BreakthroughKeepV1,
                controller,
                bindings,
                keep,
                keep,
                path,
            );
            Ok(true)
        }
        E9aOpV1::GreenSunsZenithV1 => {
            note_zenith(state, continuation.ctx.source);
            let maximum = continuation.ctx.x_value;
            let cards = state.players[controller.index()]
                .library
                .iter()
                .copied()
                .filter(|&card| {
                    definition(state, card).is_some_and(|def| {
                        def.has_type(CardType::Creature) && def.mana_value <= maximum
                    }) && statics::card_has_color(state, card, ManaColor::G)
                })
                .map(|card| bind(state, card))
                .collect::<Vec<_>>();
            if cards.is_empty() {
                shuffle(state, controller)?;
                return Ok(false);
            }
            offer(
                continuation,
                E9aChoiceV1::ZenithSearchV1,
                controller,
                cards,
                0,
                1,
                path,
            );
            Ok(true)
        }
        E9aOpV1::InquisitionV1 => {
            let ctx = continuation.ctx.clone();
            let Some(player) = legal_player_target(&ctx, state, 0, TargetSpec::AnyPlayer) else {
                return Ok(false);
            };
            // "Target player reveals their hand."
            let hand = state.players[player.index()].hand.clone();
            for &card in &hand {
                state.reveal_hand_card(player.opponent(), player, card)?;
            }
            let cards = hand
                .iter()
                .copied()
                .filter(|&card| {
                    definition(state, card)
                        .is_some_and(|def| !def.has_type(CardType::Land) && def.mana_value <= 3)
                })
                .map(|card| bind(state, card))
                .collect::<Vec<_>>();
            if cards.is_empty() {
                return Ok(false);
            }
            offer(
                continuation,
                E9aChoiceV1::InquisitionDiscardV1,
                controller,
                cards,
                1,
                1,
                path,
            );
            Ok(true)
        }
        E9aOpV1::ResearchDeskV1 => {
            let top = state.players[controller.index()]
                .library
                .iter()
                .copied()
                .take(2)
                .collect::<Vec<_>>();
            if top.is_empty() {
                return Ok(false);
            }
            event::propose_and_commit_batch(
                state,
                top.iter()
                    .map(|&card| ProposedEvent::zone_change(card, Zone::Exile))
                    .collect(),
            );
            let exiled = top
                .into_iter()
                .filter(|&card| state.objects.get(card).zone == Zone::Exile)
                .map(|card| bind(state, card))
                .collect::<Vec<_>>();
            if exiled.is_empty() {
                return Ok(false);
            }
            offer(
                continuation,
                E9aChoiceV1::ResearchDeskPlayV1,
                controller,
                exiled,
                1,
                1,
                path,
            );
            Ok(true)
        }
        E9aOpV1::ChaosDefilerV1 => {
            let candidates = battlefield(state)
                .filter(|&object| {
                    state.objects.get(object).controller == controller.opponent()
                        && !has_type(state, object, CardType::Land)
                })
                .map(|object| bind(state, object))
                .collect::<Vec<_>>();
            if candidates.is_empty() {
                return Ok(false);
            }
            offer(
                continuation,
                E9aChoiceV1::ChaosDefilerDestroyV1,
                controller,
                candidates,
                1,
                1,
                path,
            );
            Ok(true)
        }
        E9aOpV1::RingTemptsV1 => Ok(crate::eternal_pauper_e8b_v1::ring_tempts(
            state,
            continuation,
            path,
        )),
        E9aOpV1::GrindstoneV1 => {
            let ctx = continuation.ctx.clone();
            let Some(player) = legal_player_target(&ctx, state, 0, TargetSpec::AnyPlayer) else {
                return Ok(false);
            };
            grindstone_mill(state, continuation, player, &path);
            Ok(false)
        }
        E9aOpV1::GrindstoneCheckV1 { milled } => {
            let in_graveyard = milled
                .iter()
                .map(|card| ObjectLinkV4 {
                    object: card.object,
                    zone_change_count: card.zone_change_count.saturating_add(1),
                })
                .filter(|&card| link_live_in(state, card, Zone::Graveyard))
                .map(|card| card.object)
                .collect::<Vec<_>>();
            let [first, second] = in_graveyard.as_slice() else {
                return Ok(false);
            };
            if statics::color_mask(state, *first)
                & statics::color_mask(state, *second)
                & statics::WUBRG
                == 0
            {
                return Ok(false);
            }
            let Some(Target::Player(player)) = continuation.ctx.targets.first().copied() else {
                return Ok(false);
            };
            grindstone_mill(state, continuation, player, &path);
            Ok(false)
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
    kind: &E9aChoiceV1,
    bindings: Vec<EffectObjectBinding>,
) -> Result<bool, String> {
    let player = choice.player;
    let chosen = bindings.first().copied();
    match kind {
        E9aChoiceV1::BreakthroughKeepV1 => {
            let kept = bindings
                .iter()
                .map(|binding| binding.object)
                .collect::<Vec<_>>();
            for card in hand_cards(state, player) {
                if !kept.contains(&card) {
                    crate::engine::commit_discarded_card(state, card);
                }
            }
            Ok(false)
        }
        E9aChoiceV1::ZenithSearchV1 => {
            if let Some(card) = chosen {
                put_onto_battlefield(state, card.object, player, false);
            }
            shuffle(state, player)?;
            Ok(false)
        }
        E9aChoiceV1::InquisitionDiscardV1 => {
            let Some(card) = chosen else {
                return Err("Inquisition of Kozilek chooses a card".to_string());
            };
            if state.objects.get(card.object).zone == Zone::Hand {
                crate::engine::commit_discarded_card(state, card.object);
            }
            Ok(false)
        }
        E9aChoiceV1::ResearchDeskPlayV1 => {
            let Some(card) = chosen else {
                return Err("Mishra's Research Desk chooses a card".to_string());
            };
            let live = state.objects.get(card.object);
            if live.zone != Zone::Exile || live.zone_change_count != card.expected_zone_change_count
            {
                return Ok(false);
            }
            let def = &CARD_DEFS[usize::from(live.card_def)];
            let play_or_cast = if def.is_playable_land() {
                crate::engine::PlayOrCast::Play
            } else if def.is_castable() {
                crate::engine::PlayOrCast::Cast
            } else {
                return Ok(false);
            };
            let holder = continuation.ctx.controller;
            state
                .engine
                .exile_play_permissions
                .push(crate::engine::PlayPermission {
                    without_mana_cost: crate::engine::FreeCastV1::default(),
                    object: card.object,
                    holder,
                    zone_change_generation: card.expected_zone_change_count,
                    play_or_cast,
                    expiry: crate::engine::PlayPermissionExpiry::UntilHoldersNextTurn {
                        holder_turn_started: false,
                    },
                });
            Ok(false)
        }
        E9aChoiceV1::ChaosDefilerDestroyV1 => {
            if let Some(object) = chosen {
                if state.objects.get(object.object).zone == Zone::Battlefield {
                    let ctx = continuation.ctx.clone();
                    destroy(&ctx, state, object.object);
                }
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
    note_spell_casts(state, events);
    note_entries(state, events);
    note_departures(state, events);
    note_taps(state, events);
    prune(state);
    let Some(eternal) = state.eternal_v1.as_mut() else {
        return Vec::new();
    };
    if eternal.e9a.queued_triggers.is_empty() {
        return Vec::new();
    }
    std::mem::take(&mut eternal.e9a.queued_triggers)
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

/// The stack item of the spell `spell`, if it is on the stack.
fn spell_item(state: &GameState, spell: ObjectId) -> Option<StackItem> {
    state
        .stack
        .iter()
        .find(|item| item.kind == StackItemKind::Spell && item.source == spell)
        .cloned()
}

/// Spells cast: Fury's and Endurance's evoke (recorded for wave E7b's evoke
/// trigger), Ox of Agonas' escape, and Poxwalkers' returns.
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
        let origin = live.v4.spell_cast_origin.map(|origin| origin.origin_zone);
        let Some(item) = spell_item(state, spell) else {
            continue;
        };
        let name = definition_name(state, spell);
        if !item.is_copy
            && item.v4.cast_method == Some(CastMethodV4::Alternative)
            && matches!(name, Some(FURY | ENDURANCE))
        {
            let link = live_link(state, spell);
            let casts = &mut crate::eternal_vintage_e7b_v1::e7b_state(state).evoked_casts;
            if !casts.contains(&link) {
                casts.push(link);
            }
        }
        if !item.is_copy
            && item.v4.cast_method == Some(CastMethodV4::Escape)
            && name == Some(OX_OF_AGONAS)
        {
            let link = live_link(state, spell);
            let casts = &mut e9a_state(state).escape_casts;
            if !casts.contains(&link) {
                casts.push(link);
            }
        }
        // Poxwalkers: "Whenever you cast a spell from anywhere other than
        // your hand".
        if origin.is_none_or(|zone| zone == Zone::Hand) {
            continue;
        }
        let poxwalkers = state.players[controller.index()]
            .graveyard
            .iter()
            .copied()
            .filter(|&card| {
                let live = state.objects.get(card);
                live.owner == controller
                    && !live.v4.is_token
                    && definition(state, card).is_some_and(|def| def.name == POXWALKERS)
            })
            .collect::<Vec<_>>();
        for card in poxwalkers {
            let source = AbilitySourceContractV4::capture(state, card);
            queue_trigger(
                state,
                source,
                controller,
                e9a(E9aOpV1::PoxwalkersReturnV1),
                None,
            );
        }
    }
}

/// Permanents entering: the moons' timestamps.
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
            || !matches!(
                definition_name(state, object),
                Some(BLOOD_MOON | MAGUS_OF_THE_MOON)
            )
        {
            continue;
        }
        let link = live_link(state, object);
        if read_state(state).is_some_and(|e9a| e9a.moons.iter().any(|(host, _)| *host == link)) {
            continue;
        }
        let timestamp = crate::engine::next_timestamp(state);
        e9a_state(state).moons.push((link, timestamp));
    }
}

/// Whether `card` was put into a graveyard by one of `events`.
fn moved_to_graveyard(events: &[CommittedEvent], card: ObjectId) -> bool {
    events.iter().any(|event| {
        matches!(
            event,
            CommittedEvent::ZoneChange {
                object,
                to: Zone::Graveyard,
                ..
            } if *object == card
        )
    })
}

/// Permanents leaving the battlefield: Super Shredder's counters, Eumidian
/// Hatchery's Insects and Bridge from Below's triggers.
fn note_departures(state: &mut GameState, events: &[CommittedEvent]) {
    for event in events {
        let CommittedEvent::ZoneChange {
            object,
            from: Zone::Battlefield,
            to,
            controller_before,
        } = *event
        else {
            continue;
        };
        let Some(live) = state.objects.try_get(object) else {
            continue;
        };
        let departed = live.zone_change_count.saturating_sub(1);
        let owner = live.owner;
        let is_token = live.v4.is_token;
        let now_in_graveyard = live.zone == Zone::Graveyard;
        // Super Shredder: "Whenever another permanent leaves the
        // battlefield".
        let shredders = active_permanents(state, SUPER_SHREDDER, None)
            .filter(|&shredder| shredder != object)
            .collect::<Vec<_>>();
        for shredder in shredders {
            let source = AbilitySourceContractV4::capture(state, shredder);
            let controller = state.objects.get(shredder).controller;
            queue_trigger(
                state,
                source,
                controller,
                e9a(E9aOpV1::ShredderCounterV1),
                None,
            );
        }
        if to != Zone::Graveyard {
            continue;
        }
        // Eumidian Hatchery: "When this land is put into a graveyard from the
        // battlefield, for each hatchling counter on it, create a 1/1 black
        // Insect creature token with flying."
        if now_in_graveyard && definition_name(state, object) == Some(EUMIDIAN_HATCHERY) {
            let host = ObjectLinkV4 {
                object,
                zone_change_count: departed,
            };
            let count = read_state(state).map_or(0, |e9a| {
                e9a.hatchlings
                    .iter()
                    .find(|(link, _)| *link == host)
                    .map_or(0, |(_, count)| *count)
            });
            if count > 0 {
                let source = AbilitySourceContractV4::capture(state, object);
                queue_trigger(
                    state,
                    source,
                    controller_before,
                    e9a(E9aOpV1::HatcheryInsectsV1 { count }),
                    None,
                );
            }
        }
        // Bridge from Below, from its owner's graveyard. A Bridge put into
        // the graveyard in this same batch wasn't there to see the death.
        if !left_as_creature(state, object, departed) {
            continue;
        }
        for player in [PlayerId::P0, PlayerId::P1] {
            let bridges = state.players[player.index()]
                .graveyard
                .iter()
                .copied()
                .filter(|&card| {
                    card != object
                        && state.objects.get(card).owner == player
                        && definition(state, card).is_some_and(|def| def.name == BRIDGE_FROM_BELOW)
                        && !moved_to_graveyard(events, card)
                })
                .collect::<Vec<_>>();
            for bridge in bridges {
                let op = if player == owner {
                    if is_token {
                        continue;
                    }
                    E9aOpV1::BridgeZombieV1
                } else {
                    E9aOpV1::BridgeExileV1
                };
                let source = AbilitySourceContractV4::capture(state, bridge);
                queue_trigger(state, source, player, e9a(op), None);
            }
        }
    }
}

/// Creatures becoming tapped: Legolas's Quick Reflexes' granted trigger.
fn note_taps(state: &mut GameState, events: &[CommittedEvent]) {
    if read_state(state).is_none_or(|e9a| e9a.reflexes.is_empty()) {
        return;
    }
    let turn = TurnV1::now(state);
    for event in events {
        let CommittedEvent::Tap { object } = *event else {
            continue;
        };
        let Some(live) = state.objects.try_get(object) else {
            continue;
        };
        if live.zone != Zone::Battlefield {
            continue;
        }
        let controller = live.controller;
        let link = live_link(state, object);
        if !read_state(state).is_some_and(|e9a| e9a.reflexes.contains(&(link, turn))) {
            continue;
        }
        let source = AbilitySourceContractV4::capture(state, object);
        queue_trigger(
            state,
            source,
            controller,
            e9a(E9aOpV1::ReflexesDamageV1 { creature: link }),
            Some(TargetSpec::StandardV1(StandardTargetV1::UpToOneCreature)),
        );
    }
}

/// Drops records whose objects moved on or whose turn ended.
fn prune(state: &mut GameState) {
    let Some(e9a) = read_state(state) else {
        return;
    };
    if e9a.is_empty() {
        return;
    }
    let now = TurnV1::now(state);
    let on_battlefield = |link: &ObjectLinkV4| link_live_in(state, *link, Zone::Battlefield);
    let moons = e9a
        .moons
        .iter()
        .copied()
        .filter(|(host, _)| on_battlefield(host))
        .collect::<Vec<_>>();
    let escape_casts = e9a
        .escape_casts
        .iter()
        .copied()
        .filter(|cast| {
            link_live_in(state, *cast, Zone::Stack)
                || state.objects.try_get(cast.object).is_some_and(|live| {
                    live.zone == Zone::Battlefield
                        && cast.zone_change_count.checked_add(1) == Some(live.zone_change_count)
                })
        })
        .collect::<Vec<_>>();
    let cast_bans = e9a
        .cast_bans
        .iter()
        .copied()
        .filter(|(_, turn)| *turn == now)
        .collect::<Vec<_>>();
    let attack_bans = e9a
        .attack_bans
        .iter()
        .copied()
        .filter(|turn| *turn == now)
        .collect::<Vec<_>>();
    let block_bans = e9a
        .block_bans
        .iter()
        .copied()
        .filter(|turn| *turn == now)
        .collect::<Vec<_>>();
    let protections = e9a
        .protections
        .iter()
        .copied()
        .filter(|(host, _, turn)| *turn == now && on_battlefield(host))
        .collect::<Vec<_>>();
    let prevented = e9a
        .prevented
        .iter()
        .copied()
        .filter(|(host, turn)| *turn == now && on_battlefield(host))
        .collect::<Vec<_>>();
    let redirects = e9a
        .redirects
        .iter()
        .copied()
        .filter(|record| record.turn == now && on_battlefield(&record.from))
        .collect::<Vec<_>>();
    let reflexes = e9a
        .reflexes
        .iter()
        .copied()
        .filter(|(host, turn)| *turn == now && on_battlefield(host))
        .collect::<Vec<_>>();
    let loki_triggered = e9a
        .loki_triggered
        .iter()
        .copied()
        .filter(|(_, turn)| *turn == now)
        .collect::<Vec<_>>();
    let hatchlings = e9a
        .hatchlings
        .iter()
        .copied()
        .filter(|(host, _)| on_battlefield(host))
        .collect::<Vec<_>>();
    let deeps_answered = e9a
        .deeps_answered
        .iter()
        .copied()
        .filter(|host| on_battlefield(host))
        .collect::<Vec<_>>();
    let zenith_shuffles = e9a
        .zenith_shuffles
        .iter()
        .copied()
        .filter(|spell| link_live_in(state, *spell, Zone::Stack))
        .collect::<Vec<_>>();
    let changed = moons.len() != e9a.moons.len()
        || escape_casts.len() != e9a.escape_casts.len()
        || cast_bans.len() != e9a.cast_bans.len()
        || attack_bans.len() != e9a.attack_bans.len()
        || block_bans.len() != e9a.block_bans.len()
        || protections.len() != e9a.protections.len()
        || prevented.len() != e9a.prevented.len()
        || redirects.len() != e9a.redirects.len()
        || reflexes.len() != e9a.reflexes.len()
        || loki_triggered.len() != e9a.loki_triggered.len()
        || hatchlings.len() != e9a.hatchlings.len()
        || deeps_answered.len() != e9a.deeps_answered.len()
        || zenith_shuffles.len() != e9a.zenith_shuffles.len();
    if changed {
        let e9a = e9a_state(state);
        e9a.moons = moons;
        e9a.escape_casts = escape_casts;
        e9a.cast_bans = cast_bans;
        e9a.attack_bans = attack_bans;
        e9a.block_bans = block_bans;
        e9a.protections = protections;
        e9a.prevented = prevented;
        e9a.redirects = redirects;
        e9a.reflexes = reflexes;
        e9a.loki_triggered = loki_triggered;
        e9a.hatchlings = hatchlings;
        e9a.deeps_answered = deeps_answered;
        e9a.zenith_shuffles = zenith_shuffles;
    }
}

/// "Becomes the target" triggers, called as a spell or ability finishes
/// announcing its targets (and when a spell's targets change): Cephalid
/// Illusionist (by a spell or ability, once per item) and Loki, God of
/// Mischief (a player or permanent, by an ability its controller controls,
/// once each turn).
pub(crate) fn note_targeting(state: &mut GameState, item: &StackItem) {
    if !cfg!(feature = "eternal-fixtures") || item.targets.is_empty() {
        return;
    }
    let mut seen = Vec::new();
    for &target in &item.targets {
        let Target::Object(object) = target else {
            continue;
        };
        if seen.contains(&object) {
            continue;
        }
        seen.push(object);
        if active_named(state, object, CEPHALID_ILLUSIONIST) {
            let source = AbilitySourceContractV4::capture(state, object);
            let controller = state.objects.get(object).controller;
            queue_trigger(state, source, controller, illusionist_mill(), None);
        }
    }
    if !matches!(
        item.kind,
        StackItemKind::ActivatedAbility | StackItemKind::TriggeredAbility
    ) {
        return;
    }
    let targets_player_or_permanent = item.targets.iter().any(|target| match *target {
        Target::Player(_) => true,
        Target::Object(object) => state
            .objects
            .try_get(object)
            .is_some_and(|live| live.zone == Zone::Battlefield),
        Target::StackItem(_) => false,
    });
    if !targets_player_or_permanent {
        return;
    }
    let turn = TurnV1::now(state);
    let lokis = active_permanents(state, LOKI, Some(item.controller))
        .filter(|&loki| {
            let link = live_link(state, loki);
            !read_state(state).is_some_and(|e9a| e9a.loki_triggered.contains(&(link, turn)))
        })
        .collect::<Vec<_>>();
    for loki in lokis {
        let link = live_link(state, loki);
        e9a_state(state).loki_triggered.push((link, turn));
        let source = AbilitySourceContractV4::capture(state, loki);
        queue_trigger(
            state,
            source,
            item.controller,
            EffectOp::DrawCards {
                player: PlayerRef::Controller,
                count: 1,
            },
            None,
        );
    }
}

// ---- Hooks: engine ------------------------------------------------------------------

/// Green Sun's Zenith: "Shuffle Green Sun's Zenith into its owner's
/// library." Whether the resolved spell `item` goes to its owner's library
/// rather than the graveyard.
pub(crate) fn shuffles_into_library(state: &GameState, item: &StackItem) -> bool {
    !item.is_copy
        && state.objects.try_get(item.source).is_some_and(|live| {
            live.zone == Zone::Stack
                && read_state(state).is_some_and(|e9a| {
                    e9a.zenith_shuffles.contains(&ObjectLinkV4 {
                        object: item.source,
                        zone_change_count: live.zone_change_count,
                    })
                })
        })
}

/// After a spell left the stack: a Green Sun's Zenith that went to its
/// owner's library is shuffled into it.
pub(crate) fn after_spell_departure(state: &mut GameState, source: ObjectId) {
    let Some(live) = state.objects.try_get(source) else {
        return;
    };
    if live.zone != Zone::Library {
        return;
    }
    let stack_link = ObjectLinkV4 {
        object: source,
        zone_change_count: live.zone_change_count.saturating_sub(1),
    };
    if !read_state(state).is_some_and(|e9a| e9a.zenith_shuffles.contains(&stack_link)) {
        return;
    }
    let owner = live.owner;
    e9a_state(state)
        .zenith_shuffles
        .retain(|spell| *spell != stack_link);
    if shuffle(state, owner).is_err() {
        halt(state, source);
    }
}

/// The beginning of a turn ends the last turn's records.
pub(crate) fn begin_turn(state: &mut GameState, _player: PlayerId) {
    prune(state);
}
