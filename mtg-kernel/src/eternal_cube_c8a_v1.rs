//! Card programs for the eternal catalog's Vintage Cube wave C8a
//! (`docs/design/eternal_formats_support_v1.md`): Adventure, transform DFCs,
//! modal DFC, and prepare cards.
//!
//! `eternal_cards_v1` forwards this module's effect leaves, triggers and
//! turn bookkeeping here, and the recipe tables of `build_eternal_c8a_v1.rs`
//! name its programs. Every table is keyed by a printed name that occurs
//! only in the eternal registry, so the Pauper, FDN and Standard catalogs
//! never reach it. Behavior was read from each card's current Oracle text
//! and, where present, its XMage source.
//!
//! Where each card lives:
//!
//! - **Bonecrusher Giant // Stomp**: Stomp is a `{1}{R}` Instant Adventure
//!   (`EffectOp::DamageCannotBePreventedThisTurn` followed by 2 damage to any
//!   target); the front creature's spell-target trigger is captured in
//!   `note_targeting` (spells only, any controller, deduplicated per stack
//!   item, capturing the targeting spell's controller so it survives the
//!   spell leaving the stack).
//! - **Concealing Curtains // Revealing Eye**: Defender is a keyword row;
//!   `{2}{B}: Transform` transforms face 0 to face 1 in place (CR 701.28f:
//!   does nothing if already transformed); `CommittedEvent::Transformed` to
//!   face 1 queues `RevealingEyeTransformV1`, which rechecks `TargetOpponent`
//!   legality, publicly reveals their hand, offers an optional nonland
//!   discard (`min: 0, max: 1`), and on discard pushes a resumable
//!   `EffectOp::DrawCards` frame so Dredge/Hullbreacher/Narset hooks apply.
//! - **Jace, Vryn's Prodigy // Jace, Telepath Unbound**: `{T}` runs a
//!   resumable draw-1, discard-1, then `JaceMaybeTransformV1` (if 5+ cards in
//!   controller's graveyard and Jace's face-0 incarnation is still on the
//!   battlefield, exile and return transformed under owner's control with 5
//!   starting loyalty); `+1` installs `-2/-0` until controller's next turn
//!   (`power_modifier`, pruned in `begin_turn`); `-3` grants a turn-long
//!   graveyard cast permission (`jace_graveyard_permission`) with an
//!   exile-instead-of-graveyard departure replacement (`exiles_on_departure`);
//!   `-9` creates a persistent spell-cast mill-5 emblem (`jace_emblems`).
//! - **Jennifer Walters // The Sensational She-Hulk**: Static "Your opponents
//!   can't cast spells during your turn" applies on BOTH faces
//!   (`cast_prohibited`); `{3}{G}{W}{W}` transforms face 0 to face 1 at
//!   sorcery speed, and casting via `CastMethodV4::Alternative` casts the
//!   modal DFC back face directly (`adjust_zone_change`, MV 6, G/W); back
//!   face captures creature damage before SBAs (`note_damage_dealt`), offers
//!   an optional damage choice (`SheHulkMayDamageV1`), and consumes the
//!   per-incarnation once-each-turn cap only when accepted.
//! - **Emeritus of Abundance // Regrowth** and **Emeritus of Ideation //
//!   Ancestral Recall**: both enter prepared via `eternal_vintage_e7b_v1`;
//!   Regrowth returns a legal target card from controller's graveyard to
//!   hand; Abundance has an intervening-if 8-land attack trigger
//!   (`observes_attacks`, rechecked on resolution); Ideation has an optional
//!   exile-eight-from-graveyard attack trigger (decline or exact 8-card
//!   exile) that calls `eternal_vintage_e7b_v1::prepare`.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

use crate::card_def::{CardDef, CardType, TargetSpec, CARD_DEFS};
use crate::effect::{
    EffectContinuation, EffectFrame, EffectObjectBinding, EffectOp, ExecCtx, PlayerRef, TargetRef,
};
use crate::eternal_cards_v1::e3::{E3ChoiceKindV1, E3ChoiceV1};
use crate::eternal_cards_v1::EternalOpV1;
use crate::eternal_cube_c6c_v1::EmblemV1;
use crate::eternal_legacy_e9a_v1::TurnV1;
use crate::event::{self, CommittedEvent, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::mana::ManaColor;
use crate::standard_cards_v1::{StandardOpV1, StandardTargetV1};
use crate::state::{
    AbilitySourceContractV4, CastMethodV4, GameState, ObjectLinkV4, SpellCastRouteV4, StackItem,
    StackItemKind, Target, Zone,
};
use crate::trigger::{PendingTrigger, TriggerCondition, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};

pub(crate) const BONECRUSHER_GIANT: &str = "Bonecrusher Giant";
pub(crate) const CONCEALING_CURTAINS: &str = "Concealing Curtains";
pub(crate) const JACE_VRYNS_PRODIGY: &str = "Jace, Vryn's Prodigy";
pub(crate) const JENNIFER_WALTERS: &str = "Jennifer Walters";
pub(crate) const EMERITUS_OF_ABUNDANCE: &str = "Emeritus of Abundance";
pub(crate) const EMERITUS_OF_IDEATION: &str = "Emeritus of Ideation";

// ---- State --------------------------------------------------------------------------

/// Queued Bonecrusher Giant spell-target trigger, capturing the targeting
/// spell's controller so it survives the spell leaving the stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BonecrusherTargetV1 {
    pub source: AbilitySourceContractV4,
    pub controller: PlayerId,
    pub spell_controller: PlayerId,
}

/// Jace, Telepath Unbound +1: `-2/-0` on one battlefield creature incarnation
/// until `holder`'s next turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JaceWeakenV1 {
    pub object: ObjectLinkV4,
    pub holder: PlayerId,
}

/// Jace, Telepath Unbound -3: turn-long graveyard cast permission and
/// exile-instead-of-graveyard replacement for one exact graveyard card
/// incarnation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JaceGraveyardCastV1 {
    pub card: ObjectLinkV4,
    pub holder: PlayerId,
    pub turn: TurnV1,
}

/// Queued The Sensational She-Hulk damage trigger captured before SBAs so LKI
/// works even if She-Hulk or the damaged creature dies in combat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SheHulkDamageV1 {
    pub source: AbilitySourceContractV4,
    pub controller: PlayerId,
    pub amount: i32,
}

/// Per-game state owned by wave C8a, kept inside
/// `eternal_cards_v1::EternalStateV1`. Absent from serialization and hashes to
/// zero bytes while empty so every game without these cards keeps its exact
/// snapshot and hash.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct C8aStateV1 {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bonecrusher_targets: Vec<BonecrusherTargetV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub jace_weakened: Vec<JaceWeakenV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub jace_graveyard_casts: Vec<JaceGraveyardCastV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub jace_emblems: Vec<EmblemV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub she_hulk_damage_queue: Vec<SheHulkDamageV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub she_hulk_used: Vec<(ObjectLinkV4, TurnV1)>,
}

impl C8aStateV1 {
    pub(crate) fn is_empty(&self) -> bool {
        self.bonecrusher_targets.is_empty()
            && self.jace_weakened.is_empty()
            && self.jace_graveyard_casts.is_empty()
            && self.jace_emblems.is_empty()
            && self.she_hulk_damage_queue.is_empty()
            && self.she_hulk_used.is_empty()
    }
}

/// An empty state hashes to zero bytes, preserving every existing state hash.
impl Hash for C8aStateV1 {
    fn hash<H: Hasher>(&self, state: &mut H) {
        if !self.is_empty() {
            b"eternal-cube-c8a/v1".hash(state);
            self.bonecrusher_targets.hash(state);
            self.jace_weakened.hash(state);
            self.jace_graveyard_casts.hash(state);
            self.jace_emblems.hash(state);
            self.she_hulk_damage_queue.hash(state);
            self.she_hulk_used.hash(state);
        }
    }
}

fn read_state(state: &GameState) -> Option<&C8aStateV1> {
    state.eternal_v1.as_ref().map(|eternal| &eternal.c8a)
}

fn c8a_state(state: &mut GameState) -> &mut C8aStateV1 {
    &mut state.eternal_v1.get_or_insert_with(Default::default).c8a
}

// ---- Ops and choices ----------------------------------------------------------------

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::C8aV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum C8aOpV1 {
    /// Bonecrusher Giant: deals 2 damage to the targeting spell's controller.
    BonecrusherDamageV1 { player: PlayerId },
    /// Concealing Curtains / Jennifer Walters: transform face 0 -> face 1 in
    /// place (CR 701.28f: no-op if already transformed).
    TransformFrontToBackV1,
    /// Revealing Eye transform trigger: recheck TargetOpponent, reveal hand,
    /// optional nonland discard -> resumable draw 1.
    RevealingEyeTransformV1,
    /// Jace, Vryn's Prodigy: after draw + discard, if 5+ cards in controller's
    /// graveyard, exile Jace and return transformed under owner's control.
    JaceMaybeTransformV1,
    /// Jace, Telepath Unbound +1: up to one target creature gets -2/-0 until
    /// controller's next turn.
    JacePlusOneV1,
    /// Jace, Telepath Unbound -3: grant turn-long graveyard cast permission
    /// with exile-instead-of-graveyard replacement to target instant/sorcery.
    JaceMinusThreeV1,
    /// Jace, Telepath Unbound -9: create persistent spell-cast mill-5 emblem.
    JaceMinusNineV1,
    /// Jace, Telepath Unbound emblem trigger: recheck TargetOpponent, mill 5.
    JaceEmblemMillV1,
    /// The Sensational She-Hulk trigger: if not yet used this turn and target
    /// is still legal, offer optional choice to deal `amount` damage.
    SheHulkMayDamageV1 { amount: i32 },
    /// Accepted branch of She-Hulk's optional trigger: mark once-per-turn used
    /// and deal `amount` damage to target 0.
    SheHulkCommitDamageV1 { amount: i32 },
    /// Regrowth: return target card from controller's graveyard to hand.
    RegrowthV1,
    /// Emeritus of Abundance attack trigger: recheck >= 8 lands, become
    /// prepared.
    EmeritusAbundancePrepareV1,
    /// Emeritus of Ideation attack trigger: if >= 8 cards in graveyard, offer
    /// optional choice to exile 8 cards.
    EmeritusIdeationAttackV1,
    /// Accepted branch of Emeritus of Ideation's attack trigger: exile exact
    /// 8 cards from controller's graveyard, then become prepared.
    EmeritusIdeationExileEightV1,
}

impl C8aOpV1 {
    /// Whether the leaf may stop for a player's choice (or push a resumable
    /// mill/choice frame), so it runs in the resumable interpreter.
    pub(crate) fn contains_player_choice(&self) -> bool {
        matches!(
            self,
            Self::RevealingEyeTransformV1
                | Self::JaceEmblemMillV1
                | Self::SheHulkMayDamageV1 { .. }
                | Self::EmeritusIdeationAttackV1
                | Self::EmeritusIdeationExileEightV1
        )
    }
}

/// Resolution choices owned by this module, carried as
/// `E3ChoiceKindV1::C8aV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum C8aChoiceV1 {
    /// Revealing Eye: optional choice of a nonland card from the target
    /// opponent's just-revealed hand.
    RevealingEyeChooseNonlandV1 { opponent: PlayerId },
    /// Emeritus of Ideation: choose exactly 8 cards from controller's
    /// graveyard to exile.
    EmeritusIdeationExileEightV1,
}

impl C8aChoiceV1 {
    pub(crate) fn chooser_private(&self) -> bool {
        match self {
            // Opponent's hand was publicly revealed by Revealing Eye; graveyard
            // is a public zone.
            Self::RevealingEyeChooseNonlandV1 { .. } | Self::EmeritusIdeationExileEightV1 => false,
        }
    }

    pub(crate) fn public_purpose(&self) -> crate::rl::TargetSelectionPurposeV4 {
        crate::rl::TargetSelectionPurposeV4::CardSelection
    }

    pub(crate) fn referenced_bindings(&self) -> Vec<EffectObjectBinding> {
        Vec::new()
    }
}

// ---- Helpers ------------------------------------------------------------------------

fn leaf(op: C8aOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::C8aV1(op)))
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

fn binding_link(binding: &EffectObjectBinding) -> ObjectLinkV4 {
    ObjectLinkV4 {
        object: binding.object,
        zone_change_count: binding.expected_zone_change_count,
    }
}

fn definition_name(state: &GameState, object: ObjectId) -> Option<&'static str> {
    let live = state.objects.try_get(object)?;
    CARD_DEFS
        .get(usize::from(live.card_def))
        .filter(|def| def.is_executable())
        .map(|def| def.name)
}

fn active_named_face(state: &GameState, object: ObjectId, name: &str, face_index: u8) -> bool {
    state.objects.try_get(object).is_some_and(|live| {
        live.zone == Zone::Battlefield
            && live.v4.face_index == face_index
            && live.v4.face_down_v1.is_none()
            && CARD_DEFS
                .get(usize::from(live.card_def))
                .is_some_and(|def| def.name == name && def.is_executable())
            && crate::continuous_characteristics_v1::printed_abilities_active(state, object)
    })
}

fn active_named(state: &GameState, object: ObjectId, name: &str) -> bool {
    active_named_face(state, object, name, 0)
}

fn battlefield(state: &GameState) -> impl Iterator<Item = ObjectId> + '_ {
    [PlayerId::P0, PlayerId::P1]
        .into_iter()
        .flat_map(move |player| state.players[player.index()].battlefield.iter().copied())
}

fn live_ability_source(ctx: &ExecCtx, state: &GameState) -> Option<ObjectId> {
    let contract = ctx.ability_source_contract?;
    let live = state.objects.try_get(contract.source)?;
    (live.zone == Zone::Battlefield && live.zone_change_count == contract.zone_change_count)
        .then_some(contract.source)
}

fn graveyard_cards(state: &GameState, player: PlayerId) -> Vec<ObjectId> {
    state.players[player.index()]
        .graveyard
        .iter()
        .copied()
        .filter(|&card| {
            let live = state.objects.get(card);
            live.zone == Zone::Graveyard
                && live.owner == player
                && !live.v4.is_token
                && live.spell_copy_origin.is_none()
        })
        .collect()
}

fn controlled_land_count(state: &GameState, player: PlayerId) -> usize {
    state.players[player.index()]
        .battlefield
        .iter()
        .copied()
        .filter(|&object| {
            let live = state.objects.get(object);
            live.zone == Zone::Battlefield
                && live.controller == player
                && crate::engine::object_has_type(state, object, CardType::Land)
        })
        .count()
}

fn she_hulk_already_used(state: &GameState, link: ObjectLinkV4) -> bool {
    let now = TurnV1::now(state);
    read_state(state).is_some_and(|c8a| c8a.she_hulk_used.contains(&(link, now)))
}

fn is_she_hulk_stack_cast(state: &GameState, object: ObjectId) -> bool {
    let Some(live) = state.objects.try_get(object) else {
        return false;
    };
    if live.zone != Zone::Stack || definition_name(state, object) != Some(JENNIFER_WALTERS) {
        return false;
    }
    if live
        .v4
        .spell_cast_origin
        .and_then(|origin| origin.finalized_method)
        == Some(CastMethodV4::Alternative)
    {
        return true;
    }
    state.engine.pending_cast.as_ref().is_some_and(|pending| {
        pending.spell == object && pending.cast_mode == Some(crate::engine::CastMode::Alternative)
    })
}

fn halt(state: &mut GameState, source: ObjectId) {
    state.engine.halted = Some((
        crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
        source,
    ));
}

// ---- Card programs ------------------------------------------------------------------

/// Stomp (Bonecrusher Giant's Adventure): "Damage can't be prevented this turn.
/// Stomp deals 2 damage to any target."
pub fn stomp() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::DamageCannotBePreventedThisTurn,
        EffectOp::DealDamage {
            target: TargetRef::Target(0),
            amount: 2,
        },
    ])
}

/// Concealing Curtains / Jennifer Walters: transform front face to back face
/// in place.
pub fn transform_front_to_back() -> EffectOp {
    leaf(C8aOpV1::TransformFrontToBackV1)
}

/// Jace, Vryn's Prodigy: "{T}: Draw a card, then discard a card. If there are
/// five or more cards in your graveyard, exile Jace, then return him to the
/// battlefield transformed under his owner's control."
///
/// The draw is a direct root `Sequence` element so Dredge, Hullbreacher,
/// Narset, Jace, and Quantum Riddler all apply natively.
pub fn jace_loot_transform() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 1,
        },
        EffectOp::DiscardCards {
            player: PlayerRef::Controller,
            count: 1,
        },
        leaf(C8aOpV1::JaceMaybeTransformV1),
    ])
}

/// Jace, Telepath Unbound +1: "Up to one target creature gets -2/-0 until your
/// next turn."
pub fn jace_plus_one() -> EffectOp {
    leaf(C8aOpV1::JacePlusOneV1)
}

/// Jace, Telepath Unbound -3: "You may cast target instant or sorcery card from
/// your graveyard this turn. If that spell would be put into your graveyard,
/// exile it instead."
pub fn jace_minus_three() -> EffectOp {
    leaf(C8aOpV1::JaceMinusThreeV1)
}

/// Jace, Telepath Unbound -9: "You get an emblem with 'Whenever you cast a
/// spell, target opponent mills five cards.'"
pub fn jace_minus_nine() -> EffectOp {
    leaf(C8aOpV1::JaceMinusNineV1)
}

/// Regrowth (Emeritus of Abundance's prepare spell): "Return target card from
/// your graveyard to your hand."
pub fn regrowth() -> EffectOp {
    leaf(C8aOpV1::RegrowthV1)
}

fn revealing_eye_transform() -> EffectOp {
    leaf(C8aOpV1::RevealingEyeTransformV1)
}

fn jace_emblem_mill() -> EffectOp {
    leaf(C8aOpV1::JaceEmblemMillV1)
}

fn emeritus_abundance_prepare() -> EffectOp {
    leaf(C8aOpV1::EmeritusAbundancePrepareV1)
}

fn emeritus_ideation_attack() -> EffectOp {
    leaf(C8aOpV1::EmeritusIdeationAttackV1)
}

// ---- Triggered abilities ------------------------------------------------------------

const EMERITUS_IDEATION_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::Attacks,
    home_zone: Zone::Battlefield,
    intervening_if_kicked: false,
    intervening_if_controls_another_source_card: false,
    face_index: 0,
    effect: emeritus_ideation_attack,
}];

/// Triggered abilities of this module's cards, falling through to wave C8b.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        EMERITUS_OF_IDEATION => &EMERITUS_IDEATION_TRIGGERS,
        _ => crate::eternal_cube_c8b_v1::triggers_for(name),
    }
}

/// Target specs of this module's printed and queued triggered abilities.
pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    if name == CONCEALING_CURTAINS && *effect == revealing_eye_transform() {
        return Some(TargetSpec::TargetOpponent);
    }
    if name == JACE_VRYNS_PRODIGY && *effect == jace_emblem_mill() {
        return Some(TargetSpec::TargetOpponent);
    }
    if name == JENNIFER_WALTERS
        && matches!(
            effect,
            EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::C8aV1(
                C8aOpV1::SheHulkMayDamageV1 { .. }
            )))
        )
    {
        return Some(TargetSpec::AnyTarget);
    }
    if name == EMERITUS_OF_IDEATION && *effect == emeritus_ideation_attack() {
        return Some(TargetSpec::None);
    }
    delayed_effect_matches(name, effect).then_some(TargetSpec::None)
}

/// Queued triggers owned by this module's cards.
pub(crate) fn delayed_effect_matches(name: &str, effect: &EffectOp) -> bool {
    let EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::C8aV1(op))) = effect else {
        return false;
    };
    match name {
        BONECRUSHER_GIANT => matches!(op, C8aOpV1::BonecrusherDamageV1 { .. }),
        CONCEALING_CURTAINS => matches!(op, C8aOpV1::RevealingEyeTransformV1),
        JACE_VRYNS_PRODIGY => matches!(op, C8aOpV1::JaceEmblemMillV1),
        JENNIFER_WALTERS => matches!(op, C8aOpV1::SheHulkMayDamageV1 { .. }),
        EMERITUS_OF_ABUNDANCE => matches!(op, C8aOpV1::EmeritusAbundancePrepareV1),
        _ => false,
    }
}

// ---- Engine hooks -------------------------------------------------------------------

/// Bonecrusher Giant: "Whenever this creature becomes the target of a spell,
/// this creature deals 2 damage to that spell's controller."
///
/// Called whenever a spell or ability finishes announcing its targets (or a
/// spell is retargeted). Fires only on `StackItemKind::Spell`, from any
/// player, deduplicated once per stack item.
pub(crate) fn note_targeting(state: &mut GameState, item: &StackItem) {
    if !cfg!(feature = "eternal-fixtures")
        || item.kind != StackItemKind::Spell
        || item.targets.is_empty()
    {
        return;
    }
    let mut seen = Vec::new();
    let mut hits = Vec::new();
    for &target in &item.targets {
        let Target::Object(object) = target else {
            continue;
        };
        if seen.contains(&object) {
            continue;
        }
        seen.push(object);
        if active_named(state, object, BONECRUSHER_GIANT) {
            hits.push(BonecrusherTargetV1 {
                source: AbilitySourceContractV4::capture(state, object),
                controller: state.objects.get(object).controller,
                spell_controller: item.controller,
            });
        }
    }
    if !hits.is_empty() {
        c8a_state(state).bonecrusher_targets.extend(hits);
    }
}

/// The Sensational She-Hulk: "Whenever a creature you control is dealt damage,
/// you may have The Sensational She-Hulk deal that much damage to any target.
/// Do this only once each turn."
///
/// Called inside `event::materialize_event_v1` as each non-scene damage event
/// commits (before SBAs destroy lethally damaged creatures), so She-Hulk's
/// contract is captured even when She-Hulk or the damaged creature dies in
/// combat.
pub(crate) fn note_damage_dealt(state: &mut GameState, target: Target, amount: i32) {
    if !cfg!(feature = "eternal-fixtures") || amount <= 0 {
        return;
    }
    let Target::Object(damaged) = target else {
        return;
    };
    let Some(live) = state.objects.try_get(damaged) else {
        return;
    };
    if live.zone != Zone::Battlefield
        || !crate::engine::object_has_type(state, damaged, CardType::Creature)
    {
        return;
    }
    let controller = live.controller;
    let sources: Vec<AbilitySourceContractV4> = state.players[controller.index()]
        .battlefield
        .iter()
        .copied()
        .filter(|&object| {
            active_named_face(state, object, JENNIFER_WALTERS, 1)
                && !she_hulk_already_used(state, live_link(state, object))
        })
        .map(|object| AbilitySourceContractV4::capture(state, object))
        .collect();
    if sources.is_empty() {
        return;
    }
    let queue = &mut c8a_state(state).she_hulk_damage_queue;
    for source in sources {
        queue.push(SheHulkDamageV1 {
            source,
            controller,
            amount,
        });
    }
}

/// Whether `CommittedEvent::DeclaredAttacker` should be emitted for Emeritus
/// of Abundance's intervening-if attack trigger.
pub(crate) fn observes_attacks(state: &GameState) -> bool {
    battlefield(state).any(|object| active_named(state, object, EMERITUS_OF_ABUNDANCE))
}

/// Drains queued Bonecrusher Giant and She-Hulk triggers and collects
/// Revealing Eye transforms, Jace emblem triggers, and Emeritus of Abundance
/// attack triggers.
pub(crate) fn delayed_triggers(
    state: &mut GameState,
    events: &[CommittedEvent],
) -> Vec<PendingTrigger> {
    if !cfg!(feature = "eternal-fixtures") {
        return Vec::new();
    }
    let mut triggers = Vec::new();

    // 1. Bonecrusher Giant queued spell-target triggers.
    if let Some(eternal) = state.eternal_v1.as_mut() {
        if !eternal.c8a.bonecrusher_targets.is_empty() {
            let queued = std::mem::take(&mut eternal.c8a.bonecrusher_targets);
            for entry in queued {
                triggers.push(crate::eternal_cards_v1::delayed_trigger(
                    entry.source,
                    entry.controller,
                    leaf(C8aOpV1::BonecrusherDamageV1 {
                        player: entry.spell_controller,
                    }),
                ));
            }
        }
        // 2. The Sensational She-Hulk queued damage triggers.
        if !eternal.c8a.she_hulk_damage_queue.is_empty() {
            let queued = std::mem::take(&mut eternal.c8a.she_hulk_damage_queue);
            for entry in queued {
                let mut trigger = crate::eternal_cards_v1::delayed_trigger(
                    entry.source,
                    entry.controller,
                    leaf(C8aOpV1::SheHulkMayDamageV1 {
                        amount: entry.amount,
                    }),
                );
                trigger.target_spec = TargetSpec::AnyTarget;
                triggers.push(trigger);
            }
        }
    }

    // 3. Concealing Curtains -> Revealing Eye transform triggers.
    for event in events {
        let CommittedEvent::Transformed {
            object,
            face_index: 1,
        } = *event
        else {
            continue;
        };
        if active_named_face(state, object, CONCEALING_CURTAINS, 1) {
            let controller = state.objects.get(object).controller;
            let mut trigger = crate::eternal_cards_v1::delayed_trigger(
                AbilitySourceContractV4::capture(state, object),
                controller,
                revealing_eye_transform(),
            );
            trigger.target_spec = TargetSpec::TargetOpponent;
            triggers.push(trigger);
        }
    }

    // 4. Jace, Telepath Unbound -9 emblems: "Whenever you cast a spell, target
    //    opponent mills five cards."
    let emblems = read_state(state)
        .map(|c8a| c8a.jace_emblems.clone())
        .unwrap_or_default();
    if !emblems.is_empty() {
        for event in events {
            let CommittedEvent::SpellCast { controller, .. } = *event else {
                continue;
            };
            for emblem in &emblems {
                if emblem.controller == controller {
                    let mut trigger = crate::eternal_cards_v1::delayed_trigger(
                        emblem.source,
                        emblem.controller,
                        jace_emblem_mill(),
                    );
                    trigger.target_spec = TargetSpec::TargetOpponent;
                    triggers.push(trigger);
                }
            }
        }
    }

    // 5. Emeritus of Abundance: "Whenever this creature attacks, if you control
    //    eight or more lands, this creature becomes prepared." (Intervening-if
    //    checked both here and on resolution.)
    for event in events {
        let CommittedEvent::DeclaredAttacker {
            source,
            source_zone_change_count,
            controller,
        } = *event
        else {
            continue;
        };
        if active_named(state, source, EMERITUS_OF_ABUNDANCE)
            && state.objects.get(source).zone_change_count == source_zone_change_count
            && controlled_land_count(state, controller) >= 8
        {
            triggers.push(crate::eternal_cards_v1::delayed_trigger(
                AbilitySourceContractV4::capture(state, source),
                controller,
                emeritus_abundance_prepare(),
            ));
        }
    }

    triggers
}

/// The beginning of `player`'s turn expires `player`'s Jace +1 `-2/-0` effects
/// ("until your next turn") and prunes turn-scoped markers from previous turns.
pub(crate) fn begin_turn(state: &mut GameState, player: PlayerId) {
    let now = TurnV1::now(state);
    if let Some(eternal) = state.eternal_v1.as_mut() {
        let c8a = &mut eternal.c8a;
        c8a.jace_weakened.retain(|entry| entry.holder != player);
        c8a.jace_graveyard_casts.retain(|entry| entry.turn == now);
        c8a.she_hulk_used.retain(|&(_, turn)| turn == now);
    }
}

/// Jace, Telepath Unbound +1: `-2/-0` on `object` until its holder's next turn.
pub(crate) fn power_modifier(state: &GameState, object: ObjectId) -> i32 {
    let Some(c8a) = read_state(state) else {
        return 0;
    };
    if c8a.jace_weakened.is_empty() {
        return 0;
    }
    let Some(live) = state.objects.try_get(object) else {
        return 0;
    };
    if live.zone != Zone::Battlefield {
        return 0;
    }
    let link = ObjectLinkV4 {
        object,
        zone_change_count: live.zone_change_count,
    };
    let count = c8a
        .jace_weakened
        .iter()
        .filter(|entry| entry.object == link)
        .count();
    -2 * i32::try_from(count).unwrap_or(i32::MAX / 2)
}

/// Jace, Telepath Unbound -3: whether `holder` may cast `object`'s exact
/// graveyard incarnation `zone_change_count` this turn.
pub(crate) fn jace_graveyard_permission(
    state: &GameState,
    object: ObjectId,
    holder: PlayerId,
    zone_change_count: u32,
) -> bool {
    let Some(c8a) = read_state(state) else {
        return false;
    };
    if c8a.jace_graveyard_casts.is_empty() {
        return false;
    }
    let now = TurnV1::now(state);
    c8a.jace_graveyard_casts.iter().any(|entry| {
        entry.card.object == object
            && entry.card.zone_change_count == zone_change_count
            && entry.holder == holder
            && entry.turn == now
    })
}

/// Jace, Telepath Unbound -3: "If that spell would be put into your graveyard,
/// exile it instead."
pub(crate) fn exiles_on_departure(state: &GameState, item: &StackItem) -> bool {
    if item.is_copy {
        return false;
    }
    let Some(origin) = item
        .v4
        .source_contract
        .and_then(|contract| contract.spell_cast_origin)
    else {
        return false;
    };
    origin.origin_zone == Zone::Graveyard
        && matches!(
            origin.route,
            SpellCastRouteV4::GraveyardPermissionV1 {
                holder,
                permission_zone_change_count,
            } if permission_zone_change_count == origin.origin_zone_change_count
                && jace_graveyard_permission(state, item.source, holder, permission_zone_change_count)
        )
}

/// Jennifer Walters // The Sensational She-Hulk: "Your opponents can't cast
/// spells during your turn." Active on BOTH faces.
pub(crate) fn cast_prohibited(state: &GameState, player: PlayerId) -> bool {
    let opponent = player.opponent();
    state.active_player == opponent
        && state.players[opponent.index()]
            .battlefield
            .iter()
            .copied()
            .any(|object| {
                let live = state.objects.get(object);
                live.zone == Zone::Battlefield
                    && live.controller == opponent
                    && live.v4.face_down_v1.is_none()
                    && definition_name(state, object) == Some(JENNIFER_WALTERS)
                    && crate::continuous_characteristics_v1::printed_abilities_active(state, object)
            })
}

/// Jennifer Walters modal DFC back-face cast: when cast via
/// `CastMethodV4::Alternative`, enters the battlefield transformed as The
/// Sensational She-Hulk (`face_index = 1`).
pub(crate) fn adjust_zone_change(state: &GameState, change: &mut event::ZoneChangeProposed) {
    if change.to_zone != Zone::Battlefield {
        return;
    }
    if is_she_hulk_stack_cast(state, change.object) {
        change.battlefield_face_index = Some(1);
    }
}

/// Jennifer Walters modal DFC back-face mana value on the stack when cast as
/// The Sensational She-Hulk (`{3}{G}{W}{W}` = 6).
pub(crate) fn modal_back_face_stack_mana_value(state: &GameState, item: &StackItem) -> Option<u16> {
    (item.kind == StackItemKind::Spell
        && definition_name(state, item.source) == Some(JENNIFER_WALTERS)
        && (item.v4.cast_method == Some(CastMethodV4::Alternative)
            || is_she_hulk_stack_cast(state, item.source)))
    .then_some(6)
}

/// Modal DFC back-face mana value on the battlefield (`The Sensational
/// She-Hulk` has printed cost `{3}{G}{W}{W}` -> mana value 6, CR 712.8e).
pub(crate) fn back_face_mana_value(name: &str) -> Option<u16> {
    (name == JENNIFER_WALTERS).then_some(6)
}

/// Modal DFC back-face colors on the stack when Jennifer Walters is cast as
/// The Sensational She-Hulk (green and white).
pub(crate) fn color_override(state: &GameState, id: ObjectId) -> Option<u8> {
    is_she_hulk_stack_cast(state, id).then_some(crate::card_def::mana_colors_mask(&[
        ManaColor::G,
        ManaColor::W,
    ]))
}

/// Disallows casting the 6-mana modal back face of Jennifer Walters from a
/// graveyard when the only graveyard permission is a mana-value-restricted one
/// like Lurrus of the Dream-Den (MV <= 2).
pub(crate) fn quote_allowed(
    state: &GameState,
    definition: &CardDef,
    source: ObjectId,
    origin_zone: Zone,
    method: CastMethodV4,
) -> bool {
    if definition.name != JENNIFER_WALTERS
        || method != CastMethodV4::Alternative
        || origin_zone != Zone::Graveyard
    {
        return true;
    }
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
    ) || crate::eternal_e5a_v1::permissions::will_permission(state, source, live.owner)
        || crate::engine::muldrotha_v1::allowed(state, source, live.owner)
}

// ---- Execution ----------------------------------------------------------------------

/// Runs one of this module's leaves in synchronous execution.
pub(crate) fn execute(op: &C8aOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        C8aOpV1::BonecrusherDamageV1 { player } => {
            event::propose_and_commit(
                state,
                event::damage_from_exec_ctx_v1(ctx, Target::Player(*player), 2),
            );
        }
        C8aOpV1::TransformFrontToBackV1 => {
            let Some(contract) = ctx.ability_source_contract else {
                halt(state, ctx.source);
                return;
            };
            let Some(live) = state.objects.try_get(contract.source) else {
                return;
            };
            if live.zone != Zone::Battlefield
                || live.zone_change_count != contract.zone_change_count
                || live.card_def != contract.card_def
                || live.owner != contract.owner
            {
                return;
            }
            // CR 701.28a / 701.28f / 712.9: only a real transforming double-faced
            // permanent on its front face can transform in place; non-DFC clones
            // and token copies do nothing.
            let link = ObjectLinkV4 {
                object: contract.source,
                zone_change_count: live.zone_change_count,
            };
            let physical_def =
                crate::standard_cards_v1::archived_copy_printing(state, link, live.card_def);
            if live.v4.is_token
                || live.v4.face_index != 0
                || physical_def != Some(live.card_def)
                || CARD_DEFS[usize::from(live.card_def)]
                    .transform_face
                    .is_none()
            {
                return;
            }
            event::propose_and_commit(state, ProposedEvent::transform_in_place(contract.source, 1));
        }
        C8aOpV1::JaceMaybeTransformV1 => {
            if graveyard_cards(state, ctx.controller).len() < 5 {
                return;
            }
            let Some(contract) = ctx.ability_source_contract else {
                halt(state, ctx.source);
                return;
            };
            let Some(live) = state.objects.try_get(contract.source) else {
                return;
            };
            if live.zone != Zone::Battlefield
                || live.zone_change_count != contract.zone_change_count
                || live.v4.face_index != 0
            {
                return;
            }
            let owner = live.owner;
            let is_token = live.v4.is_token;
            event::propose_and_commit(
                state,
                ProposedEvent::zone_change(contract.source, Zone::Exile),
            );
            // CR 712.14a: check the exiled card's own physical back face AFTER
            // the zone change has unwound any copy effect so a single-faced
            // clone stays in exile instead of returning face_index = 1.
            let has_back_face = CARD_DEFS[usize::from(state.objects.get(contract.source).card_def)]
                .transform_face
                .is_some();
            if !is_token && has_back_face && state.objects.get(contract.source).zone == Zone::Exile
            {
                event::propose_and_commit(
                    state,
                    ProposedEvent::transformed_battlefield_return(contract.source, 1, owner),
                );
            }
        }
        C8aOpV1::JacePlusOneV1 => {
            let Some(Target::Object(creature)) = ctx.targets.first().copied() else {
                return;
            };
            if !ctx.target_incarnation_matches(0, state)
                || !ctx.ability_source_contract.is_some_and(|source| {
                    crate::engine::effect_target_is_legal_from_ability_source(
                        state,
                        source,
                        ctx.controller,
                        TargetSpec::StandardV1(StandardTargetV1::UpToOneCreature),
                        &ctx.targets,
                        0,
                    )
                })
            {
                return;
            }
            let link = live_link(state, creature);
            c8a_state(state).jace_weakened.push(JaceWeakenV1 {
                object: link,
                holder: ctx.controller,
            });
        }
        C8aOpV1::JaceMinusThreeV1 => {
            let Some(Target::Object(card)) = ctx.targets.first().copied() else {
                return;
            };
            if !ctx.target_incarnation_matches(0, state)
                || !ctx.ability_source_contract.is_some_and(|source| {
                    crate::engine::effect_target_is_legal_from_ability_source(
                        state,
                        source,
                        ctx.controller,
                        TargetSpec::StandardV1(
                            StandardTargetV1::InstantOrSorceryCardInOwnGraveyard,
                        ),
                        &ctx.targets,
                        0,
                    )
                })
            {
                return;
            }
            let entry = JaceGraveyardCastV1 {
                card: live_link(state, card),
                holder: ctx.controller,
                turn: TurnV1::now(state),
            };
            let grants = &mut c8a_state(state).jace_graveyard_casts;
            if !grants.contains(&entry) {
                grants.push(entry);
            }
        }
        C8aOpV1::JaceMinusNineV1 => {
            if let Some(source) = ctx.ability_source_contract {
                c8a_state(state).jace_emblems.push(EmblemV1 {
                    controller: ctx.controller,
                    source,
                });
            }
        }
        C8aOpV1::SheHulkCommitDamageV1 { amount } => {
            let Some(contract) = ctx.ability_source_contract else {
                return;
            };
            let source_link = ObjectLinkV4 {
                object: contract.source,
                zone_change_count: contract.zone_change_count,
            };
            if she_hulk_already_used(state, source_link) {
                return;
            }
            let Some(target) = ctx.targets.first().copied() else {
                return;
            };
            if !ctx.target_incarnation_matches(0, state)
                || !crate::engine::effect_target_is_legal_from_ability_source(
                    state,
                    contract,
                    ctx.controller,
                    TargetSpec::AnyTarget,
                    &ctx.targets,
                    0,
                )
            {
                return;
            }
            let now = TurnV1::now(state);
            c8a_state(state).she_hulk_used.push((source_link, now));
            event::propose_and_commit(state, event::damage_from_exec_ctx_v1(ctx, target, *amount));
        }
        C8aOpV1::RegrowthV1 => {
            let Some(Target::Object(card)) = ctx.targets.first().copied() else {
                return;
            };
            if !ctx.target_incarnation_matches(0, state)
                || !crate::engine::effect_target_is_legal(
                    state,
                    ctx.source,
                    ctx.controller,
                    TargetSpec::StandardV1(StandardTargetV1::CardInOwnGraveyardV1),
                    &ctx.targets,
                    0,
                )
            {
                return;
            }
            event::propose_and_commit(state, ProposedEvent::zone_change(card, Zone::Hand));
        }
        C8aOpV1::EmeritusAbundancePrepareV1 => {
            // Intervening-if rechecked on resolution (CR 603.4).
            if controlled_land_count(state, ctx.controller) < 8 {
                return;
            }
            if let Some(source) = live_ability_source(ctx, state) {
                crate::eternal_vintage_e7b_v1::prepare(state, source);
            }
        }
        C8aOpV1::RevealingEyeTransformV1
        | C8aOpV1::JaceEmblemMillV1
        | C8aOpV1::SheHulkMayDamageV1 { .. }
        | C8aOpV1::EmeritusIdeationAttackV1
        | C8aOpV1::EmeritusIdeationExileEightV1 => {
            halt(state, ctx.source);
        }
    }
}

/// Runs one of this module's leaves in the resumable interpreter.
pub(crate) fn drive(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    op: &C8aOpV1,
    path: Vec<u16>,
) -> Result<bool, String> {
    match op {
        C8aOpV1::RevealingEyeTransformV1 => {
            let Some(Target::Player(opponent)) = continuation.ctx.targets.first().copied() else {
                return Ok(false);
            };
            if !continuation
                .ctx
                .ability_source_contract
                .is_some_and(|source| {
                    crate::engine::effect_target_is_legal_from_ability_source(
                        state,
                        source,
                        continuation.ctx.controller,
                        TargetSpec::TargetOpponent,
                        &continuation.ctx.targets,
                        0,
                    )
                })
            {
                return Ok(false);
            }
            let hand: Vec<ObjectId> = state.players[opponent.index()]
                .hand
                .iter()
                .copied()
                .filter(|&card| state.objects.get(card).zone == Zone::Hand)
                .collect();
            // spellbench-public-reveal: Revealing Eye reveals the target opponent's hand.
            state.reveal_public_hand_group_v3(opponent, hand.iter().copied())?;
            let nonlands: Vec<ObjectId> = hand
                .into_iter()
                .filter(|&card| {
                    !CARD_DEFS[usize::from(state.objects.get(card).card_def)]
                        .has_type(CardType::Land)
                })
                .collect();
            if nonlands.is_empty() {
                return Ok(false);
            }
            let bindings: Vec<EffectObjectBinding> =
                nonlands.iter().map(|&card| bind(state, card)).collect();
            crate::eternal_cards_v1::e3::offer(
                continuation,
                E3ChoiceV1 {
                    kind: E3ChoiceKindV1::C8aV1(C8aChoiceV1::RevealingEyeChooseNonlandV1 {
                        opponent,
                    }),
                    player: continuation.ctx.controller,
                    candidates: nonlands.into_iter().map(Target::Object).collect(),
                    bindings,
                    min: 0,
                    max: 1,
                    ordered: false,
                    path,
                    remaining: continuation.frames.clone(),
                },
            );
            Ok(true)
        }
        C8aOpV1::JaceEmblemMillV1 => {
            let Some(Target::Player(_)) = continuation.ctx.targets.first().copied() else {
                return Ok(false);
            };
            if !continuation
                .ctx
                .ability_source_contract
                .is_some_and(|source| {
                    crate::engine::effect_target_is_legal_from_ability_source(
                        state,
                        source,
                        continuation.ctx.controller,
                        TargetSpec::TargetOpponent,
                        &continuation.ctx.targets,
                        0,
                    )
                })
            {
                return Ok(false);
            }
            continuation.frames.push(EffectFrame::Program {
                op: EffectOp::MillCards {
                    player: PlayerRef::Target(0),
                    count: 5,
                },
                path,
            });
            Ok(false)
        }
        C8aOpV1::SheHulkMayDamageV1 { amount } => {
            let Some(contract) = continuation.ctx.ability_source_contract else {
                return Ok(false);
            };
            let source_link = ObjectLinkV4 {
                object: contract.source,
                zone_change_count: contract.zone_change_count,
            };
            if she_hulk_already_used(state, source_link) {
                return Ok(false);
            }
            if !continuation.ctx.target_incarnation_matches(0, state)
                || !crate::engine::effect_target_is_legal_from_ability_source(
                    state,
                    contract,
                    continuation.ctx.controller,
                    TargetSpec::AnyTarget,
                    &continuation.ctx.targets,
                    0,
                )
            {
                return Ok(false);
            }
            continuation.frames.push(EffectFrame::Program {
                op: EffectOp::Choice {
                    controller: PlayerRef::Controller,
                    options: vec![
                        EffectOp::Sequence(Vec::new()),
                        leaf(C8aOpV1::SheHulkCommitDamageV1 { amount: *amount }),
                    ],
                },
                path,
            });
            Ok(false)
        }
        C8aOpV1::EmeritusIdeationAttackV1 => {
            if graveyard_cards(state, continuation.ctx.controller).len() < 8 {
                return Ok(false);
            }
            continuation.frames.push(EffectFrame::Program {
                op: EffectOp::Choice {
                    controller: PlayerRef::Controller,
                    options: vec![
                        EffectOp::Sequence(Vec::new()),
                        leaf(C8aOpV1::EmeritusIdeationExileEightV1),
                    ],
                },
                path,
            });
            Ok(false)
        }
        C8aOpV1::EmeritusIdeationExileEightV1 => {
            let cards = graveyard_cards(state, continuation.ctx.controller);
            if cards.len() < 8 {
                return Ok(false);
            }
            if cards.len() == 8 {
                exile_eight_and_prepare(state, &continuation.ctx, &cards);
                return Ok(false);
            }
            let bindings: Vec<EffectObjectBinding> =
                cards.iter().map(|&card| bind(state, card)).collect();
            crate::eternal_cards_v1::e3::offer(
                continuation,
                E3ChoiceV1 {
                    kind: E3ChoiceKindV1::C8aV1(C8aChoiceV1::EmeritusIdeationExileEightV1),
                    player: continuation.ctx.controller,
                    candidates: cards.into_iter().map(Target::Object).collect(),
                    bindings,
                    min: 8,
                    max: 8,
                    ordered: false,
                    path,
                    remaining: continuation.frames.clone(),
                },
            );
            Ok(true)
        }
        _ => {
            execute(op, &continuation.ctx, state);
            Ok(false)
        }
    }
}

fn exile_eight_and_prepare(state: &mut GameState, ctx: &ExecCtx, cards: &[ObjectId]) {
    if cards.len() != 8 {
        return;
    }
    let batch = cards
        .iter()
        .map(|&card| ProposedEvent::zone_change(card, Zone::Exile))
        .collect();
    event::propose_and_commit_batch(state, batch);
    if state.engine.halted.is_some() {
        return;
    }
    let all_exiled = cards
        .iter()
        .all(|&card| state.objects.get(card).zone == Zone::Exile);
    if all_exiled {
        if let Some(source) = live_ability_source(ctx, state) {
            crate::eternal_vintage_e7b_v1::prepare(state, source);
        }
    }
}

/// Applies an answered C8a resolution choice.
pub(crate) fn finish_selection(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    choice: &E3ChoiceV1,
    kind: &C8aChoiceV1,
    bindings: Vec<EffectObjectBinding>,
) -> Result<bool, String> {
    match kind {
        C8aChoiceV1::RevealingEyeChooseNonlandV1 { opponent } => {
            let Some(card) = bindings.first() else {
                // Controller declined: opponent neither discards nor draws.
                return Ok(false);
            };
            if !link_live_in(state, binding_link(card), Zone::Hand)
                || state.objects.get(card.object).owner != *opponent
            {
                return Err("Revealing Eye chosen card left the opponent's hand".to_string());
            }
            crate::engine::commit_discarded_card(state, card.object);
            continuation.frames.push(EffectFrame::Program {
                op: EffectOp::DrawCards {
                    player: PlayerRef::Target(0),
                    count: 1,
                },
                path: choice.path.clone(),
            });
            Ok(false)
        }
        C8aChoiceV1::EmeritusIdeationExileEightV1 => {
            if bindings.len() != 8 {
                return Err("Emeritus of Ideation must exile exactly eight cards".to_string());
            }
            let cards: Vec<ObjectId> = bindings.iter().map(|b| b.object).collect();
            for &binding in &bindings {
                if !link_live_in(state, binding_link(&binding), Zone::Graveyard)
                    || state.objects.get(binding.object).owner != choice.player
                {
                    return Err("Emeritus of Ideation chosen card left the graveyard".to_string());
                }
            }
            exile_eight_and_prepare(state, &continuation.ctx, &cards);
            Ok(false)
        }
    }
}
