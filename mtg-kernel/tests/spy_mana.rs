use mtg_kernel::card_def::{
    card_id_by_name, preflight_fully_supported_deck, CardCapability, CardType, Keywords,
    ManaAbilityAmountDef, ManaAbilityCostDef, Subtype, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::CommittedEvent;
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{self, ManaColor};
use mtg_kernel::rl::{legal_action_candidates_v1, ActionSemanticV1, LegalActionCandidateV1};
use mtg_kernel::state::{
    AbilityKindV4, AbilityUseV4, Counters, GameObject, GameState, ObjectStateV4, Step, Zone,
};
use mtg_kernel::surface_v2::SurfaceDecision;

fn ready_main1() -> GameState {
    let mut state = GameState::new_from_libraries(&[], &[], |_| String::new(), 53);
    state.step = Step::Main1;
    state.active_player = PlayerId::P0;
    state.priority_player = PlayerId::P0;
    state
}

fn put_object_for(
    state: &mut GameState,
    player: PlayerId,
    name: &str,
    zone: Zone,
    tapped: bool,
    summoning_sick: bool,
) -> ObjectId {
    let card_def = card_id_by_name(name).unwrap_or_else(|| panic!("{name} in CARD_DEFS"));
    let id = state.objects.push(GameObject {
        card_def,
        name: name.to_string(),
        owner: player,
        controller: player,
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
        Zone::Hand => state.players[player.index()].hand.push(id),
        Zone::Battlefield => state.players[player.index()].battlefield.push(id),
        Zone::Graveyard => state.players[player.index()].graveyard.push(id),
        Zone::Exile => state.exile.push(id),
        Zone::Library => state.players[player.index()].library.push(id),
        Zone::Command => state.command.push(id),
        Zone::Stack => panic!("test helper does not build stack items"),
    }
    id
}

fn put_object(
    state: &mut GameState,
    name: &str,
    zone: Zone,
    tapped: bool,
    summoning_sick: bool,
) -> ObjectId {
    put_object_for(state, PlayerId::P0, name, zone, tapped, summoning_sick)
}

fn mana_candidates(state: &GameState, source: ObjectId) -> Vec<LegalActionCandidateV1> {
    let mut decision_state = state.clone();
    let decision = engine::advance_until_decision(&mut decision_state);
    assert!(matches!(decision, Decision::CastSpellOrPass { .. }));
    legal_action_candidates_v1(&SurfaceDecision::Decision(decision), state)
        .expect("legal action projection")
        .into_iter()
        .filter(|candidate| {
            matches!(
                &candidate.record.semantic,
                ActionSemanticV1::ActivateManaAbility { source: card, .. }
                    if card.arena_id == source.0
            )
        })
        .collect()
}

fn resolve_permanent(state: &mut GameState, permanent: ObjectId) {
    for _ in 0..8 {
        if state.objects.get(permanent).zone == Zone::Battlefield {
            return;
        }
        match engine::advance_until_decision(state) {
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => panic!("unexpected decision while resolving permanent: {other:?}"),
        }
    }
    panic!("permanent did not resolve in bounded priority walk");
}

#[test]
fn registry_appends_exact_spy_mana_cards_and_definitions() {
    let expected = [
        ("Elves of Deep Shadow", 136_u16),
        ("Lotus Petal", 137),
        ("Overgrown Battlement", 138),
        ("Saruli Caretaker", 139),
        ("Tinder Wall", 140),
        ("Wall of Roots", 141),
    ];
    let ids: Vec<_> = expected
        .iter()
        .map(|(name, expected_id)| {
            let id = card_id_by_name(name).unwrap();
            assert_eq!(id, *expected_id, "append-only id for {name}");
            assert_eq!(CARD_DEFS[id as usize].capability, CardCapability::Full);
            id
        })
        .collect();
    preflight_fully_supported_deck(&ids).unwrap();

    let deep = &CARD_DEFS[ids[0] as usize];
    assert_eq!((deep.power, deep.toughness), (Some(1), Some(1)));
    assert_eq!(deep.subtypes, &[Subtype::Elf, Subtype::Druid]);
    assert_eq!(deep.mana_ability_choices, &[ManaColor::B]);
    let deep_mana = deep.mana_ability_def.unwrap();
    assert_eq!(deep_mana.cost, ManaAbilityCostDef::TapSelf);
    assert_eq!(deep_mana.amount, ManaAbilityAmountDef::Fixed(1));
    assert_eq!(deep_mana.controller_damage, 1);

    let lotus = &CARD_DEFS[ids[1] as usize];
    assert!(lotus.has_type(CardType::Artifact));
    assert_eq!(lotus.cost.generic, 0);
    assert_eq!(lotus.mana_ability_choices, &ManaColor::ALL[..5]);
    assert_eq!(
        lotus.mana_ability_def.unwrap().cost,
        ManaAbilityCostDef::SacrificeSelf
    );

    for id in &ids[2..] {
        assert!(CARD_DEFS[*id as usize].keywords.has(Keywords::DEFENDER));
    }
    assert_eq!(CARD_DEFS[ids[2] as usize].subtypes, &[Subtype::Wall]);
    assert_eq!(CARD_DEFS[ids[3] as usize].subtypes, &[Subtype::Dryad]);
    assert_eq!(
        CARD_DEFS[ids[4] as usize].subtypes,
        &[Subtype::Plant, Subtype::Wall]
    );
    assert_eq!(Subtype::Dryad.stable_id(), 56);
    assert_eq!(Subtype::Plant.stable_id(), 57);
    assert_eq!(Subtype::Wall.stable_id(), 58);
}

#[test]
fn deep_shadow_taps_for_black_and_deals_controller_damage() {
    let mut cast_state = ready_main1();
    let deep = put_object(
        &mut cast_state,
        "Elves of Deep Shadow",
        Zone::Hand,
        false,
        false,
    );
    cast_state.players[0].mana_pool[ManaColor::G.pool_index()] = 1;
    engine::step(&mut cast_state, Action::CastSpell(deep)).unwrap();
    resolve_permanent(&mut cast_state, deep);
    assert!(cast_state.objects.get(deep).summoning_sick);
    assert!(mana_candidates(&cast_state, deep).is_empty());

    cast_state.objects.get_mut(deep).summoning_sick = false;
    cast_state.priority_player = PlayerId::P0;
    cast_state.engine.priority_passes = [false, false];
    let before_life = cast_state.players[0].life;
    engine::step(&mut cast_state, Action::ActivateManaAbility(deep)).unwrap();
    assert!(cast_state.objects.get(deep).tapped);
    assert_eq!(
        cast_state.players[0].mana_pool[ManaColor::B.pool_index()],
        1
    );
    assert_eq!(cast_state.players[0].life, before_life - 1);
    assert_eq!(
        cast_state.objects.get(deep).v4.ability_uses_this_turn,
        [AbilityUseV4 {
            ability_kind: AbilityKindV4::Mana,
            ability_index: 0,
            uses: 1,
        }]
    );
    assert!(engine::step(&mut cast_state, Action::ActivateManaAbility(deep)).is_err());

    let mut lethal = ready_main1();
    let deep = put_object(
        &mut lethal,
        "Elves of Deep Shadow",
        Zone::Battlefield,
        false,
        false,
    );
    lethal.players[0].life = 1;
    engine::step(&mut lethal, Action::ActivateManaAbility(deep)).unwrap();
    assert!(lethal.players[0].has_lost);
    assert_eq!(lethal.players[0].mana_pool[ManaColor::B.pool_index()], 1);
}

#[test]
fn lotus_petal_sacrifices_from_tapped_state_for_any_color_except_colorless() {
    let mut state = ready_main1();
    let petal = put_object(&mut state, "Lotus Petal", Zone::Battlefield, true, true);
    assert!(mana::gather_sources(PlayerId::P0, &state)
        .iter()
        .all(|source| source.id != petal));
    let candidates = mana_candidates(&state, petal);
    assert_eq!(candidates.len(), 5);
    let choices: Vec<_> = candidates
        .iter()
        .map(|candidate| match candidate.record.semantic {
            ActionSemanticV1::ActivateManaAbility {
                mana_choice: Some(choice),
                cost_target: None,
                ..
            } => choice,
            ref other => panic!("unexpected Lotus Petal action: {other:?}"),
        })
        .collect();
    assert_eq!(choices, ManaColor::ALL[..5]);
    assert_eq!(
        candidates
            .iter()
            .map(|candidate| &candidate.record.stable_id)
            .collect::<std::collections::HashSet<_>>()
            .len(),
        5
    );

    engine::step(
        &mut state,
        Action::ActivateManaAbilityChoice(petal, ManaColor::R),
    )
    .unwrap();
    assert_eq!(state.objects.get(petal).zone, Zone::Graveyard);
    assert_eq!(state.players[0].mana_pool[ManaColor::R.pool_index()], 1);
    assert!(state
        .objects
        .get(petal)
        .v4
        .ability_uses_this_turn
        .is_empty());
    assert!(engine::step(
        &mut state,
        Action::ActivateManaAbilityChoice(petal, ManaColor::C)
    )
    .is_err());
}

#[test]
fn battlement_counts_only_controlled_defender_creatures_and_obeys_tap_rules() {
    let mut state = ready_main1();
    let battlement = put_object(
        &mut state,
        "Overgrown Battlement",
        Zone::Battlefield,
        false,
        true,
    );
    let _roots = put_object(&mut state, "Wall of Roots", Zone::Battlefield, true, true);
    let _tinder = put_object(&mut state, "Tinder Wall", Zone::Battlefield, false, true);
    let _ordinary = put_object(&mut state, "Elvish Mystic", Zone::Battlefield, false, false);
    let _opponent_wall = put_object_for(
        &mut state,
        PlayerId::P1,
        "Wall of Roots",
        Zone::Battlefield,
        false,
        false,
    );
    assert!(mana_candidates(&state, battlement).is_empty());
    state.objects.get_mut(battlement).summoning_sick = false;
    assert_eq!(mana_candidates(&state, battlement).len(), 1);
    assert!(mana::gather_sources(PlayerId::P0, &state)
        .iter()
        .all(|source| source.id != battlement));
    engine::step(&mut state, Action::ActivateManaAbility(battlement)).unwrap();
    assert!(state.objects.get(battlement).tapped);
    assert_eq!(state.players[0].mana_pool[ManaColor::G.pool_index()], 3);
}

#[test]
fn caretaker_exposes_every_color_and_exact_other_creature_cost_choice() {
    let mut state = ready_main1();
    let caretaker = put_object(
        &mut state,
        "Saruli Caretaker",
        Zone::Battlefield,
        false,
        false,
    );
    let sick_creature = put_object(&mut state, "Elvish Mystic", Zone::Battlefield, false, true);
    let other_creature = put_object(&mut state, "Tinder Wall", Zone::Battlefield, false, true);
    let tapped_creature = put_object(&mut state, "Wall of Roots", Zone::Battlefield, true, true);
    let artifact = put_object(&mut state, "Lotus Petal", Zone::Battlefield, false, true);

    let candidates = mana_candidates(&state, caretaker);
    assert_eq!(candidates.len(), 10, "5 colors times 2 legal creatures");
    let mut seen = std::collections::HashSet::new();
    for candidate in &candidates {
        let ActionSemanticV1::ActivateManaAbility {
            mana_choice: Some(color),
            cost_target: Some(cost_target),
            ..
        } = &candidate.record.semantic
        else {
            panic!("Caretaker candidate must bind color and cost creature");
        };
        assert!([sick_creature.0, other_creature.0].contains(&cost_target.arena_id));
        assert!(seen.insert((*color, cost_target.arena_id)));
        let encoded = serde_json::to_value(&candidate.record.semantic).unwrap();
        assert!(encoded.get("cost_target").is_some());
    }
    assert_eq!(seen.len(), 10);

    assert!(engine::step(
        &mut state.clone(),
        Action::ActivateManaAbilityChoice(caretaker, ManaColor::G)
    )
    .is_err());
    assert!(engine::step(
        &mut state.clone(),
        Action::ActivateManaAbilityWithCostTarget(caretaker, ManaColor::G, caretaker)
    )
    .is_err());
    assert!(engine::step(
        &mut state.clone(),
        Action::ActivateManaAbilityWithCostTarget(caretaker, ManaColor::G, tapped_creature)
    )
    .is_err());
    assert!(engine::step(
        &mut state.clone(),
        Action::ActivateManaAbilityWithCostTarget(caretaker, ManaColor::G, artifact)
    )
    .is_err());

    engine::step(
        &mut state,
        Action::ActivateManaAbilityWithCostTarget(caretaker, ManaColor::U, sick_creature),
    )
    .unwrap();
    assert!(state.objects.get(caretaker).tapped);
    assert!(state.objects.get(sick_creature).tapped);
    assert!(!state.objects.get(other_creature).tapped);
    assert_eq!(state.players[0].mana_pool[ManaColor::U.pool_index()], 1);
}

#[test]
fn wall_of_roots_ignores_tap_and_sickness_but_is_limited_once_each_turn() {
    let mut state = ready_main1();
    let wall = put_object(&mut state, "Wall of Roots", Zone::Battlefield, true, true);
    let before = state.diagnostic_state_hash();
    assert_eq!(mana_candidates(&state, wall).len(), 1);
    engine::step(&mut state, Action::ActivateManaAbility(wall)).unwrap();
    assert!(
        state.objects.get(wall).tapped,
        "ability does not untap its source"
    );
    assert_eq!(state.objects.get(wall).counters.minus0_minus1, 1);
    assert_eq!(engine::effective_toughness(&state, wall), 4);
    assert_eq!(state.players[0].mana_pool[ManaColor::G.pool_index()], 1);
    assert_ne!(state.diagnostic_state_hash(), before);
    assert!(serde_json::to_string(&state)
        .unwrap()
        .contains("\"minus0_minus1\":1"));
    assert!(mana_candidates(&state, wall).is_empty());
    assert!(engine::step(&mut state, Action::ActivateManaAbility(wall)).is_err());

    state.step = Step::Cleanup;
    state.active_player = PlayerId::P1;
    state.priority_player = PlayerId::P1;
    let _ = engine::advance_until_decision(&mut state);
    assert!(state.objects.get(wall).v4.ability_uses_this_turn.is_empty());
    state.step = Step::Main1;
    state.active_player = PlayerId::P0;
    state.priority_player = PlayerId::P0;
    assert_eq!(mana_candidates(&state, wall).len(), 1);

    state.objects.get_mut(wall).counters.minus0_minus1 = 4;
    engine::step(&mut state, Action::ActivateManaAbility(wall)).unwrap();
    assert_eq!(state.players[0].mana_pool[ManaColor::G.pool_index()], 1);
    assert_eq!(state.objects.get(wall).zone, Zone::Graveyard);
}

#[test]
fn wall_counter_and_once_per_turn_use_are_independently_hash_sensitive() {
    let mut base = ready_main1();
    let wall = put_object(&mut base, "Wall of Roots", Zone::Battlefield, false, false);
    let base_hash = base.diagnostic_state_hash();

    let mut counter_state = base.clone();
    counter_state.objects.get_mut(wall).counters.minus0_minus1 = 1;
    assert_ne!(counter_state.diagnostic_state_hash(), base_hash);

    let mut use_state = base.clone();
    use_state
        .objects
        .get_mut(wall)
        .v4
        .note_ability_use(AbilityKindV4::Mana, 0);
    assert_ne!(use_state.diagnostic_state_hash(), base_hash);
    assert_ne!(
        use_state.diagnostic_state_hash(),
        counter_state.diagnostic_state_hash()
    );
    let round_trip: GameState = serde_json::from_str(&serde_json::to_string(&use_state).unwrap())
        .expect("ability-use state round trips");
    assert_eq!(round_trip, use_state);
}

#[test]
fn defender_creatures_are_not_eligible_attackers() {
    let mut state = ready_main1();
    state.step = Step::DeclareAttackers;
    for name in [
        "Overgrown Battlement",
        "Saruli Caretaker",
        "Tinder Wall",
        "Wall of Roots",
    ] {
        put_object(&mut state, name, Zone::Battlefield, false, false);
    }
    let attacker = put_object(&mut state, "Elvish Mystic", Zone::Battlefield, false, false);
    let Decision::DeclareAttackers { eligible, .. } = engine::advance_until_decision(&mut state)
    else {
        panic!("expected attacker declaration");
    };
    assert_eq!(eligible, vec![attacker]);
}

#[test]
fn tinder_wall_damage_ability_only_targets_a_creature_it_is_blocking() {
    let mut state = ready_main1();
    state.step = Step::DeclareBlockers;
    let tinder = put_object(&mut state, "Tinder Wall", Zone::Battlefield, false, false);
    let attacker = put_object_for(
        &mut state,
        PlayerId::P1,
        "Elvish Mystic",
        Zone::Battlefield,
        true,
        false,
    );
    let bystander = put_object_for(
        &mut state,
        PlayerId::P1,
        "Elves of Deep Shadow",
        Zone::Battlefield,
        false,
        false,
    );
    state.engine.combat.attackers = vec![attacker];
    state.engine.combat.attackers_declared = true;
    state.engine.combat.blocked_by = vec![(attacker, vec![tinder])];
    state.engine.combat.blockers_declared = true;
    state.players[0].mana_pool[ManaColor::R.pool_index()] = 1;

    let Decision::CastSpellOrPass {
        activatable_abilities,
        ..
    } = engine::advance_until_decision(&mut state)
    else {
        panic!("expected combat priority");
    };
    assert!(activatable_abilities.contains(&(tinder, 0)));
    engine::step(&mut state, Action::ActivateAbility(tinder, 0)).unwrap();
    let Decision::ChooseTargets { legal_targets, .. } = engine::advance_until_decision(&mut state)
    else {
        panic!("expected Tinder Wall target choice");
    };
    assert_eq!(
        legal_targets,
        vec![mtg_kernel::state::Target::Object(attacker)]
    );
    assert!(engine::step(
        &mut state,
        Action::ChooseTarget(mtg_kernel::state::Target::Object(bystander))
    )
    .is_err());
    engine::step(
        &mut state,
        Action::ChooseTarget(mtg_kernel::state::Target::Object(attacker)),
    )
    .unwrap();
    let _ = engine::advance_until_decision(&mut state);
    assert_eq!(state.objects.get(tinder).zone, Zone::Graveyard);
    assert_eq!(state.players[0].mana_pool[ManaColor::R.pool_index()], 0);
    assert_eq!(state.stack.len(), 1);
    for _ in 0..4 {
        if state.stack.is_empty() {
            break;
        }
        match engine::advance_until_decision(&mut state) {
            Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
            other => panic!("unexpected decision resolving Tinder Wall: {other:?}"),
        }
    }
    assert!(state.stack.is_empty());
    assert_eq!(state.objects.get(attacker).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(attacker).damage, 0);
    assert!(state.engine.event_history.iter().any(|event| matches!(
        event,
        CommittedEvent::Damage {
            source,
            target: mtg_kernel::state::Target::Object(object),
            amount: 2,
        } if *source == tinder && *object == attacker
    )));
    assert_eq!(state.objects.get(bystander).damage, 0);

    let mut not_blocking = ready_main1();
    let tinder = put_object(
        &mut not_blocking,
        "Tinder Wall",
        Zone::Battlefield,
        false,
        false,
    );
    not_blocking.players[0].mana_pool[ManaColor::R.pool_index()] = 1;
    let Decision::CastSpellOrPass {
        activatable_abilities,
        ..
    } = engine::advance_until_decision(&mut not_blocking)
    else {
        panic!("expected priority decision");
    };
    assert!(!activatable_abilities.contains(&(tinder, 0)));
}

#[test]
fn tinder_wall_rechecks_its_combat_target_restriction_on_resolution() {
    let mut state = ready_main1();
    state.step = Step::DeclareBlockers;
    let tinder = put_object(&mut state, "Tinder Wall", Zone::Battlefield, false, false);
    let attacker = put_object_for(
        &mut state,
        PlayerId::P1,
        "Elvish Mystic",
        Zone::Battlefield,
        true,
        false,
    );
    state.engine.combat.attackers = vec![attacker];
    state.engine.combat.attackers_declared = true;
    state.engine.combat.blocked_by = vec![(attacker, vec![tinder])];
    state.engine.combat.blockers_declared = true;
    state.players[0].mana_pool[ManaColor::R.pool_index()] = 1;

    let _ = engine::advance_until_decision(&mut state);
    engine::step(&mut state, Action::ActivateAbility(tinder, 0)).unwrap();
    let _ = engine::advance_until_decision(&mut state);
    engine::step(
        &mut state,
        Action::ChooseTarget(mtg_kernel::state::Target::Object(attacker)),
    )
    .unwrap();
    let _ = engine::advance_until_decision(&mut state);
    assert_eq!(state.stack.len(), 1);

    // Model an effect removing the attacker from combat while the ability is
    // on the stack. The target is still the same battlefield incarnation, but
    // no longer satisfies Tinder Wall's printed target restriction.
    state.engine.combat.blocked_by.clear();
    for _ in 0..4 {
        if state.stack.is_empty() {
            break;
        }
        match engine::advance_until_decision(&mut state) {
            Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
            other => panic!("unexpected decision resolving Tinder Wall: {other:?}"),
        }
    }
    assert!(state.stack.is_empty());
    assert_eq!(state.objects.get(attacker).damage, 0);
}

#[test]
fn tinder_wall_mana_ability_sacrifices_for_exactly_two_red() {
    let mut state = ready_main1();
    let tinder = put_object(&mut state, "Tinder Wall", Zone::Battlefield, true, true);
    assert_eq!(mana_candidates(&state, tinder).len(), 1);
    engine::step(&mut state, Action::ActivateManaAbility(tinder)).unwrap();
    assert_eq!(state.objects.get(tinder).zone, Zone::Graveyard);
    assert_eq!(state.players[0].mana_pool[ManaColor::R.pool_index()], 2);
}
