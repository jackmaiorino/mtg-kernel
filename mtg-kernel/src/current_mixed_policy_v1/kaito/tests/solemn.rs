use super::library_search::{finish_zero, optional, wait_entered};
use super::targeted_entry::{actions, cast, choose, wait_target};
use super::*;
use crate::{
    ids::ObjectId,
    policy_observation_v6::tests::put,
    state::{GameState, Step, Zone},
};
fn quiet(a: &A) -> bool {
    matches!(
        a,
        A::Pass { .. } | A::OrderTriggers { .. } | A::ChooseTriggerOrderNext { .. }
    )
}
fn scene(p: PlayerId) -> GameState {
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    let mut s = GameState::new_from_libraries_with_starting_player_v1(
        &[forest; 40],
        &[forest; 40],
        crate::rl::card_name,
        110_384,
        p,
    );
    s.step = Step::Main1;
    for _ in 0..4 {
        put(&mut s, p, "Swamp", Zone::Battlefield);
    }
    for land in ["Island", "Swamp"] {
        for _ in 0..3 {
            put(&mut s, p.opponent(), land, Zone::Battlefield);
        }
    }
    put(&mut s, p.opponent(), "Forest", Zone::Battlefield);
    s
}
fn owner(s: GameState) -> (KaitoSession, FastActorSessionV1) {
    let native = FastActorSessionV1::from_kaito_v75_fixture_state(s.clone());
    (
        KaitoSession::from_session(FastActorSessionV1::from_kaito_v75_fixture_state(s)).unwrap(),
        native,
    )
}
fn removal(
    own: &mut KaitoSession,
    native: &mut FastActorSessionV1,
    spell: ObjectId,
    source: ObjectId,
) {
    cast(own, native, spell);
    act(
        own,
        native,
        |a| matches!(a,A::ChooseTarget{target:crate::rl::TargetRefV1::Object{object},..}if object.arena_id==source.0),
    );
}

#[test]
fn paid_solemn_optional_entry_decline_failed_find_tapped_basic_source_departure_counter_and_restore_both_seats(
) {
    let id = crate::card_def::card_id_by_name("Solemn Simulacrum").unwrap();
    assert_eq!(
        super::super::super::roots::registered_permanent_printing(id).unwrap(),
        Some(super::super::super::roots::baseline(id).unwrap())
    );
    for p in [PlayerId::P0, PlayerId::P1] {
        let q = p.opponent();
        let mut s = scene(p);
        let source = put(&mut s, p, "Solemn Simulacrum", Zone::Hand);
        let land = s.players[p.index()].library[0];
        let nonbasic = put(&mut s, p, "Evolving Wilds", Zone::Library);
        let counter = put(&mut s, q, "Counterspell", Zone::Hand);
        let downfall = put(&mut s, q, "Hero's Downfall", Zone::Hand);
        let (mut own, mut native) = owner(s);
        cast(&mut own, &mut native, source);
        let paid = own.snapshot();
        let raw = native.snapshot_v1();
        for branch in 0..5 {
            let mut expected = None;
            for _ in 0..2 {
                own.restore(&paid).unwrap();
                native.kaito_restore_v75(&raw).unwrap();
                let library = own.fixture_state().players[p.index()].library.clone();
                let hand = own.fixture_state().players[p.index()].hand.clone();
                let rng = own.fixture_state().legacy_rng().copied();
                let knowledge = own.fixture_state().library_knowledge.clone();
                if branch == 0 {
                    cast(&mut own, &mut native, counter);
                    act(&mut own, &mut native, |a| {
                        is_original_spell_target(a, source)
                    });
                } else {
                    if branch == 4 {
                        wait_entered(&mut own, &mut native, source);
                        removal(&mut own, &mut native, downfall, source);
                        // The later death trigger resolves before the entry trigger.
                        // Decline the draw, then still accept the original search.
                        optional(&mut own, &mut native, source, false);
                    }
                    optional(&mut own, &mut native, source, branch != 1);
                    if branch != 1 {
                        wait_target(&mut own, &mut native, source, land);
                        let menu = actions(&own);
                        assert!(!menu.iter().any(|a|matches!(a,A::ChooseEffectTarget{target:crate::rl::TargetRefV1::Object{object},..}if object.arena_id==nonbasic.0)));
                        let choosing = own.snapshot();
                        let choosing_raw = native.snapshot_v1();
                        own.restore(&choosing).unwrap();
                        native.kaito_restore_v75(&choosing_raw).unwrap();
                        if branch == 3 {
                            finish_zero(&mut own, &mut native, source);
                        } else {
                            choose(&mut own, &mut native, source, land);
                        }
                    }
                }
                settle(&mut own, &mut native);
                let state = own.fixture_state();
                let found = branch == 2 || branch == 4;
                assert_eq!(
                    state.objects.get(source).zone,
                    if branch == 0 || branch == 4 {
                        Zone::Graveyard
                    } else {
                        Zone::Battlefield
                    }
                );
                assert_eq!(
                    state.objects.get(land).zone,
                    if found {
                        Zone::Battlefield
                    } else {
                        Zone::Library
                    }
                );
                assert_eq!(state.players[p.index()].hand, hand);
                assert_eq!(
                    state.players[p.index()].library.len(),
                    library.len() - usize::from(found)
                );
                assert_eq!(state.players[p.index()].mana_pool, [0; 6]);
                let taps = state.players[p.index()]
                    .battlefield
                    .iter()
                    .filter(|&&o| state.objects.get(o).tapped)
                    .count();
                assert_eq!(taps, 4 + usize::from(found));
                if found {
                    assert!(state.objects.get(land).tapped);
                    assert_eq!(state.objects.get(land).controller, p);
                }
                if branch <= 1 {
                    assert_eq!(state.players[p.index()].library, library);
                    assert_eq!(state.legacy_rng().copied(), rng);
                    assert_eq!(state.library_knowledge, knowledge);
                } else {
                    assert_ne!(state.legacy_rng().copied(), rng);
                }
                assert_eq!(state.players[p.index()].life, 20);
                assert_eq!(state.players[q.index()].life, 20);
                if let Some(ref prior) = expected {
                    assert_eq!(state, prior);
                } else {
                    expected = Some(state.clone());
                }
            }
        }
    }
}

#[test]
fn paid_solemn_death_optional_draw_survives_real_graveyard_exile_and_restores_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let q = p.opponent();
        let mut s = scene(p);
        let source = put(&mut s, p, "Solemn Simulacrum", Zone::Hand);
        let stab = put(&mut s, q, "Stab", Zone::Hand);
        let ooze = put(&mut s, q, "Scavenging Ooze", Zone::Battlefield);
        let (mut own, mut native) = owner(s);
        cast(&mut own, &mut native, source);
        optional(&mut own, &mut native, source, false);
        settle(&mut own, &mut native);
        let ready = own.snapshot();
        let ready_raw = native.snapshot_v1();
        for branch in 0..3 {
            let mut expected = None;
            for _ in 0..2 {
                own.restore(&ready).unwrap();
                native.kaito_restore_v75(&ready_raw).unwrap();
                let library = own.fixture_state().players[p.index()].library.clone();
                let hand = own.fixture_state().players[p.index()].hand.len();
                removal(&mut own, &mut native, stab, source);
                for _ in 0..128 {
                    if own.fixture_state().objects.get(source).zone == Zone::Graveyard {
                        break;
                    }
                    act(&mut own, &mut native, quiet);
                }
                assert_eq!(
                    own.fixture_state().objects.get(source).zone,
                    Zone::Graveyard
                );
                assert!(!own.fixture_state().stack.is_empty());
                if branch == 2 {
                    if !actions(&own).iter().any(|a|matches!(a,A::ActivateAbility{source:s,ability_index:0,..}if s.arena_id==ooze.0)){act(&mut own,&mut native,|a|matches!(a,A::Pass{..}));}
                    act(
                        &mut own,
                        &mut native,
                        |a| matches!(a,A::ActivateAbility{source:s,ability_index:0,..}if s.arena_id==ooze.0),
                    );
                    choose(&mut own, &mut native, ooze, source);
                    for _ in 0..128 {
                        if own.fixture_state().objects.get(source).zone == Zone::Exile {
                            break;
                        }
                        act(&mut own, &mut native, quiet);
                    }
                    assert_eq!(own.fixture_state().objects.get(source).zone, Zone::Exile);
                }
                optional(&mut own, &mut native, source, branch != 0);
                settle(&mut own, &mut native);
                let state = own.fixture_state();
                let draws = usize::from(branch != 0);
                assert_eq!(
                    state.players[p.index()].library.len(),
                    library.len() - draws
                );
                assert_eq!(state.players[p.index()].hand.len(), hand + draws);
                if draws == 1 {
                    assert!(state.players[p.index()].hand.contains(&library[0]));
                }
                assert_eq!(
                    state.objects.get(source).zone,
                    if branch == 2 {
                        Zone::Exile
                    } else {
                        Zone::Graveyard
                    }
                );
                assert_eq!(state.players[p.index()].life, 20);
                assert_eq!(
                    state.players[q.index()].life,
                    if branch == 2 { 21 } else { 20 }
                );
                assert_eq!(
                    state.objects.get(ooze).counters.plus1_plus1,
                    if branch == 2 { 1 } else { 0 }
                );
                if let Some(ref prior) = expected {
                    assert_eq!(state, prior);
                } else {
                    expected = Some(state.clone());
                }
            }
        }
    }
}
