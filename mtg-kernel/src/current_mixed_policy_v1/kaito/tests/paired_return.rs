use super::*;
use super::{
    surveil_many::{checkpoint, sessions},
    targeted_entry::{actions, cast, choose},
};
use crate::{
    event::{self, ProposedEvent},
    ids::ObjectId,
    policy_observation_v6::tests::put,
    state::{GameState, Step, Zone},
};

fn target_menu(own: &KaitoSession) -> Vec<ObjectId> {
    actions(own)
        .into_iter()
        .filter_map(|a| match a {
            A::ChooseTarget {
                target: crate::rl::TargetRefV1::Object { object },
                ..
            } => Some(ObjectId(object.arena_id)),
            _ => None,
        })
        .collect()
}
fn pick(own: &mut KaitoSession, native: &mut FastActorSessionV1, target: ObjectId) {
    act(own, native, |a| {
        matches!(a,
        A::ChooseTarget { target: crate::rl::TargetRefV1::Object { object }, .. }
        if object.arena_id == target.0)
    });
}

#[test]
fn paid_pair_both_pick_orders_distinct_controllers_owner_return_partial_illegality_counter_and_restore_both_seats(
) {
    let id = crate::card_def::card_id_by_name("Run Away Together").unwrap();
    assert_eq!(
        super::super::super::roots::registered_spell_printing(id).unwrap(),
        Some(super::super::super::roots::baseline(id).unwrap())
    );
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    for p in [PlayerId::P0, PlayerId::P1] {
        for reverse in [false, true] {
            for stolen in [false, true] {
                let mut s = GameState::new_from_libraries_with_starting_player_v1(
                    &[forest; 40],
                    &[forest; 40],
                    crate::rl::card_name,
                    110_385,
                    p,
                );
                s.step = Step::Main1;
                for _ in 0..2 {
                    put(&mut s, p, "Island", Zone::Battlefield);
                    put(&mut s, p.opponent(), "Island", Zone::Battlefield);
                }
                put(&mut s, p.opponent(), "Mountain", Zone::Battlefield);
                let controllers = [p, p.opponent()];
                let owners = controllers.map(|c| if stolen { c.opponent() } else { c });
                let targets = std::array::from_fn::<_, 2, _>(|i| {
                    let target = put(&mut s, owners[i], "Llanowar Elves", Zone::Hand);
                    event::propose_and_commit(
                        &mut s,
                        ProposedEvent::zone_change_to_battlefield_under_controller(
                            target,
                            controllers[i],
                        ),
                    );
                    target
                });
                let alternatives =
                    controllers.map(|c| put(&mut s, c, "Serra Angel", Zone::Battlefield));
                let source = put(&mut s, p, "Run Away Together", Zone::Hand);
                let counter = put(&mut s, p.opponent(), "Counterspell", Zone::Hand);
                let bolt = put(&mut s, p.opponent(), "Lightning Bolt", Zone::Hand);
                let (mut own, mut native) = sessions(s);
                cast(&mut own, &mut native, source);
                let mut offered = target_menu(&own);
                offered.sort_unstable();
                let mut all = [targets[0], targets[1], alternatives[0], alternatives[1]];
                all.sort_unstable();
                assert_eq!(offered, all);
                let order = if reverse { [1, 0] } else { [0, 1] };
                checkpoint(&mut own, &mut native);
                pick(&mut own, &mut native, targets[order[0]]);
                let mut second = target_menu(&own);
                second.sort_unstable();
                let mut legal_second = [targets[order[1]], alternatives[order[1]]];
                legal_second.sort_unstable();
                assert_eq!(second, legal_second);
                assert!(!actions(&own).iter().any(|a| matches!(
                    a,
                    A::FinishTargetSelection {
                        selected_count: 1,
                        ..
                    }
                )));
                checkpoint(&mut own, &mut native);
                pick(&mut own, &mut native, targets[order[1]]);
                assert!(own.fixture_state().engine.pending_cast.is_none());
                assert_eq!(own.fixture_state().objects.get(source).zone, Zone::Stack);
                assert_eq!(
                    own.fixture_state().stack.last().unwrap().targets,
                    [
                        crate::state::Target::Object(targets[order[0]]),
                        crate::state::Target::Object(targets[order[1]])
                    ]
                );
                assert_eq!(own.fixture_state().players[p.index()].mana_pool, [0; 6]);
                assert_eq!(
                    own.fixture_state().players[p.index()]
                        .battlefield
                        .iter()
                        .filter(|&&o| own.fixture_state().objects.get(o).tapped)
                        .count(),
                    2
                );
                let paid = own.snapshot();
                let raw = native.snapshot_v1();
                for response in 0..4 {
                    let mut expected = None;
                    for _ in 0..2 {
                        own.restore(&paid).unwrap();
                        native.kaito_restore_v75(&raw).unwrap();
                        if response != 0 {
                            act(&mut own, &mut native, |a| matches!(a, A::Pass { .. }));
                            if response == 1 {
                                cast(&mut own, &mut native, counter);
                                act(&mut own, &mut native, |a| {
                                    is_original_spell_target(a, source)
                                });
                            } else {
                                cast(&mut own, &mut native, bolt);
                                choose(&mut own, &mut native, bolt, targets[response - 2]);
                            }
                        }
                        settle(&mut own, &mut native);
                        let state = own.fixture_state();
                        assert_eq!(state.objects.get(source).zone, Zone::Graveyard);
                        for i in 0..2 {
                            let zone = if response == 1 {
                                Zone::Battlefield
                            } else if response == i + 2 {
                                Zone::Graveyard
                            } else {
                                Zone::Hand
                            };
                            assert_eq!(state.objects.get(targets[i]).zone, zone);
                            assert_eq!(state.objects.get(targets[i]).owner, owners[i]);
                            if zone == Zone::Hand {
                                assert!(state.players[owners[i].index()]
                                    .hand
                                    .contains(&targets[i]));
                                assert_eq!(state.objects.get(targets[i]).controller, owners[i]);
                            }
                            assert_eq!(state.objects.get(alternatives[i]).zone, Zone::Battlefield);
                        }
                        assert_eq!((state.players[0].life, state.players[1].life), (20, 20));
                        if let Some(ref old) = expected {
                            assert_eq!(state, old);
                        } else {
                            expected = Some(state.clone());
                        }
                    }
                }
            }
        }
    }
}
