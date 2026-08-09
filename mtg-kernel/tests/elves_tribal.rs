use mtg_kernel::card_def::{
    card_id_by_name, preflight_fully_supported_deck, CardCapability, CostComponent,
    DynamicValueDef, ManaAbilityAmountDef, ManaAbilityCostDef, PermanentFilterDef, Subtype,
    CARD_DEFS,
};
use mtg_kernel::effect::{
    EffectOp, EffectTargetSelectionPurpose, LibraryPartitionSelectionStage, ObjectRef, PlayerRef,
    TargetRef,
};
use mtg_kernel::engine::{self, Action, CostKind, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::rl::{
    legal_action_candidates_v1, observe_v2, ActionSemanticV1, PendingEffectChoiceSemanticV4,
    TargetRefV1, TargetSelectionPurposeV4,
};
use mtg_kernel::state::{Counters, GameObject, GameState, ObjectStateV4, Step, Target, Zone};
use mtg_kernel::surface_v2::{HarnessSurfaceV2, SurfaceDecision};

fn card_id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name} in CARD_DEFS"))
}

fn card_name(card_def: u16) -> String {
    CARD_DEFS[card_def as usize].name.to_string()
}

fn ready_main1() -> GameState {
    let mut state = GameState::new_from_libraries(&[], &[], card_name, 0x454c_5645_535f_5631);
    state.step = Step::Main1;
    state.active_player = PlayerId::P0;
    state.priority_player = PlayerId::P0;
    state
}

fn put_object_controlled_by(
    state: &mut GameState,
    owner: PlayerId,
    controller: PlayerId,
    name: &str,
    zone: Zone,
    tapped: bool,
    summoning_sick: bool,
) -> ObjectId {
    let card_def = card_id(name);
    let id = state.objects.push(GameObject {
        card_def,
        name: name.to_string(),
        owner,
        controller,
        zone,
        tapped,
        summoning_sick,
        damage: 0,
        counters: Counters::default(),
        attachments: Vec::new(),
        v4: ObjectStateV4::from_card_def(card_def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    match zone {
        Zone::Hand => state.players[owner.index()].hand.push(id),
        Zone::Battlefield => state.players[owner.index()].battlefield.push(id),
        Zone::Library => state.players[owner.index()].library.push(id),
        Zone::Graveyard => state.players[owner.index()].graveyard.push(id),
        Zone::Exile => state.exile.push(id),
        Zone::Command => state.command.push(id),
        Zone::Stack => panic!("test helper does not create stack items"),
    }
    id
}

fn put_object(
    state: &mut GameState,
    player: PlayerId,
    name: &str,
    zone: Zone,
    tapped: bool,
    summoning_sick: bool,
) -> ObjectId {
    put_object_controlled_by(state, player, player, name, zone, tapped, summoning_sick)
}

fn resolve_current_stack(state: &mut GameState) {
    for _ in 0..16 {
        let decision = engine::advance_until_decision(state);
        match decision {
            Decision::CastSpellOrPass { .. } if state.stack.is_empty() => return,
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => panic!("unexpected decision while resolving stack: {other:?}"),
        }
    }
    panic!("stack did not resolve in bounded priority walk");
}

fn next_effect_target_choice(state: &mut GameState, source: ObjectId) -> Decision {
    for _ in 0..16 {
        let decision = engine::advance_until_decision(state);
        match decision {
            choice @ Decision::ChooseEffectTargets { source: actual, .. } if actual == source => {
                return choice
            }
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => panic!("unexpected decision before effect choice: {other:?}"),
        }
    }
    panic!("effect did not reach its target-selection choice");
}

#[test]
fn registry_ids_and_generated_recipes_are_stable_and_full() {
    let expected = [
        ("Lead the Stampede", 64_u16),
        ("Priest of Titania", 87),
        ("Quirion Ranger", 91),
        ("Timberwatch Elf", 119),
        ("Wellwisher", 129),
    ];
    let ids = expected
        .iter()
        .map(|(name, expected_id)| {
            let id = card_id(name);
            assert_eq!(id, *expected_id, "append-only id for {name}");
            assert_eq!(CARD_DEFS[id as usize].capability, CardCapability::Full);
            id
        })
        .collect::<Vec<_>>();
    preflight_fully_supported_deck(&ids).unwrap();

    assert_eq!(
        (CARD_DEFS[ids[0] as usize].spell_effect)(),
        Some(EffectOp::LookTopSelectByTypeToHandBottomRest {
            player: PlayerRef::Controller,
            count: 5,
            card_type: mtg_kernel::card_def::CardType::Creature,
        })
    );

    let priest = CARD_DEFS[ids[1] as usize].mana_ability_def.unwrap();
    assert_eq!(priest.cost, ManaAbilityCostDef::TapSelf);
    assert_eq!(
        priest.amount,
        ManaAbilityAmountDef::Dynamic(DynamicValueDef::BattlefieldPermanentsWithSubtype(
            Subtype::Elf
        ))
    );

    let ranger = &CARD_DEFS[ids[2] as usize].activated_abilities[0];
    assert!(matches!(
        ranger.cost,
        [CostComponent::ReturnControlledPermanentToOwnersHand(
            PermanentFilterDef::LandWithSubtype(Subtype::Forest)
        )]
    ));
    assert_eq!(ranger.max_activations_per_turn, Some(1));
    assert_eq!(
        (ranger.effect)(),
        EffectOp::UntapObject {
            object: ObjectRef::Target(0)
        }
    );

    assert_eq!(
        (CARD_DEFS[ids[3] as usize].activated_abilities[0].effect)(),
        EffectOp::PumpTargetUntilEndOfTurnDynamic {
            target: TargetRef::Target(0),
            power: DynamicValueDef::BattlefieldPermanentsWithSubtype(Subtype::Elf),
            toughness: DynamicValueDef::BattlefieldPermanentsWithSubtype(Subtype::Elf),
        }
    );
    assert_eq!(
        (CARD_DEFS[ids[4] as usize].activated_abilities[0].effect)(),
        EffectOp::GainLifeDynamic {
            player: PlayerRef::Controller,
            amount: DynamicValueDef::BattlefieldPermanentsWithSubtype(Subtype::Elf),
        }
    );
}

#[test]
fn priest_counts_every_elf_on_both_battlefields_and_obeys_tap_sickness() {
    let mut state = ready_main1();
    let priest = put_object(
        &mut state,
        PlayerId::P0,
        "Priest of Titania",
        Zone::Battlefield,
        false,
        true,
    );
    put_object(
        &mut state,
        PlayerId::P0,
        "Quirion Ranger",
        Zone::Battlefield,
        true,
        true,
    );
    put_object(
        &mut state,
        PlayerId::P1,
        "Timberwatch Elf",
        Zone::Battlefield,
        false,
        false,
    );
    put_object(
        &mut state,
        PlayerId::P0,
        "Overgrown Battlement",
        Zone::Battlefield,
        false,
        false,
    );

    assert!(engine::step(&mut state, Action::ActivateManaAbility(priest)).is_err());
    state.objects.get_mut(priest).summoning_sick = false;
    engine::step(&mut state, Action::ActivateManaAbility(priest)).unwrap();
    assert!(state.objects.get(priest).tapped);
    assert_eq!(state.players[0].mana_pool[ManaColor::G.pool_index()], 3);
    assert!(engine::step(&mut state, Action::ActivateManaAbility(priest)).is_err());
}

#[test]
fn quirion_uses_generic_filtered_return_cost_owner_hand_and_once_each_turn() {
    let mut state = ready_main1();
    let ranger = put_object(
        &mut state,
        PlayerId::P0,
        "Quirion Ranger",
        Zone::Battlefield,
        true,
        true,
    );
    let target = put_object(
        &mut state,
        PlayerId::P1,
        "Overgrown Battlement",
        Zone::Battlefield,
        true,
        false,
    );
    let own_forest = put_object(
        &mut state,
        PlayerId::P0,
        "Forest",
        Zone::Battlefield,
        true,
        false,
    );
    let borrowed_forest = put_object_controlled_by(
        &mut state,
        PlayerId::P1,
        PlayerId::P0,
        "Forest",
        Zone::Battlefield,
        true,
        false,
    );
    let opposing_forest = put_object(
        &mut state,
        PlayerId::P1,
        "Forest",
        Zone::Battlefield,
        false,
        false,
    );
    let island = put_object(
        &mut state,
        PlayerId::P0,
        "Island",
        Zone::Battlefield,
        false,
        false,
    );

    engine::step(&mut state, Action::ActivateAbility(ranger, 0)).unwrap();
    let Decision::ChooseTargets { legal_targets, .. } = engine::advance_until_decision(&mut state)
    else {
        panic!("Quirion Ranger should choose a creature target");
    };
    assert!(legal_targets.contains(&Target::Object(target)));
    engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();

    let cost_decision = engine::advance_until_decision(&mut state);
    let Decision::ChooseCostTargets {
        cost_kind,
        candidates,
        remaining,
        ..
    } = &cost_decision
    else {
        panic!("Quirion Ranger should choose a Forest cost");
    };
    assert_eq!(*cost_kind, CostKind::ReturnPermanentsToHand);
    assert_eq!(*remaining, 1);
    assert_eq!(candidates, &vec![own_forest, borrowed_forest]);
    assert!(!candidates.contains(&opposing_forest));
    assert!(!candidates.contains(&island));

    let actions =
        legal_action_candidates_v1(&SurfaceDecision::Decision(cost_decision.clone()), &state)
            .unwrap();
    assert_eq!(actions.len(), 2);
    assert!(actions.iter().all(|candidate| matches!(
        candidate.record.semantic,
        ActionSemanticV1::ChooseCostTarget {
            cost_kind: CostKind::ReturnPermanentsToHand,
            ..
        }
    )));

    let before_choice_hash = state.diagnostic_state_hash();
    engine::step(&mut state, Action::ChooseCostTarget(borrowed_forest)).unwrap();
    assert_ne!(state.diagnostic_state_hash(), before_choice_hash);
    let encoded = serde_json::to_string(&state).unwrap();
    assert!(encoded.contains("object_cost_chosen"));
    let restored: GameState = serde_json::from_str(&encoded).unwrap();
    assert_eq!(restored, state);

    let _ = engine::advance_until_decision(&mut state);
    assert_eq!(state.objects.get(borrowed_forest).zone, Zone::Hand);
    assert!(state.players[PlayerId::P1.index()]
        .hand
        .contains(&borrowed_forest));
    assert!(
        state.objects.get(ranger).tapped,
        "the ability must not change its already-tapped source"
    );
    resolve_current_stack(&mut state);
    assert!(!state.objects.get(target).tapped);
    assert!(engine::step(&mut state, Action::ActivateAbility(ranger, 0)).is_err());
}

#[test]
fn timberwatch_samples_all_elves_at_resolution_and_binds_target_incarnation() {
    let mut state = ready_main1();
    let timberwatch = put_object(
        &mut state,
        PlayerId::P0,
        "Timberwatch Elf",
        Zone::Battlefield,
        false,
        false,
    );
    put_object(
        &mut state,
        PlayerId::P0,
        "Quirion Ranger",
        Zone::Battlefield,
        false,
        true,
    );
    put_object(
        &mut state,
        PlayerId::P1,
        "Elvish Mystic",
        Zone::Battlefield,
        false,
        false,
    );
    let target = put_object(
        &mut state,
        PlayerId::P1,
        "Overgrown Battlement",
        Zone::Battlefield,
        false,
        false,
    );

    engine::step(&mut state, Action::ActivateAbility(timberwatch, 0)).unwrap();
    let Decision::ChooseTargets { legal_targets, .. } = engine::advance_until_decision(&mut state)
    else {
        panic!("Timberwatch should choose a creature target");
    };
    assert!(legal_targets.contains(&Target::Object(target)));
    engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
    let _ = engine::advance_until_decision(&mut state);
    assert!(state.objects.get(timberwatch).tapped);

    put_object(
        &mut state,
        PlayerId::P1,
        "Wellwisher",
        Zone::Battlefield,
        false,
        false,
    );
    resolve_current_stack(&mut state);
    assert_eq!(engine::effective_power(&state, target), 4);
    assert_eq!(engine::effective_toughness(&state, target), 8);

    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(target, Zone::Graveyard),
    );
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(target, Zone::Battlefield),
    );
    assert_eq!(engine::effective_power(&state, target), 0);
    assert_eq!(engine::effective_toughness(&state, target), 4);

    let mut sick = ready_main1();
    let timberwatch = put_object(
        &mut sick,
        PlayerId::P0,
        "Timberwatch Elf",
        Zone::Battlefield,
        false,
        true,
    );
    put_object(
        &mut sick,
        PlayerId::P0,
        "Elvish Mystic",
        Zone::Battlefield,
        false,
        false,
    );
    assert!(engine::step(&mut sick, Action::ActivateAbility(timberwatch, 0)).is_err());
}

#[test]
fn wellwisher_counts_at_resolution_including_itself_and_opposing_elves() {
    let mut state = ready_main1();
    let wellwisher = put_object(
        &mut state,
        PlayerId::P0,
        "Wellwisher",
        Zone::Battlefield,
        false,
        false,
    );
    put_object(
        &mut state,
        PlayerId::P0,
        "Quirion Ranger",
        Zone::Battlefield,
        false,
        false,
    );
    let opposing_elf = put_object(
        &mut state,
        PlayerId::P1,
        "Elvish Mystic",
        Zone::Battlefield,
        false,
        false,
    );
    let life = state.players[0].life;

    engine::step(&mut state, Action::ActivateAbility(wellwisher, 0)).unwrap();
    let _ = engine::advance_until_decision(&mut state);
    assert!(state.objects.get(wellwisher).tapped);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(opposing_elf, Zone::Graveyard),
    );
    resolve_current_stack(&mut state);
    assert_eq!(state.players[0].life, life + 2);
}

fn ready_lead() -> (GameState, ObjectId, Vec<ObjectId>) {
    let names = [
        "Elvish Mystic",
        "Mountain",
        "Quirion Ranger",
        "Island",
        "Lightning Bolt",
        "Forest",
    ];
    let definitions = names.iter().map(|name| card_id(name)).collect::<Vec<_>>();
    let mut state = GameState::new_from_libraries(
        &definitions,
        &[card_id("Forest")],
        card_name,
        0x4c45_4144_5f45_4c46,
    );
    state.step = Step::Main1;
    state.active_player = PlayerId::P0;
    state.priority_player = PlayerId::P0;
    for _ in 0..3 {
        put_object(
            &mut state,
            PlayerId::P0,
            "Forest",
            Zone::Battlefield,
            false,
            false,
        );
    }
    let lead = put_object(
        &mut state,
        PlayerId::P0,
        "Lead the Stampede",
        Zone::Hand,
        false,
        false,
    );
    let library = state.players[0].library.clone();
    (state, lead, library)
}

#[test]
fn lead_private_subset_public_reveal_bottom_order_and_rl_projection_are_exact() {
    let (mut state, lead, library) = ready_lead();
    engine::step(&mut state, Action::CastSpell(lead)).unwrap();
    let subset = next_effect_target_choice(&mut state, lead);
    assert!(matches!(
        subset,
        Decision::ChooseEffectTargets {
            selected_count: 0,
            min_targets: 0,
            max_targets: 2,
            ref legal_targets,
            can_finish: true,
            ..
        } if legal_targets == &vec![Target::Object(library[0]), Target::Object(library[2])]
    ));
    assert!(matches!(
        state
            .engine
            .pending_effect
            .as_ref()
            .and_then(|pending| pending.choice.as_ref())
            .unwrap(),
        mtg_kernel::effect::PendingEffectChoice::SelectTargets {
            purpose: EffectTargetSelectionPurpose::LookTopSelectByTypeToHandBottomRest {
                stage: LibraryPartitionSelectionStage::ChooseMatchingSubset,
                ..
            },
            ..
        }
    ));

    let chooser_actions =
        legal_action_candidates_v1(&SurfaceDecision::Decision(subset.clone()), &state).unwrap();
    assert_eq!(chooser_actions.len(), 3);
    assert!(chooser_actions.iter().any(|candidate| matches!(
        candidate.record.semantic,
        ActionSemanticV1::FinishEffectSelection { .. }
    )));
    let opponent = observe_v2(&state, &HarnessSurfaceV2::new(), PlayerId::P1, 1).unwrap();
    assert!(matches!(
        opponent
            .projection
            .engine_context
            .pending_effect
            .unwrap()
            .choice,
        Some(PendingEffectChoiceSemanticV4::Targets {
            purpose: TargetSelectionPurposeV4::CardSelection,
            ref legal_targets,
            max_targets: 0,
            ..
        }) if legal_targets.is_empty()
    ));

    let unchanged = state.clone();
    assert!(engine::step(
        &mut state,
        Action::ChooseEffectTarget(Target::Object(library[1]))
    )
    .is_err());
    assert_eq!(state, unchanged);

    engine::step(
        &mut state,
        Action::ChooseEffectTarget(Target::Object(library[2])),
    )
    .unwrap();
    let selected_hash = state.diagnostic_state_hash();
    let round_trip: GameState =
        serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
    assert_eq!(round_trip, state);
    assert_eq!(round_trip.diagnostic_state_hash(), selected_hash);
    engine::step(&mut state, Action::FinishEffectSelection).unwrap();

    let order = engine::advance_until_decision(&mut state);
    assert!(matches!(
        order,
        Decision::ChooseEffectTargets {
            selected_count: 0,
            min_targets: 4,
            max_targets: 4,
            can_finish: false,
            ..
        }
    ));
    let order_actions =
        legal_action_candidates_v1(&SurfaceDecision::Decision(order), &state).unwrap();
    assert!(order_actions.iter().all(|candidate| matches!(
        candidate.record.semantic,
        ActionSemanticV1::ChooseEffectTarget {
            target: TargetRefV1::Object { .. },
            ..
        }
    )));
    let chooser_observation =
        observe_v2(&state, &HarnessSurfaceV2::new(), PlayerId::P0, 2).unwrap();
    assert!(matches!(
        chooser_observation
            .projection
            .engine_context
            .pending_effect
            .unwrap()
            .choice,
        Some(PendingEffectChoiceSemanticV4::Targets {
            purpose: TargetSelectionPurposeV4::LibraryOrder,
            ..
        })
    ));

    for object in [library[4], library[3], library[1]] {
        engine::step(
            &mut state,
            Action::ChooseEffectTarget(Target::Object(object)),
        )
        .unwrap();
        let _ = engine::advance_until_decision(&mut state);
    }
    assert!(state.engine.pending_effect.is_none());
    assert_eq!(state.players[0].hand, vec![library[2]]);
    assert_eq!(state.objects.get(lead).zone, Zone::Graveyard);
    assert_eq!(
        state.players[0].library,
        vec![library[5], library[4], library[3], library[1], library[0]]
    );
    assert!(state
        .known_hand_cards(PlayerId::P1, PlayerId::P0)
        .iter()
        .any(|entry| entry.object == library[2]));
}

#[test]
fn lead_pending_selection_and_order_are_hash_sensitive() {
    let (mut state, lead, library) = ready_lead();
    engine::step(&mut state, Action::CastSpell(lead)).unwrap();
    let _ = next_effect_target_choice(&mut state, lead);
    let initial_hash = state.diagnostic_state_hash();
    engine::step(
        &mut state,
        Action::ChooseEffectTarget(Target::Object(library[0])),
    )
    .unwrap();
    assert_ne!(state.diagnostic_state_hash(), initial_hash);
    engine::step(&mut state, Action::FinishEffectSelection).unwrap();
    let answered_hash = state.diagnostic_state_hash();
    let _ = engine::advance_until_decision(&mut state);
    assert_ne!(state.diagnostic_state_hash(), answered_hash);
}
