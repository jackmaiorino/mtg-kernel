//! Integration coverage for Brainstorm's generic repeated private
//! hand-to-library-top substrate.
//!
//! The bounded rules oracle is XMage commit
//! `0723fc0c2be922af47b0ef0539f28114cc23b998`: `Brainstorm.java` blob
//! `c3d2c4328471ecc8e47a4f75905960fe1e832217`, `BrainstormEffect.java`
//! blob `bcf20e70b9c891211fd2ed70057ce734455296d7`,
//! `TargetCardInHand.java` blob
//! `df79653a01d4eb5bf5e41b4d3ff082e96a03a259`, `PlayerImpl.java` blob
//! `1fc883515410e4b6df6255e5be44547f82617784`, and `Library.java` blob
//! `85a74823920d73f0d443f80cf54abbece728714f`. XMage draws three, then
//! performs two fresh exact-one private hand choices. Each chosen card moves
//! to the top immediately, so the first pick is deepest and the second pick
//! is topmost. A failed draw does not interrupt either available put-back;
//! the draw-from-empty loss is an SBA after the whole spell resolves.

use mtg_kernel::card_def::{card_id_by_name, CARD_DEFS};
use mtg_kernel::effect::{EffectFrame, EffectTargetSelectionPurpose, PendingEffectChoice};
use mtg_kernel::engine::{self, Action, Decision, UnsupportedMechanic};
use mtg_kernel::event::{self, CommittedEvent, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::rl::{
    legal_action_candidates_v1, observe_v2, ActionSemanticV1, PendingEffectChoiceSemanticV4,
    PlayerSeatV1, TargetRefV1, TargetSelectionPurposeV4,
};
use mtg_kernel::state::{Counters, GameObject, GameState, Step, Target, Zone};
use mtg_kernel::surface_v2::{HarnessSurfaceV2, SurfaceDecision};

const DRAW_CARDS: [&str; 4] = ["Fiery Temper", "Lava Dart", "Lightning Bolt", "Mountain"];

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

fn ready_brainstorm(
    library_names: &[&str],
    hand_names: &[&str],
) -> (GameState, ObjectId, Vec<ObjectId>, Vec<ObjectId>) {
    let library_defs = library_names
        .iter()
        .map(|name| card_id(name))
        .collect::<Vec<_>>();
    let p1_library = [card_id("Snow-Covered Forest")];
    let mut state =
        GameState::new_from_libraries(&library_defs, &p1_library, card_name, 0x4252_4149_4E53_544F);
    state.step = Step::Main1;
    state.active_player = PlayerId::P0;
    state.priority_player = PlayerId::P0;
    put_object(&mut state, PlayerId::P0, "Island", Zone::Battlefield);
    let preexisting = hand_names
        .iter()
        .map(|name| put_object(&mut state, PlayerId::P0, name, Zone::Hand))
        .collect::<Vec<_>>();
    let brainstorm = put_object(&mut state, PlayerId::P0, "Brainstorm", Zone::Hand);
    let library = state.players[0].library.clone();
    (state, brainstorm, library, preexisting)
}

fn cast_brainstorm(state: &mut GameState, brainstorm: ObjectId) {
    engine::step(state, Action::CastSpell(brainstorm)).unwrap();
    assert_eq!(state.objects.get(brainstorm).zone, Zone::Stack);
    assert!(state.stack.iter().any(|item| item.source == brainstorm));
}

/// Passes priority only while Brainstorm is still waiting on the stack.
/// Once its resolution yields a private choice, completes, halts, or reaches
/// an SBA, return that decision unchanged.
fn advance_brainstorm(state: &mut GameState, brainstorm: ObjectId) -> Decision {
    for _ in 0..16 {
        let decision = engine::advance_until_decision(state);
        match decision {
            Decision::CastSpellOrPass { .. }
                if state.objects.get(brainstorm).zone == Zone::Stack
                    && state.engine.pending_effect.is_none() =>
            {
                engine::step(state, Action::Pass).unwrap();
            }
            other => return other,
        }
    }
    panic!("Brainstorm did not resolve or reach an effect choice")
}

fn next_brainstorm_choice(state: &mut GameState, brainstorm: ObjectId) -> Decision {
    let decision = advance_brainstorm(state, brainstorm);
    assert!(matches!(
        decision,
        Decision::ChooseEffectTargets { source, .. } if source == brainstorm
    ));
    decision
}

fn immediate_brainstorm_choice(state: &mut GameState, brainstorm: ObjectId) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(
        matches!(
            decision,
            Decision::ChooseEffectTargets { source, .. } if source == brainstorm
        ),
        "Brainstorm resolution exposed a non-effect decision between private puts: {decision:?}"
    );
    decision
}

fn assert_hand_choice(decision: &Decision, brainstorm: ObjectId, legal: &[ObjectId]) {
    assert!(matches!(
        decision,
        Decision::ChooseEffectTargets {
            player: PlayerId::P0,
            source,
            selected_count: 0,
            min_targets: 1,
            max_targets: 1,
            legal_targets,
            can_finish: false,
        } if *source == brainstorm
            && legal_targets == &legal.iter().copied().map(Target::Object).collect::<Vec<_>>()
    ));
}

fn hand_choice_actions(state: &GameState, decision: &Decision) -> Vec<(ObjectId, String)> {
    legal_action_candidates_v1(&SurfaceDecision::Decision(decision.clone()), state)
        .unwrap()
        .into_iter()
        .map(|candidate| match candidate.record.semantic {
            ActionSemanticV1::ChooseEffectTarget {
                target: TargetRefV1::Object { object },
                ..
            } => (ObjectId(object.arena_id), candidate.record.stable_id),
            other => panic!("unexpected Brainstorm hand-choice action: {other:?}"),
        })
        .collect()
}

fn answer_hand_choice(state: &mut GameState, object: ObjectId) {
    engine::step(state, Action::ChooseEffectTarget(Target::Object(object))).unwrap();
}

#[test]
fn brainstorm_golden_is_two_independent_private_puts_with_exact_history_and_restore() {
    // XMage golden: draw A,B,C from A,B,C,D; pick A, then C. A moves
    // immediately before the fresh second prompt and C ends topmost.
    let (mut state, brainstorm, library, _) = ready_brainstorm(&DRAW_CARDS, &[]);
    let [a, b, c, d] = library.as_slice() else {
        unreachable!()
    };
    cast_brainstorm(&mut state, brainstorm);

    let first = next_brainstorm_choice(&mut state, brainstorm);
    let history_start = state
        .engine
        .event_history
        .iter()
        .position(|event| {
            matches!(
                event,
                CommittedEvent::Draw {
                    player: PlayerId::P0,
                    object: Some(object),
                } if object == a
            )
        })
        .expect("Brainstorm resolution starts with its first draw");
    assert_hand_choice(&first, brainstorm, &[*a, *b, *c]);
    assert_eq!(state.players[0].hand, vec![*a, *b, *c]);
    assert_eq!(state.players[0].library, vec![*d]);
    assert_eq!(state.players[0].draws_this_turn, 3);
    assert!(!state.players[0].drew_from_empty);
    let first_actions = hand_choice_actions(&state, &first);
    assert_eq!(first_actions.len(), 3);
    assert_eq!(
        first_actions
            .iter()
            .map(|(object, _)| *object)
            .collect::<Vec<_>>(),
        vec![*a, *b, *c]
    );
    assert_eq!(
        first_actions,
        vec![
            (*a, "legal-action-v4:6ca1cb4ef6341afd".to_string()),
            (*b, "legal-action-v4:cebb55b006181f92".to_string()),
            (*c, "legal-action-v4:9b70e61f19dd7328".to_string()),
        ],
        "Brainstorm's first private prompt is a frozen schema-v4 action fixture"
    );
    let first_snapshot = state.snapshot();
    let first_hash = state.state_hash();
    assert_eq!(first_hash, 0xdc52_2840_583c_3885);

    answer_hand_choice(&mut state, *a);
    assert_eq!(
        state.players[0].library,
        vec![*d],
        "answering records a commit frame; advancing performs the private move"
    );
    assert_eq!(state.players[0].hand, vec![*a, *b, *c]);
    assert!(state
        .engine
        .pending_effect
        .as_ref()
        .unwrap()
        .choice
        .is_none());
    assert!(matches!(
        state
            .engine
            .pending_effect
            .as_ref()
            .unwrap()
            .frames
            .last(),
        Some(EffectFrame::PutCardsFromHandOnLibraryTop {
            player: PlayerId::P0,
            remaining: 2,
            prompt_index: 0,
            chosen: Some(binding),
            ..
        }) if binding.object == *a
    ));
    let answered_snapshot = state.snapshot();
    let answered_hash = state.state_hash();
    assert_eq!(answered_hash, 0x1517_5867_dbc5_cd9d);
    let answered = state.clone();
    assert!(engine::step(&mut state, Action::ChooseEffectTarget(Target::Object(*a))).is_err());
    assert_eq!(state, answered, "an answered action is immediately stale");

    let second = immediate_brainstorm_choice(&mut state, brainstorm);
    assert_hand_choice(&second, brainstorm, &[*b, *c]);
    assert_eq!(state.players[0].hand, vec![*b, *c]);
    assert_eq!(state.players[0].library, vec![*a, *d]);
    assert_eq!(state.objects.get(*a).zone_change_count, 2);
    assert_eq!(state.objects.get(brainstorm).zone, Zone::Stack);
    assert_eq!(state.priority_player, PlayerId::P0);
    let second_actions = hand_choice_actions(&state, &second);
    assert_eq!(
        second_actions,
        vec![
            (*b, "legal-action-v4:cebb55b006181f92".to_string()),
            (*c, "legal-action-v4:9b70e61f19dd7328".to_string()),
        ]
    );
    let second_snapshot = state.snapshot();
    let second_hash = state.state_hash();
    assert_eq!(second_hash, 0x0a2b_8c1f_9371_0c12);

    answer_hand_choice(&mut state, *c);
    assert_eq!(state.players[0].library, vec![*a, *d]);
    let after_second_answer = state.clone();
    assert!(engine::step(&mut state, Action::ChooseEffectTarget(Target::Object(*b))).is_err());
    assert_eq!(state, after_second_answer);
    let final_decision = engine::advance_until_decision(&mut state);
    assert!(matches!(final_decision, Decision::CastSpellOrPass { .. }));
    assert_eq!(state.players[0].hand, vec![*b]);
    assert_eq!(state.players[0].library, vec![*c, *a, *d]);
    assert_eq!(state.players[0].graveyard, vec![brainstorm]);
    assert_eq!(state.objects.get(*c).zone_change_count, 2);
    assert_eq!(state.objects.get(*b).zone_change_count, 1);
    assert_eq!(state.objects.get(brainstorm).zone, Zone::Graveyard);
    assert_eq!(
        &state.engine.event_history[history_start..],
        &[
            CommittedEvent::Draw {
                player: PlayerId::P0,
                object: Some(*a),
            },
            CommittedEvent::Draw {
                player: PlayerId::P0,
                object: Some(*b),
            },
            CommittedEvent::Draw {
                player: PlayerId::P0,
                object: Some(*c),
            },
            CommittedEvent::ZoneChange {
                object: *a,
                from: Zone::Hand,
                to: Zone::Library,
                controller_before: PlayerId::P0,
            },
            CommittedEvent::ZoneChange {
                object: *c,
                from: Zone::Hand,
                to: Zone::Library,
                controller_before: PlayerId::P0,
            },
            CommittedEvent::ZoneChange {
                object: brainstorm,
                from: Zone::Stack,
                to: Zone::Graveyard,
                controller_before: PlayerId::P0,
            },
        ],
        "draw and both private puts are one uninterrupted resolution"
    );
    let expected = state.clone();
    let expected_hash = state.state_hash();
    assert_eq!(expected_hash, 0xf5db_8aef_e3d4_bb59);

    state.restore(&second_snapshot);
    assert_eq!(state.state_hash(), second_hash);
    let restored_second = engine::advance_until_decision(&mut state);
    assert_eq!(
        hand_choice_actions(&state, &restored_second),
        second_actions
    );
    answer_hand_choice(&mut state, *c);
    engine::advance_until_decision(&mut state);
    assert_eq!(state.state_hash(), expected_hash);
    assert_eq!(state, expected);

    state.restore(&answered_snapshot);
    assert_eq!(state.state_hash(), answered_hash);
    let restored_second = engine::advance_until_decision(&mut state);
    assert_eq!(
        hand_choice_actions(&state, &restored_second),
        second_actions
    );
    answer_hand_choice(&mut state, *c);
    engine::advance_until_decision(&mut state);
    assert_eq!(state.state_hash(), expected_hash);
    assert_eq!(state, expected);

    state.restore(&first_snapshot);
    assert_eq!(state.state_hash(), first_hash);
    let restored_first = engine::advance_until_decision(&mut state);
    assert_eq!(hand_choice_actions(&state, &restored_first), first_actions);
    answer_hand_choice(&mut state, *a);
    let restored_second = immediate_brainstorm_choice(&mut state, brainstorm);
    assert_eq!(
        hand_choice_actions(&state, &restored_second),
        second_actions
    );
    answer_hand_choice(&mut state, *c);
    engine::advance_until_decision(&mut state);
    assert_eq!(state.state_hash(), expected_hash);
    assert_eq!(state, expected);
}

#[test]
fn brainstorm_three_drawn_cards_cover_all_six_ordered_pairs() {
    let ordered_pairs = [[0_usize, 1], [0, 2], [1, 0], [1, 2], [2, 0], [2, 1]];
    for [first_index, second_index] in ordered_pairs {
        let (mut state, brainstorm, library, _) = ready_brainstorm(&DRAW_CARDS, &[]);
        cast_brainstorm(&mut state, brainstorm);
        let first = next_brainstorm_choice(&mut state, brainstorm);
        assert_hand_choice(&first, brainstorm, &library[..3]);
        answer_hand_choice(&mut state, library[first_index]);

        let remaining = (0..3)
            .filter(|&index| index != first_index)
            .map(|index| library[index])
            .collect::<Vec<_>>();
        let second = immediate_brainstorm_choice(&mut state, brainstorm);
        assert_hand_choice(&second, brainstorm, &remaining);
        answer_hand_choice(&mut state, library[second_index]);
        let decision = engine::advance_until_decision(&mut state);
        assert!(matches!(decision, Decision::CastSpellOrPass { .. }));

        let unchosen = (0..3)
            .find(|&index| index != first_index && index != second_index)
            .unwrap();
        assert_eq!(state.players[0].hand, vec![library[unchosen]]);
        assert_eq!(
            state.players[0].library,
            vec![library[second_index], library[first_index], library[3],]
        );
    }
}

#[test]
fn brainstorm_can_put_preexisting_hand_cards_and_same_names_remain_distinct() {
    let (mut state, brainstorm, library, preexisting) =
        ready_brainstorm(&DRAW_CARDS, &["Fireblast"]);
    let old = preexisting[0];
    cast_brainstorm(&mut state, brainstorm);
    let first = next_brainstorm_choice(&mut state, brainstorm);
    assert_hand_choice(
        &first,
        brainstorm,
        &[old, library[0], library[1], library[2]],
    );
    answer_hand_choice(&mut state, old);
    let second = immediate_brainstorm_choice(&mut state, brainstorm);
    assert_hand_choice(&second, brainstorm, &[library[0], library[1], library[2]]);
    answer_hand_choice(&mut state, library[1]);
    engine::advance_until_decision(&mut state);
    assert_eq!(state.players[0].hand, vec![library[0], library[2]]);
    assert_eq!(state.players[0].library, vec![library[1], old, library[3]]);

    let same_names = ["Island", "Island", "Island", "Mountain"];
    let (mut first_run, spell, same_library, _) = ready_brainstorm(&same_names, &[]);
    cast_brainstorm(&mut first_run, spell);
    let decision = next_brainstorm_choice(&mut first_run, spell);
    let actions = hand_choice_actions(&first_run, &decision);
    assert_eq!(
        actions
            .iter()
            .map(|(object, _)| *object)
            .collect::<Vec<_>>(),
        same_library[..3]
    );
    assert_eq!(
        actions
            .iter()
            .map(|(_, stable_id)| stable_id)
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        3,
        "same-name physical cards require distinct stable actions"
    );

    let (mut second_run, second_spell, second_library, _) = ready_brainstorm(&same_names, &[]);
    cast_brainstorm(&mut second_run, second_spell);
    let second_decision = next_brainstorm_choice(&mut second_run, second_spell);
    assert_eq!(same_library, second_library);
    assert_eq!(first_run.state_hash(), second_run.state_hash());
    assert_eq!(actions, hand_choice_actions(&second_run, &second_decision));
}

#[test]
fn brainstorm_short_libraries_zero_one_two_three_and_forced_cards_are_exact() {
    for count in 0..=3 {
        let (mut state, brainstorm, library, _) = ready_brainstorm(&DRAW_CARDS[..count], &[]);
        cast_brainstorm(&mut state, brainstorm);
        let history_start = state.engine.event_history.len();
        let first_outcome = advance_brainstorm(&mut state, brainstorm);

        match count {
            0 => {
                assert!(matches!(
                    first_outcome,
                    Decision::GameOver {
                        winner: Some(PlayerId::P1)
                    }
                ));
                assert!(state.players[0].hand.is_empty());
                assert!(state.players[0].library.is_empty());
            }
            1 => {
                assert!(matches!(
                    first_outcome,
                    Decision::GameOver {
                        winner: Some(PlayerId::P1)
                    }
                ));
                assert!(state.players[0].hand.is_empty());
                assert_eq!(state.players[0].library, library);
            }
            2 => {
                assert_hand_choice(&first_outcome, brainstorm, &library);
                answer_hand_choice(&mut state, library[0]);
                assert_eq!(state.players[0].library, Vec::<ObjectId>::new());
                let final_decision = engine::advance_until_decision(&mut state);
                assert!(matches!(
                    final_decision,
                    Decision::GameOver {
                        winner: Some(PlayerId::P1)
                    }
                ));
                assert_eq!(state.players[0].library, vec![library[1], library[0]]);
                assert!(state.players[0].hand.is_empty());
            }
            3 => {
                assert_hand_choice(&first_outcome, brainstorm, &library);
                answer_hand_choice(&mut state, library[0]);
                let second = immediate_brainstorm_choice(&mut state, brainstorm);
                assert_hand_choice(&second, brainstorm, &library[1..]);
                answer_hand_choice(&mut state, library[1]);
                let final_decision = engine::advance_until_decision(&mut state);
                assert!(matches!(final_decision, Decision::CastSpellOrPass { .. }));
                assert_eq!(state.players[0].library, vec![library[1], library[0]]);
                assert_eq!(state.players[0].hand, vec![library[2]]);
            }
            _ => unreachable!(),
        }

        assert_eq!(state.players[0].draws_this_turn, count as u32);
        assert_eq!(state.players[0].drew_from_empty, count < 3);
        assert_eq!(state.players[0].graveyard, vec![brainstorm]);
        let draws = state.engine.event_history[history_start..]
            .iter()
            .filter(|event| matches!(event, CommittedEvent::Draw { .. }))
            .count();
        assert_eq!(draws, 3, "Brainstorm always attempts three draws");
        let failed_draws = state.engine.event_history[history_start..]
            .iter()
            .filter(|event| {
                matches!(
                    event,
                    CommittedEvent::Draw {
                        player: PlayerId::P0,
                        object: None,
                    }
                )
            })
            .count();
        assert_eq!(failed_draws, 3 - count);
        let hand_to_library = state.engine.event_history[history_start..]
            .iter()
            .filter(|event| {
                matches!(
                    event,
                    CommittedEvent::ZoneChange {
                        from: Zone::Hand,
                        to: Zone::Library,
                        ..
                    }
                )
            })
            .count();
        assert_eq!(hand_to_library, count.min(2));
    }
}

#[test]
fn brainstorm_empty_draw_waits_for_both_private_prompts_before_sba() {
    let hand_names = ["Fiery Temper", "Lava Dart", "Lightning Bolt"];
    let (mut state, brainstorm, _, hand) = ready_brainstorm(&[], &hand_names);
    cast_brainstorm(&mut state, brainstorm);
    let first = next_brainstorm_choice(&mut state, brainstorm);
    assert_hand_choice(&first, brainstorm, &hand);
    assert!(state.players[0].drew_from_empty);
    assert_eq!(state.players[0].draws_this_turn, 0);
    assert_eq!(state.objects.get(brainstorm).zone, Zone::Stack);
    answer_hand_choice(&mut state, hand[0]);

    let second = immediate_brainstorm_choice(&mut state, brainstorm);
    assert_hand_choice(&second, brainstorm, &hand[1..]);
    assert!(state.players[0].drew_from_empty);
    assert_eq!(state.players[0].library, vec![hand[0]]);
    assert_eq!(state.objects.get(brainstorm).zone, Zone::Stack);
    answer_hand_choice(&mut state, hand[2]);

    let game_over = engine::advance_until_decision(&mut state);
    assert!(matches!(
        game_over,
        Decision::GameOver {
            winner: Some(PlayerId::P1)
        }
    ));
    assert_eq!(state.players[0].hand, vec![hand[1]]);
    assert_eq!(state.players[0].library, vec![hand[2], hand[0]]);
    assert_eq!(state.objects.get(brainstorm).zone, Zone::Graveyard);
}

#[test]
fn brainstorm_private_choices_redact_nonowner_and_preserve_only_sound_knowledge() {
    let (mut state, brainstorm, library, preexisting) =
        ready_brainstorm(&DRAW_CARDS, &["Fireblast"]);
    let old_hand = preexisting[0];
    state
        .reveal_hand_card(PlayerId::P1, PlayerId::P0, old_hand)
        .unwrap();
    state.reveal_library_top(PlayerId::P0, PlayerId::P0, library.len());
    state.reveal_library_top(PlayerId::P1, PlayerId::P0, library.len());
    cast_brainstorm(&mut state, brainstorm);
    let first = next_brainstorm_choice(&mut state, brainstorm);
    assert_hand_choice(
        &first,
        brainstorm,
        &[old_hand, library[0], library[1], library[2]],
    );

    let owner = observe_v2(&state, &HarnessSurfaceV2::new(), PlayerId::P0, 0).unwrap();
    let nonowner = observe_v2(&state, &HarnessSurfaceV2::new(), PlayerId::P1, 0).unwrap();
    assert_eq!(
        owner
            .own_hand
            .iter()
            .map(|card| card.card_name.as_str())
            .collect::<Vec<_>>(),
        vec!["Fireblast", "Fiery Temper", "Lava Dart", "Lightning Bolt"]
    );
    assert!(nonowner.known_hand_cards[0].is_empty());
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
            min_targets: 1,
            max_targets: 1,
            can_finish: false,
            ordered: true,
            purpose: TargetSelectionPurposeV4::LibraryOrder,
        } if structural_path == &vec![1, 0]
            && selected_targets.is_empty()
            && legal_targets.len() == 4
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
            structural_path,
            selected_targets,
            legal_targets,
            min_targets: 1,
            max_targets: 1,
            can_finish: false,
            ordered: true,
            purpose: TargetSelectionPurposeV4::LibraryOrder,
        } if structural_path == &vec![1, 0]
            && selected_targets.is_empty()
            && legal_targets.is_empty()
    ));
    let nonowner_json = serde_json::to_string(&nonowner).unwrap();
    for private_name in ["Fireblast", "Fiery Temper", "Lava Dart", "Lightning Bolt"] {
        assert!(!nonowner_json.contains(private_name));
    }

    let first_prompt = state.clone();
    let mut chose_old = first_prompt.clone();
    answer_hand_choice(&mut chose_old, old_hand);
    let old_second = immediate_brainstorm_choice(&mut chose_old, brainstorm);
    assert_hand_choice(
        &old_second,
        brainstorm,
        &[library[0], library[1], library[2]],
    );
    let mut chose_drawn = first_prompt;
    answer_hand_choice(&mut chose_drawn, library[0]);
    let drawn_second = immediate_brainstorm_choice(&mut chose_drawn, brainstorm);
    assert_hand_choice(
        &drawn_second,
        brainstorm,
        &[old_hand, library[1], library[2]],
    );
    let p1_old = observe_v2(&chose_old, &HarnessSurfaceV2::new(), PlayerId::P1, 1).unwrap();
    let p1_drawn = observe_v2(&chose_drawn, &HarnessSurfaceV2::new(), PlayerId::P1, 1).unwrap();
    assert_eq!(
        serde_json::to_vec(&p1_old).unwrap(),
        serde_json::to_vec(&p1_drawn).unwrap(),
        "the nonowner must not infer which private hand card moved"
    );

    state = chose_old;
    assert_eq!(
        observe_v2(&state, &HarnessSurfaceV2::new(), PlayerId::P1, 1)
            .unwrap()
            .known_library_cards[0]
            .iter()
            .map(|known| (known.position, known.card.card_name.as_str()))
            .collect::<Vec<_>>(),
        vec![(1, "Mountain")],
        "the anonymous insertion shifts valid deeper knowledge"
    );
    answer_hand_choice(&mut state, library[1]);
    engine::advance_until_decision(&mut state);
    let owner_final = observe_v2(&state, &HarnessSurfaceV2::new(), PlayerId::P0, 2).unwrap();
    let nonowner_final = observe_v2(&state, &HarnessSurfaceV2::new(), PlayerId::P1, 2).unwrap();
    assert_eq!(
        owner_final.known_library_cards[0]
            .iter()
            .map(|known| (known.position, known.card.card_name.as_str()))
            .collect::<Vec<_>>(),
        vec![(0, "Lava Dart"), (1, "Fireblast"), (2, "Mountain")]
    );
    assert_eq!(
        nonowner_final.known_library_cards[0]
            .iter()
            .map(|known| (known.position, known.card.card_name.as_str()))
            .collect::<Vec<_>>(),
        vec![(2, "Mountain")]
    );
    assert!(nonowner_final.known_hand_cards[0].is_empty());
}

fn ready_at_first_prompt() -> (GameState, ObjectId, Vec<ObjectId>) {
    let (mut state, brainstorm, library, _) = ready_brainstorm(&DRAW_CARDS, &[]);
    cast_brainstorm(&mut state, brainstorm);
    let decision = next_brainstorm_choice(&mut state, brainstorm);
    assert_hand_choice(&decision, brainstorm, &library[..3]);
    (state, brainstorm, library)
}

fn assert_invalid_continuation(state: &mut GameState, brainstorm: ObjectId) {
    assert!(matches!(
        engine::advance_until_decision(state),
        Decision::Halted {
            mechanic: UnsupportedMechanic::InvalidEffectContinuation,
            source,
        } if source == brainstorm
    ));
}

fn assert_no_hand_to_library_events_since(state: &GameState, history_start: usize) {
    assert!(state.engine.event_history[history_start..]
        .iter()
        .all(|event| {
            !matches!(
                event,
                CommittedEvent::ZoneChange {
                    from: Zone::Hand,
                    to: Zone::Library,
                    ..
                }
            )
        }));
}

fn assert_invalid_private_prompt_before_move(state: &mut GameState, brainstorm: ObjectId) {
    let history_start = state.engine.event_history.len();
    assert!(
        observe_v2(state, &HarnessSurfaceV2::new(), PlayerId::P1, 0).is_err(),
        "invalid private coordinator metadata must fail before observation"
    );
    assert_invalid_continuation(state, brainstorm);
    assert_no_hand_to_library_events_since(state, history_start);
}

#[test]
fn brainstorm_prompt_tampering_fails_closed_before_private_observation() {
    let (mut chooser, brainstorm, _) = ready_at_first_prompt();
    let PendingEffectChoice::SelectTargets { player, .. } = chooser
        .engine
        .pending_effect
        .as_mut()
        .unwrap()
        .choice
        .as_mut()
        .unwrap()
    else {
        panic!("Brainstorm must be waiting for a hand target")
    };
    *player = PlayerId::P1;
    assert!(observe_v2(&chooser, &HarnessSurfaceV2::new(), PlayerId::P1, 0).is_err());
    assert_invalid_continuation(&mut chooser, brainstorm);

    let (mut hand_set, brainstorm, _) = ready_at_first_prompt();
    put_object(&mut hand_set, PlayerId::P0, "Fireblast", Zone::Hand);
    assert!(observe_v2(&hand_set, &HarnessSurfaceV2::new(), PlayerId::P1, 0).is_err());
    assert_invalid_continuation(&mut hand_set, brainstorm);

    let (mut omitted, brainstorm, _) = ready_at_first_prompt();
    let PendingEffectChoice::SelectTargets { legal, .. } = omitted
        .engine
        .pending_effect
        .as_mut()
        .unwrap()
        .choice
        .as_mut()
        .unwrap()
    else {
        panic!("Brainstorm must be waiting for a hand target")
    };
    legal.pop();
    assert!(observe_v2(&omitted, &HarnessSurfaceV2::new(), PlayerId::P1, 0).is_err());
    assert_invalid_continuation(&mut omitted, brainstorm);

    let (mut duplicate, brainstorm, _) = ready_at_first_prompt();
    let PendingEffectChoice::SelectTargets { legal, .. } = duplicate
        .engine
        .pending_effect
        .as_mut()
        .unwrap()
        .choice
        .as_mut()
        .unwrap()
    else {
        panic!("Brainstorm must be waiting for a hand target")
    };
    legal[1] = legal[0].clone();
    assert!(observe_v2(&duplicate, &HarnessSurfaceV2::new(), PlayerId::P1, 0).is_err());
    assert_invalid_continuation(&mut duplicate, brainstorm);

    let (mut incarnation, brainstorm, library) = ready_at_first_prompt();
    let pending = incarnation.engine.pending_effect.clone();
    event::propose_and_commit(
        &mut incarnation,
        ProposedEvent::zone_change(library[0], Zone::Graveyard),
    );
    event::propose_and_commit(
        &mut incarnation,
        ProposedEvent::zone_change(library[0], Zone::Hand),
    );
    incarnation.engine.pending_effect = pending;
    assert_eq!(incarnation.objects.get(library[0]).zone_change_count, 3);
    assert!(observe_v2(&incarnation, &HarnessSurfaceV2::new(), PlayerId::P1, 0).is_err());
    let before = incarnation.clone();
    assert!(engine::step(
        &mut incarnation,
        Action::ChooseEffectTarget(Target::Object(library[0]))
    )
    .is_err());
    assert_eq!(incarnation, before);
    assert_invalid_continuation(&mut incarnation, brainstorm);
}

#[test]
fn brainstorm_pending_inconsistent_coordinator_metadata_fails_before_observation_or_move() {
    let (mut remaining, brainstorm, _) = ready_at_first_prompt();
    let PendingEffectChoice::SelectTargets { purpose, .. } = remaining
        .engine
        .pending_effect
        .as_mut()
        .unwrap()
        .choice
        .as_mut()
        .unwrap()
    else {
        panic!("Brainstorm must be waiting for a hand target")
    };
    let EffectTargetSelectionPurpose::PutHandCardOnLibraryTop {
        remaining: remaining_count,
        ..
    } = purpose
    else {
        panic!("Brainstorm must carry hand-to-library coordinator metadata")
    };
    assert_eq!(*remaining_count, 2);
    *remaining_count = 3;
    assert_invalid_private_prompt_before_move(&mut remaining, brainstorm);

    let (mut total, brainstorm, _) = ready_at_first_prompt();
    let PendingEffectChoice::SelectTargets { purpose, .. } = total
        .engine
        .pending_effect
        .as_mut()
        .unwrap()
        .choice
        .as_mut()
        .unwrap()
    else {
        panic!("Brainstorm must be waiting for a hand target")
    };
    let EffectTargetSelectionPurpose::PutHandCardOnLibraryTop {
        total: total_count, ..
    } = purpose
    else {
        panic!("Brainstorm must carry hand-to-library coordinator metadata")
    };
    assert_eq!(*total_count, 2);
    *total_count = 3;
    assert_invalid_private_prompt_before_move(&mut total, brainstorm);

    for tampered_index in [1_u16, u16::MAX] {
        let (mut prompt, brainstorm, _) = ready_at_first_prompt();
        let PendingEffectChoice::SelectTargets { path, purpose, .. } = prompt
            .engine
            .pending_effect
            .as_mut()
            .unwrap()
            .choice
            .as_mut()
            .unwrap()
        else {
            panic!("Brainstorm must be waiting for a hand target")
        };
        let EffectTargetSelectionPurpose::PutHandCardOnLibraryTop { prompt_index, .. } = purpose
        else {
            panic!("Brainstorm must carry hand-to-library coordinator metadata")
        };
        assert_eq!(*prompt_index, 0);
        assert_eq!(path.as_slice(), &[1, 0]);
        *prompt_index = tampered_index;
        *path.last_mut().unwrap() = tampered_index;
        assert_invalid_private_prompt_before_move(&mut prompt, brainstorm);
    }
}

#[test]
fn brainstorm_answered_frame_inconsistent_metadata_halts_before_private_move() {
    #[derive(Clone, Copy)]
    enum Tamper {
        Remaining,
        PromptIndex(u16),
        Total,
    }

    for tamper in [
        Tamper::Remaining,
        Tamper::PromptIndex(1),
        Tamper::PromptIndex(u16::MAX),
        Tamper::Total,
    ] {
        let (mut state, brainstorm, library) = ready_at_first_prompt();
        answer_hand_choice(&mut state, library[0]);
        let frame = state
            .engine
            .pending_effect
            .as_mut()
            .unwrap()
            .frames
            .last_mut()
            .unwrap();
        let EffectFrame::PutCardsFromHandOnLibraryTop {
            total,
            remaining,
            prompt_index,
            chosen,
            ..
        } = frame
        else {
            panic!("Brainstorm answer must leave a private commit frame")
        };
        assert_eq!((*total, *remaining, *prompt_index), (2, 2, 0));
        assert!(chosen.is_some());
        match tamper {
            Tamper::Remaining => *remaining = 3,
            Tamper::PromptIndex(index) => *prompt_index = index,
            Tamper::Total => *total = 3,
        }

        let history_start = state.engine.event_history.len();
        assert_invalid_continuation(&mut state, brainstorm);
        assert_no_hand_to_library_events_since(&state, history_start);
        assert_eq!(state.objects.get(library[0]).zone, Zone::Hand);
        assert_eq!(state.players[0].library, vec![library[3]]);
    }
}

#[test]
fn brainstorm_answered_frame_is_snapshot_stable_and_rejects_stale_incarnation() {
    let (mut state, brainstorm, library) = ready_at_first_prompt();
    answer_hand_choice(&mut state, library[0]);
    let answered_snapshot = state.snapshot();
    let answered_hash = state.state_hash();
    let expected_second = immediate_brainstorm_choice(&mut state, brainstorm);
    let expected_second_actions = hand_choice_actions(&state, &expected_second);

    state.restore(&answered_snapshot);
    assert_eq!(state.state_hash(), answered_hash);
    let restored_second = engine::advance_until_decision(&mut state);
    assert_eq!(
        hand_choice_actions(&state, &restored_second),
        expected_second_actions
    );

    state.restore(&answered_snapshot);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(library[0], Zone::Graveyard),
    );
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(library[0], Zone::Hand),
    );
    let history_len = state.engine.event_history.len();
    assert_invalid_continuation(&mut state, brainstorm);
    assert!(state.engine.event_history[history_len..]
        .iter()
        .all(|event| {
            !matches!(
                event,
                CommittedEvent::ZoneChange {
                    object,
                    from: Zone::Hand,
                    to: Zone::Library,
                    ..
                } if *object == library[0]
            )
        }));
}

#[test]
fn a_countered_brainstorm_does_not_draw_or_create_private_choices() {
    let (mut state, brainstorm, library, _) = ready_brainstorm(&DRAW_CARDS, &[]);
    put_object(&mut state, PlayerId::P1, "Island", Zone::Battlefield);
    put_object(&mut state, PlayerId::P1, "Island", Zone::Battlefield);
    let counterspell = put_object(&mut state, PlayerId::P1, "Counterspell", Zone::Hand);
    cast_brainstorm(&mut state, brainstorm);

    let mut counter_announced = false;
    for _ in 0..16 {
        let decision = engine::advance_until_decision(&mut state);
        if state.objects.get(brainstorm).zone == Zone::Graveyard
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
                assert!(legal_targets.contains(&Target::Object(brainstorm)));
                engine::step(&mut state, Action::ChooseTarget(Target::Object(brainstorm))).unwrap();
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
    assert!(state.engine.pending_effect.is_none());
    assert_eq!(state.objects.get(brainstorm).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(counterspell).zone, Zone::Graveyard);
    assert!(state.engine.event_history.iter().all(|event| !matches!(
        event,
        CommittedEvent::Draw {
            player: PlayerId::P0,
            ..
        }
    )));
}
