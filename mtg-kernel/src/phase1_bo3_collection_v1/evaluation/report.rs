//! Observes an unchanged Disabled policy. Reports are never behavior records.
use super::*;
const SCHEMA: &str = "mtg-kernel-bo3-report-search/v1";
const ROOT_BYTES: u64 = 1024 * 1024;
const JOB_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bo3ReportRootV1 {
    pub game_index: u8,
    pub decision_index: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bo3ReportOptionsV1 {
    pub simulations: u32,
    pub transitions: u32,
    pub depth: u16,
    pub seed: u64,
    pub roots: Vec<Bo3ReportRootV1>,
}
/// Exact archive input, separate from current-executable packages.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bo3ReportArchiveV1 {
    pub config: Bo3CollectionConfigV1,
    pub packages: [CompleteAgentPackageV1; 2],
    pub trajectory: Bo3TrainingTrajectoryV1,
}
impl Bo3ReportOptionsV1 {
    pub fn from_json_v1(text: &str) -> Result<Self, String> {
        ensure(text.len() <= 32768, "Report options exceed 32 KiB")?;
        let value = crate::rl::parse_strict_json_value(text).map_err(|e| e.to_string())?;
        serde_json::from_value(value).map_err(|e| e.to_string())
    }
}
impl Bo3ReportArchiveV1 {
    pub fn from_json_v1(text: &str) -> Result<Self, String> {
        // Largest frozen source bundle expands to 38,191,413 JSON bytes.
        ensure(
            text.len() <= 64 * 1024 * 1024,
            "Report archive exceeds 64 MiB",
        )?;
        let value = crate::rl::parse_strict_json_value(text).map_err(|e| e.to_string())?;
        serde_json::from_value(value).map_err(|e| e.to_string())
    }
}
#[derive(Debug, Serialize)]
pub struct Bo3ReportResultV1 {
    schema: String,
    config: Bo3CollectionConfigV1,
    options: Bo3ReportOptionsV1,
    packages: [CompleteAgentPackageV1; 2],
    archived_package_sha256: [String; 2],
    archive_sha256: String,
    current_runtimes: [CurrentAgentRuntimeV1; 2],
    evaluated: EvaluatedMatch,
    report: ReportSummary,
    semantic_sha256: String,
    timings: Vec<SearchTiming>,
}
#[derive(Debug, Serialize)]
struct ReportSummary {
    archive_replay_equal: bool,
    /// False if ANY frame or whole-match comparison fails. Rows remain visible
    /// for diagnosis but NONE is usable for disagreement in that case.
    usable_job: bool,
    first_frame_divergence: Option<Bo3ReportRootV1>,
    rows: Vec<ReportRow>,
    retained_payload_bytes: u64,
}
#[derive(Debug, Serialize)]
struct ReportRow {
    root: Bo3ReportRootV1,
    actor: PlayerSeatV1,
    receipt: Receipt,
    observation: Observation,
}
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Receipt {
    Missing,
    Pending,
    Committed,
    Discarded,
}
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[allow(
    clippy::large_enum_variant,
    reason = "retains the existing inline policy and report layout during merge preparation"
)]
enum Observation {
    Missing,
    FrameMismatch,
    Available { diagnostics: Compact },
    Rejected { error: search::Error },
    ObserverFailure { error: search::Error },
    Oversize { attempted_bytes: u64 },
    SerializationFailure { message: String },
}
#[derive(Debug, Serialize)]
struct Compact {
    ordinary_sampled: u32,
    ordinary_modal: Vec<u32>,
    selected: u32,
    selected_by_mean: u32,
    selected_by_estimate: u32,
    root_estimates: Vec<Option<i64>>,
    root_visits: Vec<u32>,
    root_value_sums: Vec<i64>,
    root_priors: Vec<u32>,
    root_work: Vec<search::RootWork>,
    census: search::Census,
    simulations: u32,
    transitions: u32,
    nodes: usize,
    headroom: [u64; 2],
    outcome_sha256: String,
}
pub(super) struct ReportSink {
    expected: Vec<(Bo3ReportRootV1, Bo3DecisionRecordV1)>,
    limits: search::Limits,
    summary: ReportSummary,
    timings: Vec<SearchTiming>,
    max_root_bytes: u64,
    max_job_bytes: u64,
}
fn equal_package_except_runtime(a: &CompleteAgentPackageV1, b: &CompleteAgentPackageV1) -> bool {
    let mut comparable = a.clone();
    comparable.runtime = b.runtime.clone();
    comparable == *b
}
impl ReportSink {
    fn new(
        config: &Bo3CollectionConfigV1,
        packages: [&CompleteAgentPackageV1; 2],
        options: &Bo3ReportOptionsV1,
        archive: &Bo3ReportArchiveV1,
    ) -> Result<Self, String> {
        ensure(
            config == &archive.config,
            "Report replay configuration differs from archive",
        )?;
        ensure(options.roots.len() <= 128, "Report frame exceeds 128 roots")?;
        ensure(
            (1..=1024).contains(&options.simulations)
                && options.transitions >= options.simulations
                && options.transitions <= 16384
                && (1..=32).contains(&options.depth),
            "invalid Report search limits",
        )?;
        for (i, package) in packages.iter().enumerate() {
            ensure(
                matches!(package.search, AgentSearchPolicyV1::Disabled),
                "Report observer requires both Disabled packages",
            )?;
            ensure(
                equal_package_except_runtime(package, &archive.packages[i]),
                "Report package differs beyond runtime",
            )?;
        }
        archive
            .trajectory
            .validate_v1(archive.packages.each_ref())?;
        ensure(
            matches!(
                archive.trajectory.ending,
                Bo3TrajectoryEndingV1::Complete { .. }
            ),
            "Report archive must be naturally complete",
        )?;
        ensure(
            archive.trajectory.match_id == config.match_id
                && archive.trajectory.initial_chooser == config.initial_chooser
                && archive.trajectory.registrations_by_seat == config.registrations,
            "Report archive identity differs",
        )?;
        let mut expected = Vec::new();
        let mut rows = Vec::new();
        let mut previous = None;
        for root in &options.roots {
            ensure(
                previous.is_none_or(|i| root.decision_index > i),
                "Report roots must be unique in decision order",
            )?;
            previous = Some(root.decision_index);
            let record = archive
                .trajectory
                .games
                .iter()
                .find(|g| g.game_index == root.game_index)
                .and_then(|g| {
                    g.decisions
                        .iter()
                        .find(|r| r.decision_index == root.decision_index)
                })
                .ok_or_else(|| "Report root absent from archive".to_owned())?;
            let ActorVisibleDecisionV1::Gameplay {
                ordered_actions, ..
            } = &record.visible
            else {
                return Err("Report root is not gameplay".into());
            };
            ensure(
                ordered_actions.len() > 1,
                "Report root must have multiple actions",
            )?;
            ensure(
                matches!(record.behavior, BehaviorDistributionV1::HamiltonQ64 { .. }),
                "Report requires archived Hamilton behavior",
            )?;
            expected.push((root.clone(), record.clone()));
            rows.push(ReportRow {
                root: root.clone(),
                actor: record.actor,
                receipt: Receipt::Missing,
                observation: Observation::Missing,
            });
        }
        Ok(Self {
            expected,
            limits: search::Limits {
                simulations: options.simulations,
                transitions: options.transitions,
                depth: options.depth,
                seed: options.seed,
            },
            summary: ReportSummary {
                archive_replay_equal: false,
                usable_job: false,
                first_frame_divergence: None,
                rows,
                retained_payload_bytes: 0,
            },
            timings: Vec::new(),
            max_root_bytes: ROOT_BYTES - 1024,
            max_job_bytes: JOB_BYTES - 128 * 1024,
        })
    }
    fn mismatch(&mut self, root: Bo3ReportRootV1, index: usize) {
        self.summary.first_frame_divergence = Some(root);
        self.summary.rows[index].observation = Observation::FrameMismatch;
    }
    pub(super) fn observe(
        &mut self,
        game_index: u8,
        record: &Bo3DecisionRecordV1,
        input: &PairedBo1PolicyInputV1<'_>,
        policy: &FrozenPlayPolicyV1,
    ) -> Option<usize> {
        if self.summary.first_frame_divergence.is_some() {
            return None;
        }
        let index = self
            .summary
            .rows
            .iter()
            .position(|r| r.receipt == Receipt::Missing)?;
        let (root, expected) = &self.expected[index];
        if record.decision_index < root.decision_index {
            return None;
        }
        let actual_root = Bo3ReportRootV1 {
            game_index,
            decision_index: record.decision_index,
        };
        if actual_root != *root
            || record.actor != expected.actor
            || record.behavior != expected.behavior
            || record.visible != expected.visible
        {
            self.mismatch(actual_root, index);
            return None;
        }
        let started = std::time::Instant::now();
        let result = input.report_search_v4(policy, self.limits);
        self.timings.push(SearchTiming {
            game_index,
            actor: record.actor,
            step: input.decision().step,
            elapsed_ns: u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX),
        });
        let observation = match result {
            Ok(outcome) => match compact(record, outcome) {
                Ok(diagnostics) => Observation::Available { diagnostics },
                Err(message) => Observation::SerializationFailure { message },
            },
            Err(
                error @ (search::Error::ObserverEnvironment(_)
                | search::Error::ObserverInvariant { .. }
                | search::Error::InvalidAdapterBinding),
            ) => Observation::ObserverFailure { error },
            Err(error) => Observation::Rejected { error },
        };
        self.store(index, observation);
        Some(index)
    }
    fn store(&mut self, index: usize, observation: Observation) {
        let observation = match serde_json::to_vec(&observation) {
            Ok(encoded) => {
                let bytes = encoded.len() as u64;
                if bytes > self.max_root_bytes
                    || self.summary.retained_payload_bytes.saturating_add(bytes)
                        > self.max_job_bytes
                {
                    Observation::Oversize {
                        attempted_bytes: bytes,
                    }
                } else {
                    self.summary.retained_payload_bytes += bytes;
                    observation
                }
            }
            Err(error) => Observation::SerializationFailure {
                message: error.to_string(),
            },
        };
        self.summary.rows[index].receipt = Receipt::Pending;
        self.summary.rows[index].observation = observation;
    }
    pub(super) fn commit(&mut self, index: usize) {
        self.summary.rows[index].receipt = Receipt::Committed;
    }
    pub(super) fn discard(&mut self, index: usize) {
        self.summary.rows[index].receipt = Receipt::Discarded;
    }
    fn finish(&mut self, evaluated: &EvaluatedMatch, archive: &Bo3ReportArchiveV1) {
        self.summary.archive_replay_equal = archive_equal(evaluated, &archive.trajectory);
        // An incomplete replay leaves unvisited assignments Missing, not rejected.
        if self.summary.archive_replay_equal && self.summary.first_frame_divergence.is_none() {
            if let Some(index) = self
                .summary
                .rows
                .iter()
                .position(|r| r.receipt == Receipt::Missing)
            {
                self.mismatch(self.expected[index].0.clone(), index);
            }
        }
        self.summary.usable_job = self.summary.archive_replay_equal
            && self.summary.first_frame_divergence.is_none()
            && self.summary.rows.iter().all(|r| {
                r.receipt == Receipt::Committed
                    && !matches!(
                        r.observation,
                        Observation::ObserverFailure { .. }
                            | Observation::SerializationFailure { .. }
                    )
            });
    }
}
fn compact(record: &Bo3DecisionRecordV1, outcome: search::Outcome) -> Result<Compact, String> {
    let BehaviorDistributionV1::HamiltonQ64 {
        selected_index,
        mass_numerators,
    } = &record.behavior
    else {
        return Err("ordinary behavior is not Hamilton".into());
    };
    let masses = mass_numerators
        .iter()
        .map(|s| s.parse::<u128>().map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    let maximum = masses
        .iter()
        .max()
        .ok_or_else(|| "empty Hamilton masses".to_owned())?;
    let ordinary_modal = masses
        .iter()
        .enumerate()
        .filter_map(|(i, m)| (m == maximum).then_some(i as u32))
        .collect();
    let estimate = outcome
        .estimator
        .as_ref()
        .ok_or_else(|| "Report lacks estimator".to_owned())?;
    let root = estimate
        .nodes
        .first()
        .ok_or_else(|| "Report lacks root estimate".to_owned())?;
    Ok(Compact {
        ordinary_sampled: *selected_index,
        ordinary_modal,
        selected: outcome.selected,
        selected_by_mean: outcome.selected_by_mean,
        selected_by_estimate: estimate.selected_by_estimate,
        root_estimates: root.edges.iter().map(|e| e.value).collect(),
        outcome_sha256: semantic_hash(&outcome)?,
        root_visits: outcome.root_visits,
        root_value_sums: outcome.root_value_sums,
        root_priors: outcome.root_priors,
        root_work: outcome.root_work,
        census: outcome.census,
        simulations: outcome.simulations,
        transitions: outcome.transitions,
        nodes: outcome.nodes,
        headroom: outcome.headroom,
    })
}
fn archive_equal(actual: &EvaluatedMatch, archive: &Bo3TrainingTrajectoryV1) -> bool {
    actual.abort.is_none()
        && actual.match_id == archive.match_id
        && actual.initial_chooser == archive.initial_chooser
        && actual.registrations_by_seat == archive.registrations_by_seat
        && actual.ending == archive.ending
        && actual.games.len() == archive.games.len()
        && actual.games.iter().zip(&archive.games).all(|(a, b)| {
            a.game_index == b.game_index
                && a.start == b.start
                && a.terminal == b.terminal
                && a.discarded_pending_selections == 0
                && a.decisions.len() == b.decisions.len()
                && a.decisions.iter().zip(&b.decisions).all(|(a, b)| {
                    let EvaluationDecision::Ordinary { record } = a else {
                        return false;
                    };
                    let mut comparable = record.clone();
                    comparable.behavior_package_sha256 =
                        archive.behavior_packages_by_seat[seat(record.actor)].clone();
                    comparable == *b
                })
        })
}
/// All archive comparison happens after ordinary replay, even after a frame mismatch.
/// This result is deliberately not a native training trajectory.
pub fn report_bo3_v4(
    config: Bo3CollectionConfigV1,
    packages: [CompleteAgentPackageV1; 2],
    options: Bo3ReportOptionsV1,
    archive: Bo3ReportArchiveV1,
) -> Result<Bo3ReportResultV1, String> {
    let evaluation_options = Bo3EvaluationOptionsV1 {
        max_retained_search_outcomes: 0,
    };
    validate_evaluation(&config, packages.each_ref(), &evaluation_options)?;
    let mut sink = ReportSink::new(&config, packages.each_ref(), &options, &archive)?;
    let [p0, p1] = packages
        .each_ref()
        .map(CompleteAgentPackageV1::load_evaluation_components_v1);
    let p0 = p0?;
    let p1 = p1?;
    let mut policies = [p0.gameplay, p1.gameplay];
    let heads = [p0.sideboard, p1.sideboard];
    let (evaluated, timings) = evaluate_loaded_with_report(
        &config,
        packages.each_ref(),
        &mut policies,
        heads.each_ref().map(Option::as_ref),
        &evaluation_options,
        Some(&mut sink),
    )?;
    ensure(
        timings.is_empty(),
        "Report unexpectedly entered the playing search route",
    )?;
    sink.finish(&evaluated, &archive);
    let archive_sha256 = semantic_hash(&archive)?;
    let semantic_sha256 = semantic_hash(&(
        &config,
        &options,
        &packages,
        &archive_sha256,
        &evaluated,
        &sink.summary,
    ))?;
    Ok(Bo3ReportResultV1 {
        schema: SCHEMA.into(),
        config,
        options,
        packages,
        archive_sha256,
        archived_package_sha256: archive.trajectory.behavior_packages_by_seat,
        current_runtimes: [p0.current_runtime, p1.current_runtime],
        evaluated,
        report: sink.summary,
        semantic_sha256,
        timings: sink.timings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::phase1_bo3_collection_v1::tests as fixtures;
    fn archive() -> (Bo3ReportArchiveV1, Bo3ReportOptionsV1) {
        let config = fixtures::config("report-parity");
        let (mut policies, packages) = fixtures::fixtures_v4([PlayDrawChoiceV1::Play; 2]);
        let native =
            collect_loaded(&config, packages.each_ref(), &mut policies, [None, None]).unwrap();
        let mut roots = Vec::new();
        for actor in [PlayerSeatV1::P0, PlayerSeatV1::P1] {
            let (g,r)=native.trajectory.games.iter().flat_map(|g|g.decisions.iter().map(move|r|(g,r)))
                .find(|(_,r)|r.actor==actor && matches!(&r.visible,ActorVisibleDecisionV1::Gameplay{ordered_actions,..} if ordered_actions.len()>1)).unwrap();
            roots.push(Bo3ReportRootV1 {
                game_index: g.game_index,
                decision_index: r.decision_index,
            });
        }
        roots.sort_by_key(|r| r.decision_index);
        (
            Bo3ReportArchiveV1 {
                config,
                packages,
                trajectory: native.trajectory,
            },
            Bo3ReportOptionsV1 {
                simulations: 32,
                transitions: 128,
                depth: 4,
                seed: 29,
                roots,
            },
        )
    }
    fn run(archive: &Bo3ReportArchiveV1, sink: &mut ReportSink) -> EvaluatedMatch {
        let mut policies = fixtures::fixtures_v4([PlayDrawChoiceV1::Play; 2]).0;
        let (actual, timings) = evaluate_loaded_with_report(
            &archive.config,
            archive.packages.each_ref(),
            &mut policies,
            [None, None],
            &Bo3EvaluationOptionsV1 {
                max_retained_search_outcomes: 0,
            },
            Some(sink),
        )
        .unwrap();
        assert!(timings.is_empty());
        sink.finish(&actual, archive);
        actual
    }
    #[test]
    fn v4_evaluation_report_both_seats_and_empty_frame_preserve_every_native_record() {
        let (archive, options) = archive();
        let mut sink = ReportSink::new(
            &archive.config,
            archive.packages.each_ref(),
            &options,
            &archive,
        )
        .unwrap();
        let actual = run(&archive, &mut sink);
        assert!(sink.summary.usable_job);
        assert_eq!(sink.timings.len(), 2);
        assert!(sink
            .summary
            .rows
            .iter()
            .all(|r| r.receipt == Receipt::Committed));
        assert!(sink
            .summary
            .rows
            .iter()
            .any(|r| matches!(r.observation, Observation::Available { .. })));
        let mut repeat = ReportSink::new(
            &archive.config,
            archive.packages.each_ref(),
            &options,
            &archive,
        )
        .unwrap();
        let repeated = run(&archive, &mut repeat);
        assert_eq!(
            semantic_hash(&actual).unwrap(),
            semantic_hash(&repeated).unwrap()
        );
        assert_eq!(
            semantic_hash(&sink.summary).unwrap(),
            semantic_hash(&repeat.summary).unwrap()
        );
        let mut empty_options = options;
        empty_options.roots.clear();
        let mut empty = ReportSink::new(
            &archive.config,
            archive.packages.each_ref(),
            &empty_options,
            &archive,
        )
        .unwrap();
        let empty_actual = run(&archive, &mut empty);
        assert!(empty.summary.usable_job);
        assert!(empty.timings.is_empty());
        assert_eq!(
            semantic_hash(&actual).unwrap(),
            semantic_hash(&empty_actual).unwrap()
        );
        let native_bytes: u64 = archive
            .trajectory
            .games
            .iter()
            .flat_map(|g| &g.decisions)
            .map(|r| serde_json::to_vec(r).unwrap().len() as u64)
            .sum();
        assert_eq!(actual.committed_decision_json_bytes, native_bytes);
    }
    #[test]
    fn v4_evaluation_report_rejection_and_independent_byte_cap_leave_play_unchanged() {
        let (archive, mut options) = archive();
        options.simulations = 1;
        let mut rejected = ReportSink::new(
            &archive.config,
            archive.packages.each_ref(),
            &options,
            &archive,
        )
        .unwrap();
        let actual = run(&archive, &mut rejected);
        assert!(rejected.summary.usable_job);
        assert!(rejected.summary.rows.iter().all(|r| matches!(
            r.observation,
            Observation::Rejected {
                error: search::Error::InvalidBudget
            }
        )));
        let mut capped = ReportSink::new(
            &archive.config,
            archive.packages.each_ref(),
            &options,
            &archive,
        )
        .unwrap();
        capped.max_root_bytes = 1;
        capped.max_job_bytes = 1;
        let capped_actual = run(&archive, &mut capped);
        assert!(capped.summary.usable_job);
        assert_eq!(capped.summary.retained_payload_bytes, 0);
        assert!(capped
            .summary
            .rows
            .iter()
            .all(|r| matches!(r.observation, Observation::Oversize { .. })));
        assert_eq!(
            semantic_hash(&actual).unwrap(),
            semantic_hash(&capped_actual).unwrap()
        );
    }
    #[test]
    fn v4_evaluation_report_mismatch_stops_observation_not_replay_and_marks_whole_job_unusable() {
        let (archive, options) = archive();
        let mut sink = ReportSink::new(
            &archive.config,
            archive.packages.each_ref(),
            &options,
            &archive,
        )
        .unwrap();
        sink.expected[0].1.actor = match sink.expected[0].1.actor {
            PlayerSeatV1::P0 => PlayerSeatV1::P1,
            PlayerSeatV1::P1 => PlayerSeatV1::P0,
        };
        run(&archive, &mut sink);
        assert!(sink.summary.archive_replay_equal);
        assert!(!sink.summary.usable_job);
        assert!(sink.timings.is_empty());
        assert!(sink.summary.first_frame_divergence.is_some());
        assert!(matches!(
            sink.summary.rows[0].observation,
            Observation::FrameMismatch
        ));
        assert!(matches!(
            sink.summary.rows[1].observation,
            Observation::Missing
        ));
        // Skipping an expected decision is detected, even if a later root exists.
        let mut skipped = ReportSink::new(
            &archive.config,
            archive.packages.each_ref(),
            &options,
            &archive,
        )
        .unwrap();
        skipped.expected[0].0.decision_index = 0;
        run(&archive, &mut skipped);
        assert!(!skipped.summary.usable_job);
        assert!(skipped.timings.is_empty());
    }
    #[test]
    fn v4_evaluation_report_package_normalization_is_runtime_only() {
        let (archive, options) = archive();
        let mut current = archive.packages.clone();
        current[0].runtime.tracked_tree_sha256 = "a".repeat(64);
        assert!(ReportSink::new(&archive.config, current.each_ref(), &options, &archive).is_ok());
        current[0].gameplay_sampler_identity = "different".into();
        assert_eq!(
            ReportSink::new(&archive.config, current.each_ref(), &options, &archive)
                .err()
                .unwrap(),
            "Report package differs beyond runtime"
        );
        let mut unordered = options.clone();
        unordered.roots.reverse();
        assert!(ReportSink::new(
            &archive.config,
            archive.packages.each_ref(),
            &unordered,
            &archive
        )
        .is_err());
        let mut altered = archive.clone();
        altered.config.seed ^= 1;
        assert!(ReportSink::new(
            &altered.config,
            archive.packages.each_ref(),
            &options,
            &archive
        )
        .is_err());
    }
    #[test]
    fn v4_evaluation_report_failed_engine_receipt_discards_observation() {
        let (archive, options) = archive();
        let mut sink = ReportSink::new(
            &archive.config,
            archive.packages.each_ref(),
            &options,
            &archive,
        )
        .unwrap();
        sink.store(
            0,
            Observation::Rejected {
                error: search::Error::InvalidBudget,
            },
        );
        let record = sink.expected[0].1.clone();
        let (mut policies, packages) = fixtures::fixtures_v4([PlayDrawChoiceV1::Play; 2]);
        let hashes = archive.trajectory.behavior_packages_by_seat.clone();
        let mut game = EvaluationGame {
            game_index: 1,
            start: None,
            environment_seed: None,
            decisions: vec![],
            terminal: None,
            discarded_pending_selections: 0,
        };
        let mut budget = EvaluationBudget {
            count: record.decision_index,
            bytes: 0,
            retained: 0,
            max_count: 10000,
            max_bytes: MAX_RECORD_BYTES,
            max_retained: 0,
            native_record_bytes: true,
        };
        let mut timings = Vec::new();
        let mut policy = EvaluationPolicy {
            policies: &mut policies,
            packages: packages.each_ref(),
            hashes: &hashes,
            game: &mut game,
            budget: &mut budget,
            timings: &mut timings,
            report: Some(&mut sink),
            activation: None,
            pending: Some(EvaluationPending {
                step: 0,
                record: EvaluationDecision::Ordinary { record },
                size: 1,
                report_row: Some(0),
            }),
            failure: None,
            attempted: None,
        };
        assert!(policy
            .finish_game(Err("engine receipt failed".into()), None)
            .is_err());
        assert_eq!(policy.game.discarded_pending_selections, 1);
        assert!(policy.game.decisions.is_empty());
        assert_eq!(sink.summary.rows[0].receipt, Receipt::Discarded);
        assert_eq!(sink.summary.rows[1].receipt, Receipt::Missing);
    }
    #[test]
    fn v4_evaluation_report_inputs_reject_nested_duplicates_and_unknown_fields() {
        let (archive, options) = archive();
        let text = serde_json::to_string(&archive).unwrap();
        assert!(Bo3ReportArchiveV1::from_json_v1(&text).is_ok());
        let duplicate = text.replacen("\"config\":{", "\"config\":{\"seed\":0,", 1);
        assert!(Bo3ReportArchiveV1::from_json_v1(&duplicate).is_err());
        // Strict parser catches duplicate arbitrary map keys, before typed decoding.
        assert!(
            Bo3ReportArchiveV1::from_json_v1(r#"{"extra":{"x":1,"x":2}}"#)
                .unwrap_err()
                .contains("duplicate")
        );
        let options_text = serde_json::to_string(&options).unwrap();
        assert!(Bo3ReportOptionsV1::from_json_v1(&options_text).is_ok());
        assert!(Bo3ReportOptionsV1::from_json_v1(&options_text.replacen(
            '{',
            "{\"unexpected\":1,",
            1
        ))
        .is_err());
        assert!(Bo3ReportOptionsV1::from_json_v1(&" ".repeat(32769)).is_err());
    }
}
