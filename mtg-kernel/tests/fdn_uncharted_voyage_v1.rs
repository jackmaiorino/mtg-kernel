//! Owner library placement followed by a private, resumable surveil choice.
#![cfg(feature = "limited-fdn-fixtures")]

use mtg_kernel::card_def::{
    card_id_by_name, preflight_fully_supported_deck, CardType, TargetSpec, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::rl::{
    legal_action_candidates_v1, observe_v2, PendingEffectChoiceSemanticV4, TargetSelectionPurposeV4,
};
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};
use mtg_kernel::surface_v2::{HarnessSurfaceV2, SurfaceDecision};

fn ready_with_libraries(p0: &[&str], p1: &[&str]) -> GameState {
    let ids = |names: &[&str]| {
        names
            .iter()
            .map(|name| card_id_by_name(name).unwrap())
            .collect::<Vec<_>>()
    };
    let mut state = GameState::new_from_libraries(
        &ids(p0),
        &ids(p1),
        |id| CARD_DEFS[id as usize].name.into(),
        123,
    );
    state.step = Step::Main1;
    state.players[0].mana_pool[ManaColor::U.pool_index()] = 4;
    state
}

fn ready() -> GameState {
    ready_with_libraries(
        &["Forest", "Plains", "Island"],
        &["Mountain", "Swamp", "Island"],
    )
}

fn put(state: &mut GameState, owner: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id_by_name(name).unwrap();
    let object = state.objects.push(GameObject {
        card_def,
        name: CARD_DEFS[card_def as usize].object_name.into(),
        owner,
        controller: owner,
        zone,
        tapped: false,
        summoning_sick: false,
        damage: 0,
        counters: Default::default(),
        attachments: vec![],
        v4: ObjectStateV4::from_card_def(card_def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    match zone {
        Zone::Battlefield => state.players[owner.index()].battlefield.push(object),
        Zone::Hand => state.players[owner.index()].hand.push(object),
        _ => panic!("helper zone"),
    }
    object
}

fn next(state: &mut GameState) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
    decision
}

fn pass(state: &mut GameState, decision: Decision) {
    let action = match decision {
        Decision::CastSpellOrPass { .. } => Action::Pass,
        Decision::OrderTriggers { pending, .. } => {
            Action::OrderTriggers((0..pending.len()).collect())
        }
        other => panic!("unexpected priority decision: {other:?}"),
    };
    engine::step(state, action).unwrap();
}

fn cast(state: &mut GameState, target: ObjectId) -> ObjectId {
    let spell = put(state, PlayerId::P0, "Uncharted Voyage", Zone::Hand);
    assert!(
        matches!(next(state), Decision::CastSpellOrPass {castable_spells, ..} if castable_spells.contains(&spell))
    );
    engine::step(state, Action::CastSpell(spell)).unwrap();
    assert!(
        matches!(next(state), Decision::ChooseTargets {legal_targets, ..} if legal_targets.contains(&Target::Object(target)))
    );
    engine::step(state, Action::ChooseTarget(Target::Object(target))).unwrap();
    assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
    assert!(state.engine.pending_cast.is_none());
    spell
}

fn owner_choice(state: &mut GameState, owner: PlayerId) -> Decision {
    for _ in 0..30 {
        let decision = next(state);
        if let Decision::ChooseEffectOption {
            player,
            option_count,
            ..
        } = &decision
        {
            assert_eq!(*player, owner);
            assert_eq!(*option_count, 2);
            return decision;
        }
        pass(state, decision);
    }
    panic!("no owner choice");
}

fn surveil_choice(state: &mut GameState) -> (Decision, ObjectId) {
    let decision = next(state);
    let Decision::ChooseEffectTargets {
        player,
        min_targets,
        max_targets,
        legal_targets,
        can_finish,
        ..
    } = &decision
    else {
        panic!("expected immediate private surveil, got {decision:?}");
    };
    assert_eq!(*player, PlayerId::P0);
    assert_eq!((*min_targets, *max_targets, *can_finish), (0, 1, true));
    let [Target::Object(top)] = legal_targets.as_slice() else {
        panic!("surveil top card");
    };
    (decision.clone(), *top)
}

fn finish(state: &mut GameState) {
    for _ in 0..30 {
        let decision = next(state);
        if matches!(decision, Decision::CastSpellOrPass { .. }) && state.stack.is_empty() {
            assert!(state.engine.pending_effect.is_none());
            return;
        }
        pass(state, decision);
    }
    panic!("resolution did not finish");
}

fn restored(state: &GameState) -> GameState {
    serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
}

#[test]
fn exact_printed_definition_and_full_admission() {
    assert_eq!(card_id_by_name("Uncharted Voyage"), Some(201));
    let card = &CARD_DEFS[201];
    assert_eq!(card.mana_value, 4);
    assert_eq!(card.cost.generic, 3);
    assert_eq!(card.cost.pips, &[Pip::Colored(ManaColor::U)]);
    assert_eq!(card.colors, &[ManaColor::U]);
    assert_eq!(card.types, &[CardType::Instant]);
    assert_eq!(card.target_spec, TargetSpec::Creature);
    preflight_fully_supported_deck(&[201]).unwrap();
}

#[test]
fn four_land_payment_consumes_exactly_four_blue_sources() {
    let mut state = ready();
    state.players[0].mana_pool = [0; 6];
    let islands: Vec<_> = (0..4)
        .map(|_| put(&mut state, PlayerId::P0, "Island", Zone::Battlefield))
        .collect();
    let target = put(&mut state, PlayerId::P1, "Elvish Mystic", Zone::Battlefield);
    cast(&mut state, target);
    assert!(islands.iter().all(|&id| state.objects.get(id).tapped));
    assert_eq!(state.players[0].mana_pool, [0; 6]);
}

#[test]
fn offer_requires_blue_and_four_total_mana() {
    for (blue, white, offered) in [(0, 4, false), (1, 2, false), (1, 3, true)] {
        let mut state = ready();
        state.players[0].mana_pool = [0; 6];
        state.players[0].mana_pool[ManaColor::U.pool_index()] = blue;
        state.players[0].mana_pool[ManaColor::W.pool_index()] = white;
        put(&mut state, PlayerId::P1, "Elvish Mystic", Zone::Battlefield);
        let spell = put(&mut state, PlayerId::P0, "Uncharted Voyage", Zone::Hand);
        assert!(
            matches!(next(&mut state), Decision::CastSpellOrPass {castable_spells, ..} if castable_spells.contains(&spell) == offered)
        );
    }
}

#[test]
fn opponent_owner_chooses_exact_top_or_bottom_before_caster_surveils() {
    for option in [0, 1] {
        let mut state = ready();
        let own_before = state.players[0].library.clone();
        let mut expected = state.players[1].library.clone();
        let target = put(&mut state, PlayerId::P1, "Elvish Mystic", Zone::Battlefield);
        let spell = cast(&mut state, target);
        owner_choice(&mut state, PlayerId::P1);
        engine::step(&mut state, Action::ChooseEffectOption(option)).unwrap();
        let (_, top) = surveil_choice(&mut state);
        if option == 0 {
            expected.insert(0, target);
        } else {
            expected.push(target);
        }
        assert_eq!(state.players[1].library, expected);
        assert_eq!(state.players[0].library, own_before);
        assert_eq!(top, own_before[0]);
        assert_eq!(state.step, Step::Main1);
        engine::step(&mut state, Action::FinishEffectTargets).unwrap();
        finish(&mut state);
        assert_eq!(state.objects.get(spell).zone, Zone::Graveyard);
        assert_eq!(state.objects.get(target).zone, Zone::Library);
        assert_eq!(state.players[0].library, own_before);
    }
}

#[test]
fn owner_differs_from_controller_and_receives_the_card_in_their_library() {
    let mut state = ready();
    let target = put(&mut state, PlayerId::P1, "Elvish Mystic", Zone::Battlefield);
    state.players[1].battlefield.retain(|&id| id != target);
    state.players[0].battlefield.push(target);
    state.objects.get_mut(target).controller = PlayerId::P0;
    cast(&mut state, target);
    owner_choice(&mut state, PlayerId::P1);
    engine::step(&mut state, Action::ChooseEffectOption(0)).unwrap();
    surveil_choice(&mut state);
    assert_eq!(state.players[1].library[0], target);
    assert!(!state.players[0].library.contains(&target));
    engine::step(&mut state, Action::FinishEffectTargets).unwrap();
    finish(&mut state);
}

#[test]
fn own_top_target_can_be_kept_or_put_in_graveyard_by_surveil() {
    for graveyard in [false, true] {
        let mut state = ready();
        let original = state.players[0].library.clone();
        let target = put(&mut state, PlayerId::P0, "Elvish Mystic", Zone::Battlefield);
        cast(&mut state, target);
        owner_choice(&mut state, PlayerId::P0);
        engine::step(&mut state, Action::ChooseEffectOption(0)).unwrap();
        let (_, top) = surveil_choice(&mut state);
        assert_eq!(top, target);
        let action = if graveyard {
            Action::ChooseEffectTarget(Target::Object(top))
        } else {
            Action::FinishEffectTargets
        };
        engine::step(&mut state, action).unwrap();
        finish(&mut state);
        if graveyard {
            assert_eq!(state.players[0].library, original);
            assert_eq!(state.objects.get(target).zone, Zone::Graveyard);
        } else {
            assert_eq!(state.players[0].library[0], target);
            assert_eq!(&state.players[0].library[1..], &original);
        }
    }
}

#[test]
fn surveil_top_identity_is_visible_only_to_its_chooser() {
    let mut state = ready_with_libraries(&["Brainstorm"], &["Island"]);
    let target = put(&mut state, PlayerId::P1, "Elvish Mystic", Zone::Battlefield);
    cast(&mut state, target);
    owner_choice(&mut state, PlayerId::P1);
    engine::step(&mut state, Action::ChooseEffectOption(1)).unwrap();
    let (decision, _) = surveil_choice(&mut state);
    let chooser = observe_v2(&state, &HarnessSurfaceV2::new(), PlayerId::P0, 0).unwrap();
    let other = observe_v2(&state, &HarnessSurfaceV2::new(), PlayerId::P1, 0).unwrap();
    for (observation, count) in [(&chooser, 1), (&other, 0)] {
        assert!(
            matches!(observation.projection.engine_context.pending_effect.as_ref().unwrap().choice.as_ref().unwrap(),
            PendingEffectChoiceSemanticV4::Targets {legal_targets, selected_targets, purpose: TargetSelectionPurposeV4::CardSelection, ..}
            if legal_targets.len() == count && selected_targets.is_empty())
        );
    }
    assert!(!serde_json::to_string(&other)
        .unwrap()
        .contains("Brainstorm"));
    assert_eq!(
        legal_action_candidates_v1(&SurfaceDecision::Decision(decision), &state)
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn empty_library_skips_surveil_without_drawing_or_losing() {
    let mut state = ready_with_libraries(&[], &["Island"]);
    let target = put(&mut state, PlayerId::P1, "Elvish Mystic", Zone::Battlefield);
    let spell = cast(&mut state, target);
    owner_choice(&mut state, PlayerId::P1);
    engine::step(&mut state, Action::ChooseEffectOption(1)).unwrap();
    finish(&mut state);
    assert!(state.players[0].library.is_empty());
    assert_eq!(state.objects.get(spell).zone, Zone::Graveyard);
}

#[test]
fn an_illegal_only_target_skips_both_placement_and_surveil() {
    for reentry in [false, true] {
        let mut state = ready();
        let original = state.players[0].library.clone();
        let target = put(&mut state, PlayerId::P1, "Elvish Mystic", Zone::Battlefield);
        let spell = cast(&mut state, target);
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(target, Zone::Hand));
        if reentry {
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(target, Zone::Battlefield),
            );
        }
        finish(&mut state);
        assert_eq!(state.players[0].library, original);
        assert_eq!(state.objects.get(spell).zone, Zone::Graveyard);
        assert_eq!(
            state.objects.get(target).zone,
            if reentry {
                Zone::Battlefield
            } else {
                Zone::Hand
            }
        );
    }
}

#[test]
fn a_token_target_departs_and_does_not_remain_in_its_owners_library() {
    for option in [0, 1] {
        let mut state = ready();
        let target = put(&mut state, PlayerId::P1, "Koma's Coil", Zone::Battlefield);
        cast(&mut state, target);
        owner_choice(&mut state, PlayerId::P1);
        engine::step(&mut state, Action::ChooseEffectOption(option)).unwrap();
        surveil_choice(&mut state);
        engine::step(&mut state, Action::FinishEffectTargets).unwrap();
        finish(&mut state);
        assert!(!state.players[1].battlefield.contains(&target));
        assert!(!state.players[1].library.contains(&target));
        assert!(!state.players[1].graveyard.contains(&target));
    }
}

#[test]
fn ward_payment_resolves_before_the_owner_placement_choice() {
    let mut state = ready();
    state.players[0].mana_pool[ManaColor::U.pool_index()] = 6;
    let target = put(
        &mut state,
        PlayerId::P1,
        "Cackling Prowler",
        Zone::Battlefield,
    );
    cast(&mut state, target);
    for _ in 0..30 {
        let decision = next(&mut state);
        if matches!(decision, Decision::ChooseEffectBoolean { .. }) {
            engine::step(&mut state, Action::ChooseEffectBoolean(true)).unwrap();
            break;
        }
        pass(&mut state, decision);
    }
    owner_choice(&mut state, PlayerId::P1);
    assert_eq!(state.players[0].mana_pool, [0; 6]);
    engine::step(&mut state, Action::ChooseEffectOption(1)).unwrap();
    surveil_choice(&mut state);
    engine::step(&mut state, Action::FinishEffectTargets).unwrap();
    finish(&mut state);
}

#[test]
fn pending_owner_choice_restores_with_the_same_selected_placement_and_surveil() {
    for option in [0, 1] {
        let mut state = ready();
        let target = put(&mut state, PlayerId::P1, "Elvish Mystic", Zone::Battlefield);
        cast(&mut state, target);
        owner_choice(&mut state, PlayerId::P1);
        let mut copy = restored(&state);
        for instance in [&mut state, &mut copy] {
            engine::step(instance, Action::ChooseEffectOption(option)).unwrap();
            surveil_choice(instance);
            engine::step(instance, Action::FinishEffectTargets).unwrap();
            finish(instance);
        }
        assert_eq!(state.state_hash(), copy.state_hash());
    }
}

#[test]
fn pending_private_surveil_choice_restores_for_keep_or_graveyard() {
    for graveyard in [false, true] {
        let mut state = ready();
        let target = put(&mut state, PlayerId::P1, "Elvish Mystic", Zone::Battlefield);
        cast(&mut state, target);
        owner_choice(&mut state, PlayerId::P1);
        engine::step(&mut state, Action::ChooseEffectOption(0)).unwrap();
        let (_, top) = surveil_choice(&mut state);
        let mut copy = restored(&state);
        for instance in [&mut state, &mut copy] {
            let action = if graveyard {
                Action::ChooseEffectTarget(Target::Object(top))
            } else {
                Action::FinishEffectTargets
            };
            engine::step(instance, action).unwrap();
            finish(instance);
        }
        assert_eq!(state.state_hash(), copy.state_hash());
    }
}

#[test]
fn answered_owner_and_surveil_frames_restore_before_their_moves() {
    for owner_answer in [true, false] {
        let mut state = ready();
        let target = put(&mut state, PlayerId::P1, "Elvish Mystic", Zone::Battlefield);
        cast(&mut state, target);
        owner_choice(&mut state, PlayerId::P1);
        engine::step(&mut state, Action::ChooseEffectOption(0)).unwrap();
        if !owner_answer {
            let (_, top) = surveil_choice(&mut state);
            engine::step(&mut state, Action::ChooseEffectTarget(Target::Object(top))).unwrap();
        }
        let mut copy = restored(&state);
        for instance in [&mut state, &mut copy] {
            if owner_answer {
                surveil_choice(instance);
                engine::step(instance, Action::FinishEffectTargets).unwrap();
            }
            finish(instance);
        }
        assert_eq!(state.state_hash(), copy.state_hash());
    }
}

#[test]
fn invalid_owner_option_does_not_mutate_pending_choice() {
    let mut state = ready();
    let target = put(&mut state, PlayerId::P1, "Elvish Mystic", Zone::Battlefield);
    cast(&mut state, target);
    owner_choice(&mut state, PlayerId::P1);
    let hash = state.state_hash();
    assert!(engine::step(&mut state, Action::ChooseEffectOption(2)).is_err());
    assert_eq!(state.state_hash(), hash);
}

#[test]
fn surveil_rejects_an_unlooked_card_without_mutating_state() {
    let mut state = ready();
    let target = put(&mut state, PlayerId::P1, "Elvish Mystic", Zone::Battlefield);
    cast(&mut state, target);
    owner_choice(&mut state, PlayerId::P1);
    engine::step(&mut state, Action::ChooseEffectOption(1)).unwrap();
    surveil_choice(&mut state);
    let hash = state.state_hash();
    let unlooked = state.players[0].library[1];
    assert!(engine::step(
        &mut state,
        Action::ChooseEffectTarget(Target::Object(unlooked))
    )
    .is_err());
    assert_eq!(state.state_hash(), hash);
}
