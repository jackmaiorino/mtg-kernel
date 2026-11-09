//! Private custody for Leyline opening history in disposable FullPool worlds.
use super::{candidates, candidates_for, InputV1};
use crate::state::{GameState, ObjectLinkV4};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct SampledHistoryV1 {
    input: u64,
    retired: [Option<Vec<ObjectLinkV4>>; 2],
}

pub(super) fn validated_candidates(
    state: &GameState,
    at: u64,
    original: &InputV1,
) -> Result<Vec<ObjectLinkV4>, String> {
    let Some(history) = &state.leyline_sampled_history_v1 else {
        return candidates(state, &original.hands);
    };
    if history.input != at {
        return Err("sampled opening custody lost its native input".into());
    }
    let mut result = Vec::new();
    for player in [state.starting_player, state.starting_player.opponent()] {
        let seat = player.index();
        if let Some(retired) = &history.retired[seat] {
            result.extend_from_slice(retired);
        } else {
            result.extend(candidates_for(state, &original.hands[seat])?);
        }
    }
    Ok(result)
}

#[cfg(feature = "limited-fdn-fixtures")]
use super::{input, validate, RecordV1};
#[cfg(feature = "limited-fdn-fixtures")]
use crate::{event::CommittedEvent, ids::PlayerId};

#[cfg(feature = "limited-fdn-fixtures")]
pub(crate) struct FutureSeat {
    input: u64,
    actor: PlayerId,
}

#[cfg(feature = "limited-fdn-fixtures")]
pub(crate) fn prepare(
    state: &mut GameState,
    actor: PlayerId,
) -> Result<Option<FutureSeat>, String> {
    let history = state
        .engine
        .event_history
        .scan_for_v1(&[crate::event_history_v1::EventKindV1::LeylineOpeningV1]);
    let Some(at) = history.iter().position(|event| {
        matches!(
            event,
            CommittedEvent::LeylineOpeningV1 {
                record: RecordV1::Input(_)
            },
        )
    }) else {
        return Ok(None);
    };
    validate(state)?;
    let at = u64::try_from(at).map_err(|_| "opening input index width")?;
    let original = input(state, at)?;
    let retired = if let Some(pending) = state.leyline_opening_v1 {
        let source = original
            .candidates
            .get(pending.next as usize)
            .ok_or("sampled opening lacks an actual current choice")?;
        if state.objects.get(source.object).owner != actor {
            return Err("sampled opening has the wrong current chooser".into());
        }
        if actor == state.starting_player {
            return Ok(Some(FutureSeat { input: at, actor }));
        }
        // Only the starting seat has finished its private choices. The
        // nonstarting actor still owns its unchanged current hand and menu.
        [Some(state.starting_player), None]
    } else {
        // After completion, either seat's old kept cards might now occupy
        // rewritten hidden library/hand slots.
        [Some(PlayerId::P0), Some(PlayerId::P1)]
    };
    let mut custody = state
        .leyline_sampled_history_v1
        .clone()
        .unwrap_or(SampledHistoryV1 {
            input: at,
            retired: [None, None],
        });
    for player in retired.into_iter().flatten() {
        let seat = player.index();
        custody.retired[seat] = Some(
            original
                .candidates
                .iter()
                .copied()
                .filter(|link| original.hands[seat].contains(link))
                .collect(),
        );
    }
    state.leyline_sampled_history_v1 = Some(custody);
    Ok(None)
}

#[cfg(feature = "limited-fdn-fixtures")]
impl FutureSeat {
    pub(crate) fn rebind(self, state: &mut GameState) -> Result<(), String> {
        let original = input(state, self.input)?;
        let own = &original.hands[self.actor.index()];
        let candidates = candidates(state, &original.hands)?;
        if original
            .candidates
            .iter()
            .filter(|link| own.contains(link))
            .ne(candidates.iter().filter(|link| own.contains(link)))
        {
            return Err("sampled opening changed the current chooser's original actions".into());
        }
        let at = usize::try_from(self.input).map_err(|_| "opening input index width")?;
        let Some(CommittedEvent::LeylineOpeningV1 {
            record: RecordV1::Input(original),
        }) = state.engine.event_history.get_mut(at)
        else {
            return Err("sampled opening lost its native input".into());
        };
        original.candidates = candidates;
        validate(state)
    }
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[path = "sampling_tests.rs"]
mod tests;
