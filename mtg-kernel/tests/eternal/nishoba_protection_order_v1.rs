use super::{
    act, cast, next, pool, put, restored, settle, settled, turn_of, Action, Decision, GameState,
    ManaColor, ObjectId, PlayerId, Target, Zone, P0, P1,
};
use mtg_kernel::event::{self, ProposedEvent};

fn board(caster: PlayerId, protected: bool) -> (GameState, ObjectId) {
    let mut state = turn_of(caster);
    let phantom = put(
        &mut state,
        caster.opponent(),
        "Phantom Nishoba",
        Zone::Battlefield,
    );
    state.objects.get_mut(phantom).counters.plus1_plus1 = 7;
    if protected {
        let mask = put(&mut state, caster, "Mask of Law and Grace", Zone::Hand);
        pool(&mut state, caster, &[ManaColor::W], 0);
        cast(&mut state, mask, &[Target::Object(phantom)]);
        settled(&mut state);
        assert_eq!(
            state.objects.get(mask).v4.attached_to.unwrap().object,
            phantom
        );
    }
    (state, phantom)
}

fn counters(state: &GameState, phantom: ObjectId) -> i32 {
    let object = state.objects.get(phantom);
    assert_eq!((object.zone, object.damage), (Zone::Battlefield, 0));
    object.counters.plus1_plus1
}

#[test]
fn pyroclasm_offers_protection_or_phantom_counter_to_either_controller() {
    for caster in [P0, P1] {
        for protected in [false, true] {
            let (mut state, phantom) = board(caster, protected);
            let pyro = put(&mut state, caster, "Pyroclasm", Zone::Hand);
            pool(&mut state, caster, &[ManaColor::R], 1);
            cast(&mut state, pyro, &[]);
            if !protected {
                settled(&mut state);
                assert_eq!(counters(&state, phantom), 6);
                continue;
            }
            assert!(
                matches!(settle(&mut state), Some(Decision::ChooseEffectOption {
                player, source, option_count: 2
            }) if player == caster.opponent() && source == pyro)
            );
            assert_eq!(counters(&state, phantom), 7);
            for (choice, expected) in [(0, 7), (1, 6)] {
                let mut branch = restored(&state);
                act(&mut branch, Action::ChooseEffectOption(choice));
                settled(&mut branch);
                assert_eq!(branch.objects.get(pyro).zone, Zone::Graveyard);
                assert_eq!(counters(&branch, phantom), expected);
            }
        }
    }
}

#[test]
fn protected_and_uncovered_damage_share_one_phantom_counter_per_batch() {
    for red_first in [false, true] {
        for second_red in [false, true] {
            let (mut state, phantom) = board(P0, true);
            let red = put(&mut state, P0, "Jackal Pup", Zone::Battlefield);
            let other = put(
                &mut state,
                P0,
                if second_red {
                    "Jackal Pup"
                } else {
                    "Elvish Mystic"
                },
                Zone::Battlefield,
            );
            let sources = if red_first {
                [red, other]
            } else {
                [other, red]
            };
            event::propose_and_commit_batch(
                &mut state,
                sources
                    .into_iter()
                    .map(|source| ProposedEvent::damage(source, Target::Object(phantom), 1))
                    .collect(),
            );
            let expected_source = if second_red { sources[0] } else { red };
            assert!(matches!(next(&mut state), Decision::ChooseEffectOption {
                player: P1, source, option_count: 2
            } if source == expected_source));
            let mut branch = restored(&state);
            act(&mut branch, Action::ChooseEffectOption(1));
            settled(&mut branch);
            assert_eq!(counters(&branch, phantom), 6);
        }
    }
}

#[test]
fn bare_phantom_can_keep_its_counter_with_fog_strands_or_maze_both_seats() {
    use super::prevention_order_no_hallow_v1::{combat_choice, global_prevention};
    use mtg_kernel::state::Step;

    for attacker in [P0, P1] {
        let defender = attacker.opponent();
        for (name, color) in [
            ("Moment's Peace", ManaColor::G),
            ("Prismatic Strands", ManaColor::G),
            ("Prismatic Strands", ManaColor::U),
            ("Maze of Ith", ManaColor::C),
        ] {
            let (mut game, phantom) = board(attacker, false);
            let mongrel = put(&mut game, attacker, "Wild Mongrel", Zone::Battlefield);
            let maze = if name == "Maze of Ith" {
                Some(put(&mut game, defender, name, Zone::Battlefield))
            } else {
                global_prevention(&mut game, defender, name, color);
                None
            };
            super::pass_until(&mut game, |state| state.step == Step::DeclareAttackers);
            act(&mut game, Action::DeclareAttackers(vec![mongrel]));
            if let Some(maze) = maze {
                super::give_priority(&mut game, defender);
                super::activate(&mut game, maze, 0, &[Target::Object(mongrel)]);
                settled(&mut game);
            }
            super::pass_until(&mut game, |state| state.step == Step::DeclareBlockers);
            act(&mut game, Action::DeclareBlockers(vec![(phantom, mongrel)]));
            if color == ManaColor::U {
                super::pass_until(&mut game, |state| state.step == Step::EndCombat);
                assert_eq!(counters(&game, phantom), 6);
                continue;
            }
            assert!(
                matches!(combat_choice(&mut game), Some(Decision::ChooseEffectOption {
                player, source, option_count: 2
            }) if player == defender && source == mongrel),
                "{name}"
            );
            // Registered replacements follow the static abilities in the existing packet order.
            let outcomes = if name == "Prismatic Strands" {
                [6, 7]
            } else {
                [7, 6]
            };
            for (option, expected) in outcomes.into_iter().enumerate() {
                let mut branch = restored(&game);
                act(&mut branch, Action::ChooseEffectOption(option as u16));
                super::pass_until(&mut branch, |state| state.step == Step::EndCombat);
                assert_eq!(counters(&branch, phantom), expected, "{name}");
            }
        }
    }
}
