use super::*;
use crate::{
    ids::ObjectId,
    policy_observation_v6::tests::put,
    state::{GameState, Step, Target, Zone},
};

fn choose(own: &mut KaitoSession, native: &mut FastActorSessionV1, id: ObjectId) {
    act(
        own,
        native,
        |a| matches!(a,A::ChooseTarget{target:crate::rl::TargetRefV1::Object{object},..}if object.arena_id==id.0),
    );
}
fn cast(own: &mut KaitoSession, native: &mut FastActorSessionV1, id: ObjectId) {
    act(
        own,
        native,
        |a| matches!(a,A::CastSpell{source,..}if source.arena_id==id.0),
    );
}
fn ready(p: PlayerId) -> GameState {
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    let mut s = GameState::new_from_libraries_with_starting_player_v1(
        &[forest; 40],
        &[forest; 40],
        crate::rl::card_name,
        110_211,
        p,
    );
    s.step = Step::Main1;
    for _ in 0..4 {
        put(&mut s, p, "Forest", Zone::Battlefield);
    }
    for name in ["Mountain", "Island"] {
        for _ in 0..2 {
            put(&mut s, p, name, Zone::Battlefield);
        }
    }
    for _ in 0..3 {
        put(&mut s, p.opponent(), "Swamp", Zone::Battlefield);
    }
    for name in ["Island", "Forest"] {
        for _ in 0..2 {
            put(&mut s, p.opponent(), name, Zone::Battlefield);
        }
    }
    s
}
#[test]
fn paid_bite_partial_targets_current_power_lifelink_counter_and_restore_both_seats() {
    paid_creature("Bite Down");
}
#[test]
fn paid_felling_counter_with_illegal_recipient_and_no_retaliation_full_restore_both_seats() {
    paid_creature("Felling Blow");
}

fn paid_creature(name: &str) {
    let felling = name == "Felling Blow";
    let id = crate::card_def::card_id_by_name(name).unwrap();
    let meaning = super::super::super::roots::baseline(id).unwrap();
    assert_eq!(
        super::super::super::roots::registered_spell_printing(id).unwrap(),
        Some(meaning)
    );
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p);
        // The both-illegal branch pays for Downfall and then Stab before
        // either resolves. Four Swamps cover the real 3+1 payment sequence.
        put(&mut s, p.opponent(), "Swamp", Zone::Battlefield);
        let source = put(&mut s, p, "Healer's Hawk", Zone::Battlefield);
        let victim = put(&mut s, p.opponent(), "Helpful Hunter", Zone::Battlefield);
        let spell = put(&mut s, p, name, Zone::Hand);
        let counter = put(&mut s, p.opponent(), "Counterspell", Zone::Hand);
        let downfall = put(&mut s, p.opponent(), "Hero's Downfall", Zone::Hand);
        let stab = put(&mut s, p.opponent(), "Stab", Zone::Hand);
        let strike = put(&mut s, p, "Sure Strike", Zone::Hand);
        let mut native = FastActorSessionV1::from_kaito_v75_fixture_state(s.clone());
        let mut own =
            KaitoSession::from_session(FastActorSessionV1::from_kaito_v75_fixture_state(s))
                .unwrap();
        cast(&mut own, &mut native, spell);
        choose(&mut own, &mut native, source);
        choose(&mut own, &mut native, victim);
        let saved = own.snapshot();
        let raw = native.snapshot_v1();
        for response in 0..6 {
            let mut expected = None;
            for _ in 0..2 {
                own.restore(&saved).unwrap();
                native.kaito_restore_v75(&raw).unwrap();
                if response == 4 {
                    cast(&mut own, &mut native, strike);
                    choose(&mut own, &mut native, source);
                } else if response != 0 {
                    act(&mut own, &mut native, |a| matches!(a, A::Pass { .. }));
                    if response == 1 {
                        cast(&mut own, &mut native, counter);
                        act(&mut own, &mut native, |a| {
                            is_original_spell_target(a, spell)
                        });
                    } else {
                        if matches!(response, 2 | 5) {
                            cast(&mut own, &mut native, downfall);
                            choose(&mut own, &mut native, source);
                        }
                        if matches!(response, 3 | 5) {
                            cast(&mut own, &mut native, stab);
                            choose(&mut own, &mut native, victim);
                        }
                    }
                }
                settle(&mut own, &mut native);
                let state = own.fixture_state();
                let source_gone = matches!(response, 2 | 5);
                let counters = i32::from(felling && matches!(response, 0 | 3 | 4));
                let dealt = if matches!(response, 0 | 4) {
                    1 + counters + if response == 4 { 3 } else { 0 }
                } else {
                    0
                };
                assert_eq!(state.objects.get(spell).zone, Zone::Graveyard);
                assert_eq!(
                    state.objects.get(source).zone,
                    if source_gone {
                        Zone::Graveyard
                    } else {
                        Zone::Battlefield
                    }
                );
                assert_eq!(state.objects.get(source).counters.plus1_plus1, counters);
                assert_eq!(
                    state.objects.get(source).damage,
                    0,
                    "one-way damage never retaliates"
                );
                assert_eq!(
                    state.objects.get(victim).zone,
                    if matches!(response, 1 | 2) {
                        Zone::Battlefield
                    } else {
                        Zone::Graveyard
                    }
                );
                assert_eq!(
                    state.players[p.index()].life,
                    20 + dealt,
                    "damage is dealt by the lifelink creature"
                );
                assert_eq!(state.players[p.opponent().index()].life, 20);
                let damage: Vec<_> = state
                    .engine
                    .event_history
                    .iter()
                    .filter_map(|e| match e {
                        crate::event::CommittedEvent::Damage {
                            source: actual,
                            target: Target::Object(target),
                            amount,
                        } if *actual == source && *target == victim => Some(*amount),
                        _ => None,
                    })
                    .collect();
                assert_eq!(damage, if dealt == 0 { vec![] } else { vec![dealt] });
                assert_eq!(state.players[p.index()].mana_pool, [0; 6]);
                assert_eq!(
                    state.players[p.index()]
                        .battlefield
                        .iter()
                        .filter(|&&o| state.objects.get(o).tapped)
                        .count(),
                    if felling { 3 } else { 2 } + if response == 4 { 2 } else { 0 }
                );
                if let Some(ref previous) = expected {
                    assert_eq!(state, previous);
                } else {
                    expected = Some(state.clone());
                }
            }
        }
    }
}

#[test]
fn paid_planeswalker_target_domain_uses_real_liliana_cast_and_native_restore_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for name in ["Bite Down", "Felling Blow"] {
            let felling = name == "Felling Blow";
            let mut s = ready(p.opponent());
            // p's resources above are assigned to the starting caster; add
            // only initial resources for the later green caster and Liliana.
            for _ in 0..6 {
                put(&mut s, p.opponent(), "Swamp", Zone::Battlefield);
            }
            for _ in 0..4 {
                put(&mut s, p, "Forest", Zone::Battlefield);
            }
            let source = put(&mut s, p, "Healer's Hawk", Zone::Battlefield);
            let victim = put(&mut s, p.opponent(), "Llanowar Elves", Zone::Battlefield);
            let liliana = put(
                &mut s,
                p.opponent(),
                "Liliana, Dreadhorde General",
                Zone::Hand,
            );
            let spell = put(&mut s, p, name, Zone::Hand);
            let mut native = FastActorSessionV1::from_kaito_v75_fixture_state(s.clone());
            let mut own =
                KaitoSession::from_session(FastActorSessionV1::from_kaito_v75_fixture_state(s))
                    .unwrap();
            cast(&mut own, &mut native, liliana);
            settle(&mut own, &mut native);
            assert_eq!(
                crate::planeswalker_v1::loyalty(own.fixture_state(), liliana),
                Some(6)
            );
            super::ordinary_equipment::reach_main(&mut own, &mut native, p);
            cast(&mut own, &mut native, spell);
            choose(&mut own, &mut native, source);
            let FastActorResponseV1::Decision(h) = own.current_response() else {
                panic!()
            };
            let decision = own.encode(h).unwrap();
            assert_eq!(decision.backend.actions().iter().any(|a|matches!(a,
                A::ChooseTarget{target:crate::rl::TargetRefV1::Object{object},..}if object.arena_id==liliana.0)),!felling);
            choose(
                &mut own,
                &mut native,
                if felling { victim } else { liliana },
            );
            let saved = own.snapshot();
            let raw = native.snapshot_v1();
            let mut expected = None;
            for _ in 0..2 {
                own.restore(&saved).unwrap();
                native.kaito_restore_v75(&raw).unwrap();
                settle(&mut own, &mut native);
                let state = own.fixture_state();
                assert_eq!(
                    crate::planeswalker_v1::loyalty(state, liliana),
                    Some(if felling { 6 } else { 5 })
                );
                assert_eq!(
                    state.objects.get(source).counters.plus1_plus1,
                    i32::from(felling)
                );
                assert_eq!(state.objects.get(source).damage, 0);
                assert_eq!(state.players[p.index()].life, if felling { 22 } else { 21 });
                assert_eq!(
                    state.objects.get(victim).zone,
                    if felling {
                        Zone::Graveyard
                    } else {
                        Zone::Battlefield
                    }
                );
                assert_eq!(
                    state.players[p.opponent().index()].library.len(),
                    if felling { 39 } else { 40 },
                    "Liliana draws for the actual dying controlled Elf only"
                );
                assert_eq!(state.objects.get(spell).zone, Zone::Graveyard);
                if let Some(ref previous) = expected {
                    assert_eq!(state, previous);
                } else {
                    expected = Some(state.clone());
                }
            }
        }
    }
}
