//! Eternal wave E3: Legacy Doomsday, Sneak and Show, Aluren and Omni-Tell
//! (`docs/design/eternal_formats_support_v1.md`).
//!
//! Every table here is keyed by a printed name that occurs only in the
//! eternal registry. Resolution choices use one typed selection
//! (`E3ChoiceV1`) that the generic effect interpreter stages, validates and
//! answers through `EffectTargetSelectionPurpose::EternalV1` and
//! `EffectFrame::EternalSelectionV1`, so the interpreter keeps a single hook
//! per stage.

use super::{eternal, printed_battlefield_source, EternalOpV1, EternalTriggerV1};
use crate::card_def::{card_id_by_name, CardType, Subtype, TargetSpec, CARD_DEFS};
use crate::effect::{
    EffectContinuation, EffectFrame, EffectObjectBinding, EffectOp, EffectTargetCandidate,
    EffectTargetSelectionPurpose, ExecCtx, PendingEffectChoice, PlayerRef,
};
use crate::event::{self, CommittedEvent, ProposedEvent};
use crate::ids::{ObjectId, PlayerId, StackItemId};
use crate::mana::ManaColor;
use crate::state::{GameState, ObjectLinkV4, StackItemKind, Target, Zone};
use crate::trigger::{TriggerCondition, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};

pub(crate) const THE_ONE_RING: &str = "The One Ring";
pub(crate) const CHROME_MOX: &str = "Chrome Mox";
pub(crate) const SINK_INTO_STUPOR: &str = "Sink into Stupor";
pub(crate) const CAVERN_OF_SOULS: &str = "Cavern of Souls";

/// Per-game state of the E3 cards, inside `EternalStateV1::e3`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct E3StateV1 {
    /// Players with protection from everything until their next turn (The
    /// One Ring).
    #[serde(default, skip_serializing_if = "no_players")]
    pub protected_players: [bool; 2],
    /// Burden counters per The One Ring incarnation. An entry outlives its
    /// permanent as last-known information for its upkeep trigger.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub burden_counters: Vec<(ObjectLinkV4, u32)>,
    /// Chrome Mox incarnations and the card each exiled with its imprint.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub imprints: Vec<(ObjectLinkV4, ObjectLinkV4)>,
    /// The creature type chosen as each Cavern of Souls incarnation entered.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cavern_types: Vec<(ObjectLinkV4, Subtype)>,
    /// Soporific Springs incarnations still choosing whether to pay 3 life
    /// or be tapped.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub springs_entering: Vec<ObjectLinkV4>,
    /// Spell incarnations that can't be countered (Cavern of Souls).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub uncounterable_spells: Vec<ObjectLinkV4>,
}

fn no_players(players: &[bool; 2]) -> bool {
    !players[0] && !players[1]
}

fn e3(state: &GameState) -> Option<&E3StateV1> {
    state.eternal_v1.as_ref()?.e3.as_deref()
}

fn e3_mut(state: &mut GameState) -> &mut E3StateV1 {
    state
        .eternal_v1
        .get_or_insert_with(Default::default)
        .e3
        .get_or_insert_with(Default::default)
}

fn halt(state: &mut GameState, source: ObjectId) {
    state.engine.halted = Some((
        crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
        source,
    ));
}

fn link(state: &GameState, object: ObjectId) -> ObjectLinkV4 {
    ObjectLinkV4 {
        object,
        zone_change_count: state.objects.get(object).zone_change_count,
    }
}

fn link_live(state: &GameState, link: ObjectLinkV4, zone: Zone) -> bool {
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

fn printed(state: &GameState, object: ObjectId) -> &'static crate::card_def::CardDef {
    &CARD_DEFS[state.objects.get(object).card_def as usize]
}

/// The cards in `player`'s hand matching `keep`, bound to their hand
/// incarnations, in hand order.
fn hand_bindings(
    state: &GameState,
    player: PlayerId,
    keep: impl Fn(&crate::card_def::CardDef) -> bool,
) -> Vec<EffectObjectBinding> {
    state.players[player.index()]
        .hand
        .iter()
        .copied()
        .filter(|&card| {
            let live = state.objects.get(card);
            live.zone == Zone::Hand && live.owner == player && keep(printed(state, card))
        })
        .map(|card| bind(state, card))
        .collect()
}

// ---- The selection framework -----------------------------------------------

/// What an E3 resolution choice decides.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum E3ChoiceKindV1 {
    /// Chrome Mox's imprint: up to one nonartifact, nonland card from the
    /// controller's hand, exiled and linked to the Mox incarnation.
    ImprintV1 { mox: ObjectLinkV4 },
    /// Misdirection: the new target of the targeted spell.
    RetargetSpellV1 { spell: StackItemId },
}

/// One staged E3 resolution choice. `candidates` are the offered targets in
/// order; `bindings` binds every object candidate to its incarnation.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct E3ChoiceV1 {
    pub kind: E3ChoiceKindV1,
    pub player: PlayerId,
    pub candidates: Vec<Target>,
    pub bindings: Vec<EffectObjectBinding>,
    pub min: u16,
    pub max: u16,
    pub ordered: bool,
    pub path: Vec<u16>,
    pub remaining: Vec<EffectFrame>,
}

impl E3ChoiceV1 {
    /// Whether the candidates are hidden from the other player (hand or
    /// library cards).
    pub(crate) fn chooser_private(&self) -> bool {
        matches!(self.kind, E3ChoiceKindV1::ImprintV1 { .. })
    }

    /// Whether the chooser sees library cards it has not looked at before
    /// (a search).
    pub(crate) fn library_search(&self) -> bool {
        false
    }

    pub(crate) fn public_purpose(&self) -> crate::rl::TargetSelectionPurposeV4 {
        match self.kind {
            E3ChoiceKindV1::ImprintV1 { .. } => crate::rl::TargetSelectionPurposeV4::CardSelection,
            E3ChoiceKindV1::RetargetSpellV1 { .. } => {
                crate::rl::TargetSelectionPurposeV4::EffectTargets
            }
        }
    }

    /// Every object incarnation this choice refers to.
    pub(crate) fn referenced_bindings(&self) -> Vec<EffectObjectBinding> {
        let mut out = self.bindings.clone();
        if let E3ChoiceKindV1::ImprintV1 { mox } = self.kind {
            out.push(EffectObjectBinding {
                object: mox.object,
                expected_zone: Zone::Battlefield,
                expected_zone_change_count: mox.zone_change_count,
            });
        }
        out
    }

    fn binding_for(&self, target: Target) -> Option<EffectObjectBinding> {
        let Target::Object(object) = target else {
            return None;
        };
        self.bindings
            .iter()
            .copied()
            .find(|binding| binding.object == object)
    }

    fn prompt(&self) -> PendingEffectChoice {
        PendingEffectChoice::SelectTargets {
            player: self.player,
            path: self.path.clone(),
            selected: Vec::new(),
            legal: self
                .candidates
                .iter()
                .map(|&target| EffectTargetCandidate {
                    target,
                    expected_object: self.binding_for(target),
                })
                .collect(),
            min_targets: self.min,
            max_targets: self.max,
            ordered: self.ordered,
            purpose: EffectTargetSelectionPurpose::EternalV1 {
                choice: Box::new(self.clone()),
            },
        }
    }
}

/// Stages `choice` as the continuation's pending choice.
fn offer(continuation: &mut EffectContinuation, choice: E3ChoiceV1) {
    continuation.choice = Some(choice.prompt());
}

/// Checks a live prompt against its typed choice.
#[allow(clippy::too_many_arguments)]
pub(crate) fn validate_selection(
    state: &GameState,
    pending: &EffectContinuation,
    choice: &E3ChoiceV1,
    chooser: PlayerId,
    path: &[u16],
    selected: &[EffectTargetCandidate],
    legal: &[EffectTargetCandidate],
    min: u16,
    max: u16,
    ordered: bool,
) -> Result<(), String> {
    if chooser != choice.player
        || path != choice.path.as_slice()
        || ordered != choice.ordered
        || max != choice.max
        || min > choice.max
        || pending.frames != choice.remaining
    {
        return Err("eternal choice prompt changed its shape".to_string());
    }
    let mut offered = selected
        .iter()
        .chain(legal)
        .map(|candidate| candidate.target)
        .collect::<Vec<_>>();
    let mut expected = choice.candidates.clone();
    offered.sort_by_key(target_key);
    expected.sort_by_key(target_key);
    if offered != expected {
        return Err("eternal choice candidates changed".to_string());
    }
    for candidate in selected.iter().chain(legal) {
        if candidate.expected_object != choice.binding_for(candidate.target) {
            return Err("eternal choice candidate binding changed".to_string());
        }
    }
    for binding in &choice.bindings {
        if !binding_live(state, *binding) {
            return Err("eternal choice candidate left its zone".to_string());
        }
    }
    Ok(())
}

fn target_key(target: &Target) -> (u8, u64) {
    match *target {
        Target::Object(object) => (0, u64::from(object.0)),
        Target::Player(player) => (1, player.index() as u64),
        Target::StackItem(item) => (2, item.0),
    }
}

/// Applies an answered choice. `Ok(true)` when it staged a further choice.
pub(crate) fn finish_selection(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    choice: &E3ChoiceV1,
    selected: &[Target],
) -> Result<bool, String> {
    if continuation.frames != choice.remaining
        || selected.len() > usize::from(choice.max)
        || selected.len() < usize::from(choice.min).min(choice.candidates.len())
        || selected.iter().enumerate().any(|(index, target)| {
            !choice.candidates.contains(target) || selected[..index].contains(target)
        })
    {
        return Err("eternal choice answer changed".to_string());
    }
    let bindings = selected
        .iter()
        .filter_map(|&target| choice.binding_for(target))
        .collect::<Vec<_>>();
    if bindings
        .iter()
        .any(|binding| !binding_live(state, *binding))
    {
        return Err("eternal choice answer is stale".to_string());
    }
    match &choice.kind {
        E3ChoiceKindV1::ImprintV1 { mox } => {
            if let Some(card) = bindings.first() {
                imprint(state, *mox, card.object);
            }
            Ok(false)
        }
        E3ChoiceKindV1::RetargetSpellV1 { spell } => {
            if let Some(&target) = selected.first() {
                crate::engine::retarget_spell_v1(state, *spell, target)?;
            }
            Ok(false)
        }
    }
}

/// Runs an E3 leaf inside the resumable interpreter. `Ok(true)` when it
/// staged a choice and the continuation must suspend.
pub(crate) fn drive(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    op: &EternalOpV1,
    path: Vec<u16>,
) -> Result<bool, String> {
    match op {
        EternalOpV1::MayImprintFromHandV1 => {
            let Some(contract) = continuation.ctx.ability_source_contract else {
                return Err("imprint lost its source".to_string());
            };
            let player = continuation.ctx.controller;
            let cards = hand_bindings(state, player, |def| {
                !def.has_type(CardType::Artifact) && !def.has_type(CardType::Land)
            });
            if cards.is_empty() {
                return Ok(false);
            }
            offer(
                continuation,
                E3ChoiceV1 {
                    kind: E3ChoiceKindV1::ImprintV1 {
                        mox: ObjectLinkV4 {
                            object: contract.source,
                            zone_change_count: contract.zone_change_count,
                        },
                    },
                    player,
                    candidates: cards
                        .iter()
                        .map(|card| Target::Object(card.object))
                        .collect(),
                    bindings: cards,
                    min: 0,
                    max: 1,
                    ordered: false,
                    path,
                    remaining: continuation.frames.clone(),
                },
            );
            Ok(true)
        }
        EternalOpV1::ChangeTargetOfTargetSpellV1 => {
            let Some(Target::Object(spell)) = continuation.ctx.targets.first().copied() else {
                return Ok(false);
            };
            if !continuation.ctx.target_incarnation_matches(0, state) {
                return Ok(false);
            }
            let Some(item) = state
                .stack
                .iter()
                .find(|item| item.kind == StackItemKind::Spell && item.source == spell)
                .map(|item| item.v4.stack_item_id)
            else {
                return Ok(false);
            };
            let candidates = crate::engine::spell_retarget_candidates_v1(state, item);
            if candidates.is_empty() {
                // 115.7a: with no other legal target, the target is unchanged.
                return Ok(false);
            }
            let bindings = candidates
                .iter()
                .filter_map(|target| match *target {
                    Target::Object(object) => Some(bind(state, object)),
                    _ => None,
                })
                .collect();
            offer(
                continuation,
                E3ChoiceV1 {
                    kind: E3ChoiceKindV1::RetargetSpellV1 { spell: item },
                    player: continuation.ctx.controller,
                    candidates,
                    bindings,
                    min: 1,
                    max: 1,
                    ordered: false,
                    path,
                    remaining: continuation.frames.clone(),
                },
            );
            Ok(true)
        }
        other => {
            super::execute(other, &continuation.ctx, state);
            Ok(false)
        }
    }
}

/// Whether `op` may stage a choice, so its program enters the interpreter.
pub(crate) fn contains_player_choice(op: &EternalOpV1) -> bool {
    matches!(
        op,
        EternalOpV1::MayImprintFromHandV1 | EternalOpV1::ChangeTargetOfTargetSpellV1
    )
}

// ---- Ancient Tomb -------------------------------------------------------------
// "{T}: Add {C}{C}. This land deals 2 damage to you." is the generic rich
// mana ability (`build_eternal_v1::mana_ability_def_for`).

// ---- The One Ring ------------------------------------------------------------

/// "When The One Ring enters, if you cast it, you gain protection from
/// everything until your next turn."
fn one_ring_protection() -> EffectOp {
    eternal(EternalOpV1::ProtectionFromEverythingUntilYourNextTurnV1)
}

/// "At the beginning of your upkeep, you lose 1 life for each burden
/// counter on The One Ring."
fn one_ring_upkeep() -> EffectOp {
    eternal(EternalOpV1::LoseLifePerBurdenCounterV1)
}

/// "{T}: Put a burden counter on The One Ring, then draw a card for each
/// burden counter on The One Ring."
pub fn the_one_ring_draw() -> EffectOp {
    eternal(EternalOpV1::BurdenCounterThenDrawV1)
}

fn burden_count(state: &GameState, ring: ObjectLinkV4) -> u32 {
    e3(state)
        .and_then(|e3| {
            e3.burden_counters
                .iter()
                .find(|(entry, _)| *entry == ring)
                .map(|(_, count)| *count)
        })
        .unwrap_or(0)
}

/// Whether `player` has protection from everything (The One Ring).
pub(crate) fn player_protected(state: &GameState, player: PlayerId) -> bool {
    e3(state).is_some_and(|e3| e3.protected_players[player.index()])
}

/// Damage an E3 protection prevents: all damage to a player with
/// protection from everything.
pub(crate) fn damage_prevented_by_protection(
    state: &GameState,
    _source: ObjectId,
    target: Target,
) -> bool {
    match target {
        Target::Player(player) => player_protected(state, player),
        _ => false,
    }
}

// ---- Chrome Mox ----------------------------------------------------------------

/// "Imprint -- When this artifact enters, you may exile a nonartifact,
/// nonland card from your hand."
fn chrome_mox_imprint() -> EffectOp {
    eternal(EternalOpV1::MayImprintFromHandV1)
}

fn imprint(state: &mut GameState, mox: ObjectLinkV4, card: ObjectId) {
    event::propose_and_commit(state, ProposedEvent::zone_change(card, Zone::Exile));
    let live = state.objects.get(card);
    if live.zone != Zone::Exile {
        return;
    }
    let exiled = ObjectLinkV4 {
        object: card,
        zone_change_count: live.zone_change_count,
    };
    let e3 = e3_mut(state);
    e3.imprints.retain(|(entry, _)| *entry != mox);
    e3.imprints.push((mox, exiled));
}

/// "{T}: Add one mana of any of the exiled card's colors." The colors a
/// Chrome Mox can add, empty for every other permanent.
pub(crate) fn imprint_mana_colors(state: &GameState, object: ObjectId) -> Vec<ManaColor> {
    let Some(e3) = e3(state) else {
        return Vec::new();
    };
    let Some(live) = state.objects.try_get(object) else {
        return Vec::new();
    };
    if live.zone != Zone::Battlefield || !printed_battlefield_source(state, object, CHROME_MOX) {
        return Vec::new();
    }
    let mox = ObjectLinkV4 {
        object,
        zone_change_count: live.zone_change_count,
    };
    let Some(&(_, card)) = e3.imprints.iter().find(|(entry, _)| *entry == mox) else {
        return Vec::new();
    };
    if !link_live(state, card, Zone::Exile) {
        return Vec::new();
    }
    let mask = crate::engine::object_color_mask(state, card.object);
    [
        ManaColor::W,
        ManaColor::U,
        ManaColor::B,
        ManaColor::R,
        ManaColor::G,
    ]
    .into_iter()
    .filter(|&color| mask & crate::card_def::mana_color_mask(color) != 0)
    .collect()
}

// ---- Misdirection --------------------------------------------------------------

/// Misdirection: "Change the target of target spell with a single target."
pub fn misdirection() -> EffectOp {
    eternal(EternalOpV1::ChangeTargetOfTargetSpellV1)
}

// ---- Sink into Stupor // Soporific Springs --------------------------------------

/// Sink into Stupor: "Return target spell or nonland permanent an opponent
/// controls to its owner's hand."
pub fn sink_into_stupor() -> EffectOp {
    EffectOp::MoveObject {
        object: crate::effect::ObjectRef::Target(0),
        to_zone: Zone::Hand,
    }
}

/// Modal double-faced cards whose back face is a land played from hand.
pub(crate) fn has_land_back_face(name: &str) -> bool {
    name == SINK_INTO_STUPOR
}

/// Whether `card` is a modal double-faced card in its owner's hand whose
/// land face can be played.
pub(crate) fn playable_land_back_face(state: &GameState, card: ObjectId) -> bool {
    state.objects.try_get(card).is_some_and(|live| {
        live.zone == Zone::Hand
            && CARD_DEFS
                .get(live.card_def as usize)
                .is_some_and(|def| def.is_executable() && has_land_back_face(def.name))
    })
}

/// Plays the land face of a modal double-faced card. "As this land enters,
/// you may pay 3 life. If you don't, it enters tapped." is asked right after
/// it enters, before anything else happens.
pub(crate) fn play_land_back_face(state: &mut GameState, player: PlayerId, card: ObjectId) {
    event::propose_and_commit(
        state,
        ProposedEvent::transformed_battlefield_return(card, 1, player),
    );
    let live = state.objects.get(card);
    if live.zone == Zone::Battlefield && live.v4.face_index == 1 {
        let entering = link(state, card);
        e3_mut(state).springs_entering.push(entering);
    }
}

/// The mana value of a permanent showing a modal double-faced card's land
/// face, which has no mana cost. `None` for every other object.
pub(crate) fn back_face_mana_value(state: &GameState, object: ObjectId) -> Option<u16> {
    let live = state.objects.try_get(object)?;
    (live.zone == Zone::Battlefield
        && live.v4.face_index == 1
        && has_land_back_face(CARD_DEFS.get(live.card_def as usize)?.name))
    .then_some(0)
}

// ---- Cavern of Souls -------------------------------------------------------------

/// The creature types a Cavern of Souls may name for `player`: those of the
/// creature cards that player owns, in stable subtype order. Naming any
/// other type is equivalent for that player, so these are the distinct
/// choices.
pub fn cavern_type_options(state: &GameState, player: PlayerId) -> Vec<Subtype> {
    let mut types: Vec<Subtype> = Vec::new();
    for (_, object) in state.objects.iter() {
        if object.owner != player || object.v4.is_token || object.spell_copy_origin.is_some() {
            continue;
        }
        let Some(def) = CARD_DEFS.get(object.card_def as usize) else {
            continue;
        };
        if !def.has_type(CardType::Creature) {
            continue;
        }
        for &subtype in def.subtypes {
            if subtype.is_creature_type()
                && !types.iter().any(|known| known.same_subtype_as(subtype))
            {
                types.push(subtype);
            }
        }
    }
    types.sort_by_key(|subtype| subtype.stable_id());
    if types.is_empty() {
        types.push(Subtype::CREATURE_TYPES[0]);
    }
    types
}

/// The creature type chosen for `cavern`, if it is a Cavern of Souls that
/// has chosen one.
pub(crate) fn cavern_chosen_type(state: &GameState, cavern: ObjectId) -> Option<Subtype> {
    let live = state.objects.try_get(cavern)?;
    let entry = ObjectLinkV4 {
        object: cavern,
        zone_change_count: live.zone_change_count,
    };
    e3(state)?
        .cavern_types
        .iter()
        .find(|(cavern, _)| *cavern == entry)
        .map(|(_, subtype)| *subtype)
}

/// Whether `cavern`'s restricted mana may pay for a creature spell with
/// these effective subtype ids.
pub(crate) fn cavern_permits(state: &GameState, cavern: ObjectId, spell_subtypes: &[u16]) -> bool {
    cavern_chosen_type(state, cavern)
        .is_some_and(|subtype| subtype.is_in_subtype_ids(spell_subtypes))
}

/// The as-enters choices still owed by permanents that have just entered
/// (Cavern of Souls' creature type, Soporific Springs' life payment), as a
/// decision for the next one.
pub(crate) fn entry_choice_decision(state: &GameState) -> Option<crate::engine::Decision> {
    if let Some(entering) = live_springs_entry(state) {
        let player = state.objects.get(entering.object).controller;
        return Some(crate::engine::Decision::ChooseEffectOption {
            player,
            source: entering.object,
            option_count: 2,
        });
    }
    let cavern = cavern_needing_type(state)?;
    let player = state.objects.get(cavern).controller;
    Some(crate::engine::Decision::ChooseEffectOption {
        player,
        source: cavern,
        option_count: u16::try_from(cavern_type_options(state, player).len()).unwrap_or(u16::MAX),
    })
}

fn cavern_needing_type(state: &GameState) -> Option<ObjectId> {
    let id = card_id_by_name(CAVERN_OF_SOULS)?;
    [PlayerId::P0, PlayerId::P1]
        .into_iter()
        .flat_map(|player| state.players[player.index()].battlefield.iter().copied())
        .find(|&object| {
            state.objects.get(object).card_def == id && cavern_chosen_type(state, object).is_none()
        })
}

fn live_springs_entry(state: &GameState) -> Option<ObjectLinkV4> {
    e3(state)?
        .springs_entering
        .iter()
        .copied()
        .find(|&entering| link_live(state, entering, Zone::Battlefield))
}

/// Whether an as-enters choice is pending.
pub(crate) fn entry_choice_pending(state: &GameState) -> bool {
    entry_choice_decision(state).is_some()
}

/// Answers the pending as-enters choice.
pub(crate) fn answer_entry_choice(state: &mut GameState, option: u16) -> Result<(), String> {
    if let Some(entering) = live_springs_entry(state) {
        let player = state.objects.get(entering.object).controller;
        if option == 1 && state.players[player.index()].life < 3 {
            return Err("Soporific Springs needs 3 life to pay".to_string());
        }
        if option > 1 {
            return Err("Soporific Springs has two options".to_string());
        }
        let still_entering = e3(state)
            .map(|e3| {
                e3.springs_entering
                    .iter()
                    .copied()
                    .filter(|&other| {
                        other != entering && link_live(state, other, Zone::Battlefield)
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        e3_mut(state).springs_entering = still_entering;
        if option == 0 {
            // "If you don't, it enters tapped."
            event::propose_and_commit(state, ProposedEvent::tap(entering.object));
        } else {
            event::propose_and_commit(state, ProposedEvent::life_loss(player, 3));
        }
        return Ok(());
    }
    let cavern = cavern_needing_type(state).ok_or("no as-enters choice is pending")?;
    let player = state.objects.get(cavern).controller;
    let options = cavern_type_options(state, player);
    let subtype = *options
        .get(usize::from(option))
        .ok_or("creature type option is out of range")?;
    let entry = link(state, cavern);
    let e3 = e3_mut(state);
    e3.cavern_types
        .retain(|(entry_link, _)| entry_link.object != cavern);
    e3.cavern_types.push((entry, subtype));
    Ok(())
}

/// Marks a creature spell paid with a Cavern of Souls of its type as
/// uncounterable (the controller uses Cavern's restricted ability whenever
/// it may).
pub(crate) fn note_spell_payment(
    state: &mut GameState,
    spell: ObjectId,
    taps: &[(ObjectId, ManaColor)],
) {
    let Some(cavern) = card_id_by_name(CAVERN_OF_SOULS) else {
        return;
    };
    if !crate::engine::object_has_type(state, spell, CardType::Creature) {
        return;
    }
    let subtypes = state.objects.get(spell).v4.effective_subtype_ids.clone();
    let paid_with_cavern = taps.iter().any(|&(source, _)| {
        state.objects.get(source).card_def == cavern && cavern_permits(state, source, &subtypes)
    });
    if paid_with_cavern {
        let spell = link(state, spell);
        e3_mut(state).uncounterable_spells.push(spell);
    }
}

/// Whether the spell `object` on the stack can't be countered by an E3
/// effect (Cavern of Souls, Veil of Summer).
pub(crate) fn spell_cannot_be_countered(state: &GameState, object: ObjectId) -> bool {
    let Some(live) = state.objects.try_get(object) else {
        return false;
    };
    let entry = ObjectLinkV4 {
        object,
        zone_change_count: live.zone_change_count,
    };
    e3(state).is_some_and(|e3| e3.uncounterable_spells.contains(&entry))
}

// ---- Triggers and op execution ---------------------------------------------------

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

const fn eternal_trigger(
    condition: EternalTriggerV1,
    effect: fn() -> EffectOp,
) -> TriggeredAbilityDef {
    trigger(
        TriggerCondition::StandardV1(crate::standard_cards_v1::StandardTriggerV1::EternalV1(
            condition,
        )),
        effect,
    )
}

const THE_ONE_RING_TRIGGERS: [TriggeredAbilityDef; 2] = [
    eternal_trigger(EternalTriggerV1::EntersAfterCastV1, one_ring_protection),
    trigger(
        TriggerCondition::BeginningOfUpkeep {
            controller_only: true,
        },
        one_ring_upkeep,
    ),
];
const CHROME_MOX_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, chrome_mox_imprint)];

pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        THE_ONE_RING => &THE_ONE_RING_TRIGGERS,
        CHROME_MOX => &CHROME_MOX_TRIGGERS,
        _ => &[],
    }
}

pub(crate) fn trigger_target_spec(name: &str, _effect: &EffectOp) -> Option<TargetSpec> {
    match name {
        THE_ONE_RING | CHROME_MOX => Some(TargetSpec::None),
        _ => None,
    }
}

/// Matches the E3 trigger conditions. `source` is the trigger's source.
pub(crate) fn trigger_matches(
    condition: EternalTriggerV1,
    events: &[CommittedEvent],
    index: usize,
    source: ObjectId,
    state: &GameState,
) -> bool {
    match condition {
        EternalTriggerV1::EntersAfterCastV1 => {
            // "if you cast it": a permanent spell resolving enters from the
            // stack. The condition cannot change before resolution.
            matches!(
                events[index],
                CommittedEvent::ZoneChange {
                    object,
                    from: Zone::Stack,
                    to: Zone::Battlefield,
                    ..
                } if object == source
            ) && state.objects.get(source).spell_copy_origin.is_none()
        }
        _ => false,
    }
}

pub(crate) fn execute(op: &EternalOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        EternalOpV1::ProtectionFromEverythingUntilYourNextTurnV1 => {
            e3_mut(state).protected_players[ctx.controller.index()] = true;
        }
        EternalOpV1::LoseLifePerBurdenCounterV1 => {
            let Some(contract) = ctx.ability_source_contract else {
                return;
            };
            let count = burden_count(
                state,
                ObjectLinkV4 {
                    object: contract.source,
                    zone_change_count: contract.zone_change_count,
                },
            );
            if count > 0 {
                event::propose_and_commit(
                    state,
                    ProposedEvent::life_loss(
                        ctx.controller,
                        i32::try_from(count).unwrap_or(i32::MAX),
                    ),
                );
            }
        }
        EternalOpV1::BurdenCounterThenDrawV1 => {
            let Some(contract) = ctx.ability_source_contract else {
                halt(state, ctx.source);
                return;
            };
            let ring = ObjectLinkV4 {
                object: contract.source,
                zone_change_count: contract.zone_change_count,
            };
            if link_live(state, ring, Zone::Battlefield) {
                let e3 = e3_mut(state);
                match e3
                    .burden_counters
                    .iter_mut()
                    .find(|(entry, _)| *entry == ring)
                {
                    Some((_, count)) => *count = count.saturating_add(1),
                    None => e3.burden_counters.push((ring, 1)),
                }
            }
            // Last-known counters if the Ring has left the battlefield.
            let count = burden_count(state, ring);
            crate::effect::execute(
                &EffectOp::DrawCards {
                    player: PlayerRef::Controller,
                    count,
                },
                ctx,
                state,
            );
        }
        EternalOpV1::MayImprintFromHandV1 | EternalOpV1::ChangeTargetOfTargetSpellV1 => {
            // Choice leaves run only inside the resumable interpreter.
            halt(state, ctx.source);
        }
        _ => halt(state, ctx.source),
    }
}

/// The beginning of `player`'s turn ends their "until your next turn"
/// effects.
pub(crate) fn begin_turn(state: &mut GameState, player: PlayerId) {
    if let Some(e3) = state.eternal_v1.as_mut().and_then(|e| e.e3.as_deref_mut()) {
        e3.protected_players[player.index()] = false;
    }
}

/// Whether `player` may be targeted by `controller`'s spell or ability from
/// `source` under E3 protections (The One Ring's protection from
/// everything).
pub(crate) fn player_targetable(
    state: &GameState,
    player: PlayerId,
    _controller: PlayerId,
    _source: Option<ObjectId>,
) -> bool {
    !player_protected(state, player)
}
