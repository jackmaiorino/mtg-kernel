//! Focused coverage for the Affinity and Wildfire value-card tranche.
//!
//! The checked-in Mage implementations are authoritative: all three draw
//! spells sacrifice an artifact or creature in addition to paying {1}{B};
//! Reckoner's Bargain gains the sacrificed permanent's printed mana value;
//! Eviscerator's Insight has flashback {4}{B}; Fanatical Offering creates a
//! Map; and Blood Fountain creates a Blood before recurring up to two owned
//! creature cards.

use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardType, CostComponent, PermanentFilter, Subtype, TargetSpec,
    CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, CostKind, Decision};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{Cost, ManaColor, Pip};
use mtg_kernel::rl::{legal_action_candidates_v1, ActionSemanticV1};
use mtg_kernel::state::{
    CastMethodV4, Counters, GameObject, GameState, ObjectStateV4, StackItemKind, Step, Target, Zone,
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

fn add_black(state: &mut GameState, amount: u8) {
    state.players[PlayerId::P0.index()].mana_pool[ManaColor::B.pool_index()] += amount;
}

fn pass_until_idle(state: &mut GameState) {
    for _ in 0..96 {
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

fn choose_sacrifice(state: &mut GameState, spell: ObjectId, wanted: ObjectId) -> Vec<ObjectId> {
    let decision = engine::advance_until_decision(state);
    let candidates = match decision {
        Decision::ChooseCostTargets {
            player,
            source,
            cost_kind,
            remaining,
            candidates,
        } => {
            assert_eq!(player, PlayerId::P0);
            assert_eq!(source, spell);
            assert_eq!(cost_kind, CostKind::SacrificePermanents);
            assert_eq!(remaining, 1);
            candidates
        }
        other => panic!("expected permanent-sacrifice choice, got {other:?}"),
    };
    assert!(candidates.contains(&wanted));
    engine::step(state, Action::ChooseCostTarget(wanted)).unwrap();
    let after = engine::advance_until_decision(state);
    assert!(matches!(after, Decision::CastSpellOrPass { .. }));
    candidates
}

#[test]
fn registry_and_generated_definitions_match_current_mage_cards() {
    assert_eq!(card_id("Blood Fountain"), 6);
    assert_eq!(card_id("Eviscerator's Insight"), 29);
    assert_eq!(card_id("Fanatical Offering"), 35);
    assert_eq!(card_id("Reckoner's Bargain"), 94);
    assert_eq!(card_id("Map Token"), 147);
    assert_eq!(CARD_DEFS.len(), 160);
    assert_eq!(Subtype::Map.stable_id(), 61);

    let sacrifice = [CostComponent::SacrificeControlled {
        count: 1,
        filter: PermanentFilter::ArtifactOrCreature,
    }];
    for name in [
        "Eviscerator's Insight",
        "Fanatical Offering",
        "Reckoner's Bargain",
    ] {
        let def = &CARD_DEFS[card_id(name) as usize];
        assert_eq!(def.capability, CardCapability::Full);
        assert_eq!(def.mana_value, 2);
        assert_eq!(def.types, &[CardType::Instant]);
        assert_eq!(
            def.cost,
            Cost {
                pips: &[Pip::Colored(ManaColor::B)],
                generic: 1,
                x_count: 0,
            }
        );
        assert_eq!(def.additional_cost, Some(sacrifice.as_slice()));
    }

    let insight = &CARD_DEFS[card_id("Eviscerator's Insight") as usize];
    assert_eq!(
        insight.flashback.as_ref().expect("flashback").cost,
        &[CostComponent::Mana(Cost {
            pips: &[Pip::Colored(ManaColor::B)],
            generic: 4,
            x_count: 0,
        })]
    );

    let fountain = &CARD_DEFS[card_id("Blood Fountain") as usize];
    assert_eq!(fountain.capability, CardCapability::Full);
    assert_eq!(fountain.mana_value, 1);
    assert_eq!(fountain.activated_abilities.len(), 1);
    assert_eq!(
        fountain.activated_abilities[0].target_spec,
        TargetSpec::UpToTwoCreatureCardsInOwnGraveyard
    );
    assert_eq!(
        fountain.activated_abilities[0].cost,
        &[
            CostComponent::Mana(Cost {
                pips: &[Pip::Colored(ManaColor::B)],
                generic: 3,
                x_count: 0,
            }),
            CostComponent::Tap,
            CostComponent::SacrificeSelf,
        ]
    );

    let map = &CARD_DEFS[card_id("Map Token") as usize];
    assert!(map.is_token);
    assert_eq!(map.subtypes, &[Subtype::Map]);
    assert_eq!(map.activated_abilities.len(), 1);
    assert_eq!(
        map.activated_abilities[0].target_spec,
        TargetSpec::ControlledCreature
    );
    assert!(map.activated_abilities[0].sorcery_speed_only);
}

#[test]
fn fanatical_offering_filters_payment_and_creates_a_map_after_drawing_two() {
    let mut state = ready_main(0x4641_4e41_5449_4341);
    put_owned(&mut state, "Mountain", Zone::Library);
    put_owned(&mut state, "Swamp", Zone::Library);
    let offering = put_owned(&mut state, "Fanatical Offering", Zone::Hand);
    let creature = put_owned(&mut state, "Voldaren Epicure", Zone::Battlefield);
    let artifact_land = put_owned(&mut state, "Great Furnace", Zone::Battlefield);
    let plain_land = put_owned(&mut state, "Mountain", Zone::Battlefield);
    let opponent_artifact = put_object(
        &mut state,
        PlayerId::P1,
        PlayerId::P1,
        "Blood Token",
        Zone::Battlefield,
    );
    add_black(&mut state, 2);

    engine::step(&mut state, Action::CastSpell(offering)).unwrap();
    let candidates = choose_sacrifice(&mut state, offering, artifact_land);
    assert_eq!(candidates, vec![creature, artifact_land]);
    assert!(!candidates.contains(&plain_land));
    assert!(!candidates.contains(&opponent_artifact));
    assert_eq!(state.stack.last().unwrap().v4.paid_cost_refs.len(), 1);
    assert_eq!(
        state.stack.last().unwrap().v4.paid_cost_refs[0].object,
        artifact_land
    );

    pass_until_idle(&mut state);
    assert_eq!(state.players[0].library.len(), 0);
    assert_eq!(state.players[0].hand.len(), 2);
    assert_eq!(state.objects.get(offering).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(artifact_land).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(plain_land).zone, Zone::Battlefield);
    assert_eq!(state.objects.get(opponent_artifact).zone, Zone::Battlefield);
    assert!(state.players[0]
        .battlefield
        .iter()
        .any(|&object| { state.objects.get(object).card_def == card_id("Map Token") }));

    let mut no_payment = ready_main(0x4641_4e41_5449_4342);
    let unpayable = put_owned(&mut no_payment, "Fanatical Offering", Zone::Hand);
    add_black(&mut no_payment, 2);
    let before = no_payment.clone();
    assert!(engine::step(&mut no_payment, Action::CastSpell(unpayable)).is_err());
    assert_eq!(no_payment, before);
}

#[test]
fn reckoners_bargain_uses_printed_mana_value_and_not_mana_paid() {
    let mut state = ready_main(0x4241_5247_4149_4e31);
    put_owned(&mut state, "Mountain", Zone::Library);
    put_owned(&mut state, "Swamp", Zone::Library);
    let bargain = put_owned(&mut state, "Reckoner's Bargain", Zone::Hand);
    let enforcer = put_owned(&mut state, "Myr Enforcer", Zone::Battlefield);
    put_owned(&mut state, "Great Furnace", Zone::Battlefield);
    state.players[0].life = 11;
    add_black(&mut state, 2);

    engine::step(&mut state, Action::CastSpell(bargain)).unwrap();
    choose_sacrifice(&mut state, bargain, enforcer);
    let paid = state.stack.last().unwrap().v4.paid_cost_refs[0];
    assert_eq!(paid.card_def, card_id("Myr Enforcer"));
    assert_eq!(CARD_DEFS[paid.card_def as usize].mana_value, 7);
    pass_until_idle(&mut state);

    assert_eq!(state.players[0].life, 18);
    assert_eq!(state.players[0].library.len(), 0);
    assert_eq!(state.players[0].hand.len(), 2);
}

#[test]
fn eviscerators_insight_casts_normally_then_flashbacks_with_the_same_cost_filter() {
    let mut state = ready_main(0x4556_4953_4345_5241);
    for name in ["Mountain", "Swamp", "Forest", "Island"] {
        put_owned(&mut state, name, Zone::Library);
    }
    let insight = put_owned(&mut state, "Eviscerator's Insight", Zone::Hand);
    put_owned(&mut state, "Voldaren Epicure", Zone::Battlefield);
    add_black(&mut state, 2);

    engine::step(&mut state, Action::CastSpell(insight)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    assert_eq!(
        state.stack.last().unwrap().v4.cast_method,
        Some(CastMethodV4::Normal)
    );
    pass_until_idle(&mut state);
    assert_eq!(state.objects.get(insight).zone, Zone::Graveyard);
    assert_eq!(state.players[0].hand.len(), 2);

    put_owned(&mut state, "Great Furnace", Zone::Battlefield);
    add_black(&mut state, 5);
    engine::step(&mut state, Action::CastSpell(insight)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    let item = state.stack.last().unwrap();
    assert_eq!(item.kind, StackItemKind::Spell);
    assert!(item.is_flashback);
    assert_eq!(item.v4.cast_method, Some(CastMethodV4::Flashback));
    assert_eq!(item.v4.paid_cost_refs.len(), 1);
    pass_until_idle(&mut state);

    assert_eq!(state.objects.get(insight).zone, Zone::Exile);
    assert_eq!(state.players[0].library.len(), 0);
    assert_eq!(state.players[0].hand.len(), 4);
}

#[test]
fn blood_fountain_optional_targets_are_rl_and_snapshot_stable() {
    let mut state = ready_main(0x424c_4f4f_4446_4f55);
    let fountain = put_owned(&mut state, "Blood Fountain", Zone::Hand);
    add_black(&mut state, 1);
    engine::step(&mut state, Action::CastSpell(fountain)).unwrap();
    pass_until_idle(&mut state);
    assert_eq!(state.objects.get(fountain).zone, Zone::Battlefield);
    assert_eq!(
        state.players[0]
            .battlefield
            .iter()
            .filter(|&&object| state.objects.get(object).card_def == card_id("Blood Token"))
            .count(),
        1
    );

    let first = put_owned(&mut state, "Voldaren Epicure", Zone::Graveyard);
    let second = put_owned(&mut state, "Myr Enforcer", Zone::Graveyard);
    let unchosen = put_owned(&mut state, "Sneaky Snacker", Zone::Graveyard);
    let noncreature = put_owned(&mut state, "Mountain", Zone::Graveyard);
    let opponent_creature = put_object(
        &mut state,
        PlayerId::P1,
        PlayerId::P1,
        "Voldaren Epicure",
        Zone::Graveyard,
    );
    add_black(&mut state, 4);
    engine::step(&mut state, Action::ActivateAbility(fountain, 0)).unwrap();

    let decision = engine::advance_until_decision(&mut state);
    let legal_targets = match &decision {
        Decision::ChooseEffectTargets {
            selected_count,
            min_targets,
            max_targets,
            legal_targets,
            can_finish,
            ..
        } => {
            assert_eq!((*selected_count, *min_targets, *max_targets), (0, 0, 2));
            assert!(*can_finish);
            legal_targets.clone()
        }
        other => panic!("expected optional graveyard targets, got {other:?}"),
    };
    assert_eq!(
        legal_targets,
        vec![
            Target::Object(first),
            Target::Object(second),
            Target::Object(unchosen)
        ]
    );
    assert!(!legal_targets.contains(&Target::Object(noncreature)));
    assert!(!legal_targets.contains(&Target::Object(opponent_creature)));

    let actions =
        legal_action_candidates_v1(&SurfaceDecision::Decision(decision.clone()), &state).unwrap();
    assert_eq!(actions.len(), 4);
    assert!(matches!(
        actions.last().unwrap().record.semantic,
        ActionSemanticV1::FinishEffectSelection {
            selected_count: 0,
            ..
        }
    ));

    let bytes = serde_json::to_vec(&state).unwrap();
    let mut restored: GameState = serde_json::from_slice(&bytes).unwrap();
    let restored_decision = engine::advance_until_decision(&mut restored);
    assert_eq!(decision, restored_decision);
    assert_eq!(state, restored);
    assert_eq!(
        state.diagnostic_state_hash(),
        restored.diagnostic_state_hash()
    );
    let restored_ids =
        legal_action_candidates_v1(&SurfaceDecision::Decision(restored_decision), &restored)
            .unwrap()
            .into_iter()
            .map(|candidate| candidate.record.stable_id)
            .collect::<Vec<_>>();
    assert_eq!(
        actions
            .iter()
            .map(|candidate| candidate.record.stable_id.clone())
            .collect::<Vec<_>>(),
        restored_ids
    );

    let mut zero_targets = state.clone();
    engine::step(&mut zero_targets, Action::FinishEffectSelection).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut zero_targets),
        Decision::CastSpellOrPass { .. }
    ));
    assert!(zero_targets.stack.last().unwrap().targets.is_empty());
    pass_until_idle(&mut zero_targets);
    assert_eq!(zero_targets.objects.get(fountain).zone, Zone::Graveyard);
    assert_eq!(zero_targets.objects.get(first).zone, Zone::Graveyard);

    engine::step(
        &mut state,
        Action::ChooseEffectTarget(Target::Object(first)),
    )
    .unwrap();
    let second_decision = engine::advance_until_decision(&mut state);
    assert!(matches!(
        second_decision,
        Decision::ChooseEffectTargets {
            selected_count: 1,
            can_finish: true,
            ..
        }
    ));
    engine::step(
        &mut state,
        Action::ChooseEffectTarget(Target::Object(second)),
    )
    .unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    let ability = state.stack.last().unwrap();
    assert_eq!(ability.kind, StackItemKind::ActivatedAbility);
    assert_eq!(
        ability.targets,
        vec![Target::Object(first), Target::Object(second)]
    );
    assert_eq!(ability.v4.paid_cost_refs.len(), 1);
    assert_eq!(ability.v4.paid_cost_refs[0].object, fountain);
    pass_until_idle(&mut state);

    assert_eq!(state.objects.get(fountain).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(first).zone, Zone::Hand);
    assert_eq!(state.objects.get(second).zone, Zone::Hand);
    assert_eq!(state.objects.get(unchosen).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(noncreature).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(opponent_creature).zone, Zone::Graveyard);
}

#[test]
fn map_explore_handles_land_nonland_and_empty_libraries() {
    let mut nonland = ready_main(0x4d41_5000_0000_0001);
    let map = put_owned(&mut nonland, "Map Token", Zone::Battlefield);
    let creature = put_owned(&mut nonland, "Voldaren Epicure", Zone::Battlefield);
    let revealed = put_owned(&mut nonland, "Fanatical Offering", Zone::Library);
    add_black(&mut nonland, 1);
    engine::step(&mut nonland, Action::ActivateAbility(map, 0)).unwrap();
    let target_decision = engine::advance_until_decision(&mut nonland);
    assert!(matches!(
        target_decision,
        Decision::ChooseTargets {
            legal_targets,
            ..
        } if legal_targets == vec![Target::Object(creature)]
    ));
    engine::step(&mut nonland, Action::ChooseTarget(Target::Object(creature))).unwrap();
    let choice = loop {
        let decision = engine::advance_until_decision(&mut nonland);
        match decision {
            Decision::CastSpellOrPass { .. } => engine::step(&mut nonland, Action::Pass).unwrap(),
            Decision::ChooseEffectOption {
                option_count: 2, ..
            } => break decision,
            other => panic!("unexpected explore decision: {other:?}"),
        }
    };
    assert!(matches!(choice, Decision::ChooseEffectOption { .. }));
    assert_eq!(nonland.objects.get(creature).counters.plus1_plus1, 1);
    engine::step(&mut nonland, Action::ChooseEffectOption(1)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut nonland),
        Decision::CastSpellOrPass { .. }
    ));
    assert_eq!(nonland.objects.get(revealed).zone, Zone::Graveyard);

    let mut land = ready_main(0x4d41_5000_0000_0002);
    let map = put_owned(&mut land, "Map Token", Zone::Battlefield);
    let creature = put_owned(&mut land, "Voldaren Epicure", Zone::Battlefield);
    let revealed = put_owned(&mut land, "Forest", Zone::Library);
    add_black(&mut land, 1);
    engine::step(&mut land, Action::ActivateAbility(map, 0)).unwrap();
    engine::advance_until_decision(&mut land);
    engine::step(&mut land, Action::ChooseTarget(Target::Object(creature))).unwrap();
    pass_until_idle(&mut land);
    assert_eq!(land.objects.get(revealed).zone, Zone::Hand);
    assert_eq!(land.objects.get(creature).counters.plus1_plus1, 0);

    let mut empty = ready_main(0x4d41_5000_0000_0003);
    let map = put_owned(&mut empty, "Map Token", Zone::Battlefield);
    let creature = put_owned(&mut empty, "Voldaren Epicure", Zone::Battlefield);
    add_black(&mut empty, 1);
    engine::step(&mut empty, Action::ActivateAbility(map, 0)).unwrap();
    engine::advance_until_decision(&mut empty);
    engine::step(&mut empty, Action::ChooseTarget(Target::Object(creature))).unwrap();
    pass_until_idle(&mut empty);
    assert_eq!(empty.objects.get(creature).counters.plus1_plus1, 0);
}
