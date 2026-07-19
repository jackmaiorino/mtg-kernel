//! Build-time capture for the Windows-only native training-store V2 writer.
//!
//! This module is included by `mtg-kernel/build.rs` and, for portable parser
//! tests, by an integration-test crate.  The generated Rust source contains
//! only path-free values.  Builds without the production feature, and all
//! non-Windows targets, generate a comment-only file with no capture constants.

use semver::Version;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{OsStr, OsString};
use std::fmt;
#[cfg(any(not(test), windows))]
use std::fs;
use std::path::{Path, PathBuf};

const PRODUCTION_FEATURE_NAME_V1: &str = "native-training-store-v2-production";
const PRODUCTION_FEATURE_ENV_V1: &str = "CARGO_FEATURE_NATIVE_TRAINING_STORE_V2_PRODUCTION";
const GENERATED_FILE_NAME_V1: &str = "native_store_build_capture_v1.rs";
const PACKAGE_NAME_V1: &str = "mtg-kernel";
const EMPTY_SHA256_V1: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
const ADMITTED_X86_64_TRIPLE_V1: &str = "x86_64-pc-windows-msvc";
const ADMITTED_AARCH64_TRIPLE_V1: &str = "aarch64-pc-windows-msvc";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct BuildCaptureErrorV1(&'static str);

impl fmt::Display for BuildCaptureErrorV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

impl std::error::Error for BuildCaptureErrorV1 {}

type CaptureResultV1<T> = Result<T, BuildCaptureErrorV1>;

fn capture_error(code: &'static str) -> BuildCaptureErrorV1 {
    BuildCaptureErrorV1(code)
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ParsedManifestV1 {
    package_name: String,
    package_version: String,
    feature_by_normalized_env_suffix: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RustcVerboseVersionV1 {
    rustc_release: String,
    rustc_commit_hash: String,
    rustc_commit_date: String,
    host_triple: String,
    llvm_version: String,
    raw_sha256: String,
    line_ending: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct WindowsAdmissionV1 {
    host_triple: String,
    target_triple: String,
    build_profile: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BuildCaptureV1 {
    package_name: String,
    package_version: String,
    workspace_manifest_sha256: String,
    crate_manifest_sha256: String,
    cargo_lock_sha256: String,
    enabled_features: Vec<String>,
    rustc_release: String,
    rustc_commit_hash: String,
    rustc_commit_date: String,
    host_triple: String,
    target_triple: String,
    llvm_version: String,
    rustc_verbose_version_sha256: String,
    rustc_verbose_version_line_ending: String,
    build_profile: String,
    source_git_commit: String,
    source_tree_recipe_identity: String,
    source_tree_recipe_sha256: String,
    source_tree_recipe_byte_count: u64,
    source_tree_sha256: String,
    source_worktree_clean: bool,
    source_git_status_sha256: String,
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(64);
    for byte in digest {
        use fmt::Write as _;
        write!(&mut output, "{byte:02x}").expect("writing to String cannot fail");
    }
    output
}

fn is_nonempty_printable_ascii(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|byte| (0x20..=0x7e).contains(&byte))
}

fn is_lower_hex(value: &str, expected_length: usize) -> bool {
    value.len() == expected_length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn is_feature_name(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

fn normalize_feature_env_suffix(value: &str) -> String {
    value
        .bytes()
        .map(|byte| {
            if byte == b'-' {
                '_'
            } else {
                byte.to_ascii_uppercase() as char
            }
        })
        .collect()
}

fn parse_crate_manifest_v1(raw: &[u8]) -> CaptureResultV1<ParsedManifestV1> {
    let text = std::str::from_utf8(raw)
        .map_err(|_| capture_error("native_store_crate_manifest_not_utf8"))?;
    let value = text
        .parse::<toml::Value>()
        .map_err(|_| capture_error("native_store_crate_manifest_toml_invalid"))?;
    let package = value
        .get("package")
        .and_then(toml::Value::as_table)
        .ok_or_else(|| capture_error("native_store_crate_manifest_package_missing"))?;
    let package_name = package
        .get("name")
        .and_then(toml::Value::as_str)
        .ok_or_else(|| capture_error("native_store_crate_manifest_package_name_invalid"))?;
    let package_version = package
        .get("version")
        .and_then(toml::Value::as_str)
        .ok_or_else(|| capture_error("native_store_crate_manifest_package_version_invalid"))?;
    if package_name != PACKAGE_NAME_V1 {
        return Err(capture_error(
            "native_store_crate_manifest_package_name_mismatch",
        ));
    }
    if !is_nonempty_printable_ascii(package_version) || Version::parse(package_version).is_err() {
        return Err(capture_error(
            "native_store_crate_manifest_package_version_not_semver",
        ));
    }

    let mut feature_by_normalized_env_suffix = BTreeMap::new();
    if let Some(features) = value.get("features") {
        let features = features
            .as_table()
            .ok_or_else(|| capture_error("native_store_crate_manifest_features_invalid"))?;
        for feature_name in features.keys() {
            if !is_feature_name(feature_name) {
                return Err(capture_error(
                    "native_store_crate_manifest_feature_name_invalid",
                ));
            }
            let normalized = normalize_feature_env_suffix(feature_name);
            if feature_by_normalized_env_suffix
                .insert(normalized, feature_name.clone())
                .is_some()
            {
                return Err(capture_error(
                    "native_store_crate_manifest_feature_normalization_collision",
                ));
            }
        }
    }
    if !feature_by_normalized_env_suffix
        .values()
        .any(|value| value == PRODUCTION_FEATURE_NAME_V1)
    {
        return Err(capture_error(
            "native_store_production_feature_missing_from_manifest",
        ));
    }

    Ok(ParsedManifestV1 {
        package_name: package_name.to_owned(),
        package_version: package_version.to_owned(),
        feature_by_normalized_env_suffix,
    })
}

fn enabled_features_from_environment_v1(
    manifest: &ParsedManifestV1,
    environment: &[(OsString, OsString)],
) -> CaptureResultV1<Vec<String>> {
    let mut enabled = BTreeSet::new();
    for (name, value) in environment {
        let Some(name) = name.to_str() else {
            continue;
        };
        let Some(suffix) = name.strip_prefix("CARGO_FEATURE_") else {
            continue;
        };
        if suffix.is_empty() {
            return Err(capture_error("native_store_cargo_feature_name_empty"));
        }
        if value.as_encoded_bytes() != b"1" {
            return Err(capture_error("native_store_cargo_feature_value_invalid"));
        }
        let feature = manifest
            .feature_by_normalized_env_suffix
            .get(suffix)
            .ok_or_else(|| capture_error("native_store_cargo_feature_unknown"))?;
        if !enabled.insert(feature.clone()) {
            return Err(capture_error("native_store_cargo_feature_duplicate"));
        }
    }
    if !enabled.contains(PRODUCTION_FEATURE_NAME_V1) {
        return Err(capture_error(
            "native_store_production_feature_environment_missing",
        ));
    }
    Ok(enabled.into_iter().collect())
}

fn required_environment_unicode_v1(
    environment: &[(OsString, OsString)],
    required_name: &str,
) -> CaptureResultV1<String> {
    let mut matches = environment
        .iter()
        .filter(|(name, _)| name == OsStr::new(required_name));
    let (_, value) = matches
        .next()
        .ok_or_else(|| capture_error("native_store_required_build_environment_missing"))?;
    if matches.next().is_some() {
        return Err(capture_error(
            "native_store_required_build_environment_duplicate",
        ));
    }
    let value = value
        .to_str()
        .ok_or_else(|| capture_error("native_store_required_build_environment_not_unicode"))?;
    if value.is_empty() || value.contains('\0') {
        return Err(capture_error(
            "native_store_required_build_environment_invalid",
        ));
    }
    Ok(value.to_owned())
}

fn admitted_triple_arch_v1(triple: &str) -> Option<&'static str> {
    match triple {
        ADMITTED_X86_64_TRIPLE_V1 => Some("x86_64"),
        ADMITTED_AARCH64_TRIPLE_V1 => Some("aarch64"),
        _ => None,
    }
}

fn validate_windows_admission_v1(
    environment: &[(OsString, OsString)],
    build_script_debug_assertions: bool,
) -> CaptureResultV1<WindowsAdmissionV1> {
    let target_os = required_environment_unicode_v1(environment, "CARGO_CFG_TARGET_OS")?;
    let target_env = required_environment_unicode_v1(environment, "CARGO_CFG_TARGET_ENV")?;
    let target_arch = required_environment_unicode_v1(environment, "CARGO_CFG_TARGET_ARCH")?;
    let host = required_environment_unicode_v1(environment, "HOST")?;
    let target = required_environment_unicode_v1(environment, "TARGET")?;
    let profile = required_environment_unicode_v1(environment, "PROFILE")?;

    if target_os != "windows" || target_env != "msvc" {
        return Err(capture_error("native_store_windows_abi_not_admitted"));
    }
    admitted_triple_arch_v1(&host)
        .ok_or_else(|| capture_error("native_store_host_triple_not_admitted"))?;
    let expected_target_arch = admitted_triple_arch_v1(&target)
        .ok_or_else(|| capture_error("native_store_target_triple_not_admitted"))?;
    if expected_target_arch != target_arch {
        return Err(capture_error("native_store_target_arch_mismatch"));
    }
    if profile != "release" {
        return Err(capture_error("native_store_build_profile_not_release"));
    }
    if build_script_debug_assertions
        || environment
            .iter()
            .any(|(name, _)| name == OsStr::new("CARGO_CFG_DEBUG_ASSERTIONS"))
    {
        return Err(capture_error("native_store_debug_assertions_not_admitted"));
    }

    Ok(WindowsAdmissionV1 {
        host_triple: host,
        target_triple: target,
        build_profile: profile,
    })
}

fn validate_yyyy_mm_dd_v1(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes
            .iter()
            .enumerate()
            .any(|(index, byte)| index != 4 && index != 7 && !byte.is_ascii_digit())
    {
        return false;
    }
    let number = |range: std::ops::Range<usize>| -> u32 {
        bytes[range]
            .iter()
            .fold(0, |value, byte| value * 10 + u32::from(byte - b'0'))
    };
    let year = number(0..4);
    let month = number(5..7);
    let day = number(8..10);
    if year == 0 || !(1..=12).contains(&month) {
        return false;
    }
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let maximum_day = match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    (1..=maximum_day).contains(&day)
}

fn parse_rustc_verbose_version_v1(raw: &[u8]) -> CaptureResultV1<RustcVerboseVersionV1> {
    if raw.is_empty() || raw.len() > 65_536 {
        return Err(capture_error("native_store_rustc_stdout_size_invalid"));
    }
    if raw.contains(&0) {
        return Err(capture_error("native_store_rustc_stdout_contains_nul"));
    }
    if !raw.ends_with(b"\n") {
        return Err(capture_error(
            "native_store_rustc_stdout_missing_final_newline",
        ));
    }

    let mut saw_lf = false;
    let mut saw_crlf = false;
    for (index, byte) in raw.iter().copied().enumerate() {
        match byte {
            b'\r' => {
                if raw.get(index + 1) != Some(&b'\n') {
                    return Err(capture_error("native_store_rustc_stdout_bare_cr"));
                }
                saw_crlf = true;
            }
            b'\n' => {
                if index == 0 || raw[index - 1] != b'\r' {
                    saw_lf = true;
                }
            }
            _ => {}
        }
    }
    if saw_lf && saw_crlf {
        return Err(capture_error(
            "native_store_rustc_stdout_mixed_line_endings",
        ));
    }
    let line_ending = if saw_crlf { "crlf" } else { "lf" };
    let text = std::str::from_utf8(raw)
        .map_err(|_| capture_error("native_store_rustc_stdout_not_utf8"))?;
    let lines: Vec<&str> = if saw_crlf {
        text.split_terminator("\r\n").collect()
    } else {
        text.split_terminator('\n').collect()
    };
    if lines.is_empty() || lines.iter().any(|line| line.is_empty()) {
        return Err(capture_error("native_store_rustc_stdout_empty_line"));
    }
    if !lines[0].starts_with("rustc ") || lines[0].len() == "rustc ".len() {
        return Err(capture_error("native_store_rustc_leading_line_invalid"));
    }
    if lines.iter().skip(1).any(|line| line.starts_with("rustc ")) {
        return Err(capture_error("native_store_rustc_leading_line_duplicate"));
    }

    let mut required = BTreeMap::<&'static str, String>::new();
    let required_keys = [
        ("commit-hash:", "commit-hash: "),
        ("commit-date:", "commit-date: "),
        ("host:", "host: "),
        ("release:", "release: "),
        ("LLVM version:", "LLVM version: "),
    ];
    for line in lines.iter().skip(1) {
        for (key, exact_prefix) in required_keys {
            if line.starts_with(key) {
                let value = line
                    .strip_prefix(exact_prefix)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| capture_error("native_store_rustc_required_line_invalid"))?;
                if required.insert(key, value.to_owned()).is_some() {
                    return Err(capture_error("native_store_rustc_required_line_duplicate"));
                }
                break;
            }
        }
    }
    let take = |key: &'static str| {
        required
            .get(key)
            .cloned()
            .ok_or_else(|| capture_error("native_store_rustc_required_line_missing"))
    };
    let rustc_commit_hash = take("commit-hash:")?;
    let rustc_commit_date = take("commit-date:")?;
    let host_triple = take("host:")?;
    let rustc_release = take("release:")?;
    let llvm_version = take("LLVM version:")?;

    if !is_lower_hex(&rustc_commit_hash, 40) {
        return Err(capture_error("native_store_rustc_commit_hash_invalid"));
    }
    if !validate_yyyy_mm_dd_v1(&rustc_commit_date) {
        return Err(capture_error("native_store_rustc_commit_date_invalid"));
    }
    if admitted_triple_arch_v1(&host_triple).is_none() {
        return Err(capture_error("native_store_rustc_host_not_admitted"));
    }
    if !is_nonempty_printable_ascii(&rustc_release) || !is_nonempty_printable_ascii(&llvm_version) {
        return Err(capture_error("native_store_rustc_printable_value_invalid"));
    }

    Ok(RustcVerboseVersionV1 {
        rustc_release,
        rustc_commit_hash,
        rustc_commit_date,
        host_triple,
        llvm_version,
        raw_sha256: sha256_hex(raw),
        line_ending,
    })
}

fn validate_drive_absolute_windows_path_v1(value: &str) -> CaptureResultV1<()> {
    let bytes = value.as_bytes();
    if value.is_empty()
        || value.contains('\0')
        || value.contains('/')
        || value.contains('%')
        || bytes.len() < 4
        || !bytes[0].is_ascii_alphabetic()
        || bytes[1] != b':'
        || bytes[2] != b'\\'
        || bytes[3] == b'\\'
        || bytes[2..].contains(&b':')
        || value.starts_with("\\\\")
    {
        return Err(capture_error("native_store_rustc_path_not_drive_absolute"));
    }
    let mut component_count = 0usize;
    for component in value[3..].split('\\') {
        if component.is_empty()
            || component == "."
            || component == ".."
            || component.ends_with(' ')
            || component.ends_with('.')
        {
            return Err(capture_error("native_store_rustc_path_component_invalid"));
        }
        component_count += 1;
    }
    if component_count == 0 {
        return Err(capture_error("native_store_rustc_path_leaf_missing"));
    }
    Ok(())
}

fn rust_string_literal(value: &str) -> String {
    format!("{value:?}")
}

fn render_capture_constants_v1(capture: &BuildCaptureV1) -> String {
    let mut output = String::from(
        "// @generated by native_store_build_capture_v1; path-free production tuple.\n",
    );
    macro_rules! string_constant {
        ($name:literal, $value:expr) => {{
            use fmt::Write as _;
            writeln!(
                &mut output,
                "pub(crate) const {}: &str = {};",
                $name,
                rust_string_literal($value)
            )
            .expect("writing to String cannot fail");
        }};
    }
    string_constant!("NATIVE_STORE_BUILD_PACKAGE_NAME_V1", &capture.package_name);
    string_constant!(
        "NATIVE_STORE_BUILD_PACKAGE_VERSION_V1",
        &capture.package_version
    );
    string_constant!(
        "NATIVE_STORE_BUILD_WORKSPACE_MANIFEST_SHA256_V1",
        &capture.workspace_manifest_sha256
    );
    string_constant!(
        "NATIVE_STORE_BUILD_CRATE_MANIFEST_SHA256_V1",
        &capture.crate_manifest_sha256
    );
    string_constant!(
        "NATIVE_STORE_BUILD_CARGO_LOCK_SHA256_V1",
        &capture.cargo_lock_sha256
    );
    {
        output.push_str("pub(crate) const NATIVE_STORE_BUILD_ENABLED_FEATURES_V1: &[&str] = &[");
        for (index, feature) in capture.enabled_features.iter().enumerate() {
            if index != 0 {
                output.push_str(", ");
            }
            output.push_str(&rust_string_literal(feature));
        }
        output.push_str("];\n");
    }
    string_constant!(
        "NATIVE_STORE_BUILD_RUSTC_RELEASE_V1",
        &capture.rustc_release
    );
    string_constant!(
        "NATIVE_STORE_BUILD_RUSTC_COMMIT_HASH_V1",
        &capture.rustc_commit_hash
    );
    string_constant!(
        "NATIVE_STORE_BUILD_RUSTC_COMMIT_DATE_V1",
        &capture.rustc_commit_date
    );
    string_constant!("NATIVE_STORE_BUILD_HOST_TRIPLE_V1", &capture.host_triple);
    string_constant!(
        "NATIVE_STORE_BUILD_TARGET_TRIPLE_V1",
        &capture.target_triple
    );
    string_constant!("NATIVE_STORE_BUILD_LLVM_VERSION_V1", &capture.llvm_version);
    string_constant!(
        "NATIVE_STORE_BUILD_RUSTC_VERBOSE_VERSION_SHA256_V1",
        &capture.rustc_verbose_version_sha256
    );
    string_constant!(
        "NATIVE_STORE_BUILD_RUSTC_VERBOSE_VERSION_LINE_ENDING_V1",
        &capture.rustc_verbose_version_line_ending
    );
    string_constant!("NATIVE_STORE_BUILD_PROFILE_V1", &capture.build_profile);
    string_constant!(
        "NATIVE_STORE_BUILD_SOURCE_GIT_COMMIT_V1",
        &capture.source_git_commit
    );
    string_constant!(
        "NATIVE_STORE_BUILD_SOURCE_TREE_RECIPE_IDENTITY_V1",
        &capture.source_tree_recipe_identity
    );
    string_constant!(
        "NATIVE_STORE_BUILD_SOURCE_TREE_RECIPE_SHA256_V1",
        &capture.source_tree_recipe_sha256
    );
    {
        use fmt::Write as _;
        writeln!(
            &mut output,
            "pub(crate) const NATIVE_STORE_BUILD_SOURCE_TREE_RECIPE_BYTE_COUNT_V1: u64 = {};",
            capture.source_tree_recipe_byte_count
        )
        .expect("writing to String cannot fail");
    }
    string_constant!(
        "NATIVE_STORE_BUILD_SOURCE_TREE_SHA256_V1",
        &capture.source_tree_sha256
    );
    {
        use fmt::Write as _;
        writeln!(
            &mut output,
            "pub(crate) const NATIVE_STORE_BUILD_SOURCE_WORKTREE_CLEAN_V1: bool = {};",
            capture.source_worktree_clean
        )
        .expect("writing to String cannot fail");
    }
    string_constant!(
        "NATIVE_STORE_BUILD_SOURCE_GIT_STATUS_SHA256_V1",
        &capture.source_git_status_sha256
    );
    output
}

fn disabled_generated_source_v1() -> &'static str {
    "// Native Store V2 production build capture is unavailable for this build.\n"
}

#[cfg(not(test))]
fn write_generated_source_v1(out_dir: &Path, source: &str) -> CaptureResultV1<()> {
    fs::write(out_dir.join(GENERATED_FILE_NAME_V1), source)
        .map_err(|_| capture_error("native_store_generated_capture_write_failed"))
}

#[cfg(not(test))]
fn emit_rerun_inputs_v1(crate_manifest_dir: &Path) {
    let repo_root = crate_manifest_dir
        .parent()
        .expect("CARGO_MANIFEST_DIR must have a parent");
    for path in [
        repo_root.join("Cargo.toml"),
        crate_manifest_dir.join("Cargo.toml"),
        repo_root.join("Cargo.lock"),
    ] {
        println!("cargo:rerun-if-changed={}", path.display());
    }
    for name in [
        PRODUCTION_FEATURE_ENV_V1,
        "CARGO_CFG_TARGET_OS",
        "CARGO_CFG_TARGET_ENV",
        "CARGO_CFG_TARGET_ARCH",
        "CARGO_CFG_DEBUG_ASSERTIONS",
        "CARGO_PKG_NAME",
        "CARGO_PKG_VERSION",
        "HOST",
        "TARGET",
        "PROFILE",
        "RUSTC",
    ] {
        println!("cargo:rerun-if-env-changed={name}");
    }
}

fn parse_git_control_line_v1<'a>(
    bytes: &'a [u8],
    prefix: Option<&str>,
    code: &'static str,
) -> CaptureResultV1<&'a str> {
    if bytes.is_empty() || bytes.len() > 4_096 || bytes.contains(&0) {
        return Err(capture_error(code));
    }
    let value = std::str::from_utf8(bytes).map_err(|_| capture_error(code))?;
    let value = value
        .strip_suffix("\r\n")
        .or_else(|| value.strip_suffix('\n'))
        .unwrap_or(value);
    let value = match prefix {
        Some(prefix) => value
            .strip_prefix(prefix)
            .ok_or_else(|| capture_error(code))?,
        None => value,
    };
    if value.is_empty()
        || value.contains('\r')
        || value.contains('\n')
        || value.bytes().any(|byte| byte < 0x20 || byte == 0x7f)
    {
        return Err(capture_error(code));
    }
    Ok(value)
}

fn validate_git_reference_v1(reference: &str) -> CaptureResultV1<()> {
    if !reference.starts_with("refs/")
        || reference.contains('\\')
        || reference
            .split('/')
            .any(|component| component.is_empty() || matches!(component, "." | ".."))
    {
        return Err(capture_error("native_store_git_head_reference_invalid"));
    }
    Ok(())
}

#[cfg(not(test))]
fn canonical_directory_v1(path: &Path, code: &'static str) -> CaptureResultV1<PathBuf> {
    let canonical = fs::canonicalize(path).map_err(|_| capture_error(code))?;
    if !fs::metadata(&canonical)
        .map_err(|_| capture_error(code))?
        .is_dir()
    {
        return Err(capture_error(code));
    }
    Ok(canonical)
}

#[cfg(not(test))]
fn resolve_git_directories_v1(repo_root: &Path) -> CaptureResultV1<(PathBuf, PathBuf)> {
    let marker = repo_root.join(".git");
    let metadata =
        fs::metadata(&marker).map_err(|_| capture_error("native_store_git_marker_missing"))?;
    let git_dir = if metadata.is_dir() {
        canonical_directory_v1(&marker, "native_store_git_directory_invalid")?
    } else if metadata.is_file() {
        let bytes =
            fs::read(&marker).map_err(|_| capture_error("native_store_git_pointer_read_failed"))?;
        let value = parse_git_control_line_v1(
            &bytes,
            Some("gitdir: "),
            "native_store_git_pointer_invalid",
        )?;
        let path = PathBuf::from(value);
        let path = if path.is_absolute() {
            path
        } else {
            repo_root.join(path)
        };
        canonical_directory_v1(&path, "native_store_git_directory_invalid")?
    } else {
        return Err(capture_error("native_store_git_marker_invalid"));
    };

    let common_marker = git_dir.join("commondir");
    let common_dir = if common_marker.exists() {
        let bytes = fs::read(&common_marker)
            .map_err(|_| capture_error("native_store_git_common_pointer_read_failed"))?;
        let value =
            parse_git_control_line_v1(&bytes, None, "native_store_git_common_pointer_invalid")?;
        let path = PathBuf::from(value);
        let path = if path.is_absolute() {
            path
        } else {
            git_dir.join(path)
        };
        canonical_directory_v1(&path, "native_store_git_common_directory_invalid")?
    } else {
        git_dir.clone()
    };
    Ok((git_dir, common_dir))
}

#[cfg(not(test))]
fn emit_git_identity_rerun_inputs_v1(repo_root: &Path) -> CaptureResultV1<()> {
    let marker = repo_root.join(".git");
    let (git_dir, common_dir) = resolve_git_directories_v1(repo_root)?;
    let head_path = git_dir.join("HEAD");
    let head_bytes =
        fs::read(&head_path).map_err(|_| capture_error("native_store_git_head_read_failed"))?;
    let head = parse_git_control_line_v1(&head_bytes, None, "native_store_git_head_invalid")?;

    for path in [
        marker,
        head_path,
        git_dir.join("index"),
        git_dir.join("commondir"),
        common_dir.join("packed-refs"),
    ] {
        println!("cargo:rerun-if-changed={}", path.display());
    }
    if let Some(reference) = head.strip_prefix("ref: ") {
        validate_git_reference_v1(reference)?;
        println!(
            "cargo:rerun-if-changed={}",
            common_dir.join(reference).display()
        );
    } else if !is_lower_hex(head, 40) {
        return Err(capture_error("native_store_git_head_detached_invalid"));
    }
    Ok(())
}

#[cfg(not(test))]
pub(crate) fn configure_native_store_build_capture_v1(crate_manifest_dir: &Path, out_dir: &Path) {
    emit_rerun_inputs_v1(crate_manifest_dir);
    let production_feature_enabled = std::env::var_os(PRODUCTION_FEATURE_ENV_V1).is_some();
    if !production_feature_enabled {
        write_generated_source_v1(out_dir, disabled_generated_source_v1())
            .unwrap_or_else(|error| panic!("{error}"));
        return;
    }
    let target_os = std::env::var_os("CARGO_CFG_TARGET_OS");
    if target_os.as_deref() != Some(OsStr::new("windows")) {
        write_generated_source_v1(out_dir, disabled_generated_source_v1())
            .unwrap_or_else(|error| panic!("{error}"));
        return;
    }
    let repo_root = crate_manifest_dir
        .parent()
        .expect("CARGO_MANIFEST_DIR must have a parent");
    emit_git_identity_rerun_inputs_v1(repo_root).unwrap_or_else(|error| panic!("{error}"));

    #[cfg(not(windows))]
    {
        let _ = crate_manifest_dir;
        panic!("native_store_windows_target_requires_windows_build_host");
    }

    #[cfg(windows)]
    {
        let capture =
            capture_windows_build_v1(crate_manifest_dir).unwrap_or_else(|error| panic!("{error}"));
        let generated = render_capture_constants_v1(&capture);
        write_generated_source_v1(out_dir, &generated).unwrap_or_else(|error| panic!("{error}"));
    }
}

#[cfg(windows)]
mod windows_capture {
    use super::*;
    use std::ffi::c_void;
    use std::fs::File;
    use std::io::{Read, Seek, SeekFrom};
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle, RawHandle};
    use std::process::{Command, Stdio};

    type Handle = *mut c_void;
    type Bool = i32;
    type Dword = u32;

    const INVALID_HANDLE_VALUE: Handle = -1isize as Handle;
    const GENERIC_READ: Dword = 0x8000_0000;
    const FILE_READ_ATTRIBUTES: Dword = 0x0000_0080;
    const FILE_SHARE_READ: Dword = 0x0000_0001;
    const FILE_SHARE_WRITE: Dword = 0x0000_0002;
    const FILE_SHARE_DELETE: Dword = 0x0000_0004;
    const OPEN_EXISTING: Dword = 3;
    const FILE_ATTRIBUTE_DIRECTORY: Dword = 0x0000_0010;
    const FILE_ATTRIBUTE_REPARSE_POINT: Dword = 0x0000_0400;
    const FILE_FLAG_BACKUP_SEMANTICS: Dword = 0x0200_0000;
    const FILE_FLAG_OPEN_REPARSE_POINT: Dword = 0x0020_0000;
    const FILE_STANDARD_INFO_CLASS: Dword = 1;
    const FILE_ATTRIBUTE_TAG_INFO_CLASS: Dword = 9;
    const FILE_ID_INFO_CLASS: Dword = 18;
    const DRIVE_FIXED: Dword = 3;

    #[repr(C)]
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    struct FileIdInfo {
        volume_serial_number: u64,
        file_id: [u8; 16],
    }

    #[repr(C)]
    #[derive(Clone, Copy, Debug)]
    struct FileStandardInfo {
        allocation_size: i64,
        end_of_file: i64,
        number_of_links: u32,
        delete_pending: u8,
        directory: u8,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Debug)]
    struct FileAttributeTagInfo {
        file_attributes: Dword,
        reparse_tag: Dword,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateFileW(
            file_name: *const u16,
            desired_access: Dword,
            share_mode: Dword,
            security_attributes: *mut c_void,
            creation_disposition: Dword,
            flags_and_attributes: Dword,
            template_file: Handle,
        ) -> Handle;
        fn GetFileInformationByHandleEx(
            file: Handle,
            information_class: Dword,
            file_information: *mut c_void,
            buffer_size: Dword,
        ) -> Bool;
        fn GetDriveTypeW(root_path_name: *const u16) -> Dword;
    }

    fn wide_null(value: &OsStr) -> Vec<u16> {
        value.encode_wide().chain(std::iter::once(0)).collect()
    }

    fn query_information<T: Copy>(
        handle: RawHandle,
        information_class: Dword,
    ) -> CaptureResultV1<T> {
        let mut value = std::mem::MaybeUninit::<T>::zeroed();
        let result = unsafe {
            GetFileInformationByHandleEx(
                handle.cast(),
                information_class,
                value.as_mut_ptr().cast(),
                u32::try_from(std::mem::size_of::<T>())
                    .map_err(|_| capture_error("native_store_windows_info_size_overflow"))?,
            )
        };
        if result == 0 {
            return Err(capture_error(
                "native_store_windows_file_information_failed",
            ));
        }
        Ok(unsafe { value.assume_init() })
    }

    fn open_raw_v1(
        path: &Path,
        desired_access: Dword,
        share_mode: Dword,
        flags: Dword,
    ) -> CaptureResultV1<RawHandle> {
        let wide = wide_null(path.as_os_str());
        let handle = unsafe {
            CreateFileW(
                wide.as_ptr(),
                desired_access,
                share_mode,
                std::ptr::null_mut(),
                OPEN_EXISTING,
                flags | FILE_FLAG_OPEN_REPARSE_POINT,
                std::ptr::null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(capture_error("native_store_windows_open_no_follow_failed"));
        }
        Ok(handle.cast())
    }

    fn require_directory_handle_v1(handle: RawHandle) -> CaptureResultV1<()> {
        let attributes: FileAttributeTagInfo =
            query_information(handle, FILE_ATTRIBUTE_TAG_INFO_CLASS)?;
        if attributes.file_attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
            || attributes.file_attributes & FILE_ATTRIBUTE_DIRECTORY == 0
        {
            return Err(capture_error(
                "native_store_windows_parent_not_plain_directory",
            ));
        }
        let standard: FileStandardInfo = query_information(handle, FILE_STANDARD_INFO_CLASS)?;
        if standard.directory == 0 || standard.delete_pending != 0 {
            return Err(capture_error(
                "native_store_windows_parent_directory_state_invalid",
            ));
        }
        let _: FileIdInfo = query_information(handle, FILE_ID_INFO_CLASS)?;
        Ok(())
    }

    fn require_regular_file_handle_v1(handle: RawHandle) -> CaptureResultV1<(FileIdInfo, u64)> {
        let attributes: FileAttributeTagInfo =
            query_information(handle, FILE_ATTRIBUTE_TAG_INFO_CLASS)?;
        if attributes.file_attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
            || attributes.file_attributes & FILE_ATTRIBUTE_DIRECTORY != 0
        {
            return Err(capture_error(
                "native_store_windows_leaf_not_plain_regular_file",
            ));
        }
        let standard: FileStandardInfo = query_information(handle, FILE_STANDARD_INFO_CLASS)?;
        if standard.directory != 0 || standard.delete_pending != 0 || standard.end_of_file < 0 {
            return Err(capture_error(
                "native_store_windows_leaf_file_state_invalid",
            ));
        }
        let identity: FileIdInfo = query_information(handle, FILE_ID_INFO_CLASS)?;
        let length = u64::try_from(standard.end_of_file)
            .map_err(|_| capture_error("native_store_windows_leaf_length_invalid"))?;
        Ok((identity, length))
    }

    fn drive_root_from_windows_path_v1(path: &str) -> PathBuf {
        PathBuf::from(format!("{}:\\", path.as_bytes()[0] as char))
    }

    fn require_fixed_drive_v1(path: &str) -> CaptureResultV1<()> {
        let root = drive_root_from_windows_path_v1(path);
        let wide = wide_null(root.as_os_str());
        let drive_type = unsafe { GetDriveTypeW(wide.as_ptr()) };
        if drive_type != DRIVE_FIXED {
            return Err(capture_error("native_store_rustc_drive_not_fixed"));
        }
        Ok(())
    }

    fn open_parent_chain_v1(path: &Path) -> CaptureResultV1<Vec<OwnedHandle>> {
        let parent = path
            .parent()
            .ok_or_else(|| capture_error("native_store_windows_leaf_parent_missing"))?;
        let mut ancestors = parent.ancestors().collect::<Vec<_>>();
        ancestors.reverse();
        let mut handles = Vec::new();
        for ancestor in ancestors {
            let raw = open_raw_v1(
                ancestor,
                FILE_READ_ATTRIBUTES,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                FILE_FLAG_BACKUP_SEMANTICS,
            )?;
            let owned = unsafe { OwnedHandle::from_raw_handle(raw) };
            require_directory_handle_v1(owned.as_raw_handle())?;
            handles.push(owned);
        }
        Ok(handles)
    }

    fn open_regular_file_v1(path: &Path) -> CaptureResultV1<(File, FileIdInfo, u64)> {
        let raw = open_raw_v1(path, GENERIC_READ, FILE_SHARE_READ, 0)?;
        let file = unsafe { File::from_raw_handle(raw) };
        let (identity, length) = require_regular_file_handle_v1(file.as_raw_handle())?;
        Ok((file, identity, length))
    }

    fn read_held_file_v1(file: &mut File, expected_length: u64) -> CaptureResultV1<Vec<u8>> {
        let capacity = usize::try_from(expected_length)
            .map_err(|_| capture_error("native_store_windows_file_too_large"))?;
        file.seek(SeekFrom::Start(0))
            .map_err(|_| capture_error("native_store_windows_file_seek_failed"))?;
        let mut bytes = Vec::with_capacity(capacity);
        file.read_to_end(&mut bytes)
            .map_err(|_| capture_error("native_store_windows_file_read_failed"))?;
        if bytes.len() != capacity {
            return Err(capture_error("native_store_windows_file_length_drift"));
        }
        Ok(bytes)
    }

    #[derive(Debug)]
    struct StableFileCaptureV1 {
        bytes: Vec<u8>,
        sha256: String,
    }

    fn stable_file_capture_v1(path: &Path) -> CaptureResultV1<StableFileCaptureV1> {
        let _parents = open_parent_chain_v1(path)?;
        let (mut primary, primary_identity, primary_length) = open_regular_file_v1(path)?;
        let primary_bytes = read_held_file_v1(&mut primary, primary_length)?;
        let primary_sha256 = sha256_hex(&primary_bytes);

        let (mut reopened, reopened_identity, reopened_length) = open_regular_file_v1(path)?;
        if reopened_identity != primary_identity || reopened_length != primary_length {
            return Err(capture_error(
                "native_store_windows_file_reopen_identity_drift",
            ));
        }
        let reopened_bytes = read_held_file_v1(&mut reopened, reopened_length)?;
        let reopened_sha256 = sha256_hex(&reopened_bytes);
        if reopened_sha256 != primary_sha256 || reopened_bytes != primary_bytes {
            return Err(capture_error(
                "native_store_windows_file_reopen_digest_drift",
            ));
        }
        Ok(StableFileCaptureV1 {
            bytes: primary_bytes,
            sha256: primary_sha256,
        })
    }

    fn capture_rustc_v1(
        environment: &[(OsString, OsString)],
    ) -> CaptureResultV1<RustcVerboseVersionV1> {
        let rustc = required_environment_unicode_v1(environment, "RUSTC")?;
        validate_drive_absolute_windows_path_v1(&rustc)?;
        require_fixed_drive_v1(&rustc)?;
        let rustc_path = PathBuf::from(&rustc);
        let _parents = open_parent_chain_v1(&rustc_path)?;
        let (_primary, identity_before, _) = open_regular_file_v1(&rustc_path)?;

        let output = Command::new(&rustc_path)
            .arg("-vV")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|_| capture_error("native_store_rustc_child_creation_failed"))?;
        if output.status.code() != Some(0) {
            return Err(capture_error("native_store_rustc_child_exit_invalid"));
        }
        if !output.stderr.is_empty() {
            return Err(capture_error("native_store_rustc_stderr_not_empty"));
        }

        let (_reopened, identity_after, _) = open_regular_file_v1(&rustc_path)?;
        if identity_after != identity_before {
            return Err(capture_error("native_store_rustc_reopen_identity_drift"));
        }
        parse_rustc_verbose_version_v1(&output.stdout)
    }

    #[cfg(not(test))]
    pub(super) fn capture_windows_build_v1(
        crate_manifest_dir: &Path,
    ) -> CaptureResultV1<BuildCaptureV1> {
        if !crate_manifest_dir.is_absolute()
            || crate_manifest_dir.file_name() != Some(OsStr::new(PACKAGE_NAME_V1))
            || crate_manifest_dir.components().any(|component| {
                matches!(
                    component,
                    std::path::Component::CurDir | std::path::Component::ParentDir
                )
            })
        {
            return Err(capture_error(
                "native_store_cargo_manifest_dir_crate_name_invalid",
            ));
        }
        let repo_root = crate_manifest_dir
            .parent()
            .ok_or_else(|| capture_error("native_store_repository_root_missing"))?;

        let workspace_manifest = stable_file_capture_v1(&repo_root.join("Cargo.toml"))?;
        let crate_manifest = stable_file_capture_v1(&crate_manifest_dir.join("Cargo.toml"))?;
        let cargo_lock = stable_file_capture_v1(&repo_root.join("Cargo.lock"))?;
        let manifest = parse_crate_manifest_v1(&crate_manifest.bytes)?;
        let environment: Vec<(OsString, OsString)> = std::env::vars_os().collect();
        let enabled_features = enabled_features_from_environment_v1(&manifest, &environment)?;
        let package_name = required_environment_unicode_v1(&environment, "CARGO_PKG_NAME")?;
        let package_version = required_environment_unicode_v1(&environment, "CARGO_PKG_VERSION")?;
        if package_name != manifest.package_name || package_name != PACKAGE_NAME_V1 {
            return Err(capture_error("native_store_cargo_package_name_mismatch"));
        }
        if package_version != manifest.package_version
            || !is_nonempty_printable_ascii(&package_version)
            || Version::parse(&package_version).is_err()
        {
            return Err(capture_error("native_store_cargo_package_version_mismatch"));
        }

        let admission = validate_windows_admission_v1(&environment, cfg!(debug_assertions))?;
        let rustc = capture_rustc_v1(&environment)?;
        if rustc.host_triple != admission.host_triple {
            return Err(capture_error(
                "native_store_rustc_host_environment_mismatch",
            ));
        }

        let source =
            crate::strict_source_tree_attestation_v1::capture_strict_source_tree_v1(repo_root)
                .map_err(|_| capture_error("native_store_strict_source_capture_failed"))?;
        if !source.worktree_clean() || source.git_status_sha256() != EMPTY_SHA256_V1 {
            return Err(capture_error("native_store_strict_source_worktree_dirty"));
        }

        Ok(BuildCaptureV1 {
            package_name,
            package_version,
            workspace_manifest_sha256: workspace_manifest.sha256,
            crate_manifest_sha256: crate_manifest.sha256,
            cargo_lock_sha256: cargo_lock.sha256,
            enabled_features,
            rustc_release: rustc.rustc_release,
            rustc_commit_hash: rustc.rustc_commit_hash,
            rustc_commit_date: rustc.rustc_commit_date,
            host_triple: rustc.host_triple,
            target_triple: admission.target_triple,
            llvm_version: rustc.llvm_version,
            rustc_verbose_version_sha256: rustc.raw_sha256,
            rustc_verbose_version_line_ending: rustc.line_ending.to_owned(),
            build_profile: admission.build_profile,
            source_git_commit: source.git_commit().to_owned(),
            source_tree_recipe_identity: source.source_tree_recipe_identity().to_owned(),
            source_tree_recipe_sha256: source.source_tree_recipe_sha256().to_owned(),
            source_tree_recipe_byte_count:
                crate::strict_source_tree_attestation_v1::STRICT_SOURCE_TREE_RECIPE_BYTE_COUNT_V1,
            source_tree_sha256: source.source_tree_sha256().to_owned(),
            source_worktree_clean: source.worktree_clean(),
            source_git_status_sha256: source.git_status_sha256().to_owned(),
        })
    }

    #[cfg(test)]
    pub(super) fn stable_file_capture_for_test_v1(
        path: &Path,
    ) -> CaptureResultV1<(Vec<u8>, String)> {
        let capture = stable_file_capture_v1(path)?;
        Ok((capture.bytes, capture.sha256))
    }
}

#[cfg(all(windows, not(test)))]
use windows_capture::capture_windows_build_v1;

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_manifest() -> &'static [u8] {
        br#"[package]
name = "mtg-kernel"
version = "1.2.3-alpha.1+build.7"

[features]
alpha = []
native-training-store-v2-production = []
z-last = []
"#
    }

    fn rustc_verbose(line_ending: &str) -> Vec<u8> {
        [
            "rustc 1.94.1 (abcdef012 2026-07-01)",
            "binary: rustc",
            "commit-hash: abcdef0123456789abcdef0123456789abcdef01",
            "commit-date: 2026-07-01",
            "host: x86_64-pc-windows-msvc",
            "release: 1.94.1",
            "LLVM version: 21.1.8",
        ]
        .join(line_ending)
        .as_bytes()
        .iter()
        .copied()
        .chain(line_ending.as_bytes().iter().copied())
        .collect()
    }

    fn os_environment(values: &[(&str, &str)]) -> Vec<(OsString, OsString)> {
        values
            .iter()
            .map(|(name, value)| (OsString::from(name), OsString::from(value)))
            .collect()
    }

    #[test]
    fn manifest_parser_preserves_exact_features_and_semver() {
        let parsed = parse_crate_manifest_v1(valid_manifest()).expect("manifest should parse");
        assert_eq!(parsed.package_name, "mtg-kernel");
        assert_eq!(parsed.package_version, "1.2.3-alpha.1+build.7");
        assert_eq!(
            parsed
                .feature_by_normalized_env_suffix
                .get("NATIVE_TRAINING_STORE_V2_PRODUCTION")
                .map(String::as_str),
            Some("native-training-store-v2-production")
        );
    }

    #[test]
    fn manifest_parser_rejects_feature_normalization_collision() {
        let raw = br#"[package]
name = "mtg-kernel"
version = "1.0.0"
[features]
native-training-store-v2-production = []
a-b = []
a_b = []
"#;
        let error = parse_crate_manifest_v1(raw).expect_err("collision must fail");
        assert_eq!(
            error.0,
            "native_store_crate_manifest_feature_normalization_collision"
        );
    }

    #[test]
    fn enabled_features_are_exact_sorted_and_value_checked() {
        let parsed = parse_crate_manifest_v1(valid_manifest()).expect("manifest should parse");
        let environment = os_environment(&[
            ("CARGO_FEATURE_Z_LAST", "1"),
            ("CARGO_FEATURE_NATIVE_TRAINING_STORE_V2_PRODUCTION", "1"),
            ("CARGO_FEATURE_ALPHA", "1"),
        ]);
        assert_eq!(
            enabled_features_from_environment_v1(&parsed, &environment)
                .expect("features should parse"),
            vec![
                "alpha".to_owned(),
                "native-training-store-v2-production".to_owned(),
                "z-last".to_owned()
            ]
        );

        let bad_value =
            os_environment(&[("CARGO_FEATURE_NATIVE_TRAINING_STORE_V2_PRODUCTION", "true")]);
        assert_eq!(
            enabled_features_from_environment_v1(&parsed, &bad_value)
                .expect_err("non-one feature value must fail")
                .0,
            "native_store_cargo_feature_value_invalid"
        );
        let unknown = os_environment(&[
            ("CARGO_FEATURE_NATIVE_TRAINING_STORE_V2_PRODUCTION", "1"),
            ("CARGO_FEATURE_UNKNOWN", "1"),
        ]);
        assert_eq!(
            enabled_features_from_environment_v1(&parsed, &unknown)
                .expect_err("unknown feature must fail")
                .0,
            "native_store_cargo_feature_unknown"
        );
    }

    #[test]
    fn rustc_verbose_parser_accepts_lf_and_crlf_without_normalizing_hash() {
        let lf_raw = rustc_verbose("\n");
        let crlf_raw = rustc_verbose("\r\n");
        let lf = parse_rustc_verbose_version_v1(&lf_raw).expect("LF should parse");
        let crlf = parse_rustc_verbose_version_v1(&crlf_raw).expect("CRLF should parse");
        assert_eq!(lf.line_ending, "lf");
        assert_eq!(crlf.line_ending, "crlf");
        assert_eq!(lf.host_triple, ADMITTED_X86_64_TRIPLE_V1);
        assert_ne!(lf.raw_sha256, crlf.raw_sha256);
        assert_eq!(lf.raw_sha256, sha256_hex(&lf_raw));
        assert_eq!(crlf.raw_sha256, sha256_hex(&crlf_raw));
    }

    #[test]
    fn rustc_verbose_parser_rejects_mixed_missing_duplicate_and_malformed_fields() {
        let mut mixed = rustc_verbose("\n");
        mixed.splice(0..0, b"unknown: value\r\n".iter().copied());
        assert_eq!(
            parse_rustc_verbose_version_v1(&mixed)
                .expect_err("mixed line endings must fail")
                .0,
            "native_store_rustc_stdout_mixed_line_endings"
        );

        let duplicate = String::from_utf8(rustc_verbose("\n")).expect("fixture utf8")
            + "commit-hash: abcdef0123456789abcdef0123456789abcdef01\n";
        assert_eq!(
            parse_rustc_verbose_version_v1(duplicate.as_bytes())
                .expect_err("duplicate required field must fail")
                .0,
            "native_store_rustc_required_line_duplicate"
        );

        let missing = String::from_utf8(rustc_verbose("\n"))
            .expect("fixture utf8")
            .replace("LLVM version: 21.1.8\n", "");
        assert_eq!(
            parse_rustc_verbose_version_v1(missing.as_bytes())
                .expect_err("missing required field must fail")
                .0,
            "native_store_rustc_required_line_missing"
        );

        let bad_date = String::from_utf8(rustc_verbose("\n"))
            .expect("fixture utf8")
            .replace("2026-07-01", "2026-02-30");
        assert_eq!(
            parse_rustc_verbose_version_v1(bad_date.as_bytes())
                .expect_err("invalid calendar date must fail")
                .0,
            "native_store_rustc_commit_date_invalid"
        );
    }

    #[test]
    fn windows_path_grammar_rejects_resolution_and_reparse_spellings() {
        assert!(validate_drive_absolute_windows_path_v1(
            r"C:\Users\Jack\.rustup\toolchains\stable\bin\rustc.exe"
        )
        .is_ok());
        for invalid in [
            "rustc.exe",
            r"C:rustc.exe",
            r"\\server\share\rustc.exe",
            r"\\?\C:\rustc.exe",
            r"\\.\C:\rustc.exe",
            r"C:/rustc.exe",
            r"C:\bin\rustc.exe:stream",
            r"%RUSTUP_HOME%\bin\rustc.exe",
            r"C:\bin\..\rustc.exe",
            r"C:\bin\\rustc.exe",
        ] {
            assert!(
                validate_drive_absolute_windows_path_v1(invalid).is_err(),
                "invalid path unexpectedly admitted: {invalid:?}"
            );
        }
    }

    #[test]
    fn windows_admission_requires_release_msvc_and_matching_arch() {
        let environment = os_environment(&[
            ("CARGO_CFG_TARGET_OS", "windows"),
            ("CARGO_CFG_TARGET_ENV", "msvc"),
            ("CARGO_CFG_TARGET_ARCH", "x86_64"),
            ("HOST", ADMITTED_X86_64_TRIPLE_V1),
            ("TARGET", ADMITTED_X86_64_TRIPLE_V1),
            ("PROFILE", "release"),
        ]);
        assert_eq!(
            validate_windows_admission_v1(&environment, false)
                .expect("valid admission should pass")
                .target_triple,
            ADMITTED_X86_64_TRIPLE_V1
        );
        assert_eq!(
            validate_windows_admission_v1(&environment, true)
                .expect_err("debug assertions must fail")
                .0,
            "native_store_debug_assertions_not_admitted"
        );
        let wrong_arch = environment
            .iter()
            .map(|(name, value)| {
                if name == OsStr::new("CARGO_CFG_TARGET_ARCH") {
                    (name.clone(), OsString::from("aarch64"))
                } else {
                    (name.clone(), value.clone())
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(
            validate_windows_admission_v1(&wrong_arch, false)
                .expect_err("wrong target arch must fail")
                .0,
            "native_store_target_arch_mismatch"
        );

        let cross_arch = environment
            .iter()
            .map(|(name, value)| {
                if name == OsStr::new("TARGET") {
                    (name.clone(), OsString::from(ADMITTED_AARCH64_TRIPLE_V1))
                } else if name == OsStr::new("CARGO_CFG_TARGET_ARCH") {
                    (name.clone(), OsString::from("aarch64"))
                } else {
                    (name.clone(), value.clone())
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(
            validate_windows_admission_v1(&cross_arch, false)
                .expect("admitted host-to-target cross compilation should pass")
                .target_triple,
            ADMITTED_AARCH64_TRIPLE_V1
        );

        for (name, value, expected_error) in [
            (
                "CARGO_CFG_TARGET_ENV",
                "gnu",
                "native_store_windows_abi_not_admitted",
            ),
            ("PROFILE", "dev", "native_store_build_profile_not_release"),
        ] {
            let invalid = environment
                .iter()
                .map(|(existing_name, existing_value)| {
                    if existing_name == OsStr::new(name) {
                        (existing_name.clone(), OsString::from(value))
                    } else {
                        (existing_name.clone(), existing_value.clone())
                    }
                })
                .collect::<Vec<_>>();
            assert_eq!(
                validate_windows_admission_v1(&invalid, false)
                    .expect_err("invalid admission field must fail")
                    .0,
                expected_error
            );
        }
    }

    #[test]
    fn disabled_generated_source_contains_no_capture_constants() {
        let source = disabled_generated_source_v1();
        assert!(!source.contains("pub(crate) const"));
        assert!(!source.contains("NATIVE_STORE_BUILD_"));
    }

    #[test]
    fn git_control_lines_and_head_references_fail_closed() {
        assert_eq!(
            parse_git_control_line_v1(
                b"gitdir: C:\\repo\\.git\\worktrees\\capture\n",
                Some("gitdir: "),
                "invalid",
            )
            .unwrap(),
            "C:\\repo\\.git\\worktrees\\capture"
        );
        assert_eq!(
            parse_git_control_line_v1(b"../..\r\n", None, "invalid").unwrap(),
            "../.."
        );
        for invalid in [
            b"".as_slice(),
            b"gitdir: \n".as_slice(),
            b"gitdir: first\nsecond\n".as_slice(),
            b"gitdir: first\0second\n".as_slice(),
        ] {
            assert!(parse_git_control_line_v1(invalid, Some("gitdir: "), "invalid").is_err());
        }

        assert!(validate_git_reference_v1("refs/heads/capture").is_ok());
        for invalid in [
            "heads/capture",
            "refs//capture",
            "refs/heads/../capture",
            "refs\\heads\\capture",
        ] {
            assert!(validate_git_reference_v1(invalid).is_err());
        }
    }

    #[test]
    fn generated_constants_match_the_pinned_path_free_interface() {
        let capture = BuildCaptureV1 {
            package_name: "mtg-kernel".to_owned(),
            package_version: "1.2.3".to_owned(),
            workspace_manifest_sha256: "a".repeat(64),
            crate_manifest_sha256: "b".repeat(64),
            cargo_lock_sha256: "c".repeat(64),
            enabled_features: vec![PRODUCTION_FEATURE_NAME_V1.to_owned()],
            rustc_release: "1.94.1".to_owned(),
            rustc_commit_hash: "d".repeat(40),
            rustc_commit_date: "2026-07-01".to_owned(),
            host_triple: ADMITTED_X86_64_TRIPLE_V1.to_owned(),
            target_triple: ADMITTED_X86_64_TRIPLE_V1.to_owned(),
            llvm_version: "21.1.8".to_owned(),
            rustc_verbose_version_sha256: "e".repeat(64),
            rustc_verbose_version_line_ending: "lf".to_owned(),
            build_profile: "release".to_owned(),
            source_git_commit: "f".repeat(40),
            source_tree_recipe_identity: "recipe".to_owned(),
            source_tree_recipe_sha256: "1".repeat(64),
            source_tree_recipe_byte_count: 5_847,
            source_tree_sha256: "2".repeat(64),
            source_worktree_clean: true,
            source_git_status_sha256: EMPTY_SHA256_V1.to_owned(),
        };
        let generated = render_capture_constants_v1(&capture);
        for name in [
            "NATIVE_STORE_BUILD_PACKAGE_NAME_V1",
            "NATIVE_STORE_BUILD_PACKAGE_VERSION_V1",
            "NATIVE_STORE_BUILD_WORKSPACE_MANIFEST_SHA256_V1",
            "NATIVE_STORE_BUILD_CRATE_MANIFEST_SHA256_V1",
            "NATIVE_STORE_BUILD_CARGO_LOCK_SHA256_V1",
            "NATIVE_STORE_BUILD_ENABLED_FEATURES_V1",
            "NATIVE_STORE_BUILD_RUSTC_RELEASE_V1",
            "NATIVE_STORE_BUILD_RUSTC_COMMIT_HASH_V1",
            "NATIVE_STORE_BUILD_RUSTC_COMMIT_DATE_V1",
            "NATIVE_STORE_BUILD_HOST_TRIPLE_V1",
            "NATIVE_STORE_BUILD_TARGET_TRIPLE_V1",
            "NATIVE_STORE_BUILD_LLVM_VERSION_V1",
            "NATIVE_STORE_BUILD_RUSTC_VERBOSE_VERSION_SHA256_V1",
            "NATIVE_STORE_BUILD_RUSTC_VERBOSE_VERSION_LINE_ENDING_V1",
            "NATIVE_STORE_BUILD_PROFILE_V1",
            "NATIVE_STORE_BUILD_SOURCE_GIT_COMMIT_V1",
            "NATIVE_STORE_BUILD_SOURCE_TREE_RECIPE_IDENTITY_V1",
            "NATIVE_STORE_BUILD_SOURCE_TREE_RECIPE_SHA256_V1",
            "NATIVE_STORE_BUILD_SOURCE_TREE_RECIPE_BYTE_COUNT_V1",
            "NATIVE_STORE_BUILD_SOURCE_TREE_SHA256_V1",
            "NATIVE_STORE_BUILD_SOURCE_WORKTREE_CLEAN_V1",
            "NATIVE_STORE_BUILD_SOURCE_GIT_STATUS_SHA256_V1",
        ] {
            assert!(
                generated.contains(name),
                "missing generated constant {name}"
            );
        }
        assert!(!generated.contains("C:\\"));
        assert!(!generated.contains("E:\\"));
        assert!(!generated.contains("/Users/"));
    }

    #[cfg(windows)]
    #[test]
    fn windows_stable_file_capture_hashes_held_bytes() {
        use std::time::{SystemTime, UNIX_EPOCH};

        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time after epoch")
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "mtg-kernel-native-store-build-capture-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&directory).expect("create isolated test directory");
        let path = directory.join("Cargo.toml");
        let bytes = b"[workspace]\nresolver = \"2\"\n";
        fs::write(&path, bytes).expect("write fixture");
        let (captured, digest) = windows_capture::stable_file_capture_for_test_v1(&path)
            .expect("stable file capture should pass");
        assert_eq!(captured, bytes);
        assert_eq!(digest, sha256_hex(bytes));
        fs::remove_file(&path).expect("remove fixture");
        fs::remove_dir(&directory).expect("remove isolated test directory");
    }

    #[cfg(windows)]
    #[test]
    fn windows_stable_file_capture_rejects_a_leaf_symlink_when_supported() {
        use std::os::windows::fs::symlink_file;
        use std::time::{SystemTime, UNIX_EPOCH};

        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time after epoch")
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "mtg-kernel-native-store-build-capture-symlink-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&directory).expect("create isolated test directory");
        let target = directory.join("real-Cargo.toml");
        let link = directory.join("Cargo.toml");
        fs::write(&target, b"[workspace]\n").expect("write fixture");
        match symlink_file(&target, &link) {
            Ok(()) => {
                assert_eq!(
                    windows_capture::stable_file_capture_for_test_v1(&link)
                        .expect_err("a no-follow leaf symlink must fail")
                        .0,
                    "native_store_windows_leaf_not_plain_regular_file"
                );
                fs::remove_file(&link).expect("remove symlink fixture");
            }
            Err(error) if error.raw_os_error() == Some(1314) => {}
            Err(error) => panic!("unexpected symlink creation failure: {error}"),
        }
        fs::remove_file(&target).expect("remove target fixture");
        fs::remove_dir(&directory).expect("remove isolated test directory");
    }
}
