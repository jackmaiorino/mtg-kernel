//! Integration coverage for Ponder's generic private library-order and
//! optional-shuffle substrate.
//!
//! The bounded rules oracle is XMage commit
//! `0723fc0c2be922af47b0ef0539f28114cc23b998`: `Ponder.java` blob
//! `7191766552c09ffdaa49eaf483f2c24bebadfb11`,
//! `LookLibraryControllerEffect.java` blob
//! `c6db2ed40dfdfd3bd33793934eb75dd8e0db24e2`,
//! `ShuffleLibrarySourceEffect.java` blob
//! `a6c18c2f4387f14eb29ffa870266b51a215e7818`, `PutCards.java` blob
//! `f2881ca500c2eb97197f338e78dad95dd97d6126`, and `PlayerImpl.java` blob
//! `1fc883515410e4b6df6255e5be44547f82617784`. The ordered-card prompt says
//! the last selected card is topmost: because XMage moves every explicit
//! pick to the top immediately, the first pick ends deepest and the forced
//! final card is topmost. XMage displays the optional shuffle as Yes then
//! No; schema-v4 intentionally retains its canonical semantic Boolean order,
//! false/No then true/Yes, without changing either outcome.

use mtg_kernel::card_def::{card_id_by_name, CARD_DEFS};
use mtg_kernel::effect::EffectBooleanChoicePurpose;
use mtg_kernel::engine::{self, Action, Decision, UnsupportedMechanic};
use mtg_kernel::event::{self, CommittedEvent, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::rl::{
    legal_action_candidates_v1, observe_v2, ActionSemanticV1, BooleanChoicePurposeV4,
    PendingEffectChoiceSemanticV4, PlayerSeatV1, TargetRefV1, TargetSelectionPurposeV4,
};
use mtg_kernel::state::{Counters, GameObject, GameState, Step, Target, Zone};
use mtg_kernel::surface_v2::{HarnessSurfaceV2, SurfaceDecision};

const LONG_LIBRARY: [&str; 7] = [
    "Fiery Temper",
    "Lava Dart",
    "Lightning Bolt",
    "Mountain",
    "Fireblast",
    "Counterspell",
    "Island",
];

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

fn ready_ponder(library_names: &[&str]) -> (GameState, ObjectId, Vec<ObjectId>) {
    let library_defs = library_names
        .iter()
        .map(|name| card_id(name))
        .collect::<Vec<_>>();
    let p1_library = [card_id("Snow-Covered Forest")];
    let mut state =
        GameState::new_from_libraries(&library_defs, &p1_library, card_name, 0x504F_4E44_4552);
    state.step = Step::Main1;
    state.active_player = PlayerId::P0;
    state.priority_player = PlayerId::P0;
    put_object(&mut state, PlayerId::P0, "Island", Zone::Battlefield);
    let ponder = put_object(&mut state, PlayerId::P0, "Ponder", Zone::Hand);
    let library = state.players[0].library.clone();
    (state, ponder, library)
}

fn cast_ponder(state: &mut GameState, ponder: ObjectId) {
    engine::step(state, Action::CastSpell(ponder)).unwrap();
    assert_eq!(state.objects.get(ponder).zone, Zone::Stack);
    assert!(state.stack.iter().any(|item| item.source == ponder));
}

fn next_ponder_choice(state: &mut GameState, ponder: ObjectId) -> Decision {
    for _ in 0..12 {
        let decision = engine::advance_until_decision(state);
        match decision {
            Decision::ChooseEffectTargets { source, .. }
            | Decision::ChooseEffectBoolean { source, .. }
                if source == ponder =>
            {
                return decision;
            }
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => panic!("unexpected decision before Ponder choice: {other:?}"),
        }
    }
    panic!("Ponder did not reach its effect choice")
}

fn immediate_ponder_choice(state: &mut GameState, ponder: ObjectId) -> Decision {
    let decision = engine::advance_until_decision(state);
    match decision {
        choice @ (Decision::ChooseEffectTargets { source, .. }
        | Decision::ChooseEffectBoolean { source, .. })
            if source == ponder =>
        {
            choice
        }
        other => panic!("Ponder resolution exposed a decision between effect choices: {other:?}"),
    }
}

fn reorder_actions(state: &GameState, decision: &Decision) -> Vec<(ObjectId, String)> {
    legal_action_candidates_v1(&SurfaceDecision::Decision(decision.clone()), state)
        .unwrap()
        .into_iter()
        .map(|candidate| match candidate.record.semantic {
            ActionSemanticV1::ChooseEffectTarget {
                target: TargetRefV1::Object { object },
                ..
            } => (ObjectId(object.arena_id), candidate.record.stable_id),
            other => panic!("unexpected Ponder reorder action: {other:?}"),
        })
        .collect()
}

fn boolean_actions(state: &GameState, decision: &Decision) -> Vec<(bool, String)> {
    legal_action_candidates_v1(&SurfaceDecision::Decision(decision.clone()), state)
        .unwrap()
        .into_iter()
        .map(|candidate| match candidate.record.semantic {
            ActionSemanticV1::ChooseEffectBoolean { value, .. } => {
                (value, candidate.record.stable_id)
            }
            other => panic!("unexpected Ponder Boolean action: {other:?}"),
        })
        .collect()
}

fn assert_reorder_decision(
    decision: &Decision,
    ponder: ObjectId,
    selected_count: u16,
    legal: &[ObjectId],
) {
    assert!(matches!(
        decision,
        Decision::ChooseEffectTargets {
            player: PlayerId::P0,
            source,
            selected_count: actual_selected,
            min_targets,
            max_targets,
            legal_targets,
            can_finish: false,
        } if *source == ponder
            && *actual_selected == selected_count
            && *min_targets == (selected_count + legal.len() as u16)
            && *max_targets == *min_targets
            && legal_targets == &legal.iter().copied().map(Target::Object).collect::<Vec<_>>()
    ));
}

fn assert_boolean_decision(decision: &Decision, ponder: ObjectId) {
    assert!(matches!(
        decision,
        Decision::ChooseEffectBoolean {
            player: PlayerId::P0,
            source,
            default: Some(false),
            purpose: EffectBooleanChoicePurpose::ShuffleLibrary {
                player: PlayerId::P0,
            },
        } if *source == ponder
    ));
}

fn finish_boolean(state: &mut GameState, ponder: ObjectId, shuffle: bool) -> Decision {
    engine::step(state, Action::ChooseEffectBoolean(shuffle)).unwrap();
    assert!(state
        .engine
        .pending_effect
        .as_ref()
        .unwrap()
        .choice
        .is_none());
    assert_eq!(state.objects.get(ponder).zone, Zone::Stack);
    engine::advance_until_decision(state)
}

fn choose_explicit_order(
    state: &mut GameState,
    ponder: ObjectId,
    explicit_order: &[ObjectId],
) -> Decision {
    for (selected_count, &object) in explicit_order.iter().enumerate() {
        let decision = if selected_count == 0 {
            next_ponder_choice(state, ponder)
        } else {
            immediate_ponder_choice(state, ponder)
        };
        assert!(matches!(
            decision,
            Decision::ChooseEffectTargets {
                selected_count: actual,
                ..
            } if actual == selected_count as u16
        ));
        engine::step(state, Action::ChooseEffectTarget(Target::Object(object))).unwrap();
    }
    let boolean = if explicit_order.is_empty() {
        next_ponder_choice(state, ponder)
    } else {
        immediate_ponder_choice(state, ponder)
    };
    assert_boolean_decision(&boolean, ponder);
    boolean
}

#[test]
fn ponder_no_shuffle_golden_is_deepest_first_atomic_stable_and_restorable() {
    // XMage golden: initial top A,B,C,D; explicitly pick A then C. Each pick
    // is moved to the top, then the implicit B is moved last/topmost. Thus the
    // pre-draw order is B,C,A,D and declining shuffle draws B, leaving C,A,D.
    let (mut state, ponder, library) = ready_ponder(&LONG_LIBRARY[..4]);
    let [a, b, c, d] = library.as_slice() else {
        unreachable!()
    };
    cast_ponder(&mut state, ponder);
    let first = next_ponder_choice(&mut state, ponder);
    assert_reorder_decision(&first, ponder, 0, &[*a, *b, *c]);
    let first_actions = reorder_actions(&state, &first);
    assert_eq!(
        first_actions,
        vec![
            (*a, "legal-action-v4:ba96c6d5b55231b7".to_string()),
            (*b, "legal-action-v4:8730f4004967e0d6".to_string()),
            (*c, "legal-action-v4:e3c112eb71b44b30".to_string()),
        ]
    );
    let reorder_snapshot = state.snapshot();
    let reorder_hash = state.state_hash();

    engine::step(&mut state, Action::ChooseEffectTarget(Target::Object(*a))).unwrap();
    let after_first = state.clone();
    assert!(engine::step(&mut state, Action::ChooseEffectTarget(Target::Object(*a))).is_err());
    assert_eq!(state, after_first, "a consumed reorder action is stale");
    let second = immediate_ponder_choice(&mut state, ponder);
    assert_reorder_decision(&second, ponder, 1, &[*b, *c]);
    engine::step(&mut state, Action::ChooseEffectTarget(Target::Object(*c))).unwrap();
    assert_eq!(
        state.players[0].library, library,
        "answering the final pick only records the continuation"
    );
    assert!(state.players[0].hand.is_empty());
    assert!(state
        .engine
        .pending_effect
        .as_ref()
        .unwrap()
        .choice
        .is_none());

    let completed_reorder = state.clone();
    assert!(engine::step(&mut state, Action::ChooseEffectTarget(Target::Object(*b))).is_err());
    assert_eq!(state, completed_reorder);
    let boolean = immediate_ponder_choice(&mut state, ponder);
    assert_boolean_decision(&boolean, ponder);
    assert_eq!(state.players[0].library, vec![*b, *c, *a, *d]);
    assert!(state.players[0].hand.is_empty());
    assert_eq!(state.objects.get(ponder).zone, Zone::Stack);
    let bool_actions = boolean_actions(&state, &boolean);
    assert_eq!(
        bool_actions,
        vec![
            (false, "legal-action-v4:67f6686fd7011996".to_string()),
            (true, "legal-action-v4:f3b82f02dbf89fd3".to_string()),
        ]
    );
    let boolean_snapshot = state.snapshot();
    let boolean_hash = state.state_hash();
    let history_start = state.engine.event_history.len();

    engine::step(&mut state, Action::ChooseEffectBoolean(false)).unwrap();
    assert!(state.players[0].hand.is_empty());
    assert_eq!(state.players[0].library, vec![*b, *c, *a, *d]);
    let answered_boolean = state.clone();
    assert!(engine::step(&mut state, Action::ChooseEffectBoolean(true)).is_err());
    assert_eq!(
        state, answered_boolean,
        "an answered Boolean action is stale"
    );
    let final_decision = engine::advance_until_decision(&mut state);
    assert!(matches!(final_decision, Decision::CastSpellOrPass { .. }));
    assert_eq!(state.players[0].hand, vec![*b]);
    assert_eq!(state.players[0].library, vec![*c, *a, *d]);
    assert_eq!(state.players[0].draws_this_turn, 1);
    assert!(!state.players[0].drew_from_empty);
    assert_eq!(state.objects.get(ponder).zone, Zone::Graveyard);
    assert_eq!(
        &state.engine.event_history[history_start..],
        &[
            CommittedEvent::Draw {
                player: PlayerId::P0,
                object: Some(*b),
            },
            CommittedEvent::ZoneChange {
                object: ponder,
                from: Zone::Stack,
                to: Zone::Graveyard,
                controller_before: PlayerId::P0,
            },
        ],
        "reorder, optional shuffle, and draw are one uninterrupted resolution"
    );
    let expected = state.clone();
    let expected_hash = state.state_hash();

    state.restore(&boolean_snapshot);
    assert_eq!(state.state_hash(), boolean_hash);
    let restored_boolean = engine::advance_until_decision(&mut state);
    assert_eq!(boolean_actions(&state, &restored_boolean), bool_actions);
    finish_boolean(&mut state, ponder, false);
    assert_eq!(state.state_hash(), expected_hash);
    assert_eq!(state, expected);

    state.restore(&reorder_snapshot);
    assert_eq!(state.state_hash(), reorder_hash);
    let restored_first = engine::advance_until_decision(&mut state);
    assert_eq!(reorder_actions(&state, &restored_first), first_actions);
    let restored_boolean = choose_explicit_order(&mut state, ponder, &[*a, *c]);
    assert_eq!(boolean_actions(&state, &restored_boolean), bool_actions);
    finish_boolean(&mut state, ponder, false);
    assert_eq!(state.state_hash(), expected_hash);
    assert_eq!(state, expected);
}

#[test]
fn ponder_three_card_reorder_covers_all_six_top_orders() {
    let permutations = [
        [0_usize, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ];
    for picked in permutations {
        let (mut state, ponder, library) = ready_ponder(&LONG_LIBRARY[..4]);
        cast_ponder(&mut state, ponder);
        let boolean = choose_explicit_order(
            &mut state,
            ponder,
            &[library[picked[0]], library[picked[1]]],
        );
        assert_boolean_decision(&boolean, ponder);
        let expected_pre_draw = [
            library[picked[2]],
            library[picked[1]],
            library[picked[0]],
            library[3],
        ];
        assert_eq!(state.players[0].library, expected_pre_draw);
        finish_boolean(&mut state, ponder, false);
        assert_eq!(state.players[0].hand, vec![expected_pre_draw[0]]);
        assert_eq!(state.players[0].library, expected_pre_draw[1..]);
    }
}

fn run_shuffled_ponder() -> (GameState, Vec<ObjectId>) {
    let (mut state, ponder, library) = ready_ponder(&LONG_LIBRARY);
    state.reveal_library_top(PlayerId::P0, PlayerId::P0, library.len());
    state.reveal_library_top(PlayerId::P1, PlayerId::P0, library.len());
    cast_ponder(&mut state, ponder);
    let boolean = choose_explicit_order(&mut state, ponder, &[library[0], library[2]]);
    assert_eq!(
        boolean_actions(&state, &boolean)
            .iter()
            .map(|(value, _)| *value)
            .collect::<Vec<_>>(),
        vec![false, true]
    );
    let boolean_snapshot = state.snapshot();
    let boolean_hash = state.state_hash();
    finish_boolean(&mut state, ponder, true);
    let expected = state.clone();
    let expected_hash = state.state_hash();

    state.restore(&boolean_snapshot);
    assert_eq!(state.state_hash(), boolean_hash);
    let restored_boolean = engine::advance_until_decision(&mut state);
    assert_boolean_decision(&restored_boolean, ponder);
    finish_boolean(&mut state, ponder, true);
    assert_eq!(state.state_hash(), expected_hash);
    assert_eq!(state, expected);
    (state, library)
}

#[test]
fn ponder_shuffle_yes_is_deterministic_preserves_multiset_and_clears_knowledge() {
    let (first, original) = run_shuffled_ponder();
    let (second, second_original) = run_shuffled_ponder();
    assert_eq!(original, second_original);
    assert_eq!(first.state_hash(), second.state_hash());
    assert_eq!(first, second);

    let mut after = first.players[0].library.clone();
    after.extend(first.players[0].hand.iter().copied());
    after.sort_unstable();
    let mut before = original.clone();
    before.sort_unstable();
    assert_eq!(
        after, before,
        "shuffle plus draw preserves the card multiset"
    );
    assert_eq!(first.players[0].hand.len(), 1);
    assert_eq!(first.players[0].library.len(), LONG_LIBRARY.len() - 1);
    assert!(first
        .known_library_cards(PlayerId::P0, PlayerId::P0)
        .is_empty());
    assert!(first
        .known_library_cards(PlayerId::P1, PlayerId::P0)
        .is_empty());
    assert_eq!(
        first.players[0]
            .hand
            .iter()
            .map(|&object| first.objects.get(object).name.as_str())
            .collect::<Vec<_>>(),
        vec!["Fiery Temper"]
    );
    assert_eq!(
        first.players[0]
            .library
            .iter()
            .map(|&object| first.objects.get(object).name.as_str())
            .collect::<Vec<_>>(),
        vec![
            "Fireblast",
            "Island",
            "Counterspell",
            "Lightning Bolt",
            "Mountain",
            "Lava Dart",
        ]
    );
}

#[test]
fn ponder_private_reorder_redacts_nonowner_and_updates_preexisting_knowledge() {
    let (mut private, ponder, _) = ready_ponder(&LONG_LIBRARY[..4]);
    cast_ponder(&mut private, ponder);
    let decision = next_ponder_choice(&mut private, ponder);
    let owner = observe_v2(&private, &HarnessSurfaceV2::new(), PlayerId::P0, 0).unwrap();
    let nonowner = observe_v2(&private, &HarnessSurfaceV2::new(), PlayerId::P1, 0).unwrap();
    assert_eq!(
        owner.known_library_cards[0]
            .iter()
            .map(|known| known.card.card_name.as_str())
            .collect::<Vec<_>>(),
        vec!["Fiery Temper", "Lava Dart", "Lightning Bolt"]
    );
    assert!(nonowner.known_library_cards[0].is_empty());
    assert!(matches!(
        owner
            .projection
            .engine_context
            .pending_effect
            .as_ref()
            .unwrap()
            .choice
            .as_ref()
            .unwrap(),
        PendingEffectChoiceSemanticV4::Targets {
            player: PlayerSeatV1::P0,
            structural_path,
            selected_targets,
            legal_targets,
            min_targets: 3,
            max_targets: 3,
            can_finish: false,
            ordered: true,
            purpose: TargetSelectionPurposeV4::LibraryOrder,
        } if structural_path == &vec![0]
            && selected_targets.is_empty()
            && legal_targets.len() == 3
    ));
    assert!(matches!(
        nonowner
            .projection
            .engine_context
            .pending_effect
            .as_ref()
            .unwrap()
            .choice
            .as_ref()
            .unwrap(),
        PendingEffectChoiceSemanticV4::Targets {
            player: PlayerSeatV1::P0,
            selected_targets,
            legal_targets,
            min_targets: 3,
            max_targets: 3,
            can_finish: false,
            ordered: true,
            purpose: TargetSelectionPurposeV4::LibraryOrder,
            ..
        } if selected_targets.is_empty() && legal_targets.is_empty()
    ));
    let nonowner_json = serde_json::to_string(&nonowner).unwrap();
    for name in ["Fiery Temper", "Lava Dart", "Lightning Bolt"] {
        assert!(!nonowner_json.contains(name));
    }
    assert_eq!(reorder_actions(&private, &decision).len(), 3);

    let (mut informed, ponder, library) = ready_ponder(&LONG_LIBRARY[..4]);
    informed.reveal_library_top(PlayerId::P1, PlayerId::P0, 4);
    cast_ponder(&mut informed, ponder);
    let first = next_ponder_choice(&mut informed, ponder);
    assert!(matches!(
        first,
        Decision::ChooseEffectTargets {
            selected_count: 0,
            ref legal_targets,
            ..
        } if legal_targets.len() == 3
    ));
    assert_eq!(
        observe_v2(&informed, &HarnessSurfaceV2::new(), PlayerId::P1, 0)
            .unwrap()
            .known_library_cards[0]
            .len(),
        4,
        "a private look does not erase valid preexisting facts"
    );
    engine::step(
        &mut informed,
        Action::ChooseEffectTarget(Target::Object(library[0])),
    )
    .unwrap();
    let second = immediate_ponder_choice(&mut informed, ponder);
    assert!(matches!(
        second,
        Decision::ChooseEffectTargets {
            selected_count: 1,
            ref legal_targets,
            ..
        } if legal_targets.len() == 2
    ));
    let owner_after_pick =
        observe_v2(&informed, &HarnessSurfaceV2::new(), PlayerId::P0, 1).unwrap();
    let nonowner_after_pick =
        observe_v2(&informed, &HarnessSurfaceV2::new(), PlayerId::P1, 1).unwrap();
    assert!(matches!(
        owner_after_pick
            .projection
            .engine_context
            .pending_effect
            .as_ref()
            .unwrap()
            .choice
            .as_ref()
            .unwrap(),
        PendingEffectChoiceSemanticV4::Targets {
            selected_targets,
            legal_targets,
            ..
        } if selected_targets.len() == 1 && legal_targets.len() == 2
    ));
    assert!(matches!(
        nonowner_after_pick
            .projection
            .engine_context
            .pending_effect
            .as_ref()
            .unwrap()
            .choice
            .as_ref()
            .unwrap(),
        PendingEffectChoiceSemanticV4::Targets {
            selected_targets,
            legal_targets,
            ..
        } if selected_targets.is_empty() && legal_targets.is_empty()
    ));
    engine::step(
        &mut informed,
        Action::ChooseEffectTarget(Target::Object(library[2])),
    )
    .unwrap();
    let boolean = immediate_ponder_choice(&mut informed, ponder);
    assert_boolean_decision(&boolean, ponder);
    let p1_at_boolean = observe_v2(&informed, &HarnessSurfaceV2::new(), PlayerId::P1, 1).unwrap();
    assert_eq!(
        p1_at_boolean.known_library_cards[0]
            .iter()
            .map(|known| (known.position, known.card.card_name.as_str()))
            .collect::<Vec<_>>(),
        vec![(3, "Mountain")],
        "the changed private prefix is forgotten while unaffected depth remains known"
    );
    assert!(matches!(
        p1_at_boolean
            .projection
            .engine_context
            .pending_effect
            .as_ref()
            .unwrap()
            .choice
            .as_ref()
            .unwrap(),
        PendingEffectChoiceSemanticV4::Boolean {
            player: PlayerSeatV1::P0,
            structural_path,
            default: Some(false),
            purpose: BooleanChoicePurposeV4::Shuffle,
        } if structural_path == &vec![1]
    ));
    finish_boolean(&mut informed, ponder, false);
    assert_eq!(
        observe_v2(&informed, &HarnessSurfaceV2::new(), PlayerId::P1, 2)
            .unwrap()
            .known_library_cards[0]
            .iter()
            .map(|known| (known.position, known.card.card_name.as_str()))
            .collect::<Vec<_>>(),
        vec![(2, "Mountain")],
        "drawing shifts the still-valid deeper fact by one position"
    );
}

#[test]
fn ponder_short_libraries_zero_one_two_three_and_deck_out_are_exact() {
    const SHORT: [&str; 3] = ["Fiery Temper", "Lava Dart", "Lightning Bolt"];
    for count in 0..=3 {
        let (mut state, ponder, library) = ready_ponder(&SHORT[..count]);
        cast_ponder(&mut state, ponder);
        let explicit = count.saturating_sub(1);
        for selected_count in 0..explicit {
            let decision = if selected_count == 0 {
                next_ponder_choice(&mut state, ponder)
            } else {
                immediate_ponder_choice(&mut state, ponder)
            };
            assert!(matches!(
                decision,
                Decision::ChooseEffectTargets {
                    selected_count: actual,
                    ..
                } if actual == selected_count as u16
            ));
            let legal = reorder_actions(&state, &decision);
            engine::step(
                &mut state,
                Action::ChooseEffectTarget(Target::Object(legal[0].0)),
            )
            .unwrap();
        }
        let boolean = if explicit == 0 {
            next_ponder_choice(&mut state, ponder)
        } else {
            immediate_ponder_choice(&mut state, ponder)
        };
        assert_boolean_decision(&boolean, ponder);
        assert_eq!(
            boolean_actions(&state, &boolean)
                .iter()
                .map(|(value, _)| *value)
                .collect::<Vec<_>>(),
            vec![false, true],
            "the optional shuffle is real even for a {count}-card library"
        );
        let mut expected_pre_draw = library.clone();
        expected_pre_draw.reverse();
        assert_eq!(state.players[0].library, expected_pre_draw);
        let final_decision = finish_boolean(&mut state, ponder, false);
        if count == 0 {
            assert!(
                matches!(
                    final_decision,
                    Decision::GameOver {
                        winner: Some(PlayerId::P1)
                    }
                ),
                "empty-library Ponder must reach SBA without priority, got {final_decision:?}"
            );
            assert!(state.players[0].hand.is_empty());
            assert_eq!(state.players[0].draws_this_turn, 0);
            assert!(state.players[0].drew_from_empty);
            assert!(state.engine.event_history.iter().any(|event| matches!(
                event,
                CommittedEvent::Draw {
                    player: PlayerId::P0,
                    object: None,
                }
            )));
        } else {
            assert!(matches!(final_decision, Decision::CastSpellOrPass { .. }));
            assert_eq!(
                state.players[0].hand,
                vec![*expected_pre_draw.first().unwrap()]
            );
            assert_eq!(state.players[0].library, expected_pre_draw[1..]);
            assert_eq!(state.players[0].draws_this_turn, 1);
            assert!(!state.players[0].drew_from_empty);
        }
        assert_eq!(state.players[0].graveyard, vec![ponder]);
    }
}

fn ready_at_reorder() -> (GameState, ObjectId, Vec<ObjectId>) {
    let (mut state, ponder, library) = ready_ponder(&LONG_LIBRARY[..4]);
    cast_ponder(&mut state, ponder);
    assert!(matches!(
        next_ponder_choice(&mut state, ponder),
        Decision::ChooseEffectTargets { .. }
    ));
    (state, ponder, library)
}

#[test]
fn ponder_reorder_fails_closed_on_stale_prefix_and_stale_incarnation() {
    let (mut prefix, ponder, library) = ready_at_reorder();
    let pending = prefix.engine.pending_effect.clone();
    prefix
        .reorder_library_top(PlayerId::P0, &[library[1], library[0], library[2]], &[])
        .unwrap();
    prefix.engine.pending_effect = pending;
    let before = prefix.clone();
    assert!(engine::step(
        &mut prefix,
        Action::ChooseEffectTarget(Target::Object(library[0]))
    )
    .is_err());
    assert_eq!(prefix, before);
    assert!(matches!(
        engine::advance_until_decision(&mut prefix),
        Decision::Halted {
            mechanic: UnsupportedMechanic::InvalidEffectContinuation,
            source,
        } if source == ponder
    ));
    assert!(prefix.players[0].hand.is_empty());

    let (mut incarnation, ponder, library) = ready_at_reorder();
    let pending = incarnation.engine.pending_effect.clone();
    event::propose_and_commit(
        &mut incarnation,
        ProposedEvent::zone_change(library[0], Zone::Graveyard),
    );
    event::propose_and_commit(
        &mut incarnation,
        ProposedEvent::zone_change(library[0], Zone::Library),
    );
    incarnation
        .reorder_library_top(PlayerId::P0, &library, &[PlayerId::P0])
        .unwrap();
    incarnation.engine.pending_effect = pending;
    assert_eq!(incarnation.players[0].library, library);
    assert_eq!(incarnation.objects.get(library[0]).zone_change_count, 2);
    let before = incarnation.clone();
    assert!(engine::step(
        &mut incarnation,
        Action::ChooseEffectTarget(Target::Object(library[0]))
    )
    .is_err());
    assert_eq!(incarnation, before);
    assert!(matches!(
        engine::advance_until_decision(&mut incarnation),
        Decision::Halted {
            mechanic: UnsupportedMechanic::InvalidEffectContinuation,
            source,
        } if source == ponder
    ));
    assert!(incarnation.players[0].hand.is_empty());
}

#[test]
fn ponder_reorder_fails_closed_before_observation_on_tampered_chooser() {
    let (mut state, ponder, _) = ready_at_reorder();
    let choice = state
        .engine
        .pending_effect
        .as_mut()
        .unwrap()
        .choice
        .as_mut()
        .unwrap();
    let mtg_kernel::effect::PendingEffectChoice::SelectTargets { player, .. } = choice else {
        panic!("Ponder must be waiting for its library-order target choice")
    };
    *player = PlayerId::P1;

    assert!(
        observe_v2(&state, &HarnessSurfaceV2::new(), PlayerId::P1, 0).is_err(),
        "an invalid chooser/library pairing must not expose private candidates"
    );
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::Halted {
            mechanic: UnsupportedMechanic::InvalidEffectContinuation,
            source,
        } if source == ponder
    ));
    assert!(state.players[0].hand.is_empty());
}

#[test]
fn a_countered_ponder_does_not_look_reorder_shuffle_or_draw() {
    let (mut state, ponder, library) = ready_ponder(&LONG_LIBRARY[..4]);
    put_object(&mut state, PlayerId::P1, "Island", Zone::Battlefield);
    put_object(&mut state, PlayerId::P1, "Island", Zone::Battlefield);
    let counterspell = put_object(&mut state, PlayerId::P1, "Counterspell", Zone::Hand);
    cast_ponder(&mut state, ponder);

    let mut counter_announced = false;
    for _ in 0..16 {
        let decision = engine::advance_until_decision(&mut state);
        if state.objects.get(ponder).zone == Zone::Graveyard
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
                assert!(legal_targets.contains(&Target::Object(ponder)));
                engine::step(&mut state, Action::ChooseTarget(Target::Object(ponder))).unwrap();
            }
            Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
            other => panic!("unexpected Counterspell decision: {other:?}"),
        }
    }

    assert!(counter_announced);
    assert_eq!(state.players[0].library, library);
    assert!(state.players[0].hand.is_empty());
    assert_eq!(state.players[0].draws_this_turn, 0);
    assert!(!state.players[0].drew_from_empty);
    assert!(state
        .known_library_cards(PlayerId::P0, PlayerId::P0)
        .is_empty());
    assert!(state.engine.pending_effect.is_none());
    assert_eq!(state.objects.get(ponder).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(counterspell).zone, Zone::Graveyard);
}
