use super::*;
use super::{
    surveil_many::{checkpoint, sessions},
    targeted_entry::{actions, cast},
};
use crate::{
    ids::ObjectId,
    policy_observation_v6::tests::put,
    policy_surface_v5::LimitedAppendixV1,
    state::{GameState, Step, Zone},
};

#[path = "pilfer_probe.rs"]
mod probe;
#[path = "pilfer_sampling.rs"]
mod sampling;

fn scene(p: PlayerId) -> (GameState, ObjectId) {
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    let mut s = GameState::new_from_libraries_with_starting_player_v1(
        &[forest; 40],
        &[forest; 40],
        crate::rl::card_name,
        110_350,
        p,
    );
    s.step = Step::Main1;
    for _ in 0..3 {
        put(&mut s, p, "Swamp", Zone::Battlefield);
        put(&mut s, p.opponent(), "Island", Zone::Battlefield);
    }
    let source = put(&mut s, p, "Pilfer", Zone::Hand);
    (s, source)
}
fn setup(
    p: PlayerId,
    kind: u8,
    creature: &str,
) -> (KaitoSession, FastActorSessionV1, ObjectId, Option<ObjectId>) {
    let (mut s, source) = scene(p);
    if kind > 0 {
        put(&mut s, p.opponent(), "Forest", Zone::Hand);
        put(&mut s, p.opponent(), "Plains", Zone::Hand);
    }
    let counter = if kind == 2 {
        put(&mut s, p.opponent(), creature, Zone::Hand);
        put(&mut s, p.opponent(), "Giant Growth", Zone::Hand);
        Some(put(&mut s, p.opponent(), "Counterspell", Zone::Hand))
    } else {
        None
    };
    let (own, native) = sessions(s);
    (own, native, source, counter)
}

fn full_pool_pending(
    p: PlayerId,
    mut s: GameState,
    source: ObjectId,
) -> (KaitoSession, FastActorSessionV1, Packet) {
    FastActorSessionV1::enable_limited_native_modes_v1(&mut s, LimitedAppendixV1::FullPool);
    s.reveal_knowledge_v4 = Some(Default::default());
    s.reveal_meaning_v3.enable();
    let mut native = FastActorSessionV1::from_kaito_v75_fixture_state_scoped(
        s.clone(),
        LimitedAppendixV1::FullPool,
    );
    let mut own = KaitoSession::from_session(
        FastActorSessionV1::from_kaito_v75_fixture_state_scoped(s, LimitedAppendixV1::FullPool),
    )
    .unwrap();
    let before = packet(&own);
    begin(&mut own, &mut native, source, p);
    for _ in 0..32 {
        if own.fixture_state().engine.pending_effect.is_some() {
            return (own, native, before);
        }
        act(&mut own, &mut native, |a| matches!(a, A::Pass { .. }));
    }
    panic!("no paid Pilfer choice");
}

#[test]
fn full_pool_paid_pilfer_uses_public_names_and_exact_native_answers_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let opponent = p.opponent();
        let mut paired_packets = None;
        let mut mapped_orders = Vec::new();
        for names in [
            [
                "Forest",
                "Giant Growth",
                "Hero's Downfall",
                "Hero's Downfall",
                "Plains",
            ],
            [
                "Hero's Downfall",
                "Forest",
                "Hero's Downfall",
                "Plains",
                "Giant Growth",
            ],
        ] {
            let (mut s, source) = scene(p);
            let hand = names.map(|name| put(&mut s, opponent, name, Zone::Hand));
            let (mut own, mut native, before) = full_pool_pending(p, s, source);
            let FastActorResponseV1::Decision(h) = own.current_response() else {
                panic!("no paid Pilfer choice");
            };
            assert_eq!(h.acting_player, p.into());
            let d = own.encode(h).unwrap();
            assert_eq!(d.packet().actions().len(), 3);
            // A name operand is valid in the numeric successor, never in the
            // incoming typed wire projection.
            let mut forged_wire = serde_json::to_value(d.backend.candidates_v75()).unwrap();
            forged_wire["observation"]["common"]["common"]["projection"]["engine_context"]
                ["pending_effect"]["choice"]["legal_targets"][0] = serde_json::json!({
                "target_kind": "revealed_hand_name",
                "card_db_id": crate::card_def::card_id_by_name("Giant Growth").unwrap(),
                "card_name": "Giant Growth",
                "owner": PlayerSeatV1::from(opponent),
            });
            let raw_actions = forged_wire["candidates"].as_array().unwrap().clone();
            assert!(matches!(
                normalize::encode(Source::KaitoV75, forged_wire, raw_actions),
                Err(Error::UnsupportedOperand(operand)) if operand == "public_projection_operand"
            ));
            let inputs = own.training_input_v1(&d).unwrap();
            let logits = crate::current_tree_learner_v1::ModelV1::deterministic_fixture()
                .forward(&inputs)
                .unwrap();
            assert_eq!(logits.len(), 3);
            assert!(logits.iter().all(|v| v.is_finite()));
            if let Some((opening, revealed)) = &paired_packets {
                assert_eq!(opening, &before);
                assert_eq!(revealed, d.packet());
            } else {
                paired_packets = Some((before, d.packet().clone()));
            }
            let order = d.current.name_order.as_ref().unwrap();
            mapped_orders.push(order.clone());
            let expected = order
                .iter()
                .map(|&i| match &d.backend.actions()[i as usize] {
                    A::ChooseEffectTarget {
                        target: TargetRefV1::Object { object },
                        ..
                    } => object.card_db_id,
                    _ => panic!("incorrect Pilfer semantic"),
                })
                .collect::<Vec<_>>();
            assert!(expected.is_sorted());
            let downfall = crate::card_def::card_id_by_name("Hero's Downfall").unwrap();
            let HandFacts::RevealMinimaV3 { minima, .. } = own.hand_facts().unwrap() else {
                panic!("no prompt-local hand names");
            };
            assert!(minima[opponent.index()].contains(&(downfall, 2)));
            let revealed = &own.fixture_state().reveal_knowledge_v4.as_ref().unwrap()[p.index()];
            assert_eq!(
                revealed.hand(opponent).find(|&(id, _)| id == downfall),
                Some((downfall, 1))
            );

            let saved = own.snapshot();
            let raw = native.snapshot_v1();
            for (public, &backend) in order.iter().enumerate() {
                own.restore(&saved).unwrap();
                native.kaito_restore_v75(&raw).unwrap();
                assert!(own.apply(&d, public as u32).is_err());
                let FastActorResponseV1::Decision(restored) = own.current_response() else {
                    panic!("no restored Pilfer choice");
                };
                let fresh = own.encode(restored).unwrap();
                assert_eq!(fresh.packet(), d.packet());
                let A::ChooseEffectTarget {
                    target: TargetRefV1::Object { object },
                    ..
                } = &fresh.backend.actions()[backend as usize]
                else {
                    panic!("incorrect restored Pilfer semantic");
                };
                let discarded = ObjectId(object.arena_id);
                let native_index = fresh.backend.fixture_original_index_v75(backend).unwrap();
                assert_eq!(
                    own.apply(&fresh, public as u32).unwrap(),
                    native
                        .step(restored.episode_id, restored.step, native_index)
                        .unwrap(),
                );
                settle(&mut own, &mut native);
                let state = own.fixture_state();
                assert_eq!(state, native.kaito_fixture_state());
                assert_eq!(state.objects.get(discarded).zone, Zone::Graveyard);
                assert!(hand.iter().filter(|&&id| id != discarded).all(|&id| state
                    .objects
                    .get(id)
                    .zone
                    == Zone::Hand));
                let retained = &state.reveal_knowledge_v4.as_ref().unwrap()[p.index()];
                let prior_minimum = (object.card_db_id != downfall).then_some((downfall, 1));
                assert_eq!(
                    retained.hand(opponent).find(|&(id, _)| id == downfall),
                    prior_minimum
                );
            }
        }
        assert_ne!(mapped_orders[0], mapped_orders[1]);
    }
}
fn packet(own: &KaitoSession) -> crate::current_mixed_policy_v1::Packet {
    let FastActorResponseV1::Decision(h) = own.current_response() else {
        panic!()
    };
    own.encode(h).unwrap().packet().clone()
}
fn known(s: &GameState, observer: PlayerId, target: PlayerId) -> Vec<u32> {
    let observed =
        crate::rl::observe_v2(s, &crate::surface_v2::HarnessSurfaceV2::new(), observer, 0).unwrap();
    let mut cards = observed.known_hand_cards[target.index()]
        .iter()
        .map(|c| c.stable.arena_id)
        .collect::<Vec<_>>();
    cards.sort_unstable();
    cards
}
fn target_player(a: &A, p: PlayerId) -> bool {
    matches!(a,A::ChooseTarget{target:crate::rl::TargetRefV1::Player{player},..}if *player==p.into())
}
fn begin(own: &mut KaitoSession, native: &mut FastActorSessionV1, source: ObjectId, p: PlayerId) {
    cast(own, native, source);
    let targets = actions(own);
    assert!(targets.iter().any(|a| target_player(a, p.opponent())));
    assert!(!targets.iter().any(|a| target_player(a, p)));
    checkpoint(own, native);
    act(own, native, |a| target_player(a, p.opponent()));
    assert!(own.fixture_state().engine.pending_cast.is_none());
    assert_eq!(own.fixture_state().objects.get(source).zone, Zone::Stack);
    assert_eq!(
        own.fixture_state().players[p.index()]
            .battlefield
            .iter()
            .filter(|&&o| own.fixture_state().objects.get(o).tapped)
            .count(),
        2
    );
}
fn chosen(a: &A, source: ObjectId, card: ObjectId) -> bool {
    matches!(a,A::ChooseEffectTarget{source:s,target:crate::rl::TargetRefV1::Object{object},selected_count:0,min_targets:1,max_targets:1,..}if s.arena_id==source.0&&object.arena_id==card.0)
}
#[test]
fn paid_pilfer_hidden_world_equality_revealed_full_hand_caster_nonland_choice_and_restore_both_seats(
) {
    let id = crate::card_def::card_id_by_name("Pilfer").unwrap();
    assert_eq!(
        super::super::super::roots::registered_spell_printing(id).unwrap(),
        Some(super::super::super::roots::baseline(id).unwrap())
    );
    for p in [PlayerId::P0, PlayerId::P1] {
        let (mut own, mut native, source, _) = setup(p, 2, "Savannah Lions");
        let (other, _, _, _) = setup(p, 2, "Serra Angel");
        assert_eq!(
            packet(&own),
            packet(&other),
            "unknown opponent creature identity changed numeric input"
        );
        assert!(known(own.fixture_state(), p, p.opponent()).is_empty());
        drop(other);
        begin(&mut own, &mut native, source, p);
        for _ in 0..32 {
            if actions(&own)
                .iter()
                .any(|a| matches!(a,A::ChooseEffectTarget{source:s,..}if s.arena_id==source.0))
            {
                break;
            }
            act(&mut own, &mut native, |a| matches!(a, A::Pass { .. }));
        }
        let FastActorResponseV1::Decision(h) = own.current_response() else {
            panic!()
        };
        assert_eq!(h.acting_player, p.into());
        let hand = own.fixture_state().players[p.opponent().index()]
            .hand
            .clone();
        let eligible = hand
            .iter()
            .copied()
            .filter(|&o| {
                !crate::engine::object_has_type(
                    own.fixture_state(),
                    o,
                    crate::card_def::CardType::Land,
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(eligible.len(), 3);
        let menu = actions(&own);
        assert_eq!(
            menu.iter()
                .filter(|a| matches!(a, A::ChooseEffectTarget { .. }))
                .count(),
            eligible.len()
        );
        assert!(eligible
            .iter()
            .all(|&o| menu.iter().any(|a| chosen(a, source, o))));
        assert!(!menu.iter().any(|a| matches!(
            a,
            A::FinishEffectSelection { .. } | A::FinishTargetSelection { .. }
        )));
        let mut identities = hand.iter().map(|o| o.0).collect::<Vec<_>>();
        identities.sort_unstable();
        assert_eq!(known(own.fixture_state(), p, p.opponent()), identities);
        checkpoint(&mut own, &mut native);
        let saved = own.snapshot();
        let raw = native.snapshot_v1();
        for card in eligible {
            let mut expected = None;
            for _ in 0..2 {
                own.restore(&saved).unwrap();
                native.kaito_restore_v75(&raw).unwrap();
                act(&mut own, &mut native, |a| chosen(a, source, card));
                settle(&mut own, &mut native);
                let s = own.fixture_state();
                assert_eq!(s.objects.get(card).zone, Zone::Graveyard);
                assert_eq!(s.objects.get(source).zone, Zone::Graveyard);
                assert_eq!(s.players[p.opponent().index()].hand.len(), hand.len() - 1);
                assert!(hand
                    .iter()
                    .filter(|&&o| o != card)
                    .all(|&o| s.objects.get(o).zone == Zone::Hand));
                assert_eq!(s.players[p.index()].mana_pool, [0; 6]);
                let mut retained = hand
                    .iter()
                    .filter(|&&o| o != card)
                    .map(|o| o.0)
                    .collect::<Vec<_>>();
                retained.sort_unstable();
                assert_eq!(known(s, p, p.opponent()), retained);
                if let Some(ref state) = expected {
                    assert_eq!(s, state);
                } else {
                    expected = Some(s.clone());
                }
            }
        }
    }
}
#[test]
fn paid_pilfer_empty_or_all_land_hand_has_no_choice_and_counter_prevents_reveal_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for kind in [0, 1, 2] {
            let (mut own, mut native, source, counter) = setup(p, kind, "Savannah Lions");
            begin(&mut own, &mut native, source, p);
            let saved = own.snapshot();
            let raw = native.snapshot_v1();
            let before = own.fixture_state().players[p.opponent().index()]
                .hand
                .clone();
            let mut expected = None;
            for _ in 0..2 {
                own.restore(&saved).unwrap();
                native.kaito_restore_v75(&raw).unwrap();
                if let Some(counter) = counter {
                    act(&mut own, &mut native, |a| matches!(a, A::Pass { .. }));
                    cast(&mut own, &mut native, counter);
                    act(&mut own, &mut native, |a| {
                        is_original_spell_target(a, source)
                    });
                }
                for _ in 0..32 {
                    let s = own.fixture_state();
                    if s.stack.is_empty() && s.engine.pending_effect.is_none() {
                        break;
                    }
                    assert!(!actions(&own).iter().any(
                        |a| matches!(a,A::ChooseEffectTarget{source:s,..}if s.arena_id==source.0)
                    ));
                    act(&mut own, &mut native, |a| matches!(a, A::Pass { .. }));
                }
                let s = own.fixture_state();
                assert!(s.stack.is_empty() && s.engine.pending_effect.is_none());
                assert_eq!(s.objects.get(source).zone, Zone::Graveyard);
                let remaining = before
                    .iter()
                    .copied()
                    .filter(|&o| Some(o) != counter)
                    .collect::<Vec<_>>();
                assert_eq!(s.players[p.opponent().index()].hand, remaining);
                let mut visible = if counter.is_some() {
                    vec![]
                } else {
                    remaining.iter().map(|o| o.0).collect::<Vec<_>>()
                };
                visible.sort_unstable();
                assert_eq!(known(s, p, p.opponent()), visible);
                assert_eq!(
                    s.players[p.index()]
                        .battlefield
                        .iter()
                        .filter(|&&o| s.objects.get(o).tapped)
                        .count(),
                    2
                );
                assert_eq!(s.players[p.index()].mana_pool, [0; 6]);
                if let Some(ref state) = expected {
                    assert_eq!(s, state);
                } else {
                    expected = Some(s.clone());
                }
            }
        }
    }
}
