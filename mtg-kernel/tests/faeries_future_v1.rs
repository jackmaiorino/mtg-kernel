//! Focused executable coverage for the Mono-Blue Faeries future wave.

use mtg_kernel::card_def::{
    card_id_by_name, AttachmentDef, CardCapability, CardType, CostComponent, DynamicValueDef,
    Keywords, Subtype, TargetSpec, CARD_DEFS, KERNEL_CARDDB_HASH,
};
use mtg_kernel::effect::{EffectOp, ObjectRef, PendingEffectChoice, PlayerRef, TargetRef};
use mtg_kernel::engine::{self, Action, CostKind, Decision, UnsupportedMechanic};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId, StackItemId};
use mtg_kernel::mana::{Cost, ManaColor, Pip};
use mtg_kernel::rl::{
    legal_action_candidates_v1, observe_v2, PendingEffectChoiceSemanticV4, TargetSelectionPurposeV4,
};
use mtg_kernel::state::{
    CastMethodV4, Counters, GameObject, GameState, ObjectStateV4, SpellCastOriginV4,
    SpellCastRouteV4, StackItem, StackItemKind, StackSourceContractV4, StackStateV4, Step, Target,
    Zone,
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
        Zone::Stack => panic!("put_spell_on_stack owns stack insertion"),
    }
    object
}

fn put_spell_on_stack(state: &mut GameState, controller: PlayerId, name: &str) -> ObjectId {
    let card_def = card_id(name);
    let mut object_v4 = ObjectStateV4::from_card_def(card_def);
    object_v4.spell_cast_origin = Some(SpellCastOriginV4 {
        origin_zone: Zone::Hand,
        origin_zone_change_count: 0,
        route: SpellCastRouteV4::Hand,
        finalized_method: Some(CastMethodV4::Normal),
    });
    let object = state.objects.push(GameObject {
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
    state.engine.next_stack_item_id += 1;
    let stack_item_id = StackItemId(state.engine.next_stack_item_id);
    state.stack.push(StackItem {
        kind: StackItemKind::Spell,
        source: object,
        controller,
        targets: Vec::new(),
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
                object,
                CastMethodV4::Normal,
            )),
            target_spec: Some(CARD_DEFS[card_def as usize].target_spec),
            ..StackStateV4::spell(CastMethodV4::Normal)
        },
    });
    object
}

fn enter_from_hand(state: &mut GameState, player: PlayerId, name: &str) -> ObjectId {
    let object = put_object(state, player, player, name, Zone::Hand);
    event::propose_and_commit(state, ProposedEvent::zone_change(object, Zone::Battlefield));
    object
}

fn cast_targeted_spell(
    state: &mut GameState,
    player: PlayerId,
    name: &str,
    target: ObjectId,
    blue_mana: u8,
) -> ObjectId {
    state.players[player.index()].mana_pool[ManaColor::U.pool_index()] = blue_mana;
    let spell = put_object(state, player, player, name, Zone::Hand);
    engine::step(state, Action::CastSpell(spell)).unwrap();
    let decision = engine::advance_until_decision(state);
    assert!(matches!(
        decision,
        Decision::ChooseTargets {
            player: chooser,
            spell: source,
            ref legal_targets,
            ..
        } if chooser == player && source == spell && legal_targets.contains(&Target::Object(target))
    ));
    engine::step(state, Action::ChooseTarget(Target::Object(target))).unwrap();
    spell
}

fn stack_contains(state: &GameState, stack_item_id: StackItemId) -> bool {
    state
        .stack
        .iter()
        .any(|item| item.v4.stack_item_id == stack_item_id)
}

fn resolve_one_item_until_choice(state: &mut GameState) -> Decision {
    let stack_item_id = state.stack.last().expect("stack item").v4.stack_item_id;
    for _ in 0..16 {
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
    assert!(matches!(decision, Decision::CastSpellOrPass { .. }));
}

#[test]
fn definitions_ids_keywords_programs_and_target_grammars_are_exact() {
    assert_eq!(KERNEL_CARDDB_HASH, 0x64c8_2a26_1e07_8f1a);
    for (name, expected_id) in [
        ("Bind the Monster", 4),
        ("Harrier Strix", 52),
        ("Humbling Elder", 55),
        ("Moon-Circuit Hacker", 75),
        ("Ninja of the Deep Hours", 80),
        ("Saiba Cryptomancer", 99),
        ("Snap", 106),
        ("Spellstutter Sprite", 110),
    ] {
        assert_eq!(card_id(name), expected_id, "append-only id for {name}");
        assert_eq!(
            CARD_DEFS[expected_id as usize].capability,
            CardCapability::Full,
            "{name} is executable"
        );
    }

    let bind = &CARD_DEFS[card_id("Bind the Monster") as usize];
    assert_eq!(bind.types, &[CardType::Enchantment]);
    assert_eq!(bind.target_spec, TargetSpec::Creature);
    assert_eq!(
        bind.attachment,
        Some(AttachmentDef::AuraCreature {
            prevents_untap: true
        })
    );
    assert_eq!(
        (bind.spell_effect)(),
        Some(EffectOp::PutSourceOntoBattlefieldAttachedToTarget {
            target: ObjectRef::Target(0)
        })
    );

    let harrier = &CARD_DEFS[card_id("Harrier Strix") as usize];
    assert!(harrier.keywords.has(Keywords::FLYING));
    assert_eq!(harrier.activated_abilities.len(), 1);
    assert_eq!(
        harrier.activated_abilities[0].cost,
        &[CostComponent::Mana(Cost {
            pips: &[Pip::Colored(ManaColor::U)],
            generic: 2,
            x_count: 0,
        })]
    );
    assert_eq!(
        (harrier.activated_abilities[0].effect)(),
        EffectOp::Sequence(vec![
            EffectOp::DrawCards {
                player: PlayerRef::Controller,
                count: 1,
            },
            EffectOp::DiscardCards {
                player: PlayerRef::Controller,
                count: 1,
            },
        ])
    );

    let humbling = &CARD_DEFS[card_id("Humbling Elder") as usize];
    assert!(humbling.keywords.has(Keywords::FLASH));
    assert_eq!(
        trigger::trigger_target_spec(card_id("Humbling Elder")),
        TargetSpec::OpponentControlledCreature
    );
    assert_eq!(
        (trigger::triggers_for(card_id("Humbling Elder"))[0].effect)(),
        EffectOp::PumpTargetUntilEndOfTurnDynamic {
            target: TargetRef::Target(0),
            power: DynamicValueDef::Fixed(-2),
            toughness: DynamicValueDef::Fixed(0),
        }
    );

    for name in ["Moon-Circuit Hacker", "Ninja of the Deep Hours"] {
        let definition = &CARD_DEFS[card_id(name) as usize];
        assert_eq!(definition.activated_abilities.len(), 1);
        assert_eq!(
            definition.activated_abilities[0].activation_zone,
            Zone::Hand
        );
        assert!(matches!(
            definition.activated_abilities[0].cost,
            [
                CostComponent::Mana(_),
                CostComponent::ReturnControlledUnblockedAttackerToOwnersHand
            ]
        ));
        assert_eq!(
            (definition.activated_abilities[0].effect)(),
            EffectOp::PutSourceOntoBattlefieldTappedAndAttacking
        );
        assert_eq!(
            trigger::triggers_for(card_id(name))[0].condition,
            TriggerCondition::DealsCombatDamageToPlayer
        );
    }

    let saiba = &CARD_DEFS[card_id("Saiba Cryptomancer") as usize];
    assert!(saiba.keywords.has(Keywords::FLASH));
    assert!(saiba.keywords.has(Keywords::HEXPROOF));
    assert_eq!(
        (trigger::triggers_for(card_id("Saiba Cryptomancer"))[0].effect)(),
        EffectOp::BackupTarget {
            target: ObjectRef::Target(0),
            keyword: Keywords::HEXPROOF,
        }
    );

    let snap = &CARD_DEFS[card_id("Snap") as usize];
    assert_eq!(snap.target_spec, TargetSpec::Creature);
    assert_eq!(
        (snap.spell_effect)(),
        Some(EffectOp::Sequence(vec![
            EffectOp::MoveObject {
                object: ObjectRef::Target(0),
                to_zone: Zone::Hand,
            },
            EffectOp::UntapUpToLands {
                chooser: PlayerRef::Controller,
                max_targets: 2,
            },
        ]))
    );

    let sprite = &CARD_DEFS[card_id("Spellstutter Sprite") as usize];
    assert!(sprite.keywords.has(Keywords::FLASH));
    assert!(sprite.keywords.has(Keywords::FLYING));
    assert_eq!(
        trigger::trigger_target_spec(card_id("Spellstutter Sprite")),
        TargetSpec::SpellManaValueAtMostControlledSubtypes {
            first: Subtype::Faerie,
            second: Some(Subtype::FaerieAllCaps),
        }
    );
}

#[test]
fn flash_and_targeted_etb_triggers_use_controller_aware_legal_targets() {
    let mut timing = ready_game(0x4641_4552_4945_5301);
    timing.active_player = PlayerId::P1;
    timing.priority_player = PlayerId::P0;
    timing.step = Step::End;
    timing.players[0].mana_pool[ManaColor::U.pool_index()] = 4;
    let harrier = put_object(
        &mut timing,
        PlayerId::P0,
        PlayerId::P0,
        "Harrier Strix",
        Zone::Hand,
    );
    let humbling = put_object(
        &mut timing,
        PlayerId::P0,
        PlayerId::P0,
        "Humbling Elder",
        Zone::Hand,
    );
    let saiba = put_object(
        &mut timing,
        PlayerId::P0,
        PlayerId::P0,
        "Saiba Cryptomancer",
        Zone::Hand,
    );
    let sprite = put_object(
        &mut timing,
        PlayerId::P0,
        PlayerId::P0,
        "Spellstutter Sprite",
        Zone::Hand,
    );
    let Decision::CastSpellOrPass {
        castable_spells, ..
    } = engine::advance_until_decision(&mut timing)
    else {
        panic!("priority decision")
    };
    assert!(!castable_spells.contains(&harrier));
    assert!(castable_spells.contains(&humbling));
    assert!(castable_spells.contains(&saiba));
    assert!(castable_spells.contains(&sprite));

    let mut harrier_state = ready_game(0x4641_4552_4945_5302);
    let land = put_object(
        &mut harrier_state,
        PlayerId::P1,
        PlayerId::P1,
        "Island",
        Zone::Battlefield,
    );
    let hexproof = put_object(
        &mut harrier_state,
        PlayerId::P1,
        PlayerId::P1,
        "Saiba Cryptomancer",
        Zone::Battlefield,
    );
    let harrier = enter_from_hand(&mut harrier_state, PlayerId::P0, "Harrier Strix");
    let decision = engine::advance_until_decision(&mut harrier_state);
    let Decision::ChooseTargets {
        player,
        spell,
        legal_targets,
        ..
    } = decision.clone()
    else {
        panic!("Harrier ETB target")
    };
    assert_eq!((player, spell), (PlayerId::P0, harrier));
    assert!(legal_targets.contains(&Target::Object(land)));
    assert!(!legal_targets.contains(&Target::Object(hexproof)));
    let actions =
        legal_action_candidates_v1(&SurfaceDecision::Decision(decision.clone()), &harrier_state)
            .unwrap();
    assert_eq!(actions.len(), legal_targets.len());
    let restored: GameState =
        serde_json::from_str(&serde_json::to_string(&harrier_state).unwrap()).unwrap();
    assert_eq!(restored, harrier_state);
    engine::step(
        &mut harrier_state,
        Action::ChooseTarget(Target::Object(land)),
    )
    .unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut harrier_state),
        Decision::CastSpellOrPass { .. }
    ));
    resolve_top_without_choice(&mut harrier_state);
    assert!(harrier_state.objects.get(land).tapped);

    let mut humbling_state = ready_game(0x4641_4552_4945_5303);
    let own = put_object(
        &mut humbling_state,
        PlayerId::P0,
        PlayerId::P0,
        "Guttersnipe",
        Zone::Battlefield,
    );
    let enemy = put_object(
        &mut humbling_state,
        PlayerId::P1,
        PlayerId::P1,
        "Guttersnipe",
        Zone::Battlefield,
    );
    let protected = put_object(
        &mut humbling_state,
        PlayerId::P1,
        PlayerId::P1,
        "Saiba Cryptomancer",
        Zone::Battlefield,
    );
    let elder = enter_from_hand(&mut humbling_state, PlayerId::P0, "Humbling Elder");
    let Decision::ChooseTargets {
        spell,
        legal_targets,
        ..
    } = engine::advance_until_decision(&mut humbling_state)
    else {
        panic!("Humbling Elder ETB target")
    };
    assert_eq!(spell, elder);
    assert_eq!(legal_targets, vec![Target::Object(enemy)]);
    assert!(!legal_targets.contains(&Target::Object(own)));
    assert!(!legal_targets.contains(&Target::Object(protected)));
    engine::step(
        &mut humbling_state,
        Action::ChooseTarget(Target::Object(enemy)),
    )
    .unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut humbling_state),
        Decision::CastSpellOrPass { .. }
    ));
    resolve_top_without_choice(&mut humbling_state);
    assert_eq!(engine::effective_power(&humbling_state, enemy), 0);
    assert_eq!(engine::effective_toughness(&humbling_state, enemy), 2);

    let mut saiba_state = ready_game(0x4641_4552_4945_5304);
    let ally = put_object(
        &mut saiba_state,
        PlayerId::P0,
        PlayerId::P0,
        "Faerie Seer",
        Zone::Battlefield,
    );
    let saiba = enter_from_hand(&mut saiba_state, PlayerId::P0, "Saiba Cryptomancer");
    let Decision::ChooseTargets { legal_targets, .. } =
        engine::advance_until_decision(&mut saiba_state)
    else {
        panic!("Saiba ETB target")
    };
    assert!(legal_targets.contains(&Target::Object(ally)));
    assert!(legal_targets.contains(&Target::Object(saiba)));
    engine::step(&mut saiba_state, Action::ChooseTarget(Target::Object(ally))).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut saiba_state),
        Decision::CastSpellOrPass { .. }
    ));
    resolve_top_without_choice(&mut saiba_state);
    assert_eq!(saiba_state.objects.get(ally).counters.plus1_plus1, 1);
    assert!(engine::has_effective_keyword(
        &saiba_state,
        ally,
        Keywords::HEXPROOF
    ));
}

#[test]
fn an_old_etb_trigger_and_a_recast_spell_can_share_one_physical_source() {
    let mut state = ready_game(0x4641_4552_4945_5311);
    let ally = put_object(
        &mut state,
        PlayerId::P0,
        PlayerId::P0,
        "Faerie Seer",
        Zone::Battlefield,
    );
    let saiba = enter_from_hand(&mut state, PlayerId::P0, "Saiba Cryptomancer");
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::ChooseTargets { .. }
    ));
    engine::step(&mut state, Action::ChooseTarget(Target::Object(ally))).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    assert_eq!(state.stack.len(), 1);
    let old_trigger_id = state.stack[0].v4.stack_item_id;
    let old_generation = state.stack[0]
        .v4
        .ability_source_contract
        .expect("old Saiba trigger source")
        .zone_change_count;

    event::propose_and_commit(&mut state, ProposedEvent::zone_change(saiba, Zone::Hand));
    state.players[0].mana_pool[ManaColor::U.pool_index()] = 2;
    engine::step(&mut state, Action::CastSpell(saiba)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    assert_eq!(state.stack.len(), 2);
    assert_eq!(state.stack[0].v4.stack_item_id, old_trigger_id);
    assert_eq!(state.stack[0].source, saiba);
    assert_eq!(state.stack[1].source, saiba);
    let new_spell_generation = state.stack[1]
        .v4
        .source_contract
        .expect("recast Saiba spell source")
        .zone_change_count;
    assert!(new_spell_generation > old_generation);

    let serialized = serde_json::to_string(&state).unwrap();
    state = serde_json::from_str(&serialized).unwrap();
    let observed = observe_v2(&state, &HarnessSurfaceV2::new(), PlayerId::P1, 0).unwrap();
    assert_eq!(observed.projection.stack.len(), 2);
    assert_eq!(
        observed.projection.stack[0].source.arena_id,
        observed.projection.stack[1].source.arena_id
    );
    assert_eq!(
        observed.projection.stack[0].source.zone_change_count,
        old_generation
    );
    assert_eq!(
        observed.projection.stack[1].source.zone_change_count,
        new_spell_generation
    );

    let new_etb_decision = resolve_one_item_until_choice(&mut state);
    assert!(matches!(new_etb_decision, Decision::ChooseTargets { .. }));
    assert!(stack_contains(&state, old_trigger_id));
    assert_eq!(state.objects.get(saiba).zone, Zone::Battlefield);
    engine::step(&mut state, Action::ChooseTarget(Target::Object(ally))).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    assert_eq!(state.stack.len(), 2, "old and new ETB triggers coexist");
    assert!(stack_contains(&state, old_trigger_id));

    resolve_top_without_choice(&mut state);
    assert!(stack_contains(&state, old_trigger_id));
    resolve_top_without_choice(&mut state);
    assert_eq!(state.objects.get(ally).counters.plus1_plus1, 2);
    assert!(state.stack.is_empty());
}

#[test]
fn spellstutter_counts_both_faerie_subtypes_and_rechecks_at_resolution() {
    let mut state = ready_game(0x4641_4552_4945_5305);
    let one = put_spell_on_stack(&mut state, PlayerId::P1, "Ponder");
    let two = put_spell_on_stack(&mut state, PlayerId::P1, "Snap");
    let _seer = put_object(
        &mut state,
        PlayerId::P0,
        PlayerId::P0,
        "Faerie Seer",
        Zone::Battlefield,
    );
    let sprite = enter_from_hand(&mut state, PlayerId::P0, "Spellstutter Sprite");
    let Decision::ChooseTargets {
        spell,
        legal_targets,
        ..
    } = engine::advance_until_decision(&mut state)
    else {
        panic!("Spellstutter ETB target")
    };
    assert_eq!(spell, sprite);
    assert_eq!(
        legal_targets,
        vec![Target::Object(one), Target::Object(two)]
    );
    engine::step(&mut state, Action::ChooseTarget(Target::Object(two))).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    resolve_top_without_choice(&mut state);
    assert_eq!(state.objects.get(two).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(one).zone, Zone::Stack);

    let mut shrinks = ready_game(0x4641_4552_4945_5306);
    let target = put_spell_on_stack(&mut shrinks, PlayerId::P1, "Snap");
    let seer = put_object(
        &mut shrinks,
        PlayerId::P0,
        PlayerId::P0,
        "Faerie Seer",
        Zone::Battlefield,
    );
    enter_from_hand(&mut shrinks, PlayerId::P0, "Spellstutter Sprite");
    assert!(matches!(
        engine::advance_until_decision(&mut shrinks),
        Decision::ChooseTargets { .. }
    ));
    engine::step(&mut shrinks, Action::ChooseTarget(Target::Object(target))).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut shrinks),
        Decision::CastSpellOrPass { .. }
    ));
    event::propose_and_commit(
        &mut shrinks,
        ProposedEvent::zone_change(seer, Zone::Graveyard),
    );
    resolve_top_without_choice(&mut shrinks);
    assert_eq!(
        shrinks.objects.get(target).zone,
        Zone::Stack,
        "mana-value condition is live when the trigger resolves"
    );
}

#[test]
fn ninjutsu_is_post_blockers_only_pays_an_exact_attacker_and_enters_attacking() {
    let mut state = ready_game(0x4641_4552_4945_5307);
    state.step = Step::DeclareBlockers;
    state.engine.combat.attackers_declared = true;
    state.engine.combat.blockers_declared = true;
    let first = put_object(
        &mut state,
        PlayerId::P0,
        PlayerId::P0,
        "Faerie Seer",
        Zone::Battlefield,
    );
    let second = put_object(
        &mut state,
        PlayerId::P0,
        PlayerId::P0,
        "Harrier Strix",
        Zone::Battlefield,
    );
    state.objects.get_mut(first).tapped = true;
    state.objects.get_mut(second).tapped = true;
    state.engine.combat.attackers = vec![first, second];
    let ninja = put_object(
        &mut state,
        PlayerId::P0,
        PlayerId::P0,
        "Ninja of the Deep Hours",
        Zone::Hand,
    );
    state.players[0].mana_pool[ManaColor::U.pool_index()] = 2;

    let mut too_early = state.clone();
    too_early.engine.combat.blockers_declared = false;
    too_early.step = Step::Main1;
    let Decision::CastSpellOrPass {
        activatable_abilities,
        ..
    } = engine::advance_until_decision(&mut too_early)
    else {
        panic!("priority decision")
    };
    assert!(!activatable_abilities.contains(&(ninja, 0)));

    let Decision::CastSpellOrPass {
        activatable_abilities,
        ..
    } = engine::advance_until_decision(&mut state)
    else {
        panic!("priority decision")
    };
    assert!(activatable_abilities.contains(&(ninja, 0)));
    engine::step(&mut state, Action::ActivateAbility(ninja, 0)).unwrap();
    let Decision::ChooseCostTargets {
        player,
        source,
        cost_kind,
        remaining,
        candidates,
    } = engine::advance_until_decision(&mut state)
    else {
        panic!("ninjutsu return cost")
    };
    assert_eq!(
        (player, source, cost_kind, remaining),
        (PlayerId::P0, ninja, CostKind::ReturnPermanentsToHand, 1)
    );
    assert_eq!(candidates, vec![first, second]);
    engine::step(&mut state, Action::ChooseCostTarget(first)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    assert_eq!(state.objects.get(first).zone, Zone::Hand);
    assert_eq!(state.objects.get(ninja).zone, Zone::Hand);
    let activation = state.stack.last().expect("ninjutsu stack item");
    assert_eq!(activation.kind, StackItemKind::ActivatedAbility);
    assert_eq!(activation.source, ninja);
    assert_eq!(
        activation
            .v4
            .hidden_ability_source
            .unwrap()
            .zone_change_count,
        state.objects.get(ninja).zone_change_count
    );
    let nonowner = observe_v2(&state, &HarnessSurfaceV2::new(), PlayerId::P1, 0).unwrap();
    let encoded = serde_json::to_string(&nonowner).unwrap();
    assert!(encoded.contains("Ninja of the Deep Hours"));

    let mut departed = state.clone();
    event::propose_and_commit(
        &mut departed,
        ProposedEvent::zone_change(ninja, Zone::Graveyard),
    );
    let departed_decision = resolve_one_item_until_choice(&mut departed);
    assert!(matches!(
        departed_decision,
        Decision::CastSpellOrPass { .. }
    ));
    assert_eq!(departed.objects.get(ninja).zone, Zone::Graveyard);
    assert!(departed.engine.halted.is_none());

    let mut tampered = state.clone();
    tampered
        .stack
        .last_mut()
        .expect("ninjutsu stack item")
        .v4
        .hidden_ability_source
        .as_mut()
        .expect("ninjutsu source contract")
        .zone_change_count += 1;
    let tampered_decision = resolve_one_item_until_choice(&mut tampered);
    assert!(matches!(
        &tampered_decision,
        Decision::Halted {
            mechanic: UnsupportedMechanic::InvalidEffectContinuation,
            source,
        } if *source == ninja
    ));

    resolve_top_without_choice(&mut state);
    assert_eq!(state.objects.get(ninja).zone, Zone::Battlefield);
    assert!(state.objects.get(ninja).tapped);
    assert!(state.engine.combat.attackers.contains(&ninja));
}

#[test]
fn moon_circuit_hacker_freezes_entered_this_turn_before_source_leaves() {
    let mut state = ready_game(0x4641_4552_4945_5308);
    let moon = enter_from_hand(&mut state, PlayerId::P0, "Moon-Circuit Hacker");
    let drawn = put_object(
        &mut state,
        PlayerId::P0,
        PlayerId::P0,
        "Island",
        Zone::Library,
    );
    let moon_generation = state.objects.get(moon).zone_change_count;
    event::log_combat_damage_to_player(&mut state, moon, moon_generation, PlayerId::P1, 2);
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    assert_eq!(
        state.stack.last().unwrap().kind,
        StackItemKind::TriggeredAbility
    );
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(moon, Zone::Hand));
    let choice = resolve_one_item_until_choice(&mut state);
    assert!(matches!(
        choice,
        Decision::ChooseEffectOption {
            player: PlayerId::P0,
            source,
            option_count: 2,
        } if source == moon
    ));
    engine::step(&mut state, Action::ChooseEffectOption(1)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    assert_eq!(state.objects.get(drawn).zone, Zone::Hand);
    assert!(
        state.engine.pending_discard.is_none(),
        "the frozen entered-this-turn branch does not discard after drawing"
    );

    let mut old = ready_game(0x4641_4552_4945_5309);
    let old_moon = put_object(
        &mut old,
        PlayerId::P0,
        PlayerId::P0,
        "Moon-Circuit Hacker",
        Zone::Battlefield,
    );
    put_object(
        &mut old,
        PlayerId::P0,
        PlayerId::P0,
        "Island",
        Zone::Library,
    );
    put_object(&mut old, PlayerId::P0, PlayerId::P0, "Ponder", Zone::Hand);
    let old_moon_generation = old.objects.get(old_moon).zone_change_count;
    event::log_combat_damage_to_player(&mut old, old_moon, old_moon_generation, PlayerId::P1, 2);
    assert!(matches!(
        engine::advance_until_decision(&mut old),
        Decision::CastSpellOrPass { .. }
    ));
    assert!(matches!(
        resolve_one_item_until_choice(&mut old),
        Decision::ChooseEffectOption { .. }
    ));
    engine::step(&mut old, Action::ChooseEffectOption(1)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut old),
        Decision::Discard {
            player: PlayerId::P0,
            count: 1,
            ..
        }
    ));
}

#[test]
fn snap_bounces_first_then_selects_only_tapped_lands_without_a_controller_filter() {
    let mut state = ready_game(0x4641_4552_4945_5310);
    let target = put_object(
        &mut state,
        PlayerId::P1,
        PlayerId::P1,
        "Guttersnipe",
        Zone::Battlefield,
    );
    let first = put_object(
        &mut state,
        PlayerId::P0,
        PlayerId::P0,
        "Island",
        Zone::Battlefield,
    );
    let second = put_object(
        &mut state,
        PlayerId::P0,
        PlayerId::P0,
        "Island",
        Zone::Battlefield,
    );
    let third = put_object(
        &mut state,
        PlayerId::P0,
        PlayerId::P0,
        "Island",
        Zone::Battlefield,
    );
    let opponent_land = put_object(
        &mut state,
        PlayerId::P1,
        PlayerId::P1,
        "Island",
        Zone::Battlefield,
    );
    for land in [first, second, opponent_land] {
        state.objects.get_mut(land).tapped = true;
    }
    let snap = cast_targeted_spell(&mut state, PlayerId::P0, "Snap", target, 2);
    let choice = resolve_one_item_until_choice(&mut state);
    let Decision::ChooseEffectTargets {
        player,
        source,
        selected_count,
        min_targets,
        max_targets,
        legal_targets,
        can_finish,
    } = choice.clone()
    else {
        panic!("Snap land selection")
    };
    assert_eq!((player, source), (PlayerId::P0, snap));
    assert_eq!((selected_count, min_targets, max_targets), (0, 0, 2));
    assert!(can_finish);
    assert_eq!(
        legal_targets,
        vec![
            Target::Object(first),
            Target::Object(second),
            Target::Object(opponent_land)
        ]
    );
    assert!(!legal_targets.contains(&Target::Object(third)));
    assert_eq!(state.objects.get(target).zone, Zone::Hand);

    let restored: GameState =
        serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
    assert_eq!(restored, state);
    let observation = observe_v2(&state, &HarnessSurfaceV2::new(), PlayerId::P1, 1).unwrap();
    assert!(matches!(
        observation
            .projection
            .engine_context
            .pending_effect
            .as_ref()
            .and_then(|pending| pending.choice.as_ref()),
        Some(PendingEffectChoiceSemanticV4::Targets {
            purpose: TargetSelectionPurposeV4::PermanentSelection,
            min_targets: 0,
            max_targets: 2,
            can_finish: true,
            ..
        })
    ));
    let actions = legal_action_candidates_v1(&SurfaceDecision::Decision(choice), &state).unwrap();
    assert_eq!(actions.len(), 4, "three lands plus finish");

    let mut tampered = state.clone();
    let PendingEffectChoice::SelectTargets { path, .. } = tampered
        .engine
        .pending_effect
        .as_mut()
        .and_then(|pending| pending.choice.as_mut())
        .expect("Snap pending land choice")
    else {
        panic!("Snap target-selection continuation")
    };
    path.push(65_535);
    assert!(matches!(
        engine::advance_until_decision(&mut tampered),
        Decision::Halted {
            mechanic: UnsupportedMechanic::InvalidEffectContinuation,
            source,
        } if source == snap
    ));

    engine::step(
        &mut state,
        Action::ChooseEffectTarget(Target::Object(first)),
    )
    .unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::ChooseEffectTargets {
            selected_count: 1,
            can_finish: true,
            ..
        }
    ));
    engine::step(
        &mut state,
        Action::ChooseEffectTarget(Target::Object(opponent_land)),
    )
    .unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    assert!(!state.objects.get(first).tapped);
    assert!(state.objects.get(second).tapped);
    assert!(!state.objects.get(third).tapped);
    assert!(!state.objects.get(opponent_land).tapped);
    assert_eq!(state.objects.get(snap).zone, Zone::Graveyard);
}

#[test]
fn bind_attaches_before_etb_damage_uses_the_creature_and_prevents_untap() {
    let mut state = ready_game(0x4641_4552_4945_5311);
    let creature = put_object(
        &mut state,
        PlayerId::P1,
        PlayerId::P1,
        "Sacred Cat",
        Zone::Battlefield,
    );
    state.objects.get_mut(creature).counters.plus1_plus1 = 1;
    let bind = cast_targeted_spell(&mut state, PlayerId::P0, "Bind the Monster", creature, 1);
    resolve_top_without_choice(&mut state);
    assert_eq!(state.objects.get(bind).zone, Zone::Battlefield);
    let host_generation = state.objects.get(creature).zone_change_count;
    assert_eq!(
        state.objects.get(bind).v4.attached_to,
        Some(mtg_kernel::state::ObjectLinkV4 {
            object: creature,
            zone_change_count: host_generation,
        })
    );
    assert!(state.objects.get(creature).attachments.contains(&bind));
    assert_eq!(
        state.stack.last().unwrap().kind,
        StackItemKind::TriggeredAbility
    );

    let mut lki = state.clone();
    event::propose_and_commit(&mut lki, ProposedEvent::zone_change(bind, Zone::Graveyard));
    resolve_top_without_choice(&mut lki);
    assert!(lki.objects.get(creature).tapped);
    assert_eq!(lki.players[0].life, 18);
    assert_eq!(lki.players[1].life, 22);

    resolve_top_without_choice(&mut state);
    assert!(state.objects.get(creature).tapped);
    assert_eq!(state.players[0].life, 18);
    assert_eq!(
        state.players[1].life, 22,
        "the attached lifelink creature is the damage source"
    );

    state.step = Step::Cleanup;
    state.active_player = PlayerId::P0;
    state.priority_player = PlayerId::P0;
    state.engine.priority_passes = [false, false];
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass {
            player: PlayerId::P1,
            ..
        }
    ));
    assert_eq!(state.step, Step::Upkeep);
    assert!(
        state.objects.get(creature).tapped,
        "Bind prevents the host from untapping"
    );

    event::propose_and_commit(&mut state, ProposedEvent::zone_change(creature, Zone::Hand));
    let _ = engine::advance_until_decision(&mut state);
    assert_eq!(
        state.objects.get(bind).zone,
        Zone::Graveyard,
        "an unattached Aura is put into its owner's graveyard by SBA"
    );
}
