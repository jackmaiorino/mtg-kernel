//! Native-backed source wire fixtures, not real-process/runtime qualification.
use super::*;
use crate::{
    ids::PlayerId, rl::ActionSemanticV1 as A, rl_session::vivien_v70::tests::ready,
    rl_session::FastActorSessionV1, state::GameState,
};
use serde_json::{json, Value};
fn send(s: &mut LimitedVivienJsonlServerV11, seq: &mut u64, mut v: Value) -> Value {
    *seq += 1;
    v["schema_version"] = 11.into();
    v["request_id"] = format!("call{seq}").into();
    serde_json::from_str(&s.handle_line(&v.to_string())).unwrap()
}
fn fixture(state: GameState) -> (LimitedVivienJsonlServerV11, FastActorSessionV1) {
    let native = FastActorSessionV1::from_vivien_v70_fixture_state(state.clone());
    let mut s = LimitedVivienJsonlServerV11::new();
    s.active = Some(Active {
        core: VivienSessionV70::from_session(FastActorSessionV1::from_vivien_v70_fixture_state(
            state,
        ))
        .unwrap(),
        owner: Arc::new(()),
    });
    s.generation = 1;
    (s, native)
}
fn world(s: &LimitedVivienJsonlServerV11) -> &GameState {
    s.active.as_ref().unwrap().core.fixture_state_v70()
}
fn step(
    s: &mut LimitedVivienJsonlServerV11,
    native: &mut FastActorSessionV1,
    seq: &mut u64,
    choose: impl Fn(&A) -> bool,
) -> Value {
    let current = send(s, seq, json!({"request_type":"current"}));
    assert_eq!(current["state"]["state_type"], "decision");
    assert_eq!(
        current["state"]["decision"]["schema"],
        "mtg-kernel-limited-vivien-decision/v11"
    );
    let bound = s.pending.as_ref().unwrap();
    let index = bound.decision.actions().iter().position(choose).unwrap();
    let raw = bound
        .decision
        .fixture_original_index_v70(index as u32)
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
    assert_eq!(world(s), native.vivien_fixture_state());
    assert_eq!(
        s.active.as_ref().unwrap().core.fixture_counts_v70(),
        (native.policy_step_count(), native.physical_decision_count())
    );
    reply
}

fn paid_look(p: PlayerId, hidden: &str) -> GameState {
    let (s, v, _) = ready(p, hidden);
    let mut pair = crate::rl_session::vivien_v70::tests::Pair::new(s);
    pair.cast(v, p);
    pair.act(|a| matches!(a,A::ActivateAbility{source,ability_index:0,..}if source.arena_id==v.0));
    pair.to_look();
    pair.own.fixture_state_v70().clone()
}
#[test]
fn genuine_paid_vivien_wire_look_restore_retry_and_native_parity_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (mut wire, mut native) = fixture(paid_look(p, "Island"));
        let mut seq = 0;
        let first = send(&mut wire, &mut seq, json!({"request_type":"current"}));
        assert_eq!(
            first["state"]["decision"]["state"]["vivien"]["activations"][0]["loyalty_after"],
            6
        );
        let saved = send(&mut wire, &mut seq, json!({"request_type":"snapshot"}));
        let raw = native.snapshot_v1();
        step(&mut wire, &mut native, &mut seq, |a| {
            matches!(a, A::FinishEffectSelection { .. })
        });
        let restored = send(
            &mut wire,
            &mut seq,
            json!({"request_type":"restore","snapshot_id":saved["snapshot_id"]}),
        );
        assert_eq!(restored["response_type"], "state");
        native.vivien_restore_v70(&raw).unwrap();
        assert_eq!(world(&wire), native.vivien_fixture_state());
        step(&mut wire, &mut native, &mut seq, |a| {
            matches!(a, A::FinishEffectSelection { .. })
        });
    }
}
#[test]
fn genuine_paid_vivien_wire_private_target_selection_preserves_hand_reveal_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (mut wire, mut native) = fixture(paid_look(p, "Island"));
        let mut seq = 0;
        let before = world(&wire).players[p.index()].hand.len();
        step(
            &mut wire,
            &mut native,
            &mut seq,
            |a| matches!(a,A::ChooseEffectTarget{target:crate::rl::TargetRefV1::Object{object},..}if object.card_db_id==crate::card_def::card_id_by_name("Llanowar Elves").unwrap()),
        );
        assert_eq!(world(&wire).players[p.index()].hand.len(), before + 1);
        assert_eq!(world(&wire).known_hand_cards(p.opponent(), p).len(), 1);
    }
}
#[test]
fn genuine_paid_vivien_wire_old_protocol_and_stale_prompt_refuse_readonly_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (mut wire, _) = fixture(paid_look(p, "Island"));
        let mut seq = 0;
        let current = send(&mut wire, &mut seq, json!({"request_type":"current"}));
        let before = world(&wire).clone();
        let bad = wire.handle_line(
            &json!({"schema_version":10,"request_id":"wrong","request_type":"current"}).to_string(),
        );
        assert_eq!(
            serde_json::from_str::<Value>(&bad).unwrap()["response_type"],
            "error"
        );
        let reply = send(
            &mut wire,
            &mut seq,
            json!({"request_type":"step","episode_id":current["state"]["episode_id"],"authority_generation":current["state"]["authority_generation"],"actor":crate::rl::PlayerSeatV1::from(p),"prompt_seq":u64::MAX,"action_index":0}),
        );
        assert_eq!(reply["response_type"], "error");
        assert_eq!(world(&wire), &before);
    }
}
#[test]
fn genuine_paid_vivien_wire_partial_reset_failure_preserves_live_prompt_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (mut wire, _) = fixture(paid_look(p, "Island"));
        let mut seq = 0;
        let before = world(&wire).clone();
        let reply = send(
            &mut wire,
            &mut seq,
            json!({"request_type":"reset","expected_card_db_hash":KERNEL_CARDDB_HASH,"episode_id":1,"env_seed":123,"starting_player":crate::rl::PlayerSeatV1::from(p),"decks":[{"cards":[{"name":"Vivien Reid","count":40}]},{"cards":[{"name":"Forest","count":40}]}],"max_physical_decisions":1000,"max_policy_steps":1000}),
        );
        assert_eq!(reply["code"], "unsupported_deck");
        assert_eq!(world(&wire), &before);
    }
}
#[test]
fn genuine_paid_vivien_wire_independent_hidden_worlds_and_closed_private_fields_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (mut a, _) = fixture(paid_look(p, "Island"));
        let (mut b, _) = fixture(paid_look(p, "Mountain"));
        let mut sa = 0;
        let mut sb = 0;
        let av = send(&mut a, &mut sa, json!({"request_type":"current"}));
        let bv = send(&mut b, &mut sb, json!({"request_type":"current"}));
        assert_ne!(world(&a), world(&b));
        assert_eq!(av, bv);
        let text = av.to_string();
        for forbidden in [
            "arena_id",
            "history_index",
            "original_library",
            "private_world",
        ] {
            assert!(!text.contains(forbidden));
        }
        let before = world(&a).clone();
        let invalid = send(
            &mut a,
            &mut sa,
            json!({"request_type":"current","private_world":{}}),
        );
        assert_eq!(invalid["response_type"], "error");
        assert_eq!(world(&a), &before);
    }
}
#[test]
fn genuine_paid_vivien_wire_emblem_and_full_u32_loyalty_are_distinct_native_facts_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = crate::effect::vivien_reid_v1::tests::ready(
            p,
            &["Forest", "Island"],
            &["Vivien Reid"],
            "Mountain",
        );
        let v = crate::effect::vivien_reid_v1::tests::walker(&mut s, p);
        let before = crate::planeswalker_v1::loyalty(&s, v).unwrap(); /* Native counter-placement primitive, not paid loyalty. */
        crate::planeswalker_v1::add_loyalty_counters_checked_v1(&mut s, v, u32::MAX - before)
            .unwrap();
        let (mut wire, _) = fixture(s);
        let mut seq = 0;
        let reply = send(&mut wire, &mut seq, json!({"request_type":"current"}));
        assert_eq!(
            reply["state"]["decision"]["state"]["vivien"]["current"][0]["counters"]["loyalty"],
            u32::MAX
        );
        assert_eq!(
            reply["state"]["decision"]["state"]["vivien"]["emblems"],
            json!([])
        );
    }
}
