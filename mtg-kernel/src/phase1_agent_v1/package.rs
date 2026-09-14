use super::{hex_digest, require};
use crate::bo3_match::PlayDrawChoiceV1;
use crate::expanded_deck_training_v1::{
    load_expanded_inference_v1, ExpandedSeatBehaviorV1, PinnedFileV1,
};
use crate::fast_sampler::{FAST_CATEGORICAL_SAMPLER_VERSION, WIDE_CATEGORICAL_SAMPLER_VERSION_V1};
use crate::kernel_native_search_opponent_v1::KernelNativeSearchAuthorityV1;
use crate::learned_bo3_v1::Bo3OpeningProtocolV1;
use crate::learned_sideboard_v1::{
    FrozenSideboardEmbeddingsV1, LearnedSideboardModelV1, SideboardPlayIdentityV1,
};
use crate::sideboard_play_policy_v1::FrozenPlayPolicyV1;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

pub const COMPLETE_AGENT_PACKAGE_SCHEMA_V1: &str = "mtg-kernel-complete-agent-package/v1";
const MAX_SIDEBOARD_CHECKPOINT_BYTES: u64 = 64 * 1024 * 1024;

/// Runtime identity is explicit and machine-specific. Relocating a file makes a
/// new package digest; it does not change the installed model's content identity.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentRuntimeIdentityV1 {
    pub executable: PinnedFileV1,
    pub toolchain: PinnedFileV1,
    pub engine_commit: String,
    /// The build script hashes the committed HEAD tree, not dirty worktree bytes.
    pub tracked_tree_sha256: String,
    pub tracked_tree_contract: String,
    pub build_git_clean: bool,
    pub card_db_hash: String,
    pub card_registry_sha256: String,
    pub feature_contract_digest: String,
    pub feature_encoding_digest: String,
    pub features_source_sha256: String,
    pub feature_descriptor_sha256: String,
}

/// Read-only evidence of the current executable and its compiled identities.
/// This does not certify another interface, a remote process or numerical parity.
/// Private fields prevent callers from constructing a successful check directly.
#[derive(Clone, Debug, Serialize)]
pub struct CurrentAgentRuntimeV1 {
    executable_path: PathBuf,
    executable_sha256: String,
    engine_commit: String,
    tracked_tree_sha256: String,
    tracked_tree_contract: String,
    toolchain_sha256: String,
}

impl CurrentAgentRuntimeV1 {
    pub fn executable_sha256_v1(&self) -> &str {
        &self.executable_sha256
    }
    pub fn executable_path_v1(&self) -> &Path {
        &self.executable_path
    }
}

impl AgentRuntimeIdentityV1 {
    pub fn verify_current_runtime_v1(&self) -> Result<CurrentAgentRuntimeV1, String> {
        pin_shape(&self.executable)?;
        pin_shape(&self.toolchain)?;
        verify_file(&self.executable)?;
        let executable_path = std::env::current_exe().map_err(|error| error.to_string())?;
        let executable_sha256 = file_sha256(&executable_path)?;
        require(
            executable_sha256 == self.executable.sha256,
            "supplied executable is not byte-identical to the currently running executable",
        )?;
        require(
            self.engine_commit == env!("MTG_KERNEL_BUILD_GIT_HEAD")
                && self.tracked_tree_sha256 == env!("MTG_KERNEL_BUILD_TRACKED_TREE_SHA256")
                && self.tracked_tree_contract == env!("MTG_KERNEL_BUILD_TRACKED_TREE_CONTRACT"),
            "runtime commit or tracked-tree identity differs from the compiled build",
        )?;
        // Matching HEAD/tree alone cannot describe uncommitted source changes.
        require(
            self.build_git_clean && env!("MTG_KERNEL_BUILD_GIT_CLEAN") == "true",
            "current-runtime verification requires a clean compiled source tree",
        )?;
        let toolchain_sha256 = format!(
            "{:x}",
            Sha256::digest(include_bytes!("../../../rust-toolchain.toml"))
        );
        let registry_sha256 = format!(
            "{:x}",
            Sha256::digest(include_bytes!("../../../data/cards_v1.json"))
        );
        require(
            self.toolchain.sha256 == toolchain_sha256
                && self.card_registry_sha256 == registry_sha256,
            "runtime toolchain/registry pin differs from compiled bytes",
        )?;
        verify_file(&self.toolchain)?;
        require(
            self.card_db_hash == format!("{:016x}", crate::card_def::KERNEL_CARDDB_HASH)
                && self.feature_contract_digest
                    == crate::native_flat_tensorizer_v3::FEATURE_CONTRACT_DIGEST_V3
                && self.feature_encoding_digest
                    == crate::native_flat_tensorizer_v3::FEATURE_ENCODING_DIGEST_V3
                && self.features_source_sha256
                    == crate::native_flat_tensorizer_v3::FEATURES_SOURCE_SHA256_V3
                && self.feature_descriptor_sha256
                    == crate::native_flat_tensorizer_v3::FEATURE_DESCRIPTOR_SHA256_V3,
            "runtime feature/card identity differs from the compiled loader",
        )?;
        Ok(CurrentAgentRuntimeV1 {
            executable_path,
            executable_sha256,
            engine_commit: self.engine_commit.clone(),
            tracked_tree_sha256: self.tracked_tree_sha256.clone(),
            tracked_tree_contract: self.tracked_tree_contract.clone(),
            toolchain_sha256,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuxiliaryPolicyBindingV1 {
    pub checkpoint: PinnedFileV1,
    pub play_weights_sha256: String,
    pub embedding_table_sha256: String,
    pub feature_contract_digest: String,
    pub feature_encoding_digest: String,
    pub card_db_hash: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AgentOpeningPolicyV1 {
    Existing {
        protocol: Bo3OpeningProtocolV1,
    },
    /// A descriptor for the future adapter, never an implicit keep-seven fallback.
    LearnedLondonV1 {
        policy: AuxiliaryPolicyBindingV1,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AgentPlayDrawPolicyV1 {
    Fixed { choice: PlayDrawChoiceV1 },
    LearnedV1 { policy: AuxiliaryPolicyBindingV1 },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AgentSideboardPolicyV1 {
    Keep,
    LearnedGreedyV1 {
        checkpoint: PinnedFileV1,
        play_identity: SideboardPlayIdentityV1,
        embedding_table_sha256: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AgentSearchPolicyV1 {
    Disabled,
    /// Preserve the actual older search contract. This does not assert V3/BO3
    /// compatibility, authorize its seeds, or install a search adapter.
    KernelNativeDescriptorV1 {
        authority: KernelNativeSearchAuthorityV1,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompleteAgentPackageV1 {
    pub schema: String,
    pub runtime: AgentRuntimeIdentityV1,
    pub gameplay: ExpandedSeatBehaviorV1,
    pub gameplay_sampler_identity: String,
    pub opening: AgentOpeningPolicyV1,
    pub play_draw: AgentPlayDrawPolicyV1,
    pub sideboard: AgentSideboardPolicyV1,
    pub search: AgentSearchPolicyV1,
}

/// Loaded inference components only. The full policy-routing match driver and
/// learned-opening/search adapters are separate work.
pub struct VerifiedAgentComponentsV1 {
    pub gameplay: FrozenPlayPolicyV1,
    pub sideboard: Option<LearnedSideboardModelV1>,
    pub package_sha256: String,
    pub current_runtime: CurrentAgentRuntimeV1,
}

fn pin_shape(pin: &PinnedFileV1) -> Result<(), String> {
    require(
        !pin.path.as_os_str().is_empty() && hex_digest(&pin.sha256, 64),
        "invalid agent artifact pin",
    )
}

fn verify_file(pin: &PinnedFileV1) -> Result<(), String> {
    pin_shape(pin)?;
    require(
        file_sha256(&pin.path)? == pin.sha256,
        "agent artifact bytes differ from their pin",
    )
}

fn file_sha256(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path).map_err(|error| error.to_string())?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 65_536];
    loop {
        let count = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

impl CompleteAgentPackageV1 {
    pub fn from_json_v1(json: &str) -> Result<Self, String> {
        let value = crate::rl::parse_strict_json_value(json).map_err(|error| error.to_string())?;
        let package: Self = serde_json::from_value(value).map_err(|error| error.to_string())?;
        package.validate_metadata_v1()?;
        Ok(package)
    }

    pub fn validate_metadata_v1(&self) -> Result<(), String> {
        require(
            self.schema == COMPLETE_AGENT_PACKAGE_SCHEMA_V1,
            "agent package schema differs",
        )?;
        let runtime = &self.runtime;
        pin_shape(&runtime.executable)?;
        pin_shape(&runtime.toolchain)?;
        require(
            hex_digest(&runtime.engine_commit, 40) && hex_digest(&runtime.card_db_hash, 16),
            "invalid runtime source/card identity",
        )?;
        require(
            hex_digest(&runtime.tracked_tree_sha256, 64)
                && !runtime.tracked_tree_contract.is_empty()
                && runtime.tracked_tree_contract.is_ascii(),
            "invalid runtime tracked-tree identity",
        )?;
        for value in [
            &runtime.card_registry_sha256,
            &runtime.feature_contract_digest,
            &runtime.feature_encoding_digest,
            &runtime.features_source_sha256,
            &runtime.feature_descriptor_sha256,
        ] {
            require(
                hex_digest(value, 64),
                "invalid runtime feature/registry digest",
            )?;
        }
        let behavior = &self.gameplay;
        let identity = &behavior.identity;
        let model = &identity.model;
        pin_shape(&behavior.source.play_import)?;
        if let Some(checkpoint) = &behavior.source.checkpoint {
            pin_shape(checkpoint)?;
        }
        require(
            identity.has_supported_origin_schema_v1()
                && model.schema == "mtg-kernel-actual-play-model/v1",
            "unknown installed gameplay identity",
        )?;
        require(
            identity.checkpoint_sha256
                == behavior
                    .source
                    .checkpoint
                    .as_ref()
                    .map(|pin| pin.sha256.clone()),
            "gameplay source and installed checkpoint differ",
        )?;
        for value in [
            &model.weights_sha256,
            &model.model_parameter_sha256,
            &model.embedding_table_sha256,
            &identity.state_sha256,
        ] {
            require(
                hex_digest(value, 64),
                "invalid actual gameplay parameter/state digest",
            )?;
        }
        require(
            model.card_db_hash == runtime.card_db_hash
                && identity.source_import.destination_registry_sha256_v1()
                    == runtime.card_registry_sha256.as_str()
                && model.feature_contract_digest == runtime.feature_contract_digest
                && model.feature_encoding_digest == runtime.feature_encoding_digest
                && identity.features_source_sha256 == runtime.features_source_sha256
                && identity.feature_descriptor_sha256 == runtime.feature_descriptor_sha256
                && behavior
                    .source
                    .feature_transfer
                    .expected_feature_contract_digest
                    == runtime.feature_contract_digest
                && behavior
                    .source
                    .feature_transfer
                    .expected_feature_encoding_digest
                    == runtime.feature_encoding_digest,
            "gameplay and runtime feature/card identities differ",
        )?;
        require(
            [
                FAST_CATEGORICAL_SAMPLER_VERSION,
                WIDE_CATEGORICAL_SAMPLER_VERSION_V1,
            ]
            .contains(&self.gameplay_sampler_identity.as_str()),
            "unknown gameplay behavior sampler",
        )?;
        if let AgentOpeningPolicyV1::Existing { protocol } = &self.opening {
            require(
                *protocol == Bo3OpeningProtocolV1::KeepSevenV2,
                "new packages cannot silently retain legacy opening semantics",
            )?;
        }
        let mut auxiliary = Vec::new();
        if let AgentOpeningPolicyV1::LearnedLondonV1 { policy } = &self.opening {
            auxiliary.push(policy);
        }
        if let AgentPlayDrawPolicyV1::LearnedV1 { policy } = &self.play_draw {
            auxiliary.push(policy);
        }
        for policy in auxiliary {
            pin_shape(&policy.checkpoint)?;
            require(
                policy.play_weights_sha256 == model.weights_sha256
                    && policy.embedding_table_sha256 == model.embedding_table_sha256
                    && policy.feature_contract_digest == runtime.feature_contract_digest
                    && policy.feature_encoding_digest == runtime.feature_encoding_digest
                    && policy.card_db_hash == runtime.card_db_hash,
                "auxiliary policy is incompatible with the installed player",
            )?;
        }
        if let AgentSideboardPolicyV1::LearnedGreedyV1 {
            checkpoint,
            play_identity,
            embedding_table_sha256,
        } = &self.sideboard
        {
            pin_shape(checkpoint)?;
            require(
                hex_digest(&play_identity.git_head, 40)
                    && play_identity.weights_sha256 == model.weights_sha256
                    && *embedding_table_sha256 == model.embedding_table_sha256,
                "sideboard head does not bind the installed gameplay weights/embeddings",
            )?;
        }
        if let AgentSearchPolicyV1::KernelNativeDescriptorV1 { authority } = &self.search {
            require(
                authority.engine_commit == runtime.engine_commit
                    && format!("{:016x}", authority.card_db_hash) == runtime.card_db_hash
                    && authority.transition_budget == authority.tier.transition_budget(),
                "search descriptor runtime/budget differs",
            )?;
        }
        Ok(())
    }

    pub fn package_sha256_v1(&self) -> Result<String, String> {
        self.validate_metadata_v1()?;
        let bytes = serde_json::to_vec(self).map_err(|error| error.to_string())?;
        let mut digest = Sha256::new();
        digest.update(b"mtg-kernel-complete-agent-package/v1\0");
        digest.update(bytes);
        Ok(format!("{:x}", digest.finalize()))
    }

    /// This deliberately refuses future descriptors until their execution paths
    /// have been implemented and qualified. There is no implicit fallback.
    pub fn load_supported_components_v1(&self) -> Result<VerifiedAgentComponentsV1, String> {
        self.validate_metadata_v1()?;
        require(
            matches!(
                self.opening,
                AgentOpeningPolicyV1::Existing {
                    protocol: Bo3OpeningProtocolV1::KeepSevenV2
                }
            ) && matches!(self.play_draw, AgentPlayDrawPolicyV1::Fixed { .. })
                && matches!(self.search, AgentSearchPolicyV1::Disabled),
            "learned opening/play-draw or search execution is not implemented by this interface",
        )?;
        let current_runtime = self.runtime.verify_current_runtime_v1()?;
        let (gameplay, actual) = load_expanded_inference_v1(&self.gameplay.source)?;
        require(
            actual == self.gameplay.identity
                && gameplay.runtime_sampler_identity_v1() == self.gameplay_sampler_identity,
            "loaded gameplay identity/sampler differs from the complete package",
        )?;
        let sideboard = match &self.sideboard {
            AgentSideboardPolicyV1::Keep => None,
            AgentSideboardPolicyV1::LearnedGreedyV1 {
                checkpoint,
                play_identity,
                ..
            } => {
                // Bind the bytes actually decoded. A separate hash pass followed
                // by reopening the path could load an unpinned replacement head.
                pin_shape(checkpoint)?;
                let mut bytes = Vec::new();
                fs::File::open(&checkpoint.path)
                    .map_err(|error| error.to_string())?
                    .take(MAX_SIDEBOARD_CHECKPOINT_BYTES + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|error| error.to_string())?;
                require(
                    bytes.len() as u64 <= MAX_SIDEBOARD_CHECKPOINT_BYTES,
                    "sideboard checkpoint exceeds 64 MiB",
                )?;
                require(
                    format!("{:x}", Sha256::digest(&bytes)) == checkpoint.sha256,
                    "loaded sideboard checkpoint bytes differ from their pin",
                )?;
                let text = String::from_utf8(bytes).map_err(|error| error.to_string())?;
                crate::rl::parse_strict_json_value(&text).map_err(|error| error.to_string())?;
                let head = LearnedSideboardModelV1::from_json_v1(&text)
                    .map_err(|error| error.to_string())?;
                require(
                    head.play_identity_v1() == play_identity,
                    "loaded sideboard play identity differs",
                )?;
                let embeddings = FrozenSideboardEmbeddingsV1::new_v1(
                    gameplay.embedding_rows_v1(),
                    play_identity.clone(),
                )
                .map_err(|error| error.to_string())?;
                head.validate_frozen_embeddings_v1(&embeddings)
                    .map_err(|error| error.to_string())?;
                Some(head)
            }
        };
        Ok(VerifiedAgentComponentsV1 {
            gameplay,
            sideboard,
            package_sha256: self.package_sha256_v1()?,
            current_runtime,
        })
    }
}
