use super::*;
use crate::{
    card_def::Keywords,
    ids::ObjectId,
    policy_observation_v6::tests::put,
    state::{GameState, Step, Zone},
};

#[test]
fn paid_blanchwood_own_or_opposing_host_controller_forests_responses_and_restore_both_seats() {
    paid_aura("Blanchwood Armor");
}

#[test]
fn paid_twinblade_flash_own_or_opposing_host_double_strike_responses_and_restore_both_seats() {
    paid_aura("Twinblade Blessing");
}

fn target(own: &mut KaitoSession, native: &mut FastActorSessionV1, id: ObjectId) {
    act(
        own,
        native,
        |a| matches!(a, A::ChooseTarget { target: crate::rl::TargetRefV1::Object { object }, .. } if object.arena_id == id.0),
    );
}

fn check(state: &GameState, host: ObjectId, armor: bool, forests: i32, attached: bool) {
    let boost = if attached && armor { forests } else { 0 };
    assert_eq!(
        crate::engine::checked_effective_pt(state, host).unwrap(),
        (2 + boost, 2 + boost)
    );
    assert_eq!(
        crate::engine::has_effective_keyword(state, host, Keywords::DOUBLE_STRIKE),
        attached && !armor
    );
    assert_eq!(state.objects.get(host).counters.plus1_plus1, 0);
    assert!(crate::engine::has_effective_keyword(
        state,
        host,
        Keywords::VIGILANCE
    ));
}

fn paid_aura(name: &str) {
    let armor = name == "Blanchwood Armor";
    let id = crate::card_def::card_id_by_name(name).unwrap();
    let meaning = super::super::super::roots::baseline(id).unwrap();
    let root = &meaning["ordinary_aura_printing"];
    assert_eq!(
        root["ordered_cast_roots"][0]["target_filter"]["controller"],
        "EitherPlayer"
    );
    if armor {
        assert_eq!(
            root["ordered_static_roots"][0]["multiplier"]["controller"],
            "AuraController"
        );
    }
    assert_eq!(
        super::super::super::roots::registered_permanent_printing(id).unwrap(),
        Some(meaning)
    );
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    for p in [PlayerId::P0, PlayerId::P1] {
        for host_owner in [p, p.opponent()] {
            for offturn in [false, true] {
                if armor && offturn {
                    continue;
                }
                let active = if offturn { p.opponent() } else { p };
                let mut s = GameState::new_from_libraries_with_starting_player_v1(
                    &[forest; 40],
                    &[forest; 40],
                    crate::rl::card_name,
                    110_300,
                    active,
                );
                s.step = Step::Main1;
                for _ in 0..3 {
                    put(&mut s, p, "Forest", Zone::Battlefield);
                }
                for _ in 0..2 {
                    put(&mut s, p, "Plains", Zone::Battlefield);
                }
                for _ in 0..2 {
                    put(&mut s, p.opponent(), "Forest", Zone::Battlefield);
                }
                for _ in 0..3 {
                    put(&mut s, p.opponent(), "Plains", Zone::Battlefield);
                }
                put(&mut s, p.opponent(), "Swamp", Zone::Battlefield);
                let own_land = put(&mut s, p, "Forest", Zone::Hand);
                let other_land = put(&mut s, p.opponent(), "Forest", Zone::Hand);
                let aura = put(&mut s, p, name, Zone::Hand);
                let host = put(&mut s, host_owner, "Prideful Parent", Zone::Battlefield);
                let stab = put(&mut s, p.opponent(), "Stab", Zone::Hand);
                let wings = put(&mut s, p.opponent(), "Broken Wings", Zone::Hand);
                let mut native = FastActorSessionV1::from_kaito_v75_fixture_state(s.clone());
                let mut own =
                    KaitoSession::from_session(FastActorSessionV1::from_kaito_v75_fixture_state(s))
                        .unwrap();
                if offturn {
                    act(&mut own, &mut native, |a| matches!(a, A::Pass { .. }));
                }
                act(
                    &mut own,
                    &mut native,
                    |a| matches!(a, A::CastSpell { source, .. } if source.arena_id == aura.0),
                );
                target(&mut own, &mut native, host);
                assert_eq!(own.fixture_state().objects.get(aura).zone, Zone::Stack);
                assert_eq!(own.fixture_state().players[p.index()].mana_pool, [0; 6]);
                assert_eq!(
                    own.fixture_state().players[p.index()]
                        .battlefield
                        .iter()
                        .filter(|&&card| own.fixture_state().objects.get(card).tapped)
                        .count(),
                    3
                );
                let saved = own.snapshot();
                let raw = native.snapshot_v1();
                for remove_target in [false, true] {
                    let mut expected = None;
                    for _ in 0..2 {
                        own.restore(&saved).unwrap();
                        native.kaito_restore_v75(&raw).unwrap();
                        if remove_target {
                            if own.fixture_state().priority_player != p.opponent() {
                                act(&mut own, &mut native, |a| matches!(a, A::Pass { .. }));
                            }
                            act(
                                &mut own,
                                &mut native,
                                |a| matches!(a, A::CastSpell { source, .. } if source.arena_id == stab.0),
                            );
                            target(&mut own, &mut native, host);
                        }
                        settle(&mut own, &mut native);
                        if remove_target {
                            assert_eq!(own.fixture_state().objects.get(aura).zone, Zone::Graveyard);
                            assert_eq!(own.fixture_state().objects.get(host).zone, Zone::Graveyard);
                            assert!(own
                                .fixture_state()
                                .objects
                                .get(aura)
                                .v4
                                .attached_to
                                .is_none());
                        } else {
                            assert_eq!(
                                own.fixture_state().objects.get(aura).zone,
                                Zone::Battlefield
                            );
                            assert_eq!(own.fixture_state().objects.get(aura).controller, p);
                            assert_eq!(
                                own.fixture_state()
                                    .objects
                                    .get(aura)
                                    .v4
                                    .attached_to
                                    .unwrap()
                                    .object,
                                host
                            );
                            check(own.fixture_state(), host, armor, 3, true);
                            // A genuine land play changes the active player's Forest
                            // count. Opposing host control never selects the multiplier.
                            let land = if active == p { own_land } else { other_land };
                            act(
                                &mut own,
                                &mut native,
                                |a| matches!(a, A::PlayLand { source, .. } if source.arena_id == land.0),
                            );
                            check(
                                own.fixture_state(),
                                host,
                                armor,
                                if active == p { 4 } else { 3 },
                                true,
                            );
                            if own.fixture_state().priority_player != p.opponent() {
                                act(&mut own, &mut native, |a| matches!(a, A::Pass { .. }));
                            }
                            act(
                                &mut own,
                                &mut native,
                                |a| matches!(a, A::CastSpell { source, .. } if source.arena_id == wings.0),
                            );
                            target(&mut own, &mut native, aura);
                            settle(&mut own, &mut native);
                            assert_eq!(own.fixture_state().objects.get(aura).zone, Zone::Graveyard);
                            check(own.fixture_state(), host, armor, 0, false);
                        }
                        own.fixture_state().validate_attachment_relations().unwrap();
                        assert_eq!(own.fixture_state().players[p.index()].mana_pool, [0; 6]);
                        assert_eq!(
                            own.fixture_state().players[p.index()]
                                .battlefield
                                .iter()
                                .filter(|&&card| own.fixture_state().objects.get(card).tapped)
                                .count(),
                            3
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
    }
}
