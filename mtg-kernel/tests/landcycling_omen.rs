//! Focused coverage for the reusable landcycling and Omen tranche.
//!
//! The checked-in Mage implementations are authoritative: Generous Ent is a
//! 5/7 reach Treefolk with Forestcycling {1} and an ETB Food token; Troll of
//! Khazad-dum is a 6/5 Troll with Swampcycling {1} that needs three blockers;
//! Sagu Wildling is a 3/3 flying Dragon with an ETB gain 3 and may be cast as
//! the {G} sorcery Roost Seek to find a basic land before shuffling itself
//! into its owner's library.

use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardType, CostComponent, Keywords, Subtype, TargetSpec,
    CARD_DEFS,
};
use mtg_kernel::effect::{EffectOp, LibraryCardFilter, PlayerRef};
use mtg_kernel::engine::{self, Action, Decision, UnsupportedMechanic};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{Cost, ManaColor, Pip};
use mtg_kernel::rl::{legal_action_candidates_v1, observe_v2};
use mtg_kernel::state::{
    CastMethodV4, Counters, GameObject, GameState, ObjectStateV4, StackItemKind, Step, Target, Zone,
};
use mtg_kernel::surface_v2::{HarnessSurfaceV2, SurfaceDecision};

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
        Zone::Stack => panic!("test helper does not construct stack objects"),
    }
    id
}

fn ready_main(library: &[&str], seed: u64) -> GameState {
    let p0 = library.iter().map(|name| card_id(name)).collect::<Vec<_>>();
    let p1 = vec![card_id("Mountain"); 8];
    let mut state = GameState::new_from_libraries(&p0, &p1, card_name, seed);
    state.step = Step::Main1;
    state.active_player = PlayerId::P0;
    state.priority_player = PlayerId::P0;
    state
}

fn add_mana(state: &mut GameState, color: ManaColor, amount: u8) {
    state.players[PlayerId::P0.index()].mana_pool[color.pool_index()] += amount;
}

fn pass_until<F>(state: &mut GameState, mut done: F)
where
    F: FnMut(&GameState, &Decision) -> bool,
{
    for _ in 0..64 {
        let decision = engine::advance_until_decision(state);
        if done(state, &decision) {
            return;
        }
        match decision {
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => panic!("unexpected decision during bounded resolution: {other:?}"),
        }
    }
    panic!("bounded resolution did not reach its expected state");
}

fn activate_landcycling_to_search(
    state: &mut GameState,
    source: ObjectId,
) -> (ObjectId, Vec<Target>) {
    engine::step(state, Action::ActivateAbility(source, 0)).unwrap();
    let decision = engine::advance_until_decision(state);
    assert!(matches!(decision, Decision::CastSpellOrPass { .. }));
    assert_eq!(state.objects.get(source).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(source).zone_change_count, 1);
    assert!(matches!(
        state.stack.last(),
        Some(item) if item.kind == StackItemKind::ActivatedAbility && item.source == source
    ));

    let mut search = None;
    pass_until(state, |_, decision| {
        if let Decision::ChooseEffectTargets {
            source: actual,
            legal_targets,
            ..
        } = decision
        {
            assert_eq!(*actual, source);
            search = Some(legal_targets.clone());
            true
        } else {
            false
        }
    });
    (source, search.expect("landcycling search decision"))
}

#[test]
fn registry_and_generated_definitions_match_the_three_mage_cards() {
    assert_eq!(card_id("Bird Illusion Token"), 135);
    assert_eq!(card_id("Sagu Wildling"), 142);
    assert_eq!(card_id("Troll of Khazad-dum"), 143);
    assert_eq!(card_id("Food Token"), 144);
    assert_eq!(CARD_DEFS.len(), 160);
    assert_eq!(Subtype::Troll.stable_id(), 59);

    let ent = &CARD_DEFS[card_id("Generous Ent") as usize];
    assert_eq!(ent.capability, CardCapability::Full);
    assert_eq!(
        ent.cost,
        Cost {
            pips: &[Pip::Colored(ManaColor::G)],
            generic: 5,
            x_count: 0,
        }
    );
    assert_eq!(ent.types, &[CardType::Creature]);
    assert_eq!(ent.subtypes, &[Subtype::Treefolk]);
    assert_eq!((ent.power, ent.toughness), (Some(5), Some(7)));
    assert!(ent.keywords.has(Keywords::REACH));
    assert_eq!(ent.activated_abilities.len(), 1);
    assert_eq!(ent.activated_abilities[0].activation_zone, Zone::Hand);
    assert_eq!(
        ent.activated_abilities[0].cost,
        &[
            CostComponent::Mana(Cost {
                pips: &[],
                generic: 1,
                x_count: 0,
            }),
            CostComponent::DiscardSelf,
        ]
    );
    assert_eq!(
        (ent.activated_abilities[0].effect)(),
        EffectOp::SearchLibraryToHand {
            player: PlayerRef::Controller,
            filter: LibraryCardFilter::LandWithSubtype(Subtype::Forest),
        }
    );

    let troll = &CARD_DEFS[card_id("Troll of Khazad-dum") as usize];
    assert_eq!(
        troll.cost,
        Cost {
            pips: &[Pip::Colored(ManaColor::B)],
            generic: 5,
            x_count: 0,
        }
    );
    assert_eq!(troll.subtypes, &[Subtype::Troll]);
    assert_eq!((troll.power, troll.toughness), (Some(6), Some(5)));
    assert_eq!(troll.minimum_blockers, 3);
    assert_eq!(troll.activated_abilities.len(), 1);
    assert_eq!(
        troll.activated_abilities[0].cost,
        &[
            CostComponent::Mana(Cost {
                pips: &[],
                generic: 1,
                x_count: 0,
            }),
            CostComponent::DiscardSelf,
        ]
    );
    assert_eq!(
        (troll.activated_abilities[0].effect)(),
        EffectOp::SearchLibraryToHand {
            player: PlayerRef::Controller,
            filter: LibraryCardFilter::LandWithSubtype(Subtype::Swamp),
        }
    );

    let sagu = &CARD_DEFS[card_id("Sagu Wildling") as usize];
    assert_eq!(
        sagu.cost,
        Cost {
            pips: &[Pip::Colored(ManaColor::G)],
            generic: 4,
            x_count: 0,
        }
    );
    assert_eq!(sagu.subtypes, &[Subtype::Dragon]);
    assert_eq!((sagu.power, sagu.toughness), (Some(3), Some(3)));
    assert!(sagu.keywords.has(Keywords::FLYING));
    let omen = sagu.omen.as_ref().expect("Roost Seek Omen definition");
    assert_eq!(
        omen.cost,
        Cost {
            pips: &[Pip::Colored(ManaColor::G)],
            generic: 0,
            x_count: 0,
        }
    );
    assert_eq!(omen.types, &[CardType::Sorcery]);
    assert_eq!(omen.target_spec, TargetSpec::None);
    assert_eq!(
        (omen.effect)(),
        EffectOp::SearchLibraryToHand {
            player: PlayerRef::Controller,
            filter: LibraryCardFilter::BasicLand,
        }
    );

    let food = &CARD_DEFS[card_id("Food Token") as usize];
    assert!(food.is_token);
    assert_eq!(food.types, &[CardType::Artifact]);
    assert_eq!(food.subtypes, &[Subtype::Food]);
    assert_eq!(food.activated_abilities.len(), 1);
    assert_eq!(
        food.activated_abilities[0].cost,
        &[
            CostComponent::Mana(Cost {
                pips: &[],
                generic: 2,
                x_count: 0,
            }),
            CostComponent::Tap,
            CostComponent::SacrificeSelf,
        ]
    );
}

#[test]
fn forestcycling_and_swampcycling_pay_from_hand_and_search_the_right_subtype() {
    let cases = [
        (
            "Generous Ent",
            vec!["Forest", "Gingerbread Cabin", "Swamp", "Mountain"],
            vec!["Forest", "Gingerbread Cabin"],
        ),
        (
            "Troll of Khazad-dum",
            vec!["Forest", "Swamp", "Mountain"],
            vec!["Swamp"],
        ),
    ];
    for (case, (source_name, library, expected_names)) in cases.into_iter().enumerate() {
        let mut state = ready_main(&library, 0x4c41_4e44_4359_0000 + case as u64);
        add_mana(&mut state, ManaColor::C, 1);
        let source = put_object(&mut state, PlayerId::P0, source_name, Zone::Hand);
        let (_, legal) = activate_landcycling_to_search(&mut state, source);
        let mut actual_names = legal
            .iter()
            .map(|target| match target {
                Target::Object(object) => state.objects.get(*object).name.as_str(),
                other => panic!("library search exposed non-object target {other:?}"),
            })
            .collect::<Vec<_>>();
        actual_names.sort_unstable();
        let mut expected_names = expected_names;
        expected_names.sort_unstable();
        assert_eq!(actual_names, expected_names, "{source_name}");

        let selected = match legal[0] {
            Target::Object(object) => object,
            _ => unreachable!(),
        };
        let mut replay = state.clone();
        engine::step(
            &mut state,
            Action::ChooseEffectTarget(Target::Object(selected)),
        )
        .unwrap();
        engine::step(
            &mut replay,
            Action::ChooseEffectTarget(Target::Object(selected)),
        )
        .unwrap();
        let state_decision = engine::advance_until_decision(&mut state);
        let replay_decision = engine::advance_until_decision(&mut replay);
        assert_eq!(state_decision, replay_decision);
        assert_eq!(state, replay);
        assert_eq!(
            state.diagnostic_state_hash(),
            replay.diagnostic_state_hash()
        );
        assert!(state.players[0].hand.contains(&selected));
        assert!(state
            .known_hand_cards(PlayerId::P1, PlayerId::P0)
            .iter()
            .any(|known| known.object == selected));
        assert_eq!(state.objects.get(source).zone, Zone::Graveyard);
    }
}

#[test]
fn landcycling_rejects_missing_cost_and_wrong_zone_and_may_fail_to_find() {
    let mut no_mana = ready_main(&["Forest"], 0x4c41_4e44_4359_0010);
    let ent = put_object(&mut no_mana, PlayerId::P0, "Generous Ent", Zone::Hand);
    assert!(engine::step(&mut no_mana, Action::ActivateAbility(ent, 0)).is_err());

    let mut wrong_zone = ready_main(&["Swamp"], 0x4c41_4e44_4359_0011);
    add_mana(&mut wrong_zone, ManaColor::C, 1);
    let troll = put_object(
        &mut wrong_zone,
        PlayerId::P0,
        "Troll of Khazad-dum",
        Zone::Graveyard,
    );
    assert!(engine::step(&mut wrong_zone, Action::ActivateAbility(troll, 0)).is_err());

    let mut no_hit = ready_main(&["Mountain"], 0x4c41_4e44_4359_0012);
    add_mana(&mut no_hit, ManaColor::C, 1);
    let ent = put_object(&mut no_hit, PlayerId::P0, "Generous Ent", Zone::Hand);
    let (_, legal) = activate_landcycling_to_search(&mut no_hit, ent);
    assert!(legal.is_empty());
    engine::step(&mut no_hit, Action::FinishEffectSelection).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut no_hit),
        Decision::CastSpellOrPass { .. }
    ));
    assert!(no_hit.players[0].hand.is_empty());
    assert_eq!(no_hit.objects.get(ent).zone, Zone::Graveyard);
    assert_eq!(no_hit.players[0].library.len(), 1);
}

#[test]
fn normal_creature_casts_resolve_with_keywords_etbs_and_food_activation() {
    let mut ent_state = ready_main(&["Mountain"], 0x4e4f_524d_414c_0001);
    add_mana(&mut ent_state, ManaColor::G, 6);
    let ent = put_object(&mut ent_state, PlayerId::P0, "Generous Ent", Zone::Hand);
    engine::step(&mut ent_state, Action::CastSpell(ent)).unwrap();
    pass_until(&mut ent_state, |state, _| {
        state.players[0]
            .battlefield
            .iter()
            .any(|&object| state.objects.get(object).card_def == card_id("Food Token"))
    });
    assert_eq!(ent_state.objects.get(ent).zone, Zone::Battlefield);
    assert!(CARD_DEFS[ent_state.objects.get(ent).card_def as usize]
        .keywords
        .has(Keywords::REACH));
    let food = ent_state.players[0]
        .battlefield
        .iter()
        .copied()
        .find(|&object| ent_state.objects.get(object).card_def == card_id("Food Token"))
        .expect("Ent ETB Food");

    add_mana(&mut ent_state, ManaColor::C, 2);
    engine::step(&mut ent_state, Action::ActivateAbility(food, 0)).unwrap();
    pass_until(&mut ent_state, |state, _| state.players[0].life == 23);
    assert!(!ent_state.players[0].battlefield.contains(&food));

    let mut sagu_state = ready_main(&["Mountain"], 0x4e4f_524d_414c_0002);
    add_mana(&mut sagu_state, ManaColor::G, 5);
    let sagu = put_object(&mut sagu_state, PlayerId::P0, "Sagu Wildling", Zone::Hand);
    engine::step(&mut sagu_state, Action::CastSpell(sagu)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut sagu_state),
        Decision::ChooseSpellMode { mode_count: 2, .. }
    ));
    engine::step(&mut sagu_state, Action::ChooseSpellMode(0)).unwrap();
    pass_until(&mut sagu_state, |state, _| {
        state.objects.get(sagu).zone == Zone::Battlefield && state.players[0].life == 23
    });
    assert!(CARD_DEFS[sagu_state.objects.get(sagu).card_def as usize]
        .keywords
        .has(Keywords::FLYING));

    let mut troll_state = ready_main(&["Mountain"], 0x4e4f_524d_414c_0003);
    add_mana(&mut troll_state, ManaColor::B, 6);
    let troll = put_object(
        &mut troll_state,
        PlayerId::P0,
        "Troll of Khazad-dum",
        Zone::Hand,
    );
    engine::step(&mut troll_state, Action::CastSpell(troll)).unwrap();
    pass_until(&mut troll_state, |state, _| {
        state.objects.get(troll).zone == Zone::Battlefield
    });
    assert_eq!(troll_state.objects.get(troll).zone, Zone::Battlefield);
}

#[test]
fn omen_is_a_sorcery_cast_method_searches_only_basic_lands_and_shuffles_source() {
    let mut state = ready_main(
        &["Forest", "Island", "Gingerbread Cabin", "Lightning Bolt"],
        0x4f4d_454e_0000_0001,
    );
    add_mana(&mut state, ManaColor::G, 1);
    let guttersnipe = put_object(&mut state, PlayerId::P0, "Guttersnipe", Zone::Battlefield);
    let sagu = put_object(&mut state, PlayerId::P0, "Sagu Wildling", Zone::Hand);

    engine::step(&mut state, Action::CastSpell(sagu)).unwrap();
    let staged_bytes = serde_json::to_vec(&state).unwrap();
    let mut staged_round_trip: GameState = serde_json::from_slice(&staged_bytes).unwrap();
    assert_eq!(state, staged_round_trip);
    assert_eq!(
        state.diagnostic_state_hash(),
        staged_round_trip.diagnostic_state_hash()
    );

    let decision = engine::advance_until_decision(&mut state);
    let replay_decision = engine::advance_until_decision(&mut staged_round_trip);
    assert_eq!(decision, replay_decision);
    assert_eq!(state, staged_round_trip);
    assert!(matches!(decision, Decision::CastSpellOrPass { .. }));
    let omen_item = state
        .stack
        .iter()
        .find(|item| item.source == sagu)
        .expect("Roost Seek spell on stack");
    assert_eq!(omen_item.v4.cast_method, Some(CastMethodV4::Omen));
    assert_eq!(omen_item.mode_chosen, 0);
    assert_eq!(omen_item.v4.target_spec, Some(TargetSpec::None));
    assert!(state.stack.iter().any(|item| {
        item.kind == StackItemKind::TriggeredAbility && item.source == guttersnipe
    }));

    let mut search_decision = None;
    pass_until(&mut state, |_, decision| {
        if matches!(decision, Decision::ChooseEffectTargets { .. }) {
            search_decision = Some(decision.clone());
            true
        } else {
            false
        }
    });
    assert_eq!(
        state.players[1].life, 18,
        "Omen is an instant-or-sorcery cast"
    );
    let Decision::ChooseEffectTargets { legal_targets, .. } =
        search_decision.expect("Roost Seek basic-land search")
    else {
        unreachable!()
    };
    let mut offered = legal_targets
        .iter()
        .map(|target| match target {
            Target::Object(object) => state.objects.get(*object).name.as_str(),
            _ => panic!("basic-land search exposed a non-card target"),
        })
        .collect::<Vec<_>>();
    offered.sort_unstable();
    assert_eq!(offered, vec!["Forest", "Island"]);
    let selected = legal_targets
        .iter()
        .find_map(|target| match target {
            Target::Object(object) if state.objects.get(*object).name == "Forest" => Some(*object),
            _ => None,
        })
        .expect("Forest offered");

    let pending_bytes = serde_json::to_vec(&state).unwrap();
    let mut replay: GameState = serde_json::from_slice(&pending_bytes).unwrap();
    engine::step(
        &mut state,
        Action::ChooseEffectTarget(Target::Object(selected)),
    )
    .unwrap();
    engine::step(
        &mut replay,
        Action::ChooseEffectTarget(Target::Object(selected)),
    )
    .unwrap();
    let decision = engine::advance_until_decision(&mut state);
    let replay_decision = engine::advance_until_decision(&mut replay);
    assert_eq!(decision, replay_decision);
    assert_eq!(state, replay);
    assert_eq!(
        state.diagnostic_state_hash(),
        replay.diagnostic_state_hash()
    );
    assert!(state.players[0].hand.contains(&selected));
    assert!(state.players[0].library.contains(&sagu));
    assert_eq!(state.objects.get(sagu).zone, Zone::Library);
    assert_eq!(state.objects.get(sagu).zone_change_count, 2);
    assert!(!state.players[0].graveyard.contains(&sagu));
    assert!(!state.players[0].battlefield.contains(&sagu));
    assert!(state
        .known_hand_cards(PlayerId::P1, PlayerId::P0)
        .iter()
        .any(|known| known.object == selected));
}

#[test]
fn omen_may_fail_to_find_and_insufficient_green_cannot_start_either_form() {
    let mut no_mana = ready_main(&["Forest"], 0x4f4d_454e_0000_0010);
    let sagu = put_object(&mut no_mana, PlayerId::P0, "Sagu Wildling", Zone::Hand);
    assert!(engine::step(&mut no_mana, Action::CastSpell(sagu)).is_err());

    let mut colorless = ready_main(&["Forest"], 0x4f4d_454e_0000_0011);
    add_mana(&mut colorless, ManaColor::C, 5);
    let sagu = put_object(&mut colorless, PlayerId::P0, "Sagu Wildling", Zone::Hand);
    assert!(engine::step(&mut colorless, Action::CastSpell(sagu)).is_err());

    let mut no_hit = ready_main(&["Gingerbread Cabin"], 0x4f4d_454e_0000_0012);
    add_mana(&mut no_hit, ManaColor::G, 1);
    let sagu = put_object(&mut no_hit, PlayerId::P0, "Sagu Wildling", Zone::Hand);
    engine::step(&mut no_hit, Action::CastSpell(sagu)).unwrap();
    let mut search = None;
    pass_until(&mut no_hit, |_, decision| {
        if let Decision::ChooseEffectTargets { legal_targets, .. } = decision {
            search = Some(legal_targets.clone());
            true
        } else {
            false
        }
    });
    assert!(search.expect("Roost Seek search").is_empty());
    engine::step(&mut no_hit, Action::FinishEffectSelection).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut no_hit),
        Decision::CastSpellOrPass { .. }
    ));
    assert_eq!(no_hit.objects.get(sagu).zone, Zone::Library);
    assert!(no_hit.players[0].library.contains(&sagu));
    assert_eq!(
        no_hit.players[0].life, 20,
        "Omen does not enter the battlefield"
    );
}

#[test]
fn countered_omen_goes_to_the_graveyard_without_searching_or_shuffling() {
    let mut state = ready_main(
        &["Forest", "Island", "Gingerbread Cabin"],
        0x4f4d_454e_0000_0013,
    );
    add_mana(&mut state, ManaColor::G, 1);
    state.players[PlayerId::P1.index()].mana_pool[ManaColor::U.pool_index()] = 2;
    let sagu = put_object(&mut state, PlayerId::P0, "Sagu Wildling", Zone::Hand);
    let counterspell = put_object(&mut state, PlayerId::P1, "Counterspell", Zone::Hand);
    let library_before = state.players[PlayerId::P0.index()].library.clone();

    engine::step(&mut state, Action::CastSpell(sagu)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass {
            player: PlayerId::P0,
            ..
        }
    ));
    engine::step(&mut state, Action::Pass).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass {
            player: PlayerId::P1,
            ..
        }
    ));
    engine::step(&mut state, Action::CastSpell(counterspell)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::ChooseTargets {
            player: PlayerId::P1,
            legal_targets,
            ..
        } if legal_targets.contains(&Target::Object(sagu))
    ));
    engine::step(&mut state, Action::ChooseTarget(Target::Object(sagu))).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass {
            player: PlayerId::P1,
            ..
        }
    ));
    engine::step(&mut state, Action::Pass).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass {
            player: PlayerId::P0,
            ..
        }
    ));
    engine::step(&mut state, Action::Pass).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));

    assert_eq!(state.objects.get(sagu).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(counterspell).zone, Zone::Graveyard);
    assert_eq!(state.players[PlayerId::P0.index()].library, library_before);
    assert!(!state.players[PlayerId::P0.index()].library.contains(&sagu));
}

#[test]
fn omen_form_choice_and_pending_source_incarnation_are_snapshot_stable() {
    let mut state = ready_main(&["Forest"], 0x4f4d_454e_0000_0020);
    add_mana(&mut state, ManaColor::G, 5);
    let sagu = put_object(&mut state, PlayerId::P0, "Sagu Wildling", Zone::Hand);
    engine::step(&mut state, Action::CastSpell(sagu)).unwrap();

    let bytes = serde_json::to_vec(&state).unwrap();
    let mut restored: GameState = serde_json::from_slice(&bytes).unwrap();
    let decision = engine::advance_until_decision(&mut state);
    let restored_decision = engine::advance_until_decision(&mut restored);
    assert_eq!(decision, restored_decision);
    assert_eq!(state, restored);
    assert_eq!(
        state.diagnostic_state_hash(),
        restored.diagnostic_state_hash()
    );
    assert!(matches!(
        decision,
        Decision::ChooseSpellMode {
            spell,
            mode_count: 2,
            ..
        } if spell == sagu
    ));
    let ids = legal_action_candidates_v1(&SurfaceDecision::Decision(decision.clone()), &state)
        .unwrap()
        .into_iter()
        .map(|candidate| candidate.record.stable_id)
        .collect::<Vec<_>>();
    let restored_ids =
        legal_action_candidates_v1(&SurfaceDecision::Decision(restored_decision), &restored)
            .unwrap()
            .into_iter()
            .map(|candidate| candidate.record.stable_id)
            .collect::<Vec<_>>();
    assert_eq!(ids, restored_ids);

    let mut tampered: GameState = serde_json::from_slice(&bytes).unwrap();
    tampered.objects.get_mut(sagu).zone_change_count += 1;
    assert!(matches!(
        engine::advance_until_decision(&mut tampered),
        Decision::Halted {
            mechanic: UnsupportedMechanic::InvalidEffectContinuation,
            source,
        } if source == sagu
    ));
}

#[test]
fn troll_requires_zero_or_at_least_three_blockers() {
    let mut state = ready_main(&["Mountain"], 0x5452_4f4c_4c00_0001);
    let troll = put_object(
        &mut state,
        PlayerId::P0,
        "Troll of Khazad-dum",
        Zone::Battlefield,
    );
    let observation = observe_v2(&state, &HarnessSurfaceV2::new(), PlayerId::P0, 0).unwrap();
    let observed_troll = observation.projection.battlefield[PlayerId::P0.index()]
        .iter()
        .find(|card| card.stable.arena_id == troll.0)
        .expect("Troll is public on the battlefield");
    assert_eq!(
        observed_troll
            .characteristics
            .effective_keywords
            .minimum_blockers,
        3
    );
    let blockers = (0..3)
        .map(|_| {
            put_object(
                &mut state,
                PlayerId::P1,
                "Voldaren Epicure",
                Zone::Battlefield,
            )
        })
        .collect::<Vec<_>>();
    state.step = Step::DeclareBlockers;
    state.active_player = PlayerId::P0;
    state.engine.combat.attackers = vec![troll];
    state.engine.combat.attackers_declared = true;

    match engine::advance_until_decision(&mut state) {
        Decision::DeclareBlockers {
            attackers,
            legal_blockers,
            ..
        } => {
            assert_eq!(attackers, vec![troll]);
            assert_eq!(legal_blockers, vec![(troll, blockers.clone())]);
        }
        other => panic!("expected blocker declaration, got {other:?}"),
    }
    let policy_actions = legal_action_candidates_v1(
        &SurfaceDecision::DeclareBlockersForAttacker {
            attacker: troll,
            legal_blockers: blockers.clone(),
        },
        &state,
    )
    .unwrap();
    assert_eq!(
        policy_actions.len(),
        2,
        "policy exposes only no block or all three blockers"
    );
    let before = state.clone();
    assert!(engine::step(
        &mut state,
        Action::DeclareBlockers(vec![(blockers[0], troll), (blockers[1], troll)]),
    )
    .is_err());
    assert_eq!(state, before);
    engine::step(
        &mut state,
        Action::DeclareBlockers(
            blockers
                .iter()
                .copied()
                .map(|blocker| (blocker, troll))
                .collect(),
        ),
    )
    .unwrap();

    let mut short = ready_main(&["Mountain"], 0x5452_4f4c_4c00_0002);
    let troll = put_object(
        &mut short,
        PlayerId::P0,
        "Troll of Khazad-dum",
        Zone::Battlefield,
    );
    let blocker = put_object(
        &mut short,
        PlayerId::P1,
        "Voldaren Epicure",
        Zone::Battlefield,
    );
    put_object(
        &mut short,
        PlayerId::P1,
        "Voldaren Epicure",
        Zone::Battlefield,
    );
    short.step = Step::DeclareBlockers;
    short.active_player = PlayerId::P0;
    short.engine.combat.attackers = vec![troll];
    short.engine.combat.attackers_declared = true;
    assert!(matches!(
        engine::advance_until_decision(&mut short),
        Decision::DeclareBlockers { legal_blockers, .. }
            if legal_blockers == vec![(troll, vec![])]
    ));
    assert!(engine::step(&mut short, Action::DeclareBlockers(vec![(blocker, troll)]),).is_err());
    engine::step(&mut short, Action::DeclareBlockers(vec![])).unwrap();
}
