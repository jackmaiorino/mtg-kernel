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
use crate::infoset_v1::order::{canonical_ids_v1, imported_ids_v1, live_rows_v1, row_keys_v1};
use crate::infoset_v1::{
    export_reveal_v4_from_journal, export_v1, ExportErrorV1, ExportInputV1, ExportOptionsV1,
    PayloadV1, RefusalTallyV1, INFOSET_SCHEMA_REVEAL_V4, INFOSET_SCHEMA_V1,
};
use crate::reveal_meaning_v3::RevealJournalV3;
use crate::rl::{ActionSemanticV1, PlayerSeatV1};
use crate::rl_session::{FastActorSessionV1, RlSessionDecisionV1};
use crate::runtime_decks::runtime_deck_by_id;
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// The extension key and its command-line flag.
pub const SPELLBENCH_EXTENSION_KERNEL_INFOSET_V1: &str = "x_kernel_infoset_v1";
pub const SPELLBENCH_FLAG_X_KERNEL_INFOSET_V1: &str = "--x-kernel-infoset-v1";
pub const SPELLBENCH_FLAG_X_KERNEL_INFOSET_REVEAL_V4: &str = "--x-kernel-infoset-reveal-v4";

/// Per-game producer state of one bridge game.
#[derive(Debug, Clone, Default)]
pub(crate) struct InfosetBridgeV1 {
    reveal_v4: bool,
    reveal_group_v4: Option<RevealGroupRootV4>,
    /// The extension of the decision being sent, if it is eligible.
    pub(crate) pending: Option<Value>,
    /// Eligible decisions so far, per seat (the payload's viewer-local
    /// decision index).
    pub(crate) eligible: [u64; 2],
    /// Exports and refusals by predicate, per seat.
    pub(crate) tally: [RefusalTallyV1; 2],
    /// Whether the game-end receipt was written.
    pub(crate) receipt_written: bool,
    /// The canonical id map of the last exported decision, with that
    /// decision's step: private previews of a buffered group rooted there
    /// key their rows with it (design 7).
    pub(crate) root_ids: Option<(u64, BTreeMap<u64, u64>)>,
    /// Exported decisions whose menus the canonical keys could not pair
    /// one to one with the import's, per seat (engine-side receipt).
    pub(crate) order_map_missing: [u64; 2],
}

#[derive(Debug, Clone)]
struct RevealGroupRootV4 {
    identity: (PlayerSeatV1, String, String),
    step: u64,
    ids: BTreeMap<u64, u64>,
}

impl InfosetBridgeV1 {
    pub(crate) fn reveal_v4() -> Self {
        Self {
            reveal_v4: true,
            ..Self::default()
        }
    }

    pub(crate) fn extension(&self) -> &'static str {
        if self.reveal_v4 {
            INFOSET_SCHEMA_REVEAL_V4
        } else {
            SPELLBENCH_EXTENSION_KERNEL_INFOSET_V1
        }
    }

    pub(crate) fn live_group_rows_v4(
        &mut self,
        actor: PlayerSeatV1,
        support: &Value,
        fast: &FastActorSessionV1,
    ) -> Option<Value> {
        if !self.reveal_v4 {
            return None;
        }
        let identity = if let Some(instance) = support["block_declaration_instance"].as_str() {
            Some((actor, "block".into(), instance.to_owned()))
        } else {
            let purpose = support["choice"]["purpose"].as_str().filter(|&purpose| {
                matches!(
                    purpose,
                    "scry"
                        | "look_select"
                        | "look"
                        | "reveal_partition"
                        | "mill"
                        | "put_on_top"
                        | "discard_order"
                )
            });
            purpose
                .zip(support["effect_instance"].as_str())
                .map(|(purpose, instance)| (actor, purpose.to_owned(), instance.to_owned()))
        };
        let Some(identity) = identity else {
            self.reveal_group_v4 = None;
            return None;
        };
        if self
            .reveal_group_v4
            .as_ref()
            .is_none_or(|root| root.identity != identity)
        {
            self.reveal_group_v4 = self.root_ids.as_ref().map(|(step, ids)| RevealGroupRootV4 {
                identity,
                step: *step,
                ids: ids.clone(),
            });
        }
        let root = self.reveal_group_v4.as_ref()?;
        Some(json!({"root_step": root.step, "row_keys": row_keys_v1(fast, Some(&root.ids))}))
    }
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
    eligible_semantics_v1(decision.legal_actions.iter().map(|a| &a.semantic))
}

/// Design 5's eligibility on a menu's semantics: at least two candidates,
/// at least two of them not mana abilities.
pub(crate) fn eligible_semantics_v1<'a>(
    semantics: impl IntoIterator<Item = &'a ActionSemanticV1>,
) -> bool {
    let (mut all, mut non_mana) = (0usize, 0usize);
    for semantic in semantics {
        all += 1;
        non_mana += usize::from(!matches!(
            semantic,
            ActionSemanticV1::ActivateManaAbility { .. }
        ));
    }
    all >= 2 && non_mana >= 2
}

/// What the bridge delivers at an eligible decision of `viewer` whose
/// viewer-local index is `index`: the wire extension and what it records.
/// A pure function of the session (the bridge and AC1 both call it).
pub(crate) enum DeliveredV1 {
    Payload {
        value: Value,
        ids: BTreeMap<u64, u64>,
        map_missing: bool,
    },
    Refused {
        value: Value,
        error: ExportErrorV1,
        subclass: &'static str,
    },
}

/// The extension `viewer` receives at the current (eligible) decision; an
/// engine fault is an error.
pub(crate) fn delivered_v1(
    fast: &FastActorSessionV1,
    viewer: PlayerId,
    index: u64,
) -> Result<DeliveredV1, String> {
    delivered(fast, viewer, index, None)
}

fn delivered(
    fast: &FastActorSessionV1,
    viewer: PlayerId,
    index: u64,
    reveal: Option<&RevealJournalV3>,
) -> Result<DeliveredV1, String> {
    let lists = fast.deck_ids_v1().clone().map(|id| {
        runtime_deck_by_id(&id)
            .map(|d| d.card_ids.to_vec())
            .unwrap_or_default()
    });
    let game = fast.game_state();
    let input = ExportInputV1 {
        state: game,
        journal: game.observation_journal.notes(),
        viewer,
        decklists: [&lists[0], &lists[1]],
        marks: Some(&fast.infoset_marks_v1(viewer)),
    };
    let (schema, exported) = match reveal {
        Some(journal) => (
            INFOSET_SCHEMA_REVEAL_V4,
            export_reveal_v4_from_journal(&input, journal),
        ),
        None => (
            INFOSET_SCHEMA_V1,
            export_v1(&input, ExportOptionsV1::default()),
        ),
    };
    Ok(match exported {
        Ok(export) => match PayloadV1::new(&export, fast, viewer, index) {
            Some(payload) => {
                // The order map (design 4 and 7): the entrant's pick is an
                // index into the imported session's menu, which lists the
                // candidates in canonical-id order; `live_rows[i]` is the
                // lockstep row (the `x_kernel_flat_v4` row) of imported row
                // i, paired by canonical key. Null when the keys do not
                // pair the menus one to one: the entrant then answers with
                // its policy. `row_keys` keys the lockstep rows, for a
                // buffered group's completions.
                let ids = canonical_ids_v1(&export);
                let lockstep_keys = row_keys_v1(fast, Some(&ids));
                let imported_ids = imported_ids_v1(&payload.handles);
                let imported_keys = payload
                    .import()
                    .ok()
                    .and_then(|s| row_keys_v1(&s, Some(&imported_ids)));
                let live_rows = match (&lockstep_keys, &imported_keys) {
                    (Some(a), Some(b)) => live_rows_v1(a, b),
                    _ => None,
                };
                // JSON text, like `x_kernel_v5`: the canonical state carries
                // u64 values (the RNG state, hashes) beyond the wire's 2^53
                // integer bound.
                DeliveredV1::Payload {
                    map_missing: live_rows.is_none(),
                    value: json!({
                        "schema": schema,
                        "payload_json": String::from_utf8(payload.to_bytes())
                            .map_err(|_| "engine_contract_failure:infoset payload".to_string())?,
                        "live_rows": live_rows,
                        "row_keys": lockstep_keys,
                    }),
                    ids,
                }
            }
            None => {
                // The live surface names objects the payload cannot carry
                // (R2); the subclass names that surface state (amendment
                // 43.1 part 2), so every R2 can be classified.
                let subclass = fast.payload_refusal_detail_v1(viewer);
                DeliveredV1::Refused {
                    value: json!({"schema": schema, "refusal": "R2", "refusal_detail": subclass}),
                    error: ExportErrorV1::Refused {
                        code: crate::infoset_v1::RefusalCodeV1::R2,
                        detail: format!("surface: {subclass}"),
                    },
                    subclass,
                }
            }
        },
        Err(error @ ExportErrorV1::Refused { .. }) => {
            // An export refusal's subclass is fixed by its code (the free
            // text stays engine-side): R3 says whose plan, any other code
            // `export`.
            let subclass = export_refusal_subclass_v1(&error);
            DeliveredV1::Refused {
                value: json!({"schema": schema, "refusal": error.code_name(), "refusal_detail": subclass}),
                error,
                subclass,
            }
        }
        Err(ExportErrorV1::Fault { code, detail }) => {
            return Err(format!("engine_fault:infoset:{code:?}: {detail}"));
        }
    })
}

pub(crate) fn infoset_extension_v1(
    fast: &FastActorSessionV1,
    decision: &RlSessionDecisionV1,
    state: &mut InfosetBridgeV1,
    reveal: &RevealJournalV3,
) -> Result<(), String> {
    state.pending = None;
    state.root_ids = None;
    if !eligible_v1(decision) {
        return Ok(());
    }
    let viewer = seat(decision.acting_player);
    let index = state.eligible[viewer.index()];
    state.eligible[viewer.index()] += 1;
    let seat_index = viewer.index();
    let result = if state.reveal_v4 {
        delivered(fast, viewer, index, Some(reveal))?
    } else {
        delivered_v1(fast, viewer, index)?
    };
    let value = match result {
        DeliveredV1::Payload {
            value,
            ids,
            map_missing,
        } => {
            state.tally[seat_index].record(&Ok(()));
            if map_missing {
                state.order_map_missing[seat_index] += 1;
            }
            state.root_ids = Some((decision.step, ids));
            value
        }
        DeliveredV1::Refused {
            value,
            error,
            subclass,
        } => {
            state.tally[seat_index].record_refusal(&error, subclass);
            if !state.reveal_v4 {
                log_refusal_v1(fast, viewer, decision.step, &error, subclass);
            }
            value
        }
    };
    state.pending = Some(value);
    Ok(())
}

/// The environment variable naming a directory for engine-side refusal
/// diagnostics (`refusals-<pid>.jsonl`): one line per refusal with the
/// export's free-text detail and, for R1, the live references that name
/// untracked objects. Diagnostics only; nothing reaches the wire.
pub(crate) const REFUSAL_LOG_ENV_V1: &str = "W1_INFOSET_REFUSAL_LOG";

fn log_refusal_v1(
    fast: &FastActorSessionV1,
    viewer: PlayerId,
    step: u64,
    error: &ExportErrorV1,
    subclass: &str,
) {
    let Some(dir) = std::env::var_os(REFUSAL_LOG_ENV_V1).filter(|d| !d.is_empty()) else {
        return;
    };
    let detail = match error {
        ExportErrorV1::Refused { detail, .. } | ExportErrorV1::Fault { detail, .. } => detail,
    };
    let mut line = json!({"step": step, "viewer": viewer.0, "turn": fast.game_state().turn,
                          "code": error.code_name(), "subclass": subclass, "detail": detail});
    let r2_order = error.code_name() == "R2"
        && detail.as_str() == crate::infoset_v1::export::R2_ORDER_DETAIL_V1;
    if error.code_name() == "R1" || r2_order {
        let lists = fast.deck_ids_v1().clone().map(|id| {
            runtime_deck_by_id(&id)
                .map(|d| d.card_ids.to_vec())
                .unwrap_or_default()
        });
        let game = fast.game_state();
        let marks = fast.infoset_marks_v1(viewer);
        let input = ExportInputV1 {
            state: game,
            journal: game.observation_journal.notes(),
            viewer,
            decklists: [&lists[0], &lists[1]],
            marks: Some(&marks),
        };
        line["diagnostics"] = if r2_order {
            crate::infoset_v1::export::r2_order_diagnostics_v1(&input)
        } else {
            crate::infoset_v1::export::refusal_diagnostics_v1(&input)
        };
    }
    let path = std::path::Path::new(&dir).join(format!("refusals-{}.jsonl", std::process::id()));
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        use std::io::Write as _;
        let _ = writeln!(f, "{line}");
    }
}

/// The wire subclass of an export refusal: R3 names whose plan is pending
/// (`opponent`, the design 6.7 predicate; `plan_error`, a search the planner
/// could not read), R3b `own_library_search`, R1 `same_name_batch` (lead
/// ruling 7 (a)), `same_name_hand` (amendment 43.7) and
/// `same_name_hand_reveal` (lead ruling 19), every other code
/// `export`.
pub(crate) fn export_refusal_subclass_v1(error: &ExportErrorV1) -> &'static str {
    match error {
        ExportErrorV1::Refused {
            code: crate::infoset_v1::RefusalCodeV1::R3,
            detail,
        } if detail == crate::infoset_v1::export::R3_OPPONENT_DETAIL => "opponent",
        ExportErrorV1::Refused {
            code: crate::infoset_v1::RefusalCodeV1::R3,
            ..
        } => "plan_error",
        ExportErrorV1::Refused {
            code: crate::infoset_v1::RefusalCodeV1::R3b,
            ..
        } => "own_library_search",
        // Lead ruling 7 (a): two same-name objects left in one batch of V's
        // transcript and a live reference names one of them. A C5 fault.
        ExportErrorV1::Refused {
            code: crate::infoset_v1::RefusalCodeV1::R1,
            detail,
        } if detail == crate::infoset_v1::export::R1_SAME_NAME_BATCH_DETAIL => "same_name_batch",
        // Amendment 43.7 (Sol round 8, finding 2): a live reference made at
        // an opponent-hand epoch (not the object's own cast receipt). A C5
        // fault, tallied apart.
        ExportErrorV1::Refused {
            code: crate::infoset_v1::RefusalCodeV1::R1,
            detail,
        } if detail == crate::infoset_v1::export::R1_SAME_NAME_HAND_DETAIL => "same_name_hand",
        // Lead ruling 19: the reveal-triggered rule is the sole refusing
        // test. Ineligible by definition, tallied apart; not a C5 fault.
        ExportErrorV1::Refused {
            code: crate::infoset_v1::RefusalCodeV1::R1,
            detail,
        } if detail == crate::infoset_v1::export::R1_SAME_NAME_HAND_REVEAL_DETAIL => {
            "same_name_hand_reveal"
        }
        _ => "export",
    }
}

/// The environment variable naming a directory for the engine-side receipt
/// files (`<game_id>.bridge.json`), read by the availability reconciliation.
pub(crate) const RECEIPT_DIR_ENV_V1: &str = "W1_INFOSET_RECEIPT_DIR";

/// Writes the engine-side receipt line to stderr once, when a game ends, and
/// to `$W1_INFOSET_RECEIPT_DIR/<game_id>.bridge.json` when that is set.
pub(crate) fn write_receipt_v1(game_id: &str, state: &mut InfosetBridgeV1) {
    state.pending = None;
    if !state.receipt_written {
        state.receipt_written = true;
        let line = receipt_v1(game_id, state);
        eprintln!("{line}");
        let receipt_dir = if state.reveal_v4 {
            "REVEAL_V4_INFOSET_RECEIPT_DIR"
        } else {
            RECEIPT_DIR_ENV_V1
        };
        if let Some(dir) = std::env::var_os(receipt_dir).filter(|d| !d.is_empty()) {
            let safe: String = game_id
                .chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() || c == '-' {
                        c
                    } else {
                        '_'
                    }
                })
                .collect();
            let path = std::path::Path::new(&dir).join(format!("{safe}.bridge.json"));
            if let Err(e) = std::fs::write(&path, format!("{line}\n")) {
                eprintln!("{} receipt {}: {e}", state.extension(), path.display());
            }
        }
    }
}

/// The engine-side receipt line written when a game ends.
pub(crate) fn receipt_v1(game_id: &str, state: &InfosetBridgeV1) -> String {
    json!({
        "receipt": state.extension(),
        "game_id": game_id,
        "eligible": state.eligible,
        "tally": [&state.tally[0], &state.tally[1]],
        "order_map_missing": state.order_map_missing,
    })
    .to_string()
}

/// The canonical keys of a private preview's rows, when the group's root
/// decision (at `root_step`) carried a payload: the rows of a buffered
/// group's later native points, keyed with the root export's id map.
pub(crate) fn preview_row_keys_v1(
    state: &InfosetBridgeV1,
    root_step: u64,
    fast: &FastActorSessionV1,
) -> Option<Value> {
    let (step, ids) = state.root_ids.as_ref()?;
    if *step != root_step {
        return None;
    }
    Some(json!({"schema": state.extension(), "row_keys": row_keys_v1(fast, Some(ids))}))
}
