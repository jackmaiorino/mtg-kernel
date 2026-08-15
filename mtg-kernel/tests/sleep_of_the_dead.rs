//! Focused Sleep of the Dead and Escape coverage.
//!
//! Rules/oracle baseline: XMage commit
//! `0723fc0c2be922af47b0ef0539f28114cc23b998`; Git blobs
//! `SleepOfTheDead.java` `0a736726d6204a91ee08793588c6b933a03f29e2`,
//! `EscapeAbility.java` `d841f1724223a80565127cc60bdf8d7cfdaea5da`,
//! `ExileFromGraveCost.java` `6565f9e164da4832ed6d09aac22876cad6a8638b`,
//! `DontUntapInControllersNextUntapStepTargetEffect.java`
//! `e0ca981deb84d077a5c5b6ac644aa287e02f7881`, and `TapTargetEffect.java`
//! `329ce20cf6377cff0dde80a0e3a82c04c6c9dde2`. Together they pin normal
//! `{U}`, the creature target/effect, and Escape `{2}{U}` plus three cards.

use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardType, CostComponent, TargetSpec, CARD_DEFS,
};
use mtg_kernel::effect::{EffectOp, ObjectRef};
use mtg_kernel::engine::{self, Action, CostKind, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::policy_surface_v5::PolicySurfaceV5;
use mtg_kernel::rl::{legal_action_candidates_v1, observe_policy_v5, observe_v2};
use mtg_kernel::state::{
    CastMethodV4, Counters, GameObject, GameState, SpellCastRouteV4, Step, Target, Zone,
};
use mtg_kernel::surface_v2::{HarnessSurfaceV2, SurfaceDecision};

fn card_id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name} in CARD_DEFS"))
}

fn card_name(card_def: u16) -> String {
    CARD_DEFS[card_def as usize].name.to_string()
}

fn put_object(state: &mut GameState, owner: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id(name);
    let object = state.objects.push(GameObject {
        card_def,
        name: name.to_string(),
        owner,
        controller: owner,
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
        Zone::Battlefield => state.players[owner.index()].battlefield.push(object),
        Zone::Library => state.players[owner.index()].library.push(object),
        Zone::Graveyard => state.players[owner.index()].graveyard.push(object),
        Zone::Exile => state.exile.push(object),
        Zone::Command => state.command.push(object),
        Zone::Stack => panic!("casts own stack insertion"),
    }
    object
}

fn ready_sleep(
    origin: Zone,
    island_count: usize,
    graveyard_count: usize,
) -> (GameState, ObjectId, ObjectId, Vec<ObjectId>, Vec<ObjectId>) {
    assert!(matches!(origin, Zone::Hand | Zone::Graveyard));
    let libraries = [card_id("Island"), card_id("Mountain")];
    let mut state =
        GameState::new_from_libraries(&libraries, &libraries, card_name, 0x534c_4545_505f_0001);
    state.step = Step::Main1;
    state.active_player = PlayerId::P0;
    state.priority_player = PlayerId::P0;
    let sleep = put_object(&mut state, PlayerId::P0, "Sleep of the Dead", origin);
    let target = put_object(
        &mut state,
        PlayerId::P1,
        "Cryptic Serpent",
        Zone::Battlefield,
    );
    let islands = (0..island_count)
        .map(|_| put_object(&mut state, PlayerId::P0, "Island", Zone::Battlefield))
        .collect();
    let names = [
        "Lightning Bolt",
        "Fireblast",
        "Counterspell",
        "Mountain",
        "Deep Analysis",
        "Lava Dart",
    ];
    let graveyard = (0..graveyard_count)
        .map(|index| {
            put_object(
                &mut state,
                PlayerId::P0,
                names[index % names.len()],
                Zone::Graveyard,
            )
        })
        .collect();
    (state, sleep, target, islands, graveyard)
}

fn cast_decision(state: &mut GameState) -> Decision {
    engine::advance_until_decision(state)
}

fn is_offered(state: &GameState, spell: ObjectId) -> bool {
    matches!(
        cast_decision(&mut state.clone()),
        Decision::CastSpellOrPass { castable_spells, .. }
            if castable_spells.contains(&spell)
    )
}

fn cast_and_target(state: &mut GameState, sleep: ObjectId, target: ObjectId) -> Decision {
    engine::step(state, Action::CastSpell(sleep)).unwrap();
    let decision = engine::advance_until_decision(state);
    assert!(matches!(
        decision,
        Decision::ChooseTargets {
            spell,
            remaining: 1,
            ref legal_targets,
            ..
        } if spell == sleep && legal_targets.contains(&Target::Object(target))
    ));
    engine::step(state, Action::ChooseTarget(Target::Object(target))).unwrap();
    engine::advance_until_decision(state)
}

fn pass_until_spell_finishes(state: &mut GameState, spell: ObjectId) {
    for _ in 0..16 {
        if state.objects.get(spell).zone != Zone::Stack {
            return;
        }
        let decision = engine::advance_until_decision(state);
        if state.objects.get(spell).zone != Zone::Stack {
            return;
        }
        match decision {
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => panic!("unexpected decision while resolving Sleep: {other:?}"),
        }
    }
    panic!("Sleep did not leave the stack");
}

fn stable_ids(state: &GameState, decision: &Decision) -> Vec<String> {
    legal_action_candidates_v1(&SurfaceDecision::Decision(decision.clone()), state)
        .expect("schema-v4 action projection")
        .into_iter()
        .map(|candidate| candidate.record.stable_id)
        .collect()
}

#[test]
fn definition_and_normal_effect_are_exact() {
    let def = &CARD_DEFS[card_id("Sleep of the Dead") as usize];
    assert_eq!(def.capability, CardCapability::Full);
    assert_eq!(def.types, &[CardType::Sorcery]);
    assert_eq!(def.cost.generic, 0);
    assert_eq!(def.cost.pips, &[Pip::Colored(ManaColor::U)]);
    assert_eq!(def.target_spec, TargetSpec::Creature);
    assert!(matches!(
        (def.spell_effect)(),
        Some(EffectOp::Sequence(ops))
            if matches!(
                ops.as_slice(),
                [
                    EffectOp::TapObject { object: ObjectRef::Target(0) },
                    EffectOp::SkipNextUntap { object: ObjectRef::Target(0) },
                ]
            )
    ));
    let escape = def.escape.as_ref().expect("Sleep owns Escape");
    assert!(matches!(
        escape.cost,
        [
            CostComponent::Mana(cost),
            CostComponent::ExileOtherCardsFromOwnGraveyard(3),
        ] if cost.generic == 2 && cost.pips == [Pip::Colored(ManaColor::U)]
    ));

    let (mut state, sleep, target, islands, _) = ready_sleep(Zone::Hand, 1, 0);
    state.objects.get_mut(target).tapped = true;
    assert!(is_offered(&state, sleep));
    assert!(matches!(
        cast_and_target(&mut state, sleep, target),
        Decision::CastSpellOrPass { .. }
    ));
    assert!(islands
        .iter()
        .all(|island| state.objects.get(*island).tapped));
    pass_until_spell_finishes(&mut state, sleep);
    assert!(state.objects.get(target).tapped);
    assert!(state.objects.get(target).v4.skip_next_untap);
    assert_eq!(state.objects.get(sleep).zone, Zone::Graveyard);
}

#[test]
fn escape_offer_and_candidate_contract_are_exact() {
    let (short_graveyard, sleep, _, _, _) = ready_sleep(Zone::Graveyard, 3, 2);
    assert!(!is_offered(&short_graveyard, sleep));

    let (short_mana, sleep, _, _, _) = ready_sleep(Zone::Graveyard, 2, 3);
    assert!(!is_offered(&short_mana, sleep));

    let (mut state, sleep, target, _, graveyard) = ready_sleep(Zone::Graveyard, 3, 3);
    let opponent_card = put_object(&mut state, PlayerId::P1, "Lightning Bolt", Zone::Graveyard);
    let ceased_token = put_object(&mut state, PlayerId::P0, "Blood Token", Zone::Graveyard);
    assert!(event::cease_to_exist(&mut state, ceased_token));
    assert_eq!(state.objects.get(ceased_token).zone, Zone::Graveyard);
    assert!(is_offered(&state, sleep));

    let decision = cast_and_target(&mut state, sleep, target);
    assert!(matches!(
        decision,
        Decision::ChooseCostTargets {
            player: PlayerId::P0,
            source,
            cost_kind: CostKind::ExileFromGraveyard,
            remaining: 3,
            ref candidates,
        } if source == sleep
            && candidates == &graveyard
            && !candidates.contains(&opponent_card)
            && !candidates.contains(&ceased_token)
            && !candidates.contains(&sleep)
    ));
}

#[test]
fn escape_choices_round_trip_and_commit_exact_provenance() {
    let (mut state, sleep, target, islands, graveyard) = ready_sleep(Zone::Graveyard, 3, 5);
    let mut canonical_order_state = state.clone();
    let first_decision = cast_and_target(&mut state, sleep, target);
    assert!(matches!(first_decision, Decision::ChooseCostTargets { .. }));

    let before_invalid = state.clone();
    assert!(engine::step(&mut state, Action::ChooseCostTarget(sleep)).is_err());
    assert_eq!(state, before_invalid);

    let chosen = [graveyard[4], graveyard[1], graveyard[2]];
    let canonical = [graveyard[1], graveyard[2], graveyard[4]];
    engine::step(&mut state, Action::ChooseCostTarget(chosen[0])).unwrap();
    let second_decision = engine::advance_until_decision(&mut state);
    assert!(matches!(
        second_decision,
        Decision::ChooseCostTargets {
            remaining: 2,
            ref candidates,
            ..
        } if !candidates.contains(&chosen[0])
    ));

    let observation = observe_v2(&state, &HarnessSurfaceV2::new(), PlayerId::P0, 7).unwrap();
    let visible_sacrifices = &observation
        .projection
        .engine_context
        .pending_cast
        .as_ref()
        .expect("Escape remains a pending cast")
        .sacrifice_chosen;
    assert!(
        visible_sacrifices.is_empty(),
        "Escape must not reinterpret the frozen sacrifice-only feature"
    );
    assert_eq!(
        state
            .engine
            .pending_cast
            .as_ref()
            .expect("Escape remains pending internally")
            .sacrifice_chosen,
        vec![chosen[0]],
        "privileged serialized state retains the exact staged prefix"
    );
    let policy_error = observe_policy_v5(&state, &PolicySurfaceV5::new(), PlayerId::P0, 7, 1, 0, 1)
        .expect_err("schema-v5 policy state must fail closed on a hidden Escape prefix");
    assert!(policy_error
        .0
        .contains("versioned object-cost successor is required"));

    let snapshot = state.snapshot();
    let state_hash = state.state_hash();
    let diagnostic_hash = state.diagnostic_state_hash();
    let action_ids = stable_ids(&state, &second_decision);
    let json = serde_json::to_string(&state).expect("serialize a mid-Escape choice");
    assert!(json.contains("\"GraveyardEscape\""));
    let mut restored: GameState =
        serde_json::from_str(&json).expect("deserialize a mid-Escape choice");
    assert_eq!(restored, state);
    restored.restore(&snapshot);
    assert_eq!(restored.state_hash(), state_hash);
    assert_eq!(restored.diagnostic_state_hash(), diagnostic_hash);
    let restored_decision = engine::advance_until_decision(&mut restored);
    assert_eq!(restored_decision, second_decision);
    assert_eq!(stable_ids(&restored, &restored_decision), action_ids);

    let before_duplicate = state.clone();
    assert!(engine::step(&mut state, Action::ChooseCostTarget(chosen[0])).is_err());
    assert_eq!(state, before_duplicate);

    engine::step(&mut state, Action::ChooseCostTarget(chosen[1])).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::ChooseCostTargets { remaining: 1, .. }
    ));
    engine::step(&mut state, Action::ChooseCostTarget(chosen[2])).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));

    let item = state.stack.last().expect("escaped Sleep on stack");
    assert!(!item.is_flashback);
    assert_eq!(item.v4.cast_method, Some(CastMethodV4::Escape));
    let source_contract = item.v4.source_contract.expect("physical source contract");
    assert_eq!(source_contract.cast_method, CastMethodV4::Escape);
    assert_eq!(
        source_contract.spell_cast_origin.expect("cast route").route,
        SpellCastRouteV4::GraveyardEscape
    );
    assert_eq!(
        item.v4
            .paid_cost_refs
            .iter()
            .map(|paid| paid.object)
            .collect::<Vec<_>>(),
        canonical,
        "Escape provenance is canonical rather than dependent on hidden pick order"
    );
    assert!(item
        .v4
        .paid_cost_refs
        .iter()
        .all(|paid| paid.zone == Zone::Exile));
    assert!(chosen.iter().all(|card| state.exile.contains(card)));
    assert!(graveyard
        .iter()
        .filter(|card| !chosen.contains(card))
        .all(|card| state.players[0].graveyard.contains(card)));
    assert!(islands
        .iter()
        .all(|island| state.objects.get(*island).tapped));

    assert!(matches!(
        cast_and_target(&mut canonical_order_state, sleep, target),
        Decision::ChooseCostTargets { .. }
    ));
    for pick in canonical {
        engine::step(&mut canonical_order_state, Action::ChooseCostTarget(pick)).unwrap();
        let _ = engine::advance_until_decision(&mut canonical_order_state);
    }
    assert_eq!(
        canonical_order_state, state,
        "two pick orders for one Escape set must have identical future state"
    );

    pass_until_spell_finishes(&mut state, sleep);
    assert_eq!(state.objects.get(sleep).zone, Zone::Graveyard);
    assert!(state.objects.get(target).v4.skip_next_untap);
    for _ in 0..3 {
        put_object(&mut state, PlayerId::P0, "Island", Zone::Battlefield);
    }
    put_object(&mut state, PlayerId::P0, "Lightning Bolt", Zone::Graveyard);
    let recast_decision = cast_decision(&mut state.clone());
    assert!(
        matches!(
            recast_decision,
            Decision::CastSpellOrPass {
                ref castable_spells,
                ..
            } if castable_spells.contains(&sleep)
        ),
        "Escape returns Sleep to the graveyard and can be offered again; got {recast_decision:?}",
    );
}

#[test]
fn escape_continuation_loss_aborts_without_partial_payment() {
    let (mut state, sleep, target, islands, graveyard) = ready_sleep(Zone::Graveyard, 3, 4);
    assert!(matches!(
        cast_and_target(&mut state, sleep, target),
        Decision::ChooseCostTargets { .. }
    ));
    engine::step(&mut state, Action::ChooseCostTarget(graveyard[0])).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::ChooseCostTargets { remaining: 2, .. }
    ));
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(graveyard[1], Zone::Exile),
    );
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(graveyard[2], Zone::Exile),
    );

    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    assert!(state.engine.pending_cast.is_none());
    assert_eq!(state.objects.get(sleep).zone, Zone::Graveyard);
    assert!(state.players[0].graveyard.contains(&graveyard[0]));
    assert!(!state.exile.contains(&graveyard[0]));
    assert!(islands
        .iter()
        .all(|island| !state.objects.get(*island).tapped));
}

#[test]
fn stale_target_and_countered_escape_return_sleep_to_graveyard() {
    let (mut stale, sleep, target, _, graveyard) = ready_sleep(Zone::Graveyard, 3, 3);
    let first = cast_and_target(&mut stale, sleep, target);
    let first_pick = match first {
        Decision::ChooseCostTargets { candidates, .. } => candidates[0],
        other => panic!("expected Escape cost choice, got {other:?}"),
    };
    engine::step(&mut stale, Action::ChooseCostTarget(first_pick)).unwrap();
    let second = engine::advance_until_decision(&mut stale);
    let second_pick = match second {
        Decision::ChooseCostTargets { candidates, .. } => candidates[0],
        other => panic!("expected second Escape cost choice, got {other:?}"),
    };
    engine::step(&mut stale, Action::ChooseCostTarget(second_pick)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut stale),
        Decision::CastSpellOrPass { .. }
    ));
    event::propose_and_commit(
        &mut stale,
        ProposedEvent::zone_change(target, Zone::Graveyard),
    );
    event::propose_and_commit(
        &mut stale,
        ProposedEvent::zone_change(target, Zone::Battlefield),
    );
    pass_until_spell_finishes(&mut stale, sleep);
    assert_eq!(stale.objects.get(sleep).zone, Zone::Graveyard);
    assert!(!stale.objects.get(target).v4.skip_next_untap);
    assert!(graveyard
        .iter()
        .all(|card| { stale.exile.contains(card) || stale.players[0].graveyard.contains(card) }));

    let (mut countered, sleep, target, _, paid_exiles) = ready_sleep(Zone::Graveyard, 3, 3);
    put_object(&mut countered, PlayerId::P1, "Island", Zone::Battlefield);
    put_object(&mut countered, PlayerId::P1, "Island", Zone::Battlefield);
    let counterspell = put_object(&mut countered, PlayerId::P1, "Counterspell", Zone::Hand);
    let first = cast_and_target(&mut countered, sleep, target);
    let first_pick = match first {
        Decision::ChooseCostTargets { candidates, .. } => candidates[0],
        other => panic!("expected Escape cost choice, got {other:?}"),
    };
    engine::step(&mut countered, Action::ChooseCostTarget(first_pick)).unwrap();
    let second = engine::advance_until_decision(&mut countered);
    let second_pick = match second {
        Decision::ChooseCostTargets { candidates, .. } => candidates[0],
        other => panic!("expected second Escape cost choice, got {other:?}"),
    };
    engine::step(&mut countered, Action::ChooseCostTarget(second_pick)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut countered),
        Decision::CastSpellOrPass {
            player: PlayerId::P0,
            ..
        }
    ));
    engine::step(&mut countered, Action::Pass).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut countered),
        Decision::CastSpellOrPass {
            player: PlayerId::P1,
            ..
        }
    ));
    engine::step(&mut countered, Action::CastSpell(counterspell)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut countered),
        Decision::ChooseTargets { spell, .. } if spell == counterspell
    ));
    engine::step(&mut countered, Action::ChooseTarget(Target::Object(sleep))).unwrap();
    pass_until_spell_finishes(&mut countered, sleep);
    assert_eq!(countered.objects.get(sleep).zone, Zone::Graveyard);
    assert!(paid_exiles
        .iter()
        .all(|card| countered.exile.contains(card)));
    assert!(!countered.objects.get(target).v4.skip_next_untap);
}
