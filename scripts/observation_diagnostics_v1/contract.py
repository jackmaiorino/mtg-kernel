"""Frozen contracts and strict record helpers for observation diagnostics v1."""

from __future__ import annotations

import dataclasses
from decimal import Decimal
import hashlib
import json
import ntpath
import os
from pathlib import Path
import re
import subprocess
import sys
from typing import Any, Iterable, Mapping, NoReturn


LABEL = "OBSERVATION-HASH-RELIANCE-DIAGNOSTIC-NON-EVIDENCE"
MANIFEST_RELATIVE_PATH = Path("OBSERVATION_DIAGNOSTICS_EXECUTION_V1.md")
ARTIFACT_ROOT_WINDOWS = r"D:\mtg-kernel-observation-diagnostics-v1-20260726"
TARGET_DIR_WINDOWS = r"E:\cargo-target-observation-diagnostics-v1"
BUILD_RECEIPT_WINDOWS = ntpath.join(
    ARTIFACT_ROOT_WINDOWS, "build", "build-receipt.json"
)
COMPLETION_RECEIPT_WINDOWS = ntpath.join(
    ARTIFACT_ROOT_WINDOWS, "completion-receipt.json"
)
CLASSIFICATION_ROOT_WINDOWS = ntpath.join(
    ARTIFACT_ROOT_WINDOWS, "classification"
)
CLASSIFICATION_OUTPUT_WINDOWS = ntpath.join(
    CLASSIFICATION_ROOT_WINDOWS, "classification.json"
)
CLASSIFICATION_STDOUT_WINDOWS = ntpath.join(
    CLASSIFICATION_ROOT_WINDOWS, "classifier.stdout.log"
)
CLASSIFICATION_STDERR_WINDOWS = ntpath.join(
    CLASSIFICATION_ROOT_WINDOWS, "classifier.stderr.log"
)
CLASSIFICATION_RECEIPT_WINDOWS = ntpath.join(
    CLASSIFICATION_ROOT_WINDOWS, "classification-receipt.json"
)
CLASSIFIER_TIMEOUT_SECONDS = 120

BUILD_RECEIPT_SCHEMA = "mtg-kernel-observation-diagnostics-build-receipt/v1"
INVOCATION_RECEIPT_SCHEMA = (
    "mtg-kernel-observation-diagnostics-invocation-receipt/v1"
)
COMPLETION_RECEIPT_SCHEMA = (
    "mtg-kernel-observation-diagnostics-completion-receipt/v1"
)
CLASSIFICATION_RECEIPT_SCHEMA = (
    "mtg-kernel-observation-diagnostics-classification-receipt/v1"
)
PROBE_ENVELOPE_SCHEMA = "mtg-kernel-checkpoint-reliance-probe/v1"
PROBE_PAYLOAD_SCHEMA = "mtg-kernel-checkpoint-reliance-probe-payload/v1"

TEST_MODULE = (
    "native_checkpoint_inference_v1::checkpoint_reliance_probe_v1"
)
REQUIRED_TESTS = (
    f"{TEST_MODULE}::feature_partitions_and_ingress_groups_are_exact_v1",
    (
        f"{TEST_MODULE}::"
        "permutation_and_ablation_transforms_are_scoped_and_reversible_v1"
    ),
    (
        f"{TEST_MODULE}::"
        "metric_primitives_are_gauge_invariant_and_fail_on_nonfinite_v1"
    ),
    (
        f"{TEST_MODULE}::"
        "complete_action_rotation_is_logit_equivariant_and_value_invariant_v1"
    ),
    (
        f"{TEST_MODULE}::"
        "fixed_rally_corpus_is_repeatable_without_external_artifacts_v1"
    ),
    f"{TEST_MODULE}::trained_checkpoint_hash_vs_direct_reliance_probe_v1",
)
PROBE_TEST = REQUIRED_TESTS[-1]

SHA256_RE = re.compile(r"^[0-9a-f]{64}$")
GIT_HEAD_RE = re.compile(r"^[0-9a-f]{40}$")
MAX_JSON_BYTES = 64 * 1024 * 1024
MAX_PROCESS_STREAM_BYTES = 128 * 1024 * 1024
STATIC_AUDIT_TOOL_RELATIVE_PATH = Path(
    "python/tools/audit_feature_coverage_collision_v1.py"
)
STATIC_AUDIT_TEST_RELATIVE_PATH = Path(
    "python/tests/test_feature_coverage_collision_v1.py"
)
STATIC_AUDIT_REPORT_RELATIVE_PATH = Path(
    "data/flat_policy_v2/feature_coverage_collision_audit_v1.json"
)
STATIC_AUDIT_SCHEMA = "mtg-kernel-feature-coverage-collision-audit/v1"
STATIC_AUDIT_POSITIVE_MARKER = "FEATURE_COVERAGE_COLLISION_AUDIT_V1_OK"
STATIC_AUDIT_REQUIRED_TEST_COUNT = 7
STATIC_AUDIT_STATUSES = {
    "INVALID",
    "COLLISION-DETECTED",
    "COVERAGE-INCOMPLETE",
    "HASH-DEPENDENCE-CANDIDATE",
    "STRUCTURED-DISTINGUISHABLE",
}


class DiagnosticError(RuntimeError):
    """A frozen packaging, execution, or record contract failed."""


@dataclasses.dataclass(frozen=True)
class PairSpec:
    ordinal: int
    arm: str
    seed: int
    store_root: str
    candidate_generation: int

    @property
    def name(self) -> str:
        return (
            f"{self.ordinal:02d}-{self.arm}-seed{self.seed}-"
            f"g{self.candidate_generation}"
        )

    def as_record(self) -> dict[str, Any]:
        return {
            "arm": self.arm,
            "candidate_generation": self.candidate_generation,
            "name": self.name,
            "ordinal": self.ordinal,
            "seed": self.seed,
            "store_root": self.store_root,
        }


PAIR_SPECS = (
    PairSpec(
        1,
        "mirror-start",
        920013,
        (
            r"D:\mtg-kernel-exploiter-v3b-20260726"
            r"\runs-arm1\dev0\run-0\store"
        ),
        256,
    ),
    PairSpec(
        2,
        "mirror-start",
        920014,
        (
            r"D:\mtg-kernel-exploiter-v3b-20260726"
            r"\runs-arm1\dev0\run-1\store"
        ),
        384,
    ),
    PairSpec(
        3,
        "mirror-start",
        920015,
        (
            r"D:\mtg-kernel-exploiter-v3b-20260726"
            r"\runs-arm1\dev0\run-2\store"
        ),
        256,
    ),
    PairSpec(
        4,
        "diverged-start",
        920016,
        (
            r"D:\mtg-kernel-exploiter-v3b-20260726"
            r"\runs-arm2\dev0\run-0\store"
        ),
        256,
    ),
    PairSpec(
        5,
        "diverged-start",
        920017,
        (
            r"D:\mtg-kernel-exploiter-v3b-20260726"
            r"\runs-arm2\dev0\run-1\store"
        ),
        512,
    ),
    PairSpec(
        6,
        "diverged-start",
        920018,
        (
            r"D:\mtg-kernel-exploiter-v3b-20260726"
            r"\runs-arm2\dev0\run-2\store"
        ),
        128,
    ),
)


def fail(message: str) -> NoReturn:
    raise DiagnosticError(message)


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
        raise DiagnosticError(f"value is not canonical finite JSON: {error}") from error


def record_bytes(value: Mapping[str, Any]) -> bytes:
    return canonical_json_bytes(value) + b"\n"


def payload_sha256(document: Mapping[str, Any]) -> str:
    payload = dict(document)
    payload.pop("payload_sha256", None)
    return hashlib.sha256(canonical_json_bytes(payload)).hexdigest()


def attach_payload_sha256(document: dict[str, Any]) -> dict[str, Any]:
    if "payload_sha256" in document:
        fail("payload_sha256 must not exist before attachment")
    document["payload_sha256"] = payload_sha256(document)
    return document


def verify_payload_sha256(document: Mapping[str, Any], where: str) -> None:
    expected = document.get("payload_sha256")
    if not is_sha256(expected):
        fail(f"{where}.payload_sha256 must be lower-case SHA-256")
    actual = payload_sha256(document)
    if expected != actual:
        fail(
            f"{where}.payload_sha256 mismatch: "
            f"expected={expected} actual={actual}"
        )


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
        raise DiagnosticError(f"could not hash {file_path}: {error}") from error
    before_identity = (
        before.st_dev,
        before.st_ino,
        before.st_size,
        before.st_mtime_ns,
    )
    after_identity = (
        after.st_dev,
        after.st_ino,
        after.st_size,
        after.st_mtime_ns,
    )
    if before_identity != after_identity or byte_count != before.st_size:
        fail(f"file changed while being hashed: {file_path}")
    return digest.hexdigest()


def _duplicate_rejecting_pairs(
    pairs: list[tuple[str, Any]],
) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            fail(f"duplicate JSON key: {key!r}")
        result[key] = value
    return result


def _reject_nonfinite(token: str) -> NoReturn:
    fail(f"non-finite JSON number is forbidden: {token}")


def _check_finite(value: Any, where: str) -> None:
    if isinstance(value, Decimal):
        if not value.is_finite():
            fail(f"{where} contains a non-finite number")
        return
    if isinstance(value, list):
        for index, child in enumerate(value):
            _check_finite(child, f"{where}[{index}]")
        return
    if isinstance(value, dict):
        for key, child in value.items():
            _check_finite(child, f"{where}.{key}")


def parse_json_bytes(raw: bytes, where: str) -> dict[str, Any]:
    if not raw or len(raw) > MAX_JSON_BYTES:
        fail(f"{where} size must be within 1..{MAX_JSON_BYTES} bytes")
    try:
        value = json.loads(
            raw.decode("utf-8"),
            object_pairs_hook=_duplicate_rejecting_pairs,
            parse_float=Decimal,
            parse_constant=_reject_nonfinite,
        )
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise DiagnosticError(f"{where} is not strict UTF-8 JSON: {error}") from error
    if not isinstance(value, dict):
        fail(f"{where} root must be a JSON object")
    _check_finite(value, where)
    return value


def read_json_document(path: os.PathLike[str] | str) -> dict[str, Any]:
    file_path = Path(path)
    try:
        raw = file_path.read_bytes()
    except OSError as error:
        raise DiagnosticError(f"could not read {file_path}: {error}") from error
    return parse_json_bytes(raw, str(file_path))


def write_exclusive(path: Path, raw: bytes) -> None:
    try:
        path.parent.mkdir(parents=True, exist_ok=True)
        with path.open("xb") as destination:
            destination.write(raw)
    except FileExistsError as error:
        raise DiagnosticError(f"refusing to overwrite existing output: {path}") from error
    except OSError as error:
        raise DiagnosticError(f"could not write {path}: {error}") from error


def is_sha256(value: Any) -> bool:
    return type(value) is str and SHA256_RE.fullmatch(value) is not None


def require_sha256(value: Any, where: str) -> str:
    if not is_sha256(value):
        fail(f"{where} must be lower-case SHA-256")
    return value


def require_natural(value: Any, where: str, *, positive: bool = False) -> int:
    minimum = 1 if positive else 0
    if type(value) is not int or value < minimum or value >= (1 << 63):
        domain = "positive u63" if positive else "u63"
        fail(f"{where} must be {domain}")
    return value


def exact_keys(
    value: Any,
    expected: Iterable[str],
    where: str,
) -> Mapping[str, Any]:
    if not isinstance(value, Mapping):
        fail(f"{where} must be an object")
    expected_set = set(expected)
    missing = sorted(expected_set - set(value))
    extra = sorted(set(value) - expected_set)
    if missing or extra:
        fail(f"{where} key mismatch: missing={missing} extra={extra}")
    return value


def require_array(value: Any, where: str) -> list[Any]:
    if not isinstance(value, list):
        fail(f"{where} must be an array")
    return value


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
        raise DiagnosticError(
            f"git {' '.join(arguments)} failed in {repo}: {error}"
        ) from error
    return result.stdout.strip()


def require_clean_worktree(repo: Path) -> str:
    status = checked_git(repo, "status", "--porcelain=v1", "--untracked-files=all")
    if status:
        fail(f"worktree must be clean:\n{status}")
    head = checked_git(repo, "rev-parse", "HEAD")
    if GIT_HEAD_RE.fullmatch(head) is None:
        fail(f"unexpected git HEAD identity: {head!r}")
    return head


def same_path(left: os.PathLike[str] | str, right: os.PathLike[str] | str) -> bool:
    return os.path.normcase(str(Path(left).resolve())) == os.path.normcase(
        str(Path(right).resolve())
    )


def require_frozen_windows_path(
    value: os.PathLike[str] | str,
    expected: str,
    where: str,
) -> None:
    try:
        raw = os.fspath(value)
    except TypeError as error:
        raise DiagnosticError(f"{where} must be a Windows text path") from error
    if type(raw) is not str:
        fail(f"{where} must be a Windows text path")
    actual_normalized = ntpath.normcase(ntpath.normpath(raw))
    expected_normalized = ntpath.normcase(ntpath.normpath(expected))
    if not ntpath.isabs(raw) or actual_normalized != expected_normalized:
        fail(
            f"{where} must equal the frozen Windows path: "
            f"expected={expected!r} actual={raw!r}"
        )


def require_descendant_path(
    child: os.PathLike[str] | str,
    root: os.PathLike[str] | str,
    where: str,
) -> Path:
    try:
        child_path = Path(child).resolve()
        root_path = Path(root).resolve()
    except TypeError as error:
        raise DiagnosticError(f"{where} paths must be text paths") from error
    try:
        common = os.path.commonpath((str(root_path), str(child_path)))
    except ValueError as error:
        raise DiagnosticError(
            f"{where} must be a descendant of {root_path}: {child_path}"
        ) from error
    if not same_path(common, root_path) or same_path(child_path, root_path):
        fail(f"{where} must be a descendant of {root_path}: {child_path}")
    return child_path


def cargo_build_command() -> list[str]:
    return [
        "cargo",
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


def probe_command(executable: Path) -> list[str]:
    return [
        str(executable),
        PROBE_TEST,
        "--ignored",
        "--exact",
        "--nocapture",
        "--test-threads=1",
    ]


def integrity_test_command(executable: Path) -> list[str]:
    return [
        str(executable),
        TEST_MODULE,
        "--nocapture",
        "--test-threads=1",
    ]


def static_audit_check_command() -> list[str]:
    return [
        sys.executable,
        STATIC_AUDIT_TOOL_RELATIVE_PATH.as_posix(),
        "--check",
    ]


def static_audit_test_command() -> list[str]:
    return [
        sys.executable,
        "-m",
        "unittest",
        "discover",
        "-s",
        "python/tests",
        "-p",
        STATIC_AUDIT_TEST_RELATIVE_PATH.name,
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
        raise DiagnosticError(f"{where} was not UTF-8") from error
    summary = re.compile(
        r"Ran ([1-9][0-9]*) tests? in [0-9]+(?:\.[0-9]+)?s"
    )
    matches = [
        match.group(1)
        for line in text.splitlines()
        if (match := summary.fullmatch(line)) is not None
    ]
    if len(matches) != 1 or text.splitlines().count("OK") != 1:
        fail(f"{where} lacks one positive nonempty unittest summary")
    return int(matches[0])


def resolve_lib_test_executable(messages: Iterable[Mapping[str, Any]]) -> Path:
    candidates: list[Path] = []
    for message in messages:
        if message.get("reason") != "compiler-artifact":
            continue
        profile = message.get("profile")
        target = message.get("target")
        if not isinstance(profile, Mapping) or not isinstance(target, Mapping):
            fail("Cargo compiler-artifact profile/target must be objects")
        kind = target.get("kind")
        if not isinstance(kind, list) or any(type(item) is not str for item in kind):
            fail("Cargo compiler-artifact target.kind must be a string array")
        if (
            profile.get("test") is True
            and "lib" in kind
            and target.get("name") == "mtg_kernel"
        ):
            executable = message.get("executable")
            if type(executable) is not str or not executable:
                fail("mtg_kernel lib-test compiler artifact lacks an executable")
            candidates.append(Path(executable))
    unique = list(dict.fromkeys(candidates))
    if len(unique) != 1:
        fail(f"expected exactly one mtg_kernel lib-test executable, got {unique}")
    if not unique[0].is_absolute():
        fail(f"resolved executable path must be absolute: {unique[0]}")
    if not unique[0].is_file():
        fail(f"resolved executable does not exist: {unique[0]}")
    return unique[0]
