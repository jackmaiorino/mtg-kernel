//! Prepared targeted creature triggers; catalog admission and gameplay qualification remain pending.
#![cfg(feature = "limited-fdn-fixtures")]

use mtg_kernel::card_def::{card_id_by_name, CardCapability, CardType, Subtype, CARD_DEFS};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};
use mtg_kernel::trigger;

fn ready(player: PlayerId) -> GameState {
    let forest = card_id_by_name("Forest").unwrap();
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &[forest; 40],
        &[forest; 40],
        |_| "Forest".into(),
        835,
        player,
    );
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
        attachments: vec![],
        v4: ObjectStateV4::from_card_def(card_def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    match zone {
        Zone::Hand => state.players[player.index()].hand.push(id),
        Zone::Battlefield => state.players[player.index()].battlefield.push(id),
        _ => panic!("fixture zone"),
    }
    id
}

fn collect(state: &mut GameState) {
    let pending = trigger::collect_and_process(state);
    state.engine.pending_triggers.extend(pending);
}

fn enter(state: &mut GameState, player: PlayerId, name: &str) -> ObjectId {
    let id = put(state, player, name, Zone::Hand);
    event::propose_and_commit(state, ProposedEvent::zone_change(id, Zone::Battlefield));
    collect(state);
    id
}

fn next(state: &mut GameState) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
    decision
}

fn pass(state: &mut GameState, decision: Decision) {
    let action = match decision {
        Decision::CastSpellOrPass { .. } => Action::Pass,
        Decision::OrderTriggers { pending, .. } => {
            Action::OrderTriggers((0..pending.len()).collect())
        }
        other => panic!("unexpected decision {other:?}"),
    };
    engine::step(state, action).unwrap();
}

fn target(state: &mut GameState, wanted: ObjectId) {
    for _ in 0..32 {
        let decision = next(state);
        if let Decision::ChooseTargets {
            legal_targets,
            remaining,
            can_finish,
            ..
        } = decision
        {
            assert_eq!(legal_targets, vec![Target::Object(wanted)]);
            assert_eq!((remaining, can_finish), (1, false));
            engine::step(state, Action::ChooseTarget(Target::Object(wanted))).unwrap();
            return;
        }
        pass(state, decision);
    }
    panic!("target choice absent");
}

fn settle(state: &mut GameState) {
    for _ in 0..64 {
        let decision = next(state);
        if matches!(decision, Decision::CastSpellOrPass { .. })
            && state.stack.is_empty()
            && state.engine.pending_triggers.is_empty()
        {
            return;
        }
        pass(state, decision);
    }
    panic!("resolution did not settle");
}

fn copy(state: &GameState) -> GameState {
    serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
}

#[test]
fn morbid_and_landfall_creatures_have_exact_printing_costs_and_payment() {
    for (name, id, generic, color, colored, power, toughness, subtype) in [
        (
            "Tragic Banshee",
            357,
            4,
            ManaColor::B,
            1,
            5,
            3,
            Subtype::Spirit,
        ),
        (
            "Grappling Kraken",
            358,
            4,
            ManaColor::U,
            2,
            5,
            6,
            Subtype::Kraken,
        ),
    ] {
        let card = card_id_by_name(name).unwrap();
        assert_eq!(card, id);
        let def = &CARD_DEFS[card as usize];
        assert_eq!(def.capability, CardCapability::Full);
        assert_eq!(def.types, &[CardType::Creature]);
        assert_eq!(def.subtypes, &[subtype]);
        assert_eq!((def.power, def.toughness), (Some(power), Some(toughness)));
        assert_eq!(def.cost.generic, generic);
        assert_eq!(def.cost.pips, vec![Pip::Colored(color); colored]);
        assert_eq!(def.mana_value, u16::from(generic) + colored as u16);
        for player in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready(player);
            let spell = put(&mut state, player, name, Zone::Hand);
            state.players[player.index()].mana_pool[5] = generic + colored as u8;
            next(&mut state);
            let before = serde_json::to_vec(&state).unwrap();
            assert!(engine::step(&mut state, Action::CastSpell(spell)).is_err());
            assert_eq!(serde_json::to_vec(&state).unwrap(), before);
            state.players[player.index()].mana_pool[5] = generic;
            state.players[player.index()].mana_pool[color.pool_index()] = colored as u8;
            engine::step(&mut state, Action::CastSpell(spell)).unwrap();
            settle(&mut state);
            assert_eq!(state.objects.get(spell).zone, Zone::Battlefield);
            assert_eq!(state.players[player.index()].mana_pool, [0; 6]);
        }
    }
}

#[test]
fn banshee_samples_morbid_at_resolution_and_replays_then_expires_at_cleanup() {
    for player in [PlayerId::P0, PlayerId::P1] {
        for death_timing in [0, 1, 2] {
            let mut state = ready(player);
            let wanted = put(
                &mut state,
                player.opponent(),
                "Quakestrider Ceratops",
                Zone::Battlefield,
            );
            // Printed 12/8 plus twenty counters survives either debuff branch.
            state.objects.get_mut(wanted).counters.plus1_plus1 = 20;
            let own = put(&mut state, player, "Faerie Miscreant", Zone::Battlefield);
            if death_timing == 1 {
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(own, Zone::Graveyard),
                );
                collect(&mut state);
            }
            enter(&mut state, player, "Tragic Banshee");
            target(&mut state, wanted);
            if death_timing == 2 {
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(own, Zone::Graveyard),
                );
                collect(&mut state);
            }
            let mut restored = copy(&state);
            for current in [&mut state, &mut restored] {
                settle(current);
                let debuff = if death_timing == 0 { 1 } else { 13 };
                assert_eq!(engine::effective_power(current, wanted), 32 - debuff);
                assert_eq!(engine::effective_toughness(current, wanted), 28 - debuff);
                current.step = Step::Cleanup;
                next(current);
                assert_eq!(engine::effective_power(current, wanted), 32);
                assert_eq!(engine::effective_toughness(current, wanted), 28);
            }
            assert_eq!(state.state_hash(), restored.state_hash());
            assert_eq!(state.state_hash_v4(), restored.state_hash_v4());
        }
    }
}

#[test]
fn banshee_source_departure_keeps_trigger_but_target_reentry_or_control_change_fizzles() {
    for change in [0, 1, 2] {
        let mut state = ready(PlayerId::P0);
        let wanted = put(
            &mut state,
            PlayerId::P1,
            "Quakestrider Ceratops",
            Zone::Battlefield,
        );
        let source = enter(&mut state, PlayerId::P0, "Tragic Banshee");
        target(&mut state, wanted);
        // Exiling the source is not a creature death and must not turn on morbid.
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(source, Zone::Exile));
        if change == 1 {
            event::propose_and_commit(&mut state, ProposedEvent::zone_change(wanted, Zone::Hand));
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(wanted, Zone::Battlefield),
            );
        } else if change == 2 {
            state.players[1].battlefield.retain(|id| *id != wanted);
            state.players[0].battlefield.push(wanted);
            state.objects.get_mut(wanted).controller = PlayerId::P0;
        }
        let mut restored = copy(&state);
        for current in [&mut state, &mut restored] {
            settle(current);
            assert_eq!(
                engine::effective_toughness(current, wanted),
                if change == 0 { 7 } else { 8 }
            );
        }
        assert_eq!(state.state_hash_v4(), restored.state_hash_v4());
    }
}

#[test]
fn kraken_landfall_stuns_already_tapped_targets_and_actual_untap_consumes_one_counter() {
    for player in [PlayerId::P0, PlayerId::P1] {
        for tapped in [false, true] {
            let mut state = ready(player);
            let wanted = put(
                &mut state,
                player.opponent(),
                "Quakestrider Ceratops",
                Zone::Battlefield,
            );
            state.objects.get_mut(wanted).tapped = tapped;
            put(&mut state, player, "Grappling Kraken", Zone::Battlefield);
            enter(&mut state, player, "Forest");
            target(&mut state, wanted);
            let mut restored = copy(&state);
            for current in [&mut state, &mut restored] {
                settle(current);
                assert!(current.objects.get(wanted).tapped);
                assert_eq!(current.objects.get(wanted).counters.stun, 1);
                current.active_player = player.opponent();
                current.priority_player = player.opponent();
                current.step = Step::Untap;
                next(current);
                assert!(current.objects.get(wanted).tapped);
                assert_eq!(current.objects.get(wanted).counters.stun, 0);
            }
            assert_eq!(state.state_hash_v4(), restored.state_hash_v4());
        }
    }
}

#[test]
fn kraken_ignores_opponent_land_entry_and_illegal_target_incarnations() {
    for change in [0, 1, 2] {
        let mut state = ready(PlayerId::P0);
        let wanted = put(
            &mut state,
            PlayerId::P1,
            "Quakestrider Ceratops",
            Zone::Battlefield,
        );
        put(
            &mut state,
            PlayerId::P0,
            "Grappling Kraken",
            Zone::Battlefield,
        );
        enter(&mut state, PlayerId::P1, "Forest");
        assert!(state.engine.pending_triggers.is_empty());
        enter(&mut state, PlayerId::P0, "Forest");
        target(&mut state, wanted);
        if change == 1 {
            event::propose_and_commit(&mut state, ProposedEvent::zone_change(wanted, Zone::Hand));
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(wanted, Zone::Battlefield),
            );
        } else if change == 2 {
            state.players[1].battlefield.retain(|id| *id != wanted);
            state.players[0].battlefield.push(wanted);
            state.objects.get_mut(wanted).controller = PlayerId::P0;
        }
        let mut restored = copy(&state);
        for current in [&mut state, &mut restored] {
            settle(current);
            assert_eq!(current.objects.get(wanted).tapped, change == 0);
            assert_eq!(
                current.objects.get(wanted).counters.stun,
                if change == 0 { 1 } else { 0 }
            );
        }
        assert_eq!(state.state_hash_v4(), restored.state_hash_v4());
    }
}
