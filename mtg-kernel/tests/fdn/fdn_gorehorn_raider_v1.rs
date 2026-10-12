//! Future printed acceptance requires actual registration, without injected roots.
#![cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
use mtg_kernel::card_def::{card_id_by_name, CardType, Keywords, Subtype, CARD_DEFS};
use mtg_kernel::effect::{self, EffectCond, EffectOp};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

fn put(state: &mut GameState, owner: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id_by_name(name)
        .unwrap_or_else(|| panic!("printed acceptance requires registration: {name}"));
    let id = state.objects.push(GameObject {
        card_def,
        name: name.into(),
        owner,
        controller: owner,
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
        Zone::Hand => state.players[owner.index()].hand.push(id),
        Zone::Battlefield => state.players[owner.index()].battlefield.push(id),
        _ => panic!("fixture zone"),
    }
    id
}
fn ready(actor: PlayerId, size: usize) -> GameState {
    let cards = ["Forest", "Plains", "Island", "Mountain", "Swamp"]
        .map(|name| card_id_by_name(name).unwrap());
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &cards[..size],
        &cards[..size],
        |id| CARD_DEFS[id as usize].name.into(),
        11006,
        actor,
    );
    state.step = Step::Main1;
    for seat in [actor, actor.opponent()] {
        put(&mut state, seat, "Mountain", Zone::Battlefield);
        put(&mut state, seat, "Lightning Bolt", Zone::Hand);
    }
    state
}
fn next(state: &mut GameState) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
    decision
}
fn settle(state: &mut GameState) -> Option<Decision> {
    for _ in 0..40 {
        let decision = next(state);
        match decision {
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
fn raid(state: &mut GameState, actor: PlayerId, nonempty: bool) {
    let attacker = put(state, actor, "Llanowar Elves", Zone::Battlefield);
    state.step = Step::DeclareAttackers;
    assert!(matches!(next(state), Decision::DeclareAttackers { player, .. } if player == actor));
    engine::step(
        state,
        Action::DeclareAttackers(if nonempty { vec![attacker] } else { vec![] }),
    )
    .unwrap();
    for _ in 0..40 {
        let decision = next(state);
        if state.step == Step::Main2
            && matches!(decision, Decision::CastSpellOrPass { player, .. } if player == actor)
        {
            return;
        }
        match decision {
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            Decision::DeclareBlockers { .. } => {
                engine::step(state, Action::DeclareBlockers(vec![])).unwrap()
            }
            other => panic!("unexpected combat decision {other:?}"),
        }
    }
    panic!("Main2 did not retain a castable decision");
}
fn cast(state: &mut GameState, actor: PlayerId) -> ObjectId {
    let source = put(state, actor, "Gorehorn Raider", Zone::Hand);
    state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 1;
    state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 4;
    assert!(
        matches!(next(state), Decision::CastSpellOrPass { player, castable_spells, .. }
        if player == actor && castable_spells.contains(&source))
    );
    engine::step(state, Action::CastSpell(source)).unwrap();
    assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
    assert!(state.stack.iter().any(|item| item.source == source));
    source
}
fn replay(state: &GameState) -> GameState {
    let bytes = serde_json::to_vec(state).unwrap();
    let restored: GameState = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(bytes, serde_json::to_vec(&restored).unwrap());
    assert_eq!(state.state_hash(), restored.state_hash());
    restored
}

fn target_trigger(state: &mut GameState, actor: PlayerId, target: Target) {
    assert!(
        matches!(settle(state), Some(Decision::ChooseTargets { player, legal_targets, .. })
        if player == actor && legal_targets.contains(&target))
    );
    engine::step(state, Action::ChooseTarget(target)).unwrap();
    let _ = next(state);
    assert!(state
        .stack
        .iter()
        .any(|item| item.kind == mtg_kernel::state::StackItemKind::TriggeredAbility));
}
#[test]
fn printed_raid_root_and_precise_meaning_are_exact_and_rewrites_refuse() {
    let id = card_id_by_name("Gorehorn Raider").expect("actual registration required");
    let def = &CARD_DEFS[id as usize];
    assert_eq!(def.types, &[CardType::Creature]);
    assert_eq!(def.subtypes, &[Subtype::Minotaur, Subtype::Pirate]);
    assert_eq!(
        (def.cost.generic, def.mana_value, def.power, def.toughness),
        (4, 5, Some(4), Some(4))
    );
    assert_eq!(def.cost.pips, &[Pip::Colored(ManaColor::R)]);
    assert_eq!(def.keywords, Keywords::NONE);
    assert_eq!(
        (mtg_kernel::trigger::triggers_for(id)[0].effect)(),
        EffectOp::Conditional {
            cond: EffectCond::ControllerAttackedThisTurn,
            then: Box::new(EffectOp::DealDamage {
                target: mtg_kernel::effect::TargetRef::Target(0),
                amount: 2
            }),
            else_: Box::new(EffectOp::Sequence(vec![]))
        }
    );
    let selector = mtg_kernel::rules_meaning_raid_entry_v1::selector_v1().unwrap();
    let meaning = mtg_kernel::rules_meaning_raid_entry_v1::meaning_v1(&selector, id, 0).unwrap();
    assert!(mtg_kernel::rules_meaning_raid_entry_v1::meaning_v1(&selector, id, 1).is_err());
    for mode in 0..4 {
        let mut bad = meaning.clone();
        match mode {
            0 => bad.original_controller = false,
            1 => bad.resolution_recheck = false,
            2 => bad.selector.card_db_hash ^= 1,
            _ => {
                bad.instruction =
                    mtg_kernel::rules_meaning_raid_entry_v1::InstructionV1::DamageAnyTarget {
                        amount: 3,
                        players: true,
                        creatures: true,
                        planeswalkers: true,
                    }
            }
        }
        assert!(mtg_kernel::rules_meaning_raid_entry_v1::validate_v1(&bad).is_err());
    }
}
#[test]
fn real_both_seat_cast_and_raid_target_player_or_creature_restore_damage_two() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for creature in [false, true] {
            let mut state = ready(actor, 5);
            raid(&mut state, actor, true);
            let recipient = put(
                &mut state,
                actor.opponent(),
                "Serra Angel",
                Zone::Battlefield,
            );
            cast(&mut state, actor);
            let target = if creature {
                Target::Object(recipient)
            } else {
                Target::Player(actor.opponent())
            };
            target_trigger(&mut state, actor, target);
            let before = state.players[actor.opponent().index()].life;
            let mut restored = replay(&state);
            assert!(settle(&mut state).is_none());
            assert!(settle(&mut restored).is_none());
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&restored).unwrap()
            );
            if creature {
                assert_eq!(state.objects.get(recipient).damage, 2);
                assert_eq!(state.players[actor.opponent().index()].life, before);
            } else {
                assert_eq!(state.players[actor.opponent().index()].life, before - 2);
                assert_eq!(state.objects.get(recipient).damage, 0);
            }
        }
    }
}
#[test]
fn real_empty_or_missing_attack_has_no_trigger_and_stale_raid_fails_resolution() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for mode in 0..3 {
            let mut state = ready(actor, 5);
            if mode > 0 {
                raid(&mut state, actor, mode == 2);
            }
            cast(&mut state, actor);
            if mode == 2 {
                target_trigger(&mut state, actor, Target::Player(actor.opponent()));
                state.attack_declaration_turn_v1 = None;
            }
            let before = state.players[actor.opponent().index()].life;
            assert!(settle(&mut state).is_none());
            assert_eq!(state.players[actor.opponent().index()].life, before);
        }
    }
}
#[test]
fn captured_source_departure_does_not_cancel_damage_but_target_blink_does() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for blink in [false, true] {
            let mut state = ready(actor, 5);
            raid(&mut state, actor, true);
            let recipient = put(
                &mut state,
                actor.opponent(),
                "Serra Angel",
                Zone::Battlefield,
            );
            let source = cast(&mut state, actor);
            target_trigger(&mut state, actor, Target::Object(recipient));
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(source, Zone::Graveyard),
            );
            if blink {
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(recipient, Zone::Exile),
                );
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(recipient, Zone::Battlefield),
                );
            }
            let mut restored = replay(&state);
            assert!(settle(&mut state).is_none());
            assert!(settle(&mut restored).is_none());
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&restored).unwrap()
            );
            assert_eq!(
                state.objects.get(recipient).damage,
                if blink { 0 } else { 2 }
            );
        }
    }
}
#[test]
fn original_entry_controller_receipt_rejects_coherent_live_trigger_rewrite_readonly() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor, 5);
        raid(&mut state, actor, true);
        cast(&mut state, actor);
        target_trigger(&mut state, actor, Target::Player(actor.opponent()));
        let item = state
            .stack
            .iter_mut()
            .find(|item| item.kind == mtg_kernel::state::StackItemKind::TriggeredAbility)
            .unwrap();
        item.controller = actor.opponent();
        item.v4.ability_source_contract.as_mut().unwrap().controller = actor.opponent();
        let mut restored = replay(&state);
        let before = serde_json::to_vec(&restored).unwrap();
        assert!(effect::validate_pending_effect_choice(&restored).is_err());
        assert!(engine::step(&mut restored, Action::Pass).is_err());
        assert_eq!(before, serde_json::to_vec(&restored).unwrap());
    }
}
