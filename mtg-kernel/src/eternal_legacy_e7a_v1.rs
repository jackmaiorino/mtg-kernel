//! Card programs for the eternal catalog's wave E7a
//! (`docs/design/eternal_formats_support_v1.md`): Legacy Lands, Sewer-Cam
//! combo, Rakdos Reanimator, The Epic Storm, Stiflenought, Mardu Energy and
//! Affinity Stompy.
//!
//! `eternal_cards_v1` forwards its effect leaves, trigger conditions and
//! state here, and the engine calls the hooks below only in
//! `eternal-fixtures` builds. Every table is keyed by a printed name that
//! occurs only in the eternal registry, so the Pauper, FDN and Standard
//! catalogs never reach it. Behavior was read from each card's current
//! Oracle text and, where present, its XMage source.
//!
//! Primitives added here:
//! - additional land plays (Exploration, Icetill Explorer, Song of
//!   Creation) and land plays from the graveyard (Icetill Explorer);
//! - a spell tax (Sphere of Resistance) and Grafdigger's Cage's entry and
//!   cast prohibitions;
//! - Dark Depths' ice counters and its state trigger, Blast Zone's charge
//!   counters, Maze of Ith's two-way combat damage prevention, The
//!   Tabernacle's granted upkeep payment and Yavimaya's granted land type;
//! - "becomes a copy" of a permanent already on the battlefield (Thespian's
//!   Stage keeping its own ability, Curie keeping its trigger);
//! - Animate Dead's return and attachment, Reanimate, Shallow Grave;
//! - suspend (Gaea's Will), manifest
//!   dread (Abhorrent Oculus), improvise (Kappa Cannoneer, through the
//!   convoke payment), exert (Arena of Glory) and "creatures lose all
//!   abilities" (Dress Down).
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

use crate::card_def::{card_id_by_name, CardDef, CardType, Keywords, TargetSpec, CARD_DEFS};
use crate::effect::{
    EffectContinuation, EffectFrame, EffectObjectBinding, EffectOp, ExecCtx, ObjectRef, PlayerRef,
    TargetRef,
};
use crate::eternal_cards_v1::e3::{E3ChoiceKindV1, E3ChoiceV1};
use crate::eternal_cards_v1::EternalOpV1;
use crate::event::{self, CommittedEvent, DamageProposed, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::mana::ManaColor;
use crate::standard_cards_v1::{StandardOpV1, StandardTriggerV1};
use crate::state::{AbilitySourceContractV4, GameState, ObjectLinkV4, StackItem, Target, Zone};
use crate::trigger::{PendingTrigger, TriggerCondition, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};

// ---- State ------------------------------------------------------------------------

/// Per-game state for this module's cards, kept inside
/// `eternal_cards_v1::EternalStateV1`. Default (and absent from snapshots)
/// until one of these cards needs it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct E7aStateV1 {
    /// Triggers detected by hooks, waiting for the next trigger collection.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub queued_triggers: Vec<QueuedTriggerV1>,
    /// Ice counters on each Dark Depths incarnation. A battlefield Dark
    /// Depths without an entry has none (a copy made by Thespian's Stage).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ice_counters: Vec<(ObjectLinkV4, u32)>,
    /// Permanents that became copies of other permanents while on the
    /// battlefield (Thespian's Stage, Curie), with what they were.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub copies: Vec<CopyRecordV1>,
    /// Battlefield incarnations that were Thespian's Stage copies: their
    /// retained ability resolves even after they leave.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stage_hosts: Vec<ObjectLinkV4>,
    /// Maze of Ith: creatures whose combat damage, dealt and received, is
    /// prevented this turn.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mazed: Vec<(ObjectLinkV4, TurnV1)>,
    /// Shadowspear: permanents that lose hexproof and indestructible this
    /// turn.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stripped: Vec<(ObjectLinkV4, TurnV1)>,
    /// Dress Down incarnations and the timestamp at which they entered.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dress_downs: Vec<(ObjectLinkV4, u64)>,
    /// Animate Dead Auras on the battlefield waiting for their entry
    /// trigger, with the creature card each enchants in a graveyard.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub animating: Vec<(ObjectLinkV4, ObjectLinkV4)>,
    /// Animate Dead Auras and the creature each returned.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub animated: Vec<(ObjectLinkV4, ObjectLinkV4)>,
    /// Shallow Grave's creatures, exiled at the beginning of the next end
    /// step.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shallow_graves: Vec<DelayedExileV1>,
    /// Burning Wish spells that resolved and exile themselves, by stack
    /// incarnation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub self_exiling: Vec<ObjectLinkV4>,
    /// Cards Galvanic Relay exiled: their permission waits for the
    /// holder's next turn.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relayed: Vec<ObjectLinkV4>,
    /// Creature spells paid with Arena of Glory's exerted mana, by stack
    /// incarnation: they gain haste as they enter this turn.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub arena_spells: Vec<(ObjectLinkV4, TurnV1)>,
    /// Suspended cards, by exile incarnation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suspended: Vec<ObjectLinkV4>,
    /// Face-down permanents put onto the battlefield by manifest dread.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub manifested: Vec<ObjectLinkV4>,
    /// Emry's "you may cast that card this turn" permissions.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub emry_permissions: Vec<(ObjectLinkV4, PlayerId, TurnV1)>,
    /// Mox Diamonds that entered and still owe their discard choice.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mox_entering: Vec<ObjectLinkV4>,
}

impl E7aStateV1 {
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

/// A permanent that became a copy while on the battlefield, and the
/// characteristics it returns to when the copy ends.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CopyRecordV1 {
    pub host: ObjectLinkV4,
    pub card_def: u16,
    pub name: String,
    pub color_mask: u8,
    pub subtype_ids: Vec<u16>,
    pub face_index: u8,
}

/// "Exile it at the beginning of the next end step" (Shallow Grave).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DelayedExileV1 {
    pub source: AbilitySourceContractV4,
    pub controller: PlayerId,
    pub creature: ObjectLinkV4,
}

/// A triggered ability queued by a hook.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueuedTriggerV1 {
    pub source: AbilitySourceContractV4,
    pub controller: PlayerId,
    pub op: E7aOpV1,
}

fn e7a_state(state: &mut GameState) -> &mut E7aStateV1 {
    &mut state.eternal_v1.get_or_insert_with(Default::default).e7a
}

fn read_state(state: &GameState) -> Option<&E7aStateV1> {
    state.eternal_v1.as_ref().map(|eternal| &eternal.e7a)
}

/// Trigger conditions owned by this module, carried as
/// `eternal_cards_v1::EternalTriggerV1::E7aV1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum E7aTriggerV1 {
    /// "Whenever you cast an artifact spell" (Patchwork Automaton,
    /// Pinnacle Emissary).
    ControllerCastsArtifactSpellV1,
    /// "When this creature enters from your graveyard" (Phyrexian Dragon
    /// Engine).
    EntersFromYourGraveyardV1,
    /// "At the beginning of each opponent's upkeep" (Abhorrent Oculus).
    OpponentUpkeepV1,
}

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::E7aV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum E7aOpV1 {
    /// Blast Zone: put `count` charge counters on the ability's source
    /// incarnation.
    AddChargeCountersV1 { count: u8 },
    /// Blast Zone: destroy each nonland permanent with mana value equal to
    /// the number of charge counters the sacrificed land had.
    BlastZoneDestroyV1,
    /// Dark Depths: remove an ice counter from the ability's source.
    RemoveIceCounterV1,
    /// Dark Depths' state trigger: sacrifice `depths`, and if you do, create
    /// Marit Lage.
    DarkDepthsV1 { depths: ObjectLinkV4 },
    /// Ghost Quarter: destroy target 0; its controller may search.
    GhostQuarterV1,
    /// Ghost Quarter's search, for `player`. A choice.
    GhostQuarterSearchV1 { player: PlayerId },
    /// Maze of Ith on target 0.
    MazeOfIthV1,
    /// Thespian's Stage: the source becomes a copy of target 0.
    ThespiansStageV1,
    /// Animate Dead's resolution: it enters, enchanting target 0 in its
    /// graveyard.
    AnimateDeadEnterV1,
    /// Animate Dead's entry trigger: return the enchanted card and attach.
    AnimateDeadReturnV1,
    /// Animate Dead's leave trigger: the animated creature's controller
    /// sacrifices it.
    AnimateDeadLeavesV1,
    /// Reanimate on target 0.
    ReanimateV1,
    /// Shallow Grave's effect.
    ShallowGraveV1,
    /// Shallow Grave's delayed trigger: exile `creature`.
    ShallowGraveExileV1 { creature: ObjectLinkV4 },
    /// Cabal Therapy: target 0 reveals their hand and discards every card
    /// named `named`.
    TherapyV1 { named: u16 },
    /// Archon of Cruelty's trigger on target opponent 0. A choice.
    ArchonV1,
    /// Echo of Eons' effect.
    EchoOfEonsV1,
    /// Galvanic Relay's effect.
    GalvanicRelayV1,
    /// Gamble's search. A choice.
    GambleV1,
    /// Burning Wish: no card outside the game; exile it as it resolves.
    BurningWishV1,
    /// Discard the controller's hand (Song of Creation).
    DiscardHandV1,
    /// "Discard your hand. If you do, draw `count` cards" (Phyrexian Dragon
    /// Engine).
    DiscardHandThenDrawV1 { count: u8 },
    /// Goblin Engineer's entry search. A choice.
    EngineerSearchV1,
    /// Goblin Engineer's return of target 0.
    EngineerReturnV1,
    /// Goblin Welder's swap of targets 0 and 1.
    GoblinWelderV1,
    /// Emry's permission for target 0.
    EmryPermissionV1,
    /// A +1/+1 counter on the source and it can't be blocked this turn
    /// (Kappa Cannoneer).
    KappaV1,
    /// Phyrexian Dreadnought's entry trigger. A choice.
    DreadnoughtV1,
    /// Curie: the source becomes a copy of the creature its cost exiled.
    CurieCopyV1,
    /// Curie's combat damage trigger: draw cards equal to the source's base
    /// power, `fallback` if it left.
    CurieDrawV1 { fallback: u32 },
    /// Shadowspear: opponents' permanents lose hexproof and indestructible.
    ShadowspearStripV1,
    /// Tap target 0 (Skateboard).
    TapTargetV1,
    /// The Tabernacle's granted upkeep trigger on its creature. A choice.
    TabernacleV1,
    /// The Tabernacle's payment.
    TabernaclePayV1,
    /// The Tabernacle: destroy the creature.
    TabernacleDestroyV1,
    /// Suspend's upkeep trigger for `card`: remove a time counter, and cast
    /// it when the last is removed. May be a choice.
    SuspendTickV1 { card: ObjectLinkV4 },
    /// Suspend's cast answer for `card`. A choice.
    SuspendCastV1 { card: ObjectLinkV4 },
    /// Manifest dread. A choice.
    ManifestDreadV1,
    /// Arena of Glory's and Gaea's Will's printed slots: the engine performs
    /// them as actions without the stack and never resolves these.
    SpecialActionV1,
}

impl E7aOpV1 {
    /// Whether this leaf can yield a player decision, so its program must
    /// use the resumable interpreter.
    pub(crate) fn contains_player_choice(&self) -> bool {
        matches!(
            self,
            Self::GhostQuarterV1
                | Self::GhostQuarterSearchV1 { .. }
                | Self::ArchonV1
                | Self::GambleV1
                | Self::EngineerSearchV1
                | Self::DreadnoughtV1
                | Self::TabernacleV1
                | Self::SuspendTickV1 { .. }
                | Self::SuspendCastV1 { .. }
                | Self::ManifestDreadV1
        )
    }
}

/// Resolution choices owned by this module, carried as
/// `E3ChoiceKindV1::E7aV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum E7aChoiceV1 {
    /// Ghost Quarter: up to one basic land card from the chooser's library.
    GhostQuarterSearchV1,
    /// Gamble: one card from the chooser's library.
    GambleSearchV1,
    /// Goblin Engineer: up to one artifact card from the chooser's library.
    EngineerSearchV1,
    /// Archon of Cruelty: the creature or planeswalker the opponent
    /// sacrifices.
    ArchonSacrificeV1,
    /// Phyrexian Dreadnought: the creatures to sacrifice instead.
    DreadnoughtV1 { dreadnought: ObjectLinkV4 },
    /// Manifest dread: the looked-at card to manifest.
    ManifestDreadV1 { looked: Vec<EffectObjectBinding> },
}

impl E7aChoiceV1 {
    /// Whether the candidates are hidden from the other player.
    pub(crate) fn chooser_private(&self) -> bool {
        matches!(
            self,
            Self::GhostQuarterSearchV1
                | Self::GambleSearchV1
                | Self::EngineerSearchV1
                | Self::ManifestDreadV1 { .. }
        )
    }

    /// Whether the chooser sees library cards it has not looked at before.
    pub(crate) fn library_search(&self) -> bool {
        matches!(
            self,
            Self::GhostQuarterSearchV1 | Self::GambleSearchV1 | Self::EngineerSearchV1
        )
    }

    pub(crate) fn public_purpose(&self) -> crate::rl::TargetSelectionPurposeV4 {
        match self {
            Self::ArchonSacrificeV1 | Self::DreadnoughtV1 { .. } => {
                crate::rl::TargetSelectionPurposeV4::PermanentSelection
            }
            _ => crate::rl::TargetSelectionPurposeV4::CardSelection,
        }
    }

    /// Object incarnations this choice refers to beyond its candidates.
    pub(crate) fn referenced_bindings(&self) -> Vec<EffectObjectBinding> {
        match self {
            Self::ManifestDreadV1 { looked } => looked.clone(),
            _ => Vec::new(),
        }
    }
}

// ---- Names and helpers --------------------------------------------------------------

const ABHORRENT_OCULUS: &str = "Abhorrent Oculus";
const ANIMATE_DEAD: &str = "Animate Dead";
const ARCHON_OF_CRUELTY: &str = "Archon of Cruelty";
const ARENA_OF_GLORY: &str = "Arena of Glory";
const BLAST_ZONE: &str = "Blast Zone";
const CHROMATIC_STAR: &str = "Chromatic Star";
const CURIE: &str = "Curie, Emergent Intelligence";
const DARK_DEPTHS: &str = "Dark Depths";
const DRESS_DOWN: &str = "Dress Down";
const EMRY: &str = "Emry, Lurker of the Loch";
const EXPLORATION: &str = "Exploration";
const GAEAS_WILL: &str = "Gaea's Will";
const GOBLIN_ENGINEER: &str = "Goblin Engineer";
const GRAFDIGGERS_CAGE: &str = "Grafdigger's Cage";
const ICETILL_EXPLORER: &str = "Icetill Explorer";
const KAPPA_CANNONEER: &str = "Kappa Cannoneer";
const LEGION_EXTRUDER: &str = "Legion Extruder";
const MOX_DIAMOND: &str = "Mox Diamond";
const PATCHWORK_AUTOMATON: &str = "Patchwork Automaton";
const PHYREXIAN_DRAGON_ENGINE: &str = "Phyrexian Dragon Engine";
const PHYREXIAN_DREADNOUGHT: &str = "Phyrexian Dreadnought";
const PINNACLE_EMISSARY: &str = "Pinnacle Emissary";
const SHALLOW_GRAVE: &str = "Shallow Grave";
const SKATEBOARD: &str = "Skateboard";
const SONG_OF_CREATION: &str = "Song of Creation";
const TABERNACLE: &str = "The Tabernacle at Pendrell Vale";
const THESPIANS_STAGE: &str = "Thespian's Stage";
const YAVIMAYA: &str = "Yavimaya, Cradle of Growth";
const MARIT_LAGE_TOKEN: &str = "Marit Lage Token";
const DRONE_TOKEN: &str = "Drone Token";

/// The number of ice counters Dark Depths enters with.
const DARK_DEPTHS_ICE: u32 = 10;
/// Gaea's Will: "Suspend 4".
const SUSPEND_TIME_COUNTERS: u8 = 4;

fn e7a(op: E7aOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::E7aV1(op)))
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

fn binding_live(state: &GameState, binding: EffectObjectBinding) -> bool {
    state.objects.try_get(binding.object).is_some_and(|live| {
        live.zone == binding.expected_zone
            && live.zone_change_count == binding.expected_zone_change_count
    })
}

fn definition(state: &GameState, object: ObjectId) -> Option<&'static CardDef> {
    let live = state.objects.try_get(object)?;
    CARD_DEFS.get(live.card_def as usize)
}

fn definition_name(state: &GameState, object: ObjectId) -> Option<&'static str> {
    definition(state, object).map(|def| def.name)
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

fn any_active(state: &GameState, name: &str) -> bool {
    active_permanents(state, name, None).next().is_some()
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

/// Every battlefield permanent.
fn battlefield(state: &GameState) -> Vec<ObjectId> {
    [PlayerId::P0, PlayerId::P1]
        .into_iter()
        .flat_map(|seat| state.players[seat.index()].battlefield.iter().copied())
        .filter(|&object| state.objects.get(object).zone == Zone::Battlefield)
        .collect()
}

/// The resolving ability's source, if it is still the incarnation that
/// created the ability, on the battlefield.
fn live_ability_source(ctx: &ExecCtx, state: &GameState) -> Option<ObjectId> {
    let contract = ctx.ability_source_contract?;
    let live = state.objects.try_get(contract.source)?;
    (live.zone == Zone::Battlefield && live.zone_change_count == contract.zone_change_count)
        .then_some(contract.source)
}

/// Target `index` if it still names its announced incarnation, in `zone`.
fn bound_target_in(ctx: &ExecCtx, state: &GameState, index: usize, zone: Zone) -> Option<ObjectId> {
    let Some(Target::Object(object)) = ctx.targets.get(index).copied() else {
        return None;
    };
    (ctx.target_incarnation_matches(index, state) && state.objects.get(object).zone == zone)
        .then_some(object)
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

fn queue_trigger(
    state: &mut GameState,
    source: AbilitySourceContractV4,
    controller: PlayerId,
    op: E7aOpV1,
) {
    e7a_state(state).queued_triggers.push(QueuedTriggerV1 {
        source,
        controller,
        op,
    });
}

fn nothing() -> EffectOp {
    EffectOp::Sequence(Vec::new())
}

/// "Sacrifice it" for a resolving ability's own source incarnation.
fn sacrifice_source() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::SacrificeSourceV1))
}

/// Destroy `object` unless it is indestructible.
fn destroy(state: &mut GameState, object: ObjectId) {
    if state.objects.get(object).zone == Zone::Battlefield
        && !crate::engine::has_effective_keyword(state, object, Keywords::INDESTRUCTIBLE)
    {
        event::propose_and_commit(state, ProposedEvent::zone_change(object, Zone::Graveyard));
    }
}

/// Sacrifices each of `objects` still on the battlefield, as one batch.
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

/// The cards (not tokens or copies) in `player`'s graveyard, in order.
fn graveyard_cards(state: &GameState, player: PlayerId) -> Vec<ObjectId> {
    state.players[player.index()]
        .graveyard
        .iter()
        .copied()
        .filter(|&card| {
            let live = state.objects.get(card);
            live.owner == player && !live.v4.is_token && live.spell_copy_origin.is_none()
        })
        .collect()
}

/// The cards in `player`'s library matching `keep`, bound to their library
/// incarnations.
fn library_cards(
    state: &GameState,
    player: PlayerId,
    keep: impl Fn(&CardDef) -> bool,
) -> Vec<EffectObjectBinding> {
    state.players[player.index()]
        .library
        .iter()
        .copied()
        .filter(|&card| definition(state, card).is_some_and(&keep))
        .map(|card| bind(state, card))
        .collect()
}

fn shuffle(state: &mut GameState, player: PlayerId) -> Result<(), String> {
    state
        .shuffle_library(player)
        .map_err(|error| format!("{error:?}"))
}

/// Puts `card` onto the battlefield under `controller`'s control.
fn put_onto_battlefield(state: &mut GameState, card: ObjectId, controller: PlayerId) {
    event::propose_and_commit(
        state,
        ProposedEvent::zone_change_to_battlefield_under_controller(card, controller),
    );
}

// ---- Programs named by the card database -------------------------------------------

/// Blast Zone: "Put X charge counters on this land", one program per X.
pub fn blast_zone_charge_0() -> EffectOp {
    e7a(E7aOpV1::AddChargeCountersV1 { count: 0 })
}
pub fn blast_zone_charge_1() -> EffectOp {
    e7a(E7aOpV1::AddChargeCountersV1 { count: 1 })
}
pub fn blast_zone_charge_2() -> EffectOp {
    e7a(E7aOpV1::AddChargeCountersV1 { count: 2 })
}
pub fn blast_zone_charge_3() -> EffectOp {
    e7a(E7aOpV1::AddChargeCountersV1 { count: 3 })
}
pub fn blast_zone_charge_4() -> EffectOp {
    e7a(E7aOpV1::AddChargeCountersV1 { count: 4 })
}
pub fn blast_zone_charge_5() -> EffectOp {
    e7a(E7aOpV1::AddChargeCountersV1 { count: 5 })
}
pub fn blast_zone_charge_6() -> EffectOp {
    e7a(E7aOpV1::AddChargeCountersV1 { count: 6 })
}
pub fn blast_zone_charge_7() -> EffectOp {
    e7a(E7aOpV1::AddChargeCountersV1 { count: 7 })
}

/// Blast Zone: "Destroy each nonland permanent with mana value equal to the
/// number of charge counters on this land."
pub fn blast_zone_destroy() -> EffectOp {
    e7a(E7aOpV1::BlastZoneDestroyV1)
}

/// Dark Depths: "Remove an ice counter from Dark Depths."
pub fn dark_depths_remove_ice() -> EffectOp {
    e7a(E7aOpV1::RemoveIceCounterV1)
}

/// Ghost Quarter: "Destroy target land. Its controller may search their
/// library for a basic land card, put it onto the battlefield, then
/// shuffle."
pub fn ghost_quarter() -> EffectOp {
    e7a(E7aOpV1::GhostQuarterV1)
}

/// Maze of Ith: "Untap target attacking creature. Prevent all combat damage
/// that would be dealt to and dealt by that creature this turn."
pub fn maze_of_ith() -> EffectOp {
    e7a(E7aOpV1::MazeOfIthV1)
}

/// Thespian's Stage: "This land becomes a copy of target land, except it
/// has this ability."
pub fn thespians_stage() -> EffectOp {
    e7a(E7aOpV1::ThespiansStageV1)
}

/// Goblin Engineer: "Return target artifact card with mana value 3 or less
/// from your graveyard to the battlefield."
pub fn goblin_engineer_return() -> EffectOp {
    e7a(E7aOpV1::EngineerReturnV1)
}

/// Goblin Welder's ability.
pub fn goblin_welder() -> EffectOp {
    e7a(E7aOpV1::GoblinWelderV1)
}

/// Emry, Lurker of the Loch: "Choose target artifact card in your
/// graveyard. You may cast that card this turn."
pub fn emry_choose_artifact() -> EffectOp {
    e7a(E7aOpV1::EmryPermissionV1)
}

/// Shadowspear: "Permanents your opponents control lose hexproof and
/// indestructible until end of turn."
pub fn shadowspear_strip() -> EffectOp {
    e7a(E7aOpV1::ShadowspearStripV1)
}

/// Curie, Emergent Intelligence's copy ability.
pub fn curie_copy() -> EffectOp {
    e7a(E7aOpV1::CurieCopyV1)
}

/// Arena of Glory's exert mana ability, performed without the stack.
pub fn arena_exert() -> EffectOp {
    e7a(E7aOpV1::SpecialActionV1)
}

/// Gaea's Will's suspend special action, performed without the stack.
pub fn suspend_action() -> EffectOp {
    e7a(E7aOpV1::SpecialActionV1)
}

/// Animate Dead: the Aura enters enchanting its target card; its entry
/// trigger does the rest.
pub fn animate_dead() -> EffectOp {
    e7a(E7aOpV1::AnimateDeadEnterV1)
}

/// Burning Wish: "You may reveal a sorcery card you own from outside the
/// game and put it into your hand. Exile Burning Wish." A best-of-one game
/// has no cards outside the game, so it only exiles itself.
pub fn burning_wish() -> EffectOp {
    e7a(E7aOpV1::BurningWishV1)
}

/// Cabal Therapy: "Choose a nonland card name. Target player reveals their
/// hand and discards all cards with that name." The name is chosen by first
/// letter, then from the names with that letter.
pub fn cabal_therapy() -> EffectOp {
    EffectOp::Choice {
        controller: PlayerRef::Controller,
        options: nonland_names()
            .iter()
            .map(|ids| EffectOp::Choice {
                controller: PlayerRef::Controller,
                options: ids
                    .iter()
                    .map(|&named| e7a(E7aOpV1::TherapyV1 { named }))
                    .collect(),
            })
            .collect(),
    }
}

/// Every nonland card name, once, grouped by first letter.
fn nonland_names() -> &'static [Vec<u16>] {
    static TABLE: std::sync::OnceLock<Vec<Vec<u16>>> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        let mut seen: Vec<&str> = Vec::new();
        let mut groups: Vec<(char, Vec<u16>)> = Vec::new();
        for (index, def) in CARD_DEFS.iter().enumerate() {
            if def.is_token
                || def.is_land
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

/// Echo of Eons: "Each player shuffles their hand and graveyard into their
/// library, then draws seven cards."
pub fn echo_of_eons() -> EffectOp {
    e7a(E7aOpV1::EchoOfEonsV1)
}

/// Gaea's Will: "Until end of turn, you may play lands and cast spells from
/// your graveyard. If a card would be put into your graveyard from anywhere
/// this turn, exile that card instead." Yawgmoth's Will's effect.
pub fn gaeas_will() -> EffectOp {
    crate::eternal_e5a_v1::yawgmoths_will()
}

/// Galvanic Relay: "Exile the top card of your library. During your next
/// turn, you may play that card." Storm is its cast trigger.
pub fn galvanic_relay() -> EffectOp {
    e7a(E7aOpV1::GalvanicRelayV1)
}

/// Gamble: "Search your library for a card, put that card into your hand,
/// discard a card at random, then shuffle."
pub fn gamble() -> EffectOp {
    e7a(E7aOpV1::GambleV1)
}

/// Life from the Loam: "Return up to three target land cards from your
/// graveyard to your hand." Its dredge 3 is `eternal_vintage_e7b_v1::dredge`.
pub fn life_from_the_loam() -> EffectOp {
    EffectOp::MoveAllTargets {
        to_zone: Zone::Hand,
    }
}

/// Reanimate: "Put target creature card from a graveyard onto the
/// battlefield under your control. You lose life equal to its mana value."
pub fn reanimate() -> EffectOp {
    e7a(E7aOpV1::ReanimateV1)
}

/// Shallow Grave: "Return the top creature card of your graveyard to the
/// battlefield. That creature gains haste until end of turn. Exile it at
/// the beginning of the next end step."
pub fn shallow_grave() -> EffectOp {
    e7a(E7aOpV1::ShallowGraveV1)
}

/// Unmask: "Target player reveals their hand. You choose a nonland card
/// from it. That player discards that card." Its alternative cost exiles a
/// black card from its caster's hand.
pub fn unmask() -> EffectOp {
    EffectOp::RevealTargetHandChooseNonlandDiscard {
        player: PlayerRef::Target(0),
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

const fn e7a_trigger(condition: E7aTriggerV1, effect: fn() -> EffectOp) -> TriggeredAbilityDef {
    trigger(
        TriggerCondition::StandardV1(StandardTriggerV1::EternalV1(
            crate::eternal_cards_v1::EternalTriggerV1::E7aV1(condition),
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

fn draw_two() -> EffectOp {
    EffectOp::DrawCards {
        player: PlayerRef::Controller,
        count: 2,
    }
}

fn mill_one() -> EffectOp {
    EffectOp::MillCards {
        player: PlayerRef::Controller,
        count: 1,
    }
}

fn mill_four() -> EffectOp {
    EffectOp::MillCards {
        player: PlayerRef::Controller,
        count: 4,
    }
}

/// Archon of Cruelty: "target opponent sacrifices a creature or
/// planeswalker of their choice, discards a card, and loses 3 life. You draw
/// a card and gain 3 life." A root sequence, so its discard can suspend.
fn archon_trigger() -> EffectOp {
    EffectOp::Sequence(vec![
        e7a(E7aOpV1::ArchonV1),
        EffectOp::DiscardCards {
            player: PlayerRef::Target(0),
            count: 1,
        },
        EffectOp::LoseLife {
            player: PlayerRef::Target(0),
            amount: 3,
        },
        EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 1,
        },
        EffectOp::GainLife {
            player: PlayerRef::Controller,
            amount: 3,
        },
    ])
}

fn animate_dead_return() -> EffectOp {
    e7a(E7aOpV1::AnimateDeadReturnV1)
}

fn animate_dead_leaves() -> EffectOp {
    e7a(E7aOpV1::AnimateDeadLeavesV1)
}

fn engineer_search() -> EffectOp {
    e7a(E7aOpV1::EngineerSearchV1)
}

/// Kappa Cannoneer: "put a +1/+1 counter on this creature. It can't be
/// blocked this turn."
fn kappa_counter() -> EffectOp {
    e7a(E7aOpV1::KappaV1)
}

/// Legion Extruder: "it deals 2 damage to any target."
fn extruder_damage() -> EffectOp {
    EffectOp::DealDamage {
        target: TargetRef::Target(0),
        amount: 2,
    }
}

/// Patchwork Automaton: "put a +1/+1 counter on this creature."
fn patchwork_counter() -> EffectOp {
    EffectOp::AddPlusOneCounterToAbilitySource
}

/// Pinnacle Emissary: "create a 1/1 colorless Drone artifact creature token
/// with flying and 'This token can block only creatures with flying.'"
fn create_drone() -> EffectOp {
    EffectOp::CreateToken {
        token_def: token_id(DRONE_TOKEN),
        controller: PlayerRef::Controller,
    }
}

/// Phyrexian Dragon Engine: "you may discard your hand. If you do, draw
/// three cards."
fn dragon_engine_rummage() -> EffectOp {
    EffectOp::Choice {
        controller: PlayerRef::Controller,
        options: vec![e7a(E7aOpV1::DiscardHandThenDrawV1 { count: 3 }), nothing()],
    }
}

fn dreadnought_entry() -> EffectOp {
    e7a(E7aOpV1::DreadnoughtV1)
}

/// Warp's and unearth's "exile it at the beginning of the next end step"
/// (the engine's shared machinery).
fn next_end_step_exile() -> EffectOp {
    EffectOp::BindWarpExileToTriggerSource
}

fn skateboard_tap() -> EffectOp {
    e7a(E7aOpV1::TapTargetV1)
}

fn discard_hand() -> EffectOp {
    e7a(E7aOpV1::DiscardHandV1)
}

fn manifest_dread() -> EffectOp {
    e7a(E7aOpV1::ManifestDreadV1)
}

const ABHORRENT_OCULUS_TRIGGERS: [TriggeredAbilityDef; 1] =
    [e7a_trigger(E7aTriggerV1::OpponentUpkeepV1, manifest_dread)];
const ANIMATE_DEAD_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(TriggerCondition::Etb, animate_dead_return),
    trigger(TriggerCondition::LeftBattlefield, animate_dead_leaves),
];
const ARCHON_OF_CRUELTY_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(TriggerCondition::Etb, archon_trigger),
    trigger(TriggerCondition::Attacks, archon_trigger),
];
const CHROMATIC_STAR_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    home_zone: Zone::Graveyard,
    ..trigger(TriggerCondition::LeftBattlefieldToGraveyard, draw_one)
}];
const DRAW_ONE_ETB_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(TriggerCondition::Etb, draw_one)];
const EMRY_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(TriggerCondition::Etb, mill_four)];
const GOBLIN_ENGINEER_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, engineer_search)];
const ICETILL_EXPLORER_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::ControlledLandEnters, mill_one)];
const KAPPA_CANNONEER_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(
    TriggerCondition::ControlledArtifactEnters,
    kappa_counter,
)];
const LEGION_EXTRUDER_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, extruder_damage)];
const PATCHWORK_AUTOMATON_TRIGGERS: [TriggeredAbilityDef; 1] = [e7a_trigger(
    E7aTriggerV1::ControllerCastsArtifactSpellV1,
    patchwork_counter,
)];
const PHYREXIAN_DRAGON_ENGINE_TRIGGERS: [TriggeredAbilityDef; 2] = [
    e7a_trigger(
        E7aTriggerV1::EntersFromYourGraveyardV1,
        dragon_engine_rummage,
    ),
    trigger(
        TriggerCondition::BeginningEndStepAfterUnearth,
        next_end_step_exile,
    ),
];
const PHYREXIAN_DREADNOUGHT_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, dreadnought_entry)];
const PINNACLE_EMISSARY_TRIGGERS: [TriggeredAbilityDef; 2] = [
    e7a_trigger(E7aTriggerV1::ControllerCastsArtifactSpellV1, create_drone),
    trigger(
        TriggerCondition::BeginningEndStepAfterWarp,
        next_end_step_exile,
    ),
];
const SKATEBOARD_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, skateboard_tap)];
const SONG_OF_CREATION_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(TriggerCondition::CastSpell, draw_two),
    trigger(TriggerCondition::BeginningControllerEndStep, discard_hand),
];

/// Triggered abilities of this module's cards.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        ABHORRENT_OCULUS => &ABHORRENT_OCULUS_TRIGGERS,
        ANIMATE_DEAD => &ANIMATE_DEAD_TRIGGERS,
        ARCHON_OF_CRUELTY => &ARCHON_OF_CRUELTY_TRIGGERS,
        CHROMATIC_STAR => &CHROMATIC_STAR_TRIGGERS,
        DRESS_DOWN => &DRAW_ONE_ETB_TRIGGERS,
        EMRY => &EMRY_TRIGGERS,
        GOBLIN_ENGINEER => &GOBLIN_ENGINEER_TRIGGERS,
        ICETILL_EXPLORER => &ICETILL_EXPLORER_TRIGGERS,
        KAPPA_CANNONEER => &KAPPA_CANNONEER_TRIGGERS,
        LEGION_EXTRUDER => &LEGION_EXTRUDER_TRIGGERS,
        PATCHWORK_AUTOMATON => &PATCHWORK_AUTOMATON_TRIGGERS,
        PHYREXIAN_DRAGON_ENGINE => &PHYREXIAN_DRAGON_ENGINE_TRIGGERS,
        PHYREXIAN_DREADNOUGHT => &PHYREXIAN_DREADNOUGHT_TRIGGERS,
        PINNACLE_EMISSARY => &PINNACLE_EMISSARY_TRIGGERS,
        SKATEBOARD => &SKATEBOARD_TRIGGERS,
        SONG_OF_CREATION => &SONG_OF_CREATION_TRIGGERS,
        _ => crate::eternal_pauper_e8b_v1::triggers_for(name),
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
    // `triggers_for` falls through to E8b's cards, so they answer first.
    if let Some(spec) = crate::eternal_pauper_e8b_v1::trigger_target_spec(name, effect) {
        return Some(spec);
    }
    if !own_trigger_effect(name, effect) {
        return None;
    }
    Some(match name {
        ARCHON_OF_CRUELTY if *effect == archon_trigger() => TargetSpec::TargetOpponent,
        LEGION_EXTRUDER if *effect == extruder_damage() => TargetSpec::AnyTarget,
        SKATEBOARD if *effect == skateboard_tap() => TargetSpec::AnyPermanent,
        _ => TargetSpec::None,
    })
}

/// Queued and delayed triggers owned by this module's cards. The
/// Tabernacle's granted trigger and Curie's kept trigger belong to whatever
/// creature has them.
pub(crate) fn delayed_effect_matches(name: &str, effect: &EffectOp) -> bool {
    if crate::eternal_pauper_e8b_v1::delayed_effect_matches(name, effect) {
        return true;
    }
    if name == DRESS_DOWN && *effect == sacrifice_source() {
        return true;
    }
    let EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::E7aV1(op))) = effect else {
        return false;
    };
    match op {
        E7aOpV1::TabernacleV1 | E7aOpV1::CurieDrawV1 { .. } => true,
        E7aOpV1::DarkDepthsV1 { .. } => name == DARK_DEPTHS,
        E7aOpV1::ShallowGraveExileV1 { .. } => name == SHALLOW_GRAVE,
        E7aOpV1::SuspendTickV1 { .. } => name == GAEAS_WILL,
        _ => false,
    }
}

/// The triggers this module puts on the stack for whatever creature has
/// them: The Tabernacle's granted upkeep payment and Curie's kept draw.
pub(crate) fn granted_trigger_effect(effect: &EffectOp) -> bool {
    matches!(
        effect,
        EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::E7aV1(
            E7aOpV1::TabernacleV1 | E7aOpV1::CurieDrawV1 { .. }
        )))
    )
}

/// Whether `condition` triggers on its source's arrival on the battlefield.
pub(crate) fn arrival_condition(condition: E7aTriggerV1) -> bool {
    condition == E7aTriggerV1::EntersFromYourGraveyardV1
}

pub(crate) fn trigger_matches(
    condition: E7aTriggerV1,
    events: &[CommittedEvent],
    index: usize,
    source: ObjectId,
    state: &GameState,
) -> bool {
    let controller = state.objects.get(source).controller;
    match condition {
        E7aTriggerV1::ControllerCastsArtifactSpellV1 => {
            let CommittedEvent::SpellCast {
                spell,
                controller: caster,
            } = events[index]
            else {
                return false;
            };
            caster == controller
                && state.stack.iter().any(|item| {
                    item.kind == crate::state::StackItemKind::Spell
                        && item.source == spell
                        && crate::engine::stack_spell_has_type(state, item, CardType::Artifact)
                })
        }
        E7aTriggerV1::EntersFromYourGraveyardV1 => {
            matches!(
                events[index],
                CommittedEvent::ZoneChange {
                    object,
                    from: Zone::Graveyard,
                    to: Zone::Battlefield,
                    ..
                } if object == source
            ) && state.objects.get(source).owner == controller
        }
        E7aTriggerV1::OpponentUpkeepV1 => {
            matches!(events[index], CommittedEvent::UpkeepBegan { player } if player != controller)
        }
    }
}

// ---- Effect leaves ------------------------------------------------------------------

pub(crate) fn execute(op: &E7aOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        E7aOpV1::AddChargeCountersV1 { count } => {
            if let Some(source) = live_ability_source(ctx, state) {
                let counters = &mut state.objects.get_mut(source).counters.charge;
                *counters = counters.saturating_add(i32::from(*count));
            }
        }
        E7aOpV1::BlastZoneDestroyV1 => {
            let Some(contract) = ctx.ability_source_contract else {
                return;
            };
            let charge = state
                .counter_lki_for(contract.source, contract.zone_change_count)
                .map_or(0, |counters| counters.charge);
            let Ok(value) = u16::try_from(charge) else {
                return;
            };
            let doomed = battlefield(state)
                .into_iter()
                .filter(|&object| {
                    !crate::engine::object_has_type(state, object, CardType::Land)
                        && crate::engine::object_mana_value(state, object) == value
                        && !crate::engine::has_effective_keyword(
                            state,
                            object,
                            Keywords::INDESTRUCTIBLE,
                        )
                })
                .collect::<Vec<_>>();
            if !doomed.is_empty() {
                event::propose_and_commit_batch(
                    state,
                    doomed
                        .into_iter()
                        .map(|object| ProposedEvent::zone_change(object, Zone::Graveyard))
                        .collect(),
                );
            }
        }
        E7aOpV1::RemoveIceCounterV1 => {
            let Some(source) = live_ability_source(ctx, state) else {
                return;
            };
            let link = live_link(state, source);
            if let Some(entry) = e7a_state(state)
                .ice_counters
                .iter_mut()
                .find(|(host, _)| *host == link)
            {
                entry.1 = entry.1.saturating_sub(1);
            }
        }
        E7aOpV1::DarkDepthsV1 { depths } => {
            if !link_live_in(state, *depths, Zone::Battlefield)
                || state.objects.get(depths.object).controller != ctx.controller
            {
                return;
            }
            sacrifice_all(state, &[depths.object]);
            if state.objects.get(depths.object).zone != Zone::Battlefield {
                crate::effect::execute(
                    &EffectOp::CreateToken {
                        token_def: token_id(MARIT_LAGE_TOKEN),
                        controller: PlayerRef::Controller,
                    },
                    ctx,
                    state,
                );
            }
        }
        E7aOpV1::MazeOfIthV1 => {
            let Some(creature) = legal_object_target(ctx, state, 0, TargetSpec::AttackingCreature)
            else {
                return;
            };
            crate::engine::attempt_untap(state, creature);
            let record = (live_link(state, creature), TurnV1::now(state));
            let mazed = &mut e7a_state(state).mazed;
            if !mazed.contains(&record) {
                mazed.push(record);
            }
        }
        E7aOpV1::ThespiansStageV1 => {
            let Some(stage) = live_ability_source(ctx, state) else {
                return;
            };
            let Some(land) = legal_object_target(ctx, state, 0, TargetSpec::Land) else {
                return;
            };
            let copied = state.objects.get(land).card_def;
            let face = state.objects.get(land).v4.face_index;
            become_copy(state, stage, copied, face);
            let host = live_link(state, stage);
            let hosts = &mut e7a_state(state).stage_hosts;
            if !hosts.contains(&host) {
                hosts.push(host);
            }
        }
        E7aOpV1::AnimateDeadEnterV1 => {
            let Some(card) = bound_target_in(ctx, state, 0, Zone::Graveyard) else {
                return;
            };
            if state
                .objects
                .try_get(ctx.source)
                .is_none_or(|live| live.zone != Zone::Stack)
            {
                return;
            }
            let card = live_link(state, card);
            // The Aura enters enchanting the card in the graveyard; its
            // entry trigger returns the card.
            crate::effect::execute(
                &EffectOp::MoveObject {
                    object: ObjectRef::ThisSource,
                    to_zone: Zone::Battlefield,
                },
                ctx,
                state,
            );
            if state.objects.get(ctx.source).zone == Zone::Battlefield {
                let aura = live_link(state, ctx.source);
                e7a_state(state).animating.push((aura, card));
            }
        }
        E7aOpV1::AnimateDeadReturnV1 => {
            let Some(aura) = live_ability_source(ctx, state) else {
                return;
            };
            let aura_link = live_link(state, aura);
            let Some(card) = read_state(state).and_then(|e7a| {
                e7a.animating
                    .iter()
                    .find(|(host, _)| *host == aura_link)
                    .map(|(_, card)| *card)
            }) else {
                return;
            };
            e7a_state(state)
                .animating
                .retain(|(host, _)| *host != aura_link);
            if !link_live_in(state, card, Zone::Graveyard)
                || !definition(state, card.object)
                    .is_some_and(|def| def.has_type(CardType::Creature))
            {
                return;
            }
            put_onto_battlefield(state, card.object, ctx.controller);
            if state.objects.get(card.object).zone != Zone::Battlefield {
                return;
            }
            let creature = live_link(state, card.object);
            if state
                .attach_object_exact(
                    aura,
                    aura_link.zone_change_count,
                    creature.object,
                    creature.zone_change_count,
                )
                .is_err()
            {
                halt(state, aura);
                return;
            }
            e7a_state(state).animated.push((aura_link, creature));
        }
        E7aOpV1::AnimateDeadLeavesV1 => {
            let Some(contract) = ctx.ability_source_contract else {
                return;
            };
            let aura = ObjectLinkV4 {
                object: contract.source,
                zone_change_count: contract.zone_change_count,
            };
            let Some(creature) = read_state(state).and_then(|e7a| {
                e7a.animated
                    .iter()
                    .find(|(host, _)| *host == aura)
                    .map(|(_, creature)| *creature)
            }) else {
                return;
            };
            e7a_state(state).animated.retain(|(host, _)| *host != aura);
            if link_live_in(state, creature, Zone::Battlefield) {
                sacrifice_all(state, &[creature.object]);
            }
        }
        E7aOpV1::ReanimateV1 => {
            let spec = TargetSpec::StandardV1(
                crate::standard_cards_v1::StandardTargetV1::CreatureCardInAGraveyardV1,
            );
            let Some(card) = legal_object_target(ctx, state, 0, spec) else {
                return;
            };
            let life = i32::from(crate::engine::object_mana_value(state, card));
            put_onto_battlefield(state, card, ctx.controller);
            if life > 0 {
                crate::effect::execute(
                    &EffectOp::LoseLife {
                        player: PlayerRef::Controller,
                        amount: life,
                    },
                    ctx,
                    state,
                );
            }
        }
        E7aOpV1::ShallowGraveV1 => {
            // The top of a graveyard is its most recently added card.
            let Some(card) = state.players[ctx.controller.index()]
                .graveyard
                .iter()
                .rev()
                .copied()
                .find(|&card| {
                    let live = state.objects.get(card);
                    live.owner == ctx.controller
                        && !live.v4.is_token
                        && definition(state, card)
                            .is_some_and(|def| def.has_type(CardType::Creature))
                })
            else {
                return;
            };
            put_onto_battlefield(state, card, ctx.controller);
            if state.objects.get(card).zone != Zone::Battlefield {
                return;
            }
            let creature = bind(state, card);
            crate::effect::install_temporary_boost(state, creature, 0, 0, Keywords::HASTE);
            let source = AbilitySourceContractV4::capture(state, ctx.source);
            let record = DelayedExileV1 {
                source,
                controller: ctx.controller,
                creature: live_link(state, card),
            };
            e7a_state(state).shallow_graves.push(record);
        }
        E7aOpV1::ShallowGraveExileV1 { creature } => {
            if link_live_in(state, *creature, Zone::Battlefield) {
                event::propose_and_commit(
                    state,
                    ProposedEvent::zone_change(creature.object, Zone::Exile),
                );
            }
        }
        E7aOpV1::TherapyV1 { named } => therapy(ctx, state, *named),
        E7aOpV1::EchoOfEonsV1 => echo_of_eons_effect(ctx, state),
        E7aOpV1::GalvanicRelayV1 => galvanic_relay_effect(ctx, state),
        E7aOpV1::BurningWishV1 => mark_self_exile(ctx, state),
        E7aOpV1::DiscardHandV1 => {
            let hand = state.players[ctx.controller.index()].hand.clone();
            for card in hand {
                crate::engine::commit_discarded_card(state, card);
            }
        }
        E7aOpV1::DiscardHandThenDrawV1 { count } => {
            let hand = state.players[ctx.controller.index()].hand.clone();
            for card in hand {
                crate::engine::commit_discarded_card(state, card);
            }
            crate::effect::execute(
                &EffectOp::DrawCards {
                    player: PlayerRef::Controller,
                    count: u32::from(*count),
                },
                ctx,
                state,
            );
        }
        E7aOpV1::EngineerReturnV1 => {
            let spec = TargetSpec::StandardV1(
                crate::standard_cards_v1::StandardTargetV1::ArtifactCardInOwnGraveyardManaValueAtMostV1(3),
            );
            if let Some(card) = legal_object_target(ctx, state, 0, spec) {
                let owner = state.objects.get(card).owner;
                put_onto_battlefield(state, card, owner);
            }
        }
        E7aOpV1::GoblinWelderV1 => goblin_welder_effect(ctx, state),
        E7aOpV1::EmryPermissionV1 => {
            let spec = TargetSpec::StandardV1(
                crate::standard_cards_v1::StandardTargetV1::ArtifactCardInOwnGraveyardV1,
            );
            if let Some(card) = legal_object_target(ctx, state, 0, spec) {
                let record = (live_link(state, card), ctx.controller, TurnV1::now(state));
                e7a_state(state).emry_permissions.push(record);
            }
        }
        E7aOpV1::KappaV1 => {
            if live_ability_source(ctx, state).is_none() {
                return;
            }
            crate::effect::execute(&EffectOp::AddPlusOneCounterToAbilitySource, ctx, state);
            crate::effect::execute(
                &EffectOp::GrantKeywordTargetUntilEndOfTurn {
                    object: ObjectRef::ThisSource,
                    keyword: Keywords::CANT_BE_BLOCKED,
                },
                ctx,
                state,
            );
        }
        E7aOpV1::CurieCopyV1 => {
            let Some(curie) = live_ability_source(ctx, state) else {
                return;
            };
            let Some(exiled) = ctx.paid_cost_refs.first() else {
                return;
            };
            let copied = exiled.card_def;
            if !CARD_DEFS
                .get(usize::from(copied))
                .is_some_and(|def| def.has_type(CardType::Creature))
            {
                return;
            }
            become_copy(state, curie, copied, 0);
        }
        E7aOpV1::CurieDrawV1 { fallback } => {
            let count = live_ability_source(ctx, state)
                .and_then(|source| crate::engine::effective_base_power(state, source))
                .map_or(*fallback, |power| u32::try_from(power.max(0)).unwrap_or(0));
            if count > 0 {
                crate::effect::execute(
                    &EffectOp::DrawCards {
                        player: PlayerRef::Controller,
                        count,
                    },
                    ctx,
                    state,
                );
            }
        }
        E7aOpV1::ShadowspearStripV1 => {
            let turn = TurnV1::now(state);
            let stripped = controlled_permanents(state, ctx.controller.opponent())
                .into_iter()
                .map(|object| (live_link(state, object), turn))
                .collect::<Vec<_>>();
            let records = &mut e7a_state(state).stripped;
            for record in stripped {
                if !records.contains(&record) {
                    records.push(record);
                }
            }
        }
        E7aOpV1::TapTargetV1 => {
            if let Some(object) = bound_target_in(ctx, state, 0, Zone::Battlefield) {
                event::propose_and_commit(state, ProposedEvent::tap(object));
            }
        }
        E7aOpV1::TabernaclePayV1 => {
            if !crate::engine::pay_effect_mana(ctx.controller, &[], 1, state) {
                // The option is offered only while the payment is possible.
                halt(state, ctx.source);
            }
        }
        E7aOpV1::TabernacleDestroyV1 => {
            if let Some(creature) = live_ability_source(ctx, state) {
                destroy(state, creature);
            }
        }
        // Choices run in `drive`; the plain executor never reaches them.
        E7aOpV1::GhostQuarterV1
        | E7aOpV1::GhostQuarterSearchV1 { .. }
        | E7aOpV1::ArchonV1
        | E7aOpV1::GambleV1
        | E7aOpV1::EngineerSearchV1
        | E7aOpV1::DreadnoughtV1
        | E7aOpV1::TabernacleV1
        | E7aOpV1::SuspendTickV1 { .. }
        | E7aOpV1::SuspendCastV1 { .. }
        | E7aOpV1::ManifestDreadV1 => halt(state, ctx.source),
        E7aOpV1::SpecialActionV1 => {}
    }
}

/// Cabal Therapy's resolution: the target player reveals their hand, then
/// discards every card with the chosen name.
fn therapy(ctx: &ExecCtx, state: &mut GameState, named: u16) {
    let Some(Target::Player(player)) = ctx.targets.first().copied() else {
        return;
    };
    if !ctx.target_incarnation_matches(0, state) {
        return;
    }
    let Some(name) = CARD_DEFS.get(usize::from(named)).map(|def| def.object_name) else {
        return;
    };
    let hand = state.players[player.index()].hand.clone();
    for &card in &hand {
        if state
            .reveal_hand_card(player.opponent(), player, card)
            .is_err()
        {
            halt(state, ctx.source);
            return;
        }
    }
    for card in hand {
        if definition(state, card).is_some_and(|def| def.object_name == name) {
            crate::engine::commit_discarded_card(state, card);
        }
    }
}

/// Echo of Eons: each player, in turn order, shuffles their hand and
/// graveyard into their library; then each draws seven.
fn echo_of_eons_effect(ctx: &ExecCtx, state: &mut GameState) {
    let first = state.active_player;
    for player in [first, first.opponent()] {
        let cards = state.players[player.index()]
            .hand
            .iter()
            .copied()
            .chain(graveyard_cards(state, player))
            .collect::<Vec<_>>();
        if !cards.is_empty() {
            event::propose_and_commit_batch(
                state,
                cards
                    .into_iter()
                    .map(|card| ProposedEvent::zone_change(card, Zone::Library))
                    .collect(),
            );
        }
        if shuffle(state, player).is_err() {
            halt(state, ctx.source);
            return;
        }
    }
    let (first_ref, second_ref) = if first == ctx.controller {
        (PlayerRef::Controller, PlayerRef::Opponent)
    } else {
        (PlayerRef::Opponent, PlayerRef::Controller)
    };
    for player in [first_ref, second_ref] {
        crate::effect::execute(&EffectOp::DrawCards { player, count: 7 }, ctx, state);
    }
}

/// Galvanic Relay: exile the top card of the controller's library; they may
/// play it during their next turn.
fn galvanic_relay_effect(ctx: &ExecCtx, state: &mut GameState) {
    let player = ctx.controller;
    let Some(&top) = state.players[player.index()].library.first() else {
        return;
    };
    event::propose_and_commit(state, ProposedEvent::zone_change(top, Zone::Exile));
    let live = state.objects.get(top);
    if live.zone != Zone::Exile {
        return;
    }
    let land = CARD_DEFS[live.card_def as usize].is_land;
    let permission = crate::engine::PlayPermission {
        without_mana_cost: crate::engine::FreeCastV1::default(),
        object: top,
        holder: player,
        zone_change_generation: live.zone_change_count,
        play_or_cast: if land {
            crate::engine::PlayOrCast::Play
        } else {
            crate::engine::PlayOrCast::Cast
        },
        expiry: crate::engine::PlayPermissionExpiry::UntilHoldersNextTurn {
            holder_turn_started: false,
        },
    };
    state.engine.exile_play_permissions.push(permission);
    let link = live_link(state, top);
    e7a_state(state).relayed.push(link);
}

/// Burning Wish exiles itself as it resolves: marks its stack incarnation
/// for the departure hook `exiled_instead_of_graveyard`.
fn mark_self_exile(ctx: &ExecCtx, state: &mut GameState) {
    let Some(live) = state.objects.try_get(ctx.source) else {
        return;
    };
    if live.zone != Zone::Stack || live.spell_copy_origin.is_some() {
        return;
    }
    let link = live_link(state, ctx.source);
    let marked = &mut e7a_state(state).self_exiling;
    if !marked.contains(&link) {
        marked.push(link);
    }
}

/// Goblin Welder: if both targets are still legal, that player sacrifices
/// the artifact and returns the artifact card at the same time.
fn goblin_welder_effect(ctx: &ExecCtx, state: &mut GameState) {
    let spec = TargetSpec::StandardV1(
        crate::standard_cards_v1::StandardTargetV1::ArtifactThenArtifactCardInItsControllersGraveyardV1,
    );
    let (Some(artifact), Some(card)) = (
        legal_object_target(ctx, state, 0, spec),
        legal_object_target(ctx, state, 1, spec),
    ) else {
        return;
    };
    let player = state.objects.get(artifact).controller;
    if state.objects.get(card).owner != player {
        return;
    }
    event::log_sacrifice(state, artifact);
    event::propose_and_commit_batch(
        state,
        vec![
            ProposedEvent::zone_change(artifact, Zone::Graveyard),
            ProposedEvent::zone_change_to_battlefield_under_controller(card, player),
        ],
    );
}

/// `host` becomes a copy of `copied` (shown with `face`), keeping its own
/// identity and any copy record for when it leaves.
pub(crate) fn become_copy(state: &mut GameState, host: ObjectId, copied: u16, face: u8) {
    let link = live_link(state, host);
    let recorded =
        read_state(state).is_some_and(|e7a| e7a.copies.iter().any(|copy| copy.host == link));
    let live = state.objects.get_mut(host);
    let own_def = live.card_def;
    if !recorded {
        let record = CopyRecordV1 {
            host: link,
            card_def: live.card_def,
            name: std::mem::take(&mut live.name),
            color_mask: live.v4.effective_color_mask,
            subtype_ids: std::mem::take(&mut live.v4.effective_subtype_ids),
            face_index: live.v4.face_index,
        };
        e7a_state(state).copies.push(record);
    }
    let base = crate::state::ObjectStateV4::from_card_def(copied);
    let live = state.objects.get_mut(host);
    live.card_def = copied;
    live.name = CARD_DEFS[copied as usize].object_name.into();
    live.v4.effective_color_mask = base.effective_color_mask;
    live.v4.effective_subtype_ids = base.effective_subtype_ids;
    live.v4.face_index = face;
    crate::standard_cards_v1::note_copied_definitions(
        state,
        host,
        link.zone_change_count,
        &[own_def, copied],
    );
}

fn copy_index(state: &GameState, object: ObjectId) -> Option<usize> {
    let live = state.objects.try_get(object)?;
    let link = ObjectLinkV4 {
        object,
        zone_change_count: live.zone_change_count,
    };
    read_state(state)?
        .copies
        .iter()
        .position(|copy| copy.host == link)
}

/// Whether `object` is a battlefield permanent that became a copy here.
pub(crate) fn active_copy(state: &GameState, object: ObjectId) -> bool {
    copy_index(state, object).is_some()
}

/// A copy leaving the battlefield stops being a copy first, so it arrives
/// in its new zone as its own card.
pub(crate) fn end_copy_before_departure(state: &mut GameState, object: ObjectId) {
    let Some(index) = copy_index(state, object) else {
        return;
    };
    let record = e7a_state(state).copies.remove(index);
    let live = state.objects.get_mut(object);
    live.card_def = record.card_def;
    live.name = record.name;
    live.v4.effective_color_mask = record.color_mask;
    live.v4.effective_subtype_ids = record.subtype_ids;
    live.v4.face_index = record.face_index;
}

/// Agatha's Soul Cauldron marks the cards it exiles without a linked-exile
/// record. Once that Cauldron leaves the battlefield (sacrificed to Goblin
/// Engineer, swapped by Goblin Welder), the mark points at a past
/// incarnation that nothing reads, so it is dropped.
pub(crate) fn release_unlinked_exiles(state: &mut GameState, departing: ObjectId) {
    let Some(live) = state.objects.try_get(departing) else {
        return;
    };
    if CARD_DEFS
        .get(usize::from(live.card_def))
        .is_none_or(|def| def.name != "Agatha's Soul Cauldron")
    {
        return;
    }
    let link = ObjectLinkV4 {
        object: departing,
        zone_change_count: live.zone_change_count,
    };
    let marked = state
        .exile
        .iter()
        .copied()
        .filter(|&card| {
            state.objects.get(card).v4.exiled_by == Some(link)
                && !state.engine.linked_exile_records.iter().any(|record| {
                    record.exiled == card
                        && record.source.source == departing
                        && record.source.zone_change_count == link.zone_change_count
                })
        })
        .collect::<Vec<_>>();
    for card in marked {
        state.objects.get_mut(card).v4.exiled_by = None;
    }
}

/// The definition a battlefield permanent has as its own card: its copy
/// record's, or its live one.
fn own_definition(state: &GameState, object: ObjectId) -> Option<u16> {
    match copy_index(state, object) {
        Some(index) => read_state(state).map(|e7a| e7a.copies[index].card_def),
        None => state.objects.try_get(object).map(|live| live.card_def),
    }
}

// ---- Resolution choices -------------------------------------------------------------

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
    kind: E7aChoiceV1,
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
            kind: E3ChoiceKindV1::E7aV1(kind),
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

/// A choice among `options` for `controller`, as a frame pushed for the
/// interpreter.
fn push_choice(
    continuation: &mut EffectContinuation,
    controller: PlayerRef,
    options: Vec<EffectOp>,
    path: Vec<u16>,
) {
    continuation.frames.push(EffectFrame::Program {
        op: EffectOp::Choice {
            controller,
            options,
        },
        path,
    });
}

fn push(continuation: &mut EffectContinuation, op: EffectOp, path: &[u16]) {
    continuation.frames.push(EffectFrame::Program {
        op,
        path: path.to_vec(),
    });
}

/// Runs a leaf inside the resumable interpreter. `Ok(true)` when it staged a
/// choice and the continuation must suspend.
pub(crate) fn drive(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    op: &E7aOpV1,
    path: Vec<u16>,
) -> Result<bool, String> {
    let controller = continuation.ctx.controller;
    match op {
        E7aOpV1::GhostQuarterV1 => {
            let Some(land) = legal_object_target(&continuation.ctx, state, 0, TargetSpec::Land)
            else {
                return Ok(false);
            };
            let player = state.objects.get(land).controller;
            destroy(state, land);
            push(
                continuation,
                e7a(E7aOpV1::GhostQuarterSearchV1 { player }),
                &path,
            );
            Ok(false)
        }
        E7aOpV1::GhostQuarterSearchV1 { player } => {
            let cards = library_cards(state, *player, |def| {
                def.is_land && def.supertypes.contains(&crate::card_def::Supertype::Basic)
            });
            if cards.is_empty() {
                // Searching is optional; a player who searches finds
                // nothing and still shuffles.
                return Ok(false);
            }
            offer(
                continuation,
                E7aChoiceV1::GhostQuarterSearchV1,
                *player,
                cards,
                0,
                1,
                path,
            );
            Ok(true)
        }
        E7aOpV1::GambleV1 => {
            let cards = library_cards(state, controller, |_| true);
            if cards.is_empty() {
                gamble_finish(state, controller, None)?;
                return Ok(false);
            }
            offer(
                continuation,
                E7aChoiceV1::GambleSearchV1,
                controller,
                cards,
                1,
                1,
                path,
            );
            Ok(true)
        }
        E7aOpV1::EngineerSearchV1 => {
            let cards = library_cards(state, controller, |def| def.has_type(CardType::Artifact));
            if cards.is_empty() {
                shuffle(state, controller)?;
                return Ok(false);
            }
            offer(
                continuation,
                E7aChoiceV1::EngineerSearchV1,
                controller,
                cards,
                0,
                1,
                path,
            );
            Ok(true)
        }
        E7aOpV1::ArchonV1 => {
            let Some(Target::Player(opponent)) = continuation.ctx.targets.first().copied() else {
                return Ok(false);
            };
            if !continuation.ctx.target_incarnation_matches(0, state) {
                return Ok(false);
            }
            // The sacrifice; the rest of the trigger's sequence follows.
            let candidates = controlled_permanents(state, opponent)
                .into_iter()
                .filter(|&object| {
                    crate::engine::object_has_type(state, object, CardType::Creature)
                        || crate::engine::object_has_type(state, object, CardType::Planeswalker)
                })
                .map(|object| bind(state, object))
                .collect::<Vec<_>>();
            if candidates.is_empty() {
                return Ok(false);
            }
            if candidates.len() == 1 {
                sacrifice_all(state, &[candidates[0].object]);
                return Ok(false);
            }
            offer(
                continuation,
                E7aChoiceV1::ArchonSacrificeV1,
                opponent,
                candidates,
                1,
                1,
                path,
            );
            Ok(true)
        }
        E7aOpV1::DreadnoughtV1 => {
            let Some(dreadnought) = live_ability_source(&continuation.ctx, state) else {
                return Ok(false);
            };
            let creatures = controlled_permanents(state, controller)
                .into_iter()
                .filter(|&object| crate::engine::object_has_type(state, object, CardType::Creature))
                .map(|object| bind(state, object))
                .collect::<Vec<_>>();
            let total = creatures
                .iter()
                .map(|creature| {
                    i64::from(crate::engine::effective_power(state, creature.object).max(0))
                })
                .sum::<i64>();
            if total < 12 {
                // No set of creatures can pay; the Dreadnought is sacrificed.
                sacrifice_all(state, &[dreadnought]);
                return Ok(false);
            }
            let max = u16::try_from(creatures.len()).unwrap_or(u16::MAX);
            let link = live_link(state, dreadnought);
            offer(
                continuation,
                E7aChoiceV1::DreadnoughtV1 { dreadnought: link },
                controller,
                creatures,
                0,
                max,
                path,
            );
            Ok(true)
        }
        E7aOpV1::TabernacleV1 => {
            let Some(creature) = live_ability_source(&continuation.ctx, state) else {
                return Ok(false);
            };
            let mut options = Vec::new();
            if crate::engine::can_pay_effect_mana(controller, &[], 1, state) {
                options.push(e7a(E7aOpV1::TabernaclePayV1));
            }
            options.push(e7a(E7aOpV1::TabernacleDestroyV1));
            if options.len() == 1 {
                destroy(state, creature);
                return Ok(false);
            }
            push_choice(continuation, PlayerRef::Controller, options, path);
            Ok(false)
        }
        E7aOpV1::SuspendTickV1 { card } => {
            if !link_live_in(state, *card, Zone::Exile) {
                return Ok(false);
            }
            let counters = &mut state.objects.get_mut(card.object).v4.time_counters_v1;
            *counters = counters.saturating_sub(1);
            if *counters > 0 {
                return Ok(false);
            }
            // 702.62a: when the last is removed, its owner may cast it
            // without paying its mana cost.
            let binding = bind(state, card.object);
            if crate::engine::resolution_cast_v1::can_cast_exiled_without_mana(
                state, controller, binding, None,
            ) {
                push_choice(
                    continuation,
                    PlayerRef::Controller,
                    vec![e7a(E7aOpV1::SuspendCastV1 { card: *card }), nothing()],
                    path,
                );
            }
            Ok(false)
        }
        E7aOpV1::SuspendCastV1 { card } => {
            if !link_live_in(state, *card, Zone::Exile) {
                return Ok(false);
            }
            let binding = bind(state, card.object);
            Ok(crate::eternal_legacy_e5b_v1::offer_paid_cast(
                state,
                continuation,
                binding,
                path,
            ))
        }
        E7aOpV1::ManifestDreadV1 => {
            let library = state.players[controller.index()].library.clone();
            let count = library.len().min(2);
            if count == 0 {
                return Ok(false);
            }
            // Only the controller looks.
            state.reveal_library_top(controller, controller, count);
            let looked = library[..count]
                .iter()
                .map(|&card| bind(state, card))
                .collect::<Vec<_>>();
            offer(
                continuation,
                E7aChoiceV1::ManifestDreadV1 {
                    looked: looked.clone(),
                },
                controller,
                looked,
                1,
                1,
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

/// Gamble's end: the found card goes to the hand (hidden), a card at random
/// is discarded, then the library is shuffled.
fn gamble_finish(
    state: &mut GameState,
    player: PlayerId,
    found: Option<EffectObjectBinding>,
) -> Result<(), String> {
    if let Some(card) = found {
        event::propose_and_commit(state, ProposedEvent::zone_change(card.object, Zone::Hand));
    }
    match state.random_hand_card_v1(player) {
        Ok(Some(card)) => crate::engine::commit_discarded_card(state, card),
        Ok(None) => {}
        Err(error) => return Err(format!("Gamble's random discard failed: {error:?}")),
    }
    shuffle(state, player)
}

/// Applies an answered choice of this module.
pub(crate) fn finish_selection(
    state: &mut GameState,
    _continuation: &mut EffectContinuation,
    choice: &E3ChoiceV1,
    kind: &E7aChoiceV1,
    bindings: Vec<EffectObjectBinding>,
) -> Result<bool, String> {
    let player = choice.player;
    match kind {
        E7aChoiceV1::GhostQuarterSearchV1 => {
            if let Some(card) = bindings.first() {
                put_onto_battlefield(state, card.object, player);
            }
            shuffle(state, player)?;
            Ok(false)
        }
        E7aChoiceV1::GambleSearchV1 => {
            gamble_finish(state, player, bindings.first().copied())?;
            Ok(false)
        }
        E7aChoiceV1::EngineerSearchV1 => {
            if let Some(card) = bindings.first() {
                event::propose_and_commit(
                    state,
                    ProposedEvent::zone_change(card.object, Zone::Graveyard),
                );
            }
            shuffle(state, player)?;
            Ok(false)
        }
        E7aChoiceV1::ArchonSacrificeV1 => {
            if let Some(permanent) = bindings.first() {
                sacrifice_all(state, &[permanent.object]);
            }
            Ok(false)
        }
        E7aChoiceV1::DreadnoughtV1 { dreadnought } => {
            let chosen = bindings
                .iter()
                .map(|binding| binding.object)
                .collect::<Vec<_>>();
            let power = chosen
                .iter()
                .map(|&object| i64::from(crate::engine::effective_power(state, object).max(0)))
                .sum::<i64>();
            if power >= 12 {
                sacrifice_all(state, &chosen);
            } else if link_live_in(state, *dreadnought, Zone::Battlefield) {
                // Creatures with less total power can't pay; it is
                // sacrificed.
                sacrifice_all(state, &[dreadnought.object]);
            }
            Ok(false)
        }
        E7aChoiceV1::ManifestDreadV1 { looked } => {
            let Some(chosen) = bindings.first().copied() else {
                return Err("manifest dread needs one card".to_string());
            };
            if looked.iter().any(|card| !binding_live(state, *card)) {
                return Err("manifest dread's cards left the library".to_string());
            }
            manifest(state, player, chosen.object);
            let rest = looked
                .iter()
                .filter(|card| card.object != chosen.object)
                .map(|card| ProposedEvent::zone_change(card.object, Zone::Graveyard))
                .collect::<Vec<_>>();
            if !rest.is_empty() {
                event::propose_and_commit_batch(state, rest);
            }
            Ok(false)
        }
    }
}

/// Puts `card` onto the battlefield face down as a 2/2 creature under
/// `player`'s control; only `player` may look at it.
fn manifest(state: &mut GameState, player: PlayerId, card: ObjectId) {
    put_onto_battlefield(state, card, player);
    if state.objects.get(card).zone != Zone::Battlefield {
        return;
    }
    let live = state.objects.get_mut(card);
    live.v4.face_down_v1 = Some(crate::state::FaceDownV1 {
        disguised: true,
        lookers: 1 << player.index(),
        hidden_by: None,
        hidden_by_source: None,
    });
    live.v4.ward_generic = 0;
    let link = live_link(state, card);
    e7a_state(state).manifested.push(link);
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
    upkeep_triggers(state, events);
    end_step_triggers(state, events);
    curie_triggers(state, events);
    dark_depths_triggers(state);
    prune(state);
    let Some(eternal) = state.eternal_v1.as_mut() else {
        return Vec::new();
    };
    if eternal.e7a.queued_triggers.is_empty() {
        return Vec::new();
    }
    std::mem::take(&mut eternal.e7a.queued_triggers)
        .into_iter()
        .map(|queued| {
            crate::eternal_cards_v1::delayed_trigger(
                queued.source,
                queued.controller,
                e7a(queued.op),
            )
        })
        .collect()
}

/// Battlefield entries this module tracks: Dress Down's timestamp, the
/// haste of creatures paid with Arena of Glory's mana.
fn note_entries(state: &mut GameState, events: &[CommittedEvent]) {
    for event in events {
        let CommittedEvent::ZoneChange {
            object,
            from,
            to: Zone::Battlefield,
            ..
        } = *event
        else {
            continue;
        };
        let Some(live) = state.objects.try_get(object) else {
            continue;
        };
        if live.zone != Zone::Battlefield {
            continue;
        }
        let link = live_link(state, object);
        if definition_name(state, object) == Some(DRESS_DOWN)
            && !read_state(state)
                .is_some_and(|e7a| e7a.dress_downs.iter().any(|(host, _)| *host == link))
        {
            let timestamp = crate::engine::next_timestamp(state);
            e7a_state(state).dress_downs.push((link, timestamp));
        }
        if from == Zone::Stack {
            let Some(stack) = link.zone_change_count.checked_sub(1) else {
                continue;
            };
            let spell = ObjectLinkV4 {
                object,
                zone_change_count: stack,
            };
            let hasted = read_state(state).is_some_and(|e7a| {
                e7a.arena_spells
                    .iter()
                    .any(|(cast, turn)| *cast == spell && turn.current(state))
            });
            if hasted && crate::engine::object_has_type(state, object, CardType::Creature) {
                let creature = bind(state, object);
                crate::effect::install_temporary_boost(state, creature, 0, 0, Keywords::HASTE);
            }
        }
    }
}

/// Upkeep triggers: The Tabernacle's granted ability, suspend's time
/// counters.
fn upkeep_triggers(state: &mut GameState, events: &[CommittedEvent]) {
    for event in events {
        let CommittedEvent::UpkeepBegan { player } = *event else {
            continue;
        };
        let tabernacles = active_permanents(state, TABERNACLE, None).count();
        if tabernacles > 0 {
            let creatures = controlled_permanents(state, player)
                .into_iter()
                .filter(|&object| crate::engine::object_has_type(state, object, CardType::Creature))
                .collect::<Vec<_>>();
            for creature in creatures {
                for _ in 0..tabernacles {
                    let source = AbilitySourceContractV4::capture(state, creature);
                    queue_trigger(state, source, player, E7aOpV1::TabernacleV1);
                }
            }
        }
        let suspended = read_state(state)
            .map(|e7a| e7a.suspended.clone())
            .unwrap_or_default();
        for card in suspended {
            if !link_live_in(state, card, Zone::Exile)
                || state.objects.get(card.object).owner != player
                || state.objects.get(card.object).v4.time_counters_v1 == 0
            {
                continue;
            }
            let source = AbilitySourceContractV4::capture(state, card.object);
            queue_trigger(state, source, player, E7aOpV1::SuspendTickV1 { card });
        }
    }
}

/// "At the beginning of the next end step" (Shallow Grave) and "At the
/// beginning of the end step, sacrifice this enchantment" (Dress Down).
fn end_step_triggers(state: &mut GameState, events: &[CommittedEvent]) {
    if !events
        .iter()
        .any(|event| matches!(event, CommittedEvent::BeginningEndStep { .. }))
    {
        return;
    }
    let graves = state
        .eternal_v1
        .as_mut()
        .map(|eternal| std::mem::take(&mut eternal.e7a.shallow_graves))
        .unwrap_or_default();
    for record in graves {
        // The spell has left the stack: the trigger's source is the card
        // where it is now.
        let Some(source) = spell_delayed_source(state, record.source, record.controller) else {
            continue;
        };
        queue_trigger(
            state,
            source,
            record.controller,
            E7aOpV1::ShallowGraveExileV1 {
                creature: record.creature,
            },
        );
    }
    let dress_downs = active_permanents(state, DRESS_DOWN, None).collect::<Vec<_>>();
    for dress_down in dress_downs {
        let source = AbilitySourceContractV4::capture(state, dress_down);
        let controller = state.objects.get(dress_down).controller;
        let pending =
            crate::eternal_cards_v1::delayed_trigger(source, controller, sacrifice_source());
        state.engine.pending_triggers.push(pending);
    }
}

/// The current source contract of the spell card `recorded` captured on the
/// stack, controlled by `controller`.
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

/// Curie's trigger, which a Curie keeps as a copy: "Whenever this creature
/// deals combat damage to a player, draw cards equal to its base power."
fn curie_triggers(state: &mut GameState, events: &[CommittedEvent]) {
    for event in events {
        let CommittedEvent::CombatDamageToPlayer {
            source,
            source_zone_change_count,
            amount,
            ..
        } = *event
        else {
            continue;
        };
        if amount <= 0 {
            continue;
        }
        let Some(live) = state.objects.try_get(source) else {
            continue;
        };
        if live.zone != Zone::Battlefield
            || live.zone_change_count != source_zone_change_count
            || own_definition(state, source)
                .and_then(|def| CARD_DEFS.get(usize::from(def)))
                .is_none_or(|def| def.name != CURIE)
            || !crate::continuous_characteristics_v1::printed_abilities_active(state, source)
        {
            continue;
        }
        let fallback = crate::engine::effective_base_power(state, source)
            .map_or(0, |power| u32::try_from(power.max(0)).unwrap_or(0));
        let controller = live.controller;
        let contract = AbilitySourceContractV4::capture(state, source);
        queue_trigger(
            state,
            contract,
            controller,
            E7aOpV1::CurieDrawV1 { fallback },
        );
    }
}

/// Dark Depths: "When Dark Depths has no ice counters on it, sacrifice it.
/// If you do, create Marit Lage." A state trigger: it triggers again once
/// it has left the stack while the condition still holds (603.8). It waits
/// with the other pending triggers at once, so a second look in the same
/// collection finds it.
fn dark_depths_triggers(state: &mut GameState) {
    let depths = active_permanents(state, DARK_DEPTHS, None).collect::<Vec<_>>();
    for object in depths {
        let link = live_link(state, object);
        let ice = read_state(state)
            .and_then(|e7a| {
                e7a.ice_counters
                    .iter()
                    .find(|(host, _)| *host == link)
                    .map(|(_, count)| *count)
            })
            .unwrap_or(0);
        if ice > 0 {
            continue;
        }
        let effect = e7a(E7aOpV1::DarkDepthsV1 { depths: link });
        let pending = state
            .stack
            .iter()
            .any(|item| item.inline_effect.as_ref() == Some(&effect))
            || state
                .engine
                .pending_triggers
                .iter()
                .any(|trigger| trigger.effect == effect);
        if pending {
            continue;
        }
        let controller = state.objects.get(object).controller;
        let source = AbilitySourceContractV4::capture(state, object);
        let trigger = crate::eternal_cards_v1::delayed_trigger(source, controller, effect);
        state.engine.pending_triggers.push(trigger);
    }
}

/// Drops records whose objects moved on.
fn prune(state: &mut GameState) {
    let Some(e7a) = read_state(state) else {
        return;
    };
    if e7a.is_empty() {
        return;
    }
    let battlefield_links = |links: &[(ObjectLinkV4, TurnV1)]| {
        links
            .iter()
            .copied()
            .filter(|(link, turn)| {
                link_live_in(state, *link, Zone::Battlefield) && turn.current(state)
            })
            .collect::<Vec<_>>()
    };
    let ice = e7a
        .ice_counters
        .iter()
        .copied()
        .filter(|(link, _)| link_live_in(state, *link, Zone::Battlefield))
        .collect::<Vec<_>>();
    let mazed = battlefield_links(&e7a.mazed);
    let stripped = battlefield_links(&e7a.stripped);
    let dress_downs = e7a
        .dress_downs
        .iter()
        .copied()
        .filter(|(link, _)| link_live_in(state, *link, Zone::Battlefield))
        .collect::<Vec<_>>();
    let animating = e7a
        .animating
        .iter()
        .copied()
        .filter(|(aura, _)| link_live_in(state, *aura, Zone::Battlefield))
        .collect::<Vec<_>>();
    let self_exiling = e7a
        .self_exiling
        .iter()
        .copied()
        .filter(|&link| link_live_in(state, link, Zone::Stack))
        .collect::<Vec<_>>();
    let relayed = e7a
        .relayed
        .iter()
        .copied()
        .filter(|&link| link_live_in(state, link, Zone::Exile))
        .collect::<Vec<_>>();
    let arena_spells = e7a
        .arena_spells
        .iter()
        .copied()
        .filter(|(_, turn)| turn.current(state))
        .collect::<Vec<_>>();
    let suspended = e7a
        .suspended
        .iter()
        .copied()
        .filter(|&link| link_live_in(state, link, Zone::Exile))
        .collect::<Vec<_>>();
    let manifested = e7a
        .manifested
        .iter()
        .copied()
        .filter(|&link| {
            link_live_in(state, link, Zone::Battlefield)
                && state.objects.get(link.object).v4.face_down_v1.is_some()
        })
        .collect::<Vec<_>>();
    let emry = e7a
        .emry_permissions
        .iter()
        .copied()
        .filter(|(_, _, turn)| turn.current(state))
        .collect::<Vec<_>>();
    let mox = e7a
        .mox_entering
        .iter()
        .copied()
        .filter(|&link| link_live_in(state, link, Zone::Battlefield))
        .collect::<Vec<_>>();
    let changed = ice.len() != e7a.ice_counters.len()
        || mazed.len() != e7a.mazed.len()
        || stripped.len() != e7a.stripped.len()
        || dress_downs.len() != e7a.dress_downs.len()
        || animating.len() != e7a.animating.len()
        || self_exiling.len() != e7a.self_exiling.len()
        || relayed.len() != e7a.relayed.len()
        || arena_spells.len() != e7a.arena_spells.len()
        || suspended.len() != e7a.suspended.len()
        || manifested.len() != e7a.manifested.len()
        || emry.len() != e7a.emry_permissions.len()
        || mox.len() != e7a.mox_entering.len();
    if changed {
        let e7a = e7a_state(state);
        e7a.ice_counters = ice;
        e7a.mazed = mazed;
        e7a.stripped = stripped;
        e7a.dress_downs = dress_downs;
        e7a.animating = animating;
        e7a.self_exiling = self_exiling;
        e7a.relayed = relayed;
        e7a.arena_spells = arena_spells;
        e7a.suspended = suspended;
        e7a.manifested = manifested;
        e7a.emry_permissions = emry;
        e7a.mox_entering = mox;
    }
}

/// The beginning of a turn ends nothing this module tracks beyond what
/// `prune` drops by turn key.
pub(crate) fn begin_turn(_state: &mut GameState, _active: PlayerId) {}

/// "Enters with" counters: Dark Depths' ten ice counters, Blast Zone's
/// charge counter, Mox Diamond's discard choice.
pub(crate) fn apply_entry_counters(state: &mut GameState, object: ObjectId) {
    let Some(name) = definition_name(state, object) else {
        return;
    };
    let link = live_link(state, object);
    match name {
        DARK_DEPTHS => {
            let ice = &mut e7a_state(state).ice_counters;
            ice.retain(|(host, _)| host.object != object);
            ice.push((link, DARK_DEPTHS_ICE));
        }
        BLAST_ZONE => {
            let counters = &mut state.objects.get_mut(object).counters.charge;
            *counters = counters.saturating_add(1);
        }
        MOX_DIAMOND => {
            let entering = &mut e7a_state(state).mox_entering;
            if !entering.contains(&link) {
                entering.push(link);
            }
        }
        _ => crate::eternal_pauper_e8b_v1::apply_entry_counters(state, object),
    }
}

// ---- Hooks: engine ------------------------------------------------------------------

/// Exploration, Icetill Explorer and Song of Creation: "You may play an
/// additional land on each of your turns." The number of additional land
/// plays `player` has.
pub(crate) fn additional_land_plays(state: &GameState, player: PlayerId) -> u8 {
    let count = [EXPLORATION, ICETILL_EXPLORER, SONG_OF_CREATION]
        .into_iter()
        .map(|name| active_permanents(state, name, Some(player)).count())
        .sum::<usize>();
    u8::try_from(count).unwrap_or(u8::MAX)
}

/// Icetill Explorer: "You may play lands from your graveyard."
pub(crate) fn extra_land_candidates(state: &GameState, player: PlayerId) -> Vec<ObjectId> {
    if active_permanents(state, ICETILL_EXPLORER, Some(player))
        .next()
        .is_none()
    {
        return Vec::new();
    }
    state.players[player.index()]
        .graveyard
        .iter()
        .copied()
        .filter(|&card| {
            let live = state.objects.get(card);
            live.owner == player
                && !live.v4.is_token
                && CARD_DEFS[live.card_def as usize].is_playable_land()
        })
        .collect()
}

/// Grafdigger's Cage: "Players can't cast spells from graveyards or
/// libraries." Gaea's Will has no mana cost, so it is cast only without
/// paying one: from exile (suspend) or from a hand under a free-cast
/// permission.
pub(crate) fn cast_prohibited(state: &GameState, player: PlayerId, card: ObjectId) -> bool {
    let Some(live) = state.objects.try_get(card) else {
        return false;
    };
    if matches!(live.zone, Zone::Graveyard | Zone::Library) && any_active(state, GRAFDIGGERS_CAGE) {
        return true;
    }
    if definition_name(state, card) == Some(GAEAS_WILL) {
        return match live.zone {
            Zone::Hand => {
                !crate::eternal_cards_v1::e3::hand_free_permission_any_form(state, card, player)
            }
            Zone::Exile => false,
            _ => true,
        };
    }
    false
}

/// Grafdigger's Cage: "Creature cards in graveyards and libraries can't
/// enter the battlefield." Whether `change` is prevented.
pub(crate) fn prevents_zone_change(state: &GameState, change: &event::ZoneChangeProposed) -> bool {
    if change.to_zone != Zone::Battlefield {
        return false;
    }
    let Some(live) = state.objects.try_get(change.object) else {
        return false;
    };
    matches!(live.zone, Zone::Graveyard | Zone::Library)
        && !live.v4.is_token
        && CARD_DEFS
            .get(live.card_def as usize)
            .is_some_and(|def| def.has_type(CardType::Creature))
        && any_active(state, GRAFDIGGERS_CAGE)
}

/// Maze of Ith: whether `damage` is combat damage dealt to or by a creature
/// mazed this turn.
pub(crate) fn prevents_damage(state: &GameState, damage: &DamageProposed) -> bool {
    if !damage.is_combat {
        return false;
    }
    let Some(e7a) = read_state(state) else {
        return false;
    };
    if e7a.mazed.is_empty() {
        return false;
    }
    let mazed = |object: ObjectId| {
        state.objects.try_get(object).is_some_and(|live| {
            e7a.mazed.iter().any(|(link, turn)| {
                link.object == object
                    && link.zone_change_count == live.zone_change_count
                    && turn.current(state)
            })
        })
    };
    mazed(damage.source) || matches!(damage.target, Target::Object(object) if mazed(object))
}

/// Shadowspear: whether `object` lost `keyword` (hexproof or
/// indestructible) this turn.
pub(crate) fn keyword_lost(state: &GameState, object: ObjectId, keyword: Keywords) -> bool {
    if keyword != Keywords::HEXPROOF && keyword != Keywords::INDESTRUCTIBLE {
        return false;
    }
    let Some(e7a) = read_state(state) else {
        return false;
    };
    if e7a.stripped.is_empty() {
        return false;
    }
    state.objects.try_get(object).is_some_and(|live| {
        live.zone == Zone::Battlefield
            && e7a.stripped.iter().any(|(link, turn)| {
                link.object == object
                    && link.zone_change_count == live.zone_change_count
                    && turn.current(state)
            })
    })
}

/// Dress Down: "Creatures lose all abilities." The timestamp of the latest
/// Dress Down whose ability applies, for a creature.
pub(crate) fn removal_timestamp(state: &GameState, object: ObjectId) -> Option<u64> {
    let e7a = read_state(state)?;
    if e7a.dress_downs.is_empty() {
        return None;
    }
    let live = state.objects.try_get(object)?;
    if live.zone != Zone::Battlefield
        || !CARD_DEFS.get(live.card_def as usize).is_some_and(|def| {
            def.types_for_face(live.v4.face_index)
                .contains(&CardType::Creature)
        })
    {
        return None;
    }
    e7a.dress_downs
        .iter()
        .filter(|(link, _)| {
            link.object != object
                && state.objects.try_get(link.object).is_some_and(|dress| {
                    dress.zone == Zone::Battlefield
                        && dress.zone_change_count == link.zone_change_count
                        && dress.v4.face_down_v1.is_none()
                        && CARD_DEFS
                            .get(dress.card_def as usize)
                            .is_some_and(|def| def.name == DRESS_DOWN)
                })
        })
        .map(|(_, timestamp)| *timestamp)
        .max()
}

/// Yavimaya, Cradle of Growth: "Each land is a Forest in addition to its
/// other land types."
pub(crate) fn is_extra_forest(state: &GameState, object: ObjectId) -> bool {
    state.objects.try_get(object).is_some_and(|live| {
        live.zone == Zone::Battlefield
            && live.v4.face_down_v1.is_none()
            && CARD_DEFS.get(live.card_def as usize).is_some_and(|def| {
                def.types_for_face(live.v4.face_index)
                    .contains(&CardType::Land)
            })
    }) && any_active(state, YAVIMAYA)
}

/// Yavimaya: a land that is a Forest only through it has "{T}: Add {G}."
pub(crate) fn granted_mana_colors(state: &GameState, object: ObjectId) -> Vec<ManaColor> {
    if !is_extra_forest(state, object) {
        return Vec::new();
    }
    let live = state.objects.get(object);
    let def = &CARD_DEFS[live.card_def as usize];
    let own_green = def
        .primary_mana_ability_choices(live.v4.chosen_color)
        .contains(&ManaColor::G)
        || def
            .additional_mana_abilities
            .iter()
            .any(|ability| ability.colors.contains(&ManaColor::G))
        || def
            .restricted_mana_abilities
            .iter()
            .any(|ability| ability.colors.contains(&ManaColor::G));
    if own_green {
        return Vec::new();
    }
    vec![ManaColor::G]
}

/// Animate Dead enchants a creature card in a graveyard until its entry
/// trigger returns it: whether the unattached Aura `aura` is still legal.
pub(crate) fn aura_waiting(state: &GameState, aura: ObjectId) -> bool {
    let Some(e7a) = read_state(state) else {
        return false;
    };
    if e7a.animating.is_empty() {
        return false;
    }
    let Some(live) = state.objects.try_get(aura) else {
        return false;
    };
    let link = ObjectLinkV4 {
        object: aura,
        zone_change_count: live.zone_change_count,
    };
    live.zone == Zone::Battlefield
        && live.v4.attached_to.is_none()
        && e7a
            .animating
            .iter()
            .any(|(host, card)| *host == link && link_live_in(state, *card, Zone::Graveyard))
}

/// Drone tokens: "This token can block only creatures with flying."
pub(crate) fn blocker_allowed(state: &GameState, attacker: ObjectId, blocker: ObjectId) -> bool {
    definition_name(state, blocker) != Some(DRONE_TOKEN)
        || !crate::continuous_characteristics_v1::printed_abilities_active(state, blocker)
        || crate::engine::has_effective_keyword(state, attacker, Keywords::FLYING)
}

/// Rebound-style departures: Burning Wish exiles itself as it resolves.
pub(crate) fn exiles_on_departure(state: &GameState, item: &StackItem) -> bool {
    if item.is_copy {
        return false;
    }
    let Some(live) = state.objects.try_get(item.source) else {
        return false;
    };
    let link = ObjectLinkV4 {
        object: item.source,
        zone_change_count: live.zone_change_count,
    };
    live.zone == Zone::Stack
        && read_state(state).is_some_and(|e7a| e7a.self_exiling.contains(&link))
}

/// Galvanic Relay: "During your next turn, you may play that card." The
/// permission waits until its holder's next turn begins.
pub(crate) fn permission_dormant(
    state: &GameState,
    permission: &crate::engine::PlayPermission,
) -> bool {
    matches!(
        permission.expiry,
        crate::engine::PlayPermissionExpiry::UntilHoldersNextTurn {
            holder_turn_started: false
        }
    ) && read_state(state).is_some_and(|e7a| {
        e7a.relayed.contains(&ObjectLinkV4 {
            object: permission.object,
            zone_change_count: permission.zone_change_generation,
        })
    })
}

/// Emry, Lurker of the Loch: "You may cast that card this turn." Whether
/// `holder` may cast `object`'s graveyard incarnation `zone_change_count`
/// this way.
pub(crate) fn emry_permission(
    state: &GameState,
    object: ObjectId,
    holder: PlayerId,
    zone_change_count: u32,
) -> bool {
    read_state(state).is_some_and(|e7a| {
        e7a.emry_permissions.iter().any(|(card, player, turn)| {
            card.object == object
                && card.zone_change_count == zone_change_count
                && *player == holder
                && turn.current(state)
        })
    })
}

/// Curie's activation cost exiles "another nontoken artifact creature you
/// control": the craft-material candidates it allows.
pub(crate) fn craft_material_allowed(
    state: &GameState,
    source: ObjectId,
    candidate: ObjectId,
) -> bool {
    if definition_name(state, source) != Some(CURIE) {
        return true;
    }
    state.objects.try_get(candidate).is_some_and(|live| {
        live.zone == Zone::Battlefield
            && !live.v4.is_token
            && crate::engine::object_has_type(state, candidate, CardType::Artifact)
            && crate::engine::object_has_type(state, candidate, CardType::Creature)
    })
}

/// Kappa Cannoneer's improvise uses the convoke payment with artifacts,
/// which pay only generic mana.
pub(crate) fn improvises(state: &GameState, spell: ObjectId) -> bool {
    definition_name(state, spell).is_some_and(improvise_definition)
}

/// Whether the card named `name` has improvise.
pub(crate) fn improvise_definition(name: &str) -> bool {
    name == KAPPA_CANNONEER
}

/// Whether `id` is a face-down permanent manifest dread put onto the
/// battlefield.
pub(crate) fn manifested(state: &GameState, id: ObjectId) -> bool {
    let Some(e7a) = read_state(state) else {
        return false;
    };
    state.objects.try_get(id).is_some_and(|live| {
        live.zone == Zone::Battlefield
            && live.v4.face_down_v1.is_some()
            && e7a.manifested.contains(&ObjectLinkV4 {
                object: id,
                zone_change_count: live.zone_change_count,
            })
    })
}

/// The cost of turning a manifested creature card face up: its mana cost.
/// `None` when the card is not a creature card (it can't be turned face up
/// this way).
pub(crate) fn turn_face_up_cost(state: &GameState, id: ObjectId) -> Option<crate::mana::Cost> {
    if !manifested(state, id) {
        return None;
    }
    let def = definition(state, id)?;
    def.has_type(CardType::Creature).then_some(def.cost)
}

/// Whether activating `source`'s ability `index` is one this module
/// performs without the stack: Arena of Glory's exert mana ability, Gaea's
/// Will's suspend.
pub(crate) fn is_special_activation(state: &GameState, source: ObjectId, index: u8) -> bool {
    if index != 0 {
        return false;
    }
    let Some(live) = state.objects.try_get(source) else {
        return false;
    };
    match definition_name(state, source) {
        Some(ARENA_OF_GLORY) => live.zone == Zone::Battlefield,
        Some(GAEAS_WILL) => live.zone == Zone::Hand,
        // Simian Spirit Guide (wave E9a).
        _ => crate::eternal_legacy_e9a_v1::statics::is_special_activation(state, source, index),
    }
}

/// Performs a special activation: Arena of Glory's "{R}, {T}, Exert this
/// land: Add {R}{R}" or Gaea's Will's suspend.
pub(crate) fn special_activation(
    state: &mut GameState,
    player: PlayerId,
    source: ObjectId,
) -> Result<(), String> {
    match definition_name(state, source) {
        Some(ARENA_OF_GLORY) => activate_arena(state, player, source),
        Some(GAEAS_WILL) => suspend(state, player, source),
        _ => crate::eternal_legacy_e9a_v1::statics::special_activation(state, player, source),
    }
}

/// Arena of Glory's exert ability: pays {R} from other sources, taps and
/// exerts the land (it won't untap during its controller's next untap
/// step) and adds {R}{R}. The two mana remember their source, so a creature
/// spell paid with them gains haste.
fn activate_arena(state: &mut GameState, player: PlayerId, source: ObjectId) -> Result<(), String> {
    let live = state.objects.get(source);
    if live.zone != Zone::Battlefield || live.controller != player || live.tapped {
        return Err("Arena of Glory can't be exerted now".to_string());
    }
    let cost = crate::mana::Cost {
        pips: &[crate::mana::Pip::Colored(ManaColor::R)],
        generic: 0,
        x_count: 0,
    };
    let plan = crate::mana::can_pay_excluding_source(&cost, 0, player, state, source)
        .ok_or("Arena of Glory's {R} is not payable")?;
    crate::engine::pay_plan(state, player, &plan);
    event::propose_and_commit(state, ProposedEvent::tap(source));
    state.objects.get_mut(source).v4.skip_next_untap = true;
    let unit = crate::mana::RestrictedManaUnitV1 {
        color: ManaColor::R,
        restriction: crate::card_def::ManaSpendRestrictionDef::Unrestricted,
        source: live_link(state, source),
        source_card_def: state.objects.get(source).card_def,
    };
    let seat = &mut state.players[player.index()];
    seat.restricted_mana_pool.0.push(unit.clone());
    seat.restricted_mana_pool.0.push(unit);
    Ok(())
}

/// Records creature spells paid with Arena of Glory's exerted mana: the
/// restricted units the payment used.
pub(crate) fn note_restricted_spending(
    state: &mut GameState,
    player: PlayerId,
    spell: ObjectId,
    used: &[usize],
) {
    let arena = used.iter().any(|&index| {
        state.players[player.index()]
            .restricted_mana_pool
            .0
            .get(index)
            .is_some_and(|unit| {
                CARD_DEFS
                    .get(usize::from(unit.source_card_def))
                    .is_some_and(|def| def.name == ARENA_OF_GLORY)
            })
    });
    if !arena
        || !definition(state, spell).is_some_and(|def| def.has_type(CardType::Creature))
        || state.objects.get(spell).zone != Zone::Stack
    {
        return;
    }
    let record = (live_link(state, spell), TurnV1::now(state));
    e7a_state(state).arena_spells.push(record);
}

/// Suspend (702.62a): exiles `source` from `player`'s hand with four time
/// counters, paying {G}. No stack, no priority pass.
fn suspend(state: &mut GameState, player: PlayerId, source: ObjectId) -> Result<(), String> {
    let live = state.objects.get(source);
    if live.zone != Zone::Hand || live.owner != player {
        return Err("suspend needs the card in its owner's hand".to_string());
    }
    if !crate::engine::pay_effect_mana(player, &[ManaColor::G], 0, state) {
        return Err("suspend's {G} is not payable".to_string());
    }
    event::propose_and_commit(state, ProposedEvent::zone_change(source, Zone::Exile));
    if state.objects.get(source).zone != Zone::Exile {
        return Ok(());
    }
    state.objects.get_mut(source).v4.time_counters_v1 = SUSPEND_TIME_COUNTERS;
    let link = live_link(state, source);
    e7a_state(state).suspended.push(link);
    Ok(())
}

/// Thespian's Stage keeps "{2}, {T}: This land becomes a copy of target
/// land, except it has this ability" while it is a copy: the ability, if
/// `object` is a Stage copy on the battlefield.
pub(crate) fn retained_stage_ability(
    state: &GameState,
    object: ObjectId,
) -> Option<crate::card_def::ActivatedAbilityDef> {
    let index = copy_index(state, object)?;
    let own = read_state(state)?.copies[index].card_def;
    let def = CARD_DEFS.get(usize::from(own))?;
    if def.name != THESPIANS_STAGE {
        return None;
    }
    def.activated_abilities.first().copied()
}

/// The retained ability of a stack item activated by a Stage copy, whose
/// source contract names the copied definition: the ability one past the
/// copied definition's own.
pub(crate) fn retained_stage_ability_on_stack(
    state: &GameState,
    host: AbilitySourceContractV4,
    ability_index: u8,
) -> Option<crate::card_def::ActivatedAbilityDef> {
    let copied = CARD_DEFS.get(usize::from(host.card_def))?;
    if usize::from(ability_index) != copied.activated_abilities.len() {
        return None;
    }
    let link = ObjectLinkV4 {
        object: host.source,
        zone_change_count: host.zone_change_count,
    };
    if !read_state(state)?.stage_hosts.contains(&link) {
        return None;
    }
    let stage = CARD_DEFS.get(usize::from(card_id_by_name(THESPIANS_STAGE)?))?;
    stage.activated_abilities.first().copied()
}

/// Mox Diamond: "If this artifact would enter, you may discard a land card
/// instead. If you do, put this artifact onto the battlefield. If you
/// don't, put it into its owner's graveyard." The choice is made just after
/// it enters: option 0 puts it into the graveyard, option N discards its
/// controller's Nth land card in hand.
pub(crate) fn entry_choice_decision(state: &GameState) -> Option<crate::engine::Decision> {
    let mox = live_mox_entry(state)?;
    let controller = state.objects.get(mox.object).controller;
    let lands = hand_land_cards(state, controller);
    Some(crate::engine::Decision::ChooseEffectOption {
        player: controller,
        source: mox.object,
        option_count: u16::try_from(lands.len() + 1).ok()?,
    })
}

fn live_mox_entry(state: &GameState) -> Option<ObjectLinkV4> {
    read_state(state)?
        .mox_entering
        .iter()
        .copied()
        .find(|&link| link_live_in(state, link, Zone::Battlefield))
}

fn hand_land_cards(state: &GameState, player: PlayerId) -> Vec<ObjectId> {
    state.players[player.index()]
        .hand
        .iter()
        .copied()
        .filter(|&card| definition(state, card).is_some_and(|def| def.has_type(CardType::Land)))
        .collect()
}

/// Answers the pending Mox Diamond choice. `Ok(false)` when none is
/// pending.
pub(crate) fn answer_entry_choice(state: &mut GameState, option: u16) -> Result<bool, String> {
    let Some(mox) = live_mox_entry(state) else {
        return Ok(false);
    };
    let controller = state.objects.get(mox.object).controller;
    let lands = hand_land_cards(state, controller);
    e7a_state(state).mox_entering.retain(|&link| link != mox);
    match usize::from(option) {
        0 => {
            event::propose_and_commit(
                state,
                ProposedEvent::zone_change(mox.object, Zone::Graveyard),
            );
        }
        index => {
            let land = *lands
                .get(index - 1)
                .ok_or("Mox Diamond's land option is out of range")?;
            crate::engine::commit_discarded_card(state, land);
        }
    }
    Ok(true)
}

// ---- Static helpers read by the catalog's target filters -----------------------------

/// Whether the graveyard card `card` satisfies one of this module's
/// graveyard target filters for `controller`.
pub(crate) fn graveyard_target_matches(
    target: crate::standard_cards_v1::StandardTargetV1,
    controller: PlayerId,
    card: ObjectId,
    state: &GameState,
) -> bool {
    use crate::standard_cards_v1::StandardTargetV1 as S;
    let Some(live) = state.objects.try_get(card) else {
        return false;
    };
    let Some(def) = CARD_DEFS.get(live.card_def as usize) else {
        return false;
    };
    if live.zone != Zone::Graveyard || live.v4.is_token || live.spell_copy_origin.is_some() {
        return false;
    }
    match target {
        S::ArtifactCardInOwnGraveyardV1 => {
            live.owner == controller && def.has_type(CardType::Artifact)
        }
        S::ArtifactCardInOwnGraveyardManaValueAtMostV1(maximum) => {
            live.owner == controller
                && def.has_type(CardType::Artifact)
                && def.mana_value <= u16::from(maximum)
        }
        S::CreatureCardInAGraveyardV1 => def.has_type(CardType::Creature),
        S::UpToThreeLandCardsInOwnGraveyardV1 => {
            live.owner == controller && def.has_type(CardType::Land)
        }
        _ => false,
    }
}
