use super::{
    index_from_update_path, invalid, json_bytes, publish_bytes, read_commit, read_json,
    replace_bytes, sha256_hex, stop_paths, sync_directory, update_directories, RunConfig,
    TrainingDriverError,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

const RUNNER_MARKER: &str = "runner-active.json";

/// A D0.3b resume entry in the pinned shakedown manifest. The pin is supplied
/// by the launcher from the authorized manifest record, independently of the
/// file at `manifest`. The one-based line points to a single JSON object in the
/// Markdown manifest (for example inside a fenced JSON block).
///
/// The object has `kind: "resume"`, `run`, `host_loss_evidence`,
/// `last_published_update` (zero-based batch index, null before the first
/// update), and `next_untouched_batch`.
#[derive(Debug, Clone)]
pub struct ResumeAuthorization {
    pub manifest: PathBuf,
    pub manifest_sha256: String,
    pub entry_line: usize,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ResumeEntry {
    kind: String,
    run: String,
    host_loss_evidence: String,
    last_published_update: Option<u64>,
    next_untouched_batch: u64,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ResumeReceipt {
    manifest_len: usize,
    manifest_sha256: String,
}

impl ResumeAuthorization {
    fn verify(
        &self,
        run: &str,
        next_batch: u64,
        previous: Option<ResumeReceipt>,
    ) -> Result<ResumeReceipt, TrainingDriverError> {
        let manifest = fs::read(&self.manifest)?;
        let digest = sha256_hex(&manifest);
        if digest != self.manifest_sha256 {
            return Err(invalid("resume manifest sha256 differs from the D0.3b pin"));
        }
        let mut lines = manifest.split_inclusive(|&byte| byte == b'\n');
        let line_index = self
            .entry_line
            .checked_sub(1)
            .ok_or_else(|| invalid("resume entry line must be one-based"))?;
        let offset: usize = lines.by_ref().take(line_index).map(<[u8]>::len).sum();
        let line = lines
            .next()
            .ok_or_else(|| invalid("resume entry is absent from the pinned manifest"))?;
        if let Some(previous) = previous {
            if offset < previous.manifest_len
                || sha256_hex(&manifest[..previous.manifest_len]) != previous.manifest_sha256
            {
                return Err(invalid(
                    "resume needs a new entry in the append-only manifest",
                ));
            }
        }
        let entry: ResumeEntry = serde_json::from_slice(line)?;
        if entry.kind != "resume"
            || entry.run != run
            || entry.host_loss_evidence.trim().is_empty()
            || entry.last_published_update != next_batch.checked_sub(1)
            || entry.next_untouched_batch != next_batch
        {
            return Err(invalid(
                "resume entry differs from the run or next untouched batch",
            ));
        }
        Ok(ResumeReceipt {
            manifest_len: manifest.len(),
            manifest_sha256: digest,
        })
    }
}

/// Resolve the next untouched batch without mutating the run. A staged commit
/// counts as published here so authorization precedes recovery writes.
pub(super) fn check_resume(
    config: &RunConfig,
) -> Result<Option<ResumeReceipt>, TrainingDriverError> {
    let root = &config.root;
    let marker = root.join(RUNNER_MARKER);
    let pending = update_directories(&root.join("pending"))?;
    if !marker.exists() && pending.is_empty() {
        if config.resume.is_some() {
            return Err(invalid("resume entry supplied without an unrecorded exit"));
        }
        return Ok(None);
    }
    if !stop_paths(root)?.is_empty() {
        return Ok(None);
    }
    let mut completed = BTreeSet::new();
    for path in update_directories(&root.join("updates"))? {
        let batch = index_from_update_path(&path)?;
        if path.join("stop.json").exists() {
            // Recovery verifies and mirrors the committed stop before refusing.
            return Ok(None);
        }
        completed.insert(batch);
    }
    let mut abandoned = false;
    for path in pending {
        let batch = index_from_update_path(&path)?;
        let Some(commit) = read_commit(&path, batch, false)? else {
            abandoned = true;
            continue;
        };
        if commit.files.contains_key("stop.json") {
            return Ok(None);
        }
        completed.insert(batch);
    }
    let proof = config
        .resume
        .as_ref()
        .ok_or(TrainingDriverError::ResumeAuthorizationRequired)?;
    if abandoned {
        return Err(invalid(
            "host-loss resume cannot replay a batch that was touched",
        ));
    }
    let next = completed.len() as u64;
    if completed.into_iter().ne(0..next) || next > config.update_range[1] {
        return Err(invalid("resume has no contiguous committed batch prefix"));
    }
    let previous = if marker.exists() {
        read_json::<Option<ResumeReceipt>>(&marker)?
    } else {
        None
    };
    proof.verify(&config.run, next, previous).map(Some)
}

/// The file survives a process exit which cannot run the driver's cleanup.
pub(super) struct RunnerMarker {
    path: PathBuf,
    in_batch: bool,
}

impl RunnerMarker {
    pub(super) fn open(
        root: &Path,
        receipt: Option<ResumeReceipt>,
    ) -> Result<Self, TrainingDriverError> {
        let path = root.join(RUNNER_MARKER);
        let resuming = receipt.is_some();
        let bytes = json_bytes(&receipt)?;
        if resuming && path.exists() {
            replace_bytes(&path, &bytes)?;
        } else {
            if path.exists() {
                return Err(TrainingDriverError::ResumeAuthorizationRequired);
            }
            publish_bytes(&path, &bytes)?;
        }
        // Retain the marker if recovery fails after accepting the resume entry.
        Ok(Self {
            path,
            in_batch: resuming,
        })
    }

    pub(super) fn start_batch(&mut self) {
        self.in_batch = true;
    }

    pub(super) fn finish_batch(&mut self) {
        self.in_batch = false;
    }
}

impl Drop for RunnerMarker {
    fn drop(&mut self) {
        if !self.in_batch && !std::thread::panicking() && fs::remove_file(&self.path).is_ok() {
            let _ = sync_directory(self.path.parent().expect("runner marker parent"));
        }
    }
}
