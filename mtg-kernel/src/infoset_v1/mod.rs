//! `x_kernel_infoset_v1`: the W1 search entrant's canonical information-set
//! state (collab LANES/lead/W1-ENTRANT-DESIGN.md v7, section 6).
//!
//! At a decision of seat V, [`export_v1`] turns the engine's true state into
//! a state that is a function of V's information only: tracked objects keep
//! their definitions under observer-derived `C:` handles, every unknown
//! hidden slot holds a fresh pristine object (`U:`) from a canonical refill,
//! history references outside the current span become per-event tombstones
//! (`H:`), incarnations follow the canonical epoch path and timestamps are
//! re-ranked. The result serializes to canonical bytes and imports back into
//! a flat-action session ([`crate::rl_session::FastActorSessionV1`]) that s4a
//! searches unmodified.
//!
//! Refusals (R1 to R8b) are viewer-derived: no payload, the policy answers.
//! Faults (I1 to I4) are engine invariants: the game halts as F1.

pub(crate) mod export;
pub(crate) mod ledger;
pub(crate) mod payload;
pub(crate) mod pins;
pub(crate) mod projection;
pub(crate) mod rewrite;

#[cfg(test)]
mod harness;
#[cfg(test)]
mod tests;

pub(crate) use export::{export_v1, ExportInputV1, ExportOptionsV1, InfosetV1, MutantV1};
pub(crate) use payload::PayloadV1;

use serde::Serialize;

/// Schema version written into every payload and receipt.
pub const INFOSET_SCHEMA_V1: &str = "x_kernel_infoset_v1";

/// Refusal predicates (design 6.7). Soaks report counts per predicate
/// (ruling 43).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum RefusalCodeV1 {
    R1,
    R1b,
    R2,
    R3,
    R4,
    R5,
    R6,
    R7,
    R8,
    R8b,
    /// Harness-only synthetic refusal of mutant L7.
    L7,
}

impl RefusalCodeV1 {
    pub const ALL: [RefusalCodeV1; 10] = [
        Self::R1,
        Self::R1b,
        Self::R2,
        Self::R3,
        Self::R4,
        Self::R5,
        Self::R6,
        Self::R7,
        Self::R8,
        Self::R8b,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::R1 => "R1",
            Self::R1b => "R1b",
            Self::R2 => "R2",
            Self::R3 => "R3",
            Self::R4 => "R4",
            Self::R5 => "R5",
            Self::R6 => "R6",
            Self::R7 => "R7",
            Self::R8 => "R8",
            Self::R8b => "R8b",
            Self::L7 => "L7",
        }
    }
}

/// Engine invariants (design 6.7 I1 to I4); a failure is fault F1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum FaultCodeV1 {
    I1,
    I2,
    I3,
    I4,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum ExportErrorV1 {
    /// No payload; the policy answers. Counted as a C5 fault.
    Refused { code: RefusalCodeV1, detail: String },
    /// Engine fault F1: the game halts, non-natural, for both seats.
    Fault { code: FaultCodeV1, detail: String },
}

impl ExportErrorV1 {
    pub(crate) fn refuse(code: RefusalCodeV1, detail: impl Into<String>) -> Self {
        Self::Refused {
            code,
            detail: detail.into(),
        }
    }
    pub(crate) fn fault(code: FaultCodeV1, detail: impl Into<String>) -> Self {
        Self::Fault {
            code,
            detail: detail.into(),
        }
    }
    /// The predicate's name, for per-predicate receipts.
    pub fn code_name(&self) -> &'static str {
        match self {
            Self::Refused { code, .. } => code.name(),
            Self::Fault { code, .. } => match code {
                FaultCodeV1::I1 => "I1",
                FaultCodeV1::I2 => "I2",
                FaultCodeV1::I3 => "I3",
                FaultCodeV1::I4 => "I4",
            },
        }
    }
}

/// An exported object's handle (design 6.2 step 7).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind")]
pub enum HandleV1 {
    /// A tracked object, numbered in the canonical order of 6.2.
    C { n: u32 },
    /// An unknown hidden slot of `owner`, numbered over hand and library
    /// together in step-3 order.
    U {
        owner: u8,
        i: u32,
        known_name: bool,
        pinned: bool,
    },
    /// A history tombstone: reference `j` of projected event `e`.
    H { e: u32, j: u32, named: bool },
}

impl HandleV1 {
    pub fn label(&self) -> String {
        match self {
            Self::C { n } => format!("C:{n}"),
            Self::U { owner, i, .. } => format!("U:{owner}:{i}"),
            Self::H { e, j, .. } => format!("H:{e}:{j}"),
        }
    }
}

/// Per-predicate refusal and fault counts, for soak and AC receipts.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct RefusalTallyV1 {
    pub exported: u64,
    pub by_code: std::collections::BTreeMap<String, u64>,
}

impl RefusalTallyV1 {
    pub fn record(&mut self, result: &Result<(), ExportErrorV1>) {
        match result {
            Ok(()) => self.exported += 1,
            Err(e) => *self.by_code.entry(e.code_name().to_string()).or_default() += 1,
        }
    }
}
