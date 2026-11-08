use super::*;
use crate::{
    card_def::{card_id_by_name, KERNEL_CARDDB_HASH},
    current_mixed_policy_v1::KaitoDecision,
    current_tree_learner_v1::ModelV1,
    engine,
    human_bo3_v1::{project_limited_kaito_v16, HumanActionRequestV1, HumanKaitoProjectorV75},
    ids::{ObjectId, PlayerId},
    limited_kaito_session_v16::{LimitedKaitoBodyV16 as Body, LimitedKaitoStateV16 as State},
    policy_observation_v6::tests::put,
    policy_surface_v5::LimitedAppendixV1,
    rl::{ActionSemanticV1 as A, TargetRefV1},
    rl_session::{FastActorResponseV1, KaitoSessionV75},
    state::{GameState, Step},
};

fn card(name: &str) -> u16 {
    card_id_by_name(name).unwrap()
}

fn config(actor: PlayerId) -> ResetV1 {
    let mut mainboards = [vec![card("Island"); 40], vec![card("Island"); 40]];
    mainboards[actor.index()] = [
        vec![PRINTING; 6],
        vec![card("Burst Lightning"); 4],
        vec![card("Island"); 30],
    ].concat();
    ResetV1 {
        expected_card_db_hash: KERNEL_CARDDB_HASH,
        episode_id: 550,
        env_seed: 550_478,
        starting_player: actor.into(),
        deck_ids: ["omniscience-left".into(), "omniscience-right".into()],
        mainboards,
        max_physical_decisions: 2_000,
        max_policy_steps: 2_000,
    }
}

fn decision(session: &KaitoSession) -> KaitoDecision {
    let FastActorResponseV1::Decision(expected) = session.current_response() else {
        panic!("Omniscience decision absent: {:?}", session.current_response());
    };
    session.encode(expected).unwrap()
}

fn body(wire: &mut JsonlServerV1, serial: u64, command: CommandV1) -> Body {
    let reply = wire.handle_request(RequestV1 {
        profile: PROFILE_V1.into(),
        request_id: serial.to_string(),
        command,
    });
    let ReplyBodyV1::State { payload } = reply.body else {
        panic!("Omniscience wire request rejected: {reply:?}");
    };
    payload.body
}

fn tapped(state: &GameState, player: PlayerId) -> usize {
    state.players[player.index()].battlefield.iter()
        .filter(|&&id| state.objects.get(id).tapped)
        .count()
}

#[test]
fn declared_forty_card_v3_reset_pays_ten_then_casts_free_from_hand_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let config = config(actor);
        assert!(crate::limited_v68_v75_consumer_v1::reset_numeric_v1(&config).is_err());
        assert!(crate::zimone_consumer_v1::reset_numeric_v1(&config).is_err());
        let mut invalid = config.clone();
        invalid.mainboards[actor.index()][0] = 465;
        assert!(reset_numeric_v1(&invalid).is_err());

        let mut wire = JsonlServerV1::new();
        let Body::State { state, .. } = body(
            &mut wire, 1, CommandV1::Reset { config: config.clone() },
        ) else { panic!() };
        let State::Decision { decision: wire_opening, .. } = *state else { panic!() };
        assert_eq!(wire_opening.schema, "mtg-kernel-limited-omniscience-decision/v1");

        let mut numeric = reset_numeric_reveal_facts_v3(&config).unwrap();
        assert!(numeric.fixture_state().reveal_knowledge_v4.is_some());
        let opening = decision(&numeric);
        let expected = opening.fixture_backend_v75().expected();
        let human = reset_human_v1(&config, expected.acting_player).unwrap();
        let human_opening = HumanKaitoProjectorV75::new(expected.acting_player)
            .project_current(&human, expected).unwrap();
        assert_eq!(human_opening.schema, "mtg-kernel-human-omniscience-decision/v1");
        assert_eq!(human_opening.actions, wire_opening.actions);

        let mut source = None;
        let mut burst = None;
        let mut complete = false;
        for _ in 0..550 {
            let d = decision(&numeric);
            let backend = d.fixture_backend_v75();
            let rows = &backend.candidates_v75().candidates;
            let facts = backend.observation().omniscience.as_ref().unwrap();
            let state = numeric.fixture_state();
            if let Some(published) = facts.casts.iter().find(|row| {
                row.stage == StageV1::Published
                    && Some(ObjectId(row.spell.object)) == burst
            }) {
                assert_eq!(published.route, Some(RouteV1::ReplaceBaseMana));
                assert_eq!(published.x, Some(0));
                assert_eq!(published.kicked, Some(false));
                assert!(published.original_permission_active);
                let input = numeric.training_input_v1(&d).unwrap();
                assert!(ModelV1::deterministic_fixture().forward(&input).unwrap().iter().all(|n| n.is_finite()));
                complete = true;
                break;
            }
            let choose = rows.iter().position(|row| {
                matches!(&row.action, A::CastSpell { source: spell, .. } if
                    (source.is_none() && spell.card_db_id == PRINTING)
                    || (burst.is_none() && spell.card_db_id == card("Burst Lightning")
                        && row.omniscience_action == Some(ActionV1::AnnounceHandSpell {
                            ordinary_payment_available: false,
                        })))
                    || matches!(&row.action, A::ChooseTarget {
                        target: TargetRefV1::Player { player }, ..
                    } if *player == actor.opponent().into())
                    || matches!(row.action, A::ChooseKicker { pay: false, .. })
            }).or_else(|| rows.iter().position(|row| matches!(row.action, A::PlayLand { .. })))
                .or_else(|| rows.iter().position(|row| matches!(row.action,
                    A::ChooseLondonMulligan { mulligan: false, .. }
                    | A::ChooseAttackerInclusion { include: false, .. }
                    | A::ChooseBlockerInclusion { include: false, .. }
                    | A::Pass { .. }
                    | A::Discard { .. }
                ))).expect("missing native Omniscience reset action");
            let action = &rows[choose].action;
            let before = tapped(state, actor);
            numeric.apply(&d, choose as u32).unwrap();
            match action {
                A::CastSpell { source: spell, .. } if spell.card_db_id == PRINTING => {
                    let id = ObjectId(spell.arena_id);
                    assert_eq!(numeric.fixture_state().objects.get(id).zone, Zone::Stack);
                    assert_eq!(tapped(numeric.fixture_state(), actor) - before, 10);
                    source = Some(id);
                }
                A::CastSpell { source: spell, .. } => {
                    burst = Some(ObjectId(spell.arena_id));
                    assert_eq!(tapped(numeric.fixture_state(), actor), before);
                }
                A::ChooseTarget { .. } => assert_eq!(tapped(numeric.fixture_state(), actor), before),
                _ => {}
            }
        }
        assert!(complete, "declared reset never published the real free cast");
    }
}

fn fixture(state: &GameState) -> FastActorSessionV1 {
    FastActorSessionV1::from_kaito_v75_fixture_state_scoped(
        state.clone(), LimitedAppendixV1::Omniscience,
    )
}

#[test]
fn actual_route_indices_hidden_pair_restore_and_additional_discard_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let deck = [card("Island"); 40];
        let mut state = GameState::new_from_libraries_with_starting_player_v1(
            &deck, &deck, crate::rl::card_name, 550_478, actor,
        );
        state.step = Step::Main1;
        for _ in 0..13 {
            put(&mut state, actor, "Island", Zone::Battlefield);
        }
        for _ in 0..8 {
            put(&mut state, actor, "Mountain", Zone::Battlefield);
        }
        let omni = engine::omniscience_cast_v1::tests::paid(&mut state, actor);
        let spell = put(&mut state, actor, "Thrill of Possibility", Zone::Hand);
        let fodder = put(&mut state, actor, "Forest", Zone::Hand);
        let hidden = put(&mut state, actor.opponent(), "Forest", Zone::Hand);
        let grave = put(&mut state, actor, "Thrill of Possibility", Zone::Graveyard);
        let opening = KaitoSession::from_session(fixture(&state)).unwrap();
        let d = decision(&opening);
        let facts = d.fixture_backend_v75().observation().omniscience.as_ref().unwrap();
        assert!(facts.hand_offers.iter().any(|offer| offer.card.object == spell.0 && offer.ordinary_payment_available));
        assert!(!facts.hand_offers.iter().any(|offer| [grave.0, hidden.0].contains(&offer.card.object)));
        let before = tapped(&state, actor);
        engine::step(&mut state, engine::Action::CastSpell(spell)).unwrap();
        assert!(matches!(engine::advance_until_decision(&mut state), engine::Decision::ChooseEffectOption {
            option_count: 2, ..
        }));

        let mut altered = state.clone();
        altered.objects.get_mut(hidden).card_def = PRINTING;
        let mut other = KaitoSession::from_session(fixture(&altered)).unwrap();
        let mut numeric = KaitoSession::from_session(fixture(&state)).unwrap();
        let mut raw = fixture(&state);
        let mut human = HumanKaitoSessionV75::from_session(fixture(&state), actor.into()).unwrap();
        let mut projector = HumanKaitoProjectorV75::new(actor.into());
        let mut wire = JsonlServerV1::new();
        wire.0.install_fixture_core(KaitoSessionV75::from_session(fixture(&state)).unwrap());
        let mut other_wire = JsonlServerV1::new();
        other_wire.0.install_fixture_core(KaitoSessionV75::from_session(fixture(&altered)).unwrap());

        let d = decision(&numeric);
        assert_ne!(state.state_hash(), altered.state_hash());
        assert_eq!(d.packet(), decision(&other).packet());
        assert!(other.training_input_v1(&d).is_err());
        let save = numeric.snapshot();
        assert!(other.restore(&save).is_err());
        let backend = d.fixture_backend_v75();
        let rows = &backend.candidates_v75().candidates;
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].omniscience_action, Some(ActionV1::ChooseRoute { route: RouteV1::OrdinaryPayment }));
        assert_eq!(rows[1].omniscience_action, Some(ActionV1::ChooseRoute { route: RouteV1::ReplaceBaseMana }));
        let expected = backend.expected();
        let prompt = projector.project_current(&human, expected).unwrap();
        let human_save = human.snapshot_v75();
        let current = body(&mut wire, 1, CommandV1::Current);
        assert_eq!(current, body(&mut other_wire, 1, CommandV1::Current));
        let Body::State { state: current, .. } = current else { panic!() };
        let State::Decision { episode_id, authority_generation, decision: visible } = *current else { panic!() };
        let (projected, indices) =
            project_limited_kaito_v16(backend, expected.acting_player, visible.prompt_seq).unwrap();
        assert_eq!(*visible, projected);
        assert_eq!(prompt.actions, visible.actions);
        assert_eq!(prompt.action_meanings, visible.action_meanings);
        assert_eq!(prompt.state, visible.state);
        let index = 1;
        let action_index = indices.iter().position(|&i| i == index).unwrap() as u32;
        let human_action = HumanActionRequestV1 { prompt_seq: prompt.prompt_seq, action_index };
        let step = CommandV1::Step {
            episode_id, authority_generation, actor: actor.into(),
            prompt_seq: visible.prompt_seq, action_index,
        };
        let Body::State { snapshot_id: Some(snapshot_id), .. } =
            body(&mut wire, 2, CommandV1::Snapshot)
        else { panic!() };
        let native_index = backend.fixture_original_index_v75(index).unwrap();
        let response = raw.step(expected.episode_id, expected.step, native_index).unwrap();
        assert_eq!(numeric.apply(&d, index).unwrap(), response);
        projector.submit(&mut human, human_action.clone()).unwrap();
        assert_eq!(human.current_response(), response);
        assert!(matches!(body(&mut wire, 3, step.clone()), Body::State { receipt: Some(_), .. }));
        assert_eq!(raw.kaito_fixture_state(), numeric.fixture_state());

        let mut free_state = None;
        for route in [RouteV1::ReplaceBaseMana, RouteV1::OrdinaryPayment] {
            if route == RouteV1::OrdinaryPayment {
                numeric.restore(&save).unwrap();
                assert_eq!(decision(&numeric).packet(), d.packet());
                assert!(numeric.apply(&d, index).is_err());
                let restored = decision(&numeric);
                numeric.apply(&restored, 0).unwrap();
            }
            let cost = decision(&numeric);
            let backend = cost.fixture_backend_v75();
            let facts = backend.observation().omniscience.as_ref().unwrap();
            let row = facts.casts.iter().find(|row| row.stage == StageV1::Announcing).unwrap();
            assert_eq!(row.route, Some(route));
            let terms = row.payment_terms_now.as_ref().unwrap();
            assert_eq!(terms.discard_required, 1);
            assert_eq!(terms.mana_components.is_empty(), route == RouteV1::ReplaceBaseMana);
            let discard = backend.actions().iter().position(|action| {
                matches!(action, A::Discard { cards, .. } if cards.len() == 1 && cards[0].arena_id == fodder.0)
            }).unwrap() as u32;
            numeric.apply(&cost, discard).unwrap();
            let paid = numeric.fixture_state();
            assert_eq!(tapped(paid, actor) - before, if route == RouteV1::ReplaceBaseMana { 0 } else { 2 });
            assert_eq!(paid.objects.get(fodder).zone, Zone::Graveyard);
            assert_eq!(paid.stack.last().unwrap().discarded, [fodder]);
            if route == RouteV1::ReplaceBaseMana {
                free_state = Some(paid.clone());
            }
        }
        let mut departed = free_state.unwrap();
        crate::event::propose_and_commit(
            &mut departed, crate::event::ProposedEvent::zone_change(omni, Zone::Exile),
        );
        let post = KaitoSession::from_session(fixture(&departed)).unwrap();
        let view = decision(&post);
        let facts = view.fixture_backend_v75().observation().omniscience.as_ref().unwrap();
        let cast = facts.casts.iter().find(|row| row.spell.object == spell.0).unwrap();
        assert!(!cast.original_permission_active);
        assert!(cast.permission.current_original_source.is_none());
        assert_eq!(cast.route, Some(RouteV1::ReplaceBaseMana));
        assert!(engine::omniscience_cast_v1::refuse_legacy(&departed).is_err());

        human.restore_v75(&human_save).unwrap();
        assert!(projector.submit(&mut human, human_action).is_err());
        assert!(matches!(body(&mut wire, 4, CommandV1::Restore { snapshot_id }), Body::State { .. }));
        assert!(matches!(body(&mut wire, 5, step), Body::Error { .. }));
    }
}
