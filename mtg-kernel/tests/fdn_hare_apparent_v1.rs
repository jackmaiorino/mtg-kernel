//! Unregistered Hare Apparent fixtures; actual gameplay admission remains pending.
#![cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]

use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardType, Keywords, Subtype, CARD_DEFS,
};
use mtg_kernel::effect::EffectOp;
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

fn ready(active: PlayerId) -> GameState {
    let forest = card_id_by_name("Forest").unwrap();
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &[forest; 40],
        &[forest; 40],
        |_| "Forest".into(),
        1109,
        active,
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
        Zone::Library => state.players[player.index()].library.push(id),
        Zone::Graveyard => state.players[player.index()].graveyard.push(id),
        _ => panic!("fixture zone"),
    }
    id
}

fn next(state: &mut GameState) -> Decision {
    let choice = engine::advance_until_decision(state);
    assert!(!matches!(choice, Decision::Halted { .. }), "{choice:?}");
    choice
}

fn priority(state: &mut GameState, player: PlayerId) {
    let Decision::CastSpellOrPass {
        player: chooser, ..
    } = next(state)
    else {
        panic!("priority decision absent");
    };
    if chooser != player {
        engine::step(state, Action::Pass).unwrap();
        assert!(
            matches!(next(state), Decision::CastSpellOrPass { player: chooser, .. } if chooser == player)
        );
    }
}

fn cast(state: &mut GameState, spell: ObjectId, target: Option<Target>) {
    assert!(
        matches!(next(state), Decision::CastSpellOrPass { castable_spells, .. } if castable_spells.contains(&spell))
    );
    engine::step(state, Action::CastSpell(spell)).unwrap();
    for _ in 0..16 {
        match next(state) {
            Decision::ChooseTargets { legal_targets, .. } => {
                let target = target.expect("fixture target");
                assert!(legal_targets.contains(&target));
                engine::step(state, Action::ChooseTarget(target)).unwrap();
            }
            Decision::OrderTriggers { pending, .. } => {
                engine::step(state, Action::OrderTriggers((0..pending.len()).collect())).unwrap();
            }
            Decision::CastSpellOrPass { .. } if state.engine.pending_cast.is_none() => return,
            choice => panic!("unexpected cast decision {choice:?}"),
        }
    }
    panic!("cast did not finish");
}

fn settle(state: &mut GameState) -> Decision {
    for _ in 0..64 {
        let choice = next(state);
        match choice {
            Decision::CastSpellOrPass { .. }
                if state.stack.is_empty() && state.engine.pending_triggers.is_empty() =>
            {
                return choice
            }
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            Decision::OrderTriggers { ref pending, .. } => {
                engine::step(state, Action::OrderTriggers((0..pending.len()).collect())).unwrap();
            }
            _ => return choice,
        }
    }
    panic!("resolution did not settle");
}

fn restored(state: &GameState) -> GameState {
    serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
}

fn same(state: &GameState, replay: &GameState) {
    assert_eq!(
        serde_json::to_value(state).unwrap(),
        serde_json::to_value(replay).unwrap()
    );
}

fn rabbits(state: &GameState, player: PlayerId) -> Vec<ObjectId> {
    let def = card_id_by_name("Rabbit Token").unwrap();
    state.players[player.index()]
        .battlefield
        .iter()
        .copied()
        .filter(|&id| state.objects.get(id).card_def == def)
        .collect()
}

fn announce(state: &mut GameState, player: PlayerId) -> ObjectId {
    let hare = put(state, player, "Hare Apparent", Zone::Hand);
    state.players[player.index()].mana_pool[ManaColor::W.pool_index()] = 1;
    state.players[player.index()].mana_pool[ManaColor::U.pool_index()] = 1;
    cast(state, hare, None);
    hare
}

fn trigger_ready(state: &mut GameState, source: ObjectId) {
    for _ in 0..32 {
        let choice = next(state);
        if state
            .stack
            .last()
            .is_some_and(|item| item.source == source && item.inline_effect.is_some())
        {
            assert!(matches!(choice, Decision::CastSpellOrPass { .. }));
            return;
        }
        assert!(matches!(choice, Decision::CastSpellOrPass { .. }));
        engine::step(state, Action::Pass).unwrap();
    }
    panic!("entry trigger did not reach stack");
}

fn transfer(state: &mut GameState, object: ObjectId, to: PlayerId) {
    let from = state.objects.get(object).controller;
    state.players[from.index()]
        .battlefield
        .retain(|&id| id != object);
    state.players[to.index()].battlefield.push(object);
    state.objects.get_mut(object).controller = to;
}

#[test]
fn printed_hare_rabbit_and_more_than_four_copies_are_exact() {
    let id = card_id_by_name("Hare Apparent").unwrap();
    let hare = &CARD_DEFS[id as usize];
    assert_eq!(hare.capability, CardCapability::Full);
    assert_eq!(
        (hare.power, hare.toughness, hare.mana_value),
        (Some(2), Some(2), 2)
    );
    assert_eq!(hare.types, &[CardType::Creature]);
    assert_eq!(hare.subtypes, &[Subtype::Rabbit, Subtype::Noble]);
    assert_eq!(hare.colors, &[ManaColor::W]);
    assert_eq!(hare.cost.generic, 1);
    assert_eq!(hare.cost.pips, &[Pip::Colored(ManaColor::W)]);
    mtg_kernel::card_def::preflight_fully_supported_deck(&[id; 40]).unwrap();
    let token_id = card_id_by_name("Rabbit Token").unwrap();
    let token = &CARD_DEFS[token_id as usize];
    assert!(token.is_token);
    assert_eq!(token.capability, CardCapability::Full);
    assert_eq!(
        (token.power, token.toughness, token.mana_value),
        (Some(1), Some(1), 0)
    );
    assert_eq!(token.types, &[CardType::Creature]);
    assert_eq!(token.subtypes, &[Subtype::Rabbit]);
    assert_eq!(token.colors, &[ManaColor::W]);
    assert_eq!(token.keywords, Keywords::NONE);
    assert!(mtg_kernel::card_def::preflight_fully_supported_deck(&[token_id]).is_err());
}

#[test]
fn actual_cast_requires_two_mana_and_white_and_zero_other_hares_makes_no_tokens() {
    for player in [PlayerId::P0, PlayerId::P1] {
        for (white, blue, legal) in [(0, 2, false), (1, 0, false), (1, 1, true), (2, 0, true)] {
            let mut state = ready(player);
            let hare = put(&mut state, player, "Hare Apparent", Zone::Hand);
            state.players[player.index()].mana_pool[ManaColor::W.pool_index()] = white;
            state.players[player.index()].mana_pool[ManaColor::U.pool_index()] = blue;
            assert!(
                matches!(next(&mut state), Decision::CastSpellOrPass { castable_spells, .. }
                if castable_spells.contains(&hare) == legal)
            );
            let before = state.state_hash();
            if legal {
                cast(&mut state, hare, None);
                let mut replay = restored(&state);
                for current in [&mut state, &mut replay] {
                    assert!(matches!(settle(current), Decision::CastSpellOrPass { .. }));
                    assert_eq!(current.objects.get(hare).zone, Zone::Battlefield);
                    assert!(rabbits(current, player).is_empty());
                    assert_eq!(current.players[player.index()].mana_pool, [0; 6]);
                }
                same(&state, &replay);
            } else {
                assert!(engine::step(&mut state, Action::CastSpell(hare)).is_err());
                assert_eq!(state.state_hash(), before);
            }
        }
    }
}

#[test]
fn only_other_controlled_named_creatures_count_and_tokens_emit_entry_events() {
    for player in [PlayerId::P0, PlayerId::P1] {
        for count in 0..4 {
            let mut state = ready(player);
            for _ in 0..count {
                put(&mut state, player, "Hare Apparent", Zone::Battlefield);
            }
            put(
                &mut state,
                player.opponent(),
                "Hare Apparent",
                Zone::Battlefield,
            );
            put(&mut state, player, "Llanowar Elves", Zone::Battlefield);
            let noncreature = put(&mut state, player, "Forest", Zone::Battlefield);
            state.objects.get_mut(noncreature).name = "Hare Apparent".into();
            put(&mut state, player, "Dazzling Angel", Zone::Battlefield);
            announce(&mut state, player);
            let mut replay = restored(&state);
            for current in [&mut state, &mut replay] {
                assert!(matches!(settle(current), Decision::CastSpellOrPass { .. }));
                let tokens = rabbits(current, player);
                assert_eq!(tokens.len(), count);
                assert!(rabbits(current, player.opponent()).is_empty());
                assert_eq!(current.players[player.index()].life, 21 + count as i32);
                for token in tokens {
                    let object = current.objects.get(token);
                    assert_eq!(object.owner, player);
                    assert_eq!(object.controller, player);
                    assert!(!object.tapped);
                    assert!(object.summoning_sick);
                    assert_eq!(
                        (
                            engine::effective_power(current, token),
                            engine::effective_toughness(current, token)
                        ),
                        (1, 1)
                    );
                }
            }
            same(&state, &replay);
        }
    }
}

#[test]
fn responses_change_live_count_but_keep_captured_controller_and_source_incarnation() {
    for player in [PlayerId::P0, PlayerId::P1] {
        for response in 0..5 {
            let mut state = ready(player);
            let other = put(&mut state, player, "Hare Apparent", Zone::Battlefield);
            let opponent = put(
                &mut state,
                player.opponent(),
                "Hare Apparent",
                Zone::Battlefield,
            );
            let source = announce(&mut state, player);
            trigger_ready(&mut state, source);
            match response {
                0 => {
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(other, Zone::Hand),
                    );
                }
                1 => transfer(&mut state, opponent, player),
                2 => {
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(source, Zone::Hand),
                    );
                }
                3 => transfer(&mut state, source, player.opponent()),
                4 => {
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(source, Zone::Hand),
                    );
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(source, Zone::Battlefield),
                    );
                }
                _ => unreachable!(),
            }
            let expected = [0, 2, 1, 1, 3][response];
            let mut replay = restored(&state);
            for current in [&mut state, &mut replay] {
                assert!(matches!(settle(current), Decision::CastSpellOrPass { .. }));
                assert_eq!(rabbits(current, player).len(), expected);
                assert!(rabbits(current, player.opponent()).is_empty());
            }
            same(&state, &replay);
        }
    }
}

#[test]
fn effective_name_and_already_queued_ability_survive_witness_and_forged_roots_refuse() {
    for player in [PlayerId::P0, PlayerId::P1] {
        for remove_source_ability in [false, true] {
            let mut state = ready(player);
            let other = put(&mut state, player, "Hare Apparent", Zone::Battlefield);
            let source = announce(&mut state, player);
            trigger_ready(&mut state, source);
            let target = if remove_source_ability { source } else { other };
            let witness = put(&mut state, player, "Witness Protection", Zone::Hand);
            state.players[player.index()].mana_pool[ManaColor::U.pool_index()] = 1;
            priority(&mut state, player);
            cast(&mut state, witness, Some(Target::Object(target)));
            let mut replay = restored(&state);
            for current in [&mut state, &mut replay] {
                assert!(matches!(settle(current), Decision::CastSpellOrPass { .. }));
                assert_eq!(
                    rabbits(current, player).len(),
                    usize::from(remove_source_ability)
                );
                assert_ne!(engine::effective_name(current, target), "Hare Apparent");
            }
            same(&state, &replay);
        }
        let mut state = ready(player);
        let source = announce(&mut state, player);
        trigger_ready(&mut state, source);
        let mut forged = restored(&state);
        forged.stack.last_mut().unwrap().inline_effect =
            Some(EffectOp::CreateTokensForOtherControlledCreaturesNamed {
                name: "Llanowar Elves".into(),
                token_def: card_id_by_name("Rabbit Token").unwrap(),
            });
        let mut replay = restored(&forged);
        let mut halted = false;
        for _ in 0..8 {
            match engine::advance_until_decision(&mut replay) {
                Decision::Halted { .. } => {
                    halted = true;
                    break;
                }
                Decision::CastSpellOrPass { .. } => {
                    engine::step(&mut replay, Action::Pass).unwrap();
                }
                other => panic!("unexpected forged-root decision {other:?}"),
            }
        }
        assert!(halted, "forged root reached resolution without refusal");
        assert!(rabbits(&replay, player).is_empty());
        assert!(rabbits(&replay, player.opponent()).is_empty());
    }
}
