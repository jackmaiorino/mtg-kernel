//! Prepared v75 Seeker's Folly cases, pending serial admission.
#![cfg(feature = "limited-fdn-fixtures")]

use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardType, Keywords, TargetSpec, CARD_DEFS,
};
use mtg_kernel::effect::{EffectOp, PlayerRef};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

fn ready(player: PlayerId) -> GameState {
    let forest = card_id_by_name("Forest").unwrap();
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &[forest; 40],
        &[forest; 40],
        |_| "Forest".into(),
        750,
        player,
    );
    for player in [PlayerId::P0, PlayerId::P1] {
        for _ in 0..7 {
            state.draw_card(player).unwrap();
        }
    }
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
        _ => panic!("fixture zone"),
    }
    id
}

fn next(state: &mut GameState) -> Decision {
    let d = engine::advance_until_decision(state);
    assert!(!matches!(d, Decision::Halted { .. }), "{d:?}");
    d
}

fn settle(state: &mut GameState) {
    for _ in 0..64 {
        let d = next(state);
        if matches!(d, Decision::CastSpellOrPass { .. }) && state.stack.is_empty() {
            return;
        }
        assert!(matches!(d, Decision::CastSpellOrPass { .. }));
        engine::step(state, Action::Pass).unwrap();
    }
    panic!("spell did not resolve")
}

fn cleanup(state: &mut GameState) {
    assert!(state.stack.is_empty());
    assert!(!state.engine.until_end_of_turn.is_empty());
    let active = state.active_player;
    // Expiry runs when the engine enters Cleanup, not when a fixture assigns
    // that step directly. Pass the real End window to execute the entry action.
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

fn begin(state: &mut GameState, caster: PlayerId, mode: u8) -> ObjectId {
    let source = put(state, caster, "Seeker's Folly", Zone::Hand);
    state.players[caster.index()].mana_pool[5] = 2;
    state.players[caster.index()].mana_pool[ManaColor::B.pool_index()] = 1;
    next(state);
    engine::step(state, Action::CastSpell(source)).unwrap();
    assert!(matches!(
        next(state),
        Decision::ChooseSpellMode { mode_count: 2, .. }
    ));
    engine::step(state, Action::ChooseSpellMode(mode)).unwrap();
    source
}

#[test]
fn folly_has_exact_cost_and_distinct_target_specs_for_both_modes() {
    let id = card_id_by_name("Seeker's Folly").unwrap();
    assert_eq!(id, 349);
    let def = &CARD_DEFS[id as usize];
    assert_eq!(def.capability, CardCapability::Full);
    assert_eq!(def.types, &[CardType::Sorcery]);
    assert_eq!(def.mana_value, 3);
    assert_eq!(def.cost.generic, 2);
    assert_eq!(def.cost.pips, &[Pip::Colored(ManaColor::B)]);
    assert_eq!(def.target_spec, TargetSpec::TargetOpponent);
    assert!(matches!(
        (def.spell_effect)(),
        Some(EffectOp::DiscardCards {
            player: PlayerRef::Target(0),
            count: 2
        })
    ));
    let second = def.mode2.as_ref().unwrap();
    assert_eq!(second.target_spec, TargetSpec::None);
    assert!(matches!(
        (second.effect)(),
        EffectOp::BoostPlayerCreaturesUntilEndOfTurn {
            player: PlayerRef::Opponent,
            power: -1,
            toughness: -1,
            keywords: Keywords::NONE,
        }
    ));
}

#[test]
fn folly_discard_targets_only_opponent_and_restores_their_choice() {
    for caster in [PlayerId::P0, PlayerId::P1] {
        let opponent = caster.opponent();
        let mut state = ready(caster);
        let source = begin(&mut state, caster, 0);
        assert!(
            matches!(next(&mut state), Decision::ChooseTargets { legal_targets, .. }
            if legal_targets == vec![Target::Player(opponent)])
        );
        engine::step(&mut state, Action::ChooseTarget(Target::Player(opponent))).unwrap();
        assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
        let choice = loop {
            let d = next(&mut state);
            if matches!(d, Decision::Discard { .. }) {
                break d;
            }
            assert!(matches!(d, Decision::CastSpellOrPass { .. }));
            engine::step(&mut state, Action::Pass).unwrap();
        };
        let Decision::Discard {
            player,
            count,
            choices,
        } = choice
        else {
            unreachable!()
        };
        assert_eq!(player, opponent);
        assert_eq!(count, 2);
        let selected = vec![choices[0], choices[1]];
        let before = state.state_hash();
        assert!(engine::step(&mut state, Action::Discard(vec![choices[0]])).is_err());
        assert_eq!(state.state_hash(), before);
        let mut replay: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        for branch in [&mut state, &mut replay] {
            engine::step(branch, Action::Discard(selected.clone())).unwrap();
            settle(branch);
            assert_eq!(
                branch.players[opponent.index()].hand.len(),
                choices.len() - 2
            );
            assert_eq!(branch.objects.get(source).zone, Zone::Graveyard);
            for &discarded in &selected {
                assert_eq!(branch.objects.get(discarded).zone, Zone::Graveyard);
            }
        }
        assert_eq!(state.state_hash(), replay.state_hash());
    }
}

#[test]
fn folly_discard_with_zero_or_one_card_silently_discards_everything() {
    for caster in [PlayerId::P0, PlayerId::P1] {
        for remaining in [0, 1] {
            let opponent = caster.opponent();
            let mut state = ready(caster);
            let hand = state.players[opponent.index()].hand.clone();
            for &object in hand.iter().skip(remaining) {
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(object, Zone::Graveyard),
                );
            }
            begin(&mut state, caster, 0);
            assert!(matches!(next(&mut state), Decision::ChooseTargets { .. }));
            engine::step(&mut state, Action::ChooseTarget(Target::Player(opponent))).unwrap();
            settle(&mut state);
            assert!(state.players[opponent.index()].hand.is_empty());
        }
    }
}

#[test]
fn folly_debuff_samples_opposing_creatures_and_expires_at_real_cleanup_with_restore() {
    for caster in [PlayerId::P0, PlayerId::P1] {
        let opponent = caster.opponent();
        let mut state = ready(caster);
        let own = put(
            &mut state,
            caster,
            "Treetop Snarespinner",
            Zone::Battlefield,
        );
        let affected = put(
            &mut state,
            opponent,
            "Treetop Snarespinner",
            Zone::Battlefield,
        );
        let borrowed = put(
            &mut state,
            caster,
            "Treetop Snarespinner",
            Zone::Battlefield,
        );
        state.objects.get_mut(borrowed).controller = opponent;
        state.players[caster.index()]
            .battlefield
            .retain(|&id| id != borrowed);
        state.players[opponent.index()].battlefield.push(borrowed);
        let doomed = put(&mut state, opponent, "Faerie Miscreant", Zone::Battlefield);
        let source = begin(&mut state, caster, 1);
        assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
        assert!(state.engine.pending_cast.is_none());
        let mut replay: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        for branch in [&mut state, &mut replay] {
            settle(branch);
            assert_eq!(branch.objects.get(doomed).zone, Zone::Graveyard);
            assert_eq!(branch.objects.get(source).zone, Zone::Graveyard);
            assert_eq!(engine::effective_power(branch, own), 1);
            assert_eq!(engine::effective_power(branch, affected), 0);
            assert_eq!(engine::effective_toughness(branch, affected), 3);
            assert_eq!(engine::effective_power(branch, borrowed), 0);
            let late = put(branch, opponent, "Treetop Snarespinner", Zone::Battlefield);
            assert_eq!(engine::effective_power(branch, late), 1);
            event::propose_and_commit(branch, ProposedEvent::zone_change(affected, Zone::Hand));
            event::propose_and_commit(
                branch,
                ProposedEvent::zone_change(affected, Zone::Battlefield),
            );
            assert_eq!(engine::effective_power(branch, affected), 1);
            cleanup(branch);
            assert_eq!(engine::effective_power(branch, borrowed), 1);
            assert_eq!(engine::effective_toughness(branch, borrowed), 4);
        }
        assert_eq!(state.state_hash(), replay.state_hash());
    }
}
