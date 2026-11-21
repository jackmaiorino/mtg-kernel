//! Mixed replacement choices using printed Hallow, Red Circle, and Lightning Bolt.

use super::{
    act, cast, decoy, give_priority, life, link, pool, put, restored, settle, settled, shield,
    shields, turn_of, Action, CircleColorV1, Decision, GameState, ManaColor, ObjectId, Target,
    Zone, P0, P1, RED_CIRCLE,
};

fn pending_bolt(two_hallows: bool) -> (GameState, ObjectId) {
    let mut state = turn_of(P1);
    let circle = put(&mut state, P0, RED_CIRCLE, Zone::Battlefield);
    decoy(&mut state, P1, CircleColorV1::Red);
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    let hallow = put(&mut state, P0, "Hallow", Zone::Hand);

    pool(&mut state, P1, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Player(P0)]);
    let bolt_link = link(&state, bolt);
    shield(&mut state, P0, circle, bolt_link);

    give_priority(&mut state, P0);
    pool(&mut state, P0, &[ManaColor::W], 0);
    cast(&mut state, hallow, &[Target::Object(bolt)]);
    if two_hallows {
        let opposing = put(&mut state, P1, "Hallow", Zone::Hand);
        give_priority(&mut state, P1);
        pool(&mut state, P1, &[ManaColor::W], 0);
        cast(&mut state, opposing, &[Target::Object(bolt)]);
    }
    (state, bolt)
}

fn resolution_outcomes(two_hallows: bool, expected_options: u16) -> Vec<(i32, i32, usize)> {
    let (mut state, bolt) = pending_bolt(two_hallows);
    let decision = settle(&mut state).expect("Bolted player chooses the prevention effect");
    assert!(
        matches!(decision, Decision::ChooseEffectOption { player, source, option_count }
            if player == P0 && source == bolt && option_count == expected_options),
        "{decision:?}"
    );
    assert_eq!(
        (life(&state, P0), life(&state, P1), shields(&state)),
        (20, 20, 1)
    );

    let before_invalid = state.state_hash();
    assert!(
        mtg_kernel::engine::step(&mut state, Action::ChooseEffectOption(expected_options)).is_err()
    );
    assert_eq!(state.state_hash(), before_invalid);

    let mut outcomes = Vec::new();
    for index in 0..expected_options {
        let mut branch = restored(&state);
        act(&mut branch, Action::ChooseEffectOption(index));
        settled(&mut branch);
        assert!(branch.stack.is_empty());
        assert_eq!(branch.objects.get(bolt).zone, Zone::Graveyard);
        outcomes.push((life(&branch, P0), life(&branch, P1), shields(&branch)));
    }
    outcomes
}

#[test]
fn one_hallow_and_red_circle_offer_two_distinct_prevention_choices() {
    assert_eq!(resolution_outcomes(false, 2), [(23, 20, 1), (20, 20, 0)]);
}

#[test]
fn opposing_hallows_and_red_circle_offer_three_distinct_prevention_choices() {
    assert_eq!(
        resolution_outcomes(true, 3),
        [(23, 20, 1), (20, 23, 1), (20, 20, 0)]
    );
}

/// Hallow follows the Bombardment spell onto the battlefield. Painter makes
/// both paid colors match until it is sacrificed for the second activation.
fn painted_bombardment(
    attacker: mtg_kernel::ids::PlayerId,
    sphere: bool,
) -> (GameState, ObjectId, ObjectId) {
    let defender = attacker.opponent();
    let mut state = turn_of(attacker);
    let red = put(&mut state, defender, RED_CIRCLE, Zone::Battlefield);
    let blue = put(&mut state, defender, super::BLUE_CIRCLE, Zone::Battlefield);
    if sphere {
        put(&mut state, defender, "Sphere of Law", Zone::Battlefield);
    }
    let spare = decoy(&mut state, attacker, CircleColorV1::Red).object;
    let painter = put(&mut state, attacker, "Painter's Servant", Zone::Hand);
    pool(&mut state, attacker, &[], 2);
    cast(&mut state, painter, &[]);
    super::settle_option(&mut state, 1);
    let bombardment = put(&mut state, attacker, "Goblin Bombardment", Zone::Hand);
    pool(&mut state, attacker, &[ManaColor::R], 1);
    cast(&mut state, bombardment, &[]);
    give_priority(&mut state, defender);
    let hallow = put(&mut state, defender, "Hallow", Zone::Hand);
    pool(&mut state, defender, &[ManaColor::W], 0);
    cast(&mut state, hallow, &[Target::Object(bombardment)]);
    settled(&mut state);
    let source = link(&state, bombardment);
    for circle in [red, blue, blue] {
        shield(&mut state, defender, circle, source);
    }
    give_priority(&mut state, attacker);
    super::activate_paying(
        &mut state,
        bombardment,
        0,
        &[Target::Player(defender)],
        Some(spare),
    );
    (state, bombardment, painter)
}

fn expect_menu(state: &mut GameState, source: ObjectId, count: u16) {
    let decision = settle(state);
    assert!(
        matches!(decision, Some(Decision::ChooseEffectOption { player, source: actual, option_count })
        if player == state.active_player.opponent() && actual == source && option_count == count),
        "{decision:?}"
    );
}

#[test]
fn hallow_offers_each_distinct_paid_color_and_rechecks_the_live_source() {
    for attacker in [P0, P1] {
        let defender = attacker.opponent();
        let (mut state, source, painter) = painted_bombardment(attacker, false);
        expect_menu(&mut state, source, 3);
        for first in 0..3 {
            let mut branch = restored(&state);
            act(&mut branch, Action::ChooseEffectOption(first));
            settled(&mut branch);
            assert_eq!(life(&branch, defender), if first == 0 { 21 } else { 20 });
            assert_eq!(shields(&branch), if first == 0 { 3 } else { 2 });
            give_priority(&mut branch, attacker);
            super::activate_paying(
                &mut branch,
                source,
                0,
                &[Target::Player(defender)],
                Some(painter),
            );
            if first != 1 {
                // The still-red source can use the red shield, now at option 1.
                expect_menu(&mut branch, source, 2);
                act(&mut branch, Action::ChooseEffectOption(1));
            }
            settled(&mut branch);
            // Spending red first leaves only blue shields; Hallow gains the life automatically.
            assert_eq!(life(&branch, defender), if first == 2 { 20 } else { 21 });
            assert_eq!(shields(&branch), if first == 2 { 1 } else { 2 });
        }
    }
}

#[test]
fn hallow_restores_the_original_circle_and_sphere_menu_in_order() {
    let (mut state, source, _) = painted_bombardment(P1, true);
    expect_menu(&mut state, source, 4);
    let mut old = restored(&state);
    let choice = &mut old
        .engine
        .linked_aura_operation_v1
        .pending_hallow_v1
        .as_mut()
        .unwrap()
        .choice;
    assert_eq!(choice.ordinary.len(), 2);
    // A save made before the second distinct Circle was offered.
    choice.ordinary.pop();
    for (index, expected_life, expected_shields) in [(0, 21, 3), (1, 20, 2), (2, 20, 3)] {
        let mut branch = restored(&old);
        expect_menu(&mut branch, source, 3);
        act(&mut branch, Action::ChooseEffectOption(index));
        settled(&mut branch);
        assert_eq!(
            (life(&branch, P0), shields(&branch)),
            (expected_life, expected_shields)
        );
    }
    // An omitted historical ordinary option is still rejected.
    old.engine
        .linked_aura_operation_v1
        .pending_hallow_v1
        .as_mut()
        .unwrap()
        .choice
        .ordinary
        .clear();
    assert!(mtg_kernel::engine::step(&mut old, Action::ChooseEffectOption(0)).is_err());
}
