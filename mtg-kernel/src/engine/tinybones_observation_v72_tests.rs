//! Paid native histories, complete V72 projection only. No executable authority.
use super::*;
use crate::{policy_surface_v5::PolicySurfaceV5, tinybones_observation_v72 as observation};

fn observe(
    s: &mut GameState,
) -> (
    PolicySurfaceV5,
    crate::policy_surface_v5::PolicyDecisionV5,
    observation::TinybonesObservationV72,
) {
    let mut surface = PolicySurfaceV5::new_with_engine_priority_v1();
    let mut independent = restore(s);
    let mut other_surface = surface.clone();
    let origin = surface.next_decision(s).unwrap();
    let other = other_surface.next_decision(&mut independent).unwrap();
    assert_eq!(origin, other);
    assert_eq!(*s, independent);
    assert_eq!(
        surface.harness_public_context(),
        other_surface.harness_public_context()
    );
    assert_eq!(
        surface.privileged_scan_context().unwrap(),
        other_surface.privileged_scan_context().unwrap()
    );
    let actor = origin.actor(s).unwrap();
    let before = s.clone();
    let value =
        observation::observe_v72(s, &surface, (actor, &origin), 0, 0, origin.substep()).unwrap();
    assert_eq!(*s, before);
    observation::validate_current(
        s,
        &surface,
        (actor, &origin),
        0,
        0,
        origin.substep(),
        &value,
    )
    .unwrap();
    let restored: observation::TinybonesObservationV72 =
        serde_json::from_slice(&serde_json::to_vec(&value).unwrap()).unwrap();
    assert_eq!(restored, value);
    (surface, origin, value)
}

#[test]
fn genuine_paid_stash_complete_eighteen_counter_prefixes_and_readonly_freshness_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (mut s, tiny, card) = setup(p, "Think Twice");
        stashed(&mut s, p, tiny, card);
        // Explicit native storage operands on the genuine paid/stashed history.
        let counters = &mut s.objects.get_mut(tiny).counters;
        counters.stash = i32::MAX;
        counters.fellowship = i32::MAX;
        counters.charge = i32::MAX;
        let (surface, origin, value) = observe(&mut s);
        let actor = origin.actor(&s).unwrap();
        let candidates = crate::tinybones_candidates_v72::project_v72(
            &s,
            &surface,
            (actor, &origin),
            0,
            0,
            origin.substep(),
        )
        .unwrap();
        crate::tinybones_candidates_v72::validate_artifact(&candidates).unwrap();
        assert_eq!(candidates.observation, value);
        assert!(!candidates.owning_consumer_admitted);
        assert!(!candidates.candidates.is_empty());
        let mut unknown = serde_json::to_value(&value).unwrap();
        unknown["common"]["common"]["projection"]["surface"]["battlefield"][p.index()][0]
            ["stable"]["unknown_alias"] = serde_json::json!(true);
        assert!(serde_json::from_value::<observation::TinybonesObservationV72>(unknown).is_err());
        let row = value.tinybones.battlefield[p.index()]
            .iter()
            .find(|r| r.card.object == tiny.0)
            .unwrap();
        assert_eq!(row.counters.stash, i32::MAX);
        assert_eq!(row.counters.fellowship, i32::MAX);
        assert_eq!(row.counters.charge, i32::MAX);
        assert_eq!(
            serde_json::to_value(&row.counters)
                .unwrap()
                .as_object()
                .unwrap()
                .len(),
            18
        );
        assert_eq!(value.tinybones.stash[0].card.object, card.0);
        assert!(crate::muldrotha_observation_v71::require_supported(&s, Some(actor)).is_err());
        assert!(crate::valkyries_call_public_v62::capture(&s, actor).is_err());
        let mut forged = value.clone();
        let prefix = forged.call.battlefield[p.index()]
            .iter_mut()
            .find(|r| r.card.object == tiny.0)
            .unwrap();
        prefix.counters.charge -= 1;
        forged.projection_sha256 = observation::commitment(&forged).unwrap();
        assert!(observation::validate_artifact(&forged).is_err());
        let mut erased = value.clone();
        erased.tinybones.stash.clear();
        erased.projection_sha256 = observation::commitment(&erased).unwrap();
        let before = s.clone();
        assert!(observation::validate_current(
            &s,
            &surface,
            (actor, &origin),
            0,
            0,
            origin.substep(),
            &erased
        )
        .is_err());
        assert_eq!(s, before);
        let mut restored = restore(&s);
        assert_eq!(observe(&mut restored).2, value);
    }
}

#[test]
fn independent_prepaid_hidden_worlds_keep_complete_tinybones_projection_equal_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut values = vec![];
        for hidden in ["Forest", "Imprisoned in the Moon"] {
            let mut s = ready(p, hidden);
            let tiny = put(&mut s, p, NAME, Zone::Hand);
            let card = put(&mut s, p.opponent(), "Think Twice", Zone::Hand);
            paid(&mut s, p, tiny);
            turn(&mut s, p);
            stashed(&mut s, p, tiny, card);
            priority(&mut s, p);
            let value = observe(&mut s).2;
            assert_eq!(value.common.common.acting_player, p.into());
            let json = serde_json::to_string(&value).unwrap();
            for private in [
                "event_history",
                "original_library",
                "remaining_library",
                "move_history_index",
                "play_history_index",
                "stack_item_id",
            ] {
                assert!(!json.contains(private), "{private}");
            }
            values.push(value);
        }
        assert_eq!(values[0], values[1]);
    }
}

#[test]
fn genuine_paid_time_stop_disposes_original_tinybones_trigger_with_archive_restore_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, "Forest");
        let tiny = put(&mut s, p, NAME, Zone::Hand);
        let card = put(&mut s, p.opponent(), "Think Twice", Zone::Hand);
        let stop = put(&mut s, p, "Time Stop", Zone::Hand);
        for _ in 0..4 {
            put(&mut s, p, "Island", Zone::Battlefield);
        }
        paid(&mut s, p, tiny);
        turn(&mut s, p);
        activate_to_discard(&mut s, p, tiny, card);
        act(&mut s, Action::Discard(vec![card]));
        cast(&mut s, p, stop, None);
        assert!(s
            .stack
            .iter()
            .any(|row| matches!(row.inline_effect, Some(EffectOp::TinybonesStashV72 { .. }))));
        assert!(s.stack.iter().any(|row| row.source == stop));
        for _ in 0..512 {
            let decision = next(&mut s);
            if crate::tinybones_public_v72::capture(&s, p)
                .unwrap()
                .triggers
                .iter()
                .any(|row| row.stage == crate::tinybones_public_v72::StageV72::Disposed)
            {
                break;
            }
            pass(&mut s, decision);
        }
        let value = observe(&mut s).2;
        assert!(value
            .tinybones
            .triggers
            .iter()
            .any(|row| row.stage == crate::tinybones_public_v72::StageV72::Disposed));
        assert_eq!(s.objects.get(card).zone, Zone::Graveyard);
        assert_eq!(s.objects.get(card).counters.stash, 0);
        assert!(value
            .end_turn
            .disposals
            .iter()
            .flat_map(|d| &d.disposed_rows)
            .any(|r| r.original.source.arena_id == tiny.0));
        let mut restored = restore(&s);
        assert_eq!(observe(&mut restored).2, value);
    }
}
