use super::library_search::finish_zero;
use super::targeted_entry::{actions, choose, wait_target};
use super::*;
use crate::{
    ids::ObjectId,
    policy_observation_v6::tests::put,
    state::{GameState, Step, Zone},
};
fn activation(a: &A, source: ObjectId) -> bool {
    matches!(a,A::ActivateAbility{source:s,ability_index:0,..}if s.arena_id==source.0)
}
fn quiet(a: &A) -> bool {
    matches!(
        a,
        A::Pass { .. } | A::OrderTriggers { .. } | A::ChooseTriggerOrderNext { .. }
    )
}
fn scene(p: PlayerId, concealed: &str) -> (GameState, ObjectId, ObjectId, ObjectId, ObjectId) {
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    let mut s = GameState::new_from_libraries_with_starting_player_v1(
        &[forest; 40],
        &[forest; 40],
        crate::rl::card_name,
        110_385,
        p,
    );
    s.step = Step::Main1;
    let source = put(&mut s, p, "Evolving Wilds", Zone::Hand);
    let extra = put(&mut s, p, "Island", Zone::Hand);
    put(&mut s, p, "Evolving Wilds", Zone::Library);
    put(&mut s, p, "Lightning Bolt", Zone::Library);
    put(&mut s, p.opponent(), "Forest", Zone::Battlefield);
    let ooze = put(&mut s, p.opponent(), "Scavenging Ooze", Zone::Battlefield);
    put(&mut s, p.opponent(), concealed, Zone::Hand);
    let land = s.players[p.index()].library[0];
    (s, source, extra, ooze, land)
}
fn owner(s: GameState) -> (KaitoSession, FastActorSessionV1) {
    let native = FastActorSessionV1::from_kaito_v75_fixture_state(s.clone());
    (
        KaitoSession::from_session(FastActorSessionV1::from_kaito_v75_fixture_state(s)).unwrap(),
        native,
    )
}
fn play(own: &mut KaitoSession, native: &mut FastActorSessionV1, source: ObjectId, p: PlayerId) {
    assert_eq!(
        own.fixture_state().players[p.index()].lands_played_this_turn,
        0
    );
    act(
        own,
        native,
        |a| matches!(a,A::PlayLand{source:s,..}if s.arena_id==source.0),
    );
    let s = own.fixture_state();
    assert_eq!(s.objects.get(source).zone, Zone::Battlefield);
    assert!(!s.objects.get(source).tapped);
    assert!(s.stack.is_empty());
    assert_eq!(s.players[p.index()].lands_played_this_turn, 1);
    assert_eq!(s.players[p.index()].mana_pool, [0; 6]);
    let menu = actions(own);
    assert!(menu
        .iter()
        .any(|a| matches!(a,A::Pass{actor}if *actor==crate::rl::PlayerSeatV1::from(p))));
    assert!(!menu.iter().any(|a| matches!(a, A::PlayLand { .. })));
    assert!(!menu.iter().any(|a|matches!(a,A::CastSpell{source:s,..}|A::ActivateManaAbility{source:s,..}if s.arena_id==source.0)));
}

#[test]
fn paid_evolving_land_play_quota_tap_sacrifice_private_basic_search_source_exile_and_restore_both_seats(
) {
    let id = crate::card_def::card_id_by_name("Evolving Wilds").unwrap();
    assert_eq!(
        super::super::super::roots::registered_permanent_printing(id).unwrap(),
        Some(super::super::super::roots::baseline(id).unwrap())
    );
    for p in [PlayerId::P0, PlayerId::P1] {
        let (s, source, extra, ooze, land) = scene(p, "Island");
        let (mut own, mut native) = owner(s);
        play(&mut own, &mut native, source, p);
        let played = own.snapshot();
        let raw = native.snapshot_v1();
        for branch in 0..3 {
            let mut expected = None;
            for _ in 0..2 {
                own.restore(&played).unwrap();
                native.kaito_restore_v75(&raw).unwrap();
                let library = own.fixture_state().players[p.index()].library.clone();
                let hand = own.fixture_state().players[p.index()].hand.clone();
                let rng = own.fixture_state().legacy_rng().copied();
                let generation = own.fixture_state().objects.get(source).zone_change_count;
                act(&mut own, &mut native, |a| activation(a, source));
                assert_eq!(
                    own.fixture_state().objects.get(source).zone,
                    Zone::Graveyard
                );
                assert_eq!(
                    own.fixture_state().objects.get(source).zone_change_count,
                    generation + 1
                );
                assert_eq!(own.fixture_state().players[p.index()].library, library);
                assert!(!own.fixture_state().stack.is_empty());
                assert!(!actions(&own).iter().any(|a| activation(a, source)));
                let pending = own.snapshot();
                let pending_raw = native.snapshot_v1();
                own.restore(&pending).unwrap();
                native.kaito_restore_v75(&pending_raw).unwrap();
                if branch == 2 {
                    if !actions(&own).iter().any(|a| activation(a, ooze)) {
                        act(&mut own, &mut native, |a| matches!(a, A::Pass { .. }));
                    }
                    act(&mut own, &mut native, |a| activation(a, ooze));
                    choose(&mut own, &mut native, ooze, source);
                    for _ in 0..128 {
                        if own.fixture_state().objects.get(source).zone == Zone::Exile {
                            break;
                        }
                        act(&mut own, &mut native, quiet);
                    }
                    assert_eq!(own.fixture_state().objects.get(source).zone, Zone::Exile);
                }
                wait_target(&mut own, &mut native, source, land);
                let menu = actions(&own);
                let mut actual = menu
                    .iter()
                    .filter_map(|a| match a {
                        A::ChooseEffectTarget {
                            source: s,
                            target: crate::rl::TargetRefV1::Object { object },
                            ..
                        } if s.arena_id == source.0 => Some(ObjectId(object.arena_id)),
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                actual.sort_unstable();
                let forest = crate::card_def::card_id_by_name("Forest").unwrap();
                let mut basics = library
                    .iter()
                    .copied()
                    .filter(|&o| own.fixture_state().objects.get(o).card_def == forest)
                    .collect::<Vec<_>>();
                basics.sort_unstable();
                assert_eq!(actual, basics);
                let choosing = own.snapshot();
                let choosing_raw = native.snapshot_v1();
                own.restore(&choosing).unwrap();
                native.kaito_restore_v75(&choosing_raw).unwrap();
                if branch == 0 {
                    finish_zero(&mut own, &mut native, source);
                } else {
                    choose(&mut own, &mut native, source, land);
                }
                settle(&mut own, &mut native);
                let s = own.fixture_state();
                let found = branch != 0;
                assert_eq!(
                    s.objects.get(source).zone,
                    if branch == 2 {
                        Zone::Exile
                    } else {
                        Zone::Graveyard
                    }
                );
                assert_eq!(
                    s.objects.get(source).zone_change_count,
                    generation + if branch == 2 { 2 } else { 1 }
                );
                assert_eq!(
                    s.objects.get(land).zone,
                    if found {
                        Zone::Battlefield
                    } else {
                        Zone::Library
                    }
                );
                if found {
                    assert!(s.objects.get(land).tapped);
                    assert_eq!(s.objects.get(land).controller, p);
                }
                assert_eq!(
                    s.players[p.index()].library.len(),
                    library.len() - usize::from(found)
                );
                assert_ne!(s.legacy_rng().copied(), rng);
                assert_eq!(s.players[p.index()].hand, hand);
                assert!(s.players[p.index()].hand.contains(&extra));
                assert_eq!(
                    s.players[p.index()].lands_played_this_turn,
                    1,
                    "fetching a land is not another land play"
                );
                assert_eq!(s.players[p.index()].mana_pool, [0; 6]);
                assert_eq!(s.players[p.index()].life, 20);
                assert_eq!(s.players[p.opponent().index()].life, 20);
                assert_eq!(
                    s.objects.get(ooze).counters.plus1_plus1,
                    0,
                    "exiling a land gives Ooze no creature bonus"
                );
                if let Some(ref prior) = expected {
                    assert_eq!(s, prior);
                } else {
                    expected = Some(s.clone());
                }
            }
        }
    }
}

#[test]
fn independently_paid_evolving_private_library_choices_ignore_other_hidden_hands_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut packets = vec![];
        for concealed in ["Island", "Swamp"] {
            let (s, source, _, _, land) = scene(p, concealed);
            let (mut own, mut native) = owner(s);
            play(&mut own, &mut native, source, p);
            act(&mut own, &mut native, |a| activation(a, source));
            wait_target(&mut own, &mut native, source, land);
            let FastActorResponseV1::Decision(h) = own.current_response() else {
                panic!("genuine private search absent")
            };
            let d = own.encode(h).unwrap();
            assert!(NativeModel::deterministic_fixture()
                .unwrap()
                .forward(d.packet())
                .unwrap()
                .iter()
                .all(|v| v.is_finite()));
            packets.push(d.packet().clone());
            choose(&mut own, &mut native, source, land);
            settle(&mut own, &mut native);
            assert_eq!(
                own.fixture_state().objects.get(land).zone,
                Zone::Battlefield
            );
            assert!(own.fixture_state().objects.get(land).tapped);
        }
        assert_eq!(packets[0], packets[1]);
    }
}
