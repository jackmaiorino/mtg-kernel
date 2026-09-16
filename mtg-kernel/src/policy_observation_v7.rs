//! Fresh-lineage (V4 contract / V7 observation-schema) successor to
//! [`crate::policy_observation_v6`]. Additive only: `policy_observation_v6.rs`'s
//! `HistoricalSourceContextV6` and every V6 type stay byte-identical and are
//! never imported here except by value-copy through
//! [`policy_observation_extensions_v7`]. See
//! `data/flat_policy_v4/README.md` for the Python-side contract this mirrors
//! (`HISTORICAL_SOURCE_CONTEXT_V7` in `python/mtg_kernel_rl/features_v7.py`).
//!
//! `PendingTrigger { position }` is the one new arm: a same-controller
//! pending trigger (`state.engine.pending_triggers[position]`) whose live
//! source sits unrevealed in its owner's library. `position` indexes the
//! live `pending_triggers` vector directly (not a decision-scoped
//! sub-slice), so it is stable across both `Decision::ChooseTargets`
//! (always position 0, since only `pending_triggers[0]` can ever reach that
//! decision) and `Decision::OrderTriggers` (0..group_len, up to
//! `FLAT_ACTION_MAX_TRIGGER_ORDER_REFS_V1 = 7`).

use crate::ids::PlayerId;
use crate::policy_observation_v6::{
    DecisionLocalLibraryV6, FinalizedChosenCreatureCostV6, HistoricalSourceContextV6,
    PendingCastObjectCostV6, PendingChosenCreatureCostV6, QueuedWardPaymentV6, WardPaymentV6,
};
use crate::rl::{CardStableRefV1, RlContractError, StackItemKindV2};
use crate::state::GameState;
use serde::{Deserialize, Serialize};

type Result<T> = std::result::Result<T, RlContractError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum HistoricalSourceContextV7 {
    Stack { stack_index: u32 },
    PendingEffect,
    /// `position` indexes `state.engine.pending_triggers` directly (the
    /// live, full vector -- never a decision-scoped sub-slice).
    PendingTrigger { position: u32 },
}

impl From<HistoricalSourceContextV6> for HistoricalSourceContextV7 {
    fn from(value: HistoricalSourceContextV6) -> Self {
        match value {
            HistoricalSourceContextV6::Stack { stack_index } => {
                HistoricalSourceContextV7::Stack { stack_index }
            }
            HistoricalSourceContextV6::PendingEffect => HistoricalSourceContextV7::PendingEffect,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct HistoricalPublicSourceV7 {
    pub context: HistoricalSourceContextV7,
    pub source: CardStableRefV1,
    pub stack_item_kind: StackItemKindV2,
}

/// Sibling of [`crate::policy_observation_v6::PolicyObservationExtensionsV6`]:
/// every field except `historical_public_sources` reuses the identical V6
/// type unmodified (nothing about those fields changes in this fork).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct PolicyObservationExtensionsV7 {
    pub pending_cast_object_cost: Option<PendingCastObjectCostV6>,
    pub decision_local_library: Option<DecisionLocalLibraryV6>,
    pub historical_public_sources: Vec<HistoricalPublicSourceV7>,
    pub pending_chosen_creature_cost: Option<PendingChosenCreatureCostV6>,
    pub finalized_chosen_creature_costs: Vec<FinalizedChosenCreatureCostV6>,
    pub pending_ward_payment: Option<WardPaymentV6>,
    pub queued_ward_payments: Vec<QueuedWardPaymentV6>,
}

/// V7 producer, forked from
/// [`crate::rl::policy_observation_extensions_v6`]: reuses that (unmodified)
/// V6 producer for every existing extension kind, including the existing
/// `Stack`/`PendingEffect` historical-source rows (converted to
/// [`HistoricalSourceContextV7`] by value), then additively appends one
/// `PendingTrigger { position }` row for every `(position, pending)` in
/// `state.engine.pending_triggers.iter().enumerate()` where: the trigger's
/// controller is `acting_player`; it still carries a `source_contract`; and
/// its live source is unrevealed in the owner's library (the exact
/// predicate of `trigger::pending_trigger_choose_targets_gate_v1`'s
/// hiddenness check, factored into
/// [`crate::trigger::pending_trigger_hidden_source_v1`]). This is
/// decision-agnostic: it does not consult the current `Decision` at all, so
/// it produces the identical set of rows whether the active decision is
/// `ChooseTargets` (position 0 only, since only `pending_triggers[0]` can
/// ever reach that decision) or `OrderTriggers` (any prefix position).
pub(crate) fn policy_observation_extensions_v7(
    state: &GameState,
    acting_player: PlayerId,
) -> Result<PolicyObservationExtensionsV7> {
    let v6 = crate::rl::policy_observation_extensions_v6(state, acting_player)?;
    let mut historical_public_sources: Vec<HistoricalPublicSourceV7> = v6
        .historical_public_sources
        .into_iter()
        .map(|row| HistoricalPublicSourceV7 {
            context: row.context.into(),
            source: row.source,
            stack_item_kind: row.stack_item_kind,
        })
        .collect();
    for (position, pending) in state.engine.pending_triggers.iter().enumerate() {
        if pending.controller != acting_player {
            continue;
        }
        let Some(contract) = pending.source_contract else {
            continue;
        };
        if !crate::trigger::pending_trigger_hidden_source_v1(state, pending) {
            continue;
        }
        let position = u32::try_from(position)
            .map_err(|_| RlContractError("pending trigger position exceeds u32".into()))?;
        historical_public_sources.push(HistoricalPublicSourceV7 {
            context: HistoricalSourceContextV7::PendingTrigger { position },
            source: CardStableRefV1 {
                arena_id: pending.source.0,
                card_db_id: contract.card_def,
                owner: contract.owner.into(),
                controller: contract.controller.into(),
                zone: contract.zone,
                zone_change_count: contract.zone_change_count,
            },
            stack_item_kind: if pending.is_madness_offer {
                StackItemKindV2::MadnessOffer
            } else {
                StackItemKindV2::TriggeredAbility
            },
        });
    }
    Ok(PolicyObservationExtensionsV7 {
        pending_cast_object_cost: v6.pending_cast_object_cost,
        decision_local_library: v6.decision_local_library,
        historical_public_sources,
        pending_chosen_creature_cost: v6.pending_chosen_creature_cost,
        finalized_chosen_creature_costs: v6.finalized_chosen_creature_costs,
        pending_ward_payment: v6.pending_ward_payment,
        queued_ward_payments: v6.queued_ward_payments,
    })
}
