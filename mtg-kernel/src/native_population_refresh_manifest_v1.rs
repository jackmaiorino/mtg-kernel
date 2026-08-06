//! Canonical refresh authority for the scaled Net8 population program.
//!
//! The manifest is external to StoreV2 checkpoint authority. It binds the
//! exact eight-policy snapshot installed for one completed 128-update
//! boundary. Later Store integration may carry its digest, but pool mutation
//! and checkpoint loading remain outside this pure module.

use crate::canonical_json_v1::{
    from_canonical_json_bytes_v1, to_canonical_json_bytes_v1, CanonicalJsonErrorKindV1,
    CanonicalJsonErrorV1, CanonicalJsonNullPolicyV1,
};
use crate::native_training_store_digest_v1::{lower_hex_raw32_v1, sha256_v1};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt::{Display, Formatter};

pub(crate) const POPULATION_REFRESH_MANIFEST_SCHEMA_V1: &str =
    "mtg-kernel-scaled-selfplay-population-refresh/v1";
pub(crate) const POPULATION_PROGRAM_COMMIT_V1: &str = "838920e359c7a1152d97c450f4575c6be2309f22";
pub(crate) const POPULATION_PROGRAM_DOCUMENT_SHA256_V1: &str =
    "b0e836858379137e9f5068f1ed2d3cb98d0d6507d09170d8272caad2a989ea38";
pub(crate) const POPULATION_RETEST_MANIFEST_SHA256_V1: &str =
    "f3128e5f700830df2110d6abb06b5b6f7f8f642ac5064c5d3188afac93aed2c8";
pub(crate) const POPULATION_REPLAY_END_GENERATION_V1: u64 = 512;
pub(crate) const POPULATION_REFRESH_INTERVAL_V1: u64 = 128;
pub(crate) const POPULATION_BASE_REFRESH_COUNT_V1: u64 = 8;
pub(crate) const POPULATION_SLOT_COUNT_V1: usize = 8;
pub(crate) const POPULATION_WEIGHT_TOTAL_UNITS_V1: u64 = 1_000_000;
pub(crate) const POPULATION_ROLE_FLOOR_UNITS_V1: u64 = 200_000;
pub(crate) const POPULATION_POLICY_CAP_UNITS_V1: u64 = 250_000;

const EXPECTED_ROLES_V1: [&str; POPULATION_SLOT_COUNT_V1] = [
    "anchor-0",
    "anchor-1",
    "historical-0",
    "historical-1",
    "current-0",
    "current-1",
    "exploiter-0",
    "exploiter-1",
];

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PopulationRefreshSlotV1 {
    slot_index: u64,
    role: String,
    occupant_class: String,
    source_run_sha256: String,
    source_generation: u64,
    available_by_global_generation: u64,
    checkpoint_sha256: String,
    sidecar_sha256: String,
    state_sha256: String,
    model_parameter_sha256: String,
    weight_units: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct PopulationRefreshManifestWireV1 {
    schema: String,
    program_commit: String,
    program_document_sha256: String,
    retest_manifest_sha256: String,
    refresh_index: u64,
    program_update: u64,
    global_generation: u64,
    availability_generation: u64,
    weight_total_units: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    previous_manifest_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    payoff_panel_sha256: Option<String>,
    slots: Vec<PopulationRefreshSlotV1>,
}

#[derive(Clone, Debug)]
pub(crate) struct PopulationRefreshManifestV1 {
    wire: PopulationRefreshManifestWireV1,
    canonical_bytes: Vec<u8>,
    manifest_sha256: [u8; 32],
}

impl PopulationRefreshManifestV1 {
    pub(crate) fn canonical_bytes_v1(&self) -> &[u8] {
        &self.canonical_bytes
    }

    pub(crate) const fn manifest_sha256_v1(&self) -> [u8; 32] {
        self.manifest_sha256
    }

    pub(crate) const fn refresh_index_v1(&self) -> u64 {
        self.wire.refresh_index
    }

    pub(crate) const fn program_update_v1(&self) -> u64 {
        self.wire.program_update
    }

    pub(crate) const fn global_generation_v1(&self) -> u64 {
        self.wire.global_generation
    }

    pub(crate) fn slots_v1(&self) -> &[PopulationRefreshSlotV1] {
        &self.wire.slots
    }
}

impl PopulationRefreshSlotV1 {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new_v1(
        slot_index: u64,
        role: impl Into<String>,
        occupant_class: impl Into<String>,
        source_run_sha256: impl Into<String>,
        source_generation: u64,
        available_by_global_generation: u64,
        checkpoint_sha256: impl Into<String>,
        sidecar_sha256: impl Into<String>,
        state_sha256: impl Into<String>,
        model_parameter_sha256: impl Into<String>,
        weight_units: u64,
    ) -> Self {
        Self {
            slot_index,
            role: role.into(),
            occupant_class: occupant_class.into(),
            source_run_sha256: source_run_sha256.into(),
            source_generation,
            available_by_global_generation,
            checkpoint_sha256: checkpoint_sha256.into(),
            sidecar_sha256: sidecar_sha256.into(),
            state_sha256: state_sha256.into(),
            model_parameter_sha256: model_parameter_sha256.into(),
            weight_units,
        }
    }

    pub(crate) const fn slot_index_v1(&self) -> u64 {
        self.slot_index
    }

    pub(crate) fn role_v1(&self) -> &str {
        &self.role
    }

    pub(crate) const fn weight_units_v1(&self) -> u64 {
        self.weight_units
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PopulationRefreshManifestErrorKindV1 {
    CanonicalJson(CanonicalJsonErrorKindV1),
    InvalidAuthority,
    InvalidGeneration,
    InvalidChain,
    InvalidSlots,
    InvalidWeight,
    FutureCheckpoint,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PopulationRefreshManifestErrorV1 {
    kind: PopulationRefreshManifestErrorKindV1,
}

impl PopulationRefreshManifestErrorV1 {
    const fn new(kind: PopulationRefreshManifestErrorKindV1) -> Self {
        Self { kind }
    }

    pub(crate) const fn kind_v1(self) -> PopulationRefreshManifestErrorKindV1 {
        self.kind
    }
}

impl From<CanonicalJsonErrorV1> for PopulationRefreshManifestErrorV1 {
    fn from(error: CanonicalJsonErrorV1) -> Self {
        Self::new(PopulationRefreshManifestErrorKindV1::CanonicalJson(
            error.kind(),
        ))
    }
}

impl Display for PopulationRefreshManifestErrorV1 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{:?}", self.kind)
    }
}

impl Error for PopulationRefreshManifestErrorV1 {}

type Result<T> = std::result::Result<T, PopulationRefreshManifestErrorV1>;

pub(crate) fn build_population_refresh_manifest_v1(
    refresh_index: u64,
    previous: Option<&PopulationRefreshManifestV1>,
    payoff_panel_sha256: Option<&str>,
    slots: Vec<PopulationRefreshSlotV1>,
) -> Result<PopulationRefreshManifestV1> {
    let program_update = refresh_index
        .checked_mul(POPULATION_REFRESH_INTERVAL_V1)
        .ok_or_else(|| {
            PopulationRefreshManifestErrorV1::new(
                PopulationRefreshManifestErrorKindV1::InvalidGeneration,
            )
        })?;
    let global_generation = POPULATION_REPLAY_END_GENERATION_V1
        .checked_add(program_update)
        .ok_or_else(|| {
            PopulationRefreshManifestErrorV1::new(
                PopulationRefreshManifestErrorKindV1::InvalidGeneration,
            )
        })?;
    let wire = PopulationRefreshManifestWireV1 {
        schema: POPULATION_REFRESH_MANIFEST_SCHEMA_V1.to_owned(),
        program_commit: POPULATION_PROGRAM_COMMIT_V1.to_owned(),
        program_document_sha256: POPULATION_PROGRAM_DOCUMENT_SHA256_V1.to_owned(),
        retest_manifest_sha256: POPULATION_RETEST_MANIFEST_SHA256_V1.to_owned(),
        refresh_index,
        program_update,
        global_generation,
        availability_generation: global_generation,
        weight_total_units: POPULATION_WEIGHT_TOTAL_UNITS_V1,
        previous_manifest_sha256: previous
            .map(|manifest| lower_hex_raw32_v1(manifest.manifest_sha256_v1())),
        payoff_panel_sha256: payoff_panel_sha256.map(str::to_owned),
        slots,
    };
    let bytes = to_canonical_json_bytes_v1(&wire, CanonicalJsonNullPolicyV1::Forbid)?;
    decode_population_refresh_manifest_v1(&bytes, previous)
}

pub(crate) fn decode_population_refresh_manifest_v1(
    bytes: &[u8],
    previous: Option<&PopulationRefreshManifestV1>,
) -> Result<PopulationRefreshManifestV1> {
    let wire: PopulationRefreshManifestWireV1 =
        from_canonical_json_bytes_v1(bytes, CanonicalJsonNullPolicyV1::Forbid)?;
    let reencoded = to_canonical_json_bytes_v1(&wire, CanonicalJsonNullPolicyV1::Forbid)?;
    if reencoded != bytes {
        return Err(PopulationRefreshManifestErrorV1::new(
            PopulationRefreshManifestErrorKindV1::InvalidAuthority,
        ));
    }
    validate_wire_v1(&wire, previous)?;
    Ok(PopulationRefreshManifestV1 {
        manifest_sha256: sha256_v1(bytes),
        wire,
        canonical_bytes: bytes.to_vec(),
    })
}

fn validate_wire_v1(
    wire: &PopulationRefreshManifestWireV1,
    previous: Option<&PopulationRefreshManifestV1>,
) -> Result<()> {
    if wire.schema != POPULATION_REFRESH_MANIFEST_SCHEMA_V1
        || wire.program_commit != POPULATION_PROGRAM_COMMIT_V1
        || wire.program_document_sha256 != POPULATION_PROGRAM_DOCUMENT_SHA256_V1
        || wire.retest_manifest_sha256 != POPULATION_RETEST_MANIFEST_SHA256_V1
        || !is_sha256_v1(&wire.program_document_sha256)
        || !is_sha256_v1(&wire.retest_manifest_sha256)
    {
        return Err(PopulationRefreshManifestErrorV1::new(
            PopulationRefreshManifestErrorKindV1::InvalidAuthority,
        ));
    }
    let expected_program_update = wire
        .refresh_index
        .checked_mul(POPULATION_REFRESH_INTERVAL_V1)
        .ok_or_else(|| {
            PopulationRefreshManifestErrorV1::new(
                PopulationRefreshManifestErrorKindV1::InvalidGeneration,
            )
        })?;
    let expected_global_generation = POPULATION_REPLAY_END_GENERATION_V1
        .checked_add(expected_program_update)
        .ok_or_else(|| {
            PopulationRefreshManifestErrorV1::new(
                PopulationRefreshManifestErrorKindV1::InvalidGeneration,
            )
        })?;
    if wire.refresh_index > POPULATION_BASE_REFRESH_COUNT_V1
        || wire.program_update != expected_program_update
        || wire.global_generation != expected_global_generation
        || wire.availability_generation != expected_global_generation
    {
        return Err(PopulationRefreshManifestErrorV1::new(
            PopulationRefreshManifestErrorKindV1::InvalidGeneration,
        ));
    }
    match (wire.refresh_index, previous) {
        (0, None)
            if wire.previous_manifest_sha256.is_none() && wire.payoff_panel_sha256.is_none() => {}
        (0, _) => {
            return Err(PopulationRefreshManifestErrorV1::new(
                PopulationRefreshManifestErrorKindV1::InvalidChain,
            ));
        }
        (_, Some(previous)) => {
            let expected_previous = lower_hex_raw32_v1(previous.manifest_sha256_v1());
            if previous.refresh_index_v1().checked_add(1) != Some(wire.refresh_index)
                || wire.previous_manifest_sha256.as_deref() != Some(expected_previous.as_str())
                || wire
                    .payoff_panel_sha256
                    .as_deref()
                    .is_none_or(|value| !is_sha256_v1(value))
            {
                return Err(PopulationRefreshManifestErrorV1::new(
                    PopulationRefreshManifestErrorKindV1::InvalidChain,
                ));
            }
        }
        (_, None) => {
            return Err(PopulationRefreshManifestErrorV1::new(
                PopulationRefreshManifestErrorKindV1::InvalidChain,
            ));
        }
    }
    validate_slots_v1(wire)
}

fn validate_slots_v1(wire: &PopulationRefreshManifestWireV1) -> Result<()> {
    if wire.slots.len() != POPULATION_SLOT_COUNT_V1
        || wire.weight_total_units != POPULATION_WEIGHT_TOTAL_UNITS_V1
    {
        return Err(PopulationRefreshManifestErrorV1::new(
            PopulationRefreshManifestErrorKindV1::InvalidSlots,
        ));
    }
    let mut weight_sum = 0_u64;
    let mut model_hashes = std::collections::BTreeSet::new();
    let mut role_weights = [0_u64; 4];
    for (index, slot) in wire.slots.iter().enumerate() {
        let expected_index = u64::try_from(index).map_err(|_| {
            PopulationRefreshManifestErrorV1::new(
                PopulationRefreshManifestErrorKindV1::InvalidSlots,
            )
        })?;
        if slot.slot_index != expected_index
            || slot.role != EXPECTED_ROLES_V1[index]
            || !matches!(
                slot.occupant_class.as_str(),
                "policy" | "historical-fallback"
            )
            || !is_sha256_v1(&slot.source_run_sha256)
            || !is_sha256_v1(&slot.checkpoint_sha256)
            || !is_sha256_v1(&slot.sidecar_sha256)
            || !is_sha256_v1(&slot.state_sha256)
            || !is_sha256_v1(&slot.model_parameter_sha256)
        {
            return Err(PopulationRefreshManifestErrorV1::new(
                PopulationRefreshManifestErrorKindV1::InvalidSlots,
            ));
        }
        if slot.source_generation > slot.available_by_global_generation
            || slot.available_by_global_generation > wire.availability_generation
        {
            return Err(PopulationRefreshManifestErrorV1::new(
                PopulationRefreshManifestErrorKindV1::FutureCheckpoint,
            ));
        }
        if slot.weight_units == 0 || slot.weight_units > POPULATION_POLICY_CAP_UNITS_V1 {
            return Err(PopulationRefreshManifestErrorV1::new(
                PopulationRefreshManifestErrorKindV1::InvalidWeight,
            ));
        }
        weight_sum = weight_sum.checked_add(slot.weight_units).ok_or_else(|| {
            PopulationRefreshManifestErrorV1::new(
                PopulationRefreshManifestErrorKindV1::InvalidWeight,
            )
        })?;
        role_weights[index / 2] = role_weights[index / 2]
            .checked_add(slot.weight_units)
            .ok_or_else(|| {
                PopulationRefreshManifestErrorV1::new(
                    PopulationRefreshManifestErrorKindV1::InvalidWeight,
                )
            })?;
        if !model_hashes.insert(slot.model_parameter_sha256.as_str()) {
            return Err(PopulationRefreshManifestErrorV1::new(
                PopulationRefreshManifestErrorKindV1::InvalidSlots,
            ));
        }
    }
    if weight_sum != POPULATION_WEIGHT_TOTAL_UNITS_V1
        || role_weights
            .iter()
            .any(|weight| *weight < POPULATION_ROLE_FLOOR_UNITS_V1)
    {
        return Err(PopulationRefreshManifestErrorV1::new(
            PopulationRefreshManifestErrorKindV1::InvalidWeight,
        ));
    }
    Ok(())
}

fn is_sha256_v1(value: &str) -> bool {
    value.len() == 64
        && value
            .as_bytes()
            .iter()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn digest_v1(index: usize) -> String {
        format!("{index:064x}")
    }

    fn slots_v1(global_generation: u64) -> Vec<PopulationRefreshSlotV1> {
        (0..POPULATION_SLOT_COUNT_V1)
            .map(|index| {
                PopulationRefreshSlotV1::new_v1(
                    index as u64,
                    EXPECTED_ROLES_V1[index],
                    "policy",
                    digest_v1(10 + index),
                    global_generation.saturating_sub(128),
                    global_generation,
                    digest_v1(20 + index),
                    digest_v1(30 + index),
                    digest_v1(40 + index),
                    digest_v1(50 + index),
                    125_000,
                )
            })
            .collect()
    }

    fn mutate_v1(
        manifest: &PopulationRefreshManifestV1,
        mutation: impl FnOnce(&mut Value),
        previous: Option<&PopulationRefreshManifestV1>,
    ) -> PopulationRefreshManifestErrorKindV1 {
        let mut value: Value = serde_json::from_slice(manifest.canonical_bytes_v1()).unwrap();
        mutation(&mut value);
        let bytes = to_canonical_json_bytes_v1(&value, CanonicalJsonNullPolicyV1::Forbid).unwrap();
        decode_population_refresh_manifest_v1(&bytes, previous)
            .unwrap_err()
            .kind_v1()
    }

    #[test]
    fn initial_and_chained_refresh_round_trip_canonically() {
        let initial = build_population_refresh_manifest_v1(0, None, None, slots_v1(512)).unwrap();
        let next = build_population_refresh_manifest_v1(
            1,
            Some(&initial),
            Some(&digest_v1(90)),
            slots_v1(640),
        )
        .unwrap();
        assert_eq!(initial.program_update_v1(), 0);
        assert_eq!(initial.global_generation_v1(), 512);
        assert_eq!(next.program_update_v1(), 128);
        assert_eq!(next.global_generation_v1(), 640);
        assert_eq!(next.slots_v1().len(), 8);
        assert_eq!(
            decode_population_refresh_manifest_v1(next.canonical_bytes_v1(), Some(&initial))
                .unwrap()
                .canonical_bytes_v1(),
            next.canonical_bytes_v1()
        );
    }

    #[test]
    fn chain_and_generation_corruptions_fail_closed() {
        let initial = build_population_refresh_manifest_v1(0, None, None, slots_v1(512)).unwrap();
        let next = build_population_refresh_manifest_v1(
            1,
            Some(&initial),
            Some(&digest_v1(90)),
            slots_v1(640),
        )
        .unwrap();
        assert_eq!(
            mutate_v1(
                &next,
                |value| value["refresh_index"] = json!(2),
                Some(&initial)
            ),
            PopulationRefreshManifestErrorKindV1::InvalidGeneration
        );
        assert_eq!(
            mutate_v1(
                &next,
                |value| value["previous_manifest_sha256"] = json!(digest_v1(91)),
                Some(&initial)
            ),
            PopulationRefreshManifestErrorKindV1::InvalidChain
        );
        assert_eq!(
            decode_population_refresh_manifest_v1(next.canonical_bytes_v1(), None)
                .unwrap_err()
                .kind_v1(),
            PopulationRefreshManifestErrorKindV1::InvalidChain
        );
    }

    #[test]
    fn slot_weight_duplicate_and_future_corruptions_fail_closed() {
        let initial = build_population_refresh_manifest_v1(0, None, None, slots_v1(512)).unwrap();
        assert_eq!(
            mutate_v1(
                &initial,
                |value| value["slots"][0]["weight_units"] = json!(250_001),
                None
            ),
            PopulationRefreshManifestErrorKindV1::InvalidWeight
        );
        assert_eq!(
            mutate_v1(
                &initial,
                |value| value["slots"][1]["model_parameter_sha256"] =
                    value["slots"][0]["model_parameter_sha256"].clone(),
                None
            ),
            PopulationRefreshManifestErrorKindV1::InvalidSlots
        );
        assert_eq!(
            mutate_v1(
                &initial,
                |value| value["slots"][0]["available_by_global_generation"] = json!(513),
                None
            ),
            PopulationRefreshManifestErrorKindV1::FutureCheckpoint
        );
    }

    #[test]
    fn unknown_missing_and_authority_corruptions_fail_closed() {
        let initial = build_population_refresh_manifest_v1(0, None, None, slots_v1(512)).unwrap();
        assert!(matches!(
            mutate_v1(&initial, |value| value["unknown"] = json!(true), None),
            PopulationRefreshManifestErrorKindV1::CanonicalJson(_)
        ));
        assert_eq!(
            mutate_v1(
                &initial,
                |value| value["program_commit"] = json!("0".repeat(40)),
                None
            ),
            PopulationRefreshManifestErrorKindV1::InvalidAuthority
        );
        assert!(matches!(
            mutate_v1(
                &initial,
                |value| {
                    value.as_object_mut().unwrap().remove("slots");
                },
                None
            ),
            PopulationRefreshManifestErrorKindV1::CanonicalJson(_)
        ));
    }
}
