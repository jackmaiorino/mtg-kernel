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
use crate::mana::{ManaColor, Pip};
use crate::state::{
    AbilitySourceContractV4, GameState, ObjectLinkV4, StackItemKind, StackTargetContractV4, Target,
    Zone,
};
use crate::trigger::{TriggerCondition, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};

pub(crate) mod dungeons;

pub(crate) const THE_ONE_RING: &str = "The One Ring";
pub(crate) const CHROME_MOX: &str = "Chrome Mox";
pub(crate) const SINK_INTO_STUPOR: &str = "Sink into Stupor";
pub(crate) const CAVERN_OF_SOULS: &str = "Cavern of Souls";
pub(crate) const CITY_OF_TRAITORS: &str = "City of Traitors";
pub(crate) const STREET_WRAITH: &str = "Street Wraith";
pub(crate) const FLUSTERSTORM: &str = "Flusterstorm";
pub(crate) const THASSAS_ORACLE: &str = "Thassa's Oracle";
pub(crate) const BRAIN_FREEZE: &str = "Brain Freeze";
pub(crate) const TENDRILS_OF_AGONY: &str = "Tendrils of Agony";

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
    /// Land-face incarnations (Soporific Springs, Witch-Blessed Meadow)
    /// still choosing whether to pay 3 life or be tapped.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub springs_entering: Vec<ObjectLinkV4>,
    /// Spell incarnations that can't be countered (Cavern of Souls).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub uncounterable_spells: Vec<ObjectLinkV4>,
    /// Lands just played, for "when you play another land" (City of
    /// Traitors), until the next trigger collection.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lands_played: Vec<(PlayerId, ObjectLinkV4)>,
    /// Veil of Summer effects, each for the turn it resolved.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub veils: Vec<VeilV1>,
    /// Targets of Storm spells as they were cast, for their copies.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub storm_casts: Vec<StormCastV1>,
    /// Set only on a projected state: cost quotes of this hand card ask
    /// about its cast without paying its mana cost (Omniscience, Aluren).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub free_quote: Option<ObjectId>,
    /// Dungeon room abilities set off since the last trigger collection.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub room_triggers: Vec<dungeons::RoomTriggerV1>,
    /// Creatures that get -4/-0 until their `holder`'s next turn (Fungi
    /// Cavern).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub weakened: Vec<UntilNextTurnV1>,
    /// Creatures that can't attack until their `holder`'s next turn (Twisted
    /// Caverns).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pacified: Vec<UntilNextTurnV1>,
}

/// An effect on one permanent incarnation until `holder`'s next turn.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UntilNextTurnV1 {
    pub object: ObjectLinkV4,
    pub holder: PlayerId,
}

/// One resolved Veil of Summer: its controller's spells can't be countered
/// and they and `permanents` have hexproof from blue and from black, for
/// the turn with key `turn`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VeilV1 {
    pub player: PlayerId,
    pub turn: u32,
    pub permanents: Vec<ObjectLinkV4>,
}

/// A Storm spell's targets as cast (Flusterstorm), keyed by its stack id.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StormCastV1 {
    pub item: StackItemId,
    pub target_spec: TargetSpec,
    pub targets: Vec<Target>,
    pub contracts: Vec<StackTargetContractV4>,
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
    /// A Storm copy may choose a new target (or keep its own); `rest` are
    /// the later copies still to choose.
    RetargetCopyV1 {
        copy: StackItemId,
        rest: Vec<StackItemId>,
    },
    /// Doomsday: exactly five (or all) cards from the controller's library
    /// and graveyard, in order; the first chosen ends on top.
    DoomsdayV1,
    /// Thassa's Oracle: up to one of the looked-at top cards stays on top.
    /// `devotion` is X, `prefix` the cards looked at.
    OracleV1 {
        devotion: u32,
        prefix: Vec<EffectObjectBinding>,
    },
    /// Show and Tell: up to one card from the chooser's hand. `chosen` are
    /// the earlier players' cards, `later` the players still to choose.
    ShowAndTellV1 {
        chosen: Vec<EffectObjectBinding>,
        later: Vec<PlayerId>,
    },
    /// Sneak Attack: up to one creature card from the controller's hand.
    SneakAttackV1 { source: AbilitySourceContractV4 },
    /// Annihilator: the defending player's permanents to sacrifice.
    AnnihilatorV1,
    /// Atraxa: the next revealed card to put into the hand, keeping one
    /// distinct card type per picked card. Finishing ends the choice.
    AtraxaV1 {
        revealed: Vec<EffectObjectBinding>,
        picked: Vec<EffectObjectBinding>,
    },
    /// Scry: the looked-at `prefix`; first the cards kept on top in order,
    /// then (`top` chosen) the order of the rest on the bottom.
    ScryV1 {
        prefix: Vec<EffectObjectBinding>,
        top: Option<Vec<EffectObjectBinding>>,
    },
    /// Veils of Fear (room 2: discard) or Sandfall Cell (room 4:
    /// sacrifice): up to one card, or lose 2 life. `chosen` are the earlier
    /// players' answers, `later` the players still to choose.
    PunisherV1 {
        room: u16,
        chosen: Vec<(PlayerId, Option<EffectObjectBinding>)>,
        later: Vec<PlayerId>,
    },
    /// Oubliette: the discard (stage 0), then a creature, an artifact and a
    /// land to sacrifice (stages 1 to 3).
    OublietteV1 {
        discard: Option<EffectObjectBinding>,
        chosen: Vec<EffectObjectBinding>,
        stage: u8,
    },
    /// Acererak's attack: the opponent may sacrifice a creature, or
    /// `controller` creates a Zombie.
    ZombieUnlessSacrificeV1 { controller: PlayerId },
    /// Mad Wizard's Lair: up to one of the `drawn` cards to cast without
    /// paying its mana cost.
    MadWizardV1 { drawn: Vec<EffectObjectBinding> },
    /// A choice owned by the Legacy aggro, Eldrazi and control lists'
    /// module (`eternal_legacy_e5b_v1`).
    E5bV1(crate::eternal_legacy_e5b_v1::E5bChoiceV1),
    /// A wave E5a choice (`eternal_e5a_v1::choices`).
    E5aV1(crate::eternal_e5a_v1::E5aChoiceV1),
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
        if let E3ChoiceKindV1::E5bV1(kind) = &self.kind {
            return kind.chooser_private();
        }
        matches!(
            self.kind,
            E3ChoiceKindV1::ImprintV1 { .. }
                | E3ChoiceKindV1::DoomsdayV1
                | E3ChoiceKindV1::OracleV1 { .. }
                | E3ChoiceKindV1::ShowAndTellV1 { .. }
                | E3ChoiceKindV1::SneakAttackV1 { .. }
                | E3ChoiceKindV1::ScryV1 { .. }
                | E3ChoiceKindV1::PunisherV1 { room: 2, .. }
                | E3ChoiceKindV1::OublietteV1 { stage: 0, .. }
                | E3ChoiceKindV1::E5aV1(_)
        )
    }

    /// Whether the chooser sees library cards it has not looked at before
    /// (a search).
    pub(crate) fn library_search(&self) -> bool {
        match &self.kind {
            E3ChoiceKindV1::DoomsdayV1 => true,
            E3ChoiceKindV1::E5aV1(kind) => kind.library_search(),
            _ => false,
        }
    }

    pub(crate) fn public_purpose(&self) -> crate::rl::TargetSelectionPurposeV4 {
        match &self.kind {
            E3ChoiceKindV1::E5bV1(kind) => kind.public_purpose(),
            E3ChoiceKindV1::ImprintV1 { .. }
            | E3ChoiceKindV1::DoomsdayV1
            | E3ChoiceKindV1::OracleV1 { .. }
            | E3ChoiceKindV1::ShowAndTellV1 { .. }
            | E3ChoiceKindV1::SneakAttackV1 { .. }
            | E3ChoiceKindV1::AtraxaV1 { .. }
            | E3ChoiceKindV1::PunisherV1 { room: 2, .. }
            | E3ChoiceKindV1::OublietteV1 { stage: 0, .. }
            | E3ChoiceKindV1::MadWizardV1 { .. }
            | E3ChoiceKindV1::E5aV1(_) => crate::rl::TargetSelectionPurposeV4::CardSelection,
            E3ChoiceKindV1::AnnihilatorV1
            | E3ChoiceKindV1::PunisherV1 { .. }
            | E3ChoiceKindV1::OublietteV1 { .. }
            | E3ChoiceKindV1::ZombieUnlessSacrificeV1 { .. } => {
                crate::rl::TargetSelectionPurposeV4::PermanentSelection
            }
            E3ChoiceKindV1::ScryV1 { .. } => crate::rl::TargetSelectionPurposeV4::LibraryOrder,
            E3ChoiceKindV1::RetargetSpellV1 { .. } | E3ChoiceKindV1::RetargetCopyV1 { .. } => {
                crate::rl::TargetSelectionPurposeV4::EffectTargets
            }
        }
    }

    /// Every object incarnation this choice refers to.
    pub(crate) fn referenced_bindings(&self) -> Vec<EffectObjectBinding> {
        let mut out = self.bindings.clone();
        if let E3ChoiceKindV1::ShowAndTellV1 { chosen, .. } = &self.kind {
            out.extend(chosen.iter().copied());
        }
        match &self.kind {
            E3ChoiceKindV1::AtraxaV1 { revealed, .. } => out.extend(revealed.iter().copied()),
            E3ChoiceKindV1::ScryV1 { prefix, .. } => out.extend(prefix.iter().copied()),
            E3ChoiceKindV1::PunisherV1 { chosen, .. } => {
                out.extend(chosen.iter().filter_map(|(_, card)| *card));
            }
            E3ChoiceKindV1::OublietteV1 {
                discard, chosen, ..
            } => {
                out.extend(*discard);
                out.extend(chosen.iter().copied());
            }
            E3ChoiceKindV1::MadWizardV1 { drawn } => out.extend(drawn.iter().copied()),
            E3ChoiceKindV1::E5bV1(kind) => out.extend(kind.referenced_bindings()),
            _ => {}
        }
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
pub(crate) fn offer(continuation: &mut EffectContinuation, choice: E3ChoiceV1) {
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
        E3ChoiceKindV1::RetargetCopyV1 { copy, rest } => {
            let current = crate::engine::spell_copy_targets(state, *copy);
            let target = selected
                .first()
                .copied()
                .filter(|target| current.first() != Some(target));
            crate::engine::finish_spell_copy_targets(state, *copy, target)?;
            stage_copy_retargets(state, continuation, rest.clone(), choice.path.clone())
        }
        E3ChoiceKindV1::DoomsdayV1 => {
            let chosen = bindings
                .iter()
                .map(|binding| binding.object)
                .collect::<Vec<_>>();
            apply_doomsday(state, choice.player, &chosen)?;
            Ok(false)
        }
        E3ChoiceKindV1::OracleV1 { devotion, prefix } => {
            let kept = bindings.first().map(|binding| binding.object);
            apply_oracle(state, choice.player, *devotion, prefix, kept)?;
            Ok(false)
        }
        E3ChoiceKindV1::ShowAndTellV1 { chosen, later } => {
            let mut chosen = chosen.clone();
            if let Some(card) = bindings.first() {
                // The next player sees the choice before making theirs.
                state.reveal_hand_card(choice.player.opponent(), choice.player, card.object)?;
                chosen.push(*card);
            }
            stage_show_and_tell(state, continuation, later, chosen, choice.path.clone())
        }
        E3ChoiceKindV1::SneakAttackV1 { source } => {
            if let Some(card) = bindings.first() {
                sneak_in(state, *source, card.object)?;
            }
            Ok(false)
        }
        E3ChoiceKindV1::AnnihilatorV1 => {
            sacrifice_all(state, choice.player, &bindings);
            Ok(false)
        }
        E3ChoiceKindV1::ScryV1 { .. }
        | E3ChoiceKindV1::PunisherV1 { .. }
        | E3ChoiceKindV1::OublietteV1 { .. }
        | E3ChoiceKindV1::ZombieUnlessSacrificeV1 { .. }
        | E3ChoiceKindV1::MadWizardV1 { .. } => {
            dungeons::finish(state, continuation, choice, bindings)
        }
        E3ChoiceKindV1::E5bV1(kind) => crate::eternal_legacy_e5b_v1::finish_selection(
            state,
            continuation,
            choice,
            kind,
            bindings,
        ),
        E3ChoiceKindV1::AtraxaV1 { revealed, picked } => {
            let mut picked = picked.clone();
            let done = bindings.is_empty();
            picked.extend(bindings.first().copied());
            stage_atraxa(
                state,
                continuation,
                choice.player,
                revealed.clone(),
                picked,
                choice.path.clone(),
                done,
            )
        }
        E3ChoiceKindV1::E5aV1(kind) => {
            crate::eternal_e5a_v1::choices::finish(state, continuation, choice, kind, bindings)
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
        EternalOpV1::SearchBasicLandIfAtMostLandsV1 { lands } => {
            // "If you control four or fewer lands, search your library for a
            // basic land card, put it onto the battlefield tapped, then
            // shuffle." Checked as it resolves.
            if controlled_land_count(state, continuation.ctx.controller) <= usize::from(*lands) {
                continuation.frames.push(EffectFrame::Program {
                    op: EffectOp::SearchLibraryCardsToDestination {
                        player: PlayerRef::Controller,
                        filter: crate::effect::LibraryCardFilter::BasicLand,
                        max_targets: 1,
                        destination: crate::effect::LibrarySearchDestinationV1::Battlefield {
                            tapped: true,
                        },
                    },
                    path,
                });
            }
            Ok(false)
        }
        EternalOpV1::DoomsdayPileV1 => {
            let player = continuation.ctx.controller;
            let cards = state.players[player.index()]
                .library
                .iter()
                .copied()
                .chain(graveyard_card_ids(state, player))
                .map(|card| bind(state, card))
                .collect::<Vec<_>>();
            let count = cards.len().min(5);
            if count <= 1 && count == cards.len() {
                let chosen = cards.iter().map(|card| card.object).collect::<Vec<_>>();
                apply_doomsday(state, player, &chosen)?;
                return Ok(false);
            }
            let count = u16::try_from(count).unwrap_or(5);
            offer(
                continuation,
                E3ChoiceV1 {
                    kind: E3ChoiceKindV1::DoomsdayV1,
                    player,
                    candidates: cards
                        .iter()
                        .map(|card| Target::Object(card.object))
                        .collect(),
                    bindings: cards,
                    min: count,
                    max: count,
                    ordered: true,
                    path,
                    remaining: continuation.frames.clone(),
                },
            );
            Ok(true)
        }
        EternalOpV1::ThassasOracleV1 => {
            let player = continuation.ctx.controller;
            let devotion = devotion_to_blue(state, player);
            let library = state.players[player.index()].library.clone();
            let looked = library
                .len()
                .min(usize::try_from(devotion).unwrap_or(usize::MAX));
            // Only the controller looks.
            state.reveal_library_top(player, player, looked);
            let prefix = library[..looked]
                .iter()
                .map(|&card| bind(state, card))
                .collect::<Vec<_>>();
            if prefix.is_empty() {
                apply_oracle(state, player, devotion, &prefix, None)?;
                return Ok(false);
            }
            offer(
                continuation,
                E3ChoiceV1 {
                    kind: E3ChoiceKindV1::OracleV1 {
                        devotion,
                        prefix: prefix.clone(),
                    },
                    player,
                    candidates: prefix
                        .iter()
                        .map(|card| Target::Object(card.object))
                        .collect(),
                    bindings: prefix,
                    min: 0,
                    max: 1,
                    ordered: false,
                    path,
                    remaining: continuation.frames.clone(),
                },
            );
            Ok(true)
        }
        EternalOpV1::RoomAbilityV1 { dungeon, room } => {
            dungeons::resolve_room(state, continuation, *dungeon, *room, path)
        }
        EternalOpV1::AcererakEntersV1 => {
            dungeons::resolve_acererak_enters(state, continuation, path)?;
            Ok(false)
        }
        EternalOpV1::AcererakAttacksV1 => {
            dungeons::resolve_acererak_attacks(state, continuation, path)
        }
        EternalOpV1::AnnihilatorV1 { count } => {
            let defender = continuation.ctx.controller.opponent();
            let permanents = controlled_permanents(state, defender);
            if permanents.len() <= usize::from(*count) {
                sacrifice_all(state, defender, &permanents);
                return Ok(false);
            }
            offer(
                continuation,
                E3ChoiceV1 {
                    kind: E3ChoiceKindV1::AnnihilatorV1,
                    player: defender,
                    candidates: permanents
                        .iter()
                        .map(|permanent| Target::Object(permanent.object))
                        .collect(),
                    bindings: permanents,
                    min: u16::from(*count),
                    max: u16::from(*count),
                    ordered: false,
                    path,
                    remaining: continuation.frames.clone(),
                },
            );
            Ok(true)
        }
        EternalOpV1::AtraxaRevealV1 => {
            let player = continuation.ctx.controller;
            let library = state.players[player.index()].library.clone();
            let count = library.len().min(10);
            reveal_top_to_all(state, player, count);
            let revealed = library[..count]
                .iter()
                .map(|&card| bind(state, card))
                .collect();
            stage_atraxa(
                state,
                continuation,
                player,
                revealed,
                Vec::new(),
                path,
                false,
            )
        }
        EternalOpV1::ShowAndTellV1 => {
            let first = state.active_player;
            stage_show_and_tell(
                state,
                continuation,
                &[first, first.opponent()],
                Vec::new(),
                path,
            )
        }
        EternalOpV1::SneakAttackV1 => {
            let Some(source) = continuation.ctx.ability_source_contract else {
                return Err("Sneak Attack lost its source".to_string());
            };
            let player = continuation.ctx.controller;
            let cards = hand_bindings(state, player, |def| def.has_type(CardType::Creature));
            if cards.is_empty() {
                return Ok(false);
            }
            offer(
                continuation,
                E3ChoiceV1 {
                    kind: E3ChoiceKindV1::SneakAttackV1 { source },
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
        EternalOpV1::E5bV1(op) => {
            crate::eternal_legacy_e5b_v1::drive(state, continuation, op, path)
        }
        EternalOpV1::E5aV1(op) => {
            crate::eternal_e5a_v1::choices::drive(state, continuation, op, path)
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
        EternalOpV1::MayImprintFromHandV1
            | EternalOpV1::ChangeTargetOfTargetSpellV1
            | EternalOpV1::SearchBasicLandIfAtMostLandsV1 { .. }
            | EternalOpV1::DoomsdayPileV1
            | EternalOpV1::ThassasOracleV1
            | EternalOpV1::ShowAndTellV1
            | EternalOpV1::SneakAttackV1
            | EternalOpV1::AnnihilatorV1 { .. }
            | EternalOpV1::AtraxaRevealV1
            | EternalOpV1::RoomAbilityV1 { .. }
            | EternalOpV1::AcererakEntersV1
            | EternalOpV1::AcererakAttacksV1
    ) || matches!(op, EternalOpV1::E5bV1(op) if op.contains_player_choice())
}

/// The cards (not tokens or copies) in `player`'s graveyard, in order.
fn graveyard_card_ids(state: &GameState, player: PlayerId) -> Vec<ObjectId> {
    state.players[player.index()]
        .graveyard
        .iter()
        .copied()
        .filter(|&card| {
            let object = state.objects.get(card);
            object.owner == player && !object.v4.is_token && object.spell_copy_origin.is_none()
        })
        .collect()
}

/// "The player wins the game": in a two-player game the opponent loses.
fn win_game(state: &mut GameState, player: PlayerId) {
    state.players[player.opponent().index()].has_lost = true;
}

// ---- Doomsday ----------------------------------------------------------------------

/// Doomsday: "Search your library and graveyard for five cards and exile
/// the rest. Put the chosen cards on top of your library in any order. You
/// lose half your life, rounded up."
pub fn doomsday() -> EffectOp {
    EffectOp::Sequence(vec![
        eternal(EternalOpV1::DoomsdayPileV1),
        EffectOp::LoseHalfLifeRoundedUp {
            player: PlayerRef::Controller,
        },
    ])
}

/// Exiles every other card of `player`'s library and graveyard, then makes
/// `chosen` (first on top) the whole library. Only `player` knows its order.
fn apply_doomsday(
    state: &mut GameState,
    player: PlayerId,
    chosen: &[ObjectId],
) -> Result<(), String> {
    let exiled = state.players[player.index()]
        .library
        .iter()
        .copied()
        .chain(graveyard_card_ids(state, player))
        .filter(|card| !chosen.contains(card))
        .map(|card| ProposedEvent::zone_change(card, Zone::Exile))
        .collect::<Vec<_>>();
    if !exiled.is_empty() {
        event::propose_and_commit_batch(state, exiled);
    }
    for &card in chosen {
        if state.objects.get(card).zone == Zone::Graveyard {
            event::propose_and_commit(state, ProposedEvent::zone_change(card, Zone::Library));
        }
    }
    let library = &state.players[player.index()].library;
    if library.len() != chosen.len() || chosen.iter().any(|card| !library.contains(card)) {
        return Err("Doomsday's pile is not the whole library".to_string());
    }
    state.reorder_library_top(player, chosen, &[player])
}

// ---- Thassa's Oracle -----------------------------------------------------------------

/// "Each {U} in the mana costs of permanents you control counts toward
/// your devotion to blue." Permanents showing a face without a mana cost
/// count nothing.
pub(crate) fn devotion_to_blue(state: &GameState, player: PlayerId) -> u32 {
    let blue = |pip: &Pip| match *pip {
        Pip::Colored(color) | Pip::Phyrexian(color) | Pip::PhyrexianAnyColor(color) => {
            color == ManaColor::U
        }
        Pip::Hybrid(first, second) => first == ManaColor::U || second == ManaColor::U,
    };
    state.players[player.index()]
        .battlefield
        .iter()
        .map(|&permanent| {
            let object = state.objects.get(permanent);
            if object.v4.face_index != 0 || object.v4.face_down_v1.is_some() {
                return 0;
            }
            CARD_DEFS.get(object.card_def as usize).map_or(0, |def| {
                def.cost.pips.iter().filter(|pip| blue(pip)).count()
            })
        })
        .sum::<usize>()
        .try_into()
        .unwrap_or(u32::MAX)
}

/// Thassa's Oracle's entry: "look at the top X cards of your library,
/// where X is your devotion to blue. Put up to one of them on top of your
/// library and the rest on the bottom of your library in a random order. If
/// X is greater than or equal to the number of cards in your library, you
/// win the game."
fn thassas_oracle() -> EffectOp {
    eternal(EternalOpV1::ThassasOracleV1)
}

fn apply_oracle(
    state: &mut GameState,
    player: PlayerId,
    devotion: u32,
    prefix: &[EffectObjectBinding],
    kept: Option<ObjectId>,
) -> Result<(), String> {
    if !prefix.is_empty() {
        let expected = prefix
            .iter()
            .map(|card| ObjectLinkV4 {
                object: card.object,
                zone_change_count: card.expected_zone_change_count,
            })
            .collect::<Vec<_>>();
        let top = kept.into_iter().collect::<Vec<_>>();
        let bottom = prefix
            .iter()
            .map(|card| card.object)
            .filter(|card| Some(*card) != kept)
            .collect::<Vec<_>>();
        state.apply_scry_result(player, &expected, &top, &bottom)?;
        state
            .randomize_library_bottom_v1(player, bottom.len())
            .map_err(|error| error.to_string())?;
    }
    let library = u32::try_from(state.players[player.index()].library.len()).unwrap_or(u32::MAX);
    if devotion >= library {
        win_game(state, player);
    }
    Ok(())
}

// ---- Jace, Wielder of Mysteries ------------------------------------------------------

pub(crate) const JACE: &str = "Jace, Wielder of Mysteries";

/// Jace's +1: "Target player mills two cards. Draw a card."
pub fn jace_plus_one() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::MillCards {
            player: PlayerRef::Target(0),
            count: 2,
        },
        EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 1,
        },
    ])
}

/// Jace's -8: "Draw seven cards. Then if your library has no cards in it,
/// you win the game."
pub fn jace_minus_eight() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 7,
        },
        eternal(EternalOpV1::WinIfLibraryEmptyV1),
    ])
}

/// Jace's static ability: "If you would draw a card while your library has
/// no cards in it, you win the game instead."
pub(crate) fn wins_instead_of_drawing_from_empty_library(
    state: &GameState,
    player: PlayerId,
) -> bool {
    state.players[player.index()].library.is_empty()
        && state.players[player.index()]
            .battlefield
            .iter()
            .any(|&permanent| {
                state.objects.get(permanent).controller == player
                    && printed_battlefield_source(state, permanent, JACE)
            })
}

/// Applies Jace's replacement for a draw `player` would make.
pub(crate) fn replace_empty_library_draw(state: &mut GameState, player: PlayerId) {
    win_game(state, player);
}

// ---- Lion's Eye Diamond -----------------------------------------------------------------

/// "Activate only as an instant": while `player` holds priority with no
/// spell being cast, ability being activated or effect resolving.
pub(crate) fn instant_speed_mana_ability_allowed(state: &GameState, player: PlayerId) -> bool {
    state.priority_player == player
        && state.engine.pending_cast.is_none()
        && state.engine.pending_activation.is_none()
        && state.engine.pending_effect.is_none()
        && state.engine.pending_discard.is_none()
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
/// protection from everything, and damage from colored spells to Emrakul.
pub(crate) fn damage_prevented_by_protection(
    state: &GameState,
    source: ObjectId,
    target: Target,
) -> bool {
    let protected = match target {
        Target::Player(player) => player_protected(state, player),
        Target::Object(object) => protected_from_spell(state, object, source),
        Target::StackItem(_) => false,
    };
    protected || crate::eternal_e5a_v1::statics::damage_prevented(state, source, target)
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

/// Modal double-faced cards whose back face is a land played from hand:
/// Sink into Stupor // Soporific Springs and wave E4's Witch Enchanter //
/// Witch-Blessed Meadow.
pub(crate) fn has_land_back_face(name: &str) -> bool {
    name == SINK_INTO_STUPOR || name == "Witch Enchanter"
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
    // Miracle: whether to reveal a card drawn as its owner's first.
    if let Some(decision) = crate::eternal_legacy_e5b_v1::pending_decision(state) {
        return Some(decision);
    }
    if let Some(entering) = live_springs_entry(state) {
        let player = state.objects.get(entering.object).controller;
        // Option 1 pays 3 life, which a player with less cannot pay (119.4).
        let option_count = if state.players[player.index()].life >= 3 {
            2
        } else {
            1
        };
        return Some(crate::engine::Decision::ChooseEffectOption {
            player,
            source: entering.object,
            option_count,
        });
    }
    // Pithing Needle: "As this artifact enters, choose a card name."
    if let Some(decision) = crate::eternal_lurrus_v1::entry_choice_decision(state) {
        return Some(decision);
    }
    // Serra's Emissary: "As this creature enters, choose a card type."
    if let Some(decision) = crate::eternal_e5a_v1::statics::entry_choice_decision(state) {
        return Some(decision);
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
    if crate::eternal_legacy_e5b_v1::pending_decision(state).is_some() {
        crate::eternal_legacy_e5b_v1::answer_pending_decision(state, option)?;
        return Ok(());
    }
    if let Some(entering) = live_springs_entry(state) {
        let player = state.objects.get(entering.object).controller;
        if option == 1 && state.players[player.index()].life < 3 {
            return Err("the land face needs 3 life to pay".to_string());
        }
        if option > 1 {
            return Err("the land face's life payment has two options".to_string());
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
    if crate::eternal_lurrus_v1::answer_entry_choice(state, option)? {
        return Ok(());
    }
    if crate::eternal_e5a_v1::statics::answer_entry_choice(state, option)? {
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
        || active_veils(state, live.controller).next().is_some()
        || crate::eternal_lurrus_v1::spell_cannot_be_countered(state, object)
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
const THASSAS_ORACLE_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, thassas_oracle)];
/// Storm: "When you cast this spell, copy it ..." functions from the stack.
const FLUSTERSTORM_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    home_zone: Zone::Stack,
    ..trigger(TriggerCondition::CastSelf, storm_trigger)
}];
/// Emrakul: "When you cast this spell, take an extra turn after this one",
/// annihilator 6, and "When Emrakul is put into a graveyard from anywhere,
/// its owner shuffles their graveyard into their library."
const EMRAKUL_TRIGGERS: [TriggeredAbilityDef; 3] = [
    TriggeredAbilityDef {
        home_zone: Zone::Stack,
        ..trigger(TriggerCondition::CastSelf, crate::extra_turn_v1::recipe)
    },
    trigger(TriggerCondition::Attacks, annihilator_six),
    TriggeredAbilityDef {
        home_zone: Zone::Graveyard,
        ..eternal_trigger(
            EternalTriggerV1::PutIntoGraveyardFromAnywhereV1,
            shuffle_graveyard_into_library,
        )
    },
];
const ATRAXA_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(TriggerCondition::Etb, atraxa_reveal)];
const RAPH_AND_MIKEY_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Attacks, raph_and_mikey_attack)];
/// Acererak: "When Acererak enters, if you haven't completed Tomb of
/// Annihilation, return Acererak to its owner's hand and venture into the
/// dungeon." and its attack trigger.
const ACERERAK_TRIGGERS: [TriggeredAbilityDef; 2] = [
    eternal_trigger(
        EternalTriggerV1::EntersIfTombNotCompletedV1,
        dungeons::acererak_enters,
    ),
    trigger(TriggerCondition::Attacks, dungeons::acererak_attacks),
];

pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        THE_ONE_RING => &THE_ONE_RING_TRIGGERS,
        CHROME_MOX => &CHROME_MOX_TRIGGERS,
        FLUSTERSTORM | BRAIN_FREEZE | TENDRILS_OF_AGONY => &FLUSTERSTORM_TRIGGERS,
        THASSAS_ORACLE => &THASSAS_ORACLE_TRIGGERS,
        EMRAKUL => &EMRAKUL_TRIGGERS,
        ATRAXA => &ATRAXA_TRIGGERS,
        RAPH_AND_MIKEY => &RAPH_AND_MIKEY_TRIGGERS,
        dungeons::ACERERAK => &ACERERAK_TRIGGERS,
        _ => &[],
    }
}

pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    match name {
        dungeons::ACERERAK => Some(dungeons::trigger_target_spec(effect)),
        THE_ONE_RING | CHROME_MOX | CITY_OF_TRAITORS | FLUSTERSTORM | BRAIN_FREEZE
        | TENDRILS_OF_AGONY | THASSAS_ORACLE | EMRAKUL | ATRAXA | RAPH_AND_MIKEY => {
            Some(TargetSpec::None)
        }
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
        EternalTriggerV1::EntersIfTombNotCompletedV1 => {
            matches!(
                events[index],
                CommittedEvent::ZoneChange {
                    object,
                    to: Zone::Battlefield,
                    ..
                } if object == source
            ) && dungeons::acererak_entered(state, source)
        }
        EternalTriggerV1::PutIntoGraveyardFromAnywhereV1 => {
            // Emrakul: functions from the graveyard it was just put into.
            let live = state.objects.get(source);
            matches!(
                events[index],
                CommittedEvent::ZoneChange {
                    object,
                    to: Zone::Graveyard,
                    ..
                } if object == source
            ) && live.zone == Zone::Graveyard
                && state.players[live.owner.index()]
                    .graveyard
                    .contains(&source)
                && !live.v4.is_token
                && live.spell_copy_origin.is_none()
                && printed(state, source).name == EMRAKUL
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
        EternalOpV1::SacrificeSourceV1 => {
            let Some(contract) = ctx.ability_source_contract else {
                return;
            };
            let source = ObjectLinkV4 {
                object: contract.source,
                zone_change_count: contract.zone_change_count,
            };
            if link_live(state, source, Zone::Battlefield)
                && state.objects.get(source.object).controller == ctx.controller
            {
                event::log_sacrifice(state, source.object);
                event::propose_and_commit(
                    state,
                    ProposedEvent::zone_change(source.object, Zone::Graveyard),
                );
            }
        }
        EternalOpV1::VeilOfSummerV1 => {
            let player = ctx.controller;
            if opponent_cast_blue_or_black_spell_this_turn(state, player) {
                crate::effect::execute(
                    &EffectOp::DrawCards {
                        player: PlayerRef::Controller,
                        count: 1,
                    },
                    ctx,
                    state,
                );
            }
            let permanents = state.players[player.index()]
                .battlefield
                .iter()
                .map(|&object| link(state, object))
                .collect();
            let turn = state.turn_key_v1();
            let e3 = e3_mut(state);
            e3.veils.retain(|veil| veil.turn == turn);
            e3.veils.push(VeilV1 {
                player,
                turn,
                permanents,
            });
        }
        EternalOpV1::EnterRoomV1 { dungeon, room } => {
            if dungeons::enter_room(state, ctx, *dungeon, *room).is_err() {
                halt(state, ctx.source);
            }
        }
        EternalOpV1::RevealUntilCreatureAttackingV1 => {
            if reveal_until_creature_attacking(state, ctx.controller).is_err() {
                halt(state, ctx.source);
            }
        }
        EternalOpV1::WinIfLibraryEmptyV1 => {
            if state.players[ctx.controller.index()].library.is_empty() {
                win_game(state, ctx.controller);
            }
        }
        EternalOpV1::MayImprintFromHandV1
        | EternalOpV1::ChangeTargetOfTargetSpellV1
        | EternalOpV1::SearchBasicLandIfAtMostLandsV1 { .. }
        | EternalOpV1::DoomsdayPileV1
        | EternalOpV1::ThassasOracleV1
        | EternalOpV1::ShowAndTellV1
        | EternalOpV1::SneakAttackV1
        | EternalOpV1::AnnihilatorV1 { .. }
        | EternalOpV1::AtraxaRevealV1
        | EternalOpV1::RoomAbilityV1 { .. }
        | EternalOpV1::AcererakEntersV1
        | EternalOpV1::AcererakAttacksV1 => {
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
        e3.weakened.retain(|effect| effect.holder != player);
        e3.pacified.retain(|effect| effect.holder != player);
    }
}

/// Whether `player` may be targeted by `controller`'s spell or ability from
/// `source` under E3 protections: The One Ring's protection from
/// everything, Veil of Summer's hexproof from blue and from black.
pub(crate) fn player_targetable(
    state: &GameState,
    player: PlayerId,
    controller: PlayerId,
    source: Option<ObjectId>,
) -> bool {
    !player_protected(state, player)
        && (controller == player
            || active_veils(state, player).next().is_none()
            || !source_blue_or_black(state, source))
        // Serra's Emissary: protection from the chosen card type.
        && !source.is_some_and(|source| {
            crate::eternal_e5a_v1::statics::player_protected_from(state, player, source)
        })
}

/// Whether `object` may be targeted by `controller`'s spell or ability from
/// `source` under E3 protections (Veil of Summer's hexproof from blue and
/// from black).
pub(crate) fn object_targetable(
    state: &GameState,
    object: ObjectId,
    controller: PlayerId,
    source: Option<ObjectId>,
) -> bool {
    let Some(live) = state.objects.try_get(object) else {
        return true;
    };
    // Serra's Emissary: creatures with protection from the chosen card type.
    if source.is_some_and(|source| {
        crate::eternal_e5a_v1::statics::object_protected_from(state, object, source)
    }) {
        return false;
    }
    if live.controller == controller || live.zone != Zone::Battlefield {
        return true;
    }
    let entry = ObjectLinkV4 {
        object,
        zone_change_count: live.zone_change_count,
    };
    !(active_veils(state, live.controller).any(|veil| veil.permanents.contains(&entry))
        && source_blue_or_black(state, source))
}

// ---- Veil of Summer ---------------------------------------------------------------

/// Veil of Summer: "Draw a card if an opponent has cast a blue or black
/// spell this turn. Spells you control can't be countered this turn. You
/// and permanents you control gain hexproof from blue and from black until
/// end of turn."
pub fn veil_of_summer() -> EffectOp {
    eternal(EternalOpV1::VeilOfSummerV1)
}

fn blue_or_black_mask() -> u8 {
    crate::card_def::mana_color_mask(ManaColor::U) | crate::card_def::mana_color_mask(ManaColor::B)
}

/// Whether an opponent of `player` cast a blue or black spell this turn.
/// The turn begins at its `UpkeepBegan` marker; no spell is cast earlier.
fn opponent_cast_blue_or_black_spell_this_turn(state: &GameState, player: PlayerId) -> bool {
    for event in state.engine.event_history.iter().rev() {
        match event {
            CommittedEvent::UpkeepBegan { .. } => return false,
            CommittedEvent::SpellCast { spell, controller } if *controller != player => {
                let colors = state
                    .objects
                    .try_get(*spell)
                    .and_then(|object| CARD_DEFS.get(object.card_def as usize))
                    .map_or(0, |def| crate::card_def::mana_colors_mask(def.colors));
                if colors & blue_or_black_mask() != 0 {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

fn active_veils(state: &GameState, player: PlayerId) -> impl Iterator<Item = &VeilV1> {
    let turn = state.turn_key_v1();
    e3(state)
        .into_iter()
        .flat_map(|e3| e3.veils.iter())
        .filter(move |veil| veil.player == player && veil.turn == turn)
}

/// Whether a targeting source is blue or black.
fn source_blue_or_black(state: &GameState, source: Option<ObjectId>) -> bool {
    source.is_some_and(|source| {
        state.objects.try_get(source).is_some()
            && crate::engine::object_color_mask(state, source) & blue_or_black_mask() != 0
    })
}

// ---- Edge of Autumn --------------------------------------------------------------

fn controlled_land_count(state: &GameState, player: PlayerId) -> usize {
    state.players[player.index()]
        .battlefield
        .iter()
        .filter(|&&object| crate::engine::object_has_type(state, object, CardType::Land))
        .count()
}

/// Edge of Autumn's spell ability. Its cycling is an activated ability.
pub fn edge_of_autumn() -> EffectOp {
    eternal(EternalOpV1::SearchBasicLandIfAtMostLandsV1 { lands: 4 })
}

// ---- Dark Ritual, Personal Tutor ---------------------------------------------------

/// Dark Ritual: "Add {B}{B}{B}."
pub fn dark_ritual() -> EffectOp {
    EffectOp::AddMana {
        player: PlayerRef::Controller,
        colors: vec![ManaColor::B, ManaColor::B, ManaColor::B],
    }
}

/// Personal Tutor: "Search your library for a sorcery card, reveal it, then
/// shuffle and put that card on top."
pub fn personal_tutor() -> EffectOp {
    EffectOp::SearchLibraryCardsToDestination {
        player: PlayerRef::Controller,
        filter: crate::effect::LibraryCardFilter::CardWithTypeV1(CardType::Sorcery),
        max_targets: 1,
        destination: crate::effect::LibrarySearchDestinationV1::LibraryTopAfterShuffle,
    }
}

// ---- Flusterstorm and Storm copies -------------------------------------------------

/// Cards with Storm in this module.
pub(crate) fn has_storm(name: &str) -> bool {
    name == FLUSTERSTORM || name == BRAIN_FREEZE || name == TENDRILS_OF_AGONY
}

/// Whether a Storm trigger copies a targeted spell, whose copies may choose
/// new targets.
pub(crate) fn targeted_storm(binding: &crate::effect::StormCopyBindingV1) -> bool {
    CARD_DEFS
        .get(usize::from(binding.source_contract.card_def))
        .is_some_and(|def| has_storm(def.name) && def.target_spec != TargetSpec::None)
}

/// Records a Storm spell's targets as it is cast, for copies made after it
/// has left the stack.
pub(crate) fn note_cast(state: &mut GameState, stack_item: StackItemId) {
    let Some(item) = state
        .stack
        .iter()
        .find(|item| item.v4.stack_item_id == stack_item && item.kind == StackItemKind::Spell)
    else {
        return;
    };
    if !CARD_DEFS
        .get(state.objects.get(item.source).card_def as usize)
        .is_some_and(|def| has_storm(def.name))
    {
        return;
    }
    let record = StormCastV1 {
        item: stack_item,
        target_spec: item.v4.target_spec.unwrap_or_default(),
        targets: item.targets.clone(),
        contracts: item.v4.target_contracts.clone(),
    };
    e3_mut(state).storm_casts.push(record);
}

/// "Storm (When you cast this spell, copy it for each spell cast before it
/// this turn. You may choose new targets for the copies.)" for a targeted
/// spell: create the copies with the original's targets, then offer each
/// copy a new target in turn. `Ok(true)` when a choice is staged.
pub(crate) fn storm_copies(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    binding: crate::effect::StormCopyBindingV1,
    path: Vec<u16>,
) -> Result<bool, String> {
    let ctx = &continuation.ctx;
    if ctx.source != binding.source_contract.source
        || ctx.controller != binding.source_contract.controller
        || !ctx.targets.is_empty()
    {
        return Err("Storm resolution context changed from its bound trigger".to_string());
    }
    let record = e3(state)
        .and_then(|e3| {
            e3.storm_casts
                .iter()
                .find(|record| record.item == binding.producing_stack_item)
                .cloned()
        })
        .ok_or("Storm copies lost the targets of their cast")?;
    let copies = crate::engine::create_targeted_storm_copies_v1(
        state,
        binding,
        &record.targets,
        &record.contracts,
        record.target_spec,
    )?;
    stage_copy_retargets(state, continuation, copies, path)
}

/// Offers the next Storm copy a new target. Copies without another legal
/// target keep theirs.
fn stage_copy_retargets(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    mut copies: Vec<StackItemId>,
    path: Vec<u16>,
) -> Result<bool, String> {
    while !copies.is_empty() {
        let copy = copies.remove(0);
        let candidates = crate::engine::spell_copy_retarget_candidates(state, copy);
        if candidates.is_empty() {
            crate::engine::finish_spell_copy_targets(state, copy, None)?;
            continue;
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
                kind: E3ChoiceKindV1::RetargetCopyV1 { copy, rest: copies },
                player: continuation.ctx.controller,
                candidates,
                bindings,
                min: 0,
                max: 1,
                ordered: false,
                path,
                remaining: continuation.frames.clone(),
            },
        );
        return Ok(true);
    }
    Ok(false)
}

/// Flusterstorm: "Counter target instant or sorcery spell unless its
/// controller pays {1}."
pub fn flusterstorm() -> EffectOp {
    EffectOp::CounterTargetUnlessPaysGeneric {
        target: crate::effect::TargetRef::Target(0),
        generic: 1,
    }
}

fn storm_trigger() -> EffectOp {
    EffectOp::MaterializeStormCopies
}

// ---- City of Traitors ----------------------------------------------------------------

/// Records a land played by `player` (a special action), for "When you play
/// another land, sacrifice this land."
pub(crate) fn note_land_played(state: &mut GameState, player: PlayerId, land: ObjectId) {
    let Some(city) = card_id_by_name(CITY_OF_TRAITORS) else {
        return;
    };
    let watched = state.players[player.index()]
        .battlefield
        .iter()
        .any(|&object| object != land && state.objects.get(object).card_def == city);
    if watched {
        let played = link(state, land);
        e3_mut(state).lands_played.push((player, played));
    }
}

fn sacrifice_source() -> EffectOp {
    eternal(EternalOpV1::SacrificeSourceV1)
}

/// The E3 triggers this collection sets off that have no printed trigger
/// condition of their own (City of Traitors' "when you play another land").
pub(crate) fn delayed_triggers(
    state: &mut GameState,
    _events: &[CommittedEvent],
) -> Vec<crate::trigger::PendingTrigger> {
    let mut triggers = dungeons::room_triggers(state);
    let Some(played) = state
        .eternal_v1
        .as_mut()
        .and_then(|eternal| eternal.e3.as_deref_mut())
        .map(|e3| std::mem::take(&mut e3.lands_played))
    else {
        return triggers;
    };
    let Some(city) = card_id_by_name(CITY_OF_TRAITORS) else {
        return triggers;
    };
    for (player, land) in played {
        for &object in &state.players[player.index()].battlefield {
            if object != land.object
                && state.objects.get(object).card_def == city
                && printed_battlefield_source(state, object, CITY_OF_TRAITORS)
            {
                triggers.push(super::delayed_trigger(
                    AbilitySourceContractV4::capture(state, object),
                    player,
                    sacrifice_source(),
                ));
            }
        }
    }
    triggers
}

/// Triggers this module creates outside the printed trigger tables.
pub(crate) fn delayed_effect_matches(name: &str, effect: &EffectOp) -> bool {
    match name {
        CITY_OF_TRAITORS => *effect == sacrifice_source(),
        FLUSTERSTORM | BRAIN_FREEZE | TENDRILS_OF_AGONY => {
            matches!(effect, EffectOp::CreateStormCopies { .. })
        }
        dungeons::ACERERAK => dungeons::delayed_effect_matches(effect),
        _ => false,
    }
}

/// Fungi Cavern's -4/-0 on `object`, until its holder's next turn.
pub(crate) fn power_modifier(state: &GameState, object: ObjectId) -> i32 {
    let Some(e3) = e3(state) else {
        return 0;
    };
    if !state
        .objects
        .try_get(object)
        .is_some_and(|live| live.zone == Zone::Battlefield)
    {
        return 0;
    }
    let entry = link(state, object);
    let count = e3
        .weakened
        .iter()
        .filter(|effect| effect.object == entry)
        .count();
    -4 * i32::try_from(count).unwrap_or(i32::MAX / 4)
}

/// Twisted Caverns: `object` can't attack until its holder's next turn.
pub(crate) fn cant_attack(state: &GameState, object: ObjectId) -> bool {
    e3(state).is_some_and(|e3| {
        state.objects.try_get(object).is_some_and(|live| {
            let entry = ObjectLinkV4 {
                object,
                zone_change_count: live.zone_change_count,
            };
            e3.pacified.iter().any(|effect| effect.object == entry)
        })
    })
}

// ---- Street Wraith --------------------------------------------------------------------

/// Street Wraith's swampwalk: it can't be blocked while the defending player
/// controls a Swamp.
pub(crate) fn blocker_allowed(state: &GameState, attacker: ObjectId, blocker: ObjectId) -> bool {
    if !printed_battlefield_source(state, attacker, STREET_WRAITH) {
        return true;
    }
    let defender = state.objects.get(blocker).controller;
    !state.players[defender.index()]
        .battlefield
        .iter()
        .any(|&permanent| {
            crate::engine::object_has_type(state, permanent, CardType::Land)
                && crate::engine::has_effective_subtype(state, permanent, Subtype::Swamp)
        })
}

// ---- Omniscience and Aluren ---------------------------------------------------------

pub(crate) const OMNISCIENCE: &str = "Omniscience";
pub(crate) const ALUREN: &str = "Aluren";

/// Whether `player` controls a permanent named `name` whose printed
/// abilities apply.
fn controls_named(state: &GameState, player: PlayerId, name: &str) -> bool {
    state.players[player.index()]
        .battlefield
        .iter()
        .any(|&object| {
            state.objects.get(object).controller == player
                && printed_battlefield_source(state, object, name)
        })
}

/// Aluren: "Any player may cast creature spells with mana value 3 or less
/// without paying their mana costs and as though they had flash." Whether
/// `card` is such a creature card while an Aluren is on the battlefield.
pub(crate) fn aluren_flash(state: &GameState, card: ObjectId) -> bool {
    printed(state, card).has_type(CardType::Creature)
        && crate::engine::object_mana_value(state, card) <= 3
        && [PlayerId::P0, PlayerId::P1]
            .into_iter()
            .any(|player| controls_named(state, player, ALUREN))
}

/// Whether `player` may cast the hand card `card` without paying its mana
/// cost in some printed form (Omniscience: "You may cast spells from your
/// hand without paying their mana costs"; Aluren).
pub(crate) fn hand_free_permission_any_form(
    state: &GameState,
    card: ObjectId,
    player: PlayerId,
) -> bool {
    state
        .objects
        .try_get(card)
        .is_some_and(|live| live.zone == Zone::Hand && live.owner == player)
        && (controls_named(state, player, OMNISCIENCE) || aluren_flash(state, card))
}

/// Whether the cost quote of `source` in printed form `mode` is for a cast
/// without paying its mana cost: the pending hand cast has not chosen to pay
/// it, or a castability quote asks about the free cast (`free_quote`).
pub(crate) fn free_hand_cast_selected(
    state: &GameState,
    source: ObjectId,
    player: PlayerId,
    mode: u8,
) -> bool {
    let Some(live) = state.objects.try_get(source) else {
        return false;
    };
    let selected = match live.zone {
        Zone::Hand => e3(state).is_some_and(|e3| e3.free_quote == Some(source)),
        Zone::Stack => state.engine.pending_cast.as_ref().is_some_and(|pending| {
            pending.spell == source
                && pending.controller == player
                && matches!(
                    pending.hand_free_cast_v1,
                    Some(
                        crate::engine::HandFreeCastV1::Undecided
                            | crate::engine::HandFreeCastV1::WithoutManaCost
                    )
                )
        }),
        _ => false,
    };
    selected
        && (controls_named(state, player, OMNISCIENCE)
            || (mode == 0 && aluren_flash(state, source)))
}

/// Marks a projected state so cost quotes of `card` ask about its cast
/// without paying its mana cost.
pub(crate) fn set_free_quote(state: &mut GameState, card: Option<ObjectId>) {
    e3_mut(state).free_quote = card;
}

// ---- Show and Tell ------------------------------------------------------------------

/// The cards Show and Tell lets `player` put onto the battlefield: an
/// artifact, creature, enchantment or land card in their hand. An Aura
/// would need a host chosen as it enters (303.4f), which the engine models
/// only for Aura spells, so Auras are not offered.
fn show_and_tell_cards(state: &GameState, player: PlayerId) -> Vec<EffectObjectBinding> {
    hand_bindings(state, player, |def| {
        [
            CardType::Artifact,
            CardType::Creature,
            CardType::Enchantment,
            CardType::Land,
        ]
        .into_iter()
        .any(|card_type| def.has_type(card_type))
            && !def.subtypes.contains(&Subtype::Aura)
    })
}

/// Show and Tell: "Each player may put an artifact, creature, enchantment,
/// or land card from their hand onto the battlefield."
pub fn show_and_tell() -> EffectOp {
    eternal(EternalOpV1::ShowAndTellV1)
}

/// Stages the Show and Tell choice of the first player from `players` with
/// a card to offer, or puts the `chosen` cards onto the battlefield. 101.4:
/// the active player chooses first, then the other player, then the cards
/// enter at once.
fn stage_show_and_tell(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    players: &[PlayerId],
    chosen: Vec<EffectObjectBinding>,
    path: Vec<u16>,
) -> Result<bool, String> {
    for (index, &player) in players.iter().enumerate() {
        let cards = show_and_tell_cards(state, player);
        if cards.is_empty() {
            continue;
        }
        offer(
            continuation,
            E3ChoiceV1 {
                kind: E3ChoiceKindV1::ShowAndTellV1 {
                    chosen,
                    later: players[index + 1..].to_vec(),
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
        return Ok(true);
    }
    if chosen.iter().any(|binding| !binding_live(state, *binding)) {
        return Err("a Show and Tell card left its owner's hand".to_string());
    }
    for binding in chosen {
        event::propose_and_commit(
            state,
            ProposedEvent::zone_change(binding.object, Zone::Battlefield),
        );
    }
    Ok(false)
}

// ---- Sneak Attack -------------------------------------------------------------------

/// Sneak Attack: "{R}: You may put a creature card from your hand onto the
/// battlefield. That creature gains haste. Sacrifice the creature at the
/// beginning of the next end step."
pub fn sneak_attack() -> EffectOp {
    eternal(EternalOpV1::SneakAttackV1)
}

/// Puts the chosen creature card onto the battlefield with haste and its
/// end-step sacrifice, controlled by the ability's controller.
fn sneak_in(
    state: &mut GameState,
    source: AbilitySourceContractV4,
    card: ObjectId,
) -> Result<(), String> {
    event::propose_and_commit(state, ProposedEvent::zone_change(card, Zone::Battlefield));
    let live = state.objects.get(card);
    if live.zone != Zone::Battlefield {
        return Ok(());
    }
    let binding = bind(state, card);
    crate::standard_cards_v1::register_reflection_delayed_sacrifice(state, binding, source);
    Ok(())
}

// ---- Emrakul, the Aeons Torn --------------------------------------------------------

pub(crate) const EMRAKUL: &str = "Emrakul, the Aeons Torn";

fn annihilator_six() -> EffectOp {
    eternal(EternalOpV1::AnnihilatorV1 { count: 6 })
}

fn shuffle_graveyard_into_library() -> EffectOp {
    eternal(EternalOpV1::ShuffleGraveyardIntoLibrary)
}

/// "Protection from spells that are one or more colors": whether `object`
/// is an Emrakul that the colored spell `source` can't target or damage.
pub(crate) fn protected_from_spell(state: &GameState, object: ObjectId, source: ObjectId) -> bool {
    printed_battlefield_source(state, object, EMRAKUL)
        && state.objects.try_get(source).is_some_and(|live| {
            live.zone == Zone::Stack
                && state
                    .stack
                    .iter()
                    .any(|item| item.kind == StackItemKind::Spell && item.source == source)
        })
        && crate::engine::object_color_mask(state, source) != 0
}

/// The permanents `player` controls, bound to their incarnations.
fn controlled_permanents(state: &GameState, player: PlayerId) -> Vec<EffectObjectBinding> {
    state.players[player.index()]
        .battlefield
        .iter()
        .copied()
        .filter(|&object| state.objects.get(object).controller == player)
        .map(|object| bind(state, object))
        .collect()
}

/// Sacrifices every still-live permanent in `chosen` that `player`
/// controls, at once.
fn sacrifice_all(state: &mut GameState, player: PlayerId, chosen: &[EffectObjectBinding]) {
    let live = chosen
        .iter()
        .copied()
        .filter(|binding| {
            binding_live(state, *binding) && state.objects.get(binding.object).controller == player
        })
        .collect::<Vec<_>>();
    for binding in &live {
        event::log_sacrifice(state, binding.object);
    }
    if !live.is_empty() {
        event::propose_and_commit_batch(
            state,
            live.iter()
                .map(|binding| ProposedEvent::zone_change(binding.object, Zone::Graveyard))
                .collect(),
        );
    }
}

// ---- Atraxa, Grand Unifier ----------------------------------------------------------

pub(crate) const ATRAXA: &str = "Atraxa, Grand Unifier";

fn atraxa_reveal() -> EffectOp {
    eternal(EternalOpV1::AtraxaRevealV1)
}

/// The card types (205.2a) of the printed card `card`.
fn card_types(state: &GameState, card: ObjectId) -> Vec<CardType> {
    let mut types = printed(state, card).types.to_vec();
    types.sort_by_key(|card_type| *card_type as u8);
    types.dedup();
    types
}

/// Whether each of `cards` can stand for a different card type (Atraxa:
/// "For each card type, you may put a card of that type").
fn distinct_types_cover(state: &GameState, cards: &[ObjectId]) -> bool {
    fn assign(index: usize, options: &[Vec<CardType>], used: &mut Vec<CardType>) -> bool {
        if index == options.len() {
            return true;
        }
        for card_type in &options[index] {
            if !used.contains(card_type) {
                used.push(*card_type);
                if assign(index + 1, options, used) {
                    return true;
                }
                used.pop();
            }
        }
        false
    }
    let options = cards
        .iter()
        .map(|&card| card_types(state, card))
        .collect::<Vec<_>>();
    assign(0, &options, &mut Vec::new())
}

/// Offers Atraxa's next card, or ends the choice (when `done` or nothing
/// more can be taken): picked cards go to the hand and the rest of the
/// revealed cards to the bottom in a random order.
fn stage_atraxa(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    player: PlayerId,
    revealed: Vec<EffectObjectBinding>,
    picked: Vec<EffectObjectBinding>,
    path: Vec<u16>,
    done: bool,
) -> Result<bool, String> {
    if revealed
        .iter()
        .any(|binding| !binding_live(state, *binding))
    {
        return Err("an Atraxa card left the library".to_string());
    }
    let chosen = picked
        .iter()
        .map(|binding| binding.object)
        .collect::<Vec<_>>();
    let candidates = revealed
        .iter()
        .copied()
        .filter(|binding| {
            !picked.contains(binding) && {
                let mut with = chosen.clone();
                with.push(binding.object);
                distinct_types_cover(state, &with)
            }
        })
        .collect::<Vec<_>>();
    if !done && !candidates.is_empty() {
        offer(
            continuation,
            E3ChoiceV1 {
                kind: E3ChoiceKindV1::AtraxaV1 { revealed, picked },
                player,
                candidates: candidates
                    .iter()
                    .map(|card| Target::Object(card.object))
                    .collect(),
                bindings: candidates,
                min: 0,
                max: 1,
                ordered: false,
                path,
                remaining: continuation.frames.clone(),
            },
        );
        return Ok(true);
    }
    for card in &chosen {
        event::propose_and_commit(state, ProposedEvent::zone_change(*card, Zone::Hand));
    }
    let rest = revealed
        .iter()
        .filter(|binding| !picked.contains(binding))
        .collect::<Vec<_>>();
    bottom_in_random_order(
        state,
        player,
        &rest
            .iter()
            .map(|binding| ObjectLinkV4 {
                object: binding.object,
                zone_change_count: binding.expected_zone_change_count,
            })
            .collect::<Vec<_>>(),
    )?;
    Ok(false)
}

/// Puts `cards`, the current top of `player`'s library in order, on the
/// bottom in a random order.
fn bottom_in_random_order(
    state: &mut GameState,
    player: PlayerId,
    cards: &[ObjectLinkV4],
) -> Result<(), String> {
    if cards.is_empty() {
        return Ok(());
    }
    let objects = cards.iter().map(|card| card.object).collect::<Vec<_>>();
    state.apply_scry_result(player, cards, &[], &objects)?;
    state
        .randomize_library_bottom_v1(player, objects.len())
        .map_err(|error| error.to_string())
}

/// Reveals the top `count` cards of `player`'s library to both players.
fn reveal_top_to_all(state: &mut GameState, player: PlayerId, count: usize) {
    for observer in [PlayerId::P0, PlayerId::P1] {
        state.reveal_library_top(observer, player, count);
    }
}

// ---- Raph & Mikey, Troublemakers ----------------------------------------------------

pub(crate) const RAPH_AND_MIKEY: &str = "Raph & Mikey, Troublemakers";

fn raph_and_mikey_attack() -> EffectOp {
    eternal(EternalOpV1::RevealUntilCreatureAttackingV1)
}

/// "Reveal cards from the top of your library until you reveal a creature
/// card. Put that card onto the battlefield tapped and attacking and the
/// rest on the bottom of your library in a random order."
fn reveal_until_creature_attacking(state: &mut GameState, player: PlayerId) -> Result<(), String> {
    let library = state.players[player.index()].library.clone();
    let found = library
        .iter()
        .position(|&card| printed(state, card).has_type(CardType::Creature));
    let revealed = found.map_or(library.len(), |index| index + 1);
    reveal_top_to_all(state, player, revealed);
    let rest = library[..found.unwrap_or(library.len())]
        .iter()
        .map(|&card| link(state, card))
        .collect::<Vec<_>>();
    if let Some(index) = found {
        let creature = library[index];
        event::propose_and_commit(
            state,
            ProposedEvent::zone_change(creature, Zone::Battlefield),
        );
        if state.objects.get(creature).zone == Zone::Battlefield {
            state.objects.get_mut(creature).tapped = true;
            let combat = &mut state.engine.combat;
            if combat.attackers_declared
                && state.active_player == player
                && matches!(
                    state.step,
                    crate::state::Step::DeclareAttackers
                        | crate::state::Step::DeclareBlockers
                        | crate::state::Step::CombatDamage
                )
                && !combat.attackers.contains(&creature)
            {
                combat.attackers.push(creature);
            }
        }
    }
    bottom_in_random_order(state, player, &rest)
}
