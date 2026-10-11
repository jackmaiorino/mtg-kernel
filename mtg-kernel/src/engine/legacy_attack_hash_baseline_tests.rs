//! Default retains the accepted a957 hashes; Standard binds accepted main2f55986.
//! Matched both-seat controls preserve the historical Standard receipts.
use super::*;
use crate::policy_observation_v6::tests::{put, ready_state};
use crate::rl_session::{FastActorResponseV1, FastActorSessionV1};

#[cfg(not(feature = "standard-magezero-fixtures"))]
#[test]
fn accepted_default_actual_attack_hash_baseline_both_seats() {
    actual_attack_hash_baseline_both_seats(
        [
            (0x47fb83da9cd811df, 0xcbc2eaf4943736a8),
            (0x021b8dd6a3fb2f43, 0x24100b6d3bdf8d9a),
        ],
        "a957c358db9326cb3eacff819d6bcea38678aeb3",
        "accepted-default-actual-attack-hash-baseline-1",
    );
}

#[cfg(feature = "standard-magezero-fixtures")]
#[test]
fn accepted_current_main_standard_actual_attack_hash_baseline_both_seats() {
    actual_attack_hash_baseline_both_seats(
        [
            (0x0b888429a46bd910, 0x4c26f4054b11987a),
            (0xdfb303462e23d089, 0x1c6f5f450ebca677),
        ],
        "2f55986c1e5daaf4d51b50520091bc4d266e1c75",
        "accepted-current-main-standard-actual-attack-hash-baseline-1",
    );
}

fn actual_attack_hash_baseline_both_seats(
    expected_rows: [(u64, u64); 2],
    accepted_base: &str,
    schema: &str,
) {
    let mut rows = Vec::new();
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state();
        state.turn = 3;
        state.active_player = actor;
        state.priority_player = actor;
        state.step = Step::DeclareAttackers;
        let attacker = put(&mut state, actor, "Guttersnipe", Zone::Battlefield);
        // Preserve a real post-declaration priority decision rather than
        // allowing forced passes to advance into the next turn.
        put(&mut state, actor, "Lightning Bolt", Zone::Hand);
        state.players[actor.index()].mana_pool[mana::ManaColor::R.pool_index()] = 1;
        apply_declare_attackers(&mut state, vec![attacker]).unwrap();
        assert!(state.attack_declaration_turn_v1.is_none());
        let diagnostic = state.diagnostic_state_hash();
        let session = FastActorSessionV1::from_v2_search_fixture_state_v3(state);
        let FastActorResponseV1::Decision(decision) = session.current_response() else {
            panic!("the actual attack boundary must retain a playable decision");
        };
        assert_eq!(decision.acting_player, actor.into());
        assert_eq!(session.game_state().turn, 3);
        assert_eq!(session.game_state().step, Step::DeclareAttackers);
        assert_eq!(session.game_state().engine.combat.attackers, vec![attacker]);
        let core = session.privileged_core_environment_hash();
        let (expected_diagnostic, expected_core) = expected_rows[actor.index()];
        assert_eq!(diagnostic, expected_diagnostic);
        assert_eq!(session.diagnostic_state_hash(), expected_diagnostic);
        assert_eq!(core, expected_core);
        let snapshot = session.snapshot_v1();
        let mut restored = session.clone();
        restored.restore_v1(&snapshot);
        assert_eq!(restored.privileged_core_environment_hash(), core);
        assert_eq!(
            restored.diagnostic_state_hash(),
            session.diagnostic_state_hash()
        );
        rows.push(serde_json::json!({
            "actor": format!("p{}", actor.index()),
            "diagnostic_state_hash": format!("{diagnostic:016x}"),
            "session_diagnostic_state_hash": format!("{:016x}", session.diagnostic_state_hash()),
            "privileged_core_environment_hash": format!("{core:016x}"),
        }));
    }
    let artifact = serde_json::json!({
        "schema": schema,
        "accepted_base": accepted_base,
        "standard_magezero_fixtures": cfg!(feature = "standard-magezero-fixtures"),
        "limited_fdn_fixtures": cfg!(feature = "limited-fdn-fixtures"),
        "rows": rows,
    });
    println!("{}", serde_json::to_string(&artifact).unwrap());
    if let Ok(path) = std::env::var("MTG_LEGACY_ATTACK_HASH_BASELINE_EXPORT") {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(path)
            .unwrap();
        file.write_all(&serde_json::to_vec(&artifact).unwrap())
            .unwrap();
        file.sync_all().unwrap();
    }
}
