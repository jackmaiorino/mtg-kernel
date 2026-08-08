//! Runtime capture of the validator-frozen strict source-tree recipe.
//!
//! The projection binds the committed tree of `HEAD`. Working-tree changes are
//! deliberately represented only by the raw porcelain-status digest and clean
//! flag. The capture contains no repository path and is suitable for embedding
//! unchanged in trainer, runner, and evaluator records.

use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::error::Error;
use std::ffi::OsStr;
use std::fmt;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Frozen public recipe identity.
pub const STRICT_SOURCE_TREE_RECIPE_IDENTITY_V1: &str = "mtg-kernel-strict-source-tree-sha256-v1";

/// SHA-256 of `collab/STRICT-SOURCE-TREE-RECIPE-V1.md` as frozen by the
/// validator. The recipe document is intentionally not copied into this crate.
pub const STRICT_SOURCE_TREE_RECIPE_SHA256_V1: &str =
    "13ab31b8e4810d683007182d1b5fc3b76db0b9761c877a6e78880c0cadf3fece";

/// Byte length of the same frozen recipe document whose digest is above.
pub const STRICT_SOURCE_TREE_RECIPE_BYTE_COUNT_V1: u64 = 5_847;

const TRACKED_TREE_HASH_CONTRACT_V1: &str =
    "git-ls-tree-r-z-path-mode-type-framed-blob-content-or-gitlink-oid-sha256/v1";

/// Immutable six-field capture defined by the frozen recipe.
///
/// Fields are private so callers cannot manufacture an attestation. Read-only
/// accessors and `Serialize` expose exactly the values future science records
/// need without retaining the repository path or raw status bytes.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct StrictSourceTreeCaptureV1 {
    source_tree_recipe_identity: &'static str,
    source_tree_recipe_sha256: &'static str,
    git_commit: String,
    source_tree_sha256: String,
    worktree_clean: bool,
    git_status_sha256: String,
}

impl StrictSourceTreeCaptureV1 {
    pub const fn source_tree_recipe_identity(&self) -> &'static str {
        self.source_tree_recipe_identity
    }

    pub const fn source_tree_recipe_sha256(&self) -> &'static str {
        self.source_tree_recipe_sha256
    }

    pub fn git_commit(&self) -> &str {
        &self.git_commit
    }

    pub fn source_tree_sha256(&self) -> &str {
        &self.source_tree_sha256
    }

    pub const fn worktree_clean(&self) -> bool {
        self.worktree_clean
    }

    pub fn git_status_sha256(&self) -> &str {
        &self.git_status_sha256
    }
}

/// Stable error classification for fail-closed source capture and validation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StrictSourceTreeAttestationErrorKindV1 {
    GitInvocation,
    GitCommand,
    MalformedHead,
    HeadChangedDuringCapture,
    MalformedTreeListing,
    UnsupportedTreeEntry,
    BlobBatchInvocation,
    BlobBatchCommand,
    MalformedBlobBatch,
    LengthOverflow,
    RepositoryRootCoherence,
    InvalidExpectation,
    RecipeIdentityMismatch,
    WorktreeDirty,
    CommitMismatch,
    TreeMismatch,
    PostflightMismatch,
}

/// Privacy-safe source-attestation error.
///
/// The error stores only a stable code and never stores the repository path,
/// command output, tracked paths, or status bytes.
#[derive(Debug)]
pub struct StrictSourceTreeAttestationErrorV1 {
    kind: StrictSourceTreeAttestationErrorKindV1,
    code: &'static str,
}

impl StrictSourceTreeAttestationErrorV1 {
    const fn new(kind: StrictSourceTreeAttestationErrorKindV1, code: &'static str) -> Self {
        Self { kind, code }
    }

    pub const fn kind(&self) -> StrictSourceTreeAttestationErrorKindV1 {
        self.kind
    }

    pub const fn code(&self) -> &'static str {
        self.code
    }
}

impl fmt::Display for StrictSourceTreeAttestationErrorV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "strict source-tree attestation failed: {}",
            self.code
        )
    }
}

impl Error for StrictSourceTreeAttestationErrorV1 {}

#[derive(Debug)]
struct GitTreeEntryV1 {
    mode: Vec<u8>,
    kind: Vec<u8>,
    object_id: String,
    path: Vec<u8>,
}

fn attestation_error(
    kind: StrictSourceTreeAttestationErrorKindV1,
    code: &'static str,
) -> StrictSourceTreeAttestationErrorV1 {
    StrictSourceTreeAttestationErrorV1::new(kind, code)
}

fn is_git_environment_name(name: &OsStr) -> bool {
    let bytes = name.as_encoded_bytes();
    bytes.len() >= 4
        && bytes[0].eq_ignore_ascii_case(&b'g')
        && bytes[1].eq_ignore_ascii_case(&b'i')
        && bytes[2].eq_ignore_ascii_case(&b't')
        && bytes[3] == b'_'
}

fn sanitized_git_command(git_program: &OsStr) -> Command {
    let mut command = Command::new(git_program);
    command.env_clear();
    command
        .envs(std::env::vars_os().filter(|(name, _)| !is_git_environment_name(name.as_os_str())));
    command
}

fn command_at_repo(
    git_program: &OsStr,
    repo_root: &Path,
    args: &[&str],
) -> Result<Vec<u8>, StrictSourceTreeAttestationErrorV1> {
    let output = sanitized_git_command(git_program)
        .arg("-C")
        .arg(repo_root)
        .args(args)
        .output()
        .map_err(|_| {
            attestation_error(
                StrictSourceTreeAttestationErrorKindV1::GitInvocation,
                "git_invocation_failed",
            )
        })?;
    if !output.status.success() {
        return Err(attestation_error(
            StrictSourceTreeAttestationErrorKindV1::GitCommand,
            "git_command_failed",
        ));
    }
    Ok(output.stdout)
}

fn parse_git_toplevel(bytes: &[u8]) -> Result<PathBuf, StrictSourceTreeAttestationErrorV1> {
    let without_lf = bytes.strip_suffix(b"\n").ok_or_else(|| {
        attestation_error(
            StrictSourceTreeAttestationErrorKindV1::RepositoryRootCoherence,
            "git_toplevel_missing_lf",
        )
    })?;
    let candidate = without_lf.strip_suffix(b"\r").unwrap_or(without_lf);
    if candidate.is_empty() || candidate.contains(&0) {
        return Err(attestation_error(
            StrictSourceTreeAttestationErrorKindV1::RepositoryRootCoherence,
            "git_toplevel_malformed",
        ));
    }

    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        Ok(PathBuf::from(std::ffi::OsString::from_vec(
            candidate.to_vec(),
        )))
    }
    #[cfg(not(unix))]
    {
        let candidate = std::str::from_utf8(candidate).map_err(|_| {
            attestation_error(
                StrictSourceTreeAttestationErrorKindV1::RepositoryRootCoherence,
                "git_toplevel_not_utf8",
            )
        })?;
        Ok(PathBuf::from(candidate))
    }
}

fn require_canonical_repository_root(
    git_program: &OsStr,
    repo_root: &Path,
) -> Result<PathBuf, StrictSourceTreeAttestationErrorV1> {
    let requested_root = std::fs::canonicalize(repo_root).map_err(|_| {
        attestation_error(
            StrictSourceTreeAttestationErrorKindV1::RepositoryRootCoherence,
            "requested_repository_root_canonicalization_failed",
        )
    })?;
    let reported_root = parse_git_toplevel(&command_at_repo(
        git_program,
        &requested_root,
        &["rev-parse", "--show-toplevel"],
    )?)?;
    let reported_root = std::fs::canonicalize(reported_root).map_err(|_| {
        attestation_error(
            StrictSourceTreeAttestationErrorKindV1::RepositoryRootCoherence,
            "reported_repository_root_canonicalization_failed",
        )
    })?;
    if requested_root != reported_root {
        return Err(attestation_error(
            StrictSourceTreeAttestationErrorKindV1::RepositoryRootCoherence,
            "requested_repository_root_mismatch",
        ));
    }
    Ok(requested_root)
}

fn parse_head(bytes: &[u8]) -> Result<String, StrictSourceTreeAttestationErrorV1> {
    let without_lf = bytes.strip_suffix(b"\n").unwrap_or(bytes);
    let candidate = without_lf.strip_suffix(b"\r").unwrap_or(without_lf);
    if candidate.len() != 40
        || !candidate
            .iter()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
    {
        return Err(attestation_error(
            StrictSourceTreeAttestationErrorKindV1::MalformedHead,
            "git_head_not_full_lowercase_sha1",
        ));
    }
    String::from_utf8(candidate.to_vec()).map_err(|_| {
        attestation_error(
            StrictSourceTreeAttestationErrorKindV1::MalformedHead,
            "git_head_not_ascii",
        )
    })
}

fn validate_tree_entry(entry: &GitTreeEntryV1) -> Result<(), StrictSourceTreeAttestationErrorV1> {
    let supported = match entry.kind.as_slice() {
        b"blob" => matches!(entry.mode.as_slice(), b"100644" | b"100755" | b"120000"),
        b"commit" => entry.mode == b"160000",
        _ => false,
    };
    if !supported {
        return Err(attestation_error(
            StrictSourceTreeAttestationErrorKindV1::UnsupportedTreeEntry,
            "unsupported_tree_entry_mode_or_type",
        ));
    }
    Ok(())
}

fn parse_tree_entries(
    bytes: &[u8],
) -> Result<Vec<GitTreeEntryV1>, StrictSourceTreeAttestationErrorV1> {
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    if bytes.last() != Some(&0) {
        return Err(attestation_error(
            StrictSourceTreeAttestationErrorKindV1::MalformedTreeListing,
            "tree_listing_missing_nul_terminator",
        ));
    }

    let mut entries = Vec::new();
    let mut seen_paths = BTreeSet::new();
    for record in bytes[..bytes.len() - 1].split(|byte| *byte == 0) {
        if record.is_empty() {
            return Err(attestation_error(
                StrictSourceTreeAttestationErrorKindV1::MalformedTreeListing,
                "tree_listing_empty_record",
            ));
        }
        let tab = record
            .iter()
            .position(|byte| *byte == b'\t')
            .ok_or_else(|| {
                attestation_error(
                    StrictSourceTreeAttestationErrorKindV1::MalformedTreeListing,
                    "tree_record_missing_tab",
                )
            })?;
        let path = &record[tab + 1..];
        let mut metadata = record[..tab].split(|byte| *byte == b' ');
        let mode = metadata.next().unwrap_or_default();
        let kind = metadata.next().unwrap_or_default();
        let object_id = metadata.next().unwrap_or_default();
        if metadata.next().is_some()
            || mode.is_empty()
            || kind.is_empty()
            || object_id.len() != 40
            || path.is_empty()
            || !object_id
                .iter()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
        {
            return Err(attestation_error(
                StrictSourceTreeAttestationErrorKindV1::MalformedTreeListing,
                "tree_record_malformed",
            ));
        }
        if !seen_paths.insert(path.to_vec()) {
            return Err(attestation_error(
                StrictSourceTreeAttestationErrorKindV1::MalformedTreeListing,
                "tree_listing_duplicate_path",
            ));
        }
        let entry = GitTreeEntryV1 {
            mode: mode.to_vec(),
            kind: kind.to_vec(),
            object_id: String::from_utf8(object_id.to_vec()).map_err(|_| {
                attestation_error(
                    StrictSourceTreeAttestationErrorKindV1::MalformedTreeListing,
                    "tree_object_id_not_ascii",
                )
            })?,
            path: path.to_vec(),
        };
        validate_tree_entry(&entry)?;
        entries.push(entry);
    }
    // The recipe consumes Git's canonical emission order exactly. Do not sort.
    Ok(entries)
}

fn blob_contents(
    git_program: &OsStr,
    repo_root: &Path,
    entries: &[GitTreeEntryV1],
) -> Result<Vec<Option<Vec<u8>>>, StrictSourceTreeAttestationErrorV1> {
    if entries.iter().all(|entry| entry.kind != b"blob") {
        return Ok(entries.iter().map(|_| None).collect());
    }

    let mut child = sanitized_git_command(git_program)
        .arg("-C")
        .arg(repo_root)
        .args(["cat-file", "--batch"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| {
            attestation_error(
                StrictSourceTreeAttestationErrorKindV1::BlobBatchInvocation,
                "git_cat_file_invocation_failed",
            )
        })?;
    let mut stdin = child.stdin.take().ok_or_else(|| {
        attestation_error(
            StrictSourceTreeAttestationErrorKindV1::BlobBatchInvocation,
            "git_cat_file_stdin_unavailable",
        )
    })?;

    let (output, write_result) = std::thread::scope(|scope| {
        let writer = scope.spawn(move || -> std::io::Result<()> {
            for entry in entries.iter().filter(|entry| entry.kind == b"blob") {
                stdin.write_all(entry.object_id.as_bytes())?;
                stdin.write_all(b"\n")?;
            }
            drop(stdin);
            Ok(())
        });
        let output = child.wait_with_output();
        let write_result = writer.join();
        (output, write_result)
    });
    write_result
        .map_err(|_| {
            attestation_error(
                StrictSourceTreeAttestationErrorKindV1::BlobBatchCommand,
                "git_cat_file_writer_panicked",
            )
        })?
        .map_err(|_| {
            attestation_error(
                StrictSourceTreeAttestationErrorKindV1::BlobBatchCommand,
                "git_cat_file_request_failed",
            )
        })?;
    let output = output.map_err(|_| {
        attestation_error(
            StrictSourceTreeAttestationErrorKindV1::BlobBatchCommand,
            "git_cat_file_wait_failed",
        )
    })?;
    if !output.status.success() {
        return Err(attestation_error(
            StrictSourceTreeAttestationErrorKindV1::BlobBatchCommand,
            "git_cat_file_failed",
        ));
    }

    let mut cursor = 0usize;
    let mut contents = Vec::with_capacity(entries.len());
    for entry in entries {
        if entry.kind == b"commit" {
            contents.push(None);
            continue;
        }
        let header_relative_end = output.stdout[cursor..]
            .iter()
            .position(|byte| *byte == b'\n')
            .ok_or_else(|| {
                attestation_error(
                    StrictSourceTreeAttestationErrorKindV1::MalformedBlobBatch,
                    "git_cat_file_header_missing_lf",
                )
            })?;
        let header_end = cursor.checked_add(header_relative_end).ok_or_else(|| {
            attestation_error(
                StrictSourceTreeAttestationErrorKindV1::LengthOverflow,
                "git_cat_file_header_offset_overflow",
            )
        })?;
        let header = std::str::from_utf8(&output.stdout[cursor..header_end]).map_err(|_| {
            attestation_error(
                StrictSourceTreeAttestationErrorKindV1::MalformedBlobBatch,
                "git_cat_file_header_not_ascii",
            )
        })?;
        let mut fields = header.split(' ');
        let returned_id = fields.next().unwrap_or_default();
        let returned_kind = fields.next().unwrap_or_default();
        let size = fields
            .next()
            .ok_or_else(|| {
                attestation_error(
                    StrictSourceTreeAttestationErrorKindV1::MalformedBlobBatch,
                    "git_cat_file_header_missing_size",
                )
            })?
            .parse::<u64>()
            .map_err(|_| {
                attestation_error(
                    StrictSourceTreeAttestationErrorKindV1::MalformedBlobBatch,
                    "git_cat_file_size_not_u64",
                )
            })?;
        if fields.next().is_some() || returned_id != entry.object_id || returned_kind != "blob" {
            return Err(attestation_error(
                StrictSourceTreeAttestationErrorKindV1::MalformedBlobBatch,
                "git_cat_file_metadata_mismatch",
            ));
        }
        let size = usize::try_from(size).map_err(|_| {
            attestation_error(
                StrictSourceTreeAttestationErrorKindV1::LengthOverflow,
                "git_cat_file_size_does_not_fit_usize",
            )
        })?;
        let content_start = header_end.checked_add(1).ok_or_else(|| {
            attestation_error(
                StrictSourceTreeAttestationErrorKindV1::LengthOverflow,
                "git_cat_file_content_offset_overflow",
            )
        })?;
        let content_end = content_start.checked_add(size).ok_or_else(|| {
            attestation_error(
                StrictSourceTreeAttestationErrorKindV1::LengthOverflow,
                "git_cat_file_content_length_overflow",
            )
        })?;
        if content_end >= output.stdout.len() || output.stdout[content_end] != b'\n' {
            return Err(attestation_error(
                StrictSourceTreeAttestationErrorKindV1::MalformedBlobBatch,
                "git_cat_file_content_truncated",
            ));
        }
        contents.push(Some(output.stdout[content_start..content_end].to_vec()));
        cursor = content_end + 1;
    }
    if cursor != output.stdout.len() {
        return Err(attestation_error(
            StrictSourceTreeAttestationErrorKindV1::MalformedBlobBatch,
            "git_cat_file_unconsumed_output",
        ));
    }
    Ok(contents)
}

fn hash_frame(hasher: &mut Sha256, bytes: &[u8]) -> Result<(), StrictSourceTreeAttestationErrorV1> {
    let length = u64::try_from(bytes.len()).map_err(|_| {
        attestation_error(
            StrictSourceTreeAttestationErrorKindV1::LengthOverflow,
            "source_tree_frame_length_overflow",
        )
    })?;
    hasher.update(length.to_be_bytes());
    hasher.update(bytes);
    Ok(())
}

fn hash_tree_projection(
    entries: &[GitTreeEntryV1],
    contents: &[Option<Vec<u8>>],
) -> Result<String, StrictSourceTreeAttestationErrorV1> {
    if entries.len() != contents.len() {
        return Err(attestation_error(
            StrictSourceTreeAttestationErrorKindV1::MalformedBlobBatch,
            "tree_entry_content_count_mismatch",
        ));
    }
    let entry_count = u64::try_from(entries.len()).map_err(|_| {
        attestation_error(
            StrictSourceTreeAttestationErrorKindV1::LengthOverflow,
            "source_tree_entry_count_overflow",
        )
    })?;
    let mut hasher = Sha256::new();
    hasher.update(TRACKED_TREE_HASH_CONTRACT_V1.as_bytes());
    hasher.update([0]);
    hasher.update(entry_count.to_be_bytes());
    for (entry, content) in entries.iter().zip(contents) {
        hash_frame(&mut hasher, &entry.path)?;
        hash_frame(&mut hasher, &entry.mode)?;
        hash_frame(&mut hasher, &entry.kind)?;
        match (entry.kind.as_slice(), content) {
            (b"blob", Some(bytes)) => hash_frame(&mut hasher, bytes)?,
            (b"commit", None) => hash_frame(&mut hasher, entry.object_id.as_bytes())?,
            _ => {
                return Err(attestation_error(
                    StrictSourceTreeAttestationErrorKindV1::MalformedBlobBatch,
                    "tree_entry_content_kind_mismatch",
                ));
            }
        }
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn capture_with_git_program(
    repo_root: &Path,
    git_program: &OsStr,
) -> Result<StrictSourceTreeCaptureV1, StrictSourceTreeAttestationErrorV1> {
    let repo_root = require_canonical_repository_root(git_program, repo_root)?;
    let commit = parse_head(&command_at_repo(
        git_program,
        &repo_root,
        &["rev-parse", "--verify", "HEAD^{commit}"],
    )?)?;
    let listing = command_at_repo(
        git_program,
        &repo_root,
        &["ls-tree", "-r", "-z", "--full-tree", &commit],
    )?;
    let entries = parse_tree_entries(&listing)?;
    let contents = blob_contents(git_program, &repo_root, &entries)?;
    let source_tree_sha256 = hash_tree_projection(&entries, &contents)?;
    let status_bytes = command_at_repo(
        git_program,
        &repo_root,
        &["status", "--porcelain=v1", "--untracked-files=all"],
    )?;
    let final_commit = parse_head(&command_at_repo(
        git_program,
        &repo_root,
        &["rev-parse", "--verify", "HEAD^{commit}"],
    )?)?;
    if final_commit != commit {
        return Err(attestation_error(
            StrictSourceTreeAttestationErrorKindV1::HeadChangedDuringCapture,
            "git_head_changed_during_capture",
        ));
    }
    Ok(StrictSourceTreeCaptureV1 {
        source_tree_recipe_identity: STRICT_SOURCE_TREE_RECIPE_IDENTITY_V1,
        source_tree_recipe_sha256: STRICT_SOURCE_TREE_RECIPE_SHA256_V1,
        git_commit: commit,
        source_tree_sha256,
        worktree_clean: status_bytes.is_empty(),
        git_status_sha256: sha256_hex(&status_bytes),
    })
}

/// Captures the frozen committed-tree and raw-status tuple for `repo_root`.
///
/// Git is invoked directly as `git -C repo_root ...`; no shell is involved and
/// every inherited `GIT_*` variable is stripped case-insensitively. The
/// canonical requested root must equal Git's reported top level. Any process,
/// repository, parsing, object, or coherence failure returns an error and no
/// partial capture.
pub fn capture_strict_source_tree_v1(
    repo_root: impl AsRef<Path>,
) -> Result<StrictSourceTreeCaptureV1, StrictSourceTreeAttestationErrorV1> {
    capture_with_git_program(repo_root.as_ref(), OsStr::new("git"))
}

fn is_lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// Applies the strict preflight policy to a captured tuple.
///
/// Expected values are caller-owned immutable bindings. The capture must use
/// the frozen recipe, be clean, and exactly match both the full commit and the
/// committed-tree SHA-256.
pub fn require_strict_source_preflight_v1(
    capture: &StrictSourceTreeCaptureV1,
    expected_git_commit: &str,
    expected_source_tree_sha256: &str,
) -> Result<(), StrictSourceTreeAttestationErrorV1> {
    if !is_lower_hex(expected_git_commit, 40) || !is_lower_hex(expected_source_tree_sha256, 64) {
        return Err(attestation_error(
            StrictSourceTreeAttestationErrorKindV1::InvalidExpectation,
            "expected_source_binding_not_canonical_lower_hex",
        ));
    }
    if capture.source_tree_recipe_identity != STRICT_SOURCE_TREE_RECIPE_IDENTITY_V1
        || capture.source_tree_recipe_sha256 != STRICT_SOURCE_TREE_RECIPE_SHA256_V1
    {
        return Err(attestation_error(
            StrictSourceTreeAttestationErrorKindV1::RecipeIdentityMismatch,
            "source_tree_recipe_identity_mismatch",
        ));
    }
    if !capture.worktree_clean {
        return Err(attestation_error(
            StrictSourceTreeAttestationErrorKindV1::WorktreeDirty,
            "strict_source_worktree_not_clean",
        ));
    }
    if capture.git_commit != expected_git_commit {
        return Err(attestation_error(
            StrictSourceTreeAttestationErrorKindV1::CommitMismatch,
            "strict_source_commit_mismatch",
        ));
    }
    if capture.source_tree_sha256 != expected_source_tree_sha256 {
        return Err(attestation_error(
            StrictSourceTreeAttestationErrorKindV1::TreeMismatch,
            "strict_source_tree_sha256_mismatch",
        ));
    }
    Ok(())
}

/// Requires exact equality of the complete six-field before/after tuple.
pub fn require_strict_source_postflight_equality_v1(
    before: &StrictSourceTreeCaptureV1,
    after: &StrictSourceTreeCaptureV1,
) -> Result<(), StrictSourceTreeAttestationErrorV1> {
    if before != after {
        return Err(attestation_error(
            StrictSourceTreeAttestationErrorKindV1::PostflightMismatch,
            "strict_source_postflight_tuple_mismatch",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_REPO_ORDINAL: AtomicU64 = AtomicU64::new(0);
    const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    const ROUTING_CHILD_MODE: &str = "MTG_KERNEL_STRICT_SOURCE_ROUTING_CHILD_V1";
    const ROUTING_REQUESTED_ROOT: &str = "MTG_KERNEL_STRICT_SOURCE_REQUESTED_ROOT_V1";
    const ROUTING_EXPECTED_COMMIT: &str = "MTG_KERNEL_STRICT_SOURCE_EXPECTED_COMMIT_V1";
    const ROUTING_EXPECTED_TREE: &str = "MTG_KERNEL_STRICT_SOURCE_EXPECTED_TREE_V1";

    struct TestRepoV1 {
        root: PathBuf,
    }

    impl TestRepoV1 {
        fn new(initial_file: &str, initial_bytes: &[u8]) -> Self {
            let ordinal = TEST_REPO_ORDINAL.fetch_add(1, Ordering::Relaxed);
            let root = std::env::temp_dir().join(format!(
                "mtg-kernel-strict-source-v1-{}-{ordinal}",
                std::process::id()
            ));
            fs::create_dir(&root).unwrap();
            let repo = Self { root };
            repo.git(&["init", "--quiet"]);
            repo.git(&["config", "user.name", "mtg-kernel-test"]);
            repo.git(&["config", "user.email", "mtg-kernel-test@example.invalid"]);
            repo.git(&["config", "commit.gpgsign", "false"]);
            repo.git(&["config", "core.filemode", "false"]);
            fs::write(repo.root.join(".gitignore"), b"ignored.tmp\n").unwrap();
            fs::write(repo.root.join(initial_file), initial_bytes).unwrap();
            repo.commit_all("initial");
            repo
        }

        fn git(&self, args: &[&str]) -> Vec<u8> {
            let output = Command::new("git")
                .arg("-C")
                .arg(&self.root)
                .args(args)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "git {:?} failed: {}",
                args,
                String::from_utf8_lossy(&output.stderr)
            );
            output.stdout
        }

        fn commit_all(&self, message: &str) {
            self.git(&["add", "--all"]);
            self.git(&["commit", "--quiet", "-m", message]);
        }

        fn capture(&self) -> StrictSourceTreeCaptureV1 {
            capture_strict_source_tree_v1(&self.root).unwrap()
        }
    }

    impl Drop for TestRepoV1 {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn inherited_git_routing_is_stripped_without_mutating_global_environment() {
        if std::env::var_os(ROUTING_CHILD_MODE).is_some() {
            let requested_root = PathBuf::from(
                std::env::var_os(ROUTING_REQUESTED_ROOT)
                    .expect("child receives requested repository root"),
            );
            let expected_commit =
                std::env::var(ROUTING_EXPECTED_COMMIT).expect("child receives expected commit");
            let expected_tree =
                std::env::var(ROUTING_EXPECTED_TREE).expect("child receives expected tree");
            let capture = capture_strict_source_tree_v1(requested_root).unwrap();
            assert_eq!(capture.git_commit(), expected_commit);
            assert_eq!(capture.source_tree_sha256(), expected_tree);
            assert!(capture.worktree_clean());
            return;
        }

        let requested = TestRepoV1::new("requested.txt", b"requested repository\n");
        let redirected = TestRepoV1::new("redirected.txt", b"redirected repository\n");
        let expected = requested.capture();
        let redirected_capture = redirected.capture();
        assert_ne!(expected.git_commit(), redirected_capture.git_commit());
        assert_ne!(
            expected.source_tree_sha256(),
            redirected_capture.source_tree_sha256()
        );

        let output = Command::new(std::env::current_exe().unwrap())
            .arg("inherited_git_routing_is_stripped_without_mutating_global_environment")
            .arg("--nocapture")
            .arg("--test-threads=1")
            .env(ROUTING_CHILD_MODE, "1")
            .env(ROUTING_REQUESTED_ROOT, &requested.root)
            .env(ROUTING_EXPECTED_COMMIT, expected.git_commit())
            .env(ROUTING_EXPECTED_TREE, expected.source_tree_sha256())
            .env("GIT_DIR", redirected.root.join(".git"))
            .env("GIT_WORK_TREE", &redirected.root)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "isolated routing child failed:\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn repository_root_must_be_the_canonical_git_toplevel() {
        let repo = TestRepoV1::new("tracked.txt", b"committed\n");
        let nested = repo.root.join("nested");
        fs::create_dir(&nested).unwrap();
        let error = capture_strict_source_tree_v1(&nested).unwrap_err();
        assert_eq!(
            error.kind(),
            StrictSourceTreeAttestationErrorKindV1::RepositoryRootCoherence
        );
        assert_eq!(error.code(), "requested_repository_root_mismatch");
        assert!(!error.to_string().contains(&nested.display().to_string()));
    }

    #[test]
    fn git_environment_prefix_detection_is_ascii_case_insensitive() {
        assert!(is_git_environment_name(OsStr::new("GIT_DIR")));
        assert!(is_git_environment_name(OsStr::new("gIt_work_tree")));
        assert!(is_git_environment_name(OsStr::new("Git_")));
        assert!(!is_git_environment_name(OsStr::new("GIT")));
        assert!(!is_git_environment_name(OsStr::new("GITX_DIR")));
        assert!(!is_git_environment_name(OsStr::new("MTG_GIT_DIR")));
    }

    #[test]
    fn capture_is_stable_across_dirty_worktree_changes_but_status_is_not() {
        let repo = TestRepoV1::new("tracked.txt", b"committed\n");
        let clean = repo.capture();
        assert!(clean.worktree_clean());
        assert_eq!(clean.git_status_sha256(), EMPTY_SHA256);
        require_strict_source_preflight_v1(&clean, clean.git_commit(), clean.source_tree_sha256())
            .unwrap();

        fs::write(repo.root.join("tracked.txt"), b"dirty tracked\n").unwrap();
        let tracked_dirty = repo.capture();
        assert_eq!(tracked_dirty.git_commit(), clean.git_commit());
        assert_eq!(
            tracked_dirty.source_tree_sha256(),
            clean.source_tree_sha256()
        );
        assert!(!tracked_dirty.worktree_clean());
        let raw_tracked_status = repo.git(&["status", "--porcelain=v1", "--untracked-files=all"]);
        assert_eq!(
            tracked_dirty.git_status_sha256(),
            sha256_hex(&raw_tracked_status)
        );
        assert_ne!(tracked_dirty.git_status_sha256(), clean.git_status_sha256());
        assert_eq!(
            require_strict_source_preflight_v1(
                &tracked_dirty,
                clean.git_commit(),
                clean.source_tree_sha256(),
            )
            .unwrap_err()
            .kind(),
            StrictSourceTreeAttestationErrorKindV1::WorktreeDirty
        );

        fs::write(repo.root.join("tracked.txt"), b"committed\n").unwrap();
        fs::write(repo.root.join("untracked.txt"), b"untracked\n").unwrap();
        let untracked_dirty = repo.capture();
        assert_eq!(untracked_dirty.git_commit(), clean.git_commit());
        assert_eq!(
            untracked_dirty.source_tree_sha256(),
            clean.source_tree_sha256()
        );
        assert!(!untracked_dirty.worktree_clean());
        assert_ne!(
            untracked_dirty.git_status_sha256(),
            tracked_dirty.git_status_sha256()
        );
        assert_eq!(
            require_strict_source_postflight_equality_v1(&clean, &untracked_dirty)
                .unwrap_err()
                .kind(),
            StrictSourceTreeAttestationErrorKindV1::PostflightMismatch
        );
    }

    #[test]
    fn ignored_files_are_outside_both_projection_and_clean_gate() {
        let repo = TestRepoV1::new("tracked.txt", b"committed\n");
        let before = repo.capture();
        fs::write(repo.root.join("ignored.tmp"), b"build output\n").unwrap();
        let after = repo.capture();
        assert_eq!(after, before);
        require_strict_source_postflight_equality_v1(&before, &after).unwrap();
    }

    #[test]
    fn committed_content_path_and_mode_each_change_the_tree_digest() {
        let repo = TestRepoV1::new("tracked.txt", b"version one\n");
        let initial = repo.capture();

        fs::write(repo.root.join("tracked.txt"), b"version two\n").unwrap();
        repo.commit_all("content");
        let content = repo.capture();
        assert!(content.worktree_clean());
        assert_ne!(content.source_tree_sha256(), initial.source_tree_sha256());

        repo.git(&["mv", "tracked.txt", "renamed.txt"]);
        repo.commit_all("path");
        let path = repo.capture();
        assert!(path.worktree_clean());
        assert_ne!(path.source_tree_sha256(), content.source_tree_sha256());

        repo.git(&["update-index", "--chmod=+x", "renamed.txt"]);
        repo.git(&["commit", "--quiet", "-m", "mode"]);
        let mode = repo.capture();
        assert!(mode.worktree_clean());
        assert_ne!(mode.source_tree_sha256(), path.source_tree_sha256());
    }

    #[test]
    fn exact_framing_preserves_emission_order_and_matches_independent_bytes() {
        let entries = vec![
            GitTreeEntryV1 {
                mode: b"100644".to_vec(),
                kind: b"blob".to_vec(),
                object_id: "1111111111111111111111111111111111111111".into(),
                path: b"z/file\tname".to_vec(),
            },
            GitTreeEntryV1 {
                mode: b"160000".to_vec(),
                kind: b"commit".to_vec(),
                object_id: "abcdefabcdefabcdefabcdefabcdefabcdefabcd".into(),
                path: b"a-submodule".to_vec(),
            },
        ];
        let contents = vec![Some(b"blob\0bytes\n".to_vec()), None];
        let actual = hash_tree_projection(&entries, &contents).unwrap();

        let mut framed = Vec::new();
        framed.extend_from_slice(TRACKED_TREE_HASH_CONTRACT_V1.as_bytes());
        framed.push(0);
        framed.extend_from_slice(&2u64.to_be_bytes());
        for bytes in [
            b"z/file\tname".as_slice(),
            b"100644".as_slice(),
            b"blob".as_slice(),
            b"blob\0bytes\n".as_slice(),
            b"a-submodule".as_slice(),
            b"160000".as_slice(),
            b"commit".as_slice(),
            b"abcdefabcdefabcdefabcdefabcdefabcdefabcd".as_slice(),
        ] {
            framed.extend_from_slice(&u64::try_from(bytes.len()).unwrap().to_be_bytes());
            framed.extend_from_slice(bytes);
        }
        let independent = format!("{:x}", Sha256::digest(&framed));
        assert_eq!(actual, independent);
        assert_eq!(
            actual,
            "4c0ee922125d503a74f26a1f744dc35ad1324873211be26c4d7aabcf740ad40b"
        );

        let reversed_entries = vec![
            GitTreeEntryV1 {
                mode: entries[1].mode.clone(),
                kind: entries[1].kind.clone(),
                object_id: entries[1].object_id.clone(),
                path: entries[1].path.clone(),
            },
            GitTreeEntryV1 {
                mode: entries[0].mode.clone(),
                kind: entries[0].kind.clone(),
                object_id: entries[0].object_id.clone(),
                path: entries[0].path.clone(),
            },
        ];
        let reversed_contents = vec![None, contents[0].clone()];
        assert_ne!(
            hash_tree_projection(&reversed_entries, &reversed_contents).unwrap(),
            actual
        );
    }

    #[test]
    fn malformed_tree_and_blob_metadata_fail_closed() {
        let oid = "1111111111111111111111111111111111111111";
        let emitted = format!(
            "100644 blob {oid}\tz/path\twith-tab\0\
             100755 blob {oid}\ta-path\0"
        );
        let parsed = parse_tree_entries(emitted.as_bytes()).unwrap();
        assert_eq!(parsed[0].path, b"z/path\twith-tab");
        assert_eq!(parsed[1].path, b"a-path");

        let missing_nul = format!("100644 blob {oid}\tpath");
        assert_eq!(
            parse_tree_entries(missing_nul.as_bytes())
                .unwrap_err()
                .kind(),
            StrictSourceTreeAttestationErrorKindV1::MalformedTreeListing
        );
        let unsupported = format!("040000 tree {oid}\tdirectory\0");
        assert_eq!(
            parse_tree_entries(unsupported.as_bytes())
                .unwrap_err()
                .kind(),
            StrictSourceTreeAttestationErrorKindV1::UnsupportedTreeEntry
        );
        assert_eq!(
            parse_head(b"not-a-commit\n").unwrap_err().kind(),
            StrictSourceTreeAttestationErrorKindV1::MalformedHead
        );

        let repo = TestRepoV1::new("tracked.txt", b"committed\n");
        let missing_object = GitTreeEntryV1 {
            mode: b"100644".to_vec(),
            kind: b"blob".to_vec(),
            object_id: "0000000000000000000000000000000000000000".into(),
            path: b"missing".to_vec(),
        };
        assert_eq!(
            blob_contents(OsStr::new("git"), &repo.root, &[missing_object])
                .unwrap_err()
                .kind(),
            StrictSourceTreeAttestationErrorKindV1::MalformedBlobBatch
        );
    }

    #[test]
    fn non_repository_and_git_invocation_failure_return_no_capture() {
        let ordinal = TEST_REPO_ORDINAL.fetch_add(1, Ordering::Relaxed);
        let directory = std::env::temp_dir().join(format!(
            "mtg-kernel-strict-source-nonrepo-{}-{ordinal}",
            std::process::id()
        ));
        fs::create_dir(&directory).unwrap();
        let non_repo = capture_strict_source_tree_v1(&directory).unwrap_err();
        assert_eq!(
            non_repo.kind(),
            StrictSourceTreeAttestationErrorKindV1::GitCommand
        );

        let missing_program = directory.join("git-program-that-does-not-exist");
        let invocation =
            capture_with_git_program(&directory, missing_program.as_os_str()).unwrap_err();
        assert_eq!(
            invocation.kind(),
            StrictSourceTreeAttestationErrorKindV1::GitInvocation
        );
        assert!(!invocation
            .to_string()
            .contains(&directory.display().to_string()));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn preflight_rejects_noncanonical_and_mismatched_expectations() {
        let repo = TestRepoV1::new("tracked.txt", b"committed\n");
        let capture = repo.capture();
        assert_eq!(
            require_strict_source_preflight_v1(
                &capture,
                "ABCDEFABCDEFABCDEFABCDEFABCDEFABCDEFABCD",
                capture.source_tree_sha256(),
            )
            .unwrap_err()
            .kind(),
            StrictSourceTreeAttestationErrorKindV1::InvalidExpectation
        );
        assert_eq!(
            require_strict_source_preflight_v1(
                &capture,
                "0000000000000000000000000000000000000000",
                capture.source_tree_sha256(),
            )
            .unwrap_err()
            .kind(),
            StrictSourceTreeAttestationErrorKindV1::CommitMismatch
        );
        assert_eq!(
            require_strict_source_preflight_v1(
                &capture,
                capture.git_commit(),
                "0000000000000000000000000000000000000000000000000000000000000000",
            )
            .unwrap_err()
            .kind(),
            StrictSourceTreeAttestationErrorKindV1::TreeMismatch
        );
    }

    #[test]
    fn serialized_capture_is_the_exact_six_field_tuple_without_paths() {
        let repo = TestRepoV1::new("tracked.txt", b"committed\n");
        let capture = repo.capture();
        let value = serde_json::to_value(&capture).unwrap();
        assert_eq!(value.as_object().unwrap().len(), 6);
        assert_eq!(
            value["source_tree_recipe_identity"],
            STRICT_SOURCE_TREE_RECIPE_IDENTITY_V1
        );
        assert_eq!(
            value["source_tree_recipe_sha256"],
            STRICT_SOURCE_TREE_RECIPE_SHA256_V1
        );
        assert_eq!(value["git_commit"], capture.git_commit());
        assert_eq!(value["source_tree_sha256"], capture.source_tree_sha256());
        assert_eq!(value["worktree_clean"], true);
        assert_eq!(value["git_status_sha256"], EMPTY_SHA256);
        let encoded = serde_json::to_string(&capture).unwrap();
        assert!(!encoded.contains(&repo.root.display().to_string()));
    }
}
