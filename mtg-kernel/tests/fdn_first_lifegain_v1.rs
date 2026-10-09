//! Source preparation for the serial v63 first-life-gain card batch.
#![cfg(feature = "limited-fdn-fixtures")]

use mtg_kernel::card_def::{card_id_by_name, CardCapability, Keywords, Subtype, CARD_DEFS};
use mtg_kernel::engine::{self, Action, Decision, UnsupportedMechanic};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};
use mtg_kernel::trigger::{self, PendingTrigger};

fn ready(active: PlayerId) -> GameState {
    let plains = card_id_by_name("Plains").unwrap();
    let mut library = vec![plains; 38];
    library.extend([
        card_id_by_name("Vanguard Seraph").unwrap(),
        card_id_by_name("Cat Collector").unwrap(),
    ]);
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &library,
        &library,
        |id| CARD_DEFS[id as usize].name.into(),
        630,
        active,
    );
    state.step = Step::Main1;
    assert!(state.life_gain_turn_v1.is_some());
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

fn gain(state: &mut GameState, player: PlayerId, amount: i32) {
    event::propose_and_commit(state, ProposedEvent::life_gain(player, amount));
}

fn move_to(state: &mut GameState, source: ObjectId, zone: Zone) {
    event::propose_and_commit(state, ProposedEvent::zone_change(source, zone));
}

fn collected(state: &mut GameState) -> Vec<PendingTrigger> {
    let pending = trigger::collect_and_process(state);
    assert!(state.engine.halted.is_none());
    pending
}

fn queue(state: &mut GameState) {
    let pending = collected(state);
    state.engine.pending_triggers.extend(pending);
}

fn next(state: &mut GameState) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
    decision
}

fn settle(state: &mut GameState) {
    for _ in 0..100 {
        let action = match next(state) {
            Decision::CastSpellOrPass { .. } if state.stack.is_empty() => return,
            Decision::CastSpellOrPass { .. } => Action::Pass,
            Decision::OrderTriggers { pending, .. } => {
                Action::OrderTriggers((0..pending.len()).collect())
            }
            Decision::ChooseEffectTargets {
                can_finish: true, ..
            } => Action::FinishEffectSelection,
            other => panic!("unexpected decision {other:?}"),
        };
        engine::step(state, action).unwrap();
    }
    panic!("trigger resolution did not settle");
}

#[test]
fn first_lifegain_definitions_append_and_preserve_printed_characteristics() {
    let seraph_id = card_id_by_name("Vanguard Seraph").unwrap();
    let collector_id = card_id_by_name("Cat Collector").unwrap();
    assert_eq!(seraph_id, 332);
    assert_eq!(collector_id, 333);
    let seraph = &CARD_DEFS[seraph_id as usize];
    let collector = &CARD_DEFS[collector_id as usize];
    for definition in [seraph, collector] {
        assert_eq!(definition.capability, CardCapability::Full);
    }
    assert_eq!(
        (seraph.mana_value, seraph.power, seraph.toughness),
        (4, Some(3), Some(3))
    );
    assert_eq!(seraph.subtypes, &[Subtype::Angel, Subtype::Warrior]);
    assert!(seraph.keywords.has(Keywords::FLYING));
    assert_eq!(
        (collector.mana_value, collector.power, collector.toughness),
        (3, Some(3), Some(2))
    );
    assert_eq!(collector.subtypes, &[Subtype::Human, Subtype::Citizen]);
    let cat = &CARD_DEFS[card_id_by_name("Cat Token").unwrap() as usize];
    assert_eq!((cat.power, cat.toughness), (Some(1), Some(1)));
    assert_eq!(cat.subtypes, &[Subtype::Cat]);
    assert_eq!(cat.colors, &[ManaColor::W]);
    assert!(cat.is_token);
}

#[test]
fn first_lifegain_each_seat_and_own_turn_restriction_are_independent() {
    for active in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(active);
        let mut expected = Vec::new();
        for player in [PlayerId::P0, PlayerId::P1] {
            expected.push(put(
                &mut state,
                player,
                "Vanguard Seraph",
                Zone::Battlefield,
            ));
            let collector = put(&mut state, player, "Cat Collector", Zone::Battlefield);
            if player == active {
                expected.push(collector);
            }
        }
        gain(&mut state, PlayerId::P0, 9);
        gain(&mut state, PlayerId::P1, 1);
        gain(&mut state, PlayerId::P0, 1);
        gain(&mut state, PlayerId::P1, 9);
        let pending = collected(&mut state);
        let mut actual: Vec<_> = pending.iter().map(|trigger| trigger.source).collect();
        actual.sort();
        expected.sort();
        assert_eq!(actual, expected);
        assert_eq!(pending.first().unwrap().controller, active);
        assert!(collected(&mut state).is_empty(), "captures drain once");
    }
}

#[test]
fn first_lifegain_before_etb_is_consumed_even_without_live_sources() {
    for name in ["Vanguard Seraph", "Cat Collector"] {
        let mut state = ready(PlayerId::P0);
        gain(&mut state, PlayerId::P0, 1);
        assert!(collected(&mut state).is_empty());
        put(&mut state, PlayerId::P0, name, Zone::Battlefield);
        gain(&mut state, PlayerId::P0, 3);
        assert!(collected(&mut state).is_empty());
    }
}

#[test]
fn first_lifegain_atomic_gain_then_etb_is_not_retroactive() {
    let mut state = ready(PlayerId::P0);
    let seraph = put(&mut state, PlayerId::P0, "Vanguard Seraph", Zone::Hand);
    gain(&mut state, PlayerId::P0, 1);
    move_to(&mut state, seraph, Zone::Battlefield);
    gain(&mut state, PlayerId::P0, 1);
    assert!(collected(&mut state).is_empty());
}

#[test]
fn first_lifegain_atomic_etb_then_gain_captures_the_live_incarnation() {
    let mut state = ready(PlayerId::P0);
    let seraph = put(&mut state, PlayerId::P0, "Vanguard Seraph", Zone::Hand);
    move_to(&mut state, seraph, Zone::Battlefield);
    gain(&mut state, PlayerId::P0, 1);
    let pending = collected(&mut state);
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].source, seraph);
    assert_eq!(pending[0].source_contract.unwrap().zone_change_count, 1);
}

#[test]
fn first_lifegain_capture_survives_atomic_departure_blink_and_control_change() {
    for destination in [Zone::Hand, Zone::Battlefield] {
        let mut state = ready(PlayerId::P0);
        let seraph = put(
            &mut state,
            PlayerId::P0,
            "Vanguard Seraph",
            Zone::Battlefield,
        );
        gain(&mut state, PlayerId::P0, 1);
        move_to(&mut state, seraph, Zone::Hand);
        if destination == Zone::Battlefield {
            move_to(&mut state, seraph, Zone::Battlefield);
        }
        let saved = state.snapshot();
        let mut restored = ready(PlayerId::P0);
        restored.restore(&saved);
        let pending = collected(&mut state);
        assert_eq!(pending, collected(&mut restored));
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].source_contract.unwrap().zone_change_count, 0);
        state.engine.pending_triggers.extend(pending);
        settle(&mut state);
        assert!(state.engine.halted.is_none());
    }
    let mut state = ready(PlayerId::P0);
    let seraph = put(
        &mut state,
        PlayerId::P0,
        "Vanguard Seraph",
        Zone::Battlefield,
    );
    gain(&mut state, PlayerId::P0, 1);
    state.objects.get_mut(seraph).controller = PlayerId::P1;
    let pending = collected(&mut state);
    assert_eq!(pending[0].controller, PlayerId::P0);
    assert_eq!(pending[0].source_contract.unwrap().controller, PlayerId::P0);
}

#[test]
fn first_lifegain_food_costs_create_one_white_cat_and_later_gains_do_not_repeat() {
    let mut state = ready(PlayerId::P0);
    let collector = put(&mut state, PlayerId::P0, "Cat Collector", Zone::Hand);
    move_to(&mut state, collector, Zone::Battlefield);
    queue(&mut state);
    settle(&mut state);
    let food = state.players[0]
        .battlefield
        .iter()
        .copied()
        .find(|&id| state.objects.get(id).card_def == card_id_by_name("Food Token").unwrap())
        .unwrap();
    assert!(
        matches!(next(&mut state), Decision::CastSpellOrPass { activatable_abilities, .. } if !activatable_abilities.contains(&(food, 0)))
    );
    state.players[0].mana_pool[5] = 2;
    assert!(
        matches!(next(&mut state), Decision::CastSpellOrPass { activatable_abilities, .. } if activatable_abilities.contains(&(food, 0)))
    );
    engine::step(&mut state, Action::ActivateAbility(food, 0)).unwrap();
    settle(&mut state);
    assert_ne!(state.objects.get(food).zone, Zone::Battlefield);
    assert_eq!(state.players[0].mana_pool, [0; 6]);
    assert_eq!(state.players[0].life, 23);
    let cats = |state: &GameState| {
        state.players[0]
            .battlefield
            .iter()
            .filter(|&&id| state.objects.get(id).card_def == card_id_by_name("Cat Token").unwrap())
            .count()
    };
    assert_eq!(cats(&state), 1);
    gain(&mut state, PlayerId::P0, 8);
    queue(&mut state);
    settle(&mut state);
    assert_eq!(cats(&state), 1);
}

#[test]
fn first_lifegain_surveil_choice_is_private_and_restores_exactly() {
    let mut state = ready(PlayerId::P0);
    put(
        &mut state,
        PlayerId::P0,
        "Vanguard Seraph",
        Zone::Battlefield,
    );
    gain(&mut state, PlayerId::P0, 1);
    queue(&mut state);
    let top = state.players[0].library[0];
    for _ in 0..30 {
        match next(&mut state) {
            Decision::ChooseEffectTargets {
                player,
                legal_targets,
                ..
            } => {
                assert_eq!(player, PlayerId::P0);
                assert_eq!(legal_targets, vec![Target::Object(top)]);
                break;
            }
            Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
            other => panic!("unexpected decision {other:?}"),
        }
    }
    assert!(state.engine.pending_effect.is_some());
    let snapshot = state.snapshot();
    let mut restored = ready(PlayerId::P0);
    restored.restore(&snapshot);
    for current in [&mut state, &mut restored] {
        engine::step(current, Action::ChooseEffectTarget(Target::Object(top))).unwrap();
        engine::step(current, Action::FinishEffectSelection).unwrap();
        settle(current);
        assert!(current.players[0].graveyard.contains(&top));
    }
    assert_eq!(state.state_hash(), restored.state_hash());
}

#[test]
fn first_lifegain_malformed_ledger_and_restored_capture_fail_explicitly() {
    let mut state = ready(PlayerId::P0);
    let seraph = put(
        &mut state,
        PlayerId::P0,
        "Vanguard Seraph",
        Zone::Battlefield,
    );
    state.life_gain_turn_v1 = None;
    gain(&mut state, PlayerId::P0, 1);
    assert_eq!(
        state.engine.halted,
        Some((UnsupportedMechanic::InvalidFirstLifeGainHistory, seraph))
    );
    let mut state = ready(PlayerId::P0);
    let seraph = put(
        &mut state,
        PlayerId::P0,
        "Vanguard Seraph",
        Zone::Battlefield,
    );
    gain(&mut state, PlayerId::P0, 1);
    state.life_gain_turn_v1.as_mut().unwrap().captures[0].ability_index = u16::MAX;
    let mut state: GameState =
        serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
    assert!(trigger::collect_and_process(&mut state).is_empty());
    assert_eq!(
        state.engine.halted,
        Some((UnsupportedMechanic::InvalidFirstLifeGainHistory, seraph))
    );
}

#[test]
fn first_lifegain_distinct_lifelink_sources_share_only_one_first_trigger() {
    let mut state = ready(PlayerId::P0);
    let seraph = put(
        &mut state,
        PlayerId::P0,
        "Vanguard Seraph",
        Zone::Battlefield,
    );
    let a = put(
        &mut state,
        PlayerId::P0,
        "Vampire Nighthawk",
        Zone::Battlefield,
    );
    let b = put(
        &mut state,
        PlayerId::P0,
        "Vampire Nighthawk",
        Zone::Battlefield,
    );
    event::propose_and_commit_batch(
        &mut state,
        vec![
            ProposedEvent::damage(a, Target::Player(PlayerId::P1), 2),
            ProposedEvent::damage(b, Target::Player(PlayerId::P1), 2),
        ],
    );
    let pending = collected(&mut state);
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].source, seraph);
    assert_eq!(state.players[0].life, 24);
}

#[test]
fn first_lifegain_prevented_lifelink_damage_does_not_consume_first_gain() {
    let mut state = ready(PlayerId::P0);
    let seraph = put(
        &mut state,
        PlayerId::P0,
        "Vanguard Seraph",
        Zone::Battlefield,
    );
    let vampire = put(
        &mut state,
        PlayerId::P0,
        "Vampire Nighthawk",
        Zone::Battlefield,
    );
    state
        .engine
        .active_replacements
        .push(event::ActiveReplacement {
            id: 1,
            source: seraph,
            kind: event::ReplacementEffectKind::PreventNextDamage {
                target: Target::Player(PlayerId::P1),
                remaining: 2,
            },
        });
    event::propose_and_commit(
        &mut state,
        ProposedEvent::damage(vampire, Target::Player(PlayerId::P1), 2),
    );
    assert_eq!(state.players[0].life, 20);
    assert!(collected(&mut state).is_empty());
    gain(&mut state, PlayerId::P0, 1);
    let pending = collected(&mut state);
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].source, seraph);
}

#[test]
fn first_lifegain_real_next_turn_resets_both_seats_and_own_turn_gate() {
    let mut state = ready(PlayerId::P0);
    let p0_seraph = put(
        &mut state,
        PlayerId::P0,
        "Vanguard Seraph",
        Zone::Battlefield,
    );
    let p1_seraph = put(
        &mut state,
        PlayerId::P1,
        "Vanguard Seraph",
        Zone::Battlefield,
    );
    put(&mut state, PlayerId::P0, "Cat Collector", Zone::Battlefield);
    let p1_collector = put(&mut state, PlayerId::P1, "Cat Collector", Zone::Battlefield);
    gain(&mut state, PlayerId::P0, 1);
    gain(&mut state, PlayerId::P1, 1);
    queue(&mut state);
    settle(&mut state);
    let old_anchor = state.life_gain_turn_v1.as_ref().unwrap().history_index;
    state.step = Step::Cleanup;
    next(&mut state);
    assert_eq!(state.active_player, PlayerId::P1);
    let ledger = state.life_gain_turn_v1.as_ref().unwrap();
    assert_eq!(ledger.active_player, PlayerId::P1);
    assert!(ledger.history_index > old_anchor);
    gain(&mut state, PlayerId::P0, 1);
    gain(&mut state, PlayerId::P1, 1);
    let pending = collected(&mut state);
    let mut actual: Vec<_> = pending.iter().map(|ability| ability.source).collect();
    let mut expected = vec![p0_seraph, p1_seraph, p1_collector];
    actual.sort();
    expected.sort();
    assert_eq!(actual, expected);
    assert_eq!(pending.first().unwrap().controller, PlayerId::P1);
}
