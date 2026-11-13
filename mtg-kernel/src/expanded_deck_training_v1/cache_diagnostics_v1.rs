//! The opt-in cache diagnostics file (#595). When the environment names one
//! in `MTG_KERNEL_CACHE_DIAGNOSTICS_V1` (an absolute path), the native
//! training run (`native_expanded_training_run_v1`) appends one JSON line to
//! it after every iteration it completes or recovers: each counter family's
//! cumulative totals for this process, so a reader takes differences between
//! lines. Unset, nothing is written. No receipt, checkpoint, stdout line or
//! result depends on the variable; only this file does.
//!
//! A counter family adds its report as one more entry of [`sections_v1`].

use super::loaded_source_profile_v1;
use serde_json::{json, Value};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub(crate) const ENV_V1: &str = "MTG_KERNEL_CACHE_DIAGNOSTICS_V1";
pub(crate) const SCHEMA_V1: &str = "mtg-kernel-cache-diagnostics/v1";

/// Every counter family's report, by section name.
fn sections_v1() -> Vec<(&'static str, Value)> {
    vec![("loaded_source", loaded_source_profile_v1::report_v1())]
}

/// The file the environment names, read once per process.
fn named_path_v1() -> Option<&'static Path> {
    static PATH: OnceLock<Option<PathBuf>> = OnceLock::new();
    PATH.get_or_init(|| std::env::var_os(ENV_V1).map(PathBuf::from))
        .as_deref()
}

/// One line after `iteration`, which this invocation completed
/// (`newly_completed`) or recovered from its receipt, if a file is named.
pub(crate) fn append_iteration_v1(iteration: usize, newly_completed: bool) -> Result<(), String> {
    match named_path_v1() {
        None => Ok(()),
        Some(path) => append_line_v1(path, iteration, newly_completed),
    }
}

fn append_line_v1(path: &Path, iteration: usize, newly_completed: bool) -> Result<(), String> {
    if !path.is_absolute() {
        return Err(format!("{ENV_V1} must name an absolute path"));
    }
    let mut line = json!({"schema": SCHEMA_V1, "completed_iteration": iteration,
        "newly_completed": newly_completed, "process_id": std::process::id()});
    for (name, section) in sections_v1() {
        line[name] = section;
    }
    let mut text = serde_json::to_string(&line).map_err(|error| error.to_string())?;
    text.push('\n');
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut file| file.write_all(text.as_bytes()))
        .map_err(|error| format!("cache diagnostics {}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use loaded_source_profile_v1::{record_lookup_v1, scoped_v1, LoadRoleV1};

    #[test]
    fn lines_append_cumulative_counters_and_refuse_a_relative_path() {
        loaded_source_profile_v1::reset_for_test_v1();
        let directory = std::env::temp_dir().join(format!(
            "cache-diagnostics-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("diagnostics.jsonl");
        scoped_v1(LoadRoleV1::UpdateValidation, record_lookup_v1);
        append_line_v1(&path, 0, true).unwrap();
        scoped_v1(LoadRoleV1::UpdateValidation, record_lookup_v1);
        append_line_v1(&path, 1, false).unwrap();
        let lines: Vec<Value> = std::fs::read_to_string(&path)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(lines.len(), 2);
        for (index, line) in lines.iter().enumerate() {
            assert_eq!(line["schema"], SCHEMA_V1);
            assert_eq!(line["completed_iteration"], index);
            assert_eq!(line["newly_completed"], index == 0);
            assert_eq!(line["process_id"], std::process::id());
            assert_eq!(
                line["loaded_source"]["roles"]["update_validation"]["cache_lookups"],
                index + 1,
                "each line holds the totals so far"
            );
        }
        assert!(append_line_v1(Path::new("relative.jsonl"), 0, true)
            .unwrap_err()
            .contains("absolute path"));
        std::fs::remove_dir_all(directory).unwrap();
    }
}
