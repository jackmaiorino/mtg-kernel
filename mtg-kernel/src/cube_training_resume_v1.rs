use super::{
    index_from_update_path, invalid, json_bytes, publish_bytes, read_commit, read_json,
    replace_bytes, sha256_hex, stop_paths, sync_directory, update_directories, RunConfig,
    TrainingDriverError,
};
#[path = "cube_training_manifest_v1.rs"]
mod manifest;
use manifest::{read_resume_entry, verify_entry_commit};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

const RUNNER_MARKER: &str = "runner-active.json";

/// D0.3b entry selector and provenance from the authorized launcher record.
#[derive(Debug, Clone)]
pub struct ResumeAuthorization {
    pub manifest: PathBuf,
    pub manifest_sha256: String,
    pub entry_seq: u64,
    pub entry_commit: String,
    pub records_repository: PathBuf,
    /// Trusted full ref from launch configuration, independent of the manifest.
    pub lead_ref: String,
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
        run_id: &str,
        next_batch: u64,
        previous: Option<ResumeReceipt>,
    ) -> Result<ResumeReceipt, TrainingDriverError> {
        let manifest = fs::read(&self.manifest)?;
        let digest = sha256_hex(&manifest);
        if digest != self.manifest_sha256 {
            return Err(invalid("resume manifest sha256 differs from the D0.3b pin"));
        }
        let entry = read_resume_entry(&manifest, self.entry_seq)?;
        if let Some(previous) = previous {
            if entry.start < previous.manifest_len
                || sha256_hex(&manifest[..previous.manifest_len]) != previous.manifest_sha256
            {
                return Err(invalid(
                    "resume needs a new entry in the append-only manifest",
                ));
            }
        }
        // A resume before any batch uses 0 for both fields.
        if entry.resume.run_id != run_id
            || entry.resume.host_loss_evidence.trim().is_empty()
            || entry.resume.last_published_update != next_batch.saturating_sub(1)
            || entry.resume.resume_batch != next_batch
        {
            return Err(invalid(
                "resume entry differs from the run or next untouched batch",
            ));
        }
        verify_entry_commit(
            &self.records_repository,
            &self.manifest,
            &self.lead_ref,
            &self.entry_commit,
            &manifest,
            &entry,
        )?;
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
    let run_id = format!("{}/{}", config.namespace, config.run);
    proof.verify(&run_id, next, previous).map(Some)
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
