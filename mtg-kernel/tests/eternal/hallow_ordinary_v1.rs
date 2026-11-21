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

#[test]
fn nomads_redirect_after_dominate_reveals_an_active_player_choice() {
    let mut state = turn_of(P0);
    let nomads = put(&mut state, P1, "Nomads en-Kor", Zone::Battlefield);
    let acolyte = put(&mut state, P1, "Crimson Acolyte", Zone::Battlefield);
    give_priority(&mut state, P1);
    super::activate(&mut state, nomads, 0, &[Target::Object(acolyte)]);
    settled(&mut state);

    give_priority(&mut state, P0);
    let dominate = put(&mut state, P0, "Dominate", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 3);
    cast(&mut state, dominate, &[Target::Object(acolyte)]);
    settled(&mut state);
    assert_eq!(state.objects.get(acolyte).controller, P0);

    let pyro = put(&mut state, P0, "Pyroclasm", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast(&mut state, pyro, &[]);
    protect(&mut state, pyro);

    // First P0 takes Acolyte's red protection for its own two damage. P1
    // redirects one damage from Nomads. The redirected point is a new P0
    // choice; Hallow gains one for that point and one for the remainder.
    for (chooser, option) in [(P0, 1), (P1, 1), (P0, 0)] {
        assert!(
            matches!(settle(&mut state), Some(Decision::ChooseEffectOption {
            player, source, option_count: 2
        }) if player == chooser && source == pyro)
        );
        state = restored(&state);
        act(&mut state, Action::ChooseEffectOption(option));
    }
    settled(&mut state);
    assert_eq!((life(&state, P0), life(&state, P1)), (22, 20));
    for id in [nomads, acolyte] {
        assert_eq!(state.objects.get(id).zone, Zone::Battlefield);
        assert_eq!(state.objects.get(id).damage, 0);
    }
}

#[test]
fn confirmed_packet_keeps_its_prevention_when_a_later_redirect_consumes_a_shared_charge() {
    let mut state = turn_of(P0);
    let first = put(&mut state, P1, "Nomads en-Kor", Zone::Battlefield);
    let second = put(&mut state, P1, "Nomads en-Kor", Zone::Battlefield);
    let third = put(&mut state, P1, "Grizzly Bears", Zone::Battlefield);
    give_priority(&mut state, P1);
    super::activate(&mut state, first, 0, &[Target::Object(second)]);
    settled(&mut state);
    give_priority(&mut state, P1);
    super::activate(&mut state, second, 0, &[Target::Object(third)]);
    settled(&mut state);
    give_priority(&mut state, P0);
    let dominate = put(&mut state, P0, "Dominate", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 3);
    cast(&mut state, dominate, &[Target::Object(second)]);
    settled(&mut state);
    assert_eq!(state.objects.get(second).controller, P0);
    let pyro = put(&mut state, P0, "Pyroclasm", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast(&mut state, pyro, &[]);
    protect(&mut state, pyro);

    // P0 prevents B's original packet without consuming B->C. P1 then
    // redirects A->B and P0 spends B->C on that newly revealed point.
    // The already selected prevention for B's original packet stays applied.
    for (chooser, option) in [(P0, 0), (P1, 1), (P0, 1)] {
        assert!(
            matches!(settle(&mut state), Some(Decision::ChooseEffectOption {
            player, source, option_count: 2
        }) if player == chooser && source == pyro)
        );
        state = restored(&state);
        act(&mut state, Action::ChooseEffectOption(option));
    }
    settled(&mut state);
    assert_eq!((life(&state, P0), life(&state, P1)), (26, 20));
    for object in [first, second, third] {
        assert_eq!(state.objects.get(object).zone, Zone::Battlefield);
        assert_eq!(state.objects.get(object).damage, 0);
    }
}
