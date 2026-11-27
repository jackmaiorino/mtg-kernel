//! Card programs for the eternal catalog's Vintage Cube wave C8b
//! (`docs/design/eternal_formats_support_v1.md`): copy creatures, dies copies,
//! Saheeli, and bestow cards.
//!
//! `eternal_cards_v1` forwards this module's effect leaves and triggers here,
//! and the recipe tables of `build_eternal_c8b_v1.rs` name its programs. Every
//! table is keyed by a printed name that occurs only in the eternal registry,
//! so the Pauper, FDN and Standard catalogs never reach it. Behavior was read
//! from each card's current Oracle text and rulings.
//!
//! Where each card lives:
//!
//! - **Phantasmal Image**: as-enters copy via `entry_choice_decision` and
//!   `defer_triggers` (covering both cast entry and non-cast flicker/reanimate
//!   entry, suppressed under Dress Down), adding `Subtype::Illusion` and the
//!   granted becomes-target sacrifice trigger observed in `note_targeting`
//!   (subject to Layer 6 ability removal such as Dress Down or The Wondrous
//!   Wasp).
//! - **Superior Spider-Man**: Mind Swap as-enters copy of any creature card in
//!   any graveyard via `entry_choice_decision` and `defer_triggers`, overriding
//!   copiable name to `"Superior Spider-Man"`, base P/T to `4/4` (above Layer
//!   7a CDAs per CR 707.9d, below Layer 7b setters), adding `Spider`, `Human`,
//!   and `Hero` subtypes while keeping the copied card's supertypes, colors,
//!   mana cost, and abilities, and queuing the reflexive trigger to exile that
//!   exact graveyard incarnation (`SuperiorSpiderManExileV1`).
//! - **Vaultborn Tyrant**: Trample; ETB trigger for itself or another creature
//!   you control with power 4 or greater (`VaultbornEntersV1`) gaining 3 life
//!   and drawing a card via the resumable `EffectOp::DrawCards` path; nontoken
//!   dies trigger (`VaultbornDeathCopyV1`, suppressed on tokens via
//!   `suppress_token_death_copy`) creating a token copy that is an artifact in
//!   addition to its other types (via `C8bCopyModsV1` staged before entry
//!   observers so the token enters as a 6/6 artifact creature and triggers its
//!   own ETB).
//! - **Saheeli, Sublime Artificer**: noncreature spell cast trigger creating a
//!   1/1 colorless `Servo Token`; `-2` loyalty ability with two distinct
//!   controlled targets (`targets.rs`) turning target 0 into a copy of target 1
//!   plus `CardType::Artifact` until end of turn, restored in `begin_cleanup`
//!   (or on departure).
//! - **Springheart Nantuko**: Bestow `{1}{G}` (`bestow_host_pt_bonus` gives
//!   enchanted creature `+1/+1`); Landfall checks at resolution whether this
//!   exact permanent is attached to a creature you control and whether `{1}{G}`
//!   is payable (`can_pay_effect_mana`), offering the optional payment to
//!   create a token copy of that creature or otherwise creating a 1/1
//!   `Green Insect Token`.
//! - **Detective's Phoenix**: Flying, Haste; Bestow `{R}`, Collect evidence 6
//!   (`bestow_host_pt_bonus` gives enchanted creature `+2/+2` and Flying/Haste);
//!   castable from graveyard only via its Bestow ability (`graveyard_bestow_allowed`
//!   and `bestow_collect_evidence_minimum`), requiring other graveyard cards
//!   totaling mana value 6 or greater.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

pub(crate) mod targets;

use crate::card_def::{
    card_id_by_name, mana_colors_mask, CardType, Subtype, Supertype, TargetSpec, CARD_DEFS,
};
use crate::effect::{
    EffectContinuation, EffectFrame, EffectObjectBinding, EffectOp, ExecCtx, PlayerRef,
};
use crate::engine::{Decision, UnsupportedMechanic};
use crate::eternal_cards_v1::{EternalOpV1, EternalTriggerV1};
use crate::event::{self, CommittedEvent, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::mana::ManaColor;
use crate::standard_cards_v1::{StandardOpV1, StandardTargetV1, StandardTriggerV1};
use crate::state::{AbilitySourceContractV4, GameState, ObjectLinkV4, StackItem, Target, Zone};
use crate::trigger::{TriggerCondition, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};

pub(crate) const PHANTASMAL_IMAGE: &str = "Phantasmal Image";
pub(crate) const SUPERIOR_SPIDER_MAN: &str = "Superior Spider-Man";
pub(crate) const VAULTBORN_TYRANT: &str = "Vaultborn Tyrant";
pub(crate) const SAHEELI: &str = "Saheeli, Sublime Artificer";
pub(crate) const SPRINGHEART_NANTUKO: &str = "Springheart Nantuko";
pub(crate) const DETECTIVES_PHOENIX: &str = "Detective's Phoenix";
pub(crate) const SERVO_TOKEN: &str = "Servo Token";
pub(crate) const GREEN_INSECT_TOKEN: &str = "Green Insect Token";

const SAHEELI_TARGET: TargetSpec = TargetSpec::StandardV1(
    StandardTargetV1::ControlledArtifactThenAnotherControlledArtifactOrCreatureV1,
);

// ---- State --------------------------------------------------------------------------

/// Copiable modifications layered on a battlefield permanent incarnation
/// (CR 707.2, 707.9): added Artifact type (Vaultborn Tyrant token copy,
/// Saheeli -2), added subtypes (Phantasmal Image: Illusion; Superior
/// Spider-Man: Spider, Human, Hero), overridden name and base P/T (Superior
/// Spider-Man), and Phantasmal Image's granted becomes-target sacrifice
/// triggered ability.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct C8bCopyModsV1 {
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub added_artifact: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub added_subtypes: Vec<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub override_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_pt: Option<(i16, i16)>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub illusion_sacrifice: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub added_creature: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub added_legendary: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub added_vigilance: bool,
}

impl C8bCopyModsV1 {
    pub(crate) fn is_empty(&self) -> bool {
        !self.added_artifact
            && self.added_subtypes.is_empty()
            && self.override_name.is_none()
            && self.base_pt.is_none()
            && !self.illusion_sacrifice
            && !self.added_creature
            && !self.added_legendary
            && !self.added_vigilance
    }

    pub(crate) fn merge_subtypes(&mut self, extra: &[Subtype]) {
        for subtype in extra {
            let id = subtype.stable_id();
            if !self.added_subtypes.contains(&id) {
                self.added_subtypes.push(id);
            }
        }
        self.added_subtypes.sort_unstable();
        self.added_subtypes.dedup();
    }
}

/// Physical card characteristics saved when Phantasmal Image or Superior
/// Spider-Man enters as a copy, restored immediately before leaving the
/// battlefield so the card arrives in its new zone as itself.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct C8bSavedCopyV1 {
    pub host: ObjectLinkV4,
    pub card_def: u16,
    pub name: String,
    pub color_mask: u8,
    pub subtype_ids: Vec<u16>,
    pub face_index: u8,
}

/// Temporary until-end-of-turn copy installed by Saheeli, Sublime Artificer's
/// -2 ability, saving the target's pre-Saheeli values so cleanup (or departure)
/// restores them cleanly even if the target was already a copy.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct C8bTempCopyV1 {
    pub host: ObjectLinkV4,
    pub card_def: u16,
    pub name: String,
    pub color_mask: u8,
    pub subtype_ids: Vec<u16>,
    pub face_index: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prev_mods: Option<C8bCopyModsV1>,
}

/// Last-known copiable values captured immediately before a nontoken
/// Vaultborn Tyrant dies, so its dies trigger creates an exact artifact token
/// copy (including inherited copy mods if a Phantasmal Image or Superior
/// Spider-Man copied Vaultborn Tyrant).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct C8bDeathCopyLkiV1 {
    pub source: ObjectLinkV4,
    pub card_def: u16,
    pub face_index: u8,
    #[serde(default, skip_serializing_if = "C8bCopyModsV1::is_empty")]
    pub mods: C8bCopyModsV1,
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub copied_end_step_sacrifices_v1: u32,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub copied_nightmare_v1: bool,
}

fn is_zero_u32(v: &u32) -> bool {
    *v == 0
}

/// Per-game state owned by Vintage Cube wave C8b, kept inside
/// `eternal_cards_v1::EternalStateV1`. Skips serde and hashes to zero bytes
/// while empty so every game without these cards keeps its exact snapshot and
/// hash.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct C8bStateV1 {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entry_answered: Vec<ObjectLinkV4>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub permanent_copies: Vec<C8bSavedCopyV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub copy_mods: Vec<(ObjectLinkV4, C8bCopyModsV1)>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub saheeli_copies: Vec<C8bTempCopyV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub death_lki: Vec<C8bDeathCopyLkiV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_token_mods: Option<C8bCopyModsV1>,
}

impl C8bStateV1 {
    pub(crate) fn is_empty(&self) -> bool {
        self.entry_answered.is_empty()
            && self.permanent_copies.is_empty()
            && self.copy_mods.is_empty()
            && self.saheeli_copies.is_empty()
            && self.death_lki.is_empty()
            && self.pending_token_mods.is_none()
    }
}

/// An empty state hashes to nothing, so every game without these cards keeps
/// its exact state hash.
impl std::hash::Hash for C8bStateV1 {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        if !self.is_empty() {
            b"eternal-cube-c8b/v1".hash(state);
            self.entry_answered.hash(state);
            self.permanent_copies.hash(state);
            self.copy_mods.hash(state);
            self.saheeli_copies.hash(state);
            self.death_lki.hash(state);
            self.pending_token_mods.hash(state);
        }
    }
}

fn c8b_state(state: &mut GameState) -> &mut C8bStateV1 {
    &mut state.eternal_v1.get_or_insert_with(Default::default).c8b
}

pub(crate) fn read_state(state: &GameState) -> Option<&C8bStateV1> {
    state.eternal_v1.as_ref().map(|eternal| &eternal.c8b)
}

// ---- Triggers and ops ---------------------------------------------------------------

/// Trigger conditions owned by this module, carried as
/// `eternal_cards_v1::EternalTriggerV1::C8bV1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum C8bTriggerV1 {
    /// Vaultborn Tyrant: "Whenever this creature or another creature you
    /// control with power 4 or greater enters".
    VaultbornEntersV1,
}

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::C8bV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum C8bOpV1 {
    /// Phantasmal Image's granted trigger: "When this creature becomes the
    /// target of a spell or ability, sacrifice it."
    SacrificeSourceV1,
    /// Superior Spider-Man's reflexive trigger: "When you do, exile that card."
    SuperiorSpiderManExileV1 { card: ObjectLinkV4 },
    /// Vaultborn Tyrant's dies trigger: create a token that's a copy of it,
    /// except it's an artifact in addition to its other types.
    VaultbornDeathCopyV1,
    /// Saheeli, Sublime Artificer's -2: target 0 becomes a copy of target 1
    /// until end of turn, except it's an artifact in addition to its other
    /// types.
    SaheeliCopyV1,
    /// Springheart Nantuko's Landfall resolution entry point: if attached to a
    /// creature you control and `{1}{G}` is payable, offers the optional
    /// payment; otherwise directly creates a 1/1 Green Insect Token.
    SpringheartLandfallV1,
    /// Springheart Nantuko chosen branch: pay `{1}{G}` and create a token copy
    /// of `host`, falling back to a 1/1 Green Insect Token if no token was
    /// created.
    SpringheartPayAndCopyV1 { host: ObjectLinkV4 },
    /// Springheart Nantuko fallback branch: create a 1/1 Green Insect Token.
    SpringheartCreateInsectV1,
}

impl C8bOpV1 {
    /// Whether the leaf may stop for a player's choice, so it runs in the
    /// resumable interpreter.
    pub(crate) fn contains_player_choice(&self) -> bool {
        matches!(self, Self::SpringheartLandfallV1)
    }
}

fn leaf(op: C8bOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::C8bV1(op)))
}

fn token_id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("eternal catalog token missing: {name}"))
}

// ---- Card programs ------------------------------------------------------------------

/// Vaultborn Tyrant: "you gain 3 life and draw a card." Uses `EffectOp::DrawCards`
/// so Hullbreacher, Narset, Notion Thief, and Dredge replacements work through
/// the resumable draw hook.
pub fn vaultborn_etb() -> EffectOp {
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

/// Vaultborn Tyrant: "When this creature dies, if it's not a token, create a
/// token that's a copy of it, except it's an artifact in addition to its other
/// types."
pub fn vaultborn_death_copy() -> EffectOp {
    leaf(C8bOpV1::VaultbornDeathCopyV1)
}

/// Saheeli, Sublime Artificer: "Whenever you cast a noncreature spell, create
/// a 1/1 colorless Servo artifact creature token."
pub fn saheeli_servo() -> EffectOp {
    EffectOp::CreateToken {
        token_def: token_id(SERVO_TOKEN),
        controller: PlayerRef::Controller,
    }
}

/// Saheeli, Sublime Artificer's -2 ability.
pub fn saheeli_minus_two() -> EffectOp {
    leaf(C8bOpV1::SaheeliCopyV1)
}

/// Springheart Nantuko's Landfall trigger.
pub fn springheart_landfall() -> EffectOp {
    leaf(C8bOpV1::SpringheartLandfallV1)
}

fn green_insect_op() -> EffectOp {
    EffectOp::CreateToken {
        token_def: token_id(GREEN_INSECT_TOKEN),
        controller: PlayerRef::Controller,
    }
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

const VAULTBORN_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(
        TriggerCondition::StandardV1(StandardTriggerV1::EternalV1(EternalTriggerV1::C8bV1(
            C8bTriggerV1::VaultbornEntersV1,
        ))),
        vaultborn_etb,
    ),
    trigger(
        TriggerCondition::LeftBattlefieldToGraveyard,
        vaultborn_death_copy,
    ),
];

const SAHEELI_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(
    TriggerCondition::CastNoncreatureSpell,
    saheeli_servo,
)];

const SPRINGHEART_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(
    TriggerCondition::ControlledLandEnters,
    springheart_landfall,
)];

fn own_triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        VAULTBORN_TYRANT => &VAULTBORN_TRIGGERS,
        SAHEELI => &SAHEELI_TRIGGERS,
        SPRINGHEART_NANTUKO => &SPRINGHEART_TRIGGERS,
        _ => &[],
    }
}

/// Triggered abilities of this module's cards, falling through to wave C8c.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    let own = own_triggers_for(name);
    if !own.is_empty() {
        own
    } else {
        crate::eternal_cube_c8c_v1::triggers_for(name)
    }
}

/// Target specs of this module's printed and queued triggered abilities.
pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    let own = own_triggers_for(name)
        .iter()
        .any(|def| (def.effect)() == *effect)
        || delayed_effect_matches(name, effect);
    own.then_some(TargetSpec::None)
}

/// Queued triggers owned by this module's cards. Note that Phantasmal Image
/// and Superior Spider-Man have the copied permanent's `card_def` when their
/// triggers are queued, so `_name` is intentionally unconstrained.
pub(crate) fn delayed_effect_matches(_name: &str, effect: &EffectOp) -> bool {
    matches!(
        effect,
        EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::C8bV1(
            C8bOpV1::SacrificeSourceV1 | C8bOpV1::SuperiorSpiderManExileV1 { .. }
        )))
    )
}

/// Whether `condition` triggers on its source's own arrival on the battlefield.
pub(crate) fn arrival_condition(condition: C8bTriggerV1) -> bool {
    matches!(condition, C8bTriggerV1::VaultbornEntersV1)
}

/// Intervening-if on Vaultborn Tyrant's dies trigger: "if it's not a token".
/// Checking before trigger publication prevents token copies from ever placing
/// a dies trigger on the stack.
pub(crate) fn suppress_token_death_copy(
    name: &str,
    condition: TriggerCondition,
    is_token: bool,
) -> bool {
    name == VAULTBORN_TYRANT
        && is_token
        && condition == TriggerCondition::LeftBattlefieldToGraveyard
}

/// Evaluates `C8bTriggerV1` against event `index` of `events`.
pub(crate) fn trigger_matches(
    condition: C8bTriggerV1,
    events: &[CommittedEvent],
    index: usize,
    source: ObjectId,
    state: &GameState,
) -> bool {
    match condition {
        C8bTriggerV1::VaultbornEntersV1 => {
            let Some(event) = events.get(index) else {
                return false;
            };
            let (entered, token_controller) = match *event {
                CommittedEvent::ZoneChange {
                    object,
                    to: Zone::Battlefield,
                    ..
                } => (object, None),
                CommittedEvent::CreateToken {
                    object, controller, ..
                } => (object, Some(controller)),
                _ => return false,
            };
            if entered == source {
                return true;
            }
            let Some(live_source) = state.objects.try_get(source) else {
                return false;
            };
            let controller = live_source.controller;
            let Some(entrant) = state.objects.try_get(entered) else {
                return false;
            };
            if entrant.zone == Zone::Battlefield {
                entrant.controller == controller
                    && crate::engine::object_has_type(state, entered, CardType::Creature)
                    && crate::engine::effective_power(state, entered) >= 4
            } else {
                // Creature entered and left before trigger collection (e.g.
                // legend rule or 4/0 toughness); consult its departure stats.
                let entered_controller = token_controller
                    .or_else(|| {
                        events[index + 1..].iter().find_map(|later| match *later {
                            CommittedEvent::ZoneChange {
                                object,
                                from: Zone::Battlefield,
                                controller_before,
                                ..
                            } if object == entered => Some(controller_before),
                            _ => None,
                        })
                    })
                    .unwrap_or(entrant.controller);
                let power = crate::creature_stats_lki_v1::current_or_last_creature_stats(
                    state,
                    EffectObjectBinding {
                        object: entered,
                        expected_zone: Zone::Battlefield,
                        expected_zone_change_count: entrant.zone_change_count.saturating_sub(1),
                    },
                )
                .map(|(p, _)| p);
                entered_controller == controller && power.is_some_and(|p| p >= 4)
            }
        }
    }
}

// ---- Helpers ------------------------------------------------------------------------

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

fn live_ability_source(ctx: &ExecCtx, state: &GameState) -> Option<ObjectId> {
    let contract = ctx.ability_source_contract?;
    let link = ObjectLinkV4 {
        object: contract.source,
        zone_change_count: contract.zone_change_count,
    };
    link_live_in(state, link, Zone::Battlefield).then_some(contract.source)
}

fn halt(state: &mut GameState, source: ObjectId) {
    state.engine.halted = Some((UnsupportedMechanic::InvalidEffectContinuation, source));
}

fn face_object_name(card_def: u16, face_index: u8) -> &'static str {
    let def = &CARD_DEFS[usize::from(card_def)];
    if face_index == 1 {
        def.transform_face
            .as_ref()
            .map_or(def.object_name, |face| face.name)
    } else {
        def.object_name
    }
}

pub(crate) fn current_copy_mods(state: &GameState, link: ObjectLinkV4) -> Option<C8bCopyModsV1> {
    read_state(state).and_then(|c8b| {
        c8b.copy_mods
            .iter()
            .find(|(host, _)| *host == link)
            .map(|(_, mods)| mods.clone())
    })
}

use crate::copy_entry_v1::{capture_battlefield_copiable_values, CopiableValuesV1};

/// Installs `(card_def, face_index, mods)` onto battlefield permanent `host`
/// without changing its zone, counters, damage, attachments, or summoning
/// sickness.
pub(crate) fn apply_copied_characteristics(
    state: &mut GameState,
    host: ObjectId,
    card_def: u16,
    face_index: u8,
    mods: C8bCopyModsV1,
) {
    let link = live_link(state, host);
    let def = &CARD_DEFS[usize::from(card_def)];
    let colors = mana_colors_mask(def.colors_for_face(face_index));
    let is_creature_face =
        def.types_for_face(face_index).contains(&CardType::Creature) || mods.added_creature;
    let mut subtypes: Vec<u16> = def
        .subtypes_for_face(face_index)
        .iter()
        .map(|subtype| subtype.stable_id())
        .collect();
    subtypes.extend(mods.added_subtypes.iter().copied().filter(|&id| {
        is_creature_face
            || !Subtype::from_stable_id(id).is_some_and(Subtype::is_native_creature_type)
    }));
    subtypes.sort_unstable();
    subtypes.dedup();
    let name = mods
        .override_name
        .clone()
        .unwrap_or_else(|| face_object_name(card_def, face_index).to_string());

    let live = state.objects.get_mut(host);
    live.card_def = card_def;
    live.name = name;
    live.v4.face_index = face_index;
    live.v4.effective_color_mask = colors;
    live.v4.effective_subtype_ids = subtypes;

    let copy_mods = &mut c8b_state(state).copy_mods;
    copy_mods.retain(|(existing, _)| *existing != link);
    if !mods.is_empty() {
        copy_mods.push((link, mods));
    }
}

/// Applies the copied permanent's own "enters tapped", "enters with counters",
/// planeswalker starting loyalty, and Saga initial lore counter.
fn apply_copied_entry_effects(
    state: &mut GameState,
    host: ObjectId,
    copied_def: u16,
    face_index: u8,
) {
    let def = &CARD_DEFS[usize::from(copied_def)];
    if def.enters_battlefield_tapped {
        state.objects.get_mut(host).tapped = true;
    }
    if let Some(entry) = def.enters_with_plus_one_counters {
        if !entry.if_kicked {
            let controller = state.objects.get(host).controller;
            if let Ok(amount) =
                crate::doubling_season_v48::entry_amount(state, None, controller, entry.count)
            {
                state.objects.get_mut(host).counters.plus1_plus1 += amount;
            }
        }
    }
    crate::eternal_cards_v1::apply_entry_counters(state, host, None);
    if def
        .types_for_face(face_index)
        .contains(&CardType::Planeswalker)
    {
        crate::planeswalker_v1::after_zone_change(state, host, None);
    }
    if def.saga.is_some()
        && crate::trigger::saga_has_chapter_abilities(state, host)
        && event::add_lore_counters(state, host, 1).is_err()
    {
        halt(state, host);
    }
}

// ---- As-enters copies (Phantasmal Image, Superior Spider-Man) -----------------------

#[derive(Clone, Copy)]
enum EntryCopyKind {
    PhantasmalImage,
    SuperiorSpiderMan,
}

fn phantasmal_image_candidates(state: &GameState, entering: ObjectId) -> Vec<ObjectId> {
    [PlayerId::P0, PlayerId::P1]
        .into_iter()
        .flat_map(|seat| state.players[seat.index()].battlefield.iter().copied())
        .filter(|&object| {
            object != entering
                && state
                    .objects
                    .try_get(object)
                    .is_some_and(|live| live.zone == Zone::Battlefield)
                && crate::engine::object_has_type(state, object, CardType::Creature)
        })
        .collect()
}

fn superior_spider_man_candidates(state: &GameState) -> Vec<ObjectId> {
    [PlayerId::P0, PlayerId::P1]
        .into_iter()
        .flat_map(|seat| state.players[seat.index()].graveyard.iter().copied())
        .filter(|&card| {
            state.objects.try_get(card).is_some_and(|live| {
                live.zone == Zone::Graveyard
                    && !live.v4.is_token
                    && live.spell_copy_origin.is_none()
                    && CARD_DEFS
                        .get(usize::from(live.card_def))
                        .is_some_and(crate::eternal_cube_c9c_v1::is_nonbattlefield_creature_def)
            })
        })
        .collect()
}

fn copy_entry_needing_choice(
    state: &GameState,
) -> Option<(ObjectId, EntryCopyKind, Vec<ObjectId>)> {
    if !cfg!(feature = "eternal-fixtures") {
        return None;
    }
    let answered = read_state(state).map(|c8b| c8b.entry_answered.as_slice());
    for event in &state.engine.event_log {
        let object = match *event {
            CommittedEvent::ZoneChange {
                object,
                to: Zone::Battlefield,
                ..
            } => object,
            CommittedEvent::CreateToken { object, .. } => object,
            _ => continue,
        };
        let Some(live) = state.objects.try_get(object) else {
            continue;
        };
        if live.zone != Zone::Battlefield
            || live.v4.face_index != 0
            || live.v4.face_down_v1.is_some()
        {
            continue;
        }
        let link = ObjectLinkV4 {
            object,
            zone_change_count: live.zone_change_count,
        };
        if answered.is_some_and(|done| done.contains(&link)) {
            continue;
        }
        let kind = match CARD_DEFS.get(usize::from(live.card_def)).map(|d| d.name) {
            Some(PHANTASMAL_IMAGE) => EntryCopyKind::PhantasmalImage,
            Some(SUPERIOR_SPIDER_MAN) => EntryCopyKind::SuperiorSpiderMan,
            _ => continue,
        };
        // CR 614.12: Dress Down removes creature abilities as they enter,
        // suppressing both Phantasmal Image and Superior Spider-Man's copy
        // replacement effects.
        if crate::eternal_legacy_e7a_v1::active_dress_down_timestamp(state, Some(object)).is_some()
        {
            continue;
        }
        let candidates = match kind {
            EntryCopyKind::PhantasmalImage => phantasmal_image_candidates(state, object),
            EntryCopyKind::SuperiorSpiderMan => superior_spider_man_candidates(state),
        };
        if !candidates.is_empty() {
            return Some((object, kind, candidates));
        }
    }
    None
}

/// Whether trigger collection and SBAs wait for Phantasmal Image's or Superior
/// Spider-Man's as-enters copy choice, so the copied creature's ETB triggers
/// fire and 0/0 Phantasmal Image does not die before choosing.
pub(crate) fn defer_triggers(state: &GameState) -> bool {
    copy_entry_needing_choice(state).is_some()
}

/// Pending as-enters copy decision (option 0 declines; option `1..=N` chooses
/// candidate `option - 1`).
pub(crate) fn entry_choice_decision(state: &GameState) -> Option<Decision> {
    let (entering, _, candidates) = copy_entry_needing_choice(state)?;
    Some(Decision::ChooseEffectOption {
        player: state.objects.get(entering).controller,
        source: entering,
        option_count: u16::try_from(candidates.len().saturating_add(1)).unwrap_or(u16::MAX),
    })
}

/// Answers Phantasmal Image's or Superior Spider-Man's pending as-enters copy
/// choice. Returns `Ok(false)` when no C8b entry choice is pending.
pub(crate) fn answer_entry_choice(state: &mut GameState, option: u16) -> Result<bool, String> {
    let Some((entering, kind, candidates)) = copy_entry_needing_choice(state) else {
        return Ok(false);
    };
    let chosen = match option {
        0 => None,
        index => Some(
            *candidates
                .get(usize::from(index) - 1)
                .ok_or("copy entry option is out of range")?,
        ),
    };
    let host_link = live_link(state, entering);
    c8b_state(state).entry_answered.push(host_link);
    let Some(chosen) = chosen else {
        return Ok(true);
    };

    // Save the physical card before applying the copy so departure restores it.
    let (own_def, saved) = {
        let live = state.objects.get(entering);
        (
            live.card_def,
            C8bSavedCopyV1 {
                host: host_link,
                card_def: live.card_def,
                name: live.name.clone(),
                color_mask: live.v4.effective_color_mask,
                subtype_ids: live.v4.effective_subtype_ids.clone(),
                face_index: live.v4.face_index,
            },
        )
    };

    match kind {
        EntryCopyKind::PhantasmalImage => {
            let chosen_link = live_link(state, chosen);
            let Some(values) = capture_battlefield_copiable_values(state, chosen) else {
                return Ok(true);
            };
            let mut mods = values.mods;
            mods.merge_subtypes(&[Subtype::Illusion]);
            mods.illusion_sacrifice = true;
            c8b_state(state).permanent_copies.push(saved);
            apply_copied_characteristics(state, entering, values.card_def, values.face_index, mods);
            if values.prototyped {
                crate::eternal_pauper_pf2c_v1::note_prototyped_entry(state, entering);
            }
            crate::standard_cards_v1::note_copied_definitions(
                state,
                entering,
                host_link.zone_change_count,
                &[own_def, values.card_def],
            );
            if let Some(applied) = capture_battlefield_copiable_values(state, entering) {
                crate::copy_entry_v1::record_entry_copy_applied(
                    state,
                    host_link,
                    chosen_link,
                    applied,
                );
            }
            apply_copied_entry_effects(state, entering, values.card_def, values.face_index);
        }
        EntryCopyKind::SuperiorSpiderMan => {
            let chosen_link = live_link(state, chosen);
            let copied_def = state.objects.get(chosen).card_def;
            let mut mods = C8bCopyModsV1 {
                override_name: Some(SUPERIOR_SPIDER_MAN.to_string()),
                base_pt: Some((4, 4)),
                ..Default::default()
            };
            mods.merge_subtypes(&[Subtype::Spider, Subtype::Human, Subtype::Hero]);
            c8b_state(state).permanent_copies.push(saved);
            apply_copied_characteristics(state, entering, copied_def, 0, mods);
            crate::standard_cards_v1::note_copied_definitions(
                state,
                entering,
                host_link.zone_change_count,
                &[own_def, copied_def],
            );
            if let Some(applied) = capture_battlefield_copiable_values(state, entering) {
                crate::copy_entry_v1::record_entry_copy_applied(
                    state,
                    host_link,
                    chosen_link,
                    applied,
                );
            }
            apply_copied_entry_effects(state, entering, copied_def, 0);
            // Reflexive trigger: "When you do, exile that card."
            let controller = state.objects.get(entering).controller;
            let source = AbilitySourceContractV4::capture(state, entering);
            state
                .engine
                .pending_triggers
                .push(crate::eternal_cards_v1::delayed_trigger(
                    source,
                    controller,
                    leaf(C8bOpV1::SuperiorSpiderManExileV1 { card: chosen_link }),
                ));
        }
    }
    Ok(true)
}

// ---- Targeting hook (Phantasmal Image's Illusion sacrifice trigger) -----------------

/// Called whenever a spell or ability finishes announcing its targets (or
/// retargets). Fires once per stack item for each targeted permanent that has
/// Phantasmal Image's granted sacrifice trigger and active abilities.
pub(crate) fn note_targeting(state: &mut GameState, item: &StackItem) {
    if !cfg!(feature = "eternal-fixtures")
        || item.targets.is_empty()
        || read_state(state).is_none_or(|c8b| c8b.copy_mods.is_empty())
    {
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
        let Some(live) = state.objects.try_get(object) else {
            continue;
        };
        if live.zone != Zone::Battlefield {
            continue;
        }
        let has_illusion_sac = active_copy_mods(state, object)
            .is_some_and(|mods| mods.illusion_sacrifice)
            && crate::continuous_characteristics_v1::printed_abilities_active(state, object);
        if has_illusion_sac {
            let source = AbilitySourceContractV4::capture(state, object);
            let controller = live.controller;
            state
                .engine
                .pending_triggers
                .push(crate::eternal_cards_v1::delayed_trigger(
                    source,
                    controller,
                    leaf(C8bOpV1::SacrificeSourceV1),
                ));
        }
    }
}

// ---- Copy queries, departure, cleanup, and token initialization ---------------------

/// Whether `object` is a battlefield permanent carrying a C8b copy replacement
/// (Phantasmal Image, Superior Spider-Man, or Saheeli -2), so `event.rs` emits
/// `LeftBattlefieldCopyV1` before restoring its pre-copy definition.
pub(crate) fn active_copy(state: &GameState, object: ObjectId) -> bool {
    if !cfg!(feature = "eternal-fixtures") {
        return false;
    }
    let Some(c8b) = read_state(state) else {
        return false;
    };
    if c8b.permanent_copies.is_empty() && c8b.saheeli_copies.is_empty() {
        return false;
    }
    let Some(live) = state.objects.try_get(object) else {
        return false;
    };
    if live.zone != Zone::Battlefield {
        return false;
    }
    let link = ObjectLinkV4 {
        object,
        zone_change_count: live.zone_change_count,
    };
    c8b.permanent_copies.iter().any(|copy| copy.host == link)
        || c8b.saheeli_copies.iter().any(|copy| copy.host == link)
}

fn has_saheeli_copy_link(state: &GameState, link: ObjectLinkV4) -> bool {
    read_state(state).is_some_and(|c8b| c8b.saheeli_copies.iter().any(|copy| copy.host == link))
}

/// Whether `object` currently has a temporary Saheeli -2 copy on top of any
/// earlier copy layers.
pub(crate) fn has_saheeli_copy(state: &GameState, object: ObjectId) -> bool {
    if !cfg!(feature = "eternal-fixtures") {
        return false;
    }
    let Some(live) = state.objects.try_get(object) else {
        return false;
    };
    if live.zone != Zone::Battlefield {
        return false;
    }
    has_saheeli_copy_link(
        state,
        ObjectLinkV4 {
            object,
            zone_change_count: live.zone_change_count,
        },
    )
}

fn active_copy_mods(state: &GameState, object: ObjectId) -> Option<C8bCopyModsV1> {
    let live = state.objects.try_get(object)?;
    if live.zone != Zone::Battlefield {
        return None;
    }
    let link = ObjectLinkV4 {
        object,
        zone_change_count: live.zone_change_count,
    };
    if !has_saheeli_copy_link(state, link)
        && crate::eternal_cube_c8c_v1::is_absorbing_copy(state, object)
    {
        return None;
    }
    current_copy_mods(state, link)
}

/// Whether `object` has `CardType::Artifact` added in Layer 1 by a C8b copy
/// effect (Vaultborn Tyrant's artifact token copy or Saheeli's -2).
pub(crate) fn adds_artifact(state: &GameState, object: ObjectId) -> bool {
    cfg!(feature = "eternal-fixtures")
        && active_copy_mods(state, object).is_some_and(|mods| mods.added_artifact)
}

/// Whether `object` has `CardType::Creature` added in Layer 1 by copying an
/// Absorbing Man copy.
pub(crate) fn adds_creature(state: &GameState, object: ObjectId) -> bool {
    cfg!(feature = "eternal-fixtures")
        && active_copy_mods(state, object).is_some_and(|mods| mods.added_creature)
}

/// Whether `object` has `Keywords::VIGILANCE` added in Layer 1 by copying an
/// Absorbing Man copy.
pub(crate) fn adds_vigilance(state: &GameState, object: ObjectId) -> bool {
    cfg!(feature = "eternal-fixtures")
        && active_copy_mods(state, object).is_some_and(|mods| mods.added_vigilance)
}

/// Layer 1 copy-added Legendary supertype propagated from an Absorbing Man
/// copy.
pub(crate) fn copy_supertypes(state: &GameState, object: ObjectId) -> Option<&'static [Supertype]> {
    if !cfg!(feature = "eternal-fixtures") {
        return None;
    }
    let live = state.objects.try_get(object)?;
    if live.zone != Zone::Battlefield || live.v4.face_down_v1.is_some() {
        return None;
    }
    if !active_copy_mods(state, object).is_some_and(|mods| mods.added_legendary) {
        return None;
    }
    let def = CARD_DEFS.get(usize::from(live.card_def))?;
    let printed = if live.v4.face_index == 1 && def.name == "Ojer Axonil, Deepest Might" {
        &[][..]
    } else {
        def.supertypes
    };
    let has_basic = printed.contains(&Supertype::Basic);
    let has_snow = printed.contains(&Supertype::Snow);
    Some(match (has_basic, has_snow) {
        (true, true) => &[Supertype::Legendary, Supertype::Basic, Supertype::Snow],
        (true, false) => &[Supertype::Legendary, Supertype::Basic],
        (false, true) => &[Supertype::Legendary, Supertype::Snow],
        (false, false) => &[Supertype::Legendary],
    })
}

/// Copiable subtypes added by a C8b copy effect on `object`, if any.
pub(crate) fn copy_added_subtypes(state: &GameState, object: ObjectId) -> Vec<u16> {
    if !cfg!(feature = "eternal-fixtures") {
        return Vec::new();
    }
    active_copy_mods(state, object)
        .map(|mods| mods.added_subtypes)
        .unwrap_or_default()
}

/// Layer 1 copy-modified base P/T (Superior Spider-Man's 4/4). Consulted after
/// Layer 7b setters and before Layer 7a CDAs (CR 707.9d).
pub(crate) fn copy_base_power_toughness(state: &GameState, object: ObjectId) -> Option<(i16, i16)> {
    if !cfg!(feature = "eternal-fixtures") {
        return None;
    }
    active_copy_mods(state, object).and_then(|mods| mods.base_pt)
}

/// Captures last-known copiable values immediately before a nontoken Vaultborn
/// Tyrant dies (before any `end_copy_before_departure` unwinds a copy).
pub(crate) fn before_departure(state: &mut GameState, object: ObjectId, to_zone: Zone) {
    if !cfg!(feature = "eternal-fixtures") || to_zone != Zone::Graveyard {
        return;
    }
    let Some(live) = state.objects.try_get(object) else {
        return;
    };
    if live.zone != Zone::Battlefield || live.v4.is_token {
        return;
    }
    if CARD_DEFS
        .get(usize::from(live.card_def))
        .is_none_or(|def| def.name != VAULTBORN_TYRANT)
    {
        return;
    }
    if !crate::continuous_characteristics_v1::printed_abilities_active(state, object) {
        return;
    }
    let link = live_link(state, object);
    let Some(values) = capture_battlefield_copiable_values(state, object) else {
        return;
    };
    let lki = C8bDeathCopyLkiV1 {
        source: link,
        card_def: values.card_def,
        face_index: values.face_index,
        mods: values.mods,
        copied_end_step_sacrifices_v1: values.copied_end_step_sacrifices_v1,
        copied_nightmare_v1: values.copied_nightmare_v1,
    };
    let records = &mut c8b_state(state).death_lki;
    records.retain(|existing| existing.source != link);
    records.push(lki);
}

/// Unwinds any temporary Saheeli copy immediately before `object` leaves the
/// battlefield (above C8c Absorbing Man and permanent as-entry copies).
pub(crate) fn end_temp_copy_before_departure(state: &mut GameState, object: ObjectId) {
    if !cfg!(feature = "eternal-fixtures") || read_state(state).is_none_or(C8bStateV1::is_empty) {
        return;
    }
    let Some(live) = state.objects.try_get(object) else {
        return;
    };
    let link = ObjectLinkV4 {
        object,
        zone_change_count: live.zone_change_count,
    };
    if let Some(pos) = read_state(state)
        .and_then(|c8b| c8b.saheeli_copies.iter().position(|copy| copy.host == link))
    {
        let saved = c8b_state(state).saheeli_copies.remove(pos);
        let obj = state.objects.get_mut(object);
        obj.card_def = saved.card_def;
        obj.name = saved.name;
        obj.v4.effective_color_mask = saved.color_mask;
        obj.v4.effective_subtype_ids = saved.subtype_ids;
        obj.v4.face_index = saved.face_index;
        let copy_mods = &mut c8b_state(state).copy_mods;
        copy_mods.retain(|(host, _)| *host != link);
        if let Some(prev) = saved.prev_mods {
            if !prev.is_empty() {
                copy_mods.push((link, prev));
            }
        }
        crate::copy_entry_v1::set_covered_by_saheeli(state, object, false);
        crate::eternal_pauper_pf2c_v1::restore_saheeli_prototype(state, object);
    }
}

/// Unwinds any permanent as-entry copy (Phantasmal Image, Superior Spider-Man)
/// immediately before `object` leaves the battlefield (after temporary and
/// Absorbing Man copies unwind), so it arrives in its new zone as its physical
/// card.
pub(crate) fn end_permanent_copy_before_departure(state: &mut GameState, object: ObjectId) {
    if !cfg!(feature = "eternal-fixtures") || read_state(state).is_none_or(C8bStateV1::is_empty) {
        return;
    }
    let Some(live) = state.objects.try_get(object) else {
        return;
    };
    let link = ObjectLinkV4 {
        object,
        zone_change_count: live.zone_change_count,
    };
    if let Some(pos) = read_state(state).and_then(|c8b| {
        c8b.permanent_copies
            .iter()
            .position(|copy| copy.host == link)
    }) {
        let saved = c8b_state(state).permanent_copies.remove(pos);
        let obj = state.objects.get_mut(object);
        obj.card_def = saved.card_def;
        obj.name = saved.name;
        obj.v4.effective_color_mask = saved.color_mask;
        obj.v4.effective_subtype_ids = saved.subtype_ids;
        obj.v4.face_index = saved.face_index;
    }
    let c8b = c8b_state(state);
    c8b.copy_mods.retain(|(host, _)| *host != link);
    c8b.entry_answered.retain(|answered| *answered != link);
}

fn death_lki_referenced(state: &GameState, link: ObjectLinkV4) -> bool {
    let matches_op = |op: &EffectOp| {
        matches!(
            op,
            EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::C8bV1(
                C8bOpV1::VaultbornDeathCopyV1
            )))
        )
    };
    let matches_contract = |contract: AbilitySourceContractV4| {
        contract.source == link.object && contract.zone_change_count == link.zone_change_count
    };
    state.stack.iter().any(|item| {
        item.v4
            .ability_source_contract
            .is_some_and(matches_contract)
            && item.inline_effect.as_ref().is_some_and(matches_op)
    }) || state.engine.pending_triggers.iter().any(|pending| {
        pending.source_contract.is_some_and(matches_contract) && matches_op(&pending.effect)
    }) || state.engine.pending_effect.as_ref().is_some_and(|pending| {
        pending
            .ctx
            .ability_source_contract
            .is_some_and(matches_contract)
            && pending
                .resolving_item
                .inline_effect
                .as_ref()
                .is_some_and(matches_op)
    })
}

/// Cleanup step expiration (CR 514.2): every temporary copy created by
/// Saheeli, Sublime Artificer's -2 ends and restores the permanent's exact
/// pre-Saheeli values (including any underlying permanent copy mods), and
/// unreferenced Vaultborn death LKI entries are pruned.
pub(crate) fn begin_cleanup(state: &mut GameState) {
    if !cfg!(feature = "eternal-fixtures") || read_state(state).is_none_or(C8bStateV1::is_empty) {
        return;
    }
    let copies = std::mem::take(&mut c8b_state(state).saheeli_copies);
    for saved in copies {
        if !link_live_in(state, saved.host, Zone::Battlefield) {
            continue;
        }
        let obj = state.objects.get_mut(saved.host.object);
        obj.card_def = saved.card_def;
        obj.name = saved.name;
        obj.v4.effective_color_mask = saved.color_mask;
        obj.v4.effective_subtype_ids = saved.subtype_ids;
        obj.v4.face_index = saved.face_index;
        let copy_mods = &mut c8b_state(state).copy_mods;
        copy_mods.retain(|(host, _)| *host != saved.host);
        if let Some(prev) = saved.prev_mods {
            if !prev.is_empty() {
                copy_mods.push((saved.host, prev));
            }
        }
        crate::copy_entry_v1::set_covered_by_saheeli(state, saved.host.object, false);
        crate::eternal_pauper_pf2c_v1::restore_saheeli_prototype(state, saved.host.object);
    }
    if read_state(state).is_some_and(|c8b| !c8b.death_lki.is_empty()) {
        let keep: Vec<bool> = read_state(state)
            .map(|c8b| {
                c8b.death_lki
                    .iter()
                    .map(|row| death_lki_referenced(state, row.source))
                    .collect()
            })
            .unwrap_or_default();
        let mut iter = keep.into_iter();
        c8b_state(state)
            .death_lki
            .retain(|_| iter.next().unwrap_or(false));
    }
}

/// Applies staged copiable modifications (`pending_token_mods`) to a newly
/// created token inside `event.rs` BEFORE entry counters and entry observers
/// run, so Doubling Season extra tokens receive the same mods and entry
/// triggers see the final types, subtypes, name, and base P/T.
pub(crate) fn apply_pending_token_mods(state: &mut GameState, object: ObjectId) {
    if !cfg!(feature = "eternal-fixtures") {
        return;
    }
    let Some(mods) = read_state(state).and_then(|c8b| c8b.pending_token_mods.clone()) else {
        return;
    };
    if mods.is_empty() {
        return;
    }
    let Some(live) = state.objects.try_get(object) else {
        return;
    };
    let card_def = live.card_def;
    let face_index = live.v4.face_index;
    apply_copied_characteristics(state, object, card_def, face_index, mods);
}

// ---- Bestow & Detective's Phoenix hooks ---------------------------------------------

/// Static P/T bonus granted to the enchanted creature by an attached C8b
/// Bestow Aura (Springheart Nantuko: +1/+1; Detective's Phoenix: +2/+2),
/// checking that the Aura's own printed abilities are active.
pub(crate) fn bestow_host_pt_bonus(state: &GameState, aura: ObjectId) -> Option<i128> {
    if !cfg!(feature = "eternal-fixtures") {
        return None;
    }
    if !crate::continuous_characteristics_v1::printed_abilities_active(state, aura) {
        return None;
    }
    let live = state.objects.try_get(aura)?;
    match CARD_DEFS.get(usize::from(live.card_def))?.name {
        SPRINGHEART_NANTUKO => Some(1),
        DETECTIVES_PHOENIX => Some(2),
        _ => None,
    }
}

/// Collect evidence minimum required as part of `name`'s Bestow cost, if any.
pub(crate) fn bestow_collect_evidence_minimum(name: &str) -> Option<u16> {
    if cfg!(feature = "eternal-fixtures") && name == DETECTIVES_PHOENIX {
        Some(6)
    } else {
        None
    }
}

/// Whether `name` may be cast from its owner's graveyard using its Bestow
/// ability (Detective's Phoenix).
pub(crate) fn is_graveyard_bestow_card(name: &str) -> bool {
    cfg!(feature = "eternal-fixtures") && name == DETECTIVES_PHOENIX
}

/// Whether `player` has Detective's Phoenix's own permission to cast `object`
/// from their graveyard using Bestow.
pub(crate) fn graveyard_bestow_allowed(
    state: &GameState,
    object: ObjectId,
    player: PlayerId,
) -> bool {
    if !cfg!(feature = "eternal-fixtures") {
        return false;
    }
    state.objects.try_get(object).is_some_and(|live| {
        live.zone == Zone::Graveyard
            && live.owner == player
            && !live.v4.is_token
            && live.spell_copy_origin.is_none()
            && CARD_DEFS
                .get(usize::from(live.card_def))
                .is_some_and(|def| def.name == DETECTIVES_PHOENIX)
    })
}

/// CR 702.103e / 704.5m: a Bestow Aura attached to a noncreature or a host
/// with protection/forbidden attachment immediately becomes unattached and
/// turns back into an Enchantment Creature rather than going to the graveyard.
pub(crate) fn unattach_illegal_bestow(state: &mut GameState) -> bool {
    if !cfg!(feature = "eternal-fixtures") {
        return false;
    }
    let detach: Vec<_> = state
        .objects
        .iter()
        .filter_map(|(id, live)| {
            let def = CARD_DEFS.get(usize::from(live.card_def))?;
            let link = live.v4.attached_to?;
            if live.zone != Zone::Battlefield || def.bestow.is_none() {
                return None;
            }
            let host_live = state.objects.try_get(link.object).is_some_and(|host| {
                host.zone == Zone::Battlefield
                    && host.zone_change_count == link.zone_change_count
                    && host.attachments.contains(&id)
            });
            let illegal = !host_live
                || !crate::engine::object_has_type(state, link.object, CardType::Creature)
                || crate::progenitus_v1::attachment_forbidden(state, id, link.object)
                || crate::eternal_pauper_e6b_v1::protection_removes_attachment(
                    state,
                    id,
                    link.object,
                );
            illegal.then_some((id, link))
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
        let aura_link = live_link(state, id);
        let extra = current_copy_mods(state, aura_link)
            .map(|mods| mods.added_subtypes)
            .unwrap_or_default();
        let live = state.objects.get_mut(id);
        live.v4.attached_to = None;
        let def = &CARD_DEFS[usize::from(live.card_def)];
        let mut subtypes: Vec<u16> = def
            .subtypes_for_face(live.v4.face_index)
            .iter()
            .map(|subtype| subtype.stable_id())
            .collect();
        subtypes.extend(extra);
        subtypes.sort_unstable();
        subtypes.dedup();
        live.v4.effective_subtype_ids = subtypes;
    }
    !detach.is_empty()
}

// ---- Token-copy creation helper -----------------------------------------------------

/// Creates a token copy of `(card_def, face_index, mods, ...)` under
/// `controller`, applying `mods` during token creation before entry observers
/// run. Returns whether at least one token was created.
fn create_token_copy(
    state: &mut GameState,
    source: ObjectId,
    controller: PlayerId,
    producer: AbilitySourceContractV4,
    donor: ObjectLinkV4,
    values: CopiableValuesV1,
    stack_item: Option<crate::ids::StackItemId>,
) -> bool {
    let fallback_values = values.clone();
    let CopiableValuesV1 {
        card_def,
        face_index,
        mods,
        copied_end_step_sacrifices_v1,
        copied_nightmare_v1,
        prototyped,
    } = values;
    let mut proposal = ProposedEvent::create_token(card_def, controller);
    if let ProposedEvent::CreateToken(token) = &mut proposal {
        token.entry_face_index = (face_index != 0).then_some(face_index);
        token.initialization_v48 = Some(crate::doubling_season_v48::TokenEntryV48 {
            copied: true,
            copied_end_step_sacrifices_v1,
            copied_nightmare_v1,
            ..Default::default()
        });
    }
    if crate::doubling_season_v48::preflight_token_initialization(
        state,
        std::slice::from_ref(&proposal),
    )
    .is_err()
        || crate::doubling_season_v48::preflight_entry_roster(
            state,
            std::slice::from_ref(&proposal),
        )
        .is_err()
    {
        halt(state, source);
        return false;
    }
    let history_before = state.engine.event_history.len();
    if !mods.is_empty() {
        c8b_state(state).pending_token_mods = Some(mods);
    }
    if prototyped {
        crate::eternal_pauper_pf2c_v1::set_pending_token_prototyped(state, true);
    }
    event::propose_and_commit_batch(state, vec![proposal]);
    if prototyped {
        crate::eternal_pauper_pf2c_v1::set_pending_token_prototyped(state, false);
    }
    if let Some(eternal) = state.eternal_v1.as_mut() {
        eternal.c8b.pending_token_mods = None;
    }
    let created: Vec<(usize, ObjectId)> = state.engine.event_history[history_before..]
        .iter()
        .enumerate()
        .filter_map(|(offset, ev)| match ev {
            CommittedEvent::CreateToken {
                object,
                controller: c,
                ..
            } if *c == controller => Some((history_before + offset, *object)),
            _ => None,
        })
        .collect();
    if let Some(stack_item) = stack_item {
        for &(create_at, object) in &created {
            let applied = capture_battlefield_copiable_values(state, object)
                .unwrap_or_else(|| fallback_values.clone());
            crate::copy_entry_v1::record_token_copy_created(
                state,
                ObjectLinkV4 {
                    object,
                    zone_change_count: 0,
                },
                create_at,
                producer,
                donor,
                applied,
                stack_item,
            );
        }
    }
    !created.is_empty()
}

// ---- Resolution helpers -------------------------------------------------------------

fn resolve_saheeli_copy(ctx: &ExecCtx, state: &mut GameState) {
    let Some(source_contract) = ctx.ability_source_contract else {
        return;
    };
    if ctx.targets.len() < 2 {
        return;
    }
    let (Target::Object(target_artifact), Target::Object(copied_perm)) =
        (ctx.targets[0], ctx.targets[1])
    else {
        return;
    };
    if target_artifact == copied_perm {
        return;
    }
    if !ctx.target_incarnation_matches(0, state) || !ctx.target_incarnation_matches(1, state) {
        return;
    }
    let t0_legal = crate::engine::effect_target_is_legal_from_ability_source(
        state,
        source_contract,
        ctx.controller,
        SAHEELI_TARGET,
        &ctx.targets,
        0,
    );
    let t1_legal = crate::engine::effect_target_is_legal_from_ability_source(
        state,
        source_contract,
        ctx.controller,
        SAHEELI_TARGET,
        &ctx.targets,
        1,
    );
    if !t0_legal || !t1_legal {
        return;
    }
    let Some(values) = capture_battlefield_copiable_values(state, copied_perm) else {
        return;
    };
    let host_link = live_link(state, target_artifact);
    let model_link = live_link(state, copied_perm);
    let already_saved = read_state(state)
        .is_some_and(|c8b| c8b.saheeli_copies.iter().any(|copy| copy.host == host_link));
    let own_def = state.objects.get(target_artifact).card_def;
    if !already_saved {
        crate::eternal_pauper_pf2c_v1::save_saheeli_prototype(state, target_artifact);
        let prev_mods = current_copy_mods(state, host_link);
        let live = state.objects.get(target_artifact);
        let saved = C8bTempCopyV1 {
            host: host_link,
            card_def: live.card_def,
            name: live.name.clone(),
            color_mask: live.v4.effective_color_mask,
            subtype_ids: live.v4.effective_subtype_ids.clone(),
            face_index: live.v4.face_index,
            prev_mods,
        };
        c8b_state(state).saheeli_copies.push(saved);
    }
    let mut mods = values.mods;
    mods.added_artifact = true;
    apply_copied_characteristics(
        state,
        target_artifact,
        values.card_def,
        values.face_index,
        mods,
    );
    crate::copy_entry_v1::set_covered_by_saheeli(state, target_artifact, true);
    if values.prototyped {
        crate::eternal_pauper_pf2c_v1::note_prototyped_entry(state, target_artifact);
    } else {
        crate::eternal_pauper_pf2c_v1::clear_prototyped(state, target_artifact);
    }
    crate::standard_cards_v1::note_copied_definitions(
        state,
        target_artifact,
        host_link.zone_change_count,
        &[own_def, values.card_def],
    );
    if let (Some(stack_item), Some(applied)) = (
        ctx.stack_item_id,
        capture_battlefield_copiable_values(state, target_artifact),
    ) {
        crate::copy_entry_v1::record_saheeli_applied(
            state,
            source_contract,
            stack_item,
            host_link,
            model_link,
            applied,
        );
    }
}

fn resolve_vaultborn_death_copy(ctx: &ExecCtx, state: &mut GameState) {
    let Some(contract) = ctx.ability_source_contract else {
        return;
    };
    let link = ObjectLinkV4 {
        object: contract.source,
        zone_change_count: contract.zone_change_count,
    };
    let lki = read_state(state)
        .and_then(|c8b| c8b.death_lki.iter().find(|row| row.source == link))
        .cloned();
    let (card_def, face_index, mut mods, copied_sacrifices, copied_nightmare) = match lki {
        Some(row) => (
            row.card_def,
            row.face_index,
            row.mods,
            row.copied_end_step_sacrifices_v1,
            row.copied_nightmare_v1,
        ),
        None => {
            // Fallback for a synthetic test trigger whose source did not go
            // through `before_departure`: verify the source is not a token.
            if state
                .objects
                .try_get(contract.source)
                .is_some_and(|live| live.v4.is_token)
            {
                return;
            }
            (contract.card_def, 0, C8bCopyModsV1::default(), 0, false)
        }
    };
    mods.added_artifact = true;
    create_token_copy(
        state,
        ctx.source,
        ctx.controller,
        contract,
        link,
        CopiableValuesV1 {
            card_def,
            face_index,
            mods,
            copied_end_step_sacrifices_v1: copied_sacrifices,
            copied_nightmare_v1: copied_nightmare,
            prototyped: false,
        },
        ctx.stack_item_id,
    );
}

// ---- Execution ----------------------------------------------------------------------

/// Runs one of this module's leaves in synchronous execution.
pub(crate) fn execute(op: &C8bOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        C8bOpV1::SacrificeSourceV1 => {
            let Some(source) = live_ability_source(ctx, state) else {
                return;
            };
            event::log_sacrifice(state, source);
            event::propose_and_commit(state, ProposedEvent::zone_change(source, Zone::Graveyard));
        }
        C8bOpV1::SuperiorSpiderManExileV1 { card } => {
            if link_live_in(state, *card, Zone::Graveyard) {
                event::propose_and_commit(
                    state,
                    ProposedEvent::zone_change(card.object, Zone::Exile),
                );
            }
        }
        C8bOpV1::VaultbornDeathCopyV1 => resolve_vaultborn_death_copy(ctx, state),
        C8bOpV1::SaheeliCopyV1 => resolve_saheeli_copy(ctx, state),
        C8bOpV1::SpringheartPayAndCopyV1 { host } => {
            let paid = crate::engine::pay_effect_mana(ctx.controller, &[ManaColor::G], 1, state);
            let created = if paid && link_live_in(state, *host, Zone::Battlefield) {
                if let Some(values) = capture_battlefield_copiable_values(state, host.object) {
                    let producer = ctx
                        .ability_source_contract
                        .unwrap_or_else(|| AbilitySourceContractV4::capture(state, ctx.source));
                    create_token_copy(
                        state,
                        ctx.source,
                        ctx.controller,
                        producer,
                        *host,
                        values,
                        ctx.stack_item_id,
                    )
                } else {
                    false
                }
            } else {
                false
            };
            if !created && state.engine.halted.is_none() {
                crate::effect::execute(&green_insect_op(), ctx, state);
            }
        }
        C8bOpV1::SpringheartCreateInsectV1 => {
            crate::effect::execute(&green_insect_op(), ctx, state);
        }
        // `SpringheartLandfallV1` runs in the resumable interpreter (`drive`).
        C8bOpV1::SpringheartLandfallV1 => halt(state, ctx.source),
    }
}

/// Runs one of this module's leaves in the resumable interpreter.
pub(crate) fn drive(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    op: &C8bOpV1,
    path: Vec<u16>,
) -> Result<bool, String> {
    match op {
        C8bOpV1::SpringheartLandfallV1 => {
            let controller = continuation.ctx.controller;
            let attached_host = live_ability_source(&continuation.ctx, state).and_then(|nantuko| {
                let link = state.objects.get(nantuko).v4.attached_to?;
                let host = state.objects.try_get(link.object)?;
                (host.zone == Zone::Battlefield
                    && host.zone_change_count == link.zone_change_count
                    && host.attachments.contains(&nantuko)
                    && host.controller == controller
                    && crate::engine::object_has_type(state, link.object, CardType::Creature))
                .then_some(link)
            });
            if let Some(host_link) = attached_host {
                if crate::engine::can_pay_effect_mana(controller, &[ManaColor::G], 1, state) {
                    // Option 0: pay {1}{G} and create a token copy of the
                    // enchanted creature. Option 1: decline and create a 1/1
                    // Green Insect Token.
                    continuation.frames.push(EffectFrame::Program {
                        op: EffectOp::Choice {
                            controller: PlayerRef::Controller,
                            options: vec![
                                leaf(C8bOpV1::SpringheartPayAndCopyV1 { host: host_link }),
                                leaf(C8bOpV1::SpringheartCreateInsectV1),
                            ],
                        },
                        path,
                    });
                    return Ok(false);
                }
            }
            crate::effect::execute(&green_insect_op(), &continuation.ctx, state);
            Ok(false)
        }
        other => {
            execute(other, &continuation.ctx, state);
            Ok(false)
        }
    }
}
