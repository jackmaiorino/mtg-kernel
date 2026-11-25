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
