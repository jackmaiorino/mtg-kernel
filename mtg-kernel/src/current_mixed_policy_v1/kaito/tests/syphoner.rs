use super::*;
use crate::{
    ids::ObjectId,
    policy_observation_v6::tests::put,
    state::{GameState, Step, Zone},
};

pub(super) fn declare_paid_source(
    own: &mut KaitoSession,
    native: &mut FastActorSessionV1,
    source: ObjectId,
) {
    for _ in 0..128 {
        let s = own.fixture_state();
        if s.engine.combat.attackers.contains(&source) && !s.stack.is_empty() {
            return;
        }
        let FastActorResponseV1::Decision(header) = own.current_response() else {
            panic!("attack declaration ended unexpectedly");
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
                _ => false,
            })
            .or_else(|| actions.iter().find(|a| matches!(a, A::Pass { .. })))
            .expect("no actual attack/priority action")
            .clone();
        act(own, native, |a| a == &selected);
    }
    panic!("paid Syphoner did not actually attack and publish its trigger");
}

#[test]
fn paid_syphoner_natural_ready_attack_nontargeted_life_departure_counter_and_restore_both_seats() {
    let id = crate::card_def::card_id_by_name("Sanguine Syphoner").unwrap();
    assert_eq!(
        super::super::super::roots::registered_permanent_printing(id).unwrap(),
        Some(super::super::super::roots::baseline(id).unwrap())
    );
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut state = GameState::new_from_libraries_with_starting_player_v1(
            &[forest; 40],
            &[forest; 40],
            crate::rl::card_name,
            110_370,
            p,
        );
        state.step = Step::Main1;
        for _ in 0..3 {
            put(&mut state, p, "Swamp", Zone::Battlefield);
        }
        for land in ["Island", "Swamp"] {
            for _ in 0..3 {
                put(&mut state, p.opponent(), land, Zone::Battlefield);
            }
        }
        let source = put(&mut state, p, "Sanguine Syphoner", Zone::Hand);
        let counter = put(&mut state, p.opponent(), "Counterspell", Zone::Hand);
        let removal = put(&mut state, p.opponent(), "Hero's Downfall", Zone::Hand);
        let mut native = FastActorSessionV1::from_kaito_v75_fixture_state(state.clone());
        let mut own =
            KaitoSession::from_session(FastActorSessionV1::from_kaito_v75_fixture_state(state))
                .unwrap();
        act(
            &mut own,
            &mut native,
            |a| matches!(a,A::CastSpell{source:s,..}if s.arena_id==source.0),
        );
        assert_eq!(own.fixture_state().objects.get(source).zone, Zone::Stack);
        assert_eq!(
            own.fixture_state().players[p.index()]
                .battlefield
                .iter()
                .filter(|&&o| own.fixture_state().objects.get(o).tapped)
                .count(),
            2
        );
        let saved = own.snapshot();
        let raw = native.snapshot_v1();
        for response in [0, 1, 2] {
            let mut expected = None;
            for _ in 0..2 {
                own.restore(&saved).unwrap();
                native.kaito_restore_v75(&raw).unwrap();
                if response == 1 {
                    act(&mut own, &mut native, |a| matches!(a, A::Pass { .. }));
                    act(
                        &mut own,
                        &mut native,
                        |a| matches!(a,A::CastSpell{source:s,..}if s.arena_id==counter.0),
                    );
                    act(&mut own, &mut native, |a| {
                        is_original_spell_target(a, source)
                    });
                }
                settle(&mut own, &mut native);
                assert_eq!(
                    own.fixture_state().objects.get(source).zone,
                    if response == 1 {
                        Zone::Graveyard
                    } else {
                        Zone::Battlefield
                    }
                );
                assert_eq!(own.fixture_state().players[p.index()].mana_pool, [0; 6]);
                assert_eq!(own.fixture_state().players[p.index()].life, 20);
                assert_eq!(own.fixture_state().players[p.opponent().index()].life, 20);
                super::ordinary_equipment::reach_main(&mut own, &mut native, p.opponent());
                super::ordinary_equipment::reach_main(&mut own, &mut native, p);
                if response != 1 {
                    assert!(!own.fixture_state().objects.get(source).summoning_sick);
                    assert!(!own.fixture_state().objects.get(source).tapped);
                    declare_paid_source(&mut own, &mut native, source);
                    assert!(own.fixture_state().objects.get(source).tapped);
                    assert!(own
                        .fixture_state()
                        .engine
                        .combat
                        .attackers
                        .contains(&source));
                    assert_eq!(own.fixture_state().players[p.index()].life, 20);
                    assert_eq!(own.fixture_state().players[p.opponent().index()].life, 20);
                    let pending = own.snapshot();
                    let pending_raw = native.snapshot_v1();
                    own.restore(&pending).unwrap();
                    native.kaito_restore_v75(&pending_raw).unwrap();
                    if response == 2 {
                        act(&mut own, &mut native, |a| matches!(a, A::Pass { .. }));
                        act(
                            &mut own,
                            &mut native,
                            |a| matches!(a,A::CastSpell{source:s,..}if s.arena_id==removal.0),
                        );
                        act(
                            &mut own,
                            &mut native,
                            |a| matches!(a,A::ChooseTarget{target:crate::rl::TargetRefV1::Object{object},..}if object.arena_id==source.0),
                        );
                    }
                    settle(&mut own, &mut native);
                    assert_eq!(own.fixture_state().players[p.index()].life, 21);
                    assert_eq!(own.fixture_state().players[p.opponent().index()].life, 19);
                    assert_eq!(
                        own.fixture_state().objects.get(source).zone,
                        if response == 2 {
                            Zone::Graveyard
                        } else {
                            Zone::Battlefield
                        }
                    );
                }
                super::ordinary_equipment::reach_main(&mut own, &mut native, p.opponent());
                assert_eq!(
                    own.fixture_state().players[p.index()].life,
                    if response == 1 { 20 } else { 21 }
                );
                assert_eq!(
                    own.fixture_state().players[p.opponent().index()].life,
                    match response {
                        0 => 18,
                        1 => 20,
                        _ => 19,
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
