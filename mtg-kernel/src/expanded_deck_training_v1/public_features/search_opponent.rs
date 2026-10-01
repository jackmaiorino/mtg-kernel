//! D3 search wrapper on the public collector's opponent seat (CODEX #521
//! C1 to C5). The wrapper is the unchanged evaluation `SearchPlayV3`; budget,
//! algorithm, sampler and descriptor are not parameters here. The frozen net
//! it wraps also scores each opponent decision, unsampled, so the stored row
//! keeps its ordinary shape; the learner never trains on these rows.
//! See docs/search_opponent_collection_v1.md.
use super::*;
use crate::learned_bo3_v1::public_evaluation::search_v3::SearchPlayV3;
use crate::native_flat_tensorizer_v4::NativeFlatDecisionTensorV4;
use crate::phase1_agent_v1::V4InformationSetSearchDescriptorV1;
use crate::rl_session::FastActorDecisionV1;
use crate::sideboard_play_policy_v1::FrozenPlayDecisionScoresV1;

/// Sampler identity of an opponent row chosen by the D3 wrapper.
pub(crate) const SEARCH_SAMPLER_IDENTITY: &str = "mtg-kernel-v4-information-set-estimate-search/v3";
/// Outer schema of a public trajectory whose opponent seat was the D3
/// wrapper. Distinct from the ordinary public schema so every reader that
/// checks the ordinary string refuses it (FABLE-REVIEW-20260927 change 3).
pub(crate) const SEARCH_OPPONENT_TRAJECTORY_SCHEMA: &str =
    "mtg-kernel-public-input-search-opponent-trajectory/v1";
/// Schema of the compact search record carried inside such a trajectory.
const SEARCH_RECORD_SCHEMA: &str = "mtg-kernel-public-search-opponent-record/v1";
/// The reviewed D3 budget: simulations, transitions, depth, experiment seed.
const D3_BUDGET: (u32, u32, u16, u64) = (128, 1024, 8, 20260922);
/// Prefix of the error string that carries a typed search failure record.
const SEARCH_FAILURE_MARKER: &str = "search-opponent-failure/v1:";
/// Prefix of the error string that carries a typed audit stop record
/// (FABLE-REVIEW-20260927 22:40, change 12).
const AUDIT_FAILURE_MARKER: &str = "search-opponent-audit-failure/v1:";
/// Opt-in sampler dump of the full hidden state; never set while collecting.
const HIDDEN_STATE_DUMP_ENV: &str = "MTG_V4_SEARCH_FAILURE_STATE";
/// E:/mtg-g115-lineage-20260923/d3-search-descriptor-reviewed.json.
pub(crate) const REVIEWED_DESCRIPTOR_SHA256: &str =
    "5eb1d55d13b78b341f8ff0c4df2589f8ee725974dc9fcadd691db6e12b2133d7";
/// Frozen g115, campaign-002/g/block115 iteration 199 update checkpoint.
pub(crate) const G115_CHECKPOINT_SHA256: &str =
    "88c0b997708c2b5156b44f3940ad9d5d682f78ac24d346978bb3c9f34c59e8d1";

/// Build that produced a search trajectory. Search simulation seeds derive
/// from engine encodings, so a search game reproduces only at this build.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SearchBuildV1 {
    git_head: String,
    git_clean: String,
    tracked_tree_sha256: String,
}

impl SearchBuildV1 {
    pub(crate) fn current() -> Self {
        Self {
            git_head: env!("MTG_KERNEL_BUILD_GIT_HEAD").into(),
            git_clean: env!("MTG_KERNEL_BUILD_GIT_CLEAN").into(),
            tracked_tree_sha256: env!("MTG_KERNEL_BUILD_TRACKED_TREE_SHA256").into(),
        }
    }
}

/// Compact binding of one wrapper decision. The digest names the full search
/// outcome; it does not reconstruct the tree, so determinism is shown by
/// replaying the game at the recorded build.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SearchDecisionV1 {
    step: u64,
    physical_decision_id: u64,
    substep_index: u32,
    substep_count: u32,
    environment_revision: u64,
    actor: u8,
    legal_action_count: u32,
    root_key: String,
    selected: u32,
    simulations: u32,
    transitions: u32,
    outcome_sha256: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SearchTrajectoryV1 {
    schema: String,
    seat: u8,
    descriptor_sha256: String,
    descriptor: V4InformationSetSearchDescriptorV1,
    build: SearchBuildV1,
    decisions: Vec<SearchDecisionV1>,
}

pub(crate) struct SearchOpponentV1 {
    wrapper: SearchPlayV3,
    seat: u8,
    descriptor_sha256: String,
    descriptor: V4InformationSetSearchDescriptorV1,
    count: usize,
    episode_id: String,
}

impl SearchOpponentV1 {
    /// Binds the reviewed descriptor and frozen g115 independently of the
    /// wrapper's own descriptor/model agreement check (CODEX #521 C1).
    pub(crate) fn load(
        pin: &PinnedFileV1,
        episode: &ExpandedEpisodeV1,
        net: &FrozenPlayPolicyV1,
        identity: &ExpandedInferenceIdentityV1,
    ) -> Result<Self, String> {
        ensure(
            pin.sha256 == REVIEWED_DESCRIPTOR_SHA256,
            "search opponent requires the reviewed D3 descriptor",
        )?;
        let source = episode
            .opponent
            .as_ref()
            .ok_or("search opponent wraps an explicit opponent model")?;
        ensure(
            source
                .checkpoint
                .as_ref()
                .is_some_and(|c| c.sha256 == G115_CHECKPOINT_SHA256)
                && identity.checkpoint_sha256.as_deref() == Some(G115_CHECKPOINT_SHA256),
            "search opponent requires the frozen g115 checkpoint",
        )?;
        let descriptor: V4InformationSetSearchDescriptorV1 =
            serde_json::from_slice(&read_pinned_bytes(pin)?).map_err(err)?;
        Self::new(
            net,
            descriptor,
            pin.sha256.clone(),
            1 - episode.learner_seat,
        )
    }

    /// Wrapper over a fresh fork of `net`, so the search's own policy state
    /// evolves exactly as in evaluation; `net` itself only scores rows.
    pub(crate) fn new(
        net: &FrozenPlayPolicyV1,
        descriptor: V4InformationSetSearchDescriptorV1,
        descriptor_sha256: String,
        seat: u8,
    ) -> Result<Self, String> {
        ensure(seat < 2, "invalid search seat")?;
        ensure(
            std::env::var_os(HIDDEN_STATE_DUMP_ENV).is_none(),
            "MTG_V4_SEARCH_FAILURE_STATE must stay unset while collecting with a search opponent",
        )?;
        let wrapper = SearchPlayV3::new(net.fork_for_collection_v3()?, descriptor.clone())?;
        Ok(Self {
            wrapper,
            seat,
            descriptor_sha256,
            descriptor,
            count: 0,
            episode_id: String::new(),
        })
    }

    /// Search decisions made in the current game.
    pub(crate) fn decisions(&self) -> u64 {
        self.count as u64
    }

    pub(crate) fn reset_for_game(
        &mut self,
        seeds: [u64; 2],
        episode_id: &str,
    ) -> Result<(), String> {
        self.wrapper.begin_match();
        self.count = 0;
        self.episode_id = episode_id.into();
        self.wrapper.reset_for_game_v1(seeds).map_err(err)
    }

    /// Scores the actor-visible tensor with the frozen net, unsampled, then
    /// lets the unchanged wrapper choose. Neither draws seat randomness.
    pub(crate) fn select(
        &mut self,
        net: &mut FrozenPlayPolicyV1,
        session: &FastActorSessionV1,
        decision: FastActorDecisionV1,
    ) -> Result<(u32, FrozenPlayDecisionScoresV1, NativeFlatDecisionTensorV4), String> {
        ensure(
            seat(decision.acting_player) == self.seat,
            "search wrapper asked to act for the learner seat",
        )?;
        let scores = net.score_fast_session_v1(session)?;
        let tensor = net.last_scored_training_tensor_v4()?.clone();
        let selected = self
            .wrapper
            .select_action_v1(PairedBo1PolicyInputV1::new(session, decision))
            .map_err(|error| self.failure(decision, error))?;
        self.count += 1;
        Ok((selected, scores, tensor))
    }

    /// A typed search error ends the run (no fallback, no retry). The error
    /// carries a failure record with public bindings only: episode, seat,
    /// decision position, menu width and the wrapper's error variant.
    fn failure(
        &self,
        decision: FastActorDecisionV1,
        error: crate::rl_session::RlSessionError,
    ) -> String {
        let record = json!({
            "schema": "mtg-kernel-public-search-opponent-failure/v1",
            "episode_id": self.episode_id,
            "seat": self.seat,
            "step": decision.step,
            "physical_decision_id": decision.physical_decision_id,
            "substep_index": decision.substep_index,
            "legal_action_count": decision.legal_action_count,
            "error": self.wrapper.records()["failure"]["error"].clone(),
            "descriptor_sha256": self.descriptor_sha256,
            "build": SearchBuildV1::current(),
            "non_claim": "No hidden state is recorded; MTG_V4_SEARCH_FAILURE_STATE is unset.",
        });
        format!("{SEARCH_FAILURE_MARKER}{record} ({})", error.message)
    }

    pub(crate) fn finish(&self) -> Result<SearchTrajectoryV1, String> {
        let records = self.wrapper.records();
        ensure(
            records["failure"].is_null(),
            "search wrapper recorded a failure",
        )?;
        let rows = records["decisions"]
            .as_array()
            .ok_or("search wrapper records are missing")?;
        ensure(rows.len() == self.count, "search record count differs")?;
        Ok(SearchTrajectoryV1 {
            schema: SEARCH_RECORD_SCHEMA.into(),
            seat: self.seat,
            descriptor_sha256: self.descriptor_sha256.clone(),
            descriptor: self.descriptor.clone(),
            build: SearchBuildV1::current(),
            decisions: rows.iter().map(compact).collect::<Result<_, _>>()?,
        })
    }
}

fn compact(row: &Value) -> Result<SearchDecisionV1, String> {
    let d = &row["decision"];
    let unsigned = |v: &Value, name: &str| v.as_u64().ok_or(format!("search record {name}"));
    let small = |v: &Value, name: &str| {
        unsigned(v, name)
            .and_then(|x| u32::try_from(x).map_err(|_| format!("search record {name}")))
    };
    let key = row["root_key"]
        .as_array()
        .ok_or("search record root_key")?
        .iter()
        .map(|b| b.as_u64().filter(|x| *x < 256).map(|x| format!("{x:02x}")))
        .collect::<Option<String>>()
        .filter(|k| k.len() == 64)
        .ok_or("search record root_key")?;
    Ok(SearchDecisionV1 {
        step: unsigned(&d["step"], "step")?,
        physical_decision_id: unsigned(&d["physical_decision_id"], "physical_decision_id")?,
        substep_index: small(&d["substep_index"], "substep_index")?,
        substep_count: small(&d["substep_count"], "substep_count")?,
        environment_revision: unsigned(&d["environment_revision"], "environment_revision")?,
        actor: seat(
            serde_json::from_value::<crate::rl::PlayerSeatV1>(d["actor"].clone()).map_err(err)?,
        ),
        legal_action_count: small(&d["legal_action_count"], "legal_action_count")?,
        root_key: key,
        selected: small(&row["selected"], "selected")?,
        simulations: small(&row["simulations"], "simulations")?,
        transitions: small(&row["transitions"], "transitions")?,
        outcome_sha256: row["outcome_sha256"]
            .as_str()
            .ok_or("search record outcome_sha256")?
            .into(),
    })
}

impl SearchTrajectoryV1 {
    /// Learner rows replay the recorded sampler as for any public trajectory;
    /// each search-seat row must match its compact record, in order.
    pub(super) fn validate(
        &self,
        episode: &ExpandedEpisodeV1,
        configuration_sha256: &[String; 2],
        decisions: &[DecisionRecordV1],
        terminal: &RlSessionTerminalV1,
    ) -> Result<(), String> {
        let d = &self.descriptor;
        ensure(
            self.schema == SEARCH_RECORD_SCHEMA
                && episode.opponent_search.as_ref().map(|p| &p.sha256)
                    == Some(&self.descriptor_sha256)
                && self.descriptor_sha256 == REVIEWED_DESCRIPTOR_SHA256
                && (d.simulations, d.transitions, d.depth, d.experiment_seed) == D3_BUDGET
                && self.seat == 1 - episode.learner_seat,
            "search trajectory identity differs from its episode",
        )?;
        let (simulations, transitions) = (d.simulations, d.transitions);
        let mut records = self.decisions.iter();
        {
            let mut check = |row: &DecisionRecordV1| -> Result<SeatRowDrawV1, String> {
                let r = records.next().ok_or("search row has no search record")?;
                ensure(
                    row.sampler_identity.as_deref() == Some(SEARCH_SAMPLER_IDENTITY)
                        && (
                            r.step,
                            r.physical_decision_id,
                            r.substep_index,
                            r.substep_count,
                            r.actor,
                        ) == (
                            row.step,
                            row.physical_decision_id,
                            row.substep_index,
                            row.substep_count,
                            row.actor,
                        )
                        && r.legal_action_count as usize == row.logits.len()
                        && r.selected == row.selected
                        && (1..=simulations).contains(&r.simulations)
                        && (1..=transitions).contains(&r.transitions),
                    "search row differs from its search record",
                )?;
                // The wrapper chose without the seat stream: no draw.
                Ok(SeatRowDrawV1::None)
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
        ensure(records.next().is_none(), "search record has no search row")
    }
}

/// Identity receipt for a run whose schedule contains search-opponent games:
/// descriptor and checkpoint pins, build and executable. None when the run
/// has no such game, so ordinary runs publish nothing new.
pub(crate) fn run_receipt(
    config: &Config,
    config_sha256: &str,
    invocation: std::ops::Range<usize>,
) -> Result<Option<Value>, String> {
    let search_ids = |updates: &[Vec<ExpandedEpisodeV1>]| -> Vec<String> {
        updates
            .iter()
            .flatten()
            .filter(|e| e.opponent_search.is_some())
            .map(|e| e.id.clone())
            .collect()
    };
    let scheduled = search_ids(&config.updates);
    if scheduled.is_empty() {
        return Ok(None);
    }
    let played = search_ids(
        config
            .updates
            .get(invocation.clone())
            .ok_or("receipt update range exceeds the schedule")?,
    );
    for episode in config.updates.iter().flatten() {
        if let Some(pin) = &episode.opponent_search {
            ensure(
                pin.sha256 == REVIEWED_DESCRIPTOR_SHA256
                    && episode
                        .opponent
                        .as_ref()
                        .and_then(|s| s.checkpoint.as_ref())
                        .is_some_and(|c| c.sha256 == G115_CHECKPOINT_SHA256),
                "search opponent requires the reviewed descriptor around frozen g115",
            )?;
        }
    }
    let executable = std::env::current_exe().map_err(err)?;
    Ok(Some(json!({
        "schema": "mtg-kernel-public-search-opponent-run-receipt/v2",
        "config_sha256": config_sha256,
        "descriptor_sha256": REVIEWED_DESCRIPTOR_SHA256,
        "opponent_checkpoint_sha256": G115_CHECKPOINT_SHA256,
        "scheduled_search_episode_ids": scheduled,
        // The updates this invocation plays; completion.json records how far
        // it got.
        "invocation_updates": [invocation.start, invocation.end],
        "invocation_search_episode_ids": played,
        "build": SearchBuildV1::current(),
        "executable_sha256": sha(&fs::read(executable).map_err(err)?),
        "non_claim": "Engineering identity only; no strength or promotion claim.",
    })))
}

/// Perturbations of a live root that leave the searcher's information set
/// unchanged (FABLE-REVIEW-20260927 change 2).
const AUDIT_VARIANTS: [&str; 4] = [
    "learner draw-consistent hand/library swap",
    "learner library order",
    "searcher library order",
    "future randomness",
];

/// Counts of one boundary audit. Observational and deterministic. A variant
/// is unavailable when it cannot be built at that root; a built variant that
/// changes the searcher's decision binding or visible key is inadmissible and
/// stops collection, since availability implies invariance by construction.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BoundaryAuditCountsV1 {
    pub(crate) roots: u64,
    pub(crate) checked: [u64; 4],
    pub(crate) unavailable: [u64; 4],
    pub(crate) inadmissible: [u64; 4],
}

/// The decision part of a search outcome: everything but node keys, whose
/// hidden zone-change offsets may legitimately differ.
fn audit_fields(outcome: &crate::model_guided_search_core_v4::Outcome) -> Result<Value, String> {
    let v = serde_json::to_value(outcome).map_err(err)?;
    let estimates: Vec<Value> = v["estimator"]["nodes"][0]["edges"]
        .as_array()
        .ok_or("boundary audit: estimator missing")?
        .iter()
        .map(|e| e["value"].clone())
        .collect();
    Ok(json!({
        "selected": v["estimator"]["selected_by_estimate"], "selected_by_core": v["selected"],
        "selected_by_mean": v["selected_by_mean"], "root_estimates": estimates,
        "root_visits": v["root_visits"], "root_value_sums": v["root_value_sums"],
        "root_priors": v["root_priors"], "root_work": v["root_work"], "nodes": v["nodes"],
        "simulations": v["simulations"], "transitions": v["transitions"],
        "headroom": v["headroom"], "census": v["census"],
    }))
}

impl SearchOpponentV1 {
    /// Reruns the unchanged D3 search on each perturbation of a live root.
    /// A fresh fork of the net searches; the wrapper, its records and the
    /// session are untouched. The rerun of the root must select the action
    /// the wrapper played (`recorded`); each built perturbation must keep the
    /// searcher's decision binding and visible key and give the same decision
    /// fields. A perturbation that cannot be built is counted as unavailable.
    pub(crate) fn audit_root(
        &self,
        net: &FrozenPlayPolicyV1,
        session: &FastActorSessionV1,
        decision: FastActorDecisionV1,
        recorded: u32,
        counts: &mut BoundaryAuditCountsV1,
    ) -> Result<(), String> {
        let (learner, searcher) = (PlayerId(1 - self.seat), PlayerId(self.seat));
        let variants = [
            session.diagnostic_draw_consistent_swap_clone_v1(learner),
            session
                .diagnostic_certificate_perturbed_clone_v1(Some(learner.index()), false)
                .ok(),
            session
                .diagnostic_certificate_perturbed_clone_v1(Some(searcher.index()), false)
                .ok(),
            session
                .diagnostic_certificate_perturbed_clone_v1(None, true)
                .ok(),
        ];
        self.audit_variants(net, session, decision, recorded, variants, counts)
    }

    pub(super) fn audit_variants(
        &self,
        net: &FrozenPlayPolicyV1,
        session: &FastActorSessionV1,
        decision: FastActorDecisionV1,
        recorded: u32,
        variants: [Option<FastActorSessionV1>; 4],
        counts: &mut BoundaryAuditCountsV1,
    ) -> Result<(), String> {
        use crate::model_guided_search_core_v4::Limits;
        let d = &self.descriptor;
        let limits = Limits {
            simulations: d.simulations,
            transitions: d.transitions,
            depth: d.depth,
            seed: d.experiment_seed,
        };
        let policy = net.fork_for_collection_v3()?;
        let search = |s: &FastActorSessionV1| {
            PairedBo1PolicyInputV1::new(s, decision)
                .report_search_future_v3(&policy, limits)
                .map_err(|e| format!("boundary audit search: {e:?}"))
                .and_then(|o| audit_fields(&o))
        };
        let stop = |cause: &str,
                    variant: Option<usize>,
                    counts: &BoundaryAuditCountsV1,
                    detail: Value,
                    message: String| {
            self.audit_stop(cause, variant, decision, counts, detail, message)
        };
        let expected = search(session).map_err(|e| {
            stop(
                "audit-error",
                None,
                counts,
                json!({ "error": e }),
                e.clone(),
            )
        })?;
        if expected["selected"] != json!(recorded) {
            return Err(stop(
                "rerun-differs-from-played",
                None,
                counts,
                json!({ "played": recorded, "rerun_selected": expected["selected"] }),
                format!(
                    "boundary audit: the fresh rerun selected {} but the wrapper played {recorded} at episode {} step {}",
                    expected["selected"], self.episode_id, decision.step
                ),
            ));
        }
        let depth = u32::from(d.depth);
        let key = session.kernel_search_visible_key_v4(depth).map_err(|e| {
            let e = format!("boundary audit key: {e:?}");
            stop(
                "audit-error",
                None,
                counts,
                json!({ "error": e }),
                e.clone(),
            )
        })?;
        counts.roots += 1;
        for (i, variant) in variants.iter().enumerate() {
            let Some(variant) = variant.as_ref() else {
                counts.unavailable[i] += 1;
                continue;
            };
            let binding = variant.current_response() == FastActorResponseV1::Decision(decision);
            let visible = variant.kernel_search_visible_key_v4(depth).ok() == Some(key);
            if !(binding && visible) {
                counts.inadmissible[i] += 1;
                return Err(stop(
                    "inadmissible-variant",
                    Some(i),
                    counts,
                    json!({ "decision_binding_kept": binding, "visible_key_kept": visible }),
                    format!(
                        "boundary audit: {} changed the searcher's decision binding or visible key at episode {} step {}",
                        AUDIT_VARIANTS[i], self.episode_id, decision.step
                    ),
                ));
            }
            let fields = search(variant).map_err(|e| {
                stop(
                    "audit-error",
                    Some(i),
                    counts,
                    json!({ "error": e }),
                    e.clone(),
                )
            })?;
            if fields != expected {
                let differing: Vec<&String> = expected
                    .as_object()
                    .into_iter()
                    .flatten()
                    .filter(|(k, v)| fields.get(k.as_str()) != Some(*v))
                    .map(|(k, _)| k)
                    .collect();
                return Err(stop(
                    "variant-changed-decision",
                    Some(i),
                    counts,
                    json!({ "differing_fields": differing }),
                    format!(
                        "boundary audit: {} changed the D3 decision at episode {} step {}",
                        AUDIT_VARIANTS[i], self.episode_id, decision.step
                    ),
                ));
            }
            counts.checked[i] += 1;
        }
        Ok(())
    }

    /// An audit stop (FABLE-REVIEW-20260927 22:40, change 12): the error
    /// carries a typed record with the cause, the variant, the root's public
    /// bindings and the root's counts so far; audit_live_root adds the run's
    /// counts before this root. Unavailable variants never stop an audit.
    fn audit_stop(
        &self,
        cause: &str,
        variant: Option<usize>,
        decision: FastActorDecisionV1,
        counts: &BoundaryAuditCountsV1,
        detail: Value,
        message: String,
    ) -> String {
        let record = json!({
            "schema": "mtg-kernel-public-search-opponent-audit-failure/v1",
            "cause": cause,
            "variant": variant.map(|i| AUDIT_VARIANTS[i]),
            "variant_index": variant,
            "episode_id": self.episode_id,
            "seat": self.seat,
            "step": decision.step,
            "physical_decision_id": decision.physical_decision_id,
            "substep_index": decision.substep_index,
            "legal_action_count": decision.legal_action_count,
            "root_counts": counts,
            "detail": detail,
            "descriptor_sha256": self.descriptor_sha256,
            "build": SearchBuildV1::current(),
            "non_claim": "Public bindings only; no hidden state is recorded.",
        });
        format!("{AUDIT_FAILURE_MARKER}{record} ({message})")
    }
}

/// Adds the run's counts before the stopping root to an audit stop record.
pub(super) fn with_run_counts(error: String, before: &BoundaryAuditCountsV1) -> String {
    let Some(start) = error.find(AUDIT_FAILURE_MARKER) else {
        return error;
    };
    let body = start + AUDIT_FAILURE_MARKER.len();
    let mut stream = serde_json::Deserializer::from_str(&error[body..]).into_iter::<Value>();
    let Some(Ok(mut record)) = stream.next() else {
        return error;
    };
    let rest = &error[body + stream.byte_offset()..];
    record["run_counts_before_this_root"] = json!(before);
    format!("{}{AUDIT_FAILURE_MARKER}{record}{rest}", &error[..start])
}

/// Run-wide audit counts. One trainer run per process; collector threads add
/// their per-root counts here and the run publishes the total at completion.
static AUDIT_COUNTS: std::sync::Mutex<BoundaryAuditCountsV1> =
    std::sync::Mutex::new(BoundaryAuditCountsV1 {
        roots: 0,
        checked: [0; 4],
        unavailable: [0; 4],
        inadmissible: [0; 4],
    });

pub(crate) fn reset_audit_counts() {
    *AUDIT_COUNTS.lock().unwrap_or_else(|p| p.into_inner()) = BoundaryAuditCountsV1::default();
}

/// Audits one live search root and adds its counts to the run total. A
/// mismatch stops collection like a typed search error.
pub(crate) fn audit_live_root(
    search: &SearchOpponentV1,
    net: &FrozenPlayPolicyV1,
    session: &FastActorSessionV1,
    decision: FastActorDecisionV1,
    recorded: u32,
) -> Result<(), String> {
    let mut counts = BoundaryAuditCountsV1::default();
    let audited = search.audit_root(net, session, decision, recorded, &mut counts);
    let mut total = AUDIT_COUNTS.lock().unwrap_or_else(|p| p.into_inner());
    if let Err(error) = audited {
        // The stop record carries the counts; nothing folds into the total,
        // since no report is published after a stop.
        return Err(with_run_counts(error, &total));
    }
    total.roots += counts.roots;
    for i in 0..4 {
        total.checked[i] += counts.checked[i];
        total.unavailable[i] += counts.unavailable[i];
        total.inadmissible[i] += counts.inadmissible[i];
    }
    Ok(())
}

pub(crate) fn audit_report(every: u32) -> Result<Value, String> {
    let counts = AUDIT_COUNTS
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .clone();
    Ok(json!({
        "schema": "mtg-kernel-public-search-opponent-boundary-audit/v2",
        "every": every,
        "variants": AUDIT_VARIANTS,
        "counts": counts,
        "assertion": "at each audited root a fresh rerun selected the action the wrapper played; each built variant kept the searcher's decision binding and visible key (a built variant that changes either stops collection) and gave the same D3 decision fields as the rerun (selected actions, root estimates, visits, value sums, priors, root work, nodes, simulations, transitions, headroom, census); a variant that cannot be built at a root is counted as unavailable; node keys and outcome digests are not compared",
        "non_claim": "Engineering boundary audit; no strength claim.",
    }))
}

/// Publishes the typed record a collection error carries, then returns the
/// error so the run still stops: `search-failure.json` for a typed search
/// error, `search-opponent-audit-failure.json` for an audit stop. Other
/// errors pass through unchanged.
pub(crate) fn publish_failure(directory: &Path, error: String) -> String {
    for (marker, name) in [
        (SEARCH_FAILURE_MARKER, "search-failure.json"),
        (AUDIT_FAILURE_MARKER, "search-opponent-audit-failure.json"),
    ] {
        let Some(start) = error.find(marker) else {
            continue;
        };
        let tail = &error[start + marker.len()..];
        let parsed = serde_json::Deserializer::from_str(tail)
            .into_iter::<Value>()
            .next()
            .and_then(Result::ok);
        return match parsed.map(|record| publish_json(directory, name, &record)) {
            Some(Ok(_)) => error,
            Some(Err(publish)) => format!("{error}; failure receipt not written: {publish}"),
            None => format!("{error}; failure record unreadable"),
        };
    }
    error
}

/// Byte copy of the reviewed D3 descriptor,
/// E:/mtg-g115-lineage-20260923/d3-search-descriptor-reviewed.json.
#[cfg(test)]
pub(crate) const REVIEWED_DESCRIPTOR_JSON: &str = r#"{"algorithm":"v4-depth-keyed-estimate-library-independent-chance/v3","depth":8,"embedding_table_sha256":"9f2ba50d7097345caf1930edd2e911bad87383524b30bbe09726fb344096f2bb","experiment_seed":20260922,"feature_contract_digest":"c4af415a3b0cf1e9c9960dbe2bc2d134c63e9f08206a9a364e113121fea5538b","feature_encoding_digest":"271c0e5a0fdce75663c897e89a9d7280ab1a3bbb6679bd10ecb5f524991952de","interior_bonus":"prior_free","model_parameter_sha256":"614326d2ec55c94583b1b050451f770ce9404e03bc21b9fb5fb6cb4f7d32263f","root_allocation":"round_robin","schema":"mtg-kernel-v4-information-set-estimate-search/v3","simulations":128,"transitions":1024,"weights_sha256":"e2ca2f2b5dd750a59e24c71a4bac325ed7449d97b5892a79a80132e45d538333"}"#;

#[cfg(test)]
mod boundary_tests;
#[cfg(test)]
mod collect_tests;
#[cfg(test)]
mod contract_tests;
