use super::*;
use crate::{
    card_def::{card_id_by_name, CardType, KERNEL_CARDDB_HASH},
    current_mixed_policy_v1::KaitoDecision,
    current_tree_learner_v1::ModelV1,
    engine::{self, imprisoned_moon_v1_tests as native},
    human_bo3_v1::{project_limited_kaito_v16, HumanActionRequestV1, HumanKaitoProjectorV75},
    ids::{ObjectId, PlayerId},
    limited_kaito_session_v16::{LimitedKaitoBodyV16 as Body, LimitedKaitoStateV16 as State},
    policy_observation_v6::tests::put,
    policy_surface_v5::LimitedAppendixV1,
    rl_session::{FastActorResponseV1, KaitoSessionV75},
    state::GameState,
};

fn card(name: &str) -> u16 {
    card_id_by_name(name).unwrap()
}

fn config(actor: PlayerId) -> ResetV1 {
    let mut mainboards = [vec![card("Island"); 40], vec![card("Island"); 40]];
    mainboards[actor.index()] = [
        vec![PRINTING; 6],
        vec![card("Llanowar Elves"); 8],
        vec![card("Forest"); 12],
        vec![card("Island"); 14],
    ].concat();
    ResetV1 {
        expected_card_db_hash: KERNEL_CARDDB_HASH,
        episode_id: 550,
        env_seed: 550_484,
        starting_player: actor.into(),
        deck_ids: ["moon-left".into(), "moon-right".into()],
        mainboards,
        max_physical_decisions: 2_000,
        max_policy_steps: 2_000,
    }
}

fn decision(session: &KaitoSession) -> KaitoDecision {
    let FastActorResponseV1::Decision(expected) = session.current_response() else {
        panic!("Moon decision absent: {:?}", session.current_response());
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
        panic!("Moon wire request rejected: {reply:?}");
    };
    payload.body
}

#[test]
fn declared_forty_card_v3_reset_pays_cast_and_grants_native_mana_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let config = config(actor);
        assert!(crate::limited_v68_v75_consumer_v1::reset_numeric_v1(&config).is_err());
        assert!(crate::tutelage_consumer_v1::reset_numeric_v1(&config).is_err());
        let mut invalid = config.clone();
        invalid.mainboards[actor.index()][0] = 466;
        assert!(reset_numeric_v1(&invalid).is_err());
        let mut wire = JsonlServerV1::new();
        let Body::State { state, .. } = body(&mut wire, 1, CommandV1::Reset {
            config: config.clone(),
        }) else { panic!() };
        let State::Decision { decision: wire_opening, .. } = *state else { panic!() };
        assert_eq!(wire_opening.schema, "mtg-kernel-limited-moon-decision/v1");
        let mut numeric = reset_numeric_reveal_facts_v3(&config).unwrap();
        assert!(numeric.fixture_state().reveal_knowledge_v4.is_some());
        let opening = decision(&numeric);
        let expected = opening.fixture_backend_v75().expected();
        let human = reset_human_v1(&config, expected.acting_player).unwrap();
        let prompt = HumanKaitoProjectorV75::new(expected.acting_player)
            .project_current(&human, expected).unwrap();
        assert_eq!(prompt.schema, "mtg-kernel-human-moon-decision/v1");
        assert_eq!(prompt.actions, wire_opening.actions);

        let elf = card("Llanowar Elves");
        let mut host = None;
        let mut aura = None;
        let mut completed = false;
        for _ in 0..550 {
            let d = decision(&numeric);
            let backend = d.fixture_backend_v75();
            let rows = &backend.candidates_v75().candidates;
            let state = numeric.fixture_state();
            let next = rows.iter().position(|row| {
                row.moon_action == Some(ActionV1::TapForColorless)
                    || row.moon_action == Some(ActionV1::ChooseCastTarget)
                        && matches!(&row.action, Action::ChooseTarget {
                            target: TargetRefV1::Object { object }, ..
                        } if host == Some(ObjectId(object.arena_id)))
                    || matches!(&row.action, Action::CastSpell { source, .. }
                        if (host.is_none() && source.card_db_id == elf)
                            || (aura.is_none() && source.card_db_id == PRINTING
                                && host.is_some_and(|host| state.objects.get(host).zone == Zone::Battlefield)))
            }).or_else(|| rows.iter().position(|row| matches!(row.action, Action::PlayLand { .. })))
                .or_else(|| rows.iter().position(|row| matches!(row.action,
                    Action::ChooseLondonMulligan { mulligan: false, .. }
                    | Action::ChooseAttackerInclusion { include: false, .. }
                    | Action::ChooseBlockerInclusion { include: false, .. }
                    | Action::Pass { .. }
                    | Action::Discard { .. }
                ))).expect("missing native Moon fixture action");
            let before = native::tapped(state, actor);
            let row = &rows[next];
            if row.moon_action == Some(ActionV1::TapForColorless) {
                let facts = backend.observation().moon.as_ref().unwrap();
                let live = facts.hosts.iter().find(|row| host == Some(ObjectId(row.card.object))).unwrap();
                assert_eq!(live.form.types, [CardType::Land]);
                assert_eq!(live.form.color_mask, 0);
                assert!(!live.form.printed_abilities_active);
                assert!(live.moon_grants_tap_for_colorless && live.granted_mana_available_now);
                let input = numeric.training_input_v1(&d).unwrap();
                assert!(ModelV1::deterministic_fixture().forward(&input).unwrap().iter().all(|value| value.is_finite()));
                numeric.apply(&d, next as u32).unwrap();
                let paid = numeric.fixture_state();
                assert!(paid.objects.get(host.unwrap()).tapped);
                assert_eq!(paid.players[actor.index()].mana_pool[ManaColor::C.pool_index()], 1);
                completed = true;
                break;
            }
            numeric.apply(&d, next as u32).unwrap();
            match &row.action {
                Action::CastSpell { source, .. } if source.card_db_id == elf => {
                    host = Some(ObjectId(source.arena_id));
                }
                Action::CastSpell { source, .. } if source.card_db_id == PRINTING => {
                    aura = Some(ObjectId(source.arena_id));
                }
                _ if row.moon_action == Some(ActionV1::ChooseCastTarget) => {
                    let paid = numeric.fixture_state();
                    assert_eq!(native::tapped(paid, actor) - before, 3);
                    assert_eq!(paid.objects.get(aura.unwrap()).zone, Zone::Stack);
                }
                _ => {}
            }
        }
        assert!(completed, "declared reset did not produce Moon-granted colorless mana");
    }
}

fn fixture(state: &GameState) -> FastActorSessionV1 {
    FastActorSessionV1::from_kaito_v75_fixture_state_scoped(state.clone(), LimitedAppendixV1::Moon)
}

fn paid_target_choice(actor: PlayerId, hidden: &str) -> (GameState, ObjectId, ObjectId) {
    let mut state = native::ready(actor, hidden);
    let host = native::spell(&mut state, actor, "Llanowar Elves", &[], 1);
    if matches!(engine::advance_until_decision(&mut state), engine::Decision::CastSpellOrPass {
        player, ..
    } if player != actor) {
        engine::step(&mut state, engine::Action::Pass).unwrap();
    }
    assert!(matches!(engine::advance_until_decision(&mut state), engine::Decision::CastSpellOrPass {
        player, ..
    } if player == actor));
    let source = put(&mut state, actor, public::NAME, Zone::Hand);
    engine::step(&mut state, engine::Action::CastSpell(source)).unwrap();
    assert!(matches!(engine::advance_until_decision(&mut state), engine::Decision::ChooseTargets {
        spell, player, ..
    } if spell == source && player == actor));
    (state, host, source)
}

#[test]
fn paid_cast_target_hidden_pair_indices_restore_and_fizzle_on_new_incarnation_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (state, host, source) = paid_target_choice(actor, "Island");
        let (other_state, _, _) = paid_target_choice(actor, public::NAME);
        assert_ne!(state.state_hash(), other_state.state_hash());
        let mut raw = fixture(&state);
        let mut numeric = KaitoSession::from_session(fixture(&state)).unwrap();
        let mut other = KaitoSession::from_session(fixture(&other_state)).unwrap();
        let mut human = HumanKaitoSessionV75::from_session(fixture(&state), actor.into()).unwrap();
        let mut projector = HumanKaitoProjectorV75::new(actor.into());
        let mut wire = JsonlServerV1::new();
        wire.0.install_fixture_core(KaitoSessionV75::from_session(fixture(&state)).unwrap());
        let mut other_wire = JsonlServerV1::new();
        other_wire.0.install_fixture_core(KaitoSessionV75::from_session(fixture(&other_state)).unwrap());

        let d = decision(&numeric);
        assert_eq!(d.packet(), decision(&other).packet());
        assert!(other.training_input_v1(&d).is_err());
        let save = numeric.snapshot();
        assert!(other.restore(&save).is_err());
        let backend = d.fixture_backend_v75();
        let rows = &backend.candidates_v75().candidates;
        let index = rows.iter().position(|row| row.moon_action == Some(ActionV1::ChooseCastTarget)
            && matches!(&row.action, Action::ChooseTarget {
                target: TargetRefV1::Object { object }, ..
            } if object.arena_id == host.0)).unwrap() as u32;
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
        assert_eq!(prompt.state, visible.state);
        assert_eq!(prompt.actions, visible.actions);
        assert_eq!(prompt.action_meanings, visible.action_meanings);
        let action_index = indices.iter().position(|&i| i == index).unwrap() as u32;
        let human_action = HumanActionRequestV1 { prompt_seq: prompt.prompt_seq, action_index };
        let step = CommandV1::Step {
            episode_id, authority_generation, actor: actor.into(),
            prompt_seq: visible.prompt_seq, action_index,
        };
        let Body::State { snapshot_id: Some(snapshot_id), .. } =
            body(&mut wire, 2, CommandV1::Snapshot)
        else { panic!() };
        let original = backend.fixture_original_index_v75(index).unwrap();
        let response = raw.step(expected.episode_id, expected.step, original).unwrap();
        assert_eq!(numeric.apply(&d, index).unwrap(), response);
        projector.submit(&mut human, human_action.clone()).unwrap();
        assert_eq!(human.current_response(), response);
        assert!(matches!(body(&mut wire, 3, step.clone()), Body::State { receipt: Some(_), .. }));
        assert_eq!(raw.kaito_fixture_state(), numeric.fixture_state());
        assert_eq!(native::tapped(numeric.fixture_state(), actor) - native::tapped(&state, actor), 3);

        let mut departed = numeric.fixture_state().clone();
        let generation = departed.objects.get(host).zone_change_count;
        // Native zone-change and reentry operands isolate the original target
        // contract. The Moon itself was paid through its actual menu above.
        for destination in [Zone::Exile, Zone::Battlefield] {
            crate::event::propose_and_commit(
                &mut departed, crate::event::ProposedEvent::zone_change(host, destination),
            );
        }
        let view = public::capture(&departed, actor).unwrap();
        let target = view.casts[0].target.as_ref().unwrap();
        assert_eq!(target.original.generation, generation);
        assert!(target.current_original.is_none());
        assert!(!target.currently_legal);
        let saved: GameState = serde_json::from_slice(&serde_json::to_vec(&departed).unwrap()).unwrap();
        assert_eq!(public::capture(&saved, actor).unwrap(), view);
        native::finish(&mut departed);
        assert_eq!(departed.objects.get(source).zone, Zone::Graveyard);
        assert!(departed.objects.get(host).attachments.is_empty());

        numeric.restore(&save).unwrap();
        human.restore_v75(&human_save).unwrap();
        assert_eq!(decision(&numeric).packet(), d.packet());
        assert!(numeric.apply(&d, index).is_err());
        assert!(projector.submit(&mut human, human_action).is_err());
        assert!(matches!(body(&mut wire, 4, CommandV1::Restore { snapshot_id }), Body::State { .. }));
        assert!(matches!(body(&mut wire, 5, step), Body::Error { .. }));
    }
}
