//! D9 learned-pilot adapter skeleton (`m4-interfaces/v2` / `v3`, M5 D11.3).
//!
//! Bridges M4/M5's [`GamePilot`] and [`PilotProvider`] contracts to M2's
//! native fast-session collector once M2 (`T1` through `T6`, `T15`, `T8`) and
//! M3 (`P1`, `gate`) freeze. Carries:
//! - fail-closed [`PilotPin`] and [`D9PilotConfig`] validation (decoding mode,
//!   London keep offset, card database hash, feature table, training math, and
//!   checkpoint/source-binary digests);
//! - [`FastSessionCollector`] trait boundary for M2's in-process collector;
//! - `encoder_refusal:<kind>` refusal-to-halt mapping (`GameEnd::Terminal`
//!   yielding pod-record `stop_kind = "terminal:encoder_refusal:<kind>"` and
//!   tripwire/census `cube_b_census_v1::stop_kind = "encoder_refusal:<kind>"`);
//! - [`D9PilotRegistry`] implementing [`PilotProvider`] for training and reads;
//! - [`verify_d9_collector_parity`] parity harness scaffold for D15 / M5
//!   qualification.

use crate::card_def::KERNEL_CARDDB_HASH;
use crate::cube_draft_v1::{game_decision_counter, Deck, GameCaps, GameEnd};
use crate::cube_seat_v1::{
    AttemptOutcome, AttemptSpec, GamePilot, PilotSampleCounts, UniformPilot,
};
use crate::cube_training_driver_v1::{PilotPin, PilotProvider, TrainingDriverError};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// M3 release decoding identifier (temperature-1 wide sampler).
pub const M3_SAMPLE_TEMP1_WIDE_V1: &str = "m3-sample-temp1-wide-v1";
/// `GameEnd::Terminal` classification recorded when the learned-pilot encoder
/// refuses a state or legal action set.
pub const ENCODER_REFUSAL_CLASSIFICATION: &str = "encoder_refusal";
/// Prefix prepended to the refusal kind inside `GameEnd::Terminal::code`,
/// producing pod-record `stop_kind = "terminal:encoder_refusal:<kind>"` and
/// tripwire/census `stop_kind = "encoder_refusal:<kind>"`.
pub const ENCODER_REFUSAL_CODE_PREFIX: &str = "encoder_refusal:";

/// Decoding mode certified by M3 and bound in D11.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum D9DecodingMode {
    /// Temperature-1 sampling over the full wide legal-action distribution.
    SampleTemp1Wide,
}

impl D9DecodingMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SampleTemp1Wide => M3_SAMPLE_TEMP1_WIDE_V1,
        }
    }

    pub fn from_str_name(text: &str) -> Option<Self> {
        match text {
            M3_SAMPLE_TEMP1_WIDE_V1 => Some(Self::SampleTemp1Wide),
            _ => None,
        }
    }
}

/// Errors returned by D9 configuration, pin verification, or parity checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum D9AdapterError {
    InvalidConfig(String),
    PinMismatch {
        field: &'static str,
        expected: String,
        actual: String,
    },
    UnknownCheckpoint(String),
    ParityMismatch(String),
}

impl fmt::Display for D9AdapterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfig(msg) => write!(f, "invalid D9 pilot config: {msg}"),
            Self::PinMismatch {
                field,
                expected,
                actual,
            } => write!(
                f,
                "D9 pilot pin mismatch on {field}: expected {expected:?}, got {actual:?}"
            ),
            Self::UnknownCheckpoint(pin) => {
                write!(f, "D9 pilot registry has no checkpoint for pin {pin:?}")
            }
            Self::ParityMismatch(msg) => write!(f, "D9 collector parity mismatch: {msg}"),
        }
    }
}

impl std::error::Error for D9AdapterError {}

impl From<D9AdapterError> for TrainingDriverError {
    fn from(error: D9AdapterError) -> Self {
        TrainingDriverError::Invalid(error.to_string())
    }
}

/// Immutable runtime configuration for one pinned learned pilot (`P1` or `gate`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct D9PilotConfig {
    pub pin: PilotPin,
    pub decoding_mode: D9DecodingMode,
    /// Exact IEEE-754 f32 bit pattern of the M3-signed London mulligan keep
    /// offset (must be finite).
    pub london_keep_offset_bits: u32,
    /// Expected 16-hex kernel card database hash of the running binary.
    pub expected_card_database_sha256: String,
}

impl D9PilotConfig {
    /// Builds a config bound to this binary's compiled [`KERNEL_CARDDB_HASH`].
    pub fn for_compiled_kernel(
        pin: PilotPin,
        decoding_mode: D9DecodingMode,
        london_keep_offset: f32,
    ) -> Self {
        Self {
            pin,
            decoding_mode,
            london_keep_offset_bits: london_keep_offset.to_bits(),
            expected_card_database_sha256: format!("{KERNEL_CARDDB_HASH:016x}"),
        }
    }

    pub fn london_keep_offset(&self) -> f32 {
        f32::from_bits(self.london_keep_offset_bits)
    }

    /// Fails closed on any missing or malformed digest, non-finite keep offset,
    /// decoding mismatch, or compiled card-database hash mismatch.
    pub fn validate(&self) -> Result<(), D9AdapterError> {
        let is_lower_hex = |s: &str, len: usize| {
            s.len() == len && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
        };
        for (name, value) in [
            ("checkpoint_sha256", &self.pin.checkpoint_sha256),
            ("after_state_sha256", &self.pin.after_state_sha256),
            ("table_sha256", &self.pin.table_sha256),
            ("source_binary_sha256", &self.pin.source_binary_sha256),
        ] {
            if !is_lower_hex(value, 64) {
                return Err(D9AdapterError::InvalidConfig(format!(
                    "pin.{name} must be a 64-character lowercase hex sha256"
                )));
            }
        }
        for (name, value) in [
            ("card_database_sha256", &self.pin.card_database_sha256),
            ("training_math", &self.pin.training_math),
            ("decoding", &self.pin.decoding),
        ] {
            if value.trim().is_empty() {
                return Err(D9AdapterError::InvalidConfig(format!(
                    "pin.{name} must be non-empty"
                )));
            }
        }
        if !self.london_keep_offset().is_finite() {
            return Err(D9AdapterError::InvalidConfig(
                "london_keep_offset must be finite".to_owned(),
            ));
        }
        if self.pin.decoding != self.decoding_mode.as_str() {
            return Err(D9AdapterError::PinMismatch {
                field: "decoding",
                expected: self.decoding_mode.as_str().to_owned(),
                actual: self.pin.decoding.clone(),
            });
        }
        let compiled_card_db = format!("{KERNEL_CARDDB_HASH:016x}");
        if self.expected_card_database_sha256 != compiled_card_db {
            return Err(D9AdapterError::PinMismatch {
                field: "expected_card_database_sha256",
                expected: compiled_card_db,
                actual: self.expected_card_database_sha256.clone(),
            });
        }
        if self.pin.card_database_sha256 != compiled_card_db {
            return Err(D9AdapterError::PinMismatch {
                field: "card_database_sha256",
                expected: compiled_card_db,
                actual: self.pin.card_database_sha256.clone(),
            });
        }
        Ok(())
    }
}

/// Checks exact equality on every field of two [`PilotPin`]s, naming the first
/// disagreeing field.
pub fn verify_pilot_pin_match(
    expected: &PilotPin,
    actual: &PilotPin,
) -> Result<(), D9AdapterError> {
    let checks: [(&'static str, &str, &str); 7] = [
        (
            "checkpoint_sha256",
            &expected.checkpoint_sha256,
            &actual.checkpoint_sha256,
        ),
        (
            "after_state_sha256",
            &expected.after_state_sha256,
            &actual.after_state_sha256,
        ),
        (
            "card_database_sha256",
            &expected.card_database_sha256,
            &actual.card_database_sha256,
        ),
        ("table_sha256", &expected.table_sha256, &actual.table_sha256),
        (
            "training_math",
            &expected.training_math,
            &actual.training_math,
        ),
        (
            "source_binary_sha256",
            &expected.source_binary_sha256,
            &actual.source_binary_sha256,
        ),
        ("decoding", &expected.decoding, &actual.decoding),
    ];
    for (field, exp, act) in checks {
        if exp != act {
            return Err(D9AdapterError::PinMismatch {
                field,
                expected: exp.to_owned(),
                actual: act.to_owned(),
            });
        }
    }
    Ok(())
}

/// Request handed by [`D9GamePilot`] to an M2 [`FastSessionCollector`].
#[derive(Debug, Clone)]
pub struct FastSessionRequest {
    pub decks: [Vec<u16>; 2],
    pub seats: [usize; 2],
    pub episode: u64,
    pub env_seed: u64,
    pub action_seed: u64,
    pub caps: GameCaps,
    pub decoding_mode: D9DecodingMode,
    pub london_keep_offset_bits: u32,
    /// Calling thread's live per-decision counter (`game_decision_counter()`).
    /// Collectors MUST store the running decision count here (`Ordering::Relaxed`)
    /// after every decision to uphold the `per-decision-v1` watchdog contract.
    pub decision_counter: Arc<AtomicU64>,
    /// When true, this attempt belongs to the deterministic logical-ID
    /// pilot-path sample (`m4-interfaces/v3` section 4) and the collector MUST
    /// populate `pilot_sample` on completion and encoder refusal.
    pub sample_pilot_path: bool,
}

/// Structured encoder refusal reported by an M2 collector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FastSessionRefusal {
    /// Short snake_case refusal category (e.g. `unsupported_zone`,
    /// `feature_overflow`), mapped into `encoder_refusal:<kind>`.
    pub kind: String,
    pub reason: String,
    pub decisions: u64,
    pub physical_decisions: Option<u64>,
    pub policy_steps: Option<u64>,
    pub last_action_source: Option<u16>,
    pub first_player_seat: Option<usize>,
    pub last_acting_seat: Option<usize>,
    pub pilot_sample: Option<PilotSampleCounts>,
}

/// Result returned by [`FastSessionCollector::play_attempt`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FastSessionOutcome {
    Finished {
        decisions: u64,
        physical_decisions: Option<u64>,
        policy_steps: Option<u64>,
        end: GameEnd,
        last_action_source: Option<u16>,
        first_player_seat: Option<usize>,
        last_acting_seat: Option<usize>,
        pilot_sample: Option<PilotSampleCounts>,
    },
    EncoderRefusal(FastSessionRefusal),
}

/// Abstraction boundary for M2's native fast-session collector (`T1`-`T6`/`T15`/`T8`).
///
/// Implementing this trait connects M2's frozen collector and M3's frozen model
/// directly to [`D9GamePilot`] and [`D9PilotRegistry`] without changing
/// `cube_pod_runner_v1` or `cube_training_driver_v1`.
pub trait FastSessionCollector: Send + Sync {
    /// Immutable pin describing the loaded checkpoint, table, and math binding.
    fn pin(&self) -> &PilotPin;

    /// Executes one full game attempt under the exact seeds, seats, caps, and
    /// M3 decoding settings in `request`.
    ///
    /// Implementations MUST update `request.decision_counter` (or equivalently
    /// `game_decision_counter()` on the calling thread) after every decision so
    /// the in-process watchdog and heartbeat observe live progress.
    fn play_attempt(&self, request: &FastSessionRequest) -> FastSessionOutcome;
}

fn sanitize_refusal_kind(kind: &str) -> String {
    let trimmed = kind.trim();
    if !trimmed.is_empty()
        && trimmed
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
    {
        trimmed.to_owned()
    } else {
        "unspecified".to_owned()
    }
}

/// Maps a [`FastSessionRefusal`] into the canonical halted [`GameEnd::Terminal`]
/// carrying `code = "encoder_refusal:<kind>"`.
pub fn encoder_refusal_game_end(kind: &str, reason: String) -> GameEnd {
    let clean_kind = sanitize_refusal_kind(kind);
    GameEnd::Terminal {
        code: format!("{ENCODER_REFUSAL_CODE_PREFIX}{clean_kind}"),
        classification: ENCODER_REFUSAL_CLASSIFICATION.to_owned(),
        reason,
    }
}

/// Learned-pilot [`GamePilot`] adapter backed by a pinned [`FastSessionCollector`].
pub struct D9GamePilot<C: FastSessionCollector> {
    config: D9PilotConfig,
    collector: C,
}

impl<C: FastSessionCollector> D9GamePilot<C> {
    /// Constructs a validated adapter, failing closed if `config` is invalid or
    /// `collector.pin()` disagrees with `config.pin` on any field.
    pub fn new(config: D9PilotConfig, collector: C) -> Result<Self, D9AdapterError> {
        config.validate()?;
        verify_pilot_pin_match(&config.pin, collector.pin())?;
        Ok(Self { config, collector })
    }

    pub fn config(&self) -> &D9PilotConfig {
        &self.config
    }

    pub fn collector(&self) -> &C {
        &self.collector
    }
}

impl<C: FastSessionCollector> D9GamePilot<C> {
    fn run_attempt(
        &self,
        spec: &AttemptSpec<'_>,
        caps: &GameCaps,
        sample_pilot_path: bool,
    ) -> AttemptOutcome {
        let counter = game_decision_counter();
        counter.store(0, Ordering::Relaxed);
        let request = FastSessionRequest {
            decks: [spec.decks[0].card_ids(), spec.decks[1].card_ids()],
            seats: spec.seats,
            episode: spec.episode,
            env_seed: spec.env_seed,
            action_seed: spec.action_seed,
            caps: *caps,
            decoding_mode: self.config.decoding_mode,
            london_keep_offset_bits: self.config.london_keep_offset_bits,
            decision_counter: Arc::clone(&counter),
            sample_pilot_path,
        };
        let result = catch_unwind(AssertUnwindSafe(|| self.collector.play_attempt(&request)));
        match result {
            Ok(FastSessionOutcome::Finished {
                decisions,
                physical_decisions,
                policy_steps,
                end,
                last_action_source,
                first_player_seat,
                last_acting_seat,
                pilot_sample,
            }) => {
                counter.store(decisions, Ordering::Relaxed);
                AttemptOutcome {
                    decisions,
                    physical_decisions,
                    policy_steps,
                    end,
                    last_action_source,
                    first_player_seat,
                    last_acting_seat: if decisions == 0 {
                        None
                    } else {
                        last_acting_seat
                    },
                    pilot_sample: if sample_pilot_path {
                        pilot_sample
                    } else {
                        None
                    },
                }
            }
            Ok(FastSessionOutcome::EncoderRefusal(refusal)) => {
                counter.store(refusal.decisions, Ordering::Relaxed);
                let pilot_sample = if sample_pilot_path {
                    match refusal.pilot_sample {
                        Some(mut counts) => {
                            if counts.refused_decisions == 0 {
                                counts.refused_decisions = 1;
                            }
                            Some(counts)
                        }
                        None if refusal.decisions == 0 => Some(PilotSampleCounts {
                            refused_decisions: 1,
                            ..PilotSampleCounts::default()
                        }),
                        None => None,
                    }
                } else {
                    None
                };
                AttemptOutcome {
                    decisions: refusal.decisions,
                    physical_decisions: refusal.physical_decisions,
                    policy_steps: refusal.policy_steps,
                    end: encoder_refusal_game_end(&refusal.kind, refusal.reason),
                    last_action_source: refusal.last_action_source,
                    first_player_seat: refusal.first_player_seat,
                    last_acting_seat: if refusal.decisions == 0 {
                        None
                    } else {
                        refusal.last_acting_seat
                    },
                    pilot_sample,
                }
            }
            Err(payload) => {
                let message = payload
                    .downcast_ref::<&str>()
                    .map(|s| (*s).to_owned())
                    .or_else(|| payload.downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "panic".to_owned());
                AttemptOutcome {
                    decisions: counter.load(Ordering::Relaxed),
                    physical_decisions: None,
                    policy_steps: None,
                    end: GameEnd::Panic { message },
                    last_action_source: None,
                    first_player_seat: None,
                    last_acting_seat: None,
                    pilot_sample: None,
                }
            }
        }
    }
}

impl<C: FastSessionCollector> GamePilot for D9GamePilot<C> {
    fn play(&self, spec: &AttemptSpec<'_>, caps: &GameCaps) -> AttemptOutcome {
        self.run_attempt(spec, caps, false)
    }

    fn play_sampled(&self, spec: &AttemptSpec<'_>, caps: &GameCaps) -> AttemptOutcome {
        self.run_attempt(spec, caps, true)
    }
}

struct RegisteredPilot {
    pin: PilotPin,
    pilot: Arc<dyn GamePilot + Sync>,
}

/// Multi-checkpoint [`PilotProvider`] serving pinned D9 pilots (`P1`, `gate`,
/// second M3 seed) with strict full-pin verification on every load.
#[derive(Default)]
pub struct D9PilotRegistry {
    uniform: Option<Arc<dyn GamePilot + Sync>>,
    pilots: BTreeMap<String, RegisteredPilot>,
}

impl D9PilotRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_uniform_pilot(mut self, uniform: Arc<dyn GamePilot + Sync>) -> Self {
        self.uniform = Some(uniform);
        self
    }

    /// Registers one validated [`D9GamePilot`]. Duplicate registration for an
    /// identical pin is idempotent and keeps the existing registered collector;
    /// any pin field difference fails closed.
    pub fn register<C: FastSessionCollector + 'static>(
        &mut self,
        pilot: D9GamePilot<C>,
    ) -> Result<(), D9AdapterError> {
        let pin = pilot.config().pin.clone();
        if let Some(existing) = self.pilots.get(&pin.checkpoint_sha256) {
            verify_pilot_pin_match(&existing.pin, &pin)?;
            return Ok(());
        }
        self.pilots.insert(
            pin.checkpoint_sha256.clone(),
            RegisteredPilot {
                pin,
                pilot: Arc::new(pilot),
            },
        );
        Ok(())
    }

    /// Resolves a pinned pilot after checking every field of `pin`.
    pub fn resolve(&self, pin: &PilotPin) -> Result<Arc<dyn GamePilot + Sync>, D9AdapterError> {
        let entry = self
            .pilots
            .get(&pin.checkpoint_sha256)
            .ok_or_else(|| D9AdapterError::UnknownCheckpoint(pin.checkpoint_sha256.clone()))?;
        verify_pilot_pin_match(&entry.pin, pin)?;
        Ok(entry.pilot.clone())
    }
}

impl PilotProvider for D9PilotRegistry {
    fn uniform_pilot(&self) -> Arc<dyn GamePilot + Sync> {
        self.uniform
            .clone()
            .unwrap_or_else(|| Arc::new(UniformPilot))
    }

    fn load_checkpoint(
        &self,
        pin: &PilotPin,
    ) -> Result<Arc<dyn GamePilot + Sync>, TrainingDriverError> {
        self.resolve(pin).map_err(TrainingDriverError::from)
    }
}

/// One test case in the D9 collector parity harness.
#[derive(Debug, Clone)]
pub struct D9ParityCase {
    pub label: String,
    pub decks: [Deck; 2],
    pub seats: [usize; 2],
    pub episode: u64,
    pub env_seed: u64,
    pub action_seed: u64,
    pub caps: GameCaps,
}

/// Summary report returned by [`verify_d9_collector_parity`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct D9ParityReport {
    pub cases_checked: usize,
    pub natural_ends: usize,
    pub encoder_refusals: usize,
    pub other_halts: usize,
}

/// Verifies exact [`AttemptOutcome`] parity between a `candidate` pilot (e.g.
/// [`D9GamePilot`] backed by M2's fast-session collector) and a `reference`
/// pilot across every case in `cases`.
pub fn verify_d9_collector_parity(
    reference: &dyn GamePilot,
    candidate: &dyn GamePilot,
    cases: &[D9ParityCase],
) -> Result<D9ParityReport, D9AdapterError> {
    if cases.is_empty() {
        return Err(D9AdapterError::InvalidConfig(
            "parity harness requires at least one case".to_owned(),
        ));
    }
    let mut natural_ends = 0usize;
    let mut encoder_refusals = 0usize;
    let mut other_halts = 0usize;
    for case in cases {
        let spec = AttemptSpec {
            decks: [&case.decks[0], &case.decks[1]],
            seats: case.seats,
            episode: case.episode,
            env_seed: case.env_seed,
            action_seed: case.action_seed,
        };
        let expected = reference.play(&spec, &case.caps);
        let actual = candidate.play(&spec, &case.caps);
        if expected != actual {
            return Err(D9AdapterError::ParityMismatch(format!(
                "case {:?}: expected {:?}, got {:?}",
                case.label, expected, actual
            )));
        }
        match &actual.end {
            GameEnd::Natural { .. } => natural_ends += 1,
            GameEnd::Terminal { code, .. } if code.starts_with(ENCODER_REFUSAL_CODE_PREFIX) => {
                encoder_refusals += 1;
            }
            _ => other_halts += 1,
        }
    }
    Ok(D9ParityReport {
        cases_checked: cases.len(),
        natural_ends,
        encoder_refusals,
        other_halts,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card_def::card_id_by_name;
    use crate::cube_draft_v1::CubePool;
    use crate::cube_pod_runner_v1::{
        is_halted, run_training_pod, run_training_pod_with_options, stop_kind,
        training_roles_with_older, uniform_training_chairs, validate_pod_record_json, PilotInfo,
        PilotKind, PodRecordVersion, PodRunnerError, PodStatus, PodUse, TrainingPodConfig,
        TrainingPodOptions,
    };
    use crate::cube_seat_v1::canonical_deck;

    fn sample_pin() -> PilotPin {
        PilotPin {
            checkpoint_sha256: "11".repeat(32),
            after_state_sha256: "22".repeat(32),
            card_database_sha256: format!("{KERNEL_CARDDB_HASH:016x}"),
            table_sha256: "33".repeat(32),
            training_math: "portable-math-v1+m3-v1".to_owned(),
            source_binary_sha256: "44".repeat(32),
            decoding: M3_SAMPLE_TEMP1_WIDE_V1.to_owned(),
        }
    }

    struct MockCollector {
        pin: PilotPin,
        refuse_action_seed_mod: Option<u64>,
        refuse_all: bool,
        expected_keep_offset_bits: u32,
    }

    impl FastSessionCollector for MockCollector {
        fn pin(&self) -> &PilotPin {
            &self.pin
        }

        fn play_attempt(&self, req: &FastSessionRequest) -> FastSessionOutcome {
            assert_eq!(req.decoding_mode, D9DecodingMode::SampleTemp1Wide);
            assert_eq!(req.london_keep_offset_bits, self.expected_keep_offset_bits);
            if self.refuse_all
                || self
                    .refuse_action_seed_mod
                    .is_some_and(|m| req.action_seed.is_multiple_of(m))
            {
                for step in 1..=7 {
                    req.decision_counter.store(step, Ordering::Relaxed);
                }
                let pilot_sample = req.sample_pilot_path.then(|| {
                    let mut counts = PilotSampleCounts {
                        decisions: 7,
                        refused_decisions: 1,
                        offered_decisions_null: 1,
                        offered_actions_null: 2,
                        chosen_null: 1,
                        ..PilotSampleCounts::default()
                    };
                    counts.offered_decisions_by_card.insert(req.decks[0][0], 6);
                    counts.offered_actions_by_card.insert(req.decks[0][0], 12);
                    counts.chosen_by_card.insert(req.decks[0][0], 6);
                    counts
                });
                return FastSessionOutcome::EncoderRefusal(FastSessionRefusal {
                    kind: "unsupported_zone".to_owned(),
                    reason: "mock encoder refusal".to_owned(),
                    decisions: 7,
                    physical_decisions: Some(7),
                    policy_steps: Some(9),
                    last_action_source: Some(req.decks[0][0]),
                    first_player_seat: Some(req.seats[0]),
                    last_acting_seat: Some(req.seats[0]),
                    pilot_sample,
                });
            }
            for step in 1..=12 {
                req.decision_counter.store(step, Ordering::Relaxed);
            }
            let winner = req.seats[(req.action_seed & 1) as usize];
            let pilot_sample = req.sample_pilot_path.then(|| {
                let mut counts = PilotSampleCounts {
                    decisions: 12,
                    ..PilotSampleCounts::default()
                };
                counts.offered_decisions_by_card.insert(req.decks[1][0], 12);
                counts.offered_actions_by_card.insert(req.decks[1][0], 24);
                counts.chosen_by_card.insert(req.decks[1][0], 12);
                counts
            });
            FastSessionOutcome::Finished {
                decisions: 12,
                physical_decisions: Some(12),
                policy_steps: Some(15),
                end: GameEnd::Natural {
                    winner_seat: Some(winner),
                    reason: "mock_natural".to_owned(),
                },
                last_action_source: Some(req.decks[1][0]),
                first_player_seat: Some(req.seats[0]),
                last_acting_seat: Some(req.seats[1]),
                pilot_sample,
            }
        }
    }

    #[test]
    fn d9_config_and_pin_verification_fail_closed_on_every_mismatch() {
        let pin = sample_pin();
        let cfg =
            D9PilotConfig::for_compiled_kernel(pin.clone(), D9DecodingMode::SampleTemp1Wide, 0.25);
        assert!(cfg.validate().is_ok());

        let mut bad_db = cfg.clone();
        bad_db.pin.card_database_sha256 = "0000000000000000".to_owned();
        assert!(matches!(
            bad_db.validate(),
            Err(D9AdapterError::PinMismatch {
                field: "card_database_sha256",
                ..
            })
        ));

        // JSON config carrying the same wrong card DB hash in both fields must still fail closed.
        let mut both_wrong = cfg.clone();
        both_wrong.pin.card_database_sha256 = "0000000000000000".to_owned();
        both_wrong.expected_card_database_sha256 = "0000000000000000".to_owned();
        let roundtripped: D9PilotConfig =
            serde_json::from_str(&serde_json::to_string(&both_wrong).unwrap()).unwrap();
        assert!(matches!(
            roundtripped.validate(),
            Err(D9AdapterError::PinMismatch {
                field: "expected_card_database_sha256",
                ..
            })
        ));

        let mut empty_math = cfg.clone();
        empty_math.pin.training_math = "   ".to_owned();
        assert!(matches!(
            empty_math.validate(),
            Err(D9AdapterError::InvalidConfig(_))
        ));

        let mut bad_decoding = cfg.clone();
        bad_decoding.pin.decoding = "uniform-index-splitmix64-v1".to_owned();
        assert!(matches!(
            bad_decoding.validate(),
            Err(D9AdapterError::PinMismatch {
                field: "decoding",
                ..
            })
        ));

        for bad_offset in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let bad = D9PilotConfig::for_compiled_kernel(
                pin.clone(),
                D9DecodingMode::SampleTemp1Wide,
                bad_offset,
            );
            assert!(matches!(
                bad.validate(),
                Err(D9AdapterError::InvalidConfig(_))
            ));
        }

        let mut collector_pin = pin.clone();
        collector_pin.table_sha256 = "ff".repeat(32);
        let collector = MockCollector {
            pin: collector_pin,
            refuse_action_seed_mod: None,
            refuse_all: false,
            expected_keep_offset_bits: cfg.london_keep_offset_bits,
        };
        assert!(matches!(
            D9GamePilot::new(cfg.clone(), collector),
            Err(D9AdapterError::PinMismatch {
                field: "table_sha256",
                ..
            })
        ));
    }

    #[test]
    fn d9_registry_enforces_full_pin_match_on_load_checkpoint() {
        let pin = sample_pin();
        let cfg =
            D9PilotConfig::for_compiled_kernel(pin.clone(), D9DecodingMode::SampleTemp1Wide, -0.5);
        let pilot = D9GamePilot::new(
            cfg.clone(),
            MockCollector {
                pin: pin.clone(),
                refuse_action_seed_mod: None,
                refuse_all: false,
                expected_keep_offset_bits: cfg.london_keep_offset_bits,
            },
        )
        .unwrap();
        let mut registry = D9PilotRegistry::new();
        registry.register(pilot).unwrap();

        // Duplicate registration with a differing pin field for the same checkpoint fails closed.
        let mut altered_pin = pin.clone();
        altered_pin.table_sha256 = "aa".repeat(32);
        let altered_cfg = D9PilotConfig::for_compiled_kernel(
            altered_pin.clone(),
            D9DecodingMode::SampleTemp1Wide,
            -0.5,
        );
        let altered_pilot = D9GamePilot::new(
            altered_cfg.clone(),
            MockCollector {
                pin: altered_pin,
                refuse_action_seed_mod: None,
                refuse_all: false,
                expected_keep_offset_bits: altered_cfg.london_keep_offset_bits,
            },
        )
        .unwrap();
        assert!(matches!(
            registry.register(altered_pilot),
            Err(D9AdapterError::PinMismatch {
                field: "table_sha256",
                ..
            })
        ));

        assert!(registry.load_checkpoint(&pin).is_ok());

        let mut wrong_math = pin.clone();
        wrong_math.training_math = "different-math".to_owned();
        assert!(registry.load_checkpoint(&wrong_math).is_err());

        let mut unknown = pin;
        unknown.checkpoint_sha256 = "99".repeat(32);
        assert!(registry.load_checkpoint(&unknown).is_err());
    }

    #[test]
    fn d9_collector_advances_live_decision_counter_during_play() {
        use crate::cube_worker_liveness_v1::observe_decisions;
        use std::time::Duration;

        struct AdvancingCollector {
            pin: PilotPin,
        }
        impl FastSessionCollector for AdvancingCollector {
            fn pin(&self) -> &PilotPin {
                &self.pin
            }

            fn play_attempt(&self, req: &FastSessionRequest) -> FastSessionOutcome {
                for step in 1..=6u64 {
                    req.decision_counter.store(step, Ordering::Relaxed);
                    std::thread::sleep(Duration::from_millis(60));
                }
                FastSessionOutcome::Finished {
                    decisions: 6,
                    physical_decisions: Some(6),
                    policy_steps: Some(6),
                    end: GameEnd::Natural {
                        winner_seat: Some(req.seats[0]),
                        reason: "advancing".to_owned(),
                    },
                    last_action_source: None,
                    first_player_seat: Some(req.seats[0]),
                    last_acting_seat: Some(req.seats[0]),
                    pilot_sample: None,
                }
            }
        }

        let pin = sample_pin();
        let cfg =
            D9PilotConfig::for_compiled_kernel(pin.clone(), D9DecodingMode::SampleTemp1Wide, 0.0);
        let pilot = D9GamePilot::new(cfg, AdvancingCollector { pin }).unwrap();
        let forest = card_id_by_name("Forest").unwrap();
        let deck = canonical_deck(&[forest; 40]);
        let spec = AttemptSpec {
            decks: [&deck, &deck],
            seats: [0, 1],
            episode: 0,
            env_seed: 1,
            action_seed: 2,
        };
        // Total play time ~360ms > 200ms watchdog, but counter advances every ~60ms so it must not stall.
        let (outcome, stalled) = observe_decisions(Duration::from_millis(200), || {
            pilot.play(&spec, &GameCaps::default())
        });
        assert!(
            !stalled,
            "advancing decision counter must satisfy per-decision-v1 liveness"
        );
        assert_eq!(outcome.decisions, 6);
    }

    #[test]
    fn d9_encoder_refusal_maps_to_halt_and_integrates_with_pod_runner() {
        let end = encoder_refusal_game_end("unsupported_zone", "zone X".to_owned());
        assert!(is_halted(&end));
        assert_eq!(
            stop_kind(&end).as_deref(),
            Some("terminal:encoder_refusal:unsupported_zone")
        );
        assert_eq!(
            crate::cube_b_census_v1::stop_kind(&end).as_deref(),
            Some("encoder_refusal:unsupported_zone")
        );

        let forest = card_id_by_name("Forest").unwrap();
        let island = card_id_by_name("Island").unwrap();
        let d0 = canonical_deck(&[forest; 40]);
        let d1 = canonical_deck(&[island; 40]);

        let pin = sample_pin();
        let cfg =
            D9PilotConfig::for_compiled_kernel(pin.clone(), D9DecodingMode::SampleTemp1Wide, 0.125);
        let refusing_pilot = D9GamePilot::new(
            cfg.clone(),
            MockCollector {
                pin: pin.clone(),
                refuse_action_seed_mod: None,
                refuse_all: true,
                expected_keep_offset_bits: cfg.london_keep_offset_bits,
            },
        )
        .unwrap();

        // Parity harness checks natural + refusal cases cleanly.
        let mixed_pilot_a = D9GamePilot::new(
            cfg.clone(),
            MockCollector {
                pin: pin.clone(),
                refuse_action_seed_mod: Some(2),
                refuse_all: false,
                expected_keep_offset_bits: cfg.london_keep_offset_bits,
            },
        )
        .unwrap();
        let mixed_pilot_b = D9GamePilot::new(
            cfg.clone(),
            MockCollector {
                pin: pin.clone(),
                refuse_action_seed_mod: Some(2),
                refuse_all: false,
                expected_keep_offset_bits: cfg.london_keep_offset_bits,
            },
        )
        .unwrap();
        let cases = vec![
            D9ParityCase {
                label: "refusal-case".to_owned(),
                decks: [d0.clone(), d1.clone()],
                seats: [2, 5],
                episode: 0,
                env_seed: 101,
                action_seed: 202,
                caps: GameCaps::default(),
            },
            D9ParityCase {
                label: "natural-case".to_owned(),
                decks: [d1.clone(), d0.clone()],
                seats: [5, 2],
                episode: 1,
                env_seed: 103,
                action_seed: 203,
                caps: GameCaps::default(),
            },
        ];
        let report = verify_d9_collector_parity(&mixed_pilot_a, &mixed_pilot_b, &cases).unwrap();
        assert_eq!(report.cases_checked, 2);
        assert_eq!(report.natural_ends, 1);
        assert_eq!(report.encoder_refusals, 1);

        // Divergent collector fails closed in parity check.
        let natural_pilot = D9GamePilot::new(
            cfg.clone(),
            MockCollector {
                pin: pin.clone(),
                refuse_action_seed_mod: None,
                refuse_all: false,
                expected_keep_offset_bits: cfg.london_keep_offset_bits,
            },
        )
        .unwrap();
        assert!(matches!(
            verify_d9_collector_parity(&refusing_pilot, &natural_pilot, &cases),
            Err(D9AdapterError::ParityMismatch(_))
        ));

        // Sampled refusal populates refused_decisions and conserved chosen counts.
        let refusal_spec = AttemptSpec {
            decks: [&d0, &d1],
            seats: [2, 5],
            episode: 0,
            env_seed: 101,
            action_seed: 202,
        };
        let sampled_refusal = refusing_pilot.play_sampled(&refusal_spec, &GameCaps::default());
        assert_eq!(sampled_refusal.first_player_seat, Some(2));
        assert_eq!(sampled_refusal.last_acting_seat, Some(2));
        let sample = sampled_refusal
            .pilot_sample
            .expect("sampled refusal emits pilot_sample");
        assert_eq!(sample.decisions, 7);
        assert_eq!(sample.refused_decisions, 1);
        assert_eq!(
            sample.chosen_by_card.values().sum::<u64>() + sample.chosen_null,
            sample.decisions
        );

        // Through real run_training_pod (v2) and v3 with sampling: 4 consecutive encoder refusals
        // exhaust retries cleanly and emit a valid PodRecord with
        // pod-record stop_kind == terminal:encoder_refusal:unsupported_zone.
        let cube = CubePool::from_json_texts(
            include_str!("../../data/cube_v1/mtgo_vintage_cube_20261007.json"),
            include_str!("../../data/cube_v1/mtgo_vintage_cube_20261007_gaps.json"),
            None,
        )
        .unwrap();
        let mut pod_cfg = TrainingPodConfig::uniform_a2(PodUse::Training, "m5-train", "b1", 0, 1);
        pod_cfg.pilot_info = PilotInfo {
            kind: PilotKind::Checkpoint,
            pin: Some(pin.checkpoint_sha256.clone()),
            decoding: M3_SAMPLE_TEMP1_WIDE_V1.to_owned(),
        };
        let roles = training_roles_with_older([6, 7]).unwrap();
        let mut chairs = uniform_training_chairs(roles);
        let err = run_training_pod(&cube, &pod_cfg, &mut chairs, &refusing_pilot).unwrap_err();
        let PodRunnerError::Stopped(pod) = err else {
            panic!("expected Stopped pod on 4 encoder refusals, got {err:?}");
        };
        assert_eq!(pod.status, PodStatus::Stopped);
        assert_eq!(
            pod.games[0].attempts[0].stop_kind.as_deref(),
            Some("terminal:encoder_refusal:unsupported_zone")
        );
        let json = serde_json::to_string(&*pod).unwrap();
        validate_pod_record_json(&json).expect("stopped refusal pod record passes schema check");

        let mut v3_chairs = uniform_training_chairs(roles);
        let v3_opts = TrainingPodOptions {
            record_version: PodRecordVersion::V3,
            pilot_sample_denom: 1,
            host: "test-host",
        };
        let v3_err = run_training_pod_with_options(
            &cube,
            &pod_cfg,
            &mut v3_chairs,
            &refusing_pilot,
            &v3_opts,
            None,
        )
        .err()
        .expect("expected Stopped v3 pod on 4 encoder refusals");
        let PodRunnerError::Stopped(v3_pod) = v3_err else {
            panic!("expected Stopped v3 pod on 4 encoder refusals, got {v3_err:?}");
        };
        assert_eq!(
            v3_pod.games[0].attempts[0].stop_kind.as_deref(),
            Some("terminal:encoder_refusal:unsupported_zone")
        );
        assert!(v3_pod.games[0].chairs[0].is_some());
        assert_eq!(
            v3_pod.games[0].attempts[0].first_player_seat,
            v3_pod.games[0].chairs[0]
        );
        assert_eq!(
            v3_pod.games[0].attempts[0].last_acting_seat,
            v3_pod.games[0].chairs[0]
        );
        assert!(v3_pod.games[0].attempts[0].last_acting_arm.is_some());
        let v3_json = serde_json::to_string(&*v3_pod).unwrap();
        validate_pod_record_json(&v3_json)
            .expect("stopped v3 refusal pod record passes schema check");
    }
}
