//! Process-level checks of the `spellbench_scorer_v1` binary that need no
//! model files: exit status, the stdout error record and the stderr cause.

use serde_json::{json, Value};
use std::process::{Command, Stdio};

const SCORER: &str = env!("CARGO_BIN_EXE_spellbench_scorer_v1");

#[test]
fn a_missing_config_exits_nonzero_with_one_error_record_and_its_cause() {
    let missing = std::env::temp_dir().join(format!(
        "mtg-kernel-spellbench-scorer-v1-missing-{}.json",
        std::process::id()
    ));
    let output = Command::new(SCORER)
        .arg("--config")
        .arg(&missing)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let records: Vec<Value> = output
        .stdout
        .split(|b| *b == b'\n')
        .filter(|l| !l.is_empty())
        .map(|l| serde_json::from_slice(l).unwrap())
        .collect();
    assert_eq!(
        records,
        vec![json!({
            "schema": "mtg-kernel-spellbench-scorer-error/v1",
            "code": "config_open",
            "service_continues": false,
        })]
    );
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr
            .lines()
            .next()
            .is_some_and(|line| line.starts_with("spellbench scorer: config_open: ")),
        "{stderr}"
    );
}
