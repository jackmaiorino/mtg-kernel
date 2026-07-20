//! Production-only provenance capture for Native Training Store V2.
//!
//! This module deliberately owns no Store root, artifact, publication,
//! persistence receipt, or executor mutation. It captures path-free record
//! fragments, retains the current executable handle, and proves pre/postflight
//! equality against an already validated run.

use crate::card_def::KERNEL_CARDDB_HASH;
use crate::native_policy_train_step_v1::NATIVE_POLICY_TRAIN_STEP_NUMERICAL_BACKEND_IDENTITY_V1;
use crate::native_training_store_run_v2::{
    TrainRunEnvironmentV2, TrainRunPackageV2, TrainRunRuntimeV2, TrainRunSourceV2,
    TrainRunToolchainV2, ValidatedTrainRunV2,
};
use crate::policy_surface_v5::POLICY_SURFACE_VERSION;
use crate::rl_session::{
    CANONICAL_RALLY_DECK_ID, RL_SESSION_PROTOCOL_NAME, RL_SESSION_PROTOCOL_VERSION,
    RL_SESSION_SCHEMA_VERSION,
};
use crate::runtime_decks::{
    runtime_deck_by_id, RUNTIME_DECK_CATALOG_FILE_SHA256, RUNTIME_DECK_CATALOG_SCHEMA,
    RUNTIME_DECK_PROTOCOL,
};
use crate::strict_source_tree_attestation_v1::{
    capture_strict_source_tree_v1, StrictSourceTreeCaptureV1,
    STRICT_SOURCE_TREE_RECIPE_BYTE_COUNT_V1,
};
use crate::surface_v2::H2_PREDICATE_VERSION;
use crate::KERNEL_VERSION;
use sha2::{Digest, Sha256};
use std::error::Error;
use std::ffi::c_void;
use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::path::{Component, Path, PathBuf};

mod build_capture {
    include!(concat!(
        env!("OUT_DIR"),
        "/native_store_build_capture_v1.rs"
    ));
}

const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
const FILE_SHARE_READ: u32 = 0x0000_0001;
const FILE_READ_DATA: u32 = 0x0000_0001;
const FILE_READ_ATTRIBUTES: u32 = 0x0000_0080;
const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x0000_0010;
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
const DRIVE_FIXED: u32 = 3;
const FILE_STANDARD_INFO_CLASS: i32 = 1;
const FILE_ATTRIBUTE_TAG_INFO_CLASS: i32 = 9;
const FILE_ID_INFO_CLASS: i32 = 18;
const IMAGE_FILE_MACHINE_UNKNOWN: u16 = 0x0000;
const IMAGE_FILE_MACHINE_AMD64: u16 = 0x8664;
const IMAGE_FILE_MACHINE_ARM64: u16 = 0xaa64;
const IMAGE_FILE_EXECUTABLE_IMAGE: u16 = 0x0002;
const IMAGE_FILE_DLL: u16 = 0x2000;
const VER_PLATFORM_WIN32_NT: u32 = 2;
const VER_NT_WORKSTATION: u8 = 1;
const VER_NT_DOMAIN_CONTROLLER: u8 = 2;
const VER_NT_SERVER: u8 = 3;
const MODULE_PATH_SENTINEL: u16 = 0xffff;
const MODULE_PATH_CAPACITIES: [usize; 8] = [260, 520, 1_040, 2_080, 4_160, 8_320, 16_640, 32_768];
const PE32_PLUS_MIN_OPTIONAL_HEADER_BYTES: usize = 112;
const PE_SECTION_HEADER_BYTES: u64 = 40;
const PE_MAX_SECTION_COUNT: u16 = 96;
const PE_MAX_DATA_DIRECTORY_COUNT: u32 = 16;
const MAX_U63: u64 = i64::MAX as u64;

type Handle = *mut c_void;

#[repr(C)]
struct FileIdInfo {
    volume_serial_number: u64,
    file_id: [u8; 16],
}

#[repr(C)]
struct FileStandardInfo {
    allocation_size: i64,
    end_of_file: i64,
    number_of_links: u32,
    delete_pending: u8,
    directory: u8,
}

#[repr(C)]
struct FileAttributeTagInfo {
    file_attributes: u32,
    reparse_tag: u32,
}

#[repr(C)]
struct RtlOsVersionInfoExW {
    os_version_info_size: u32,
    major_version: u32,
    minor_version: u32,
    build_number: u32,
    platform_id: u32,
    csd_version: [u16; 128],
    service_pack_major: u16,
    service_pack_minor: u16,
    suite_mask: u16,
    product_type: u8,
    reserved: u8,
}

#[repr(C)]
struct SystemInfo {
    processor_architecture: u16,
    reserved: u16,
    page_size: u32,
    minimum_application_address: *mut c_void,
    maximum_application_address: *mut c_void,
    active_processor_mask: usize,
    number_of_processors: u32,
    processor_type: u32,
    allocation_granularity: u32,
    processor_level: u16,
    processor_revision: u16,
}

const _: [(); 24] = [(); std::mem::size_of::<FileIdInfo>()];
const _: [(); 24] = [(); std::mem::size_of::<FileStandardInfo>()];
const _: [(); 8] = [(); std::mem::size_of::<FileAttributeTagInfo>()];
const _: [(); 284] = [(); std::mem::size_of::<RtlOsVersionInfoExW>()];
const _: [(); 48] = [(); std::mem::size_of::<SystemInfo>()];

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetModuleHandleW(module_name: *const u16) -> Handle;
    fn GetModuleFileNameW(module: Handle, filename: *mut u16, size: u32) -> u32;
    fn GetDriveTypeW(root_path_name: *const u16) -> u32;
    fn GetFileInformationByHandleEx(
        file: Handle,
        information_class: i32,
        information: *mut c_void,
        buffer_size: u32,
    ) -> i32;
    fn GetNativeSystemInfo(system_info: *mut SystemInfo);
    fn GetCurrentProcess() -> Handle;
    fn IsWow64Process2(process: Handle, process_machine: *mut u16, native_machine: *mut u16)
        -> i32;
}

#[link(name = "ntdll")]
unsafe extern "system" {
    fn RtlGetVersion(version: *mut RtlOsVersionInfoExW) -> i32;
}

/// Stable fail-closed error classes for production capture.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeStoreProductionCaptureErrorKindV2 {
    BuildCapture,
    SourceCapture,
    SourceMismatch,
    Path,
    FileOpen,
    FileMetadata,
    FileChanged,
    FileRead,
    ExecutableFormat,
    ExecutableMismatch,
    RuntimeCapture,
    RuntimeMismatch,
    CatalogMismatch,
    RunMismatch,
}

/// Privacy-safe error containing no path, environment value, or nested OS text.
#[derive(Debug)]
pub struct NativeStoreProductionCaptureErrorV2 {
    kind: NativeStoreProductionCaptureErrorKindV2,
    code: &'static str,
}

impl NativeStoreProductionCaptureErrorV2 {
    const fn new(kind: NativeStoreProductionCaptureErrorKindV2, code: &'static str) -> Self {
        Self { kind, code }
    }

    pub const fn kind(&self) -> NativeStoreProductionCaptureErrorKindV2 {
        self.kind
    }

    pub const fn code(&self) -> &'static str {
        self.code
    }
}

impl fmt::Display for NativeStoreProductionCaptureErrorV2 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "native Store V2 production capture failed: {}",
            self.code
        )
    }
}

impl Error for NativeStoreProductionCaptureErrorV2 {}

type Result<T> = std::result::Result<T, NativeStoreProductionCaptureErrorV2>;

fn capture_error(
    kind: NativeStoreProductionCaptureErrorKindV2,
    code: &'static str,
) -> NativeStoreProductionCaptureErrorV2 {
    NativeStoreProductionCaptureErrorV2::new(kind, code)
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct FileIdentityV2 {
    volume_serial_number: u64,
    file_id: [u8; 16],
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ExecutableSnapshotV2 {
    identity: FileIdentityV2,
    byte_len: u64,
    sha256: String,
    pe_machine: u16,
    pe_size_of_image_bytes: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CatalogSnapshotV2 {
    identity: FileIdentityV2,
    byte_len: u64,
    sha256: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RuntimeSnapshotV2 {
    os_major: u64,
    os_minor: u64,
    os_build: u64,
    service_pack_major: u64,
    service_pack_minor: u64,
    product_type: u64,
    suite_mask_u16_hex: String,
    native_architecture: String,
    process_architecture: String,
}

/// Borrowed path-free record fragments captured by the production guard.
#[derive(Clone, Copy)]
pub struct NativeStoreProductionCapturedValuesV2<'a> {
    pub package: &'a TrainRunPackageV2,
    pub toolchain: &'a TrainRunToolchainV2,
    pub source: &'a TrainRunSourceV2,
    pub runtime: &'a TrainRunRuntimeV2,
    pub environment: &'a TrainRunEnvironmentV2,
}

/// Non-persisting production capture guard.
///
/// The guard is intentionally neither `Clone`, `Debug`, nor serializable. It
/// retains path-bearing state and the primary executable handle privately.
#[must_use = "a production capture guard must complete postflight validation"]
pub struct NativeStoreProductionCaptureGuardV2 {
    source_root: PathBuf,
    _source_directory_chain: Vec<File>,
    executable_path: PathBuf,
    _executable_parent_chain: Vec<File>,
    executable_file: File,
    source_capture: StrictSourceTreeCaptureV1,
    executable_snapshot: ExecutableSnapshotV2,
    catalog_snapshot: CatalogSnapshotV2,
    runtime_snapshot: RuntimeSnapshotV2,
    package: TrainRunPackageV2,
    toolchain: TrainRunToolchainV2,
    source: TrainRunSourceV2,
    runtime: TrainRunRuntimeV2,
    environment: TrainRunEnvironmentV2,
}

impl NativeStoreProductionCaptureGuardV2 {
    /// Captures every frozen production value before returning a guard.
    pub fn begin(source_root: impl AsRef<Path>) -> Result<Self> {
        validate_build_capture_constants()?;
        let source_root = source_root.as_ref().to_path_buf();
        let source_directory_chain = validate_and_open_directory_chain(&source_root)?;
        let source_capture = capture_and_require_build_source(&source_root)?;
        let catalog_snapshot = capture_runtime_catalog(&source_root)?;
        let runtime_snapshot = capture_runtime_snapshot()?;
        let (executable_path, executable_parent_chain, mut executable_file, executable_snapshot) =
            capture_current_executable()?;
        let secondary_snapshot = capture_secondary_executable(&executable_path)?;
        if secondary_snapshot != executable_snapshot {
            return Err(capture_error(
                NativeStoreProductionCaptureErrorKindV2::ExecutableMismatch,
                "executable_secondary_reopen_mismatch",
            ));
        }

        let package = build_package_record();
        let toolchain = build_toolchain_record();
        let source = build_source_record(&source_capture, &executable_snapshot);
        let runtime = build_runtime_record(&runtime_snapshot, &toolchain);
        let environment = build_environment_record()?;

        // Recheck the retained handle after all record construction so begin
        // never returns a guard over bytes that drifted during capture.
        let retained_recheck = capture_executable_handle(&mut executable_file)?;
        if retained_recheck != executable_snapshot {
            return Err(capture_error(
                NativeStoreProductionCaptureErrorKindV2::ExecutableMismatch,
                "executable_primary_changed_during_begin",
            ));
        }

        Ok(Self {
            source_root,
            _source_directory_chain: source_directory_chain,
            executable_path,
            _executable_parent_chain: executable_parent_chain,
            executable_file,
            source_capture,
            executable_snapshot,
            catalog_snapshot,
            runtime_snapshot,
            package,
            toolchain,
            source,
            runtime,
            environment,
        })
    }

    pub fn captured_values(&self) -> NativeStoreProductionCapturedValuesV2<'_> {
        NativeStoreProductionCapturedValuesV2 {
            package: &self.package,
            toolchain: &self.toolchain,
            source: &self.source,
            runtime: &self.runtime,
            environment: &self.environment,
        }
    }

    /// Requires an already validated run to contain this exact captured tuple.
    pub fn require_matches_run_v2(&self, run: &ValidatedTrainRunV2) -> Result<()> {
        let record = run.record();
        if record.package != self.package
            || record.toolchain != self.toolchain
            || record.source != self.source
            || record.runtime != self.runtime
            || record.environment != self.environment
        {
            return Err(capture_error(
                NativeStoreProductionCaptureErrorKindV2::RunMismatch,
                "validated_run_capture_tuple_mismatch",
            ));
        }
        Ok(())
    }

    /// Consumes the guard after exact source/catalog/runtime/executable
    /// postflight recapture and validated-run equality.
    pub fn finish_against_run_v2(mut self, run: &ValidatedTrainRunV2) -> Result<()> {
        self.require_matches_run_v2(run)?;

        let _source_after_chain = validate_and_open_directory_chain(&self.source_root)?;
        let source_after = capture_and_require_build_source(&self.source_root)?;
        if source_after != self.source_capture {
            return Err(capture_error(
                NativeStoreProductionCaptureErrorKindV2::SourceMismatch,
                "source_postflight_tuple_mismatch",
            ));
        }

        let catalog_after = capture_runtime_catalog(&self.source_root)?;
        if catalog_after != self.catalog_snapshot {
            return Err(capture_error(
                NativeStoreProductionCaptureErrorKindV2::CatalogMismatch,
                "runtime_catalog_postflight_mismatch",
            ));
        }

        let runtime_after = capture_runtime_snapshot()?;
        if runtime_after != self.runtime_snapshot {
            return Err(capture_error(
                NativeStoreProductionCaptureErrorKindV2::RuntimeMismatch,
                "runtime_postflight_tuple_mismatch",
            ));
        }

        // These are deliberately the final fallible operations. The retained
        // executable handle is re-read first and the same path is then reopened
        // no-follow once more.
        let primary_after = capture_executable_handle(&mut self.executable_file)?;
        if primary_after != self.executable_snapshot {
            return Err(capture_error(
                NativeStoreProductionCaptureErrorKindV2::ExecutableMismatch,
                "executable_primary_postflight_mismatch",
            ));
        }
        let secondary_after = capture_secondary_executable(&self.executable_path)?;
        if secondary_after != self.executable_snapshot {
            return Err(capture_error(
                NativeStoreProductionCaptureErrorKindV2::ExecutableMismatch,
                "executable_secondary_postflight_mismatch",
            ));
        }
        Ok(())
    }
}

fn validate_build_capture_constants() -> Result<()> {
    if build_capture::NATIVE_STORE_BUILD_PACKAGE_NAME_V1 != "mtg-kernel"
        || build_capture::NATIVE_STORE_BUILD_PACKAGE_VERSION_V1 != env!("CARGO_PKG_VERSION")
        || build_capture::NATIVE_STORE_BUILD_PROFILE_V1 != "release"
        || !build_capture::NATIVE_STORE_BUILD_SOURCE_WORKTREE_CLEAN_V1
        || build_capture::NATIVE_STORE_BUILD_SOURCE_GIT_STATUS_SHA256_V1 != EMPTY_SHA256
        || build_capture::NATIVE_STORE_BUILD_SOURCE_TREE_RECIPE_BYTE_COUNT_V1
            != STRICT_SOURCE_TREE_RECIPE_BYTE_COUNT_V1
        || build_capture::NATIVE_STORE_BUILD_ENABLED_FEATURES_V1.is_empty()
        || !build_capture::NATIVE_STORE_BUILD_ENABLED_FEATURES_V1
            .contains(&"native-training-store-v2-production")
    {
        return Err(capture_error(
            NativeStoreProductionCaptureErrorKindV2::BuildCapture,
            "embedded_build_capture_invalid",
        ));
    }
    Ok(())
}

fn build_package_record() -> TrainRunPackageV2 {
    TrainRunPackageV2 {
        name: build_capture::NATIVE_STORE_BUILD_PACKAGE_NAME_V1.to_string(),
        version: build_capture::NATIVE_STORE_BUILD_PACKAGE_VERSION_V1.to_string(),
        workspace_manifest_sha256: build_capture::NATIVE_STORE_BUILD_WORKSPACE_MANIFEST_SHA256_V1
            .to_string(),
        crate_manifest_sha256: build_capture::NATIVE_STORE_BUILD_CRATE_MANIFEST_SHA256_V1
            .to_string(),
        cargo_lock_sha256: build_capture::NATIVE_STORE_BUILD_CARGO_LOCK_SHA256_V1.to_string(),
        enabled_features: build_capture::NATIVE_STORE_BUILD_ENABLED_FEATURES_V1
            .iter()
            .map(|feature| (*feature).to_string())
            .collect(),
    }
}

fn build_toolchain_record() -> TrainRunToolchainV2 {
    TrainRunToolchainV2 {
        capture_identity: "rustc-verbose-version-build-embed-v1".to_string(),
        rustc_release: build_capture::NATIVE_STORE_BUILD_RUSTC_RELEASE_V1.to_string(),
        rustc_commit_hash: build_capture::NATIVE_STORE_BUILD_RUSTC_COMMIT_HASH_V1.to_string(),
        rustc_commit_date: build_capture::NATIVE_STORE_BUILD_RUSTC_COMMIT_DATE_V1.to_string(),
        host_triple: build_capture::NATIVE_STORE_BUILD_HOST_TRIPLE_V1.to_string(),
        target_triple: build_capture::NATIVE_STORE_BUILD_TARGET_TRIPLE_V1.to_string(),
        llvm_version: build_capture::NATIVE_STORE_BUILD_LLVM_VERSION_V1.to_string(),
        rustc_verbose_version_sha256:
            build_capture::NATIVE_STORE_BUILD_RUSTC_VERBOSE_VERSION_SHA256_V1.to_string(),
        rustc_verbose_version_line_ending:
            build_capture::NATIVE_STORE_BUILD_RUSTC_VERBOSE_VERSION_LINE_ENDING_V1.to_string(),
        build_profile: build_capture::NATIVE_STORE_BUILD_PROFILE_V1.to_string(),
    }
}

fn build_source_record(
    source: &StrictSourceTreeCaptureV1,
    executable: &ExecutableSnapshotV2,
) -> TrainRunSourceV2 {
    TrainRunSourceV2 {
        git_commit: source.git_commit().to_string(),
        source_tree_recipe_identity: source.source_tree_recipe_identity().to_string(),
        source_tree_recipe_sha256: source.source_tree_recipe_sha256().to_string(),
        source_tree_recipe_byte_count: STRICT_SOURCE_TREE_RECIPE_BYTE_COUNT_V1,
        source_tree_sha256: source.source_tree_sha256().to_string(),
        worktree_clean: source.worktree_clean(),
        git_status_sha256: source.git_status_sha256().to_string(),
        executable_capture_identity: "windows-current-module-path-file-v2".to_string(),
        binary_name: "mtg-kernel-native.exe".to_string(),
        binary_sha256: executable.sha256.clone(),
        binary_byte_len: executable.byte_len,
        binary_volume_serial_u64_hex: format!("{:016x}", executable.identity.volume_serial_number),
        binary_file_id_128_hex: file_id_hex(&executable.identity.file_id),
        binary_pe_size_of_image_bytes: executable.pe_size_of_image_bytes,
        capture_scope: "module-path-file-not-loaded-section-provenance/v1".to_string(),
    }
}

fn build_runtime_record(
    runtime: &RuntimeSnapshotV2,
    toolchain: &TrainRunToolchainV2,
) -> TrainRunRuntimeV2 {
    TrainRunRuntimeV2 {
        tuple_identity: "mtg-kernel-native-windows-cpu-runtime-tuple-v1".to_string(),
        os_capture_identity: "windows-rtlgetversion-native-system-info-v1".to_string(),
        os_system: "windows".to_string(),
        os_major: runtime.os_major,
        os_minor: runtime.os_minor,
        os_build: runtime.os_build,
        service_pack_major: runtime.service_pack_major,
        service_pack_minor: runtime.service_pack_minor,
        product_type: runtime.product_type,
        suite_mask_u16_hex: runtime.suite_mask_u16_hex.clone(),
        native_architecture: runtime.native_architecture.clone(),
        process_architecture: runtime.process_architecture.clone(),
        byte_order: "little".to_string(),
        numerical_backend_identity: NATIVE_POLICY_TRAIN_STEP_NUMERICAL_BACKEND_IDENTITY_V1
            .to_string(),
        rustc_release: toolchain.rustc_release.clone(),
        rustc_commit_hash: toolchain.rustc_commit_hash.clone(),
        target_triple: toolchain.target_triple.clone(),
        build_profile: "release".to_string(),
    }
}

fn build_environment_record() -> Result<TrainRunEnvironmentV2> {
    let rally = runtime_deck_by_id(CANONICAL_RALLY_DECK_ID).ok_or_else(|| {
        capture_error(
            NativeStoreProductionCaptureErrorKindV2::CatalogMismatch,
            "runtime_catalog_rally_missing",
        )
    })?;
    Ok(TrainRunEnvironmentV2 {
        card_db_hash_u64_hex: format!("{KERNEL_CARDDB_HASH:016x}"),
        runtime_catalog_schema: RUNTIME_DECK_CATALOG_SCHEMA.to_string(),
        runtime_catalog_protocol: RUNTIME_DECK_PROTOCOL.to_string(),
        runtime_catalog_sha256: RUNTIME_DECK_CATALOG_FILE_SHA256.to_string(),
        deck_ids: [
            CANONICAL_RALLY_DECK_ID.to_string(),
            CANONICAL_RALLY_DECK_ID.to_string(),
        ],
        deck_hashes_u64_hex: [
            format!("{:016x}", rally.runtime_deck_hash),
            format!("{:016x}", rally.runtime_deck_hash),
        ],
        protocol: RL_SESSION_PROTOCOL_NAME.to_string(),
        protocol_version: u64::from(RL_SESSION_PROTOCOL_VERSION),
        schema_version: u64::from(RL_SESSION_SCHEMA_VERSION),
        kernel_version: KERNEL_VERSION.to_string(),
        surface_version: u64::from(H2_PREDICATE_VERSION),
        policy_surface_version: u64::from(POLICY_SURFACE_VERSION),
    })
}

fn capture_and_require_build_source(source_root: &Path) -> Result<StrictSourceTreeCaptureV1> {
    let capture = capture_strict_source_tree_v1(source_root).map_err(|_| {
        capture_error(
            NativeStoreProductionCaptureErrorKindV2::SourceCapture,
            "strict_source_capture_failed",
        )
    })?;
    if capture.git_commit() != build_capture::NATIVE_STORE_BUILD_SOURCE_GIT_COMMIT_V1
        || capture.source_tree_recipe_identity()
            != build_capture::NATIVE_STORE_BUILD_SOURCE_TREE_RECIPE_IDENTITY_V1
        || capture.source_tree_recipe_sha256()
            != build_capture::NATIVE_STORE_BUILD_SOURCE_TREE_RECIPE_SHA256_V1
        || capture.source_tree_sha256() != build_capture::NATIVE_STORE_BUILD_SOURCE_TREE_SHA256_V1
        || capture.worktree_clean() != build_capture::NATIVE_STORE_BUILD_SOURCE_WORKTREE_CLEAN_V1
        || capture.git_status_sha256()
            != build_capture::NATIVE_STORE_BUILD_SOURCE_GIT_STATUS_SHA256_V1
    {
        return Err(capture_error(
            NativeStoreProductionCaptureErrorKindV2::SourceMismatch,
            "runtime_source_does_not_match_embedded_build_source",
        ));
    }
    Ok(capture)
}

fn capture_runtime_catalog(source_root: &Path) -> Result<CatalogSnapshotV2> {
    if RUNTIME_DECK_CATALOG_SCHEMA != "kernel_runtime_decks/v1"
        || RUNTIME_DECK_PROTOCOL != "canonical-mainboard-bo1/v1"
    {
        return Err(capture_error(
            NativeStoreProductionCaptureErrorKindV2::CatalogMismatch,
            "generated_runtime_catalog_authority_mismatch",
        ));
    }
    let catalog_path = source_root.join("data").join("runtime_decks_v1.json");
    let _parent_chain = validate_and_open_parent_chain(&catalog_path)?;
    let mut file = open_regular_read_only(&catalog_path)?;
    let before = capture_regular_handle(&mut file)?;
    let mut reopened = open_regular_read_only(&catalog_path)?;
    let after = capture_regular_handle(&mut reopened)?;
    if before != after || before.sha256 != RUNTIME_DECK_CATALOG_FILE_SHA256 {
        return Err(capture_error(
            NativeStoreProductionCaptureErrorKindV2::CatalogMismatch,
            "runtime_catalog_exact_capture_mismatch",
        ));
    }
    Ok(before)
}

fn capture_current_executable() -> Result<(PathBuf, Vec<File>, File, ExecutableSnapshotV2)> {
    let path = current_module_path()?;
    validate_executable_path(&path, "mtg-kernel-native.exe")?;
    let parent_chain = validate_and_open_parent_chain(&path)?;
    let mut file = open_regular_read_only(&path)?;
    let snapshot = capture_executable_handle(&mut file)?;
    Ok((path, parent_chain, file, snapshot))
}

fn capture_secondary_executable(path: &Path) -> Result<ExecutableSnapshotV2> {
    validate_executable_path(path, "mtg-kernel-native.exe")?;
    let _parent_chain = validate_and_open_parent_chain(path)?;
    let mut file = open_regular_read_only(path)?;
    capture_executable_handle(&mut file)
}

fn current_module_path() -> Result<PathBuf> {
    let module = unsafe { GetModuleHandleW(std::ptr::null()) };
    if module.is_null() {
        return Err(capture_error(
            NativeStoreProductionCaptureErrorKindV2::Path,
            "current_module_handle_failed",
        ));
    }
    for capacity in MODULE_PATH_CAPACITIES {
        // A nonzero sentinel makes the required terminator check observable;
        // zero-filled storage would let a missing API terminator pass vacuously.
        let mut buffer = vec![MODULE_PATH_SENTINEL; capacity];
        let returned = unsafe { GetModuleFileNameW(module, buffer.as_mut_ptr(), capacity as u32) };
        if let Some(path) = interpret_module_path_result(&buffer, returned)? {
            return Ok(path);
        }
    }
    Err(capture_error(
        NativeStoreProductionCaptureErrorKindV2::Path,
        "current_module_path_exceeds_bound",
    ))
}

fn interpret_module_path_result(buffer: &[u16], returned: u32) -> Result<Option<PathBuf>> {
    let capacity = buffer.len();
    let returned = returned as usize;
    if returned == 0 || returned > capacity {
        return Err(capture_error(
            NativeStoreProductionCaptureErrorKindV2::Path,
            "current_module_path_query_failed",
        ));
    }
    if returned == capacity {
        return Ok(None);
    }
    if returned > 32_767 || buffer[returned] != 0 || buffer[..returned].contains(&0) {
        return Err(capture_error(
            NativeStoreProductionCaptureErrorKindV2::Path,
            "current_module_path_malformed",
        ));
    }
    let path = String::from_utf16(&buffer[..returned]).map_err(|_| {
        capture_error(
            NativeStoreProductionCaptureErrorKindV2::Path,
            "current_module_path_not_unicode",
        )
    })?;
    Ok(Some(PathBuf::from(path)))
}

fn validate_executable_path(path: &Path, expected_leaf: &str) -> Result<()> {
    validate_drive_absolute_local_path(path)?;
    let leaf = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| {
            capture_error(
                NativeStoreProductionCaptureErrorKindV2::Path,
                "executable_leaf_invalid",
            )
        })?;
    if !leaf.eq_ignore_ascii_case(expected_leaf) {
        return Err(capture_error(
            NativeStoreProductionCaptureErrorKindV2::Path,
            "executable_leaf_mismatch",
        ));
    }
    Ok(())
}

fn validate_drive_absolute_local_path(path: &Path) -> Result<()> {
    let value = path.to_str().ok_or_else(|| {
        capture_error(
            NativeStoreProductionCaptureErrorKindV2::Path,
            "path_not_unicode",
        )
    })?;
    let bytes = value.as_bytes();
    if bytes.len() < 3
        || !bytes[0].is_ascii_alphabetic()
        || bytes[1] != b':'
        || bytes[2] != b'\\'
        || bytes.contains(&b'/')
        || bytes[2..].contains(&b':')
        || value.starts_with("\\\\")
    {
        return Err(capture_error(
            NativeStoreProductionCaptureErrorKindV2::Path,
            "path_not_drive_absolute_local",
        ));
    }
    if bytes.len() != 3 {
        for component in value[3..].split('\\') {
            if component.is_empty()
                || matches!(component, "." | "..")
                || component.ends_with('.')
                || component.ends_with(' ')
                || component.chars().any(char::is_control)
                || component
                    .bytes()
                    .any(|byte| matches!(byte, b'<' | b'>' | b'"' | b'|' | b'?' | b'*'))
            {
                return Err(capture_error(
                    NativeStoreProductionCaptureErrorKindV2::Path,
                    "path_component_invalid",
                ));
            }
        }
    }
    let drive_root = format!("{}:\\", char::from(bytes[0]));
    let mut wide: Vec<u16> = std::ffi::OsStr::new(&drive_root).encode_wide().collect();
    wide.push(0);
    if unsafe { GetDriveTypeW(wide.as_ptr()) } != DRIVE_FIXED {
        return Err(capture_error(
            NativeStoreProductionCaptureErrorKindV2::Path,
            "path_drive_not_fixed",
        ));
    }
    Ok(())
}

fn validate_and_open_directory_chain(path: &Path) -> Result<Vec<File>> {
    validate_drive_absolute_local_path(path)?;
    let mut current = PathBuf::new();
    let mut retained = Vec::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => current.push(prefix.as_os_str()),
            Component::RootDir => current.push("\\"),
            Component::Normal(value) => {
                current.push(value);
                let file = open_directory_no_follow(&current)?;
                require_directory_handle(&file)?;
                retained.push(file);
            }
            _ => {
                return Err(capture_error(
                    NativeStoreProductionCaptureErrorKindV2::Path,
                    "directory_path_component_invalid",
                ));
            }
        }
    }
    Ok(retained)
}

fn validate_and_open_parent_chain(path: &Path) -> Result<Vec<File>> {
    let parent = path.parent().ok_or_else(|| {
        capture_error(
            NativeStoreProductionCaptureErrorKindV2::Path,
            "path_parent_missing",
        )
    })?;
    validate_and_open_directory_chain(parent)
}

fn open_directory_no_follow(path: &Path) -> Result<File> {
    let mut options = OpenOptions::new();
    options
        .access_mode(FILE_READ_ATTRIBUTES)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT);
    options.open(path).map_err(|_| {
        capture_error(
            NativeStoreProductionCaptureErrorKindV2::FileOpen,
            "directory_open_failed",
        )
    })
}

fn open_regular_read_only(path: &Path) -> Result<File> {
    let mut options = OpenOptions::new();
    options
        .access_mode(FILE_READ_DATA | FILE_READ_ATTRIBUTES)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    options.open(path).map_err(|_| {
        capture_error(
            NativeStoreProductionCaptureErrorKindV2::FileOpen,
            "regular_file_open_failed",
        )
    })
}

fn require_directory_handle(file: &File) -> Result<()> {
    let attributes = query_attribute_tag(file)?;
    if attributes.file_attributes & FILE_ATTRIBUTE_DIRECTORY == 0
        || attributes.file_attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
    {
        return Err(capture_error(
            NativeStoreProductionCaptureErrorKindV2::FileMetadata,
            "directory_handle_kind_invalid",
        ));
    }
    let standard = query_standard_info(file)?;
    if standard.directory == 0 || standard.delete_pending != 0 {
        return Err(capture_error(
            NativeStoreProductionCaptureErrorKindV2::FileMetadata,
            "directory_handle_state_invalid",
        ));
    }
    Ok(())
}

fn query_file_identity(file: &File) -> Result<FileIdentityV2> {
    let mut info = FileIdInfo {
        volume_serial_number: 0,
        file_id: [0; 16],
    };
    let ok = unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle() as Handle,
            FILE_ID_INFO_CLASS,
            (&mut info as *mut FileIdInfo).cast(),
            std::mem::size_of::<FileIdInfo>() as u32,
        )
    };
    if ok == 0 {
        return Err(capture_error(
            NativeStoreProductionCaptureErrorKindV2::FileMetadata,
            "file_id_query_failed",
        ));
    }
    Ok(FileIdentityV2 {
        volume_serial_number: info.volume_serial_number,
        file_id: info.file_id,
    })
}

fn query_standard_info(file: &File) -> Result<FileStandardInfo> {
    let mut info = FileStandardInfo {
        allocation_size: 0,
        end_of_file: 0,
        number_of_links: 0,
        delete_pending: 0,
        directory: 0,
    };
    let ok = unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle() as Handle,
            FILE_STANDARD_INFO_CLASS,
            (&mut info as *mut FileStandardInfo).cast(),
            std::mem::size_of::<FileStandardInfo>() as u32,
        )
    };
    if ok == 0 {
        return Err(capture_error(
            NativeStoreProductionCaptureErrorKindV2::FileMetadata,
            "file_standard_info_query_failed",
        ));
    }
    Ok(info)
}

fn query_attribute_tag(file: &File) -> Result<FileAttributeTagInfo> {
    let mut info = FileAttributeTagInfo {
        file_attributes: 0,
        reparse_tag: 0,
    };
    let ok = unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle() as Handle,
            FILE_ATTRIBUTE_TAG_INFO_CLASS,
            (&mut info as *mut FileAttributeTagInfo).cast(),
            std::mem::size_of::<FileAttributeTagInfo>() as u32,
        )
    };
    if ok == 0 {
        return Err(capture_error(
            NativeStoreProductionCaptureErrorKindV2::FileMetadata,
            "file_attribute_query_failed",
        ));
    }
    Ok(info)
}

fn capture_regular_handle(file: &mut File) -> Result<CatalogSnapshotV2> {
    let identity_before = query_file_identity(file)?;
    let standard_before = query_standard_info(file)?;
    let attributes = query_attribute_tag(file)?;
    let byte_len = require_regular_standard(&standard_before, &attributes)?;
    let sha256 = hash_exact_file(file, byte_len)?;
    let identity_after = query_file_identity(file)?;
    let standard_after = query_standard_info(file)?;
    if identity_after != identity_before
        || standard_after.end_of_file != standard_before.end_of_file
        || standard_after.delete_pending != 0
        || standard_after.directory != 0
    {
        return Err(capture_error(
            NativeStoreProductionCaptureErrorKindV2::FileChanged,
            "regular_file_changed_during_capture",
        ));
    }
    Ok(CatalogSnapshotV2 {
        identity: identity_before,
        byte_len,
        sha256,
    })
}

fn capture_executable_handle(file: &mut File) -> Result<ExecutableSnapshotV2> {
    let regular = capture_regular_handle(file)?;
    let (pe_machine, pe_size_of_image_bytes) = parse_pe32_plus(file, regular.byte_len)?;
    let identity_after = query_file_identity(file)?;
    let standard_after = query_standard_info(file)?;
    if identity_after != regular.identity
        || standard_after.end_of_file != regular.byte_len as i64
        || standard_after.delete_pending != 0
        || standard_after.directory != 0
    {
        return Err(capture_error(
            NativeStoreProductionCaptureErrorKindV2::FileChanged,
            "executable_changed_during_pe_capture",
        ));
    }
    Ok(ExecutableSnapshotV2 {
        identity: regular.identity,
        byte_len: regular.byte_len,
        sha256: regular.sha256,
        pe_machine,
        pe_size_of_image_bytes,
    })
}

fn require_regular_standard(
    standard: &FileStandardInfo,
    attributes: &FileAttributeTagInfo,
) -> Result<u64> {
    if standard.directory != 0
        || standard.delete_pending != 0
        || standard.end_of_file <= 0
        || attributes.file_attributes & FILE_ATTRIBUTE_DIRECTORY != 0
        || attributes.file_attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
    {
        return Err(capture_error(
            NativeStoreProductionCaptureErrorKindV2::FileMetadata,
            "regular_file_state_invalid",
        ));
    }
    let byte_len = u64::try_from(standard.end_of_file).map_err(|_| {
        capture_error(
            NativeStoreProductionCaptureErrorKindV2::FileMetadata,
            "regular_file_length_invalid",
        )
    })?;
    if byte_len > MAX_U63 {
        return Err(capture_error(
            NativeStoreProductionCaptureErrorKindV2::FileMetadata,
            "regular_file_length_out_of_range",
        ));
    }
    Ok(byte_len)
}

fn hash_exact_file(file: &mut File, byte_len: u64) -> Result<String> {
    file.seek(SeekFrom::Start(0)).map_err(|_| {
        capture_error(
            NativeStoreProductionCaptureErrorKindV2::FileRead,
            "file_seek_failed",
        )
    })?;
    let mut remaining = byte_len;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 65_536];
    while remaining != 0 {
        let wanted = usize::try_from(remaining.min(buffer.len() as u64)).unwrap_or(buffer.len());
        let read = file.read(&mut buffer[..wanted]).map_err(|_| {
            capture_error(
                NativeStoreProductionCaptureErrorKindV2::FileRead,
                "file_read_failed",
            )
        })?;
        if read == 0 {
            return Err(capture_error(
                NativeStoreProductionCaptureErrorKindV2::FileChanged,
                "file_short_read",
            ));
        }
        hasher.update(&buffer[..read]);
        remaining -= read as u64;
    }
    let mut extra = [0u8; 1];
    if file.read(&mut extra).map_err(|_| {
        capture_error(
            NativeStoreProductionCaptureErrorKindV2::FileRead,
            "file_eof_check_failed",
        )
    })? != 0
    {
        return Err(capture_error(
            NativeStoreProductionCaptureErrorKindV2::FileChanged,
            "file_grew_during_capture",
        ));
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn parse_pe32_plus(file: &mut File, byte_len: u64) -> Result<(u16, u64)> {
    let mut dos = [0u8; 64];
    read_exact_at(file, 0, &mut dos, byte_len)?;
    if &dos[..2] != b"MZ" {
        return Err(capture_error(
            NativeStoreProductionCaptureErrorKindV2::ExecutableFormat,
            "executable_dos_signature_invalid",
        ));
    }
    let pe_offset = u64::from(u32::from_le_bytes(dos[0x3c..0x40].try_into().unwrap()));
    let mut header = [0u8; 24 + PE32_PLUS_MIN_OPTIONAL_HEADER_BYTES];
    read_exact_at(file, pe_offset, &mut header, byte_len)?;
    validate_pe32_plus_header(&header, pe_offset, byte_len)
}

fn validate_pe32_plus_header(
    header: &[u8; 24 + PE32_PLUS_MIN_OPTIONAL_HEADER_BYTES],
    pe_offset: u64,
    byte_len: u64,
) -> Result<(u16, u64)> {
    if &header[..4] != b"PE\0\0" {
        return Err(capture_error(
            NativeStoreProductionCaptureErrorKindV2::ExecutableFormat,
            "executable_pe_signature_invalid",
        ));
    }
    let machine = u16::from_le_bytes(header[4..6].try_into().unwrap());
    let section_count = u16::from_le_bytes(header[6..8].try_into().unwrap());
    let optional_size = usize::from(u16::from_le_bytes(header[20..22].try_into().unwrap()));
    let characteristics = u16::from_le_bytes(header[22..24].try_into().unwrap());
    if optional_size < PE32_PLUS_MIN_OPTIONAL_HEADER_BYTES
        || u16::from_le_bytes(header[24..26].try_into().unwrap()) != 0x020b
    {
        return Err(capture_error(
            NativeStoreProductionCaptureErrorKindV2::ExecutableFormat,
            "executable_not_pe32_plus",
        ));
    }
    if section_count == 0 || section_count > PE_MAX_SECTION_COUNT {
        return Err(capture_error(
            NativeStoreProductionCaptureErrorKindV2::ExecutableFormat,
            "executable_section_count_invalid",
        ));
    }
    if characteristics & IMAGE_FILE_EXECUTABLE_IMAGE == 0 || characteristics & IMAGE_FILE_DLL != 0 {
        return Err(capture_error(
            NativeStoreProductionCaptureErrorKindV2::ExecutableFormat,
            "executable_characteristics_invalid",
        ));
    }
    if machine != compiled_target_machine() {
        return Err(capture_error(
            NativeStoreProductionCaptureErrorKindV2::ExecutableFormat,
            "executable_machine_mismatch",
        ));
    }

    let optional_end = pe_offset
        .checked_add(24)
        .and_then(|value| value.checked_add(optional_size as u64))
        .ok_or_else(|| {
            capture_error(
                NativeStoreProductionCaptureErrorKindV2::ExecutableFormat,
                "executable_optional_header_extent_overflow",
            )
        })?;
    let section_table_end = optional_end
        .checked_add(u64::from(section_count) * PE_SECTION_HEADER_BYTES)
        .ok_or_else(|| {
            capture_error(
                NativeStoreProductionCaptureErrorKindV2::ExecutableFormat,
                "executable_section_table_extent_overflow",
            )
        })?;
    if optional_end > byte_len || section_table_end > byte_len {
        return Err(capture_error(
            NativeStoreProductionCaptureErrorKindV2::ExecutableFormat,
            "executable_declared_headers_truncated",
        ));
    }

    let data_directory_count = u32::from_le_bytes(header[132..136].try_into().unwrap());
    let required_optional_bytes = PE32_PLUS_MIN_OPTIONAL_HEADER_BYTES.saturating_add(
        usize::try_from(data_directory_count)
            .unwrap_or(usize::MAX)
            .saturating_mul(8),
    );
    if data_directory_count > PE_MAX_DATA_DIRECTORY_COUNT || required_optional_bytes > optional_size
    {
        return Err(capture_error(
            NativeStoreProductionCaptureErrorKindV2::ExecutableFormat,
            "executable_data_directory_extent_invalid",
        ));
    }

    let section_alignment = u64::from(u32::from_le_bytes(header[56..60].try_into().unwrap()));
    let file_alignment = u64::from(u32::from_le_bytes(header[60..64].try_into().unwrap()));
    let size_of_image = u64::from(u32::from_le_bytes(header[80..84].try_into().unwrap()));
    let size_of_headers = u64::from(u32::from_le_bytes(header[84..88].try_into().unwrap()));
    if section_alignment == 0
        || file_alignment == 0
        || section_alignment < file_alignment
        || !file_alignment.is_power_of_two()
        || !(512..=65_536).contains(&file_alignment)
        || size_of_headers < section_table_end
        || size_of_headers > byte_len
        || size_of_image < size_of_headers
        || size_of_image > MAX_U63
        || size_of_image % section_alignment != 0
    {
        return Err(capture_error(
            NativeStoreProductionCaptureErrorKindV2::ExecutableFormat,
            "executable_image_layout_invalid",
        ));
    }
    Ok((machine, size_of_image))
}

fn read_exact_at(file: &mut File, offset: u64, output: &mut [u8], byte_len: u64) -> Result<()> {
    let end = offset.checked_add(output.len() as u64).ok_or_else(|| {
        capture_error(
            NativeStoreProductionCaptureErrorKindV2::ExecutableFormat,
            "executable_offset_overflow",
        )
    })?;
    if end > byte_len {
        return Err(capture_error(
            NativeStoreProductionCaptureErrorKindV2::ExecutableFormat,
            "executable_header_truncated",
        ));
    }
    file.seek(SeekFrom::Start(offset)).map_err(|_| {
        capture_error(
            NativeStoreProductionCaptureErrorKindV2::FileRead,
            "executable_seek_failed",
        )
    })?;
    file.read_exact(output).map_err(|_| {
        capture_error(
            NativeStoreProductionCaptureErrorKindV2::FileRead,
            "executable_header_read_failed",
        )
    })
}

fn file_id_hex(identifier: &[u8; 16]) -> String {
    use std::fmt::Write as _;
    let mut result = String::with_capacity(32);
    for byte in identifier {
        write!(&mut result, "{byte:02x}").expect("writing into String is infallible");
    }
    result
}

fn capture_runtime_snapshot() -> Result<RuntimeSnapshotV2> {
    let mut version = RtlOsVersionInfoExW {
        os_version_info_size: std::mem::size_of::<RtlOsVersionInfoExW>() as u32,
        major_version: 0,
        minor_version: 0,
        build_number: 0,
        platform_id: 0,
        csd_version: [0; 128],
        service_pack_major: 0,
        service_pack_minor: 0,
        suite_mask: 0,
        product_type: 0,
        reserved: 0,
    };
    if unsafe { RtlGetVersion(&mut version) } != 0 {
        return Err(capture_error(
            NativeStoreProductionCaptureErrorKindV2::RuntimeCapture,
            "rtl_get_version_failed",
        ));
    }

    let mut system_info = SystemInfo {
        processor_architecture: u16::MAX,
        reserved: 0,
        page_size: 0,
        minimum_application_address: std::ptr::null_mut(),
        maximum_application_address: std::ptr::null_mut(),
        active_processor_mask: 0,
        number_of_processors: 0,
        processor_type: 0,
        allocation_granularity: 0,
        processor_level: 0,
        processor_revision: 0,
    };
    unsafe { GetNativeSystemInfo(&mut system_info) };

    let mut process_machine = u16::MAX;
    let mut native_machine = u16::MAX;
    if unsafe {
        IsWow64Process2(
            GetCurrentProcess(),
            &mut process_machine,
            &mut native_machine,
        )
    } == 0
    {
        return Err(capture_error(
            NativeStoreProductionCaptureErrorKindV2::RuntimeCapture,
            "is_wow64_process2_failed",
        ));
    }

    validate_runtime_snapshot_components(
        &version,
        system_info.processor_architecture,
        process_machine,
        native_machine,
    )
}

fn validate_runtime_snapshot_components(
    version: &RtlOsVersionInfoExW,
    system_processor_architecture: u16,
    process_machine: u16,
    native_machine: u16,
) -> Result<RuntimeSnapshotV2> {
    if version.os_version_info_size as usize != std::mem::size_of::<RtlOsVersionInfoExW>()
        || version.platform_id != VER_PLATFORM_WIN32_NT
        || version.major_version == 0
        || version.build_number == 0
        || !matches!(
            version.product_type,
            VER_NT_WORKSTATION | VER_NT_DOMAIN_CONTROLLER | VER_NT_SERVER
        )
        || version.reserved != 0
    {
        return Err(capture_error(
            NativeStoreProductionCaptureErrorKindV2::RuntimeCapture,
            "rtl_version_result_invalid",
        ));
    }

    let native_architecture = architecture_name(native_machine)?;
    if architecture_from_system_info(system_processor_architecture)? != native_architecture {
        return Err(capture_error(
            NativeStoreProductionCaptureErrorKindV2::RuntimeMismatch,
            "native_architecture_capture_mismatch",
        ));
    }
    let effective_process_machine = if process_machine == IMAGE_FILE_MACHINE_UNKNOWN {
        native_machine
    } else {
        process_machine
    };
    let process_architecture = architecture_name(effective_process_machine)?;
    if !matches!(
        (native_machine, effective_process_machine),
        (IMAGE_FILE_MACHINE_AMD64, IMAGE_FILE_MACHINE_AMD64)
            | (IMAGE_FILE_MACHINE_ARM64, IMAGE_FILE_MACHINE_ARM64)
            | (IMAGE_FILE_MACHINE_ARM64, IMAGE_FILE_MACHINE_AMD64)
    ) {
        return Err(capture_error(
            NativeStoreProductionCaptureErrorKindV2::RuntimeMismatch,
            "native_process_architecture_pair_invalid",
        ));
    }
    if effective_process_machine != compiled_target_machine() {
        return Err(capture_error(
            NativeStoreProductionCaptureErrorKindV2::RuntimeMismatch,
            "process_architecture_target_mismatch",
        ));
    }

    Ok(RuntimeSnapshotV2 {
        os_major: u64::from(version.major_version),
        os_minor: u64::from(version.minor_version),
        os_build: u64::from(version.build_number),
        service_pack_major: u64::from(version.service_pack_major),
        service_pack_minor: u64::from(version.service_pack_minor),
        product_type: u64::from(version.product_type),
        suite_mask_u16_hex: format!("{:04x}", version.suite_mask),
        native_architecture: native_architecture.to_string(),
        process_architecture: process_architecture.to_string(),
    })
}

const fn compiled_target_machine() -> u16 {
    #[cfg(target_arch = "x86_64")]
    {
        IMAGE_FILE_MACHINE_AMD64
    }
    #[cfg(target_arch = "aarch64")]
    {
        IMAGE_FILE_MACHINE_ARM64
    }
}

fn architecture_name(machine: u16) -> Result<&'static str> {
    match machine {
        IMAGE_FILE_MACHINE_AMD64 => Ok("amd64"),
        IMAGE_FILE_MACHINE_ARM64 => Ok("arm64"),
        _ => Err(capture_error(
            NativeStoreProductionCaptureErrorKindV2::RuntimeCapture,
            "runtime_machine_unknown",
        )),
    }
}

fn architecture_from_system_info(value: u16) -> Result<&'static str> {
    match value {
        9 => Ok("amd64"),
        12 => Ok("arm64"),
        _ => Err(capture_error(
            NativeStoreProductionCaptureErrorKindV2::RuntimeCapture,
            "system_info_architecture_unknown",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_pe32_plus_header() -> [u8; 24 + PE32_PLUS_MIN_OPTIONAL_HEADER_BYTES] {
        let mut header = [0u8; 24 + PE32_PLUS_MIN_OPTIONAL_HEADER_BYTES];
        header[..4].copy_from_slice(b"PE\0\0");
        header[4..6].copy_from_slice(&compiled_target_machine().to_le_bytes());
        header[6..8].copy_from_slice(&3u16.to_le_bytes());
        header[20..22].copy_from_slice(&240u16.to_le_bytes());
        header[22..24].copy_from_slice(&IMAGE_FILE_EXECUTABLE_IMAGE.to_le_bytes());
        header[24..26].copy_from_slice(&0x020bu16.to_le_bytes());
        header[56..60].copy_from_slice(&4_096u32.to_le_bytes());
        header[60..64].copy_from_slice(&512u32.to_le_bytes());
        header[80..84].copy_from_slice(&8_192u32.to_le_bytes());
        header[84..88].copy_from_slice(&1_024u32.to_le_bytes());
        header[132..136].copy_from_slice(&16u32.to_le_bytes());
        header
    }

    fn valid_windows_version() -> RtlOsVersionInfoExW {
        RtlOsVersionInfoExW {
            os_version_info_size: std::mem::size_of::<RtlOsVersionInfoExW>() as u32,
            major_version: 10,
            minor_version: 0,
            build_number: 26_100,
            platform_id: VER_PLATFORM_WIN32_NT,
            csd_version: [0; 128],
            service_pack_major: 0,
            service_pack_minor: 0,
            suite_mask: 0,
            product_type: VER_NT_WORKSTATION,
            reserved: 0,
        }
    }

    #[test]
    fn file_id_hex_uses_api_array_order() {
        assert_eq!(
            file_id_hex(&[
                0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d,
                0x0e, 0x0f,
            ]),
            "000102030405060708090a0b0c0d0e0f"
        );
    }

    #[test]
    fn module_path_capacity_sequence_is_frozen() {
        assert_eq!(
            MODULE_PATH_CAPACITIES,
            [260, 520, 1_040, 2_080, 4_160, 8_320, 16_640, 32_768]
        );
    }

    #[test]
    fn module_path_result_requires_api_written_terminator() {
        let encoded: Vec<u16> = "C:\\x".encode_utf16().collect();
        let mut buffer = vec![MODULE_PATH_SENTINEL; 8];
        buffer[..encoded.len()].copy_from_slice(&encoded);
        assert_eq!(
            interpret_module_path_result(&buffer, encoded.len() as u32)
                .unwrap_err()
                .code(),
            "current_module_path_malformed"
        );
        buffer[encoded.len()] = 0;
        assert_eq!(
            interpret_module_path_result(&buffer, encoded.len() as u32)
                .unwrap()
                .unwrap(),
            PathBuf::from("C:\\x")
        );
    }

    #[test]
    fn module_path_result_pins_failure_and_truncation_boundaries() {
        let buffer = vec![MODULE_PATH_SENTINEL; 8];
        assert!(interpret_module_path_result(&buffer, 0).is_err());
        assert!(interpret_module_path_result(&buffer, 9).is_err());
        assert!(interpret_module_path_result(&buffer, 8).unwrap().is_none());

        let mut interior_nul = buffer;
        interior_nul[0] = b'C' as u16;
        interior_nul[1] = 0;
        interior_nul[2] = 0;
        assert!(interpret_module_path_result(&interior_nul, 2).is_err());
    }

    #[test]
    fn runtime_path_validation_rejects_ascii_and_unicode_controls() {
        assert!(validate_drive_absolute_local_path(Path::new("C:\\safe")).is_ok());
        assert!(validate_drive_absolute_local_path(Path::new("C:\\del\u{7f}")).is_err());
        assert!(validate_drive_absolute_local_path(Path::new("C:\\c1\u{85}")).is_err());
    }

    #[test]
    fn pe32_plus_header_requires_structural_image_extents() {
        let header = valid_pe32_plus_header();
        assert_eq!(
            validate_pe32_plus_header(&header, 128, 4_096).unwrap(),
            (compiled_target_machine(), 8_192)
        );

        let mut oversized_optional = header;
        oversized_optional[20..22].copy_from_slice(&u16::MAX.to_le_bytes());
        assert_eq!(
            validate_pe32_plus_header(&oversized_optional, 128, 4_096)
                .unwrap_err()
                .code(),
            "executable_declared_headers_truncated"
        );

        let mut no_sections = header;
        no_sections[6..8].copy_from_slice(&0u16.to_le_bytes());
        assert_eq!(
            validate_pe32_plus_header(&no_sections, 128, 4_096)
                .unwrap_err()
                .code(),
            "executable_section_count_invalid"
        );

        let mut not_executable = header;
        not_executable[22..24].copy_from_slice(&0u16.to_le_bytes());
        assert_eq!(
            validate_pe32_plus_header(&not_executable, 128, 4_096)
                .unwrap_err()
                .code(),
            "executable_characteristics_invalid"
        );
    }

    #[test]
    fn current_test_executable_has_a_stable_real_pe32_plus_snapshot() {
        let path = current_module_path().unwrap();
        let _parent_chain = validate_and_open_parent_chain(&path).unwrap();
        let mut primary = open_regular_read_only(&path).unwrap();
        let first = capture_executable_handle(&mut primary).unwrap();
        let mut secondary = open_regular_read_only(&path).unwrap();
        let second = capture_executable_handle(&mut secondary).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.pe_machine, compiled_target_machine());
        assert!(first.byte_len > 0);
        assert!(first.pe_size_of_image_bytes > 0);
    }

    #[test]
    fn runtime_component_validator_rejects_invalid_api_results_and_arch_pairs() {
        let mut invalid_version = valid_windows_version();
        invalid_version.platform_id = 0;
        assert_eq!(
            validate_runtime_snapshot_components(
                &invalid_version,
                9,
                IMAGE_FILE_MACHINE_UNKNOWN,
                IMAGE_FILE_MACHINE_AMD64,
            )
            .unwrap_err()
            .code(),
            "rtl_version_result_invalid"
        );

        assert_eq!(
            validate_runtime_snapshot_components(
                &valid_windows_version(),
                9,
                IMAGE_FILE_MACHINE_ARM64,
                IMAGE_FILE_MACHINE_AMD64,
            )
            .unwrap_err()
            .code(),
            "native_process_architecture_pair_invalid"
        );
    }

    #[test]
    fn current_process_runtime_capture_is_coherent() {
        let capture = capture_runtime_snapshot().unwrap();
        assert!(matches!(
            capture.native_architecture.as_str(),
            "amd64" | "arm64"
        ));
        assert_eq!(
            capture.process_architecture,
            architecture_name(compiled_target_machine()).unwrap()
        );
    }
}
