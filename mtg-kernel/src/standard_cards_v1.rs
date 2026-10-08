//! Card programs for the MageZero Standard catalog's non-creature permanent,
//! planeswalker and transforming-legend families (inventory families E and F).
//!
//! Every program here is selected by printed card name from the registry, so
//! catalogs without these names never reach it. Behavior was read from each
//! card's XMage source (`magefree/mage` master).

use crate::card_def::CardType;
use crate::effect::{EffectObjectBinding, EffectOp, ExecCtx, PlayerRef};
use crate::event::{self, ProposedEvent};
use crate::ids::PlayerId;
use crate::state::{GameState, Zone};
use crate::trigger::{TriggerCondition, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};

/// Effect leaves owned by this module. Appended to `EffectOp` as one variant
/// so earlier serialized programs keep their shapes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StandardOpV1 {
    /// Put loyalty counters on the resolving ability's own source, if it is
    /// still the same battlefield planeswalker incarnation.
    AddLoyaltyToSource { amount: u8 },
    /// `player` chooses one permanent they control matching `filter` (a
    /// public choice, not targeted) and `action` applies to it. With one
    /// candidate the choice is automatic; with none nothing happens.
    PlayerChoosesControlledPermanent {
        player: PlayerRef,
        filter: StandardPermanentFilterV1,
        action: StandardChosenActionV1,
    },
    /// The answered half of `PlayerChoosesControlledPermanent`: interpreter
    /// owned, never part of a generated program.
    ApplyChosenPermanent {
        chosen: EffectObjectBinding,
        action: StandardChosenActionV1,
    },
    /// Each nonland permanent `player` controls goes into its owner's
    /// library, then every affected library is shuffled.
    ShuffleNonlandPermanentsIntoLibraries { player: PlayerRef },
}

impl StandardOpV1 {
    /// Whether this leaf can yield a player decision, so its program must use
    /// the resumable interpreter.
    pub(crate) fn contains_player_choice(&self) -> bool {
        matches!(self, Self::PlayerChoosesControlledPermanent { .. })
    }

    /// Interpreter-owned leaves that a generated program may never contain.
    pub(crate) fn is_bound_continuation(&self) -> bool {
        matches!(self, Self::ApplyChosenPermanent { .. })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StandardPermanentFilterV1 {
    Any,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StandardChosenActionV1 {
    ReturnToOwnersHand,
}

/// The exact battlefield permanents `player` controls that match `filter`,
/// in battlefield order.
pub(crate) fn controlled_permanent_candidates(
    state: &GameState,
    player: PlayerId,
    filter: StandardPermanentFilterV1,
) -> Vec<EffectObjectBinding> {
    state.players[player.index()]
        .battlefield
        .iter()
        .copied()
        .filter(|&object| {
            let live = state.objects.get(object);
            live.zone == Zone::Battlefield
                && live.controller == player
                && match filter {
                    StandardPermanentFilterV1::Any => true,
                }
        })
        .map(|object| EffectObjectBinding {
            object,
            expected_zone: Zone::Battlefield,
            expected_zone_change_count: state.objects.get(object).zone_change_count,
        })
        .collect()
}

/// Applies a chosen-permanent action to its exact bound incarnation.
pub(crate) fn apply_chosen_permanent(
    state: &mut GameState,
    chosen: EffectObjectBinding,
    action: StandardChosenActionV1,
) {
    let live = state.objects.get(chosen.object);
    if live.zone != chosen.expected_zone
        || live.zone_change_count != chosen.expected_zone_change_count
    {
        return;
    }
    match action {
        StandardChosenActionV1::ReturnToOwnersHand => event::propose_and_commit(
            state,
            ProposedEvent::zone_change(chosen.object, Zone::Hand),
        ),
    }
}

/// True iff the resolving ability's source is still the incarnation that
/// created it, on the battlefield.
fn source_incarnation_live(ctx: &ExecCtx, state: &GameState) -> bool {
    let Some(contract) = ctx.ability_source_contract else {
        return false;
    };
    state.objects.try_get(ctx.source).is_some_and(|live| {
        live.zone == Zone::Battlefield
            && live.card_def == contract.card_def
            && live.zone_change_count == contract.zone_change_count
    })
}

pub(crate) fn execute(op: &StandardOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        StandardOpV1::AddLoyaltyToSource { amount } => {
            if source_incarnation_live(ctx, state) {
                crate::planeswalker_v1::change_loyalty(state, ctx.source, i32::from(*amount));
            }
        }
        StandardOpV1::ShuffleNonlandPermanentsIntoLibraries { player } => {
            let player = ctx.resolve_player(*player, state);
            let moving = state.players[player.index()]
                .battlefield
                .iter()
                .copied()
                .filter(|&object| !crate::engine::object_has_type(state, object, CardType::Land))
                .collect::<Vec<_>>();
            if moving.is_empty() {
                return;
            }
            let mut owners = moving
                .iter()
                .map(|&object| state.objects.get(object).owner)
                .collect::<Vec<_>>();
            owners.sort_unstable();
            owners.dedup();
            event::propose_and_commit_batch(
                state,
                moving
                    .into_iter()
                    .map(|object| ProposedEvent::zone_change(object, Zone::Library))
                    .collect(),
            );
            for owner in owners {
                if state.shuffle_library(owner).is_err() {
                    state.engine.halted = Some((
                        crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                        ctx.source,
                    ));
                    return;
                }
            }
        }
        StandardOpV1::PlayerChoosesControlledPermanent { .. }
        | StandardOpV1::ApplyChosenPermanent { .. } => {
            panic!("choice-bearing effects must use the resumable interpreter")
        }
    }
}

const fn trigger(condition: TriggerCondition, effect: fn() -> EffectOp) -> TriggeredAbilityDef {
    TriggeredAbilityDef {
        condition,
        home_zone: Zone::Battlefield,
        intervening_if_kicked: false,
        intervening_if_controls_another_source_card: false,
        effect,
    }
}

// ---- Teferi, Temporal Pilgrim -------------------------------------------

fn teferi_draw_loyalty() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::AddLoyaltyToSource { amount: 1 })
}

const TEFERI_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::ControllerDraws, teferi_draw_loyalty)];

/// "-2: Create a 2/2 blue Spirit creature token with vigilance and 'Whenever
/// you draw a card, put a +1/+1 counter on this creature.'"
pub fn teferi_create_spirit() -> EffectOp {
    EffectOp::CreateToken {
        token_def: crate::card_def::card_id_by_name("Teferi Spirit Token")
            .expect("Teferi Spirit Token in CARD_DEFS"),
        controller: PlayerRef::Controller,
    }
}

/// "-12: Target opponent chooses a permanent they control and returns it to
/// its owner's hand. Then they shuffle each nonland permanent they control
/// into its owner's library."
pub fn teferi_ultimate() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::StandardV1(StandardOpV1::PlayerChoosesControlledPermanent {
            player: PlayerRef::Target(0),
            filter: StandardPermanentFilterV1::Any,
            action: StandardChosenActionV1::ReturnToOwnersHand,
        }),
        EffectOp::StandardV1(StandardOpV1::ShuffleNonlandPermanentsIntoLibraries {
            player: PlayerRef::Target(0),
        }),
    ])
}

fn spirit_draw_counter() -> EffectOp {
    EffectOp::BindPlusOnePlusOneCounterToTriggerSource
}

const TEFERI_SPIRIT_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::ControllerDraws, spirit_draw_counter)];

/// Triggered abilities of this module's cards, by registry name.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        "Teferi, Temporal Pilgrim" => &TEFERI_TRIGGERS,
        "Teferi Spirit Token" => &TEFERI_SPIRIT_TRIGGERS,
        _ => &[],
    }
}

/// The transforming-card face each triggered ability is printed on. Abilities
/// of single-faced cards report face 0.
pub(crate) fn trigger_face(_name: &str, _ability_index: usize) -> u8 {
    0
}
