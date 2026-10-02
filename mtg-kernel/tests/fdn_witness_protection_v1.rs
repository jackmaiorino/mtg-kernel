//! Witness Protection: characteristics, ability loss and effect ordering.
#![cfg(feature = "limited-fdn-fixtures")]

use mtg_kernel::card_def::{
    card_id_by_name, preflight_fully_supported_deck, CardType, Keywords, Subtype, Supertype,
    TargetSpec, CARD_DEFS, KERNEL_CARDDB_HASH,
};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{self, ManaColor};
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

fn announce(state: &mut GameState, name: &str, target: ObjectId) -> ObjectId {
    let spell = put(state, PlayerId::P0, name, Zone::Hand);
    state.players[0].mana_pool = [5; 6];
    next(state);
    engine::step(state, Action::CastSpell(spell)).unwrap();
    assert!(matches!(next(state), Decision::ChooseTargets { .. }));
    engine::step(state, Action::ChooseTarget(Target::Object(target))).unwrap();
    spell
}

fn witness(state: &mut GameState, target: ObjectId) -> ObjectId {
    let aura = announce(state, "Witness Protection", target);
    finish(state);
    aura
}

fn armor(state: &mut GameState, target: ObjectId) -> ObjectId {
    let equipment = put(state, PlayerId::P0, "Celestial Armor", Zone::Hand);
    state.players[0].mana_pool = [5; 6];
    next(state);
    engine::step(state, Action::CastSpell(equipment)).unwrap();
    for _ in 0..30 {
        match next(state) {
            Decision::ChooseTargets { .. } => {
                engine::step(state, Action::ChooseTarget(Target::Object(target))).unwrap();
                finish(state);
                return equipment;
            }
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => panic!("unexpected Armor decision: {other:?}"),
        }
    }
    panic!("Armor entry target missing");
}

fn checkpoint(state: &mut GameState) {
    let pending = trigger::collect_and_process(state);
    state.engine.pending_triggers.extend(pending);
}

fn restored(state: &GameState) -> GameState {
    serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
}

#[test]
fn definition_cost_and_creature_aura_admission() {
    let id = card_id_by_name("Witness Protection").unwrap();
    assert_eq!(id, 205);
    let definition = &CARD_DEFS[id as usize];
    assert_eq!(definition.mana_value, 1);
    assert_eq!(definition.colors, &[ManaColor::U]);
    assert_eq!(definition.types, &[CardType::Enchantment]);
    assert_eq!(definition.subtypes, &[Subtype::Aura]);
    assert_eq!(definition.target_spec, TargetSpec::Creature);
    assert!(definition.attachment.unwrap().is_creature_aura());
    preflight_fully_supported_deck(&[id]).unwrap();
    println!("FDN v49 hash: {KERNEL_CARDDB_HASH:016x}");
}

#[test]
fn exact_blue_cost_is_required() {
    for (blue, generic, legal) in [(0, 1, false), (1, 0, true)] {
        let mut state = ready();
        put(
            &mut state,
            PlayerId::P1,
            "Treetop Snarespinner",
            Zone::Battlefield,
        );
        let aura = put(&mut state, PlayerId::P0, "Witness Protection", Zone::Hand);
        state.players[0].mana_pool[1] = blue;
        state.players[0].mana_pool[5] = generic;
        assert!(
            matches!(next(&mut state), Decision::CastSpellOrPass { castable_spells, .. } if castable_spells.contains(&aura) == legal)
        );
    }
}

#[test]
fn a_noncreature_is_not_a_legal_target() {
    let mut state = ready();
    let land = put(&mut state, PlayerId::P0, "Island", Zone::Battlefield);
    let creature = put(
        &mut state,
        PlayerId::P1,
        "Treetop Snarespinner",
        Zone::Battlefield,
    );
    let aura = put(&mut state, PlayerId::P0, "Witness Protection", Zone::Hand);
    state.players[0].mana_pool[1] = 1;
    next(&mut state);
    engine::step(&mut state, Action::CastSpell(aura)).unwrap();
    let before = state.state_hash();
    assert!(engine::step(&mut state, Action::ChooseTarget(Target::Object(land))).is_err());
    assert_eq!(state.state_hash(), before);
    engine::step(&mut state, Action::ChooseTarget(Target::Object(creature))).unwrap();
    finish(&mut state);
}

#[test]
fn all_overridden_characteristics_and_legendary_supertype_are_exact() {
    let mut state = ready();
    let creature = put(
        &mut state,
        PlayerId::P0,
        "Dwynen, Gilt-Leaf Daen",
        Zone::Battlefield,
    );
    let card_def = state.objects.get(creature).card_def;
    witness(&mut state, creature);
    assert_eq!(
        engine::effective_name(&state, creature),
        "Legitimate Businessperson"
    );
    assert_eq!(state.objects.get(creature).card_def, card_def);
    assert_eq!(
        engine::effective_subtype_ids(&state, creature),
        vec![Subtype::Citizen.stable_id()]
    );
    assert_eq!(
        engine::object_color_mask(&state, creature),
        (1 << ManaColor::G.pool_index()) | (1 << ManaColor::W.pool_index())
    );
    assert_eq!(
        (
            engine::effective_power(&state, creature),
            engine::effective_toughness(&state, creature)
        ),
        (1, 1)
    );
    assert!(engine::object_has_type(
        &state,
        creature,
        CardType::Creature
    ));
    assert!(!engine::object_has_type(
        &state,
        creature,
        CardType::Enchantment
    ));
    assert!(CARD_DEFS[card_def as usize]
        .supertypes
        .contains(&Supertype::Legendary));
    assert!(!engine::has_effective_keyword(
        &state,
        creature,
        Keywords::REACH
    ));
}

#[test]
fn counters_and_later_pumps_modify_the_new_base() {
    let mut state = ready();
    let creature = put(
        &mut state,
        PlayerId::P0,
        "Mossborn Hydra",
        Zone::Battlefield,
    );
    state.objects.get_mut(creature).counters.plus1_plus1 = 3;
    witness(&mut state, creature);
    assert_eq!(
        (
            engine::effective_power(&state, creature),
            engine::effective_toughness(&state, creature)
        ),
        (4, 4)
    );
    let flight = announce(&mut state, "Fleeting Flight", creature);
    finish(&mut state);
    assert_eq!(state.objects.get(flight).zone, Zone::Graveyard);
    assert!(engine::has_effective_keyword(
        &state,
        creature,
        Keywords::FLYING
    ));
    assert_eq!(engine::effective_power(&state, creature), 5);
}

#[test]
fn older_temporary_flying_is_removed_but_power_bonus_remains() {
    let mut state = ready();
    let creature = put(
        &mut state,
        PlayerId::P0,
        "Treetop Snarespinner",
        Zone::Battlefield,
    );
    announce(&mut state, "Fleeting Flight", creature);
    finish(&mut state);
    witness(&mut state, creature);
    assert!(!engine::has_effective_keyword(
        &state,
        creature,
        Keywords::FLYING
    ));
    assert_eq!(engine::effective_power(&state, creature), 2);
}

#[test]
fn older_armor_loses_flying_and_temporary_abilities_but_keeps_plus_two_power() {
    let mut state = ready();
    let creature = put(
        &mut state,
        PlayerId::P0,
        "Treetop Snarespinner",
        Zone::Battlefield,
    );
    armor(&mut state, creature);
    witness(&mut state, creature);
    for keyword in [
        Keywords::FLYING,
        Keywords::HEXPROOF,
        Keywords::INDESTRUCTIBLE,
        Keywords::REACH,
    ] {
        assert!(!engine::has_effective_keyword(&state, creature, keyword));
    }
    assert_eq!(
        (
            engine::effective_power(&state, creature),
            engine::effective_toughness(&state, creature)
        ),
        (3, 1)
    );
}

#[test]
fn later_armor_grants_flying_hexproof_and_indestructible() {
    let mut state = ready();
    let creature = put(
        &mut state,
        PlayerId::P0,
        "Treetop Snarespinner",
        Zone::Battlefield,
    );
    witness(&mut state, creature);
    armor(&mut state, creature);
    for keyword in [
        Keywords::FLYING,
        Keywords::HEXPROOF,
        Keywords::INDESTRUCTIBLE,
    ] {
        assert!(engine::has_effective_keyword(&state, creature, keyword));
    }
    assert!(!engine::has_effective_keyword(
        &state,
        creature,
        Keywords::REACH
    ));
    assert_eq!(engine::effective_power(&state, creature), 3);
}

#[test]
fn transformed_mana_creature_cannot_activate_or_pay_automatically() {
    let mut state = ready();
    let creature = put(
        &mut state,
        PlayerId::P0,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    assert!(
        matches!(next(&mut state), Decision::CastSpellOrPass { mana_abilities, .. } if mana_abilities.contains(&creature))
    );
    witness(&mut state, creature);
    assert!(
        matches!(next(&mut state), Decision::CastSpellOrPass { mana_abilities, .. } if !mana_abilities.contains(&creature))
    );
    assert!(mana::gather_sources(PlayerId::P0, &state)
        .iter()
        .all(|source| source.id != creature));
}

#[test]
fn transformed_sailor_loses_its_draw_activation() {
    let mut state = ready();
    let creature = put(
        &mut state,
        PlayerId::P0,
        "Spectral Sailor",
        Zone::Battlefield,
    );
    witness(&mut state, creature);
    state.players[0].mana_pool = [5; 6];
    assert!(
        matches!(next(&mut state), Decision::CastSpellOrPass { activatable_abilities, .. } if !activatable_abilities.iter().any(|(source, _)| *source == creature))
    );
    let before = state.state_hash();
    assert!(engine::step(&mut state, Action::ActivateAbility(creature, 0)).is_err());
    assert_eq!(state.state_hash(), before);
}

#[test]
fn removed_dwynen_lord_ability_stops_boosting_other_elves() {
    let mut state = ready();
    let lord = put(
        &mut state,
        PlayerId::P0,
        "Dwynen, Gilt-Leaf Daen",
        Zone::Battlefield,
    );
    let elf = put(
        &mut state,
        PlayerId::P0,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    assert_eq!(engine::effective_power(&state, elf), 2);
    witness(&mut state, lord);
    assert_eq!(engine::effective_power(&state, elf), 1);
    assert_eq!(engine::effective_power(&state, lord), 1);
}

#[test]
fn removed_gnarlid_static_ability_stops_granting_trample() {
    let mut state = ready();
    let colony = put(
        &mut state,
        PlayerId::P0,
        "Gnarlid Colony",
        Zone::Battlefield,
    );
    let creature = put(
        &mut state,
        PlayerId::P0,
        "Treetop Snarespinner",
        Zone::Battlefield,
    );
    state.objects.get_mut(creature).counters.plus1_plus1 = 1;
    assert!(engine::has_effective_keyword(
        &state,
        creature,
        Keywords::TRAMPLE
    ));
    witness(&mut state, colony);
    assert!(!engine::has_effective_keyword(
        &state,
        creature,
        Keywords::TRAMPLE
    ));
}

#[test]
fn percussionist_has_no_death_trigger_after_ability_removal() {
    let mut state = ready();
    let creature = put(
        &mut state,
        PlayerId::P0,
        "Clockwork Percussionist",
        Zone::Battlefield,
    );
    witness(&mut state, creature);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(creature, Zone::Graveyard),
    );
    checkpoint(&mut state);
    assert!(state.engine.pending_triggers.is_empty());
    assert!(state.players[0].battlefield.is_empty());
}

#[test]
fn simultaneous_aura_then_creature_departure_uses_pre_move_ability_state() {
    let mut state = ready();
    let creature = put(
        &mut state,
        PlayerId::P0,
        "Clockwork Percussionist",
        Zone::Battlefield,
    );
    let aura = witness(&mut state, creature);
    event::propose_and_commit_batch(
        &mut state,
        vec![
            ProposedEvent::zone_change(aura, Zone::Graveyard),
            ProposedEvent::zone_change(creature, Zone::Graveyard),
        ],
    );
    checkpoint(&mut state);
    assert!(state.engine.pending_triggers.is_empty());
}

#[test]
fn aura_removal_restores_printed_abilities_and_characteristics() {
    let mut state = ready();
    let creature = put(
        &mut state,
        PlayerId::P0,
        "Treetop Snarespinner",
        Zone::Battlefield,
    );
    let aura = witness(&mut state, creature);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(aura, Zone::Graveyard),
    );
    checkpoint(&mut state);
    assert_eq!(
        engine::effective_name(&state, creature),
        "Treetop Snarespinner"
    );
    assert_eq!(
        (
            engine::effective_power(&state, creature),
            engine::effective_toughness(&state, creature)
        ),
        (1, 4)
    );
    assert!(engine::has_effective_keyword(
        &state,
        creature,
        Keywords::REACH
    ));
}

#[test]
fn host_reentry_is_a_new_unenchanted_incarnation() {
    let mut state = ready();
    let creature = put(
        &mut state,
        PlayerId::P0,
        "Treetop Snarespinner",
        Zone::Battlefield,
    );
    let aura = witness(&mut state, creature);
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(creature, Zone::Hand));
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(creature, Zone::Battlefield),
    );
    checkpoint(&mut state);
    assert_eq!(state.objects.get(aura).zone, Zone::Graveyard);
    assert!(state.objects.get(creature).attachments.is_empty());
    assert!(engine::has_effective_keyword(
        &state,
        creature,
        Keywords::REACH
    ));
}

#[test]
fn pending_targeting_restores_and_replays_identically() {
    let mut state = ready();
    let creature = put(
        &mut state,
        PlayerId::P1,
        "Treetop Snarespinner",
        Zone::Battlefield,
    );
    let aura = put(&mut state, PlayerId::P0, "Witness Protection", Zone::Hand);
    state.players[0].mana_pool[1] = 1;
    next(&mut state);
    engine::step(&mut state, Action::CastSpell(aura)).unwrap();
    assert!(matches!(next(&mut state), Decision::ChooseTargets { .. }));
    let mut restored = restored(&state);
    assert_eq!(state.state_hash(), restored.state_hash());
    for state in [&mut state, &mut restored] {
        engine::step(state, Action::ChooseTarget(Target::Object(creature))).unwrap();
        finish(state);
    }
    assert_eq!(state.state_hash(), restored.state_hash());
    assert_eq!(
        serde_json::to_vec(&state).unwrap(),
        serde_json::to_vec(&restored).unwrap()
    );
}

#[test]
fn different_legends_renamed_to_businessperson_require_a_keep_choice() {
    let mut state = ready();
    let dwynen = put(
        &mut state,
        PlayerId::P0,
        "Dwynen, Gilt-Leaf Daen",
        Zone::Battlefield,
    );
    let koma = put(
        &mut state,
        PlayerId::P0,
        "Koma, World-Eater",
        Zone::Battlefield,
    );
    witness(&mut state, dwynen);
    announce(&mut state, "Witness Protection", koma);
    for _ in 0..30 {
        let decision = next(&mut state);
        match decision {
            Decision::ChooseLegendPermanent { candidates, .. } => {
                assert_eq!(candidates, vec![dwynen, koma]);
                let mut restored = restored(&state);
                for state in [&mut state, &mut restored] {
                    engine::step(state, Action::ChooseLegendPermanent(koma)).unwrap();
                    finish(state);
                    assert_eq!(state.objects.get(dwynen).zone, Zone::Graveyard);
                    assert_eq!(state.objects.get(koma).zone, Zone::Battlefield);
                }
                assert_eq!(state.state_hash(), restored.state_hash());
                return;
            }
            Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
            other => panic!("unexpected legend resolution: {other:?}"),
        }
    }
    panic!("renamed legend choice missing");
}

#[test]
fn a_draw_activation_already_on_the_stack_resolves_after_ability_removal() {
    let mut state = ready();
    let sailor = put(
        &mut state,
        PlayerId::P0,
        "Spectral Sailor",
        Zone::Battlefield,
    );
    state.players[0].mana_pool = [5; 6];
    next(&mut state);
    engine::step(&mut state, Action::ActivateAbility(sailor, 0)).unwrap();
    // Isolate ability independence by installing a normally attached Aura
    // while this activated ability remains on the stack. The standard Aura
    // targeting/entry path is covered by the casting tests above.
    let aura = put(&mut state, PlayerId::P0, "Witness Protection", Zone::Hand);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(aura, Zone::Battlefield),
    );
    let host_generation = state.objects.get(sailor).zone_change_count;
    state.objects.get_mut(aura).v4.attached_to = Some(mtg_kernel::state::ObjectLinkV4 {
        object: sailor,
        zone_change_count: host_generation,
    });
    state.objects.get_mut(sailor).attachments.push(aura);
    assert!(!engine::has_effective_keyword(
        &state,
        sailor,
        Keywords::FLYING
    ));
    let before = state.players[0].hand.len();
    let mut restored = restored(&state);
    for state in [&mut state, &mut restored] {
        finish(state);
        assert_eq!(state.players[0].hand.len(), before + 1);
    }
    assert_eq!(state.state_hash(), restored.state_hash());
}

#[test]
fn transformed_koma_does_not_trigger_ward_for_the_next_opponent_spell() {
    let mut state = ready();
    let koma = put(
        &mut state,
        PlayerId::P0,
        "Koma, World-Eater",
        Zone::Battlefield,
    );
    witness(&mut state, koma);
    let bolt = put(&mut state, PlayerId::P1, "Lightning Bolt", Zone::Hand);
    state.players[1].mana_pool[3] = 1;
    next(&mut state);
    engine::step(&mut state, Action::Pass).unwrap();
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass {
            player: PlayerId::P1,
            ..
        }
    ));
    engine::step(&mut state, Action::CastSpell(bolt)).unwrap();
    assert!(matches!(next(&mut state), Decision::ChooseTargets { .. }));
    engine::step(&mut state, Action::ChooseTarget(Target::Object(koma))).unwrap();
    finish(&mut state);
    assert_eq!(state.objects.get(koma).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(bolt).zone, Zone::Graveyard);
}

#[test]
fn removing_witness_before_percussionist_dies_restores_its_death_trigger() {
    let mut state = ready();
    let heir = put(
        &mut state,
        PlayerId::P0,
        "Clockwork Percussionist",
        Zone::Battlefield,
    );
    let aura = witness(&mut state, heir);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(aura, Zone::Graveyard),
    );
    checkpoint(&mut state);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(heir, Zone::Graveyard),
    );
    checkpoint(&mut state);
    assert_eq!(state.engine.pending_triggers.len(), 1);
    finish(&mut state);
    assert!(state.players[0].battlefield.is_empty());
    assert_eq!(state.players[0].exile.len(), 1);
}

fn equip(state: &mut GameState, equipment: ObjectId, target: ObjectId) {
    state.players[0].mana_pool = [5; 6];
    next(state);
    engine::step(state, Action::ActivateAbility(equipment, 0)).unwrap();
    assert!(matches!(next(state), Decision::ChooseTargets { .. }));
    engine::step(state, Action::ChooseTarget(Target::Object(target))).unwrap();
    finish(state);
}

#[test]
fn moving_older_armor_to_a_transformed_host_acquires_a_later_timestamp() {
    let mut state = ready();
    let first = put(
        &mut state,
        PlayerId::P0,
        "Treetop Snarespinner",
        Zone::Battlefield,
    );
    let second = put(
        &mut state,
        PlayerId::P0,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    let equipment = armor(&mut state, first);
    witness(&mut state, second);
    assert!(!engine::has_effective_keyword(
        &state,
        second,
        Keywords::FLYING
    ));
    let old_timestamp = state.objects.get(equipment).v4.layer_timestamp;
    equip(&mut state, equipment, second);
    assert!(state.objects.get(equipment).v4.layer_timestamp > old_timestamp);
    assert!(engine::has_effective_keyword(
        &state,
        second,
        Keywords::FLYING
    ));
    assert!(!engine::has_effective_keyword(
        &state,
        second,
        Keywords::HEXPROOF
    ));
    assert_eq!(engine::effective_power(&state, second), 3);
}

#[test]
fn equipping_the_same_host_does_not_refresh_an_older_armor_timestamp() {
    let mut state = ready();
    let creature = put(
        &mut state,
        PlayerId::P0,
        "Treetop Snarespinner",
        Zone::Battlefield,
    );
    let equipment = armor(&mut state, creature);
    witness(&mut state, creature);
    let before = state.objects.get(equipment).v4.layer_timestamp;
    equip(&mut state, equipment, creature);
    assert_eq!(state.objects.get(equipment).v4.layer_timestamp, before);
    assert!(!engine::has_effective_keyword(
        &state,
        creature,
        Keywords::FLYING
    ));
}

#[test]
fn lifelink_keyword_counters_remain_but_follow_layer_six_ordering() {
    let mut state = ready();
    let creature = put(
        &mut state,
        PlayerId::P0,
        "Treetop Snarespinner",
        Zone::Battlefield,
    );
    announce(&mut state, "Unexpected Fangs", creature);
    finish(&mut state);
    assert!(engine::has_effective_keyword(
        &state,
        creature,
        Keywords::LIFELINK
    ));
    witness(&mut state, creature);
    assert_eq!(state.objects.get(creature).v4.lifelink_keyword_counters, 1);
    assert!(!engine::has_effective_keyword(
        &state,
        creature,
        Keywords::LIFELINK
    ));
    assert_eq!(engine::effective_power(&state, creature), 2);
    announce(&mut state, "Unexpected Fangs", creature);
    finish(&mut state);
    assert_eq!(state.objects.get(creature).v4.lifelink_keyword_counters, 2);
    assert!(engine::has_effective_keyword(
        &state,
        creature,
        Keywords::LIFELINK
    ));
    assert_eq!(engine::effective_power(&state, creature), 3);
    let restored = restored(&state);
    assert_eq!(state.state_hash(), restored.state_hash());
    assert!(engine::has_effective_keyword(
        &restored,
        creature,
        Keywords::LIFELINK
    ));
}
