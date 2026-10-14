//! Selected Limited life-loss history. Standard keeps its original recording path.
//! Opt-in uses public catalog definitions, never hidden roster identities.
use crate::card_def::CARD_DEFS;
use crate::event::CommittedEvent;
use crate::ids::PlayerId;
use crate::state::{GameState, LifeLossTurnV1};

pub(crate) fn enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        cfg!(all(feature = "limited-fdn-fixtures", not(feature = "standard-magezero-fixtures")))
            && CARD_DEFS.iter().enumerate().any(|(id, definition)| {
                definition.is_executable() && crate::trigger::triggers_for(id as u16).iter().any(|ability| {
                    ability.condition == crate::trigger::TriggerCondition::BeginningControllerEndStepIfOpponentLostLife
                        && ability.home_zone == crate::state::Zone::Battlefield
                })
            })
    })
}
fn begin_turn(state: &mut GameState) {
    state
        .engine
        .event_history
        .push(CommittedEvent::LifeLossTurnBeganV1 {
            turn: state.turn_key_v1(),
            active_player: state.active_player,
        });
    state.life_loss_turn_v1 = Some(LifeLossTurnV1 {
        turn: state.turn_key_v1(),
        active_player: state.active_player,
        players: [false; 2],
    });
}
pub(crate) fn initialize_for_catalog(state: &mut GameState) {
    if enabled() {
        begin_turn(state);
    }
}
pub(crate) fn reset_at_untap(state: &mut GameState) {
    if enabled() {
        begin_turn(state);
    }
}
pub(crate) fn committed_loss_player(event: &CommittedEvent) -> Option<PlayerId> {
    match event {
        CommittedEvent::LifeLoss { player, amount } if *amount > 0 => Some(*player),
        CommittedEvent::Damage {
            target: crate::state::Target::Player(player),
            amount,
            ..
        } if *amount > 0 => Some(*player),
        _ => None,
    }
}
/// Called immediately after the actual final replacement-adjusted event commits.
pub(crate) fn capture_committed_loss_at(state: &mut GameState, at: usize) {
    if !enabled() {
        return;
    }
    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    if let Some(player) = state
        .engine
        .event_history
        .get(at)
        .and_then(committed_loss_player)
    {
        state.record_life_loss_v1(player);
    }
    #[cfg(not(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )))]
    let _ = (state, at);
}
pub(crate) fn flags(state: &GameState) -> Result<[bool; 2], String> {
    let ledger = state
        .life_loss_turn_v1
        .ok_or("missing selected life-loss ledger")?;
    if ledger.turn != state.turn_key_v1() || ledger.active_player != state.active_player {
        return Err("life-loss ledger belongs to another turn".into());
    }
    let boundary = state
        .engine
        .event_history
        .iter()
        .rposition(|event| matches!(event, CommittedEvent::LifeLossTurnBeganV1 { .. }))
        .ok_or("life-loss ledger lost its actual turn anchor")?;
    if state.engine.event_history[boundary]
        != (CommittedEvent::LifeLossTurnBeganV1 {
            turn: state.turn_key_v1(),
            active_player: state.active_player,
        })
    {
        return Err("life-loss anchor belongs to another turn".into());
    }
    let mut flags = [false; 2];
    for event in &state.engine.event_history[boundary + 1..] {
        if let Some(player) = committed_loss_player(event) {
            flags[player.index()] = true;
        }
    }
    if ledger.players != flags {
        return Err("life-loss ledger disagrees with actual loss history".into());
    }
    Ok(flags)
}
pub(crate) fn validate(state: &GameState) -> Result<(), String> {
    if state
        .engine
        .event_log
        .iter()
        .any(|event| matches!(event, CommittedEvent::LifeLossTurnBeganV1 { .. }))
    {
        return Err("private life-loss anchor entered public trigger log".into());
    }
    if enabled() {
        flags(state)?;
    }
    Ok(())
}
pub(crate) fn opponent_lost_life(state: &GameState, controller: PlayerId) -> Result<bool, String> {
    if !enabled() {
        return Ok(false);
    }
    Ok(flags(state)?[controller.opponent().index()])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{self, ProposedEvent};
    use crate::policy_observation_v6::tests::ready_state;

    #[test]
    fn old_catalog_absence_preserves_state_bytes_and_primary_hashes() {
        if enabled() {
            return;
        }
        let mut state = ready_state();
        let bytes = serde_json::to_vec(&state).unwrap();
        let hash = state.diagnostic_state_hash();
        initialize_for_catalog(&mut state);
        reset_at_untap(&mut state);
        capture_committed_loss_at(&mut state, 0);
        assert_eq!(serde_json::to_vec(&state).unwrap(), bytes);
        assert_eq!(state.diagnostic_state_hash(), hash);
    }
    #[test]
    fn anchored_history_primitive_uses_actual_loss_payment_gain_and_zero_occurrences() {
        // A typed ledger primitive, without a fabricated printed trigger or source.
        for active in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state();
            state.active_player = active;
            begin_turn(&mut state);
            for proposed in [
                ProposedEvent::life_gain(active.opponent(), 3),
                ProposedEvent::life_loss(active.opponent(), 0),
                ProposedEvent::life_payment(active.opponent(), 2),
            ] {
                event::propose_and_commit(&mut state, proposed);
            }
            state.life_loss_turn_v1.as_mut().unwrap().players[active.opponent().index()] = true;
            assert!(flags(&state).unwrap()[active.opponent().index()]);
            let mut replay: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            assert_eq!(flags(&replay), flags(&state));
            replay.life_loss_turn_v1.as_mut().unwrap().players[active.index()] = true;
            assert!(flags(&replay).is_err());
            replay.life_loss_turn_v1 = None;
            assert!(flags(&replay).is_err());
            state.active_player = active.opponent();
            assert!(flags(&state).is_err());
            begin_turn(&mut state);
            assert_eq!(flags(&state).unwrap(), [false; 2]);
        }
    }
}
