use super::*;
use crate::policy_observation_v6::tests::ready_state_for;

#[test]
fn registered_absence_preserves_proposal_and_state_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let state = ready_state_for(actor);
        let before = serde_json::to_vec(&state).unwrap();
        let mut damage =
            match ProposedEvent::damage(ObjectId(0), Target::Player(actor.opponent()), 3) {
                ProposedEvent::Damage(d) => d,
                _ => unreachable!(),
            };
        let original = damage.clone();
        replace(&state, &mut damage).unwrap();
        assert_eq!(damage, original);
        assert!(!requires_successor(&state));
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
    }
}

#[test]
fn absent_combat_successor_does_not_publish_or_mutate_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state_for(actor);
        let before = serde_json::to_vec(&state).unwrap();
        let hash = state.state_hash();
        assert!(!combat::begin(
            &mut state,
            &[ProposedEvent::damage(
                ObjectId(0),
                Target::Player(actor.opponent()),
                3
            )],
            false
        )
        .unwrap());
        combat::validate(&state).unwrap();
        assert!(combat::decision(&state).unwrap().is_none());
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        assert_eq!(state.state_hash(), hash);
    }
}

#[test]
fn absent_applied_multiplier_wrapper_contributes_no_hash_bytes() {
    use std::hash::{Hash, Hasher};
    #[derive(Default)]
    struct Bytes(Vec<u8>);
    impl Hasher for Bytes {
        fn finish(&self) -> u64 {
            0
        }
        fn write(&mut self, b: &[u8]) {
            self.0.extend_from_slice(b);
        }
    }
    let mut h = Bytes::default();
    ordering::AppliedMultipliersV38::default().hash(&mut h);
    ordering::PreparedBatchOptionsV38::default().hash(&mut h);
    combat::StateV38::default().hash(&mut h);
    assert!(h.0.is_empty());
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod future_printed {
    use super::*;
    use crate::engine::{self, Action, Decision};
    use crate::event::{self, CommittedEvent};
    use crate::mana::ManaColor;
    use crate::policy_observation_v6::tests::put;
    fn ready(actor: PlayerId) -> GameState {
        let mut state = ready_state_for(actor);
        for player in [actor, actor.opponent()] {
            for _ in 0..12 {
                put(&mut state, player, "Forest", Zone::Library);
            }
        }
        state
    }
    fn finish(state: &mut GameState) {
        for _ in 0..20 {
            if state.stack.is_empty() && state.engine.pending_cast.is_none() {
                return;
            }
            let decision = engine::advance_until_decision(state);
            assert!(
                matches!(decision, Decision::CastSpellOrPass { .. }),
                "{decision:?}"
            );
            if state.stack.is_empty() && state.engine.pending_cast.is_none() {
                return;
            }
            engine::step(state, Action::Pass).unwrap();
        }
        panic!("actual stack did not finish")
    }
    fn cast(state: &mut GameState, actor: PlayerId) -> ObjectId {
        let source = put(state, actor, "Twinflame Tyrant", Zone::Hand);
        assert!(owns_definition(state.objects.get(source).card_def));
        let before = serde_json::to_vec(state).unwrap();
        assert!(engine::step(state, Action::CastSpell(source)).is_err());
        assert_eq!(serde_json::to_vec(state).unwrap(), before);
        state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 3;
        state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 2;
        engine::step(state, Action::CastSpell(source)).unwrap();
        finish(state);
        assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
        source
    }
    fn bolt(state: &mut GameState, actor: PlayerId, target: Target) {
        let source = put(state, actor, "Lightning Bolt", Zone::Hand);
        state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 1;
        engine::step(state, Action::CastSpell(source)).unwrap();
        engine::step(state, Action::ChooseTarget(target)).unwrap();
        finish(state);
        assert_eq!(state.objects.get(source).zone, Zone::Graveyard);
    }
    #[test]
    #[ignore = "true Twinflame printing remains unregistered"]
    fn actual_cast_player_damage_multiple_live_sources_and_restore_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready(actor);
            let tyrant = cast(&mut state, actor);
            let baseline = state.players[actor.opponent().index()].life;
            let saved = serde_json::to_vec(&state).unwrap();
            let mut restored = serde_json::from_slice(&saved).unwrap();
            for branch in [&mut state, &mut restored] {
                bolt(branch, actor, Target::Player(actor.opponent()));
                assert_eq!(branch.players[actor.opponent().index()].life, baseline - 6);
                assert!(crate::rl::observe_v1(branch, actor, 0).is_err());
                assert!(crate::rl::observe_v2(
                    branch,
                    &crate::surface_v2::HarnessSurfaceV2::new(),
                    actor,
                    0
                )
                .is_err());
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&restored).unwrap()
            );
            cast(&mut state, actor);
            let before = state.players[actor.opponent().index()].life;
            bolt(&mut state, actor, Target::Player(actor.opponent()));
            assert_eq!(state.players[actor.opponent().index()].life, before - 12);
            assert_eq!(controller(&state, tyrant), Some(actor));
        }
    }
    #[test]
    #[ignore = "true Twinflame printing remains unregistered"]
    fn actual_damage_recipient_control_zero_combat_and_consumable_order_refusal_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready(actor);
            cast(&mut state, actor);
            let own = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
            let enemy = put(
                &mut state,
                actor.opponent(),
                "Shivan Dragon",
                Zone::Battlefield,
            );
            // Typed accepted combat packet, not a claimed attack declaration.
            let mut combat = match ProposedEvent::damage(own, Target::Object(enemy), 1) {
                ProposedEvent::Damage(d) => d,
                _ => unreachable!(),
            };
            combat.is_combat = true;
            event::propose_and_commit_batch(&mut state, vec![ProposedEvent::Damage(combat)]);
            assert_eq!(state.objects.get(enemy).damage, 2);
            event::propose_and_commit(
                &mut state,
                ProposedEvent::damage(own, Target::Object(own), 1),
            );
            assert_eq!(state.objects.get(own).damage, 1);
            let before = state.engine.event_history.len();
            event::propose_and_commit(
                &mut state,
                ProposedEvent::damage(own, Target::Object(enemy), 0),
            );
            assert!(!state.engine.event_history[before..]
                .iter()
                .any(|e| matches!(e, CommittedEvent::Damage{ amount, .. } if *amount != 0)));
            state
                .engine
                .active_replacements
                .push(event::ActiveReplacement {
                    id: 90_038,
                    source: enemy,
                    kind: event::ReplacementEffectKind::PreventNextDamage {
                        target: Target::Object(enemy),
                        remaining: 2,
                    },
                });
            let before = serde_json::to_vec(&state).unwrap();
            let proposal = ProposedEvent::damage(own, Target::Object(enemy), 3);
            assert!(event::apply_replacements(&mut state, proposal).is_none());
            let halted = state.engine.halted.take();
            assert!(halted.is_some());
            assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        }
    }
    #[test]
    #[ignore = "true Twinflame printing remains unregistered"]
    fn genuine_bolt_owned_scalar_order_has_zero_or_three_damage_and_retained_input_proof_both_seats(
    ) {
        use crate::effect::crystal_prevention_v1::PreventionV1;
        use crate::effect::{EffectOptionChoicePurpose, PendingEffectChoice};
        for actor in [PlayerId::P0, PlayerId::P1] {
            for double_first in [false, true] {
                let mut state = ready(actor);
                cast(&mut state, actor);
                let recipient = put(
                    &mut state,
                    actor.opponent(),
                    "Shivan Dragon",
                    Zone::Battlefield,
                );
                // Genuine existing numeric shield primitive; no counterfeit spell producer.
                state
                    .engine
                    .active_replacements
                    .push(event::ActiveReplacement {
                        id: 90_039,
                        source: recipient,
                        kind: event::ReplacementEffectKind::PreventNextDamage {
                            target: Target::Object(recipient),
                            remaining: 3,
                        },
                    });
                let spell = put(&mut state, actor, "Lightning Bolt", Zone::Hand);
                state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 1;
                engine::step(&mut state, Action::CastSpell(spell)).unwrap();
                engine::step(&mut state, Action::ChooseTarget(Target::Object(recipient))).unwrap();
                for _ in 0..12 {
                    if state.engine.pending_effect.as_ref().is_some_and(|p| {
                        matches!(
                            p.choice,
                            Some(PendingEffectChoice::ChooseOption {
                                purpose: EffectOptionChoicePurpose::CrystalPreventionV1 { .. },
                                ..
                            })
                        )
                    }) {
                        break;
                    }
                    assert!(matches!(
                        engine::advance_until_decision(&mut state),
                        Decision::CastSpellOrPass { .. }
                    ));
                    engine::step(&mut state, Action::Pass).unwrap();
                }
                let bytes = serde_json::to_vec(&state).unwrap();
                let mut restored: GameState = serde_json::from_slice(&bytes).unwrap();
                for branch in [&mut state, &mut restored] {
                    let pending = branch.engine.pending_effect.as_ref().unwrap();
                    let Some(PendingEffectChoice::ChooseOption {
                        player,
                        purpose: EffectOptionChoicePurpose::CrystalPreventionV1 { choice },
                        ..
                    }) = pending.choice.as_ref()
                    else {
                        panic!("actual affected-player order")
                    };
                    assert_eq!(*player, actor.opponent());
                    let index = choice
                        .options
                        .iter()
                        .position(|o| {
                            if double_first {
                                matches!(o, PreventionV1::Multiplier(_))
                            } else {
                                matches!(o, PreventionV1::Replacement(_))
                            }
                        })
                        .unwrap();
                    engine::step(branch, Action::ChooseEffectOption(index as u16)).unwrap();
                    if double_first {
                        let Some(PendingEffectChoice::ChooseOption {
                            purpose: EffectOptionChoicePurpose::CrystalPreventionV1 { choice },
                            ..
                        }) = branch
                            .engine
                            .pending_effect
                            .as_ref()
                            .unwrap()
                            .choice
                            .as_ref()
                        else {
                            panic!("real remaining shield")
                        };
                        assert_eq!(choice.remaining_amount, 6);
                        assert_eq!(choice.applied_multipliers.0.len(), 1);
                        assert_eq!(choice.options.len(), 1);
                        engine::step(branch, Action::ChooseEffectOption(0)).unwrap();
                    }
                    finish(branch);
                    assert_eq!(
                        branch.objects.get(recipient).damage,
                        if double_first { 3 } else { 0 }
                    );
                    assert_eq!(branch.objects.get(spell).zone, Zone::Graveyard);
                    crate::effect::validate_pending_effect_choice(branch).unwrap();
                }
                assert_eq!(
                    serde_json::to_vec(&state).unwrap(),
                    serde_json::to_vec(&restored).unwrap()
                );
                for input in [true, false] {
                    let mut bad: GameState =
                        serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
                    let index = bad
                        .engine
                        .event_history
                        .iter()
                        .position(|e| {
                            if input {
                                matches!(e, CommittedEvent::TwinflameReplacementInputV38 { .. })
                            } else {
                                matches!(e, CommittedEvent::TwinflameReplacementOperationV38 { .. })
                            }
                        })
                        .unwrap();
                    bad.engine.event_history[index] = CommittedEvent::LifeGain {
                        player: actor,
                        amount: 0,
                    };
                    let before = serde_json::to_vec(&bad).unwrap();
                    assert!(engine::step(&mut bad, Action::Pass).is_err());
                    assert_eq!(serde_json::to_vec(&bad).unwrap(), before);
                }
            }
        }
    }
    #[test]
    #[ignore = "true Twinflame printing remains unregistered"]
    fn genuine_bushwhack_batch_orders_both_players_before_damage_and_binds_outcomes_both_seats() {
        use crate::effect::crystal_prevention_v1::PreventionV1;
        use crate::effect::{EffectOptionChoicePurpose, PendingEffectChoice};
        for actor in [PlayerId::P0, PlayerId::P1] {
            for double_first in [false, true] {
                let mut state = ready(actor);
                cast(&mut state, actor);
                // The second active printing is a physical opposing static operand.
                // The actual spell and both fight participants use registered definitions.
                put(
                    &mut state,
                    actor.opponent(),
                    "Twinflame Tyrant",
                    Zone::Battlefield,
                );
                let fighters = [
                    put(&mut state, actor, "Shivan Dragon", Zone::Battlefield),
                    put(
                        &mut state,
                        actor.opponent(),
                        "Shivan Dragon",
                        Zone::Battlefield,
                    ),
                ];
                for (slot, fighter) in fighters.iter().enumerate() {
                    state.objects.get_mut(*fighter).v4.lifelink_keyword_counters = 1;
                    state
                        .engine
                        .active_replacements
                        .push(event::ActiveReplacement {
                            id: 90_040 + slot as u32,
                            source: *fighter,
                            kind: event::ReplacementEffectKind::PreventNextDamage {
                                target: Target::Object(*fighter),
                                remaining: 5,
                            },
                        });
                }
                let spell = put(&mut state, actor, "Bushwhack", Zone::Hand);
                state.players[actor.index()].mana_pool[ManaColor::G.pool_index()] = 1;
                engine::step(&mut state, Action::CastSpell(spell)).unwrap();
                assert!(matches!(
                    engine::advance_until_decision(&mut state),
                    Decision::ChooseSpellMode { .. }
                ));
                engine::step(&mut state, Action::ChooseSpellMode(1)).unwrap();
                for fighter in fighters {
                    assert!(
                        matches!(engine::advance_until_decision(&mut state), Decision::ChooseTargets { legal_targets, .. } if legal_targets.contains(&Target::Object(fighter)))
                    );
                    engine::step(&mut state, Action::ChooseTarget(Target::Object(fighter)))
                        .unwrap();
                }
                for _ in 0..12 {
                    if state.engine.pending_effect.as_ref().is_some_and(|p| {
                        matches!(
                            p.choice,
                            Some(PendingEffectChoice::ChooseOption {
                                purpose: EffectOptionChoicePurpose::CrystalPreventionV1 { .. },
                                ..
                            })
                        )
                    }) {
                        break;
                    }
                    assert!(matches!(
                        engine::advance_until_decision(&mut state),
                        Decision::CastSpellOrPass { .. }
                    ));
                    engine::step(&mut state, Action::Pass).unwrap();
                }
                let lives = state.players.each_ref().map(|p| p.life);
                let before = serde_json::to_vec(&state).unwrap();
                let mut replay: GameState = serde_json::from_slice(&before).unwrap();
                for branch in [&mut state, &mut replay] {
                    for chooser in [actor, actor.opponent()] {
                        let Some(PendingEffectChoice::ChooseOption {
                            player,
                            purpose: EffectOptionChoicePurpose::CrystalPreventionV1 { choice },
                            ..
                        }) = branch
                            .engine
                            .pending_effect
                            .as_ref()
                            .unwrap()
                            .choice
                            .as_ref()
                        else {
                            panic!("real affected batch menu")
                        };
                        assert_eq!(*player, chooser);
                        assert!(fighters
                            .iter()
                            .all(|id| branch.objects.get(*id).damage == 0));
                        assert_eq!(branch.players.each_ref().map(|p| p.life), lives);
                        let option = choice
                            .options
                            .iter()
                            .position(|option| {
                                if double_first {
                                    matches!(option, PreventionV1::Multiplier(_))
                                } else {
                                    matches!(option, PreventionV1::Replacement(_))
                                }
                            })
                            .unwrap();
                        engine::step(branch, Action::ChooseEffectOption(option as u16)).unwrap();
                        if double_first {
                            engine::step(branch, Action::ChooseEffectOption(0)).unwrap();
                        }
                    }
                    finish(branch);
                    assert_eq!(branch.objects.get(spell).zone, Zone::Graveyard);
                    for fighter in fighters {
                        assert_eq!(
                            branch.objects.get(fighter).damage,
                            if double_first { 5 } else { 0 }
                        );
                        assert_eq!(
                            branch.objects.get(fighter).zone,
                            if double_first {
                                Zone::Graveyard
                            } else {
                                Zone::Battlefield
                            }
                        );
                    }
                    for player in [actor, actor.opponent()] {
                        assert_eq!(
                            branch.players[player.index()].life,
                            lives[player.index()] + if double_first { 5 } else { 0 }
                        );
                    }
                    crate::effect::validate_pending_effect_choice(branch).unwrap();
                }
                assert_eq!(
                    serde_json::to_vec(&state).unwrap(),
                    serde_json::to_vec(&replay).unwrap()
                );
                for kind in 0..3 {
                    let mut malformed: GameState =
                        serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
                    let at = malformed
                        .engine
                        .event_history
                        .iter()
                        .position(|event| match kind {
                            0 => matches!(event, CommittedEvent::CrystalBatchPreparedV3 { .. }),
                            1 => {
                                matches!(event, CommittedEvent::TwinflameReplacementInputV38 { .. })
                            }
                            _ => matches!(
                                event,
                                CommittedEvent::TwinflameReplacementOperationV38 { .. }
                            ),
                        })
                        .unwrap();
                    malformed.engine.event_history[at] = CommittedEvent::LifeGain {
                        player: actor,
                        amount: 0,
                    };
                    let bytes = serde_json::to_vec(&malformed).unwrap();
                    assert!(engine::step(&mut malformed, Action::Pass).is_err());
                    assert_eq!(serde_json::to_vec(&malformed).unwrap(), bytes);
                }
            }
        }
    }
    #[test]
    #[ignore = "true Twinflame printing remains unregistered"]
    fn actual_combat_shared_player_shield_orders_packets_before_simultaneous_damage_both_seats() {
        use crate::effect::crystal_prevention_v1::PreventionV1;
        for actor in [PlayerId::P0, PlayerId::P1] {
            for double_first in [false, true] {
                let mut state = ready(actor);
                let tyrant = cast(&mut state, actor);
                let dragon = put(&mut state, actor, "Shivan Dragon", Zone::Battlefield);
                // Labeled physical maturity and keyword-counter operands before actual declaration.
                for source in [tyrant, dragon] {
                    state.objects.get_mut(source).summoning_sick = false;
                    state.objects.get_mut(source).v4.lifelink_keyword_counters = 1;
                }
                state
                    .engine
                    .active_replacements
                    .push(event::ActiveReplacement {
                        id: 90_050,
                        source: dragon,
                        kind: event::ReplacementEffectKind::PreventNextDamage {
                            target: Target::Player(actor.opponent()),
                            remaining: 3,
                        },
                    });
                for _ in 0..24 {
                    let decision = engine::advance_until_decision(&mut state);
                    if matches!(decision, Decision::DeclareAttackers { .. }) {
                        break;
                    }
                    assert!(
                        matches!(decision, Decision::CastSpellOrPass { .. }),
                        "{decision:?}"
                    );
                    engine::step(&mut state, Action::Pass).unwrap();
                }
                engine::step(&mut state, Action::DeclareAttackers(vec![tyrant, dragon])).unwrap();
                let mut blocked = false;
                for _ in 0..24 {
                    let decision = engine::advance_until_decision(&mut state);
                    if combat::has_pending(&state) {
                        break;
                    }
                    if matches!(decision, Decision::DeclareBlockers { .. }) {
                        assert!(!blocked);
                        blocked = true;
                        engine::step(&mut state, Action::DeclareBlockers(Vec::new())).unwrap();
                    } else {
                        assert!(
                            matches!(decision, Decision::CastSpellOrPass { .. }),
                            "{decision:?}"
                        );
                        engine::step(&mut state, Action::Pass).unwrap();
                    }
                }
                assert!(blocked);
                assert!(
                    matches!(combat::decision(&state).unwrap(),Some(Decision::ChooseEffectOption {player,option_count:2,..}) if player==actor.opponent())
                );
                assert!(state.engine.pending_effect.is_none());
                assert!(state.stack.is_empty());
                let original = serde_json::to_vec(&state).unwrap();
                let saved_pending = state.engine.twinflame_combat_v38.clone();
                let lives = state.players.each_ref().map(|p| p.life);
                let mut replay: GameState = serde_json::from_slice(&original).unwrap();
                for branch in [&mut state, &mut replay] {
                    // Allocation first selects the genuine three-power Tyrant packet.
                    engine::step(branch, Action::ChooseEffectOption(0)).unwrap();
                    let pending = branch.engine.twinflame_combat_v38.0.as_ref().unwrap();
                    let packet = &pending.packets[pending.selected.unwrap()];
                    assert_eq!(packet.source, tyrant);
                    assert_eq!(packet.original_amount, 3);
                    assert_eq!(branch.players.each_ref().map(|p| p.life), lives);
                    let index = packet
                        .options
                        .iter()
                        .position(|option| {
                            if double_first {
                                matches!(option, PreventionV1::Multiplier(_))
                            } else {
                                matches!(option, PreventionV1::Replacement(_))
                            }
                        })
                        .unwrap();
                    engine::step(branch, Action::ChooseEffectOption(index as u16)).unwrap();
                    // Remaining single replacements execute under the same private wave owner.
                    assert!(!combat::has_pending(branch));
                    combat::validate(branch).unwrap();
                    let total = if double_first { 13 } else { 10 };
                    assert_eq!(
                        branch.players[actor.opponent().index()].life,
                        lives[actor.opponent().index()] - total
                    );
                    assert_eq!(
                        branch.players[actor.index()].life,
                        lives[actor.index()] + total
                    );
                    assert!(branch
                        .engine
                        .active_replacements
                        .iter()
                        .all(|r| r.id != 90_050));
                    assert_eq!(branch.step, crate::state::Step::CombatDamage);
                }
                assert_eq!(
                    serde_json::to_vec(&state).unwrap(),
                    serde_json::to_vec(&replay).unwrap()
                );
                for mutation in 0..4 {
                    let mut bad: GameState =
                        serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
                    if mutation == 0 {
                        bad.engine.twinflame_combat_v38 = saved_pending.clone();
                    } else {
                        let at = bad
                            .engine
                            .event_history
                            .iter()
                            .position(|event| match mutation {
                                1 => matches!(
                                    event,
                                    CommittedEvent::TwinflameCombatPublishedV38 { .. }
                                ),
                                2 => {
                                    matches!(event, CommittedEvent::TwinflameCombatInputV38 { .. })
                                }
                                _ => matches!(
                                    event,
                                    CommittedEvent::TwinflameCombatOperationV38 { .. }
                                ),
                            })
                            .unwrap();
                        bad.engine.event_history[at] = CommittedEvent::LifeGain {
                            player: actor,
                            amount: 0,
                        };
                    }
                    let bytes = serde_json::to_vec(&bad).unwrap();
                    assert!(engine::step(&mut bad, Action::Pass).is_err());
                    assert_eq!(serde_json::to_vec(&bad).unwrap(), bytes);
                }
                let mut bad: GameState = serde_json::from_slice(&original).unwrap();
                bad.engine
                    .active_replacements
                    .iter_mut()
                    .find(|r| r.id == 90_050)
                    .unwrap()
                    .kind = event::ReplacementEffectKind::PreventNextDamage {
                    target: Target::Player(actor.opponent()),
                    remaining: 99,
                };
                let bytes = serde_json::to_vec(&bad).unwrap();
                assert!(engine::step(&mut bad, Action::ChooseEffectOption(0)).is_err());
                assert_eq!(serde_json::to_vec(&bad).unwrap(), bytes);
            }
        }
    }
    #[test]
    #[ignore = "true Twinflame printing remains unregistered"]
    fn actual_gorehorn_raid_without_numeric_prevention_uses_owned_doubled_outcome_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready(actor);
            let tyrant = cast(&mut state, actor);
            for player in [actor, actor.opponent()] {
                put(&mut state, player, "Mountain", Zone::Battlefield);
                put(&mut state, player, "Lightning Bolt", Zone::Hand);
            }
            let attacker = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
            state.objects.get_mut(attacker).summoning_sick = false;
            for _ in 0..24 {
                let decision = engine::advance_until_decision(&mut state);
                if matches!(decision, Decision::DeclareAttackers { .. }) {
                    break;
                }
                assert!(
                    matches!(decision, Decision::CastSpellOrPass { .. }),
                    "{decision:?}"
                );
                engine::step(&mut state, Action::Pass).unwrap();
            }
            engine::step(&mut state, Action::DeclareAttackers(vec![attacker])).unwrap();
            for _ in 0..40 {
                let decision = engine::advance_until_decision(&mut state);
                if state.step == crate::state::Step::Main2
                    && matches!(decision, Decision::CastSpellOrPass { player, .. } if player == actor)
                {
                    break;
                }
                match decision {
                    Decision::CastSpellOrPass { .. } => {
                        engine::step(&mut state, Action::Pass).unwrap()
                    }
                    Decision::DeclareBlockers { .. } => {
                        engine::step(&mut state, Action::DeclareBlockers(vec![])).unwrap()
                    }
                    other => panic!("actual Raid traversal {other:?}"),
                }
            }
            assert_eq!(state.step, crate::state::Step::Main2);
            let raider = put(&mut state, actor, "Gorehorn Raider", Zone::Hand);
            state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 4;
            state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 1;
            engine::step(&mut state, Action::CastSpell(raider)).unwrap();
            for _ in 0..24 {
                let decision = engine::advance_until_decision(&mut state);
                if let Decision::ChooseTargets {
                    player,
                    legal_targets,
                    ..
                } = decision
                {
                    assert_eq!(player, actor);
                    assert!(legal_targets.contains(&Target::Player(actor.opponent())));
                    engine::step(
                        &mut state,
                        Action::ChooseTarget(Target::Player(actor.opponent())),
                    )
                    .unwrap();
                    break;
                }
                assert!(
                    matches!(decision, Decision::CastSpellOrPass { .. }),
                    "{decision:?}"
                );
                engine::step(&mut state, Action::Pass).unwrap();
            }
            let lives = state.players.each_ref().map(|p| p.life);
            let saved = serde_json::to_vec(&state).unwrap();
            let mut restored = serde_json::from_slice(&saved).unwrap();
            for branch in [&mut state, &mut restored] {
                finish(branch);
                assert_eq!(
                    branch.players[actor.opponent().index()].life,
                    lives[actor.opponent().index()] - 4
                );
                assert!(branch.engine.event_history.iter().any(|e| matches!(e,
                    CommittedEvent::TwinflameDamagePreparationV38 { preparation }
                        if preparation.input.source == raider && preparation.input.amount == 2)));
                crate::effect::crystal_prevention_v1::validate(branch).unwrap();
                crate::gorehorn_damage_v1::validate_state(branch).unwrap();
                assert_eq!(branch.objects.get(tyrant).zone, Zone::Battlefield);
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&restored).unwrap()
            );
        }
    }
    #[test]
    #[ignore = "true Twinflame printing remains unregistered"]
    fn actual_sure_strike_and_double_strike_have_two_owned_combat_waves_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            for foundations in [false, true] {
                for double_first in [false, true] {
                    let mut state = ready(actor);
                    let tyrant = cast(&mut state, actor);
                    let swiftblade = put(&mut state, actor, "Swiftblade Vindicator", Zone::Hand);
                    state.players[actor.index()].mana_pool[ManaColor::W.pool_index()] = 1;
                    state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 1;
                    engine::step(&mut state, Action::CastSpell(swiftblade)).unwrap();
                    finish(&mut state);
                    let strike = put(&mut state, actor, "Sure Strike", Zone::Hand);
                    state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 1;
                    state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 1;
                    engine::step(&mut state, Action::CastSpell(strike)).unwrap();
                    engine::step(&mut state, Action::ChooseTarget(Target::Object(tyrant))).unwrap();
                    finish(&mut state);
                    assert_eq!(engine::effective_power(&state, tyrant), 6);
                    assert!(engine::has_effective_keyword(
                        &state,
                        tyrant,
                        crate::card_def::Keywords::FIRST_STRIKE
                    ));
                    assert!(engine::has_effective_keyword(
                        &state,
                        swiftblade,
                        crate::card_def::Keywords::DOUBLE_STRIKE
                    ));
                    for source in [tyrant, swiftblade] {
                        state.objects.get_mut(source).summoning_sick = false;
                        state.objects.get_mut(source).v4.lifelink_keyword_counters = 1;
                    }
                    if foundations {
                        crate::combat_damage_v1::enable_foundations_combat_v1(&mut state).unwrap();
                    }
                    state
                        .engine
                        .active_replacements
                        .push(event::ActiveReplacement {
                            id: 90_055,
                            source: swiftblade,
                            kind: event::ReplacementEffectKind::PreventNextDamage {
                                target: Target::Player(actor.opponent()),
                                remaining: 3,
                            },
                        });
                    for _ in 0..24 {
                        let decision = engine::advance_until_decision(&mut state);
                        if matches!(decision, Decision::DeclareAttackers { .. }) {
                            break;
                        }
                        assert!(
                            matches!(decision, Decision::CastSpellOrPass { .. }),
                            "{decision:?}"
                        );
                        engine::step(&mut state, Action::Pass).unwrap();
                    }
                    engine::step(
                        &mut state,
                        Action::DeclareAttackers(vec![tyrant, swiftblade]),
                    )
                    .unwrap();
                    for _ in 0..24 {
                        let decision = engine::advance_until_decision(&mut state);
                        if combat::has_pending(&state) {
                            break;
                        }
                        match decision {
                            Decision::DeclareBlockers { .. } => {
                                engine::step(&mut state, Action::DeclareBlockers(vec![])).unwrap()
                            }
                            Decision::CastSpellOrPass { .. } => {
                                engine::step(&mut state, Action::Pass).unwrap()
                            }
                            other => panic!("actual first-strike entry {other:?}"),
                        }
                    }
                    assert!(combat::has_pending(&state));
                    let lives = state.players.each_ref().map(|p| p.life);
                    let saved = serde_json::to_vec(&state).unwrap();
                    let mut restored = serde_json::from_slice(&saved).unwrap();
                    for branch in [&mut state, &mut restored] {
                        // The affected player allocates the shared shield to the six-power first-strike packet.
                        engine::step(branch, Action::ChooseEffectOption(0)).unwrap();
                        let pending = branch.engine.twinflame_combat_v38.0.as_ref().unwrap();
                        let packet = &pending.packets[pending.selected.unwrap()];
                        assert_eq!(packet.source, tyrant);
                        let option = packet.options.iter().position(|p| if double_first {
                            matches!(p, crate::effect::crystal_prevention_v1::PreventionV1::Multiplier(_))
                        } else { matches!(p, crate::effect::crystal_prevention_v1::PreventionV1::Replacement(_)) }).unwrap();
                        engine::step(branch, Action::ChooseEffectOption(option as u16)).unwrap();
                        assert!(!combat::has_pending(branch));
                        for _ in 0..16 {
                            let count = branch
                                .engine
                                .event_history
                                .iter()
                                .filter(|e| {
                                    matches!(e, CommittedEvent::TwinflameCombatPublishedV38 { .. })
                                })
                                .count();
                            if count == 2 {
                                break;
                            }
                            let decision = engine::advance_until_decision(branch);
                            assert!(
                                matches!(decision, Decision::CastSpellOrPass { .. }),
                                "{decision:?}"
                            );
                            engine::step(branch, Action::Pass).unwrap();
                        }
                        assert!(!combat::has_pending(branch));
                        combat::validate(branch).unwrap();
                        let total = if double_first { 13 } else { 10 };
                        assert_eq!(
                            branch.players[actor.opponent().index()].life,
                            lives[actor.opponent().index()] - total
                        );
                        assert_eq!(
                            branch.players[actor.index()].life,
                            lives[actor.index()] + total
                        );
                        let waves: Vec<_> = branch
                            .engine
                            .event_history
                            .iter()
                            .filter_map(|e| match e {
                                CommittedEvent::TwinflameCombatPublishedV38 { publication } => {
                                    Some(publication)
                                }
                                _ => None,
                            })
                            .collect();
                        assert_eq!(waves.len(), 2);
                        assert_eq!(waves[0].packets.len(), 2);
                        assert_eq!(waves[1].packets.len(), 1);
                        assert_eq!(waves[1].packets[0].source, swiftblade);
                        assert!(!branch
                            .engine
                            .active_replacements
                            .iter()
                            .any(|r| r.id == 90_055));
                    }
                    assert_eq!(
                        serde_json::to_vec(&state).unwrap(),
                        serde_json::to_vec(&restored).unwrap()
                    );
                }
            }
        }
    }
    #[test]
    #[ignore = "true Twinflame printing remains unregistered"]
    fn real_bite_down_owned_menu_refuses_overflow_before_mutation_and_accepts_prevention_first_both_seats(
    ) {
        use crate::effect::crystal_prevention_v1::PreventionV1;
        use crate::effect::{EffectOptionChoicePurpose, PendingEffectChoice};
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready(actor);
            cast(&mut state, actor);
            let dealing = put(&mut state, actor, "Shivan Dragon", Zone::Battlefield);
            let recipient = put(
                &mut state,
                actor.opponent(),
                "Shivan Dragon",
                Zone::Battlefield,
            );
            // Labeled real counter operand, sampled by the actual registered power-damage root.
            state.objects.get_mut(dealing).counters.plus1_plus1 = i32::MAX / 2 - 4;
            let original_amount = engine::effective_power(&state, dealing);
            assert_eq!(original_amount, i32::MAX / 2 + 1);
            state
                .engine
                .active_replacements
                .push(event::ActiveReplacement {
                    id: 90_058,
                    source: recipient,
                    kind: event::ReplacementEffectKind::PreventNextDamage {
                        target: Target::Object(recipient),
                        remaining: original_amount,
                    },
                });
            let spell = put(&mut state, actor, "Bite Down", Zone::Hand);
            state.players[actor.index()].mana_pool[ManaColor::G.pool_index()] = 1;
            state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 1;
            engine::step(&mut state, Action::CastSpell(spell)).unwrap();
            engine::step(&mut state, Action::ChooseTarget(Target::Object(dealing))).unwrap();
            engine::step(&mut state, Action::ChooseTarget(Target::Object(recipient))).unwrap();
            for _ in 0..24 {
                let decision = engine::advance_until_decision(&mut state);
                if crate::effect::crystal_prevention_v1::is_choice(&state) {
                    break;
                }
                assert!(
                    matches!(decision, Decision::CastSpellOrPass { .. }),
                    "{decision:?}"
                );
                engine::step(&mut state, Action::Pass).unwrap();
            }
            let pending = state.engine.pending_effect.as_ref().unwrap();
            let Some(PendingEffectChoice::ChooseOption {
                purpose: EffectOptionChoicePurpose::CrystalPreventionV1 { choice },
                ..
            }) = pending.choice.as_ref()
            else {
                panic!("actual power-damage menu missing")
            };
            assert_eq!(choice.original_amount, original_amount);
            let multiplier = choice
                .options
                .iter()
                .position(|o| matches!(o, PreventionV1::Multiplier(_)))
                .unwrap();
            let prevention = choice
                .options
                .iter()
                .position(|o| matches!(o, PreventionV1::Replacement(_)))
                .unwrap();
            let saved = serde_json::to_vec(&state).unwrap();
            for _ in 0..2 {
                state = serde_json::from_slice(&saved).unwrap();
                assert!(
                    engine::step(&mut state, Action::ChooseEffectOption(multiplier as u16))
                        .is_err()
                );
                assert_eq!(serde_json::to_vec(&state).unwrap(), saved);
            }
            let mut restored: GameState = serde_json::from_slice(&saved).unwrap();
            for branch in [&mut state, &mut restored] {
                engine::step(branch, Action::ChooseEffectOption(prevention as u16)).unwrap();
                finish(branch);
                assert_eq!(branch.objects.get(recipient).zone, Zone::Battlefield);
                assert_eq!(branch.objects.get(recipient).damage, 0);
                assert_eq!(branch.objects.get(spell).zone, Zone::Graveyard);
                crate::effect::crystal_prevention_v1::validate(branch).unwrap();
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&restored).unwrap()
            );
        }
    }
    #[test]
    #[ignore = "true Twinflame printing remains unregistered"]
    fn large_real_bite_down_with_guaranteed_barricade_prevention_commits_zero_without_overflow_both_seats(
    ) {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready(actor);
            cast(&mut state, actor);
            let dealing = put(&mut state, actor, "Shivan Dragon", Zone::Battlefield);
            let recipient = put(
                &mut state,
                actor.opponent(),
                "Shivan Dragon",
                Zone::Battlefield,
            );
            // Independent registered static prevention operand and original real counters.
            put(
                &mut state,
                actor.opponent(),
                "Crystal Barricade",
                Zone::Battlefield,
            );
            state.objects.get_mut(dealing).counters.plus1_plus1 = i32::MAX / 2 - 4;
            assert_eq!(engine::effective_power(&state, dealing), i32::MAX / 2 + 1);
            assert!(state.engine.active_replacements.is_empty());
            let spell = put(&mut state, actor, "Bite Down", Zone::Hand);
            state.players[actor.index()].mana_pool[ManaColor::G.pool_index()] = 1;
            state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 1;
            engine::step(&mut state, Action::CastSpell(spell)).unwrap();
            engine::step(&mut state, Action::ChooseTarget(Target::Object(dealing))).unwrap();
            engine::step(&mut state, Action::ChooseTarget(Target::Object(recipient))).unwrap();
            let saved = serde_json::to_vec(&state).unwrap();
            let mut restored: GameState = serde_json::from_slice(&saved).unwrap();
            for branch in [&mut state, &mut restored] {
                finish(branch);
                assert_eq!(branch.objects.get(recipient).zone, Zone::Battlefield);
                assert_eq!(branch.objects.get(recipient).damage, 0);
                assert_eq!(branch.objects.get(spell).zone, Zone::Graveyard);
                crate::effect::crystal_prevention_v1::validate(branch).unwrap();
                assert!(branch.engine.event_history.iter().any(|e| matches!(e,
                    CommittedEvent::CrystalDamageOperationV3 { operation }
                        if operation.input.source == dealing && operation.input.amount == i32::MAX / 2 + 1
                            && operation.output.is_none() && operation.damage_history.is_none())));
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&restored).unwrap()
            );
        }
    }
    #[test]
    #[ignore = "true Twinflame printing remains unregistered"]
    fn actual_lotleth_dynamic_damage_samples_original_graveyard_before_ordering_both_seats() {
        use crate::effect::crystal_prevention_v1::PreventionV1;
        use crate::effect::{EffectOptionChoicePurpose, PendingEffectChoice};
        for actor in [PlayerId::P0, PlayerId::P1] {
            for double_first in [false, true] {
                let mut state = ready(actor);
                cast(&mut state, actor);
                put(&mut state, actor, "Llanowar Elves", Zone::Graveyard);
                put(&mut state, actor, "Shivan Dragon", Zone::Graveyard);
                put(&mut state, actor, "Forest", Zone::Graveyard);
                let giant = put(&mut state, actor, "Lotleth Giant", Zone::Hand);
                state
                    .engine
                    .active_replacements
                    .push(event::ActiveReplacement {
                        id: 90_059,
                        source: giant,
                        kind: event::ReplacementEffectKind::PreventNextDamage {
                            target: Target::Player(actor.opponent()),
                            remaining: 2,
                        },
                    });
                state.players[actor.index()].mana_pool[ManaColor::B.pool_index()] = 1;
                state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 6;
                engine::step(&mut state, Action::CastSpell(giant)).unwrap();
                let mut targeted = false;
                for _ in 0..32 {
                    let decision = engine::advance_until_decision(&mut state);
                    if crate::effect::crystal_prevention_v1::is_choice(&state) {
                        break;
                    }
                    match decision {
                        Decision::ChooseTargets {
                            player,
                            legal_targets,
                            ..
                        } => {
                            assert!(!targeted);
                            assert_eq!(player, actor);
                            assert_eq!(legal_targets, vec![Target::Player(actor.opponent())]);
                            engine::step(
                                &mut state,
                                Action::ChooseTarget(Target::Player(actor.opponent())),
                            )
                            .unwrap();
                            targeted = true;
                        }
                        Decision::CastSpellOrPass { .. } => {
                            engine::step(&mut state, Action::Pass).unwrap()
                        }
                        other => panic!("actual Lotleth ETB traversal {other:?}"),
                    }
                }
                assert!(targeted);
                let Some(PendingEffectChoice::ChooseOption {
                    purpose: EffectOptionChoicePurpose::CrystalPreventionV1 { choice },
                    ..
                }) = state
                    .engine
                    .pending_effect
                    .as_ref()
                    .unwrap()
                    .choice
                    .as_ref()
                else {
                    panic!("actual dynamic menu missing")
                };
                assert_eq!(choice.original_amount, 2);
                let option = choice
                    .options
                    .iter()
                    .position(|o| {
                        if double_first {
                            matches!(o, PreventionV1::Multiplier(_))
                        } else {
                            matches!(o, PreventionV1::Replacement(_))
                        }
                    })
                    .unwrap();
                let lives = state.players.each_ref().map(|p| p.life);
                let saved = serde_json::to_vec(&state).unwrap();
                let mut restored: GameState = serde_json::from_slice(&saved).unwrap();
                for branch in [&mut state, &mut restored] {
                    engine::step(branch, Action::ChooseEffectOption(option as u16)).unwrap();
                    finish(branch);
                    assert_eq!(
                        branch.players[actor.opponent().index()].life,
                        lives[actor.opponent().index()] - if double_first { 2 } else { 0 }
                    );
                    crate::effect::crystal_prevention_v1::validate(branch).unwrap();
                    sampling::validate(branch).unwrap();
                }
                assert_eq!(
                    serde_json::to_vec(&state).unwrap(),
                    serde_json::to_vec(&restored).unwrap()
                );
                let mut malformed: GameState = serde_json::from_slice(&saved).unwrap();
                let at = malformed
                    .engine
                    .event_history
                    .iter()
                    .position(|e| {
                        matches!(e, CommittedEvent::TwinflameDynamicDamageSampleV38 { .. })
                    })
                    .unwrap();
                malformed.engine.event_history[at] = CommittedEvent::LifeGain {
                    player: actor,
                    amount: 0,
                };
                let bytes = serde_json::to_vec(&malformed).unwrap();
                assert!(engine::step(&mut malformed, Action::Pass).is_err());
                assert_eq!(serde_json::to_vec(&malformed).unwrap(), bytes);
            }
        }
    }
    #[test]
    #[ignore = "true Twinflame printing remains unregistered"]
    fn genuine_cast_into_fire_multi_target_ordering_and_returned_target_exclusion_both_seats() {
        use crate::effect::crystal_prevention_v1::PreventionV1;
        use crate::effect::{EffectOptionChoicePurpose, PendingEffectChoice};
        for actor in [PlayerId::P0, PlayerId::P1] {
            for returned in [false, true] {
                for double_first in [false, true] {
                    let mut state = ready(actor);
                    cast(&mut state, actor);
                    let own = put(&mut state, actor, "Shivan Dragon", Zone::Battlefield);
                    let opposing = put(
                        &mut state,
                        actor.opponent(),
                        "Shivan Dragon",
                        Zone::Battlefield,
                    );
                    state
                        .engine
                        .active_replacements
                        .push(event::ActiveReplacement {
                            id: 90_060,
                            source: opposing,
                            kind: event::ReplacementEffectKind::PreventNextDamage {
                                target: Target::Object(opposing),
                                remaining: 1,
                            },
                        });
                    let spell = put(&mut state, actor, "Cast Into the Fire", Zone::Hand);
                    state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 1;
                    state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 1;
                    engine::step(&mut state, Action::CastSpell(spell)).unwrap();
                    assert!(matches!(
                        engine::advance_until_decision(&mut state),
                        Decision::ChooseSpellMode { .. }
                    ));
                    engine::step(&mut state, Action::ChooseSpellMode(0)).unwrap();
                    engine::step(&mut state, Action::ChooseTarget(Target::Object(own))).unwrap();
                    engine::step(&mut state, Action::ChooseTarget(Target::Object(opposing)))
                        .unwrap();
                    if returned {
                        let old = state.objects.get(own).zone_change_count;
                        event::propose_and_commit(
                            &mut state,
                            event::ProposedEvent::zone_change(own, Zone::Hand),
                        );
                        event::propose_and_commit(
                            &mut state,
                            event::ProposedEvent::zone_change(own, Zone::Battlefield),
                        );
                        assert_eq!(state.objects.get(own).zone_change_count, old + 2);
                    }
                    for _ in 0..24 {
                        let decision = engine::advance_until_decision(&mut state);
                        if crate::effect::crystal_prevention_v1::is_choice(&state) {
                            break;
                        }
                        assert!(
                            matches!(decision, Decision::CastSpellOrPass { .. }),
                            "{decision:?}"
                        );
                        engine::step(&mut state, Action::Pass).unwrap();
                    }
                    assert_eq!(state.objects.get(own).damage, 0);
                    assert_eq!(state.objects.get(opposing).damage, 0);
                    let Some(PendingEffectChoice::ChooseOption {
                        purpose: EffectOptionChoicePurpose::CrystalPreventionV1 { choice },
                        ..
                    }) = state
                        .engine
                        .pending_effect
                        .as_ref()
                        .unwrap()
                        .choice
                        .as_ref()
                    else {
                        panic!("actual multi-target menu missing")
                    };
                    assert_eq!(choice.target, Target::Object(opposing));
                    assert_eq!(choice.original_amount, 1);
                    let slot = choice
                        .options
                        .iter()
                        .position(|option| {
                            if double_first {
                                matches!(option, PreventionV1::Multiplier(_))
                            } else {
                                matches!(option, PreventionV1::Replacement(_))
                            }
                        })
                        .unwrap();
                    let saved = serde_json::to_vec(&state).unwrap();
                    let mut restored: GameState = serde_json::from_slice(&saved).unwrap();
                    for branch in [&mut state, &mut restored] {
                        engine::step(branch, Action::ChooseEffectOption(slot as u16)).unwrap();
                        finish(branch);
                        assert_eq!(branch.objects.get(own).damage, if returned { 0 } else { 1 });
                        assert_eq!(
                            branch.objects.get(opposing).damage,
                            if double_first { 1 } else { 0 }
                        );
                        assert_eq!(branch.objects.get(spell).zone, Zone::Graveyard);
                        crate::effect::crystal_prevention_v1::validate(branch).unwrap();
                        let captures: Vec<_> = branch
                            .engine
                            .event_history
                            .iter()
                            .filter_map(|e| match e {
                                CommittedEvent::CrystalBatchPreparedV3 { capture }
                                    if matches!(
                                        capture.op,
                                        crate::effect::EffectOp::DamageAllTargets { amount: 1 }
                                    ) =>
                                {
                                    Some(capture)
                                }
                                _ => None,
                            })
                            .collect();
                        assert_eq!(captures.len(), 1);
                        assert_eq!(captures[0].proposals.len(), if returned { 1 } else { 2 });
                        assert!(captures[0].proposals.iter().all(|p| p.source == spell));
                    }
                    assert_eq!(
                        serde_json::to_vec(&state).unwrap(),
                        serde_json::to_vec(&restored).unwrap()
                    );
                }
            }
        }
    }
}
