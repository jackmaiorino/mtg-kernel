//! Native-backed source wire fixtures, not real-process/runtime qualification.
use super::*;
use crate::{
    call_observation_v62::tests::{hand, ready},
    ids::PlayerId,
    rl::ActionSemanticV1 as A,
    rl_session::FastActorSessionV1,
    state::{GameState, Zone},
};
use serde_json::{json, Value};
fn send(s: &mut LimitedEndTurnJsonlServerV9, seq: &mut u64, mut v: Value) -> Value {
    *seq += 1;
    v["schema_version"] = 9.into();
    v["request_id"] = format!("call{seq}").into();
    serde_json::from_str(&s.handle_line(&v.to_string())).unwrap()
}
fn fixture(state: GameState) -> (LimitedEndTurnJsonlServerV9, FastActorSessionV1) {
    let native = FastActorSessionV1::from_end_turn_v68_fixture_state(state.clone());
    let mut s = LimitedEndTurnJsonlServerV9::new();
    s.active = Some(Active {
        core: EndTurnSessionV68::from_session(FastActorSessionV1::from_end_turn_v68_fixture_state(
            state,
        ))
        .unwrap(),
        owner: Arc::new(()),
    });
    s.generation = 1;
    (s, native)
}
fn world(s: &LimitedEndTurnJsonlServerV9) -> &GameState {
    s.active.as_ref().unwrap().core.fixture_state_v68()
}
fn step(
    s: &mut LimitedEndTurnJsonlServerV9,
    native: &mut FastActorSessionV1,
    seq: &mut u64,
    choose: impl Fn(&A) -> bool,
) -> Value {
    let current = send(s, seq, json!({"request_type":"current"}));
    assert_eq!(current["state"]["state_type"], "decision");
    assert_eq!(
        current["state"]["decision"]["schema"],
        "mtg-kernel-limited-end-turn-decision/v9"
    );
    let bound = s.pending.as_ref().unwrap();
    let index = bound.decision.actions().iter().position(choose).unwrap();
    let raw = bound
        .decision
        .fixture_original_index_v68(index as u32)
        .unwrap();
    let expected = bound.expected;
    let visible = bound
        .indexes
        .iter()
        .position(|&v| v as usize == index)
        .unwrap();
    let request = json!({"request_type":"step","episode_id":expected.episode_id,"authority_generation":s.generation,"actor":expected.acting_player,"prompt_seq":bound.visible.prompt_seq,"action_index":visible});
    let reply = send(s, seq, request);
    assert_eq!(reply["response_type"], "state");
    native
        .step(expected.episode_id, expected.step, raw)
        .unwrap();
    assert_eq!(world(s), native.end_turn_fixture_state());
    assert_eq!(
        s.active.as_ref().unwrap().core.fixture_counts_v68(),
        (native.policy_step_count(), native.physical_decision_count())
    );
    reply
}
#[test]
fn genuine_paid_time_stop_wire_capture_original_index_restore_and_failed_partial_reset_are_atomic_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(p, "Forest");
        let call = hand(&mut state, p, "Time Stop");
        let (mut wire, mut native) = fixture(state);
        let mut seq = 0;
        let before_taps = crate::engine::time_stop_v1::tests::tapped(world(&wire), p);
        let reply = step(
            &mut wire,
            &mut native,
            &mut seq,
            |a| matches!(a,A::CastSpell{source,..}if source.arena_id==call.0),
        );
        assert_eq!(
            crate::engine::time_stop_v1::tests::tapped(world(&wire), p) - before_taps,
            6
        );
        assert_eq!(world(&wire).objects.get(call).zone, Zone::Stack);
        assert!(world(&wire).engine.pending_cast.is_none());
        assert_eq!(world(&wire).players[p.index()].mana_pool, [0; 6]);
        assert_eq!(
            reply["state"]["decision"]["state"]["end_turn"]["meaning"]["card_id"],
            486
        );
        let snapshot = send(&mut wire, &mut seq, json!({"request_type":"snapshot"}));
        let reference = native.snapshot_v1();
        let before = world(&wire).clone();
        let prompt = wire.pending.as_ref().unwrap().visible.clone();
        let generation = wire.generation;
        let reset = json!({"request_type":"reset","expected_card_db_hash":KERNEL_CARDDB_HASH,"episode_id":77,"env_seed":123,"starting_player":"p0","decks":[{"cards":[{"name":"Time Stop","count":40}]},{"cards":[{"name":"Forest","count":40}]}],"max_physical_decisions":1000,"max_policy_steps":1000});
        let error = send(&mut wire, &mut seq, reset);
        assert_eq!(error["response_type"], "error");
        assert_eq!(error["code"], "unsupported_deck");
        assert_eq!(world(&wire), &before);
        assert_eq!(wire.generation, generation);
        assert_eq!(wire.pending.as_ref().unwrap().visible, prompt);
        step(&mut wire, &mut native, &mut seq, |a| {
            matches!(a, A::Pass { .. })
        });
        let restored = send(
            &mut wire,
            &mut seq,
            json!({"request_type":"restore","snapshot_id":snapshot["snapshot_id"]}),
        );
        assert_eq!(restored["response_type"], "state");
        native.end_turn_restore_v68(&reference).unwrap();
        assert_eq!(world(&wire), native.end_turn_fixture_state());
        assert_eq!(world(&wire), &before);
        assert!(wire.generation > generation);
        let stale = send(
            &mut wire,
            &mut seq,
            json!({"request_type":"step","episode_id":reply["state"]["episode_id"],"authority_generation":generation,"actor":PlayerSeatV1::from(p),"prompt_seq":prompt.prompt_seq,"action_index":0}),
        );
        assert_eq!(stale["response_type"], "error");
        assert_eq!(world(&wire), &before);
    }
}

#[test]
fn genuine_paid_time_stop_wire_disposal_cleanup_choices_and_midprompt_restore_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(p, "Forest");
        let stop = hand(&mut state, p, "Time Stop");
        for _ in 0..12 {
            hand(&mut state, p, "Forest");
        }
        let (mut wire, mut native) = fixture(state);
        let mut seq = 0;
        step(
            &mut wire,
            &mut native,
            &mut seq,
            |a| matches!(a,A::CastSpell{source,..}if source.arena_id==stop.0),
        );
        for _ in 0..80 {
            let current = send(&mut wire, &mut seq, json!({"request_type":"current"}));
            if current["state"]["decision"]["state"]["end_turn"]["cleanup"]["phase"] == "Discard" {
                break;
            }
            step(&mut wire, &mut native, &mut seq, |a| {
                matches!(a, A::Pass { .. })
            });
        }
        let current = send(&mut wire, &mut seq, json!({"request_type":"current"}));
        let end = &current["state"]["decision"]["state"]["end_turn"];
        assert_eq!(end["cleanup"]["discard_count"], 5);
        assert_eq!(end["disposals"].as_array().unwrap().len(), 1);
        assert_eq!(
            end["disposals"][0]["disposed_rows"][0]["original"]["source"]["name"],
            "Time Stop"
        );
        let snapshot = send(&mut wire, &mut seq, json!({"request_type":"snapshot"}));
        let saved = native.snapshot_v1();
        let before = world(&wire).clone();
        step(&mut wire, &mut native, &mut seq, |a| {
            matches!(a, A::Discard { .. })
        });
        let restored = send(
            &mut wire,
            &mut seq,
            json!({"request_type":"restore","snapshot_id":snapshot["snapshot_id"]}),
        );
        native.end_turn_restore_v68(&saved).unwrap();
        assert_eq!(world(&wire), native.end_turn_fixture_state());
        assert_eq!(world(&wire), &before);
        assert_eq!(
            restored["state"]["decision"]["state"]["end_turn"],
            current["state"]["decision"]["state"]["end_turn"]
        );
    }
}
