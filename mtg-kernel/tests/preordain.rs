//! Focused integration coverage for Preordain's schema-neutral scry
//! substrate. The exhaustive matrix executes generic inline scry programs,
//! while registered-spell tests cast the promoted Preordain through ordinary
//! mana payment, priority, generated resolution, and countering.
//!
//! XMage's AIRL chooser displays STOP before object actions for the initial
//! subset. Schema-v4 deliberately retains its canonical object-actions-first,
//! Finish-last order. Assertions compare semantic stable object identities,
//! not AIRL candidate indices.

use mtg_kernel::card_def::{card_id_by_name, CARD_DEFS};
use mtg_kernel::effect::{
    EffectFrame, EffectOp, EffectTargetSelectionPurpose, PlayerRef, ScryProgress,
    ScrySelectionStage,
};
use mtg_kernel::engine::{self, Action, Decision, UnsupportedMechanic};
use mtg_kernel::event::CommittedEvent;
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::rl::{
    legal_action_candidates_v1, observe_v2, ActionSemanticV1, PendingEffectChoiceSemanticV4,
    PlayerSeatV1, TargetRefV1, TargetSelectionPurposeV4,
};
use mtg_kernel::state::{
    Counters, GameObject, GameState, StackItem, StackItemKind, StackStateV4, Step, Target, Zone,
};
use mtg_kernel::surface_v2::{HarnessSurfaceV2, SurfaceDecision};

const FOUR: [&str; 4] = ["Fiery Temper", "Lava Dart", "Lightning Bolt", "Mountain"];

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
        Zone::Stack => panic!("test helper does not create stack-zone objects"),
    }
    id
}

/// Consumes exactly one priority round: P0's `CastSpellOrPass`, then P1's,
/// each answered with `Pass`. Two consecutive passes resolve the top of the
/// stack, so calling this once resolves whatever is currently on top (a
/// cast spell) and calling it again resolves whatever that left behind (a
/// freshly queued triggered ability), matching the fixed number of real
/// priority rounds each staged program needs -- not an unbounded "keep
/// passing" loop, which would silently walk straight through an
/// automatically-completing resolution (e.g. an empty-library scry that
/// never pauses) into the next turn's phases.
fn pass_priority_round(state: &mut GameState) {
    assert!(matches!(
        engine::advance_until_decision(state),
        Decision::CastSpellOrPass {
            player: PlayerId::P0,
            ..
        }
    ));
    engine::step(state, Action::Pass).unwrap();
    assert!(matches!(
        engine::advance_until_decision(state),
        Decision::CastSpellOrPass {
            player: PlayerId::P1,
            ..
        }
    ));
    engine::step(state, Action::Pass).unwrap();
}

/// Stages a scry (optionally followed by a draw) so the caller's own
/// `advance_until_decision` reaches the scry decision. `triggered_stack_
/// item_expected_target_spec`'s definition-ownership gate (engine.rs) now
/// requires a `TriggeredAbility` stack item's inline effect to be a literal
/// match to a registered `TriggeredAbilityDef` on its source. This 165-card
/// pool has exactly three cards that ever emit `EffectOp::Scry`: Faerie
/// Seer's ETB trigger (`Scry { count: 2 }` alone), Preordain's own spell
/// (`Sequence([Scry { count: 2 }, DrawCards { count: 1 }])`), and Lembas's
/// ETB trigger (`Sequence([Scry { count: 1 }, DrawCards { count: 1 }])`,
/// never a bare one-card scry). `count == 2` therefore casts the real
/// matching card through the engine's real cast path (Faerie Seer when
/// `draw_after` is false, Preordain -- the same registered program
/// `ready_registered_preordain` exercises -- when it is true) and drives
/// priority to the point where the caller's `advance_until_decision`
/// resolves it into the scry decision, exactly as real play produces it.
/// Every other `count` (only `3`, from `scry_three_fails_before_prefix_
/// binding_reveal_or_state_mutation`) has no definition-owned source at
/// all in this pool; that test only requires the fail-closed halt the
/// definition-ownership gate itself now produces, so the original
/// hand-built stack item is kept unchanged for it.
fn ready_scry(
    library_names: &[&str],
    count: u8,
    draw_after: bool,
) -> (GameState, ObjectId, Vec<ObjectId>) {
    let library_defs = library_names
        .iter()
        .map(|name| card_id(name))
        .collect::<Vec<_>>();
    let p1_library = [card_id("Snow-Covered Forest")];
    let mut state =
        GameState::new_from_libraries(&library_defs, &p1_library, card_name, 0x5052_454F_5244_4149);
    state.step = Step::Main1;
    state.active_player = PlayerId::P0;
    state.priority_player = PlayerId::P0;
    let library = state.players[0].library.clone();
    let island = put_object(&mut state, PlayerId::P0, "Island", Zone::Battlefield);

    if count != 2 {
        let scry = EffectOp::Scry {
            player: PlayerRef::Controller,
            count,
        };
        let program = if draw_after {
            EffectOp::Sequence(vec![
                scry,
                EffectOp::DrawCards {
                    player: PlayerRef::Controller,
                    count: 1,
                },
            ])
        } else {
            scry
        };
        state.stack.push(StackItem {
            kind: StackItemKind::TriggeredAbility,
            source: island,
            controller: PlayerId::P0,
            targets: Vec::new(),
            is_copy: false,
            inline_effect: Some(program),
            discarded: Vec::new(),
            is_flashback: false,
            mode_chosen: 0,
            madness_offer: false,
            kicked: false,
            v4: StackStateV4 {
                ability_source_contract: Some(mtg_kernel::state::AbilitySourceContractV4::capture(
                    &state, island,
                )),
                ..StackStateV4::default()
            },
        });
        state.engine.priority_passes = [true, true];
        return (state, island, library);
    }

    if draw_after {
        // Preordain is itself the stack item that resolves into the scry
        // decision (or, with an empty library, straight past it): one
        // priority round.
        let preordain = put_object(&mut state, PlayerId::P0, "Preordain", Zone::Hand);
        engine::step(&mut state, Action::CastSpell(preordain)).unwrap();
        pass_priority_round(&mut state);
        (state, preordain, library)
    } else {
        // Faerie Seer's ETB trigger is a second, separately-queued stack
        // item: one priority round resolves the creature spell (queuing the
        // trigger), a second resolves that trigger into the scry decision
        // (or, with an empty library, straight past it).
        let seer = put_object(&mut state, PlayerId::P0, "Faerie Seer", Zone::Hand);
        engine::step(&mut state, Action::CastSpell(seer)).unwrap();
        pass_priority_round(&mut state);
        pass_priority_round(&mut state);
        (state, seer, library)
    }
}

fn ready_registered_preordain(
    library_names: &[&str],
) -> (GameState, ObjectId, ObjectId, Vec<ObjectId>) {
    let library_defs = library_names
        .iter()
        .map(|name| card_id(name))
        .collect::<Vec<_>>();
    let p1_library = [card_id("Snow-Covered Forest")];
    let mut state =
        GameState::new_from_libraries(&library_defs, &p1_library, card_name, 0x5052_454F_5244_4341);
    state.step = Step::Main1;
    state.active_player = PlayerId::P0;
    state.priority_player = PlayerId::P0;
    let island = put_object(&mut state, PlayerId::P0, "Island", Zone::Battlefield);
    let preordain = put_object(&mut state, PlayerId::P0, "Preordain", Zone::Hand);
    let library = state.players[0].library.clone();
    (state, preordain, island, library)
}

fn cast_registered_preordain(state: &mut GameState, preordain: ObjectId) {
    engine::step(state, Action::CastSpell(preordain)).unwrap();
    assert_eq!(state.objects.get(preordain).zone, Zone::Stack);
    assert!(state.stack.iter().any(|item| item.source == preordain));
}

fn next_registered_preordain_choice(state: &mut GameState, preordain: ObjectId) -> Decision {
    for _ in 0..16 {
        let decision = engine::advance_until_decision(state);
        match decision {
            choice @ Decision::ChooseEffectTargets { source, .. } if source == preordain => {
                return choice;
            }
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => panic!("unexpected decision before registered Preordain choice: {other:?}"),
        }
    }
    panic!("registered Preordain did not reach its scry choice")
}

fn choice_stage(state: &GameState) -> ScrySelectionStage {
    let purpose = match state
        .engine
        .pending_effect
        .as_ref()
        .and_then(|pending| pending.choice.as_ref())
        .expect("scry choice")
    {
        mtg_kernel::effect::PendingEffectChoice::SelectTargets { purpose, .. } => purpose,
        other => panic!("unexpected scry choice: {other:?}"),
    };
    match purpose {
        EffectTargetSelectionPurpose::ScryLibrary { stage, .. } => stage.clone(),
        other => panic!("unexpected effect purpose: {other:?}"),
    }
}

fn assert_scry_decision(
    decision: &Decision,
    source: ObjectId,
    selected: u16,
    min: u16,
    max: u16,
    legal: &[ObjectId],
    can_finish: bool,
) {
    assert!(matches!(
        decision,
        Decision::ChooseEffectTargets {
            player: PlayerId::P0,
            source: actual_source,
            selected_count,
            min_targets,
            max_targets,
            legal_targets,
            can_finish: actual_can_finish,
        } if *actual_source == source
            && *selected_count == selected
            && *min_targets == min
            && *max_targets == max
            && *actual_can_finish == can_finish
            && legal_targets == &legal.iter().copied().map(Target::Object).collect::<Vec<_>>()
    ));
}

fn actions(state: &GameState, decision: &Decision) -> Vec<(Option<ObjectId>, String)> {
    legal_action_candidates_v1(&SurfaceDecision::Decision(decision.clone()), state)
        .unwrap()
        .into_iter()
        .map(|candidate| {
            let object = match candidate.record.semantic {
                ActionSemanticV1::ChooseEffectTarget {
                    target: TargetRefV1::Object { object },
                    ..
                } => Some(ObjectId(object.arena_id)),
                ActionSemanticV1::FinishEffectSelection { .. } => None,
                other => panic!("unexpected scry action: {other:?}"),
            };
            (object, candidate.record.stable_id)
        })
        .collect()
}

fn choose(state: &mut GameState, object: ObjectId) {
    engine::step(state, Action::ChooseEffectTarget(Target::Object(object))).unwrap();
}

fn finish(state: &mut GameState) {
    engine::step(state, Action::FinishEffectSelection).unwrap();
}

fn run_truth_case(
    subset_pick_order: &[usize],
    bottom_first: Option<usize>,
    top_first: Option<usize>,
) -> (GameState, Vec<ObjectId>) {
    let (mut state, source, library) = ready_scry(&FOUR, 2, true);
    let mut decision = engine::advance_until_decision(&mut state);
    assert_scry_decision(&decision, source, 0, 0, 2, &library[..2], true);
    for &index in subset_pick_order {
        choose(&mut state, library[index]);
        if state
            .engine
            .pending_effect
            .as_ref()
            .and_then(|pending| pending.choice.as_ref())
            .is_some()
        {
            let _ = engine::advance_until_decision(&mut state);
        }
    }
    if state
        .engine
        .pending_effect
        .as_ref()
        .and_then(|pending| pending.choice.as_ref())
        .is_some()
    {
        finish(&mut state);
    }

    decision = engine::advance_until_decision(&mut state);
    if let Some(index) = bottom_first {
        assert!(matches!(
            choice_stage(&state),
            ScrySelectionStage::OrderBottom { .. }
        ));
        choose(&mut state, library[index]);
        decision = engine::advance_until_decision(&mut state);
    }
    if let Some(index) = top_first {
        assert!(matches!(
            choice_stage(&state),
            ScrySelectionStage::OrderRetainedTop { .. }
        ));
        choose(&mut state, library[index]);
        decision = engine::advance_until_decision(&mut state);
    }
    assert!(matches!(decision, Decision::CastSpellOrPass { .. }));
    assert!(state.engine.pending_effect.is_none());
    assert_eq!(state.stack.len(), 0);
    (state, library)
}

#[test]
fn preordain_six_case_truth_table_matches_bottom_and_top_order_directions() {
    struct TruthCase<'a> {
        subset: &'a [usize],
        bottom_first: Option<usize>,
        top_first: Option<usize>,
        drawn: usize,
        remaining: &'a [usize],
    }
    let cases = [
        TruthCase {
            subset: &[0, 1],
            bottom_first: Some(1),
            top_first: None,
            drawn: 2,
            remaining: &[3, 1, 0],
        },
        TruthCase {
            subset: &[0, 1],
            bottom_first: Some(0),
            top_first: None,
            drawn: 2,
            remaining: &[3, 0, 1],
        },
        TruthCase {
            subset: &[0],
            bottom_first: None,
            top_first: None,
            drawn: 1,
            remaining: &[2, 3, 0],
        },
        TruthCase {
            subset: &[1],
            bottom_first: None,
            top_first: None,
            drawn: 0,
            remaining: &[2, 3, 1],
        },
        TruthCase {
            subset: &[],
            bottom_first: None,
            top_first: Some(1),
            drawn: 0,
            remaining: &[1, 2, 3],
        },
        TruthCase {
            subset: &[],
            bottom_first: None,
            top_first: Some(0),
            drawn: 1,
            remaining: &[0, 2, 3],
        },
    ];
    for case in cases {
        let (state, library) = run_truth_case(case.subset, case.bottom_first, case.top_first);
        assert_eq!(state.players[0].hand, vec![library[case.drawn]]);
        assert_eq!(
            state.players[0].library,
            case.remaining
                .iter()
                .map(|&index| library[index])
                .collect::<Vec<_>>()
        );
        assert_eq!(state.players[0].draws_this_turn, 1);
        assert!(!state.players[0].drew_from_empty);
    }
}

#[test]
fn registered_preordain_cast_executes_generated_bottom_a_then_draw_b_truth_path() {
    let (mut state, preordain, island, library) = ready_registered_preordain(&FOUR);
    let definition = &CARD_DEFS[card_id("Preordain") as usize];
    assert_eq!(
        (definition.spell_effect)(),
        Some(EffectOp::Sequence(vec![
            EffectOp::Scry {
                player: PlayerRef::Controller,
                count: 2,
            },
            EffectOp::DrawCards {
                player: PlayerRef::Controller,
                count: 1,
            },
        ])),
        "the registered spell must use the generated scry-two then draw-one program"
    );

    cast_registered_preordain(&mut state, preordain);
    let subset = next_registered_preordain_choice(&mut state, preordain);
    assert!(
        state.objects.get(island).tapped,
        "the ordinary cast pays Preordain's blue mana cost"
    );
    assert_scry_decision(&subset, preordain, 0, 0, 2, &library[..2], true);

    // Truth-table branch: bottom {A}; before draw this is B,C,D,A, so the
    // generated trailing DrawCards operation draws B and leaves C,D,A.
    choose(&mut state, library[0]);
    let after_a = engine::advance_until_decision(&mut state);
    assert_scry_decision(&after_a, preordain, 1, 0, 2, &[library[1]], true);
    finish(&mut state);
    let final_decision = engine::advance_until_decision(&mut state);

    assert!(matches!(final_decision, Decision::CastSpellOrPass { .. }));
    assert_eq!(state.players[0].hand, vec![library[1]]);
    assert_eq!(
        state.players[0].library,
        vec![library[2], library[3], library[0]]
    );
    assert_eq!(state.players[0].draws_this_turn, 1);
    assert_eq!(state.objects.get(preordain).zone, Zone::Graveyard);
    assert_eq!(state.players[0].graveyard, vec![preordain]);
    assert!(state.engine.pending_effect.is_none());
    assert!(state.engine.event_history.iter().any(|event| matches!(
        event,
        CommittedEvent::Draw {
            player: PlayerId::P0,
            object: Some(object),
        } if *object == library[1]
    )));
}

#[test]
fn scry_subset_is_unordered_and_schema_v4_actions_are_objects_then_finish() {
    let (mut first, source, library) = ready_scry(&FOUR, 2, false);
    let decision = engine::advance_until_decision(&mut first);
    let first_actions = actions(&first, &decision);
    assert_eq!(
        first_actions.iter().map(|(object, _)| *object).collect::<Vec<_>>(),
        vec![Some(library[0]), Some(library[1]), None],
        "kernel action order is object-actions-first and Finish-last; AIRL STOP-first index parity is intentionally not claimed"
    );
    assert_eq!(
        first_actions,
        vec![
            (
                // Incidental: the stable id folds in the ability source
                // object, so moving the scry decision's source from a
                // synthetic Island to the real Faerie Seer card (see
                // ready_scry) changes these literal hashes; the decision
                // shape they identify (the two scried objects, then Finish)
                // is unchanged.
                Some(library[0]),
                "legal-action-v4:65e5906d5621bdc3".to_string(),
            ),
            (
                Some(library[1]),
                "legal-action-v4:54780d9cb60b8bc2".to_string(),
            ),
            (None, "legal-action-v4:3d0cd129687231eb".to_string()),
        ],
        "literal stable IDs freeze semantic object/Finish mapping, not AIRL indices"
    );

    choose(&mut first, library[0]);
    let after_a = engine::advance_until_decision(&mut first);
    choose(&mut first, library[1]);
    let bottom_from_ab = engine::advance_until_decision(&mut first);
    assert!(matches!(
        choice_stage(&first),
        ScrySelectionStage::OrderBottom { .. }
    ));
    assert_scry_decision(&bottom_from_ab, source, 0, 2, 2, &library[..2], false);

    let (mut second, _, second_library) = ready_scry(&FOUR, 2, false);
    engine::advance_until_decision(&mut second);
    choose(&mut second, second_library[1]);
    engine::advance_until_decision(&mut second);
    choose(&mut second, second_library[0]);
    let bottom_from_ba = engine::advance_until_decision(&mut second);
    assert_eq!(
        actions(&first, &bottom_from_ab)
            .iter()
            .map(|(object, _)| *object)
            .collect::<Vec<_>>(),
        actions(&second, &bottom_from_ba)
            .iter()
            .map(|(object, _)| *object)
            .collect::<Vec<_>>(),
        "stage-one selection order cannot determine bottom order"
    );
    assert!(matches!(
        after_a,
        Decision::ChooseEffectTargets {
            selected_count: 1,
            can_finish: true,
            ..
        }
    ));
}

#[test]
fn scry_zero_one_two_and_duplicate_definitions_are_exact() {
    let (mut empty, _, _) = ready_scry(&[], 2, false);
    assert!(matches!(
        engine::advance_until_decision(&mut empty),
        Decision::CastSpellOrPass { .. }
    ));
    assert!(empty.engine.pending_effect.is_none());

    for bottom in [false, true] {
        let (mut one, source, library) = ready_scry(&FOUR[..1], 2, false);
        let choice = engine::advance_until_decision(&mut one);
        assert_scry_decision(&choice, source, 0, 0, 1, &library, true);
        if bottom {
            choose(&mut one, library[0]);
        } else {
            finish(&mut one);
        }
        let final_decision = engine::advance_until_decision(&mut one);
        assert!(matches!(final_decision, Decision::CastSpellOrPass { .. }));
        assert_eq!(one.players[0].library, library);
        assert!(one.engine.pending_effect.is_none());
    }

    let duplicate_names = ["Lightning Bolt", "Lightning Bolt", "Mountain"];
    let (mut duplicate, source, library) = ready_scry(&duplicate_names, 2, false);
    assert_ne!(library[0], library[1]);
    let subset = engine::advance_until_decision(&mut duplicate);
    assert_scry_decision(&subset, source, 0, 0, 2, &library[..2], true);
    choose(&mut duplicate, library[0]);
    finish(&mut duplicate);
    assert!(matches!(
        engine::advance_until_decision(&mut duplicate),
        Decision::CastSpellOrPass { .. }
    ));
    assert_eq!(
        duplicate.players[0].library,
        vec![library[1], library[2], library[0]]
    );
}

#[test]
fn scry_three_fails_before_prefix_binding_reveal_or_state_mutation() {
    let (mut state, source, library) = ready_scry(&FOUR, 3, false);
    state.reveal_library_top(PlayerId::P0, PlayerId::P0, 1);
    state.reveal_library_top(PlayerId::P1, PlayerId::P0, 4);
    let library_before = state.players[0].library.clone();
    let knowledge_before = state.library_knowledge.clone();
    let history_before = state.engine.event_history.clone();
    let pending_before = state.engine.pending_effect.clone();

    let decision = engine::advance_until_decision(&mut state);

    assert!(matches!(
        decision,
        Decision::Halted {
            mechanic: UnsupportedMechanic::InvalidEffectContinuation,
            source: actual_source,
        } if actual_source == source
    ));
    assert_eq!(state.players[0].library, library_before);
    assert_eq!(state.players[0].library, library);
    assert_eq!(state.library_knowledge, knowledge_before);
    assert_eq!(state.engine.event_history, history_before);
    assert_eq!(state.engine.pending_effect, pending_before);
    assert!(state.engine.pending_effect.is_none());
}

#[test]
fn scry_private_choices_redact_identities_and_update_knowledge_once() {
    let (mut private, _, _) = ready_scry(&FOUR, 2, false);
    let subset = engine::advance_until_decision(&mut private);
    let owner = observe_v2(&private, &HarnessSurfaceV2::new(), PlayerId::P0, 0).unwrap();
    let opponent = observe_v2(&private, &HarnessSurfaceV2::new(), PlayerId::P1, 0).unwrap();
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
            min_targets: 0,
            max_targets: 2,
            can_finish: true,
            ordered: false,
            purpose: TargetSelectionPurposeV4::CardSelection,
        } if structural_path == &vec![0]
            && selected_targets.is_empty()
            && legal_targets.len() == 2
    ));
    assert!(matches!(
        opponent
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
            purpose: TargetSelectionPurposeV4::CardSelection,
            ..
        } if selected_targets.is_empty() && legal_targets.is_empty()
    ));
    let opponent_json = serde_json::to_string(&opponent).unwrap();
    for name in ["Fiery Temper", "Lava Dart"] {
        assert!(!opponent_json.contains(name));
    }
    assert_eq!(actions(&private, &subset).len(), 3);
    let private_library = private.players[0].library.clone();
    choose(&mut private, private_library[0]);
    engine::advance_until_decision(&mut private);
    choose(&mut private, private_library[1]);
    engine::advance_until_decision(&mut private);
    let opponent_bottom = observe_v2(&private, &HarnessSurfaceV2::new(), PlayerId::P1, 1).unwrap();
    assert!(matches!(
        opponent_bottom
            .projection
            .engine_context
            .pending_effect
            .as_ref()
            .unwrap()
            .choice
            .as_ref()
            .unwrap(),
        PendingEffectChoiceSemanticV4::Targets {
            structural_path,
            selected_targets,
            legal_targets,
            purpose: TargetSelectionPurposeV4::LibraryOrder,
            ..
        } if structural_path == &vec![1]
            && selected_targets.is_empty()
            && legal_targets.is_empty()
    ));

    let (mut private_top, _, _) = ready_scry(&FOUR, 2, false);
    engine::advance_until_decision(&mut private_top);
    finish(&mut private_top);
    engine::advance_until_decision(&mut private_top);
    let opponent_top = observe_v2(&private_top, &HarnessSurfaceV2::new(), PlayerId::P1, 2).unwrap();
    assert!(matches!(
        opponent_top
            .projection
            .engine_context
            .pending_effect
            .as_ref()
            .unwrap()
            .choice
            .as_ref()
            .unwrap(),
        PendingEffectChoiceSemanticV4::Targets {
            structural_path,
            selected_targets,
            legal_targets,
            purpose: TargetSelectionPurposeV4::LibraryOrder,
            ..
        } if structural_path == &vec![2]
            && selected_targets.is_empty()
            && legal_targets.is_empty()
    ));

    let (mut state, _, library) = ready_scry(&FOUR, 2, false);
    state.reveal_library_top(PlayerId::P1, PlayerId::P0, 4);
    engine::advance_until_decision(&mut state);
    choose(&mut state, library[0]);
    finish(&mut state);
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    assert_eq!(
        state.players[0].library,
        vec![library[1], library[2], library[3], library[0]]
    );
    assert_eq!(
        state
            .known_library_cards(PlayerId::P0, PlayerId::P0)
            .iter()
            .map(|entry| (entry.position, entry.object))
            .collect::<Vec<_>>(),
        vec![(0, library[1]), (3, library[0])]
    );
    assert_eq!(
        state
            .known_library_cards(PlayerId::P1, PlayerId::P0)
            .iter()
            .map(|entry| (entry.position, entry.object))
            .collect::<Vec<_>>(),
        vec![(1, library[2]), (2, library[3])],
        "ambiguous private prefix facts vanish while untouched-tail facts shift by one"
    );

    // Lembas's real ETB trigger is a definition-owned one-card scry:
    // `Sequence([Scry { count: 1 }, DrawCards { count: 1 }])` (see
    // `lembas_etb_effect` in trigger.rs). `GameState::apply_scry_result`
    // (state.rs) keys its `prefix_len == 1` identity-preservation branch
    // purely on the bound scry prefix's length, not on what the calling
    // program does afterward, so that branch fires exactly the same way here
    // as it did for the hand-built bare `EffectOp::Scry { count: 1 }` this
    // block used to stage. This drives a real Lembas through the ordinary
    // cast path (paying its own `{2}` mana cost) so the source's stack
    // provenance is definition-owned, exactly like this file's other
    // modernized fixtures.
    //
    // Choosing the lone scry candidate completes the whole staged
    // `EffectContinuation` -- draw included -- before the engine's public
    // `step`/`advance_until_decision` API returns another decision, so the
    // scry-only intermediate library/knowledge state is not observable here.
    // The assertion below is therefore the honest composition of two
    // separately-proven laws: the scry's identity-preserving keep/bottom
    // branch, then the ordinary draw-time removal/shift
    // (`GameState::note_library_removal`) applied on top of it.
    let lembas_library_defs = FOUR[..3]
        .iter()
        .map(|name| card_id(name))
        .collect::<Vec<_>>();
    let lembas_p1_library = [card_id("Snow-Covered Forest")];
    let mut lembas_state = GameState::new_from_libraries(
        &lembas_library_defs,
        &lembas_p1_library,
        card_name,
        0x5052_454F_5244_4C42,
    );
    lembas_state.step = Step::Main1;
    lembas_state.active_player = PlayerId::P0;
    lembas_state.priority_player = PlayerId::P0;
    let lembas_library = lembas_state.players[0].library.clone();
    // Lembas costs {2}: two untapped generic sources, mirroring the single
    // Island `ready_scry` puts down for Preordain/Faerie Seer's {U}.
    put_object(&mut lembas_state, PlayerId::P0, "Island", Zone::Battlefield);
    put_object(&mut lembas_state, PlayerId::P0, "Island", Zone::Battlefield);
    let lembas = put_object(&mut lembas_state, PlayerId::P0, "Lembas", Zone::Hand);
    lembas_state.reveal_library_top(PlayerId::P1, PlayerId::P0, 3);

    engine::step(&mut lembas_state, Action::CastSpell(lembas)).unwrap();
    // One priority round resolves the Lembas artifact spell onto the
    // battlefield (queuing its ETB trigger); a second resolves that trigger
    // into the scry decision, exactly like `ready_scry`'s Faerie Seer path.
    pass_priority_round(&mut lembas_state);
    pass_priority_round(&mut lembas_state);
    let scry_decision = engine::advance_until_decision(&mut lembas_state);
    assert_scry_decision(&scry_decision, lembas, 0, 0, 1, &lembas_library[..1], true);

    // Same keep-or-bottom choice the original block made: bottom the one
    // scried card.
    choose(&mut lembas_state, lembas_library[0]);
    assert!(matches!(
        engine::advance_until_decision(&mut lembas_state),
        Decision::CastSpellOrPass { .. }
    ));
    // What the trailing draw consumed: it drew the card scry left on top
    // (lembas_library[1]) and left the bottomed card (lembas_library[0])
    // beneath the untouched tail (lembas_library[2]).
    assert_eq!(lembas_state.players[0].hand, vec![lembas_library[1]]);
    assert_eq!(
        lembas_state.players[0].library,
        vec![lembas_library[2], lembas_library[0]]
    );
    assert_eq!(
        lembas_state
            .known_library_cards(PlayerId::P1, PlayerId::P0)
            .iter()
            .map(|entry| (entry.position, entry.object))
            .collect::<Vec<_>>(),
        vec![(0, lembas_library[2]), (1, lembas_library[0])],
        "a one-card scry preserves already-known deterministic identities \
         exactly; the trailing draw then removes the fact for the card it \
         drew and shifts the remaining known identities shallower by one, \
         like any other draw"
    );
}

#[test]
fn scry_snapshot_restore_is_exact_at_subset_bottom_and_top_stages() {
    let (mut state, _, library) = ready_scry(&FOUR, 2, false);
    let subset = engine::advance_until_decision(&mut state);
    let subset_snapshot = state.snapshot();
    let subset_hash = state.state_hash();

    choose(&mut state, library[0]);
    engine::advance_until_decision(&mut state);
    choose(&mut state, library[1]);
    let _bottom = engine::advance_until_decision(&mut state);
    assert!(matches!(
        choice_stage(&state),
        ScrySelectionStage::OrderBottom { .. }
    ));
    let bottom_snapshot = state.snapshot();
    let bottom_hash = state.state_hash();
    choose(&mut state, library[1]);
    engine::advance_until_decision(&mut state);
    let expected_bottom_result = state.players[0].library.clone();

    state.restore(&bottom_snapshot);
    assert_eq!(state.state_hash(), bottom_hash);
    choose(&mut state, library[1]);
    engine::advance_until_decision(&mut state);
    assert_eq!(state.players[0].library, expected_bottom_result);

    state.restore(&subset_snapshot);
    assert_eq!(state.state_hash(), subset_hash);
    assert_eq!(actions(&state, &subset).len(), 3);
    finish(&mut state);
    let top = engine::advance_until_decision(&mut state);
    assert!(matches!(
        choice_stage(&state),
        ScrySelectionStage::OrderRetainedTop { .. }
    ));
    let top_snapshot = state.snapshot();
    let top_hash = state.state_hash();
    choose(&mut state, library[1]);
    engine::advance_until_decision(&mut state);
    let expected_top_result = state.players[0].library.clone();

    state.restore(&top_snapshot);
    assert_eq!(state.state_hash(), top_hash);
    assert_eq!(actions(&state, &top).len(), 2);
    choose(&mut state, library[1]);
    engine::advance_until_decision(&mut state);
    assert_eq!(state.players[0].library, expected_top_result);
}

fn assert_rejected_without_action_mutation(mut tampered: GameState, action: Action) {
    let before = tampered.clone();
    assert!(engine::step(&mut tampered, action).is_err());
    assert_eq!(tampered, before);
}

#[test]
fn scry_pending_choices_reject_chooser_partition_prefix_incarnation_and_progress_tamper() {
    let (mut state, _, library) = ready_scry(&FOUR, 2, false);
    engine::advance_until_decision(&mut state);

    let mut chooser = state.clone();
    let mtg_kernel::effect::PendingEffectChoice::SelectTargets { player, .. } = chooser
        .engine
        .pending_effect
        .as_mut()
        .unwrap()
        .choice
        .as_mut()
        .unwrap()
    else {
        unreachable!()
    };
    *player = PlayerId::P1;
    assert_rejected_without_action_mutation(chooser, Action::FinishEffectSelection);

    let mut partition = state.clone();
    let mtg_kernel::effect::PendingEffectChoice::SelectTargets { legal, .. } = partition
        .engine
        .pending_effect
        .as_mut()
        .unwrap()
        .choice
        .as_mut()
        .unwrap()
    else {
        unreachable!()
    };
    legal.pop();
    assert_rejected_without_action_mutation(partition, Action::FinishEffectSelection);

    let mut path_state = state.clone();
    let mtg_kernel::effect::PendingEffectChoice::SelectTargets {
        path: structural_path,
        ..
    } = path_state
        .engine
        .pending_effect
        .as_mut()
        .unwrap()
        .choice
        .as_mut()
        .unwrap()
    else {
        unreachable!()
    };
    structural_path.push(99);
    assert_rejected_without_action_mutation(path_state, Action::FinishEffectSelection);

    let mut metadata = state.clone();
    let mtg_kernel::effect::PendingEffectChoice::SelectTargets { purpose, .. } = metadata
        .engine
        .pending_effect
        .as_mut()
        .unwrap()
        .choice
        .as_mut()
        .unwrap()
    else {
        unreachable!()
    };
    let EffectTargetSelectionPurpose::ScryLibrary {
        requested_count, ..
    } = purpose
    else {
        unreachable!()
    };
    *requested_count = 3;
    assert_rejected_without_action_mutation(metadata, Action::FinishEffectSelection);

    let mut prefix = state.clone();
    prefix.players[0].library.swap(0, 1);
    assert_rejected_without_action_mutation(prefix, Action::FinishEffectSelection);

    let mut incarnation = state.clone();
    incarnation.objects.get_mut(library[0]).zone_change_count += 1;
    assert_rejected_without_action_mutation(incarnation, Action::FinishEffectSelection);

    choose(&mut state, library[0]);
    engine::advance_until_decision(&mut state);
    choose(&mut state, library[1]);
    engine::advance_until_decision(&mut state);
    let mut bottom_shape = state.clone();
    let mtg_kernel::effect::PendingEffectChoice::SelectTargets { min_targets, .. } = bottom_shape
        .engine
        .pending_effect
        .as_mut()
        .unwrap()
        .choice
        .as_mut()
        .unwrap()
    else {
        unreachable!()
    };
    *min_targets = 0;
    assert_rejected_without_action_mutation(
        bottom_shape,
        Action::ChooseEffectTarget(Target::Object(library[0])),
    );
}

#[test]
fn scry_answered_frames_fail_before_library_or_event_mutation() {
    let (mut state, _, library) = ready_scry(&FOUR, 2, false);
    engine::advance_until_decision(&mut state);
    finish(&mut state);
    let frame_snapshot = state.clone();
    let history = state.engine.event_history.clone();
    let original_library = state.players[0].library.clone();

    let mut path_state = frame_snapshot.clone();
    let Some(EffectFrame::ScryLibrary {
        path: structural_path,
        ..
    }) = path_state
        .engine
        .pending_effect
        .as_mut()
        .unwrap()
        .frames
        .last_mut()
    else {
        unreachable!()
    };
    structural_path.push(9);
    engine::advance_until_decision(&mut path_state);
    assert_eq!(path_state.players[0].library, original_library);
    assert_eq!(path_state.engine.event_history, history);
    assert!(matches!(
        path_state.engine.halted,
        Some((UnsupportedMechanic::InvalidEffectContinuation, _))
    ));

    let mut progress = frame_snapshot;
    let Some(EffectFrame::ScryLibrary {
        progress: scry_progress,
        ..
    }) = progress
        .engine
        .pending_effect
        .as_mut()
        .unwrap()
        .frames
        .last_mut()
    else {
        unreachable!()
    };
    *scry_progress = ScryProgress::BottomSubsetChosen {
        bottom_subset: vec![mtg_kernel::effect::EffectObjectBinding {
            // This is a structurally valid alternative subset, not merely an
            // out-of-prefix corruption. The untouched redundant fingerprint
            // must still reject the isolated progress substitution.
            object: library[0],
            expected_zone: Zone::Library,
            expected_zone_change_count: 0,
        }],
    };
    engine::advance_until_decision(&mut progress);
    assert_eq!(progress.players[0].library, original_library);
    assert_eq!(progress.engine.event_history, history);
    assert!(matches!(
        progress.engine.halted,
        Some((UnsupportedMechanic::InvalidEffectContinuation, _))
    ));
}

#[test]
fn scry_stages_do_not_open_priority_or_sba_and_empty_draw_loses_after_resolution() {
    let (mut staged, source, library) = ready_scry(&FOUR, 2, true);
    let initial_priority = staged.priority_player;
    engine::advance_until_decision(&mut staged);
    choose(&mut staged, library[0]);
    engine::advance_until_decision(&mut staged);
    choose(&mut staged, library[1]);
    let bottom = engine::advance_until_decision(&mut staged);
    assert!(
        matches!(bottom, Decision::ChooseEffectTargets { source: actual, .. } if actual == source)
    );
    assert_eq!(staged.priority_player, initial_priority);
    assert_eq!(staged.stack.last().map(|item| item.source), Some(source));
    assert!(!staged.players[0].has_lost);

    let (mut empty, _, _) = ready_scry(&[], 2, true);
    let terminal = engine::advance_until_decision(&mut empty);
    assert!(matches!(
        terminal,
        Decision::GameOver {
            winner: Some(PlayerId::P1)
        }
    ));
    assert!(empty.players[0].drew_from_empty);
    assert!(empty.players[0].has_lost);
    assert!(empty.engine.pending_effect.is_none());
}

#[test]
fn a_countered_registered_preordain_does_not_scry_draw_or_change_library_knowledge() {
    let (mut state, preordain, _, library) = ready_registered_preordain(&FOUR);
    put_object(&mut state, PlayerId::P1, "Island", Zone::Battlefield);
    put_object(&mut state, PlayerId::P1, "Island", Zone::Battlefield);
    let counterspell = put_object(&mut state, PlayerId::P1, "Counterspell", Zone::Hand);
    state.reveal_library_top(PlayerId::P0, PlayerId::P0, 2);
    state.reveal_library_top(PlayerId::P1, PlayerId::P0, 4);
    let knowledge_before = state.library_knowledge.clone();
    cast_registered_preordain(&mut state, preordain);

    let mut counter_announced = false;
    for _ in 0..16 {
        let decision = engine::advance_until_decision(&mut state);
        if state.objects.get(preordain).zone == Zone::Graveyard
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
                assert!(legal_targets.contains(&Target::Object(preordain)));
                engine::step(&mut state, Action::ChooseTarget(Target::Object(preordain))).unwrap();
            }
            Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
            other => panic!("unexpected registered Preordain counter decision: {other:?}"),
        }
    }

    assert!(counter_announced);
    assert_eq!(state.players[0].library, library);
    assert!(state.players[0].hand.is_empty());
    assert_eq!(state.players[0].draws_this_turn, 0);
    assert!(!state.players[0].drew_from_empty);
    assert_eq!(state.library_knowledge, knowledge_before);
    assert!(state.engine.pending_effect.is_none());
    assert_eq!(state.objects.get(preordain).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(counterspell).zone, Zone::Graveyard);
    assert!(state.engine.event_history.iter().all(|event| !matches!(
        event,
        CommittedEvent::Draw {
            player: PlayerId::P0,
            ..
        }
    )));
}
