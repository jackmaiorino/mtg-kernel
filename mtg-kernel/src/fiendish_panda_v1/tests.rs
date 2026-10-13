use super::*;
use crate::engine::{self, Action, Decision};
use crate::event::{self, CommittedEvent};
use crate::policy_observation_v6::tests::{put, ready_state_for};

#[test]
fn absent_printing_emits_no_capture_and_genuine_predecessor_gain_keeps_its_old_path_both_seats() {
    if crate::card_def::card_id_by_name("Fiendish Panda").is_some() {
        return;
    }
    assert!(!enabled());
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state_for(actor);
        let source = put(&mut state, actor, "Lightning Bolt", Zone::Hand);
        let before = serde_json::to_vec(&state).unwrap();
        assert!(
            capture_death(&state, &ProposedEvent::zone_change(source, Zone::Graveyard)).is_none()
        );
        assert!(validate_state(&state).is_ok());
        assert!(require_legacy_supported(&state, Some(actor)).is_ok());
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        let life = state.players[actor.index()].life;
        event::propose_and_commit(&mut state, ProposedEvent::life_gain(actor, 4));
        assert_eq!(state.players[actor.index()].life, life + 4);
        assert!(state.engine.event_history.iter().any(
            |e| matches!(e, CommittedEvent::LifeGain { player, amount: 4 } if *player == actor)
        ));
        assert!(!state.engine.event_history.iter().any(|e| matches!(
            e,
            CommittedEvent::FiendishPandaLifeGainCaptureV1 { .. }
                | CommittedEvent::FiendishPandaDeathCaptureV1 { .. }
                | CommittedEvent::FiendishPandaCaptureCollectedV1 { .. }
        )));
        let mut expected = state.clone();
        record_gain(&mut expected, 0);
        assert_eq!(
            serde_json::to_vec(&expected).unwrap(),
            serde_json::to_vec(&state).unwrap()
        );
    }
}

#[test]
fn foreign_actual_gain_cannot_authorize_a_panda_root_or_an_orphan_collection_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut original = ready_state_for(actor);
        let source = put(&mut original, actor, "Lightning Bolt", Zone::Hand);
        let gain = original.engine.event_history.len();
        event::propose_and_commit(&mut original, ProposedEvent::life_gain(actor, 1));
        let contract = AbilitySourceContractV4::capture(&original, source);
        for orphan in [false, true] {
            let mut state = original.clone();
            if orphan {
                state
                    .engine
                    .event_history
                    .push(CommittedEvent::FiendishPandaCaptureCollectedV1 {
                        capture_history_index: gain as u64,
                    });
            } else {
                state
                    .engine
                    .event_history
                    .push(CommittedEvent::FiendishPandaLifeGainCaptureV1 {
                        source: contract,
                        gain_history_index: gain as u64,
                        amount: 1,
                    });
            }
            let before = serde_json::to_vec(&state).unwrap();
            assert!(validate_state(&state).is_err());
            assert!(require_legacy_supported(&state, Some(actor)).is_err());
            assert!(engine::step(&mut state, Action::Pass).is_err());
            assert_eq!(serde_json::to_vec(&state).unwrap(), before);
            let history = state.engine.event_history.clone();
            assert!(matches!(
                engine::advance_until_decision(&mut state),
                Decision::Halted { .. }
            ));
            assert_eq!(state.engine.event_history, history);
            state.engine.halted = None;
            assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        }
    }
}

#[test]
fn precise_panda_meaning_never_relabels_a_registered_predecessor_as_the_missing_printing() {
    let bolt = crate::card_def::card_id_by_name("Lightning Bolt").unwrap();
    use crate::rules_meaning_fiendish_panda_v1::{printed_meaning, RootV1};
    for root in [
        RootV1::CreatureSpell,
        RootV1::ControllerLifeGain,
        RootV1::SourceDeath,
    ] {
        assert!(printed_meaning(bolt, root).is_err());
        assert!(printed_meaning(u16::MAX, root).is_err());
    }
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod printed {
    use super::*;
    fn panda(state: &mut GameState, actor: PlayerId) -> ObjectId {
        let source = put(state, actor, "Fiendish Panda", Zone::Hand);
        state.players[actor.index()].mana_pool[ManaColor::W.pool_index()] = 1;
        state.players[actor.index()].mana_pool[ManaColor::B.pool_index()] = 1;
        state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 2;
        engine::step(state, Action::CastSpell(source)).unwrap();
        assert!(
            matches!(engine::advance_until_decision(state), Decision::CastSpellOrPass { player, .. } if player == actor)
        );
        for _ in 0..12 {
            if state.objects.get(source).zone == Zone::Battlefield {
                return source;
            }
            assert!(matches!(
                engine::advance_until_decision(state),
                Decision::CastSpellOrPass { .. }
            ));
            engine::step(state, Action::Pass).unwrap();
        }
        panic!("genuine paid Panda creature spell did not resolve");
    }
    #[test]
    #[ignore = "Requires parent-owned genuine Fiendish Panda registration; no source-validation bypass"]
    fn actual_paid_panda_captures_each_gain_event_once_regardless_of_amount_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state_for(actor);
            let source = panda(&mut state, actor);
            for (player, amount) in [(actor, 4), (actor, 1), (actor.opponent(), 3), (actor, 0)] {
                event::propose_and_commit(&mut state, ProposedEvent::life_gain(player, amount));
            }
            let pending = crate::trigger::capture_triggers_without_sba(&mut state);
            let own = pending
                .iter()
                .filter(|p| p.source == source)
                .collect::<Vec<_>>();
            assert_eq!(own.len(), 2);
            assert!(own.iter().all(|p| p.controller == actor
                && p.target_spec == TargetSpec::None
                && matches!(p.effect, EffectOp::FiendishPandaCounterFromCaptureV1 { .. })));
            assert!(crate::trigger::capture_triggers_without_sba(&mut state)
                .iter()
                .all(|p| p.source != source));
            assert_eq!(
                state.objects.get(source).counters.plus1_plus1,
                0,
                "waiting counter roots cannot rescue lethal damage before resolution"
            );
        }
    }
    #[test]
    #[ignore = "Requires parent-owned genuine Fiendish Panda registration; no source-validation bypass"]
    fn actual_paid_panda_death_freezes_power_before_simultaneous_batch_removes_anthem_both_orders_and_seats(
    ) {
        for actor in [PlayerId::P0, PlayerId::P1] {
            for reverse in [false, true] {
                let mut state = ready_state_for(actor);
                let source = panda(&mut state, actor);
                let anthem = put(&mut state, actor, "Anthem of Champions", Zone::Battlefield);
                assert_eq!(crate::engine::effective_power(&state, source), 4);
                let mut deaths = vec![anthem, source];
                if reverse {
                    deaths.reverse();
                }
                event::propose_and_commit_batch(
                    &mut state,
                    deaths
                        .into_iter()
                        .map(|id| ProposedEvent::zone_change(id, Zone::Graveyard))
                        .collect(),
                );
                let pending = crate::trigger::capture_triggers_without_sba(&mut state);
                let own = pending
                    .iter()
                    .filter(|p| p.source == source)
                    .collect::<Vec<_>>();
                assert_eq!(own.len(), 1);
                assert_eq!(own[0].controller, actor);
                assert_eq!(
                    own[0].target_spec,
                    TargetSpec::AnotherNonBearCreatureCardInOwnGraveyardManaValueAtMost(4)
                );
                assert!(matches!(
                    own[0].effect,
                    EffectOp::FiendishPandaReturnFromCaptureV1 { maximum: 4, .. }
                ));
                assert!(validate_state(&state).is_ok());
            }
        }
    }
}
