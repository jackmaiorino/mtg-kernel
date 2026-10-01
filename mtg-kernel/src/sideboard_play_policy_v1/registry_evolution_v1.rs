//! Versioned registry-evolution import route (director ruling R14, 2026-09-27).
//!
//! A separate, explicit constructor that the V1 import loader cannot select,
//! following `from_registry_transfer_v1`. Every `load_v1` check is repeated in
//! the same order, except that the append-only namespace check is replaced by
//! an exact comparison against one committed allowlist of named card-field
//! differences, keyed to the pinned (source, destination) registry SHA256 pair.
//! The observed difference set must equal the allowlist exactly: an extra,
//! missing or changed difference, any name or order change, any count change
//! and an empty difference set (the V1 route) are refused. `load_v1`,
//! `load_feature_transfer_v3` and `validate_card_namespace` are unchanged.

use super::*;
use std::collections::BTreeSet;

pub const REGISTRY_EVOLUTION_IMPORT_SCHEMA_V1: &str =
    "mtg-kernel-frozen-play-registry-evolution-import/v1";
const IDENTITY_SCHEMA_V1: &str = "mtg-kernel-frozen-sideboard-play-registry-evolution-transfer/v1";
const ALLOWLIST_SCHEMA_V1: &str = "mtg-kernel-registry-evolution-allowlist/v1";
/// The source card database the allowlist describes: data/cards_v1.json at
/// 1804e9f9 under that commit's build.rs (card-DB recomputation receipt
/// 993da0d9). A source run recording any other card-DB hash is refused.
const SOURCE_CARD_DB_HASH_V1: &str = "a06fa9566106f0ea";
/// Learner initialization, collection and update refuse R14 descriptors.
pub(crate) const REGISTRY_EVOLUTION_LEARNER_REFUSAL_V1: &str =
    "registry evolution imports are inference-only archival opponents; learner initialization, collection and update refuse them";
/// The only admitted allowlist, keyed to source 16d308da (data/cards_v1.json
/// at 1804e9f9) and destination ef738001 (the compiled registry).
const ALLOWLIST_BYTES_V1: &[u8] = include_bytes!(
    "../../../data/line_a_source_registries/allowlists/r14-16d308da03202644-to-ef738001c3730760.json"
);

/// Pinned inputs of the R14 route: the V1 import fields with unchanged
/// meaning, plus the allowlist pin.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrozenPlayPolicyRegistryEvolutionImportV1 {
    pub schema: String,
    pub export_directory: PathBuf,
    pub expected_metadata_sha256: String,
    pub expected_model_parameter_sha256: String,
    pub source_run_path: PathBuf,
    pub source_registry_path: PathBuf,
    pub expected_source_registry_sha256: String,
    pub source_registry_git_commit: String,
    pub expected_destination_card_db_hash: String,
    pub allowlist_path: PathBuf,
    pub expected_allowlist_sha256: String,
}

/// One card field that differs between source and destination. An absent
/// field is `present: false` with a null value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryFieldDifferenceV1 {
    pub field: String,
    pub source_present: bool,
    pub source: Value,
    pub destination_present: bool,
    pub destination: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryEvolutionEntryV1 {
    pub id: usize,
    pub name: String,
    pub differences: Vec<RegistryFieldDifferenceV1>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryEvolutionAllowlistV1 {
    pub schema: String,
    pub source_registry_sha256: String,
    pub source_registry_revision: String,
    pub source_card_count: usize,
    pub destination_registry_sha256: String,
    pub destination_card_count: usize,
    pub excluded_fields: Vec<String>,
    pub field_difference_count: usize,
    pub entries: Vec<RegistryEvolutionEntryV1>,
}

pub fn pinned_allowlist_sha256_v1() -> String {
    hash(ALLOWLIST_BYTES_V1)
}

/// The compiled allowlist, shape-checked. Its bytes are the only admissible ones.
pub fn pinned_allowlist_v1() -> Result<RegistryEvolutionAllowlistV1, String> {
    let allowlist: RegistryEvolutionAllowlistV1 =
        serde_json::from_slice(ALLOWLIST_BYTES_V1).map_err(|e| e.to_string())?;
    validate_allowlist_shape_v1(&allowlist)?;
    Ok(allowlist)
}

fn validate_allowlist_shape_v1(allowlist: &RegistryEvolutionAllowlistV1) -> Result<(), String> {
    let ids: Vec<usize> = allowlist.entries.iter().map(|entry| entry.id).collect();
    require(
        allowlist.schema == ALLOWLIST_SCHEMA_V1
            && allowlist.excluded_fields == ["decks"]
            && !allowlist.entries.is_empty()
            && ids.windows(2).all(|pair| pair[0] < pair[1])
            && ids.iter().all(|id| *id < allowlist.source_card_count)
            && allowlist.field_difference_count
                == allowlist
                    .entries
                    .iter()
                    .map(|entry| entry.differences.len())
                    .sum::<usize>()
            && allowlist.entries.iter().all(|entry| {
                !entry.differences.is_empty()
                    && entry
                        .differences
                        .iter()
                        .all(|d| d.field != "name" && d.field != "decks")
            }),
        "registry evolution allowlist is malformed",
    )
}

/// The source run must record the pinned source card database.
fn require_pinned_source_card_db_v1(source_run: &Value) -> Result<(), String> {
    require(
        string(source_run, "/environment/card_db_hash_u64_hex")? == SOURCE_CARD_DB_HASH_V1,
        "source run card-DB hash is not the allowlist's pinned source card database",
    )
}

/// The descriptor's allowlist file must be the compiled allowlist, byte for byte.
fn check_allowlist_pin_v1(bytes: &[u8], expected_sha256: &str) -> Result<(), String> {
    require(
        bytes == ALLOWLIST_BYTES_V1 && hash(bytes) == expected_sha256,
        "registry evolution allowlist differs from the pinned allowlist",
    )
}

fn observed_differences_v1(
    source: &[Value],
    destination: &[Value],
) -> Result<Vec<RegistryEvolutionEntryV1>, String> {
    let mut entries = Vec::new();
    for (id, (old, new)) in source.iter().zip(destination).enumerate() {
        let old = old.as_object().ok_or("source card is not an object")?;
        let new = new.as_object().ok_or("destination card is not an object")?;
        let name = old
            .get("name")
            .and_then(Value::as_str)
            .filter(|name| !name.is_empty())
            .ok_or("source card name missing")?;
        require(
            new.get("name").and_then(Value::as_str) == Some(name),
            &format!("existing card-id {id} changed name or order"),
        )?;
        let fields: BTreeSet<&String> = old
            .keys()
            .chain(new.keys())
            .filter(|field| field.as_str() != "decks")
            .collect();
        let differences: Vec<RegistryFieldDifferenceV1> = fields
            .into_iter()
            .filter(|field| old.get(*field) != new.get(*field))
            .map(|field| RegistryFieldDifferenceV1 {
                field: field.clone(),
                source_present: old.contains_key(field),
                source: old.get(field).cloned().unwrap_or(Value::Null),
                destination_present: new.contains_key(field),
                destination: new.get(field).cloned().unwrap_or(Value::Null),
            })
            .collect();
        if !differences.is_empty() {
            entries.push(RegistryEvolutionEntryV1 {
                id,
                name: name.to_owned(),
                differences,
            });
        }
    }
    Ok(entries)
}

fn validate_differences_v1(
    source: &Value,
    destination: &Value,
    allowlist: &RegistryEvolutionAllowlistV1,
) -> Result<(usize, usize), String> {
    let source = source
        .get("cards")
        .and_then(Value::as_array)
        .ok_or("source cards missing")?;
    let destination = destination
        .get("cards")
        .and_then(Value::as_array)
        .ok_or("destination cards missing")?;
    require(
        !source.is_empty()
            && source.len() == allowlist.source_card_count
            && destination.len() == allowlist.destination_card_count
            && source.len() <= destination.len()
            && destination.len() < CARD_VOCAB_SIZE_V1,
        "card namespace counts differ from the pinned allowlist",
    )?;
    let mut names = BTreeSet::new();
    for entry in destination {
        let name = entry
            .get("name")
            .and_then(Value::as_str)
            .ok_or("card name missing")?;
        require(
            !name.is_empty() && names.insert(name),
            "duplicate or empty destination card name",
        )?;
    }
    let observed = observed_differences_v1(source, destination)?;
    require(
        !observed.is_empty(),
        "no registry differences: the V1 strict route applies",
    )?;
    require(
        observed == allowlist.entries,
        "observed registry differences differ from the pinned allowlist",
    )?;
    Ok((source.len(), destination.len()))
}

/// The R14 replacement for the namespace step: both registries must be the
/// allowlist's pinned pair and differ by exactly its enumerated differences.
pub fn validate_card_namespace_with_allowlist_v1(
    source_bytes: &[u8],
    destination_bytes: &[u8],
    allowlist: &RegistryEvolutionAllowlistV1,
) -> Result<(usize, usize), String> {
    validate_allowlist_shape_v1(allowlist)?;
    require(
        hash(source_bytes) == allowlist.source_registry_sha256,
        "source registry is not the allowlist's pinned source",
    )?;
    require(
        hash(destination_bytes) == allowlist.destination_registry_sha256,
        "destination registry is not the allowlist's pinned destination",
    )?;
    let source: Value = serde_json::from_slice(source_bytes).map_err(|e| e.to_string())?;
    let destination: Value =
        serde_json::from_slice(destination_bytes).map_err(|e| e.to_string())?;
    validate_differences_v1(&source, &destination, allowlist)
}

impl FrozenPlayPolicyV1 {
    /// R14 route: `load_v1` check for check with the allowlist namespace
    /// step, then the same explicit V3 observation transfer as
    /// `load_feature_transfer_v3`. Inference-only archival imports.
    pub(crate) fn load_registry_evolution_v3(
        input: &FrozenPlayPolicyRegistryEvolutionImportV1,
        transfer: &FrozenPlayObservationTransferV3,
    ) -> Result<Self, String> {
        require(
            input.schema == REGISTRY_EVOLUTION_IMPORT_SCHEMA_V1,
            "registry evolution descriptor schema differs",
        )?;
        require(
            transfer.expected_feature_contract_digest == FEATURE_CONTRACT_DIGEST_V3
                && transfer.expected_feature_encoding_digest == FEATURE_ENCODING_DIGEST_V3,
            "explicit V3 destination feature identity differs",
        )?;
        let allowlist_bytes = read_bounded(&input.allowlist_path, 1_048_576)?;
        check_allowlist_pin_v1(&allowlist_bytes, &input.expected_allowlist_sha256)?;
        let allowlist = pinned_allowlist_v1()?;

        let metadata_bytes = read_bounded(&input.export_directory.join("metadata.json"), 65_536)?;
        require(
            hash(&metadata_bytes) == input.expected_metadata_sha256,
            "export metadata SHA differs",
        )?;
        let metadata: Value = serde_json::from_slice(&metadata_bytes).map_err(|e| e.to_string())?;
        require(
            string(&metadata, "/schema")? == EXPORT_SCHEMA,
            "export schema differs",
        )?;
        require(
            string(&metadata, "/parameter_encoding")? == PARAMETER_ENCODING,
            "parameter encoding differs",
        )?;
        require(
            number(&metadata, "/parameter_byte_count")? == PARAMETER_BYTES as u64,
            "parameter count differs",
        )?;
        for (path, expected) in [
            ("/architecture_identity", MODEL_ARCHITECTURE_VERSION_V1),
            ("/model_config_fingerprint", MODEL_CONFIG_FINGERPRINT_V1),
            ("/feature_contract_digest", FEATURE_CONTRACT_DIGEST_V1),
            ("/feature_encoding_digest", FEATURE_ENCODING_DIGEST_V1),
            ("/source_validation", EXPORT_VALIDATION),
        ] {
            require(
                string(&metadata, path)? == expected,
                &format!("inference contract differs: {path}"),
            )?;
        }
        require(
            string(&metadata, "/identity/model_parameter_sha256")?
                == input.expected_model_parameter_sha256,
            "independent model identity differs",
        )?;
        require(
            string(&metadata, "/source/checkpoint/expected_model_sha256")?
                == input.expected_model_parameter_sha256,
            "export source model identity differs",
        )?;
        require(
            string(&metadata, "/exporter_build/source_git_commit")?
                == input.source_registry_git_commit,
            "registry provenance does not name exporter commit",
        )?;
        require(
            input.expected_destination_card_db_hash == format!("{KERNEL_CARDDB_HASH:016x}"),
            "destination card database differs from the intended transfer",
        )?;

        let source_run_bytes = read_bounded(&input.source_run_path, 1_048_576)?;
        let source_run_sha = string(&metadata, "/identity/loaded_run_sha256")?;
        require(
            hash(&source_run_bytes) == source_run_sha,
            "source run SHA differs from export",
        )?;
        let source_run: Value =
            serde_json::from_slice(&source_run_bytes).map_err(|e| e.to_string())?;
        require(
            string(&source_run, "/contracts/model/architecture_identity")?
                == MODEL_ARCHITECTURE_VERSION_V1,
            "source run model architecture differs",
        )?;
        require(
            string(&source_run, "/contracts/tensorizer/feature_contract_digest")?
                == FEATURE_CONTRACT_DIGEST_V1
                && string(&source_run, "/contracts/tensorizer/feature_encoding_digest")?
                    == FEATURE_ENCODING_DIGEST_V1,
            "source run feature contracts differ",
        )?;
        require_pinned_source_card_db_v1(&source_run)?;

        let source_registry_bytes = read_bounded(&input.source_registry_path, 4_194_304)?;
        require(
            hash(&source_registry_bytes) == input.expected_source_registry_sha256,
            "source registry SHA differs",
        )?;
        let (source_card_count, destination_card_count) =
            validate_card_namespace_with_allowlist_v1(
                &source_registry_bytes,
                DESTINATION_REGISTRY,
                &allowlist,
            )?;

        let raw = read_bounded(
            &input.export_directory.join("parameters.f32le"),
            PARAMETER_BYTES,
        )?;
        require(raw.len() == PARAMETER_BYTES, "parameter byte count differs")?;
        let weights_sha = hash(&raw);
        require(
            weights_sha == string(&metadata, "/parameter_section_sha256")?,
            "parameter SHA differs",
        )?;
        let parameters = decode_parameters(&raw)?;
        let embeddings = parameters
            .iter()
            .find(|p| p.name == "card_embedding.weight")
            .ok_or("card embedding tensor missing")?
            .values
            .clone();
        let mut model =
            NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
                .map_err(|e| format!("model construction: {e:?}"))?;
        model
            .replace_parameter_snapshot_v1(&parameters)
            .map_err(|e| format!("parameter validation: {e:?}"))?;
        require(
            model.parameter_manifest_sha256_v1() == input.expected_model_parameter_sha256,
            "named-parameter digest differs",
        )?;
        let training_decks = source_run
            .pointer("/environment/deck_ids")
            .and_then(Value::as_array)
            .ok_or("source run deck ids missing")?
            .iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| "invalid source deck id".to_owned())
            })
            .collect::<Result<Vec<_>, _>>()?;
        let identity = FrozenPlayPolicyIdentityV1 {
            schema: IDENTITY_SCHEMA_V1.into(),
            source_export_schema: EXPORT_SCHEMA.into(),
            source_metadata_sha256: input.expected_metadata_sha256.clone(),
            weights_sha256: weights_sha,
            model_parameter_sha256: input.expected_model_parameter_sha256.clone(),
            source_run_sha256: source_run_sha.into(),
            source_generation: number(&metadata, "/identity/loaded_generation")?,
            source_git_commit: input.source_registry_git_commit.clone(),
            source_card_db_hash: string(&source_run, "/environment/card_db_hash_u64_hex")?.into(),
            destination_card_db_hash: format!("{KERNEL_CARDDB_HASH:016x}"),
            source_registry_sha256: input.expected_source_registry_sha256.clone(),
            destination_registry_sha256: hash(DESTINATION_REGISTRY),
            source_card_count,
            destination_card_count,
            source_training_deck_ids: training_decks,
            namespace_rule: format!(
                "card-token=id+1; names and order identical; entries identical except deck membership and exactly the {} field differences on {} cards of registry evolution allowlist sha256 {} (source {}, destination {}); append-only",
                allowlist.field_difference_count,
                allowlist.entries.len(),
                input.expected_allowlist_sha256,
                allowlist.source_registry_sha256,
                allowlist.destination_registry_sha256
            ),
            appended_rows: "unchanged exported rows; no training or strength claim for appended cards".into(),
            feature_contract_digest: FEATURE_CONTRACT_DIGEST_V3.into(),
            feature_encoding_digest: FEATURE_ENCODING_DIGEST_V3.into(),
            sampler_identity: FAST_CATEGORICAL_SAMPLER_VERSION.into(),
            reader_revalidated_store_chain: false,
            observation_successor: Some(FrozenPlayObservationReceiptV3 {
                schema: "mtg-kernel-frozen-play-observation-transfer/v3".into(),
                source_feature_contract_digest: FEATURE_CONTRACT_DIGEST_V1.into(),
                source_feature_encoding_digest: FEATURE_ENCODING_DIGEST_V1.into(),
                destination: transfer.clone(),
                features_source_sha256: FEATURES_SOURCE_SHA256_V3.into(),
                feature_descriptor_sha256: FEATURE_DESCRIPTOR_SHA256_V3.into(),
                semantics: "rich V6 / flat V3 revision 3; exact public historical sources, chooser-only unordered library candidates, typed object-cost prefixes, private chosen-creature branch, visible refreshed paid power, and exact pending/queued Ward-to-targeter payment bindings; unchanged imported weights and dimensions; source inference transfer only, no learned competence claim".into(),
            }),
        };
        Ok(Self {
            model,
            embeddings,
            identity: identity.into(),
            encoder: FlatDecisionEncoderV2::default(),
            owned: OwnedScoringV1::default(),
            tensorizer: NativeFlatTensorizerV2::new(),
            tensor: NativeFlatDecisionTensorV2::default(),
            sampler: FastCategoricalScratch::default(),
            seat_rng: [SplitMix64::seed(0), SplitMix64::seed(0)],
            sampling_initialized: false,
            successor: Some(FrozenPlaySuccessorStateV3::default()),
            fresh_successor: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE_16D308DA: &[u8] = include_bytes!(
        "../../../data/line_a_source_registries/16d308da032026442994649291453846e2c857842542795092df12530011fd77.json"
    );

    fn parsed() -> (Value, Value, RegistryEvolutionAllowlistV1) {
        (
            serde_json::from_slice(SOURCE_16D308DA).unwrap(),
            serde_json::from_slice(DESTINATION_REGISTRY).unwrap(),
            pinned_allowlist_v1().unwrap(),
        )
    }

    #[test]
    fn exact_allowlist_is_admitted_and_v1_still_refuses_the_pair() {
        let allowlist = pinned_allowlist_v1().unwrap();
        assert_eq!(allowlist.entries.len(), 87);
        assert_eq!(allowlist.field_difference_count, 89);
        assert_eq!(
            validate_card_namespace_with_allowlist_v1(
                SOURCE_16D308DA,
                DESTINATION_REGISTRY,
                &allowlist
            ),
            Ok((136, 192))
        );
        let (source, destination, _) = parsed();
        assert!(validate_card_namespace(&source, &destination).is_err());
    }

    #[test]
    fn one_extra_or_one_missing_difference_is_refused() {
        let (mut source, destination, allowlist) = parsed();
        source["cards"][0]["mana_cost"] = serde_json::json!("{9}");
        assert!(validate_differences_v1(&source, &destination, &allowlist).is_err());
        let (mut source, destination, allowlist) = parsed();
        source["cards"][42]["mechanics"] = destination["cards"][42]["mechanics"].clone();
        assert!(validate_differences_v1(&source, &destination, &allowlist).is_err());
        // Removing one allowlisted difference (the first entry) is a missing difference.
        let (mut source, destination, allowlist) = parsed();
        let first = allowlist.entries[0].id;
        assert!(allowlist.entries[0]
            .differences
            .iter()
            .any(|d| d.field == "engine_capability"));
        source["cards"][first]["engine_capability"] =
            destination["cards"][first]["engine_capability"].clone();
        assert!(validate_differences_v1(&source, &destination, &allowlist).is_err());
    }

    #[test]
    fn presence_flag_only_and_changed_value_mutations_are_refused() {
        // Same value (null) but a different presence flag: present-null is not absent.
        let (mut source, destination, allowlist) = parsed();
        let first = allowlist.entries[0].id;
        source["cards"][first]["engine_capability"] = Value::Null;
        assert!(validate_differences_v1(&source, &destination, &allowlist).is_err());
        // An allowlisted field with a changed source value.
        let (mut source, destination, allowlist) = parsed();
        source["cards"][42]["mechanics"] = serde_json::json!(["etb_trigger"]);
        assert!(validate_differences_v1(&source, &destination, &allowlist).is_err());
        let (mut source, destination, allowlist) = parsed();
        source["cards"][131]["colors"] = serde_json::json!(["R"]);
        assert!(validate_differences_v1(&source, &destination, &allowlist).is_err());
    }

    #[test]
    fn a_source_run_with_another_card_database_is_refused() {
        let pinned =
            serde_json::json!({"environment": {"card_db_hash_u64_hex": "a06fa9566106f0ea"}});
        assert!(require_pinned_source_card_db_v1(&pinned).is_ok());
        let other =
            serde_json::json!({"environment": {"card_db_hash_u64_hex": "64c82a261e078f1a"}});
        assert!(require_pinned_source_card_db_v1(&other).is_err());
        assert!(require_pinned_source_card_db_v1(&serde_json::json!({})).is_err());
    }

    #[test]
    fn name_order_and_count_changes_are_refused() {
        let (mut source, destination, allowlist) = parsed();
        source["cards"][5]["name"] = serde_json::json!("Renamed Card");
        assert!(validate_differences_v1(&source, &destination, &allowlist).is_err());
        let (mut source, destination, allowlist) = parsed();
        source["cards"].as_array_mut().unwrap().swap(3, 4);
        assert!(validate_differences_v1(&source, &destination, &allowlist).is_err());
        let (mut source, destination, allowlist) = parsed();
        source["cards"].as_array_mut().unwrap().pop();
        assert!(validate_differences_v1(&source, &destination, &allowlist).is_err());
    }

    #[test]
    fn an_empty_difference_set_is_refused_because_v1_applies() {
        let (_, destination, allowlist) = parsed();
        let mut source = destination.clone();
        source["cards"].as_array_mut().unwrap().truncate(136);
        assert_eq!(
            validate_differences_v1(&source, &destination, &allowlist),
            Err("no registry differences: the V1 strict route applies".into())
        );
    }

    #[test]
    fn mutated_allowlist_bytes_and_unpinned_registries_are_refused() {
        let pinned = pinned_allowlist_sha256_v1();
        assert!(check_allowlist_pin_v1(ALLOWLIST_BYTES_V1, &pinned).is_ok());
        let mut mutated = ALLOWLIST_BYTES_V1.to_vec();
        let last = mutated.len() - 2;
        mutated[last] ^= 1;
        assert!(check_allowlist_pin_v1(&mutated, &pinned).is_err());
        assert!(check_allowlist_pin_v1(&mutated, &hash(&mutated)).is_err());
        let allowlist = pinned_allowlist_v1().unwrap();
        let mut destination = DESTINATION_REGISTRY.to_vec();
        destination.push(b'\n');
        assert!(validate_card_namespace_with_allowlist_v1(
            SOURCE_16D308DA,
            &destination,
            &allowlist
        )
        .is_err());
        let mut source = SOURCE_16D308DA.to_vec();
        source.push(b'\n');
        assert!(validate_card_namespace_with_allowlist_v1(
            &source,
            DESTINATION_REGISTRY,
            &allowlist
        )
        .is_err());
        let mut shape = allowlist.clone();
        shape.entries[0].differences[0].field = "name".into();
        assert!(validate_allowlist_shape_v1(&shape).is_err());
    }

    /// The braces of the first function whose text starts with `signature`.
    fn function_body<'a>(source: &'a str, signature: &str) -> &'a str {
        let start = source.find(signature).expect("function present");
        let open = start + source[start..].find('{').expect("function body");
        let mut depth = 0usize;
        for (offset, c) in source[open..].char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        return &source[open..=open + offset];
                    }
                }
                _ => {}
            }
        }
        panic!("unbalanced function body")
    }

    /// String literals of six or more characters, in order: the check
    /// messages and the metadata and run pointers each check reads.
    fn literals(body: &str) -> Vec<&str> {
        let mut found = Vec::new();
        let mut rest = body;
        while let Some(open) = rest.find('"') {
            let after = &rest[open + 1..];
            let close = after.find('"').expect("closed literal");
            let literal = &after[..close];
            if literal.len() >= 6 && !literal.contains('\n') {
                found.push(literal);
            }
            rest = &after[close + 1..];
        }
        found
    }

    /// FABLE-REVIEW-20260927 R14 point 3 (recommended parity test): this
    /// loader repeats load_v1's checks in load_v1's order. After its two
    /// route-specific checks, load_v1's ordered literals from the metadata
    /// read through the source deck ids appear here contiguously, so a check
    /// added to, removed from or reordered in either loader fails this test.
    #[test]
    fn the_loader_repeats_the_v1_checks_in_order() {
        let v1_source = include_str!("../sideboard_play_policy_v1.rs");
        let v1 = literals(function_body(
            v1_source,
            "pub fn load_v1(input: &FrozenPlayPolicyImportV1)",
        ));
        let r14 = literals(function_body(
            include_str!("registry_evolution_v1.rs"),
            "fn load_registry_evolution_v3(",
        ));
        let first = v1.iter().position(|s| *s == "metadata.json").unwrap();
        let last = v1
            .iter()
            .position(|s| *s == "invalid source deck id")
            .unwrap();
        let shared = &v1[first..=last];
        assert!(shared.len() >= 40, "{shared:?}");
        let offset = r14.iter().position(|s| *s == "metadata.json").unwrap();
        assert_eq!(
            r14[..offset],
            [
                "registry evolution descriptor schema differs",
                "explicit V3 destination feature identity differs"
            ]
        );
        assert_eq!(&r14[offset..offset + shared.len()], shared);
    }
}
