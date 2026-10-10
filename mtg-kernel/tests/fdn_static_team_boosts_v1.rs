//! Source preparation for serial v65, before registry/profile admission.
#![cfg(feature = "limited-fdn-fixtures")]

use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardType, Keywords, Subtype, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};
use mtg_kernel::trigger;

fn ready() -> GameState {
    let plains = card_id_by_name("Plains").unwrap();
    let mut state =
        GameState::new_from_libraries(&[plains; 40], &[plains; 40], |_| "Plains".into(), 650);
    state.step = Step::Main1;
    state
}

fn put(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id_by_name(name).unwrap();
    let id = state.objects.push(GameObject {
        card_def,
        name: name.into(),
        owner: player,
        controller: player,
        zone,
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
    match zone {
        Zone::Battlefield => state.players[player.index()].battlefield.push(id),
        Zone::Hand => state.players[player.index()].hand.push(id),
        _ => panic!("helper zone"),
    }
    id
}

fn stats(state: &GameState, object: ObjectId) -> (i32, i32) {
    (
        engine::effective_power(state, object),
        engine::effective_toughness(state, object),
    )
}

fn move_to(state: &mut GameState, object: ObjectId, zone: Zone) {
    event::propose_and_commit(state, ProposedEvent::zone_change(object, zone));
}

fn control(state: &mut GameState, object: ObjectId, player: PlayerId) {
    let old = state.objects.get(object).controller;
    state.players[old.index()]
        .battlefield
        .retain(|id| *id != object);
    state.players[player.index()].battlefield.push(object);
    state.objects.get_mut(object).controller = player;
}

fn next(state: &mut GameState) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
    decision
}

fn settle(state: &mut GameState) {
    for _ in 0..100 {
        match next(state) {
            Decision::CastSpellOrPass { .. } if state.stack.is_empty() => return,
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            Decision::OrderTriggers { pending, .. } => {
                engine::step(state, Action::OrderTriggers((0..pending.len()).collect())).unwrap()
            }
            other => panic!("unexpected decision {other:?}"),
        }
    }
    panic!("did not settle");
}

fn pass_end_into_cleanup(state: &mut GameState) {
    assert!(state.stack.is_empty());
    assert!(!state.engine.until_end_of_turn.is_empty());
    let active = state.active_player;
    state.step = Step::End;
    state.priority_player = active;
    state.engine.priority_passes = [false, false];
    for _ in 0..8 {
        assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
        if state.active_player != active {
            assert_eq!(state.active_player, active.opponent());
            assert!(state.engine.until_end_of_turn.is_empty());
            return;
        }
        engine::step(state, Action::Pass).unwrap();
    }
    panic!("End passes did not enter Cleanup and the next turn");
}

fn cast_targeted(state: &mut GameState, name: &str, target: ObjectId) -> ObjectId {
    let spell = put(state, PlayerId::P0, name, Zone::Hand);
    state.players[0].mana_pool = [5; 6];
    assert!(
        matches!(next(state), Decision::CastSpellOrPass { castable_spells, .. }
        if castable_spells.contains(&spell))
    );
    engine::step(state, Action::CastSpell(spell)).unwrap();
    assert!(matches!(next(state), Decision::ChooseTargets { .. }));
    engine::step(state, Action::ChooseTarget(Target::Object(target))).unwrap();
    settle(state);
    state.players[0].mana_pool = [0; 6];
    spell
}

#[test]
fn static_team_definitions_match_printed_rules() {
    let anthem_id = card_id_by_name("Anthem of Champions").unwrap();
    let eagle_id = card_id_by_name("Empyrean Eagle").unwrap();
    assert_eq!((anthem_id, eagle_id), (336, 337));
    let anthem = &CARD_DEFS[anthem_id as usize];
    let eagle = &CARD_DEFS[eagle_id as usize];
    assert_eq!(anthem.capability, CardCapability::Full);
    assert_eq!(eagle.capability, CardCapability::Full);
    assert_eq!(anthem.types, &[CardType::Enchantment]);
    assert_eq!(anthem.colors, &[ManaColor::W, ManaColor::G]);
    assert_eq!(anthem.mana_value, 2);
    assert_eq!((anthem.power, anthem.toughness), (None, None));
    assert_eq!(eagle.types, &[CardType::Creature]);
    assert_eq!(eagle.subtypes, &[Subtype::Bird, Subtype::Spirit]);
    assert_eq!(eagle.colors, &[ManaColor::W, ManaColor::U]);
    assert_eq!(eagle.mana_value, 3);
    assert_eq!((eagle.power, eagle.toughness), (Some(2), Some(3)));
    assert!(eagle.keywords.has(Keywords::FLYING));
}

#[test]
fn anthem_is_continuous_stacks_and_matches_only_controlled_battlefield_creatures() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready();
        put(&mut state, player, "Anthem of Champions", Zone::Battlefield);
        let creature = put(&mut state, player, "Llanowar Elves", Zone::Battlefield);
        let enemy = put(
            &mut state,
            player.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        let land = put(&mut state, player, "Plains", Zone::Battlefield);
        let late = put(&mut state, player, "Llanowar Elves", Zone::Hand);
        assert_eq!(stats(&state, creature), (2, 2));
        assert_eq!(stats(&state, enemy), (1, 1));
        assert_eq!(stats(&state, land), (0, 0));
        assert_eq!(stats(&state, late), (1, 1));
        move_to(&mut state, late, Zone::Battlefield);
        assert_eq!(stats(&state, late), (2, 2));
        put(&mut state, player, "Anthem of Champions", Zone::Battlefield);
        assert_eq!(stats(&state, creature), (3, 3));
        assert_eq!(stats(&state, late), (3, 3));
    }
}

#[test]
fn eagles_exclude_themselves_but_boost_each_other_and_other_fliers() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready();
        let a = put(&mut state, player, "Empyrean Eagle", Zone::Battlefield);
        let hawk = put(&mut state, player, "Squadron Hawk", Zone::Battlefield);
        let elf = put(&mut state, player, "Llanowar Elves", Zone::Battlefield);
        let reach = put(&mut state, player, "Generous Ent", Zone::Battlefield);
        let enemy = put(
            &mut state,
            player.opponent(),
            "Squadron Hawk",
            Zone::Battlefield,
        );
        assert_eq!(stats(&state, a), (2, 3));
        assert_eq!(stats(&state, hawk), (2, 2));
        assert_eq!(stats(&state, elf), (1, 1));
        assert_eq!(stats(&state, reach), (5, 7)); // reach alone does not match
        assert_eq!(stats(&state, enemy), (1, 1));
        let b = put(&mut state, player, "Empyrean Eagle", Zone::Battlefield);
        assert_eq!(stats(&state, a), (3, 4));
        assert_eq!(stats(&state, b), (3, 4));
        assert_eq!(stats(&state, hawk), (3, 3));
        put(&mut state, player, "Anthem of Champions", Zone::Battlefield);
        assert_eq!(stats(&state, a), (4, 5));
        assert_eq!(stats(&state, hawk), (4, 4));
    }
}

#[test]
fn static_team_sources_and_recipients_follow_live_controller_and_zone() {
    let mut state = ready();
    let anthem = put(
        &mut state,
        PlayerId::P0,
        "Anthem of Champions",
        Zone::Battlefield,
    );
    let eagle = put(
        &mut state,
        PlayerId::P0,
        "Empyrean Eagle",
        Zone::Battlefield,
    );
    let a = put(&mut state, PlayerId::P0, "Squadron Hawk", Zone::Battlefield);
    let b = put(&mut state, PlayerId::P1, "Squadron Hawk", Zone::Battlefield);
    assert_eq!(stats(&state, a), (3, 3));
    control(&mut state, eagle, PlayerId::P1);
    assert_eq!(stats(&state, a), (2, 2));
    assert_eq!(stats(&state, b), (2, 2));
    control(&mut state, a, PlayerId::P1);
    assert_eq!(stats(&state, a), (2, 2));
    control(&mut state, anthem, PlayerId::P1);
    assert_eq!(stats(&state, a), (3, 3));
    move_to(&mut state, eagle, Zone::Hand);
    assert_eq!(stats(&state, a), (2, 2));
    move_to(&mut state, eagle, Zone::Battlefield);
    // Returning a card restores its owner as controller under the zone change.
    assert_eq!(state.objects.get(eagle).controller, PlayerId::P0);
    assert_eq!(stats(&state, a), (2, 2));
    move_to(&mut state, a, Zone::Hand);
    assert_eq!(stats(&state, a), (1, 1));
    move_to(&mut state, anthem, Zone::Graveyard);
    assert_eq!(stats(&state, b), (1, 1));
}

#[test]
fn eagle_uses_layered_flying_and_team_boost_applies_after_base_override() {
    let mut state = ready();
    put(
        &mut state,
        PlayerId::P0,
        "Empyrean Eagle",
        Zone::Battlefield,
    );
    let elf = put(
        &mut state,
        PlayerId::P0,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    cast_targeted(&mut state, "Fleeting Flight", elf);
    assert!(engine::has_effective_keyword(&state, elf, Keywords::FLYING));
    assert_eq!(stats(&state, elf), (3, 3)); // counter plus Eagle bonus
    cast_targeted(&mut state, "Witness Protection", elf);
    assert!(!engine::has_effective_keyword(
        &state,
        elf,
        Keywords::FLYING
    ));
    assert_eq!(stats(&state, elf), (2, 2)); // base 1/1 plus counter
    cast_targeted(&mut state, "Fleeting Flight", elf);
    assert!(engine::has_effective_keyword(&state, elf, Keywords::FLYING));
    assert_eq!(stats(&state, elf), (4, 4)); // later flying grant survives removal
    put(
        &mut state,
        PlayerId::P0,
        "Anthem of Champions",
        Zone::Battlefield,
    );
    assert_eq!(stats(&state, elf), (5, 5));
    pass_end_into_cleanup(&mut state);
    assert!(!engine::has_effective_keyword(
        &state,
        elf,
        Keywords::FLYING
    ));
    assert_eq!(stats(&state, elf), (4, 4)); // counters and Anthem persist
}

#[test]
fn removing_source_printed_abilities_removes_only_its_continuous_bonus() {
    let mut state = ready();
    let eagle = put(
        &mut state,
        PlayerId::P0,
        "Empyrean Eagle",
        Zone::Battlefield,
    );
    let hawk = put(&mut state, PlayerId::P0, "Squadron Hawk", Zone::Battlefield);
    put(
        &mut state,
        PlayerId::P0,
        "Anthem of Champions",
        Zone::Battlefield,
    );
    let aura = cast_targeted(&mut state, "Witness Protection", eagle);
    assert_eq!(stats(&state, eagle), (2, 2)); // override 1/1 plus Anthem
    assert_eq!(stats(&state, hawk), (2, 2)); // only Anthem remains
    assert!(!engine::has_effective_keyword(
        &state,
        eagle,
        Keywords::FLYING
    ));
    move_to(&mut state, aura, Zone::Graveyard);
    assert_eq!(stats(&state, eagle), (3, 4));
    assert_eq!(stats(&state, hawk), (3, 3));
    assert!(engine::has_effective_keyword(
        &state,
        eagle,
        Keywords::FLYING
    ));
}

#[test]
fn source_departure_recomputes_toughness_before_sba() {
    for source_name in ["Anthem of Champions", "Empyrean Eagle"] {
        let mut state = ready();
        let source = put(&mut state, PlayerId::P0, source_name, Zone::Battlefield);
        let hawk = put(&mut state, PlayerId::P0, "Squadron Hawk", Zone::Battlefield);
        let enemy = put(
            &mut state,
            PlayerId::P1,
            "Llanowar Elves",
            Zone::Battlefield,
        );
        event::propose_and_commit(
            &mut state,
            ProposedEvent::damage(enemy, Target::Object(hawk), 1),
        );
        trigger::sba_fixed_point(&mut state);
        assert_eq!(state.objects.get(hawk).zone, Zone::Battlefield);
        move_to(&mut state, source, Zone::Hand);
        trigger::sba_fixed_point(&mut state);
        assert_eq!(state.objects.get(hawk).zone, Zone::Graveyard);
        assert!(state.engine.halted.is_none());
    }
}

#[test]
fn static_team_bonus_changes_actual_unblocked_combat_damage() {
    let mut state = ready();
    mtg_kernel::combat_damage_v1::enable_foundations_combat_v1(&mut state).unwrap();
    let hawk = put(&mut state, PlayerId::P0, "Squadron Hawk", Zone::Battlefield);
    put(
        &mut state,
        PlayerId::P0,
        "Empyrean Eagle",
        Zone::Battlefield,
    );
    put(
        &mut state,
        PlayerId::P0,
        "Anthem of Champions",
        Zone::Battlefield,
    );
    state.step = Step::DeclareAttackers;
    state.engine.priority_passes = [false, false];
    assert!(matches!(
        next(&mut state),
        Decision::DeclareAttackers { .. }
    ));
    engine::step(&mut state, Action::DeclareAttackers(vec![hawk])).unwrap();
    for _ in 0..60 {
        let action = match next(&mut state) {
            Decision::CastSpellOrPass { .. } if state.step == Step::EndCombat => break,
            Decision::CastSpellOrPass { .. } => Action::Pass,
            Decision::DeclareBlockers { .. } => Action::DeclareBlockers(Vec::new()),
            other => panic!("unexpected combat decision {other:?}"),
        };
        engine::step(&mut state, action).unwrap();
    }
    assert_eq!(state.players[1].life, 17);
    assert!(state.engine.halted.is_none());
}

#[test]
fn static_team_real_costs_and_pending_cast_restore_match() {
    for (name, wrong, exact, expected) in [
        (
            "Anthem of Champions",
            [0, 0, 0, 0, 0, 2],
            [1, 0, 0, 0, 1, 0],
            (2, 2),
        ),
        (
            "Empyrean Eagle",
            [2, 0, 0, 0, 0, 1],
            [1, 1, 0, 0, 0, 1],
            (2, 2),
        ),
    ] {
        let mut state = ready();
        let hawk = put(&mut state, PlayerId::P0, "Squadron Hawk", Zone::Battlefield);
        let spell = put(&mut state, PlayerId::P0, name, Zone::Hand);
        state.players[0].mana_pool = wrong;
        assert!(
            matches!(next(&mut state), Decision::CastSpellOrPass { castable_spells, .. }
            if !castable_spells.contains(&spell))
        );
        state.players[0].mana_pool = exact;
        assert!(
            matches!(next(&mut state), Decision::CastSpellOrPass { castable_spells, .. }
            if castable_spells.contains(&spell))
        );
        engine::step(&mut state, Action::CastSpell(spell)).unwrap();
        assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
        assert_eq!(state.players[0].mana_pool, [0; 6]);
        assert_eq!(stats(&state, hawk), (1, 1));
        let saved = state.snapshot();
        let bytes = serde_json::to_vec(&state).unwrap();
        let mut restored: GameState = serde_json::from_slice(&bytes).unwrap();
        settle(&mut state);
        settle(&mut restored);
        assert_eq!(stats(&state, hawk), expected);
        assert_eq!(state, restored);
        state.restore(&saved);
        settle(&mut state);
        assert_eq!(state, restored);
    }
}
