//! Conqueror-specific mandatory occurrence and actual gain/removal ownership.
use super::*;
use crate::effect::{EffectOp, ExecCtx};
use crate::state::{StackItem, StackItemKind, StackStateV4};
use crate::trigger::PendingTrigger;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum JournalV49 {
    Published {
        capture: u64,
        item: Box<StackItem>,
        world: WorldV49,
    },
    RemovalInput {
        publication: u64,
        item: Box<StackItem>,
        countered: bool,
        world: WorldV49,
    },
    Removed {
        publication: u64,
        input: u64,
        countered: bool,
    },
    GainInput {
        removal: u64,
        context: Box<ExecCtx>,
        world: WorldV49,
    },
    Gain {
        input: u64,
        gain_history_index: u64,
    },
    Closed {
        removal: u64,
        gain: Option<u64>,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WorldV49 {
    history_count: u64,
    bytes: Vec<u8>,
}
impl WorldV49 {
    fn capture(state: &GameState) -> Result<Self, String> {
        let mut original = state.clone();
        let history_count = original.engine.event_history.len() as u64;
        original.engine.event_history.clear();
        Ok(Self {
            history_count,
            bytes: serde_json::to_vec(&original).map_err(|e| e.to_string())?,
        })
    }
    fn restore(&self, state: &GameState, boundary: u64) -> Result<GameState, String> {
        if self.history_count != boundary {
            return Err("Conqueror original input boundary changed".into());
        }
        let prefix = state
            .engine
            .event_history
            .get(..usize::try_from(boundary).map_err(|_| "Conqueror prefix exceeds usize")?)
            .ok_or("Conqueror original input lost history")?;
        let mut original: GameState =
            serde_json::from_slice(&self.bytes).map_err(|e| e.to_string())?;
        if !original.engine.event_history.is_empty() {
            return Err("Conqueror recursive input history".into());
        }
        original.engine.event_history = prefix.to_vec();
        Ok(original)
    }
}
fn append(state: &mut GameState, journal: JournalV49) -> u64 {
    let index = state.engine.event_history.len() as u64;
    state
        .engine
        .event_history
        .push(CommittedEvent::ConquerorJournalV49 {
            journal: Box::new(journal),
        });
    index
}
fn journal(state: &GameState, at: u64) -> Result<&JournalV49, String> {
    match state
        .engine
        .event_history
        .get(usize::try_from(at).map_err(|_| "Conqueror index exceeds usize")?)
    {
        Some(CommittedEvent::ConquerorJournalV49 { journal }) => Ok(journal),
        _ => Err("Conqueror independent original journal absent".into()),
    }
}
pub(crate) fn binding(effect: &EffectOp) -> Option<u64> {
    if let EffectOp::ConquerorGainFromCaptureV49 {
        capture_history_index,
    } = effect
    {
        Some(*capture_history_index)
    } else {
        None
    }
}
fn item_capture(item: &StackItem) -> Option<u64> {
    item.inline_effect.as_ref().and_then(binding)
}
fn context(item: &StackItem) -> ExecCtx {
    let mut ctx = ExecCtx::no_targets(item.source, item.controller);
    ctx.stack_item_id = Some(item.v4.stack_item_id);
    ctx.ability_source_contract = item.v4.ability_source_contract;
    ctx
}
fn pending_matches(
    state: &GameState,
    pending: &PendingTrigger,
    capture: u64,
) -> Result<(), String> {
    let (source, _) = root_at(state, capture)?;
    let expected = PendingTrigger {
        controller: source.controller,
        source: source.source,
        effect: EffectOp::ConquerorGainFromCaptureV49 {
            capture_history_index: capture,
        },
        is_madness_offer: false,
        kicked: false,
        target_spec: TargetSpec::None,
        targets: Vec::new(),
        target_contracts: Vec::new(),
        placement_ordered: pending.placement_ordered,
        source_contract: Some(source),
        granted_by: None,
        optional_additional_cost_paid: None,
        paid_cost_refs: Vec::new(),
    };
    if pending != &expected {
        return Err("Conqueror complete held root changed".into());
    }
    Ok(())
}
fn item_matches(state: &GameState, item: &StackItem, capture: u64) -> Result<(), String> {
    let (source, _) = root_at(state, capture)?;
    let expected = StackItem {
        kind: StackItemKind::TriggeredAbility,
        source: source.source,
        controller: source.controller,
        targets: Vec::new(),
        inline_effect: Some(EffectOp::ConquerorGainFromCaptureV49 {
            capture_history_index: capture,
        }),
        is_copy: false,
        discarded: Vec::new(),
        is_flashback: false,
        mode_chosen: 0,
        madness_offer: false,
        kicked: false,
        v4: StackStateV4 {
            stack_item_id: item.v4.stack_item_id,
            ability_source_contract: Some(source),
            ..Default::default()
        },
    };
    if *item != expected || item.v4.stack_item_id == Default::default() {
        return Err("Conqueror original complete stack item changed".into());
    }
    Ok(())
}
fn unique_live(state: &GameState, item: &StackItem) -> bool {
    state
        .stack
        .iter()
        .filter(|row| row.v4.stack_item_id == item.v4.stack_item_id)
        .count()
        == 1
        && state.stack.iter().any(|row| row == item)
}
pub(crate) fn publish(state: &mut GameState, item: &StackItem) -> Result<(), String> {
    let Some(capture) = item_capture(item) else {
        return Ok(());
    };
    item_matches(state, item, capture)?;
    if !unique_live(state, item) || state.engine.event_history.iter().any(|event| matches!(event,
        CommittedEvent::ConquerorJournalV49 { journal } if matches!(journal.as_ref(), JournalV49::Published { capture: old, .. } if *old == capture))) {
        return Err("Conqueror publication lost or duplicated genuine full row".into());
    }
    let world = WorldV49::capture(state)?;
    append(
        state,
        JournalV49::Published {
            capture,
            item: Box::new(item.clone()),
            world,
        },
    );
    Ok(())
}
fn publication(state: &GameState, at: u64) -> Result<(u64, &StackItem), String> {
    let JournalV49::Published {
        capture,
        item,
        world,
    } = journal(state, at)?
    else {
        return Err("Conqueror original full publication absent".into());
    };
    let original = world.restore(state, at)?;
    if !unique_live(&original, item) {
        return Err("Conqueror publication differs from genuine original owner".into());
    }
    item_matches(&original, item, *capture)?;
    Ok((*capture, item))
}
/// Private nonserializable loan acquired against the unique live original row.
pub(crate) struct OwnerV49 {
    publication: u64,
    original: StackItem,
    world: WorldV49,
}
pub(crate) fn acquire(state: &GameState, item: &StackItem) -> Result<Option<OwnerV49>, String> {
    let Some(capture) = item_capture(item) else {
        return Ok(None);
    };
    validate(state)?;
    item_matches(state, item, capture)?;
    if !unique_live(state, item) {
        return Err("Conqueror loan requires unique actual live item".into());
    }
    let publications = state.engine.event_history.iter().enumerate().filter_map(|(at, event)| match event {
        CommittedEvent::ConquerorJournalV49 { journal } if matches!(journal.as_ref(), JournalV49::Published { capture: c, item: original, .. } if *c == capture && original.as_ref() == item) => Some(at as u64),
        _ => None,
    }).collect::<Vec<_>>();
    let [publication] = publications.as_slice() else {
        return Err("Conqueror loan lost original publication".into());
    };
    Ok(Some(OwnerV49 {
        publication: *publication,
        original: item.clone(),
        world: WorldV49::capture(state)?,
    }))
}
fn removed(
    state: &mut GameState,
    owner: &OwnerV49,
    item: &StackItem,
    countered: bool,
) -> Result<u64, String> {
    if owner.original != *item
        || state
            .stack
            .iter()
            .any(|row| row.v4.stack_item_id == item.v4.stack_item_id)
    {
        return Err("Conqueror genuine private removal owner changed".into());
    }
    if state.engine.event_history.iter().any(|event| matches!(event, CommittedEvent::ConquerorJournalV49 { journal }
        if matches!(journal.as_ref(), JournalV49::Removed { publication, .. } if *publication == owner.publication))) {
        return Err("Conqueror removal replayed".into());
    }
    let input = append(
        state,
        JournalV49::RemovalInput {
            publication: owner.publication,
            item: Box::new(item.clone()),
            countered,
            world: owner.world.clone(),
        },
    );
    Ok(append(
        state,
        JournalV49::Removed {
            publication: owner.publication,
            input,
            countered,
        },
    ))
}
pub(crate) fn counter_removed(
    state: &mut GameState,
    owner: Option<&OwnerV49>,
    item: &StackItem,
) -> Result<(), String> {
    let Some(owner) = owner else {
        if item_capture(item).is_some() {
            return Err("Conqueror counter lost private original owner".into());
        }
        return Ok(());
    };
    let removal = removed(state, owner, item, true)?;
    append(
        state,
        JournalV49::Closed {
            removal,
            gain: None,
        },
    );
    Ok(())
}
pub(crate) fn resolve_owned(
    state: &mut GameState,
    owner: &OwnerV49,
    item: &StackItem,
    ctx: &ExecCtx,
    legal: bool,
) -> Result<(), String> {
    if !legal || *ctx != context(item) {
        return Err("Untargeted Conqueror root cannot fizzle or change captured controller".into());
    }
    let capture = item_capture(item).ok_or("Conqueror bound root absent")?;
    let (source, amount) = root_at(state, capture)?;
    let removal = removed(state, owner, item, false)?;
    let world = WorldV49::capture(state)?;
    let input = append(
        state,
        JournalV49::GainInput {
            removal,
            context: Box::new(ctx.clone()),
            world,
        },
    );
    if let Some(borrow) =
        crate::linked_aura_operation_v1::capture_conqueror_drake_replay_borrow_v49(state)
    {
        state
            .engine
            .event_history
            .push(CommittedEvent::ConquerorGainReplayBorrowV49 {
                input_history_index: input,
                borrow,
            });
    }
    let start = state.engine.event_history.len();
    crate::event::propose_and_commit(
        state,
        crate::event::ProposedEvent::life_gain(source.controller, amount),
    );
    let gains = state.engine.event_history[start..].iter().enumerate().filter_map(|(offset, event)|
        matches!(event, CommittedEvent::LifeGain { player, amount: actual } if *player == source.controller && *actual == amount).then_some((start + offset) as u64)).collect::<Vec<_>>();
    let [gain_history_index] = gains.as_slice() else {
        return Err("Conqueror actual saved-amount gain did not commit exactly once".into());
    };
    let gain = append(
        state,
        JournalV49::Gain {
            input,
            gain_history_index: *gain_history_index,
        },
    );
    append(
        state,
        JournalV49::Closed {
            removal,
            gain: Some(gain),
        },
    );
    Ok(())
}
fn validate_removal(state: &GameState, at: u64) -> Result<(u64, bool, &StackItem), String> {
    let JournalV49::Removed {
        publication: p,
        input,
        countered,
    } = journal(state, at)?
    else {
        return Err("Conqueror removal absent".into());
    };
    let JournalV49::RemovalInput {
        publication: original_p,
        item,
        countered: original_countered,
        world,
    } = journal(state, *input)?
    else {
        return Err("Conqueror removal lost independent actual input".into());
    };
    if *input + 1 != at || original_p != p || original_countered != countered {
        return Err("Conqueror actual removal phase changed".into());
    }
    let replay = world.restore(state, *input)?;
    let (_, published) = publication(&replay, *p)?;
    if item.as_ref() != published || !unique_live(&replay, item) {
        return Err("Conqueror removal input differs from original full live row".into());
    }
    if !*countered && !crate::engine::stack_targets_still_legal(item, &replay)? {
        return Err("Conqueror original untargeted resolution was illegal".into());
    }
    Ok((*p, *countered, item))
}
fn validate_gain(state: &GameState, at: u64) -> Result<u64, String> {
    let JournalV49::Gain {
        input,
        gain_history_index,
    } = journal(state, at)?
    else {
        return Err("Conqueror actual gain operation absent".into());
    };
    let JournalV49::GainInput {
        removal,
        context: original_ctx,
        world,
    } = journal(state, *input)?
    else {
        return Err("Conqueror gain lost independent actual input".into());
    };
    let (_, countered, item) = validate_removal(state, *removal)?;
    if countered
        || *input <= *removal
        || *gain_history_index <= *input
        || *gain_history_index >= at
        || original_ctx.as_ref() != &context(item)
    {
        return Err("Conqueror gain operation context/order changed".into());
    }
    let capture = item_capture(item).ok_or("Conqueror gain original root erased")?;
    let (source, amount) = root_at(state, capture)?;
    if !matches!(state.engine.event_history.get(*gain_history_index as usize), Some(CommittedEvent::LifeGain { player, amount: actual }) if *player == source.controller && *actual == amount)
    {
        return Err("Conqueror actual saved-amount gain occurrence changed".into());
    }
    let mut replay = world.restore(state, *input)?;
    // Exclude this ownership marker from the actual operation replay, then
    // compare its entire produced event sequence, including gain observers.
    let marker = state.engine.event_history[*input as usize].clone();
    replay.engine.event_history.push(marker);
    let sidecars = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(position, event)| {
            if let CommittedEvent::ConquerorGainReplayBorrowV49 {
                input_history_index,
                borrow,
            } = event
            {
                (*input_history_index == *input).then_some((position, borrow, event))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let needs_borrow = crate::drake_hatcher_v1::custody::relevant(&replay);
    let start = match sidecars.as_slice() {
        [(position, borrow, event)]
            if needs_borrow && u64::try_from(*position).ok() == input.checked_add(1) =>
        {
            // This history-only sidecar precedes the genuine operation. It can
            // reinstall a Runner only in this disposable verification world,
            // after the unchanged original Advance/full-row/unique-ID checks.
            replay.engine.event_history.push((*event).clone());
            let start = replay.engine.event_history.len();
            crate::linked_aura_operation_v1::with_panda_replay_borrow_v1(
                &mut replay,
                item,
                borrow,
                |replay| {
                    crate::event::propose_and_commit(
                        replay,
                        crate::event::ProposedEvent::life_gain(source.controller, amount),
                    );
                    Ok(())
                },
            )?;
            start
        }
        [] if !needs_borrow => {
            // The original non-Drake path emits and replays exactly the old bytes.
            let start = replay.engine.event_history.len();
            crate::event::propose_and_commit(
                &mut replay,
                crate::event::ProposedEvent::life_gain(source.controller, amount),
            );
            start
        }
        _ => {
            return Err(
                "Conqueror genuine Drake gain replay sidecar erased/duplicated/shifted".into(),
            )
        }
    };
    if replay.engine.event_history[start..] != state.engine.event_history[start..at as usize] {
        return Err("Conqueror gain and its actual downstream captures changed".into());
    }
    Ok(*removal)
}
pub(crate) fn validate(state: &GameState) -> Result<(), String> {
    if state.engine.event_log.iter().any(|event| {
        matches!(
            event,
            CommittedEvent::ConquerorJournalV49 { .. }
                | CommittedEvent::ConquerorCollectedV49 { .. }
                | CommittedEvent::ConquerorGainReplayBorrowV49 { .. }
        )
    }) {
        return Err("Conqueror private custody entered trigger log".into());
    }
    let waiting = state
        .engine
        .pending_triggers
        .iter()
        .chain(state.legend_pending_v1.iter().flatten())
        .chain(
            state
                .pending_legend_rule_v1
                .iter()
                .flat_map(|pending| pending.waiting_triggers()),
        )
        .chain(
            state
                .standard_v1
                .iter()
                .filter_map(|standard| standard.resolution_play.as_ref())
                .flat_map(|pending| pending.deferred_triggers.iter()),
        );
    let captures = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(at, event)| {
            matches!(event, CommittedEvent::ConquerorCapturedV49 { .. }).then_some(at as u64)
        })
        .collect::<Vec<_>>();
    if captures.is_empty()
        && !state.engine.event_history.iter().any(|event| {
            matches!(
                event,
                CommittedEvent::ConquerorJournalV49 { .. }
                    | CommittedEvent::ConquerorCollectedV49 { .. }
                    | CommittedEvent::ConquerorGainReplayBorrowV49 { .. }
            )
        })
        && !waiting
            .clone()
            .any(|pending| binding(&pending.effect).is_some())
        && !state.stack.iter().any(|item| item_capture(item).is_some())
    {
        return Ok(());
    }
    let waiting = waiting.collect::<Vec<_>>();
    for capture in &captures {
        root_at(state, *capture)?;
        let fresh = state
            .engine
            .event_log
            .iter()
            .filter(|event| **event == state.engine.event_history[*capture as usize])
            .count();
        let collected = state.engine.event_history.iter().enumerate().filter_map(|(at, event)| matches!(event, CommittedEvent::ConquerorCollectedV49 { capture_history_index } if capture_history_index == capture).then_some(at as u64)).collect::<Vec<_>>();
        let held = waiting
            .iter()
            .filter(|pending| binding(&pending.effect) == Some(*capture))
            .collect::<Vec<_>>();
        let publications = state.engine.event_history.iter().enumerate().filter_map(|(at, event)| match event {
            CommittedEvent::ConquerorJournalV49 { journal } if matches!(journal.as_ref(), JournalV49::Published { capture: original, .. } if original == capture) => Some(at as u64), _ => None,
        }).collect::<Vec<_>>();
        if collected.is_empty() {
            if fresh != 1 || !held.is_empty() || !publications.is_empty() {
                return Err("Conqueror fresh mandatory capture erased or duplicated".into());
            }
            continue;
        }
        if collected.len() != 1 || collected[0] <= *capture || fresh != 0 {
            return Err("Conqueror original collection owner changed".into());
        }
        for pending in &held {
            pending_matches(state, pending, *capture)?;
        }
        if publications.is_empty() && held.len() == 1 {
            continue;
        }
        let [published] = publications.as_slice() else {
            return Err("Conqueror held mandatory root disappeared or duplicated".into());
        };
        if !held.is_empty() || *published <= collected[0] {
            return Err("Conqueror publication overlaps held owner".into());
        }
        let (_, item) = publication(state, *published)?;
        let live = state
            .stack
            .iter()
            .filter(|row| row.v4.stack_item_id == item.v4.stack_item_id)
            .collect::<Vec<_>>();
        let removals = state.engine.event_history.iter().enumerate().filter_map(|(at, event)| match event {
            CommittedEvent::ConquerorJournalV49 { journal } if matches!(journal.as_ref(), JournalV49::Removed { publication, .. } if publication == published) => Some(at as u64), _ => None,
        }).collect::<Vec<_>>();
        if removals.is_empty() {
            if live.as_slice() != [item] {
                return Err("Conqueror published complete live owner erased or changed".into());
            }
            continue;
        }
        let [removal] = removals.as_slice() else {
            return Err("Conqueror mandatory root removed twice".into());
        };
        if !live.is_empty() || *removal <= *published {
            return Err("Conqueror removal preceded publication or revived its item".into());
        }
        let (_, countered, _) = validate_removal(state, *removal)?;
        let closures = state
            .engine
            .event_history
            .iter()
            .enumerate()
            .filter_map(|(at, event)| match event {
                CommittedEvent::ConquerorJournalV49 { journal } => match journal.as_ref() {
                    JournalV49::Closed {
                        removal: original,
                        gain,
                    } if original == removal => Some((at as u64, *gain)),
                    _ => None,
                },
                _ => None,
            })
            .collect::<Vec<_>>();
        let [(closed, gain)] = closures.as_slice() else {
            return Err("Conqueror actual completion erased or duplicated".into());
        };
        if countered {
            if gain.is_some() || *closed != *removal + 1 {
                return Err("Conqueror counter manufactured a gain or delayed completion".into());
            }
        } else {
            let gain = gain.ok_or("Conqueror resolved mandatory gain omitted")?;
            if *closed != gain + 1 || validate_gain(state, gain)? != *removal {
                return Err("Conqueror completed gain does not own original removal".into());
            }
        }
    }
    // Every private input/outcome is consumed exactly once. Mutable closure
    // records alone cannot manufacture countering, missing-target or gain.
    for (at, event) in state.engine.event_history.iter().enumerate() {
        if let CommittedEvent::ConquerorCollectedV49 {
            capture_history_index,
        } = event
        {
            if !captures.contains(capture_history_index) {
                return Err("Conqueror collection points to foreign root".into());
            }
        }
        if let CommittedEvent::ConquerorGainReplayBorrowV49 {
            input_history_index,
            ..
        } = event
        {
            if input_history_index.checked_add(1) != Some(at as u64)
                || !matches!(
                    journal(state, *input_history_index)?,
                    JournalV49::GainInput { .. }
                )
                || state
                    .engine
                    .event_history
                    .iter()
                    .filter(|event| {
                        matches!(event,
                    CommittedEvent::ConquerorJournalV49 { journal } if matches!(journal.as_ref(),
                        JournalV49::Gain { input, .. } if input == input_history_index))
                    })
                    .count()
                    != 1
            {
                return Err(
                    "Conqueror replay sidecar lacks its unique adjacent gain input/output".into(),
                );
            }
        }
        let CommittedEvent::ConquerorJournalV49 { journal: entry } = event else {
            continue;
        };
        match entry.as_ref() {
            JournalV49::Published { capture, .. } if !captures.contains(capture) => {
                return Err("Conqueror publication points to foreign capture".into())
            }
            JournalV49::Removed { .. } => {
                validate_removal(state, at as u64)?;
            }
            JournalV49::Closed { removal, .. } => {
                validate_removal(state, *removal)?;
            }
            _ => {}
        }
        let uses = state.engine.event_history.iter().filter(|event| matches!(event, CommittedEvent::ConquerorJournalV49 { journal } if match (entry.as_ref(), journal.as_ref()) {
            (JournalV49::RemovalInput { .. }, JournalV49::Removed { input, .. }) => *input == at as u64,
            (JournalV49::GainInput { .. }, JournalV49::Gain { input, .. }) => *input == at as u64,
            (JournalV49::Gain { .. }, JournalV49::Closed { gain: Some(original), .. }) => *original == at as u64,
            _ => false,
        })).count();
        if matches!(
            entry.as_ref(),
            JournalV49::RemovalInput { .. }
                | JournalV49::GainInput { .. }
                | JournalV49::Gain { .. }
        ) && uses != 1
        {
            return Err("Conqueror actual operation input lost its unique consumer".into());
        }
    }
    for item in &state.stack {
        if let Some(capture) = item_capture(item) {
            if !captures.contains(&capture) {
                return Err("Conqueror live root has no original capture".into());
            }
        }
    }
    for pending in waiting {
        if let Some(capture) = binding(&pending.effect) {
            if !captures.contains(&capture) {
                return Err("Conqueror held root has no original capture".into());
            }
        }
    }
    Ok(())
}
