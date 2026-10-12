use super::*;
use crate::engine::{self, Action, Decision};
use crate::policy_observation_v6::tests::{put, ready_state};
#[test]
fn unrelated_actual_bolt_private_loan_publication_and_restore_preserve_bytes_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state();
        state.active_player = actor;
        state.priority_player = actor;
        let bolt = put(&mut state, actor, "Lightning Bolt", Zone::Hand);
        state.players[actor.index()].mana_pool[crate::mana::ManaColor::R.pool_index()] = 1;
        engine::step(&mut state, Action::CastSpell(bolt)).unwrap();
        engine::step(
            &mut state,
            Action::ChooseTarget(Target::Player(actor.opponent())),
        )
        .unwrap();
        assert!(matches!(
            engine::advance_until_decision(&mut state),
            Decision::CastSpellOrPass { .. }
        ));
        let item = state
            .stack
            .iter()
            .find(|i| i.source == bolt)
            .unwrap()
            .clone();
        let before = serde_json::to_vec(&state).unwrap();
        let hash = state.state_hash();
        assert!(acquire(&state, &item).unwrap().is_none());
        publish(&mut state, item.v4.stack_item_id).unwrap();
        note_cleanup(&mut state).unwrap();
        expire(&mut state).unwrap();
        validate_state(&state).unwrap();
        assert!(!permission(&state, bolt, actor, 0));
        assert!(!has_active(&state));
        assert_eq!(before, serde_json::to_vec(&state).unwrap());
        assert_eq!(hash, state.state_hash());
        let mut restored: GameState = serde_json::from_slice(&before).unwrap();
        engine::step(&mut state, Action::Pass).unwrap();
        engine::step(&mut restored, Action::Pass).unwrap();
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&restored).unwrap()
        );
    }
}
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod future_printed {
    use super::*;
    fn next(state: &mut GameState) -> Decision {
        let d = engine::advance_until_decision(state);
        assert!(!matches!(d, Decision::Halted { .. }), "{d:?}");
        d
    }
    fn setup(actor: PlayerId) -> (GameState, ObjectId, ObjectId) {
        let mut state = ready_state();
        state.active_player = actor;
        state.priority_player = actor;
        state.step = crate::state::Step::Main1;
        let source = put(&mut state, actor, "Zul Ashur, Lich Lord", Zone::Battlefield);
        let target = put(&mut state, actor, "Diregraf Ghoul", Zone::Graveyard);
        let changeling = put(&mut state, actor, "Webweaver Changeling", Zone::Graveyard);
        let foreign = put(
            &mut state,
            actor.opponent(),
            "Diregraf Ghoul",
            Zone::Graveyard,
        );
        let nonzombie = put(&mut state, actor, "Llanowar Elves", Zone::Graveyard);
        let land = put(&mut state, actor, "Forest", Zone::Graveyard);
        engine::step(&mut state, Action::ActivateAbility(source, 0)).unwrap();
        let Decision::ChooseTargets { legal_targets, .. } = next(&mut state) else {
            panic!("actual target choice")
        };
        assert!(legal_targets.contains(&Target::Object(target)));
        assert!(legal_targets.contains(&Target::Object(changeling)));
        for id in [foreign, nonzombie, land] {
            assert!(!legal_targets.contains(&Target::Object(id)));
        }
        let bytes = serde_json::to_vec(&state).unwrap();
        assert!(engine::step(&mut state, Action::ChooseTarget(Target::Object(foreign))).is_err());
        assert_eq!(bytes, serde_json::to_vec(&state).unwrap());
        engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
        let _ = next(&mut state);
        assert!(state.objects.get(source).tapped);
        (state, source, target)
    }
    fn settle(state: &mut GameState) {
        for _ in 0..40 {
            let d = next(state);
            if state.stack.is_empty() {
                return;
            }
            assert!(matches!(d, Decision::CastSpellOrPass { .. }), "{d:?}");
            engine::step(state, Action::Pass).unwrap();
        }
        panic!("resolution");
    }
    #[test]

    fn actual_targets_payment_original_beneficiary_departed_source_normal_cast_and_restoration_both_seats(
    ) {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (mut state, source, target) = setup(actor);
            crate::event::propose_and_commit(
                &mut state,
                crate::event::ProposedEvent::zone_change(source, Zone::Hand),
            );
            let mut replay: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            for s in [&mut state, &mut replay] {
                settle(s);
                let generation = s.objects.get(target).zone_change_count;
                assert!(permission(s, target, actor, generation));
                assert!(!permission(s, target, actor.opponent(), generation));
                s.priority_player = actor;
                s.players[actor.index()].mana_pool.fill(0);
                let bytes = serde_json::to_vec(s).unwrap();
                assert!(engine::step(s, Action::CastSpell(target)).is_err());
                assert_eq!(bytes, serde_json::to_vec(s).unwrap());
                s.players[actor.index()].mana_pool[crate::mana::ManaColor::B.pool_index()] = 1;
                s.players[actor.index()].mana_pool[crate::mana::ManaColor::C.pool_index()] = 2;
                engine::step(s, Action::CastSpell(target)).unwrap();
                let _ = next(s);
                assert_eq!(s.objects.get(target).zone, Zone::Stack);
                settle(s);
                assert_eq!(s.objects.get(target).zone, Zone::Battlefield);
                crate::event::propose_and_commit(
                    s,
                    crate::event::ProposedEvent::zone_change(target, Zone::Graveyard),
                );
                assert!(!permission(
                    s,
                    target,
                    actor,
                    s.objects.get(target).zone_change_count
                ));
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&replay).unwrap()
            );
        }
    }
    #[test]

    fn actual_owner_erasure_counter_target_blink_cleanup_and_meaning_forgeries_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (state, _, target) = setup(actor);
            let item = state.stack.last().unwrap().clone();
            let mut bad: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            bad.engine
                .event_history
                .retain(|e| !matches!(e, CommittedEvent::ZulAshurPublishedV1 { .. }));
            let bytes = serde_json::to_vec(&bad).unwrap();
            assert!(engine::step(&mut bad, Action::Pass).is_err());
            assert_eq!(bytes, serde_json::to_vec(&bad).unwrap());
            let mut countered = state.clone();
            engine::counter_stack_item_by_id(&mut countered, item.v4.stack_item_id)
                .unwrap()
                .unwrap();
            validate_state(&countered).unwrap();
            assert!(!has_active(&countered));
            let mut stale = state.clone();
            for zone in [Zone::Hand, Zone::Graveyard] {
                crate::event::propose_and_commit(
                    &mut stale,
                    crate::event::ProposedEvent::zone_change(target, zone),
                );
            }
            settle(&mut stale);
            assert!(!has_active(&stale));
            let mut resolved = state;
            settle(&mut resolved);
            assert!(has_active(&resolved));
            resolved.step = crate::state::Step::End;
            for _ in 0..40 {
                let d = next(&mut resolved);
                if resolved.active_player != actor {
                    break;
                }
                assert!(matches!(d, Decision::CastSpellOrPass { .. }), "{d:?}");
                engine::step(&mut resolved, Action::Pass).unwrap();
            }
            assert_eq!(resolved.active_player, actor.opponent());
            assert!(!has_active(&resolved));
            validate_state(&resolved).unwrap();
            let selector = crate::rules_meaning_zul_ashur_v1::live_selector().unwrap();
            let id = crate::card_def::card_id_by_name("Zul Ashur, Lich Lord").unwrap();
            for root in [
                crate::rules_meaning_zul_ashur_v1::RootV1::PrintedCreature,
                crate::rules_meaning_zul_ashur_v1::RootV1::WardPayTwoLife,
                crate::rules_meaning_zul_ashur_v1::RootV1::PrintedActivation,
            ] {
                let mut meaning =
                    crate::rules_meaning_zul_ashur_v1::meaning(&selector, id, root).unwrap();
                crate::rules_meaning_zul_ashur_v1::validate(&meaning).unwrap();
                meaning.rule.ordinary_timing_and_all_costs = false;
                assert!(crate::rules_meaning_zul_ashur_v1::validate(&meaning).is_err());
            }
        }
    }

    #[test]

    fn actual_printed_cast_life_ward_pay_decline_and_restore_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            for pay in [false, true] {
                let mut state = ready_state();
                state.active_player = actor;
                state.priority_player = actor;
                state.step = crate::state::Step::Main1;
                let source = put(&mut state, actor, "Zul Ashur, Lich Lord", Zone::Hand);
                state.players[actor.index()].mana_pool[crate::mana::ManaColor::B.pool_index()] = 1;
                state.players[actor.index()].mana_pool[crate::mana::ManaColor::C.pool_index()] = 1;
                engine::step(&mut state, Action::CastSpell(source)).unwrap();
                let _ = next(&mut state);
                settle(&mut state);
                assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
                assert!(state.objects.get(source).summoning_sick);
                let caster = actor.opponent();
                let bolt = put(&mut state, caster, "Lightning Bolt", Zone::Hand);
                state.priority_player = caster;
                state.players[caster.index()].mana_pool[crate::mana::ManaColor::R.pool_index()] = 1;
                engine::step(&mut state, Action::CastSpell(bolt)).unwrap();
                engine::step(&mut state, Action::ChooseTarget(Target::Object(source))).unwrap();
                let _ = next(&mut state);
                for _ in 0..20 {
                    match next(&mut state) {
                        Decision::ChooseEffectBoolean { player, .. } => {
                            assert_eq!(player, caster);
                            break;
                        }
                        Decision::CastSpellOrPass { .. } => {
                            engine::step(&mut state, Action::Pass).unwrap();
                        }
                        other => panic!("actual life Ward {other:?}"),
                    }
                }
                assert!(matches!(
                    next(&mut state),
                    Decision::ChooseEffectBoolean { .. }
                ));
                let life = state.players[caster.index()].life;
                let mut restored: GameState =
                    serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
                for branch in [&mut state, &mut restored] {
                    engine::step(branch, Action::ChooseEffectBoolean(pay)).unwrap();
                    settle(branch);
                    assert_eq!(
                        branch.players[caster.index()].life,
                        life - if pay { 2 } else { 0 }
                    );
                    assert_eq!(
                        branch.objects.get(source).zone,
                        if pay {
                            Zone::Graveyard
                        } else {
                            Zone::Battlefield
                        }
                    );
                    assert_eq!(branch.objects.get(bolt).zone, Zone::Graveyard);
                    assert!(!has_active(branch));
                    validate_state(branch).unwrap();
                }
                assert_eq!(
                    serde_json::to_vec(&state).unwrap(),
                    serde_json::to_vec(&restored).unwrap()
                );
            }
        }
    }

    #[test]

    fn actual_original_holder_after_typed_control_change_offturn_timing_and_expiry_forgery_both_seats(
    ) {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (mut state, source, target) = setup(actor);
            let donor = put(&mut state, actor.opponent(), "Lightning Bolt", Zone::Hand);
            let mut ctx = crate::effect::ExecCtx::no_targets(donor, actor.opponent());
            ctx.targets.push(Target::Object(source));
            ctx.target_contracts.push(StackTargetContractV4::capture(
                &state,
                Target::Object(source),
            ));
            // Existing typed control primitive setup; the Zul activation above was genuinely paid.
            crate::effect::execute(
                &EffectOp::GainControlOfTargetCreatureUntilEndOfTurn { target_index: 0 },
                &ctx,
                &mut state,
            );
            assert_eq!(state.objects.get(source).controller, actor.opponent());
            let mut restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            for branch in [&mut state, &mut restored] {
                settle(branch);
                let generation = branch.objects.get(target).zone_change_count;
                assert!(permission(branch, target, actor, generation));
                assert!(!permission(branch, target, actor.opponent(), generation));
                let installation = installations(branch).next().unwrap().index;
                let mut bad: GameState =
                    serde_json::from_slice(&serde_json::to_vec(branch).unwrap()).unwrap();
                bad.engine
                    .event_history
                    .push(CommittedEvent::ZulAshurExpiredV1 {
                        expiry: ExpiryV1 {
                            installation,
                            cleanup: bad.engine.event_history.len() as u32,
                        },
                    });
                let bytes = serde_json::to_vec(&bad).unwrap();
                assert!(engine::step(&mut bad, Action::Pass).is_err());
                assert_eq!(bytes, serde_json::to_vec(&bad).unwrap());
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&restored).unwrap()
            );

            let mut offturn = ready_state();
            offturn.active_player = actor.opponent();
            offturn.priority_player = actor;
            offturn.step = crate::state::Step::Main1;
            let source = put(
                &mut offturn,
                actor,
                "Zul Ashur, Lich Lord",
                Zone::Battlefield,
            );
            let target = put(&mut offturn, actor, "Diregraf Ghoul", Zone::Graveyard);
            engine::step(&mut offturn, Action::ActivateAbility(source, 0)).unwrap();
            let _ = next(&mut offturn);
            engine::step(&mut offturn, Action::ChooseTarget(Target::Object(target))).unwrap();
            let _ = next(&mut offturn);
            settle(&mut offturn);
            assert!(permission(
                &offturn,
                target,
                actor,
                offturn.objects.get(target).zone_change_count
            ));
            offturn.priority_player = actor;
            offturn.players[actor.index()].mana_pool[crate::mana::ManaColor::B.pool_index()] = 1;
            let bytes = serde_json::to_vec(&offturn).unwrap();
            assert!(engine::step(&mut offturn, Action::CastSpell(target)).is_err());
            assert_eq!(bytes, serde_json::to_vec(&offturn).unwrap());
            offturn.step = crate::state::Step::End;
            for _ in 0..40 {
                let d = next(&mut offturn);
                if offturn.active_player == actor {
                    break;
                }
                assert!(matches!(d, Decision::CastSpellOrPass { .. }), "{d:?}");
                engine::step(&mut offturn, Action::Pass).unwrap();
            }
            assert_eq!(offturn.active_player, actor);
            assert!(!has_active(&offturn));
        }
    }

    fn readonly_common_refusal(mut bad: GameState) {
        let saved = serde_json::to_vec(&bad).unwrap();
        let mut replay: GameState = serde_json::from_slice(&saved).unwrap();
        for branch in [&mut bad, &mut replay] {
            let before = serde_json::to_vec(branch).unwrap();
            assert!(validate_state(branch).is_err());
            assert!(engine::step(branch, Action::Pass).is_err());
            assert_eq!(before, serde_json::to_vec(branch).unwrap());
            assert!(crate::zul_ashur_observation_v1::observe_v1(
                branch,
                &crate::policy_surface_v5::PolicySurfaceV5::new_with_engine_priority_v1(),
                branch.priority_player,
                0,
                0,
                0,
                1
            )
            .is_err());
            assert_eq!(before, serde_json::to_vec(branch).unwrap());
            let mut expected: GameState = serde_json::from_slice(&before).unwrap();
            assert!(matches!(
                engine::advance_until_decision(branch),
                Decision::Halted { .. }
            ));
            assert!(matches!(
                branch.engine.halted,
                Some((
                    crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                    _
                ))
            ));
            expected.engine.halted = branch.engine.halted;
            assert_eq!(
                serde_json::to_vec(&expected).unwrap(),
                serde_json::to_vec(branch).unwrap()
            );
        }
        assert_eq!(
            serde_json::to_vec(&bad).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
    }
    #[test]
    #[ignore = "Zul Ashur is unregistered; genuine removal input and restore forgery qualification remains root-owned"]
    fn actual_original_journals_cannot_be_closed_by_manufactured_counter_or_fizzle_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (state, _, target) = setup(actor);
            let item = state.stack.last().unwrap().clone();
            let publication = published(&state, item.v4.stack_item_id).unwrap().index;
            for outcome in [OutcomeV1::Countered, OutcomeV1::NoLegalTarget] {
                let mut forged = state.clone();
                let original_journal = forged.engine.event_history.clone();
                forged
                    .stack
                    .retain(|i| i.v4.stack_item_id != item.v4.stack_item_id);
                let at = forged.engine.event_history.len() as u32;
                forged
                    .engine
                    .event_history
                    .push(CommittedEvent::ZulAshurRemovedV1 {
                        removal: Box::new(RemovalV1 {
                            index: at,
                            input: at - 1,
                            publication,
                            item: Box::new(item.clone()),
                            outcome,
                            target_legal: outcome == OutcomeV1::Countered,
                            turn: forged.turn_key_v1(),
                            active_player: forged.active_player,
                        }),
                    });
                assert_eq!(
                    original_journal,
                    forged.engine.event_history[..original_journal.len()]
                );
                readonly_common_refusal(forged);
            }
            let mut actual_counter = state.clone();
            engine::counter_stack_item_by_id(&mut actual_counter, item.v4.stack_item_id)
                .unwrap()
                .unwrap();
            validate_state(&actual_counter).unwrap();
            assert!(!has_active(&actual_counter));
            let input_at = actual_counter.engine.event_history.iter().position(|e|matches!(e,
                CommittedEvent::ZulAshurRemovalInputV1 { input } if input.publication == publication)).unwrap();
            let mut erased = actual_counter.clone();
            erased.engine.event_history[input_at] = CommittedEvent::Tap {
                object: item.source,
            };
            readonly_common_refusal(erased);
            let mut changed = actual_counter.clone();
            if let CommittedEvent::ZulAshurRemovalInputV1 { input } =
                &mut changed.engine.event_history[input_at]
            {
                input.outcome = OutcomeV1::NoLegalTarget;
                input.target_legal = false;
            } else {
                panic!("actual input");
            }
            if let Some(CommittedEvent::ZulAshurRemovedV1 { removal }) =
                changed.engine.event_history.get_mut(input_at + 1)
            {
                removal.outcome = OutcomeV1::NoLegalTarget;
                removal.target_legal = false;
            } else {
                panic!("actual removal");
            }
            readonly_common_refusal(changed);
            let mut stale = state;
            for zone in [Zone::Hand, Zone::Graveyard] {
                crate::event::propose_and_commit(
                    &mut stale,
                    crate::event::ProposedEvent::zone_change(target, zone),
                );
            }
            let mut restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(&stale).unwrap()).unwrap();
            for branch in [&mut stale, &mut restored] {
                settle(branch);
                validate_state(branch).unwrap();
                assert!(!has_active(branch));
                assert!(branch.engine.event_history.iter().any(|e|matches!(e,
                    CommittedEvent::ZulAshurRemovalInputV1 { input } if input.publication == publication
                        && input.outcome == OutcomeV1::NoLegalTarget && !input.target_legal)));
            }
            assert_eq!(
                serde_json::to_vec(&stale).unwrap(),
                serde_json::to_vec(&restored).unwrap()
            );
        }
    }
}
