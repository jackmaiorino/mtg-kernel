//! Schema-neutral primitives for publishing one immutable file without overwrite.
//!
//! Publication uses a `create_new` staging file in the already-existing target
//! directory. The staged bytes are written, file-synced, reopened, and checked
//! against an exact length and SHA-256. [`std::fs::hard_link`] then creates the
//! absent final directory entry without replacing anything. A hard link makes
//! the complete, already-synced inode visible in one namespace operation; there
//! is no partially-written final-file interval. The final path is reopened and
//! verified before and after the staging name is removed.
//!
//! The filesystem must support same-volume hard links. Unsupported filesystems
//! fail closed and leave no accepted receipt. On Unix, both final-link creation
//! and staging-name removal are followed by a directory `sync_all`. Rust's
//! standard library does not expose a portable Windows directory-handle flush,
//! so Windows guarantees synced file data and an atomic no-replace namespace
//! operation during execution, but not survival of that directory entry across
//! sudden power loss. The same namespace-durability limitation applies on other
//! non-Unix targets.
//!
//! All operations are path based. Parent and file identities are revalidated at
//! every material boundary on Windows and Unix, and symlink/reparse ancestors
//! are rejected. Nevertheless, the standard library cannot make the entire
//! sequence race-free against a hostile process concurrently renaming directory
//! ancestors or rewriting a linked inode. Callers must exclusively own their
//! store directory. Every successful return reflects a final reopen performed
//! immediately before the receipt; later external mutation is out of scope.

use sha2::{Digest, Sha256};
use std::error::Error;
use std::ffi::OsStr;
use std::fmt;
use std::fs::{self, File, Metadata, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};

const RECAPTURE_BUFFER_BYTES_V1: usize = 64 * 1024;

/// Exact content required at every accepted publication boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DurableFileExpectationV1 {
    exact_length: u64,
    sha256: [u8; 32],
}

impl DurableFileExpectationV1 {
    /// Constructs an expectation from an externally authoritative length and digest.
    pub const fn from_parts(exact_length: u64, sha256: [u8; 32]) -> Self {
        Self {
            exact_length,
            sha256,
        }
    }

    /// Computes the exact expectation for borrowed caller bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, DurablePublicationErrorV1> {
        let exact_length = u64::try_from(bytes.len()).map_err(|_| {
            DurablePublicationErrorV1::new(
                DurablePublicationErrorKindV1::InputContentMismatch,
                "caller byte length does not fit u64",
            )
        })?;
        Ok(Self {
            exact_length,
            sha256: sha256_v1(bytes),
        })
    }

    pub const fn exact_length(&self) -> u64 {
        self.exact_length
    }

    pub const fn sha256(&self) -> [u8; 32] {
        self.sha256
    }
}

/// Captured, identity-bound existing directory used for publication.
#[derive(Clone, Debug)]
pub struct ValidatedPublicationParentV1 {
    canonical_path: PathBuf,
    identity: ObjectIdentityV1,
}

impl ValidatedPublicationParentV1 {
    /// Returns the canonical directory captured by
    /// [`capture_existing_publication_parent_v1`].
    pub fn canonical_path(&self) -> &Path {
        &self.canonical_path
    }
}

/// Receipt returned only after the final path has been reopened and reverified.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DurablePublicationReceiptV1 {
    final_path: PathBuf,
    exact_length: u64,
    sha256: [u8; 32],
}

impl DurablePublicationReceiptV1 {
    pub fn final_path(&self) -> &Path {
        &self.final_path
    }

    pub const fn exact_length(&self) -> u64 {
        self.exact_length
    }

    pub const fn sha256(&self) -> [u8; 32] {
        self.sha256
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DurablePublicationErrorKindV1 {
    InvalidParent,
    ParentChanged,
    InvalidChildName,
    InputContentMismatch,
    StageCollision,
    StageCreate,
    StageWrite,
    StageSync,
    StageVerification,
    FinalCollision,
    FinalPublish,
    ParentNamespaceSync,
    FinalVerification,
    StageCleanup,
    UnsupportedPlatform,
    #[cfg(test)]
    InjectedFault,
}

#[derive(Debug)]
pub struct DurablePublicationErrorV1 {
    kind: DurablePublicationErrorKindV1,
    detail: String,
    source: Option<io::Error>,
}

impl DurablePublicationErrorV1 {
    pub(crate) fn new(kind: DurablePublicationErrorKindV1, detail: impl Into<String>) -> Self {
        Self {
            kind,
            detail: detail.into(),
            source: None,
        }
    }

    pub(crate) fn with_io(
        kind: DurablePublicationErrorKindV1,
        detail: impl Into<String>,
        source: io::Error,
    ) -> Self {
        Self {
            kind,
            detail: detail.into(),
            source: Some(source),
        }
    }

    pub const fn kind(&self) -> DurablePublicationErrorKindV1 {
        self.kind
    }
}

impl fmt::Display for DurablePublicationErrorV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "durable publication {:?}: {}",
            self.kind, self.detail
        )
    }
}

impl Error for DurablePublicationErrorV1 {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source
            .as_ref()
            .map(|source| source as &(dyn Error + 'static))
    }
}

#[cfg(unix)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ObjectIdentityV1 {
    device: u64,
    inode: u64,
}

#[cfg(windows)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ObjectIdentityV1 {
    volume_serial_number: u32,
    file_index: u64,
}

#[cfg(not(any(unix, windows)))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ObjectIdentityV1;

#[cfg(unix)]
fn object_identity_v1(metadata: &Metadata) -> Result<ObjectIdentityV1, DurablePublicationErrorV1> {
    use std::os::unix::fs::MetadataExt;
    Ok(ObjectIdentityV1 {
        device: metadata.dev(),
        inode: metadata.ino(),
    })
}

#[cfg(windows)]
mod windows_identity_v1 {
    use super::{
        DurablePublicationErrorKindV1, DurablePublicationErrorV1, File, ObjectIdentityV1, Path,
    };
    use std::ffi::c_void;
    use std::mem::MaybeUninit;
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::AsRawHandle;

    type HandleV1 = *mut c_void;
    const INVALID_HANDLE_VALUE_V1: HandleV1 = -1_isize as HandleV1;
    const FILE_SHARE_READ_V1: u32 = 0x0000_0001;
    const FILE_SHARE_WRITE_V1: u32 = 0x0000_0002;
    const FILE_SHARE_DELETE_V1: u32 = 0x0000_0004;
    const OPEN_EXISTING_V1: u32 = 3;
    const FILE_FLAG_OPEN_REPARSE_POINT_V1: u32 = 0x0020_0000;
    const FILE_FLAG_BACKUP_SEMANTICS_V1: u32 = 0x0200_0000;

    #[repr(C)]
    struct FileTimeV1 {
        _low: u32,
        _high: u32,
    }

    #[repr(C)]
    struct ByHandleFileInformationV1 {
        _file_attributes: u32,
        _creation_time: FileTimeV1,
        _last_access_time: FileTimeV1,
        _last_write_time: FileTimeV1,
        volume_serial_number: u32,
        _file_size_high: u32,
        _file_size_low: u32,
        _number_of_links: u32,
        file_index_high: u32,
        file_index_low: u32,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateFileW(
            file_name: *const u16,
            desired_access: u32,
            share_mode: u32,
            security_attributes: *mut c_void,
            creation_disposition: u32,
            flags_and_attributes: u32,
            template_file: HandleV1,
        ) -> HandleV1;
        fn GetFileInformationByHandle(
            file: HandleV1,
            information: *mut ByHandleFileInformationV1,
        ) -> i32;
        fn CloseHandle(object: HandleV1) -> i32;
    }

    fn from_handle_v1(handle: HandleV1) -> Result<ObjectIdentityV1, DurablePublicationErrorV1> {
        let mut information = MaybeUninit::<ByHandleFileInformationV1>::uninit();
        // SAFETY: `handle` is live for this call and the out pointer references
        // enough aligned storage for BY_HANDLE_FILE_INFORMATION.
        let success = unsafe { GetFileInformationByHandle(handle, information.as_mut_ptr()) };
        if success == 0 {
            return Err(DurablePublicationErrorV1::with_io(
                DurablePublicationErrorKindV1::UnsupportedPlatform,
                "cannot query stable Windows filesystem-object identity",
                std::io::Error::last_os_error(),
            ));
        }
        // SAFETY: the successful Windows call initialized the entire structure.
        let information = unsafe { information.assume_init() };
        Ok(ObjectIdentityV1 {
            volume_serial_number: information.volume_serial_number,
            file_index: (u64::from(information.file_index_high) << 32)
                | u64::from(information.file_index_low),
        })
    }

    pub(super) fn from_file_v1(file: &File) -> Result<ObjectIdentityV1, DurablePublicationErrorV1> {
        from_handle_v1(file.as_raw_handle().cast())
    }

    pub(super) fn from_path_v1(path: &Path) -> Result<ObjectIdentityV1, DurablePublicationErrorV1> {
        let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
        if wide.contains(&0) {
            return Err(DurablePublicationErrorV1::new(
                DurablePublicationErrorKindV1::UnsupportedPlatform,
                "Windows path contains an embedded NUL",
            ));
        }
        wide.push(0);
        // Opening with zero desired access is sufficient for an identity query.
        // OPEN_REPARSE_POINT ensures a last-component reparse object is queried
        // rather than silently followed; the metadata gate rejects it separately.
        // SAFETY: `wide` is NUL-terminated and all remaining pointers are null or
        // Windows constants with the documented CreateFileW ABI.
        let handle = unsafe {
            CreateFileW(
                wide.as_ptr(),
                0,
                FILE_SHARE_READ_V1 | FILE_SHARE_WRITE_V1 | FILE_SHARE_DELETE_V1,
                std::ptr::null_mut(),
                OPEN_EXISTING_V1,
                FILE_FLAG_OPEN_REPARSE_POINT_V1 | FILE_FLAG_BACKUP_SEMANTICS_V1,
                std::ptr::null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE_V1 {
            return Err(DurablePublicationErrorV1::with_io(
                DurablePublicationErrorKindV1::UnsupportedPlatform,
                format!("cannot open Windows object identity for {}", path.display()),
                std::io::Error::last_os_error(),
            ));
        }
        let result = from_handle_v1(handle);
        // SAFETY: `handle` was returned live by CreateFileW and is closed once.
        let close_success = unsafe { CloseHandle(handle) };
        if close_success == 0 {
            return Err(DurablePublicationErrorV1::with_io(
                DurablePublicationErrorKindV1::UnsupportedPlatform,
                format!(
                    "cannot close Windows identity handle for {}",
                    path.display()
                ),
                std::io::Error::last_os_error(),
            ));
        }
        result
    }
}

#[cfg(not(any(unix, windows)))]
fn object_identity_v1(_metadata: &Metadata) -> Result<ObjectIdentityV1, DurablePublicationErrorV1> {
    Err(DurablePublicationErrorV1::new(
        DurablePublicationErrorKindV1::UnsupportedPlatform,
        "stable object identity is unavailable on this platform",
    ))
}

#[cfg(unix)]
fn path_identity_v1(
    _path: &Path,
    metadata: &Metadata,
) -> Result<ObjectIdentityV1, DurablePublicationErrorV1> {
    object_identity_v1(metadata)
}

#[cfg(windows)]
fn path_identity_v1(
    path: &Path,
    _metadata: &Metadata,
) -> Result<ObjectIdentityV1, DurablePublicationErrorV1> {
    windows_identity_v1::from_path_v1(path)
}

#[cfg(not(any(unix, windows)))]
fn path_identity_v1(
    _path: &Path,
    metadata: &Metadata,
) -> Result<ObjectIdentityV1, DurablePublicationErrorV1> {
    object_identity_v1(metadata)
}

#[cfg(unix)]
pub(crate) fn file_identity_v1(file: &File) -> Result<ObjectIdentityV1, DurablePublicationErrorV1> {
    object_identity_v1(&file.metadata().map_err(|error| {
        DurablePublicationErrorV1::with_io(
            DurablePublicationErrorKindV1::UnsupportedPlatform,
            "cannot inspect filesystem-object handle identity",
            error,
        )
    })?)
}

#[cfg(windows)]
pub(crate) fn file_identity_v1(file: &File) -> Result<ObjectIdentityV1, DurablePublicationErrorV1> {
    windows_identity_v1::from_file_v1(file)
}

#[cfg(not(any(unix, windows)))]
pub(crate) fn file_identity_v1(
    _file: &File,
) -> Result<ObjectIdentityV1, DurablePublicationErrorV1> {
    Err(DurablePublicationErrorV1::new(
        DurablePublicationErrorKindV1::UnsupportedPlatform,
        "stable filesystem-object handle identity is unavailable on this platform",
    ))
}

#[cfg(windows)]
fn is_reparse_v1(metadata: &Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_reparse_v1(_metadata: &Metadata) -> bool {
    false
}

fn is_link_or_reparse_v1(metadata: &Metadata) -> bool {
    metadata.file_type().is_symlink() || is_reparse_v1(metadata)
}

fn absolute_path_v1(path: &Path) -> Result<PathBuf, DurablePublicationErrorV1> {
    if path.as_os_str().is_empty() {
        return Err(DurablePublicationErrorV1::new(
            DurablePublicationErrorKindV1::InvalidParent,
            "publication parent is empty",
        ));
    }
    if path.is_absolute() {
        Ok(path.to_owned())
    } else {
        std::env::current_dir()
            .map(|current| current.join(path))
            .map_err(|error| {
                DurablePublicationErrorV1::with_io(
                    DurablePublicationErrorKindV1::InvalidParent,
                    "cannot resolve the current directory for a relative parent",
                    error,
                )
            })
    }
}

fn inspect_parent_chain_v1(path: &Path) -> Result<(), DurablePublicationErrorV1> {
    let absolute = absolute_path_v1(path)?;
    let mut ancestors: Vec<&Path> = absolute.ancestors().collect();
    ancestors.reverse();
    for ancestor in ancestors {
        if ancestor.as_os_str().is_empty() {
            continue;
        }
        let metadata = fs::symlink_metadata(ancestor).map_err(|error| {
            DurablePublicationErrorV1::with_io(
                DurablePublicationErrorKindV1::InvalidParent,
                format!(
                    "cannot inspect publication-parent ancestor {}",
                    ancestor.display()
                ),
                error,
            )
        })?;
        if is_link_or_reparse_v1(&metadata) || !metadata.file_type().is_dir() {
            return Err(DurablePublicationErrorV1::new(
                DurablePublicationErrorKindV1::InvalidParent,
                format!(
                    "publication-parent chain contains a link, reparse point, or non-directory: {}",
                    ancestor.display()
                ),
            ));
        }
    }
    Ok(())
}

fn inspect_parent_v1(
    path: &Path,
) -> Result<(PathBuf, ObjectIdentityV1), DurablePublicationErrorV1> {
    inspect_parent_chain_v1(path)?;
    let original_metadata = fs::symlink_metadata(path).map_err(|error| {
        DurablePublicationErrorV1::with_io(
            DurablePublicationErrorKindV1::InvalidParent,
            format!("cannot inspect publication parent {}", path.display()),
            error,
        )
    })?;
    if is_link_or_reparse_v1(&original_metadata) || !original_metadata.file_type().is_dir() {
        return Err(DurablePublicationErrorV1::new(
            DurablePublicationErrorKindV1::InvalidParent,
            format!(
                "publication parent is a link, reparse point, or non-directory: {}",
                path.display()
            ),
        ));
    }
    let original_identity = path_identity_v1(path, &original_metadata)?;
    let canonical_path = fs::canonicalize(path).map_err(|error| {
        DurablePublicationErrorV1::with_io(
            DurablePublicationErrorKindV1::InvalidParent,
            format!("cannot canonicalize publication parent {}", path.display()),
            error,
        )
    })?;
    inspect_parent_chain_v1(&canonical_path)?;
    let canonical_metadata = fs::symlink_metadata(&canonical_path).map_err(|error| {
        DurablePublicationErrorV1::with_io(
            DurablePublicationErrorKindV1::InvalidParent,
            format!(
                "cannot inspect canonical publication parent {}",
                canonical_path.display()
            ),
            error,
        )
    })?;
    if is_link_or_reparse_v1(&canonical_metadata)
        || !canonical_metadata.file_type().is_dir()
        || path_identity_v1(&canonical_path, &canonical_metadata)? != original_identity
    {
        return Err(DurablePublicationErrorV1::new(
            DurablePublicationErrorKindV1::InvalidParent,
            "publication parent changed or resolved through an invalid object",
        ));
    }
    Ok((canonical_path, original_identity))
}

/// Captures an existing parent directory and its stable filesystem identity.
///
/// Every lexical ancestor is inspected. Symlinks, Windows reparse points, and
/// non-directories are rejected before any child path is created.
pub fn capture_existing_publication_parent_v1(
    parent: impl AsRef<Path>,
) -> Result<ValidatedPublicationParentV1, DurablePublicationErrorV1> {
    let (canonical_path, identity) = inspect_parent_v1(parent.as_ref())?;
    Ok(ValidatedPublicationParentV1 {
        canonical_path,
        identity,
    })
}

pub(crate) fn revalidate_parent_v1(
    parent: &ValidatedPublicationParentV1,
) -> Result<(), DurablePublicationErrorV1> {
    let (canonical_path, identity) =
        inspect_parent_v1(&parent.canonical_path).map_err(|error| DurablePublicationErrorV1 {
            kind: DurablePublicationErrorKindV1::ParentChanged,
            detail: error.detail,
            source: error.source,
        })?;
    if canonical_path != parent.canonical_path || identity != parent.identity {
        return Err(DurablePublicationErrorV1::new(
            DurablePublicationErrorKindV1::ParentChanged,
            "publication parent identity changed after capture",
        ));
    }
    Ok(())
}

pub(crate) fn child_path_v1(
    parent: &ValidatedPublicationParentV1,
    name: &OsStr,
) -> Result<PathBuf, DurablePublicationErrorV1> {
    let candidate = Path::new(name);
    let mut components = candidate.components();
    let valid = matches!(components.next(), Some(Component::Normal(component)) if component == name)
        && components.next().is_none();
    if !valid {
        return Err(DurablePublicationErrorV1::new(
            DurablePublicationErrorKindV1::InvalidChildName,
            "publication child name must be one non-empty normal path component",
        ));
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        if name.encode_wide().any(|unit| unit == u16::from(b':')) {
            return Err(DurablePublicationErrorV1::new(
                DurablePublicationErrorKindV1::InvalidChildName,
                "Windows alternate-data-stream child names are forbidden",
            ));
        }
    }
    Ok(parent.canonical_path.join(name))
}

fn sha256_v1(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

pub(crate) fn validate_caller_content_v1(
    bytes: &[u8],
    expectation: DurableFileExpectationV1,
) -> Result<(), DurablePublicationErrorV1> {
    let actual_length = u64::try_from(bytes.len()).map_err(|_| {
        DurablePublicationErrorV1::new(
            DurablePublicationErrorKindV1::InputContentMismatch,
            "caller byte length does not fit u64",
        )
    })?;
    if actual_length != expectation.exact_length || sha256_v1(bytes) != expectation.sha256 {
        return Err(DurablePublicationErrorV1::new(
            DurablePublicationErrorKindV1::InputContentMismatch,
            "borrowed caller bytes do not match the required exact length and SHA-256",
        ));
    }
    Ok(())
}

pub(crate) fn regular_path_metadata_v1(
    path: &Path,
    error_kind: DurablePublicationErrorKindV1,
) -> Result<Metadata, DurablePublicationErrorV1> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        DurablePublicationErrorV1::with_io(
            error_kind,
            format!("cannot inspect publication file {}", path.display()),
            error,
        )
    })?;
    if is_link_or_reparse_v1(&metadata) || !metadata.file_type().is_file() {
        return Err(DurablePublicationErrorV1::new(
            error_kind,
            format!(
                "publication path is a link, reparse point, or non-regular file: {}",
                path.display()
            ),
        ));
    }
    Ok(metadata)
}

pub(crate) fn recapture_exact_file_v1(
    path: &Path,
    required_identity: ObjectIdentityV1,
    expectation: DurableFileExpectationV1,
    error_kind: DurablePublicationErrorKindV1,
) -> Result<(), DurablePublicationErrorV1> {
    let path_before = regular_path_metadata_v1(path, error_kind)?;
    let identity_before =
        path_identity_v1(path, &path_before).map_err(|error| DurablePublicationErrorV1 {
            kind: error_kind,
            detail: error.detail,
            source: error.source,
        })?;
    if identity_before != required_identity || path_before.len() != expectation.exact_length {
        return Err(DurablePublicationErrorV1::new(
            error_kind,
            "publication file identity or exact length differs from the staged object",
        ));
    }

    let mut file = File::open(path).map_err(|error| {
        DurablePublicationErrorV1::with_io(
            error_kind,
            format!("cannot open publication file {}", path.display()),
            error,
        )
    })?;
    let opened_before = file.metadata().map_err(|error| {
        DurablePublicationErrorV1::with_io(
            error_kind,
            format!("cannot inspect opened publication file {}", path.display()),
            error,
        )
    })?;
    if !opened_before.file_type().is_file()
        || file_identity_v1(&file).map_err(|error| DurablePublicationErrorV1 {
            kind: error_kind,
            detail: error.detail,
            source: error.source,
        })? != required_identity
    {
        return Err(DurablePublicationErrorV1::new(
            error_kind,
            "opened publication file does not have the staged object identity",
        ));
    }

    let mut hasher = Sha256::new();
    let mut total_length = 0_u64;
    let mut buffer = [0_u8; RECAPTURE_BUFFER_BYTES_V1];
    loop {
        let read = file.read(&mut buffer).map_err(|error| {
            DurablePublicationErrorV1::with_io(
                error_kind,
                format!("cannot read publication file {}", path.display()),
                error,
            )
        })?;
        if read == 0 {
            break;
        }
        total_length = total_length
            .checked_add(u64::try_from(read).map_err(|_| {
                DurablePublicationErrorV1::new(error_kind, "read length does not fit u64")
            })?)
            .ok_or_else(|| {
                DurablePublicationErrorV1::new(error_kind, "publication file length overflow")
            })?;
        if total_length > expectation.exact_length {
            return Err(DurablePublicationErrorV1::new(
                error_kind,
                "publication file exceeds its required exact length",
            ));
        }
        hasher.update(&buffer[..read]);
    }
    let digest: [u8; 32] = hasher.finalize().into();

    let opened_after = file.metadata().map_err(|error| {
        DurablePublicationErrorV1::with_io(
            error_kind,
            format!(
                "cannot re-inspect opened publication file {}",
                path.display()
            ),
            error,
        )
    })?;
    let path_after = regular_path_metadata_v1(path, error_kind)?;
    let reopened = File::open(path).map_err(|error| {
        DurablePublicationErrorV1::with_io(
            error_kind,
            format!("cannot reopen publication file {}", path.display()),
            error,
        )
    })?;
    let reopened_metadata = reopened.metadata().map_err(|error| {
        DurablePublicationErrorV1::with_io(
            error_kind,
            format!(
                "cannot inspect reopened publication file {}",
                path.display()
            ),
            error,
        )
    })?;
    let identities = [
        file_identity_v1(&file),
        path_identity_v1(path, &path_after),
        file_identity_v1(&reopened),
    ];
    for identity in identities {
        let identity = identity.map_err(|error| DurablePublicationErrorV1 {
            kind: error_kind,
            detail: error.detail,
            source: error.source,
        })?;
        if identity != required_identity {
            return Err(DurablePublicationErrorV1::new(
                error_kind,
                "publication file identity changed during recapture",
            ));
        }
    }
    if opened_after.len() != expectation.exact_length
        || path_after.len() != expectation.exact_length
        || reopened_metadata.len() != expectation.exact_length
        || total_length != expectation.exact_length
        || digest != expectation.sha256
    {
        return Err(DurablePublicationErrorV1::new(
            error_kind,
            "publication file failed exact length or SHA-256 recapture",
        ));
    }
    Ok(())
}

#[cfg(unix)]
pub(crate) fn sync_parent_namespace_v1(
    parent: &ValidatedPublicationParentV1,
) -> Result<(), DurablePublicationErrorV1> {
    revalidate_parent_v1(parent)?;
    let directory = File::open(&parent.canonical_path).map_err(|error| {
        DurablePublicationErrorV1::with_io(
            DurablePublicationErrorKindV1::ParentNamespaceSync,
            format!(
                "cannot open publication parent for namespace sync {}",
                parent.canonical_path.display()
            ),
            error,
        )
    })?;
    directory.sync_all().map_err(|error| {
        DurablePublicationErrorV1::with_io(
            DurablePublicationErrorKindV1::ParentNamespaceSync,
            format!(
                "cannot sync publication-parent namespace {}",
                parent.canonical_path.display()
            ),
            error,
        )
    })
}

#[cfg(not(unix))]
pub(crate) fn sync_parent_namespace_v1(
    parent: &ValidatedPublicationParentV1,
) -> Result<(), DurablePublicationErrorV1> {
    // See the module-level platform guarantee. Revalidation still closes the
    // observable identity boundary even though std cannot flush this namespace.
    revalidate_parent_v1(parent)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PublicationBoundaryV1 {
    ParentValidated,
    StageCreated,
    StageWritten,
    StageSynced,
    StageVerified,
    FinalPublished,
    FinalNamespaceSynced,
    FinalVerified,
    StageRemoved,
    CleanupNamespaceSynced,
}

fn receipt_v1(
    final_path: PathBuf,
    expectation: DurableFileExpectationV1,
) -> DurablePublicationReceiptV1 {
    DurablePublicationReceiptV1 {
        final_path,
        exact_length: expectation.exact_length,
        sha256: expectation.sha256,
    }
}

/// Publishes borrowed bytes under a new final name without overwriting any file.
///
/// `stage_name` and `final_name` must each be one normal path component. Both
/// are resolved beneath the captured parent; an existing staging name is
/// treated as debris and an existing final name as authoritative. Neither is
/// overwritten or removed. The caller's bytes are borrowed and never mutated.
///
/// On any error after final-link creation, a final file may remain. No receipt
/// is returned, so callers must not accept it implicitly; they may use
/// [`verify_existing_publication_v1`] to classify it on recovery. This function
/// never deletes a final path.
pub fn publish_new_file_v1(
    parent: &ValidatedPublicationParentV1,
    stage_name: impl AsRef<OsStr>,
    final_name: impl AsRef<OsStr>,
    bytes: &[u8],
    expectation: DurableFileExpectationV1,
) -> Result<DurablePublicationReceiptV1, DurablePublicationErrorV1> {
    publish_new_file_with_hook_v1(
        parent,
        stage_name.as_ref(),
        final_name.as_ref(),
        bytes,
        expectation,
        |_, _, _| Ok(()),
    )
}

fn publish_new_file_with_hook_v1(
    parent: &ValidatedPublicationParentV1,
    stage_name: &OsStr,
    final_name: &OsStr,
    bytes: &[u8],
    expectation: DurableFileExpectationV1,
    mut boundary_hook: impl FnMut(
        PublicationBoundaryV1,
        &Path,
        &Path,
    ) -> Result<(), DurablePublicationErrorV1>,
) -> Result<DurablePublicationReceiptV1, DurablePublicationErrorV1> {
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
        PublicationBoundaryV1::ParentValidated,
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
        PublicationBoundaryV1::StageCreated,
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
        PublicationBoundaryV1::StageWritten,
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
    boundary_hook(PublicationBoundaryV1::StageSynced, &stage_path, &final_path)?;

    recapture_exact_file_v1(
        &stage_path,
        stage_identity,
        expectation,
        DurablePublicationErrorKindV1::StageVerification,
    )?;
    boundary_hook(
        PublicationBoundaryV1::StageVerified,
        &stage_path,
        &final_path,
    )?;

    revalidate_parent_v1(parent)?;
    // Recheck after the testable boundary and immediately before publication.
    recapture_exact_file_v1(
        &stage_path,
        stage_identity,
        expectation,
        DurablePublicationErrorKindV1::StageVerification,
    )?;
    fs::hard_link(&stage_path, &final_path).map_err(|error| {
        let kind = if error.kind() == io::ErrorKind::AlreadyExists {
            DurablePublicationErrorKindV1::FinalCollision
        } else {
            DurablePublicationErrorKindV1::FinalPublish
        };
        DurablePublicationErrorV1::with_io(
            kind,
            format!(
                "cannot atomically publish staging file {} as new final {}",
                stage_path.display(),
                final_path.display()
            ),
            error,
        )
    })?;
    boundary_hook(
        PublicationBoundaryV1::FinalPublished,
        &stage_path,
        &final_path,
    )?;

    sync_parent_namespace_v1(parent)?;
    boundary_hook(
        PublicationBoundaryV1::FinalNamespaceSynced,
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
        PublicationBoundaryV1::FinalVerified,
        &stage_path,
        &final_path,
    )?;

    // The held handle is no longer needed. Closing it before unlinking keeps
    // Windows behavior independent of share-delete defaults.
    drop(stage_file);
    revalidate_parent_v1(parent)?;
    let stage_before_cleanup =
        regular_path_metadata_v1(&stage_path, DurablePublicationErrorKindV1::StageCleanup)?;
    if path_identity_v1(&stage_path, &stage_before_cleanup).map_err(|error| {
        DurablePublicationErrorV1 {
            kind: DurablePublicationErrorKindV1::StageCleanup,
            detail: error.detail,
            source: error.source,
        }
    })? != stage_identity
    {
        return Err(DurablePublicationErrorV1::new(
            DurablePublicationErrorKindV1::StageCleanup,
            "staging path identity changed before cleanup",
        ));
    }
    fs::remove_file(&stage_path).map_err(|error| {
        DurablePublicationErrorV1::with_io(
            DurablePublicationErrorKindV1::StageCleanup,
            format!("cannot remove exact staging file {}", stage_path.display()),
            error,
        )
    })?;
    boundary_hook(
        PublicationBoundaryV1::StageRemoved,
        &stage_path,
        &final_path,
    )?;
    sync_parent_namespace_v1(parent)?;
    boundary_hook(
        PublicationBoundaryV1::CleanupNamespaceSynced,
        &stage_path,
        &final_path,
    )?;

    // There is deliberately no test hook after this last reopen: no test-only
    // mutation boundary can slip invalid content into a successful receipt.
    recapture_exact_file_v1(
        &final_path,
        stage_identity,
        expectation,
        DurablePublicationErrorKindV1::FinalVerification,
    )?;
    Ok(receipt_v1(final_path, expectation))
}

/// Reopens an existing final path and verifies its exact length and SHA-256.
///
/// This is recovery verification only. It does not make an existing file
/// authoritative by itself and never mutates the filesystem.
pub fn verify_existing_publication_v1(
    parent: &ValidatedPublicationParentV1,
    final_name: impl AsRef<OsStr>,
    expectation: DurableFileExpectationV1,
) -> Result<DurablePublicationReceiptV1, DurablePublicationErrorV1> {
    revalidate_parent_v1(parent)?;
    let final_path = child_path_v1(parent, final_name.as_ref())?;
    let metadata = regular_path_metadata_v1(
        &final_path,
        DurablePublicationErrorKindV1::FinalVerification,
    )?;
    let identity = path_identity_v1(&final_path, &metadata)?;
    recapture_exact_file_v1(
        &final_path,
        identity,
        expectation,
        DurablePublicationErrorKindV1::FinalVerification,
    )?;
    Ok(receipt_v1(final_path, expectation))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_DIRECTORY_ORDINAL_V1: AtomicU64 = AtomicU64::new(0);

    struct TestDirectoryV1 {
        path: PathBuf,
    }

    impl TestDirectoryV1 {
        fn new(label: &str) -> Self {
            for _ in 0..1_000 {
                let ordinal = TEST_DIRECTORY_ORDINAL_V1.fetch_add(1, Ordering::Relaxed);
                let path = std::env::temp_dir().join(format!(
                    "mtg-kernel-durable-publication-v1-{}-{}-{}",
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

    impl Drop for TestDirectoryV1 {
        fn drop(&mut self) {
            if let Ok(entries) = fs::read_dir(&self.path) {
                for entry in entries.flatten() {
                    if let Ok(file_type) = entry.file_type() {
                        if file_type.is_file() || file_type.is_symlink() {
                            let _ = fs::remove_file(entry.path());
                        } else if file_type.is_dir() {
                            // Tests create only direct, empty child directories;
                            // production code contains no recursive deletion.
                            let _ = fs::remove_dir(entry.path());
                        }
                    }
                }
            }
            let _ = fs::remove_dir(&self.path);
        }
    }

    const ALL_BOUNDARIES_V1: [PublicationBoundaryV1; 10] = [
        PublicationBoundaryV1::ParentValidated,
        PublicationBoundaryV1::StageCreated,
        PublicationBoundaryV1::StageWritten,
        PublicationBoundaryV1::StageSynced,
        PublicationBoundaryV1::StageVerified,
        PublicationBoundaryV1::FinalPublished,
        PublicationBoundaryV1::FinalNamespaceSynced,
        PublicationBoundaryV1::FinalVerified,
        PublicationBoundaryV1::StageRemoved,
        PublicationBoundaryV1::CleanupNamespaceSynced,
    ];

    fn injected_fault_v1(boundary: PublicationBoundaryV1) -> DurablePublicationErrorV1 {
        DurablePublicationErrorV1::new(
            DurablePublicationErrorKindV1::InjectedFault,
            format!("injected fault at {boundary:?}"),
        )
    }

    fn overwrite_same_length_v1(path: &Path, length: usize) {
        let mut file = OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(path)
            .expect("open fault target");
        file.write_all(&vec![0xa5; length])
            .expect("write corrupt bytes");
        file.sync_all().expect("sync corrupt bytes");
    }

    #[test]
    fn successful_publication_is_no_replace_exact_and_recoverably_verifiable() {
        let directory = TestDirectoryV1::new("success");
        let parent = capture_existing_publication_parent_v1(directory.path()).unwrap();
        let bytes = b"complete immutable native checkpoint payload";
        let expectation = DurableFileExpectationV1::from_bytes(bytes).unwrap();
        let receipt = publish_new_file_v1(
            &parent,
            OsStr::new("checkpoint.stage"),
            OsStr::new("checkpoint.bin"),
            bytes,
            expectation,
        )
        .unwrap();

        assert!(!directory.path().join("checkpoint.stage").exists());
        assert_eq!(fs::read(receipt.final_path()).unwrap(), bytes);
        assert_eq!(receipt.exact_length(), bytes.len() as u64);
        assert_eq!(receipt.sha256(), sha256_v1(bytes));
        assert_eq!(
            verify_existing_publication_v1(&parent, OsStr::new("checkpoint.bin"), expectation)
                .unwrap(),
            receipt
        );
    }

    #[test]
    fn every_boundary_fault_returns_no_receipt_and_never_leaves_an_invalid_final() {
        let bytes = b"fault-boundary-complete-content";
        let expectation = DurableFileExpectationV1::from_bytes(bytes).unwrap();
        for target in ALL_BOUNDARIES_V1 {
            let directory = TestDirectoryV1::new("boundary-fault");
            let parent = capture_existing_publication_parent_v1(directory.path()).unwrap();
            let result = publish_new_file_with_hook_v1(
                &parent,
                OsStr::new("candidate.stage"),
                OsStr::new("authoritative.bin"),
                bytes,
                expectation,
                |boundary, _, _| {
                    if boundary == target {
                        Err(injected_fault_v1(boundary))
                    } else {
                        Ok(())
                    }
                },
            );
            assert_eq!(
                result.unwrap_err().kind(),
                DurablePublicationErrorKindV1::InjectedFault,
                "{target:?}"
            );
            let final_path = directory.path().join("authoritative.bin");
            if final_path.exists() {
                assert_eq!(fs::read(&final_path).unwrap(), bytes, "{target:?}");
                verify_existing_publication_v1(
                    &parent,
                    OsStr::new("authoritative.bin"),
                    expectation,
                )
                .unwrap();
            }
        }
    }

    #[test]
    fn deterministic_boundary_corruption_never_returns_an_accepted_receipt() {
        let mutation_boundaries = [
            PublicationBoundaryV1::StageWritten,
            PublicationBoundaryV1::StageSynced,
            PublicationBoundaryV1::StageVerified,
            PublicationBoundaryV1::FinalPublished,
            PublicationBoundaryV1::FinalNamespaceSynced,
            PublicationBoundaryV1::FinalVerified,
            PublicationBoundaryV1::StageRemoved,
            PublicationBoundaryV1::CleanupNamespaceSynced,
        ];
        let bytes = b"same-length-content-for-corruption";
        let expectation = DurableFileExpectationV1::from_bytes(bytes).unwrap();
        for target in mutation_boundaries {
            let directory = TestDirectoryV1::new("boundary-corruption");
            let parent = capture_existing_publication_parent_v1(directory.path()).unwrap();
            let result = publish_new_file_with_hook_v1(
                &parent,
                OsStr::new("candidate.stage"),
                OsStr::new("authoritative.bin"),
                bytes,
                expectation,
                |boundary, stage_path, final_path| {
                    if boundary == target {
                        let mutation_path = if matches!(
                            boundary,
                            PublicationBoundaryV1::FinalPublished
                                | PublicationBoundaryV1::FinalNamespaceSynced
                                | PublicationBoundaryV1::FinalVerified
                                | PublicationBoundaryV1::StageRemoved
                                | PublicationBoundaryV1::CleanupNamespaceSynced
                        ) {
                            final_path
                        } else {
                            stage_path
                        };
                        overwrite_same_length_v1(mutation_path, bytes.len());
                    }
                    Ok(())
                },
            );
            assert!(result.is_err(), "corruption was accepted at {target:?}");
            let final_path = directory.path().join("authoritative.bin");
            if final_path.exists() {
                assert!(
                    verify_existing_publication_v1(
                        &parent,
                        OsStr::new("authoritative.bin"),
                        expectation,
                    )
                    .is_err(),
                    "invalid final was accepted at {target:?}"
                );
            }
        }
    }

    #[test]
    fn caller_mismatch_and_invalid_names_fail_before_filesystem_mutation() {
        let directory = TestDirectoryV1::new("input-rejection");
        let parent = capture_existing_publication_parent_v1(directory.path()).unwrap();
        let bytes = b"caller-owned-bytes";
        let preserved = bytes.to_vec();
        let wrong = DurableFileExpectationV1::from_parts(bytes.len() as u64, [0; 32]);
        let mismatch = publish_new_file_v1(
            &parent,
            OsStr::new("candidate.stage"),
            OsStr::new("authoritative.bin"),
            bytes,
            wrong,
        )
        .unwrap_err();
        assert_eq!(
            mismatch.kind(),
            DurablePublicationErrorKindV1::InputContentMismatch
        );
        assert_eq!(bytes.as_slice(), preserved.as_slice());
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);

        let expectation = DurableFileExpectationV1::from_bytes(bytes).unwrap();
        for invalid in ["", ".", "..", "nested/child"] {
            let result = publish_new_file_v1(
                &parent,
                OsStr::new(invalid),
                OsStr::new("authoritative.bin"),
                bytes,
                expectation,
            );
            assert!(result.is_err(), "invalid name accepted: {invalid:?}");
        }
        #[cfg(windows)]
        for invalid in ["nested\\child", "alternate:data"] {
            assert!(
                publish_new_file_v1(
                    &parent,
                    OsStr::new(invalid),
                    OsStr::new("authoritative.bin"),
                    bytes,
                    expectation,
                )
                .is_err(),
                "invalid Windows name accepted: {invalid:?}"
            );
        }
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
    }

    #[test]
    fn collisions_preserve_preexisting_stage_and_final_bytes() {
        let bytes = b"new publication bytes";
        let expectation = DurableFileExpectationV1::from_bytes(bytes).unwrap();

        let stage_directory = TestDirectoryV1::new("stage-collision");
        fs::write(
            stage_directory.path().join("candidate.stage"),
            b"old stage debris",
        )
        .unwrap();
        let stage_parent = capture_existing_publication_parent_v1(stage_directory.path()).unwrap();
        let stage_error = publish_new_file_v1(
            &stage_parent,
            OsStr::new("candidate.stage"),
            OsStr::new("authoritative.bin"),
            bytes,
            expectation,
        )
        .unwrap_err();
        assert_eq!(
            stage_error.kind(),
            DurablePublicationErrorKindV1::StageCollision
        );
        assert_eq!(
            fs::read(stage_directory.path().join("candidate.stage")).unwrap(),
            b"old stage debris"
        );
        assert!(!stage_directory.path().join("authoritative.bin").exists());

        let final_directory = TestDirectoryV1::new("final-collision");
        fs::write(
            final_directory.path().join("authoritative.bin"),
            b"old authoritative bytes",
        )
        .unwrap();
        let final_parent = capture_existing_publication_parent_v1(final_directory.path()).unwrap();
        let final_error = publish_new_file_v1(
            &final_parent,
            OsStr::new("candidate.stage"),
            OsStr::new("authoritative.bin"),
            bytes,
            expectation,
        )
        .unwrap_err();
        assert_eq!(
            final_error.kind(),
            DurablePublicationErrorKindV1::FinalCollision
        );
        assert_eq!(
            fs::read(final_directory.path().join("authoritative.bin")).unwrap(),
            b"old authoritative bytes"
        );
        assert_eq!(
            fs::read(final_directory.path().join("candidate.stage")).unwrap(),
            bytes
        );
    }

    #[test]
    fn parent_capture_rejects_missing_non_directory_and_linked_parents() {
        let directory = TestDirectoryV1::new("parent-rejection");
        assert_eq!(
            capture_existing_publication_parent_v1(directory.path().join("missing"))
                .unwrap_err()
                .kind(),
            DurablePublicationErrorKindV1::InvalidParent
        );
        let file_path = directory.path().join("ordinary-file");
        fs::write(&file_path, b"not a directory").unwrap();
        assert_eq!(
            capture_existing_publication_parent_v1(&file_path)
                .unwrap_err()
                .kind(),
            DurablePublicationErrorKindV1::InvalidParent
        );

        let real_parent = directory.path().join("real-parent");
        fs::create_dir(&real_parent).unwrap();
        let linked_parent = directory.path().join("linked-parent");
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&real_parent, &linked_parent).unwrap();
            assert_eq!(
                capture_existing_publication_parent_v1(&linked_parent)
                    .unwrap_err()
                    .kind(),
                DurablePublicationErrorKindV1::InvalidParent
            );
        }
        #[cfg(windows)]
        {
            match std::os::windows::fs::symlink_dir(&real_parent, &linked_parent) {
                Ok(()) => assert_eq!(
                    capture_existing_publication_parent_v1(&linked_parent)
                        .unwrap_err()
                        .kind(),
                    DurablePublicationErrorKindV1::InvalidParent
                ),
                Err(error) if error.raw_os_error() == Some(1314) => {
                    // Developer Mode / SeCreateSymbolicLinkPrivilege is not
                    // guaranteed in CI. Production reparse detection is still
                    // exercised for every ordinary parent capture.
                }
                Err(error) => panic!("cannot create Windows test symlink: {error}"),
            }
        }
    }
}
