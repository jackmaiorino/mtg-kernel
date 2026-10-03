//! Spellbench `spellbench/v1` environment-role bridge over the in-process
//! kernel RL session.
//!
//! This module is the mtg-kernel adapter for the cross-engine Spellbench
//! tournament protocol (`SPELLBENCH_PROTOCOL_V1.md`, the external authority;
//! `docs/contracts/AGENT_BRIDGE_V1.md` is this adapter's contract). It serves
//! the environment role only: `hello`, `reset`, and `step`, one game per
//! process, strict NDJSON over stdio. Every wire decision is converted from
//! the kernel's policy-v5 surface (`ActionSemanticV1` / `ObservationV5` /
//! `LegalActionV5`) into the neutral candidate semantics of the protocol, and
//! every selection is bound back to the exact kernel candidate by dense index
//! plus `stable_id` after a field-for-field semantic-echo recomputation.
//!
//! Fail-closed rules:
//! - Anything the protocol does not license is rejected with the closed
//!   error-code table; no unknown `kind`, format, field, or deck is guessed.
//! - The refused `ActionSemanticV1::Ambiguous` variant (and the legacy
//!   aggregate combat variants, already forbidden on policy surface v5) are
//!   never emitted; an unrepresentable candidate fails the whole game as
//!   `halted` rather than emitting an unlicensed semantic.
//! - Retransmission safety is a single-entry byte-exact request-id cache, the
//!   same discipline as `KernelRlJsonlServerV1`.

use crate::agent_bridge_flat_v4::{FlatV4LockstepErrorV1, FlatV4LockstepV1};
#[cfg(test)]
use crate::agent_bridge_flat_v4_encode::FlatV4EncodeErrorV1;
use crate::agent_bridge_flat_v4_encode::{
    encode_flat_v4_tensor_v1, flat_v4_extension_value_v1, FlatV4EncodeScratchV1,
    SPELLBENCH_EXTENSION_KERNEL_FLAT_V4,
};
use crate::card_def::{CARD_DEFS, KERNEL_CARDDB_HASH};
use crate::engine::{CastMode, CostKind, OptionalCostChoice};
use crate::mana::ManaColor;
use crate::rl::{
    parse_strict_json_value, ActionSemanticV1, CardStableRefV1, ObservationV5,
    PendingEffectChoiceSemanticV4, PendingEffectSemanticV4, PlayerSeatV1, TargetRefV1,
    TerminalClassificationV1, TerminalOutcomeV1, ZoneIndependentStepV1,
};
use crate::rl_session::{
    RlEpisodeSessionV1, RlSessionDecisionV1, RlSessionError, RlSessionErrorCode,
    RlSessionResponseV1, RlSessionTerminalV1, SessionDeckIdsV1,
};
use crate::state::Zone;
use crate::KERNEL_VERSION;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::io::{self, BufRead, Read};

/// Protocol string carried by every envelope.
pub const SPELLBENCH_PROTOCOL_V1: &str = "spellbench/v1";
/// Section 2 framing cap: a received line longer than this is rejected.
pub const SPELLBENCH_MAX_LINE_BYTES_V1: usize = 8 * 1024 * 1024;
/// The single format this engine serves.
pub const SPELLBENCH_FORMAT_PAUPER_BO1_V1: &str = "pauper-bo1";
/// The extension key every decision carries (raw v5 payloads, Section 9);
/// `x_kernel_flat_v4` is opt-in ([`SpellbenchBridgeOptionsV1`]).
pub const SPELLBENCH_EXTENSION_KERNEL_V5: &str = "x_kernel_v5";
/// Process environment variable supplying `engine.source_revision`.
pub const SPELLBENCH_SOURCE_REVISION_ENV_V1: &str = "SPELLBENCH_SOURCE_REVISION";
/// Catalog decks refused with `unsupported_deck`: they stage actions (Escape)
/// that policy surface v5 cannot represent, so most of their games halt and
/// the rest would be a biased sample for any rating.
pub const SPELLBENCH_POLICY_V5_UNPLAYABLE_DECKS_V1: &[&str] = &["Terror"];
/// Deterministic `game_id` (host string) to `episode_id` (kernel u64)
/// mapping identity: FNV-1a 64 over the game id's UTF-8 bytes. One game per
/// process makes collisions across game ids unreachable within a session.
pub const SPELLBENCH_GAME_ID_TO_EPISODE_V1: &str = "fnv1a64-utf8-game-id/v1";
/// Opaque object id derivation: `obj-<arena_id>-z<zone_change_count>`,
/// decimal, zero-padded (`%06` / `%04`). Reissued on every zone change, which
/// satisfies the spec's stale-id prohibition and reissue recommendation.
pub const SPELLBENCH_OBJECT_ID_DERIVATION_V1: &str = "kernel-arena-id-zone-change-count/v1";
/// Command-line flag enabling `x_kernel_flat_v4`.
pub const SPELLBENCH_FLAG_X_KERNEL_FLAT_V4: &str = "--x-kernel-flat-v4";
pub const SPELLBENCH_FLAG_V2_SUPPORT: &str = "--private-v2-support";

/// Opt-in behaviors of one bridge process.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SpellbenchBridgeOptionsV1 {
    /// Run the lockstep training-mode copy and attach `x_kernel_flat_v4`.
    pub x_kernel_flat_v4: bool,
    /// Private wrapper metadata. Never forwarded to an agent as an extension.
    /// It observes stack instance IDs without changing the frozen RL schema.
    pub private_v2_support: bool,
}

fn fnv1a64_utf8_v1(text: &str) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for &byte in text.as_bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// The kernel episode id a Spellbench game id maps to (Section 2).
pub fn spellbench_episode_id_v1(game_id: &str) -> u64 {
    fnv1a64_utf8_v1(game_id)
}

/// Engine identity reported by `hello_ok` and repeated as decision/terminal
/// `provenance`. Captured once per process; `source_revision` comes from
/// `SPELLBENCH_SOURCE_REVISION` (absent or empty maps to null).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpellbenchEngineIdentityV1 {
    pub name: String,
    pub version: String,
    pub source_revision: Option<String>,
    pub rules_snapshot_id: String,
    pub card_pool_identity: String,
}

impl SpellbenchEngineIdentityV1 {
    /// Production identity: reads `SPELLBENCH_SOURCE_REVISION` from the
    /// process environment.
    pub fn current() -> Self {
        Self::with_source_revision(
            std::env::var(SPELLBENCH_SOURCE_REVISION_ENV_V1)
                .ok()
                .filter(|value| !value.is_empty()),
        )
    }

    /// Explicit identity constructor; tests inject the revision directly so
    /// parallel test processes never race on the shared environment.
    pub fn with_source_revision(source_revision: Option<String>) -> Self {
        let carddb = format!("{KERNEL_CARDDB_HASH:016x}");
        SpellbenchEngineIdentityV1 {
            name: "mtg-kernel".to_string(),
            version: KERNEL_VERSION.to_string(),
            source_revision,
            rules_snapshot_id: format!("mtg-kernel-rules/{KERNEL_VERSION}/carddb-{carddb}"),
            card_pool_identity: format!("mtg-kernel-pauper-pool-v1/carddb-{carddb}"),
        }
    }

    fn hello_engine_value(&self) -> Value {
        json!({
            "name": self.name,
            "version": self.version,
            "source_revision": self.source_revision,
            "rules_snapshot_id": self.rules_snapshot_id,
            "card_pool_identity": self.card_pool_identity,
        })
    }

    fn provenance_value(&self) -> Value {
        json!({
            "engine_name": self.name,
            "engine_version": self.version,
            "rules_snapshot_id": self.rules_snapshot_id,
            "card_pool_identity": self.card_pool_identity,
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "request_type", rename_all = "snake_case", deny_unknown_fields)]
enum SpellbenchRequestV1 {
    Hello {
        // Validated against the raw value before typed decode; carried here so
        // the field is licensed rather than rejected as unknown.
        #[serde(rename = "protocol")]
        _protocol: String,
        request_id: String,
    },
    Reset {
        #[serde(rename = "protocol")]
        _protocol: String,
        request_id: String,
        game_id: String,
        format: String,
        seats: [SpellbenchResetSeatV1; 2],
        game_seed: u64,
        max_decisions: u64,
        max_steps: u64,
    },
    Step {
        #[serde(rename = "protocol")]
        _protocol: String,
        request_id: String,
        game_id: String,
        expected_step: u64,
        selection: SpellbenchSelectionV1,
    },
    /// Private decomposition view on a clone. Never an agent wire method.
    PreviewSteps {
        #[serde(rename = "protocol")]
        _protocol: String,
        request_id: String,
        game_id: String,
        expected_step: u64,
        candidate_ids: Vec<u32>,
    },
}

impl SpellbenchRequestV1 {
    fn request_id(&self) -> &str {
        match self {
            SpellbenchRequestV1::Hello { request_id, .. }
            | SpellbenchRequestV1::Reset { request_id, .. }
            | SpellbenchRequestV1::Step { request_id, .. }
            | SpellbenchRequestV1::PreviewSteps { request_id, .. } => request_id,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SpellbenchResetSeatV1 {
    seat: String,
    deck: SpellbenchResetDeckV1,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SpellbenchResetDeckV1 {
    catalog_id: Option<String>,
    decklist: Option<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SpellbenchSelectionV1 {
    candidate_id: u32,
    semantic_echo: Value,
}

/// Borrowed reset payload for dispatch, keeping handler arity low.
struct SpellbenchResetFieldsV1<'a> {
    game_id: &'a str,
    format: &'a str,
    seats: &'a [SpellbenchResetSeatV1; 2],
    game_seed: u64,
    max_decisions: u64,
    max_steps: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CachedSpellbenchExchangeV1 {
    // Request ids are process-unique except for one immediate identical retry.
    // The cache is deliberately one entry, matching KernelRlJsonlServerV1.
    request_id: String,
    /// Byte-exact request payload as received (line terminators stripped).
    request_line: String,
    response_line: String,
}

enum RetryDispositionV1 {
    Cached,
    ReuseMismatch,
}

struct SpellbenchBridgeGameV1 {
    game_id: String,
    episode_id: u64,
    session: RlEpisodeSessionV1,
    terminal_sent: bool,
    flat_v4: Option<FlatV4LockstepV1>,
    /// Row map of the pending decision (row r executes candidate rows[r]).
    flat_v4_rows: Vec<u32>,
    flat_v4_scratch: FlatV4EncodeScratchV1,
    /// Private declaration identity, retained across all attacker-major scans.
    private_block_root_step: Option<u64>,
    #[cfg(test)]
    force_flat_v4_encode_error: bool,
}

/// In-process spellbench/v1 environment-role server. The stdio binary is a
/// thin framing shell around [`SpellbenchBridgeServerV1::handle_line`].
pub struct SpellbenchBridgeServerV1 {
    identity: SpellbenchEngineIdentityV1,
    options: SpellbenchBridgeOptionsV1,
    game: Option<SpellbenchBridgeGameV1>,
    used_game_ids: HashSet<String>,
    last_exchange: Option<CachedSpellbenchExchangeV1>,
}

impl Default for SpellbenchBridgeServerV1 {
    fn default() -> Self {
        Self::new()
    }
}

impl SpellbenchBridgeServerV1 {
    pub fn new() -> Self {
        Self::with_options(
            SpellbenchEngineIdentityV1::current(),
            SpellbenchBridgeOptionsV1::default(),
        )
    }

    /// Test constructor with an explicit identity (no environment read).
    pub fn with_engine_identity(identity: SpellbenchEngineIdentityV1) -> Self {
        Self::with_options(identity, SpellbenchBridgeOptionsV1::default())
    }

    /// Server with an explicit identity and opt-in behaviors (the stdio
    /// binary passes the production identity and its command-line flags).
    pub fn with_options(
        identity: SpellbenchEngineIdentityV1,
        options: SpellbenchBridgeOptionsV1,
    ) -> Self {
        SpellbenchBridgeServerV1 {
            identity,
            options,
            game: None,
            used_game_ids: HashSet::new(),
            last_exchange: None,
        }
    }

    fn identity(&self) -> &SpellbenchEngineIdentityV1 {
        &self.identity
    }

    /// Handles one received line (terminators already stripped by the
    /// framing layer) and returns the exact response line to emit.
    pub fn handle_line(&mut self, line: &str) -> String {
        if line.len() > SPELLBENCH_MAX_LINE_BYTES_V1 {
            return self.uncached_error_line(
                None,
                "malformed_json",
                "request line exceeds the 8 MiB framing limit",
            );
        }
        let value = match parse_strict_json_value(line) {
            Ok(value) => value,
            // Strictness failures (duplicate keys, non-finite or fractional
            // numbers, invalid syntax) are all malformed_json: the line is
            // not valid strict protocol JSON regardless of whether a lenient
            // parser could read it.
            Err(_) => {
                return self.uncached_error_line(
                    None,
                    "malformed_json",
                    "request line is not valid strict JSON",
                );
            }
        };
        if contains_fractional_number_v1(&value) {
            return self.uncached_error_line(
                None,
                "malformed_json",
                "request line contains a fractional number",
            );
        }
        if !value.is_object() {
            return self.uncached_error_line(
                None,
                "malformed_request",
                "request top level must be a JSON object",
            );
        }
        let request_id = request_id_from_value_v1(&value);
        match value.get("protocol") {
            Some(Value::String(protocol)) if protocol == SPELLBENCH_PROTOCOL_V1 => {}
            Some(Value::String(_)) => {
                return self.uncached_error_line(
                    request_id.as_deref(),
                    "protocol_mismatch",
                    "protocol must be spellbench/v1",
                );
            }
            _ => {
                return self.uncached_error_line(
                    request_id.as_deref(),
                    "malformed_request",
                    "protocol must be the string spellbench/v1",
                );
            }
        }
        let request = match serde_json::from_value::<SpellbenchRequestV1>(value) {
            Ok(request) => request,
            Err(_) => {
                return self.uncached_error_line(
                    request_id.as_deref(),
                    "malformed_request",
                    "request does not match the spellbench/v1 environment schema",
                );
            }
        };
        let retry = self.last_exchange.as_ref().and_then(|cached| {
            if cached.request_id != request.request_id() {
                return None;
            }
            if cached.request_line == line {
                Some(RetryDispositionV1::Cached)
            } else {
                Some(RetryDispositionV1::ReuseMismatch)
            }
        });
        match retry {
            Some(RetryDispositionV1::Cached) => {
                return self
                    .last_exchange
                    .as_ref()
                    .expect("cached retry has an exchange")
                    .response_line
                    .clone();
            }
            Some(RetryDispositionV1::ReuseMismatch) => {
                return self.uncached_error_line(
                    Some(request.request_id()),
                    "request_id_reuse_mismatch",
                    "request_id was reused for a different request payload",
                );
            }
            None => {}
        }
        let response = self.dispatch_request(&request);
        let response_line = serialize_response_line_v1(&response);
        self.last_exchange = Some(CachedSpellbenchExchangeV1 {
            request_id: request.request_id().to_string(),
            request_line: line.to_string(),
            response_line: response_line.clone(),
        });
        response_line
    }

    /// Error response for a rejected oversized or non-UTF-8 line, which never
    /// reaches request parsing and is never retry-cached.
    pub fn oversized_line_response(&mut self) -> String {
        self.uncached_error_line(
            None,
            "malformed_json",
            "request line exceeds the 8 MiB framing limit",
        )
    }

    /// Error response for a line that is not valid UTF-8.
    pub fn invalid_utf8_line_response(&mut self) -> String {
        self.uncached_error_line(None, "malformed_json", "request line is not valid UTF-8")
    }

    fn uncached_error_line(&self, request_id: Option<&str>, code: &str, message: &str) -> String {
        serialize_response_line_v1(&error_response_value_v1(request_id, code, message))
    }

    fn dispatch_request(&mut self, request: &SpellbenchRequestV1) -> Value {
        match request {
            SpellbenchRequestV1::Hello { request_id, .. } => {
                let mut extensions = vec![SPELLBENCH_EXTENSION_KERNEL_V5];
                if self.options.x_kernel_flat_v4 {
                    extensions.push(SPELLBENCH_EXTENSION_KERNEL_FLAT_V4);
                }
                json!({
                    "response_type": "hello_ok",
                    "protocol": SPELLBENCH_PROTOCOL_V1,
                    "request_id": request_id,
                    "engine": self.identity().hello_engine_value(),
                    "formats": [SPELLBENCH_FORMAT_PAUPER_BO1_V1],
                    "capabilities": {"decklists_as_data": false},
                    "extensions": extensions,
                })
            }
            SpellbenchRequestV1::Reset {
                request_id,
                game_id,
                format,
                seats,
                game_seed,
                max_decisions,
                max_steps,
                ..
            } => self.handle_reset(
                request_id,
                SpellbenchResetFieldsV1 {
                    game_id,
                    format,
                    seats,
                    game_seed: *game_seed,
                    max_decisions: *max_decisions,
                    max_steps: *max_steps,
                },
            ),
            SpellbenchRequestV1::Step {
                request_id,
                game_id,
                expected_step,
                selection,
                ..
            } => self.handle_step(request_id, game_id, *expected_step, selection),
            SpellbenchRequestV1::PreviewSteps { request_id, game_id, expected_step, candidate_ids, .. } =>
                self.handle_preview_steps(request_id, game_id, *expected_step, candidate_ids),
        }
    }

    fn handle_preview_steps(&self, request_id: &str, game_id: &str, expected_step: u64, ids: &[u32]) -> Value {
        let reject = |message| error_response_value_v1(Some(request_id), "malformed_request", message);
        if !self.options.private_v2_support || !self.options.x_kernel_flat_v4 {
            return reject("private decomposition views require both opt-in bridge flags");
        }
        let Some(game) = self.game.as_ref() else { return reject("private view requires an active game"); };
        let RlSessionResponseV1::Decision(root) = game.session.current_response() else {
            return reject("private view requires a pending decision");
        };
        if game.game_id != game_id || root.step != expected_step || game.terminal_sent || ids.len() > 4096 {
            return reject("private view game, step or prefix bound mismatch");
        }
        let root_support = private_v2_support(game);
        let is_block = root.observation.projection.surface.surface_context.private_blockers.is_some();
        let purpose = root_support["choice"]["purpose"].as_str();
        let arrangement = matches!(purpose, Some("scry" | "look_select" | "look" | "reveal_partition" | "mill" | "put_on_top" | "discard_order"));
        if !is_block && !arrangement {
            return reject("private view is only for an unfinished blocker or arrangement declaration");
        }
        let mut copy = Self::with_options(self.identity.clone(), self.options);
        copy.game = Some(SpellbenchBridgeGameV1 {
            game_id: game.game_id.clone(), episode_id: game.episode_id,
            session: game.session.clone(), terminal_sent: false, flat_v4: game.flat_v4.clone(),
            flat_v4_rows: game.flat_v4_rows.clone(), flat_v4_scratch: FlatV4EncodeScratchV1::default(),
            private_block_root_step: game.private_block_root_step,
            #[cfg(test)] force_flat_v4_encode_error: game.force_flat_v4_encode_error,
        });
        let complete = || json!({"response_type": "private_complete", "protocol": SPELLBENCH_PROTOCOL_V1,
            "request_id": request_id, "game_id": game_id, "expected_step": expected_step});
        for (position, &id) in ids.iter().enumerate() {
            let current = match copy.game.as_ref().unwrap().session.current_response() {
                RlSessionResponseV1::Decision(value) => value,
                _ => return reject("private prefix crossed the declaration boundary"),
            };
            let Some(chosen) = current.legal_actions.get(id as usize) else {
                return reject("private prefix candidate is out of range");
            };
            let semantic_echo = match NeutralObserverV1::from_observation(&current.observation)
                .and_then(|observer| neutral_semantic_v1(&chosen.semantic, &observer)) {
                Ok(value) => value, Err(_) => return reject("private prefix cannot be represented"),
            };
            let response = copy.handle_step(request_id, game_id, current.step,
                &SpellbenchSelectionV1 { candidate_id: id, semantic_echo });
            if response["response_type"] != "decision" {
                if response["response_type"] == "terminal" && response["classification"] == "natural" && position + 1 == ids.len() {
                    return complete();
                }
                return reject("private prefix failed native transition or lockstep");
            }
            let next = copy.game.as_ref().unwrap();
            let RlSessionResponseV1::Decision(decision) = next.session.current_response() else { unreachable!() };
            if is_block && (decision.observation.projection.surface.surface_context.private_blockers.is_none() ||
                decision.observation.projection.surface.combat.ordered_attackers != root.observation.projection.surface.combat.ordered_attackers ||
                decision.acting_player != root.acting_player) {
                return if position + 1 == ids.len() { complete() }
                    else { reject("private prefix continues past its completed declaration") };
            }
            if arrangement {
                let support = private_v2_support(next);
                if support["effect_instance"] != root_support["effect_instance"] ||
                    support["choice"]["purpose"] != root_support["choice"]["purpose"] ||
                    decision.acting_player != root.acting_player {
                    return if position + 1 == ids.len() { complete() }
                        else { reject("private prefix continues past its completed declaration") };
                }
            }
        }
        let game = copy.game.as_mut().unwrap();
        let response = game.session.current_response();
        let counts = (game.session.policy_step_count(), game.session.physical_decision_count());
        let extension = match flat_v4_extension_v1(game, &response) {
            Ok(value) => value, Err(_) => return reject("private view failed encoding"),
        };
        let support = private_v2_support(game);
        match copy.session_response_to_wire(request_id, game_id, response, counts, extension, Some(support)) {
            WireResponseV1::Decision(view) => json!({"response_type": "private_preview", "protocol": SPELLBENCH_PROTOCOL_V1,
                "request_id": request_id, "view": view}),
            _ => reject("private prefix crossed the declaration boundary"),
        }
    }

    fn handle_reset(&mut self, request_id: &str, reset: SpellbenchResetFieldsV1<'_>) -> Value {
        let SpellbenchResetFieldsV1 {
            game_id,
            format,
            seats,
            game_seed,
            max_decisions,
            max_steps,
        } = reset;
        // Spec Section 2: at most one active game per process, and game ids
        // are unique per process; a finished game frees the process.
        if self.game.as_ref().is_some_and(|game| !game.terminal_sent) {
            return error_response_value_v1(
                Some(request_id),
                "game_already_active",
                "this engine process already hosts an active game",
            );
        }
        if self.used_game_ids.contains(game_id) {
            return error_response_value_v1(
                Some(request_id),
                "malformed_request",
                "game_id was already used by this engine process",
            );
        }
        if format != SPELLBENCH_FORMAT_PAUPER_BO1_V1 {
            return error_response_value_v1(
                Some(request_id),
                "unsupported_format",
                "format is not served by this engine",
            );
        }
        if seats[0].seat != "p0" || seats[1].seat != "p1" {
            return error_response_value_v1(
                Some(request_id),
                "malformed_request",
                "seats must be exactly p0 then p1",
            );
        }
        let mut deck_ids: SessionDeckIdsV1 = [String::new(), String::new()];
        for (index, seat) in seats.iter().enumerate() {
            match (&seat.deck.catalog_id, &seat.deck.decklist) {
                (Some(catalog_id), None)
                    if SPELLBENCH_POLICY_V5_UNPLAYABLE_DECKS_V1.contains(&catalog_id.as_str()) =>
                {
                    return error_response_value_v1(
                        Some(request_id),
                        "unsupported_deck",
                        "this deck needs actions policy surface v5 cannot represent (Escape); \
                         most of its games would halt",
                    );
                }
                (Some(catalog_id), None) => deck_ids[index] = catalog_id.clone(),
                (None, Some(_)) => {
                    return error_response_value_v1(
                        Some(request_id),
                        "unsupported_deck",
                        "this engine does not accept decklists as data",
                    );
                }
                _ => {
                    return error_response_value_v1(
                        Some(request_id),
                        "malformed_request",
                        "deck must carry exactly one of catalog_id or decklist",
                    );
                }
            }
        }
        let episode_id = spellbench_episode_id_v1(game_id);
        let lockstep = if self.options.x_kernel_flat_v4 {
            Some(FlatV4LockstepV1::reset(
                episode_id,
                game_seed,
                max_decisions,
                max_steps,
                deck_ids.clone(),
            ))
        } else {
            None
        };
        let mut session = match RlEpisodeSessionV1::reset_with_decks_and_limits(
            episode_id,
            game_seed,
            max_decisions,
            max_steps,
            deck_ids,
        ) {
            Ok(session) => session,
            Err(error) => {
                return error_response_value_v1(
                    Some(request_id),
                    "unsupported_deck",
                    &format!("deck is not in the engine catalog: {}", error.message),
                );
            }
        };
        let (mut lockstep, lockstep_failure) = match lockstep {
            Some(Ok(lockstep)) => (Some(lockstep), None),
            Some(Err(error)) => (None, Some(error.halt_reason())),
            None => (None, None),
        };
        if self.options.private_v2_support {
            session.enable_spellbench_neutral_choices();
            if let Some(copy) = lockstep.as_mut() {
                copy.enable_spellbench_neutral_choices();
            }
        }
        let counts = (
            session.policy_step_count(),
            session.physical_decision_count(),
        );
        let response = session.current_response();
        let mut game = SpellbenchBridgeGameV1 {
            game_id: game_id.to_string(),
            episode_id,
            session,
            terminal_sent: false,
            flat_v4: lockstep,
            flat_v4_rows: Vec::new(),
            flat_v4_scratch: FlatV4EncodeScratchV1::default(),
            private_block_root_step: None,
            #[cfg(test)]
            force_flat_v4_encode_error: false,
        };
        let flat_v4 = match lockstep_failure {
            Some(reason) => Err(reason),
            None => flat_v4_extension_v1(&mut game, &response),
        };
        let wire = match flat_v4 {
            Ok(flat_v4) => {
                let support = self.options.private_v2_support.then(|| private_v2_support(&game));
                self.session_response_to_wire(request_id, game_id, response, counts, flat_v4, support)
            }
            Err(reason) => WireResponseV1::Terminal(
                self.halted_terminal_value(request_id, game_id, counts, &reason),
            ),
        };
        game.terminal_sent = matches!(wire, WireResponseV1::Terminal(_));
        self.used_game_ids.insert(game_id.to_string());
        self.game = Some(game);
        match wire {
            WireResponseV1::Decision(value) | WireResponseV1::Terminal(value) => value,
        }
    }

    fn handle_step(
        &mut self,
        request_id: &str,
        game_id: &str,
        expected_step: u64,
        selection: &SpellbenchSelectionV1,
    ) -> Value {
        let Some(game) = self.game.as_mut() else {
            return error_response_value_v1(
                Some(request_id),
                "step_before_reset",
                "step request received before any reset",
            );
        };
        if game.game_id != game_id {
            return error_response_value_v1(
                Some(request_id),
                "game_id_mismatch",
                "step request game_id does not match the active game",
            );
        }
        if game.terminal_sent {
            return error_response_value_v1(
                Some(request_id),
                "game_already_terminal",
                "the active game has already reached a terminal response",
            );
        }
        let current = match game.session.current_response() {
            RlSessionResponseV1::Decision(current) => current,
            RlSessionResponseV1::Terminal(terminal) => {
                let session_counts = (
                    game.session.policy_step_count(),
                    game.session.physical_decision_count(),
                );
                let reason = format!(
                    "engine_contract_failure:decision_expected_before_step:{}",
                    terminal.terminal_reason
                );
                return self.fail_game_halted(request_id, session_counts, &reason);
            }
        };
        if expected_step != current.step {
            return error_response_value_v1(
                Some(request_id),
                "expected_step_mismatch",
                "expected_step does not match the pending decision step",
            );
        }
        let selected_index = selection.candidate_id as usize;
        let Some(chosen) = current.legal_actions.get(selected_index) else {
            return error_response_value_v1(
                Some(request_id),
                "candidate_id_out_of_range",
                "candidate_id does not index the current candidate list",
            );
        };
        // From here on the decision counts as answered: a halted terminal
        // reports it as a step (and its group, if this substep closes it).
        let answered_counts = answered_decision_counts_v1(&current);
        let recomputed = match NeutralObserverV1::from_observation(&current.observation)
            .and_then(|observer| neutral_semantic_v1(&chosen.semantic, &observer))
        {
            Ok(semantic) => semantic,
            Err(reason) => return self.fail_game_halted(request_id, answered_counts, &reason),
        };
        let echo_canonical = match spellbench_canonical_json_v1(&selection.semantic_echo) {
            Some(canonical) => canonical,
            None => {
                return error_response_value_v1(
                    Some(request_id),
                    "semantic_echo_mismatch",
                    "semantic_echo is not canonicalizable protocol content",
                );
            }
        };
        let recomputed_canonical = spellbench_canonical_json_v1(&recomputed)
            .expect("bridge-emitted semantics are canonicalizable");
        if echo_canonical != recomputed_canonical {
            return error_response_value_v1(
                Some(request_id),
                "semantic_echo_mismatch",
                "semantic_echo does not equal the selected candidate semantic",
            );
        }
        if game.flat_v4.is_some() && !game.flat_v4_rows.contains(&selection.candidate_id) {
            let reason = FlatV4LockstepErrorV1::SelectionNotInTrainingList {
                candidate_id: selection.candidate_id,
            }
            .halt_reason();
            return self.fail_game_halted(request_id, answered_counts, &reason);
        }
        let applied = game.session.step(
            game.episode_id,
            expected_step,
            selection.candidate_id,
            &chosen.stable_id,
        );
        match applied {
            Ok(response) => {
                let counts = (
                    game.session.policy_step_count(),
                    game.session.physical_decision_count(),
                );
                let rows = std::mem::take(&mut game.flat_v4_rows);
                // A halt the raw session reports ends the game with its own
                // reason: the copy is neither stepped nor checked.
                let lockstep = game.flat_v4.as_mut().filter(|_| !is_raw_halt_v1(&response));
                if let Some(lockstep) = lockstep {
                    if let Err(error) =
                        lockstep.mirror_step(&rows, expected_step, selection.candidate_id)
                    {
                        return self.fail_game_halted(request_id, counts, &error.halt_reason());
                    }
                }
                let flat_v4 = match flat_v4_extension_v1(game, &response) {
                    Ok(flat_v4) => flat_v4,
                    Err(reason) => return self.fail_game_halted(request_id, counts, &reason),
                };
                let support = self.options.private_v2_support.then(|| private_v2_support(game));
                let wire =
                    self.session_response_to_wire(request_id, game_id, response, counts, flat_v4, support);
                let game = self.game.as_mut().expect("active game");
                if matches!(wire, WireResponseV1::Terminal(_)) {
                    game.terminal_sent = true;
                }
                match wire {
                    WireResponseV1::Decision(value) | WireResponseV1::Terminal(value) => value,
                }
            }
            Err(error) => match session_error_to_bridge_code_v1(&error) {
                Some(code) => error_response_value_v1(Some(request_id), code, &error.message),
                None => self.fail_game_halted(
                    request_id,
                    answered_counts,
                    &session_rejection_reason_v1(&error),
                ),
            },
        }
    }

    /// Converts a session response into a wire decision/terminal. A decision
    /// that cannot be represented in neutral semantics fails the whole game
    /// as `halted` (spec Sections 8 and 11) rather than emitting an
    /// unlicensed candidate. `counts` carries the session's current
    /// (step, physical decision) totals for that halted terminal. `flat_v4`
    /// is the decision's `x_kernel_flat_v4` payload, when the option is on.
    fn session_response_to_wire(
        &self,
        request_id: &str,
        game_id: &str,
        response: RlSessionResponseV1,
        counts: (u64, u64),
        flat_v4: Option<Value>,
        private_support: Option<Value>,
    ) -> WireResponseV1 {
        match response {
            RlSessionResponseV1::Decision(decision) => {
                match decision_response_value_v1(
                    self.identity(),
                    request_id,
                    game_id,
                    &decision,
                    flat_v4,
                ) {
                    Ok(mut value) => {
                        if let Some(support) = private_support {
                            value["extensions"]["x_kernel_v2_support"] = support;
                        }
                        WireResponseV1::Decision(value)
                    },
                    Err(reason) => WireResponseV1::Terminal(
                        self.halted_terminal_value(request_id, game_id, counts, &reason),
                    ),
                }
            }
            RlSessionResponseV1::Terminal(terminal) => WireResponseV1::Terminal(
                terminal_response_value_v1(self.identity(), request_id, game_id, &terminal),
            ),
        }
    }

    /// Marks the active game halted and returns the synthetic terminal. Used
    /// when the bridge itself cannot honor the protocol for this game.
    /// `counts` are the (step, physical decision) totals the terminal reports.
    fn fail_game_halted(&mut self, request_id: &str, counts: (u64, u64), reason: &str) -> Value {
        let game_id = self
            .game
            .as_ref()
            .expect("halted failure needs a game")
            .game_id
            .clone();
        let value = self.halted_terminal_value(request_id, &game_id, counts, reason);
        if let Some(game) = self.game.as_mut() {
            game.terminal_sent = true;
        }
        value
    }

    fn halted_terminal_value(
        &self,
        request_id: &str,
        game_id: &str,
        counts: (u64, u64),
        reason: &str,
    ) -> Value {
        let (step_count, decision_count) = counts;
        json!({
            "response_type": "terminal",
            "protocol": SPELLBENCH_PROTOCOL_V1,
            "request_id": request_id,
            "game_id": game_id,
            "outcome": "halted",
            "classification": "halted",
            "winner": Value::Null,
            "reason": reason,
            "step_count": step_count,
            "decision_count": decision_count,
            "provenance": self.identity().provenance_value(),
        })
    }
}

enum WireResponseV1 {
    Decision(Value),
    Terminal(Value),
}

/// A terminal the raw session itself classifies `halted`.
fn is_raw_halt_v1(response: &RlSessionResponseV1) -> bool {
    matches!(
        response,
        RlSessionResponseV1::Terminal(terminal)
            if terminal.terminal_classification == TerminalClassificationV1::Halted
    )
}

/// The `x_kernel_flat_v4` payload for `response` (None when the option is off
/// or the game ended); stores the decision's row map. An Err is the halted
/// reason: the engine could not produce the model input. A halt the raw
/// session reports passes through before the lockstep check and keeps its
/// own reason: the copy has no counterpart for some raw halts (for example
/// `fail_closed:observation`), and a finished game needs no model input.
/// (`handle_step` does not step the copy for such a halt either.)
fn flat_v4_extension_v1(
    game: &mut SpellbenchBridgeGameV1,
    response: &RlSessionResponseV1,
) -> Result<Option<Value>, String> {
    if let RlSessionResponseV1::Decision(decision) = response {
        if decision.observation.projection.surface.surface_context.private_blockers.is_some() {
            game.private_block_root_step.get_or_insert(decision.step);
        } else {
            game.private_block_root_step = None;
        }
    } else {
        game.private_block_root_step = None;
    }
    let SpellbenchBridgeGameV1 {
        session,
        flat_v4,
        flat_v4_rows,
        flat_v4_scratch,
        ..
    } = game;
    let Some(lockstep) = flat_v4.as_ref() else {
        return Ok(None);
    };
    if is_raw_halt_v1(response) {
        return Ok(None);
    }
    lockstep
        .verify(session, response)
        .map_err(|error| error.halt_reason())?;
    let RlSessionResponseV1::Decision(decision) = response else {
        return Ok(None);
    };
    let rows = lockstep
        .row_candidate_ids(decision)
        .map_err(|error| error.halt_reason())?;
    #[cfg(test)]
    if game.force_flat_v4_encode_error {
        return Err(FlatV4EncodeErrorV1::Encode("forced by test".to_string()).halt_reason());
    }
    let encoded = encode_flat_v4_tensor_v1(lockstep.fast_session(), flat_v4_scratch)
        .map_err(|error| error.halt_reason())?;
    let value = flat_v4_extension_value_v1(&encoded, &rows).map_err(|error| error.halt_reason())?;
    *flat_v4_rows = rows;
    Ok(Some(value))
}

/// Totals a terminal reports once `decision` has been answered: the answered
/// decision is a step, and its physical decision counts when this substep
/// closes its group (spec Section 8 counts only completed groups).
fn answered_decision_counts_v1(decision: &RlSessionDecisionV1) -> (u64, u64) {
    let closes_group = decision.substep_index + 1 == decision.substep_count;
    (
        decision.step + 1,
        decision.physical_decision_id + u64::from(closes_group),
    )
}

/// Kernel session errors that still map onto the closed spellbench table.
/// Everything else is an engine contract failure and halts the game.
fn session_error_to_bridge_code_v1(error: &RlSessionError) -> Option<&'static str> {
    match error.code {
        RlSessionErrorCode::EpisodeAlreadyTerminal => Some("game_already_terminal"),
        RlSessionErrorCode::EpisodeIdMismatch => Some("game_id_mismatch"),
        RlSessionErrorCode::ExpectedStepMismatch => Some("expected_step_mismatch"),
        RlSessionErrorCode::SelectedIndexOutOfRange => Some("candidate_id_out_of_range"),
        RlSessionErrorCode::SelectedActionIdMismatch
        | RlSessionErrorCode::SelectedActionIdUnknown
        | RlSessionErrorCode::StaleEnvironmentBinding
        | RlSessionErrorCode::UnsupportedDeck
        | RlSessionErrorCode::EnvironmentRandomization
        | RlSessionErrorCode::NonNaturalTerminal => None,
    }
}

/// A session error code's snake_case name, spelled as the kernel's JSONL
/// servers and the code's own serde form spell it.
fn session_error_code_name_v1(code: RlSessionErrorCode) -> &'static str {
    match code {
        RlSessionErrorCode::EpisodeAlreadyTerminal => "episode_already_terminal",
        RlSessionErrorCode::EpisodeIdMismatch => "episode_id_mismatch",
        RlSessionErrorCode::ExpectedStepMismatch => "expected_step_mismatch",
        RlSessionErrorCode::SelectedIndexOutOfRange => "selected_index_out_of_range",
        RlSessionErrorCode::SelectedActionIdMismatch => "selected_action_id_mismatch",
        RlSessionErrorCode::SelectedActionIdUnknown => "selected_action_id_unknown",
        RlSessionErrorCode::StaleEnvironmentBinding => "stale_environment_binding",
        RlSessionErrorCode::UnsupportedDeck => "unsupported_deck",
        RlSessionErrorCode::EnvironmentRandomization => "environment_randomization",
        RlSessionErrorCode::NonNaturalTerminal => "non_natural_terminal",
    }
}

/// Halted reason for a step the bridge validated but the session refused:
/// the session error code and the kernel's message. Session step errors
/// carry fixed text, so the reason is deterministic for a given game, which
/// hosts need because they hash reasons into their ledgers.
fn session_rejection_reason_v1(error: &RlSessionError) -> String {
    format!(
        "engine_contract_failure:session_step_rejected_after_validation:{}:{}",
        session_error_code_name_v1(error.code),
        error.message
    )
}

fn request_id_from_value_v1(value: &Value) -> Option<String> {
    value
        .get("request_id")
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// Rejects any fractional JSON number anywhere in the parsed request tree.
/// The strict parser already rejects non-finite numbers; integers within the
/// protocol's `|x| <= 2^53` guidance are a sender constraint, so full
/// u64/i64-typed fields are accepted as decoded.
fn contains_fractional_number_v1(value: &Value) -> bool {
    match value {
        Value::Number(number) => number.is_f64(),
        Value::Array(items) => items.iter().any(contains_fractional_number_v1),
        Value::Object(map) => map.values().any(contains_fractional_number_v1),
        _ => false,
    }
}

/// Spec Section 4.1: responses always carry a string `request_id`; when the
/// request's id cannot be read (malformed line, non-string id) it is "".
fn error_response_value_v1(request_id: Option<&str>, code: &str, message: &str) -> Value {
    json!({
        "response_type": "error",
        "protocol": SPELLBENCH_PROTOCOL_V1,
        "request_id": request_id.unwrap_or(""),
        "error": {"code": code, "message": message},
    })
}

fn serialize_response_line_v1(value: &Value) -> String {
    serde_json::to_string(value).expect("bridge response serializes")
}

/// Protocol seat spelling, shared by decision lines and `x_kernel_flat_v4`.
pub(crate) fn seat_str_v1(seat: PlayerSeatV1) -> &'static str {
    match seat {
        PlayerSeatV1::P0 => "p0",
        PlayerSeatV1::P1 => "p1",
    }
}

fn zone_str_v1(zone: Zone) -> &'static str {
    match zone {
        Zone::Library => "library",
        Zone::Hand => "hand",
        Zone::Battlefield => "battlefield",
        Zone::Graveyard => "graveyard",
        Zone::Stack => "stack",
        Zone::Exile => "exile",
        Zone::Command => "command",
    }
}

fn phase_step_str_v1(phase: ZoneIndependentStepV1) -> &'static str {
    match phase {
        ZoneIndependentStepV1::Untap => "untap",
        ZoneIndependentStepV1::Upkeep => "upkeep",
        ZoneIndependentStepV1::Draw => "draw",
        ZoneIndependentStepV1::Main1 => "precombat_main",
        ZoneIndependentStepV1::BeginCombat => "beginning_of_combat",
        ZoneIndependentStepV1::DeclareAttackers => "declare_attackers",
        ZoneIndependentStepV1::DeclareBlockers => "declare_blockers",
        ZoneIndependentStepV1::CombatDamage => "combat_damage",
        ZoneIndependentStepV1::EndCombat => "end_of_combat",
        ZoneIndependentStepV1::Main2 => "postcombat_main",
        ZoneIndependentStepV1::End => "end_step",
        ZoneIndependentStepV1::Cleanup => "cleanup",
    }
}

fn cost_kind_str_v1(cost_kind: CostKind) -> &'static str {
    match cost_kind {
        CostKind::SacrificeLands => "sacrifice_lands",
        CostKind::SacrificePermanents => "sacrifice_permanents",
        CostKind::SacrificeCreatures => "sacrifice_creatures",
        CostKind::SacrificeArtifacts => "sacrifice_artifacts",
        CostKind::DiscardCards => "discard_cards",
        CostKind::ExileFromGraveyard => "exile_from_graveyard",
        CostKind::TapPermanents => "tap_permanents",
        CostKind::ReturnPermanentsToHand => "return_permanents_to_hand",
        CostKind::PayLife => "pay_life",
        CostKind::RemoveCounters => "remove_counters",
        CostKind::PutCounters => "put_counters",
        CostKind::ChooseCreatureOrRevealCreature => "choose_creature_or_reveal_creature",
    }
}

fn color_str_v1(color: ManaColor) -> Result<&'static str, String> {
    match color {
        ManaColor::W => Ok("white"),
        ManaColor::U => Ok("blue"),
        ManaColor::B => Ok("black"),
        ManaColor::R => Ok("red"),
        ManaColor::G => Ok("green"),
        ManaColor::C => Err(
            "engine_contract_failure:colorless is not a spellbench choose_color color".to_string(),
        ),
    }
}

fn mana_choice_str_v1(color: ManaColor) -> &'static str {
    match color {
        ManaColor::W => "W",
        ManaColor::U => "U",
        ManaColor::B => "B",
        ManaColor::R => "R",
        ManaColor::G => "G",
        ManaColor::C => "C",
    }
}

/// Neutral `choose_optional_cost_which.choice` (spec Section 6 allows other
/// lowercase snake_case values beside the documented ones).
fn optional_cost_choice_str_v1(choice: OptionalCostChoice) -> &'static str {
    match choice {
        OptionalCostChoice::Decline => "decline",
        OptionalCostChoice::Discard => "discard",
        OptionalCostChoice::SacrificeLand => "sacrifice_land",
        OptionalCostChoice::ReturnPermanent => "return_permanent",
    }
}

/// Object incarnation key: `(arena_id, zone_change_count)`.
type ObjectKeyV1 = (u32, u32);

fn private_v2_support(game: &SpellbenchBridgeGameV1) -> Value {
    use crate::effect::{EffectBooleanChoicePurpose as B, EffectOptionChoicePurpose as O,
        EffectTargetSelectionPurpose as T, PendingEffectChoice as C, ScrySelectionStage as S,
        LibraryPartitionSelectionStage as L};
    let state = game.session.game_state();
    let mut support = json!({"stack_instances": state.stack.iter()
        .map(|item| item.v4.stack_item_id.0.to_string()).collect::<Vec<_>>()});
    let RlSessionResponseV1::Decision(decision) = game.session.current_response() else { return support; };
    let actor = decision.acting_player;
    let actor_id = match actor { PlayerSeatV1::P0 => crate::ids::PlayerId::P0, PlayerSeatV1::P1 => crate::ids::PlayerId::P1 };
    if let Some(item) = state.engine.spellbench_detached_resolution.as_ref() {
        match crate::rl::spellbench_detached_resolution_public_v2(state, actor_id) {
            Ok(public) => {
                support["detached_resolution"] = json!({"instance": item.v4.stack_item_id.0.to_string(), "item": public});
                support["resolving_stack_instance"] = json!(item.v4.stack_item_id.0.to_string());
            }
            Err(_) => support["projection_error"] = json!("invalid detached resolving item"),
        }
    }
    if decision.legal_actions.iter().any(|action| matches!(action.semantic,
        ActionSemanticV1::ChooseTarget { .. } | ActionSemanticV1::FinishTargetSelection { .. })) {
        match crate::rl::spellbench_pending_trigger_public_v2(state, actor_id) {
            Ok(Some((instance, item, minimum, maximum, selected_count))) => {
                support["announcing_trigger"] = json!({"instance": instance, "item": item});
                support["target_bounds"] = json!({"minimum": minimum, "maximum": maximum, "selected_count": selected_count, "slot": 0});
            }
            Ok(None) => {},
            Err(_) => support["projection_error"] = json!("invalid pending trigger announcement"),
        }
    }
    if let Some(pending) = state.engine.pending_cast.as_ref().filter(|pending| PlayerSeatV1::from(pending.controller) == actor) {
        support["announcement_x_value"] = json!(pending.x_value);
        let def = &crate::card_def::CARD_DEFS[state.objects.get(pending.spell).card_def as usize];
        let staged = pending.source_contract.cast_method;
        support["announcement_cast_method"] = json!(crate::engine::finalized_cast_method(pending, staged, def));
        if pending.x_value.is_none() {
            support["choice"] = json!({"purpose": "x_value", "stage": "number"});
        }
        if let Some(form) = pending.mode_chosen {
            let def = &crate::card_def::CARD_DEFS[state.objects.get(pending.spell).card_def as usize];
            if let Some(spec) = crate::engine::spell_form_target_spec(def, form) {
                support["target_bounds"] = json!({"minimum": crate::engine::target_min_count(spec),
                    "maximum": crate::engine::target_count(spec), "selected_count": pending.targets_chosen.len(), "slot": 0});
            }
        }
        support["cost_selected_count"] = json!(pending.sacrifice_chosen.len());
    } else if let Some(pending) = state.engine.pending_activation.as_ref().filter(|pending| PlayerSeatV1::from(pending.controller) == actor) {
        support["target_bounds"] = json!({"minimum": crate::engine::target_min_count(pending.target_spec),
            "maximum": crate::engine::target_count(pending.target_spec), "selected_count": pending.targets_chosen.len(), "slot": 0});
        support["cost_selected_count"] = json!(pending.object_cost_chosen.len());
    } else if let Some(pending) = state.engine.pending_optional_cost_sacrifice.as_ref().filter(|pending| PlayerSeatV1::from(pending.player) == actor) {
        support["cost_selected_count"] = json!(pending.chosen.len());
    }
    // V5 already supplies the other effective keyword flags. These two are
    // missing from that frozen schema; inspect only its visible battlefield
    // records, never enumerate hidden objects or serialize whole game state.
    let extras = decision.observation.projection.surface.battlefield.iter().flatten().map(|card| {
        let id = crate::ids::ObjectId(card.stable.arena_id);
        let key = format!("obj-{:06}-z{:04}", card.stable.arena_id, card.stable.zone_change_count);
        (key, json!({"flash": crate::engine::has_effective_keyword(state, id, crate::card_def::Keywords::FLASH),
            "islandwalk": crate::engine::has_effective_keyword(state, id, crate::card_def::Keywords::ISLANDWALK),
            "cant_be_blocked": crate::engine::has_effective_keyword(state, id, crate::card_def::Keywords::CANT_BE_BLOCKED)}))
    }).collect::<serde_json::Map<String, Value>>();
    support["public_keyword_extras"] = Value::Object(extras);
    if let Some(root) = game.private_block_root_step {
        support["block_declaration_instance"] = json!(format!("{}:{root}", game.episode_id));
        support["at_block_root"] = json!(root == decision.step);
    }
    if let Some(discard) = decision.observation.projection.surface.surface_context.private_discard.as_ref() {
        let chosen = discard.chosen.len() as u64;
        support["effect_instance"] = json!(format!("{}:discard:{}", game.episode_id, decision.step - chosen));
        support["choice"] = json!({"purpose": "discard_order", "stage": "selection"});
        support["discard_count"] = json!(chosen + u64::from(discard.remaining_needed));
        support["discard_selected_count"] = json!(chosen);
        return support;
    }
    let Some(effect) = state.engine.pending_effect.as_ref() else { return support; };
    let Some(choice) = effect.choice.as_ref() else { return support; };
    if PlayerSeatV1::from(choice.player()) != decision.acting_player { return support; }
    support["resolving_stack_instance"] = json!(effect.resolving_item.v4.stack_item_id.0.to_string());
    if let C::SelectTargets { purpose, .. } = choice {
        let look = match purpose {
            T::ScryLibrary { player, original_prefix, .. } |
            T::LookTopSelectByTypeToHandBottomRest { player, original_prefix, .. } |
            T::OrderLookedLibraryTop { player, original_prefix } => Some((*player, original_prefix.len())),
            _ => None,
        };
        if let Some((owner, count)) = look {
            support["look_owner"] = json!(seat_str_v1(owner.into()));
            support["look_card_count"] = json!(count);
        }
    }
    // Export only the chooser's purpose/stage and an internal look identity.
    // Never serialize whole continuations, hidden libraries, filters or frames.
    let mut root = choice.structural_path();
    let (purpose, stage) = match choice {
        C::SelectTargets { purpose, .. } => match purpose {
            T::ScryLibrary { canonical_path, stage, .. } => {
                root = canonical_path;
                ("scry", match stage { S::ChooseBottomSubset => "partition",
                    S::OrderBottom { .. } => "bottom_order", S::OrderRetainedTop { .. } => "top_order" })
            }
            T::LookTopSelectByTypeToHandBottomRest { canonical_path, stage, .. } => {
                root = canonical_path;
                let revealed = state.engine.spellbench_revealed_partition.as_deref() == Some(canonical_path.as_slice());
                (if revealed { "reveal_partition" } else { "look_select" }, match stage { L::ChooseMatchingSubset => "partition", L::OrderRest { .. } => "bottom_order" })
            }
            T::OrderLookedLibraryTop { .. } => ("look", "top_order"),
            T::OrderIntoGraveyard { .. } | T::OrderMilledIntoGraveyard |
                T::OrderRevealedIntoGraveyard { .. } => ("mill", "graveyard_order"),
            T::PutHandCardOnLibraryTop { canonical_path, total, remaining, prompt_index, .. } => {
                root = canonical_path;
                support["hand_to_top"] = json!({"total": total, "remaining": remaining, "selected_count": prompt_index});
                ("put_on_top", "selection")
            },
            T::SearchLibraryToHand { .. } | T::SearchLibraryToHandMany { .. } |
                T::SearchLibraryToBattlefieldTapped { .. } => ("search", "selection"),
            T::ExileOneFromGraveyard { .. } | T::ExileOneMatchingFromGraveyard { .. } => ("exile", "selection"),
            T::SacrificeCreature { .. } => ("sacrifice", "selection"),
            T::UntapLands { .. } => ("untap", "selection"),
            T::DuressDiscard { .. } => ("discard", "selection"),
            T::LinkedExileNonlandFromRevealedHand { .. } => ("exile", "selection"),
            T::UndercityThroneCreature { .. } => ("undercity_throne", "selection"),
        },
        C::ChooseBoolean { purpose, .. } => match purpose {
            B::ShuffleLibrary { .. } => ("shuffle", "boolean"),
            B::CounterUnlessPaysGeneric { .. } | B::CounterTargetUnlessPaysGeneric { .. } => ("unless_payment", "boolean"),
            B::PayManaThen { .. } => ("resolution_payment", "boolean"),
            B::SearchLibraryToBattlefieldTapped { .. } => ("search", "boolean"),
            B::PayExileFromGraveyardThen { .. } => ("exile_payment", "boolean"),
            B::LookAtTopMayRevealThen { .. } => ("reveal", "boolean"),
        },
        C::ChooseOption { purpose, .. } => match purpose {
            O::Generic => ("effect_option", "option"),
            O::OwnerLibrarySecondOrBottom { .. } => ("owner_library_position", "option"),
            O::ExploreNonlandTop { .. } => ("explore", "option"),
            O::SurveilTopCard { .. } => ("surveil", "option"),
            O::ChooseColor { .. } => ("color", "option"),
            O::UndercityRoute { .. } => ("undercity_route", "option"),
        },
    };
    support["effect_instance"] = json!(format!("{}:{}", effect.resolving_item.v4.stack_item_id.0,
        root.iter().map(u16::to_string).collect::<Vec<_>>().join(".")));
    support["choice"] = json!({"purpose": purpose, "stage": stage});
    support
}

/// What the acting seat knows about objects, read from its own observation:
/// the name the observation shows for every card it lists (public zones, own
/// hand, known library and hand cards; transformed faces read correctly),
/// and the objects its pending effect offers as targets (a search or look
/// lets the chooser see them).
struct NeutralObserverV1 {
    seat: PlayerSeatV1,
    named: HashMap<ObjectKeyV1, String>,
    offered: HashSet<ObjectKeyV1>,
}

impl NeutralObserverV1 {
    fn from_observation(observation: &ObservationV5) -> Result<Self, String> {
        let mut named = HashMap::new();
        let surface = &observation.projection.surface;
        let public = surface
            .battlefield
            .iter()
            .chain(surface.graveyards.iter())
            .flatten()
            .chain(surface.exile.iter())
            .map(|card| (&card.stable, card.card_name.as_str()));
        let private = observation
            .own_hand
            .iter()
            .chain(observation.known_hand_cards.iter().flatten())
            .chain(
                observation
                    .known_library_cards
                    .iter()
                    .flatten()
                    .map(|known| &known.card),
            )
            .map(|card| (&card.stable, card.card_name.as_str()));
        for (stable, card_name) in public.chain(private) {
            let key = (stable.arena_id, stable.zone_change_count);
            if let Some(previous) = named.insert(key, card_name.to_string()) {
                if previous != card_name {
                    return Err(format!(
                        "engine_contract_failure:observation names object {} as {previous} and {card_name}",
                        stable.arena_id
                    ));
                }
            }
        }
        let mut offered = HashSet::new();
        if let Some(PendingEffectSemanticV4 {
            choice:
                Some(PendingEffectChoiceSemanticV4::Targets {
                    legal_targets,
                    selected_targets,
                    ..
                }),
            ..
        }) = &surface.engine_context.pending_effect
        {
            for target in legal_targets.iter().chain(selected_targets) {
                if let TargetRefV1::Object { object } = target {
                    offered.insert((object.arena_id, object.zone_change_count));
                }
            }
        }
        Ok(NeutralObserverV1 {
            seat: observation.acting_player,
            named,
            offered,
        })
    }
}

/// Converts one kernel stable card reference into the neutral observer-relative
/// object reference. `object_id` is `obj-<arena_id>-z<zone_change_count>`
/// (see [`SPELLBENCH_OBJECT_ID_DERIVATION_V1`]). `card_name` is the name the
/// acting seat's observation shows for the object; an object the observation
/// does not list takes its registry name unless it sits in a hidden zone (a
/// library, or a hand the observer does not own) and is not offered to the
/// observer as a target, in which case it is null.
fn neutral_object_ref_v1(
    reference: &CardStableRefV1,
    observer: &NeutralObserverV1,
) -> Result<Value, String> {
    let key = (reference.arena_id, reference.zone_change_count);
    let hidden = reference.zone == Zone::Library
        || (reference.zone == Zone::Hand && reference.owner != observer.seat);
    let card_name = if let Some(name) = observer.named.get(&key) {
        Value::String(name.clone())
    } else if hidden && !observer.offered.contains(&key) {
        Value::Null
    } else {
        let definition = CARD_DEFS
            .get(reference.card_db_id as usize)
            .ok_or_else(|| {
                format!(
                    "engine_contract_failure:card_db_id {} is outside the card registry",
                    reference.card_db_id
                )
            })?;
        Value::String(definition.name.to_string())
    };
    Ok(json!({
        "object_id": format!("obj-{:06}-z{:04}", reference.arena_id, reference.zone_change_count),
        "card_name": card_name,
        "owner_seat": seat_str_v1(reference.owner),
        "controller_seat": seat_str_v1(reference.controller),
        "zone": zone_str_v1(reference.zone),
    }))
}

fn neutral_target_ref_v1(
    target: &TargetRefV1,
    observer: &NeutralObserverV1,
) -> Result<Value, String> {
    match target {
        TargetRefV1::Player { player } => Ok(json!({"player": seat_str_v1(*player)})),
        TargetRefV1::Object { object } => {
            Ok(json!({"object": neutral_object_ref_v1(object, observer)?}))
        }
    }
}

/// Maps one policy-v5 action semantic onto the exact neutral candidate
/// semantic of spec Section 6. The `actor` field is intentionally dropped:
/// the decision envelope's `acting_seat` carries it and the neutral schema
/// allows no other fields. `Ambiguous` and the legacy aggregate combat
/// variants have no neutral kind and fail closed; policy surface v5 already
/// refuses them, so an occurrence here is an engine contract failure.
fn neutral_semantic_v1(
    semantic: &ActionSemanticV1,
    observer: &NeutralObserverV1,
) -> Result<Value, String> {
    match semantic {
        ActionSemanticV1::Pass { .. } => Ok(json!({"kind": "pass"})),
        ActionSemanticV1::PlayLand { source, .. } => Ok(json!({
            "kind": "play_land",
            "source": neutral_object_ref_v1(source, observer)?,
        })),
        ActionSemanticV1::CastSpell { source, .. } => Ok(json!({
            "kind": "cast_spell",
            "source": neutral_object_ref_v1(source, observer)?,
        })),
        ActionSemanticV1::ActivateManaAbility {
            source,
            mana_choice,
            cost_target,
            ..
        } => {
            let mana_choice = match mana_choice {
                Some(color) => Value::String(mana_choice_str_v1(*color).to_string()),
                None => Value::Null,
            };
            let cost_target = match cost_target {
                Some(target) => json!({"object": neutral_object_ref_v1(target, observer)?}),
                None => Value::Null,
            };
            Ok(json!({
                "kind": "activate_mana_ability",
                "source": neutral_object_ref_v1(source, observer)?,
                "mana_choice": mana_choice,
                "cost_target": cost_target,
            }))
        }
        ActionSemanticV1::ActivateAbility {
            source,
            ability_index,
            ..
        } => Ok(json!({
            "kind": "activate_ability",
            "source": neutral_object_ref_v1(source, observer)?,
            "ability_index": u32::from(*ability_index),
        })),
        ActionSemanticV1::PlotSpell { source, .. } => Ok(json!({
            "kind": "plot_spell",
            "source": neutral_object_ref_v1(source, observer)?,
        })),
        ActionSemanticV1::ChooseTarget {
            source,
            remaining,
            target,
            ..
        } => Ok(json!({
            "kind": "choose_target",
            "source": neutral_object_ref_v1(source, observer)?,
            "remaining": u32::from(*remaining),
            "target": neutral_target_ref_v1(target, observer)?,
        })),
        ActionSemanticV1::ChooseCostTarget {
            source,
            cost_kind,
            remaining,
            candidate,
            ..
        } => Ok(json!({
            "kind": "choose_cost_target",
            "source": neutral_object_ref_v1(source, observer)?,
            "cost_kind": cost_kind_str_v1(*cost_kind),
            "remaining": u32::from(*remaining),
            "candidate": neutral_object_ref_v1(candidate, observer)?,
        })),
        ActionSemanticV1::ChooseCastMode { source, mode, .. } => Ok(json!({
            "kind": "choose_cast_mode",
            "source": neutral_object_ref_v1(source, observer)?,
            "mode": match mode {
                CastMode::Normal => "normal",
                CastMode::Alternative => "alternative",
            },
        })),
        ActionSemanticV1::ChooseKicker { source, pay, .. } => Ok(json!({
            "kind": "choose_kicker",
            "source": neutral_object_ref_v1(source, observer)?,
            "pay": *pay,
        })),
        ActionSemanticV1::ChooseSpellMode {
            source,
            mode_index,
            mode_count,
            ..
        } => Ok(json!({
            "kind": "choose_spell_mode",
            "source": neutral_object_ref_v1(source, observer)?,
            "mode_index": u32::from(*mode_index),
            "mode_count": u32::from(*mode_count),
        })),
        ActionSemanticV1::ChooseEffectOption {
            source,
            option_index,
            option_count,
            ..
        } => Ok(json!({
            "kind": "choose_option",
            "source": neutral_object_ref_v1(source, observer)?,
            "option_index": u32::from(*option_index),
            "option_count": u32::from(*option_count),
        })),
        ActionSemanticV1::ChooseEffectTarget {
            source,
            target,
            selected_count,
            min_targets,
            max_targets,
            ..
        } => Ok(json!({
            "kind": "choose_effect_target",
            "source": neutral_object_ref_v1(source, observer)?,
            "target": neutral_target_ref_v1(target, observer)?,
            "selected_count": u32::from(*selected_count),
            "min_targets": u32::from(*min_targets),
            "max_targets": u32::from(*max_targets),
        })),
        ActionSemanticV1::FinishEffectSelection {
            source,
            selected_count,
            ..
        } => Ok(json!({
            "kind": "finish_effect_selection",
            "source": neutral_object_ref_v1(source, observer)?,
            "selected_count": u32::from(*selected_count),
        })),
        ActionSemanticV1::ChooseEffectColor { source, color, .. } => Ok(json!({
            "kind": "choose_color",
            "source": neutral_object_ref_v1(source, observer)?,
            "color": color_str_v1(*color)?,
        })),
        ActionSemanticV1::ChooseEffectNumber {
            source,
            number,
            minimum,
            maximum,
            ..
        } => Ok(json!({
            "kind": "choose_number",
            "source": neutral_object_ref_v1(source, observer)?,
            "value": *number,
            "minimum": *minimum,
            "maximum": *maximum,
        })),
        ActionSemanticV1::ChooseEffectBoolean { source, value, .. } => Ok(json!({
            "kind": "choose_boolean",
            "source": neutral_object_ref_v1(source, observer)?,
            "value": *value,
        })),
        ActionSemanticV1::FinishTargetSelection {
            source,
            selected_count,
            ..
        } => Ok(json!({
            "kind": "finish_target_selection",
            "source": neutral_object_ref_v1(source, observer)?,
            "selected_count": u32::from(*selected_count),
        })),
        ActionSemanticV1::ChooseOptionalCostUse { use_cost, .. } => Ok(json!({
            "kind": "choose_optional_cost_use",
            "use_cost": *use_cost,
        })),
        ActionSemanticV1::ChooseOptionalCostWhich { choice, .. } => Ok(json!({
            "kind": "choose_optional_cost_which",
            "choice": optional_cost_choice_str_v1(*choice),
        })),
        ActionSemanticV1::ChooseSpellCopyPayment { source, pay, .. } => Ok(json!({
            "kind": "choose_spell_copy_payment",
            "source": neutral_object_ref_v1(source, observer)?,
            "pay": *pay,
        })),
        ActionSemanticV1::ChooseSpellCopyRetarget {
            source,
            change_target,
            ..
        } => Ok(json!({
            "kind": "choose_spell_copy_retarget",
            "source": neutral_object_ref_v1(source, observer)?,
            "change_target": *change_target,
        })),
        ActionSemanticV1::ChooseMadnessCast { card, cast_it, .. } => Ok(json!({
            "kind": "choose_madness_cast",
            "card": neutral_object_ref_v1(card, observer)?,
            "cast_it": *cast_it,
        })),
        ActionSemanticV1::Discard { cards, .. } => {
            if cards.len() != 1 {
                return Err(format!(
                    "engine_contract_failure:discard candidate carries {} cards; spellbench v1 admits exactly one",
                    cards.len()
                ));
            }
            Ok(json!({
                "kind": "discard",
                "cards": [neutral_object_ref_v1(&cards[0], observer)?],
            }))
        }
        ActionSemanticV1::ChooseAttackerInclusion {
            attacker, include, ..
        } => Ok(json!({
            "kind": "choose_attacker_inclusion",
            "attacker": neutral_object_ref_v1(attacker, observer)?,
            "include": *include,
        })),
        ActionSemanticV1::ChooseBlockerInclusion {
            attacker,
            blocker,
            include,
            ..
        } => Ok(json!({
            "kind": "choose_blocker_inclusion",
            "attacker": neutral_object_ref_v1(attacker, observer)?,
            "blocker": neutral_object_ref_v1(blocker, observer)?,
            "include": *include,
        })),
        ActionSemanticV1::OrderTriggers {
            pending_sources,
            order,
            ..
        } => {
            let mut neutral_sources = Vec::with_capacity(pending_sources.len());
            for source in pending_sources {
                neutral_sources.push(neutral_object_ref_v1(source, observer)?);
            }
            let mut neutral_order = Vec::with_capacity(order.len());
            for position in order {
                neutral_order.push(Value::from(u32::try_from(*position).map_err(|_| {
                    "engine_contract_failure:trigger order index exceeds u32".to_string()
                })?));
            }
            Ok(json!({
                "kind": "order_triggers",
                "pending_sources": neutral_sources,
                "order": neutral_order,
            }))
        }
        ActionSemanticV1::Ambiguous { .. }
        | ActionSemanticV1::DeclareAttackers { .. }
        | ActionSemanticV1::DeclareBlockersForAttacker { .. } => Err(
            "engine_contract_failure:policy surface v5 semantic has no spellbench v1 kind"
                .to_string(),
        ),
    }
}

/// Neutral state summary (spec Section 7.3) projected from `ObservationV5`:
/// public scalars and zone counts only, never hidden identities.
fn neutral_state_summary_v1(observation: &ObservationV5) -> Value {
    let projection = &observation.projection.surface;
    let seats = [PlayerSeatV1::P0, PlayerSeatV1::P1]
        .into_iter()
        .enumerate()
        .map(|(index, seat)| {
            json!({
                "seat": seat_str_v1(seat),
                "life": projection.life_totals[index],
                "hand_count": projection.hand_counts[index],
                "library_count": projection.library_counts[index],
                "graveyard_count": projection.graveyards[index].len(),
                "battlefield_count": projection.battlefield[index].len(),
            })
        })
        .collect::<Vec<_>>();
    json!({
        "turn": projection.turn,
        "phase_step": phase_step_str_v1(projection.phase),
        "active_seat": seat_str_v1(projection.active_player),
        "priority_seat": seat_str_v1(projection.priority_player),
        "seats": seats,
        "stack_count": projection.stack.len(),
    })
}

/// Canonical JSON per spec Section 4.3: UTF-8, object keys sorted by code
/// point, compact `,`/`:` separators, integers only, strings byte-exact.
/// Follows the `canonical_json_v1` byte conventions; returns `None` for
/// content that can never appear in protocol semantics (a float).
pub fn spellbench_canonical_json_v1(value: &Value) -> Option<String> {
    let mut out = String::new();
    write_canonical_value_v1(value, &mut out).ok()?;
    Some(out)
}

fn write_canonical_value_v1(value: &Value, out: &mut String) -> Result<(), ()> {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(false) => out.push_str("false"),
        Value::Bool(true) => out.push_str("true"),
        Value::Number(number) => {
            if let Some(unsigned) = number.as_u64() {
                write!(out, "{unsigned}").map_err(|_| ())?;
            } else if let Some(signed) = number.as_i64() {
                write!(out, "{signed}").map_err(|_| ())?;
            } else {
                return Err(());
            }
        }
        Value::String(text) => {
            out.push_str(&serde_json::to_string(text).map_err(|_| ())?);
        }
        Value::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index != 0 {
                    out.push(',');
                }
                write_canonical_value_v1(item, out)?;
            }
            out.push(']');
        }
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            out.push('{');
            for (index, key) in keys.iter().enumerate() {
                if index != 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::to_string(key).map_err(|_| ())?);
                out.push(':');
                write_canonical_value_v1(&map[*key], out)?;
            }
            out.push('}');
        }
    }
    Ok(())
}

/// `candidates_sha256` per spec Section 4.3: lowercase hex SHA-256 of the
/// canonical JSON of `[{"candidate_id": id, "semantic": {...}}]` in decision
/// order, `display_text` excluded.
fn candidates_sha256_v1(candidates: &[Value]) -> String {
    let reduced: Vec<Value> = candidates
        .iter()
        .map(|candidate| {
            json!({
                "candidate_id": candidate["candidate_id"],
                "semantic": candidate["semantic"],
            })
        })
        .collect();
    let canonical = spellbench_canonical_json_v1(&Value::Array(reduced))
        .expect("bridge-emitted candidate semantics are canonicalizable");
    let digest = Sha256::digest(canonical.as_bytes());
    format!("{digest:x}")
}

fn decision_response_value_v1(
    identity: &SpellbenchEngineIdentityV1,
    request_id: &str,
    game_id: &str,
    decision: &RlSessionDecisionV1,
    flat_v4: Option<Value>,
) -> Result<Value, String> {
    let observer = NeutralObserverV1::from_observation(&decision.observation)?;
    let mut candidates = Vec::with_capacity(decision.legal_actions.len());
    for (index, action) in decision.legal_actions.iter().enumerate() {
        let candidate_id = u32::try_from(index)
            .map_err(|_| "engine_contract_failure:candidate index exceeds u32".to_string())?;
        candidates.push(json!({
            "candidate_id": candidate_id,
            "semantic": neutral_semantic_v1(&action.semantic, &observer)?,
            "display_text": action.display_text,
        }));
    }
    let candidates_sha256 = candidates_sha256_v1(&candidates);
    // The raw v5 payloads travel as JSON text: they carry u64 hashes
    // (`card_db_hash`, `visible_projection_hash`) beyond the spec's 2^53
    // integer bound, and kernel-native consumers parse the exact bytes with
    // the kernel's own deserializers.
    let mut extensions = json!({
        SPELLBENCH_EXTENSION_KERNEL_V5: {
            "observation_json": serde_json::to_string(&decision.observation)
                .map_err(|_| "engine_contract_failure:observation serialization".to_string())?,
            "legal_actions_json": serde_json::to_string(&decision.legal_actions)
                .map_err(|_| "engine_contract_failure:legal_actions serialization".to_string())?,
        }
    });
    if let Some(flat_v4) = flat_v4 {
        extensions[SPELLBENCH_EXTENSION_KERNEL_FLAT_V4] = flat_v4;
    }
    Ok(json!({
        "response_type": "decision",
        "protocol": SPELLBENCH_PROTOCOL_V1,
        "request_id": request_id,
        "game_id": game_id,
        "step": decision.step,
        "acting_seat": seat_str_v1(decision.acting_player),
        "group": {
            "group_id": decision.physical_decision_id,
            "substep_index": decision.substep_index,
            "substep_count": decision.substep_count,
        },
        "state_summary": neutral_state_summary_v1(&decision.observation),
        "candidates": candidates,
        "candidates_sha256": candidates_sha256,
        "provenance": identity.provenance_value(),
        "extensions": extensions,
    }))
}

fn terminal_response_value_v1(
    identity: &SpellbenchEngineIdentityV1,
    request_id: &str,
    game_id: &str,
    terminal: &RlSessionTerminalV1,
) -> Value {
    let outcome = match terminal.terminal_outcome {
        TerminalOutcomeV1::P0Win => "p0_win",
        TerminalOutcomeV1::P1Win => "p1_win",
        TerminalOutcomeV1::Draw => "draw",
        TerminalOutcomeV1::Truncated => "truncated",
        TerminalOutcomeV1::Halted => "halted",
    };
    let classification = match terminal.terminal_classification {
        TerminalClassificationV1::Natural => "natural",
        TerminalClassificationV1::Truncated => "truncated",
        TerminalClassificationV1::Halted => "halted",
    };
    let winner = match terminal.winner {
        Some(seat) => Value::String(seat_str_v1(seat).to_string()),
        None => Value::Null,
    };
    json!({
        "response_type": "terminal",
        "protocol": SPELLBENCH_PROTOCOL_V1,
        "request_id": request_id,
        "game_id": game_id,
        "outcome": outcome,
        "classification": classification,
        "winner": winner,
        "reason": terminal.terminal_reason,
        "step_count": terminal.policy_step_count,
        "decision_count": terminal.physical_decision_count,
        "provenance": identity.provenance_value(),
    })
}

/// One read attempt of the NDJSON framing layer.
pub enum BridgeInputLineV1 {
    /// A complete (or final unterminated) line within the 8 MiB cap, with the
    /// line terminator and one tolerated trailing `\r` stripped.
    Line(Vec<u8>),
    /// The line exceeded the cap; the reader has been drained through the
    /// next newline (or EOF) and framing is resynchronized.
    Oversized,
    /// Clean end of input: closing stdin ends the process.
    Eof,
}

/// Reads one request line with the spec Section 2 framing rules: `\n`
/// terminated, `\r\n` tolerated, at most [`SPELLBENCH_MAX_LINE_BYTES_V1`]
/// content bytes. An oversized line is drained through its terminator so the
/// next read starts at a line boundary.
pub fn read_bridge_input_line_v1(
    reader: &mut impl BufRead,
    buffer: &mut Vec<u8>,
) -> io::Result<BridgeInputLineV1> {
    buffer.clear();
    let read = {
        let mut limited = (&mut *reader).take(SPELLBENCH_MAX_LINE_BYTES_V1 as u64 + 2);
        limited.read_until(b'\n', buffer)?
    };
    if read == 0 {
        return Ok(BridgeInputLineV1::Eof);
    }
    if buffer.last() == Some(&b'\n') {
        buffer.pop();
        if buffer.last() == Some(&b'\r') {
            buffer.pop();
        }
        if buffer.len() > SPELLBENCH_MAX_LINE_BYTES_V1 {
            return Ok(BridgeInputLineV1::Oversized);
        }
        return Ok(BridgeInputLineV1::Line(std::mem::take(buffer)));
    }
    if buffer.len() > SPELLBENCH_MAX_LINE_BYTES_V1 {
        // Oversized: drain through the next newline to resynchronize framing.
        let mut scratch = Vec::new();
        loop {
            scratch.clear();
            let drained = reader.read_until(b'\n', &mut scratch)?;
            if drained == 0 || scratch.last() == Some(&b'\n') {
                break;
            }
        }
        return Ok(BridgeInputLineV1::Oversized);
    }
    // Final line without a trailing newline, tolerated on read.
    Ok(BridgeInputLineV1::Line(std::mem::take(buffer)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn test_server() -> SpellbenchBridgeServerV1 {
        SpellbenchBridgeServerV1::with_engine_identity(
            SpellbenchEngineIdentityV1::with_source_revision(None),
        )
    }

    fn hello_line(request_id: &str) -> String {
        json!({
            "request_type": "hello",
            "protocol": SPELLBENCH_PROTOCOL_V1,
            "request_id": request_id,
        })
        .to_string()
    }

    fn reset_line(request_id: &str, game_id: &str, game_seed: u64) -> String {
        json!({
            "request_type": "reset",
            "protocol": SPELLBENCH_PROTOCOL_V1,
            "request_id": request_id,
            "game_id": game_id,
            "format": SPELLBENCH_FORMAT_PAUPER_BO1_V1,
            "seats": [
                {"seat": "p0", "deck": {"catalog_id": "Burn"}},
                {"seat": "p1", "deck": {"catalog_id": "Burn"}},
            ],
            "game_seed": game_seed,
            "max_decisions": 16,
            "max_steps": 2048,
        })
        .to_string()
    }

    fn step_line_for(request_id: &str, decision: &Value, candidate_index: usize) -> String {
        let candidate = &decision["candidates"].as_array().unwrap()[candidate_index];
        json!({
            "request_type": "step",
            "protocol": SPELLBENCH_PROTOCOL_V1,
            "request_id": request_id,
            "game_id": decision["game_id"],
            "expected_step": decision["step"],
            "selection": {
                "candidate_id": candidate["candidate_id"],
                "semantic_echo": candidate["semantic"],
            },
        })
        .to_string()
    }

    fn parse_line(line: &str) -> Value {
        serde_json::from_str(line).expect("response line is valid JSON")
    }

    #[test]
    fn game_id_episode_mapping_is_fnv1a64_utf8() {
        assert_eq!(fnv1a64_utf8_v1(""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a64_utf8_v1("g-0001"), 0xdfeb_5eb2_d13b_a7ae);
        assert_eq!(spellbench_episode_id_v1("g-0001"), 0xdfeb_5eb2_d13b_a7ae);
    }

    #[test]
    fn private_v2_support_preserves_the_legacy_observation_and_tensor() {
        let mut legacy = flat_v4_server();
        let mut private = SpellbenchBridgeServerV1::with_options(legacy.identity.clone(),
            SpellbenchBridgeOptionsV1 { x_kernel_flat_v4: true, private_v2_support: true });
        let reset = reset_line("reset", "g-private", 7);
        let expected = parse_line(&legacy.handle_line(&reset));
        let mut actual = parse_line(&private.handle_line(&reset));
        let sideband = actual["extensions"].as_object_mut().unwrap().remove("x_kernel_v2_support").unwrap();
        assert_eq!(sideband["stack_instances"], json!([]));
        assert_eq!(actual, expected);
        for i in 0..3 {
            let request = step_line_for(&format!("step-{i}"), &expected, 0);
            // A stale step is refused identically, too; the first advances.
            let expected = parse_line(&legacy.handle_line(&request));
            let mut actual = parse_line(&private.handle_line(&request));
            if actual["response_type"] == "decision" {
                actual["extensions"].as_object_mut().unwrap().remove("x_kernel_v2_support");
            }
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn private_preview_refuses_priority_and_does_not_advance_the_game() {
        let mut server = SpellbenchBridgeServerV1::with_options(
            SpellbenchEngineIdentityV1::with_source_revision(None),
            SpellbenchBridgeOptionsV1 { x_kernel_flat_v4: true, private_v2_support: true });
        let first = parse_line(&server.handle_line(&reset_line("reset", "g-private", 7)));
        let before = server.game.as_ref().unwrap().session.game_state().clone();
        let preview = json!({"protocol": SPELLBENCH_PROTOCOL_V1, "request_type": "preview_steps",
            "request_id": "preview", "game_id": "g-private", "expected_step": 0, "candidate_ids": [0]}).to_string();
        let refused = parse_line(&server.handle_line(&preview));
        assert_eq!(refused["error"]["code"], "malformed_request");
        assert_eq!(server.game.as_ref().unwrap().session.game_state(), &before);
        assert_eq!(server.game.as_ref().unwrap().session.policy_step_count(), 0);
        let next = parse_line(&server.handle_line(&step_line_for("step", &first, 0)));
        assert_eq!(next["step"], 1);
    }

    #[test]
    fn canonical_json_sorts_keys_and_matches_serde_sorted_encoding() {
        let value = json!({"b": [1, true, null], "a": {"z": -2, "y": "x"}});
        let canonical = spellbench_canonical_json_v1(&value).unwrap();
        assert_eq!(canonical, serde_json::to_string(&value).unwrap());
        assert_eq!(canonical, r#"{"a":{"y":"x","z":-2},"b":[1,true,null]}"#);
        assert!(spellbench_canonical_json_v1(&json!(1.5)).is_none());
    }

    #[test]
    fn hello_reports_engine_identity_and_v1_capabilities() {
        let mut server = test_server();
        let response = parse_line(&server.handle_line(&hello_line("h-1")));
        assert_eq!(response["response_type"], "hello_ok");
        assert_eq!(response["protocol"], "spellbench/v1");
        assert_eq!(response["request_id"], "h-1");
        assert_eq!(response["engine"]["name"], "mtg-kernel");
        assert_eq!(response["engine"]["version"], KERNEL_VERSION);
        assert_eq!(response["engine"]["source_revision"], Value::Null);
        assert_eq!(
            response["engine"]["rules_snapshot_id"],
            format!("mtg-kernel-rules/{KERNEL_VERSION}/carddb-{KERNEL_CARDDB_HASH:016x}")
        );
        assert_eq!(
            response["engine"]["card_pool_identity"],
            format!("mtg-kernel-pauper-pool-v1/carddb-{KERNEL_CARDDB_HASH:016x}")
        );
        assert_eq!(response["formats"], json!(["pauper-bo1"]));
        assert_eq!(
            response["capabilities"],
            json!({"decklists_as_data": false})
        );
        assert_eq!(response["extensions"], json!(["x_kernel_v5"]));
    }

    #[test]
    fn hello_source_revision_comes_from_the_explicit_identity() {
        let mut server = SpellbenchBridgeServerV1::with_engine_identity(
            SpellbenchEngineIdentityV1::with_source_revision(Some("abc123".to_string())),
        );
        let response = parse_line(&server.handle_line(&hello_line("h-1")));
        assert_eq!(response["engine"]["source_revision"], "abc123");
    }

    #[test]
    fn reset_returns_step_zero_decision_with_neutral_projection() {
        let mut server = test_server();
        let response = parse_line(&server.handle_line(&reset_line("r-1", "g-0001", 7)));
        assert_eq!(response["response_type"], "decision");
        assert_eq!(response["game_id"], "g-0001");
        assert_eq!(response["step"], 0);
        assert!(matches!(
            response["acting_seat"].as_str(),
            Some("p0" | "p1")
        ));
        assert_eq!(
            response["group"],
            json!({"group_id": 0, "substep_index": 0, "substep_count": 1})
        );
        let summary = &response["state_summary"];
        assert_eq!(summary["turn"], 1);
        assert!(summary["phase_step"].is_string());
        assert_eq!(summary["seats"][0]["seat"], "p0");
        assert_eq!(summary["seats"][0]["life"], 20);
        assert_eq!(summary["seats"][0]["hand_count"], 7);
        assert_eq!(summary["seats"][0]["library_count"], 53);
        assert_eq!(summary["seats"][0]["graveyard_count"], 0);
        assert_eq!(summary["seats"][0]["battlefield_count"], 0);
        assert_eq!(summary["seats"][1]["seat"], "p1");
        assert_eq!(summary["stack_count"], 0);
        let candidates = response["candidates"].as_array().unwrap();
        assert!(!candidates.is_empty());
        for (index, candidate) in candidates.iter().enumerate() {
            assert_eq!(candidate["candidate_id"], index as u64);
            assert!(candidate["semantic"]["kind"].is_string());
            assert!(candidate.get("display_text").is_some());
        }
        assert_eq!(response["candidates_sha256"].as_str().unwrap().len(), 64);
        assert_eq!(response["provenance"]["engine_name"], "mtg-kernel");
        let extension = &response["extensions"]["x_kernel_v5"];
        let observation: Value =
            serde_json::from_str(extension["observation_json"].as_str().unwrap()).unwrap();
        assert!(observation.is_object());
        let legal_actions: Value =
            serde_json::from_str(extension["legal_actions_json"].as_str().unwrap()).unwrap();
        // The kernel v5 payload must carry the same ordered candidates.
        assert_eq!(legal_actions.as_array().unwrap().len(), candidates.len());
    }

    #[test]
    fn candidates_sha256_matches_independent_canonical_encoding() {
        let mut server = test_server();
        let response = parse_line(&server.handle_line(&reset_line("r-1", "g-hash", 7)));
        let candidates = response["candidates"].as_array().unwrap();
        // Independent path: serde_json's default map ordering is
        // code-point-sorted and its serializer is compact, so to_string of
        // the reduced array is the spec-canonical encoding.
        let reduced: Vec<Value> = candidates
            .iter()
            .map(|candidate| {
                json!({
                    "candidate_id": candidate["candidate_id"],
                    "semantic": candidate["semantic"],
                })
            })
            .collect();
        let canonical = serde_json::to_string(&reduced).unwrap();
        let digest = Sha256::digest(canonical.as_bytes());
        assert_eq!(
            response["candidates_sha256"].as_str().unwrap(),
            format!("{digest:x}")
        );
    }

    #[test]
    fn semantic_echo_round_trips_and_mismatch_fails_closed() {
        let mut server = test_server();
        let first = parse_line(&server.handle_line(&reset_line("r-1", "g-echo", 7)));
        assert_eq!(first["response_type"], "decision");

        let good = step_line_for("s-1", &first, 0);
        let next = parse_line(&server.handle_line(&good));
        assert_ne!(next["response_type"], "error");

        // A field-for-field different echo is refused without advancing.
        let second = match next["response_type"].as_str().unwrap() {
            "decision" => next,
            other => panic!("expected decision, got {other}"),
        };
        let mut wrong_echo = second["candidates"][0]["semantic"].clone();
        wrong_echo["kind"] = json!("definitely_not_the_kind");
        let wrong = json!({
            "request_type": "step",
            "protocol": SPELLBENCH_PROTOCOL_V1,
            "request_id": "s-2",
            "game_id": "g-echo",
            "expected_step": second["step"],
            "selection": {"candidate_id": 0, "semantic_echo": wrong_echo},
        })
        .to_string();
        let rejected = parse_line(&server.handle_line(&wrong));
        assert_eq!(rejected["response_type"], "error");
        assert_eq!(rejected["error"]["code"], "semantic_echo_mismatch");
        // The pending decision is still answerable afterwards.
        let recovered = parse_line(&server.handle_line(&step_line_for("s-3", &second, 0)));
        assert_ne!(recovered["response_type"], "error");
    }

    #[test]
    fn step_preconditions_map_to_the_closed_error_table() {
        let mut server = test_server();
        let before_reset = json!({
            "request_type": "step",
            "protocol": SPELLBENCH_PROTOCOL_V1,
            "request_id": "s-early",
            "game_id": "g-none",
            "expected_step": 0,
            "selection": {"candidate_id": 0, "semantic_echo": {"kind": "pass"}},
        })
        .to_string();
        let response = parse_line(&server.handle_line(&before_reset));
        assert_eq!(response["error"]["code"], "step_before_reset");

        let first = parse_line(&server.handle_line(&reset_line("r-1", "g-pre", 7)));
        let wrong_game = json!({
            "request_type": "step",
            "protocol": SPELLBENCH_PROTOCOL_V1,
            "request_id": "s-game",
            "game_id": "g-other",
            "expected_step": 0,
            "selection": {"candidate_id": 0, "semantic_echo": first["candidates"][0]["semantic"]},
        })
        .to_string();
        let response = parse_line(&server.handle_line(&wrong_game));
        assert_eq!(response["error"]["code"], "game_id_mismatch");

        let wrong_step = json!({
            "request_type": "step",
            "protocol": SPELLBENCH_PROTOCOL_V1,
            "request_id": "s-step",
            "game_id": "g-pre",
            "expected_step": 9,
            "selection": {"candidate_id": 0, "semantic_echo": first["candidates"][0]["semantic"]},
        })
        .to_string();
        let response = parse_line(&server.handle_line(&wrong_step));
        assert_eq!(response["error"]["code"], "expected_step_mismatch");

        let out_of_range = json!({
            "request_type": "step",
            "protocol": SPELLBENCH_PROTOCOL_V1,
            "request_id": "s-range",
            "game_id": "g-pre",
            "expected_step": 0,
            "selection": {
                "candidate_id": first["candidates"].as_array().unwrap().len() as u64,
                "semantic_echo": first["candidates"][0]["semantic"],
            },
        })
        .to_string();
        let response = parse_line(&server.handle_line(&out_of_range));
        assert_eq!(response["error"]["code"], "candidate_id_out_of_range");
    }

    #[test]
    fn reset_rejects_unsupported_format_decklist_unknown_catalog_and_reentry() {
        let mut server = test_server();
        let bad_format = json!({
            "request_type": "reset",
            "protocol": SPELLBENCH_PROTOCOL_V1,
            "request_id": "r-fmt",
            "game_id": "g-fmt",
            "format": "legacy-bo3",
            "seats": [
                {"seat": "p0", "deck": {"catalog_id": "Burn"}},
                {"seat": "p1", "deck": {"catalog_id": "Burn"}},
            ],
            "game_seed": 1,
            "max_decisions": 16,
            "max_steps": 2048,
        })
        .to_string();
        let response = parse_line(&server.handle_line(&bad_format));
        assert_eq!(response["error"]["code"], "unsupported_format");

        let decklist = json!({
            "request_type": "reset",
            "protocol": SPELLBENCH_PROTOCOL_V1,
            "request_id": "r-decklist",
            "game_id": "g-decklist",
            "format": "pauper-bo1",
            "seats": [
                {"seat": "p0", "deck": {"decklist": [{"name": "Mountain", "count": 60}]}},
                {"seat": "p1", "deck": {"catalog_id": "Burn"}},
            ],
            "game_seed": 1,
            "max_decisions": 16,
            "max_steps": 2048,
        })
        .to_string();
        let response = parse_line(&server.handle_line(&decklist));
        assert_eq!(response["error"]["code"], "unsupported_deck");

        let unknown = json!({
            "request_type": "reset",
            "protocol": SPELLBENCH_PROTOCOL_V1,
            "request_id": "r-unknown",
            "game_id": "g-unknown",
            "format": "pauper-bo1",
            "seats": [
                {"seat": "p0", "deck": {"catalog_id": "Not A Deck"}},
                {"seat": "p1", "deck": {"catalog_id": "Burn"}},
            ],
            "game_seed": 1,
            "max_decisions": 16,
            "max_steps": 2048,
        })
        .to_string();
        let response = parse_line(&server.handle_line(&unknown));
        assert_eq!(response["error"]["code"], "unsupported_deck");

        let first = parse_line(&server.handle_line(&reset_line("r-1", "g-real", 7)));
        assert_eq!(first["response_type"], "decision");
        let second = parse_line(&server.handle_line(&reset_line("r-2", "g-again", 7)));
        assert_eq!(second["error"]["code"], "game_already_active");
    }

    #[test]
    fn retry_is_single_entry_byte_exact_and_reuse_fails_closed() {
        let mut server = test_server();
        let reset = reset_line("r-1", "g-retry", 7);
        let first = server.handle_line(&reset);
        let replayed = server.handle_line(&reset);
        assert_eq!(replayed, first);

        let decision = parse_line(&first);
        let step = step_line_for("s-1", &decision, 0);
        let stepped = server.handle_line(&step);
        let stepped_replay = server.handle_line(&step);
        assert_eq!(stepped_replay, stepped);

        // Same request_id, different bytes: refused and non-mutating.
        let other_step =
            step_line_for("s-1", &decision, 0).replace("\"candidate_id\":0", "\"candidate_id\":1");
        let response = parse_line(&server.handle_line(&other_step));
        assert_eq!(response["error"]["code"], "request_id_reuse_mismatch");
        // The older cached entry is untouched: replaying the step that
        // followed still works only for its own id.
        let current = parse_line(&stepped);
        if current["response_type"] == "decision" {
            let next = step_line_for("s-2", &current, 0);
            let accepted = parse_line(&server.handle_line(&next));
            assert_ne!(accepted["response_type"], "error");
        }
    }

    #[test]
    fn terminal_maps_outcome_classification_and_counts() {
        let mut server = test_server();
        let reset = json!({
            "request_type": "reset",
            "protocol": SPELLBENCH_PROTOCOL_V1,
            "request_id": "r-cap",
            "game_id": "g-cap",
            "format": "pauper-bo1",
            "seats": [
                {"seat": "p0", "deck": {"catalog_id": "Burn"}},
                {"seat": "p1", "deck": {"catalog_id": "Burn"}},
            ],
            "game_seed": 3,
            "max_decisions": 0,
            "max_steps": 128,
        })
        .to_string();
        let terminal = parse_line(&server.handle_line(&reset));
        assert_eq!(terminal["response_type"], "terminal");
        assert_eq!(terminal["outcome"], "truncated");
        assert_eq!(terminal["classification"], "truncated");
        assert_eq!(terminal["winner"], Value::Null);
        assert_eq!(terminal["step_count"], 0);
        assert_eq!(terminal["decision_count"], 0);
        assert!(terminal["reason"].is_string());
        assert_eq!(terminal["provenance"]["engine_name"], "mtg-kernel");

        let late_step = json!({
            "request_type": "step",
            "protocol": SPELLBENCH_PROTOCOL_V1,
            "request_id": "s-late",
            "game_id": "g-cap",
            "expected_step": 0,
            "selection": {"candidate_id": 0, "semantic_echo": {"kind": "pass"}},
        })
        .to_string();
        let response = parse_line(&server.handle_line(&late_step));
        assert_eq!(response["error"]["code"], "game_already_terminal");

        let reentry = parse_line(&server.handle_line(&reset_line("r-2", "g-again", 7)));
        assert_eq!(
            reentry["response_type"], "decision",
            "a finished game frees the process"
        );
    }

    #[test]
    fn session_rejection_reason_names_the_code_as_the_kernel_spells_it() {
        for code in [
            RlSessionErrorCode::EpisodeAlreadyTerminal,
            RlSessionErrorCode::EpisodeIdMismatch,
            RlSessionErrorCode::ExpectedStepMismatch,
            RlSessionErrorCode::SelectedIndexOutOfRange,
            RlSessionErrorCode::SelectedActionIdMismatch,
            RlSessionErrorCode::SelectedActionIdUnknown,
            RlSessionErrorCode::StaleEnvironmentBinding,
            RlSessionErrorCode::UnsupportedDeck,
            RlSessionErrorCode::EnvironmentRandomization,
            RlSessionErrorCode::NonNaturalTerminal,
        ] {
            assert_eq!(
                serde_json::to_value(code).unwrap(),
                json!(session_error_code_name_v1(code))
            );
        }
        let error = RlSessionError {
            code: RlSessionErrorCode::StaleEnvironmentBinding,
            message: "selected action no longer matches the active policy environment".to_string(),
        };
        assert_eq!(
            session_rejection_reason_v1(&error),
            "engine_contract_failure:session_step_rejected_after_validation:\
             stale_environment_binding:selected action no longer matches the active policy environment"
        );
    }

    #[test]
    fn a_step_against_an_unsent_session_terminal_halts_with_the_session_reason() {
        let mut server = test_server();
        let capped = json!({
            "request_type": "reset",
            "protocol": SPELLBENCH_PROTOCOL_V1,
            "request_id": "r-cap",
            "game_id": "g-lost-terminal",
            "format": "pauper-bo1",
            "seats": [
                {"seat": "p0", "deck": {"catalog_id": "Burn"}},
                {"seat": "p1", "deck": {"catalog_id": "Burn"}},
            ],
            "game_seed": 3,
            "max_decisions": 0,
            "max_steps": 128,
        })
        .to_string();
        let terminal = parse_line(&server.handle_line(&capped));
        assert_eq!(terminal["response_type"], "terminal");
        // Defense in depth: were a session terminal ever left unsent, the
        // next step halts with the session's own terminal reason attached.
        server.game.as_mut().unwrap().terminal_sent = false;
        let step = json!({
            "request_type": "step",
            "protocol": SPELLBENCH_PROTOCOL_V1,
            "request_id": "s-0",
            "game_id": "g-lost-terminal",
            "expected_step": 0,
            "selection": {"candidate_id": 0, "semantic_echo": {"kind": "pass"}},
        })
        .to_string();
        let halted = parse_line(&server.handle_line(&step));
        assert_eq!(halted["response_type"], "terminal");
        assert_eq!(halted["outcome"], "halted");
        assert_eq!(
            halted["reason"],
            "engine_contract_failure:decision_expected_before_step:physical_decision_cap_reached:0"
        );
    }

    #[test]
    fn malformed_and_protocol_failures_use_the_closed_table() {
        let mut server = test_server();
        let response = parse_line(&server.handle_line("this is not json"));
        assert_eq!(response["error"]["code"], "malformed_json");
        assert_eq!(response["request_id"], "");

        let response = parse_line(&server.handle_line("{\"a\":1,\"a\":2}"));
        assert_eq!(response["error"]["code"], "malformed_json");

        let response = parse_line(&server.handle_line("{\"request_type\":\"hello\",\"protocol\":\"spellbench/v1\",\"request_id\":\"f\",\"game_seed\":1.5}"));
        assert_eq!(response["error"]["code"], "malformed_json");

        let response = parse_line(&server.handle_line("[1,2,3]"));
        assert_eq!(response["error"]["code"], "malformed_request");

        let response = parse_line(&server.handle_line(
            "{\"request_type\":\"hello\",\"protocol\":\"spellbench/v2\",\"request_id\":\"p-1\"}",
        ));
        assert_eq!(response["error"]["code"], "protocol_mismatch");
        assert_eq!(response["request_id"], "p-1");

        let response = parse_line(&server.handle_line(
            "{\"request_type\":\"hello\",\"protocol\":\"spellbench/v1\",\"request_id\":\"m-1\",\"surprise\":true}",
        ));
        assert_eq!(response["error"]["code"], "malformed_request");

        let response = parse_line(&server.handle_line(
            "{\"request_type\":\"teleport\",\"protocol\":\"spellbench/v1\",\"request_id\":\"m-2\"}",
        ));
        assert_eq!(response["error"]["code"], "malformed_request");

        // x_-prefixed fields are not licensed extension points on requests.
        let response = parse_line(&server.handle_line(
            "{\"request_type\":\"hello\",\"protocol\":\"spellbench/v1\",\"request_id\":\"m-3\",\"x_probe\":{}}",
        ));
        assert_eq!(response["error"]["code"], "malformed_request");

        // The process still serves valid requests after every failure.
        let response = parse_line(&server.handle_line(&hello_line("h-after")));
        assert_eq!(response["response_type"], "hello_ok");
    }

    #[test]
    fn burn_mirror_game_runs_to_terminal_with_valid_groups_and_hashes() {
        let mut server = test_server();
        let decision = parse_line(&server.handle_line(&reset_line("r-1", "g-full", 11)));
        let mut current = decision;
        let mut expected_group_id = 0_u64;
        let mut saw_multi_substep_group = false;
        let mut steps = 0_u64;
        loop {
            match current["response_type"].as_str().unwrap() {
                "decision" => {
                    assert_eq!(current["step"], steps);
                    let group = &current["group"];
                    let group_id = group["group_id"].as_u64().unwrap();
                    let substep_index = group["substep_index"].as_u64().unwrap();
                    let substep_count = group["substep_count"].as_u64().unwrap();
                    assert!(substep_count >= 1);
                    assert!(substep_index < substep_count);
                    if substep_index == 0 {
                        assert_eq!(group_id, expected_group_id);
                    }
                    if substep_count > 1 {
                        saw_multi_substep_group = true;
                        assert_eq!(group_id, expected_group_id);
                    }
                    if substep_index + 1 == substep_count {
                        expected_group_id += 1;
                    }
                    // Every candidate round-trips its echo; pick candidate 0.
                    let step = step_line_for(&format!("s-{steps}"), &current, 0);
                    current = parse_line(&server.handle_line(&step));
                    steps += 1;
                    assert!(steps <= 16, "capped game must terminate");
                }
                "terminal" => break,
                other => panic!("unexpected response type {other}"),
            }
        }
        assert_eq!(current["outcome"], "truncated");
        assert_eq!(current["classification"], "truncated");
        assert_eq!(current["step_count"], steps);
        assert!(current["decision_count"].as_u64().unwrap() <= 16);
        // The capped candidate-0 Burn mirror stays on one-substep groups;
        // combat scans are covered by the driver test that attacks.
        let _ = saw_multi_substep_group;
    }

    #[test]
    fn combat_inclusion_scan_exposes_substep_groups_when_reached() {
        // Scripted attacker policy: include every offered attacker, take the
        // first candidate otherwise. Drives a Burn mirror until a multi-
        // substep group (the attacker inclusion scan) appears or the cap
        // truncates the game.
        let mut server = test_server();
        let reset = json!({
            "request_type": "reset",
            "protocol": SPELLBENCH_PROTOCOL_V1,
            "request_id": "r-combat",
            "game_id": "g-combat",
            "format": "pauper-bo1",
            "seats": [
                {"seat": "p0", "deck": {"catalog_id": "Burn"}},
                {"seat": "p1", "deck": {"catalog_id": "Burn"}},
            ],
            "game_seed": 5151,
            "max_decisions": 512,
            "max_steps": 65536,
        })
        .to_string();
        let mut current = parse_line(&server.handle_line(&reset));
        let mut steps = 0_u64;
        let mut saw_inclusion_scan = false;
        let mut last_group_completed: Option<u64> = None;
        loop {
            match current["response_type"].as_str().unwrap() {
                "decision" => {
                    let group = &current["group"];
                    let substep_count = group["substep_count"].as_u64().unwrap();
                    let substep_index = group["substep_index"].as_u64().unwrap();
                    if substep_count > 1 {
                        saw_inclusion_scan = true;
                        // Within a group the group_id is pinned and substeps run dense.
                        assert_eq!(
                            group["group_id"].as_u64().unwrap(),
                            last_group_completed
                                .map_or(group["group_id"].as_u64().unwrap(), |completed| completed
                                    + 1)
                        );
                    }
                    if substep_index + 1 == substep_count {
                        last_group_completed = Some(group["group_id"].as_u64().unwrap());
                    }
                    let candidates = current["candidates"].as_array().unwrap();
                    let chosen = candidates
                        .iter()
                        .position(|candidate| {
                            candidate["semantic"]["kind"] == "choose_attacker_inclusion"
                                && candidate["semantic"]["include"] == true
                        })
                        .unwrap_or(0);
                    let step = step_line_for(&format!("s-{steps}"), &current, chosen);
                    current = parse_line(&server.handle_line(&step));
                    steps += 1;
                    assert!(steps <= 65_536, "game must respect the step cap");
                    if steps > 20_000 {
                        panic!("combat scan should appear well before 20k steps");
                    }
                }
                "terminal" => break,
                other => panic!("unexpected response type {other}"),
            }
            if saw_inclusion_scan {
                break;
            }
        }
        assert!(
            saw_inclusion_scan,
            "an attacking Burn mirror policy must reach a decomposed combat scan"
        );
    }

    #[test]
    fn port_maps_the_codex_lineage_enum_additions() {
        assert_eq!(
            session_error_code_name_v1(RlSessionErrorCode::NonNaturalTerminal),
            "non_natural_terminal"
        );
        let refused = RlSessionError {
            code: RlSessionErrorCode::NonNaturalTerminal,
            message: "non-natural".to_string(),
        };
        assert_eq!(session_error_to_bridge_code_v1(&refused), None);
        assert_eq!(
            optional_cost_choice_str_v1(OptionalCostChoice::ReturnPermanent),
            "return_permanent"
        );
        assert_eq!(
            optional_cost_choice_str_v1(OptionalCostChoice::SacrificeLand),
            "sacrifice_land"
        );
    }

    fn flat_v4_server() -> SpellbenchBridgeServerV1 {
        SpellbenchBridgeServerV1::with_options(
            SpellbenchEngineIdentityV1::with_source_revision(None),
            SpellbenchBridgeOptionsV1 {
                x_kernel_flat_v4: true,
                ..SpellbenchBridgeOptionsV1::default()
            },
        )
    }

    fn flat_v4_reset(game_id: &str, deck: &str, seed: u64) -> String {
        json!({"request_type": "reset", "protocol": "spellbench/v1", "request_id": format!("r-{game_id}"),
            "game_id": game_id, "format": "pauper-bo1",
            "seats": [{"seat": "p0", "deck": {"catalog_id": deck}}, {"seat": "p1", "deck": {"catalog_id": deck}}],
            "game_seed": seed, "max_decisions": 3000, "max_steps": 30000}).to_string()
    }

    fn flat_v4_step(decision: &Value, candidate_id: u64) -> String {
        json!({"request_type": "step", "protocol": "spellbench/v1", "request_id": format!("s-{}", decision["step"]),
            "game_id": decision["game_id"], "expected_step": decision["step"],
            "selection": {"candidate_id": candidate_id,
                "semantic_echo": decision["candidates"][candidate_id as usize]["semantic"]}}).to_string()
    }

    fn rows_of(decision: &Value) -> Vec<u64> {
        decision["extensions"]["x_kernel_flat_v4"]["row_candidate_ids"]
            .as_array()
            .unwrap()
            .iter()
            .map(|id| id.as_u64().unwrap())
            .collect()
    }

    #[test]
    fn flat_v4_is_opt_in_and_listed_in_hello() {
        let hello =
            json!({"request_type": "hello", "protocol": "spellbench/v1", "request_id": "h-1"})
                .to_string();
        let off: Value = serde_json::from_str(
            &SpellbenchBridgeServerV1::with_engine_identity(
                SpellbenchEngineIdentityV1::with_source_revision(None),
            )
            .handle_line(&hello),
        )
        .unwrap();
        assert_eq!(off["extensions"], json!(["x_kernel_v5"]));
        let on: Value = serde_json::from_str(&flat_v4_server().handle_line(&hello)).unwrap();
        assert_eq!(on["extensions"], json!(["x_kernel_v5", "x_kernel_flat_v4"]));
    }

    #[test]
    fn flat_v4_whole_games_carry_the_extension_on_every_decision_of_every_deck() {
        for deck in [
            "Wildfire", "Rally", "Affinity", "Elves", "Spy", "Burn", "CawGates", "Faeries",
        ] {
            for seed in [3_u64, 4] {
                let mut server = flat_v4_server();
                let mut line =
                    server.handle_line(&flat_v4_reset(&format!("g-{deck}-{seed}"), deck, seed));
                let mut rng = crate::state::SplitMix64::seed(seed);
                let mut longest = line.len();
                loop {
                    let response: Value = serde_json::from_str(&line).unwrap();
                    if response["response_type"] != "decision" {
                        let reason = response["reason"].as_str().unwrap_or("");
                        assert!(!reason.contains("flat_v4"), "{deck} seed {seed}: {reason}");
                        break;
                    }
                    let extension = &response["extensions"]["x_kernel_flat_v4"];
                    assert_eq!(extension["step"], response["step"]);
                    assert_eq!(extension["acting_seat"], response["acting_seat"]);
                    let rows = rows_of(&response);
                    assert!(
                        !rows.is_empty()
                            && rows.len() <= response["candidates"].as_array().unwrap().len()
                    );
                    let pick = rows[(rng.next_u64() % rows.len() as u64) as usize];
                    line = server.handle_line(&flat_v4_step(&response, pick));
                    longest = longest.max(line.len());
                }
                assert!(
                    longest < 1024 * 1024,
                    "{deck} seed {seed}: a {longest}-byte decision line"
                );
            }
        }
    }

    #[test]
    fn flat_v4_selection_outside_the_training_list_halts_the_game() {
        let mut server = flat_v4_server();
        let mut decision: Value =
            serde_json::from_str(&server.handle_line(&flat_v4_reset("g-filter", "Burn", 5)))
                .unwrap();
        while rows_of(&decision).len() < 2 {
            let only = rows_of(&decision)[0];
            decision =
                serde_json::from_str(&server.handle_line(&flat_v4_step(&decision, only))).unwrap();
        }
        let dropped = rows_of(&decision)[0] as u32;
        server
            .game
            .as_mut()
            .unwrap()
            .flat_v4_rows
            .retain(|id| *id != dropped);
        let terminal: Value =
            serde_json::from_str(&server.handle_line(&flat_v4_step(&decision, u64::from(dropped))))
                .unwrap();
        assert_eq!(terminal["response_type"], "terminal");
        assert_eq!(terminal["classification"], "halted");
        assert_eq!(
            terminal["reason"],
            format!(
                "engine_contract_failure:flat_v4_lockstep:selection_not_in_training_list:{dropped}"
            )
        );
    }

    #[test]
    fn flat_v4_lockstep_divergence_halts_the_game() {
        let mut server = flat_v4_server();
        let decision: Value =
            serde_json::from_str(&server.handle_line(&flat_v4_reset("g-diverge", "Burn", 6)))
                .unwrap();
        {
            let fast = server
                .game
                .as_mut()
                .unwrap()
                .flat_v4
                .as_mut()
                .unwrap()
                .fast_session_mut_for_test();
            let crate::rl_session::FastActorResponseV1::Decision(expected) =
                fast.current_response()
            else {
                panic!("decision")
            };
            fast.step(expected.episode_id, expected.step, 0).unwrap();
        }
        let terminal: Value = serde_json::from_str(
            &server.handle_line(&flat_v4_step(&decision, rows_of(&decision)[0])),
        )
        .unwrap();
        assert_eq!(terminal["classification"], "halted");
        assert!(
            terminal["reason"]
                .as_str()
                .unwrap()
                .starts_with("engine_contract_failure:flat_v4_lockstep:"),
            "{terminal}"
        );
    }

    #[test]
    fn flat_v4_encode_failure_halts_instead_of_emitting_a_decision() {
        let mut server = flat_v4_server();
        let decision: Value =
            serde_json::from_str(&server.handle_line(&flat_v4_reset("g-encode", "Burn", 7)))
                .unwrap();
        server.game.as_mut().unwrap().force_flat_v4_encode_error = true;
        let response: Value = serde_json::from_str(
            &server.handle_line(&flat_v4_step(&decision, rows_of(&decision)[0])),
        )
        .unwrap();
        assert_eq!(response["classification"], "halted");
        assert_eq!(
            response["reason"],
            "engine_contract_failure:flat_v4_encode:encode:forced by test"
        );
    }

    #[test]
    fn flat_v4_capped_games_truncate_instead_of_halting() {
        // The lockstep check compares terminals too: both sessions must reach
        // each cap with the identical truncated terminal.
        for (max_decisions, max_steps, reason) in [
            (0_u64, 2048_u64, "physical_decision_cap_reached:0"),
            (8, 2048, "physical_decision_cap_reached:8"),
            (3000, 12, "policy_step_cap_reached:12"),
        ] {
            let mut server = flat_v4_server();
            let reset = json!({"request_type": "reset", "protocol": "spellbench/v1",
                "request_id": "r-cap", "game_id": format!("g-cap-{max_decisions}-{max_steps}"),
                "format": "pauper-bo1",
                "seats": [{"seat": "p0", "deck": {"catalog_id": "Burn"}}, {"seat": "p1", "deck": {"catalog_id": "Burn"}}],
                "game_seed": 3, "max_decisions": max_decisions, "max_steps": max_steps})
            .to_string();
            let mut response = parse_line(&server.handle_line(&reset));
            while response["response_type"] == "decision" {
                response = parse_line(
                    &server.handle_line(&flat_v4_step(&response, rows_of(&response)[0])),
                );
            }
            assert_eq!(response["classification"], "truncated", "{response}");
            assert_eq!(response["reason"], reason, "{response}");
        }
    }

    /// A real session terminal after answered decisions: a Burn mirror capped
    /// at four physical decisions, answered with candidate 0.
    fn capped_session_terminal() -> RlSessionTerminalV1 {
        let mut session = RlEpisodeSessionV1::reset_with_decks_and_limits(
            41,
            3,
            4,
            128,
            ["Burn".to_string(), "Burn".to_string()],
        )
        .unwrap();
        loop {
            match session.current_response() {
                RlSessionResponseV1::Decision(decision) => {
                    let stable_id = decision.legal_actions[0].stable_id.clone();
                    session
                        .step(decision.episode_id, decision.step, 0, &stable_id)
                        .unwrap();
                }
                RlSessionResponseV1::Terminal(terminal) => return terminal,
            }
        }
    }

    /// `terminal` as the kernel reports a fail-closed halt (the fields
    /// `halted_terminal` in rl_session.rs sets).
    fn as_kernel_halt(mut terminal: RlSessionTerminalV1, reason: &str) -> RlSessionTerminalV1 {
        terminal.terminal_outcome = TerminalOutcomeV1::Halted;
        terminal.terminal_classification = TerminalClassificationV1::Halted;
        terminal.terminal_code = crate::rl::TerminalSafeCodeV2::FailClosed;
        terminal.winner = None;
        terminal.terminal_reward = [0, 0];
        terminal.terminal_reason = reason.to_string();
        terminal
    }

    #[test]
    fn a_kernel_halt_passes_through_the_bridge_with_its_own_reason() {
        let terminal = capped_session_terminal();
        assert!(terminal.policy_step_count > 0 && terminal.physical_decision_count > 0);
        let reason =
            "fail_closed:observation:linked-exile exact card incarnation is not uniquely in exile";
        let identity = SpellbenchEngineIdentityV1::with_source_revision(None);
        assert_eq!(
            terminal_response_value_v1(
                &identity,
                "s-9",
                "g-halt",
                &as_kernel_halt(terminal.clone(), reason)
            ),
            json!({
                "response_type": "terminal",
                "protocol": "spellbench/v1",
                "request_id": "s-9",
                "game_id": "g-halt",
                "outcome": "halted",
                "classification": "halted",
                "winner": null,
                "reason": reason,
                "step_count": terminal.policy_step_count,
                "decision_count": terminal.physical_decision_count,
                "provenance": identity.provenance_value(),
            })
        );
    }

    #[test]
    fn flat_v4_lets_a_raw_halt_keep_its_reason_and_still_checks_other_terminals() {
        let mut server = flat_v4_server();
        let decision: Value =
            serde_json::from_str(&server.handle_line(&flat_v4_reset("g-raw-halt", "Burn", 8)))
                .unwrap();
        assert_eq!(decision["response_type"], "decision");
        let truncated = capped_session_terminal();
        let halted = as_kernel_halt(truncated.clone(), "fail_closed:observation:probe");
        // The copy stands at its first decision, so the lockstep check calls
        // any terminal a divergence. A raw halt passes through before it.
        let game = server.game.as_mut().unwrap();
        assert_eq!(
            flat_v4_extension_v1(game, &RlSessionResponseV1::Terminal(halted)),
            Ok(None)
        );
        assert_eq!(
            flat_v4_extension_v1(game, &RlSessionResponseV1::Terminal(truncated)),
            Err("engine_contract_failure:flat_v4_lockstep:diverged:response_kind".to_string())
        );
    }

    /// A flagged server hosting a mirror game of `deck`, set up as
    /// `handle_reset` does but without its deck checks (reset refuses
    /// Terror), and the game's first decision.
    fn flat_v4_server_hosting(
        game_id: &str,
        deck: &str,
        seed: u64,
    ) -> (SpellbenchBridgeServerV1, Value) {
        let episode_id = spellbench_episode_id_v1(game_id);
        let decks: SessionDeckIdsV1 = [deck.to_string(), deck.to_string()];
        let session = RlEpisodeSessionV1::reset_with_decks_and_limits(
            episode_id,
            seed,
            3000,
            30000,
            decks.clone(),
        )
        .unwrap();
        let lockstep = FlatV4LockstepV1::reset(episode_id, seed, 3000, 30000, decks).unwrap();
        let mut game = SpellbenchBridgeGameV1 {
            game_id: game_id.to_string(),
            episode_id,
            session,
            terminal_sent: false,
            flat_v4: Some(lockstep),
            flat_v4_rows: Vec::new(),
            flat_v4_scratch: FlatV4EncodeScratchV1::default(),
            private_block_root_step: None,
            force_flat_v4_encode_error: false,
        };
        let response = game.session.current_response();
        let flat_v4 = flat_v4_extension_v1(&mut game, &response).unwrap();
        let counts = (
            game.session.policy_step_count(),
            game.session.physical_decision_count(),
        );
        let mut server = flat_v4_server();
        let WireResponseV1::Decision(decision) =
            server.session_response_to_wire("r-0", game_id, response, counts, flat_v4, None)
        else {
            panic!("{game_id}: the game starts without a decision")
        };
        server.game = Some(game);
        (server, decision)
    }

    #[test]
    fn a_raw_halt_keeps_its_reason_when_the_copy_cannot_take_the_same_step() {
        // Staging an Escape cast halts only the raw session (policy surface
        // v5 cannot represent it, which is why reset refuses Terror); the
        // copy plays on. With these picks, Terror seed 1 stages one (at step
        // 112 on this branch). There the copy is put one step ahead, so its
        // own step fails: the game is over, and the raw session's reason
        // must win.
        let (mut server, mut decision) = flat_v4_server_hosting("g-escape-1", "Terror", 1);
        let mut rng = crate::state::SplitMix64::seed(1);
        loop {
            assert_eq!(
                decision["response_type"], "decision",
                "no raw halt: {decision}"
            );
            let rows = rows_of(&decision);
            let pick = rows[(rng.next_u64() % rows.len() as u64) as usize];
            let game = server.game.as_mut().unwrap();
            let RlSessionResponseV1::Decision(current) = game.session.current_response() else {
                panic!("the bridge sent a decision")
            };
            // Would this step halt the raw session? Ask a clone.
            let raw_halt = match game.session.clone().step(
                game.episode_id,
                current.step,
                pick as u32,
                &current.legal_actions[pick as usize].stable_id,
            ) {
                Ok(RlSessionResponseV1::Terminal(terminal))
                    if terminal.terminal_classification == TerminalClassificationV1::Halted =>
                {
                    Some(terminal.terminal_reason)
                }
                _ => None,
            };
            if let Some(reason) = raw_halt {
                // One step ahead, the copy cannot take this step.
                let fast = game.flat_v4.as_mut().unwrap().fast_session_mut_for_test();
                let crate::rl_session::FastActorResponseV1::Decision(expected) =
                    fast.current_response()
                else {
                    panic!("the copy stands at a decision")
                };
                fast.step(expected.episode_id, expected.step, 0).unwrap();
                let terminal = parse_line(&server.handle_line(&flat_v4_step(&decision, pick)));
                assert_eq!(
                    reason,
                    "fail_closed:observation:policy schema v5 cannot represent a staged Escape \
                     graveyard-exile prefix; a versioned object-cost successor is required"
                );
                assert_eq!(terminal["classification"], "halted", "{terminal}");
                assert_eq!(terminal["reason"], reason, "{terminal}");
                return;
            }
            decision = parse_line(&server.handle_line(&flat_v4_step(&decision, pick)));
        }
    }

    #[test]
    fn flat_v4_a_copy_in_another_state_at_the_same_step_fails_the_equality_check() {
        let mut server = flat_v4_server();
        let decision = parse_line(&server.handle_line(&flat_v4_reset("g-other-state", "Burn", 6)));
        // Same step and decision, another game state: the copy's step
        // succeeds, so only the equality check can see the difference.
        server
            .game
            .as_mut()
            .unwrap()
            .flat_v4
            .as_mut()
            .unwrap()
            .fast_session_mut_for_test()
            .kernel_search_state_mut_for_test_v1()
            .players[1]
            .life -= 1;
        let terminal =
            parse_line(&server.handle_line(&flat_v4_step(&decision, rows_of(&decision)[0])));
        assert_eq!(terminal["classification"], "halted", "{terminal}");
        assert_eq!(
            terminal["reason"],
            "engine_contract_failure:flat_v4_lockstep:diverged:game_state"
        );
    }
}
