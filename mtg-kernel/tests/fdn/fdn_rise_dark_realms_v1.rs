//! Prepared Rise of the Dark Realms cases; registration and gameplay qualification are pending.
#![cfg(feature = "limited-fdn-fixtures")]

use mtg_kernel::card_def::{card_id_by_name, CardCapability, CardType, TargetSpec, CARD_DEFS};
use mtg_kernel::effect::EffectOp;
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Zone};

fn ready(player: PlayerId) -> GameState {
    let forest = card_id_by_name("Forest").unwrap();
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &[forest; 40],
        &[forest; 40],
        |_| "Forest".into(),
        810,
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
        Zone::Battlefield => state.players[player.index()].battlefield.push(id),
        Zone::Hand => state.players[player.index()].hand.push(id),
        Zone::Graveyard => state.players[player.index()].graveyard.push(id),
        _ => panic!("fixture zone"),
    }
    id
}

fn next(state: &mut GameState) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
    decision
}

fn settle(state: &mut GameState) {
    for _ in 0..64 {
        let action = match next(state) {
            Decision::CastSpellOrPass { .. } if state.stack.is_empty() => return,
            Decision::CastSpellOrPass { .. } => Action::Pass,
            Decision::OrderTriggers { pending, .. } => {
                Action::OrderTriggers((0..pending.len()).collect())
            }
            other => panic!("unexpected resolution decision {other:?}"),
        };
        engine::step(state, action).unwrap();
    }
    panic!("resolution did not settle");
}

fn cast(state: &mut GameState, player: PlayerId) -> ObjectId {
    let spell = put(state, player, "Rise of the Dark Realms", Zone::Hand);
    state.players[player.index()].mana_pool[5] = 7;
    state.players[player.index()].mana_pool[ManaColor::B.pool_index()] = 2;
    next(state);
    engine::step(state, Action::CastSpell(spell)).unwrap();
    assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
    assert_eq!(state.stack.len(), 1);
    assert!(state.engine.pending_cast.is_none());
    spell
}

#[test]
fn dark_realms_has_printed_sorcery_cost_and_untargeted_all_graveyard_program() {
    let id = card_id_by_name("Rise of the Dark Realms").unwrap();
    assert_eq!(id, 356);
    let def = &CARD_DEFS[id as usize];
    assert_eq!(def.capability, CardCapability::Full);
    assert_eq!(def.types, &[CardType::Sorcery]);
    assert_eq!(def.mana_value, 9);
    assert_eq!(def.cost.generic, 7);
    assert_eq!(
        def.cost.pips,
        &[Pip::Colored(ManaColor::B), Pip::Colored(ManaColor::B)]
    );
    assert_eq!(def.target_spec, TargetSpec::None);
    assert_eq!(
        (def.spell_effect)(),
        Some(EffectOp::ReturnAllGraveyardCreaturesUnderController)
    );
    let mut state = ready(PlayerId::P0);
    let spell = put(
        &mut state,
        PlayerId::P0,
        "Rise of the Dark Realms",
        Zone::Hand,
    );
    state.players[0].mana_pool[5] = 9;
    next(&mut state);
    let before = serde_json::to_vec(&state).unwrap();
    assert!(engine::step(&mut state, Action::CastSpell(spell)).is_err());
    assert_eq!(serde_json::to_vec(&state).unwrap(), before);
}

#[test]
fn dark_realms_takes_both_graveyards_retains_owners_and_restores_stack_and_etbs() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        let own = put(&mut state, player, "Faerie Miscreant", Zone::Graveyard);
        let stolen = put(
            &mut state,
            player.opponent(),
            "Faerie Miscreant",
            Zone::Graveyard,
        );
        let large = put(
            &mut state,
            player.opponent(),
            "Tolarian Terror",
            Zone::Graveyard,
        );
        let excluded = [
            put(&mut state, player, "Forest", Zone::Graveyard),
            put(
                &mut state,
                player.opponent(),
                "Counterspell",
                Zone::Graveyard,
            ),
        ];
        let untouched = put(&mut state, player.opponent(), "Tolarian Terror", Zone::Hand);
        let spell = cast(&mut state, player);
        let mut stack_restore: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        settle(&mut stack_restore);
        let mut found = false;
        for _ in 0..32 {
            match next(&mut state) {
                Decision::OrderTriggers { pending, .. } => {
                    assert_eq!(pending.len(), 2);
                    found = true;
                    break;
                }
                Decision::CastSpellOrPass { .. } if !state.stack.is_empty() => {
                    engine::step(&mut state, Action::Pass).unwrap()
                }
                other => panic!("missing simultaneous ETB order {other:?}"),
            }
        }
        assert!(found);
        let mut order_restore: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        settle(&mut state);
        settle(&mut order_restore);
        assert_eq!(state.state_hash(), stack_restore.state_hash());
        assert_eq!(state.state_hash(), order_restore.state_hash());
        for branch in [&mut state, &mut stack_restore, &mut order_restore] {
            for (id, owner) in [
                (own, player),
                (stolen, player.opponent()),
                (large, player.opponent()),
            ] {
                let object = branch.objects.get(id);
                assert_eq!(object.zone, Zone::Battlefield);
                assert_eq!(object.controller, player);
                assert_eq!(object.owner, owner);
                assert_eq!(object.zone_change_count, 1);
                assert!(!object.tapped);
                assert!(object.summoning_sick);
                assert!(branch.players[player.index()].battlefield.contains(&id));
                assert!(!branch.players[player.opponent().index()]
                    .battlefield
                    .contains(&id));
            }
            for id in excluded {
                assert_eq!(branch.objects.get(id).zone, Zone::Graveyard);
            }
            assert_eq!(branch.objects.get(untouched).zone, Zone::Hand);
            assert_eq!(branch.objects.get(spell).zone, Zone::Graveyard);
            assert_eq!(branch.players[player.index()].hand.len(), 2);
            assert_eq!(branch.engine.event_history.iter().filter(|event| matches!(event, mtg_kernel::event::CommittedEvent::Draw { player: actor, .. } if *actor == player)).count(), 2);
            mtg_kernel::event::propose_and_commit(
                branch,
                mtg_kernel::event::ProposedEvent::zone_change(stolen, Zone::Hand),
            );
            assert!(branch.players[player.opponent().index()]
                .hand
                .contains(&stolen));
            assert!(!branch.players[player.index()].hand.contains(&stolen));
        }
        assert_eq!(state.state_hash(), stack_restore.state_hash());
        assert_eq!(state.state_hash(), order_restore.state_hash());
    }
}
