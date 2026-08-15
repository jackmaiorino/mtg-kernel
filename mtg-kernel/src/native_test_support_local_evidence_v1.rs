//! Shared `cfg(test)` helper for gating tests that read machine-local
//! sealed evidence: real, already-published run/store artifacts under
//! absolute `D:\` / `C:\` paths that exist on the science host but not on
//! hosted CI runners. Several `native_*` modules' tests read this evidence
//! directly (real S1 mirror checkpoints, real ladder pilot pools, real
//! published run records); before this module, those tests panicked with a
//! raw `NotFound` on any machine that lacks the evidence tree.
//!
//! [`require_local_evidence_v1`] is the single check every such test calls,
//! before any read of its primary evidence root path. On the science host,
//! set `MTG_KERNEL_REQUIRE_LOCAL_EVIDENCE_V1` (to any value) to make
//! absence a hard failure instead of a skip: this is the local merge
//! protocol, so a real evidence-tree regression on the machine that is
//! supposed to have it still fails closed instead of silently skipping.

#[cfg(test)]
use std::path::Path;

/// The real environment variable every call site reads. Kept as a named
/// constant so [`require_local_evidence_v1`] and this module's own doc
/// comments cannot drift from each other.
#[cfg(test)]
const REQUIRE_LOCAL_EVIDENCE_ENV_VAR_V1: &str = "MTG_KERNEL_REQUIRE_LOCAL_EVIDENCE_V1";

/// Returns `true` when `path` exists on this machine.
///
/// Returns `false` (after printing an explicit `SKIP-LOCAL-EVIDENCE-ABSENT`
/// marker to stderr) when `path` is absent AND
/// `MTG_KERNEL_REQUIRE_LOCAL_EVIDENCE_V1` is not set, so hosted CI runners
/// skip the dependent test cleanly instead of panicking with `NotFound`.
///
/// Panics, naming `path`, when `path` is absent AND
/// `MTG_KERNEL_REQUIRE_LOCAL_EVIDENCE_V1` IS set: the science host's local
/// merge protocol, so a genuine evidence-tree regression there still fails
/// closed instead of skipping.
#[cfg(test)]
pub(crate) fn require_local_evidence_v1(path: &Path) -> bool {
    require_local_evidence_with_named_env_var_v1(path, REQUIRE_LOCAL_EVIDENCE_ENV_VAR_V1)
}

/// The actual check, parameterized over the env var name. Exists only so
/// this module's own unit tests can exercise both branches (skip and
/// strict-mode panic) without ever mutating the real
/// `MTG_KERNEL_REQUIRE_LOCAL_EVIDENCE_V1` process environment variable: this
/// crate's tests all run inside one shared test binary process, and every
/// gated Class A test elsewhere in the crate reads that same real variable
/// concurrently under cargo's default parallel-thread test execution. On
/// CI, where their evidence is legitimately absent, a test here that
/// mutated the real variable global could race one of them into a spurious
/// strict-mode panic instead of its intended skip. Using a distinct,
/// private probe variable name in this module's own tests exercises the
/// identical logic with zero blast radius.
#[cfg(test)]
fn require_local_evidence_with_named_env_var_v1(path: &Path, env_var_name: &str) -> bool {
    if path.exists() {
        return true;
    }
    if std::env::var_os(env_var_name).is_some() {
        panic!(
            "{env_var_name} is set but required machine-local sealed evidence is absent: {}",
            path.display()
        );
    }
    eprintln!("SKIP-LOCAL-EVIDENCE-ABSENT {}", path.display());
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_existing_path_is_reported_present_and_never_skipped() {
        let existing = std::env::current_exe().expect("current test binary path must resolve");
        assert!(require_local_evidence_v1(&existing));
    }

    #[test]
    fn a_nonexistent_temp_path_skips_without_panicking_when_the_env_var_is_unset() {
        let missing = std::env::temp_dir()
            .join("mtg-kernel-nonexistent-evidence-probe-v1")
            .join("absent-file-should-never-exist-v1");
        assert!(!missing.exists(), "probe path must genuinely not exist");
        assert!(!require_local_evidence_with_named_env_var_v1(
            &missing,
            "MTG_KERNEL_REQUIRE_LOCAL_EVIDENCE_V1_UNIT_TEST_UNSET_PROBE_V1",
        ));
    }

    #[test]
    #[should_panic(expected = "is set but required machine-local sealed evidence is absent")]
    fn a_nonexistent_temp_path_panics_in_strict_mode_when_the_env_var_is_set() {
        let missing = std::env::temp_dir()
            .join("mtg-kernel-nonexistent-evidence-probe-v1")
            .join("absent-file-should-never-exist-v1");
        assert!(!missing.exists(), "probe path must genuinely not exist");
        let probe_var = "MTG_KERNEL_REQUIRE_LOCAL_EVIDENCE_V1_UNIT_TEST_STRICT_PROBE_V1";
        // SAFETY: `probe_var` is a private, test-only name distinct from the
        // real MTG_KERNEL_REQUIRE_LOCAL_EVIDENCE_V1 (see the module-level
        // doc on `require_local_evidence_with_named_env_var_v1`); no other
        // test in this process reads or writes it, and this test does not
        // spawn threads, so there is no concurrent access to race.
        unsafe {
            std::env::set_var(probe_var, "1");
        }
        require_local_evidence_with_named_env_var_v1(&missing, probe_var);
    }
}
