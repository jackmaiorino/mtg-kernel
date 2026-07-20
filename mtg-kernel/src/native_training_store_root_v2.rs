//! Path-backed native training Store V2 root authority.
//!
//! This module opens an existing Store root with Windows no-follow directory
//! semantics, retains the root, subdirectory, and lock handles, and requires a
//! local `DRIVE_FIXED` NTFS volume with stable `FILE_ID_INFO` identity on
//! every recapture. Mutators take a nonblocking exclusive `LockFileEx` range
//! lock and readers take a nonblocking shared range lock over offset zero,
//! length one. On non-Windows platforms every path-backed entry point returns
//! the stable unsupported-platform error before touching the filesystem. This
//! module owns no staging, publication, receipt, recovery, or record claim.

use crate::native_training_store_layout_v2::NativeTrainingStoreDirectoryV2;
#[cfg(windows)]
use crate::native_training_store_layout_v2::{
    NATIVE_TRAINING_STORE_LOCK_LEAF_V2, NATIVE_TRAINING_STORE_SUBDIRECTORY_ORDER_V2,
};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::path::Path;

/// Stable unsupported-platform code shared with the frozen CLI projection.
pub const NATIVE_TRAINING_STORE_UNSUPPORTED_PLATFORM_CODE_V2: &str =
    "native-training-store-v2-unsupported-platform";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeTrainingStoreRootV2ErrorKind {
    UnsupportedPlatform,
    RootInvalid,
    VolumeInvalid,
    SubdirectoryInvalid,
    LockInvalid,
    IdentityChanged,
    StoreBusy,
}

impl NativeTrainingStoreRootV2ErrorKind {
    pub const fn code(self) -> &'static str {
        match self {
            Self::UnsupportedPlatform => NATIVE_TRAINING_STORE_UNSUPPORTED_PLATFORM_CODE_V2,
            Self::RootInvalid => "native-training-store-root-invalid",
            Self::VolumeInvalid => "native-training-store-volume-invalid",
            Self::SubdirectoryInvalid => "native-training-store-subdirectory-invalid",
            Self::LockInvalid => "native-training-store-lock-invalid",
            Self::IdentityChanged => "native-training-store-identity-changed",
            Self::StoreBusy => "native-training-store-busy",
        }
    }
}

/// Redacted root error carrying only its classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeTrainingStoreRootV2Error {
    kind: NativeTrainingStoreRootV2ErrorKind,
}

impl NativeTrainingStoreRootV2Error {
    pub const fn kind(self) -> NativeTrainingStoreRootV2ErrorKind {
        self.kind
    }

    pub const fn code(self) -> &'static str {
        self.kind.code()
    }
}

impl Display for NativeTrainingStoreRootV2Error {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.code())
    }
}

impl Error for NativeTrainingStoreRootV2Error {}

type Result<T> = std::result::Result<T, NativeTrainingStoreRootV2Error>;

const fn root_error_v2(kind: NativeTrainingStoreRootV2ErrorKind) -> NativeTrainingStoreRootV2Error {
    NativeTrainingStoreRootV2Error { kind }
}

/// A validated Store root with retained no-follow handles.
#[derive(Debug)]
pub struct ValidatedNativeTrainingStoreRootV2 {
    #[cfg(windows)]
    inner: windows_store_root_v2::WindowsStoreRootV2,
    #[cfg(not(windows))]
    never: std::convert::Infallible,
}

/// Held nonblocking exclusive mutator lock over the Store lock leaf.
#[must_use = "dropping the exclusive store lock releases mutator exclusivity"]
#[derive(Debug)]
pub struct NativeTrainingStoreExclusiveLockV2<'root> {
    #[cfg(windows)]
    _held: windows_store_root_v2::HeldRangeLockV2<'root>,
    #[cfg(not(windows))]
    _never: std::convert::Infallible,
    #[cfg(not(windows))]
    _lifetime: std::marker::PhantomData<&'root ()>,
}

/// Held nonblocking shared reader lock over the Store lock leaf.
#[must_use = "dropping the shared store lock releases reader protection"]
#[derive(Debug)]
pub struct NativeTrainingStoreSharedLockV2<'root> {
    #[cfg(windows)]
    _held: windows_store_root_v2::HeldRangeLockV2<'root>,
    #[cfg(not(windows))]
    _never: std::convert::Infallible,
    #[cfg(not(windows))]
    _lifetime: std::marker::PhantomData<&'root ()>,
}

impl ValidatedNativeTrainingStoreRootV2 {
    /// Open and validate an existing Store root.
    ///
    /// On non-Windows platforms this returns the stable unsupported-platform
    /// error before any filesystem access.
    pub fn open_v2(root: impl AsRef<Path>) -> Result<Self> {
        #[cfg(windows)]
        {
            Ok(Self {
                inner: windows_store_root_v2::WindowsStoreRootV2::open_v2(root.as_ref())?,
            })
        }
        #[cfg(not(windows))]
        {
            let _ = root;
            Err(root_error_v2(
                NativeTrainingStoreRootV2ErrorKind::UnsupportedPlatform,
            ))
        }
    }

    /// Canonical validated root path.
    pub fn root_path(&self) -> &Path {
        #[cfg(windows)]
        {
            self.inner.root_path()
        }
        #[cfg(not(windows))]
        {
            match self.never {}
        }
    }

    /// Canonical path of one authoritative Store directory.
    pub fn directory_path_v2(&self, directory: NativeTrainingStoreDirectoryV2) -> &Path {
        #[cfg(windows)]
        {
            self.inner.directory_path_v2(directory)
        }
        #[cfg(not(windows))]
        {
            let _ = directory;
            match self.never {}
        }
    }

    /// Re-resolve every retained path and require identity stability.
    pub fn recapture_v2(&self) -> Result<()> {
        #[cfg(windows)]
        {
            self.inner.recapture_v2()
        }
        #[cfg(not(windows))]
        {
            match self.never {}
        }
    }

    /// Take the nonblocking exclusive mutator range lock.
    pub fn lock_exclusive_v2(&self) -> Result<NativeTrainingStoreExclusiveLockV2<'_>> {
        #[cfg(windows)]
        {
            Ok(NativeTrainingStoreExclusiveLockV2 {
                _held: self.inner.lock_range_v2(true)?,
            })
        }
        #[cfg(not(windows))]
        {
            match self.never {}
        }
    }

    /// Take the nonblocking shared reader range lock.
    pub fn lock_shared_v2(&self) -> Result<NativeTrainingStoreSharedLockV2<'_>> {
        #[cfg(windows)]
        {
            Ok(NativeTrainingStoreSharedLockV2 {
                _held: self.inner.lock_range_v2(false)?,
            })
        }
        #[cfg(not(windows))]
        {
            match self.never {}
        }
    }
}

#[cfg(windows)]
mod windows_store_root_v2 {
    use super::{
        root_error_v2, NativeTrainingStoreDirectoryV2, NativeTrainingStoreRootV2ErrorKind, Result,
        NATIVE_TRAINING_STORE_LOCK_LEAF_V2, NATIVE_TRAINING_STORE_SUBDIRECTORY_ORDER_V2,
    };
    use std::ffi::c_void;
    use std::mem::MaybeUninit;
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use std::path::{Path, PathBuf};

    type HandleV2 = *mut c_void;
    const INVALID_HANDLE_VALUE_V2: HandleV2 = -1_isize as HandleV2;
    const GENERIC_READ_V2: u32 = 0x8000_0000;
    const FILE_SHARE_READ_V2: u32 = 0x0000_0001;
    const FILE_SHARE_WRITE_V2: u32 = 0x0000_0002;
    const FILE_SHARE_DELETE_V2: u32 = 0x0000_0004;
    const OPEN_EXISTING_V2: u32 = 3;
    const FILE_FLAG_OPEN_REPARSE_POINT_V2: u32 = 0x0020_0000;
    const FILE_FLAG_BACKUP_SEMANTICS_V2: u32 = 0x0200_0000;
    const FILE_ATTRIBUTE_DIRECTORY_V2: u32 = 0x0000_0010;
    const FILE_ATTRIBUTE_REPARSE_POINT_V2: u32 = 0x0000_0400;
    const FILE_ATTRIBUTE_COMPRESSED_V2: u32 = 0x0000_0800;
    const DRIVE_FIXED_V2: u32 = 3;
    const FILE_BASIC_INFO_CLASS_V2: i32 = 0;
    const FILE_STANDARD_INFO_CLASS_V2: i32 = 1;
    const FILE_ID_INFO_CLASS_V2: i32 = 0x12;
    const LOCKFILE_FAIL_IMMEDIATELY_V2: u32 = 0x0000_0001;
    const LOCKFILE_EXCLUSIVE_LOCK_V2: u32 = 0x0000_0002;
    const ERROR_LOCK_VIOLATION_V2: i32 = 33;
    const FINAL_PATH_BUFFER_LEN_V2: usize = 0x8000;
    const NTFS_WIDE_NAME_V2: [u16; 5] = [b'N' as u16, b'T' as u16, b'F' as u16, b'S' as u16, 0];
    const VERBATIM_PREFIX_V2: [u16; 4] = [b'\\' as u16, b'\\' as u16, b'?' as u16, b'\\' as u16];

    #[repr(C)]
    struct FileBasicInfoV2 {
        creation_time: i64,
        last_access_time: i64,
        last_write_time: i64,
        change_time: i64,
        file_attributes: u32,
    }

    // FILE_BASIC_INFO is four LARGE_INTEGER fields plus one DWORD padded to
    // eight-byte alignment. Pinning the ABI size prevents a layout edit from
    // silently changing the FFI buffer.
    const _: [(); 40] = [(); std::mem::size_of::<FileBasicInfoV2>()];

    #[repr(C)]
    struct FileStandardInfoV2 {
        allocation_size: i64,
        end_of_file: i64,
        number_of_links: u32,
        delete_pending: u8,
        directory: u8,
    }

    // FILE_STANDARD_INFO is two LARGE_INTEGER fields, one DWORD, and two
    // BOOLEAN fields padded to eight-byte alignment.
    const _: [(); 24] = [(); std::mem::size_of::<FileStandardInfoV2>()];

    #[repr(C)]
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    struct FileIdInfoV2 {
        volume_serial_number: u64,
        file_id: [u8; 16],
    }

    // FILE_ID_INFO is exactly a ULONGLONG followed by FILE_ID_128.
    const _: [(); 24] = [(); std::mem::size_of::<FileIdInfoV2>()];

    #[repr(C)]
    struct OverlappedV2 {
        internal: usize,
        internal_high: usize,
        offset: u32,
        offset_high: u32,
        event: HandleV2,
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
            template_file: HandleV2,
        ) -> HandleV2;
        fn CloseHandle(object: HandleV2) -> i32;
        fn GetFileInformationByHandleEx(
            file: HandleV2,
            information_class: i32,
            information: *mut c_void,
            information_size: u32,
        ) -> i32;
        fn GetFinalPathNameByHandleW(
            file: HandleV2,
            file_path: *mut u16,
            file_path_len: u32,
            flags: u32,
        ) -> u32;
        fn GetVolumeInformationByHandleW(
            file: HandleV2,
            volume_name: *mut u16,
            volume_name_size: u32,
            volume_serial_number: *mut u32,
            maximum_component_length: *mut u32,
            file_system_flags: *mut u32,
            file_system_name: *mut u16,
            file_system_name_size: u32,
        ) -> i32;
        fn GetDriveTypeW(root_path_name: *const u16) -> u32;
        fn LockFileEx(
            file: HandleV2,
            flags: u32,
            reserved: u32,
            number_of_bytes_to_lock_low: u32,
            number_of_bytes_to_lock_high: u32,
            overlapped: *mut OverlappedV2,
        ) -> i32;
        fn UnlockFileEx(
            file: HandleV2,
            reserved: u32,
            number_of_bytes_to_unlock_low: u32,
            number_of_bytes_to_unlock_high: u32,
            overlapped: *mut OverlappedV2,
        ) -> i32;
    }

    /// One retained no-follow handle plus its captured identity.
    #[derive(Debug)]
    struct RetainedObjectV2 {
        handle: OwnedHandleV2,
        path: PathBuf,
        identity: FileIdInfoV2,
    }

    /// Owned raw handle closed exactly once on drop.
    #[derive(Debug)]
    struct OwnedHandleV2 {
        raw: HandleV2,
    }

    // SAFETY: the wrapped kernel handle is used only for handle-based
    // metadata, lock, and unlock calls, all of which are thread-safe.
    unsafe impl Send for OwnedHandleV2 {}
    // SAFETY: shared references only issue read-only kernel queries plus
    // LockFileEx/UnlockFileEx range calls, which the kernel serializes.
    unsafe impl Sync for OwnedHandleV2 {}

    impl Drop for OwnedHandleV2 {
        fn drop(&mut self) {
            // SAFETY: `raw` was returned live by CreateFileW and is closed
            // exactly once here.
            let _ = unsafe { CloseHandle(self.raw) };
        }
    }

    #[derive(Debug)]
    pub(super) struct WindowsStoreRootV2 {
        root: RetainedObjectV2,
        segments: RetainedObjectV2,
        checkpoints: RetainedObjectV2,
        heads: RetainedObjectV2,
        refs: RetainedObjectV2,
        lock: RetainedObjectV2,
    }

    /// A held LockFileEx range lock released exactly once on drop.
    #[derive(Debug)]
    pub(super) struct HeldRangeLockV2<'root> {
        handle: &'root OwnedHandleV2,
    }

    impl Drop for HeldRangeLockV2<'_> {
        fn drop(&mut self) {
            let mut overlapped = zero_overlapped_v2();
            // SAFETY: the handle is live for the borrow lifetime and the
            // overlapped range names exactly the locked offset-zero byte.
            let _ = unsafe { UnlockFileEx(self.handle.raw, 0, 1, 0, &mut overlapped) };
        }
    }

    const fn zero_overlapped_v2() -> OverlappedV2 {
        OverlappedV2 {
            internal: 0,
            internal_high: 0,
            offset: 0,
            offset_high: 0,
            event: std::ptr::null_mut(),
        }
    }

    fn wide_path_v2(path: &Path, kind: NativeTrainingStoreRootV2ErrorKind) -> Result<Vec<u16>> {
        let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
        if wide.contains(&0) {
            return Err(root_error_v2(kind));
        }
        wide.push(0);
        Ok(wide)
    }

    fn open_no_follow_v2(
        path: &Path,
        desired_access: u32,
        share_mode: u32,
        backup_semantics: bool,
        kind: NativeTrainingStoreRootV2ErrorKind,
    ) -> Result<OwnedHandleV2> {
        let wide = wide_path_v2(path, kind)?;
        let mut flags = FILE_FLAG_OPEN_REPARSE_POINT_V2;
        if backup_semantics {
            flags |= FILE_FLAG_BACKUP_SEMANTICS_V2;
        }
        // SAFETY: `wide` is NUL-terminated and all remaining pointers are null
        // or documented Windows constants for the CreateFileW ABI.
        let handle = unsafe {
            CreateFileW(
                wide.as_ptr(),
                desired_access,
                share_mode,
                std::ptr::null_mut(),
                OPEN_EXISTING_V2,
                flags,
                std::ptr::null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE_V2 {
            return Err(root_error_v2(kind));
        }
        Ok(OwnedHandleV2 { raw: handle })
    }

    fn basic_info_v2(
        handle: &OwnedHandleV2,
        kind: NativeTrainingStoreRootV2ErrorKind,
    ) -> Result<FileBasicInfoV2> {
        let mut information = MaybeUninit::<FileBasicInfoV2>::uninit();
        // SAFETY: the handle is live and the out pointer references exactly
        // one aligned FILE_BASIC_INFO buffer of the declared size.
        let success = unsafe {
            GetFileInformationByHandleEx(
                handle.raw,
                FILE_BASIC_INFO_CLASS_V2,
                information.as_mut_ptr().cast(),
                std::mem::size_of::<FileBasicInfoV2>() as u32,
            )
        };
        if success == 0 {
            return Err(root_error_v2(kind));
        }
        // SAFETY: the successful call initialized the entire structure.
        Ok(unsafe { information.assume_init() })
    }

    fn standard_info_v2(
        handle: &OwnedHandleV2,
        kind: NativeTrainingStoreRootV2ErrorKind,
    ) -> Result<FileStandardInfoV2> {
        let mut information = MaybeUninit::<FileStandardInfoV2>::uninit();
        // SAFETY: the handle is live and the out pointer references exactly
        // one aligned FILE_STANDARD_INFO buffer of the declared size.
        let success = unsafe {
            GetFileInformationByHandleEx(
                handle.raw,
                FILE_STANDARD_INFO_CLASS_V2,
                information.as_mut_ptr().cast(),
                std::mem::size_of::<FileStandardInfoV2>() as u32,
            )
        };
        if success == 0 {
            return Err(root_error_v2(kind));
        }
        // SAFETY: the successful call initialized the entire structure.
        Ok(unsafe { information.assume_init() })
    }

    fn identity_v2(
        handle: &OwnedHandleV2,
        kind: NativeTrainingStoreRootV2ErrorKind,
    ) -> Result<FileIdInfoV2> {
        let mut information = MaybeUninit::<FileIdInfoV2>::uninit();
        // SAFETY: the handle is live and the out pointer references exactly
        // one aligned FILE_ID_INFO buffer of the declared size.
        let success = unsafe {
            GetFileInformationByHandleEx(
                handle.raw,
                FILE_ID_INFO_CLASS_V2,
                information.as_mut_ptr().cast(),
                std::mem::size_of::<FileIdInfoV2>() as u32,
            )
        };
        if success == 0 {
            return Err(root_error_v2(kind));
        }
        // SAFETY: the successful call initialized the entire structure.
        Ok(unsafe { information.assume_init() })
    }

    fn final_path_v2(
        handle: &OwnedHandleV2,
        kind: NativeTrainingStoreRootV2ErrorKind,
    ) -> Result<PathBuf> {
        let mut buffer = vec![0_u16; FINAL_PATH_BUFFER_LEN_V2];
        // SAFETY: the handle is live and the buffer length is passed exactly.
        let length = unsafe {
            GetFinalPathNameByHandleW(handle.raw, buffer.as_mut_ptr(), buffer.len() as u32, 0)
        };
        if length == 0 || (length as usize) >= buffer.len() {
            return Err(root_error_v2(kind));
        }
        let wide = &buffer[..length as usize];
        if wide.contains(&0) {
            return Err(root_error_v2(kind));
        }
        Ok(PathBuf::from(std::ffi::OsString::from_wide(wide)))
    }

    fn require_local_fixed_ntfs_v2(handle: &OwnedHandleV2, final_path: &Path) -> Result<()> {
        let kind = NativeTrainingStoreRootV2ErrorKind::VolumeInvalid;
        let mut serial = 0_u32;
        let mut maximum_component = 0_u32;
        let mut flags = 0_u32;
        let mut name = [0_u16; 64];
        // SAFETY: the handle is live, out pointers reference live locals, and
        // the name buffer length is passed exactly.
        let success = unsafe {
            GetVolumeInformationByHandleW(
                handle.raw,
                std::ptr::null_mut(),
                0,
                &mut serial,
                &mut maximum_component,
                &mut flags,
                name.as_mut_ptr(),
                name.len() as u32,
            )
        };
        if success == 0 {
            return Err(root_error_v2(kind));
        }
        let terminated = name
            .iter()
            .position(|&unit| unit == 0)
            .map(|end| &name[..=end]);
        if terminated != Some(&NTFS_WIDE_NAME_V2[..]) {
            return Err(root_error_v2(kind));
        }
        let drive_root = drive_root_v2(final_path).ok_or(root_error_v2(kind))?;
        // SAFETY: the drive-root buffer is NUL-terminated.
        let drive_type = unsafe { GetDriveTypeW(drive_root.as_ptr()) };
        if drive_type != DRIVE_FIXED_V2 {
            return Err(root_error_v2(kind));
        }
        Ok(())
    }

    /// Extract `X:\` from a verbatim `\\?\X:\...` final path; reject UNC and
    /// every other resolution form.
    fn drive_root_v2(final_path: &Path) -> Option<Vec<u16>> {
        let wide: Vec<u16> = final_path.as_os_str().encode_wide().collect();
        if wide.len() < 7 || wide[..4] != VERBATIM_PREFIX_V2 {
            return None;
        }
        let letter = wide[4];
        let ascii = u8::try_from(letter).ok()?;
        if !ascii.is_ascii_alphabetic() || wide[5] != u16::from(b':') || wide[6] != u16::from(b'\\')
        {
            return None;
        }
        Some(vec![letter, u16::from(b':'), u16::from(b'\\'), 0])
    }

    fn require_directory_attributes_v2(
        handle: &OwnedHandleV2,
        kind: NativeTrainingStoreRootV2ErrorKind,
    ) -> Result<()> {
        let basic = basic_info_v2(handle, kind)?;
        let attributes = basic.file_attributes;
        if attributes & FILE_ATTRIBUTE_DIRECTORY_V2 == 0
            || attributes & FILE_ATTRIBUTE_REPARSE_POINT_V2 != 0
            || attributes & FILE_ATTRIBUTE_COMPRESSED_V2 != 0
        {
            return Err(root_error_v2(kind));
        }
        Ok(())
    }

    fn open_retained_directory_v2(
        path: &Path,
        expected_final_path: Option<&Path>,
        kind: NativeTrainingStoreRootV2ErrorKind,
    ) -> Result<RetainedObjectV2> {
        let handle = open_no_follow_v2(
            path,
            0,
            FILE_SHARE_READ_V2 | FILE_SHARE_WRITE_V2 | FILE_SHARE_DELETE_V2,
            true,
            kind,
        )?;
        require_directory_attributes_v2(&handle, kind)?;
        let resolved = final_path_v2(&handle, kind)?;
        if let Some(expected) = expected_final_path {
            if resolved != expected {
                return Err(root_error_v2(kind));
            }
        }
        let identity = identity_v2(&handle, kind)?;
        Ok(RetainedObjectV2 {
            handle,
            path: resolved,
            identity,
        })
    }

    fn open_retained_lock_v2(path: &Path, volume_serial_number: u64) -> Result<RetainedObjectV2> {
        let kind = NativeTrainingStoreRootV2ErrorKind::LockInvalid;
        let handle = open_no_follow_v2(path, GENERIC_READ_V2, FILE_SHARE_READ_V2, false, kind)?;
        let basic = basic_info_v2(&handle, kind)?;
        if basic.file_attributes & (FILE_ATTRIBUTE_DIRECTORY_V2 | FILE_ATTRIBUTE_REPARSE_POINT_V2)
            != 0
        {
            return Err(root_error_v2(kind));
        }
        let standard = standard_info_v2(&handle, kind)?;
        if standard.end_of_file != 0
            || standard.directory != 0
            || standard.delete_pending != 0
            || standard.number_of_links != 1
        {
            return Err(root_error_v2(kind));
        }
        let resolved = final_path_v2(&handle, kind)?;
        if resolved != path {
            return Err(root_error_v2(kind));
        }
        let identity = identity_v2(&handle, kind)?;
        if identity.volume_serial_number != volume_serial_number {
            return Err(root_error_v2(kind));
        }
        Ok(RetainedObjectV2 {
            handle,
            path: resolved,
            identity,
        })
    }

    impl WindowsStoreRootV2 {
        pub(super) fn open_v2(root: &Path) -> Result<Self> {
            let root_retained = open_retained_directory_v2(
                root,
                None,
                NativeTrainingStoreRootV2ErrorKind::RootInvalid,
            )?;
            require_local_fixed_ntfs_v2(&root_retained.handle, &root_retained.path)?;
            let volume_serial_number = root_retained.identity.volume_serial_number;
            let mut subdirectories = NATIVE_TRAINING_STORE_SUBDIRECTORY_ORDER_V2
                .iter()
                .map(|directory| {
                    let kind = NativeTrainingStoreRootV2ErrorKind::SubdirectoryInvalid;
                    let basename = directory.basename().ok_or(root_error_v2(kind))?;
                    let expected = root_retained.path.join(basename);
                    let retained = open_retained_directory_v2(&expected, Some(&expected), kind)?;
                    if retained.identity.volume_serial_number != volume_serial_number {
                        return Err(root_error_v2(
                            NativeTrainingStoreRootV2ErrorKind::VolumeInvalid,
                        ));
                    }
                    Ok(retained)
                })
                .collect::<Result<Vec<RetainedObjectV2>>>()?
                .into_iter();
            let segments = subdirectories.next().ok_or(root_error_v2(
                NativeTrainingStoreRootV2ErrorKind::SubdirectoryInvalid,
            ))?;
            let checkpoints = subdirectories.next().ok_or(root_error_v2(
                NativeTrainingStoreRootV2ErrorKind::SubdirectoryInvalid,
            ))?;
            let heads = subdirectories.next().ok_or(root_error_v2(
                NativeTrainingStoreRootV2ErrorKind::SubdirectoryInvalid,
            ))?;
            let refs = subdirectories.next().ok_or(root_error_v2(
                NativeTrainingStoreRootV2ErrorKind::SubdirectoryInvalid,
            ))?;
            let lock = open_retained_lock_v2(
                &root_retained.path.join(NATIVE_TRAINING_STORE_LOCK_LEAF_V2),
                volume_serial_number,
            )?;
            Ok(Self {
                root: root_retained,
                segments,
                checkpoints,
                heads,
                refs,
                lock,
            })
        }

        pub(super) fn root_path(&self) -> &Path {
            &self.root.path
        }

        pub(super) fn directory_path_v2(&self, directory: NativeTrainingStoreDirectoryV2) -> &Path {
            match directory {
                NativeTrainingStoreDirectoryV2::Root => &self.root.path,
                NativeTrainingStoreDirectoryV2::Segments => &self.segments.path,
                NativeTrainingStoreDirectoryV2::Checkpoints => &self.checkpoints.path,
                NativeTrainingStoreDirectoryV2::Heads => &self.heads.path,
                NativeTrainingStoreDirectoryV2::Refs => &self.refs.path,
            }
        }

        pub(super) fn recapture_v2(&self) -> Result<()> {
            let kind = NativeTrainingStoreRootV2ErrorKind::IdentityChanged;
            for (retained, backup_semantics, access, share) in [
                (
                    &self.root,
                    true,
                    0,
                    FILE_SHARE_READ_V2 | FILE_SHARE_WRITE_V2 | FILE_SHARE_DELETE_V2,
                ),
                (
                    &self.segments,
                    true,
                    0,
                    FILE_SHARE_READ_V2 | FILE_SHARE_WRITE_V2 | FILE_SHARE_DELETE_V2,
                ),
                (
                    &self.checkpoints,
                    true,
                    0,
                    FILE_SHARE_READ_V2 | FILE_SHARE_WRITE_V2 | FILE_SHARE_DELETE_V2,
                ),
                (
                    &self.heads,
                    true,
                    0,
                    FILE_SHARE_READ_V2 | FILE_SHARE_WRITE_V2 | FILE_SHARE_DELETE_V2,
                ),
                (
                    &self.refs,
                    true,
                    0,
                    FILE_SHARE_READ_V2 | FILE_SHARE_WRITE_V2 | FILE_SHARE_DELETE_V2,
                ),
                (&self.lock, false, 0, FILE_SHARE_READ_V2),
            ] {
                let reopened =
                    open_no_follow_v2(&retained.path, access, share, backup_semantics, kind)?;
                let identity = identity_v2(&reopened, kind)?;
                if identity != retained.identity {
                    return Err(root_error_v2(kind));
                }
                let basic = basic_info_v2(&reopened, kind)?;
                if basic.file_attributes & FILE_ATTRIBUTE_REPARSE_POINT_V2 != 0 {
                    return Err(root_error_v2(kind));
                }
            }
            require_local_fixed_ntfs_v2(&self.root.handle, &self.root.path)?;
            Ok(())
        }

        pub(super) fn lock_range_v2(&self, exclusive: bool) -> Result<HeldRangeLockV2<'_>> {
            let mut flags = LOCKFILE_FAIL_IMMEDIATELY_V2;
            if exclusive {
                flags |= LOCKFILE_EXCLUSIVE_LOCK_V2;
            }
            let mut overlapped = zero_overlapped_v2();
            // SAFETY: the retained lock handle is live and the overlapped
            // range names exactly the offset-zero byte required by the
            // frozen lock contract.
            let success =
                unsafe { LockFileEx(self.lock.handle.raw, flags, 0, 1, 0, &mut overlapped) };
            if success == 0 {
                let raw = std::io::Error::last_os_error().raw_os_error();
                return Err(root_error_v2(if raw == Some(ERROR_LOCK_VIOLATION_V2) {
                    NativeTrainingStoreRootV2ErrorKind::StoreBusy
                } else {
                    NativeTrainingStoreRootV2ErrorKind::LockInvalid
                }));
            }
            Ok(HeldRangeLockV2 {
                handle: &self.lock.handle,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    #[cfg(windows)]
    mod windows_tests {
        use super::super::{
            NativeTrainingStoreRootV2ErrorKind, ValidatedNativeTrainingStoreRootV2,
            NATIVE_TRAINING_STORE_LOCK_LEAF_V2, NATIVE_TRAINING_STORE_SUBDIRECTORY_ORDER_V2,
        };
        use std::fs;
        use std::path::{Path, PathBuf};
        use std::sync::atomic::{AtomicU64, Ordering};

        struct TestStoreV2 {
            root: PathBuf,
        }

        impl TestStoreV2 {
            fn new(label: &str) -> Self {
                static ORDINAL: AtomicU64 = AtomicU64::new(0);
                let ordinal = ORDINAL.fetch_add(1, Ordering::Relaxed);
                let root = std::env::temp_dir().join(format!(
                    "mtg-kernel-store-root-v2-{}-{label}-{ordinal}",
                    std::process::id()
                ));
                fs::create_dir(&root).expect("create test root");
                Self { root }
            }

            fn with_skeleton(label: &str) -> Self {
                let store = Self::new(label);
                for directory in NATIVE_TRAINING_STORE_SUBDIRECTORY_ORDER_V2 {
                    fs::create_dir(store.root.join(directory.basename().unwrap()))
                        .expect("create subdirectory");
                }
                fs::write(store.root.join(NATIVE_TRAINING_STORE_LOCK_LEAF_V2), [])
                    .expect("create lock leaf");
                store
            }

            fn path(&self) -> &Path {
                &self.root
            }
        }

        impl Drop for TestStoreV2 {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.root);
            }
        }

        #[test]
        fn a_complete_skeleton_opens_and_reports_canonical_directories() {
            let store = TestStoreV2::with_skeleton("open");
            let root = ValidatedNativeTrainingStoreRootV2::open_v2(store.path()).unwrap();
            assert!(root.root_path().ends_with(
                store
                    .path()
                    .file_name()
                    .expect("test root has a final component")
            ));
            for directory in NATIVE_TRAINING_STORE_SUBDIRECTORY_ORDER_V2 {
                let path = root.directory_path_v2(directory);
                assert!(path.starts_with(root.root_path()));
                assert!(path.ends_with(directory.basename().unwrap()));
            }
            root.recapture_v2().unwrap();
        }

        #[test]
        fn missing_or_nonregular_members_fail_closed() {
            let missing_subdirectory = TestStoreV2::new("missing-subdirectory");
            fs::write(
                missing_subdirectory
                    .path()
                    .join(NATIVE_TRAINING_STORE_LOCK_LEAF_V2),
                [],
            )
            .unwrap();
            assert_eq!(
                ValidatedNativeTrainingStoreRootV2::open_v2(missing_subdirectory.path())
                    .unwrap_err()
                    .kind(),
                NativeTrainingStoreRootV2ErrorKind::SubdirectoryInvalid
            );

            let missing_lock = TestStoreV2::new("missing-lock");
            for directory in NATIVE_TRAINING_STORE_SUBDIRECTORY_ORDER_V2 {
                fs::create_dir(missing_lock.path().join(directory.basename().unwrap())).unwrap();
            }
            assert_eq!(
                ValidatedNativeTrainingStoreRootV2::open_v2(missing_lock.path())
                    .unwrap_err()
                    .kind(),
                NativeTrainingStoreRootV2ErrorKind::LockInvalid
            );

            let nonempty_lock = TestStoreV2::with_skeleton("nonempty-lock");
            fs::write(
                nonempty_lock
                    .path()
                    .join(NATIVE_TRAINING_STORE_LOCK_LEAF_V2),
                b"x",
            )
            .unwrap();
            assert_eq!(
                ValidatedNativeTrainingStoreRootV2::open_v2(nonempty_lock.path())
                    .unwrap_err()
                    .kind(),
                NativeTrainingStoreRootV2ErrorKind::LockInvalid
            );

            let directory_lock = TestStoreV2::new("directory-lock");
            for directory in NATIVE_TRAINING_STORE_SUBDIRECTORY_ORDER_V2 {
                fs::create_dir(directory_lock.path().join(directory.basename().unwrap())).unwrap();
            }
            fs::create_dir(
                directory_lock
                    .path()
                    .join(NATIVE_TRAINING_STORE_LOCK_LEAF_V2),
            )
            .unwrap();
            assert_eq!(
                ValidatedNativeTrainingStoreRootV2::open_v2(directory_lock.path())
                    .unwrap_err()
                    .kind(),
                NativeTrainingStoreRootV2ErrorKind::LockInvalid
            );

            let file_subdirectory = TestStoreV2::new("file-subdirectory");
            fs::write(file_subdirectory.path().join("segments"), []).unwrap();
            for directory in &NATIVE_TRAINING_STORE_SUBDIRECTORY_ORDER_V2[1..] {
                fs::create_dir(file_subdirectory.path().join(directory.basename().unwrap()))
                    .unwrap();
            }
            fs::write(
                file_subdirectory
                    .path()
                    .join(NATIVE_TRAINING_STORE_LOCK_LEAF_V2),
                [],
            )
            .unwrap();
            assert_eq!(
                ValidatedNativeTrainingStoreRootV2::open_v2(file_subdirectory.path())
                    .unwrap_err()
                    .kind(),
                NativeTrainingStoreRootV2ErrorKind::SubdirectoryInvalid
            );
        }

        #[test]
        fn a_junction_root_or_subdirectory_is_rejected_as_a_reparse_point() {
            let target = TestStoreV2::with_skeleton("junction-target");
            let holder = TestStoreV2::new("junction-holder");
            let junction = holder.path().join("junction-root");
            let status = std::process::Command::new("cmd")
                .args([
                    "/c",
                    "mklink",
                    "/J",
                    junction.to_str().expect("junction path is unicode"),
                    target.path().to_str().expect("target path is unicode"),
                ])
                .output()
                .expect("run mklink");
            assert!(
                status.status.success(),
                "mklink /J must succeed without privileges"
            );
            assert_eq!(
                ValidatedNativeTrainingStoreRootV2::open_v2(&junction)
                    .unwrap_err()
                    .kind(),
                NativeTrainingStoreRootV2ErrorKind::RootInvalid
            );

            let junction_member = TestStoreV2::new("junction-member");
            let segments_target = TestStoreV2::new("junction-member-target");
            let segments_link = junction_member.path().join("segments");
            let link_status = std::process::Command::new("cmd")
                .args([
                    "/c",
                    "mklink",
                    "/J",
                    segments_link.to_str().unwrap(),
                    segments_target.path().to_str().unwrap(),
                ])
                .output()
                .expect("run mklink");
            assert!(link_status.status.success());
            for directory in &NATIVE_TRAINING_STORE_SUBDIRECTORY_ORDER_V2[1..] {
                fs::create_dir(junction_member.path().join(directory.basename().unwrap())).unwrap();
            }
            fs::write(
                junction_member
                    .path()
                    .join(NATIVE_TRAINING_STORE_LOCK_LEAF_V2),
                [],
            )
            .unwrap();
            assert_eq!(
                ValidatedNativeTrainingStoreRootV2::open_v2(junction_member.path())
                    .unwrap_err()
                    .kind(),
                NativeTrainingStoreRootV2ErrorKind::SubdirectoryInvalid
            );
        }

        #[test]
        fn exclusive_and_shared_range_locks_conflict_exactly() {
            let store = TestStoreV2::with_skeleton("locks");
            let first = ValidatedNativeTrainingStoreRootV2::open_v2(store.path()).unwrap();
            let second = ValidatedNativeTrainingStoreRootV2::open_v2(store.path()).unwrap();

            let exclusive = first.lock_exclusive_v2().unwrap();
            assert_eq!(
                second.lock_exclusive_v2().unwrap_err().kind(),
                NativeTrainingStoreRootV2ErrorKind::StoreBusy
            );
            assert_eq!(
                second.lock_shared_v2().unwrap_err().kind(),
                NativeTrainingStoreRootV2ErrorKind::StoreBusy
            );
            drop(exclusive);

            let shared_first = first.lock_shared_v2().unwrap();
            let shared_second = second.lock_shared_v2().unwrap();
            assert_eq!(
                first.lock_exclusive_v2().unwrap_err().kind(),
                NativeTrainingStoreRootV2ErrorKind::StoreBusy
            );
            drop(shared_first);
            assert_eq!(
                second.lock_exclusive_v2().unwrap_err().kind(),
                NativeTrainingStoreRootV2ErrorKind::StoreBusy
            );
            drop(shared_second);

            let regained = second.lock_exclusive_v2().unwrap();
            drop(regained);
        }

        #[test]
        fn recapture_detects_replaced_members() {
            let store = TestStoreV2::with_skeleton("recapture");
            let root = ValidatedNativeTrainingStoreRootV2::open_v2(store.path()).unwrap();
            root.recapture_v2().unwrap();

            let heads = store.path().join("heads");
            let heads_moved = store.path().join("heads-moved");
            fs::rename(&heads, &heads_moved).unwrap();
            fs::create_dir(&heads).unwrap();
            assert_eq!(
                root.recapture_v2().unwrap_err().kind(),
                NativeTrainingStoreRootV2ErrorKind::IdentityChanged
            );
            fs::remove_dir(&heads).unwrap();
            fs::rename(&heads_moved, &heads).unwrap();
            root.recapture_v2().unwrap();
        }
    }

    #[cfg(not(windows))]
    mod non_windows_tests {
        use super::super::{
            NativeTrainingStoreRootV2ErrorKind, ValidatedNativeTrainingStoreRootV2,
            NATIVE_TRAINING_STORE_UNSUPPORTED_PLATFORM_CODE_V2,
        };

        #[test]
        fn every_path_backed_entry_point_reports_the_stable_unsupported_platform_error() {
            let probe = std::env::temp_dir().join("mtg-kernel-store-root-v2-unsupported-probe");
            let error = ValidatedNativeTrainingStoreRootV2::open_v2(&probe).unwrap_err();
            assert_eq!(
                error.kind(),
                NativeTrainingStoreRootV2ErrorKind::UnsupportedPlatform
            );
            assert_eq!(
                error.code(),
                NATIVE_TRAINING_STORE_UNSUPPORTED_PLATFORM_CODE_V2
            );
            assert!(
                !probe.exists(),
                "the unsupported-platform gate must precede any filesystem mutation"
            );
        }
    }
}
