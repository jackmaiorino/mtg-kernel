use super::super::{tests::deal, validate};
use crate::{
    card_def::card_id_by_name,
    current_mixed_policy_v1::{KaitoDecision, KaitoSession},
    engine::{self, Action, Decision},
    ids::{ObjectId, PlayerId},
    leyline_axe_consumer_v1::ActionV1 as Axe,
    leyline_axe_public_v1::{self as public, PRINTING},
    limited_match_v1::LimitedBeliefV1,
    policy_observation_v6::tests::put,
    policy_surface_v5::LimitedAppendixV1,
    rl::{ActionSemanticV1, TargetRefV1},
    rl_session::{FastActorResponseV1, FastActorSessionV1},
    state::{GameState, Zone},
};

fn initial(first: PlayerId, first_axes: usize, other_axes: usize) -> GameState {
    let mut state = deal(first, first_axes, other_axes);
    FastActorSessionV1::enable_limited_native_modes_v1(&mut state, LimitedAppendixV1::FullPool);
    state.reveal_knowledge_v4 = Some(Default::default());
    state
}

fn raw(state: &GameState) -> FastActorSessionV1 {
    FastActorSessionV1::from_kaito_v75_fixture_state_scoped(
        state.clone(), LimitedAppendixV1::FullPool,
    )
}

fn owner(state: &GameState) -> KaitoSession {
    KaitoSession::from_session(raw(state)).unwrap()
}

fn decision(owner: &KaitoSession) -> KaitoDecision {
    let FastActorResponseV1::Decision(expected) = owner.current_response() else {
        panic!("sampled opening lost current decision: {:?}", owner.current_response());
    };
    owner.encode(expected).unwrap()
}

fn answer(state: &mut GameState, player: PlayerId, accept: bool) -> ObjectId {
    let Decision::ChooseEffectOption { player: chooser, source, option_count: 2 } =
        engine::advance_until_decision(state)
    else { panic!("genuine opening choice unavailable") };
    assert_eq!(chooser, player);
    engine::step(state, Action::ChooseEffectOption(u16::from(accept))).unwrap();
    source
}

fn ordinal(d: &KaitoDecision, accept: bool) -> u32 {
    d.fixture_backend_v75().candidates_v75().candidates.iter()
        .position(|row| row.leyline_axe_action == Some(Axe::ChooseOpeningEntry { accept }))
        .unwrap() as u32
}

fn corpus(axes: usize) -> LimitedBeliefV1 {
    let mut deck = vec![card_id_by_name("Forest").unwrap(); 40];
    deck[..axes].fill(PRINTING);
    LimitedBeliefV1::new([deck])
}

#[test]
fn active_opening_rebinds_future_opponent_choices_and_retires_past_declines_both_seats() {
    for first in [PlayerId::P0, PlayerId::P1] {
        let opponent = first.opponent();
        let live = owner(&initial(first, 1, 0));
        let paired = owner(&initial(first, 1, 1));
        let a = decision(&live);
        let b = decision(&paired);
        assert_eq!(a.packet(), b.packet());
        let before = live.fixture_state().clone();
        let paired_before = paired.fixture_state().clone();
        let mut sample = live.sampled_full_pool_closed_list_v1(&a, 552_482, &corpus(40)).unwrap();
        let mut alternate = paired.sampled_full_pool_closed_list_v1(&b, 552_482, &corpus(40)).unwrap();
        let d = decision(&sample);
        let other = decision(&alternate);
        assert_eq!(d.packet(), a.packet());
        assert_eq!(other.packet(), a.packet());
        let decline = ordinal(&d, false);
        let binding = d.fixture_backend_v75();
        let original = binding.fixture_original_index_v75(decline).unwrap();
        let expected = binding.expected();
        let mut native = raw(sample.fixture_state());
        let native_outcome = native.step(expected.episode_id, expected.step, original).unwrap();
        assert_eq!(sample.apply(&d, decline).unwrap(), native_outcome);
        assert_eq!(sample.fixture_state(), native.kaito_fixture_state());
        alternate.apply(&other, decline).unwrap();
        let future = decision(&sample);
        let future_pair = decision(&alternate);
        assert_eq!(future.packet(), future_pair.packet());
        assert_eq!(future.fixture_backend_v75().expected().acting_player, opponent.into());
        let actual = future.fixture_backend_v75().observation().leyline_axe
            .as_ref().unwrap().opening_choice.as_ref().unwrap();
        assert_eq!(actual.object, sample.fixture_state().players[opponent.index()].hand[0].0);
        sample.apply(&future, ordinal(&future, true)).unwrap();
        let entered = public::capture(sample.fixture_state(), first).unwrap();
        assert_eq!(entered.opening_entries.len(), 1);
        assert_eq!(entered.opening_entries[0].original.object, actual.object);
        assert_eq!(live.fixture_state(), &before);
        assert_eq!(paired.fixture_state(), &paired_before);

        let mut declined = initial(first, 1, 1);
        let hidden = answer(&mut declined, first, false);
        let no_decline = initial(first, 0, 1);
        let original = owner(&declined);
        let paired = owner(&no_decline);
        let a = decision(&original);
        let b = decision(&paired);
        assert_eq!(a.packet(), b.packet());
        assert_eq!(a.fixture_backend_v75().expected().acting_player, opponent.into());
        let mut sample = original.sampled_full_pool_closed_list_v1(&a, 552_483, &corpus(0)).unwrap();
        let alternate = paired.sampled_full_pool_closed_list_v1(&b, 552_483, &corpus(0)).unwrap();
        assert_ne!(sample.fixture_state().objects.get(hidden).card_def, PRINTING);
        let fresh = decision(&sample);
        assert_eq!(fresh.packet(), decision(&alternate).packet());
        assert_eq!(fresh.packet(), a.packet());
        sample.apply(&fresh, ordinal(&fresh, true)).unwrap();
        let outcome = public::capture(sample.fixture_state(), first).unwrap();
        assert_eq!(outcome.opening_entries.len(), 1);
        assert_eq!(outcome.opening_entries[0].original.owner, opponent.into());
    }
}

fn closed_opening(first: PlayerId, hidden_decline: bool) -> (GameState, ObjectId, ObjectId, ObjectId) {
    let viewer = first.opponent();
    let mut state = initial(first, 1 + usize::from(hidden_decline), 0);
    let accepted = answer(&mut state, first, true);
    if hidden_decline {
        answer(&mut state, first, false);
    }
    assert!(matches!(engine::advance_until_decision(&mut state), Decision::CastSpellOrPass { .. }));
    assert!(state.leyline_opening_v1.is_none());
    // Identical public mana and private viewer spell support after the real
    // native opening. All subsequent payment and targeting use owned actions.
    let mana = put(&mut state, viewer, "Mountain", Zone::Battlefield);
    let spell = put(&mut state, viewer, "Burst Lightning", Zone::Hand);
    for _ in 0..4 {
        match engine::advance_until_decision(&mut state) {
            Decision::CastSpellOrPass { player, .. } if player == viewer =>
                return (state, accepted, mana, spell),
            Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
            other => panic!("expected actual priority after the opening: {other:?}"),
        }
    }
    panic!("fixture lost opponent priority");
}

#[test]
fn closed_opening_resamples_private_declines_preserves_public_entry_and_runs_paid_cast() {
    for first in [PlayerId::P0, PlayerId::P1] {
        let viewer = first.opponent();
        let (state, accepted, mana, spell) = closed_opening(first, true);
        let (paired, other_accepted, _, _) = closed_opening(first, false);
        assert_eq!(accepted, other_accepted);
        let hidden = state.players[first.index()].hand[0];
        assert_eq!(state.objects.get(hidden).card_def, PRINTING);
        assert_ne!(state.state_hash(), paired.state_hash());
        let original = owner(&state);
        let paired = owner(&paired);
        let d = decision(&original);
        let other = decision(&paired);
        assert_eq!(d.packet(), other.packet());
        let mut sample = original.sampled_full_pool_closed_list_v1(&d, 552_484, &corpus(1)).unwrap();
        let alternate = paired.sampled_full_pool_closed_list_v1(&other, 552_484, &corpus(1)).unwrap();
        let fresh = decision(&sample);
        assert_eq!(fresh.packet(), decision(&alternate).packet());
        assert_eq!(fresh.packet(), d.packet());
        assert_ne!(sample.fixture_state().objects.get(hidden).card_def, PRINTING);
        assert_eq!(sample.fixture_state().objects.get(accepted).card_def, PRINTING);
        assert_eq!(sample.fixture_state().objects.get(accepted).zone, Zone::Battlefield);
        assert_eq!(sample.fixture_state().engine.event_history, original.fixture_state().engine.event_history);
        let serialized = serde_json::to_vec(sample.fixture_state()).unwrap();
        let untrusted: GameState = serde_json::from_slice(&serialized).unwrap();
        assert!(untrusted.leyline_sampled_history_v1.is_none());
        assert!(validate(&untrusted).is_err());

        let snapshot = sample.snapshot();
        let mut paid = false;
        for _ in 0..8 {
            if sample.fixture_state().objects.get(spell).zone == Zone::Stack
                && sample.fixture_state().engine.pending_cast.is_none()
            {
                assert!(sample.fixture_state().objects.get(mana).tapped);
                paid = true;
                break;
            }
            let current = decision(&sample);
            let rows = &current.fixture_backend_v75().candidates_v75().candidates;
            let pick = rows.iter().position(|row| match &row.action {
                ActionSemanticV1::CastSpell { source, .. } => source.arena_id == spell.0,
                ActionSemanticV1::ChooseKicker { pay, .. } => !pay,
                ActionSemanticV1::ChooseTarget { target: TargetRefV1::Player { player }, .. } =>
                    *player == first.into(),
                _ => false,
            }).expect("expected actual paid Burst Lightning continuation");
            sample.apply(&current, pick as u32).unwrap();
        }
        assert!(paid);
        sample.restore(&snapshot).unwrap();
        assert_eq!(decision(&sample).packet(), d.packet());
        assert!(sample.apply(&fresh, 0).is_err());
        assert_eq!(public::capture(original.fixture_state(), viewer).unwrap().opening_entries.len(), 1);
        assert_eq!(original.fixture_state().objects.get(hidden).card_def, PRINTING);
    }
}
