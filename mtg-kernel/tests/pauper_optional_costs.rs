//! Exact Mage parity for the closed-pool optional-cost wave.
//!
//! Collect Evidence and Bargain are announcement-time optional costs. Their
//! physical payments remain bound to the producing stack incarnation. Masked
//! Vandal instead pays its optional creature-card exile during ETB resolution.

use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardType, Keywords, OptionalAdditionalCostDef, Subtype,
    TargetSpec, CARD_DEFS,
};
use mtg_kernel::effect::{
    CreatureSacrificeFilter, EffectCond, EffectOp, PendingEffectChoice, PlayerRef,
};
use mtg_kernel::engine::{self, Action, CostKind, Decision, UnsupportedMechanic};
use mtg_kernel::event::{self, CommittedEvent, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::rl::{legal_action_candidates_v1, observe_v2, ActionSemanticV1};
use mtg_kernel::state::{Counters, GameObject, GameState, ObjectStateV4, Step, Target, Zone};
use mtg_kernel::surface_v2::{HarnessSurfaceV2, SurfaceDecision};
use mtg_kernel::trigger;

fn card_id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name} in CARD_DEFS"))
}

fn put_object_as(
    state: &mut GameState,
    owner: PlayerId,
    controller: PlayerId,
    name: &str,
    zone: Zone,
) -> ObjectId {
    let card_def = card_id(name);
    let object = state.objects.push(GameObject {
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
        Zone::Hand => state.players[owner.index()].hand.push(object),
        Zone::Battlefield => state.players[controller.index()].battlefield.push(object),
        Zone::Library => state.players[owner.index()].library.push(object),
        Zone::Graveyard => state.players[owner.index()].graveyard.push(object),
        Zone::Exile => state.exile.push(object),
        Zone::Command => state.command.push(object),
        Zone::Stack => panic!("test helper does not construct stack objects"),
    }
    object
}

fn put_object(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    put_object_as(state, player, player, name, zone)
}

fn ready_main(seed: u64) -> GameState {
    let mut state = GameState::new_from_libraries(&[], &[], |_| String::new(), seed);
    state.step = Step::Main1;
    state.active_player = PlayerId::P0;
    state.priority_player = PlayerId::P0;
    state
}

fn add_mana(state: &mut GameState, color: ManaColor, amount: u8) {
    state.players[PlayerId::P0.index()].mana_pool[color.pool_index()] += amount;
}

fn pass_until<F>(state: &mut GameState, mut done: F) -> Decision
where
    F: FnMut(&GameState, &Decision) -> bool,
{
    for _ in 0..96 {
        let decision = engine::advance_until_decision(state);
        if done(state, &decision) {
            return decision;
        }
        match decision {
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => panic!("unexpected decision during bounded resolution: {other:?}"),
        }
    }
    panic!("bounded resolution did not reach the expected state");
}

fn choose_cast_optional_cost(state: &mut GameState, pay: bool) {
    let decision = engine::advance_until_decision(state);
    assert!(matches!(
        decision,
        Decision::ChooseEffectOption {
            player: PlayerId::P0,
            option_count: 2,
            ..
        }
    ));
    engine::step(state, Action::ChooseEffectOption(u16::from(pay))).unwrap();
}

fn finish_collect_evidence_with(state: &mut GameState, cards: &[ObjectId]) {
    for &card in cards {
        let Decision::ChooseEffectTargets { legal_targets, .. } =
            engine::advance_until_decision(state)
        else {
            panic!("Collect Evidence must expose its exact graveyard candidates")
        };
        if legal_targets.contains(&Target::Object(card)) {
            engine::step(state, Action::ChooseEffectTarget(Target::Object(card))).unwrap();
        } else {
            assert!(state
                .engine
                .pending_cast
                .as_ref()
                .unwrap()
                .optional_additional_cost_chosen
                .iter()
                .any(|binding| binding.object == card));
        }
    }
    let Decision::ChooseEffectTargets {
        can_finish: true, ..
    } = engine::advance_until_decision(state)
    else {
        panic!("Collect Evidence must require an explicit finish once its sum is sufficient")
    };
    engine::step(state, Action::FinishEffectSelection).unwrap();
    assert!(matches!(
        engine::advance_until_decision(state),
        Decision::CastSpellOrPass { .. }
    ));
}

#[test]
fn definitions_ids_and_append_only_grammar_match_checked_in_mage() {
    assert_eq!(card_id("Extract a Confession"), 31);
    assert_eq!(card_id("Masked Vandal"), 71);
    assert_eq!(card_id("Troublemaker Ouphe"), 122);
    assert_eq!(card_id("Vitu-Ghazi Inspector"), 126);
    assert_eq!(CARD_DEFS.len(), 161);
    assert_eq!(
        TargetSpec::OpponentArtifactOrEnchantmentPermanent.stable_id(),
        35
    );

    let collect = Some(OptionalAdditionalCostDef::CollectEvidence {
        minimum_mana_value: 6,
    });
    let extract = &CARD_DEFS[card_id("Extract a Confession") as usize];
    assert_eq!(extract.capability, CardCapability::Full);
    assert_eq!(extract.optional_additional_cost, collect);
    assert_eq!(extract.target_spec, TargetSpec::None);
    assert_eq!(
        (extract.spell_effect)(),
        Some(EffectOp::Conditional {
            cond: EffectCond::OptionalAdditionalCostPaid(collect.unwrap()),
            then: Box::new(EffectOp::SacrificeCreature {
                player: PlayerRef::Opponent,
                filter: CreatureSacrificeFilter::GreatestPower,
            }),
            else_: Box::new(EffectOp::SacrificeCreature {
                player: PlayerRef::Opponent,
                filter: CreatureSacrificeFilter::Any,
            }),
        })
    );

    let vandal = &CARD_DEFS[card_id("Masked Vandal") as usize];
    assert_eq!(vandal.capability, CardCapability::Full);
    assert_eq!(vandal.types, &[CardType::Creature]);
    assert_eq!(vandal.subtypes, &[Subtype::Shapeshifter]);
    assert!(vandal.changeling);
    assert_eq!(vandal.optional_additional_cost, None);
    assert_eq!(
        trigger::trigger_target_spec(card_id("Masked Vandal")),
        TargetSpec::OpponentArtifactOrEnchantmentPermanent
    );

    let ouphe = &CARD_DEFS[card_id("Troublemaker Ouphe") as usize];
    assert_eq!(ouphe.capability, CardCapability::Full);
    assert_eq!(
        ouphe.optional_additional_cost,
        Some(OptionalAdditionalCostDef::Bargain)
    );
    assert_eq!((ouphe.power, ouphe.toughness), (Some(2), Some(2)));

    let inspector = &CARD_DEFS[card_id("Vitu-Ghazi Inspector") as usize];
    assert_eq!(inspector.capability, CardCapability::Full);
    assert_eq!(inspector.optional_additional_cost, collect);
    assert_eq!((inspector.power, inspector.toughness), (Some(1), Some(3)));
    assert!(inspector.keywords.has(Keywords::REACH));
    assert_eq!(
        trigger::trigger_target_spec(card_id("Vitu-Ghazi Inspector")),
        TargetSpec::Creature
    );
}

#[test]
fn changeling_materializes_every_closed_creature_type_and_no_noncreature_type() {
    let object = ObjectStateV4::from_card_def(card_id("Masked Vandal"));
    for creature_type in Subtype::CREATURE_TYPES {
        assert!(creature_type.is_creature_type());
        assert!(
            object
                .effective_subtype_ids
                .contains(&creature_type.stable_id()),
            "Changeling omitted {creature_type:?}"
        );
    }
    for noncreature in [
        Subtype::Aura,
        Subtype::Equipment,
        Subtype::Forest,
        Subtype::Gate,
        Subtype::Food,
        Subtype::Treasure,
    ] {
        assert!(!noncreature.is_creature_type());
        assert!(!object
            .effective_subtype_ids
            .contains(&noncreature.stable_id()));
    }
}

#[test]
fn extract_declined_lets_opponent_choose_any_creature() {
    let mut state = ready_main(0x4558_5452_4143_5401);
    add_mana(&mut state, ManaColor::B, 2);
    let extract = put_object(&mut state, PlayerId::P0, "Extract a Confession", Zone::Hand);
    let evidence = put_object(&mut state, PlayerId::P0, "Fireblast", Zone::Graveyard);
    let small = put_object(&mut state, PlayerId::P1, "Elvish Mystic", Zone::Battlefield);
    let greatest = put_object(&mut state, PlayerId::P1, "Myr Enforcer", Zone::Battlefield);

    engine::step(&mut state, Action::CastSpell(extract)).unwrap();
    choose_cast_optional_cost(&mut state, false);
    let sacrifice = pass_until(&mut state, |_, decision| {
        matches!(
            decision,
            Decision::ChooseEffectTargets {
                player: PlayerId::P1,
                ..
            }
        )
    });
    let Decision::ChooseEffectTargets { legal_targets, .. } = sacrifice else {
        unreachable!()
    };
    assert_eq!(
        legal_targets,
        vec![Target::Object(small), Target::Object(greatest)]
    );
    engine::step(
        &mut state,
        Action::ChooseEffectTarget(Target::Object(small)),
    )
    .unwrap();
    pass_until(&mut state, |state, _| state.stack.is_empty());

    assert_eq!(state.objects.get(small).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(greatest).zone, Zone::Battlefield);
    assert_eq!(state.objects.get(evidence).zone, Zone::Graveyard);
    assert!(state.engine.event_history.iter().any(
        |event| matches!(event, CommittedEvent::Sacrificed { object, .. } if *object == small)
    ));
}

#[test]
fn collect_evidence_is_exact_owned_sum_bound_and_extract_uses_greatest_power() {
    let mut state = ready_main(0x4558_5452_4143_5402);
    add_mana(&mut state, ManaColor::B, 2);
    let extract = put_object(&mut state, PlayerId::P0, "Extract a Confession", Zone::Hand);
    let evidence = put_object(&mut state, PlayerId::P0, "Fireblast", Zone::Graveyard);
    let extra = put_object(&mut state, PlayerId::P0, "Lightning Bolt", Zone::Graveyard);
    let own_token = put_object(
        &mut state,
        PlayerId::P0,
        "Human Soldier Token",
        Zone::Graveyard,
    );
    let opposing = put_object(&mut state, PlayerId::P1, "Cryptic Serpent", Zone::Graveyard);
    let small = put_object(&mut state, PlayerId::P1, "Elvish Mystic", Zone::Battlefield);
    let greatest = put_object(&mut state, PlayerId::P1, "Myr Enforcer", Zone::Battlefield);
    let greatest_tie = put_object(&mut state, PlayerId::P1, "Myr Enforcer", Zone::Battlefield);

    engine::step(&mut state, Action::CastSpell(extract)).unwrap();
    let offer = engine::advance_until_decision(&mut state);
    let actions = legal_action_candidates_v1(&SurfaceDecision::Decision(offer.clone()), &state)
        .expect("existing generic option grammar projects the cast choice");
    assert_eq!(actions.len(), 2);
    assert!(actions.iter().all(|candidate| matches!(
        candidate.record.semantic,
        ActionSemanticV1::ChooseEffectOption {
            option_count: 2,
            ..
        }
    )));
    engine::step(&mut state, Action::ChooseEffectOption(1)).unwrap();

    let Decision::ChooseEffectTargets {
        legal_targets,
        can_finish,
        ..
    } = engine::advance_until_decision(&mut state)
    else {
        panic!("Collect Evidence target selection")
    };
    assert!(!can_finish);
    assert_eq!(
        legal_targets,
        vec![Target::Object(evidence), Target::Object(extra)]
    );
    assert!(!legal_targets.contains(&Target::Object(own_token)));
    assert!(!legal_targets.contains(&Target::Object(opposing)));
    engine::step(
        &mut state,
        Action::ChooseEffectTarget(Target::Object(evidence)),
    )
    .unwrap();

    let snapshot = serde_json::to_vec(&state).unwrap();
    let mut restored: GameState = serde_json::from_slice(&snapshot).unwrap();
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
        Decision::ChooseEffectTargets {
            can_finish: true,
            ..
        }
    ));

    let observer = observe_v2(&state, &HarnessSurfaceV2::new(), PlayerId::P1, 17).unwrap();
    let public_pending = serde_json::to_value(
        observer
            .projection
            .engine_context
            .pending_cast
            .as_ref()
            .unwrap(),
    )
    .unwrap();
    assert!(public_pending
        .get("optional_additional_cost_paid")
        .is_none());
    assert!(public_pending
        .get("optional_additional_cost_chosen")
        .is_none());
    assert!(public_pending
        .get("optional_additional_cost_selection_finished")
        .is_none());

    engine::step(&mut state, Action::FinishEffectSelection).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    let spell = state.stack.last().expect("Extract is announced");
    assert_eq!(
        spell.v4.optional_additional_cost_paid,
        Some(OptionalAdditionalCostDef::CollectEvidence {
            minimum_mana_value: 6,
        })
    );
    assert_eq!(spell.v4.paid_cost_refs.len(), 1);
    assert_eq!(spell.v4.paid_cost_refs[0].object, evidence);
    assert_eq!(spell.v4.paid_cost_refs[0].zone, Zone::Exile);
    assert_eq!(spell.v4.paid_cost_refs[0].visible_to_mask, 0b11);
    assert_eq!(state.objects.get(evidence).zone, Zone::Exile);
    assert_eq!(state.objects.get(extra).zone, Zone::Graveyard);

    let sacrifice = pass_until(&mut state, |_, decision| {
        matches!(
            decision,
            Decision::ChooseEffectTargets {
                player: PlayerId::P1,
                ..
            }
        )
    });
    let Decision::ChooseEffectTargets { legal_targets, .. } = sacrifice else {
        unreachable!()
    };
    assert_eq!(
        legal_targets,
        vec![Target::Object(greatest), Target::Object(greatest_tie)]
    );
    assert!(!legal_targets.contains(&Target::Object(small)));
    engine::step(
        &mut state,
        Action::ChooseEffectTarget(Target::Object(greatest)),
    )
    .unwrap();
    pass_until(&mut state, |state, _| state.stack.is_empty());
    assert_eq!(state.objects.get(greatest).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(greatest_tie).zone, Zone::Battlefield);
    assert_eq!(state.objects.get(small).zone, Zone::Battlefield);
    assert!(!state
        .engine
        .event_history
        .iter()
        .any(|event| matches!(event, CommittedEvent::OptionalAdditionalCostPaid { .. })));
}

#[test]
fn collect_evidence_rejects_insufficient_opponent_stale_and_tampered_picks_atomically() {
    let mut state = ready_main(0x4556_4944_454e_4345);
    add_mana(&mut state, ManaColor::B, 2);
    let extract = put_object(&mut state, PlayerId::P0, "Extract a Confession", Zone::Hand);
    let two = put_object(&mut state, PlayerId::P0, "Counterspell", Zone::Graveyard);
    let four = put_object(&mut state, PlayerId::P0, "Deem Inferior", Zone::Graveyard);
    let opponent = put_object(&mut state, PlayerId::P1, "Fireblast", Zone::Graveyard);
    put_object(&mut state, PlayerId::P1, "Elvish Mystic", Zone::Battlefield);

    engine::step(&mut state, Action::CastSpell(extract)).unwrap();
    choose_cast_optional_cost(&mut state, true);
    engine::step(&mut state, Action::ChooseEffectTarget(Target::Object(two))).unwrap();

    let insufficient = state.clone();
    assert!(engine::step(&mut state, Action::FinishEffectSelection).is_err());
    assert_eq!(state, insufficient);
    assert!(engine::step(
        &mut state,
        Action::ChooseEffectTarget(Target::Object(opponent))
    )
    .is_err());
    assert_eq!(state, insufficient);

    let mut tampered = state.clone();
    tampered
        .engine
        .pending_cast
        .as_mut()
        .unwrap()
        .optional_additional_cost_chosen[0]
        .expected_zone_change_count += 1;
    let before_tamper_action = tampered.clone();
    assert!(engine::step(
        &mut tampered,
        Action::ChooseEffectTarget(Target::Object(four))
    )
    .is_err());
    assert_eq!(tampered, before_tamper_action);

    event::propose_and_commit(&mut state, ProposedEvent::zone_change(two, Zone::Exile));
    let stale = state.clone();
    assert!(engine::step(&mut state, Action::ChooseEffectTarget(Target::Object(four))).is_err());
    assert_eq!(state, stale);
    assert_eq!(state.objects.get(four).zone, Zone::Graveyard);
}

#[test]
fn bargain_accepts_only_controlled_artifact_enchantment_or_token_and_ouphe_exiles_target() {
    let mut state = ready_main(0x4241_5247_4149_4e01);
    add_mana(&mut state, ManaColor::G, 2);
    let ouphe = put_object(&mut state, PlayerId::P0, "Troublemaker Ouphe", Zone::Hand);
    let artifact = put_object(&mut state, PlayerId::P0, "Great Furnace", Zone::Battlefield);
    let token = put_object(
        &mut state,
        PlayerId::P0,
        "Human Soldier Token",
        Zone::Battlefield,
    );
    let enchantment = put_object(
        &mut state,
        PlayerId::P0,
        "Makeshift Munitions",
        Zone::Battlefield,
    );
    let ordinary = put_object(&mut state, PlayerId::P0, "Elvish Mystic", Zone::Battlefield);
    let opponent_artifact =
        put_object(&mut state, PlayerId::P1, "Great Furnace", Zone::Battlefield);

    engine::step(&mut state, Action::CastSpell(ouphe)).unwrap();
    choose_cast_optional_cost(&mut state, true);
    let Decision::ChooseCostTargets {
        cost_kind,
        candidates,
        remaining: 1,
        ..
    } = engine::advance_until_decision(&mut state)
    else {
        panic!("Bargain permanent selection")
    };
    assert_eq!(cost_kind, CostKind::SacrificePermanents);
    assert_eq!(candidates, vec![artifact, token, enchantment]);
    assert!(!candidates.contains(&ordinary));
    assert!(!candidates.contains(&opponent_artifact));
    engine::step(&mut state, Action::ChooseCostTarget(token)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));

    let spell = state.stack.last().expect("Ouphe announced");
    assert_eq!(
        spell.v4.optional_additional_cost_paid,
        Some(OptionalAdditionalCostDef::Bargain)
    );
    assert_eq!(spell.v4.paid_cost_refs.len(), 1);
    assert_eq!(spell.v4.paid_cost_refs[0].object, token);
    assert_eq!(spell.v4.paid_cost_refs[0].zone, Zone::Graveyard);

    let target = pass_until(
        &mut state,
        |_, decision| matches!(decision, Decision::ChooseTargets { spell, .. } if *spell == ouphe),
    );
    let Decision::ChooseTargets { legal_targets, .. } = target else {
        unreachable!()
    };
    assert_eq!(legal_targets, vec![Target::Object(opponent_artifact)]);
    engine::step(
        &mut state,
        Action::ChooseTarget(Target::Object(opponent_artifact)),
    )
    .unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    let trigger = state.stack.last().expect("paid Ouphe ETB trigger");
    assert_eq!(
        trigger.v4.optional_additional_cost_paid,
        Some(OptionalAdditionalCostDef::Bargain)
    );
    assert_eq!(trigger.v4.paid_cost_refs[0].object, token);

    let bytes = serde_json::to_vec(&state).unwrap();
    let restored: GameState = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(restored, state);
    assert_eq!(
        restored.diagnostic_state_hash(),
        state.diagnostic_state_hash()
    );

    pass_until(&mut state, |state, _| state.stack.is_empty());
    assert_eq!(state.objects.get(opponent_artifact).zone, Zone::Exile);
    assert_eq!(state.objects.get(artifact).zone, Zone::Battlefield);
    assert_eq!(state.objects.get(enchantment).zone, Zone::Battlefield);
}

#[test]
fn bargain_intervening_if_is_checked_at_trigger_time_and_resolution() {
    let mut unpaid = ready_main(0x4241_5247_4149_4e02);
    add_mana(&mut unpaid, ManaColor::G, 2);
    let ouphe = put_object(&mut unpaid, PlayerId::P0, "Troublemaker Ouphe", Zone::Hand);
    put_object(
        &mut unpaid,
        PlayerId::P0,
        "Great Furnace",
        Zone::Battlefield,
    );
    let target = put_object(
        &mut unpaid,
        PlayerId::P1,
        "Great Furnace",
        Zone::Battlefield,
    );
    engine::step(&mut unpaid, Action::CastSpell(ouphe)).unwrap();
    choose_cast_optional_cost(&mut unpaid, false);
    pass_until(&mut unpaid, |state, _| state.stack.is_empty());
    assert_eq!(unpaid.objects.get(target).zone, Zone::Battlefield);
    assert!(unpaid.engine.pending_triggers.is_empty());

    let mut paid = ready_main(0x4241_5247_4149_4e03);
    add_mana(&mut paid, ManaColor::G, 2);
    let ouphe = put_object(&mut paid, PlayerId::P0, "Troublemaker Ouphe", Zone::Hand);
    put_object(&mut paid, PlayerId::P0, "Great Furnace", Zone::Battlefield);
    let token = put_object(
        &mut paid,
        PlayerId::P0,
        "Human Soldier Token",
        Zone::Battlefield,
    );
    let target = put_object(&mut paid, PlayerId::P1, "Great Furnace", Zone::Battlefield);
    engine::step(&mut paid, Action::CastSpell(ouphe)).unwrap();
    choose_cast_optional_cost(&mut paid, true);
    let Decision::ChooseCostTargets { .. } = engine::advance_until_decision(&mut paid) else {
        panic!("Bargain selection")
    };
    engine::step(&mut paid, Action::ChooseCostTarget(token)).unwrap();
    let target_decision = pass_until(
        &mut paid,
        |_, decision| matches!(decision, Decision::ChooseTargets { spell, .. } if *spell == ouphe),
    );
    assert!(matches!(target_decision, Decision::ChooseTargets { .. }));
    engine::step(&mut paid, Action::ChooseTarget(Target::Object(target))).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut paid),
        Decision::CastSpellOrPass { .. }
    ));

    paid.stack
        .last_mut()
        .unwrap()
        .v4
        .optional_additional_cost_paid = Some(OptionalAdditionalCostDef::CollectEvidence {
        minimum_mana_value: 6,
    });
    pass_until(&mut paid, |_, decision| {
        matches!(decision, Decision::Halted { .. })
    });
    assert_eq!(
        paid.engine.halted,
        Some((UnsupportedMechanic::InvalidEffectContinuation, ouphe))
    );
    assert_eq!(paid.objects.get(target).zone, Zone::Battlefield);
}

#[test]
fn masked_vandal_targets_first_then_pays_exact_creature_card_exile() {
    let mut state = ready_main(0x4d41_534b_4544_0001);
    add_mana(&mut state, ManaColor::G, 2);
    let vandal = put_object(&mut state, PlayerId::P0, "Masked Vandal", Zone::Hand);
    let first = put_object(&mut state, PlayerId::P0, "Elvish Mystic", Zone::Graveyard);
    let second = put_object(&mut state, PlayerId::P0, "Myr Enforcer", Zone::Graveyard);
    let token = put_object(
        &mut state,
        PlayerId::P0,
        "Human Soldier Token",
        Zone::Graveyard,
    );
    let noncreature = put_object(&mut state, PlayerId::P0, "Counterspell", Zone::Graveyard);
    let opposing_creature = put_object(&mut state, PlayerId::P1, "Elvish Mystic", Zone::Graveyard);
    let target = put_object(&mut state, PlayerId::P1, "Great Furnace", Zone::Battlefield);

    engine::step(&mut state, Action::CastSpell(vandal)).unwrap();
    let target_choice = pass_until(
        &mut state,
        |_, decision| matches!(decision, Decision::ChooseTargets { spell, .. } if *spell == vandal),
    );
    assert!(matches!(target_choice, Decision::ChooseTargets { .. }));
    assert_eq!(state.objects.get(first).zone, Zone::Graveyard);
    engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();

    let pay = pass_until(&mut state, |_, decision| {
        matches!(decision, Decision::ChooseEffectBoolean { .. })
    });
    let projected = legal_action_candidates_v1(&SurfaceDecision::Decision(pay), &state).unwrap();
    assert_eq!(projected.len(), 2);
    assert!(matches!(
        projected[0].record.semantic,
        ActionSemanticV1::ChooseEffectBoolean { value: false, .. }
    ));
    assert!(matches!(
        projected[1].record.semantic,
        ActionSemanticV1::ChooseEffectBoolean { value: true, .. }
    ));
    engine::step(&mut state, Action::ChooseEffectBoolean(true)).unwrap();

    let selection = engine::advance_until_decision(&mut state);
    let Decision::ChooseEffectTargets { legal_targets, .. } = &selection else {
        panic!("Masked Vandal creature-card payment selection")
    };
    assert_eq!(
        *legal_targets,
        vec![Target::Object(first), Target::Object(second)]
    );
    assert!(!legal_targets.contains(&Target::Object(noncreature)));
    assert!(!legal_targets.contains(&Target::Object(token)));
    assert!(!legal_targets.contains(&Target::Object(opposing_creature)));

    let snapshot = serde_json::to_vec(&state).unwrap();
    let mut restored: GameState = serde_json::from_slice(&snapshot).unwrap();
    assert_eq!(engine::advance_until_decision(&mut restored), selection);
    assert_eq!(restored, state);

    engine::step(
        &mut state,
        Action::ChooseEffectTarget(Target::Object(second)),
    )
    .unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    assert_eq!(state.objects.get(second).zone, Zone::Exile);
    assert_eq!(state.objects.get(first).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(target).zone, Zone::Exile);
}

#[test]
fn masked_vandal_decline_empty_stale_and_tampered_costs_do_not_exile_target() {
    let make_pending = |seed| {
        let mut state = ready_main(seed);
        add_mana(&mut state, ManaColor::G, 2);
        let vandal = put_object(&mut state, PlayerId::P0, "Masked Vandal", Zone::Hand);
        let first = put_object(&mut state, PlayerId::P0, "Elvish Mystic", Zone::Graveyard);
        let second = put_object(&mut state, PlayerId::P0, "Myr Enforcer", Zone::Graveyard);
        let target = put_object(&mut state, PlayerId::P1, "Great Furnace", Zone::Battlefield);
        engine::step(&mut state, Action::CastSpell(vandal)).unwrap();
        pass_until(
            &mut state,
            |_, decision| matches!(decision, Decision::ChooseTargets { spell, .. } if *spell == vandal),
        );
        engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
        pass_until(&mut state, |_, decision| {
            matches!(decision, Decision::ChooseEffectBoolean { .. })
        });
        (state, vandal, first, second, target)
    };

    let (mut declined, _, first, _, target) = make_pending(0x4d41_534b_4544_0002);
    engine::step(&mut declined, Action::ChooseEffectBoolean(false)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut declined),
        Decision::CastSpellOrPass { .. }
    ));
    assert_eq!(declined.objects.get(first).zone, Zone::Graveyard);
    assert_eq!(declined.objects.get(target).zone, Zone::Battlefield);

    let (mut stale, _, first, _, target) = make_pending(0x4d41_534b_4544_0003);
    engine::step(&mut stale, Action::ChooseEffectBoolean(true)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut stale),
        Decision::ChooseEffectTargets { .. }
    ));
    event::propose_and_commit(&mut stale, ProposedEvent::zone_change(first, Zone::Exile));
    let stale_before = stale.clone();
    assert!(engine::step(
        &mut stale,
        Action::ChooseEffectTarget(Target::Object(first))
    )
    .is_err());
    assert_eq!(stale, stale_before);
    assert_eq!(stale.objects.get(target).zone, Zone::Battlefield);

    let (mut tampered, _, _, _, target) = make_pending(0x4d41_534b_4544_0004);
    engine::step(&mut tampered, Action::ChooseEffectBoolean(true)).unwrap();
    let choice = {
        let PendingEffectChoice::SelectTargets { legal, .. } = tampered
            .engine
            .pending_effect
            .as_mut()
            .unwrap()
            .choice
            .as_mut()
            .unwrap()
        else {
            panic!("matching graveyard selection")
        };
        legal[0]
            .expected_object
            .as_mut()
            .unwrap()
            .expected_zone_change_count += 1;
        legal[0].target
    };
    let tampered_before = tampered.clone();
    assert!(engine::step(&mut tampered, Action::ChooseEffectTarget(choice)).is_err());
    assert_eq!(tampered, tampered_before);
    assert_eq!(tampered.objects.get(target).zone, Zone::Battlefield);

    let mut empty = ready_main(0x4d41_534b_4544_0005);
    add_mana(&mut empty, ManaColor::G, 2);
    let vandal = put_object(&mut empty, PlayerId::P0, "Masked Vandal", Zone::Hand);
    put_object(&mut empty, PlayerId::P0, "Counterspell", Zone::Graveyard);
    let target = put_object(&mut empty, PlayerId::P1, "Great Furnace", Zone::Battlefield);
    engine::step(&mut empty, Action::CastSpell(vandal)).unwrap();
    pass_until(
        &mut empty,
        |_, decision| matches!(decision, Decision::ChooseTargets { spell, .. } if *spell == vandal),
    );
    engine::step(&mut empty, Action::ChooseTarget(Target::Object(target))).unwrap();
    pass_until(&mut empty, |state, _| state.stack.is_empty());
    assert_eq!(empty.objects.get(target).zone, Zone::Battlefield);
}

#[test]
fn inspector_only_targets_counters_and_gains_life_when_evidence_was_collected() {
    let mut paid = ready_main(0x494e_5350_4543_5401);
    add_mana(&mut paid, ManaColor::G, 2);
    let inspector = put_object(&mut paid, PlayerId::P0, "Vitu-Ghazi Inspector", Zone::Hand);
    let evidence = put_object(&mut paid, PlayerId::P0, "Fireblast", Zone::Graveyard);
    let target = put_object(&mut paid, PlayerId::P0, "Elvish Mystic", Zone::Battlefield);
    engine::step(&mut paid, Action::CastSpell(inspector)).unwrap();
    choose_cast_optional_cost(&mut paid, true);
    finish_collect_evidence_with(&mut paid, &[evidence]);

    let targeting = pass_until(
        &mut paid,
        |_, decision| matches!(decision, Decision::ChooseTargets { spell, .. } if *spell == inspector),
    );
    let Decision::ChooseTargets { legal_targets, .. } = targeting else {
        unreachable!()
    };
    assert!(legal_targets.contains(&Target::Object(target)));
    assert!(legal_targets.contains(&Target::Object(inspector)));
    engine::step(&mut paid, Action::ChooseTarget(Target::Object(target))).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut paid),
        Decision::CastSpellOrPass { .. }
    ));

    let bytes = serde_json::to_vec(&paid).unwrap();
    let mut restored: GameState = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(restored, paid);
    assert_eq!(
        restored.diagnostic_state_hash(),
        paid.diagnostic_state_hash()
    );
    pass_until(&mut restored, |state, _| state.stack.is_empty());
    assert_eq!(restored.objects.get(target).counters.plus1_plus1, 1);
    assert_eq!(restored.players[0].life, 22);

    let mut tampered: GameState = serde_json::from_slice(&bytes).unwrap();
    tampered
        .stack
        .last_mut()
        .unwrap()
        .v4
        .optional_additional_cost_paid = Some(OptionalAdditionalCostDef::Bargain);
    pass_until(&mut tampered, |_, decision| {
        matches!(decision, Decision::Halted { .. })
    });
    assert_eq!(tampered.objects.get(target).counters.plus1_plus1, 0);
    assert_eq!(tampered.players[0].life, 20);

    let mut declined = ready_main(0x494e_5350_4543_5402);
    add_mana(&mut declined, ManaColor::G, 2);
    let inspector = put_object(
        &mut declined,
        PlayerId::P0,
        "Vitu-Ghazi Inspector",
        Zone::Hand,
    );
    put_object(&mut declined, PlayerId::P0, "Fireblast", Zone::Graveyard);
    let target = put_object(
        &mut declined,
        PlayerId::P0,
        "Elvish Mystic",
        Zone::Battlefield,
    );
    engine::step(&mut declined, Action::CastSpell(inspector)).unwrap();
    choose_cast_optional_cost(&mut declined, false);
    pass_until(&mut declined, |state, _| state.stack.is_empty());
    assert_eq!(declined.objects.get(target).counters.plus1_plus1, 0);
    assert_eq!(declined.players[0].life, 20);
    assert!(declined.engine.pending_triggers.is_empty());
}
