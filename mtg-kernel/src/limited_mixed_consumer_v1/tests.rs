use super::*;
use crate::{
    card_def::card_id_by_name,
    current_mixed_policy_v1::KaitoDecision,
    current_tree_learner_v1::ModelV1,
    effect::sphinxs_tutelage_v1_tests as native,
    engine,
    human_bo3_v1::HumanKaitoProjectorV75,
    ids::PlayerId,
    limited_kaito_session_v16::{LimitedKaitoBodyV16 as Body, LimitedKaitoStateV16 as State},
    policy_observation_v6::tests::put,
    policy_surface_v5::LimitedAppendixV1,
    rl::ActionSemanticV1 as Action,
    rl_session::FastActorResponseV1,
    state::Zone,
    tutelage_public_v1::StageV1,
};

fn card(name: &str) -> u16 { card_id_by_name(name).unwrap() }

fn decision(session: &KaitoSession) -> KaitoDecision {
    let FastActorResponseV1::Decision(expected) = session.current_response() else {
        panic!("mixed fixture ended before its required decision");
    };
    session.encode(expected).unwrap()
}

#[test]
fn declared_twenty_printing_decks_share_one_numeric_human_wire_owner() {
    for first in [PlayerSeatV1::P0, PlayerSeatV1::P1] {
        let mut deck = vec![card("Island"); 20];
        deck.extend(PARTIAL_PRINTINGS_V1);
        let config = ResetV1 {
            expected_card_db_hash: KERNEL_CARDDB_HASH,
            episode_id: 550, env_seed: 550_020, starting_player: first,
            deck_ids: ["mixed-left".into(), "mixed-right".into()],
            mainboards: [deck.clone(), deck], max_physical_decisions: 2000, max_policy_steps: 2000,
        };
        assert!(crate::limited_return_consumer_v1::reset_numeric_v1(&config).is_err());
        assert!(crate::omniscience_consumer_v1::reset_numeric_v1(&config).is_err());
        let mut invalid = config.clone();
        invalid.mainboards[0][0] = card("Consuming Aberration");
        assert!(reset_numeric_v1(&invalid).is_err());

        let mut numeric = reset_numeric_reveal_facts_v3(&config).unwrap();
        assert!(numeric.fixture_state().reveal_knowledge_v4.is_some());
        let d = decision(&numeric);
        let backend = d.fixture_backend_v75();
        assert_eq!(backend.observation().schema, OBSERVATION_SCHEMA);
        assert_eq!(backend.candidates_v75().schema, CANDIDATES_SCHEMA);
        let input = numeric.training_input_v1(&d).unwrap();
        assert!(ModelV1::deterministic_fixture().forward(&input).unwrap().iter().all(|v| v.is_finite()));
        let expected = backend.expected();
        let human = reset_human_v1(&config, expected.acting_player).unwrap();
        let prompt = HumanKaitoProjectorV75::new(expected.acting_player)
            .project_current(&human, expected).unwrap();
        assert_eq!(prompt.schema, "mtg-kernel-human-mixed-fdn-decision/v1");
        let mut wire = JsonlServerV1::new();
        let response = wire.handle_request(RequestV1 {
            profile: PROFILE_V1.into(), request_id: "reset".into(),
            command: CommandV1::Reset { config: config.clone() },
        });
        let ReplyBodyV1::State { payload } = response.body else { panic!("mixed wire refused") };
        let Body::State { state, .. } = payload.body else { panic!("mixed wire not current") };
        let State::Decision { decision: visible, .. } = *state else { panic!("mixed wire terminal") };
        assert_eq!(visible.schema, "mtg-kernel-limited-mixed-fdn-decision/v1");
        assert_eq!(prompt.actions, visible.actions);
        let snapshot = numeric.snapshot();
        numeric.apply(&d, 0).unwrap();
        numeric.restore(&snapshot).unwrap();
        assert_eq!(decision(&numeric).fixture_backend_v75().observation(), backend.observation());
        let mut foreign = reset_numeric_reveal_facts_v3(&config).unwrap();
        assert!(foreign.restore(&snapshot).is_err());
    }
}

#[test]
fn paid_omniscience_casts_tutelage_and_its_paid_draw_mills_through_mixed_owner() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = native::ready(actor, &["Forest", "Island", "Forest", "Island"]);
        engine::omniscience_cast_v1::tests::paid(&mut state, actor);
        let tutelage = put(&mut state, actor, "Sphinx's Tutelage", Zone::Hand);
        let hidden = put(&mut state, actor.opponent(), "Island", Zone::Hand);
        let mut paired = state.clone();
        let other = paired.objects.get_mut(hidden);
        other.card_def = card("Forest");
        other.name = "Forest".into();
        other.v4 = crate::state::ObjectStateV4::from_card_def(other.card_def);
        let owner = |state| KaitoSession::from_session(
            FastActorSessionV1::from_kaito_v75_fixture_state_scoped(state, LimitedAppendixV1::Mixed)
        ).unwrap();
        let mut numeric = owner(state);
        let paired = owner(paired);
        assert_eq!(decision(&numeric).fixture_backend_v75().observation(),
            decision(&paired).fixture_backend_v75().observation());
        let mut announced = false;
        let mut free_route = false;
        let mut activated = false;
        let mut completed = false;
        for _ in 0..80 {
            let d = decision(&numeric);
            let backend = d.fixture_backend_v75();
            let facts = backend.observation();
            if activated && facts.tutelage.as_ref().unwrap().draws.iter()
                .flat_map(|draw| &draw.observers).any(|trigger| trigger.stage == StageV1::MillEnded
                    && trigger.batches.iter().any(|batch| batch.visible_milled_in_move_order.len() == 2))
            {
                completed = true;
                break;
            }
            let rows = &backend.candidates_v75().candidates;
            let pick = rows.iter().position(|row| !announced && matches!(&row.action,
                Action::CastSpell { source, .. } if source.arena_id == tutelage.0))
                .or_else(|| rows.iter().position(|row| matches!(row.omniscience_action,
                    Some(crate::omniscience_consumer_v1::ActionV1::ChooseRoute {
                        route: crate::omniscience_public_v1::RouteV1::ReplaceBaseMana,
                    }))))
                .or_else(|| rows.iter().position(|row| !activated && row.tutelage_action
                    == Some(crate::tutelage_consumer_v1::ActionV1::ActivateDrawDiscard)))
                .or_else(|| rows.iter().position(|row| matches!(row.tutelage_action,
                    Some(crate::tutelage_consumer_v1::ActionV1::TargetOpponent { .. }
                        | crate::tutelage_consumer_v1::ActionV1::OrderMilledCard { .. }
                        | crate::tutelage_consumer_v1::ActionV1::DiscardAfterDraw))))
                .or_else(|| rows.iter().position(|row| matches!(row.action,
                    Action::Pass { .. } | Action::ChooseTriggerOrderNext { .. } | Action::OrderTriggers { .. })))
                .expect("no native mixed interaction continuation");
            let row = &rows[pick];
            let before = native::taps(numeric.fixture_state(), actor);
            numeric.apply(&d, pick as u32).unwrap();
            if matches!(&row.action, Action::CastSpell { source, .. } if source.arena_id == tutelage.0) {
                announced = true;
            }
            if matches!(row.omniscience_action, Some(crate::omniscience_consumer_v1::ActionV1::ChooseRoute {
                route: crate::omniscience_public_v1::RouteV1::ReplaceBaseMana,
            })) {
                free_route = true;
                assert_eq!(native::taps(numeric.fixture_state(), actor), before);
            }
            if row.tutelage_action == Some(crate::tutelage_consumer_v1::ActionV1::ActivateDrawDiscard) {
                activated = true;
                assert_eq!(native::taps(numeric.fixture_state(), actor) - before, 6);
            }
            let input = numeric.training_input_v1(&decision(&numeric)).unwrap();
            assert!(ModelV1::deterministic_fixture().forward(&input).unwrap().iter().all(|v| v.is_finite()));
        }
        assert!(announced && free_route && activated && completed);
    }
}
