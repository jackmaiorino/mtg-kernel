//! Public saved-loss occurrences; original worlds and engine journals stay private.
use crate::bloodthirsty_conqueror_v49::{self as conqueror, custody::JournalV49};
use crate::event::CommittedEvent;
use crate::rl::{CardStableRefV1, PlayerSeatV1, RlContractError};
use crate::rules_meaning_bloodthirsty_conqueror_v49 as meaning;
use crate::state::{AbilitySourceContractV4, GameState, StackItem, Zone};
use crate::trigger::PendingTrigger;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA: &str = "mtg-kernel/bloodthirsty-conqueror/public/v49";
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PhaseV49 {
    Fresh,
    Held,
    Published,
    Resolved,
    Countered,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LossOccurrenceV49 {
    /// Public ordinal of a real loss operation, never its executable history index.
    pub operation: usize,
    pub losing_player: PlayerSeatV1,
    pub actual_amount: i32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OccurrenceV49 {
    pub occurrence: usize,
    pub loss_operation: usize,
    pub source: CardStableRefV1,
    /// Only the original still-public BF incarnation, never a hidden or reentered source.
    pub current_original_source: Option<CardStableRefV1>,
    pub original_controller: PlayerSeatV1,
    pub losing_opponent: PlayerSeatV1,
    pub saved_actual_amount: i32,
    pub phase: PhaseV49,
    pub actual_gain: Option<i32>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TriggerOrderV49 {
    pub controller: PlayerSeatV1,
    pub group_position: usize,
    pub occurrence: usize,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactsV49 {
    pub schema: String,
    pub producer_sha256: String,
    pub card_db_hash: u64,
    pub meanings: Vec<meaning::MeaningV49>,
    pub losses: Vec<LossOccurrenceV49>,
    pub occurrences: Vec<OccurrenceV49>,
    pub trigger_order: Vec<TriggerOrderV49>,
}
pub fn producer_sha256() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut hash = Sha256::new();
        for text in [
            SCHEMA,
            include_str!("conqueror_public_v49.rs"),
            include_str!("sower_restriction_capture_v1.rs"),
        ] {
            hash.update((text.len() as u64).to_le_bytes());
            hash.update(text.as_bytes());
        }
        hash.update(meaning::producer_sha256().as_bytes());
        hash.finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    })
    .clone()
}
fn source_ref(source: AbilitySourceContractV4) -> CardStableRefV1 {
    // Eligible observers are genuine original face-up front-face sources.
    // This identity was public when the operation completed, irrespective of later zones.
    CardStableRefV1 {
        arena_id: source.source.0,
        card_db_id: source.card_def,
        owner: source.owner.into(),
        controller: source.controller.into(),
        zone: source.zone,
        zone_change_count: source.zone_change_count,
    }
}
fn waiting(state: &GameState) -> impl Iterator<Item = &PendingTrigger> {
    state
        .engine
        .pending_triggers
        .iter()
        .chain(state.legend_pending_v1.iter().flatten())
        .chain(
            state
                .pending_legend_rule_v1
                .iter()
                .flat_map(|p| p.waiting_triggers()),
        )
        .chain(
            state
                .standard_v1
                .iter()
                .filter_map(|s| s.resolution_play.as_ref())
                .flat_map(|p| p.deferred_triggers.iter()),
        )
}
/// Shape classification only; the helpers below authenticate complete real owners.
pub fn selected(item: &StackItem) -> bool {
    item.inline_effect
        .as_ref()
        .and_then(conqueror::custody::binding)
        .is_some()
}
/// One full validation of one immutably borrowed original state. This is
/// private construction-time evidence, never a cache, DTO or restored marker.
pub(crate) struct ValidatedStateV49<'a> {
    state: &'a GameState,
}
impl<'a> ValidatedStateV49<'a> {
    pub(crate) fn new(state: &'a GameState) -> Result<Self, RlContractError> {
        conqueror::validate(state).map_err(RlContractError)?;
        Ok(Self { state })
    }
    pub(crate) fn require_state(&self, state: &GameState) -> Result<(), RlContractError> {
        if !std::ptr::eq(self.state, state) {
            return Err(RlContractError(
                "Conqueror validation belongs to a different original state".into(),
            ));
        }
        Ok(())
    }
    pub(crate) fn pending_source_reference(
        &self,
        state: &GameState,
        pending: &PendingTrigger,
    ) -> Result<Option<CardStableRefV1>, RlContractError> {
        self.require_state(state)?;
        pending_source_reference_validated(self.state, pending)
    }
    pub(crate) fn stack_source_reference(
        &self,
        state: &GameState,
        item: &StackItem,
    ) -> Result<Option<CardStableRefV1>, RlContractError> {
        self.require_state(state)?;
        stack_source_reference_validated(self.state, item)
    }
}
pub(crate) fn validate_with_proof(
    state: &GameState,
    proof: Option<&ValidatedStateV49<'_>>,
) -> Result<(), RlContractError> {
    match proof {
        Some(proof) => proof.require_state(state),
        None => conqueror::validate(state).map_err(RlContractError),
    }
}
pub fn stack_source_reference(
    state: &GameState,
    item: &StackItem,
) -> Result<Option<CardStableRefV1>, RlContractError> {
    crate::snack_wide_life_v59::require_legacy(state).map_err(RlContractError)?;
    let Some(_) = item
        .inline_effect
        .as_ref()
        .and_then(conqueror::custody::binding)
    else {
        return Ok(None);
    };
    conqueror::validate(state).map_err(RlContractError)?;
    stack_source_reference_validated(state, item)
}
fn stack_source_reference_validated(
    state: &GameState,
    item: &StackItem,
) -> Result<Option<CardStableRefV1>, RlContractError> {
    let Some(capture) = item
        .inline_effect
        .as_ref()
        .and_then(conqueror::custody::binding)
    else {
        return Ok(None);
    };
    if !state.stack.iter().any(|original| original == item) {
        return Err(RlContractError(
            "Conqueror source requires genuine complete live row".into(),
        ));
    }
    Ok(Some(source_ref(
        conqueror::root_at(state, capture)
            .map_err(RlContractError)?
            .0,
    )))
}
pub fn pending_source_reference(
    state: &GameState,
    pending: &PendingTrigger,
) -> Result<Option<CardStableRefV1>, RlContractError> {
    crate::snack_wide_life_v59::require_legacy(state).map_err(RlContractError)?;
    let Some(_) = conqueror::custody::binding(&pending.effect) else {
        return Ok(None);
    };
    conqueror::validate(state).map_err(RlContractError)?;
    pending_source_reference_validated(state, pending)
}
fn pending_source_reference_validated(
    state: &GameState,
    pending: &PendingTrigger,
) -> Result<Option<CardStableRefV1>, RlContractError> {
    let Some(capture) = conqueror::custody::binding(&pending.effect) else {
        return Ok(None);
    };
    if !waiting(state).any(|original| original == pending) {
        return Err(RlContractError(
            "Conqueror source requires genuine complete pending owner".into(),
        ));
    }
    Ok(Some(source_ref(
        conqueror::root_at(state, capture)
            .map_err(RlContractError)?
            .0,
    )))
}
fn entry(state: &GameState, at: u64) -> Result<&CommittedEvent, RlContractError> {
    state
        .engine
        .event_history
        .get(
            usize::try_from(at)
                .map_err(|_| RlContractError("Conqueror history exceeds usize".into()))?,
        )
        .ok_or_else(|| RlContractError("Conqueror validated original entry absent".into()))
}
fn journal(state: &GameState, at: u64) -> Result<&JournalV49, RlContractError> {
    match entry(state, at)? {
        CommittedEvent::ConquerorJournalV49 { journal } => Ok(journal.as_ref()),
        _ => Err(RlContractError(
            "Conqueror validated original journal absent".into(),
        )),
    }
}
/// Metadata derives only from the public catalog, never a hidden battlefield roster.
pub fn meanings() -> Result<Vec<meaning::MeaningV49>, RlContractError> {
    let mut meanings = Vec::new();
    for (id, _) in crate::card_def::CARD_DEFS.iter().enumerate() {
        let id = u16::try_from(id)
            .map_err(|_| RlContractError("Conqueror catalog exceeds u16".into()))?;
        if conqueror::owns_definition(id) {
            let selector = meaning::live_selector().map_err(RlContractError)?;
            for root in [
                meaning::RootV49::CreatureSpell,
                meaning::RootV49::OpponentLifeLossBatch,
            ] {
                meanings.push(meaning::meaning(&selector, id, root).map_err(RlContractError)?);
            }
        }
    }
    Ok(meanings)
}
pub fn capture(state: &GameState) -> Result<FactsV49, RlContractError> {
    crate::snack_wide_life_v59::require_legacy(state).map_err(RlContractError)?;
    conqueror::validate(state).map_err(RlContractError)?;
    let meanings = meanings()?;
    let batches = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(at, event)| {
            if let CommittedEvent::ConquerorLossBatchV49 { batch } = event {
                Some((at as u64, batch.as_ref()))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let mut losses = Vec::new();
    for (operation, (_, batch)) in batches.iter().enumerate() {
        for player in [crate::ids::PlayerId::P0, crate::ids::PlayerId::P1] {
            let amount = batch.amount_for(player).map_err(RlContractError)?;
            if amount > 0 {
                losses.push(LossOccurrenceV49 {
                    operation,
                    losing_player: player.into(),
                    actual_amount: amount,
                });
            }
        }
    }
    let captures = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(at, event)| {
            matches!(event, CommittedEvent::ConquerorCapturedV49 { .. }).then_some(at as u64)
        })
        .collect::<Vec<_>>();
    let mut occurrences = Vec::new();
    for (occurrence, captured) in captures.iter().copied().enumerate() {
        let (source, amount) = conqueror::root_at(state, captured).map_err(RlContractError)?;
        let CommittedEvent::ConquerorCapturedV49 {
            batch_history_index,
            ..
        } = entry(state, captured)?
        else {
            unreachable!("validated filtered capture");
        };
        let operation = batches
            .iter()
            .position(|(at, _)| at == batch_history_index)
            .ok_or_else(|| RlContractError("Conqueror public loss operation absent".into()))?;
        let mut row = OccurrenceV49 {
            occurrence,
            loss_operation: operation,
            source: source_ref(source),
            current_original_source: state
                .objects
                .try_get(source.source)
                .filter(|live| {
                    live.zone == Zone::Battlefield
                        && live.owner == source.owner
                        && live.zone_change_count == source.zone_change_count
                })
                .map(|live| crate::sower_restriction_capture_v1::masked_ref(source.source, live))
                .transpose()?,
            original_controller: source.controller.into(),
            losing_opponent: source.controller.opponent().into(),
            saved_actual_amount: amount,
            phase: if waiting(state)
                .any(|p| conqueror::custody::binding(&p.effect) == Some(captured))
            {
                PhaseV49::Held
            } else {
                PhaseV49::Fresh
            },
            actual_gain: None,
        };
        let publication = state.engine.event_history.iter().enumerate().find_map(|(at, event)| match event {
            CommittedEvent::ConquerorJournalV49 { journal } if matches!(journal.as_ref(), JournalV49::Published { capture, .. } if *capture == captured) => Some(at as u64), _ => None,
        });
        if let Some(publication) = publication {
            row.phase = PhaseV49::Published;
            let removed = state
                .engine
                .event_history
                .iter()
                .enumerate()
                .find_map(|(at, event)| match event {
                    CommittedEvent::ConquerorJournalV49 { journal } => match journal.as_ref() {
                        JournalV49::Removed {
                            publication: original,
                            countered,
                            ..
                        } if *original == publication => Some((at as u64, *countered)),
                        _ => None,
                    },
                    _ => None,
                });
            if let Some((removal, countered)) = removed {
                row.phase = if countered {
                    PhaseV49::Countered
                } else {
                    PhaseV49::Resolved
                };
                let closed = state
                    .engine
                    .event_history
                    .iter()
                    .find_map(|event| match event {
                        CommittedEvent::ConquerorJournalV49 { journal } => match journal.as_ref() {
                            JournalV49::Closed {
                                removal: original,
                                gain,
                            } if *original == removal => Some(*gain),
                            _ => None,
                        },
                        _ => None,
                    })
                    .ok_or_else(|| {
                        RlContractError(
                            "Conqueror public completion lost authenticated closure".into(),
                        )
                    })?;
                if let Some(gain) = closed {
                    let JournalV49::Gain {
                        gain_history_index, ..
                    } = journal(state, gain)?
                    else {
                        return Err(RlContractError(
                            "Conqueror public completion names foreign gain".into(),
                        ));
                    };
                    let CommittedEvent::LifeGain { player, amount } =
                        entry(state, *gain_history_index)?
                    else {
                        return Err(RlContractError(
                            "Conqueror public actual gain absent".into(),
                        ));
                    };
                    if *player != source.controller {
                        return Err(RlContractError(
                            "Conqueror public beneficiary changed".into(),
                        ));
                    }
                    row.actual_gain = Some(*amount);
                }
            }
        }
        occurrences.push(row);
    }
    let mut positions = [0usize; 2];
    let mut trigger_order = Vec::new();
    for pending in &state.engine.pending_triggers {
        let group_position = positions[pending.controller.index()];
        positions[pending.controller.index()] += 1;
        if let Some(capture) = conqueror::custody::binding(&pending.effect) {
            let occurrence = captures
                .iter()
                .position(|original| *original == capture)
                .ok_or_else(|| RlContractError("Conqueror order occurrence absent".into()))?;
            trigger_order.push(TriggerOrderV49 {
                controller: pending.controller.into(),
                group_position,
                occurrence,
            });
        }
    }
    Ok(FactsV49 {
        schema: SCHEMA.into(),
        producer_sha256: producer_sha256(),
        card_db_hash: crate::card_def::KERNEL_CARDDB_HASH,
        meanings,
        losses,
        occurrences,
        trigger_order,
    })
}
pub fn validate(state: &GameState, facts: &FactsV49) -> Result<(), RlContractError> {
    if *facts != capture(state)? {
        return Err(RlContractError(
            "Conqueror full public facts changed".into(),
        ));
    }
    Ok(())
}
#[cfg(test)]
mod tests;
