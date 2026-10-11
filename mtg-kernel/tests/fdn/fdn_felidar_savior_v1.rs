//! Source preparation. Card registration and gameplay qualification are pending.
#![cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]

use mtg_kernel::card_def::{card_id_by_name, CardType, Keywords, Subtype, TargetSpec, CARD_DEFS};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::state::{
    GameObject, GameState, ObjectStateV4, StackTargetContractV4, Step, Target, Zone,
};
use mtg_kernel::trigger;

fn ready(player: PlayerId) -> GameState {
    let forest = card_id_by_name("Forest").unwrap();
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &[forest; 40],
        &[forest; 40],
        |_| "Forest".into(),
        838,
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

fn enter(state: &mut GameState, player: PlayerId) -> ObjectId {
    let source = put(state, player, "Felidar Savior", Zone::Hand);
    event::propose_and_commit(state, ProposedEvent::zone_change(source, Zone::Battlefield));
    let pending = trigger::collect_and_process(state);
    assert_eq!(pending.len(), 1);
    state.engine.pending_triggers.extend(pending);
    source
}

fn next(state: &mut GameState) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
    decision
}

fn targets(state: &mut GameState) -> Decision {
    for _ in 0..32 {
        match next(state) {
            choice @ Decision::ChooseTargets { .. } => return choice,
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            Decision::OrderTriggers { pending, .. } => {
                engine::step(state, Action::OrderTriggers((0..pending.len()).collect())).unwrap()
            }
            other => panic!("unexpected decision {other:?}"),
        }
    }
    panic!("Pup target choice absent");
}

fn settle(state: &mut GameState) {
    for _ in 0..64 {
        match next(state) {
            Decision::CastSpellOrPass { .. }
                if state.stack.is_empty() && state.engine.pending_triggers.is_empty() =>
            {
                return
            }
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            Decision::ChooseTargets {
                can_finish: true, ..
            } => engine::step(state, Action::FinishEffectSelection).unwrap(),
            Decision::OrderTriggers { pending, .. } => {
                engine::step(state, Action::OrderTriggers((0..pending.len()).collect())).unwrap()
            }
            other => panic!("unexpected decision {other:?}"),
        }
    }
    panic!("Pup resolution did not settle");
}

fn restored(state: &GameState) -> GameState {
    serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
}

fn refuse(state: &mut GameState, action: Action) {
    let before = serde_json::to_vec(state).unwrap();
    assert!(engine::step(state, action).is_err());
    assert_eq!(serde_json::to_vec(state).unwrap(), before);
}

#[test]
fn exact_metadata_colored_payment_and_lifelink_for_both_seats() {
    let id = card_id_by_name("Felidar Savior").unwrap();
    assert_eq!(id, 366);
    let def = &CARD_DEFS[id as usize];
    assert_eq!(def.types, &[CardType::Creature]);
    assert_eq!(def.subtypes, &[Subtype::Cat, Subtype::Beast]);
    assert_eq!(
        (def.power, def.toughness, def.mana_value),
        (Some(2), Some(3), 4)
    );
    assert_eq!(def.cost.generic, 3);
    assert_eq!(def.cost.pips, &[Pip::Colored(ManaColor::W)]);
    assert!(def.keywords.has(Keywords::LIFELINK));
    assert!(!def.keywords.has(Keywords::FLASH));
    assert_eq!(
        trigger::trigger_target_spec(id),
        TargetSpec::UpToTwoOtherControlledCreatures
    );
    assert_eq!(TargetSpec::UpToTwoOtherControlledCreatures.stable_id(), 59);
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        let source = put(&mut state, player, "Felidar Savior", Zone::Hand);
        state.players[player.index()].mana_pool[5] = 4;
        next(&mut state);
        refuse(&mut state, Action::CastSpell(source));
        state.players[player.index()].mana_pool[5] = 3;
        state.players[player.index()].mana_pool[ManaColor::W.pool_index()] = 1;
        engine::step(&mut state, Action::CastSpell(source)).unwrap();
        assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
        assert_eq!(state.players[player.index()].mana_pool, [0; 6]);
        let mut replay = restored(&state);
        for current in [&mut state, &mut replay] {
            let Decision::ChooseTargets {
                legal_targets,
                can_finish,
                ..
            } = targets(current)
            else {
                unreachable!();
            };
            assert!(legal_targets.is_empty() && can_finish);
            engine::step(current, Action::FinishEffectSelection).unwrap();
            settle(current);
            assert_eq!(current.objects.get(source).zone, Zone::Battlefield);
        }
        assert_eq!(
            state.diagnostic_state_hash(),
            replay.diagnostic_state_hash()
        );
    }
}

#[test]
fn zero_one_or_two_distinct_other_controlled_creatures_and_partial_prefix_restore() {
    for player in [PlayerId::P0, PlayerId::P1] {
        for count in 0..=2 {
            let mut state = ready(player);
            let first = put(&mut state, player, "Llanowar Elves", Zone::Battlefield);
            let second = put(&mut state, player, "Rat Token", Zone::Battlefield);
            let land = put(&mut state, player, "Forest", Zone::Battlefield);
            let foreign = put(
                &mut state,
                player.opponent(),
                "Llanowar Elves",
                Zone::Battlefield,
            );
            let source = enter(&mut state, player);
            let Decision::ChooseTargets {
                legal_targets,
                remaining,
                can_finish,
                ..
            } = targets(&mut state)
            else {
                unreachable!();
            };
            assert_eq!((remaining, can_finish), (2, true));
            assert_eq!(
                legal_targets,
                vec![Target::Object(first), Target::Object(second)]
            );
            for excluded in [source, land, foreign] {
                refuse(&mut state, Action::ChooseTarget(Target::Object(excluded)));
            }
            if count > 0 {
                engine::step(&mut state, Action::ChooseTarget(Target::Object(first))).unwrap();
                let Decision::ChooseTargets {
                    remaining,
                    can_finish,
                    ..
                } = next(&mut state)
                else {
                    unreachable!();
                };
                assert_eq!((remaining, can_finish), (1, true));
                refuse(&mut state, Action::ChooseTarget(Target::Object(first)));
            }
            let mut replay = restored(&state);
            for current in [&mut state, &mut replay] {
                if count == 2 {
                    engine::step(current, Action::ChooseTarget(Target::Object(second))).unwrap();
                    refuse(current, Action::ChooseTarget(Target::Object(first)));
                } else {
                    engine::step(current, Action::FinishEffectSelection).unwrap();
                }
                settle(current);
                assert_eq!(
                    current.objects.get(first).counters.plus1_plus1,
                    i32::from(count > 0)
                );
                assert_eq!(
                    current.objects.get(second).counters.plus1_plus1,
                    i32::from(count == 2)
                );
                assert_eq!(current.objects.get(source).counters.plus1_plus1, 0);
            }
            assert_eq!(
                state.diagnostic_state_hash(),
                replay.diagnostic_state_hash()
            );
        }
    }
}

fn control(state: &mut GameState, object: ObjectId, controller: PlayerId) {
    for player in &mut state.players {
        player.battlefield.retain(|&id| id != object);
    }
    state.players[controller.index()].battlefield.push(object);
    state.objects.get_mut(object).controller = controller;
}

#[test]
fn each_target_rechecks_control_protection_and_incarnation_while_other_target_resolves() {
    use mtg_kernel::effect::{self, EffectOp, ExecCtx, ObjectRef};
    for player in [PlayerId::P0, PlayerId::P1] {
        for change in 0..5 {
            let mut state = ready(player);
            let first = put(&mut state, player, "Llanowar Elves", Zone::Battlefield);
            let second = put(&mut state, player, "Llanowar Elves", Zone::Battlefield);
            let source = enter(&mut state, player);
            targets(&mut state);
            engine::step(&mut state, Action::ChooseTarget(Target::Object(first))).unwrap();
            engine::step(&mut state, Action::ChooseTarget(Target::Object(second))).unwrap();
            assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
            assert_eq!(state.stack.len(), 1);
            event::propose_and_commit(&mut state, ProposedEvent::zone_change(source, Zone::Exile));
            match change {
                0 => {}
                1 => control(&mut state, second, player.opponent()),
                2 => {
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(second, Zone::Exile),
                    );
                }
                3 => {
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(second, Zone::Exile),
                    );
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(second, Zone::Battlefield),
                    );
                }
                4 => {
                    let mut ctx = ExecCtx::no_targets(first, player);
                    ctx.targets = vec![Target::Object(second)];
                    ctx.target_contracts =
                        vec![StackTargetContractV4::capture(&state, ctx.targets[0])];
                    effect::execute(
                        &EffectOp::GrantKeywordTargetUntilEndOfTurn {
                            object: ObjectRef::Target(0),
                            keyword: Keywords::PROTECTION_FROM_MONOCOLORED,
                        },
                        &ctx,
                        &mut state,
                    );
                }
                _ => unreachable!(),
            }
            let mut replay = restored(&state);
            for current in [&mut state, &mut replay] {
                settle(current);
                assert_eq!(current.objects.get(first).counters.plus1_plus1, 1);
                assert_eq!(
                    current.objects.get(second).counters.plus1_plus1,
                    i32::from(change == 0)
                );
                assert_eq!(current.objects.get(source).zone, Zone::Exile);
            }
            assert_eq!(
                state.diagnostic_state_hash(),
                replay.diagnostic_state_hash()
            );
        }
    }
}

#[test]
fn returned_source_is_eligible_for_its_older_trigger_and_forged_original_self_is_rejected() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        let source = enter(&mut state, player);
        targets(&mut state);
        refuse(&mut state, Action::ChooseTarget(Target::Object(source)));
        let mut forged = restored(&state);
        let self_target = Target::Object(source);
        let self_contract = StackTargetContractV4::capture(&forged, self_target);
        forged.engine.pending_triggers[0].targets = vec![self_target];
        forged.engine.pending_triggers[0].target_contracts = vec![self_contract];
        assert!(matches!(
            engine::advance_until_decision(&mut forged),
            Decision::Halted { .. }
        ));
        assert!(forged.stack.is_empty());
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(source, Zone::Exile));
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(source, Zone::Battlefield),
        );
        let new_etb = trigger::collect_and_process(&mut state);
        assert_eq!(new_etb.len(), 1); // Isolate the old trigger from the new ETB.
        let Decision::ChooseTargets { legal_targets, .. } = next(&mut state) else {
            unreachable!();
        };
        assert_eq!(legal_targets, vec![self_target]);
        engine::step(&mut state, Action::ChooseTarget(self_target)).unwrap();
        engine::step(&mut state, Action::FinishEffectSelection).unwrap();
        let mut replay = restored(&state);
        for current in [&mut state, &mut replay] {
            settle(current);
            assert_eq!(current.objects.get(source).counters.plus1_plus1, 1);
        }
        assert_eq!(
            state.diagnostic_state_hash(),
            replay.diagnostic_state_hash()
        );
    }
}

#[test]
fn actual_unblocked_combat_uses_printed_lifelink_for_both_seats() {
    use mtg_kernel::combat_damage_v1::enable_foundations_combat_v1;
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        enable_foundations_combat_v1(&mut state).unwrap();
        let source = put(&mut state, player, "Felidar Savior", Zone::Battlefield);
        state.step = Step::DeclareAttackers;
        engine::step(&mut state, Action::DeclareAttackers(vec![source])).unwrap();
        for _ in 0..32 {
            if state.players[player.opponent().index()].life < 20 {
                break;
            }
            match next(&mut state) {
                Decision::DeclareBlockers { .. } => {
                    engine::step(&mut state, Action::DeclareBlockers(vec![])).unwrap()
                }
                Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
                other => panic!("unexpected combat decision {other:?}"),
            }
        }
        assert_eq!(state.players[player.index()].life, 22);
        assert_eq!(state.players[player.opponent().index()].life, 18);
    }
}
