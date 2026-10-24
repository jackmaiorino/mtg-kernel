use super::surveil_many::{checkpoint, sessions};
use super::targeted_entry::{actions, cast, choose};
use super::*;
use crate::{
    ids::ObjectId,
    policy_observation_v6::tests::put,
    state::{GameState, StackItemKind, Step, Zone},
};

fn wait_trigger(own: &mut KaitoSession, native: &mut FastActorSessionV1, source: ObjectId) {
    for _ in 0..128 {
        if own.fixture_state().stack.last().is_some_and(|item| {
            item.source == source && item.kind == StackItemKind::TriggeredAbility
        }) {
            assert!(own.fixture_state().stack.last().unwrap().targets.is_empty());
            checkpoint(own, native);
            return;
        }
        act(own, native, |a| {
            matches!(
                a,
                A::Pass { .. } | A::OrderTriggers { .. } | A::ChooseTriggerOrderNext { .. }
            )
        });
    }
    panic!("actual event-object trigger not published");
}

#[test]
fn paid_unicorn_other_entrant_exact_counter_source_or_entrant_departure_tokens_counter_and_restore_both_seats(
) {
    let id = crate::card_def::card_id_by_name("Good-Fortune Unicorn").unwrap();
    assert_eq!(
        super::super::super::roots::registered_permanent_printing(id).unwrap(),
        Some(super::super::super::roots::baseline(id).unwrap())
    );
    let island = crate::card_def::card_id_by_name("Island").unwrap();
    for p in [PlayerId::P0, PlayerId::P1] {
        let q = p.opponent();
        let mut s = GameState::new_from_libraries_with_starting_player_v1(
            &[island; 40],
            &[island; 40],
            crate::rl::card_name,
            110_401,
            p,
        );
        s.step = Step::Main1;
        for land in ["Plains", "Forest"] {
            for _ in 0..4 {
                put(&mut s, p, land, Zone::Battlefield);
            }
        }
        for _ in 0..4 {
            put(&mut s, q, "Island", Zone::Battlefield);
        }
        let source = put(&mut s, p, "Good-Fortune Unicorn", Zone::Hand);
        let entrant = put(&mut s, p, "Healer's Hawk", Zone::Hand);
        let tokens = put(&mut s, p, "Resolute Reinforcements", Zone::Hand);
        let land = put(&mut s, p, "Forest", Zone::Hand);
        let other = put(&mut s, q, "Spectral Sailor", Zone::Hand);
        let counter = put(&mut s, q, "Counterspell", Zone::Hand);
        let remove = put(&mut s, q, "Unsummon", Zone::Hand);
        let (mut own, mut native) = sessions(s);
        cast(&mut own, &mut native, source);
        let paid = own.snapshot();
        let raw = native.snapshot_v1();
        let mut prior = None;
        for _ in 0..2 {
            own.restore(&paid).unwrap();
            native.kaito_restore_v75(&raw).unwrap();
            cast(&mut own, &mut native, counter);
            act(&mut own, &mut native, |a| {
                is_original_spell_target(a, source)
            });
            settle(&mut own, &mut native);
            assert_eq!(
                own.fixture_state().objects.get(source).zone,
                Zone::Graveyard
            );
            if let Some(ref old) = prior {
                assert_eq!(own.fixture_state(), old);
            } else {
                prior = Some(own.fixture_state().clone());
            }
        }
        own.restore(&paid).unwrap();
        native.kaito_restore_v75(&raw).unwrap();
        settle(&mut own, &mut native);
        assert_eq!(
            own.fixture_state().objects.get(source).counters.plus1_plus1,
            0
        );
        cast(&mut own, &mut native, other);
        settle(&mut own, &mut native);
        assert_eq!(
            own.fixture_state().objects.get(other).counters.plus1_plus1,
            0
        );
        if !actions(&own)
            .iter()
            .any(|a| matches!(a,A::PlayLand{source:s,..}if s.arena_id==land.0))
        {
            act(&mut own, &mut native, |a| matches!(a, A::Pass { .. }));
        }
        act(
            &mut own,
            &mut native,
            |a| matches!(a,A::PlayLand{source:s,..}if s.arena_id==land.0),
        );
        assert!(own.fixture_state().stack.is_empty());
        assert!(own.fixture_state().engine.pending_triggers.is_empty());
        let ready = own.snapshot();
        let ready_raw = native.snapshot_v1();
        for token_entry in [false, true] {
            own.restore(&ready).unwrap();
            native.kaito_restore_v75(&ready_raw).unwrap();
            let subject = if token_entry { tokens } else { entrant };
            cast(&mut own, &mut native, subject);
            wait_trigger(&mut own, &mut native, source);
            assert_eq!(
                own.fixture_state()
                    .objects
                    .get(subject)
                    .counters
                    .plus1_plus1,
                0
            );
            let pending = own.snapshot();
            let pending_raw = native.snapshot_v1();
            for departed in [None, Some(source), Some(subject)] {
                // The Reinforcements token trigger may still be below the
                // first Unicorn trigger. Keep its observer present until the
                // actual token entry; departure branches use the lone Hawk.
                if token_entry && departed.is_some() {
                    continue;
                }
                let mut expected = None;
                for _ in 0..2 {
                    own.restore(&pending).unwrap();
                    native.kaito_restore_v75(&pending_raw).unwrap();
                    if let Some(object) = departed {
                        cast(&mut own, &mut native, remove);
                        choose(&mut own, &mut native, remove, object);
                    }
                    settle(&mut own, &mut native);
                    let s = own.fixture_state();
                    assert_eq!(s.objects.get(source).counters.plus1_plus1, 0);
                    assert_eq!(
                        s.objects.get(subject).zone,
                        if departed == Some(subject) {
                            Zone::Hand
                        } else {
                            Zone::Battlefield
                        }
                    );
                    assert_eq!(
                        s.objects.get(subject).counters.plus1_plus1,
                        i32::from(departed != Some(subject))
                    );
                    assert_eq!(s.objects.get(other).counters.plus1_plus1, 0);
                    if token_entry {
                        let created = s.players[p.index()]
                            .battlefield
                            .iter()
                            .copied()
                            .filter(|&o| s.objects.get(o).name == "Soldier Token")
                            .collect::<Vec<_>>();
                        assert_eq!(created.len(), 1);
                        assert_eq!(s.objects.get(created[0]).counters.plus1_plus1, 1);
                    }
                    assert_eq!(s.players[p.index()].mana_pool, [0; 6]);
                    if let Some(ref old) = expected {
                        assert_eq!(s, old);
                    } else {
                        expected = Some(s.clone());
                    }
                }
            }
        }
    }
}
