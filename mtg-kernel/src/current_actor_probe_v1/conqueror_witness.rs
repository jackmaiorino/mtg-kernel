//! Test-only witness for a game the session halted on Bloodthirsty Conqueror
//! replay custody. It repeats each saved gain replay outside the custody
//! modules, from the same journals and saved worlds, and reports the first
//! event where the replay and the real history differ. It reads the halted
//! world only; the session, its custody checks and the report rule are
//! unchanged.
use crate::bloodthirsty_conqueror_v49::custody::JournalV49;
use crate::event::{CommittedEvent, ProposedEvent};
use crate::linked_aura_operation_v1::PandaReplayBorrowV1;
use crate::snack_wide_life_v59::custody::JournalV59;
use crate::state::GameState;
use serde::Deserialize;

/// Longest event text kept in one witness line.
const EVENT_TEXT: usize = 600;

/// The saved input world of either journal family, read through its own
/// serialized form. Only the V59 world carries a replay borrow.
#[derive(Deserialize)]
struct SavedWorld {
    history_count: u64,
    bytes: Vec<u8>,
    #[serde(default)]
    linked_borrow: Option<PandaReplayBorrowV1>,
}

fn saved(world: &impl serde::Serialize) -> Result<SavedWorld, String> {
    serde_json::to_value(world)
        .and_then(serde_json::from_value)
        .map_err(|error| error.to_string())
}

fn restore(
    state: &GameState,
    world: &SavedWorld,
    boundary: usize,
    parse: impl FnOnce(&[u8]) -> Result<GameState, String>,
) -> Result<GameState, String> {
    if world.history_count != boundary as u64 {
        return Err("saved input boundary changed".into());
    }
    let prefix = state
        .engine
        .event_history
        .get(..boundary)
        .ok_or("saved input lost its history prefix")?;
    let mut original = parse(&world.bytes)?;
    if !original.engine.event_history.is_empty() {
        return Err("saved input world carries its own history".into());
    }
    original.engine.event_history = prefix.to_vec().into();
    Ok(original)
}

fn text(event: Option<&CommittedEvent>) -> String {
    event.map_or_else(
        || "none".into(),
        |event| format!("{event:?}").chars().take(EVENT_TEXT).collect(),
    )
}

/// Compares the replayed events from `start` with the real events in
/// `start..at`; `None` when they are equal.
fn compare(
    family: &str,
    state: &GameState,
    replay: &GameState,
    start: usize,
    at: usize,
    input: usize,
) -> Option<String> {
    let real_len = at.saturating_sub(start);
    let produced_len = replay.engine.event_history.len().saturating_sub(start);
    let real = |k: usize| {
        if k < real_len {
            state.engine.event_history.get(start + k)
        } else {
            None
        }
    };
    let produced = |k: usize| replay.engine.event_history.get(start + k);
    let first = (0..real_len.max(produced_len)).find(|&k| real(k) != produced(k))?;
    Some(format!(
        "{family} gain journal at history index {at} (input {input}): the replay produced \
         {produced_len} events from index {start} and the real history has {real_len}; \
         first difference at history index {}: replay {} real {}",
        start + first,
        text(produced(first)),
        text(real(first)),
    ))
}

fn journal_v49(state: &GameState, at: u64) -> Result<&JournalV49, String> {
    match state.engine.event_history.get(at as usize) {
        Some(CommittedEvent::ConquerorJournalV49 { journal }) => Ok(journal),
        _ => Err(format!("V49 journal absent at history index {at}")),
    }
}

fn journal_v59(state: &GameState, at: u64) -> Result<&JournalV59, String> {
    match state.engine.event_history.get(at as usize) {
        Some(CommittedEvent::ConquerorJournalV59 { journal }) => Ok(journal),
        _ => Err(format!("V59 journal absent at history index {at}")),
    }
}

fn replay_v49(state: &GameState, at: usize, input: u64) -> Result<Option<String>, String> {
    let JournalV49::GainInput { removal, world, .. } = journal_v49(state, input)? else {
        return Err("V49 gain input absent".into());
    };
    let JournalV49::Removed {
        input: removal_input,
        ..
    } = journal_v49(state, *removal)?
    else {
        return Err("V49 removal absent".into());
    };
    let JournalV49::RemovalInput { item, .. } = journal_v49(state, *removal_input)? else {
        return Err("V49 removal input absent".into());
    };
    let capture = item
        .inline_effect
        .as_ref()
        .and_then(crate::bloodthirsty_conqueror_v49::custody::binding)
        .ok_or("V49 gain root erased")?;
    let (source, amount) = crate::bloodthirsty_conqueror_v49::root_at(state, capture)?;
    let input = input as usize;
    let mut replay = restore(state, &saved(world)?, input, |bytes| {
        crate::retained_world_parse_v1::parse_world(bytes).map_err(|error| error.to_string())
    })?;
    replay
        .engine
        .event_history
        .push(state.engine.event_history[input].clone());
    let sidecars = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(position, event)| match event {
            CommittedEvent::ConquerorGainReplayBorrowV49 {
                input_history_index,
                borrow,
            } if *input_history_index == input as u64 => Some((position, borrow, event)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let gain = ProposedEvent::life_gain(source.controller, amount);
    let start = match sidecars.as_slice() {
        [(position, borrow, event)] if *position == input + 1 => {
            replay.engine.event_history.push((*event).clone());
            let start = replay.engine.event_history.len();
            crate::linked_aura_operation_v1::with_panda_replay_borrow_v1(
                &mut replay,
                item,
                borrow,
                |replay| {
                    crate::event::propose_and_commit(replay, gain);
                    Ok(())
                },
            )?;
            start
        }
        [] => {
            let start = replay.engine.event_history.len();
            crate::event::propose_and_commit(&mut replay, gain);
            start
        }
        _ => return Err("V49 replay sidecar erased, duplicated or shifted".into()),
    };
    Ok(compare("V49", state, &replay, start, at, input))
}

fn replay_v59(state: &GameState, at: usize, input: u64) -> Result<Option<String>, String> {
    let JournalV59::GainInput { removal, world, .. } = journal_v59(state, input)? else {
        return Err("V59 gain input absent".into());
    };
    let JournalV59::Removed {
        input: removal_input,
        ..
    } = journal_v59(state, *removal)?
    else {
        return Err("V59 removal absent".into());
    };
    let JournalV59::RemovalInput { item, .. } = journal_v59(state, *removal_input)? else {
        return Err("V59 removal input absent".into());
    };
    let capture = item
        .inline_effect
        .as_ref()
        .and_then(crate::snack_wide_life_v59::custody::binding)
        .ok_or("V59 gain root erased")?;
    let (source, amount) = crate::snack_wide_life_v59::root_at(state, capture)?;
    let world = saved(world)?;
    let borrow = world
        .linked_borrow
        .clone()
        .ok_or("V59 saved world lost its replay borrow")?;
    let input = input as usize;
    let mut replay = restore(state, &world, input, |bytes| {
        serde_json::from_slice(bytes).map_err(|error| error.to_string())
    })?;
    replay
        .engine
        .event_history
        .push(state.engine.event_history[input].clone());
    let start = replay.engine.event_history.len();
    crate::linked_aura_operation_v1::with_panda_replay_borrow_v1(
        &mut replay,
        item,
        &borrow,
        |replay| {
            crate::event::propose_and_commit(
                replay,
                ProposedEvent::life_gain_wide_v59(source.controller, amount),
            );
            Ok(())
        },
    )?;
    Ok(compare("V59", state, &replay, start, at, input))
}

/// One line per gain journal whose replay differs from the real history, or
/// could not run; one summary line when every replay matches.
pub(super) fn witness(state: &GameState) -> Vec<String> {
    let mut lines = Vec::new();
    let mut gains = 0;
    for (at, event) in state.engine.event_history.iter().enumerate() {
        let result = match event {
            CommittedEvent::ConquerorJournalV49 { journal } => match journal.as_ref() {
                JournalV49::Gain { input, .. } => Some(replay_v49(state, at, *input)),
                _ => None,
            },
            CommittedEvent::ConquerorJournalV59 { journal } => match journal.as_ref() {
                JournalV59::Gain { input, .. } => Some(replay_v59(state, at, *input)),
                _ => None,
            },
            _ => None,
        };
        gains += usize::from(result.is_some());
        match result {
            Some(Ok(Some(line))) => lines.push(line),
            Some(Err(error)) => lines.push(format!(
                "gain journal at history index {at}: replay could not run: {error}"
            )),
            Some(Ok(None)) | None => {}
        }
    }
    if lines.is_empty() {
        lines.push(format!(
            "{gains} Conqueror gain journals; every replay matches the real history"
        ));
    }
    lines
}
