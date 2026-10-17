use super::*;
use crate::{
    engine::{self, Action, Decision},
    graveyard_return_public_v61::{
        tests::{announce, paid, quiet, raid_ready, ready, settle},
        PowerSourceV61,
    },
    ids::ObjectId,
    policy_observation_v6::tests::put,
    state::Target,
};

fn current(
    state: &mut GameState,
) -> (
    PolicySurfaceV5,
    PolicyDecisionV5,
    GraveyardReturnCandidatesV61,
    NativeCandidateProjectionV61,
) {
    let mut surface = PolicySurfaceV5::new_with_engine_priority_v1();
    let origin = surface.next_decision(state).unwrap();
    let actor = origin.actor(state).unwrap();
    let before = serde_json::to_vec(state).unwrap();
    let (value, binding) =
        project_parts_v61(state, &surface, (actor, &origin), 0, 0, origin.substep()).unwrap();
    validate_current(
        state,
        &surface,
        (actor, &origin),
        0,
        0,
        origin.substep(),
        &value,
    )
    .unwrap();
    let roundtrip: GraveyardReturnCandidatesV61 =
        serde_json::from_slice(&serde_json::to_vec(&value).unwrap()).unwrap();
    assert_eq!(value, roundtrip);
    let restored: GameState = serde_json::from_slice(&before).unwrap();
    assert_eq!(
        project_v61(
            &restored,
            &surface,
            (actor, &origin),
            0,
            0,
            origin.substep()
        )
        .unwrap(),
        value
    );
    assert_eq!(serde_json::to_vec(state).unwrap(), before);
    assert_eq!(binding.public_to_original.len(), value.candidates.len());
    for (ordinal, (row, &original_index)) in value
        .candidates
        .iter()
        .zip(&binding.public_to_original)
        .enumerate()
    {
        assert_eq!(row.ordinal as usize, ordinal);
        assert_eq!(
            row.action,
            visible_action(
                &value.observation,
                binding.original[original_index].semantic.clone()
            )
            .unwrap()
        );
        // Apply only the actual private native action on an isolated copy. This
        // verifies every published row, not just a convenient pass candidate.
        let mut probe = state.clone();
        let mut probe_surface = surface.clone();
        probe_surface
            .apply(
                &mut probe,
                binding.original[original_index].policy_action.clone(),
            )
            .unwrap();
        assert!(probe.engine.halted.is_none());
    }
    assert!(!value.owning_consumer_admitted);
    (surface, origin, value, binding)
}

fn raid_power(value: &GraveyardReturnCandidatesV61, expected: i32, source: PowerSourceV61) {
    assert!(value.candidates.iter().all(|row| row.programs.iter().any(
        |program| matches!(&program.program, ProgramV61::AleshaRaid { power, .. }
            if power.amount == expected && power.source == source)
    )));
}

#[test]
fn paid_normal_familiar_mapping_and_drake_nonidempotent_boundary_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let familiar = paid(&mut state, actor, "Nine-Lives Familiar", None);
        let (_, origin, value, binding) = current(&mut state);
        assert_eq!(
            binding.public_to_original,
            (0..binding.original.len()).collect::<Vec<_>>()
        );
        assert!(value
            .candidates
            .iter()
            .any(|row| matches!(row.action, A::Pass { .. })));
        assert_eq!(
            value.observation.graveyard_return.battlefield_counters[actor.index()]
                .iter()
                .find(|row| row.card.object == familiar.0)
                .unwrap()
                .counters
                .revival,
            8
        );
        assert!(crate::next_five_observation_v59::validate_state(&state).is_err());
        assert!(crate::rl::core_policy_action_candidates_next_five_v59(&origin, &state).is_err());
        paid(&mut state, actor, "Drake Hatcher", None);
        let mut surface = PolicySurfaceV5::new_with_engine_priority_v1();
        let origin = surface.next_decision(&mut state).unwrap();
        let before = serde_json::to_vec(&state).unwrap();
        let mut probe = state.clone();
        let mut probe_surface = surface.clone();
        assert_eq!(probe_surface.next_decision(&mut probe).unwrap(), origin);
        assert_ne!(probe, state);
        assert!(probe.engine.event_history[state.engine.event_history.len()..].iter().any(|event|
            matches!(event, crate::event::CommittedEvent::DrakeJournalV1 { record }
                if matches!(&record.value, crate::drake_hatcher_v1::custody::JournalV1::Enclosing { .. }))));
        // Re-entering Advance issues a real enclosing operation even with an
        // unchanged menu. Retain the full-world guard and prove this boundary;
        // do not strip receipts or pretend the activation was projected.
        let refusal = project_v61(
            &state,
            &surface,
            (origin.actor(&state).unwrap(), &origin),
            0,
            0,
            origin.substep(),
        )
        .unwrap_err();
        assert_eq!(
            refusal.0,
            "V61 candidates require the actual already-current exact-bound surface"
        );
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        let restored: GameState = serde_json::from_slice(&before).unwrap();
        assert!(project_v61(
            &restored,
            &surface,
            (origin.actor(&restored).unwrap(), &origin),
            0,
            0,
            origin.substep()
        )
        .is_err());
    }
}

#[test]
fn paid_raid_target_and_original_current_then_departure_power_have_exact_annotations_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, alesha, target) = raid_ready(actor);
        let (_, origin, value, _) = current(&mut state);
        // Current native Alesha has a mandatory target prompt. There is no
        // native optional return decision here, and this projection invents none.
        assert!(matches!(
            origin,
            PolicyDecisionV5::Surface(crate::surface::SurfaceDecision::Decision(
                Decision::ChooseTargets {
                    can_finish: false,
                    ..
                }
            ))
        ));
        raid_power(&value, 3, PowerSourceV61::CurrentOriginalIncarnation);
        let choice = value.candidates.iter().find(|row| matches!(&row.action,
            A::ChooseTarget { target: TargetRefV1::Object { object }, .. } if object.arena_id == target.0)).unwrap();
        let bound = choice.selected_graveyard_target.as_ref().unwrap();
        assert_eq!(
            bound.original.generation,
            state.objects.get(target).zone_change_count
        );
        assert_eq!(
            bound.current_exact_graveyard_card.as_ref(),
            Some(&bound.original)
        );
        assert!(bound.legal_now);
        engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
        assert!(matches!(
            engine::advance_until_decision(&mut state),
            Decision::CastSpellOrPass { .. }
        ));
        let published = state.clone();
        let shrink = put(
            &mut state,
            actor.opponent(),
            "Fleeting Distraction",
            Zone::Hand,
        );
        announce(&mut state, actor.opponent(), shrink, Some(alesha));
        for _ in 0..12 {
            let d = engine::advance_until_decision(&mut state);
            if engine::effective_power(&state, alesha) == 2 {
                break;
            }
            quiet(&mut state, d);
        }
        assert_eq!(engine::effective_power(&state, alesha), 2);
        let (_, _, shrunk, _) = current(&mut state);
        raid_power(&shrunk, 2, PowerSourceV61::CurrentOriginalIncarnation);
        assert!(shrunk
            .candidates
            .iter()
            .all(|row| row.programs.iter().any(|p| matches!(
                p.program,
                ProgramV61::AleshaRaid { .. }
            ) && p.targets.len() == 1
                && !p.targets[0].legal_now)));
        let mut departed = published;
        let removal = put(&mut departed, actor.opponent(), "Abrade", Zone::Hand);
        announce(&mut departed, actor.opponent(), removal, Some(alesha));
        for _ in 0..12 {
            let d = engine::advance_until_decision(&mut departed);
            if departed.objects.get(alesha).zone == Zone::Graveyard {
                break;
            }
            quiet(&mut departed, d);
        }
        assert_eq!(departed.objects.get(alesha).zone, Zone::Graveyard);
        let (_, _, lki, _) = current(&mut departed);
        raid_power(&lki, 3, PowerSourceV61::ExactDepartureLki);
    }
}

fn two_deaths(actor: PlayerId) -> (GameState, [ObjectId; 2]) {
    let mut state = ready(actor);
    let one = paid(&mut state, actor, "Nine-Lives Familiar", None);
    let two = paid(&mut state, actor, "Nine-Lives Familiar", None);
    let spell = put(&mut state, actor, "Breath Weapon", Zone::Hand);
    announce(&mut state, actor, spell, None);
    for _ in 0..40 {
        let d = engine::advance_until_decision(&mut state);
        if matches!(&d, Decision::OrderTriggers { pending, .. } if pending.len() == 2) {
            return (state, [one, two]);
        }
        quiet(&mut state, d);
    }
    panic!("genuine simultaneous Nine-Lives death ordering absent");
}

#[test]
fn paid_nine_death_and_future_end_step_order_only_genuine_native_rows_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, familiars) = two_deaths(actor);
        let (_, _, death, binding) = current(&mut state);
        assert_eq!(death.candidates.len(), 2);
        assert_eq!(binding.original.len(), 2);
        assert!(death
            .candidates
            .iter()
            .all(|row| matches!(row.action, A::OrderTriggers { .. })
                && row.programs.len() == 2
                && row.programs.iter().all(|p| matches!(
                    p.program,
                    ProgramV61::NineLivesDeath {
                        departure_revival: 8,
                        return_base_before_replacements: 7
                    }
                ))));
        engine::step(&mut state, Action::OrderTriggers(vec![1, 0])).unwrap();
        settle(&mut state);
        let (_, _, scheduled, _) = current(&mut state);
        assert!(scheduled
            .candidates
            .iter()
            .all(|row| row.nine_lives_schedules.len() == 2));
        assert!(scheduled
            .candidates
            .iter()
            .all(|row| !matches!(row.action, A::OrderTriggers { .. })));
        let mut found = false;
        for _ in 0..300 {
            let d = engine::advance_until_decision(&mut state);
            if matches!(&d, Decision::OrderTriggers { pending, .. } if pending.len() == 2) {
                found = true;
                break;
            }
            quiet(&mut state, d);
        }
        assert!(found, "genuine simultaneous delayed return ordering absent");
        let (_, _, delayed, _) = current(&mut state);
        assert!(delayed
            .candidates
            .iter()
            .all(|row| row.nine_lives_schedules.is_empty()
                && row.programs.iter().all(|p| matches!(
                    p.program,
                    ProgramV61::NineLivesDelayedReturn {
                        departure_revival: 8,
                        return_base_before_replacements: 7
                    }
                ))));
        engine::step(&mut state, Action::OrderTriggers(vec![0, 1])).unwrap();
        settle(&mut state);
        for familiar in familiars {
            assert_eq!(state.objects.get(familiar).counters.revival, 7);
        }
    }
}

fn paid_search(actor: PlayerId, reversed: bool) -> GameState {
    let mut state = ready(actor);
    paid(&mut state, actor, "Nine-Lives Familiar", None);
    for name in ["Forest", "Lightning Bolt", "Snow-Covered Forest", "Forest"] {
        put(&mut state, actor, name, Zone::Library);
    }
    if reversed {
        state.players[actor.index()].library.reverse();
    }
    let ent = put(&mut state, actor, "Generous Ent", Zone::Hand);
    let forest = state.players[actor.index()]
        .battlefield
        .iter()
        .copied()
        .find(|&id| !state.objects.get(id).tapped && state.objects.get(id).name == "Forest")
        .unwrap();
    assert!(
        matches!(engine::advance_until_decision(&mut state), Decision::CastSpellOrPass { player, .. } if player == actor)
    );
    engine::step(&mut state, Action::ActivateManaAbility(forest)).unwrap();
    assert!(state.objects.get(forest).tapped);
    assert!(
        matches!(engine::advance_until_decision(&mut state), Decision::CastSpellOrPass { player, .. } if player == actor)
    );
    engine::step(&mut state, Action::ActivateAbility(ent, 0)).unwrap();
    // Activation first stages its original source. The native driver then
    // pays forestcycling and publishes the ability before returning priority.
    assert!(state
        .engine
        .pending_activation
        .as_ref()
        .is_some_and(|p| p.source == ent && p.ability_index == 0));
    assert_eq!(state.objects.get(ent).zone, Zone::Hand);
    assert!(
        matches!(engine::advance_until_decision(&mut state), Decision::CastSpellOrPass { player, .. } if player == actor)
    );
    assert!(state.engine.pending_activation.is_none());
    assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
    assert_eq!(state.objects.get(ent).zone, Zone::Graveyard);
    let published = state.stack.last().unwrap();
    assert_eq!(
        published.kind,
        crate::state::StackItemKind::ActivatedAbility
    );
    assert_eq!(published.source, ent);
    assert_eq!(published.v4.activated_ability_index, Some(0));
    assert_eq!(published.discarded, vec![ent]);
    for _ in 0..24 {
        let d = engine::advance_until_decision(&mut state);
        if matches!(d, Decision::ChooseEffectTargets { .. }) {
            return state;
        }
        quiet(&mut state, d);
    }
    panic!("paid native Generous Ent search absent");
}

#[test]
fn paid_equal_visible_library_orders_keep_private_native_map_and_identical_public_rows_both_seats()
{
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut first = paid_search(actor, false);
        let mut second = paid_search(actor, true);
        let (_, _, left, left_binding) = current(&mut first);
        let (_, _, right, right_binding) = current(&mut second);
        assert_ne!(left_binding.original, right_binding.original);
        assert_ne!(
            left_binding.public_to_original,
            right_binding.public_to_original
        );
        assert_eq!(left, right);
        let encoded = serde_json::to_value(&left).unwrap();
        assert!(encoded["candidates"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row.get("original_index").is_none() && row.get("policy_action").is_none()));
    }
}

#[test]
fn paid_exact_bound_combat_omits_only_native_infeasible_answer_and_rejects_owned_scan_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let familiar = paid(&mut state, actor, "Nine-Lives Familiar", None);
        let turn = state.turn;
        let mut found = false;
        for _ in 0..300 {
            let d = engine::advance_until_decision(&mut state);
            if matches!(d, Decision::DeclareAttackers { player, .. } if player == actor && state.turn > turn)
            {
                found = true;
                break;
            }
            quiet(&mut state, d);
        }
        assert!(found);
        // Explicit native combat requirement operand, independent of the
        // Familiar's printing. The exact native scan still decides legality.
        state.objects.get_mut(familiar).v4.goaded_by = vec![crate::state::GoadStateV4 {
            player: actor.opponent(),
            expires_at_turn: state.turn + 1,
        }];
        let (surface, origin, value, binding) = current(&mut state);
        assert!(matches!(origin, PolicyDecisionV5::AttackerInclusion { .. }));
        assert_eq!(
            surface.feasible_scan_answers(&state, &origin).unwrap(),
            [false, true]
        );
        assert_eq!(binding.original.len(), 2);
        assert_eq!(binding.public_to_original, vec![1]);
        assert_eq!(value.candidates.len(), 1);
        assert!(matches!(
            value.candidates[0].action,
            A::ChooseAttackerInclusion { include: true, .. }
        ));
        let mut owner_state = state.clone();
        let mut owner_surface = PolicySurfaceV5::new_with_engine_priority_v1();
        let owner_origin = owner_surface
            .next_decision_owned(&mut owner_state, 17)
            .unwrap();
        assert!(project_v61(
            &owner_state,
            &owner_surface,
            (actor, &owner_origin),
            0,
            0,
            owner_origin.substep()
        )
        .is_err());
    }
}

#[test]
fn paid_opponent_concealed_identity_changes_do_not_enter_candidate_projection_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let mut alternative = ready(actor);
        put(&mut state, actor.opponent(), "Counterspell", Zone::Hand);
        put(
            &mut alternative,
            actor.opponent(),
            "Valkyrie's Call",
            Zone::Hand,
        );
        // Independent initial hidden worlds precede both genuine paid histories.
        paid(&mut state, actor, "Nine-Lives Familiar", None);
        paid(&mut alternative, actor, "Nine-Lives Familiar", None);
        let (_, _, left, _) = current(&mut state);
        let (_, _, right, _) = current(&mut alternative);
        assert_eq!(left, right);
    }
}

#[test]
fn paid_candidate_annotations_actual_origin_and_source_state_forgeries_reject_read_only_both_seats()
{
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, _, target) = raid_ready(actor);
        let (surface, origin, value, _) = current(&mut state);
        let before = serde_json::to_vec(&state).unwrap();
        let mut forged = value.clone();
        forged.candidates[0].programs.clear();
        forged.projection_sha256 = commitment(&forged).unwrap();
        assert!(validate_artifact(&forged).is_err());
        let mut forged = value.clone();
        let row = forged
            .candidates
            .iter_mut()
            .find(|r| r.selected_graveyard_target.is_some())
            .unwrap();
        row.selected_graveyard_target
            .as_mut()
            .unwrap()
            .original
            .generation += 1;
        forged.projection_sha256 = commitment(&forged).unwrap();
        assert!(validate_artifact(&forged).is_err());
        let mut altered_origin = origin.clone();
        if let PolicyDecisionV5::Surface(crate::surface::SurfaceDecision::Decision(
            Decision::ChooseTargets { legal_targets, .. },
        )) = &mut altered_origin
        {
            legal_targets.clear();
        }
        assert!(project_v61(
            &state,
            &surface,
            (actor, &altered_origin),
            0,
            0,
            origin.substep()
        )
        .is_err());
        let mut wrong_state = state.clone();
        wrong_state.engine.pending_triggers[0]
            .source_contract
            .as_mut()
            .unwrap()
            .controller = actor.opponent();
        let wrong_before = serde_json::to_vec(&wrong_state).unwrap();
        assert!(project_v61(
            &wrong_state,
            &surface,
            (actor, &origin),
            0,
            0,
            origin.substep()
        )
        .is_err());
        assert_eq!(serde_json::to_vec(&wrong_state).unwrap(), wrong_before);
        let mut missing = serde_json::to_value(&value).unwrap();
        missing["candidates"][0]
            .as_object_mut()
            .unwrap()
            .remove("programs");
        assert!(serde_json::from_value::<GraveyardReturnCandidatesV61>(missing).is_err());
        let mut missing = serde_json::to_value(&value).unwrap();
        missing["candidates"][0]
            .as_object_mut()
            .unwrap()
            .remove("drake_activation");
        assert!(serde_json::from_value::<GraveyardReturnCandidatesV61>(missing).is_err());
        let mut unknown = serde_json::to_value(&value).unwrap();
        unknown["candidates"][0]["action"]["private_operand"] = 1.into();
        assert!(serde_json::from_value::<GraveyardReturnCandidatesV61>(unknown).is_err());
        let mut changed = value.clone();
        changed.candidates[0].ordinal = 1;
        changed.projection_sha256 = commitment(&changed).unwrap();
        assert!(validate_current(
            &state,
            &surface,
            (actor, &origin),
            0,
            0,
            origin.substep(),
            &changed
        )
        .is_err());
        assert_eq!(state.objects.get(target).zone, Zone::Graveyard);
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
    }
}

#[test]
fn paid_fishing_equip_keeps_typed_incarnation_relations_without_printing_authority_both_seats() {
    use crate::akroma_family_public_v40::FamilyFactsV40 as F;
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let host = paid(&mut state, actor, "Nine-Lives Familiar", None);
        let pole = paid(&mut state, actor, "Fishing Pole", None);
        let forests = state.players[actor.index()]
            .battlefield
            .iter()
            .copied()
            .filter(|&id| !state.objects.get(id).tapped && state.objects.get(id).name == "Forest")
            .take(2)
            .collect::<Vec<_>>();
        assert_eq!(forests.len(), 2);
        for forest in forests {
            assert!(
                matches!(engine::advance_until_decision(&mut state), Decision::CastSpellOrPass { player, .. } if player == actor)
            );
            engine::step(&mut state, Action::ActivateManaAbility(forest)).unwrap();
            assert!(state.objects.get(forest).tapped);
        }
        assert!(
            matches!(engine::advance_until_decision(&mut state), Decision::CastSpellOrPass { activatable_abilities, .. } if activatable_abilities.contains(&(pole, 0)))
        );
        engine::step(&mut state, Action::ActivateAbility(pole, 0)).unwrap();
        assert!(
            matches!(engine::advance_until_decision(&mut state), Decision::ChooseTargets { legal_targets, .. } if legal_targets.contains(&Target::Object(host)))
        );
        engine::step(&mut state, Action::ChooseTarget(Target::Object(host))).unwrap();
        settle(&mut state);
        assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
        assert_eq!(state.objects.get(pole).v4.attached_to.unwrap().object, host);
        let (surface, origin, value, _) = current(&mut state);
        let family_index = value
            .observation
            .common
            .common
            .families
            .iter()
            .position(|f| matches!(f, F::Fishing(_)))
            .unwrap();
        let F::Fishing(fishing) = &value.observation.common.common.families[family_index] else {
            unreachable!()
        };
        let pole_index = fishing
            .poles
            .iter()
            .position(|p| p.source.arena_id == pole.0)
            .unwrap();
        let relation = fishing.poles[pole_index].attached_host.unwrap();
        assert_eq!(
            (relation.arena_id, relation.zone_change_count),
            (host.0, state.objects.get(host).zone_change_count)
        );
        let before = serde_json::to_vec(&state).unwrap();
        let authorized = authorized_references(&value.observation).unwrap();
        let path = format!(
            "/common/common/families/{family_index}/facts/poles/{pole_index}/attached_host"
        );
        let mut raw = serde_json::to_value(&value.observation).unwrap();
        raw.pointer_mut(&path).unwrap()["zone_change_count"] =
            (relation.zone_change_count + 1).into();
        assert!(mask_incarnation_relations(&value.observation, &mut raw).is_err());
        let mut raw = serde_json::to_value(&value.observation).unwrap();
        raw.pointer_mut(&path).unwrap()["card_db_id"] = state.objects.get(host).card_def.into();
        assert!(mask_incarnation_relations(&value.observation, &mut raw).is_err());
        assert!(serde_json::from_value::<GraveyardReturnObservationV61>(raw).is_err());
        let mut output = Vec::new();
        assert!(references(&serde_json::to_value(relation).unwrap(), &mut output).is_err());
        assert!(output.is_empty());
        let mut forged = value.clone();
        let F::Fishing(fishing) = &mut forged.observation.common.common.families[family_index]
        else {
            unreachable!()
        };
        fishing.poles[pole_index]
            .attached_host
            .as_mut()
            .unwrap()
            .arena_id = u32::MAX;
        // Even a forged typed relation contributes no CardStableRef. Only
        // native recapture can authenticate this relation's actual incarnation.
        assert_eq!(
            authorized_references(&forged.observation).unwrap(),
            authorized
        );
        assert!(validate_current(
            &state,
            &surface,
            (origin.actor(&state).unwrap(), &origin),
            0,
            0,
            origin.substep(),
            &forged
        )
        .is_err());
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
    }
}
