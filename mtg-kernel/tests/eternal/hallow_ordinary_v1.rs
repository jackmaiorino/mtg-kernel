//! Replacement order on a real spell, including partial prevention and prohibition.
use super::{
    act, cast, give_priority, life, pool, put, restored, settle, settled, turn_of, Action,
    Decision, GameState, ManaColor, ObjectId, Target, Zone, P0, P1,
};

fn protect(state: &mut GameState, spell: ObjectId) {
    give_priority(state, P0);
    let hallow = put(state, P0, "Hallow", Zone::Hand);
    pool(state, P0, &[ManaColor::W], 0);
    cast(state, hallow, &[Target::Object(spell)]);
}

fn bolt_board(spheres: usize, ojer: bool) -> (GameState, ObjectId) {
    let mut state = turn_of(P1);
    for _ in 0..spheres {
        put(&mut state, P0, "Sphere of Law", Zone::Battlefield);
    }
    if ojer {
        put(
            &mut state,
            P1,
            "Ojer Axonil, Deepest Might",
            Zone::Battlefield,
        );
    }
    let spell = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R], 0);
    cast(&mut state, spell, &[Target::Player(P0)]);
    protect(&mut state, spell);
    (state, spell)
}

fn outcome(start: &GameState, source: ObjectId, choices: &[(u16, u16)]) -> i32 {
    let mut state = restored(start);
    for &(option, count) in choices {
        assert!(
            matches!(settle(&mut state), Some(Decision::ChooseEffectOption {
            player: P0, source: actual, option_count
        }) if actual == source && option_count == count)
        );
        state = restored(&state);
        act(&mut state, Action::ChooseEffectOption(option));
    }
    settled(&mut state);
    assert_eq!(state.objects.get(source).zone, Zone::Graveyard);
    assert_eq!(life(&state, P1), 20);
    life(&state, P0)
}

#[test]
fn two_spheres_allow_hallow_before_between_or_after_them() {
    let (state, bolt) = bolt_board(2, false);
    assert_eq!(outcome(&state, bolt, &[(0, 3)]), 23);
    assert_eq!(outcome(&state, bolt, &[(1, 3), (0, 2)]), 21);
    assert_eq!(outcome(&state, bolt, &[(2, 3), (1, 2)]), 20);
}

#[test]
fn ojer_and_sphere_may_each_precede_the_other_on_a_shielded_bolt() {
    let (state, bolt) = bolt_board(1, true);
    assert_eq!(outcome(&state, bolt, &[(0, 3)]), 23);
    assert_eq!(outcome(&state, bolt, &[(1, 3), (0, 2)]), 21);
    assert_eq!(outcome(&state, bolt, &[(2, 3), (1, 2)]), 22);
    assert_eq!(outcome(&state, bolt, &[(1, 3), (1, 2)]), 24);
}

#[test]
fn impractical_joke_prohibits_hallow_and_nishoba_still_loses_its_counter() {
    let mut state = turn_of(P1);
    let phantom = put(&mut state, P0, "Phantom Nishoba", Zone::Battlefield);
    state.objects.get_mut(phantom).counters.plus1_plus1 = 7;
    let spell = put(&mut state, P1, "Impractical Joke", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R], 0);
    cast(&mut state, spell, &[Target::Object(phantom)]);
    if matches!(
        super::next(&mut state),
        Decision::ChooseTargets {
            can_finish: true,
            ..
        }
    ) {
        act(&mut state, Action::FinishEffectSelection);
    }
    protect(&mut state, spell);
    settled(&mut state);
    assert_eq!(life(&state, P0), 20);
    let phantom = state.objects.get(phantom);
    assert_eq!(
        (phantom.zone, phantom.damage, phantom.counters.plus1_plus1),
        (Zone::Battlefield, 3, 6)
    );
}
