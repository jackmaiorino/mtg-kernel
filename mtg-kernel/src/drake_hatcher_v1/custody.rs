//! Private ownership for genuine combat captures and paid Drake activations.
//! Whole enclosing operations are replayed, including all prior family history.
use super::SourceCounterKindV1;
use crate::{
    card_def::{CostComponent, TargetSpec, CARD_DEFS},
    effect::{EffectOp, ExecCtx},
    engine::PendingActivation,
    event::{CommittedEvent, ProposedEvent},
    ids::{ObjectId, PlayerId, StackItemId},
    state::{AbilitySourceContractV4, GameState, StackItem, StackItemKind, StackStateV4, Zone},
    trigger::PendingTrigger,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, VecDeque},
    hash::{Hash, Hasher},
};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedgerV1 {
    pub issued: u64,
}
impl LedgerV1 {
    pub fn is_empty(&self) -> bool {
        self.issued == 0
    }
}
impl Hash for LedgerV1 {
    fn hash<H: Hasher>(&self, h: &mut H) {
        if self.issued != 0 {
            b"drake-custody/v1".hash(h);
            self.issued.hash(h);
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordV1 {
    pub index: u64,
    pub value: JournalV1,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RemovalV1 {
    Resolving,
    Countered,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum JournalV1 {
    CombatInput {
        world: Box<GameState>,
        proposals: Vec<ProposedEvent>,
    },
    CombatFinished {
        input: u64,
    },
    Captured {
        input: u64,
        damage: u64,
        marker: u64,
        source: AbilitySourceContractV4,
        player: PlayerId,
        amount: i32,
    },
    Collected {
        capture: u64,
        pending: Box<PendingTrigger>,
    },
    Published {
        capture: u64,
        item: Box<StackItem>,
    },
    Debit {
        source: AbilitySourceContractV4,
        kind: SourceCounterKindV1,
        count: u16,
        before: i32,
        after: i32,
    },
    Paid {
        debit: u64,
        original: Box<GameState>,
        pending: Box<PendingActivation>,
        item: Box<StackItem>,
    },
    Removed {
        publication: u64,
        item: Box<StackItem>,
        kind: RemovalV1,
        original: Box<GameState>,
    },
    Completed {
        removed: u64,
        output: Box<GameState>,
    },
    Enclosing {
        start: u64,
        original: Box<GameState>,
        operation: crate::linked_aura_operation_v1::OperationV1,
        answers: Vec<crate::linked_aura_operation_v1::GeneralAnswerV36>,
        used_banner: bool,
        output: Box<GameState>,
    },
}
thread_local! { static VERIFYING: Cell<bool> = const { Cell::new(false) }; }

// Process-local memoization of one pure enclosing replay predicate, never of
// validate(). Keys retain neither a world nor executable authority. No caller
// can restore or serialize this cache; misses run the original predicate.
const REPLAY_MEMO_LIMIT: usize = 1024;
type ReplayKey = [u8; 32];
thread_local! {
    static REPLAY_MEMO: RefCell<VecDeque<ReplayKey>> = const { RefCell::new(VecDeque::new()) };
}

/// Every byte of the actual history before the physical row, including the
/// operation's actual emitted segment, and the complete current enclosing row
/// are committed. Round-trip equality makes serde injective on eligible typed
/// inputs, including every nested serde-skipped runtime field. Ineligibility
/// affects performance only and must never become a validation rejection.
fn replay_keys(history: &[CommittedEvent]) -> BTreeMap<u64, ReplayKey> {
    #[cfg(test)]
    let _timing = crate::rl_session::test_timing_v1::span(
        crate::rl_session::test_timing_v1::Phase::DrakeReplayKeys,
    );
    let mut keys = BTreeMap::new();
    let mut prefix = Sha256::new();
    prefix.update(b"drake-enclosing-actual-history/v1");
    for (position, event) in history.iter().enumerate() {
        let Ok(position) = u64::try_from(position) else {
            break;
        };
        let Ok(bytes) = serde_json::to_vec(event) else {
            break;
        };
        let Ok(length) = u64::try_from(bytes.len()) else {
            break;
        };
        if !serde_json::from_slice::<CommittedEvent>(&bytes).is_ok_and(|typed| typed == *event) {
            break;
        }
        if journal(event).is_some_and(|row| matches!(row.value, JournalV1::Enclosing { .. })) {
            let mut key = Sha256::new();
            key.update(b"drake-enclosing-replay-predicate/v1");
            key.update(prefix.clone().finalize());
            key.update(position.to_le_bytes());
            key.update(length.to_le_bytes());
            key.update(&bytes);
            keys.insert(position, key.finalize().into());
        }
        prefix.update(position.to_le_bytes());
        prefix.update(length.to_le_bytes());
        prefix.update(&bytes);
    }
    keys
}

fn replay_cached(key: &ReplayKey) -> bool {
    // Release the borrow before any replay, including reentrant validation.
    REPLAY_MEMO.with(|cache| cache.borrow().contains(key))
}
fn remember_replay(key: ReplayKey) {
    REPLAY_MEMO.with(|cache| {
        let mut cache = cache.borrow_mut();
        if !cache.contains(&key) {
            if cache.len() == REPLAY_MEMO_LIMIT {
                cache.pop_front();
            }
            cache.push_back(key);
        }
    });
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct ReplayMemoStats {
    pub executions: u64,
    pub hits: u64,
    pub ineligible: u64,
}
#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
thread_local! {
    static REPLAY_MEMO_STATS: Cell<ReplayMemoStats> = const {
        Cell::new(ReplayMemoStats { executions: 0, hits: 0, ineligible: 0 })
    };
}
#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
pub(super) fn reset_replay_memo_for_test() {
    REPLAY_MEMO.with(|cache| cache.borrow_mut().clear());
    REPLAY_MEMO_STATS.with(|stats| stats.set(ReplayMemoStats::default()));
}
#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
pub(super) fn replay_memo_stats_for_test() -> ReplayMemoStats {
    REPLAY_MEMO_STATS.with(Cell::get)
}
#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn count_replay(hit: bool, eligible: bool) {
    REPLAY_MEMO_STATS.with(|stats| {
        let mut value = stats.get();
        if hit {
            value.hits = value.hits.saturating_add(1);
        } else {
            value.executions = value.executions.saturating_add(1);
            if !eligible {
                value.ineligible = value.ineligible.saturating_add(1);
            }
        }
        stats.set(value);
    });
}

struct VerifyGuard(bool);
impl Drop for VerifyGuard {
    fn drop(&mut self) {
        VERIFYING.with(|cell| cell.set(self.0));
    }
}
fn verification<T>(run: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    let old = VERIFYING.with(|cell| cell.replace(true));
    let _guard = VerifyGuard(old);
    run()
}
pub(crate) fn journal(event: &CommittedEvent) -> Option<&RecordV1> {
    if let CommittedEvent::DrakeJournalV1 { record } = event {
        Some(record)
    } else {
        None
    }
}
fn at(state: &GameState, index: u64) -> Result<&RecordV1, String> {
    let row = usize::try_from(index)
        .ok()
        .and_then(|i| state.engine.event_history.get(i))
        .and_then(journal)
        .ok_or("Drake original record absent")?;
    if row.index != index {
        return Err("Drake original record moved".into());
    }
    Ok(row)
}
fn emit(state: &mut GameState, value: JournalV1) -> Result<u64, String> {
    if !crate::linked_aura_operation_v1::locally_owned(state) {
        return Err("Drake record lacks actual enclosing operation owner".into());
    }
    let next = state
        .engine
        .drake_ledger_v1
        .issued
        .checked_add(1)
        .ok_or("Drake journal count overflow")?;
    let index =
        u64::try_from(state.engine.event_history.len()).map_err(|_| "Drake history overflow")?;
    state
        .engine
        .event_history
        .push(CommittedEvent::DrakeJournalV1 {
            record: Box::new(RecordV1 { index, value }),
        });
    state.engine.drake_ledger_v1.issued = next;
    Ok(index)
}
/// Strip only the outer history; replay reattaches its actual immutable prefix.
/// Pending logs, complete stack, hidden world and family checkpoints are retained.
fn world(state: &GameState) -> Result<GameState, String> {
    #[cfg(test)]
    let _timing = crate::rl_session::test_timing_v1::span(
        crate::rl_session::test_timing_v1::Phase::DrakeWorld,
    );
    let mut copy = state.clone();
    copy.engine.event_history.clear();
    let mut value = serde_json::to_value(&copy).map_err(|e| e.to_string())?;
    strip_nested_receipts(&mut value, &state.engine.event_history)?;
    serde_json::from_value(value).map_err(|e| e.to_string())
}
// A sibling checkpoint retains its exact original history length and positions.
// Only existing Drake records in that original are represented by their actual
// immutable outer index. Every non-Drake field/event remains byte-equivalent.
fn strip_nested_receipts(
    value: &mut serde_json::Value,
    prefix: &[CommittedEvent],
) -> Result<(), String> {
    fn strip(
        value: &mut serde_json::Value,
        position: Option<usize>,
        prefix: &[CommittedEvent],
    ) -> Result<(), String> {
        if let Some(record) = value.get("DrakeJournalV1").and_then(|v| v.get("record")) {
            let index = record
                .get("index")
                .and_then(|i| i.as_u64())
                .ok_or("Drake nested original index absent")?;
            if position.and_then(|p| u64::try_from(p).ok()) != Some(index) {
                return Err(
                    "Drake nested original record shifted from its bound input position".into(),
                );
            }
            let actual = usize::try_from(index)
                .ok()
                .and_then(|at| prefix.get(at))
                .ok_or("Drake nested original lies outside actual enclosing prefix")?;
            if journal(actual).is_none_or(|row| row.index != index)
                || serde_json::to_value(actual).map_err(|e| e.to_string())? != *value
            {
                return Err(
                    "Drake nested original differs from whole actual indexed record".into(),
                );
            }
            *value = serde_json::json!({ "DrakeHistoryRefV1": { "index": index } });
            return Ok(());
        }
        match value {
            serde_json::Value::Array(values) => {
                for child in values {
                    strip(child, None, prefix)?;
                }
            }
            serde_json::Value::Object(values) => {
                for (name, child) in values {
                    if name == "event_history" {
                        let events = child
                            .as_array_mut()
                            .ok_or("Drake nested history changed type")?;
                        for (at, event) in events.iter_mut().enumerate() {
                            strip(event, Some(at), prefix)?;
                        }
                    } else {
                        strip(child, None, prefix)?;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
    strip(value, None, prefix)
}
fn restore_world(saved: &GameState, prefix: &[CommittedEvent]) -> Result<GameState, String> {
    fn restore(
        value: &mut serde_json::Value,
        prefix: &[CommittedEvent],
        position: Option<usize>,
    ) -> Result<(), String> {
        if let Some(index) = value
            .get("DrakeHistoryRefV1")
            .and_then(|v| v.get("index"))
            .and_then(|v| v.as_u64())
        {
            if position.and_then(|p| u64::try_from(p).ok()) != Some(index) {
                return Err(
                    "Drake nested original reference shifted from its bound input position".into(),
                );
            }
            let actual = usize::try_from(index)
                .ok()
                .and_then(|i| prefix.get(i))
                .ok_or("Drake nested original prefix absent")?;
            if journal(actual).is_none_or(|r| r.index != index) {
                return Err("Drake nested original index shifted/ambiguous".into());
            }
            // Clone the actual original record. Never descend into its saved worlds,
            // which retain their own explicit input positions and replay boundary.
            *value = serde_json::to_value(actual).map_err(|e| e.to_string())?;
            return Ok(());
        }
        match value {
            serde_json::Value::Array(values) => {
                for child in values {
                    restore(child, prefix, None)?;
                }
            }
            serde_json::Value::Object(values) => {
                for (name, child) in values {
                    if name == "event_history" {
                        let events = child
                            .as_array_mut()
                            .ok_or("Drake nested history changed type")?;
                        for (at, event) in events.iter_mut().enumerate() {
                            restore(event, prefix, Some(at))?;
                        }
                    } else {
                        restore(child, prefix, None)?;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
    let mut value = serde_json::to_value(saved).map_err(|e| e.to_string())?;
    restore(&mut value, prefix, None)?;
    let mut input: GameState = serde_json::from_value(value).map_err(|e| e.to_string())?;
    input.engine.event_history = prefix.to_vec();
    Ok(input)
}
pub(crate) fn definition(id: u16) -> bool {
    cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) && CARD_DEFS.get(usize::from(id)).is_some_and(|d| {
        d.name == "Drake Hatcher"
            && d.is_executable()
            && d.activated_abilities.len() == 1
            && d.activated_abilities[0].cost
                == [CostComponent::RemoveSourceCountersV1 {
                    kind: SourceCounterKindV1::Incubation,
                    count: 3,
                }]
            && (d.activated_abilities[0].effect)() == EffectOp::DrakeCreateTokenFromPaidV1
    })
}
/// A fresh owner is selected from publicly active printing semantics only.
/// A concealed underlying definition cannot change ownership, refusal or Hash.
fn fresh_source(state: &GameState, id: ObjectId) -> bool {
    state.objects.try_get(id).is_some_and(|object| {
        matches!(object.zone, Zone::Battlefield | Zone::Stack)
            && object.v4.face_down_v1.is_none()
            && definition(object.card_def)
            && crate::continuous_characteristics_v1::printed_abilities_active(state, id)
    })
}
pub(crate) fn relevant(state: &GameState) -> bool {
    !state.engine.drake_ledger_v1.is_empty()
        || state
            .engine
            .event_history
            .iter()
            .any(|event| journal(event).is_some())
        || state.stack.iter().any(tracked)
        || held(state).any(|pending| capture_of(&pending.effect).is_some())
        || state.objects.iter().any(|(id, _)| fresh_source(state, id))
}
fn capture_of(effect: &EffectOp) -> Option<u64> {
    if let EffectOp::DrakeIncubationFromCaptureV1 { capture } = effect {
        Some(*capture)
    } else {
        None
    }
}
fn tracked(item: &StackItem) -> bool {
    item.inline_effect
        .as_ref()
        .is_some_and(|op| capture_of(op).is_some() || *op == EffectOp::DrakeCreateTokenFromPaidV1)
}
fn source(state: &GameState, capture: u64) -> Result<(AbilitySourceContractV4, i32), String> {
    if let JournalV1::Captured { source, amount, .. } = &at(state, capture)?.value {
        Ok((*source, *amount))
    } else {
        Err("Drake root lacks original combat capture".into())
    }
}
fn pending(source: AbilitySourceContractV4, capture: u64) -> PendingTrigger {
    PendingTrigger {
        controller: source.controller,
        source: source.source,
        effect: EffectOp::DrakeIncubationFromCaptureV1 { capture },
        is_madness_offer: false,
        kicked: false,
        target_spec: TargetSpec::None,
        targets: vec![],
        target_contracts: vec![],
        placement_ordered: false,
        source_contract: Some(source),
        granted_by: None,
        optional_additional_cost_paid: None,
        paid_cost_refs: vec![],
    }
}
fn trigger_item(source: AbilitySourceContractV4, capture: u64, id: StackItemId) -> StackItem {
    StackItem {
        kind: StackItemKind::TriggeredAbility,
        source: source.source,
        controller: source.controller,
        targets: vec![],
        is_copy: false,
        inline_effect: Some(EffectOp::DrakeIncubationFromCaptureV1 { capture }),
        discarded: vec![],
        is_flashback: false,
        mode_chosen: 0,
        madness_offer: false,
        kicked: false,
        v4: StackStateV4 {
            stack_item_id: id,
            ability_source_contract: Some(source),
            ..Default::default()
        },
    }
}
fn activation_item(source: AbilitySourceContractV4, id: StackItemId) -> StackItem {
    StackItem {
        kind: StackItemKind::ActivatedAbility,
        source: source.source,
        controller: source.controller,
        targets: vec![],
        is_copy: false,
        inline_effect: Some(EffectOp::DrakeCreateTokenFromPaidV1),
        discarded: vec![],
        is_flashback: false,
        mode_chosen: 0,
        madness_offer: false,
        kicked: false,
        v4: StackStateV4 {
            stack_item_id: id,
            ability_source_contract: Some(source),
            activated_ability_index: Some(0),
            target_spec: Some(TargetSpec::None),
            ..Default::default()
        },
    }
}
pub(crate) fn begin_combat(
    state: &mut GameState,
    proposals: &[ProposedEvent],
) -> Result<(), String> {
    if !proposals
        .iter()
        .any(|p| matches!(p, ProposedEvent::Damage(d) if fresh_source(state, d.source)))
    {
        return Ok(());
    }
    if open_combat(state)?.is_some() {
        return Err("Drake combat input replayed before completion".into());
    }
    let original = Box::new(world(state)?);
    emit(
        state,
        JournalV1::CombatInput {
            world: original,
            proposals: proposals.to_vec(),
        },
    )?;
    Ok(())
}
fn open_combat(state: &GameState) -> Result<Option<u64>, String> {
    let mut inputs = Vec::new();
    for event in &state.engine.event_history {
        if let Some(row) = journal(event) {
            match &row.value {
                JournalV1::CombatInput { .. } => inputs.push(row.index),
                JournalV1::CombatFinished { input } => {
                    let i = inputs
                        .iter()
                        .position(|v| *v == *input)
                        .ok_or("Drake combat completion lacks input")?;
                    inputs.remove(i);
                }
                _ => {}
            }
        }
    }
    if inputs.len() > 1 {
        return Err("Drake combat inputs overlap".into());
    }
    Ok(inputs.first().copied())
}
pub(crate) fn original_generation(state: &GameState, object: ObjectId) -> Option<u32> {
    let input = open_combat(state).ok().flatten()?;
    let JournalV1::CombatInput { world, .. } = &at(state, input).ok()?.value else {
        return None;
    };
    let live = world.objects.try_get(object)?;
    fresh_source(world, object).then_some(live.zone_change_count)
}
pub(crate) fn finish_combat(state: &mut GameState) -> Result<(), String> {
    let Some(input) = open_combat(state)? else {
        return Ok(());
    };
    let JournalV1::CombatInput {
        world: original, ..
    } = &at(state, input)?.value
    else {
        unreachable!()
    };
    let original = original.clone();
    let start = usize::try_from(input).map_err(|_| "Drake batch index overflow")? + 1;
    let damages = state.engine.event_history[start..]
        .iter()
        .enumerate()
        .filter_map(|(offset, e)| {
            if let CommittedEvent::Damage {
                source,
                target: crate::state::Target::Player(player),
                amount,
            } = e
            {
                (*amount > 0).then_some(((start + offset) as u64, *source, *player, *amount))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let markers = state.engine.event_history[start..]
        .iter()
        .enumerate()
        .filter_map(|(offset, e)| {
            if let CommittedEvent::CombatDamageToPlayer {
                source,
                source_zone_change_count,
                player,
                amount,
            } = e
            {
                Some((
                    (start + offset) as u64,
                    *source,
                    *source_zone_change_count,
                    *player,
                    *amount,
                ))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    if damages.len() != markers.len() {
        return Err("Drake final damage/marker cardinality changed".into());
    }
    for ((damage, id, player, amount), (marker, marked, generation, hit, quantity)) in
        damages.into_iter().zip(markers)
    {
        if id != marked || player != hit || amount != quantity {
            return Err("Drake marker differs from final committed damage".into());
        }
        let Some(live) = original.objects.try_get(id) else {
            continue;
        };
        if !definition(live.card_def)
            || live.zone != Zone::Battlefield
            || !crate::continuous_characteristics_v1::printed_abilities_active(&original, id)
        {
            continue;
        }
        let source = AbilitySourceContractV4::capture(&original, id);
        if generation != source.zone_change_count {
            return Err("Drake combat rebound its original incarnation".into());
        }
        emit(
            state,
            JournalV1::Captured {
                input,
                damage,
                marker,
                source,
                player,
                amount,
            },
        )?;
    }
    emit(state, JournalV1::CombatFinished { input })?;
    Ok(())
}
pub(crate) fn collect(
    state: &mut GameState,
    events: &[CommittedEvent],
) -> Result<Vec<PendingTrigger>, String> {
    let mut ready = Vec::new();
    for e in &state.engine.event_history {
        let Some(row) = journal(e) else { continue };
        if let JournalV1::Captured { marker, .. } = &row.value {
            if state.engine.event_history.iter().any(|e| matches!(journal(e).map(|r|&r.value),Some(JournalV1::Collected {capture,..}) if *capture==row.index)) {continue;}
            let committed = state
                .engine
                .event_history
                .get(*marker as usize)
                .ok_or("Drake marker erased")?;
            if events.contains(committed) {
                ready.push(row.index);
            }
        }
    }
    let mut result = Vec::new();
    for capture in ready {
        let (original, _) = source(state, capture)?;
        let p = pending(original, capture);
        emit(
            state,
            JournalV1::Collected {
                capture,
                pending: Box::new(p.clone()),
            },
        )?;
        result.push(p);
    }
    Ok(result)
}
pub(crate) fn publish(state: &mut GameState, item: &StackItem) -> Result<(), String> {
    let Some(capture) = item.inline_effect.as_ref().and_then(capture_of) else {
        return Ok(());
    };
    let (original, _) = source(state, capture)?;
    if *item != trigger_item(original, capture, item.v4.stack_item_id)
        || item.v4.stack_item_id == StackItemId::default()
    {
        return Err("Drake publication changed original trigger row".into());
    }
    let collected=state.engine.event_history.iter().filter(|e| matches!(journal(e).map(|r|&r.value),Some(JournalV1::Collected {capture:c,..}) if *c==capture)).count();
    let published=state.engine.event_history.iter().any(|e| matches!(journal(e).map(|r|&r.value),Some(JournalV1::Published {capture:c,..}) if *c==capture));
    if collected != 1 || published {
        return Err("Drake publication missing/replayed collection".into());
    }
    emit(
        state,
        JournalV1::Published {
            capture,
            item: Box::new(item.clone()),
        },
    )?;
    Ok(())
}
pub(crate) struct PaymentOwnerV1 {
    original: Box<GameState>,
    pending: PendingActivation,
    start: usize,
    source: AbilitySourceContractV4,
}
pub(crate) fn capture_payment(
    state: &GameState,
    p: &PendingActivation,
) -> Result<Option<PaymentOwnerV1>, String> {
    let Some(live) = state.objects.try_get(p.source) else {
        return Err("Drake payment source absent".into());
    };
    // This loan owns only the printed slot. Equipment and Cauldron donor
    // activations have independent exact producers and payment validators.
    if !definition(live.card_def) || p.ability_index != 0 || p.cauldron_grant.0.is_some() {
        return Ok(None);
    };
    if live.zone != Zone::Battlefield
        || live.controller != p.controller
        || live.zone_change_count != p.source_zone_change_count
        || !crate::continuous_characteristics_v1::printed_abilities_active(state, p.source)
        || live.counters.incubation < 3
        || p.target_spec != TargetSpec::None
        || !p.targets_chosen.is_empty()
        || !p.target_contracts.is_empty()
        || !p.object_cost_chosen.is_empty()
    {
        return Err("Drake payment changed exact printed source/cost/input".into());
    }
    Ok(Some(PaymentOwnerV1 {
        original: Box::new(world(state)?),
        pending: p.clone(),
        start: state.engine.event_history.len(),
        source: AbilitySourceContractV4::capture(state, p.source),
    }))
}
pub(crate) fn debit(
    state: &mut GameState,
    object: ObjectId,
    kind: SourceCounterKindV1,
    count: u16,
    before: i32,
    after: i32,
) -> Result<(), String> {
    if count == 0 || before < 0 || before.checked_sub(i32::from(count)) != Some(after) || after < 0
    {
        return Err("Drake typed counter cost arithmetic changed".into());
    }
    let source = AbilitySourceContractV4::capture(state, object);
    emit(
        state,
        JournalV1::Debit {
            source,
            kind,
            count,
            before,
            after,
        },
    )?;
    Ok(())
}
pub(crate) fn record_paid(
    state: &mut GameState,
    id: StackItemId,
    owner: Option<PaymentOwnerV1>,
) -> Result<(), String> {
    let item = state
        .stack
        .iter()
        .find(|r| r.v4.stack_item_id == id)
        .ok_or("Drake paid row absent")?
        .clone();
    let Some(owner) = owner else {
        return if item.inline_effect == Some(EffectOp::DrakeCreateTokenFromPaidV1) {
            Err("Drake activation lost private prepayment loan".into())
        } else {
            Ok(())
        };
    };
    if item != activation_item(owner.source, id) {
        return Err("Drake paid row changed controller/source/entire program".into());
    }
    let debits = state.engine.event_history[owner.start..]
        .iter()
        .filter_map(journal)
        .filter(|r| matches!(r.value, JournalV1::Debit { .. }))
        .collect::<Vec<_>>();
    let [row] = debits.as_slice() else {
        return Err("Drake payment needs one exact debit".into());
    };
    let JournalV1::Debit {
        source,
        kind,
        count,
        before,
        after,
    } = &row.value
    else {
        unreachable!()
    };
    if *source != owner.source
        || *kind != SourceCounterKindV1::Incubation
        || *count != 3
        || *before
            != owner
                .original
                .objects
                .get(owner.source.source)
                .counters
                .incubation
        || before.checked_sub(3) != Some(*after)
        || state.objects.get(source.source).counters.incubation != *after
    {
        return Err("Drake publication lost original counter payment".into());
    }
    let debit = row.index;
    emit(
        state,
        JournalV1::Paid {
            debit,
            original: owner.original,
            pending: Box::new(owner.pending),
            item: Box::new(item),
        },
    )?;
    Ok(())
}
fn publication(state: &GameState, item: &StackItem) -> Result<u64, String> {
    let rows=state.engine.event_history.iter().filter_map(journal).filter(|r|matches!(&r.value,JournalV1::Published {item:i,..}|JournalV1::Paid {item:i,..} if i.v4.stack_item_id==item.v4.stack_item_id)).collect::<Vec<_>>();
    let [row] = rows.as_slice() else {
        return Err("Drake row lost unique actual publication/payment".into());
    };
    match &row.value {
        JournalV1::Published { item: i, .. } | JournalV1::Paid { item: i, .. }
            if i.as_ref() == item =>
        {
            Ok(row.index)
        }
        _ => Err("Drake published row changed".into()),
    }
}
pub(crate) fn validate_item(state: &GameState, item: &StackItem) -> Result<(), String> {
    if !tracked(item) {
        return Ok(());
    }
    let p = publication(state, item)?;
    if state.engine.event_history.iter().any(|e|matches!(journal(e).map(|r|&r.value),Some(JournalV1::Removed {publication,..}) if *publication==p)){return Err("Drake removed publication replayed".into());}
    Ok(())
}
pub(crate) struct OwnerV1 {
    publication: u64,
    item: StackItem,
    original: Box<GameState>,
    start: usize,
}
pub(crate) fn acquire(state: &GameState, item: &StackItem) -> Result<Option<OwnerV1>, String> {
    if !tracked(item) {
        return Ok(None);
    }
    validate_item(state, item)?;
    if state
        .stack
        .iter()
        .filter(|r| r.v4.stack_item_id == item.v4.stack_item_id)
        .collect::<Vec<_>>()
        .as_slice()
        != [item]
    {
        return Err("Drake removal lacks actual unique live row".into());
    }
    Ok(Some(OwnerV1 {
        publication: publication(state, item)?,
        item: item.clone(),
        original: Box::new(world(state)?),
        start: state.engine.event_history.len(),
    }))
}
fn after_remove(
    original: &GameState,
    item: &StackItem,
    kind: RemovalV1,
) -> Result<GameState, String> {
    let mut output = original.clone();
    let positions = output
        .stack
        .iter()
        .enumerate()
        .filter_map(|(i, r)| (r.v4.stack_item_id == item.v4.stack_item_id).then_some(i))
        .collect::<Vec<_>>();
    let [position] = positions.as_slice() else {
        return Err("Drake original stack row lost/duplicated".into());
    };
    if output.stack[*position] != *item
        || (kind == RemovalV1::Resolving && *position + 1 != output.stack.len())
    {
        return Err("Drake removal changed whole original stack/top".into());
    }
    output.stack.remove(*position);
    if kind == RemovalV1::Resolving {
        output.engine.pending_kicked_source = None;
    }
    Ok(output)
}
fn removed(state: &mut GameState, owner: &OwnerV1, kind: RemovalV1) -> Result<u64, String> {
    validate_item(state, &owner.item)?;
    if state.engine.event_history.len() != owner.start
        || world(state)? != after_remove(&owner.original, &owner.item, kind)?
    {
        return Err("Drake removal changed its borrowed entire input world".into());
    }
    emit(
        state,
        JournalV1::Removed {
            publication: owner.publication,
            item: Box::new(owner.item.clone()),
            kind,
            original: owner.original.clone(),
        },
    )
}
fn complete(state: &mut GameState, removed: u64) -> Result<(), String> {
    let output = Box::new(world(state)?);
    emit(state, JournalV1::Completed { removed, output })?;
    Ok(())
}
pub(crate) fn countered(state: &mut GameState, owner: Option<OwnerV1>) -> Result<(), String> {
    if let Some(owner) = owner {
        let at = removed(state, &owner, RemovalV1::Countered)?;
        complete(state, at)?;
    }
    Ok(())
}
pub(crate) fn resolve(state: &mut GameState, owner: OwnerV1, ctx: &ExecCtx) -> Result<(), String> {
    let mut expected = ExecCtx::no_targets(owner.item.source, owner.item.controller);
    expected.stack_item_id = Some(owner.item.v4.stack_item_id);
    expected.ability_source_contract = owner.item.v4.ability_source_contract;
    if *ctx != expected {
        return Err("Drake resolution changed original entire context".into());
    }
    let at = removed(state, &owner, RemovalV1::Resolving)?;
    match owner.item.inline_effect.as_ref() {
        Some(EffectOp::DrakeIncubationFromCaptureV1 { capture }) => {
            let (source, amount) = source(state, *capture)?;
            if Some(source) != ctx.ability_source_contract {
                return Err("Drake resolution rebound original damage source".into());
            }
            super::place(state, ctx, Some(amount))?;
        }
        Some(EffectOp::DrakeCreateTokenFromPaidV1) => {
            crate::effect::execute(
                &EffectOp::CreateToken {
                    token_def: crate::card_def::card_id_by_name("Drake Token")
                        .ok_or("Drake blueprint absent")?,
                    controller: crate::effect::PlayerRef::Controller,
                },
                ctx,
                state,
            );
        }
        _ => return Err("Drake resolution lost exact private root".into()),
    }
    if state.engine.halted.is_some() || state.engine.pending_effect.is_some() {
        return Err("Drake atomic operation halted/suspended".into());
    }
    complete(state, at)
}
/// Called by the original linked owner, after it restores all real checkpoints.
pub(crate) fn enclose(
    original: &GameState,
    operation: &crate::linked_aura_operation_v1::OperationV1,
    answers: &[crate::linked_aura_operation_v1::GeneralAnswerV36],
    used_banner: bool,
    output: &mut GameState,
) -> Result<(), String> {
    if VERIFYING.with(Cell::get) || (!relevant(original) && !relevant(output)) {
        return Ok(());
    }
    let start = u64::try_from(original.engine.event_history.len())
        .map_err(|_| "Drake operation prefix overflow")?;
    let index = output.engine.event_history.len() as u64;
    let count = output
        .engine
        .drake_ledger_v1
        .issued
        .checked_add(1)
        .ok_or("Drake operation count overflow")?;
    let record = RecordV1 {
        index,
        value: JournalV1::Enclosing {
            start,
            original: Box::new(world(original)?),
            operation: operation.clone(),
            answers: answers.to_vec(),
            used_banner,
            output: Box::new(world(output)?),
        },
    };
    output
        .engine
        .event_history
        .push(CommittedEvent::DrakeJournalV1 {
            record: Box::new(record),
        });
    output.engine.drake_ledger_v1.issued = count;
    Ok(())
}
fn held(state: &GameState) -> impl Iterator<Item = &PendingTrigger> {
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
fn refs<'a>(rows: &'a [&RecordV1], predicate: impl Fn(&JournalV1) -> bool) -> Vec<&'a RecordV1> {
    rows.iter()
        .copied()
        .filter(|r| predicate(&r.value))
        .collect()
}
/// Validate originals and one-use custody before a public engine mutation.
/// Private current-operation tails are permitted only under the actual Runner.
pub(crate) fn validate(state: &GameState) -> Result<(), String> {
    let rows = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(position, event)| journal(event).map(|row| (position, row)))
        .map(|(position, row)| {
            // An identical appended row must not borrow the earlier record's
            // index and enclosing range. Authenticate its physical position
            // before normalization, relation checks, or enclosing replay.
            if u64::try_from(position).ok() != Some(row.index) {
                return Err("Drake record index differs from actual history position".to_string());
            }
            Ok(row)
        })
        .collect::<Result<Vec<_>, String>>()?;
    if rows.len() as u64 != state.engine.drake_ledger_v1.issued {
        return Err("Drake original journal was erased/replayed".into());
    }
    if !rows.is_empty() {
        // Compression must prove every discarded nested original equals the
        // whole actual indexed record before any caller can normalize it.
        world(state)?;
    }
    if state
        .engine
        .event_history
        .iter()
        .any(|e| matches!(e, CommittedEvent::DrakeHistoryRefV1 { .. }))
    {
        return Err("Drake nested original reference escaped into actual history".into());
    }
    if state
        .engine
        .event_log
        .iter()
        .any(|e| journal(e).is_some() || matches!(e, CommittedEvent::DrakeHistoryRefV1 { .. }))
    {
        return Err("Drake private authority entered trigger log".into());
    }
    let local = crate::linked_aura_operation_v1::locally_owned(state);
    let verifying = VERIFYING.with(Cell::get);
    let keys = if verifying
        || !rows
            .iter()
            .any(|row| matches!(row.value, JournalV1::Enclosing { .. }))
    {
        BTreeMap::new()
    } else {
        replay_keys(&state.engine.event_history)
    };
    let mut verified_keys = Vec::new();
    let last_closed = rows
        .iter()
        .filter(|r| matches!(r.value, JournalV1::Enclosing { .. }))
        .map(|r| r.index)
        .max();
    for row in &rows {
        if state
            .engine
            .event_history
            .get(row.index as usize)
            .and_then(journal)
            != Some(*row)
        {
            return Err("Drake record index moved".into());
        }
        let coverage = rows.iter().filter(|operation| matches!(operation.value, JournalV1::Enclosing { start, .. } if row.index >= start && row.index < operation.index)).count();
        if !matches!(row.value, JournalV1::Enclosing { .. })
            && coverage != 1
            && !(local && last_closed.is_none_or(|last| row.index > last))
        {
            return Err("Drake record lost its unique original enclosing operation".into());
        }
        match &row.value {
            JournalV1::CombatInput { world, proposals } => {
                let genuine = proposals.iter().any(|p| matches!(p, ProposedEvent::Damage(d) if d.is_combat && fresh_source(world, d.source)));
                if !world.engine.event_history.is_empty() || !genuine {
                    return Err("Drake pre-batch input changed".into());
                }
            }
            JournalV1::CombatFinished { input } => {
                if !matches!(at(state, *input)?.value, JournalV1::CombatInput { .. })
                    || *input >= row.index
                {
                    return Err("Drake batch finished without earlier original input".into());
                }
            }
            JournalV1::Captured {
                input,
                damage,
                marker,
                source: original,
                player,
                amount,
            } => {
                let JournalV1::CombatInput { world, .. } = &at(state, *input)?.value else {
                    return Err("Drake capture input absent".into());
                };
                if *amount <= 0
                    || original.zone != Zone::Battlefield
                    || !definition(original.card_def)
                    || world.objects.try_get(original.source).is_none()
                    || AbilitySourceContractV4::capture(world, original.source) != *original
                    || !crate::continuous_characteristics_v1::printed_abilities_active(
                        world,
                        original.source,
                    )
                    || !(*input < *damage && *damage < *marker && *marker < row.index)
                {
                    return Err("Drake original combat source/controller/quantity changed".into());
                }
                let exact_damage = matches!(state.engine.event_history.get(*damage as usize), Some(CommittedEvent::Damage { source, target: crate::state::Target::Player(hit), amount: n }) if *source == original.source && hit == player && n == amount);
                let exact_marker = matches!(state.engine.event_history.get(*marker as usize), Some(CommittedEvent::CombatDamageToPlayer { source, source_zone_change_count, player: hit, amount: n }) if *source == original.source && *source_zone_change_count == original.zone_change_count && hit == player && n == amount);
                if !exact_damage || !exact_marker {
                    return Err(
                        "Drake capture no longer matches actual final committed damage/marker"
                            .into(),
                    );
                }
                let collections = refs(
                    &rows,
                    |j| matches!(j, JournalV1::Collected { capture, .. } if *capture == row.index),
                );
                if collections.len() > 1 || (!local && collections.len() != 1) {
                    return Err("Drake capture collection lost/duplicated".into());
                }
            }
            JournalV1::Collected {
                capture,
                pending: saved,
            } => {
                let (original, _) = source(state, *capture)?;
                if *capture >= row.index || saved.as_ref() != &pending(original, *capture) {
                    return Err("Drake collected input changed".into());
                }
                let publications = refs(
                    &rows,
                    |j| matches!(j, JournalV1::Published { capture: c, .. } if c == capture),
                );
                let held_count = held(state)
                    .filter(|p| capture_of(&p.effect) == Some(*capture))
                    .count();
                if publications.len() > 1
                    || held_count > 1
                    || (!local && publications.len() + held_count != 1)
                {
                    return Err("Drake APNAP occurrence disappeared/replayed".into());
                }
            }
            JournalV1::Published { capture, item } => {
                let (original, _) = source(state, *capture)?;
                let collections = refs(
                    &rows,
                    |j| matches!(j, JournalV1::Collected { capture: c, .. } if c == capture),
                );
                if **item != trigger_item(original, *capture, item.v4.stack_item_id)
                    || item.v4.stack_item_id == StackItemId::default()
                    || collections.len() != 1
                    || collections[0].index >= row.index
                {
                    return Err("Drake publication no longer owns collected source".into());
                }
            }
            JournalV1::Debit {
                source,
                kind,
                count,
                before,
                after,
            } => {
                if *kind != SourceCounterKindV1::Incubation
                    || *count == 0
                    || *before < 0
                    || before.checked_sub(i32::from(*count)) != Some(*after)
                    || *after < 0
                    || source.zone != Zone::Battlefield
                {
                    return Err("Drake original typed debit changed".into());
                }
                if !local
                    && refs(
                        &rows,
                        |j| matches!(j, JournalV1::Paid { debit, .. } if *debit == row.index),
                    )
                    .len()
                        != 1
                {
                    return Err("Drake debit lost unique paid publication".into());
                }
            }
            JournalV1::Paid {
                debit,
                original,
                pending: saved,
                item,
            } => {
                let JournalV1::Debit {
                    source,
                    kind,
                    count,
                    before,
                    after,
                } = &at(state, *debit)?.value
                else {
                    return Err("Drake payment debit absent".into());
                };
                if !original.engine.event_history.is_empty()
                    || *debit >= row.index
                    || *kind != SourceCounterKindV1::Incubation
                    || *count != 3
                    || !definition(source.card_def)
                    || **item != activation_item(*source, item.v4.stack_item_id)
                    || item.v4.stack_item_id == StackItemId::default()
                    || saved.source != source.source
                    || saved.controller != source.controller
                    || saved.source_zone_change_count != source.zone_change_count
                    || saved.ability_index != 0
                    || saved.cauldron_grant.0.is_some()
                    || saved.target_spec != TargetSpec::None
                    || !saved.targets_chosen.is_empty()
                    || !saved.target_contracts.is_empty()
                    || !saved.object_cost_chosen.is_empty()
                    || !crate::continuous_characteristics_v1::printed_abilities_active(
                        original,
                        source.source,
                    )
                    || original
                        .objects
                        .try_get(source.source)
                        .is_none_or(|o| o.counters.incubation != *before)
                    || AbilitySourceContractV4::capture(original, source.source) != *source
                    || before.checked_sub(3) != Some(*after)
                {
                    return Err("Drake paid source/counter/input changed".into());
                }
            }
            JournalV1::Removed {
                publication,
                item,
                kind,
                original,
            } => {
                let published = at(state, *publication)?;
                match &published.value {
                    JournalV1::Published { item: saved, .. }
                    | JournalV1::Paid { item: saved, .. }
                        if saved == item => {}
                    _ => return Err("Drake removal lost original publication".into()),
                }
                after_remove(original, item, *kind)?;
                if *publication >= row.index || !original.engine.event_history.is_empty() {
                    return Err("Drake removal input moved/nested".into());
                }
                if !local && refs(&rows, |j| matches!(j, JournalV1::Completed { removed, .. } if *removed == row.index)).len() != 1 {
                    return Err("Drake removal lost unique completion".into());
                }
            }
            JournalV1::Completed { removed, output } => {
                if *removed >= row.index
                    || !matches!(at(state, *removed)?.value, JournalV1::Removed { .. })
                    || !output.engine.event_history.is_empty()
                {
                    return Err("Drake completion lost original removal/output".into());
                }
            }
            JournalV1::Enclosing {
                start,
                original,
                operation,
                answers,
                used_banner,
                output,
            } => {
                if *start > row.index
                    || !original.engine.event_history.is_empty()
                    || !output.engine.event_history.is_empty()
                {
                    return Err("Drake whole operation input/output prefix changed".into());
                }
                let prefix = state
                    .engine
                    .event_history
                    .get(..*start as usize)
                    .ok_or("Drake operation original prefix absent")?;
                if original.engine.drake_ledger_v1.issued
                    != prefix.iter().filter(|e| journal(e).is_some()).count() as u64
                {
                    return Err("Drake operation original ledger changed".into());
                }
                if !verifying {
                    let key = keys.get(&row.index);
                    let hit = key.is_some_and(replay_cached);
                    #[cfg(all(
                        test,
                        feature = "limited-fdn-fixtures",
                        not(feature = "standard-magezero-fixtures")
                    ))]
                    count_replay(hit, key.is_some());
                    if !hit {
                        verification(|| {
                            let input = restore_world(original, prefix)?;
                            let replay = {
                                #[cfg(test)]
                                let _timing = crate::rl_session::test_timing_v1::span(
                                    crate::rl_session::test_timing_v1::Phase::DrakeActualEnclosingReplay,
                                );
                                crate::linked_aura_operation_v1::replay_drake_enclosing_v1(
                                    &input,
                                    operation,
                                    answers,
                                    *used_banner,
                                )?
                            };
                            if replay.engine.event_history.get(*start as usize..)
                                != state
                                    .engine
                                    .event_history
                                    .get(*start as usize..row.index as usize)
                                || world(&replay)? != **output
                            {
                                return Err("Drake complete enclosing original operation/history replay differs".into());
                            }
                            Ok(())
                        })?;
                        if let Some(key) = key {
                            verified_keys.push(*key);
                        }
                    }
                }
            }
        }
        if let JournalV1::Published { item, .. } | JournalV1::Paid { item, .. } = &row.value {
            let removals = refs(
                &rows,
                |j| matches!(j, JournalV1::Removed { publication, .. } if *publication == row.index),
            );
            let lives = state
                .stack
                .iter()
                .filter(|i| i.v4.stack_item_id == item.v4.stack_item_id)
                .collect::<Vec<_>>();
            let live_once = matches!((lives.as_slice(), removals.as_slice()), ([live], []) if *live == item.as_ref());
            if removals.len() > 1
                || !(local || live_once || lives.is_empty() && removals.len() == 1)
            {
                return Err("Drake issued stack row lost/duplicated/replayed".into());
            }
        }
    }
    for p in held(state) {
        if let Some(capture) = capture_of(&p.effect) {
            let (original, _) = source(state, capture)?;
            let mut exact = p.clone();
            exact.placement_ordered = false;
            if exact != pending(original, capture) {
                return Err("Drake held APNAP row changed".into());
            }
        }
    }
    for item in &state.stack {
        validate_item(state, item)?;
    }
    // Even successful pure predicates are not remembered after a later error.
    for key in verified_keys {
        remember_replay(key);
    }
    Ok(())
}

impl Hash for JournalV1 {
    fn hash<H: Hasher>(&self, h: &mut H) {
        serde_json::to_vec(self)
            .expect("typed Drake journal serializes")
            .hash(h);
    }
}
