//! Genuine Partial Embercleave source fixtures. Lands and vanilla creatures
//! are setup operands; casts, attacker declarations and equip are paid actions.
use super::*;
use crate::{
    card_def::{card_id_by_name, DynamicCountDef},
    event::ProposedEvent,
    policy_observation_v6::tests::put,
};

fn count(state: &GameState, player: PlayerId) -> u32 {
    normal_cast_reduction_count(
        DynamicCountDef::ControllerAttackingCreatures,
        player,
        &[],
        state,
    )
}
fn next(state: &mut GameState) -> Decision {
    let value = advance_until_decision(state);
    assert!(!matches!(value, Decision::Halted { .. }), "{value:?}");
    value
}
fn quiet(state: &mut GameState) {
    match next(state) {
        Decision::CastSpellOrPass { .. } => step(state, Action::Pass).unwrap(),
        Decision::DeclareBlockers { .. } => {
            step(state, Action::DeclareBlockers(Vec::new())).unwrap()
        }
        other => panic!("unexpected actual fixture choice: {other:?}"),
    }
}
fn settle(state: &mut GameState) {
    for _ in 0..32 {
        if matches!(next(state), Decision::CastSpellOrPass { .. })
            && state.stack.is_empty()
            && state.engine.pending_triggers.is_empty()
        {
            return;
        }
        quiet(state);
    }
    panic!("native equipment stack did not settle");
}

#[test]
fn current_attacker_cost_count_tracks_native_declaration_departure_control_and_combat_end_both_seats(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = crate::extra_turn_v1::tests::ready(actor);
        let first = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        let second = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        state.step = Step::DeclareAttackers;
        assert_eq!(count(&state, actor), 0);
        assert!(
            matches!(next(&mut state), Decision::DeclareAttackers { player, .. } if player == actor)
        );
        step(&mut state, Action::DeclareAttackers(vec![first, second])).unwrap();
        assert_eq!(count(&state, actor), 2);
        assert_eq!(count(&state, actor.opponent()), 0);
        let declared = state.clone();
        let restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&declared).unwrap()).unwrap();
        assert_eq!(restored, declared);
        assert_eq!(count(&restored, actor), 2);

        // Real control transfer removes the old attacker from combat. It
        // cannot become an attacking creature of the new controller.
        crate::standard_cards_v1::gain_control(&mut state, second, actor.opponent());
        assert_eq!(count(&state, actor), 1);
        assert_eq!(count(&state, actor.opponent()), 0);
        // Labelled departure/reentry primitive: the same physical ID returns,
        // but its new battlefield incarnation has never attacked.
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(first, Zone::Hand));
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(first, Zone::Battlefield),
        );
        assert_eq!(count(&state, actor), 0);
        state = restored;
        for _ in 0..32 {
            if state.step == Step::Main2 {
                break;
            }
            quiet(&mut state);
        }
        assert_eq!(state.step, Step::Main2);
        assert_eq!(count(&state, actor), 0);
        assert!(state.engine.combat.attackers.is_empty());
    }
}

#[test]
fn genuine_paid_flash_attach_equip_and_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for attackers in [0_usize, 1, 4, 6] {
            let mut state = crate::extra_turn_v1::tests::ready(actor);
            let source = put(&mut state, actor, "Embercleave", Zone::Hand);
            let host = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
            let alternate = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
            let attacking = (0..attackers)
                .map(|_| put(&mut state, actor, "Llanowar Elves", Zone::Battlefield))
                .collect::<Vec<_>>();
            let lands = (0..9)
                .map(|_| put(&mut state, actor, "Mountain", Zone::Battlefield))
                .collect::<Vec<_>>();
            state.step = Step::DeclareAttackers;
            assert!(matches!(
                next(&mut state),
                Decision::DeclareAttackers { .. }
            ));
            step(&mut state, Action::DeclareAttackers(attacking)).unwrap();
            assert_eq!(count(&state, actor), attackers as u32);
            let def = &card_def::CARD_DEFS[usize::from(card_id_by_name("Embercleave").unwrap())];
            assert_eq!(card_id_by_name("Embercleave"), Some(462));
            assert_eq!(def.capability, card_def::CardCapability::Partial);
            assert_eq!(def.types, &[CardType::Artifact]);
            assert!(def.keywords.has(Keywords::FLASH));
            let cost = effective_normal_cast_cost(def, actor, &state);
            assert_eq!(cost.generic, 4_u8.saturating_sub(attackers as u8));
            assert_eq!(
                cost.pips,
                &[
                    mana::Pip::Colored(mana::ManaColor::R),
                    mana::Pip::Colored(mana::ManaColor::R)
                ]
            );
            let due = usize::from(cost.generic) + 2;
            for land in &lands[..due] {
                step(&mut state, Action::ActivateManaAbility(*land)).unwrap();
            }
            assert!(
                matches!(next(&mut state), Decision::CastSpellOrPass { player, castable_spells, .. }
                if player == actor && castable_spells.contains(&source))
            );
            step(&mut state, Action::CastSpell(source)).unwrap();
            assert!(
                matches!(next(&mut state), Decision::CastSpellOrPass { player, .. } if player == actor)
            );
            assert!(state.engine.pending_cast.is_none());
            assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
            assert!(lands[..due].iter().all(|id| state.objects.get(*id).tapped));
            assert_eq!(state.objects.get(source).zone, Zone::Stack);
            assert!(state
                .engine
                .event_history
                .iter()
                .any(|event| matches!(event,
                crate::event::CommittedEvent::SpellCast { spell, controller }
                if *spell == source && *controller == actor)));
            for _ in 0..16 {
                if matches!(next(&mut state), Decision::ChooseTargets { spell, .. } if spell == source)
                {
                    break;
                }
                quiet(&mut state);
            }
            assert!(
                matches!(next(&mut state), Decision::ChooseTargets { player, spell, remaining: 1, legal_targets, can_finish: false }
                if player == actor && spell == source && legal_targets.contains(&Target::Object(host)))
            );
            let waiting = state.clone();
            let mut expected = None;
            for mut replay in [
                waiting.clone(),
                serde_json::from_slice::<GameState>(&serde_json::to_vec(&waiting).unwrap())
                    .unwrap(),
            ] {
                step(&mut replay, Action::ChooseTarget(Target::Object(host))).unwrap();
                assert!(matches!(
                    next(&mut replay),
                    Decision::CastSpellOrPass { .. }
                ));
                let published = replay.clone();
                settle(&mut replay);
                let link = replay.objects.get(source).v4.attached_to.unwrap();
                assert_eq!(link.object, host);
                assert_eq!(
                    link.zone_change_count,
                    replay.objects.get(host).zone_change_count
                );
                assert_eq!(checked_effective_pt(&replay, host).unwrap(), (2, 2));
                for keyword in [Keywords::DOUBLE_STRIKE, Keywords::TRAMPLE] {
                    assert!(has_effective_keyword(&replay, host, keyword));
                }
                let mut published_restore: GameState =
                    serde_json::from_slice(&serde_json::to_vec(&published).unwrap()).unwrap();
                settle(&mut published_restore);
                assert_eq!(published_restore, replay);
                if let Some(previous) = &expected {
                    assert_eq!(&replay, previous);
                } else {
                    expected = Some(replay);
                }
            }
            // A departed source cannot attach from the controller's hand.
            // These are labelled zone/control event primitives on the
            // genuinely paid history, not claims of paid response spells.
            let mut replaced = waiting.clone();
            step(&mut replaced, Action::ChooseTarget(Target::Object(host))).unwrap();
            assert!(matches!(
                next(&mut replaced),
                Decision::CastSpellOrPass { .. }
            ));
            event::propose_and_commit(
                &mut replaced,
                ProposedEvent::zone_change(source, Zone::Hand),
            );
            settle(&mut replaced);
            assert!(replaced.objects.get(source).v4.attached_to.is_none());
            assert_eq!(replaced.objects.get(source).zone, Zone::Hand);
            // Returning that same physical Equipment creates a new source
            // incarnation; the published old trigger cannot attach it.
            let mut reentered = waiting.clone();
            step(&mut reentered, Action::ChooseTarget(Target::Object(host))).unwrap();
            assert!(matches!(
                next(&mut reentered),
                Decision::CastSpellOrPass { .. }
            ));
            event::propose_and_commit(
                &mut reentered,
                ProposedEvent::zone_change(source, Zone::Hand),
            );
            event::propose_and_commit(
                &mut reentered,
                ProposedEvent::zone_change(source, Zone::Battlefield),
            );
            // The reentry's own mandatory ETB must be answered independently.
            assert!(
                matches!(next(&mut reentered), Decision::ChooseTargets { spell, .. } if spell == source)
            );
            step(
                &mut reentered,
                Action::ChooseTarget(Target::Object(alternate)),
            )
            .unwrap();
            next(&mut reentered);
            settle(&mut reentered);
            assert_eq!(
                reentered.objects.get(source).v4.attached_to.unwrap().object,
                alternate
            );
            for mode in 0..2 {
                let mut changed = waiting.clone();
                step(&mut changed, Action::ChooseTarget(Target::Object(host))).unwrap();
                assert!(matches!(
                    next(&mut changed),
                    Decision::CastSpellOrPass { .. }
                ));
                if mode == 0 {
                    event::propose_and_commit(
                        &mut changed,
                        ProposedEvent::zone_change(host, Zone::Hand),
                    );
                    event::propose_and_commit(
                        &mut changed,
                        ProposedEvent::zone_change(host, Zone::Battlefield),
                    );
                } else {
                    crate::standard_cards_v1::gain_control(&mut changed, host, actor.opponent());
                }
                settle(&mut changed);
                assert!(changed.objects.get(source).v4.attached_to.is_none());
            }

            state = expected.unwrap();
            for _ in 0..64 {
                if state.step == Step::Main2 {
                    break;
                }
                quiet(&mut state);
            }
            assert_eq!(state.step, Step::Main2);
            assert_eq!(count(&state, actor), 0);
            assert_eq!(effective_normal_cast_cost(def, actor, &state).generic, 4);
            for land in &lands[6..9] {
                step(&mut state, Action::ActivateManaAbility(*land)).unwrap();
            }
            step(&mut state, Action::ActivateAbility(source, 0)).unwrap();
            assert!(
                matches!(next(&mut state), Decision::ChooseTargets { spell, legal_targets, .. }
                if spell == source && legal_targets.contains(&Target::Object(alternate)))
            );
            step(&mut state, Action::ChooseTarget(Target::Object(alternate))).unwrap();
            assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
            assert!(state.engine.pending_activation.is_none());
            assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
            let equipped = state.clone();
            settle(&mut state);
            assert_eq!(
                state.objects.get(source).v4.attached_to.unwrap().object,
                alternate
            );
            let result = state.clone();
            state = serde_json::from_slice(&serde_json::to_vec(&equipped).unwrap()).unwrap();
            settle(&mut state);
            assert_eq!(state, result);
            assert!(!has_effective_keyword(
                &state,
                host,
                Keywords::DOUBLE_STRIKE
            ));
            assert!(!has_effective_keyword(&state, host, Keywords::TRAMPLE));
        }
    }
}

#[test]
fn genuine_paid_flash_on_opponents_turn_and_targetless_entry_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = crate::extra_turn_v1::tests::ready(actor.opponent());
        let source = put(&mut state, actor, "Embercleave", Zone::Hand);
        let lands = (0..6)
            .map(|_| put(&mut state, actor, "Mountain", Zone::Battlefield))
            .collect::<Vec<_>>();
        assert!(
            matches!(next(&mut state), Decision::CastSpellOrPass { player, .. }
            if player == actor.opponent())
        );
        step(&mut state, Action::Pass).unwrap();
        // Six total mana with only one red cannot pay the printed RR. These
        // additional basic lands are setup in an independent negative world.
        let mut wrong_colors = state.clone();
        let forests = (0..5)
            .map(|_| put(&mut wrong_colors, actor, "Forest", Zone::Battlefield))
            .collect::<Vec<_>>();
        for land in forests {
            step(&mut wrong_colors, Action::ActivateManaAbility(land)).unwrap();
        }
        step(&mut wrong_colors, Action::ActivateManaAbility(lands[0])).unwrap();
        assert_eq!(
            wrong_colors.players[actor.index()]
                .mana_pool
                .iter()
                .map(|&v| u32::from(v))
                .sum::<u32>(),
            6
        );
        assert!(
            matches!(next(&mut wrong_colors), Decision::CastSpellOrPass { player, castable_spells, .. }
            if player == actor && !castable_spells.contains(&source))
        );
        for land in lands {
            step(&mut state, Action::ActivateManaAbility(land)).unwrap();
        }
        assert!(
            matches!(next(&mut state), Decision::CastSpellOrPass { player, castable_spells, .. }
            if player == actor && castable_spells.contains(&source))
        );
        step(&mut state, Action::CastSpell(source)).unwrap();
        assert!(
            matches!(next(&mut state), Decision::CastSpellOrPass { player, .. } if player == actor)
        );
        assert!(state.engine.pending_cast.is_none());
        assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
        assert_eq!(state.objects.get(source).zone, Zone::Stack);
        let paid = state.clone();
        settle(&mut state);
        assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
        assert!(state.objects.get(source).v4.attached_to.is_none());
        assert!(state.engine.pending_triggers.is_empty());
        assert_eq!(state.active_player, actor.opponent());
        let complete = state.clone();
        state = serde_json::from_slice(&serde_json::to_vec(&paid).unwrap()).unwrap();
        settle(&mut state);
        assert_eq!(state, complete);
    }
}
