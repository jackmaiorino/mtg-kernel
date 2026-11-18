//! The campaign checkpoint schema `limited-campaign-checkpoint/v1` (design
//! Section 3.4) and the verification of a checkpoint chain.
//!
//! A campaign directory holds, for every update `u` from 0:
//! - `checkpoints/step_UUUU.json`: one [`CheckpointV1`];
//! - `iterations/iter_UUUU/receipt.json`: the iteration receipt, whose SHA-256
//!   the checkpoint carries as `iteration_receipt_sha256`;
//! - `iterations/iter_UUUU/complete.json`: one [`IterationCompleteV1`],
//!   written last, which commits the iteration.
//!
//! Every file is the compact `serde_json` encoding of its type, and a reader
//! refuses bytes that do not re-encode to themselves, so a digest names one
//! exact file. `parent_checkpoint_sha256` is `null` at step 0 and the
//! SHA-256 of the previous step's file after it; [`verify_chain_v1`] walks
//! the chain from step 0 to a committed head and refuses any changed byte.
use super::{is_sha256_hex, sha256_hex};
use crate::current_mixed_policy_v1::Result as PolicyResult;
use crate::current_tree_learner_v1::{
    feature_identity_v1, AdamConfigV1, AuthenticatedInputV1, LearnerV1, ModelV1, SnapshotV1,
    CONTRACT_V1,
};
use crate::current_tree_learner_v2::{
    feature_identity_v2, feature_identity_v2_h16, LearnerV2, LearnerV2H16, ModelV2, ModelV2H16,
    OptimizerConfigV2, SnapshotV2, CONTRACT_V2, CONTRACT_V2_H16,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const CHECKPOINT_SCHEMA_V1: &str = "limited-campaign-checkpoint/v1";
pub const ITERATION_COMPLETE_SCHEMA_V1: &str = "limited-campaign-iteration-complete/v1";
/// The hand knowledge every campaign seat acts on (design Section 2.1).
pub const HAND_KNOWLEDGE_V1: &str = "public-reveal-minima/v3";

pub fn checkpoint_file_name_v1(step: u64) -> String {
    format!("step_{step:04}.json")
}
pub fn iteration_dir_name_v1(update: u64) -> String {
    format!("iter_{update:04}")
}
pub fn checkpoint_path_v1(root: &Path, step: u64) -> PathBuf {
    root.join("checkpoints").join(checkpoint_file_name_v1(step))
}
pub fn iteration_dir_v1(root: &Path, update: u64) -> PathBuf {
    root.join("iterations").join(iteration_dir_name_v1(update))
}

type Result<T> = std::result::Result<T, String>;

/// Which learner a campaign trains. The primary arm is the v2 actor-critic
/// at hidden width 32; width 16 is the compiled ablation profile; the
/// diagnostic control `pair-centered-reinforce/v1` runs on the unchanged v1
/// learner (design Section 3.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LearnerKindV1 {
    ActorCriticH32,
    ActorCriticH16,
    PairCenteredControl,
}

impl LearnerKindV1 {
    pub fn contract(self) -> &'static str {
        match self {
            Self::ActorCriticH32 => CONTRACT_V2,
            Self::ActorCriticH16 => CONTRACT_V2_H16,
            Self::PairCenteredControl => CONTRACT_V1,
        }
    }
    pub fn from_contract(contract: &str) -> Result<Self> {
        [
            Self::ActorCriticH32,
            Self::ActorCriticH16,
            Self::PairCenteredControl,
        ]
        .into_iter()
        .find(|kind| kind.contract() == contract)
        .ok_or_else(|| format!("unknown learner contract {contract:?}"))
    }
    /// The feature identity of this build for the learner.
    pub fn feature_sha256(self) -> String {
        match self {
            Self::ActorCriticH32 => feature_identity_v2(),
            Self::ActorCriticH16 => feature_identity_v2_h16(),
            Self::PairCenteredControl => feature_identity_v1(),
        }
    }
    /// The objective name recorded with the learner.
    pub fn objective(self) -> &'static str {
        match self {
            Self::ActorCriticH32 | Self::ActorCriticH16 => {
                "gae-actor-critic-scale-only-standardized/v1"
            }
            Self::PairCenteredControl => crate::current_tree_learner_v2::PAIR_CENTERED_CONTROL_V1,
        }
    }
}

/// The optimizer configuration as the learner's snapshot records it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OptimizerConfigV1 {
    ActorCritic(OptimizerConfigV2),
    PairCenteredControl(AdamConfigV1),
}

/// The learner's bit-exact state: a v2 snapshot for the actor-critic arms,
/// a v1 snapshot for the control arm.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LearnerSnapshotV1 {
    ActorCritic(SnapshotV2),
    PairCenteredControl(SnapshotV1),
}

/// The fields of a learner snapshot that the checkpoint repeats.
struct SnapshotFields {
    contract: String,
    feature_identity: String,
    config: OptimizerConfigV1,
    step: u64,
}

impl LearnerSnapshotV1 {
    fn fields(&self) -> Result<SnapshotFields> {
        match self {
            Self::ActorCritic(snapshot) => Ok(SnapshotFields {
                contract: snapshot.contract().into(),
                feature_identity: snapshot.feature_identity().into(),
                config: OptimizerConfigV1::ActorCritic(snapshot.config().clone()),
                step: snapshot.step(),
            }),
            // The v1 snapshot has no accessors; its serialized fields are
            // its public record.
            Self::PairCenteredControl(snapshot) => {
                #[derive(Deserialize)]
                struct Fields {
                    contract: String,
                    feature_identity: String,
                    config: AdamConfigV1,
                    step: u64,
                }
                let value = serde_json::to_value(snapshot).map_err(|error| error.to_string())?;
                let fields: Fields = serde_json::from_value(serde_json::json!({
                    "contract": value["contract"],
                    "feature_identity": value["feature_identity"],
                    "config": value["config"],
                    "step": value["step"],
                }))
                .map_err(|error| format!("v1 snapshot fields: {error}"))?;
                Ok(SnapshotFields {
                    contract: fields.contract,
                    feature_identity: fields.feature_identity,
                    config: OptimizerConfigV1::PairCenteredControl(fields.config),
                    step: fields.step,
                })
            }
        }
    }
    pub fn step(&self) -> Result<u64> {
        Ok(self.fields()?.step)
    }
}

/// The source identity every checkpoint, receipt and manifest of one campaign
/// carries (design Section 3.4). All values are lowercase hexadecimal except
/// the two profile names and `git_head`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceIdentityV1 {
    pub git_head: String,
    pub binary_sha256: String,
    pub binary_source_sha256: String,
    pub card_db_hash: String,
    pub sample_sha256_in_pool_order: String,
    pub actor_profile: String,
    pub actor_producer_sha256: String,
    pub hand_knowledge: String,
    pub reveal_producer_sha256: String,
    pub reveal_reducer_sha256: String,
}

impl SourceIdentityV1 {
    fn validate(&self) -> Result<()> {
        for (name, value) in [
            ("binary_sha256", &self.binary_sha256),
            ("binary_source_sha256", &self.binary_source_sha256),
            (
                "sample_sha256_in_pool_order",
                &self.sample_sha256_in_pool_order,
            ),
            ("actor_producer_sha256", &self.actor_producer_sha256),
            ("reveal_producer_sha256", &self.reveal_producer_sha256),
            ("reveal_reducer_sha256", &self.reveal_reducer_sha256),
        ] {
            if !is_sha256_hex(value) {
                return Err(format!("{name} is not a SHA-256 digest"));
            }
        }
        if self.card_db_hash.len() != 16
            || !self.card_db_hash.bytes().all(|b| b.is_ascii_hexdigit())
            || self.card_db_hash.bytes().any(|b| b.is_ascii_uppercase())
        {
            return Err("card_db_hash is not 16 lowercase hexadecimal digits".into());
        }
        if self.git_head.is_empty()
            || self.actor_profile.is_empty()
            || self.hand_knowledge != HAND_KNOWLEDGE_V1
        {
            return Err("checkpoint source identity is incomplete".into());
        }
        Ok(())
    }
}

/// One checkpoint, `checkpoints/step_UUUU.json`. The fields are those the
/// design lists for Section 3.4, plus `step` and `campaign_sha256` (the
/// SHA-256 of the campaign's `campaign.json`, which binds the checkpoint to
/// its configuration).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckpointV1 {
    pub schema: String,
    pub campaign_sha256: String,
    pub step: u64,
    pub git_head: String,
    pub binary_sha256: String,
    pub binary_source_sha256: String,
    pub card_db_hash: String,
    pub sample_sha256_in_pool_order: String,
    pub actor_profile: String,
    pub actor_producer_sha256: String,
    pub hand_knowledge: String,
    pub reveal_producer_sha256: String,
    pub reveal_reducer_sha256: String,
    pub learner_contract: String,
    pub feature_sha256: String,
    pub optimizer_config: OptimizerConfigV1,
    pub parent_checkpoint_sha256: Option<String>,
    pub iteration_receipt_sha256: String,
    pub snapshot: LearnerSnapshotV1,
}

impl CheckpointV1 {
    /// Builds a checkpoint around `snapshot`; the step, contract, feature
    /// identity and optimizer configuration are copied from the snapshot.
    pub fn new(
        campaign_sha256: &str,
        identity: &SourceIdentityV1,
        parent_checkpoint_sha256: Option<String>,
        iteration_receipt_sha256: &str,
        snapshot: LearnerSnapshotV1,
    ) -> Result<Self> {
        let fields = snapshot.fields()?;
        let checkpoint = Self {
            schema: CHECKPOINT_SCHEMA_V1.into(),
            campaign_sha256: campaign_sha256.into(),
            step: fields.step,
            git_head: identity.git_head.clone(),
            binary_sha256: identity.binary_sha256.clone(),
            binary_source_sha256: identity.binary_source_sha256.clone(),
            card_db_hash: identity.card_db_hash.clone(),
            sample_sha256_in_pool_order: identity.sample_sha256_in_pool_order.clone(),
            actor_profile: identity.actor_profile.clone(),
            actor_producer_sha256: identity.actor_producer_sha256.clone(),
            hand_knowledge: identity.hand_knowledge.clone(),
            reveal_producer_sha256: identity.reveal_producer_sha256.clone(),
            reveal_reducer_sha256: identity.reveal_reducer_sha256.clone(),
            learner_contract: fields.contract,
            feature_sha256: fields.feature_identity,
            optimizer_config: fields.config,
            parent_checkpoint_sha256,
            iteration_receipt_sha256: iteration_receipt_sha256.into(),
            snapshot,
        };
        checkpoint.validate()?;
        Ok(checkpoint)
    }

    pub fn identity(&self) -> SourceIdentityV1 {
        SourceIdentityV1 {
            git_head: self.git_head.clone(),
            binary_sha256: self.binary_sha256.clone(),
            binary_source_sha256: self.binary_source_sha256.clone(),
            card_db_hash: self.card_db_hash.clone(),
            sample_sha256_in_pool_order: self.sample_sha256_in_pool_order.clone(),
            actor_profile: self.actor_profile.clone(),
            actor_producer_sha256: self.actor_producer_sha256.clone(),
            hand_knowledge: self.hand_knowledge.clone(),
            reveal_producer_sha256: self.reveal_producer_sha256.clone(),
            reveal_reducer_sha256: self.reveal_reducer_sha256.clone(),
        }
    }

    pub fn learner_kind(&self) -> Result<LearnerKindV1> {
        LearnerKindV1::from_contract(&self.learner_contract)
    }

    /// Refuses a wrong schema, malformed digests, a snapshot whose contract,
    /// feature identity, configuration or step differs from the checkpoint's
    /// own fields, a snapshot of the wrong learner family, a feature identity
    /// other than this build's, and a parent digest at step 0 or a missing one
    /// after it.
    pub fn validate(&self) -> Result<()> {
        if self.schema != CHECKPOINT_SCHEMA_V1 {
            return Err(format!("checkpoint schema {:?} is not v1", self.schema));
        }
        self.identity().validate()?;
        for (name, value) in [
            ("campaign_sha256", &self.campaign_sha256),
            ("iteration_receipt_sha256", &self.iteration_receipt_sha256),
        ] {
            if !is_sha256_hex(value) {
                return Err(format!("{name} is not a SHA-256 digest"));
            }
        }
        match (self.step, &self.parent_checkpoint_sha256) {
            (0, None) => {}
            (0, Some(_)) => return Err("step 0 has a parent checkpoint".into()),
            (_, None) => return Err("a checkpoint after step 0 needs its parent".into()),
            (_, Some(parent)) if !is_sha256_hex(parent) => {
                return Err("parent_checkpoint_sha256 is not a SHA-256 digest".into())
            }
            _ => {}
        }
        let kind = self.learner_kind()?;
        let family_matches = matches!(
            (kind, &self.snapshot),
            (
                LearnerKindV1::ActorCriticH32 | LearnerKindV1::ActorCriticH16,
                LearnerSnapshotV1::ActorCritic(_)
            ) | (
                LearnerKindV1::PairCenteredControl,
                LearnerSnapshotV1::PairCenteredControl(_)
            )
        );
        if !family_matches {
            return Err("checkpoint snapshot belongs to another learner family".into());
        }
        let fields = self.snapshot.fields()?;
        if fields.contract != self.learner_contract
            || fields.feature_identity != self.feature_sha256
            || fields.config != self.optimizer_config
            || fields.step != self.step
        {
            return Err("checkpoint fields disagree with its learner snapshot".into());
        }
        if self.feature_sha256 != kind.feature_sha256() {
            return Err("checkpoint feature identity is not this build's".into());
        }
        Ok(())
    }

    /// The canonical bytes: the compact `serde_json` encoding.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        serde_json::to_vec(self).map_err(|error| error.to_string())
    }

    /// Parses and validates canonical checkpoint bytes and returns the
    /// checkpoint with the SHA-256 of `bytes`.
    pub fn parse_v1(bytes: &[u8]) -> Result<(Self, String)> {
        let checkpoint: Self =
            serde_json::from_slice(bytes).map_err(|error| format!("checkpoint: {error}"))?;
        if checkpoint.to_bytes()? != bytes {
            return Err("checkpoint bytes are not in canonical form".into());
        }
        checkpoint.validate()?;
        Ok((checkpoint, sha256_hex(bytes)))
    }

    pub fn load_v1(path: &Path) -> Result<(Self, String)> {
        let bytes = std::fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
        Self::parse_v1(&bytes).map_err(|error| format!("{}: {error}", path.display()))
    }

    /// Restores the learner with its optimizer state, through the learner's
    /// own `restore`, which checks the contract, identity and every bit.
    pub fn restore_learner_v1(&self) -> Result<CampaignLearnerV1> {
        let refuse = |error: crate::current_mixed_policy_v1::Error| {
            format!("checkpoint step {} does not restore: {error}", self.step)
        };
        let learner = match (self.learner_kind()?, &self.snapshot) {
            (LearnerKindV1::ActorCriticH32, LearnerSnapshotV1::ActorCritic(snapshot)) => {
                let learner = LearnerV2::restore(snapshot.clone()).map_err(refuse)?;
                CampaignLearnerV1::ActorCriticH32(learner)
            }
            (LearnerKindV1::ActorCriticH16, LearnerSnapshotV1::ActorCritic(snapshot)) => {
                let learner = LearnerV2H16::restore(snapshot.clone()).map_err(refuse)?;
                CampaignLearnerV1::ActorCriticH16(learner)
            }
            (
                LearnerKindV1::PairCenteredControl,
                LearnerSnapshotV1::PairCenteredControl(snapshot),
            ) => {
                let learner = LearnerV1::restore(snapshot.clone()).map_err(refuse)?;
                CampaignLearnerV1::PairCenteredControl(learner)
            }
            _ => return Err("checkpoint snapshot belongs to another learner family".into()),
        };
        Ok(learner)
    }

    /// The policy this checkpoint plays: its model, without optimizer state.
    pub fn policy_v1(&self) -> Result<CheckpointPolicyV1> {
        Ok(self.restore_learner_v1()?.policy())
    }
}

/// A restored learner with its optimizer state.
pub enum CampaignLearnerV1 {
    ActorCriticH32(LearnerV2),
    ActorCriticH16(LearnerV2H16),
    PairCenteredControl(LearnerV1),
}

impl CampaignLearnerV1 {
    pub fn kind(&self) -> LearnerKindV1 {
        match self {
            Self::ActorCriticH32(_) => LearnerKindV1::ActorCriticH32,
            Self::ActorCriticH16(_) => LearnerKindV1::ActorCriticH16,
            Self::PairCenteredControl(_) => LearnerKindV1::PairCenteredControl,
        }
    }
    pub fn snapshot(&self) -> LearnerSnapshotV1 {
        match self {
            Self::ActorCriticH32(learner) => LearnerSnapshotV1::ActorCritic(learner.snapshot()),
            Self::ActorCriticH16(learner) => LearnerSnapshotV1::ActorCritic(learner.snapshot()),
            Self::PairCenteredControl(learner) => {
                LearnerSnapshotV1::PairCenteredControl(learner.snapshot())
            }
        }
    }
    pub fn policy(&self) -> CheckpointPolicyV1 {
        match self {
            Self::ActorCriticH32(learner) => {
                CheckpointPolicyV1::ActorCriticH32(learner.model().clone())
            }
            Self::ActorCriticH16(learner) => {
                CheckpointPolicyV1::ActorCriticH16(learner.model().clone())
            }
            Self::PairCenteredControl(learner) => {
                CheckpointPolicyV1::PairCenteredControl(learner.model().clone())
            }
        }
    }
}

/// A checkpoint's model, for playing.
#[derive(Clone)]
pub enum CheckpointPolicyV1 {
    ActorCriticH32(ModelV2),
    ActorCriticH16(ModelV2H16),
    PairCenteredControl(ModelV1),
}

impl CheckpointPolicyV1 {
    /// Logits over the decision's full legal menu, in menu order. The v2
    /// models are libm-free; the v1 control model uses the platform `tanh`,
    /// so its logits, and the control arm's whole run, replay within one
    /// host class only.
    pub fn logits(&self, input: &AuthenticatedInputV1) -> PolicyResult<Vec<f64>> {
        match self {
            Self::ActorCriticH32(model) => Ok(model.forward(input)?.logits),
            Self::ActorCriticH16(model) => Ok(model.forward(input)?.logits),
            Self::PairCenteredControl(model) => model.forward(input),
        }
    }
}

/// `iterations/iter_UUUU/complete.json`: commits update `update`.
/// `dev_sha256` is the SHA-256 of the iteration's `dev.json` at a dev-panel
/// milestone and `null` otherwise.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IterationCompleteV1 {
    pub schema: String,
    pub update: u64,
    pub checkpoint_sha256: String,
    pub iteration_receipt_sha256: String,
    pub dev_sha256: Option<String>,
}

impl IterationCompleteV1 {
    pub fn new(
        update: u64,
        checkpoint_sha256: &str,
        iteration_receipt_sha256: &str,
        dev_sha256: Option<String>,
    ) -> Self {
        Self {
            schema: ITERATION_COMPLETE_SCHEMA_V1.into(),
            update,
            checkpoint_sha256: checkpoint_sha256.into(),
            iteration_receipt_sha256: iteration_receipt_sha256.into(),
            dev_sha256,
        }
    }
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        serde_json::to_vec(self).map_err(|error| error.to_string())
    }
    pub fn parse_v1(bytes: &[u8]) -> Result<Self> {
        let complete: Self = serde_json::from_slice(bytes)
            .map_err(|error| format!("iteration complete record: {error}"))?;
        if complete.to_bytes()? != bytes {
            return Err("iteration complete record is not in canonical form".into());
        }
        if complete.schema != ITERATION_COMPLETE_SCHEMA_V1
            || !is_sha256_hex(&complete.checkpoint_sha256)
            || !is_sha256_hex(&complete.iteration_receipt_sha256)
            || complete
                .dev_sha256
                .as_deref()
                .is_some_and(|digest| !is_sha256_hex(digest))
        {
            return Err("iteration complete record is malformed".into());
        }
        Ok(complete)
    }
}

/// One verified link of a chain.
pub struct VerifiedCheckpointV1 {
    pub checkpoint: CheckpointV1,
    pub sha256: String,
    pub complete: IterationCompleteV1,
}

fn read(path: &Path) -> Result<Vec<u8>> {
    std::fs::read(path).map_err(|error| format!("{}: {error}", path.display()))
}

/// Verifies the committed chain from step 0 to `head` under `root`. For each
/// update `u` it reads `complete.json`, the receipt, an optional `dev.json`
/// and `step_u.json`, and refuses unless: the record names update `u`; the
/// receipt's SHA-256 equals both the record's and the checkpoint's; the
/// checkpoint's SHA-256 equals the record's; the dev file's SHA-256 equals the
/// record's (and the file exists exactly when the record names it); the
/// checkpoint is step `u` with the previous step's SHA-256 as its parent; its
/// campaign digest, source identity, learner contract, feature identity and
/// optimizer configuration equal step 0's; and the learner restores.
pub fn verify_chain_v1(root: &Path, head: u64) -> Result<Vec<VerifiedCheckpointV1>> {
    let mut chain: Vec<VerifiedCheckpointV1> = Vec::new();
    for update in 0..=head {
        let directory = iteration_dir_v1(root, update);
        let complete = IterationCompleteV1::parse_v1(&read(&directory.join("complete.json"))?)
            .map_err(|error| format!("update {update}: {error}"))?;
        if complete.update != update {
            return Err(format!(
                "update {update}: complete record names another update"
            ));
        }
        let receipt_sha256 = sha256_hex(&read(&directory.join("receipt.json"))?);
        let dev_path = directory.join("dev.json");
        let dev_sha256 = if dev_path.exists() {
            Some(sha256_hex(&read(&dev_path)?))
        } else {
            None
        };
        let (checkpoint, sha256) = CheckpointV1::load_v1(&checkpoint_path_v1(root, update))?;
        let previous = chain.last();
        let parent = previous.map(|link| link.sha256.clone());
        if receipt_sha256 != complete.iteration_receipt_sha256
            || receipt_sha256 != checkpoint.iteration_receipt_sha256
        {
            return Err(format!("update {update}: iteration receipt digest differs"));
        }
        if sha256 != complete.checkpoint_sha256 {
            return Err(format!("update {update}: checkpoint digest differs"));
        }
        if dev_sha256 != complete.dev_sha256 {
            return Err(format!("update {update}: dev panel digest differs"));
        }
        if checkpoint.step != update || checkpoint.parent_checkpoint_sha256 != parent {
            return Err(format!(
                "update {update}: checkpoint parent chain is broken"
            ));
        }
        if let Some(first) = chain.first().map(|link| &link.checkpoint) {
            if checkpoint.campaign_sha256 != first.campaign_sha256
                || checkpoint.identity() != first.identity()
                || checkpoint.learner_contract != first.learner_contract
                || checkpoint.feature_sha256 != first.feature_sha256
                || checkpoint.optimizer_config != first.optimizer_config
            {
                return Err(format!(
                    "update {update}: checkpoint identity differs from step 0"
                ));
            }
        }
        checkpoint.restore_learner_v1()?;
        chain.push(VerifiedCheckpointV1 {
            checkpoint,
            sha256,
            complete,
        });
    }
    Ok(chain)
}

#[cfg(all(test, not(feature = "standard-magezero-fixtures")))]
#[path = "checkpoint_tests.rs"]
mod tests;
