use super::*;
use crate::{
    current_mixed_policy_v1::KaitoDecision,
    current_tree_learner_v1::{AdamConfigV1, LearnerV1, ModelV1, TrainingExampleV1},
    genesis_wave_public_v69::StageV69,
    human_bo3_v1::{
        project_limited_kaito_v16, HumanActionRequestV1, HumanKaitoProjectorV75,
        LimitedKaitoDecisionV16,
    },
    ids::{ObjectId, PlayerId},
    limited_kaito_session_v16::{LimitedKaitoBodyV16 as Body, LimitedKaitoStateV16 as State},
    policy_observation_v6::tests::put,
    rl::{ActionSemanticV1 as Action, TargetRefV1},
    rl_session::{FastActorResponseV1, KaitoSessionV75},
    state::{GameState, Zone},
};

fn id(name: &str) -> u16 {
    crate::card_def::card_id_by_name(name).unwrap()
}

fn reset_config(actor: PlayerId, printing: u16, lands: &[(&str, usize)]) -> ResetV1 {
    let mut deck = vec![printing; 4];
    for &(name, count) in lands {
        deck.extend(std::iter::repeat_n(id(name), count));
    }
    assert_eq!(deck.len(), 40);
    let mut mainboards = [vec![id("Forest"); 40], vec![id("Forest"); 40]];
    mainboards[actor.index()] = deck;
    ResetV1 {
        expected_card_db_hash: KERNEL_CARDDB_HASH,
        episode_id: 550,
        env_seed: 550_487,
        starting_player: actor.into(),
        deck_ids: ["v68-v75-left".into(), "v68-v75-right".into()],
        mainboards,
        max_physical_decisions: 2_000,
        max_policy_steps: 2_000,
    }
}

fn request(serial: impl ToString, command: CommandV1) -> RequestV1 {
    RequestV1 {
        profile: PROFILE_V1.into(),
        request_id: serial.to_string(),
        command,
    }
}

fn body(reply: ReplyV1) -> Body {
    assert_eq!(reply.profile, PROFILE_V1);
    assert_eq!(reply.producer_sha256, producer_sha256_v1());
    match reply.body {
        ReplyBodyV1::State { payload } => payload.body,
        other => panic!("unexpected profile reply {other:?}"),
    }
}

struct Owners {
    raw: FastActorSessionV1,
    numeric: KaitoSession,
    human: HumanKaitoSessionV75,
    projector: HumanKaitoProjectorV75,
    human_seat: PlayerSeatV1,
    wire: JsonlServerV1,
    serial: u64,
}

impl Owners {
    fn reset(config: &ResetV1) -> Self {
        let human_seat = config.starting_player;
        let mut wire = JsonlServerV1::new();
        let reset = request(
            0,
            CommandV1::Reset {
                config: config.clone(),
            },
        );
        let reply = wire.handle_request(reset.clone());
        assert_eq!(wire.handle_request(reset), reply);
        assert!(matches!(
            body(reply),
            Body::State { state, .. } if matches!(*state, State::Decision { .. })
        ));
        Self {
            raw: native(config).unwrap(),
            numeric: reset_numeric_v1(config).unwrap(),
            human: reset_human_v1(config, human_seat).unwrap(),
            projector: HumanKaitoProjectorV75::new(human_seat),
            human_seat,
            wire,
            serial: 0,
        }
    }

    fn fixture(state: GameState, actor: PlayerId) -> Self {
        let make = || FastActorSessionV1::from_kaito_v75_fixture_state(state.clone());
        let human_seat = actor.into();
        let mut wire = JsonlServerV1::new();
        wire.0
            .install_fixture_core(KaitoSessionV75::from_session(make()).unwrap());
        Self {
            raw: make(),
            numeric: KaitoSession::from_session(make()).unwrap(),
            human: HumanKaitoSessionV75::from_session(make(), human_seat).unwrap(),
            projector: HumanKaitoProjectorV75::new(human_seat),
            human_seat,
            wire,
            serial: 0,
        }
    }

    fn state(&self) -> &GameState {
        self.raw.kaito_fixture_state()
    }

    fn decision(&self) -> KaitoDecision {
        let FastActorResponseV1::Decision(expected) = self.numeric.current_response() else {
            panic!(
                "expected decision, got {:?}",
                self.numeric.current_response()
            );
        };
        assert_eq!(self.raw.current_response(), self.numeric.current_response());
        self.numeric.encode(expected).unwrap()
    }

    fn wire_request(&mut self, command: CommandV1) -> Body {
        self.serial += 1;
        body(self.wire.handle_request(request(self.serial, command)))
    }

    fn wire_menu(&mut self) -> (u64, u64, Box<LimitedKaitoDecisionV16>) {
        let Body::State { state, .. } = self.wire_request(CommandV1::Current) else {
            panic!("wire current refused");
        };
        let State::Decision {
            episode_id,
            authority_generation,
            decision,
        } = *state
        else {
            panic!("wire decision absent: {state:?}");
        };
        (episode_id, authority_generation, decision)
    }

    fn score(&self, decision: &KaitoDecision) {
        let input = self.numeric.training_input_v1(decision).unwrap();
        let scores = ModelV1::deterministic_fixture().forward(&input).unwrap();
        assert_eq!(scores.len(), decision.fixture_backend_v75().actions().len());
        assert!(scores.iter().all(|x| x.is_finite()));
    }

    fn step(&mut self, decision: &KaitoDecision, chosen: usize) -> CommandV1 {
        let chosen = u32::try_from(chosen).unwrap();
        let backend = decision.fixture_backend_v75();
        let expected = backend.expected();
        let raw_index = backend.fixture_original_index_v75(chosen).unwrap();
        let (episode_id, authority_generation, wire) = self.wire_menu();
        let (projected, indices) =
            project_limited_kaito_v16(backend, expected.acting_player, wire.prompt_seq).unwrap();
        assert_eq!(*wire, projected);
        let wire_index = indices.iter().position(|&index| index == chosen).unwrap() as u32;
        if expected.acting_player == self.human_seat {
            let human = self
                .projector
                .project_current(&self.human, expected)
                .unwrap();
            assert_eq!(human.state, wire.state);
            assert_eq!(human.actions, wire.actions);
            assert_eq!(human.action_meanings, wire.action_meanings);
            self.projector
                .submit(
                    &mut self.human,
                    HumanActionRequestV1 {
                        prompt_seq: human.prompt_seq,
                        action_index: wire_index,
                    },
                )
                .unwrap();
        } else {
            assert!(self
                .projector
                .project_current(&self.human, expected)
                .is_err());
            self.human
                .step_opponent_with(expected, |observation, actions| {
                    assert_eq!(observation, backend.observation());
                    assert_eq!(actions, backend.actions());
                    Ok(chosen)
                })
                .unwrap();
        }
        let command = CommandV1::Step {
            episode_id,
            authority_generation,
            actor: wire.actor,
            prompt_seq: wire.prompt_seq,
            action_index: wire_index,
        };
        assert!(matches!(
            self.wire_request(command.clone()),
            Body::State {
                receipt: Some(_),
                ..
            }
        ));
        let next = self
            .raw
            .step(expected.episode_id, expected.step, raw_index)
            .unwrap();
        assert_eq!(self.numeric.apply(decision, chosen).unwrap(), next);
        assert_eq!(self.human.current_response(), next);
        assert_eq!(self.numeric.fixture_state(), self.state());
        command
    }

    fn act(&mut self, select: impl Fn(&Action) -> bool) {
        let decision = self.decision();
        let actions = decision.fixture_backend_v75().actions();
        let chosen = actions
            .iter()
            .position(select)
            .unwrap_or_else(|| panic!("required action absent: {actions:?}"));
        self.score(&decision);
        self.step(&decision, chosen);
    }

    fn quiet(&mut self, decision: &KaitoDecision) {
        let actions = decision.fixture_backend_v75().actions();
        let chosen = actions
            .iter()
            .position(|action| matches!(action, Action::PlayLand { .. }))
            .or_else(|| {
                actions.iter().position(|action| {
                    matches!(
                        action,
                        Action::ChooseLondonMulligan {
                            mulligan: false,
                            ..
                        } | Action::Pass { .. }
                            | Action::ChooseAttackerInclusion { include: false, .. }
                            | Action::ChooseBlockerInclusion { include: false, .. }
                            | Action::OrderTriggers { .. }
                            | Action::ChooseTriggerOrderNext { .. }
                            | Action::Discard { .. }
                            | Action::FinishEffectSelection { .. }
                    )
                })
            })
            .or_else(|| {
                actions
                    .iter()
                    .position(|action| matches!(action, Action::ChooseEffectTarget { .. }))
            })
            .unwrap_or_else(|| panic!("unhandled fixture menu: {actions:?}"));
        self.step(decision, chosen);
    }

    fn wait(&mut self, limit: usize, predicate: impl Fn(&KaitoDecision) -> bool) -> KaitoDecision {
        for _ in 0..limit {
            let decision = self.decision();
            if predicate(&decision) {
                return decision;
            }
            self.quiet(&decision);
        }
        panic!("required profile decision did not occur");
    }

    fn paid_cast(
        &mut self,
        actor: PlayerId,
        printing: u16,
        cost: usize,
        x: Option<u16>,
    ) -> ObjectId {
        let mut source = None;
        for _ in 0..400 {
            let decision = self.decision();
            if self.state().players[actor.index()]
                .battlefield
                .iter()
                .filter(|&&land| !self.state().objects.get(land).tapped)
                .count()
                >= cost
            {
                if let Some((chosen, Action::CastSpell { source: card, .. })) = decision
                    .fixture_backend_v75().actions().iter().enumerate()
                    .find(|(_, action)| matches!(action, Action::CastSpell { source, .. }
                        if source.card_db_id == printing && source.owner == PlayerSeatV1::from(actor)))
                {
                    source = Some(ObjectId(card.arena_id));
                    let before = self.tapped(actor);
                    self.score(&decision);
                    self.step(&decision, chosen);
                    if let Some(x) = x {
                        self.act(|a| matches!(a, Action::ChooseFullRangeCastXValueV59 { source, x_value, .. }
                            if source.arena_id == card.arena_id && *x_value == x)
                            || matches!(a, Action::ChooseEffectOption { source, option_index, .. }
                                if source.arena_id == card.arena_id && *option_index == x));
                    }
                    assert_eq!(self.tapped(actor) - before, cost);
                    break;
                }
            }
            self.quiet(&decision);
        }
        let source = source.unwrap_or_else(|| panic!("printing {printing} did not cast"));
        let state = self.state();
        assert_eq!(state.objects.get(source).zone, Zone::Stack);
        assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
        let row = state.stack.iter().find(|row| row.source == source).unwrap();
        assert_eq!(row.controller, actor);
        assert_eq!(row.v4.x_value, x.unwrap_or(0));
        assert!(state.engine.event_history.iter().any(|event| matches!(
            event, crate::event::CommittedEvent::SpellCast { spell, .. } if *spell == source
        )));
        source
    }

    fn tapped(&self, actor: PlayerId) -> usize {
        self.state().players[actor.index()]
            .battlefield
            .iter()
            .filter(|&&card| self.state().objects.get(card).tapped)
            .count()
    }

    fn check_hidden_pair(&mut self, other: &mut Self) {
        assert_ne!(self.state().state_hash(), other.state().state_hash());
        let a = self.decision();
        let b = other.decision();
        assert_eq!(a.packet(), b.packet());
        assert_eq!(
            a.fixture_backend_v75().observation(),
            b.fixture_backend_v75().observation()
        );
        let expected = a.fixture_backend_v75().expected();
        assert_eq!(expected.acting_player, self.human_seat);
        assert_eq!(
            self.projector
                .project_current(&self.human, expected)
                .unwrap(),
            other
                .projector
                .project_current(&other.human, expected)
                .unwrap()
        );
        assert_eq!(self.wire_menu(), other.wire_menu());
        assert!(other.numeric.training_input_v1(&a).is_err());
        assert!(other.numeric.restore(&self.numeric.snapshot()).is_err());
        assert!(other.human.restore_v75(&self.human.snapshot_v75()).is_err());
        self.score(&a);
    }

    fn rewind_after(&mut self, select: impl Fn(&Action) -> bool) {
        let decision = self.decision();
        let expected = decision.fixture_backend_v75().expected();
        let human_prompt = self
            .projector
            .project_current(&self.human, expected)
            .unwrap();
        let native_snapshot = self.raw.snapshot_v1();
        let numeric_snapshot = self.numeric.snapshot();
        let human_snapshot = self.human.snapshot_v75();
        let Body::State {
            snapshot_id: Some(snapshot_id),
            ..
        } = self.wire_request(CommandV1::Snapshot)
        else {
            panic!("expected owned wire snapshot");
        };
        let index = decision
            .fixture_backend_v75()
            .actions()
            .iter()
            .position(select)
            .unwrap();
        let stale = self.step(&decision, index);
        let CommandV1::Step {
            authority_generation,
            action_index,
            ..
        } = stale
        else {
            unreachable!()
        };

        self.raw.kaito_restore_v75(&native_snapshot).unwrap();
        self.numeric.restore(&numeric_snapshot).unwrap();
        self.human.restore_v75(&human_snapshot).unwrap();
        let restored = self.wire_request(CommandV1::Restore { snapshot_id });
        assert!(
            matches!(restored, Body::State { state, .. } if matches!(*state,
            State::Decision { authority_generation: fresh, .. } if fresh > authority_generation))
        );
        assert_eq!(self.decision().packet(), decision.packet());
        assert_eq!(self.numeric.fixture_state(), self.state());
        assert!(self.numeric.training_input_v1(&decision).is_err());
        assert!(self.numeric.apply(&decision, index as u32).is_err());
        assert!(self
            .projector
            .submit(
                &mut self.human,
                HumanActionRequestV1 {
                    prompt_seq: human_prompt.prompt_seq,
                    action_index,
                }
            )
            .is_err());
        assert!(matches!(self.wire_request(stale), Body::Error { .. }));
    }
}

#[test]
fn exact_profile_gates_and_v3_learner_reset_are_separate_from_legacy() {
    let mut config = reset_config(PlayerId::P0, 486, &[("Island", 36)]);
    config.mainboards[1][..PARTIAL_PRINTINGS_V1.len()].copy_from_slice(&PARTIAL_PRINTINGS_V1);
    let owner = reset_numeric_reveal_facts_v3(&config).unwrap();
    assert!(owner.fixture_state().reveal_knowledge_v4.is_some());
    let FastActorResponseV1::Decision(expected) = owner.current_response() else {
        panic!("V3 reset did not surface a typed decision");
    };
    let decision = owner.encode(expected).unwrap();
    let input = owner.training_input_v1(&decision).unwrap();
    assert!(ModelV1::deterministic_fixture()
        .forward(&input)
        .unwrap()
        .iter()
        .all(|x| x.is_finite()));
    assert!(owner.sampled_clone(&decision, 19).is_err());
    assert!(KaitoSession::reset_explicit_mainboards(
        config.episode_id,
        config.env_seed,
        (100, 100),
        config.deck_ids.clone(),
        config.mainboards.clone(),
        PlayerId::P0,
    )
    .is_err());
    assert!(crate::limited_return_consumer_v1::reset_numeric_v1(&config).is_err());
    for printing in PARTIAL_PRINTINGS_V1 {
        assert!(!CARD_DEFS[usize::from(printing)].has_full_support());
    }
    for illegal in [457, 458, 459, 460, 492, 495, u16::MAX] {
        let mut bad = config.clone();
        bad.mainboards[0][0] = illegal;
        assert!(reset_numeric_v1(&bad).is_err(), "declared {illegal}");
    }
    let mut wire = JsonlServerV1::new();
    let reset = |config| CommandV1::Reset { config };
    let valid = request("valid", reset(config.clone()));
    let mut foreign = valid.clone();
    foreign.profile = crate::limited_return_consumer_v1::PROFILE_V1.into();
    assert!(matches!(wire.handle_request(foreign).body,
        ReplyBodyV1::Error { code } if code == "invalid_header"));
    let ready = body(wire.handle_request(valid));
    let mut old = crate::limited_return_consumer_v1::JsonlServerV1::new();
    assert!(
        matches!(old.handle_request(request("foreign", reset(config.clone()))).body,
        ReplyBodyV1::Error { code } if code == "invalid_header")
    );
    config.expected_card_db_hash ^= 1;
    assert!(
        matches!(wire.handle_request(request("bad-hash", reset(config))).body,
        ReplyBodyV1::Error { code } if code == "unsupported_reset")
    );
    assert_eq!(
        body(wire.handle_request(request("current", CommandV1::Current))),
        ready
    );
    let invalid = r#"{"profile":"fdn-v68-v75-consumer/v1","profile":"fdn-return-consumer/v1","request_id":"duplicate","command":{"kind":"current"}}"#;
    let error: serde_json::Value = serde_json::from_str(&wire.handle_line(invalid)).unwrap();
    assert_eq!(error["code"], "invalid_request");
    let line = serde_json::to_string(&request("jsonl", CommandV1::Current)).unwrap();
    let reply: serde_json::Value = serde_json::from_str(&wire.handle_line(&line)).unwrap();
    assert_eq!(reply["profile"], PROFILE_V1);
    assert_eq!(reply["payload"]["state"]["state_type"], "decision");
}

#[test]
fn all_eight_printings_cast_from_real_forty_card_reset_with_paid_consumer_parity() {
    let cases: [(u16, &[(&str, usize)], usize, Option<u16>); 8] = [
        (486, &[("Island", 36)], 6, None),
        (487, &[("Forest", 36)], 5, Some(2)),
        (488, &[("Forest", 36)], 5, None),
        (
            489,
            &[("Swamp", 12), ("Forest", 12), ("Island", 12)],
            6,
            None,
        ),
        (490, &[("Swamp", 36)], 2, None),
        (491, &[("Swamp", 36)], 6, None),
        (493, &[("Mountain", 36)], 7, None),
        (494, &[("Island", 36)], 3, None),
    ];
    for actor in [PlayerId::P0, PlayerId::P1] {
        for (printing, lands, cost, x) in cases {
            let mut owners = Owners::reset(&reset_config(actor, printing, lands));
            let source = owners.paid_cast(actor, printing, cost, x);
            for _ in 0..60 {
                if owners.state().objects.get(source).zone != Zone::Stack {
                    break;
                }
                owners.quiet(&owners.decision());
            }
            let expected_zone = match printing {
                486 => Zone::Exile,
                487 => Zone::Graveyard,
                _ => Zone::Battlefield,
            };
            assert_eq!(owners.state().objects.get(source).zone, expected_zone);
        }
    }
}

#[test]
fn paid_wave_hidden_pair_subset_aura_finish_and_authenticated_rewind_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let make = |hidden| {
            let (state, _, _) = crate::rl_session::genesis_wave_v69::tests::ready(actor, hidden);
            Owners::fixture(state, actor)
        };
        let (mut a, mut b) = (make("Island"), make("Counterspell"));
        for owner in [&mut a, &mut b] {
            owner.paid_cast(actor, 487, 7, Some(4));
            let decision = owner.wait(40, |d| {
                d.fixture_backend_v75()
                    .observation()
                    .genesis_wave
                    .waves
                    .iter()
                    .any(|w| w.stage == StageV69::Subset)
            });
            let wave = &decision
                .fixture_backend_v75()
                .observation()
                .genesis_wave
                .waves[0];
            assert_eq!(wave.x_value, 4);
            assert_eq!(wave.revealed.len(), 4);
            assert_eq!(wave.revealed.iter().filter(|row| row.eligible).count(), 3);
        }
        a.check_hidden_pair(&mut b);
        let choose = |printing, zone| {
            move |action: &Action| {
                matches!(action,
            Action::ChooseEffectTarget { target: TargetRefV1::Object { object }, .. }
                if object.card_db_id == printing && object.zone == zone)
            }
        };
        let subset = a.decision();
        let forest = subset
            .fixture_backend_v75()
            .actions()
            .iter()
            .position(choose(id("Forest"), Zone::Library))
            .unwrap();
        let input = a.numeric.training_input_v1(&subset).unwrap();
        let mut targets = vec![0.; input.packet().actions().len()];
        targets[forest] = 1.;
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
                targets: &targets,
            }])
            .unwrap()
            .is_finite());
        for owner in [&mut a, &mut b] {
            owner.rewind_after(choose(id("Forest"), Zone::Library));
            owner.act(choose(id("Forest"), Zone::Library));
            owner.act(choose(id("Witness Protection"), Zone::Library));
            owner.act(|action| matches!(action, Action::FinishEffectSelection { .. }));
            owner.act(choose(id("Llanowar Elves"), Zone::Battlefield));
            let decision = owner.wait(40, |d| {
                d.fixture_backend_v75()
                    .observation()
                    .genesis_wave
                    .waves
                    .iter()
                    .any(|w| w.stage == StageV69::Finished)
            });
            let wave = &decision
                .fixture_backend_v75()
                .observation()
                .genesis_wave
                .waves[0];
            assert_eq!(wave.selected_indices, vec![0, 3]);
            assert_eq!(wave.attachments.len(), 1);
            assert_eq!(wave.spell_departure, Some(Zone::Graveyard));
        }
    }
}

#[test]
fn paid_time_stop_disposes_paid_wave_and_preserves_hidden_pair_during_cleanup_restore() {
    use crate::time_stop_public_v68::PhaseV68;

    for actor in [PlayerId::P0, PlayerId::P1] {
        let make = |hidden| {
            let mut state = crate::engine::time_stop_v1::tests::ready_hidden(actor, hidden);
            put(&mut state, actor, "Genesis Wave", Zone::Hand);
            put(&mut state, actor, "Time Stop", Zone::Hand);
            for _ in 0..10 {
                put(&mut state, actor, "Island", Zone::Hand);
            }
            Owners::fixture(state, actor)
        };
        let (mut a, mut b) = (make("Forest"), make("Counterspell"));
        for owner in [&mut a, &mut b] {
            let wave = owner.paid_cast(actor, 487, 4, Some(1));
            let stop = owner.paid_cast(actor, 486, 6, None);
            let decision = owner.wait(40, |d| {
                d.fixture_backend_v75()
                    .observation()
                    .end_turn
                    .cleanup
                    .as_ref()
                    .is_some_and(|cleanup| cleanup.phase == PhaseV68::Discard)
            });
            let observed = decision.fixture_backend_v75().observation();
            let disposal = &observed.end_turn.disposals[0];
            assert_eq!(disposal.disposed_rows.len(), 2);
            assert!(disposal
                .disposed_rows
                .iter()
                .any(|row| row.original.source.arena_id == wave.0 && row.original.x_value == 1));
            assert!(disposal
                .disposed_rows
                .iter()
                .any(|row| row.original.source.arena_id == stop.0));
            assert_eq!(
                observed.end_turn.cleanup.as_ref().unwrap().discard_count,
                Some(3)
            );
            assert!(observed.genesis_wave.waves.is_empty());
            assert_eq!(owner.state().objects.get(wave).zone, Zone::Exile);
            assert_eq!(owner.state().objects.get(stop).zone, Zone::Exile);
        }
        a.check_hidden_pair(&mut b);
        for owner in [&mut a, &mut b] {
            let discard = |action: &Action| matches!(action, Action::Discard { .. });
            owner.rewind_after(discard);
            owner.act(discard);
        }
    }
}
