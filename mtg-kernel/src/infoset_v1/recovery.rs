//! #579: versioned recovery without ordering or joining private copies.
use super::*;
use crate::engine::{self, Action, Decision};
use crate::event::{propose_and_commit, CommittedEvent, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::state::{
    FaceDownV1, GameState, PaidCostRefV4, StackItem, StackItemKind, StackStateV4, Target, Zone,
};

fn fixture(reverse_ids: bool) -> (GameState, [Vec<u16>; 2]) {
    let cards = ["Fiery Temper", "Island", "Mountain", "Island"].map(super::fixtures::id);
    let lists = [cards.to_vec(), cards.to_vec()];
    let mut state = GameState::new_from_libraries(
        &lists[0],
        &lists[1],
        |id| crate::card_def::CARD_DEFS[id as usize].name.into(),
        579,
    );
    if reverse_ids {
        // Relabel the entire pristine arena before any observation, including
        // each zone's IDs. This changes allocation order, not the deal.
        let count = state.objects.len() as u32;
        let mut value = super::rewrite::tagged_value(&state);
        fn relabel(value: &mut serde_json::Value, count: u32) {
            if let Some(id) = super::rewrite::parse_tag(value) {
                *value = (count - 1 - id.0).into();
                return;
            }
            match value {
                serde_json::Value::Array(xs) => xs.iter_mut().for_each(|x| relabel(x, count)),
                serde_json::Value::Object(xs) => xs.values_mut().for_each(|x| relabel(x, count)),
                _ => {}
            }
        }
        relabel(&mut value, count);
        value["objects"]["items"].as_array_mut().unwrap().reverse();
        state = serde_json::from_value(value).unwrap();
    }
    state.step = crate::state::Step::Main1;
    (state, lists)
}

fn input<'a>(
    state: &'a GameState,
    lists: &'a [Vec<u16>; 2],
    viewer: PlayerId,
) -> ExportInputV1<'a> {
    ExportInputV1 {
        state,
        journal: state.observation_journal.notes(),
        viewer,
        decklists: [&lists[0], &lists[1]],
        marks: None,
    }
}

fn graveyard_rollback(state: &mut GameState, owner: PlayerId) -> ObjectId {
    propose_and_commit(state, ProposedEvent::draw(owner));
    let card = state.players[owner.index()].hand[0];
    propose_and_commit(state, ProposedEvent::zone_change(card, Zone::Graveyard));
    // The reported madness rollback has no committed move for the attempted
    // stack entry and return. Its public card remains at the native epoch 4.
    state.objects.get_mut(card).zone_change_count += 2;
    card
}

fn waiting(source: ObjectId, controller: PlayerId) -> StackItem {
    StackItem {
        kind: StackItemKind::TriggeredAbility,
        source,
        controller,
        targets: vec![],
        is_copy: false,
        inline_effect: Some(crate::effect::EffectOp::Sequence(vec![])),
        discarded: vec![],
        is_flashback: false,
        mode_chosen: 0,
        madness_offer: false,
        kicked: false,
        v4: StackStateV4::default(),
    }
}

fn departed_public_damage(state: &mut GameState, owner: PlayerId) -> (ObjectId, usize) {
    propose_and_commit(state, ProposedEvent::draw(owner));
    let source = state.players[owner.index()].hand[0];
    let entry = state.engine.event_history.len();
    propose_and_commit(state, ProposedEvent::zone_change(source, Zone::Battlefield));
    // Event-level disguised-entry fixture, as in the v2 privacy tests. The
    // subsequent damage and hidden departure use actual committed operations.
    state.objects.get_mut(source).v4.face_down_v1 = Some(FaceDownV1 {
        disguised: true,
        lookers: 1 << owner.index(),
        hidden_by: None,
        hidden_by_source: None,
    });
    state
        .observation_journal
        .record_visibility_v2(entry, source, 1 << owner.index());
    state.forget_hand_after_face_down_departure_v1(owner);
    let damage = state.engine.event_history.len();
    propose_and_commit(
        state,
        ProposedEvent::damage(source, Target::Player(owner.opponent()), 1),
    );
    assert_eq!(
        state
            .observation_journal
            .damage_public_owner_v3(damage, source),
        Some(owner)
    );
    propose_and_commit(state, ProposedEvent::zone_change(source, Zone::Library));
    (source, damage)
}

#[test]
fn v3_public_rollback_order_keeps_paid_refs_and_ignores_arena_order_both_seats() {
    for viewer in [PlayerId::P0, PlayerId::P1] {
        let mut outputs = vec![];
        for reversed in [false, true] {
            let (mut state, lists) = fixture(reversed);
            let paid = graveyard_rollback(&mut state, PlayerId::P0);
            graveyard_rollback(&mut state, PlayerId::P1);
            let mut item = waiting(paid, PlayerId::P0);
            item.discarded.push(paid);
            item.v4.paid_cost_refs.push(PaidCostRefV4 {
                object: paid,
                card_def: state.objects.get(paid).card_def,
                owner: PlayerId::P0,
                controller: PlayerId::P0,
                zone: Zone::Graveyard,
                zone_change_count: 2,
                visible_to_mask: 3,
                power_lki: None,
            });
            state.stack.push(item);
            let source = input(&state, &lists, viewer);
            for old in [
                export_v1(&source, ExportOptionsV1::default()),
                export_v2(&source),
            ] {
                assert!(
                    matches!(old, Err(ExportErrorV1::Refused { code: RefusalCodeV1::R2, detail })
                    if detail == super::export::R2_ORDER_DETAIL_V1)
                );
            }
            let out = export_v3(&source).unwrap();
            assert_eq!(out.state.stack[0].discarded.len(), 1);
            assert_eq!(out.state.stack[0].v4.paid_cost_refs.len(), 1);
            assert_eq!(out.schema, INFOSET_SCHEMA_V3);
            outputs.push(out.bytes);
        }
        assert_eq!(outputs[0], outputs[1]);
    }
}

#[test]
fn v3_same_owner_rollbacks_still_refuse_and_recover_after_a_visible_move() {
    for viewer in [PlayerId::P0, PlayerId::P1] {
        let (mut state, lists) = fixture(false);
        let first = graveyard_rollback(&mut state, PlayerId::P0);
        graveyard_rollback(&mut state, PlayerId::P0);
        assert!(matches!(
            export_v3(&input(&state, &lists, viewer)),
            Err(ExportErrorV1::Refused {
                code: RefusalCodeV1::R2,
                ..
            })
        ));
        propose_and_commit(&mut state, ProposedEvent::zone_change(first, Zone::Library));
        assert!(matches!(
            engine::advance_until_decision(&mut state),
            Decision::CastSpellOrPass { .. }
        ));
        export_v3(&input(&state, &lists, viewer)).unwrap();
    }
}

#[test]
fn v3_unattributed_damage_refuses_across_hidden_owners_and_private_knowledge() {
    for viewer in [PlayerId::P0, PlayerId::P1] {
        let (base, lists) = fixture(false);
        let mut outputs = vec![];
        for owner in [PlayerId::P0, PlayerId::P1] {
            for index in 0..2 {
                let mut state = base.clone();
                let source = state.players[owner.index()].library[index];
                // Private knowledge differs, including whether it names the
                // damage's native source. Neither difference reaches V.
                state.reveal_library_position(
                    viewer.opponent(),
                    viewer.opponent(),
                    if index == 0 { 0 } else { 2 },
                );
                propose_and_commit(
                    &mut state,
                    ProposedEvent::damage(source, Target::Player(viewer), 1),
                );
                assert_eq!(
                    state.observation_journal.damage_public_owner_v3(0, source),
                    None
                );
                assert!(matches!(
                    export_v2(&input(&state, &lists, viewer)),
                    Err(ExportErrorV1::Refused {
                        code: RefusalCodeV1::R2,
                        ..
                    })
                ));
                let error = export_v3(&input(&state, &lists, viewer)).unwrap_err();
                assert!(
                    matches!(&error, ExportErrorV1::Refused { code: RefusalCodeV1::R2, detail }
                if detail == "unobserved Damage owner has no public event-time proof")
                );
                outputs.push(error);
            }
        }
        assert!(outputs.iter().all(|error| *error == outputs[0]));
    }
}

#[test]
fn v3_damage_owner_receipt_rejects_nominal_public_zone_without_correspondence() {
    for owner in [PlayerId::P0, PlayerId::P1] {
        for zone in [Zone::Battlefield, Zone::Graveyard, Zone::Exile, Zone::Stack] {
            let (mut state, _) = fixture(false);
            let source = state.players[owner.index()].library[0];
            // A stale zone marker alone cannot establish the actual publicly
            // indexed source. These intentionally malformed history fixtures
            // exercise receipt capture, not engine scene admission.
            state.objects.get_mut(source).zone = zone;
            propose_and_commit(
                &mut state,
                ProposedEvent::damage(source, Target::Player(owner.opponent()), 1),
            );
            assert_eq!(
                state.observation_journal.damage_public_owner_v3(0, source),
                None
            );
        }
    }
}

#[test]
fn v3_damage_refusal_recovers_when_the_public_operation_finishes() {
    for viewer in [PlayerId::P0, PlayerId::P1] {
        let (mut state, lists) = fixture(false);
        let (departed, damage) = departed_public_damage(&mut state, viewer.opponent());
        propose_and_commit(&mut state, ProposedEvent::draw(viewer));
        let public = state.players[viewer.index()].hand[0];
        propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(public, Zone::Graveyard),
        );
        state.stack.push(waiting(public, viewer));
        let history = state.engine.event_history.len();
        assert!(matches!(
            export_v3(&input(&state, &lists, viewer)),
            Err(ExportErrorV1::Refused {
                code: RefusalCodeV1::R2,
                ..
            })
        ));
        for _ in 0..4 {
            assert!(matches!(
                engine::advance_until_decision(&mut state),
                Decision::CastSpellOrPass { .. }
            ));
            if state.stack.is_empty() {
                break;
            }
            engine::step(&mut state, Action::Pass).unwrap();
        }
        assert!(state.stack.is_empty());
        assert!(state.engine.event_history.len() >= history);
        let out = export_v3(&input(&state, &lists, viewer)).unwrap();
        let CommittedEvent::Damage { source, .. } = out.state.engine.event_history[damage] else {
            panic!("damage");
        };
        assert!(matches!(
            out.handles[source.0 as usize],
            HandleV1::H { named: false, .. }
        ));
        assert_eq!(out.state.objects.get(source).owner, viewer.opponent());
        assert_eq!(state.objects.get(departed).zone, Zone::Library);
        assert!(export_v2(&input(&state, &lists, viewer)).is_err());
    }
}

#[test]
fn v3_damage_retains_current_and_historical_public_identity() {
    for viewer in [PlayerId::P0, PlayerId::P1] {
        let (mut state, lists) = fixture(false);
        let owner = viewer.opponent();
        propose_and_commit(&mut state, ProposedEvent::draw(owner));
        let public = state.players[owner.index()].hand[0];
        propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(public, Zone::Graveyard),
        );
        let damage = state.engine.event_history.len();
        state.engine.event_history.push(CommittedEvent::Damage {
            source: public,
            target: Target::Player(viewer),
            amount: 1,
        });
        let current = export_v3(&input(&state, &lists, viewer)).unwrap();
        let CommittedEvent::Damage { source, .. } = current.state.engine.event_history[damage]
        else {
            panic!("damage")
        };
        assert!(matches!(
            current.handles[source.0 as usize],
            HandleV1::C { .. }
        ));
        propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(public, Zone::Library),
        );
        let history = export_v3(&input(&state, &lists, viewer)).unwrap();
        let CommittedEvent::Damage { source, .. } = history.state.engine.event_history[damage]
        else {
            panic!("damage")
        };
        assert!(matches!(
            history.handles[source.0 as usize],
            HandleV1::H { named: true, .. }
        ));
        assert_eq!(
            history.state.objects.get(source).card_def,
            super::fixtures::id("Fiery Temper")
        );
    }
}

#[test]
fn v3_damage_pool_boundary_is_public_and_does_not_limit_ordinary_exports() {
    let outsider = (0..crate::card_def::CARD_DEFS.len())
        .map(|i| i as u16)
        .find(|id| {
            !crate::runtime_decks::RUNTIME_DECKS
                .iter()
                .any(|deck| deck.card_ids.contains(id))
        })
        .expect("the catalog contains cards outside W1");
    let island = super::fixtures::id("Island");
    let lists = [vec![island, island], vec![outsider, island]];
    assert!(!super::export::damage_pool_audited_v3(
        [&lists[0], &lists[1]],
        crate::runtime_decks::RUNTIME_DECK_CATALOG_FILE_SHA256
    ));
    assert!(!super::export::damage_pool_audited_v3(
        [&[island], &[island]],
        "changed-catalog"
    ));
    for viewer in [PlayerId::P0, PlayerId::P1] {
        let mut state = GameState::new_from_libraries(
            &lists[0],
            &lists[1],
            |id| crate::card_def::CARD_DEFS[id as usize].name.into(),
            579,
        );
        let source = input(&state, &lists, viewer);
        let before = [
            export_v1(&source, ExportOptionsV1::default())
                .unwrap()
                .bytes,
            export_v2(&source).unwrap().bytes,
        ];
        let third = export_v3(&source).unwrap();
        assert_ne!(third.bytes, before[0]);
        assert_ne!(third.bytes, before[1]);
        assert_eq!(
            export_v1(&source, ExportOptionsV1::default())
                .unwrap()
                .bytes,
            before[0]
        );
        assert_eq!(export_v2(&source).unwrap().bytes, before[1]);
        for card in state.players[viewer.opponent().index()].library.clone() {
            state.engine.event_history.push(CommittedEvent::Damage {
                source: card,
                target: Target::Player(viewer),
                amount: 1,
            });
            assert!(matches!(
                export_v3(&input(&state, &lists, viewer)),
                Err(ExportErrorV1::Refused {
                    code: RefusalCodeV1::R2,
                    ..
                })
            ));
            state.engine.event_history.pop();
        }
    }
}
