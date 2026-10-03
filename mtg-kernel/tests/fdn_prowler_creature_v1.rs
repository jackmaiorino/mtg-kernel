//! Morbid end-step timing, actual creature deaths and exact source bindings.
#![cfg(feature = "limited-fdn-fixtures")]

use mtg_kernel::card_def::{
    card_id_by_name, preflight_fully_supported_deck, Subtype, WardCostDef, CARD_DEFS,
};
use mtg_kernel::effect::EffectBooleanChoicePurpose;
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, CommittedEvent, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

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

fn pass_or_order(state: &mut GameState, decision: Decision) {
    let action = match decision {
        Decision::CastSpellOrPass { .. } => Action::Pass,
        Decision::OrderTriggers { pending, .. } => {
            Action::OrderTriggers((0..pending.len()).collect())
        }
        Decision::DeclareAttackers { .. } => Action::DeclareAttackers(vec![]),
        Decision::DeclareBlockers { .. } => Action::DeclareBlockers(vec![]),
        other => panic!("unexpected decision {other:?}"),
    };
    engine::step(state, action).unwrap();
}

fn drain(state: &mut GameState, payment: Option<bool>) -> usize {
    let mut payment_choices = 0;
    for _ in 0..100 {
        let decision = next(state);
        if matches!(decision, Decision::CastSpellOrPass { .. }) && state.stack.is_empty() {
            assert!(state.engine.pending_triggers.is_empty());
            return payment_choices;
        }
        match decision {
            Decision::ChooseEffectBoolean { .. } => {
                payment_choices += 1;
                engine::step(state, Action::ChooseEffectBoolean(payment.unwrap())).unwrap();
            }
            other => pass_or_order(state, other),
        }
    }
    panic!("resolution did not finish");
}

fn enter_end(state: &mut GameState) {
    state.step = Step::Main2;
    for _ in 0..100 {
        let decision = next(state);
        if state.step == Step::End {
            return;
        }
        pass_or_order(state, decision);
    }
    panic!("end step did not begin");
}

fn die(state: &mut GameState, player: PlayerId, name: &str) {
    let creature = put(state, player, name, Zone::Battlefield);
    event::propose_and_commit(state, ProposedEvent::zone_change(creature, Zone::Graveyard));
}

#[test]
fn printed_characteristics_and_full_admission_are_exact() {
    assert_eq!(card_id_by_name("Cackling Prowler"), Some(229));
    let prowler = &CARD_DEFS[229];
    assert_eq!(
        (prowler.power, prowler.toughness, prowler.mana_value),
        (Some(4), Some(3), 4)
    );
    assert_eq!(prowler.cost.generic, 3);
    assert_eq!(prowler.cost.pips, &[Pip::Colored(ManaColor::G)]);
    assert_eq!(prowler.colors, &[ManaColor::G]);
    assert_eq!(prowler.subtypes, &[Subtype::Hyena, Subtype::Rogue]);
    assert_eq!(prowler.ward_cost, Some(WardCostDef::Generic(2)));
    preflight_fully_supported_deck(&[229]).unwrap();
}

#[test]
fn casting_requires_four_mana_including_green_and_rejects_without_mutation() {
    for (green, blue, succeeds) in [(3, 0, false), (0, 4, false), (1, 3, true)] {
        let mut state = ready();
        let prowler = put(&mut state, PlayerId::P0, "Cackling Prowler", Zone::Hand);
        state.players[0].mana_pool[ManaColor::G.pool_index()] = green;
        state.players[0].mana_pool[ManaColor::U.pool_index()] = blue;
        let Decision::CastSpellOrPass {
            castable_spells, ..
        } = next(&mut state)
        else {
            panic!("casting window");
        };
        assert_eq!(castable_spells.contains(&prowler), succeeds);
        let before = state.state_hash();
        if succeeds {
            engine::step(&mut state, Action::CastSpell(prowler)).unwrap();
            drain(&mut state, None);
            assert_eq!(state.objects.get(prowler).zone, Zone::Battlefield);
            assert_eq!(state.players[0].mana_pool, [0; 6]);
        } else {
            assert!(engine::step(&mut state, Action::CastSpell(prowler)).is_err());
            assert_eq!(state.state_hash(), before);
        }
    }
}

#[test]
fn no_death_does_not_create_an_end_step_trigger() {
    let mut state = ready();
    let prowler = put(
        &mut state,
        PlayerId::P0,
        "Cackling Prowler",
        Zone::Battlefield,
    );
    enter_end(&mut state);
    assert!(state.stack.is_empty());
    assert!(state.engine.pending_triggers.is_empty());
    assert_eq!(state.objects.get(prowler).counters.plus1_plus1, 0);
}

#[test]
fn own_and_opposing_creature_deaths_each_add_one_counter() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready();
        let prowler = put(
            &mut state,
            PlayerId::P0,
            "Cackling Prowler",
            Zone::Battlefield,
        );
        die(&mut state, player, "Elvish Mystic");
        assert!(state.creature_died_this_turn_v1());
        enter_end(&mut state);
        drain(&mut state, None);
        assert_eq!(state.objects.get(prowler).counters.plus1_plus1, 1);
    }
}

#[test]
fn token_death_counts_even_after_the_token_ceases_to_exist() {
    let mut state = ready();
    let prowler = put(
        &mut state,
        PlayerId::P0,
        "Cackling Prowler",
        Zone::Battlefield,
    );
    die(&mut state, PlayerId::P1, "Koma's Coil Token");
    next(&mut state);
    assert!(state.players[1].graveyard.is_empty());
    assert!(state.creature_died_this_turn_v1());
    enter_end(&mut state);
    drain(&mut state, None);
    assert_eq!(state.objects.get(prowler).counters.plus1_plus1, 1);
}

#[test]
fn noncreature_death_discard_bounce_and_exile_do_not_count() {
    for (name, from, to) in [
        ("Island", Zone::Battlefield, Zone::Graveyard),
        ("Elvish Mystic", Zone::Hand, Zone::Graveyard),
        ("Elvish Mystic", Zone::Battlefield, Zone::Hand),
        ("Elvish Mystic", Zone::Battlefield, Zone::Exile),
    ] {
        let mut state = ready();
        let prowler = put(
            &mut state,
            PlayerId::P0,
            "Cackling Prowler",
            Zone::Battlefield,
        );
        let object = put(&mut state, PlayerId::P1, name, from);
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(object, to));
        assert!(!state.creature_died_this_turn_v1());
        enter_end(&mut state);
        assert!(state.stack.is_empty());
        assert_eq!(state.objects.get(prowler).counters.plus1_plus1, 0);
    }
}

#[test]
fn deaths_before_entry_and_multiple_deaths_still_add_only_one_counter() {
    let mut state = ready();
    die(&mut state, PlayerId::P0, "Elvish Mystic");
    die(&mut state, PlayerId::P1, "Elvish Mystic");
    let prowler = put(
        &mut state,
        PlayerId::P0,
        "Cackling Prowler",
        Zone::Battlefield,
    );
    enter_end(&mut state);
    drain(&mut state, None);
    assert_eq!(state.objects.get(prowler).counters.plus1_plus1, 1);
    // Repeated priority inspection cannot replay the beginning-of-step event.
    next(&mut state);
    next(&mut state);
    assert_eq!(state.objects.get(prowler).counters.plus1_plus1, 1);
    assert_eq!(
        state
            .engine
            .event_history
            .iter()
            .filter(|event| matches!(event, CommittedEvent::BeginningEndStep { .. }))
            .count(),
        1
    );
}

#[test]
fn only_the_current_controllers_end_step_triggers() {
    let mut state = ready();
    let own = put(
        &mut state,
        PlayerId::P0,
        "Cackling Prowler",
        Zone::Battlefield,
    );
    let opposing = put(
        &mut state,
        PlayerId::P1,
        "Cackling Prowler",
        Zone::Battlefield,
    );
    die(&mut state, PlayerId::P1, "Elvish Mystic");
    enter_end(&mut state);
    drain(&mut state, None);
    assert_eq!(state.objects.get(own).counters.plus1_plus1, 1);
    assert_eq!(state.objects.get(opposing).counters.plus1_plus1, 0);
}

#[test]
fn late_end_step_death_and_entry_cannot_create_a_missed_trigger() {
    for late_entry in [false, true] {
        let mut state = ready();
        let prowler = if late_entry {
            die(&mut state, PlayerId::P1, "Elvish Mystic");
            enter_end(&mut state);
            put(
                &mut state,
                PlayerId::P0,
                "Cackling Prowler",
                Zone::Battlefield,
            )
        } else {
            let prowler = put(
                &mut state,
                PlayerId::P0,
                "Cackling Prowler",
                Zone::Battlefield,
            );
            enter_end(&mut state);
            die(&mut state, PlayerId::P1, "Elvish Mystic");
            prowler
        };
        drain(&mut state, None);
        assert_eq!(state.objects.get(prowler).counters.plus1_plus1, 0);
    }
}

#[test]
fn next_players_turn_resets_death_history_even_when_round_number_is_unchanged() {
    let mut state = ready();
    let opposing = put(
        &mut state,
        PlayerId::P1,
        "Cackling Prowler",
        Zone::Battlefield,
    );
    die(&mut state, PlayerId::P0, "Elvish Mystic");
    let round = state.turn;
    enter_end(&mut state);
    for _ in 0..100 {
        let decision = next(&mut state);
        if state.active_player == PlayerId::P1 && state.step == Step::Main1 {
            break;
        }
        pass_or_order(&mut state, decision);
    }
    assert_eq!(state.active_player, PlayerId::P1);
    assert_eq!(state.step, Step::Main1);
    assert_eq!(state.turn, round);
    assert!(!state.creature_died_this_turn_v1());
    assert!(state.creature_death_turn_v1.is_none());
    enter_end(&mut state);
    assert!(state.stack.is_empty());
    assert_eq!(state.objects.get(opposing).counters.plus1_plus1, 0);
}

#[test]
fn queued_trigger_does_not_add_a_counter_to_a_returned_source_incarnation() {
    let mut state = ready();
    let prowler = put(
        &mut state,
        PlayerId::P0,
        "Cackling Prowler",
        Zone::Battlefield,
    );
    die(&mut state, PlayerId::P1, "Elvish Mystic");
    enter_end(&mut state);
    assert_eq!(state.stack.len(), 1);
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(prowler, Zone::Hand));
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(prowler, Zone::Battlefield),
    );
    drain(&mut state, None);
    assert_eq!(state.objects.get(prowler).counters.plus1_plus1, 0);
}

#[test]
fn death_history_and_queued_conditional_trigger_restore_identically() {
    let mut state = ready();
    let prowler = put(
        &mut state,
        PlayerId::P0,
        "Cackling Prowler",
        Zone::Battlefield,
    );
    die(&mut state, PlayerId::P1, "Elvish Mystic");
    let before_entry = serde_json::to_vec(&state).unwrap();
    let mut restored_before: GameState = serde_json::from_slice(&before_entry).unwrap();
    enter_end(&mut state);
    enter_end(&mut restored_before);
    assert_eq!(state.state_hash(), restored_before.state_hash());
    let pending = serde_json::to_vec(&state).unwrap();
    let mut restored_pending: GameState = serde_json::from_slice(&pending).unwrap();
    drain(&mut state, None);
    drain(&mut restored_before, None);
    drain(&mut restored_pending, None);
    assert_eq!(state.state_hash(), restored_before.state_hash());
    assert_eq!(state.state_hash(), restored_pending.state_hash());
    assert_eq!(state.objects.get(prowler).counters.plus1_plus1, 1);
}

fn opposing_snap(state: &mut GameState, prowler: ObjectId, mana: u8) -> ObjectId {
    let snap = put(state, PlayerId::P1, "Snap", Zone::Hand);
    state.players[1].mana_pool[ManaColor::U.pool_index()] = mana;
    let decision = next(state);
    pass_or_order(state, decision);
    assert!(matches!(
        next(state),
        Decision::CastSpellOrPass {
            player: PlayerId::P1,
            ..
        }
    ));
    engine::step(state, Action::CastSpell(snap)).unwrap();
    assert!(matches!(next(state), Decision::ChooseTargets { .. }));
    engine::step(state, Action::ChooseTarget(Target::Object(prowler))).unwrap();
    snap
}

#[test]
fn declining_or_unpayable_ward_counters_the_opposing_spell() {
    for (mana, payment, choices) in [(4, Some(false), 1), (2, None, 0)] {
        let mut state = ready();
        let prowler = put(
            &mut state,
            PlayerId::P0,
            "Cackling Prowler",
            Zone::Battlefield,
        );
        let snap = opposing_snap(&mut state, prowler, mana);
        assert_eq!(drain(&mut state, payment), choices);
        assert_eq!(state.objects.get(prowler).zone, Zone::Battlefield);
        assert_eq!(state.objects.get(snap).zone, Zone::Graveyard);
    }
}

#[test]
fn payable_ward_costs_two_and_its_pending_choice_restores_identically() {
    let mut state = ready();
    let prowler = put(
        &mut state,
        PlayerId::P0,
        "Cackling Prowler",
        Zone::Battlefield,
    );
    opposing_snap(&mut state, prowler, 4);
    for _ in 0..2 {
        let decision = next(&mut state);
        pass_or_order(&mut state, decision);
    }
    assert!(matches!(
        next(&mut state),
        Decision::ChooseEffectBoolean {
            player: PlayerId::P1,
            purpose: EffectBooleanChoicePurpose::CounterUnlessPaysGeneric { generic: 2, .. },
            ..
        }
    ));
    let pending = serde_json::to_vec(&state).unwrap();
    let mut restored: GameState = serde_json::from_slice(&pending).unwrap();
    engine::step(&mut state, Action::ChooseEffectBoolean(true)).unwrap();
    engine::step(&mut restored, Action::ChooseEffectBoolean(true)).unwrap();
    drain(&mut state, None);
    drain(&mut restored, None);
    assert_eq!(state.state_hash(), restored.state_hash());
    assert_eq!(state.objects.get(prowler).zone, Zone::Hand);
    assert_eq!(state.players[1].mana_pool, [0; 6]);
}

#[test]
fn prowler_and_monarch_share_the_end_step_ordering_window() {
    for order in [vec![0, 1], vec![1, 0]] {
        let mut state = ready();
        let admiral = put(&mut state, PlayerId::P0, "Azure Fleet Admiral", Zone::Hand);
        state.players[0].mana_pool[ManaColor::U.pool_index()] = 1;
        state.players[0].mana_pool[ManaColor::C.pool_index()] = 3;
        assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
        engine::step(&mut state, Action::CastSpell(admiral)).unwrap();
        drain(&mut state, None);
        assert_eq!(state.monarch, Some(PlayerId::P0));
        let prowler = put(
            &mut state,
            PlayerId::P0,
            "Cackling Prowler",
            Zone::Battlefield,
        );
        die(&mut state, PlayerId::P1, "Elvish Mystic");
        let hand_before = state.players[0].hand.len();
        enter_end(&mut state);
        let Decision::OrderTriggers { player, pending } = next(&mut state) else {
            panic!("simultaneous end-step triggers must offer one ordering decision");
        };
        assert_eq!(player, PlayerId::P0);
        assert_eq!(pending.len(), 2);
        assert!(pending.iter().any(|trigger| trigger.source == admiral));
        assert!(pending.iter().any(|trigger| trigger.source == prowler));
        engine::step(&mut state, Action::OrderTriggers(order)).unwrap();
        drain(&mut state, None);
        assert_eq!(state.players[0].hand.len(), hand_before + 1);
        assert_eq!(state.objects.get(prowler).counters.plus1_plus1, 1);
    }
}
