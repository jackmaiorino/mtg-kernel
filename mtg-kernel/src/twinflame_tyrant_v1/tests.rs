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
}
