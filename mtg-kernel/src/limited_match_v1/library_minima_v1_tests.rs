use super::*;
use crate::{
    card_def::{card_id_by_name, CARD_DEFS},
    current_mixed_policy_v1::{KaitoDecision, KaitoSession},
    engine::{self, progenitus_v1_tests as native, Action, Decision},
    event::{self, ProposedEvent},
    limited_match_v1::LimitedBeliefV1,
    policy_observation_v6::tests::put,
    policy_surface_v5::LimitedAppendixV1,
    rl::ActionSemanticV1,
    rl_session::{FastActorResponseV1, FastActorSessionV1},
    state::ObjectStateV4,
};

fn card(name: &str) -> u16 {
    card_id_by_name(name).unwrap()
}

fn numeric(state: &GameState) -> KaitoSession {
    KaitoSession::from_session(raw(state)).unwrap()
}

fn raw(state: &GameState) -> FastActorSessionV1 {
    FastActorSessionV1::from_kaito_v75_fixture_state_scoped(
        state.clone(), LimitedAppendixV1::FullPool,
    )
}

fn decision(owner: &KaitoSession) -> KaitoDecision {
    let FastActorResponseV1::Decision(expected) = owner.current_response() else {
        panic!("library minima fixture stopped: {:?}", owner.current_response());
    };
    owner.encode(expected).unwrap()
}

fn genuine_replacement(owner: PlayerId) -> (GameState, ObjectId) {
    let mut state = native::ready(owner, "Forest");
    FastActorSessionV1::enable_limited_native_modes_v1(&mut state, LimitedAppendixV1::FullPool);
    state.reveal_knowledge_v4 = Some(Default::default());
    let source = put(&mut state, owner, "Progenitus", Zone::Hand);
    native::cast(
        &mut state, owner, "Thrill of Possibility", (false, 0, false), &[], Some(source), 2,
    );
    assert_eq!(state.objects.get(source).zone, Zone::Library);
    assert!(state.known_library_cards(owner.opponent(), owner).is_empty());
    assert_eq!(minimum_names(&state, owner), [PRINTING]);
    (state, source)
}

fn give_priority(state: &mut GameState, viewer: PlayerId) {
    for _ in 0..4 {
        match engine::advance_until_decision(state) {
            Decision::CastSpellOrPass { player, .. } if player == viewer => return,
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => panic!("no native priority while Thrill was pending: {other:?}"),
        }
    }
    panic!("opponent never got actual priority");
}

fn corpus() -> LimitedBeliefV1 {
    let mut deck = Vec::new();
    for name in ["Forest", "Mountain", "Island", "Plains", "Swamp"] {
        deck.extend(std::iter::repeat_n(card(name), 8));
    }
    // The only Progenitus admitted by this belief must come from the public
    // library receipt. The known Forest in hand is a separate constraint.
    deck.extend([card("Forest"), card("Thrill of Possibility")]);
    deck.extend(std::iter::repeat_n(card("Swamp"), 80));
    LimitedBeliefV1::new([deck])
}

fn definitions(state: &GameState, ids: &[ObjectId]) -> Vec<u16> {
    ids.iter().map(|&id| state.objects.get(id).card_def).collect()
}

#[test]
fn paid_public_shuffle_conditions_only_the_sampled_library_both_seats() {
    for physical_owner in [PlayerId::P0, PlayerId::P1] {
        let viewer = physical_owner.opponent();
        let (mut state, source) = genuine_replacement(physical_owner);
        let hand = put(&mut state, physical_owner, "Forest", Zone::Hand);
        state.reveal_distinct_hand_card(viewer, physical_owner, hand).unwrap();
        give_priority(&mut state, viewer);
        let mut paired = state.clone();
        let library = &mut paired.players[physical_owner.index()].library;
        let original_position = library.iter().position(|&id| id == source).unwrap();
        let another = (original_position + 1) % library.len();
        library.swap(original_position, another);
        let alternate = library[(another + 1) % library.len()];
        let changed = paired.objects.get_mut(alternate);
        changed.card_def = card("Island");
        changed.name = CARD_DEFS[usize::from(changed.card_def)].name.into();
        changed.v4 = ObjectStateV4::from_card_def(changed.card_def);
        assert_ne!(state.state_hash(), paired.state_hash());

        let left = numeric(&state);
        let right = numeric(&paired);
        let da = decision(&left);
        let db = decision(&right);
        assert_eq!(da.packet(), db.packet());
        let live = left.fixture_state().clone();
        let right_live = right.fixture_state().clone();
        let belief = corpus();
        let mut sampled_original_alias_changed = false;
        for seed in [552_480, 552_481, 552_482] {
            let mut sample = left.sampled_full_pool_closed_list_v1(&da, seed, &belief).unwrap();
            let mut other = right.sampled_full_pool_closed_list_v1(&db, seed, &belief).unwrap();
            let current = decision(&sample);
            let other_current = decision(&other);
            assert_eq!(current.packet(), da.packet());
            assert_eq!(current.packet(), other_current.packet());
            let player = &sample.fixture_state().players[physical_owner.index()];
            let other_player = &other.fixture_state().players[physical_owner.index()];
            let library = definitions(sample.fixture_state(), &player.library);
            let hand = definitions(sample.fixture_state(), &player.hand);
            assert_eq!(library, definitions(other.fixture_state(), &other_player.library));
            assert_eq!(hand, definitions(other.fixture_state(), &other_player.hand));
            assert_eq!(library.iter().filter(|&&name| name == PRINTING).count(), 1);
            assert!(!hand.contains(&PRINTING));
            assert!(hand.contains(&card("Forest")));
            sampled_original_alias_changed |= sample.fixture_state().objects.get(source).card_def != PRINTING;

            let selected = current.fixture_backend_v75().candidates_v75().candidates.iter()
                .position(|row| matches!(row.action, ActionSemanticV1::Pass { .. })).unwrap() as u32;
            let expected = current.fixture_backend_v75().expected();
            let native_index = current.fixture_backend_v75().fixture_original_index_v75(selected).unwrap();
            let mut expected_owner = raw(sample.fixture_state());
            let native = expected_owner.step(expected.episode_id, expected.step, native_index).unwrap();
            let snapshot = sample.snapshot();
            assert_eq!(sample.apply(&current, selected).unwrap(), native);
            assert_eq!(sample.fixture_state(), expected_owner.kaito_fixture_state());
            other.apply(&other_current, selected).unwrap();
            assert!(minimum_names(sample.fixture_state(), physical_owner).is_empty());
            assert!(minimum_names(other.fixture_state(), physical_owner).is_empty());
            sample.restore(&snapshot).unwrap();
            assert_eq!(decision(&sample).packet(), da.packet());
            assert!(sample.apply(&current, selected).is_err());
        }
        assert!(sampled_original_alias_changed);
        assert_eq!(left.fixture_state(), &live);
        assert_eq!(right.fixture_state(), &right_live);
    }
}

#[test]
fn library_minima_preserve_ordinary_mill_and_reduce_anonymous_draws_both_seats() {
    for owner in [PlayerId::P0, PlayerId::P1] {
        let (mut state, original) = genuine_replacement(owner);
        let second = put(&mut state, owner, "Progenitus", Zone::Hand);
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(second, Zone::Graveyard));
        assert_eq!(state.objects.get(second).zone, Zone::Library);
        assert_eq!(minimum_names(&state, owner), [PRINTING; 2]);

        let mill = put(&mut state, owner, "Island", Zone::Library);
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(mill, Zone::Graveyard));
        assert_eq!(state.objects.get(mill).zone, Zone::Graveyard);
        assert_eq!(minimum_names(&state, owner), [PRINTING; 2]);

        event::propose_and_commit(&mut state, ProposedEvent::zone_change(original, Zone::Graveyard));
        assert_eq!(state.objects.get(original).zone, Zone::Library);
        assert_eq!(minimum_names(&state, owner), [PRINTING; 2]);
        let restored: GameState = serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        assert_eq!(minimum_names(&restored, owner), [PRINTING; 2]);

        let mut paired = state.clone();
        let top = |state: &mut GameState, object| {
            let library = &mut state.players[owner.index()].library;
            let position = library.iter().position(|&id| id == object).unwrap();
            library.swap(0, position);
        };
        top(&mut state, original);
        let ordinary = paired.players[owner.index()].library.iter().copied()
            .find(|&id| paired.objects.get(id).card_def != PRINTING).unwrap();
        top(&mut paired, ordinary);
        for branch in [&mut state, &mut paired] {
            event::propose_and_commit(branch, ProposedEvent::draw(owner));
            assert_eq!(minimum_names(branch, owner), [PRINTING]);
            assert!(minimum_names(branch, owner.opponent()).is_empty());
        }
        assert_eq!(state.objects.get(original).zone, Zone::Hand);
        assert_eq!(paired.objects.get(original).zone, Zone::Library);
    }
}
