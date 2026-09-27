//! Opt-in opponent kinds of the public collector (opponent kinds interface
//! v1, docs/search_opponent_collection_v1.md): a recent public-input
//! checkpoint, and Legacy V3 admitted against a declared-identity receipt.
//! Opponent rows never enter learner targets, normalization, entropy or
//! gradients; the update keeps reading learner rows only.
use super::search_opponent::{SearchBuildV1, SearchOpponentV1};
use super::*;
use crate::paired_bo1_harness_v1::PlayPolicyGenerationV1;
use crate::rl_session::FastActorDecisionV1;
use crate::sideboard_play_policy_v1::public_inputs::{
    select_forced_v3_for_evaluation, select_spell_adapter_v3_for_evaluation,
};
use crate::sideboard_play_policy_v1::FrozenPlayDecisionScoresV1;

#[cfg(test)]
mod tests;

/// Outer schema of a public trajectory whose opponent seat was a recent
/// public-input checkpoint; distinct so ordinary readers refuse it.
pub(crate) const PUBLIC_CHECKPOINT_TRAJECTORY_SCHEMA: &str =
    "mtg-kernel-public-input-public-checkpoint-opponent-trajectory/v1";
/// Outer schema of a public trajectory whose opponent seat was Legacy V3.
pub(crate) const LEGACY_TRAJECTORY_SCHEMA: &str =
    "mtg-kernel-public-input-legacy-opponent-trajectory/v1";
const OPPONENT_RECORD_SCHEMA: &str = "mtg-kernel-public-opponent-record/v1";
/// Per-member admission receipt shared with opus-panel-export (CLAUDE #472,
/// python/tools/line_a_panel_admission_v1.py); the frozen V3 uses it too.
pub(crate) const LEGACY_ADMISSION_SCHEMA: &str = "mtg-kernel-line-a-panel-admission/v1";
/// Sampler identity of an unscored V3 forced singleton row.
pub(crate) const V3_FORCED_SINGLETON_SAMPLER: &str = "mtg-kernel-v3-forced-singleton/v1";
/// Admitted routes. The R14 registry route is refused until its acceptance
/// is bound (CODEX #558: a matching receipt alone cannot admit it).
const LEGACY_ROUTES: [&str; 2] = ["v3-frozen", "strict"];

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
    /// V3 forced singleton: no forward pass; the row holds no logits and no
    /// tensor, and one draw from the seat stream selects action 0.
    UnscoredSingleton,
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
    selected: u32,
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

/// Import descriptor named by an admission receipt: the source's play import.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AdmissionImportV1 {
    pub(crate) path: PathBuf,
    pub(crate) sha256: String,
    pub(crate) schema: Option<String>,
}

/// The identity the loaded model must show.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AdmissionIdentityV1 {
    pub(crate) identity_schema: String,
    pub(crate) model_parameter_sha256: String,
    pub(crate) weights_sha256: String,
    pub(crate) feature_generation: String,
    pub(crate) observation_successor: bool,
    pub(crate) feature_contract_digest: String,
    pub(crate) feature_encoding_digest: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AdmissionFlagsV1 {
    pub(crate) v3_forced_actions: bool,
    pub(crate) v3_spell_target_reference_adapter: bool,
}

/// Declared-identity receipt for one Legacy V3 opponent (a panel import on
/// the strict route, or the frozen V3 relocated from the D3 envelope). The
/// model source is bound by its pinned descriptor file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LegacyAdmissionV1 {
    pub(crate) schema: String,
    pub(crate) member: String,
    pub(crate) route: String,
    pub(crate) model_source: PinnedFileV1,
    pub(crate) import_descriptor: AdmissionImportV1,
    pub(crate) expected_identity: AdmissionIdentityV1,
    pub(crate) adapter_flags: AdmissionFlagsV1,
    pub(crate) registry_pins: Value,
    pub(crate) evidence: Value,
    pub(crate) nonclaims: Vec<String>,
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
    /// Legacy V3 through the evaluator's adapter functions, in its order.
    Legacy {
        policy: FrozenPlayPolicyV1,
        identity: Value,
        forced: bool,
        spell_adapter: bool,
        rows: Vec<OpponentRowV1>,
    },
}

/// Checks a Legacy declaration against its admission receipt, then loads it
/// and verifies generation, observation successor and model identity. A
/// receipt with adapter flags true/true is not by itself authorization: the
/// member, route, source and model must all agree.
fn admit_legacy(
    source: &ExpandedModelSourceV1,
    forced: bool,
    spell_adapter: bool,
    admission: &PinnedFileV1,
) -> Result<(FrozenPlayPolicyV1, Value), String> {
    let receipt: LegacyAdmissionV1 =
        serde_json::from_slice(&read_pinned_bytes(admission)?).map_err(err)?;
    // The pinned descriptor file must hold exactly the declared source.
    let descriptor: ExpandedModelSourceV1 =
        serde_json::from_slice(&read_pinned_bytes(&receipt.model_source)?).map_err(err)?;
    let e = &receipt.expected_identity;
    ensure(
        receipt.schema == LEGACY_ADMISSION_SCHEMA
            && LEGACY_ROUTES.contains(&receipt.route.as_str())
            && descriptor == *source
            && (
                &receipt.import_descriptor.path,
                &receipt.import_descriptor.sha256,
            ) == (&source.play_import.path, &source.play_import.sha256)
            && e.feature_generation == "V3"
            && e.observation_successor
            && receipt.adapter_flags
                == (AdmissionFlagsV1 {
                    v3_forced_actions: forced,
                    v3_spell_target_reference_adapter: spell_adapter,
                }),
        "legacy opponent differs from its admission receipt",
    )?;
    let (policy, identity) = load_expanded_inference_v1(source)?;
    let origin = serde_json::to_value(&identity.source_import).map_err(err)?;
    ensure(
        policy.feature_generation_v1() == PlayPolicyGenerationV1::V3
            && policy.uses_observation_successor_v3()
            && origin["schema"] == e.identity_schema.as_str()
            && identity.model.model_parameter_sha256 == e.model_parameter_sha256
            && identity.model.weights_sha256 == e.weights_sha256
            && identity.model.feature_contract_digest == e.feature_contract_digest
            && identity.model.feature_encoding_digest == e.feature_encoding_digest,
        "legacy opponent must load as the admitted V3 model",
    )?;
    let effective = json!({
        "schema": "legacy-collection-model/v1", "member": receipt.member, "route": receipt.route,
        "admission": admission, "identity": identity,
        "v3_forced_actions": forced, "v3_spell_target_reference_adapter": spell_adapter,
    });
    Ok((policy, effective))
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
                policy.feature_generation_v1() == PlayPolicyGenerationV1::V4,
                "public-checkpoint opponent must be V4",
            )?;
            Ok(OpponentSeatV1::PublicCheckpoint {
                policy,
                identity,
                rows: Vec::new(),
            })
        }
        ExpandedOpponentKindV1::Legacy {
            source,
            v3_forced_actions,
            v3_spell_target_reference_adapter,
            admission,
        } => {
            let (policy, identity) = admit_legacy(
                source,
                *v3_forced_actions,
                *v3_spell_target_reference_adapter,
                admission,
            )?;
            Ok(OpponentSeatV1::Legacy {
                policy,
                identity,
                forced: *v3_forced_actions,
                spell_adapter: *v3_spell_target_reference_adapter,
                rows: Vec::new(),
            })
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

fn row(
    session: &FastActorSessionV1,
    decision: FastActorDecisionV1,
    selected: u32,
    form: OpponentRowFormV1,
) -> Result<OpponentRowV1, String> {
    Ok(OpponentRowV1 {
        step: decision.step,
        physical_decision_id: decision.physical_decision_id,
        substep_index: decision.substep_index,
        actor: seat(decision.acting_player),
        legal_action_count: decision.legal_action_count,
        selected,
        menu_sha256: menu_sha256(session, decision)?,
        form,
    })
}

fn digests(generation: &str) -> Option<(&'static str, &'static str)> {
    match generation {
        "v3" => Some((FEATURE_CONTRACT_DIGEST_V3, FEATURE_ENCODING_DIGEST_V3)),
        "v4" => Some((FEATURE_CONTRACT_DIGEST_V4, FEATURE_ENCODING_DIGEST_V4)),
        _ => None,
    }
}

fn scored(generation: &str, observation: &str, width: usize) -> OpponentRowFormV1 {
    let (contract, encoding) = digests(generation).expect("generation is v3 or v4");
    OpponentRowFormV1::Scored {
        generation: generation.into(),
        feature_contract_digest: contract.into(),
        feature_encoding_digest: encoding.into(),
        observation: observation.into(),
        sampler_identity: decision_sampler_identity_v1(width).map(str::to_owned),
    }
}

/// One opponent decision of a tensor-recording kind: the chosen action, its
/// scores and tensor (empty for an unscored singleton) and the row's sampler
/// identity when it is not the ordinary one of its width.
pub(super) type OpponentChoiceV1 = (
    u32,
    FrozenPlayDecisionScoresV1,
    TensorBitsV1,
    Option<String>,
);

impl OpponentSeatV1 {
    pub(crate) fn outer_schema(&self) -> &'static str {
        match self {
            Self::Net {
                search: Some(_), ..
            } => super::search_opponent::SEARCH_OPPONENT_TRAJECTORY_SCHEMA,
            Self::Net { search: None, .. } => "mtg-kernel-public-input-trajectory/v1",
            Self::PublicCheckpoint { .. } => PUBLIC_CHECKPOINT_TRAJECTORY_SCHEMA,
            Self::Legacy { .. } => LEGACY_TRAJECTORY_SCHEMA,
        }
    }

    /// Record a public-checkpoint row: V4 scoring on the original observation
    /// with the ordinary sampler of its width.
    pub(crate) fn push_public_row(
        rows: &mut Vec<OpponentRowV1>,
        session: &FastActorSessionV1,
        decision: FastActorDecisionV1,
        selected: u32,
        width: usize,
    ) -> Result<(), String> {
        rows.push(row(
            session,
            decision,
            selected,
            scored("v4", "original", width),
        )?);
        Ok(())
    }

    /// The evaluator's Legacy precedence: the forced singleton adapter first
    /// (when enabled), then the spell-target adapter (when enabled), then
    /// ordinary V3 scoring. Each consumes exactly one draw from the physical
    /// actor's continuing stream, singletons included.
    pub(super) fn legacy_decide(
        policy: &mut FrozenPlayPolicyV1,
        forced: bool,
        spell_adapter: bool,
        rows: &mut Vec<OpponentRowV1>,
        session: &FastActorSessionV1,
        decision: FastActorDecisionV1,
    ) -> Result<OpponentChoiceV1, String> {
        let input = PairedBo1PolicyInputV1::new(session, decision);
        if forced && decision.legal_action_count == 1 {
            let selected = select_forced_v3_for_evaluation(policy, &input).map_err(err)?;
            rows.push(row(
                session,
                decision,
                selected,
                OpponentRowFormV1::UnscoredSingleton,
            )?);
            let unscored = FrozenPlayDecisionScoresV1 {
                logits: Vec::new(),
                value: 0.0,
            };
            return Ok((
                selected,
                unscored,
                TensorBitsV1::empty(),
                Some(V3_FORCED_SINGLETON_SAMPLER.into()),
            ));
        }
        if spell_adapter {
            let (selected, scores, repaired) =
                select_spell_adapter_v3_for_evaluation(policy, &input).map_err(err)?;
            let tensor =
                TensorBitsV1::from_tensor(&policy.last_scored_training_tensor_v3()?.common);
            let observation = if repaired {
                "spell_target_repaired"
            } else {
                "original"
            };
            rows.push(row(
                session,
                decision,
                selected,
                scored("v3", observation, scores.logits.len()),
            )?);
            return Ok((selected, scores, tensor, None));
        }
        let (selected, scores, tensor) = policy.select_with_training_tensor_v3(session)?;
        rows.push(row(
            session,
            decision,
            selected,
            scored("v3", "original", scores.logits.len()),
        )?);
        Ok((
            selected,
            scores,
            TensorBitsV1::from_tensor(&tensor.common),
            None,
        ))
    }
}

impl OpponentRecordV1 {
    pub(crate) fn new(kind: &str, seat: u8, identity: Value, rows: Vec<OpponentRowV1>) -> Self {
        Self {
            schema: OPPONENT_RECORD_SCHEMA.into(),
            kind: kind.into(),
            seat,
            effective_identity: identity,
            build: SearchBuildV1::current(),
            rows,
        }
    }

    /// Learner rows replay the behavior sampler as usual; each opponent row
    /// must match its record in order and replays exactly one draw from its
    /// seat (over a single logit for an unscored singleton).
    pub(super) fn validate(
        &self,
        episode: &ExpandedEpisodeV1,
        configuration_sha256: &[String; 2],
        decisions: &[DecisionRecordV1],
        terminal: &RlSessionTerminalV1,
    ) -> Result<(), String> {
        let (kind, generation, singletons) = match &episode.opponent_kind {
            Some(ExpandedOpponentKindV1::PublicCheckpoint { .. }) => {
                ("public_checkpoint", "v4", false)
            }
            Some(ExpandedOpponentKindV1::Legacy {
                v3_forced_actions, ..
            }) => ("legacy", "v3", *v3_forced_actions),
            None => return Err("opponent record without an opponent kind".into()),
        };
        ensure(
            self.schema == OPPONENT_RECORD_SCHEMA
                && self.kind == kind
                && self.seat == 1 - episode.learner_seat,
            "opponent record identity differs from its episode",
        )?;
        let mut rows = self.rows.iter();
        {
            let mut check = |row: &DecisionRecordV1| -> Result<SeatRowDrawV1, String> {
                let r = rows.next().ok_or("opponent row has no record")?;
                ensure(
                    (
                        r.step,
                        r.physical_decision_id,
                        r.substep_index,
                        r.actor,
                        r.selected,
                    ) == (
                        row.step,
                        row.physical_decision_id,
                        row.substep_index,
                        row.actor,
                        row.selected,
                    ) && r.menu_sha256.len() == 64
                        && r.menu_sha256
                            .bytes()
                            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()),
                    "opponent row differs from its record",
                )?;
                match &r.form {
                    OpponentRowFormV1::Scored {
                        generation: g,
                        feature_contract_digest,
                        feature_encoding_digest,
                        observation,
                        sampler_identity,
                    } => {
                        ensure(
                            r.legal_action_count as usize == row.logits.len()
                                && g == generation
                                && digests(g)
                                    == Some((
                                        feature_contract_digest.as_str(),
                                        feature_encoding_digest.as_str(),
                                    ))
                                && (observation == "original"
                                    || (observation == "spell_target_repaired"
                                        && generation == "v3"))
                                && row.sampler_identity == *sampler_identity
                                && row.sampler_identity.as_deref()
                                    == decision_sampler_identity_v1(row.logits.len()),
                            "opponent row differs from its record",
                        )?;
                        Ok(SeatRowDrawV1::Logits)
                    }
                    OpponentRowFormV1::UnscoredSingleton => {
                        ensure(
                            singletons
                                && r.legal_action_count == 1
                                && row.selected == 0
                                && row.logits.is_empty()
                                && row.tensor.is_empty()
                                && row.sampler_identity.as_deref()
                                    == Some(V3_FORCED_SINGLETON_SAMPLER),
                            "unscored singleton row differs from its record",
                        )?;
                        Ok(SeatRowDrawV1::Singleton)
                    }
                }
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

/// Identity receipt for a run whose schedule declares opt-in opponent kinds:
/// each distinct declaration is loaded and admitted once before collection
/// (so a bad pin or receipt stops the run early) and its effective identity
/// is bound with the build and executable. None when the run declares none.
pub(crate) fn run_receipt(config: &Config, config_sha256: &str) -> Result<Option<Value>, String> {
    let mut kinds: Vec<&ExpandedOpponentKindV1> = Vec::new();
    let mut episodes = 0usize;
    for episode in config.updates.iter().flatten() {
        if let Some(kind) = &episode.opponent_kind {
            episode.configurations_for_public_collector_v1()?;
            episodes += 1;
            if !kinds.contains(&kind) {
                kinds.push(kind);
            }
        }
    }
    if kinds.is_empty() {
        return Ok(None);
    }
    let mut entries = Vec::new();
    for kind in kinds {
        match kind {
            ExpandedOpponentKindV1::PublicCheckpoint { config, checkpoint } => {
                let (_, identity) = load_for_evaluation(config, checkpoint)?;
                entries.push(json!({
                    "kind": "public_checkpoint", "config": config, "checkpoint": checkpoint,
                    "effective_identity": identity,
                    "sampler": "ordinary behavior sampler of each menu width",
                }));
            }
            ExpandedOpponentKindV1::Legacy {
                source,
                v3_forced_actions,
                v3_spell_target_reference_adapter,
                admission,
            } => {
                let (_, identity) = admit_legacy(
                    source,
                    *v3_forced_actions,
                    *v3_spell_target_reference_adapter,
                    admission,
                )?;
                entries.push(json!({
                    "kind": "legacy", "source": source, "admission": admission,
                    "effective_identity": identity,
                    "adapter_version": "evaluator precedence: forced singleton, spell-target adapter, ordinary V3",
                    "sampler": "ordinary behavior sampler of each menu width; one draw over a single logit for forced singletons",
                }));
            }
        }
    }
    let executable = std::env::current_exe().map_err(err)?;
    Ok(Some(json!({
        "schema": "mtg-kernel-public-opponent-kinds-receipt/v1",
        "config_sha256": config_sha256,
        "episodes": episodes,
        "kinds": entries,
        "build": SearchBuildV1::current(),
        "executable_sha256": sha(&fs::read(executable).map_err(err)?),
        "non_claim": "Engineering identity only; no strength or promotion claim.",
    })))
}
