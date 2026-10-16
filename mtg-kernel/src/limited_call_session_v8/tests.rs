//! Native-backed source wire fixtures, not real-process/runtime qualification.
use super::*;
use crate::{
    call_observation_v62::tests::{hand, paid_scene, ready},
    ids::PlayerId,
    rl::ActionSemanticV1 as A,
    rl_session::FastActorSessionV1,
    state::{GameState, Zone},
};
use serde_json::{json, Value};
fn send(s: &mut LimitedCallJsonlServerV8, seq: &mut u64, mut v: Value) -> Value {
    *seq += 1;
    v["schema_version"] = 8.into();
    v["request_id"] = format!("call{seq}").into();
    serde_json::from_str(&s.handle_line(&v.to_string())).unwrap()
}
fn fixture(state: GameState) -> (LimitedCallJsonlServerV8, FastActorSessionV1) {
    let native = FastActorSessionV1::from_call_v62_fixture_state(state.clone());
    let mut s = LimitedCallJsonlServerV8::new();
    s.active = Some(Active {
        core: CallSessionV62::from_session(FastActorSessionV1::from_call_v62_fixture_state(state))
            .unwrap(),
        owner: Arc::new(()),
    });
    s.generation = 1;
    (s, native)
}
fn world(s: &LimitedCallJsonlServerV8) -> &GameState {
    s.active.as_ref().unwrap().core.fixture_state_v62()
}
fn step(
    s: &mut LimitedCallJsonlServerV8,
    native: &mut FastActorSessionV1,
    seq: &mut u64,
    choose: impl Fn(&A) -> bool,
) -> Value {
    let current = send(s, seq, json!({"request_type":"current"}));
    assert_eq!(current["state"]["state_type"], "decision");
    assert_eq!(
        current["state"]["decision"]["schema"],
        "mtg-kernel-limited-call-decision/v8"
    );
    let bound = s.pending.as_ref().unwrap();
    let index = bound.decision.actions().iter().position(choose).unwrap();
    let raw = bound
        .decision
        .fixture_original_index_v62(index as u32)
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
    assert_eq!(world(s), native.call_fixture_state());
    assert_eq!(
        s.active.as_ref().unwrap().core.fixture_counts_v62(),
        (native.policy_step_count(), native.physical_decision_count())
    );
    reply
}
#[test]
fn genuine_paid_call_wire_capture_original_index_restore_and_failed_partial_reset_are_atomic_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(p, "Forest");
        let call = hand(&mut state, p, "Valkyrie's Call");
        let (mut wire, mut native) = fixture(state);
        let mut seq = 0;
        let reply = step(
            &mut wire,
            &mut native,
            &mut seq,
            |a| matches!(a,A::CastSpell{source,..}if source.arena_id==call.0),
        );
        assert_eq!(world(&wire).objects.get(call).zone, Zone::Stack);
        assert!(world(&wire).engine.pending_cast.is_none());
        assert_eq!(world(&wire).players[p.index()].mana_pool, [0; 6]);
        assert_eq!(
            reply["state"]["decision"]["state"]["call"]["meaning"]["card_id"],
            459
        );
        let snapshot = send(&mut wire, &mut seq, json!({"request_type":"snapshot"}));
        let reference = native.snapshot_v1();
        let before = world(&wire).clone();
        let prompt = wire.pending.as_ref().unwrap().visible.clone();
        let generation = wire.generation;
        let reset = json!({"request_type":"reset","expected_card_db_hash":KERNEL_CARDDB_HASH,"episode_id":77,"env_seed":123,"starting_player":"p0","decks":[{"cards":[{"name":"Valkyrie's Call","count":40}]},{"cards":[{"name":"Forest","count":40}]}],"max_physical_decisions":1000,"max_policy_steps":1000});
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
        native.call_restore_v62(&reference).unwrap();
        assert_eq!(world(&wire), native.call_fixture_state());
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
fn independent_initial_hidden_worlds_paid_call_wire_visible_state_and_meanings_match_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (a, _, _) = paid_scene(p, "Forest");
        let (b, _, _) = paid_scene(p, "Counterspell");
        let (mut a, _) = fixture(a);
        let (mut b, _) = fixture(b);
        let (mut sa, mut sb) = (0, 0);
        let aview = send(&mut a, &mut sa, json!({"request_type":"current"}));
        let bview = send(&mut b, &mut sb, json!({"request_type":"current"}));
        assert_eq!(aview["state"]["decision"], bview["state"]["decision"]);
        assert_ne!(world(&a).state_hash(), world(&b).state_hash());
    }
}
