//! Source preparation. Registration and card gameplay qualification are pending.
#![cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]

use mtg_kernel::card_def::{
    card_id_by_name, CardType, CostComponent, Keywords, PermanentFilter, Subtype, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Zone};
use mtg_kernel::trigger;

fn ready(player: PlayerId) -> GameState {
    let forest = card_id_by_name("Forest").unwrap();
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &[forest; 40],
        &[forest; 40],
        |_| "Forest".into(),
        840,
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
    panic!("Arbiter resolution did not settle");
}

fn restored(state: &GameState) -> GameState {
    serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
}

fn refuse(state: &mut GameState, action: Action) {
    let before = serde_json::to_vec(state).unwrap();
    assert!(engine::step(state, action).is_err());
    assert_eq!(serde_json::to_vec(state).unwrap(), before);
}

fn mana(state: &mut GameState, player: PlayerId) {
    state.players[player.index()].mana_pool[5] = 4;
    state.players[player.index()].mana_pool[ManaColor::B.pool_index()] = 2;
}

fn enter(state: &mut GameState, player: PlayerId) -> ObjectId {
    let source = put(state, player, "Arbiter of Woe", Zone::Hand);
    event::propose_and_commit(state, ProposedEvent::zone_change(source, Zone::Battlefield));
    let pending = trigger::collect_and_process(state);
    assert_eq!(pending.len(), 1);
    state.engine.pending_triggers.extend(pending);
    source
}

#[test]
fn exact_metadata_and_mandatory_cost_selection_restore_for_both_seats() {
    let id = card_id_by_name("Arbiter of Woe").unwrap();
    assert_eq!(id, 368);
    let def = &CARD_DEFS[id as usize];
    assert_eq!(def.types, &[CardType::Creature]);
    assert_eq!(def.subtypes, &[Subtype::Demon]);
    assert_eq!(
        (def.power, def.toughness, def.mana_value),
        (Some(5), Some(4), 6)
    );
    assert_eq!(def.cost.generic, 4);
    assert_eq!(
        def.cost.pips,
        &[Pip::Colored(ManaColor::B), Pip::Colored(ManaColor::B)]
    );
    assert!(def.keywords.has(Keywords::FLYING));
    assert!(matches!(
        def.additional_cost,
        Some([CostComponent::SacrificeControlled {
            count: 1,
            filter: PermanentFilter::Creature
        }])
    ));
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        let source = put(&mut state, player, "Arbiter of Woe", Zone::Hand);
        mana(&mut state, player);
        next(&mut state);
        refuse(&mut state, Action::CastSpell(source));
        let payment = put(&mut state, player, "Llanowar Elves", Zone::Battlefield);
        let token = put(&mut state, player, "Rat Token", Zone::Battlefield);
        let land = put(&mut state, player, "Forest", Zone::Battlefield);
        let artifact = put(&mut state, player, "Great Furnace", Zone::Battlefield);
        let foreign = put(
            &mut state,
            player.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        state.players[player.index()].mana_pool = [0; 6];
        state.players[player.index()].mana_pool[5] = 6;
        refuse(&mut state, Action::CastSpell(source));
        state.players[player.index()].mana_pool = [0; 6];
        mana(&mut state, player);
        engine::step(&mut state, Action::CastSpell(source)).unwrap();
        let Decision::ChooseCostTargets {
            player: chooser,
            remaining,
            candidates,
            ..
        } = next(&mut state)
        else {
            panic!("creature cost absent")
        };
        assert_eq!((chooser, remaining), (player, 1));
        assert_eq!(candidates, vec![payment, token]);
        for excluded in [source, land, artifact, foreign] {
            refuse(&mut state, Action::ChooseCostTarget(excluded));
        }
        let mut replay = restored(&state);
        for current in [&mut state, &mut replay] {
            engine::step(current, Action::ChooseCostTarget(payment)).unwrap();
            let mut answered = restored(current);
            for continuation in [current, &mut answered] {
                assert!(matches!(
                    next(continuation),
                    Decision::CastSpellOrPass { .. }
                ));
                assert_eq!(continuation.objects.get(payment).zone, Zone::Graveyard);
                assert_eq!(continuation.objects.get(source).zone, Zone::Stack);
                assert_eq!(continuation.players[player.index()].mana_pool, [0; 6]);
                assert_eq!(continuation.stack[0].v4.paid_cost_refs[0].object, payment);
                assert!(settle(continuation).is_none());
                assert_eq!(
                    (
                        continuation.players[player.index()].life,
                        continuation.players[player.opponent().index()].life
                    ),
                    (22, 18)
                );
                assert_eq!(continuation.players[player.index()].library.len(), 39);
            }
        }
        assert_eq!(
            state.diagnostic_state_hash(),
            replay.diagnostic_state_hash()
        );
    }
}

#[test]
fn opponent_discards_exactly_one_then_life_draw_gain_continue_after_source_leaves() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        let chosen = put(&mut state, player.opponent(), "Forest", Zone::Hand);
        let retained = put(&mut state, player.opponent(), "Island", Zone::Hand);
        let source = enter(&mut state, player);
        let Some(Decision::Discard {
            player: chooser,
            count,
            choices,
        }) = settle(&mut state)
        else {
            panic!("discard absent")
        };
        assert_eq!((chooser, count), (player.opponent(), 1));
        assert_eq!(choices, vec![chosen, retained]);
        assert_eq!(
            (
                state.players[player.index()].life,
                state.players[player.opponent().index()].life
            ),
            (20, 20)
        );
        assert_eq!(state.players[player.index()].library.len(), 40);
        assert!(state.engine.pending_effect.is_some());
        for kind in 0..4 {
            let mut forged = restored(&state);
            match kind {
                0 => forged.engine.pending_discard.as_mut().unwrap().count = 2,
                1 => forged
                    .engine
                    .pending_effect
                    .as_mut()
                    .unwrap()
                    .frames
                    .clear(),
                2 => {
                    let mtg_kernel::engine::DiscardResume::FinishEffectContinuation {
                        path, ..
                    } = &mut forged.engine.pending_discard.as_mut().unwrap().resume
                    else {
                        panic!("resumable discard absent")
                    };
                    *path = vec![1];
                }
                3 => {
                    let mtg_kernel::engine::DiscardResume::FinishEffectContinuation {
                        original_hand,
                        ..
                    } = &mut forged.engine.pending_discard.as_mut().unwrap().resume
                    else {
                        panic!("resumable discard absent")
                    };
                    original_hand[0].expected_zone_change_count += 1;
                }
                _ => unreachable!(),
            }
            refuse(&mut forged, Action::Discard(vec![chosen]));
        }
        refuse(&mut state, Action::Discard(vec![]));
        refuse(&mut state, Action::Discard(vec![chosen, retained]));
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(source, Zone::Exile));
        let mut replay = restored(&state);
        for current in [&mut state, &mut replay] {
            engine::step(current, Action::Discard(vec![chosen])).unwrap();
            assert!(settle(current).is_none());
            assert_eq!(current.objects.get(chosen).zone, Zone::Graveyard);
            assert_eq!(
                current.players[player.opponent().index()].hand,
                vec![retained]
            );
            assert_eq!(
                (
                    current.players[player.index()].life,
                    current.players[player.opponent().index()].life
                ),
                (22, 18)
            );
            assert_eq!(current.players[player.index()].library.len(), 39);
            assert_eq!(current.players[player.index()].hand.len(), 1);
            assert_eq!(current.objects.get(source).zone, Zone::Exile);
        }
        assert_eq!(
            state.diagnostic_state_hash(),
            replay.diagnostic_state_hash()
        );
    }
}

#[test]
fn empty_opponent_hand_does_not_skip_remaining_etb_operations() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        enter(&mut state, player);
        assert!(settle(&mut state).is_none());
        assert!(state.players[player.opponent().index()].hand.is_empty());
        assert_eq!(
            (
                state.players[player.index()].life,
                state.players[player.opponent().index()].life
            ),
            (22, 18)
        );
        assert_eq!(state.players[player.index()].library.len(), 39);
        assert_eq!(state.players[player.index()].hand.len(), 1);
    }
}

#[test]
fn token_borrowed_and_death_trigger_creatures_pay_before_spell_resolution() {
    for player in [PlayerId::P0, PlayerId::P1] {
        for kind in 0..3 {
            let mut state = ready(player);
            let source = put(&mut state, player, "Arbiter of Woe", Zone::Hand);
            let payment = match kind {
                0 => put(&mut state, player, "Rat Token", Zone::Battlefield),
                1 => {
                    let borrowed = put(
                        &mut state,
                        player.opponent(),
                        "Llanowar Elves",
                        Zone::Battlefield,
                    );
                    state.players[player.opponent().index()]
                        .battlefield
                        .retain(|&id| id != borrowed);
                    state.players[player.index()].battlefield.push(borrowed);
                    state.objects.get_mut(borrowed).controller = player;
                    borrowed
                }
                2 => put(&mut state, player, "Infestation Sage", Zone::Battlefield),
                _ => unreachable!(),
            };
            mana(&mut state, player);
            let alternate = put(&mut state, player, "Llanowar Elves", Zone::Battlefield);
            next(&mut state);
            engine::step(&mut state, Action::CastSpell(source)).unwrap();
            let Decision::ChooseCostTargets { candidates, .. } = next(&mut state) else {
                panic!("creature cost absent")
            };
            assert_eq!(candidates, vec![payment, alternate]);
            engine::step(&mut state, Action::ChooseCostTarget(payment)).unwrap();
            assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
            assert_eq!(state.objects.get(source).zone, Zone::Stack);
            if kind == 1 {
                assert!(state.players[player.opponent().index()]
                    .graveyard
                    .contains(&payment));
            }
            if kind == 2 {
                assert!(state
                    .stack
                    .iter()
                    .skip(1)
                    .any(|item| item.source == payment));
            }
            assert!(settle(&mut state).is_none());
            assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
            if kind == 0 {
                assert!(!state.players[player.index()].graveyard.contains(&payment));
                assert!(!state.players[player.index()].battlefield.contains(&payment));
            }
            if kind == 2 {
                assert!(state.players[player.index()]
                    .battlefield
                    .iter()
                    .any(|&id| state.objects.get(id).name == "Insect Token"));
            }
        }
    }
}
