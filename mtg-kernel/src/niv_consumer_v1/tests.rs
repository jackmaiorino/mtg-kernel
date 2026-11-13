use super::*;
use crate::{
    card_def::KERNEL_CARDDB_HASH,
    current_mixed_policy_v1::KaitoDecision,
    current_tree_learner_v1::ModelV1,
    human_bo3_v1::{project_limited_kaito_v16, HumanActionRequestV1, HumanKaitoProjectorV75},
    ids::{ObjectId, PlayerId},
    limited_kaito_session_v16::{LimitedKaitoBodyV16 as Body, LimitedKaitoStateV16 as State},
    niv_mizzet_public_v63::{capture_for_consumer, DrawResultV1},
    policy_surface_v5::LimitedAppendixV1,
    rl::{ActionSemanticV1 as Action, TargetRefV1},
    rl_session::{FastActorResponseV1, KaitoSessionV75},
};

fn card(name: &str) -> u16 {
    crate::card_def::card_id_by_name(name).unwrap()
}

fn config(actor: PlayerId) -> ResetV1 {
    let mut deck = vec![PRINTING_V1; 4];
    deck.extend([card("Lightning Bolt"); 4]);
    deck.extend([card("Island"); 16]);
    deck.extend([card("Mountain"); 16]);
    let mut mainboards = [vec![card("Forest"); 40], vec![card("Forest"); 40]];
    mainboards[actor.index()] = deck;
    ResetV1 {
        expected_card_db_hash: KERNEL_CARDDB_HASH,
        episode_id: 550,
        // Both seats draw Niv, Bolt and their colors within this bounded fixture.
        // The declared multiset, real payment assertions and decision limit stay fixed.
        env_seed: 550_476,
        starting_player: actor.into(),
        deck_ids: ["niv-left".into(), "niv-right".into()],
        mainboards,
        max_physical_decisions: 2_000,
        max_policy_steps: 2_000,
    }
}

fn req(serial: impl ToString, command: CommandV1) -> RequestV1 {
    RequestV1 {
        profile: PROFILE_V1.into(),
        request_id: serial.to_string(),
        command,
    }
}

fn decision(session: &KaitoSession) -> KaitoDecision {
    let FastActorResponseV1::Decision(expected) = session.current_response() else {
        panic!("Niv decision absent: {:?}", session.current_response());
    };
    session.encode(expected).unwrap()
}

fn body(wire: &mut JsonlServerV1, serial: u64, command: CommandV1) -> Body {
    let reply = wire.handle_request(req(serial, command));
    assert_eq!(reply.profile, PROFILE_V1);
    let ReplyBodyV1::State { payload } = reply.body else {
        panic!("profile request rejected: {reply:?}");
    };
    payload.body
}

fn facts(d: &KaitoDecision) -> &ConsumerFactsV1 {
    d.fixture_backend_v75().observation().niv.as_ref().unwrap()
}

#[test]
fn exact_forty_card_v3_reset_pays_niv_and_bolt_and_projects_actual_draws_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let config = config(actor);
        assert!(crate::artist_consumer_v1::reset_numeric_v1(&config).is_err());
        assert!(crate::limited_return_consumer_v1::reset_numeric_v1(&config).is_err());
        assert!(crate::limited_v68_v75_consumer_v1::reset_numeric_v1(&config).is_err());
        assert!(KaitoSession::reset_explicit_mainboards(
            config.episode_id,
            config.env_seed,
            (100, 100),
            config.deck_ids.clone(),
            config.mainboards.clone(),
            actor,
        )
        .is_err());
        for illegal in [460, 486, 492, 495, u16::MAX] {
            let mut invalid = config.clone();
            invalid.mainboards[actor.index()][0] = illegal;
            assert!(reset_numeric_v1(&invalid).is_err());
        }
        let mut wire = JsonlServerV1::new();
        let reset = req(
            "reset",
            CommandV1::Reset {
                config: config.clone(),
            },
        );
        let line = serde_json::to_string(&reset).unwrap();
        let reply: serde_json::Value = serde_json::from_str(&wire.handle_line(&line)).unwrap();
        assert_eq!(
            reply["payload"]["state"]["decision"]["schema"],
            "mtg-kernel-limited-niv-decision/v1"
        );
        let mut numeric = reset_numeric_reveal_facts_v3(&config).unwrap();
        assert!(numeric.fixture_state().reveal_knowledge_v4.is_some());
        let opening = decision(&numeric);
        let expected = opening.fixture_backend_v75().expected();
        let human = reset_human_v1(&config, expected.acting_player).unwrap();
        let view = HumanKaitoProjectorV75::new(expected.acting_player)
            .project_current(&human, expected)
            .unwrap();
        assert_eq!(view.schema, "mtg-kernel-human-niv-decision/v1");
        assert_eq!(
            opening.fixture_backend_v75().observation().schema,
            OBSERVATION_SCHEMA
        );

        let mut niv = None;
        let mut completed = false;
        for _ in 0..550 {
            let current = decision(&numeric);
            let observed = facts(&current);
            if let Some(result) = observed.results.first() {
                assert_eq!(result.outcome, DrawOutcomeV1::Resolved { cards_drawn: 3 });
                assert_eq!(observed.appendix.damage_draws[0].committed_damage, 3);
                assert_eq!(
                    observed.appendix.damage_draws[0]
                        .damage_source
                        .damage_controller,
                    actor.into()
                );
                assert!(observed.appendix.no_maximum_hand_size[actor.index()]);
                assert_eq!(
                    numeric.fixture_state().objects.get(niv.unwrap()).zone,
                    Zone::Battlefield
                );
                let input = numeric.training_input_v1(&current).unwrap();
                assert!(ModelV1::deterministic_fixture()
                    .forward(&input)
                    .unwrap()
                    .iter()
                    .all(|x| x.is_finite()));
                completed = true;
                break;
            }
            let in_play = niv.is_some_and(|id| {
                numeric.fixture_state().objects.get(id).zone == Zone::Battlefield
            });
            let actions = current.fixture_backend_v75().actions();
            let selected = actions
                .iter()
                .position(|action| {
                    matches!(action,
                        Action::CastSpell { source, .. } if
                            (niv.is_none() && source.card_db_id == PRINTING_V1)
                            || (in_play && source.card_db_id == card("Lightning Bolt")
                                && numeric.fixture_state().stack.is_empty())
                    )
                })
                .or_else(|| {
                    actions.iter().position(|action| {
                        matches!(action,
                            Action::ChooseTarget { target: TargetRefV1::Player { player }, .. }
                                if *player == PlayerSeatV1::from(actor.opponent())
                        )
                    })
                })
                .or_else(|| {
                    actions
                        .iter()
                        .position(|action| matches!(action, Action::PlayLand { .. }))
                })
                .or_else(|| {
                    actions.iter().position(|action| {
                        matches!(
                            action,
                            Action::ChooseLondonMulligan {
                                mulligan: false,
                                ..
                            } | Action::ChooseAttackerInclusion { include: false, .. }
                                | Action::ChooseBlockerInclusion { include: false, .. }
                                | Action::Pass { .. }
                                | Action::Discard { .. }
                                | Action::ChooseTriggerOrderNext { .. }
                                | Action::OrderTriggers { .. }
                        )
                    })
                })
                .unwrap_or_else(|| panic!("unhandled Niv reset menu: {actions:?}"));
            let casting_niv = match &actions[selected] {
                Action::CastSpell { source, .. } if source.card_db_id == PRINTING_V1 => {
                    Some(ObjectId(source.arena_id))
                }
                _ => None,
            };
            let tapped = |state: &crate::state::GameState| {
                state.players[actor.index()]
                    .battlefield
                    .iter()
                    .filter(|&&id| state.objects.get(id).tapped)
                    .count()
            };
            let before = tapped(numeric.fixture_state());
            numeric.apply(&current, selected as u32).unwrap();
            if let Some(source) = casting_niv {
                let state = numeric.fixture_state();
                assert_eq!(tapped(state) - before, 6);
                assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
                assert_eq!(state.objects.get(source).zone, Zone::Stack);
                niv = Some(source);
            }
        }
        assert!(completed, "real declared-deck Niv draw did not resolve");
    }
}

#[test]
fn paid_niv_draw_native_index_parity_private_top_and_restore_both_seats() {
    use crate::niv_mizzet_public_v63::tests::published_with_top;

    for actor in [PlayerId::P0, PlayerId::P1] {
        let make = |hidden| {
            let (state, _, _) = published_with_top(actor, hidden);
            move || {
                FastActorSessionV1::from_kaito_v75_fixture_state_scoped(
                    state.clone(),
                    LimitedAppendixV1::Niv,
                )
            }
        };
        let (make_a, make_b) = (make("Forest"), make("Mountain"));
        let mut raw = make_a();
        let mut a = KaitoSession::from_session(make_a()).unwrap();
        let mut b = KaitoSession::from_session(make_b()).unwrap();
        let mut human = HumanKaitoSessionV75::from_session(make_a(), actor.into()).unwrap();
        let mut projector = HumanKaitoProjectorV75::new(actor.into());
        let wire_make = |native| {
            let mut wire = JsonlServerV1::new();
            wire.0
                .install_fixture_core(KaitoSessionV75::from_session(native).unwrap());
            wire
        };
        let (mut wire, mut other_wire) = (wire_make(make_a()), wire_make(make_b()));
        let original = decision(&a);
        assert_ne!(
            a.fixture_state().state_hash(),
            b.fixture_state().state_hash()
        );
        assert_eq!(original.packet(), decision(&b).packet());
        assert_eq!(
            body(&mut wire, 1, CommandV1::Current),
            body(&mut other_wire, 1, CommandV1::Current)
        );
        assert!(b.training_input_v1(&original).is_err());
        assert!(b.restore(&a.snapshot()).is_err());
        let saved = a.snapshot();
        let human_saved = human.snapshot_v75();
        let Body::State {
            snapshot_id: Some(snapshot_id),
            ..
        } = body(&mut wire, 2, CommandV1::Snapshot)
        else {
            panic!("Niv wire snapshot absent")
        };
        let mut first_step = None;
        let mut first_human = None;
        let mut realized = false;
        for offset in 0..20 {
            let d = decision(&a);
            let other = decision(&b);
            if !facts(&d).results.is_empty() {
                assert_eq!(
                    facts(&d).results,
                    vec![DrawResultV1 {
                        occurrence: 0,
                        outcome: DrawOutcomeV1::Resolved { cards_drawn: 1 },
                    }]
                );
                assert_eq!(
                    capture_for_consumer(a.fixture_state(), actor.opponent()).unwrap(),
                    capture_for_consumer(b.fixture_state(), actor.opponent()).unwrap()
                );
                let input = a.training_input_v1(&d).unwrap();
                assert!(ModelV1::deterministic_fixture()
                    .forward(&input)
                    .unwrap()
                    .iter()
                    .all(|x| x.is_finite()));
                realized = true;
                break;
            }
            let backend = d.fixture_backend_v75();
            let expected = backend.expected();
            let selected = backend
                .actions()
                .iter()
                .position(|a| matches!(a, Action::Pass { .. }))
                .unwrap() as u32;
            assert_eq!(backend.actions(), other.fixture_backend_v75().actions());
            let seq = 3 + 2 * offset;
            let Body::State { state, .. } = body(&mut wire, seq, CommandV1::Current) else {
                panic!()
            };
            let State::Decision {
                episode_id,
                authority_generation,
                decision: visible,
            } = *state
            else {
                panic!()
            };
            let (projected, indices) =
                project_limited_kaito_v16(backend, expected.acting_player, visible.prompt_seq)
                    .unwrap();
            assert_eq!(*visible, projected);
            let action_index = indices.iter().position(|&index| index == selected).unwrap() as u32;
            if expected.acting_player == PlayerSeatV1::from(actor) {
                let menu = projector.project_current(&human, expected).unwrap();
                assert_eq!(menu.state, visible.state);
                assert_eq!(menu.actions, visible.actions);
                let command = HumanActionRequestV1 {
                    prompt_seq: menu.prompt_seq,
                    action_index,
                };
                first_human.get_or_insert_with(|| command.clone());
                projector.submit(&mut human, command).unwrap();
            } else {
                assert!(projector.project_current(&human, expected).is_err());
                human
                    .step_opponent_with(expected, |public, actions| {
                        assert_eq!(public, backend.observation());
                        assert_eq!(actions, backend.actions());
                        Ok(selected)
                    })
                    .unwrap();
            }
            let command = CommandV1::Step {
                episode_id,
                authority_generation,
                actor: expected.acting_player,
                prompt_seq: visible.prompt_seq,
                action_index,
            };
            first_step.get_or_insert_with(|| command.clone());
            assert!(matches!(
                body(&mut wire, seq + 1, command),
                Body::State {
                    receipt: Some(_),
                    ..
                }
            ));
            let native_index = backend.fixture_original_index_v75(selected).unwrap();
            let native = raw
                .step(expected.episode_id, expected.step, native_index)
                .unwrap();
            assert_eq!(a.apply(&d, selected).unwrap(), native);
            assert_eq!(b.apply(&other, selected).unwrap(), native);
            assert_eq!(human.current_response(), native);
            assert_eq!(a.fixture_state(), raw.kaito_fixture_state());
        }
        assert!(realized);
        a.restore(&saved).unwrap();
        assert_eq!(decision(&a).packet(), original.packet());
        assert!(a.training_input_v1(&original).is_err());
        assert!(a.apply(&original, 0).is_err());
        human.restore_v75(&human_saved).unwrap();
        assert!(projector.submit(&mut human, first_human.unwrap()).is_err());
        assert!(matches!(
            body(&mut wire, 100, CommandV1::Restore { snapshot_id }),
            Body::State { .. }
        ));
        assert!(matches!(
            body(&mut wire, 101, first_step.unwrap()),
            Body::Error { .. }
        ));
    }
}
