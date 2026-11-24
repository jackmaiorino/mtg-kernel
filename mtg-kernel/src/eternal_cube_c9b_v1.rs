//! Card programs for the eternal catalog's Vintage Cube wave C9b
//! (`docs/design/eternal_formats_support_v1.md`): Figure of Destiny, Figure
//! of Fable, Hexdrinker, Nadu, Winged Wisdom, Serra Paragon, and Swift
//! Reconfiguration.
//!
//! `eternal_cards_v1` forwards this module's effect leaves, triggers, combat
//! restrictions, entry hooks, and turn bookkeeping here, and the recipe tables
//! of `build_eternal_c9b_v1.rs` name its programs. Every table is keyed by a
//! printed name that occurs only in the eternal registry, so the Pauper, FDN,
//! and Standard catalogs never reach it. Behavior was read from each card's
//! current Oracle text and the Comprehensive Rules.
//!
//! Where each card lives:
//!
//! - **Figure of Destiny** & **Figure of Fable**: permanent characteristic-
//!   setting effects recorded per activation in `C9bStateV1::figure_forms` with
//!   Layer 4/6/7b timestamps. Type conditions ("If this creature is a
//!   Spirit/Warrior/Scout/Soldier") are checked on resolution, never on
//!   activation. Layer 4 replaces creature subtypes while preserving
//!   non-creature subtypes; Layer 7b sets base power and toughness; Layer 6
//!   grants Flying and First Strike (Figure of Destiny) or dynamic Protection
//!   from each of your opponents (Figure of Fable, relative to current
//!   controller across damage, targeting, blocking, and attachments). Earlier
//!   Layer 6 grants survive a later lower-tier activation (CR 613.1f).
//! - **Hexdrinker**: `Level up {1}` (sorcery speed, CR 702.87a) puts level
//!   counters into `C9bStateV1::level_counters` through
//!   `doubling_season_v48::counter_amount`. Level bands (3-7: 4/4 + protection
//!   from instants; 8+: 6/6 + `Keywords::PROTECTION_FROM_EVERYTHING`) are
//!   static abilities suppressed by ability removal (CR 711.2, 613.6) while
//!   keeping level counters until departure.
//! - **Nadu, Winged Wisdom**: Flying; static Layer 6 grant observed in
//!   `note_targeting`. Each active Nadu incarnation grants controlled creatures
//!   a twice-per-turn trigger tracked by `(creature_link, nadu_link, TurnV1)`
//!   so flickering either resets the cap. Resolution publicly reveals the top
//!   card of the library, putting lands onto the battlefield and nonlands into
//!   hand (not a draw).
//! - **Serra Paragon**: Flying; once per turn during its controller's turn per
//!   Serra Paragon incarnation, shared across playing a land from graveyard
//!   (`extra_land_candidates` / `begin_graveyard_land_play` /
//!   `finish_graveyard_land_play`) or casting a permanent spell with mana value
//!   3 or less from graveyard (`serra_paragon_permission` /
//!   `serra_paragon_x_cap` / `note_graveyard_permission_cast`). Entering
//!   permanents gain a dies trigger (`SerraParagonDiesV1`) that survives Serra
//!   Paragon leaving, is suppressed by later ability removal, and on resolution
//!   exiles that exact graveyard incarnation if still present while always
//!   gaining 2 life (CR 609.3).
//! - **Swift Reconfiguration**: Flash Aura (`Enchant creature or Vehicle`,
//!   `StandardTargetV1::CreatureOrVehicleV1`). Layer 4 sets card types to
//!   `[CardType::Artifact]` and subtypes to `[Subtype::Vehicle]` (removing
//!   creature and other card-type subtypes, CR 205.1a/205.3d) while keeping
//!   printed power/toughness and abilities; attaching removes a former creature
//!   from combat (CR 506.4). Layer 6 grants `Crew 5` at dedicated index
//!   `SWIFT_RECONFIGURATION_CREW_INDEX`; crewing records a later Layer 4
//!   timestamp in `C9bStateV1::crewed_after_reconfig` so the enchanted
//!   permanent becomes an Artifact Creature - Vehicle until end of turn.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

use crate::card_def::{
    ActivatedAbilityDef, ActivationTargetFilter, CardDef, CardType, CostComponent, Keywords,
    Subtype, TargetSpec, CARD_DEFS,
};
use crate::effect::{EffectContinuation, EffectOp, ExecCtx, PlayerRef};
use crate::eternal_cards_v1::EternalOpV1;
use crate::eternal_legacy_e9a_v1::TurnV1;
use crate::event::{self, CommittedEvent, HistoricalDamageSourceV1, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::standard_cards_v1::StandardOpV1;
use crate::state::{
    AbilitySourceContractV4, CastMethodV4, GameState, ObjectLinkV4, StackItem, Target, Zone,
};
use crate::trigger::{PendingTrigger, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};

pub(crate) const HEXDRINKER: &str = "Hexdrinker";
pub(crate) const NADU: &str = "Nadu, Winged Wisdom";
pub(crate) const SERRA_PARAGON: &str = "Serra Paragon";
pub(crate) const SWIFT_RECONFIGURATION: &str = "Swift Reconfiguration";

/// Dedicated activated-ability index for the `Crew 5` ability granted by
/// Swift Reconfiguration (distinct from printed indices and from wave C7c's
/// `LURKER_GRANTED_INDEX = 254` and face-up action `255`).
pub(crate) const SWIFT_RECONFIGURATION_CREW_INDEX: u8 = 253;

static CREW_5_COST: [CostComponent; 1] = [CostComponent::Crew(5)];

// ---- State --------------------------------------------------------------------------

/// Which tier of Figure of Destiny or Figure of Fable resolved onto a
/// battlefield incarnation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FigureTierV1 {
    /// Figure of Destiny tier 1: 2/2 Kithkin Spirit.
    DestinySpiritV1,
    /// Figure of Destiny tier 2: 4/4 Kithkin Spirit Warrior.
    DestinyWarriorV1,
    /// Figure of Destiny tier 3: 8/8 Kithkin Spirit Warrior Avatar, flying,
    /// first strike.
    DestinyAvatarV1,
    /// Figure of Fable tier 1: 2/3 Kithkin Scout.
    FableScoutV1,
    /// Figure of Fable tier 2: 4/5 Kithkin Soldier.
    FableSoldierV1,
    /// Figure of Fable tier 3: 7/8 Kithkin Avatar, protection from each of
    /// your opponents.
    FableAvatarV1,
}

impl FigureTierV1 {
    const fn subtypes(self) -> &'static [Subtype] {
        match self {
            Self::DestinySpiritV1 => &[Subtype::Kithkin, Subtype::Spirit],
            Self::DestinyWarriorV1 => &[Subtype::Kithkin, Subtype::Spirit, Subtype::Warrior],
            Self::DestinyAvatarV1 => &[
                Subtype::Kithkin,
                Subtype::Spirit,
                Subtype::Warrior,
                Subtype::Avatar,
            ],
            Self::FableScoutV1 => &[Subtype::Kithkin, Subtype::Scout],
            Self::FableSoldierV1 => &[Subtype::Kithkin, Subtype::Soldier],
            Self::FableAvatarV1 => &[Subtype::Kithkin, Subtype::Avatar],
        }
    }

    const fn base_pt(self) -> (i16, i16) {
        match self {
            Self::DestinySpiritV1 => (2, 2),
            Self::DestinyWarriorV1 => (4, 4),
            Self::DestinyAvatarV1 => (8, 8),
            Self::FableScoutV1 => (2, 3),
            Self::FableSoldierV1 => (4, 5),
            Self::FableAvatarV1 => (7, 8),
        }
    }

    const fn keywords(self) -> Keywords {
        match self {
            Self::DestinyAvatarV1 => Keywords(Keywords::FLYING.0 | Keywords::FIRST_STRIKE.0),
            _ => Keywords::NONE,
        }
    }

    const fn grants_protection_from_opponents(self) -> bool {
        matches!(self, Self::FableAvatarV1)
    }
}

/// One resolved Figure of Destiny or Figure of Fable characteristic-setting
/// effect on a battlefield incarnation, with its Layer 4/6/7b timestamp.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FigureFormV1 {
    pub host: ObjectLinkV4,
    pub tier: FigureTierV1,
    pub timestamp: u64,
}

/// Nadu, Winged Wisdom trigger usage count for one `(creature, nadu, turn)`
/// tuple (capped at 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NaduUseV1 {
    pub creature: ObjectLinkV4,
    pub nadu: ObjectLinkV4,
    pub turn: TurnV1,
    pub count: u8,
}

/// One use of a Serra Paragon incarnation's once-per-turn graveyard permission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SerraUseV1 {
    pub paragon: ObjectLinkV4,
    pub turn: TurnV1,
    pub card: ObjectLinkV4,
}

/// A permanent spell cast via Serra Paragon waiting on the stack to enter the
/// battlefield and receive its granted dies trigger at `grant_timestamp`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SerraPendingEntryV1 {
    pub spell: ObjectLinkV4,
    pub grant_timestamp: u64,
}

/// A battlefield permanent carrying Serra Paragon's granted dies trigger, with
/// its Layer 6 grant timestamp.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SerraGrantedDiesV1 {
    pub permanent: ObjectLinkV4,
    pub timestamp: u64,
}

/// A Serra Paragon granted dies trigger captured right before the permanent
/// left the battlefield for the graveyard, awaiting trigger collection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SerraQueuedDiesV1 {
    pub source: AbilitySourceContractV4,
    pub controller: PlayerId,
    pub dead: ObjectLinkV4,
}

/// Transient pre-move token for a graveyard land play authorized by Serra
/// Paragon (not serialized in `GameState`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SerraLandPlayV1 {
    paragon: ObjectLinkV4,
    card: ObjectLinkV4,
    grant_timestamp: u64,
}

/// Per-game state owned by Vintage Cube wave C9b, kept inside
/// `eternal_cards_v1::EternalStateV1`. Absent from serialization and hashes to
/// zero bytes while empty so every game without these cards keeps its exact
/// snapshot and hash.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct C9bStateV1 {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub figure_forms: Vec<FigureFormV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub level_counters: Vec<(ObjectLinkV4, u32)>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nadu_uses: Vec<NaduUseV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub serra_uses: Vec<SerraUseV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub serra_pending_entries: Vec<SerraPendingEntryV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub serra_granted_dies: Vec<SerraGrantedDiesV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub serra_queued_dies: Vec<SerraQueuedDiesV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub crewed_after_reconfig: Vec<(ObjectLinkV4, u64)>,
}

impl C9bStateV1 {
    pub fn is_empty(&self) -> bool {
        self.figure_forms.is_empty()
            && self.level_counters.is_empty()
            && self.nadu_uses.is_empty()
            && self.serra_uses.is_empty()
            && self.serra_pending_entries.is_empty()
            && self.serra_granted_dies.is_empty()
            && self.serra_queued_dies.is_empty()
            && self.crewed_after_reconfig.is_empty()
    }
}

/// An empty state hashes to zero bytes so every game without these cards keeps
/// its exact state hash.
impl std::hash::Hash for C9bStateV1 {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        if !self.is_empty() {
            b"eternal-cube-c9b/v1".hash(state);
            self.figure_forms.hash(state);
            self.level_counters.hash(state);
            self.nadu_uses.hash(state);
            self.serra_uses.hash(state);
            self.serra_pending_entries.hash(state);
            self.serra_granted_dies.hash(state);
            self.serra_queued_dies.hash(state);
            self.crewed_after_reconfig.hash(state);
        }
    }
}

fn c9b_state(state: &mut GameState) -> &mut C9bStateV1 {
    &mut state.eternal_v1.get_or_insert_with(Default::default).c9b
}

fn read_state(state: &GameState) -> Option<&C9bStateV1> {
    state.eternal_v1.as_ref().map(|eternal| &eternal.c9b)
}

// ---- Ops ----------------------------------------------------------------------------

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::C9bV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum C9bOpV1 {
    /// Figure of Destiny: `{R/W}: This creature becomes a Kithkin Spirit with
    /// base power and toughness 2/2.`
    FigureOfDestinyTier1V1,
    /// Figure of Destiny: `{R/W}{R/W}{R/W}: If this creature is a Spirit, it
    /// becomes a Kithkin Spirit Warrior with base power and toughness 4/4.`
    FigureOfDestinyTier2V1,
    /// Figure of Destiny: `{R/W}^6: If this creature is a Warrior, it becomes
    /// a Kithkin Spirit Warrior Avatar with base power and toughness 8/8,
    /// flying, and first strike.`
    FigureOfDestinyTier3V1,
    /// Figure of Fable: `{G/W}: This creature becomes a Kithkin Scout with
    /// base power and toughness 2/3.`
    FigureOfFableTier1V1,
    /// Figure of Fable: `{1}{G/W}{G/W}: If this creature is a Scout, it
    /// becomes a Kithkin Soldier with base power and toughness 4/5.`
    FigureOfFableTier2V1,
    /// Figure of Fable: `{3}{G/W}{G/W}{G/W}: If this creature is a Soldier, it
    /// becomes a Kithkin Avatar with base power and toughness 7/8 and
    /// protection from each of your opponents.`
    FigureOfFableTier3V1,
    /// Hexdrinker: `Level up {1}` (put a level counter on this permanent).
    HexdrinkerLevelUpV1,
    /// Nadu, Winged Wisdom granted trigger: reveal top card of library; if
    /// land put onto battlefield, else put into hand.
    NaduTriggerV1,
    /// Serra Paragon granted dies trigger: exile `dead` if still that
    /// graveyard incarnation, and controller gains 2 life.
    SerraParagonDiesV1 { dead: ObjectLinkV4 },
}

impl C9bOpV1 {
    /// Whether the leaf may stop for a player's choice (or a draw that a
    /// replacement can interrupt), so it runs in the resumable interpreter.
    pub(crate) fn contains_player_choice(&self) -> bool {
        false
    }
}

fn leaf(op: C9bOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::C9bV1(op)))
}

fn halt(state: &mut GameState, source: ObjectId) {
    state.engine.halted = Some((
        crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
        source,
    ));
}

// ---- Public card-recipe constructors ------------------------------------------------

pub fn figure_of_destiny_tier_1() -> EffectOp {
    leaf(C9bOpV1::FigureOfDestinyTier1V1)
}

pub fn figure_of_destiny_tier_2() -> EffectOp {
    leaf(C9bOpV1::FigureOfDestinyTier2V1)
}

pub fn figure_of_destiny_tier_3() -> EffectOp {
    leaf(C9bOpV1::FigureOfDestinyTier3V1)
}

pub fn figure_of_fable_tier_1() -> EffectOp {
    leaf(C9bOpV1::FigureOfFableTier1V1)
}

pub fn figure_of_fable_tier_2() -> EffectOp {
    leaf(C9bOpV1::FigureOfFableTier2V1)
}

pub fn figure_of_fable_tier_3() -> EffectOp {
    leaf(C9bOpV1::FigureOfFableTier3V1)
}

pub fn hexdrinker_level_up() -> EffectOp {
    leaf(C9bOpV1::HexdrinkerLevelUpV1)
}

// ---- Common helpers -----------------------------------------------------------------

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

fn battlefield_link(state: &GameState, object: ObjectId) -> Option<ObjectLinkV4> {
    let live = state.objects.try_get(object)?;
    (live.zone == Zone::Battlefield).then_some(ObjectLinkV4 {
        object,
        zone_change_count: live.zone_change_count,
    })
}

fn active_named(state: &GameState, object: ObjectId, name: &str) -> bool {
    state.objects.try_get(object).is_some_and(|live| {
        live.zone == Zone::Battlefield
            && live.v4.face_index == 0
            && live.v4.face_down_v1.is_none()
            && CARD_DEFS
                .get(usize::from(live.card_def))
                .is_some_and(|def| def.name == name && def.is_executable())
            && crate::continuous_characteristics_v1::printed_abilities_active(state, object)
    })
}

fn active_permanents<'a>(
    state: &'a GameState,
    name: &'a str,
    controller: Option<PlayerId>,
) -> impl Iterator<Item = ObjectId> + 'a {
    [PlayerId::P0, PlayerId::P1]
        .into_iter()
        .flat_map(move |seat| state.players[seat.index()].battlefield.iter().copied())
        .filter(move |&object| {
            active_named(state, object, name)
                && controller.is_none_or(|player| state.objects.get(object).controller == player)
        })
}

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

fn is_creature_subtype_id(id: u16) -> bool {
    Subtype::from_stable_id(id).is_some_and(Subtype::is_native_creature_type)
}

// ---- Triggered abilities ------------------------------------------------------------

/// Triggered abilities of this module's cards, falling through to wave C9c.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    crate::eternal_cube_c9c_v1::triggers_for(name)
}

/// Target specs of this module's printed and queued triggered abilities.
pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    delayed_effect_matches(name, effect).then_some(TargetSpec::None)
}

/// Queued triggers owned by this module's cards.
pub(crate) fn delayed_effect_matches(_name: &str, effect: &EffectOp) -> bool {
    granted_trigger_effect(effect)
}

/// Whether `effect` is a C9b granted triggered ability (Nadu's targeting
/// trigger or Serra Paragon's dies trigger), which may originate from any
/// permanent regardless of card name.
pub(crate) fn granted_trigger_effect(effect: &EffectOp) -> bool {
    matches!(
        effect,
        EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::C9bV1(
            C9bOpV1::NaduTriggerV1 | C9bOpV1::SerraParagonDiesV1 { .. }
        )))
    )
}

/// Target spec for C9b's granted triggered abilities.
pub(crate) fn granted_trigger_target_spec(effect: &EffectOp) -> Option<TargetSpec> {
    granted_trigger_effect(effect).then_some(TargetSpec::None)
}

// ---- Figure of Destiny & Figure of Fable --------------------------------------------

fn apply_figure_form(
    ctx: &ExecCtx,
    state: &mut GameState,
    required_subtype: Option<Subtype>,
    tier: FigureTierV1,
) {
    let Some(source) = crate::eternal_premodern_p1a_v1::live_ability_source(ctx, state) else {
        return;
    };
    if let Some(required) = required_subtype {
        if !crate::engine::has_effective_subtype(state, source, required) {
            return;
        }
    }
    let host = live_link(state, source);
    let timestamp = crate::engine::next_timestamp(state);
    c9b_state(state).figure_forms.push(FigureFormV1 {
        host,
        tier,
        timestamp,
    });
}

fn latest_figure_form(state: &GameState, object: ObjectId) -> Option<FigureFormV1> {
    let c9b = read_state(state)?;
    if c9b.figure_forms.is_empty() {
        return None;
    }
    let host = battlefield_link(state, object)?;
    c9b.figure_forms
        .iter()
        .copied()
        .filter(|form| form.host == host)
        .max_by_key(|form| form.timestamp)
}

/// Whether `object` currently has Figure of Fable's Layer 6 "protection from
/// each of your opponents" (relative to its current controller, respecting
/// later ability removals such as Dress Down or Oko's Elk).
pub(crate) fn has_protection_from_opponents(state: &GameState, object: ObjectId) -> bool {
    let Some(c9b) = read_state(state) else {
        return false;
    };
    if c9b.figure_forms.is_empty() {
        return false;
    }
    let Some(host) = battlefield_link(state, object) else {
        return false;
    };
    c9b.figure_forms.iter().any(|form| {
        form.host == host
            && form.tier.grants_protection_from_opponents()
            && crate::continuous_characteristics_v1::granted_ability_survives(
                state,
                object,
                form.timestamp,
            )
    })
}

// ---- Hexdrinker (Level up & level bands) --------------------------------------------

/// Number of level counters currently on `object`'s battlefield incarnation.
pub(crate) fn level_counters(state: &GameState, object: ObjectId) -> u32 {
    let Some(c9b) = read_state(state) else {
        return 0;
    };
    if c9b.level_counters.is_empty() {
        return 0;
    }
    let Some(host) = battlefield_link(state, object) else {
        return 0;
    };
    c9b.level_counters
        .iter()
        .find(|(link, _)| *link == host)
        .map_or(0, |(_, count)| *count)
}

fn resolve_hexdrinker_level_up(ctx: &ExecCtx, state: &mut GameState) {
    let Some(source) = crate::eternal_premodern_p1a_v1::live_ability_source(ctx, state) else {
        return;
    };
    let Ok(placed) = crate::doubling_season_v48::counter_amount(
        state,
        source,
        ctx.controller,
        crate::doubling_season_v48::CounterOriginV48::Effect,
        1,
    ) else {
        halt(state, source);
        return;
    };
    let Ok(added) = u32::try_from(placed) else {
        return;
    };
    if added == 0 {
        return;
    }
    let host = live_link(state, source);
    let ledger = &mut c9b_state(state).level_counters;
    match ledger.iter_mut().find(|(link, _)| *link == host) {
        Some((_, count)) => *count = count.saturating_add(added),
        None => ledger.push((host, added)),
    }
}

/// Active Hexdrinker level band (CR 711.2 static ability; suppressed when
/// Hexdrinker loses its printed abilities, e.g. Dress Down or The Wondrous
/// Wasp).
fn hexdrinker_active_level(state: &GameState, object: ObjectId) -> Option<u32> {
    if !active_named(state, object, HEXDRINKER) {
        return None;
    }
    let count = level_counters(state, object);
    (count >= 3).then_some(count)
}

/// Whether `object` currently has Hexdrinker's LEVEL 3-7 "Protection from
/// instants" static ability. (LEVEL 8+ uses `Keywords::PROTECTION_FROM_EVERYTHING`.)
pub(crate) fn has_protection_from_instants(state: &GameState, object: ObjectId) -> bool {
    hexdrinker_active_level(state, object).is_some_and(|level| (3..=7).contains(&level))
}

// ---- Layers 4, 6, and 7b (Figures, Hexdrinker, Swift Reconfiguration) ---------------

/// Layer 7b base power/toughness settings with timestamps:
/// - Figure of Destiny / Figure of Fable permanent characteristic changes
///   (timestamp of each resolved ability).
/// - Hexdrinker LEVEL 3-7 (4/4) and LEVEL 8+ (6/6) static abilities (timestamp
///   of Hexdrinker entering the battlefield, CR 613.7a; suppressed if Hexdrinker
///   lost abilities in Layer 6, CR 613.6).
pub(crate) fn base_power_toughness(state: &GameState, object: ObjectId) -> Vec<(i16, i16, u64)> {
    let mut out = Vec::new();
    if let Some(c9b) = read_state(state) {
        if !c9b.figure_forms.is_empty() {
            if let Some(host) = battlefield_link(state, object) {
                for form in &c9b.figure_forms {
                    if form.host == host {
                        let (power, toughness) = form.tier.base_pt();
                        out.push((power, toughness, form.timestamp));
                    }
                }
            }
        }
    }
    if let Some(level) = hexdrinker_active_level(state, object) {
        let timestamp = state.objects.get(object).v4.layer_timestamp.unwrap_or(0);
        if level >= 8 {
            out.push((6, 6, timestamp));
        } else {
            out.push((4, 4, timestamp));
        }
    }
    out
}

/// Layer 6 keywords granted by wave C9b:
/// - Figure of Destiny tier 3: Flying and First Strike (each grant survives
///   later lower-tier activations and is removed only by a later Layer 6
///   ability removal).
/// - Hexdrinker LEVEL 8+: `Keywords::PROTECTION_FROM_EVERYTHING` (static
///   ability, suppressed when Hexdrinker loses abilities).
pub(crate) fn granted_keywords(state: &GameState, object: ObjectId) -> Keywords {
    let mut found = Keywords::NONE;
    if let Some(c9b) = read_state(state) {
        if !c9b.figure_forms.is_empty() {
            if let Some(host) = battlefield_link(state, object) {
                for form in &c9b.figure_forms {
                    let kw = form.tier.keywords();
                    if form.host == host
                        && kw != Keywords::NONE
                        && crate::continuous_characteristics_v1::granted_ability_survives(
                            state,
                            object,
                            form.timestamp,
                        )
                    {
                        found = found | kw;
                    }
                }
            }
        }
    }
    if hexdrinker_active_level(state, object).is_some_and(|level| level >= 8) {
        found = found | Keywords::PROTECTION_FROM_EVERYTHING;
    }
    found
}

/// Latest attachment timestamp of an active Swift Reconfiguration enchanting
/// `host`. Does not query `host`'s effective types to avoid Layer 4 cycles.
pub(crate) fn swift_reconfiguration_timestamp(state: &GameState, host: ObjectId) -> Option<u64> {
    let live = state.objects.try_get(host)?;
    if live.zone != Zone::Battlefield || live.attachments.is_empty() {
        return None;
    }
    let host_link = Some(ObjectLinkV4 {
        object: host,
        zone_change_count: live.zone_change_count,
    });
    live.attachments
        .iter()
        .copied()
        .filter_map(|aura| {
            if aura == host {
                return None;
            }
            let source = state.objects.try_get(aura)?;
            if source.zone != Zone::Battlefield
                || source.v4.attached_to != host_link
                || !active_named(state, aura, SWIFT_RECONFIGURATION)
            {
                return None;
            }
            Some(source.v4.layer_timestamp.unwrap_or(0))
        })
        .max()
}

fn crewed_after_timestamp(state: &GameState, object: ObjectId, timestamp: u64) -> bool {
    if !crate::standard_cards_v1::is_crewed_creature(state, object) {
        return false;
    }
    let Some(c9b) = read_state(state) else {
        return false;
    };
    if c9b.crewed_after_reconfig.is_empty() {
        return false;
    }
    let Some(host) = battlefield_link(state, object) else {
        return false;
    };
    c9b.crewed_after_reconfig
        .iter()
        .any(|&(link, crew_ts)| link == host && crew_ts > timestamp)
}

/// Layer 4 card-type override for Swift Reconfiguration:
/// "Enchanted permanent is a Vehicle artifact with crew 5 and it loses all
/// other card types. (It's not a creature unless it's crewed.)"
///
/// Yields `None` when no Swift Reconfiguration applies or when a later Layer 4
/// type-setting effect (such as Oko's Elk) has a newer timestamp.
pub(crate) fn swift_reconfiguration_type_override(
    state: &GameState,
    object: ObjectId,
    card_type: CardType,
) -> Option<bool> {
    let sr_ts = swift_reconfiguration_timestamp(state, object)?;
    if crate::eternal_e5a_v1::statics::elk_timestamp(state, object)
        .is_some_and(|oko_ts| oko_ts > sr_ts)
    {
        return None;
    }
    if crate::continuous_characteristics_v1::creature_override(state, object)
        .is_some_and(|(_, other)| other > sr_ts)
    {
        return None;
    }
    if crate::continuous_characteristics_v1::land_type_timestamp(state, object)
        .is_some_and(|other| other > sr_ts)
    {
        return None;
    }
    if card_type == CardType::Artifact {
        return Some(true);
    }
    if card_type == CardType::Creature {
        let later_creature = crewed_after_timestamp(state, object, sr_ts)
            || crate::continuous_characteristics_v1::animation(state, object)
                .is_some_and(|(_, other)| other > sr_ts)
            || crate::continuous_characteristics_v1::later_land_animation(state, object, sr_ts)
            || crate::eternal_sideboard_s1e_v1::scream_timestamp(state, object)
                .is_some_and(|other| other > sr_ts)
            || crate::eternal_cube_c9a_v1::earthbent_timestamp(state, object)
                .is_some_and(|other| other > sr_ts)
            || crate::eternal_cube_c9c_v1::nissa_animated_timestamp(state, object)
                .is_some_and(|other| other > sr_ts);
        return Some(later_creature);
    }
    Some(false)
}

/// Applies C9b Layer 4 subtype changes (Figure of Destiny, Figure of Fable,
/// and Swift Reconfiguration) in timestamp order relative to each other and
/// any earlier subtype replacement.
pub(crate) fn adjust_subtypes(
    state: &GameState,
    id: ObjectId,
    subtype_ids: &mut Vec<u16>,
    replacement_timestamp: &mut Option<u64>,
) {
    let elk_ts = crate::eternal_e5a_v1::statics::elk_timestamp(state, id);
    let sr_ts = swift_reconfiguration_timestamp(state, id)
        .filter(|&ts| elk_ts.is_none_or(|oko| ts > oko))
        .filter(|&ts| replacement_timestamp.is_none_or(|other| ts > other));
    let fig = latest_figure_form(state, id)
        .filter(|form| elk_ts.is_none_or(|oko| form.timestamp > oko))
        .filter(|form| replacement_timestamp.is_none_or(|other| form.timestamp > other));

    match (sr_ts, fig) {
        (None, None) => {}
        (Some(ts), None) => {
            subtype_ids.clear();
            subtype_ids.push(Subtype::Vehicle.stable_id());
            *replacement_timestamp = Some(ts);
        }
        (None, Some(form)) => {
            if crate::engine::object_has_type(state, id, CardType::Creature) {
                subtype_ids.retain(|&sid| !is_creature_subtype_id(sid));
                subtype_ids.extend(form.tier.subtypes().iter().map(|s| s.stable_id()));
                *replacement_timestamp = Some(form.timestamp);
            }
        }
        (Some(ts), Some(form)) => {
            if ts >= form.timestamp {
                // Swift Reconfiguration attached after Figure's activation:
                // replaces card types with Artifact and subtypes with Vehicle.
                subtype_ids.clear();
                subtype_ids.push(Subtype::Vehicle.stable_id());
                *replacement_timestamp = Some(ts);
            } else {
                // Swift Reconfiguration applied first (setting [Vehicle]),
                // then a later Figure activation applied while crewed: keep
                // non-creature subtype Vehicle and add Figure's creature types
                // only while the permanent is currently a creature (CR 205.3d).
                subtype_ids.clear();
                subtype_ids.push(Subtype::Vehicle.stable_id());
                if crate::engine::object_has_type(state, id, CardType::Creature) {
                    subtype_ids.extend(form.tier.subtypes().iter().map(|s| s.stable_id()));
                    *replacement_timestamp = Some(form.timestamp);
                } else {
                    *replacement_timestamp = Some(ts);
                }
            }
        }
    }
}

/// Post-pass CR 205.3d filter when Swift Reconfiguration makes a permanent a
/// noncreature Vehicle artifact: no creature subtypes may remain while it is
/// not a creature.
pub(crate) fn finalize_subtypes(state: &GameState, id: ObjectId, subtype_ids: &mut Vec<u16>) {
    if swift_reconfiguration_timestamp(state, id).is_some()
        && !crate::engine::object_has_type(state, id, CardType::Creature)
    {
        subtype_ids.retain(|&sid| !is_creature_subtype_id(sid));
    }
}

// ---- Swift Reconfiguration (Aura, Crew 5, and combat removal) -----------------------

pub(crate) fn is_aura(def: &CardDef) -> bool {
    def.name == SWIFT_RECONFIGURATION
}

/// Swift Reconfiguration: "Enchant creature or Vehicle".
pub(crate) fn aura_host_legal(def: &CardDef, host: ObjectId, state: &GameState) -> Option<bool> {
    (def.name == SWIFT_RECONFIGURATION).then(|| {
        state.objects.get(host).zone == Zone::Battlefield
            && (crate::engine::object_has_type(state, host, CardType::Creature)
                || crate::engine::has_effective_subtype(state, host, Subtype::Vehicle))
    })
}

/// The synthesized `Crew 5` activated ability granted by Swift
/// Reconfiguration.
pub(crate) fn swift_reconfiguration_crew_ability() -> ActivatedAbilityDef {
    ActivatedAbilityDef {
        cost: &CREW_5_COST,
        target_spec: TargetSpec::None,
        effect: crate::standard_cards_v1::crew_animation,
        activation_zone: Zone::Battlefield,
        sorcery_speed_only: false,
        activation_target_filter: ActivationTargetFilter::TargetSpecOnly,
        max_activations_per_turn: None,
        face: None,
    }
}

/// Whether `object` currently has Swift Reconfiguration's Layer 6 granted
/// `Crew 5` activated ability.
pub(crate) fn granted_crew_ability(
    state: &GameState,
    object: ObjectId,
) -> Option<ActivatedAbilityDef> {
    let sr_ts = swift_reconfiguration_timestamp(state, object)?;
    crate::continuous_characteristics_v1::granted_ability_survives(state, object, sr_ts)
        .then(swift_reconfiguration_crew_ability)
}

/// Records the Layer 4 timestamp when a permanent enchanted by Swift
/// Reconfiguration is crewed, so Crew's later timestamp overrides Swift
/// Reconfiguration's type-removal until end of turn.
pub(crate) fn note_crewed(state: &mut GameState, object: ObjectId) {
    if swift_reconfiguration_timestamp(state, object).is_none() {
        return;
    }
    let Some(host) = battlefield_link(state, object) else {
        return;
    };
    let timestamp = crate::engine::next_timestamp(state);
    let entries = &mut c9b_state(state).crewed_after_reconfig;
    entries.retain(|&(link, _)| link != host);
    entries.push((host, timestamp));
}

/// CR 506.4: when attaching an Aura (such as Swift Reconfiguration) causes an
/// attacking or blocking creature to stop being a creature, remove it from
/// combat immediately.
pub(crate) fn after_attachment(state: &mut GameState, host: ObjectId) {
    if swift_reconfiguration_timestamp(state, host).is_some()
        && !crate::engine::object_has_type(state, host, CardType::Creature)
    {
        state.engine.combat.remove_from_combat(host);
    }
}

// ---- Protection checks (Figure of Fable & Hexdrinker) -------------------------------

fn damage_source_controller(state: &GameState, source: ObjectId) -> Option<PlayerId> {
    if let Some(item) = state.stack.iter().rev().find(|item| item.source == source) {
        return Some(item.controller);
    }
    state.objects.try_get(source).map(|live| live.controller)
}

fn source_is_instant_spell(state: &GameState, source: ObjectId) -> bool {
    let Some(live) = state.objects.try_get(source) else {
        return false;
    };
    if live.zone == Zone::Battlefield {
        return false;
    }
    crate::source_type_hexproof_v1::source_types(
        state,
        source,
        live.card_def,
        live.zone,
        live.zone_change_count,
    )
    .map_or_else(
        |_| {
            CARD_DEFS
                .get(usize::from(live.card_def))
                .is_some_and(|def| def.has_type(CardType::Instant))
        },
        |types| types.contains(CardType::Instant),
    )
}

/// Damage prevention from C9b protections (checked inside the engine's
/// `if !damage_cannot_be_prevented` gate, so Questing Beast / Flaring Pain /
/// Stomp bypass it):
/// - Figure of Fable: protection from sources controlled by an opponent of its
///   current controller.
/// - Hexdrinker LEVEL 3-7: protection from instant spells (not abilities).
pub(crate) fn protected_from_damage(state: &GameState, object: ObjectId, source: ObjectId) -> bool {
    let Some(live) = state.objects.try_get(object) else {
        return false;
    };
    if live.zone != Zone::Battlefield {
        return false;
    }
    if has_protection_from_opponents(state, object)
        && damage_source_controller(state, source).is_some_and(|ctrl| ctrl != live.controller)
    {
        return true;
    }
    has_protection_from_instants(state, object) && source_is_instant_spell(state, source)
}

/// Historical-source damage protection (`source_power_activation_v1::protected`).
pub(crate) fn protected_from_historical(
    state: &GameState,
    target: ObjectId,
    facts: HistoricalDamageSourceV1,
) -> bool {
    let Some(live) = state.objects.try_get(target) else {
        return false;
    };
    if live.zone != Zone::Battlefield {
        return false;
    }
    if has_protection_from_opponents(state, target) && facts.source.controller != live.controller {
        return true;
    }
    has_protection_from_instants(state, target)
        && facts.source.zone != Zone::Battlefield
        && CARD_DEFS
            .get(usize::from(facts.source.card_def))
            .is_some_and(|def| def.has_type(CardType::Instant))
}

/// Targeting protection for Figure of Fable ("protection from each of your
/// opponents"): blocks spells and abilities controlled by an opponent of
/// `object`'s current controller.
pub(crate) fn protected_from_targeting(
    state: &GameState,
    object: ObjectId,
    controller: PlayerId,
) -> bool {
    let Some(live) = state.objects.try_get(object) else {
        return false;
    };
    live.zone == Zone::Battlefield
        && live.controller != controller
        && has_protection_from_opponents(state, object)
}

/// Attachment removal/prohibition for Figure of Fable: Auras and Equipment
/// controlled by an opponent of `host`'s current controller cannot stay
/// attached or be attached.
pub(crate) fn protected_from_attachment(
    state: &GameState,
    host: ObjectId,
    attachment: ObjectId,
) -> bool {
    let (Some(host_live), Some(att_live)) = (
        state.objects.try_get(host),
        state.objects.try_get(attachment),
    ) else {
        return false;
    };
    host_live.zone == Zone::Battlefield
        && att_live.controller != host_live.controller
        && has_protection_from_opponents(state, host)
}

/// CR 704.5n / 702.16c: detach any battlefield Equipment attached to an
/// illegal host (a permanent that is no longer a creature, e.g. under Swift
/// Reconfiguration, or a host whose protection forbids the attachment, e.g.
/// Hexdrinker LEVEL 8+ or Figure of Fable). Returns whether any detached.
pub(crate) fn unattach_illegal_equipment(state: &mut GameState) -> bool {
    let detach: Vec<(ObjectId, ObjectLinkV4)> = state
        .objects
        .iter()
        .filter_map(|(id, live)| {
            let definition = CARD_DEFS.get(usize::from(live.card_def))?;
            let link = live.v4.attached_to?;
            let is_equipment =
                definition.equipment.is_some() || definition.subtypes.contains(&Subtype::Equipment);
            (live.zone == Zone::Battlefield
                && is_equipment
                && (!link_live_in(state, link, Zone::Battlefield)
                    || !crate::engine::object_has_type(state, link.object, CardType::Creature)
                    || crate::progenitus_v1::attachment_forbidden(state, id, link.object)
                    || protected_from_attachment(state, link.object, id)))
            .then_some((id, link))
        })
        .collect();
    for &(id, link) in &detach {
        if state.objects.try_get(link.object).is_some() {
            state
                .objects
                .get_mut(link.object)
                .attachments
                .retain(|attachment| *attachment != id);
        }
        state.objects.get_mut(id).v4.attached_to = None;
    }
    !detach.is_empty()
}

/// Blocking restriction for Figure of Fable: cannot be blocked by creatures
/// controlled by an opponent of its current controller.
pub(crate) fn blocker_allowed(state: &GameState, attacker: ObjectId, blocker: ObjectId) -> bool {
    if !has_protection_from_opponents(state, attacker) {
        return true;
    }
    let (Some(att), Some(blk)) = (
        state.objects.try_get(attacker),
        state.objects.try_get(blocker),
    ) else {
        return true;
    };
    blk.controller == att.controller
}

// ---- Nadu, Winged Wisdom ------------------------------------------------------------

/// Called whenever any spell or ability finishes announcing its targets (or
/// retargets). For each distinct targeted battlefield creature whose
/// controller controls one or more active Nadu, Winged Wisdom permanents,
/// queues Nadu's granted trigger on that creature up to twice per turn per
/// `(creature_link, nadu_link, turn)`.
pub(crate) fn note_targeting(state: &mut GameState, item: &StackItem) {
    if !cfg!(feature = "eternal-fixtures") || item.targets.is_empty() {
        return;
    }
    let active_nadus: Vec<(ObjectId, PlayerId, ObjectLinkV4, u64)> =
        active_permanents(state, NADU, None)
            .map(|nadu| {
                let live = state.objects.get(nadu);
                (
                    nadu,
                    live.controller,
                    live_link(state, nadu),
                    live.v4.layer_timestamp.unwrap_or(0),
                )
            })
            .collect();
    if active_nadus.is_empty() {
        return;
    }

    let turn = TurnV1::now(state);
    let mut seen = Vec::new();
    for &target in &item.targets {
        let Target::Object(object) = target else {
            continue;
        };
        if seen.contains(&object) {
            continue;
        }
        seen.push(object);
        let Some(live) = state.objects.try_get(object) else {
            continue;
        };
        if live.zone != Zone::Battlefield
            || !crate::engine::object_has_type(state, object, CardType::Creature)
        {
            continue;
        }
        let controller = live.controller;
        let creature_link = ObjectLinkV4 {
            object,
            zone_change_count: live.zone_change_count,
        };
        for &(_nadu_id, nadu_controller, nadu_link, nadu_ts) in &active_nadus {
            if nadu_controller != controller {
                continue;
            }
            if !crate::continuous_characteristics_v1::granted_ability_survives(
                state, object, nadu_ts,
            ) {
                continue;
            }
            let current_uses = read_state(state)
                .and_then(|c9b| {
                    c9b.nadu_uses
                        .iter()
                        .find(|entry| {
                            entry.creature == creature_link
                                && entry.nadu == nadu_link
                                && entry.turn == turn
                        })
                        .map(|entry| entry.count)
                })
                .unwrap_or(0);
            if current_uses >= 2 {
                continue;
            }
            let uses = &mut c9b_state(state).nadu_uses;
            if let Some(entry) = uses.iter_mut().find(|entry| {
                entry.creature == creature_link && entry.nadu == nadu_link && entry.turn == turn
            }) {
                entry.count += 1;
            } else {
                uses.push(NaduUseV1 {
                    creature: creature_link,
                    nadu: nadu_link,
                    turn,
                    count: 1,
                });
            }
            let source = AbilitySourceContractV4::capture(state, object);
            state
                .engine
                .pending_triggers
                .push(crate::eternal_cards_v1::delayed_trigger(
                    source,
                    controller,
                    leaf(C9bOpV1::NaduTriggerV1),
                ));
        }
    }
}

fn resolve_nadu_trigger(ctx: &ExecCtx, state: &mut GameState) {
    let player = ctx.controller;
    let Some(&top) = state.players[player.index()].library.first() else {
        return;
    };
    // spellbench-public-reveal: Nadu publicly reveals the top card of its
    // controller's library to both players.
    for observer in [PlayerId::P0, PlayerId::P1] {
        state.reveal_library_top(observer, player, 1);
    }
    let is_land = CARD_DEFS
        .get(usize::from(state.objects.get(top).card_def))
        .is_some_and(|def| def.has_type(CardType::Land));
    if is_land {
        event::propose_and_commit(
            state,
            ProposedEvent::zone_change_to_battlefield_under_controller(top, player),
        );
    } else {
        event::propose_and_commit(state, ProposedEvent::zone_change(top, Zone::Hand));
        if state.objects.get(top).zone == Zone::Hand {
            // spellbench-public-reveal: revealed nonland card goes to hand
            // (not a draw) and remains publicly known.
            for observer in [PlayerId::P0, PlayerId::P1] {
                if state
                    .reveal_distinct_hand_card(observer, player, top)
                    .is_err()
                {
                    halt(state, ctx.source);
                    return;
                }
            }
        }
    }
}

// ---- Serra Paragon ------------------------------------------------------------------

fn unused_serra_paragon(state: &GameState, holder: PlayerId) -> Option<ObjectLinkV4> {
    let turn = TurnV1::now(state);
    active_permanents(state, SERRA_PARAGON, Some(holder))
        .map(|object| live_link(state, object))
        .find(|link| {
            read_state(state).is_none_or(|c9b| {
                !c9b.serra_uses
                    .iter()
                    .any(|used| used.paragon == *link && used.turn == turn)
            })
        })
}

fn serra_paragon_spell_card(state: &GameState, object: ObjectId, holder: PlayerId) -> bool {
    let Some(live) = state.objects.try_get(object) else {
        return false;
    };
    let Some(def) = CARD_DEFS.get(usize::from(live.card_def)) else {
        return false;
    };
    live.owner == holder
        && !live.v4.is_token
        && live.spell_copy_origin.is_none()
        && !def.is_token
        && !def.is_land
        && is_permanent_card(def)
        && def.mana_value <= 3
}

fn other_graveyard_cast_permission_covers(
    state: &GameState,
    object: ObjectId,
    holder: PlayerId,
    zone_change_count: u32,
    include_lurrus: bool,
) -> bool {
    crate::standard_cards_v1::standard_graveyard_cast_granted(
        state,
        object,
        holder,
        zone_change_count,
    ) || crate::eternal_e5a_v1::permissions::will_permission(state, object, holder)
        || crate::eternal_legacy_e7a_v1::emry_permission(state, object, holder, zone_change_count)
        || (include_lurrus
            && crate::eternal_lurrus_v1::lurrus_permission(
                state,
                object,
                holder,
                zone_change_count,
            ))
        || crate::eternal_pauper_e6b_v1::disturb_permission(
            state,
            object,
            holder,
            zone_change_count,
        )
        || crate::eternal_legacy_e8a_v1::statics::hogaak_permission(
            state,
            object,
            holder,
            zone_change_count,
        )
        || crate::eternal_cube_c8a_v1::jace_graveyard_permission(
            state,
            object,
            holder,
            zone_change_count,
        )
}

/// Serra Paragon: graveyard land-play candidates during its controller's turn
/// while an active Serra Paragon incarnation has not yet used its once-per-turn
/// permission.
pub(crate) fn extra_land_candidates(state: &GameState, player: PlayerId) -> Vec<ObjectId> {
    if state.active_player != player || unused_serra_paragon(state, player).is_none() {
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
                && CARD_DEFS[usize::from(live.card_def)].is_playable_land()
        })
        .collect()
}

/// Checks right before `play_land` moves `card` whether Serra Paragon's
/// once-per-turn permission is what authorizes playing this graveyard land.
pub(crate) fn begin_graveyard_land_play(
    state: &mut GameState,
    card: ObjectId,
    player: PlayerId,
    muldrotha_used: bool,
) -> Option<SerraLandPlayV1> {
    if muldrotha_used || state.active_player != player {
        return None;
    }
    let live = state.objects.try_get(card)?;
    if live.zone != Zone::Graveyard
        || live.owner != player
        || live.v4.is_token
        || !CARD_DEFS[usize::from(live.card_def)].is_playable_land()
    {
        return None;
    }
    // If another unrestricted graveyard land permission covers this land play,
    // do not consume Serra Paragon's once-per-turn use or add the exile trigger.
    let other_covers = crate::eternal_e5a_v1::permissions::extra_land_candidates(state, player)
        .contains(&card)
        || crate::eternal_legacy_e7a_v1::extra_land_candidates(state, player).contains(&card)
        || crate::eternal_vintage_e7b_v1::statics::extra_land_candidates(state, player)
            .contains(&card)
        || crate::eternal_cube_c5b_v1::extra_land_candidates(state, player).contains(&card);
    if other_covers {
        return None;
    }
    let paragon = unused_serra_paragon(state, player)?;
    let gy_card = live_link(state, card);
    let grant_timestamp = crate::engine::next_timestamp(state);
    Some(SerraLandPlayV1 {
        paragon,
        card: gy_card,
        grant_timestamp,
    })
}

/// Records the consumed Serra Paragon land play and grants the resulting
/// battlefield permanent Serra Paragon's dies trigger.
pub(crate) fn finish_graveyard_land_play(
    state: &mut GameState,
    pending: Option<SerraLandPlayV1>,
    card: ObjectId,
) {
    let Some(play) = pending else {
        return;
    };
    let turn = TurnV1::now(state);
    let entering_link = battlefield_link(state, card);
    let c9b = c9b_state(state);
    c9b.serra_uses.retain(|used| used.turn == turn);
    c9b.serra_uses.push(SerraUseV1 {
        paragon: play.paragon,
        turn,
        card: play.card,
    });
    if let Some(permanent) = entering_link {
        c9b.serra_granted_dies.push(SerraGrantedDiesV1 {
            permanent,
            timestamp: play.grant_timestamp,
        });
    }
}

/// Serra Paragon: whether `holder` may cast the graveyard permanent card
/// incarnation `(object, zone_change_count)` (MV <= 3) during their turn.
pub(crate) fn serra_paragon_permission(
    state: &GameState,
    object: ObjectId,
    holder: PlayerId,
    zone_change_count: u32,
) -> bool {
    let Some(live) = state.objects.try_get(object) else {
        return false;
    };
    if !serra_paragon_spell_card(state, object, holder) {
        return false;
    }
    let card = ObjectLinkV4 {
        object,
        zone_change_count,
    };
    let turn = TurnV1::now(state);
    if read_state(state).is_some_and(|c9b| {
        c9b.serra_uses
            .iter()
            .any(|used| used.card == card && used.turn == turn)
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
        && unused_serra_paragon(state, holder).is_some()
}

/// Caps chosen `X` for an X-cost permanent spell cast from graveyard solely
/// through Serra Paragon so its spell mana value on the stack stays <= 3
/// (CR 202.3e, Serra Paragon ruling).
pub(crate) fn serra_paragon_x_cap(
    def: &CardDef,
    spell: ObjectId,
    origin_zone: Zone,
    controller: PlayerId,
    origin_zcc: u32,
    method: CastMethodV4,
    state: &GameState,
) -> Option<u8> {
    if origin_zone != Zone::Graveyard || method != CastMethodV4::Normal || def.cost.x_count == 0 {
        return None;
    }
    if other_graveyard_cast_permission_covers(state, spell, controller, origin_zcc, false) {
        return None;
    }
    if !serra_paragon_permission(state, spell, controller, origin_zcc) {
        return None;
    }
    let remaining = 3_u16.saturating_sub(def.mana_value);
    let max_x = remaining / u16::from(def.cost.x_count);
    Some(u8::try_from(max_x).unwrap_or(u8::MAX))
}

/// Records the Serra Paragon permission used when finalizing a permanent spell
/// cast from graveyard, and stages its stack incarnation to receive the
/// granted dies trigger if it resolves onto the battlefield.
pub(crate) fn note_graveyard_permission_cast(
    state: &mut GameState,
    spell: ObjectId,
    origin_zone_change_count: u32,
    controller: PlayerId,
) {
    if other_graveyard_cast_permission_covers(
        state,
        spell,
        controller,
        origin_zone_change_count,
        true,
    ) || state.active_player != controller
        || !serra_paragon_spell_card(state, spell, controller)
    {
        return;
    }
    let card = ObjectLinkV4 {
        object: spell,
        zone_change_count: origin_zone_change_count,
    };
    let turn = TurnV1::now(state);
    if read_state(state).is_some_and(|c9b| {
        c9b.serra_uses
            .iter()
            .any(|used| used.card == card && used.turn == turn)
    }) {
        return;
    }
    let Some(paragon) = unused_serra_paragon(state, controller) else {
        return;
    };
    let spell_link = live_link(state, spell);
    let grant_timestamp = crate::engine::next_timestamp(state);
    let c9b = c9b_state(state);
    c9b.serra_uses.retain(|used| used.turn == turn);
    c9b.serra_uses.push(SerraUseV1 {
        paragon,
        turn,
        card,
    });
    c9b.serra_pending_entries.push(SerraPendingEntryV1 {
        spell: spell_link,
        grant_timestamp,
    });
}

/// Transfers Serra Paragon's granted dies trigger from a resolving stack spell
/// onto its newly entered battlefield permanent incarnation.
pub(crate) fn apply_entry(state: &mut GameState, object: ObjectId) {
    if read_state(state).is_none_or(|c9b| c9b.serra_pending_entries.is_empty()) {
        return;
    }
    let Some(live) = state.objects.try_get(object) else {
        return;
    };
    if live.zone != Zone::Battlefield {
        return;
    }
    let new_zcc = live.zone_change_count;
    let matched = c9b_state(state)
        .serra_pending_entries
        .iter()
        .position(|entry| {
            entry.spell.object == object
                && entry.spell.zone_change_count.checked_add(1) == Some(new_zcc)
        });
    if let Some(pos) = matched {
        let entry = c9b_state(state).serra_pending_entries.remove(pos);
        c9b_state(state)
            .serra_granted_dies
            .push(SerraGrantedDiesV1 {
                permanent: ObjectLinkV4 {
                    object,
                    zone_change_count: new_zcc,
                },
                timestamp: entry.grant_timestamp,
            });
    }
}

/// Pre-departure hook called in `event.rs` before `object` moves from `from`
/// to `to`:
/// 1. Fires Serra Paragon's granted dies trigger if a tagged permanent is put
///    into a graveyard from the battlefield while its granted ability is still
///    active (not removed by a later Dress Down / Oko / Wasp).
/// 2. Prunes incarnation-scoped C9b records (`figure_forms`, `level_counters`,
///    `serra_granted_dies`, `crewed_after_reconfig`, `serra_pending_entries`)
///    for the departing incarnation.
pub(crate) fn note_departure(state: &mut GameState, object: ObjectId, from: Zone, to: Zone) {
    if read_state(state).is_none_or(C9bStateV1::is_empty) {
        return;
    }
    let Some(live) = state.objects.try_get(object) else {
        return;
    };
    let link = ObjectLinkV4 {
        object,
        zone_change_count: live.zone_change_count,
    };

    if from == Zone::Battlefield && to == Zone::Graveyard {
        let grants: Vec<u64> = read_state(state)
            .map(|c9b| {
                c9b.serra_granted_dies
                    .iter()
                    .filter(|entry| entry.permanent == link)
                    .map(|entry| entry.timestamp)
                    .collect()
            })
            .unwrap_or_default();
        if !grants.is_empty() {
            let source = AbilitySourceContractV4::capture(state, object);
            let controller = live.controller;
            let dead = ObjectLinkV4 {
                object,
                zone_change_count: live.zone_change_count.saturating_add(1),
            };
            for timestamp in grants {
                if crate::continuous_characteristics_v1::granted_ability_survives(
                    state, object, timestamp,
                ) {
                    c9b_state(state).serra_queued_dies.push(SerraQueuedDiesV1 {
                        source,
                        controller,
                        dead,
                    });
                }
            }
        }
    }

    if from == Zone::Battlefield && to != Zone::Battlefield {
        if let Some(eternal) = state.eternal_v1.as_mut() {
            let c9b = &mut eternal.c9b;
            c9b.figure_forms.retain(|form| form.host != link);
            c9b.level_counters.retain(|(entry, _)| *entry != link);
            c9b.serra_granted_dies
                .retain(|entry| entry.permanent != link);
            c9b.crewed_after_reconfig
                .retain(|(entry, _)| *entry != link);
        }
    } else if from == Zone::Stack && to != Zone::Battlefield {
        if let Some(eternal) = state.eternal_v1.as_mut() {
            eternal
                .c9b
                .serra_pending_entries
                .retain(|entry| entry.spell != link);
        }
    }
}

/// Drains queued Serra Paragon granted dies triggers for trigger collection.
pub(crate) fn delayed_triggers(
    state: &mut GameState,
    _events: &[CommittedEvent],
) -> Vec<PendingTrigger> {
    let Some(eternal) = state.eternal_v1.as_mut() else {
        return Vec::new();
    };
    if eternal.c9b.serra_queued_dies.is_empty() {
        return Vec::new();
    }
    std::mem::take(&mut eternal.c9b.serra_queued_dies)
        .into_iter()
        .map(|queued| {
            crate::eternal_cards_v1::delayed_trigger(
                queued.source,
                queued.controller,
                leaf(C9bOpV1::SerraParagonDiesV1 { dead: queued.dead }),
            )
        })
        .collect()
}

fn resolve_serra_paragon_dies(ctx: &ExecCtx, state: &mut GameState, dead: ObjectLinkV4) {
    if link_live_in(state, dead, Zone::Graveyard) {
        event::propose_and_commit(state, ProposedEvent::zone_change(dead.object, Zone::Exile));
    }
    // CR 609.3: even if the card already left the graveyard before resolution,
    // its controller still gains 2 life.
    crate::effect::execute(
        &EffectOp::GainLife {
            player: PlayerRef::Controller,
            amount: 2,
        },
        ctx,
        state,
    );
}

// ---- Turn and cleanup housekeeping --------------------------------------------------

/// Resets per-turn trackers (Nadu's twice-per-turn cap and Serra Paragon's
/// once-per-turn usage) at the start of every turn (including extra turns) and
/// prunes any stale incarnation records.
pub(crate) fn begin_turn(state: &mut GameState, _player: PlayerId) {
    if read_state(state).is_none_or(C9bStateV1::is_empty) {
        return;
    }
    let keep_figure: Vec<FigureFormV1> = read_state(state)
        .map(|c9b| {
            c9b.figure_forms
                .iter()
                .copied()
                .filter(|form| link_live_in(state, form.host, Zone::Battlefield))
                .collect()
        })
        .unwrap_or_default();
    let keep_levels: Vec<(ObjectLinkV4, u32)> = read_state(state)
        .map(|c9b| {
            c9b.level_counters
                .iter()
                .copied()
                .filter(|(link, _)| link_live_in(state, *link, Zone::Battlefield))
                .collect()
        })
        .unwrap_or_default();
    let keep_pending: Vec<SerraPendingEntryV1> = read_state(state)
        .map(|c9b| {
            c9b.serra_pending_entries
                .iter()
                .copied()
                .filter(|entry| link_live_in(state, entry.spell, Zone::Stack))
                .collect()
        })
        .unwrap_or_default();
    let keep_dies: Vec<SerraGrantedDiesV1> = read_state(state)
        .map(|c9b| {
            c9b.serra_granted_dies
                .iter()
                .copied()
                .filter(|entry| link_live_in(state, entry.permanent, Zone::Battlefield))
                .collect()
        })
        .unwrap_or_default();
    if let Some(eternal) = state.eternal_v1.as_mut() {
        let c9b = &mut eternal.c9b;
        c9b.figure_forms = keep_figure;
        c9b.level_counters = keep_levels;
        c9b.nadu_uses.clear();
        c9b.serra_uses.clear();
        c9b.serra_pending_entries = keep_pending;
        c9b.serra_granted_dies = keep_dies;
        c9b.crewed_after_reconfig.clear();
    }
}

/// Clears end-of-turn crew timestamps for Swift Reconfiguration at cleanup.
pub(crate) fn begin_cleanup(state: &mut GameState) {
    if let Some(eternal) = state.eternal_v1.as_mut() {
        eternal.c9b.crewed_after_reconfig.clear();
    }
}

// ---- Execution ----------------------------------------------------------------------

/// Runs one of this module's leaves in synchronous execution.
pub(crate) fn execute(op: &C9bOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        C9bOpV1::FigureOfDestinyTier1V1 => {
            apply_figure_form(ctx, state, None, FigureTierV1::DestinySpiritV1);
        }
        C9bOpV1::FigureOfDestinyTier2V1 => {
            apply_figure_form(
                ctx,
                state,
                Some(Subtype::Spirit),
                FigureTierV1::DestinyWarriorV1,
            );
        }
        C9bOpV1::FigureOfDestinyTier3V1 => {
            apply_figure_form(
                ctx,
                state,
                Some(Subtype::Warrior),
                FigureTierV1::DestinyAvatarV1,
            );
        }
        C9bOpV1::FigureOfFableTier1V1 => {
            apply_figure_form(ctx, state, None, FigureTierV1::FableScoutV1);
        }
        C9bOpV1::FigureOfFableTier2V1 => {
            apply_figure_form(
                ctx,
                state,
                Some(Subtype::Scout),
                FigureTierV1::FableSoldierV1,
            );
        }
        C9bOpV1::FigureOfFableTier3V1 => {
            apply_figure_form(
                ctx,
                state,
                Some(Subtype::Soldier),
                FigureTierV1::FableAvatarV1,
            );
        }
        C9bOpV1::HexdrinkerLevelUpV1 => resolve_hexdrinker_level_up(ctx, state),
        C9bOpV1::NaduTriggerV1 => resolve_nadu_trigger(ctx, state),
        C9bOpV1::SerraParagonDiesV1 { dead } => resolve_serra_paragon_dies(ctx, state, *dead),
    }
}

/// Runs one of this module's leaves in the resumable interpreter.
pub(crate) fn drive(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    op: &C9bOpV1,
    _path: Vec<u16>,
) -> Result<bool, String> {
    execute(op, &continuation.ctx, state);
    Ok(false)
}
