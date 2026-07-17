//! Integration coverage for the generic `AnyPlayer` / `MillCards` substrate
//! through the two Pauper cards that use it.
//!
//! The bounded rules oracle is XMage commit
//! `0723fc0c2be922af47b0ef0539f28114cc23b998`: `MentalNote.java` blob
//! `5250eae45785bc1b4b3c40a623d530bedddee1e5`, `ThoughtScour.java` blob
//! `c67088843cc2738f6165bbaf9a5687e36dde6e5b`,
//! `MillCardsControllerEffect.java` blob
//! `36eb567759cf01218cf2d79c2b22262f92462b32`, and
//! `MillCardsTargetEffect.java` blob
//! `25f32e64e2cc012c5d7c11c2c8d4478ae8f615cc`. Those sources establish
//! Mental Note's controller-only mill, Thought Scour's any-player target,
//! and the printed mill-then-draw order. `PlayerImpl.java` blob
//! `1fc883515410e4b6df6255e5be44547f82617784` is the ordering oracle for a
//! multi-card move into one graveyard.

use mtg_kernel::card_def::{card_id_by_name, CARD_DEFS};
use mtg_kernel::engine::{self, Action, Decision, UnsupportedMechanic};
use mtg_kernel::event::{self, CommittedEvent, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::rl::{
    legal_action_candidates_v1, observe_v2, ActionSemanticV1, PendingEffectChoiceSemanticV4,
    PlayerSeatV1, TargetRefV1, TargetSelectionPurposeV4,
};
use mtg_kernel::state::{Counters, GameObject, GameState, Step, Target, Zone};
use mtg_kernel::surface_v2::{HarnessSurfaceV2, SurfaceDecision};

fn card_id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name} in CARD_DEFS"))
}

fn card_name(card_def: u16) -> String {
    CARD_DEFS[card_def as usize].name.to_string()
}

fn put_object(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id(name);
    let id = state.objects.push(GameObject {
        card_def,
        name: name.to_string(),
        owner: player,
        controller: player,
        zone,
        tapped: false,
        summoning_sick: false,
        damage: 0,
        counters: Counters::default(),
        attachments: Vec::new(),
        v4: mtg_kernel::state::ObjectStateV4::from_card_def(card_def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    match zone {
        Zone::Hand => state.players[player.index()].hand.push(id),
        Zone::Battlefield => state.players[player.index()].battlefield.push(id),
        Zone::Library => state.players[player.index()].library.push(id),
        Zone::Graveyard => state.players[player.index()].graveyard.push(id),
        Zone::Exile => state.exile.push(id),
        Zone::Command => state.command.push(id),
        Zone::Stack => panic!("test helper does not create stack objects"),
    }
    id
}

fn ready_spell(
    spell_name: &str,
    p0_library_names: &[&str],
    p1_library_names: &[&str],
) -> (GameState, ObjectId, [Vec<ObjectId>; 2]) {
    let p0_defs = p0_library_names
        .iter()
        .map(|name| card_id(name))
        .collect::<Vec<_>>();
    let p1_defs = p1_library_names
        .iter()
        .map(|name| card_id(name))
        .collect::<Vec<_>>();
    let mut state = GameState::new_from_libraries(&p0_defs, &p1_defs, card_name, 0x4D49_4C4C);
    state.step = Step::Main1;
    state.active_player = PlayerId::P0;
    state.priority_player = PlayerId::P0;
    put_object(&mut state, PlayerId::P0, "Island", Zone::Battlefield);
    let spell = put_object(&mut state, PlayerId::P0, spell_name, Zone::Hand);
    let libraries = [
        state.players[0].library.clone(),
        state.players[1].library.clone(),
    ];
    (state, spell, libraries)
}

fn thought_target_decision(state: &mut GameState, thought_scour: ObjectId) -> Decision {
    engine::step(state, Action::CastSpell(thought_scour)).unwrap();
    let decision = engine::advance_until_decision(state);
    assert!(matches!(
        decision,
        Decision::ChooseTargets {
            player: PlayerId::P0,
            spell,
            remaining: 1,
            ..
        } if spell == thought_scour
    ));
    decision
}

fn advance_to_mill_order(state: &mut GameState, source: ObjectId) -> Decision {
    for _ in 0..12 {
        let decision = engine::advance_until_decision(state);
        match decision {
            Decision::ChooseEffectTargets {
                source: decision_source,
                ..
            } if decision_source == source => return decision,
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => panic!("unexpected decision before mill ordering: {other:?}"),
        }
    }
    panic!("mill effect did not reach its graveyard-order choice")
}

fn resolve_without_or_after_order(
    state: &mut GameState,
    source: ObjectId,
    first_milled_card: Option<ObjectId>,
) {
    for _ in 0..12 {
        let decision = engine::advance_until_decision(state);
        match decision {
            Decision::ChooseEffectTargets {
                source: decision_source,
                ..
            } if decision_source == source => {
                let first = first_milled_card.expect("an ordering choice was not expected");
                engine::step(state, Action::ChooseEffectTarget(Target::Object(first))).unwrap();
            }
            Decision::CastSpellOrPass { .. }
            | Decision::GameOver { .. }
            | Decision::Halted { .. }
                if state.engine.pending_effect.is_none()
                    && !state.stack.iter().any(|item| item.source == source) =>
            {
                return;
            }
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => panic!("unexpected decision while resolving mill spell: {other:?}"),
        }
    }
    panic!("mill spell did not finish")
}

fn target_actions(state: &GameState, decision: &Decision) -> Vec<(PlayerSeatV1, String)> {
    legal_action_candidates_v1(&SurfaceDecision::Decision(decision.clone()), state)
        .unwrap()
        .into_iter()
        .map(|candidate| match candidate.record.semantic {
            ActionSemanticV1::ChooseTarget {
                target: TargetRefV1::Player { player },
                remaining: 1,
                ..
            } => (player, candidate.record.stable_id),
            other => panic!("unexpected Thought Scour target action: {other:?}"),
        })
        .collect()
}

fn mill_order_actions(state: &GameState, decision: &Decision) -> Vec<(ObjectId, String)> {
    legal_action_candidates_v1(&SurfaceDecision::Decision(decision.clone()), state)
        .unwrap()
        .into_iter()
        .map(|candidate| match candidate.record.semantic {
            ActionSemanticV1::ChooseEffectTarget {
                target: TargetRefV1::Object { object },
                selected_count: 0,
                min_targets: 2,
                max_targets: 2,
                ..
            } => (ObjectId(object.arena_id), candidate.record.stable_id),
            other => panic!("unexpected mill-order action: {other:?}"),
        })
        .collect()
}

#[test]
fn thought_scour_any_player_targets_are_exact_ordered_stable_and_restorable() {
    let (mut state, thought, _) = ready_spell(
        "Thought Scour",
        &["Lightning Bolt", "Mountain", "Fireblast"],
        &["Fiery Temper", "Lava Dart", "Highway Robbery"],
    );
    let decision = thought_target_decision(&mut state, thought);
    let Decision::ChooseTargets { legal_targets, .. } = &decision else {
        unreachable!()
    };
    assert_eq!(
        legal_targets,
        &vec![Target::Player(PlayerId::P0), Target::Player(PlayerId::P1)],
        "AnyPlayer is exactly P0 then P1, never a permanent"
    );

    let actions = target_actions(&state, &decision);
    assert_eq!(
        actions,
        vec![
            (
                PlayerSeatV1::P0,
                "legal-action-v4:a3b66ae5a0cc9c81".to_string(),
            ),
            (
                PlayerSeatV1::P1,
                "legal-action-v4:2573b4dd5d6c9ed0".to_string(),
            ),
        ]
    );
    let snapshot = state.snapshot();
    let snapshot_hash = state.state_hash();
    engine::step(
        &mut state,
        Action::ChooseTarget(Target::Player(PlayerId::P1)),
    )
    .unwrap();

    let after_target = state.clone();
    assert!(engine::step(
        &mut state,
        Action::ChooseTarget(Target::Player(PlayerId::P0))
    )
    .is_err());
    assert_eq!(
        state, after_target,
        "a stale target action must not mutate state"
    );

    state.restore(&snapshot);
    assert_eq!(state.state_hash(), snapshot_hash);
    let restored = engine::advance_until_decision(&mut state);
    assert_eq!(target_actions(&state, &restored), actions);
}

#[test]
fn thought_scour_opponent_mill_is_owner_private_then_public_and_precedes_draw() {
    let (mut state, thought, libraries) = ready_spell(
        "Thought Scour",
        &["Lightning Bolt", "Mountain", "Fireblast"],
        &["Fiery Temper", "Lava Dart", "Highway Robbery"],
    );
    let p0_draw = libraries[0][0];
    let milled = [libraries[1][0], libraries[1][1]];

    for observer in [PlayerId::P0, PlayerId::P1] {
        let before = observe_v2(&state, &HarnessSurfaceV2::new(), observer, 0).unwrap();
        assert!(before.known_library_cards.iter().all(Vec::is_empty));
        let json = serde_json::to_string(&before).unwrap();
        assert!(!json.contains("Fiery Temper"));
        assert!(!json.contains("Lava Dart"));
    }

    let target = thought_target_decision(&mut state, thought);
    assert_eq!(
        target_actions(&state, &target)
            .into_iter()
            .map(|(player, _)| player)
            .collect::<Vec<_>>(),
        vec![PlayerSeatV1::P0, PlayerSeatV1::P1]
    );
    engine::step(
        &mut state,
        Action::ChooseTarget(Target::Player(PlayerId::P1)),
    )
    .unwrap();
    let order = advance_to_mill_order(&mut state, thought);
    assert!(matches!(
        order,
        Decision::ChooseEffectTargets {
            player: PlayerId::P1,
            source,
            selected_count: 0,
            min_targets: 2,
            max_targets: 2,
            can_finish: false,
            ref legal_targets,
        } if source == thought
            && legal_targets == &vec![Target::Object(milled[0]), Target::Object(milled[1])]
    ));

    assert!(
        state.players[0].hand.is_empty(),
        "draw waits for mill ordering"
    );
    assert_eq!(state.players[0].library, libraries[0]);
    assert_eq!(state.players[1].library, libraries[1]);
    assert!(state.players[1].graveyard.is_empty());
    let owner_view = observe_v2(&state, &HarnessSurfaceV2::new(), PlayerId::P1, 1).unwrap();
    let controller_view = observe_v2(&state, &HarnessSurfaceV2::new(), PlayerId::P0, 1).unwrap();
    let owner_json = serde_json::to_string(&owner_view).unwrap();
    let controller_json = serde_json::to_string(&controller_view).unwrap();
    for name in ["Fiery Temper", "Lava Dart"] {
        assert!(owner_json.contains(name), "the ordering player sees {name}");
        assert!(
            !controller_json.contains(name),
            "the non-owner must not see a still-library {name}"
        );
    }
    assert!(matches!(
        owner_view
            .projection
            .engine_context
            .pending_effect
            .as_ref()
            .unwrap()
            .choice
            .as_ref()
            .unwrap(),
        PendingEffectChoiceSemanticV4::Targets {
            player: PlayerSeatV1::P1,
            structural_path,
            selected_targets,
            legal_targets,
            min_targets: 2,
            max_targets: 2,
            can_finish: false,
            ordered: true,
            purpose: TargetSelectionPurposeV4::CardSelection,
        } if structural_path == &vec![0]
            && selected_targets.is_empty()
            && legal_targets.len() == 2
    ));
    assert!(matches!(
        controller_view
            .projection
            .engine_context
            .pending_effect
            .as_ref()
            .unwrap()
            .choice
            .as_ref()
            .unwrap(),
        PendingEffectChoiceSemanticV4::Targets {
            player: PlayerSeatV1::P1,
            selected_targets,
            legal_targets,
            min_targets: 2,
            max_targets: 2,
            can_finish: false,
            ordered: true,
            purpose: TargetSelectionPurposeV4::CardSelection,
            ..
        } if selected_targets.is_empty() && legal_targets.is_empty()
    ));

    let order_actions = mill_order_actions(&state, &order);
    assert_eq!(
        order_actions,
        vec![
            (milled[0], "legal-action-v4:bc791a0ff723df1d".to_string(),),
            (milled[1], "legal-action-v4:151b5f4df227ba8e".to_string(),),
        ]
    );
    let order_snapshot = state.snapshot();
    let order_hash = state.state_hash();
    let history_start = state.engine.event_history.len();
    engine::step(
        &mut state,
        Action::ChooseEffectTarget(Target::Object(milled[1])),
    )
    .unwrap();
    assert!(state
        .engine
        .pending_effect
        .as_ref()
        .unwrap()
        .choice
        .is_none());
    assert!(state.players[1].graveyard.is_empty());
    assert!(state.players[0].hand.is_empty());

    let completed_choice = state.clone();
    assert!(engine::step(
        &mut state,
        Action::ChooseEffectTarget(Target::Object(milled[0]))
    )
    .is_err());
    assert_eq!(state, completed_choice, "a consumed order action is stale");
    engine::advance_until_decision(&mut state);

    assert_eq!(state.players[1].graveyard, vec![milled[1], milled[0]]);
    assert_eq!(state.players[1].library, vec![libraries[1][2]]);
    assert_eq!(state.players[0].hand, vec![p0_draw]);
    assert_eq!(
        state.players[0].library,
        vec![libraries[0][1], libraries[0][2]]
    );
    assert_eq!(state.players[0].draws_this_turn, 1);
    assert!(!state.players[0].drew_from_empty);
    assert!(!state.players[1].drew_from_empty, "mill is not a draw");
    assert_eq!(state.objects.get(thought).zone, Zone::Graveyard);

    assert_eq!(
        &state.engine.event_history[history_start..],
        &[
            CommittedEvent::ZoneChange {
                object: milled[1],
                from: Zone::Library,
                to: Zone::Graveyard,
                controller_before: PlayerId::P1,
            },
            CommittedEvent::ZoneChange {
                object: milled[0],
                from: Zone::Library,
                to: Zone::Graveyard,
                controller_before: PlayerId::P1,
            },
            CommittedEvent::Draw {
                player: PlayerId::P0,
                object: Some(p0_draw),
            },
            CommittedEvent::ZoneChange {
                object: thought,
                from: Zone::Stack,
                to: Zone::Graveyard,
                controller_before: PlayerId::P0,
            },
        ],
        "the mill batch commits in owner order before the controller draws"
    );
    for observer in [PlayerId::P0, PlayerId::P1] {
        let after = observe_v2(&state, &HarnessSurfaceV2::new(), observer, 2).unwrap();
        assert_eq!(
            after.projection.graveyards[1]
                .iter()
                .map(|card| card.card_name.as_str())
                .collect::<Vec<_>>(),
            vec!["Lava Dart", "Fiery Temper"]
        );
    }

    let completed = state.clone();
    let completed_hash = state.state_hash();
    state.restore(&order_snapshot);
    assert_eq!(state.state_hash(), order_hash);
    let restored_order = engine::advance_until_decision(&mut state);
    assert_eq!(mill_order_actions(&state, &restored_order), order_actions);
    engine::step(
        &mut state,
        Action::ChooseEffectTarget(Target::Object(milled[1])),
    )
    .unwrap();
    engine::advance_until_decision(&mut state);
    assert_eq!(state.state_hash(), completed_hash);
    assert_eq!(state, completed);
}

#[test]
fn mill_preserves_prior_opponent_library_knowledge_and_shifts_the_remaining_card() {
    let (mut state, thought, libraries) = ready_spell(
        "Thought Scour",
        &["Lightning Bolt"],
        &["Fiery Temper", "Lava Dart", "Highway Robbery", "Mountain"],
    );
    state.reveal_library_top(PlayerId::P0, PlayerId::P1, 3);
    assert_eq!(
        state
            .known_library_cards(PlayerId::P0, PlayerId::P1)
            .iter()
            .map(|entry| (entry.position, entry.object))
            .collect::<Vec<_>>(),
        vec![
            (0, libraries[1][0]),
            (1, libraries[1][1]),
            (2, libraries[1][2]),
        ]
    );

    thought_target_decision(&mut state, thought);
    engine::step(
        &mut state,
        Action::ChooseTarget(Target::Player(PlayerId::P1)),
    )
    .unwrap();
    let order = advance_to_mill_order(&mut state, thought);
    assert!(matches!(
        order,
        Decision::ChooseEffectTargets {
            player: PlayerId::P1,
            ..
        }
    ));
    assert_eq!(
        state
            .known_library_cards(PlayerId::P0, PlayerId::P1)
            .iter()
            .map(|entry| (entry.position, entry.object))
            .collect::<Vec<_>>(),
        vec![
            (0, libraries[1][0]),
            (1, libraries[1][1]),
            (2, libraries[1][2]),
        ],
        "a private ordering prompt must not erase identities the opponent already knew"
    );

    engine::step(
        &mut state,
        Action::ChooseEffectTarget(Target::Object(libraries[1][0])),
    )
    .unwrap();
    engine::advance_until_decision(&mut state);
    assert_eq!(
        state.players[1].library,
        vec![libraries[1][2], libraries[1][3]]
    );
    assert_eq!(
        state
            .known_library_cards(PlayerId::P0, PlayerId::P1)
            .iter()
            .map(|entry| (entry.position, entry.object))
            .collect::<Vec<_>>(),
        vec![(0, libraries[1][2])],
        "the known third card becomes the known top card after two cards are milled"
    );
    assert!(state
        .known_library_cards(PlayerId::P1, PlayerId::P1)
        .is_empty());
}

#[test]
fn mental_note_and_thought_scour_cover_both_targets_and_both_owner_orders() {
    for first_index in [0_usize, 1] {
        let (mut state, mental, libraries) = ready_spell(
            "Mental Note",
            &["Fiery Temper", "Lava Dart", "Lightning Bolt"],
            &[],
        );
        engine::step(&mut state, Action::CastSpell(mental)).unwrap();
        let order = advance_to_mill_order(&mut state, mental);
        assert!(matches!(
            order,
            Decision::ChooseEffectTargets {
                player: PlayerId::P0,
                ..
            }
        ));
        let actions = mill_order_actions(&state, &order);
        if first_index == 0 {
            assert_eq!(
                actions,
                vec![
                    (
                        libraries[0][0],
                        "legal-action-v4:c9f2931441e6c31b".to_string(),
                    ),
                    (
                        libraries[0][1],
                        "legal-action-v4:3fd2543d61027a2a".to_string(),
                    ),
                ]
            );
        }
        engine::step(
            &mut state,
            Action::ChooseEffectTarget(Target::Object(libraries[0][first_index])),
        )
        .unwrap();
        engine::advance_until_decision(&mut state);
        assert_eq!(
            state.players[0].graveyard,
            vec![
                libraries[0][first_index],
                libraries[0][1 - first_index],
                mental,
            ]
        );
        assert_eq!(state.players[0].hand, vec![libraries[0][2]]);
    }

    for target in [PlayerId::P0, PlayerId::P1] {
        for first_index in [0_usize, 1] {
            let (mut state, thought, libraries) = ready_spell(
                "Thought Scour",
                &["Lightning Bolt", "Mountain", "Fireblast"],
                &["Fiery Temper", "Lava Dart", "Highway Robbery"],
            );
            thought_target_decision(&mut state, thought);
            engine::step(&mut state, Action::ChooseTarget(Target::Player(target))).unwrap();
            let order = advance_to_mill_order(&mut state, thought);
            assert!(matches!(
                order,
                Decision::ChooseEffectTargets { player, .. } if player == target
            ));
            let target_library = &libraries[target.index()];
            engine::step(
                &mut state,
                Action::ChooseEffectTarget(Target::Object(target_library[first_index])),
            )
            .unwrap();
            engine::advance_until_decision(&mut state);
            assert_eq!(
                &state.players[target.index()].graveyard[..2],
                &[target_library[first_index], target_library[1 - first_index],]
            );
            let expected_draw = if target == PlayerId::P0 {
                libraries[0][2]
            } else {
                libraries[0][0]
            };
            assert_eq!(state.players[0].hand, vec![expected_draw]);
            assert_eq!(state.objects.get(expected_draw).zone, Zone::Hand);
        }
    }
}

#[test]
fn mill_cards_short_libraries_zero_one_two_are_bounded_and_not_draws() {
    const SHORT: [&str; 2] = ["Fiery Temper", "Lava Dart"];
    for count in 0..=2 {
        let (mut state, mental, libraries) = ready_spell("Mental Note", &SHORT[..count], &[]);
        engine::step(&mut state, Action::CastSpell(mental)).unwrap();
        resolve_without_or_after_order(&mut state, mental, (count == 2).then(|| libraries[0][0]));
        assert!(state.players[0].library.is_empty());
        assert!(state.players[0].hand.is_empty());
        assert_eq!(
            state.players[0].draws_this_turn, 0,
            "only successful draws increment the counter"
        );
        assert!(state.players[0].drew_from_empty);
        assert!(state.engine.event_history.iter().any(|event| matches!(
            event,
            CommittedEvent::Draw {
                player: PlayerId::P0,
                object: None,
            }
        )));
        assert_eq!(
            &state.players[0].graveyard[..count],
            libraries[0].as_slice()
        );
        assert_eq!(state.players[0].graveyard.last(), Some(&mental));
    }

    for count in 0..=2 {
        let (mut state, thought, libraries) =
            ready_spell("Thought Scour", &["Lightning Bolt"], &SHORT[..count]);
        let draw = libraries[0][0];
        thought_target_decision(&mut state, thought);
        engine::step(
            &mut state,
            Action::ChooseTarget(Target::Player(PlayerId::P1)),
        )
        .unwrap();
        resolve_without_or_after_order(&mut state, thought, (count == 2).then(|| libraries[1][0]));
        assert_eq!(state.players[0].hand, vec![draw]);
        assert_eq!(state.players[0].draws_this_turn, 1);
        assert!(!state.players[0].drew_from_empty);
        assert!(!state.players[1].drew_from_empty);
        assert!(state.players[1].library.is_empty());
        assert_eq!(state.players[1].graveyard, libraries[1]);
    }
}

#[test]
fn mill_order_rejects_a_stale_library_incarnation_without_partial_effects() {
    let (mut state, thought, libraries) = ready_spell(
        "Thought Scour",
        &["Lightning Bolt"],
        &["Fiery Temper", "Lava Dart", "Highway Robbery"],
    );
    thought_target_decision(&mut state, thought);
    engine::step(
        &mut state,
        Action::ChooseTarget(Target::Player(PlayerId::P1)),
    )
    .unwrap();
    assert!(matches!(
        advance_to_mill_order(&mut state, thought),
        Decision::ChooseEffectTargets {
            player: PlayerId::P1,
            ..
        }
    ));
    let stale = libraries[1][0];
    let pending = state.engine.pending_effect.clone();

    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(stale, Zone::Graveyard),
    );
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(stale, Zone::Library));
    state.engine.pending_effect = pending;
    assert_eq!(state.objects.get(stale).zone_change_count, 2);

    let before = state.clone();
    assert!(engine::step(
        &mut state,
        Action::ChooseEffectTarget(Target::Object(stale))
    )
    .is_err());
    assert_eq!(state, before);
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::Halted {
            mechanic: UnsupportedMechanic::InvalidEffectContinuation,
            source,
        } if source == thought
    ));
    assert!(
        state.players[0].hand.is_empty(),
        "draw must not partially run"
    );
    assert!(state.players[1].graveyard.is_empty());
    assert_eq!(state.players[1].library.len(), 3);
}

#[test]
fn mill_order_fails_closed_before_observation_on_tampered_chooser() {
    let (mut state, thought, _) = ready_spell(
        "Thought Scour",
        &["Lightning Bolt"],
        &["Fiery Temper", "Lava Dart", "Highway Robbery"],
    );
    thought_target_decision(&mut state, thought);
    engine::step(
        &mut state,
        Action::ChooseTarget(Target::Player(PlayerId::P1)),
    )
    .unwrap();
    assert!(matches!(
        advance_to_mill_order(&mut state, thought),
        Decision::ChooseEffectTargets {
            player: PlayerId::P1,
            ..
        }
    ));

    let choice = state
        .engine
        .pending_effect
        .as_mut()
        .unwrap()
        .choice
        .as_mut()
        .unwrap();
    let mtg_kernel::effect::PendingEffectChoice::SelectTargets { player, .. } = choice else {
        panic!("Thought Scour must be waiting for its mill-order target choice")
    };
    *player = PlayerId::P0;

    assert!(
        observe_v2(&state, &HarnessSurfaceV2::new(), PlayerId::P0, 0).is_err(),
        "an invalid chooser/library pairing must not expose private mill candidates"
    );
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::Halted {
            mechanic: UnsupportedMechanic::InvalidEffectContinuation,
            source,
        } if source == thought
    ));
    assert!(state.players[0].hand.is_empty());
    assert!(state.players[1].graveyard.is_empty());
    assert_eq!(state.players[1].library.len(), 3);
}

#[test]
fn a_countered_thought_scour_does_not_mill_or_draw() {
    let (mut state, thought, libraries) = ready_spell(
        "Thought Scour",
        &["Lightning Bolt", "Mountain", "Fireblast"],
        &["Fiery Temper", "Lava Dart", "Highway Robbery"],
    );
    put_object(&mut state, PlayerId::P1, "Island", Zone::Battlefield);
    put_object(&mut state, PlayerId::P1, "Island", Zone::Battlefield);
    let counterspell = put_object(&mut state, PlayerId::P1, "Counterspell", Zone::Hand);

    thought_target_decision(&mut state, thought);
    engine::step(
        &mut state,
        Action::ChooseTarget(Target::Player(PlayerId::P1)),
    )
    .unwrap();
    let mut counter_announced = false;
    for _ in 0..16 {
        let decision = engine::advance_until_decision(&mut state);
        if state.objects.get(thought).zone == Zone::Graveyard
            && state.objects.get(counterspell).zone == Zone::Graveyard
        {
            break;
        }
        match decision {
            Decision::CastSpellOrPass {
                player: PlayerId::P1,
                ref castable_spells,
                ..
            } if !counter_announced => {
                assert!(castable_spells.contains(&counterspell));
                engine::step(&mut state, Action::CastSpell(counterspell)).unwrap();
                counter_announced = true;
            }
            Decision::ChooseTargets {
                player: PlayerId::P1,
                spell,
                ref legal_targets,
                ..
            } if spell == counterspell => {
                assert!(legal_targets.contains(&Target::Object(thought)));
                engine::step(&mut state, Action::ChooseTarget(Target::Object(thought))).unwrap();
            }
            Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
            other => panic!("unexpected Counterspell decision: {other:?}"),
        }
    }

    assert!(counter_announced);
    assert_eq!(state.players[0].library, libraries[0]);
    assert_eq!(state.players[1].library, libraries[1]);
    assert!(state.players[0].hand.is_empty());
    assert_eq!(state.players[0].draws_this_turn, 0);
    assert_eq!(state.players[1].draws_this_turn, 0);
    assert!(state.engine.pending_effect.is_none());
    assert_eq!(state.objects.get(thought).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(counterspell).zone, Zone::Graveyard);
}
