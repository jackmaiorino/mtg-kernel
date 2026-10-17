use super::*;
use crate::ids::ObjectId;
use crate::mana::ManaColor;
use crate::policy_observation_v6::tests::{advance_to_next_main_for, put};
use crate::ravenous_public_v51::OutcomeV51;
use crate::state::Zone;

fn expected(session: &FastActorSessionV1) -> FastActorDecisionV1 {
    match session.current_response() {
        FastActorResponseV1::Decision(value) => value,
        other => panic!("actual V51 input: {other:?}"),
    }
}

fn fixture(actor: PlayerId) -> (FastActorSessionV1, ObjectId, ObjectId, [ObjectId; 7]) {
    let mut state = crate::extra_turn_v1::tests::ready(actor);
    for player in [actor, actor.opponent()] {
        for _ in 0..12 {
            put(&mut state, player, "Forest", Zone::Library);
        }
    }
    put(&mut state, actor, "Island", Zone::Hand);
    let amulet = put(&mut state, actor, "Ravenous Amulet", Zone::Hand);
    let chosen = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
    put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
    let forests = std::array::from_fn(|_| put(&mut state, actor, "Forest", Zone::Battlefield));
    (
        FastActorSessionV1::from_ravenous_v51_fixture_state(state),
        amulet,
        chosen,
        forests,
    )
}

fn act(session: &mut FastActorSessionV1, choose: impl Fn(&ActionSemanticV1) -> bool) {
    let current = expected(session);
    let (_, actions, binding) = session
        .ravenous_current_decision_input_v51(current, current.acting_player)
        .unwrap();
    let index = actions
        .iter()
        .position(choose)
        .unwrap_or_else(|| panic!("actual V51 action absent: {actions:?}"));
    session
        .ravenous_validate_bound_decision_v51(current, current.acting_player, &binding)
        .unwrap();
    session
        .step(current.episode_id, current.step, index as u32)
        .unwrap();
}

fn drain(session: &mut FastActorSessionV1) -> RavenousObservationV51 {
    for _ in 0..32 {
        let current = expected(session);
        let public = session
            .ravenous_current_decision_input_v51(current, current.acting_player)
            .unwrap()
            .0;
        if session.state.stack.is_empty() && session.state.engine.pending_triggers.is_empty() {
            return public;
        }
        act(session, |action| {
            matches!(action, ActionSemanticV1::Pass { .. })
        });
    }
    panic!("actual V51 stack did not finish");
}

fn gameplay(session: &FastActorSessionV1) -> (Vec<u8>, u64, u64) {
    (
        serde_json::to_vec(session.ravenous_fixture_state()).unwrap(),
        session.policy_step_count(),
        session.physical_decision_count(),
    )
}

fn mana(session: &mut FastActorSessionV1, source: ObjectId) {
    act(session, |action| {
        matches!(action,
        ActionSemanticV1::ActivateManaAbility { source: actual, .. } if actual.arena_id == source.0)
    });
}

#[test]
fn actual_paid_amulet_backend_cast_draw_soul_loss_and_owning_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut session, amulet, chosen, forests) = fixture(actor);
        for forest in &forests[..2] {
            mana(&mut session, *forest);
        }
        assert_eq!(
            session.state.players[actor.index()].mana_pool[ManaColor::G.pool_index()],
            2
        );
        act(&mut session, |action| {
            matches!(action,
            ActionSemanticV1::CastSpell { source, .. } if source.arena_id == amulet.0)
        });
        assert_eq!(
            session.state.players[actor.index()].mana_pool[ManaColor::G.pool_index()],
            0
        );
        drain(&mut session);
        assert_eq!(session.state.objects.get(amulet).zone, Zone::Battlefield);
        mana(&mut session, forests[2]);
        act(&mut session, |action| {
            matches!(action,
            ActionSemanticV1::ActivateAbility { source, ability_index: 0, .. } if source.arena_id == amulet.0)
        });
        let cost_snapshot = session.snapshot_v1();
        let hand_before = session.state.players[actor.index()].hand.len();
        act(&mut session, |action| {
            matches!(action,
            ActionSemanticV1::ChooseCostTarget { candidate, .. } if candidate.arena_id == chosen.0)
        });
        let paid_snapshot = session.snapshot_v1();
        assert_eq!(
            session.state.players[actor.index()].mana_pool[ManaColor::G.pool_index()],
            0
        );
        assert!(session.state.objects.get(amulet).tapped);
        assert_eq!(session.state.objects.get(chosen).zone, Zone::Graveyard);
        let public = drain(&mut session);
        assert_eq!(
            session.state.players[actor.index()].hand.len(),
            hand_before + 1
        );
        assert_eq!(session.state.objects.get(amulet).counters.soul, 1);
        assert!(matches!(
            public.ravenous.activations.last().unwrap().outcome,
            OutcomeV51::Draw {
                drew_card: true,
                ..
            }
        ));
        let row = public
            .battlefield
            .iter()
            .flatten()
            .find(|row| row.card.stable.arena_id == amulet.0)
            .unwrap();
        assert_eq!(row.soul, 1);
        let first = gameplay(&session);
        let old_revision = expected(&session).environment_revision;
        session.ravenous_restore_v51(&paid_snapshot).unwrap();
        assert!(expected(&session).environment_revision > old_revision);
        assert_eq!(drain(&mut session), public);
        assert_eq!(gameplay(&session), first);
        session.restore_v1(&cost_snapshot);
        act(&mut session, |action| {
            matches!(action,
            ActionSemanticV1::ChooseCostTarget { candidate, .. } if candidate.arena_id == chosen.0)
        });
        assert_eq!(drain(&mut session), public);
        assert_eq!(gameplay(&session), first);

        // A real intervening turn supplies the printed untap. The helper uses
        // ordinary engine transitions, then the backend acquires its new menu.
        advance_to_next_main_for(&mut session.state, actor);
        session.current = None;
        session.environment_revision += 1;
        session.advance_to_decision_or_terminal();
        for forest in &forests[..4] {
            mana(&mut session, *forest);
        }
        let life_before = session.state.players[actor.opponent().index()].life;
        act(&mut session, |action| {
            matches!(action,
            ActionSemanticV1::ActivateAbility { source, ability_index: 1, .. } if source.arena_id == amulet.0)
        });
        assert_eq!(session.state.objects.get(amulet).zone, Zone::Graveyard);
        assert_eq!(
            session.state.players[actor.index()].mana_pool[ManaColor::G.pool_index()],
            0
        );
        let loss_snapshot = session.snapshot_v1();
        let final_public = drain(&mut session);
        assert_eq!(
            session.state.players[actor.opponent().index()].life,
            life_before - 1
        );
        assert_eq!(
            final_public.ravenous.activations.last().unwrap().outcome,
            OutcomeV51::Loss {
                opponent: actor.opponent().into(),
                amount: 1
            }
        );
        let final_gameplay = gameplay(&session);
        session.ravenous_restore_v51(&loss_snapshot).unwrap();
        assert_eq!(drain(&mut session), final_public);
        assert_eq!(gameplay(&session), final_gameplay);
    }
}

#[test]
fn v51_complete_binding_counter_values_candidates_and_restore_owners_refuse_readonly_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut session, _, _, forests) = fixture(actor);
        let current = expected(&session);
        let (_, _, binding) = session
            .ravenous_current_decision_input_v51(current, actor.into())
            .unwrap();
        assert!(binding.candidates.len() > 1);
        let before = gameplay(&session);
        for case in 0..6 {
            let mut changed = binding.clone();
            match case {
                0 => changed.candidates.reverse(),
                1 => changed.observation.battlefield[actor.index()][0].soul ^= 1,
                2 => changed.observation.battlefield[actor.index()][0].marked_damage ^= 1,
                3 => changed.observation.battlefield[actor.index()][0].plus_one_counters ^= 1,
                4 => {
                    changed.observation.battlefield[actor.index()].pop();
                }
                5 => changed.observation.fake.card_db_hash ^= 1,
                _ => unreachable!(),
            }
            changed.observation.projection_sha256 =
                crate::ravenous_observation_v51::commitment(&changed.observation).unwrap();
            assert!(session
                .ravenous_validate_bound_decision_v51(current, actor.into(), &changed)
                .is_err());
            assert_eq!(gameplay(&session), before);
        }
        assert!(session
            .ravenous_validate_bound_decision_v51(current, actor.opponent().into(), &binding)
            .is_err());
        let snapshot = session.snapshot_v1();
        mana(&mut session, forests[0]);
        let revision = expected(&session).environment_revision;
        session.restore_v1(&snapshot);
        assert!(expected(&session).environment_revision > revision);
        assert_eq!(gameplay(&session), before);
        assert!(session
            .ravenous_validate_bound_decision_v51(current, actor.into(), &binding)
            .is_err());
        let mut foreign = snapshot.clone();
        foreign.0.episode_id += 1;
        let before = gameplay(&session);
        assert!(session.ravenous_restore_v51(&foreign).is_err());
        session.restore_v1(&foreign);
        assert_eq!(gameplay(&session), before);
        let mut old_state = crate::extra_turn_v1::tests::ready(actor);
        put(&mut old_state, actor, "Forest", Zone::Hand);
        let mut old = FastActorSessionV1::from_fake_v50_fixture_state(old_state);
        let before = gameplay(&old);
        old.restore_v1(&snapshot);
        assert_eq!(gameplay(&old), before);
        assert_eq!(
            old.flat_action_contract_mode,
            FlatActionContractModeV1::FakeV50
        );
    }
}

#[test]
fn full_amulet_keeps_legacy_visible_model_and_search_refusals_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (session, amulet, _, _) = fixture(actor);
        let definition = session.state.objects.get(amulet).card_def;
        assert_eq!(
            crate::card_def::CARD_DEFS[definition as usize].capability,
            crate::card_def::CardCapability::Full
        );
        assert!(crate::card_def::preflight_fully_supported_deck(&[definition]).is_ok());
        let before = serde_json::to_vec(&session.state).unwrap();
        assert!(session.clone().into_fake_v50().is_err());
        let e = expected(&session);
        assert!(session.flat_policy_observation_v1(e).is_err());
        assert!(session.flat_policy_observation_v2(e).is_err());
        assert!(session.flat_policy_observation_v3(e).is_err());
        assert!(session.flat_policy_observation_v4(e).is_err());
        assert!(session.flat_policy_observation_v5(e).is_err());
        assert!(session.kernel_search_redeterminized_clone_v4(1).is_err());
        assert!(crate::rl::observe_v1(&session.state, actor, 0).is_err());
        assert!(
            crate::rl::observe_v2(&session.state, session.surface.harness_surface(), actor, 0)
                .is_err()
        );
        assert!(
            crate::ravenous_amulet_v1::require_legacy_consumer(&session.state, Some(actor))
                .is_err()
        );
        let old = FastActorSessionV1::from_fake_v50_fixture_state(session.state.clone());
        assert!(matches!(
            old.current_response(),
            FastActorResponseV1::Terminal(_)
        ));
        assert_eq!(serde_json::to_vec(&old.state).unwrap(), before);
        assert_eq!(session.state.objects.get(amulet).zone, Zone::Hand);
    }
}

#[test]
fn known_library_and_newly_drawn_amulet_require_v51_before_legacy_publication_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = crate::extra_turn_v1::tests::ready(actor);
        let amulet = put(&mut state, actor, "Ravenous Amulet", Zone::Library);
        assert_eq!(state.players[actor.index()].library.pop(), Some(amulet));
        state.players[actor.index()].library.insert(0, amulet);
        let hidden = state.clone();
        assert!(crate::ravenous_amulet_v1::require_legacy_consumer(&state, Some(actor)).is_ok());
        state.reveal_library_top(actor, actor, 1);
        let before = state.clone();
        assert!(crate::ravenous_amulet_v1::require_legacy_consumer(&state, Some(actor)).is_err());
        assert!(
            crate::ravenous_amulet_v1::require_legacy_consumer(&state, Some(actor.opponent()))
                .is_ok()
        );
        assert!(crate::rl::observe_v1(&state, actor, 0).is_err());
        assert_eq!(state, before);

        // The printing starts hidden. Ordinary upkeep passes reach the real
        // draw before the legacy backend refuses to publish another menu.
        let mut before_draw = hidden;
        before_draw.step = crate::state::Step::Upkeep;
        let mut old = FastActorSessionV1::from_fake_v50_fixture_state(before_draw);
        // The starting player's first draw is skipped, so retain enough real
        // priority passes to reach that player's next eligible draw step.
        for _ in 0..128 {
            let FastActorResponseV1::Decision(current) = old.current_response() else {
                break;
            };
            let (_, actions, binding) = old
                .fake_current_decision_input_v50(current, current.acting_player)
                .unwrap();
            let index = actions
                .iter()
                .position(|action| matches!(action, ActionSemanticV1::Pass { .. }))
                .unwrap();
            old.fake_validate_bound_decision_v50(current, current.acting_player, &binding)
                .unwrap();
            old.step(current.episode_id, current.step, index as u32)
                .unwrap();
        }
        assert_eq!(old.state.objects.get(amulet).zone, Zone::Hand);
        assert!(matches!(
            old.current_response(),
            FastActorResponseV1::Terminal(_)
        ));
        assert!(old.current.is_none());
    }
}

#[test]
fn paid_grim_tutor_search_reveals_amulet_only_to_v51_and_restores_real_choice_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = crate::extra_turn_v1::tests::ready(actor);
        let amulet = put(&mut state, actor, "Ravenous Amulet", Zone::Library);
        let tutor = put(&mut state, actor, "Grim Tutor", Zone::Hand);
        let sources = [
            put(&mut state, actor, "Swamp", Zone::Battlefield),
            put(&mut state, actor, "Swamp", Zone::Battlefield),
            put(&mut state, actor, "Forest", Zone::Battlefield),
        ];
        let mut session = FastActorSessionV1::from_ravenous_v51_fixture_state(state);
        for source in sources {
            mana(&mut session, source);
        }
        act(&mut session, |action| {
            matches!(action,
            ActionSemanticV1::CastSpell { source, .. } if source.arena_id == tutor.0)
        });
        assert_eq!(
            session.state.players[actor.index()].mana_pool[ManaColor::B.pool_index()],
            0
        );
        assert_eq!(
            session.state.players[actor.index()].mana_pool[ManaColor::G.pool_index()],
            0
        );
        let mut found = false;
        for _ in 0..32 {
            let current = expected(&session);
            let (public, actions, _) = session
                .ravenous_current_decision_input_v51(current, current.acting_player)
                .unwrap();
            if actions.iter().any(|action| matches!(action,
                ActionSemanticV1::ChooseEffectTarget { target: crate::rl::TargetRefV1::Object { object }, .. }
                    if object.arena_id == amulet.0))
            {
                assert_eq!(current.acting_player, actor.into());
                assert!(public.extensions.decision_local_library.as_ref().unwrap().cards.iter().any(|card| card.stable.arena_id == amulet.0));
                found = true;
                break;
            }
            act(&mut session, |action| {
                matches!(action, ActionSemanticV1::Pass { .. })
            });
        }
        assert!(found, "actual paid tutor must reach its search choice");
        assert!(session.state.known_library_cards(actor, actor).is_empty());
        assert!(
            crate::ravenous_amulet_v1::require_legacy_consumer(&session.state, Some(actor)).is_ok()
        );
        let before = gameplay(&session);
        assert!(crate::rl::policy_observation_extensions_fake_v50(&session.state, actor).is_err());
        assert!(session.clone().into_fake_v50().is_err());
        assert_eq!(gameplay(&session), before);
        let snapshot = session.snapshot_v1();
        act(&mut session, |action| {
            matches!(action,
            ActionSemanticV1::ChooseEffectTarget { target: crate::rl::TargetRefV1::Object { object }, .. }
                if object.arena_id == amulet.0)
        });
        let public = drain(&mut session);
        assert_eq!(session.state.objects.get(amulet).zone, Zone::Hand);
        let finished = gameplay(&session);
        session.ravenous_restore_v51(&snapshot).unwrap();
        act(&mut session, |action| {
            matches!(action,
            ActionSemanticV1::ChooseEffectTarget { target: crate::rl::TargetRefV1::Object { object }, .. }
                if object.arena_id == amulet.0)
        });
        assert_eq!(drain(&mut session), public);
        assert_eq!(gameplay(&session), finished);
    }
}

fn assert_borrowed_common_parity(session: &FastActorSessionV1) {
    let before = gameplay(session);
    let current = session.current.as_ref().unwrap();
    let expected = expected(session);
    let (actual, actions, binding) = session
        .ravenous_current_decision_input_v51(expected, expected.acting_player)
        .unwrap();
    crate::elenda_public_v52::require_legacy_visible(&session.state, Some(current.actor)).unwrap();
    let (rebuilt, mut families) =
        crate::ravenous_observation_v51::observe_parts_complete_base_v52_fully_rebuilt(
            &session.state,
            &session.surface,
            (current.actor, &current.origin_decision),
            session.policy_step_count,
            current.physical_decision_id,
            (current.substep_index, current.substep_count),
        )
        .unwrap();
    assert_eq!(
        serde_json::to_vec(&actual).unwrap(),
        serde_json::to_vec(&rebuilt).unwrap()
    );
    let candidates = normalized_origin_with_parts(session, current).unwrap().0;
    let rebuilt_actions = candidates
        .iter()
        .map(|candidate| {
            visible_semantic(
                &session.state,
                &rebuilt.projection,
                candidate.semantic.clone(),
            )
        })
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(actions, rebuilt_actions);
    let classified = rebuilt_actions
        .iter()
        .cloned()
        .map(|semantic| super::super::fishing_pole_v39::classify(&session.state, semantic))
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    for family in &mut families {
        if let crate::akroma_family_public_v40::FamilyProjectionV40::Fishing(fishing) = family {
            fishing.actions = classified.clone();
        }
    }
    let rebuilt_binding = RavenousDecisionBindingV51 {
        expected,
        candidates,
        observation: rebuilt,
        families,
    };
    assert!(binding == rebuilt_binding);
    session
        .ravenous_validate_bound_decision_v51(expected, expected.acting_player, &rebuilt_binding)
        .unwrap();
    let mut forged = rebuilt_binding;
    forged.families.reverse();
    assert!(session
        .ravenous_validate_bound_decision_v51(expected, expected.acting_player, &forged)
        .is_err());
    assert_eq!(before, gameplay(session));
}

#[test]
fn borrowed_common_complete_paid_amulet_and_restored_binding_parity_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut session, amulet, _, forests) = fixture(actor);
        for forest in &forests[..2] {
            mana(&mut session, *forest);
        }
        act(
            &mut session,
            |action| matches!(action, ActionSemanticV1::CastSpell{source,..}if source.arena_id==amulet.0),
        );
        assert!(session.state.engine.pending_cast.is_none());
        assert_eq!(session.state.players[actor.index()].mana_pool, [0; 6]);
        assert!(forests[..2]
            .iter()
            .all(|id| session.state.objects.get(*id).tapped));
        assert_borrowed_common_parity(&session);
        let snapshot = session.snapshot_v1();
        drain(&mut session);
        assert_eq!(session.state.objects.get(amulet).zone, Zone::Battlefield);
        assert_borrowed_common_parity(&session);
        session.ravenous_restore_v51(&snapshot).unwrap();
        assert_borrowed_common_parity(&session);
        let before = gameplay(&session);
        let mut foreign = fixture(actor).0.snapshot_v1();
        // V51's raw backend binds snapshots to the episode, unlike the sealed
        // Prepared owner. Exercise its actual foreign-episode contract.
        foreign.0.episode_id += 1;
        assert!(session.ravenous_restore_v51(&foreign).is_err());
        assert_eq!(before, gameplay(&session));
    }
}
