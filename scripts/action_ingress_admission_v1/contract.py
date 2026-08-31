"""Frozen constants and fail-closed helpers for action-ingress admission v1."""

from __future__ import annotations

import dataclasses
from decimal import Decimal
import hashlib
import json
import math
import ntpath
import os
from pathlib import Path
import re
import shutil
import subprocess
from typing import Any, Iterable, Mapping, NoReturn


LABEL = "ACTION-INGRESS-ADMISSION-DIAGNOSTIC-NON-EVIDENCE"
MANIFEST_RELATIVE_PATH = Path("docs/contracts/ACTION_INGRESS_ADMISSION_V1.md")
ARTIFACT_ROOT_WINDOWS = r"D:\mtg-kernel-action-ingress-admission-v1-20260726"
TARGET_DIR_WINDOWS = r"E:\cargo-target-action-ingress-admission-v1"
WORKTREE_WINDOWS = (
    r"C:\Users\Jack\IdeaProjects\mtg-kernel-observation-diagnostics-codex"
)
LINUX_REPO = "/mnt/c/Users/Jack/IdeaProjects/mtg-kernel-observation-diagnostics-codex"
BRANCH = "codex/observation-diagnostics-v1"

BUILD_RECEIPT_SCHEMA = "mtg-kernel-action-ingress-admission-build-receipt/v1"
INVOCATION_RECEIPT_SCHEMA = (
    "mtg-kernel-action-ingress-admission-invocation-receipt/v1"
)
COMPLETION_RECEIPT_SCHEMA = (
    "mtg-kernel-action-ingress-admission-completion-receipt/v1"
)
CLASSIFICATION_SCHEMA = (
    "mtg-kernel-action-ingress-admission-classification/v1"
)
CLASSIFICATION_RECEIPT_SCHEMA = (
    "mtg-kernel-action-ingress-admission-classification-receipt/v1"
)
PROBE_ENVELOPE_SCHEMA = "mtg-kernel-action-ingress-admission-envelope/v1"
PROBE_PAYLOAD_SCHEMA = "mtg-kernel-action-ingress-admission-payload/v1"

# Producer-facing names are deliberately centralized.  The Rust producer and
# static producer are independently reviewed; changing either interface must
# change this package and therefore the implementation commit.
TEST_MODULE = (
    "native_checkpoint_inference_v1::checkpoint_reliance_probe_v1::"
    "action_ingress_admission_v1"
)
REQUIRED_TESTS = (
    f"{TEST_MODULE}::slot69_repair_and_fixed_digest_gates_are_exact_and_fail_closed_v1",
    f"{TEST_MODULE}::digest_zero_stress_and_non_digest_ingress_controls_are_exact_v1",
    (
        f"{TEST_MODULE}::"
        "supplemental_player_target_cross_runtime_tensorization_is_bit_exact_v1"
    ),
    f"{TEST_MODULE}::official_action_ingress_admission_probe_v1",
)
PROBE_TEST = REQUIRED_TESTS[-1]

STATIC_TOOL_RELATIVE_PATH = Path(
    "python/tools/audit_action_ingress_static_admission_v1.py"
)
STATIC_TEST_RELATIVE_PATH = Path(
    "python/tests/test_action_ingress_static_admission_v1.py"
)
STATIC_REPORT_RELATIVE_PATH = Path(
    "data/action_ingress_admission_v1/static-admission.json"
)
STATIC_REPORT_SCHEMA = "mtg-kernel-action-ingress-static-admission/v1"
STATIC_POSITIVE_MARKER = "ACTION_INGRESS_STATIC_ADMISSION_V1_OK"
STATIC_REQUIRED_TEST_COUNT = 7
PACKAGING_REQUIRED_TEST_COUNT = 26
STATIC_TOOL_SHA256 = (
    "3f2d8f5a1960c417953d5365fc092aaf8ebdcea28805e2b31665cae2d8cf519a"
)
STATIC_TEST_SHA256 = (
    "0e0cde4981b76819e54c07f1e28e6f5a28702901f6d1a44f41168f6d17d15734"
)
STATIC_REPORT_SHA256 = (
    "4b4f0a6c9a28dd98b7a482fe23ad44a0b6e072619343ae01c68e9f335f99e78d"
)
STATIC_REPORT_PAYLOAD_SHA256 = (
    "65f171c88260f2397614d947809b1e54f196d52a6df1c0b7d0faa57bbabf47aa"
)
STATIC_COMBINED_IDENTITY = "net8-action-ingress-static-checked-116-v1"
STATIC_COMBINED_IDENTITY_SHA256 = (
    "7a8cc702393253fdb2dfe61bcca648cbc8684cd1527e0d30a37b242baeb20a6e"
)
STATIC_CASE_DIGEST_CONTRACT = (
    "sha256(canonical-json(case_identity_rows sorted lexicographically by name))"
)

CORPUS_IDENTITY = (
    "rally-mirror-splitmix64-modulo-fixed-256-post-tensorization-v1"
)
CORPUS_SHA256 = (
    "72103ea367a662f76675a044ad4efcf4c52bf86d32630df88e5247cf79f5e5e0"
)
CORPUS_ACTION_REFERENCE_COUNT = 978
OUTPUT_DIGEST_IDENTITY = (
    "sha256-framed-role-condition-decision-logit-value-f32le-v1"
)
TRANSFORM_IDENTITY = (
    "net8-action-kind-conditioned-boolean-slot69-counterfactual-v1"
)
GATE_IDENTITY = "net8-fixed-action-digest-gate-f32-v1"
COMMON_SNAPSHOT_PARAMETER_STREAM_SHA256 = (
    "36157c71b9fd736d4913e6c5722dcb9c1e4f119b7b28b108bde9d74f18862d54"
)
COMMON_SNAPSHOT_SEED = 6_443_515_232_517_447_393
FEATURES_SHA256 = (
    "fce419176dbd15e2b911e5c5f688bb390e731e3817da142571f38b1a7cc778eb"
)
MODEL_SOURCE_SHA256 = (
    "2e3e830d4212b8c8f8085861b2508c49a6d7192b9621cef087dd396e22d12c59"
)
COMMON_MANIFEST_SHA256 = (
    "d5d296f5d4ee1f7e40a6005f1e1dd328b2885f6b95f0c6968c6bf1b87351c7cc"
)
COMMON_PARAMETERS_SHA256 = (
    "79f715b11ccce80ac66cc832bfdc0c963a8a20f27f7b492fdfbb433c008a90a5"
)
ACTION_GOLDEN_SHA256 = (
    "6fab4b246b052e6b8404520d945b630e24ce60323a8fa4c6e78fdc17d3f9a3b8"
)

FROZEN_INPUTS = {
    "docs/reports/OBSERVATION_DIAGNOSTICS_RESULT_V1.md": (
        "a728aafcba53f42b9d78f7f5db468c5fe0dc87c325168cacec94926aa9ff63f3"
    ),
    "rust-toolchain.toml": (
        "d52c5633ea77aefd345519d0a6c87e19c2636a1e90178585c30db481b3de9de0"
    ),
    "python/mtg_kernel_rl/features.py": FEATURES_SHA256,
    "python/mtg_kernel_rl/model.py": MODEL_SOURCE_SHA256,
    "data/common_model_snapshot_v1/manifest.json": COMMON_MANIFEST_SHA256,
    "data/common_model_snapshot_v1/parameters.f32le": COMMON_PARAMETERS_SHA256,
    "data/flat_policy_v2/python_action_features_v2.json": ACTION_GOLDEN_SHA256,
}
IMPLEMENTATION_EXTRA_PATHS = (
    Path(
        "mtg-kernel/src/native_checkpoint_inference_v1/"
        "checkpoint_reliance_probe_v1.rs"
    ),
    Path(
        "mtg-kernel/src/native_checkpoint_inference_v1/"
        "checkpoint_reliance_probe_v1/action_ingress_admission_v1.rs"
    ),
    Path("mtg-kernel/src/native_flat_tensorizer_v2.rs"),
    Path("mtg-kernel/src/native_policy_value_net_v1.rs"),
    Path("mtg-kernel/src/rl_session.rs"),
    STATIC_TOOL_RELATIVE_PATH,
    STATIC_TEST_RELATIVE_PATH,
    STATIC_REPORT_RELATIVE_PATH,
)


@dataclasses.dataclass(frozen=True)
class ModelSpec:
    ordinal: int
    identity: str
    kind: str
    store_root: str | None
    model_parameter_sha256: str | None
    baseline_output_sha256: str | None
    provenance: Mapping[str, Any] | None

    @property
    def run_name(self) -> str:
        return f"{self.ordinal:02d}-{self.identity}"


MIRROR_PROVENANCE = {
    "generation_index": 0,
    "run_sha256": "0b46f9507caede181e745da51dabbb6c9f73d72d3eb2315f089ef248c60e2f80",
    "identity_bundle_sha256": "3b3e4e2270d307e7984314b91be69f1ccad0ec171d3210e3048a7ba2eb747024",
    "segment_ordinal": 0,
    "segment_manifest_sha256": "54c1d3cc527bc339f55734a47c660b2f5078b291a9d2d4b0cdfd36eeeaa8ec5e",
    "parent_boundary_head_sha256": None,
    "boundary_head_sha256": "659a9e4cd250cf1f38a678d3632d1ce6ae1fd6aa7d7bc02918bb6e0d4762cfd2",
    "boundary_head_record_sha256": "0b45da9663aed2f56460c85693122dae267b9a7f782152023dcbe02f2fa3d64e",
    "checkpoint_manifest_sha256": "fb780bfb8c5de8f88a9a1254108c7f45f7a90dba75f8ef614c8103681c7127a1",
    "checkpoint_payload_sha256": "2a0840425ccfd09df56747d016d8fcd6b5bc19bba09b6f8cbcdc4507b7315095",
    "checkpoint_sidecar_sha256": "a6a6c1934f388ff0e212bb15a5f43f7fd6a03dc9ec1dff91acfe762a4a72b62f",
    "logical_state_sha256": "f46efcc86d9cc6ad2aec8bcc13e02560d1cd3bc3da166bb9a9e7054430dba18a",
    "train_state_sha256": "0b35c448201efe92375f48a22201c432d3272a3286fae1440f6e7aa2277b9de5",
    "last_update_evidence_sha256": None,
    "adam_step": 0,
}
DIVERGED_PROVENANCE = {
    "generation_index": 0,
    "run_sha256": "fee86543272b4f709be46bb7f9eec820d979d264a93b606408e07c9a6871e51f",
    "identity_bundle_sha256": "27c1c4798f8eb4a396e1952d055cb04122ce44d24fc8ff98118787ae0cb0985c",
    "segment_ordinal": 0,
    "segment_manifest_sha256": "9957484508c494032526b91a3226c8b30e3e82d5a50e4070479b74e5fda4a5b5",
    "parent_boundary_head_sha256": None,
    "boundary_head_sha256": "142fe85ace4c0b8e4d006b2d424c5f65604375eb0f64856395e87b783d648a13",
    "boundary_head_record_sha256": "42360bb84f74a995be98f473181601baedd22ff238012fed4222d1790d11c456",
    "checkpoint_manifest_sha256": "2503dc79396fd9cf22e2771324e13b246f686de89503e497680d50091a4fbd99",
    "checkpoint_payload_sha256": "0d818f5803a96c7ae15c0a550cc9cec99bc50bf72a996697e2d0a1f09fd41145",
    "checkpoint_sidecar_sha256": "3c6ef5aa5fb4358014a95870060cc1cb0d80b2f38ee5fde8660167968f666ad0",
    "logical_state_sha256": "c05d303a31e300398ea40d3eca4b37b75a7cc832648fa0dda22920586a93e09b",
    "train_state_sha256": "207f2b99499ec67fcca99b332b28614771be84088696bbd4983c2053b482bd2c",
    "last_update_evidence_sha256": None,
    "adam_step": 0,
}

MODEL_SPECS = (
    ModelSpec(1, "raw-common-snapshot", "raw", None, None, None, None),
    ModelSpec(
        2,
        "imported-mirror-g0",
        "imported",
        (
            r"D:\mtg-kernel-exploiter-v3b-20260726"
            r"\runs-arm1\dev0\run-0\store"
        ),
        "db58dbe3f1f76b5bdf3bae4de657711dc818393b2bf1eeae88c02d8866b4d01d",
        "92d40cc1bd5ad4d54cb65cabb66b2788e4de16306727e6efc8c92f1b37e631da",
        MIRROR_PROVENANCE,
    ),
    ModelSpec(
        3,
        "imported-diverged-g0",
        "imported",
        (
            r"D:\mtg-kernel-exploiter-v3b-20260726"
            r"\runs-arm2\dev0\run-0\store"
        ),
        "9c692503df20669686d4b5706cd5ed53989a60ca9dec3778c10312b3bddc722e",
        "39d5466625461fe9eb364436255a0dec0ba75d10c1d0fcdc27b6cc582a436dfc",
        DIVERGED_PROVENANCE,
    ),
)

SHA256_RE = re.compile(r"^[0-9a-f]{64}$")
GIT_HEAD_RE = re.compile(r"^[0-9a-f]{40}$")
MAX_JSON_BYTES = 64 * 1024 * 1024
MAX_STREAM_BYTES = 128 * 1024 * 1024
MODEL_TIMEOUT_SECONDS = 120
BUILD_ENVIRONMENT_POLICY_IDENTITY = (
    "ambient-os-environment-with-build-overrides-rejected-v1"
)
RUST_TOOLCHAIN_CHANNEL = "1.94.1"
BUILD_ENVIRONMENT_ALLOWED_CARGO_RUST_NAMES = frozenset(
    {"CARGO_HOME", "RUSTUP_HOME"}
)
BUILD_ENVIRONMENT_FORBIDDEN_EXACT = frozenset(
    {
        "AR",
        "ARFLAGS",
        "CC",
        "CFLAGS",
        "CL",
        "CPP",
        "CPPFLAGS",
        "CXX",
        "CXXFLAGS",
        "INCLUDE",
        "LD",
        "LDFLAGS",
        "LIB",
        "LIBPATH",
        "LINK",
        "RUSTC",
        "RUSTDOC",
    }
)
BUILD_ENVIRONMENT_FORBIDDEN_PREFIXES = (
    "AR_",
    "CC_",
    "CFLAGS_",
    "CPPFLAGS_",
    "CXX_",
    "CXXFLAGS_",
    "LD_",
    "LDFLAGS_",
)


class AdmissionError(RuntimeError):
    """An immutable source, invocation, or output contract failed."""


def fail(message: str) -> NoReturn:
    raise AdmissionError(message)


def canonical_json_bytes(value: Any) -> bytes:
    try:
        return json.dumps(
            value,
            sort_keys=True,
            separators=(",", ":"),
            ensure_ascii=False,
            allow_nan=False,
        ).encode("utf-8")
    except (TypeError, ValueError) as error:
        raise AdmissionError(f"value is not finite canonical JSON: {error}") from error


def record_bytes(value: Mapping[str, Any]) -> bytes:
    return canonical_json_bytes(value) + b"\n"


def payload_sha256(document: Mapping[str, Any]) -> str:
    payload = dict(document)
    payload.pop("payload_sha256", None)
    return hashlib.sha256(canonical_json_bytes(payload)).hexdigest()


def attach_payload_sha256(document: dict[str, Any]) -> dict[str, Any]:
    if "payload_sha256" in document:
        fail("payload_sha256 already exists")
    document["payload_sha256"] = payload_sha256(document)
    return document


def verify_payload_sha256(document: Mapping[str, Any], where: str) -> None:
    expected = require_sha256(document.get("payload_sha256"), f"{where}.payload_sha256")
    actual = payload_sha256(document)
    if actual != expected:
        fail(f"{where}.payload_sha256 mismatch: expected={expected} actual={actual}")


def sha256_bytes(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()


def sha256_file(path: os.PathLike[str] | str) -> str:
    file_path = Path(path)
    digest = hashlib.sha256()
    byte_count = 0
    try:
        with file_path.open("rb") as source:
            before = os.fstat(source.fileno())
            if not file_path.is_file():
                fail(f"required path is not a regular file: {file_path}")
            for chunk in iter(lambda: source.read(1024 * 1024), b""):
                byte_count += len(chunk)
                digest.update(chunk)
            after = os.fstat(source.fileno())
    except OSError as error:
        raise AdmissionError(f"could not hash {file_path}: {error}") from error
    identity_before = (before.st_dev, before.st_ino, before.st_size, before.st_mtime_ns)
    identity_after = (after.st_dev, after.st_ino, after.st_size, after.st_mtime_ns)
    if identity_before != identity_after or byte_count != before.st_size:
        fail(f"file changed while being hashed: {file_path}")
    return digest.hexdigest()


def _pairs(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            fail(f"duplicate JSON key: {key!r}")
        result[key] = value
    return result


def _nonfinite(token: str) -> NoReturn:
    fail(f"non-finite JSON number is forbidden: {token}")


def _finite_tree(value: Any, where: str) -> None:
    if isinstance(value, Decimal):
        if not value.is_finite():
            fail(f"{where} contains a non-finite number")
    elif isinstance(value, list):
        for index, child in enumerate(value):
            _finite_tree(child, f"{where}[{index}]")
    elif isinstance(value, dict):
        for key, child in value.items():
            _finite_tree(child, f"{where}.{key}")


def parse_json_bytes(raw: bytes, where: str) -> dict[str, Any]:
    if not raw or len(raw) > MAX_JSON_BYTES:
        fail(f"{where} size must be within 1..{MAX_JSON_BYTES} bytes")
    try:
        value = json.loads(
            raw.decode("utf-8"),
            object_pairs_hook=_pairs,
            parse_float=Decimal,
            parse_constant=_nonfinite,
        )
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise AdmissionError(f"{where} is not strict UTF-8 JSON: {error}") from error
    if not isinstance(value, dict):
        fail(f"{where} root must be an object")
    _finite_tree(value, where)
    return value


def parse_json_bytes_native_numbers(raw: bytes, where: str) -> dict[str, Any]:
    """Parse producer JSON with Python numeric types after strict token checks.

    Producer payload hashes are verified over their raw serde_json bytes.  This
    parser is for metric validation/classification and intentionally avoids
    feeding Decimal instances back through json.dumps, whose spelling would not
    prove the original Rust serialization.
    """

    if not raw or len(raw) > MAX_JSON_BYTES:
        fail(f"{where} size must be within 1..{MAX_JSON_BYTES} bytes")
    try:
        value = json.loads(
            raw.decode("utf-8"),
            object_pairs_hook=_pairs,
            parse_constant=_nonfinite,
        )
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise AdmissionError(f"{where} is not strict UTF-8 JSON: {error}") from error
    if not isinstance(value, dict):
        fail(f"{where} root must be an object")

    def check(node: Any, location: str) -> None:
        if isinstance(node, float) and not math.isfinite(node):
            fail(f"{location} contains a non-finite number")
        if isinstance(node, list):
            for index, child in enumerate(node):
                check(child, f"{location}[{index}]")
        elif isinstance(node, dict):
            for key, child in node.items():
                check(child, f"{location}.{key}")

    check(value, where)
    return value


def read_json_document(path: os.PathLike[str] | str) -> dict[str, Any]:
    file_path = Path(path)
    try:
        return parse_json_bytes(file_path.read_bytes(), str(file_path))
    except OSError as error:
        raise AdmissionError(f"could not read {file_path}: {error}") from error


def read_canonical_record(path: Path, where: str) -> dict[str, Any]:
    try:
        raw = path.read_bytes()
    except OSError as error:
        raise AdmissionError(f"could not read {where}: {error}") from error
    document = parse_json_bytes_native_numbers(raw, where)
    if raw != record_bytes(document):
        fail(f"{where} must be canonical JSON plus one newline")
    verify_payload_sha256(document, where)
    return document


def write_exclusive(path: Path, raw: bytes) -> None:
    try:
        path.parent.mkdir(parents=True, exist_ok=True)
        with path.open("xb") as destination:
            destination.write(raw)
    except FileExistsError as error:
        raise AdmissionError(f"refusing to overwrite output: {path}") from error
    except OSError as error:
        raise AdmissionError(f"could not write {path}: {error}") from error


def exact_keys(value: Any, expected: Iterable[str], where: str) -> Mapping[str, Any]:
    if not isinstance(value, Mapping):
        fail(f"{where} must be an object")
    expected_set = set(expected)
    missing = sorted(expected_set - set(value))
    extra = sorted(set(value) - expected_set)
    if missing or extra:
        fail(f"{where} key mismatch: missing={missing} extra={extra}")
    return value


def require_array(value: Any, where: str, length: int | None = None) -> list[Any]:
    if not isinstance(value, list):
        fail(f"{where} must be an array")
    if length is not None and len(value) != length:
        fail(f"{where} must contain exactly {length} entries")
    return value


def require_sha256(value: Any, where: str) -> str:
    if type(value) is not str or SHA256_RE.fullmatch(value) is None:
        fail(f"{where} must be lower-case SHA-256")
    return value


def require_natural(value: Any, where: str, positive: bool = False) -> int:
    minimum = 1 if positive else 0
    if type(value) is not int or value < minimum or value >= 1 << 63:
        fail(f"{where} must be {'positive ' if positive else ''}u63")
    return value


def require_bool(value: Any, where: str, expected: bool | None = None) -> bool:
    if type(value) is not bool:
        fail(f"{where} must be Boolean")
    if expected is not None and value is not expected:
        fail(f"{where} must be {expected}")
    return value


def require_string(value: Any, where: str, expected: str | None = None) -> str:
    if type(value) is not str or not value:
        fail(f"{where} must be a nonempty string")
    if expected is not None and value != expected:
        fail(f"{where} mismatch: expected={expected!r} actual={value!r}")
    return value


def require_finite_number(value: Any, where: str) -> Decimal:
    if isinstance(value, bool) or not isinstance(value, (int, Decimal)):
        fail(f"{where} must be a finite number")
    result = Decimal(value)
    if not result.is_finite() or not float(result) == float(result):
        fail(f"{where} must be finite")
    return result


def same_path(left: os.PathLike[str] | str, right: os.PathLike[str] | str) -> bool:
    return os.path.normcase(str(Path(left).resolve())) == os.path.normcase(
        str(Path(right).resolve())
    )


def require_frozen_windows_path(value: os.PathLike[str] | str, expected: str, where: str) -> None:
    raw = os.fspath(value)
    if type(raw) is not str:
        fail(f"{where} must be a text path")
    if not ntpath.isabs(raw) or ntpath.normcase(ntpath.normpath(raw)) != ntpath.normcase(
        ntpath.normpath(expected)
    ):
        fail(f"{where} must equal {expected!r}; got {raw!r}")


def require_descendant_path(child: Path, root: Path, where: str) -> Path:
    child = child.resolve()
    root = root.resolve()
    try:
        common = os.path.commonpath((str(child), str(root)))
    except ValueError as error:
        raise AdmissionError(f"{where} is not below {root}: {child}") from error
    if not same_path(common, root) or same_path(child, root):
        fail(f"{where} is not a strict descendant of {root}: {child}")
    return child


def checked_git(repo: Path, *arguments: str) -> str:
    try:
        result = subprocess.run(
            ["git", *arguments],
            cwd=repo,
            check=True,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
    except (OSError, subprocess.CalledProcessError) as error:
        raise AdmissionError(f"git {' '.join(arguments)} failed: {error}") from error
    return result.stdout.strip()


def require_clean_worktree(repo: Path) -> str:
    status = checked_git(repo, "status", "--porcelain=v1", "--untracked-files=all")
    if status:
        fail(f"worktree must be clean:\n{status}")
    head = checked_git(repo, "rev-parse", "HEAD")
    if GIT_HEAD_RE.fullmatch(head) is None:
        fail(f"unexpected git HEAD: {head!r}")
    return head


def require_frozen_branch(repo: Path) -> str:
    branch = checked_git(repo, "branch", "--show-current")
    if branch != BRANCH:
        fail(f"git branch must equal {BRANCH!r}; got {branch!r}")
    return branch


def verify_frozen_inputs(repo: Path) -> dict[str, str]:
    actual: dict[str, str] = {}
    for relative, expected in FROZEN_INPUTS.items():
        digest = sha256_file(repo / relative)
        if digest != expected:
            fail(f"frozen input drift: {relative}: expected={expected} actual={digest}")
        actual[relative] = digest
    return actual


def implementation_source_records(repo: Path) -> list[dict[str, str]]:
    root = repo / "scripts" / "action_ingress_admission_v1"
    records = []
    for path in sorted(root.glob("*.py"), key=lambda candidate: candidate.name):
        records.append(
            {
                "path": path.relative_to(repo).as_posix(),
                "sha256": sha256_file(path),
            }
        )
    for relative in IMPLEMENTATION_EXTRA_PATHS:
        path = repo / relative
        records.append(
            {
                "path": relative.as_posix(),
                "sha256": sha256_file(path),
            }
        )
    records.sort(key=lambda record: record["path"])
    if not records:
        fail("no action-ingress packaging sources found")
    return records


def resolved_tool_executable(name: str) -> Path:
    raw = shutil.which(name)
    if raw is None:
        fail(f"required tool is not PATH-resolvable: {name}")
    invocation_path = Path(raw).absolute()
    resolved_path = invocation_path.resolve()
    if not resolved_path.is_absolute() or not resolved_path.is_file():
        fail(
            f"required tool must resolve to an absolute regular file: "
            f"{name}: {resolved_path}"
        )
    # Preserve the proxy invocation name (`cargo` versus `rustc`) while the
    # receipt's file record resolves and hashes the actual proxy bytes.
    return invocation_path


def selected_rust_toolchain_executable(name: str, *, rustup: Path) -> Path:
    try:
        process = subprocess.run(
            [
                str(rustup),
                "which",
                "--toolchain",
                RUST_TOOLCHAIN_CHANNEL,
                name,
            ],
            check=True,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
    except (OSError, subprocess.CalledProcessError) as error:
        raise AdmissionError(f"could not resolve frozen Rust tool {name}: {error}") from error
    path = Path(process.stdout.strip()).resolve()
    if not path.is_absolute() or not path.is_file() or path.is_symlink():
        fail(f"frozen Rust tool is not an absolute regular file: {name}: {path}")
    return path


def _cargo_config_candidates(
    repo: Path,
    environment: Mapping[str, str],
) -> tuple[Path, Path, list[Path]]:
    cargo_home = Path(
        environment.get("CARGO_HOME", str(Path.home() / ".cargo"))
    ).resolve()
    rustup_home = Path(
        environment.get("RUSTUP_HOME", str(Path.home() / ".rustup"))
    ).resolve()
    raw_candidates: list[Path] = []
    resolved_repo = repo.resolve()
    for ancestor in (resolved_repo, *resolved_repo.parents):
        raw_candidates.extend(
            (
                ancestor / ".cargo" / "config",
                ancestor / ".cargo" / "config.toml",
            )
        )
    raw_candidates.extend((cargo_home / "config", cargo_home / "config.toml"))
    candidates: list[Path] = []
    seen: set[str] = set()
    for candidate in raw_candidates:
        absolute = candidate.absolute()
        key = os.path.normcase(str(absolute))
        if key not in seen:
            seen.add(key)
            candidates.append(absolute)
    return cargo_home, rustup_home, candidates


def build_environment_policy(
    environment: Mapping[str, str],
    *,
    repo: Path,
    target_dir: Path,
    rustc: Path,
) -> dict[str, Any]:
    offenders: list[str] = []
    for raw_name in environment:
        name = raw_name.upper()
        cargo_or_rust_override = (
            (name.startswith("CARGO_") or name.startswith("RUST"))
            and name not in BUILD_ENVIRONMENT_ALLOWED_CARGO_RUST_NAMES
        )
        compiler_override = (
            name in BUILD_ENVIRONMENT_FORBIDDEN_EXACT
            or any(name.startswith(prefix) for prefix in BUILD_ENVIRONMENT_FORBIDDEN_PREFIXES)
        )
        if cargo_or_rust_override or compiler_override:
            offenders.append(raw_name)
    if offenders:
        fail(f"ambient build-affecting environment is forbidden: {sorted(offenders)}")
    cargo_home, rustup_home, config_candidates = _cargo_config_candidates(
        repo, environment
    )
    present_configs = [
        str(path) for path in config_candidates if os.path.lexists(path)
    ]
    if present_configs:
        fail(f"Cargo configuration inputs must be absent: {present_configs}")
    return {
        "allowed_ambient_cargo_rust_names": sorted(
            BUILD_ENVIRONMENT_ALLOWED_CARGO_RUST_NAMES
        ),
        "cargo_config_candidates_absent": [str(path) for path in config_candidates],
        "cargo_home": str(cargo_home),
        "effective_overrides": {
            "CARGO_TARGET_DIR": str(target_dir.resolve()),
            "CUDA_VISIBLE_DEVICES": "",
            "MTG_KERNEL_DEVICE": "cpu",
            "RUSTC": str(rustc.resolve()),
        },
        "forbidden_exact_names": sorted(BUILD_ENVIRONMENT_FORBIDDEN_EXACT),
        "forbidden_prefixes": list(BUILD_ENVIRONMENT_FORBIDDEN_PREFIXES),
        "identity": BUILD_ENVIRONMENT_POLICY_IDENTITY,
        "present_build_affecting_overrides": [],
        "rustup_home": str(rustup_home),
    }


def cargo_build_command(cargo: str = "cargo") -> list[str]:
    return [
        cargo,
        "test",
        "--release",
        "--locked",
        "-p",
        "mtg-kernel",
        "--lib",
        "--no-run",
        "--no-default-features",
        "--message-format=json",
    ]


def integrity_test_command(executable: Path) -> list[str]:
    return [str(executable), TEST_MODULE, "--nocapture", "--test-threads=1"]


def probe_command(executable: Path) -> list[str]:
    return [
        str(executable),
        PROBE_TEST,
        "--ignored",
        "--exact",
        "--nocapture",
        "--test-threads=1",
    ]


def windows_static_commands(python: str) -> list[list[str]]:
    return [
        [
            python,
            STATIC_TOOL_RELATIVE_PATH.as_posix(),
            "--check",
            "--output",
            STATIC_REPORT_RELATIVE_PATH.as_posix(),
        ],
        [
            python,
            "-m",
            "unittest",
            "discover",
            "-s",
            "python/tests",
            "-p",
            STATIC_TEST_RELATIVE_PATH.name,
            "-v",
        ],
    ]


def linux_static_commands(wsl: str = "wsl.exe") -> list[list[str]]:
    prefix = [wsl, "--cd", LINUX_REPO, "python3"]
    return [
        [
            *prefix,
            STATIC_TOOL_RELATIVE_PATH.as_posix(),
            "--check",
            "--output",
            STATIC_REPORT_RELATIVE_PATH.as_posix(),
        ],
        [
            *prefix,
            "-m",
            "unittest",
            "discover",
            "-s",
            "python/tests",
            "-p",
            STATIC_TEST_RELATIVE_PATH.name,
            "-v",
        ],
    ]


def windows_packaging_test_command(python: str) -> list[str]:
    return [
        python,
        "-m",
        "unittest",
        "discover",
        "-s",
        "scripts/action_ingress_admission_v1",
        "-p",
        "test_*.py",
        "-v",
    ]


def linux_packaging_test_command(wsl: str = "wsl.exe") -> list[str]:
    return [
        wsl,
        "--cd",
        LINUX_REPO,
        "python3",
        "-m",
        "unittest",
        "discover",
        "-s",
        "scripts/action_ingress_admission_v1",
        "-p",
        "test_*.py",
        "-v",
    ]


def listed_test_names(listed: str) -> set[str]:
    names: set[str] = set()
    for line in listed.splitlines():
        name, separator, kind = line.rpartition(": ")
        if separator and kind == "test":
            names.add(name)
    return names


def unittest_success_count(stderr: bytes, where: str) -> int:
    try:
        text = stderr.decode("utf-8")
    except UnicodeDecodeError as error:
        raise AdmissionError(f"{where} was not UTF-8") from error
    lines = text.splitlines()
    summary = re.compile(
        r"Ran ([1-9][0-9]*) tests? in [0-9]+(?:\.[0-9]+)?s"
    )
    matches = [
        match.group(1)
        for line in lines
        if (match := summary.fullmatch(line)) is not None
    ]
    if len(matches) != 1 or lines.count("OK") != 1:
        fail(f"{where} lacks one positive unittest summary")
    return int(matches[0])


def resolve_lib_test_executable(messages: Iterable[Mapping[str, Any]]) -> Path:
    candidates: list[Path] = []
    for message in messages:
        if message.get("reason") != "compiler-artifact":
            continue
        profile = message.get("profile")
        target = message.get("target")
        if not isinstance(profile, Mapping) or not isinstance(target, Mapping):
            fail("Cargo compiler artifact profile/target must be objects")
        if (
            profile.get("test") is True
            and target.get("name") == "mtg_kernel"
            and target.get("kind") == ["lib"]
        ):
            executable = require_string(message.get("executable"), "Cargo executable")
            candidates.append(Path(executable))
    unique = list(dict.fromkeys(candidates))
    if len(unique) != 1:
        fail(f"expected one mtg_kernel lib-test executable, got {unique}")
    if not unique[0].is_absolute() or not unique[0].is_file():
        fail(f"Cargo executable must be an existing absolute file: {unique[0]}")
    return unique[0]


def file_record(path: Path, *, root: Path | None = None) -> dict[str, Any]:
    resolved = path.resolve()
    shown = (
        resolved.relative_to(root.resolve()).as_posix()
        if root is not None
        else str(resolved)
    )
    return {
        "byte_count": resolved.stat().st_size,
        "path": shown,
        "sha256": sha256_file(resolved),
    }
