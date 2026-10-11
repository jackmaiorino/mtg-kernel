//! Completion regressions for MageZero Standard removal, selection, and public counters.
#![cfg(feature = "standard-magezero-fixtures")]
use mtg_kernel::card_def::{card_id_by_name, TargetSpec, CARD_DEFS};
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
fn gix_announces_each_of_six_pairs_and_keeps_choice_on_stack() {
    for selected in 0..6 {
        let mut state = ready(Step::Main1);
        let spell = put(&mut state, PlayerId::P0, "Gix's Command", Zone::Hand);
        state.players[0].mana_pool = pool(&[(ManaColor::B, 2)], 3);
        cast(&mut state, spell, &[]);
        assert!(
            matches!(next(&mut state), Decision::ChooseSpellMode { mode_count: 6, legal_modes, .. }
            if legal_modes == vec![0,1,2,3,4,5])
        );
        engine::step(&mut state, Action::ChooseSpellMode(selected)).unwrap();
        assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
        assert_eq!(state.stack.last().unwrap().mode_chosen, selected);
        assert_eq!(state.players[0].mana_pool, [0; 6]);
        let restored: GameState =
            serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
        assert_eq!(restored, state);
    }
}

#[test]
fn tear_requires_kicker_for_a_creature_and_records_paid_target_contract() {
    let mut state = ready(Step::Main1);
    let creature = put(
        &mut state,
        PlayerId::P1,
        "Quirion Beastcaller",
        Zone::Battlefield,
    );
    let spell = put(&mut state, PlayerId::P0, "Tear Asunder", Zone::Hand);
    state.players[0].mana_pool = pool(&[(ManaColor::G, 1)], 1);
    assert!(
        matches!(next(&mut state), Decision::CastSpellOrPass { castable_spells, .. }
        if !castable_spells.contains(&spell))
    );
    state.players[0].mana_pool = pool(&[(ManaColor::G, 1), (ManaColor::B, 1)], 2);
    cast(&mut state, spell, &[Target::Object(creature)]);
    assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
    let item = state.stack.last().unwrap();
    assert!(item.kicked);
    assert_eq!(item.v4.target_spec, Some(TargetSpec::NonlandPermanent));
    settled(&mut state);
    assert_eq!(state.objects.get(creature).zone, Zone::Exile);
}

#[test]
fn tear_declining_kicker_keeps_artifact_or_enchantment_target_rules() {
    let mut state = ready(Step::Main1);
    let creature = put(
        &mut state,
        PlayerId::P1,
        "Quirion Beastcaller",
        Zone::Battlefield,
    );
    let artifact = put(
        &mut state,
        PlayerId::P1,
        "Basilisk Collar",
        Zone::Battlefield,
    );
    let spell = put(&mut state, PlayerId::P0, "Tear Asunder", Zone::Hand);
    state.players[0].mana_pool = pool(&[(ManaColor::G, 1), (ManaColor::B, 1)], 2);
    cast(&mut state, spell, &[]);
    assert!(matches!(next(&mut state), Decision::ChooseKicker { .. }));
    engine::step(&mut state, Action::ChooseKicker(false)).unwrap();
    assert!(
        matches!(next(&mut state), Decision::ChooseTargets { legal_targets, .. }
        if legal_targets.contains(&Target::Object(artifact)) && !legal_targets.contains(&Target::Object(creature)))
    );
    engine::step(&mut state, Action::ChooseTarget(Target::Object(artifact))).unwrap();
    settled(&mut state);
    assert_eq!(state.objects.get(artifact).zone, Zone::Exile);
    assert_eq!(state.objects.get(creature).zone, Zone::Battlefield);
}
