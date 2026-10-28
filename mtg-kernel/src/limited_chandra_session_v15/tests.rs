//! Native-backed actor transport source fixtures. Partial reset remains refused.
use super::*;
use crate::{
    effect::chandra_flameshaper_v1::tests::owner_v74_tests::owning_ready, ids::PlayerId,
    rl::ActionSemanticV1 as A, rl_session::FastActorSessionV1,
};
use serde_json::{json, Value};
fn send(s: &mut LimitedChandraJsonlServerV15, seq: &mut u64, mut v: Value) -> Value {
    *seq += 1;
    v["schema_version"] = 15.into();
    v["request_id"] = format!("chandra{seq}").into();
    serde_json::from_str(&s.handle_line(&v.to_string())).unwrap()
}
fn server(world: crate::state::GameState) -> LimitedChandraJsonlServerV15 {
    let mut s = LimitedChandraJsonlServerV15::new();
    s.active = Some(Active {
        core: ChandraSessionV74::from_session(FastActorSessionV1::from_chandra_v74_fixture_state(
            world,
        ))
        .unwrap(),
        owner: Arc::new(()),
    });
    s.generation = 1;
    s
}

#[test]
fn genuine_paid_v15_three_roots_original_human_indices_and_restore_match_native_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for index in [0, 1, 2] {
            let (world, id, _, _) = owning_ready(p, index == 1, "Island");
            let mut native = FastActorSessionV1::from_chandra_v74_fixture_state(world.clone());
            let mut s = server(world);
            let mut seq = 0;
            let first = send(&mut s, &mut seq, json!({"request_type":"current"}));
            assert_eq!(first["protocol"], PROTOCOL);
            assert_eq!(first["schema_version"], 15);
            assert_eq!(
                first["state"]["decision"]["schema"],
                "mtg-kernel-limited-chandra-decision/v15"
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
            native.chandra_restore_v74(&raw).unwrap();
            assert_eq!(send(&mut s, &mut seq, stale)["response_type"], "error");
            for turn in 0..512 {
                let current = send(&mut s, &mut seq, json!({"request_type":"current"}));
                assert_eq!(current["state"]["state_type"], "decision");
                let b = s.pending.as_ref().unwrap();
                if turn > 0
                    && b.decision
                        .observation()
                        .chandra
                        .activations
                        .last()
                        .is_some_and(|a| {
                            a.stage
                                == crate::chandra_public_v74::ActivationStageV74::NativeCompleted
                        })
                {
                    break;
                }
                let finish = b
                    .decision
                    .actions()
                    .iter()
                    .any(|a| matches!(a, A::FinishEffectSelection { .. }));
                let i=b.decision.actions().iter().position(|a|if turn==0 {matches!(a,A::ActivateAbility{source,ability_index,..}if source.arena_id==id.0&&*ability_index==index)} else if finish {matches!(a,A::FinishEffectSelection{..})} else {matches!(a,A::Pass{..}|A::OrderTriggers{..}|A::ChooseTriggerOrderNext{..}|A::ChooseEffectTarget{..}|A::ChooseTarget{..}|A::ChooseAttackerInclusion{include:false,..}|A::ChooseBlockerInclusion{include:false,..})}).unwrap();
                let display = b.indexes.iter().position(|x| *x == i as u32).unwrap();
                let e = b.expected;
                let original = b.decision.fixture_original_index_v74(i as u32).unwrap();
                let request = json!({"request_type":"step","episode_id":e.episode_id,"authority_generation":s.generation,"actor":e.acting_player,"prompt_seq":b.visible.prompt_seq,"action_index":display});
                assert_eq!(send(&mut s, &mut seq, request)["response_type"], "state");
                native.step(e.episode_id, e.step, original).unwrap();
                assert_eq!(
                    s.active.as_ref().unwrap().core.fixture_state_v74(),
                    native.chandra_fixture_state()
                );
            }
            let b = s.pending.as_ref().unwrap();
            let f = &b.visible.state.chandra;
            assert_eq!(f.activations[0].ability_index, index);
            assert_eq!(
                f.activations[0].stage,
                crate::chandra_public_v74::ActivationStageV74::NativeCompleted
            );
            if index == 0 {
                assert_eq!(f.exiles[0].cards.len(), 3);
                assert!(f.exiles[0].permission_present);
            } else if index == 1 {
                assert_eq!(f.copied_tokens.len(), 2);
                assert!(f
                    .copied_tokens
                    .iter()
                    .all(|t| t.current_haste == Some(true)));
            } else {
                assert!(f.activations[0].targets.is_empty());
            }
        }
    }
}
#[test]
fn genuine_paid_v15_affected_damage_original_option_bindings_readonly_restore_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let world =
            crate::effect::chandra_flameshaper_v1::tests::owner_v74_tests::pending_damage_boundary(
                p,
            );
        let mut native = FastActorSessionV1::from_chandra_v74_fixture_state(world.clone());
        let mut s = server(world);
        let mut seq = 0;
        let current = send(&mut s, &mut seq, json!({"request_type":"current"}));
        assert_eq!(
            current["state"]["decision"]["actor"],
            serde_json::to_value(crate::rl::PlayerSeatV1::from(p.opponent())).unwrap()
        );
        let b = s.pending.as_ref().unwrap();
        let f = b.visible.state.chandra.pending_damage.as_ref().unwrap();
        assert_eq!((f.original_amount, f.remaining_amount), (8, 8));
        for (display, original) in b.indexes.iter().enumerate() {
            let action = &b.decision.actions()[*original as usize];
            if let A::ChooseEffectOption { option_index, .. } = action {
                assert_eq!(
                    b.visible.action_meanings[display]
                        .chandra
                        .selected_damage_option,
                    Some(*option_index)
                );
            }
        }
        let saved = send(&mut s, &mut seq, json!({"request_type":"snapshot"}));
        let raw = native.snapshot_v1();
        send(
            &mut s,
            &mut seq,
            json!({"request_type":"restore","snapshot_id":saved["snapshot_id"]}),
        );
        native.chandra_restore_v74(&raw).unwrap();
        let b = s.pending.as_ref().unwrap();
        let selected = b
            .visible
            .state
            .chandra
            .pending_damage
            .as_ref()
            .unwrap()
            .options
            .iter()
            .find(|o| o.kind == crate::chandra_public_v74::PreventionKindV74::Shield)
            .unwrap()
            .menu_index;
        let public = b
            .decision
            .actions()
            .iter()
            .position(
                |a| matches!(a,A::ChooseEffectOption{option_index,..}if *option_index==selected),
            )
            .unwrap();
        let display = b.indexes.iter().position(|i| *i == public as u32).unwrap();
        let e = b.expected;
        let original = b
            .decision
            .fixture_original_index_v74(public as u32)
            .unwrap();
        let req = json!({"request_type":"step","episode_id":e.episode_id,"authority_generation":s.generation,"actor":e.acting_player,"prompt_seq":b.visible.prompt_seq,"action_index":display});
        assert_eq!(send(&mut s, &mut seq, req)["response_type"], "state");
        native.step(e.episode_id, e.step, original).unwrap();
        assert_eq!(
            s.active.as_ref().unwrap().core.fixture_state_v74(),
            native.chandra_fixture_state()
        );
        let before = s.active.as_ref().unwrap().core.fixture_state_v74().clone();
        let generation = s.generation;
        let req = json!({"request_type":"current","schema_version":15,"request_id":"same-current"})
            .to_string();
        let a = s.handle_line(&req);
        assert_eq!(s.handle_line(&req), a);
        assert_eq!(s.generation, generation);
        assert_eq!(s.active.as_ref().unwrap().core.fixture_state_v74(), &before);
        for key in [
            "arena_id",
            "zone_change_count",
            "history_index",
            "private_world",
        ] {
            assert!(!current.to_string().contains(key));
        }
    }
}
#[test]
fn genuine_paid_v15_old_schema_partial_reset_and_foreign_snapshot_refuse_atomically_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (world, _, _, _) = owning_ready(p, false, "Island");
        let mut s = server(world);
        let mut seq = 0;
        let before = s.active.as_ref().unwrap().core.fixture_state_v74().clone();
        let generation = s.generation;
        let old: Value = serde_json::from_str(
            &s.handle_line(
                &json!({"request_type":"current","schema_version":14,"request_id":"old-v14"})
                    .to_string(),
            ),
        )
        .unwrap();
        assert_eq!(old["response_type"], "error");
        let deck = json!({"cards":[{"name":"Chandra, Flameshaper","count":1},{"name":"Mountain","count":39}]});
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
        assert_eq!(s.active.as_ref().unwrap().core.fixture_state_v74(), &before);
    }
}
