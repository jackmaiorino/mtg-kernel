//! Registered absence operands, not Crystal admission.
use super::*;
use crate::policy_observation_v6::tests::{put, ready_state};
#[test]
fn absent_crystal_keeps_registered_shield_damage_and_restore_unchanged() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state();
        let source = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        let target = put(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        state.engine.active_replacements.push(ActiveReplacement {
            id: 1,
            source: target,
            kind: ReplacementEffectKind::PreventNextDamage {
                target: Target::Object(target),
                remaining: 2,
            },
        });
        let bytes = serde_json::to_vec(&state).unwrap();
        assert!(!requires_successor(&state));
        validate(&state).unwrap();
        assert_eq!(serde_json::to_vec(&state).unwrap(), bytes);
        let mut replay: GameState = serde_json::from_slice(&bytes).unwrap();
        for current in [&mut state, &mut replay] {
            crate::event::propose_and_commit(
                current,
                crate::event::ProposedEvent::damage(source, Target::Object(target), 3),
            );
            assert_eq!(current.objects.get(target).damage, 1);
            assert!(current.engine.active_replacements.is_empty());
            assert!(!requires_successor(current));
            validate(current).unwrap();
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
    }
}
#[test]
fn nondamage_classifier_requires_every_branch_and_never_guesses_unknown_leaves() {
    let draw = EffectOp::DrawCards {
        player: PlayerRef::Controller,
        count: 1,
    };
    assert!(ordinary_nondamage(&draw));
    assert!(ordinary_nondamage(&EffectOp::Sequence(vec![draw.clone()])));
    assert!(ordinary_nondamage(&EffectOp::Choice {
        controller: PlayerRef::Controller,
        options: vec![draw.clone()]
    }));
    assert!(!ordinary_nondamage(&EffectOp::Sequence(vec![
        draw,
        EffectOp::DealDamage {
            target: TargetRef::Opponent,
            amount: 3
        }
    ])));
    assert!(!ordinary_nondamage(&EffectOp::StandardV1(
        crate::standard_cards_v1::StandardOpV1::SourceLastPowerDamageV1
    )));
}
#[test]
fn payment_comparison_never_accepts_an_unpublished_crystal_marker_on_registered_item() {
    use crate::engine::{self, Action, Decision};
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state();
        state.active_player = actor;
        state.priority_player = actor;
        let bolt = put(&mut state, actor, "Lightning Bolt", Zone::Hand);
        state.players[actor.index()].mana_pool[crate::mana::ManaColor::R.pool_index()] = 1;
        engine::advance_until_decision(&mut state);
        engine::step(&mut state, Action::CastSpell(bolt)).unwrap();
        assert!(matches!(
            engine::advance_until_decision(&mut state),
            Decision::ChooseTargets { .. }
        ));
        engine::step(
            &mut state,
            Action::ChooseTarget(Target::Player(actor.opponent())),
        )
        .unwrap();
        engine::advance_until_decision(&mut state);
        let original = state
            .stack
            .iter()
            .find(|i| i.source == bolt)
            .unwrap()
            .clone();
        assert!(source_power_payment_matches(&state, &original, &original));
        let mut forged = original.clone();
        forged.v4.crystal_prevention_started_v1 = CrystalPreventionStartedV1(Some(0));
        let before = serde_json::to_vec(&state).unwrap();
        assert!(!source_power_payment_matches(&state, &forged, &original));
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
    }
}
#[test]
fn absent_stack_wrapper_writes_no_hash_input_or_serialized_field() {
    #[derive(Default)]
    struct Writes(usize);
    impl std::hash::Hasher for Writes {
        fn finish(&self) -> u64 {
            self.0 as u64
        }
        fn write(&mut self, bytes: &[u8]) {
            self.0 += bytes.len();
        }
    }
    let mut hash = Writes::default();
    std::hash::Hash::hash(&CrystalPreventionStartedV1::default(), &mut hash);
    assert_eq!(hash.0, 0);
    let value = serde_json::to_value(crate::state::StackStateV4::default()).unwrap();
    assert!(value.get("crystal_prevention_started_v1").is_none());
    std::hash::Hash::hash(&CrystalPreventionStartedV1(Some(0)), &mut hash);
    assert!(hash.0 > 0);
}

#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn absent_crystal_real_incinerating_keeps_legacy_optional_cost_and_replay() {
    use crate::engine::{self, Action, Decision, OptionalCostChoice};
    use crate::mana::ManaColor;
    for actor in [PlayerId::P0, PlayerId::P1] {
        for pay in [false, true] {
            let mut state = ready_state();
            state.active_player = actor;
            state.priority_player = actor;
            let target = put(
                &mut state,
                actor.opponent(),
                "Llanowar Elves",
                Zone::Battlefield,
            );
            let card = put(&mut state, actor, "Forest", Zone::Hand);
            let spell = put(&mut state, actor, "Incinerating Blast", Zone::Hand);
            state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 1;
            state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 4;
            assert!(
                matches!(engine::advance_until_decision(&mut state),Decision::CastSpellOrPass {player,..} if player==actor)
            );
            engine::step(&mut state, Action::CastSpell(spell)).unwrap();
            assert!(matches!(
                engine::advance_until_decision(&mut state),
                Decision::ChooseTargets { .. }
            ));
            engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
            engine::advance_until_decision(&mut state);
            for _ in 0..10 {
                let d = engine::advance_until_decision(&mut state);
                if matches!(d,Decision::ChooseOptionalCost {player,..} if player==actor) {
                    break;
                }
                assert!(matches!(d, Decision::CastSpellOrPass { .. }), "{d:?}");
                engine::step(&mut state, Action::Pass).unwrap();
            }
            assert!(
                matches!(engine::advance_until_decision(&mut state),Decision::ChooseOptionalCost {player,discard_payable:true,..}
                if player==actor)
            );
            assert!(state.engine.pending_effect.is_none());
            assert!(!requires_successor(&state));
            let bytes = serde_json::to_vec(&state).unwrap();
            let mut replay: GameState = serde_json::from_slice(&bytes).unwrap();
            for current in [&mut state, &mut replay] {
                engine::step(
                    current,
                    Action::ChooseOptionalCost(if pay {
                        OptionalCostChoice::Discard
                    } else {
                        OptionalCostChoice::Decline
                    }),
                )
                .unwrap();
                if pay {
                    assert!(matches!(
                        engine::advance_until_decision(current),
                        Decision::Discard { .. }
                    ));
                    engine::step(current, Action::Discard(vec![card])).unwrap();
                }
                for _ in 0..15 {
                    let d = engine::advance_until_decision(current);
                    if current.stack.is_empty() && current.engine.pending_triggers.is_empty() {
                        break;
                    }
                    assert!(matches!(d, Decision::CastSpellOrPass { .. }), "{d:?}");
                    engine::step(current, Action::Pass).unwrap();
                }
                assert_eq!(current.objects.get(spell).zone, Zone::Graveyard);
                assert_eq!(
                    current.objects.get(card).zone,
                    if pay { Zone::Graveyard } else { Zone::Hand }
                );
                assert!(!requires_successor(current));
                validate(current).unwrap();
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&replay).unwrap()
            );
        }
    }
}

#[test]
fn typed_nondamage_effects_preserve_their_own_dispatch_and_reject_nested_damage() {
    let safe = vec![
        EffectOp::MoveObject {
            object: ObjectRef::ThisSource,
            to_zone: Zone::Battlefield,
        },
        EffectOp::CreateToken {
            token_def: 0,
            controller: PlayerRef::Controller,
        },
        EffectOp::PutPlusOnePlusOneCounter {
            object: ObjectRef::Target(0),
        },
        EffectOp::DestroyObject {
            object: ObjectRef::Target(0),
        },
        EffectOp::GrantKeywordTargetUntilEndOfTurn {
            object: ObjectRef::Target(0),
            keyword: crate::card_def::Keywords::INDESTRUCTIBLE,
        },
    ];
    assert!(ordinary_nondamage(&EffectOp::Sequence(safe.clone())));
    for op in safe {
        assert!(ordinary_nondamage(&op));
        assert!(!ordinary_nondamage(&EffectOp::Choice {
            controller: PlayerRef::Controller,
            options: vec![
                op,
                EffectOp::DealDamage {
                    target: TargetRef::Opponent,
                    amount: 1
                }
            ],
        }));
    }
    assert!(!ordinary_nondamage(&EffectOp::MayPayManaThen {
        player: PlayerRef::Controller,
        colored: vec![],
        generic: 0,
        then: Box::new(EffectOp::DealDamage {
            target: TargetRef::Opponent,
            amount: 1
        }),
    }));
    assert!(!ordinary_nondamage(&EffectOp::BindAuthorityEntryGainLifeV1));
}

#[test]
fn registered_fight_primitive_without_crystal_keeps_generic_batch_and_restore() {
    let op = EffectOp::FightObjects {
        first: ObjectRef::Target(0),
        second: ObjectRef::Target(1),
        target_spec: crate::card_def::TargetSpec::ControlledCreatureThenOpponentCreature,
    };
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state();
        let spell = put(&mut state, actor, "Lightning Bolt", Zone::Hand);
        let first = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        let second = put(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        let mut ctx = ExecCtx::no_targets(spell, actor);
        ctx.targets = vec![Target::Object(first), Target::Object(second)];
        ctx.target_contracts = ctx
            .targets
            .iter()
            .map(|target| crate::state::StackTargetContractV4::capture(&state, *target))
            .collect();
        let bytes = serde_json::to_vec(&state).unwrap();
        let mut replay: GameState = serde_json::from_slice(&bytes).unwrap();
        for current in [&mut state, &mut replay] {
            assert!(!requires_successor(current));
            crate::effect::execute(&op, &ctx, current);
            assert_eq!(current.objects.get(first).damage, 1);
            assert_eq!(current.objects.get(second).damage, 1);
            assert!(!current.engine.event_history.iter().any(|event| matches!(
                event,
                CommittedEvent::CrystalBatchPreparedV3 { .. }
                    | CommittedEvent::CrystalExecutionV1 { .. }
            )));
            validate(current).unwrap();
            assert!(!requires_successor(current));
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
        assert_eq!(state.state_hash(), replay.state_hash());
    }
}
