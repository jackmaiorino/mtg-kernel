//! Registered physical primitives; future paid full-Ajani games are separate.
use super::*;
use crate::effect::ExecCtx;
use crate::engine;
use crate::ids::PlayerId;
use crate::policy_observation_v6::tests::{put, ready_state_for};
use crate::state::{GameState, StackTargetContractV4, Target};

#[test]
fn registered_optional_creature_pool_and_controller_life_preserve_exact_bounds_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state_for(actor);
        let own = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        let other = put(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        put(&mut state, actor, "Forest", Zone::Battlefield);
        let expected =
            engine::legal_targets_for_controller(TargetSpec::Creature, &[], actor, &state);
        let optional = engine::legal_targets_for_controller(
            TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::UpToOneCreature),
            &[],
            actor,
            &state,
        );
        assert_eq!(optional, expected);
        assert!(
            optional.contains(&Target::Object(own)) && optional.contains(&Target::Object(other))
        );
        for life in [-2, 0, 1, 19, i32::MAX] {
            state.players[actor.index()].life = life;
            state.players[actor.opponent().index()].life = 73;
            assert_eq!(
                engine::evaluate_dynamic_value(&state, LIFE_COUNT, actor),
                life
            );
        }
        let restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        assert_eq!(
            engine::evaluate_dynamic_value(&restored, LIFE_COUNT, actor),
            i32::MAX
        );
        assert_eq!(
            TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::UpToOneCreature)
                .stable_id(),
            60
        );
    }
}

#[test]
fn actual_optional_counter_leaf_zero_and_exact_target_incarnation_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state_for(actor);
        let source = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        let target = put(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        let op = EffectOp::AddCountersToTarget {
            target_index: 0,
            optional: true,
            plus1_plus1: 1,
            lifelink: 0,
            stun: 0,
        };
        let mut ctx = ExecCtx::no_targets(source, actor);
        let before = serde_json::to_vec(&state).unwrap();
        crate::effect::execute(&op, &ctx, &mut state);
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        ctx.targets = vec![Target::Object(target)];
        ctx.target_contracts = vec![StackTargetContractV4::capture(
            &state,
            Target::Object(target),
        )];
        let mut restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        for branch in [&mut state, &mut restored] {
            crate::effect::execute(&op, &ctx, branch);
            assert_eq!(branch.objects.get(target).counters.plus1_plus1, 1);
            crate::event::propose_and_commit(
                branch,
                crate::event::ProposedEvent::zone_change(target, Zone::Hand),
            );
            crate::event::propose_and_commit(
                branch,
                crate::event::ProposedEvent::zone_change(target, Zone::Battlefield),
            );
            crate::effect::execute(&op, &ctx, branch);
            assert_eq!(branch.objects.get(target).counters.plus1_plus1, 0);
            assert!(branch.engine.halted.is_none());
        }
        assert_eq!(state, restored);
    }
}

#[test]
fn ordinary_registered_state_has_no_ajani_journal_or_mutation() {
    let state = ready_state_for(PlayerId::P0);
    let bytes = serde_json::to_vec(&state).unwrap();
    lifecycle::validate_state(&state).unwrap();
    assert!(!lifecycle::requires_successor(&state));
    assert_eq!(serde_json::to_vec(&state).unwrap(), bytes);
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod future {
    use super::*;
    use crate::engine::{Action, Decision};
    use crate::event::{CommittedEvent, ProposedEvent};
    use crate::ids::ObjectId;
    use crate::mana::ManaColor;
    use crate::state::{StackItem, Step};

    fn ready(actor: PlayerId) -> GameState {
        let forest = crate::card_def::card_id_by_name("Forest").unwrap();
        let mut state = GameState::new_from_libraries_with_starting_player_v1(
            &[forest; 40],
            &[forest; 40],
            |_| "Forest".into(),
            42042,
            actor,
        );
        state.step = Step::Main1;
        put(&mut state, actor, "Forest", Zone::Battlefield);
        put(&mut state, actor.opponent(), "Forest", Zone::Battlefield);
        state
    }
    fn next(state: &mut GameState) -> Decision {
        let d = engine::advance_until_decision(state);
        assert!(!matches!(d, Decision::Halted { .. }), "{d:?}");
        d
    }
    fn finish(state: &mut GameState) {
        for _ in 0..40 {
            let d = next(state);
            if state.stack.is_empty()
                && state.engine.pending_activation.is_none()
                && state.engine.pending_triggers.is_empty()
                && matches!(d, Decision::CastSpellOrPass { .. })
            {
                return;
            }
            match d {
                Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
                other => panic!("unexpected traversal {other:?}"),
            }
        }
        panic!("actual loyalty resolution did not complete");
    }
    fn cast(state: &mut GameState, actor: PlayerId) -> ObjectId {
        let source = put(state, actor, "Ajani, Caller of the Pride", Zone::Hand);
        validate_definition(state.objects.get(source).card_def).unwrap();
        state.players[actor.index()].mana_pool[ManaColor::W.pool_index()] = 3;
        assert!(
            matches!(next(state),Decision::CastSpellOrPass{castable_spells,..} if castable_spells.contains(&source))
        );
        engine::step(state, Action::CastSpell(source)).unwrap();
        finish(state);
        assert_eq!(crate::planeswalker_v1::loyalty(state, source), Some(4));
        source
    }
    fn activate(
        state: &mut GameState,
        source: ObjectId,
        index: u8,
        target: Option<ObjectId>,
    ) -> StackItem {
        assert!(
            matches!(next(state),Decision::CastSpellOrPass{activatable_abilities,..} if activatable_abilities.contains(&(source,index)))
        );
        engine::step(state, Action::ActivateAbility(source, index)).unwrap();
        match next(state) {
            Decision::ChooseEffectTargets { .. } => {
                engine::step(
                    state,
                    target.map_or(Action::FinishEffectSelection, |t| {
                        Action::ChooseEffectTarget(Target::Object(t))
                    }),
                )
                .unwrap();
            }
            Decision::ChooseTargets { .. } => {
                engine::step(state, Action::ChooseTarget(Target::Object(target.unwrap()))).unwrap();
            }
            Decision::CastSpellOrPass { .. } => {}
            other => panic!("unexpected actual loyalty target stage {other:?}"),
        }
        next(state);
        assert!(state.engine.pending_activation.is_none());
        state
            .stack
            .iter()
            .find(|i| i.source == source && i.kind == crate::state::StackItemKind::ActivatedAbility)
            .unwrap()
            .clone()
    }
    fn restored(state: &GameState) -> GameState {
        serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
    }
    fn refuses(mut state: GameState) {
        let bytes = serde_json::to_vec(&state).unwrap();
        assert!(lifecycle::validate_state(&state).is_err());
        assert!(engine::step(&mut state, Action::Pass).is_err());
        assert_eq!(serde_json::to_vec(&state).unwrap(), bytes);
        assert!(lifecycle::validate_state(&restored(&state)).is_err());
    }

    #[test]
    #[ignore = "requires actual Full Ajani promotion and 2/2 Cat token admission"]
    fn genuine_cast_zero_target_paid_once_and_targeted_counter_restore_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            for targeted in [false, true] {
                let mut state = ready(actor);
                let source = cast(&mut state, actor);
                let elf = put(
                    &mut state,
                    actor.opponent(),
                    "Llanowar Elves",
                    Zone::Battlefield,
                );
                let item = activate(&mut state, source, 0, targeted.then_some(elf));
                assert_eq!(crate::planeswalker_v1::loyalty(&state, source), Some(5));
                let bytes = serde_json::to_vec(&state).unwrap();
                assert!(engine::step(&mut state, Action::ActivateAbility(source, 1)).is_err());
                assert_eq!(serde_json::to_vec(&state).unwrap(), bytes);
                let mut replay = restored(&state);
                finish(&mut state);
                finish(&mut replay);
                assert_eq!(state, replay);
                assert_eq!(
                    state.objects.get(elf).counters.plus1_plus1,
                    i32::from(targeted)
                );
                let mut forged = state.clone();
                forged.stack.push(item);
                refuses(forged);
            }
        }
    }
    #[test]
    #[ignore = "requires actual Full Ajani promotion and 2/2 Cat token admission"]
    fn genuine_strike_departure_reentry_control_target_fizzle_and_natural_expiry_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            for stale in [false, true] {
                let mut state = ready(actor);
                let source = cast(&mut state, actor);
                let elf = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
                activate(&mut state, source, 1, Some(elf));
                assert_eq!(crate::planeswalker_v1::loyalty(&state, source), Some(1));
                crate::event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(source, Zone::Hand),
                );
                if stale {
                    crate::event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(elf, Zone::Hand),
                    );
                    crate::event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(elf, Zone::Battlefield),
                    );
                }
                let mut replay = restored(&state);
                finish(&mut state);
                finish(&mut replay);
                assert_eq!(state, replay);
                for keyword in [Keywords::FLYING, Keywords::DOUBLE_STRIKE] {
                    assert_eq!(engine::has_effective_keyword(&state, elf, keyword), !stale);
                }
                if !stale {
                    for _ in 0..60 {
                        let d = next(&mut state);
                        if state.active_player != actor {
                            break;
                        }
                        match d {
                            Decision::CastSpellOrPass { .. } => {
                                engine::step(&mut state, Action::Pass).unwrap()
                            }
                            Decision::DeclareAttackers { .. } => {
                                engine::step(&mut state, Action::DeclareAttackers(vec![])).unwrap()
                            }
                            other => panic!("{other:?}"),
                        }
                    }
                    assert!(!engine::has_effective_keyword(
                        &state,
                        elf,
                        Keywords::DOUBLE_STRIKE
                    ));
                    lifecycle::validate_state(&state).unwrap();
                }
            }
        }
    }
    #[test]
    #[ignore = "requires actual Full Ajani promotion and 2/2 Cat token admission"]
    fn ultimate_uses_original_controller_current_life_after_source_departure_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready(actor);
            let source = cast(&mut state, actor);
            // Accepted physical loyalty-counter boundary, not a printed support claim.
            crate::planeswalker_v1::change_loyalty(&mut state, source, 4);
            activate(&mut state, source, 2, None);
            state.players[actor.index()].life = 3;
            state.players[actor.opponent().index()].life = 17;
            assert_eq!(state.objects.get(source).zone, Zone::Graveyard);
            let mut replay = restored(&state);
            finish(&mut state);
            finish(&mut replay);
            assert_eq!(state, replay);
            let token = token_definition().unwrap();
            let created = state
                .engine
                .event_history
                .iter()
                .filter_map(|e| match e {
                    CommittedEvent::CreateToken {
                        object,
                        token_def,
                        controller,
                    } if *token_def == token => {
                        assert_eq!(*controller, actor);
                        Some(*object)
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(created.len(), 3);
            for object in created {
                assert_eq!(state.objects.get(object).owner, actor);
                assert_eq!(engine::effective_power(&state, object), 2);
                assert_eq!(engine::effective_toughness(&state, object), 2);
            }
        }
    }
    #[test]
    #[ignore = "requires actual Full Ajani promotion and 2/2 Cat token admission"]
    fn paid_owner_erasure_coherent_source_rewrite_counter_and_completed_replay_both_seats() {
        use lifecycle::JournalV42;
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready(actor);
            let source = cast(&mut state, actor);
            let item = activate(&mut state, source, 0, None);
            let mut erased = restored(&state);
            erased.stack.clear();
            refuses(erased);
            let mut erased = restored(&state);
            erased.engine.event_history.iter_mut().filter(|e|matches!(e,CommittedEvent::AjaniJournalV42{journal} if matches!(journal.as_ref(),JournalV42::PaymentInput{..}))).for_each(|e|*e=CommittedEvent::Tap{object:source});
            refuses(erased);
            let mut rewritten = restored(&state);
            rewritten.stack.last_mut().unwrap().controller = actor.opponent();
            rewritten
                .stack
                .last_mut()
                .unwrap()
                .v4
                .ability_source_contract
                .as_mut()
                .unwrap()
                .controller = actor.opponent();
            for e in &mut rewritten.engine.event_history {
                if let CommittedEvent::AjaniJournalV42 { journal } = e {
                    if let JournalV42::Paid { item, .. } = journal.as_mut() {
                        item.controller = actor.opponent();
                        item.v4.ability_source_contract.as_mut().unwrap().controller =
                            actor.opponent();
                    }
                }
            }
            refuses(rewritten);
            // Genuine accepted exact-ID counter primitive. An Offer only counters spells.
            engine::counter_stack_item_by_id(&mut state, item.v4.stack_item_id).unwrap();
            lifecycle::validate_state(&state).unwrap();
            let mut replay = restored(&state);
            replay.stack.push(item);
            refuses(replay);
        }
    }
}
