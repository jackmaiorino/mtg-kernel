use super::*;
use crate::{
    current_mixed_policy_v1::KaitoDecision,
    current_tree_learner_v1::ModelV1,
    human_bo3_v1::{project_limited_kaito_v16, HumanActionRequestV1, HumanKaitoProjectorV75},
    ids::ObjectId,
    limited_kaito_session_v16::{LimitedKaitoBodyV16 as Body, LimitedKaitoStateV16 as State},
    policy_observation_v6::tests::put,
    rl::{ActionSemanticV1 as Action, TargetRefV1},
    rl_session::{FastActorResponseV1, KaitoSessionV75},
    standard_cards_v1::drakuseth_tests as native_paid,
    state::Target,
};

fn card(name: &str) -> u16 {
    crate::card_def::card_id_by_name(name).unwrap()
}

fn config(actor: PlayerId) -> ResetV1 {
    let mut deck = vec![PRINTING_V1; 4];
    deck.extend([card("Mountain"); 36]);
    let mut mainboards = [vec![card("Forest"); 40], vec![card("Forest"); 40]];
    mainboards[actor.index()] = deck;
    ResetV1 {
        expected_card_db_hash: KERNEL_CARDDB_HASH,
        episode_id: 550,
        env_seed: 550_464,
        starting_player: actor.into(),
        deck_ids: ["drakuseth-left".into(), "drakuseth-right".into()],
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
        panic!(
            "Drakuseth decision absent: {:?}",
            session.current_response()
        );
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
                crate::policy_surface_v5::LimitedAppendixV1::Drakuseth,
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
            panic!("wire lacks Drakuseth menu: {state:?}")
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
}

#[test]
fn declared_deck_paid_cast_attack_first_target_real_owner_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let config = config(actor);
        assert!(crate::limited_return_consumer_v1::reset_numeric_v1(&config).is_err());
        assert!(crate::artist_consumer_v1::reset_numeric_v1(&config).is_err());
        for printing in [460, 461, 465, u16::MAX] {
            let mut bad = config.clone();
            bad.mainboards[actor.index()][0] = printing;
            assert!(reset_numeric_v1(&bad).is_err());
        }
        let mut wire = JsonlServerV1::new();
        let line = serde_json::to_string(&req(
            "reset",
            CommandV1::Reset {
                config: config.clone(),
            },
        ))
        .unwrap();
        let value: serde_json::Value = serde_json::from_str(&wire.handle_line(&line)).unwrap();
        assert_eq!(
            value["payload"]["state"]["decision"]["schema"],
            "mtg-kernel-limited-drakuseth-decision/v1"
        );
        let mut owner = reset_numeric_reveal_facts_v3(&config).unwrap();
        assert!(owner.fixture_state().reveal_knowledge_v4.is_some());
        let opening = decision(&owner);
        let expected = opening.fixture_backend_v75().expected();
        let human = reset_human_v1(&config, expected.acting_player).unwrap();
        assert_eq!(
            HumanKaitoProjectorV75::new(expected.acting_player)
                .project_current(&human, expected)
                .unwrap()
                .schema,
            "mtg-kernel-human-drakuseth-decision/v1"
        );
        let mut paid = None;
        let mut selected = false;
        let mut completed = false;
        for _ in 0..700 {
            if let Some(source) = paid {
                if !native_paid::damage_since(owner.fixture_state(), 0, source).is_empty() {
                    completed = true;
                    break;
                }
            }
            let d = decision(&owner);
            let backend = d.fixture_backend_v75();
            let actions = backend.actions();
            let index=actions.iter().position(|a|matches!(a,Action::CastSpell {source,..} if source.card_db_id==PRINTING_V1 && paid.is_none()))
                .or_else(|| actions.iter().position(|a|matches!(a,Action::ChooseTarget {target:TargetRefV1::Player {player},..} if *player==actor.opponent().into() && !selected)))
                .or_else(||actions.iter().position(|a|matches!(a,Action::FinishTargetSelection {..})))
                .or_else(||actions.iter().position(|a|matches!(a,Action::PlayLand {..}|Action::ChooseAttackerInclusion {include:true,..})))
                .or_else(||actions.iter().position(|a|matches!(a,Action::ChooseLondonMulligan {mulligan:false,..}|Action::ChooseBlockerInclusion {include:false,..}|Action::Pass {..}|Action::Discard {..}|Action::OrderTriggers {..}|Action::ChooseTriggerOrderNext {..})))
                .unwrap_or_else(||panic!("unhandled declared Drakuseth menu: {actions:?}"));
            let casting = match &actions[index] {
                Action::CastSpell { source, .. } => Some(ObjectId(source.arena_id)),
                _ => None,
            };
            if matches!(actions[index], Action::ChooseTarget { .. }) {
                assert_eq!(
                    backend.candidates_v75().candidates[index].drakuseth_target,
                    Some(TargetActionV1::Choose {
                        original_slot: 0,
                        printed_damage: 4
                    })
                );
                selected = true;
            }
            let tapped = |s: &GameState| {
                s.players[actor.index()]
                    .battlefield
                    .iter()
                    .filter(|&&id| s.objects.get(id).tapped)
                    .count()
            };
            let before = tapped(owner.fixture_state());
            owner.apply(&d, index as u32).unwrap();
            if let Some(source) = casting {
                assert_eq!(owner.fixture_state().objects.get(source).zone, Zone::Stack);
                assert_eq!(tapped(owner.fixture_state()) - before, 7);
                paid = Some(source);
            }
        }
        assert!(
            selected && completed,
            "declared deck did not complete genuine paid attack damage"
        );
        assert_eq!(
            native_paid::damage_since(owner.fixture_state(), 0, paid.unwrap())[0],
            (Target::Player(actor.opponent()), 4)
        );
    }
}

fn quiet(owner: &mut Owners) {
    let d = decision(&owner.numeric);
    let i = d
        .fixture_backend_v75()
        .actions()
        .iter()
        .position(|a| {
            matches!(
                a,
                Action::Pass { .. }
                    | Action::OrderTriggers { .. }
                    | Action::ChooseTriggerOrderNext { .. }
            )
        })
        .expect("unhandled pending Drakuseth decision");
    owner.apply(&d, i);
}
#[test]
fn paid_target_slots_native_human_wire_parity_and_authenticated_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for count in 1..=3 {
            let (mut s, source) = native_paid::paid(actor);
            let other = [
                put(&mut s, actor.opponent(), "Serra Angel", Zone::Battlefield),
                put(&mut s, actor.opponent(), "Serra Angel", Zone::Battlefield),
            ];
            native_paid::attack(&mut s, source, actor);
            let start = s.engine.event_history.len();
            let mut owners = Owners::fixture(s, actor.into());
            let targets = [
                Target::Player(actor.opponent()),
                Target::Object(other[0]),
                Target::Object(other[1]),
            ];
            for (slot, target) in targets[..count].iter().enumerate() {
                let d = decision(&owners.numeric);
                let backend = d.fixture_backend_v75();
                let i = backend
                    .actions()
                    .iter()
                    .position(|a| match (a, target) {
                        (
                            Action::ChooseTarget {
                                target: TargetRefV1::Player { player },
                                ..
                            },
                            Target::Player(p),
                        ) => *player == (*p).into(),
                        (
                            Action::ChooseTarget {
                                target: TargetRefV1::Object { object },
                                ..
                            },
                            Target::Object(o),
                        ) => object.arena_id == o.0,
                        _ => false,
                    })
                    .unwrap();
                assert_eq!(
                    backend.candidates_v75().candidates[i].drakuseth_target,
                    Some(TargetActionV1::Choose {
                        original_slot: slot as u32,
                        printed_damage: if slot == 0 { 4 } else { 3 }
                    })
                );
                assert_eq!(
                    backend
                        .actions()
                        .iter()
                        .any(|a| matches!(a, Action::FinishTargetSelection { .. })),
                    slot > 0
                );
                let before = owners.numeric.snapshot();
                let state = owners.numeric.fixture_state().clone();
                let foreign = Owners::fixture(state.clone(), actor.into());
                assert!(owners.numeric.restore(&foreign.numeric.snapshot()).is_err());
                let input = owners.numeric.training_input_v1(&d).unwrap();
                assert!(ModelV1::deterministic_fixture()
                    .forward(&input)
                    .unwrap()
                    .iter()
                    .all(|x| x.is_finite()));
                owners.numeric.apply(&d, i as u32).unwrap();
                owners.numeric.restore(&before).unwrap();
                assert_eq!(owners.numeric.fixture_state(), &state);
                assert_eq!(decision(&owners.numeric).packet(), d.packet());
                assert!(owners.numeric.apply(&d, i as u32).is_err());
                assert!(owners.numeric.training_input_v1(&d).is_err());
                // Recreate synchronized owners after the independent stale-token check.
                owners = Owners::fixture(state, actor.into());
                let fresh = decision(&owners.numeric);
                owners.apply(&fresh, i);
                if slot + 1 < 3 {
                    assert!(!decision(&owners.numeric)
                        .fixture_backend_v75()
                        .actions()
                        .iter()
                        .any(|a| match (a, target) {
                            (
                                Action::ChooseTarget {
                                    target: TargetRefV1::Player { player },
                                    ..
                                },
                                Target::Player(p),
                            ) => *player == (*p).into(),
                            (
                                Action::ChooseTarget {
                                    target: TargetRefV1::Object { object },
                                    ..
                                },
                                Target::Object(o),
                            ) => object.arena_id == o.0,
                            _ => false,
                        }));
                }
            }
            if count < 3 {
                let d = decision(&owners.numeric);
                let i = d
                    .fixture_backend_v75()
                    .actions()
                    .iter()
                    .position(|a| matches!(a, Action::FinishTargetSelection { .. }))
                    .unwrap();
                owners.apply(&d, i);
            }
            for _ in 0..24 {
                if !native_paid::damage_since(owners.numeric.fixture_state(), start, source)
                    .is_empty()
                {
                    break;
                }
                quiet(&mut owners);
            }
            assert_eq!(
                native_paid::damage_since(owners.numeric.fixture_state(), start, source),
                targets[..count]
                    .iter()
                    .enumerate()
                    .map(|(i, t)| (*t, if i == 0 { 4 } else { 3 }))
                    .collect::<Vec<_>>()
            );
        }
    }
}
#[test]
fn hidden_composition_and_native_allocation_have_equal_numeric_and_human_target_views_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let make = |early: bool| {
            let (mut s, source) = native_paid::paid_with_hidden(actor, early.then_some("Forest"));
            if !early {
                put(&mut s, actor.opponent(), "Island", Zone::Hand);
            }
            native_paid::attack(&mut s, source, actor);
            Owners::fixture(s, actor.into())
        };
        let mut a = make(true);
        let mut b = make(false);
        assert_ne!(
            a.numeric.fixture_state().engine.combat.attackers,
            b.numeric.fixture_state().engine.combat.attackers
        );
        let da = decision(&a.numeric);
        let db = decision(&b.numeric);
        assert_eq!(da.packet(), db.packet());
        assert_eq!(a.request(CommandV1::Current), b.request(CommandV1::Current));
        let model = ModelV1::deterministic_fixture();
        assert_eq!(
            model
                .forward(&a.numeric.training_input_v1(&da).unwrap())
                .unwrap(),
            model
                .forward(&b.numeric.training_input_v1(&db).unwrap())
                .unwrap()
        );
    }
}
#[test]
fn mixed_player_and_permanent_damage_choices_preserve_batch_lki_and_consumer_bindings_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for form in 0..=2 {
            let (s, source, _, start) = native_paid::mixed_prevention_scene(actor, form);
            let mut owners = Owners::fixture(s, actor.into());
            let mut saw_player = false;
            let mut saw_permanent = false;
            for _ in 0..64 {
                if !native_paid::damage_since(owners.numeric.fixture_state(), start, source)
                    .is_empty()
                {
                    break;
                }
                let d = decision(&owners.numeric);
                let backend = d.fixture_backend_v75();
                let facts = backend.observation().drakuseth.as_ref().unwrap();
                if let Some(menu) = &facts.pending_damage {
                    saw_player |= menu.active_original_slot == 0;
                    saw_permanent |= menu.active_original_slot > 0;
                    assert!(native_paid::damage_since(
                        owners.numeric.fixture_state(),
                        start,
                        source
                    )
                    .is_empty());
                    assert_eq!(menu.chooser, backend.expected().acting_player);
                    assert_eq!(menu.rows.len(), 3);
                    let i = backend
                        .actions()
                        .iter()
                        .position(|a| {
                            matches!(
                                a,
                                Action::ChooseEffectOption {
                                    option_index: 0,
                                    ..
                                }
                            )
                        })
                        .unwrap();
                    assert_eq!(menu.options[0].menu_index, 0);
                    owners.apply(&d, i);
                } else {
                    quiet(&mut owners)
                }
            }
            assert!(saw_player && saw_permanent);
            assert!(
                !native_paid::damage_since(owners.numeric.fixture_state(), start, source)
                    .is_empty()
            );
            assert!(owners
                .numeric
                .fixture_state()
                .engine
                .pending_effect
                .is_none());
        }
    }
}

#[test]
fn original_target_illegality_preserves_damage_slots_and_owner_restore_revokes_all_views_both_seats(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut s, source) = native_paid::paid(actor);
        let target = put(&mut s, actor.opponent(), "Serra Angel", Zone::Battlefield);
        native_paid::attack(&mut s, source, actor);
        let mut owners = Owners::fixture(s, actor.into());
        let d = decision(&owners.numeric);
        let expected = d.fixture_backend_v75().expected();
        let i=d.fixture_backend_v75().actions().iter().position(|a|matches!(a,Action::ChooseTarget {target:TargetRefV1::Object {object},..} if object.arena_id==target.0)).unwrap();
        let saved = owners.numeric.snapshot();
        let human_saved = owners.human.snapshot_v75();
        let prompt = owners
            .projector
            .project_current(&owners.human, expected)
            .unwrap();
        let Body::State {
            snapshot_id: Some(snapshot),
            ..
        } = owners.request(CommandV1::Snapshot)
        else {
            panic!("wire snapshot absent")
        };
        let stale = owners.apply(&d, i);
        let CommandV1::Step { action_index, .. } = stale else {
            unreachable!()
        };
        owners.numeric.restore(&saved).unwrap();
        owners.human.restore_v75(&human_saved).unwrap();
        assert!(matches!(
            owners.request(CommandV1::Restore {
                snapshot_id: snapshot
            }),
            Body::State { .. }
        ));
        assert_eq!(decision(&owners.numeric).packet(), d.packet());
        assert!(owners.numeric.apply(&d, i as u32).is_err());
        assert!(owners
            .projector
            .submit(
                &mut owners.human,
                HumanActionRequestV1 {
                    prompt_seq: prompt.prompt_seq,
                    action_index
                }
            )
            .is_err());
        assert!(matches!(owners.request(stale), Body::Error { .. }));
        let mut s = owners.numeric.fixture_state().clone();
        native_paid::choose(
            &mut s,
            source,
            &[Target::Object(target), Target::Player(actor.opponent())],
        );
        // Labelled native departure/reentry operand following a genuine paid
        // cast, attack and target selection. This is not a paid bounce claim.
        crate::event::propose_and_commit(
            &mut s,
            crate::event::ProposedEvent::zone_change(target, Zone::Hand),
        );
        crate::event::propose_and_commit(
            &mut s,
            crate::event::ProposedEvent::zone_change(target, Zone::Battlefield),
        );
        let start = s.engine.event_history.len();
        let mut owners = Owners::fixture(s, actor.into());
        let d = decision(&owners.numeric);
        let crate::fdn_family_source_v76::LaterFamilyV76::Drakuseth(f) = &d
            .fixture_backend_v75()
            .observation()
            .drakuseth
            .as_ref()
            .unwrap()
            .source
            .families[0];
        let trigger = f
            .triggers
            .iter()
            .find(|r| r.stage == crate::fdn_family_source_v76::StageV76::Published)
            .unwrap();
        assert!(!trigger.original_target_slots[0].currently_legal);
        assert_eq!(trigger.original_target_slots[0].original.printed_damage, 4);
        assert_eq!(trigger.original_target_slots[1].original.printed_damage, 3);
        for _ in 0..24 {
            if !native_paid::damage_since(owners.numeric.fixture_state(), start, source).is_empty()
            {
                break;
            }
            quiet(&mut owners);
        }
        assert_eq!(
            native_paid::damage_since(owners.numeric.fixture_state(), start, source),
            vec![(Target::Player(actor.opponent()), 3)]
        );
    }
}
