//! Printed surveil 2/3 windows, private choices and exact pending restores.
#![cfg(feature = "limited-fdn-fixtures")]

use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardType, Keywords, Subtype, CARD_DEFS,
};
use mtg_kernel::effect::{EffectOp, EffectTargetSelectionPurpose, PendingEffectChoice};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::policy_surface_v5::PolicySurfaceV5;
use mtg_kernel::rl::{
    observe_policy_v6, observe_v2, PendingEffectChoiceSemanticV4, TargetSelectionPurposeV4,
};
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};
use mtg_kernel::surface_v2::HarnessSurfaceV2;
use mtg_kernel::trigger;

fn ready(size: usize) -> GameState {
    let names = ["Forest", "Plains", "Island", "Mountain", "Swamp"];
    let cards = names[..size]
        .iter()
        .map(|name| card_id_by_name(name).unwrap())
        .collect::<Vec<_>>();
    let forest = card_id_by_name("Forest").unwrap();
    let mut state = GameState::new_from_libraries(
        &cards,
        &[forest; 5],
        |id| CARD_DEFS[id as usize].name.into(),
        123,
    );
    state.step = Step::Main1;
    state
}

fn put(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id_by_name(name).unwrap();
    let id = state.objects.push(GameObject {
        card_def,
        name: name.into(),
        owner: player,
        controller: player,
        zone,
        tapped: false,
        summoning_sick: false,
        damage: 0,
        counters: Default::default(),
        attachments: Vec::new(),
        v4: ObjectStateV4::from_card_def(card_def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    match zone {
        Zone::Hand => state.players[player.index()].hand.push(id),
        Zone::Battlefield => state.players[player.index()].battlefield.push(id),
        Zone::Graveyard => state.players[player.index()].graveyard.push(id),
        _ => panic!("helper zone"),
    }
    id
}

fn settle(state: &mut GameState) -> Option<Decision> {
    for _ in 0..100 {
        let decision = engine::advance_until_decision(state);
        match decision {
            Decision::OrderTriggers { pending, .. } => {
                engine::step(state, Action::OrderTriggers((0..pending.len()).collect())).unwrap()
            }
            Decision::CastSpellOrPass { .. }
                if state.stack.is_empty() && state.engine.pending_triggers.is_empty() =>
            {
                return None
            }
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            Decision::Halted { .. } => panic!("{decision:?}"),
            other => return Some(other),
        }
    }
    panic!("resolution did not settle")
}

fn enter(state: &mut GameState, name: &str) -> ObjectId {
    let source = put(state, PlayerId::P0, name, Zone::Hand);
    let definition = &CARD_DEFS[state.objects.get(source).card_def as usize];
    state.players[0].mana_pool[ManaColor::U.pool_index()] = 1;
    state.players[0].mana_pool[5] = definition.cost.generic;
    assert!(
        matches!(engine::advance_until_decision(state), Decision::CastSpellOrPass { castable_spells, .. } if castable_spells.contains(&source))
    );
    engine::step(state, Action::CastSpell(source)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(state),
        Decision::CastSpellOrPass { .. }
    ));
    assert_eq!(state.players[0].mana_pool, [0; 6]);
    source
}

fn choose_order(state: &mut GameState, objects: &[ObjectId]) {
    for (index, &object) in objects.iter().enumerate() {
        if state
            .engine
            .pending_effect
            .as_ref()
            .is_some_and(|pending| pending.choice.is_none())
        {
            // The engine automatically appends the forced final ordering card.
            assert_eq!(index + 1, objects.len());
            break;
        }
        engine::step(state, Action::ChooseEffectTarget(Target::Object(object))).unwrap();
    }
    if state
        .engine
        .pending_effect
        .as_ref()
        .is_some_and(|pending| pending.choice.is_some())
    {
        engine::step(state, Action::FinishEffectSelection).unwrap();
    }
}

fn restored(state: &GameState) -> GameState {
    serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
}

#[test]
fn exact_printed_definitions_and_complete_abilities() {
    for (name, id, generic, stats, subtypes, triggers) in [
        (
            "Lightshell Duo",
            330,
            3,
            (3, 4),
            &[Subtype::Rat, Subtype::Otter][..],
            2,
        ),
        (
            "Cephalid Inkmage",
            331,
            2,
            (2, 2),
            &[Subtype::Octopus, Subtype::Wizard][..],
            1,
        ),
    ] {
        assert_eq!(card_id_by_name(name), Some(id));
        let card = &CARD_DEFS[id as usize];
        assert_eq!(card.capability, CardCapability::Full);
        assert_eq!(card.cost.generic, generic);
        assert_eq!(card.cost.pips, &[Pip::Colored(ManaColor::U)]);
        assert_eq!(card.mana_value, u16::from(generic) + 1);
        assert_eq!(card.colors, &[ManaColor::U]);
        assert_eq!(card.types, &[CardType::Creature]);
        assert_eq!(card.subtypes, subtypes);
        assert_eq!((card.power, card.toughness), (Some(stats.0), Some(stats.1)));
        assert_eq!(trigger::triggers_for(id).len(), triggers);
    }
}

#[test]
fn every_three_card_partition_preserves_tail_and_supports_both_orders() {
    for mask in 0..8 {
        for reverse_kept in [false, true] {
            let mut state = ready(5);
            let original = state.players[0].library.clone();
            let source = enter(&mut state, "Cephalid Inkmage");
            let Some(Decision::ChooseEffectTargets {
                player,
                source: prompt_source,
                min_targets,
                max_targets,
                legal_targets,
                ..
            }) = settle(&mut state)
            else {
                panic!("surveil window");
            };
            assert_eq!(
                (player, prompt_source, min_targets, max_targets),
                (PlayerId::P0, source, 0, 3)
            );
            assert_eq!(
                legal_targets,
                original[..3]
                    .iter()
                    .copied()
                    .map(Target::Object)
                    .collect::<Vec<_>>()
            );
            let graveyard = original[..3]
                .iter()
                .enumerate()
                .rev()
                .filter(|(i, _)| mask & (1 << i) != 0)
                .map(|(_, id)| *id)
                .collect::<Vec<_>>();
            let mut kept = original[..3]
                .iter()
                .copied()
                .filter(|id| !graveyard.contains(id))
                .collect::<Vec<_>>();
            choose_order(&mut state, &graveyard);
            assert_eq!(state.players[0].library, original);
            assert!(state.players[0].graveyard.is_empty());
            if kept.len() >= 2 {
                assert!(
                    matches!(settle(&mut state), Some(Decision::ChooseEffectTargets { min_targets, max_targets, .. }) if usize::from(min_targets) == kept.len() && min_targets == max_targets)
                );
                assert_eq!(state.players[0].library, original);
                if reverse_kept {
                    kept.reverse();
                }
                choose_order(&mut state, &kept);
            }
            assert!(settle(&mut state).is_none());
            let mut expected = kept;
            expected.extend_from_slice(&original[3..]);
            assert_eq!(state.players[0].library, expected);
            assert_eq!(state.players[0].graveyard, graveyard);
        }
    }
}

#[test]
fn surveil_two_reads_two_distinct_cards_and_can_reverse_both_kept() {
    let mut state = ready(5);
    let original = state.players[0].library.clone();
    enter(&mut state, "Lightshell Duo");
    assert!(matches!(
        settle(&mut state),
        Some(Decision::ChooseEffectTargets { max_targets: 2, .. })
    ));
    choose_order(&mut state, &[]);
    assert!(matches!(
        settle(&mut state),
        Some(Decision::ChooseEffectTargets { max_targets: 2, .. })
    ));
    choose_order(&mut state, &[original[1], original[0]]);
    assert!(settle(&mut state).is_none());
    assert_eq!(
        state.players[0].library,
        [
            original[1],
            original[0],
            original[2],
            original[3],
            original[4]
        ]
    );
}

#[test]
fn empty_and_short_libraries_have_no_draw_or_loss() {
    for size in 0..3 {
        let mut state = ready(size);
        let original = state.players[0].library.clone();
        enter(&mut state, "Cephalid Inkmage");
        if size == 0 {
            assert!(settle(&mut state).is_none());
        } else {
            assert!(
                matches!(settle(&mut state), Some(Decision::ChooseEffectTargets { max_targets, .. }) if usize::from(max_targets) == size)
            );
            choose_order(&mut state, &original);
            assert!(settle(&mut state).is_none());
        }
        assert!(state.players[0].library.is_empty());
        assert_eq!(state.players[0].graveyard, original);
        assert_eq!(state.players[0].life, 20);
    }
}

#[test]
fn chooser_private_candidates_and_shape_stay_hidden_at_both_stages() {
    let mut state = ready(5);
    let original = state.players[0].library.clone();
    enter(&mut state, "Cephalid Inkmage");
    settle(&mut state).unwrap();
    for stage in 0..2 {
        if stage != 0 {
            settle(&mut state).unwrap();
        };
        let own = observe_v2(&state, &HarnessSurfaceV2::new(), PlayerId::P0, 0).unwrap();
        let other = observe_v2(&state, &HarnessSurfaceV2::new(), PlayerId::P1, 0).unwrap();
        let mut hidden_changed = restored(&state);
        let replacement = card_id_by_name("Lightning Bolt").unwrap();
        hidden_changed.objects.get_mut(original[0]).card_def = replacement;
        hidden_changed.objects.get_mut(original[0]).name = "Lightning Bolt".into();
        assert_eq!(
            other.visible_projection_hash,
            observe_v2(&hidden_changed, &HarnessSurfaceV2::new(), PlayerId::P1, 0)
                .unwrap()
                .visible_projection_hash
        );
        let Some(PendingEffectChoiceSemanticV4::Targets {
            legal_targets,
            purpose,
            ..
        }) = own.projection.engine_context.pending_effect.unwrap().choice
        else {
            panic!("chooser prompt");
        };
        assert_eq!(legal_targets.len(), 3 - stage);
        assert_eq!(
            purpose,
            if stage == 0 {
                TargetSelectionPurposeV4::CardSelection
            } else {
                TargetSelectionPurposeV4::LibraryOrder
            }
        );
        let Some(PendingEffectChoiceSemanticV4::Targets {
            legal_targets,
            selected_targets,
            min_targets,
            max_targets,
            can_finish,
            ..
        }) = other
            .projection
            .engine_context
            .pending_effect
            .unwrap()
            .choice
        else {
            panic!("nonchooser prompt");
        };
        assert!(legal_targets.is_empty() && selected_targets.is_empty());
        assert_eq!((min_targets, max_targets, can_finish), (0, 0, true));
        if stage == 0 {
            choose_order(&mut state, &[original[1]]);
        }
    }
}

#[test]
fn pending_and_answered_stages_restore_identically_before_any_moves() {
    for boundary in 0..4 {
        let mut state = ready(5);
        let original = state.players[0].library.clone();
        enter(&mut state, "Cephalid Inkmage");
        settle(&mut state).unwrap();
        if boundary >= 1 {
            choose_order(&mut state, &[original[1]]);
        }
        if boundary >= 2 {
            settle(&mut state).unwrap();
        }
        if boundary >= 3 {
            choose_order(&mut state, &[original[2], original[0]]);
        }
        let mut copy = restored(&state);
        for instance in [&mut state, &mut copy] {
            if boundary == 0 {
                choose_order(instance, &[original[1]]);
            }
            if boundary <= 1 {
                settle(instance).unwrap();
            }
            if boundary <= 2 {
                choose_order(instance, &[original[2], original[0]]);
            }
            assert!(settle(instance).is_none());
        }
        assert_eq!(state.state_hash(), copy.state_hash());
        assert_eq!(state.players[0].graveyard, [original[1]]);
        assert_eq!(
            state.players[0].library,
            [original[2], original[0], original[3], original[4]]
        );
    }
}

#[test]
fn nonchooser_public_and_typed_contexts_match_across_subsets_and_all_private_stages() {
    let mut state = ready(5);
    let original = state.players[0].library.clone();
    enter(&mut state, "Cephalid Inkmage");
    settle(&mut state).unwrap();
    let public = observe_v2(&state, &HarnessSurfaceV2::new(), PlayerId::P1, 0).unwrap();
    let typed =
        observe_policy_v6(&state, &PolicySurfaceV5::new(), PlayerId::P1, 0, 0, 0, 1).unwrap();
    let check = |instance: &GameState| {
        assert_eq!(
            public.visible_projection_hash,
            observe_v2(instance, &HarnessSurfaceV2::new(), PlayerId::P1, 0)
                .unwrap()
                .visible_projection_hash
        );
        let view =
            observe_policy_v6(instance, &PolicySurfaceV5::new(), PlayerId::P1, 0, 0, 0, 1).unwrap();
        assert_eq!(typed.projection, view.projection);
        assert_eq!(typed.extensions, view.extensions);
        assert_eq!(typed.visible_projection_hash, view.visible_projection_hash);
    };
    for mask in 0..8 {
        let mut copy = restored(&state);
        let graveyard = original[..3]
            .iter()
            .enumerate()
            .filter(|(i, _)| mask & (1 << i) != 0)
            .map(|(_, id)| *id)
            .collect::<Vec<_>>();
        let kept = original[..3]
            .iter()
            .copied()
            .filter(|id| !graveyard.contains(id))
            .collect::<Vec<_>>();
        for &card in &graveyard {
            engine::step(&mut copy, Action::ChooseEffectTarget(Target::Object(card))).unwrap();
            check(&copy);
        }
        if copy
            .engine
            .pending_effect
            .as_ref()
            .unwrap()
            .choice
            .is_some()
        {
            engine::step(&mut copy, Action::FinishEffectSelection).unwrap();
        }
        check(&copy);
        if kept.len() >= 2 {
            settle(&mut copy).unwrap();
            check(&copy);
            for (index, &card) in kept.iter().rev().enumerate() {
                if copy
                    .engine
                    .pending_effect
                    .as_ref()
                    .unwrap()
                    .choice
                    .is_some()
                {
                    engine::step(&mut copy, Action::ChooseEffectTarget(Target::Object(card)))
                        .unwrap();
                } else {
                    assert_eq!(index + 1, kept.len());
                }
                check(&copy);
            }
        }
    }
}

#[test]
fn tampered_count_program_source_or_library_and_unlooked_cards_fail_closed() {
    let mut original = ready(5);
    let source = enter(&mut original, "Lightshell Duo");
    settle(&mut original).unwrap();
    let tail = original.players[0].library[2];
    let hash = original.state_hash();
    assert!(engine::step(
        &mut original,
        Action::ChooseEffectTarget(Target::Object(tail))
    )
    .is_err());
    assert_eq!(original.state_hash(), hash);
    for mutation in 0..4 {
        let mut state = restored(&original);
        match mutation {
            0 => {
                let Some(PendingEffectChoice::SelectTargets {
                    purpose:
                        EffectTargetSelectionPurpose::SurveilLibraryMany {
                            requested_count, ..
                        },
                    ..
                }) = state
                    .engine
                    .pending_effect
                    .as_mut()
                    .unwrap()
                    .choice
                    .as_mut()
                else {
                    panic!("batch choice");
                };
                *requested_count = 3;
            }
            1 => {
                state
                    .engine
                    .pending_effect
                    .as_mut()
                    .unwrap()
                    .resolving_item
                    .inline_effect = Some(EffectOp::Sequence(Vec::new()));
            }
            2 => {
                state.objects.get_mut(source).card_def = card_id_by_name("Wary Thespian").unwrap();
            }
            3 => state.players[0].library.swap(0, 1),
            _ => unreachable!(),
        }
        let before = state.state_hash();
        assert!(
            engine::step(&mut state, Action::FinishEffectSelection).is_err(),
            "mutation {mutation}"
        );
        assert_eq!(state.state_hash(), before);
    }
}

#[test]
fn departed_source_surveil_finishes_without_requiring_source_on_battlefield() {
    let mut state = ready(5);
    let source = put(&mut state, PlayerId::P0, "Lightshell Duo", Zone::Hand);
    let original = state.players[0].library.clone();
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(source, Zone::Battlefield),
    );
    let pending = trigger::collect_and_process(&mut state);
    state.engine.pending_triggers.extend(pending);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(source, Zone::Graveyard),
    );
    assert!(matches!(
        settle(&mut state),
        Some(Decision::ChooseEffectTargets { max_targets: 2, .. })
    ));
    choose_order(&mut state, &[original[0], original[1]]);
    assert!(settle(&mut state).is_none());
    assert_eq!(
        state.players[0].graveyard,
        [source, original[0], original[1]]
    );
}

#[test]
fn threshold_counts_controller_cards_and_updates_immediately() {
    let mut state = ready(5);
    let source = put(
        &mut state,
        PlayerId::P0,
        "Cephalid Inkmage",
        Zone::Battlefield,
    );
    for _ in 0..6 {
        put(&mut state, PlayerId::P0, "Forest", Zone::Graveyard);
    }
    for _ in 0..7 {
        put(&mut state, PlayerId::P1, "Forest", Zone::Graveyard);
    }
    assert!(!engine::has_effective_keyword(
        &state,
        source,
        Keywords::CANT_BE_BLOCKED
    ));
    put(&mut state, PlayerId::P0, "Rat Token", Zone::Graveyard);
    assert!(!engine::has_effective_keyword(
        &state,
        source,
        Keywords::CANT_BE_BLOCKED
    ));
    let seventh = put(&mut state, PlayerId::P0, "Forest", Zone::Graveyard);
    assert!(engine::has_effective_keyword(
        &state,
        source,
        Keywords::CANT_BE_BLOCKED
    ));
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(seventh, Zone::Exile));
    assert!(!engine::has_effective_keyword(
        &state,
        source,
        Keywords::CANT_BE_BLOCKED
    ));
}

#[test]
fn witness_protection_removes_threshold_unblockable_and_prowess_is_bound_to_duo() {
    let mut state = ready(5);
    let ink = put(
        &mut state,
        PlayerId::P0,
        "Cephalid Inkmage",
        Zone::Battlefield,
    );
    let duo = put(
        &mut state,
        PlayerId::P0,
        "Lightshell Duo",
        Zone::Battlefield,
    );
    for _ in 0..7 {
        put(&mut state, PlayerId::P0, "Forest", Zone::Graveyard);
    }
    assert!(engine::has_effective_keyword(
        &state,
        ink,
        Keywords::CANT_BE_BLOCKED
    ));
    let aura = put(&mut state, PlayerId::P0, "Witness Protection", Zone::Hand);
    state.players[0].mana_pool[ManaColor::U.pool_index()] = 1;
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    engine::step(&mut state, Action::CastSpell(aura)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::ChooseTargets { .. }
    ));
    engine::step(&mut state, Action::ChooseTarget(Target::Object(ink))).unwrap();
    assert!(settle(&mut state).is_none());
    assert!(!engine::has_effective_keyword(
        &state,
        ink,
        Keywords::CANT_BE_BLOCKED
    ));
    assert_eq!(
        (
            engine::effective_power(&state, duo),
            engine::effective_toughness(&state, duo)
        ),
        (4, 5)
    );
    assert_eq!(
        (
            engine::effective_power(&state, ink),
            engine::effective_toughness(&state, ink)
        ),
        (1, 1)
    );
}

#[test]
fn count_one_keeps_the_existing_two_option_prompt_and_one_card_move() {
    let mut state = ready(5);
    let source = put(&mut state, PlayerId::P0, "Wary Thespian", Zone::Hand);
    let top = state.players[0].library[0];
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(source, Zone::Battlefield),
    );
    let pending = trigger::collect_and_process(&mut state);
    state.engine.pending_triggers.extend(pending);
    assert!(matches!(
        settle(&mut state),
        Some(Decision::ChooseEffectOption {
            option_count: 2,
            ..
        })
    ));
    engine::step(&mut state, Action::ChooseEffectOption(1)).unwrap();
    assert!(settle(&mut state).is_none());
    assert_eq!(state.players[0].graveyard, [top]);
    assert_eq!(state.players[0].library.len(), 4);
}
