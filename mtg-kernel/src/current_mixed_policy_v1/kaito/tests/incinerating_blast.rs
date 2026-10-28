use super::*;
use super::{
    surveil_many::{checkpoint, sessions},
    targeted_entry::{actions, cast, choose},
};
use crate::{
    ids::ObjectId,
    policy_observation_v6::tests::put,
    state::{GameState, Step, Zone},
};

struct Scene {
    own: KaitoSession,
    native: FastActorSessionV1,
    source: ObjectId,
    victim: ObjectId,
    hand: Vec<ObjectId>,
    counter: ObjectId,
    removal: ObjectId,
}
fn scene(p: PlayerId, target_player: PlayerId, hand: usize, empty: bool, crystal: bool) -> Scene {
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    let own = if empty { vec![] } else { vec![forest; 40] };
    let other = vec![forest; 40];
    let (left, right) = if p == PlayerId::P0 {
        (own, other)
    } else {
        (other, own)
    };
    let mut s = GameState::new_from_libraries_with_starting_player_v1(
        &left,
        &right,
        crate::rl::card_name,
        110_294,
        p,
    );
    s.step = Step::Main1;
    for _ in 0..5 {
        put(&mut s, p, "Mountain", Zone::Battlefield);
    }
    for _ in 0..3 {
        put(&mut s, p.opponent(), "Swamp", Zone::Battlefield);
    }
    for _ in 0..2 {
        put(&mut s, p.opponent(), "Island", Zone::Battlefield);
    }
    let victim = put(&mut s, target_player, "Serra Angel", Zone::Battlefield);
    if crystal {
        put(
            &mut s,
            target_player,
            "Crystal Barricade",
            Zone::Battlefield,
        );
    }
    let source = put(&mut s, p, "Incinerating Blast", Zone::Hand);
    let hand = ["Forest", "Llanowar Elves"]
        .iter()
        .take(hand)
        .map(|name| put(&mut s, p, name, Zone::Hand))
        .collect();
    let counter = put(&mut s, p.opponent(), "Counterspell", Zone::Hand);
    let removal = put(&mut s, p.opponent(), "Hero's Downfall", Zone::Hand);
    let (own, native) = sessions(s);
    Scene {
        own,
        native,
        source,
        victim,
        hand,
        counter,
        removal,
    }
}
fn begin(s: &mut Scene, p: PlayerId) {
    cast(&mut s.own, &mut s.native, s.source);
    choose(&mut s.own, &mut s.native, s.source, s.victim);
    assert!(s.own.fixture_state().engine.pending_cast.is_none());
    assert_eq!(
        s.own.fixture_state().objects.get(s.source).zone,
        Zone::Stack
    );
    assert_eq!(
        s.own.fixture_state().players[p.index()]
            .battlefield
            .iter()
            .filter(|&&o| s.own.fixture_state().objects.get(o).tapped)
            .count(),
        5
    );
}
fn optional(a: &A, source: ObjectId) -> bool {
    matches!(a, A::ChooseOptionalCostUse { .. })
        || matches!(a,A::ChooseEffectTarget{source:s,min_targets:0,max_targets:1,..}|A::FinishEffectSelection{source:s,..}if s.arena_id==source.0)
}
fn wait_optional(s: &mut Scene, p: PlayerId) {
    for _ in 0..64 {
        if actions(&s.own).iter().any(|a| optional(a, s.source)) {
            let FastActorResponseV1::Decision(h) = s.own.current_response() else {
                panic!()
            };
            assert_eq!(h.acting_player, p.into());
            assert_eq!(
                s.own.fixture_state().objects.get(s.source).zone,
                Zone::Stack
            );
            checkpoint(&mut s.own, &mut s.native);
            return;
        }
        act(&mut s.own, &mut s.native, |a| matches!(a, A::Pass { .. }));
    }
    panic!("Blast did not offer its genuine optional discard");
}
fn pick(s: &mut Scene, card: Option<ObjectId>) {
    let menu = actions(&s.own);
    if menu
        .iter()
        .any(|a| matches!(a, A::ChooseOptionalCostUse { .. }))
    {
        assert_eq!(
            menu.iter()
                .filter(|a| matches!(a, A::ChooseOptionalCostUse { .. }))
                .count(),
            2
        );
        act(
            &mut s.own,
            &mut s.native,
            |a| matches!(a,A::ChooseOptionalCostUse{use_cost,..}if *use_cost==card.is_some()),
        );
        if let Some(card) = card {
            if matches!(s.own.current_response(), FastActorResponseV1::Terminal(_)) {
                return;
            }
            let menu = actions(&s.own);
            if menu.iter().any(|a| matches!(a, A::Discard { .. })) {
                assert_eq!(
                    menu.iter()
                        .filter(|a| matches!(a, A::Discard { .. }))
                        .count(),
                    s.hand.len()
                );
                assert!(s.hand.iter().all(|o| menu.iter().any(
                    |a| matches!(a,A::Discard{cards,..}if cards.len()==1&&cards[0].arena_id==o.0)
                )));
                act(
                    &mut s.own,
                    &mut s.native,
                    |a| matches!(a,A::Discard{cards,..}if cards.len()==1&&cards[0].arena_id==card.0),
                );
            } else {
                assert_eq!(s.hand.len(), 1, "multi-card cost selection was lost");
            }
        }
    } else {
        assert_eq!(
            menu.iter()
                .filter(
                    |a| matches!(a,A::ChooseEffectTarget{source,..}if source.arena_id==s.source.0)
                )
                .count(),
            s.hand.len()
        );
        assert!(menu.iter().any(|a|matches!(a,A::FinishEffectSelection{source,selected_count:0,..}if source.arena_id==s.source.0)));
        assert!(s.hand.iter().all(|o|menu.iter().any(|a|matches!(a,A::ChooseEffectTarget{source,target:crate::rl::TargetRefV1::Object{object},min_targets:0,max_targets:1,..}if source.arena_id==s.source.0&&object.arena_id==o.0))));
        if let Some(card) = card {
            act(
                &mut s.own,
                &mut s.native,
                |a| matches!(a,A::ChooseEffectTarget{source,target:crate::rl::TargetRefV1::Object{object},..}if source.arena_id==s.source.0&&object.arena_id==card.0),
            );
            if matches!(s.own.current_response(), FastActorResponseV1::Terminal(_)) {
                return;
            }
            if actions(&s.own).iter().any(|a|matches!(a,A::FinishEffectSelection{source,selected_count:1,..}if source.arena_id==s.source.0)) {
                act(&mut s.own,&mut s.native,|a|matches!(a,A::FinishEffectSelection{source,selected_count:1,..}if source.arena_id==s.source.0));
            }
        } else {
            act(
                &mut s.own,
                &mut s.native,
                |a| matches!(a,A::FinishEffectSelection{source,selected_count:0,..}if source.arena_id==s.source.0),
            );
        }
    }
}
fn finish(s: &mut Scene, no_optional: bool) {
    for _ in 0..64 {
        if matches!(s.own.current_response(), FastActorResponseV1::Terminal(_)) {
            return;
        }
        let state = s.own.fixture_state();
        if state.stack.is_empty()
            && state.engine.pending_effect.is_none()
            && state.engine.pending_discard.is_none()
            && state.engine.pending_optional_cost.is_none()
        {
            return;
        }
        if no_optional {
            assert!(!actions(&s.own).iter().any(|a| optional(a, s.source)));
        }
        act(&mut s.own, &mut s.native, |a| {
            matches!(
                a,
                A::Pass { .. } | A::OrderTriggers { .. } | A::ChooseTriggerOrderNext { .. }
            )
        });
    }
    panic!("Blast did not finish");
}
#[test]
fn paid_blast_six_damage_precedes_decline_or_private_land_creature_discard_draw_and_restore_both_seats(
) {
    let id = crate::card_def::card_id_by_name("Incinerating Blast").unwrap();
    assert_eq!(
        super::super::super::roots::registered_spell_printing(id).unwrap(),
        Some(super::super::super::roots::baseline(id).unwrap())
    );
    for p in [PlayerId::P0, PlayerId::P1] {
        for target in [p, p.opponent()] {
            let mut s = scene(p, target, 2, false, false);
            begin(&mut s, p);
            wait_optional(&mut s, p);
            assert_eq!(
                s.own.fixture_state().objects.get(s.victim).zone,
                Zone::Battlefield,
                "SBA ran inside resolution"
            );
            assert_eq!(s.own.fixture_state().objects.get(s.victim).damage, 6);
            let drawn = s.own.fixture_state().players[p.index()].library[0];
            let saved = s.own.snapshot();
            let raw = s.native.snapshot_v1();
            for selected in [None, Some(s.hand[0]), Some(s.hand[1])] {
                let mut expected = None;
                for _ in 0..2 {
                    s.own.restore(&saved).unwrap();
                    s.native.kaito_restore_v75(&raw).unwrap();
                    pick(&mut s, selected);
                    finish(&mut s, false);
                    let state = s.own.fixture_state();
                    assert_eq!(state.objects.get(s.source).zone, Zone::Graveyard);
                    assert_eq!(state.objects.get(s.victim).zone, Zone::Graveyard);
                    assert_eq!(
                        state.players[p.index()].library.len(),
                        if selected.is_some() { 39 } else { 40 }
                    );
                    assert_eq!(
                        state.players[p.index()].hand.contains(&drawn),
                        selected.is_some()
                    );
                    for &o in &s.hand {
                        assert_eq!(
                            state.objects.get(o).zone,
                            if Some(o) == selected {
                                Zone::Graveyard
                            } else {
                                Zone::Hand
                            }
                        );
                    }
                    assert_eq!(state.players[p.index()].mana_pool, [0; 6]);
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
#[test]
fn paid_blast_counter_or_illegal_target_skips_optional_cost_and_empty_hand_skips_offer_both_seats()
{
    for p in [PlayerId::P0, PlayerId::P1] {
        for branch in 0..3 {
            let mut s = scene(
                p,
                p.opponent(),
                if branch == 0 { 0 } else { 2 },
                false,
                false,
            );
            begin(&mut s, p);
            let saved = s.own.snapshot();
            let raw = s.native.snapshot_v1();
            let mut expected = None;
            for _ in 0..2 {
                s.own.restore(&saved).unwrap();
                s.native.kaito_restore_v75(&raw).unwrap();
                if branch != 0 {
                    act(&mut s.own, &mut s.native, |a| matches!(a, A::Pass { .. }));
                    if branch == 1 {
                        cast(&mut s.own, &mut s.native, s.counter);
                        act(&mut s.own, &mut s.native, |a| {
                            is_original_spell_target(a, s.source)
                        });
                    } else {
                        cast(&mut s.own, &mut s.native, s.removal);
                        choose(&mut s.own, &mut s.native, s.removal, s.victim);
                    }
                }
                finish(&mut s, true);
                let state = s.own.fixture_state();
                assert_eq!(state.objects.get(s.source).zone, Zone::Graveyard);
                assert_eq!(
                    state.objects.get(s.victim).zone,
                    if branch == 1 {
                        Zone::Battlefield
                    } else {
                        Zone::Graveyard
                    }
                );
                assert!(s
                    .hand
                    .iter()
                    .all(|o| state.players[p.index()].hand.contains(o)));
                assert_eq!(state.players[p.index()].library.len(), 40);
                assert_eq!(state.players[p.index()].mana_pool, [0; 6]);
                if let Some(ref old) = expected {
                    assert_eq!(state, old);
                } else {
                    expected = Some(state.clone());
                }
            }
        }
    }
}
#[test]
fn paid_blast_prevention_keeps_optional_instruction_and_actual_paid_empty_library_draw_loses_after_completion_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        for hand in [0, 1] {
            let mut s = scene(p, p.opponent(), hand, true, true);
            begin(&mut s, p);
            let saved = s.own.snapshot();
            let raw = s.native.snapshot_v1();
            let mut expected = None;
            for _ in 0..2 {
                s.own.restore(&saved).unwrap();
                s.native.kaito_restore_v75(&raw).unwrap();
                if hand == 1 {
                    wait_optional(&mut s, p);
                    assert_eq!(s.own.fixture_state().objects.get(s.victim).damage, 0);
                    let card = s.hand[0];
                    pick(&mut s, Some(card));
                }
                finish(&mut s, hand == 0);
                let state = s.own.fixture_state();
                assert_eq!(state.objects.get(s.source).zone, Zone::Graveyard);
                assert_eq!(state.objects.get(s.victim).zone, Zone::Battlefield);
                assert_eq!(state.objects.get(s.victim).damage, 0);
                if hand == 1 {
                    let FastActorResponseV1::Terminal(terminal) = s.own.current_response() else {
                        panic!("paid empty-library draw did not lose")
                    };
                    assert_eq!(
                        terminal.terminal_classification,
                        crate::rl::TerminalClassificationV1::Natural
                    );
                    assert_eq!(terminal.winner, Some(p.opponent().into()));
                    assert_eq!(state.objects.get(s.hand[0]).zone, Zone::Graveyard);
                } else {
                    assert!(matches!(
                        s.own.current_response(),
                        FastActorResponseV1::Decision(_)
                    ));
                }
                assert!(
                    state.engine.pending_effect.is_none()
                        && state.engine.pending_discard.is_none()
                        && state.engine.pending_optional_cost.is_none()
                );
                assert_eq!(state.engine.event_history.iter().filter(|e|matches!(e,crate::event::CommittedEvent::Draw{player,object:None}if *player==p)).count(),hand);
                if let Some(ref old) = expected {
                    assert_eq!(state, old);
                } else {
                    expected = Some(state.clone());
                }
            }
        }
    }
}
