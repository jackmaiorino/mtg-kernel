use crate::{
    card_def::{card_id_by_name, CARD_DEFS},
    curator_consumer_v1::ActionV1,
    curator_public_v1::{self as public, CardsStageV1, ChoiceV1, PileV1, StageV1},
    curator_receipts_v1::JournalV1,
    current_mixed_policy_v1::{KaitoDecision, KaitoSession},
    effect::curator_v1::{self, tests as native},
    engine::{self, Action, Decision},
    event::CommittedEvent,
    ids::{ObjectId, PlayerId},
    limited_match_v1::LimitedBeliefV1,
    policy_observation_v6::tests::put,
    policy_surface_v5::LimitedAppendixV1,
    rl_session::{FastActorResponseV1, FastActorSessionV1},
    state::{GameState, ObjectStateV4, Zone},
};

fn id(name: &str) -> u16 {
    card_id_by_name(name).unwrap()
}

fn raw(state: &GameState, appendix: LimitedAppendixV1) -> FastActorSessionV1 {
    FastActorSessionV1::from_kaito_v75_fixture_state_scoped(state.clone(), appendix)
}

fn owner(state: &GameState) -> KaitoSession {
    KaitoSession::from_session(raw(state, LimitedAppendixV1::FullPool)).unwrap()
}

fn decision(owner: &KaitoSession) -> KaitoDecision {
    let FastActorResponseV1::Decision(expected) = owner.current_response() else {
        panic!("Curator sampler decision absent: {:?}", owner.current_response());
    };
    owner.encode(expected).unwrap()
}

fn corpus() -> LimitedBeliefV1 {
    // Public corpus includes the fixture's known lands, source and pile names.
    // All remaining unknown opponent slots draw Swamps, independent of live names.
    let mut deck = vec![id("Island"); 10];
    deck.extend(std::iter::repeat_n(id("Mountain"), 13));
    deck.extend([id("Plains"), id("Curator of Destinies")]);
    deck.extend(std::iter::repeat_n(id("Swamp"), 35));
    LimitedBeliefV1::new([deck])
}

fn look(controller: PlayerId, paired: bool) -> (GameState, Vec<ObjectId>) {
    let left = [
        "Forest", "Island", "Mountain", "Mountain", "Plains", "Swamp", "Island", "Forest",
    ];
    let right = [
        "Mountain", "Swamp", "Plains", "Mountain", "Forest", "Plains", "Forest", "Island",
    ];
    let mut state = native::ready(controller, if paired { &right } else { &left });
    FastActorSessionV1::enable_limited_native_modes_v1(&mut state, LimitedAppendixV1::FullPool);
    state.reveal_knowledge_v4 = Some(Default::default());
    if paired {
        // Exchange two opaque aliases in the private down pile before the look.
        state.players[controller.index()].library.swap(1, 4);
    }
    state.reveal_library_position(controller.opponent(), controller, 3);
    put(&mut state, controller, "Forest", Zone::Hand);
    put(&mut state, controller, "Island", Zone::Hand);
    put(&mut state, controller.opponent(), "Burst Lightning", Zone::Hand);
    native::paid_to_trigger(&mut state, controller);
    let prefix = native::partition_prompt(&mut state, controller);
    (state, prefix)
}

fn partition(state: &mut GameState, controller: PlayerId, prefix: &[ObjectId], paired: bool) {
    let down = if paired { [prefix[1], prefix[4]] } else { [prefix[0], prefix[1]] };
    native::partition(state, &down);
    native::pile_prompt(state, controller, 2, 3);
}

fn layout(state: &GameState, controller: PlayerId) -> Vec<(bool, u16)> {
    let pending = state.engine.pending_effect.as_ref().unwrap();
    let choice = curator_v1::prepare_pile_answer(state, pending, 2, 3).unwrap();
    assert_eq!(choice.player, controller);
    choice.prefix.iter().map(|binding| {
        (choice.down.contains(binding), state.objects.get(binding.object).card_def)
    }).collect()
}

fn pile_ordinal(d: &KaitoDecision, pile: PileV1) -> u32 {
    d.fixture_backend_v75().candidates_v75().candidates.iter()
        .position(|row| row.curator_action == Some(ActionV1::ChoosePile { pile })).unwrap() as u32
}

fn order_names(d: &KaitoDecision) -> Vec<u16> {
    let facts = d.fixture_backend_v75().observation().curator.as_ref().unwrap();
    let Some(ChoiceV1::Cards { stage: CardsStageV1::OrderGraveyard, legal, .. }) = &facts.chooser else {
        panic!("sampled controller did not get its actual order menu");
    };
    legal.iter().map(|row| match row.identity {
        crate::optional_trigger_public_v1::IdentityV1::Known { card_id } => card_id,
        _ => panic!("sampled controller lost its own looked card"),
    }).collect()
}

#[test]
fn paid_curator_sampled_layout_is_invariant_to_private_piles_and_aliases_both_seats() {
    for controller in [PlayerId::P0, PlayerId::P1] {
        let viewer = controller.opponent();
        let belief = corpus();
        let (mut live, prefix) = look(controller, false);
        let (mut paired, paired_prefix) = look(controller, true);
        let looked = owner(&live);
        let looked_d = decision(&looked);
        let looked_sample = looked.sampled_full_pool_closed_list_v1(&looked_d, 552, &belief).unwrap();
        assert_eq!(looked_d.packet(), decision(&looked_sample).packet());
        assert_eq!(
            &looked_sample.fixture_state().players[controller.index()].library[..5], prefix.as_slice(),
        );
        partition(&mut live, controller, &prefix, false);
        partition(&mut paired, controller, &paired_prefix, true);
        assert_ne!(live.state_hash(), paired.state_hash());
        assert_ne!(layout(&live, controller), layout(&paired, controller));

        let a = owner(&live);
        let b = owner(&paired);
        let da = decision(&a);
        let db = decision(&b);
        let before_a = a.fixture_state().clone();
        let before_b = b.fixture_state().clone();
        assert_eq!(da.packet(), db.packet());
        assert!(a.sampled_clone(&da, 1).is_err());
        let old = raw(&live, LimitedAppendixV1::Curator);
        assert!(old.refuse_curator_sampling_v1().is_err());
        let mut varied = false;
        let mut previous = None;
        for seed in [552_001, 552_002, 552_003, 552_004] {
            let mut sa = a.sampled_full_pool_closed_list_v1(&da, seed, &belief).unwrap();
            let mut sb = b.sampled_full_pool_closed_list_v1(&db, seed, &belief).unwrap();
            let before = layout(sa.fixture_state(), controller);
            assert_eq!(before, layout(sb.fixture_state(), controller));
            assert_eq!(before[3].1, id("Mountain"));
            assert!(before.iter().enumerate().filter(|(at, (down, _))| *at != 3 && *down)
                .any(|(_, (_, card))| *card == id("Swamp")));
            assert_ne!(before, layout(&live, controller));
            varied |= previous.as_ref().is_some_and(|prior| prior != &before);
            previous = Some(before);
            assert_eq!(
                sa.fixture_state().known_library_cards(viewer, controller),
                live.known_library_cards(viewer, controller),
            );
            let current = decision(&sa);
            let other = decision(&sb);
            assert_eq!(current.packet(), da.packet());
            assert_eq!(current.packet(), other.packet());
            let selected = pile_ordinal(&current, PileV1::FaceUp);
            let native_index = current.fixture_backend_v75().fixture_original_index_v75(selected).unwrap();
            assert_eq!(other.fixture_backend_v75().fixture_original_index_v75(selected), Some(native_index));
            let expected = current.fixture_backend_v75().expected();
            let mut engine = raw(sa.fixture_state(), LimitedAppendixV1::FullPool);
            let result = engine.step(expected.episode_id, expected.step, native_index).unwrap();
            let saved = sa.snapshot();
            assert_eq!(sa.apply(&current, selected).unwrap(), result);
            assert_eq!(sa.fixture_state(), engine.kaito_fixture_state());
            sb.apply(&other, selected).unwrap();
            let order_a = decision(&sa);
            let order_b = decision(&sb);
            assert_eq!(order_names(&order_a), order_names(&order_b));
            sa.apply(&order_a, 0).unwrap();
            sb.apply(&order_b, 0).unwrap();
            for sampled in [&sa, &sb] {
                let facts = public::capture(sampled.fixture_state(), viewer).unwrap();
                let StageV1::Resolved(outcome) = &facts.entries[0].stage else { panic!() };
                assert_eq!(outcome.actual_hand_arrivals, 3);
                assert_eq!(outcome.public_moves.len(), 2);
                assert_eq!(outcome.revealed_hand_arrivals.len(), 2);
            }
            sa.restore(&saved).unwrap();
            assert_eq!(decision(&sa).packet(), da.packet());
            assert!(sa.apply(&current, selected).is_err());
        }
        assert!(varied);
        assert_eq!(a.fixture_state(), &before_a);
        assert_eq!(b.fixture_state(), &before_b);
    }
}

#[test]
fn archived_curator_name_minima_resample_hand_slots_and_keep_native_receipts_both_seats() {
    for controller in [PlayerId::P0, PlayerId::P1] {
        let viewer = controller.opponent();
        let (mut state, prefix) = look(controller, false);
        partition(&mut state, controller, &prefix, false);
        engine::step(&mut state, Action::ChooseEffectBoolean(false)).unwrap();
        native::finish(&mut state);
        for _ in 0..4 {
            match engine::advance_until_decision(&mut state) {
                Decision::CastSpellOrPass { player, .. } if player == viewer => break,
                Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
                other => panic!("expected genuine post-Curator priority: {other:?}"),
            }
        }
        let restored: GameState = serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        public::validate_native(&restored).unwrap();
        let arrivals = state.engine.event_history.iter().filter_map(|event| {
            match event {
                CommittedEvent::CuratorReceiptV1 { journal } => match journal.as_ref() {
                    JournalV1::Moved { move_row, .. } if move_row.destination == Zone::Hand =>
                        move_row.public_printing.map(|name| (move_row.original.object, name)),
                    _ => None,
                },
                _ => None,
            }
        }).collect::<Vec<_>>();
        assert_eq!(arrivals.len(), 3);
        let live = owner(&state);
        let root = decision(&live);
        assert_eq!(root.fixture_backend_v75().expected().acting_player, viewer.into());
        let before = public::capture(live.fixture_state(), viewer).unwrap();
        let before_state = live.fixture_state().clone();
        let mut unpinned = false;
        for seed in [552_015, 552_016, 552_017, 552_018] {
            let mut sampled = live.sampled_full_pool_closed_list_v1(&root, seed, &corpus()).unwrap();
            let fresh = decision(&sampled);
            assert_eq!(fresh.packet(), root.packet());
            assert_eq!(public::capture(sampled.fixture_state(), viewer).unwrap(), before);
            assert_eq!(
                sampled.fixture_state().engine.event_history,
                before_state.engine.event_history,
            );
            let minima = sampled.fixture_state().reveal_knowledge_v4.as_ref().unwrap()[viewer.index()]
                .hand(controller).collect::<std::collections::BTreeMap<_, _>>();
            assert_eq!(minima.get(&id("Mountain")), Some(&2));
            assert_eq!(minima.get(&id("Plains")), Some(&1));
            unpinned |= arrivals.iter().any(|&(object, name)| {
                sampled.fixture_state().objects.get(object).card_def != name
            });
            let snapshot = sampled.snapshot();
            sampled.restore(&snapshot).unwrap();
            assert_eq!(decision(&sampled).packet(), root.packet());
        }
        assert!(unpinned);
        assert_eq!(live.fixture_state(), &before_state);

        let mut tampered = restored;
        let private_slot = tampered.objects.get_mut(arrivals[0].0);
        private_slot.card_def = id("Swamp");
        private_slot.name = CARD_DEFS[usize::from(private_slot.card_def)].name.into();
        private_slot.v4 = ObjectStateV4::from_card_def(private_slot.card_def);
        let before = tampered.clone();
        assert!(public::validate_native(&tampered).is_err());
        assert_eq!(tampered, before);
    }
}
