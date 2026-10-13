//! Genuine typed Aegis-copy primitive setup plus real Heartfire cast/activation.
use super::*;
use crate::{
    effect::{EffectObjectBinding, ExecCtx},
    engine::{self, Action, Decision},
    event::{self, ProposedEvent},
    ids::{ObjectId, PlayerId},
    mana::ManaColor,
    policy_observation_v6::tests::{put, ready_state_for},
    state::{AbilitySourceContractV4, GameState, ObjectLinkV4, Target, Zone},
};
fn resolve(state: &mut GameState, source: ObjectId) {
    for _ in 0..40 {
        let next = engine::advance_until_decision(state);
        assert!(matches!(next, Decision::CastSpellOrPass { .. }), "{next:?}");
        if state.engine.pending_cast.is_none()
            && !state.stack.iter().any(|row| row.source == source)
        {
            return;
        }
        engine::step(state, Action::Pass).unwrap();
    }
    panic!("genuine cast did not finish");
}
fn announced(actor: PlayerId, memorial: bool) -> (GameState, ObjectId, ObjectId, u32) {
    let mut state = ready_state_for(actor);
    put(&mut state, actor, "Lightning Bolt", Zone::Hand);
    put(&mut state, actor.opponent(), "Forest", Zone::Hand);
    if memorial {
        let m = put(&mut state, actor, "Akroma's Memorial", Zone::Hand);
        state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 7;
        engine::step(&mut state, Action::CastSpell(m)).unwrap();
        resolve(&mut state, m);
    }
    let donor = put(&mut state, actor, "Heartfire Immolator", Zone::Hand);
    state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 1;
    state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 1;
    engine::step(&mut state, Action::CastSpell(donor)).unwrap();
    resolve(&mut state, donor);
    assert_eq!(state.objects.get(donor).zone, Zone::Battlefield);
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(donor, Zone::Exile));
    let source = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
    let equipment = put(&mut state, actor, "Viridian Longbow", Zone::Battlefield);
    let host = ObjectLinkV4 {
        object: source,
        zone_change_count: state.objects.get(source).zone_change_count,
    };
    // Same reviewed typed copy setup as HeartfireHistoryV1, not a printed
    // Longbow copy spell or a mixed-catalog Aegis admission.
    state.objects.get_mut(equipment).v4.attached_to = Some(host);
    state.objects.get_mut(equipment).v4.layer_timestamp = Some(17);
    state.objects.get_mut(source).attachments.push(equipment);
    let contract = AbilitySourceContractV4::capture(&state, equipment);
    state.objects.get_mut(donor).v4.exiled_by = Some(ObjectLinkV4 {
        object: equipment,
        zone_change_count: contract.zone_change_count,
    });
    let mut context = ExecCtx::no_targets(equipment, actor);
    context.ability_source_contract = Some(contract);
    let choice = EffectObjectBinding {
        object: donor,
        expected_zone: Zone::Exile,
        expected_zone_change_count: state.objects.get(donor).zone_change_count,
    };
    assert!(
        crate::standard_cards_v1::aegis_copy_candidates(&state, &context, host, 17)
            .contains(&choice)
    );
    crate::standard_cards_v1::apply_aegis_copy(&mut state, &context, host, 17, choice);
    assert!(crate::standard_cards_v1::had_card_def(
        &state,
        source,
        host.zone_change_count,
        state.objects.get(donor).card_def
    ));
    let target = put(
        &mut state,
        actor.opponent(),
        "Shivan Dragon",
        Zone::Battlefield,
    );
    state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 1;
    engine::step(&mut state, Action::ActivateAbility(source, 0)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::ChooseTargets { .. }
    ));
    engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    assert!(state.engine.pending_activation.is_none());
    assert_eq!(state.objects.get(source).zone, Zone::Graveyard);
    assert_eq!(
        state.objects.get(source).card_def,
        crate::card_def::card_id_by_name("Llanowar Elves").unwrap()
    );
    (state, source, target, host.zone_change_count)
}
fn check(memorial: bool) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, source, target, generation) = announced(actor, memorial);
        // Real zone primitive separates the later physical incarnation from
        // the genuinely paid copied source. It is not a resurrection spell.
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change_to_battlefield_under_controller(source, actor.opponent()),
        );
        assert_eq!(state.objects.get(source).controller, actor.opponent());
        let surface = crate::policy_surface_v5::PolicySurfaceV5::new_for_session();
        assert!(crate::rl::observe_policy_v8(&state, &surface, actor, 0, 0, 0, 1).is_err());
        let core = FastActorSessionV1::from_akroma_v40_fixture_state(state.clone());
        let mut session = HumanAkromaSessionV40::from_session(core, actor.into()).unwrap();
        let FastActorResponseV1::Decision(expected) = session.current_response() else {
            panic!("actual held ability");
        };
        let public = session.public_observation(expected).unwrap();
        let stack = &public.projection.surface.stack[0];
        assert_eq!(
            stack.source.card_db_id,
            crate::card_def::card_id_by_name("Heartfire Immolator").unwrap()
        );
        assert_eq!(stack.source.zone_change_count, generation);
        assert_eq!(stack.source.controller, actor.into());
        assert!(
            public.projection.surface.battlefield[actor.opponent().index()]
                .iter()
                .any(|row| row.stable.arena_id == source.0
                    && row.stable.zone_change_count > generation
                    && row.stable.card_db_id != stack.source.card_db_id)
        );
        let early = public
            .families
            .iter()
            .find_map(|row| match row {
                crate::akroma_family_public_v40::FamilyFactsV40::Early(o) => Some(o),
                _ => None,
            })
            .unwrap();
        assert_eq!(early.saved_heartfire_damage[0].signed_departing_power, 2);
        assert_eq!(early.saved_heartfire_damage[0].captured_color_mask, 8);
        let snapshot = session.snapshot_v40();
        let mut projector = HumanAkromaProjectorV40::new(actor.into());
        let menu = projector.project_current(&session, expected).unwrap();
        let json = serde_json::to_string(&menu).unwrap();
        for private in [
            "arena_id",
            "source_contract",
            "private_snapshot_json",
            "event_history",
            "Forest",
        ] {
            assert!(!json.contains(private), "{private}");
        }
        let pass = menu
            .actions
            .iter()
            .find(|row| row.label == "Pass priority")
            .unwrap();
        let request = HumanActionRequestV1 {
            prompt_seq: menu.prompt_seq,
            action_index: pass.action_index,
        };
        let receipt = projector.submit(&mut session, request.clone()).unwrap();
        assert_eq!(
            projector.submit(&mut session, request.clone()).unwrap(),
            receipt
        );
        session.restore_v40(&snapshot).unwrap();
        let before = session.inner.privileged_core_environment_hash();
        assert_eq!(
            projector.submit(&mut session, request),
            Err(Error::StaleDecision)
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        let FastActorResponseV1::Decision(expected) = session.current_response() else {
            panic!("restored ability");
        };
        let replay = projector.project_current(&session, expected).unwrap();
        assert_eq!(replay.state, menu.state);
        assert_eq!(replay.actions, menu.actions);
        let mut json = serde_json::to_value(&state).unwrap();
        json["standard_v1"]["copied_card_defs"] = serde_json::json!([]);
        let missing: GameState = serde_json::from_value(json).unwrap();
        let before = serde_json::to_vec(&missing).unwrap();
        assert!(
            crate::akroma_public_v1::observe_v1(&missing, &surface, actor, 0, 0, (0, 1)).is_err()
        );
        assert_eq!(serde_json::to_vec(&missing).unwrap(), before);
        // Genuine captured damage still executes after the original copy ended.
        for _ in 0..40 {
            if state.stack.is_empty() {
                break;
            }
            assert!(matches!(
                engine::advance_until_decision(&mut state),
                Decision::CastSpellOrPass { .. }
            ));
            engine::step(&mut state, Action::Pass).unwrap();
        }
        assert!(state.stack.is_empty());
        assert_eq!(state.objects.get(target).damage, 2);
        assert_eq!(state.objects.get(source).controller, actor.opponent());
    }
}
fn menu_check(memorial: bool) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, source, target, generation) = announced(actor, memorial);
        let barricade = put(
            &mut state,
            actor.opponent(),
            "Crystal Barricade",
            Zone::Battlefield,
        );
        // Labeled replacement primitive, as in the existing Crystal fixtures.
        // No printed shield spell or mana payment is manufactured.
        state.engine.next_replacement_id += 1;
        state
            .engine
            .active_replacements
            .push(crate::event::ActiveReplacement {
                id: state.engine.next_replacement_id,
                source: barricade,
                kind: crate::event::ReplacementEffectKind::PreventNextDamage {
                    target: Target::Object(target),
                    remaining: 1,
                },
            });
        for _ in 0..20 {
            let next = engine::advance_until_decision(&mut state);
            if matches!(next, Decision::ChooseEffectOption { .. }) {
                break;
            }
            assert!(matches!(next, Decision::CastSpellOrPass { .. }), "{next:?}");
            engine::step(&mut state, Action::Pass).unwrap();
        }
        let pending = state.engine.pending_effect.as_ref().unwrap();
        assert_eq!(pending.ctx.controller, actor);
        assert_eq!(state.objects.get(source).zone, Zone::Graveyard);
        let chooser = actor.opponent();
        let mut session = HumanAkromaSessionV40::from_session(
            FastActorSessionV1::from_akroma_v40_fixture_state(state),
            chooser.into(),
        )
        .unwrap();
        let mut projector = HumanAkromaProjectorV40::new(chooser.into());
        for _ in 0..3 {
            let FastActorResponseV1::Decision(expected) = session.current_response() else {
                panic!("actual affected-controller prompt");
            };
            let public = session.public_observation(expected).unwrap();
            let crystal = public
                .families
                .iter()
                .find_map(|row| match row {
                    crate::akroma_family_public_v40::FamilyFactsV40::Crystal(o) => {
                        o.prevention_menu.as_ref()
                    }
                    _ => None,
                })
                .unwrap();
            assert_eq!(crystal.chooser, chooser.into());
            assert_eq!(
                crystal.resolving_source.card_db_id,
                crate::card_def::card_id_by_name("Heartfire Immolator").unwrap()
            );
            assert_eq!(crystal.resolving_source.zone_change_count, generation);
            assert_eq!(crystal.source_history.as_ref().unwrap().signed_power, 2);
            assert_eq!(
                crystal.source_history.as_ref().unwrap().captured_controller,
                actor.into()
            );
            let choose = crystal
                .options
                .iter()
                .position(|row| row.kind == "shield")
                .unwrap_or(0);
            let snapshot = session.snapshot_v40();
            let safe = projector.project_current(&session, expected).unwrap();
            let json = serde_json::to_string(&safe).unwrap();
            for hidden in [
                "arena_id",
                "event_history",
                "source_contract",
                "private_snapshot_json",
            ] {
                assert!(!json.contains(hidden));
            }
            // Human presentation is sorted, while its private map retains the
            // actual structural option index selected from the authenticated menu.
            let (_, semantics, _) = session
                .inner
                .akroma_current_decision_input_v40(expected, chooser.into())
                .unwrap();
            let core_index=semantics.iter().position(|semantic|matches!(semantic,
                crate::rl::ActionSemanticV1::ChooseEffectOption {option_index,..} if usize::from(*option_index)==choose)).unwrap();
            let action = projector
                .pending
                .as_ref()
                .unwrap()
                .engine_indexes
                .iter()
                .position(|index| *index == core_index as u32)
                .unwrap();
            let request = HumanActionRequestV1 {
                prompt_seq: safe.prompt_seq,
                action_index: action as u32,
            };
            projector.submit(&mut session, request.clone()).unwrap();
            session.restore_v40(&snapshot).unwrap();
            let before = session.inner.privileged_core_environment_hash();
            assert_eq!(
                projector.submit(&mut session, request),
                Err(Error::StaleDecision)
            );
            assert_eq!(session.inner.privileged_core_environment_hash(), before);
            let FastActorResponseV1::Decision(expected) = session.current_response() else {
                panic!("restored replacement");
            };
            let replay = projector.project_current(&session, expected).unwrap();
            assert_eq!(replay.state, safe.state);
            assert_eq!(replay.actions, safe.actions);
            projector
                .submit(
                    &mut session,
                    HumanActionRequestV1 {
                        prompt_seq: replay.prompt_seq,
                        action_index: action as u32,
                    },
                )
                .unwrap();
            if session.inner.akroma_fixture_state().stack.is_empty() {
                break;
            }
        }
        let core = session.inner.akroma_fixture_state();
        assert!(core.stack.is_empty());
        assert!(core.engine.pending_effect.is_none());
        assert_eq!(core.objects.get(target).damage, 0);
        assert_eq!(
            core.engine
                .event_history
                .iter()
                .filter(|event| matches!(
                    event,
                    crate::event::CommittedEvent::SourcePowerActivationFinishedV1 { .. }
                ))
                .count(),
            1
        );
        assert_eq!(
            core.engine
                .active_replacements
                .iter()
                .filter(|row| matches!(
                    row.kind,
                    crate::event::ReplacementEffectKind::PreventNextDamage { .. }
                ))
                .count(),
            0
        );
    }
}
#[test]
fn registered_copied_heartfire_crystal_menu_captured_controller_private_mapping_and_owning_restore_both_seats(
) {
    menu_check(false);
}
#[test]
#[ignore = "requires genuine Akroma registration; actual Heartfire/Crystal roots and labeled shield primitive remain"]
fn actual_memorial_copied_heartfire_crystal_three_family_replacement_restore_both_seats() {
    menu_check(true);
}
#[test]
fn registered_copied_heartfire_captured_identity_reentry_privacy_owning_restore_and_forgery_both_seats(
) {
    check(false);
}
#[test]
#[ignore = "requires genuine Akroma registration; real Heartfire and typed copy provenance are unchanged"]
fn actual_memorial_copied_heartfire_union_uses_original_public_source_and_damage_facts_both_seats()
{
    check(true);
}
