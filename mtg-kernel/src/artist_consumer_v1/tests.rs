use super::*;
use crate::{
    current_mixed_policy_v1::KaitoDecision,
    current_tree_learner_v1::{AdamConfigV1, LearnerV1, ModelV1, TrainingExampleV1},
    effect::perforating_artist_v1::tests as native_paid,
    human_bo3_v1::{project_limited_kaito_v16, HumanActionRequestV1, HumanKaitoProjectorV75},
    ids::ObjectId,
    limited_kaito_session_v16::{LimitedKaitoBodyV16 as Body, LimitedKaitoStateV16 as State},
    policy_observation_v6::tests::put,
    rl_session::{FastActorResponseV1, KaitoSessionV75},
};

fn card(name: &str) -> u16 {
    crate::card_def::card_id_by_name(name).unwrap()
}

fn config(actor: PlayerId) -> ResetV1 {
    let mut deck = vec![PRINTING_V1; 4];
    deck.extend([card("Swamp"); 18]);
    deck.extend([card("Mountain"); 18]);
    let mut mainboards = [vec![card("Forest"); 40], vec![card("Forest"); 40]];
    mainboards[actor.index()] = deck;
    ResetV1 {
        expected_card_db_hash: KERNEL_CARDDB_HASH,
        episode_id: 550,
        env_seed: 550_460,
        starting_player: actor.into(),
        deck_ids: ["artist-left".into(), "artist-right".into()],
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
        panic!("Artist decision absent: {:?}", session.current_response());
    };
    session.encode(expected).unwrap()
}

fn wire_body(wire: &mut JsonlServerV1, serial: impl ToString, command: CommandV1) -> Body {
    let reply = wire.handle_request(req(serial, command));
    assert_eq!(reply.profile, PROFILE_V1);
    let ReplyBodyV1::State { payload } = reply.body else {
        panic!("profile request rejected: {reply:?}");
    };
    payload.body
}

#[test]
fn exact_forty_card_v3_reset_reaches_genuine_paid_artist_attack_and_opponent_discard_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let config = config(actor);
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
        for printing in [457, 461, 486, 492, 495, u16::MAX] {
            let mut bad = config.clone();
            bad.mainboards[actor.index()][0] = printing;
            assert!(reset_numeric_v1(&bad).is_err());
        }
        let mut wire = JsonlServerV1::new();
        let request = req(
            "reset",
            CommandV1::Reset {
                config: config.clone(),
            },
        );
        let line = serde_json::to_string(&request).unwrap();
        let result: serde_json::Value = serde_json::from_str(&wire.handle_line(&line)).unwrap();
        assert_eq!(result["profile"], PROFILE_V1);
        assert_eq!(
            result["payload"]["state"]["decision"]["schema"],
            "mtg-kernel-limited-artist-decision/v1"
        );
        let mut owner = reset_numeric_reveal_facts_v3(&config).unwrap();
        assert!(owner.fixture_state().reveal_knowledge_v4.is_some());
        let opening = decision(&owner);
        let expected = opening.fixture_backend_v75().expected();
        let human = reset_human_v1(&config, expected.acting_player).unwrap();
        let view = HumanKaitoProjectorV75::new(expected.acting_player)
            .project_current(&human, expected)
            .unwrap();
        assert_eq!(view.schema, "mtg-kernel-human-artist-decision/v1");
        assert_eq!(
            opening.fixture_backend_v75().observation().schema,
            OBSERVATION_SCHEMA
        );

        let mut paid = None;
        let mut completed = false;
        for _ in 0..450 {
            let current = decision(&owner);
            let backend = current.fixture_backend_v75();
            let actions = backend.actions();
            if let Some(i) = backend.candidates_v75().candidates.iter().position(|row| {
                row.artist_payment.is_some_and(|a| {
                    a.kind == PaymentKindV64::DiscardCard && a.answer == AnswerV1::SelectPayment
                })
            }) {
                assert_eq!(backend.expected().acting_player, actor.opponent().into());
                let source = paid.expect("Raid must follow a real paid Artist cast");
                assert_eq!(
                    owner.fixture_state().objects.get(source).zone,
                    Zone::Battlefield
                );
                assert!(owner.fixture_state().player_attacked_this_turn_v1(actor));
                let input = owner.training_input_v1(&current).unwrap();
                assert!(ModelV1::deterministic_fixture()
                    .forward(&input)
                    .unwrap()
                    .iter()
                    .all(|v| v.is_finite()));
                let Action::ChooseEffectTarget {
                    target: TargetRefV1::Object { object },
                    ..
                } = &actions[i]
                else {
                    panic!("discard lost original object action")
                };
                let discarded = ObjectId(object.arena_id);
                owner.apply(&current, i as u32).unwrap();
                assert_eq!(
                    owner.fixture_state().objects.get(discarded).zone,
                    Zone::Graveyard
                );
                completed = true;
                break;
            }
            let choose = actions.iter().position(|action| matches!(action,
                Action::CastSpell { source, .. } if source.card_db_id == PRINTING_V1 && paid.is_none()
            )).or_else(|| actions.iter().position(|action| matches!(action,
                Action::PlayLand { .. } | Action::ChooseAttackerInclusion { include: true, .. }
            ))).or_else(|| actions.iter().position(|action| matches!(action,
                Action::ChooseLondonMulligan { mulligan: false, .. }
                | Action::ChooseBlockerInclusion { include: false, .. }
                | Action::Pass { .. } | Action::Discard { .. }
                | Action::FinishEffectSelection { .. }
                | Action::ChooseTriggerOrderNext { .. } | Action::OrderTriggers { .. }
            ))).unwrap_or_else(|| panic!("unhandled Artist reset menu: {actions:?}"));
            let casting = match &actions[choose] {
                Action::CastSpell { source, .. } => Some(ObjectId(source.arena_id)),
                _ => None,
            };
            let tapped = |state: &GameState| {
                state.players[actor.index()]
                    .battlefield
                    .iter()
                    .filter(|&&id| state.objects.get(id).tapped)
                    .count()
            };
            let before = tapped(owner.fixture_state());
            owner.apply(&current, choose as u32).unwrap();
            if let Some(source) = casting {
                let state = owner.fixture_state();
                assert_eq!(state.objects.get(source).zone, Zone::Stack);
                assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
                assert_eq!(tapped(state) - before, 3);
                paid = Some(source);
            }
        }
        assert!(
            completed,
            "declared Artist never completed its genuine discard menu"
        );
    }
}

struct Owners {
    raw: FastActorSessionV1,
    numeric: KaitoSession,
    human: HumanKaitoSessionV75,
    projector: HumanKaitoProjectorV75,
    wire: JsonlServerV1,
    serial: u64,
    seat: PlayerSeatV1,
}

impl Owners {
    fn fixture(state: GameState, seat: PlayerSeatV1) -> Self {
        let make = || FastActorSessionV1::from_kaito_v75_fixture_state_scoped(state.clone(), true);
        let mut wire = JsonlServerV1::new();
        wire.0
            .install_fixture_core(KaitoSessionV75::from_session(make()).unwrap());
        Self {
            raw: make(),
            numeric: KaitoSession::from_session(make()).unwrap(),
            human: HumanKaitoSessionV75::from_session(make(), seat).unwrap(),
            projector: HumanKaitoProjectorV75::new(seat),
            wire,
            serial: 0,
            seat,
        }
    }

    fn request(&mut self, command: CommandV1) -> Body {
        self.serial += 1;
        wire_body(&mut self.wire, self.serial, command)
    }

    fn apply(&mut self, d: &KaitoDecision, i: usize) -> CommandV1 {
        let backend = d.fixture_backend_v75();
        let expected = backend.expected();
        let Body::State { state, .. } = self.request(CommandV1::Current) else {
            panic!()
        };
        let State::Decision {
            episode_id,
            authority_generation,
            decision: visible,
        } = *state
        else {
            panic!("wire lacks Artist menu: {state:?}")
        };
        let (projected, indices) =
            project_limited_kaito_v16(backend, expected.acting_player, visible.prompt_seq).unwrap();
        assert_eq!(*visible, projected);
        let i = i as u32;
        let index = indices.iter().position(|&index| index == i).unwrap() as u32;
        if expected.acting_player == self.seat {
            let prompt = self
                .projector
                .project_current(&self.human, expected)
                .unwrap();
            assert_eq!(prompt.actions, visible.actions);
            assert_eq!(prompt.action_meanings, visible.action_meanings);
            assert_eq!(prompt.state, visible.state);
            self.projector
                .submit(
                    &mut self.human,
                    HumanActionRequestV1 {
                        prompt_seq: prompt.prompt_seq,
                        action_index: index,
                    },
                )
                .unwrap();
        } else {
            assert!(self
                .projector
                .project_current(&self.human, expected)
                .is_err());
            self.human
                .step_opponent_with(expected, |observed, actions| {
                    assert_eq!(observed, backend.observation());
                    assert_eq!(actions, backend.actions());
                    Ok(i)
                })
                .unwrap();
        }
        let command = CommandV1::Step {
            episode_id,
            authority_generation,
            actor: expected.acting_player,
            prompt_seq: visible.prompt_seq,
            action_index: index,
        };
        assert!(matches!(
            self.request(command.clone()),
            Body::State {
                receipt: Some(_),
                ..
            }
        ));
        let native_index = backend.fixture_original_index_v75(i).unwrap();
        let result = self
            .raw
            .step(expected.episode_id, expected.step, native_index)
            .unwrap();
        assert_eq!(self.numeric.apply(d, i).unwrap(), result);
        assert_eq!(self.human.current_response(), result);
        assert_eq!(self.raw.kaito_fixture_state(), self.numeric.fixture_state());
        command
    }

    fn wait_payment(&mut self, kind: PaymentKindV64) -> KaitoDecision {
        for _ in 0..40 {
            let d = decision(&self.numeric);
            if d.fixture_backend_v75()
                .candidates_v75()
                .candidates
                .iter()
                .any(|r| r.artist_payment.is_some_and(|p| p.kind == kind))
            {
                return d;
            }
            let index = d
                .fixture_backend_v75()
                .actions()
                .iter()
                .position(|action| {
                    matches!(
                        action,
                        Action::Pass { .. }
                            | Action::OrderTriggers { .. }
                            | Action::ChooseTriggerOrderNext { .. }
                    )
                })
                .expect("unhandled native Raid fixture step");
            self.apply(&d, index);
        }
        panic!("Artist payment did not surface");
    }
}

#[test]
fn paid_artist_choice_parity_private_hand_pairs_and_authenticated_restore_both_seats() {
    for controller in [PlayerId::P0, PlayerId::P1] {
        let payer = controller.opponent();
        for (kind, answer) in [
            (
                PaymentKindV64::SacrificeControlledNonlandPermanent,
                AnswerV1::SelectPayment,
            ),
            (PaymentKindV64::DiscardCard, AnswerV1::SelectPayment),
            (PaymentKindV64::DiscardCard, AnswerV1::DeclinePayment),
        ] {
            let make = |hidden| {
                let (mut state, artist) = native_paid::paid(controller);
                let permanent = put(&mut state, payer, "Fishing Pole", Zone::Battlefield);
                let hand = put(&mut state, payer, hidden, Zone::Hand);
                native_paid::raid(&mut state, controller, artist, true);
                (Owners::fixture(state, payer.into()), permanent, hand)
            };
            let (mut a, permanent, hand) = make("Forest");
            let (mut b, _, _) = make("Island");
            let initial = decision(&a.numeric);
            assert_eq!(
                initial.fixture_backend_v75().expected().acting_player,
                controller.into()
            );
            assert_eq!(initial.packet(), decision(&b.numeric).packet());
            assert_eq!(a.request(CommandV1::Current), b.request(CommandV1::Current));

            for owner in [&mut a, &mut b] {
                let sacrifice =
                    owner.wait_payment(PaymentKindV64::SacrificeControlledNonlandPermanent);
                if kind == PaymentKindV64::DiscardCard {
                    let decline = sacrifice
                        .fixture_backend_v75()
                        .candidates_v75()
                        .candidates
                        .iter()
                        .position(|row| {
                            row.artist_payment.unwrap().answer == AnswerV1::DeclinePayment
                        })
                        .unwrap();
                    owner.apply(&sacrifice, decline);
                }
            }
            let d = decision(&a.numeric);
            let other = decision(&b.numeric);
            if kind == PaymentKindV64::DiscardCard {
                let public_a = capture(a.numeric.fixture_state(), controller).unwrap();
                let public_b = capture(b.numeric.fixture_state(), controller).unwrap();
                assert_eq!(public_a, public_b);
                let redacted = public_a.triggers[0].payment.as_ref().unwrap();
                assert!(redacted.candidates.is_none() && redacted.selected.is_none());
                let chooser = d
                    .fixture_backend_v75()
                    .observation()
                    .artist
                    .as_ref()
                    .unwrap();
                assert_eq!(
                    chooser.triggers[0]
                        .payment
                        .as_ref()
                        .unwrap()
                        .candidates
                        .as_ref()
                        .unwrap()[0]
                        .object,
                    hand.0
                );
                assert_ne!(d.packet(), other.packet());
                assert_ne!(a.request(CommandV1::Current), b.request(CommandV1::Current));
                let human_controller = HumanKaitoSessionV75::from_session(
                    FastActorSessionV1::from_kaito_v75_fixture_state_scoped(
                        a.numeric.fixture_state().clone(),
                        true,
                    ),
                    controller.into(),
                )
                .unwrap();
                assert!(HumanKaitoProjectorV75::new(controller.into())
                    .project_current(&human_controller, d.fixture_backend_v75().expected())
                    .is_err());
            }
            let selected = d
                .fixture_backend_v75()
                .candidates_v75()
                .candidates
                .iter()
                .position(|row| row.artist_payment == Some(PaymentActionV1 { kind, answer }))
                .unwrap();
            let input = a.numeric.training_input_v1(&d).unwrap();
            assert!(b.numeric.training_input_v1(&d).is_err());
            assert!(a.numeric.restore(&b.numeric.snapshot()).is_err());
            let mut targets = vec![0.; input.packet().actions().len()];
            targets[selected] = 1.;
            let mut learner = LearnerV1::new(
                ModelV1::deterministic_fixture(),
                AdamConfigV1 {
                    learning_rate: 0.001,
                    beta1: 0.9,
                    beta2: 0.999,
                    epsilon: 1e-8,
                },
            )
            .unwrap();
            assert!(learner
                .update(&[TrainingExampleV1 {
                    input: &input,
                    targets: &targets
                }])
                .unwrap()
                .is_finite());
            let saved = a.numeric.snapshot();
            let expected = d.fixture_backend_v75().expected();
            let human_prompt = a.projector.project_current(&a.human, expected).unwrap();
            let human_saved = a.human.snapshot_v75();
            let Body::State {
                snapshot_id: Some(snapshot),
                ..
            } = a.request(CommandV1::Snapshot)
            else {
                panic!("Artist wire snapshot absent")
            };
            let before = a.numeric.fixture_state().clone();
            let stale = a.apply(&d, selected);
            let CommandV1::Step { action_index, .. } = stale else {
                unreachable!()
            };
            let after = a.numeric.fixture_state();
            assert_eq!(
                after.objects.get(permanent).zone,
                if kind == PaymentKindV64::SacrificeControlledNonlandPermanent {
                    Zone::Graveyard
                } else {
                    Zone::Battlefield
                }
            );
            assert_eq!(
                after.objects.get(hand).zone,
                if kind == PaymentKindV64::DiscardCard && answer == AnswerV1::SelectPayment {
                    Zone::Graveyard
                } else {
                    Zone::Hand
                }
            );
            assert_eq!(
                after.players[payer.index()].life,
                before.players[payer.index()].life
                    - if answer == AnswerV1::DeclinePayment {
                        3
                    } else {
                        0
                    }
            );
            a.numeric.restore(&saved).unwrap();
            assert_eq!(a.numeric.fixture_state(), &before);
            assert_eq!(decision(&a.numeric).packet(), d.packet());
            assert!(a.numeric.apply(&d, selected as u32).is_err());
            assert!(a.numeric.training_input_v1(&d).is_err());
            a.human.restore_v75(&human_saved).unwrap();
            assert!(a
                .projector
                .submit(
                    &mut a.human,
                    HumanActionRequestV1 {
                        prompt_seq: human_prompt.prompt_seq,
                        action_index,
                    }
                )
                .is_err());
            assert!(matches!(
                a.request(CommandV1::Restore {
                    snapshot_id: snapshot
                }),
                Body::State { .. }
            ));
            assert!(matches!(a.request(stale), Body::Error { .. }));
        }
    }
}
