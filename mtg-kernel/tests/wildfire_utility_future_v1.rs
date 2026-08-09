//! Focused executable coverage for the Wildfire utility completion wave.

use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardType, CostComponent, Keywords, Subtype, TargetSpec,
    CARD_DEFS, KERNEL_CARDDB_HASH,
};
use mtg_kernel::effect::{
    EffectObjectBinding, EffectOp, EffectTargetCandidate, EffectTargetSelectionPurpose,
    LibraryCardFilter, ObjectRef, PendingEffectChoice, PlayerRef,
};
use mtg_kernel::engine::{self, Action, Decision, UnsupportedMechanic};
use mtg_kernel::event::{self, CommittedEvent, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId, StackItemId};
use mtg_kernel::mana::{Cost, ManaColor, Pip};
use mtg_kernel::rl::{legal_action_candidates_v1, observe_v2, PendingEffectChoiceSemanticV4};
use mtg_kernel::state::{
    AbilitySourceContractV4, CastMethodV4, Counters, GameObject, GameState, ObjectStateV4,
    SpellCastOriginV4, SpellCastRouteV4, StackItem, StackItemKind, StackSourceContractV4,
    StackStateV4, StackTargetContractV4, Step, Target, Zone,
};
use mtg_kernel::surface_v2::{HarnessSurfaceV2, SurfaceDecision};
use mtg_kernel::trigger::{self, TriggerCondition};

fn card_id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name} in CARD_DEFS"))
}

fn card_name(card_def: u16) -> String {
    CARD_DEFS[card_def as usize].name.to_string()
}

fn ready_game(seed: u64) -> GameState {
    let mut state = GameState::new_from_libraries(&[], &[], card_name, seed);
    state.active_player = PlayerId::P0;
    state.priority_player = PlayerId::P0;
    state.step = Step::Main1;
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
    let object = state.objects.push(GameObject {
        card_def,
        name: CARD_DEFS[card_def as usize].object_name.to_string(),
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
        Zone::Hand => state.players[owner.index()].hand.push(object),
        Zone::Battlefield => state.players[owner.index()].battlefield.push(object),
        Zone::Library => state.players[owner.index()].library.push(object),
        Zone::Graveyard => state.players[owner.index()].graveyard.push(object),
        Zone::Exile => state.exile.push(object),
        Zone::Command => state.command.push(object),
        Zone::Stack => panic!("put_stack_spell owns stack insertion"),
    }
    object
}

fn put_stack_spell(
    state: &mut GameState,
    controller: PlayerId,
    name: &str,
    targets: Vec<Target>,
) -> ObjectId {
    let card_def = card_id(name);
    let mut object_v4 = ObjectStateV4::from_card_def(card_def);
    object_v4.spell_cast_origin = Some(SpellCastOriginV4 {
        origin_zone: Zone::Hand,
        origin_zone_change_count: 0,
        route: SpellCastRouteV4::Hand,
        finalized_method: Some(CastMethodV4::Normal),
    });
    let source = state.objects.push(GameObject {
        card_def,
        name: CARD_DEFS[card_def as usize].object_name.to_string(),
        owner: controller,
        controller,
        zone: Zone::Stack,
        tapped: false,
        summoning_sick: false,
        damage: 0,
        counters: Counters::default(),
        attachments: Vec::new(),
        v4: object_v4,
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 1,
    });
    let target_contracts = targets
        .iter()
        .copied()
        .map(|target| StackTargetContractV4::capture(state, target))
        .collect();
    state.engine.next_stack_item_id += 1;
    let stack_item_id = StackItemId(state.engine.next_stack_item_id);
    state.stack.push(StackItem {
        kind: StackItemKind::Spell,
        source,
        controller,
        targets,
        is_copy: false,
        inline_effect: None,
        discarded: Vec::new(),
        is_flashback: false,
        mode_chosen: 0,
        madness_offer: false,
        kicked: false,
        v4: StackStateV4 {
            stack_item_id,
            source_contract: Some(StackSourceContractV4::capture(
                state,
                source,
                CastMethodV4::Normal,
            )),
            target_spec: Some(CARD_DEFS[card_def as usize].target_spec),
            target_contracts,
            ..StackStateV4::spell(CastMethodV4::Normal)
        },
    });
    source
}

fn put_trigger_on_stack(
    state: &mut GameState,
    source: ObjectId,
    controller: PlayerId,
    effect: EffectOp,
    source_contract: AbilitySourceContractV4,
) -> StackItemId {
    state.engine.next_stack_item_id += 1;
    let stack_item_id = StackItemId(state.engine.next_stack_item_id);
    state.stack.push(StackItem {
        kind: StackItemKind::TriggeredAbility,
        source,
        controller,
        targets: Vec::new(),
        is_copy: false,
        inline_effect: Some(effect),
        discarded: Vec::new(),
        is_flashback: false,
        mode_chosen: 0,
        madness_offer: false,
        kicked: false,
        v4: StackStateV4 {
            stack_item_id,
            ability_source_contract: Some(source_contract),
            ..StackStateV4::default()
        },
    });
    stack_item_id
}

fn stack_contains(state: &GameState, stack_item_id: StackItemId) -> bool {
    state
        .stack
        .iter()
        .any(|item| item.v4.stack_item_id == stack_item_id)
}

fn resolve_one_item_until_choice(state: &mut GameState) -> Decision {
    let stack_item_id = state.stack.last().expect("stack item").v4.stack_item_id;
    for _ in 0..32 {
        let decision = engine::advance_until_decision(state);
        if !stack_contains(state, stack_item_id) {
            return decision;
        }
        match decision {
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => return other,
        }
    }
    panic!("stack item did not resolve within bounded priority passes")
}

fn resolve_top_without_choice(state: &mut GameState) {
    let decision = resolve_one_item_until_choice(state);
    assert!(
        matches!(decision, Decision::CastSpellOrPass { .. }),
        "unexpected decision after resolution: {decision:?}"
    );
}

fn action_candidates(
    state: &GameState,
    decision: &Decision,
) -> Vec<mtg_kernel::rl::LegalActionCandidateV1> {
    legal_action_candidates_v1(&SurfaceDecision::Decision(decision.clone()), state).unwrap()
}

#[test]
fn definitions_ids_and_generic_programs_are_exact() {
    assert_eq!(KERNEL_CARDDB_HASH, 0x5da6_ab41_0e1a_7686);
    for (name, expected_id) in [
        ("Cleansing Wildfire", 15),
        ("Duress", 24),
        ("Lembas", 65),
        ("Toxin Analysis", 121),
        ("Twisted Landscape", 123),
        ("Weather the Storm", 128),
        ("Clue Token", 160),
    ] {
        assert_eq!(card_id(name), expected_id, "append-only id for {name}");
        assert_eq!(
            CARD_DEFS[expected_id as usize].capability,
            CardCapability::Full,
            "{name} is executable"
        );
    }
    assert_eq!(CARD_DEFS.len(), 161);
    assert_eq!(Subtype::Clue.stable_id(), 68);
    assert_eq!(TargetSpec::Land.stable_id(), 34);

    let cleansing = &CARD_DEFS[card_id("Cleansing Wildfire") as usize];
    assert_eq!(cleansing.target_spec, TargetSpec::Land);
    assert_eq!(
        (cleansing.spell_effect)(),
        Some(EffectOp::Sequence(vec![
            EffectOp::DestroyTargetLandThenMaySearchBasicTapped {
                object: ObjectRef::Target(0),
            },
            EffectOp::DrawCards {
                player: PlayerRef::Controller,
                count: 1,
            },
        ]))
    );

    let duress = &CARD_DEFS[card_id("Duress") as usize];
    assert_eq!(duress.target_spec, TargetSpec::TargetOpponent);
    assert_eq!(
        (duress.spell_effect)(),
        Some(EffectOp::RevealTargetHandChooseNoncreatureNonlandDiscard {
            player: PlayerRef::Target(0),
        })
    );

    let toxin = &CARD_DEFS[card_id("Toxin Analysis") as usize];
    assert_eq!(toxin.target_spec, TargetSpec::Creature);
    assert_eq!(
        (toxin.spell_effect)(),
        Some(EffectOp::Sequence(vec![
            EffectOp::GrantKeywordTargetUntilEndOfTurn {
                object: ObjectRef::Target(0),
                keyword: Keywords::DEATHTOUCH | Keywords::LIFELINK,
            },
            EffectOp::CreateToken {
                token_def: card_id("Clue Token"),
                controller: PlayerRef::Controller,
            },
        ]))
    );

    let lembas = &CARD_DEFS[card_id("Lembas") as usize];
    assert_eq!(lembas.activated_abilities.len(), 1);
    assert_eq!(
        lembas.activated_abilities[0].cost,
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
    assert_eq!(
        (lembas.activated_abilities[0].effect)(),
        EffectOp::GainLife {
            player: PlayerRef::Controller,
            amount: 3,
        }
    );

    let landscape = &CARD_DEFS[card_id("Twisted Landscape") as usize];
    assert_eq!(landscape.produces_mana, &[ManaColor::C]);
    assert_eq!(landscape.activated_abilities.len(), 2);
    assert_eq!(
        landscape.activated_abilities[0].cost,
        &[CostComponent::Tap, CostComponent::SacrificeSelf]
    );
    assert_eq!(
        (landscape.activated_abilities[0].effect)(),
        EffectOp::SearchLibraryToBattlefieldTapped {
            player: PlayerRef::Controller,
            filter: LibraryCardFilter::BasicLandWithAnySubtype([
                Subtype::Swamp,
                Subtype::Mountain,
                Subtype::Forest,
            ]),
        }
    );
    assert_eq!(landscape.activated_abilities[1].activation_zone, Zone::Hand);
    assert_eq!(
        landscape.activated_abilities[1].cost,
        &[
            CostComponent::Mana(Cost {
                pips: &[
                    Pip::Colored(ManaColor::B),
                    Pip::Colored(ManaColor::R),
                    Pip::Colored(ManaColor::G),
                ],
                generic: 0,
                x_count: 0,
            }),
            CostComponent::DiscardSelf,
        ]
    );
    assert_eq!(
        (landscape.activated_abilities[1].effect)(),
        EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 1,
        }
    );

    let weather = &CARD_DEFS[card_id("Weather the Storm") as usize];
    assert_eq!(
        (weather.spell_effect)(),
        Some(EffectOp::GainLife {
            player: PlayerRef::Controller,
            amount: 3,
        })
    );
    assert_eq!(trigger::triggers_for(card_id("Weather the Storm")).len(), 1);
    assert_eq!(
        trigger::triggers_for(card_id("Weather the Storm"))[0].condition,
        TriggerCondition::CastSelf
    );
    assert_eq!(
        (trigger::triggers_for(card_id("Weather the Storm"))[0].effect)(),
        EffectOp::MaterializeStormCopies
    );

    let clue = &CARD_DEFS[card_id("Clue Token") as usize];
    assert!(clue.is_token && clue.has_type(CardType::Artifact));
    assert_eq!(clue.subtypes, &[Subtype::Clue]);
    assert_eq!(clue.activated_abilities.len(), 1);
    assert_eq!(
        clue.activated_abilities[0].cost,
        &[
            CostComponent::Mana(Cost {
                pips: &[],
                generic: 2,
                x_count: 0,
            }),
            CostComponent::SacrificeSelf,
        ]
    );
}

#[test]
fn cleansing_wildfire_destroys_then_privately_searches_tapped_and_draws() {
    let mut state = ready_game(0x434c_4541_4e01);
    let draw = put_object(
        &mut state,
        PlayerId::P0,
        PlayerId::P0,
        "Mountain",
        Zone::Library,
    );
    let basic = put_object(
        &mut state,
        PlayerId::P1,
        PlayerId::P1,
        "Forest",
        Zone::Library,
    );
    let miss = put_object(
        &mut state,
        PlayerId::P1,
        PlayerId::P1,
        "Counterspell",
        Zone::Library,
    );
    let target_land = put_object(
        &mut state,
        PlayerId::P1,
        PlayerId::P1,
        "Island",
        Zone::Battlefield,
    );
    let source = put_stack_spell(
        &mut state,
        PlayerId::P0,
        "Cleansing Wildfire",
        vec![Target::Object(target_land)],
    );
    observe_v2(&state, &HarnessSurfaceV2::new(), PlayerId::P0, 0)
        .expect("Cleansing stack item validates before resolution");

    let boolean = resolve_one_item_until_choice(&mut state);
    assert!(
        matches!(
            &boolean,
            Decision::ChooseEffectBoolean {
                player: PlayerId::P1,
                source: actual,
                default: Some(false),
                ..
            } if *actual == source
        ),
        "unexpected Cleansing choice: {boolean:?}; pending={:?}; target={:?}",
        state.engine.pending_effect,
        state.objects.get(target_land)
    );
    assert_eq!(state.objects.get(target_land).zone, Zone::Graveyard);
    assert_eq!(action_candidates(&state, &boolean).len(), 2);
    engine::step(&mut state, Action::ChooseEffectBoolean(true)).unwrap();

    let search = engine::advance_until_decision(&mut state);
    assert!(matches!(
        search,
        Decision::ChooseEffectTargets {
            player: PlayerId::P1,
            source: actual,
            ref legal_targets,
            min_targets: 0,
            max_targets: 1,
            can_finish: true,
            selected_count: 0,
        } if actual == source && legal_targets == &vec![Target::Object(basic)]
    ));
    assert!(!action_candidates(&state, &search).is_empty());
    let caster_observation = observe_v2(&state, &HarnessSurfaceV2::new(), PlayerId::P0, 0).unwrap();
    assert!(matches!(
        caster_observation
            .projection
            .engine_context
            .pending_effect
            .as_ref()
            .and_then(|pending| pending.choice.as_ref()),
        Some(PendingEffectChoiceSemanticV4::Targets {
            legal_targets,
            min_targets: 0,
            max_targets: 0,
            can_finish: true,
            ..
        }) if legal_targets.is_empty()
    ));
    assert_eq!(
        serde_json::from_str::<GameState>(&serde_json::to_string(&state).unwrap()).unwrap(),
        state
    );

    let mut tampered = state.clone();
    let Some(PendingEffectChoice::SelectTargets { legal, .. }) = tampered
        .engine
        .pending_effect
        .as_mut()
        .and_then(|pending| pending.choice.as_mut())
    else {
        unreachable!()
    };
    legal[0]
        .expected_object
        .as_mut()
        .unwrap()
        .expected_zone_change_count += 1;
    assert!(matches!(
        engine::advance_until_decision(&mut tampered),
        Decision::Halted {
            mechanic: UnsupportedMechanic::InvalidEffectContinuation,
            ..
        }
    ));

    engine::step(
        &mut state,
        Action::ChooseEffectTarget(Target::Object(basic)),
    )
    .unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    assert_eq!(state.objects.get(basic).zone, Zone::Battlefield);
    assert!(state.objects.get(basic).tapped);
    assert_eq!(state.objects.get(miss).zone, Zone::Library);
    assert_eq!(state.objects.get(draw).zone, Zone::Hand);
    assert_eq!(state.objects.get(source).zone, Zone::Graveyard);
}

#[test]
fn duress_reveals_the_exact_hand_and_only_discards_noncreature_nonland() {
    let mut state = ready_game(0x4455_5245_5353);
    let eligible = put_object(
        &mut state,
        PlayerId::P1,
        PlayerId::P1,
        "Counterspell",
        Zone::Hand,
    );
    let creature = put_object(
        &mut state,
        PlayerId::P1,
        PlayerId::P1,
        "Faerie Seer",
        Zone::Hand,
    );
    let land = put_object(&mut state, PlayerId::P1, PlayerId::P1, "Island", Zone::Hand);
    let source = put_stack_spell(
        &mut state,
        PlayerId::P0,
        "Duress",
        vec![Target::Player(PlayerId::P1)],
    );

    let choice = resolve_one_item_until_choice(&mut state);
    assert!(matches!(
        choice,
        Decision::ChooseEffectTargets {
            player: PlayerId::P0,
            source: actual,
            ref legal_targets,
            min_targets: 1,
            max_targets: 1,
            can_finish: false,
            selected_count: 0,
        } if actual == source && legal_targets == &vec![Target::Object(eligible)]
    ));
    assert_eq!(action_candidates(&state, &choice).len(), 1);
    for observer in [PlayerId::P0, PlayerId::P1] {
        let observation = observe_v2(&state, &HarnessSurfaceV2::new(), observer, 0).unwrap();
        assert!(matches!(
            observation
                .projection
                .engine_context
                .pending_effect
                .as_ref()
                .and_then(|pending| pending.choice.as_ref()),
            Some(PendingEffectChoiceSemanticV4::Targets {
                legal_targets,
                min_targets: 1,
                max_targets: 1,
                can_finish: false,
                ..
            }) if legal_targets.len() == 1
        ));
        let encoded = serde_json::to_string(&observation).unwrap();
        for card_def in [
            card_id("Counterspell"),
            card_id("Faerie Seer"),
            card_id("Island"),
        ] {
            assert!(encoded.contains(&format!("\"card_db_id\":{card_def}")));
        }
    }
    assert_eq!(
        serde_json::from_str::<GameState>(&serde_json::to_string(&state).unwrap()).unwrap(),
        state
    );

    let mut tampered = state.clone();
    let Some(PendingEffectChoice::SelectTargets { legal, .. }) = tampered
        .engine
        .pending_effect
        .as_mut()
        .and_then(|pending| pending.choice.as_mut())
    else {
        unreachable!()
    };
    legal[0]
        .expected_object
        .as_mut()
        .unwrap()
        .expected_zone_change_count += 1;
    assert!(matches!(
        engine::advance_until_decision(&mut tampered),
        Decision::Halted {
            mechanic: UnsupportedMechanic::InvalidEffectContinuation,
            ..
        }
    ));

    engine::step(
        &mut state,
        Action::ChooseEffectTarget(Target::Object(eligible)),
    )
    .unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    assert_eq!(state.objects.get(eligible).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(creature).zone, Zone::Hand);
    assert_eq!(state.objects.get(land).zone, Zone::Hand);
    assert_eq!(state.objects.get(source).zone, Zone::Graveyard);
}

#[test]
fn lembas_scries_then_draws_and_its_food_payment_uses_incarnation_bound_shuffle() {
    let mut etb = ready_game(0x4c45_4d42_4153_01);
    let top = put_object(
        &mut etb,
        PlayerId::P0,
        PlayerId::P0,
        "Mountain",
        Zone::Library,
    );
    let lembas = put_object(
        &mut etb,
        PlayerId::P0,
        PlayerId::P0,
        "Lembas",
        Zone::Battlefield,
    );
    let etb_source_contract = AbilitySourceContractV4::capture(&etb, lembas);
    put_trigger_on_stack(
        &mut etb,
        lembas,
        PlayerId::P0,
        (trigger::triggers_for(card_id("Lembas"))[0].effect)(),
        etb_source_contract,
    );
    let scry = resolve_one_item_until_choice(&mut etb);
    assert!(matches!(
        scry,
        Decision::ChooseEffectTargets {
            player: PlayerId::P0,
            source,
            ref legal_targets,
            min_targets: 0,
            max_targets: 1,
            can_finish: true,
            ..
        } if source == lembas && legal_targets == &vec![Target::Object(top)]
    ));
    engine::step(&mut etb, Action::FinishEffectSelection).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut etb),
        Decision::CastSpellOrPass { .. }
    ));
    assert_eq!(etb.objects.get(top).zone, Zone::Hand);

    let mut state = ready_game(0x4c45_4d42_4153_02);
    let filler = put_object(
        &mut state,
        PlayerId::P0,
        PlayerId::P0,
        "Forest",
        Zone::Library,
    );
    let lembas = put_object(
        &mut state,
        PlayerId::P0,
        PlayerId::P0,
        "Lembas",
        Zone::Battlefield,
    );
    state.players[0].life = 17;
    state.players[0].mana_pool[ManaColor::C.pool_index()] = 2;
    engine::step(&mut state, Action::ActivateAbility(lembas, 0)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    assert_eq!(state.objects.get(lembas).zone, Zone::Graveyard);
    assert_eq!(
        state.stack.len(),
        2,
        "leave trigger is above the Food ability"
    );
    let leave_trigger = state.stack.last().unwrap();
    assert!(matches!(
        leave_trigger.inline_effect,
        Some(EffectOp::ShuffleTriggerSourceIntoOwnersLibrary)
    ));
    assert!(matches!(
        leave_trigger.v4.ability_source_contract,
        Some(AbilitySourceContractV4 {
            source,
            zone: Zone::Battlefield,
            zone_change_count: 0,
            ..
        }) if source == lembas
    ));
    assert_eq!(
        serde_json::from_str::<GameState>(&serde_json::to_string(&state).unwrap()).unwrap(),
        state
    );

    let mut later_incarnation = state.clone();
    event::propose_and_commit(
        &mut later_incarnation,
        ProposedEvent::zone_change(lembas, Zone::Hand),
    );
    resolve_top_without_choice(&mut later_incarnation);
    assert_eq!(later_incarnation.objects.get(lembas).zone, Zone::Hand);
    assert!(!later_incarnation.players[0].library.contains(&lembas));

    resolve_top_without_choice(&mut state);
    assert_eq!(state.objects.get(lembas).zone, Zone::Library);
    assert!(state.players[0].library.contains(&lembas));
    assert!(state.players[0].library.contains(&filler));
    resolve_top_without_choice(&mut state);
    assert_eq!(state.players[0].life, 20);
}

#[test]
fn toxin_analysis_binds_keywords_and_investigates_with_an_exact_clue_payment() {
    let mut state = ready_game(0x544f_5849_4e01);
    let draw = put_object(
        &mut state,
        PlayerId::P0,
        PlayerId::P0,
        "Mountain",
        Zone::Library,
    );
    let creature = put_object(
        &mut state,
        PlayerId::P0,
        PlayerId::P0,
        "Faerie Seer",
        Zone::Battlefield,
    );
    let toxin = put_stack_spell(
        &mut state,
        PlayerId::P0,
        "Toxin Analysis",
        vec![Target::Object(creature)],
    );

    let mut stale = state.clone();
    event::propose_and_commit(&mut stale, ProposedEvent::zone_change(creature, Zone::Hand));
    resolve_top_without_choice(&mut stale);
    assert!(!stale.players[0]
        .battlefield
        .iter()
        .any(|object| { stale.objects.get(*object).card_def == card_id("Clue Token") }));

    resolve_top_without_choice(&mut state);
    assert_eq!(state.objects.get(toxin).zone, Zone::Graveyard);
    assert!(engine::has_effective_keyword(
        &state,
        creature,
        Keywords::DEATHTOUCH
    ));
    assert!(engine::has_effective_keyword(
        &state,
        creature,
        Keywords::LIFELINK
    ));
    let clues = state.players[0]
        .battlefield
        .iter()
        .copied()
        .filter(|object| state.objects.get(*object).card_def == card_id("Clue Token"))
        .collect::<Vec<_>>();
    assert_eq!(clues.len(), 1);
    let clue = clues[0];

    let life_before = state.players[0].life;
    event::propose_and_commit(
        &mut state,
        ProposedEvent::damage(creature, Target::Player(PlayerId::P1), 1),
    );
    assert_eq!(state.players[0].life, life_before + 1);
    assert_eq!(state.players[1].life, 19);

    state.objects.get_mut(clue).tapped = true;
    state.players[0].mana_pool[ManaColor::C.pool_index()] = 2;
    engine::step(&mut state, Action::ActivateAbility(clue, 0)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    assert!(!state.players[0].battlefield.contains(&clue));
    resolve_top_without_choice(&mut state);
    assert_eq!(state.objects.get(draw).zone, Zone::Hand);
}

#[test]
fn twisted_landscape_makes_colorless_searches_exact_basic_types_tapped_and_cycles_brg() {
    let mut mana_state = ready_game(0x5457_4953_5401);
    let mana_land = put_object(
        &mut mana_state,
        PlayerId::P0,
        PlayerId::P0,
        "Twisted Landscape",
        Zone::Battlefield,
    );
    engine::step(&mut mana_state, Action::ActivateManaAbility(mana_land)).unwrap();
    assert!(mana_state.objects.get(mana_land).tapped);
    assert_eq!(
        mana_state.players[0].mana_pool[ManaColor::C.pool_index()],
        1
    );

    let mut search_state = ready_game(0x5457_4953_5402);
    let forest = put_object(
        &mut search_state,
        PlayerId::P0,
        PlayerId::P0,
        "Forest",
        Zone::Library,
    );
    let island = put_object(
        &mut search_state,
        PlayerId::P0,
        PlayerId::P0,
        "Island",
        Zone::Library,
    );
    let search_land = put_object(
        &mut search_state,
        PlayerId::P0,
        PlayerId::P0,
        "Twisted Landscape",
        Zone::Battlefield,
    );
    engine::step(&mut search_state, Action::ActivateAbility(search_land, 0)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut search_state),
        Decision::CastSpellOrPass { .. }
    ));
    assert_eq!(search_state.objects.get(search_land).zone, Zone::Graveyard);
    let search = resolve_one_item_until_choice(&mut search_state);
    assert!(matches!(
        search,
        Decision::ChooseEffectTargets {
            player: PlayerId::P0,
            source,
            ref legal_targets,
            min_targets: 0,
            max_targets: 1,
            can_finish: true,
            selected_count: 0,
        } if source == search_land && legal_targets == &vec![Target::Object(forest)]
    ));
    let opponent_observation =
        observe_v2(&search_state, &HarnessSurfaceV2::new(), PlayerId::P1, 0).unwrap();
    assert!(matches!(
        opponent_observation
            .projection
            .engine_context
            .pending_effect
            .as_ref()
            .and_then(|pending| pending.choice.as_ref()),
        Some(PendingEffectChoiceSemanticV4::Targets {
            legal_targets,
            min_targets: 0,
            max_targets: 0,
            can_finish: true,
            ..
        }) if legal_targets.is_empty()
    ));

    let mut filter_tamper = search_state.clone();
    let Some(PendingEffectChoice::SelectTargets { legal, purpose, .. }) = filter_tamper
        .engine
        .pending_effect
        .as_mut()
        .and_then(|pending| pending.choice.as_mut())
    else {
        unreachable!()
    };
    let EffectTargetSelectionPurpose::SearchLibraryToBattlefieldTapped {
        filter,
        filter_fingerprint,
        ..
    } = purpose
    else {
        unreachable!()
    };
    *filter = LibraryCardFilter::BasicLand;
    *filter_fingerprint =
        1_u64
            .to_le_bytes()
            .into_iter()
            .fold(0xcbf2_9ce4_8422_2325, |mut hash, byte| {
                hash ^= u64::from(byte);
                hash.wrapping_mul(0x0000_0100_0000_01b3)
            });
    legal.push(EffectTargetCandidate {
        target: Target::Object(island),
        expected_object: Some(EffectObjectBinding {
            object: island,
            expected_zone: Zone::Library,
            expected_zone_change_count: search_state.objects.get(island).zone_change_count,
        }),
    });
    assert!(matches!(
        engine::advance_until_decision(&mut filter_tamper),
        Decision::Halted {
            mechanic: UnsupportedMechanic::InvalidEffectContinuation,
            ..
        }
    ));

    engine::step(
        &mut search_state,
        Action::ChooseEffectTarget(Target::Object(forest)),
    )
    .unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut search_state),
        Decision::CastSpellOrPass { .. }
    ));
    assert_eq!(search_state.objects.get(forest).zone, Zone::Battlefield);
    assert!(search_state.objects.get(forest).tapped);
    assert_eq!(search_state.objects.get(island).zone, Zone::Library);

    let mut cycling_state = ready_game(0x5457_4953_5403);
    let draw = put_object(
        &mut cycling_state,
        PlayerId::P0,
        PlayerId::P0,
        "Mountain",
        Zone::Library,
    );
    let cycling_land = put_object(
        &mut cycling_state,
        PlayerId::P0,
        PlayerId::P0,
        "Twisted Landscape",
        Zone::Hand,
    );
    cycling_state.players[0].mana_pool[ManaColor::B.pool_index()] = 1;
    cycling_state.players[0].mana_pool[ManaColor::R.pool_index()] = 1;
    cycling_state.players[0].mana_pool[ManaColor::G.pool_index()] = 1;
    engine::step(&mut cycling_state, Action::ActivateAbility(cycling_land, 1)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut cycling_state),
        Decision::CastSpellOrPass { .. }
    ));
    assert_eq!(
        cycling_state.objects.get(cycling_land).zone,
        Zone::Graveyard
    );
    assert_eq!(cycling_state.players[0].mana_pool, [0; 6]);
    resolve_top_without_choice(&mut cycling_state);
    assert_eq!(cycling_state.objects.get(draw).zone, Zone::Hand);
}

#[test]
fn weather_storm_is_cross_player_frozen_survives_countering_and_rejects_tamper() {
    let mut state = ready_game(0x5354_4f52_4d01);
    let prior_p0 = put_object(
        &mut state,
        PlayerId::P0,
        PlayerId::P0,
        "Mental Note",
        Zone::Graveyard,
    );
    let prior_p1 = put_object(
        &mut state,
        PlayerId::P1,
        PlayerId::P1,
        "Mental Note",
        Zone::Graveyard,
    );
    state.players[0].spells_cast_this_turn = 1;
    state.players[1].spells_cast_this_turn = 1;
    state.engine.event_history.extend([
        CommittedEvent::SpellCast {
            spell: prior_p0,
            controller: PlayerId::P0,
        },
        CommittedEvent::SpellCast {
            spell: prior_p1,
            controller: PlayerId::P1,
        },
    ]);
    state.players[0].mana_pool[ManaColor::G.pool_index()] = 1;
    state.players[0].mana_pool[ManaColor::C.pool_index()] = 1;
    let weather = put_object(
        &mut state,
        PlayerId::P0,
        PlayerId::P0,
        "Weather the Storm",
        Zone::Hand,
    );
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { ref castable_spells, .. } if castable_spells.contains(&weather)
    ));
    engine::step(&mut state, Action::CastSpell(weather)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    assert_eq!(state.players[0].spells_cast_this_turn, 2);
    assert_eq!(state.stack.len(), 2, "spell plus its Storm trigger");
    let storm_trigger_id = state.stack.last().unwrap().v4.stack_item_id;
    let frozen = match state.stack.last().unwrap().inline_effect.as_ref().unwrap() {
        EffectOp::CreateStormCopies { binding } => *binding,
        other => panic!("expected bound Storm trigger, got {other:?}"),
    };
    assert_eq!(frozen.casts_after_source, [2, 1]);
    assert_eq!(
        serde_json::from_str::<GameState>(&serde_json::to_string(&state).unwrap()).unwrap(),
        state
    );

    let observed = observe_v2(&state, &HarnessSurfaceV2::new(), PlayerId::P1, 0).unwrap();
    assert_eq!(observed.projection.stack.len(), 2);

    let mut tampered = state.clone();
    let EffectOp::CreateStormCopies { binding } = tampered
        .stack
        .last_mut()
        .unwrap()
        .inline_effect
        .as_mut()
        .unwrap()
    else {
        unreachable!()
    };
    binding.casts_after_source[0] -= 1;
    assert!(matches!(
        resolve_one_item_until_choice(&mut tampered),
        Decision::Halted {
            mechanic: UnsupportedMechanic::InvalidEffectContinuation,
            ..
        }
    ));

    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(weather, Zone::Graveyard),
    );
    assert!(stack_contains(&state, storm_trigger_id));
    assert!(!state
        .stack
        .iter()
        .any(|item| item.kind == StackItemKind::Spell && item.source == weather));
    let spell_cast_events_before = state
        .engine
        .event_history
        .iter()
        .filter(|event| matches!(event, CommittedEvent::SpellCast { .. }))
        .count();
    resolve_top_without_choice(&mut state);
    let copies = state
        .stack
        .iter()
        .filter(|item| item.kind == StackItemKind::Spell && item.is_copy)
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(copies.len(), 2);
    assert!(copies.iter().all(|copy| {
        copy.source != weather
            && copy.controller == PlayerId::P0
            && copy.targets.is_empty()
            && copy.v4.source_contract.is_some_and(|contract| {
                contract
                    .spell_copy_origin
                    .is_some_and(|origin| origin.parent == weather && !origin.parent_was_copy)
            })
    }));
    assert_eq!(
        state
            .engine
            .event_history
            .iter()
            .filter(|event| matches!(event, CommittedEvent::SpellCast { .. }))
            .count(),
        spell_cast_events_before,
        "Storm copies are not casts"
    );
    assert_eq!(state.players[0].spells_cast_this_turn, 2);
    assert_eq!(state.players[1].spells_cast_this_turn, 1);
    observe_v2(&state, &HarnessSurfaceV2::new(), PlayerId::P0, 1)
        .expect("Storm copies project as valid stack spells");

    let life_before = state.players[0].life;
    resolve_top_without_choice(&mut state);
    resolve_top_without_choice(&mut state);
    assert_eq!(state.players[0].life, life_before + 6);
    assert!(copies.iter().all(|copy| {
        !state.players[0].graveyard.contains(&copy.source)
            && !state.players[1].graveyard.contains(&copy.source)
    }));
}
