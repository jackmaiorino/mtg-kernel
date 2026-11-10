//! Echo shuffles both libraries before its two ordered draw instructions.

use super::{
    completed_attempts, has_started, record, start_instruction, InstructionV1, JournalV1, SourceV1,
    StartedV1,
};
use crate::card_def::CARD_DEFS;
use crate::effect::{EffectContinuation, EffectFrame};
use crate::environment_randomization_v2::PhysicalOwnerV2;
use crate::eternal_legacy_e7a_v1;
use crate::eternal_vintage_e7b_v1::dredge;
use crate::event::{self, CommittedEvent};
use crate::ids::PlayerId;
use crate::state::{GameState, StackItemKind, StackSourceContractV4, Zone};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShuffleCommitV1 {
    player: PlayerId,
    moves_start: usize,
    history_after_moves: usize,
    ordinal_after: Option<u64>,
}

fn shuffle_ordinal(state: &GameState, player: PlayerId) -> Option<u64> {
    let owner = match player {
        PlayerId::P0 => PhysicalOwnerV2::P0,
        PlayerId::P1 => PhysicalOwnerV2::P1,
        _ => return None,
    };
    state
        .environment_randomization_v2()
        .map(|rng| rng.next_live_shuffle_ordinal(owner))
}

/// The checked shuffle has no generic event row. Capture its actual commit
/// boundary here, retaining the proof only if Echo needs an owned draw stage.
pub(crate) fn commit_shuffle(
    state: &mut GameState,
    player: PlayerId,
    moves_start: usize,
) -> Result<ShuffleCommitV1, String> {
    state
        .shuffle_library(player)
        .map_err(|error| error.to_string())?;
    Ok(ShuffleCommitV1 {
        player,
        moves_start,
        history_after_moves: state.engine.event_history.len(),
        ordinal_after: shuffle_ordinal(state, player),
    })
}

pub(super) fn selected(
    state: &GameState,
    pending: &EffectContinuation,
) -> Option<StackSourceContractV4> {
    let item = &pending.resolving_item;
    let source = item.v4.source_contract?;
    let def = CARD_DEFS.get(usize::from(source.card_def))?;
    let finalized = if item.is_copy {
        source.spell_copy_origin.is_some() && source.spell_cast_origin.is_none()
    } else {
        source.spell_copy_origin.is_none()
            && source
                .spell_cast_origin
                .is_some_and(|origin| origin.finalized_method == Some(source.cast_method))
    };
    if item.kind != StackItemKind::Spell
        || !finalized
        || item.inline_effect.is_some()
        || !item.targets.is_empty()
        || source.zone != Zone::Stack
        || source.source != item.source
        || source.controller != item.controller
        || Some(source.cast_method) != item.v4.cast_method
        || def.object_name != "Echo of Eons"
        || def.printed_mode_effect(item.mode_chosen) != Some(eternal_legacy_e7a_v1::echo_of_eons())
        || pending.ctx.source != item.source
        || pending.ctx.controller != item.controller
        || pending.ctx.stack_item_id != Some(item.v4.stack_item_id)
    {
        return None;
    }
    state.objects.try_get(item.source)?;
    (StackSourceContractV4::capture(state, item.source, source.cast_method) == source)
        .then_some(source)
}

pub(super) fn before_start(pending: &EffectContinuation) -> bool {
    matches!(
        (&pending.choice, pending.frames.as_slice()),
        (None, [EffectFrame::Program { op, path }])
            if path.is_empty() && *op == eternal_legacy_e7a_v1::echo_of_eons()
    )
}

pub(crate) fn begin(
    state: &mut GameState,
    pending: &mut EffectContinuation,
    path: &[u16],
) -> Result<(), String> {
    let source = selected(state, pending).ok_or("Echo of Eons lost its original spell")?;
    let item = pending.resolving_item.v4.stack_item_id;
    if !path.is_empty()
        || !pending.frames.is_empty()
        || state.eternal_owned_draw_progress_v1.is_some()
        || state.stack.iter().any(|row| row.v4.stack_item_id == item)
        || has_started(state, item)
    {
        return Err("Echo of Eons cannot replay its shuffle".into());
    }
    let first = state.active_player;
    let commits = eternal_legacy_e7a_v1::shuffle_echo_libraries(state)?;
    if ![first, first.opponent()]
        .into_iter()
        .any(|drawer| dredge::owned_draw_needs_choice(state, drawer, 7))
    {
        eternal_legacy_e7a_v1::draw_echo_cards(state);
        return Ok(());
    }
    let shuffles = commits.map(|commit| {
        let at = state.engine.event_history.len();
        record(state, JournalV1::EchoShuffleCommittedV1 { item, commit });
        at
    });
    let shuffled_at = state.engine.event_history.len();
    let first_extra = event::draw_instruction_extra(state, first, 7);
    record(
        state,
        JournalV1::EchoShuffledV1 {
            item,
            source,
            first,
            shuffles,
            first_extra,
        },
    );
    start_instruction(
        state,
        pending,
        InstructionV1::Echo {
            shuffled_at,
            drawer: first,
        },
        SourceV1::Spell(source),
        7,
        first_extra,
    );
    Ok(())
}

pub(super) fn start_draws(
    state: &mut GameState,
    pending: &mut EffectContinuation,
    shuffled_at: usize,
    drawer: PlayerId,
    source: SourceV1,
) {
    let extra = event::draw_instruction_extra(state, drawer, 7);
    start_instruction(
        state,
        pending,
        InstructionV1::Echo {
            shuffled_at,
            drawer,
        },
        source,
        7,
        extra,
    );
}

fn journal(state: &GameState, at: usize) -> Option<JournalV1> {
    let CommittedEvent::EternalOwnedDrawV1 { journal } = state.engine.event_history.get(at)? else {
        return None;
    };
    Some(**journal)
}

pub(super) fn matches_start(
    state: &GameState,
    pending: &EffectContinuation,
    started: StartedV1,
    started_at: usize,
) -> bool {
    let InstructionV1::Echo {
        shuffled_at,
        drawer,
    } = started.instruction
    else {
        return false;
    };
    let Some(JournalV1::EchoShuffledV1 {
        item,
        source,
        first,
        shuffles,
        first_extra,
    }) = journal(state, shuffled_at)
    else {
        return false;
    };
    if item != started.item
        || started.source != SourceV1::Spell(source)
        || Some(source) != selected(state, pending)
        || first != state.active_player
        || !matches!(first, PlayerId::P0 | PlayerId::P1)
    {
        return false;
    }
    let [first_shuffle, second_shuffle] = shuffles;
    if first_shuffle.checked_add(1) != Some(second_shuffle)
        || second_shuffle.checked_add(1) != Some(shuffled_at)
    {
        return false;
    }
    let [Some(JournalV1::EchoShuffleCommittedV1 {
        item: first_item,
        commit: first_commit,
    }), Some(JournalV1::EchoShuffleCommittedV1 {
        item: second_item,
        commit: second_commit,
    })] = shuffles.map(|at| journal(state, at))
    else {
        return false;
    };
    if first_item != item
        || second_item != item
        || first_commit.player != first
        || second_commit.player != first.opponent()
        || first_commit.moves_start > first_commit.history_after_moves
        || first_commit.history_after_moves != second_commit.moves_start
        || second_commit.moves_start > second_commit.history_after_moves
        || second_commit.history_after_moves != first_shuffle
        || first_commit.ordinal_after != shuffle_ordinal(state, first)
        || second_commit.ordinal_after != shuffle_ordinal(state, first.opponent())
    {
        return false;
    }
    let first_at = shuffled_at + 1;
    let Some(expected_first_count) = 7u32.checked_add(first_extra) else {
        return false;
    };
    if drawer == first {
        return started_at == first_at
            && journal(state, first_at)
                .and_then(JournalV1::start)
                .map(|(_, count)| count)
                == Some(expected_first_count);
    }
    if drawer != first.opponent() || started_at <= first_at {
        return false;
    }
    let Some((first_start, first_count)) = journal(state, first_at).and_then(JournalV1::start)
    else {
        return false;
    };
    let first_leg = InstructionV1::Echo {
        shuffled_at,
        drawer: first,
    };
    first_start.instruction == first_leg
        && first_start.item == started.item
        && first_start.source == started.source
        && first_start.controller == started.controller
        && first_start.total == 7
        && first_count == expected_first_count
        && first_start.initial_library_len.is_none()
        && completed_attempts(
            &state.engine.event_history[first_at + 1..started_at],
            first_at,
        ) == Some(first_count)
        && journal(state, started_at - 1)
            == Some(JournalV1::Completed {
                started_at: first_at,
                ordinal: first_count - 1,
            })
}
