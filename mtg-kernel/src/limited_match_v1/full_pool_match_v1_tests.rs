use super::*;
use crate::{
    effect::thousand_year_storm_v1_tests as paid,
    policy_observation_v6::tests::put,
    policy_surface_v5::LimitedAppendixV1,
    rl::ActionSemanticV1,
    state::{GameState, Step, Target},
};

fn limits(max_decisions: u64) -> GameLimitsV1 {
    GameLimitsV1 {
        max_turns: 20,
        max_decisions,
        full_validation: true,
        session: SessionKindV1::FullPoolV1,
    }
}

fn scene(actor: PlayerId, hidden: &str) -> (KaitoSession, LimitedBeliefV1) {
    let own_library = [card_id_by_name("Forest").unwrap(); 40];
    let other_library = [card_id_by_name(hidden).unwrap(); 40];
    let libraries = if actor == PlayerId::P0 {
        [&own_library, &other_library]
    } else {
        [&other_library, &own_library]
    };
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        libraries[0],
        libraries[1],
        crate::rl::card_name,
        552_286,
        actor,
    );
    state.step = Step::Main1;
    // Supply the real spell costs and one spare red mana action. The Storm
    // stress fixture's 160 lands add irrelevant branches to this search test.
    for _ in 0..4 {
        put(&mut state, actor, "Mountain", Zone::Battlefield);
    }
    put(&mut state, actor, "Swamp", Zone::Battlefield);
    put(&mut state, actor.opponent(), "Forest", Zone::Battlefield);
    FastActorSessionV1::enable_limited_native_modes_v1(&mut state, LimitedAppendixV1::FullPool);
    state.reveal_knowledge_v4 = Some(Default::default());
    state.reveal_meaning_v3.enable();
    let opponent = actor.opponent();
    // Short terminal scene. Both spells below are paid through native land taps.
    state.players[opponent.index()].life = 2;
    let partial = paid::cast(&mut state, actor, "Alesha, Who Laughs at Fate", &[], 3);
    let damage = paid::cast(
        &mut state,
        actor,
        "Burst Lightning",
        &[Target::Player(opponent)],
        1,
    );
    assert!(state.stack.iter().any(|row| row.source == partial));
    assert_eq!(state.stack.last().unwrap().source, damage);
    put(&mut state, actor, "Burst Lightning", Zone::Hand);

    let opposing = &state.players[opponent.index()];
    let mut public_corpus = opposing
        .battlefield
        .iter()
        .map(|&object| state.objects.get(object).card_def)
        .collect::<Vec<_>>();
    public_corpus.extend(std::iter::repeat_n(
        card_id_by_name("Forest").unwrap(),
        opposing.hand.len() + opposing.library.len(),
    ));
    let raw =
        FastActorSessionV1::from_kaito_v75_fixture_state_scoped(state, LimitedAppendixV1::FullPool);
    (
        KaitoSession::from_session(raw).unwrap(),
        LimitedBeliefV1::new([public_corpus]),
    )
}

#[test]
fn full_pool_terminal_pimc_plays_paid_partial_and_preserves_hidden_pair_both_seats() {
    let search = PolicySpecV1::parse("pimc:sims=2:depth=8:closedlist-full-pool-v1").unwrap();
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (left, belief) = scene(actor, "Forest");
        let (right, _) = scene(actor, "Island");
        let a = encode_current(&left).unwrap();
        let b = encode_current(&right).unwrap();
        assert_eq!(a.packet(), b.packet());
        assert_eq!(
            a.fixture_backend_v75().expected().acting_player,
            PlayerSeatV1::from(actor)
        );
        let before = left.fixture_state().clone();
        let pass = a
            .fixture_backend_v75()
            .actions()
            .iter()
            .position(|action| matches!(action, ActionSemanticV1::Pass { .. }))
            .unwrap();
        let sims = legal_count(&a).unwrap();
        assert!(sims > 1);
        let a_search = pimc_choose_full_pool_v1(&left, &a, sims, 8, 20, &belief, 552_286).unwrap();
        let b_search = pimc_choose_full_pool_v1(&right, &b, sims, 8, 20, &belief, 552_286).unwrap();
        assert_eq!(a_search, b_search);
        assert_eq!(a_search.1.visits.iter().sum::<u32>(), sims);
        assert_eq!(a_search.1.rejected, 0);
        assert_eq!(a_search.1.rollout_errors, 0);
        assert_eq!(a_search.1.value_sum[pass], 1.0);
        assert_eq!(left.fixture_state(), &before);

        let mut result = GameResultV1::empty();
        let mut policies = [&PolicySpecV1::Random; 2];
        policies[actor.index()] = &search;
        let mut beliefs = [None; 2];
        beliefs[actor.index()] = Some(&belief);
        let outcome =
            play_owner(left, policies, beliefs, 552_286, limits(24), &mut result).unwrap();
        assert_eq!(outcome, GameOutcomeV1::Winner(actor));
        assert!(result.searched_decisions > 0);
        assert_eq!(
            (
                result.search_fallbacks,
                result.rejected_worlds,
                result.rollout_errors
            ),
            (0, 0, 0)
        );
    }
}

#[test]
fn full_pool_match_reset_and_caps_are_distinct_from_natural_draw() {
    let forest = card_id_by_name("Forest").unwrap();
    let island = card_id_by_name("Island").unwrap();
    let decks = [&[forest; 40][..], &[island; 40][..]];
    let random = PolicySpecV1::Random;
    let search = PolicySpecV1::parse("pimc:sims=2:depth=1:closedlist-full-pool-v1").unwrap();
    for starting in [PlayerId::P0, PlayerId::P1] {
        let capped = play_game_v1(decks, [&random; 2], [None; 2], 552, starting, limits(0));
        assert_eq!(capped.outcome, GameOutcomeV1::Stall);
        assert_eq!(capped.decisions, 0);
        let missing = play_game_v1(
            decks,
            [&search, &random],
            [None; 2],
            552,
            starting,
            limits(0),
        );
        assert!(matches!(missing.outcome, GameOutcomeV1::Error(_)));
    }
    assert!(search
        .validate_full_pool_session_v1(SessionKindV1::PreparedV57)
        .is_err());
    // Match rows and the summary keep this policy's search counts and replay
    // identity, without the rules-only session requirement of v2 and v3.
    assert!(search.records_search_v1() && !search.uses_versioned_closed_list());
    assert!(PolicySpecV1::parse("pimc:closedlist-v3")
        .unwrap()
        .records_search_v1());
    assert!(!random.records_search_v1());
    assert!(!PolicySpecV1::parse("pimc:closedlist")
        .unwrap()
        .records_search_v1());
    assert!(PolicySpecV1::parse("pimc:closedlist-v3")
        .unwrap()
        .validate_full_pool_session_v1(SessionKindV1::FullPoolV1)
        .is_err());
}

fn step_zero_learner() -> PolicySpecV1 {
    use crate::current_tree_learner_v2::{LearnerV2, OptimizerConfigV2};
    let snapshot = LearnerV2::new(OptimizerConfigV2::campaign_v1(200))
        .unwrap()
        .snapshot();
    PolicySpecV1::LearnerCheckpoint(
        LearnerCheckpointV1::from_snapshot_v1("step_0000.json", "fixture", snapshot).unwrap(),
    )
}

#[test]
fn campaign_policy_specs_parse_and_stay_on_the_full_pool_owner() {
    let search = PolicySpecV1::parse("pimc:sims=16:depth=24:closedlist-full-pool-heuristic-v1")
        .unwrap();
    assert_eq!(
        search,
        PolicySpecV1::Pimc {
            sims: 16,
            depth: 24,
            world: WorldModeV1::FullPoolClosedListHeuristicV1,
        }
    );
    let heuristic = PolicySpecV1::parse("sampled-public-heuristic/v1").unwrap();
    assert_eq!(heuristic, PolicySpecV1::SampledPublicHeuristicV1);
    let learner = step_zero_learner();
    for policy in [&search, &heuristic] {
        assert!(policy.needs_belief() && policy.records_search_v1());
        assert!(policy.uses_full_pool_sampled_world_v1() && !policy.uses_full_pool_closed_list_v1());
    }
    assert!(!learner.needs_belief() && !learner.records_search_v1());
    for policy in [&search, &heuristic, &learner] {
        assert!(policy.is_campaign_evaluation_policy_v1());
        assert!(policy
            .validate_full_pool_session_v1(SessionKindV1::FullPoolV1)
            .is_ok());
        for raw in [
            SessionKindV1::PreparedV57,
            SessionKindV1::RulesOnly(RulesAdmissionV1::Admitted),
        ] {
            assert!(policy.validate_full_pool_session_v1(raw).is_err());
        }
    }
    // The existing policies keep their predicates.
    let terminal = PolicySpecV1::parse("pimc:sims=2:depth=8:closedlist-full-pool-v1").unwrap();
    assert!(terminal.uses_full_pool_closed_list_v1() && terminal.uses_full_pool_sampled_world_v1());
    assert!(!terminal.is_campaign_evaluation_policy_v1());
    assert!(!PolicySpecV1::Random.is_campaign_evaluation_policy_v1());
    assert!(!PolicySpecV1::Random.uses_full_pool_sampled_world_v1());
    for refused in [
        "sampled-public-heuristic/v2",
        "sampled-public-heuristic/v1:sims=2",
        "learner-checkpoint:",
        "learner-checkpoint:no/such/step_0000.json",
        "pimc:closedlist-full-pool-heuristic-v1:closedlist-full-pool-v1",
    ] {
        assert!(PolicySpecV1::parse(refused).is_err(), "{refused}");
    }
    // A raw session refuses the policies before its first step.
    let forest = card_id_by_name("Forest").unwrap();
    let decks = [&[forest; 40][..], &[forest; 40][..]];
    let raw_limits = GameLimitsV1 {
        session: SessionKindV1::PreparedV57,
        ..limits(4)
    };
    for policy in [&search, &heuristic, &learner] {
        let result = play_game_v1(
            decks,
            [policy, &PolicySpecV1::Random],
            [None; 2],
            552,
            PlayerId::P0,
            raw_limits,
        );
        assert!(matches!(result.outcome, GameOutcomeV1::Error(_)));
    }
}

#[test]
fn state_evaluator_reads_only_sampled_worlds_of_the_current_decision() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut left, belief) = scene(actor, "Forest");
        let (right, _) = scene(actor, "Island");
        let a = encode_current(&left).unwrap();
        // The live owner never evaluates its own state.
        assert!(left.sampled_world_state_eval_v1(actor).is_err());
        assert!(right.sampled_world_state_eval_v1(actor).is_err());
        let world = left
            .sampled_full_pool_closed_list_v1(&a, 552_301, &belief)
            .unwrap();
        let score = world.sampled_world_state_eval_v1(actor).unwrap();
        assert_eq!(world.sampled_world_state_eval_v1(actor.opponent()), Ok(-score));
        // The opponent's true hidden cards do not reach the score.
        let b = encode_current(&right).unwrap();
        let other = right
            .sampled_full_pool_closed_list_v1(&b, 552_301, &belief)
            .unwrap();
        assert_eq!(other.sampled_world_state_eval_v1(actor), Ok(score));
        // A foreign or stale decision draws no world.
        assert!(right
            .sampled_full_pool_closed_list_v1(&a, 552_301, &belief)
            .is_err());
        left.apply(&a, 0).unwrap();
        assert!(left
            .sampled_full_pool_closed_list_v1(&a, 552_301, &belief)
            .is_err());
    }
}

#[test]
fn sampled_public_heuristic_is_blind_to_hidden_cards_and_deterministic() {
    let heuristic = PolicySpecV1::SampledPublicHeuristicV1;
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (left, belief) = scene(actor, "Forest");
        let (right, _) = scene(actor, "Island");
        let a = encode_current(&left).unwrap();
        let b = encode_current(&right).unwrap();
        let before = left.fixture_state().clone();
        let first = sampled_public_heuristic_v1(&left, &a, &belief, 552_302).unwrap();
        assert_eq!(
            first,
            sampled_public_heuristic_v1(&left, &a, &belief, 552_302).unwrap()
        );
        assert_eq!(
            first,
            sampled_public_heuristic_v1(&right, &b, &belief, 552_302).unwrap()
        );
        let legal = legal_count(&a).unwrap() as usize;
        assert!(legal > 1);
        assert_eq!(first.1.visits, vec![1; legal]);
        assert_eq!((first.1.rejected, first.1.rollout_errors), (0, 0));
        let best = first
            .1
            .value_sum
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        assert_eq!(first.1.value_sum[first.0 as usize], best);
        assert_eq!(left.fixture_state(), &before);

        let mut policies = [&PolicySpecV1::Random; 2];
        policies[actor.index()] = &heuristic;
        let mut beliefs = [None; 2];
        beliefs[actor.index()] = Some(&belief);
        let mut result = GameResultV1::empty();
        let outcome = play_owner(left, policies, beliefs, 552_286, limits(24), &mut result).unwrap();
        assert!(!matches!(outcome, GameOutcomeV1::Error(_)));
        assert!(result.searched_decisions > 0);
        assert_eq!(
            (
                result.search_fallbacks,
                result.rejected_worlds,
                result.rollout_errors
            ),
            (0, 0, 0)
        );
    }
}

#[test]
fn heuristic_leaf_search_is_blind_to_hidden_cards_and_breaks_ties_by_value_then_seed() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (left, belief) = scene(actor, "Forest");
        let (right, _) = scene(actor, "Island");
        let a = encode_current(&left).unwrap();
        let b = encode_current(&right).unwrap();
        let sims = legal_count(&a).unwrap();
        let pass = a
            .fixture_backend_v75()
            .actions()
            .iter()
            .position(|action| matches!(action, ActionSemanticV1::Pass { .. }))
            .unwrap();
        let first =
            pimc_choose_full_pool_heuristic_v1(&left, &a, sims, 8, 20, &belief, 552_286).unwrap();
        let second =
            pimc_choose_full_pool_heuristic_v1(&right, &b, sims, 8, 20, &belief, 552_286).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.1.visits, vec![1; sims as usize]);
        assert_eq!((first.1.rejected, first.1.rollout_errors), (0, 0));
        // The terminal rollout after passing is the same one the terminal-only
        // search scores; the pick has the highest value among equal visits.
        assert_eq!(first.1.value_sum[pass], 1.0);
        assert_eq!(first.1.value_sum[first.0 as usize], 1.0);
        let terminal = pimc_choose_full_pool_v1(&left, &a, sims, 8, 20, &belief, 552_286).unwrap();
        assert_eq!(terminal.1.visits, first.1.visits);
        assert_eq!(terminal.1.value_sum[pass], 1.0);
    }
    let stats = |visits: Vec<u32>, value_sum: Vec<f64>| PimcStatsV1 {
        visits,
        value_sum,
        ..Default::default()
    };
    assert_eq!(
        heuristic_search_pick_v1(0, &stats(vec![2, 3, 3], vec![0.9, 0.1, 0.2]), 1),
        2
    );
    assert_eq!(heuristic_search_pick_v1(1, &stats(vec![0, 0], vec![0.0, 0.0]), 1), 1);
    let tied = stats(vec![2, 2, 2], vec![0.5, 0.5, 0.5]);
    let picks: std::collections::BTreeSet<u32> = (0..64)
        .map(|seed| heuristic_search_pick_v1(0, &tied, seed))
        .collect();
    assert_eq!(picks.len(), 3, "the seed decides among exact ties");
    assert_eq!(
        heuristic_search_pick_v1(0, &tied, 9),
        heuristic_search_pick_v1(0, &tied, 9)
    );
}

#[test]
fn learner_seat_draws_from_its_own_stream_and_other_policies_ignore_it() {
    let learner = step_zero_learner();
    let stream = MatchStreamV1 {
        seed: 657_101,
        pair: 2,
        leg: 1,
    };
    for actor in [PlayerId::P0, PlayerId::P1] {
        // Random play is the same with and without a pair stream.
        let (plain, _) = scene(actor, "Forest");
        let (paired, _) = scene(actor, "Forest");
        let mut without = GameResultV1::empty();
        let mut with = GameResultV1::empty();
        let random = [&PolicySpecV1::Random; 2];
        let outcome = play_owner(plain, random, [None; 2], 552_286, limits(24), &mut without);
        let again = play_owner_in_pair(
            paired,
            random,
            [None; 2],
            552_286,
            limits(24),
            Some(stream),
            &mut with,
        );
        assert_eq!(outcome, again);
        assert_eq!((without.turns, without.decisions), (with.turns, with.decisions));

        let mut policies = [&PolicySpecV1::Random; 2];
        policies[actor.index()] = &learner;
        let (session, _) = scene(actor, "Forest");
        let mut result = GameResultV1::empty();
        assert!(play_owner(session, policies, [None; 2], 552_286, limits(24), &mut result).is_err());

        let play = |hidden: &str| {
            let (session, _) = scene(actor, hidden);
            let mut result = GameResultV1::empty();
            let outcome = play_owner_in_pair(
                session,
                policies,
                [None; 2],
                552_286,
                limits(24),
                Some(stream),
                &mut result,
            )
            .unwrap();
            (outcome, result.turns, result.decisions)
        };
        let first = play("Forest");
        assert!(!matches!(first.0, GameOutcomeV1::Error(_)));
        assert!(first.2 > 0);
        assert_eq!(first, play("Forest"));
    }
    // The learner samples what its model and its own stream give.
    let (session, _) = scene(PlayerId::P0, "Forest");
    let decision = encode_current(&session).unwrap();
    let input = session.training_input_v1(&decision).unwrap();
    let PolicySpecV1::LearnerCheckpoint(checkpoint) = &learner else {
        unreachable!()
    };
    let logits = checkpoint.logits_v1(&input).unwrap();
    assert_eq!(logits.len(), legal_count(&decision).unwrap() as usize);
    let mut own = SplitMix64::seed(learner_policy_v1::learner_stream_seed_v1(stream, 0));
    let draw = own.next_u64();
    assert_eq!(
        checkpoint.sample_v1(&input, draw).unwrap(),
        learner_policy_v1::sample_learner_action_v1(&logits, draw).unwrap()
    );
}
