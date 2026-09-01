//! Cell-centered advantage-baseline state for terminal_reinforce_value/v4.
//!
//! Contract: `docs/native_trainer_terminal_reinforce_value_v4_candidate_v1.md`
//! sections 2-4. One cell per (opponent behavior identity, learner role); the
//! value trained against is the state committed BEFORE the batch (strict
//! lag), and the successor state is derived only after the optimizer step
//! from the completed batch's per-cell decision-weighted residual means.
//! Arithmetic is pinned: residuals accumulate in batch order into an f64 sum,
//! the mean rounds once to f32 at the division, and the EMA step
//! `c' = (1 - BETA) * c + BETA * mean` runs entirely in f32.

use crate::native_training_store_digest_v1::sha256_v1;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

pub(crate) const NATIVE_BASELINE_STATE_SCHEMA_V4: &str = "mtg-kernel-native-baseline-state/v1";
pub(crate) const NATIVE_BASELINE_BETA_V4: f32 = 0.05;
pub(crate) const NATIVE_BASELINE_MAX_CELLS_V4: usize = 256;
/// Domain separator for the composed v4 train-state hash.
pub(crate) const NATIVE_TRAIN_STATE_V4_HASH_DOMAIN: &[u8] = b"mtg-kernel-train-state-v4";

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum BaselineRoleV4 {
    /// Learner in physical seat P0 (on the play under the production
    /// P0-first start).
    P0,
    /// Learner in physical seat P1 (on the draw).
    P1,
}

impl BaselineRoleV4 {
    pub(crate) const fn wire_v4(self) -> &'static str {
        match self {
            Self::P0 => "p0",
            Self::P1 => "p1",
        }
    }

    pub(crate) fn from_wire_v4(value: &str) -> Result<Self> {
        match value {
            "p0" => Ok(Self::P0),
            "p1" => Ok(Self::P1),
            _ => Err(NativeBaselineErrorV4::new(
                NativeBaselineErrorKindV4::InvalidRole,
            )),
        }
    }

    const fn byte_v4(self) -> u8 {
        match self {
            Self::P0 => 0,
            Self::P1 => 1,
        }
    }
}

/// One baseline cell key: the opponent's exact checkpoint manifest SHA-256
/// (behavior identity, never a mutable slot index) and the learner's role.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct BaselineCellKeyV4 {
    pub(crate) opponent_checkpoint_manifest_sha256: String,
    pub(crate) role: BaselineRoleV4,
}

impl BaselineCellKeyV4 {
    pub(crate) fn new_v4(
        opponent_checkpoint_manifest_sha256: impl Into<String>,
        role: BaselineRoleV4,
    ) -> Result<Self> {
        let opponent_checkpoint_manifest_sha256 = opponent_checkpoint_manifest_sha256.into();
        if !is_sha256_hex_v4(&opponent_checkpoint_manifest_sha256) {
            return Err(NativeBaselineErrorV4::new(
                NativeBaselineErrorKindV4::InvalidIdentity,
            ));
        }
        Ok(Self {
            opponent_checkpoint_manifest_sha256,
            role,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BaselineCellV4 {
    pub(crate) c_bits: u32,
    pub(crate) decision_count: u64,
    pub(crate) episode_count: u64,
}

/// One completed update's observation of one cell: the decision-weighted
/// residual sum (target - value), accumulated in batch order as f64, plus
/// the counts. `decision_count` is the number of learner physical terms in
/// the cell this update; it weights the mean exactly as the policy loss
/// weights its terms.
#[derive(Clone, Debug)]
pub(crate) struct BaselineObservationV4 {
    pub(crate) key: BaselineCellKeyV4,
    pub(crate) residual_sum_f64: f64,
    pub(crate) decision_count: u64,
    pub(crate) episode_count: u64,
}

/// The committed baseline state: a canonically ordered cell map.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct NativeBaselineStateV4 {
    cells: BTreeMap<BaselineCellKeyV4, BaselineCellV4>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NativeBaselineErrorKindV4 {
    InvalidRole,
    InvalidIdentity,
    InvalidCounts,
    NonFiniteResidual,
    DuplicateObservation,
    CellCapExceeded,
    InvalidWire,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct NativeBaselineErrorV4 {
    kind: NativeBaselineErrorKindV4,
}

impl NativeBaselineErrorV4 {
    const fn new(kind: NativeBaselineErrorKindV4) -> Self {
        Self { kind }
    }

    pub(crate) const fn kind_v4(self) -> NativeBaselineErrorKindV4 {
        self.kind
    }
}

impl Display for NativeBaselineErrorV4 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "native baseline state v4 error: {:?}", self.kind)
    }
}

impl Error for NativeBaselineErrorV4 {}

type Result<T> = std::result::Result<T, NativeBaselineErrorV4>;

fn is_sha256_hex_v4(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

impl NativeBaselineStateV4 {
    pub(crate) fn empty_v4() -> Self {
        Self::default()
    }

    pub(crate) fn cell_count_v4(&self) -> usize {
        self.cells.len()
    }

    pub(crate) fn cells_v4(&self) -> impl Iterator<Item = (&BaselineCellKeyV4, &BaselineCellV4)> {
        self.cells.iter()
    }

    /// The baseline value the trainer subtracts for a cell under the strict
    /// lag: the committed value, or exactly zero for a never-observed cell.
    pub(crate) fn c_for_cell_v4(&self, key: &BaselineCellKeyV4) -> f32 {
        self.cells
            .get(key)
            .map_or(0.0, |cell| f32::from_bits(cell.c_bits))
    }

    /// Derives the successor state from one completed update's per-cell
    /// observations. `self` is `c_t` (already used by the batch's policy
    /// terms); the return value is `c_{t+1}`. Unobserved cells carry forward
    /// unchanged; a genuinely new cell initializes from zero. The normative
    /// arithmetic per cell:
    ///
    /// ```text
    /// mean = (residual_sum_f64 / decision_count as f64) as f32
    /// c'   = (1.0f32 - BETA) * c + BETA * mean        // entirely f32
    /// ```
    pub(crate) fn apply_update_v4(
        &self,
        observations: &[BaselineObservationV4],
    ) -> Result<NativeBaselineStateV4> {
        let mut seen = std::collections::BTreeSet::new();
        let mut next = self.clone();
        for observation in observations {
            if observation.decision_count == 0
                || observation.episode_count == 0
                || observation.decision_count < observation.episode_count
            {
                return Err(NativeBaselineErrorV4::new(
                    NativeBaselineErrorKindV4::InvalidCounts,
                ));
            }
            if !observation.residual_sum_f64.is_finite() {
                return Err(NativeBaselineErrorV4::new(
                    NativeBaselineErrorKindV4::NonFiniteResidual,
                ));
            }
            if !seen.insert(observation.key.clone()) {
                return Err(NativeBaselineErrorV4::new(
                    NativeBaselineErrorKindV4::DuplicateObservation,
                ));
            }
            #[allow(clippy::cast_precision_loss)]
            let mean = (observation.residual_sum_f64 / observation.decision_count as f64) as f32;
            if !mean.is_finite() {
                return Err(NativeBaselineErrorV4::new(
                    NativeBaselineErrorKindV4::NonFiniteResidual,
                ));
            }
            let prior = next
                .cells
                .get(&observation.key)
                .copied()
                .unwrap_or(BaselineCellV4 {
                    c_bits: 0.0_f32.to_bits(),
                    decision_count: 0,
                    episode_count: 0,
                });
            let c = f32::from_bits(prior.c_bits);
            let c_next = (1.0_f32 - NATIVE_BASELINE_BETA_V4) * c + NATIVE_BASELINE_BETA_V4 * mean;
            if !c_next.is_finite() {
                return Err(NativeBaselineErrorV4::new(
                    NativeBaselineErrorKindV4::NonFiniteResidual,
                ));
            }
            let decision_count = prior
                .decision_count
                .checked_add(observation.decision_count)
                .ok_or_else(|| {
                    NativeBaselineErrorV4::new(NativeBaselineErrorKindV4::InvalidCounts)
                })?;
            let episode_count = prior
                .episode_count
                .checked_add(observation.episode_count)
                .ok_or_else(|| {
                    NativeBaselineErrorV4::new(NativeBaselineErrorKindV4::InvalidCounts)
                })?;
            next.cells.insert(
                observation.key.clone(),
                BaselineCellV4 {
                    c_bits: c_next.to_bits(),
                    decision_count,
                    episode_count,
                },
            );
        }
        if next.cells.len() > NATIVE_BASELINE_MAX_CELLS_V4 {
            return Err(NativeBaselineErrorV4::new(
                NativeBaselineErrorKindV4::CellCapExceeded,
            ));
        }
        Ok(next)
    }

    /// Canonical byte encoding: schema string, cell count as big-endian u64,
    /// then each cell in `BTreeMap` order as 64 ASCII identity bytes, one
    /// role byte, and big-endian `c_bits`/`decision_count`/`episode_count`.
    pub(crate) fn canonical_bytes_v4(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(64 + self.cells.len() * 85);
        bytes.extend_from_slice(NATIVE_BASELINE_STATE_SCHEMA_V4.as_bytes());
        bytes.extend_from_slice(&(self.cells.len() as u64).to_be_bytes());
        for (key, cell) in &self.cells {
            bytes.extend_from_slice(key.opponent_checkpoint_manifest_sha256.as_bytes());
            bytes.push(key.role.byte_v4());
            bytes.extend_from_slice(&cell.c_bits.to_be_bytes());
            bytes.extend_from_slice(&cell.decision_count.to_be_bytes());
            bytes.extend_from_slice(&cell.episode_count.to_be_bytes());
        }
        bytes
    }

    /// The composed v4 train-state hash:
    /// `SHA-256(domain || core_train_state_sha256 || canonical baseline bytes)`.
    /// The core hash is the unchanged v3 snapshot hash, so the baseline is
    /// covered without reimplementing tensor hashing.
    pub(crate) fn compose_train_state_sha256_v4(&self, core_state_sha256: [u8; 32]) -> [u8; 32] {
        let mut bytes = Vec::with_capacity(
            NATIVE_TRAIN_STATE_V4_HASH_DOMAIN.len() + 32 + 64 + self.cells.len() * 85,
        );
        bytes.extend_from_slice(NATIVE_TRAIN_STATE_V4_HASH_DOMAIN);
        bytes.extend_from_slice(&core_state_sha256);
        bytes.extend_from_slice(&self.canonical_bytes_v4());
        sha256_v1(&bytes)
    }

    pub(crate) fn to_wire_v4(&self) -> Vec<BaselineCellWireV4> {
        self.cells
            .iter()
            .map(|(key, cell)| BaselineCellWireV4 {
                opponent_checkpoint_manifest_sha256: key
                    .opponent_checkpoint_manifest_sha256
                    .clone(),
                role: key.role.wire_v4().to_owned(),
                c_f32_bits: format!("{:08x}", cell.c_bits),
                decision_count: cell.decision_count,
                episode_count: cell.episode_count,
            })
            .collect()
    }

    /// Decodes the wire form, enforcing canonical `(identity, role)` order,
    /// uniqueness, identity format, and the cell cap.
    pub(crate) fn from_wire_v4(wire: &[BaselineCellWireV4]) -> Result<Self> {
        if wire.len() > NATIVE_BASELINE_MAX_CELLS_V4 {
            return Err(NativeBaselineErrorV4::new(
                NativeBaselineErrorKindV4::CellCapExceeded,
            ));
        }
        let mut cells = BTreeMap::new();
        let mut previous: Option<BaselineCellKeyV4> = None;
        for entry in wire {
            let role = BaselineRoleV4::from_wire_v4(&entry.role)?;
            let key = BaselineCellKeyV4::new_v4(&entry.opponent_checkpoint_manifest_sha256, role)?;
            if let Some(previous_key) = &previous {
                if *previous_key >= key {
                    return Err(NativeBaselineErrorV4::new(
                        NativeBaselineErrorKindV4::InvalidWire,
                    ));
                }
            }
            if entry.c_f32_bits.len() != 8
                || !entry
                    .c_f32_bits
                    .bytes()
                    .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
            {
                return Err(NativeBaselineErrorV4::new(
                    NativeBaselineErrorKindV4::InvalidWire,
                ));
            }
            let c_bits = u32::from_str_radix(&entry.c_f32_bits, 16)
                .map_err(|_| NativeBaselineErrorV4::new(NativeBaselineErrorKindV4::InvalidWire))?;
            if !f32::from_bits(c_bits).is_finite() {
                return Err(NativeBaselineErrorV4::new(
                    NativeBaselineErrorKindV4::InvalidWire,
                ));
            }
            if entry.decision_count == 0
                || entry.episode_count == 0
                || entry.decision_count < entry.episode_count
            {
                return Err(NativeBaselineErrorV4::new(
                    NativeBaselineErrorKindV4::InvalidCounts,
                ));
            }
            previous = Some(key.clone());
            cells.insert(
                key,
                BaselineCellV4 {
                    c_bits,
                    decision_count: entry.decision_count,
                    episode_count: entry.episode_count,
                },
            );
        }
        Ok(Self { cells })
    }
}

/// Wire form of one baseline cell for the checkpoint-manifest v4 sibling.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BaselineCellWireV4 {
    pub(crate) opponent_checkpoint_manifest_sha256: String,
    pub(crate) role: String,
    pub(crate) c_f32_bits: String,
    pub(crate) decision_count: u64,
    pub(crate) episode_count: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(tag: u8, role: BaselineRoleV4) -> BaselineCellKeyV4 {
        BaselineCellKeyV4::new_v4(format!("ab{:062x}", u64::from(tag)), role).expect("key")
    }

    fn observation(
        tag: u8,
        role: BaselineRoleV4,
        residual_sum: f64,
        decisions: u64,
        episodes: u64,
    ) -> BaselineObservationV4 {
        BaselineObservationV4 {
            key: key(tag, role),
            residual_sum_f64: residual_sum,
            decision_count: decisions,
            episode_count: episodes,
        }
    }

    #[test]
    fn new_cell_initializes_from_zero_with_exact_ema_arithmetic_v4() {
        let state = NativeBaselineStateV4::empty_v4();
        let next = state
            .apply_update_v4(&[observation(1, BaselineRoleV4::P0, 30.0, 60, 32)])
            .expect("update");
        // mean = 30/60 = 0.5 exactly; c' = 0.95*0 + 0.05*0.5, computed in f32.
        let expected = (1.0_f32 - NATIVE_BASELINE_BETA_V4) * 0.0
            + NATIVE_BASELINE_BETA_V4 * ((30.0_f64 / 60.0_f64) as f32);
        assert_eq!(
            next.c_for_cell_v4(&key(1, BaselineRoleV4::P0)).to_bits(),
            expected.to_bits()
        );
        // Strict lag: the prior state is untouched.
        assert_eq!(state.c_for_cell_v4(&key(1, BaselineRoleV4::P0)), 0.0);
        assert_eq!(next.cell_count_v4(), 1);
    }

    #[test]
    fn unobserved_cells_carry_forward_and_observed_cells_compound_v4() {
        let state = NativeBaselineStateV4::empty_v4();
        let first = state
            .apply_update_v4(&[
                observation(1, BaselineRoleV4::P0, 30.0, 60, 32),
                observation(2, BaselineRoleV4::P1, -12.0, 48, 30),
            ])
            .expect("first");
        let second = first
            .apply_update_v4(&[observation(1, BaselineRoleV4::P0, -10.0, 40, 32)])
            .expect("second");
        // Cell 2 carried forward bit-identically.
        assert_eq!(
            second.c_for_cell_v4(&key(2, BaselineRoleV4::P1)).to_bits(),
            first.c_for_cell_v4(&key(2, BaselineRoleV4::P1)).to_bits()
        );
        // Cell 1 compounded from its committed value.
        let c1 = first.c_for_cell_v4(&key(1, BaselineRoleV4::P0));
        let mean = ((-10.0_f64) / 40.0_f64) as f32;
        let expected = (1.0_f32 - NATIVE_BASELINE_BETA_V4) * c1 + NATIVE_BASELINE_BETA_V4 * mean;
        assert_eq!(
            second.c_for_cell_v4(&key(1, BaselineRoleV4::P0)).to_bits(),
            expected.to_bits()
        );
        // Counts accumulate.
        let (_, cell) = second
            .cells_v4()
            .find(|(cell_key, _)| **cell_key == key(1, BaselineRoleV4::P0))
            .expect("cell");
        assert_eq!(cell.decision_count, 100);
        assert_eq!(cell.episode_count, 64);
    }

    #[test]
    fn invalid_observations_fail_closed_v4() {
        let state = NativeBaselineStateV4::empty_v4();
        let zero_decisions = state
            .apply_update_v4(&[observation(1, BaselineRoleV4::P0, 1.0, 0, 0)])
            .expect_err("zero counts");
        assert_eq!(
            zero_decisions.kind_v4(),
            NativeBaselineErrorKindV4::InvalidCounts
        );
        let non_finite = state
            .apply_update_v4(&[observation(1, BaselineRoleV4::P0, f64::NAN, 10, 5)])
            .expect_err("nan");
        assert_eq!(
            non_finite.kind_v4(),
            NativeBaselineErrorKindV4::NonFiniteResidual
        );
        let duplicate = state
            .apply_update_v4(&[
                observation(1, BaselineRoleV4::P0, 1.0, 10, 5),
                observation(1, BaselineRoleV4::P0, 2.0, 10, 5),
            ])
            .expect_err("duplicate");
        assert_eq!(
            duplicate.kind_v4(),
            NativeBaselineErrorKindV4::DuplicateObservation
        );
        // decision_count below episode_count is impossible (>=1 decision per
        // episode) and fails closed.
        let inverted = state
            .apply_update_v4(&[observation(1, BaselineRoleV4::P0, 1.0, 4, 5)])
            .expect_err("inverted counts");
        assert_eq!(inverted.kind_v4(), NativeBaselineErrorKindV4::InvalidCounts);
    }

    #[test]
    fn cell_cap_fails_closed_v4() {
        let state = NativeBaselineStateV4::empty_v4();
        let mut observations = Vec::new();
        for tag in 0..=128_u8 {
            observations.push(observation(tag, BaselineRoleV4::P0, 1.0, 4, 2));
            observations.push(observation(tag, BaselineRoleV4::P1, 1.0, 4, 2));
        }
        let error = state
            .apply_update_v4(&observations)
            .expect_err("cap exceeded");
        assert_eq!(error.kind_v4(), NativeBaselineErrorKindV4::CellCapExceeded);
    }

    #[test]
    fn canonical_bytes_and_composed_hash_are_stable_and_sensitive_v4() {
        let state = NativeBaselineStateV4::empty_v4();
        let a = state
            .apply_update_v4(&[observation(1, BaselineRoleV4::P0, 30.0, 60, 32)])
            .expect("a");
        let b = state
            .apply_update_v4(&[observation(1, BaselineRoleV4::P0, 30.0, 60, 32)])
            .expect("b");
        assert_eq!(a.canonical_bytes_v4(), b.canonical_bytes_v4());
        let core = [7_u8; 32];
        assert_eq!(
            a.compose_train_state_sha256_v4(core),
            b.compose_train_state_sha256_v4(core)
        );
        // The composed hash moves when the baseline moves and when the core
        // moves.
        let c = a
            .apply_update_v4(&[observation(1, BaselineRoleV4::P0, -30.0, 60, 32)])
            .expect("c");
        assert_ne!(
            a.compose_train_state_sha256_v4(core),
            c.compose_train_state_sha256_v4(core)
        );
        assert_ne!(
            a.compose_train_state_sha256_v4(core),
            a.compose_train_state_sha256_v4([8_u8; 32])
        );
        // Empty baseline still composes deterministically (genesis).
        assert_eq!(
            state.compose_train_state_sha256_v4(core),
            NativeBaselineStateV4::empty_v4().compose_train_state_sha256_v4(core)
        );
    }

    #[test]
    fn wire_round_trip_enforces_order_and_format_v4() {
        let state = NativeBaselineStateV4::empty_v4();
        let next = state
            .apply_update_v4(&[
                observation(2, BaselineRoleV4::P1, -12.0, 48, 30),
                observation(1, BaselineRoleV4::P0, 30.0, 60, 32),
                observation(1, BaselineRoleV4::P1, 6.0, 50, 32),
            ])
            .expect("update");
        let wire = next.to_wire_v4();
        assert_eq!(wire.len(), 3);
        let decoded = NativeBaselineStateV4::from_wire_v4(&wire).expect("decode");
        assert_eq!(decoded, next);
        // Out-of-order wire is rejected.
        let mut reversed = wire.clone();
        reversed.reverse();
        let error = NativeBaselineStateV4::from_wire_v4(&reversed).expect_err("order");
        assert_eq!(error.kind_v4(), NativeBaselineErrorKindV4::InvalidWire);
        // Malformed bits are rejected.
        let mut bad_bits = wire.clone();
        bad_bits[0].c_f32_bits = "xyz".to_owned();
        let error = NativeBaselineStateV4::from_wire_v4(&bad_bits).expect_err("bits");
        assert_eq!(error.kind_v4(), NativeBaselineErrorKindV4::InvalidWire);
        // Non-finite committed values are rejected.
        let mut nan_bits = wire;
        nan_bits[0].c_f32_bits = format!("{:08x}", f32::NAN.to_bits());
        let error = NativeBaselineStateV4::from_wire_v4(&nan_bits).expect_err("nan");
        assert_eq!(error.kind_v4(), NativeBaselineErrorKindV4::InvalidWire);
    }
}
