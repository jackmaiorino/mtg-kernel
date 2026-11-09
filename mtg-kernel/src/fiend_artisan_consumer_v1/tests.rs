use super::*;
use crate::{
    card_def::{card_id_by_name, KERNEL_CARDDB_HASH},
    current_mixed_policy_v1::KaitoDecision,
    current_tree_learner_v1::ModelV1,
    engine::{self, fiend_artisan_v1_tests as native},
    event::{self, CommittedEvent as E, ProposedEvent},
    fiend_artisan_receipts_v1::{self as receipts, JournalV1},
    human_bo3_v1::{project_limited_kaito_v16, HumanActionRequestV1, HumanKaitoProjectorV75},
    ids::{ObjectId, PlayerId},
    limited_kaito_session_v16::{LimitedKaitoBodyV16 as Body, LimitedKaitoStateV16 as State},
    policy_observation_v6::tests::put,
    policy_surface_v5::{LimitedAppendixV1, PolicyDecisionV5},
    rl_session::{FastActorResponseV1, KaitoSessionV75},
    state::{FaceDownV1, GameState, Target},
    surface_v2::SurfaceDecision,
};

fn id(name: &str) -> u16 {
    card_id_by_name(name).unwrap()
}

fn config(actor: PlayerId) -> ResetV1 {
    let mut mainboards = [vec![id("Swamp"); 40], vec![id("Forest"); 40]];
    mainboards[actor.index()][..8].fill(PRINTING);
    ResetV1 {
        expected_card_db_hash: KERNEL_CARDDB_HASH,
        episode_id: 550,
        env_seed: 550_483,
        starting_player: actor.into(),
        deck_ids: ["fiend-left".into(), "fiend-right".into()],
        mainboards,
        max_physical_decisions: 2_000,
        max_policy_steps: 2_000,
    }
}

fn decision(session: &KaitoSession) -> KaitoDecision {
    let FastActorResponseV1::Decision(expected) = session.current_response() else {
        panic!(
            "Fiend Artisan decision absent: {:?}",
            session.current_response()
        );
    };
    session.encode(expected).unwrap()
}

fn owner(state: &GameState) -> FastActorSessionV1 {
    FastActorSessionV1::from_kaito_v75_fixture_state_scoped(
        state.clone(),
        LimitedAppendixV1::FiendArtisan,
    )
}

fn wire_reply(wire: &mut JsonlServerV1, serial: u32, command: CommandV1) -> Body {
    let reply = wire.handle_request(RequestV1 {
        profile: PROFILE_V1.into(),
        request_id: serial.to_string(),
        command,
    });
    let ReplyBodyV1::State { payload } = reply.body else {
        panic!("{reply:?}")
    };
    payload.body
}

fn facts(state: &GameState, viewer: PlayerId, decision: engine::Decision) -> FactsV1 {
    public::capture(
        state,
        viewer,
        &PolicyDecisionV5::Surface(SurfaceDecision::Decision(decision)),
    )
    .unwrap()
}

#[test]
fn declared_forty_card_v3_reset_pays_exact_hybrid_cast_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let config = config(actor);
        assert!(crate::limited_v68_v75_consumer_v1::reset_numeric_v1(&config).is_err());
        let mut bad = config.clone();
        bad.mainboards[actor.index()][0] = 472;
        assert!(reset_numeric_v1(&bad).is_err());

        let mut numeric = reset_numeric_reveal_facts_v3(&config).unwrap();
        assert!(numeric.fixture_state().fiend_artisan_receipts_v1);
        assert!(numeric.fixture_state().reveal_knowledge_v4.is_some());
        let mut wire = JsonlServerV1::new();
        let Body::State { state, .. } = wire_reply(
            &mut wire,
            1,
            CommandV1::Reset {
                config: config.clone(),
            },
        ) else {
            panic!()
        };
        let State::Decision {
            decision: opening, ..
        } = *state
        else {
            panic!()
        };
        assert_eq!(
            opening.schema,
            "mtg-kernel-limited-fiend-artisan-decision/v1"
        );
        let expected = decision(&numeric).fixture_backend_v75().expected();
        let human = reset_human_v1(&config, expected.acting_player).unwrap();
        let prompt = HumanKaitoProjectorV75::new(expected.acting_player)
            .project_current(&human, expected)
            .unwrap();
        assert_eq!(prompt.schema, "mtg-kernel-human-fiend-artisan-decision/v1");
        assert_eq!(prompt.actions, opening.actions);

        let mut cast = None;
        let mut entered = false;
        for _ in 0..360 {
            let d = decision(&numeric);
            let backend = d.fixture_backend_v75();
            let facts = backend.observation().fiend_artisan.as_ref().unwrap();
            if let Some(row) = facts.battlefield.first() {
                assert_eq!(Some(row.current.card.object), cast);
                assert!(row.current.summoning_sick);
                assert_eq!(row.graveyard_creature_bonus, 0);
                assert!(ModelV1::deterministic_fixture()
                    .forward(&numeric.training_input_v1(&d).unwrap())
                    .unwrap()
                    .iter()
                    .all(|value| value.is_finite()));
                entered = true;
                break;
            }
            let candidates = &backend.candidates_v75().candidates;
            let chosen = candidates
                .iter()
                .position(|row| row.fiend_artisan_action == Some(ActionV1::Cast) && cast.is_none())
                .or_else(|| {
                    candidates
                        .iter()
                        .position(|row| matches!(row.action, Action::PlayLand { .. }))
                })
                .or_else(|| {
                    candidates.iter().position(|row| {
                        matches!(
                            row.action,
                            Action::ChooseLondonMulligan {
                                mulligan: false,
                                ..
                            } | Action::Pass { .. }
                                | Action::Discard { .. }
                                | Action::ChooseAttackerInclusion { include: false, .. }
                                | Action::ChooseBlockerInclusion { include: false, .. }
                        )
                    })
                })
                .expect("missing native reset action");
            let before = native::tapped(numeric.fixture_state(), actor);
            numeric.apply(&d, chosen as u32).unwrap();
            if let Action::CastSpell { source, .. } = &candidates[chosen].action {
                cast = Some(source.arena_id);
                assert_eq!(native::tapped(numeric.fixture_state(), actor) - before, 2);
            }
        }
        assert!(
            entered,
            "declared reset did not genuinely cast Fiend Artisan"
        );
    }
}

fn ready(
    actor: PlayerId,
    hidden_hand: &str,
    private_second: &str,
) -> (GameState, ObjectId, ObjectId, ObjectId) {
    let mut state = native::ready(actor, "Forest");
    state.fiend_artisan_receipts_v1 = true;
    let source = native::paid(&mut state, actor);
    let sacrifice = native::cast(
        &mut state,
        actor,
        "Llanowar Elves",
        (false, 0, false),
        &[],
        None,
        1,
    );
    native::finish(&mut state);
    native::mature(&mut state, actor, source);
    let second_payer = native::cast(
        &mut state,
        actor,
        "Llanowar Elves",
        (false, 0, false),
        &[],
        None,
        1,
    );
    native::finish(&mut state);
    assert_ne!(second_payer, sacrifice);
    put(&mut state, actor, private_second, Zone::Library);
    let find = put(&mut state, actor, "Axgard Cavalry", Zone::Library);
    put(&mut state, actor, "Meteor Golem", Zone::Library);
    put(&mut state, actor.opponent(), hidden_hand, Zone::Hand);
    native::priority(&mut state, actor);
    (state, source, sacrifice, find)
}

fn paid(state: &GameState, actor: PlayerId, source: ObjectId, sacrifice: ObjectId) -> GameState {
    let mut session = KaitoSession::from_session(owner(state)).unwrap();
    let initial = decision(&session);
    let index = initial
        .fixture_backend_v75()
        .candidates_v75()
        .candidates
        .iter()
        .position(|row| row.fiend_artisan_action == Some(ActionV1::Activate))
        .unwrap();
    session.apply(&initial, index as u32).unwrap();
    let x = decision(&session);
    let backend = x.fixture_backend_v75();
    assert!(
        matches!(&backend.observation().fiend_artisan.as_ref().unwrap().chooser,
        Some(ChoiceV1::PayX { maximum, .. }) if *maximum >= 2)
    );
    assert_eq!(backend.fixture_original_index_v75(2), Some(2));
    assert_eq!(
        backend.candidates_v75().candidates[2].fiend_artisan_action,
        Some(ActionV1::PayX { x: 2 })
    );
    session.training_input_v1(&x).unwrap();
    session.apply(&x, 2).unwrap();

    let selection = decision(&session);
    let backend = selection.fixture_backend_v75();
    let index = backend.candidates_v75().candidates.iter().position(|row| matches!(
        &row.action, Action::ChooseCostTarget { candidate, .. } if candidate.arena_id == sacrifice.0
    )).unwrap();
    assert_eq!(
        backend.candidates_v75().candidates[index].fiend_artisan_action,
        Some(ActionV1::Sacrifice)
    );
    assert_eq!(
        backend.fixture_original_index_v75(index as u32),
        Some(index as u32)
    );
    let before = native::tapped(session.fixture_state(), actor);
    session.apply(&selection, index as u32).unwrap();
    let paid = session.fixture_state();
    assert_eq!(native::tapped(paid, actor) - before, 4);
    assert_eq!(paid.objects.get(sacrifice).zone, Zone::Graveyard);
    assert_eq!(engine::checked_effective_pt(paid, source).unwrap(), (2, 2));
    let activation = receipts::activations(paid).unwrap().pop().unwrap();
    assert_eq!(activation.x, 2);
    assert_eq!(activation.sacrifice.object, sacrifice.0);
    paid.clone()
}

fn search(
    mut state: GameState,
    actor: PlayerId,
    source: ObjectId,
) -> (GameState, engine::Decision) {
    native::cast(
        &mut state,
        actor.opponent(),
        "Lightning Bolt",
        (false, 0, false),
        &[Target::Object(source)],
        None,
        1,
    );
    let d = native::search(&mut state, actor);
    assert_eq!(state.objects.get(source).zone, Zone::Graveyard);
    (state, d)
}

#[test]
fn paid_native_fiend_token_copy_uses_same_graveyard_bonus_and_form_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = native::ready(actor, "Forest");
        state.fiend_artisan_receipts_v1 = true;
        let source = native::paid(&mut state, actor);
        let sacrificed = native::cast(
            &mut state,
            actor,
            "Llanowar Elves",
            (false, 0, false),
            &[],
            None,
            1,
        );
        native::finish(&mut state);
        native::mature(&mut state, actor, source);
        let payer = native::cast(
            &mut state,
            actor,
            "Llanowar Elves",
            (false, 0, false),
            &[],
            None,
            1,
        );
        native::finish(&mut state);
        state = paid(&state, actor, source, sacrificed);
        let _search = native::search(&mut state, actor);
        native::answer(&mut state, None);

        native::cast(
            &mut state,
            actor,
            "Self-Reflection",
            (false, 0, false),
            &[Target::Object(source)],
            None,
            6,
        );
        native::finish(&mut state);
        native::priority(&mut state, actor);
        let session = KaitoSession::from_session(owner(&state)).unwrap();
        let d = decision(&session);
        let facts = d
            .fixture_backend_v75()
            .observation()
            .fiend_artisan
            .as_ref()
            .unwrap();
        assert_eq!(facts.battlefield.len(), 2);
        assert_eq!(
            facts
                .battlefield
                .iter()
                .filter(|row| row.current.is_token)
                .count(),
            1
        );
        for row in &facts.battlefield {
            assert_eq!(row.graveyard_creature_bonus, 1);
            assert_eq!(
                (row.current.form.power, row.current.form.toughness),
                (Some(2), Some(2))
            );
            assert!(row.current.form.printed_abilities_active);
        }
        assert_eq!(state.objects.get(payer).zone, Zone::Battlefield);
        assert!(session.training_input_v1(&d).is_ok());
        assert!(crate::fiend_artisan_receipts_v1::refuse_legacy(&state).is_err());
    }
}

#[test]
fn paid_concealed_sacrifice_retains_original_public_face_on_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, source, _, _) = ready(actor, "Island", "Forest");
        // Support operand for the actual payable face-down creature. The
        // activation and sacrifice below use the real owning native actions.
        let concealed = put(&mut state, actor, "Meteor Golem", Zone::Battlefield);
        state.objects.get_mut(concealed).v4.face_down_v1 = Some(FaceDownV1 {
            disguised: true,
            lookers: 1 << actor.index(),
            hidden_by: None,
            hidden_by_source: None,
        });
        let mut paid: GameState = serde_json::from_slice(
            &serde_json::to_vec(&paid(&state, actor, source, concealed)).unwrap(),
        )
        .unwrap();
        let original = receipts::activations(&paid).unwrap()[0].sacrifice;
        assert_eq!(original.identity, IdentityV1::FaceDown { disguised: true });
        let mut edited = false;
        for event in paid.engine.event_history.iter_mut() {
            if let E::FiendArtisanConsumerReceiptV1 { journal } = event {
                if let JournalV1::Sacrificed {
                    original,
                    source,
                    face_down,
                    ..
                } = journal.as_mut()
                {
                    *face_down = None;
                    original.identity = IdentityV1::Known {
                        card_id: source.card_def,
                    };
                    edited = true;
                }
            }
        }
        assert!(edited);
        assert!(receipts::activations(&paid).is_err());
    }
}

#[test]
fn real_paid_x_sacrifice_search_private_worlds_index_and_source_departure_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (state, source, sacrifice, find) = ready(actor, "Island", "Llanowar Elves");
        let (pair, _, _, _) = ready(actor, "Mountain", "Llanowar Elves");
        let (private, _, _, _) = ready(actor, "Island", "Forest");
        let paid_state = paid(&state, actor, source, sacrifice);
        let mut countered = paid_state.clone();
        let stack_id = receipts::activations(&countered).unwrap()[0]
            .item
            .v4
            .stack_item_id;
        engine::counter_stack_item_by_id(&mut countered, stack_id).unwrap();
        let countered_row = receipts::activations(&countered).unwrap().pop().unwrap();
        assert!(
            countered_row.countered && !countered_row.shuffled && countered_row.arrival.is_none()
        );
        assert_eq!(countered.objects.get(sacrifice).zone, Zone::Graveyard);

        let (state, actual) = search(paid_state, actor, source);
        let (pair, _) = search(paid(&pair, actor, source, sacrifice), actor, source);
        let (private, private_actual) =
            search(paid(&private, actor, source, sacrifice), actor, source);
        assert_ne!(state.state_hash(), pair.state_hash());
        let other_view = facts(&state, actor.opponent(), actual.clone());
        assert!(other_view.chooser.is_none());
        assert_eq!(
            other_view,
            facts(&private, actor.opponent(), private_actual)
        );
        assert!(crate::fiend_artisan_receipts_v1::refuse_legacy(&state).is_err());

        let mut raw = owner(&state);
        let mut numeric = KaitoSession::from_session(owner(&state)).unwrap();
        let mut paired = KaitoSession::from_session(owner(&pair)).unwrap();
        let d = decision(&numeric);
        let snapshot = numeric.snapshot();
        assert_eq!(d.packet(), decision(&paired).packet());
        assert!(paired.restore(&snapshot).is_err());
        assert!(paired.training_input_v1(&d).is_err());
        assert!(numeric.training_input_v1(&d).is_ok());
        let backend = d.fixture_backend_v75();
        let facts = backend.observation().fiend_artisan.as_ref().unwrap();
        assert!(facts.activations[0]
            .source
            .current_original_source
            .is_none());
        let Some(ChoiceV1::Search {
            legal, paid_x: 2, ..
        }) = &facts.chooser
        else {
            panic!()
        };
        assert_eq!(legal.len(), 2);
        let engine::Decision::ChooseEffectTargets { legal_targets, .. } = actual else {
            panic!()
        };
        let original = legal_targets
            .iter()
            .position(|target| *target == Target::Object(find))
            .unwrap();
        let expected = backend.expected();
        let mut human = HumanKaitoSessionV75::from_session(owner(&state), actor.into()).unwrap();
        let mut projector = HumanKaitoProjectorV75::new(actor.into());
        let prompt = projector.project_current(&human, expected).unwrap();
        let human_snapshot = human.snapshot_v75();
        let mut wire = JsonlServerV1::new();
        wire.0
            .install_fixture_core(KaitoSessionV75::from_session(owner(&state)).unwrap());
        let Body::State { state: current, .. } = wire_reply(&mut wire, 1, CommandV1::Current)
        else {
            panic!()
        };
        let State::Decision {
            episode_id,
            authority_generation,
            decision: wire_prompt,
        } = *current
        else {
            panic!()
        };
        let (projected, indices) =
            project_limited_kaito_v16(backend, actor.into(), wire_prompt.prompt_seq).unwrap();
        assert_eq!(*wire_prompt, projected);
        assert_eq!(prompt.state, wire_prompt.state);
        assert_eq!(prompt.actions, wire_prompt.actions);
        assert_eq!(prompt.action_meanings, wire_prompt.action_meanings);
        let ui = indices
            .iter()
            .position(|&index| backend.fixture_original_index_v75(index) == Some(original as u32))
            .unwrap();
        let request = HumanActionRequestV1 {
            prompt_seq: prompt.prompt_seq,
            action_index: ui as u32,
        };
        let step = CommandV1::Step {
            episode_id,
            authority_generation,
            actor: actor.into(),
            prompt_seq: wire_prompt.prompt_seq,
            action_index: ui as u32,
        };
        let Body::State {
            snapshot_id: Some(snapshot_id),
            ..
        } = wire_reply(&mut wire, 2, CommandV1::Snapshot)
        else {
            panic!()
        };
        let response = raw
            .step(expected.episode_id, expected.step, original as u32)
            .unwrap();
        assert_eq!(numeric.apply(&d, indices[ui]).unwrap(), response);
        projector.submit(&mut human, request.clone()).unwrap();
        assert_eq!(human.current_response(), response);
        assert!(matches!(
            wire_reply(&mut wire, 3, step.clone()),
            Body::State {
                receipt: Some(_),
                ..
            }
        ));

        let mut declined = state.clone();
        native::answer(&mut declined, None);
        let row = receipts::activations(&declined).unwrap().pop().unwrap();
        assert!(row.shuffled && row.arrival.is_none());
        assert_eq!(declined.objects.get(find).zone, Zone::Library);
        let mut found = state.clone();
        native::answer(&mut found, Some(find));
        let row = receipts::activations(&found).unwrap().pop().unwrap();
        assert!(row.shuffled && !row.countered);
        assert_eq!(row.arrival.unwrap().object, find.0);
        assert_eq!(row.arrival.unwrap().zone, Zone::Battlefield);
        // Historical birth identity remains exact after a later private
        // incarnation. It must agree with the committed visibility witness.
        event::propose_and_commit(&mut found, ProposedEvent::zone_change(find, Zone::Hand));
        let mut restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&found).unwrap()).unwrap();
        assert_eq!(restored.state_hash(), found.state_hash());
        let historical = receipts::activations(&restored).unwrap().pop().unwrap();
        assert_eq!(
            historical.arrival.unwrap().identity,
            IdentityV1::Known {
                card_id: id("Axgard Cavalry")
            }
        );
        let mut wrong_name = restored.clone();
        let mut edited = false;
        for event in wrong_name.engine.event_history.iter_mut() {
            if let E::FiendArtisanConsumerReceiptV1 { journal } = event {
                if let JournalV1::Moved {
                    arrival: Some(card),
                    ..
                } = journal.as_mut()
                {
                    card.identity = IdentityV1::Known {
                        card_id: id("Llanowar Elves"),
                    };
                    edited = true;
                }
            }
        }
        assert!(edited);
        assert!(receipts::activations(&wrong_name).is_err());
        restored.engine.event_history.retain(|event| !matches!(event,
            E::FiendArtisanConsumerReceiptV1 { journal } if matches!(journal.as_ref(), JournalV1::Moved { .. })));
        assert!(receipts::activations(&restored).is_err());

        numeric.restore(&snapshot).unwrap();
        human.restore_v75(&human_snapshot).unwrap();
        assert_eq!(decision(&numeric).packet(), d.packet());
        assert!(numeric.apply(&d, indices[ui]).is_err());
        assert!(projector.submit(&mut human, request).is_err());
        wire_reply(&mut wire, 4, CommandV1::Restore { snapshot_id });
        assert!(matches!(wire_reply(&mut wire, 5, step), Body::Error { .. }));
    }
}
