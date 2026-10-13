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
                if !stale {
                    // Accepted physical control producer after genuine payment;
                    // this is not a claim that Employment targets planeswalkers.
                    crate::standard_cards_v1::gain_control(&mut state, elf, actor.opponent());
                }
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
    fn ultimate_uses_original_controller_current_life_after_source_departure_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready(actor);
            let source = cast(&mut state, actor);
            // Accepted physical loyalty-counter boundary, not a printed support claim.
            crate::planeswalker_v1::change_loyalty(&mut state, source, 4);
            activate(&mut state, source, 2, None);
            crate::event::propose_and_commit(&mut state, ProposedEvent::life_loss(actor, 17));
            crate::event::propose_and_commit(
                &mut state,
                ProposedEvent::life_loss(actor.opponent(), 3),
            );
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
    fn cast_named(
        state: &mut GameState,
        actor: PlayerId,
        name: &str,
        color: ManaColor,
        amount: u8,
    ) -> ObjectId {
        let source = put(state, actor, name, Zone::Hand);
        assert!(CARD_DEFS[usize::from(state.objects.get(source).card_def)].has_full_support());
        state.players[actor.index()].mana_pool[color.pool_index()] = amount;
        assert!(
            matches!(next(state),Decision::CastSpellOrPass{player,castable_spells,..} if player==actor && castable_spells.contains(&source))
        );
        engine::step(state, Action::CastSpell(source)).unwrap();
        next(state);
        if CARD_DEFS[usize::from(state.objects.get(source).card_def)].target_spec
            == TargetSpec::None
        {
            assert!(state.engine.pending_cast.is_none());
        }
        source
    }
    fn genuine_public_and_owning_restore(
        state: &GameState,
        actor: PlayerId,
    ) -> crate::ajani_observation_v42::AjaniObservationV42 {
        use crate::rl_session::{FastActorResponseV1, FastActorSessionV1};
        let mut session = FastActorSessionV1::from_ajani_v42_fixture_state(state.clone());
        let FastActorResponseV1::Decision(expected) = session.current_response() else {
            panic!("genuine V42 mixed decision");
        };
        assert_eq!(expected.acting_player, actor.into());
        let (public, _, binding) = session
            .ajani_current_decision_input_v42(expected, actor.into())
            .unwrap();
        public.validate_artifact().unwrap();
        let snapshot = session.snapshot_v1();
        session.ajani_restore_v42(&snapshot).unwrap();
        assert!(session
            .ajani_validate_bound_decision_v42(expected, actor.into(), &binding)
            .is_err());
        let FastActorResponseV1::Decision(current) = session.current_response() else {
            panic!("genuine V42 restored decision");
        };
        let (restored, _, new_binding) = session
            .ajani_current_decision_input_v42(current, actor.into())
            .unwrap();
        session
            .ajani_validate_bound_decision_v42(current, actor.into(), &new_binding)
            .unwrap();
        assert_eq!(restored.ajani, public.ajani);
        assert_eq!(restored.families, public.families);
        assert_eq!(restored.projection, public.projection);
        public
    }
    #[test]
    fn protected_herald_nonpositive_ultimate_is_real_zero_operation_with_mixed_restore_both_seats()
    {
        for actor in [PlayerId::P0, PlayerId::P1] {
            for life in [0, -3] {
                let mut state = ready(actor);
                let source = cast(&mut state, actor);
                let herald =
                    cast_named(&mut state, actor, "Herald of Eternal Dawn", ManaColor::W, 7);
                finish(&mut state);
                assert_eq!(state.objects.get(herald).zone, Zone::Battlefield);
                assert!(crate::game_outcome_protection_v1::cannot_lose(
                    &state, actor
                ));
                crate::event::propose_and_commit(
                    &mut state,
                    ProposedEvent::life_loss(actor, 20 - life),
                );
                assert_eq!(state.players[actor.index()].life, life);
                // Accepted physical loyalty-counter placement before the actual -8.
                crate::planeswalker_v1::change_loyalty(&mut state, source, 4);
                let item = activate(&mut state, source, 2, None);
                assert_eq!(state.objects.get(source).zone, Zone::Graveyard);
                let public = genuine_public_and_owning_restore(&state, state.priority_player);
                assert!(public.families.iter().any(|f|matches!(f,crate::akroma_family_public_v40::FamilyFactsV40::GameOutcome(facts) if facts.protection.cannot_lose[actor.index()])));
                let mut replay = restored(&state);
                let start = state.engine.event_history.len();
                finish(&mut state);
                finish(&mut replay);
                assert_eq!(state, replay);
                assert!(!state.engine.event_history[start..]
                    .iter()
                    .any(|e| matches!(e, CommittedEvent::CreateToken { .. })));
                lifecycle::validate_state(&state).unwrap();
                let public = genuine_public_and_owning_restore(&state, state.priority_player);
                let operation = &public.ajani.activations[0];
                assert_eq!(operation.stage, crate::ajani_public_v42::StageV42::Resolved);
                assert_eq!(operation.controller_life_at_resolution, Some(life));
                assert!(operation.created_tokens.is_empty());
                let mut forged = state.clone();
                forged.stack.push(item);
                refuses(forged);
            }
        }
    }
    #[test]
    fn genuine_copy_allocator_initializes_loyalty_before_real_legend_choice_then_token_can_activate_both_seats(
    ) {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready(actor);
            let source = cast(&mut state, actor);
            // Actual accepted permanent-copy allocator, not an assertion that
            // Self-Reflection can target a noncreature planeswalker.
            let token =
                crate::standard_cards_v1::create_token_copy(&mut state, source, actor).unwrap();
            assert!(state.objects.get(token).v4.is_token);
            assert_eq!(crate::planeswalker_v1::loyalty(&state, token), Some(4));
            assert!(
                matches!(next(&mut state),Decision::ChooseLegendPermanent{player,candidates} if player==actor && candidates.contains(&source)&&candidates.contains(&token))
            );
            let public = genuine_public_and_owning_restore(&state, actor);
            assert_eq!(public.ajani.permanents.len(), 2);
            let mut replay = restored(&state);
            for branch in [&mut state, &mut replay] {
                engine::step(branch, Action::ChooseLegendPermanent(token)).unwrap();
                next(branch);
                assert_eq!(branch.objects.get(source).zone, Zone::Graveyard);
                assert_eq!(crate::planeswalker_v1::loyalty(branch, token), Some(4));
                activate(branch, token, 0, None);
                finish(branch);
                assert_eq!(crate::planeswalker_v1::loyalty(branch, token), Some(5));
            }
            assert_eq!(state, replay);
            let public = genuine_public_and_owning_restore(&state, state.priority_player);
            assert_eq!(
                public.ajani.activations[0].source_at_payment.arena_id,
                token.0
            );
        }
    }
    #[test]
    fn real_bolt_planeswalker_damage_and_twin_multiplier_preserve_loyalty_and_owner_on_restore_both_seats(
    ) {
        for actor in [PlayerId::P0, PlayerId::P1] {
            for doubled in [false, true] {
                let mut state = ready(actor);
                let source = cast(&mut state, actor);
                if doubled {
                    let tyrant = put(&mut state, actor.opponent(), "Twinflame Tyrant", Zone::Hand);
                    crate::event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(tyrant, Zone::Battlefield),
                    );
                }
                assert!(
                    matches!(next(&mut state),Decision::CastSpellOrPass{player,..} if player==actor)
                );
                engine::step(&mut state, Action::Pass).unwrap();
                let bolt = cast_named(
                    &mut state,
                    actor.opponent(),
                    "Lightning Bolt",
                    ManaColor::R,
                    1,
                );
                assert!(
                    matches!(next(&mut state),Decision::ChooseTargets{legal_targets,..} if legal_targets.contains(&Target::Object(source)))
                );
                engine::step(&mut state, Action::ChooseTarget(Target::Object(source))).unwrap();
                next(&mut state);
                let public = genuine_public_and_owning_restore(&state, state.priority_player);
                if doubled {
                    assert!(public.families.iter().any(|f|matches!(f,crate::akroma_family_public_v40::FamilyFactsV40::Twinflame(facts) if !facts.multiplier_sources.is_empty())));
                }
                let mut replay = restored(&state);
                finish(&mut state);
                finish(&mut replay);
                assert_eq!(state, replay);
                assert!(state.engine.event_history.iter().any(|e|matches!(e,CommittedEvent::Damage{source:s,target:Target::Object(t),amount} if *s==bolt && *t==source && *amount==if doubled{6}else{3})));
                assert_eq!(state.objects.get(source).owner, actor);
                assert_eq!(
                    state.objects.get(source).zone,
                    if doubled {
                        Zone::Graveyard
                    } else {
                        Zone::Battlefield
                    }
                );
                assert_eq!(
                    crate::planeswalker_v1::loyalty(&state, source),
                    if doubled { None } else { Some(1) }
                );
                genuine_public_and_owning_restore(&state, state.priority_player);
            }
        }
    }
    #[test]
    fn paid_temporal_same_seat_extra_turn_reopens_one_loyalty_use_with_new_occurrence_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready(actor);
            let source = cast(&mut state, actor);
            activate(&mut state, source, 0, None);
            finish(&mut state);
            assert_eq!(crate::planeswalker_v1::loyalty(&state, source), Some(5));
            cast_named(&mut state, actor, "Temporal Manipulation", ManaColor::U, 5);
            finish(&mut state);
            crate::extra_turn_v1::validate(&state).unwrap();
            let round = state.turn;
            let old_occurrence = state.turn_key_v1();
            let mut replay = restored(&state);
            for branch in [&mut state, &mut replay] {
                for _ in 0..90 {
                    let d = next(branch);
                    if branch.active_player == actor
                        && branch.turn_key_v1() > old_occurrence
                        && branch.step == Step::Main1
                        && branch.priority_player == actor
                    {
                        assert!(
                            matches!(d,Decision::CastSpellOrPass{activatable_abilities,..} if activatable_abilities.contains(&(source,0)))
                        );
                        break;
                    }
                    match d {
                        Decision::CastSpellOrPass { .. } => {
                            engine::step(branch, Action::Pass).unwrap()
                        }
                        Decision::DeclareAttackers { .. } => {
                            engine::step(branch, Action::DeclareAttackers(vec![])).unwrap()
                        }
                        other => panic!("unexpected actual extra-turn traversal {other:?}"),
                    }
                }
                assert_eq!(branch.active_player, actor);
                assert_eq!(branch.turn, round);
                assert!(branch.turn_key_v1() > old_occurrence);
                assert!(!lifecycle::used(branch, source));
                activate(branch, source, 0, None);
                finish(branch);
                assert_eq!(crate::planeswalker_v1::loyalty(branch, source), Some(6));
                assert!(lifecycle::used(branch, source));
                let before = serde_json::to_vec(branch).unwrap();
                assert!(engine::step(branch, Action::ActivateAbility(source, 0)).is_err());
                assert_eq!(serde_json::to_vec(branch).unwrap(), before);
            }
            assert_eq!(state, replay);
            genuine_public_and_owning_restore(&state, state.priority_player);
        }
    }
    #[test]
    fn stolen_source_sorcery_timing_and_original_controller_life_survive_owner_reentry_both_seats()
    {
        for owner in [PlayerId::P0, PlayerId::P1] {
            let controller = owner.opponent();
            let mut state = ready(owner);
            let source = cast(&mut state, owner);
            // Genuine accepted control primitive. No printed Employment-on-
            // planeswalker support or fabricated source/cast receipts is asserted.
            crate::standard_cards_v1::gain_control(&mut state, source, controller);
            let before = serde_json::to_vec(&state).unwrap();
            assert!(engine::step(&mut state, Action::ActivateAbility(source, 0)).is_err());
            assert_eq!(serde_json::to_vec(&state).unwrap(), before);
            for _ in 0..90 {
                let d = next(&mut state);
                if state.active_player == controller
                    && state.step == Step::Main1
                    && state.priority_player == controller
                {
                    assert!(
                        matches!(d,Decision::CastSpellOrPass{activatable_abilities,..} if activatable_abilities.contains(&(source,0)))
                    );
                    break;
                }
                match d {
                    Decision::CastSpellOrPass { .. } => {
                        engine::step(&mut state, Action::Pass).unwrap()
                    }
                    Decision::DeclareAttackers { .. } => {
                        engine::step(&mut state, Action::DeclareAttackers(vec![])).unwrap()
                    }
                    other => panic!("unexpected actual control/turn traversal {other:?}"),
                }
            }
            assert_eq!(state.active_player, controller);
            assert_eq!(state.objects.get(source).owner, owner);
            assert_eq!(state.objects.get(source).controller, controller);
            crate::planeswalker_v1::change_loyalty(&mut state, source, 4);
            let item = activate(&mut state, source, 2, None);
            assert_eq!(item.controller, controller);
            assert_eq!(item.v4.ability_source_contract.unwrap().owner, owner);
            assert_eq!(state.objects.get(source).zone, Zone::Graveyard);
            crate::event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(source, Zone::Hand),
            );
            crate::event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(source, Zone::Battlefield),
            );
            assert_eq!(state.objects.get(source).controller, owner);
            assert_eq!(crate::planeswalker_v1::loyalty(&state, source), Some(4));
            crate::event::propose_and_commit(&mut state, ProposedEvent::life_loss(owner, 17));
            crate::event::propose_and_commit(&mut state, ProposedEvent::life_loss(controller, 14));
            let public = genuine_public_and_owning_restore(&state, state.priority_player);
            assert_eq!(
                public.ajani.activations[0].source_at_payment.controller,
                controller.into()
            );
            assert!(public.ajani.activations[0]
                .current_original_source
                .is_none());
            let mut replay = restored(&state);
            let start = state.engine.event_history.len();
            finish(&mut state);
            finish(&mut replay);
            assert_eq!(state, replay);
            let token = token_definition().unwrap();
            let created = state.engine.event_history[start..]
                .iter()
                .filter_map(|e| match e {
                    CommittedEvent::CreateToken {
                        object,
                        token_def,
                        controller: c,
                    } if *token_def == token => {
                        assert_eq!(*c, controller);
                        Some(*object)
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(created.len(), 6);
            for token in created {
                assert_eq!(state.objects.get(token).owner, controller);
            }
            let public = genuine_public_and_owning_restore(&state, state.priority_player);
            assert_eq!(
                public.ajani.activations[0].controller_life_at_resolution,
                Some(6)
            );
            assert_eq!(public.ajani.activations[0].created_tokens.len(), 6);
        }
    }
}
