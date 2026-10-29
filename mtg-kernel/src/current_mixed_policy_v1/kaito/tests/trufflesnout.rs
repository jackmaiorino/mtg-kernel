use super::*;
use crate::{
    ids::ObjectId,
    policy_observation_v6::tests::put,
    state::{GameState, Step, Zone},
};

fn cast(own: &mut KaitoSession, native: &mut FastActorSessionV1, object: ObjectId) {
    act(
        own,
        native,
        |a| matches!(a, A::CastSpell { source, .. } if source.arena_id == object.0),
    );
}
fn target(own: &mut KaitoSession, native: &mut FastActorSessionV1, object: ObjectId) {
    act(
        own,
        native,
        |a| matches!(a, A::ChooseTarget { target: crate::rl::TargetRefV1::Object { object: o }, .. } if o.arena_id == object.0),
    );
}
fn foods(s: &GameState, p: PlayerId) -> Vec<ObjectId> {
    let food = crate::card_def::card_id_by_name("Food Token").unwrap();
    s.players[p.index()]
        .battlefield
        .iter()
        .copied()
        .filter(|&o| s.objects.get(o).card_def == food)
        .collect()
}

// Stop at the actual damage trigger, before it resolves. A blocked attack that
// deals no player damage instead reaches the next main phase with no trigger.
fn combat(
    own: &mut KaitoSession,
    native: &mut FastActorSessionV1,
    source: ObjectId,
    hawk: ObjectId,
    p: PlayerId,
    blocked: bool,
) {
    for _ in 0..256 {
        let s = own.fixture_state();
        if !s.stack.is_empty() && s.players[p.opponent().index()].life < 20 {
            assert_eq!(s.players[p.opponent().index()].life, 16);
            assert_eq!(s.players[p.index()].life, 20);
            assert!(foods(s, p).is_empty());
            return;
        }
        if s.active_player == p.opponent() && s.step == Step::Main1 {
            assert!(blocked);
            assert!(foods(s, p).is_empty());
            return;
        }
        let FastActorResponseV1::Decision(header) = own.current_response() else {
            panic!("combat ended unexpectedly");
        };
        let actions = own.encode(header).unwrap().backend.actions().to_vec();
        let selected = actions
            .iter()
            .find(|a| match a {
                A::ChooseAttackerInclusion {
                    attacker,
                    include: true,
                    ..
                } => attacker.arena_id == source.0,
                A::DeclareAttackers { attackers, .. } => {
                    attackers.len() == 1 && attackers[0].arena_id == source.0
                }
                A::ChooseBlockerInclusion {
                    attacker,
                    blocker,
                    include,
                    ..
                } => {
                    attacker.arena_id == source.0
                        && blocker.arena_id == hawk.0
                        && *include == blocked
                }
                A::ChooseCombatDamageRange { .. } | A::Pass { .. } => true,
                _ => false,
            })
            .expect("actual combat action absent")
            .clone();
        act(own, native, |a| a == &selected);
    }
    panic!("natural combat did not finish or publish the damage trigger");
}

#[test]
fn paid_trufflesnout_actual_combat_player_damage_food_blocked_departure_counter_and_restore_both_seats(
) {
    let id = crate::card_def::card_id_by_name("Eager Trufflesnout").unwrap();
    let meaning = super::super::super::roots::baseline(id).unwrap();
    assert_eq!(
        super::super::super::roots::registered_permanent_printing(id).unwrap(),
        Some(meaning)
    );
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut state = GameState::new_from_libraries_with_starting_player_v1(
            &[forest; 40],
            &[forest; 40],
            crate::rl::card_name,
            110_374,
            p,
        );
        state.step = Step::Main1;
        for _ in 0..3 {
            put(&mut state, p, "Forest", Zone::Battlefield);
        }
        for land in ["Island", "Swamp", "Forest"] {
            for _ in 0..3 {
                put(&mut state, p.opponent(), land, Zone::Battlefield);
            }
        }
        let source = put(&mut state, p, "Eager Trufflesnout", Zone::Hand);
        let hawk = put(&mut state, p.opponent(), "Healer's Hawk", Zone::Battlefield);
        let growth = put(&mut state, p.opponent(), "Giant Growth", Zone::Hand);
        let counter = put(&mut state, p.opponent(), "Counterspell", Zone::Hand);
        let removal = put(&mut state, p.opponent(), "Hero's Downfall", Zone::Hand);
        let mut native = FastActorSessionV1::from_kaito_v75_fixture_state(state.clone());
        let mut own =
            KaitoSession::from_session(FastActorSessionV1::from_kaito_v75_fixture_state(state))
                .unwrap();
        cast(&mut own, &mut native, source);
        assert_eq!(own.fixture_state().objects.get(source).zone, Zone::Stack);
        assert_eq!(
            own.fixture_state().players[p.index()]
                .battlefield
                .iter()
                .filter(|&&o| own.fixture_state().objects.get(o).tapped)
                .count(),
            3
        );
        assert_eq!(own.fixture_state().players[p.index()].mana_pool, [0; 6]);
        let saved = own.snapshot();
        let raw = native.snapshot_v1();
        // Unblocked; source removed after damage; source countered; no player
        // damage because the real Growth makes the sole blocker lethal-sized.
        for response in [0, 1, 2, 3] {
            let mut expected = None;
            for _ in 0..2 {
                own.restore(&saved).unwrap();
                native.kaito_restore_v75(&raw).unwrap();
                if response == 2 {
                    act(&mut own, &mut native, |a| matches!(a, A::Pass { .. }));
                    cast(&mut own, &mut native, counter);
                    act(&mut own, &mut native, |a| {
                        is_original_spell_target(a, source)
                    });
                }
                settle(&mut own, &mut native);
                assert!(foods(own.fixture_state(), p).is_empty());
                assert_eq!(
                    own.fixture_state().objects.get(source).zone,
                    if response == 2 {
                        Zone::Graveyard
                    } else {
                        Zone::Battlefield
                    }
                );
                super::ordinary_equipment::reach_main(&mut own, &mut native, p.opponent());
                super::ordinary_equipment::reach_main(&mut own, &mut native, p);
                if response != 2 {
                    assert!(!own.fixture_state().objects.get(source).summoning_sick);
                    if response == 3 {
                        act(&mut own, &mut native, |a| matches!(a, A::Pass { .. }));
                        cast(&mut own, &mut native, growth);
                        target(&mut own, &mut native, hawk);
                        settle(&mut own, &mut native);
                        assert_eq!(
                            crate::engine::checked_effective_pt(own.fixture_state(), hawk).unwrap(),
                            (4, 4)
                        );
                    }
                    combat(&mut own, &mut native, source, hawk, p, response == 3);
                    if response != 3 {
                        let pending = own.snapshot();
                        let pending_raw = native.snapshot_v1();
                        own.restore(&pending).unwrap();
                        native.kaito_restore_v75(&pending_raw).unwrap();
                        if response == 1 {
                            act(&mut own, &mut native, |a| matches!(a, A::Pass { .. }));
                            cast(&mut own, &mut native, removal);
                            target(&mut own, &mut native, source);
                        }
                        settle(&mut own, &mut native);
                        let tokens = foods(own.fixture_state(), p);
                        assert_eq!(tokens.len(), 1);
                        let token = own.fixture_state().objects.get(tokens[0]);
                        assert!(token.v4.is_token);
                        assert_eq!(token.owner, p);
                        assert_eq!(token.controller, p);
                        assert!(!token.tapped);
                    }
                }
                super::ordinary_equipment::reach_main(&mut own, &mut native, p.opponent());
                assert_eq!(
                    foods(own.fixture_state(), p).len(),
                    usize::from(response < 2)
                );
                assert_eq!(own.fixture_state().players[p.index()].life, 20);
                assert_eq!(
                    own.fixture_state().players[p.opponent().index()].life,
                    match response {
                        0 | 1 => 16,
                        3 => 24,
                        _ => 20,
                    }
                );
                assert_eq!(
                    own.fixture_state().objects.get(source).zone,
                    if response == 0 {
                        Zone::Battlefield
                    } else {
                        Zone::Graveyard
                    }
                );
                if let Some(ref previous) = expected {
                    assert_eq!(own.fixture_state(), previous);
                } else {
                    expected = Some(own.fixture_state().clone());
                }
            }
        }
    }
}
