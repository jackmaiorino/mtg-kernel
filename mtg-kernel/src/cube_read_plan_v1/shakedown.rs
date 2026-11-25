use super::*;
use serde::de::{self, MapAccess, SeqAccess, Visitor};
use std::process::Stdio;

const MANIFEST_FILE: &str = "SHAKEDOWN-MANIFEST.md";

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum RestartKind {
    Amendment,
    Note,
}

#[derive(Deserialize)]
struct Restart {
    run: String,
    restart_kind: RestartKind,
    note_path: PathBuf,
    note_sha256: String,
    signature_line: String,
    a6_part2_sha256: String,
    panel: Option<RestartPanel>,
    namespaces: Option<Vec<String>>,
    replacements: Vec<Replacement>,
}

#[derive(Deserialize)]
struct RestartPanel {
    run: String,
    sha256: String,
}

fn record_panel(
    panels: &mut BTreeMap<String, String>,
    run: &str,
    digest: &str,
) -> Result<(), PodRunnerError> {
    if d8_run_number(run).is_none() || !is_sha256_hex(digest) {
        return Err(plan_err("invalid dose-d8 panel pin"));
    }
    if panels
        .insert(run.to_owned(), digest.to_owned())
        .is_some_and(|old| old != digest)
    {
        return Err(plan_err("conflicting dose-d8 panel pins"));
    }
    Ok(())
}

#[derive(Deserialize)]
struct Replacement {
    namespace: String,
    set: String,
    replaces: String,
    retired: Vec<[u64; 2]>,
}

// A structured entry is JSON, but duplicate keys cannot be treated as authorization.
struct UniqueJson(Value);

impl<'de> Deserialize<'de> for UniqueJson {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Unique;
        impl<'de> Visitor<'de> for Unique {
            type Value = Value;

            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("JSON without duplicate keys")
            }

            fn visit_bool<E: de::Error>(self, value: bool) -> Result<Value, E> {
                Ok(value.into())
            }
            fn visit_i64<E: de::Error>(self, value: i64) -> Result<Value, E> {
                Ok(value.into())
            }
            fn visit_u64<E: de::Error>(self, value: u64) -> Result<Value, E> {
                Ok(value.into())
            }
            fn visit_f64<E: de::Error>(self, value: f64) -> Result<Value, E> {
                Ok(value.into())
            }
            fn visit_str<E: de::Error>(self, value: &str) -> Result<Value, E> {
                Ok(value.into())
            }
            fn visit_unit<E: de::Error>(self) -> Result<Value, E> {
                Ok(Value::Null)
            }

            fn visit_seq<S: SeqAccess<'de>>(self, mut seq: S) -> Result<Value, S::Error> {
                let mut values = Vec::new();
                while let Some(UniqueJson(value)) = seq.next_element()? {
                    values.push(value);
                }
                Ok(Value::Array(values))
            }

            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Value, M::Error> {
                let mut values = serde_json::Map::new();
                while let Some((key, UniqueJson(value))) = map.next_entry::<String, UniqueJson>()? {
                    if values.insert(key.clone(), value).is_some() {
                        return Err(de::Error::custom(format!("duplicate JSON key {key}")));
                    }
                }
                Ok(Value::Object(values))
            }
        }
        deserializer.deserialize_any(Unique).map(Self)
    }
}

struct Entry<'a> {
    kind: &'a str,
    value: Value,
    bytes: &'a [u8],
}

struct PendingEntry<'a> {
    seq: u64,
    kind: &'a str,
    start: usize,
    block: Option<(usize, usize, usize)>,
}

fn finish_entry<'a>(
    pending: PendingEntry<'a>,
    text: &'a str,
    entries: &mut Vec<Entry<'a>>,
) -> Result<(), PodRunnerError> {
    let (start, end, fence_end) = pending
        .block
        .ok_or_else(|| plan_err("shakedown entry lacks a fenced JSON block"))?;
    let UniqueJson(value) = serde_json::from_str::<UniqueJson>(&text[start..end])
        .map_err(|error| plan_err(format!("invalid shakedown JSON: {error}")))?;
    if pending.seq != entries.len() as u64 + 1
        || value["seq"].as_u64() != Some(pending.seq)
        || value["kind"].as_str() != Some(pending.kind)
    {
        return Err(plan_err(
            "shakedown entry seq or kind disagrees with its heading",
        ));
    }
    let time = value["time_et"]
        .as_str()
        .and_then(|time| time.strip_suffix(" ET"))
        .and_then(|time| time.split_once(' '));
    if !time.is_some_and(|(date, time)| super::attestation::is_timestamp(date, time)) {
        return Err(plan_err("invalid shakedown entry time_et"));
    }
    if value["prefix_sha256"].as_str()
        != Some(sha256_hex(&text.as_bytes()[..pending.start]).as_str())
    {
        return Err(plan_err("shakedown entry prefix hash does not match"));
    }
    if !value["files"].as_object().is_some_and(|files| {
        files
            .iter()
            .all(|(path, digest)| !path.is_empty() && digest.as_str().is_some_and(is_sha256_hex))
    }) {
        return Err(plan_err("invalid shakedown entry files map"));
    }
    entries.push(Entry {
        kind: pending.kind,
        value,
        bytes: &text.as_bytes()[pending.start..fence_end],
    });
    Ok(())
}

fn parse_entries(raw: &[u8]) -> Result<Vec<Entry<'_>>, PodRunnerError> {
    let text = std::str::from_utf8(raw).map_err(|_| plan_err("shakedown manifest is not UTF-8"))?;
    let mut entries = Vec::new();
    let mut current: Option<PendingEntry<'_>> = None;
    let mut fence: Option<(&str, usize)> = None;
    let mut offset = 0;
    for raw_line in text.split_inclusive('\n') {
        let start = offset;
        offset += raw_line.len();
        let line = raw_line.trim_end_matches(['\r', '\n']);
        if let Some((tag, body_start)) = fence {
            if line.trim_end_matches([' ', '\t']) == "```" {
                fence = None;
                if let Some(entry) = current.as_mut().filter(|_| tag == "json") {
                    if entry.block.replace((body_start, start, offset)).is_some() {
                        return Err(plan_err("shakedown entry has more than one JSON block"));
                    }
                }
            }
            continue;
        }
        if let Some(tag) = line.strip_prefix("```") {
            fence = Some((tag.trim(), offset));
            continue;
        }
        let Some(heading) = line.strip_prefix("## ") else {
            continue;
        };
        if let Some(entry) = current.take() {
            finish_entry(entry, text, &mut entries)?;
        }
        if !(heading == "Entry" || heading.starts_with("Entry ") || heading.starts_with("Entry:")) {
            continue;
        }
        let (seq, kind) = heading
            .strip_prefix("Entry ")
            .and_then(|heading| heading.trim_end_matches([' ', '\t']).split_once(": "))
            .ok_or_else(|| plan_err("malformed shakedown entry heading"))?;
        let seq =
            positive_decimal(seq).ok_or_else(|| plan_err("invalid shakedown entry sequence"))?;
        if kind.is_empty()
            || !kind
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
        {
            return Err(plan_err("invalid shakedown entry kind"));
        }
        current = Some(PendingEntry {
            seq,
            kind,
            start,
            block: None,
        });
    }
    if fence.is_some() {
        return Err(plan_err("unclosed shakedown manifest fence"));
    }
    if let Some(entry) = current {
        finish_entry(entry, text, &mut entries)?;
    }
    if entries.is_empty() {
        return Err(plan_err("no structured shakedown entries"));
    }
    Ok(entries)
}

fn git_command(repo: &Path, args: &[&str]) -> Command {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(repo).args(args);
    cmd
}

fn git_result(output: std::io::Result<std::process::Output>) -> Result<Vec<u8>, PodRunnerError> {
    let output =
        output.map_err(|error| plan_err(format!("cannot run records Git check: {error}")))?;
    if !output.status.success() {
        return Err(plan_err(format!(
            "records Git check failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(output.stdout)
}

fn git(repo: &Path, args: &[&str]) -> Result<Vec<u8>, PodRunnerError> {
    git_result(git_command(repo, args).output())
}

fn git_blobs(repo: &Path, revisions: &str) -> Result<String, PodRunnerError> {
    let mut child = git_command(
        repo,
        &[
            "cat-file",
            "--batch-check=%(objecttype) %(objectname)",
            "--buffer",
        ],
    )
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .map_err(|error| plan_err(format!("cannot run records Git batch check: {error}")))?;
    let mut input = child.stdin.take().expect("Git batch input is piped");
    let bytes = std::thread::scope(|scope| {
        let write = scope.spawn(move || input.write_all(revisions.as_bytes()));
        let output = child.wait_with_output();
        write
            .join()
            .expect("Git input thread failed")
            .map_err(|error| plan_err(error.to_string()))?;
        git_result(output)
    })?;
    String::from_utf8(bytes).map_err(|_| plan_err("invalid Git batch output"))
}

fn reject_copied_manifest(
    repo: &Path,
    parent: &str,
    commit: &str,
    path: &str,
) -> Result<(), PodRunnerError> {
    let diff = git(
        repo,
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
            commit,
        ],
    )?;
    let mut fields = diff.split(|&byte| byte == 0);
    while let Some(status) = fields.next().filter(|status| !status.is_empty()) {
        let _old_path = fields.next();
        if status.starts_with(b"R") || status.starts_with(b"C") {
            if fields.next() == Some(path.as_bytes()) {
                return Err(plan_err(
                    "shakedown manifest was created by a rename or copy",
                ));
            }
        }
    }
    Ok(())
}

fn verify_lead_history(
    pin: &ShakedownManifestPin,
    raw: &[u8],
    entries: &[Entry<'_>],
) -> Result<(), PodRunnerError> {
    if pin.path.file_name().and_then(|name| name.to_str()) != Some(MANIFEST_FILE) {
        return Err(plan_err("shakedown manifest must use the fixed filename"));
    }
    let first = &entries[0];
    let branch = first.value["lead_branch"]
        .as_str()
        .filter(|branch| {
            !branch.is_empty()
                && !branch.starts_with('-')
                && !branch.chars().any(|c| c.is_whitespace() || c.is_control())
        })
        .filter(|_| first.kind == "d0_signed")
        .ok_or_else(|| plan_err("entry 1 must name the research lead branch"))?;
    let top = git(&pin.records_repo, &["rev-parse", "--show-toplevel"])?;
    let top = std::str::from_utf8(&top).map_err(|_| plan_err("invalid Git root path"))?;
    let repo = fs::canonicalize(top.trim()).map_err(|error| plan_err(error.to_string()))?;
    let path = fs::canonicalize(&pin.path).map_err(|error| plan_err(error.to_string()))?;
    let relative = path
        .strip_prefix(&repo)
        .ok()
        .and_then(Path::to_str)
        .ok_or_else(|| plan_err("shakedown manifest is outside the records repository"))?
        .replace('\\', "/");
    if relative.contains(['\n', '\r']) {
        return Err(plan_err("invalid shakedown manifest repository path"));
    }
    let refs = if branch.starts_with("refs/heads/") || branch.starts_with("refs/remotes/") {
        vec![branch.to_owned()]
    } else {
        vec![
            format!("refs/heads/{branch}"),
            format!("refs/remotes/{branch}"),
        ]
    };
    let lead_ref = refs
        .iter()
        .find(|name| {
            git_command(&repo, &["show-ref", "--verify", "--quiet", name])
                .status()
                .is_ok_and(|status| status.success())
        })
        .ok_or_else(|| plan_err("research lead branch ref is missing"))?;
    let chain = git(
        &repo,
        &[
            "rev-list",
            "--reverse",
            "--first-parent",
            "--parents",
            lead_ref,
        ],
    )?;
    let chain = std::str::from_utf8(&chain).map_err(|_| plan_err("invalid Git history output"))?;
    let commits: Vec<Vec<&str>> = chain
        .lines()
        .map(|line| line.split_ascii_whitespace().collect())
        .collect();
    let paths: String = commits
        .iter()
        .map(|commit| format!("{}:{relative}\n", commit[0]))
        .collect();
    let blobs = git_blobs(&repo, &paths)?;
    if commits.len() != blobs.lines().count() {
        return Err(plan_err("incomplete Git manifest history"));
    }

    let pinned: Vec<_> = entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| entry.kind == "restart")
        .collect();
    let mut introduced = BTreeSet::new();
    let mut previous_blob = "";
    let mut previous = Vec::new();
    for (commit, blob) in commits.iter().zip(blobs.lines()) {
        let Some(blob) = blob.strip_prefix("blob ") else {
            if !previous_blob.is_empty() {
                return Err(plan_err(
                    "shakedown manifest was removed from the lead branch",
                ));
            }
            continue;
        };
        if blob == previous_blob {
            continue;
        }
        let next = git(&repo, &["cat-file", "blob", blob])?;
        if !next.starts_with(&previous) {
            return Err(plan_err(
                "shakedown manifest history is not byte-for-byte append-only",
            ));
        }
        let created = previous_blob.is_empty();
        if created {
            if commit.len() > 2 {
                return Err(plan_err("a merge created the shakedown manifest"));
            }
            if let [commit, parent] = commit.as_slice() {
                reject_copied_manifest(&repo, parent, commit, &relative)?;
            }
        }
        let historical = parse_entries(&next)?;
        for &(index, entry) in &pinned {
            if let Some(found) = historical.get(index) {
                if found.bytes != entry.bytes {
                    return Err(plan_err(
                        "shakedown restart entry differs from the pinned bytes",
                    ));
                }
                if introduced.insert(index) && (created || commit.len() != 2) {
                    return Err(plan_err(
                        "shakedown restart must be appended by a single-parent lead commit",
                    ));
                }
            } else if introduced.contains(&index) {
                return Err(plan_err("shakedown restart entry was removed"));
            }
        }
        previous_blob = blob;
        previous = next;
    }
    if !previous.starts_with(raw)
        || (previous.len() > raw.len() && !raw.ends_with(b"\n"))
        || introduced.len() != pinned.len()
    {
        return Err(plan_err(
            "pinned shakedown manifest is not present on the lead branch",
        ));
    }
    Ok(())
}

pub(super) fn authorize_restart(
    manifest: &SetManifest,
    pin: &ShakedownManifestPin,
) -> Result<RestartKind, PodRunnerError> {
    let raw = fs::read(&pin.path)
        .map_err(|error| plan_err(format!("cannot read shakedown manifest: {error}")))?;
    if sha256_hex(&raw) != pin.sha256 {
        return Err(plan_err("shakedown manifest differs from its D0.3b pin"));
    }
    let entries = parse_entries(&raw)?;
    let mut authorization = None;
    let mut replacements = BTreeMap::new();
    let mut panels = BTreeMap::new();
    for entry in &entries {
        if entry.kind == "panel" {
            let (run, digest) =
                if let Some(panel) = entry.value.get("panel").filter(|value| value.is_object()) {
                    (panel.get("run"), panel.get("sha256"))
                } else {
                    (
                        entry.value.get("run"),
                        entry
                            .value
                            .get("panel_sha256")
                            .filter(|v| !v.is_null() && v.as_str() != Some(""))
                            .or_else(|| entry.value.get("sha256")),
                    )
                };
            let (Some(run), Some(digest)) =
                (run.and_then(Value::as_str), digest.and_then(Value::as_str))
            else {
                return Err(plan_err("structured panel entry requires run and digest"));
            };
            record_panel(&mut panels, run, digest)?;
            continue;
        }
        if entry.kind != "restart" {
            continue;
        }
        let restart: Restart = serde_json::from_value(entry.value.clone())
            .map_err(|error| plan_err(format!("invalid structured restart: {error}")))?;
        if d8_run_number(&restart.run).is_none_or(|n| n < 2)
            || !is_sha256_hex(&restart.note_sha256)
            || !is_sha256_hex(&restart.a6_part2_sha256)
            || restart.note_path.as_os_str().is_empty()
            || !restart.signature_line.starts_with("Research lead,")
            || restart.signature_line.contains(['\r', '\n'])
        {
            return Err(plan_err(
                "invalid structured restart pins or signature line",
            ));
        }
        let note_path = pin
            .path
            .parent()
            .ok_or_else(|| plan_err("shakedown manifest has no parent directory"))?
            .join(&restart.note_path);
        let note = fs::read(&note_path)
            .map_err(|error| plan_err(format!("cannot read restart note: {error}")))?;
        let note_text =
            std::str::from_utf8(&note).map_err(|_| plan_err("restart note is not UTF-8"))?;
        if sha256_hex(&note) != restart.note_sha256
            || !note_text.lines().any(|line| line == restart.signature_line)
        {
            return Err(plan_err(
                "restart note differs from its pin or lacks the exact signature line",
            ));
        }
        let mut namespaces: BTreeSet<&str> = restart
            .namespaces
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(String::as_str)
            .collect();
        if let Some(panel) = &restart.panel {
            if panel.run != restart.run {
                return Err(plan_err("invalid restart panel pin"));
            }
            record_panel(&mut panels, &panel.run, &panel.sha256)?;
            namespaces.insert("dose-d8");
        }
        let mut listed = BTreeMap::new();
        for event in restart.replacements {
            if !d8_namespace(&event.namespace)
                || !event
                    .set
                    .rsplit_once("-restart-")
                    .is_some_and(|(name, number)| {
                        !name.is_empty() && positive_decimal(number).is_some()
                    })
                || !event
                    .replaces
                    .split_once('/')
                    .is_some_and(|(run, set)| d8_run_number(run).is_some() && !set.is_empty())
                || event.retired.iter().any(|[first, end]| first > end)
            {
                return Err(plan_err("invalid structured restart replacement"));
            }
            let key = (event.namespace, event.set);
            let value = (event.replaces, event.retired);
            if listed
                .insert(key, value.clone())
                .is_some_and(|old| old != value)
            {
                return Err(plan_err("conflicting structured restart replacements"));
            }
        }
        namespaces.extend(listed.keys().map(|(namespace, _)| namespace.as_str()));
        if namespaces.is_empty() || namespaces.iter().any(|ns| !d8_namespace(ns)) {
            return Err(plan_err("invalid or missing restart namespaces"));
        }
        if restart.run != manifest.run || !namespaces.contains(manifest.namespace.as_str()) {
            continue;
        }
        let identity = (
            restart.restart_kind,
            restart.a6_part2_sha256,
            restart.note_sha256,
        );
        if authorization
            .replace(identity.clone())
            .is_some_and(|previous| previous != identity)
            || identity.1 != manifest.declaration_sha256
        {
            return Err(plan_err(
                "restart countersignature pin or authorizations disagree",
            ));
        }
        for ((ns, set), value) in listed {
            if ns == manifest.namespace
                && replacements
                    .insert(set, value.clone())
                    .is_some_and(|old| old != value)
            {
                return Err(plan_err("conflicting restart replacement entries"));
            }
        }
    }

    let (kind, _, _) = authorization.ok_or_else(|| {
        plan_err("structured shakedown manifest does not authorize this restart run")
    })?;
    if manifest.namespace == "dose-d8"
        && manifest
            .panel_sha256
            .as_ref()
            .is_some_and(|panel| panels.get(&manifest.run) != Some(panel))
    {
        return Err(plan_err("dose-d8 panel differs from the shakedown pin"));
    }
    if let Some(predecessor) = &manifest.replaces {
        let expected = Some((predecessor.clone(), manifest.retired.clone()));
        if replacements.remove(&manifest.set) != expected {
            return Err(plan_err(
                "shakedown restart entry does not authorize this predecessor and retired range",
            ));
        }
    }
    verify_lead_history(pin, &raw, &entries)?;
    Ok(kind)
}
