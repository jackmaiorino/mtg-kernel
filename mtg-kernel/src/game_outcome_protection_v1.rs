//! Printed game-outcome prohibitions. No new mutable state or inferred keyword.
use crate::card_def::{GameOutcomeProtectionDefV1, CARD_DEFS};
use crate::ids::{ObjectId, PlayerId};
use crate::state::{GameState, Zone};

fn enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        CARD_DEFS
            .iter()
            .any(|def| def.is_executable() && def.game_outcome_protection_v1.is_some())
    })
}

pub fn active(state: &GameState, source: ObjectId) -> bool {
    state.objects.try_get(source).is_some_and(|object| {
        object.zone == Zone::Battlefield
            && object.v4.face_index == 0
            && CARD_DEFS
                .get(usize::from(object.card_def))
                .is_some_and(|def| {
                    def.is_executable()
                    && def.game_outcome_protection_v1
                        == Some(
                            GameOutcomeProtectionDefV1::ControllerCannotLoseAndOpponentsCannotWin,
                        )
                })
            && crate::continuous_characteristics_v1::printed_abilities_active(state, source)
    })
}

pub fn cannot_lose(state: &GameState, player: PlayerId) -> bool {
    if !enabled() {
        return false;
    }
    state
        .objects
        .iter()
        .any(|(source, object)| object.controller == player && active(state, source))
}

pub fn cannot_win(state: &GameState, player: PlayerId) -> bool {
    cannot_lose(state, player.opponent())
}

pub(crate) fn protected_players(state: &GameState) -> [bool; 2] {
    [
        cannot_lose(state, PlayerId::P0),
        cannot_lose(state, PlayerId::P1),
    ]
}

/// A prevented empty draw is checked at this SBA boundary, not retained as a
/// later loss when its protecting permanent leaves. Life and poison persist.
pub(crate) fn consume_protected_empty_draws(state: &mut GameState, protected: [bool; 2]) -> bool {
    let mut changed = false;
    for player in [PlayerId::P0, PlayerId::P1] {
        let status = &mut state.players[player.index()];
        if protected[player.index()] && status.drew_from_empty {
            status.drew_from_empty = false;
            changed = true;
        }
    }
    changed
}

#[cfg(test)]
mod tests;
