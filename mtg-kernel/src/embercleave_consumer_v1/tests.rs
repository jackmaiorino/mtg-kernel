use super::*;
use crate::{
    card_def::{card_id_by_name, Keywords, KERNEL_CARDDB_HASH},
    current_mixed_policy_v1::KaitoDecision,
    current_tree_learner_v1::ModelV1,
    human_bo3_v1::{project_limited_kaito_v16, HumanActionRequestV1, HumanKaitoProjectorV75},
    ids::ObjectId,
    limited_kaito_session_v16::{LimitedKaitoBodyV16 as Body, LimitedKaitoStateV16 as State},
    policy_observation_v6::tests::put,
    policy_surface_v5::LimitedAppendixV1,
    rl_session::{FastActorResponseV1, KaitoSessionV75},
};

fn card(name: &str) -> u16 {
    card_id_by_name(name).unwrap()
}

fn config(actor: PlayerId) -> ResetV1 {
    let mut deck = vec![PRINTING_V1; 4];
    deck.extend([card("Firebrand Archer"); 12]);
    deck.extend([card("Mountain"); 24]);
    let mut mainboards = [vec![card("Forest"); 40], vec![card("Forest"); 40]];
    mainboards[actor.index()] = deck;
    ResetV1 {
        expected_card_db_hash: KERNEL_CARDDB_HASH,
        episode_id: 550,
        env_seed: 550_462,
        starting_player: actor.into(),
        deck_ids: ["embercleave-left".into(), "embercleave-right".into()],
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

fn decision(session: &KaitoSession) -> KaitoDecision {
    let FastActorResponseV1::Decision(expected) = session.current_response() else {
        panic!(
            "Embercleave decision absent: {:?}",
            session.current_response()
        );
    };
    session.encode(expected).unwrap()
}

fn body(wire: &mut JsonlServerV1, serial: u64, command: CommandV1) -> Body {
    let reply = wire.handle_request(request(serial, command));
    assert_eq!(reply.profile, PROFILE_V1);
    let ReplyBodyV1::State { payload } = reply.body else {
        panic!("profile request rejected: {reply:?}");
    };
    payload.body
}

fn tapped(state: &GameState, actor: PlayerId) -> usize {
    state.players[actor.index()]
        .battlefield
        .iter()
        .filter(|&&id| state.objects.get(id).tapped)
        .count()
}

#[test]
fn exact_forty_card_v3_reset_pays_current_attack_reduction_and_attaches_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let config = config(actor);
        assert!(crate::niv_consumer_v1::reset_numeric_v1(&config).is_err());
        assert!(crate::artist_consumer_v1::reset_numeric_v1(&config).is_err());
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
        for illegal in [461, 463, 486, 492, 495, u16::MAX] {
            let mut invalid = config.clone();
            invalid.mainboards[actor.index()][0] = illegal;
            assert!(reset_numeric_v1(&invalid).is_err());
        }
        let reset = request(
            "reset",
            CommandV1::Reset {
                config: config.clone(),
            },
        );
        let mut wire = JsonlServerV1::new();
        let line = serde_json::to_string(&reset).unwrap();
        let reply: serde_json::Value = serde_json::from_str(&wire.handle_line(&line)).unwrap();
        assert_eq!(
            reply["payload"]["state"]["decision"]["schema"],
            "mtg-kernel-limited-embercleave-decision/v1"
        );

        let mut numeric = reset_numeric_reveal_facts_v3(&config).unwrap();
        assert!(numeric.fixture_state().reveal_knowledge_v4.is_some());
        let opening = decision(&numeric);
        let expected = opening.fixture_backend_v75().expected();
        let human = reset_human_v1(&config, expected.acting_player).unwrap();
        let view = HumanKaitoProjectorV75::new(expected.acting_player)
            .project_current(&human, expected)
            .unwrap();
        assert_eq!(view.schema, "mtg-kernel-human-embercleave-decision/v1");
        assert_eq!(
            opening.fixture_backend_v75().observation().schema,
            OBSERVATION_SCHEMA
        );

        let mut ember = None;
        let mut attached = false;
        let archer = card("Firebrand Archer");
        for _ in 0..500 {
            let state = numeric.fixture_state();
            if let Some(link) = ember.and_then(|id| state.objects.get(id).v4.attached_to) {
                assert_eq!(state.objects.get(link.object).card_def, archer);
                assert_eq!(
                    crate::engine::checked_effective_pt(state, link.object).unwrap(),
                    (3, 2)
                );
                for keyword in [Keywords::DOUBLE_STRIKE, Keywords::TRAMPLE] {
                    assert!(crate::engine::has_effective_keyword(
                        state,
                        link.object,
                        keyword
                    ));
                }
                attached = true;
                break;
            }
            let current = decision(&numeric);
            let rows = &current.fixture_backend_v75().candidates_v75().candidates;
            let priority = |a: &Action| matches!(a, Action::PlayLand { .. });
            let battlefield_archers = state.players[actor.index()]
                .battlefield
                .iter()
                .filter(|&&object| state.objects.get(object).card_def == archer)
                .count();
            let selected = rows
                .iter()
                .position(|row| {
                    matches!(row.embercleave_action,
                        Some(ActionV1::CastQuote { controlled_attackers, .. })
                            if ember.is_none() && controlled_attackers > 0
                    ) || matches!(row.embercleave_action, Some(ActionV1::AttachTarget { .. }))
                })
                .or_else(|| rows.iter().position(|row| priority(&row.action)))
                .or_else(|| {
                    rows.iter().position(|row| {
                        matches!(&row.action,
                            Action::CastSpell { source, .. }
                                if source.card_db_id == archer && battlefield_archers < 2
                        )
                    })
                })
                .or_else(|| {
                    rows.iter().position(|row| {
                        matches!(
                            row.action,
                            Action::ChooseLondonMulligan {
                                mulligan: false,
                                ..
                            } | Action::ChooseAttackerInclusion { include: true, .. }
                                | Action::ChooseBlockerInclusion { include: false, .. }
                                | Action::Pass { .. }
                                | Action::Discard { .. }
                                | Action::ChooseTriggerOrderNext { .. }
                                | Action::OrderTriggers { .. }
                        )
                    })
                })
                .unwrap_or_else(|| {
                    panic!(
                        "unhandled Embercleave reset menu: {:?}",
                        current.fixture_backend_v75().actions()
                    )
                });
            let before = tapped(state, actor);
            let row = &rows[selected];
            if let Some(ActionV1::CastQuote {
                generic_due_now,
                retained_red_pips,
                ..
            }) = row.embercleave_action
            {
                let Action::CastSpell { source, .. } = &row.action else {
                    panic!()
                };
                let id = ObjectId(source.arena_id);
                let input = numeric.training_input_v1(&current).unwrap();
                assert!(ModelV1::deterministic_fixture()
                    .forward(&input)
                    .unwrap()
                    .iter()
                    .all(|x| x.is_finite()));
                numeric.apply(&current, selected as u32).unwrap();
                let paid = numeric.fixture_state();
                assert_eq!(paid.objects.get(id).zone, Zone::Stack);
                assert_eq!(
                    tapped(paid, actor) - before,
                    usize::from(generic_due_now + retained_red_pips)
                );
                assert_eq!(paid.players[actor.index()].mana_pool, [0; 6]);
                ember = Some(id);
            } else {
                numeric.apply(&current, selected as u32).unwrap();
            }
        }
        assert!(
            attached,
            "real Embercleave cast and attachment did not complete"
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
        let make = || {
            FastActorSessionV1::from_kaito_v75_fixture_state_scoped(
                state.clone(),
                LimitedAppendixV1::Embercleave,
            )
        };
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

    fn wire(&mut self, command: CommandV1) -> Body {
        self.serial += 1;
        body(&mut self.wire, self.serial, command)
    }

    fn act(&mut self, d: &KaitoDecision, chosen: usize) -> CommandV1 {
        let chosen = u32::try_from(chosen).unwrap();
        let backend = d.fixture_backend_v75();
        let expected = backend.expected();
        let Body::State { state, .. } = self.wire(CommandV1::Current) else {
            panic!()
        };
        let State::Decision {
            episode_id,
            authority_generation,
            decision: visible,
        } = *state
        else {
            panic!("wire lacks Embercleave menu: {state:?}")
        };
        let (projected, indices) =
            project_limited_kaito_v16(backend, expected.acting_player, visible.prompt_seq).unwrap();
        assert_eq!(*visible, projected);
        let action_index = indices.iter().position(|&index| index == chosen).unwrap() as u32;
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
                        action_index,
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
            actor: visible.actor,
            prompt_seq: visible.prompt_seq,
            action_index,
        };
        assert!(matches!(
            self.wire(command.clone()),
            Body::State {
                receipt: Some(_),
                ..
            }
        ));
        let original = backend.fixture_original_index_v75(chosen).unwrap();
        let result = self
            .raw
            .step(expected.episode_id, expected.step, original)
            .unwrap();
        assert_eq!(self.numeric.apply(d, chosen).unwrap(), result);
        assert_eq!(self.human.current_response(), result);
        assert_eq!(self.raw.kaito_fixture_state(), self.numeric.fixture_state());
        command
    }

    fn select(
        &mut self,
        predicate: impl Fn(&crate::kaito_candidates_v75::CandidateMeaningV75) -> bool,
    ) {
        let d = decision(&self.numeric);
        let index = d
            .fixture_backend_v75()
            .candidates_v75()
            .candidates
            .iter()
            .position(predicate)
            .unwrap();
        self.act(&d, index);
    }

    fn settle(&mut self, source: ObjectId, host: ObjectId) {
        for _ in 0..35 {
            if self
                .numeric
                .fixture_state()
                .objects
                .get(source)
                .v4
                .attached_to
                .is_some_and(|link| link.object == host)
            {
                return;
            }
            self.select(|row| {
                matches!(
                    row.action,
                    Action::Pass { .. }
                        | Action::OrderTriggers { .. }
                        | Action::ChooseTriggerOrderNext { .. }
                )
            });
        }
        panic!("paid Embercleave attachment did not complete");
    }
}

#[test]
fn paid_entry_and_equip_preserve_hidden_pairs_original_indices_and_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let make = |hidden| {
            let mut state = crate::graveyard_return_public_v61::tests::ready(actor);
            put(&mut state, actor.opponent(), hidden, Zone::Library);
            let (state, source, host, alternate) =
                crate::embercleave_public_v65::tests::waiting_from(actor, state);
            (
                Owners::fixture(state, actor.into()),
                source,
                host,
                alternate,
            )
        };
        let (mut a, source, host, alternate) = make("Forest");
        let (mut b, _, _, _) = make("Counterspell");
        assert_ne!(
            a.numeric.fixture_state().state_hash(),
            b.numeric.fixture_state().state_hash()
        );
        let original = decision(&a.numeric);
        assert_eq!(original.packet(), decision(&b.numeric).packet());
        assert_eq!(a.wire(CommandV1::Current), b.wire(CommandV1::Current));
        assert!(b.numeric.training_input_v1(&original).is_err());
        assert!(b.numeric.restore(&a.numeric.snapshot()).is_err());

        let expected = original.fixture_backend_v75().expected();
        let prompt = a.projector.project_current(&a.human, expected).unwrap();
        let human_save = a.human.snapshot_v75();
        let numeric_save = a.numeric.snapshot();
        let raw_save = a.raw.snapshot_v1();
        let Body::State {
            snapshot_id: Some(snapshot_id),
            ..
        } = a.wire(CommandV1::Snapshot)
        else {
            panic!("Embercleave wire snapshot absent")
        };
        let choose_host = |row: &crate::kaito_candidates_v75::CandidateMeaningV75| {
            matches!(
                row.embercleave_action,
                Some(ActionV1::AttachTarget {
                    kind: AbilityKindV65::EntryAttach,
                    ..
                })
            ) && matches!(&row.action, Action::ChooseTarget { target: TargetRefV1::Object { object }, .. }
                    if object.arena_id == host.0)
        };
        let index = original
            .fixture_backend_v75()
            .candidates_v75()
            .candidates
            .iter()
            .position(choose_host)
            .unwrap();
        let stale = a.act(&original, index);
        b.select(choose_host);
        a.settle(source, host);
        b.settle(source, host);
        assert_eq!(decision(&a.numeric).packet(), decision(&b.numeric).packet());

        for owner in [&mut a, &mut b] {
            let before = tapped(owner.numeric.fixture_state(), actor);
            owner.select(|row| {
                row.embercleave_action
                    == Some(ActionV1::ActivateEquip {
                        printed_generic_cost: 3,
                    })
            });
            owner.select(|row| matches!(row.embercleave_action,
                Some(ActionV1::AttachTarget { kind: AbilityKindV65::Equip, .. }))
                && matches!(&row.action, Action::ChooseTarget { target: TargetRefV1::Object { object }, .. }
                    if object.arena_id == alternate.0));
            owner.settle(source, alternate);
            let state = owner.numeric.fixture_state();
            assert_eq!(tapped(state, actor) - before, 3);
            assert!(!crate::engine::has_effective_keyword(
                state,
                host,
                Keywords::DOUBLE_STRIKE
            ));
            assert!(crate::engine::has_effective_keyword(
                state,
                alternate,
                Keywords::DOUBLE_STRIKE
            ));
            let after = decision(&owner.numeric);
            let input = owner.numeric.training_input_v1(&after).unwrap();
            assert!(ModelV1::deterministic_fixture()
                .forward(&input)
                .unwrap()
                .iter()
                .all(|x| x.is_finite()));
        }

        let CommandV1::Step { action_index, .. } = stale else {
            unreachable!()
        };
        a.raw.kaito_restore_v75(&raw_save).unwrap();
        a.numeric.restore(&numeric_save).unwrap();
        a.human.restore_v75(&human_save).unwrap();
        assert_eq!(a.raw.kaito_fixture_state(), a.numeric.fixture_state());
        assert_eq!(decision(&a.numeric).packet(), original.packet());
        assert!(a.numeric.training_input_v1(&original).is_err());
        assert!(a.numeric.apply(&original, index as u32).is_err());
        assert!(a
            .projector
            .submit(
                &mut a.human,
                HumanActionRequestV1 {
                    prompt_seq: prompt.prompt_seq,
                    action_index,
                }
            )
            .is_err());
        assert!(matches!(
            a.wire(CommandV1::Restore { snapshot_id }),
            Body::State { .. }
        ));
        assert!(matches!(a.wire(stale), Body::Error { .. }));
    }
}
