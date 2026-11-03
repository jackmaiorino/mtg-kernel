//! `x_kernel_infoset_v1` on the Spellbench bridge (W1 design v7, sections 5,
//! 6.2 and 6.7; collab LANES/lead/W1-ENTRANT-DESIGN.md).
//!
//! At every decision whose menu holds at least two candidates, two of them
//! not mana abilities (the W1 entrant's eligible decisions), the bridge runs
//! the producer on its own flat-action lockstep session (one engine: the
//! session the bridge answers from) for the acting seat and attaches either
//! the payload or the refusal predicate. A refusal is viewer-derived, so its
//! code may travel with the decision; the entrant answers it with its
//! policy. An invariant failure (I1 to I4) is engine fault F1: the game
//! halts, non-natural, for both seats.
//!
//! Receipts stay engine-side: per-seat tallies by predicate are written to
//! the bridge's stderr when the game ends, never into a decision.

use crate::ids::PlayerId;
use crate::infoset_v1::{
    export_v1, ExportErrorV1, ExportInputV1, ExportOptionsV1, PayloadV1, RefusalTallyV1,
    INFOSET_SCHEMA_V1,
};
use crate::rl::{ActionSemanticV1, PlayerSeatV1};
use crate::rl_session::{FastActorSessionV1, RlSessionDecisionV1};
use crate::runtime_decks::runtime_deck_by_id;
use serde_json::{json, Value};

/// The extension key and its command-line flag.
pub const SPELLBENCH_EXTENSION_KERNEL_INFOSET_V1: &str = "x_kernel_infoset_v1";
pub const SPELLBENCH_FLAG_X_KERNEL_INFOSET_V1: &str = "--x-kernel-infoset-v1";

/// Per-game producer state of one bridge game.
#[derive(Debug, Clone, Default)]
pub(crate) struct InfosetBridgeV1 {
    /// The extension of the decision being sent, if it is eligible.
    pub(crate) pending: Option<Value>,
    /// Eligible decisions so far, per seat (the payload's viewer-local
    /// decision index).
    pub(crate) eligible: [u64; 2],
    /// Exports and refusals by predicate, per seat.
    pub(crate) tally: [RefusalTallyV1; 2],
    /// Whether the game-end receipt was written.
    pub(crate) receipt_written: bool,
}

fn seat(p: PlayerSeatV1) -> PlayerId {
    match p {
        PlayerSeatV1::P0 => PlayerId::P0,
        PlayerSeatV1::P1 => PlayerId::P1,
    }
}

/// Section 5: at least two candidates, at least two of them not mana
/// abilities.
pub(crate) fn eligible_v1(decision: &RlSessionDecisionV1) -> bool {
    let non_mana = decision
        .legal_actions
        .iter()
        .filter(|a| !matches!(a.semantic, ActionSemanticV1::ActivateManaAbility { .. }))
        .count();
    decision.legal_actions.len() >= 2 && non_mana >= 2
}

/// Runs the producer for `decision` on the lockstep session `fast` and
/// stores the extension in `state.pending`. An Err is the halted reason
/// (engine fault F1).
pub(crate) fn infoset_extension_v1(
    fast: &FastActorSessionV1,
    decision: &RlSessionDecisionV1,
    state: &mut InfosetBridgeV1,
) -> Result<(), String> {
    state.pending = None;
    if !eligible_v1(decision) {
        return Ok(());
    }
    let viewer = seat(decision.acting_player);
    let index = state.eligible[viewer.index()];
    state.eligible[viewer.index()] += 1;
    let lists = fast.deck_ids_v1().clone().map(|id| {
        runtime_deck_by_id(&id)
            .map(|d| d.card_ids.to_vec())
            .unwrap_or_default()
    });
    let game = fast.game_state();
    let exported = export_v1(
        &ExportInputV1 {
            state: game,
            journal: game.observation_journal.notes(),
            viewer,
            decklists: [&lists[0], &lists[1]],
        },
        ExportOptionsV1::default(),
    );
    let tally = &mut state.tally[viewer.index()];
    let value = match exported {
        Ok(export) => match PayloadV1::new(&export, fast, viewer, index) {
            Some(payload) => {
                tally.record(&Ok(()));
                // JSON text, like `x_kernel_v5`: the canonical state carries
                // u64 values (the RNG state, hashes) beyond the wire's 2^53
                // integer bound.
                json!({
                    "schema": INFOSET_SCHEMA_V1,
                    "payload_json": String::from_utf8(payload.to_bytes())
                        .map_err(|_| "engine_contract_failure:infoset payload".to_string())?,
                })
            }
            None => {
                // The live surface names objects mid-scan (R2).
                let refused = ExportErrorV1::Refused {
                    code: crate::infoset_v1::RefusalCodeV1::R2,
                    detail: "surface mid-scan".into(),
                };
                tally.record(&Err(refused));
                json!({"schema": INFOSET_SCHEMA_V1, "refusal": "R2"})
            }
        },
        Err(error @ ExportErrorV1::Refused { .. }) => {
            let code = error.code_name();
            tally.record(&Err(error.clone()));
            json!({"schema": INFOSET_SCHEMA_V1, "refusal": code})
        }
        Err(ExportErrorV1::Fault { code, detail }) => {
            return Err(format!("engine_fault:infoset:{code:?}: {detail}"));
        }
    };
    state.pending = Some(value);
    Ok(())
}

/// Writes the engine-side receipt line to stderr once, when a game ends.
pub(crate) fn write_receipt_v1(game_id: &str, state: &mut InfosetBridgeV1) {
    state.pending = None;
    if !state.receipt_written {
        state.receipt_written = true;
        eprintln!("{}", receipt_v1(game_id, state));
    }
}

/// The engine-side receipt line written when a game ends.
pub(crate) fn receipt_v1(game_id: &str, state: &InfosetBridgeV1) -> String {
    json!({
        "receipt": "x_kernel_infoset_v1",
        "game_id": game_id,
        "eligible": state.eligible,
        "tally": [&state.tally[0], &state.tally[1]],
    })
    .to_string()
}
