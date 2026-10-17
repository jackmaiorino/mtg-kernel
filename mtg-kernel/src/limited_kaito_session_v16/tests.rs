//! Native-backed actor transport source fixtures. Partial reset remains refused.
use super::*;
use crate::{
    effect::kaito_cunning_v75::tests::owner_v75_tests::owning_ready, ids::PlayerId,
    rl::ActionSemanticV1 as A, rl_session::FastActorSessionV1,
};
use serde_json::{json, Value};
fn send(s: &mut LimitedKaitoJsonlServerV16, seq: &mut u64, mut v: Value) -> Value {
    *seq += 1;
    v["schema_version"] = 16.into();
    v["request_id"] = format!("kaito{seq}").into();
    serde_json::from_str(&s.handle_line(&v.to_string())).unwrap()
}
fn server(world: crate::state::GameState) -> LimitedKaitoJsonlServerV16 {
    let mut s = LimitedKaitoJsonlServerV16::new();
    s.active = Some(Active {
        core: KaitoSessionV75::from_session(FastActorSessionV1::from_kaito_v75_fixture_state(
            world,
        ))
        .unwrap(),
        owner: Arc::new(()),
    });
    s.generation = 1;
    s
}

#[test]
fn genuine_paid_v16_three_roots_original_human_indices_and_restore_match_native_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for index in [0, 1, 2] {
            let (mut world, id, _) = owning_ready(p, index == 1, "Island");
            if index == 2 {
                crate::effect::kaito_cunning_v75::tests::owner_v75_tests::prepare_ultimate(
                    &mut world, id,
                );
            }
            let mut native = FastActorSessionV1::from_kaito_v75_fixture_state(world.clone());
            let mut s = server(world);
            let mut seq = 0;
            let first = send(&mut s, &mut seq, json!({"request_type":"current"}));
            assert_eq!(first["protocol"], PROTOCOL);
            assert_eq!(first["schema_version"], 16);
            assert_eq!(
                first["state"]["decision"]["schema"],
                "mtg-kernel-limited-kaito-decision/v16"
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
            native.kaito_restore_v75(&raw).unwrap();
            assert_eq!(send(&mut s, &mut seq, stale)["response_type"], "error");
            for turn in 0..512 {
                let current = send(&mut s, &mut seq, json!({"request_type":"current"}));
                assert_eq!(current["state"]["state_type"], "decision");
                let b = s.pending.as_ref().unwrap();
                if turn > 0
                    && b.decision
                        .observation()
                        .kaito
                        .activations
                        .last()
                        .is_some_and(|a| a.stage == crate::kaito_public_v75::StageV75::Resolved)
                {
                    break;
                }
                let finish = b
                    .decision
                    .actions()
                    .iter()
                    .any(|a| matches!(a, A::FinishEffectSelection { .. }));
                let i=b.decision.actions().iter().position(|a|if turn==0 {matches!(a,A::ActivateAbility{source,ability_index,..}if source.arena_id==id.0&&*ability_index==index)} else if finish {matches!(a,A::FinishEffectSelection{..})} else {matches!(a,A::Pass{..}|A::OrderTriggers{..}|A::ChooseTriggerOrderNext{..}|A::ChooseEffectTarget{..}|A::ChooseTarget{..}|A::Discard{..}|A::ChooseAttackerInclusion{include:false,..}|A::ChooseBlockerInclusion{include:false,..})}).unwrap();
                let display = b.indexes.iter().position(|x| *x == i as u32).unwrap();
                let e = b.expected;
                let original = b.decision.fixture_original_index_v75(i as u32).unwrap();
                let request = json!({"request_type":"step","episode_id":e.episode_id,"authority_generation":s.generation,"actor":e.acting_player,"prompt_seq":b.visible.prompt_seq,"action_index":display});
                assert_eq!(send(&mut s, &mut seq, request)["response_type"], "state");
                native.step(e.episode_id, e.step, original).unwrap();
                assert_eq!(
                    s.active.as_ref().unwrap().core.fixture_state_v75(),
                    native.kaito_fixture_state()
                );
            }
            let b = s.pending.as_ref().unwrap();
            let f = &b.visible.state.kaito;
            assert_eq!(f.activations[0].ability_index, index);
            assert_eq!(
                f.activations[0].stage,
                crate::kaito_public_v75::StageV75::Resolved
            );
            if index == 0 {
                assert_eq!(
                    f.activations[0].draw,
                    crate::kaito_public_v75::DrawV75::Drawn
                );
                assert!(f.activations[0].discard.as_ref().unwrap().answered);
            } else if index == 1 {
                assert_eq!(f.ninja_batches[0].created.len(), 2);
            } else {
                assert_eq!(f.emblems.len(), 1);
            }
        }
    }
}
#[test]
fn genuine_paid_v16_controller_private_discard_original_bindings_restore_and_readonly_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (world, id, _) = owning_ready(p, false, "Island");
        let mut native = FastActorSessionV1::from_kaito_v75_fixture_state(world.clone());
        let mut s = server(world);
        let mut seq = 0;
        for turn in 0..128 {
            send(&mut s, &mut seq, json!({"request_type":"current"}));
            let b = s.pending.as_ref().unwrap();
            if b.visible
                .state
                .kaito
                .activations
                .last()
                .is_some_and(|a| a.stage == crate::kaito_public_v75::StageV75::WaitingDiscard)
            {
                break;
            }
            let finish = b
                .decision
                .actions()
                .iter()
                .any(|a| matches!(a, A::FinishEffectSelection { .. }));
            let public=b.decision.actions().iter().position(|a|if turn==0{matches!(a,A::ActivateAbility{source,ability_index:0,..}if source.arena_id==id.0)}else if finish{matches!(a,A::FinishEffectSelection{..})}else{matches!(a,A::Pass{..}|A::OrderTriggers{..}|A::ChooseTriggerOrderNext{..})}).unwrap();
            let display = b.indexes.iter().position(|i| *i == public as u32).unwrap();
            let e = b.expected;
            let original = b
                .decision
                .fixture_original_index_v75(public as u32)
                .unwrap();
            let req = json!({"request_type":"step","episode_id":e.episode_id,"authority_generation":s.generation,"actor":e.acting_player,"prompt_seq":b.visible.prompt_seq,"action_index":display});
            assert_eq!(send(&mut s, &mut seq, req)["response_type"], "state");
            native.step(e.episode_id, e.step, original).unwrap();
            assert_eq!(
                s.active.as_ref().unwrap().core.fixture_state_v75(),
                native.kaito_fixture_state()
            );
        }
        let b = s.pending.as_ref().unwrap();
        let a = &b.visible.state.kaito.activations[0];
        assert_eq!(a.stage, crate::kaito_public_v75::StageV75::WaitingDiscard);
        assert_eq!(b.expected.acting_player, crate::rl::PlayerSeatV1::from(p));
        assert!(a.discard.as_ref().unwrap().private_choices.is_some());
        for (display, public) in b.indexes.iter().enumerate() {
            if let A::Discard { cards, .. } = &b.decision.actions()[*public as usize] {
                let selected = b.visible.action_meanings[display]
                    .kaito
                    .selected_discard
                    .as_ref()
                    .unwrap();
                assert_eq!(selected.len(), cards.len());
                assert!(selected.iter().all(|r| a
                    .discard
                    .as_ref()
                    .unwrap()
                    .private_choices
                    .as_ref()
                    .unwrap()
                    .contains(r)));
            }
        }
        let saved = send(&mut s, &mut seq, json!({"request_type":"snapshot"}));
        let raw = native.snapshot_v1();
        send(
            &mut s,
            &mut seq,
            json!({"request_type":"restore","snapshot_id":saved["snapshot_id"]}),
        );
        native.kaito_restore_v75(&raw).unwrap();
        let b = s.pending.as_ref().unwrap();
        let public = b
            .decision
            .actions()
            .iter()
            .position(|a| matches!(a, A::Discard { .. }))
            .unwrap();
        let display = b.indexes.iter().position(|i| *i == public as u32).unwrap();
        let e = b.expected;
        let original = b
            .decision
            .fixture_original_index_v75(public as u32)
            .unwrap();
        let req = json!({"request_type":"step","episode_id":e.episode_id,"authority_generation":s.generation,"actor":e.acting_player,"prompt_seq":b.visible.prompt_seq,"action_index":display});
        assert_eq!(send(&mut s, &mut seq, req)["response_type"], "state");
        native.step(e.episode_id, e.step, original).unwrap();
        assert_eq!(
            s.active.as_ref().unwrap().core.fixture_state_v75(),
            native.kaito_fixture_state()
        );
        let before = s.active.as_ref().unwrap().core.fixture_state_v75().clone();
        let generation = s.generation;
        let req = json!({"request_type":"current","schema_version":16,"request_id":"same-current"})
            .to_string();
        let response = s.handle_line(&req);
        assert_eq!(s.handle_line(&req), response);
        assert_eq!(s.generation, generation);
        assert_eq!(s.active.as_ref().unwrap().core.fixture_state_v75(), &before);
    }
}
#[test]
fn genuine_paid_v16_old_schema_partial_reset_and_foreign_snapshot_refuse_atomically_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (world, _, _) = owning_ready(p, false, "Island");
        let mut s = server(world);
        let mut seq = 0;
        let before = s.active.as_ref().unwrap().core.fixture_state_v75().clone();
        let generation = s.generation;
        let old: Value = serde_json::from_str(
            &s.handle_line(
                &json!({"request_type":"current","schema_version":15,"request_id":"old-v15"})
                    .to_string(),
            ),
        )
        .unwrap();
        assert_eq!(old["response_type"], "error");
        let deck = json!({"cards":[{"name":"Kaito, Cunning Infiltrator","count":1},{"name":"Island","count":39}]});
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
        assert_eq!(s.active.as_ref().unwrap().core.fixture_state_v75(), &before);
    }
}
