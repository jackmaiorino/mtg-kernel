//! Launch-ticket gate for the `multirun_pilot_v1` harness (COMPUTE-POLICY
//! item 5: supported training launchers reject missing throughput evidence
//! before spawning the workload).
//!
//! `python/tools/multirun_launcher_v1.py` validates the compute-choice
//! receipt (serial versus parallel throughput, byte-identity to the serial
//! goldens, fresh placement inventory) and then hands each process a ticket
//! through `MULTIRUN_LAUNCH_TICKET`. The harness cannot re-validate the
//! receipt, but it refuses a substantial run whose ticket is missing or
//! names another executable, seed, length, stop point or run count, so a raw
//! invocation cannot silently stand in for a qualified launch. Small
//! correctness and timing checks (at most
//! `PILOT_UNGUARDED_TOTAL_UPDATES_V1` planned updates across all runs of
//! the process) stay ticket-free. Like the Python check, this is a launch
//! check, not a security boundary against a deliberately forged ticket.

use std::path::Path;

use serde::Deserialize;

use crate::native_training_store_digest_v1::{lower_hex_raw32_v1, sha256_v1};

/// Planned updates (run count times record length) a process may train
/// without a ticket: 64 updates of 64 episodes, a few minutes of compute.
pub(crate) const PILOT_UNGUARDED_TOTAL_UPDATES_V1: u64 = 64;
/// Longest prefix a qualification ticket may license.
pub(crate) const QUALIFICATION_PREFIX_LIMIT_V1: u64 = 32;
pub(crate) const LAUNCH_TICKET_SCHEMA_V1: &str = "mtg-kernel-multirun-launch-ticket/v1";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LaunchTicketV1 {
    schema: String,
    kind: String,
    executable_sha256: String,
    compute_choice_sha256: Option<String>,
    workload_sha256: String,
    run_id: String,
    seed: u64,
    planned_updates: u64,
    stop_after_generation: Option<u64>,
    allocation: String,
    issued_at: String,
}

/// What this harness process is about to train.
pub(crate) struct PilotLaunchV1 {
    pub(crate) run_count: u64,
    pub(crate) updates: u64,
    pub(crate) base_seed: u64,
    pub(crate) seed_offset: u64,
    pub(crate) stop_after_generation: Option<u64>,
    pub(crate) expected_resume_generation: Option<u64>,
}

/// Ok when the launch is small or carries a ticket that matches it exactly.
pub(crate) fn require_pilot_launch_ticket_v1(
    launch: &PilotLaunchV1,
    ticket_path: Option<&Path>,
    executable: &Path,
) -> Result<(), String> {
    let total = launch.run_count.saturating_mul(launch.updates);
    if total <= PILOT_UNGUARDED_TOTAL_UPDATES_V1 {
        return Ok(());
    }
    let refuse = |reason: &str| {
        Err(format!(
            "multirun-launch-ticket-refused: {reason}; {total} planned updates exceed the \
             {PILOT_UNGUARDED_TOTAL_UPDATES_V1}-update small-check limit, so launch through \
             python/tools/multirun_launcher_v1.py (COMPUTE-POLICY items 2-5)"
        ))
    };
    let Some(ticket_path) = ticket_path else {
        return refuse("MULTIRUN_LAUNCH_TICKET is not set");
    };
    let Ok(bytes) = std::fs::read(ticket_path) else {
        return refuse("the launch ticket is unreadable");
    };
    let Ok(ticket) = serde_json::from_slice::<LaunchTicketV1>(&bytes) else {
        return refuse("the launch ticket does not decode");
    };
    if ticket.schema != LAUNCH_TICKET_SCHEMA_V1 {
        return refuse("unsupported launch ticket schema");
    }
    if ticket.run_id.is_empty() || ticket.allocation.is_empty() || ticket.issued_at.is_empty() {
        return refuse("the launch ticket is incomplete");
    }
    if !is_lower_hex_64(&ticket.workload_sha256) {
        return refuse("the launch ticket names no workload");
    }
    let Ok(executable_bytes) = std::fs::read(executable) else {
        return refuse("this executable cannot be hashed");
    };
    if lower_hex_raw32_v1(sha256_v1(&executable_bytes)) != ticket.executable_sha256 {
        return refuse("the ticket was issued for another executable");
    }
    if launch.run_count != 1 {
        return refuse("the launcher runs exactly one run per process");
    }
    if launch.base_seed.checked_add(launch.seed_offset) != Some(ticket.seed) {
        return refuse("the ticket was issued for another seed");
    }
    if launch.updates != ticket.planned_updates {
        return refuse("the ticket was issued for another run length");
    }
    if launch.expected_resume_generation.is_some() {
        return refuse("resumed segments are not a qualified launch shape yet");
    }
    if launch.stop_after_generation != ticket.stop_after_generation {
        return refuse("the ticket was issued for another stop generation");
    }
    match ticket.kind.as_str() {
        "launch" => {
            let evidence = ticket.compute_choice_sha256.as_deref().unwrap_or("");
            if !is_lower_hex_64(evidence) {
                return refuse("a launch ticket must bind its compute-choice receipt");
            }
            if ticket.stop_after_generation.is_some() {
                return refuse("a launch ticket runs the whole planned length");
            }
        }
        "qualification" => {
            if ticket.compute_choice_sha256.is_some() {
                return refuse("a qualification ticket precedes any receipt");
            }
            match ticket.stop_after_generation {
                Some(prefix) if (1..=QUALIFICATION_PREFIX_LIMIT_V1).contains(&prefix) => {}
                _ => return refuse("a qualification ticket must stop within the prefix limit"),
            }
        }
        _ => return refuse("unknown launch ticket kind"),
    }
    Ok(())
}

fn is_lower_hex_64(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        directory: std::path::PathBuf,
        executable: std::path::PathBuf,
    }

    impl Fixture {
        fn new(name: &str) -> Self {
            let directory = std::env::temp_dir().join(format!(
                "multirun-launch-ticket-v1-{name}-{}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&directory);
            std::fs::create_dir_all(&directory).unwrap();
            let executable = directory.join("trainer.exe");
            std::fs::write(&executable, b"stand-in executable bytes").unwrap();
            Self {
                directory,
                executable,
            }
        }

        fn executable_sha256(&self) -> String {
            lower_hex_raw32_v1(sha256_v1(&std::fs::read(&self.executable).unwrap()))
        }

        fn ticket(&self, edit: impl FnOnce(&mut serde_json::Value)) -> std::path::PathBuf {
            let mut ticket = serde_json::json!({
                "schema": LAUNCH_TICKET_SCHEMA_V1,
                "kind": "launch",
                "executable_sha256": self.executable_sha256(),
                "compute_choice_sha256": "a".repeat(64),
                "workload_sha256": "b".repeat(64),
                "run_id": "control-s0",
                "seed": 424_242,
                "planned_updates": 256,
                "stop_after_generation": null,
                "allocation": "3@jack:0+1@jack:1",
                "issued_at": "2026-09-23T00:00:00+00:00",
            });
            edit(&mut ticket);
            let path = self.directory.join("ticket.json");
            std::fs::write(&path, serde_json::to_vec(&ticket).unwrap()).unwrap();
            path
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.directory);
        }
    }

    fn launch() -> PilotLaunchV1 {
        PilotLaunchV1 {
            run_count: 1,
            updates: 256,
            base_seed: 424_242,
            seed_offset: 0,
            stop_after_generation: None,
            expected_resume_generation: None,
        }
    }

    fn refused(result: Result<(), String>, fragment: &str) {
        let reason = result.expect_err("launch must be refused");
        assert!(reason.contains(fragment), "{reason}");
        assert!(
            reason.starts_with("multirun-launch-ticket-refused: "),
            "{reason}"
        );
    }

    #[test]
    fn small_checks_need_no_ticket_and_substantial_runs_do() {
        let fixture = Fixture::new("small");
        let small = PilotLaunchV1 {
            run_count: 4,
            updates: 16,
            ..launch()
        };
        assert_eq!(
            require_pilot_launch_ticket_v1(&small, None, &fixture.executable),
            Ok(())
        );
        refused(
            require_pilot_launch_ticket_v1(&launch(), None, &fixture.executable),
            "MULTIRUN_LAUNCH_TICKET is not set",
        );
        // A long record stopped early is still measured by its record length:
        // splitting a campaign into short stops does not escape the ticket.
        let stopped = PilotLaunchV1 {
            stop_after_generation: Some(4),
            ..launch()
        };
        refused(
            require_pilot_launch_ticket_v1(&stopped, None, &fixture.executable),
            "not set",
        );
    }

    #[test]
    fn a_matching_launch_ticket_is_accepted() {
        let fixture = Fixture::new("accept");
        let ticket = fixture.ticket(|_| {});
        assert_eq!(
            require_pilot_launch_ticket_v1(&launch(), Some(&ticket), &fixture.executable),
            Ok(())
        );
    }

    #[test]
    fn a_ticket_for_anything_else_is_refused() {
        let fixture = Fixture::new("mismatch");
        let cases: Vec<(Box<dyn Fn(&mut serde_json::Value)>, &str)> = vec![
            (
                Box::new(|t| t["executable_sha256"] = "0".repeat(64).into()),
                "another executable",
            ),
            (Box::new(|t| t["seed"] = 1.into()), "another seed"),
            (
                Box::new(|t| t["planned_updates"] = 128.into()),
                "another run length",
            ),
            (
                Box::new(|t| t["stop_after_generation"] = 4.into()),
                "another stop generation",
            ),
            (
                Box::new(|t| t["compute_choice_sha256"] = serde_json::Value::Null),
                "bind its compute-choice receipt",
            ),
            (Box::new(|t| t["schema"] = "other/v1".into()), "schema"),
            (Box::new(|t| t["kind"] = "other".into()), "unknown"),
            (Box::new(|t| t["extra"] = true.into()), "does not decode"),
        ];
        for (edit, fragment) in cases {
            let ticket = fixture.ticket(edit);
            refused(
                require_pilot_launch_ticket_v1(&launch(), Some(&ticket), &fixture.executable),
                fragment,
            );
        }
        let ticket = fixture.ticket(|_| {});
        let two_runs = PilotLaunchV1 {
            run_count: 2,
            ..launch()
        };
        refused(
            require_pilot_launch_ticket_v1(&two_runs, Some(&ticket), &fixture.executable),
            "one run per process",
        );
        let resumed = PilotLaunchV1 {
            expected_resume_generation: Some(128),
            ..launch()
        };
        refused(
            require_pilot_launch_ticket_v1(&resumed, Some(&ticket), &fixture.executable),
            "resumed segments",
        );
        refused(
            require_pilot_launch_ticket_v1(
                &launch(),
                Some(&fixture.directory.join("absent.json")),
                &fixture.executable,
            ),
            "unreadable",
        );
    }

    #[test]
    fn qualification_tickets_license_only_a_short_prefix() {
        let fixture = Fixture::new("qualification");
        let prefix = PilotLaunchV1 {
            stop_after_generation: Some(4),
            ..launch()
        };
        let ticket = fixture.ticket(|t| {
            t["kind"] = "qualification".into();
            t["compute_choice_sha256"] = serde_json::Value::Null;
            t["stop_after_generation"] = 4.into();
        });
        assert_eq!(
            require_pilot_launch_ticket_v1(&prefix, Some(&ticket), &fixture.executable),
            Ok(())
        );
        let long = PilotLaunchV1 {
            stop_after_generation: Some(QUALIFICATION_PREFIX_LIMIT_V1 + 1),
            ..launch()
        };
        let ticket = fixture.ticket(|t| {
            t["kind"] = "qualification".into();
            t["compute_choice_sha256"] = serde_json::Value::Null;
            t["stop_after_generation"] = (QUALIFICATION_PREFIX_LIMIT_V1 + 1).into();
        });
        refused(
            require_pilot_launch_ticket_v1(&long, Some(&ticket), &fixture.executable),
            "prefix limit",
        );
        let unstopped = fixture.ticket(|t| {
            t["kind"] = "qualification".into();
            t["compute_choice_sha256"] = serde_json::Value::Null;
        });
        refused(
            require_pilot_launch_ticket_v1(&launch(), Some(&unstopped), &fixture.executable),
            "prefix limit",
        );
    }
}
