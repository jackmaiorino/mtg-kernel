//! Surviving damage after the affected player orders Sphere of Law and Ojer.
use super::prevention_order_no_hallow_v1::choose_public;
use super::{
    act, cast, give_priority, life, pool, put, restored, settle, settled, turn_of, Action,
    Decision, GameState, ManaColor, ObjectId, Target, Zone, P0, P1,
};
use mtg_kernel::ids::PlayerId;

fn board(active: PlayerId, sphere: bool, ojer: bool) -> GameState {
    let mut state = turn_of(active);
    if sphere {
        put(
            &mut state,
            active.opponent(),
            "Sphere of Law",
            Zone::Battlefield,
        );
    }
    if ojer {
        put(
            &mut state,
            active,
            "Ojer Axonil, Deepest Might",
            Zone::Battlefield,
        );
    }
    state
}

fn bolt(state: &mut GameState, active: PlayerId, name: &str) -> ObjectId {
    give_priority(state, active);
    let source = put(state, active, name, Zone::Hand);
    pool(state, active, &[ManaColor::R], 0);
    cast(state, source, &[Target::Player(active.opponent())]);
    source
}

fn no_shields(state: &GameState) {
    assert!(state.eternal_v1.as_ref().is_none_or(
        |eternal| eternal.pf1c.hallow_shields.is_empty() && eternal.s1d.cop_shields.is_empty()
    ));
}

#[test]
fn real_bolt_can_leave_two_or_four_damage_after_a_saved_public_order_both_seats() {
    for active in [P0, P1] {
        let mut start = board(active, true, true);
        let source = bolt(&mut start, active, "Lightning Bolt");
        assert!(
            matches!(settle(&mut start), Some(Decision::ChooseEffectOption {
            player, source: actual, option_count: 2,
        }) if player == active.opponent() && actual == source)
        );
        no_shields(&start);
        assert_eq!(life(&start, active.opponent()), 20);
        for (option, damage) in [(0, 4), (1, 2)] {
            let mut state = restored(&start);
            choose_public(&mut state, active.opponent(), source, option, 2);
            settled(&mut state);
            assert_eq!(life(&state, active.opponent()), 20 - damage);
            assert_eq!(life(&state, active), 20);
            assert_eq!(state.objects.get(source).zone, Zone::Graveyard);
            assert!(state.engine.linked_aura_operation_v1.is_empty());
            assert_eq!(restored(&state).state_hash(), state.state_hash());
        }
    }
}

#[test]
fn a_reduction_to_zero_does_not_reapply_ojer_or_commit_damage() {
    for active in [P0, P1] {
        let mut start = board(active, true, true);
        let source = bolt(&mut start, active, "Lava Dart");
        assert!(matches!(
            settle(&mut start),
            Some(Decision::ChooseEffectOption {
                option_count: 2,
                ..
            })
        ));
        for (option, damage) in [(0, 0), (1, 2)] {
            let mut state = restored(&start);
            choose_public(&mut state, active.opponent(), source, option, 2);
            settled(&mut state);
            assert_eq!(life(&state, active.opponent()), 20 - damage);
        }
    }
}

#[test]
fn lone_effects_and_unpreventable_damage_keep_their_ordinary_resolution() {
    for active in [P0, P1] {
        for (sphere, ojer, prohibit, damage) in [
            (false, false, false, 3),
            (true, false, false, 1),
            (false, true, false, 4),
            (true, true, true, 4),
        ] {
            let mut state = board(active, sphere, ojer);
            if prohibit {
                let pain = put(&mut state, active, "Flaring Pain", Zone::Hand);
                pool(&mut state, active, &[ManaColor::R], 1);
                cast(&mut state, pain, &[]);
                settled(&mut state);
            }
            bolt(&mut state, active, "Lightning Bolt");
            settled(&mut state);
            assert_eq!(life(&state, active.opponent()), 20 - damage);
            no_shields(&state);
            assert!(state.engine.linked_aura_operation_v1.is_empty());
        }
    }
}

#[test]
fn earthquake_residual_and_uncovered_damage_commit_once_in_the_same_batch() {
    for active in [P0, P1] {
        let mut start = board(active, true, true);
        put(&mut start, active, "Sphere of Law", Zone::Battlefield);
        let source = put(&mut start, active, "Earthquake", Zone::Hand);
        pool(&mut start, active, &[ManaColor::R], 3);
        cast(&mut start, source, &[]);
        assert!(
            matches!(super::next(&mut start), Decision::ChooseEffectOption { option_count, .. } if option_count > 3)
        );
        act(&mut start, Action::ChooseEffectOption(3));
        assert!(
            matches!(settle(&mut start), Some(Decision::ChooseEffectOption {
            player, source: actual, option_count: 2,
        }) if player == active.opponent() && actual == source)
        );
        assert_eq!((life(&start, P0), life(&start, P1)), (20, 20));
        no_shields(&start);
        for (option, damage) in [(0, 4), (1, 2)] {
            let mut state = restored(&start);
            choose_public(&mut state, active.opponent(), source, option, 2);
            settled(&mut state);
            assert_eq!(life(&state, active), 19);
            assert_eq!(life(&state, active.opponent()), 20 - damage);
            assert!(state.engine.linked_aura_operation_v1.is_empty());
        }
    }
}
