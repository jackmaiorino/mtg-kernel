//! Paid engine choices through the single prepared owner. No printed status or
//! reset permission is inferred from the fixture-state constructor.
use super::*;
use crate::{
    ids::ObjectId,
    mana::{ManaColor, Pip},
    policy_observation_v6::tests::put,
    rl::TargetRefV1,
    state::{GameState, Zone},
};
fn ready(actor: PlayerId) -> GameState {
    let mut state = crate::extra_turn_v1::tests::ready(actor);
    for player in [actor, actor.opponent()] {
        put(&mut state, player, "Forest", Zone::Hand);
        for _ in 0..16 {
            put(&mut state, player, "Forest", Zone::Library);
        }
    }
    state
}
fn own(state: GameState) -> PreparedSessionV57 {
    PreparedSessionV57::from_session(FastActorSessionV1::from_prepared_v57_fixture_state(state))
        .unwrap()
}
fn state(session: &PreparedSessionV57) -> &GameState {
    session.fixture_state_v57()
}
fn gameplay(session: &PreparedSessionV57) -> (Vec<u8>, (u64, u64)) {
    (
        serde_json::to_vec(state(session)).unwrap(),
        session.fixture_counts_v57(),
    )
}
fn act(session: &mut PreparedSessionV57, choose: impl Fn(&ActionSemanticV1) -> bool) {
    let current = session.decision_v57().unwrap();
    let index = current
        .actions()
        .iter()
        .position(choose)
        .unwrap_or_else(|| panic!("real prepared action absent:{:?}", current.actions()));
    session.validate_decision_v57(&current).unwrap();
    let mut reference = session.inner.clone();
    let e = current.expected();
    let reply = session.step_v57(&current, index as u32).unwrap();
    assert_eq!(
        reply,
        reference.step(e.episode_id, e.step, index as u32).unwrap()
    );
    assert_eq!(
        session.inner.prepared_fixture_state(),
        reference.prepared_fixture_state()
    );
    assert_eq!(
        session.fixture_counts_v57(),
        (
            reference.policy_step_count(),
            reference.physical_decision_count()
        )
    );
}
fn drain(session: &mut PreparedSessionV57) {
    for _ in 0..64 {
        let decision = session.decision_v57().unwrap();
        let s = state(session);
        if s.stack.is_empty()
            && s.engine.pending_cast.is_none()
            && s.engine.pending_activation.is_none()
            && s.engine.pending_effect.is_none()
            && s.engine.pending_triggers.is_empty()
        {
            return;
        }
        act(session, |a| matches!(a, ActionSemanticV1::Pass { .. }));
        assert!(!decision.actions().is_empty());
    }
    panic!("genuine prepared work did not settle")
}
fn cast_setup(actor: PlayerId, name: &str) -> (PreparedSessionV57, ObjectId) {
    let mut raw = ready(actor);
    let spell = put(&mut raw, actor, name, Zone::Hand);
    let d = &crate::card_def::CARD_DEFS[usize::from(raw.objects.get(spell).card_def)];
    let mut colors = vec![ManaColor::G; usize::from(d.cost.generic)];
    for pip in d.cost.pips {
        let Pip::Colored(color) = pip else {
            panic!("prepared fixture is not an ordinary colored cost")
        };
        colors.push(*color);
    }
    let lands = colors
        .into_iter()
        .map(|color| {
            put(
                &mut raw,
                actor,
                match color {
                    ManaColor::W => "Plains",
                    ManaColor::U => "Island",
                    ManaColor::B => "Swamp",
                    ManaColor::R => "Mountain",
                    ManaColor::G => "Forest",
                    ManaColor::C => panic!("no colorless pip"),
                },
                Zone::Battlefield,
            )
        })
        .collect::<Vec<_>>();
    let mut session = own(raw);
    for land in lands {
        act(
            &mut session,
            |a| matches!(a,ActionSemanticV1::ActivateManaAbility{source,..}if source.arena_id==land.0),
        );
    }
    act(
        &mut session,
        |a| matches!(a,ActionSemanticV1::CastSpell{source,..}if source.arena_id==spell.0),
    );
    assert_eq!(state(&session).players[actor.index()].mana_pool, [0; 6]);
    drain(&mut session);
    assert_eq!(state(&session).objects.get(spell).zone, Zone::Battlefield);
    (session, spell)
}
#[test]
fn all_targetless_prepared_printings_really_pay_cast_and_keep_distinct_compatibility_schemas_both_seats(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for name in [
            "Elenda, Saint of Dusk",
            "Drake Hatcher",
            "Spinner of Souls",
            "Painful Quandary",
            "Flamewake Phoenix",
            "Bloodthirsty Conqueror",
            "Doubling Season",
            "Ravenous Amulet",
        ] {
            let (mut session, _) = cast_setup(actor, name);
            let decision = session.decision_v57().unwrap();
            let observation = decision.observation();
            crate::prepared_observation_v57::validate_artifact(observation).unwrap();
            assert_ne!(
                observation.common.schema,
                crate::elenda_observation_v52::SCHEMA
            );
            assert_ne!(
                observation.common.common.schema,
                crate::ravenous_observation_v51::SCHEMA
            );
            assert!(observation.common.common.validate_artifact().is_err());
            assert_eq!(observation.action_meanings.len(), decision.actions().len());
            let original = gameplay(&session);
            let saved = session.snapshot_v57();
            let revision = decision.expected().environment_revision;
            session.restore_v57(&saved).unwrap();
            assert!(
                session
                    .decision_v57()
                    .unwrap()
                    .expected()
                    .environment_revision
                    > revision
            );
            assert!(session.step_v57(&decision, 0).is_err());
            assert_eq!(gameplay(&session), original);
        }
    }
}
#[test]
fn real_paid_hatcher_combat_debit_activation_counter_and_token_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (paid, hatcher) = cast_setup(actor, "Drake Hatcher");
        let mut raw = state(&paid).clone();
        // Existing production combat commit boundary, not a fabricated trigger.
        // This is a final damage batch operand, not a declaration fixture.
        crate::engine::commit_combat_damage_events(
            &mut raw,
            vec![crate::event::ProposedEvent::damage(
                hatcher,
                crate::state::Target::Player(actor.opponent()),
                3,
            )],
        );
        let mut session = own(raw);
        drain(&mut session);
        assert_eq!(state(&session).objects.get(hatcher).counters.incubation, 3);
        if session.decision_v57().unwrap().expected().acting_player != actor.into() {
            act(&mut session, |a| matches!(a, ActionSemanticV1::Pass { .. }));
        }
        let decision = session.decision_v57().unwrap();
        let index=decision.actions().iter().position(|a|matches!(a,ActionSemanticV1::ActivateAbility{source,ability_index:0,..}if source.arena_id==hatcher.0)).unwrap();
        let annotation = decision.observation().action_meanings[index]
            .as_ref()
            .unwrap();
        assert_eq!(
            (annotation.count, annotation.before, annotation.after),
            (3, 3, 0)
        );
        assert!(annotation.costs_paid_before_stack);
        assert!(!annotation.countering_refunds_costs);
        let saved = session.snapshot_v57();
        let mut forged = decision.clone();
        forged.observation.action_meanings[index]
            .as_mut()
            .unwrap()
            .count = 2;
        let original = gameplay(&session);
        assert!(session.step_v57(&forged, index as u32).is_err());
        assert_eq!(gameplay(&session), original);
        act(
            &mut session,
            |a| matches!(a,ActionSemanticV1::ActivateAbility{source,ability_index:0,..}if source.arena_id==hatcher.0),
        );
        assert_eq!(state(&session).objects.get(hatcher).counters.incubation, 0);
        assert_eq!(
            session
                .decision_v57()
                .unwrap()
                .observation()
                .drake
                .activations
                .len(),
            1
        );
        let mut countered = state(&session).clone();
        let item = countered.stack.last().unwrap().v4.stack_item_id;
        crate::engine::counter_stack_item_by_id(&mut countered, item).unwrap();
        assert_eq!(countered.objects.get(hatcher).counters.incubation, 0);
        let countered = own(countered);
        assert!(matches!(
            countered
                .decision_v57()
                .unwrap()
                .observation()
                .drake
                .activations[0]
                .outcome,
            crate::drake_public_v53::ActivationOutcomeV53::Countered
        ));
        drain(&mut session);
        assert_eq!(
            state(&session)
                .objects
                .iter()
                .filter(|(_, o)| o.name == "Drake Token" && o.zone == Zone::Battlefield)
                .count(),
            1
        );
        let result = gameplay(&session);
        session.restore_v57(&saved).unwrap();
        assert!(session.step_v57(&decision, index as u32).is_err());
        act(
            &mut session,
            |a| matches!(a,ActionSemanticV1::ActivateAbility{source,ability_index:0,..}if source.arena_id==hatcher.0),
        );
        drain(&mut session);
        assert_eq!(gameplay(&session), result);
    }
}
#[test]
fn full_binding_wide_roster_hidden_hand_foreign_snapshot_and_current_forgery_refuse_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut raw = ready(actor);
        let wide = put(&mut raw, actor, "Forest", Zone::Battlefield);
        raw.objects.get_mut(wide).counters.plus1_plus1 = i32::MAX;
        raw.objects.get_mut(wide).counters.soul = i32::MAX;
        raw.objects.get_mut(wide).counters.incubation = i32::MAX;
        raw.objects.get_mut(wide).damage = u32::MAX;
        let hidden = put(&mut raw, actor.opponent(), "Spinner of Souls", Zone::Hand);
        let mut alternative = raw.clone();
        alternative.objects.get_mut(hidden).card_def =
            crate::card_def::card_id_by_name("Painful Quandary").unwrap();
        alternative.objects.get_mut(hidden).name = "Painful Quandary".into();
        let mut session = own(raw);
        let other = own(alternative);
        let decision = session.decision_v57().unwrap();
        assert_eq!(
            decision.observation(),
            other.decision_v57().unwrap().observation()
        );
        assert_eq!(decision.actions(), other.decision_v57().unwrap().actions());
        let card = decision
            .observation()
            .drake
            .battlefield
            .iter()
            .flatten()
            .find(|row| row.reference.incarnation.object == wide.0)
            .unwrap();
        assert_eq!(
            (
                card.counters.plus_one,
                card.counters.soul,
                card.counters.incubation,
                card.marked_damage
            ),
            (i32::MAX, i32::MAX, i32::MAX, u32::MAX)
        );
        let original = gameplay(&session);
        assert!(session.restore_v57(&other.snapshot_v57()).is_err());
        assert_eq!(gameplay(&session), original);
        let mut forged = decision.clone();
        forged.binding.candidates.reverse();
        assert!(session.step_v57(&forged, 0).is_err());
        assert_eq!(gameplay(&session), original);
        let mut forged = decision.clone();
        for row in &mut forged.observation.drake.battlefield {
            row.clear();
        }
        assert!(session.step_v57(&forged, 0).is_err());
        assert_eq!(gameplay(&session), original);
        let current = session.inner.current.as_ref().unwrap();
        assert!(
            flat_validate_expected_decision_v1(&session.inner, current, decision.expected())
                .is_err()
        );
    }
}
#[test]
fn paid_quandary_opposing_spell_binds_payer_not_source_controller_and_decline_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (paid, source) = cast_setup(actor, "Painful Quandary");
        let mut raw = state(&paid).clone();
        let payer = actor.opponent();
        let bolt = put(&mut raw, payer, "Lightning Bolt", Zone::Hand);
        let mountain = put(&mut raw, payer, "Mountain", Zone::Battlefield);
        let life = raw.players[payer.index()].life;
        let mut session = own(raw);
        if session.decision_v57().unwrap().expected().acting_player != payer.into() {
            act(&mut session, |a| matches!(a, ActionSemanticV1::Pass { .. }));
        }
        act(
            &mut session,
            |a| matches!(a,ActionSemanticV1::ActivateManaAbility{source,..}if source.arena_id==mountain.0),
        );
        act(
            &mut session,
            |a| matches!(a,ActionSemanticV1::CastSpell{source,..}if source.arena_id==bolt.0),
        );
        act(
            &mut session,
            |a| matches!(a,ActionSemanticV1::ChooseTarget{target:TargetRefV1::Player{player},..}if *player==actor.into()),
        );
        for _ in 0..32 {
            let decision = session.decision_v57().unwrap();
            if decision
                .actions()
                .iter()
                .any(|a| matches!(a, ActionSemanticV1::FinishEffectSelection { .. }))
            {
                break;
            }
            act(&mut session, |a| matches!(a, ActionSemanticV1::Pass { .. }));
        }
        let decision = session.decision_v57().unwrap();
        assert_eq!(decision.expected().acting_player, payer.into());
        assert_eq!(state(&session).objects.get(source).controller, actor);
        assert!(decision.observation().quandary.stack.iter().any(|row|matches!(row.stage,crate::quandary_public_v55::StageV55::AwaitingPayment{payer:actual,minimum:0,maximum:1,payer_hand:Some(_)}if actual==payer.into())));
        let saved = session.snapshot_v57();
        act(&mut session, |a| {
            matches!(a, ActionSemanticV1::FinishEffectSelection { .. })
        });
        drain(&mut session);
        assert_eq!(state(&session).players[payer.index()].life, life - 5);
        let result = gameplay(&session);
        session.restore_v57(&saved).unwrap();
        assert!(session.step_v57(&decision, 0).is_err());
        act(&mut session, |a| {
            matches!(a, ActionSemanticV1::FinishEffectSelection { .. })
        });
        drain(&mut session);
        assert_eq!(gameplay(&session), result);
    }
}

#[test]
fn simultaneous_identical_spinner_occurrences_keep_real_order_and_captured_source_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (paid, spinner) = cast_setup(actor, "Spinner of Souls");
        let mut raw = state(&paid).clone();
        let elves = [
            put(&mut raw, actor, "Llanowar Elves", Zone::Battlefield),
            put(&mut raw, actor, "Llanowar Elves", Zone::Battlefield),
        ];
        crate::event::propose_and_commit_batch(
            &mut raw,
            elves
                .into_iter()
                .map(|object| crate::event::ProposedEvent::zone_change(object, Zone::Graveyard))
                .collect(),
        );
        let mut session = own(raw);
        let decision = session.decision_v57().unwrap();
        assert_eq!(decision.observation().spinner.held.len(), 2);
        assert_eq!(
            decision.observation().spinner.held[0],
            decision.observation().spinner.held[1]
        );
        let order=decision.actions().iter().position(|action|matches!(action,ActionSemanticV1::OrderTriggers{order,..}if order==&vec![1,0])).unwrap();
        let saved = session.snapshot_v57();
        let mut reference = session.inner.clone();
        let expected = decision.expected();
        assert_eq!(
            session.step_v57(&decision, order as u32).unwrap(),
            reference
                .step(expected.episode_id, expected.step, order as u32)
                .unwrap()
        );
        assert_eq!(
            gameplay(&session).0,
            serde_json::to_vec(reference.prepared_fixture_state()).unwrap()
        );
        let after = gameplay(&session);
        session.restore_v57(&saved).unwrap();
        assert!(session.step_v57(&decision, order as u32).is_err());
        let current = session.decision_v57().unwrap();
        session.step_v57(&current, order as u32).unwrap();
        assert_eq!(gameplay(&session), after);
        let rows = &state(&session).stack;
        assert_eq!(rows.len(), 2);
        for item in rows {
            let captured =
                crate::prepared_observation_v57::stack_source_reference(state(&session), item)
                    .unwrap()
                    .unwrap();
            assert_eq!(captured.arena_id, spinner.0);
            assert_eq!(captured.zone, Zone::Battlefield);
            let mut forged = item.clone();
            forged.controller = actor.opponent();
            assert!(crate::prepared_observation_v57::stack_source_reference(
                state(&session),
                &forged
            )
            .is_err());
        }
        // A real departure operand after publication keeps the original
        // battlefield identity; no bounce spell or historical record is invented.
        let mut moved = state(&session).clone();
        crate::event::propose_and_commit(
            &mut moved,
            crate::event::ProposedEvent::zone_change(spinner, Zone::Hand),
        );
        let moved = own(moved);
        assert!(moved
            .decision_v57()
            .unwrap()
            .observation()
            .spinner
            .current
            .is_empty());
        for item in &state(&moved).stack {
            let captured =
                crate::prepared_observation_v57::stack_source_reference(state(&moved), item)
                    .unwrap()
                    .unwrap();
            assert_eq!(captured.arena_id, spinner.0);
            assert_eq!(captured.zone, Zone::Battlefield);
            assert!(
                captured.zone_change_count < state(&moved).objects.get(spinner).zone_change_count
            );
        }
    }
}

// Both assemblies execute the complete fresh observation and binding guards.
// Only the 24 later common constructions differ (borrowed versus rebuilt).
fn assert_borrowed_common_parity(session: &PreparedSessionV57) {
    let before = gameplay(session);
    let current = session.inner.current.as_ref().unwrap();
    let actual = session.decision_v57().unwrap();
    let (rebuilt, mut families) = crate::prepared_observation_v57::observe_parts_v57_fully_rebuilt(
        &session.inner.state,
        &session.inner.surface,
        (current.actor, &current.origin_decision),
        session.inner.policy_step_count,
        current.physical_decision_id,
        (current.substep_index, current.substep_count),
    )
    .unwrap();
    assert_eq!(
        serde_json::to_vec(actual.observation()).unwrap(),
        serde_json::to_vec(&rebuilt).unwrap()
    );
    let candidates = normalized_origin_parts(&session.inner, current).unwrap().0;
    let actions = candidates
        .iter()
        .map(|candidate| {
            visible_semantic(
                &session.inner.state,
                &rebuilt.common.common.projection,
                candidate.semantic.clone(),
            )
        })
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(actual.actions(), actions.as_slice());
    let classified = actions
        .iter()
        .cloned()
        .map(|semantic| super::super::fishing_pole_v39::classify(&session.inner.state, semantic))
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    for family in &mut families {
        if let crate::akroma_family_public_v40::FamilyProjectionV40::Fishing(fishing) = family {
            fishing.actions = classified.clone();
        }
    }
    let binding = PreparedDecisionBindingV57 {
        expected: actual.expected(),
        candidates,
        observation: rebuilt,
        families,
    };
    assert!(binding == actual.binding);
    let mut rebuilt_decision = actual.clone();
    rebuilt_decision.binding = binding;
    session.validate_decision_v57(&rebuilt_decision).unwrap();
    let mut forged = rebuilt_decision;
    forged.binding.families.reverse();
    assert!(session.validate_decision_v57(&forged).is_err());
    assert_eq!(before, gameplay(session));
}

#[test]
fn borrowed_common_complete_paid_faebloom_and_restored_binding_parity_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut raw = ready(actor);
        let spell = put(&mut raw, actor, "Faebloom Trick", Zone::Hand);
        put(
            &mut raw,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        let lands: [_; 3] =
            std::array::from_fn(|_| put(&mut raw, actor, "Island", Zone::Battlefield));
        let mut session = own(raw);
        for land in lands {
            act(
                &mut session,
                |a| matches!(a,ActionSemanticV1::ActivateManaAbility{source,..}if source.arena_id==land.0),
            );
        }
        act(
            &mut session,
            |a| matches!(a,ActionSemanticV1::CastSpell{source,..}if source.arena_id==spell.0),
        );
        assert!(state(&session).engine.pending_cast.is_none());
        assert!(lands
            .iter()
            .all(|land| state(&session).objects.get(*land).tapped));
        assert_eq!(state(&session).players[actor.index()].mana_pool, [0; 6]);
        assert_borrowed_common_parity(&session);
        let mut reached_targets = false;
        for _ in 0..16 {
            let current = session.decision_v57().unwrap();
            if current
                .actions()
                .iter()
                .any(|a| matches!(a, ActionSemanticV1::ChooseTarget { .. }))
            {
                reached_targets = true;
                break;
            }
            act(&mut session, |a| matches!(a, ActionSemanticV1::Pass { .. }));
        }
        assert!(reached_targets);
        assert_eq!(state(&session).objects.get(spell).zone, Zone::Graveyard);
        assert_eq!(
            state(&session).players[actor.index()]
                .battlefield
                .iter()
                .filter(|id| state(&session).objects.get(**id).card_def
                    == crate::card_def::card_id_by_name("Faerie Token").unwrap())
                .count(),
            2
        );
        assert_borrowed_common_parity(&session);
        let saved = session.snapshot_v57();
        let stale = session.decision_v57().unwrap();
        act(&mut session, |a| {
            matches!(a, ActionSemanticV1::FinishTargetSelection { .. })
        });
        drain(&mut session);
        session.restore_v57(&saved).unwrap();
        assert_borrowed_common_parity(&session);
        let before = gameplay(&session);
        assert!(session.step_v57(&stale, 0).is_err());
        let foreign = own(state(&session).clone());
        assert!(session.restore_v57(&foreign.snapshot_v57()).is_err());
        assert_eq!(before, gameplay(&session));
    }
}
