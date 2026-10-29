use super::*;
use super::{
    searslicer::through_combat,
    surveil_many::{checkpoint, sessions},
    targeted_entry::{actions, cast, choose},
};
use crate::{
    card_def::Keywords,
    ids::ObjectId,
    policy_observation_v6::tests::put,
    state::{GameState, Zone},
};
fn scene(
    p: PlayerId,
) -> (
    KaitoSession,
    FastActorSessionV1,
    ObjectId,
    ObjectId,
    ObjectId,
    ObjectId,
) {
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    let mut s = GameState::new_from_libraries_with_starting_player_v1(
        &[forest; 40],
        &[forest; 40],
        crate::rl::card_name,
        110_399,
        p,
    );
    s.step = crate::state::Step::Main1;
    for _ in 0..4 {
        put(&mut s, p, "Forest", Zone::Battlefield);
        put(&mut s, p.opponent(), "Island", Zone::Battlefield);
    }
    for _ in 0..3 {
        put(&mut s, p, "Island", Zone::Battlefield);
        put(&mut s, p.opponent(), "Swamp", Zone::Battlefield);
    }
    let source = put(&mut s, p, "Koma, World-Eater", Zone::Hand);
    let cavalry = put(&mut s, p, "Axgard Cavalry", Zone::Battlefield);
    let counter = put(&mut s, p.opponent(), "Counterspell", Zone::Hand);
    let removal = put(&mut s, p.opponent(), "Hero's Downfall", Zone::Hand);
    let (mut own, mut native) = sessions(s);
    cast(&mut own, &mut native, source);
    assert_eq!(own.fixture_state().objects.get(source).zone, Zone::Stack);
    assert_eq!(
        crate::engine::object_mana_value(own.fixture_state(), source),
        7
    );
    assert_eq!(tapped(&own, p), 7);
    assert_eq!(own.fixture_state().players[p.index()].mana_pool, [0; 6]);
    (own, native, source, cavalry, counter, removal)
}
fn tapped(own: &KaitoSession, p: PlayerId) -> usize {
    own.fixture_state().players[p.index()]
        .battlefield
        .iter()
        .filter(|&&o| own.fixture_state().objects.get(o).tapped)
        .count()
}
#[test]
fn paid_koma_actual_counter_attempt_seven_mana_ward_four_pay_or_decline_and_restore_both_seats() {
    let id = crate::card_def::card_id_by_name("Koma, World-Eater").unwrap();
    assert_eq!(
        super::super::super::roots::registered_permanent_printing(id).unwrap(),
        Some(super::super::super::roots::baseline(id).unwrap())
    );
    for p in [PlayerId::P0, PlayerId::P1] {
        let (mut own, mut native, source, _, counter, removal) = scene(p);
        let paid = own.snapshot();
        let raw = native.snapshot_v1();
        cast(&mut own, &mut native, counter);
        act(&mut own, &mut native, |a| {
            is_original_spell_target(a, source)
        });
        settle(&mut own, &mut native);
        assert_eq!(
            own.fixture_state().objects.get(counter).zone,
            Zone::Graveyard
        );
        assert_eq!(
            own.fixture_state().objects.get(source).zone,
            Zone::Battlefield
        );
        own.restore(&paid).unwrap();
        native.kaito_restore_v75(&raw).unwrap();
        settle(&mut own, &mut native);
        assert_eq!(
            crate::engine::checked_effective_pt(own.fixture_state(), source).unwrap(),
            (8, 12)
        );
        assert!(crate::engine::has_effective_keyword(
            own.fixture_state(),
            source,
            Keywords::TRAMPLE
        ));
        let entered = own.snapshot();
        let entered_raw = native.snapshot_v1();
        for pay in [false, true] {
            let mut expected = None;
            for _ in 0..2 {
                own.restore(&entered).unwrap();
                native.kaito_restore_v75(&entered_raw).unwrap();
                cast(&mut own, &mut native, removal);
                choose(&mut own, &mut native, removal, source);
                for _ in 0..64 {
                    if actions(&own)
                        .iter()
                        .any(|a| matches!(a, A::ChooseEffectBoolean { .. }))
                    {
                        break;
                    }
                    act(&mut own, &mut native, |a| {
                        matches!(
                            a,
                            A::Pass { .. }
                                | A::OrderTriggers { .. }
                                | A::ChooseTriggerOrderNext { .. }
                        )
                    });
                }
                assert_eq!(tapped(&own, p.opponent()), 3);
                checkpoint(&mut own, &mut native);
                act(
                    &mut own,
                    &mut native,
                    |a| matches!(a,A::ChooseEffectBoolean{value,..}if *value==pay),
                );
                settle(&mut own, &mut native);
                assert_eq!(tapped(&own, p.opponent()), if pay { 7 } else { 3 });
                assert_eq!(
                    own.fixture_state().objects.get(source).zone,
                    if pay {
                        Zone::Graveyard
                    } else {
                        Zone::Battlefield
                    }
                );
                assert_eq!(
                    own.fixture_state().objects.get(removal).zone,
                    Zone::Graveyard
                );
                assert_eq!(
                    own.fixture_state().players[p.opponent().index()].mana_pool,
                    [0; 6]
                );
                if let Some(ref old) = expected {
                    assert_eq!(own.fixture_state(), old);
                } else {
                    expected = Some(own.fixture_state().clone());
                }
                checkpoint(&mut own, &mut native);
            }
        }
    }
}
#[test]
fn paid_koma_actual_cavalry_haste_combat_damage_four_coils_as_one_batch_and_restore_both_seats() {
    let coil = crate::card_def::card_id_by_name("Koma's Coil Token").unwrap();
    for p in [PlayerId::P0, PlayerId::P1] {
        let (mut own, mut native, source, cavalry, _, _) = scene(p);
        settle(&mut own, &mut native);
        act(
            &mut own,
            &mut native,
            |a| matches!(a,A::ActivateAbility{source:s,ability_index:0,..}if s.arena_id==cavalry.0),
        );
        choose(&mut own, &mut native, cavalry, source);
        settle(&mut own, &mut native);
        assert!(own.fixture_state().objects.get(cavalry).tapped);
        assert!(crate::engine::has_effective_keyword(
            own.fixture_state(),
            source,
            Keywords::HASTE
        ));
        let saved = own.snapshot();
        let raw = native.snapshot_v1();
        for attack in [false, true] {
            let mut expected = None;
            for _ in 0..2 {
                own.restore(&saved).unwrap();
                native.kaito_restore_v75(&raw).unwrap();
                through_combat(&mut own, &mut native, source, attack);
                let s = own.fixture_state();
                let tokens = s.players[p.index()]
                    .battlefield
                    .iter()
                    .copied()
                    .filter(|&o| s.objects.get(o).card_def == coil)
                    .collect::<Vec<_>>();
                assert_eq!(tokens.len(), if attack { 4 } else { 0 });
                assert_eq!(
                    s.players[p.opponent().index()].life,
                    if attack { 12 } else { 20 }
                );
                for token in tokens {
                    let o = s.objects.get(token);
                    assert_eq!((o.owner, o.controller), (p, p));
                    assert!(o.v4.is_token && o.summoning_sick);
                    assert!(!o.tapped);
                    assert_eq!(
                        crate::engine::checked_effective_pt(s, token).unwrap(),
                        (3, 3)
                    );
                    assert!(!crate::engine::has_effective_keyword(
                        s,
                        token,
                        Keywords::HASTE
                    ));
                }
                if let Some(ref old) = expected {
                    assert_eq!(s, old);
                } else {
                    expected = Some(s.clone());
                }
                checkpoint(&mut own, &mut native);
            }
        }
    }
}
