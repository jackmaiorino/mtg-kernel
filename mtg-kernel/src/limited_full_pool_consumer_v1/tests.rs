use super::*;
use crate::{
    current_mixed_policy_v1::KaitoDecision,
    current_tree_learner_v1::ModelV1,
    effect::{
        standard_exile::etali_primal_storm_tests as etali, thousand_year_storm_v1_tests as storm,
    },
    engine::{Action as NativeAction, Decision as NativeDecision},
    human_bo3_v1::{project_limited_kaito_v16, HumanActionRequestV1, HumanKaitoProjectorV75},
    ids::PlayerId,
    limited_kaito_session_v16::{LimitedKaitoBodyV16 as Body, LimitedKaitoStateV16 as WireState},
    policy_observation_v6::tests::{advance_to_next_main_for, put},
    policy_surface_v5::LimitedAppendixV1,
    rl::{ActionSemanticV1 as Action, TargetRefV1},
    rl_session::{FastActorResponseV1, KaitoSessionV75},
    state::{Target, Zone},
};

#[path = "elf_lords.rs"]
mod elf_lords;
#[path = "equipment.rs"]
mod equipment;

fn card(name: &str) -> u16 {
    card_id_by_name(name).unwrap()
}

fn decision(owner: &KaitoSession) -> KaitoDecision {
    let FastActorResponseV1::Decision(expected) = owner.current_response() else {
        panic!("full-pool decision absent: {:?}", owner.current_response());
    };
    owner.encode(expected).unwrap()
}

fn raw(state: &GameState) -> FastActorSessionV1 {
    FastActorSessionV1::from_kaito_v75_fixture_state_scoped(
        state.clone(),
        LimitedAppendixV1::FullPool,
    )
}

fn wire_command(wire: &mut JsonlServerV1, serial: u64, command: CommandV1) -> ReplyBodyV1 {
    let reply = wire.handle_request(RequestV1 {
        profile: PROFILE_V1.into(),
        request_id: serial.to_string(),
        command,
    });
    assert_eq!(reply.profile, PROFILE_V1);
    assert_eq!(reply.producer_sha256, producer_sha256_v1());
    reply.body
}

fn wire_state(body: ReplyBodyV1) -> Body {
    let ReplyBodyV1::State { payload } = body else {
        panic!("full-pool wire refused: {body:?}");
    };
    payload.body
}

fn assert_model(owner: &KaitoSession, d: &KaitoDecision) {
    let input = owner.training_input_v1(d).unwrap();
    let logits = ModelV1::deterministic_fixture().forward(&input).unwrap();
    assert_eq!(logits.len(), d.packet().actions().len());
    assert!(logits.iter().all(|score| score.is_finite()));
}

#[test]
fn frozen_286_declared_reset_all_appendices_and_distinct_sealed_profile_both_seats() {
    let deck: Vec<_> = declared_printings_v1().unwrap().iter().copied().collect();
    assert_eq!(deck.len(), 286);
    for first in [PlayerSeatV1::P0, PlayerSeatV1::P1] {
        let config = ResetV1 {
            expected_card_db_hash: KERNEL_CARDDB_HASH,
            episode_id: 550,
            env_seed: 550_286,
            starting_player: first,
            deck_ids: ["frozen-left".into(), "frozen-right".into()],
            mainboards: [deck.clone(), deck.clone()],
            max_physical_decisions: 2_000,
            max_policy_steps: 2_000,
        };
        assert!(crate::limited_mixed_consumer_v1::reset_numeric_v1(&config).is_err());
        let mut outside = config.clone();
        assert!(!deck.contains(&card("Lightning Bolt")));
        outside.mainboards[0][0] = card("Lightning Bolt");
        assert!(reset_numeric_v1(&outside).is_err());

        let mut owner = reset_numeric_reveal_facts_v3(&config).unwrap();
        assert!(owner.fixture_state().reveal_knowledge_v4.is_some());
        require_native_modes(owner.fixture_state()).unwrap();
        let d = decision(&owner);
        let backend = d.fixture_backend_v75();
        let expected = backend.expected();
        assert_eq!(backend.observation().schema, OBSERVATION_SCHEMA);
        assert_eq!(backend.candidates_v75().schema, CANDIDATES_SCHEMA);
        assert!(backend
            .candidates_v75()
            .candidates
            .iter()
            .all(|row| row.schema == ACTION_SCHEMA));
        assert_model(&owner, &d);

        let human = reset_human_v1(&config, expected.acting_player).unwrap();
        let prompt = HumanKaitoProjectorV75::new(expected.acting_player)
            .project_current(&human, expected)
            .unwrap();
        assert_eq!(prompt.schema, HUMAN_DECISION_SCHEMA);
        let mut wire = JsonlServerV1::new();
        let Body::State { state, .. } = wire_state(wire_command(
            &mut wire,
            1,
            CommandV1::Reset {
                config: config.clone(),
            },
        )) else {
            panic!("full-pool reset did not reach its first decision");
        };
        let WireState::Decision {
            decision: visible, ..
        } = *state
        else {
            panic!("full-pool reset was terminal");
        };
        assert_eq!(visible.schema, WIRE_DECISION_SCHEMA);
        assert_eq!(prompt.actions, visible.actions);
        assert_eq!(prompt.action_meanings, visible.action_meanings);

        let mut missing = backend.observation().clone();
        missing.etali = None;
        missing.projection_sha256 = crate::kaito_observation_v75::commitment(&missing).unwrap();
        assert!(crate::kaito_observation_v75::validate_artifact(&missing).is_err());
        let mut retagged = backend.observation().clone();
        retagged.schema = crate::limited_mixed_consumer_v1::OBSERVATION_SCHEMA.into();
        retagged.projection_sha256 = crate::kaito_observation_v75::commitment(&retagged).unwrap();
        assert!(crate::kaito_observation_v75::validate_artifact(&retagged).is_err());

        let saved = owner.snapshot();
        owner.apply(&d, 0).unwrap();
        owner.restore(&saved).unwrap();
        assert_eq!(decision(&owner).packet(), d.packet());
        assert!(owner.apply(&d, 0).is_err());
        let mut foreign = reset_numeric_reveal_facts_v3(&config).unwrap();
        assert!(foreign.restore(&saved).is_err());
    }
}

fn paid_etali_storm(actor: PlayerId) -> (GameState, crate::ids::ObjectId) {
    let mut state = storm::ready(actor, "Island");
    crate::combat_damage_v1::enable_foundations_combat_v1(&mut state).unwrap();
    FastActorSessionV1::enable_limited_native_modes_v1(&mut state, LimitedAppendixV1::FullPool);
    storm::paid_storm(&mut state, actor);
    let source = storm::cast(&mut state, actor, "Etali, Primal Storm", &[], 6);
    storm::finish(&mut state);
    assert!(state.objects.get(source).summoning_sick);
    advance_to_next_main_for(&mut state, actor);
    // A genuine paid instant this turn supplies exactly one prior Storm cast.
    storm::cast(
        &mut state,
        actor,
        "Burst Lightning",
        &[Target::Player(actor.opponent())],
        1,
    );
    storm::finish(&mut state);

    // Seed the next authentic top-card batch with one nonland and one land.
    let hit = put(
        &mut state,
        actor.opponent(),
        "Burst Lightning",
        Zone::Library,
    );
    state.players[actor.opponent().index()]
        .library
        .rotate_right(1);
    for _ in 0..24 {
        match etali::next(&mut state) {
            NativeDecision::DeclareAttackers { player, eligible } if player == actor => {
                assert!(eligible.contains(&source));
                etali::answer(&mut state, NativeAction::DeclareAttackers(vec![source]));
                let choice = etali::batch(&mut state, actor);
                assert!(choice
                    .scans
                    .iter()
                    .any(|scan| scan.hit.is_some_and(|row| row.object == hit)));
                return (state, hit);
            }
            NativeDecision::CastSpellOrPass { .. } => etali::answer(&mut state, NativeAction::Pass),
            other => panic!("unexpected Etali attack progression {other:?}"),
        }
    }
    panic!("no actual Etali attack decision");
}

#[test]
fn paid_etali_free_cast_feeds_storm_and_both_owners_bind_the_actual_actions() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (state, hit) = paid_etali_storm(actor);
        let mut other = state.clone();
        let hidden = other.players[actor.opponent().index()].library[0];
        let object = other.objects.get_mut(hidden);
        object.card_def = card("Forest");
        object.name = "Forest".into();
        object.v4 = crate::state::ObjectStateV4::from_card_def(object.card_def);
        assert_ne!(state.state_hash(), other.state_hash());
        let mut disabled = state.clone();
        disabled.fiend_artisan_receipts_v1 = false;
        assert!(crate::kaito_observation_v75::validate_state_scoped(
            &disabled,
            LimitedAppendixV1::FullPool,
        )
        .is_err());

        let mut numeric = KaitoSession::from_session(raw(&state)).unwrap();
        let mut paired = KaitoSession::from_session(raw(&other)).unwrap();
        let opening = decision(&numeric);
        assert_eq!(opening.packet(), decision(&paired).packet());
        let candidate = |d: &KaitoDecision| {
            d.fixture_backend_v75()
                .candidates_v75()
                .candidates
                .iter()
                .position(|row| {
                    row.etali_action
                        == Some(crate::etali_primal_storm_consumer_v1::ActionV1::CastOriginalExile)
                        && matches!(&row.action, Action::ChooseEffectTarget {
                        target: TargetRefV1::Object { object }, ..
                    } if object.arena_id == hit.0)
                })
                .unwrap()
        };
        let selected = candidate(&opening) as u32;
        let raw_index = opening
            .fixture_backend_v75()
            .fixture_original_index_v75(selected)
            .unwrap();
        let expected = opening.fixture_backend_v75().expected();
        let mut engine_owner = raw(&state);
        let expected_after = engine_owner
            .step(expected.episode_id, expected.step, raw_index)
            .unwrap();
        assert_eq!(numeric.apply(&opening, selected).unwrap(), expected_after);
        let pair_opening = decision(&paired);
        paired.apply(&pair_opening, selected).unwrap();

        let mut reached_storm = false;
        for _ in 0..40 {
            let d = decision(&numeric);
            let paired_d = decision(&paired);
            assert_eq!(d.packet(), paired_d.packet());
            let backend = d.fixture_backend_v75();
            let obs = backend.observation();
            let rows = &backend.candidates_v75().candidates;
            let storms = obs.thousand_year_storm.as_ref().unwrap();
            if let Some(trigger) = storms.triggers.iter().find(|row| row.choice.is_some()) {
                let cast = &storms.casts[trigger.cast as usize];
                assert_eq!(cast.spell.original_spell.object, hit.0);
                assert_eq!(cast.spell.mana_spent, 0);
                assert_eq!(trigger.expected_copies, 1);
                let attack = obs.etali.as_ref().unwrap().attacks.last().unwrap();
                assert_eq!(attack.actual_casts.len(), 1);
                assert_eq!(attack.actual_casts[0].spell.object, hit.0);
                assert_eq!(
                    attack.actual_casts[0].original_exile.owner,
                    actor.opponent().into()
                );
                assert_eq!(attack.actual_casts[0].method, cast.spell.cast_method);
                assert_model(&numeric, &d);

                let original = rows
                    .iter()
                    .position(|row| {
                        row.thousand_year_storm_action.is_some()
                            && matches!(row.action, Action::FinishEffectSelection { .. })
                    })
                    .unwrap();
                let mut raw_copy = raw(numeric.fixture_state());
                let mut human =
                    HumanKaitoSessionV75::from_session(raw(numeric.fixture_state()), actor.into())
                        .unwrap();
                let mut human_projector = HumanKaitoProjectorV75::new(actor.into());
                let current = backend.expected();
                let human_binding = match human.current_response() {
                    FastActorResponseV1::Decision(h) => h,
                    _ => panic!("Storm Human checkpoint missing"),
                };
                let prompt = human_projector
                    .project_current(&human, human_binding)
                    .unwrap();
                let mut wire = JsonlServerV1::new();
                wire.0.install_fixture_core(
                    KaitoSessionV75::from_session(raw(numeric.fixture_state())).unwrap(),
                );
                let Body::State { state, .. } =
                    wire_state(wire_command(&mut wire, 1, CommandV1::Current))
                else {
                    panic!("Storm wire checkpoint missing");
                };
                let WireState::Decision {
                    episode_id,
                    authority_generation,
                    decision: visible,
                } = *state
                else {
                    panic!("Storm wire checkpoint terminal");
                };
                let (_, indices) =
                    project_limited_kaito_v16(backend, actor.into(), visible.prompt_seq).unwrap();
                assert_eq!(prompt.actions, visible.actions);
                assert_eq!(prompt.action_meanings, visible.action_meanings);
                let ui = indices
                    .iter()
                    .position(|&i| i as usize == original)
                    .unwrap();
                let raw_backend =
                    decision(&KaitoSession::from_session(raw(numeric.fixture_state())).unwrap());
                let raw_expected = raw_backend.fixture_backend_v75().expected();
                let raw_index = raw_backend
                    .fixture_backend_v75()
                    .fixture_original_index_v75(original as u32)
                    .unwrap();
                let raw_response = raw_copy
                    .step(raw_expected.episode_id, raw_expected.step, raw_index)
                    .unwrap();
                let saved = numeric.snapshot();
                numeric.apply(&d, original as u32).unwrap();
                assert_eq!(raw_copy.kaito_fixture_state(), numeric.fixture_state());
                human_projector
                    .submit(
                        &mut human,
                        HumanActionRequestV1 {
                            prompt_seq: prompt.prompt_seq,
                            action_index: ui as u32,
                        },
                    )
                    .unwrap();
                assert_eq!(human.current_response(), raw_response);
                let step = CommandV1::Step {
                    episode_id,
                    authority_generation,
                    actor: current.acting_player,
                    prompt_seq: visible.prompt_seq,
                    action_index: ui as u32,
                };
                assert!(matches!(
                    wire_state(wire_command(&mut wire, 2, step)),
                    Body::State {
                        receipt: Some(_),
                        ..
                    }
                ));
                let after = decision(&numeric);
                let after_storm = after
                    .fixture_backend_v75()
                    .observation()
                    .thousand_year_storm
                    .as_ref()
                    .unwrap();
                assert!(after_storm
                    .triggers
                    .iter()
                    .any(|row| row.created_copies.len() == 1));
                numeric.restore(&saved).unwrap();
                assert_eq!(decision(&numeric).packet(), d.packet());
                assert!(numeric.apply(&d, original as u32).is_err());
                reached_storm = true;
                break;
            }
            let pick = rows
                .iter()
                .position(|row| {
                    matches!(
                        &row.action,
                        Action::ChooseKicker { pay: false, .. }
                            | Action::ChooseTarget {
                                target: TargetRefV1::Player { .. },
                                ..
                            }
                    )
                })
                .or_else(|| {
                    rows.iter().position(|row| {
                        matches!(
                            row.action,
                            Action::Pass { .. }
                                | Action::OrderTriggers { .. }
                                | Action::ChooseTriggerOrderNext { .. }
                                | Action::FinishEffectSelection { .. }
                        )
                    })
                })
                .expect("missing real Etali child or Storm continuation");
            numeric.apply(&d, pick as u32).unwrap();
            paired.apply(&paired_d, pick as u32).unwrap();
        }
        assert!(reached_storm);
    }
}

#[test]
fn paid_axe_equips_the_real_harvester_nightmare_under_one_copy_scope_both_seats() {
    use crate::{
        card_def::Keywords, effect::electroduplicate_v1_tests as native,
        leyline_axe_consumer_v1::ActionV1 as Axe,
    };
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = native::hv_ready(actor);
        FastActorSessionV1::enable_limited_native_modes_v1(&mut state, LimitedAppendixV1::FullPool);
        let harvester = native::paid(&mut state, actor, "Abyssal Harvester", 3);
        let axe = native::paid(&mut state, actor, "Leyline Axe", 4);
        native::hv_main(&mut state, actor);
        let victim = native::paid(&mut state, actor, "Llanowar Elves", 1);
        storm::cast(
            &mut state,
            actor.opponent(),
            "Burst Lightning",
            &[Target::Object(victim)],
            1,
        );
        storm::finish(&mut state);
        assert_eq!(state.objects.get(victim).zone, Zone::Graveyard);
        native::hv_activate(&mut state, actor, harvester, victim);
        native::hv_finish_pair(&mut state);
        let [nightmare] = native::hv_nightmares(&state, actor)[..] else {
            panic!("Harvester did not create exactly one actual Nightmare");
        };
        assert!(crate::effect::harvester_v1::require_legacy_absence(&state).is_err());
        assert!(crate::leyline_axe_v1::require_legacy_absence(&state).is_err());

        storm::cast(
            &mut state,
            actor,
            "Electroduplicate",
            &[Target::Object(nightmare)],
            3,
        );
        storm::finish(&mut state);
        let mut owner = KaitoSession::from_session(raw(&state)).unwrap();
        let d = decision(&owner);
        let mut forged = d.fixture_backend_v75().observation().clone();
        let tokens = &mut forged.electroduplicate.as_mut().unwrap().copied_tokens;
        assert_eq!(tokens.len(), 1);
        tokens.clear();
        forged.projection_sha256 = crate::kaito_observation_v75::commitment(&forged).unwrap();
        assert!(crate::kaito_observation_v75::validate_artifact(&forged).is_err());

        let mut selected = false;
        let mut equipped = false;
        for _ in 0..24 {
            let d = decision(&owner);
            let backend = d.fixture_backend_v75();
            let obs = backend.observation();
            let facts = obs.harvester.as_ref().unwrap();
            let birth = &facts.activations.last().unwrap().created[0];
            assert_eq!(birth.original.object, nightmare.0);
            assert_eq!(birth.copied_printing, card("Llanowar Elves"));
            assert!(birth.added_copiable_nightmare);
            let equipment = obs
                .leyline_axe
                .as_ref()
                .unwrap()
                .battlefield
                .iter()
                .find(|row| row.card.object == axe.0)
                .unwrap();
            if let Some(host) = &equipment.host {
                assert!(selected);
                assert_eq!(host.card.object, nightmare.0);
                assert!(host.receives_printed_bonus);
                assert!(crate::engine::has_effective_keyword(
                    owner.fixture_state(),
                    nightmare,
                    Keywords::DOUBLE_STRIKE,
                ));
                assert!(crate::engine::has_effective_subtype(
                    owner.fixture_state(),
                    nightmare,
                    crate::card_def::Subtype::Nightmare,
                ));
                assert_model(&owner, &d);
                equipped = true;
                break;
            }
            let rows = &backend.candidates_v75().candidates;
            let pick = rows
                .iter()
                .position(|row| {
                    !selected
                        && (row.leyline_axe_action == Some(Axe::ActivateEquip { generic_cost: 3 })
                            || (row.leyline_axe_action == Some(Axe::ChooseEquipTarget)
                                && matches!(&row.action, Action::ChooseTarget {
                            target: TargetRefV1::Object { object }, ..
                        } if object.arena_id == nightmare.0)))
                })
                .or_else(|| {
                    rows.iter()
                        .position(|row| matches!(row.action, Action::Pass { .. }))
                })
                .expect("missing genuine Axe equip continuation");
            let target_choice = rows[pick].leyline_axe_action == Some(Axe::ChooseEquipTarget);
            let count_tapped_lands = |state: &GameState| {
                state.players[actor.index()]
                    .battlefield
                    .iter()
                    .filter(|&&id| {
                        let object = state.objects.get(id);
                        object.tapped && CARD_DEFS[usize::from(object.card_def)].is_land
                    })
                    .count()
            };
            let before = count_tapped_lands(owner.fixture_state());
            owner.apply(&d, pick as u32).unwrap();
            if target_choice {
                assert_eq!(count_tapped_lands(owner.fixture_state()) - before, 3);
                selected = true;
            }
        }
        assert!(equipped);
    }
}

#[test]
fn every_frozen_printing_reaches_the_full_pool_sealed_learner_for_each_seat() {
    fn check(actor: PlayerId, printings: &[u16], model: &ModelV1) -> Result<(), String> {
        let library = [card("Forest"); 40];
        // Put a bounded own-hand sample at an actual main-phase decision.
        let mut state = GameState::new_from_libraries_with_starting_player_v1(
            &library,
            &library,
            crate::rl::card_name,
            550_286,
            actor,
        );
        state.step = crate::state::Step::Main1;
        FastActorSessionV1::enable_limited_native_modes_v1(&mut state, LimitedAppendixV1::FullPool);
        for land in ["Plains", "Island", "Swamp", "Mountain", "Forest"] {
            put(&mut state, actor, land, Zone::Battlefield);
        }
        let objects: Vec<_> = printings
            .iter()
            .map(|&id| {
                put(
                    &mut state,
                    actor,
                    CARD_DEFS[usize::from(id)].name,
                    Zone::Hand,
                )
            })
            .collect();
        let owner = KaitoSession::from_session(raw(&state)).map_err(|e| format!("{e:?}"))?;
        let FastActorResponseV1::Decision(expected) = owner.current_response() else {
            return Err(format!(
                "expected native decision, got {:?}",
                owner.current_response()
            ));
        };
        if expected.acting_player != actor.into() {
            return Err(format!(
                "unexpected native chooser {:?}",
                expected.acting_player
            ));
        }
        let d = owner.encode(expected).map_err(|e| format!("{e:?}"))?;
        let hand = &d.fixture_backend_v75().observation().common.common.own_hand;
        for (&printing, object) in printings.iter().zip(objects) {
            if !hand
                .iter()
                .any(|row| row.stable.arena_id == object.0 && row.stable.card_db_id == printing)
            {
                return Err(format!("own hand lost frozen printing {printing}"));
            }
        }
        let input = owner.training_input_v1(&d).map_err(|e| format!("{e:?}"))?;
        let scores = model.forward(&input).map_err(|e| format!("{e:?}"))?;
        if scores.len() != d.packet().actions().len()
            || scores.iter().any(|score| !score.is_finite())
        {
            return Err("sealed learner returned an invalid score vector".into());
        }
        Ok(())
    }

    let deck: Vec<_> = declared_printings_v1().unwrap().iter().copied().collect();
    assert_eq!(deck.len(), 286);
    let model = ModelV1::deterministic_fixture();
    let mut failures = Vec::new();
    for actor in [PlayerId::P0, PlayerId::P1] {
        for printings in deck.chunks(7) {
            if let Err(error) = check(actor, printings, &model) {
                failures.push(format!("{actor:?} batch {printings:?}: {error}"));
                // A failed batch can hide later unsupported roots. Keep its
                // failure and report each individually failing printing too.
                for &id in printings {
                    if let Err(error) = check(actor, &[id], &model) {
                        let name = CARD_DEFS[usize::from(id)].name;
                        failures.push(format!("{actor:?} {id} {name}: {error}"));
                    }
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
