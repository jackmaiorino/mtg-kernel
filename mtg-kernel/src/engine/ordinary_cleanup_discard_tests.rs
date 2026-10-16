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
