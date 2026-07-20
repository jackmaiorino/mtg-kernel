//! Idempotent native training Store V2 root-skeleton bootstrap and pre-run
//! recovery.
//!
//! Bootstrap admits exactly the frozen states B0 through B8, creates the lock
//! leaf and the four authoritative subdirectories one at a time in the exact
//! order `segments`, `checkpoints`, `heads`, `refs` under the exclusive range
//! lock, and deletes only a fully name/type-recognized `.run.json.stage-v2`
//! after the complete inventory plan succeeds. No unknown, malformed-name, or
//! out-of-state object is ever deleted, renamed, quarantined, truncated,
//! overwritten, or traversed; on every error already created skeleton members
//! remain for idempotent retry. Run-record byte authority, generation
//! publication, and resume orchestration live in later layers. On non-Windows
//! platforms the entry point returns the stable unsupported-platform error
//! before any filesystem access.

use crate::native_training_store_root_v2::ValidatedNativeTrainingStoreRootV2;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::path::Path;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeTrainingStoreBootstrapV2ErrorKind {
    UnsupportedPlatform,
    RootBasenameInvalid,
    ParentInvalid,
    VolumeInvalid,
    StoreBusy,
    LockInvalid,
    CorruptRoot,
    MutationFailed,
    IdentityChanged,
}

impl NativeTrainingStoreBootstrapV2ErrorKind {
    pub const fn code(self) -> &'static str {
        match self {
            Self::UnsupportedPlatform => "native-training-store-v2-unsupported-platform",
            Self::RootBasenameInvalid => "native-training-store-bootstrap-root-basename-invalid",
            Self::ParentInvalid => "native-training-store-bootstrap-parent-invalid",
            Self::VolumeInvalid => "native-training-store-bootstrap-volume-invalid",
            Self::StoreBusy => "native-training-store-busy",
            Self::LockInvalid => "native-training-store-bootstrap-lock-invalid",
            Self::CorruptRoot => "native-training-store-bootstrap-corrupt-root",
            Self::MutationFailed => "native-training-store-bootstrap-mutation-failed",
            Self::IdentityChanged => "native-training-store-bootstrap-identity-changed",
        }
    }
}

/// Redacted bootstrap error carrying only its classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeTrainingStoreBootstrapV2Error {
    kind: NativeTrainingStoreBootstrapV2ErrorKind,
}

impl NativeTrainingStoreBootstrapV2Error {
    pub const fn kind(self) -> NativeTrainingStoreBootstrapV2ErrorKind {
        self.kind
    }

    pub const fn code(self) -> &'static str {
        self.kind.code()
    }
}

impl Display for NativeTrainingStoreBootstrapV2Error {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.code())
    }
}

impl Error for NativeTrainingStoreBootstrapV2Error {}

type Result<T> = std::result::Result<T, NativeTrainingStoreBootstrapV2Error>;

const fn bootstrap_error_v2(
    kind: NativeTrainingStoreBootstrapV2ErrorKind,
) -> NativeTrainingStoreBootstrapV2Error {
    NativeTrainingStoreBootstrapV2Error { kind }
}

/// Terminal bootstrap admission after the skeleton is complete.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeTrainingStoreBootstrapOutcomeV2 {
    /// The complete empty skeleton exists and `run.json` is absent (B6).
    SkeletonReady,
    /// The complete skeleton exists and `run.json` is present (B8). Byte
    /// equality against the reconstructed run candidate is the caller's
    /// obligation before any generation mutation.
    RunAuthorityPresent,
}

/// A bootstrapped Store: the validated root plus its terminal admission.
#[derive(Debug)]
pub struct NativeTrainingStoreBootstrapV2 {
    root: ValidatedNativeTrainingStoreRootV2,
    outcome: NativeTrainingStoreBootstrapOutcomeV2,
}

impl NativeTrainingStoreBootstrapV2 {
    pub const fn outcome(&self) -> NativeTrainingStoreBootstrapOutcomeV2 {
        self.outcome
    }

    pub const fn root(&self) -> &ValidatedNativeTrainingStoreRootV2 {
        &self.root
    }

    pub fn into_root(self) -> ValidatedNativeTrainingStoreRootV2 {
        self.root
    }
}

/// Fault-injection boundaries after each bootstrap operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BootstrapBoundaryV2 {
    ParentValidated,
    RootCreated,
    RootReopened,
    LockCreated,
    LockValidated,
    LockAcquired,
    LockedRescan,
    DirectoryCreated(usize),
    DirectoryValidated(usize),
    RunStageDeleted,
}

/// Validate the caller-supplied final Store root basename.
///
/// The basename must be a single final component: not empty, `.`, or `..`,
/// with no separator, colon, wildcard, control byte, or alternate-data-stream
/// or URI syntax, no trailing dot or space, and not a reserved Windows device
/// basename (with or without an extension, including superscript digits).
pub fn validate_store_root_basename_v2(basename: &str) -> Result<()> {
    let invalid = bootstrap_error_v2(NativeTrainingStoreBootstrapV2ErrorKind::RootBasenameInvalid);
    if basename.is_empty() || basename == "." || basename == ".." {
        return Err(invalid);
    }
    if basename.chars().any(|character| {
        matches!(
            character,
            '\\' | '/' | ':' | '<' | '>' | '"' | '|' | '?' | '*'
        ) || character.is_control()
    }) {
        return Err(invalid);
    }
    if basename.ends_with('.') || basename.ends_with(' ') {
        return Err(invalid);
    }
    if reserved_device_base_v2(basename) {
        return Err(invalid);
    }
    Ok(())
}

fn reserved_device_base_v2(basename: &str) -> bool {
    let base = basename.split('.').next().unwrap_or_default();
    let upper = base.to_ascii_uppercase();
    if matches!(
        upper.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$" | "CLOCK$"
    ) {
        return true;
    }
    for prefix in ["COM", "LPT"] {
        if let Some(rest) = upper.strip_prefix(prefix) {
            let mut digits = rest.chars();
            if let (Some(digit), None) = (digits.next(), digits.next()) {
                if digit.is_ascii_digit() || matches!(digit, '\u{00B9}' | '\u{00B2}' | '\u{00B3}') {
                    return true;
                }
            }
        }
    }
    false
}

/// Bootstrap the Store root skeleton under `parent`, admitting only the
/// frozen B0 through B8 states, and return the validated root.
pub fn bootstrap_native_training_store_v2(
    parent: impl AsRef<Path>,
    root_basename: &str,
) -> Result<NativeTrainingStoreBootstrapV2> {
    bootstrap_native_training_store_with_hook_v2(parent.as_ref(), root_basename, |_| Ok(()))
}

fn bootstrap_native_training_store_with_hook_v2(
    parent: &Path,
    root_basename: &str,
    hook: impl FnMut(BootstrapBoundaryV2) -> Result<()>,
) -> Result<NativeTrainingStoreBootstrapV2> {
    validate_store_root_basename_v2(root_basename)?;
    #[cfg(windows)]
    {
        windows_bootstrap_v2::bootstrap_v2(parent, root_basename, hook)
    }
    #[cfg(not(windows))]
    {
        let _ = (parent, hook);
        Err(bootstrap_error_v2(
            NativeTrainingStoreBootstrapV2ErrorKind::UnsupportedPlatform,
        ))
    }
}

#[cfg(windows)]
mod windows_bootstrap_v2 {
    use super::{
        bootstrap_error_v2, BootstrapBoundaryV2, NativeTrainingStoreBootstrapOutcomeV2,
        NativeTrainingStoreBootstrapV2, NativeTrainingStoreBootstrapV2ErrorKind, Result,
    };
    use crate::native_training_store_layout_v2::{
        NATIVE_TRAINING_STORE_LOCK_LEAF_V2, NATIVE_TRAINING_STORE_SUBDIRECTORY_ORDER_V2,
    };
    use crate::native_training_store_root_v2::windows_store_root_v2::{
        basic_info_v2, final_path_v2, identity_v2, open_no_follow_v2, release_range_lock_v2,
        require_directory_attributes_v2, require_local_fixed_ntfs_v2, standard_info_v2,
        try_range_lock_v2, FileIdInfoV2, OwnedHandleV2, FILE_ATTRIBUTE_DIRECTORY_V2,
        FILE_ATTRIBUTE_REPARSE_POINT_V2, FILE_SHARE_DELETE_V2, FILE_SHARE_READ_V2,
        FILE_SHARE_WRITE_V2, GENERIC_READ_V2, RANGE_LOCK_HELD_RAW_OS_V2,
    };
    use crate::native_training_store_root_v2::{
        NativeTrainingStoreRootV2ErrorKind, ValidatedNativeTrainingStoreRootV2,
    };
    use std::fs;
    use std::path::{Path, PathBuf};

    const RUN_LEAF_V2: &str = "run.json";
    const RUN_STAGE_LEAF_V2: &str = ".run.json.stage-v2";
    const DIRECTORY_SHARE_V2: u32 = FILE_SHARE_READ_V2 | FILE_SHARE_WRITE_V2 | FILE_SHARE_DELETE_V2;

    /// Presence inventory admitted by the strict B-state classifier.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    struct RootInventoryV2 {
        lock_present: bool,
        directory_count: usize,
        run_stage_present: bool,
        run_present: bool,
    }

    struct HeldBootstrapLockV2<'handle> {
        handle: &'handle OwnedHandleV2,
    }

    impl Drop for HeldBootstrapLockV2<'_> {
        fn drop(&mut self) {
            release_range_lock_v2(self.handle);
        }
    }

    fn map_kind_v2(
        kind: NativeTrainingStoreBootstrapV2ErrorKind,
    ) -> impl Fn(
        crate::native_training_store_root_v2::NativeTrainingStoreRootV2Error,
    ) -> super::NativeTrainingStoreBootstrapV2Error {
        move |_| bootstrap_error_v2(kind)
    }

    /// Enumerate the root and admit only exact B-state member objects.
    fn classify_root_v2(root_path: &Path) -> Result<RootInventoryV2> {
        let corrupt = bootstrap_error_v2(NativeTrainingStoreBootstrapV2ErrorKind::CorruptRoot);
        let mut inventory = RootInventoryV2 {
            lock_present: false,
            directory_count: 0,
            run_stage_present: false,
            run_present: false,
        };
        let mut seen_directories = [false; 4];
        for entry in fs::read_dir(root_path).map_err(|_| corrupt)? {
            let entry = entry.map_err(|_| corrupt)?;
            let file_name = entry.file_name();
            let Some(leaf) = file_name.to_str() else {
                return Err(corrupt);
            };
            let file_type = entry.file_type().map_err(|_| corrupt)?;
            if file_type.is_symlink() {
                return Err(corrupt);
            }
            match leaf {
                NATIVE_TRAINING_STORE_LOCK_LEAF_V2 => {
                    if !file_type.is_file() {
                        return Err(corrupt);
                    }
                    inventory.lock_present = true;
                }
                RUN_LEAF_V2 => {
                    if !file_type.is_file() {
                        return Err(corrupt);
                    }
                    inventory.run_present = true;
                }
                RUN_STAGE_LEAF_V2 => {
                    if !file_type.is_file() {
                        return Err(corrupt);
                    }
                    inventory.run_stage_present = true;
                }
                _ => {
                    let position = NATIVE_TRAINING_STORE_SUBDIRECTORY_ORDER_V2
                        .iter()
                        .position(|directory| directory.basename() == Some(leaf));
                    let Some(position) = position else {
                        return Err(corrupt);
                    };
                    if !file_type.is_dir() {
                        return Err(corrupt);
                    }
                    seen_directories[position] = true;
                }
            }
        }
        let directory_count = seen_directories
            .iter()
            .position(|present| !present)
            .unwrap_or(4);
        if seen_directories[directory_count..].iter().any(|seen| *seen) {
            // A later directory exists without an earlier one: not a prefix.
            return Err(corrupt);
        }
        inventory.directory_count = directory_count;

        // Cross-member admission: the lock precedes directories, a run stage
        // requires the complete skeleton with `run.json` absent, and
        // `run.json` requires the complete skeleton.
        if inventory.directory_count > 0 && !inventory.lock_present {
            return Err(corrupt);
        }
        if inventory.run_stage_present
            && (inventory.directory_count != 4 || !inventory.lock_present)
        {
            return Err(corrupt);
        }
        if inventory.run_present && (inventory.directory_count != 4 || !inventory.lock_present) {
            return Err(corrupt);
        }
        Ok(inventory)
    }

    fn open_validated_directory_v2(
        path: &Path,
        expected_final_path: &Path,
        volume_serial_number: Option<u64>,
        kind: NativeTrainingStoreBootstrapV2ErrorKind,
    ) -> Result<(OwnedHandleV2, FileIdInfoV2)> {
        let root_kind = NativeTrainingStoreRootV2ErrorKind::RootInvalid;
        let handle = open_no_follow_v2(path, 0, DIRECTORY_SHARE_V2, true, root_kind)
            .map_err(map_kind_v2(kind))?;
        require_directory_attributes_v2(&handle, root_kind).map_err(map_kind_v2(kind))?;
        let resolved = final_path_v2(&handle, root_kind).map_err(map_kind_v2(kind))?;
        if resolved != expected_final_path {
            return Err(bootstrap_error_v2(kind));
        }
        let identity = identity_v2(&handle, root_kind).map_err(map_kind_v2(kind))?;
        if let Some(required) = volume_serial_number {
            if identity.volume_serial_number != required {
                return Err(bootstrap_error_v2(
                    NativeTrainingStoreBootstrapV2ErrorKind::VolumeInvalid,
                ));
            }
        }
        Ok((handle, identity))
    }

    fn require_empty_directory_v2(path: &Path) -> Result<()> {
        let corrupt = bootstrap_error_v2(NativeTrainingStoreBootstrapV2ErrorKind::CorruptRoot);
        if fs::read_dir(path).map_err(|_| corrupt)?.next().is_some() {
            return Err(corrupt);
        }
        Ok(())
    }

    pub(super) fn bootstrap_v2(
        parent: &Path,
        root_basename: &str,
        mut hook: impl FnMut(BootstrapBoundaryV2) -> Result<()>,
    ) -> Result<NativeTrainingStoreBootstrapV2> {
        let parent_kind = NativeTrainingStoreBootstrapV2ErrorKind::ParentInvalid;
        let root_kind_error = NativeTrainingStoreRootV2ErrorKind::RootInvalid;

        // Parent admission: existing no-follow directory on a local fixed
        // NTFS volume, handle retained through the bootstrap.
        let parent_handle = open_no_follow_v2(parent, 0, DIRECTORY_SHARE_V2, true, root_kind_error)
            .map_err(map_kind_v2(parent_kind))?;
        require_directory_attributes_v2(&parent_handle, root_kind_error)
            .map_err(map_kind_v2(parent_kind))?;
        let parent_final =
            final_path_v2(&parent_handle, root_kind_error).map_err(map_kind_v2(parent_kind))?;
        require_local_fixed_ntfs_v2(&parent_handle, &parent_final).map_err(map_kind_v2(
            NativeTrainingStoreBootstrapV2ErrorKind::VolumeInvalid,
        ))?;
        let parent_identity =
            identity_v2(&parent_handle, root_kind_error).map_err(map_kind_v2(parent_kind))?;
        hook(BootstrapBoundaryV2::ParentValidated)?;

        // B0: create exactly the final root component; a create race is
        // handled only by reopening and validating the winner.
        let root_path: PathBuf = parent_final.join(root_basename);
        if fs::symlink_metadata(&root_path).is_err() {
            match fs::create_dir(&root_path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(_) => {
                    return Err(bootstrap_error_v2(
                        NativeTrainingStoreBootstrapV2ErrorKind::MutationFailed,
                    ))
                }
            }
            hook(BootstrapBoundaryV2::RootCreated)?;
        }
        let (root_handle, root_identity) = open_validated_directory_v2(
            &root_path,
            &root_path,
            Some(parent_identity.volume_serial_number),
            NativeTrainingStoreBootstrapV2ErrorKind::CorruptRoot,
        )?;
        hook(BootstrapBoundaryV2::RootReopened)?;

        // Strict pre-lock inventory.
        let inventory = classify_root_v2(&root_path)?;

        // B1: create the empty lock leaf with create-new semantics.
        let lock_path = root_path.join(NATIVE_TRAINING_STORE_LOCK_LEAF_V2);
        if !inventory.lock_present {
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&lock_path)
            {
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(_) => {
                    return Err(bootstrap_error_v2(
                        NativeTrainingStoreBootstrapV2ErrorKind::MutationFailed,
                    ))
                }
            }
            hook(BootstrapBoundaryV2::LockCreated)?;
        }

        // B2 through B8: open that same regular file no-follow, require zero
        // bytes, capture its identity, and take the exclusive range lock.
        let lock_kind = NativeTrainingStoreBootstrapV2ErrorKind::LockInvalid;
        let root_lock_kind = NativeTrainingStoreRootV2ErrorKind::LockInvalid;
        let lock_handle = open_no_follow_v2(
            &lock_path,
            GENERIC_READ_V2,
            FILE_SHARE_READ_V2,
            false,
            root_lock_kind,
        )
        .map_err(map_kind_v2(lock_kind))?;
        let lock_basic =
            basic_info_v2(&lock_handle, root_lock_kind).map_err(map_kind_v2(lock_kind))?;
        if lock_basic.file_attributes
            & (FILE_ATTRIBUTE_DIRECTORY_V2 | FILE_ATTRIBUTE_REPARSE_POINT_V2)
            != 0
        {
            return Err(bootstrap_error_v2(lock_kind));
        }
        let lock_standard =
            standard_info_v2(&lock_handle, root_lock_kind).map_err(map_kind_v2(lock_kind))?;
        if lock_standard.end_of_file != 0
            || lock_standard.directory != 0
            || lock_standard.delete_pending != 0
            || lock_standard.number_of_links != 1
        {
            return Err(bootstrap_error_v2(lock_kind));
        }
        let lock_identity =
            identity_v2(&lock_handle, root_lock_kind).map_err(map_kind_v2(lock_kind))?;
        if lock_identity.volume_serial_number != root_identity.volume_serial_number {
            return Err(bootstrap_error_v2(
                NativeTrainingStoreBootstrapV2ErrorKind::VolumeInvalid,
            ));
        }
        hook(BootstrapBoundaryV2::LockValidated)?;

        try_range_lock_v2(&lock_handle, true).map_err(|raw| {
            bootstrap_error_v2(if raw == Some(RANGE_LOCK_HELD_RAW_OS_V2) {
                NativeTrainingStoreBootstrapV2ErrorKind::StoreBusy
            } else {
                NativeTrainingStoreBootstrapV2ErrorKind::LockInvalid
            })
        })?;
        let held_lock = HeldBootstrapLockV2 {
            handle: &lock_handle,
        };
        hook(BootstrapBoundaryV2::LockAcquired)?;

        // Locked rescan: require the same state and identities observed by
        // the retained handles.
        let identity_kind = NativeTrainingStoreBootstrapV2ErrorKind::IdentityChanged;
        let rescanned = classify_root_v2(&root_path)?;
        if !rescanned.lock_present
            || rescanned.run_present != inventory.run_present
            || rescanned.directory_count < inventory.directory_count
        {
            return Err(bootstrap_error_v2(identity_kind));
        }
        let (_parent_reopen, parent_reidentity) =
            open_validated_directory_v2(&parent_final, &parent_final, None, identity_kind)?;
        if parent_reidentity != parent_identity {
            return Err(bootstrap_error_v2(identity_kind));
        }
        let (_root_reopen, root_reidentity) = open_validated_directory_v2(
            &root_path,
            &root_path,
            Some(parent_identity.volume_serial_number),
            identity_kind,
        )?;
        if root_reidentity != root_identity {
            return Err(bootstrap_error_v2(identity_kind));
        }
        // Existing admitted directories must be validated empty before B8.
        let mut retained_directories: Vec<(OwnedHandleV2, FileIdInfoV2)> = Vec::with_capacity(4);
        for directory in NATIVE_TRAINING_STORE_SUBDIRECTORY_ORDER_V2
            .iter()
            .take(rescanned.directory_count)
        {
            let basename = directory
                .basename()
                .expect("subdirectory order names only child directories");
            let child_path = root_path.join(basename);
            let opened = open_validated_directory_v2(
                &child_path,
                &child_path,
                Some(root_identity.volume_serial_number),
                NativeTrainingStoreBootstrapV2ErrorKind::CorruptRoot,
            )?;
            if !rescanned.run_present {
                require_empty_directory_v2(&child_path)?;
            }
            retained_directories.push(opened);
        }
        hook(BootstrapBoundaryV2::LockedRescan)?;

        // B2 onward: create the missing directory suffix one at a time in the
        // exact frozen order, never recreating or clearing an existing one.
        for (ordinal, directory) in NATIVE_TRAINING_STORE_SUBDIRECTORY_ORDER_V2
            .iter()
            .enumerate()
            .skip(rescanned.directory_count)
        {
            let basename = directory
                .basename()
                .expect("subdirectory order names only child directories");
            let child_path = root_path.join(basename);
            match fs::create_dir(&child_path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(_) => {
                    return Err(bootstrap_error_v2(
                        NativeTrainingStoreBootstrapV2ErrorKind::MutationFailed,
                    ))
                }
            }
            hook(BootstrapBoundaryV2::DirectoryCreated(ordinal))?;
            let opened = open_validated_directory_v2(
                &child_path,
                &child_path,
                Some(root_identity.volume_serial_number),
                NativeTrainingStoreBootstrapV2ErrorKind::CorruptRoot,
            )?;
            require_empty_directory_v2(&child_path)?;
            retained_directories.push(opened);
            hook(BootstrapBoundaryV2::DirectoryValidated(ordinal))?;
        }

        // B7: the only pre-run deletion is the fully recognized
        // `.run.json.stage-v2`, under the lock, after the complete plan.
        if rescanned.run_stage_present && !rescanned.run_present {
            fs::remove_file(root_path.join(RUN_STAGE_LEAF_V2)).map_err(|_| {
                bootstrap_error_v2(NativeTrainingStoreBootstrapV2ErrorKind::MutationFailed)
            })?;
            hook(BootstrapBoundaryV2::RunStageDeleted)?;
        }

        // Recapture identities immediately before handing the skeleton over.
        let final_identity =
            identity_v2(&root_handle, root_kind_error).map_err(map_kind_v2(identity_kind))?;
        if final_identity != root_identity {
            return Err(bootstrap_error_v2(identity_kind));
        }

        // Open the completed skeleton through the strict root authority while
        // the bootstrap range lock is still held, then release it.
        let outcome = if rescanned.run_present {
            NativeTrainingStoreBootstrapOutcomeV2::RunAuthorityPresent
        } else {
            NativeTrainingStoreBootstrapOutcomeV2::SkeletonReady
        };
        let root = ValidatedNativeTrainingStoreRootV2::open_v2(&root_path).map_err(|error| {
            bootstrap_error_v2(match error.kind() {
                NativeTrainingStoreRootV2ErrorKind::StoreBusy => {
                    NativeTrainingStoreBootstrapV2ErrorKind::StoreBusy
                }
                NativeTrainingStoreRootV2ErrorKind::VolumeInvalid => {
                    NativeTrainingStoreBootstrapV2ErrorKind::VolumeInvalid
                }
                NativeTrainingStoreRootV2ErrorKind::LockInvalid => {
                    NativeTrainingStoreBootstrapV2ErrorKind::LockInvalid
                }
                _ => NativeTrainingStoreBootstrapV2ErrorKind::CorruptRoot,
            })
        })?;
        drop(held_lock);
        let _ = retained_directories;
        Ok(NativeTrainingStoreBootstrapV2 { root, outcome })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_basename_grammar_accepts_plain_names_and_rejects_every_reserved_form() {
        for accepted in ["store", "mtg-store-v2", "a", "STORE.local", "com10", "lpt"] {
            assert!(
                validate_store_root_basename_v2(accepted).is_ok(),
                "{accepted:?} must be accepted"
            );
        }
        let rejected = [
            "",
            ".",
            "..",
            "a/b",
            "a\\b",
            "a:b",
            "con",
            "CON",
            "Con.txt",
            "PRN",
            "aux.json",
            "NUL",
            "conin$",
            "CONOUT$",
            "clock$",
            "COM1",
            "com9.txt",
            "LPT3",
            "lpt\u{00B9}",
            "name.",
            "name ",
            "a\u{0000}b",
            "a\tb",
            "a*b",
            "a?b",
            "a<b",
            "a>b",
            "a|b",
            "a\"b",
        ];
        for basename in rejected {
            assert_eq!(
                validate_store_root_basename_v2(basename)
                    .unwrap_err()
                    .kind(),
                NativeTrainingStoreBootstrapV2ErrorKind::RootBasenameInvalid,
                "{basename:?} must be rejected"
            );
        }
    }

    #[cfg(windows)]
    mod windows_tests {
        use super::super::*;
        use crate::native_training_store_layout_v2::{
            NATIVE_TRAINING_STORE_LOCK_LEAF_V2, NATIVE_TRAINING_STORE_SUBDIRECTORY_ORDER_V2,
        };
        use std::fs;
        use std::path::{Path, PathBuf};
        use std::sync::atomic::{AtomicU64, Ordering};

        struct TestParentV2 {
            parent: PathBuf,
        }

        impl TestParentV2 {
            fn new(label: &str) -> Self {
                static ORDINAL: AtomicU64 = AtomicU64::new(0);
                let ordinal = ORDINAL.fetch_add(1, Ordering::Relaxed);
                let parent = std::env::temp_dir().join(format!(
                    "mtg-kernel-store-bootstrap-v2-{}-{label}-{ordinal}",
                    std::process::id()
                ));
                fs::create_dir(&parent).expect("create test parent");
                Self { parent }
            }

            fn path(&self) -> &Path {
                &self.parent
            }
        }

        impl Drop for TestParentV2 {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.parent);
            }
        }

        fn assert_complete_skeleton(root: &Path) {
            assert!(
                fs::symlink_metadata(root.join(NATIVE_TRAINING_STORE_LOCK_LEAF_V2))
                    .unwrap()
                    .is_file()
            );
            for directory in NATIVE_TRAINING_STORE_SUBDIRECTORY_ORDER_V2 {
                let path = root.join(directory.basename().unwrap());
                assert!(fs::symlink_metadata(&path).unwrap().is_dir());
                assert_eq!(fs::read_dir(&path).unwrap().count(), 0);
            }
        }

        #[test]
        fn bootstrap_creates_the_complete_skeleton_and_is_idempotent() {
            let parent = TestParentV2::new("create");
            let first = bootstrap_native_training_store_v2(parent.path(), "store").unwrap();
            assert_eq!(
                first.outcome(),
                NativeTrainingStoreBootstrapOutcomeV2::SkeletonReady
            );
            let root_path = parent.path().join("store");
            drop(first);
            assert_complete_skeleton(&root_path);

            let second = bootstrap_native_training_store_v2(parent.path(), "store").unwrap();
            assert_eq!(
                second.outcome(),
                NativeTrainingStoreBootstrapOutcomeV2::SkeletonReady
            );
            drop(second);

            // Run-authority presence flips the outcome without byte claims.
            fs::write(root_path.join("run.json"), b"placeholder").unwrap();
            let third = bootstrap_native_training_store_v2(parent.path(), "store").unwrap();
            assert_eq!(
                third.outcome(),
                NativeTrainingStoreBootstrapOutcomeV2::RunAuthorityPresent
            );
        }

        #[test]
        fn every_intermediate_crash_state_resumes_without_recreating_members() {
            let parent = TestParentV2::new("resume-states");
            let root_path = parent.path().join("store");

            // Manually construct each admitted prefix state and prove the
            // bootstrap completes it idempotently.
            let build_states: [&dyn Fn(); 7] = [
                &|| {},
                &|| fs::create_dir(&root_path).unwrap(),
                &|| fs::write(root_path.join(NATIVE_TRAINING_STORE_LOCK_LEAF_V2), []).unwrap(),
                &|| fs::create_dir(root_path.join("segments")).unwrap(),
                &|| fs::create_dir(root_path.join("checkpoints")).unwrap(),
                &|| fs::create_dir(root_path.join("heads")).unwrap(),
                &|| fs::create_dir(root_path.join("refs")).unwrap(),
            ];
            for (state, advance) in build_states.iter().enumerate() {
                advance();
                let bootstrapped =
                    bootstrap_native_training_store_v2(parent.path(), "store").unwrap();
                assert_eq!(
                    bootstrapped.outcome(),
                    NativeTrainingStoreBootstrapOutcomeV2::SkeletonReady,
                    "state B{state} must complete"
                );
                drop(bootstrapped);
                assert_complete_skeleton(&root_path);
                // Reset to the constructed prefix for the next iteration by
                // removing only members later states would add.
                if state < build_states.len() - 1 {
                    fs::remove_dir_all(&root_path).unwrap();
                    for rebuild in build_states.iter().take(state + 1).skip(1) {
                        rebuild();
                    }
                }
            }

            // A recognized run stage without run.json is deleted (B7 -> B6).
            let stage = root_path.join(".run.json.stage-v2");
            fs::write(&stage, b"partial run bytes").unwrap();
            let recovered = bootstrap_native_training_store_v2(parent.path(), "store").unwrap();
            assert_eq!(
                recovered.outcome(),
                NativeTrainingStoreBootstrapOutcomeV2::SkeletonReady
            );
            assert!(fs::symlink_metadata(&stage).is_err());
        }

        #[test]
        fn out_of_state_and_unknown_objects_are_corruption_and_preserved() {
            // refs before heads is not a prefix.
            let out_of_order = TestParentV2::new("out-of-order");
            let root = out_of_order.path().join("store");
            fs::create_dir(&root).unwrap();
            fs::write(root.join(NATIVE_TRAINING_STORE_LOCK_LEAF_V2), []).unwrap();
            fs::create_dir(root.join("segments")).unwrap();
            fs::create_dir(root.join("refs")).unwrap();
            assert_eq!(
                bootstrap_native_training_store_v2(out_of_order.path(), "store")
                    .unwrap_err()
                    .kind(),
                NativeTrainingStoreBootstrapV2ErrorKind::CorruptRoot
            );
            assert!(fs::symlink_metadata(root.join("refs")).unwrap().is_dir());

            // An unknown leaf is corruption and preserved.
            let unknown = TestParentV2::new("unknown-leaf");
            let root = unknown.path().join("store");
            fs::create_dir(&root).unwrap();
            fs::write(root.join("notes.txt"), b"evidence").unwrap();
            assert_eq!(
                bootstrap_native_training_store_v2(unknown.path(), "store")
                    .unwrap_err()
                    .kind(),
                NativeTrainingStoreBootstrapV2ErrorKind::CorruptRoot
            );
            assert_eq!(fs::read(root.join("notes.txt")).unwrap(), b"evidence");

            // latest.json before run.json is out of state.
            let early_latest = TestParentV2::new("early-latest");
            let root = early_latest.path().join("store");
            fs::create_dir(&root).unwrap();
            fs::write(root.join(NATIVE_TRAINING_STORE_LOCK_LEAF_V2), []).unwrap();
            fs::write(root.join("latest.json"), b"{}").unwrap();
            assert_eq!(
                bootstrap_native_training_store_v2(early_latest.path(), "store")
                    .unwrap_err()
                    .kind(),
                NativeTrainingStoreBootstrapV2ErrorKind::CorruptRoot
            );

            // run.json without the complete skeleton is corrupt.
            let early_run = TestParentV2::new("early-run");
            let root = early_run.path().join("store");
            fs::create_dir(&root).unwrap();
            fs::write(root.join(NATIVE_TRAINING_STORE_LOCK_LEAF_V2), []).unwrap();
            fs::write(root.join("run.json"), b"{}").unwrap();
            assert_eq!(
                bootstrap_native_training_store_v2(early_run.path(), "store")
                    .unwrap_err()
                    .kind(),
                NativeTrainingStoreBootstrapV2ErrorKind::CorruptRoot
            );

            // A malformed stage-like leaf is corruption, not cleanup input.
            let malformed = TestParentV2::new("malformed-stage");
            let root = malformed.path().join("store");
            fs::create_dir(&root).unwrap();
            fs::write(root.join(".run.json.stage-v3"), b"evidence").unwrap();
            assert_eq!(
                bootstrap_native_training_store_v2(malformed.path(), "store")
                    .unwrap_err()
                    .kind(),
                NativeTrainingStoreBootstrapV2ErrorKind::CorruptRoot
            );
            assert_eq!(
                fs::read(root.join(".run.json.stage-v3")).unwrap(),
                b"evidence"
            );

            // A nonempty admitted directory without run.json is corrupt.
            let dirty_directory = TestParentV2::new("dirty-directory");
            let root = dirty_directory.path().join("store");
            fs::create_dir(&root).unwrap();
            fs::write(root.join(NATIVE_TRAINING_STORE_LOCK_LEAF_V2), []).unwrap();
            fs::create_dir(root.join("segments")).unwrap();
            fs::write(root.join("segments").join("segment-00000000.json"), b"{}").unwrap();
            assert_eq!(
                bootstrap_native_training_store_v2(dirty_directory.path(), "store")
                    .unwrap_err()
                    .kind(),
                NativeTrainingStoreBootstrapV2ErrorKind::CorruptRoot
            );

            // A nonempty lock leaf is invalid.
            let dirty_lock = TestParentV2::new("dirty-lock");
            let root = dirty_lock.path().join("store");
            fs::create_dir(&root).unwrap();
            fs::write(root.join(NATIVE_TRAINING_STORE_LOCK_LEAF_V2), b"x").unwrap();
            assert_eq!(
                bootstrap_native_training_store_v2(dirty_lock.path(), "store")
                    .unwrap_err()
                    .kind(),
                NativeTrainingStoreBootstrapV2ErrorKind::LockInvalid
            );

            // A missing parent is a parent error, and no root is created.
            let missing_parent = TestParentV2::new("missing-parent");
            let absent = missing_parent.path().join("absent-parent");
            assert_eq!(
                bootstrap_native_training_store_v2(&absent, "store")
                    .unwrap_err()
                    .kind(),
                NativeTrainingStoreBootstrapV2ErrorKind::ParentInvalid
            );
            assert!(fs::symlink_metadata(&absent).is_err());
        }

        #[test]
        fn every_bootstrap_fault_boundary_interrupts_and_retries_idempotently() {
            let injected =
                bootstrap_error_v2(NativeTrainingStoreBootstrapV2ErrorKind::UnsupportedPlatform);
            let boundaries = [
                BootstrapBoundaryV2::ParentValidated,
                BootstrapBoundaryV2::RootCreated,
                BootstrapBoundaryV2::RootReopened,
                BootstrapBoundaryV2::LockCreated,
                BootstrapBoundaryV2::LockValidated,
                BootstrapBoundaryV2::LockAcquired,
                BootstrapBoundaryV2::LockedRescan,
                BootstrapBoundaryV2::DirectoryCreated(0),
                BootstrapBoundaryV2::DirectoryValidated(0),
                BootstrapBoundaryV2::DirectoryCreated(1),
                BootstrapBoundaryV2::DirectoryValidated(1),
                BootstrapBoundaryV2::DirectoryCreated(2),
                BootstrapBoundaryV2::DirectoryValidated(2),
                BootstrapBoundaryV2::DirectoryCreated(3),
                BootstrapBoundaryV2::DirectoryValidated(3),
            ];
            for &boundary in &boundaries {
                let parent = TestParentV2::new("fault");
                let result = bootstrap_native_training_store_with_hook_v2(
                    parent.path(),
                    "store",
                    |reached| {
                        if reached == boundary {
                            Err(injected)
                        } else {
                            Ok(())
                        }
                    },
                );
                assert_eq!(
                    result.unwrap_err().kind(),
                    NativeTrainingStoreBootstrapV2ErrorKind::UnsupportedPlatform,
                    "boundary {boundary:?} must interrupt the bootstrap"
                );
                let recovered = bootstrap_native_training_store_v2(parent.path(), "store").unwrap();
                assert_eq!(
                    recovered.outcome(),
                    NativeTrainingStoreBootstrapOutcomeV2::SkeletonReady,
                    "boundary {boundary:?} must retry idempotently"
                );
                drop(recovered);
                assert_complete_skeleton(&parent.path().join("store"));
            }

            // The run-stage deletion boundary requires a prepared B7 state.
            let parent = TestParentV2::new("fault-run-stage");
            let prepared = bootstrap_native_training_store_v2(parent.path(), "store").unwrap();
            drop(prepared);
            let stage = parent.path().join("store").join(".run.json.stage-v2");
            fs::write(&stage, b"partial").unwrap();
            let result =
                bootstrap_native_training_store_with_hook_v2(parent.path(), "store", |reached| {
                    if reached == BootstrapBoundaryV2::RunStageDeleted {
                        Err(injected)
                    } else {
                        Ok(())
                    }
                });
            assert_eq!(
                result.unwrap_err().kind(),
                NativeTrainingStoreBootstrapV2ErrorKind::UnsupportedPlatform
            );
            assert!(fs::symlink_metadata(&stage).is_err());
            let recovered = bootstrap_native_training_store_v2(parent.path(), "store").unwrap();
            assert_eq!(
                recovered.outcome(),
                NativeTrainingStoreBootstrapOutcomeV2::SkeletonReady
            );
        }

        #[test]
        fn a_held_exclusive_lock_reports_store_busy_and_mutates_nothing() {
            let parent = TestParentV2::new("busy");
            let bootstrapped = bootstrap_native_training_store_v2(parent.path(), "store").unwrap();
            let root = bootstrapped.into_root();
            let held = root.lock_exclusive_v2().unwrap();

            let busy = bootstrap_native_training_store_v2(parent.path(), "store").unwrap_err();
            assert_eq!(
                busy.kind(),
                NativeTrainingStoreBootstrapV2ErrorKind::StoreBusy
            );
            drop(held);

            let recovered = bootstrap_native_training_store_v2(parent.path(), "store").unwrap();
            assert_eq!(
                recovered.outcome(),
                NativeTrainingStoreBootstrapOutcomeV2::SkeletonReady
            );
        }
    }

    #[cfg(not(windows))]
    mod non_windows_tests {
        use super::super::*;

        #[test]
        fn the_stable_unsupported_platform_error_precedes_any_mutation() {
            let parent = std::env::temp_dir();
            let probe = parent.join("mtg-kernel-bootstrap-unsupported-probe");
            let error = bootstrap_native_training_store_v2(
                &parent,
                "mtg-kernel-bootstrap-unsupported-probe",
            )
            .unwrap_err();
            assert_eq!(
                error.kind(),
                NativeTrainingStoreBootstrapV2ErrorKind::UnsupportedPlatform
            );
            assert!(!probe.exists());

            // The pure grammar still rejects before the platform gate.
            assert_eq!(
                bootstrap_native_training_store_v2(&parent, "con")
                    .unwrap_err()
                    .kind(),
                NativeTrainingStoreBootstrapV2ErrorKind::RootBasenameInvalid
            );
        }
    }
}
