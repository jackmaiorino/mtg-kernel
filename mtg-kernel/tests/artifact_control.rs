//! Focused current-Mage parity for the Affinity/Wildfire artifact-control wave.

use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardType, CostComponent, Keywords, PermanentFilter,
    TargetSpec, CARD_DEFS,
};
use mtg_kernel::effect::{
    CreatureFilter, EffectAnsweredChoiceGuard, EffectBooleanChoicePurpose, EffectFrame,
    EffectObjectBinding, EffectOp,
};
use mtg_kernel::engine::{self, Action, CostKind, Decision, UnsupportedMechanic};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{Cost, ManaColor};
use mtg_kernel::policy_surface_v5::PolicySurfaceV5;
use mtg_kernel::rl::{
    legal_action_candidates_v1, observe_policy_v5, ActionSemanticV1, BooleanChoicePurposeV4,
};
use mtg_kernel::state::{
    Counters, GameObject, GameState, ObjectStateV4, StackItemKind, Step, Target, Zone,
};
use mtg_kernel::surface_v2::SurfaceDecision;

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

fn put_object(
    state: &mut GameState,
    owner: PlayerId,
    controller: PlayerId,
    name: &str,
    zone: Zone,
) -> ObjectId {
    let card_def = card_id(name);
    let id = state.objects.push(GameObject {
        card_def,
        name: name.to_string(),
        owner,
        controller,
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
        Zone::Hand => state.players[owner.index()].hand.push(id),
        Zone::Battlefield => state.players[controller.index()].battlefield.push(id),
        Zone::Library => state.players[owner.index()].library.push(id),
        Zone::Graveyard => state.players[owner.index()].graveyard.push(id),
        Zone::Exile => state.exile.push(id),
        Zone::Command => state.command.push(id),
        Zone::Stack => panic!("test helper does not construct stack objects"),
    }
    id
}

fn put_owned(state: &mut GameState, name: &str, zone: Zone) -> ObjectId {
    put_object(state, PlayerId::P0, PlayerId::P0, name, zone)
}

fn pass_until_idle(state: &mut GameState) {
    for _ in 0..128 {
        let decision = engine::advance_until_decision(state);
        if state.stack.is_empty()
            && state.engine.pending_effect.is_none()
            && matches!(decision, Decision::CastSpellOrPass { .. })
        {
            return;
        }
        match decision {
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => panic!("unexpected decision while resolving to idle: {other:?}"),
        }
    }
    panic!("bounded resolution did not become idle");
}

fn advance_to_effect_boolean(state: &mut GameState) -> Decision {
    for _ in 0..64 {
        let decision = engine::advance_until_decision(state);
        match decision {
            boolean @ Decision::ChooseEffectBoolean { .. } => return boolean,
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => panic!("unexpected decision before Boolean prompt: {other:?}"),
        }
    }
    panic!("bounded advance did not reach a Boolean prompt");
}

fn advance_to_effect_targets(state: &mut GameState) -> Decision {
    for _ in 0..64 {
        let decision = engine::advance_until_decision(state);
        match decision {
            targets @ Decision::ChooseEffectTargets { .. } => return targets,
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => panic!("unexpected decision before effect targets: {other:?}"),
        }
    }
    panic!("bounded advance did not reach effect targets");
}

#[test]
fn generated_definitions_match_the_four_current_mage_cards() {
    assert_eq!(card_id("Krark-Clan Shaman"), 62);
    assert_eq!(card_id("Makeshift Munitions"), 69);
    assert_eq!(card_id("Nihil Spellbomb"), 79);
    assert_eq!(card_id("Relic of Progenitus"), 97);
    assert_eq!(CARD_DEFS.len(), 162);

    for name in [
        "Krark-Clan Shaman",
        "Makeshift Munitions",
        "Nihil Spellbomb",
        "Relic of Progenitus",
    ] {
        let def = &CARD_DEFS[card_id(name) as usize];
        assert_eq!(def.capability, CardCapability::Full, "{name}");
        assert_eq!(
            def.activated_abilities.len(),
            if name == "Relic of Progenitus" { 2 } else { 1 }
        );
    }

    let shaman = &CARD_DEFS[card_id("Krark-Clan Shaman") as usize];
    assert_eq!(shaman.types, &[CardType::Creature]);
    assert_eq!(shaman.power, Some(1));
    assert_eq!(shaman.toughness, Some(1));
    assert_eq!(
        shaman.activated_abilities[0].cost,
        &[CostComponent::SacrificeControlled {
            count: 1,
            filter: PermanentFilter::Artifact,
        }]
    );
    assert_eq!(
        (shaman.activated_abilities[0].effect)(),
        EffectOp::DamageAllCreatures {
            filter: CreatureFilter::WithoutKeyword(Keywords::FLYING),
            amount: 1,
        }
    );

    let munitions = &CARD_DEFS[card_id("Makeshift Munitions") as usize];
    assert_eq!(munitions.target_spec, TargetSpec::None);
    assert_eq!(
        munitions.activated_abilities[0].target_spec,
        TargetSpec::AnyTarget
    );
    assert_eq!(
        munitions.activated_abilities[0].cost,
        &[
            CostComponent::Mana(Cost {
                pips: &[],
                generic: 1,
                x_count: 0,
            }),
            CostComponent::SacrificeControlled {
                count: 1,
                filter: PermanentFilter::ArtifactOrCreature,
            },
        ]
    );

    let nihil = &CARD_DEFS[card_id("Nihil Spellbomb") as usize];
    assert_eq!(
        nihil.activated_abilities[0].target_spec,
        TargetSpec::AnyPlayer
    );
    assert_eq!(
        nihil.activated_abilities[0].cost,
        &[CostComponent::Tap, CostComponent::SacrificeSelf]
    );

    let relic = &CARD_DEFS[card_id("Relic of Progenitus") as usize];
    assert_eq!(
        relic.activated_abilities[0].target_spec,
        TargetSpec::AnyPlayer
    );
    assert_eq!(relic.activated_abilities[0].cost, &[CostComponent::Tap]);
    assert_eq!(relic.activated_abilities[1].target_spec, TargetSpec::None);
    assert_eq!(
        relic.activated_abilities[1].cost,
        &[
            CostComponent::Mana(Cost {
                pips: &[],
                generic: 1,
                x_count: 0,
            }),
            CostComponent::ExileSelf,
        ]
    );
}

#[test]
fn krark_clan_shaman_filters_artifacts_and_damages_each_nonflier() {
    let mut state = ready_main(0x4b52_4152_4b00_0001);
    let shaman = put_owned(&mut state, "Krark-Clan Shaman", Zone::Battlefield);
    let blood = put_owned(&mut state, "Blood Token", Zone::Battlefield);
    let furnace = put_owned(&mut state, "Great Furnace", Zone::Battlefield);
    let enforcer = put_owned(&mut state, "Myr Enforcer", Zone::Battlefield);
    let mountain = put_owned(&mut state, "Mountain", Zone::Battlefield);
    let opponent_artifact = put_object(
        &mut state,
        PlayerId::P1,
        PlayerId::P1,
        "Blood Token",
        Zone::Battlefield,
    );
    let opponent_nonflier = put_object(
        &mut state,
        PlayerId::P1,
        PlayerId::P1,
        "Voldaren Epicure",
        Zone::Battlefield,
    );
    let opponent_flier = put_object(
        &mut state,
        PlayerId::P1,
        PlayerId::P1,
        "Sagu Wildling",
        Zone::Battlefield,
    );

    engine::step(&mut state, Action::ActivateAbility(shaman, 0)).unwrap();
    let decision = engine::advance_until_decision(&mut state);
    let candidates = match decision {
        Decision::ChooseCostTargets {
            cost_kind: CostKind::SacrificeArtifacts,
            remaining: 1,
            candidates,
            ..
        } => candidates,
        other => panic!("expected artifact sacrifice choice, got {other:?}"),
    };
    assert_eq!(candidates, vec![blood, furnace, enforcer]);
    assert!(!candidates.contains(&mountain));
    assert!(!candidates.contains(&opponent_artifact));

    engine::step(&mut state, Action::ChooseCostTarget(blood)).unwrap();
    let bytes = serde_json::to_vec(&state).unwrap();
    let restored: GameState = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(state, restored);
    assert_eq!(
        state.diagnostic_state_hash(),
        restored.diagnostic_state_hash()
    );

    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    let ability = state.stack.last().unwrap();
    assert_eq!(ability.kind, StackItemKind::ActivatedAbility);
    assert_eq!(ability.v4.paid_cost_refs[0].object, blood);
    pass_until_idle(&mut state);

    assert_eq!(state.objects.get(blood).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(shaman).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(opponent_nonflier).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(opponent_flier).zone, Zone::Battlefield);
    assert_eq!(state.objects.get(opponent_flier).damage, 0);
    assert_eq!(state.objects.get(enforcer).damage, 1);
}

#[test]
fn makeshift_munitions_targets_before_payment_and_roundtrips_cost_state() {
    let mut state = ready_main(0x4d55_4e49_5449_4f4e);
    let munitions = put_owned(&mut state, "Makeshift Munitions", Zone::Battlefield);
    let creature = put_owned(&mut state, "Voldaren Epicure", Zone::Battlefield);
    let furnace = put_owned(&mut state, "Great Furnace", Zone::Battlefield);
    let mountain = put_owned(&mut state, "Mountain", Zone::Battlefield);
    let opponent_artifact = put_object(
        &mut state,
        PlayerId::P1,
        PlayerId::P1,
        "Blood Token",
        Zone::Battlefield,
    );
    state.players[0].mana_pool[ManaColor::R.pool_index()] = 1;

    engine::step(&mut state, Action::ActivateAbility(munitions, 0)).unwrap();
    let targets = engine::advance_until_decision(&mut state);
    assert!(matches!(targets, Decision::ChooseTargets { .. }));
    engine::step(
        &mut state,
        Action::ChooseTarget(Target::Player(PlayerId::P1)),
    )
    .unwrap();

    let decision = engine::advance_until_decision(&mut state);
    let candidates = match &decision {
        Decision::ChooseCostTargets {
            cost_kind: CostKind::SacrificePermanents,
            remaining: 1,
            candidates,
            ..
        } => candidates.clone(),
        other => panic!("expected artifact-or-creature sacrifice, got {other:?}"),
    };
    assert_eq!(candidates, vec![creature, furnace]);
    assert!(!candidates.contains(&mountain));
    assert!(!candidates.contains(&opponent_artifact));

    let actions =
        legal_action_candidates_v1(&SurfaceDecision::Decision(decision.clone()), &state).unwrap();
    assert_eq!(actions.len(), 2);
    assert!(actions.iter().all(|candidate| matches!(
        candidate.record.semantic,
        ActionSemanticV1::ChooseCostTarget { .. }
    )));
    let bytes = serde_json::to_vec(&state).unwrap();
    let mut restored: GameState = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(decision, engine::advance_until_decision(&mut restored));
    assert_eq!(
        state.diagnostic_state_hash(),
        restored.diagnostic_state_hash()
    );

    engine::step(&mut state, Action::ChooseCostTarget(creature)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    assert_eq!(
        state.stack.last().unwrap().v4.paid_cost_refs[0].object,
        creature
    );
    pass_until_idle(&mut state);
    assert_eq!(state.players[1].life, 19);
    assert_eq!(state.objects.get(creature).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(munitions).zone, Zone::Battlefield);

    let mut artifact_land_payment = ready_main(0x4d55_4e49_5449_4f50);
    let source = put_owned(
        &mut artifact_land_payment,
        "Makeshift Munitions",
        Zone::Battlefield,
    );
    let furnace = put_owned(
        &mut artifact_land_payment,
        "Great Furnace",
        Zone::Battlefield,
    );
    engine::step(
        &mut artifact_land_payment,
        Action::ActivateAbility(source, 0),
    )
    .unwrap();
    engine::advance_until_decision(&mut artifact_land_payment);
    engine::step(
        &mut artifact_land_payment,
        Action::ChooseTarget(Target::Player(PlayerId::P1)),
    )
    .unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut artifact_land_payment),
        Decision::CastSpellOrPass { .. }
    ));
    assert_eq!(
        artifact_land_payment.objects.get(furnace).zone,
        Zone::Graveyard
    );
    pass_until_idle(&mut artifact_land_payment);
    assert_eq!(artifact_land_payment.players[1].life, 19);

    let mut unpayable = ready_main(0x4d55_4e49_5449_4f4f);
    let source = put_owned(&mut unpayable, "Makeshift Munitions", Zone::Battlefield);
    put_owned(&mut unpayable, "Blood Token", Zone::Battlefield);
    assert!(engine::step(&mut unpayable, Action::ActivateAbility(source, 0)).is_err());
}

#[test]
fn nihil_spellbomb_trigger_may_pay_black_then_target_graveyard_is_exiled() {
    let mut state = ready_main(0x4e49_4849_4c00_0001);
    let spellbomb = put_owned(&mut state, "Nihil Spellbomb", Zone::Battlefield);
    let drawn = put_owned(&mut state, "Mountain", Zone::Library);
    let own_grave = put_owned(&mut state, "Forest", Zone::Graveyard);
    let first = put_object(
        &mut state,
        PlayerId::P1,
        PlayerId::P1,
        "Island",
        Zone::Graveyard,
    );
    let second = put_object(
        &mut state,
        PlayerId::P1,
        PlayerId::P1,
        "Voldaren Epicure",
        Zone::Graveyard,
    );
    state.players[0].mana_pool[ManaColor::B.pool_index()] = 1;

    engine::step(&mut state, Action::ActivateAbility(spellbomb, 0)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::ChooseTargets { .. }
    ));
    engine::step(
        &mut state,
        Action::ChooseTarget(Target::Player(PlayerId::P1)),
    )
    .unwrap();

    let boolean = advance_to_effect_boolean(&mut state);
    match &boolean {
        Decision::ChooseEffectBoolean {
            player,
            purpose:
                EffectBooleanChoicePurpose::PayManaThen {
                    player: payer,
                    colored,
                    generic,
                    ..
                },
            ..
        } => {
            assert_eq!((*player, *payer), (PlayerId::P0, PlayerId::P0));
            assert_eq!(colored, &[ManaColor::B]);
            assert_eq!(*generic, 0);
        }
        other => panic!("expected optional black payment, got {other:?}"),
    }
    let actions =
        legal_action_candidates_v1(&SurfaceDecision::Decision(boolean.clone()), &state).unwrap();
    assert_eq!(actions.len(), 2);
    assert!(matches!(
        observe_policy_v5(&state, &PolicySurfaceV5::new(), PlayerId::P0, 0, 0, 0, 1,)
            .unwrap()
            .projection
            .surface
            .engine_context
            .pending_effect
            .unwrap()
            .choice
            .unwrap(),
        mtg_kernel::rl::PendingEffectChoiceSemanticV4::Boolean {
            purpose: BooleanChoicePurposeV4::PayCost,
            ..
        }
    ));
    let bytes = serde_json::to_vec(&state).unwrap();
    let restored: GameState = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(state, restored);
    assert_eq!(
        state.diagnostic_state_hash(),
        restored.diagnostic_state_hash()
    );

    engine::step(&mut state, Action::ChooseEffectBoolean(true)).unwrap();
    let accepted_bytes = serde_json::to_vec(&state).unwrap();
    let accepted_restored: GameState = serde_json::from_slice(&accepted_bytes).unwrap();
    assert_eq!(state, accepted_restored);
    assert_eq!(
        state.diagnostic_state_hash(),
        accepted_restored.diagnostic_state_hash()
    );
    let mut free_draw_tamper = state.clone();
    let continuation = free_draw_tamper.engine.pending_effect.as_mut().unwrap();
    let remove_black = |frame: &mut EffectFrame| {
        let EffectFrame::PayManaThen { colored, .. } = frame else {
            panic!("typed optional-mana answered frame")
        };
        colored.clear();
    };
    remove_black(continuation.frames.last_mut().unwrap());
    let Some(EffectAnsweredChoiceGuard::PayManaThen { frame }) =
        continuation.answered_choice_guard.as_mut()
    else {
        panic!("typed optional-mana answered guard")
    };
    remove_black(frame);
    let _ = engine::advance_until_decision(&mut free_draw_tamper);
    assert_eq!(
        free_draw_tamper.engine.halted,
        Some((UnsupportedMechanic::InvalidEffectContinuation, spellbomb))
    );
    assert_eq!(free_draw_tamper.objects.get(drawn).zone, Zone::Library);
    assert_eq!(
        free_draw_tamper.players[0].mana_pool[ManaColor::B.pool_index()],
        1
    );

    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    assert_eq!(state.players[0].mana_pool[ManaColor::B.pool_index()], 0);
    assert_eq!(state.objects.get(drawn).zone, Zone::Hand);
    pass_until_idle(&mut state);

    assert_eq!(state.objects.get(spellbomb).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(own_grave).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(first).zone, Zone::Exile);
    assert_eq!(state.objects.get(second).zone, Zone::Exile);

    let mut no_black = ready_main(0x4e49_4849_4c00_0002);
    let spellbomb = put_owned(&mut no_black, "Nihil Spellbomb", Zone::Battlefield);
    let undrawn = put_owned(&mut no_black, "Mountain", Zone::Library);
    let grave = put_object(
        &mut no_black,
        PlayerId::P1,
        PlayerId::P1,
        "Island",
        Zone::Graveyard,
    );
    engine::step(&mut no_black, Action::ActivateAbility(spellbomb, 0)).unwrap();
    engine::advance_until_decision(&mut no_black);
    engine::step(
        &mut no_black,
        Action::ChooseTarget(Target::Player(PlayerId::P1)),
    )
    .unwrap();
    pass_until_idle(&mut no_black);
    assert_eq!(no_black.objects.get(grave).zone, Zone::Exile);
    assert_eq!(no_black.objects.get(undrawn).zone, Zone::Library);

    let mut declined = ready_main(0x4e49_4849_4c00_0003);
    let spellbomb = put_owned(&mut declined, "Nihil Spellbomb", Zone::Battlefield);
    let undrawn = put_owned(&mut declined, "Mountain", Zone::Library);
    declined.players[0].mana_pool[ManaColor::B.pool_index()] = 1;
    engine::step(&mut declined, Action::ActivateAbility(spellbomb, 0)).unwrap();
    engine::advance_until_decision(&mut declined);
    engine::step(
        &mut declined,
        Action::ChooseTarget(Target::Player(PlayerId::P1)),
    )
    .unwrap();
    advance_to_effect_boolean(&mut declined);
    engine::step(&mut declined, Action::ChooseEffectBoolean(false)).unwrap();
    pass_until_idle(&mut declined);
    assert_eq!(declined.objects.get(undrawn).zone, Zone::Library);
    assert_eq!(declined.players[0].mana_pool[ManaColor::B.pool_index()], 1);
}

#[test]
fn relic_first_ability_uses_target_player_choice_and_second_exiles_everything() {
    let mut first_state = ready_main(0x5245_4c49_4300_0001);
    let relic = put_owned(&mut first_state, "Relic of Progenitus", Zone::Battlefield);
    let first = put_object(
        &mut first_state,
        PlayerId::P1,
        PlayerId::P1,
        "Island",
        Zone::Graveyard,
    );
    let second = put_object(
        &mut first_state,
        PlayerId::P1,
        PlayerId::P1,
        "Mountain",
        Zone::Graveyard,
    );
    engine::step(&mut first_state, Action::ActivateAbility(relic, 0)).unwrap();
    engine::advance_until_decision(&mut first_state);
    engine::step(
        &mut first_state,
        Action::ChooseTarget(Target::Player(PlayerId::P1)),
    )
    .unwrap();

    let choice = advance_to_effect_targets(&mut first_state);
    match &choice {
        Decision::ChooseEffectTargets {
            player,
            selected_count: 0,
            min_targets: 1,
            max_targets: 1,
            legal_targets,
            can_finish: false,
            ..
        } => {
            assert_eq!(*player, PlayerId::P1);
            assert_eq!(
                legal_targets,
                &[Target::Object(first), Target::Object(second)]
            );
        }
        other => panic!("expected target player's graveyard choice, got {other:?}"),
    }
    let bytes = serde_json::to_vec(&first_state).unwrap();
    let mut restored: GameState = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(choice, engine::advance_until_decision(&mut restored));
    assert_eq!(
        first_state.diagnostic_state_hash(),
        restored.diagnostic_state_hash()
    );
    engine::step(
        &mut first_state,
        Action::ChooseEffectTarget(Target::Object(second)),
    )
    .unwrap();
    let answered_bytes = serde_json::to_vec(&first_state).unwrap();
    let answered_restored: GameState = serde_json::from_slice(&answered_bytes).unwrap();
    assert_eq!(first_state, answered_restored);
    assert_eq!(
        first_state.diagnostic_state_hash(),
        answered_restored.diagnostic_state_hash()
    );
    let mut redirected = first_state.clone();
    let source_binding = EffectObjectBinding {
        object: relic,
        expected_zone: Zone::Battlefield,
        expected_zone_change_count: redirected.objects.get(relic).zone_change_count,
    };
    let continuation = redirected.engine.pending_effect.as_mut().unwrap();
    let rewrite_choice = |frame: &mut EffectFrame| {
        let EffectFrame::ExileChosenGraveyardCard { chosen, .. } = frame else {
            panic!("typed graveyard-exile answered frame")
        };
        *chosen = source_binding;
    };
    rewrite_choice(continuation.frames.last_mut().unwrap());
    let Some(EffectAnsweredChoiceGuard::ExileOneFromGraveyard { frame }) =
        continuation.answered_choice_guard.as_mut()
    else {
        panic!("typed graveyard-exile answered guard")
    };
    rewrite_choice(frame);
    let _ = engine::advance_until_decision(&mut redirected);
    assert_eq!(
        redirected.engine.halted,
        Some((UnsupportedMechanic::InvalidEffectContinuation, relic))
    );
    assert_eq!(redirected.objects.get(first).zone, Zone::Graveyard);
    assert_eq!(redirected.objects.get(second).zone, Zone::Graveyard);
    assert_eq!(redirected.objects.get(relic).zone, Zone::Battlefield);

    pass_until_idle(&mut first_state);
    assert_eq!(first_state.objects.get(first).zone, Zone::Graveyard);
    assert_eq!(first_state.objects.get(second).zone, Zone::Exile);

    let mut singleton_state = ready_main(0x5245_4c49_4300_0003);
    let relic = put_owned(
        &mut singleton_state,
        "Relic of Progenitus",
        Zone::Battlefield,
    );
    let singleton = put_object(
        &mut singleton_state,
        PlayerId::P1,
        PlayerId::P1,
        "Island",
        Zone::Graveyard,
    );
    engine::step(&mut singleton_state, Action::ActivateAbility(relic, 0)).unwrap();
    engine::advance_until_decision(&mut singleton_state);
    engine::step(
        &mut singleton_state,
        Action::ChooseTarget(Target::Player(PlayerId::P1)),
    )
    .unwrap();
    pass_until_idle(&mut singleton_state);
    assert_eq!(singleton_state.objects.get(singleton).zone, Zone::Exile);

    let mut second_state = ready_main(0x5245_4c49_4300_0002);
    let relic = put_owned(&mut second_state, "Relic of Progenitus", Zone::Battlefield);
    let drawn = put_owned(&mut second_state, "Forest", Zone::Library);
    let own = put_owned(&mut second_state, "Mountain", Zone::Graveyard);
    let opposing = put_object(
        &mut second_state,
        PlayerId::P1,
        PlayerId::P1,
        "Island",
        Zone::Graveyard,
    );
    second_state.players[0].mana_pool[ManaColor::R.pool_index()] = 1;
    engine::step(&mut second_state, Action::ActivateAbility(relic, 1)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut second_state),
        Decision::CastSpellOrPass { .. }
    ));
    let ability = second_state.stack.last().unwrap();
    assert_eq!(ability.v4.paid_cost_refs[0].object, relic);
    assert_eq!(second_state.objects.get(relic).zone, Zone::Exile);
    pass_until_idle(&mut second_state);

    assert_eq!(second_state.objects.get(own).zone, Zone::Exile);
    assert_eq!(second_state.objects.get(opposing).zone, Zone::Exile);
    assert_eq!(second_state.objects.get(drawn).zone, Zone::Hand);
}
