//! Focused current-Mage parity for Monstrous Emergence and Nyxborn Hydra.

use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardType, CostComponent, Keywords, Subtype, TargetSpec,
    CARD_DEFS, KERNEL_CARDDB_HASH,
};
use mtg_kernel::effect::EffectOp;
use mtg_kernel::engine::{self, Action, CostKind, Decision, UnsupportedMechanic};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{Cost, ManaColor, Pip};
use mtg_kernel::rl::{legal_action_candidates_v1, ActionSemanticV1};
use mtg_kernel::state::{
    CastMethodV4, Counters, GameObject, GameState, ObjectStateV4, Step, Target, Zone,
};
use mtg_kernel::surface_v2::SurfaceDecision;

fn card_id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name} must exist in CARD_DEFS"))
}

fn card_name(card_def: u16) -> String {
    CARD_DEFS[card_def as usize].name.to_string()
}

fn ready_state(seed: u64) -> GameState {
    let mut state = GameState::new_from_libraries(&[], &[], card_name, seed);
    state.active_player = PlayerId::P0;
    state.priority_player = PlayerId::P0;
    state.step = Step::Main1;
    state
}

fn put_object(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id(name);
    let id = state.objects.push(GameObject {
        card_def,
        name: CARD_DEFS[card_def as usize].object_name.to_string(),
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
        Zone::Stack => panic!("test helper does not fabricate stack objects"),
    }
    id
}

fn add_mana(state: &mut GameState, green: u8, generic: u8) {
    state.players[0].mana_pool[ManaColor::G.pool_index()] = green;
    state.players[0].mana_pool[ManaColor::C.pool_index()] = generic;
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
            Decision::Halted { mechanic, source } => {
                panic!("unexpected halt {mechanic:?} from {source}")
            }
            other => panic!("unexpected decision while resolving: {other:?}"),
        }
    }
    panic!("bounded resolution did not become idle")
}

fn assert_invalid_continuation(state: &mut GameState) {
    for _ in 0..24 {
        match engine::advance_until_decision(state) {
            Decision::Halted {
                mechanic: UnsupportedMechanic::InvalidEffectContinuation,
                ..
            } => return,
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => panic!("tampered state remained actionable: {other:?}"),
        }
    }
    panic!("tampered state did not fail closed")
}

fn cast_monstrous_through_target(
    state: &mut GameState,
    spell: ObjectId,
    target: ObjectId,
) -> Decision {
    engine::step(state, Action::CastSpell(spell)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(state),
        Decision::ChooseTargets { .. }
    ));
    engine::step(state, Action::ChooseTarget(Target::Object(target))).unwrap();
    engine::advance_until_decision(state)
}

fn cast_hydra(state: &mut GameState, hydra: ObjectId, form: u8, target: Option<ObjectId>, x: u16) {
    engine::step(state, Action::CastSpell(hydra)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(state),
        Decision::ChooseSpellMode { mode_count: 2, .. }
    ));
    engine::step(state, Action::ChooseSpellMode(form)).unwrap();
    if let Some(target) = target {
        assert!(matches!(
            engine::advance_until_decision(state),
            Decision::ChooseTargets { .. }
        ));
        engine::step(state, Action::ChooseTarget(Target::Object(target))).unwrap();
    }
    let choice = engine::advance_until_decision(state);
    assert!(matches!(
        choice,
        Decision::ChooseEffectOption { option_count, .. } if option_count > x
    ));
    engine::step(state, Action::ChooseEffectOption(x)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(state),
        Decision::CastSpellOrPass { .. }
    ));
}

#[test]
fn generated_definitions_ids_and_append_only_token_are_exact() {
    assert_eq!(card_id("Avenging Hunter"), 1);
    assert_eq!(card_id("Monstrous Emergence"), 74);
    assert_eq!(card_id("Nyxborn Hydra"), 81);
    assert_eq!(card_id("Skeleton Token"), 161);
    assert_eq!(Subtype::Skeleton.stable_id(), 69);
    assert_eq!(CARD_DEFS.len(), 162);
    assert_eq!(KERNEL_CARDDB_HASH, 0x64c8_2a26_1e07_8f1a);

    let monstrous = &CARD_DEFS[74];
    assert_eq!(monstrous.capability, CardCapability::Full);
    assert_eq!(monstrous.types, &[CardType::Sorcery]);
    assert_eq!(monstrous.target_spec, TargetSpec::Creature);
    assert_eq!(
        monstrous.additional_cost,
        Some(&[CostComponent::ChooseControlledCreatureOrRevealCreatureCardFromHand][..])
    );
    assert_eq!(
        (monstrous.spell_effect)(),
        Some(EffectOp::DealDamageToTargetEqualToChosenCostCreaturePower {
            target: mtg_kernel::effect::TargetRef::Target(0),
        })
    );

    let hydra = &CARD_DEFS[81];
    assert_eq!(hydra.capability, CardCapability::Full);
    assert_eq!(hydra.types, &[CardType::Enchantment, CardType::Creature]);
    assert_eq!(hydra.subtypes, &[Subtype::Hydra]);
    assert_eq!((hydra.power, hydra.toughness), (Some(0), Some(1)));
    assert!(hydra.keywords.has(Keywords::REACH));
    assert!(hydra.keywords.has(Keywords::TRAMPLE));
    assert_eq!(hydra.cost.x_count, 1);
    assert_eq!(hydra.cost.pips, &[Pip::Colored(ManaColor::G)]);
    let bestow = hydra.bestow.as_ref().expect("Nyxborn has Bestow");
    assert_eq!(
        bestow.cost,
        Cost {
            pips: &[Pip::Colored(ManaColor::G), Pip::Colored(ManaColor::G)],
            generic: 0,
            x_count: 1,
        }
    );
    assert_eq!(bestow.target_spec, TargetSpec::Creature);
    assert_eq!(
        (hydra.spell_effect)(),
        Some(EffectOp::PutSourceOntoBattlefieldWithXPlusOneCounters)
    );

    let skeleton = &CARD_DEFS[161];
    assert!(skeleton.is_token);
    assert_eq!(skeleton.colors, &[ManaColor::B]);
    assert_eq!(skeleton.subtypes, &[Subtype::Skeleton]);
    assert_eq!((skeleton.power, skeleton.toughness), (Some(4), Some(1)));
    assert!(skeleton.keywords.has(Keywords::MENACE));
}

#[test]
fn monstrous_battlefield_branch_samples_live_power_then_exact_lki_after_departure() {
    let mut live = ready_state(0x4d4f_4e53_5452_0001);
    let spell = put_object(&mut live, PlayerId::P0, "Monstrous Emergence", Zone::Hand);
    let chosen = put_object(
        &mut live,
        PlayerId::P0,
        "Avenging Hunter",
        Zone::Battlefield,
    );
    let other = put_object(&mut live, PlayerId::P0, "Sacred Cat", Zone::Battlefield);
    let target = put_object(&mut live, PlayerId::P1, "Tinder Wall", Zone::Battlefield);
    live.objects.get_mut(target).counters.plus1_plus1 = 20;
    add_mana(&mut live, 1, 1);

    let candidates = match cast_monstrous_through_target(&mut live, spell, target) {
        Decision::ChooseCostTargets {
            cost_kind: CostKind::ChooseCreatureOrRevealCreature,
            candidates,
            ..
        } => candidates,
        other => panic!("expected battlefield creature cost, got {other:?}"),
    };
    assert_eq!(candidates, vec![chosen, other]);
    engine::step(&mut live, Action::ChooseCostTarget(chosen)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut live),
        Decision::CastSpellOrPass { .. }
    ));
    assert_eq!(
        live.stack.last().unwrap().v4.paid_cost_refs[0].power_lki,
        Some(5)
    );

    live.objects.get_mut(chosen).counters.plus1_plus1 = 2;
    pass_until_idle(&mut live);
    assert_eq!(live.objects.get(target).damage, 7);

    let mut lki = ready_state(0x4d4f_4e53_5452_0002);
    let spell = put_object(&mut lki, PlayerId::P0, "Monstrous Emergence", Zone::Hand);
    let chosen = put_object(&mut lki, PlayerId::P0, "Avenging Hunter", Zone::Battlefield);
    let _other = put_object(&mut lki, PlayerId::P0, "Sacred Cat", Zone::Battlefield);
    let target = put_object(&mut lki, PlayerId::P1, "Tinder Wall", Zone::Battlefield);
    lki.objects.get_mut(target).counters.plus1_plus1 = 20;
    add_mana(&mut lki, 1, 1);
    assert!(matches!(
        cast_monstrous_through_target(&mut lki, spell, target),
        Decision::ChooseCostTargets { .. }
    ));
    engine::step(&mut lki, Action::ChooseCostTarget(chosen)).unwrap();
    engine::advance_until_decision(&mut lki);
    lki.objects.get_mut(chosen).counters.plus1_plus1 = 3;
    event::propose_and_commit(
        &mut lki,
        ProposedEvent::zone_change(chosen, Zone::Graveyard),
    );
    assert_eq!(
        lki.stack.last().unwrap().v4.paid_cost_refs[0].power_lki,
        Some(8)
    );
    assert_eq!(
        lki.stack
            .last()
            .unwrap()
            .v4
            .source_contract
            .unwrap()
            .finalized_cast_binding
            .unwrap()
            .chosen_creature_cost
            .unwrap()
            .power_lki,
        Some(8)
    );
    assert_eq!(
        lki.objects
            .get(spell)
            .v4
            .finalized_cast_binding
            .unwrap()
            .chosen_creature_cost
            .unwrap()
            .power_lki,
        Some(8)
    );

    let bytes = serde_json::to_vec(&lki).unwrap();
    let restored: GameState = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(lki, restored);

    let mut tampered = restored.clone();
    tampered.stack.last_mut().unwrap().v4.paid_cost_refs[0].card_def = card_id("Sacred Cat");
    assert_invalid_continuation(&mut tampered);

    let mut power_tampered = restored.clone();
    power_tampered.stack.last_mut().unwrap().v4.paid_cost_refs[0].power_lki = Some(99);
    assert_invalid_continuation(&mut power_tampered);

    pass_until_idle(&mut lki);
    assert_eq!(lki.objects.get(target).damage, 8);
}

#[test]
fn monstrous_hand_reveal_is_identity_scoped_and_both_cost_branches_keep_flat_identity() {
    let mut base = ready_state(0x4d4f_4e53_5452_0003);
    let spell = put_object(&mut base, PlayerId::P0, "Monstrous Emergence", Zone::Hand);
    let hand_a = put_object(&mut base, PlayerId::P0, "Avenging Hunter", Zone::Hand);
    let hand_b = put_object(&mut base, PlayerId::P0, "Nyxborn Hydra", Zone::Hand);
    let field_a = put_object(&mut base, PlayerId::P0, "Sacred Cat", Zone::Battlefield);
    let field_b = put_object(
        &mut base,
        PlayerId::P0,
        "Avenging Hunter",
        Zone::Battlefield,
    );
    let target = put_object(&mut base, PlayerId::P1, "Tinder Wall", Zone::Battlefield);
    base.objects.get_mut(target).counters.plus1_plus1 = 20;
    add_mana(&mut base, 1, 1);
    assert!(matches!(
        cast_monstrous_through_target(&mut base, spell, target),
        Decision::ChooseEffectOption {
            option_count: 2,
            ..
        }
    ));

    let mut battlefield = base.clone();
    engine::step(&mut battlefield, Action::ChooseEffectOption(0)).unwrap();
    let battlefield_decision = engine::advance_until_decision(&mut battlefield);
    let battlefield_actions = legal_action_candidates_v1(
        &SurfaceDecision::Decision(battlefield_decision),
        &battlefield,
    )
    .unwrap();
    assert_eq!(battlefield_actions.len(), 2);
    for action in &battlefield_actions {
        match &action.record.semantic {
            ActionSemanticV1::ChooseCostTarget {
                cost_kind: CostKind::ChooseCreatureOrRevealCreature,
                candidate,
                ..
            } => {
                assert_eq!(candidate.zone, Zone::Battlefield);
                assert!([field_a.0, field_b.0].contains(&candidate.arena_id));
            }
            other => panic!("unexpected battlefield cost semantic {other:?}"),
        }
    }

    let mut hand = base;
    engine::step(&mut hand, Action::ChooseEffectOption(1)).unwrap();
    let hand_decision = engine::advance_until_decision(&mut hand);
    let hand_actions =
        legal_action_candidates_v1(&SurfaceDecision::Decision(hand_decision), &hand).unwrap();
    assert_eq!(hand_actions.len(), 2);
    for action in &hand_actions {
        match &action.record.semantic {
            ActionSemanticV1::ChooseCostTarget {
                cost_kind: CostKind::ChooseCreatureOrRevealCreature,
                candidate,
                ..
            } => {
                assert_eq!(candidate.zone, Zone::Hand);
                assert!([hand_a.0, hand_b.0].contains(&candidate.arena_id));
            }
            other => panic!("unexpected hand cost semantic {other:?}"),
        }
    }
    assert_ne!(
        battlefield_actions[0].record.stable_id,
        hand_actions[0].record.stable_id
    );

    engine::step(&mut hand, Action::ChooseCostTarget(hand_a)).unwrap();
    engine::advance_until_decision(&mut hand);
    let known = hand.known_hand_cards(PlayerId::P1, PlayerId::P0);
    assert_eq!(known.len(), 1);
    assert_eq!(known[0].object, hand_a);
    assert!(!known.iter().any(|entry| entry.object == hand_b));
    assert_eq!(
        hand.stack.last().unwrap().v4.paid_cost_refs[0].object,
        hand_a
    );
    pass_until_idle(&mut hand);
    assert_eq!(hand.objects.get(target).damage, 5);
}

#[test]
fn nyxborn_normal_and_bestow_bind_x_and_live_counter_bonus() {
    let mut normal = ready_state(0x4e59_5842_4f52_0001);
    let hydra = put_object(&mut normal, PlayerId::P0, "Nyxborn Hydra", Zone::Hand);
    let _host = put_object(
        &mut normal,
        PlayerId::P0,
        "Avenging Hunter",
        Zone::Battlefield,
    );
    add_mana(&mut normal, 2, 3);
    cast_hydra(&mut normal, hydra, 0, None, 3);
    assert_eq!(normal.stack.last().unwrap().v4.x_value, 3);
    assert_eq!(
        normal.stack.last().unwrap().v4.cast_method,
        Some(CastMethodV4::Normal)
    );
    pass_until_idle(&mut normal);
    assert_eq!(normal.objects.get(hydra).zone, Zone::Battlefield);
    assert_eq!(normal.objects.get(hydra).counters.plus1_plus1, 3);
    assert_eq!(engine::effective_power(&normal, hydra), 3);
    assert_eq!(engine::effective_toughness(&normal, hydra), 4);
    assert!(engine::has_effective_keyword(
        &normal,
        hydra,
        Keywords::REACH
    ));
    assert!(engine::has_effective_keyword(
        &normal,
        hydra,
        Keywords::TRAMPLE
    ));

    let mut bestow = ready_state(0x4e59_5842_4f52_0002);
    let hydra = put_object(&mut bestow, PlayerId::P0, "Nyxborn Hydra", Zone::Hand);
    let host = put_object(
        &mut bestow,
        PlayerId::P0,
        "Avenging Hunter",
        Zone::Battlefield,
    );
    add_mana(&mut bestow, 2, 3);
    cast_hydra(&mut bestow, hydra, 1, Some(host), 3);
    assert_eq!(
        bestow.stack.last().unwrap().v4.cast_method,
        Some(CastMethodV4::Bestow)
    );
    pass_until_idle(&mut bestow);
    assert_eq!(
        bestow.objects.get(hydra).v4.attached_to.unwrap().object,
        host
    );
    assert_eq!(bestow.objects.get(hydra).counters.plus1_plus1, 3);
    assert!(!engine::object_has_type(&bestow, hydra, CardType::Creature));
    assert!(engine::object_has_type(
        &bestow,
        hydra,
        CardType::Enchantment
    ));
    assert_eq!(engine::effective_power(&bestow, host), 8);
    assert_eq!(engine::effective_toughness(&bestow, host), 7);
    assert!(engine::has_effective_keyword(
        &bestow,
        host,
        Keywords::REACH
    ));
    assert!(engine::has_effective_keyword(
        &bestow,
        host,
        Keywords::TRAMPLE
    ));

    bestow.objects.get_mut(hydra).counters.plus1_plus1 = 5;
    assert_eq!(engine::effective_power(&bestow, host), 10);
    event::propose_and_commit(
        &mut bestow,
        ProposedEvent::zone_change(host, Zone::Graveyard),
    );
    assert!(bestow.objects.get(hydra).v4.attached_to.is_none());
    assert!(engine::object_has_type(&bestow, hydra, CardType::Creature));
    assert_eq!(bestow.objects.get(hydra).counters.plus1_plus1, 5);
    assert_eq!(engine::effective_power(&bestow, hydra), 5);
    assert_eq!(engine::effective_toughness(&bestow, hydra), 6);
}

#[test]
fn nyxborn_bestow_is_a_noncreature_aura_spell_for_stack_targeting() {
    let mut normal = ready_state(0x4e59_5842_4f52_0004);
    let hydra = put_object(&mut normal, PlayerId::P0, "Nyxborn Hydra", Zone::Hand);
    let _host = put_object(
        &mut normal,
        PlayerId::P0,
        "Avenging Hunter",
        Zone::Battlefield,
    );
    let pierce = put_object(&mut normal, PlayerId::P1, "Spell Pierce", Zone::Hand);
    normal.players[PlayerId::P1.index()].mana_pool[ManaColor::U.pool_index()] = 1;
    add_mana(&mut normal, 2, 2);
    cast_hydra(&mut normal, hydra, 0, None, 2);
    engine::step(&mut normal, Action::Pass).unwrap();
    let Decision::CastSpellOrPass {
        player,
        castable_spells,
        ..
    } = engine::advance_until_decision(&mut normal)
    else {
        panic!("opponent should receive priority over normal Nyxborn")
    };
    assert_eq!(player, PlayerId::P1);
    assert!(!castable_spells.contains(&pierce));

    let mut bestow = ready_state(0x4e59_5842_4f52_0005);
    let hydra = put_object(&mut bestow, PlayerId::P0, "Nyxborn Hydra", Zone::Hand);
    let host = put_object(
        &mut bestow,
        PlayerId::P0,
        "Avenging Hunter",
        Zone::Battlefield,
    );
    let pierce = put_object(&mut bestow, PlayerId::P1, "Spell Pierce", Zone::Hand);
    bestow.players[PlayerId::P1.index()].mana_pool[ManaColor::U.pool_index()] = 1;
    add_mana(&mut bestow, 2, 2);
    cast_hydra(&mut bestow, hydra, 1, Some(host), 2);
    engine::step(&mut bestow, Action::Pass).unwrap();
    let Decision::CastSpellOrPass {
        player,
        castable_spells,
        ..
    } = engine::advance_until_decision(&mut bestow)
    else {
        panic!("opponent should receive priority over bestowed Nyxborn")
    };
    assert_eq!(player, PlayerId::P1);
    assert!(castable_spells.contains(&pierce));
    engine::step(&mut bestow, Action::CastSpell(pierce)).unwrap();
    let Decision::ChooseTargets { legal_targets, .. } = engine::advance_until_decision(&mut bestow)
    else {
        panic!("Spell Pierce should target the Bestow Aura spell")
    };
    assert_eq!(legal_targets, vec![Target::Object(hydra)]);
}

#[test]
fn nyxborn_stack_mana_value_is_printed_x_plus_one_for_normal_and_bestow() {
    let spec = TargetSpec::SpellManaValueAtMostControlledSubtypes {
        first: Subtype::Faerie,
        second: Some(Subtype::FaerieAllCaps),
    };

    let mut normal = ready_state(0x4e59_5842_4f52_0006);
    let hydra = put_object(&mut normal, PlayerId::P0, "Nyxborn Hydra", Zone::Hand);
    let _host = put_object(
        &mut normal,
        PlayerId::P0,
        "Avenging Hunter",
        Zone::Battlefield,
    );
    put_object(&mut normal, PlayerId::P1, "Faerie Seer", Zone::Battlefield);
    put_object(
        &mut normal,
        PlayerId::P1,
        "Spellstutter Sprite",
        Zone::Battlefield,
    );
    add_mana(&mut normal, 2, 2);
    cast_hydra(&mut normal, hydra, 0, None, 2);
    normal.priority_player = PlayerId::P1;
    assert_eq!(
        engine::legal_targets_for(spec, &[], &normal),
        Vec::<Target>::new()
    );

    let mut bestow = ready_state(0x4e59_5842_4f52_0007);
    let hydra = put_object(&mut bestow, PlayerId::P0, "Nyxborn Hydra", Zone::Hand);
    let host = put_object(
        &mut bestow,
        PlayerId::P0,
        "Avenging Hunter",
        Zone::Battlefield,
    );
    put_object(&mut bestow, PlayerId::P1, "Faerie Seer", Zone::Battlefield);
    put_object(
        &mut bestow,
        PlayerId::P1,
        "Faerie Miscreant",
        Zone::Battlefield,
    );
    put_object(
        &mut bestow,
        PlayerId::P1,
        "Spellstutter Sprite",
        Zone::Battlefield,
    );
    add_mana(&mut bestow, 2, 2);
    cast_hydra(&mut bestow, hydra, 1, Some(host), 2);
    bestow.priority_player = PlayerId::P1;
    assert_eq!(
        engine::legal_targets_for(spec, &[], &bestow),
        vec![Target::Object(hydra)],
        "Bestow's XGG payment does not replace printed XG mana value"
    );
}

#[test]
fn nyxborn_illegal_bestow_target_falls_back_and_stack_tamper_fails_closed() {
    let mut state = ready_state(0x4e59_5842_4f52_0003);
    let hydra = put_object(&mut state, PlayerId::P0, "Nyxborn Hydra", Zone::Hand);
    let host = put_object(
        &mut state,
        PlayerId::P0,
        "Avenging Hunter",
        Zone::Battlefield,
    );
    add_mana(&mut state, 2, 2);
    cast_hydra(&mut state, hydra, 1, Some(host), 2);

    let mut x_tamper = state.clone();
    x_tamper.stack.last_mut().unwrap().v4.x_value = 3;
    assert_invalid_continuation(&mut x_tamper);

    let mut method_tamper = state.clone();
    method_tamper.stack.last_mut().unwrap().v4.cast_method = Some(CastMethodV4::Normal);
    assert_invalid_continuation(&mut method_tamper);

    let other = put_object(&mut state, PlayerId::P1, "Sacred Cat", Zone::Battlefield);
    let mut target_tamper = state.clone();
    target_tamper.stack.last_mut().unwrap().targets[0] = Target::Object(other);
    assert_invalid_continuation(&mut target_tamper);

    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(host, Zone::Graveyard),
    );
    pass_until_idle(&mut state);
    assert_eq!(state.objects.get(hydra).zone, Zone::Battlefield);
    assert_eq!(state.objects.get(hydra).counters.plus1_plus1, 2);
    assert!(state.objects.get(hydra).v4.attached_to.is_none());
    assert!(engine::object_has_type(&state, hydra, CardType::Creature));
    assert_eq!(state.objects.get(other).zone, Zone::Battlefield);
}
