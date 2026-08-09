//! Focused Caw-Gates coverage against the checked-in Mage source at
//! `a5c90fe180021e70e2a644ade00eeab07f857a40`.

use mtg_kernel::card_def::{
    card_id_by_name, mana_colors_mask, CardCapability, CardType, CostComponent, Keywords,
    ManaAbilityCostDef, Subtype, TargetSpec, CARD_DEFS, KERNEL_CARDDB_HASH,
};
use mtg_kernel::effect::{EffectOp, LibraryCardFilter, PlayerRef};
use mtg_kernel::engine::{self, Action, CostKind, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{Cost, ManaColor, Pip};
use mtg_kernel::rl::{
    legal_action_candidates_v1, observe_v2, ActionSemanticV1, PendingEffectChoiceSemanticV4,
    TargetSelectionPurposeV4,
};
use mtg_kernel::state::{
    Counters, GameObject, GameState, ObjectStateV4, StackItem, StackItemKind, StackStateV4, Step,
    Target, Zone,
};
use mtg_kernel::surface_v2::{HarnessSurfaceV2, SurfaceAction, SurfaceDecision};
use mtg_kernel::trigger::{self, TriggerCondition};

fn card_id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name} in CARD_DEFS"))
}

fn card_name(card_def: u16) -> String {
    CARD_DEFS[card_def as usize].name.to_string()
}

fn ready_main(seed: u64) -> GameState {
    let mut state = GameState::new_from_libraries(&[], &[], card_name, seed);
    state.step = Step::Main1;
    state.active_player = PlayerId::P0;
    state.priority_player = PlayerId::P0;
    state
}

fn put_object(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id(name);
    let id = state.objects.push(GameObject {
        card_def,
        name: CARD_DEFS[card_def as usize].object_name.to_string(),
        owner: player,
        controller: player,
        zone,
        tapped: false,
        summoning_sick: false,
        damage: 0,
        counters: Counters::default(),
        attachments: Vec::new(),
        v4: ObjectStateV4::from_card_def(card_def),
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
        Zone::Stack => panic!("test helper does not fabricate stack-zone objects"),
    }
    id
}

fn action_candidates(
    state: &GameState,
    decision: &Decision,
) -> Vec<mtg_kernel::rl::LegalActionCandidateV1> {
    legal_action_candidates_v1(&SurfaceDecision::Decision(decision.clone()), state)
        .expect("legal action projection")
}

fn mana_colors_for(state: &GameState, decision: &Decision, source: ObjectId) -> Vec<ManaColor> {
    action_candidates(state, decision)
        .into_iter()
        .filter_map(|candidate| match candidate.record.semantic {
            ActionSemanticV1::ActivateManaAbility {
                source: card,
                mana_choice: Some(color),
                ..
            } if card.arena_id == source.0 => Some(color),
            _ => None,
        })
        .collect()
}

fn activatable_contains(state: &mut GameState, source: ObjectId) -> bool {
    match engine::advance_until_decision(state) {
        Decision::CastSpellOrPass {
            activatable_abilities,
            ..
        } => activatable_abilities.contains(&(source, 0)),
        other => panic!("expected priority decision, got {other:?}"),
    }
}

fn resolve_top_without_choice(state: &mut GameState) {
    for _ in 0..8 {
        let decision = engine::advance_until_decision(state);
        if state.stack.is_empty() && state.engine.pending_effect.is_none() {
            return;
        }
        match decision {
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => panic!("unexpected decision while resolving stack item: {other:?}"),
        }
    }
    panic!("stack item did not resolve within the bounded priority walk");
}

fn reference_shuffle(
    mut objects: Vec<ObjectId>,
    mut rng: mtg_kernel::state::SplitMix64,
) -> Vec<ObjectId> {
    for i in (1..objects.len()).rev() {
        let j = (rng.next_u64() % (i as u64 + 1)) as usize;
        objects.swap(i, j);
    }
    objects
}

#[test]
fn definitions_ids_and_generic_programs_are_exact() {
    assert_eq!(KERNEL_CARDDB_HASH, 0xbfe2_c254_4934_26f1);
    let expected_ids = [
        ("Basilisk Gate", 3),
        ("Citadel Gate", 14),
        ("Heap Gate", 53),
        ("Sacred Cat", 98),
        ("Sea Gate", 100),
        ("Squadron Hawk", 112),
        ("Sacred Cat Embalmed Token", 148),
        ("Treasure Token", 149),
    ];
    assert_eq!(CARD_DEFS.len(), 150);
    for (name, expected_id) in expected_ids {
        let id = card_id(name);
        assert_eq!(id, expected_id, "append-only id for {name}");
        assert_eq!(CARD_DEFS[id as usize].capability, CardCapability::Full);
    }
    assert_eq!(Subtype::Treasure.stable_id(), 62);

    let basilisk = &CARD_DEFS[card_id("Basilisk Gate") as usize];
    assert_eq!(basilisk.types, &[CardType::Land]);
    assert_eq!(basilisk.subtypes, &[Subtype::Gate]);
    assert_eq!(basilisk.mana_ability_choices, &[ManaColor::C]);
    assert_eq!(basilisk.activated_abilities.len(), 1);
    let pump = &basilisk.activated_abilities[0];
    assert_eq!(
        pump.cost,
        &[
            CostComponent::Mana(Cost {
                pips: &[],
                generic: 2,
                x_count: 0,
            }),
            CostComponent::Tap,
        ]
    );
    assert!(pump.sorcery_speed_only);
    assert_eq!(pump.target_spec, TargetSpec::Creature);
    assert_eq!(
        (pump.effect)(),
        EffectOp::PumpTargetByControlledSubtypeCount {
            target: mtg_kernel::effect::ObjectRef::Target(0),
            subtype: Subtype::Gate,
        }
    );

    for (name, fixed, excluded) in [
        ("Citadel Gate", ManaColor::W, ManaColor::W),
        ("Sea Gate", ManaColor::U, ManaColor::U),
    ] {
        let gate = &CARD_DEFS[card_id(name) as usize];
        assert_eq!(gate.mana_ability_choices, &[fixed]);
        assert!(gate.mana_ability_includes_chosen_color);
        assert_eq!(gate.as_enters_choose_color_other_than, Some(excluded));
        assert!(gate.enters_battlefield_tapped);
    }

    let heap = &CARD_DEFS[card_id("Heap Gate") as usize];
    assert_eq!(heap.mana_ability_choices, &[ManaColor::C]);
    assert_eq!(heap.additional_mana_abilities.len(), 1);
    let paid_mana = heap.additional_mana_abilities[0];
    assert_eq!(
        paid_mana.colors,
        &[
            ManaColor::W,
            ManaColor::U,
            ManaColor::B,
            ManaColor::R,
            ManaColor::G,
        ]
    );
    assert_eq!(paid_mana.mana_cost.generic, 1);
    assert_eq!(paid_mana.ability.cost, ManaAbilityCostDef::TapSelf);
    assert!(matches!(
        heap.activated_abilities[0].cost,
        [
            CostComponent::Mana(Cost { generic: 1, .. }),
            CostComponent::Tap,
            CostComponent::TapOtherUntappedControlledPermanentWithSubtype(Subtype::Gate),
        ]
    ));

    let hawk = &CARD_DEFS[card_id("Squadron Hawk") as usize];
    assert!(hawk.keywords.has(Keywords::FLYING));
    assert_eq!((hawk.power, hawk.toughness), (Some(1), Some(1)));
    let triggers = trigger::triggers_for(card_id("Squadron Hawk"));
    assert_eq!(triggers.len(), 1);
    assert_eq!(triggers[0].condition, TriggerCondition::Etb);
    assert_eq!(
        (triggers[0].effect)(),
        EffectOp::SearchLibraryToHandUpTo {
            player: PlayerRef::Controller,
            filter: LibraryCardFilter::CardDefinition(card_id("Squadron Hawk")),
            max_targets: 3,
        }
    );

    let cat = &CARD_DEFS[card_id("Sacred Cat") as usize];
    assert!(cat.keywords.has(Keywords::LIFELINK));
    assert_eq!(cat.activated_abilities[0].activation_zone, Zone::Graveyard);
    assert!(cat.activated_abilities[0].sorcery_speed_only);
    assert_eq!(
        cat.activated_abilities[0].cost,
        &[
            CostComponent::Mana(Cost {
                pips: &[Pip::Colored(ManaColor::W)],
                generic: 0,
                x_count: 0,
            }),
            CostComponent::ExileSelf,
        ]
    );
    let embalmed = &CARD_DEFS[card_id("Sacred Cat Embalmed Token") as usize];
    assert!(embalmed.is_token);
    assert_eq!(embalmed.object_name, "Sacred Cat");
    assert_eq!(embalmed.colors, &[ManaColor::W]);
    assert_eq!(embalmed.subtypes, &[Subtype::Cat, Subtype::Zombie]);
    assert_eq!(embalmed.cost, Cost::zero());
    assert!(embalmed.keywords.has(Keywords::LIFELINK));

    let treasure = &CARD_DEFS[card_id("Treasure Token") as usize];
    assert!(treasure.is_token);
    assert_eq!(treasure.subtypes, &[Subtype::Treasure]);
    assert_eq!(
        treasure.mana_ability_def.unwrap().cost,
        ManaAbilityCostDef::TapAndSacrificeSelf
    );
}

#[test]
fn chosen_color_gates_stage_before_entry_and_expose_only_fixed_plus_chosen_mana() {
    let mut state = ready_main(0x4341_5747_4154_4501);
    let citadel = put_object(&mut state, PlayerId::P0, "Citadel Gate", Zone::Hand);
    engine::step(&mut state, Action::PlayLand(citadel)).unwrap();
    assert_eq!(state.objects.get(citadel).zone, Zone::Hand);
    let pending = state.engine.pending_land_play.as_ref().unwrap();
    assert_eq!(pending.source, citadel);
    assert_eq!(pending.excluded_color, ManaColor::W);
    let pending_json = serde_json::to_value(pending).unwrap();
    assert_eq!(
        pending_json
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>(),
        vec![
            "controller",
            "excluded_color",
            "origin_zone",
            "source",
            "source_zone_change_count",
        ]
    );
    let restored: GameState =
        serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
    assert_eq!(restored, state);

    let decision = engine::advance_until_decision(&mut state);
    assert!(matches!(
        decision,
        Decision::ChooseEffectOption {
            player: PlayerId::P0,
            source,
            option_count: 4,
        } if source == citadel
    ));
    let candidates = action_candidates(&state, &decision);
    let colors = candidates
        .iter()
        .map(|candidate| match candidate.record.semantic {
            ActionSemanticV1::ChooseEffectColor { color, .. } => color,
            ref other => panic!("unexpected land-choice semantic: {other:?}"),
        })
        .collect::<Vec<_>>();
    assert_eq!(
        colors,
        vec![ManaColor::U, ManaColor::B, ManaColor::R, ManaColor::G]
    );
    let mut stable_ids = candidates
        .iter()
        .map(|candidate| candidate.record.stable_id.as_str())
        .collect::<Vec<_>>();
    stable_ids.sort_unstable();
    stable_ids.dedup();
    assert_eq!(stable_ids.len(), 4);
    assert!(candidates.iter().all(|candidate| matches!(
        candidate.surface_action,
        SurfaceAction::Action(Action::ChooseEffectOption(_))
    )));

    let before_bad = state.clone();
    assert!(engine::step(&mut state, Action::ChooseEffectOption(4)).is_err());
    assert_eq!(state, before_bad);
    engine::step(&mut state, Action::ChooseEffectOption(3)).unwrap();
    assert_eq!(state.objects.get(citadel).zone, Zone::Battlefield);
    assert!(state.objects.get(citadel).tapped);
    assert_eq!(
        state.objects.get(citadel).v4.chosen_color,
        Some(ManaColor::G)
    );
    assert!(state.engine.pending_land_play.is_none());

    state.objects.get_mut(citadel).tapped = false;
    let main = engine::advance_until_decision(&mut state);
    assert_eq!(
        mana_colors_for(&state, &main, citadel),
        vec![ManaColor::W, ManaColor::G]
    );
    engine::step(
        &mut state,
        Action::ActivateManaAbilityChoice(citadel, ManaColor::G),
    )
    .unwrap();
    assert_eq!(state.players[0].mana_pool[ManaColor::G.pool_index()], 1);

    event::propose_and_commit(&mut state, ProposedEvent::zone_change(citadel, Zone::Hand));
    assert_eq!(state.objects.get(citadel).v4.chosen_color, None);

    let mut sea_state = ready_main(0x4341_5747_4154_4502);
    let sea = put_object(&mut sea_state, PlayerId::P0, "Sea Gate", Zone::Hand);
    engine::step(&mut sea_state, Action::PlayLand(sea)).unwrap();
    let sea_choice = engine::advance_until_decision(&mut sea_state);
    let sea_colors = action_candidates(&sea_state, &sea_choice)
        .into_iter()
        .map(|candidate| match candidate.record.semantic {
            ActionSemanticV1::ChooseEffectColor { color, .. } => color,
            other => panic!("unexpected Sea Gate choice: {other:?}"),
        })
        .collect::<Vec<_>>();
    assert_eq!(
        sea_colors,
        vec![ManaColor::W, ManaColor::B, ManaColor::R, ManaColor::G]
    );
    engine::step(&mut sea_state, Action::ChooseEffectOption(0)).unwrap();
    assert!(sea_state.objects.get(sea).tapped);
    assert_eq!(
        sea_state.objects.get(sea).v4.chosen_color,
        Some(ManaColor::W)
    );
}

#[test]
fn basilisk_gate_counts_live_controlled_gates_at_resolution_and_ends_at_cleanup() {
    let mut state = ready_main(0x4341_5747_4154_4503);
    let basilisk = put_object(&mut state, PlayerId::P0, "Basilisk Gate", Zone::Battlefield);
    put_object(&mut state, PlayerId::P0, "Citadel Gate", Zone::Battlefield);
    put_object(&mut state, PlayerId::P0, "Mountain", Zone::Battlefield);
    let target = put_object(&mut state, PlayerId::P1, "Faerie Seer", Zone::Battlefield);
    state.players[0].mana_pool[ManaColor::C.pool_index()] = 2;

    let mut wrong_time = state.clone();
    wrong_time.step = Step::Upkeep;
    assert!(!activatable_contains(&mut wrong_time, basilisk));
    assert!(activatable_contains(&mut state, basilisk));
    engine::step(&mut state, Action::ActivateAbility(basilisk, 0)).unwrap();
    let target_decision = engine::advance_until_decision(&mut state);
    assert!(matches!(
        target_decision,
        Decision::ChooseTargets {
            player: PlayerId::P0,
            spell,
            legal_targets,
            ..
        } if spell == basilisk
            && legal_targets.contains(&Target::Object(target))
            && !legal_targets.contains(&Target::Object(basilisk))
    ));
    engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    assert!(state.objects.get(basilisk).tapped);
    assert_eq!(state.players[0].mana_pool, [0; 6]);

    put_object(&mut state, PlayerId::P0, "Heap Gate", Zone::Battlefield);
    assert_eq!(engine::effective_power(&state, target), 1);
    resolve_top_without_choice(&mut state);
    assert_eq!(engine::effective_power(&state, target), 4);
    assert_eq!(engine::effective_toughness(&state, target), 4);

    state.step = Step::End;
    state.engine.priority_passes = [false, false];
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    engine::step(&mut state, Action::Pass).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    engine::step(&mut state, Action::Pass).unwrap();
    let _ = engine::advance_until_decision(&mut state);
    assert_eq!(engine::effective_power(&state, target), 1);
    assert_eq!(engine::effective_toughness(&state, target), 1);
}

#[test]
fn heap_gate_reserves_both_tap_costs_and_treasure_is_exact_one_shot_mana() {
    let mut no_third_resource = ready_main(0x4341_5747_4154_4504);
    let heap = put_object(
        &mut no_third_resource,
        PlayerId::P0,
        "Heap Gate",
        Zone::Battlefield,
    );
    put_object(
        &mut no_third_resource,
        PlayerId::P0,
        "Citadel Gate",
        Zone::Battlefield,
    );
    assert!(
        !activatable_contains(&mut no_third_resource, heap),
        "neither Gate may also pay the generic mana"
    );

    let mut paid_mana_state = ready_main(0x4341_5747_4154_4505);
    let paid_heap = put_object(
        &mut paid_mana_state,
        PlayerId::P0,
        "Heap Gate",
        Zone::Battlefield,
    );
    let mountain = put_object(
        &mut paid_mana_state,
        PlayerId::P0,
        "Mountain",
        Zone::Battlefield,
    );
    let decision = engine::advance_until_decision(&mut paid_mana_state);
    assert_eq!(
        mana_colors_for(&paid_mana_state, &decision, paid_heap),
        vec![
            ManaColor::C,
            ManaColor::W,
            ManaColor::U,
            ManaColor::B,
            ManaColor::R,
            ManaColor::G,
        ]
    );
    engine::step(
        &mut paid_mana_state,
        Action::ActivateManaAbilityChoice(paid_heap, ManaColor::G),
    )
    .unwrap();
    assert!(paid_mana_state.objects.get(paid_heap).tapped);
    assert!(paid_mana_state.objects.get(mountain).tapped);
    assert_eq!(paid_mana_state.players[0].mana_pool, [0, 0, 0, 0, 1, 0]);
    assert_eq!(
        paid_mana_state
            .objects
            .get(paid_heap)
            .v4
            .ability_uses_this_turn[0]
            .ability_index,
        1
    );

    let mut state = ready_main(0x4341_5747_4154_4506);
    let heap = put_object(&mut state, PlayerId::P0, "Heap Gate", Zone::Battlefield);
    let citadel = put_object(&mut state, PlayerId::P0, "Citadel Gate", Zone::Battlefield);
    let payment_land = put_object(&mut state, PlayerId::P0, "Mountain", Zone::Battlefield);
    let sea = put_object(&mut state, PlayerId::P0, "Sea Gate", Zone::Battlefield);
    assert!(activatable_contains(&mut state, heap));
    engine::step(&mut state, Action::ActivateAbility(heap, 0)).unwrap();
    let cost_decision = engine::advance_until_decision(&mut state);
    assert!(matches!(
        cost_decision,
        Decision::ChooseCostTargets {
            player: PlayerId::P0,
            source,
            cost_kind: CostKind::TapPermanents,
            remaining: 1,
            ref candidates,
        } if source == heap && candidates == &vec![citadel, sea]
    ));
    let serialized: GameState =
        serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
    assert_eq!(serialized, state);
    let before_bad = state.clone();
    assert!(engine::step(&mut state, Action::ChooseCostTarget(payment_land)).is_err());
    assert_eq!(state, before_bad);

    engine::step(&mut state, Action::ChooseCostTarget(citadel)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    assert!(state.objects.get(heap).tapped);
    assert!(state.objects.get(citadel).tapped);
    assert!(state.objects.get(payment_land).tapped);
    assert!(!state.objects.get(sea).tapped);
    assert_eq!(state.stack.last().unwrap().v4.paid_cost_refs.len(), 1);
    assert_eq!(
        state.stack.last().unwrap().v4.paid_cost_refs[0].object,
        citadel
    );

    resolve_top_without_choice(&mut state);
    let treasure_def = card_id("Treasure Token");
    let treasure = *state.players[0]
        .battlefield
        .iter()
        .find(|&&object| state.objects.get(object).card_def == treasure_def)
        .expect("Heap Gate created one Treasure");
    let treasure_object = state.objects.get(treasure);
    assert!(treasure_object.v4.is_token);
    assert_eq!(treasure_object.name, "Treasure Token");
    assert!(treasure_object
        .v4
        .effective_subtype_ids
        .contains(&Subtype::Treasure.stable_id()));
    assert!(!mtg_kernel::mana::gather_sources(PlayerId::P0, &state)
        .iter()
        .any(|source| source.id == treasure));

    engine::step(
        &mut state,
        Action::ActivateManaAbilityChoice(treasure, ManaColor::B),
    )
    .unwrap();
    assert_eq!(state.objects.get(treasure).zone, Zone::Graveyard);
    assert_eq!(state.players[0].mana_pool[ManaColor::B.pool_index()], 1);
    let _ = engine::advance_until_decision(&mut state);
    assert!(!state.players[0].graveyard.contains(&treasure));
}

#[test]
fn squadron_hawk_searches_zero_through_three_exact_physical_hawks_reveals_and_shuffles() {
    let mut state = ready_main(0x4341_5747_4154_4507);
    let hawks = (0..4)
        .map(|_| put_object(&mut state, PlayerId::P0, "Squadron Hawk", Zone::Library))
        .collect::<Vec<_>>();
    let mountain = put_object(&mut state, PlayerId::P0, "Mountain", Zone::Library);
    let source = put_object(&mut state, PlayerId::P0, "Squadron Hawk", Zone::Battlefield);
    let trigger_effect = (trigger::triggers_for(card_id("Squadron Hawk"))[0].effect)();
    state.stack.push(StackItem {
        kind: StackItemKind::TriggeredAbility,
        source,
        controller: PlayerId::P0,
        targets: Vec::new(),
        is_copy: false,
        inline_effect: Some(trigger_effect),
        discarded: Vec::new(),
        is_flashback: false,
        mode_chosen: 0,
        madness_offer: false,
        kicked: false,
        v4: StackStateV4::default(),
    });
    state.engine.priority_passes = [true, true];

    let search = engine::advance_until_decision(&mut state);
    assert!(matches!(
        search,
        Decision::ChooseEffectTargets {
            player: PlayerId::P0,
            source: decision_source,
            ref legal_targets,
            min_targets: 0,
            max_targets: 3,
            can_finish: true,
            selected_count: 0,
        } if decision_source == source
            && legal_targets == &hawks.iter().copied().map(Target::Object).collect::<Vec<_>>()
            && !legal_targets.contains(&Target::Object(mountain))
    ));
    let actions = action_candidates(&state, &search);
    assert_eq!(actions.len(), 5);
    assert!(matches!(
        actions.last().unwrap().record.semantic,
        ActionSemanticV1::FinishEffectSelection { .. }
    ));

    let opponent = observe_v2(&state, &HarnessSurfaceV2::new(), PlayerId::P1, 0).unwrap();
    let choice = opponent
        .projection
        .engine_context
        .pending_effect
        .as_ref()
        .and_then(|pending| pending.choice.as_ref())
        .unwrap();
    assert!(matches!(
        choice,
        PendingEffectChoiceSemanticV4::Targets {
            selected_targets,
            legal_targets,
            min_targets: 0,
            max_targets: 0,
            can_finish: true,
            purpose: TargetSelectionPurposeV4::SearchResult,
            ..
        } if selected_targets.is_empty() && legal_targets.is_empty()
    ));
    let restored: GameState =
        serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
    assert_eq!(restored, state);

    let mut zero = state.clone();
    let expected_zero = reference_shuffle(
        zero.players[0].library.clone(),
        *zero.legacy_rng().expect("legacy rng"),
    );
    engine::step(&mut zero, Action::FinishEffectSelection).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut zero),
        Decision::CastSpellOrPass { .. }
    ));
    assert!(zero.players[0].hand.is_empty());
    assert_eq!(zero.players[0].library, expected_zero);

    for &hawk in &hawks[..3] {
        engine::step(&mut state, Action::ChooseEffectTarget(Target::Object(hawk))).unwrap();
    }
    assert!(state
        .engine
        .pending_effect
        .as_ref()
        .is_some_and(|pending| pending.choice.is_none()));
    let mut expected_remaining = vec![hawks[3], mountain];
    expected_remaining =
        reference_shuffle(expected_remaining, *state.legacy_rng().expect("legacy rng"));
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    assert_eq!(&state.players[0].hand, &hawks[..3]);
    assert_eq!(state.players[0].library, expected_remaining);
    for &hawk in &hawks[..3] {
        assert!(state
            .known_hand_cards(PlayerId::P1, PlayerId::P0)
            .iter()
            .any(|entry| entry.object == hawk));
    }
    assert!(state
        .known_library_cards(PlayerId::P0, PlayerId::P0)
        .is_empty());
    assert!(state
        .known_library_cards(PlayerId::P1, PlayerId::P0)
        .is_empty());
}

#[test]
fn sacred_cat_lifelink_and_embalm_pay_exile_then_make_the_exact_copy_token() {
    let mut state = ready_main(0x4341_5747_4154_4508);
    let cat = put_object(&mut state, PlayerId::P0, "Sacred Cat", Zone::Graveyard);
    state.players[0].mana_pool[ManaColor::W.pool_index()] = 1;

    let mut wrong_time = state.clone();
    wrong_time.step = Step::Upkeep;
    assert!(!activatable_contains(&mut wrong_time, cat));
    assert!(activatable_contains(&mut state, cat));
    engine::step(&mut state, Action::ActivateAbility(cat, 0)).unwrap();
    assert_eq!(state.objects.get(cat).zone, Zone::Graveyard);
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    assert_eq!(state.objects.get(cat).zone, Zone::Exile);
    assert!(state.exile.contains(&cat));
    assert_eq!(state.players[0].mana_pool, [0; 6]);
    let item = state.stack.last().expect("Embalm uses the stack");
    assert_eq!(item.kind, StackItemKind::ActivatedAbility);
    assert_eq!(item.v4.paid_cost_refs.len(), 1);
    assert_eq!(item.v4.paid_cost_refs[0].object, cat);
    assert_eq!(item.v4.paid_cost_refs[0].zone, Zone::Exile);

    resolve_top_without_choice(&mut state);
    let token_def = card_id("Sacred Cat Embalmed Token");
    let token = *state.players[0]
        .battlefield
        .iter()
        .find(|&&object| state.objects.get(object).card_def == token_def)
        .expect("Embalm created its token");
    let object = state.objects.get(token);
    assert_eq!(object.name, "Sacred Cat");
    assert!(object.v4.is_token);
    assert_eq!(
        object.v4.effective_color_mask,
        mana_colors_mask(&[ManaColor::W])
    );
    assert_eq!(
        object.v4.effective_subtype_ids,
        vec![Subtype::Cat.stable_id(), Subtype::Zombie.stable_id()]
    );
    assert_eq!(
        (
            engine::effective_power(&state, token),
            engine::effective_toughness(&state, token)
        ),
        (1, 1)
    );

    let life_before = state.players[0].life;
    event::propose_and_commit(
        &mut state,
        ProposedEvent::damage(token, Target::Player(PlayerId::P1), 1),
    );
    assert_eq!(state.players[0].life, life_before + 1);
    assert_eq!(state.objects.get(cat).zone, Zone::Exile);
}
