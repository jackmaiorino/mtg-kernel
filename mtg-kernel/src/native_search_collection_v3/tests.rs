use super::*;
use crate::async_flat_scored_rollout_v1::select_search_root_for_collection_v3;
use crate::ids::PlayerId;
use crate::kernel_native_search_opponent_v1::{
    KernelNativeSearchTierV1, KERNEL_NATIVE_SEARCH_AUTHORIZED_SEEDS_V1,
};

fn authority() -> KernelNativeSearchAuthorityV1 {
    KernelNativeSearchAuthorityV1::current_v3(
        KernelNativeSearchTierV1::T512,
        KERNEL_NATIVE_SEARCH_AUTHORIZED_SEEDS_V1[0],
        crate::state::DIAGNOSTIC_STATE_HASH_ALGORITHM,
    )
    .unwrap()
}
fn population() -> NativeSearchPopulationV3 {
    NativeSearchPopulationV3::new([1; 8], std::array::from_fn(|_| authority())).unwrap()
}

#[test]
fn fresh_population_requires_explicit_v3_authorities() {
    let mut old = authority();
    old.algorithm_identity =
        crate::kernel_native_search_opponent_v1::KERNEL_NATIVE_SEARCH_ALGORITHM_V1.to_string();
    old.node_key_identity =
        crate::kernel_native_search_opponent_v1::KERNEL_NATIVE_SEARCH_NODE_KEY_V1.to_string();
    assert!(NativeSearchPopulationV3::new([1; 8], std::array::from_fn(|_| old.clone())).is_err());
    let pop = population();
    assert!(pop.0.supports_search_collection_v3());
    let selected: std::collections::BTreeSet<_> = (0..32)
        .map(|episode| {
            let slot = pop.0.slot_for_episode_v1(7101, episode).unwrap();
            assert!(pop
                .0
                .search_authority_for_slot_v1(slot)
                .unwrap()
                .authority()
                .uses_v4_contract_v3());
            slot.index_v1()
        })
        .collect();
    assert!(selected.len() > 1);
}

#[test]
fn fresh_learner_and_population_dispatch_bind_library_roots_without_v2_cache() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for form in 0..3 {
            let names = if form == 1 {
                vec!["Squadron Hawk", "Forest", "Squadron Hawk", "Island"]
            } else {
                vec!["Forest", "Island", "Snow-Covered Forest", "Lightning Bolt"]
            };
            let state = crate::rl_session::search_library_fixture_v3(
                actor, form, &names, true, false, false,
            );
            let original_v2 = FastActorSessionV1::from_v2_search_fixture_state_v3(state.clone());
            let mut session = FastActorSessionV1::from_v3_fixture_state(state);
            let FastActorResponseV1::Decision(d) = session.current_response() else {
                panic!("root")
            };
            if form != 1 {
                assert!(original_v2
                    .native_full_trajectory_current_binding_v2(d)
                    .is_err());
            }
            let before = session.privileged_core_environment_hash();
            let packet = Family::encode_packet(
                &session,
                d,
                &mut FlatDecisionEncoderV4::default(),
                Packet::default(),
            )
            .unwrap();
            assert!(packet.view().extensions().decision_local_library.is_some());
            let payload = Family::test_safe_packet_payload(&packet);
            for forbidden in [
                "arena_id",
                "rng",
                "randomization",
                "raw_state",
                "library_position",
            ] {
                assert!(!payload.contains(forbidden));
            }
            let pop = population();
            let slot = pop.0.slot_for_episode_v1(7101, 23).unwrap();
            let search = pop.0.search_authority_for_slot_v1(slot).unwrap();
            let (index, rejected) =
                select_search_root_for_collection_v3(search, &session, d, true).unwrap();
            assert!(!rejected);
            assert_eq!(session.privileged_core_environment_hash(), before);
            assert_eq!(
                Family::native_full_trajectory_commitment(Family::packet_binding(&packet)).unwrap(),
                Family::native_full_trajectory_opponent_commitment(&session, d).unwrap()
            );
            let mut learner = session.clone();
            let direct = session.step(d.episode_id, d.step, index).unwrap();
            assert_eq!(
                Family::consume(&mut learner, Family::packet_binding(&packet), index).unwrap(),
                direct
            );
            assert_eq!(
                session.privileged_core_environment_hash(),
                learner.privileged_core_environment_hash()
            );
        }
    }
}

struct ZeroScorer;
impl NativeSearchBatchScorerV3 for ZeroScorer {
    fn score_batch(
        &mut self,
        contract: NativeSearchScorerContractV3,
        views: &[NativeSearchDecisionViewV3<'_>],
        offsets: &[usize],
        logits: &mut [f32],
        values: &mut [f32],
    ) -> Result<(), FlatBatchScorerErrorV1> {
        assert_eq!(contract.scorer_packet_version, 4);
        assert_eq!(views.len(), values.len());
        assert_eq!(offsets.last(), Some(&logits.len()));
        logits.fill(0.0);
        values.fill(0.0);
        Ok(())
    }
}

#[test]
fn fresh_native_eight_slot_collection_finishes_and_replays_receipts() {
    let config = AsyncRolloutConfigV2 {
        deck_ids: ["Burn".to_string(), "Burn".to_string()],
        learner_seat: crate::rl::PlayerSeatV1::P0,
        environment_seed: 7101,
        opponent_policy_seed: 7201,
        learner_policy_seed: 7301,
        max_physical_decisions: 10_000,
        max_policy_steps: 100_000,
        worker_count: 1,
        sessions_per_worker: 1,
        broker_batch_target: 1,
        first_episode_id: 0,
        episode_count: 1,
        scheduler_timeout: std::time::Duration::from_secs(120),
        measure_broker_service_time: false,
        starting_player: None,
    };
    let first =
        collect_native_search_population_v3(config.clone(), 7101, population(), &mut ZeroScorer)
            .unwrap();
    let replay =
        collect_native_search_population_v3(config, 7101, population(), &mut ZeroScorer).unwrap();
    assert_eq!(first.receipts.len(), 1);
    assert_eq!(first.receipts, replay.receipts);
    assert_eq!(first.result.episodes, replay.result.episodes);
    assert!(!first.learner_decisions.is_empty());
    assert_eq!(
        first.receipts[0].learner_policy_step_count(),
        first.learner_decisions.len() as u64
    );
    assert_eq!(
        first.receipts[0].search_authority_sha256(),
        authority().digest().unwrap()
    );
    assert_eq!(
        first.receipts[0].identity(),
        crate::native_search_trajectory_v3::NATIVE_SEARCH_TRAJECTORY_IDENTITY_V3
    );
}

#[test]
fn lawful_reference_rejection_uses_validated_visible_menu_once() {
    for reverse in [false, true] {
        let mut state = crate::rl_session::search_library_fixture_v3(
            PlayerId::P0,
            0,
            &["Forest", "Island", "Snow-Covered Forest", "Lightning Bolt"],
            reverse,
            false,
            false,
        );
        let hidden = state.players[0].library[0];
        state
            .engine
            .until_end_of_turn
            .push(crate::engine::UntilEndOfTurnEffect::SyntheticMarker(hidden));
        let mut session = FastActorSessionV1::from_v3_fixture_state(state);
        let FastActorResponseV1::Decision(d) = session.current_response() else {
            panic!("root")
        };
        let search = KernelNativeSearchOpponentV1::new(authority()).unwrap();
        assert_eq!(
            search.select_action(&session, d),
            Err(KernelNativeSearchErrorV1::DeterminizationReferenceConflict)
        );
        let before = session.privileged_core_environment_hash();
        let (selected, rejected) =
            select_search_root_for_collection_v3(&search, &session, d, true).unwrap();
        assert!(rejected);
        assert_eq!(before, session.privileged_core_environment_hash());
        let mut stale = d;
        stale.step += 1;
        assert_eq!(
            select_search_root_for_collection_v3(&search, &session, stale, true),
            Err(KernelNativeSearchErrorV1::InvalidDecision)
        );
        assert_eq!(
            select_search_root_for_collection_v3(&search, &session, d, false),
            Err(KernelNativeSearchErrorV1::DeterminizationReferenceConflict)
        );
        session.step(d.episode_id, d.step, selected).unwrap();
        assert!(session.kernel_search_state_v1().engine.halted.is_none());
    }
}

#[test]
fn lembas_shuffled_below_its_etb_trigger_remains_a_real_steppable_choice() {
    use crate::engine::{self, Action, Decision};
    use crate::policy_observation_v6::tests::{put, ready_state};
    use crate::state::Zone;
    let mut state = ready_state();
    let source = put(&mut state, PlayerId::P0, "Lembas", Zone::Hand);
    for name in [
        "Forest", "Island", "Mountain", "Swamp", "Forest", "Island", "Mountain", "Swamp",
    ] {
        put(&mut state, PlayerId::P0, name, Zone::Library);
    }
    for name in ["Forest", "Island", "Mountain"] {
        put(&mut state, PlayerId::P1, name, Zone::Library);
    }
    state.players[0].mana_pool[crate::mana::ManaColor::C.pool_index()] = 8;
    engine::step(&mut state, Action::CastSpell(source)).unwrap();
    let mut sacrificed = false;
    let mut choice = false;
    for _ in 0..32 {
        match engine::advance_until_decision(&mut state) {
            Decision::CastSpellOrPass { player, .. } => {
                if !sacrificed
                    && player == PlayerId::P0
                    && state.objects.get(source).zone == Zone::Battlefield
                {
                    engine::step(&mut state, Action::ActivateAbility(source, 0)).unwrap();
                    sacrificed = true;
                } else {
                    engine::step(&mut state, Action::Pass).unwrap();
                }
            }
            Decision::ChooseEffectTargets { .. } => {
                choice = true;
                break;
            }
            decision => panic!("unexpected Lembas decision {decision:?}"),
        }
    }
    assert!(sacrificed && choice);
    assert_eq!(state.objects.get(source).zone, Zone::Library);
    let mut session = FastActorSessionV1::from_v3_fixture_state(state);
    let FastActorResponseV1::Decision(d) = session.current_response() else {
        panic!("Lembas root")
    };
    let search = KernelNativeSearchOpponentV1::new(authority()).unwrap();
    let (selected, _) = select_search_root_for_collection_v3(&search, &session, d, true).unwrap();
    session.step(d.episode_id, d.step, selected).unwrap();
    assert!(session.kernel_search_state_v1().engine.halted.is_none());
}
