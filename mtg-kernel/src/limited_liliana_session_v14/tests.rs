//! Native-backed actor transport source fixtures. Partial reset remains refused.
use super::*;
use crate::{
    effect::liliana_dreadhorde_v1::tests::owner_v73_tests::owning_ready, ids::PlayerId,
    rl::ActionSemanticV1 as A, rl_session::FastActorSessionV1,
};
use serde_json::{json, Value};
fn send(s: &mut LimitedLilianaJsonlServerV14, seq: &mut u64, mut v: Value) -> Value {
    *seq += 1;
    v["schema_version"] = 14.into();
    v["request_id"] = format!("liliana{seq}").into();
    serde_json::from_str(&s.handle_line(&v.to_string())).unwrap()
}
fn server(world: crate::state::GameState) -> LimitedLilianaJsonlServerV14 {
    let mut s = LimitedLilianaJsonlServerV14::new();
    s.active = Some(Active {
        core: LilianaSessionV73::from_session(FastActorSessionV1::from_liliana_v73_fixture_state(
            world,
        ))
        .unwrap(),
        owner: Arc::new(()),
    });
    s.generation = 1;
    s
}

#[test]
fn genuine_paid_v14_three_roots_original_human_indices_and_restore_match_native_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for index in [0, 1, 2] {
            let (world, id) = owning_ready(p, index == 2, "Island");
            let mut native = FastActorSessionV1::from_liliana_v73_fixture_state(world.clone());
            let mut s = server(world);
            let mut seq = 0;
            let first = send(&mut s, &mut seq, json!({"request_type":"current"}));
            assert_eq!(first["protocol"], PROTOCOL);
            assert_eq!(first["schema_version"], 14);
            assert_eq!(
                first["state"]["decision"]["schema"],
                "mtg-kernel-limited-liliana-decision/v14"
            );
            let saved = send(&mut s, &mut seq, json!({"request_type":"snapshot"}));
            let raw = native.snapshot_v1();
            let b = s.pending.as_ref().unwrap();
            let stale = json!({"request_type":"step","episode_id":b.expected.episode_id,"authority_generation":s.generation,"actor":b.expected.acting_player,"prompt_seq":b.visible.prompt_seq,"action_index":0});
            send(
                &mut s,
                &mut seq,
                json!({"request_type":"restore","snapshot_id":saved["snapshot_id"]}),
            );
            native.liliana_restore_v73(&raw).unwrap();
            assert_eq!(send(&mut s, &mut seq, stale)["response_type"], "error");
            for turn in 0..512 {
                let current = send(&mut s, &mut seq, json!({"request_type":"current"}));
                assert_eq!(current["state"]["state_type"], "decision");
                let b = s.pending.as_ref().unwrap();
                if turn > 0
                    && b.decision
                        .observation()
                        .liliana
                        .activations
                        .last()
                        .is_some_and(|a| {
                            a.stage
                                == crate::liliana_public_v73::ActivationStageV73::NativeCompleted
                        })
                {
                    break;
                }
                let finish = b
                    .decision
                    .actions()
                    .iter()
                    .any(|a| matches!(a, A::FinishEffectSelection { .. }));
                let i=b.decision.actions().iter().position(|a|if turn==0 {matches!(a,A::ActivateAbility{source,ability_index,..}if source.arena_id==id.0&&*ability_index==index)} else if finish {matches!(a,A::FinishEffectSelection{..})} else {matches!(a,A::Pass{..}|A::OrderTriggers{..}|A::ChooseTriggerOrderNext{..}|A::ChooseEffectTarget{..}|A::ChooseAttackerInclusion{include:false,..}|A::ChooseBlockerInclusion{include:false,..})}).unwrap();
                let display = b.indexes.iter().position(|x| *x == i as u32).unwrap();
                let e = b.expected;
                let original = b.decision.fixture_original_index_v73(i as u32).unwrap();
                let request = json!({"request_type":"step","episode_id":e.episode_id,"authority_generation":s.generation,"actor":e.acting_player,"prompt_seq":b.visible.prompt_seq,"action_index":display});
                assert_eq!(send(&mut s, &mut seq, request)["response_type"], "state");
                native.step(e.episode_id, e.step, original).unwrap();
                assert_eq!(
                    s.active.as_ref().unwrap().core.fixture_state_v73(),
                    native.liliana_fixture_state()
                );
            }
            let b = s.pending.as_ref().unwrap();
            let f = &b.visible.state.liliana;
            assert_eq!(f.activations[0].ability_index, index);
            assert_eq!(
                f.activations[0].stage,
                crate::liliana_public_v73::ActivationStageV73::NativeCompleted
            );
            if index == 0 {
                assert_eq!(f.activations[0].created_zombies.len(), 1);
            } else {
                assert!(f.resolutions[0].finished);
            }
        }
    }
}
#[test]
fn genuine_paid_v14_original_observer_roles_private_view_and_duplicate_readonly_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let world =
            crate::effect::liliana_dreadhorde_v1::tests::owner_v73_tests::observer_boundary_ready(
                p,
            );
        let mut s = server(world);
        let mut seq = 0;
        let current = send(&mut s, &mut seq, json!({"request_type":"current"}));
        let f = &current["state"]["decision"]["state"]["liliana"];
        assert_eq!(f["current_draws"].as_array().unwrap().len(), 3);
        assert_eq!(f["deaths"].as_array().unwrap().len(), 3);
        assert!(f["deaths"][0]["observer"]["current_original_source"].is_null());
        assert_eq!(f["deaths"][0]["creature"].as_object().unwrap().len(), 5);
        let text = current.to_string();
        for key in [
            "arena_id",
            "zone_change_count",
            "history_index",
            "private_world",
        ] {
            assert!(!text.contains(key));
        }
        let before = s.active.as_ref().unwrap().core.fixture_state_v73().clone();
        let generation = s.generation;
        let req = json!({"request_type":"current","schema_version":14,"request_id":"same-current"})
            .to_string();
        let a = s.handle_line(&req);
        assert_eq!(s.handle_line(&req), a);
        assert_eq!(s.generation, generation);
        assert_eq!(s.active.as_ref().unwrap().core.fixture_state_v73(), &before);
    }
}
#[test]
fn genuine_paid_v14_old_schema_partial_reset_and_foreign_snapshot_refuse_atomically_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (world, _) = owning_ready(p, false, "Island");
        let mut s = server(world);
        let mut seq = 0;
        let before = s.active.as_ref().unwrap().core.fixture_state_v73().clone();
        let generation = s.generation;
        let old: Value = serde_json::from_str(
            &s.handle_line(
                &json!({"request_type":"current","schema_version":13,"request_id":"old-v13"})
                    .to_string(),
            ),
        )
        .unwrap();
        assert_eq!(old["response_type"], "error");
        let deck = json!({"cards":[{"name":"Liliana, Dreadhorde General","count":1},{"name":"Swamp","count":39}]});
        let r = send(
            &mut s,
            &mut seq,
            json!({"request_type":"reset","expected_card_db_hash":KERNEL_CARDDB_HASH,"episode_id":300,"env_seed":123,"starting_player":if p==PlayerId::P0{"p0"}else{"p1"},"decks":[deck.clone(),deck],"max_physical_decisions":1000,"max_policy_steps":1000}),
        );
        assert_eq!(r["response_type"], "error");
        assert_eq!(
            send(
                &mut s,
                &mut seq,
                json!({"request_type":"restore","snapshot_id":9999})
            )["response_type"],
            "error"
        );
        assert_eq!(s.generation, generation);
        assert_eq!(s.active.as_ref().unwrap().core.fixture_state_v73(), &before);
    }
}
