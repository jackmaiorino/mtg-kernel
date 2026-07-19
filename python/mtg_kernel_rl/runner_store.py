"""Immutable, independently validated artifacts for deterministic policy runs."""

from __future__ import annotations

import dataclasses
import re
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from . import __version__
from .action_sampling import fixed_categorical_sampler_contract
from .artifact_io import (
    CapturedFile,
    canonical_json_bytes,
    json_values_equal_strict,
    parse_canonical_json_bytes,
    read_regular_file_bytes,
    sha256_bytes,
    validate_training_json_privacy,
)
from .artifacts import inject_fault, write_bytes_atomic
from .checkpoint import compute_head
from .client import (
    POLICY_SURFACE_VERSION,
    PROTOCOL_NAME,
    PROTOCOL_VERSION,
    SCHEMA_VERSION,
    SURFACE_VERSION,
)
from .determinism import SeedDerivation, derive_env_seed, validate_uint63
from .model import ModelConfig
from .path_safety import (
    OUTPUT_LOCK_FILE_NAME,
    ensure_real_dir,
    ensure_real_file,
    is_verified_output_lock_entry,
    scandir_no_follow,
)
from .training_store import MAX_PHYSICAL_DECISIONS, MAX_POLICY_STEPS, RUN_SCHEMA as SOURCE_RUN_SCHEMA


RUN_SCHEMA = "kernel_rl_runner_run/v5"
EPISODE_SCHEMA = "kernel_rl_runner_episode/v1"
RUN_FILE_NAME = "run.json"
EPISODES_FILE_NAME = "episodes.jsonl"
MAX_EPISODES = 262_144
MAX_RUN_BYTES = 2 * 1024 * 1024
MAX_EPISODE_ROW_BYTES = 64 * 1024
MAX_EPISODES_BYTES = 256 * 1024 * 1024
HEX64_RE = re.compile(r"^[0-9a-f]{64}$")


@dataclass(frozen=True, slots=True)
class ValidatedRunnerArtifacts:
    """Path-free receipt for a complete, immutable runner publication."""

    run_sha256: str
    episode_count: int
    p0_wins: int
    p1_wins: int
    draws: int
    policy_head: str | None


def _require_keys(value: Any, expected: set[str], context: str) -> dict[str, Any]:
    if type(value) is not dict:
        raise TypeError(f"{context} must be an object")
    missing = expected - set(value)
    extra = set(value) - expected
    if missing or extra:
        raise ValueError(f"{context} fields mismatch: missing={sorted(missing)} extra={sorted(extra)}")
    return value


def _int(value: Any, context: str, *, minimum: int = 0, maximum: int | None = None) -> int:
    if type(value) is not int:
        raise TypeError(f"{context} must be an integer and not bool")
    if value < minimum or (maximum is not None and value > maximum):
        limit = f"[{minimum}, {maximum}]" if maximum is not None else f">= {minimum}"
        raise ValueError(f"{context} must be {limit}")
    return value


def _str(value: Any, context: str, *, nonempty: bool = True) -> str:
    if type(value) is not str or (nonempty and not value):
        raise ValueError(f"{context} must be {'a nonempty ' if nonempty else ''}string")
    return value


def _hash(value: Any, context: str) -> str:
    if type(value) is not str or HEX64_RE.fullmatch(value) is None:
        raise ValueError(f"{context} must be exactly 64 lowercase hexadecimal characters")
    return value


def _policy(value: Any, context: str) -> str:
    policy = _str(value, context)
    if policy not in {"greedy", "sampled", "uniform"}:
        raise ValueError(f"{context} is unsupported")
    return policy


def _deck_ids(value: Any, context: str) -> tuple[str, str]:
    if type(value) is not list or len(value) != 2:
        raise ValueError(f"{context} must contain two deck IDs")
    if any(type(item) is not str or not item for item in value):
        raise ValueError(f"{context} entries must be nonempty strings")
    return value[0], value[1]


def _deck_hashes(value: Any, context: str) -> tuple[int, int]:
    if type(value) is not list or len(value) != 2:
        raise ValueError(f"{context} must contain two deck hashes")
    return (
        _int(value[0], f"{context}[0]", maximum=(1 << 64) - 1),
        _int(value[1], f"{context}[1]", maximum=(1 << 64) - 1),
    )


def _validate_provenance(value: Any, context: str) -> dict[str, Any]:
    provenance = _require_keys(
        value,
        {
            "card_db_hash",
            "kernel_version",
            "policy_surface_version",
            "protocol",
            "protocol_version",
            "schema_version",
            "surface_version",
        },
        context,
    )
    if provenance["protocol"] != PROTOCOL_NAME:
        raise ValueError(f"{context}.protocol mismatch")
    if _int(provenance["protocol_version"], f"{context}.protocol_version") != PROTOCOL_VERSION:
        raise ValueError(f"{context}.protocol_version mismatch")
    if _int(provenance["schema_version"], f"{context}.schema_version") != SCHEMA_VERSION:
        raise ValueError(f"{context}.schema_version mismatch")
    if _int(provenance["surface_version"], f"{context}.surface_version") != SURFACE_VERSION:
        raise ValueError(f"{context}.surface_version mismatch")
    if _int(provenance["policy_surface_version"], f"{context}.policy_surface_version") != POLICY_SURFACE_VERSION:
        raise ValueError(f"{context}.policy_surface_version mismatch")
    _int(provenance["card_db_hash"], f"{context}.card_db_hash", maximum=(1 << 64) - 1)
    _str(provenance["kernel_version"], f"{context}.kernel_version")
    return provenance


def _sampled_contract() -> dict[str, Any]:
    return {
        "categorical_sampler": fixed_categorical_sampler_contract(),
        "inference": "torch.no_grad model forward; selector consumes detached logits",
        "mode": "sampled_softmax",
        "replacement": False,
        "temperature_hex": "0x1.0000000000000p+0",
    }


def _policy_contract(policy: str) -> dict[str, Any]:
    if policy == "uniform":
        return {
            "algorithm": "derive_uniform_index modulo legal_action_count",
            "inference": "unused",
            "mode": "uniform",
        }
    if policy == "greedy":
        return {
            "algorithm": "argmax over finite CPU float32 logits",
            "inference": "torch.no_grad model forward",
            "mode": "greedy",
            "tie_break": "lowest legal-action index",
        }
    if policy == "sampled":
        return _sampled_contract()
    raise ValueError(f"unsupported policy {policy}")


def _seed_contract() -> dict[str, Any]:
    contract = dataclasses.asdict(SeedDerivation())
    contract["outputs"] = {
        "environment_seed": "full unsigned 64-bit SplitMix64 output",
        "sampled_action_seed": "SplitMix64 output & 0x7fff_ffff",
        "uniform_action_index": "SplitMix64 output modulo legal_action_count",
    }
    contract["seat_encoding"] = {"p0": "0x5030", "p1": "0x5031"}
    return contract


def _validate_snapshot(value: Any, *, source_digest: str, model_fingerprint: str) -> str:
    snapshot = _require_keys(
        value,
        {
            "checkpoint_sha256",
            "head",
            "logical_state_sha256",
            "model_contract_fingerprint",
            "parent_head",
            "run_digest",
            "update",
            "update_record_sha256",
        },
        "policy_source.snapshot",
    )
    _int(snapshot["update"], "policy_source.snapshot.update", minimum=1, maximum=1_000_000)
    run_digest = _hash(snapshot["run_digest"], "policy_source.snapshot.run_digest")
    if run_digest != source_digest:
        raise ValueError("policy snapshot run digest mismatch")
    if _hash(snapshot["model_contract_fingerprint"], "policy snapshot model fingerprint") != model_fingerprint:
        raise ValueError("policy snapshot model fingerprint mismatch")
    parent_head = _hash(snapshot["parent_head"], "policy snapshot parent_head")
    checkpoint_sha = _hash(snapshot["checkpoint_sha256"], "policy snapshot checkpoint_sha256")
    logical_sha = _hash(snapshot["logical_state_sha256"], "policy snapshot logical_state_sha256")
    update_sha = _hash(snapshot["update_record_sha256"], "policy snapshot update_record_sha256")
    head = _hash(snapshot["head"], "policy snapshot head")
    expected = compute_head(
        parent_head=parent_head,
        checkpoint_byte_hash=checkpoint_sha,
        logical_hash=logical_sha,
        update_hash=update_sha,
    )
    if head != expected:
        raise ValueError("policy snapshot head algebra mismatch")
    return head


def _validate_runtime(value: Any, context: str) -> dict[str, Any]:
    runtime = _require_keys(
        value,
        {
            "architecture",
            "cpu_only",
            "default_device",
            "default_dtype",
            "deterministic_algorithms",
            "machine",
            "num_interop_threads",
            "num_threads",
            "os_release",
            "os_system",
            "python_byteorder",
            "python_implementation",
            "python_version",
            "torch_config_sha256",
            "torch_version",
        },
        context,
    )
    for key in (
        "architecture",
        "default_device",
        "default_dtype",
        "machine",
        "os_release",
        "os_system",
        "python_byteorder",
        "python_implementation",
        "python_version",
        "torch_version",
    ):
        _str(runtime[key], f"{context}.{key}", nonempty=False)
    _hash(runtime["torch_config_sha256"], f"{context}.torch_config_sha256")
    if runtime["cpu_only"] is not True or runtime["deterministic_algorithms"] is not True:
        raise ValueError(f"{context} deterministic CPU flags mismatch")
    if runtime["default_device"] != "cpu" or runtime["default_dtype"] != "torch.float32":
        raise ValueError(f"{context} device/dtype mismatch")
    _int(runtime["num_threads"], f"{context}.num_threads", minimum=1)
    _int(runtime["num_interop_threads"], f"{context}.num_interop_threads", minimum=1)
    return runtime


def _validate_policy_source(
    value: Any,
    *,
    p0: str,
    p1: str,
    env_sha: str,
    deck_ids: tuple[str, str],
    deck_hashes: tuple[int, int],
    provenance: dict[str, Any],
    runtime: dict[str, Any],
    max_physical_decisions: int,
    max_policy_steps: int,
) -> str | None:
    if p0 == "uniform" and p1 == "uniform":
        _require_keys(value, {"mode"}, "policy_source")
        if value["mode"] != "none_uniform_only":
            raise ValueError("uniform-only run policy source mismatch")
        return None

    source = _require_keys(value, {"mode", "snapshot", "source_training"}, "policy_source")
    if source["mode"] != "validated_training_head":
        raise ValueError("neural runner policies require a validated training head")
    training = _require_keys(
        source["source_training"],
        {
            "environment",
            "feature_contract",
            "model_contract",
            "protocol",
            "protocol_provenance",
            "run",
            "runtime_compatibility",
            "trainer_max_physical_decisions",
            "trainer_max_policy_steps",
        },
        "policy_source.source_training",
    )
    run = _require_keys(training["run"], {"schema", "sha256"}, "policy_source.source_training.run")
    if run["schema"] != SOURCE_RUN_SCHEMA:
        raise ValueError("source training run schema mismatch")
    source_digest = _hash(run["sha256"], "source training run sha256")
    source_environment = _require_keys(
        training["environment"], {"binary_sha256", "deck_hashes", "deck_ids"}, "source training environment"
    )
    if _hash(source_environment["binary_sha256"], "source environment binary_sha256") != env_sha:
        raise ValueError("runner environment differs from source training environment")
    if _deck_ids(source_environment["deck_ids"], "source environment deck_ids") != deck_ids:
        raise ValueError("runner deck IDs differ from source training environment")
    if _deck_hashes(source_environment["deck_hashes"], "source environment deck_hashes") != deck_hashes:
        raise ValueError("runner deck hashes differ from source training environment")
    expected_protocol = {
        "protocol": PROTOCOL_NAME,
        "protocol_version": PROTOCOL_VERSION,
        "schema_version": SCHEMA_VERSION,
    }
    if not json_values_equal_strict(training["protocol"], expected_protocol):
        raise ValueError("source training protocol mismatch")
    if not json_values_equal_strict(training["protocol_provenance"], provenance):
        raise ValueError("runner provenance differs from source training provenance")
    feature = _require_keys(
        training["feature_contract"],
        {"feature_contract_digest", "feature_encoding_digest", "feature_registry_version", "feature_schema_version"},
        "source training feature contract",
    )
    for key in ("feature_contract_digest", "feature_encoding_digest"):
        _hash(feature[key], f"source training feature contract {key}")
    for key in ("feature_registry_version", "feature_schema_version"):
        _str(feature[key], f"source training feature contract {key}")
    model_contract = _require_keys(training["model_contract"], {"config", "contract_fingerprint"}, "source model contract")
    model_config = ModelConfig.from_dict(model_contract["config"])
    model_fingerprint = _hash(model_contract["contract_fingerprint"], "source model fingerprint")
    if model_config.contract_fingerprint() != model_fingerprint:
        raise ValueError("source model contract fingerprint mismatch")
    if (
        feature["feature_contract_digest"] != model_config.feature_contract_digest
        or feature["feature_encoding_digest"] != model_config.feature_encoding_digest
        or feature["feature_registry_version"] != model_config.feature_registry_version
        or feature["feature_schema_version"] != model_config.feature_schema_version
    ):
        raise ValueError("source feature and model contracts disagree")
    source_runtime = _validate_runtime(training["runtime_compatibility"], "source runtime compatibility")
    if not json_values_equal_strict(source_runtime, runtime):
        raise ValueError("runner runtime differs from source training runtime")
    if _int(
        training["trainer_max_physical_decisions"],
        "source trainer max_physical_decisions",
        minimum=1,
        maximum=MAX_PHYSICAL_DECISIONS,
    ) != max_physical_decisions:
        raise ValueError("runner max_physical_decisions differs from source training")
    if _int(
        training["trainer_max_policy_steps"],
        "source trainer max_policy_steps",
        minimum=1,
        maximum=MAX_POLICY_STEPS,
    ) != max_policy_steps:
        raise ValueError("runner max_policy_steps differs from source training")
    return _validate_snapshot(source["snapshot"], source_digest=source_digest, model_fingerprint=model_fingerprint)


def _validate_manifest(
    manifest: dict[str, Any],
) -> tuple[int, int, int, int, str, str, tuple[str, str], tuple[int, int], str | None]:
    _require_keys(
        manifest,
        {
            "action_selection",
            "aggregate",
            "artifact_schema_version",
            "artifact_schemas",
            "config",
            "environment",
            "files",
            "package",
            "policy_source",
            "publication",
            "runner_runtime_compatibility",
            "schema",
            "seed_derivation",
        },
        RUN_FILE_NAME,
    )
    if manifest["schema"] != RUN_SCHEMA or manifest["artifact_schema_version"] != 5:
        raise ValueError("runner run schema mismatch")
    if not json_values_equal_strict(manifest["package"], {"name": "mtg-kernel-rl", "version": __version__}):
        raise ValueError("runner package identity mismatch")
    if not json_values_equal_strict(
        manifest["artifact_schemas"], {"episode": EPISODE_SCHEMA, "run": RUN_SCHEMA}
    ):
        raise ValueError("runner artifact schemas mismatch")
    config = _require_keys(
        manifest["config"],
        {
            "base_seed",
            "deck_ids",
            "episodes",
            "max_physical_decisions",
            "max_policy_steps",
            "p0_policy",
            "p1_policy",
        },
        "config",
    )
    episodes = _int(config["episodes"], "config.episodes", minimum=1, maximum=MAX_EPISODES)
    base_seed = validate_uint63(config["base_seed"], "config.base_seed")
    max_physical_decisions = _int(
        config["max_physical_decisions"], "config.max_physical_decisions", minimum=1, maximum=MAX_PHYSICAL_DECISIONS
    )
    max_policy_steps = _int(
        config["max_policy_steps"], "config.max_policy_steps", minimum=1, maximum=MAX_POLICY_STEPS
    )
    p0 = _policy(config["p0_policy"], "config.p0_policy")
    p1 = _policy(config["p1_policy"], "config.p1_policy")
    deck_ids = _deck_ids(config["deck_ids"], "config.deck_ids")
    expected_action = {"p0": _policy_contract(p0), "p1": _policy_contract(p1)}
    if not json_values_equal_strict(manifest["action_selection"], expected_action):
        raise ValueError("runner action-selection contract mismatch")
    if not json_values_equal_strict(manifest["seed_derivation"], _seed_contract()):
        raise ValueError("runner seed-derivation contract mismatch")
    environment = _require_keys(
        manifest["environment"],
        {"binary_sha256", "deck_hashes", "deck_ids", "protocol", "protocol_provenance"},
        "environment",
    )
    env_sha = _hash(environment["binary_sha256"], "environment.binary_sha256")
    if _deck_ids(environment["deck_ids"], "environment.deck_ids") != deck_ids:
        raise ValueError("environment and configuration deck IDs differ")
    deck_hashes = _deck_hashes(environment["deck_hashes"], "environment.deck_hashes")
    expected_protocol = {
        "protocol": PROTOCOL_NAME,
        "protocol_version": PROTOCOL_VERSION,
        "schema_version": SCHEMA_VERSION,
    }
    if not json_values_equal_strict(environment["protocol"], expected_protocol):
        raise ValueError("runner environment protocol mismatch")
    provenance = _validate_provenance(environment["protocol_provenance"], "environment.protocol_provenance")
    runtime = _validate_runtime(manifest["runner_runtime_compatibility"], "runner runtime compatibility")
    policy_head = _validate_policy_source(
        manifest["policy_source"],
        p0=p0,
        p1=p1,
        env_sha=env_sha,
        deck_ids=deck_ids,
        deck_hashes=deck_hashes,
        provenance=provenance,
        runtime=runtime,
        max_physical_decisions=max_physical_decisions,
        max_policy_steps=max_policy_steps,
    )
    if not json_values_equal_strict(
        manifest["publication"],
        {
            "authoritative_file": RUN_FILE_NAME,
            "data_files_published_first": [EPISODES_FILE_NAME],
            "fresh_only": True,
            "resume": False,
        },
    ):
        raise ValueError("runner publication contract mismatch")
    return (
        episodes,
        base_seed,
        max_physical_decisions,
        max_policy_steps,
        p0,
        p1,
        deck_ids,
        deck_hashes,
        policy_head,
    )


def _jsonl_bytes(rows: list[dict[str, Any]]) -> bytes:
    if not rows:
        raise ValueError("runner must contain at least one episode row")
    chunks: list[bytes] = []
    total = 0
    for index, row in enumerate(rows):
        if index >= MAX_EPISODES:
            raise ValueError("runner episode count exceeds limit")
        validate_training_json_privacy(row, f"$.episodes[{index}]")
        data = canonical_json_bytes(row)
        if len(data) > MAX_EPISODE_ROW_BYTES:
            raise ValueError("runner episode row exceeds byte limit")
        total += len(data)
        if total > MAX_EPISODES_BYTES:
            raise ValueError("runner episodes file exceeds byte limit")
        chunks.append(data)
    return b"".join(chunks)


def _parse_jsonl(captured: CapturedFile) -> tuple[dict[str, Any], ...]:
    if not captured.data.endswith(b"\n"):
        raise ValueError("episodes.jsonl must end with LF")
    if captured.data.count(b"\n") > MAX_EPISODES:
        raise ValueError("runner episode count exceeds limit")
    raw_rows = captured.data[:-1].split(b"\n")
    if not raw_rows or any(not row for row in raw_rows):
        raise ValueError("episodes.jsonl contains an empty row")
    rows: list[dict[str, Any]] = []
    for index, row in enumerate(raw_rows):
        if len(row) + 1 > MAX_EPISODE_ROW_BYTES:
            raise ValueError("runner episode row exceeds byte limit")
        parsed = parse_canonical_json_bytes(
            row + b"\n", source=f"episodes.jsonl row {index}", max_bytes=MAX_EPISODE_ROW_BYTES
        )
        validate_training_json_privacy(parsed, f"$.episodes[{index}]")
        rows.append(parsed)
    return tuple(rows)


def _validate_episode_row(
    row: dict[str, Any],
    *,
    episode: int,
    base_seed: int,
    max_physical_decisions: int,
    max_policy_steps: int,
    p0: str,
    p1: str,
    deck_ids: tuple[str, str],
    deck_hashes: tuple[int, int],
) -> str:
    _require_keys(
        row,
        {
            "deck_hashes",
            "deck_ids",
            "env_seed",
            "episode",
            "p0_policy",
            "p1_policy",
            "physical_decision_count",
            "policy_step_count",
            "schema",
            "terminal_classification",
            "terminal_code",
            "terminal_outcome",
            "terminal_reward",
            "winner",
        },
        f"episode row {episode}",
    )
    if row["schema"] != EPISODE_SCHEMA:
        raise ValueError("runner episode schema mismatch")
    if _int(row["episode"], "episode row index", maximum=MAX_EPISODES - 1) != episode:
        raise ValueError("runner episode order mismatch")
    expected_env_seed = derive_env_seed(base_seed, episode)
    if _int(row["env_seed"], "episode env_seed", maximum=(1 << 64) - 1) != expected_env_seed:
        raise ValueError("runner episode environment seed mismatch")
    if _deck_ids(row["deck_ids"], "episode deck_ids") != deck_ids:
        raise ValueError("runner episode deck IDs mismatch")
    if _deck_hashes(row["deck_hashes"], "episode deck_hashes") != deck_hashes:
        raise ValueError("runner episode deck hashes mismatch")
    if _policy(row["p0_policy"], "episode p0_policy") != p0 or _policy(row["p1_policy"], "episode p1_policy") != p1:
        raise ValueError("runner episode policies mismatch")
    _int(row["policy_step_count"], "episode policy_step_count", maximum=max_policy_steps)
    _int(row["physical_decision_count"], "episode physical_decision_count", maximum=max_physical_decisions)
    _str(row["terminal_classification"], "episode terminal_classification")
    _str(row["terminal_code"], "episode terminal_code")
    outcome = _str(row["terminal_outcome"], "episode terminal_outcome")
    expected_terminal = {
        "draw": (None, [0, 0]),
        "p0_win": ("p0", [1, -1]),
        "p1_win": ("p1", [-1, 1]),
    }
    if outcome not in expected_terminal:
        raise ValueError("runner artifact contains a non-natural terminal outcome")
    winner, reward = expected_terminal[outcome]
    if row["winner"] != winner or not json_values_equal_strict(row["terminal_reward"], reward):
        raise ValueError("runner terminal outcome, winner, and reward disagree")
    return outcome


def _validate_file_entry(value: Any, captured: CapturedFile, *, rows: int) -> None:
    entry = _require_keys(value, {"row_count", "sha256", "size_bytes"}, "files.episodes.jsonl")
    if _int(entry["row_count"], "files.episodes.jsonl.row_count", minimum=1, maximum=MAX_EPISODES) != rows:
        raise ValueError("runner episodes row count metadata mismatch")
    if _hash(entry["sha256"], "files.episodes.jsonl.sha256") != captured.sha256:
        raise ValueError("runner episodes hash mismatch")
    if _int(entry["size_bytes"], "files.episodes.jsonl.size_bytes", minimum=1, maximum=MAX_EPISODES_BYTES) != captured.size:
        raise ValueError("runner episodes size mismatch")


def _validate_captured_payload(
    manifest: dict[str, Any], captured_episodes: CapturedFile
) -> tuple[int, int, int, int, str | None]:
    validate_training_json_privacy(manifest)
    (
        episode_count,
        base_seed,
        max_physical_decisions,
        max_policy_steps,
        p0,
        p1,
        deck_ids,
        deck_hashes,
        policy_head,
    ) = _validate_manifest(manifest)
    rows = _parse_jsonl(captured_episodes)
    if len(rows) != episode_count:
        raise ValueError("runner episode rows do not match configuration")
    aggregate = {"draws": 0, "episodes": episode_count, "halted": 0, "p0_wins": 0, "p1_wins": 0, "truncated": 0}
    for episode, row in enumerate(rows):
        outcome = _validate_episode_row(
            row,
            episode=episode,
            base_seed=base_seed,
            max_physical_decisions=max_physical_decisions,
            max_policy_steps=max_policy_steps,
            p0=p0,
            p1=p1,
            deck_ids=deck_ids,
            deck_hashes=deck_hashes,
        )
        aggregate[{"draw": "draws", "p0_win": "p0_wins", "p1_win": "p1_wins"}[outcome]] += 1
    if not json_values_equal_strict(manifest["aggregate"], aggregate):
        raise ValueError("runner aggregate is not derived from episode rows")
    files = _require_keys(manifest["files"], {EPISODES_FILE_NAME}, "files")
    _validate_file_entry(files[EPISODES_FILE_NAME], captured_episodes, rows=len(rows))
    return episode_count, aggregate["p0_wins"], aggregate["p1_wins"], aggregate["draws"], policy_head


def publish_runner_artifacts(
    root: str | Path,
    *,
    episodes: list[dict[str, Any]],
    manifest_without_files: dict[str, Any],
) -> ValidatedRunnerArtifacts:
    """Publish immutable episode data first and the authoritative metadata last."""

    root = ensure_real_dir(root)
    entries = scandir_no_follow(root)
    if {entry.name for entry in entries} != {OUTPUT_LOCK_FILE_NAME}:
        raise FileExistsError("fresh runner root must contain only the verified persistent lock")
    is_verified_output_lock_entry(root, entries[0])
    if "files" in manifest_without_files:
        raise ValueError("runner store owns files metadata")
    config = _require_keys(
        manifest_without_files.get("config"),
        {"base_seed", "deck_ids", "episodes", "max_physical_decisions", "max_policy_steps", "p0_policy", "p1_policy"},
        "config",
    )
    if _int(config["episodes"], "config.episodes", minimum=1, maximum=MAX_EPISODES) != len(episodes):
        raise ValueError("runner episode rows do not match configured count")
    episode_data = _jsonl_bytes(episodes)
    manifest = {
        **manifest_without_files,
        "files": {
            EPISODES_FILE_NAME: {
                "row_count": len(episodes),
                "sha256": sha256_bytes(episode_data),
                "size_bytes": len(episode_data),
            }
        },
    }
    validate_training_json_privacy(manifest)
    run_data = canonical_json_bytes(manifest)
    if len(run_data) > MAX_RUN_BYTES:
        raise ValueError("runner run manifest exceeds byte limit")
    parsed_manifest = parse_canonical_json_bytes(run_data, source=RUN_FILE_NAME, max_bytes=MAX_RUN_BYTES)
    episodes_path = root / EPISODES_FILE_NAME
    run_path = root / RUN_FILE_NAME
    inject_fault("runner_episodes_publish_before", episodes_path)
    write_bytes_atomic(episodes_path, episode_data)
    inject_fault("runner_episodes_publish_after", episodes_path)
    captured_episodes = read_regular_file_bytes(episodes_path, max_bytes=MAX_EPISODES_BYTES, allow_empty=False)
    if captured_episodes.data != episode_data:
        raise ValueError("runner episodes changed after atomic publication")
    _validate_captured_payload(parsed_manifest, captured_episodes)
    inject_fault("runner_run_publish_before", run_path)
    write_bytes_atomic(run_path, run_data)
    inject_fault("runner_run_publish_after", run_path)
    return validate_runner_artifacts(root)


def validate_runner_artifacts(root: str | Path) -> ValidatedRunnerArtifacts:
    """Validate a complete runner artifact set without launching or writing."""

    root = ensure_real_dir(root)
    entries = scandir_no_follow(root)
    expected = {OUTPUT_LOCK_FILE_NAME, EPISODES_FILE_NAME, RUN_FILE_NAME}
    actual = {entry.name for entry in entries}
    if actual != expected:
        raise ValueError(f"runner root entries mismatch: missing={sorted(expected - actual)} extra={sorted(actual - expected)}")
    by_name = {entry.name: entry for entry in entries}
    is_verified_output_lock_entry(root, by_name[OUTPUT_LOCK_FILE_NAME])
    for name in (EPISODES_FILE_NAME, RUN_FILE_NAME):
        ensure_real_file(root, root / name, reject_hardlinks=True)
    captured_run = read_regular_file_bytes(root / RUN_FILE_NAME, max_bytes=MAX_RUN_BYTES, allow_empty=False)
    manifest = parse_canonical_json_bytes(captured_run.data, source=RUN_FILE_NAME, max_bytes=MAX_RUN_BYTES)
    captured_episodes = read_regular_file_bytes(
        root / EPISODES_FILE_NAME, max_bytes=MAX_EPISODES_BYTES, allow_empty=False
    )
    episode_count, p0_wins, p1_wins, draws, policy_head = _validate_captured_payload(manifest, captured_episodes)
    return ValidatedRunnerArtifacts(
        run_sha256=captured_run.sha256,
        episode_count=episode_count,
        p0_wins=p0_wins,
        p1_wins=p1_wins,
        draws=draws,
        policy_head=policy_head,
    )


__all__ = [
    "EPISODES_FILE_NAME",
    "EPISODE_SCHEMA",
    "RUN_FILE_NAME",
    "RUN_SCHEMA",
    "ValidatedRunnerArtifacts",
    "publish_runner_artifacts",
    "validate_runner_artifacts",
]
