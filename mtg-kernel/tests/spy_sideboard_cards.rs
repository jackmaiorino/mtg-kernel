//! Focused rules, RL, and restored-state coverage for Spy's remaining pool cards.
//!
//! The checked-in Mage Java implementations are the behavior authority:
//! `FaerieMacabre.java`, `FlaringPain.java`, `FumeSpitter.java`, and
//! `MesmericFiend.java`.

use mtg_kernel::card_def::{
    card_id_by_name, preflight_fully_supported_deck, CardCapability, CostComponent, Keywords,
    Subtype, TargetSpec, CARD_DEFS,
};
use mtg_kernel::effect::{
    EffectOp, EffectTargetSelectionPurpose, ObjectRef, PendingEffectChoice, PlayerRef,
};
use mtg_kernel::engine::{self, Action, Decision, UnsupportedMechanic, UntilEndOfTurnEffect};
use mtg_kernel::event::{self, ActiveReplacement, ProposedEvent, ReplacementEffectKind};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{Cost, ManaColor, Pip};
use mtg_kernel::rl::{
    legal_action_candidates_v1, observe_v2, ActionSemanticV1, ObjectRelationPublicV4,
    PendingEffectChoiceSemanticV4, TargetRefV1, TargetSelectionPurposeV4,
};
use mtg_kernel::state::{
    CastMethodV4, Counters, GameObject, GameState, ObjectStateV4, StackItemKind, Step, Target, Zone,
};
use mtg_kernel::surface_v2::{HarnessSurfaceV2, SurfaceDecision};
use mtg_kernel::trigger::{self, TriggerCondition};

fn card_id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name} in CARD_DEFS"))
}

fn card_name(card_def: u16) -> String {
    CARD_DEFS[card_def as usize].name.to_string()
}

fn ready_main(seed: u64) -> GameState {
    let p0 = vec![card_id("Mountain"); 8];
    let p1 = vec![card_id("Island"); 8];
    let mut state = GameState::new_from_libraries(&p0, &p1, card_name, seed);
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

fn add_mana(state: &mut GameState, color: ManaColor, amount: u8) {
    state.players[PlayerId::P0.index()].mana_pool[color.pool_index()] += amount;
}

fn queue_move_triggers(state: &mut GameState, object: ObjectId, to: Zone) {
    event::propose_and_commit(state, ProposedEvent::zone_change(object, to));
    let pending = trigger::collect_and_process(state);
    state.engine.pending_triggers.extend(pending);
}

fn pass_until_idle(state: &mut GameState) -> Decision {
    for _ in 0..96 {
        let decision = engine::advance_until_decision(state);
        if state.stack.is_empty()
            && state.engine.pending_effect.is_none()
            && state.engine.pending_triggers.is_empty()
            && matches!(decision, Decision::CastSpellOrPass { .. })
        {
            return decision;
        }
        match decision {
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            Decision::Halted { .. } | Decision::GameOver { .. } => return decision,
            other => panic!("unexpected decision while resolving to idle: {other:?}"),
        }
    }
    panic!("bounded resolution did not become idle")
}

fn pass_until_effect_targets(state: &mut GameState, source: ObjectId) -> Decision {
    for _ in 0..48 {
        let decision = engine::advance_until_decision(state);
        match decision {
            Decision::ChooseEffectTargets { source: actual, .. } if actual == source => {
                return decision
            }
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => panic!("unexpected decision before effect target choice: {other:?}"),
        }
    }
    panic!("bounded resolution did not yield an effect target choice")
}

fn choose_fiend_trigger_target(state: &mut GameState, fiend: ObjectId) {
    let decision = engine::advance_until_decision(state);
    let Decision::ChooseTargets {
        player,
        spell,
        remaining,
        legal_targets,
        can_finish,
    } = decision
    else {
        panic!("expected Mesmeric Fiend trigger targeting, got {decision:?}")
    };
    assert_eq!(player, PlayerId::P0);
    assert_eq!(spell, fiend);
    assert_eq!(remaining, 1);
    assert_eq!(legal_targets, vec![Target::Player(PlayerId::P1)]);
    assert!(!can_finish);
    engine::step(state, Action::ChooseTarget(Target::Player(PlayerId::P1))).unwrap();
}

fn assert_historical_exile_relation(
    state: &GameState,
    fiend: ObjectId,
    fiend_zone_change_count: u32,
    exiled: ObjectId,
) {
    for observer in [PlayerId::P0, PlayerId::P1] {
        let observation = observe_v2(state, &HarnessSurfaceV2::default(), observer, 17).unwrap();
        assert!(observation
            .projection
            .object_relations
            .iter()
            .any(|relation| {
                matches!(
                    relation,
                    ObjectRelationPublicV4::ExiledBy {
                        object,
                        exiled_by,
                    } if object.arena_id == exiled.0
                        && exiled_by.arena_id == fiend.0
                        && exiled_by.zone == Zone::Battlefield
                        && exiled_by.zone_change_count == fiend_zone_change_count
                )
            }));
    }
}

#[test]
fn registry_appends_exact_mage_definitions_and_stable_ids() {
    let expected = [
        ("Faerie Macabre", 155_u16),
        ("Flaring Pain", 156),
        ("Fume Spitter", 157),
        ("Mesmeric Fiend", 158),
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

    let macabre = &CARD_DEFS[ids[0] as usize];
    assert_eq!((macabre.power, macabre.toughness), (Some(2), Some(2)));
    assert_eq!(macabre.subtypes, &[Subtype::Faerie, Subtype::Rogue]);
    assert!(macabre.keywords.has(Keywords::FLYING));
    assert_eq!(macabre.activated_abilities.len(), 1);
    let macabre_ability = &macabre.activated_abilities[0];
    assert_eq!(macabre_ability.activation_zone, Zone::Hand);
    assert_eq!(macabre_ability.cost, &[CostComponent::DiscardSelf]);
    assert_eq!(
        macabre_ability.target_spec,
        TargetSpec::UpToTwoCardsInGraveyards
    );
    assert_eq!(
        (macabre_ability.effect)(),
        EffectOp::MoveAllTargets {
            to_zone: Zone::Exile,
        }
    );

    let flaring = &CARD_DEFS[ids[1] as usize];
    assert_eq!(
        (flaring.spell_effect)(),
        Some(EffectOp::DamageCannotBePreventedThisTurn)
    );
    assert_eq!(
        flaring.flashback.as_ref().unwrap().cost,
        &[CostComponent::Mana(Cost {
            pips: &[Pip::Colored(ManaColor::R)],
            generic: 0,
            x_count: 0,
        })]
    );

    let spitter = &CARD_DEFS[ids[2] as usize];
    assert_eq!((spitter.power, spitter.toughness), (Some(1), Some(1)));
    assert_eq!(spitter.subtypes, &[Subtype::Phyrexian, Subtype::Horror]);
    assert_eq!(spitter.activated_abilities.len(), 1);
    let spitter_ability = &spitter.activated_abilities[0];
    assert_eq!(spitter_ability.cost, &[CostComponent::SacrificeSelf]);
    assert_eq!(spitter_ability.target_spec, TargetSpec::Creature);
    assert_eq!(
        (spitter_ability.effect)(),
        EffectOp::AddMinusOneMinusOneCounter {
            object: ObjectRef::Target(0),
        }
    );

    let fiend = &CARD_DEFS[ids[3] as usize];
    assert_eq!((fiend.power, fiend.toughness), (Some(1), Some(1)));
    assert_eq!(fiend.subtypes, &[Subtype::Nightmare, Subtype::Horror]);
    let triggers = trigger::triggers_for(ids[3]);
    assert_eq!(triggers.len(), 2);
    assert_eq!(triggers[0].condition, TriggerCondition::Etb);
    assert_eq!(triggers[1].condition, TriggerCondition::LeftBattlefield);
    assert_eq!(
        (triggers[0].effect)(),
        EffectOp::RevealHandChooseNonlandToLinkedExile {
            player: PlayerRef::Target(0),
        }
    );
    assert_eq!(
        (triggers[1].effect)(),
        EffectOp::ReturnLinkedExiledCardToOwnersHand
    );
    assert_eq!(
        trigger::target_spec_for_trigger(ids[3], &(triggers[0].effect)()),
        Some(TargetSpec::TargetOpponent)
    );

    assert_eq!(TargetSpec::UpToTwoCardsInGraveyards.stable_id(), 30);
    assert_eq!(Subtype::Giant.stable_id(), 63);
    assert_eq!(Subtype::Spawn.stable_id(), 64);
    assert_eq!(Subtype::Phyrexian.stable_id(), 65);
    assert_eq!(Subtype::Horror.stable_id(), 66);
    assert_eq!(Subtype::Nightmare.stable_id(), 67);
}

#[test]
fn faerie_macabre_discards_from_hand_and_exiles_zero_one_or_two_exact_targets() {
    let mut state = ready_main(0x4641_4552_4945_0001);
    let macabre = put_object(&mut state, PlayerId::P0, "Faerie Macabre", Zone::Hand);
    let own = put_object(&mut state, PlayerId::P0, "Flaring Pain", Zone::Graveyard);
    let opposing = put_object(&mut state, PlayerId::P1, "Fume Spitter", Zone::Graveyard);
    let opposing_other = put_object(&mut state, PlayerId::P1, "Forest", Zone::Graveyard);

    engine::step(&mut state, Action::ActivateAbility(macabre, 0)).unwrap();
    let first = engine::advance_until_decision(&mut state);
    let Decision::ChooseEffectTargets {
        legal_targets,
        can_finish,
        selected_count,
        min_targets,
        max_targets,
        ..
    } = &first
    else {
        panic!("expected optional graveyard targets")
    };
    assert_eq!((*selected_count, *min_targets, *max_targets), (0, 0, 2));
    assert!(*can_finish);
    assert_eq!(
        legal_targets,
        &vec![
            Target::Object(own),
            Target::Object(opposing),
            Target::Object(opposing_other),
        ]
    );
    let candidates =
        legal_action_candidates_v1(&SurfaceDecision::Decision(first.clone()), &state).unwrap();
    assert_eq!(
        candidates
            .iter()
            .filter(|candidate| {
                matches!(
                    candidate.record.semantic,
                    ActionSemanticV1::ChooseEffectTarget { .. }
                )
            })
            .count(),
        3
    );
    assert!(candidates.iter().any(|candidate| matches!(
        candidate.record.semantic,
        ActionSemanticV1::FinishEffectSelection {
            selected_count: 0,
            ..
        }
    )));

    engine::step(&mut state, Action::ChooseEffectTarget(Target::Object(own))).unwrap();
    let second = engine::advance_until_decision(&mut state);
    let Decision::ChooseEffectTargets { legal_targets, .. } = second else {
        panic!("expected second optional graveyard target")
    };
    assert_eq!(
        legal_targets,
        vec![Target::Object(opposing), Target::Object(opposing_other)]
    );
    engine::step(
        &mut state,
        Action::ChooseEffectTarget(Target::Object(opposing)),
    )
    .unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    assert_eq!(state.objects.get(macabre).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(macabre).zone_change_count, 1);
    let item = state.stack.last().unwrap();
    assert_eq!(item.kind, StackItemKind::ActivatedAbility);
    let contract = item.v4.ability_source_contract.unwrap();
    assert_eq!(contract.source, macabre);
    assert_eq!(contract.zone, Zone::Hand);
    assert_eq!(contract.zone_change_count, 0);
    assert_eq!(item.v4.paid_cost_refs.len(), 1);
    pass_until_idle(&mut state);
    assert_eq!(state.objects.get(own).zone, Zone::Exile);
    assert_eq!(state.objects.get(opposing).zone, Zone::Exile);
    assert_eq!(state.objects.get(opposing_other).zone, Zone::Graveyard);

    let mut one_stale = ready_main(0x4641_4552_4945_0002);
    let macabre = put_object(&mut one_stale, PlayerId::P0, "Faerie Macabre", Zone::Hand);
    let first = put_object(
        &mut one_stale,
        PlayerId::P0,
        "Flaring Pain",
        Zone::Graveyard,
    );
    let stale = put_object(
        &mut one_stale,
        PlayerId::P1,
        "Fume Spitter",
        Zone::Graveyard,
    );
    engine::step(&mut one_stale, Action::ActivateAbility(macabre, 0)).unwrap();
    engine::step(
        &mut one_stale,
        Action::ChooseEffectTarget(Target::Object(first)),
    )
    .unwrap();
    engine::step(
        &mut one_stale,
        Action::ChooseEffectTarget(Target::Object(stale)),
    )
    .unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut one_stale),
        Decision::CastSpellOrPass { .. }
    ));
    event::propose_and_commit(
        &mut one_stale,
        ProposedEvent::zone_change(stale, Zone::Hand),
    );
    pass_until_idle(&mut one_stale);
    assert_eq!(one_stale.objects.get(first).zone, Zone::Exile);
    assert_eq!(one_stale.objects.get(stale).zone, Zone::Hand);

    let mut zero = ready_main(0x4641_4552_4945_0003);
    let macabre = put_object(&mut zero, PlayerId::P0, "Faerie Macabre", Zone::Hand);
    engine::step(&mut zero, Action::ActivateAbility(macabre, 0)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut zero),
        Decision::CastSpellOrPass { .. }
    ));
    assert_eq!(zero.objects.get(macabre).zone, Zone::Graveyard);
    assert_eq!(zero.stack.last().unwrap().targets, Vec::<Target>::new());
    pass_until_idle(&mut zero);
}

#[test]
fn flaring_pain_disables_prevention_without_consuming_shields_and_flashbacks() {
    let mut state = ready_main(0x464c_4152_494e_0001);
    let flaring = put_object(&mut state, PlayerId::P0, "Flaring Pain", Zone::Hand);
    add_mana(&mut state, ManaColor::R, 2);
    engine::step(&mut state, Action::CastSpell(flaring)).unwrap();
    assert_eq!(
        state.stack.last().unwrap().v4.cast_method,
        Some(CastMethodV4::Normal)
    );
    pass_until_idle(&mut state);
    assert_eq!(state.objects.get(flaring).zone, Zone::Graveyard);
    assert!(matches!(
        state.engine.until_end_of_turn.as_slice(),
        [UntilEndOfTurnEffect::DamageCannotBePrevented { .. }]
    ));
    let observation = observe_v2(&state, &HarnessSurfaceV2::default(), PlayerId::P1, 3).unwrap();
    let public = observation
        .projection
        .continuous_effects
        .iter()
        .find(|effect| effect.damage_cannot_be_prevented)
        .expect("Flaring Pain public global effect");
    assert!(public.global);
    assert!(public.source.is_none());
    assert!(public.affected_objects.is_empty());
    assert!(public.affected_players.is_empty());

    state.engine.active_replacements.push(ActiveReplacement {
        id: 91,
        source: flaring,
        kind: ReplacementEffectKind::PreventNextDamage {
            target: Target::Player(PlayerId::P1),
            remaining: 2,
        },
    });
    event::propose_and_commit(
        &mut state,
        ProposedEvent::damage(flaring, Target::Player(PlayerId::P1), 3),
    );
    assert_eq!(state.players[PlayerId::P1.index()].life, 17);
    assert!(matches!(
        state.engine.active_replacements[0].kind,
        ReplacementEffectKind::PreventNextDamage { remaining: 2, .. }
    ));

    state.engine.until_end_of_turn.clear();
    event::propose_and_commit(
        &mut state,
        ProposedEvent::damage(flaring, Target::Player(PlayerId::P1), 3),
    );
    assert_eq!(state.players[PlayerId::P1.index()].life, 16);
    assert!(state.engine.active_replacements.is_empty());

    let mut flashback = ready_main(0x464c_4152_494e_0002);
    let flaring = put_object(
        &mut flashback,
        PlayerId::P0,
        "Flaring Pain",
        Zone::Graveyard,
    );
    add_mana(&mut flashback, ManaColor::R, 1);
    engine::step(&mut flashback, Action::CastSpell(flaring)).unwrap();
    let item = flashback.stack.last().unwrap();
    assert!(item.is_flashback);
    assert_eq!(item.v4.cast_method, Some(CastMethodV4::Flashback));
    pass_until_idle(&mut flashback);
    assert_eq!(flashback.objects.get(flaring).zone, Zone::Exile);
    assert!(matches!(
        flashback.engine.until_end_of_turn.as_slice(),
        [UntilEndOfTurnEffect::DamageCannotBePrevented { .. }]
    ));
}

#[test]
fn fume_spitter_sacrifices_before_resolution_and_binds_target_and_source_incarnations() {
    let mut state = ready_main(0x4655_4d45_0000_0001);
    let spitter = put_object(&mut state, PlayerId::P0, "Fume Spitter", Zone::Battlefield);
    let target = put_object(&mut state, PlayerId::P1, "Myr Enforcer", Zone::Battlefield);
    engine::step(&mut state, Action::ActivateAbility(spitter, 0)).unwrap();
    let Decision::ChooseTargets { legal_targets, .. } = engine::advance_until_decision(&mut state)
    else {
        panic!("expected Fume Spitter target")
    };
    assert!(legal_targets.contains(&Target::Object(target)));
    engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    assert_eq!(state.objects.get(spitter).zone, Zone::Graveyard);
    let item = state.stack.last().unwrap();
    assert_eq!(
        item.v4.ability_source_contract.unwrap().zone,
        Zone::Battlefield
    );
    assert_eq!(
        item.v4.ability_source_contract.unwrap().zone_change_count,
        0
    );
    pass_until_idle(&mut state);
    assert_eq!(state.objects.get(target).counters.minus1_minus1, 1);
    assert_eq!(engine::effective_power(&state, target), 3);
    assert_eq!(engine::effective_toughness(&state, target), 3);

    let mut stale = ready_main(0x4655_4d45_0000_0002);
    let spitter = put_object(&mut stale, PlayerId::P0, "Fume Spitter", Zone::Battlefield);
    let target = put_object(&mut stale, PlayerId::P1, "Myr Enforcer", Zone::Battlefield);
    engine::step(&mut stale, Action::ActivateAbility(spitter, 0)).unwrap();
    engine::step(&mut stale, Action::ChooseTarget(Target::Object(target))).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut stale),
        Decision::CastSpellOrPass { .. }
    ));
    event::propose_and_commit(&mut stale, ProposedEvent::zone_change(target, Zone::Hand));
    event::propose_and_commit(
        &mut stale,
        ProposedEvent::zone_change(target, Zone::Battlefield),
    );
    pass_until_idle(&mut stale);
    assert_eq!(stale.objects.get(target).counters.minus1_minus1, 0);

    let mut tampered = ready_main(0x4655_4d45_0000_0003);
    let spitter = put_object(
        &mut tampered,
        PlayerId::P0,
        "Fume Spitter",
        Zone::Battlefield,
    );
    let target = put_object(
        &mut tampered,
        PlayerId::P1,
        "Myr Enforcer",
        Zone::Battlefield,
    );
    engine::step(&mut tampered, Action::ActivateAbility(spitter, 0)).unwrap();
    engine::step(&mut tampered, Action::ChooseTarget(Target::Object(target))).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut tampered),
        Decision::CastSpellOrPass { .. }
    ));
    tampered
        .stack
        .last_mut()
        .unwrap()
        .v4
        .ability_source_contract
        .as_mut()
        .unwrap()
        .card_def = card_id("Faerie Macabre");
    let wire = serde_json::to_vec(&tampered).unwrap();
    let mut restored: GameState = serde_json::from_slice(&wire).unwrap();
    assert!(observe_v2(&restored, &HarnessSurfaceV2::default(), PlayerId::P0, 4).is_err());
    let target_before = restored.objects.get(target).clone();
    let decision = pass_until_idle(&mut restored);
    assert!(matches!(
        decision,
        Decision::Halted {
            mechanic: UnsupportedMechanic::InvalidEffectContinuation,
            source,
        } if source == spitter
    ));
    assert_eq!(restored.objects.get(target), &target_before);
}

#[test]
fn mesmeric_fiend_reveals_publicly_exiles_nonland_and_returns_exact_linked_card() {
    let mut state = ready_main(0x4d45_534d_4552_0001);
    let fiend = put_object(&mut state, PlayerId::P0, "Mesmeric Fiend", Zone::Hand);
    let land = put_object(&mut state, PlayerId::P1, "Forest", Zone::Hand);
    let chosen = put_object(&mut state, PlayerId::P1, "Flaring Pain", Zone::Hand);
    let other = put_object(&mut state, PlayerId::P1, "Fume Spitter", Zone::Hand);
    let before = observe_v2(&state, &HarnessSurfaceV2::default(), PlayerId::P0, 1).unwrap();
    assert!(before.known_hand_cards[PlayerId::P1.index()].is_empty());

    queue_move_triggers(&mut state, fiend, Zone::Battlefield);
    choose_fiend_trigger_target(&mut state, fiend);
    let choice = pass_until_effect_targets(&mut state, fiend);
    let Decision::ChooseEffectTargets {
        player,
        selected_count,
        min_targets,
        max_targets,
        legal_targets,
        can_finish,
        ..
    } = &choice
    else {
        unreachable!()
    };
    assert_eq!(*player, PlayerId::P0);
    assert_eq!((*selected_count, *min_targets, *max_targets), (0, 1, 1));
    assert_eq!(
        legal_targets,
        &vec![Target::Object(chosen), Target::Object(other)]
    );
    assert!(!can_finish);
    assert!(!legal_targets.contains(&Target::Object(land)));

    for observer in [PlayerId::P0, PlayerId::P1] {
        let observation = observe_v2(&state, &HarnessSurfaceV2::default(), observer, 2).unwrap();
        if observer == PlayerId::P0 {
            assert_eq!(observation.known_hand_cards[PlayerId::P1.index()].len(), 3);
        } else {
            assert_eq!(observation.own_hand.len(), 3);
        }
        let pending = observation
            .projection
            .engine_context
            .pending_effect
            .as_ref()
            .expect("Mesmeric pending effect");
        let Some(PendingEffectChoiceSemanticV4::Targets {
            legal_targets,
            purpose,
            ..
        }) = pending.choice.as_ref()
        else {
            panic!("Mesmeric choice must project as public targets")
        };
        assert_eq!(*purpose, TargetSelectionPurposeV4::CardSelection);
        let ids = legal_targets
            .iter()
            .map(|target| match target {
                TargetRefV1::Object { object } => object.arena_id,
                TargetRefV1::Player { .. } => panic!("hand choice exposed a player"),
            })
            .collect::<Vec<_>>();
        assert_eq!(ids, vec![chosen.0, other.0]);
    }
    let actions =
        legal_action_candidates_v1(&SurfaceDecision::Decision(choice.clone()), &state).unwrap();
    assert_eq!(
        actions
            .iter()
            .filter(|candidate| matches!(
                candidate.record.semantic,
                ActionSemanticV1::ChooseEffectTarget { .. }
            ))
            .count(),
        2
    );

    engine::step(
        &mut state,
        Action::ChooseEffectTarget(Target::Object(chosen)),
    )
    .unwrap();
    pass_until_idle(&mut state);
    assert_eq!(state.objects.get(chosen).zone, Zone::Exile);
    assert_eq!(state.objects.get(other).zone, Zone::Hand);
    assert_eq!(state.engine.linked_exile_records.len(), 1);
    let record = state.engine.linked_exile_records[0];
    assert_eq!(record.source.source, fiend);
    assert_eq!(record.source.zone_change_count, 1);
    assert_eq!(record.exiled, chosen);
    assert_historical_exile_relation(&state, fiend, 1, chosen);

    let mut tampered = state.clone();
    tampered.engine.linked_exile_records[0].exiled_card_def = card_id("Forest");
    let wire = serde_json::to_vec(&tampered).unwrap();
    let restored: GameState = serde_json::from_slice(&wire).unwrap();
    assert!(observe_v2(&restored, &HarnessSurfaceV2::default(), PlayerId::P0, 5).is_err());

    queue_move_triggers(&mut state, fiend, Zone::Graveyard);
    assert_historical_exile_relation(&state, fiend, 1, chosen);
    pass_until_idle(&mut state);
    assert_eq!(state.objects.get(chosen).zone, Zone::Hand);
    assert!(state.players[PlayerId::P1.index()].hand.contains(&chosen));
    assert!(state.engine.linked_exile_records.is_empty());
    assert!(state
        .known_hand_cards(PlayerId::P0, PlayerId::P1)
        .iter()
        .any(|entry| entry.object == chosen));
}

#[test]
fn mesmeric_fiend_classic_leave_before_etb_permanently_exiles_and_all_land_is_noop() {
    let mut state = ready_main(0x4d45_534d_4552_0002);
    let fiend = put_object(&mut state, PlayerId::P0, "Mesmeric Fiend", Zone::Hand);
    let chosen = put_object(&mut state, PlayerId::P1, "Flaring Pain", Zone::Hand);
    queue_move_triggers(&mut state, fiend, Zone::Battlefield);
    choose_fiend_trigger_target(&mut state, fiend);
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));

    queue_move_triggers(&mut state, fiend, Zone::Graveyard);
    pass_until_idle(&mut state);
    assert_eq!(state.objects.get(chosen).zone, Zone::Exile);
    assert_eq!(state.objects.get(chosen).v4.exiled_by, None);
    assert!(state.engine.linked_exile_records.is_empty());

    let mut all_land = ready_main(0x4d45_534d_4552_0003);
    let fiend = put_object(&mut all_land, PlayerId::P0, "Mesmeric Fiend", Zone::Hand);
    let forest = put_object(&mut all_land, PlayerId::P1, "Forest", Zone::Hand);
    let island = put_object(&mut all_land, PlayerId::P1, "Island", Zone::Hand);
    queue_move_triggers(&mut all_land, fiend, Zone::Battlefield);
    choose_fiend_trigger_target(&mut all_land, fiend);
    pass_until_idle(&mut all_land);
    assert_eq!(all_land.objects.get(forest).zone, Zone::Hand);
    assert_eq!(all_land.objects.get(island).zone, Zone::Hand);
    assert!(all_land.engine.linked_exile_records.is_empty());
    assert_eq!(
        all_land.known_hand_cards(PlayerId::P0, PlayerId::P1).len(),
        2
    );
}

#[test]
fn mesmeric_fiend_tampered_restored_choice_is_rejected_atomically() {
    let mut state = ready_main(0x4d45_534d_4552_0004);
    let fiend = put_object(&mut state, PlayerId::P0, "Mesmeric Fiend", Zone::Hand);
    let chosen = put_object(&mut state, PlayerId::P1, "Flaring Pain", Zone::Hand);
    put_object(&mut state, PlayerId::P1, "Fume Spitter", Zone::Hand);
    queue_move_triggers(&mut state, fiend, Zone::Battlefield);
    choose_fiend_trigger_target(&mut state, fiend);
    let _ = pass_until_effect_targets(&mut state, fiend);

    let pending = state.engine.pending_effect.as_mut().unwrap();
    let Some(PendingEffectChoice::SelectTargets {
        legal,
        purpose: EffectTargetSelectionPurpose::LinkedExileNonlandFromRevealedHand { .. },
        ..
    }) = pending.choice.as_mut()
    else {
        panic!("expected linked hand choice")
    };
    legal.pop();
    let wire = serde_json::to_vec(&state).unwrap();
    let mut restored: GameState = serde_json::from_slice(&wire).unwrap();
    assert!(observe_v2(&restored, &HarnessSurfaceV2::default(), PlayerId::P0, 8).is_err());
    let before = restored.clone();
    assert!(engine::step(
        &mut restored,
        Action::ChooseEffectTarget(Target::Object(chosen))
    )
    .is_err());
    assert_eq!(restored, before, "rejected restored choice must be atomic");
}
