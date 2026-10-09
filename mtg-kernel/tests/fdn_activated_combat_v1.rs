//! Printed combat activations, with exact source/target incarnations.
#![cfg(feature = "limited-fdn-fixtures")]

use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardType, Keywords, Subtype, TargetSpec, CARD_DEFS,
    KERNEL_CARDDB_HASH,
};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

fn ready() -> GameState {
    let mountain = card_id_by_name("Mountain").unwrap();
    let mut state =
        GameState::new_from_libraries(&[mountain; 40], &[mountain; 40], |_| "Mountain".into(), 610);
    state.step = Step::Main1;
    state
}

fn put(state: &mut GameState, player: PlayerId, name: &str) -> ObjectId {
    let card_def = card_id_by_name(name).unwrap();
    let id = state.objects.push(GameObject {
        card_def,
        name: name.into(),
        owner: player,
        controller: player,
        zone: Zone::Battlefield,
        tapped: false,
        summoning_sick: false,
        damage: 0,
        counters: Default::default(),
        attachments: Vec::new(),
        v4: ObjectStateV4::from_card_def(card_def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    state.players[player.index()].battlefield.push(id);
    id
}

fn next(state: &mut GameState) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
    decision
}

fn offered(state: &mut GameState, source: ObjectId) -> bool {
    matches!(next(state), Decision::CastSpellOrPass { activatable_abilities, .. } if activatable_abilities.contains(&(source, 0)))
}

fn activate(state: &mut GameState, source: ObjectId) {
    assert!(offered(state, source));
    engine::step(state, Action::ActivateAbility(source, 0)).unwrap();
}

fn settle(state: &mut GameState) {
    for _ in 0..60 {
        match next(state) {
            Decision::CastSpellOrPass { .. } if state.stack.is_empty() => return,
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => panic!("unexpected resolution choice {other:?}"),
        }
    }
    panic!("resolution did not settle");
}

fn choose(state: &mut GameState, target: ObjectId) {
    assert!(matches!(next(state), Decision::ChooseTargets { .. }));
    engine::step(state, Action::ChooseTarget(Target::Object(target))).unwrap();
    next(state); // finish staging the activation on the stack
}

fn cleanup(state: &mut GameState) {
    state.step = Step::Cleanup;
    state.players[0].hand.clear();
    state.players[1].hand.clear();
    next(state);
    assert!(state.engine.until_end_of_turn.is_empty());
}

#[test]
fn registrations_append_and_preserve_printed_characteristics() {
    println!("FDN activated combat catalog hash: {KERNEL_CARDDB_HASH:016x}");
    for (offset, name) in ["Shivan Dragon", "Axgard Cavalry", "Rogue's Passage"]
        .into_iter()
        .enumerate()
    {
        let id = card_id_by_name(name).unwrap();
        assert_eq!(id as usize, 327 + offset);
        assert_eq!(CARD_DEFS[id as usize].capability, CardCapability::Full);
        assert_eq!(CARD_DEFS[id as usize].activated_abilities.len(), 1);
        assert!(!CARD_DEFS[id as usize].activated_abilities[0].sorcery_speed_only);
        assert_eq!(
            CARD_DEFS[id as usize].activated_abilities[0].max_activations_per_turn,
            None
        );
    }
    let dragon = &CARD_DEFS[327];
    assert_eq!(
        (dragon.power, dragon.toughness, dragon.mana_value),
        (Some(5), Some(5), 6)
    );
    assert_eq!(dragon.subtypes, &[Subtype::Dragon]);
    assert!(dragon.keywords.has(Keywords::FLYING));
    assert_eq!(dragon.activated_abilities[0].target_spec, TargetSpec::None);
    let cavalry = &CARD_DEFS[328];
    assert_eq!(
        (cavalry.power, cavalry.toughness, cavalry.mana_value),
        (Some(2), Some(2), 2)
    );
    assert_eq!(cavalry.subtypes, &[Subtype::Dwarf, Subtype::Berserker]);
    assert_eq!(
        cavalry.activated_abilities[0].target_spec,
        TargetSpec::Creature
    );
    let passage = &CARD_DEFS[329];
    assert!(passage.is_land && passage.has_type(CardType::Land));
    assert_eq!(passage.produces_mana, &[ManaColor::C]);
    assert!(passage.colors.is_empty());
    assert_eq!(
        passage.activated_abilities[0].target_spec,
        TargetSpec::Creature
    );
}

#[test]
fn shivan_pays_red_each_time_pumps_only_power_and_expires() {
    let mut state = ready();
    let dragon = put(&mut state, PlayerId::P0, "Shivan Dragon");
    state.objects.get_mut(dragon).summoning_sick = true;
    state.players[0].mana_pool[5] = 3;
    assert!(
        !offered(&mut state, dragon),
        "colorless cannot pay the red pip"
    );
    state.players[0].mana_pool = [0; 6];
    state.players[0].mana_pool[ManaColor::R.pool_index()] = 2;
    for expected in [6, 7] {
        activate(&mut state, dragon);
        settle(&mut state);
        assert_eq!(engine::effective_power(&state, dragon), expected);
        assert_eq!(engine::effective_toughness(&state, dragon), 5);
        assert!(!state.objects.get(dragon).tapped);
    }
    assert_eq!(state.players[0].mana_pool, [0; 6]);
    assert!(!offered(&mut state, dragon));
    cleanup(&mut state);
    assert_eq!(engine::effective_power(&state, dragon), 5);
}

#[test]
fn shivan_old_activation_never_pumps_a_returned_source() {
    for return_source in [false, true] {
        let mut state = ready();
        let dragon = put(&mut state, PlayerId::P0, "Shivan Dragon");
        state.players[0].mana_pool[ManaColor::R.pool_index()] = 1;
        activate(&mut state, dragon);
        next(&mut state);
        assert_eq!(state.stack.len(), 1);
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(dragon, Zone::Hand));
        if return_source {
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(dragon, Zone::Battlefield),
            );
        }
        let saved = state.snapshot();
        let mut restored = ready();
        restored.restore(&saved);
        settle(&mut state);
        settle(&mut restored);
        assert_eq!(state.state_hash(), restored.state_hash());
        assert!(state.engine.until_end_of_turn.is_empty());
        if return_source {
            assert_eq!(engine::effective_power(&state, dragon), 5);
        }
    }
}

#[test]
fn cavalry_requires_an_untapped_nonsick_source_and_grants_real_haste() {
    let mut state = ready();
    let cavalry = put(&mut state, PlayerId::P0, "Axgard Cavalry");
    let dragon = put(&mut state, PlayerId::P0, "Shivan Dragon");
    state.objects.get_mut(cavalry).summoning_sick = true;
    state.objects.get_mut(dragon).summoning_sick = true;
    assert!(!offered(&mut state, cavalry));
    state.objects.get_mut(cavalry).summoning_sick = false;
    activate(&mut state, cavalry);
    choose(&mut state, dragon);
    settle(&mut state);
    assert!(state.objects.get(cavalry).tapped);
    assert!(!offered(&mut state, cavalry));
    assert!(engine::has_effective_keyword(
        &state,
        dragon,
        Keywords::HASTE
    ));
    state.step = Step::DeclareAttackers;
    assert!(
        matches!(next(&mut state), Decision::DeclareAttackers { eligible, .. } if eligible.contains(&dragon))
    );
    cleanup(&mut state);
    assert!(!engine::has_effective_keyword(
        &state,
        dragon,
        Keywords::HASTE
    ));
}

#[test]
fn cavalry_can_target_opposing_creatures_and_restore_target_selection() {
    let mut state = ready();
    let cavalry = put(&mut state, PlayerId::P0, "Axgard Cavalry");
    let dragon = put(&mut state, PlayerId::P1, "Shivan Dragon");
    let land = put(&mut state, PlayerId::P0, "Mountain");
    activate(&mut state, cavalry);
    let pending = next(&mut state);
    assert!(matches!(pending, Decision::ChooseTargets { .. }));
    let before = state.state_hash();
    assert!(engine::step(&mut state, Action::ChooseTarget(Target::Object(land))).is_err());
    assert_eq!(state.state_hash(), before);
    let saved = state.snapshot();
    let mut restored = ready();
    restored.restore(&saved);
    for current in [&mut state, &mut restored] {
        engine::step(current, Action::ChooseTarget(Target::Object(dragon))).unwrap();
        settle(current);
        assert!(engine::has_effective_keyword(
            current,
            dragon,
            Keywords::HASTE
        ));
    }
    assert_eq!(state.state_hash(), restored.state_hash());
}

#[test]
fn keyword_activations_do_not_follow_targets_across_zone_changes() {
    for (name, keyword) in [
        ("Axgard Cavalry", Keywords::HASTE),
        ("Rogue's Passage", Keywords::CANT_BE_BLOCKED),
    ] {
        let mut state = ready();
        let source = put(&mut state, PlayerId::P0, name);
        let dragon = put(&mut state, PlayerId::P1, "Shivan Dragon");
        state.players[0].mana_pool[5] = 4;
        activate(&mut state, source);
        choose(&mut state, dragon);
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(dragon, Zone::Hand));
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(dragon, Zone::Battlefield),
        );
        settle(&mut state);
        assert!(!engine::has_effective_keyword(&state, dragon, keyword));
        assert!(state.engine.until_end_of_turn.is_empty());
    }
}

#[test]
fn passage_mana_ability_is_separate_and_does_not_pay_its_own_tap_cost() {
    let mut state = ready();
    let passage = put(&mut state, PlayerId::P0, "Rogue's Passage");
    put(&mut state, PlayerId::P0, "Shivan Dragon");
    state.players[0].mana_pool[5] = 3;
    assert!(
        !offered(&mut state, passage),
        "the fourth mana cannot consume the required source tap"
    );
    engine::step(&mut state, Action::ActivateManaAbility(passage)).unwrap();
    assert!(state.objects.get(passage).tapped);
    assert_eq!(state.players[0].mana_pool[5], 4);
    assert!(state.stack.is_empty());
    assert!(!offered(&mut state, passage));
}

#[test]
fn passage_cost_target_restore_expiry_and_repeat_after_untap() {
    let mut state = ready();
    let passage = put(&mut state, PlayerId::P0, "Rogue's Passage");
    let dragon = put(&mut state, PlayerId::P1, "Shivan Dragon");
    state.players[0].mana_pool[5] = 4;
    activate(&mut state, passage);
    assert!(matches!(next(&mut state), Decision::ChooseTargets { .. }));
    let saved = state.snapshot();
    let mut restored = ready();
    restored.restore(&saved);
    for current in [&mut state, &mut restored] {
        engine::step(current, Action::ChooseTarget(Target::Object(dragon))).unwrap();
        settle(current);
        assert!(engine::has_effective_keyword(
            current,
            dragon,
            Keywords::CANT_BE_BLOCKED
        ));
        assert_eq!(current.players[0].mana_pool, [0; 6]);
        assert!(current.objects.get(passage).tapped);
        assert!(!offered(current, passage));
    }
    assert_eq!(state.state_hash(), restored.state_hash());
    cleanup(&mut state);
    assert!(!engine::has_effective_keyword(
        &state,
        dragon,
        Keywords::CANT_BE_BLOCKED
    ));
    state.active_player = PlayerId::P0;
    state.priority_player = PlayerId::P0;
    state.step = Step::Main1;
    state.objects.get_mut(passage).tapped = false;
    state.players[0].mana_pool[5] = 4;
    activate(&mut state, passage);
    choose(&mut state, dragon);
    settle(&mut state);
    assert!(engine::has_effective_keyword(
        &state,
        dragon,
        Keywords::CANT_BE_BLOCKED
    ));
}

#[test]
fn passage_unblockability_removes_real_blockers() {
    let mut state = ready();
    let passage = put(&mut state, PlayerId::P0, "Rogue's Passage");
    let attacker = put(&mut state, PlayerId::P0, "Shivan Dragon");
    let blocker = put(&mut state, PlayerId::P1, "Shivan Dragon");
    state.players[0].mana_pool[5] = 4;
    activate(&mut state, passage);
    choose(&mut state, attacker);
    settle(&mut state);
    state.step = Step::DeclareBlockers;
    state.engine.combat.attackers = vec![attacker];
    state.engine.combat.attackers_declared = true;
    assert!(
        matches!(next(&mut state), Decision::DeclareBlockers { legal_blockers, .. }
        if legal_blockers.iter().all(|(_, blockers)| !blockers.contains(&blocker)))
    );
    let before = state.state_hash();
    assert!(engine::step(
        &mut state,
        Action::DeclareBlockers(vec![(blocker, attacker)])
    )
    .is_err());
    assert_eq!(state.state_hash(), before);
}

#[test]
fn targeted_fixed_pump_keeps_its_existing_target_contract() {
    let mut state = ready();
    let dragon = put(&mut state, PlayerId::P0, "Shivan Dragon");
    let growth = put(&mut state, PlayerId::P0, "Giant Growth");
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(growth, Zone::Hand));
    state.players[0].mana_pool[ManaColor::G.pool_index()] = 1;
    next(&mut state);
    engine::step(&mut state, Action::CastSpell(growth)).unwrap();
    choose(&mut state, dragon);
    settle(&mut state);
    assert_eq!(engine::effective_power(&state, dragon), 8);
    assert_eq!(engine::effective_toughness(&state, dragon), 8);
}
