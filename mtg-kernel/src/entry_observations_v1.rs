//! Arahbo's and Garruk's Uprising's observation checks over the events past a
//! proved prefix of the history.
//!
//! Each of these validators checks, on the whole history: (i) every
//! battlefield entry has exactly one observation; (ii) every observation
//! names an earlier position holding the observed entry; (iii) every
//! (source, root) an observation lists has exactly one capture. A proof at
//! `from` ([`EventHistoryV1::proof_v1`]) says that (i) to (iii) held on the
//! first `from` events and that every capture there named an observation
//! below `from` ([`recordable`]). Then the checks over the whole history fail
//! exactly when one of these holds for the events from `from` on:
//!
//! - (a) a new observation names a battlefield entry below `from`, which
//!   then has two observations;
//! - (b) a new battlefield entry does not have exactly one new observation,
//!   since by (ii) old observations name only positions below `from`;
//! - (c) a new capture names an old observation whose list holds its
//!   (source, root), which then has two captures;
//! - (d) a new observation fails (ii), or one of its (source, root) does not
//!   have exactly one new capture, since by the proof old captures name only
//!   observations below `from`.
//!
//! The full check reports (i) first and then the observations in history
//! order, old before new, and each of its checks has one message. So
//! checking (a) and (b), then (c), then (d) in history order gives its
//! result.

use crate::event::CommittedEvent;
use crate::event_history_v1::{EventHistoryV1, EventKindV1};
use crate::ids::ObjectId;
use crate::state::AbilitySourceContractV4;
use crate::trigger::battlefield_entry_object;
use std::collections::HashMap;
use std::hash::Hash;

/// An observation's entry position, entry object and (source, root) list.
pub(crate) type ObservationV1<'a, R> = (u64, ObjectId, &'a [(AbilitySourceContractV4, R)]);
pub(crate) type ObservedV1<R> = for<'a> fn(&'a CommittedEvent) -> Option<ObservationV1<'a, R>>;
/// A capture's observation position, source and root.
pub(crate) type CapturedV1<R> = fn(&CommittedEvent) -> Option<(u64, AbilitySourceContractV4, R)>;

/// One validator's observation and capture rows, and the messages of its
/// full check.
pub(crate) struct CatalogV1<R: 'static> {
    /// Whether the validator checks (i): its catalog is selected.
    pub(crate) entries: bool,
    pub(crate) capture_kind: EventKindV1,
    pub(crate) observed: ObservedV1<R>,
    pub(crate) captured: CapturedV1<R>,
    pub(crate) lost_entry: &'static str,
    pub(crate) overflow: &'static str,
    pub(crate) lost_actual: &'static str,
    pub(crate) lost_capture: &'static str,
}

/// The full check's result, reading only the events from `from` on and the
/// positions they name, given a proof at `from`.
pub(crate) fn check_since<R: Copy + Eq + Hash + 'static>(
    history: &EventHistoryV1,
    from: usize,
    catalog: &CatalogV1<R>,
) -> Result<(), String> {
    let new = history.range_v2(from..);
    let old = |position: u64| position < from as u64;
    if catalog.entries {
        let mut observations = HashMap::<u64, usize>::new();
        for event in new.iter() {
            if let Some((entry, _, _)) = (catalog.observed)(event) {
                *observations.entry(entry).or_default() += 1;
            }
        }
        // (a)
        if observations.keys().any(|&entry| {
            old(entry)
                && history
                    .get(entry as usize)
                    .and_then(battlefield_entry_object)
                    .is_some()
        }) {
            return Err(catalog.lost_entry.into());
        }
        // (b)
        for (offset, event) in new.iter().enumerate() {
            if battlefield_entry_object(event).is_some()
                && observations
                    .get(&((from + offset) as u64))
                    .copied()
                    .unwrap_or(0)
                    != 1
            {
                return Err(catalog.lost_entry.into());
            }
        }
    }
    let mut captures = HashMap::<(u64, AbilitySourceContractV4, R), usize>::new();
    for event in new.iter() {
        if let Some(key @ (observation, source, root)) = (catalog.captured)(event) {
            // (c)
            if old(observation)
                && history
                    .get(observation as usize)
                    .and_then(catalog.observed)
                    .is_some_and(|(_, _, sources)| sources.contains(&(source, root)))
            {
                return Err(catalog.lost_capture.into());
            }
            *captures.entry(key).or_default() += 1;
        }
    }
    // (d)
    for (offset, event) in new.iter().enumerate() {
        let observation = from + offset;
        if let Some((entry, object, sources)) = (catalog.observed)(event) {
            let original = usize::try_from(entry).map_err(|_| catalog.overflow)?;
            if original >= observation
                || history.get(original).and_then(battlefield_entry_object) != Some(object)
            {
                return Err(catalog.lost_actual.into());
            }
            for (source, root) in sources {
                if captures
                    .get(&(observation as u64, *source, *root))
                    .copied()
                    .unwrap_or(0)
                    != 1
                {
                    return Err(catalog.lost_capture.into());
                }
            }
        }
    }
    Ok(())
}

/// Whether every capture from `from` on names an observation below the
/// current length: what a proof at the current length records beside a
/// passing check, given one at `from`.
pub(crate) fn recordable<R: 'static>(
    history: &EventHistoryV1,
    from: usize,
    catalog: &CatalogV1<R>,
) -> bool {
    let len = history.len() as u64;
    let named_earlier = |event: &CommittedEvent| {
        (catalog.captured)(event).is_none_or(|(observation, _, _)| observation < len)
    };
    if from == 0 {
        history
            .of_kinds_v2(&[catalog.capture_kind])
            .all(|(_, event)| named_earlier(event))
    } else {
        history.range_v2(from..).iter().all(named_earlier)
    }
}

/// Test-only: the violations a later event can add after a proof.
#[cfg(test)]
pub(crate) mod tests_v1 {
    use super::*;
    use crate::event_history_v1::{proof_tests_v1, ProvedV1};
    use crate::state::GameState;

    /// Validates `state`, which must hold an observation that lists a
    /// (source, root) and its capture, and checks that the validation left a
    /// proof. Then, after each violation pushed past that proof, checks that
    /// the validation used the proof and failed with the full check's
    /// message (the test build also asserts that the full check agrees).
    /// `retarget` sets an observation's entry position.
    pub(crate) fn violations_after_proof<R: Copy + Eq + Hash + 'static>(
        state: &GameState,
        validator: ProvedV1,
        validate: fn(&GameState) -> Result<(), String>,
        catalog: &CatalogV1<R>,
        retarget: fn(&mut CommittedEvent, u64),
    ) {
        validate(state).unwrap();
        let history = &state.engine.event_history;
        let len = history.len();
        assert_eq!(history.proof_v1(validator), Some((len, 0)));
        let observation = history
            .iter()
            .rev()
            .find(|event| {
                (catalog.observed)(event).is_some_and(|(_, _, sources)| !sources.is_empty())
            })
            .expect("an observation that lists a (source, root)")
            .clone();
        let (entry, _, _) = (catalog.observed)(&observation).unwrap();
        let entry = history[entry as usize].clone();
        let capture = history
            .iter()
            .rev()
            .find(|event| (catalog.captured)(event).is_some())
            .expect("a capture")
            .clone();
        let pushed = |events: &[&CommittedEvent]| {
            let mut bad = state.clone();
            for event in events {
                bad.engine.event_history.push((*event).clone());
            }
            let uses = proof_tests_v1::uses();
            let result = validate(&bad);
            assert_eq!(proof_tests_v1::uses(), uses + 1, "the proof was used");
            result.unwrap_err()
        };
        // (a) a second observation of an old entry.
        assert_eq!(pushed(&[&observation]), catalog.lost_entry);
        // (b) a new entry without an observation.
        assert_eq!(pushed(&[&entry]), catalog.lost_entry);
        // (c) a second capture of an old observation's (source, root).
        assert_eq!(pushed(&[&capture]), catalog.lost_capture);
        // (d) a new observation of a new entry, with no capture...
        let mut later = observation.clone();
        retarget(&mut later, len as u64);
        assert_eq!(pushed(&[&entry, &later]), catalog.lost_capture);
        // ...and one naming a later position.
        retarget(&mut later, len as u64 + 5);
        assert_eq!(pushed(&[&later]), catalog.lost_actual);
        // The entry check comes first, then the old observations, then the
        // new ones.
        assert_eq!(pushed(&[&capture, &observation]), catalog.lost_entry);
        assert_eq!(pushed(&[&capture, &later]), catalog.lost_capture);
    }
}
