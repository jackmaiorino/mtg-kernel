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
    announced_with_fishing(actor, memorial, false).0
}
type AnnouncedWithFishing = (
    (GameState, ObjectId, ObjectId, u32),
    Option<(ObjectId, ObjectId)>,
);
fn announced_with_fishing(actor: PlayerId, memorial: bool, fishing: bool) -> AnnouncedWithFishing {
    let mut state = ready_state_for(actor);
    put(&mut state, actor, "Lightning Bolt", Zone::Hand);
    put(&mut state, actor.opponent(), "Forest", Zone::Hand);
    if memorial {
        let m = put(&mut state, actor, "Akroma's Memorial", Zone::Hand);
        state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 7;
        engine::step(&mut state, Action::CastSpell(m)).unwrap();
        resolve(&mut state, m);
    }
    let fishing_objects = fishing.then(|| {
        let host = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        let pole = put(&mut state, actor, "Fishing Pole", Zone::Hand);
        state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 1;
        engine::step(&mut state, Action::CastSpell(pole)).unwrap();
        resolve(&mut state, pole);
        state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 2;
        engine::step(&mut state, Action::ActivateAbility(pole, 0)).unwrap();
        assert!(matches!(
            engine::advance_until_decision(&mut state),
            Decision::ChooseTargets { .. }
        ));
        engine::step(&mut state, Action::ChooseTarget(Target::Object(host))).unwrap();
        resolve(&mut state, pole);
        assert_eq!(state.objects.get(pole).v4.attached_to.unwrap().object, host);
        (host, pole)
    });
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
    (
        (state, source, target, host.zone_change_count),
        fishing_objects,
    )
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
            assert!(public.families.iter().any(|row| matches!(row,
                crate::akroma_family_public_v40::FamilyFactsV40::Twinflame(o)
                    if o.prevention_menu.is_none() && o.optional_discard_menu.is_none())));
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

#[test]
#[ignore = "requires genuine future Fishing Pole and Fish registration; no fixture admission hook"]
fn real_fishing_and_copied_heartfire_rows_keep_independent_original_sources_and_binding_both_seats()
{
    use crate::akroma_family_public_v40::FamilyFactsV40;
    for actor in [PlayerId::P0, PlayerId::P1] {
        let ((mut state, heartfire, target, generation), fishing) =
            announced_with_fishing(actor, false, true);
        let (host, pole) = fishing.unwrap();
        let fishing_generation = state.objects.get(host).zone_change_count;
        let native = u8::try_from(
            crate::card_def::CARD_DEFS[state.objects.get(host).card_def as usize]
                .activated_abilities
                .len(),
        )
        .unwrap();
        state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 1;
        engine::step(&mut state, Action::ActivateAbility(host, native)).unwrap();
        assert!(matches!(
            engine::advance_until_decision(&mut state),
            Decision::ChooseCostTargets { player, candidates, .. }
                if player == actor && candidates.contains(&pole)
        ));
        engine::step(&mut state, Action::ChooseCostTarget(pole)).unwrap();
        assert!(matches!(
            engine::advance_until_decision(&mut state),
            Decision::CastSpellOrPass { .. }
        ));
        assert!(state.engine.pending_activation.is_none());
        assert_eq!(state.stack.len(), 2);
        assert!(state.objects.get(host).tapped && state.objects.get(pole).tapped);
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(host, Zone::Hand));
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change_to_battlefield_under_controller(heartfire, actor.opponent()),
        );
        let restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        let core = FastActorSessionV1::from_akroma_v40_fixture_state(restored);
        let mut session = HumanAkromaSessionV40::from_session(core, actor.into()).unwrap();
        let FastActorResponseV1::Decision(expected) = session.current_response() else {
            panic!("actual two-family held rows");
        };
        let public = session.public_observation(expected).unwrap();
        let heart = public
            .projection
            .surface
            .stack
            .iter()
            .find(|row| row.source.arena_id == heartfire.0)
            .unwrap();
        let fishing = public
            .projection
            .surface
            .stack
            .iter()
            .find(|row| row.source.arena_id == host.0)
            .unwrap();
        assert_eq!(
            heart.source.card_db_id,
            crate::card_def::card_id_by_name("Heartfire Immolator").unwrap()
        );
        assert_eq!(heart.source.zone_change_count, generation);
        assert_eq!(heart.source.controller, actor.into());
        assert_eq!(
            fishing.source.card_db_id,
            crate::card_def::card_id_by_name("Llanowar Elves").unwrap()
        );
        assert_eq!(fishing.source.zone, Zone::Battlefield);
        assert_eq!(fishing.source.zone_change_count, fishing_generation);
        assert_eq!(fishing.source.controller, actor.into());
        for source in [&heart.source, &fishing.source] {
            assert!(public
                .extensions
                .historical_public_sources
                .iter()
                .any(|row| &row.source == source));
        }
        assert!(public
            .families
            .iter()
            .any(|row| matches!(row, FamilyFactsV40::Fishing(_))));
        assert!(public
            .families
            .iter()
            .any(|row| matches!(row, FamilyFactsV40::Early(facts)
            if facts.saved_heartfire_damage.len() == 1)));
        let snapshot = session.snapshot_v40();
        let mut projector = HumanAkromaProjectorV40::new(actor.into());
        let prompt = projector.project_current(&session, expected).unwrap();
        let json = serde_json::to_string(&prompt).unwrap();
        for private in [
            "arena_id",
            "source_contract",
            "private_snapshot_json",
            "event_history",
        ] {
            assert!(!json.contains(private), "{private}");
        }
        let row = prompt
            .actions
            .iter()
            .find(|row| row.label == "Pass priority")
            .unwrap();
        let request = HumanActionRequestV1 {
            prompt_seq: prompt.prompt_seq,
            action_index: row.action_index,
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
            panic!("restored mixed rows");
        };
        let replay = projector.project_current(&session, expected).unwrap();
        assert_eq!(replay.state, prompt.state);
        assert_eq!(replay.actions, prompt.actions);
        for mode in 0..2 {
            let mut bad = state.clone();
            if mode == 0 {
                bad.engine.event_history.retain(|event| {
                    !matches!(
                        event,
                        crate::event::CommittedEvent::FishingPoleJournalV39 { .. }
                    )
                });
            } else {
                let mut json = serde_json::to_value(&bad).unwrap();
                json["standard_v1"]["copied_card_defs"] = serde_json::json!([]);
                bad = serde_json::from_value(json).unwrap();
            }
            let bytes = serde_json::to_vec(&bad).unwrap();
            let surface = crate::policy_surface_v5::PolicySurfaceV5::new_for_session();
            assert!(
                crate::akroma_public_v1::observe_v1(&bad, &surface, actor, 0, 0, (0, 1)).is_err()
            );
            assert_eq!(serde_json::to_vec(&bad).unwrap(), bytes);
        }
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
        assert_eq!(state.objects.get(pole).counters.bait, 1);
        assert_eq!(state.objects.get(target).damage, 2);
        assert_eq!(state.objects.get(heartfire).controller, actor.opponent());
    }
}

#[test]
#[ignore = "requires genuine Twinflame registration; actual copied Heartfire payment and V40 history route"]
fn actual_twinflame_copied_heartfire_order_history_private_binding_and_restore_both_seats() {
    use crate::akroma_family_public_v40::{FamilyFactsV40, FamilyProjectionV40};
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, source, target, generation) = announced(actor, false);
        put(&mut state, actor, "Mountain", Zone::Battlefield);
        for seat in [actor, actor.opponent()] {
            for _ in 0..8 {
                put(&mut state, seat, "Forest", Zone::Library);
            }
        }
        // Cast the real Tyrant above the already paid copied activation. Its
        // actual entry precedes damage preparation, without replacing journals.
        let tyrant = put(&mut state, actor, "Twinflame Tyrant", Zone::Hand);
        state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 3;
        state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 2;
        engine::step(&mut state, Action::CastSpell(tyrant)).unwrap();
        resolve(&mut state, tyrant);
        assert_eq!(state.objects.get(tyrant).zone, Zone::Battlefield);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change_to_battlefield_under_controller(source, actor.opponent()),
        );
        assert_eq!(state.objects.get(source).controller, actor.opponent());
        assert!(state.objects.get(source).zone_change_count > generation);
        state.engine.next_replacement_id += 1;
        state
            .engine
            .active_replacements
            .push(crate::event::ActiveReplacement {
                id: state.engine.next_replacement_id,
                source: target,
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
        assert!(crate::effect::crystal_prevention_v1::is_choice(&state));
        let chooser = actor.opponent();
        let mut session = HumanAkromaSessionV40::from_session(
            FastActorSessionV1::from_akroma_v40_fixture_state(state),
            chooser.into(),
        )
        .unwrap();
        let mut projector = HumanAkromaProjectorV40::new(chooser.into());
        let saved = session.snapshot_v40();
        let FastActorResponseV1::Decision(expected) = session.current_response() else {
            panic!("real historical replacement menu");
        };
        let public = session.public_observation(expected).unwrap();
        let menu = public
            .families
            .iter()
            .find_map(|row| match row {
                FamilyFactsV40::Twinflame(o) => o.prevention_menu.as_ref(),
                _ => None,
            })
            .unwrap();
        assert_eq!(menu.chooser, chooser.into());
        assert_eq!(menu.original_damage, 2);
        assert_eq!(
            menu.source_history.as_ref().unwrap().captured_controller,
            actor.into()
        );
        assert_eq!(menu.source_history.as_ref().unwrap().signed_power, 2);
        assert_eq!(
            menu.resolving_source.card_db_id,
            crate::card_def::card_id_by_name("Heartfire Immolator").unwrap()
        );
        assert_eq!(menu.resolving_source.zone_change_count, generation);
        assert_eq!(menu.resolving_source.controller, actor.into());
        assert!(public.families.iter().any(|row| matches!(row,
            FamilyFactsV40::Crystal(o) if o.prevention_menu.is_none())));
        let (_, _, binding) = session
            .inner
            .akroma_current_decision_input_v40(expected, chooser.into())
            .unwrap();
        let mut forged = binding.clone();
        let part = forged
            .families
            .iter_mut()
            .find_map(|row| match row {
                FamilyProjectionV40::Twinflame(o) => Some(o),
                _ => None,
            })
            .unwrap();
        part.prevention_menu
            .as_mut()
            .unwrap()
            .resolving_source
            .zone_change_count += 1;
        let before = session.inner.privileged_core_environment_hash();
        assert!(session
            .inner
            .akroma_validate_bound_decision_v40(expected, chooser.into(), &forged)
            .is_err());
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        let prompt = projector.project_current(&session, expected).unwrap();
        let json = serde_json::to_string(&prompt).unwrap();
        for private in [
            "arena_id",
            "event_history",
            "source_contract",
            "private_snapshot_json",
            "zone_change_count",
        ] {
            assert!(!json.contains(private), "{private}");
        }
        let choose = |prompt: &HumanDecisionAkromaV40, needle: &str| HumanActionRequestV1 {
            prompt_seq: prompt.prompt_seq,
            action_index: prompt
                .actions
                .iter()
                .find(|row| row.label.contains(needle))
                .unwrap()
                .action_index,
        };
        let doubled = choose(&prompt, "double the remaining damage");
        let receipt = projector.submit(&mut session, doubled.clone()).unwrap();
        let before = session.inner.privileged_core_environment_hash();
        assert_eq!(
            projector.submit(&mut session, doubled.clone()).unwrap(),
            receipt
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        let FastActorResponseV1::Decision(expected) = session.current_response() else {
            panic!("remaining shield");
        };
        let remaining = projector.project_current(&session, expected).unwrap();
        projector
            .submit(&mut session, choose(&remaining, "shield"))
            .unwrap();
        let core = session.inner.akroma_fixture_state();
        assert_eq!(core.objects.get(target).damage, 3);
        assert!(core.engine.pending_effect.is_none());
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
        session.restore_v40(&saved).unwrap();
        let before = session.inner.privileged_core_environment_hash();
        assert_eq!(
            projector.submit(&mut session, doubled),
            Err(Error::StaleDecision)
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        let FastActorResponseV1::Decision(expected) = session.current_response() else {
            panic!("restored original menu");
        };
        let restored = projector.project_current(&session, expected).unwrap();
        assert_eq!(restored.state, prompt.state);
        assert_eq!(restored.actions, prompt.actions);
        projector
            .submit(&mut session, choose(&restored, "shield"))
            .unwrap();
        let FastActorResponseV1::Decision(expected) = session.current_response() else {
            panic!("remaining multiplier");
        };
        let remaining = projector.project_current(&session, expected).unwrap();
        projector
            .submit(
                &mut session,
                choose(&remaining, "double the remaining damage"),
            )
            .unwrap();
        let core = session.inner.akroma_fixture_state();
        assert_eq!(core.objects.get(target).damage, 2);
        assert!(core.engine.pending_effect.is_none());
        assert_eq!(core.objects.get(source).controller, chooser);
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
    }
}

#[test]
#[ignore = "requires genuine Twinflame registration; actual Incinerating Blast optional discard DTO and owning choices"]
fn actual_twinflame_incinerating_discard_parts_and_owning_decline_pay_restore_both_seats() {
    use crate::akroma_family_public_v40::FamilyFactsV40;
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state_for(actor);
        put(&mut state, actor, "Mountain", Zone::Battlefield);
        put(&mut state, actor, "Lightning Bolt", Zone::Hand);
        let discarded = put(&mut state, actor, "Forest", Zone::Hand);
        for seat in [actor, actor.opponent()] {
            for _ in 0..8 {
                put(&mut state, seat, "Forest", Zone::Library);
            }
        }
        let drawn = state.players[actor.index()].library[0];
        let tyrant = put(&mut state, actor, "Twinflame Tyrant", Zone::Hand);
        state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 3;
        state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 2;
        engine::step(&mut state, Action::CastSpell(tyrant)).unwrap();
        resolve(&mut state, tyrant);
        let target = put(
            &mut state,
            actor.opponent(),
            "Shivan Dragon",
            Zone::Battlefield,
        );
        let spell = put(&mut state, actor, "Incinerating Blast", Zone::Hand);
        state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 4;
        state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 1;
        engine::step(&mut state, Action::CastSpell(spell)).unwrap();
        assert!(matches!(
            engine::advance_until_decision(&mut state),
            Decision::ChooseTargets { .. }
        ));
        engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
        for _ in 0..24 {
            let next = engine::advance_until_decision(&mut state);
            if crate::effect::crystal_prevention_v1::is_discard_choice(&state) {
                break;
            }
            assert!(matches!(next, Decision::CastSpellOrPass { .. }), "{next:?}");
            engine::step(&mut state, Action::Pass).unwrap();
        }
        assert!(crate::effect::crystal_prevention_v1::is_discard_choice(
            &state
        ));
        let mut session = HumanAkromaSessionV40::from_session(
            FastActorSessionV1::from_akroma_v40_fixture_state(state),
            actor.into(),
        )
        .unwrap();
        let mut projector = HumanAkromaProjectorV40::new(actor.into());
        let saved = session.snapshot_v40();
        let FastActorResponseV1::Decision(expected) = session.current_response() else {
            panic!("actual optional discard");
        };
        let public = session.public_observation(expected).unwrap();
        let menu = public
            .families
            .iter()
            .find_map(|row| match row {
                FamilyFactsV40::Twinflame(o) => o.optional_discard_menu.as_ref(),
                _ => None,
            })
            .unwrap();
        assert_eq!(menu.chooser, actor.into());
        assert_eq!((menu.maximum_cards, menu.draws_if_discarded), (1, 1));
        assert_eq!(
            menu.source.card_db_id,
            crate::card_def::card_id_by_name("Incinerating Blast").unwrap()
        );
        assert!(public.families.iter().any(|row| matches!(row,
            FamilyFactsV40::Crystal(o) if o.optional_discard_menu.is_none())));
        let prompt = projector.project_current(&session, expected).unwrap();
        let decline = HumanActionRequestV1 {
            prompt_seq: prompt.prompt_seq,
            action_index: prompt
                .actions
                .iter()
                .find(|row| row.label == "Decline the optional discard")
                .unwrap()
                .action_index,
        };
        projector.submit(&mut session, decline.clone()).unwrap();
        assert_eq!(
            session
                .inner
                .akroma_fixture_state()
                .objects
                .get(discarded)
                .zone,
            Zone::Hand
        );
        assert_eq!(
            session.inner.akroma_fixture_state().objects.get(drawn).zone,
            Zone::Library
        );
        assert_eq!(
            session.inner.akroma_fixture_state().objects.get(spell).zone,
            Zone::Graveyard
        );
        session.restore_v40(&saved).unwrap();
        let before = session.inner.privileged_core_environment_hash();
        assert_eq!(
            projector.submit(&mut session, decline),
            Err(Error::StaleDecision)
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        let FastActorResponseV1::Decision(expected) = session.current_response() else {
            panic!("restored optional discard");
        };
        let restored = projector.project_current(&session, expected).unwrap();
        assert_eq!(restored.state, prompt.state);
        assert_eq!(restored.actions, prompt.actions);
        let pay = HumanActionRequestV1 {
            prompt_seq: restored.prompt_seq,
            action_index: restored
                .actions
                .iter()
                .find(|row| row.label.starts_with("Discard Forest "))
                .unwrap()
                .action_index,
        };
        let receipt = projector.submit(&mut session, pay.clone()).unwrap();
        let before = session.inner.privileged_core_environment_hash();
        assert_eq!(projector.submit(&mut session, pay).unwrap(), receipt);
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        let core = session.inner.akroma_fixture_state();
        assert_eq!(core.objects.get(discarded).zone, Zone::Graveyard);
        assert_eq!(core.objects.get(drawn).zone, Zone::Hand);
        assert_eq!(core.objects.get(spell).zone, Zone::Graveyard);
        assert!(core.engine.pending_effect.is_none());
        assert!(core.engine.event_history.iter().any(|event| matches!(event,
            crate::event::CommittedEvent::Damage { source, target: actual, amount: 12, .. }
                if *source == spell && *actual == Target::Object(target))));
    }
}
