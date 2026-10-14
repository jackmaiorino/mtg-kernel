use super::*;
use crate::event::{self, ProposedEvent};
use crate::ids::PlayerId;
#[test]
fn registered_absence_and_original_amount_only_public_facts_do_not_export_private_worlds() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let Some(mut state) = conqueror::tests::amount_only_sampler_state(actor) else {
            continue;
        };
        let before = serde_json::to_vec(&state).unwrap();
        let facts = capture(&state).unwrap();
        validate(&state, &facts).unwrap();
        assert!(facts.meanings.is_empty());
        assert!(facts.occurrences.is_empty());
        assert_eq!(
            facts.losses,
            [LossOccurrenceV49 {
                operation: 0,
                losing_player: actor.opponent().into(),
                actual_amount: 3
            }]
        );
        let json = serde_json::to_value(&facts).unwrap();
        for private in [
            "original_world",
            "observer_world",
            "history_index",
            "event_history",
            "bytes",
        ] {
            assert!(!json.to_string().contains(private));
        }
        for case in 0..4 {
            let mut forged = facts.clone();
            match case {
                0 => forged.producer_sha256.clear(),
                1 => forged.card_db_hash ^= 1,
                2 => forged.losses[0].actual_amount += 1,
                _ => forged.losses[0].losing_player = actor.into(),
            }
            assert!(validate(&state, &forged).is_err());
            assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        }
        // A real hidden draw changes the private hand, not the prior public loss facts.
        event::propose_and_commit(&mut state, ProposedEvent::draw(actor.opponent()));
        assert_eq!(capture(&state).unwrap(), facts);
    }
}
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[test]
#[ignore = "genuine Conqueror registration/native qualification belongs to parent"]
fn genuine_paid_original_public_occurrence_source_and_completion_rederive_before_after_restore() {
    use conqueror::tests::future_printed;
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, source) = future_printed::paid(actor);
        event::propose_and_commit(&mut state, ProposedEvent::life_loss(actor.opponent(), 4));
        let fresh = capture(&state).unwrap();
        assert_eq!(fresh.occurrences.len(), 1);
        assert_eq!(fresh.occurrences[0].phase, PhaseV49::Fresh);
        assert_eq!(fresh.occurrences[0].saved_actual_amount, 4);
        crate::engine::advance_until_decision(&mut state);
        let public = capture(&state).unwrap();
        assert_eq!(public.occurrences[0].phase, PhaseV49::Published);
        let item = state
            .stack
            .iter()
            .find(|item| item.source == source && selected(item))
            .unwrap()
            .clone();
        assert_eq!(
            stack_source_reference(&state, &item).unwrap(),
            Some(public.occurrences[0].source.clone())
        );
        let mut foreign = item.clone();
        foreign.controller = actor.opponent();
        assert!(stack_source_reference(&state, &foreign).is_err());
        let bytes = serde_json::to_vec(&state).unwrap();
        let mut replay = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(capture(&replay).unwrap(), public);
        for current in [&mut state, &mut replay] {
            future_printed::finish(current);
            let facts = capture(current).unwrap();
            assert_eq!(facts.occurrences[0].phase, PhaseV49::Resolved);
            assert_eq!(facts.occurrences[0].actual_gain, Some(4));
            assert_eq!(facts.occurrences[0].source, public.occurrences[0].source);
            let saved = serde_json::to_vec(current).unwrap();
            let mut forged = facts.clone();
            forged.occurrences[0].actual_gain = None;
            assert!(validate(current, &forged).is_err());
            assert_eq!(serde_json::to_vec(current).unwrap(), saved);
        }
        assert_eq!(state, replay);
    }
}
