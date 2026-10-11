//! Prepared v77 fight cards, pending serial admission.
#![cfg(feature = "limited-fdn-fixtures")]

use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardType, Subtype, TargetSpec, CARD_DEFS,
};
use mtg_kernel::effect::EffectOp;
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
        770,
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

fn next(state: &mut GameState) -> Decision {
    let d = engine::advance_until_decision(state);
    assert!(!matches!(d, Decision::Halted { .. }), "{d:?}");
    d
}

fn pass(state: &mut GameState, d: Decision) {
    match d {
        Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
        Decision::OrderTriggers { pending, .. } => {
            engine::step(state, Action::OrderTriggers((0..pending.len()).collect())).unwrap()
        }
        _ => panic!("unexpected decision {d:?}"),
    }
}

fn settle(state: &mut GameState) {
    for _ in 0..64 {
        let d = next(state);
        if matches!(d, Decision::CastSpellOrPass { .. })
            && state.stack.is_empty()
            && state.engine.pending_triggers.is_empty()
        {
            return;
        }
        pass(state, d);
    }
    panic!("did not settle")
}

fn until(state: &mut GameState, predicate: impl Fn(&Decision) -> bool) -> Decision {
    for _ in 0..32 {
        let d = next(state);
        if predicate(&d) {
            return d;
        }
        assert!(!state.stack.is_empty() || !state.engine.pending_triggers.is_empty());
        pass(state, d);
    }
    panic!("choice missing")
}

fn refuse(state: &mut GameState, action: Action) {
    let before = serde_json::to_vec(state).unwrap();
    assert!(engine::step(state, action).is_err());
    assert_eq!(serde_json::to_vec(state).unwrap(), before);
}

fn restored(state: &GameState) -> GameState {
    let replay: GameState = serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap();
    assert_eq!(state.state_hash(), replay.state_hash());
    replay
}

fn enter_indrik(state: &mut GameState, player: PlayerId) -> ObjectId {
    let source = put(state, player, "Affectionate Indrik", Zone::Hand);
    event::propose_and_commit(state, ProposedEvent::zone_change(source, Zone::Battlefield));
    let pending = trigger::collect_and_process(state);
    state.engine.pending_triggers.extend(pending);
    source
}

fn cast_bushwhack(state: &mut GameState, player: PlayerId, mode: u8) -> ObjectId {
    let source = put(state, player, "Bushwhack", Zone::Hand);
    state.players[player.index()].mana_pool[ManaColor::G.pool_index()] = 1;
    next(state);
    engine::step(state, Action::CastSpell(source)).unwrap();
    assert!(matches!(
        next(state),
        Decision::ChooseSpellMode { mode_count: 2, .. }
    ));
    engine::step(state, Action::ChooseSpellMode(mode)).unwrap();
    source
}

fn choose_pair(state: &mut GameState, first: ObjectId, second: ObjectId) {
    assert!(matches!(
        next(state),
        Decision::ChooseTargets { remaining: 2, .. }
    ));
    refuse(state, Action::ChooseTarget(Target::Object(second)));
    engine::step(state, Action::ChooseTarget(Target::Object(first))).unwrap();
    assert!(matches!(
        next(state),
        Decision::ChooseTargets { remaining: 1, .. }
    ));
    refuse(state, Action::ChooseTarget(Target::Object(first)));
    engine::step(state, Action::ChooseTarget(Target::Object(second))).unwrap();
    assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
    assert_eq!(
        state.stack.len(),
        1,
        "finalized spell before restore or changes"
    );
}

#[test]
fn fight_cards_have_exact_printed_metadata_modes_and_costs() {
    let indrik = card_id_by_name("Affectionate Indrik").unwrap();
    let bushwhack = card_id_by_name("Bushwhack").unwrap();
    assert_eq!((indrik, bushwhack), (351, 352));
    let creature = &CARD_DEFS[indrik as usize];
    assert_eq!(creature.capability, CardCapability::Full);
    assert_eq!(creature.types, &[CardType::Creature]);
    assert_eq!(creature.subtypes, &[Subtype::Beast]);
    assert_eq!(
        (creature.power, creature.toughness, creature.mana_value),
        (Some(4), Some(4), 6)
    );
    assert_eq!(creature.cost.generic, 5);
    assert_eq!(creature.cost.pips, &[Pip::Colored(ManaColor::G)]);
    assert_eq!(
        trigger::trigger_target_spec(indrik),
        TargetSpec::OpponentControlledCreature
    );
    let spell = &CARD_DEFS[bushwhack as usize];
    assert_eq!(spell.capability, CardCapability::Full);
    assert_eq!(spell.types, &[CardType::Sorcery]);
    assert_eq!(spell.cost.generic, 0);
    assert_eq!(spell.cost.pips, &[Pip::Colored(ManaColor::G)]);
    assert_eq!(spell.target_spec, TargetSpec::None);
    assert!(spell.mode3.is_none());
    let second = spell.mode2.as_ref().expect("Bushwhack's fight mode");
    assert_eq!(
        second.target_spec,
        TargetSpec::ControlledCreatureThenOpponentCreature
    );
    assert!(matches!((second.effect)(), EffectOp::FightObjects { .. }));
    let mut state = ready(PlayerId::P0);
    let source = put(&mut state, PlayerId::P0, "Affectionate Indrik", Zone::Hand);
    state.players[0].mana_pool[5] = 6;
    next(&mut state);
    refuse(&mut state, Action::CastSpell(source));
}

#[test]
fn indrik_targets_opposing_creature_then_optionally_fights_with_restored_choice() {
    for player in [PlayerId::P0, PlayerId::P1] {
        for accept in [false, true] {
            let mut state = ready(player);
            let target = put(
                &mut state,
                player.opponent(),
                "Faerie Miscreant",
                Zone::Battlefield,
            );
            let own = put(&mut state, player, "Faerie Miscreant", Zone::Battlefield);
            let source = enter_indrik(&mut state, player);
            let d = until(&mut state, |d| matches!(d, Decision::ChooseTargets { .. }));
            assert!(
                matches!(&d, Decision::ChooseTargets { legal_targets, remaining: 1, can_finish: false, .. }
                if legal_targets == &vec![Target::Object(target)])
            );
            refuse(&mut state, Action::ChooseTarget(Target::Object(own)));
            let mut replay = restored(&state);
            for branch in [&mut state, &mut replay] {
                assert_eq!(next(branch), d);
                engine::step(branch, Action::ChooseTarget(Target::Object(target))).unwrap();
                let choice = until(branch, |d| matches!(d, Decision::ChooseEffectOption { .. }));
                assert!(
                    matches!(choice, Decision::ChooseEffectOption { player: chooser, option_count: 2, .. } if chooser == player)
                );
                let mut pending = restored(branch);
                engine::step(branch, Action::ChooseEffectOption(u16::from(accept))).unwrap();
                engine::step(&mut pending, Action::ChooseEffectOption(u16::from(accept))).unwrap();
                let mut answered = restored(branch);
                settle(branch);
                settle(&mut pending);
                settle(&mut answered);
                assert_eq!(branch.state_hash(), pending.state_hash());
                assert_eq!(branch.state_hash(), answered.state_hash());
                assert_eq!(
                    branch.objects.get(target).zone,
                    if accept {
                        Zone::Graveyard
                    } else {
                        Zone::Battlefield
                    }
                );
                assert_eq!(
                    branch.objects.get(source).damage,
                    if accept { 1 } else { 0 }
                );
                assert_eq!(branch.objects.get(source).zone, Zone::Battlefield);
            }
            assert_eq!(state.state_hash(), replay.state_hash());
        }
    }
}

#[test]
fn indrik_does_not_fight_using_a_departed_source_last_known_power() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        let target = put(
            &mut state,
            player.opponent(),
            "Treetop Snarespinner",
            Zone::Battlefield,
        );
        let source = enter_indrik(&mut state, player);
        until(&mut state, |d| matches!(d, Decision::ChooseTargets { .. }));
        engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
        assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
        assert_eq!(state.stack.len(), 1);
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(source, Zone::Hand));
        until(&mut state, |d| {
            matches!(d, Decision::ChooseEffectOption { .. })
        });
        engine::step(&mut state, Action::ChooseEffectOption(1)).unwrap();
        settle(&mut state);
        assert_eq!(state.objects.get(source).zone, Zone::Hand);
        assert_eq!(state.objects.get(target).damage, 0);
    }
}

#[test]
fn indrik_with_no_legal_targets_places_no_fight_choice() {
    let mut state = ready(PlayerId::P0);
    enter_indrik(&mut state, PlayerId::P0);
    settle(&mut state);
    assert!(state.engine.pending_effect.is_none());
}

#[test]
fn bushwhack_search_can_fail_or_reveal_a_basic_land_with_pending_restore() {
    for player in [PlayerId::P0, PlayerId::P1] {
        for accept in [false, true] {
            let mut state = ready(player);
            // Keep both modes viable so this case exercises the explicit choice.
            put(&mut state, player, "Faerie Miscreant", Zone::Battlefield);
            put(
                &mut state,
                player.opponent(),
                "Faerie Miscreant",
                Zone::Battlefield,
            );
            let before = state.players[player.index()].library.len();
            let source = cast_bushwhack(&mut state, player, 0);
            let d = until(&mut state, |d| {
                matches!(d, Decision::ChooseEffectTargets { .. })
            });
            let Decision::ChooseEffectTargets {
                ref legal_targets,
                min_targets: 0,
                max_targets: 1,
                can_finish: true,
                ..
            } = d
            else {
                panic!("search decision {d:?}");
            };
            let Target::Object(land) = legal_targets[0] else {
                unreachable!()
            };
            let mut replay = restored(&state);
            for branch in [&mut state, &mut replay] {
                assert_eq!(next(branch), d);
                if accept {
                    engine::step(branch, Action::ChooseEffectTarget(Target::Object(land))).unwrap();
                } else {
                    engine::step(branch, Action::FinishEffectSelection).unwrap();
                }
                let mut answered = restored(branch);
                settle(branch);
                settle(&mut answered);
                assert_eq!(branch.state_hash(), answered.state_hash());
                assert_eq!(
                    branch.players[player.index()].library.len(),
                    before - usize::from(accept)
                );
                assert_eq!(
                    branch.objects.get(land).zone,
                    if accept { Zone::Hand } else { Zone::Library }
                );
                if accept {
                    assert!(branch
                        .known_hand_cards(player.opponent(), player)
                        .iter()
                        .any(|entry| entry.object == land));
                }
                assert_eq!(branch.objects.get(source).zone, Zone::Graveyard);
            }
            assert_eq!(state.state_hash(), replay.state_hash());
        }
    }
}

#[test]
fn bushwhack_fight_samples_current_powers_and_deals_reciprocal_damage() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        let first = put(&mut state, player, "Rune-Sealed Wall", Zone::Battlefield);
        let second = put(
            &mut state,
            player.opponent(),
            "Rune-Sealed Wall",
            Zone::Battlefield,
        );
        state.objects.get_mut(first).counters.plus1_plus1 = 1;
        state.objects.get_mut(second).counters.plus1_plus1 = 1;
        let source = cast_bushwhack(&mut state, player, 1);
        choose_pair(&mut state, first, second);
        // Power changes after casting and before resolution are sampled now.
        state.objects.get_mut(first).counters.plus1_plus1 = 2;
        let mut replay = restored(&state);
        settle(&mut state);
        settle(&mut replay);
        assert_eq!(state.objects.get(first).damage, 1);
        assert_eq!(state.objects.get(second).damage, 2);
        assert_eq!(state.objects.get(first).zone, Zone::Battlefield);
        assert_eq!(state.objects.get(second).zone, Zone::Battlefield);
        assert_eq!(state.objects.get(source).zone, Zone::Graveyard);
        assert_eq!(state.state_hash(), replay.state_hash());
    }
}

#[test]
fn bushwhack_refuses_to_fight_when_a_target_changes_control_or_incarnation() {
    for player in [PlayerId::P0, PlayerId::P1] {
        for stale in [false, true] {
            let mut state = ready(player);
            let first = put(
                &mut state,
                player,
                "Treetop Snarespinner",
                Zone::Battlefield,
            );
            let second = put(
                &mut state,
                player.opponent(),
                "Treetop Snarespinner",
                Zone::Battlefield,
            );
            cast_bushwhack(&mut state, player, 1);
            choose_pair(&mut state, first, second);
            if stale {
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(second, Zone::Hand),
                );
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(second, Zone::Battlefield),
                );
            } else {
                state.objects.get_mut(first).controller = player.opponent();
                state.players[player.index()]
                    .battlefield
                    .retain(|id| *id != first);
                state.players[player.opponent().index()]
                    .battlefield
                    .push(first);
            }
            let mut replay = restored(&state);
            settle(&mut state);
            settle(&mut replay);
            assert_eq!(state.objects.get(first).damage, 0);
            assert_eq!(state.objects.get(second).damage, 0);
            assert_eq!(state.state_hash(), replay.state_hash());
        }
    }
}
