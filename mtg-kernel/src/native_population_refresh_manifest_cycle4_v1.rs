//! Canonical refresh authority for the cycle-4 population campaign.
//!
//! Versioned successor of `native_population_refresh_manifest_v1`; it
//! reinterprets nothing retroactively. Contract:
//! `docs/native_population_refresh_manifest_cycle4_v1.md` under the ratified
//! pre-registration `OX_CYCLE4_PREREG_SKETCH_V2.md`. The cycle-3 lesson is
//! binding here: a non-genesis manifest decodes only when the caller supplies
//! the payoff panel's exact bytes and their SHA-256 matches the declared
//! value, and any declared panel hash matching the placeholder pattern (48 or
//! more leading zero hex characters) is rejected outright. Weight arithmetic
//! (`mw_update_cycle4_v1`) runs on one designated machine; cross-platform
//! bit identity of the `exp` path is not claimed, matching the program-v1
//! practice.

use crate::canonical_json_v1::{
    from_canonical_json_bytes_v1, to_canonical_json_bytes_v1, CanonicalJsonErrorKindV1,
    CanonicalJsonErrorV1, CanonicalJsonNullPolicyV1,
};
use crate::native_training_store_digest_v1::{lower_hex_raw32_v1, sha256_v1};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt::{Display, Formatter};

pub(crate) const CYCLE4_REFRESH_MANIFEST_SCHEMA_V1: &str =
    "mtg-kernel-population-refresh-manifest-cycle4/v1";
/// SHA-256 of the ratified pre-registration sketch V2.
pub(crate) const CYCLE4_PREREG_SHA256_V1: &str =
    "c49bffd62084285328a24b11531d80d148cba0f5bad8083349b7d24856326481";
pub(crate) const CYCLE4_REFRESH_INTERVAL_V1: u64 = 128;
pub(crate) const CYCLE4_REFRESH_MAX_INDEX_V1: u64 = 16;
pub(crate) const CYCLE4_SLOT_COUNT_V1: usize = 8;
pub(crate) const CYCLE4_WEIGHT_TOTAL_UNITS_V1: u64 = 1_000_000;
pub(crate) const CYCLE4_ROLE_FLOOR_UNITS_V1: u64 = 200_000;
pub(crate) const CYCLE4_POLICY_CAP_UNITS_V1: u64 = 250_000;
pub(crate) const CYCLE4_GENESIS_SLOT_WEIGHT_UNITS_V1: u64 = 125_000;
pub(crate) const CYCLE4_TRAINEE_START_LOCAL_GENERATION_V1: u64 = 896;
pub(crate) const CYCLE4_HISTORICAL_LAG_V1: u64 = 512;
/// 28 matchups, `G = 256` games per matchup; each policy plays `7 * G` games.
pub(crate) const CYCLE4_PANEL_GAMES_PER_MATCHUP_V1: u64 = 256;
pub(crate) const CYCLE4_PANEL_GAMES_PER_POLICY_V1: u64 = 7 * CYCLE4_PANEL_GAMES_PER_MATCHUP_V1;
pub(crate) const CYCLE4_MW_ETA_V1: f64 = 0.10;
/// A declared panel hash with this many (or more) leading zero hex
/// characters is a placeholder pattern, never a genuine content hash.
pub(crate) const CYCLE4_PLACEHOLDER_ZERO_PREFIX_V1: usize = 48;

/// Cycle-3 trainee lineage (historical-0's source at refresh indices 0..=3).
pub(crate) const CYCLE4_CYCLE3_LINEAGE_RUN_SHA256_V1: &str =
    "f25a63d0a2968016c2d44220b02d46b642fad5c4d524cd7ed82d699dbfda83a1";
pub(crate) const CYCLE4_CYCLE3_LINEAGE_BASE_SEED_V1: u64 = 977_002;

const EXPECTED_ROLES_CYCLE4_V1: [&str; CYCLE4_SLOT_COUNT_V1] = [
    "anchor-0",
    "anchor-1",
    "historical-0",
    "historical-1",
    "current-0",
    "current-1",
    "exploiter-0",
    "exploiter-1",
];

/// One frozen occupant's exact identity: base seed, generation, and the five
/// store-identity hashes, in slot field order.
pub(crate) struct FrozenOccupantIdentityCycle4V1 {
    pub(crate) source_base_seed: u64,
    pub(crate) source_generation: u64,
    pub(crate) source_run_sha256: &'static str,
    pub(crate) checkpoint_manifest_sha256: &'static str,
    pub(crate) checkpoint_payload_sha256: &'static str,
    pub(crate) model_parameter_sha256: &'static str,
    pub(crate) train_state_sha256: &'static str,
}

/// anchor-0: promoted(2), seed 920012, generation 384.
pub(crate) const CYCLE4_ANCHOR_0_V1: FrozenOccupantIdentityCycle4V1 =
    FrozenOccupantIdentityCycle4V1 {
        source_base_seed: 920_012,
        source_generation: 384,
        source_run_sha256: "2c9b7423004428c0e2bb138afafc15ec65957f6bd98c4587bea704fbf9549aae",
        checkpoint_manifest_sha256:
            "4bd38cf3a9af3fb03fb04428fbc4286d4635007e848c7b9f0740122e430cbba8",
        checkpoint_payload_sha256:
            "a6c87366b2da9fc33923abab3c0e22d70c884cd9420477df3a475117be6beb99",
        model_parameter_sha256: "db58dbe3f1f76b5bdf3bae4de657711dc818393b2bf1eeae88c02d8866b4d01d",
        train_state_sha256: "fc471f85d28293d72b42dc61de628859173bd67426e251a51bfbbe86c7d586d8",
    };

/// anchor-1: program-v1 lineage 970002, generation 1536.
pub(crate) const CYCLE4_ANCHOR_1_V1: FrozenOccupantIdentityCycle4V1 =
    FrozenOccupantIdentityCycle4V1 {
        source_base_seed: 970_002,
        source_generation: 1536,
        source_run_sha256: "dc171fe72549154e533e337bc39884faa76811809abc0bc573bb975cea500a42",
        checkpoint_manifest_sha256:
            "8d6219e0c5acf040de202793b6f73131a30585ce3a1fea33b73e52734e91e53b",
        checkpoint_payload_sha256:
            "dc0f3c0d6ae9b4c87745c802b0b5b71b398b378d1689e9dd040c86ad12853ba2",
        model_parameter_sha256: "429446148ee88c527c307e0d9cde545a450a9f94e5be445b683d1d9955d93e53",
        train_state_sha256: "041dc02e23d51180f3f564d3070a6d9673ebc51339a73736e0d246c37614e602",
    };

/// historical-1 rotation: program-v1 seeds 970001/970002/970003, each at
/// generation 1024; index = `refresh_index % 3`.
pub(crate) const CYCLE4_HISTORICAL_1_ROTATION_V1: [FrozenOccupantIdentityCycle4V1; 3] = [
    FrozenOccupantIdentityCycle4V1 {
        source_base_seed: 970_001,
        source_generation: 1024,
        source_run_sha256: "3d41a6ddd18383e104563cc0c1d29011466961e54662df2aef89a64338dd0f81",
        checkpoint_manifest_sha256:
            "68df9ab80e5674e950cbf1e67cc2692d34e92c9e9b0dceb41577075cf5492b68",
        checkpoint_payload_sha256:
            "417877b68ffb217ff0d626c0ffe7abf00b3b80d55e8da272a619e22c00700113",
        model_parameter_sha256: "90d1d08cb9cf9f9b0b8016983292249b3b66667c36584d9c3d202a47fc658939",
        train_state_sha256: "7aa63f92188ef2a912ee9ca1c42e8b50c7f04a3b065468a5544439925b78d790",
    },
    FrozenOccupantIdentityCycle4V1 {
        source_base_seed: 970_002,
        source_generation: 1024,
        source_run_sha256: "dc171fe72549154e533e337bc39884faa76811809abc0bc573bb975cea500a42",
        checkpoint_manifest_sha256:
            "9e55dfa9dd2802c1886cfb5a2b53e736ed0bd71307cca42c2c0d8579831dceba",
        checkpoint_payload_sha256:
            "17f25b13a6a4f76f9ca99154783f87f578497b06db263cc4f34696f70e075117",
        model_parameter_sha256: "25961c9626a41b92e5ae1ff5c68933715061a15488d244a095b958d677a12558",
        train_state_sha256: "385975e0062b828b15daf21fc84c9c1a229e2626b95461efde0135cd6ba5fbba",
    },
    FrozenOccupantIdentityCycle4V1 {
        source_base_seed: 970_003,
        source_generation: 1024,
        source_run_sha256: "5816d5d3cca083e47dd4bf63245035f222e2d5edce778091a75d191cd8722e3a",
        checkpoint_manifest_sha256:
            "ac630be2ac39e6166d744d2be01fb252063dd5e93532e9be6d7f63a34a9cf7e4",
        checkpoint_payload_sha256:
            "48da0281a346c53bfe31d17828eee6f3f6df619a8132b5eabb8b94321d9d9dcd",
        model_parameter_sha256: "f7e57ef74f9f6c33edb85817c5fb0968ef44397b5c0c07a404e51f3da2fc0bf4",
        train_state_sha256: "9e18e7b5e053523bb9f5e22eedb4b877e23fe346535e7fec4a1f83e6055212c2",
    },
];

/// current-0: parent import, seed 975002, generation 2048.
pub(crate) const CYCLE4_CURRENT_0_V1: FrozenOccupantIdentityCycle4V1 =
    FrozenOccupantIdentityCycle4V1 {
        source_base_seed: 975_002,
        source_generation: 2048,
        source_run_sha256: "8d9a8287ef57651d5744d26275d2a8c0dc74cfb69cb7e1b2dd22691b5bd8a504",
        checkpoint_manifest_sha256:
            "5e1ff645091bfacdade2a3e06b47c3cd71c96ed1c9fee4dd9756b343d7c834fd",
        checkpoint_payload_sha256:
            "e4aa3172bf3962af1498028f19649a85424d0e30f226b5c1f6722160fb24a2d4",
        model_parameter_sha256: "67c5d0a2c506c0514623f3f4ea0f273b904662cbdae4f6ddc89c44e255b9a70d",
        train_state_sha256: "c528f15f2e354315ff757c5de61299e4297e9794ddd08b19109bf7ff1ca89a5e",
    };

/// exploiter-0: frozen de-novo fallback, seed 971222, generation 1024.
pub(crate) const CYCLE4_EXPLOITER_0_V1: FrozenOccupantIdentityCycle4V1 =
    FrozenOccupantIdentityCycle4V1 {
        source_base_seed: 971_222,
        source_generation: 1024,
        source_run_sha256: "c9bd4a75d9ac8b73951e5d681295bfb3b8d468f5e00775535b69e3cd05a963f1",
        checkpoint_manifest_sha256:
            "476594a1ad72e3180d4cf33ecb1e3034bb029cf19782595922fd8451ca5b6089",
        checkpoint_payload_sha256:
            "bcaf671f77788655e7b8b40dcbc5942dd89b10e25a3d570e4ac34464bc2c7f5b",
        model_parameter_sha256: "6b42f88ed103090e029814371e38412bb5afb4979ead8571f7d628ce24780c8d",
        train_state_sha256: "5098754de0f32f46c855e6db038a8fdd469f902fc0fb354da9a650cebe2b4600",
    };

/// exploiter-1: frozen de-novo fallback, seed 971221, generation 512.
pub(crate) const CYCLE4_EXPLOITER_1_V1: FrozenOccupantIdentityCycle4V1 =
    FrozenOccupantIdentityCycle4V1 {
        source_base_seed: 971_221,
        source_generation: 512,
        source_run_sha256: "7d111d855c09858cbc8404152cdf8878af3d8d5244dc3983ac25ddb2fe566232",
        checkpoint_manifest_sha256:
            "26ac933447f12cea8c09a5a1c3ba447883325e32252148c12e0838d59804dc22",
        checkpoint_payload_sha256:
            "f123b4e04fbbc49984fa3cc72f846e0098d70abf2e05a10fa52f8f32900d4400",
        model_parameter_sha256: "10ae7b2f28a3116ef01eb38edfb83e64cffa73a3ba7a169a0520a4d05c8b729b",
        train_state_sha256: "8378f2306a7576d0f39c8bf154fee7945cc700214a4f51c4ffd13eca341196cf",
    };

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Cycle4RefreshSlotV1 {
    pub(crate) slot_index: u64,
    pub(crate) role: String,
    pub(crate) occupant_class: String,
    pub(crate) source_base_seed: u64,
    pub(crate) source_run_sha256: String,
    pub(crate) source_generation: u64,
    pub(crate) checkpoint_manifest_sha256: String,
    pub(crate) checkpoint_payload_sha256: String,
    pub(crate) model_parameter_sha256: String,
    pub(crate) train_state_sha256: String,
    pub(crate) weight_units: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct Cycle4RefreshManifestWireV1 {
    schema: String,
    prereg_sha256: String,
    refresh_index: u64,
    program_update: u64,
    trainee_local_generation: u64,
    trainee_run_sha256: String,
    trainee_base_seed: u64,
    weight_total_units: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    previous_manifest_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    payoff_panel_sha256: Option<String>,
    slots: Vec<Cycle4RefreshSlotV1>,
}

#[derive(Clone, Debug)]
pub(crate) struct Cycle4RefreshManifestV1 {
    wire: Cycle4RefreshManifestWireV1,
    canonical_bytes: Vec<u8>,
    manifest_sha256: [u8; 32],
}

impl Cycle4RefreshManifestV1 {
    pub(crate) fn canonical_bytes_v1(&self) -> &[u8] {
        &self.canonical_bytes
    }

    pub(crate) const fn manifest_sha256_v1(&self) -> [u8; 32] {
        self.manifest_sha256
    }

    pub(crate) const fn refresh_index_v1(&self) -> u64 {
        self.wire.refresh_index
    }

    pub(crate) const fn trainee_local_generation_v1(&self) -> u64 {
        self.wire.trainee_local_generation
    }

    pub(crate) fn trainee_run_sha256_v1(&self) -> &str {
        &self.wire.trainee_run_sha256
    }

    pub(crate) const fn trainee_base_seed_v1(&self) -> u64 {
        self.wire.trainee_base_seed
    }

    pub(crate) fn slots_v1(&self) -> &[Cycle4RefreshSlotV1] {
        &self.wire.slots
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Cycle4RefreshManifestErrorKindV1 {
    CanonicalJson(CanonicalJsonErrorKindV1),
    InvalidAuthority,
    InvalidGeneration,
    InvalidChain,
    InvalidSlots,
    InvalidWeight,
    MissingPanelBytes,
    PanelContentMismatch,
    PlaceholderPanelHash,
    MwArithmetic,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Cycle4RefreshManifestErrorV1 {
    kind: Cycle4RefreshManifestErrorKindV1,
}

impl Cycle4RefreshManifestErrorV1 {
    const fn new(kind: Cycle4RefreshManifestErrorKindV1) -> Self {
        Self { kind }
    }

    pub(crate) const fn kind_v1(self) -> Cycle4RefreshManifestErrorKindV1 {
        self.kind
    }
}

impl From<CanonicalJsonErrorV1> for Cycle4RefreshManifestErrorV1 {
    fn from(error: CanonicalJsonErrorV1) -> Self {
        Self::new(Cycle4RefreshManifestErrorKindV1::CanonicalJson(
            error.kind(),
        ))
    }
}

impl Display for Cycle4RefreshManifestErrorV1 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{:?}", self.kind)
    }
}

impl Error for Cycle4RefreshManifestErrorV1 {}

type Result<T> = std::result::Result<T, Cycle4RefreshManifestErrorV1>;

fn is_sha256_cycle4_v1(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

/// True when a well-formed 64-hex value matches the placeholder pattern the
/// cycle-3 manifests carried: 48 or more leading `0` characters.
pub(crate) fn is_placeholder_hash_cycle4_v1(value: &str) -> bool {
    is_sha256_cycle4_v1(value)
        && value
            .bytes()
            .take(CYCLE4_PLACEHOLDER_ZERO_PREFIX_V1)
            .all(|byte| byte == b'0')
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn build_cycle4_refresh_manifest_v1(
    refresh_index: u64,
    previous: Option<&Cycle4RefreshManifestV1>,
    payoff_panel_bytes: Option<&[u8]>,
    trainee_run_sha256: &str,
    trainee_base_seed: u64,
    slots: Vec<Cycle4RefreshSlotV1>,
) -> Result<Cycle4RefreshManifestV1> {
    let program_update = refresh_index
        .checked_mul(CYCLE4_REFRESH_INTERVAL_V1)
        .ok_or_else(|| {
            Cycle4RefreshManifestErrorV1::new(Cycle4RefreshManifestErrorKindV1::InvalidGeneration)
        })?;
    let trainee_local_generation = CYCLE4_TRAINEE_START_LOCAL_GENERATION_V1
        .checked_add(program_update)
        .ok_or_else(|| {
            Cycle4RefreshManifestErrorV1::new(Cycle4RefreshManifestErrorKindV1::InvalidGeneration)
        })?;
    let wire = Cycle4RefreshManifestWireV1 {
        schema: CYCLE4_REFRESH_MANIFEST_SCHEMA_V1.to_owned(),
        prereg_sha256: CYCLE4_PREREG_SHA256_V1.to_owned(),
        refresh_index,
        program_update,
        trainee_local_generation,
        trainee_run_sha256: trainee_run_sha256.to_owned(),
        trainee_base_seed,
        weight_total_units: CYCLE4_WEIGHT_TOTAL_UNITS_V1,
        previous_manifest_sha256: previous
            .map(|manifest| lower_hex_raw32_v1(manifest.manifest_sha256_v1())),
        payoff_panel_sha256: payoff_panel_bytes.map(|bytes| lower_hex_raw32_v1(sha256_v1(bytes))),
        slots,
    };
    let bytes = to_canonical_json_bytes_v1(&wire, CanonicalJsonNullPolicyV1::Forbid)?;
    decode_cycle4_refresh_manifest_v1(&bytes, previous, payoff_panel_bytes)
}

/// Decodes and fully validates one manifest. For `refresh_index >= 1` the
/// caller MUST supply the payoff panel's exact bytes; there is no
/// format-only acceptance path.
pub(crate) fn decode_cycle4_refresh_manifest_v1(
    bytes: &[u8],
    previous: Option<&Cycle4RefreshManifestV1>,
    payoff_panel_bytes: Option<&[u8]>,
) -> Result<Cycle4RefreshManifestV1> {
    let wire: Cycle4RefreshManifestWireV1 =
        from_canonical_json_bytes_v1(bytes, CanonicalJsonNullPolicyV1::Forbid)?;
    let reencoded = to_canonical_json_bytes_v1(&wire, CanonicalJsonNullPolicyV1::Forbid)?;
    if reencoded != bytes {
        return Err(Cycle4RefreshManifestErrorV1::new(
            Cycle4RefreshManifestErrorKindV1::InvalidAuthority,
        ));
    }
    validate_wire_cycle4_v1(&wire, previous, payoff_panel_bytes)?;
    Ok(Cycle4RefreshManifestV1 {
        manifest_sha256: sha256_v1(bytes),
        wire,
        canonical_bytes: bytes.to_vec(),
    })
}

fn validate_wire_cycle4_v1(
    wire: &Cycle4RefreshManifestWireV1,
    previous: Option<&Cycle4RefreshManifestV1>,
    payoff_panel_bytes: Option<&[u8]>,
) -> Result<()> {
    if wire.schema != CYCLE4_REFRESH_MANIFEST_SCHEMA_V1
        || wire.prereg_sha256 != CYCLE4_PREREG_SHA256_V1
        || !is_sha256_cycle4_v1(&wire.trainee_run_sha256)
    {
        return Err(Cycle4RefreshManifestErrorV1::new(
            Cycle4RefreshManifestErrorKindV1::InvalidAuthority,
        ));
    }
    let expected_program_update = wire
        .refresh_index
        .checked_mul(CYCLE4_REFRESH_INTERVAL_V1)
        .ok_or_else(|| {
            Cycle4RefreshManifestErrorV1::new(Cycle4RefreshManifestErrorKindV1::InvalidGeneration)
        })?;
    let expected_local_generation = CYCLE4_TRAINEE_START_LOCAL_GENERATION_V1
        .checked_add(expected_program_update)
        .ok_or_else(|| {
            Cycle4RefreshManifestErrorV1::new(Cycle4RefreshManifestErrorKindV1::InvalidGeneration)
        })?;
    if wire.refresh_index > CYCLE4_REFRESH_MAX_INDEX_V1
        || wire.program_update != expected_program_update
        || wire.trainee_local_generation != expected_local_generation
    {
        return Err(Cycle4RefreshManifestErrorV1::new(
            Cycle4RefreshManifestErrorKindV1::InvalidGeneration,
        ));
    }
    match (wire.refresh_index, previous) {
        (0, None)
            if wire.previous_manifest_sha256.is_none() && wire.payoff_panel_sha256.is_none() => {}
        (0, _) => {
            return Err(Cycle4RefreshManifestErrorV1::new(
                Cycle4RefreshManifestErrorKindV1::InvalidChain,
            ));
        }
        (_, Some(previous_manifest)) => {
            let expected_previous = lower_hex_raw32_v1(previous_manifest.manifest_sha256_v1());
            if previous_manifest.refresh_index_v1().checked_add(1) != Some(wire.refresh_index)
                || wire.previous_manifest_sha256.as_deref() != Some(expected_previous.as_str())
                || previous_manifest.trainee_run_sha256_v1() != wire.trainee_run_sha256
                || previous_manifest.trainee_base_seed_v1() != wire.trainee_base_seed
            {
                return Err(Cycle4RefreshManifestErrorV1::new(
                    Cycle4RefreshManifestErrorKindV1::InvalidChain,
                ));
            }
            let declared_panel = wire.payoff_panel_sha256.as_deref().ok_or_else(|| {
                Cycle4RefreshManifestErrorV1::new(Cycle4RefreshManifestErrorKindV1::InvalidChain)
            })?;
            if !is_sha256_cycle4_v1(declared_panel) {
                return Err(Cycle4RefreshManifestErrorV1::new(
                    Cycle4RefreshManifestErrorKindV1::InvalidChain,
                ));
            }
            if is_placeholder_hash_cycle4_v1(declared_panel) {
                return Err(Cycle4RefreshManifestErrorV1::new(
                    Cycle4RefreshManifestErrorKindV1::PlaceholderPanelHash,
                ));
            }
            let panel_bytes = payoff_panel_bytes.ok_or_else(|| {
                Cycle4RefreshManifestErrorV1::new(
                    Cycle4RefreshManifestErrorKindV1::MissingPanelBytes,
                )
            })?;
            if lower_hex_raw32_v1(sha256_v1(panel_bytes)) != declared_panel {
                return Err(Cycle4RefreshManifestErrorV1::new(
                    Cycle4RefreshManifestErrorKindV1::PanelContentMismatch,
                ));
            }
        }
        (_, None) => {
            return Err(Cycle4RefreshManifestErrorV1::new(
                Cycle4RefreshManifestErrorKindV1::InvalidChain,
            ));
        }
    }
    validate_slots_cycle4_v1(wire)
}

fn slot_matches_frozen_v1(
    slot: &Cycle4RefreshSlotV1,
    frozen: &FrozenOccupantIdentityCycle4V1,
) -> bool {
    slot.source_base_seed == frozen.source_base_seed
        && slot.source_generation == frozen.source_generation
        && slot.source_run_sha256 == frozen.source_run_sha256
        && slot.checkpoint_manifest_sha256 == frozen.checkpoint_manifest_sha256
        && slot.checkpoint_payload_sha256 == frozen.checkpoint_payload_sha256
        && slot.model_parameter_sha256 == frozen.model_parameter_sha256
        && slot.train_state_sha256 == frozen.train_state_sha256
}

fn validate_slots_cycle4_v1(wire: &Cycle4RefreshManifestWireV1) -> Result<()> {
    if wire.slots.len() != CYCLE4_SLOT_COUNT_V1
        || wire.weight_total_units != CYCLE4_WEIGHT_TOTAL_UNITS_V1
    {
        return Err(Cycle4RefreshManifestErrorV1::new(
            Cycle4RefreshManifestErrorKindV1::InvalidSlots,
        ));
    }
    let invalid =
        || Cycle4RefreshManifestErrorV1::new(Cycle4RefreshManifestErrorKindV1::InvalidSlots);
    let mut weight_sum = 0_u64;
    let mut model_hashes = std::collections::BTreeSet::new();
    let mut role_weights = [0_u64; 4];
    for (index, slot) in wire.slots.iter().enumerate() {
        let expected_index = u64::try_from(index).map_err(|_| invalid())?;
        // Slots 0-5 carry live pool policies; slots 6-7 are frozen fallbacks
        // and keep that provenance in their occupant class (the no-exploiter
        // claim depends on it).
        let expected_class = if index >= 6 {
            "historical-fallback"
        } else {
            "policy"
        };
        if slot.slot_index != expected_index
            || slot.role != EXPECTED_ROLES_CYCLE4_V1[index]
            || slot.occupant_class != expected_class
            || !is_sha256_cycle4_v1(&slot.source_run_sha256)
            || !is_sha256_cycle4_v1(&slot.checkpoint_manifest_sha256)
            || !is_sha256_cycle4_v1(&slot.checkpoint_payload_sha256)
            || !is_sha256_cycle4_v1(&slot.model_parameter_sha256)
            || !is_sha256_cycle4_v1(&slot.train_state_sha256)
        {
            return Err(invalid());
        }
        validate_slot_assignment_cycle4_v1(wire, index, slot)?;
        if slot.weight_units == 0 || slot.weight_units > CYCLE4_POLICY_CAP_UNITS_V1 {
            return Err(Cycle4RefreshManifestErrorV1::new(
                Cycle4RefreshManifestErrorKindV1::InvalidWeight,
            ));
        }
        if wire.refresh_index == 0 && slot.weight_units != CYCLE4_GENESIS_SLOT_WEIGHT_UNITS_V1 {
            return Err(Cycle4RefreshManifestErrorV1::new(
                Cycle4RefreshManifestErrorKindV1::InvalidWeight,
            ));
        }
        weight_sum = weight_sum.checked_add(slot.weight_units).ok_or_else(|| {
            Cycle4RefreshManifestErrorV1::new(Cycle4RefreshManifestErrorKindV1::InvalidWeight)
        })?;
        role_weights[index / 2] = role_weights[index / 2]
            .checked_add(slot.weight_units)
            .ok_or_else(|| {
                Cycle4RefreshManifestErrorV1::new(Cycle4RefreshManifestErrorKindV1::InvalidWeight)
            })?;
        if !model_hashes.insert(slot.model_parameter_sha256.as_str()) {
            return Err(invalid());
        }
    }
    if weight_sum != CYCLE4_WEIGHT_TOTAL_UNITS_V1
        || role_weights
            .iter()
            .any(|weight| *weight < CYCLE4_ROLE_FLOOR_UNITS_V1)
    {
        return Err(Cycle4RefreshManifestErrorV1::new(
            Cycle4RefreshManifestErrorKindV1::InvalidWeight,
        ));
    }
    Ok(())
}

fn validate_slot_assignment_cycle4_v1(
    wire: &Cycle4RefreshManifestWireV1,
    index: usize,
    slot: &Cycle4RefreshSlotV1,
) -> Result<()> {
    let invalid =
        || Cycle4RefreshManifestErrorV1::new(Cycle4RefreshManifestErrorKindV1::InvalidSlots);
    match index {
        0 => {
            if !slot_matches_frozen_v1(slot, &CYCLE4_ANCHOR_0_V1) {
                return Err(invalid());
            }
        }
        1 => {
            if !slot_matches_frozen_v1(slot, &CYCLE4_ANCHOR_1_V1) {
                return Err(invalid());
            }
        }
        2 => {
            let lagged_generation = wire
                .trainee_local_generation
                .checked_sub(CYCLE4_HISTORICAL_LAG_V1)
                .ok_or_else(invalid)?;
            let (expected_run, expected_seed): (&str, u64) = if wire.refresh_index <= 3 {
                (
                    CYCLE4_CYCLE3_LINEAGE_RUN_SHA256_V1,
                    CYCLE4_CYCLE3_LINEAGE_BASE_SEED_V1,
                )
            } else {
                (wire.trainee_run_sha256.as_str(), wire.trainee_base_seed)
            };
            if slot.source_run_sha256 != expected_run
                || slot.source_base_seed != expected_seed
                || slot.source_generation != lagged_generation
            {
                return Err(invalid());
            }
        }
        3 => {
            let rotation_index = usize::try_from(wire.refresh_index % 3).map_err(|_| invalid())?;
            if !slot_matches_frozen_v1(slot, &CYCLE4_HISTORICAL_1_ROTATION_V1[rotation_index]) {
                return Err(invalid());
            }
        }
        4 => {
            if !slot_matches_frozen_v1(slot, &CYCLE4_CURRENT_0_V1) {
                return Err(invalid());
            }
        }
        5 => {
            if slot.source_run_sha256 != wire.trainee_run_sha256
                || slot.source_base_seed != wire.trainee_base_seed
                || slot.source_generation != wire.trainee_local_generation
            {
                return Err(invalid());
            }
        }
        6 => {
            if !slot_matches_frozen_v1(slot, &CYCLE4_EXPLOITER_0_V1) {
                return Err(invalid());
            }
        }
        7 => {
            if !slot_matches_frozen_v1(slot, &CYCLE4_EXPLOITER_1_V1) {
                return Err(invalid());
            }
        }
        _ => return Err(invalid()),
    }
    // Future-checkpoint impossibility is structural here: frozen slots are
    // exact-identity matches from completed runs, and slots 2 and 5 are bound
    // to generation formulas at or below the manifest's own
    // `trainee_local_generation`. No separate availability check is needed.
    Ok(())
}

/// Converts one policy's summed terminal ranks into the normalized MW input
/// `p_i = u_i / (7 * G)`; `|u|` may not exceed the games played.
pub(crate) fn panel_score_fraction_cycle4_v1(rank_sum: i64) -> Result<f64> {
    let games = i64::try_from(CYCLE4_PANEL_GAMES_PER_POLICY_V1).map_err(|_| {
        Cycle4RefreshManifestErrorV1::new(Cycle4RefreshManifestErrorKindV1::MwArithmetic)
    })?;
    // Range comparison rather than `abs()`: `i64::MIN.abs()` would overflow.
    if !(-games..=games).contains(&rank_sum) {
        return Err(Cycle4RefreshManifestErrorV1::new(
            Cycle4RefreshManifestErrorKindV1::MwArithmetic,
        ));
    }
    #[allow(clippy::cast_precision_loss)]
    Ok(rank_sum as f64 / games as f64)
}

/// The v1 multiplicative-weights update with the cycle-4 panel size:
/// `r_i = w_i * exp(eta * p_i)`, normalized, deterministically projected onto
/// the 25% policy cap and 20% role floors, then converted to exactly
/// 1,000,000 integer units by largest remainder (ascending-index ties) with
/// the one-unit role-floor repair.
pub(crate) fn mw_update_cycle4_v1(
    prior_weight_units: &[u64; CYCLE4_SLOT_COUNT_V1],
    panel_score_fractions: &[f64; CYCLE4_SLOT_COUNT_V1],
) -> Result<[u64; CYCLE4_SLOT_COUNT_V1]> {
    let arithmetic =
        || Cycle4RefreshManifestErrorV1::new(Cycle4RefreshManifestErrorKindV1::MwArithmetic);
    let cap = 0.25_f64;
    let role_floor = 0.20_f64;
    let mut weights = [0.0_f64; CYCLE4_SLOT_COUNT_V1];
    let mut sum = 0.0_f64;
    for index in 0..CYCLE4_SLOT_COUNT_V1 {
        let fraction = panel_score_fractions[index];
        if !fraction.is_finite() || fraction.abs() > 1.0 || prior_weight_units[index] == 0 {
            return Err(arithmetic());
        }
        #[allow(clippy::cast_precision_loss)]
        let raw = prior_weight_units[index] as f64 * (CYCLE4_MW_ETA_V1 * fraction).exp();
        if !raw.is_finite() || raw <= 0.0 {
            return Err(arithmetic());
        }
        weights[index] = raw;
        sum += raw;
    }
    if !sum.is_finite() || sum <= 0.0 {
        return Err(arithmetic());
    }
    for weight in &mut weights {
        *weight /= sum;
    }
    for _round in 0..64 {
        let mut changed = false;
        // Cap policies above 25%, redistributing the excess proportionally
        // over the uncapped slots.
        let mut excess = 0.0_f64;
        let mut uncapped_sum = 0.0_f64;
        for weight in &weights {
            if *weight > cap {
                excess += *weight - cap;
            } else {
                uncapped_sum += *weight;
            }
        }
        if excess > 1e-12 {
            if uncapped_sum <= 0.0 {
                return Err(arithmetic());
            }
            for weight in &mut weights {
                if *weight > cap {
                    *weight = cap;
                } else {
                    *weight += excess * (*weight / uncapped_sum);
                }
            }
            changed = true;
        }
        // Raise every deficient two-slot role to exactly 20% in the same
        // step, preserving each role's internal ratio, and rescale the
        // remaining above-floor roles proportionally to the remaining mass.
        // Repairing all deficient roles simultaneously (rather than the
        // first per round) is what makes the loop converge: one-at-a-time
        // repair can push an already-repaired role back below the floor and
        // oscillate past the round budget on feasible inputs.
        let mut role_sums = [0.0_f64; 4];
        for (index, weight) in weights.iter().enumerate() {
            role_sums[index / 2] += *weight;
        }
        let deficient: Vec<usize> = role_sums
            .iter()
            .enumerate()
            .filter(|(_, role)| **role < role_floor - 1e-12)
            .map(|(role_index, _)| role_index)
            .collect();
        if !deficient.is_empty() {
            let deficient_count = deficient.len();
            let surplus_sum: f64 = role_sums
                .iter()
                .enumerate()
                .filter(|(role_index, _)| !deficient.contains(role_index))
                .map(|(_, role)| *role)
                .sum();
            #[allow(clippy::cast_precision_loss)]
            let surplus_target = 1.0 - role_floor * deficient_count as f64;
            if surplus_sum <= 0.0 || surplus_target <= 0.0 {
                return Err(arithmetic());
            }
            let scale_surplus = surplus_target / surplus_sum;
            for (index, weight) in weights.iter_mut().enumerate() {
                let role_index = index / 2;
                if deficient.contains(&role_index) {
                    if role_sums[role_index] <= 0.0 {
                        return Err(arithmetic());
                    }
                    *weight *= role_floor / role_sums[role_index];
                } else {
                    *weight *= scale_surplus;
                }
            }
            changed = true;
        }
        if !changed {
            break;
        }
    }
    // Verify constraints hold after the bounded projection loop.
    let mut role_sums = [0.0_f64; 4];
    for (index, weight) in weights.iter().enumerate() {
        if *weight > cap + 1e-9 {
            return Err(arithmetic());
        }
        role_sums[index / 2] += *weight;
    }
    if role_sums.iter().any(|role| *role < role_floor - 1e-9) {
        return Err(arithmetic());
    }
    // Largest-remainder integerization to exactly one million units.
    #[allow(clippy::cast_precision_loss)]
    let total = CYCLE4_WEIGHT_TOTAL_UNITS_V1 as f64;
    let mut units = [0_u64; CYCLE4_SLOT_COUNT_V1];
    let mut remainders: Vec<(usize, f64)> = Vec::with_capacity(CYCLE4_SLOT_COUNT_V1);
    let mut assigned = 0_u64;
    for (index, weight) in weights.iter().enumerate() {
        let exact = *weight * total;
        if !exact.is_finite() || exact < 0.0 {
            return Err(arithmetic());
        }
        let floor = exact.floor();
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let floor_units = floor as u64;
        units[index] = floor_units;
        assigned = assigned.checked_add(floor_units).ok_or_else(arithmetic)?;
        remainders.push((index, exact - floor));
    }
    let missing = CYCLE4_WEIGHT_TOTAL_UNITS_V1
        .checked_sub(assigned)
        .ok_or_else(arithmetic)?;
    let missing_count = usize::try_from(missing).map_err(|_| arithmetic())?;
    if missing_count > CYCLE4_SLOT_COUNT_V1 {
        return Err(arithmetic());
    }
    remainders.sort_by(|left, right| {
        right
            .1
            .partial_cmp(&left.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(left.0.cmp(&right.0))
    });
    for entry in remainders.iter().take(missing_count) {
        units[entry.0] = units[entry.0].checked_add(1).ok_or_else(arithmetic)?;
    }
    // One-unit role-floor repair after integerization.
    let mut role_units = [0_u64; 4];
    for (index, unit) in units.iter().enumerate() {
        role_units[index / 2] = role_units[index / 2]
            .checked_add(*unit)
            .ok_or_else(arithmetic)?;
    }
    for deficient in 0..4 {
        while role_units[deficient] < CYCLE4_ROLE_FLOOR_UNITS_V1 {
            let deficit_slot =
                2 * deficient + usize::from(units[2 * deficient + 1] < units[2 * deficient]);
            let donor_role = role_units
                .iter()
                .enumerate()
                .filter(|(role_index, role)| {
                    *role_index != deficient && **role > CYCLE4_ROLE_FLOOR_UNITS_V1
                })
                .max_by_key(|(_, role)| **role)
                .map(|(role_index, _)| role_index)
                .ok_or_else(arithmetic)?;
            let donor_slot =
                2 * donor_role + usize::from(units[2 * donor_role + 1] > units[2 * donor_role]);
            if units[donor_slot] <= 1 {
                return Err(arithmetic());
            }
            units[donor_slot] -= 1;
            units[deficit_slot] = units[deficit_slot].checked_add(1).ok_or_else(arithmetic)?;
            role_units[donor_role] -= 1;
            role_units[deficient] += 1;
        }
    }
    let final_sum: u64 = units.iter().sum();
    if final_sum != CYCLE4_WEIGHT_TOTAL_UNITS_V1
        || units
            .iter()
            .any(|unit| *unit > CYCLE4_POLICY_CAP_UNITS_V1 || *unit == 0)
    {
        return Err(arithmetic());
    }
    Ok(units)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_TRAINEE_RUN: &str = CYCLE4_CYCLE3_LINEAGE_RUN_SHA256_V1;
    const TEST_TRAINEE_SEED: u64 = CYCLE4_CYCLE3_LINEAGE_BASE_SEED_V1;

    fn test_hash(tag: u8) -> String {
        format!("ab{:062x}", u64::from(tag))
    }

    fn frozen_slot(
        index: u64,
        role: &str,
        frozen: &FrozenOccupantIdentityCycle4V1,
        weight: u64,
    ) -> Cycle4RefreshSlotV1 {
        Cycle4RefreshSlotV1 {
            slot_index: index,
            role: role.to_owned(),
            occupant_class: if index >= 6 {
                "historical-fallback".to_owned()
            } else {
                "policy".to_owned()
            },
            source_base_seed: frozen.source_base_seed,
            source_run_sha256: frozen.source_run_sha256.to_owned(),
            source_generation: frozen.source_generation,
            checkpoint_manifest_sha256: frozen.checkpoint_manifest_sha256.to_owned(),
            checkpoint_payload_sha256: frozen.checkpoint_payload_sha256.to_owned(),
            model_parameter_sha256: frozen.model_parameter_sha256.to_owned(),
            train_state_sha256: frozen.train_state_sha256.to_owned(),
            weight_units: weight,
        }
    }

    fn trainee_slot(
        index: u64,
        role: &str,
        run: &str,
        seed: u64,
        generation: u64,
        hash_tag: u8,
        weight: u64,
    ) -> Cycle4RefreshSlotV1 {
        Cycle4RefreshSlotV1 {
            slot_index: index,
            role: role.to_owned(),
            occupant_class: "policy".to_owned(),
            source_base_seed: seed,
            source_run_sha256: run.to_owned(),
            source_generation: generation,
            checkpoint_manifest_sha256: test_hash(hash_tag),
            checkpoint_payload_sha256: test_hash(hash_tag + 1),
            model_parameter_sha256: test_hash(hash_tag + 2),
            train_state_sha256: test_hash(hash_tag + 3),
            weight_units: weight,
        }
    }

    fn slots_for(refresh_index: u64, weight: u64) -> Vec<Cycle4RefreshSlotV1> {
        let local = CYCLE4_TRAINEE_START_LOCAL_GENERATION_V1 + refresh_index * 128;
        let rotation = usize::try_from(refresh_index % 3).expect("rotation");
        vec![
            frozen_slot(0, "anchor-0", &CYCLE4_ANCHOR_0_V1, weight),
            frozen_slot(1, "anchor-1", &CYCLE4_ANCHOR_1_V1, weight),
            trainee_slot(
                2,
                "historical-0",
                if refresh_index <= 3 {
                    CYCLE4_CYCLE3_LINEAGE_RUN_SHA256_V1
                } else {
                    TEST_TRAINEE_RUN
                },
                if refresh_index <= 3 {
                    CYCLE4_CYCLE3_LINEAGE_BASE_SEED_V1
                } else {
                    TEST_TRAINEE_SEED
                },
                local - CYCLE4_HISTORICAL_LAG_V1,
                16,
                weight,
            ),
            frozen_slot(
                3,
                "historical-1",
                &CYCLE4_HISTORICAL_1_ROTATION_V1[rotation],
                weight,
            ),
            frozen_slot(4, "current-0", &CYCLE4_CURRENT_0_V1, weight),
            trainee_slot(
                5,
                "current-1",
                TEST_TRAINEE_RUN,
                TEST_TRAINEE_SEED,
                local,
                32,
                weight,
            ),
            frozen_slot(6, "exploiter-0", &CYCLE4_EXPLOITER_0_V1, weight),
            frozen_slot(7, "exploiter-1", &CYCLE4_EXPLOITER_1_V1, weight),
        ]
    }

    fn genesis() -> Cycle4RefreshManifestV1 {
        build_cycle4_refresh_manifest_v1(
            0,
            None,
            None,
            TEST_TRAINEE_RUN,
            TEST_TRAINEE_SEED,
            slots_for(0, CYCLE4_GENESIS_SLOT_WEIGHT_UNITS_V1),
        )
        .expect("genesis manifest")
    }

    #[test]
    fn genesis_builds_and_round_trips_v1() {
        let manifest = genesis();
        let decoded = decode_cycle4_refresh_manifest_v1(manifest.canonical_bytes_v1(), None, None)
            .expect("decode");
        assert_eq!(decoded.manifest_sha256_v1(), manifest.manifest_sha256_v1());
        assert_eq!(decoded.refresh_index_v1(), 0);
        assert_eq!(decoded.trainee_local_generation_v1(), 896);
    }

    #[test]
    fn refresh_one_requires_and_binds_panel_bytes_v1() {
        let genesis = genesis();
        let panel = b"cycle4-test-panel-bytes-v1".to_vec();
        let manifest = build_cycle4_refresh_manifest_v1(
            1,
            Some(&genesis),
            Some(&panel),
            TEST_TRAINEE_RUN,
            TEST_TRAINEE_SEED,
            slots_for(1, CYCLE4_GENESIS_SLOT_WEIGHT_UNITS_V1),
        )
        .expect("refresh 1");
        // Decoding without the panel bytes must fail closed.
        let missing =
            decode_cycle4_refresh_manifest_v1(manifest.canonical_bytes_v1(), Some(&genesis), None)
                .expect_err("missing panel");
        assert_eq!(
            missing.kind_v1(),
            Cycle4RefreshManifestErrorKindV1::MissingPanelBytes
        );
        // Decoding with different bytes must fail closed.
        let mismatch = decode_cycle4_refresh_manifest_v1(
            manifest.canonical_bytes_v1(),
            Some(&genesis),
            Some(b"different-bytes"),
        )
        .expect_err("panel mismatch");
        assert_eq!(
            mismatch.kind_v1(),
            Cycle4RefreshManifestErrorKindV1::PanelContentMismatch
        );
        // Exact bytes decode.
        let decoded = decode_cycle4_refresh_manifest_v1(
            manifest.canonical_bytes_v1(),
            Some(&genesis),
            Some(&panel),
        )
        .expect("decode with panel");
        assert_eq!(decoded.refresh_index_v1(), 1);
    }

    #[test]
    fn placeholder_panel_hash_is_rejected_v1() {
        assert!(is_placeholder_hash_cycle4_v1(
            "00000000000000000000000000000000000000000000000000000000000007e3"
        ));
        assert!(!is_placeholder_hash_cycle4_v1(
            "4b000f7cba9ebac27af058de18c75adc557d6d9cda8cf54bca8f51171695a40c"
        ));
        // A manifest carrying a placeholder hash fails before content checks.
        let genesis = genesis();
        let panel = b"panel".to_vec();
        let manifest = build_cycle4_refresh_manifest_v1(
            1,
            Some(&genesis),
            Some(&panel),
            TEST_TRAINEE_RUN,
            TEST_TRAINEE_SEED,
            slots_for(1, CYCLE4_GENESIS_SLOT_WEIGHT_UNITS_V1),
        )
        .expect("refresh 1");
        let mut text = String::from_utf8(manifest.canonical_bytes_v1().to_vec()).expect("utf8");
        let real = lower_hex_raw32_v1(sha256_v1(&panel));
        text = text.replace(
            &real,
            "00000000000000000000000000000000000000000000000000000000000007e3",
        );
        let error =
            decode_cycle4_refresh_manifest_v1(text.as_bytes(), Some(&genesis), Some(&panel))
                .expect_err("placeholder");
        assert_eq!(
            error.kind_v1(),
            Cycle4RefreshManifestErrorKindV1::PlaceholderPanelHash
        );
    }

    #[test]
    fn wrong_rotation_identity_is_rejected_v1() {
        let genesis = genesis();
        let panel = b"panel".to_vec();
        let mut slots = slots_for(1, CYCLE4_GENESIS_SLOT_WEIGHT_UNITS_V1);
        // Refresh 1 expects rotation index 1 (seed 970002); install 970003.
        slots[3] = frozen_slot(
            3,
            "historical-1",
            &CYCLE4_HISTORICAL_1_ROTATION_V1[2],
            CYCLE4_GENESIS_SLOT_WEIGHT_UNITS_V1,
        );
        let error = build_cycle4_refresh_manifest_v1(
            1,
            Some(&genesis),
            Some(&panel),
            TEST_TRAINEE_RUN,
            TEST_TRAINEE_SEED,
            slots,
        )
        .expect_err("wrong rotation");
        assert_eq!(
            error.kind_v1(),
            Cycle4RefreshManifestErrorKindV1::InvalidSlots
        );
    }

    #[test]
    fn historical_zero_switches_source_at_index_four_v1() {
        // Index <= 3 must bind the cycle-3 lineage; index >= 4 the arm run.
        let mut previous = genesis();
        let panel = b"panel".to_vec();
        for index in 1..=4_u64 {
            let manifest = build_cycle4_refresh_manifest_v1(
                index,
                Some(&previous),
                Some(&panel),
                TEST_TRAINEE_RUN,
                TEST_TRAINEE_SEED,
                slots_for(index, CYCLE4_GENESIS_SLOT_WEIGHT_UNITS_V1),
            )
            .expect("chain");
            previous = manifest;
        }
        assert_eq!(previous.refresh_index_v1(), 4);
        assert_eq!(
            previous.slots_v1()[2].source_generation,
            896 + 4 * 128 - 512
        );
    }

    #[test]
    fn duplicate_model_hash_is_rejected_v1() {
        let mut slots = slots_for(0, CYCLE4_GENESIS_SLOT_WEIGHT_UNITS_V1);
        slots[5].model_parameter_sha256 = slots[2].model_parameter_sha256.clone();
        let error = build_cycle4_refresh_manifest_v1(
            0,
            None,
            None,
            TEST_TRAINEE_RUN,
            TEST_TRAINEE_SEED,
            slots,
        )
        .expect_err("duplicate model hash");
        assert_eq!(
            error.kind_v1(),
            Cycle4RefreshManifestErrorKindV1::InvalidSlots
        );
    }

    #[test]
    fn genesis_weights_must_be_uniform_v1() {
        let mut slots = slots_for(0, CYCLE4_GENESIS_SLOT_WEIGHT_UNITS_V1);
        slots[0].weight_units = 130_000;
        slots[1].weight_units = 120_000;
        let error = build_cycle4_refresh_manifest_v1(
            0,
            None,
            None,
            TEST_TRAINEE_RUN,
            TEST_TRAINEE_SEED,
            slots,
        )
        .expect_err("nonuniform genesis");
        assert_eq!(
            error.kind_v1(),
            Cycle4RefreshManifestErrorKindV1::InvalidWeight
        );
    }

    #[test]
    fn chain_rejects_trainee_binding_drift_v1() {
        let genesis = genesis();
        let panel = b"panel".to_vec();
        let error = build_cycle4_refresh_manifest_v1(
            1,
            Some(&genesis),
            Some(&panel),
            &test_hash(200),
            TEST_TRAINEE_SEED,
            slots_for(1, CYCLE4_GENESIS_SLOT_WEIGHT_UNITS_V1),
        )
        .expect_err("run drift");
        assert_eq!(
            error.kind_v1(),
            Cycle4RefreshManifestErrorKindV1::InvalidChain
        );
    }

    #[test]
    fn mw_update_zero_scores_preserve_uniform_weights_v1() {
        let prior = [CYCLE4_GENESIS_SLOT_WEIGHT_UNITS_V1; 8];
        let scores = [0.0_f64; 8];
        let updated = mw_update_cycle4_v1(&prior, &scores).expect("mw");
        assert_eq!(updated, prior);
    }

    #[test]
    fn mw_update_moves_mass_toward_winners_within_constraints_v1() {
        let prior = [CYCLE4_GENESIS_SLOT_WEIGHT_UNITS_V1; 8];
        let scores = [0.5, -0.5, 0.25, -0.25, 0.0, 0.0, 1.0, -1.0];
        let updated = mw_update_cycle4_v1(&prior, &scores).expect("mw");
        let total: u64 = updated.iter().sum();
        assert_eq!(total, CYCLE4_WEIGHT_TOTAL_UNITS_V1);
        assert!(updated[0] > updated[1]);
        assert!(updated[2] > updated[3]);
        assert!(updated[6] > updated[7]);
        for pair in 0..4 {
            assert!(updated[2 * pair] + updated[2 * pair + 1] >= CYCLE4_ROLE_FLOOR_UNITS_V1);
        }
        for unit in updated {
            assert!(unit > 0 && unit <= CYCLE4_POLICY_CAP_UNITS_V1);
        }
    }

    #[test]
    fn panel_score_fraction_bounds_v1() {
        assert!(panel_score_fraction_cycle4_v1(1_793).is_err());
        assert!(panel_score_fraction_cycle4_v1(-1_793).is_err());
        // Regression: i64::MIN must fail closed, not overflow in `abs()`.
        assert!(panel_score_fraction_cycle4_v1(i64::MIN).is_err());
        let fraction = panel_score_fraction_cycle4_v1(896).expect("fraction");
        assert!((fraction - 0.5).abs() < 1e-12);
    }

    #[test]
    fn mw_projection_handles_multiple_deficient_roles_v1() {
        // Regression for the reviewer counterexample: one-at-a-time role
        // repair oscillated past the round budget on this feasible input.
        let prior = [
            231_510, 164_349, 14_693, 186_850, 100_524, 99_553, 148_416, 54_105,
        ];
        let scores = [
            1.0 / 7.0,
            3.0 / 7.0,
            -1.0 / 7.0,
            -1.0 / 7.0,
            -1.0 / 7.0,
            1.0 / 7.0,
            1.0 / 7.0,
            -3.0 / 7.0,
        ];
        let updated = mw_update_cycle4_v1(&prior, &scores).expect("feasible projection");
        let total: u64 = updated.iter().sum();
        assert_eq!(total, CYCLE4_WEIGHT_TOTAL_UNITS_V1);
        for pair in 0..4 {
            assert!(updated[2 * pair] + updated[2 * pair + 1] >= CYCLE4_ROLE_FLOOR_UNITS_V1);
        }
        for unit in updated {
            assert!(unit > 0 && unit <= CYCLE4_POLICY_CAP_UNITS_V1);
        }
    }

    #[test]
    fn fallback_slots_require_fallback_occupant_class_v1() {
        let mut slots = slots_for(0, CYCLE4_GENESIS_SLOT_WEIGHT_UNITS_V1);
        slots[6].occupant_class = "policy".to_owned();
        let error = build_cycle4_refresh_manifest_v1(
            0,
            None,
            None,
            TEST_TRAINEE_RUN,
            TEST_TRAINEE_SEED,
            slots,
        )
        .expect_err("fallback class");
        assert_eq!(
            error.kind_v1(),
            Cycle4RefreshManifestErrorKindV1::InvalidSlots
        );
        let mut slots = slots_for(0, CYCLE4_GENESIS_SLOT_WEIGHT_UNITS_V1);
        slots[0].occupant_class = "historical-fallback".to_owned();
        let error = build_cycle4_refresh_manifest_v1(
            0,
            None,
            None,
            TEST_TRAINEE_RUN,
            TEST_TRAINEE_SEED,
            slots,
        )
        .expect_err("policy class");
        assert_eq!(
            error.kind_v1(),
            Cycle4RefreshManifestErrorKindV1::InvalidSlots
        );
    }
}
