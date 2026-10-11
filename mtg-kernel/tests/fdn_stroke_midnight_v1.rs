//! Unregistered Stroke of Midnight fixtures; actual gameplay admission remains pending.
#![cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]

use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardType, Keywords, Subtype, CARD_DEFS,
};
use mtg_kernel::effect::{EffectOp, ExecCtx, ObjectRef};
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

fn tokens(state: &GameState, player: PlayerId) -> Vec<ObjectId> {
    let def = card_id_by_name("Human Token").unwrap();
    state.players[player.index()]
        .battlefield
        .iter()
        .copied()
        .filter(|&id| state.objects.get(id).card_def == def)
        .collect()
}

fn announce(state: &mut GameState, player: PlayerId, target: ObjectId) -> ObjectId {
    let spell = put(state, player, "Stroke of Midnight", Zone::Hand);
    state.players[player.index()].mana_pool[ManaColor::W.pool_index()] = 1;
    state.players[player.index()].mana_pool[ManaColor::U.pool_index()] = 2;
    cast(state, spell, Some(Target::Object(target)));
    spell
}

#[test]
fn spell_token_metadata_and_actual_colored_cost_are_exact() {
    let id = card_id_by_name("Stroke of Midnight").unwrap();
    let card = &CARD_DEFS[id as usize];
    assert_eq!(card.capability, CardCapability::Full);
    assert_eq!(card.types, &[CardType::Instant]);
    assert_eq!(card.colors, &[ManaColor::W]);
    assert_eq!(card.mana_value, 3);
    assert_eq!(card.cost.generic, 2);
    assert_eq!(card.cost.pips, &[Pip::Colored(ManaColor::W)]);
    let token_id = card_id_by_name("Human Token").unwrap();
    let token = &CARD_DEFS[token_id as usize];
    assert!(token.is_token);
    assert_eq!(token.capability, CardCapability::Full);
    assert_eq!(token.types, &[CardType::Creature]);
    assert_eq!(token.subtypes, &[Subtype::Human]);
    assert_eq!(token.colors, &[ManaColor::W]);
    assert_eq!(
        (token.power, token.toughness, token.mana_value),
        (Some(1), Some(1), 0)
    );
    assert_eq!(token.keywords, Keywords::NONE);
    assert!(mtg_kernel::card_def::preflight_fully_supported_deck(&[token_id]).is_err());
    for player in [PlayerId::P0, PlayerId::P1] {
        for (white, blue, legal) in [(0, 3, false), (1, 1, false), (1, 2, true)] {
            let mut state = ready(player);
            let target = put(
                &mut state,
                player.opponent(),
                "Llanowar Elves",
                Zone::Battlefield,
            );
            let spell = put(&mut state, player, card.name, Zone::Hand);
            state.players[player.index()].mana_pool[ManaColor::W.pool_index()] = white;
            state.players[player.index()].mana_pool[ManaColor::U.pool_index()] = blue;
            assert!(
                matches!(next(&mut state), Decision::CastSpellOrPass { castable_spells, .. }
                if castable_spells.contains(&spell) == legal)
            );
            if legal {
                cast(&mut state, spell, Some(Target::Object(target)));
                settle(&mut state);
                assert_eq!(state.objects.get(target).zone, Zone::Graveyard);
                assert_eq!(state.players[player.index()].mana_pool, [0; 6]);
                assert_eq!(tokens(&state, player.opponent()).len(), 1);
            } else {
                let before = state.state_hash();
                assert!(engine::step(&mut state, Action::CastSpell(spell)).is_err());
                assert_eq!(state.state_hash(), before);
            }
        }
    }
}

#[test]
fn own_and_opposing_nonland_types_create_exact_tokens_and_entry_events_after_restore() {
    for player in [PlayerId::P0, PlayerId::P1] {
        for recipient in [player, player.opponent()] {
            for name in ["Llanowar Elves", "Goldvein Pick", "Sylvan Scavenging"] {
                let mut state = ready(player);
                put(&mut state, recipient, "Dazzling Angel", Zone::Battlefield);
                let target = put(&mut state, recipient, name, Zone::Battlefield);
                let spell = announce(&mut state, player, target);
                let mut replay = restored(&state);
                for current in [&mut state, &mut replay] {
                    assert!(matches!(settle(current), Decision::CastSpellOrPass { .. }));
                    assert_eq!(current.objects.get(target).zone, Zone::Graveyard);
                    assert_eq!(current.objects.get(spell).zone, Zone::Graveyard);
                    assert_eq!(current.players[recipient.index()].life, 21);
                    assert!(tokens(current, recipient.opponent()).is_empty());
                    let humans = tokens(current, recipient);
                    assert_eq!(humans.len(), 1);
                    let human = current.objects.get(humans[0]);
                    assert_eq!(human.owner, recipient);
                    assert_eq!(human.controller, recipient);
                    assert!(human.summoning_sick);
                    assert!(!human.tapped);
                    assert_eq!(
                        (
                            engine::effective_power(current, humans[0]),
                            engine::effective_toughness(current, humans[0])
                        ),
                        (1, 1)
                    );
                }
                same(&state, &replay);
            }
        }
    }
}

#[test]
fn current_controller_receives_token_even_when_indestructible_prevents_destruction() {
    for player in [PlayerId::P0, PlayerId::P1] {
        for indestructible in [false, true] {
            let mut state = ready(player);
            let target = put(&mut state, player, "Llanowar Elves", Zone::Battlefield);
            announce(&mut state, player, target);
            state.players[player.index()]
                .battlefield
                .retain(|&id| id != target);
            state.players[player.opponent().index()]
                .battlefield
                .push(target);
            state.objects.get_mut(target).controller = player.opponent();
            if indestructible {
                mtg_kernel::effect::execute(
                    &EffectOp::GrantKeywordTargetUntilEndOfTurn {
                        object: ObjectRef::ThisSource,
                        keyword: Keywords::INDESTRUCTIBLE,
                    },
                    &ExecCtx::no_targets(target, player.opponent()),
                    &mut state,
                );
            }
            let mut replay = restored(&state);
            for current in [&mut state, &mut replay] {
                settle(current);
                assert_eq!(
                    current.objects.get(target).zone,
                    if indestructible {
                        Zone::Battlefield
                    } else {
                        Zone::Graveyard
                    }
                );
                assert_eq!(tokens(current, player.opponent()).len(), 1);
                assert!(tokens(current, player).is_empty());
            }
            same(&state, &replay);
        }
    }
}

#[test]
fn land_illegal_actions_and_stale_or_hexproof_targets_create_no_tokens() {
    for player in [PlayerId::P0, PlayerId::P1] {
        for response in 0..3 {
            let mut state = ready(player);
            let target = put(
                &mut state,
                player.opponent(),
                "Llanowar Elves",
                Zone::Battlefield,
            );
            let land = put(&mut state, player.opponent(), "Forest", Zone::Battlefield);
            let spell = put(&mut state, player, "Stroke of Midnight", Zone::Hand);
            state.players[player.index()].mana_pool[ManaColor::W.pool_index()] = 1;
            state.players[player.index()].mana_pool[ManaColor::U.pool_index()] = 2;
            next(&mut state);
            engine::step(&mut state, Action::CastSpell(spell)).unwrap();
            assert!(
                matches!(next(&mut state), Decision::ChooseTargets { legal_targets, .. }
                if !legal_targets.contains(&Target::Object(land))
                    && !legal_targets.contains(&Target::Player(player)))
            );
            let before = state.state_hash();
            assert!(engine::step(&mut state, Action::ChooseTarget(Target::Object(land))).is_err());
            assert_eq!(state.state_hash(), before);
            engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
            next(&mut state);
            if response < 2 {
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(target, Zone::Hand),
                );
                if response == 1 {
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(target, Zone::Battlefield),
                    );
                }
            } else {
                mtg_kernel::effect::execute(
                    &EffectOp::GrantKeywordTargetUntilEndOfTurn {
                        object: ObjectRef::ThisSource,
                        keyword: Keywords::HEXPROOF,
                    },
                    &ExecCtx::no_targets(target, player.opponent()),
                    &mut state,
                );
            }
            let mut replay = restored(&state);
            for current in [&mut state, &mut replay] {
                settle(current);
                assert!(tokens(current, player).is_empty());
                assert!(tokens(current, player.opponent()).is_empty());
                assert_ne!(current.objects.get(target).zone, Zone::Graveyard);
                assert_eq!(current.objects.get(spell).zone, Zone::Graveyard);
            }
            same(&state, &replay);
        }
    }
}
