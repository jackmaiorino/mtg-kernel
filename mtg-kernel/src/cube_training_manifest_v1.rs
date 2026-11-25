use super::{invalid, sha256_hex, TrainingDriverError};
use crate::cube_pod_runner_v1::is_sha256_hex;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

#[derive(Deserialize)]
struct CommonEntry {
    seq: u64,
    kind: String,
    time_et: String,
    prefix_sha256: String,
    files: BTreeMap<String, String>,
}

#[derive(Deserialize)]
pub(super) struct ResumeFields {
    pub(super) run_id: String,
    pub(super) host_loss_evidence: String,
    pub(super) last_published_update: u64,
    pub(super) resume_batch: u64,
}

pub(super) struct EntryProof {
    pub(super) start: usize,
    end: usize,
    pub(super) resume: ResumeFields,
}

struct Section<'a> {
    seq: u64,
    kind: &'a str,
    start: usize,
    has_json: bool,
}

pub(super) fn read_resume_entry(
    manifest: &[u8],
    seq: u64,
) -> Result<EntryProof, TrainingDriverError> {
    let text = std::str::from_utf8(manifest).map_err(|_| invalid("manifest is not UTF-8"))?;
    let lines = text.split_inclusive('\n').scan(0, |offset, line| {
        let start = *offset;
        *offset += line.len();
        Some((start, line.trim_end_matches(['\r', '\n'])))
    });
    let mut lines = lines.peekable();
    let mut current: Option<Section<'_>> = None;
    let mut next_seq = 1;
    let mut selected = None;
    while let Some((start, line)) = lines.next() {
        if let Some(heading) = line.strip_prefix("## ") {
            require_entry_block(current.take())?;
            if let Some(entry) = heading.strip_prefix("Entry ") {
                let (number, kind) = entry
                    .split_once(": ")
                    .ok_or_else(|| invalid("invalid manifest entry heading"))?;
                if number != next_seq.to_string() || !known_kind(kind) {
                    return Err(invalid("invalid manifest entry sequence or kind"));
                }
                current = Some(Section {
                    seq: next_seq,
                    kind,
                    start,
                    has_json: false,
                });
            }
        } else if let Some(language) = line.strip_prefix("```") {
            let body_start = lines.peek().map_or(manifest.len(), |&(offset, _)| offset);
            let (body_end, _) = lines
                .by_ref()
                .find(|&(_, text)| text == "```")
                .ok_or_else(|| invalid("unterminated fenced block in manifest"))?;
            if language == "json" {
                if let Some(section) = current.as_mut() {
                    if section.has_json {
                        return Err(invalid("manifest entry has multiple JSON blocks"));
                    }
                    let body = &manifest[body_start..body_end];
                    let common: CommonEntry = serde_json::from_slice(body)?;
                    if common.seq != section.seq
                        || common.kind != section.kind
                        || common.prefix_sha256 != sha256_hex(&manifest[..section.start])
                        || !valid_time(&common.time_et)
                        || common
                            .files
                            .iter()
                            .any(|(name, pin)| name.is_empty() || !is_sha256_hex(pin))
                    {
                        return Err(invalid("manifest entry header or prefix sha256 differs"));
                    }
                    section.has_json = true;
                    next_seq += 1;
                    if common.seq == seq && common.kind == "resume" {
                        selected = Some(EntryProof {
                            start: section.start,
                            end: lines.peek().map_or(manifest.len(), |&(offset, _)| offset),
                            resume: serde_json::from_slice(body)?,
                        });
                    }
                }
            }
        }
    }
    require_entry_block(current)?;
    selected.ok_or_else(|| invalid("resume entry is absent from the pinned manifest"))
}

fn require_entry_block(section: Option<Section<'_>>) -> Result<(), TrainingDriverError> {
    if section.is_some_and(|section| !section.has_json) {
        return Err(invalid("manifest entry is missing its JSON block"));
    }
    Ok(())
}

fn known_kind(kind: &str) -> bool {
    matches!(
        kind,
        "d0_signed"
            | "a6_part2_countersigned"
            | "panel"
            | "fixtures"
            | "training_run"
            | "read_set"
            | "stop"
            | "restart"
            | "resume"
    )
}

fn valid_time(time: &str) -> bool {
    let shape = b"0000-00-00 00:00 ET";
    time.len() == shape.len()
        && time
            .bytes()
            .zip(shape.iter().copied())
            .all(|(got, expected)| {
                if expected == b'0' {
                    got.is_ascii_digit()
                } else {
                    got == expected
                }
            })
}

pub(super) fn verify_entry_commit(
    repository: &Path,
    manifest_path: &Path,
    lead_ref: &str,
    commit: &str,
    manifest: &[u8],
    entry: &EntryProof,
) -> Result<(), TrainingDriverError> {
    if !(lead_ref.starts_with("refs/heads/") || lead_ref.starts_with("refs/remotes/"))
        || !matches!(commit.len(), 40 | 64)
        || !commit
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(invalid(
            "resume needs a full commit id and the trusted lead branch ref",
        ));
    }
    git(repository, &["check-ref-format", lead_ref])?;
    if git(repository, &["rev-parse", "--show-prefix"])? != b"\n" {
        return Err(invalid("resume needs the records repository root"));
    }
    let root = repository.canonicalize()?;
    let path = manifest_path.canonicalize()?;
    let relative = path
        .strip_prefix(root)
        .map_err(|_| invalid("resume manifest is outside the records repository"))?;
    let relative = git_path(relative)?;
    let history = git(repository, &["rev-list", "--first-parent", lead_ref, "--"])?;
    if !history
        .split(|&byte| byte == b'\n')
        .any(|line| line == commit.as_bytes())
    {
        return Err(invalid(
            "resume entry commit is not on the lead branch's first-parent history",
        ));
    }
    let committed = git(repository, &["show", &format!("{commit}:{relative}")])?;
    if !committed.starts_with(&manifest[..entry.end]) {
        return Err(invalid(
            "resume entry bytes differ from its lead-branch commit",
        ));
    }
    let parents = git(repository, &["show", "-s", "--format=%P", commit, "--"])?;
    for parent in std::str::from_utf8(&parents)
        .map_err(|_| invalid("invalid parent commit ids"))?
        .split_ascii_whitespace()
    {
        let present = git(repository, &["ls-tree", "-z", parent, "--", &relative])?;
        if !present.is_empty() {
            let before = git(repository, &["show", &format!("{parent}:{relative}")])?;
            if !manifest[..entry.start].starts_with(&before) {
                return Err(invalid(
                    "resume entry was not appended by its cited lead-branch commit",
                ));
            }
        }
    }
    Ok(())
}

fn git_path(path: &Path) -> Result<String, TrainingDriverError> {
    let name = path
        .to_str()
        .filter(|name| !name.is_empty())
        .ok_or_else(|| invalid("invalid resume manifest path"))?;
    Ok(name.replace(std::path::MAIN_SEPARATOR, "/"))
}

fn git(repository: &Path, args: &[&str]) -> Result<Vec<u8>, TrainingDriverError> {
    let output = Command::new("git")
        .arg("--no-replace-objects")
        .arg("--literal-pathspecs")
        .arg("-C")
        .arg(repository)
        .args(args)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_COMMON_DIR")
        .output()?;
    if !output.status.success() {
        return Err(invalid(format!(
            "cannot verify resume commit using git {}: {}",
            args[0],
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(output.stdout)
}
