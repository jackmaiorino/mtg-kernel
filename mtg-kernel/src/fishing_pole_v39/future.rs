//! Real printed fixtures require future actual Fishing Pole/Fish registration.
use super::*;
use crate::effect::EffectOp;
use crate::event::{self, CommittedEvent, ProposedEvent};
use crate::fishing_pole_v39::lifecycle::{self, JournalV39};
use crate::mana::ManaColor;
use crate::state::Step;

fn ready(actor: PlayerId) -> GameState {
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &[forest; 40],
        &[forest; 40],
        |_| "Forest".into(),
        3939,
        actor,
    );
    state.step = Step::Main1;
    for owner in [actor, actor.opponent()] {
        put(&mut state, owner, "Forest", Zone::Battlefield);
    }
    put(&mut state, actor, "Forest", Zone::Library);
    state
}
fn finish(state: &mut GameState) {
    for _ in 0..40 {
        let decision = next(state);
        if state.stack.is_empty()
            && state.engine.pending_activation.is_none()
            && state.engine.pending_triggers.is_empty()
            && matches!(decision, Decision::CastSpellOrPass { .. })
        {
            return;
        }
        match decision {
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            Decision::OrderTriggers { pending, .. } => {
                engine::step(state, Action::OrderTriggers((0..pending.len()).collect())).unwrap()
            }
            other => panic!("actual Fishing traversal {other:?}"),
        }
    }
    panic!("Fishing resolution failed to finish");
}
fn fixture(actor: PlayerId) -> (GameState, ObjectId, ObjectId) {
    let mut state = ready(actor);
    let host = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
    let pole = put(&mut state, actor, "Fishing Pole", Zone::Hand);
    state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 1;
    engine::step(&mut state, Action::CastSpell(pole)).unwrap();
    finish(&mut state);
    assert_eq!(state.objects.get(pole).zone, Zone::Battlefield);
    equip(&mut state, actor, pole, host);
    (state, host, pole)
}
fn equip(state: &mut GameState, actor: PlayerId, pole: ObjectId, host: ObjectId) {
    assert_eq!(state.step, Step::Main1);
    assert_eq!(state.priority_player, actor);
    state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 2;
    engine::step(state, Action::ActivateAbility(pole, 0)).unwrap();
    assert!(matches!(next(state), Decision::ChooseTargets { .. }));
    engine::step(state, Action::ChooseTarget(Target::Object(host))).unwrap();
    finish(state);
    assert_eq!(state.objects.get(pole).v4.attached_to.unwrap().object, host);
}
fn add(
    state: &mut GameState,
    actor: PlayerId,
    host: ObjectId,
    pole: ObjectId,
) -> crate::state::StackItem {
    state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 1;
    let mana_before: u16 = state.players[actor.index()]
        .mana_pool
        .iter()
        .copied()
        .map(u16::from)
        .sum();
    let index = u8::try_from(
        CARD_DEFS[state.objects.get(host).card_def as usize]
            .activated_abilities
            .len()
            + usize::from(
                engine::attached_equipment_profiles(state, host).any(|(id, p)| {
                    !selected_definition(state.objects.get(id).card_def)
                        && p.granted_activated_ability.is_some()
                }),
            ),
    )
    .unwrap();
    assert!(
        matches!(next(state),Decision::CastSpellOrPass {activatable_abilities,..} if activatable_abilities.contains(&(host,index)))
    );
    engine::step(state, Action::ActivateAbility(host, index)).unwrap();
    assert!(
        matches!(next(state),Decision::ChooseCostTargets {player,candidates,..} if player==actor && candidates.contains(&pole))
    );
    engine::step(state, Action::ChooseCostTarget(pole)).unwrap();
    next(state);
    assert!(state.engine.pending_activation.is_none());
    assert_eq!(
        state.players[actor.index()]
            .mana_pool
            .iter()
            .copied()
            .map(u16::from)
            .sum::<u16>(),
        mana_before - 1
    );
    assert!(state.objects.get(host).tapped && state.objects.get(pole).tapped);
    let item = state.stack.last().unwrap().clone();
    assert_eq!(item.v4.granted_by.unwrap().source, pole);
    assert_eq!(item.v4.paid_cost_refs[0].object, pole);
    item
}
fn fish(state: &GameState, actor: PlayerId) -> usize {
    let fish = lifecycle::fish_definition().unwrap();
    state
        .objects
        .iter()
        .filter(|(_, o)| {
            o.zone == Zone::Battlefield
                && o.controller == actor
                && o.v4.is_token
                && o.card_def == fish
        })
        .count()
}
fn snapshot(state: &GameState) -> GameState {
    serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
}
fn refuses(state: &mut GameState) {
    let before = serde_json::to_vec(state).unwrap();
    assert!(engine::step(state, Action::Pass).is_err());
    assert_eq!(serde_json::to_vec(state).unwrap(), before);
}
#[test]
#[ignore = "requires actual future Fishing Pole and Fish Token registration"]
fn printed_cast_equip_named_cost_first_and_zero_bait_real_untap_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, host, pole) = fixture(actor);
        let def = &CARD_DEFS[state.objects.get(pole).card_def as usize];
        assert_eq!(def.cost.generic, 1);
        assert_eq!(def.types, &[crate::card_def::CardType::Artifact]);
        assert_eq!(def.subtypes, &[crate::card_def::Subtype::Equipment]);
        assert_eq!(
            def.activated_abilities[0].cost,
            [crate::card_def::CostComponent::Mana(crate::mana::Cost {
                generic: 2,
                pips: &[],
                x_count: 0
            })]
        );
        engine::step(&mut state, Action::ActivateManaAbility(host)).unwrap();
        assert!(state.objects.get(host).tapped);
        assert!(engine::attempt_untap(&mut state, host));
        let mut restored = snapshot(&state);
        for branch in [&mut state, &mut restored] {
            finish(branch);
            assert_eq!(fish(branch, actor), 0);
            assert_eq!(branch.objects.get(pole).counters.bait, 0);
        }
        assert_eq!(state, restored);
        add(&mut state, actor, host, pole);
        finish(&mut state);
        assert_eq!(state.objects.get(pole).counters.bait, 1);
        assert!(engine::attempt_untap(&mut state, host));
        let mut restored = snapshot(&state);
        for branch in [&mut state, &mut restored] {
            finish(branch);
            assert_eq!(fish(branch, actor), 1);
            assert_eq!(branch.objects.get(pole).counters.bait, 0);
        }
        assert_eq!(state, restored);
        assert!(!engine::attempt_untap(&mut state, host));
        finish(&mut state);
        assert_eq!(fish(&state, actor), 1);
    }
}
#[test]
#[ignore = "requires actual future Fishing Pole and Fish Token registration"]
fn multiple_bait_remove_exactly_one_and_explicit_effect_untap_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, host, pole) = fixture(actor);
        add(&mut state, actor, host, pole);
        finish(&mut state);
        let source = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        // Accepted UntapObject physical primitive, with a real registered
        // source and target contract. This is not a claim that Llanowar
        // Elves has a printed ability to untap Equipment.
        let ctx = crate::effect::ExecCtx {
            source,
            controller: actor,
            targets: vec![Target::Object(pole)],
            stack_item_id: None,
            target_contracts: vec![crate::state::StackTargetContractV4::capture(
                &state,
                Target::Object(pole),
            )],
            discarded: vec![],
            paid_cost_refs: vec![],
            hidden_ability_source: None,
            ability_source_contract: None,
            cauldron_grant: Default::default(),
            kicked: false,
            optional_additional_cost_paid: None,
            x_value: 0,
        };
        crate::effect::execute(
            &mut state,
            &EffectOp::UntapObject {
                target: crate::effect::TargetRef::Target(0),
            },
            &ctx,
        );
        assert!(!state.objects.get(pole).tapped);
        // Untap host while the first bait activation remains below its own
        // mandatory trigger: the next bait ability can resolve above it.
        assert!(engine::attempt_untap(&mut state, host));
        next(&mut state);
        add(&mut state, actor, host, pole);
        // Resolve just the added ability, retaining the first untap trigger.
        while state
            .stack
            .last()
            .is_some_and(|i| i.inline_effect == Some(EffectOp::FishingPoleAddBaitV39))
        {
            engine::step(&mut state, Action::Pass).unwrap();
            next(&mut state);
        }
        assert_eq!(state.objects.get(pole).counters.bait, 2);
        let mut restored = snapshot(&state);
        for branch in [&mut state, &mut restored] {
            finish(branch);
            assert_eq!(branch.objects.get(pole).counters.bait, 1);
            assert_eq!(fish(branch, actor), 1);
        }
        assert_eq!(state, restored);
    }
}
#[test]
#[ignore = "requires actual future Fishing Pole and Fish Token registration"]
fn multiple_poles_foreign_named_cost_short_mana_sickness_and_longbow_remain_distinct_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, host, pole) = fixture(actor);
        let other = put(&mut state, actor, "Fishing Pole", Zone::Battlefield);
        equip(&mut state, actor, other, host);
        let bow = put(&mut state, actor, "Viridian Longbow", Zone::Battlefield);
        state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 3;
        engine::step(&mut state, Action::ActivateAbility(bow, 0)).unwrap();
        next(&mut state);
        engine::step(&mut state, Action::ChooseTarget(Target::Object(host))).unwrap();
        finish(&mut state);
        let native = u8::try_from(
            CARD_DEFS[state.objects.get(host).card_def as usize]
                .activated_abilities
                .len(),
        )
        .unwrap();
        assert!(
            matches!(next(&mut state),Decision::CastSpellOrPass {activatable_abilities,..} if activatable_abilities.contains(&(host,native)))
        );
        state.objects.get_mut(host).summoning_sick = true;
        let before = serde_json::to_vec(&state).unwrap();
        assert!(engine::step(&mut state, Action::ActivateAbility(host, native + 1)).is_err());
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        state.objects.get_mut(host).summoning_sick = false;
        crate::standard_cards_v1::gain_control(&mut state, other, actor.opponent());
        assert_eq!(cost_candidates(&state, actor, host), [pole]);
        state.players[actor.index()].mana_pool = [0; 6];
        // Host is the only potential mana producer, and is reserved for its
        // tap-symbol cost; neither it nor the named Pole may pay the generic.
        for owner in [actor, actor.opponent()] {
            for &id in &state.players[owner.index()].battlefield.clone() {
                if CARD_DEFS[state.objects.get(id).card_def as usize].is_land {
                    state.objects.get_mut(id).tapped = true;
                }
            }
        }
        let before = serde_json::to_vec(&state).unwrap();
        assert!(engine::step(&mut state, Action::ActivateAbility(host, native + 1)).is_err());
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 1;
        engine::step(&mut state, Action::ActivateAbility(host, native + 1)).unwrap();
        assert!(
            matches!(next(&mut state),Decision::ChooseCostTargets {candidates,..} if candidates==[pole])
        );
        let before = serde_json::to_vec(&state).unwrap();
        assert!(engine::step(&mut state, Action::ChooseCostTarget(other)).is_err());
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        engine::step(&mut state, Action::ChooseCostTarget(pole)).unwrap();
        next(&mut state);
        finish(&mut state);
        assert_eq!(state.objects.get(pole).counters.bait, 1);
        assert_eq!(state.objects.get(other).counters.bait, 0);
    }
}
#[test]
#[ignore = "requires actual future Fishing Pole and Fish Token registration"]
fn source_and_host_departure_control_change_and_exact_counter_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, host, pole) = fixture(actor);
        add(&mut state, actor, host, pole);
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(host, Zone::Hand));
        crate::standard_cards_v1::gain_control(&mut state, pole, actor.opponent());
        let mut restored = snapshot(&state);
        for branch in [&mut state, &mut restored] {
            finish(branch);
            assert_eq!(branch.objects.get(pole).counters.bait, 1);
        }
        assert_eq!(state, restored);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(host, Zone::Battlefield),
        );
        state.objects.get_mut(host).summoning_sick = false;
        crate::standard_cards_v1::gain_control(&mut state, pole, actor);
        equip(&mut state, actor, pole, host);
        state.objects.get_mut(host).tapped = true;
        assert!(engine::attempt_untap(&mut state, host));
        next(&mut state);
        let item = state.stack.last().unwrap().clone();
        crate::standard_cards_v1::gain_control(&mut state, pole, actor.opponent());
        engine::counter_stack_item_by_id(&mut state, item.v4.stack_item_id).unwrap();
        lifecycle::validate_state(&state).unwrap();
        assert_eq!(state.objects.get(pole).counters.bait, 1);
        assert_eq!(fish(&state, actor), 0);
        let mut replay = snapshot(&state);
        replay.stack.push(item);
        refuses(&mut replay);
    }
}
#[test]
#[ignore = "requires actual future Fishing Pole and Fish Token registration"]
fn original_pole_departure_reentry_does_not_receive_bait_or_fish_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, host, pole) = fixture(actor);
        add(&mut state, actor, host, pole);
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(pole, Zone::Hand));
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(pole, Zone::Battlefield),
        );
        let mut restored = snapshot(&state);
        for branch in [&mut state, &mut restored] {
            finish(branch);
            assert_eq!(branch.objects.get(pole).counters.bait, 0);
            assert_eq!(fish(branch, actor), 0);
        }
        assert_eq!(state, restored);
    }
}
#[test]
#[ignore = "requires actual future Fishing Pole and Fish Token registration"]
fn untap_uncollected_pending_live_beneficiary_counter_balance_and_completed_replay_refuse_readonly_both_seats(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, host, pole) = fixture(actor);
        add(&mut state, actor, host, pole);
        finish(&mut state);
        assert!(engine::attempt_untap(&mut state, host));
        let original = state.engine.event_history.clone();
        let mut erased = state.clone();
        erased
            .engine
            .event_log
            .retain(|e| !matches!(e, CommittedEvent::FishingPoleJournalV39 { .. }));
        refuses(&mut erased);
        assert_eq!(erased.engine.event_history, original);
        next(&mut state);
        let item = state.stack.last().unwrap().clone();
        let mut erased = state.clone();
        erased.stack.pop();
        refuses(&mut erased);
        let mut rewritten = snapshot(&state);
        rewritten.stack.last_mut().unwrap().controller = actor.opponent();
        refuses(&mut rewritten);
        let mut balance = state.clone();
        balance.objects.get_mut(pole).counters.bait = 0;
        refuses(&mut balance);
        let mut restored = snapshot(&state);
        for branch in [&mut state, &mut restored] {
            finish(branch);
            assert_eq!(fish(branch, actor), 1);
        }
        assert_eq!(state, restored);
        let mut replay = snapshot(&state);
        replay.stack.push(item);
        refuses(&mut replay);
        let mut log = snapshot(&state);
        let capture=log.engine.event_history.iter().find(|e|matches!(e,CommittedEvent::FishingPoleJournalV39 {journal} if matches!(journal.as_ref(),JournalV39::Captured {..}))).unwrap().clone();
        log.engine.event_log.push(capture);
        refuses(&mut log);
    }
}

#[test]
#[ignore = "requires actual future Fishing Pole and Fish Token registration"]
fn genuine_natural_untap_and_stun_replacement_mandatory_trigger_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, host, pole) = fixture(actor);
        add(&mut state, actor, host, pole);
        finish(&mut state);
        state.objects.get_mut(host).counters.stun = 1;
        let captures = |s: &GameState| {
            s.engine.event_history.iter().filter(|e|matches!(e,CommittedEvent::FishingPoleJournalV39 {journal} if matches!(journal.as_ref(),JournalV39::Captured {..}))).count()
        };
        let before = captures(&state);
        assert!(!engine::attempt_untap(&mut state, host));
        assert!(state.objects.get(host).tapped);
        assert_eq!(captures(&state), before);
        for _ in 0..150 {
            let decision = next(&mut state);
            if fish(&state, actor) == 1 {
                break;
            }
            match decision {
                Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
                Decision::DeclareAttackers { .. } => {
                    engine::step(&mut state, Action::DeclareAttackers(vec![])).unwrap()
                }
                Decision::OrderTriggers { pending, .. } => engine::step(
                    &mut state,
                    Action::OrderTriggers((0..pending.len()).collect()),
                )
                .unwrap(),
                other => panic!("actual natural untap {other:?}"),
            }
        }
        assert_eq!(fish(&state, actor), 1);
        assert_eq!(state.objects.get(pole).counters.bait, 0);
        assert_eq!(captures(&state), before + 1);
        assert!(!state.objects.get(host).tapped);
        lifecycle::validate_state(&snapshot(&state)).unwrap();
    }
}

#[test]
#[ignore = "requires actual future Fishing Pole and Fish Token registration"]
fn actual_registered_quirion_return_forest_cost_untaps_equipped_host_and_original_owner_gets_fish_both_seats(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, host, pole) = fixture(actor);
        add(&mut state, actor, host, pole);
        finish(&mut state);
        let ranger = put(&mut state, actor, "Quirion Ranger", Zone::Battlefield);
        let forest = state.players[actor.index()]
            .battlefield
            .iter()
            .copied()
            .find(|id| CARD_DEFS[state.objects.get(*id).card_def as usize].name == "Forest")
            .unwrap();
        engine::step(&mut state, Action::ActivateAbility(ranger, 0)).unwrap();
        // The current engine stages the target and named physical Forest
        // cost independently; service only actual offered stages.
        for _ in 0..4 {
            match next(&mut state) {
                Decision::ChooseTargets { .. } => {
                    engine::step(&mut state, Action::ChooseTarget(Target::Object(host))).unwrap()
                }
                Decision::ChooseCostTargets { candidates, .. } => {
                    assert!(candidates.contains(&forest));
                    engine::step(&mut state, Action::ChooseCostTarget(forest)).unwrap();
                }
                Decision::CastSpellOrPass { .. } => break,
                other => panic!("actual Ranger activation {other:?}"),
            }
        }
        assert!(state.engine.pending_activation.is_none());
        assert_eq!(state.objects.get(forest).zone, Zone::Hand);
        assert!(state.objects.get(host).tapped);
        let mut restored = snapshot(&state);
        for branch in [&mut state, &mut restored] {
            finish(branch);
            assert!(!branch.objects.get(host).tapped);
            assert_eq!(fish(branch, actor), 1);
            assert_eq!(branch.objects.get(pole).counters.bait, 0);
        }
        assert_eq!(state, restored);
    }
}

#[test]
#[ignore = "requires actual future Fishing Pole and Fish Token registration"]
fn original_untap_controller_survives_host_departure_and_pole_control_change_with_coherent_capture_forgery_both_seats(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, host, pole) = fixture(actor);
        add(&mut state, actor, host, pole);
        finish(&mut state);
        assert!(engine::attempt_untap(&mut state, host));
        next(&mut state);
        crate::standard_cards_v1::gain_control(&mut state, pole, actor.opponent());
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(host, Zone::Hand));
        let mut forged = snapshot(&state);
        let origin = forged
            .engine
            .event_history
            .iter()
            .enumerate()
            .find_map(|(i, e)| match e {
                CommittedEvent::FishingPoleJournalV39 { journal }
                    if matches!(journal.as_ref(), JournalV39::Captured { .. }) =>
                {
                    Some(i as u64)
                }
                _ => None,
            })
            .unwrap();
        // Rewrite capture, full publication and live source/controller while
        // retaining every independent original untap/observer journal.
        for e in &mut forged.engine.event_history {
            if let CommittedEvent::FishingPoleJournalV39 { journal } = e {
                match journal.as_mut() {
                    JournalV39::Captured { pole: captured, .. } => {
                        captured.controller = actor.opponent()
                    }
                    JournalV39::Published {
                        origin: captured,
                        item,
                    } if *captured == origin => {
                        item.controller = actor.opponent();
                        item.v4.ability_source_contract.as_mut().unwrap().controller =
                            actor.opponent();
                    }
                    _ => {}
                }
            }
        }
        let item = forged.stack.last_mut().unwrap();
        item.controller = actor.opponent();
        item.v4.ability_source_contract.as_mut().unwrap().controller = actor.opponent();
        refuses(&mut forged);
        let mut restored = snapshot(&state);
        for branch in [&mut state, &mut restored] {
            finish(branch);
            assert_eq!(fish(branch, actor), 1);
            assert_eq!(fish(branch, actor.opponent()), 0);
            assert_eq!(branch.objects.get(pole).counters.bait, 0);
        }
        assert_eq!(state, restored);
    }
}

#[test]
#[ignore = "requires actual future Fishing Pole and Fish Token registration"]
fn actual_witness_suppresses_granted_bait_but_not_equipment_own_untap_trigger_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, host, pole) = fixture(actor);
        add(&mut state, actor, host, pole);
        finish(&mut state);
        let witness = put(&mut state, actor, "Witness Protection", Zone::Hand);
        state.players[actor.index()].mana_pool[ManaColor::G.pool_index()] = 1;
        engine::step(&mut state, Action::CastSpell(witness)).unwrap();
        assert!(matches!(next(&mut state), Decision::ChooseTargets { .. }));
        engine::step(&mut state, Action::ChooseTarget(Target::Object(host))).unwrap();
        finish(&mut state);
        assert!(grants(&state, host).is_empty());
        assert!(cost_candidates(&state, actor, host).is_empty());
        assert!(state.objects.get(host).tapped);
        assert!(engine::attempt_untap(&mut state, host));
        let mut restored = snapshot(&state);
        for branch in [&mut state, &mut restored] {
            finish(branch);
            assert_eq!(fish(branch, actor), 1);
            assert_eq!(branch.objects.get(pole).counters.bait, 0);
        }
        assert_eq!(state, restored);
    }
}

#[test]
#[ignore = "requires actual future Fishing Pole and Fish Token registration"]
fn manufactured_counter_operation_completion_requires_real_original_removal_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, host, pole) = fixture(actor);
        let item = add(&mut state, actor, host, pole);
        let origin = state
            .engine
            .event_history
            .iter()
            .find_map(|e| match e {
                CommittedEvent::FishingPoleJournalV39 { journal } => match journal.as_ref() {
                    JournalV39::Published {
                        origin,
                        item: original,
                    } if original.as_ref() == &item => Some(*origin),
                    _ => None,
                },
                _ => None,
            })
            .unwrap();
        let original = state.engine.event_history.clone();
        let mut manufactured = snapshot(&state);
        manufactured.stack.pop();
        let removal = manufactured.engine.event_history.len() as u64;
        manufactured
            .engine
            .event_history
            .push(CommittedEvent::FishingPoleJournalV39 {
                journal: Box::new(JournalV39::Removed {
                    input: removal,
                    origin,
                    item: Box::new(item.clone()),
                    countered: true,
                    bait: Some(0),
                    departure: None,
                }),
            });
        manufactured
            .engine
            .event_history
            .push(CommittedEvent::FishingPoleJournalV39 {
                journal: Box::new(JournalV39::Completed {
                    origin,
                    removal,
                    change: None,
                    token: None,
                }),
            });
        for mut branch in [manufactured.clone(), snapshot(&manufactured)] {
            refuses(&mut branch);
            assert_eq!(
                &branch.engine.event_history[..original.len()],
                original.as_slice()
            );
        }
        engine::counter_stack_item_by_id(&mut state, item.v4.stack_item_id).unwrap();
        lifecycle::validate_state(&state).unwrap();
        assert_eq!(state.objects.get(pole).counters.bait, 0);
        let mut erased = snapshot(&state);
        let input=erased.engine.event_history.iter_mut().find(|e|matches!(e,CommittedEvent::FishingPoleJournalV39 {journal} if matches!(journal.as_ref(),JournalV39::OriginalRemoval {..}))).unwrap();
        *input = CommittedEvent::Tap { object: host };
        refuses(&mut erased);
    }
}

// The accepted generic copy allocator is a physical primitive here, not a claim
// that an admitted FDN spell copies arbitrary artifacts.
fn copied_pole(actor: PlayerId) -> (GameState, ObjectId, ObjectId) {
    let (mut state, host, original) = fixture(actor);
    let token = crate::standard_cards_v1::create_token_copy(&mut state, original, actor).unwrap();
    assert!(state.objects.get(token).v4.is_token);
    assert_eq!(
        state.objects.get(token).card_def,
        state.objects.get(original).card_def
    );
    assert!(state.engine.event_history.iter().any(|event| matches!(event,
        CommittedEvent::CreateToken { object, controller, .. } if *object == token && *controller == actor)));
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(original, Zone::Hand));
    finish(&mut state);
    equip(&mut state, actor, token, host);
    (state, host, token)
}
fn projected_source(state: &GameState, source: ObjectId) -> crate::rl::CardStableRefV1 {
    use crate::rl_session::{FastActorResponseV1, FastActorSessionV1};
    let session = FastActorSessionV1::from_fishing_pole_v39_fixture_state(snapshot(state));
    let FastActorResponseV1::Decision(expected) = session.current_response() else {
        panic!("genuine copied Pole menu");
    };
    let (observation, _, _) = session
        .fishing_pole_current_decision_input_v39(expected, expected.acting_player)
        .unwrap();
    let row = observation
        .projection
        .stack
        .iter()
        .find(|row| row.source.arena_id == source.0)
        .unwrap();
    let historical = observation
        .extensions
        .historical_public_sources
        .iter()
        .find(|row| row.source.arena_id == source.0)
        .unwrap();
    assert_eq!(row.source, historical.source);
    let reference = row.source.clone();
    let human = crate::human_bo3_v1::HumanFishingPoleSessionV39::from_session(
        session,
        expected.acting_player,
    )
    .unwrap();
    let mut projector =
        crate::human_bo3_v1::HumanFishingPoleProjectorV39::new(expected.acting_player);
    let prompt = projector.project_current(&human, expected).unwrap();
    let rendered = serde_json::to_value(&prompt).unwrap();
    fn safe(value: &serde_json::Value) {
        match value {
            serde_json::Value::Object(fields) => {
                for (key, child) in fields {
                    assert!(!matches!(
                        key.as_str(),
                        "arena_id"
                            | "zone_change_count"
                            | "card_db_id"
                            | "event_history"
                            | "producer_sha256"
                            | "binding"
                    ));
                    safe(child);
                }
            }
            serde_json::Value::Array(values) => {
                for value in values {
                    safe(value);
                }
            }
            _ => {}
        }
    }
    safe(&rendered);
    let saved = human.snapshot_v39();
    let mut human = human;
    human.restore_v39(&saved).unwrap();
    let crate::rl_session::FastActorResponseV1::Decision(current) = human.current_response() else {
        panic!("restored copied Pole menu");
    };
    projector.project_current(&human, current).unwrap();
    reference
}
#[test]
#[ignore = "requires actual future Fishing Pole and Fish Token registration"]
fn genuine_copied_pole_grant_and_original_controller_untap_survive_departure_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, host, pole) = copied_pole(actor);
        let original_host = crate::state::AbilitySourceContractV4::capture(&state, host);
        add(&mut state, actor, host, pole);
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(host, Zone::Hand));
        crate::standard_cards_v1::gain_control(&mut state, pole, actor.opponent());
        let shown = projected_source(&state, host);
        assert_eq!(shown.card_db_id, original_host.card_def);
        assert_eq!(shown.zone_change_count, original_host.zone_change_count);
        assert_eq!(shown.zone, Zone::Battlefield);
        let mut restored = snapshot(&state);
        for branch in [&mut state, &mut restored] {
            finish(branch);
            assert_eq!(branch.objects.get(pole).counters.bait, 1);
        }
        assert_eq!(state, restored);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(host, Zone::Battlefield),
        );
        state.objects.get_mut(host).summoning_sick = false;
        crate::standard_cards_v1::gain_control(&mut state, pole, actor);
        equip(&mut state, actor, pole, host);
        engine::step(&mut state, Action::ActivateManaAbility(host)).unwrap();
        assert!(engine::attempt_untap(&mut state, host));
        next(&mut state);
        let original_pole = crate::state::AbilitySourceContractV4::capture(&state, pole);
        crate::standard_cards_v1::gain_control(&mut state, pole, actor.opponent());
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(host, Zone::Hand));
        let shown = projected_source(&state, pole);
        assert_eq!(shown.card_db_id, original_pole.card_def);
        assert_eq!(shown.controller, actor.into());
        let mut restored = snapshot(&state);
        for branch in [&mut state, &mut restored] {
            finish(branch);
            assert_eq!(fish(branch, actor), 1);
            assert_eq!(fish(branch, actor.opponent()), 0);
            assert_eq!(branch.objects.get(pole).counters.bait, 0);
        }
        assert_eq!(state, restored);
    }
}
#[test]
#[ignore = "requires actual future Fishing Pole and Fish Token registration"]
fn genuine_copied_pole_leaves_before_untap_resolution_keeps_public_original_and_creates_no_fish() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, host, pole) = copied_pole(actor);
        add(&mut state, actor, host, pole);
        finish(&mut state);
        assert!(engine::attempt_untap(&mut state, host));
        next(&mut state);
        let original = crate::state::AbilitySourceContractV4::capture(&state, pole);
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(pole, Zone::Hand));
        next(&mut state); // actual token SBA may cease the new incarnation
        let shown = projected_source(&state, pole);
        assert_eq!(shown.card_db_id, original.card_def);
        assert_eq!(shown.zone_change_count, original.zone_change_count);
        assert_eq!(shown.zone, Zone::Battlefield);
        let mut restored = snapshot(&state);
        for branch in [&mut state, &mut restored] {
            finish(branch);
            assert_eq!(fish(branch, actor), 0);
            lifecycle::validate_state(branch).unwrap();
        }
        assert_eq!(state, restored);
    }
}
#[test]
#[ignore = "requires actual future Fishing Pole, Fish and neutral sentinel registration"]
fn original_hidden_host_mask_survives_real_paid_grant_departure_and_restoration_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for disguised in [false, true] {
            let (mut state, host, pole) = copied_pole(actor);
            // Explicit accepted physical face-down characteristic substrate.
            // No FDN printed disguise/hideaway producer or synthetic journal is claimed.
            state.objects.get_mut(host).v4.face_down_v1 = Some(crate::state::FaceDownV1 {
                disguised,
                lookers: 1 << actor.index(),
                hidden_by: None,
                hidden_by_source: None,
            });
            add(&mut state, actor, host, pole);
            let item = state.stack.last().unwrap().clone();
            event::propose_and_commit(&mut state, ProposedEvent::zone_change(host, Zone::Hand));
            for branch in [&state, &snapshot(&state)] {
                let (source, mask) = lifecycle::public_stack_source(branch, &item)
                    .unwrap()
                    .unwrap();
                assert_eq!(source.source, host);
                assert_eq!(mask, Some(disguised));
                if let Some(sentinel) = crate::card_def::card_id_by_name(if disguised {
                    "Face-down creature"
                } else {
                    "Face-down card"
                }) {
                    assert_eq!(projected_source(branch, host).card_db_id, sentinel);
                } else {
                    let session =
                        crate::rl_session::FastActorSessionV1::from_fishing_pole_v39_fixture_state(
                            snapshot(branch),
                        );
                    assert!(matches!(
                        session.current_response(),
                        crate::rl_session::FastActorResponseV1::Terminal(_)
                    ));
                }
            }
            let before = serde_json::to_vec(&state).unwrap();
            let mut forged = item.clone();
            forged.controller = actor.opponent();
            assert!(lifecycle::public_stack_source(&state, &forged).is_err());
            assert_eq!(serde_json::to_vec(&state).unwrap(), before);
            finish(&mut state);
            assert_eq!(state.objects.get(pole).counters.bait, 1);
        }
    }
}

#[test]
#[ignore = "requires actual future Fishing Pole and Fish registration"]
fn private_underlying_pole_is_not_enumerated_as_a_public_current_pole_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, _, pole) = copied_pole(actor);
        // Explicit physical face-down boundary, not a printed artifact disguise producer.
        state.objects.get_mut(pole).v4.face_down_v1 = Some(crate::state::FaceDownV1 {
            disguised: false,
            lookers: 1 << actor.index(),
            hidden_by: None,
            hidden_by_source: None,
        });
        let before = serde_json::to_vec(&state).unwrap();
        assert!(crate::fishing_pole_public_v39::facts_v39(&state)
            .unwrap()
            .poles
            .is_empty());
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        assert!(crate::fishing_pole_public_v39::facts_v39(&snapshot(&state))
            .unwrap()
            .poles
            .is_empty());
    }
}

fn mixed_memorial(actor: PlayerId) -> (GameState, ObjectId, ObjectId) {
    let (mut state, host, pole) = copied_pole(actor);
    let memorial = put(&mut state, actor, "Akroma's Memorial", Zone::Hand);
    state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 7;
    engine::step(&mut state, Action::CastSpell(memorial)).unwrap();
    finish(&mut state);
    assert!(engine::has_effective_keyword(
        &state,
        host,
        crate::card_def::Keywords::HASTE
    ));
    let bow = put(&mut state, actor, "Viridian Longbow", Zone::Battlefield);
    state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 3;
    engine::step(&mut state, Action::ActivateAbility(bow, 0)).unwrap();
    assert!(matches!(next(&mut state), Decision::ChooseTargets { .. }));
    engine::step(&mut state, Action::ChooseTarget(Target::Object(host))).unwrap();
    finish(&mut state);
    state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 1;
    (state, host, pole)
}
#[test]
#[ignore = "requires genuine future Fishing/Fish and Akroma registrations"]
fn real_mixed_family_binding_keeps_named_pole_and_longbow_grants_distinct_and_readonly_both_seats()
{
    use crate::akroma_family_public_v40::{FamilyFactsV40, FamilyProjectionV40};
    use crate::rl_session::{FastActorResponseV1, FastActorSessionV1};
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (state, host, pole) = mixed_memorial(actor);
        let legacy = FastActorSessionV1::from_fishing_pole_v39_fixture_state(snapshot(&state));
        assert!(matches!(
            legacy.current_response(),
            FastActorResponseV1::Terminal(_)
        ));
        let mut session = FastActorSessionV1::from_akroma_v40_fixture_state(state);
        let FastActorResponseV1::Decision(expected) = session.current_response() else {
            panic!("actual mixed menu");
        };
        let (public, actions, binding) = session
            .akroma_current_decision_input_v40(expected, expected.acting_player)
            .unwrap();
        assert!(!public.grants.is_empty());
        assert!(public.families.iter().any(|f| matches!(f, FamilyFactsV40::Fishing(f) if f.poles.iter().any(|p| p.source.arena_id==pole.0))));
        let FamilyProjectionV40::Fishing(fishing) = binding
            .families
            .iter()
            .find(|f| matches!(f, FamilyProjectionV40::Fishing(_)))
            .unwrap()
        else {
            unreachable!();
        };
        let grants = fishing
            .actions
            .iter()
            .filter_map(|a| a.granted_label.as_ref())
            .collect::<Vec<_>>();
        assert!(grants.iter().any(|g| g.fishing));
        assert!(grants.iter().any(|g| !g.fishing));
        assert_eq!(
            fishing
                .actions
                .iter()
                .map(|a| a.semantic.clone())
                .collect::<Vec<_>>(),
            actions
        );
        let before = session.privileged_core_environment_hash();
        let mut forged = binding.clone();
        let FamilyProjectionV40::Fishing(f) = forged
            .families
            .iter_mut()
            .find(|f| matches!(f, FamilyProjectionV40::Fishing(_)))
            .unwrap()
        else {
            unreachable!();
        };
        f.actions
            .iter_mut()
            .find(|a| a.granted_label.as_ref().is_some_and(|g| g.fishing))
            .unwrap()
            .granted_label
            .as_mut()
            .unwrap()
            .cost
            .clear();
        assert!(session
            .akroma_validate_bound_decision_v40(expected, expected.acting_player, &forged)
            .is_err());
        assert_eq!(session.privileged_core_environment_hash(), before);
        let original = session.akroma_fixture_state().engine.event_history.clone();
        crate::standard_cards_v1::gain_control(
            session.akroma_fixture_state_mut(),
            pole,
            actor.opponent(),
        );
        let bytes = serde_json::to_vec(session.akroma_fixture_state()).unwrap();
        assert!(session
            .akroma_validate_bound_decision_v40(expected, expected.acting_player, &binding)
            .is_err());
        assert_eq!(
            serde_json::to_vec(session.akroma_fixture_state()).unwrap(),
            bytes
        );
        assert_eq!(
            session.akroma_fixture_state().engine.event_history[..original.len()],
            original
        );
        assert_eq!(
            session.akroma_fixture_state().objects.get(host).controller,
            actor
        );
    }
}
#[test]
#[ignore = "requires genuine future Fishing/Fish and Akroma registrations"]
fn actual_mixed_human_named_grant_cost_restore_retry_and_current_facts_both_seats() {
    use crate::akroma_family_public_v40::FamilyFactsV40;
    use crate::human_bo3_v1::{
        HumanActionRequestV1, HumanAkromaProjectorV40, HumanAkromaSessionV40, HumanDecisionErrorV1,
    };
    use crate::rl_session::{FastActorResponseV1, FastActorSessionV1};
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (state, _, pole) = mixed_memorial(actor);
        let mut human = HumanAkromaSessionV40::from_session(
            FastActorSessionV1::from_akroma_v40_fixture_state(state),
            actor.into(),
        )
        .unwrap();
        let mut projector = HumanAkromaProjectorV40::new(actor.into());
        let FastActorResponseV1::Decision(expected) = human.current_response() else {
            panic!("actual mixed Human");
        };
        let prompt = projector.project_current(&human, expected).unwrap();
        assert!(prompt
            .actions
            .iter()
            .any(|a| a.label.starts_with("Activate Fishing Pole-granted ability")));
        assert!(prompt.actions.iter().any(
            |a| a.label.starts_with("Activate granted ability") && a.label.contains("1 damage")
        ));
        let action = prompt
            .actions
            .iter()
            .find(|a| a.label.starts_with("Activate Fishing Pole-granted ability"))
            .unwrap();
        let request = HumanActionRequestV1 {
            prompt_seq: prompt.prompt_seq,
            action_index: action.action_index,
        };
        let saved = human.snapshot_v40();
        human.restore_v40(&saved).unwrap();
        assert_eq!(
            projector.submit(&mut human, request),
            Err(HumanDecisionErrorV1::StaleDecision)
        );
        let FastActorResponseV1::Decision(expected) = human.current_response() else {
            panic!("restored mixed Human");
        };
        let prompt = projector.project_current(&human, expected).unwrap();
        let action = prompt
            .actions
            .iter()
            .find(|a| a.label.starts_with("Activate Fishing Pole-granted ability"))
            .unwrap();
        projector
            .submit(
                &mut human,
                HumanActionRequestV1 {
                    prompt_seq: prompt.prompt_seq,
                    action_index: action.action_index,
                },
            )
            .unwrap();
        let FastActorResponseV1::Decision(expected) = human.current_response() else {
            panic!("real named Pole cost");
        };
        let prompt = projector.project_current(&human, expected).unwrap();
        let action = prompt
            .actions
            .iter()
            .find(|a| a.label.contains("Fishing Pole"))
            .unwrap();
        let request = HumanActionRequestV1 {
            prompt_seq: prompt.prompt_seq,
            action_index: action.action_index,
        };
        let receipt = projector.submit(&mut human, request.clone()).unwrap();
        assert_eq!(projector.submit(&mut human, request).unwrap(), receipt);
        let mut done = false;
        for _ in 0..30 {
            let FastActorResponseV1::Decision(expected) = human.current_response() else {
                panic!("real mixed priority");
            };
            if expected.acting_player == actor.into() {
                let observation = human.public_observation(expected).unwrap();
                assert!(!observation.grants.is_empty());
                if observation.families.iter().any(|f| matches!(f, FamilyFactsV40::Fishing(f) if f.poles.iter().any(|p| p.source.arena_id==pole.0 && p.bait==1) && f.operations.iter().any(|op| !op.trigger && op.stage==crate::fishing_pole_public_v39::StageV39::Resolved))) { done=true; break; }
                let prompt = projector.project_current(&human, expected).unwrap();
                let action = prompt
                    .actions
                    .iter()
                    .find(|a| a.label == "Pass priority")
                    .unwrap();
                projector
                    .submit(
                        &mut human,
                        HumanActionRequestV1 {
                            prompt_seq: prompt.prompt_seq,
                            action_index: action.action_index,
                        },
                    )
                    .unwrap();
            } else {
                assert!(human.public_observation(expected).is_err());
                human
                    .step_opponent_with(expected, |observation, actions| {
                        assert_eq!(observation.acting_player, actor.opponent().into());
                        actions
                            .iter()
                            .position(|a| matches!(a, crate::rl::ActionSemanticV1::Pass { .. }))
                            .map(|i| i as u32)
                            .ok_or_else(|| {
                                crate::rl::RlContractError("actual opponent pass".into())
                            })
                    })
                    .unwrap();
            }
        }
        assert!(done);
    }
}
#[test]
#[ignore = "requires genuine future Fishing/Fish and Akroma registrations"]
fn actual_mixed_copied_pole_untap_original_controller_history_and_human_restore_both_seats() {
    use crate::akroma_family_public_v40::FamilyFactsV40;
    use crate::human_bo3_v1::{HumanAkromaProjectorV40, HumanAkromaSessionV40};
    use crate::rl_session::{FastActorResponseV1, FastActorSessionV1};
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, host, pole) = mixed_memorial(actor);
        add(&mut state, actor, host, pole);
        finish(&mut state);
        assert!(engine::attempt_untap(&mut state, host));
        next(&mut state);
        crate::standard_cards_v1::gain_control(&mut state, pole, actor.opponent());
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(host, Zone::Hand));
        let mut restored = snapshot(&state);
        let base = FastActorSessionV1::from_akroma_v40_fixture_state(snapshot(&state));
        let FastActorResponseV1::Decision(expected) = base.current_response() else {
            panic!("true mixed original trigger");
        };
        let mut human = HumanAkromaSessionV40::from_session(base, expected.acting_player).unwrap();
        let mut projector = HumanAkromaProjectorV40::new(expected.acting_player);
        let prompt = projector.project_current(&human, expected).unwrap();
        assert!(!prompt.state.extensions.grants.is_empty());
        let snapshot = human.snapshot_v40();
        human.restore_v40(&snapshot).unwrap();
        let FastActorResponseV1::Decision(expected) = human.current_response() else {
            panic!("restored mixed source");
        };
        let observation = human.public_observation(expected).unwrap();
        assert!(observation.families.iter().any(|f| matches!(f, FamilyFactsV40::Fishing(f) if f.operations.iter().any(|op| op.trigger && op.original_controller==actor.into() && op.original_pole.arena_id==pole.0))));
        projector.project_current(&human, expected).unwrap();
        for branch in [&mut state, &mut restored] {
            finish(branch);
            assert_eq!(fish(branch, actor), 1);
            assert_eq!(fish(branch, actor.opponent()), 0);
        }
        assert_eq!(state, restored);
    }
}
