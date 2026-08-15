//! Focused Tolarian Terror, graveyard reducer, and Ward {2} coverage.

use mtg_kernel::card_def::{
    card_id_by_name, preflight_fully_supported_deck, WardCostDef, CARD_DEFS,
};
use mtg_kernel::effect::EffectBooleanChoicePurpose;
use mtg_kernel::engine::{self, Action, Decision, UnsupportedMechanic};
use mtg_kernel::event::CommittedEvent;
use mtg_kernel::ids::{ObjectId, PlayerId, StackItemId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::rl::{
    legal_action_candidates_v1, observe_v2, ActionSemanticV1, BooleanChoicePurposeV4,
    PendingEffectChoiceSemanticV4,
};
use mtg_kernel::state::{Counters, GameObject, GameState, Step, Target, Zone};
use mtg_kernel::surface_v2::{HarnessSurfaceV2, SurfaceDecision};

fn card_id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name} in CARD_DEFS"))
}

fn card_name(card_def: u16) -> String {
    CARD_DEFS[card_def as usize].name.to_string()
}

fn empty_main() -> GameState {
    let mut state = GameState::new_from_libraries(&[], &[], card_name, 0x544f_4c41_5249_414e);
    state.active_player = PlayerId::P0;
    state.priority_player = PlayerId::P0;
    state.step = Step::Main1;
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
        Zone::Stack => panic!("tests use the engine for stack insertion"),
    }
    id
}

fn put_lands(state: &mut GameState, player: PlayerId, name: &str, count: usize) -> Vec<ObjectId> {
    (0..count)
        .map(|_| put_object(state, player, name, Zone::Battlefield))
        .collect()
}

fn cast_at(state: &mut GameState, spell: ObjectId, target: Target) {
    engine::step(state, Action::CastSpell(spell)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(state),
        Decision::ChooseTargets { spell: source, .. } if source == spell
    ));
    engine::step(state, Action::ChooseTarget(target)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(state),
        Decision::CastSpellOrPass { .. }
    ));
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
    panic!("stack did not reach a choice or empty state");
}

fn setup_bolt_targeting_terror(
    extra_p1_lands: usize,
) -> (GameState, ObjectId, ObjectId, Vec<ObjectId>) {
    let mut state = empty_main();
    let terror = put_object(
        &mut state,
        PlayerId::P0,
        "Tolarian Terror",
        Zone::Battlefield,
    );
    let bolt = put_object(&mut state, PlayerId::P1, "Lightning Bolt", Zone::Hand);
    let lands = put_lands(&mut state, PlayerId::P1, "Mountain", 1 + extra_p1_lands);
    state.priority_player = PlayerId::P1;
    cast_at(&mut state, bolt, Target::Object(terror));
    (state, terror, bolt, lands)
}

fn reach_ward_boolean(state: &mut GameState) -> Decision {
    let decision = pass_until_choice_or_empty(state);
    assert!(matches!(decision, Decision::ChooseEffectBoolean { .. }));
    decision
}

#[test]
fn reducer_counts_only_the_casters_instant_and_sorcery_cards() {
    let cases: &[(&[&str], usize)] = &[
        (&[], 7),
        (&["Lightning Bolt"], 6),
        (
            &[
                "Lightning Bolt",
                "Preordain",
                "Mental Note",
                "Ponder",
                "Thought Scour",
                "Brainstorm",
            ],
            1,
        ),
    ];

    for &(graveyard, islands_needed) in cases {
        let mut state = empty_main();
        let terror = put_object(&mut state, PlayerId::P0, "Tolarian Terror", Zone::Hand);
        let islands = put_lands(&mut state, PlayerId::P0, "Island", islands_needed);
        for &name in graveyard {
            put_object(&mut state, PlayerId::P0, name, Zone::Graveyard);
        }
        put_object(&mut state, PlayerId::P0, "Masked Meower", Zone::Graveyard);
        put_object(&mut state, PlayerId::P1, "Lightning Bolt", Zone::Graveyard);

        assert!(matches!(
            engine::advance_until_decision(&mut state),
            Decision::CastSpellOrPass { castable_spells, .. } if castable_spells.contains(&terror)
        ));
        engine::step(&mut state, Action::CastSpell(terror)).unwrap();
        let _ = engine::advance_until_decision(&mut state);
        assert_eq!(
            islands
                .iter()
                .filter(|&&land| state.objects.get(land).tapped)
                .count(),
            islands_needed
        );
    }
}

#[test]
fn definition_preflight_and_resolved_permanent_are_exact() {
    let terror_def = card_id("Tolarian Terror");
    preflight_fully_supported_deck(&[terror_def]).unwrap();
    let def = &CARD_DEFS[terror_def as usize];
    assert_eq!((def.power, def.toughness), (Some(5), Some(5)));
    assert_eq!(def.ward_cost, Some(WardCostDef::Generic(2)));

    let mut state = empty_main();
    let terror = put_object(&mut state, PlayerId::P0, "Tolarian Terror", Zone::Hand);
    put_object(&mut state, PlayerId::P0, "Island", Zone::Battlefield);
    for _ in 0..6 {
        put_object(&mut state, PlayerId::P0, "Preordain", Zone::Graveyard);
    }
    engine::step(&mut state, Action::CastSpell(terror)).unwrap();
    let _ = engine::advance_until_decision(&mut state);
    let _ = pass_until_choice_or_empty(&mut state);
    assert_eq!(state.objects.get(terror).zone, Zone::Battlefield);
    assert_eq!(state.objects.get(terror).v4.ward_generic, 2);
}

#[test]
fn opponent_targeting_logs_exact_event_and_unpayable_ward_counters() {
    let (mut state, terror, bolt, _) = setup_bolt_targeting_terror(0);
    let targeted = state
        .engine
        .event_history
        .iter()
        .filter_map(|event| match event {
            CommittedEvent::Targeted {
                target,
                targeting_stack_item,
                targeting_controller,
                ..
            } if *target == terror => Some((*targeting_stack_item, *targeting_controller)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(targeted.len(), 1);
    assert_ne!(targeted[0].0, StackItemId::default());
    assert_eq!(targeted[0].1, PlayerId::P1);

    let decision = pass_until_choice_or_empty(&mut state);
    assert!(matches!(decision, Decision::CastSpellOrPass { .. }));
    assert!(state.stack.is_empty());
    assert_eq!(state.objects.get(bolt).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(terror).damage, 0);
}

#[test]
fn payable_ward_projects_pay_cost_and_decline_or_payment_are_snapshot_stable() {
    let (mut state, terror, bolt, lands) = setup_bolt_targeting_terror(2);
    let decision = reach_ward_boolean(&mut state);
    assert!(matches!(
        decision,
        Decision::ChooseEffectBoolean {
            player: PlayerId::P1,
            source,
            default: Some(false),
            purpose: EffectBooleanChoicePurpose::CounterUnlessPaysGeneric {
                player: PlayerId::P1,
                generic: 2,
                ..
            },
        } if source == terror
    ));

    let candidates =
        legal_action_candidates_v1(&SurfaceDecision::Decision(decision.clone()), &state).unwrap();
    assert_eq!(candidates.len(), 2);
    assert!(matches!(
        candidates[0].record.semantic,
        ActionSemanticV1::ChooseEffectBoolean { value: false, .. }
    ));
    assert!(matches!(
        candidates[1].record.semantic,
        ActionSemanticV1::ChooseEffectBoolean { value: true, .. }
    ));
    let observation = observe_v2(&state, &HarnessSurfaceV2::new(), PlayerId::P1, 9).unwrap();
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

    let snapshot = state.snapshot();
    engine::step(&mut state, Action::ChooseEffectBoolean(false)).unwrap();
    let _ = engine::advance_until_decision(&mut state);
    assert_eq!(state.objects.get(bolt).zone, Zone::Graveyard);

    state.restore(&snapshot);
    engine::step(&mut state, Action::ChooseEffectBoolean(true)).unwrap();
    let _ = engine::advance_until_decision(&mut state);
    assert_eq!(state.objects.get(bolt).zone, Zone::Stack);
    assert_eq!(
        lands
            .iter()
            .filter(|&&land| state.objects.get(land).tapped)
            .count(),
        3,
        "one land pays Bolt and two pay Ward"
    );
}

#[test]
fn ward_payment_uses_floating_mana_before_tapping_one_remaining_land() {
    let (mut state, _, bolt, lands) = setup_bolt_targeting_terror(2);
    state.players[1].mana_pool[ManaColor::U.pool_index()] = 1;
    let _ = reach_ward_boolean(&mut state);
    engine::step(&mut state, Action::ChooseEffectBoolean(true)).unwrap();
    let _ = engine::advance_until_decision(&mut state);

    assert_eq!(state.players[1].mana_pool[ManaColor::U.pool_index()], 0);
    assert_eq!(state.objects.get(bolt).zone, Zone::Stack);
    assert_eq!(
        lands
            .iter()
            .filter(|&&land| state.objects.get(land).tapped)
            .count(),
        2
    );
}

#[test]
fn controller_targeting_its_own_terror_never_creates_ward() {
    let mut state = empty_main();
    let terror = put_object(
        &mut state,
        PlayerId::P0,
        "Tolarian Terror",
        Zone::Battlefield,
    );
    let bolt = put_object(&mut state, PlayerId::P0, "Lightning Bolt", Zone::Hand);
    put_object(&mut state, PlayerId::P0, "Mountain", Zone::Battlefield);
    cast_at(&mut state, bolt, Target::Object(terror));
    assert!(state.engine.pending_triggers.is_empty());
    assert_eq!(state.stack.len(), 1);
}

#[test]
fn retargeted_chain_lightning_copy_triggers_ward_and_only_the_copy_ceases() {
    let mut state = empty_main();
    let terror = put_object(
        &mut state,
        PlayerId::P0,
        "Tolarian Terror",
        Zone::Battlefield,
    );
    let chain = put_object(&mut state, PlayerId::P0, "Chain Lightning", Zone::Hand);
    put_object(&mut state, PlayerId::P0, "Mountain", Zone::Battlefield);
    put_lands(&mut state, PlayerId::P1, "Great Furnace", 4);

    cast_at(&mut state, chain, Target::Player(PlayerId::P1));
    assert!(matches!(
        pass_until_choice_or_empty(&mut state),
        Decision::ChooseSpellCopyPayment {
            player: PlayerId::P1,
            ..
        }
    ));
    engine::step(&mut state, Action::ChooseSpellCopyPayment(true)).unwrap();
    let copy = state
        .engine
        .pending_spell_copy
        .as_ref()
        .and_then(|pending| pending.copy_source)
        .expect("copy exists after payment");
    engine::step(&mut state, Action::ChooseSpellCopyRetarget(true)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::ChooseTargets { spell, .. } if spell == copy
    ));
    engine::step(&mut state, Action::ChooseTarget(Target::Object(terror))).unwrap();
    assert_eq!(state.objects.get(chain).zone, Zone::Graveyard);
    assert!(state
        .stack
        .iter()
        .any(|item| item.source == copy && item.is_copy));

    let _ = reach_ward_boolean(&mut state);
    engine::step(&mut state, Action::ChooseEffectBoolean(false)).unwrap();
    let _ = engine::advance_until_decision(&mut state);
    assert!(state.stack.is_empty());
    assert_eq!(state.objects.get(chain).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(copy).zone, Zone::Stack);
    assert_eq!(state.objects.get(terror).damage, 0);
}

#[test]
fn malformed_ward_stack_binding_halts_before_payment_or_counter() {
    let (mut state, terror, bolt, lands) = setup_bolt_targeting_terror(2);
    let _ = reach_ward_boolean(&mut state);
    let original = state.clone();
    let choice = state
        .engine
        .pending_effect
        .as_mut()
        .and_then(|pending| pending.choice.as_mut())
        .expect("Ward choice");
    let mtg_kernel::effect::PendingEffectChoice::ChooseBoolean { purpose, .. } = choice else {
        panic!("Ward must be a Boolean choice");
    };
    let EffectBooleanChoicePurpose::CounterUnlessPaysGeneric {
        targeting_stack_item,
        ..
    } = purpose
    else {
        panic!("Ward purpose");
    };
    *targeting_stack_item = StackItemId(999_999);

    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::Halted {
            mechanic: UnsupportedMechanic::InvalidEffectContinuation,
            source,
        } if source == terror
    ));
    assert_eq!(state.objects.get(bolt).zone, Zone::Stack);
    assert_eq!(
        lands
            .iter()
            .filter(|&&land| state.objects.get(land).tapped)
            .count(),
        1
    );
    assert_ne!(
        state, original,
        "only the deliberate malformed binding differs"
    );
}
