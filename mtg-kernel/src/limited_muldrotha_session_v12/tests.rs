use super::*;
use crate::{
    ids::PlayerId,
    rl::ActionSemanticV1 as A,
    rl_session::{
        muldrotha_v71::tests::{ready, Pair},
        FastActorSessionV1,
    },
};
use serde_json::{json, Value};
fn send(s: &mut LimitedMuldrothaJsonlServerV12, seq: &mut u64, mut v: Value) -> Value {
    *seq += 1;
    v["schema_version"] = 12.into();
    v["request_id"] = format!("muldrotha{seq}").into();
    serde_json::from_str(&s.handle_line(&v.to_string())).unwrap()
}
#[test]
fn paid_v12_permission_original_ordinal_payment_restore_and_stale_prompt_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (world, _, card) = ready(p, "Island");
        let mut pair = Pair::new(world);
        pair.act(|a| matches!(a,A::CastSpell{source,..}if source.arena_id==card.0));
        let world = pair.own.fixture_state_v71().clone();
        let mut native = FastActorSessionV1::from_muldrotha_v71_fixture_state(world.clone());
        let mut s = LimitedMuldrothaJsonlServerV12::new();
        s.active = Some(Active {
            core: MuldrothaSessionV71::from_session(
                FastActorSessionV1::from_muldrotha_v71_fixture_state(world),
            )
            .unwrap(),
            owner: Arc::new(()),
        });
        s.generation = 1;
        let mut seq = 0;
        let current = send(&mut s, &mut seq, json!({"request_type":"current"}));
        assert_eq!(current["protocol"], PROTOCOL);
        assert_eq!(current["schema_version"], 12);
        assert_eq!(
            current["state"]["decision"]["schema"],
            "mtg-kernel-limited-muldrotha-decision/v12"
        );
        assert_eq!(
            current["state"]["decision"]["state"]["muldrotha"]["plays"][0]["stage"],
            "PermissionChoice"
        );
        let save = send(&mut s, &mut seq, json!({"request_type":"snapshot"}));
        let raw = native.snapshot_v1();
        let bound = s.pending.as_ref().unwrap();
        let e = bound.expected;
        let original = bound.decision.fixture_original_index_v71(0).unwrap();
        let request = json!({"request_type":"step","episode_id":e.episode_id,"authority_generation":s.generation,"actor":e.acting_player,"prompt_seq":bound.visible.prompt_seq,"action_index":0});
        let reply = send(&mut s, &mut seq, request.clone());
        assert_eq!(reply["response_type"], "state");
        native.step(e.episode_id, e.step, original).unwrap();
        assert_eq!(
            s.active.as_ref().unwrap().core.fixture_state_v71(),
            native.muldrotha_fixture_state()
        );
        let restored = send(
            &mut s,
            &mut seq,
            json!({"request_type":"restore","snapshot_id":save["snapshot_id"]}),
        );
        native.restore_v1(&raw);
        assert_eq!(restored["state"]["state_type"], "decision");
        let before = s.active.as_ref().unwrap().core.fixture_state_v71().clone();
        let stale = send(&mut s, &mut seq, request);
        assert_eq!(stale["response_type"], "error");
        assert_eq!(s.active.as_ref().unwrap().core.fixture_state_v71(), &before);
    }
}
