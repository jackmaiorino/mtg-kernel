#!/usr/bin/env python3
"""Shared exact GAE8 package and XMage outcome-v2 validation for V4."""

from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import re
from typing import Any, Iterator, TextIO


PACKAGE_SCHEMA = "mtg-kernel-xmage-fixed-native-state/v1"
OUTCOME_CONTRACT = "mtg-kernel-xmage-cp7-outcome-jsonl/v2"
PACKAGE_MANIFEST_FILENAME = "fixed_native_state.json"
PACKAGE_PAYLOAD_FILENAME = "checkpoint.state.f32le"
PACKAGE_MANIFEST_SHA256 = "d86a430298a5a91be324e22d798c7cdc7f9f5e61e7ffa255a559ff68287e5ab1"
PACKAGE_PAYLOAD_SHA256 = "a0b7752181a562f8e5a0821a490ce20b777b509855d754283536e8242f489b98"
PACKAGE_PAYLOAD_BYTES = 14_771_928

EXPECTED_PACKAGE_MANIFEST: dict[str, Any] = {
    "schema": PACKAGE_SCHEMA,
    "authority_kind": "current-net8-gae8-v1",
    "source_result_sha256": "dc581e2bd6548c5fa9c05d9d8ab70a2bd6f213b20dd82a042109ee9a500b628e",
    "payload": {
        "filename": PACKAGE_PAYLOAD_FILENAME,
        "byte_count": PACKAGE_PAYLOAD_BYTES,
        "adam_step": 520,
        "scorer_bias_anchor_f32_bits": 3_141_403_366,
        "payload_sha256": PACKAGE_PAYLOAD_SHA256,
        "parameters_sha256": "039c07b60df0b1e580abb875eff93f3b4e5e33c7ef7a209c400a639adefd6053",
        "first_moments_sha256": "15faca5c01c0fe055e285d46a9ae91313ac71cac80d2fe02a9ead7567a0f9aaa",
        "second_moments_sha256": "fa9221dc26881559ecd14b11ae20321ac8fd98c8063f7e57cd33f20078dc8424",
        "model_parameter_sha256": "5efe2f167045bde379da3be8af6c480b6702f5d7a849ff8435d8ac6b1d91daa8",
        "native_state_sha256": "ab7dd25ca6619a4a613ca089e1eb8e75981f8e5cfc0bae8535b78cddd7efa952",
    },
    "non_claims": [
        "external software anchor is not professional-level evidence",
        "terminal win/loss/draw is the only playing-strength outcome",
    ],
}

SOURCE_IDENTITY = {
    "source_run_sha256": "2c9b7423004428c0e2bb138afafc15ec65957f6bd98c4587bea704fbf9549aae",
    "source_generation": 384,
    "source_checkpoint_sha256": "4bd38cf3a9af3fb03fb04428fbc4286d4635007e848c7b9f0740122e430cbba8",
    "source_sidecar_sha256": "7511c0377edd4e8d918fa5843f89a0270a8264e5466c329f6b4ef18bbf9e76bb",
    "source_payload_sha256": "a6c87366b2da9fc33923abab3c0e22d70c884cd9420477df3a475117be6beb99",
    "source_train_state_sha256": "fc471f85d28293d72b42dc61de628859173bd67426e251a51bfbbe86c7d586d8",
}

EXPECTED_CHECKPOINT: dict[str, Any] = {
    "authority_kind": EXPECTED_PACKAGE_MANIFEST["authority_kind"],
    **SOURCE_IDENTITY,
    "loaded_run_sha256": SOURCE_IDENTITY["source_run_sha256"],
    "loaded_generation": EXPECTED_PACKAGE_MANIFEST["payload"]["adam_step"],
    "loaded_checkpoint_sha256": PACKAGE_MANIFEST_SHA256,
    "loaded_payload_sha256": PACKAGE_PAYLOAD_SHA256,
    "loaded_train_state_sha256": EXPECTED_PACKAGE_MANIFEST["payload"]["native_state_sha256"],
    "model_parameter_sha256": EXPECTED_PACKAGE_MANIFEST["payload"]["model_parameter_sha256"],
    "environment_trajectory_contract": "environment-randomization-v2",
    "sampler_identity": "f32-q8-expq63-hamilton-splitmix64-v1",
    "sampler_contract_sha256": "276407494966b195b7c011caf984d2354484f7532161107b19ecc83388de92b6",
}

EXPECTED_HEADER: dict[str, Any] = {
    "record_type": "header",
    "schema_version": 2,
    "record_ordinal": 0,
    "export_contract": OUTCOME_CONTRACT,
    "selection_source": "candidate_checkpoint_policy",
    "tensorizer_identity": "mtg-kernel-python-encoded-decision-tensor-contract-v2",
    "tensorizer_features_source_sha256": "fce419176dbd15e2b911e5c5f688bb390e731e3817da142571f38b1a7cc778eb",
    "model_input_commitment": "mtg-kernel-checkpoint-shadow-model-input-framed-sha256/v1",
    "checkpoint": EXPECTED_CHECKPOINT,
}

DECISION_KEYS = {
    "record_type",
    "schema_version",
    "record_ordinal",
    "outcome_decision_ordinal",
    "pair_index",
    "episode_id",
    "candidate_seat",
    "base_seed_u64_hex",
    "pair_environment_seed_u64_hex",
    "deck_ids",
    "environment_revision",
    "randomization_identity",
    "selection_source",
    "acting_player",
    "step",
    "decision_kind",
    "physical_decision_id",
    "actor_physical_decision_ordinal",
    "substep_index",
    "substep_count",
    "legal_action_count",
    "selected_index",
    "selected_semantic",
    "candidate_order_commitment_128_hex",
    "action_semantics",
    "tensor",
    "model_input_sha256",
    "old_policy_logits_f32_bits",
    "old_value_f32_bits",
    "checkpoint",
}

TERMINAL_KEYS = {
    "record_type",
    "schema_version",
    "record_ordinal",
    "pair_index",
    "episode_id",
    "candidate_seat",
    "base_seed_u64_hex",
    "pair_environment_seed_u64_hex",
    "deck_ids",
    "randomization_identity",
    "core_environment_hash_u64_hex",
    "diagnostic_state_hash_u64_hex",
    "first_outcome_decision_ordinal",
    "outcome_decision_count",
    "terminal",
    "candidate_terminal_reward",
    "checkpoint",
}

HEX_16 = re.compile(r"[0-9a-f]{16}\Z")
HEX_32 = re.compile(r"[0-9a-f]{32}\Z")
HEX_64 = re.compile(r"[0-9a-f]{64}\Z")


def fail(message: str) -> None:
    raise ValueError(message)


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def canonical_json_bytes(value: Any, *, indent: int | None = None) -> bytes:
    if indent is None:
        text = json.dumps(
            value,
            ensure_ascii=False,
            allow_nan=False,
            sort_keys=True,
            separators=(",", ":"),
        )
    else:
        text = json.dumps(
            value,
            ensure_ascii=False,
            allow_nan=False,
            sort_keys=True,
            indent=indent,
        )
    return text.encode("utf-8") + b"\n"


def exclusive_write(path: Path, payload: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o644)
    try:
        with os.fdopen(descriptor, "wb") as handle:
            handle.write(payload)
            handle.flush()
            os.fsync(handle.fileno())
    except BaseException:
        try:
            path.unlink()
        except FileNotFoundError:
            pass
        raise


def write_json_row(handle: TextIO, row: dict[str, Any]) -> None:
    handle.write(
        json.dumps(
            row,
            ensure_ascii=False,
            allow_nan=False,
            sort_keys=True,
            separators=(",", ":"),
        )
    )
    handle.write("\n")


def load_exact_gae8_package(root: Path) -> dict[str, Any]:
    if not root.is_dir():
        fail(f"GAE8 package root is not a directory: {root}")
    inventory = sorted(path.name for path in root.iterdir())
    if inventory != [PACKAGE_PAYLOAD_FILENAME, PACKAGE_MANIFEST_FILENAME]:
        fail(f"GAE8 package inventory is not the exact two-file package: {inventory}")
    manifest_path = root / PACKAGE_MANIFEST_FILENAME
    payload_path = root / PACKAGE_PAYLOAD_FILENAME
    manifest_bytes = manifest_path.read_bytes()
    if not manifest_bytes.endswith(b"\n") or b"\r" in manifest_bytes:
        fail("GAE8 package manifest is not canonical LF text")
    if hashlib.sha256(manifest_bytes).hexdigest() != PACKAGE_MANIFEST_SHA256:
        fail("GAE8 package manifest SHA-256 mismatch")
    try:
        manifest = json.loads(manifest_bytes)
    except json.JSONDecodeError as error:
        fail(f"GAE8 package manifest is invalid JSON: {error}")
    if manifest != EXPECTED_PACKAGE_MANIFEST:
        fail("GAE8 package manifest semantic identity mismatch")
    if payload_path.stat().st_size != PACKAGE_PAYLOAD_BYTES:
        fail("GAE8 package payload byte count mismatch")
    if sha256(payload_path) != PACKAGE_PAYLOAD_SHA256:
        fail("GAE8 package payload SHA-256 mismatch")
    return {
        "root": str(root.resolve()),
        "inventory": inventory,
        "authority_kind": EXPECTED_PACKAGE_MANIFEST["authority_kind"],
        "manifest_sha256": PACKAGE_MANIFEST_SHA256,
        "payload_sha256": PACKAGE_PAYLOAD_SHA256,
        "payload_bytes": PACKAGE_PAYLOAD_BYTES,
        "adam_step": EXPECTED_PACKAGE_MANIFEST["payload"]["adam_step"],
        "train_state_sha256": EXPECTED_PACKAGE_MANIFEST["payload"]["native_state_sha256"],
        "model_parameter_sha256": EXPECTED_PACKAGE_MANIFEST["payload"]["model_parameter_sha256"],
        "source_result_sha256": EXPECTED_PACKAGE_MANIFEST["source_result_sha256"],
        "checkpoint": EXPECTED_CHECKPOINT,
    }


def iter_jsonl(path: Path) -> Iterator[dict[str, Any]]:
    saw_row = False
    with path.open("rb") as handle:
        for line_number, raw in enumerate(handle, 1):
            saw_row = True
            if not raw.endswith(b"\n") or b"\r" in raw:
                fail(f"{path}:{line_number}: row is not canonical LF text")
            if raw == b"\n":
                fail(f"{path}:{line_number}: empty row")
            try:
                row = json.loads(raw)
            except (UnicodeDecodeError, json.JSONDecodeError) as error:
                fail(f"{path}:{line_number}: invalid JSON: {error}")
            if not isinstance(row, dict):
                fail(f"{path}:{line_number}: row is not an object")
            yield row
    if not saw_row:
        fail(f"{path}: empty outcome shard")


def _is_plain_int(value: Any) -> bool:
    return isinstance(value, int) and not isinstance(value, bool)


def _validate_common_row(
    path: Path,
    row: dict[str, Any],
    ordinal: int,
    base_seed_hex: str,
    first_pair: int,
    pair_count: int,
) -> tuple[int, int, str]:
    if row.get("schema_version") != 2 or row.get("record_ordinal") != ordinal:
        fail(f"{path}: invalid schema or record ordinal at record {ordinal}")
    if row.get("checkpoint") != EXPECTED_CHECKPOINT:
        fail(f"{path}: checkpoint identity mismatch at record {ordinal}")
    pair = row.get("pair_index")
    episode = row.get("episode_id")
    seat = row.get("candidate_seat")
    if (
        not _is_plain_int(pair)
        or not first_pair <= pair < first_pair + pair_count
        or not _is_plain_int(episode)
        or episode != pair * 2 + (0 if seat == "p0" else 1 if seat == "p1" else -1)
        or row.get("base_seed_u64_hex") != base_seed_hex
        or not isinstance(row.get("pair_environment_seed_u64_hex"), str)
        or HEX_16.fullmatch(row["pair_environment_seed_u64_hex"]) is None
    ):
        fail(f"{path}: pair, episode, seat, or seed identity mismatch at record {ordinal}")
    return pair, episode, seat


def _validate_decision(path: Path, row: dict[str, Any], ordinal: int) -> None:
    if set(row) != DECISION_KEYS:
        fail(f"{path}: outcome-v2 decision fields differ at record {ordinal}")
    legal_count = row.get("legal_action_count")
    selected = row.get("selected_index")
    semantics = row.get("action_semantics")
    logits = row.get("old_policy_logits_f32_bits")
    if (
        row.get("record_type") != "decision"
        or row.get("selection_source") != "candidate_checkpoint_policy"
        or not _is_plain_int(row.get("outcome_decision_ordinal"))
        or not _is_plain_int(legal_count)
        or legal_count < 1
        or not _is_plain_int(selected)
        or not 0 <= selected < legal_count
        or not isinstance(semantics, list)
        or len(semantics) != legal_count
        or row.get("selected_semantic") != semantics[selected]
        or not isinstance(logits, list)
        or len(logits) != legal_count
        or any(not _is_plain_int(value) for value in logits)
        or not isinstance(row.get("tensor"), dict)
        or not isinstance(row.get("model_input_sha256"), str)
        or HEX_64.fullmatch(row["model_input_sha256"]) is None
        or not isinstance(row.get("candidate_order_commitment_128_hex"), str)
        or HEX_32.fullmatch(row["candidate_order_commitment_128_hex"]) is None
        or not _is_plain_int(row.get("old_value_f32_bits"))
        or row.get("acting_player") != row.get("candidate_seat")
        or not _is_plain_int(row.get("physical_decision_id"))
        or row["physical_decision_id"] < 0
        or not _is_plain_int(row.get("actor_physical_decision_ordinal"))
        or row["actor_physical_decision_ordinal"] < 0
        or not _is_plain_int(row.get("substep_index"))
        or not _is_plain_int(row.get("substep_count"))
        or row["substep_count"] < 1
        or not 0 <= row["substep_index"] < row["substep_count"]
    ):
        fail(f"{path}: malformed outcome-v2 decision at record {ordinal}")


def _validate_terminal(path: Path, row: dict[str, Any], ordinal: int) -> None:
    if set(row) != TERMINAL_KEYS:
        fail(f"{path}: outcome-v2 terminal fields differ at record {ordinal}")
    terminal = row.get("terminal")
    reward = row.get("candidate_terminal_reward")
    if not isinstance(terminal, dict):
        fail(f"{path}: terminal payload is missing at record {ordinal}")
    outcome = terminal.get("terminal_outcome")
    expected = {
        "p0_win": ("p0", [1, -1]),
        "p1_win": ("p1", [-1, 1]),
        "draw": (None, [0, 0]),
    }.get(outcome)
    seat_index = 0 if row.get("candidate_seat") == "p0" else 1
    if (
        row.get("record_type") != "terminal"
        or terminal.get("schema_version") != 5
        or terminal.get("episode_id") != row.get("episode_id")
        or terminal.get("terminal_classification") != "natural"
        or terminal.get("terminal_code") != "natural_game_over"
        or terminal.get("terminal_reason") != "game_over"
        or expected is None
        or terminal.get("winner") != expected[0]
        or terminal.get("terminal_reward") != expected[1]
        or reward not in (-1, 0, 1)
        or expected[1][seat_index] != reward
        or not _is_plain_int(row.get("outcome_decision_count"))
        or row["outcome_decision_count"] < 0
    ):
        fail(f"{path}: terminal is not an exact natural outcome at record {ordinal}")


def validate_outcome_shard(
    path: Path,
    *,
    base_seed: int,
    first_pair: int,
    pair_count: int,
) -> dict[str, Any]:
    if not path.is_file() or not 0 <= base_seed <= 0x7FFF_FFFF_FFFF_FFFF:
        fail(f"invalid outcome path or base seed: {path}")
    if first_pair < 0 or not 1 <= pair_count <= 4096:
        fail("invalid shard pair range")
    base_seed_hex = f"{base_seed:016x}"
    header: dict[str, Any] | None = None
    decision_ordinal = 0
    active_episode: int | None = None
    active_first_decision: int | None = None
    active_decision_count = 0
    expected_episode = first_pair * 2
    terminal_keys: set[tuple[int, int, str]] = set()
    environment_seeds: dict[int, str] = {}
    record_count = 0
    digest = hashlib.sha256()

    with path.open("rb") as handle:
        for line_number, raw in enumerate(handle, 1):
            digest.update(raw)
            if not raw.endswith(b"\n") or b"\r" in raw:
                fail(f"{path}:{line_number}: row is not canonical LF text")
            if raw == b"\n":
                fail(f"{path}:{line_number}: empty row")
            try:
                row = json.loads(raw)
            except (UnicodeDecodeError, json.JSONDecodeError) as error:
                fail(f"{path}:{line_number}: invalid JSON: {error}")
            if not isinstance(row, dict):
                fail(f"{path}:{line_number}: row is not an object")
            ordinal = record_count
            record_count += 1
            if ordinal == 0:
                if row != EXPECTED_HEADER:
                    fail(f"{path}: first row is not the exact GAE8 outcome-v2 header")
                header = row
                continue
            if row.get("record_type") == "header":
                fail(f"{path}: duplicate header at record {ordinal}")
            pair, episode, seat = _validate_common_row(
                path, row, ordinal, base_seed_hex, first_pair, pair_count
            )
            if episode != expected_episode:
                fail(f"{path}: episode rows are missing, reordered, or interleaved")
            environment_seed = row["pair_environment_seed_u64_hex"]
            if pair in environment_seeds and environment_seeds[pair] != environment_seed:
                fail(f"{path}: pair environment seed changed for pair {pair}")
            environment_seeds[pair] = environment_seed
            record_type = row.get("record_type")
            if record_type == "decision":
                _validate_decision(path, row, ordinal)
                if active_episode is None:
                    active_episode = episode
                    active_first_decision = decision_ordinal
                    active_decision_count = 0
                elif active_episode != episode:
                    fail(f"{path}: decisions are interleaved between episodes")
                if row["outcome_decision_ordinal"] != decision_ordinal:
                    fail(f"{path}: non-contiguous outcome decision ordinal")
                decision_ordinal += 1
                active_decision_count += 1
            elif record_type == "terminal":
                _validate_terminal(path, row, ordinal)
                if active_episode is None:
                    active_episode = episode
                    active_first_decision = None
                    active_decision_count = 0
                if active_episode != episode:
                    fail(f"{path}: terminal does not close the active episode")
                if (
                    row.get("first_outcome_decision_ordinal") != active_first_decision
                    or row.get("outcome_decision_count") != active_decision_count
                ):
                    fail(f"{path}: terminal decision range mismatch")
                key = (pair, episode, seat)
                if key in terminal_keys:
                    fail(f"{path}: duplicate terminal {key}")
                terminal_keys.add(key)
                expected_episode += 1
                active_episode = None
                active_first_decision = None
                active_decision_count = 0
            else:
                fail(f"{path}: unknown record type at record {ordinal}")

    if header is None or record_count == 0:
        fail(f"{path}: empty outcome shard")
    expected_terminals = {
        (pair, pair * 2 + seat, f"p{seat}")
        for pair in range(first_pair, first_pair + pair_count)
        for seat in (0, 1)
    }
    if active_episode is not None or terminal_keys != expected_terminals:
        fail(f"{path}: terminal pair, episode, or seat coverage mismatch")
    if set(environment_seeds) != set(range(first_pair, first_pair + pair_count)):
        fail(f"{path}: pair environment seed coverage mismatch")
    return {
        "path": str(path.resolve()),
        "sha256": digest.hexdigest(),
        "byte_count": path.stat().st_size,
        "first_pair": first_pair,
        "pair_count": pair_count,
        "episode_count": len(terminal_keys),
        "decision_count": decision_ordinal,
        "record_count": record_count,
        "header": header,
    }
