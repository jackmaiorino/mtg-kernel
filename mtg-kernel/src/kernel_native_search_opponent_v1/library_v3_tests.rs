use super::*;

fn authority() -> KernelNativeSearchAuthorityV1 {
    KernelNativeSearchAuthorityV1::current_v3(
        KernelNativeSearchTierV1::T512,
        KERNEL_NATIVE_SEARCH_AUTHORIZED_SEEDS_V1[0],
        crate::state::DIAGNOSTIC_STATE_HASH_ALGORITHM,
    )
    .unwrap()
}

#[test]
fn v3_authority_is_distinct_and_legacy_default_is_unchanged() {
    let new = authority();
    let old = KernelNativeSearchAuthorityV1::current(
        new.tier,
        new.action_seed,
        &new.private_diagnostic_identity,
    )
    .unwrap();
    assert_eq!(old.algorithm_identity, KERNEL_NATIVE_SEARCH_ALGORITHM_V1);
    assert_eq!(old.node_key_identity, KERNEL_NATIVE_SEARCH_NODE_KEY_V1);
    assert!(old.matches_fresh_reconstruction_v1());
    assert!(new.matches_fresh_reconstruction_v1());
    assert_ne!(old.digest().unwrap(), new.digest().unwrap());
    let mut wrong = new;
    wrong.node_key_identity = KERNEL_NATIVE_SEARCH_NODE_KEY_V1.to_string();
    assert_eq!(
        wrong.validate(),
        Err(KernelNativeSearchErrorV1::InvalidAuthority)
    );
}

#[test]
fn v3_search_handles_library_roots_in_both_contracts_and_maps_physical_indices() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for form in 0..3 {
            let names = if form == 1 {
                vec!["Squadron Hawk", "Forest", "Squadron Hawk", "Island"]
            } else {
                vec!["Forest", "Island", "Snow-Covered Forest", "Lightning Bolt"]
            };
            for v2 in [false, true] {
                let state = crate::rl_session::search_library_fixture_v3(
                    actor, form, &names, true, false, false,
                );
                let session = if v2 {
                    FastActorSessionV1::from_v2_search_fixture_state_v3(state)
                } else {
                    FastActorSessionV1::from_v3_fixture_state(state)
                };
                let FastActorResponseV1::Decision(d) = session.current_response() else {
                    panic!("library search root")
                };
                let original = session.privileged_core_environment_hash();
                if v2 && form != 1 {
                    assert!(session
                        .native_full_trajectory_current_binding_v2(d)
                        .is_err());
                }
                let searcher = KernelNativeSearchOpponentV1::new(authority()).unwrap();
                let first = searcher.select_action(&session, d).unwrap();
                assert_eq!(first, searcher.select_action(&session, d).unwrap());
                assert_eq!(first.root_action_stats.len(), d.legal_action_count as usize);
                assert_eq!(session.privileged_core_environment_hash(), original);
                for (index, stat) in first.root_action_stats.iter().enumerate() {
                    assert_eq!(stat.flat_action_index, index as u32);
                    assert!(stat.visits > 0);
                    let mut live = session.clone();
                    live.step(d.episode_id, d.step, stat.flat_action_index)
                        .unwrap();
                    assert!(live.kernel_search_state_v1().engine.halted.is_none());
                }
                let (adapted, mapping) = session.kernel_search_v4_root_clone_v3(d).unwrap();
                let before = session.diagnostic_current_action_semantics().unwrap();
                let after = adapted.diagnostic_current_action_semantics().unwrap();
                for (new, old) in mapping.iter().enumerate() {
                    assert_eq!(after[new], before[*old as usize]);
                }
            }
        }
    }
}

#[test]
fn census_rejects_relabeling_hidden_frozen_sources_without_pinning() {
    let (mut state, source, _, _) =
        crate::rl_session::avenging_hunter_undercity_arena_choose_targets_state_v1(false);
    crate::rl_session::shuffle_trigger_source_into_library_v1(&mut state, source, PlayerId::P0);
    for name in ["Forest", "Island", "Mountain"] {
        crate::policy_observation_v6::tests::put(&mut state, PlayerId::P0, name, Zone::Library);
    }
    let session = FastActorSessionV1::from_v3_fixture_state(state);
    let initial = session.privileged_core_environment_hash();
    let mut rejected = 0;
    let mut admitted = 0;
    for seed in 1..=32 {
        let mut unguarded = session.kernel_search_state_v1().clone();
        redeterminize_hidden_zones_pinned_v1(&mut unguarded, PlayerId::P0, seed, &[]).unwrap();
        let changed = unguarded.objects.get(source).card_def
            != session
                .kernel_search_state_v1()
                .objects
                .get(source)
                .card_def;
        match session.census_redeterminized_clone_v1(seed) {
            Err(error) => {
                assert!(changed);
                assert_eq!(error, "hidden-source provenance conflict");
                rejected += 1;
            }
            Ok(sample) => {
                assert!(!changed);
                assert_eq!(sample.kernel_search_state_v1(), &unguarded);
                admitted += 1;
            }
        }
    }
    assert!(rejected > 0);
    assert!(admitted > 0);
    assert_eq!(session.privileged_core_environment_hash(), initial);
}
