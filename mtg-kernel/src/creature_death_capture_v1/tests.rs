//! Actual paid native deaths. No receipt, trigger or continuation is installed
//! by these fixtures; negative branches edit only copies of actual output.
use super::*;
use crate::{
    engine::{self, Action, Decision},
    ids::{ObjectId, PlayerId},
    policy_observation_v6::tests::put,
    state::Target,
};

fn quiet(state: &mut GameState, decision: Decision) {
    let action = match decision {
        Decision::CastSpellOrPass { .. } => Action::Pass,
        Decision::OrderTriggers { pending, .. } => {
            Action::OrderTriggers((0..pending.len()).collect())
        }
        Decision::ChooseEffectOption {
            option_count: 2, ..
        } => Action::ChooseEffectOption(0),
        Decision::ChooseEffectBoolean { .. } => Action::ChooseEffectBoolean(false),
        other => panic!("unexpected paid death fixture decision: {other:?}"),
    };
    engine::step(state, action).unwrap();
    assert!(state.engine.halted.is_none());
}

fn settle(state: &mut GameState) {
    for _ in 0..64 {
        let decision = engine::advance_until_decision(state);
        assert!(state.engine.halted.is_none(), "{decision:?}");
        if state.stack.is_empty()
            && state.engine.pending_effect.is_none()
            && state.engine.pending_cast.is_none()
            && state.engine.pending_triggers.is_empty()
        {
            assert!(matches!(decision, Decision::CastSpellOrPass { .. }));
            return;
        }
        quiet(state, decision);
    }
    panic!("paid death fixture did not settle");
}

fn cast(state: &mut GameState, actor: PlayerId, spell: ObjectId, target: Option<ObjectId>) {
    let mut found = false;
    for _ in 0..4 {
        let decision = engine::advance_until_decision(state);
        if matches!(&decision, Decision::CastSpellOrPass { player, castable_spells, .. }
            if *player == actor && castable_spells.contains(&spell))
        {
            found = true;
            break;
        }
        quiet(state, decision);
    }
    assert!(found, "genuine spell must be castable by its actual actor");
    let tapped = |state: &GameState| {
        state.players[actor.index()]
            .battlefield
            .iter()
            .filter(|&&id| state.objects.get(id).tapped)
            .count()
    };
    let before = tapped(state);
    let mana_value = CARD_DEFS[usize::from(state.objects.get(spell).card_def)].mana_value;
    engine::step(state, Action::CastSpell(spell)).unwrap();
    if let Some(target) = target {
        let decision = engine::advance_until_decision(state);
        assert!(
            matches!(decision, Decision::ChooseTargets { legal_targets, .. }
            if legal_targets.contains(&Target::Object(target)))
        );
        engine::step(state, Action::ChooseTarget(Target::Object(target))).unwrap();
    }
    assert!(matches!(engine::advance_until_decision(state),
        Decision::CastSpellOrPass { player, .. } if player == actor));
    assert!(state.engine.pending_cast.is_none());
    assert!(state.engine.halted.is_none());
    assert_eq!(tapped(state) - before, usize::from(mana_value));
    assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
}

fn paid_death(actor: PlayerId, alesha: bool) -> (GameState, Vec<ObjectId>, ObjectLinkV4) {
    let mut state = crate::extra_turn_v1::tests::ready(actor);
    for player in [PlayerId::P0, PlayerId::P1] {
        for _ in 0..8 {
            for land in ["Forest", "Swamp", "Mountain"] {
                put(&mut state, player, land, Zone::Battlefield);
            }
        }
    }
    let mut observers = Vec::new();
    for name in if alesha {
        vec!["Spinner of Souls", "High-Society Hunter"]
    } else {
        vec!["Spinner of Souls"]
    } {
        let source = put(&mut state, actor, name, Zone::Hand);
        cast(&mut state, actor, source, None);
        settle(&mut state);
        observers.push(source);
    }
    let target = put(
        &mut state,
        actor,
        if alesha {
            "Alesha, Who Laughs at Fate"
        } else {
            "Llanowar Elves"
        },
        Zone::Hand,
    );
    cast(&mut state, actor, target, None);
    settle(&mut state);
    let died = ObjectLinkV4 {
        object: target,
        zone_change_count: state.objects.get(target).zone_change_count,
    };
    let bolt = put(&mut state, actor.opponent(), "Lightning Bolt", Zone::Hand);
    cast(&mut state, actor.opponent(), bolt, Some(target));
    for _ in 0..8 {
        let decision = engine::advance_until_decision(&mut state);
        assert!(state.engine.halted.is_none(), "{decision:?}");
        if state.objects.get(target).zone == Zone::Graveyard {
            assert_eq!(
                state.objects.get(target).zone_change_count,
                died.zone_change_count + 1
            );
            assert_eq!(state.objects.get(bolt).zone, Zone::Graveyard);
            return (state, observers, died);
        }
        quiet(&mut state, decision);
    }
    panic!("genuine opposing Bolt must resolve and kill the paid creature");
}

fn capture_rows(state: &GameState, died: ObjectLinkV4) -> Vec<(usize, AbilitySourceContractV4)> {
    state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(index, event)| match event {
            CommittedEvent::ControlledCreatureDeathCaptureV1 {
                source,
                died: actual,
                ..
            } if *actual == died => Some((index, *source)),
            _ => None,
        })
        .collect()
}

fn assert_native_capture_and_restore(
    mut state: GameState,
    actor: PlayerId,
    observers: &[ObjectId],
    died: ObjectLinkV4,
) {
    let rows = capture_rows(&state, died);
    assert_eq!(rows.len(), observers.len());
    for (at, source) in rows {
        assert!(observers.contains(&source.source));
        assert_eq!(source.controller, actor);
        assert_eq!(source.owner, actor);
        assert_eq!(source.zone, Zone::Battlefield);
        assert_eq!(
            source.zone_change_count,
            state.objects.get(source.source).zone_change_count
        );
        assert!(state
            .engine
            .event_history
            .iter()
            .any(|event| matches!(event,
            CommittedEvent::ControlledCreatureDeathCaptureCollectedV1 { capture_history_index }
                if usize::try_from(*capture_history_index).ok() == Some(at))));
    }
    let mut replay: GameState =
        serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
    assert_eq!(state, replay);
    settle(&mut state);
    settle(&mut replay);
    assert_eq!(state, replay);
    assert_eq!(state.state_hash(), replay.state_hash());
    assert_eq!(state.objects.get(died.object).zone, Zone::Graveyard);
}

#[test]
fn paid_spinner_elf_death_keeps_exact_native_capture_and_restore_both_seats() {
    assert!(crate::event::niv_mizzet_catalog_enabled_v1());
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (state, observers, died) = paid_death(actor, false);
        assert_native_capture_and_restore(state, actor, &observers, died);
    }
}

#[test]
fn paid_alesha_death_keeps_spinner_hunter_lki_and_refuses_departure_receipt_tamper_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (state, observers, died) = paid_death(actor, true);
        let rows = capture_rows(&state, died);
        assert_eq!(rows.len(), 2);
        let capture_at = rows[0].0;
        let capture = state.engine.event_history[capture_at].clone();
        // These are unchanged actual native history records projected exactly
        // as the emitter's collection log, whose sole omission is Niv's receipt.
        let events: Vec<_> = state.engine.event_history[..=capture_at]
            .iter()
            .filter(|event| {
                !matches!(
                    event,
                    CommittedEvent::NivMizzetVisionaryDamageSourceDepartureV1 { .. }
                )
            })
            .cloned()
            .collect();
        let log_index = events.len() - 1;
        let death = preceding_death(&state, &events, log_index, capture_at, died, actor).unwrap();
        assert!(
            matches!(death, CommittedEvent::ZoneChange { object, .. } if *object == died.object)
        );
        let niv_at = capture_at - 2;
        let alesha_at = capture_at - 1;
        assert!(matches!(
            state.engine.event_history[niv_at],
            CommittedEvent::NivMizzetVisionaryDamageSourceDepartureV1 { .. }
        ));
        assert!(matches!(state.engine.event_history[alesha_at],
            CommittedEvent::AleshaPowerLkiV1 { source, power: 2, .. } if source == died.object));
        for mutation in 0..11 {
            let mut forged = state.clone();
            let history = &mut forged.engine.event_history;
            match mutation {
                0 => {
                    history.remove(niv_at);
                }
                1 => {
                    history.remove(alesha_at);
                }
                2 => {
                    history.insert(niv_at, history[niv_at].clone());
                }
                3 => {
                    history.insert(alesha_at, history[alesha_at].clone());
                }
                4 => history.swap(niv_at, alesha_at),
                5 => {
                    if let CommittedEvent::NivMizzetVisionaryDamageSourceDepartureV1 {
                        departure_history_index,
                        ..
                    } = &mut history[niv_at]
                    {
                        *departure_history_index += 1;
                    }
                }
                6 => {
                    if let CommittedEvent::NivMizzetVisionaryDamageSourceDepartureV1 {
                        source,
                        ..
                    } = &mut history[niv_at]
                    {
                        source.controller = actor.opponent();
                    }
                }
                7 => {
                    if let CommittedEvent::NivMizzetVisionaryDamageSourceDepartureV1 {
                        source,
                        ..
                    } = &mut history[niv_at]
                    {
                        source.zone_change_count += 1;
                    }
                }
                8 => {
                    if let CommittedEvent::AleshaPowerLkiV1 { power, .. } = &mut history[alesha_at]
                    {
                        *power += 1;
                    }
                }
                9 => {
                    history.insert(
                        capture_at,
                        CommittedEvent::LifeGain {
                            player: actor,
                            amount: 1,
                        },
                    );
                }
                10 => {
                    if let CommittedEvent::AleshaPowerLkiV1 {
                        departure_history_index,
                        ..
                    } = &mut history[alesha_at]
                    {
                        *departure_history_index += 1;
                    }
                }
                _ => unreachable!(),
            }
            let at = history.iter().position(|event| event == &capture).unwrap();
            let unchanged = forged.clone();
            assert!(
                preceding_death(&forged, &events, log_index, at, died, actor).is_none(),
                "altered actual departure receipt accepted, mutation {mutation}"
            );
            // Power itself retains the existing mutable-native-LKI trust
            // contract. Its alteration is detectable here through the genuine
            // independent collection log, not invented raw-world authority.
            if mutation != 8 {
                assert!(validated_capture_death(&forged, at).is_err());
            }
            assert_eq!(forged, unchanged);
        }
        assert_native_capture_and_restore(state, actor, &observers, died);
    }
}
