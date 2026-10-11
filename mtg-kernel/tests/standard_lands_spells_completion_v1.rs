//! Completion regressions for MageZero Standard removal, selection, and public counters.
#![cfg(feature = "standard-magezero-fixtures")]
use mtg_kernel::card_def::{card_id_by_name, CardCapability, Keywords, TargetSpec, CARD_DEFS};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};
use mtg_kernel::trigger;
fn ready_with_library(step: Step, library: &[&str]) -> GameState {
    let ids: Vec<u16> = library
        .iter()
        .map(|name| card_id_by_name(name).unwrap())
        .collect();
    let mut state = GameState::new_from_libraries(
        &ids,
        &ids,
        |id| CARD_DEFS[id as usize].object_name.into(),
        0x4741_4d45,
    );
    state.step = step;
    state
}

fn ready(step: Step) -> GameState {
    ready_with_library(step, &["Forest"; 40])
}

fn put(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id_by_name(name).unwrap();
    let id = state.objects.push(GameObject {
        card_def,
        name: CARD_DEFS[card_def as usize].object_name.into(),
        owner: player,
        controller: player,
        zone,
        tapped: false,
        summoning_sick: zone == Zone::Hand,
        damage: 0,
        counters: Default::default(),
        attachments: Vec::new(),
        v4: ObjectStateV4::from_card_def(card_def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    let seat = &mut state.players[player.index()];
    match zone {
        Zone::Hand => seat.hand.push(id),
        Zone::Battlefield => seat.battlefield.push(id),
        Zone::Graveyard => seat.graveyard.push(id),
        _ => panic!("helper zone"),
    }
    id
}

#[allow(dead_code)]
fn move_to(state: &mut GameState, id: ObjectId, zone: Zone) {
    event::propose_and_commit(state, ProposedEvent::zone_change(id, zone));
    let triggers = trigger::collect_and_process(state);
    state.engine.pending_triggers.extend(triggers);
}

fn pool(cost: &[(ManaColor, u8)], generic: u8) -> [u8; 6] {
    let mut pool = [0; 6];
    for &(color, count) in cost {
        pool[color.pool_index()] += count;
    }
    pool[5] += generic;
    pool
}

fn next(state: &mut GameState) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
    decision
}

/// Passes priority and orders triggers until the stack and pending triggers
/// are empty, returning the first other decision if one interrupts.
fn settle(state: &mut GameState) -> Option<Decision> {
    for _ in 0..200 {
        match next(state) {
            Decision::OrderTriggers { pending, .. } => {
                engine::step(state, Action::OrderTriggers((0..pending.len()).collect())).unwrap()
            }
            Decision::CastSpellOrPass { .. }
                if state.stack.is_empty() && state.engine.pending_triggers.is_empty() =>
            {
                return None
            }
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => return Some(other),
        }
    }
    panic!("did not settle");
}

fn settled(state: &mut GameState) {
    if let Some(other) = settle(state) {
        panic!("unexpected choice: {other:?}");
    }
}

/// Casts a spell from hand with mana already in the pool, answering the
/// target prompts in order.
fn cast(state: &mut GameState, spell: ObjectId, targets: &[Target]) {
    assert!(
        matches!(next(state), Decision::CastSpellOrPass { castable_spells, .. } if castable_spells.contains(&spell)),
        "{} not castable",
        state.objects.get(spell).name
    );
    engine::step(state, Action::CastSpell(spell)).unwrap();
    for &target in targets {
        match next(state) {
            Decision::ChooseTargets { legal_targets, .. } => {
                assert!(legal_targets.contains(&target), "{target:?} not legal");
                engine::step(state, Action::ChooseTarget(target)).unwrap();
            }
            other => panic!("expected a target choice, got {other:?}"),
        }
    }
}

#[test]
fn completion_spells_are_executable_full_definitions() {
    for name in [
        "Abrade",
        "Boltwave",
        "Essence Scatter",
        "Snakeskin Veil",
        "Sleight of Hand",
        "Stock Up",
        "Shore Up",
        "Big Score",
        "Cut Down",
        "Go for the Throat",
    ] {
        let def = &CARD_DEFS[card_id_by_name(name).unwrap() as usize];
        assert_eq!(def.capability, CardCapability::Full, "{name}");
        assert!((def.spell_effect)().is_some(), "{name}");
    }
}

#[test]
fn removal_targets_use_effective_power_and_toughness_and_artifact_type() {
    let mut state = ready(Step::Main1);
    let small = put(
        &mut state,
        PlayerId::P1,
        "Quirion Beastcaller",
        Zone::Battlefield,
    );
    let artifact = put(&mut state, PlayerId::P1, "Blood Token", Zone::Battlefield);
    let large = put(
        &mut state,
        PlayerId::P1,
        "Bloodletter of Aclazotz",
        Zone::Battlefield,
    );
    let targets = engine::legal_targets_for(
        TargetSpec::CreaturePowerPlusToughnessAtMostFive,
        &[],
        &state,
    );
    assert!(targets.contains(&Target::Object(small)));
    assert!(!targets.contains(&Target::Object(large)));
    assert!(!targets.contains(&Target::Object(artifact)));
    state.objects.get_mut(small).counters.plus1_plus1 = 1;
    assert!(!engine::legal_targets_for(
        TargetSpec::CreaturePowerPlusToughnessAtMostFive,
        &[],
        &state
    )
    .contains(&Target::Object(small)));
    state.objects.get_mut(small).counters.plus1_plus1 = 0;
    let spell = put(&mut state, PlayerId::P0, "Cut Down", Zone::Hand);
    state.players[0].mana_pool = pool(&[(ManaColor::B, 1)], 0);
    cast(&mut state, spell, &[Target::Object(small)]);
    state.objects.get_mut(small).counters.plus1_plus1 = 1;
    settled(&mut state);
    assert_eq!(
        state.objects.get(small).zone,
        Zone::Battlefield,
        "target is rechecked after a pump"
    );
}

#[test]
fn shore_up_untaps_and_grants_hexproof_until_cleanup() {
    let mut state = ready(Step::Main1);
    let creature = put(
        &mut state,
        PlayerId::P0,
        "Quirion Beastcaller",
        Zone::Battlefield,
    );
    state.objects.get_mut(creature).tapped = true;
    let spell = put(&mut state, PlayerId::P0, "Shore Up", Zone::Hand);
    state.players[0].mana_pool = pool(&[(ManaColor::U, 1)], 0);
    cast(&mut state, spell, &[Target::Object(creature)]);
    settled(&mut state);
    assert!(!state.objects.get(creature).tapped);
    assert_eq!(engine::effective_power(&state, creature), 3);
    assert!(engine::has_effective_keyword(
        &state,
        creature,
        Keywords::HEXPROOF
    ));
}

#[test]
fn boltwave_damages_opponent_and_snakeskin_adds_a_counter() {
    let mut state = ready(Step::Main1);
    let creature = put(
        &mut state,
        PlayerId::P0,
        "Quirion Beastcaller",
        Zone::Battlefield,
    );
    let spell = put(&mut state, PlayerId::P0, "Snakeskin Veil", Zone::Hand);
    state.players[0].mana_pool = pool(&[(ManaColor::G, 1)], 0);
    cast(&mut state, spell, &[Target::Object(creature)]);
    settled(&mut state);
    assert_eq!(state.objects.get(creature).counters.plus1_plus1, 1);
    assert!(engine::has_effective_keyword(
        &state,
        creature,
        Keywords::HEXPROOF
    ));
    let spell = put(&mut state, PlayerId::P0, "Boltwave", Zone::Hand);
    state.players[0].mana_pool = pool(&[(ManaColor::R, 1)], 0);
    cast(&mut state, spell, &[]);
    settled(&mut state);
    assert_eq!(state.players[1].life, 17);
    assert_eq!(state.players[0].life, 20);
}

#[test]
fn poison_is_public_to_both_players_and_absent_when_zero() {
    use mtg_kernel::policy_surface_v5::PolicySurfaceV5;
    use mtg_kernel::rl::observe_policy_v6;
    let mut state = ready(Step::Main1);
    let observe = |state: &GameState, actor| {
        observe_policy_v6(state, &PolicySurfaceV5::new(), actor, 0, 0, 0, 1).unwrap()
    };
    let before = observe(&state, PlayerId::P0);
    assert!(before.projection.poison_counters.is_none());
    assert!(serde_json::to_value(before).unwrap()["projection"]
        .get("poison_counters")
        .is_none());
    state.players[1].poison_counters.0 = 3;
    for actor in [PlayerId::P0, PlayerId::P1] {
        let observed = observe(&state, actor);
        assert_eq!(observed.projection.poison_counters, Some([0, 3]));
    }
}

#[test]
fn soulstone_animation_survives_cleanup_and_has_all_creature_types() {
    use mtg_kernel::card_def::{CardType, Subtype};
    let mut state = ready(Step::Main1);
    let land = put(
        &mut state,
        PlayerId::P0,
        "Soulstone Sanctuary",
        Zone::Battlefield,
    );
    state.players[0].mana_pool = pool(&[], 4);
    next(&mut state);
    engine::step(&mut state, Action::ActivateAbility(land, 0)).unwrap();
    settled(&mut state);
    assert_eq!(engine::effective_power(&state, land), 3);
    assert!(engine::object_has_type(&state, land, CardType::Land));
    assert!(engine::object_has_type(&state, land, CardType::Creature));
    assert!(engine::has_effective_keyword(
        &state,
        land,
        Keywords::VIGILANCE
    ));
    for &subtype in Subtype::CREATURE_TYPES {
        assert!(engine::has_effective_subtype(&state, land, subtype));
    }
    state.step = Step::Cleanup;
    next(&mut state);
    assert!(engine::object_has_type(&state, land, CardType::Creature));
    assert_eq!(engine::effective_power(&state, land), 3);
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(land, Zone::Hand));
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(land, Zone::Battlefield),
    );
    assert!(!engine::object_has_type(&state, land, CardType::Creature));
}

#[test]
fn otawara_channels_with_legendary_reduction_and_returns_artifacts() {
    let mut state = ready(Step::Main1);
    put(
        &mut state,
        PlayerId::P0,
        "Adeline, Resplendent Cathar",
        Zone::Battlefield,
    );
    let target = put(
        &mut state,
        PlayerId::P1,
        "Experimental Synthesizer",
        Zone::Battlefield,
    );
    let land = put(
        &mut state,
        PlayerId::P0,
        "Otawara, Soaring City",
        Zone::Hand,
    );
    state.players[0].mana_pool = pool(&[(ManaColor::U, 1)], 2);
    next(&mut state);
    engine::step(&mut state, Action::ActivateAbility(land, 0)).unwrap();
    assert!(matches!(next(&mut state), Decision::ChooseTargets { .. }));
    engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
    settled(&mut state);
    assert_eq!(state.objects.get(land).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(target).zone, Zone::Hand);
    assert_eq!(state.players[0].mana_pool, [0; 6]);
}

#[test]
fn anoint_checks_corruption_at_resolution_and_gleeful_rewards_own_artifact() {
    let mut state = ready(Step::Main1);
    let target = put(
        &mut state,
        PlayerId::P1,
        "Bloodletter of Aclazotz",
        Zone::Battlefield,
    );
    let spell = put(
        &mut state,
        PlayerId::P0,
        "Anoint with Affliction",
        Zone::Hand,
    );
    state.players[0].mana_pool = pool(&[(ManaColor::B, 1)], 1);
    cast(&mut state, spell, &[Target::Object(target)]);
    state.players[1].poison_counters.0 = 3;
    settled(&mut state);
    assert_eq!(state.objects.get(target).zone, Zone::Exile);
    let artifact = put(&mut state, PlayerId::P0, "Blood Token", Zone::Battlefield);
    let spell = put(&mut state, PlayerId::P0, "Gleeful Demolition", Zone::Hand);
    state.players[0].mana_pool = pool(&[(ManaColor::R, 1)], 0);
    cast(&mut state, spell, &[Target::Object(artifact)]);
    settled(&mut state);
    assert_eq!(state.players[0].battlefield.len(), 3);
    assert!(state.players[0]
        .battlefield
        .iter()
        .all(|&id| state.objects.get(id).card_def
            == card_id_by_name("Phyrexian Goblin Token").unwrap()));
}
