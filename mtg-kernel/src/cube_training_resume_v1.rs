use super::{
    check_stop_ledger, index_from_update_path, invalid, json_bytes, publish_bytes,
    publish_fresh_bytes, published_update_indices, read_commit, read_json, record_process_death,
    replace_bytes, sha256_hex, stop_paths, sync_directory, update_directories, RunConfig,
    StopRecord, TrainingDriverError,
};
#[path = "cube_training_manifest_v1.rs"]
mod manifest;
use manifest::{read_resume_entry, verify_entry_commit};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

const RUNNER_MARKER: &str = "runner-active.json";

/// The OS releases the lock on exit. The file itself stays at a stable path.
pub(super) struct RunLock {
    _file: File,
}

impl RunLock {
    pub(super) fn acquire(config: &RunConfig) -> Result<Self, TrainingDriverError> {
        let root = &config.root;
        let mut options = OpenOptions::new();
        options.read(true).write(true);
        let path = root.join("runner.lock");
        let file = match options.open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == ErrorKind::NotFound => {
                if config.resume.is_some()
                    || root.join(RUNNER_MARKER).try_exists()?
                    || !update_directories(&root.join("pending"))?.is_empty()
                {
                    return Err(invalid(
                        "cannot verify an interrupted run without runner.lock",
                    ));
                }
                fs::create_dir_all(root)?;
                let file = options.create(true).truncate(false).open(&path)?;
                file.sync_all()?;
                sync_directory(root)?;
                file
            }
            Err(error) => return Err(error.into()),
        };
        match file.try_lock() {
            Ok(()) => Ok(Self { _file: file }),
            Err(TryLockError::WouldBlock) => Err(TrainingDriverError::RunAlreadyActive),
            Err(TryLockError::Error(error)) => Err(error.into()),
        }
    }
}

fn receipt_path(root: &Path, seq: u64) -> PathBuf {
    root.join("resumes").join(format!("entry-{seq:020}.json"))
}

/// D0.3b entry selector and provenance from the authorized launcher record.
#[derive(Debug, Clone)]
pub struct ResumeAuthorization {
    pub manifest: PathBuf,
    pub manifest_sha256: String,
    pub entry_seq: u64,
    /// Git repository containing the manifest and the lead branch named by entry 1.
    pub records_repository: PathBuf,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ResumeReceipt {
    entry_seq: u64,
    batch_index: u64,
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
        verify_entry_commit(&self.records_repository, &self.manifest, &manifest, &entry)?;
        Ok(ResumeReceipt {
            entry_seq: self.entry_seq,
            batch_index: next_batch,
            manifest_len: manifest.len(),
            manifest_sha256: digest,
        })
    }
}

/// A staged commit counts as completed so authorization precedes recovery.
/// On an unrecorded exit without proof, write only the process-death stop.
pub(super) fn check_resume(
    config: &RunConfig,
    _lock: &RunLock,
) -> Result<Option<ResumeReceipt>, TrainingDriverError> {
    let root = &config.root;
    let marker = root.join(RUNNER_MARKER);
    let stops = stop_paths(root)?;
    if stops
        .iter()
        .any(|path| read_json::<StopRecord>(path).is_ok_and(|stop| stop.kind == "process_death"))
    {
        return check_stop_ledger(root).map(|()| None);
    }
    let pending = update_directories(&root.join("pending"))?;
    if !marker.exists() && pending.is_empty() {
        if config.resume.is_some() {
            return Err(invalid("resume entry supplied without an unrecorded exit"));
        }
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
    let mut abandoned = None;
    for path in pending {
        let batch = index_from_update_path(&path)?;
        let Some(commit) = read_commit(&path, batch, false)? else {
            abandoned.get_or_insert((batch, path));
            continue;
        };
        if commit.files.contains_key("stop.json") {
            return Ok(None);
        }
        completed.insert(batch);
    }
    if !stops.is_empty() {
        if abandoned.is_some() {
            return check_stop_ledger(root).map(|()| None);
        }
        // Recover a completed batch whose publication already logged an I/O stop.
        return Ok(None);
    }
    let Some(proof) = &config.resume else {
        let saved: RunConfig = read_json(&root.join("run.json"))?;
        if saved.run != config.run || saved.namespace != config.namespace {
            return Err(invalid("run id differs from the interrupted run"));
        }
        published_update_indices(root)?;
        record_process_death(config, completed.last().copied(), abandoned)?;
        return check_stop_ledger(root).map(|()| None);
    };
    if abandoned.is_some() {
        return Err(invalid(
            "host-loss resume cannot replay a batch that was touched",
        ));
    }
    let next = completed.len() as u64;
    if completed.into_iter().ne(0..next) || next > config.update_range[1] {
        return Err(invalid("resume has no contiguous committed batch prefix"));
    }
    if receipt_path(root, proof.entry_seq).try_exists()? {
        return Err(invalid("resume entry was already used"));
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
        if let Some(receipt) = &receipt {
            publish_fresh_bytes(
                &receipt_path(root, receipt.entry_seq),
                &json_bytes(receipt)?,
            )?;
        }
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
