"""Seeded, untrained Net8 parameter interchange for explicit native V3 bootstrap.

This module does not run games, create Adam, or load a learned checkpoint. Its
target card hash is a request for the native loader to verify against compiled
recipes. Original common-snapshot generation and readers remain unchanged.
"""
from __future__ import annotations

import argparse
import copy
import hashlib
import json
import math
import os
import platform
import re
import stat
import struct
import subprocess
import sys
from pathlib import Path, PurePosixPath, PureWindowsPath
from typing import Any

REQUEST_SCHEMA = "mtg-kernel-fresh-initialization-request/v1"
SCHEMA = "mtg-kernel-fresh-initialization/v1"
JSON_CAP = 1024 * 1024
SOURCE_CAP = 16 * 1024 * 1024
RUNTIME_CAP = 2 * 1024 * 1024 * 1024
PAYLOAD_BYTES = 4_923_976
ELEMENTS = 1_230_994
ENCODING = "ieee-754-binary32-little-endian"
LAYOUT = "torch-named-parameters-c-contiguous-row-major-linear-output-input-no-padding-v1"
TOKEN_RULE = "card-token=id+1; padding=0"
# V3 (frozen, `features_v6.py`) source pin inventory. Kept as `SOURCE_PATHS`
# too, unchanged, for every existing caller of that name.
SOURCE_PATHS_V3 = (
    "python/mtg_kernel_rl/phase1_fresh_initialization_v1.py",
    "python/mtg_kernel_rl/model.py",
    "python/mtg_kernel_rl/features.py",
    "python/mtg_kernel_rl/determinism.py",
    "python/mtg_kernel_rl/common_model_snapshot_v1.py",
    "python/mtg_kernel_rl/features_v6.py",
    "data/flat_policy_v3/feature_contract_v3.json",
    "data/cards_v1.json",
)
SOURCE_PATHS = SOURCE_PATHS_V3
# V4 (fresh-lineage, `features_v7.py`) sibling of `SOURCE_PATHS_V3`. Only
# indices 5 and 6 (the feature-source and feature-contract pins) differ.
SOURCE_PATHS_V4 = (
    "python/mtg_kernel_rl/phase1_fresh_initialization_v1.py",
    "python/mtg_kernel_rl/model.py",
    "python/mtg_kernel_rl/features.py",
    "python/mtg_kernel_rl/determinism.py",
    "python/mtg_kernel_rl/common_model_snapshot_v1.py",
    "python/mtg_kernel_rl/features_v7.py",
    "data/flat_policy_v4/feature_contract_v4.json",
    "data/cards_v1.json",
)
TARGET_KEYS = {"registry", "card_db_hash", "feature_contract_digest",
               "feature_encoding_digest", "features_source_sha256", "feature_descriptor_sha256"}
LAYOUT_KEYS = ("ordinal", "name", "shape", "byte_offset", "byte_count")


class FreshInitializationErrorV1(ValueError):
    pass


def _require(condition: bool, message: str) -> None:
    if not condition:
        raise FreshInitializationErrorV1(message)


def canonical_json_v1(value: Any) -> bytes:
    try:
        data = json.dumps(value, sort_keys=True, separators=(",", ":"),
                          ensure_ascii=True, allow_nan=False).encode("ascii")
    except (TypeError, ValueError, RecursionError) as exc:
        raise FreshInitializationErrorV1("invalid finite JSON value") from exc
    _require(len(data) <= JSON_CAP, "JSON exceeds 1 MiB")
    return data


def _object(value: Any, keys: set[str], label: str) -> dict:
    _require(type(value) is dict and set(value) == keys, f"{label} fields differ")
    return value


def _integer(value: Any, low: int, high: int, label: str) -> int:
    _require(type(value) is int and low <= value <= high, f"{label} integer out of range")
    return value


def _hex(value: Any, digits: int = 64) -> str:
    _require(type(value) is str and re.fullmatch(f"[0-9a-f]{{{digits}}}", value) is not None,
             "invalid lowercase hexadecimal identity")
    return value


def _absolute(value: Any) -> str:
    _require(type(value) is str and 0 < len(value) <= 4096 and "\x00" not in value,
             "invalid file path")
    _require(PurePosixPath(value).is_absolute() or PureWindowsPath(value).is_absolute(),
             "file path must be absolute")
    return value


def _file_pin(value: Any, sized: bool = False) -> None:
    _object(value, {"path", "sha256", "bytes"} if sized else {"path", "sha256"}, "file pin")
    _absolute(value["path"])
    _hex(value["sha256"])
    if sized:
        _integer(value["bytes"], 1, RUNTIME_CAP, "file bytes")


def parse_json_v1(data: bytes, *, cap: int = JSON_CAP) -> Any:
    _require(type(data) is bytes and len(data) <= cap, "JSON exceeds byte bound")

    def pairs(items: list[tuple[str, Any]]) -> dict:
        out = {}
        for key, value in items:
            _require(key not in out, "duplicate JSON key")
            out[key] = value
        return out

    def constant(_: str) -> None:
        raise FreshInitializationErrorV1("non-finite JSON token")

    def floating(token: str) -> float:
        value = float(token)
        _require(math.isfinite(value), "non-finite JSON number")
        return value

    try:
        return json.loads(data, object_pairs_hook=pairs, parse_constant=constant, parse_float=floating)
    except (ValueError, UnicodeError, RecursionError) as exc:
        raise FreshInitializationErrorV1("invalid strict JSON") from exc


def validate_request_v1(request: dict) -> dict:
    canonical_json_v1(request)
    _object(request, {"schema", "lineage_id", "base_seed", "target"}, "request")
    _require(request["schema"] == REQUEST_SCHEMA, "request schema differs")
    _require(type(request["lineage_id"]) is str
             and re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_.-]{0,127}", request["lineage_id"]) is not None,
             "invalid lineage identifier")
    _integer(request["base_seed"], 0, (1 << 63) - 1, "base seed")
    target = _object(request["target"], TARGET_KEYS, "target")
    _file_pin(target["registry"])
    _hex(target["card_db_hash"], 16)
    for key in TARGET_KEYS - {"registry", "card_db_hash"}:
        _hex(target[key])
    return copy.deepcopy(request)


def _read(path: Path, cap: int) -> bytes:
    with path.open("rb") as stream:
        info = os.fstat(stream.fileno())
        _require(stat.S_ISREG(info.st_mode) and info.st_size <= cap, "input not bounded regular file")
        data = stream.read(cap + 1)
    _require(len(data) <= cap, "input exceeds cap")
    return data


def _pin(path: Path, cap: int = SOURCE_CAP) -> dict:
    resolved = path.resolve(strict=True)
    digest = hashlib.sha256()
    count = 0
    with resolved.open("rb") as stream:
        before = os.fstat(stream.fileno())
        _require(stat.S_ISREG(before.st_mode) and before.st_size <= cap, "pin file exceeds bounds")
        while chunk := stream.read(1024 * 1024):
            count += len(chunk)
            _require(count <= cap, "pin file grew beyond bound")
            digest.update(chunk)
        after = os.fstat(stream.fileno())
    _require((before.st_dev, before.st_ino, before.st_size, before.st_mtime_ns)
             == (after.st_dev, after.st_ino, after.st_size, after.st_mtime_ns)
             and count == before.st_size, "file changed during hashing")
    return {"path": resolved.as_posix(), "sha256": digest.hexdigest(), "bytes": count}


def _sources(root: Path, source_paths: tuple[str, ...] = SOURCE_PATHS_V3) -> list[dict]:
    return [{**_pin(root / relative), "path": relative} for relative in source_paths]


def _git(root: Path) -> tuple[str, bool]:
    def run(arguments: list[str]) -> str:
        result = subprocess.run(["git", "-C", str(root), *arguments], capture_output=True,
                                timeout=10, check=False)
        _require(result.returncode == 0 and len(result.stdout) <= JSON_CAP, "git source query failed")
        return result.stdout.decode("utf-8", errors="strict").strip()
    head = run(["rev-parse", "HEAD"])
    _hex(head, 40)
    return head, not run(["status", "--porcelain=v1", "--untracked-files=all"])


def _runtime(torch: Any) -> dict:
    _require(sys.byteorder == "little", "producer requires little-endian runtime")
    system = platform.system()
    library_name = {"Windows": "torch_cpu.dll", "Linux": "libtorch_cpu.so"}.get(system)
    _require(library_name is not None, "producer supports Windows/Linux CPU runtime only")
    library = Path(torch.__file__).resolve().parent / "lib" / library_name
    return {
        "python_version": platform.python_version(), "python_implementation": platform.python_implementation(),
        "platform_system": system, "platform_machine": platform.machine(), "byte_order": sys.byteorder,
        "torch_version": str(torch.__version__), "device": "cpu", "dtype": str(torch.get_default_dtype()),
        "deterministic_algorithms": torch.are_deterministic_algorithms_enabled(),
        "num_threads": torch.get_num_threads(), "num_interop_threads": torch.get_num_interop_threads(),
        "python_executable": _pin(Path(sys.executable), RUNTIME_CAP),
        "torch_C": _pin(Path(torch._C.__file__), RUNTIME_CAP),
        "torch_cpu_library": _pin(library, RUNTIME_CAP),
    }


def _layout() -> tuple:
    # Reuse only the existing exact layout, never its fixed-seed generator or validator.
    from .common_model_snapshot_v1 import EXPECTED_PARAMETER_LAYOUT_V1
    _require(len(EXPECTED_PARAMETER_LAYOUT_V1) == 33, "shared layout tensor count changed")
    return EXPECTED_PARAMETER_LAYOUT_V1


def _parameter_records(payload: bytes) -> tuple[list[dict], dict, int]:
    _require(type(payload) is bytes and len(payload) == PAYLOAD_BYTES, "parameter payload byte count differs")
    _require(payload[:64] == bytes(64), "padding row must contain positive zero")
    _require(all(math.isfinite(value[0]) for value in struct.iter_unpack("<f", payload)),
             "non-finite parameter")
    rows = []
    named = hashlib.sha256()
    offset = 0
    anchor = None
    for ordinal, (name, shape, element_offset, element_count) in enumerate(_layout()):
        _require(element_offset * 4 == offset and math.prod(shape) == element_count,
                 "shared layout is not contiguous")
        data = payload[offset:offset + element_count * 4]
        _require(len(data) == element_count * 4, "truncated tensor")
        row = {"ordinal": ordinal, "name": name, "shape": list(shape), "byte_offset": offset,
               "byte_count": len(data), "sha256": hashlib.sha256(data).hexdigest()}
        rows.append(row)
        name_bytes = name.encode("ascii")
        named.update(struct.pack(">I", len(name_bytes)))
        named.update(name_bytes)
        named.update(struct.pack(">I", len(shape)))
        for dimension in shape:
            named.update(struct.pack(">Q", dimension))
        named.update(struct.pack(">Q", element_count))
        named.update(data)
        if name == "scorer.2.bias":
            _require(len(data) == 4, "scorer anchor layout differs")
            anchor = int.from_bytes(data, "little")
        offset += len(data)
    _require(offset == PAYLOAD_BYTES and anchor is not None, "shared layout size or anchor differs")
    layout_json = [{key: row[key] for key in LAYOUT_KEYS} for row in rows]
    contract = {"file": "parameters.f32le", "encoding": ENCODING, "layout": LAYOUT,
                "bytes": PAYLOAD_BYTES, "sha256": hashlib.sha256(payload).hexdigest(),
                "model_parameter_sha256": named.hexdigest(),
                "parameter_layout_sha256": hashlib.sha256(canonical_json_v1(layout_json)).hexdigest(),
                "tensor_count": 33, "element_count": ELEMENTS}
    return rows, contract, anchor


def _bootstrap(anchor: int) -> dict:
    return {"optimizer_identity": "native-adam-canonical-scorer-bias-gauge-v1", "adam_step": 0,
            "moment_initialization": "positive-zero-f32", "scorer_bias_anchor_bits": anchor,
            "value_head_gauge": "none"}


def _initializer(seed: int) -> dict:
    from .determinism import derive_model_init_seed, TRAINER_SEED_DERIVATION_VERSION
    return {"identity": "trainer-seeded-v1", "authority": "Python KernelPolicyValueNet.reset_seeded_parameters",
            "base_seed": seed, "model_init_seed": derive_model_init_seed(seed),
            "seed_derivation": TRAINER_SEED_DERIVATION_VERSION}


def validate_initialization_bytes_v1(manifest_bytes: bytes, payload: bytes) -> dict:
    """Validate bytes without initializing weights, loading target files or creating Adam.

    This verifies the recorded contract, not that an untrusted caller really ran
    the named RNG/runtime. Actual generation repetition is separate evidence.
    """
    manifest = parse_json_v1(manifest_bytes)
    _object(manifest, {"schema", "lineage_id", "initializer", "producer", "target", "model",
                       "payload", "parameters", "optimizer_bootstrap"}, "initialization")
    _require(manifest["schema"] == SCHEMA, "initialization schema differs")
    _require(manifest_bytes == canonical_json_v1(manifest) + b"\n", "manifest encoding is not canonical")
    target = _object(manifest["target"], TARGET_KEYS | {"registry_card_count", "card_token_rule"}, "target")
    init = _object(manifest["initializer"], {"identity", "authority", "base_seed", "model_init_seed",
                                            "seed_derivation"}, "initializer")
    request = {"schema": REQUEST_SCHEMA, "lineage_id": manifest["lineage_id"], "base_seed": init["base_seed"],
               "target": {key: target[key] for key in TARGET_KEYS}}
    validate_request_v1(request)
    _integer(init["model_init_seed"], 0, (1 << 63) - 1, "model seed")
    _require(init == _initializer(init["base_seed"]), "initializer or derived seed differs")
    _integer(target["registry_card_count"], 1, 65536, "registry count")
    _require(target["card_token_rule"] == TOKEN_RULE, "card token rule differs")
    producer = _object(manifest["producer"], {"source_git_commit", "source_git_clean", "source_files", "runtime"}, "producer")
    _hex(producer["source_git_commit"], 40)
    _require(type(producer["source_git_clean"]) is bool, "git clean must be bool")
    records = producer["source_files"]
    # Classify from the producer's own recorded features-module path (index
    # 5), never a caller flag: this only selects which expected tuple the
    # exact structural zip-check below verifies against, so a malformed or
    # reordered list still fails that check regardless of which tuple was
    # picked here. Mirrors the Rust reader's dispatch on the loaded source's
    # own declared identity.
    source_paths = SOURCE_PATHS_V4 if (
        type(records) is list and len(records) > 5 and type(records[5]) is dict
        and records[5].get("path") == SOURCE_PATHS_V4[5]
    ) else SOURCE_PATHS_V3
    _require(type(records) is list and len(records) == len(source_paths), "source file count differs")
    for row, expected in zip(records, source_paths):
        _object(row, {"path", "sha256", "bytes"}, "source file")
        _require(row["path"] == expected, "source file order/path differs")
        _hex(row["sha256"])
        _integer(row["bytes"], 1, SOURCE_CAP, "source bytes")
    recorded_hashes = {row["path"]: row["sha256"] for row in records}
    _require(recorded_hashes[source_paths[5]] == target["features_source_sha256"]
             and recorded_hashes[source_paths[6]] == target["feature_descriptor_sha256"]
             and recorded_hashes[source_paths[7]] == target["registry"]["sha256"], "target/source pins differ")
    runtime = _object(producer["runtime"], {"python_version", "python_implementation", "platform_system",
        "platform_machine", "byte_order", "torch_version", "device", "dtype", "deterministic_algorithms",
        "num_threads", "num_interop_threads", "python_executable", "torch_C", "torch_cpu_library"}, "runtime")
    for key in ("python_version", "python_implementation", "platform_machine", "torch_version"):
        _require(type(runtime[key]) is str and 0 < len(runtime[key]) <= 256, "invalid runtime string")
    _require(runtime["platform_system"] in ("Windows", "Linux") and runtime["byte_order"] == "little"
             and runtime["device"] == "cpu" and runtime["dtype"] == "torch.float32"
             and runtime["deterministic_algorithms"] is True, "runtime CPU contract differs")
    for key in ("num_threads", "num_interop_threads"):
        _integer(runtime[key], 1, 1, key)
    for key in ("python_executable", "torch_C", "torch_cpu_library"):
        _file_pin(runtime[key], sized=True)
    from .model import ModelConfig
    model = _object(manifest["model"], {"architecture", "generator_model_config", "generator_model_config_sha256"}, "model")
    config = ModelConfig.from_dict(model["generator_model_config"])
    _require(model["architecture"] == "kernel-policy-value-net-8"
             and model["generator_model_config_sha256"] == hashlib.sha256(canonical_json_v1(config.to_dict())).hexdigest(),
             "generator model configuration differs")
    rows, contract, anchor = _parameter_records(payload)
    # Canonical byte equality distinguishes bool from int in all numerical DTO fields.
    for actual, expected, label in ((manifest["parameters"], rows, "parameter records"),
                                   (manifest["payload"], contract, "payload"),
                                   (manifest["optimizer_bootstrap"], _bootstrap(anchor), "optimizer bootstrap")):
        _require(canonical_json_v1(actual) == canonical_json_v1(expected), f"{label} differs")
    return manifest


def _select_generation_v1(target: dict):
    """Select which fresh-lineage generation's source-file inventory and
    features module a request's declared target belongs to, matched by each
    generation's own live fingerprint functions, never a hardcoded digest
    literal in this file. Mirrors the Rust admission's whole-tuple dispatch
    (`fresh_lineage_generation_v1`), matched against exactly the compiled V3
    or V4 contract, never a mixed pair.
    """
    from . import features_v6, features_v7
    if (target["feature_contract_digest"] == features_v6.feature_contract_fingerprint()
            and target["feature_encoding_digest"] == features_v6.encoding_contract_fingerprint()):
        return SOURCE_PATHS_V3, features_v6
    if (target["feature_contract_digest"] == features_v7.feature_contract_fingerprint()
            and target["feature_encoding_digest"] == features_v7.encoding_contract_fingerprint()):
        return SOURCE_PATHS_V4, features_v7
    raise FreshInitializationErrorV1(
        "requested target feature identity matches neither the V3 nor V4 contract")


def generate_initialization_v1(request: dict, repo_root: Path | None = None) -> tuple[bytes, bytes]:
    """Generate one explicit CPU seed. No outcome selection or training occurs."""
    request = validate_request_v1(request)
    root = (Path(__file__).resolve().parents[2] if repo_root is None else Path(repo_root).resolve())
    _require(Path(__file__).resolve() == root / SOURCE_PATHS[0], "generator root does not match loaded module")
    source_paths, features_module = _select_generation_v1(request["target"])
    sources = _sources(root, source_paths)
    git_head, git_clean = _git(root)
    import torch
    from . import model, features, determinism, common_model_snapshot_v1
    for module, relative in ((model, source_paths[1]), (features, source_paths[2]),
                             (determinism, source_paths[3]), (common_model_snapshot_v1, source_paths[4]),
                             (features_module, source_paths[5])):
        _require(Path(module.__file__).resolve() == root / relative, "loaded producer module is from another root")
    determinism.configure_torch_determinism()
    runtime = _runtime(torch)
    target = copy.deepcopy(request["target"])
    registry_bytes = _read(Path(target["registry"]["path"]), SOURCE_CAP)
    _require(hashlib.sha256(registry_bytes).hexdigest() == target["registry"]["sha256"]
             == sources[7]["sha256"], "requested registry differs from actual producer registry")
    registry = parse_json_v1(registry_bytes, cap=SOURCE_CAP)
    _require(type(registry) is dict, "registry must be an object")
    cards = registry.get("cards")
    _require(type(cards) is list and 1 <= len(cards) <= 65536, "invalid registry card count")
    names = [card.get("name") if type(card) is dict else None for card in cards]
    _require(all(type(name) is str and name for name in names) and len(set(names)) == len(names),
             "invalid/duplicate registry card names")
    descriptor_bytes = _read(root / source_paths[6], JSON_CAP)
    descriptor = parse_json_v1(descriptor_bytes)
    _require(type(descriptor) is dict, "feature descriptor must be an object")
    _require(target["feature_contract_digest"] == features_module.feature_contract_fingerprint()
             == descriptor.get("feature_contract_digest")
             and target["feature_encoding_digest"] == features_module.encoding_contract_fingerprint()
             == descriptor.get("feature_encoding_digest")
             and target["features_source_sha256"] == sources[5]["sha256"]
             == descriptor.get("features_source_sha256")
             and target["feature_descriptor_sha256"] == hashlib.sha256(descriptor_bytes).hexdigest(),
             "requested target differs from actual source/descriptor")
    target.update(registry_card_count=len(cards), card_token_rule=TOKEN_RULE)
    config = model.ModelConfig()
    config.validate()
    initializer = _initializer(request["base_seed"])
    network = model.KernelPolicyValueNet(config, initializer=model.INITIALIZER_TRAINER_SEEDED_V1,
                                         initializer_seed=initializer["model_init_seed"], configure_runtime=False)
    _require(not list(network.named_buffers()), "unexpected model buffers")
    named = list(network.named_parameters())
    _require(len(named) == 33, "model parameter count differs")
    chunks = []
    for (name, tensor), (expected_name, shape, _, _) in zip(named, _layout()):
        _require(name == expected_name and tuple(tensor.shape) == shape and tensor.dtype is torch.float32
                 and tensor.device.type == "cpu", "generated parameter layout/dtype differs")
        chunks.append(tensor.detach().contiguous().numpy().astype("<f4", copy=False).tobytes(order="C"))
    payload = b"".join(chunks)
    rows, payload_contract, anchor = _parameter_records(payload)
    manifest = {"schema": SCHEMA, "lineage_id": request["lineage_id"], "initializer": initializer,
        "producer": {"source_git_commit": git_head, "source_git_clean": git_clean,
                     "source_files": sources, "runtime": runtime},
        "target": target,
        "model": {"architecture": "kernel-policy-value-net-8", "generator_model_config": config.to_dict(),
                  "generator_model_config_sha256": hashlib.sha256(canonical_json_v1(config.to_dict())).hexdigest()},
        "payload": payload_contract, "parameters": rows, "optimizer_bootstrap": _bootstrap(anchor)}
    result = canonical_json_v1(manifest) + b"\n"
    validate_initialization_bytes_v1(result, payload)
    _require(_sources(root, source_paths) == sources and _git(root) == (git_head, git_clean)
             and _runtime(torch) == runtime, "producer source/runtime changed during generation")
    _require(hashlib.sha256(_read(Path(target["registry"]["path"]), SOURCE_CAP)).hexdigest()
             == target["registry"]["sha256"], "target registry changed during generation")
    return result, payload


def write_initialization_v1(output: Path, manifest: bytes, payload: bytes) -> dict:
    """Publish fresh files only; retain partial output on failure for inspection.

    File fsync is used everywhere; POSIX additionally fsyncs directories. This
    helper does not claim Windows directory-flush or arbitrary power-loss proof.
    """
    validate_initialization_bytes_v1(manifest, payload)
    output = Path(output)
    _require(output.is_absolute(), "output directory must be absolute")
    output.mkdir(parents=False, exist_ok=False)
    if os.name != "nt":
        fd = os.open(output.parent, os.O_RDONLY | os.O_DIRECTORY)
        try:
            os.fsync(fd)
        finally:
            os.close(fd)
    for name, data in (("parameters.f32le", payload), ("initialization.json", manifest)):
        with (output / name).open("xb") as stream:
            stream.write(data)
            stream.flush()
            os.fsync(stream.fileno())
        if os.name != "nt":
            fd = os.open(output, os.O_RDONLY | os.O_DIRECTORY)
            try:
                os.fsync(fd)
            finally:
                os.close(fd)
    result = {"initialization": _pin(output / "initialization.json", JSON_CAP),
              "parameters": _pin(output / "parameters.f32le", PAYLOAD_BYTES)}
    _require(result["initialization"]["sha256"] == hashlib.sha256(manifest).hexdigest()
             and result["parameters"]["sha256"] == hashlib.sha256(payload).hexdigest(),
             "published bytes differ from validated input")
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--request", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    _require(args.request.is_absolute(), "request path must be absolute")
    _require(args.output.is_absolute() and not args.output.exists(), "output must be fresh and absolute")
    request = parse_json_v1(_read(args.request, JSON_CAP))
    manifest, payload = generate_initialization_v1(request)
    print(json.dumps(write_initialization_v1(args.output, manifest, payload), sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
