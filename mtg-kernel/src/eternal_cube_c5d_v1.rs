//! Card programs for the eternal catalog's Vintage Cube wave C5d
//! (`docs/design/eternal_formats_support_v1.md`): each-player decision spells and hidden-zone artifacts.
//!
//! `eternal_cards_v1` forwards this module's effect leaves here, and the
//! recipe tables of `build_eternal_c5d_v1.rs` name its programs. Every
//! table is keyed by a printed name that occurs only in the eternal registry,
//! so the Pauper, FDN and Standard catalogs never reach it. Behavior was read
//! from each card's current Oracle text and, where present, its XMage source.
//!
//! Where each card lives:
//!
//! - Proft's Eidetic Memory: the entry draw is Omnath's (`EffectOp::DrawCards`
//!   as the root of the trigger). "No maximum hand size" is read where the
//!   cleanup discard is staged (`removes_hand_limit`, called from
//!   `eternal_cards_v1::no_maximum_hand_size`, the flag the emblems already
//!   set). The combat trigger is Luminarch Aspirant's shape (beginning of
//!   combat on your turn, target creature you control) with the trigger
//!   condition `C5dTriggerV1::YourCombatIfDrewMoreThanOneV1` (the intervening
//!   "if you've drawn more than one card this turn", read from
//!   `PlayerState::draws_this_turn` when the step begins and again as the
//!   trigger resolves) and X counters placed through the +1/+1 placement
//!   path (`event::add_plus_one_counters`, so counter doublers apply).
//! - Council's Judgment: a vote is a series of ordinary choose-a-permanent
//!   decisions. `C5dChoiceV1::VoteV1` offers each voter the nonland
//!   permanents the caster does not control (caster first), carrying the
//!   votes so far; the last vote tallies and exiles every permanent with the
//!   most votes in one batch. It does not target.
//! - Balance: three leaves (lands, cards in hand, creatures) in printed
//!   order. Each counts both players' cards as it begins, lets every player
//!   with more than the fewest choose that many to keep (active player first,
//!   `C5dChoiceV1::BalanceKeepV1`; the hand choice is private to its
//!   chooser), then discards or sacrifices everything else of both players in
//!   one batch.
//! - Palantir of Orthanc: its influence counters are a ledger in
//!   `C5dStateV1::influence` (Wishclaw Talisman's wish counters are the same
//!   shape). The end step trigger is Scry 2 between the counter and the
//!   opponent's `EffectOp::Choice` (draw, or let the controller mill X and
//!   lose life equal to the milled cards' total mana value). The draw sits in
//!   a chosen option, as Moon-Circuit Hacker's does.
//! - Fractured Identity: exile the target, then each player other than its
//!   controller creates a token copy made from the permanent's copiable
//!   values as it last existed (read before the exile, as Extravagant
//!   Replication's copy reads them). An Aura copy asks the new token's
//!   controller what it enchants (`C5dChoiceV1::AuraHostV1`).
//! - Memory Jar: the activation exiles both hands one card at a time, face
//!   down and visible only to the owner (foretell's hidden exile, with the
//!   concealed-exile and visibility marks of `eternal_pauper_e6a_v1::foretell`),
//!   then each player draws seven (Time Spiral's draws). The delayed "next
//!   end step" action is a record in `C5dStateV1::jars` that queues
//!   `JarReturnV1` at the next end step, even when the Jar has left.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

use crate::card_def::{CardType, Keywords, TargetSpec, CARD_DEFS};
use crate::effect::nonland_copy_v1::CopyTemplateV1;
use crate::effect::{
    EffectContinuation, EffectFrame, EffectObjectBinding, EffectOp, ExecCtx, PlayerRef,
};
use crate::eternal_cards_v1::e3::{E3ChoiceKindV1, E3ChoiceV1};
use crate::eternal_cards_v1::{EternalOpV1, EternalTriggerV1};
use crate::event::{self, CommittedEvent, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::standard_cards_v1::{StandardOpV1, StandardTriggerV1};
use crate::state::{AbilitySourceContractV4, GameState, ObjectLinkV4, Step, Target, Zone};
use crate::trigger::{PendingTrigger, TriggerCondition, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};

pub(crate) const MEMORY_JAR: &str = "Memory Jar";
pub(crate) const PALANTIR_OF_ORTHANC: &str = "Palantir of Orthanc";
pub(crate) const PROFTS_EIDETIC_MEMORY: &str = "Proft's Eidetic Memory";

// ---- State --------------------------------------------------------------------------

/// Per-game state for this module's cards, kept inside
/// `eternal_cards_v1::EternalStateV1`. Default (and absent from snapshots)
/// until a card of this wave needs it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct C5dStateV1 {
    /// Influence counters on each Palantir of Orthanc incarnation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub influence: Vec<(ObjectLinkV4, u8)>,
    /// Memory Jar activations whose "next end step" action has not resolved.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub jars: Vec<JarRecordV1>,
    /// The serial of the next Memory Jar record.
    #[serde(default, skip_serializing_if = "serial_is_zero")]
    pub next_jar_serial: u32,
}

fn serial_is_zero(serial: &u32) -> bool {
    *serial == 0
}

impl C5dStateV1 {
    pub(crate) fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// One resolved Memory Jar activation: the cards each player exiled face
/// down, returned at the next end step.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JarRecordV1 {
    pub serial: u32,
    /// The Jar's ability source as the ability was activated (the Jar was
    /// sacrificed as a cost).
    pub source: AbilitySourceContractV4,
    /// The exile incarnations of the cards exiled this way, both players'.
    pub cards: Vec<ObjectLinkV4>,
    /// The turn whose end step was already under way when the ability
    /// resolved: that end step does not return the cards.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_in_end_step: Option<u32>,
    /// The end step trigger is on its way to the stack.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub queued: bool,
}

fn read_state(state: &GameState) -> Option<&C5dStateV1> {
    state.eternal_v1.as_ref().map(|eternal| &eternal.c5d)
}

fn c5d_state(state: &mut GameState) -> &mut C5dStateV1 {
    &mut state.eternal_v1.get_or_insert_with(Default::default).c5d
}

// ---- Leaves -------------------------------------------------------------------------

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::C5dV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum C5dOpV1 {
    /// Proft's Eidetic Memory's combat trigger: X +1/+1 counters on target
    /// creature you control, where X is the cards you've drawn this turn
    /// minus one (nothing if that is not positive).
    ProftCountersV1,
    /// Council's Judgment: each player votes, caster first, then every
    /// permanent with the most votes is exiled. A choice.
    VoteExileV1,
    /// One part of Balance. A choice.
    BalanceV1 { part: BalancePartV1 },
    /// Palantir of Orthanc: an influence counter on the Palantir.
    PalantirCounterV1,
    /// Palantir of Orthanc, the declined branch: its controller mills X
    /// cards (X is the influence counters on it) and the opponent loses life
    /// equal to their total mana value. Runs in `drive`.
    PalantirMillV1,
    /// Fractured Identity: exile target 0, then each other player creates a
    /// token copy. A choice (an Aura copy's host).
    FracturedIdentityV1,
    /// Memory Jar's resolution, first half: each player exiles their hand
    /// face down, and the end step action is recorded.
    JarExileV1,
    /// Memory Jar's end step action (record `serial`): each player discards
    /// their hand, then each player gets the cards they exiled back.
    JarReturnV1 { serial: u32 },
}

/// The three parts of Balance, in printed order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BalancePartV1 {
    Lands,
    Hand,
    Creatures,
}

impl C5dOpV1 {
    /// Whether the leaf may stop for a player's choice (or a draw that a
    /// replacement can interrupt), so it runs in the resumable interpreter.
    pub(crate) fn contains_player_choice(&self) -> bool {
        matches!(
            self,
            Self::VoteExileV1
                | Self::BalanceV1 { .. }
                | Self::PalantirMillV1
                | Self::FracturedIdentityV1
        )
    }
}

/// A choice of this module, carried as `E3ChoiceKindV1::C5dV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum C5dChoiceV1 {
    /// Balance: exactly `keep` of the chooser's cards of `part` stay. `remove`
    /// are the cards earlier choosers lose, `later` the players still to
    /// choose.
    BalanceKeepV1 {
        part: BalancePartV1,
        keep: u16,
        remove: Vec<EffectObjectBinding>,
        later: Vec<PlayerId>,
    },
    /// Council's Judgment: one nonland permanent the caster does not control.
    /// `votes` are the earlier voters' votes, `later` the voters still to go.
    VoteV1 {
        votes: Vec<EffectObjectBinding>,
        later: Vec<PlayerId>,
    },
    /// Fractured Identity: what the Aura token copy enchants. One answer per
    /// token (`count` tokens), `selected` the earlier answers.
    AuraHostV1 {
        template: CopyTemplateV1,
        count: u16,
        selected: Vec<EffectObjectBinding>,
    },
}

impl C5dChoiceV1 {
    /// Whether the candidates are hidden from the other player.
    pub(crate) fn chooser_private(&self) -> bool {
        matches!(
            self,
            Self::BalanceKeepV1 {
                part: BalancePartV1::Hand,
                ..
            }
        )
    }

    pub(crate) fn public_purpose(&self) -> crate::rl::TargetSelectionPurposeV4 {
        if self.chooser_private() {
            crate::rl::TargetSelectionPurposeV4::CardSelection
        } else {
            crate::rl::TargetSelectionPurposeV4::PermanentSelection
        }
    }

    /// Object incarnations this choice refers to beyond its candidates.
    pub(crate) fn referenced_bindings(&self) -> Vec<EffectObjectBinding> {
        match self {
            Self::BalanceKeepV1 { remove, .. } => remove.clone(),
            Self::VoteV1 { votes, .. } => votes.clone(),
            Self::AuraHostV1 { selected, .. } => selected.clone(),
        }
    }
}

/// Trigger conditions owned by this module, carried as
/// `eternal_cards_v1::EternalTriggerV1::C5dV1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum C5dTriggerV1 {
    /// "At the beginning of combat on your turn, if you've drawn more than
    /// one card this turn" (Proft's Eidetic Memory).
    YourCombatIfDrewMoreThanOneV1,
}

fn c5d_op(op: C5dOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::C5dV1(op)))
}

fn halt(state: &mut GameState, source: ObjectId) {
    state.engine.halted = Some((
        crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
        source,
    ));
}

// ---- Programs -----------------------------------------------------------------------

/// Proft's Eidetic Memory's "When this enters, draw a card." The draw is a
/// root element of the program, so a draw replacement can interrupt it.
fn proft_draw() -> EffectOp {
    EffectOp::Sequence(vec![EffectOp::DrawCards {
        player: PlayerRef::Controller,
        count: 1,
    }])
}

fn proft_counters() -> EffectOp {
    c5d_op(C5dOpV1::ProftCountersV1)
}

/// Balance: "Each player chooses a number of lands they control equal to the
/// number of lands controlled by the player who controls the fewest, then
/// sacrifices the rest. Players discard cards and sacrifice creatures the
/// same way." Three instructions in printed order, each counting anew.
pub fn balance() -> EffectOp {
    EffectOp::Sequence(vec![
        c5d_op(C5dOpV1::BalanceV1 {
            part: BalancePartV1::Lands,
        }),
        c5d_op(C5dOpV1::BalanceV1 {
            part: BalancePartV1::Hand,
        }),
        c5d_op(C5dOpV1::BalanceV1 {
            part: BalancePartV1::Creatures,
        }),
    ])
}

/// Council's Judgment: "Will of the council: Starting with you, each player
/// votes for a nonland permanent you don't control. Exile each permanent
/// with the most votes or tied for most votes."
pub fn councils_judgment() -> EffectOp {
    c5d_op(C5dOpV1::VoteExileV1)
}

/// Fractured Identity: "Exile target nonland permanent. Each player other
/// than its controller creates a token that's a copy of it."
pub fn fractured_identity() -> EffectOp {
    c5d_op(C5dOpV1::FracturedIdentityV1)
}

/// Palantir of Orthanc's end step trigger: "put an influence counter on
/// Palantir of Orthanc and scry 2. Then target opponent may have you draw a
/// card. If that player doesn't, you mill X cards, where X is the number of
/// influence counters on Palantir of Orthanc, and that player loses life
/// equal to the total mana value of those cards." Option 0 of the
/// opponent's choice lets the draw happen.
fn palantir_end_step() -> EffectOp {
    EffectOp::Sequence(vec![
        c5d_op(C5dOpV1::PalantirCounterV1),
        EffectOp::Scry {
            player: PlayerRef::Controller,
            count: 2,
        },
        EffectOp::Choice {
            controller: PlayerRef::Opponent,
            options: vec![
                EffectOp::DrawCards {
                    player: PlayerRef::Controller,
                    count: 1,
                },
                c5d_op(C5dOpV1::PalantirMillV1),
            ],
        },
    ])
}

/// Memory Jar: "Each player exiles all cards from their hand face down and
/// draws seven cards. At the beginning of the next end step, each player
/// discards their hand and returns to their hand each card they exiled this
/// way." Both hands leave first, then the draws (the controller's, then the
/// opponent's, a draw that wins ending the game at once).
pub fn memory_jar() -> EffectOp {
    EffectOp::Sequence(vec![
        c5d_op(C5dOpV1::JarExileV1),
        EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 7,
        },
        crate::eternal_cube_c1a_v1::stop_if_won(),
        EffectOp::DrawCards {
            player: PlayerRef::Opponent,
            count: 7,
        },
        crate::eternal_cube_c1a_v1::stop_if_won(),
    ])
}

// ---- Helpers ------------------------------------------------------------------------

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

fn push(continuation: &mut EffectContinuation, op: EffectOp, path: &[u16]) {
    continuation.frames.push(EffectFrame::Program {
        op,
        path: path.to_vec(),
    });
}

/// Offers `player` the choice of `kind` among `bindings`, `min` to `max`
/// of them.
fn offer(
    continuation: &mut EffectContinuation,
    kind: C5dChoiceV1,
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
            kind: E3ChoiceKindV1::C5dV1(kind),
            player,
            candidates: bindings
                .iter()
                .map(|binding| Target::Object(binding.object))
                .collect(),
            bindings,
            min,
            max,
            ordered: false,
            path,
            remaining,
        },
    );
}

/// The ability source of the resolving ability: the contract it was created
/// with, or one captured from its source object.
fn ability_source(ctx: &ExecCtx, state: &GameState) -> AbilitySourceContractV4 {
    ctx.ability_source_contract
        .unwrap_or_else(|| AbilitySourceContractV4::capture(state, ctx.source))
}

/// Target `index`, if it is an object still legal for the resolving spell or
/// ability under `spec` (608.2b).
fn legal_target(
    ctx: &ExecCtx,
    state: &GameState,
    spec: TargetSpec,
    index: usize,
) -> Option<ObjectId> {
    let Target::Object(object) = ctx.targets.get(index).copied()? else {
        return None;
    };
    (ctx.target_incarnation_matches(index, state)
        && state.objects.get(object).zone == Zone::Battlefield
        && crate::engine::effect_target_is_legal(
            state,
            ctx.source,
            ctx.controller,
            spec,
            &ctx.targets[index..=index],
            0,
        ))
    .then_some(object)
}

/// The permanents `player` controls that satisfy `keep`, in battlefield order.
fn controlled_matching(
    state: &GameState,
    player: PlayerId,
    keep: impl Fn(ObjectId) -> bool,
) -> Vec<ObjectId> {
    [PlayerId::P0, PlayerId::P1]
        .into_iter()
        .flat_map(|seat| state.players[seat.index()].battlefield.iter().copied())
        .filter(|&object| {
            let live = state.objects.get(object);
            live.zone == Zone::Battlefield && live.controller == player && keep(object)
        })
        .collect()
}

// ---- Proft's Eidetic Memory -----------------------------------------------------------

/// Whether `player` controls a Proft's Eidetic Memory whose printed text
/// applies: "You have no maximum hand size." Called from
/// `eternal_cards_v1::no_maximum_hand_size`, where the cleanup discard is
/// staged.
pub(crate) fn removes_hand_limit(state: &GameState, player: PlayerId) -> bool {
    cfg!(feature = "eternal-fixtures")
        && state.players[player.index()]
            .battlefield
            .iter()
            .copied()
            .any(|id| {
                let object = state.objects.get(id);
                object.zone == Zone::Battlefield
                    && object.controller == player
                    && object.v4.face_index == 0
                    && CARD_DEFS
                        .get(usize::from(object.card_def))
                        .is_some_and(|def| def.is_executable() && def.name == PROFTS_EIDETIC_MEMORY)
                    && crate::continuous_characteristics_v1::printed_abilities_active(state, id)
            })
}

/// The creature target 0 names when it is still the controller's creature on
/// the battlefield.
fn controlled_creature_target(ctx: &ExecCtx, state: &GameState) -> Option<ObjectId> {
    let object = legal_target(ctx, state, TargetSpec::ControlledCreature, 0)?;
    (state.objects.get(object).controller == ctx.controller
        && crate::engine::object_has_type(state, object, CardType::Creature))
    .then_some(object)
}

/// "Put X +1/+1 counters on target creature you control, where X is the
/// number of cards you've drawn this turn minus one." The intervening "if"
/// is read again as the trigger resolves (603.4).
fn run_proft_counters(ctx: &ExecCtx, state: &mut GameState) {
    let drawn = state.players[ctx.controller.index()].draws_this_turn;
    if drawn < 2 {
        return;
    }
    let Some(creature) = controlled_creature_target(ctx, state) else {
        return;
    };
    let amount = i32::try_from(drawn - 1).unwrap_or(i32::MAX);
    if event::add_plus_one_counters(state, creature, ctx.controller, amount).is_err() {
        halt(state, ctx.source);
    }
}

pub(crate) fn trigger_matches(
    condition: C5dTriggerV1,
    events: &[CommittedEvent],
    index: usize,
    source: ObjectId,
    state: &GameState,
) -> bool {
    let controller = state.objects.get(source).controller;
    match condition {
        C5dTriggerV1::YourCombatIfDrewMoreThanOneV1 => {
            matches!(
                events[index],
                CommittedEvent::BeginningOfCombat { active_player } if active_player == controller
            ) && state.players[controller.index()].draws_this_turn > 1
        }
    }
}

// ---- Balance ------------------------------------------------------------------------

/// The cards of `part` that `player` has: lands and creatures they control
/// (as they are now, so an animated land is both), or cards in their hand.
fn balance_cards(
    state: &GameState,
    player: PlayerId,
    part: BalancePartV1,
) -> Vec<EffectObjectBinding> {
    match part {
        BalancePartV1::Hand => state.players[player.index()]
            .hand
            .iter()
            .copied()
            .filter(|&card| state.objects.get(card).zone == Zone::Hand)
            .map(|card| bind(state, card))
            .collect(),
        BalancePartV1::Lands => controlled_matching(state, player, |object| {
            crate::engine::object_has_type(state, object, CardType::Land)
        })
        .into_iter()
        .map(|object| bind(state, object))
        .collect(),
        BalancePartV1::Creatures => controlled_matching(state, player, |object| {
            crate::engine::object_has_type(state, object, CardType::Creature)
        })
        .into_iter()
        .map(|object| bind(state, object))
        .collect(),
    }
}

/// Asks each player in `players` with more than `keep` cards of `part` which
/// `keep` of them stay (active player first), then takes everything else
/// away at once. `Ok(true)` when a choice was staged.
fn stage_balance(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    part: BalancePartV1,
    keep: usize,
    mut remove: Vec<EffectObjectBinding>,
    players: &[PlayerId],
    path: Vec<u16>,
) -> Result<bool, String> {
    for (index, &player) in players.iter().enumerate() {
        let cards = balance_cards(state, player, part);
        if cards.len() <= keep {
            continue;
        }
        if keep == 0 {
            remove.extend(cards);
            continue;
        }
        let keep = u16::try_from(keep).unwrap_or(u16::MAX);
        offer(
            continuation,
            C5dChoiceV1::BalanceKeepV1 {
                part,
                keep,
                remove,
                later: players[index + 1..].to_vec(),
            },
            player,
            cards,
            keep,
            keep,
            path,
        );
        return Ok(true);
    }
    apply_balance(state, part, &remove);
    Ok(false)
}

/// Discards or sacrifices `remove` (those still where they were counted),
/// both players' cards in one batch.
fn apply_balance(state: &mut GameState, part: BalancePartV1, remove: &[EffectObjectBinding]) {
    let live = remove
        .iter()
        .copied()
        .filter(|binding| binding_live(state, *binding))
        .map(|binding| binding.object)
        .collect::<Vec<_>>();
    if live.is_empty() {
        return;
    }
    match part {
        BalancePartV1::Hand => crate::engine::commit_discarded_cards(state, &live),
        BalancePartV1::Lands | BalancePartV1::Creatures => {
            for &object in &live {
                event::log_sacrifice(state, object);
            }
            event::propose_and_commit_batch(
                state,
                live.iter()
                    .map(|&object| ProposedEvent::zone_change(object, Zone::Graveyard))
                    .collect(),
            );
        }
    }
}

// ---- Council's Judgment -------------------------------------------------------------

/// The nonland permanents `caster` does not control, each player's in
/// battlefield order.
fn vote_candidates(state: &GameState, caster: PlayerId) -> Vec<EffectObjectBinding> {
    [PlayerId::P0, PlayerId::P1]
        .into_iter()
        .flat_map(|seat| state.players[seat.index()].battlefield.iter().copied())
        .filter(|&object| {
            let live = state.objects.get(object);
            live.zone == Zone::Battlefield
                && live.controller != caster
                && !crate::engine::object_has_type(state, object, CardType::Land)
        })
        .map(|object| bind(state, object))
        .collect()
}

/// Asks each voter in `voters` for a vote, then tallies. A player with no
/// permanent to vote for casts no vote, and nobody does when there is none.
/// `Ok(true)` when a vote was staged.
fn stage_vote(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    votes: Vec<EffectObjectBinding>,
    voters: &[PlayerId],
    caster: PlayerId,
    path: Vec<u16>,
) -> Result<bool, String> {
    let candidates = vote_candidates(state, caster);
    if let Some((&voter, later)) = voters.split_first() {
        if !candidates.is_empty() {
            offer(
                continuation,
                C5dChoiceV1::VoteV1 {
                    votes,
                    later: later.to_vec(),
                },
                voter,
                candidates,
                1,
                1,
                path,
            );
            return Ok(true);
        }
    }
    tally_and_exile(state, &votes);
    Ok(false)
}

/// Exiles every permanent with the most votes (or tied for the most), at
/// once.
fn tally_and_exile(state: &mut GameState, votes: &[EffectObjectBinding]) {
    let mut tally: Vec<(EffectObjectBinding, usize)> = Vec::new();
    for vote in votes {
        match tally.iter_mut().find(|entry| entry.0 == *vote) {
            Some(entry) => entry.1 += 1,
            None => tally.push((*vote, 1)),
        }
    }
    let most = tally.iter().map(|entry| entry.1).max().unwrap_or(0);
    let winners = tally
        .iter()
        .filter(|entry| entry.1 == most && binding_live(state, entry.0))
        .map(|entry| entry.0.object)
        .collect::<Vec<_>>();
    if !winners.is_empty() {
        event::propose_and_commit_batch(
            state,
            winners
                .into_iter()
                .map(|object| ProposedEvent::zone_change(object, Zone::Exile))
                .collect(),
        );
    }
}

// ---- Palantir of Orthanc ------------------------------------------------------------

fn influence_counters(state: &GameState, source: AbilitySourceContractV4) -> u8 {
    read_state(state)
        .and_then(|ledger| {
            ledger.influence.iter().find(|(link, _)| {
                link.object == source.source && link.zone_change_count == source.zone_change_count
            })
        })
        .map_or(0, |(_, count)| *count)
}

/// "Put an influence counter on Palantir of Orthanc." A Palantir that has
/// left the battlefield by now takes none.
fn run_palantir_counter(ctx: &ExecCtx, state: &mut GameState) {
    let source = ability_source(ctx, state);
    let on_battlefield = state.objects.try_get(source.source).is_some_and(|live| {
        live.zone == Zone::Battlefield && live.zone_change_count == source.zone_change_count
    });
    if !on_battlefield {
        return;
    }
    // Counter placement replacements (Doubling Season's family) apply.
    let placed = if crate::doubling_season_v48::requires_successor(state) {
        match crate::doubling_season_v48::counter_amount(
            state,
            source.source,
            ctx.controller,
            crate::doubling_season_v48::CounterOriginV48::Effect,
            1,
        )
        .ok()
        .and_then(|count| u8::try_from(count).ok())
        {
            Some(count) => count,
            None => {
                halt(state, ctx.source);
                return;
            }
        }
    } else {
        1
    };
    let link = ObjectLinkV4 {
        object: source.source,
        zone_change_count: source.zone_change_count,
    };
    let ledger = &mut c5d_state(state).influence;
    match ledger.iter_mut().find(|(host, _)| *host == link) {
        Some((_, count)) => *count = count.saturating_add(placed),
        None => ledger.push((link, placed)),
    }
}

/// The declined branch: mill X, then the opponent loses life equal to the
/// milled cards' total mana value. The top X cards are read now; nothing can
/// change them before the mill.
fn drive_palantir_mill(state: &mut GameState, continuation: &mut EffectContinuation, path: &[u16]) {
    let source = ability_source(&continuation.ctx, state);
    let controller = continuation.ctx.controller;
    let x = influence_counters(state, source);
    if x == 0 {
        return;
    }
    let total: i32 = state.players[controller.index()]
        .library
        .iter()
        .take(usize::from(x))
        .map(|&card| i32::from(CARD_DEFS[usize::from(state.objects.get(card).card_def)].mana_value))
        .sum();
    // Frames run last pushed first: the mill, then the life loss.
    if total > 0 {
        push(
            continuation,
            EffectOp::LoseLife {
                player: PlayerRef::Opponent,
                amount: total,
            },
            path,
        );
    }
    push(
        continuation,
        EffectOp::MillCards {
            player: PlayerRef::Controller,
            count: x,
        },
        path,
    );
}

/// Drops influence ledger entries whose Palantir is gone and whose triggers
/// are no longer waiting (a departed Palantir's last-known counters are
/// still read while its trigger is on the stack or in line for it).
fn prune_influence(state: &mut GameState) {
    let Some(ledger) = read_state(state).filter(|ledger| !ledger.influence.is_empty()) else {
        return;
    };
    let keep = ledger
        .influence
        .iter()
        .copied()
        .filter(|(link, _)| {
            link_live_in(state, *link, Zone::Battlefield)
                || state.stack.iter().any(|item| {
                    item.v4.ability_source_contract.is_some_and(|source| {
                        source.source == link.object
                            && source.zone_change_count == link.zone_change_count
                    })
                })
                || state.engine.pending_triggers.iter().any(|pending| {
                    pending.source_contract.is_some_and(|source| {
                        source.source == link.object
                            && source.zone_change_count == link.zone_change_count
                    })
                })
        })
        .collect::<Vec<_>>();
    if keep.len() != ledger.influence.len() {
        c5d_state(state).influence = keep;
    }
}

// ---- Fractured Identity -------------------------------------------------------------

/// The copiable values of `object` (a permanent on the battlefield), as
/// Extravagant Replication reads them. A face-down permanent copies as the
/// anonymous creature blueprint.
fn copy_template(state: &GameState, object: ObjectId) -> Result<CopyTemplateV1, String> {
    let live = state.objects.get(object);
    if live.v4.face_down_v1.is_some() {
        let anonymous =
            crate::card_def::card_id_by_name(crate::self_reflection_v1::ANONYMOUS_BLUEPRINT)
                .filter(|id| crate::self_reflection_v1::anonymous_blueprint_is_exact(*id))
                .ok_or("anonymous copy blueprint absent")?;
        return Ok(CopyTemplateV1 {
            card_def: anonymous,
            face_index: 0,
            copied_end_step_sacrifices_v1: 0,
            copied_nightmare_v1: false,
        });
    }
    let def = &CARD_DEFS[usize::from(live.card_def)];
    if live.v4.face_index > 1 || live.v4.face_index == 1 && def.transform_face.is_none() {
        return Err("a copied permanent shows an unsupported face".into());
    }
    Ok(CopyTemplateV1 {
        card_def: live.card_def,
        face_index: live.v4.face_index,
        copied_end_step_sacrifices_v1: event::copiable_token_abilities_v1(state, object)?,
        copied_nightmare_v1: crate::effect::harvester_v1::copiable_nightmare(state, object)?,
    })
}

/// The token copy proposal for `controller`.
fn copy_proposal(template: &CopyTemplateV1, controller: PlayerId) -> ProposedEvent {
    let mut proposal = ProposedEvent::create_token(template.card_def, controller);
    if let ProposedEvent::CreateToken(token) = &mut proposal {
        token.entry_face_index = (template.face_index != 0).then_some(template.face_index);
        token.initialization_v48 = Some(crate::doubling_season_v48::TokenEntryV48 {
            copied: true,
            copied_end_step_sacrifices_v1: template.copied_end_step_sacrifices_v1,
            copied_nightmare_v1: template.copied_nightmare_v1,
            ..Default::default()
        });
    }
    proposal
}

/// What an Aura copy may enchant: the battlefield permanents its enchant
/// restriction allows and protection does not rule out (Extravagant
/// Replication's host list).
fn aura_hosts(state: &GameState, template: &CopyTemplateV1) -> Vec<EffectObjectBinding> {
    let def = &CARD_DEFS[usize::from(template.card_def)];
    let Some(attachment) = def.attachment else {
        return Vec::new();
    };
    let colors = if template.face_index == 1 {
        def.transform_face
            .as_ref()
            .map_or(def.colors, |face| face.colors)
    } else {
        def.colors
    };
    let monocolored = colors.len() == 1;
    state
        .objects
        .iter()
        .filter_map(|(id, live)| {
            (live.zone == Zone::Battlefield
                && attachment.host_matches(state, id)
                && !crate::engine::has_effective_keyword(
                    state,
                    id,
                    Keywords::PROTECTION_FROM_EVERYTHING,
                )
                && !(monocolored
                    && crate::engine::has_effective_keyword(
                        state,
                        id,
                        Keywords::PROTECTION_FROM_MONOCOLORED,
                    )))
            .then_some(bind(state, id))
        })
        .collect()
}

/// Creates the copy token(s) for `player`, asking what an Aura copy enchants
/// when that is a real choice. `Ok(true)` when a choice was staged.
fn stage_copy(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    template: &CopyTemplateV1,
    player: PlayerId,
    selected: Vec<EffectObjectBinding>,
    path: Vec<u16>,
) -> Result<bool, String> {
    let def = &CARD_DEFS[usize::from(template.card_def)];
    if def.attachment.is_none() {
        event::propose_and_commit_batch(state, vec![copy_proposal(template, player)]);
        return Ok(false);
    }
    let count = crate::doubling_season_v48::ReplacementInputV48::capture(state, player)
        .replaced_token_count(1)?;
    let hosts = aura_hosts(state, template);
    if hosts.is_empty() {
        // An Aura with nothing to enchant does not enter.
        return Ok(false);
    }
    if hosts.len() == 1 || selected.len() >= count {
        let chosen = if selected.len() >= count {
            selected
        } else {
            vec![hosts[0]; count]
        };
        event::batch_with_copied_aura_hosts_v1(
            state,
            copy_proposal(template, player),
            chosen
                .into_iter()
                .map(|host| ObjectLinkV4 {
                    object: host.object,
                    zone_change_count: host.expected_zone_change_count,
                })
                .collect(),
        );
        return Ok(false);
    }
    offer(
        continuation,
        C5dChoiceV1::AuraHostV1 {
            template: template.clone(),
            count: u16::try_from(count).unwrap_or(u16::MAX),
            selected,
        },
        player,
        hosts,
        1,
        1,
        path,
    );
    Ok(true)
}

fn drive_fractured_identity(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    path: Vec<u16>,
) -> Result<bool, String> {
    let ctx = continuation.ctx.clone();
    let Some(target) = legal_target(&ctx, state, TargetSpec::NonlandPermanent, 0) else {
        return Ok(false);
    };
    let victim_controller = state.objects.get(target).controller;
    // The copiable values as the permanent last existed on the battlefield.
    let template = copy_template(state, target)?;
    event::propose_and_commit(state, ProposedEvent::zone_change(target, Zone::Exile));
    if state.engine.halted.is_some() {
        return Ok(false);
    }
    // "Each player other than its controller": the one opponent.
    stage_copy(
        state,
        continuation,
        &template,
        victim_controller.opponent(),
        Vec::new(),
        path,
    )
}

// ---- Memory Jar ---------------------------------------------------------------------

/// Exiles every card of `player`'s hand face down, one at a time (each
/// departure is hidden from the other player as foretell's is: the move is
/// committed with the concealed-exile mark, and the event is marked visible
/// to the owner only). The exile incarnations are added to `exiled`.
fn exile_hand_face_down(state: &mut GameState, player: PlayerId, exiled: &mut Vec<ObjectLinkV4>) {
    let hand = state.players[player.index()].hand.clone();
    for card in hand {
        if state.objects.get(card).zone != Zone::Hand {
            continue;
        }
        let history_start = state.engine.event_history.len();
        state.reveal_meaning_v3.set_concealed_exile(Some(card));
        event::propose_and_commit(state, ProposedEvent::zone_change(card, Zone::Exile));
        state.reveal_meaning_v3.set_concealed_exile(None);
        if state.engine.halted.is_some() || state.objects.get(card).zone != Zone::Exile {
            continue;
        }
        state.objects.get_mut(card).v4.face_down_v1 = Some(crate::state::FaceDownV1 {
            disguised: false,
            lookers: 1 << player.index(),
            hidden_by: None,
            hidden_by_source: None,
        });
        for index in history_start..state.engine.event_history.len() {
            if matches!(state.engine.event_history[index], CommittedEvent::ZoneChange {
                object, from: Zone::Hand, to: Zone::Exile, ..
            } if object == card)
            {
                state
                    .observation_journal
                    .record_visibility_v2(index, card, 1 << player.index());
            }
        }
        exiled.push(live_link(state, card));
    }
    // The other player cannot tell which cards left the hand.
    state.forget_hand_after_face_down_departure_v1(player);
}

fn run_jar_exile(ctx: &ExecCtx, state: &mut GameState) {
    let source = ability_source(ctx, state);
    let mut exiled = Vec::new();
    for player in [ctx.controller, ctx.controller.opponent()] {
        exile_hand_face_down(state, player, &mut exiled);
        if state.engine.halted.is_some() {
            return;
        }
    }
    let created_in_end_step = (state.step == Step::End).then(|| state.turn_key_v1());
    let ledger = c5d_state(state);
    let serial = ledger.next_jar_serial;
    ledger.next_jar_serial = serial.saturating_add(1);
    ledger.jars.push(JarRecordV1 {
        serial,
        source,
        cards: exiled,
        created_in_end_step,
        queued: false,
    });
}

/// "At the beginning of the next end step, each player discards their hand
/// and returns to their hand each card they exiled this way." A card that
/// left exile in the meantime stays where it went.
fn run_jar_return(ctx: &ExecCtx, state: &mut GameState, serial: u32) {
    let Some(position) = read_state(state).and_then(|ledger| {
        ledger
            .jars
            .iter()
            .position(|record| record.serial == serial)
    }) else {
        return;
    };
    let record = c5d_state(state).jars.remove(position);
    // Each player discards their hand, all at once.
    let mut hands = Vec::new();
    for player in [state.active_player, state.active_player.opponent()] {
        hands.extend(
            state.players[player.index()]
                .hand
                .iter()
                .copied()
                .filter(|&card| state.objects.get(card).zone == Zone::Hand),
        );
    }
    crate::engine::commit_discarded_cards(state, &hands);
    if state.engine.halted.is_some() {
        halt(state, ctx.source);
        return;
    }
    let returning = record
        .cards
        .iter()
        .copied()
        .filter(|&card| link_live_in(state, card, Zone::Exile))
        .map(|card| card.object)
        .collect::<Vec<_>>();
    if returning.is_empty() {
        return;
    }
    let history_start = state.engine.event_history.len();
    event::propose_and_commit_batch(
        state,
        returning
            .iter()
            .map(|&card| ProposedEvent::zone_change(card, Zone::Hand))
            .collect(),
    );
    // The cards go back to their owners' hands unseen by the other player.
    for index in history_start..state.engine.event_history.len() {
        let CommittedEvent::ZoneChange {
            object,
            from: Zone::Exile,
            to: Zone::Hand,
            ..
        } = state.engine.event_history[index]
        else {
            continue;
        };
        if returning.contains(&object) {
            let owner = state.objects.get(object).owner;
            state
                .observation_journal
                .record_visibility_v2(index, object, 1 << owner.index());
        }
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

const fn c5d_trigger(condition: C5dTriggerV1, effect: fn() -> EffectOp) -> TriggeredAbilityDef {
    trigger(
        TriggerCondition::StandardV1(StandardTriggerV1::EternalV1(EternalTriggerV1::C5dV1(
            condition,
        ))),
        effect,
    )
}

const PROFT_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(TriggerCondition::Etb, proft_draw),
    c5d_trigger(C5dTriggerV1::YourCombatIfDrewMoreThanOneV1, proft_counters),
];
const PALANTIR_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(
    TriggerCondition::BeginningControllerEndStep,
    palantir_end_step,
)];

/// Triggered abilities of this module's cards, the tail of the trigger
/// tables.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        PROFTS_EIDETIC_MEMORY => &PROFT_TRIGGERS,
        PALANTIR_OF_ORTHANC => &PALANTIR_TRIGGERS,
        _ => &[],
    }
}

/// Target specs of this module's printed and queued triggered abilities.
pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    match name {
        PROFTS_EIDETIC_MEMORY if *effect == proft_counters() => {
            Some(TargetSpec::ControlledCreature)
        }
        PROFTS_EIDETIC_MEMORY if *effect == proft_draw() => Some(TargetSpec::None),
        PALANTIR_OF_ORTHANC if *effect == palantir_end_step() => Some(TargetSpec::TargetOpponent),
        _ => delayed_effect_matches(name, effect).then_some(TargetSpec::None),
    }
}

/// Queued triggers owned by this module's cards: Memory Jar's end step
/// action, which outlives the sacrificed Jar.
pub(crate) fn delayed_effect_matches(name: &str, effect: &EffectOp) -> bool {
    name == MEMORY_JAR
        && matches!(
            effect,
            EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::C5dV1(
                C5dOpV1::JarReturnV1 { .. }
            )))
        )
}

/// The delayed triggers this collection sets off: Memory Jar's end step
/// action at the beginning of the next end step.
pub(crate) fn delayed_triggers(
    state: &mut GameState,
    events: &[CommittedEvent],
) -> Vec<PendingTrigger> {
    if !cfg!(feature = "eternal-fixtures") {
        return Vec::new();
    }
    prune_influence(state);
    if !events
        .iter()
        .any(|event| matches!(event, CommittedEvent::BeginningEndStep { .. }))
    {
        return Vec::new();
    }
    let turn = state.turn_key_v1();
    let Some(ledger) = read_state(state).filter(|ledger| !ledger.jars.is_empty()) else {
        return Vec::new();
    };
    let due = ledger
        .jars
        .iter()
        .filter(|record| !record.queued && record.created_in_end_step != Some(turn))
        .map(|record| (record.serial, record.source))
        .collect::<Vec<_>>();
    let mut triggers = Vec::new();
    for (serial, source) in due {
        if let Some(record) = c5d_state(state)
            .jars
            .iter_mut()
            .find(|record| record.serial == serial)
        {
            record.queued = true;
        }
        triggers.push(crate::eternal_cards_v1::delayed_trigger(
            source,
            source.controller,
            c5d_op(C5dOpV1::JarReturnV1 { serial }),
        ));
    }
    triggers
}

// ---- Execution ----------------------------------------------------------------------

/// Runs one of this module's leaves in synchronous execution.
pub(crate) fn execute(op: &C5dOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        C5dOpV1::ProftCountersV1 => run_proft_counters(ctx, state),
        C5dOpV1::PalantirCounterV1 => run_palantir_counter(ctx, state),
        C5dOpV1::JarExileV1 => run_jar_exile(ctx, state),
        C5dOpV1::JarReturnV1 { serial } => run_jar_return(ctx, state, *serial),
        // Choices run in `drive`.
        C5dOpV1::VoteExileV1
        | C5dOpV1::BalanceV1 { .. }
        | C5dOpV1::PalantirMillV1
        | C5dOpV1::FracturedIdentityV1 => halt(state, ctx.source),
    }
}

/// Runs one of this module's leaves in the resumable interpreter.
pub(crate) fn drive(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    op: &C5dOpV1,
    path: Vec<u16>,
) -> Result<bool, String> {
    match op {
        C5dOpV1::VoteExileV1 => {
            let caster = continuation.ctx.controller;
            stage_vote(
                state,
                continuation,
                Vec::new(),
                &[caster, caster.opponent()],
                caster,
                path,
            )
        }
        C5dOpV1::BalanceV1 { part } => {
            let active = state.active_player;
            let players = [active, active.opponent()];
            let keep = players
                .iter()
                .map(|&player| balance_cards(state, player, *part).len())
                .min()
                .unwrap_or(0);
            stage_balance(state, continuation, *part, keep, Vec::new(), &players, path)
        }
        C5dOpV1::PalantirMillV1 => {
            drive_palantir_mill(state, continuation, &path);
            Ok(false)
        }
        C5dOpV1::FracturedIdentityV1 => drive_fractured_identity(state, continuation, path),
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
    kind: &C5dChoiceV1,
    bindings: Vec<EffectObjectBinding>,
) -> Result<bool, String> {
    match kind {
        C5dChoiceV1::BalanceKeepV1 {
            part,
            keep,
            remove,
            later,
        } => {
            // What the chooser did not keep goes with the earlier choosers'.
            let mut remove = remove.clone();
            remove.extend(
                balance_cards(state, choice.player, *part)
                    .into_iter()
                    .filter(|card| !bindings.contains(card)),
            );
            stage_balance(
                state,
                continuation,
                *part,
                usize::from(*keep),
                remove,
                later,
                choice.path.clone(),
            )
        }
        C5dChoiceV1::VoteV1 { votes, later } => {
            let mut votes = votes.clone();
            votes.extend(bindings.first().copied());
            let caster = continuation.ctx.controller;
            stage_vote(
                state,
                continuation,
                votes,
                later,
                caster,
                choice.path.clone(),
            )
        }
        C5dChoiceV1::AuraHostV1 {
            template, selected, ..
        } => {
            let mut selected = selected.clone();
            selected.extend(bindings.first().copied());
            stage_copy(
                state,
                continuation,
                template,
                choice.player,
                selected,
                choice.path.clone(),
            )
        }
    }
}
