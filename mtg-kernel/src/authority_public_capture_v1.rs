//! Private event-time visibility, linked to the independent entry journal.
//! No current hidden card definition is used to reconstruct historical facts.
use crate::event::CommittedEvent;
use crate::rl::CardStableRefV1;
use crate::state::{GameObject, GameState, Zone};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EntryPublicFactsV1 {
    pub(crate) observer_authority_index: u64,
    pub(crate) entrant: Option<CardStableRefV1>,
    pub(crate) entrant_face_index: u8,
    // Private snapshot authenticates masking, never leaves the engine history.
    pub(crate) entrant_snapshot: String,
}

pub(crate) fn record_entry(state: &mut GameState, observer_authority_index: u64) {
    let CommittedEvent::AuthorityEntryObserversV1 { entrant, .. } =
        &state.engine.event_history[observer_authority_index as usize]
    else {
        unreachable!("owned entry journal")
    };
    let object = state.objects.get(entrant.object);
    let capture = EntryPublicFactsV1 {
        observer_authority_index,
        entrant: crate::sower_restriction_capture_v1::masked_ref(entrant.object, object).ok(),
        entrant_face_index: if object.v4.face_down_v1.is_some() {
            0
        } else {
            object.v4.face_index
        },
        entrant_snapshot: serde_json::to_string(object).expect("serializable object"),
    };
    state
        .engine
        .event_history
        .push(CommittedEvent::AuthorityEntryPublicFactsV1 { capture });
}

pub(crate) fn facts_at(
    state: &GameState,
    observer_index: u64,
) -> Result<&EntryPublicFactsV1, String> {
    let index = usize::try_from(observer_index).map_err(|_| "entry index range")?;
    let Some(CommittedEvent::AuthorityEntryObserversV1 {
        entrant,
        entrant_controller,
        ..
    }) = state.engine.event_history.get(index)
    else {
        return Err("public entrant lost original observer journal".into());
    };
    // One adjacent immutable publication per original entry, independent of
    // mutable dispatch rows, source survival, event-log drainage or completion.
    let Some(CommittedEvent::AuthorityEntryPublicFactsV1 { capture }) =
        state.engine.event_history.get(index + 1)
    else {
        return Err("public entrant snapshot missing".into());
    };
    if capture.observer_authority_index != observer_index || state.engine.event_history.iter().filter(|event| {
        matches!(event, CommittedEvent::AuthorityEntryPublicFactsV1 { capture } if capture.observer_authority_index == observer_index)
    }).count() != 1 { return Err("public entrant snapshot repeated or redirected".into()) }
    let snapshot: GameObject = serde_json::from_str(&capture.entrant_snapshot)
        .map_err(|_| "invalid private entrant snapshot")?;
    if snapshot.zone != Zone::Battlefield
        || snapshot.zone_change_count != entrant.zone_change_count
        || snapshot.controller != *entrant_controller
        || snapshot.card_def as usize >= crate::card_def::CARD_DEFS.len()
        || capture.entrant_face_index
            != if snapshot.v4.face_down_v1.is_some() {
                0
            } else {
                snapshot.v4.face_index
            }
        || state
            .objects
            .try_get(entrant.object)
            .is_none_or(|live| live.owner != snapshot.owner)
        || capture.entrant
            != crate::sower_restriction_capture_v1::masked_ref(entrant.object, &snapshot).ok()
    {
        return Err("public entrant changed captured occurrence or masking".into());
    }
    if let Some(CommittedEvent::CreateToken {
        token_def,
        controller,
        ..
    }) = state
        .engine
        .event_history
        .get(match &state.engine.event_history[index] {
            CommittedEvent::AuthorityEntryObserversV1 {
                entry_history_index,
                ..
            } => usize::try_from(*entry_history_index).map_err(|_| "entry index range")?,
            _ => unreachable!(),
        })
    {
        if snapshot.card_def != *token_def || snapshot.controller != *controller {
            return Err("public entrant changed its committed token definition".into());
        }
    }
    Ok(capture)
}

pub(crate) fn validate_facts(state: &GameState) -> Result<(), String> {
    for (index, event) in state.engine.event_history.iter().enumerate() {
        match event {
            CommittedEvent::AuthorityEntryObserversV1 { .. } => {
                facts_at(state, index as u64)?;
            }
            CommittedEvent::AuthorityEntryPublicFactsV1 { capture } => {
                if facts_at(state, capture.observer_authority_index)? != capture {
                    return Err("orphan public entrant snapshot".into());
                }
            }
            _ => {}
        }
    }
    if state
        .engine
        .event_log
        .iter()
        .any(|event| matches!(event, CommittedEvent::AuthorityEntryPublicFactsV1 { .. }))
    {
        return Err("private entrant snapshot injected into mutable log".into());
    }
    Ok(())
}

pub(crate) fn validate_state(state: &GameState) -> Result<(), String> {
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) && state.engine.event_history.iter().any(|event| {
        matches!(
            event,
            CommittedEvent::AuthorityEntryObserversV1 { .. }
                | CommittedEvent::AuthorityEntryCaptureV1 { .. }
                | CommittedEvent::AuthorityEntryCollectedV1 { .. }
                | CommittedEvent::AuthorityEntryPublishedV1 { .. }
                | CommittedEvent::AuthorityEntryOperationV1 { .. }
                | CommittedEvent::AuthorityEntryCompletedV1 { .. }
                | CommittedEvent::AuthorityEntryPublicFactsV1 { .. }
        )
    }) {
        return Err("Authority journal requires its selected Limited catalog".into());
    }
    crate::authority_of_the_consuls_v1::validate_state(state)?;
    validate_facts(state)
}
