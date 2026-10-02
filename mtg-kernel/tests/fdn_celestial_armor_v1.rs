//! Equipment entry protection, flash, equip timing and exact incarnations.
#![cfg(feature = "limited-fdn-fixtures")]

use mtg_kernel::card_def::{
    card_id_by_name, preflight_fully_supported_deck, CardType, Keywords, Subtype, TargetSpec,
    CARD_DEFS, KERNEL_CARDDB_HASH,
};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};
use mtg_kernel::trigger;

fn ready() -> GameState {
    let island = card_id_by_name("Island").unwrap();
    let mut state =
        GameState::new_from_libraries(&[island; 40], &[island; 40], |_| "Island".into(), 123);
    state.step = Step::Main1;
    state
}

fn put(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id_by_name(name).unwrap();
    let object = state.objects.push(GameObject {
        card_def,
        name: CARD_DEFS[card_def as usize].object_name.into(),
        owner: player,
        controller: player,
        zone,
        tapped: false,
        summoning_sick: false,
        damage: 0,
        counters: Default::default(),
        attachments: vec![],
        v4: ObjectStateV4::from_card_def(card_def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    match zone {
        Zone::Battlefield => state.players[player.index()].battlefield.push(object),
        Zone::Hand => state.players[player.index()].hand.push(object),
        _ => panic!("helper zone"),
    }
    object
}

fn mana(state: &mut GameState, white: u8, generic: u8) {
    state.players[0].mana_pool[ManaColor::W.pool_index()] = white;
    state.players[0].mana_pool[ManaColor::C.pool_index()] = generic;
}

fn next(state: &mut GameState) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
    decision
}

fn finish(state: &mut GameState) {
    for _ in 0..60 {
        let decision = next(state);
        if matches!(decision, Decision::CastSpellOrPass { .. }) && state.stack.is_empty() {
            assert!(state.engine.pending_triggers.is_empty());
            return;
        }
        match decision {
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => panic!("unexpected resolution decision: {other:?}"),
        }
    }
    panic!("resolution did not finish");
}

fn entry_targets(state: &mut GameState) -> Decision {
    for _ in 0..30 {
        let decision = next(state);
        match decision {
            Decision::ChooseTargets { .. } => return decision,
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => panic!("unexpected entry decision: {other:?}"),
        }
    }
    panic!("no entry target decision");
}

fn announce(state: &mut GameState) -> ObjectId {
    let armor = put(state, PlayerId::P0, "Celestial Armor", Zone::Hand);
    mana(state, 1, 2);
    next(state);
    engine::step(state, Action::CastSpell(armor)).unwrap();
    armor
}

fn attach(state: &mut GameState, target: ObjectId) -> ObjectId {
    let armor = announce(state);
    entry_targets(state);
    engine::step(state, Action::ChooseTarget(Target::Object(target))).unwrap();
    finish(state);
    armor
}

fn equip(state: &mut GameState, armor: ObjectId, target: ObjectId) {
    mana(state, 1, 3);
    next(state);
    engine::step(state, Action::ActivateAbility(armor, 0)).unwrap();
    assert!(matches!(next(state), Decision::ChooseTargets { .. }));
    engine::step(state, Action::ChooseTarget(Target::Object(target))).unwrap();
}

fn protection(state: &GameState, target: ObjectId, expected: bool) {
    for keyword in [Keywords::HEXPROOF, Keywords::INDESTRUCTIBLE] {
        assert_eq!(
            engine::has_effective_keyword(state, target, keyword),
            expected
        );
    }
}

fn restored(state: &GameState) -> GameState {
    serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
}

#[test]
fn printed_artifact_flash_equipment_and_admission_are_exact() {
    assert_eq!(card_id_by_name("Celestial Armor"), Some(204));
    let card = &CARD_DEFS[204];
    assert_eq!(card.mana_value, 3);
    assert_eq!(card.cost.generic, 2);
    assert_eq!(card.cost.pips, &[Pip::Colored(ManaColor::W)]);
    assert_eq!(card.types, &[CardType::Artifact]);
    assert_eq!(card.colors, &[ManaColor::W]);
    assert_eq!(card.subtypes, &[Subtype::Equipment]);
    assert_eq!(card.keywords, Keywords::FLASH);
    assert_eq!(card.target_spec, TargetSpec::None);
    assert_eq!(card.activated_abilities.len(), 1);
    assert!(card.activated_abilities[0].sorcery_speed_only);
    assert_eq!(
        card.activated_abilities[0].target_spec,
        TargetSpec::ControlledCreature
    );
    preflight_fully_supported_deck(&[204]).unwrap();
    println!("FDN v48 hash: {KERNEL_CARDDB_HASH:016x}");
}

#[test]
fn cast_requires_three_mana_including_white() {
    for (white, generic, legal) in [(0, 3, false), (1, 1, false), (1, 2, true)] {
        let mut state = ready();
        let armor = put(&mut state, PlayerId::P0, "Celestial Armor", Zone::Hand);
        mana(&mut state, white, generic);
        let decision = next(&mut state);
        assert!(
            matches!(decision, Decision::CastSpellOrPass { castable_spells, .. }
            if castable_spells.contains(&armor) == legal)
        );
        let hash = state.state_hash();
        if legal {
            engine::step(&mut state, Action::CastSpell(armor)).unwrap();
            finish(&mut state);
            assert_eq!(state.objects.get(armor).zone, Zone::Battlefield);
            assert_eq!(state.players[0].mana_pool, [0; 6]);
        } else {
            assert!(engine::step(&mut state, Action::CastSpell(armor)).is_err());
            assert_eq!(state.state_hash(), hash);
        }
    }
}

#[test]
fn flash_allows_cast_during_opponents_turn() {
    let mut state = ready();
    state.active_player = PlayerId::P1;
    let target = put(&mut state, PlayerId::P0, "Elvish Mystic", Zone::Battlefield);
    let armor = attach(&mut state, target);
    assert_eq!(
        state.objects.get(armor).v4.attached_to.unwrap().object,
        target
    );
    assert_eq!(engine::effective_power(&state, target), 3);
    assert!(engine::has_effective_keyword(
        &state,
        target,
        Keywords::FLYING
    ));
    protection(&state, target, true);
}

#[test]
fn entry_targets_only_controlled_creatures_and_rejects_invalid_answers() {
    let mut state = ready();
    let own = put(&mut state, PlayerId::P0, "Elvish Mystic", Zone::Battlefield);
    let enemy = put(&mut state, PlayerId::P1, "Elvish Mystic", Zone::Battlefield);
    let land = put(&mut state, PlayerId::P0, "Forest", Zone::Battlefield);
    let armor = announce(&mut state);
    assert!(
        matches!(entry_targets(&mut state), Decision::ChooseTargets { legal_targets, remaining: 1, .. }
        if legal_targets == vec![Target::Object(own)])
    );
    for target in [enemy, land, armor] {
        let hash = state.state_hash();
        assert!(engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).is_err());
        assert_eq!(state.state_hash(), hash);
    }
    engine::step(&mut state, Action::ChooseTarget(Target::Object(own))).unwrap();
    finish(&mut state);
    state.validate_attachment_relations().unwrap();
    assert_eq!(state.objects.get(own).attachments, vec![armor]);
    assert_eq!(engine::effective_power(&state, own), 3);
    assert_eq!(engine::effective_toughness(&state, own), 1);
    assert!(engine::has_effective_keyword(&state, own, Keywords::FLYING));
    protection(&state, own, true);
    protection(&state, enemy, false);
}

#[test]
fn empty_battlefield_armor_resolves_without_attachment() {
    let mut state = ready();
    let armor = announce(&mut state);
    finish(&mut state);
    assert_eq!(state.objects.get(armor).zone, Zone::Battlefield);
    assert!(state.objects.get(armor).v4.attached_to.is_none());
    assert!(state.engine.until_end_of_turn.is_empty());
}

#[test]
fn source_removed_before_entry_resolution_still_grants_protection() {
    let mut state = ready();
    let target = put(&mut state, PlayerId::P0, "Elvish Mystic", Zone::Battlefield);
    let armor = announce(&mut state);
    entry_targets(&mut state);
    engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
    next(&mut state);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(armor, Zone::Graveyard),
    );
    finish(&mut state);
    protection(&state, target, true);
    assert_eq!(engine::effective_power(&state, target), 1);
    assert!(!engine::has_effective_keyword(
        &state,
        target,
        Keywords::FLYING
    ));
    assert!(state.objects.get(target).attachments.is_empty());
}

#[test]
fn lost_or_returned_entry_target_gets_neither_attachment_nor_protection() {
    for returns in [false, true] {
        let mut state = ready();
        let target = put(&mut state, PlayerId::P0, "Elvish Mystic", Zone::Battlefield);
        let armor = announce(&mut state);
        entry_targets(&mut state);
        engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
        next(&mut state);
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(target, Zone::Hand));
        if returns {
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(target, Zone::Battlefield),
            );
        }
        finish(&mut state);
        assert!(state.objects.get(armor).v4.attached_to.is_none());
        protection(&state, target, false);
    }
}

#[test]
fn reequipping_moves_static_bonus_but_leaves_entry_protection_on_original_target() {
    let mut state = ready();
    let first = put(&mut state, PlayerId::P0, "Elvish Mystic", Zone::Battlefield);
    let second = put(
        &mut state,
        PlayerId::P0,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    let armor = attach(&mut state, first);
    equip(&mut state, armor, second);
    finish(&mut state);
    assert_eq!(state.players[0].mana_pool, [0; 6]);
    assert_eq!(
        state.objects.get(armor).v4.attached_to.unwrap().object,
        second
    );
    assert_eq!(engine::effective_power(&state, first), 1);
    assert_eq!(engine::effective_power(&state, second), 3);
    assert!(!engine::has_effective_keyword(
        &state,
        first,
        Keywords::FLYING
    ));
    assert!(engine::has_effective_keyword(
        &state,
        second,
        Keywords::FLYING
    ));
    protection(&state, first, true);
    protection(&state, second, false);
    state.validate_attachment_relations().unwrap();
}

#[test]
fn removing_attached_armor_preserves_temporary_protection() {
    let mut state = ready();
    let target = put(&mut state, PlayerId::P0, "Elvish Mystic", Zone::Battlefield);
    let armor = attach(&mut state, target);
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(armor, Zone::Hand));
    next(&mut state);
    protection(&state, target, true);
    assert_eq!(engine::effective_power(&state, target), 1);
    assert!(!engine::has_effective_keyword(
        &state,
        target,
        Keywords::FLYING
    ));
    state.validate_attachment_relations().unwrap();
}

#[test]
fn equip_requires_four_mana_including_white_and_sorcery_timing() {
    for (white, generic, step, active, legal) in [
        (0, 4, Step::Main1, PlayerId::P0, false),
        (1, 2, Step::Main1, PlayerId::P0, false),
        (1, 3, Step::BeginCombat, PlayerId::P0, false),
        (1, 3, Step::Main1, PlayerId::P1, false),
        (1, 3, Step::Main1, PlayerId::P0, true),
    ] {
        let mut state = ready();
        state.step = step;
        state.active_player = active;
        let armor = put(
            &mut state,
            PlayerId::P0,
            "Celestial Armor",
            Zone::Battlefield,
        );
        let target = put(&mut state, PlayerId::P0, "Elvish Mystic", Zone::Battlefield);
        mana(&mut state, white, generic);
        assert!(
            matches!(next(&mut state), Decision::CastSpellOrPass { activatable_abilities, .. }
            if activatable_abilities.contains(&(armor, 0)) == legal)
        );
        let hash = state.state_hash();
        if legal {
            engine::step(&mut state, Action::ActivateAbility(armor, 0)).unwrap();
            next(&mut state);
            engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
            finish(&mut state);
            protection(&state, target, false);
            assert_eq!(state.players[0].mana_pool, [0; 6]);
        } else {
            assert!(engine::step(&mut state, Action::ActivateAbility(armor, 0)).is_err());
            assert_eq!(state.state_hash(), hash);
        }
    }
}

#[test]
fn equip_target_loss_preserves_previous_attachment() {
    let mut state = ready();
    let first = put(&mut state, PlayerId::P0, "Elvish Mystic", Zone::Battlefield);
    let second = put(
        &mut state,
        PlayerId::P0,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    let armor = attach(&mut state, first);
    equip(&mut state, armor, second);
    next(&mut state);
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(second, Zone::Hand));
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(second, Zone::Battlefield),
    );
    finish(&mut state);
    assert_eq!(
        state.objects.get(armor).v4.attached_to.unwrap().object,
        first
    );
    protection(&state, second, false);
}

#[test]
fn protection_prevents_lethal_damage_but_not_zero_toughness_or_bounce() {
    let mut state = ready();
    let target = put(&mut state, PlayerId::P0, "Elvish Mystic", Zone::Battlefield);
    attach(&mut state, target);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::damage(target, Target::Object(target), 20),
    );
    trigger::sba_fixed_point(&mut state);
    assert_eq!(state.objects.get(target).zone, Zone::Battlefield);
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(target, Zone::Hand));
    assert_eq!(state.objects.get(target).zone, Zone::Hand);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(target, Zone::Battlefield),
    );
    protection(&state, target, false);
}

#[test]
fn cleanup_removes_protection_and_leaves_equipped_flying_bonus() {
    let mut state = ready();
    let target = put(&mut state, PlayerId::P0, "Elvish Mystic", Zone::Battlefield);
    let armor = attach(&mut state, target);
    state.step = Step::End;
    for _ in 0..30 {
        let decision = next(&mut state);
        if state.active_player == PlayerId::P1 {
            protection(&state, target, false);
            assert_eq!(
                state.objects.get(armor).v4.attached_to.unwrap().object,
                target
            );
            assert_eq!(engine::effective_power(&state, target), 3);
            assert!(engine::has_effective_keyword(
                &state,
                target,
                Keywords::FLYING
            ));
            return;
        }
        match decision {
            Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
            other => panic!("unexpected cleanup decision: {other:?}"),
        }
    }
    panic!("cleanup did not complete");
}

#[test]
fn restore_preserves_entry_targets_equipping_and_resolved_protection() {
    let mut state = ready();
    let first = put(&mut state, PlayerId::P0, "Elvish Mystic", Zone::Battlefield);
    let second = put(
        &mut state,
        PlayerId::P0,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    let armor = announce(&mut state);
    entry_targets(&mut state);
    let mut copy = restored(&state);
    assert_eq!(next(&mut state), next(&mut copy));
    for current in [&mut state, &mut copy] {
        engine::step(current, Action::ChooseTarget(Target::Object(first))).unwrap();
        finish(current);
    }
    assert_eq!(state.state_hash(), copy.state_hash());
    protection(&copy, first, true);
    mana(&mut state, 1, 3);
    next(&mut state);
    engine::step(&mut state, Action::ActivateAbility(armor, 0)).unwrap();
    next(&mut state);
    let mut copy = restored(&state);
    assert_eq!(next(&mut state), next(&mut copy));
    for current in [&mut state, &mut copy] {
        engine::step(current, Action::ChooseTarget(Target::Object(second))).unwrap();
        finish(current);
    }
    assert_eq!(state.state_hash(), copy.state_hash());
    assert_eq!(restored(&state).state_hash(), state.state_hash());
}
