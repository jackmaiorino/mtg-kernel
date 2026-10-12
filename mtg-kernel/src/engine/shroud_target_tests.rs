//! Registered operands exercise targeting and the explicit frozen DTO refusal.
use super::*;
use crate::effect::{self, EffectObjectBinding};
use crate::event::{self, ProposedEvent};
use crate::policy_observation_v6::tests::{put, ready_state};

#[test]
fn shroud_excludes_both_players_and_rechecks_real_announced_spell_readonly() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state();
        state.active_player = actor;
        state.priority_player = actor;
        let bolt = put(&mut state, actor, "Lightning Bolt", Zone::Hand);
        let creature = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        state.players[actor.index()].mana_pool[crate::mana::ManaColor::R.pool_index()] = 1;
        step(&mut state, Action::CastSpell(bolt)).unwrap();
        assert!(
            matches!(advance_until_decision(&mut state), Decision::ChooseTargets {
            legal_targets, ..
        } if legal_targets.contains(&Target::Object(creature)))
        );
        effect::install_temporary_boost(
            &mut state,
            EffectObjectBinding {
                object: creature,
                expected_zone: Zone::Battlefield,
                expected_zone_change_count: 0,
            },
            0,
            0,
            Keywords::SHROUD,
        );
        assert!(has_effective_keyword(&state, creature, Keywords::SHROUD));
        for chooser in [actor, actor.opponent()] {
            assert!(
                !legal_targets_for_controller(TargetSpec::Creature, &[], chooser, &state)
                    .contains(&Target::Object(creature))
            );
            assert!(!effect_target_is_legal(
                &state,
                bolt,
                chooser,
                TargetSpec::AnyTarget,
                &[Target::Object(creature)],
                0
            ));
        }
        let bytes = serde_json::to_vec(&state).unwrap();
        let mut restored: GameState = serde_json::from_slice(&bytes).unwrap();
        for branch in [&mut state, &mut restored] {
            let before = serde_json::to_vec(branch).unwrap();
            assert!(step(branch, Action::ChooseTarget(Target::Object(creature))).is_err());
            assert!(crate::rl::observe_v1(branch, actor, 0).is_err());
            assert_eq!(serde_json::to_vec(branch).unwrap(), before);
            // Choosing the genuine player recipient still completes the cast.
            step(
                branch,
                Action::ChooseTarget(Target::Player(actor.opponent())),
            )
            .unwrap();
            assert!(matches!(
                advance_until_decision(branch),
                Decision::CastSpellOrPass { .. }
            ));
            assert_eq!(
                branch.stack.last().unwrap().targets,
                [Target::Player(actor.opponent())]
            );
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&restored).unwrap()
        );
    }
}

#[test]
fn shroud_grant_does_not_survive_a_new_incarnation_and_old_keyword_bits_are_exact() {
    assert_eq!(Keywords::TOXIC_1.0, 1 << 17);
    assert_eq!(Keywords::SHROUD.0, 1 << 18);
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state();
        let creature = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        effect::install_temporary_boost(
            &mut state,
            EffectObjectBinding {
                object: creature,
                expected_zone: Zone::Battlefield,
                expected_zone_change_count: 0,
            },
            0,
            0,
            Keywords::SHROUD,
        );
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(creature, Zone::Hand));
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change_to_battlefield_under_controller(creature, actor),
        );
        assert_eq!(state.objects.get(creature).zone_change_count, 2);
        assert!(!has_effective_keyword(&state, creature, Keywords::SHROUD));
        for chooser in [actor, actor.opponent()] {
            assert!(
                legal_targets_for_controller(TargetSpec::Creature, &[], chooser, &state)
                    .contains(&Target::Object(creature))
            );
        }
    }
}
