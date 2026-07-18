use mtg_kernel::async_rollout_v2::{run_seeded_uniform_async_rollout_v2, AsyncRolloutConfigV2};
use mtg_kernel::rl::{
    PlayerSeatV1, TerminalClassificationV1, TerminalOutcomeV1, TerminalSafeCodeV2,
};
use std::time::Duration;

#[test]
fn legacy_uniform_v2_c625d9a_golden_is_unchanged() {
    let result = run_seeded_uniform_async_rollout_v2(AsyncRolloutConfigV2 {
        deck_ids: ["Rally".to_string(), "Rally".to_string()],
        learner_seat: PlayerSeatV1::P0,
        environment_seed: 71_501,
        opponent_policy_seed: 72_501,
        learner_policy_seed: 73_501,
        max_physical_decisions: 5_000,
        max_policy_steps: 640_000,
        worker_count: 2,
        sessions_per_worker: 2,
        broker_batch_target: 4,
        first_episode_id: 0,
        episode_count: 16,
        scheduler_timeout: Duration::from_secs(30),
        measure_broker_service_time: true,
    })
    .unwrap();

    assert_eq!(result.policy_step_count, 4_209);
    assert_eq!(result.physical_decision_count, 3_678);
    assert_eq!(result.metrics.learner_action_count, 2_047);
    assert_eq!(result.metrics.terminal_notifications, 16);
    assert_eq!(result.metrics.complete_round_count, 689);
    assert_eq!(result.metrics.batch_publication_count, 688);
    assert_eq!(result.metrics.batch_width_sum, 2_047);
    assert_eq!(result.metrics.max_batch_width, 4);
    assert_eq!(result.metrics.target_flush_count, 351);
    assert_eq!(result.metrics.quiescent_flush_count, 337);
    assert_eq!(
        result.metrics.batch_membership_digest,
        [
            0x03, 0xdb, 0xea, 0x56, 0x45, 0x50, 0x3a, 0xed, 0x3f, 0xf5, 0x77, 0x0c, 0x2a, 0x8e,
            0x9b, 0x60, 0x2d, 0x6a, 0x25, 0xff, 0x66, 0xeb, 0x9c, 0x15, 0x74, 0xd2, 0xfa, 0xd4,
            0xbb, 0x35, 0xb5, 0xb3,
        ]
    );

    let expected = [
        (
            TerminalOutcomeV1::P1Win,
            335,
            287,
            141,
            0x41c5_02d9_db2c_f1ab,
        ),
        (TerminalOutcomeV1::P1Win, 115, 96, 39, 0xf1ce_57b3_21b5_d22b),
        (
            TerminalOutcomeV1::P1Win,
            243,
            201,
            124,
            0x01eb_5e45_fb30_136d,
        ),
        (
            TerminalOutcomeV1::P0Win,
            206,
            186,
            91,
            0xbbd8_260e_2fbc_0b13,
        ),
        (
            TerminalOutcomeV1::P1Win,
            234,
            204,
            115,
            0x522e_a60f_0547_2e1b,
        ),
        (
            TerminalOutcomeV1::P1Win,
            529,
            472,
            253,
            0x971d_087c_2abc_1de1,
        ),
        (
            TerminalOutcomeV1::P0Win,
            287,
            248,
            158,
            0xeaa0_a3da_2585_a043,
        ),
        (
            TerminalOutcomeV1::P1Win,
            198,
            181,
            116,
            0xbd84_3e0f_b265_ae36,
        ),
        (
            TerminalOutcomeV1::P1Win,
            140,
            118,
            65,
            0xdf73_3ae9_69c8_427d,
        ),
        (
            TerminalOutcomeV1::P1Win,
            480,
            429,
            230,
            0x7ac7_709f_e9dd_3bf3,
        ),
        (
            TerminalOutcomeV1::P0Win,
            262,
            239,
            115,
            0x9199_fade_5718_2660,
        ),
        (
            TerminalOutcomeV1::P1Win,
            120,
            106,
            56,
            0x1836_29d0_3a35_699f,
        ),
        (
            TerminalOutcomeV1::P0Win,
            282,
            231,
            148,
            0x5d84_3c7c_875b_764f,
        ),
        (
            TerminalOutcomeV1::P0Win,
            346,
            290,
            163,
            0xc485_f920_d6e4_906c,
        ),
        (
            TerminalOutcomeV1::P1Win,
            261,
            232,
            138,
            0x9e0a_6484_b7d4_7947,
        ),
        (
            TerminalOutcomeV1::P0Win,
            171,
            158,
            95,
            0x9496_9c1a_fa74_9955,
        ),
    ];
    assert_eq!(result.episodes.len(), expected.len());
    for (episode_id, (episode, expected)) in result.episodes.iter().zip(expected).enumerate() {
        let (outcome, policy, physical, learner, trace) = expected;
        let winner = match outcome {
            TerminalOutcomeV1::P0Win => PlayerSeatV1::P0,
            TerminalOutcomeV1::P1Win => PlayerSeatV1::P1,
            _ => unreachable!(),
        };
        let reward = if winner == PlayerSeatV1::P0 {
            [1, -1]
        } else {
            [-1, 1]
        };
        assert_eq!(episode.terminal.episode_id, episode_id as u64);
        assert_eq!(episode.terminal.terminal_outcome, outcome);
        assert_eq!(
            episode.terminal.terminal_classification,
            TerminalClassificationV1::Natural
        );
        assert_eq!(
            episode.terminal.terminal_code,
            TerminalSafeCodeV2::NaturalGameOver
        );
        assert_eq!(episode.terminal.winner, Some(winner));
        assert_eq!(episode.terminal.terminal_reward, reward);
        assert_eq!(episode.terminal.policy_step_count, policy);
        assert_eq!(episode.terminal.physical_decision_count, physical);
        assert_eq!(episode.learner_action_count, learner);
        assert_eq!(episode.learner_trace_hash, trace);
    }
}
