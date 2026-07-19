//! Schema-neutral, move-only durable file publication primitives.
//!
//! Both operations create a same-parent staging file with `create_new`, write
//! and sync it, and recapture its stable filesystem identity, exact length,
//! and SHA-256 repeatedly before moving it. The immutable operation refuses to
//! replace a destination. The replacement operation is a separate API with a
//! distinct, non-cloneable receipt, so a caller cannot mistake replacement of
//! a mutable pointer file for publication of an immutable artifact.
//!
//! On Windows, immutable publication is exactly
//! `MoveFileExW(stage, final, MOVEFILE_WRITE_THROUGH)` and replacement is
//! exactly `MoveFileExW(stage, final, MOVEFILE_REPLACE_EXISTING |
//! MOVEFILE_WRITE_THROUGH)`. No copy fallback is permitted. Microsoft's
//! documented `MOVEFILE_WRITE_THROUGH` behavior applies to the move operation;
//! this module does not claim a portable directory fsync, broader sudden-power-
//! loss dirent survival, or equivalence to another store's durability model.
//!
//! On Linux, immutable publication uses `renameat2(..., RENAME_NOREPLACE)`;
//! Apple targets use `renamex_np(..., RENAME_EXCL)`. Other Unix targets fail
//! closed for immutable publication rather than emulating no-replace with a
//! racy check. Replacement uses same-parent `rename`. Every successful Unix
//! move is followed by a directory `sync_all` before final reread.
//!
//! These primitives deliberately know nothing about JSON, store identities,
//! generations, trainer commits, CLI grammar, or latest-pointer contents. A
//! higher layer must reread and semantically validate every related artifact
//! before constructing its own commit witness. The receipts here have private
//! fields, no public constructor, and no `Clone` implementation.
//!
//! Operations remain path based. Parent and file identities are revalidated at
//! every material boundary, and symlink/reparse ancestors are rejected by the
//! shared generic primitive. The standard library still cannot make the whole
//! sequence race-free against a hostile process that mutates a caller-owned
//! directory concurrently. Callers must exclusively own that directory.

use crate::durable_publication_v1::{
    child_path_v1, file_identity_v1, recapture_exact_file_v1,
    recapture_existing_regular_identity_v1, revalidate_parent_v1, sync_parent_namespace_v1,
    validate_caller_content_v1, DurableFileExpectationV1, DurablePublicationErrorKindV1,
    DurablePublicationErrorV1, ObjectIdentityV1, ValidatedPublicationParentV1,
};
use std::ffi::OsStr;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Platform mechanism that produced an immutable-file move receipt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImmutableMoveMechanismV2 {
    WindowsMoveFileExWriteThroughNoReplace,
    UnixAtomicRenameNoReplaceDirectorySynced,
}

/// Platform mechanism that produced a replacement-file move receipt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplacementMoveMechanismV2 {
    WindowsMoveFileExReplaceExistingWriteThrough,
    UnixAtomicRenameReplaceDirectorySynced,
}

/// Proof that this call moved a fully recaptured stage to an absent final path.
///
/// This low-level receipt proves only exact file publication. It is not a store
/// generation receipt or a trainer commit witness.
#[derive(Debug, Eq, PartialEq)]
pub struct ImmutableMovePublicationReceiptV2 {
    final_path: PathBuf,
    exact_length: u64,
    sha256: [u8; 32],
    mechanism: ImmutableMoveMechanismV2,
}

impl ImmutableMovePublicationReceiptV2 {
    pub fn final_path(&self) -> &Path {
        &self.final_path
    }

    pub const fn exact_length(&self) -> u64 {
        self.exact_length
    }

    pub const fn sha256(&self) -> [u8; 32] {
        self.sha256
    }

    pub const fn mechanism(&self) -> ImmutableMoveMechanismV2 {
        self.mechanism
    }
}

/// Proof that this call moved a fully recaptured stage over, or into, a final
/// pointer path using the platform's replacement operation.
///
/// This type is intentionally distinct from [`ImmutableMovePublicationReceiptV2`].
/// It is not a store generation receipt or a trainer commit witness.
#[derive(Debug, Eq, PartialEq)]
pub struct ReplacementMovePublicationReceiptV2 {
    final_path: PathBuf,
    exact_length: u64,
    sha256: [u8; 32],
    mechanism: ReplacementMoveMechanismV2,
}

impl ReplacementMovePublicationReceiptV2 {
    pub fn final_path(&self) -> &Path {
        &self.final_path
    }

    pub const fn exact_length(&self) -> u64 {
        self.exact_length
    }

    pub const fn sha256(&self) -> [u8; 32] {
        self.sha256
    }

    pub const fn mechanism(&self) -> ReplacementMoveMechanismV2 {
        self.mechanism
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MoveDispositionV2 {
    ImmutableNoReplace,
    Replace,
}

#[derive(Debug)]
struct PublishedMoveV2 {
    final_path: PathBuf,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PublicationBoundaryV2 {
    ParentValidated,
    StageCreated,
    StageWritten,
    StageSynced,
    StageVerified,
    StageReverified,
    FinalMoved,
    ParentNamespaceSynced,
    FinalVerified,
}

/// Moves borrowed exact bytes to a new immutable final name without replacing
/// any existing destination.
///
/// `stage_name` and `final_name` must be distinct single normal path
/// components beneath `parent`. Any stage or final collision is preserved and
/// returns no receipt. On an error after the move, a valid final file may
/// remain, but no receipt is returned and the caller must recover explicitly.
pub fn publish_immutable_file_by_move_v2(
    parent: &ValidatedPublicationParentV1,
    stage_name: impl AsRef<OsStr>,
    final_name: impl AsRef<OsStr>,
    bytes: &[u8],
    expectation: DurableFileExpectationV1,
) -> Result<ImmutableMovePublicationReceiptV2, DurablePublicationErrorV1> {
    publish_immutable_file_by_move_with_hook_v2(
        parent,
        stage_name.as_ref(),
        final_name.as_ref(),
        bytes,
        expectation,
        |_, _, _| Ok(()),
    )
}

fn publish_immutable_file_by_move_with_hook_v2(
    parent: &ValidatedPublicationParentV1,
    stage_name: &OsStr,
    final_name: &OsStr,
    bytes: &[u8],
    expectation: DurableFileExpectationV1,
    boundary_hook: impl FnMut(
        PublicationBoundaryV2,
        &Path,
        &Path,
    ) -> Result<(), DurablePublicationErrorV1>,
) -> Result<ImmutableMovePublicationReceiptV2, DurablePublicationErrorV1> {
    let published = publish_file_by_move_with_hook_v2(
        parent,
        stage_name,
        final_name,
        bytes,
        expectation,
        MoveDispositionV2::ImmutableNoReplace,
        boundary_hook,
    )?;
    Ok(ImmutableMovePublicationReceiptV2 {
        final_path: published.final_path,
        exact_length: expectation.exact_length(),
        sha256: expectation.sha256(),
        mechanism: immutable_mechanism_v2(),
    })
}

/// Atomically moves borrowed exact bytes into a replaceable final path.
///
/// The destination may be absent (initial pointer publication) or an ordinary
/// regular file (pointer replacement). A link, reparse point, directory, or
/// other non-regular existing destination fails closed. This operation never
/// returns [`ImmutableMovePublicationReceiptV2`].
pub fn replace_file_by_move_v2(
    parent: &ValidatedPublicationParentV1,
    stage_name: impl AsRef<OsStr>,
    final_name: impl AsRef<OsStr>,
    bytes: &[u8],
    expectation: DurableFileExpectationV1,
) -> Result<ReplacementMovePublicationReceiptV2, DurablePublicationErrorV1> {
    replace_file_by_move_with_hook_v2(
        parent,
        stage_name.as_ref(),
        final_name.as_ref(),
        bytes,
        expectation,
        |_, _, _| Ok(()),
    )
}

fn replace_file_by_move_with_hook_v2(
    parent: &ValidatedPublicationParentV1,
    stage_name: &OsStr,
    final_name: &OsStr,
    bytes: &[u8],
    expectation: DurableFileExpectationV1,
    boundary_hook: impl FnMut(
        PublicationBoundaryV2,
        &Path,
        &Path,
    ) -> Result<(), DurablePublicationErrorV1>,
) -> Result<ReplacementMovePublicationReceiptV2, DurablePublicationErrorV1> {
    let published = publish_file_by_move_with_hook_v2(
        parent,
        stage_name,
        final_name,
        bytes,
        expectation,
        MoveDispositionV2::Replace,
        boundary_hook,
    )?;
    Ok(ReplacementMovePublicationReceiptV2 {
        final_path: published.final_path,
        exact_length: expectation.exact_length(),
        sha256: expectation.sha256(),
        mechanism: replacement_mechanism_v2(),
    })
}

fn publish_file_by_move_with_hook_v2(
    parent: &ValidatedPublicationParentV1,
    stage_name: &OsStr,
    final_name: &OsStr,
    bytes: &[u8],
    expectation: DurableFileExpectationV1,
    disposition: MoveDispositionV2,
    mut boundary_hook: impl FnMut(
        PublicationBoundaryV2,
        &Path,
        &Path,
    ) -> Result<(), DurablePublicationErrorV1>,
) -> Result<PublishedMoveV2, DurablePublicationErrorV1> {
    // This precheck deliberately precedes every path lookup and mutation.
    validate_caller_content_v1(bytes, expectation)?;
    let stage_path = child_path_v1(parent, stage_name)?;
    let final_path = child_path_v1(parent, final_name)?;
    if stage_path == final_path {
        return Err(DurablePublicationErrorV1::new(
            DurablePublicationErrorKindV1::InvalidChildName,
            "staging and final child names must differ",
        ));
    }

    revalidate_parent_v1(parent)?;
    boundary_hook(
        PublicationBoundaryV2::ParentValidated,
        &stage_path,
        &final_path,
    )?;

    let mut stage_file = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&stage_path)
        .map_err(|error| {
            let kind = if error.kind() == io::ErrorKind::AlreadyExists {
                DurablePublicationErrorKindV1::StageCollision
            } else {
                DurablePublicationErrorKindV1::StageCreate
            };
            DurablePublicationErrorV1::with_io(
                kind,
                format!("cannot create new staging file {}", stage_path.display()),
                error,
            )
        })?;
    let stage_metadata = stage_file.metadata().map_err(|error| {
        DurablePublicationErrorV1::with_io(
            DurablePublicationErrorKindV1::StageCreate,
            format!("cannot inspect new staging file {}", stage_path.display()),
            error,
        )
    })?;
    if !stage_metadata.file_type().is_file() {
        return Err(DurablePublicationErrorV1::new(
            DurablePublicationErrorKindV1::StageCreate,
            "new staging handle is not a regular file",
        ));
    }
    let stage_identity = file_identity_v1(&stage_file)?;
    boundary_hook(
        PublicationBoundaryV2::StageCreated,
        &stage_path,
        &final_path,
    )?;

    stage_file.write_all(bytes).map_err(|error| {
        DurablePublicationErrorV1::with_io(
            DurablePublicationErrorKindV1::StageWrite,
            format!(
                "cannot write complete staging file {}",
                stage_path.display()
            ),
            error,
        )
    })?;
    boundary_hook(
        PublicationBoundaryV2::StageWritten,
        &stage_path,
        &final_path,
    )?;
    stage_file.sync_all().map_err(|error| {
        DurablePublicationErrorV1::with_io(
            DurablePublicationErrorKindV1::StageSync,
            format!("cannot sync staging file {}", stage_path.display()),
            error,
        )
    })?;
    boundary_hook(PublicationBoundaryV2::StageSynced, &stage_path, &final_path)?;

    recapture_exact_file_v1(
        &stage_path,
        stage_identity,
        expectation,
        DurablePublicationErrorKindV1::StageVerification,
    )?;
    boundary_hook(
        PublicationBoundaryV2::StageVerified,
        &stage_path,
        &final_path,
    )?;

    // MoveFileExW requires the stage handle to be closed. A second complete
    // identity/length/digest recapture follows the close. After the testable
    // boundary, a third recapture sits immediately before the platform move.
    drop(stage_file);
    revalidate_parent_v1(parent)?;
    recapture_exact_file_v1(
        &stage_path,
        stage_identity,
        expectation,
        DurablePublicationErrorKindV1::StageVerification,
    )?;
    boundary_hook(
        PublicationBoundaryV2::StageReverified,
        &stage_path,
        &final_path,
    )?;

    revalidate_parent_v1(parent)?;
    recapture_exact_file_v1(
        &stage_path,
        stage_identity,
        expectation,
        DurablePublicationErrorKindV1::StageVerification,
    )?;
    // This is deliberately the final path operation before either move. It
    // detects case-folded, short-name, and hard-link aliases by stable object
    // identity rather than lexical spelling.
    reject_final_stage_alias_v2(&final_path, stage_identity, disposition)?;

    match disposition {
        MoveDispositionV2::ImmutableNoReplace => {
            move_no_replace_platform_v2(&stage_path, &final_path).map_err(|error| {
                let kind = if error.kind() == io::ErrorKind::Unsupported {
                    DurablePublicationErrorKindV1::UnsupportedPlatform
                } else {
                    immutable_move_error_kind_v2(&final_path, &error)
                };
                DurablePublicationErrorV1::with_io(
                    kind,
                    format!(
                        "cannot atomically move staging file {} to new final {}",
                        stage_path.display(),
                        final_path.display()
                    ),
                    error,
                )
            })?;
        }
        MoveDispositionV2::Replace => {
            replace_platform_v2(&stage_path, &final_path).map_err(|error| {
                let kind = if error.kind() == io::ErrorKind::Unsupported {
                    DurablePublicationErrorKindV1::UnsupportedPlatform
                } else {
                    DurablePublicationErrorKindV1::FinalPublish
                };
                DurablePublicationErrorV1::with_io(
                    kind,
                    format!(
                        "cannot atomically move staging file {} over final {}",
                        stage_path.display(),
                        final_path.display()
                    ),
                    error,
                )
            })?;
        }
    }
    boundary_hook(PublicationBoundaryV2::FinalMoved, &stage_path, &final_path)?;

    // Unix performs the directory fsync here. Windows has already requested
    // MOVEFILE_WRITE_THROUGH and this call still revalidates the parent; it does
    // not pretend that std exposes a Windows directory fsync.
    sync_parent_namespace_v1(parent)?;
    boundary_hook(
        PublicationBoundaryV2::ParentNamespaceSynced,
        &stage_path,
        &final_path,
    )?;
    recapture_exact_file_v1(
        &final_path,
        stage_identity,
        expectation,
        DurablePublicationErrorKindV1::FinalVerification,
    )?;
    boundary_hook(
        PublicationBoundaryV2::FinalVerified,
        &stage_path,
        &final_path,
    )?;

    // No test hook follows this final parent check and full reopen/rehash. A
    // successful receipt therefore cannot cross a test-only corruption seam.
    revalidate_parent_v1(parent)?;
    recapture_exact_file_v1(
        &final_path,
        stage_identity,
        expectation,
        DurablePublicationErrorKindV1::FinalVerification,
    )?;
    Ok(PublishedMoveV2 { final_path })
}

fn reject_final_stage_alias_v2(
    final_path: &Path,
    stage_identity: ObjectIdentityV1,
    disposition: MoveDispositionV2,
) -> Result<(), DurablePublicationErrorV1> {
    let inspection_kind = match disposition {
        MoveDispositionV2::ImmutableNoReplace => DurablePublicationErrorKindV1::FinalCollision,
        MoveDispositionV2::Replace => DurablePublicationErrorKindV1::FinalPublish,
    };
    if recapture_existing_regular_identity_v1(final_path, inspection_kind)? == Some(stage_identity)
    {
        return Err(DurablePublicationErrorV1::new(
            DurablePublicationErrorKindV1::FinalCollision,
            "staging and final child paths resolve to the same filesystem object",
        ));
    }
    Ok(())
}

fn immutable_move_error_kind_v2(
    final_path: &Path,
    error: &io::Error,
) -> DurablePublicationErrorKindV1 {
    let explicit_collision = error.kind() == io::ErrorKind::AlreadyExists
        || matches!(error.raw_os_error(), Some(80 | 183));
    if explicit_collision || fs::symlink_metadata(final_path).is_ok() {
        DurablePublicationErrorKindV1::FinalCollision
    } else {
        DurablePublicationErrorKindV1::FinalPublish
    }
}

#[cfg(windows)]
fn immutable_mechanism_v2() -> ImmutableMoveMechanismV2 {
    ImmutableMoveMechanismV2::WindowsMoveFileExWriteThroughNoReplace
}

#[cfg(unix)]
fn immutable_mechanism_v2() -> ImmutableMoveMechanismV2 {
    ImmutableMoveMechanismV2::UnixAtomicRenameNoReplaceDirectorySynced
}

#[cfg(not(any(unix, windows)))]
fn immutable_mechanism_v2() -> ImmutableMoveMechanismV2 {
    unreachable!("unsupported platforms cannot produce publication receipts")
}

#[cfg(windows)]
fn replacement_mechanism_v2() -> ReplacementMoveMechanismV2 {
    ReplacementMoveMechanismV2::WindowsMoveFileExReplaceExistingWriteThrough
}

#[cfg(unix)]
fn replacement_mechanism_v2() -> ReplacementMoveMechanismV2 {
    ReplacementMoveMechanismV2::UnixAtomicRenameReplaceDirectorySynced
}

#[cfg(not(any(unix, windows)))]
fn replacement_mechanism_v2() -> ReplacementMoveMechanismV2 {
    unreachable!("unsupported platforms cannot produce replacement receipts")
}

#[cfg(windows)]
mod windows_move_v2 {
    use std::io;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;

    const MOVEFILE_REPLACE_EXISTING_V2: u32 = 0x0000_0001;
    const MOVEFILE_WRITE_THROUGH_V2: u32 = 0x0000_0008;

    #[link(name = "kernel32")]
    extern "system" {
        fn MoveFileExW(
            existing_file_name: *const u16,
            new_file_name: *const u16,
            flags: u32,
        ) -> i32;
    }

    fn wide_path_v2(path: &Path) -> io::Result<Vec<u16>> {
        let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
        if wide.contains(&0) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Windows publication path contains an embedded NUL",
            ));
        }
        wide.push(0);
        Ok(wide)
    }

    fn move_file_ex_v2(stage: &Path, final_path: &Path, flags: u32) -> io::Result<()> {
        let stage = wide_path_v2(stage)?;
        let final_path = wide_path_v2(final_path)?;
        // SAFETY: both vectors are NUL-terminated Windows paths, remain live
        // for the call, and `flags` is a documented MoveFileExW combination.
        let success = unsafe { MoveFileExW(stage.as_ptr(), final_path.as_ptr(), flags) };
        if success == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    pub(super) fn move_no_replace_v2(stage: &Path, final_path: &Path) -> io::Result<()> {
        move_file_ex_v2(stage, final_path, MOVEFILE_WRITE_THROUGH_V2)
    }

    pub(super) fn replace_v2(stage: &Path, final_path: &Path) -> io::Result<()> {
        move_file_ex_v2(
            stage,
            final_path,
            MOVEFILE_REPLACE_EXISTING_V2 | MOVEFILE_WRITE_THROUGH_V2,
        )
    }
}

#[cfg(windows)]
fn move_no_replace_platform_v2(stage: &Path, final_path: &Path) -> io::Result<()> {
    windows_move_v2::move_no_replace_v2(stage, final_path)
}

#[cfg(windows)]
fn replace_platform_v2(stage: &Path, final_path: &Path) -> io::Result<()> {
    windows_move_v2::replace_v2(stage, final_path)
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "ios"))]
mod unix_no_replace_move_v2 {
    use std::ffi::{c_char, c_int, CString};
    use std::io;
    use std::os::unix::ffi::OsStrExt;
    use std::path::Path;

    fn c_path_v2(path: &Path) -> io::Result<CString> {
        CString::new(path.as_os_str().as_bytes()).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "Unix publication path contains an embedded NUL",
            )
        })
    }

    #[cfg(target_os = "linux")]
    pub(super) fn move_no_replace_v2(stage: &Path, final_path: &Path) -> io::Result<()> {
        const AT_FDCWD_V2: c_int = -100;
        const RENAME_NOREPLACE_V2: u32 = 1;
        unsafe extern "C" {
            fn renameat2(
                old_directory: c_int,
                old_path: *const c_char,
                new_directory: c_int,
                new_path: *const c_char,
                flags: u32,
            ) -> c_int;
        }
        let stage = c_path_v2(stage)?;
        let final_path = c_path_v2(final_path)?;
        // SAFETY: both C strings are NUL-terminated and live for the call;
        // directory descriptors and RENAME_NOREPLACE match renameat2's ABI.
        let result = unsafe {
            renameat2(
                AT_FDCWD_V2,
                stage.as_ptr(),
                AT_FDCWD_V2,
                final_path.as_ptr(),
                RENAME_NOREPLACE_V2,
            )
        };
        if result == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    #[cfg(any(target_os = "macos", target_os = "ios"))]
    pub(super) fn move_no_replace_v2(stage: &Path, final_path: &Path) -> io::Result<()> {
        const RENAME_EXCL_V2: u32 = 0x0000_0004;
        unsafe extern "C" {
            fn renamex_np(old_path: *const c_char, new_path: *const c_char, flags: u32) -> c_int;
        }
        let stage = c_path_v2(stage)?;
        let final_path = c_path_v2(final_path)?;
        // SAFETY: both C strings are NUL-terminated and live for the call;
        // RENAME_EXCL is the documented no-replace flag for renamex_np.
        let result = unsafe { renamex_np(stage.as_ptr(), final_path.as_ptr(), RENAME_EXCL_V2) };
        if result == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "ios"))]
fn move_no_replace_platform_v2(stage: &Path, final_path: &Path) -> io::Result<()> {
    unix_no_replace_move_v2::move_no_replace_v2(stage, final_path)
}

#[cfg(all(
    unix,
    not(any(target_os = "linux", target_os = "macos", target_os = "ios"))
))]
fn move_no_replace_platform_v2(_stage: &Path, _final_path: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "atomic Unix rename-without-replace is unavailable on this target",
    ))
}

#[cfg(unix)]
fn replace_platform_v2(stage: &Path, final_path: &Path) -> io::Result<()> {
    fs::rename(stage, final_path)
}

#[cfg(not(any(unix, windows)))]
fn move_no_replace_platform_v2(_stage: &Path, _final_path: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "move-only immutable publication is unsupported on this platform",
    ))
}

#[cfg(not(any(unix, windows)))]
fn replace_platform_v2(_stage: &Path, _final_path: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "move-only replacement publication is unsupported on this platform",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::durable_publication_v1::capture_existing_publication_parent_v1;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_DIRECTORY_ORDINAL_V2: AtomicU64 = AtomicU64::new(0);

    struct TestDirectoryV2 {
        path: PathBuf,
    }

    impl TestDirectoryV2 {
        fn new(label: &str) -> Self {
            for _ in 0..1_000 {
                let ordinal = TEST_DIRECTORY_ORDINAL_V2.fetch_add(1, Ordering::Relaxed);
                let path = std::env::temp_dir().join(format!(
                    "mtg-kernel-durable-move-v2-{}-{}-{}",
                    std::process::id(),
                    label,
                    ordinal
                ));
                match fs::create_dir(&path) {
                    Ok(()) => return Self { path },
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                    Err(error) => panic!("create isolated test directory: {error}"),
                }
            }
            panic!("cannot allocate isolated test directory");
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TestDirectoryV2 {
        fn drop(&mut self) {
            if let Ok(entries) = fs::read_dir(&self.path) {
                for entry in entries.flatten() {
                    if let Ok(file_type) = entry.file_type() {
                        if file_type.is_file() || file_type.is_symlink() {
                            let _ = fs::remove_file(entry.path());
                        } else if file_type.is_dir() {
                            let _ = fs::remove_dir(entry.path());
                        }
                    }
                }
            }
            let _ = fs::remove_dir(&self.path);
        }
    }

    const ALL_BOUNDARIES_V2: [PublicationBoundaryV2; 9] = [
        PublicationBoundaryV2::ParentValidated,
        PublicationBoundaryV2::StageCreated,
        PublicationBoundaryV2::StageWritten,
        PublicationBoundaryV2::StageSynced,
        PublicationBoundaryV2::StageVerified,
        PublicationBoundaryV2::StageReverified,
        PublicationBoundaryV2::FinalMoved,
        PublicationBoundaryV2::ParentNamespaceSynced,
        PublicationBoundaryV2::FinalVerified,
    ];

    fn injected_fault_v2(boundary: PublicationBoundaryV2) -> DurablePublicationErrorV1 {
        DurablePublicationErrorV1::new(
            DurablePublicationErrorKindV1::InjectedFault,
            format!("injected fault at {boundary:?}"),
        )
    }

    fn overwrite_same_length_v2(path: &Path, length: usize) {
        let mut file = OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(path)
            .expect("open corruption target");
        file.write_all(&vec![0xa5; length])
            .expect("write corrupt bytes");
        file.sync_all().expect("sync corrupt bytes");
    }

    fn substitute_distinct_same_content_object_v2(path: &Path, bytes: &[u8]) {
        let before = recapture_existing_regular_identity_v1(
            path,
            DurablePublicationErrorKindV1::FinalVerification,
        )
        .unwrap()
        .unwrap();
        fs::remove_file(path).expect("remove substitution source");
        let mut replacement = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .expect("create distinct substitution object");
        replacement
            .write_all(bytes)
            .expect("write identical substitution bytes");
        replacement
            .sync_all()
            .expect("sync identical substitution bytes");
        drop(replacement);
        let after = recapture_existing_regular_identity_v1(
            path,
            DurablePublicationErrorKindV1::FinalVerification,
        )
        .unwrap()
        .unwrap();
        assert_ne!(before, after, "substitution reused the original identity");
        assert_eq!(fs::read(path).unwrap(), bytes);
    }

    fn create_different_name_same_identity_alias_v2(stage_path: &Path, final_path: &Path) {
        fs::hard_link(stage_path, final_path).expect("create hard-link alias");
        let stage_identity = recapture_existing_regular_identity_v1(
            stage_path,
            DurablePublicationErrorKindV1::StageVerification,
        )
        .unwrap()
        .unwrap();
        let final_identity = recapture_existing_regular_identity_v1(
            final_path,
            DurablePublicationErrorKindV1::FinalCollision,
        )
        .unwrap()
        .unwrap();
        assert_eq!(stage_identity, final_identity);
        assert_ne!(stage_path, final_path);
    }

    fn assert_receipt_content_v2(
        final_path: &Path,
        expected_bytes: &[u8],
        exact_length: u64,
        sha256: [u8; 32],
    ) {
        assert_eq!(fs::read(final_path).unwrap(), expected_bytes);
        assert_eq!(exact_length, expected_bytes.len() as u64);
        assert_eq!(
            sha256,
            DurableFileExpectationV1::from_bytes(expected_bytes)
                .unwrap()
                .sha256()
        );
    }

    #[test]
    fn immutable_and_replacement_moves_return_distinct_exact_receipts() {
        let directory = TestDirectoryV2::new("success");
        let parent = capture_existing_publication_parent_v1(directory.path()).unwrap();
        let immutable_bytes = b"immutable raw checkpoint bytes";
        let immutable_expectation = DurableFileExpectationV1::from_bytes(immutable_bytes).unwrap();
        let immutable = publish_immutable_file_by_move_v2(
            &parent,
            OsStr::new("immutable.stage"),
            OsStr::new("immutable.bin"),
            immutable_bytes,
            immutable_expectation,
        )
        .unwrap();
        assert!(!directory.path().join("immutable.stage").exists());
        assert_receipt_content_v2(
            immutable.final_path(),
            immutable_bytes,
            immutable.exact_length(),
            immutable.sha256(),
        );
        #[cfg(windows)]
        assert_eq!(
            immutable.mechanism(),
            ImmutableMoveMechanismV2::WindowsMoveFileExWriteThroughNoReplace
        );
        #[cfg(unix)]
        assert_eq!(
            immutable.mechanism(),
            ImmutableMoveMechanismV2::UnixAtomicRenameNoReplaceDirectorySynced
        );

        fs::write(directory.path().join("pointer.bin"), b"old pointer").unwrap();
        let replacement_bytes = b"new pointer";
        let replacement_expectation =
            DurableFileExpectationV1::from_bytes(replacement_bytes).unwrap();
        let replacement = replace_file_by_move_v2(
            &parent,
            OsStr::new("pointer.stage"),
            OsStr::new("pointer.bin"),
            replacement_bytes,
            replacement_expectation,
        )
        .unwrap();
        assert!(!directory.path().join("pointer.stage").exists());
        assert_receipt_content_v2(
            replacement.final_path(),
            replacement_bytes,
            replacement.exact_length(),
            replacement.sha256(),
        );
        #[cfg(windows)]
        assert_eq!(
            replacement.mechanism(),
            ReplacementMoveMechanismV2::WindowsMoveFileExReplaceExistingWriteThrough
        );
        #[cfg(unix)]
        assert_eq!(
            replacement.mechanism(),
            ReplacementMoveMechanismV2::UnixAtomicRenameReplaceDirectorySynced
        );

        let initial_bytes = b"initial pointer";
        let initial_expectation = DurableFileExpectationV1::from_bytes(initial_bytes).unwrap();
        replace_file_by_move_v2(
            &parent,
            OsStr::new("initial.stage"),
            OsStr::new("initial.bin"),
            initial_bytes,
            initial_expectation,
        )
        .unwrap();
        assert_eq!(
            fs::read(directory.path().join("initial.bin")).unwrap(),
            initial_bytes
        );
    }

    #[test]
    fn every_fault_boundary_returns_no_receipt_for_both_move_modes() {
        let bytes = b"fault-boundary-exact-move-content";
        let expectation = DurableFileExpectationV1::from_bytes(bytes).unwrap();
        for disposition in [
            MoveDispositionV2::ImmutableNoReplace,
            MoveDispositionV2::Replace,
        ] {
            for target in ALL_BOUNDARIES_V2 {
                let directory = TestDirectoryV2::new("fault");
                let parent = capture_existing_publication_parent_v1(directory.path()).unwrap();
                if disposition == MoveDispositionV2::Replace {
                    fs::write(directory.path().join("final.bin"), b"old pointer").unwrap();
                }
                let result = publish_file_by_move_with_hook_v2(
                    &parent,
                    OsStr::new("candidate.stage"),
                    OsStr::new("final.bin"),
                    bytes,
                    expectation,
                    disposition,
                    |boundary, _, _| {
                        if boundary == target {
                            Err(injected_fault_v2(boundary))
                        } else {
                            Ok(())
                        }
                    },
                );
                assert_eq!(
                    result.unwrap_err().kind(),
                    DurablePublicationErrorKindV1::InjectedFault,
                    "{disposition:?} at {target:?}"
                );
                let final_path = directory.path().join("final.bin");
                if matches!(
                    target,
                    PublicationBoundaryV2::FinalMoved
                        | PublicationBoundaryV2::ParentNamespaceSynced
                        | PublicationBoundaryV2::FinalVerified
                ) {
                    assert_eq!(
                        fs::read(final_path).unwrap(),
                        bytes,
                        "{disposition:?} at {target:?}"
                    );
                } else if disposition == MoveDispositionV2::Replace {
                    assert_eq!(fs::read(final_path).unwrap(), b"old pointer", "{target:?}");
                } else {
                    assert!(!final_path.exists(), "{target:?}");
                }
            }
        }
    }

    #[test]
    fn every_corruption_boundary_is_rejected_for_both_move_modes() {
        let mutation_boundaries = [
            PublicationBoundaryV2::StageWritten,
            PublicationBoundaryV2::StageSynced,
            PublicationBoundaryV2::StageVerified,
            PublicationBoundaryV2::StageReverified,
            PublicationBoundaryV2::FinalMoved,
            PublicationBoundaryV2::ParentNamespaceSynced,
            PublicationBoundaryV2::FinalVerified,
        ];
        let bytes = b"same-length-move-corruption-content";
        let expectation = DurableFileExpectationV1::from_bytes(bytes).unwrap();
        for disposition in [
            MoveDispositionV2::ImmutableNoReplace,
            MoveDispositionV2::Replace,
        ] {
            for target in mutation_boundaries {
                let directory = TestDirectoryV2::new("corruption");
                let parent = capture_existing_publication_parent_v1(directory.path()).unwrap();
                if disposition == MoveDispositionV2::Replace {
                    fs::write(directory.path().join("final.bin"), b"old pointer").unwrap();
                }
                let result = publish_file_by_move_with_hook_v2(
                    &parent,
                    OsStr::new("candidate.stage"),
                    OsStr::new("final.bin"),
                    bytes,
                    expectation,
                    disposition,
                    |boundary, stage_path, final_path| {
                        if boundary == target {
                            let mutation_path = if matches!(
                                boundary,
                                PublicationBoundaryV2::FinalMoved
                                    | PublicationBoundaryV2::ParentNamespaceSynced
                                    | PublicationBoundaryV2::FinalVerified
                            ) {
                                final_path
                            } else {
                                stage_path
                            };
                            overwrite_same_length_v2(mutation_path, bytes.len());
                        }
                        Ok(())
                    },
                );
                assert!(
                    result.is_err(),
                    "corruption accepted for {disposition:?} at {target:?}"
                );
                let expected_kind = if matches!(
                    target,
                    PublicationBoundaryV2::FinalMoved
                        | PublicationBoundaryV2::ParentNamespaceSynced
                        | PublicationBoundaryV2::FinalVerified
                ) {
                    DurablePublicationErrorKindV1::FinalVerification
                } else {
                    DurablePublicationErrorKindV1::StageVerification
                };
                assert_eq!(
                    result.unwrap_err().kind(),
                    expected_kind,
                    "{disposition:?} at {target:?}"
                );
                let final_path = directory.path().join("final.bin");
                if matches!(
                    target,
                    PublicationBoundaryV2::FinalMoved
                        | PublicationBoundaryV2::ParentNamespaceSynced
                        | PublicationBoundaryV2::FinalVerified
                ) {
                    assert_eq!(
                        fs::read(final_path).unwrap(),
                        vec![0xa5; bytes.len()],
                        "{disposition:?} at {target:?}"
                    );
                } else if disposition == MoveDispositionV2::Replace {
                    assert_eq!(fs::read(final_path).unwrap(), b"old pointer", "{target:?}");
                } else {
                    assert!(!final_path.exists(), "{target:?}");
                }
            }
        }
    }

    #[test]
    fn collisions_preserve_existing_objects_and_return_exact_error_kinds() {
        let bytes = b"new publication bytes";
        let expectation = DurableFileExpectationV1::from_bytes(bytes).unwrap();

        for disposition in [
            MoveDispositionV2::ImmutableNoReplace,
            MoveDispositionV2::Replace,
        ] {
            let directory = TestDirectoryV2::new("stage-collision");
            fs::write(directory.path().join("candidate.stage"), b"old debris").unwrap();
            let parent = capture_existing_publication_parent_v1(directory.path()).unwrap();
            let error = publish_file_by_move_with_hook_v2(
                &parent,
                OsStr::new("candidate.stage"),
                OsStr::new("final.bin"),
                bytes,
                expectation,
                disposition,
                |_, _, _| Ok(()),
            )
            .unwrap_err();
            assert_eq!(error.kind(), DurablePublicationErrorKindV1::StageCollision);
            assert_eq!(
                fs::read(directory.path().join("candidate.stage")).unwrap(),
                b"old debris"
            );
            assert!(!directory.path().join("final.bin").exists());
        }

        let directory = TestDirectoryV2::new("final-collision");
        fs::write(directory.path().join("final.bin"), b"old immutable").unwrap();
        let parent = capture_existing_publication_parent_v1(directory.path()).unwrap();
        let error = publish_immutable_file_by_move_v2(
            &parent,
            OsStr::new("candidate.stage"),
            OsStr::new("final.bin"),
            bytes,
            expectation,
        )
        .unwrap_err();
        assert_eq!(error.kind(), DurablePublicationErrorKindV1::FinalCollision);
        assert_eq!(
            fs::read(directory.path().join("final.bin")).unwrap(),
            b"old immutable"
        );
        assert_eq!(
            fs::read(directory.path().join("candidate.stage")).unwrap(),
            bytes
        );
    }

    #[test]
    fn hardlink_alias_is_rejected_immediately_before_both_move_modes() {
        let bytes = b"same-object-alias-content";
        let expectation = DurableFileExpectationV1::from_bytes(bytes).unwrap();
        for disposition in [
            MoveDispositionV2::ImmutableNoReplace,
            MoveDispositionV2::Replace,
        ] {
            let directory = TestDirectoryV2::new("hardlink-alias");
            let parent = capture_existing_publication_parent_v1(directory.path()).unwrap();
            let error = publish_file_by_move_with_hook_v2(
                &parent,
                OsStr::new("candidate.stage"),
                OsStr::new("different-final.bin"),
                bytes,
                expectation,
                disposition,
                |boundary, stage_path, final_path| {
                    if boundary == PublicationBoundaryV2::StageReverified {
                        create_different_name_same_identity_alias_v2(stage_path, final_path);
                    }
                    Ok(())
                },
            )
            .unwrap_err();
            assert_eq!(
                error.kind(),
                DurablePublicationErrorKindV1::FinalCollision,
                "{disposition:?}"
            );
            assert_eq!(
                fs::read(directory.path().join("candidate.stage")).unwrap(),
                bytes
            );
            assert_eq!(
                fs::read(directory.path().join("different-final.bin")).unwrap(),
                bytes
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn public_move_apis_reject_case_folded_stage_final_aliases() {
        let bytes = b"case-folded-alias-content";
        let expectation = DurableFileExpectationV1::from_bytes(bytes).unwrap();

        let immutable_directory = TestDirectoryV2::new("immutable-case-alias");
        let immutable_parent =
            capture_existing_publication_parent_v1(immutable_directory.path()).unwrap();
        let immutable_error = publish_immutable_file_by_move_v2(
            &immutable_parent,
            OsStr::new("MixedCase.Stage"),
            OsStr::new("mixedcase.stage"),
            bytes,
            expectation,
        )
        .unwrap_err();
        assert_eq!(
            immutable_error.kind(),
            DurablePublicationErrorKindV1::FinalCollision
        );
        assert_eq!(
            fs::read(immutable_directory.path().join("MixedCase.Stage")).unwrap(),
            bytes
        );

        let replacement_directory = TestDirectoryV2::new("replacement-case-alias");
        let replacement_parent =
            capture_existing_publication_parent_v1(replacement_directory.path()).unwrap();
        let replacement_error = replace_file_by_move_v2(
            &replacement_parent,
            OsStr::new("Pointer.Stage"),
            OsStr::new("pointer.stage"),
            bytes,
            expectation,
        )
        .unwrap_err();
        assert_eq!(
            replacement_error.kind(),
            DurablePublicationErrorKindV1::FinalCollision
        );
        assert_eq!(
            fs::read(replacement_directory.path().join("Pointer.Stage")).unwrap(),
            bytes
        );
    }

    #[test]
    fn distinct_same_content_stage_and_final_substitution_fail_both_move_modes() {
        let bytes = b"identity-substitution-content";
        let expectation = DurableFileExpectationV1::from_bytes(bytes).unwrap();
        for disposition in [
            MoveDispositionV2::ImmutableNoReplace,
            MoveDispositionV2::Replace,
        ] {
            for target in [
                PublicationBoundaryV2::StageReverified,
                PublicationBoundaryV2::FinalMoved,
            ] {
                let directory = TestDirectoryV2::new("object-substitution");
                let parent = capture_existing_publication_parent_v1(directory.path()).unwrap();
                if disposition == MoveDispositionV2::Replace {
                    fs::write(directory.path().join("final.bin"), b"old pointer").unwrap();
                }
                let result = publish_file_by_move_with_hook_v2(
                    &parent,
                    OsStr::new("candidate.stage"),
                    OsStr::new("final.bin"),
                    bytes,
                    expectation,
                    disposition,
                    |boundary, stage_path, final_path| {
                        if boundary == target {
                            let substitution_path =
                                if target == PublicationBoundaryV2::StageReverified {
                                    stage_path
                                } else {
                                    final_path
                                };
                            substitute_distinct_same_content_object_v2(substitution_path, bytes);
                        }
                        Ok(())
                    },
                );
                let expected_kind = if target == PublicationBoundaryV2::StageReverified {
                    DurablePublicationErrorKindV1::StageVerification
                } else {
                    DurablePublicationErrorKindV1::FinalVerification
                };
                assert_eq!(
                    result.unwrap_err().kind(),
                    expected_kind,
                    "{disposition:?} at {target:?}"
                );
                if target == PublicationBoundaryV2::StageReverified {
                    if disposition == MoveDispositionV2::Replace {
                        assert_eq!(
                            fs::read(directory.path().join("final.bin")).unwrap(),
                            b"old pointer"
                        );
                    } else {
                        assert!(!directory.path().join("final.bin").exists());
                    }
                } else {
                    assert_eq!(fs::read(directory.path().join("final.bin")).unwrap(), bytes);
                }
            }
        }
    }

    #[test]
    fn replacement_rejects_non_regular_destination_without_mutating_it() {
        let directory = TestDirectoryV2::new("replace-directory");
        let destination = directory.path().join("final.bin");
        fs::create_dir(&destination).unwrap();
        let parent = capture_existing_publication_parent_v1(directory.path()).unwrap();
        let bytes = b"replacement bytes";
        let expectation = DurableFileExpectationV1::from_bytes(bytes).unwrap();
        let error = replace_file_by_move_v2(
            &parent,
            OsStr::new("candidate.stage"),
            OsStr::new("final.bin"),
            bytes,
            expectation,
        )
        .unwrap_err();
        assert_eq!(error.kind(), DurablePublicationErrorKindV1::FinalPublish);
        assert!(destination.is_dir());
        assert_eq!(
            fs::read(directory.path().join("candidate.stage")).unwrap(),
            bytes
        );
    }

    #[test]
    fn caller_mismatch_and_invalid_names_fail_before_publication() {
        let directory = TestDirectoryV2::new("input");
        let parent = capture_existing_publication_parent_v1(directory.path()).unwrap();
        let bytes = b"borrowed caller bytes";
        let wrong = DurableFileExpectationV1::from_parts(bytes.len() as u64, [0; 32]);
        let mismatch = publish_immutable_file_by_move_v2(
            &parent,
            OsStr::new("candidate.stage"),
            OsStr::new("final.bin"),
            bytes,
            wrong,
        )
        .unwrap_err();
        assert_eq!(
            mismatch.kind(),
            DurablePublicationErrorKindV1::InputContentMismatch
        );
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);

        let expectation = DurableFileExpectationV1::from_bytes(bytes).unwrap();
        for invalid in ["", ".", "..", "nested/child"] {
            assert!(
                replace_file_by_move_v2(
                    &parent,
                    OsStr::new(invalid),
                    OsStr::new("final.bin"),
                    bytes,
                    expectation,
                )
                .is_err(),
                "invalid child accepted: {invalid:?}"
            );
        }
        assert!(publish_immutable_file_by_move_v2(
            &parent,
            OsStr::new("same.bin"),
            OsStr::new("same.bin"),
            bytes,
            expectation,
        )
        .is_err());
        #[cfg(windows)]
        for invalid in ["nested\\child", "alternate:data"] {
            assert!(
                publish_immutable_file_by_move_v2(
                    &parent,
                    OsStr::new(invalid),
                    OsStr::new("final.bin"),
                    bytes,
                    expectation,
                )
                .is_err(),
                "invalid Windows child accepted: {invalid:?}"
            );
        }
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
    }
}
