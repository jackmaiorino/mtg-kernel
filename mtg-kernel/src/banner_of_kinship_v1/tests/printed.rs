use super::*;
use crate::{
    engine::{self, Action, Decision},
    event::{self, CommittedEvent, ProposedEvent},
    linked_aura_operation_v1::{self, GeneralChoiceV36},
    policy_observation_v6::tests::put,
    state::{Step, Target},
};

fn ready(actor: PlayerId) -> GameState {
    let mut state = crate::extra_turn_v1::tests::ready(actor);
    crate::combat_damage_v1::enable_foundations_combat_v1(&mut state).unwrap();
    for _ in 0..20 {
        put(&mut state, actor, "Forest", Zone::Battlefield);
    }
    state
}
fn quiet(state: &mut GameState, decision: Decision) {
    let action = match decision {
        Decision::CastSpellOrPass { .. } => Action::Pass,
        Decision::DeclareAttackers { .. } => Action::DeclareAttackers(vec![]),
        Decision::DeclareBlockers { .. } => Action::DeclareBlockers(vec![]),
        Decision::OrderTriggers { pending, .. } => {
            Action::OrderTriggers((0..pending.len()).collect())
        }
        other => panic!("actual native progression: {other:?}"),
    };
    engine::step(state, action).unwrap();
    assert!(state.engine.halted.is_none());
}
fn priority(state: &mut GameState, actor: PlayerId) {
    for _ in 0..16 {
        let decision = engine::advance_until_decision(state);
        if matches!(decision, Decision::CastSpellOrPass { player, .. } if player == actor) {
            return;
        }
        quiet(state, decision);
    }
    panic!("original actor priority missing");
}
fn announce(state: &mut GameState, actor: PlayerId, name: &str) -> ObjectId {
    priority(state, actor);
    let source = put(state, actor, name, Zone::Hand);
    let cost = CARD_DEFS[state.objects.get(source).card_def as usize].mana_value as usize;
    let before = state.players[actor.index()]
        .battlefield
        .iter()
        .filter(|id| state.objects.get(**id).tapped)
        .count();
    assert!(
        matches!(engine::advance_until_decision(state), Decision::CastSpellOrPass { castable_spells, .. } if castable_spells.contains(&source))
    );
    engine::step(state, Action::CastSpell(source)).unwrap();
    assert!(
        matches!(engine::advance_until_decision(state), Decision::CastSpellOrPass { player, .. } if player == actor)
    );
    assert!(state.engine.pending_cast.is_none());
    assert_eq!(state.objects.get(source).zone, Zone::Stack);
    let after = state.players[actor.index()]
        .battlefield
        .iter()
        .filter(|id| state.objects.get(**id).tapped)
        .count();
    assert_eq!(after - before, cost);
    assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
    source
}
fn settle(state: &mut GameState) {
    for _ in 0..64 {
        let decision = engine::advance_until_decision(state);
        if state.stack.is_empty()
            && state.engine.pending_triggers.is_empty()
            && state.engine.pending_effect.is_none()
            && !linked_aura_operation_v1::has_pending(state)
        {
            assert!(matches!(decision, Decision::CastSpellOrPass { .. }));
            return;
        }
        quiet(state, decision);
    }
    panic!("actual paid spell did not settle");
}
fn paid(state: &mut GameState, actor: PlayerId, name: &str) -> ObjectId {
    let source = announce(state, actor, name);
    settle(state);
    assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
    source
}
fn reach_type(state: &mut GameState, actor: PlayerId) {
    for _ in 0..32 {
        let decision = engine::advance_until_decision(state);
        if let Decision::ChooseEntryCreatureTypeV1 { player, types, .. } = &decision {
            assert_eq!(*player, actor);
            assert_eq!(types.as_slice(), CreatureTypeV60::ALL);
            assert_eq!(types.len(), 324);
            crate::effect::validate_pending_effect_choice(state).unwrap();
            return;
        }
        quiet(state, decision);
    }
    panic!("definition-owned pre-entry type stage missing");
}
fn choose(state: &mut GameState, chosen: CreatureTypeV60) {
    engine::step(state, Action::ChooseEffectOption(chosen.choice_index())).unwrap();
    assert!(state.engine.halted.is_none());
}
fn roundtrip(state: &GameState) -> GameState {
    let restored = serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap();
    assert_eq!(state, &restored);
    restored
}

#[test]
fn paid_banner_complete_domain_original_payment_checkpoint_restore_and_zero_counts_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut pending = ready(actor);
        let elf = paid(&mut pending, actor, "Llanowar Elves");
        let banner = announce(&mut pending, actor, "Banner of Kinship");
        assert_eq!(pending.objects.get(banner).v4.kinship_type_v1, None);
        assert!(!linked_aura_operation_v1::has_pending(&pending));
        reach_type(&mut pending, actor);
        assert_eq!(pending.objects.get(banner).zone, Zone::Stack);
        assert_eq!(pending.objects.get(banner).counters.fellowship, 0);
        assert!(pending.stack.iter().any(|item| item.source == banner));
        for action in [
            Action::ChooseEffectOption(324),
            Action::ChooseEffectOption(u16::MAX),
            Action::Pass,
        ] {
            let mut rejected = pending.clone();
            assert!(engine::step(&mut rejected, action).is_err());
            assert_eq!(rejected, pending);
        }
        for &chosen in CreatureTypeV60::ALL {
            let expected = i32::from(matches!(
                chosen,
                CreatureTypeV60::Elf | CreatureTypeV60::Druid
            ));
            let mut original = pending.clone();
            let mut restored = roundtrip(&pending);
            for branch in [&mut original, &mut restored] {
                choose(branch, chosen);
                assert_eq!(branch.objects.get(banner).zone, Zone::Battlefield);
                assert_eq!(branch.objects.get(banner).counters.fellowship, expected);
                assert_eq!(entry::authenticated_type(branch, banner).unwrap(), chosen);
                assert_eq!(
                    engine::checked_effective_pt(branch, elf).unwrap(),
                    (1 + expected, 1 + expected)
                );
                entry::validate_state(branch).unwrap();
                assert!(crate::rl::observe_v1(branch, actor, 0).is_err());
                assert!(crate::drake_public_v53::capture(branch).is_err());
                assert!(branch
                    .engine
                    .event_log
                    .iter()
                    .all(|e| !matches!(e, CommittedEvent::KinshipEntryV1 { .. })));
            }
            assert_eq!(original, restored);
        }
    }
}

#[test]
fn paid_banner_checkpoint_rejects_changed_original_proposal_controller_scene_stack_and_prior_answers_both_seats(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        paid(&mut state, actor, "Llanowar Elves");
        let banner = announce(&mut state, actor, "Banner of Kinship");
        reach_type(&mut state, actor);
        for case in 0..8 {
            let mut forged = state.clone();
            let p = forged
                .engine
                .linked_aura_operation_v1
                .pending_kinship_v1
                .as_mut()
                .unwrap();
            let GeneralChoiceV36::KinshipV1 { choice, capture } = &mut p.choice else {
                panic!("actual native type stage")
            };
            match case {
                0 => choice.controller = actor.opponent(),
                1 => choice.counts[usize::from(CreatureTypeV60::Elf.choice_index())] += 1,
                2 => choice.original.as_mut().unwrap().zone_change_count += 1,
                3 => choice.replacement.controller = actor.opponent(),
                4 => *capture += 1,
                5 => p.original.objects.get_mut(banner).owner = actor.opponent(),
                6 => {
                    forged.stack.pop();
                }
                _ => p.operation_history_index += 1,
            }
            let before = forged.clone();
            assert!(crate::effect::validate_pending_effect_choice(&forged).is_err());
            assert!(engine::step(
                &mut forged,
                Action::ChooseEffectOption(CreatureTypeV60::Elf.choice_index())
            )
            .is_err());
            assert_eq!(forged, before);
        }
        let mut restored = roundtrip(&state);
        choose(&mut state, CreatureTypeV60::Elf);
        choose(&mut restored, CreatureTypeV60::Elf);
        assert_eq!(state, restored);
    }
}

#[test]
fn paid_banner_season_replacement_current_amount_effective_type_control_suppression_and_reentry_native_primitives_both_seats(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let elf = paid(&mut state, actor, "Llanowar Elves");
        paid(&mut state, actor, "Doubling Season");
        let banner = announce(&mut state, actor, "Banner of Kinship");
        reach_type(&mut state, actor);
        choose(&mut state, CreatureTypeV60::Elf);
        assert_eq!(state.objects.get(banner).counters.fellowship, 2);
        assert_eq!(engine::checked_effective_pt(&state, elf).unwrap(), (3, 3));
        let original_link = ObjectLinkV4 {
            object: banner,
            zone_change_count: state.objects.get(banner).zone_change_count,
        };
        // These are native state-change primitives on a genuinely paid Banner.
        // The selected catalog has no spell granting artifact control or suppressing
        // an artifact's abilities. No such paid printing is asserted here.
        let other = put(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        crate::standard_cards_v1::gain_control(&mut state, banner, actor.opponent());
        assert_eq!(engine::checked_effective_pt(&state, elf).unwrap(), (1, 1));
        assert_eq!(engine::checked_effective_pt(&state, other).unwrap(), (3, 3));
        state.objects.get_mut(banner).counters.fellowship = 4;
        assert_eq!(engine::checked_effective_pt(&state, other).unwrap(), (5, 5));
        let suppressor = ObjectLinkV4 {
            object: other,
            zone_change_count: state.objects.get(other).zone_change_count,
        };
        state.objects.get_mut(banner).v4.creature_upgrade =
            Some(crate::standard_creatures_v1::CreatureUpgradeV1 {
                suppressed_by: vec![crate::standard_creatures_v1::TidebinderSuppressionV1 {
                    source: suppressor,
                    timestamp: 8,
                }],
                ..Default::default()
            });
        assert_eq!(engine::checked_effective_pt(&state, other).unwrap(), (1, 1));
        state.objects.get_mut(banner).v4.creature_upgrade = None;
        state.objects.get_mut(other).v4.creature_upgrade =
            Some(crate::standard_creatures_v1::CreatureUpgradeV1 {
                creature_types: Some((vec![crate::card_def::Subtype::Human.stable_id()], 9)),
                ..Default::default()
            });
        assert_eq!(engine::checked_effective_pt(&state, other).unwrap(), (1, 1));
        entry::validate_state(&state).unwrap();
        let departure = AbilitySourceContractV4::capture(&state, banner);
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(banner, Zone::Hand));
        assert_eq!(
            crate::standard_creatures_v1::counter_transfer_snapshot(&state, departure)
                .counters
                .fellowship,
            4
        );
        assert_eq!(state.objects.get(banner).v4.kinship_type_v1, None);
        assert_eq!(state.objects.get(banner).counters.fellowship, 0);
        assert!(quote_controlled_boost(&state, original_link, CreatureTypeV60::Elf, elf).is_err());
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(banner, Zone::Battlefield),
        );
        reach_type(&mut state, actor);
        let mut restored = roundtrip(&state);
        for branch in [&mut state, &mut restored] {
            choose(branch, CreatureTypeV60::Bear);
            assert_eq!(branch.objects.get(banner).controller, actor);
            assert_eq!(branch.objects.get(banner).counters.fellowship, 0);
            assert_eq!(
                entry::authenticated_type(branch, banner).unwrap(),
                CreatureTypeV60::Bear
            );
            entry::validate_state(branch).unwrap();
        }
        assert_eq!(state, restored);
    }
}

#[test]
fn paid_banner_checked_multiple_season_entry_overflow_refuses_unchanged_and_zero_remains_legal_both_seats(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for seasons in [2, 31] {
            let mut state = ready(actor);
            let elf = paid(&mut state, actor, "Llanowar Elves");
            // Exact real definitions form the native replacement scene. The
            // Banner itself is genuinely paid; this large roster is a setup
            // primitive, not a claim of having paid for 31 enchantments.
            for _ in 0..seasons {
                put(&mut state, actor, "Doubling Season", Zone::Battlefield);
            }
            let banner = announce(&mut state, actor, "Banner of Kinship");
            reach_type(&mut state, actor);
            let original = state.clone();
            let mut restored = roundtrip(&state);
            if seasons == 31 {
                for branch in [&mut state, &mut restored] {
                    assert!(engine::step(
                        branch,
                        Action::ChooseEffectOption(CreatureTypeV60::Elf.choice_index())
                    )
                    .is_err());
                    assert_eq!(*branch, original);
                    choose(branch, CreatureTypeV60::Bear);
                    assert_eq!(branch.objects.get(banner).counters.fellowship, 0);
                    assert_eq!(engine::checked_effective_pt(branch, elf).unwrap(), (1, 1));
                }
            } else {
                for branch in [&mut state, &mut restored] {
                    choose(branch, CreatureTypeV60::Elf);
                    assert_eq!(branch.objects.get(banner).counters.fellowship, 4);
                    assert_eq!(engine::checked_effective_pt(branch, elf).unwrap(), (5, 5));
                }
            }
            assert_eq!(state, restored);
        }
    }
}

#[test]
fn mixed_native_entry_batch_keeps_apnap_complete_preentry_scene_and_answered_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let elf = paid(&mut state, actor, "Llanowar Elves");
        let first = announce(&mut state, actor, "Banner of Kinship");
        reach_type(&mut state, actor);
        choose(&mut state, CreatureTypeV60::Elf);
        settle(&mut state);
        // A native simultaneous zone-change primitive, after the genuine paid
        // cast, checks shared ordering with the existing color-entry family.
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(first, Zone::Hand));
        let heraldic = put(&mut state, actor.opponent(), "Heraldic Banner", Zone::Hand);
        event::propose_and_commit_batch(
            &mut state,
            vec![
                ProposedEvent::zone_change(heraldic, Zone::Battlefield),
                ProposedEvent::zone_change(first, Zone::Battlefield),
            ],
        );
        reach_type(&mut state, actor);
        assert_eq!(state.objects.get(first).zone, Zone::Hand);
        assert_eq!(state.objects.get(heraldic).zone, Zone::Hand);
        choose(&mut state, CreatureTypeV60::Druid);
        assert!(
            matches!(engine::advance_until_decision(&mut state), Decision::ChooseEntryColorV36 { player, .. } if player == actor.opponent())
        );
        assert_eq!(state.objects.get(first).zone, Zone::Hand);
        let mut forged = state.clone();
        let pending = forged
            .engine
            .linked_aura_operation_v1
            .pending_kinship_v1
            .as_mut()
            .unwrap();
        let crate::linked_aura_operation_v1::GeneralAnswerV36::KinshipV1 { chosen, .. } =
            &mut pending.answers[0]
        else {
            panic!("actual earlier type answer")
        };
        *chosen = CreatureTypeV60::Bear;
        let rejected = forged.clone();
        assert!(crate::effect::validate_pending_effect_choice(&forged).is_err());
        assert!(engine::step(&mut forged, Action::ChooseEffectOption(2)).is_err());
        assert_eq!(forged, rejected);
        let mut restored = roundtrip(&state);
        for branch in [&mut state, &mut restored] {
            engine::step(branch, Action::ChooseEffectOption(2)).unwrap();
            entry::validate_state(branch).unwrap();
            crate::heraldic_banner_v1::validate_state(branch).unwrap();
            assert_eq!(branch.objects.get(first).counters.fellowship, 1);
            assert_eq!(engine::checked_effective_pt(branch, elf).unwrap(), (2, 2));
            assert_eq!(
                crate::heraldic_banner_v1::authenticated_color(branch, heraldic).unwrap(),
                crate::mana::ManaColor::B
            );
        }
        assert_eq!(state, restored);
    }
}

#[test]
fn genuine_paid_replication_copies_banner_with_new_type_preentry_count_and_season_batch_restore_both_seats(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for season in [false, true] {
            let mut state = ready(actor);
            paid(&mut state, actor, "Llanowar Elves");
            if season {
                paid(&mut state, actor, "Doubling Season");
            }
            let banner = announce(&mut state, actor, "Banner of Kinship");
            reach_type(&mut state, actor);
            choose(&mut state, CreatureTypeV60::Elf);
            settle(&mut state);
            for _ in 0..6 {
                put(&mut state, actor, "Island", Zone::Battlefield);
            }
            let replication = paid(&mut state, actor, "Extravagant Replication");
            let mut found = false;
            for _ in 0..256 {
                let decision = engine::advance_until_decision(&mut state);
                if let Decision::ChooseTargets {
                    spell,
                    legal_targets,
                    ..
                } = &decision
                {
                    assert_eq!(*spell, replication);
                    assert_eq!(state.active_player, actor);
                    assert_eq!(state.step, Step::Upkeep);
                    assert!(legal_targets.contains(&Target::Object(banner)));
                    found = true;
                    break;
                }
                quiet(&mut state, decision);
            }
            assert!(found);
            engine::step(&mut state, Action::ChooseTarget(Target::Object(banner))).unwrap();
            assert!(matches!(
                engine::advance_until_decision(&mut state),
                Decision::CastSpellOrPass { .. }
            ));
            reach_type(&mut state, actor);
            let mut restored = roundtrip(&state);
            for branch in [&mut state, &mut restored] {
                // The original has Elf; each actual copy chooses independently.
                choose(branch, CreatureTypeV60::Bear);
                if season {
                    reach_type(branch, actor);
                    choose(branch, CreatureTypeV60::Druid);
                }
                settle(branch);
                let mut copies = branch
                    .objects
                    .iter()
                    .filter(|(_, o)| {
                        o.zone == Zone::Battlefield && o.v4.is_token && o.card_def == 463
                    })
                    .map(|(id, o)| (id, o.v4.kinship_type_v1.unwrap(), o.counters.fellowship))
                    .collect::<Vec<_>>();
                copies.sort_by_key(|x| x.0);
                assert_eq!(copies.len(), if season { 2 } else { 1 });
                assert_eq!(copies[0].1, CreatureTypeV60::Bear);
                assert_eq!(copies[0].2, 0);
                if season {
                    assert_eq!((copies[1].1, copies[1].2), (CreatureTypeV60::Druid, 2));
                }
                assert_eq!(
                    entry::authenticated_type(branch, banner).unwrap(),
                    CreatureTypeV60::Elf
                );
                entry::validate_state(branch).unwrap();
            }
            assert_eq!(state, restored);
        }
    }
}
