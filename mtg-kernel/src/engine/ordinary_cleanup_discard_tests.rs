//! Source-only regression for CR514.1 before514.2. Initial lands/hand and the
//! opponent creature are native setup; control and its expiry must come solely
//! from the actual paid Involuntary Employment spell and ordinary turn advance.
use super::*;
use crate::{card_def::CardCapability, policy_observation_v6::tests::put};

#[test]
fn genuine_paid_employment_control_expires_after_issued_cleanup_discard_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = crate::extra_turn_v1::tests::ready(actor);
        assert!(state.players[actor.index()].hand.is_empty());
        let employment = put(&mut state, actor, "Involuntary Employment", Zone::Hand);
        let remaining = (0..8)
            .map(|_| put(&mut state, actor, "Forest", Zone::Hand))
            .collect::<Vec<_>>();
        let lands = (0..4)
            .map(|_| put(&mut state, actor, "Mountain", Zone::Battlefield))
            .collect::<Vec<_>>();
        let archer = put(
            &mut state,
            actor.opponent(),
            "Firebrand Archer",
            Zone::Battlefield,
        );
        assert_eq!(state.players[actor.index()].hand.len(), 9);
        assert_eq!(state.objects.get(employment).card_def, 410);
        assert_eq!(state.objects.get(archer).card_def, 261);
        for object in [employment, archer] {
            assert_eq!(
                card_def::CARD_DEFS[usize::from(state.objects.get(object).card_def)].capability,
                CardCapability::Full
            );
        }
        let original_generation = state.objects.get(archer).zone_change_count;
        assert!(matches!(advance_until_decision(&mut state),
            Decision::CastSpellOrPass { player, castable_spells, .. }
                if player == actor && castable_spells.contains(&employment)));
        step(&mut state, Action::CastSpell(employment)).unwrap();
        assert!(matches!(advance_until_decision(&mut state),
            Decision::ChooseTargets { player, legal_targets, .. }
                if player == actor && legal_targets.contains(&Target::Object(archer))));
        step(&mut state, Action::ChooseTarget(Target::Object(archer))).unwrap();
        assert!(matches!(advance_until_decision(&mut state),
            Decision::CastSpellOrPass { player, .. } if player == actor));
        assert!(state.engine.pending_cast.is_none());
        let row = state.stack.last().unwrap();
        assert_eq!(row.source, employment);
        assert_eq!(row.controller, actor);
        assert_eq!(row.targets, vec![Target::Object(archer)]);
        assert_eq!(state.objects.get(employment).zone, Zone::Stack);
        assert!(lands.iter().all(|&id| state.objects.get(id).tapped));
        assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
        assert_eq!(state.players[actor.index()].hand, remaining);
        for _ in 0..32 {
            let decision = advance_until_decision(&mut state);
            if state.stack.is_empty()
                && state.engine.pending_effect.is_none()
                && state.engine.pending_triggers.is_empty()
            {
                break;
            }
            crate::graveyard_return_public_v61::tests::quiet(&mut state, decision);
        }
        assert_eq!(state.objects.get(employment).zone, Zone::Graveyard);
        assert_eq!(state.objects.get(archer).controller, actor);
        assert_eq!(state.objects.get(archer).owner, actor.opponent());
        assert_eq!(
            state.objects.get(archer).zone_change_count,
            original_generation
        );
        let mut issued = None;
        for _ in 0..240 {
            let decision = advance_until_decision(&mut state);
            if let Decision::Discard {
                player,
                count,
                choices,
            } = decision
            {
                assert_eq!(player, actor);
                assert_eq!(count, 1);
                assert_eq!(choices, remaining);
                issued = Some(choices);
                break;
            }
            crate::graveyard_return_public_v61::tests::quiet(&mut state, decision);
        }
        let choices = issued.expect("ordinary cleanup must issue the eight-card discard");
        assert_eq!(state.active_player, actor);
        assert_eq!(state.players[actor.index()].hand.len(), 8);
        // CR514.1 happens before514.2. The old source expires control too early.
        assert_eq!(state.objects.get(archer).controller, actor);
        assert_eq!(state.objects.get(archer).zone, Zone::Battlefield);
        assert_eq!(
            state.objects.get(archer).zone_change_count,
            original_generation
        );
        assert!(state.players[actor.index()].battlefield.contains(&archer));
        assert!(!state.players[actor.opponent().index()]
            .battlefield
            .contains(&archer));
        assert!(!state
            .engine
            .event_history
            .iter()
            .any(|e| matches!(e, event::CommittedEvent::TemporaryControlExpiredV1 { .. })));
        let discarded = choices[0];
        let generation = state.objects.get(discarded).zone_change_count;
        let before_discard = state.engine.event_history.len();
        step(&mut state, Action::Discard(vec![discarded])).unwrap();
        for _ in 0..32 {
            let decision = advance_until_decision(&mut state);
            if state.objects.get(archer).controller == actor.opponent() {
                break;
            }
            crate::graveyard_return_public_v61::tests::quiet(&mut state, decision);
        }
        assert!(state.engine.halted.is_none());
        assert_eq!(state.players[actor.index()].hand.len(), 7);
        assert_eq!(state.objects.get(discarded).zone, Zone::Graveyard);
        assert_eq!(
            state.objects.get(discarded).zone_change_count,
            generation + 1
        );
        assert_eq!(state.objects.get(archer).controller, actor.opponent());
        assert_eq!(state.objects.get(archer).owner, actor.opponent());
        assert_eq!(
            state.objects.get(archer).zone_change_count,
            original_generation
        );
        assert!(state.players[actor.opponent().index()]
            .battlefield
            .contains(&archer));
        assert!(!state.players[actor.index()].battlefield.contains(&archer));
        let discard_index = state
            .engine
            .event_history
            .iter()
            .enumerate()
            .skip(before_discard)
            .find_map(|(i, e)| {
                matches!(e, event::CommittedEvent::ZoneChange {
                object, from: Zone::Hand, to: Zone::Graveyard, ..
            } if *object == discarded)
                .then_some(i)
            })
            .unwrap();
        let expiries = state
            .engine
            .event_history
            .iter()
            .enumerate()
            .filter_map(|(i, e)| {
                if let event::CommittedEvent::TemporaryControlExpiredV1 {
                    installation_history_index,
                    cleanup_history_index,
                } = e
                {
                    Some((i, *installation_history_index, *cleanup_history_index))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(expiries.len(), 1);
        let (expired_index, installation_index, cleanup_index) = expiries[0];
        assert!(installation_index < before_discard);
        assert!(matches!(
            state.engine.event_history.get(installation_index),
            Some(event::CommittedEvent::TemporaryControlInstalledV1 { .. })
        ));
        assert!(matches!(state.engine.event_history.get(cleanup_index),
            Some(event::CommittedEvent::TemporaryControlCleanupV1 { active_player, .. }) if *active_player == actor));
        assert!(discard_index < cleanup_index && cleanup_index < expired_index);
    }
}

#[test]
fn genuine_paid_cleanup_death_priority_new_growth_repeat_and_restored_origin_both_seats() {
    fn quiet(state: &mut GameState, decision: Decision) {
        crate::graveyard_return_public_v61::tests::quiet(state, decision);
    }
    fn next(state: &mut GameState) -> Decision {
        let d = advance_until_decision(state);
        assert!(state.engine.halted.is_none(), "{:?}", state.engine.halted);
        assert!(!matches!(d, Decision::Halted { .. }), "{d:?}");
        d
    }
    fn paid(
        state: &mut GameState,
        actor: PlayerId,
        name: &str,
        target: Option<ObjectId>,
        cost: usize,
    ) -> ObjectId {
        let card = put(state, actor, name, Zone::Hand);
        let taps = state.players[actor.index()]
            .battlefield
            .iter()
            .filter(|o| state.objects.get(**o).tapped)
            .count();
        assert!(
            matches!(next(state),Decision::CastSpellOrPass{player,castable_spells,..} if player==actor && castable_spells.contains(&card))
        );
        step(state, Action::CastSpell(card)).unwrap();
        if let Some(target) = target {
            assert!(
                matches!(next(state),Decision::ChooseTargets{legal_targets,..} if legal_targets.contains(&Target::Object(target)))
            );
            step(state, Action::ChooseTarget(Target::Object(target))).unwrap();
        }
        assert!(matches!(next(state),Decision::CastSpellOrPass{player,..} if player==actor));
        assert!(state.engine.pending_cast.is_none());
        assert_eq!(
            state.players[actor.index()]
                .battlefield
                .iter()
                .filter(|o| state.objects.get(**o).tapped)
                .count()
                - taps,
            cost
        );
        assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
        assert!(state
            .stack
            .iter()
            .any(|r| r.source == card && r.kind == StackItemKind::Spell));
        for _ in 0..32 {
            let d = next(state);
            if state.stack.is_empty() && state.engine.pending_triggers.is_empty() {
                return card;
            }
            quiet(state, d);
        }
        panic!("paid spell failed to finish")
    }
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = crate::extra_turn_v1::tests::ready(actor);
        for name in ["Forest", "Mountain"] {
            for _ in 0..4 {
                put(&mut state, actor, name, Zone::Battlefield);
            }
        }
        for _ in 0..6 {
            put(&mut state, actor, "Island", Zone::Battlefield);
        }
        let clock = paid(&mut state, actor, "Clockwork Percussionist", None, 1);
        let target = put(&mut state, actor, "Axgard Cavalry", Zone::Battlefield);
        paid(&mut state, actor, "Giant Growth", Some(clock), 1);
        // Native counter setup, not a claimed paid counter-producing spell.
        // The real +3/+3 expires first, then the printed dies ability triggers.
        state.objects.get_mut(clock).counters.minus1_minus1 = 2;
        let mut found = false;
        for _ in 0..240 {
            let d = next(&mut state);
            if state.step == Step::Cleanup
                && ordinary_cleanup_v1::at_priority(&state)
                && !state.stack.is_empty()
            {
                assert_eq!(state.objects.get(clock).zone, Zone::Graveyard);
                assert!(matches!(d,Decision::CastSpellOrPass{player,..} if player==actor));
                found = true;
                break;
            }
            quiet(&mut state, d);
        }
        assert!(
            found,
            "ordinary cleanup did not issue actual trigger priority"
        );
        assert!(!state.engine.ordinary_cleanup_v1.is_empty());
        assert!(state.engine.time_stop_v1.is_empty());
        assert!(!state
            .engine
            .event_history
            .has_any(&[crate::event_history_v1::EventKindV1::TimeStopJournalV1]));
        for viewer in [actor, actor.opponent()] {
            let public = crate::rl::observe_v1(&state, viewer, 0).unwrap();
            assert!(!serde_json::to_string(&public)
                .unwrap()
                .contains("ordinary_cleanup_v1"));
        }
        let mut replay: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        assert_eq!(state, replay);
        assert_eq!(state.state_hash(), replay.state_hash());
        // A genuine paid Time Stop can replace this actual ordinary cleanup
        // priority window. Its separate replay journal closes the scalar step;
        // the discarded Clockwork trigger is not relabelled as countered.
        let mut ended = state.clone();
        let library_before = ended.players[actor.index()].library.len();
        let stop = paid(&mut ended, actor, "Time Stop", None, 6);
        assert_eq!(ended.objects.get(stop).zone, Zone::Exile);
        assert_eq!(ended.players[actor.index()].library.len(), library_before);
        assert!(ended.engine.ordinary_cleanup_v1.is_empty());
        assert!(ended.engine.time_stop_v1.is_empty());
        ordinary_cleanup_v1::validate(&ended).unwrap();
        time_stop_v1::validate(&ended).unwrap();
        let restored_ended: GameState =
            serde_json::from_slice(&serde_json::to_vec(&ended).unwrap()).unwrap();
        assert_eq!(ended, restored_ended);
        ordinary_cleanup_v1::validate(&restored_ended).unwrap();
        time_stop_v1::validate(&restored_ended).unwrap();
        let original_library = state.players[actor.index()].library.len();
        for branch in [&mut state, &mut replay] {
            paid(branch, actor, "Giant Growth", Some(target), 1);
            assert_eq!(checked_effective_pt(branch, target).unwrap(), (5, 5));
            for _ in 0..64 {
                let d = next(branch);
                if branch.active_player != actor {
                    assert_eq!(checked_effective_pt(branch, target).unwrap(), (2, 2));
                    assert_eq!(
                        branch.players[actor.index()].library.len(),
                        original_library - 1
                    );
                    assert!(branch.engine.time_stop_v1.is_empty());
                    assert!(branch.engine.ordinary_cleanup_v1.is_empty());
                    break;
                }
                quiet(branch, d);
            }
            assert_eq!(branch.active_player, actor.opponent());
            time_stop_v1::validate(branch).unwrap();
        }
        assert_eq!(state, replay);
        // Ordinary cleanup never creates Time Stop disposal or a retained
        // private-world history record.
        assert!(!state
            .engine
            .event_history
            .has_any(&[crate::event_history_v1::EventKindV1::TimeStopJournalV1]));
    }
}
