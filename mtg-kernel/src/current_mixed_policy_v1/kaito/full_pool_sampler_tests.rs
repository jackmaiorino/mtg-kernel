use super::*;
use crate::{
    current_tree_learner_v1::ModelV1,
    ids::PlayerId,
    limited_full_pool_consumer_v1 as full,
    limited_match_v1::LimitedBeliefV1,
    policy_observation_v6::tests::put,
    policy_surface_v5::LimitedAppendixV1,
    state::{GameState, Zone},
};

mod burn_draw;
mod sacrifice_morbid;

fn card(name: &str) -> u16 {
    crate::card_def::card_id_by_name(name).unwrap()
}
fn decision(owner: &KaitoSession) -> KaitoDecision {
    let FastActorResponseV1::Decision(expected) = owner.current_response() else {
        panic!("full-pool sampler lost current decision")
    };
    owner.encode(expected).unwrap()
}

fn modes(state: &mut GameState) {
    state.curator_unordered_reveal_v1 = true;
    state.greatwurm_commit_receipts_v1 = true;
    state.harvester_commit_receipts_v1 = true;
    state.fiend_artisan_receipts_v1 = true;
    state.etali_commit_receipts_v1 = true;
    state.thousand_year_storm_consumer_v1 = true;
    state.reveal_knowledge_v4 = Some(Default::default());
    state.reveal_meaning_v3.enable();
}

fn scene(
    actor: PlayerId,
    two_swamps: bool,
    distinct_reveal: bool,
) -> (KaitoSession, LimitedBeliefV1) {
    let mut state = crate::effect::thousand_year_storm_v1_tests::ready(actor, "Forest");
    modes(&mut state);
    put(&mut state, actor, "Burst Lightning", Zone::Hand);
    let opponent = actor.opponent();
    let first = put(&mut state, opponent, "Swamp", Zone::Hand);
    let second = put(
        &mut state,
        opponent,
        if two_swamps { "Swamp" } else { "Forest" },
        Zone::Hand,
    );
    put(&mut state, opponent, "Island", Zone::Hand);
    state
        .reveal_distinct_hand_card(actor, opponent, first)
        .unwrap();
    state
        .reveal_distinct_hand_card(
            actor,
            opponent,
            if distinct_reveal { second } else { first },
        )
        .unwrap();
    let mut corpus = state.players[opponent.index()]
        .battlefield
        .iter()
        .map(|&id| state.objects.get(id).card_def)
        .collect::<Vec<_>>();
    corpus.extend([card("Swamp"), card("Island")]);
    corpus.extend(std::iter::repeat_n(card("Forest"), 65));
    let owner = KaitoSession::from_session(
        FastActorSessionV1::from_kaito_v75_fixture_state_scoped(state, LimitedAppendixV1::FullPool),
    )
    .unwrap();
    (owner, LimitedBeliefV1::new([corpus]))
}

#[test]
fn full_pool_closed_list_preserves_complete_packet_native_actions_and_owner_isolation() {
    for first in [crate::rl::PlayerSeatV1::P0, crate::rl::PlayerSeatV1::P1] {
        let config = full::ResetV1 {
            expected_card_db_hash: crate::card_def::KERNEL_CARDDB_HASH,
            episode_id: 552,
            env_seed: 552_286,
            starting_player: first,
            deck_ids: ["full-left".into(), "full-right".into()],
            mainboards: [vec![card("Forest"); 40], vec![card("Island"); 40]],
            max_physical_decisions: 2000,
            max_policy_steps: 2000,
        };
        let owner = full::reset_numeric_reveal_facts_v3(&config).unwrap();
        let d = decision(&owner);
        let belief = LimitedBeliefV1::new([vec![card("Forest"); 40], vec![card("Island"); 40]]);
        let before = owner.fixture_state().clone();
        for seed in [552_001, 552_002] {
            let mut sampled = owner
                .sampled_full_pool_closed_list_v1(&d, seed, &belief)
                .unwrap();
            let again = owner
                .sampled_full_pool_closed_list_v1(&d, seed, &belief)
                .unwrap();
            assert_eq!(sampled.fixture_state(), again.fixture_state());
            let fresh = decision(&sampled);
            assert_eq!(fresh.packet(), d.packet());
            assert!(sampled.training_input_v1(&d).is_err());
            assert!(owner.training_input_v1(&fresh).is_err());
            let saved = sampled.snapshot();
            sampled.restore(&saved).unwrap();
            assert!(sampled.training_input_v1(&fresh).is_err());
            let fresh = decision(&sampled);
            assert_eq!(fresh.packet(), d.packet());
            let native_index = fresh
                .fixture_backend_v75()
                .fixture_original_index_v75(0)
                .unwrap();
            let mut native = sampled.inner.fixture_native_clone_v75();
            let expected = fresh.fixture_backend_v75().expected();
            assert_eq!(
                sampled.apply(&fresh, 0).unwrap(),
                native
                    .step(expected.episode_id, expected.step, native_index)
                    .unwrap()
            );
            assert_eq!(sampled.fixture_state(), native.kaito_fixture_state());
        }
        assert_eq!(owner.fixture_state(), &before);
    }
}

#[test]
fn full_pool_closed_list_samples_revealed_names_without_physical_copy_or_multiplicity_leaks() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (left, belief) = scene(actor, true, false);
        let base = decision(&left);
        let model = ModelV1::deterministic_fixture();
        for (two, distinct) in [(true, true), (false, false)] {
            let (right, _) = scene(actor, two, distinct);
            let other = decision(&right);
            assert_eq!(base.packet(), other.packet());
            for seed in [552_003, 552_004] {
                let a = left
                    .sampled_full_pool_closed_list_v1(&base, seed, &belief)
                    .unwrap();
                let b = right
                    .sampled_full_pool_closed_list_v1(&other, seed, &belief)
                    .unwrap();
                let da = decision(&a);
                let db = decision(&b);
                assert_eq!(da.packet(), db.packet());
                let ia = a.training_input_v1(&da).unwrap();
                let ib = b.training_input_v1(&db).unwrap();
                assert_eq!(ia.packet(), ib.packet());
                assert_eq!(ia.action_flags(), ib.action_flags());
                let logits = model.forward(&ia).unwrap();
                assert!(logits.iter().all(|v| v.is_finite()));
                assert_eq!(logits, model.forward(&ib).unwrap());
                let definitions = |owner: &KaitoSession| {
                    owner.fixture_state().players[actor.opponent().index()]
                        .hand
                        .iter()
                        .chain(&owner.fixture_state().players[actor.opponent().index()].library)
                        .map(|&id| owner.fixture_state().objects.get(id).card_def)
                        .collect::<Vec<_>>()
                };
                assert_eq!(definitions(&a), definitions(&b));
            }
        }
    }
}

#[test]
fn full_pool_closed_list_rejects_bad_corpus_and_foreign_decisions_without_mutating_owner() {
    let (owner, _) = scene(PlayerId::P0, true, false);
    let (foreign, belief) = scene(PlayerId::P0, true, false);
    let d = decision(&owner);
    let before = owner.fixture_state().clone();
    assert!(foreign
        .sampled_full_pool_closed_list_v1(&d, 1, &belief)
        .is_err());
    for invalid in [
        LimitedBeliefV1::default(),
        LimitedBeliefV1::new([vec![u16::MAX; 40]]),
        LimitedBeliefV1::new([vec![card("Lightning Bolt"); 40]]),
    ] {
        assert!(owner
            .sampled_full_pool_closed_list_v1(&d, 1, &invalid)
            .is_err());
    }
    assert!(owner.sampled_clone(&d, 1).is_err());
    assert_eq!(owner.fixture_state(), &before);
}
