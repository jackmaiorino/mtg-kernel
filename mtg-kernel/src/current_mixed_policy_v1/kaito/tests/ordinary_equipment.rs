use super::*;
use crate::{
    card_def::Keywords,
    ids::ObjectId,
    policy_observation_v6::tests::put,
    state::{GameState, Step, Zone},
};

#[test]
fn paid_boots_equip_retarget_response_turn_grants_and_restore_both_seats() {
    paid_equipment("Swiftfoot Boots");
}

#[test]
fn paid_katana_equip_retarget_response_turn_grants_and_restore_both_seats() {
    paid_equipment("Quick-Draw Katana");
}

fn choose_object(own: &mut KaitoSession, native: &mut FastActorSessionV1, id: ObjectId) {
    act(own, native, |a| {
        matches!(a, A::ChooseTarget { target: crate::rl::TargetRefV1::Object { object }, .. }
            if object.arena_id == id.0)
    });
}

pub(super) fn reach_main(
    own: &mut KaitoSession,
    native: &mut FastActorSessionV1,
    player: PlayerId,
) {
    for _ in 0..256 {
        let s = own.fixture_state();
        if s.active_player == player && s.step == Step::Main1 && s.stack.is_empty() {
            return;
        }
        act(own, native, |a| {
            matches!(
                a,
                A::Pass { .. }
                    | A::ChooseAttackerInclusion { include: false, .. }
                    | A::ChooseBlockerInclusion { include: false, .. }
                    | A::Discard { .. }
            )
        });
    }
    panic!("natural quiet turn did not reach {player:?} main phase");
}

fn check_grants(s: &GameState, host: ObjectId, katana: bool, active: bool, attached: bool) {
    assert_eq!(
        crate::engine::checked_effective_pt(s, host).unwrap(),
        (2 + if attached && katana && active { 2 } else { 0 }, 2)
    );
    for (keyword, expected) in [
        (Keywords::HASTE, attached && !katana),
        (Keywords::HEXPROOF, attached && !katana),
        (Keywords::FIRST_STRIKE, attached && katana && active),
        (Keywords::VIGILANCE, true),
    ] {
        assert_eq!(
            crate::engine::has_effective_keyword(s, host, keyword),
            expected
        );
    }
}

fn paid_equipment(name: &str) {
    let katana = name == "Quick-Draw Katana";
    let equip_cost = if katana { 2 } else { 1 };
    let id = crate::card_def::card_id_by_name(name).unwrap();
    let meaning = super::super::super::roots::baseline(id).unwrap();
    let root = &meaning["ordinary_equipment_printing"];
    assert_eq!(
        root["ordered_static_roots"][0]["controller_turn_basis"],
        "EquipmentController"
    );
    assert_eq!(
        root["ordered_activation_roots"][0]["previous_attachment_unchanged_if_target_illegal"],
        true
    );
    assert_eq!(
        super::super::super::roots::registered_permanent_printing(id).unwrap(),
        Some(meaning)
    );
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = GameState::new_from_libraries_with_starting_player_v1(
            &[forest; 40],
            &[forest; 40],
            crate::rl::card_name,
            110_298,
            p,
        );
        s.step = Step::Main1;
        for _ in 0..6 {
            put(&mut s, p, "Forest", Zone::Battlefield);
        }
        for _ in 0..3 {
            put(&mut s, p.opponent(), "Forest", Zone::Battlefield);
        }
        put(&mut s, p.opponent(), "Swamp", Zone::Battlefield);
        let equipment = put(&mut s, p, name, Zone::Hand);
        // Registered 2/2 supports have vigilance, separate from these grants.
        let first = put(&mut s, p, "Prideful Parent", Zone::Battlefield);
        let second = put(&mut s, p, "Prideful Parent", Zone::Battlefield);
        let enemy = put(&mut s, p.opponent(), "Prideful Parent", Zone::Battlefield);
        let stab = put(&mut s, p.opponent(), "Stab", Zone::Hand);
        let wings = put(&mut s, p.opponent(), "Broken Wings", Zone::Hand);
        let mut native = FastActorSessionV1::from_kaito_v75_fixture_state(s.clone());
        let mut own =
            KaitoSession::from_session(FastActorSessionV1::from_kaito_v75_fixture_state(s))
                .unwrap();
        act(
            &mut own,
            &mut native,
            |a| matches!(a, A::CastSpell { source, .. } if source.arena_id == equipment.0),
        );
        assert_eq!(own.fixture_state().objects.get(equipment).zone, Zone::Stack);
        assert_eq!(
            own.fixture_state().players[p.index()]
                .battlefield
                .iter()
                .filter(|&&card| own.fixture_state().objects.get(card).tapped)
                .count(),
            2
        );
        settle(&mut own, &mut native);
        assert_eq!(
            own.fixture_state().objects.get(equipment).zone,
            Zone::Battlefield
        );
        assert!(own
            .fixture_state()
            .objects
            .get(equipment)
            .v4
            .attached_to
            .is_none());
        act(
            &mut own,
            &mut native,
            |a| matches!(a, A::ActivateAbility { source, ability_index: 0, .. } if source.arena_id == equipment.0),
        );
        let FastActorResponseV1::Decision(h) = own.current_response() else {
            panic!("equip targets missing")
        };
        let d = own.encode(h).unwrap();
        assert!(!d.backend.actions().iter().any(|a| matches!(a, A::ChooseTarget { target: crate::rl::TargetRefV1::Object { object }, .. } if object.arena_id == enemy.0 || object.arena_id == equipment.0)));
        choose_object(&mut own, &mut native, first);
        assert_eq!(
            own.fixture_state().players[p.index()]
                .battlefield
                .iter()
                .filter(|&&card| own.fixture_state().objects.get(card).tapped)
                .count(),
            2 + equip_cost
        );
        let saved = own.snapshot();
        let raw = native.snapshot_v1();
        let mut expected = None;
        for _ in 0..2 {
            own.restore(&saved).unwrap();
            native.kaito_restore_v75(&raw).unwrap();
            settle(&mut own, &mut native);
            assert_eq!(
                own.fixture_state()
                    .objects
                    .get(equipment)
                    .v4
                    .attached_to
                    .unwrap()
                    .object,
                first
            );
            check_grants(own.fixture_state(), first, katana, true, true);
            check_grants(own.fixture_state(), second, katana, true, false);
            reach_main(&mut own, &mut native, p.opponent());
            check_grants(own.fixture_state(), first, katana, false, true);
            reach_main(&mut own, &mut native, p);
            check_grants(own.fixture_state(), first, katana, true, true);
            if let Some(ref state) = expected {
                assert_eq!(own.fixture_state(), state);
            } else {
                expected = Some(own.fixture_state().clone());
            }
        }
        act(
            &mut own,
            &mut native,
            |a| matches!(a, A::ActivateAbility { source, ability_index: 0, .. } if source.arena_id == equipment.0),
        );
        choose_object(&mut own, &mut native, second);
        let saved = own.snapshot();
        let raw = native.snapshot_v1();
        for response in [0, 1, 2] {
            let mut expected = None;
            for _ in 0..2 {
                own.restore(&saved).unwrap();
                native.kaito_restore_v75(&raw).unwrap();
                let tapped = own.fixture_state().players[p.index()]
                    .battlefield
                    .iter()
                    .filter(|&&card| own.fixture_state().objects.get(card).tapped)
                    .count();
                assert_eq!(tapped, equip_cost);
                if response != 0 {
                    act(&mut own, &mut native, |a| matches!(a, A::Pass { .. }));
                    let removal = if response == 1 { stab } else { wings };
                    act(
                        &mut own,
                        &mut native,
                        |a| matches!(a, A::CastSpell { source, .. } if source.arena_id == removal.0),
                    );
                    if response == 1 && !katana {
                        let FastActorResponseV1::Decision(h) = own.current_response() else {
                            panic!("Stab targets missing")
                        };
                        let d = own.encode(h).unwrap();
                        assert!(!d.backend.actions().iter().any(|a| matches!(a, A::ChooseTarget { target: crate::rl::TargetRefV1::Object { object }, .. } if object.arena_id == first.0)));
                    }
                    choose_object(
                        &mut own,
                        &mut native,
                        if response == 1 { second } else { equipment },
                    );
                }
                settle(&mut own, &mut native);
                let state = own.fixture_state();
                state.validate_attachment_relations().unwrap();
                assert_eq!(state.players[p.index()].mana_pool, [0; 6]);
                assert_eq!(
                    state.players[p.index()]
                        .battlefield
                        .iter()
                        .filter(|&&card| state.objects.get(card).tapped)
                        .count(),
                    tapped
                );
                assert_eq!(
                    state.objects.get(equipment).zone,
                    if response == 2 {
                        Zone::Graveyard
                    } else {
                        Zone::Battlefield
                    }
                );
                assert_eq!(
                    state.objects.get(second).zone,
                    if response == 1 {
                        Zone::Graveyard
                    } else {
                        Zone::Battlefield
                    }
                );
                if response == 2 {
                    assert!(state.objects.get(equipment).v4.attached_to.is_none());
                } else {
                    assert_eq!(
                        state.objects.get(equipment).v4.attached_to.unwrap().object,
                        if response == 1 { first } else { second }
                    );
                }
                check_grants(state, first, katana, true, response == 1);
                if response != 1 {
                    check_grants(state, second, katana, true, response == 0);
                }
                if let Some(ref previous) = expected {
                    assert_eq!(state, previous);
                } else {
                    expected = Some(state.clone());
                }
            }
        }
    }
}
