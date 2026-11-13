use super::*;
use crate::{
    current_mixed_policy_v1::KaitoDecision,
    current_tree_learner_v1::{AdamConfigV1, LearnerV1, ModelV1, TrainingExampleV1},
    environment_randomization_v2 as env2,
    human_bo3_v1::{project_limited_kaito_v16, HumanActionRequestV1, HumanKaitoProjectorV75},
    ids::ObjectId,
    limited_kaito_session_v16::{LimitedKaitoBodyV16 as Body, LimitedKaitoStateV16 as State},
    mana::ManaColor,
    policy_observation_v6::tests::put,
    policy_surface_v5::LimitedAppendixV1,
    rl::ActionSemanticV1 as A,
    rl_session::{FastActorResponseV1, KaitoSessionV75},
};

fn card(name: &str) -> u16 {
    crate::card_def::card_id_by_name(name).unwrap()
}

fn config(actor: PlayerId) -> ResetV1 {
    let seed = 550_463;
    let forest = card("Forest");
    let owner = if actor == PlayerId::P0 {
        env2::PhysicalOwnerV2::P0
    } else {
        env2::PhysicalOwnerV2::P1
    };
    let shuffle = env2::derive_environment_randomization_seed_v2(
        seed,
        owner,
        env2::ShufflePurposeV2::InitialLibraryShuffle,
        0,
    )
    .unwrap();
    let slots = env2::permutation_v2(shuffle, &(0..40).collect::<Vec<u16>>());
    let mut mainboards = [vec![forest; 40], vec![forest; 40]];
    // Pin three cards in the true seven-card opening draw using the real
    // environment shuffle. No game state or payment is modified after reset.
    for (slot, name) in
        [0, 1, 2]
            .into_iter()
            .zip(["Secluded Courtyard", "Banner of Kinship", "Llanowar Elves"])
    {
        mainboards[actor.index()][usize::from(slots[slot])] = card(name);
    }
    ResetV1 {
        expected_card_db_hash: KERNEL_CARDDB_HASH,
        episode_id: 550,
        env_seed: seed,
        starting_player: actor.into(),
        deck_ids: ["chosen-type-left".into(), "chosen-type-right".into()],
        mainboards,
        max_physical_decisions: 1000,
        max_policy_steps: 1000,
    }
}

fn decision(session: &KaitoSession) -> KaitoDecision {
    let FastActorResponseV1::Decision(expected) = session.current_response() else {
        panic!(
            "chosen-type decision absent: {:?}",
            session.current_response()
        );
    };
    session.encode(expected).unwrap()
}

fn request(server: &mut JsonlServerV1, id: &str, command: CommandV1) -> Body {
    let reply = server.handle_request(RequestV1 {
        profile: PROFILE_V1.into(),
        request_id: id.into(),
        command,
    });
    assert_eq!(reply.profile, PROFILE_V1);
    let ReplyBodyV1::State { payload } = reply.body else {
        panic!("chosen-type wire refused: {reply:?}");
    };
    payload.body
}

#[test]
fn exact_forty_card_resets_reach_paid_banner_and_restricted_courtyard_both_seats() {
    let courtyard = card("Secluded Courtyard");
    let banner = card("Banner of Kinship");
    let elf = card("Llanowar Elves");
    for actor in [PlayerId::P0, PlayerId::P1] {
        let config = config(actor);
        assert!(crate::artist_consumer_v1::reset_numeric_v1(&config).is_err());
        assert!(crate::limited_return_consumer_v1::reset_numeric_v1(&config).is_err());
        assert!(KaitoSession::reset_explicit_mainboards(
            config.episode_id,
            config.env_seed,
            (100, 100),
            config.deck_ids.clone(),
            config.mainboards.clone(),
            actor,
        )
        .is_err());
        let mut wrong = config.clone();
        wrong.mainboards[actor.index()][0] = 460;
        assert!(reset_numeric_v1(&wrong).is_err());
        let mut wire = JsonlServerV1::new();
        let Body::State { state, .. } = request(
            &mut wire,
            "reset",
            CommandV1::Reset {
                config: config.clone(),
            },
        ) else {
            panic!("wire reset failed");
        };
        assert!(matches!(*state, State::Decision { ref decision, .. }
            if decision.schema == "mtg-kernel-limited-chosen-type-decision/v1"));
        let mut owner = reset_numeric_reveal_facts_v3(&config).unwrap();
        let opening = decision(&owner);
        let expected = opening.fixture_backend_v75().expected();
        let human = reset_human_v1(&config, expected.acting_player).unwrap();
        let prompt = HumanKaitoProjectorV75::new(expected.acting_player)
            .project_current(&human, expected)
            .unwrap();
        assert_eq!(prompt.schema, "mtg-kernel-human-chosen-type-decision/v1");
        assert!(owner.fixture_state().reveal_knowledge_v4.is_some());
        let mut floated = false;
        let mut paid_banner = false;
        let mut finished = false;
        for _ in 0..400 {
            let d = decision(&owner);
            let backend = d.fixture_backend_v75();
            let facts = backend.observation().chosen_type.as_ref().unwrap();
            if let Some(entered) = facts
                .native
                .permanents
                .iter()
                .find(|row| row.family == FamilyV66::BannerOfKinship)
            {
                assert_eq!(entered.chosen, CreatureTypeV60::Elf);
                assert_eq!(entered.current_fellowship, 1);
                assert!(entered
                    .current_controlled_creature_bonuses
                    .iter()
                    .any(|boost| boost.power == 1 && boost.toughness == 1));
                finished = true;
                break;
            }
            let actions = backend.actions();
            let selected = actions
                .iter()
                .position(|a| match a {
                    A::ChooseEntryCreatureTypeV60 { creature_type, .. } => {
                        *creature_type == CreatureTypeV60::Elf
                    }
                    A::PlayLand { source, .. } => source.card_db_id == courtyard,
                    A::ActivateManaAbility {
                        source,
                        mana_choice: Some(ManaColor::G),
                        ..
                    } => source.card_db_id == courtyard && !floated,
                    A::CastSpell { source, .. } => {
                        source.card_db_id == banner || (source.card_db_id == elf && floated)
                    }
                    _ => false,
                })
                .or_else(|| actions.iter().position(|a| matches!(a, A::PlayLand { .. })))
                .or_else(|| {
                    actions.iter().position(|a| {
                        matches!(
                            a,
                            A::ChooseLondonMulligan {
                                mulligan: false,
                                ..
                            } | A::Pass { .. }
                                | A::ChooseAttackerInclusion { include: false, .. }
                                | A::ChooseBlockerInclusion { include: false, .. }
                                | A::Discard { .. }
                        )
                    })
                })
                .expect("unhandled reset decision");
            let action = actions[selected].clone();
            owner.apply(&d, selected as u32).unwrap();
            match action {
                A::ActivateManaAbility { source, .. } if source.card_db_id == courtyard => {
                    let d = decision(&owner);
                    let facts = d
                        .fixture_backend_v75()
                        .observation()
                        .chosen_type
                        .as_ref()
                        .unwrap();
                    let [unit] = facts.native.floating_chosen_type_mana.as_slice() else {
                        panic!("Courtyard payment did not float one exact unit");
                    };
                    assert_eq!(
                        (unit.color, unit.chosen),
                        (ManaColor::G, CreatureTypeV60::Elf)
                    );
                    floated = true;
                }
                A::CastSpell { source, .. } if source.card_db_id == banner => {
                    let world = owner.fixture_state();
                    assert_eq!(
                        world.objects.get(ObjectId(source.arena_id)).zone,
                        Zone::Stack
                    );
                    assert!(
                        world.players[actor.index()]
                            .battlefield
                            .iter()
                            .filter(|&&id| world.objects.get(id).tapped)
                            .count()
                            >= 5
                    );
                    paid_banner = true;
                }
                _ => {}
            }
        }
        assert!(floated && paid_banner && finished);
    }
}

#[test]
fn genuine_courtyard_choices_preserve_private_world_and_all_consumer_index_bindings() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let scene =
            |hidden, hand| {
                let mut state = crate::call_observation_v62::tests::ready(actor, hidden);
                put(&mut state, actor.opponent(), hand, Zone::Hand);
                let courtyard = put(&mut state, actor, "Secluded Courtyard", Zone::Hand);
                let mut raw = FastActorSessionV1::from_kaito_v75_fixture_state_scoped(
                    state,
                    LimitedAppendixV1::ChosenType,
                );
                let FastActorResponseV1::Decision(expected) = raw.current_response() else {
                    panic!("initial fixture menu absent");
                };
                let play = raw.diagnostic_current_action_semantics().unwrap().iter().position(|a| {
                matches!(a, A::PlayLand { source, .. } if source.arena_id == courtyard.0)
            }).unwrap();
                raw.step(expected.episode_id, expected.step, play as u32)
                    .unwrap();
                (raw, courtyard)
            };
        let (mut raw, courtyard) = scene("Forest", "Forest");
        let (other, _) = scene("Island", "Banner of Kinship");
        let mut human = HumanKaitoSessionV75::from_session(raw.clone(), actor.into()).unwrap();
        let mut projector = HumanKaitoProjectorV75::new(actor.into());
        let mut wire = JsonlServerV1::new();
        wire.0
            .install_fixture_core(KaitoSessionV75::from_session(raw.clone()).unwrap());
        let mut numeric = KaitoSession::from_session(raw.clone()).unwrap();
        let other = KaitoSession::from_session(other).unwrap();
        let before = decision(&numeric);
        let alternate = decision(&other);
        assert_eq!(before.packet(), alternate.packet());
        let first = numeric.training_input_v1(&before).unwrap();
        let second = other.training_input_v1(&alternate).unwrap();
        let model = ModelV1::deterministic_fixture();
        let logits = model.forward(&first).unwrap();
        assert_eq!(logits, model.forward(&second).unwrap());
        assert!(logits.iter().all(|value| value.is_finite()));
        let selected = usize::from(CreatureTypeV60::Elf.choice_index());
        assert!(logits.iter().any(|value| *value != logits[selected]));
        let mut target = vec![0.; logits.len()];
        target[selected] = 1.;
        let mut learner = LearnerV1::new(
            model,
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
                input: &first,
                targets: &target
            }])
            .unwrap()
            .is_finite());
        assert!(numeric.sampled_clone(&before, 550).is_err());
        let saved = numeric.snapshot();
        let backend = before.fixture_backend_v75();
        let expected = backend.expected();
        let Body::State { state, .. } = request(&mut wire, "current", CommandV1::Current) else {
            panic!("wire lacks state");
        };
        let State::Decision {
            episode_id,
            authority_generation,
            decision: visible,
        } = *state
        else {
            panic!("wire lacks type menu");
        };
        let (projection, indices) =
            project_limited_kaito_v16(backend, actor.into(), visible.prompt_seq).unwrap();
        assert_eq!(*visible, projection);
        let (paired, paired_indices) = project_limited_kaito_v16(
            alternate.fixture_backend_v75(),
            actor.into(),
            visible.prompt_seq,
        )
        .unwrap();
        assert_eq!(paired, projection);
        assert_eq!(paired_indices, indices);
        let index = indices
            .iter()
            .position(|&index| index == selected as u32)
            .unwrap();
        let prompt = projector.project_current(&human, expected).unwrap();
        assert_eq!(prompt.actions, visible.actions);
        assert_eq!(prompt.action_meanings, visible.action_meanings);
        assert_eq!(prompt.state, visible.state);
        assert_eq!(
            prompt.action_meanings[index]
                .chosen_type_answer
                .unwrap()
                .creature_type,
            CreatureTypeV60::Elf
        );
        projector
            .submit(
                &mut human,
                HumanActionRequestV1 {
                    prompt_seq: prompt.prompt_seq,
                    action_index: index as u32,
                },
            )
            .unwrap();
        assert!(matches!(
            request(
                &mut wire,
                "choose-elf",
                CommandV1::Step {
                    episode_id,
                    authority_generation,
                    actor: actor.into(),
                    prompt_seq: visible.prompt_seq,
                    action_index: index as u32,
                }
            ),
            Body::State {
                receipt: Some(_),
                ..
            }
        ));
        let raw_index = backend.fixture_original_index_v75(selected as u32).unwrap();
        let actual = raw
            .step(expected.episode_id, expected.step, raw_index)
            .unwrap();
        assert_eq!(numeric.apply(&before, selected as u32).unwrap(), actual);
        assert_eq!(human.current_response(), actual);
        assert_eq!(numeric.fixture_state(), raw.kaito_fixture_state());
        let entered = numeric.fixture_state().objects.get(courtyard);
        assert_eq!(
            (entered.zone, entered.v4.kinship_type_v1),
            (Zone::Battlefield, Some(CreatureTypeV60::Elf))
        );
        numeric.restore(&saved).unwrap();
        assert!(numeric.apply(&before, selected as u32).is_err());
        let restored = decision(&numeric);
        assert_eq!(restored.packet(), before.packet());
        numeric.apply(&restored, selected as u32).unwrap();
        assert_eq!(numeric.fixture_state(), raw.kaito_fixture_state());
    }
}
