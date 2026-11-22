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
