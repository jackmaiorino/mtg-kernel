//! Opt-in, engine-only history for the first actual life gain each turn.
//! Public observations and frozen policy encoders do not inspect this ledger.

use crate::event::CommittedEvent;
use crate::ids::ObjectId;
use crate::state::{
    AbilitySourceContractV4, FirstLifeGainCaptureV1, GameState, LifeGainTurnV1, Zone,
};
use crate::trigger::{PendingTrigger, TriggerCondition};

pub(crate) fn initialize_for_pool(state: &mut GameState) {
    #[cfg(feature = "limited-fdn-fixtures")]
    if state.objects.iter().any(|(_, object)| {
        crate::trigger::triggers_for(object.card_def)
            .iter()
            .any(|ability| {
                matches!(
                    ability.condition,
                    crate::trigger::TriggerCondition::ControllerFirstLifeGain { .. }
                )
            })
    }) {
        begin_turn(state);
    }
    #[cfg(not(feature = "limited-fdn-fixtures"))]
    let _ = state;
}

fn begin_turn(state: &mut GameState) {
    let history_index = state.engine.event_history.len();
    state
        .engine
        .event_history
        .push(CommittedEvent::LifeGainTurnBeganV1 {
            turn: state.turn,
            active_player: state.active_player,
        });
    state.life_gain_turn_v1 = Some(LifeGainTurnV1 {
        turn: state.turn,
        active_player: state.active_player,
        history_index,
        captures: Vec::new(),
    });
}

/// Reset for every actual turn entry, even when an extra turn retains the seat.
pub(crate) fn reset_at_untap(state: &mut GameState) {
    if let Some(ledger) = &state.life_gain_turn_v1 {
        if let Some(capture) = ledger.captures.first() {
            state.engine.halted = Some((
                crate::engine::UnsupportedMechanic::InvalidFirstLifeGainHistory,
                capture.pending.source,
            ));
            return;
        }
        begin_turn(state);
    }
}

/// Reconstruct the ordinal at each event's own commit time, before the entire
/// atomic resolution's events are collected. Zero/nonpositive gains do not count.
pub(crate) fn event_ordinals(
    state: &GameState,
    events: &[CommittedEvent],
) -> Result<Vec<Option<u32>>, String> {
    let ledger = state
        .life_gain_turn_v1
        .as_ref()
        .ok_or("first-life-gain trigger requires its turn-history ledger")?;
    if ledger.turn != state.turn || ledger.active_player != state.active_player {
        return Err("first-life-gain ledger belongs to another turn".into());
    }
    let expected = CommittedEvent::LifeGainTurnBeganV1 {
        turn: ledger.turn,
        active_player: ledger.active_player,
    };
    if state.engine.event_history.get(ledger.history_index) != Some(&expected) {
        return Err("first-life-gain cursor has no matching turn-boundary anchor".into());
    }
    let history = &state.engine.event_history[ledger.history_index + 1..];
    if history
        .iter()
        .any(|event| matches!(event, CommittedEvent::LifeGainTurnBeganV1 { .. }))
    {
        return Err("first-life-gain cursor precedes a newer turn boundary".into());
    }
    let gain = |event: &CommittedEvent| match event {
        CommittedEvent::LifeGain { player, amount } if *amount > 0 => Some((*player, *amount)),
        _ => None,
    };
    let historical: Vec<_> = history.iter().filter_map(gain).collect();
    let batch: Vec<_> = events.iter().filter_map(gain).collect();
    let before = historical
        .len()
        .checked_sub(batch.len())
        .ok_or("first-life-gain batch exceeds committed turn history")?;
    if historical[before..] != batch {
        return Err("first-life-gain batch disagrees with committed turn history".into());
    }
    let mut counts = [0_u32; 2];
    for (player, _) in &historical[..before] {
        counts[player.index()] = counts[player.index()]
            .checked_add(1)
            .ok_or("first-life-gain event count overflow")?;
    }
    events
        .iter()
        .map(|event| {
            let Some((player, _)) = gain(event) else {
                return Ok(None);
            };
            let ordinal = counts[player.index()]
                .checked_add(1)
                .ok_or("first-life-gain event count overflow")?;
            counts[player.index()] = ordinal;
            Ok(Some(ordinal))
        })
        .collect()
}

fn capture_pending(
    contract: AbilitySourceContractV4,
    effect: crate::effect::EffectOp,
) -> PendingTrigger {
    PendingTrigger {
        controller: contract.controller,
        source: contract.source,
        effect,
        is_madness_offer: false,
        kicked: false,
        target_spec: crate::card_def::TargetSpec::None,
        targets: Vec::new(),
        target_contracts: Vec::new(),
        placement_ordered: false,
        source_contract: Some(contract),
        granted_by: None,
        optional_additional_cost_paid: None,
        paid_cost_refs: Vec::new(),
    }
}

/// Called immediately after a positive gain commits, before the next effect op.
/// The permanent history consumes first gain even when no source is active.
pub(crate) fn capture_committed_gain(state: &mut GameState) {
    let Some(CommittedEvent::LifeGain { player, amount }) = state.engine.event_history.last()
    else {
        return;
    };
    if *amount <= 0 {
        return;
    }
    let (player, amount) = (*player, *amount);
    let active_player = state.active_player;
    let candidates: Vec<_> = state
        .objects
        .iter()
        .filter(|(id, object)| {
            object.zone == Zone::Battlefield
                && object.controller == player
                && object.v4.face_index == 0
                && crate::continuous_characteristics_v1::printed_abilities_active(state, *id)
        })
        .flat_map(|(id, object)| {
            crate::trigger::triggers_for(object.card_def)
                .iter()
                .enumerate()
                .filter_map(move |(ability_index, ability)| {
                    let TriggerCondition::ControllerFirstLifeGain { own_turn_only } =
                        ability.condition
                    else {
                        return None;
                    };
                    (!own_turn_only || active_player == player).then_some((
                        id,
                        ability_index,
                        ability,
                    ))
                })
        })
        .collect();
    if candidates.is_empty() {
        return;
    }
    let event = CommittedEvent::LifeGain { player, amount };
    match event_ordinals(state, &[event]) {
        Ok(ordinals) if ordinals == [Some(1)] => {}
        Ok(_) => return,
        Err(_) => {
            state.engine.halted = Some((
                crate::engine::UnsupportedMechanic::InvalidFirstLifeGainHistory,
                candidates[0].0,
            ));
            return;
        }
    }
    let gain_history_index = state.engine.event_history.len() - 1;
    let captures: Vec<_> = candidates
        .into_iter()
        .map(|(source, ability_index, ability)| FirstLifeGainCaptureV1 {
            gain_history_index,
            ability_index: u16::try_from(ability_index).expect("bounded definition abilities"),
            pending: capture_pending(
                AbilitySourceContractV4::capture(state, source),
                (ability.effect)(),
            ),
        })
        .collect();
    state
        .life_gain_turn_v1
        .as_mut()
        .expect("validated first-life-gain ledger")
        .captures
        .extend(captures);
}

fn validate_capture(state: &GameState, capture: &FirstLifeGainCaptureV1) -> Result<(), String> {
    // Validate the boundary even when more life gains followed this capture.
    event_ordinals(state, &[])?;
    let ledger = state.life_gain_turn_v1.as_ref().expect("validated ledger");
    if capture.gain_history_index <= ledger.history_index {
        return Err("captured first life gain precedes this turn's boundary".into());
    }
    let Some(CommittedEvent::LifeGain { player, amount }) =
        state.engine.event_history.get(capture.gain_history_index)
    else {
        return Err("captured first life gain has no committed event".into());
    };
    if *amount <= 0
        || state.engine.event_history[ledger.history_index + 1..capture.gain_history_index]
            .iter()
            .any(|event| matches!(event, CommittedEvent::LifeGain { player: previous, amount } if previous == player && *amount > 0))
    {
        return Err("captured life gain is not the player's first actual gain".into());
    }
    let contract = capture
        .pending
        .source_contract
        .ok_or("captured life-gain trigger has no historical source contract")?;
    let definition = crate::trigger::triggers_for(contract.card_def)
        .get(usize::from(capture.ability_index))
        .ok_or("captured life-gain trigger has no definition ability")?;
    let TriggerCondition::ControllerFirstLifeGain { own_turn_only } = definition.condition else {
        return Err("captured ability is not a first-life-gain trigger".into());
    };
    if contract.zone != Zone::Battlefield
        || contract.controller != *player
        || (own_turn_only && ledger.active_player != *player)
        || state.objects.try_get(contract.source).is_none_or(|object| {
            object.card_def != contract.card_def
                || object.owner != contract.owner
                || object.zone_change_count < contract.zone_change_count
                || (object.zone_change_count == contract.zone_change_count
                    && object.zone != contract.zone)
                || object.spell_copy_origin.is_some()
        })
        || capture.pending != capture_pending(contract, (definition.effect)())
    {
        return Err("captured life-gain ability or historical incarnation changed".into());
    }
    Ok(())
}

/// Join the ordinary after-resolution trigger group, validating before drain.
pub(crate) fn take_captures(state: &mut GameState) -> Result<Vec<PendingTrigger>, ObjectId> {
    let pending_gain = state
        .engine
        .event_log
        .iter()
        .any(|event| matches!(event, CommittedEvent::LifeGain { amount, .. } if *amount > 0));
    if pending_gain {
        if let Some(source) = state.objects.iter().find_map(|(id, object)| {
            crate::trigger::triggers_for(object.card_def)
                .iter()
                .any(|ability| {
                    matches!(
                        ability.condition,
                        TriggerCondition::ControllerFirstLifeGain { .. }
                    )
                })
                .then_some(id)
        }) {
            if event_ordinals(state, &state.engine.event_log).is_err() {
                return Err(source);
            }
        }
    }
    let Some(ledger) = state.life_gain_turn_v1.as_ref() else {
        return Ok(Vec::new());
    };
    let mut seen = std::collections::BTreeSet::new();
    for capture in &ledger.captures {
        let generation = capture
            .pending
            .source_contract
            .map(|contract| contract.zone_change_count);
        if validate_capture(state, capture).is_err()
            || !seen.insert((
                capture.gain_history_index,
                capture.pending.source,
                generation,
                capture.ability_index,
            ))
        {
            return Err(capture.pending.source);
        }
    }
    Ok(std::mem::take(
        &mut state
            .life_gain_turn_v1
            .as_mut()
            .expect("present ledger")
            .captures,
    )
    .into_iter()
    .map(|capture| capture.pending)
    .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{self, ProposedEvent};
    use crate::ids::PlayerId;
    use crate::state::Step;

    fn legacy() -> GameState {
        let plains = crate::card_def::card_id_by_name("Plains").unwrap();
        let mut state =
            GameState::new_from_libraries(&[plains; 40], &[plains; 40], |_| "Plains".into(), 630);
        state.step = Step::Main1;
        state
    }

    fn gain(state: &mut GameState, player: PlayerId, amount: i32) {
        event::propose_and_commit(state, ProposedEvent::life_gain(player, amount));
    }

    #[test]
    fn first_gain_counts_actual_events_for_each_seat_and_batch() {
        let mut state = legacy();
        begin_turn(&mut state);
        gain(&mut state, PlayerId::P0, 0);
        gain(&mut state, PlayerId::P0, -1);
        assert!(state.engine.event_log.is_empty());
        gain(&mut state, PlayerId::P0, 7);
        gain(&mut state, PlayerId::P1, 2);
        gain(&mut state, PlayerId::P0, 1);
        assert_eq!(
            event_ordinals(&state, &state.engine.event_log).unwrap(),
            vec![Some(1), Some(1), Some(2)]
        );
        state.engine.event_log.clear();
        gain(&mut state, PlayerId::P1, 9);
        assert_eq!(
            event_ordinals(&state, &state.engine.event_log).unwrap(),
            vec![Some(2)]
        );
    }

    #[test]
    fn first_gain_resets_at_untap_even_for_same_seat_extra_turn() {
        let mut state = legacy();
        begin_turn(&mut state);
        gain(&mut state, PlayerId::P0, 3);
        state.engine.event_log.clear();
        // An actual Untap entry is authoritative even with unchanged seat/round.
        state.step = Step::Untap;
        reset_at_untap(&mut state);
        gain(&mut state, PlayerId::P0, 4);
        assert_eq!(
            event_ordinals(&state, &state.engine.event_log).unwrap(),
            vec![Some(1)]
        );
        state.engine.event_log.clear();
        state.active_player = PlayerId::P1;
        reset_at_untap(&mut state);
        gain(&mut state, PlayerId::P0, 1);
        gain(&mut state, PlayerId::P1, 1);
        assert_eq!(
            event_ordinals(&state, &state.engine.event_log).unwrap(),
            vec![Some(1), Some(1)]
        );
    }

    #[test]
    fn first_gain_rejects_missing_stale_or_corrupted_boundary_and_batch() {
        let mut state = legacy();
        assert!(event_ordinals(&state, &[])
            .unwrap_err()
            .contains("requires"));
        begin_turn(&mut state);
        gain(&mut state, PlayerId::P0, 1);
        let valid = state.clone();
        state.life_gain_turn_v1.as_mut().unwrap().history_index += 1;
        assert!(event_ordinals(&state, &[]).unwrap_err().contains("anchor"));
        state = valid.clone();
        state.active_player = PlayerId::P1;
        assert!(event_ordinals(&state, &[])
            .unwrap_err()
            .contains("another turn"));
        state = valid.clone();
        state
            .engine
            .event_history
            .push(CommittedEvent::LifeGainTurnBeganV1 {
                turn: state.turn,
                active_player: state.active_player,
            });
        assert!(event_ordinals(&state, &[]).unwrap_err().contains("newer"));
        state = valid;
        assert!(event_ordinals(
            &state,
            &[CommittedEvent::LifeGain {
                player: PlayerId::P1,
                amount: 1
            }]
        )
        .unwrap_err()
        .contains("disagrees"));
    }

    #[test]
    fn first_gain_snapshot_preserves_pending_batch_and_both_hashes() {
        let mut state = legacy();
        begin_turn(&mut state);
        gain(&mut state, PlayerId::P0, 1);
        let snapshot = state.snapshot();
        let bytes = serde_json::to_vec(&state).unwrap();
        let hash = state.state_hash();
        let diagnostic = state.diagnostic_state_hash();
        gain(&mut state, PlayerId::P0, 2);
        state.restore(&snapshot);
        assert_eq!(serde_json::to_vec(&state).unwrap(), bytes);
        assert_eq!(state.state_hash(), hash);
        assert_eq!(state.diagnostic_state_hash(), diagnostic);
        let restored: GameState = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(restored, state);
        assert_eq!(
            event_ordinals(&restored, &restored.engine.event_log).unwrap(),
            vec![Some(1)]
        );
    }

    #[test]
    fn first_gain_legacy_state_and_public_keys_do_not_reveal_ledger_or_hidden_pool() {
        let mut state = legacy();
        assert!(state.life_gain_turn_v1.is_none());
        let initial_bytes = serde_json::to_vec(&state).unwrap();
        assert!(!String::from_utf8(initial_bytes.clone())
            .unwrap()
            .contains("life_gain_turn_v1"));
        let hash = state.state_hash();
        let historical: GameState = serde_json::from_slice(&initial_bytes).unwrap();
        assert_eq!(historical.state_hash(), hash);
        let public = crate::rl::observe_v1(&state, PlayerId::P0, 0).unwrap();
        begin_turn(&mut state);
        let changed_private = crate::rl::observe_v1(&state, PlayerId::P0, 0).unwrap();
        assert_eq!(public, changed_private);
        assert_ne!(state.state_hash(), hash);
        let mut resampled = state.clone();
        let id = resampled.players[1].library[0];
        let swamp = crate::card_def::card_id_by_name("Swamp").unwrap();
        resampled.objects.get_mut(id).card_def = swamp;
        resampled.objects.get_mut(id).name = "Swamp".into();
        assert_eq!(resampled.life_gain_turn_v1, state.life_gain_turn_v1);
        assert_eq!(
            crate::rl::observe_v1(&resampled, PlayerId::P0, 0).unwrap(),
            changed_private
        );
        let surface = crate::surface_v2::HarnessSurfaceV2::new();
        assert_eq!(
            crate::rl::observe_v2(&state, &surface, PlayerId::P0, 0).unwrap(),
            crate::rl::observe_v2(&resampled, &surface, PlayerId::P0, 0).unwrap()
        );
        assert_ne!(resampled.state_hash(), state.state_hash());
    }
}
