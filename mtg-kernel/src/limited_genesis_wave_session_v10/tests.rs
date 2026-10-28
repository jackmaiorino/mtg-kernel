//! Native-backed source wire fixtures, not real-process/runtime qualification.
use super::*;
use crate::{
    ids::PlayerId,
    rl::ActionSemanticV1 as A,
    rl_session::genesis_wave_v69::tests::ready,
    rl_session::FastActorSessionV1,
    state::{GameState, Zone},
};
use serde_json::{json, Value};
fn send(s: &mut LimitedGenesisWaveJsonlServerV10, seq: &mut u64, mut v: Value) -> Value {
    *seq += 1;
    v["schema_version"] = 10.into();
    v["request_id"] = format!("call{seq}").into();
    serde_json::from_str(&s.handle_line(&v.to_string())).unwrap()
}
fn fixture(state: GameState) -> (LimitedGenesisWaveJsonlServerV10, FastActorSessionV1) {
    let native = FastActorSessionV1::from_genesis_wave_v69_fixture_state(state.clone());
    let mut s = LimitedGenesisWaveJsonlServerV10::new();
    s.active = Some(Active {
        core: GenesisWaveSessionV69::from_session(
            FastActorSessionV1::from_genesis_wave_v69_fixture_state(state),
        )
        .unwrap(),
        owner: Arc::new(()),
    });
    s.generation = 1;
    (s, native)
}
fn world(s: &LimitedGenesisWaveJsonlServerV10) -> &GameState {
    s.active.as_ref().unwrap().core.fixture_state_v69()
}
fn step(
    s: &mut LimitedGenesisWaveJsonlServerV10,
    native: &mut FastActorSessionV1,
    seq: &mut u64,
    choose: impl Fn(&A) -> bool,
) -> Value {
    let current = send(s, seq, json!({"request_type":"current"}));
    assert_eq!(current["state"]["state_type"], "decision");
    assert_eq!(
        current["state"]["decision"]["schema"],
        "mtg-kernel-limited-genesis-wave-decision/v10"
    );
    let bound = s.pending.as_ref().unwrap();
    let index = bound.decision.actions().iter().position(choose).unwrap();
    let raw = bound
        .decision
        .fixture_original_index_v69(index as u32)
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
    assert_eq!(world(s), native.genesis_wave_fixture_state());
    assert_eq!(
        s.active.as_ref().unwrap().core.fixture_counts_v69(),
        (native.policy_step_count(), native.physical_decision_count())
    );
    reply
}

#[test]
fn genuine_paid_wave_wire_complete_reveal_restore_retry_and_failed_partial_reset_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (initial, wave, _) = ready(p, "Island");
        let (mut wire, mut native) = fixture(initial);
        let mut seq = 0;
        step(
            &mut wire,
            &mut native,
            &mut seq,
            |a| matches!(a,A::CastSpell{source,..}if source.arena_id==wave.0),
        );
        step(&mut wire, &mut native, &mut seq, |a| {
            matches!(
                a,
                A::ChooseEffectOption {
                    option_index: 4,
                    ..
                }
            )
        });
        assert_eq!(
            crate::effect::genesis_wave_v1::tests::tapped(world(&wire), p),
            7
        );
        assert!(world(&wire).engine.pending_cast.is_none());
        assert_eq!(world(&wire).players[p.index()].mana_pool, [0; 6]);
        for _ in 0..24 {
            let reply = send(&mut wire, &mut seq, json!({"request_type":"current"}));
            if reply["state"]["decision"]["state"]["genesis_wave"]["waves"][0]["stage"] == "Subset"
            {
                break;
            }
            step(&mut wire, &mut native, &mut seq, |a| {
                matches!(a, A::Pass { .. })
            });
        }
        let current = send(&mut wire, &mut seq, json!({"request_type":"current"}));
        let f = &current["state"]["decision"]["state"]["genesis_wave"];
        assert_eq!(f["meaning"]["card_id"], 487);
        assert_eq!(f["waves"][0]["x_value"], 4);
        assert_eq!(f["waves"][0]["revealed"].as_array().unwrap().len(), 4);
        let text = current.to_string();
        for forbidden in [
            "original_library",
            "remaining_library",
            "history_index",
            "arena_id",
        ] {
            assert!(!text.contains(forbidden));
        }
        let saved = send(&mut wire, &mut seq, json!({"request_type":"snapshot"}));
        let raw = native.snapshot_v1();
        let before = world(&wire).clone();
        let generation = wire.generation;
        let prompt = wire.pending.as_ref().unwrap().visible.clone();
        let failed = send(
            &mut wire,
            &mut seq,
            json!({"request_type":"reset","expected_card_db_hash":KERNEL_CARDDB_HASH,"episode_id":77,"env_seed":123,"starting_player":"p0","decks":[{"cards":[{"name":"Genesis Wave","count":40}]},{"cards":[{"name":"Forest","count":40}]}],"max_physical_decisions":1000,"max_policy_steps":1000}),
        );
        assert_eq!(failed["code"], "unsupported_deck");
        assert_eq!(wire.generation, generation);
        assert_eq!(wire.pending.as_ref().unwrap().visible, prompt);
        assert_eq!(world(&wire), &before);
        step(&mut wire, &mut native, &mut seq, |a| {
            matches!(a, A::FinishEffectSelection { .. })
        });
        let restored = send(
            &mut wire,
            &mut seq,
            json!({"request_type":"restore","snapshot_id":saved["snapshot_id"]}),
        );
        native.genesis_wave_restore_v69(&raw).unwrap();
        assert_eq!(restored["response_type"], "state");
        assert_eq!(world(&wire), &before);
        assert_eq!(world(&wire), native.genesis_wave_fixture_state());
        assert!(wire.generation > generation);
        let stale = send(
            &mut wire,
            &mut seq,
            json!({"request_type":"step","episode_id":current["state"]["episode_id"],"authority_generation":generation,"actor":PlayerSeatV1::from(p),"prompt_seq":prompt.prompt_seq,"action_index":0}),
        );
        assert_eq!(stale["response_type"], "error");
        assert_eq!(world(&wire), &before);
    }
}
#[test]
fn genuine_paid_wave_wire_retains_inherited_time_stop_disposal_before_resolution_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = crate::engine::time_stop_v1::tests::ready(p);
        for _ in 0..7 {
            crate::policy_observation_v6::tests::put(&mut s, p, "Forest", Zone::Battlefield);
        }
        let wave = crate::policy_observation_v6::tests::put(&mut s, p, "Genesis Wave", Zone::Hand);
        let stop = crate::policy_observation_v6::tests::put(&mut s, p, "Time Stop", Zone::Hand);
        let (mut wire, mut native) = fixture(s);
        let mut seq = 0;
        step(
            &mut wire,
            &mut native,
            &mut seq,
            |a| matches!(a,A::CastSpell{source,..}if source.arena_id==wave.0),
        );
        step(&mut wire, &mut native, &mut seq, |a| {
            matches!(
                a,
                A::ChooseEffectOption {
                    option_index: 1,
                    ..
                }
            )
        });
        step(
            &mut wire,
            &mut native,
            &mut seq,
            |a| matches!(a,A::CastSpell{source,..}if source.arena_id==stop.0),
        );
        for _ in 0..40 {
            let r = send(&mut wire, &mut seq, json!({"request_type":"current"}));
            if r["state"]["decision"]["state"]["end_turn"]["disposals"]
                .as_array()
                .is_some_and(|r| !r.is_empty())
            {
                break;
            }
            step(&mut wire, &mut native, &mut seq, |a| {
                matches!(a, A::Pass { .. })
            });
        }
        let r = send(&mut wire, &mut seq, json!({"request_type":"current"}));
        let d = &r["state"]["decision"]["state"]["end_turn"]["disposals"][0];
        assert_eq!(d["disposed_rows"].as_array().unwrap().len(), 2);
        assert_eq!(
            d["disposed_rows"][0]["original"]["source"]["name"],
            "Genesis Wave"
        );
        assert_eq!(world(&wire).objects.get(wave).zone, Zone::Exile);
        assert!(r["state"]["decision"]["state"]["genesis_wave"]["waves"]
            .as_array()
            .unwrap()
            .is_empty());
        assert_eq!(
            crate::effect::genesis_wave_v1::tests::tapped(world(&wire), p),
            10
        );
        assert_eq!(world(&wire).players[p.index()].mana_pool, [0; 6]);
        assert_eq!(d["disposed_rows"][0]["original"]["x_value"], 1);
        assert_eq!(
            d["disposed_rows"][0]["original"]["controller"],
            serde_json::to_value(PlayerSeatV1::from(p)).unwrap()
        );
        assert_eq!(d["disposed_rows"][0]["original"]["cast_method"], "normal");
        let decoded: GameState =
            serde_json::from_slice(&serde_json::to_vec(world(&wire)).unwrap()).unwrap();
        crate::genesis_wave_observation_v69::validate_state(&decoded).unwrap();
        assert_eq!(&decoded, world(&wire));
        let saved = send(&mut wire, &mut seq, json!({"request_type":"snapshot"}));
        let raw = native.snapshot_v1();
        let before = world(&wire).clone();
        step(&mut wire, &mut native, &mut seq, |a| {
            matches!(a, A::Pass { .. })
        });
        let restored = send(
            &mut wire,
            &mut seq,
            json!({"request_type":"restore","snapshot_id":saved["snapshot_id"]}),
        );
        native.genesis_wave_restore_v69(&raw).unwrap();
        assert_eq!(world(&wire), &before);
        assert_eq!(world(&wire), native.genesis_wave_fixture_state());
        assert_eq!(
            restored["state"]["decision"]["state"]["end_turn"]["disposals"],
            r["state"]["decision"]["state"]["end_turn"]["disposals"]
        );
    }
}
