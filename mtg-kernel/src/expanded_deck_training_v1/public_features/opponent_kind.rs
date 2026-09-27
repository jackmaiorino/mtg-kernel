//! Opt-in opponent kinds of the public collector (opponent kinds interface
//! v1, docs/search_opponent_collection_v1.md): a recent public-input
//! checkpoint, and Legacy V3 (declared, refused until its adapters land).
//! Opponent rows never enter learner targets, normalization, entropy or
//! gradients; the update keeps reading learner rows only.
use super::search_opponent::{SearchBuildV1, SearchOpponentV1};
use super::*;
use crate::rl_session::FastActorDecisionV1;

#[cfg(test)]
mod tests;

/// Outer schema of a public trajectory whose opponent seat was a recent
/// public-input checkpoint; distinct so ordinary readers refuse it.
pub(crate) const PUBLIC_CHECKPOINT_TRAJECTORY_SCHEMA: &str =
    "mtg-kernel-public-input-public-checkpoint-opponent-trajectory/v1";
const OPPONENT_RECORD_SCHEMA: &str = "mtg-kernel-public-opponent-record/v1";

/// How one opponent row was produced.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum OpponentRowFormV1 {
    /// Scored and sampled; the decision row holds tensor, logits and value.
    Scored {
        generation: String,
        feature_contract_digest: String,
        feature_encoding_digest: String,
        observation: String,
        sampler_identity: Option<String>,
    },
}

/// Binding of one opponent decision to its row and ordered legal menu.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OpponentRowV1 {
    step: u64,
    physical_decision_id: u64,
    substep_index: u32,
    actor: u8,
    legal_action_count: u32,
    menu_sha256: String,
    form: OpponentRowFormV1,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OpponentRecordV1 {
    schema: String,
    kind: String,
    seat: u8,
    effective_identity: Value,
    build: SearchBuildV1,
    rows: Vec<OpponentRowV1>,
}

/// The opponent seat of one public-collector game.
pub(crate) enum OpponentSeatV1 {
    /// Ordinary V4 net, or the unchanged D3 wrapper around it.
    Net {
        net: FrozenPlayPolicyV1,
        identity: ExpandedInferenceIdentityV1,
        search: Option<SearchOpponentV1>,
    },
    /// A recent public-input checkpoint with its full model identity.
    PublicCheckpoint {
        policy: PublicInputPlayPolicyV1,
        identity: Value,
        rows: Vec<OpponentRowV1>,
    },
}

/// Loads an opt-in opponent kind; every refusal happens before collection.
pub(crate) fn load_seat(
    kind: &ExpandedOpponentKindV1,
    episode: &ExpandedEpisodeV1,
) -> Result<OpponentSeatV1, String> {
    episode.configurations_for_public_collector_v1()?;
    match kind {
        ExpandedOpponentKindV1::PublicCheckpoint { config, checkpoint } => {
            let (policy, identity) = load_for_evaluation(config, checkpoint)?;
            ensure(
                policy.feature_generation_v1()
                    == crate::paired_bo1_harness_v1::PlayPolicyGenerationV1::V4,
                "public-checkpoint opponent must be V4",
            )?;
            Ok(OpponentSeatV1::PublicCheckpoint {
                policy,
                identity,
                rows: Vec::new(),
            })
        }
        ExpandedOpponentKindV1::Legacy { .. } => {
            Err("legacy V3 opponents are declared but not admitted by this build".into())
        }
    }
}

/// SHA256 of the ordered legal menu the actor sees at this decision.
pub(crate) fn menu_sha256(
    session: &FastActorSessionV1,
    decision: FastActorDecisionV1,
) -> Result<String, String> {
    let (_, menu) = session
        .diagnostic_current_decision_input_v4(decision)
        .map_err(|e| format!("opponent menu binding: {e:?}"))?;
    Ok(sha(&serde_json::to_vec(&menu).map_err(err)?))
}

impl OpponentSeatV1 {
    pub(crate) fn outer_schema(&self) -> &'static str {
        match self {
            Self::Net {
                search: Some(_), ..
            } => super::search_opponent::SEARCH_OPPONENT_TRAJECTORY_SCHEMA,
            Self::Net { search: None, .. } => "mtg-kernel-public-input-trajectory/v1",
            Self::PublicCheckpoint { .. } => PUBLIC_CHECKPOINT_TRAJECTORY_SCHEMA,
        }
    }

    /// Record a public-checkpoint row: V4 scoring on the original observation
    /// with the ordinary sampler of its width.
    pub(crate) fn push_public_row(
        rows: &mut Vec<OpponentRowV1>,
        session: &FastActorSessionV1,
        decision: FastActorDecisionV1,
        width: usize,
    ) -> Result<(), String> {
        rows.push(OpponentRowV1 {
            step: decision.step,
            physical_decision_id: decision.physical_decision_id,
            substep_index: decision.substep_index,
            actor: seat(decision.acting_player),
            legal_action_count: decision.legal_action_count,
            menu_sha256: menu_sha256(session, decision)?,
            form: OpponentRowFormV1::Scored {
                generation: "v4".into(),
                feature_contract_digest: FEATURE_CONTRACT_DIGEST_V4.into(),
                feature_encoding_digest: FEATURE_ENCODING_DIGEST_V4.into(),
                observation: "original".into(),
                sampler_identity: decision_sampler_identity_v1(width).map(str::to_owned),
            },
        });
        Ok(())
    }
}

impl OpponentRecordV1 {
    pub(crate) fn public_checkpoint(seat: u8, identity: Value, rows: Vec<OpponentRowV1>) -> Self {
        Self {
            schema: OPPONENT_RECORD_SCHEMA.into(),
            kind: "public_checkpoint".into(),
            seat,
            effective_identity: identity,
            build: SearchBuildV1::current(),
            rows,
        }
    }

    /// Learner rows replay the behavior sampler as usual; each opponent row
    /// must match its record in order and replays one draw from its seat.
    pub(super) fn validate(
        &self,
        episode: &ExpandedEpisodeV1,
        configuration_sha256: &[String; 2],
        decisions: &[DecisionRecordV1],
        terminal: &RlSessionTerminalV1,
    ) -> Result<(), String> {
        ensure(
            self.schema == OPPONENT_RECORD_SCHEMA
                && self.kind == "public_checkpoint"
                && matches!(
                    episode.opponent_kind,
                    Some(ExpandedOpponentKindV1::PublicCheckpoint { .. })
                )
                && self.seat == 1 - episode.learner_seat,
            "opponent record identity differs from its episode",
        )?;
        let mut rows = self.rows.iter();
        {
            let mut check = |row: &DecisionRecordV1| -> Result<SeatRowDrawV1, String> {
                let r = rows.next().ok_or("opponent row has no record")?;
                let OpponentRowFormV1::Scored {
                    generation,
                    sampler_identity,
                    ..
                } = &r.form;
                ensure(
                    (r.step, r.physical_decision_id, r.substep_index, r.actor)
                        == (
                            row.step,
                            row.physical_decision_id,
                            row.substep_index,
                            row.actor,
                        )
                        && r.legal_action_count as usize == row.logits.len()
                        && generation == "v4"
                        && row.sampler_identity == *sampler_identity
                        && row.sampler_identity.as_deref()
                            == decision_sampler_identity_v1(row.logits.len()),
                    "opponent row differs from its record",
                )?;
                Ok(SeatRowDrawV1::Logits)
            };
            let check: SeatRowCheckV1<'_> = &mut check;
            validate_episode_records_with_search_v1(
                episode.configurations_for_public_collector_v1()?,
                episode,
                configuration_sha256,
                decisions,
                terminal,
                Some((self.seat, check)),
            )?;
        }
        ensure(rows.next().is_none(), "opponent record has no row")
    }
}
