//! Source preparation. Registration and native card qualification remain pending.
#![cfg(feature = "limited-fdn-fixtures")]

use mtg_kernel::card_def::{card_id_by_name, CardType, Keywords, Subtype, CARD_DEFS};
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
        836,
        player,
    );
    state.step = Step::Main1;
    state
}

fn put(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def =
        card_id_by_name(name).unwrap_or_else(|| panic!("unregistered fixture card: {name}"));
    let id = state.objects.push(GameObject {
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
        Zone::Hand => state.players[player.index()].hand.push(id),
        Zone::Battlefield => state.players[player.index()].battlefield.push(id),
        Zone::Graveyard => state.players[player.index()].graveyard.push(id),
        _ => panic!("unsupported fixture zone"),
    }
    id
}

fn next(state: &mut GameState) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
    decision
}

fn settle(state: &mut GameState) -> Option<Decision> {
    for _ in 0..64 {
        match next(state) {
            Decision::CastSpellOrPass { .. }
                if state.stack.is_empty() && state.engine.pending_triggers.is_empty() =>
            {
                return None
            }
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            Decision::OrderTriggers { pending, .. } => {
                engine::step(state, Action::OrderTriggers((0..pending.len()).collect())).unwrap()
            }
            other => return Some(other),
        }
    }
    panic!("resolution did not settle");
}

fn copy(state: &GameState) -> GameState {
    serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
}

fn discard(state: &mut GameState, player: PlayerId, wanted: ObjectId) {
    let Some(Decision::Discard {
        player: chooser,
        count,
        choices,
    }) = settle(state)
    else {
        panic!("mandatory discard absent");
    };
    assert_eq!((chooser, count), (player, 1));
    assert!(choices.contains(&wanted));
    let before = serde_json::to_vec(state).unwrap();
    assert!(engine::step(state, Action::Discard(vec![])).is_err());
    assert_eq!(serde_json::to_vec(state).unwrap(), before);
    engine::step(state, Action::Discard(vec![wanted])).unwrap();
}

#[test]
fn printed_metadata_and_real_cast_payment_replay_for_both_seats() {
    let id = card_id_by_name("Dreadwing Scavenger").unwrap();
    assert_eq!(id, 364);
    let def = &CARD_DEFS[id as usize];
    assert_eq!(def.types, &[CardType::Creature]);
    assert_eq!(def.subtypes, &[Subtype::Nightmare, Subtype::Bird]);
    assert_eq!(
        (def.power, def.toughness, def.mana_value),
        (Some(2), Some(2), 3)
    );
    assert_eq!(def.cost.generic, 1);
    assert_eq!(
        def.cost.pips,
        &[Pip::Colored(ManaColor::U), Pip::Colored(ManaColor::B)]
    );
    assert!(def.keywords.has(Keywords::FLYING));
    assert!(!def.keywords.has(Keywords::DEATHTOUCH));
    assert_eq!(trigger::triggers_for(id).len(), 2);
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        let source = put(&mut state, player, "Dreadwing Scavenger", Zone::Hand);
        state.players[player.index()].mana_pool[5] = 3;
        next(&mut state);
        let before = serde_json::to_vec(&state).unwrap();
        assert!(engine::step(&mut state, Action::CastSpell(source)).is_err());
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        state.players[player.index()].mana_pool = [0; 6];
        state.players[player.index()].mana_pool[5] = 1;
        state.players[player.index()].mana_pool[ManaColor::U.pool_index()] = 1;
        state.players[player.index()].mana_pool[ManaColor::B.pool_index()] = 1;
        engine::step(&mut state, Action::CastSpell(source)).unwrap();
        assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
        assert!(state.engine.pending_cast.is_none());
        assert_eq!(state.players[player.index()].mana_pool, [0; 6]);
        let mut restored = copy(&state);
        for current in [&mut state, &mut restored] {
            let Some(Decision::Discard {
                player: chooser,
                count,
                choices,
            }) = settle(current)
            else {
                panic!("ETB loot absent");
            };
            assert_eq!((chooser, count, choices.len()), (player, 1, 1));
            assert_eq!(current.objects.get(source).zone, Zone::Battlefield);
            assert_eq!(current.players[player.index()].library.len(), 39);
            engine::step(current, Action::Discard(choices)).unwrap();
            assert!(settle(current).is_none());
            assert!(current.players[player.index()].hand.is_empty());
            assert_eq!(current.players[player.index()].graveyard.len(), 1);
        }
        assert_eq!(
            state.diagnostic_state_hash(),
            restored.diagnostic_state_hash()
        );
    }
}

#[test]
fn captured_etb_loot_survives_source_departure_and_discard_restores() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        let kept = put(&mut state, player, "Lightning Bolt", Zone::Hand);
        let source = put(&mut state, player, "Dreadwing Scavenger", Zone::Hand);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(source, Zone::Battlefield),
        );
        let pending = trigger::collect_and_process(&mut state);
        assert_eq!(pending.len(), 1);
        state.engine.pending_triggers.extend(pending);
        assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
        assert_eq!(state.stack.len(), 1);
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(source, Zone::Exile));
        let Some(Decision::Discard {
            player: chooser,
            count,
            choices,
        }) = settle(&mut state)
        else {
            panic!("captured source departure canceled loot");
        };
        assert_eq!((chooser, count, choices.len()), (player, 1, 2));
        let drawn = *choices.iter().find(|&&id| id != kept).unwrap();
        let mut restored = copy(&state);
        for current in [&mut state, &mut restored] {
            discard(current, player, drawn);
            assert!(settle(current).is_none());
            assert_eq!(current.players[player.index()].hand, vec![kept]);
            assert_eq!(current.objects.get(source).zone, Zone::Exile);
        }
        assert_eq!(state.state_hash(), restored.state_hash());
        assert_eq!(
            state.diagnostic_state_hash(),
            restored.diagnostic_state_hash()
        );
    }
}

#[test]
fn actual_attack_declaration_loots_once_and_preserves_madness_discard() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        let source = put(&mut state, player, "Dreadwing Scavenger", Zone::Battlefield);
        let other = put(&mut state, player, "Llanowar Elves", Zone::Battlefield);
        let temper = put(&mut state, player, "Fiery Temper", Zone::Hand);
        state.step = Step::DeclareAttackers;
        engine::step(&mut state, Action::DeclareAttackers(vec![source, other])).unwrap();
        assert_eq!(state.engine.pending_triggers.len(), 1);
        assert_eq!(state.engine.pending_triggers[0].source, source);
        discard(&mut state, player, temper);
        assert_eq!(state.players[player.index()].library.len(), 39);
        assert_eq!(state.objects.get(temper).zone, Zone::Exile);
        assert!(state
            .engine
            .pending_triggers
            .iter()
            .any(|trigger| trigger.source == temper && trigger.is_madness_offer));
    }
}

fn stats(state: &GameState, source: ObjectId) -> (i32, i32, bool) {
    (
        engine::effective_power(state, source),
        engine::effective_toughness(state, source),
        engine::has_effective_keyword(state, source, Keywords::DEATHTOUCH),
    )
}

#[test]
fn threshold_recomputes_cards_current_controller_zone_and_restored_state() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        let source = put(&mut state, player, "Dreadwing Scavenger", Zone::Battlefield);
        for _ in 0..6 {
            put(&mut state, player, "Forest", Zone::Graveyard);
        }
        for _ in 0..7 {
            put(&mut state, player.opponent(), "Forest", Zone::Graveyard);
        }
        assert_eq!(stats(&state, source), (2, 2, false));
        put(&mut state, player, "Rat Token", Zone::Graveyard);
        assert_eq!(stats(&state, source), (2, 2, false));
        let seventh = put(&mut state, player, "Forest", Zone::Graveyard);
        assert_eq!(stats(&state, source), (3, 3, true));
        assert_eq!(stats(&copy(&state), source), (3, 3, true));
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(seventh, Zone::Exile));
        assert_eq!(stats(&state, source), (2, 2, false));
        state.players[player.index()]
            .battlefield
            .retain(|&id| id != source);
        state.players[player.opponent().index()]
            .battlefield
            .push(source);
        state.objects.get_mut(source).controller = player.opponent();
        assert_eq!(stats(&state, source), (3, 3, true));
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(source, Zone::Hand));
        assert_eq!(stats(&state, source), (2, 2, false));
    }
}

#[test]
fn witness_protection_removes_both_threshold_bonus_and_printed_trigger() {
    let mut state = ready(PlayerId::P0);
    let source = put(
        &mut state,
        PlayerId::P0,
        "Dreadwing Scavenger",
        Zone::Battlefield,
    );
    for _ in 0..7 {
        put(&mut state, PlayerId::P0, "Forest", Zone::Graveyard);
    }
    assert_eq!(stats(&state, source), (3, 3, true));
    let aura = put(&mut state, PlayerId::P0, "Witness Protection", Zone::Hand);
    state.players[0].mana_pool[ManaColor::U.pool_index()] = 1;
    next(&mut state);
    engine::step(&mut state, Action::CastSpell(aura)).unwrap();
    assert!(matches!(next(&mut state), Decision::ChooseTargets { .. }));
    engine::step(&mut state, Action::ChooseTarget(Target::Object(source))).unwrap();
    assert!(settle(&mut state).is_none());
    assert_eq!(stats(&state, source), (1, 1, false));
    assert!(!engine::has_effective_keyword(
        &state,
        source,
        Keywords::FLYING
    ));
    state.step = Step::DeclareAttackers;
    engine::step(&mut state, Action::DeclareAttackers(vec![source])).unwrap();
    assert!(state.engine.pending_triggers.is_empty());
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(aura, Zone::Graveyard),
    );
    assert_eq!(stats(&state, source), (3, 3, true));
}
