use super::*;
use super::{
    ordinary_equipment::reach_main,
    surveil_many::{checkpoint, scene, sessions},
    targeted_entry::{actions, cast, choose, wait_target},
};
use crate::{card_def::Keywords, ids::ObjectId, policy_observation_v6::tests::put, state::Zone};

fn declare(own: &mut KaitoSession, native: &mut FastActorSessionV1, attackers: &[ObjectId]) {
    for _ in 0..128 {
        if own.fixture_state().engine.combat.attackers.len() == attackers.len()
            && attackers
                .iter()
                .all(|a| own.fixture_state().engine.combat.attackers.contains(a))
        {
            return;
        }
        let menu = actions(own);
        let selected = menu
            .iter()
            .find(|a| match a {
                A::ChooseAttackerInclusion {
                    attacker, include, ..
                } => *include == attackers.iter().any(|o| o.0 == attacker.arena_id),
                A::DeclareAttackers {
                    attackers: offered, ..
                } => {
                    offered.len() == attackers.len()
                        && offered
                            .iter()
                            .all(|a| attackers.iter().any(|o| o.0 == a.arena_id))
                }
                _ => false,
            })
            .or_else(|| menu.iter().find(|a| matches!(a, A::Pass { .. })))
            .expect("actual attack declaration absent")
            .clone();
        act(own, native, |a| a == &selected);
    }
    panic!("paid attack declaration did not complete");
}

fn remove(
    own: &mut KaitoSession,
    native: &mut FastActorSessionV1,
    spell: ObjectId,
    target: ObjectId,
) {
    cast(own, native, spell);
    choose(own, native, spell, target);
}

fn second_main(own: &mut KaitoSession, native: &mut FastActorSessionV1, player: PlayerId) {
    for _ in 0..256 {
        let s = own.fixture_state();
        assert_eq!(s.active_player, player);
        if s.step == crate::state::Step::Main2 && s.stack.is_empty() {
            return;
        }
        act(own, native, |a| {
            matches!(
                a,
                A::Pass { .. } | A::ChooseBlockerInclusion { include: false, .. }
            )
        });
    }
    panic!("actual attack did not reach the same turn's second main phase");
}

fn paid(name: &str) {
    let id = crate::card_def::card_id_by_name(name).unwrap();
    let meaning = super::super::super::roots::baseline(id).unwrap();
    assert_eq!(
        super::super::super::roots::registered_permanent_printing(id).unwrap(),
        Some(meaning)
    );
    let targeted = matches!(name, "Armasaur Guide" | "Battlesong Berserker");
    for p in [PlayerId::P0, PlayerId::P1] {
        let (mut s, source, counter, removal) = scene(p, name, 40);
        for land in ["Plains", "Swamp", "Mountain"] {
            for _ in 0..5 {
                put(&mut s, p, land, Zone::Battlefield);
            }
        }
        for _ in 0..3 {
            put(&mut s, p.opponent(), "Forest", Zone::Battlefield);
        }
        if name == "Ruby, Daring Tracker" {
            put(&mut s, p, "Forest", Zone::Battlefield);
        }
        let hawk = put(&mut s, p, "Healer's Hawk", Zone::Battlefield);
        let angel = put(&mut s, p, "Serra Angel", Zone::Battlefield);
        let late = put(&mut s, p, "Healer's Hawk", Zone::Hand);
        let ooze = if name == "Crypt Feaster" {
            Some(put(
                &mut s,
                p.opponent(),
                "Scavenging Ooze",
                Zone::Battlefield,
            ))
        } else {
            None
        };
        let grave = (0..7)
            .map(|_| put(&mut s, p, "Forest", Zone::Graveyard))
            .collect::<Vec<_>>();
        let (mut own, mut native) = sessions(s);
        cast(&mut own, &mut native, source);
        assert_eq!(own.fixture_state().objects.get(source).zone, Zone::Stack);
        assert_eq!(
            own.fixture_state().players[p.index()]
                .battlefield
                .iter()
                .filter(|&&o| own.fixture_state().objects.get(o).tapped)
                .count(),
            usize::from(crate::card_def::CARD_DEFS[usize::from(id)].mana_value)
        );
        let paid = own.snapshot();
        let raw = native.snapshot_v1();
        // Original paid roots, counter, source departure and target/condition response.
        for branch in [0, 1, 2, 3] {
            let mut expected = None;
            for _ in 0..2 {
                own.restore(&paid).unwrap();
                native.kaito_restore_v75(&raw).unwrap();
                if branch == 1 {
                    cast(&mut own, &mut native, counter);
                    act(&mut own, &mut native, |a| {
                        is_original_spell_target(a, source)
                    });
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
                reach_main(&mut own, &mut native, p.opponent());
                reach_main(&mut own, &mut native, p);
                if branch != 1 {
                    assert!(!own.fixture_state().objects.get(source).summoning_sick);
                    let attackers = if name == "Armasaur Guide" {
                        vec![source, hawk, angel]
                    } else {
                        vec![source]
                    };
                    declare(&mut own, &mut native, &attackers);
                    if targeted {
                        wait_target(&mut own, &mut native, source, hawk);
                        checkpoint(&mut own, &mut native);
                        choose(&mut own, &mut native, source, hawk);
                    }
                    assert!(!own.fixture_state().stack.is_empty());
                    checkpoint(&mut own, &mut native);
                    if branch == 2 {
                        remove(&mut own, &mut native, removal, source);
                    } else if branch == 3 {
                        if targeted {
                            remove(&mut own, &mut native, removal, hawk);
                        } else if let Some(ooze) = ooze {
                            act(&mut own, &mut native, |a| matches!(a, A::Pass { .. }));
                            act(
                                &mut own,
                                &mut native,
                                |a| matches!(a,A::ActivateAbility{source:s,ability_index:0,..} if s.arena_id==ooze.0),
                            );
                            choose(&mut own, &mut native, ooze, grave[0]);
                        } else {
                            // Goblin's power condition is not an intervening-if.
                            // Veteran samples only creatures still controlled at resolution.
                            remove(&mut own, &mut native, removal, angel);
                        }
                    }
                    settle(&mut own, &mut native);
                    let state = own.fixture_state();
                    assert_eq!(
                        state.objects.get(source).zone,
                        if branch == 2 {
                            Zone::Graveyard
                        } else {
                            Zone::Battlefield
                        }
                    );
                    if targeted && branch == 3 {
                        assert_eq!(state.objects.get(hawk).zone, Zone::Graveyard);
                    } else {
                        let boost = if name == "Dauntless Veteran" || name == "Battlesong Berserker"
                        {
                            (1, i32::from(name == "Dauntless Veteran"))
                        } else if name == "Armasaur Guide" {
                            (1, 1)
                        } else {
                            (0, 0)
                        };
                        assert_eq!(
                            crate::engine::checked_effective_pt(state, hawk).unwrap(),
                            (1 + boost.0, 1 + boost.1)
                        );
                        assert_eq!(
                            state.objects.get(hawk).counters.plus1_plus1,
                            i32::from(name == "Armasaur Guide")
                        );
                        assert_eq!(
                            crate::engine::has_effective_keyword(state, hawk, Keywords::MENACE),
                            name == "Battlesong Berserker"
                        );
                    }
                    if branch != 2 {
                        let boost = match name {
                            "Crypt Feaster" if branch != 3 => (2, 0),
                            "Dauntless Veteran" => (1, 1),
                            "Courageous Goblin" => (1, 0),
                            "Ruby, Daring Tracker" => (2, 2),
                            _ => (0, 0),
                        };
                        let d = &crate::card_def::CARD_DEFS[usize::from(id)];
                        assert_eq!(
                            crate::engine::checked_effective_pt(state, source).unwrap(),
                            (
                                i32::from(d.power.unwrap()) + boost.0,
                                i32::from(d.toughness.unwrap()) + boost.1
                            )
                        );
                        assert_eq!(
                            crate::engine::has_effective_keyword(state, source, Keywords::MENACE),
                            matches!(name, "Crypt Feaster" | "Courageous Goblin")
                        );
                    }
                    second_main(&mut own, &mut native, p);
                    cast(&mut own, &mut native, late);
                    settle(&mut own, &mut native);
                    assert_eq!(
                        crate::engine::checked_effective_pt(own.fixture_state(), late).unwrap(),
                        (1, 1)
                    );
                    reach_main(&mut own, &mut native, p.opponent());
                    // Counters persist; temporary power and keyword grants expire.
                    if !targeted || branch != 3 {
                        let counter = i32::from(name == "Armasaur Guide");
                        assert_eq!(
                            crate::engine::checked_effective_pt(own.fixture_state(), hawk).unwrap(),
                            (1 + counter, 1 + counter)
                        );
                        assert!(!crate::engine::has_effective_keyword(
                            own.fixture_state(),
                            hawk,
                            Keywords::MENACE
                        ));
                    }
                }
                assert_eq!(own.fixture_state().players[p.index()].mana_pool, [0; 6]);
                if let Some(ref previous) = expected {
                    assert_eq!(own.fixture_state(), previous);
                } else {
                    expected = Some(own.fixture_state().clone());
                }
            }
        }
    }
}

#[test]
fn paid_armasaur_attack_target_counter_source_departure_and_restore_both_seats() {
    paid("Armasaur Guide");
}
#[test]
fn paid_battlesong_attack_target_pump_menace_expiry_and_restore_both_seats() {
    paid("Battlesong Berserker");
}
#[test]
fn paid_crypt_feaster_attack_threshold_recheck_source_departure_and_restore_both_seats() {
    paid("Crypt Feaster");
}
#[test]
fn paid_dauntless_attack_resolution_set_departure_expiry_and_restore_both_seats() {
    paid("Dauntless Veteran");
}
#[test]
fn paid_courageous_attack_power_condition_departure_expiry_and_restore_both_seats() {
    paid("Courageous Goblin");
}

#[test]
fn paid_ruby_attack_power_condition_departure_expiry_and_restore_both_seats() {
    paid("Ruby, Daring Tracker");
}

#[test]
fn paid_ruby_haste_attacks_or_chooses_one_red_or_green_mana_immediately_after_entry_both_seats() {
    use crate::{
        mana::ManaColor,
        state::{GameState, Step},
    };
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = GameState::new_from_libraries_with_starting_player_v1(
            &[forest; 40],
            &[forest; 40],
            crate::rl::card_name,
            110_409,
            p,
        );
        s.step = Step::Main1;
        let lands = ["Mountain", "Forest"].map(|n| put(&mut s, p, n, Zone::Battlefield));
        let source = put(&mut s, p, "Ruby, Daring Tracker", Zone::Hand);
        let (mut own, mut native) = sessions(s);
        cast(&mut own, &mut native, source);
        assert!(lands
            .iter()
            .all(|&o| own.fixture_state().objects.get(o).tapped));
        assert_eq!(own.fixture_state().players[p.index()].mana_pool, [0; 6]);
        settle(&mut own, &mut native);
        assert!(own.fixture_state().objects.get(source).summoning_sick);
        assert!(crate::engine::has_effective_keyword(
            own.fixture_state(),
            source,
            Keywords::HASTE
        ));
        let choices = actions(&own)
            .into_iter()
            .filter_map(|a| match a {
                A::ActivateManaAbility {
                    source: s,
                    mana_choice,
                    ..
                } if s.arena_id == source.0 => mana_choice,
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(choices.len(), 2);
        assert!(choices.contains(&ManaColor::R) && choices.contains(&ManaColor::G));
        checkpoint(&mut own, &mut native);
        let saved = own.snapshot();
        let raw = native.snapshot_v1();
        for color in [ManaColor::R, ManaColor::G] {
            for _ in 0..2 {
                own.restore(&saved).unwrap();
                native.kaito_restore_v75(&raw).unwrap();
                act(
                    &mut own,
                    &mut native,
                    |a| matches!(a,A::ActivateManaAbility{source:s,mana_choice:Some(c),..}if s.arena_id==source.0&&*c==color),
                );
                assert!(own.fixture_state().objects.get(source).tapped);
                assert!(own.fixture_state().stack.is_empty());
                let mut expected = [0; 6];
                expected[color.pool_index()] = 1;
                assert_eq!(own.fixture_state().players[p.index()].mana_pool, expected);
                assert!(!actions(&own).iter().any(
                    |a| matches!(a,A::ActivateManaAbility{source:s,..}if s.arena_id==source.0)
                ));
            }
        }
        own.restore(&saved).unwrap();
        native.kaito_restore_v75(&raw).unwrap();
        declare(&mut own, &mut native, &[source]);
        assert!(own.fixture_state().objects.get(source).summoning_sick);
        assert!(own.fixture_state().objects.get(source).tapped);
        assert!(own.fixture_state().stack.is_empty());
        assert_eq!(
            crate::engine::checked_effective_pt(own.fixture_state(), source).unwrap(),
            (1, 2)
        );
        checkpoint(&mut own, &mut native);
        second_main(&mut own, &mut native, p);
        assert_eq!(own.fixture_state().players[p.opponent().index()].life, 19);
    }
}

#[test]
fn paid_attack_capture_boundaries_and_nonattacking_observers_restore_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for name in [
            "Armasaur Guide",
            "Battlesong Berserker",
            "Crypt Feaster",
            "Courageous Goblin",
            "Ruby, Daring Tracker",
        ] {
            for enabled in [false, true] {
                let (mut s, source, _, _) = scene(p, name, 40);
                for land in ["Plains", "Swamp", "Mountain"] {
                    for _ in 0..5 {
                        put(&mut s, p, land, Zone::Battlefield);
                    }
                }
                let hawks = (0..3)
                    .map(|_| put(&mut s, p, "Healer's Hawk", Zone::Battlefield))
                    .collect::<Vec<_>>();
                if name == "Ruby, Daring Tracker" {
                    put(&mut s, p, "Forest", Zone::Battlefield);
                }
                if matches!(name, "Courageous Goblin" | "Ruby, Daring Tracker") && enabled {
                    put(&mut s, p, "Serra Angel", Zone::Battlefield);
                }
                for _ in 0..(6 + usize::from(enabled)) {
                    put(&mut s, p, "Forest", Zone::Graveyard);
                }
                let (mut own, mut native) = sessions(s);
                cast(&mut own, &mut native, source);
                settle(&mut own, &mut native);
                reach_main(&mut own, &mut native, p.opponent());
                reach_main(&mut own, &mut native, p);
                let ready = own.snapshot();
                let raw = native.snapshot_v1();
                let mut expected = None;
                for _ in 0..2 {
                    own.restore(&ready).unwrap();
                    native.kaito_restore_v75(&raw).unwrap();
                    let attackers = match name {
                        "Armasaur Guide" => hawks[..(2 + usize::from(enabled))].to_vec(),
                        // One trigger for a declaration of either one or three creatures.
                        "Battlesong Berserker" => hawks[..if enabled { 3 } else { 1 }].to_vec(),
                        _ => vec![source],
                    };
                    declare(&mut own, &mut native, &attackers);
                    let fires = enabled || name == "Battlesong Berserker";
                    if fires && matches!(name, "Armasaur Guide" | "Battlesong Berserker") {
                        assert!(!own
                            .fixture_state()
                            .engine
                            .combat
                            .attackers
                            .contains(&source));
                        wait_target(&mut own, &mut native, source, hawks[0]);
                        checkpoint(&mut own, &mut native);
                        choose(&mut own, &mut native, source, hawks[0]);
                    }
                    assert_eq!(
                        own.fixture_state().stack.len(),
                        usize::from(fires),
                        "{name} enabled={enabled}"
                    );
                    checkpoint(&mut own, &mut native);
                    settle(&mut own, &mut native);
                    if name == "Armasaur Guide" {
                        assert_eq!(
                            own.fixture_state()
                                .objects
                                .get(hawks[0])
                                .counters
                                .plus1_plus1,
                            i32::from(enabled)
                        );
                    } else if matches!(
                        name,
                        "Crypt Feaster" | "Courageous Goblin" | "Ruby, Daring Tracker"
                    ) {
                        let base = if name == "Crypt Feaster" {
                            (3, 4)
                        } else if name == "Ruby, Daring Tracker" {
                            (1, 2)
                        } else {
                            (2, 2)
                        };
                        let boost = if matches!(name, "Crypt Feaster" | "Ruby, Daring Tracker") {
                            2
                        } else {
                            1
                        };
                        assert_eq!(
                            crate::engine::checked_effective_pt(own.fixture_state(), source)
                                .unwrap(),
                            (
                                base.0 + boost * i32::from(enabled),
                                base.1
                                    + if name == "Ruby, Daring Tracker" {
                                        2 * i32::from(enabled)
                                    } else {
                                        0
                                    }
                            )
                        );
                    } else {
                        assert_eq!(
                            crate::engine::checked_effective_pt(own.fixture_state(), hawks[0])
                                .unwrap(),
                            (2, 1)
                        );
                        assert!(crate::engine::has_effective_keyword(
                            own.fixture_state(),
                            hawks[0],
                            Keywords::MENACE
                        ));
                    }
                    if let Some(ref previous) = expected {
                        assert_eq!(own.fixture_state(), previous);
                    } else {
                        expected = Some(own.fixture_state().clone());
                    }
                }
            }
        }
    }
}
