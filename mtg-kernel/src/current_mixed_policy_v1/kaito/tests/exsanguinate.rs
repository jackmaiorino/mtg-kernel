use super::*;
use super::{
    surveil_many::{checkpoint, scene, sessions},
    targeted_entry::{actions, cast},
};
use crate::{policy_observation_v6::tests::put, state::Zone};

#[test]
fn paid_exsanguinate_native_x_menu_zero_positive_lethal_x256_counter_and_restore_both_seats() {
    let id = crate::card_def::card_id_by_name("Exsanguinate").unwrap();
    assert_eq!(
        super::super::super::roots::registered_spell_printing(id).unwrap(),
        Some(super::super::super::roots::baseline(id).unwrap())
    );
    for p in [PlayerId::P0, PlayerId::P1] {
        for x in [0u16, 3, 7, 256] {
            let (mut s, source, counter, _) = scene(p, "Exsanguinate", 40);
            // Starting life and prevention are support operands before any cast.
            let opposing_life = if x == 256 { 300 } else { 5 };
            s.players[p.opponent().index()].life = opposing_life;
            put(&mut s, p.opponent(), "Crystal Barricade", Zone::Battlefield);
            let swamps = if x == 256 { 260 } else { 12 };
            for _ in 0..swamps {
                put(&mut s, p, "Swamp", Zone::Battlefield);
            }
            let (mut own, mut native) = sessions(s);
            cast(&mut own, &mut native, source);
            let menu = actions(&own);
            let values = menu
                .iter()
                .filter_map(|a| match a {
                    A::ChooseEffectOption {
                        source: s,
                        option_index,
                        ..
                    } if s.arena_id == source.0 => Some(*option_index),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(values, (0..=(swamps + 8 - 2) as u16).collect::<Vec<_>>());
            checkpoint(&mut own, &mut native);
            act(
                &mut own,
                &mut native,
                |a| matches!(a,A::ChooseEffectOption{source:s,option_index,..}if s.arena_id==source.0&&*option_index==x),
            );
            assert_eq!(own.fixture_state().objects.get(source).zone, Zone::Stack);
            assert_eq!(
                own.fixture_state().players[p.index()]
                    .battlefield
                    .iter()
                    .filter(|&&o| own.fixture_state().objects.get(o).tapped)
                    .count(),
                usize::from(x) + 2
            );
            let FastActorResponseV1::Decision(h) = own.current_response() else {
                panic!();
            };
            let d = own.encode(h).unwrap();
            let stack = &d
                .backend
                .observation()
                .common
                .common
                .projection
                .surface
                .stack;
            assert_eq!(
                stack
                    .iter()
                    .find(|item| item.source.arena_id == source.0)
                    .unwrap()
                    .x_value,
                x
            );
            let paid = own.snapshot();
            let raw = native.snapshot_v1();
            for countered in [false, true] {
                let mut expected = None;
                for _ in 0..2 {
                    own.restore(&paid).unwrap();
                    native.kaito_restore_v75(&raw).unwrap();
                    if countered {
                        cast(&mut own, &mut native, counter);
                        act(&mut own, &mut native, |a| {
                            is_original_spell_target(a, source)
                        });
                    }
                    for _ in 0..64 {
                        if matches!(own.current_response(), FastActorResponseV1::Terminal(_))
                            || own.fixture_state().stack.is_empty()
                                && own.fixture_state().engine.pending_effect.is_none()
                        {
                            break;
                        }
                        act(&mut own, &mut native, |a| matches!(a, A::Pass { .. }));
                    }
                    let loss = if countered { 0 } else { i32::from(x) };
                    assert_eq!(
                        own.fixture_state().players[p.opponent().index()].life,
                        opposing_life - loss
                    );
                    assert_eq!(own.fixture_state().players[p.index()].life, 20 + loss);
                    if !countered && x == 7 {
                        let FastActorResponseV1::Terminal(t) = own.current_response() else {
                            panic!("lethal life loss did not end game");
                        };
                        assert_eq!(t.winner, Some(crate::rl::PlayerSeatV1::from(p)));
                        assert_eq!(
                            t.terminal_classification,
                            crate::rl::TerminalClassificationV1::Natural
                        );
                    } else {
                        assert!(matches!(
                            own.current_response(),
                            FastActorResponseV1::Decision(_)
                        ));
                        assert_eq!(
                            own.fixture_state().objects.get(source).zone,
                            Zone::Graveyard
                        );
                    }
                    if let Some(expected) = &expected {
                        assert_eq!(own.fixture_state(), expected);
                    } else {
                        expected = Some(own.fixture_state().clone());
                    }
                }
            }
        }
    }
}
