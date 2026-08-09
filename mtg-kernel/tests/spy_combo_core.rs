//! Focused coverage for the Spy combo core.
//!
//! The bounded rules authority is checked-in Mage commit
//! `a5c90fe180021e70e2a644ade00eeab07f857a40`: `BalustradeSpy.java`,
//! `DreadReturn.java`, `LandGrant.java`, and `LotlethGiant.java`.

use mtg_kernel::card_def::{
    card_id_by_name, preflight_fully_supported_deck, CardCapability, CardType, CostComponent,
    DynamicValueDef, Keywords, PermanentFilter, Subtype, TargetSpec, CARD_DEFS,
};
use mtg_kernel::effect::{
    EffectOp, EffectTargetSelectionPurpose, LibraryCardFilter, ObjectRef, PendingEffectChoice,
    PlayerRef, TargetRef,
};
use mtg_kernel::engine::{self, Action, CostKind, Decision, UnsupportedMechanic};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::rl::{
    legal_action_candidates_v1, observe_v2, ActionSemanticV1, PendingEffectChoiceSemanticV4,
    PlayerSeatV1, TargetRefV1, TargetSelectionPurposeV4,
};
use mtg_kernel::state::{Counters, GameObject, GameState, Step, Target, Zone};
use mtg_kernel::surface_v2::{HarnessSurfaceV2, SurfaceDecision};

fn card_id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name} in CARD_DEFS"))
}

fn card_name(card_def: u16) -> String {
    CARD_DEFS[card_def as usize].name.to_string()
}

fn ready_main(p0_library: &[&str], p1_library: &[&str]) -> GameState {
    let p0 = p0_library
        .iter()
        .map(|name| card_id(name))
        .collect::<Vec<_>>();
    let p1 = p1_library
        .iter()
        .map(|name| card_id(name))
        .collect::<Vec<_>>();
    let mut state = GameState::new_from_libraries(&p0, &p1, card_name, 0x5350_595f_434f_5245);
    state.step = Step::Main1;
    state.active_player = PlayerId::P0;
    state.priority_player = PlayerId::P0;
    state
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
        Zone::Stack => panic!("test helper does not construct stack items"),
    }
    id
}

fn add_mana(state: &mut GameState, color: ManaColor, amount: u8) {
    state.players[0].mana_pool[color.pool_index()] += amount;
}

fn pass_until_source_finishes(state: &mut GameState, source: ObjectId) -> Decision {
    for _ in 0..24 {
        let decision = engine::advance_until_decision(state);
        if !state.stack.iter().any(|item| item.source == source)
            && state.engine.pending_effect.is_none()
        {
            return decision;
        }
        match decision {
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            Decision::GameOver { .. } | Decision::Halted { .. } => return decision,
            other => panic!("unexpected decision while resolving {source}: {other:?}"),
        }
    }
    panic!("source {source} did not finish in the bounded priority walk")
}

fn advance_to_targeted_etb(state: &mut GameState, source: ObjectId) -> Decision {
    for _ in 0..16 {
        let decision = engine::advance_until_decision(state);
        match decision {
            Decision::ChooseTargets { spell, .. } if spell == source => return decision,
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => panic!("unexpected decision before targeted ETB: {other:?}"),
        }
    }
    panic!("targeted ETB was not offered")
}

fn advance_to_effect_targets(state: &mut GameState, source: ObjectId) -> Decision {
    for _ in 0..24 {
        let decision = engine::advance_until_decision(state);
        match decision {
            Decision::ChooseEffectTargets {
                source: decision_source,
                ..
            } if decision_source == source => return decision,
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => panic!("unexpected decision before effect target choice: {other:?}"),
        }
    }
    panic!("effect target choice was not offered")
}

#[test]
fn registry_appends_exact_spy_combo_core_definitions() {
    let expected = [
        ("Balustrade Spy", 150_u16),
        ("Dread Return", 151),
        ("Land Grant", 152),
        ("Lotleth Giant", 153),
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

    let spy = &CARD_DEFS[ids[0] as usize];
    assert_eq!((spy.power, spy.toughness), (Some(2), Some(3)));
    assert_eq!(spy.subtypes, &[Subtype::Vampire, Subtype::Rogue]);
    assert!(spy.keywords.has(Keywords::FLYING));

    let dread = &CARD_DEFS[ids[1] as usize];
    assert_eq!(dread.target_spec, TargetSpec::CreatureCardInOwnGraveyard);
    assert_eq!(
        (dread.spell_effect)(),
        Some(EffectOp::MoveObject {
            object: ObjectRef::Target(0),
            to_zone: Zone::Battlefield,
        })
    );
    assert_eq!(
        dread.flashback.as_ref().unwrap().cost,
        &[CostComponent::SacrificeControlled {
            count: 3,
            filter: PermanentFilter::Creature,
        }]
    );

    let grant = &CARD_DEFS[ids[2] as usize];
    assert_eq!(
        grant.alt_cost,
        Some(&[CostComponent::RevealHandIfNoCardsWithType(CardType::Land)][..])
    );
    assert_eq!(
        (grant.spell_effect)(),
        Some(EffectOp::SearchLibraryToHand {
            player: PlayerRef::Controller,
            filter: LibraryCardFilter::LandWithSubtype(Subtype::Forest),
        })
    );

    let giant = &CARD_DEFS[ids[3] as usize];
    assert_eq!((giant.power, giant.toughness), (Some(6), Some(5)));
    assert_eq!(giant.subtypes, &[Subtype::Zombie, Subtype::Giant]);
    assert_eq!(TargetSpec::CreatureCardInOwnGraveyard as u8, 26);
    assert_eq!(TargetSpec::TargetOpponent as u8, 27);
    assert_eq!(
        Subtype::Giant.stable_id(),
        Subtype::Treasure.stable_id() + 1
    );
    assert_eq!(
        (mtg_kernel::trigger::triggers_for(ids[3])[0].effect)(),
        EffectOp::DealDamageDynamic {
            target: TargetRef::Target(0),
            amount: DynamicValueDef::ControllerGraveyardCardsWithType(CardType::Creature),
        }
    );
}

#[test]
fn land_grant_alternative_reveals_only_a_landless_hand_and_searches_a_forest() {
    let mut state = ready_main(&["Forest", "Gingerbread Cabin", "Swamp", "Fireblast"], &[]);
    let grant = put_object(&mut state, PlayerId::P0, "Land Grant", Zone::Hand);
    let bolt = put_object(&mut state, PlayerId::P0, "Lightning Bolt", Zone::Hand);
    assert!(state
        .known_hand_cards(PlayerId::P1, PlayerId::P0)
        .is_empty());

    engine::step(&mut state, Action::CastSpell(grant)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    assert!(state
        .known_hand_cards(PlayerId::P1, PlayerId::P0)
        .iter()
        .any(|entry| entry.object == bolt));
    assert_eq!(state.objects.get(grant).zone, Zone::Stack);

    let search = advance_to_effect_targets(&mut state, grant);
    let Decision::ChooseEffectTargets { legal_targets, .. } = search else {
        unreachable!()
    };
    let offered = legal_targets
        .iter()
        .map(|target| match target {
            Target::Object(object) => state.objects.get(*object).name.as_str(),
            _ => panic!("Forest search exposed a non-card target"),
        })
        .collect::<Vec<_>>();
    assert_eq!(offered, vec!["Forest", "Gingerbread Cabin"]);
    let selected = legal_targets[1];
    engine::step(&mut state, Action::ChooseEffectTarget(selected)).unwrap();
    let _ = pass_until_source_finishes(&mut state, grant);
    let Target::Object(selected) = selected else {
        unreachable!()
    };
    assert!(state.players[0].hand.contains(&selected));
    assert_eq!(state.objects.get(grant).zone, Zone::Graveyard);
    assert!(state
        .known_hand_cards(PlayerId::P1, PlayerId::P0)
        .iter()
        .any(|entry| entry.object == selected));

    let mut land_in_hand = ready_main(&["Forest"], &[]);
    let grant = put_object(&mut land_in_hand, PlayerId::P0, "Land Grant", Zone::Hand);
    put_object(&mut land_in_hand, PlayerId::P0, "Forest", Zone::Hand);
    let before = land_in_hand.clone();
    assert!(engine::step(&mut land_in_hand, Action::CastSpell(grant)).is_err());
    assert_eq!(land_in_hand, before, "failed alternative cast is atomic");
}

#[test]
fn land_grant_normal_payment_does_not_reveal_the_rest_of_the_hand() {
    let mut state = ready_main(&["Forest", "Swamp"], &[]);
    let grant = put_object(&mut state, PlayerId::P0, "Land Grant", Zone::Hand);
    let bolt = put_object(&mut state, PlayerId::P0, "Lightning Bolt", Zone::Hand);
    put_object(&mut state, PlayerId::P0, "Forest", Zone::Hand);
    add_mana(&mut state, ManaColor::G, 2);
    engine::step(&mut state, Action::CastSpell(grant)).unwrap();
    assert!(state
        .known_hand_cards(PlayerId::P1, PlayerId::P0)
        .iter()
        .all(|entry| entry.object != bolt));
}

#[test]
fn dread_return_normal_and_flashback_use_exact_graveyard_and_creature_costs() {
    let mut normal = ready_main(&[], &[]);
    let dread = put_object(&mut normal, PlayerId::P0, "Dread Return", Zone::Hand);
    let own_creature = put_object(&mut normal, PlayerId::P0, "Elvish Mystic", Zone::Graveyard);
    let _own_noncreature = put_object(&mut normal, PlayerId::P0, "Lightning Bolt", Zone::Graveyard);
    let opponent_creature = put_object(&mut normal, PlayerId::P1, "Elvish Mystic", Zone::Graveyard);
    add_mana(&mut normal, ManaColor::B, 4);
    engine::step(&mut normal, Action::CastSpell(dread)).unwrap();
    let Decision::ChooseTargets { legal_targets, .. } = engine::advance_until_decision(&mut normal)
    else {
        panic!("Dread Return target choice")
    };
    assert_eq!(legal_targets, vec![Target::Object(own_creature)]);
    let before = normal.clone();
    assert!(engine::step(
        &mut normal,
        Action::ChooseTarget(Target::Object(opponent_creature))
    )
    .is_err());
    assert_eq!(normal, before);
    engine::step(
        &mut normal,
        Action::ChooseTarget(Target::Object(own_creature)),
    )
    .unwrap();
    let _ = pass_until_source_finishes(&mut normal, dread);
    assert_eq!(normal.objects.get(own_creature).zone, Zone::Battlefield);
    assert_eq!(normal.objects.get(dread).zone, Zone::Graveyard);

    let mut flashback = ready_main(&[], &[]);
    let dread = put_object(
        &mut flashback,
        PlayerId::P0,
        "Dread Return",
        Zone::Graveyard,
    );
    let target = put_object(
        &mut flashback,
        PlayerId::P0,
        "Lotleth Giant",
        Zone::Graveyard,
    );
    let creatures = (0..4)
        .map(|_| {
            put_object(
                &mut flashback,
                PlayerId::P0,
                "Elvish Mystic",
                Zone::Battlefield,
            )
        })
        .collect::<Vec<_>>();
    let artifact = put_object(
        &mut flashback,
        PlayerId::P0,
        "Lotus Petal",
        Zone::Battlefield,
    );
    engine::step(&mut flashback, Action::CastSpell(dread)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut flashback),
        Decision::ChooseTargets { ref legal_targets, .. }
            if legal_targets == &vec![Target::Object(target)]
    ));
    engine::step(&mut flashback, Action::ChooseTarget(Target::Object(target))).unwrap();
    for &chosen in &creatures[..3] {
        let Decision::ChooseCostTargets {
            cost_kind,
            candidates,
            ..
        } = engine::advance_until_decision(&mut flashback)
        else {
            panic!("Dread Return flashback creature sacrifice")
        };
        assert_eq!(cost_kind, CostKind::SacrificeCreatures);
        assert!(candidates.contains(&chosen));
        assert!(!candidates.contains(&artifact));
        engine::step(&mut flashback, Action::ChooseCostTarget(chosen)).unwrap();
    }
    assert!(matches!(
        engine::advance_until_decision(&mut flashback),
        Decision::CastSpellOrPass { .. }
    ));
    assert!(creatures[..3]
        .iter()
        .all(|id| flashback.objects.get(*id).zone == Zone::Graveyard));
    assert_eq!(flashback.objects.get(creatures[3]).zone, Zone::Battlefield);
    assert_eq!(flashback.objects.get(artifact).zone, Zone::Battlefield);
    let _ = pass_until_source_finishes(&mut flashback, dread);
    assert_eq!(flashback.objects.get(target).zone, Zone::Battlefield);
    assert_eq!(flashback.objects.get(dread).zone, Zone::Exile);
}

#[test]
fn balustrade_spy_targets_then_publicly_reveals_and_mills_through_land_inclusive() {
    let mut state = ready_main(
        &[],
        &["Lightning Bolt", "Lotleth Giant", "Forest", "Fireblast"],
    );
    let library = state.players[1].library.clone();
    let spy = put_object(&mut state, PlayerId::P0, "Balustrade Spy", Zone::Hand);
    add_mana(&mut state, ManaColor::B, 4);
    engine::step(&mut state, Action::CastSpell(spy)).unwrap();
    let target = advance_to_targeted_etb(&mut state, spy);
    let Decision::ChooseTargets { legal_targets, .. } = &target else {
        unreachable!()
    };
    assert_eq!(
        legal_targets,
        &vec![Target::Player(PlayerId::P0), Target::Player(PlayerId::P1)]
    );

    let candidates =
        legal_action_candidates_v1(&SurfaceDecision::Decision(target), &state).unwrap();
    assert_eq!(candidates.len(), 2);
    assert!(candidates.iter().any(|candidate| matches!(
        candidate.record.semantic,
        ActionSemanticV1::ChooseTarget {
            target: TargetRefV1::Player {
                player: PlayerSeatV1::P1
            },
            ..
        }
    )));
    let observation = observe_v2(&state, &HarnessSurfaceV2::new(), PlayerId::P0, 0).unwrap();
    let pending = &observation.projection.engine_context.pending_triggers[0];
    assert_eq!(pending.target_spec, TargetSpec::AnyPlayer);
    assert!(pending.placement_ordered);
    assert!(pending.targets.is_empty());

    engine::step(
        &mut state,
        Action::ChooseTarget(Target::Player(PlayerId::P1)),
    )
    .unwrap();
    let order = advance_to_effect_targets(&mut state, spy);
    let Decision::ChooseEffectTargets {
        player,
        legal_targets,
        min_targets,
        max_targets,
        ..
    } = &order
    else {
        unreachable!()
    };
    assert_eq!(*player, PlayerId::P1);
    assert_eq!((*min_targets, *max_targets), (3, 3));
    assert_eq!(
        legal_targets,
        &library[..3]
            .iter()
            .copied()
            .map(Target::Object)
            .collect::<Vec<_>>()
    );
    for observer in [PlayerId::P0, PlayerId::P1] {
        let view = observe_v2(&state, &HarnessSurfaceV2::new(), observer, 1).unwrap();
        let json = serde_json::to_string(&view).unwrap();
        for name in ["Lightning Bolt", "Lotleth Giant", "Forest"] {
            assert!(json.contains(name), "{observer:?} sees revealed {name}");
        }
        assert!(
            !json.contains("Fireblast"),
            "unrevealed library tail stays private"
        );
        assert!(matches!(
            view.projection
                .engine_context
                .pending_effect
                .as_ref()
                .unwrap()
                .choice
                .as_ref()
                .unwrap(),
            PendingEffectChoiceSemanticV4::Targets {
                purpose: TargetSelectionPurposeV4::LibraryOrder,
                legal_targets,
                min_targets: 3,
                max_targets: 3,
                ordered: true,
                ..
            } if legal_targets.len() == 3
        ));
    }

    engine::step(
        &mut state,
        Action::ChooseEffectTarget(Target::Object(library[1])),
    )
    .unwrap();
    let next = engine::advance_until_decision(&mut state);
    if matches!(next, Decision::ChooseEffectTargets { .. }) {
        engine::step(
            &mut state,
            Action::ChooseEffectTarget(Target::Object(library[0])),
        )
        .unwrap();
    }
    let _ = pass_until_source_finishes(&mut state, spy);
    assert_eq!(
        state.players[1].graveyard,
        vec![library[1], library[0], library[2]]
    );
    assert_eq!(state.players[1].library, vec![library[3]]);
    assert_eq!(state.objects.get(library[2]).name, "Forest");
}

#[test]
fn balustrade_reveal_continuation_rejects_tampered_prefix_without_moving_cards() {
    let mut state = ready_main(&[], &["Lightning Bolt", "Lotleth Giant", "Forest"]);
    let spy = put_object(&mut state, PlayerId::P0, "Balustrade Spy", Zone::Hand);
    add_mana(&mut state, ManaColor::B, 4);
    engine::step(&mut state, Action::CastSpell(spy)).unwrap();
    let _ = advance_to_targeted_etb(&mut state, spy);
    engine::step(
        &mut state,
        Action::ChooseTarget(Target::Player(PlayerId::P1)),
    )
    .unwrap();
    let order = advance_to_effect_targets(&mut state, spy);
    let first = match order {
        Decision::ChooseEffectTargets { legal_targets, .. } => legal_targets[0],
        _ => unreachable!(),
    };
    let pending = state.engine.pending_effect.as_mut().unwrap();
    let PendingEffectChoice::SelectTargets { purpose, .. } = pending.choice.as_mut().unwrap()
    else {
        unreachable!()
    };
    let EffectTargetSelectionPurpose::OrderRevealedIntoGraveyard {
        original_prefix, ..
    } = purpose
    else {
        unreachable!()
    };
    original_prefix.pop();
    let tampered = state.clone();
    assert!(engine::step(&mut state, Action::ChooseEffectTarget(first)).is_err());
    assert_eq!(
        state, tampered,
        "tampered continuation is rejected atomically"
    );
    assert!(state.players[1].graveyard.is_empty());
}

#[test]
fn lotleth_giant_targets_only_opponent_and_counts_creature_cards_at_resolution() {
    let mut state = ready_main(&[], &[]);
    let giant = put_object(&mut state, PlayerId::P0, "Lotleth Giant", Zone::Hand);
    put_object(&mut state, PlayerId::P0, "Elvish Mystic", Zone::Graveyard);
    put_object(&mut state, PlayerId::P0, "Lightning Bolt", Zone::Graveyard);
    add_mana(&mut state, ManaColor::B, 7);
    engine::step(&mut state, Action::CastSpell(giant)).unwrap();
    let decision = advance_to_targeted_etb(&mut state, giant);
    let Decision::ChooseTargets { legal_targets, .. } = decision else {
        unreachable!()
    };
    assert_eq!(legal_targets, vec![Target::Player(PlayerId::P1)]);
    let before = state.clone();
    assert!(engine::step(
        &mut state,
        Action::ChooseTarget(Target::Player(PlayerId::P0))
    )
    .is_err());
    assert_eq!(state, before);
    engine::step(
        &mut state,
        Action::ChooseTarget(Target::Player(PlayerId::P1)),
    )
    .unwrap();
    let priority = engine::advance_until_decision(&mut state);
    assert!(matches!(priority, Decision::CastSpellOrPass { .. }));
    put_object(&mut state, PlayerId::P0, "Balustrade Spy", Zone::Graveyard);
    let _ = pass_until_source_finishes(&mut state, giant);
    assert_eq!(state.players[1].life, 18);
    assert_eq!(state.players[0].life, 20);
}

#[test]
fn targeted_etb_metadata_tamper_halts_before_lotleth_damage() {
    let mut state = ready_main(&[], &[]);
    let giant = put_object(&mut state, PlayerId::P0, "Lotleth Giant", Zone::Hand);
    put_object(&mut state, PlayerId::P0, "Elvish Mystic", Zone::Graveyard);
    add_mana(&mut state, ManaColor::B, 7);
    engine::step(&mut state, Action::CastSpell(giant)).unwrap();
    let _ = advance_to_targeted_etb(&mut state, giant);
    engine::step(
        &mut state,
        Action::ChooseTarget(Target::Player(PlayerId::P1)),
    )
    .unwrap();
    let _ = engine::advance_until_decision(&mut state);
    let item = state
        .stack
        .last_mut()
        .expect("Lotleth Giant trigger on stack");
    item.v4.target_spec = Some(TargetSpec::AnyPlayer);
    engine::step(&mut state, Action::Pass).unwrap();
    let _ = engine::advance_until_decision(&mut state);
    engine::step(&mut state, Action::Pass).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::Halted {
            mechanic: UnsupportedMechanic::InvalidEffectContinuation,
            source,
        } if source == giant
    ));
    assert_eq!(state.players[1].life, 20);
}
