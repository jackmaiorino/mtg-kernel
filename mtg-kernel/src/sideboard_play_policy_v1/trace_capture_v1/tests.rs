use super::*;
use crate::paired_bo1_harness_v1::{PairedBo1PolicyInputV1, PairedBo1PolicyV1};
use crate::rl_session::FastActorResponseV1;

fn path(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "gameplay-trace-{}-{name}.jsonl",
        std::process::id()
    ))
}
fn config(path: std::path::PathBuf, decisions: u64) -> GameplayTraceConfigV1 {
    GameplayTraceConfigV1 {
        path,
        run_id: "test".into(),
        max_decisions: decisions,
        max_bytes: 4 * 1024 * 1024,
        max_record_bytes: 1024 * 1024,
    }
}

#[test]
fn gameplay_trace_preserves_sampling_binding_transition_and_limits() {
    let destination = path("equivalence");
    let _ = std::fs::remove_file(&destination);
    let mut plain = FrozenPlayPolicyV1::training_fixture_v4();
    let mut traced = FrozenPlayPolicyV1::training_fixture_v4();
    plain.reset_sampling_v1([71, 73]);
    traced.reset_sampling_v1([71, 73]);
    traced
        .enable_gameplay_trace_v1(config(destination.clone(), 2))
        .unwrap();
    let state = crate::policy_observation_v6::tests::forest_search_state(false, "Lightning Bolt");
    let mut a = FastActorSessionV1::from_v3_fixture_state(state.clone());
    let mut b = FastActorSessionV1::from_v3_fixture_state(state);
    for _ in 0..4 {
        let FastActorResponseV1::Decision(d) = a.current_response() else {
            break;
        };
        assert_eq!(a.current_response(), b.current_response());
        let x = plain
            .select_action_v1(PairedBo1PolicyInputV1::new(&a, d))
            .unwrap();
        let y = traced
            .select_action_v1(PairedBo1PolicyInputV1::new(&b, d))
            .unwrap();
        assert_eq!(x, y);
        assert_eq!(plain.seat_rng, traced.seat_rng);
        assert_eq!(
            a.step(d.episode_id, d.step, x).unwrap(),
            b.step(d.episode_id, d.step, y).unwrap()
        );
        assert_eq!(a.diagnostic_state_hash(), b.diagnostic_state_hash());
    }
    drop(a);
    drop(b);
    drop(traced);
    let rows: Vec<Value> = std::fs::read_to_string(&destination)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(rows.iter().filter(|r| r["kind"] == "decision").count(), 2);
    let first = &rows[1];
    assert_eq!(first["transition"]["applied"], true);
    assert_eq!(
        first["actor"],
        first["transition"]["observation"]["acting_player"]
    );
    assert_eq!(
        first["selected_index"],
        first["bound_engine_action"]["engine_index"]
    );
    let masses: Vec<u128> = first["behavior"]["mass_numerators"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().parse().unwrap())
        .collect();
    assert_eq!(masses.iter().sum::<u128>(), 1u128 << 64);
    assert_eq!(rows.last().unwrap()["stop_reason"], "decision_limit");
    std::fs::remove_file(destination).unwrap();
}

#[test]
fn gameplay_trace_excludes_unseen_cards_and_library_order() {
    let mut records = Vec::new();
    for (n, hidden) in ["Lightning Bolt", "Counterspell"].iter().enumerate() {
        let destination = path(&format!("private-{n}"));
        let _ = std::fs::remove_file(&destination);
        let state = crate::policy_observation_v6::tests::forest_search_state(false, hidden);
        let mut session = FastActorSessionV1::from_v3_fixture_state(state);
        let mut policy = FrozenPlayPolicyV1::training_fixture_v4();
        policy.reset_sampling_v1([71, 73]);
        policy
            .enable_gameplay_trace_v1(config(destination.clone(), 2))
            .unwrap();
        let FastActorResponseV1::Decision(d) = session.current_response() else {
            panic!()
        };
        let selected = policy
            .select_action_v1(PairedBo1PolicyInputV1::new(&session, d))
            .unwrap();
        // A search/snapshot fork must not emit a duplicate receipt.
        let mut fork = session.clone();
        fork.step(d.episode_id, d.step, selected).unwrap();
        drop(fork);
        session.step(d.episode_id, d.step, selected).unwrap();
        drop(session);
        drop(policy);
        let rows: Vec<Value> = std::fs::read_to_string(&destination)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(rows.len(), 3);
        let mut record = rows[1].clone();
        record.as_object_mut().unwrap().remove("transition");
        records.push(record);
        std::fs::remove_file(destination).unwrap();
    }
    assert_eq!(records[0], records[1]);
}
