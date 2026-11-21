//! A paid Circle and a nonconsuming global prevention effect compete without Hallow.
use super::{
    act, cast, decoy, give_priority, life, link, next, pool, put, restored, settle, settled,
    shield, shields, turn_of, Action, CircleColorV1, Decision, GameState, ManaColor, ObjectId,
    Target, Zone, P0, P1, RED_CIRCLE,
};
use mtg_kernel::engine;
use mtg_kernel::ids::PlayerId;
use mtg_kernel::policy_surface_v5::PolicySurfaceV5;
use mtg_kernel::rl::{observe_policy_v5, policy_legal_action_candidates_v5, ActionSemanticV1};
use mtg_kernel::state::Step;

fn global_prevention(state: &mut GameState, player: PlayerId, name: &str, color: ManaColor) {
    give_priority(state, player);
    let spell = put(state, player, name, Zone::Hand);
    match name {
        "Moment's Peace" => pool(state, player, &[ManaColor::G], 1),
        "Ethereal Haze" => pool(state, player, &[ManaColor::W], 0),
        "Prismatic Strands" => pool(state, player, &[ManaColor::W], 2),
        _ => panic!("unexpected fixture spell"),
    }
    cast(state, spell, &[]);
    if name == "Prismatic Strands" {
        assert!(matches!(
            settle(state),
            Some(Decision::ChooseEffectOption {
                option_count: 5,
                ..
            })
        ));
        let index = [
            ManaColor::W,
            ManaColor::U,
            ManaColor::B,
            ManaColor::R,
            ManaColor::G,
        ]
        .iter()
        .position(|candidate| *candidate == color)
        .unwrap();
        act(state, Action::ChooseEffectOption(index as u16));
    }
    settled(state);
}

fn combat_board(
    active: PlayerId,
    shield_count: usize,
    fog: Option<(&str, ManaColor)>,
) -> (GameState, ObjectId) {
    let defender = active.opponent();
    let mut state = turn_of(active);
    let attacker = put(&mut state, active, "Jackal Pup", Zone::Battlefield);
    decoy(&mut state, active, CircleColorV1::Red);
    if shield_count > 0 {
        let circle = put(&mut state, defender, RED_CIRCLE, Zone::Battlefield);
        let source = link(&state, attacker);
        for _ in 0..shield_count {
            shield(&mut state, defender, circle, source);
        }
    }
    if let Some((name, color)) = fog {
        global_prevention(&mut state, defender, name, color);
    }
    (state, attacker)
}

fn start_attack(state: &mut GameState, attacker: ObjectId) {
    super::pass_until(state, |state| state.step == Step::DeclareAttackers);
    act(state, Action::DeclareAttackers(vec![attacker]));
    super::pass_until(state, |state| state.step == Step::DeclareBlockers);
    act(state, Action::DeclareBlockers(Vec::new()));
}

fn combat_choice(state: &mut GameState) -> Option<Decision> {
    for _ in 0..100 {
        let decision = next(state);
        if state.step == Step::EndCombat {
            return None;
        }
        match decision {
            Decision::CastSpellOrPass { .. } => act(state, Action::Pass),
            Decision::OrderTriggers { pending, .. } => {
                act(state, Action::OrderTriggers((0..pending.len()).collect()))
            }
            decision @ Decision::ChooseEffectOption { .. } => return Some(decision),
            other => panic!("unexpected combat decision {other:?}"),
        }
    }
    panic!("combat did not complete");
}

fn choose_public(
    state: &mut GameState,
    actor: PlayerId,
    source: ObjectId,
    option: u16,
    count: u16,
) {
    let before = state.state_hash();
    assert!(engine::step(state, Action::ChooseEffectOption(count)).is_err());
    assert_eq!(state.state_hash(), before);
    let mut replay = restored(state);
    let mut live_surface = PolicySurfaceV5::new();
    let mut replay_surface = PolicySurfaceV5::new();
    let live_decision = live_surface.next_decision(state).unwrap();
    let replay_decision = replay_surface.next_decision(&mut replay).unwrap();
    let observation = observe_policy_v5(state, &live_surface, actor, 0, 0, 0, 1).unwrap();
    assert_eq!(observation.schema_version, 5);
    assert_eq!(
        observation,
        observe_policy_v5(&replay, &replay_surface, actor, 0, 0, 0, 1).unwrap()
    );
    let live = policy_legal_action_candidates_v5(&live_decision, &live_surface, state).unwrap();
    let actions =
        policy_legal_action_candidates_v5(&replay_decision, &replay_surface, &replay).unwrap();
    assert_eq!(actions.len(), usize::from(count));
    for (index, (candidate, replay)) in live.iter().zip(&actions).enumerate() {
        assert_eq!(candidate.record, replay.record);
        assert!(
            matches!(&candidate.record.semantic, ActionSemanticV1::ChooseEffectOption {
            actor: actual_actor, source: actual_source, option_index, option_count,
        } if *actual_actor == actor.into() && actual_source.arena_id == source.0
            && usize::from(*option_index) == index && *option_count == count)
        );
    }
    replay_surface
        .apply(
            &mut replay,
            actions[usize::from(option)].policy_action.clone(),
        )
        .unwrap();
    act(state, Action::ChooseEffectOption(option));
    assert_eq!(state.state_hash(), replay.state_hash());
}

#[test]
fn circle_or_global_prevention_can_spend_or_preserve_the_paid_shield_both_seats() {
    for active in [P0, P1] {
        for fog in ["Moment's Peace", "Ethereal Haze", "Prismatic Strands"] {
            let (mut start, attacker) = combat_board(active, 1, Some((fog, ManaColor::R)));
            start_attack(&mut start, attacker);
            assert!(
                matches!(combat_choice(&mut start), Some(Decision::ChooseEffectOption {
                player, source, option_count: 2,
            }) if player == active.opponent() && source == attacker)
            );
            for option in 0..2 {
                let mut state = restored(&start);
                choose_public(&mut state, active.opponent(), attacker, option, 2);
                assert!(combat_choice(&mut state).is_none());
                assert_eq!((life(&state, P0), life(&state, P1)), (20, 20));
                assert_eq!(shields(&state), usize::from(option == 1));
                assert!(state
                    .eternal_v1
                    .as_ref()
                    .unwrap()
                    .pf1c
                    .hallow_shields
                    .is_empty());
            }
        }
    }
}

#[test]
fn identical_circle_records_do_not_duplicate_equivalent_choices() {
    let (mut start, attacker) = combat_board(P0, 2, Some(("Moment's Peace", ManaColor::G)));
    start_attack(&mut start, attacker);
    assert!(matches!(
        combat_choice(&mut start),
        Some(Decision::ChooseEffectOption {
            option_count: 2,
            ..
        })
    ));
    for option in 0..2 {
        let mut state = restored(&start);
        choose_public(&mut state, P1, attacker, option, 2);
        assert!(combat_choice(&mut state).is_none());
        assert_eq!(shields(&state), if option == 0 { 1 } else { 2 });
    }
}

#[test]
fn unrelated_or_single_prevention_and_unpreventable_damage_have_no_new_menu() {
    for active in [P0, P1] {
        for (circle, fog, prohibit, expected_life, expected_shields) in [
            (0, None, false, 18, 0),
            (1, None, false, 20, 0),
            (0, Some(("Moment's Peace", ManaColor::G)), false, 20, 0),
            (1, Some(("Prismatic Strands", ManaColor::U)), false, 20, 0),
            (1, Some(("Moment's Peace", ManaColor::G)), true, 18, 1),
        ] {
            let (mut state, attacker) = combat_board(active, circle, fog);
            if prohibit {
                give_priority(&mut state, active);
                let pain = put(&mut state, active, "Flaring Pain", Zone::Hand);
                pool(&mut state, active, &[ManaColor::R], 1);
                cast(&mut state, pain, &[]);
                settled(&mut state);
            }
            start_attack(&mut state, attacker);
            assert!(combat_choice(&mut state).is_none());
            assert_eq!(life(&state, active.opponent()), expected_life);
            assert_eq!(shields(&state), expected_shields);
            assert!(state.engine.linked_aura_operation_v1.is_empty());
            assert_eq!(restored(&state).state_hash(), state.state_hash());
        }
    }
}

#[test]
fn simultaneous_player_damage_uses_independent_apnap_choices() {
    for active in [P0, P1] {
        let mut state = turn_of(active);
        let first_circle = put(&mut state, active, RED_CIRCLE, Zone::Battlefield);
        let second_circle = put(&mut state, active.opponent(), RED_CIRCLE, Zone::Battlefield);
        decoy(&mut state, active, CircleColorV1::Red);
        global_prevention(&mut state, active, "Prismatic Strands", ManaColor::R);
        give_priority(&mut state, active);
        let quake = put(&mut state, active, "Earthquake", Zone::Hand);
        pool(&mut state, active, &[ManaColor::R], 1);
        cast(&mut state, quake, &[]);
        assert!(
            matches!(next(&mut state), Decision::ChooseEffectOption { option_count, .. } if option_count > 1)
        );
        act(&mut state, Action::ChooseEffectOption(1));
        assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
        let source = link(&state, quake);
        shield(&mut state, active, first_circle, source);
        shield(&mut state, active.opponent(), second_circle, source);
        for (actor, option) in [(active, 0), (active.opponent(), 1)] {
            assert!(
                matches!(settle(&mut state), Some(Decision::ChooseEffectOption {
                player, source, option_count: 2,
            }) if player == actor && source == quake)
            );
            choose_public(&mut state, actor, quake, option, 2);
        }
        settled(&mut state);
        assert_eq!((life(&state, P0), life(&state, P1)), (20, 20));
        let remaining = &state.eternal_v1.as_ref().unwrap().s1d.cop_shields;
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].player, active.opponent());
    }
}
