use mtg_kernel::card_def::{
    card_id_by_name, preflight_fully_supported_deck, CardCapability, CardType, Keywords, Subtype,
    CARD_DEFS,
};
use mtg_kernel::effect::{self, EffectOp, ExecCtx, ObjectRef};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{self, Cost, ManaColor, Pip};
use mtg_kernel::rl::{legal_action_candidates_v1, ActionSemanticV1, LegalActionCandidateV1};
use mtg_kernel::state::{
    AbilityKindV4, Counters, GameObject, GameState, ObjectStateV4, Step, Zone,
};
use mtg_kernel::surface_v2::{SurfaceAction, SurfaceDecision};

fn ready_main1() -> GameState {
    let mut state = GameState::new_from_libraries(&[], &[], |_| String::new(), 43);
    state.step = Step::Main1;
    state.active_player = PlayerId::P0;
    state.priority_player = PlayerId::P0;
    state
}

fn put_object(
    state: &mut GameState,
    name: &str,
    zone: Zone,
    tapped: bool,
    summoning_sick: bool,
) -> ObjectId {
    let card_def = card_id_by_name(name).unwrap_or_else(|| panic!("{name} in CARD_DEFS"));
    let id = state.objects.push(GameObject {
        card_def,
        name: name.to_string(),
        owner: PlayerId::P0,
        controller: PlayerId::P0,
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
        Zone::Hand => state.players[0].hand.push(id),
        Zone::Battlefield => state.players[0].battlefield.push(id),
        Zone::Graveyard => state.players[0].graveyard.push(id),
        Zone::Exile => state.exile.push(id),
        Zone::Library => state.players[0].library.push(id),
        Zone::Command => state.command.push(id),
        Zone::Stack => panic!("test helper does not build stack items"),
    }
    id
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

fn resolve_spell(state: &mut GameState, spell: ObjectId) {
    for _ in 0..8 {
        if state.objects.get(spell).zone == Zone::Battlefield {
            return;
        }
        match engine::advance_until_decision(state) {
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => panic!("unexpected decision while resolving permanent: {other:?}"),
        }
    }
    panic!("permanent did not resolve within the bounded priority walk");
}

#[test]
fn registry_ids_and_printed_mana_permanent_contracts_are_exact() {
    let expected_ids = [
        ("Drossforge Bridge", 23),
        ("Elvish Mystic", 26),
        ("Fyndhorn Elves", 40),
        ("Llanowar Elves", 67),
        ("Mistvault Bridge", 73),
        ("Seat of the Synod", 102),
        ("Silverbluff Bridge", 103),
        ("Slagwoods Bridge", 104),
        ("Vault of Whispers", 125),
    ];
    let ids: Vec<u16> = expected_ids
        .iter()
        .map(|(name, expected)| {
            let id = card_id_by_name(name).unwrap();
            assert_eq!(id, *expected, "append-only card id for {name}");
            assert_eq!(CARD_DEFS[id as usize].capability, CardCapability::Full);
            id
        })
        .collect();
    preflight_fully_supported_deck(&ids).unwrap();

    for name in ["Elvish Mystic", "Fyndhorn Elves", "Llanowar Elves"] {
        let def = &CARD_DEFS[card_id_by_name(name).unwrap() as usize];
        assert!(def.has_type(CardType::Creature), "{name}");
        assert!(def.subtypes.contains(&Subtype::Elf), "{name}");
        assert!(def.subtypes.contains(&Subtype::Druid), "{name}");
        assert_eq!((def.power, def.toughness), (Some(1), Some(1)), "{name}");
        assert_eq!(def.mana_ability_choices, &[ManaColor::G], "{name}");
        assert!(!def.enters_battlefield_tapped, "{name}");
    }

    for (name, choices) in [
        ("Seat of the Synod", &[ManaColor::U][..]),
        ("Vault of Whispers", &[ManaColor::B][..]),
        ("Drossforge Bridge", &[ManaColor::B, ManaColor::R][..]),
        ("Mistvault Bridge", &[ManaColor::U, ManaColor::B][..]),
        ("Silverbluff Bridge", &[ManaColor::U, ManaColor::R][..]),
        ("Slagwoods Bridge", &[ManaColor::R, ManaColor::G][..]),
    ] {
        let def = &CARD_DEFS[card_id_by_name(name).unwrap() as usize];
        assert!(def.has_type(CardType::Artifact), "{name}");
        assert!(def.has_type(CardType::Land), "{name}");
        assert_eq!(def.mana_ability_choices, choices, "{name}");
        assert!(!def.is_castable(), "lands are played, not cast: {name}");
    }

    for name in [
        "Drossforge Bridge",
        "Mistvault Bridge",
        "Silverbluff Bridge",
        "Slagwoods Bridge",
    ] {
        let def = &CARD_DEFS[card_id_by_name(name).unwrap() as usize];
        assert!(def.enters_battlefield_tapped, "{name}");
        assert!(def.keywords.has(Keywords::INDESTRUCTIBLE), "{name}");
    }
}

#[test]
fn mana_elves_cast_normally_and_tap_only_after_summoning_sickness_ends() {
    let mut cast_state = ready_main1();
    let mystic = put_object(&mut cast_state, "Elvish Mystic", Zone::Hand, false, false);
    cast_state.players[0].mana_pool[ManaColor::G.pool_index()] = 1;
    engine::step(&mut cast_state, Action::CastSpell(mystic)).unwrap();
    resolve_spell(&mut cast_state, mystic);
    assert_eq!(cast_state.objects.get(mystic).zone, Zone::Battlefield);
    assert!(cast_state.objects.get(mystic).summoning_sick);
    assert!(mana_candidates(&cast_state, mystic).is_empty());

    for name in ["Elvish Mystic", "Fyndhorn Elves", "Llanowar Elves"] {
        let mut state = ready_main1();
        let elf = put_object(&mut state, name, Zone::Battlefield, false, true);
        assert!(mana::gather_sources(PlayerId::P0, &state)
            .iter()
            .all(|source| source.id != elf));
        assert!(mana_candidates(&state, elf).is_empty());

        state.objects.get_mut(elf).summoning_sick = false;
        let candidates = mana_candidates(&state, elf);
        assert_eq!(candidates.len(), 1, "{name}");
        assert!(matches!(
            candidates[0].record.semantic,
            ActionSemanticV1::ActivateManaAbility {
                mana_choice: Some(ManaColor::G),
                ..
            }
        ));
        engine::step(&mut state, Action::ActivateManaAbility(elf)).unwrap();
        assert!(state.objects.get(elf).tapped, "{name}");
        assert_eq!(state.players[0].mana_pool[ManaColor::G.pool_index()], 1);
        assert!(engine::step(&mut state, Action::ActivateManaAbility(elf)).is_err());
    }
}

#[test]
fn artifact_lands_play_untapped_and_ignore_creature_summoning_sickness() {
    for (name, color) in [
        ("Seat of the Synod", ManaColor::U),
        ("Vault of Whispers", ManaColor::B),
    ] {
        let mut state = ready_main1();
        let land = put_object(&mut state, name, Zone::Hand, false, false);
        engine::step(&mut state, Action::PlayLand(land)).unwrap();
        assert_eq!(state.objects.get(land).zone, Zone::Battlefield, "{name}");
        assert!(!state.objects.get(land).tapped, "{name}");
        assert!(state.objects.get(land).summoning_sick, "{name}");
        assert_eq!(mana_candidates(&state, land).len(), 1, "{name}");
        engine::step(&mut state, Action::ActivateManaAbility(land)).unwrap();
        assert!(state.objects.get(land).tapped, "{name}");
        assert_eq!(state.players[0].mana_pool[color.pool_index()], 1, "{name}");
    }
}

#[test]
fn bridges_enter_tapped_and_expose_two_distinct_one_mana_actions() {
    for (name, expected) in [
        ("Drossforge Bridge", [ManaColor::B, ManaColor::R]),
        ("Mistvault Bridge", [ManaColor::U, ManaColor::B]),
        ("Silverbluff Bridge", [ManaColor::U, ManaColor::R]),
        ("Slagwoods Bridge", [ManaColor::R, ManaColor::G]),
    ] {
        let mut state = ready_main1();
        let bridge = put_object(&mut state, name, Zone::Hand, false, false);
        engine::step(&mut state, Action::PlayLand(bridge)).unwrap();
        assert!(state.objects.get(bridge).tapped, "{name}");
        assert!(mana_candidates(&state, bridge).is_empty(), "{name}");

        state.objects.get_mut(bridge).tapped = false;
        let candidates = mana_candidates(&state, bridge);
        assert_eq!(candidates.len(), 2, "{name}");
        assert_ne!(
            candidates[0].record.stable_id, candidates[1].record.stable_id,
            "{name} color choices need distinct stable action ids"
        );
        let offered: Vec<ManaColor> = candidates
            .iter()
            .map(|candidate| match candidate.record.semantic {
                ActionSemanticV1::ActivateManaAbility {
                    mana_choice: Some(color),
                    ..
                } => color,
                ref other => panic!("unexpected bridge semantic: {other:?}"),
            })
            .collect();
        assert_eq!(offered, expected, "printed ability order for {name}");

        for (ability_index, candidate) in candidates.into_iter().enumerate() {
            let chosen = match candidate.record.semantic {
                ActionSemanticV1::ActivateManaAbility {
                    mana_choice: Some(color),
                    ..
                } => color,
                _ => unreachable!(),
            };
            let mut branch = state.clone();
            let SurfaceAction::Action(action) = candidate.surface_action else {
                panic!("mana activation is a direct engine action")
            };
            engine::step(&mut branch, action).unwrap();
            assert!(branch.objects.get(bridge).tapped, "{name}");
            assert_eq!(
                branch.players[0].mana_pool[chosen.pool_index()],
                1,
                "{name}"
            );
            assert_eq!(branch.players[0].mana_pool.iter().sum::<u8>(), 1, "{name}");
            assert_eq!(
                branch.objects.get(bridge).v4.ability_uses_this_turn,
                [mtg_kernel::state::AbilityUseV4 {
                    ability_kind: AbilityKindV4::Mana,
                    ability_index: u16::try_from(ability_index).unwrap(),
                    uses: 1,
                }],
                "{name} records the selected printed mana ability"
            );
        }
    }
}

#[test]
fn dual_land_sources_backtrack_by_color_when_paying_exact_costs() {
    let mut state = ready_main1();
    let bridge = put_object(
        &mut state,
        "Mistvault Bridge",
        Zone::Battlefield,
        false,
        true,
    );
    let seat = put_object(
        &mut state,
        "Seat of the Synod",
        Zone::Battlefield,
        false,
        true,
    );
    let cost = Cost {
        pips: &[Pip::Colored(ManaColor::U), Pip::Colored(ManaColor::B)],
        generic: 0,
        x_count: 0,
    };
    let plan = mana::can_pay(&cost, 0, PlayerId::P0, &state).expect("Mistvault plus Seat pays UB");
    assert_eq!(
        plan.taps,
        vec![(seat, ManaColor::U), (bridge, ManaColor::B)]
    );
}

#[test]
fn bridge_entry_and_indestructible_use_shared_zone_and_destroy_paths() {
    let mut state = ready_main1();
    let bridge = put_object(
        &mut state,
        "Drossforge Bridge",
        Zone::Graveyard,
        false,
        false,
    );
    effect::execute(
        &EffectOp::MoveObject {
            object: ObjectRef::ThisSource,
            to_zone: Zone::Battlefield,
        },
        &ExecCtx::no_targets(bridge, PlayerId::P0),
        &mut state,
    );
    assert!(state.objects.get(bridge).tapped);

    effect::execute(
        &EffectOp::DestroyObject {
            object: ObjectRef::ThisSource,
        },
        &ExecCtx::no_targets(bridge, PlayerId::P0),
        &mut state,
    );
    assert_eq!(state.objects.get(bridge).zone, Zone::Battlefield);

    let furnace = put_object(&mut state, "Great Furnace", Zone::Battlefield, false, true);
    effect::execute(
        &EffectOp::DestroyObject {
            object: ObjectRef::ThisSource,
        },
        &ExecCtx::no_targets(furnace, PlayerId::P0),
        &mut state,
    );
    assert_eq!(state.objects.get(furnace).zone, Zone::Graveyard);

    effect::execute(
        &EffectOp::MoveObject {
            object: ObjectRef::ThisSource,
            to_zone: Zone::Graveyard,
        },
        &ExecCtx::no_targets(bridge, PlayerId::P0),
        &mut state,
    );
    assert_eq!(
        state.objects.get(bridge).zone,
        Zone::Graveyard,
        "indestructible does not prevent sacrifice or another ordinary move"
    );
}
