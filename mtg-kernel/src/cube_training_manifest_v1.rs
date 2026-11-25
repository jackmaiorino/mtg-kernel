use super::{invalid, sha256_hex, TrainingDriverError};
use crate::cube_pod_runner_v1::is_sha256_hex;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::io::Write;
use std::ops::Range;
use std::path::Path;
use std::process::{Command, Output, Stdio};

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
    range: Range<usize>,
    entry_one: Range<usize>,
    seq: u64,
    lead_branch: String,
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
    let entries = parse_entries(manifest)?;
    let first = &entries[0];
    let selected = seq
        .checked_sub(1)
        .and_then(|index| usize::try_from(index).ok())
        .and_then(|index| entries.get(index))
        .filter(|entry| entry.common.kind == "resume")
        .ok_or_else(|| invalid("resume entry is absent from the pinned manifest"))?;
    #[derive(Deserialize)]
    struct D0Signed {
        lead_branch: String,
    }
    if first.common.kind != "d0_signed" {
        return Err(invalid("manifest entry 1 must be d0_signed"));
    }
    let D0Signed { lead_branch } = decode_entry(first.body)?;
    if lead_branch.is_empty()
        || lead_branch.starts_with('-')
        || lead_branch
            .chars()
            .any(|c| c.is_whitespace() || c.is_control())
    {
        return Err(invalid("manifest entry 1 has an invalid lead_branch"));
    }
    Ok(EntryProof {
        start: selected.range.start,
        range: selected.range.clone(),
        entry_one: first.range.clone(),
        seq,
        lead_branch,
        resume: decode_entry(selected.body)?,
    })
}

struct ParsedEntry<'a> {
    common: CommonEntry,
    body: &'a [u8],
    range: Range<usize>,
}

fn decode_entry<T: serde::de::DeserializeOwned>(body: &[u8]) -> Result<T, TrainingDriverError> {
    serde_json::from_slice(body)
        .map_err(|error| invalid(format!("invalid manifest entry JSON: {error}")))
}

fn parse_entries(manifest: &[u8]) -> Result<Vec<ParsedEntry<'_>>, TrainingDriverError> {
    let text = std::str::from_utf8(manifest).map_err(|_| invalid("manifest is not UTF-8"))?;
    let lines = text.split_inclusive('\n').scan(0, |offset, line| {
        let start = *offset;
        *offset += line.len();
        Some((start, line.trim_end_matches(['\r', '\n'])))
    });
    let mut lines = lines.peekable();
    let mut current: Option<Section<'_>> = None;
    let mut next_seq = 1;
    let mut parsed = Vec::new();
    while let Some((start, line)) = lines.next() {
        if let Some(heading) = line.strip_prefix("## ") {
            require_entry_block(current.take())?;
            if heading == "Entry" || heading.starts_with("Entry ") || heading.starts_with("Entry:")
            {
                let (number, kind) = heading
                    .strip_prefix("Entry ")
                    .and_then(|entry| entry.trim_end_matches([' ', '\t']).split_once(": "))
                    .ok_or_else(|| invalid("invalid manifest entry heading"))?;
                if number != next_seq.to_string() || !valid_kind_token(kind) {
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
                .find(|&(_, text)| text.trim_end_matches([' ', '\t']) == "```")
                .ok_or_else(|| invalid("unterminated fenced block in manifest"))?;
            if language.trim() == "json" {
                if let Some(section) = current.as_mut() {
                    if section.has_json {
                        return Err(invalid("manifest entry has multiple JSON blocks"));
                    }
                    let body = &manifest[body_start..body_end];
                    let common: CommonEntry = decode_entry(body)?;
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
                    let end = lines.peek().map_or(manifest.len(), |&(offset, _)| offset);
                    parsed.push(ParsedEntry {
                        common,
                        body,
                        range: section.start..end,
                    });
                }
            }
        }
    }
    require_entry_block(current)?;
    if parsed.is_empty() {
        return Err(invalid("manifest contains no structured entries"));
    }
    Ok(parsed)
}

fn require_entry_block(section: Option<Section<'_>>) -> Result<(), TrainingDriverError> {
    if section.is_some_and(|section| !section.has_json) {
        return Err(invalid("manifest entry is missing its JSON block"));
    }
    Ok(())
}

fn valid_kind_token(kind: &str) -> bool {
    !kind.is_empty()
        && kind
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
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
    manifest: &[u8],
    entry: &EntryProof,
) -> Result<(), TrainingDriverError> {
    if manifest_path.file_name().and_then(|name| name.to_str()) != Some("SHAKEDOWN-MANIFEST.md") {
        return Err(invalid(
            "resume must use the fixed SHAKEDOWN-MANIFEST.md path",
        ));
    }
    let top = git(repository, &["rev-parse", "--show-toplevel"])?;
    let top = std::str::from_utf8(&top).map_err(|_| invalid("invalid records repository root"))?;
    let root = Path::new(top.trim_end_matches(['\r', '\n']));
    let canonical_root = root.canonicalize()?;
    let canonical_path = manifest_path.canonicalize()?;
    let relative = canonical_path
        .strip_prefix(canonical_root)
        .map_err(|_| invalid("resume manifest is outside the records repository"))?;
    let relative = git_path(relative)?;
    let lead_ref = resolve_lead_ref(root, &entry.lead_branch)?;
    let history = git(
        root,
        &["rev-list", "--first-parent", "--parents", &lead_ref, "--"],
    )?;
    let history =
        std::str::from_utf8(&history).map_err(|_| invalid("invalid lead branch history"))?;
    let chain: Vec<_> = history
        .lines()
        .map(|line| line.split_ascii_whitespace().collect::<Vec<_>>())
        .collect();
    if chain.is_empty() || chain.iter().any(Vec::is_empty) {
        return Err(invalid("lead branch has no usable history"));
    }
    let mut queries = String::new();
    for commit in &chain {
        use std::fmt::Write as _;
        writeln!(queries, "{}:{relative}", commit[0]).expect("write to String");
    }
    let objects = git_with_input(
        root,
        &["cat-file", "--batch-check=%(objecttype) %(objectname)"],
        &queries,
    )?;
    let objects =
        std::str::from_utf8(&objects).map_err(|_| invalid("invalid manifest blob inventory"))?;
    let objects: Vec<_> = objects.lines().collect();
    if objects.len() != chain.len() {
        return Err(invalid("incomplete manifest blob inventory"));
    }

    let mut previous_oid = None;
    let mut previous = Vec::new();
    let mut introduced = false;
    let index = usize::try_from(entry.seq - 1).map_err(|_| invalid("invalid resume sequence"))?;
    for (commit, object) in chain.iter().zip(objects).rev() {
        let Some(oid) = object.strip_prefix("blob ") else {
            if previous_oid.is_some() {
                return Err(invalid(
                    "lead branch removed or renamed the manifest (append-only violation)",
                ));
            }
            continue;
        };
        if previous_oid == Some(oid) {
            continue;
        }
        let bytes = git(root, &["cat-file", "blob", oid])?;
        if !bytes.starts_with(&previous) {
            return Err(invalid(
                "lead branch rewrote or truncated the manifest (append-only violation)",
            ));
        }
        let parsed = parse_entries(&bytes)?;
        let matching = parsed.get(index);
        if previous_oid.is_none() {
            check_creation(root, &relative, commit)?;
            if matching.is_some() {
                return Err(invalid(
                    "resume entry must be appended after manifest creation",
                ));
            }
        } else if matching.is_some() && !introduced {
            if commit.len() != 2 {
                return Err(invalid(
                    "resume entry must be introduced by a non-merge commit on the lead branch",
                ));
            }
            introduced = true;
        }
        if introduced
            && matching
                .is_none_or(|found| bytes[found.range.clone()] != manifest[entry.range.clone()])
        {
            return Err(invalid("resume entry bytes differ on the lead branch"));
        }
        previous_oid = Some(oid);
        previous = bytes;
    }
    if !introduced {
        return Err(invalid(
            "resume entry is absent from the lead branch's first-parent history",
        ));
    }
    let tip_entries = parse_entries(&previous)?;
    if !previous.starts_with(manifest)
        || (previous.len() > manifest.len() && !manifest.ends_with(b"\n"))
        || previous[tip_entries[0].range.clone()] != manifest[entry.entry_one.clone()]
    {
        return Err(invalid(
            "pinned manifest is not an exact byte prefix of the lead branch tip",
        ));
    }
    Ok(())
}

fn resolve_lead_ref(repository: &Path, branch: &str) -> Result<String, TrainingDriverError> {
    let candidates = if branch.starts_with("refs/heads/") || branch.starts_with("refs/remotes/") {
        vec![branch.to_owned()]
    } else {
        vec![
            format!("refs/heads/{branch}"),
            format!("refs/remotes/{branch}"),
        ]
    };
    for candidate in candidates {
        if git_command(repository, &["show-ref", "--verify", "--quiet", &candidate])
            .output()?
            .status
            .success()
        {
            return Ok(candidate);
        }
    }
    Err(invalid("manifest entry 1 lead_branch was not found"))
}

fn check_creation(
    repository: &Path,
    path: &str,
    commit: &[&str],
) -> Result<(), TrainingDriverError> {
    if commit.len() > 2 {
        return Err(invalid("manifest was created by a merge commit"));
    }
    if let Some(&parent) = commit.get(1) {
        let diff = git(
            repository,
            &[
                "diff-tree",
                "-r",
                "-M",
                "-C",
                "--find-copies-harder",
                "--name-status",
                "--no-commit-id",
                "-z",
                parent,
                commit[0],
            ],
        )?;
        let mut fields = diff.split(|&byte| byte == 0);
        while let Some(status) = fields.next().filter(|field| !field.is_empty()) {
            let _from = fields.next();
            if (status.starts_with(b"R") || status.starts_with(b"C"))
                && fields.next() == Some(path.as_bytes())
            {
                return Err(invalid("manifest was created by rename or copy"));
            }
        }
    }
    Ok(())
}

fn git_path(path: &Path) -> Result<String, TrainingDriverError> {
    let name = path
        .to_str()
        .filter(|name| !name.is_empty() && !name.contains(['\r', '\n']))
        .ok_or_else(|| invalid("invalid resume manifest path"))?;
    Ok(name.replace(std::path::MAIN_SEPARATOR, "/"))
}

fn git(repository: &Path, args: &[&str]) -> Result<Vec<u8>, TrainingDriverError> {
    git_output(git_command(repository, args).output()?, args[0])
}

fn git_with_input(
    repository: &Path,
    args: &[&str],
    input: &str,
) -> Result<Vec<u8>, TrainingDriverError> {
    let mut process = git_command(repository, args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut stdin = process.stdin.take().expect("git stdin pipe");
    let output = std::thread::scope(|scope| {
        let write = scope.spawn(move || stdin.write_all(input.as_bytes()));
        let output = process.wait_with_output();
        write.join().expect("git input writer")?;
        output
    })?;
    git_output(output, args[0])
}

fn git_command(repository: &Path, args: &[&str]) -> Command {
    let mut command = Command::new("git");
    command
        .arg("--no-replace-objects")
        .arg("--literal-pathspecs")
        .arg("-C")
        .arg(repository)
        .args(args)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_COMMON_DIR");
    command
}

fn git_output(output: Output, subcommand: &str) -> Result<Vec<u8>, TrainingDriverError> {
    if !output.status.success() {
        return Err(invalid(format!(
            "cannot verify resume commit using git {subcommand}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(output.stdout)
}
