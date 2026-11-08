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
    rl_session::{FastActorResponseV1, KaitoSessionV75},
    state::{GameState, Target},
};

fn config(actor: PlayerId) -> ResetV1 {
    let forest = card_id_by_name("Forest").unwrap();
    let island = card_id_by_name("Island").unwrap();
    let mut mainboards = [vec![forest; 40], vec![forest; 40]];
    mainboards[actor.index()] = [vec![PRINTING; 8], vec![forest; 16], vec![island; 16]].concat();
    ResetV1 {
        expected_card_db_hash: KERNEL_CARDDB_HASH,
        episode_id: 550,
        env_seed: 550_465,
        starting_player: actor.into(),
        deck_ids: ["zimone-left".into(), "zimone-right".into()],
        mainboards,
        max_physical_decisions: 2_000,
        max_policy_steps: 2_000,
    }
}

fn decision(session: &KaitoSession) -> KaitoDecision {
    let FastActorResponseV1::Decision(expected) = session.current_response() else {
        panic!("Zimone decision absent: {:?}", session.current_response());
    };
    session.encode(expected).unwrap()
}

fn body(wire: &mut JsonlServerV1, serial: u64, command: CommandV1) -> Body {
    let reply = wire.handle_request(RequestV1 {
        profile: PROFILE_V1.into(),
        request_id: serial.to_string(),
        command,
    });
    assert_eq!(reply.profile, PROFILE_V1);
    let ReplyBodyV1::State { payload } = reply.body else {
        panic!("Zimone request rejected: {reply:?}");
    };
    payload.body
}

fn tapped_lands(state: &GameState, actor: PlayerId) -> usize {
    state.players[actor.index()]
        .battlefield
        .iter()
        .filter(|id| {
            let object = state.objects.get(**id);
            object.tapped && crate::card_def::CARD_DEFS[usize::from(object.card_def)].is_land
        })
        .count()
}

#[test]
fn exact_forty_card_v3_reset_pays_cast_and_both_abilities_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let config = config(actor);
        assert!(crate::limited_v68_v75_consumer_v1::reset_numeric_v1(&config).is_err());
        assert!(crate::embercleave_consumer_v1::reset_numeric_v1(&config).is_err());
        let mut foreign = config.clone();
        foreign.mainboards[actor.index()][0] = 462;
        assert!(reset_numeric_v1(&foreign).is_err());

        let mut wire = JsonlServerV1::new();
        let Body::State { state, .. } = body(
            &mut wire,
            1,
            CommandV1::Reset {
                config: config.clone(),
            },
        ) else {
            panic!("missing declared Zimone reset");
        };
        let State::Decision {
            decision: wire_opening,
            ..
        } = *state
        else {
            panic!("missing declared Zimone wire opening");
        };
        assert_eq!(wire_opening.schema, "mtg-kernel-limited-zimone-decision/v1");

        let mut numeric = reset_numeric_reveal_facts_v3(&config).unwrap();
        assert!(numeric.fixture_state().reveal_knowledge_v4.is_some());
        let opening = decision(&numeric);
        let expected = opening.fixture_backend_v75().expected();
        let human = reset_human_v1(&config, expected.acting_player).unwrap();
        let prompt = HumanKaitoProjectorV75::new(expected.acting_player)
            .project_current(&human, expected)
            .unwrap();
        assert_eq!(prompt.schema, "mtg-kernel-human-zimone-decision/v1");
        assert_eq!(prompt.actions, wire_opening.actions);
        assert_eq!(prompt.action_meanings, wire_opening.action_meanings);

        let mut zimone = None;
        let mut paid_double = false;
        let mut finished = false;
        for _ in 0..550 {
            let state = numeric.fixture_state();
            if let Some(source) = zimone {
                if paid_double && state.stack.is_empty() {
                    assert_eq!(state.objects.get(source).counters.plus1_plus1, 2);
                    finished = true;
                    break;
                }
            }
            let d = decision(&numeric);
            let rows = &d.fixture_backend_v75().candidates_v75().candidates;
            let source = zimone;
            let allow_activation =
                source.is_some_and(|id| state.objects.get(id).counters.plus1_plus1 == 1);
            let choose_target = |row: &crate::kaito_candidates_v75::CandidateMeaningV75| {
                matches!(
                    row.zimone_action,
                    Some(ActionV1::TargetChoice {
                        selected_count: 0,
                        finish: false,
                        ..
                    })
                ) && matches!(&row.action,
                    Action::ChooseTarget { target: TargetRefV1::Object { object }, .. }
                    | Action::ChooseEffectTarget { target: TargetRefV1::Object { object }, .. }
                    if source == Some(ObjectId(object.arena_id)))
            };
            let i = rows
                .iter()
                .position(choose_target)
                .or_else(|| {
                    rows.iter().position(|row| {
                        matches!(
                            row.zimone_action,
                            Some(ActionV1::TargetChoice { finish: true, .. })
                        ) || (allow_activation
                            && !paid_double
                            && row.zimone_action == Some(ActionV1::ActivateDoubleEveryKind))
                    })
                })
                .or_else(|| {
                    rows.iter()
                        .position(|row| matches!(row.action, Action::PlayLand { .. }))
                })
                .or_else(|| {
                    rows.iter().position(|row| {
                        zimone.is_none()
                            && matches!(&row.action,
                        Action::CastSpell { source, .. } if source.card_db_id == PRINTING)
                    })
                })
                .or_else(|| {
                    rows.iter().position(|row| {
                        matches!(
                            row.action,
                            Action::ChooseLondonMulligan {
                                mulligan: false,
                                ..
                            } | Action::ChooseAttackerInclusion { include: false, .. }
                                | Action::ChooseBlockerInclusion { include: false, .. }
                                | Action::ChooseTriggerOrderNext { .. }
                                | Action::OrderTriggers { .. }
                                | Action::Pass { .. }
                                | Action::Discard { .. }
                        )
                    })
                })
                .expect("missing native Zimone fixture action");
            let before = tapped_lands(state, actor);
            let row = &rows[i];
            let cast = match &row.action {
                Action::CastSpell { source, .. } => Some(ObjectId(source.arena_id)),
                _ => None,
            };
            let finishing = matches!(
                row.zimone_action,
                Some(ActionV1::TargetChoice {
                    kind: AbilityKindV1::ActivateDoubleEveryKind,
                    selected_count: 1,
                    finish: true,
                    ..
                })
            );
            if finishing {
                let input = numeric.training_input_v1(&d).unwrap();
                assert!(ModelV1::deterministic_fixture()
                    .forward(&input)
                    .unwrap()
                    .iter()
                    .all(|x| x.is_finite()));
            }
            numeric.apply(&d, i as u32).unwrap();
            if let Some(id) = cast {
                assert_eq!(numeric.fixture_state().objects.get(id).zone, Zone::Stack);
                assert_eq!(tapped_lands(numeric.fixture_state(), actor) - before, 4);
                zimone = Some(id);
            }
            if finishing {
                let state = numeric.fixture_state();
                let source = zimone.unwrap();
                assert_eq!(
                    state.stack.last().unwrap().targets,
                    [Target::Object(source)]
                );
                assert!(state.objects.get(source).tapped);
                assert_eq!(tapped_lands(state, actor) - before, 2);
                assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
                paid_double = true;
            }
        }
        assert!(
            finished,
            "declared reset did not complete Zimone's paid activation"
        );
    }
}

#[test]
fn paid_prefix_hidden_pair_native_human_wire_numeric_and_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, source) = crate::zimone_paradox_sculptor_v1::tests::paid_ready(actor);
        let first = put(&mut state, actor, "Fishing Pole", Zone::Battlefield);
        let second = put(&mut state, actor, "Fishing Pole", Zone::Battlefield);
        let hidden = put(&mut state, actor.opponent(), "Forest", Zone::Hand);
        state.objects.get_mut(first).counters.charge = 120_000;
        state.objects.get_mut(first).counters.stash = 300;
        engine::step(&mut state, engine::Action::ActivateAbility(source, 0)).unwrap();
        engine::advance_until_decision(&mut state);
        engine::step(
            &mut state,
            engine::Action::ChooseEffectTarget(Target::Object(second)),
        )
        .unwrap();
        engine::advance_until_decision(&mut state);

        let fixture = |state: &GameState| {
            FastActorSessionV1::from_kaito_v75_fixture_state_scoped(
                state.clone(),
                LimitedAppendixV1::Zimone,
            )
        };
        let mut altered = state.clone();
        altered.objects.get_mut(hidden).card_def = PRINTING;
        let other = KaitoSession::from_session(fixture(&altered)).unwrap();
        let mut numeric = KaitoSession::from_session(fixture(&state)).unwrap();
        let mut raw = fixture(&state);
        let mut human = HumanKaitoSessionV75::from_session(fixture(&state), actor.into()).unwrap();
        let mut projector = HumanKaitoProjectorV75::new(actor.into());
        let mut wire = JsonlServerV1::new();
        wire.0
            .install_fixture_core(KaitoSessionV75::from_session(fixture(&state)).unwrap());
        let mut other_wire = JsonlServerV1::new();
        other_wire
            .0
            .install_fixture_core(KaitoSessionV75::from_session(fixture(&altered)).unwrap());

        let d = decision(&numeric);
        assert_ne!(state.state_hash(), altered.state_hash());
        assert_eq!(d.packet(), decision(&other).packet());
        assert!(other.training_input_v1(&d).is_err());
        let save = numeric.snapshot();
        let mut other = other;
        assert!(other.restore(&save).is_err());
        let backend = d.fixture_backend_v75();
        let rows = &backend.candidates_v75().candidates;
        assert!(rows.iter().any(|row| matches!(
            row.zimone_action,
            Some(ActionV1::TargetChoice {
                selected_count: 1,
                finish: true,
                ..
            })
        )));
        let index = rows
            .iter()
            .position(|row| {
                matches!(
                    row.zimone_action,
                    Some(ActionV1::TargetChoice {
                        selected_count: 1,
                        finish: false,
                        ..
                    })
                ) && matches!(&row.action,
                Action::ChooseEffectTarget { target: TargetRefV1::Object { object }, .. }
                if object.arena_id == first.0)
            })
            .unwrap() as u32;
        let expected = backend.expected();
        let prompt = projector.project_current(&human, expected).unwrap();
        let human_save = human.snapshot_v75();
        let current = body(&mut wire, 1, CommandV1::Current);
        assert_eq!(current, body(&mut other_wire, 1, CommandV1::Current));
        let Body::State { state: current, .. } = current else {
            panic!()
        };
        let State::Decision {
            episode_id,
            authority_generation,
            decision: visible,
        } = *current
        else {
            panic!()
        };
        let (projected, indices) =
            project_limited_kaito_v16(backend, expected.acting_player, visible.prompt_seq).unwrap();
        assert_eq!(*visible, projected);
        assert_eq!(prompt.state, visible.state);
        assert_eq!(prompt.actions, visible.actions);
        assert_eq!(prompt.action_meanings, visible.action_meanings);
        let action_index = indices.iter().position(|&i| i == index).unwrap() as u32;
        let human_action = HumanActionRequestV1 {
            prompt_seq: prompt.prompt_seq,
            action_index,
        };
        let command = CommandV1::Step {
            episode_id,
            authority_generation,
            actor: actor.into(),
            prompt_seq: visible.prompt_seq,
            action_index,
        };
        let Body::State {
            snapshot_id: Some(snapshot_id),
            ..
        } = body(&mut wire, 2, CommandV1::Snapshot)
        else {
            panic!()
        };
        let original = backend.fixture_original_index_v75(index).unwrap();
        let native = raw
            .step(expected.episode_id, expected.step, original)
            .unwrap();
        assert_eq!(numeric.apply(&d, index).unwrap(), native);
        projector.submit(&mut human, human_action.clone()).unwrap();
        assert_eq!(human.current_response(), native);
        assert_eq!(raw.kaito_fixture_state(), numeric.fixture_state());
        assert!(matches!(
            body(&mut wire, 3, command.clone()),
            Body::State {
                receipt: Some(_),
                ..
            }
        ));
        assert_eq!(
            numeric.fixture_state().stack.last().unwrap().targets,
            [Target::Object(second), Target::Object(first)]
        );
        assert!(numeric.fixture_state().objects.get(source).tapped);

        numeric.restore(&save).unwrap();
        human.restore_v75(&human_save).unwrap();
        assert_eq!(decision(&numeric).packet(), d.packet());
        assert!(numeric.apply(&d, index).is_err());
        assert!(projector.submit(&mut human, human_action).is_err());
        assert!(matches!(
            body(&mut wire, 4, CommandV1::Restore { snapshot_id }),
            Body::State { .. }
        ));
        assert!(matches!(body(&mut wire, 5, command), Body::Error { .. }));
    }
}
