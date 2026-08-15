//! Focused coverage for Annul, Envelop, Force Spike, Spell Pierce, and
//! Steel Sabotage. Card text is bounded to the checked-in Mage Java sources
//! named by `data/cards_v1.json`.

use mtg_kernel::card_def::{card_id_by_name, TargetSpec, CARD_DEFS};
use mtg_kernel::effect::{EffectBooleanChoicePurpose, EffectOp, ObjectRef, TargetRef};
use mtg_kernel::engine::{self, Action, Decision, UnsupportedMechanic};
use mtg_kernel::ids::{ObjectId, PlayerId, StackItemId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::rl::{
    legal_action_candidates_v1, observe_v2, ActionSemanticV1, BooleanChoicePurposeV4,
    PendingEffectChoiceSemanticV4,
};
use mtg_kernel::state::{
    CastMethodV4, Counters, GameObject, GameState, SpellCastOriginV4, SpellCastRouteV4, StackItem,
    StackItemKind, StackSourceContractV4, StackStateV4, Step, Target, Zone,
};
use mtg_kernel::surface_v2::{HarnessSurfaceV2, SurfaceDecision};

fn card_id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name} in CARD_DEFS"))
}

fn card_name(card_def: u16) -> String {
    CARD_DEFS[card_def as usize].name.to_string()
}

fn ready_game() -> GameState {
    let mut state = GameState::new_from_libraries(&[], &[], card_name, 0x434f_554e_5445_5253);
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
        name: name.to_string(),
        owner,
        controller,
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
        Zone::Hand => state.players[owner.index()].hand.push(object),
        Zone::Battlefield => state.players[controller.index()].battlefield.push(object),
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
    let mut object_v4 = mtg_kernel::state::ObjectStateV4::from_card_def(card_def);
    object_v4.spell_cast_origin = Some(SpellCastOriginV4 {
        origin_zone: Zone::Hand,
        origin_zone_change_count: 0,
        route: SpellCastRouteV4::Hand,
        finalized_method: Some(CastMethodV4::Normal),
    });
    let object = state.objects.push(GameObject {
        card_def,
        name: name.to_string(),
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

fn put_islands(state: &mut GameState, player: PlayerId, count: usize) -> Vec<ObjectId> {
    (0..count)
        .map(|_| put_object(state, player, player, "Island", Zone::Battlefield))
        .collect()
}

fn cast_counter_at(
    state: &mut GameState,
    card_name: &str,
    mode: Option<u8>,
    target: ObjectId,
) -> ObjectId {
    state.players[PlayerId::P0.index()].mana_pool[ManaColor::U.pool_index()] += 1;
    let spell = put_object(state, PlayerId::P0, PlayerId::P0, card_name, Zone::Hand);
    engine::step(state, Action::CastSpell(spell)).unwrap();
    let mut decision = engine::advance_until_decision(state);
    if let Some(mode) = mode {
        if matches!(
            decision,
            Decision::ChooseSpellMode { spell: source, mode_count: 2, .. } if source == spell
        ) {
            engine::step(state, Action::ChooseSpellMode(mode)).unwrap();
            decision = engine::advance_until_decision(state);
        } else {
            assert_eq!(
                state
                    .engine
                    .pending_cast
                    .as_ref()
                    .and_then(|pending| pending.mode_chosen),
                Some(mode),
                "the sole viable Steel Sabotage mode should be selected silently"
            );
        }
    }
    assert!(matches!(
        decision,
        Decision::ChooseTargets { spell: source, .. } if source == spell
    ));
    engine::step(state, Action::ChooseTarget(Target::Object(target))).unwrap();
    assert!(matches!(
        engine::advance_until_decision(state),
        Decision::CastSpellOrPass { .. }
    ));
    spell
}

fn pass_until_choice_or_empty(state: &mut GameState) -> Decision {
    for _ in 0..32 {
        let decision = engine::advance_until_decision(state);
        match decision {
            Decision::CastSpellOrPass { .. } if !state.stack.is_empty() => {
                engine::step(state, Action::Pass).unwrap();
            }
            other => return other,
        }
    }
    panic!("stack did not reach a choice or become empty")
}

#[test]
fn definitions_bind_exact_filters_programs_and_modal_bounce() {
    let expected = [
        ("Annul", TargetSpec::ArtifactOrEnchantmentSpellOnStack),
        ("Envelop", TargetSpec::SorcerySpellOnStack),
        ("Force Spike", TargetSpec::AnySpellOnStack),
        ("Spell Pierce", TargetSpec::NoncreatureSpellOnStack),
        ("Steel Sabotage", TargetSpec::ArtifactSpellOnStack),
    ];
    for (name, target_spec) in expected {
        let def = &CARD_DEFS[card_id(name) as usize];
        assert!(def.has_full_support(), "{name}");
        assert_eq!(def.target_spec, target_spec, "{name}");
    }

    assert!(matches!(
        (CARD_DEFS[card_id("Force Spike") as usize].spell_effect)(),
        Some(EffectOp::CounterTargetUnlessPaysGeneric {
            target: TargetRef::Target(0),
            generic: 1,
        })
    ));
    assert!(matches!(
        (CARD_DEFS[card_id("Spell Pierce") as usize].spell_effect)(),
        Some(EffectOp::CounterTargetUnlessPaysGeneric {
            target: TargetRef::Target(0),
            generic: 2,
        })
    ));
    let steel_mode = CARD_DEFS[card_id("Steel Sabotage") as usize]
        .mode2
        .as_ref()
        .expect("Steel Sabotage has two modes");
    assert_eq!(steel_mode.target_spec, TargetSpec::ArtifactPermanent);
    assert!(matches!(
        (steel_mode.effect)(),
        EffectOp::Conditional {
            then,
            ..
        } if matches!(
            *then,
            EffectOp::MoveObject {
                object: ObjectRef::Target(0),
                to_zone: Zone::Hand,
            }
        )
    ));
}

#[test]
fn target_filters_accept_only_the_printed_spell_and_permanent_types() {
    let mut state = ready_game();
    let artifact = put_spell_on_stack(&mut state, PlayerId::P1, "Ichor Wellspring");
    let enchantment = put_spell_on_stack(&mut state, PlayerId::P1, "Bind the Monster");
    let sorcery = put_spell_on_stack(&mut state, PlayerId::P1, "Preordain");
    let creature = put_spell_on_stack(&mut state, PlayerId::P1, "Tolarian Terror");
    let instant = put_spell_on_stack(&mut state, PlayerId::P1, "Brainstorm");
    let artifact_permanent = put_object(
        &mut state,
        PlayerId::P1,
        PlayerId::P1,
        "Ichor Wellspring",
        Zone::Battlefield,
    );
    let creature_permanent = put_object(
        &mut state,
        PlayerId::P1,
        PlayerId::P1,
        "Faerie Seer",
        Zone::Battlefield,
    );

    assert_eq!(
        engine::legal_targets_for(TargetSpec::ArtifactOrEnchantmentSpellOnStack, &[], &state),
        vec![Target::Object(artifact), Target::Object(enchantment)]
    );
    assert_eq!(
        engine::legal_targets_for(TargetSpec::SorcerySpellOnStack, &[], &state),
        vec![Target::Object(sorcery)]
    );
    assert_eq!(
        engine::legal_targets_for(TargetSpec::NoncreatureSpellOnStack, &[], &state),
        vec![
            Target::Object(artifact),
            Target::Object(enchantment),
            Target::Object(sorcery),
            Target::Object(instant),
        ]
    );
    assert_eq!(
        engine::legal_targets_for(TargetSpec::ArtifactSpellOnStack, &[], &state),
        vec![Target::Object(artifact)]
    );
    assert_eq!(
        engine::legal_targets_for(TargetSpec::ArtifactPermanent, &[], &state),
        vec![Target::Object(artifact_permanent)]
    );
    assert_ne!(artifact_permanent, creature_permanent);
    assert!(
        !engine::legal_targets_for(TargetSpec::NoncreatureSpellOnStack, &[], &state)
            .contains(&Target::Object(creature))
    );
}

#[test]
fn annul_envelop_and_steel_counter_their_exact_spell_targets() {
    for (counter, target, mode) in [
        ("Annul", "Ichor Wellspring", None),
        ("Envelop", "Preordain", None),
        ("Steel Sabotage", "Ichor Wellspring", Some(0)),
    ] {
        let mut state = ready_game();
        let target = put_spell_on_stack(&mut state, PlayerId::P1, target);
        let counter = cast_counter_at(&mut state, counter, mode, target);
        let decision = pass_until_choice_or_empty(&mut state);
        assert!(matches!(decision, Decision::CastSpellOrPass { .. }));
        assert!(state.stack.is_empty());
        assert_eq!(state.objects.get(target).zone, Zone::Graveyard);
        assert_eq!(state.objects.get(counter).zone, Zone::Graveyard);
    }
}

#[test]
fn force_spike_auto_counters_when_one_generic_is_unpayable() {
    let mut state = ready_game();
    let target = put_spell_on_stack(&mut state, PlayerId::P1, "Brainstorm");
    let force = cast_counter_at(&mut state, "Force Spike", None, target);
    let decision = pass_until_choice_or_empty(&mut state);
    assert!(matches!(decision, Decision::CastSpellOrPass { .. }));
    assert!(state.stack.is_empty());
    assert_eq!(state.objects.get(target).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(force).zone, Zone::Graveyard);
}

#[test]
fn force_spike_payment_choice_is_public_deterministic_and_snapshot_stable() {
    let mut state = ready_game();
    let target = put_spell_on_stack(&mut state, PlayerId::P1, "Brainstorm");
    let payer_lands = put_islands(&mut state, PlayerId::P1, 1);
    let force = cast_counter_at(&mut state, "Force Spike", None, target);
    let decision = pass_until_choice_or_empty(&mut state);
    assert!(matches!(
        decision,
        Decision::ChooseEffectBoolean {
            player: PlayerId::P1,
            source,
            default: Some(false),
            purpose: EffectBooleanChoicePurpose::CounterTargetUnlessPaysGeneric {
                player: PlayerId::P1,
                generic: 1,
                ..
            },
        } if source == force
    ));

    let actions =
        legal_action_candidates_v1(&SurfaceDecision::Decision(decision.clone()), &state).unwrap();
    assert_eq!(actions.len(), 2);
    assert!(matches!(
        actions[0].record.semantic,
        ActionSemanticV1::ChooseEffectBoolean { value: false, .. }
    ));
    assert!(matches!(
        actions[1].record.semantic,
        ActionSemanticV1::ChooseEffectBoolean { value: true, .. }
    ));
    assert_eq!(
        actions
            .iter()
            .map(|candidate| candidate.record.stable_id.clone())
            .collect::<Vec<_>>(),
        legal_action_candidates_v1(&SurfaceDecision::Decision(decision), &state)
            .unwrap()
            .into_iter()
            .map(|candidate| candidate.record.stable_id)
            .collect::<Vec<_>>()
    );
    let observation = observe_v2(&state, &HarnessSurfaceV2::new(), PlayerId::P1, 7).unwrap();
    assert!(matches!(
        observation
            .projection
            .engine_context
            .pending_effect
            .and_then(|pending| pending.choice),
        Some(PendingEffectChoiceSemanticV4::Boolean {
            purpose: BooleanChoicePurposeV4::PayCost,
            ..
        })
    ));
    let encoded = serde_json::to_vec(state.engine.pending_effect.as_ref().unwrap()).unwrap();
    let decoded: mtg_kernel::effect::EffectContinuation = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(decoded, *state.engine.pending_effect.as_ref().unwrap());

    let snapshot = state.snapshot();
    engine::step(&mut state, Action::ChooseEffectBoolean(false)).unwrap();
    let _ = engine::advance_until_decision(&mut state);
    assert_eq!(state.objects.get(target).zone, Zone::Graveyard);

    state.restore(&snapshot);
    engine::step(&mut state, Action::ChooseEffectBoolean(true)).unwrap();
    let _ = engine::advance_until_decision(&mut state);
    assert_eq!(state.objects.get(target).zone, Zone::Stack);
    assert!(state.objects.get(payer_lands[0]).tapped);
}

#[test]
fn spell_pierce_requires_two_generic_and_never_targets_creatures() {
    let mut state = ready_game();
    let creature = put_spell_on_stack(&mut state, PlayerId::P1, "Tolarian Terror");
    let noncreature = put_spell_on_stack(&mut state, PlayerId::P1, "Brainstorm");
    put_islands(&mut state, PlayerId::P1, 1);
    state.players[PlayerId::P0.index()].mana_pool[ManaColor::U.pool_index()] = 1;
    let pierce = put_object(
        &mut state,
        PlayerId::P0,
        PlayerId::P0,
        "Spell Pierce",
        Zone::Hand,
    );
    engine::step(&mut state, Action::CastSpell(pierce)).unwrap();
    let decision = engine::advance_until_decision(&mut state);
    let Decision::ChooseTargets { legal_targets, .. } = decision else {
        panic!("Spell Pierce should choose a target")
    };
    assert_eq!(legal_targets, vec![Target::Object(noncreature)]);
    assert!(!legal_targets.contains(&Target::Object(creature)));
    engine::step(
        &mut state,
        Action::ChooseTarget(Target::Object(noncreature)),
    )
    .unwrap();
    let _ = engine::advance_until_decision(&mut state);
    let decision = pass_until_choice_or_empty(&mut state);
    assert!(!matches!(decision, Decision::ChooseEffectBoolean { .. }));
    assert_eq!(state.objects.get(noncreature).zone, Zone::Graveyard);
}

#[test]
fn steel_sabotage_returns_an_artifact_to_its_owner_not_controller() {
    let mut state = ready_game();
    let artifact = put_object(
        &mut state,
        PlayerId::P1,
        PlayerId::P0,
        "Ichor Wellspring",
        Zone::Battlefield,
    );
    let steel = cast_counter_at(&mut state, "Steel Sabotage", Some(1), artifact);
    let decision = pass_until_choice_or_empty(&mut state);
    assert!(matches!(decision, Decision::CastSpellOrPass { .. }));
    assert_eq!(state.objects.get(artifact).zone, Zone::Hand);
    assert!(state.players[PlayerId::P1.index()].hand.contains(&artifact));
    assert!(!state.players[PlayerId::P0.index()].hand.contains(&artifact));
    assert_eq!(state.objects.get(steel).zone, Zone::Graveyard);
}

#[test]
fn malformed_bound_stack_id_halts_before_payment_or_counter() {
    let mut state = ready_game();
    let target = put_spell_on_stack(&mut state, PlayerId::P1, "Brainstorm");
    let payer_lands = put_islands(&mut state, PlayerId::P1, 1);
    let force = cast_counter_at(&mut state, "Force Spike", None, target);
    let _ = pass_until_choice_or_empty(&mut state);
    let choice = state
        .engine
        .pending_effect
        .as_mut()
        .and_then(|pending| pending.choice.as_mut())
        .expect("Force Spike payment choice");
    let mtg_kernel::effect::PendingEffectChoice::ChooseBoolean { purpose, .. } = choice else {
        panic!("Force Spike should yield a Boolean choice")
    };
    let EffectBooleanChoicePurpose::CounterTargetUnlessPaysGeneric {
        target_stack_item, ..
    } = purpose
    else {
        panic!("Force Spike should bind its targeted spell")
    };
    *target_stack_item = StackItemId(999_999);

    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::Halted {
            mechanic: UnsupportedMechanic::InvalidEffectContinuation,
            source,
        } if source == force
    ));
    assert_eq!(state.objects.get(target).zone, Zone::Stack);
    assert!(!state.objects.get(payer_lands[0]).tapped);
}
