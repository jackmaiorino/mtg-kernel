use super::*;
use super::{
    surveil_many::{checkpoint, scene, sessions},
    targeted_entry::{actions, cast, choose},
};
use crate::{
    policy_observation_v6::tests::put,
    state::{Step, Zone},
};

fn quiet(a: &A) -> bool {
    matches!(
        a,
        A::Pass { .. }
            | A::OrderTriggers { .. }
            | A::ChooseTriggerOrderNext { .. }
            | A::ChooseAttackerInclusion { include: false, .. }
            | A::ChooseBlockerInclusion { include: false, .. }
    )
}
fn registered(name: &str) {
    let id = crate::card_def::card_id_by_name(name).unwrap();
    assert_eq!(
        super::super::super::roots::registered_permanent_printing(id).unwrap(),
        Some(super::super::super::roots::baseline(id).unwrap())
    );
}
fn ward(name: &str) {
    registered(name);
    for p in [PlayerId::P0, PlayerId::P1] {
        let (mut s, source, counter, opposing_removal) = scene(p, name, 40);
        for land in ["Forest", "Swamp"] {
            for _ in 0..4 {
                put(&mut s, p, land, Zone::Battlefield);
            }
        }
        let own_removal = put(&mut s, p, "Hero's Downfall", Zone::Hand);
        let (mut own, mut native) = sessions(s);
        cast(&mut own, &mut native, source);
        let paid = own.snapshot();
        let raw = native.snapshot_v1();
        for branch in 0..4 {
            let mut expected = None;
            for _ in 0..2 {
                own.restore(&paid).unwrap();
                native.kaito_restore_v75(&raw).unwrap();
                if branch == 0 {
                    cast(&mut own, &mut native, counter);
                    act(&mut own, &mut native, |a| {
                        is_original_spell_target(a, source)
                    });
                    settle(&mut own, &mut native);
                    assert_eq!(
                        own.fixture_state().objects.get(source).zone,
                        Zone::Graveyard
                    );
                } else {
                    settle(&mut own, &mut native);
                    assert_eq!(
                        own.fixture_state().objects.get(source).zone,
                        Zone::Battlefield
                    );
                    cast(&mut own, &mut native, opposing_removal);
                    choose(&mut own, &mut native, opposing_removal, source);
                    if branch == 3 {
                        cast(&mut own, &mut native, own_removal);
                        choose(&mut own, &mut native, own_removal, source);
                        for _ in 0..32 {
                            if own.fixture_state().objects.get(source).zone == Zone::Graveyard {
                                break;
                            }
                            act(&mut own, &mut native, quiet);
                        }
                        assert_eq!(
                            own.fixture_state().objects.get(source).zone,
                            Zone::Graveyard
                        );
                    }
                    for _ in 0..64 {
                        if actions(&own)
                            .iter()
                            .any(|a| matches!(a, A::ChooseEffectBoolean { .. }))
                        {
                            break;
                        }
                        act(&mut own, &mut native, quiet);
                    }
                    let answer = actions(&own)
                        .into_iter()
                        .find(|a| {
                            matches!(a,
                        A::ChooseEffectBoolean { value, .. } if *value == (branch != 2))
                        })
                        .expect("ward payment absent");
                    checkpoint(&mut own, &mut native);
                    let life = own.fixture_state().players[p.opponent().index()].life;
                    let tap_count = tapped(&own, p.opponent());
                    act(&mut own, &mut native, |a| a == &answer);
                    settle(&mut own, &mut native);
                    assert_eq!(
                        own.fixture_state().objects.get(source).zone,
                        if branch == 2 {
                            Zone::Battlefield
                        } else {
                            Zone::Graveyard
                        }
                    );
                    assert_eq!(
                        own.fixture_state().objects.get(opposing_removal).zone,
                        Zone::Graveyard
                    );
                    assert_eq!(
                        own.fixture_state().players[p.opponent().index()].life,
                        life - if name == "Sire of Seven Deaths" && branch != 2 {
                            7
                        } else {
                            0
                        }
                    );
                    assert_eq!(
                        tapped(&own, p.opponent()) - tap_count,
                        if name != "Sire of Seven Deaths" && branch != 2 {
                            2
                        } else {
                            0
                        }
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
fn tapped(own: &KaitoSession, p: PlayerId) -> usize {
    own.fixture_state().players[p.index()]
        .battlefield
        .iter()
        .filter(|&&o| own.fixture_state().objects.get(o).tapped)
        .count()
}
#[test]
fn paid_prowler_generic_ward_pay_decline_counter_source_departure_restore_both_seats() {
    ward("Cackling Prowler");
}
#[test]
fn paid_terror_generic_ward_pay_decline_counter_source_departure_restore_both_seats() {
    ward("Tolarian Terror");
}
#[test]
fn paid_sire_life_ward_pay_decline_counter_source_departure_restore_both_seats() {
    ward("Sire of Seven Deaths");
}

fn discount(name: &str) {
    registered(name);
    let terror = name == "Tolarian Terror";
    for p in [PlayerId::P0, PlayerId::P1] {
        for count in [0, 1, 4] {
            let (mut s, source, counter, removal) = scene(p, name, 40);
            for _ in 0..12 {
                put(&mut s, p, "Forest", Zone::Battlefield);
            }
            let mut support = vec![];
            for _ in 0..count {
                if terror {
                    put(&mut s, p, "Stab", Zone::Graveyard);
                    put(&mut s, p, "Boltwave", Zone::Graveyard);
                } else {
                    support.push(put(&mut s, p, "Serra Angel", Zone::Battlefield));
                }
            }
            for q in [p, p.opponent()] {
                put(&mut s, q, "Forest", Zone::Graveyard);
            }
            put(&mut s, p.opponent(), "Stab", Zone::Graveyard);
            put(&mut s, p.opponent(), "Boltwave", Zone::Graveyard);
            put(&mut s, p.opponent(), "Serra Angel", Zone::Battlefield);
            put(&mut s, p, "Serra Angel", Zone::Graveyard);
            let (mut own, mut native) = sessions(s);
            cast(&mut own, &mut native, source);
            assert_eq!(
                tapped(&own, p),
                if terror {
                    7 - (2 * count).min(6)
                } else {
                    12 - (4 * count).min(10)
                }
            );
            let paid = own.snapshot();
            let raw = native.snapshot_v1();
            for branch in 0..3 {
                if branch == 2 && (terror || support.is_empty()) {
                    continue;
                }
                let mut expected = None;
                for _ in 0..2 {
                    own.restore(&paid).unwrap();
                    native.kaito_restore_v75(&raw).unwrap();
                    let cost = tapped(&own, p);
                    if branch == 1 {
                        cast(&mut own, &mut native, counter);
                        act(&mut own, &mut native, |a| {
                            is_original_spell_target(a, source)
                        });
                    } else if branch == 2 {
                        cast(&mut own, &mut native, removal);
                        choose(&mut own, &mut native, removal, support[0]);
                    }
                    settle(&mut own, &mut native);
                    assert_eq!(
                        own.fixture_state().objects.get(source).zone,
                        if branch == 1 {
                            Zone::Graveyard
                        } else {
                            Zone::Battlefield
                        }
                    );
                    assert_eq!(
                        tapped(&own, p),
                        cost,
                        "finalized cost changed after response"
                    );
                    if branch == 2 {
                        assert_eq!(
                            own.fixture_state().objects.get(support[0]).zone,
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
#[test]
fn paid_terror_printed_graveyard_types_floor_colored_cost_counter_restore_both_seats() {
    discount("Tolarian Terror");
}
#[test]
fn paid_ghalta_current_controlled_power_floor_colored_cost_locked_removal_counter_restore_both_seats(
) {
    discount("Ghalta, Primal Hunger");
}

#[test]
fn paid_prowler_morbid_zero_or_opposing_death_original_source_counter_and_restore_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for died in [false, true] {
            let (mut s, source, _, _) = scene(p, "Cackling Prowler", 40);
            for land in ["Forest", "Swamp"] {
                for _ in 0..4 {
                    put(&mut s, p, land, Zone::Battlefield);
                }
            }
            let victim = put(&mut s, p.opponent(), "Healer's Hawk", Zone::Battlefield);
            let removal = put(&mut s, p, "Hero's Downfall", Zone::Hand);
            let (mut own, mut native) = sessions(s);
            cast(&mut own, &mut native, source);
            settle(&mut own, &mut native);
            if died {
                cast(&mut own, &mut native, removal);
                choose(&mut own, &mut native, removal, victim);
                settle(&mut own, &mut native);
                assert_eq!(
                    own.fixture_state().objects.get(victim).zone,
                    Zone::Graveyard
                );
            }
            let saved = own.snapshot();
            let raw = native.snapshot_v1();
            let mut expected = None;
            for _ in 0..2 {
                own.restore(&saved).unwrap();
                native.kaito_restore_v75(&raw).unwrap();
                for _ in 0..128 {
                    let s = own.fixture_state();
                    if s.step == Step::End
                        && s.stack.is_empty()
                        && s.engine.pending_effect.is_none()
                        && s.engine.pending_triggers.is_empty()
                    {
                        break;
                    }
                    act(&mut own, &mut native, quiet);
                }
                assert_eq!(own.fixture_state().step, Step::End);
                assert_eq!(
                    own.fixture_state().objects.get(source).counters.plus1_plus1,
                    i32::from(died)
                );
                if let Some(expected) = &expected {
                    assert_eq!(own.fixture_state(), expected);
                } else {
                    expected = Some(own.fixture_state().clone());
                }
            }
        }
    }
}
