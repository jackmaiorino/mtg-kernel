//! Weight ancestry with a separate, truthful origin for sampled initialization.
//!
//! Imported variants serialize through the original DTO unchanged. Decoding
//! reads fields directly through MapAccess, so neither variant selection nor a
//! nested imported observation receipt passes through a lossy JSON Value probe.

use super::{FrozenPlayObservationReceiptV3, FrozenPlayPolicyIdentityV1};
use serde::de::{self, IgnoredAny, MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

pub const FRESH_PLAY_INITIALIZATION_SCHEMA_V1: &str = "mtg-kernel-fresh-play-initialization/v1";
pub const TRANSFERRED_FRESH_PLAY_SCHEMA_V1: &str = "mtg-kernel-fresh-play-registry-transfer/v1";
/// `FrozenPlayPolicyIdentityV1::schema` for an `Imported` origin that
/// `FrozenPlayPolicyV1::from_registry_transfer_v1` produced: a frozen V3
/// checkpoint whose card registry was append-transferred to the current
/// build's registry, still scored through the V3 encoder. Single constant so
/// every reader (the trainer's cross-generation opponent admission, the
/// Wildfire/appended-card deck guard) matches the one writer exactly.
pub const REGISTRY_TRANSFERRED_PLAY_SCHEMA_V1: &str = "mtg-kernel-registry-transferred-play/v1";
const INITIALIZER: &str = "trainer-seeded-v1";
const SEED_DERIVATION: &str = "kernel-python-rl-trainer-sha256-v2";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FreshPlayPolicyIdentityV1 {
    pub schema: String,
    pub initialization_manifest_sha256: String,
    pub lineage_id: String,
    pub initializer: String,
    pub base_seed: u64,
    pub model_init_seed: u64,
    pub seed_derivation: String,
    pub producer_git_commit: String,
    pub initial_weights_sha256: String,
    pub initial_model_parameter_sha256: String,
    pub parameter_layout_sha256: String,
    pub destination_registry_sha256: String,
    pub destination_card_db_hash: String,
    pub destination_card_count: usize,
    pub feature_contract_digest: String,
    pub feature_encoding_digest: String,
    pub features_source_sha256: String,
    pub feature_descriptor_sha256: String,
    pub sampler_identity: String,
}

impl FreshPlayPolicyIdentityV1 {
    /// Metadata validation only. The native constructor also binds this origin
    /// to actual installed parameter bytes and the current runtime registry.
    pub fn validate_v1(&self) -> Result<(), String> {
        let hex = |value: &str, len| {
            value.len() == len
                && value
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        };
        if self.schema != FRESH_PLAY_INITIALIZATION_SCHEMA_V1
            || self.initializer != INITIALIZER
            || self.seed_derivation != SEED_DERIVATION
            || self.lineage_id.is_empty()
            || self.lineage_id.len() > 128
            || !self.lineage_id.bytes().all(|b| b.is_ascii_graphic())
            || self.base_seed > i64::MAX as u64
            || self.model_init_seed > i64::MAX as u64
            || self.model_init_seed != derived_model_seed(self.base_seed)
            || !hex(&self.producer_git_commit, 40)
            || !hex(&self.destination_card_db_hash, 16)
            || self.destination_card_count == 0
            || self.destination_card_count > 65_536
            || self.sampler_identity != crate::fast_sampler::WIDE_CATEGORICAL_SAMPLER_VERSION_V1
        {
            return Err("invalid fresh initialization origin metadata".into());
        }
        for value in [
            &self.initialization_manifest_sha256,
            &self.initial_weights_sha256,
            &self.initial_model_parameter_sha256,
            &self.parameter_layout_sha256,
            &self.destination_registry_sha256,
            &self.feature_contract_digest,
            &self.feature_encoding_digest,
            &self.features_source_sha256,
            &self.feature_descriptor_sha256,
        ] {
            if !hex(value, 64) {
                return Err("invalid fresh initialization origin digest".into());
            }
        }
        Ok(())
    }
}

/// Nests the entire original fresh identity unchanged (byte-for-byte
/// ancestry, including its own now-stale destination fields) and stores the
/// current post-transfer destination identity separately. `original.*` never
/// gets rewritten to claim initialization on the expanded registry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TransferredFreshPlayPolicyIdentityV1 {
    pub schema: String,
    pub original: FreshPlayPolicyIdentityV1,
    pub destination_registry_sha256: String,
    pub destination_card_db_hash: String,
    pub destination_card_count: usize,
    pub transfer_envelope_sha256: String,
}

impl TransferredFreshPlayPolicyIdentityV1 {
    /// Metadata validation only, mirroring `FreshPlayPolicyIdentityV1::validate_v1`
    /// for this wrapper's own new fields, plus delegation into the nested
    /// original identity's own validation.
    pub fn validate_v1(&self) -> Result<(), String> {
        let hex = |value: &str, len| {
            value.len() == len
                && value
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        };
        if self.schema != TRANSFERRED_FRESH_PLAY_SCHEMA_V1
            || !hex(&self.destination_registry_sha256, 64)
            || !hex(&self.transfer_envelope_sha256, 64)
            || !hex(&self.destination_card_db_hash, 16)
            || self.destination_card_count == 0
            || self.destination_card_count > 65_536
        {
            return Err("invalid fresh registry transfer origin metadata".into());
        }
        self.original.validate_v1()
    }
}

fn derived_model_seed(base_seed: u64) -> u64 {
    fn atom(hasher: &mut Sha256, tag: &str, payload: &[u8]) {
        hasher.update((tag.len() as u32).to_be_bytes());
        hasher.update(tag.as_bytes());
        hasher.update((payload.len() as u64).to_be_bytes());
        hasher.update(payload);
    }
    let mut hasher = Sha256::new();
    atom(&mut hasher, "version", SEED_DERIVATION.as_bytes());
    atom(&mut hasher, "namespace", b"model-init");
    atom(&mut hasher, "field-name", b"base_seed");
    atom(&mut hasher, "u63", &base_seed.to_be_bytes());
    let digest = hasher.finalize();
    u64::from_be_bytes(digest[..8].try_into().expect("SHA256 has eight bytes")) & i64::MAX as u64
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlayPolicyOriginV1 {
    Imported(FrozenPlayPolicyIdentityV1),
    FreshInitialization(FreshPlayPolicyIdentityV1),
    TransferredFreshInitialization(TransferredFreshPlayPolicyIdentityV1),
}

impl From<FrozenPlayPolicyIdentityV1> for PlayPolicyOriginV1 {
    fn from(value: FrozenPlayPolicyIdentityV1) -> Self {
        Self::Imported(value)
    }
}

impl PlayPolicyOriginV1 {
    pub fn fresh_initialization_v1(value: FreshPlayPolicyIdentityV1) -> Result<Self, String> {
        value.validate_v1()?;
        Ok(Self::FreshInitialization(value))
    }
    pub fn transferred_fresh_v1(value: TransferredFreshPlayPolicyIdentityV1) -> Result<Self, String> {
        value.validate_v1()?;
        Ok(Self::TransferredFreshInitialization(value))
    }
    pub fn as_imported_v1(&self) -> Option<&FrozenPlayPolicyIdentityV1> {
        match self {
            Self::Imported(value) => Some(value),
            Self::FreshInitialization(_) | Self::TransferredFreshInitialization(_) => None,
        }
    }
    /// True only for the one `Imported` sub-case
    /// `FrozenPlayPolicyV1::from_registry_transfer_v1` writes
    /// (`schema == REGISTRY_TRANSFERRED_PLAY_SCHEMA_V1`), never for an
    /// ordinary imported checkpoint or either fresh origin. The sole reader
    /// today is the expanded trainer's narrow cross-generation opponent
    /// admission (a V4 learner may pair only with this exact origin, never
    /// any other V3 source) and the deck-eligibility guard beside it.
    pub fn is_registry_transferred_v1(&self) -> bool {
        self.as_imported_v1()
            .is_some_and(|imported| imported.schema == REGISTRY_TRANSFERRED_PLAY_SCHEMA_V1)
    }
    /// True for both fresh origins: plain fresh initialization and a fresh
    /// origin that has since undergone a registry transfer. Both stay on the
    /// wide/V3 sampling and scoring path and use the fresh-family schemas.
    pub fn is_fresh_v1(&self) -> bool {
        matches!(
            self,
            Self::FreshInitialization(_) | Self::TransferredFreshInitialization(_)
        )
    }
    pub fn destination_registry_sha256_v1(&self) -> &str {
        match self {
            Self::Imported(v) => &v.destination_registry_sha256,
            Self::FreshInitialization(v) => &v.destination_registry_sha256,
            Self::TransferredFreshInitialization(v) => &v.destination_registry_sha256,
        }
    }
    pub fn destination_card_db_hash_v1(&self) -> &str {
        match self {
            Self::Imported(v) => &v.destination_card_db_hash,
            Self::FreshInitialization(v) => &v.destination_card_db_hash,
            Self::TransferredFreshInitialization(v) => &v.destination_card_db_hash,
        }
    }
    pub fn destination_card_count_v1(&self) -> usize {
        match self {
            Self::Imported(v) => v.destination_card_count,
            Self::FreshInitialization(v) => v.destination_card_count,
            Self::TransferredFreshInitialization(v) => v.destination_card_count,
        }
    }
    /// A registry transfer requires the source's feature identity to already
    /// equal the current runtime's (feature migration is unsupported), so the
    /// nested original's digests already describe the current runtime too.
    pub fn feature_contract_digest_v1(&self) -> &str {
        match self {
            Self::Imported(v) => &v.feature_contract_digest,
            Self::FreshInitialization(v) => &v.feature_contract_digest,
            Self::TransferredFreshInitialization(v) => &v.original.feature_contract_digest,
        }
    }
    pub fn feature_encoding_digest_v1(&self) -> &str {
        match self {
            Self::Imported(v) => &v.feature_encoding_digest,
            Self::FreshInitialization(v) => &v.feature_encoding_digest,
            Self::TransferredFreshInitialization(v) => &v.original.feature_encoding_digest,
        }
    }
    pub fn initial_weights_sha256_v1(&self) -> &str {
        match self {
            Self::Imported(v) => &v.weights_sha256,
            Self::FreshInitialization(v) => &v.initial_weights_sha256,
            Self::TransferredFreshInitialization(v) => &v.original.initial_weights_sha256,
        }
    }
    pub fn initial_model_parameter_sha256_v1(&self) -> &str {
        match self {
            Self::Imported(v) => &v.model_parameter_sha256,
            Self::FreshInitialization(v) => &v.initial_model_parameter_sha256,
            Self::TransferredFreshInitialization(v) => &v.original.initial_model_parameter_sha256,
        }
    }
    pub fn origin_git_commit_v1(&self) -> &str {
        match self {
            Self::Imported(v) => &v.source_git_commit,
            Self::FreshInitialization(v) => &v.producer_git_commit,
            Self::TransferredFreshInitialization(v) => &v.original.producer_git_commit,
        }
    }
}

impl Serialize for PlayPolicyOriginV1 {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Imported(value) => value.serialize(serializer),
            Self::FreshInitialization(value) => value.serialize(serializer),
            Self::TransferredFreshInitialization(value) => value.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for PlayPolicyOriginV1 {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OriginVisitor;
        impl<'de> Visitor<'de> for OriginVisitor {
            type Value = PlayPolicyOriginV1;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("an imported identity or explicit fresh initialization origin")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
                let mut strings: BTreeMap<String, String> = BTreeMap::new();
                let mut integers: BTreeMap<String, u64> = BTreeMap::new();
                let mut counts: BTreeMap<String, usize> = BTreeMap::new();
                let mut seen = BTreeSet::new();
                let mut decks: Option<Vec<String>> = None;
                let mut revalidated: Option<bool> = None;
                let mut observation: Option<FrozenPlayObservationReceiptV3> = None;
                let mut original: Option<FreshPlayPolicyIdentityV1> = None;
                while let Some(key) = map.next_key::<String>()? {
                    if !seen.insert(key.clone()) {
                        return Err(de::Error::custom(format!("duplicate origin field: {key}")));
                    }
                    match key.as_str() {
                        "schema"
                        | "source_export_schema"
                        | "source_metadata_sha256"
                        | "weights_sha256"
                        | "model_parameter_sha256"
                        | "source_run_sha256"
                        | "source_git_commit"
                        | "source_card_db_hash"
                        | "destination_card_db_hash"
                        | "source_registry_sha256"
                        | "destination_registry_sha256"
                        | "namespace_rule"
                        | "appended_rows"
                        | "feature_contract_digest"
                        | "feature_encoding_digest"
                        | "sampler_identity"
                        | "initialization_manifest_sha256"
                        | "lineage_id"
                        | "initializer"
                        | "seed_derivation"
                        | "producer_git_commit"
                        | "initial_weights_sha256"
                        | "initial_model_parameter_sha256"
                        | "parameter_layout_sha256"
                        | "features_source_sha256"
                        | "feature_descriptor_sha256"
                        | "transfer_envelope_sha256" => {
                            strings.insert(key, map.next_value()?);
                        }
                        "source_generation" | "base_seed" | "model_init_seed" => {
                            integers.insert(key, map.next_value()?);
                        }
                        "source_card_count" | "destination_card_count" => {
                            counts.insert(key, map.next_value()?);
                        }
                        "source_training_deck_ids" => {
                            decks = Some(map.next_value()?);
                        }
                        "reader_revalidated_store_chain" => {
                            revalidated = Some(map.next_value()?);
                        }
                        "observation_successor" => {
                            observation = map.next_value()?;
                        }
                        "original" => {
                            original = Some(map.next_value()?);
                        }
                        _ => {
                            let _: IgnoredAny = map.next_value()?;
                        }
                    }
                }
                macro_rules! take {
                    ($fields:ident, $key:literal) => {
                        $fields
                            .remove($key)
                            .ok_or_else(|| <M::Error as de::Error>::missing_field($key))?
                    };
                }
                let schema = take!(strings, "schema");
                if schema == FRESH_PLAY_INITIALIZATION_SCHEMA_V1 {
                    const FRESH_FIELDS: &[&str] = &[
                        "schema",
                        "initialization_manifest_sha256",
                        "lineage_id",
                        "initializer",
                        "base_seed",
                        "model_init_seed",
                        "seed_derivation",
                        "producer_git_commit",
                        "initial_weights_sha256",
                        "initial_model_parameter_sha256",
                        "parameter_layout_sha256",
                        "destination_registry_sha256",
                        "destination_card_db_hash",
                        "destination_card_count",
                        "feature_contract_digest",
                        "feature_encoding_digest",
                        "features_source_sha256",
                        "feature_descriptor_sha256",
                        "sampler_identity",
                    ];
                    if let Some(field) = seen
                        .iter()
                        .find(|field| !FRESH_FIELDS.contains(&field.as_str()))
                    {
                        return Err(de::Error::custom(format!(
                            "unknown fresh origin field: {field}"
                        )));
                    }
                    let fresh = FreshPlayPolicyIdentityV1 {
                        schema,
                        initialization_manifest_sha256: take!(
                            strings,
                            "initialization_manifest_sha256"
                        ),
                        lineage_id: take!(strings, "lineage_id"),
                        initializer: take!(strings, "initializer"),
                        base_seed: take!(integers, "base_seed"),
                        model_init_seed: take!(integers, "model_init_seed"),
                        seed_derivation: take!(strings, "seed_derivation"),
                        producer_git_commit: take!(strings, "producer_git_commit"),
                        initial_weights_sha256: take!(strings, "initial_weights_sha256"),
                        initial_model_parameter_sha256: take!(
                            strings,
                            "initial_model_parameter_sha256"
                        ),
                        parameter_layout_sha256: take!(strings, "parameter_layout_sha256"),
                        destination_registry_sha256: take!(strings, "destination_registry_sha256"),
                        destination_card_db_hash: take!(strings, "destination_card_db_hash"),
                        destination_card_count: take!(counts, "destination_card_count"),
                        feature_contract_digest: take!(strings, "feature_contract_digest"),
                        feature_encoding_digest: take!(strings, "feature_encoding_digest"),
                        features_source_sha256: take!(strings, "features_source_sha256"),
                        feature_descriptor_sha256: take!(strings, "feature_descriptor_sha256"),
                        sampler_identity: take!(strings, "sampler_identity"),
                    };
                    return PlayPolicyOriginV1::fresh_initialization_v1(fresh)
                        .map_err(de::Error::custom);
                }
                if schema == TRANSFERRED_FRESH_PLAY_SCHEMA_V1 {
                    const TRANSFERRED_FRESH_FIELDS: &[&str] = &[
                        "schema",
                        "original",
                        "destination_registry_sha256",
                        "destination_card_db_hash",
                        "destination_card_count",
                        "transfer_envelope_sha256",
                    ];
                    if let Some(field) = seen
                        .iter()
                        .find(|field| !TRANSFERRED_FRESH_FIELDS.contains(&field.as_str()))
                    {
                        return Err(de::Error::custom(format!(
                            "unknown fresh registry transfer origin field: {field}"
                        )));
                    }
                    let wrapper = TransferredFreshPlayPolicyIdentityV1 {
                        schema,
                        original: original.ok_or_else(|| {
                            <M::Error as de::Error>::missing_field("original")
                        })?,
                        destination_registry_sha256: take!(strings, "destination_registry_sha256"),
                        destination_card_db_hash: take!(strings, "destination_card_db_hash"),
                        destination_card_count: take!(counts, "destination_card_count"),
                        transfer_envelope_sha256: take!(strings, "transfer_envelope_sha256"),
                    };
                    return PlayPolicyOriginV1::transferred_fresh_v1(wrapper)
                        .map_err(de::Error::custom);
                }
                // The old DTO permitted unknown fields; retain that imported
                // behavior, but never let fresh-only fields masquerade as an
                // imported origin when a schema is changed or misspelled.
                for field in [
                    "initialization_manifest_sha256",
                    "lineage_id",
                    "initializer",
                    "base_seed",
                    "model_init_seed",
                    "seed_derivation",
                    "producer_git_commit",
                    "initial_weights_sha256",
                    "initial_model_parameter_sha256",
                    "parameter_layout_sha256",
                    "features_source_sha256",
                    "feature_descriptor_sha256",
                    "original",
                    "transfer_envelope_sha256",
                ] {
                    if seen.contains(field) {
                        return Err(de::Error::custom(
                            "fresh origin fields require the explicit fresh schema",
                        ));
                    }
                }
                Ok(PlayPolicyOriginV1::Imported(FrozenPlayPolicyIdentityV1 {
                    schema,
                    source_export_schema: take!(strings, "source_export_schema"),
                    source_metadata_sha256: take!(strings, "source_metadata_sha256"),
                    weights_sha256: take!(strings, "weights_sha256"),
                    model_parameter_sha256: take!(strings, "model_parameter_sha256"),
                    source_run_sha256: take!(strings, "source_run_sha256"),
                    source_generation: take!(integers, "source_generation"),
                    source_git_commit: take!(strings, "source_git_commit"),
                    source_card_db_hash: take!(strings, "source_card_db_hash"),
                    destination_card_db_hash: take!(strings, "destination_card_db_hash"),
                    source_registry_sha256: take!(strings, "source_registry_sha256"),
                    destination_registry_sha256: take!(strings, "destination_registry_sha256"),
                    source_card_count: take!(counts, "source_card_count"),
                    destination_card_count: take!(counts, "destination_card_count"),
                    source_training_deck_ids: decks.ok_or_else(|| {
                        <M::Error as de::Error>::missing_field("source_training_deck_ids")
                    })?,
                    namespace_rule: take!(strings, "namespace_rule"),
                    appended_rows: take!(strings, "appended_rows"),
                    feature_contract_digest: take!(strings, "feature_contract_digest"),
                    feature_encoding_digest: take!(strings, "feature_encoding_digest"),
                    sampler_identity: take!(strings, "sampler_identity"),
                    reader_revalidated_store_chain: revalidated.ok_or_else(|| {
                        <M::Error as de::Error>::missing_field("reader_revalidated_store_chain")
                    })?,
                    observation_successor: observation,
                }))
            }
        }
        deserializer.deserialize_map(OriginVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imported_origin_preserves_bytes_and_rejects_top_level_and_nested_duplicates() {
        let policy = crate::sideboard_play_policy_v1::FrozenPlayPolicyV1::training_fixture_v3();
        let mut imported = policy.identity_v1().as_imported_v1().unwrap().clone();
        imported.observation_successor = Some(FrozenPlayObservationReceiptV3 {
            schema: "test-only-observation-receipt".into(),
            source_feature_contract_digest: "contract".into(),
            source_feature_encoding_digest: "encoding".into(),
            destination: crate::sideboard_play_policy_v1::FrozenPlayObservationTransferV3 {
                expected_feature_contract_digest: "destination-contract".into(),
                expected_feature_encoding_digest: "destination-encoding".into(),
            },
            features_source_sha256: "a".repeat(64),
            feature_descriptor_sha256: "b".repeat(64),
            semantics: "serialization fixture only".into(),
        });
        let old = serde_json::to_string(&imported).unwrap();
        let origin = PlayPolicyOriginV1::from(imported);
        assert_eq!(serde_json::to_string(&origin).unwrap(), old);
        let decoded: PlayPolicyOriginV1 = serde_json::from_str(&old).unwrap();
        assert_eq!(decoded, origin);
        assert_eq!(serde_json::to_string(&decoded).unwrap(), old);
        assert!(!decoded.is_fresh_v1());
        let top_duplicate = old.replacen("{", "{\"schema\":\"duplicate\",", 1);
        assert!(serde_json::from_str::<PlayPolicyOriginV1>(&top_duplicate).is_err());
        let nested_duplicate = old.replacen(
            "\"source_feature_contract_digest\":",
            "\"source_feature_contract_digest\":\"duplicate\",\"source_feature_contract_digest\":",
            1,
        );
        assert_ne!(nested_duplicate, old);
        assert!(serde_json::from_str::<PlayPolicyOriginV1>(&nested_duplicate).is_err());
    }

    #[test]
    fn fresh_origin_has_explicit_strict_schema_and_exact_trainer_seed_derivation() {
        // Metadata-only synthetic fixture. This does not attest to a generated
        // model, producer execution, runtime compatibility or playing strength.
        let fresh = FreshPlayPolicyIdentityV1 {
            schema: FRESH_PLAY_INITIALIZATION_SCHEMA_V1.into(),
            initialization_manifest_sha256: "a".repeat(64),
            lineage_id: "synthetic-origin-test".into(),
            initializer: INITIALIZER.into(),
            base_seed: 0,
            model_init_seed: 6_443_515_232_517_447_393,
            seed_derivation: SEED_DERIVATION.into(),
            producer_git_commit: "1".repeat(40),
            initial_weights_sha256: "b".repeat(64),
            initial_model_parameter_sha256: "c".repeat(64),
            parameter_layout_sha256: "d".repeat(64),
            destination_registry_sha256: "e".repeat(64),
            destination_card_db_hash: "f".repeat(16),
            destination_card_count: 184,
            feature_contract_digest: "0".repeat(64),
            feature_encoding_digest: "1".repeat(64),
            features_source_sha256: "2".repeat(64),
            feature_descriptor_sha256: "3".repeat(64),
            sampler_identity: crate::fast_sampler::WIDE_CATEGORICAL_SAMPLER_VERSION_V1.into(),
        };
        assert_eq!(derived_model_seed(0), fresh.model_init_seed);
        let origin = PlayPolicyOriginV1::fresh_initialization_v1(fresh).unwrap();
        let raw = serde_json::to_string(&origin).unwrap();
        assert_eq!(
            serde_json::from_str::<PlayPolicyOriginV1>(&raw).unwrap(),
            origin
        );
        assert!(origin.is_fresh_v1());
        assert!(origin.as_imported_v1().is_none());
        assert_eq!(origin.origin_git_commit_v1(), "1".repeat(40));
        for changed in [
            raw.replacen("{", "{\"unexpected\":true,", 1),
            raw.replacen("{", "{\"source_generation\":0,", 1),
            raw.replacen("{", "{\"base_seed\":0,", 1),
            raw.replace(
                FRESH_PLAY_INITIALIZATION_SCHEMA_V1,
                "misspelled-fresh-schema",
            ),
            raw.replace("\"base_seed\":0", "\"base_seed\":true"),
            raw.replace("6443515232517447393", "6443515232517447394"),
        ] {
            assert_ne!(changed, raw);
            assert!(serde_json::from_str::<PlayPolicyOriginV1>(&changed).is_err());
        }
        assert!(serde_json::from_str::<FrozenPlayPolicyIdentityV1>(&raw).is_err());
    }

    fn synthetic_fresh(destination_card_count: usize) -> FreshPlayPolicyIdentityV1 {
        FreshPlayPolicyIdentityV1 {
            schema: FRESH_PLAY_INITIALIZATION_SCHEMA_V1.into(),
            initialization_manifest_sha256: "a".repeat(64),
            lineage_id: "synthetic-transfer-origin-test".into(),
            initializer: INITIALIZER.into(),
            base_seed: 0,
            model_init_seed: 6_443_515_232_517_447_393,
            seed_derivation: SEED_DERIVATION.into(),
            producer_git_commit: "1".repeat(40),
            initial_weights_sha256: "b".repeat(64),
            initial_model_parameter_sha256: "c".repeat(64),
            parameter_layout_sha256: "d".repeat(64),
            destination_registry_sha256: "e".repeat(64),
            destination_card_db_hash: "f".repeat(16),
            destination_card_count,
            feature_contract_digest: "0".repeat(64),
            feature_encoding_digest: "1".repeat(64),
            features_source_sha256: "2".repeat(64),
            feature_descriptor_sha256: "3".repeat(64),
            sampler_identity: crate::fast_sampler::WIDE_CATEGORICAL_SAMPLER_VERSION_V1.into(),
        }
    }

    #[test]
    fn transferred_fresh_origin_nests_original_ancestry_and_rejects_duplicates_at_both_levels() {
        // Metadata-only synthetic fixture. This does not attest to a real
        // transfer, generated model, producer execution or playing strength.
        let original = synthetic_fresh(184);
        let wrapper = TransferredFreshPlayPolicyIdentityV1 {
            schema: TRANSFERRED_FRESH_PLAY_SCHEMA_V1.into(),
            original: original.clone(),
            destination_registry_sha256: "4".repeat(64),
            destination_card_db_hash: "5".repeat(16),
            destination_card_count: 190,
            transfer_envelope_sha256: "6".repeat(64),
        };
        let origin = PlayPolicyOriginV1::transferred_fresh_v1(wrapper).unwrap();
        let raw = serde_json::to_string(&origin).unwrap();
        let decoded: PlayPolicyOriginV1 = serde_json::from_str(&raw).unwrap();
        assert_eq!(decoded, origin);
        assert_eq!(serde_json::to_string(&decoded).unwrap(), raw);

        assert!(origin.is_fresh_v1());
        assert!(origin.as_imported_v1().is_none());
        // Destination accessors read the wrapper's own current identity.
        assert_eq!(origin.destination_registry_sha256_v1(), "4".repeat(64));
        assert_eq!(origin.destination_card_db_hash_v1(), "5".repeat(16));
        assert_eq!(origin.destination_card_count_v1(), 190);
        // Initial weight/seed/producer accessors read the nested ancestry.
        assert_eq!(origin.feature_contract_digest_v1(), original.feature_contract_digest);
        assert_eq!(origin.feature_encoding_digest_v1(), original.feature_encoding_digest);
        assert_eq!(origin.initial_weights_sha256_v1(), original.initial_weights_sha256);
        assert_eq!(
            origin.initial_model_parameter_sha256_v1(),
            original.initial_model_parameter_sha256
        );
        assert_eq!(origin.origin_git_commit_v1(), original.producer_git_commit);

        let top_duplicate = raw.replacen("{", "{\"schema\":\"duplicate\",", 1);
        assert!(serde_json::from_str::<PlayPolicyOriginV1>(&top_duplicate).is_err());
        let nested_duplicate = raw.replacen(
            "\"lineage_id\":",
            "\"lineage_id\":\"duplicate\",\"lineage_id\":",
            1,
        );
        assert_ne!(nested_duplicate, raw);
        assert!(serde_json::from_str::<PlayPolicyOriginV1>(&nested_duplicate).is_err());

        let missing_original = raw.replacen(
            &format!(",\"original\":{}", serde_json::to_string(&original).unwrap()),
            "",
            1,
        );
        assert_ne!(missing_original, raw);
        assert!(serde_json::from_str::<PlayPolicyOriginV1>(&missing_original).is_err());
    }

    #[test]
    fn transferred_fresh_origin_misschema_fails_with_the_clear_fresh_schema_error_not_a_missing_field_error(
    ) {
        let wrapper = TransferredFreshPlayPolicyIdentityV1 {
            schema: TRANSFERRED_FRESH_PLAY_SCHEMA_V1.into(),
            original: synthetic_fresh(184),
            destination_registry_sha256: "4".repeat(64),
            destination_card_db_hash: "5".repeat(16),
            destination_card_count: 190,
            transfer_envelope_sha256: "6".repeat(64),
        };
        let origin = PlayPolicyOriginV1::transferred_fresh_v1(wrapper).unwrap();
        let raw = serde_json::to_string(&origin).unwrap();
        let misspelled = raw.replace(
            TRANSFERRED_FRESH_PLAY_SCHEMA_V1,
            "misspelled-transferred-fresh-schema",
        );
        assert_ne!(misspelled, raw);
        let error = serde_json::from_str::<PlayPolicyOriginV1>(&misspelled)
            .err()
            .unwrap()
            .to_string();
        assert!(
            error.contains("fresh origin fields require the explicit fresh schema"),
            "{error}"
        );
    }
}
