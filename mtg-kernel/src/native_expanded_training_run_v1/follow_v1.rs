//! Followed runs (`native_expanded_training_run_v1 CONFIG --follow SPOOL`):
//! one process runs consecutive segment configs of a chain.
//!
//! A chain is a sequence of runs, each from the previous run's last
//! checkpoint. A fresh process per run pays its start every time (exec, a
//! full parse and restore of the checkpoint, opponent validation, thread
//! start and a cold working set). A followed process runs the first config as
//! a fresh process does, then waits for the driver's next config and runs it
//! through the same entry point, `run_native_expanded_training_v1`, so every
//! check, publication, record and refusal of a run is the one a fresh process
//! makes. The process-wide caches it keeps (loaded sources, opponents,
//! verified bytes, serialization lengths, tensorizer prefixes) hold exact
//! duplicates of bytes and parses their own contracts verify, and in the
//! default (reread) mode every cache hit rereads and checks its files as a
//! full load does, so no state, record byte or refusal depends on whether a
//! run is followed; the one record difference is the telemetry counter
//! `update_preparation.opponent_cache_hits`. Resident reads
//! (`verified_reads: "resident"`) skip rereads of bytes the process already
//! verified, so a fresh process's boundary rereads would be lost: a resident
//! run is never continued. The mode is a command-line flag only: no config
//! key, plan or receipt names it.
//!
//! The spool protocol (a directory the driver creates empty for each
//! process):
//! - after each successful run the process writes its result line to stdout,
//!   as a fresh process does; after a complete run it then publishes
//!   `done-NNNNNN.json` (NNNNNN: the run's index in this process, from 0),
//!   once the run has returned and released its lock;
//! - it then waits for `STOP` (ends the process with exit 0), its parent's
//!   end (Unix: ends the process with exit 0, before any queued config is
//!   admitted) or `next.json` (the next config, published whole by a rename);
//!   `STOP` wins when both files are present; while a run is in flight
//!   neither is looked at, as a fresh process does not look either;
//! - `next.json` is read, parsed and removed, then admitted only if it
//!   continues the run just finished (`check_continuation_v1`); anything
//!   else (a next.json that cannot be read, parsed or removed included, which
//!   stays where it is) ends the process with `REFUSED_EXIT_CODE` before any
//!   of its work, so the driver can run that config in a fresh process.
//!
//! An incomplete run (a chain ledger stop) or a failed run ends the process
//! as it ends a fresh one, without a done record.

use super::{
    check, err, publish, run_native_expanded_training_v1, NativeExpandedTrainingRunV1,
    VerifiedReadsV1, SMALL_CAP,
};
use serde_json::{json, Value};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// The driver's request to end the process between runs.
pub const STOP_NAME: &str = "STOP";
/// The driver's next config, published whole (written aside, then renamed).
pub const NEXT_NAME: &str = "next.json";
/// Exit status of a process that refused a next config before any of its work.
pub const REFUSED_EXIT_CODE: i32 = 3;
const DONE_SCHEMA: &str = "mtg-kernel-native-expanded-follow-done/v1";
/// How often a waiting process looks at the spool.
const SPOOL_POLL: Duration = Duration::from_millis(20);

/// The name of the record published after the `index`-th run of a process.
pub fn done_name_v1(index: usize) -> String {
    format!("done-{index:06}.json")
}

/// What the driver asked for between two runs.
pub enum FollowNextV1 {
    /// The next segment config.
    Config(Box<NativeExpandedTrainingRunV1>),
    /// A next.json that is not a config (unreadable, not a file, too large or
    /// unparsable): refused like a config that is not a continuation.
    Unusable(String),
    /// End the process.
    Stop,
    /// The driver is gone (its process ended): end the process.
    Orphaned,
}

/// How a followed process ended without an error.
#[derive(Debug, PartialEq, Eq)]
pub enum FollowEndV1 {
    /// `STOP` after a complete run.
    Stopped,
    /// The driver ended while the process waited.
    Orphaned,
    /// A run ended without completing (a chain ledger stop), as a fresh
    /// process ends.
    Incomplete,
}

/// Why a followed process ended with an error.
#[derive(Debug)]
pub enum FollowErrorV1 {
    /// A run failed, as it fails in a fresh process.
    Run(String),
    /// The next config is not a continuation; nothing of it ran.
    Refused(String),
}

/// Admits `next` after the complete run of `previous` that returned
/// `previous_result` only if it continues that run: every field equal to
/// `previous`'s in its serialized bytes (so float bits count, `-0.0` is not
/// `0.0`, except in a field whose serialization omits a default zero of
/// either sign, such as `max_non_natural_episode_fraction`) except `output_directory` (which must differ), `iterations` and
/// `initial_source.checkpoint`, and that checkpoint the pin (path and
/// SHA-256) of the run's last output checkpoint, which is the check the chain
/// driver makes before it builds the next config (`chains.next_source`). A
/// run with resident reads is never continued (module documentation).
pub fn check_continuation_v1(
    previous: &NativeExpandedTrainingRunV1,
    previous_result: &Value,
    next: &NativeExpandedTrainingRunV1,
) -> Result<(), String> {
    check(
        previous_result["complete"] == true,
        "the previous run is incomplete",
    )?;
    check(
        previous.verified_reads == VerifiedReadsV1::Reread,
        "a run with resident reads is not continued in its process",
    )?;
    check(
        next.output_directory != previous.output_directory,
        "the next config reuses the previous run directory",
    )?;
    let mut expected = previous.clone();
    expected.output_directory = next.output_directory.clone();
    expected.iterations = next.iterations.clone();
    expected.initial_source.checkpoint = next.initial_source.checkpoint.clone();
    check(
        serde_json::to_vec(&expected).map_err(err)? == serde_json::to_vec(next).map_err(err)?,
        "the next config changes a field other than output_directory, iterations and the initial checkpoint",
    )?;
    let checkpoint = next
        .initial_source
        .checkpoint
        .as_ref()
        .ok_or("the next config has no initial checkpoint")?;
    check(
        serde_json::to_value(checkpoint).map_err(err)? == previous_result["source"]["checkpoint"],
        "the next initial checkpoint is not the previous run's last output checkpoint",
    )?;
    check(
        previous_result["actual_identity"]["checkpoint_sha256"] == checkpoint.sha256.as_str(),
        "the next initial checkpoint differs from the previous run's output identity",
    )?;
    Ok(())
}

/// Runs `first`, then every config `next` gives that continues the run before
/// it, each through `run_native_expanded_training_v1`. Every successful run's
/// result goes to `report`; after each complete run `done` gets the run's
/// index in this process, its config and its result, and `next` is then asked
/// for the following config with the number of runs completed so far.
pub fn follow_v1(
    first: NativeExpandedTrainingRunV1,
    mut next: impl FnMut(usize) -> Result<FollowNextV1, String>,
    mut report: impl FnMut(&Value) -> Result<(), String>,
    mut done: impl FnMut(usize, &NativeExpandedTrainingRunV1, &Value) -> Result<(), String>,
) -> Result<FollowEndV1, FollowErrorV1> {
    let mut config = first;
    let mut index = 0usize;
    loop {
        let result = run_native_expanded_training_v1(&config, None).map_err(FollowErrorV1::Run)?;
        report(&result).map_err(FollowErrorV1::Run)?;
        if result["complete"] != true {
            return Ok(FollowEndV1::Incomplete);
        }
        done(index, &config, &result).map_err(FollowErrorV1::Run)?;
        index += 1;
        match next(index).map_err(FollowErrorV1::Run)? {
            FollowNextV1::Stop => return Ok(FollowEndV1::Stopped),
            FollowNextV1::Orphaned => return Ok(FollowEndV1::Orphaned),
            FollowNextV1::Unusable(reason) => return Err(FollowErrorV1::Refused(reason)),
            FollowNextV1::Config(candidate) => {
                check_continuation_v1(&config, &result, &candidate)
                    .map_err(FollowErrorV1::Refused)?;
                config = *candidate;
            }
        }
    }
}

/// The spool a followed process reads: an existing, absolute, plain and empty
/// directory (the driver creates one per process).
pub struct SpoolV1 {
    directory: PathBuf,
    #[cfg(unix)]
    parent: u32,
}

impl SpoolV1 {
    pub fn open_v1(directory: &Path) -> Result<Self, String> {
        check(directory.is_absolute(), "absolute spool path required")?;
        let meta = fs::symlink_metadata(directory).map_err(err)?;
        check(meta.is_dir(), "the spool is not a plain directory")?;
        check(
            fs::read_dir(directory).map_err(err)?.next().is_none(),
            "the spool is not empty (each process needs a new one)",
        )?;
        Ok(Self {
            directory: directory.to_path_buf(),
            #[cfg(unix)]
            parent: std::os::unix::process::parent_id(),
        })
    }

    /// Publishes the done record of the `index`-th run (durable publication,
    /// as every receipt of a run).
    pub fn publish_done_v1(
        &self,
        index: usize,
        config: &NativeExpandedTrainingRunV1,
        result: &Value,
    ) -> Result<(), String> {
        let document = json!({"schema":DONE_SCHEMA, "index":index,
            "output_directory":config.output_directory, "source":result["source"],
            "checkpoint_sha256":result["actual_identity"]["checkpoint_sha256"]});
        publish(&self.directory, &done_name_v1(index), &document).map(|_| ())
    }

    /// Waits for `STOP`, the parent's end or `next.json`.
    pub fn wait_v1(&self) -> Result<FollowNextV1, String> {
        loop {
            if let Some(next) = self.look_v1()? {
                return Ok(next);
            }
            std::thread::sleep(SPOOL_POLL);
        }
    }

    /// One look at the spool: `STOP` first, then the parent (Unix), then
    /// `next.json` (read in full and removed, then parsed as a fresh process
    /// parses its config).
    fn look_v1(&self) -> Result<Option<FollowNextV1>, String> {
        match fs::symlink_metadata(self.directory.join(STOP_NAME)) {
            Ok(_) => return Ok(Some(FollowNextV1::Stop)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(err(error)),
        }
        #[cfg(unix)]
        if std::os::unix::process::parent_id() != self.parent {
            return Ok(Some(FollowNextV1::Orphaned));
        }
        let path = self.directory.join(NEXT_NAME);
        // Every failure to take the request is a refusal (the process ends
        // with status 3 and the driver uses a fresh process); a refused
        // request is left where it is (a directory is never deleted).
        let refuse = |why: String| Ok(Some(FollowNextV1::Unusable(why)));
        let meta = match fs::symlink_metadata(&path) {
            Ok(meta) => meta,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return refuse(format!("next config cannot be inspected: {error}")),
        };
        if !meta.is_file() || meta.len() > SMALL_CAP {
            return refuse("next config is not a regular file of at most 16 MiB".into());
        }
        let mut bytes = Vec::new();
        if let Err(error) =
            fs::File::open(&path).and_then(|file| file.take(SMALL_CAP + 1).read_to_end(&mut bytes))
        {
            return refuse(format!("next config cannot be read: {error}"));
        }
        if bytes.len() as u64 > SMALL_CAP {
            return refuse("next config exceeds 16 MiB".into());
        }
        let config = match serde_json::from_slice::<NativeExpandedTrainingRunV1>(&bytes) {
            Ok(config) => config,
            Err(error) => return refuse(format!("next config does not parse: {error}")),
        };
        // Taken: removed, so that it is never read again.
        if let Err(error) = fs::remove_file(&path) {
            return refuse(format!("next config cannot be removed: {error}"));
        }
        Ok(Some(FollowNextV1::Config(Box::new(config))))
    }
}
