//! Finite engineering checks, not Full promotion or training-strength evidence.
use super::*;
use crate::{
    current_mixed_policy_v1::KaitoDecision,
    current_tree_learner_v1::{AdamConfigV1, LearnerV1, ModelV1, TrainingExampleV1},
    graveyard_return_public_v61::tests as native,
    human_bo3_v1::{project_limited_kaito_v16, HumanActionRequestV1, HumanKaitoProjectorV75},
    ids::{ObjectId, PlayerId},
    limited_kaito_session_v16::{LimitedKaitoBodyV16, LimitedKaitoReplyV16, LimitedKaitoStateV16},
    policy_observation_v6::tests::put,
    rl::{ActionSemanticV1 as A, TargetRefV1},
    rl_session::{FastActorResponseV1, KaitoSessionV75},
    state::{GameState, Zone},
    valkyries_call_public_v62::StageV62,
};

fn request(id: impl ToString, command: CommandV1) -> RequestV1 {
    RequestV1 {
        profile: PROFILE_V1.into(),
        request_id: id.to_string(),
        command,
    }
}
fn config(actor: PlayerId) -> ResetV1 {
    let land = crate::card_def::card_id_by_name("Swamp").unwrap();
    let mut deck = vec![land; 37];
    deck.extend(PARTIAL_PRINTINGS_V1);
    ResetV1 {
        expected_card_db_hash: KERNEL_CARDDB_HASH,
        episode_id: 550,
        env_seed: 454_550,
        starting_player: actor.into(),
        deck_ids: ["return-left".into(), "return-right".into()],
        mainboards: [deck.clone(), deck],
        max_physical_decisions: 1000,
        max_policy_steps: 1000,
    }
}
fn decision(owner: &KaitoSession) -> KaitoDecision {
    let FastActorResponseV1::Decision(expected) = owner.current_response() else {
        panic!("expected decision")
    };
    owner.encode(expected).unwrap()
}
fn payload(reply: ReplyV1) -> LimitedKaitoReplyV16 {
    match reply.body {
        ReplyBodyV1::State { payload } => *payload,
        other => panic!("unexpected reply {other:?}"),
    }
}

#[test]
fn exact_forty_card_profile_reset_all_consumers_and_unchanged_full_gate_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let config = config(actor);
        assert!(KaitoSession::reset_explicit_mainboards(
            config.episode_id,
            config.env_seed,
            (1000, 1000),
            config.deck_ids.clone(),
            config.mainboards.clone(),
            actor
        )
        .is_err());
        let owner = reset_numeric_v1(&config).unwrap();
        let current = decision(&owner);
        let input = owner.training_input_v1(&current).unwrap();
        assert!(ModelV1::deterministic_fixture()
            .forward(&input)
            .unwrap()
            .iter()
            .all(|x| x.is_finite()));
        let expected = current.fixture_backend_v75().expected();
        let human = reset_human_v1(&config, expected.acting_player).unwrap();
        let mut projector = HumanKaitoProjectorV75::new(expected.acting_player);
        assert!(!projector
            .project_current(&human, expected)
            .unwrap()
            .actions
            .is_empty());
        let mut wire = JsonlServerV1::new();
        let req = request(
            "reset",
            CommandV1::Reset {
                config: config.clone(),
            },
        );
        let line = serde_json::to_string(&req).unwrap();
        let reply: serde_json::Value = serde_json::from_str(&wire.handle_line(&line)).unwrap();
        assert_eq!(reply["profile"], PROFILE_V1);
        assert_eq!(reply["payload"]["state"]["state_type"], "decision");
        assert_eq!(wire.handle_line(&line), wire.handle_line(&line));
        assert!(serde_json::from_value::<RequestV1>(serde_json::json!({
            "profile": PROFILE_V1, "request_id": "bad", "command": {"kind":"reset", "config":config, "unrepresented":true}
        })).is_err());
        for id in [460, 494, 495, u16::MAX] {
            let mut bad = config.clone();
            bad.mainboards[0][0] = id;
            assert!(reset_numeric_v1(&bad).is_err(), "undeclared printing {id}");
        }
        assert!(PARTIAL_PRINTINGS_V1
            .iter()
            .all(|&id| !CARD_DEFS[usize::from(id)].has_full_support()));
        let mut bad = config.clone();
        bad.expected_card_db_hash ^= 1;
        assert!(reset_numeric_v1(&bad).is_err());
    }
}

#[test]
fn issue552_return_profile_enables_public_minima_before_its_first_owned_menu() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let config = config(actor);
        let legacy = reset_numeric_v1(&config).unwrap();
        let owner = reset_numeric_reveal_facts_v3(&config).unwrap();
        assert!(legacy.fixture_state().reveal_knowledge_v4.is_none());
        assert!(owner.fixture_state().reveal_knowledge_v4.is_some());
        let current = decision(&owner);
        let input = owner.training_input_v1(&current).unwrap();
        assert!(ModelV1::deterministic_fixture()
            .forward(&input)
            .unwrap()
            .iter()
            .all(|x| x.is_finite()));
        let mut bad = config;
        bad.mainboards[actor.index()][0] = 460;
        assert!(reset_numeric_reveal_facts_v3(&bad).is_err());
    }
}

#[test]
fn exact_forty_card_reset_reaches_real_paid_casts_without_fixture_mana_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let id = |name| crate::card_def::card_id_by_name(name).unwrap();
        let mut config = config(actor);
        let mut deck = vec![id("Swamp"); 10];
        deck.extend([id("Plains"); 10]);
        deck.extend([id("Mountain"); 8]);
        for printing in PARTIAL_PRINTINGS_V1 {
            deck.extend([printing; 4]);
        }
        assert_eq!(deck.len(), 40);
        config.mainboards = [vec![id("Forest"); 40], vec![id("Forest"); 40]];
        config.mainboards[actor.index()] = deck;
        config.max_policy_steps = 5000;
        config.max_physical_decisions = 5000;
        let mut owner = reset_numeric_v1(&config).unwrap();
        let mut cast = std::collections::BTreeSet::new();
        for _ in 0..4000 {
            let state = owner.fixture_state();
            if PARTIAL_PRINTINGS_V1.iter().all(|&id| {
                state.players[actor.index()]
                    .battlefield
                    .iter()
                    .any(|&object| state.objects.get(object).card_def == id)
            }) {
                break;
            }
            let d = decision(&owner);
            let actions = d.fixture_backend_v75().actions();
            let selected = actions.iter().position(|a| matches!(a, A::PlayLand { .. }))
                .or_else(|| actions.iter().position(|a| matches!(a, A::CastSpell { source, .. }
                    if PARTIAL_PRINTINGS_V1.contains(&source.card_db_id) && !cast.contains(&source.card_db_id))))
                .or_else(|| actions.iter().position(|a| matches!(a,
                    A::ChooseLondonMulligan { mulligan: false, .. } | A::Pass { .. }
                    | A::ChooseAttackerInclusion { include: false, .. } | A::ChooseBlockerInclusion { include: false, .. }
                    | A::OrderTriggers { .. } | A::ChooseTriggerOrderNext { .. } | A::Discard { .. }
                    | A::FinishEffectSelection { .. })))
                .unwrap_or_else(|| panic!("unhandled reset action {actions:?}"));
            if let A::CastSpell { source, .. } = &actions[selected] {
                let printing = source.card_db_id;
                let input = owner.training_input_v1(&d).unwrap();
                assert!(ModelV1::deterministic_fixture()
                    .forward(&input)
                    .unwrap()
                    .iter()
                    .all(|x| x.is_finite()));
                owner.apply(&d, selected as u32).unwrap();
                let state = owner.fixture_state();
                let object = state.objects.get(ObjectId(source.arena_id));
                assert_eq!(object.zone, Zone::Stack);
                assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
                assert!(state.players[actor.index()]
                    .battlefield
                    .iter()
                    .any(|&id| state.objects.get(id).tapped));
                cast.insert(printing);
            } else {
                owner.apply(&d, selected as u32).unwrap();
            }
        }
        assert_eq!(cast.into_iter().collect::<Vec<_>>(), PARTIAL_PRINTINGS_V1);
        assert!(PARTIAL_PRINTINGS_V1
            .iter()
            .all(|&id| owner.fixture_state().players[actor.index()]
                .battlefield
                .iter()
                .any(|&object| owner.fixture_state().objects.get(object).card_def == id)));
    }
}

/// Independent native owner is the execution oracle. Fixtures supply initial
/// lands/hand only; all casts, deaths, return selections and effects are real.
struct Pair {
    numeric: KaitoSession,
    raw: FastActorSessionV1,
    human: HumanKaitoSessionV75,
    projector: HumanKaitoProjectorV75,
    human_seat: PlayerSeatV1,
    wire: JsonlServerV1,
    serial: u64,
}
impl Pair {
    fn new(state: GameState, actor: PlayerId) -> Self {
        let make = || FastActorSessionV1::from_kaito_v75_fixture_state(state.clone());
        let numeric = KaitoSession::from_session(make()).unwrap();
        let raw = make();
        let human = HumanKaitoSessionV75::from_session(make(), actor.into()).unwrap();
        let mut wire = JsonlServerV1::new();
        wire.0
            .install_fixture_core(KaitoSessionV75::from_session(make()).unwrap());
        Self {
            numeric,
            raw,
            human,
            projector: HumanKaitoProjectorV75::new(actor.into()),
            human_seat: actor.into(),
            wire,
            serial: 0,
        }
    }
    fn current(&self) -> KaitoDecision {
        decision(&self.numeric)
    }
    fn state(&self) -> &GameState {
        self.raw.kaito_fixture_state()
    }
    fn act(&mut self, choose: impl Fn(&A) -> bool) {
        let d = self.current();
        let backend = d.fixture_backend_v75();
        let ordinal = backend
            .actions()
            .iter()
            .position(choose)
            .unwrap_or_else(|| panic!("missing action {:?}", backend.actions()))
            as u32;
        let expected = backend.expected();
        let raw_index = backend.fixture_original_index_v75(ordinal).unwrap();
        let input = self.numeric.training_input_v1(&d).unwrap();
        assert!(ModelV1::deterministic_fixture()
            .forward(&input)
            .unwrap()
            .iter()
            .all(|x| x.is_finite()));
        self.serial += 1;
        let current = payload(self.wire.handle_request(request(
            format!("current-{}", self.serial),
            CommandV1::Current,
        )));
        let LimitedKaitoBodyV16::State { state, .. } = current.body else {
            panic!("wire refused")
        };
        let LimitedKaitoStateV16::Decision {
            episode_id,
            authority_generation,
            decision: visible,
        } = *state
        else {
            panic!("wire terminal")
        };
        let (projected, indexes) =
            project_limited_kaito_v16(backend, expected.acting_player, visible.prompt_seq).unwrap();
        assert_eq!(*visible, projected);
        let wire_index = indexes.iter().position(|&index| index == ordinal).unwrap() as u32;
        if expected.acting_player == self.human_seat {
            let human = self
                .projector
                .project_current(&self.human, expected)
                .unwrap();
            assert_eq!(human.state, visible.state);
            assert_eq!(human.actions, visible.actions);
            assert_eq!(human.action_meanings, visible.action_meanings);
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
                .step_opponent_with(expected, |_, actions| {
                    assert_eq!(actions, backend.actions());
                    Ok(ordinal)
                })
                .unwrap();
        }
        let stepped = payload(self.wire.handle_request(request(
            format!("step-{}", self.serial),
            CommandV1::Step {
                episode_id,
                authority_generation,
                actor: expected.acting_player,
                prompt_seq: visible.prompt_seq,
                action_index: wire_index,
            },
        )));
        assert!(matches!(
            stepped.body,
            LimitedKaitoBodyV16::State {
                receipt: Some(_),
                ..
            }
        ));
        let actual = self.numeric.apply(&d, ordinal).unwrap();
        assert_eq!(
            actual,
            self.raw
                .step(expected.episode_id, expected.step, raw_index)
                .unwrap()
        );
        assert_eq!(actual, self.human.current_response());
        assert_eq!(self.numeric.fixture_state(), self.state());
    }
    fn quiet(&mut self) {
        self.act(|a| {
            matches!(
                a,
                A::Pass { .. }
                    | A::OrderTriggers { .. }
                    | A::ChooseTriggerOrderNext { .. }
                    | A::ChooseAttackerInclusion { include: false, .. }
                    | A::ChooseBlockerInclusion { include: false, .. }
                    | A::FinishEffectSelection { .. }
            )
        });
    }
    fn drain(&mut self) {
        for _ in 0..100 {
            let s = self.state();
            if s.stack.is_empty()
                && s.engine.pending_cast.is_none()
                && s.engine.pending_activation.is_none()
                && s.engine.pending_effect.is_none()
                && s.engine.pending_triggers.is_empty()
            {
                return;
            }
            self.quiet();
        }
        panic!("stack did not settle")
    }
    fn cast(&mut self, source: ObjectId, target: Option<ObjectId>) {
        for _ in 0..32 {
            if self
                .current()
                .fixture_backend_v75()
                .actions()
                .iter()
                .any(|a| matches!(a, A::CastSpell { source: row, .. } if row.arena_id == source.0))
            {
                break;
            }
            self.quiet();
        }
        self.act(|a| matches!(a, A::CastSpell { source: row, .. } if row.arena_id == source.0));
        if let Some(target) = target {
            self.act(|a| matches!(a, A::ChooseTarget { target: TargetRefV1::Object { object }, .. } if object.arena_id == target.0));
        }
        assert_eq!(self.state().objects.get(source).zone, Zone::Stack);
        assert!(self.state().engine.pending_cast.is_none());
    }
}

#[test]
fn paid_three_printing_cast_death_return_and_all_consumer_index_parity_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = crate::call_observation_v62::tests::ready(actor, "Forest");
        let call = put(&mut state, actor, "Valkyrie's Call", Zone::Hand);
        let alesha = put(&mut state, actor, "Alesha, Who Laughs at Fate", Zone::Hand);
        let elf = put(&mut state, actor, "Llanowar Elves", Zone::Hand);
        let bolt = put(&mut state, actor.opponent(), "Lightning Bolt", Zone::Hand);
        let mut pair = Pair::new(state, actor);
        pair.cast(call, None);
        pair.drain();
        pair.cast(alesha, None);
        pair.drain();
        pair.cast(elf, None);
        pair.drain();
        pair.cast(bolt, Some(elf));
        // Stop at the real return trigger. The return row represents a live
        // continuation and disappears as soon as the trigger resolves.
        for _ in 0..100 {
            if pair.state().objects.get(elf).zone == Zone::Graveyard {
                break;
            }
            pair.quiet();
        }
        let dead = pair.state().objects.get(elf);
        assert_eq!(dead.zone, Zone::Graveyard);
        let pending = pair.current();
        let facts = &pending.fixture_backend_v75().observation().call;
        let [returning] = facts.returns.as_slice() else {
            panic!("expected one published Call return, got {:?}", facts.returns);
        };
        assert_eq!(returning.stage, StageV62::Published);
        assert_eq!(returning.source.original_source.object, call.0);
        assert_eq!(returning.original_graveyard.object, elf.0);
        assert_eq!(
            returning.original_graveyard.generation,
            dead.zone_change_count
        );
        assert_eq!(returning.under_physical_owner, actor.into());
        assert!(returning.return_eligible_now);
        let occurrence = returning.death_occurrence;

        pair.drain();
        assert_eq!(pair.state().objects.get(elf).zone, Zone::Battlefield);
        assert_eq!(pair.state().objects.get(elf).counters.plus1_plus1, 1);
        let settled = pair.current();
        let facts = &settled.fixture_backend_v75().observation().call;
        assert!(facts.returns.is_empty());
        let death = facts
            .deaths
            .iter()
            .find(|row| row.occurrence == occurrence)
            .unwrap();
        assert_eq!(death.original_graveyard.object, elf.0);
        assert!(death.collection_completed);
        assert!(death.current_exact_graveyard_card.is_none());

        let mut state = native::ready(actor);
        let familiar = put(&mut state, actor, "Nine-Lives Familiar", Zone::Hand);
        let breath = put(&mut state, actor, "Breath Weapon", Zone::Hand);
        let mut pair = Pair::new(state, actor);
        pair.cast(familiar, None);
        pair.drain();
        assert_eq!(pair.state().objects.get(familiar).counters.revival, 8);
        pair.cast(breath, None);
        pair.drain();
        assert_eq!(pair.state().objects.get(familiar).zone, Zone::Graveyard);
        let d = pair.current();
        let schedules = &d
            .fixture_backend_v75()
            .observation()
            .graveyard_return
            .nine_lives_schedules;
        assert_eq!(schedules.len(), 1);
        assert_eq!(schedules[0].return_base_before_replacements, 7);
        for _ in 0..300 {
            if pair.state().objects.get(familiar).zone == Zone::Battlefield {
                break;
            }
            pair.quiet();
        }
        assert_eq!(pair.state().objects.get(familiar).zone, Zone::Battlefield);
        assert_eq!(pair.state().objects.get(familiar).counters.revival, 7);
    }
}

#[test]
fn real_raid_target_accept_decline_training_capture_restore_and_foreign_refusal_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for accept in [false, true] {
            let (state, _, target) = native::raid_ready(actor);
            let mut pair = Pair::new(state.clone(), actor);
            pair.act(|a| matches!(a, A::ChooseTarget { target: TargetRefV1::Object { object }, .. } if object.arena_id == target.0));
            for _ in 0..40 {
                if pair
                    .current()
                    .fixture_backend_v75()
                    .actions()
                    .iter()
                    .any(|a| matches!(a, A::ChooseEffectBoolean { .. }))
                {
                    break;
                }
                pair.quiet();
            }
            let d = pair.current();
            let chosen = d
                .fixture_backend_v75()
                .actions()
                .iter()
                .position(|a| matches!(a, A::ChooseEffectBoolean { value, .. } if *value == accept))
                .unwrap();
            let input = pair.numeric.training_input_v1(&d).unwrap();
            assert_eq!(input.action_flags()[chosen][4], accept);
            let mut targets = vec![0.; input.packet().actions().len()];
            targets[chosen] = 1.;
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
            let saved = pair.numeric.snapshot();
            let foreign = Pair::new(state, actor);
            assert!(pair.numeric.restore(&foreign.numeric.snapshot()).is_err());
            assert!(foreign.numeric.training_input_v1(&d).is_err());
            pair.act(|a| matches!(a, A::ChooseEffectBoolean { value, .. } if *value == accept));
            pair.drain();
            assert_eq!(
                pair.state().objects.get(target).zone,
                if accept {
                    Zone::Battlefield
                } else {
                    Zone::Graveyard
                }
            );
            pair.numeric.restore(&saved).unwrap();
            assert_eq!(decision(&pair.numeric).packet(), d.packet());
            assert!(pair.numeric.apply(&d, chosen as u32).is_err());
            assert!(pair.numeric.training_input_v1(&d).is_err());
        }
    }
}

#[test]
fn independent_hidden_worlds_preserve_active_call_human_wire_and_numeric_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let make = |hidden| {
            let (mut state, _, _) = crate::call_observation_v62::tests::paid_scene(actor, hidden);
            native::paid(&mut state, actor, "Alesha, Who Laughs at Fate", None);
            native::paid(&mut state, actor, "Nine-Lives Familiar", None);
            Pair::new(state, actor)
        };
        let (mut a, mut b) = (make("Forest"), make("Counterspell"));
        assert_ne!(a.state().state_hash(), b.state().state_hash());
        assert_eq!(a.current().packet(), b.current().packet());
        let h = a.current().fixture_backend_v75().expected();
        assert_eq!(
            a.projector.project_current(&a.human, h).unwrap(),
            b.projector.project_current(&b.human, h).unwrap()
        );
        assert_eq!(
            a.wire.handle_line(
                &serde_json::to_string(&request("current", CommandV1::Current)).unwrap()
            ),
            b.wire.handle_line(
                &serde_json::to_string(&request("current", CommandV1::Current)).unwrap()
            )
        );
    }
}

#[test]
fn wire_reset_restore_preserve_monotonic_authority_and_reject_old_steps() {
    let mut server = JsonlServerV1::new();
    let config = config(PlayerId::P0);
    let reset = request(
        "reset-1",
        CommandV1::Reset {
            config: config.clone(),
        },
    );
    let first = server.handle_request(reset.clone());
    assert_eq!(server.handle_request(reset), first);
    let LimitedKaitoBodyV16::State { state, .. } = payload(first).body else {
        panic!()
    };
    let LimitedKaitoStateV16::Decision {
        episode_id,
        authority_generation,
        decision,
    } = *state
    else {
        panic!()
    };
    let old = CommandV1::Step {
        episode_id,
        authority_generation,
        actor: decision.actor,
        prompt_seq: decision.prompt_seq,
        action_index: 0,
    };
    let snapshot = payload(server.handle_request(request("snapshot", CommandV1::Snapshot)));
    let LimitedKaitoBodyV16::State {
        snapshot_id: Some(snapshot_id),
        ..
    } = snapshot.body
    else {
        panic!()
    };
    let restored =
        payload(server.handle_request(request("restore", CommandV1::Restore { snapshot_id })));
    let LimitedKaitoBodyV16::State { state, .. } = restored.body else {
        panic!()
    };
    let LimitedKaitoStateV16::Decision {
        authority_generation: restored_generation,
        ..
    } = *state
    else {
        panic!()
    };
    assert!(restored_generation > authority_generation);
    let stale = payload(server.handle_request(request("stale-restore", old.clone())));
    assert!(matches!(stale.body, LimitedKaitoBodyV16::Error { .. }));
    let fresh = payload(server.handle_request(request("reset-2", CommandV1::Reset { config })));
    let LimitedKaitoBodyV16::State { state, .. } = fresh.body else {
        panic!()
    };
    let LimitedKaitoStateV16::Decision {
        authority_generation: fresh_generation,
        ..
    } = *state
    else {
        panic!()
    };
    assert!(fresh_generation > restored_generation);
    let stale = payload(server.handle_request(request("stale-reset", old)));
    assert!(matches!(stale.body, LimitedKaitoBodyV16::Error { .. }));
    let stale_snapshot =
        payload(server.handle_request(request("old-snapshot", CommandV1::Restore { snapshot_id })));
    assert!(matches!(
        stale_snapshot.body,
        LimitedKaitoBodyV16::Error { .. }
    ));
}
