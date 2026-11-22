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
