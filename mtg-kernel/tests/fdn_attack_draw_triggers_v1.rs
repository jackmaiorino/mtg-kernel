//! Prepared attack and draw triggers; admission and gameplay qualification remain pending.
#![cfg(feature = "limited-fdn-fixtures")]

use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardType, Keywords, Subtype, CARD_DEFS,
};
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
            assert!(legal_targets.contains(&Target::Object(wanted)));
            assert_eq!((remaining, can_finish), (1, false));
            engine::step(state, Action::ChooseTarget(Target::Object(wanted))).unwrap();
            assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
            assert!(state.engine.pending_triggers.is_empty());
            assert_eq!(state.stack.len(), 1);
            assert_eq!(
                state.stack.last().unwrap().targets,
                vec![Target::Object(wanted)]
            );
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

fn next_turn_through_cleanup_and_untap(state: &mut GameState) {
    let previous = state.active_player;
    // EndStep grants priority. Passing both seats enters Cleanup, then
    // the opponent's Untap, executing both entry actions normally.
    state.step = Step::EndStep;
    for _ in 0..8 {
        let decision = next(state);
        if state.active_player != previous {
            assert_eq!(state.active_player, previous.opponent());
            assert_eq!(state.step, Step::Upkeep);
            return;
        }
        pass(state, decision);
    }
    panic!("turn transition did not finish");
}

#[test]
fn attack_and_draw_creatures_have_exact_metadata_and_payment() {
    for (name, id, generic, colored, power, toughness, types, subtypes, trigger_count) in [
        (
            "Battlesong Berserker",
            359,
            3,
            vec![ManaColor::R],
            3,
            4,
            vec![CardType::Creature],
            vec![Subtype::Human, Subtype::Berserker],
            1,
        ),
        (
            "Scrawling Crawler",
            360,
            3,
            vec![],
            3,
            2,
            vec![CardType::Artifact, CardType::Creature],
            vec![Subtype::Phyrexian, Subtype::Construct],
            2,
        ),
    ] {
        let card = card_id_by_name(name).unwrap();
        assert_eq!(card, id);
        let def = &CARD_DEFS[card as usize];
        assert_eq!(def.capability, CardCapability::Full);
        assert_eq!(def.types, types);
        assert_eq!(def.subtypes, subtypes);
        assert_eq!((def.power, def.toughness), (Some(power), Some(toughness)));
        assert_eq!(def.cost.generic, generic);
        assert_eq!(
            def.cost.pips,
            colored
                .iter()
                .copied()
                .map(Pip::Colored)
                .collect::<Vec<_>>()
        );
        assert_eq!(trigger::triggers_for(card).len(), trigger_count);
        for player in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready(player);
            let spell = put(&mut state, player, name, Zone::Hand);
            state.players[player.index()].mana_pool[5] = generic - 1;
            next(&mut state);
            let before = serde_json::to_vec(&state).unwrap();
            assert!(engine::step(&mut state, Action::CastSpell(spell)).is_err());
            assert_eq!(serde_json::to_vec(&state).unwrap(), before);
            state.players[player.index()].mana_pool[5] = generic;
            for color in &colored {
                state.players[player.index()].mana_pool[color.pool_index()] += 1;
            }
            engine::step(&mut state, Action::CastSpell(spell)).unwrap();
            settle(&mut state);
            assert_eq!(state.objects.get(spell).zone, Zone::Battlefield);
            assert_eq!(state.players[player.index()].mana_pool, [0; 6]);
        }
    }
}

#[test]
fn berserker_triggers_once_for_multiple_other_attackers_and_replays_boost_and_cleanup() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        let source = put(
            &mut state,
            player,
            "Battlesong Berserker",
            Zone::Battlefield,
        );
        let a = put(&mut state, player, "Faerie Miscreant", Zone::Battlefield);
        let b = put(&mut state, player, "Tolarian Terror", Zone::Battlefield);
        state.step = Step::DeclareAttackers;
        engine::step(&mut state, Action::DeclareAttackers(vec![a, b])).unwrap();
        assert_eq!(state.engine.pending_triggers.len(), 1);
        assert!(!state.objects.get(source).tapped);
        target(&mut state, source);
        let mut restored = copy(&state);
        for current in [&mut state, &mut restored] {
            settle(current);
            assert_eq!(engine::effective_power(current, source), 4);
            assert_eq!(engine::effective_toughness(current, source), 4);
            assert!(engine::has_effective_keyword(
                current,
                source,
                Keywords::MENACE
            ));
            assert_eq!(engine::effective_power(current, a), 1);
            assert!(!engine::has_effective_keyword(current, a, Keywords::MENACE));
            next_turn_through_cleanup_and_untap(current);
            assert_eq!(engine::effective_power(current, source), 3);
            assert!(!engine::has_effective_keyword(
                current,
                source,
                Keywords::MENACE
            ));
        }
        assert_eq!(state.state_hash(), restored.state_hash());
        assert_eq!(state.state_hash_v4(), restored.state_hash_v4());
    }
}

#[test]
fn berserker_has_no_empty_attack_trigger_and_target_reentry_does_not_inherit_either_effect() {
    for attack in [false, true] {
        let mut state = ready(PlayerId::P0);
        let source = put(
            &mut state,
            PlayerId::P0,
            "Battlesong Berserker",
            Zone::Battlefield,
        );
        let wanted = put(
            &mut state,
            PlayerId::P0,
            "Faerie Miscreant",
            Zone::Battlefield,
        );
        state.step = Step::DeclareAttackers;
        engine::step(
            &mut state,
            Action::DeclareAttackers(if attack { vec![wanted] } else { vec![] }),
        )
        .unwrap();
        if !attack {
            assert!(state.engine.pending_triggers.is_empty());
            continue;
        }
        target(&mut state, wanted);
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(source, Zone::Exile));
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(wanted, Zone::Hand));
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(wanted, Zone::Battlefield),
        );
        let mut restored = copy(&state);
        for current in [&mut state, &mut restored] {
            settle(current);
            assert_eq!(engine::effective_power(current, wanted), 1);
            assert!(!engine::has_effective_keyword(
                current,
                wanted,
                Keywords::MENACE
            ));
        }
        assert_eq!(state.state_hash_v4(), restored.state_hash_v4());
    }
}

#[test]
fn crawler_upkeep_draws_active_player_first_and_its_opponent_draw_trigger_is_untargeted() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        put(&mut state, player, "Scrawling Crawler", Zone::Battlefield);
        event::log_upkeep_began(&mut state, player);
        collect(&mut state);
        assert_eq!(state.engine.pending_triggers.len(), 1);
        assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
        assert_eq!(state.stack.len(), 1);
        assert!(state.stack.last().unwrap().targets.is_empty());
        let mut restored = copy(&state);
        for current in [&mut state, &mut restored] {
            settle(current);
            assert_eq!(current.players[player.index()].hand.len(), 1);
            assert_eq!(current.players[player.opponent().index()].hand.len(), 1);
            assert_eq!(current.players[player.index()].life, 20);
            assert_eq!(current.players[player.opponent().index()].life, 19);
            let draws = current
                .engine
                .event_history
                .iter()
                .filter_map(|event| match event {
                    event::CommittedEvent::Draw {
                        player,
                        object: Some(_),
                    } => Some(*player),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(draws, vec![player, player.opponent()]);
        }
        assert_eq!(state.state_hash(), restored.state_hash());
        assert_eq!(state.state_hash_v4(), restored.state_hash_v4());
    }
}

#[test]
fn crawler_counts_each_successful_opponent_draw_and_captured_triggers_survive_departure() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        let source = put(&mut state, player, "Scrawling Crawler", Zone::Battlefield);
        for _ in 0..2 {
            event::propose_and_commit(&mut state, ProposedEvent::draw(player));
        }
        for _ in 0..3 {
            event::propose_and_commit(&mut state, ProposedEvent::draw(player.opponent()));
        }
        collect(&mut state);
        assert_eq!(state.engine.pending_triggers.len(), 3);
        assert!(state
            .engine
            .pending_triggers
            .iter()
            .all(|trigger| trigger.targets.is_empty()));
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(source, Zone::Exile));
        let mut restored = copy(&state);
        for current in [&mut state, &mut restored] {
            settle(current);
            assert_eq!(current.players[player.index()].life, 20);
            assert_eq!(current.players[player.opponent().index()].life, 17);
        }
        assert_eq!(state.state_hash_v4(), restored.state_hash_v4());
    }
}

#[test]
fn crawler_ignores_opponents_upkeep_and_failed_empty_library_draw() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        put(&mut state, player, "Scrawling Crawler", Zone::Battlefield);
        event::log_upkeep_began(&mut state, player.opponent());
        collect(&mut state);
        assert!(state.engine.pending_triggers.is_empty());
        state.players[player.opponent().index()].library.clear();
        event::propose_and_commit(&mut state, ProposedEvent::draw(player.opponent()));
        collect(&mut state);
        assert!(state.engine.pending_triggers.is_empty());
        assert_eq!(state.players[player.opponent().index()].life, 20);
        assert!(state.engine.event_history.iter().any(|event| matches!(event,
            event::CommittedEvent::Draw { player: drawn, object: None } if *drawn == player.opponent())));
    }
}
