use super::*;
use crate::{
    card_def::{card_id_by_name, KERNEL_CARDDB_HASH},
    current_mixed_policy_v1::KaitoDecision,
    current_tree_learner_v1::ModelV1,
    edict_receipts_v1::{self as receipts, JournalV1},
    effect::blasphemous_edict_v1_tests as native_fixture,
    engine::{self, CastMode, Decision},
    event::CommittedEvent,
    human_bo3_v1::{project_limited_kaito_v16, HumanActionRequestV1, HumanKaitoProjectorV75},
    ids::{ObjectId, PlayerId},
    limited_kaito_session_v16::{LimitedKaitoBodyV16 as Body, LimitedKaitoStateV16 as State},
    policy_observation_v6::tests::put,
    policy_surface_v5::{LimitedAppendixV1, PolicyDecisionV5},
    rl_session::{FastActorResponseV1, KaitoSessionV75},
    state::{CastMethodV4, GameState},
    surface_v2::SurfaceDecision,
};

fn config(actor: PlayerId) -> ResetV1 {
    let swamp = card_id_by_name("Swamp").unwrap();
    let mut mainboards = [vec![swamp; 40], vec![swamp; 40]];
    mainboards[actor.index()][..8].fill(PRINTING);
    ResetV1 {
        expected_card_db_hash: KERNEL_CARDDB_HASH,
        episode_id: 550,
        env_seed: 550_469,
        starting_player: actor.into(),
        deck_ids: ["edict-left".into(), "edict-right".into()],
        mainboards,
        max_physical_decisions: 2_000,
        max_policy_steps: 2_000,
    }
}

fn decision(session: &KaitoSession) -> KaitoDecision {
    let FastActorResponseV1::Decision(expected) = session.current_response() else {
        panic!("Edict decision absent: {:?}", session.current_response());
    };
    session.encode(expected).unwrap()
}

fn wire_reply(wire: &mut JsonlServerV1, serial: u64, command: CommandV1) -> ReplyV1 {
    wire.handle_request(RequestV1 {
        profile: PROFILE_V1.into(),
        request_id: serial.to_string(),
        command,
    })
}

fn body(wire: &mut JsonlServerV1, serial: u64, command: CommandV1) -> Body {
    let reply = wire_reply(wire, serial, command);
    let ReplyBodyV1::State { payload } = reply.body else {
        panic!("Edict wire request rejected: {reply:?}");
    };
    payload.body
}

fn raw_owner(state: &GameState) -> FastActorSessionV1 {
    FastActorSessionV1::from_kaito_v75_fixture_state_scoped(state.clone(), LimitedAppendixV1::Edict)
}

fn facts_at_native_decision(state: &GameState, viewer: PlayerId) -> FactsV1 {
    let mut cloned = state.clone();
    let native = engine::advance_until_decision(&mut cloned);
    assert_eq!(&cloned, state);
    public::capture(
        state,
        viewer,
        &PolicyDecisionV5::Surface(SurfaceDecision::Decision(native)),
    )
    .unwrap()
}

#[test]
fn declared_forty_card_v3_reset_pays_normal_and_resolves_zero_victims_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let config = config(actor);
        assert!(crate::limited_v68_v75_consumer_v1::reset_numeric_v1(&config).is_err());
        let mut bad = config.clone();
        bad.mainboards[actor.index()][0] = 468;
        assert!(reset_numeric_v1(&bad).is_err());
        let mut numeric = reset_numeric_reveal_facts_v3(&config).unwrap();
        assert!(numeric.fixture_state().reveal_knowledge_v4.is_some());

        let mut wire = JsonlServerV1::new();
        let Body::State { state, .. } = body(
            &mut wire,
            1,
            CommandV1::Reset {
                config: config.clone(),
            },
        ) else {
            panic!()
        };
        let State::Decision {
            decision: wire_opening,
            ..
        } = *state
        else {
            panic!()
        };
        assert_eq!(wire_opening.schema, "mtg-kernel-limited-edict-decision/v1");
        let expected = decision(&numeric).fixture_backend_v75().expected();
        let human = reset_human_v1(&config, expected.acting_player).unwrap();
        let prompt = HumanKaitoProjectorV75::new(expected.acting_player)
            .project_current(&human, expected)
            .unwrap();
        assert_eq!(prompt.schema, "mtg-kernel-human-edict-decision/v1");
        assert_eq!(prompt.actions, wire_opening.actions);
        let bad_profile = RequestV1 {
            profile: crate::lathril_consumer_v1::PROFILE_V1.into(),
            request_id: "wrong".into(),
            command: CommandV1::Current,
        };
        let rejected: serde_json::Value =
            serde_json::from_str(&wire.handle_line(&serde_json::to_string(&bad_profile).unwrap()))
                .unwrap();
        assert_eq!(rejected["kind"], "error");

        let mut spell = None;
        for _ in 0..450 {
            let d = decision(&numeric);
            let backend = d.fixture_backend_v75();
            let facts = backend.observation().edict.as_ref().unwrap();
            if let Some(row) = facts.resolved.first() {
                assert_eq!(row.active, actor.into());
                assert_eq!(Some(ObjectId(row.original_spell.object)), spell);
                assert_eq!(
                    row.payment,
                    Some(public::PaymentV1 {
                        method: CastMethodV4::Normal,
                        creatures_before: 0,
                        mana_spent: 5,
                    })
                );
                assert!(row.sacrifices_in_apnap_order.is_empty());
                assert!(row.actual_graveyard_order.iter().all(Vec::is_empty));
                let input = numeric.training_input_v1(&d).unwrap();
                assert!(ModelV1::deterministic_fixture()
                    .forward(&input)
                    .unwrap()
                    .iter()
                    .all(|value| value.is_finite()));
                break;
            }
            let candidates = &backend.candidates_v75().candidates;
            let cast = candidates.iter().position(|row| matches!(&row.action,
                Action::CastSpell { source, .. } if source.card_db_id == PRINTING && spell.is_none()
            ));
            let chosen = cast
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
                .expect("missing native normal-cost fixture choice");
            let before = native_fixture::taps(numeric.fixture_state(), actor);
            numeric.apply(&d, chosen as u32).unwrap();
            if let Action::CastSpell { source, .. } = &candidates[chosen].action {
                spell = Some(ObjectId(source.arena_id));
                assert_eq!(
                    native_fixture::taps(numeric.fixture_state(), actor) - before,
                    5
                );
            }
        }
        assert_eq!(
            receipts::outcomes(numeric.fixture_state()).unwrap().len(),
            1
        );
    }
}

fn quote_state(actor: PlayerId, hidden: &str) -> (GameState, ObjectId, [Vec<ObjectId>; 2]) {
    let mut state = native_fixture::ready(actor);
    let groups =
        [PlayerId::P0, PlayerId::P1].map(|seat| native_fixture::roster(&mut state, seat, 14));
    put(&mut state, actor.opponent(), hidden, Zone::Hand);
    let spell = put(&mut state, actor, public::NAME, Zone::Hand);
    assert!(matches!(engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { player, .. } if player == actor));
    engine::step(&mut state, engine::Action::CastSpell(spell)).unwrap();
    assert!(matches!(engine::advance_until_decision(&mut state),
        Decision::ChooseCastMode { player, spell: actual, options }
            if player == actor && actual == spell && options == [CastMode::Normal, CastMode::Alternative]));
    (state, spell, groups)
}

fn pay_alternate(state: &GameState, actor: PlayerId) -> GameState {
    let mut session = KaitoSession::from_session(raw_owner(state)).unwrap();
    let d = decision(&session);
    let backend = d.fixture_backend_v75();
    let facts = backend.observation().edict.as_ref().unwrap();
    let quotes = facts.chooser_cast_quotes.as_ref().unwrap();
    assert_eq!(
        quotes.modes,
        [
            public::QuoteV1 {
                mode: CastMode::Normal,
                payable_mana_now: 5
            },
            public::QuoteV1 {
                mode: CastMode::Alternative,
                payable_mana_now: 1
            },
        ]
    );
    let candidates = &backend.candidates_v75().candidates;
    assert_eq!(candidates.len(), 2);
    for (index, quote) in quotes.modes.iter().enumerate() {
        assert_eq!(
            backend.fixture_original_index_v75(index as u32),
            Some(index as u32)
        );
        assert_eq!(
            candidates[index].edict_action,
            Some(ActionV1::ChooseCastPayment {
                mode: quote.mode,
                payable_mana_now: quote.payable_mana_now,
            })
        );
    }
    assert!(facts_at_native_decision(state, actor.opponent())
        .chooser_cast_quotes
        .is_none());
    session.apply(&d, 1).unwrap();
    let mut paid = session.fixture_state().clone();
    assert_eq!(native_fixture::taps(&paid, actor), 1);
    assert_eq!(native_fixture::to_choice(&mut paid), actor);
    paid
}

#[test]
fn paid_alternate_apnap_hidden_pairs_original_indices_and_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (quote, spell, groups) = quote_state(actor, "Island");
        let (hidden_quote, _, _) = quote_state(actor, "Forest");
        assert_ne!(quote.state_hash(), hidden_quote.state_hash());
        let quote_numeric = KaitoSession::from_session(raw_owner(&quote)).unwrap();
        let hidden_quote_numeric = KaitoSession::from_session(raw_owner(&hidden_quote)).unwrap();
        assert_eq!(
            decision(&quote_numeric).packet(),
            decision(&hidden_quote_numeric).packet()
        );
        let paid = pay_alternate(&quote, actor);
        let hidden = pay_alternate(&hidden_quote, actor);
        let mut raw = raw_owner(&paid);
        let mut numeric = KaitoSession::from_session(raw_owner(&paid)).unwrap();
        let other = KaitoSession::from_session(raw_owner(&hidden)).unwrap();
        let mut human = HumanKaitoSessionV75::from_session(raw_owner(&paid), actor.into()).unwrap();
        let mut projector = HumanKaitoProjectorV75::new(actor.into());
        let mut wire = JsonlServerV1::new();
        wire.0
            .install_fixture_core(KaitoSessionV75::from_session(raw_owner(&paid)).unwrap());
        let mut hidden_wire = JsonlServerV1::new();
        hidden_wire
            .0
            .install_fixture_core(KaitoSessionV75::from_session(raw_owner(&hidden)).unwrap());

        let d = decision(&numeric);
        let save = numeric.snapshot();
        let mut other = other;
        assert_eq!(d.packet(), decision(&other).packet());
        assert!(other.training_input_v1(&d).is_err());
        assert!(other.restore(&save).is_err());
        let backend = d.fixture_backend_v75();
        let facts = backend.observation().edict.as_ref().unwrap();
        assert!(facts.casts[0].resolving);
        assert_eq!(
            facts.casts[0].payment,
            Some(public::PaymentV1 {
                method: CastMethodV4::Alternative,
                creatures_before: 28,
                mana_spent: 1,
            })
        );
        let choice = facts.chooser_sacrifice_or_order.as_ref().unwrap();
        assert_eq!(choice.chooser, actor.into());
        assert_eq!(choice.stage, public::ChoiceStageV1::SacrificeSelection);
        assert_eq!(
            choice
                .legal
                .iter()
                .map(|card| ObjectId(card.object))
                .collect::<Vec<_>>(),
            groups[actor.index()]
        );
        assert!(facts_at_native_decision(&paid, actor.opponent())
            .chooser_sacrifice_or_order
            .is_none());
        for (index, card) in groups[actor.index()].iter().enumerate() {
            assert_eq!(
                backend.fixture_original_index_v75(index as u32),
                Some(index as u32)
            );
            assert!(matches!(&backend.candidates_v75().candidates[index].action,
                Action::ChooseEffectTarget { target: TargetRefV1::Object { object }, .. }
                    if object.arena_id == card.0));
        }

        let expected = backend.expected();
        let prompt = projector.project_current(&human, expected).unwrap();
        let human_save = human.snapshot_v75();
        let current = body(&mut wire, 1, CommandV1::Current);
        assert_eq!(current, body(&mut hidden_wire, 1, CommandV1::Current));
        let Body::State { state, .. } = current else {
            panic!()
        };
        let State::Decision {
            episode_id,
            authority_generation,
            decision: wire_prompt,
        } = *state
        else {
            panic!()
        };
        let (projected, indices) =
            project_limited_kaito_v16(backend, expected.acting_player, wire_prompt.prompt_seq)
                .unwrap();
        assert_eq!(*wire_prompt, projected);
        assert_eq!(prompt.state, wire_prompt.state);
        assert_eq!(prompt.actions, wire_prompt.actions);
        assert_eq!(prompt.action_meanings, wire_prompt.action_meanings);
        let original = backend.fixture_original_index_v75(indices[0]).unwrap();
        let request = HumanActionRequestV1 {
            prompt_seq: prompt.prompt_seq,
            action_index: 0,
        };
        let wire_action = CommandV1::Step {
            episode_id,
            authority_generation,
            actor: actor.into(),
            prompt_seq: wire_prompt.prompt_seq,
            action_index: 0,
        };
        let Body::State {
            snapshot_id: Some(snapshot_id),
            ..
        } = body(&mut wire, 2, CommandV1::Snapshot)
        else {
            panic!()
        };
        let response = raw
            .step(expected.episode_id, expected.step, original)
            .unwrap();
        assert_eq!(numeric.apply(&d, indices[0]).unwrap(), response);
        projector.submit(&mut human, request.clone()).unwrap();
        assert_eq!(human.current_response(), response);
        assert!(matches!(
            body(&mut wire, 3, wire_action.clone()),
            Body::State {
                receipt: Some(_),
                ..
            }
        ));
        assert_eq!(raw.kaito_fixture_state(), numeric.fixture_state());

        let continued = decision(&numeric);
        let facts = continued
            .fixture_backend_v75()
            .observation()
            .edict
            .as_ref()
            .unwrap();
        let first = &facts.chooser_sacrifice_or_order.as_ref().unwrap().selected[0];
        assert_eq!(first.object, choice.legal[original as usize].object);
        assert!(
            facts_at_native_decision(numeric.fixture_state(), actor.opponent())
                .chooser_sacrifice_or_order
                .is_none()
        );
        for _ in 1..13 {
            let current = decision(&numeric);
            let backend = current.fixture_backend_v75();
            let menu = backend
                .observation()
                .edict
                .as_ref()
                .unwrap()
                .chooser_sacrifice_or_order
                .as_ref()
                .unwrap();
            assert_eq!(menu.chooser, actor.into());
            numeric.apply(&current, 0).unwrap();
        }
        let handed_off = decision(&numeric);
        let backend = handed_off.fixture_backend_v75();
        let menu = backend
            .observation()
            .edict
            .as_ref()
            .unwrap()
            .chooser_sacrifice_or_order
            .as_ref()
            .unwrap();
        assert_eq!(menu.chooser, actor.opponent().into());
        assert!(menu.selected.is_empty());
        assert_eq!(menu.completed_controller_choices[actor.index()].len(), 13);
        assert!(menu.completed_controller_choices[actor.opponent().index()].is_empty());
        let mut resolved = numeric.fixture_state().clone();
        native_fixture::finish(&mut resolved);
        let actual = facts_at_native_decision(&resolved, actor.opponent());
        let outcome = &actual.resolved[0];
        assert_eq!(outcome.original_spell.object, spell.0);
        assert_eq!(outcome.sacrifices_in_apnap_order.len(), 26);
        assert!(outcome.sacrifices_in_apnap_order[..13]
            .iter()
            .all(|entry| entry.original.controller == actor.into()));
        for (seat, ordered) in outcome.actual_graveyard_order.iter().enumerate() {
            assert_eq!(ordered.len(), 13);
            let actual_order = resolved.players[seat]
                .graveyard
                .iter()
                .copied()
                .filter(|object| *object != spell)
                .collect::<Vec<_>>();
            assert_eq!(
                ordered
                    .iter()
                    .map(|entry| ObjectId(entry.object))
                    .collect::<Vec<_>>(),
                actual_order
            );
        }
        let restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&resolved).unwrap()).unwrap();
        assert_eq!(
            facts_at_native_decision(&restored, actor.opponent()),
            actual
        );

        numeric.restore(&save).unwrap();
        human.restore_v75(&human_save).unwrap();
        assert_eq!(decision(&numeric).packet(), d.packet());
        assert!(numeric.apply(&d, indices[0]).is_err());
        assert!(projector.submit(&mut human, request).is_err());
        body(&mut wire, 4, CommandV1::Restore { snapshot_id });
        assert!(matches!(
            body(&mut wire, 5, wire_action),
            Body::Error { .. }
        ));
    }
}

#[test]
fn physical_owner_order_uses_original_native_targets_for_stolen_cards() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = native_fixture::ready(actor);
        let owned = native_fixture::roster(&mut state, actor, 12);
        native_fixture::roster(&mut state, actor.opponent(), 2);
        let stolen = owned[0];
        crate::standard_cards_v1::gain_control(&mut state, stolen, actor.opponent());
        let spell = put(&mut state, actor, public::NAME, Zone::Hand);
        assert!(matches!(engine::advance_until_decision(&mut state),
            Decision::CastSpellOrPass { player, .. } if player == actor));
        engine::step(&mut state, engine::Action::CastSpell(spell)).unwrap();
        assert!(matches!(
            engine::advance_until_decision(&mut state),
            Decision::ChooseCastMode { .. }
        ));
        engine::step(
            &mut state,
            engine::Action::ChooseCastMode(CastMode::Alternative),
        )
        .unwrap();
        for _ in 0..2 {
            assert!(matches!(
                engine::advance_until_decision(&mut state),
                Decision::CastSpellOrPass { .. }
            ));
            engine::step(&mut state, engine::Action::Pass).unwrap();
        }
        assert!(matches!(engine::advance_until_decision(&mut state),
            Decision::ChooseEffectTargets { player, min_targets: 12, .. } if player == actor));
        let mut numeric = KaitoSession::from_session(raw_owner(&state)).unwrap();
        let d = decision(&numeric);
        let backend = d.fixture_backend_v75();
        let menu = backend
            .observation()
            .edict
            .as_ref()
            .unwrap()
            .chooser_sacrifice_or_order
            .as_ref()
            .unwrap();
        assert_eq!(
            menu.stage,
            public::ChoiceStageV1::PhysicalOwnerGraveyardOrder
        );
        assert_eq!(menu.completed_controller_choices[actor.index()].len(), 11);
        assert_eq!(
            menu.completed_controller_choices[actor.opponent().index()].len(),
            3
        );
        let index = menu
            .legal
            .iter()
            .position(|card| card.object == stolen.0)
            .unwrap() as u32;
        let card = &menu.legal[index as usize];
        assert_eq!(
            (card.owner, card.controller),
            (actor.into(), actor.opponent().into())
        );
        assert_eq!(backend.fixture_original_index_v75(index), Some(index));
        numeric.apply(&d, index).unwrap();
        let mut finished = numeric.fixture_state().clone();
        native_fixture::finish(&mut finished);
        let facts = facts_at_native_decision(&finished, actor.opponent());
        let outcome = &facts.resolved[0];
        assert_eq!(
            outcome.actual_graveyard_order[actor.index()][0].object,
            stolen.0
        );
    }
}

#[test]
fn actual_sacrifice_and_payment_reject_edited_auxiliary_receipts() {
    let actor = PlayerId::P0;
    let (quote, _, _) = quote_state(actor, "Forest");
    let mut finished = pay_alternate(&quote, actor);
    native_fixture::finish(&mut finished);
    let mut no_payment = finished.clone();
    no_payment.engine.event_history.retain(|event| {
        !matches!(event,
        CommittedEvent::BlasphemousEdictReceiptV1 { journal }
            if matches!(journal.as_ref(), JournalV1::Paid(_) | JournalV1::ManaCommitted { .. }))
    });
    assert!(public::validate_native(&no_payment).is_err());

    let mut changed_payment = finished.clone();
    for event in changed_payment.engine.event_history.iter_mut() {
        if let CommittedEvent::BlasphemousEdictReceiptV1 { journal } = event {
            if let JournalV1::Paid(paid) = journal.as_mut() {
                paid.mana_spent = 0;
                paid.creatures_before = 13;
            }
        }
    }
    assert!(public::validate_native(&changed_payment).is_err());

    let mut changed_outcome = finished.clone();
    let events = &mut changed_outcome.engine.event_history;
    let begin = events
        .iter()
        .position(|event| {
            matches!(event,
        CommittedEvent::BlasphemousEdictReceiptV1 { journal }
            if matches!(journal.as_ref(), JournalV1::BatchBegin { .. }))
        })
        .unwrap();
    let end = events
        .iter()
        .position(|event| {
            matches!(event,
        CommittedEvent::BlasphemousEdictReceiptV1 { journal }
            if matches!(journal.as_ref(), JournalV1::BatchEnded { .. }))
        })
        .unwrap();
    events.swap(begin, end - 1);
    let CommittedEvent::BlasphemousEdictReceiptV1 { journal } = &mut events[end] else {
        panic!()
    };
    let JournalV1::BatchEnded {
        sacrifices, moves, ..
    } = journal.as_mut()
    else {
        panic!()
    };
    (*sacrifices, *moves) = (0, 0);
    assert!(public::validate_native(&changed_outcome).is_err());
}
