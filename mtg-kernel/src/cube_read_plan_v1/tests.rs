use super::*;

fn execution_entry(execution: u64, reason: ExecutionReason) -> ManifestExecutionEntry {
    ManifestExecutionEntry {
        execution,
        reason,
        expired_games: Vec::new(),
        binary: "b".repeat(64),
        platform: "test-platform".to_owned(),
        claim_id: None,
    }
}

fn d8_manifest(namespace: &str, run: &str, set: &str, kind: ManifestKind) -> SetManifest {
    let units = if namespace == "dose-d8" {
        dose_panel_range(run).unwrap()
    } else {
        let n = d8_run_number(run).unwrap();
        [4 * (n - 1), 4 * n]
    };
    SetManifest {
        schema: READ_PLAN_SCHEMA.to_owned(),
        kind,
        family: if namespace == "dose-d8" { "dose" } else { "aa" }.to_owned(),
        namespace: namespace.to_owned(),
        run: run.to_owned(),
        set: set.to_owned(),
        interfaces: M4_INTERFACES_VERSION.to_owned(),
        declaration_sha256: "a".repeat(64),
        look: None,
        units,
        reserved: Some(units),
        pools_with: None,
        panel_sha256: (namespace == "dose-d8" && set != "prep-panel").then(|| "c".repeat(64)),
        prep_set: (namespace == "dose-d8" && kind == ManifestKind::Plans)
            .then(|| format!("dose-d8/{run}/prep-dose-0/exec-0")),
        replaces: None,
        retired: Vec::new(),
        plans: Vec::new(),
        plan_set_sha256: compute_plan_set_sha256(&[]).unwrap(),
        executions: Vec::new(),
    }
}

#[test]
fn live_prep_claim_is_the_sixth_execution_key() {
    let mut manifest = d8_manifest("dose-d8", "v1", "prep-panel", ManifestKind::Prep);
    manifest
        .executions
        .push(execution_entry(0, ExecutionReason::First));
    let unclaimed = to_canonical_compact_json(&manifest).unwrap();
    assert!(parse_and_validate_manifest_json(&unclaimed).is_err());

    manifest.executions[0].claim_id = Some("claim-0".to_owned());
    let claimed = to_canonical_compact_json(&manifest).unwrap();
    assert!(claimed.ends_with(r#""platform":"test-platform","claim_id":"claim-0"}]}"#));
    assert_eq!(
        parse_and_validate_manifest_json(&claimed)
            .unwrap()
            .executions[0]
            .claim_id
            .as_deref(),
        Some("claim-0")
    );
    let reordered = claimed.replace(
        r#""platform":"test-platform","claim_id":"claim-0""#,
        r#""claim_id":"claim-0","platform":"test-platform""#,
    );
    assert!(parse_and_validate_manifest_json(&reordered).is_err());
    let empty = claimed.replace("claim-0", " ");
    assert!(parse_and_validate_manifest_json(&empty).is_err());

    let mut played = d8_manifest("read-d8-aa", "v1", "fixed", ManifestKind::Plans);
    played.executions = manifest.executions;
    assert!(
        parse_and_validate_manifest_json(&to_canonical_compact_json(&played).unwrap()).is_err()
    );
}

struct HistoryDir(PathBuf);

impl HistoryDir {
    fn new(label: &str) -> Self {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("d5-history-{label}-{}-{stamp}", std::process::id()));
        fs::create_dir(&root).unwrap();
        Self(root)
    }

    fn set(&self, manifest: &SetManifest) -> PathBuf {
        let path = self
            .0
            .join(&manifest.namespace)
            .join(&manifest.run)
            .join(&manifest.set);
        put(
            &path.join("manifest.json"),
            to_canonical_pretty_json(manifest).unwrap(),
        );
        path
    }
}

impl Drop for HistoryDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn put(path: &Path, bytes: impl AsRef<[u8]>) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

fn add_pod(manifest: &mut SetManifest, pod_index: u64) {
    manifest.plans.push(ManifestEntry::Pod(ManifestPodEntry {
        pod_index,
        file: format!("plans/pod-{pod_index}.json"),
        sha256: "d".repeat(64),
    }));
    manifest.plan_set_sha256 = compute_plan_set_sha256(&manifest.plans).unwrap();
}

fn write_history_receipt(
    set_dir: &Path,
    manifest: &SetManifest,
    execution: u64,
    ended: PlanExecutionEnded,
    watchdog_game: Option<u32>,
) {
    let dir = set_dir.join(format!("exec-{execution}"));
    let entry = &manifest.executions[execution as usize];
    let games: Vec<_> = watchdog_game
        .into_iter()
        .map(|game| PodGameRecord {
            game,
            group: game,
            role: GameRole::Cross,
            pairing: None,
            decks: [0, 1],
            chairs: [Some(0), Some(1)],
            seat_rule: SeatRule::Schedule,
            game_seed: 2,
            episode: u64::from(game),
            attempts: vec![AttemptRecord {
                attempt: 0,
                deal_seed: 2,
                env_seed: 3,
                action_seed: 4,
                decisions: 0,
                physical_decisions: None,
                policy_steps: None,
                end: crate::cube_pod_runner_v1::watchdog_game_end(),
                halted: true,
                stop_kind: Some(WATCHDOG_STOP_KIND.to_owned()),
                last_action_source: None,
            }],
            scores: None,
        })
        .collect();
    let (halts, status, stop) =
        collect_halts_and_stop(&games, ended == PlanExecutionEnded::WatchdogStop);
    let pod_index = match &manifest.plans[0] {
        ManifestEntry::Pod(entry) => entry.pod_index,
        ManifestEntry::Unit(_) => panic!("expected played fixture"),
    };
    let record = PodRecord {
        schema: POD_RECORD_SCHEMA,
        interfaces: M4_INTERFACES_VERSION,
        pod_use: if manifest.family == "aa" {
            PodUse::ReadA
        } else {
            PodUse::Dose
        },
        namespace: manifest.namespace.clone(),
        run: manifest.run.clone(),
        pod_index,
        seeds: PodSeeds::for_read(1, 2, None, &manifest.namespace, &manifest.namespace),
        pins: PodPins::default(),
        pilot: PilotInfo::uniform(),
        caps: PodCaps::read_a2(),
        draft: None,
        chairs: Vec::new(),
        decks: Vec::new(),
        games,
        paired_builds: Vec::new(),
        halts,
        status,
        stop,
    }
    .to_json_line();
    validate_pod_record_json(&record).unwrap();
    put(&dir.join(format!("records/pod-{pod_index}.json")), &record);
    let receipt = PlanExecutionReceipt {
        execution,
        plan_set_sha256: manifest.plan_set_sha256.clone(),
        binary: entry.binary.clone(),
        platform: entry.platform.clone(),
        ended,
        records: vec![ExecutionRecordEntry {
            pod_index,
            sha256: sha256_hex(record.as_bytes()),
            status,
        }],
    };
    put(
        &dir.join("execution.json"),
        to_canonical_pretty_json(&receipt).unwrap(),
    );
}

#[test]
fn d8_history_retires_only_unstarted_units_across_all_executions() {
    let root = HistoryDir::new("started");
    let mut prior = d8_manifest("read-d8-aa", "v1", "fixed", ManifestKind::Plans);
    add_pod(&mut prior, 0);
    prior
        .executions
        .push(execution_entry(0, ExecutionReason::First));
    let prior_dir = root.set(&prior);
    write_history_receipt(&prior_dir, &prior, 0, PlanExecutionEnded::Interrupted, None);
    put(
        &prior_dir.join("exec-1/execution.json"),
        r#"{"records":[{"pod_index":4}]}"#,
    );
    put(&prior_dir.join("exec-1/timing/pod-6.jsonl"), "");
    put(
        &prior_dir.join(".supervisor-exec-2/pod-3.journal.jsonl"),
        "",
    );
    assert_eq!(
        d8_started_units(&prior_dir, &prior).unwrap(),
        BTreeSet::from([0, 1, 2, 3])
    );

    let mut next = d8_manifest("read-d8-aa", "v2", "fixed-restart-1", ManifestKind::Plans);
    next.replaces = Some("v1/fixed".to_owned());
    verify_d8_history(&root.set(&next), &next).unwrap();
    next.retired = vec![[1, 4]];
    assert!(verify_d8_history(&root.set(&next), &next)
        .unwrap_err()
        .to_string()
        .contains("unused units"));
}

#[test]
fn d8_history_distinguishes_same_game_hangs_and_different_game_voids() {
    let root = HistoryDir::new("watchdog");
    let mut prior = d8_manifest("read-d8-aa", "v1", "fixed", ManifestKind::Plans);
    add_pod(&mut prior, 0);
    prior
        .executions
        .push(execution_entry(0, ExecutionReason::First));
    let mut rerun = execution_entry(1, ExecutionReason::WatchdogRerun);
    rerun.expired_games = vec![[0, 0]];
    prior.executions.push(rerun);
    let prior_dir = root.set(&prior);
    write_history_receipt(&prior_dir, &prior, 0, PlanExecutionEnded::Complete, Some(0));

    let mut next = d8_manifest("read-d8-aa", "v2", "fixed-restart-1", ManifestKind::Plans);
    next.replaces = Some("v1/fixed".to_owned());
    next.retired = vec![[1, 4]];
    let next_dir = root.set(&next);

    // A started rerun that has no receipt is an interruption.
    verify_d8_history(&next_dir, &next).unwrap();
    write_history_receipt(
        &prior_dir,
        &prior,
        1,
        PlanExecutionEnded::WatchdogStop,
        Some(0),
    );
    verify_d8_history(&next_dir, &next).unwrap();
    write_history_receipt(
        &prior_dir,
        &prior,
        1,
        PlanExecutionEnded::WatchdogStop,
        Some(1),
    );
    assert!(verify_d8_history(&next_dir, &next)
        .unwrap_err()
        .to_string()
        .contains("not a restartable"));
    write_history_receipt(&prior_dir, &prior, 1, PlanExecutionEnded::Complete, None);
    assert!(verify_d8_history(&next_dir, &next).is_err());
}

#[test]
fn d8_history_uses_valid_markers_for_interrupted_prep() {
    let root = HistoryDir::new("prep");
    let mut prior = d8_manifest("dose-d8", "v1", "prep-dose-0", ManifestKind::Prep);
    let mut first = execution_entry(0, ExecutionReason::First);
    first.claim_id = Some("first-claim".to_owned());
    prior.executions.push(first);
    let prior_dir = root.set(&prior);
    let data = b"completed unit";
    let digest = sha256_hex(data);
    put(&prior_dir.join("exec-0/prep/unit-0.json"), data);
    let marker = PrepMarker {
        unit: 0,
        files: [PrepMarkerFile {
            file: prep_output_name(0),
            sha256: &digest,
        }],
    };
    put(
        &prior_dir.join("exec-0/done/0.json"),
        to_canonical_pretty_json(&marker).unwrap(),
    );
    put(
        &prior_dir.join("exec-0/prep/unit-1.json"),
        b"unmarked partial",
    );
    let mut next = d8_manifest("dose-d8", "v2", "prep-dose-0-restart-1", ManifestKind::Prep);
    next.replaces = Some("v1/prep-dose-0".to_owned());
    next.retired = vec![[1, 32]];
    verify_d8_history(&root.set(&next), &next).unwrap();
}
