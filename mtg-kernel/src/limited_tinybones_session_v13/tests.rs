//! Native-backed actor transport source fixtures. Partial reset remains refused.
use super::*;
use crate::{
    engine::tinybones_v72::tests::complete_observation::owning_ready, ids::PlayerId,
    rl::ActionSemanticV1 as A, rl_session::FastActorSessionV1, state::Zone,
};
use serde_json::{json, Value};
fn send(s: &mut LimitedTinybonesJsonlServerV13, seq: &mut u64, mut v: Value) -> Value {
    *seq += 1;
    v["schema_version"] = 13.into();
    v["request_id"] = format!("tinybones{seq}").into();
    serde_json::from_str(&s.handle_line(&v.to_string())).unwrap()
}
fn server(world: crate::state::GameState) -> LimitedTinybonesJsonlServerV13 {
    let mut s = LimitedTinybonesJsonlServerV13::new();
    s.active = Some(Active {
        core: TinybonesSessionV72::from_session(
            FastActorSessionV1::from_tinybones_v72_fixture_state(world),
        )
        .unwrap(),
        owner: Arc::new(()),
    });
    s.generation = 1;
    s
}
#[test]
fn genuine_paid_v13_original_indices_activation_discard_stash_and_restore_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (world, tiny, card) = owning_ready(p, "Forest");
        let mut native = FastActorSessionV1::from_tinybones_v72_fixture_state(world.clone());
        let mut s = server(world);
        let mut seq = 0;
        let current = send(&mut s, &mut seq, json!({"request_type":"current"}));
        assert_eq!(current["protocol"], PROTOCOL);
        assert_eq!(current["schema_version"], 13);
        assert_eq!(
            current["state"]["decision"]["schema"],
            "mtg-kernel-limited-tinybones-decision/v13"
        );
        let saved = send(&mut s, &mut seq, json!({"request_type":"snapshot"}));
        let raw = native.snapshot_v1();
        let b = s.pending.as_ref().unwrap();
        let old = json!({"request_type":"step","episode_id":b.expected.episode_id,"authority_generation":s.generation,"actor":b.expected.acting_player,"prompt_seq":b.visible.prompt_seq,"action_index":0});
        send(
            &mut s,
            &mut seq,
            json!({"request_type":"restore","snapshot_id":saved["snapshot_id"]}),
        );
        native.tinybones_restore_v72(&raw).unwrap();
        assert_eq!(send(&mut s, &mut seq, old)["response_type"], "error");
        for turn in 0..96 {
            if s.active
                .as_ref()
                .unwrap()
                .core
                .fixture_state_v72()
                .objects
                .get(card)
                .zone
                == Zone::Exile
            {
                break;
            }
            let current = send(&mut s, &mut seq, json!({"request_type":"current"}));
            assert_eq!(current["state"]["state_type"], "decision");
            let b = s.pending.as_ref().unwrap();
            let i=b.decision.actions().iter().position(|a|if turn==0 {matches!(a,A::ActivateAbility{source,ability_index:0,..}if source.arena_id==tiny.0)}else {matches!(a,A::Pass{..}|A::OrderTriggers{..}|A::ChooseTriggerOrderNext{..}|A::ChooseAttackerInclusion{include:false,..}|A::ChooseBlockerInclusion{include:false,..})||matches!(a,A::Discard{cards,..}if cards.iter().any(|r|r.arena_id==card.0))}).unwrap();
            let display = b.indexes.iter().position(|x| *x == i as u32).unwrap();
            let e = b.expected;
            let original = b.decision.fixture_original_index_v72(i as u32).unwrap();
            let request = json!({"request_type":"step","episode_id":e.episode_id,"authority_generation":s.generation,"actor":e.acting_player,"prompt_seq":b.visible.prompt_seq,"action_index":display});
            let r = send(&mut s, &mut seq, request);
            assert_eq!(r["response_type"], "state");
            native.step(e.episode_id, e.step, original).unwrap();
            assert_eq!(
                s.active.as_ref().unwrap().core.fixture_state_v72(),
                native.tinybones_fixture_state()
            );
        }
        let state = s.active.as_ref().unwrap().core.fixture_state_v72();
        assert_eq!(state.objects.get(card).zone, Zone::Exile);
        assert_eq!(state.objects.get(card).counters.stash, 1);
        assert!(state.objects.get(tiny).tapped);
    }
}
#[test]
fn genuine_paid_v13_failed_partial_reset_and_old_schema_leave_owner_unchanged_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (world, _, _) = owning_ready(p, "Forest");
        let mut s = server(world);
        let mut seq = 0;
        let before = s.active.as_ref().unwrap().core.fixture_state_v72().clone();
        let generation = s.generation;
        let old = serde_json::from_str::<Value>(
            &s.handle_line(
                &json!({"request_type":"current","schema_version":12,"request_id":"old-v12"})
                    .to_string(),
            ),
        )
        .unwrap();
        assert_eq!(old["response_type"], "error");
        let deck = json!({"cards":[{"name":"Tinybones, Bauble Burglar","count":1},{"name":"Swamp","count":39}]});
        let reset = send(
            &mut s,
            &mut seq,
            json!({"request_type":"reset","expected_card_db_hash":KERNEL_CARDDB_HASH,"episode_id":300,"env_seed":123,"starting_player":if p==PlayerId::P0{"p0"}else{"p1"},"decks":[deck.clone(),deck],"max_physical_decisions":1000,"max_policy_steps":1000}),
        );
        assert_eq!(reset["response_type"], "error");
        assert_eq!(s.generation, generation);
        assert_eq!(s.active.as_ref().unwrap().core.fixture_state_v72(), &before);
    }
}
