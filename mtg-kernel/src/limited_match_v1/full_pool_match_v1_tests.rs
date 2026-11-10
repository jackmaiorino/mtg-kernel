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
