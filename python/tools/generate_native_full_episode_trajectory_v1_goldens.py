#!/usr/bin/env python3
"""Generate portable full-episode trajectory SHA-256 goldens.

This stdlib-only authority is an independent transcription of the byte and
validation rules in ``collab/NATIVE-TRAINING-STORE-V1-DRAFT.md``.  It does not
import, invoke, inspect, or consume output from the Rust implementation.  The
portable artifact deliberately contains both admitted streams and inputs that
must fail closed, so another language can validate serialization and rejection
semantics from the contract alone.
"""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
import re
import sys
from pathlib import Path
from typing import Any, NoReturn


SCHEMA = "mtg_kernel_native_full_episode_trajectory_goldens/v1"
GENERATOR_IDENTITY = (
    "mtg-kernel-native-full-episode-trajectory-goldens-stdlib-python-v1"
)
TRAJECTORY_IDENTITY = "mtg-kernel-native-full-episode-trajectory-sha256-v1"
VECTOR_STREAM_IDENTITY = (
    "mtg-kernel-native-full-episode-trajectory-golden-vector-stream-sha256-v1"
)
OUTPUT_RELATIVE = Path("data/native_full_episode_trajectory_v1_goldens.json")
REPOSITORY_ROOT = Path(__file__).resolve().parents[2]
OUTPUT = REPOSITORY_ROOT / OUTPUT_RELATIVE

U32_MAX = (1 << 32) - 1
U63_MAX = (1 << 63) - 1
U64_MAX = (1 << 64) - 1
MAX_DECISIONS = 4_096
MAX_CASES = 256
MAX_ARTIFACT_BYTES = 4 * 1_024 * 1_024
NAME_RE = re.compile(r"[a-z0-9][a-z0-9-]{0,127}\Z")

INPUT_FIELDS = {
    "episode_index_u64_hex",
    "environment_seed_u64_hex",
    "deck_p0_id",
    "deck_p0_hash_u64_hex",
    "deck_p1_id",
    "deck_p1_hash_u64_hex",
    "learner_seat",
    "decisions",
    "terminal",
}
DECISION_FIELDS = {
    "row_ordinal_u64_hex",
    "actor_seat",
    "actor_role",
    "physical_decision_ordinal_u64_hex",
    "actor_physical_decision_ordinal_u64_hex",
    "substep_index_u32",
    "substep_count_u32",
    "action_seed_u64_hex",
    "legal_action_count_u32",
    "selected_index_u32",
    "flat_action_v2_commitment_hex",
}
TERMINAL_FIELDS = {
    "episode_index_u64_hex",
    "deck_p0_hash_u64_hex",
    "deck_p1_hash_u64_hex",
    "outcome",
    "winner",
    "classification",
    "terminal_code",
    "policy_step_count_u64_hex",
    "physical_decision_count_u64_hex",
}
REJECTION_CODES = {
    "invalid-deck-id",
    "episode-mismatch",
    "empty-decision-stream",
    "row-ordinal-mismatch",
    "actor-role-mismatch",
    "malformed-physical-group",
    "invalid-legal-action-count",
    "selected-index-out-of-range",
    "malformed-commitment",
    "non-natural-terminal",
    "terminal-provenance-mismatch",
    "terminal-count-mismatch",
}


class ContractRejection(ValueError):
    """A closed, portable rejection code from the trajectory contract."""

    def __init__(self, code: str) -> None:
        if code not in REJECTION_CODES:
            raise AssertionError(f"unknown trajectory rejection code: {code}")
        super().__init__(code)
        self.code = code


def reject(code: str) -> NoReturn:
    raise ContractRejection(code)


def sha256_hex(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def canonical_json_bytes(value: Any) -> bytes:
    """Store-CJ: ASCII-sorted compact JSON with exactly one final LF."""

    return (
        json.dumps(
            value,
            sort_keys=True,
            separators=(",", ":"),
            ensure_ascii=True,
            allow_nan=False,
        ).encode("ascii")
        + b"\n"
    )


def atom(tag: str, payload: bytes) -> bytes:
    encoded_tag = tag.encode("utf-8")
    return (
        len(encoded_tag).to_bytes(4, "big")
        + encoded_tag
        + len(payload).to_bytes(8, "big")
        + payload
    )


def require_exact_object(value: Any, fields: set[str], label: str) -> dict[str, Any]:
    if not isinstance(value, dict) or set(value) != fields:
        raise AssertionError(f"{label} does not have its exact declared fields")
    return value


def require_u32(value: Any, label: str) -> int:
    if type(value) is not int or not 0 <= value <= U32_MAX:
        raise AssertionError(f"{label} is not an admitted u32 JSON integer")
    return value


def parse_fixed_hex(value: Any, digits: int, label: str) -> int:
    if (
        not isinstance(value, str)
        or len(value) != digits
        or any(character not in "0123456789abcdef" for character in value)
    ):
        raise AssertionError(f"{label} is not exactly {digits} lowercase hex digits")
    return int(value, 16)


def u64_hex(value: int) -> str:
    if type(value) is not int or not 0 <= value <= U64_MAX:
        raise AssertionError("u64 fixture literal is out of range")
    return f"{value:016x}"


def ascii_bytes(value: Any, label: str, *, maximum: int) -> bytes:
    if not isinstance(value, str):
        raise AssertionError(f"{label} is not a string")
    try:
        encoded = value.encode("ascii")
    except UnicodeEncodeError as error:
        raise AssertionError(f"{label} is not ASCII") from error
    if len(encoded) > maximum or any(not 0x20 <= byte <= 0x7E for byte in encoded):
        raise AssertionError(f"{label} violates its outer printable-ASCII bounds")
    return encoded


def deck_id_bytes(value: Any) -> bytes:
    encoded = ascii_bytes(value, "deck ID", maximum=65)
    if not 1 <= len(encoded) <= 64:
        reject("invalid-deck-id")
    return encoded


def seat_code(value: Any) -> int:
    if value == "p0":
        return 0
    if value == "p1":
        return 1
    raise AssertionError("seat is outside the closed p0|p1 vocabulary")


def role_code(value: Any) -> int:
    if value == "learner":
        return 0
    if value == "opponent":
        return 1
    raise AssertionError("role is outside the closed learner|opponent vocabulary")


def commitment_bytes(value: Any) -> bytes:
    encoded = ascii_bytes(value, "commitment", maximum=34)
    if (
        len(encoded) != 32
        or any(character not in b"0123456789abcdef" for character in encoded)
    ):
        reject("malformed-commitment")
    return bytes.fromhex(value)


def decision_row_payload(row: dict[str, Any]) -> bytes:
    row_ordinal = parse_fixed_hex(row["row_ordinal_u64_hex"], 16, "row ordinal")
    physical_ordinal = parse_fixed_hex(
        row["physical_decision_ordinal_u64_hex"], 16, "physical ordinal"
    )
    actor_physical_ordinal = parse_fixed_hex(
        row["actor_physical_decision_ordinal_u64_hex"],
        16,
        "actor physical ordinal",
    )
    substep_index = require_u32(row["substep_index_u32"], "substep index")
    substep_count = require_u32(row["substep_count_u32"], "substep count")
    action_seed = parse_fixed_hex(row["action_seed_u64_hex"], 16, "action seed")
    legal_count = require_u32(row["legal_action_count_u32"], "legal action count")
    selected_index = require_u32(row["selected_index_u32"], "selected index")
    commitment = commitment_bytes(row["flat_action_v2_commitment_hex"])
    return b"".join(
        (
            atom("row_ordinal_u64be", row_ordinal.to_bytes(8, "big")),
            atom("actor_seat_u8", bytes((seat_code(row["actor_seat"]),))),
            atom("actor_role_u8", bytes((role_code(row["actor_role"]),))),
            atom(
                "physical_decision_ordinal_u64be",
                physical_ordinal.to_bytes(8, "big"),
            ),
            atom(
                "actor_physical_decision_ordinal_u64be",
                actor_physical_ordinal.to_bytes(8, "big"),
            ),
            atom("substep_index_u32be", substep_index.to_bytes(4, "big")),
            atom("substep_count_u32be", substep_count.to_bytes(4, "big")),
            atom("action_seed_u64be", action_seed.to_bytes(8, "big")),
            atom("legal_action_count_u32be", legal_count.to_bytes(4, "big")),
            atom("selected_index_u32be", selected_index.to_bytes(4, "big")),
            atom("flat_action_v2_commitment_raw16", commitment),
        )
    )


def terminal_codes(terminal: dict[str, Any]) -> tuple[int, int, int, int]:
    outcome_codes = {
        "p0-win": 0,
        "p1-win": 1,
        "draw": 2,
        "truncated": 3,
        "halted": 4,
    }
    winner_codes = {"none": 0, "p0": 1, "p1": 2}
    classification_codes = {"natural": 0, "truncated": 1, "halted": 2}
    terminal_codes_by_name = {
        "natural-game-over": 0,
        "decision-cap": 1,
        "fail-closed": 2,
    }
    try:
        codes = (
            outcome_codes[terminal["outcome"]],
            winner_codes[terminal["winner"]],
            classification_codes[terminal["classification"]],
            terminal_codes_by_name[terminal["terminal_code"]],
        )
    except (KeyError, TypeError) as error:
        raise AssertionError("terminal uses a value outside a closed vocabulary") from error
    if codes not in {(0, 1, 0, 0), (1, 2, 0, 0), (2, 0, 0, 0)}:
        reject("non-natural-terminal")
    return codes


def terminal_row_payload(
    codes: tuple[int, int, int, int],
    *,
    policy_count: int,
    physical_count: int,
    learner_policy_count: int,
    opponent_policy_count: int,
    learner_physical_count: int,
    opponent_physical_count: int,
) -> bytes:
    return b"".join(
        (
            atom("terminal_outcome_u8", bytes((codes[0],))),
            atom("winner_option_u8", bytes((codes[1],))),
            atom("terminal_classification_u8", bytes((codes[2],))),
            atom("terminal_code_u8", bytes((codes[3],))),
            atom("policy_step_count_u64be", policy_count.to_bytes(8, "big")),
            atom(
                "physical_decision_count_u64be", physical_count.to_bytes(8, "big")
            ),
            atom(
                "learner_policy_step_count_u64be",
                learner_policy_count.to_bytes(8, "big"),
            ),
            atom(
                "opponent_policy_step_count_u64be",
                opponent_policy_count.to_bytes(8, "big"),
            ),
            atom(
                "learner_physical_decision_count_u64be",
                learner_physical_count.to_bytes(8, "big"),
            ),
            atom(
                "opponent_physical_decision_count_u64be",
                opponent_physical_count.to_bytes(8, "big"),
            ),
        )
    )


def evaluate_case(input_record: Any) -> tuple[bytes, dict[str, Any]]:
    """Validate and encode one case without consulting production code."""

    record = require_exact_object(input_record, INPUT_FIELDS, "trajectory input")
    episode_index = parse_fixed_hex(
        record["episode_index_u64_hex"], 16, "episode index"
    )
    if episode_index > U63_MAX:
        raise AssertionError("episode index exceeds the declared positive-u63 ceiling")
    environment_seed = parse_fixed_hex(
        record["environment_seed_u64_hex"], 16, "environment seed"
    )
    deck_p0_id = deck_id_bytes(record["deck_p0_id"])
    deck_p1_id = deck_id_bytes(record["deck_p1_id"])
    deck_p0_hash = parse_fixed_hex(
        record["deck_p0_hash_u64_hex"], 16, "deck P0 hash"
    )
    deck_p1_hash = parse_fixed_hex(
        record["deck_p1_hash_u64_hex"], 16, "deck P1 hash"
    )
    learner_seat = record["learner_seat"]
    learner_seat_encoded = seat_code(learner_seat)
    decisions = record["decisions"]
    if not isinstance(decisions, list) or len(decisions) > MAX_DECISIONS:
        raise AssertionError("decisions is not an array inside the declared cap")

    stream_parts = [
        atom("domain", TRAJECTORY_IDENTITY.encode("ascii")),
        atom("episode_index_u64be", episode_index.to_bytes(8, "big")),
        atom("environment_seed_u64be", environment_seed.to_bytes(8, "big")),
        atom("deck_p0_id_utf8", deck_p0_id),
        atom("deck_p0_hash_u64be", deck_p0_hash.to_bytes(8, "big")),
        atom("deck_p1_id_utf8", deck_p1_id),
        atom("deck_p1_hash_u64be", deck_p1_hash.to_bytes(8, "big")),
        atom("learner_seat_u8", bytes((learner_seat_encoded,))),
    ]

    learner_policy_count = 0
    opponent_policy_count = 0
    learner_physical_count = 0
    opponent_physical_count = 0
    open_group: tuple[str, str, int, int, int, int] | None = None

    for expected_row_ordinal, raw_row in enumerate(decisions):
        row = require_exact_object(raw_row, DECISION_FIELDS, "decision")
        row_ordinal = parse_fixed_hex(
            row["row_ordinal_u64_hex"], 16, "row ordinal"
        )
        if row_ordinal != expected_row_ordinal:
            reject("row-ordinal-mismatch")
        actor_seat = row["actor_seat"]
        seat_code(actor_seat)
        actor_role = row["actor_role"]
        role_code(actor_role)
        expected_role = "learner" if actor_seat == learner_seat else "opponent"
        if actor_role != expected_role:
            reject("actor-role-mismatch")
        legal_count = require_u32(
            row["legal_action_count_u32"], "legal action count"
        )
        if not 1 <= legal_count <= 64:
            reject("invalid-legal-action-count")
        selected_index = require_u32(row["selected_index_u32"], "selected index")
        if selected_index >= legal_count:
            reject("selected-index-out-of-range")
        substep_index = require_u32(row["substep_index_u32"], "substep index")
        substep_count = require_u32(row["substep_count_u32"], "substep count")
        if substep_count == 0 or substep_index >= substep_count:
            reject("malformed-physical-group")
        physical_ordinal = parse_fixed_hex(
            row["physical_decision_ordinal_u64_hex"], 16, "physical ordinal"
        )
        actor_physical_ordinal = parse_fixed_hex(
            row["actor_physical_decision_ordinal_u64_hex"],
            16,
            "actor physical ordinal",
        )

        if open_group is None:
            expected_physical = learner_physical_count + opponent_physical_count
            expected_actor_physical = (
                learner_physical_count
                if actor_role == "learner"
                else opponent_physical_count
            )
            if (
                substep_index != 0
                or physical_ordinal != expected_physical
                or actor_physical_ordinal != expected_actor_physical
            ):
                reject("malformed-physical-group")
        else:
            expected_open = (
                actor_seat,
                actor_role,
                physical_ordinal,
                actor_physical_ordinal,
                substep_count,
                substep_index,
            )
            if expected_open != open_group:
                reject("malformed-physical-group")

        # Validate the fixed commitment before committing the row bytes.
        payload = decision_row_payload(row)
        stream_parts.append(atom("decision_row", payload))
        if actor_role == "learner":
            learner_policy_count += 1
        else:
            opponent_policy_count += 1
        if substep_index + 1 == substep_count:
            if actor_role == "learner":
                learner_physical_count += 1
            else:
                opponent_physical_count += 1
            open_group = None
        else:
            open_group = (
                actor_seat,
                actor_role,
                physical_ordinal,
                actor_physical_ordinal,
                substep_count,
                substep_index + 1,
            )

    if not decisions:
        reject("empty-decision-stream")
    if open_group is not None:
        reject("malformed-physical-group")

    terminal = require_exact_object(record["terminal"], TERMINAL_FIELDS, "terminal")
    terminal_episode = parse_fixed_hex(
        terminal["episode_index_u64_hex"], 16, "terminal episode index"
    )
    if terminal_episode != episode_index:
        reject("episode-mismatch")
    terminal_p0_hash = parse_fixed_hex(
        terminal["deck_p0_hash_u64_hex"], 16, "terminal deck P0 hash"
    )
    terminal_p1_hash = parse_fixed_hex(
        terminal["deck_p1_hash_u64_hex"], 16, "terminal deck P1 hash"
    )
    if (terminal_p0_hash, terminal_p1_hash) != (deck_p0_hash, deck_p1_hash):
        reject("terminal-provenance-mismatch")
    codes = terminal_codes(terminal)
    terminal_policy_count = parse_fixed_hex(
        terminal["policy_step_count_u64_hex"], 16, "terminal policy count"
    )
    terminal_physical_count = parse_fixed_hex(
        terminal["physical_decision_count_u64_hex"], 16, "terminal physical count"
    )
    policy_count = learner_policy_count + opponent_policy_count
    physical_count = learner_physical_count + opponent_physical_count
    if (
        terminal_policy_count != policy_count
        or terminal_physical_count != physical_count
        or policy_count != len(decisions)
    ):
        reject("terminal-count-mismatch")

    terminal_payload = terminal_row_payload(
        codes,
        policy_count=policy_count,
        physical_count=physical_count,
        learner_policy_count=learner_policy_count,
        opponent_policy_count=opponent_policy_count,
        learner_physical_count=learner_physical_count,
        opponent_physical_count=opponent_physical_count,
    )
    stream_parts.append(atom("terminal_row", terminal_payload))
    return b"".join(stream_parts), {
        "episode_index": episode_index,
        "environment_seed": environment_seed,
        "deck_hashes": (deck_p0_hash, deck_p1_hash),
        "learner_seat": learner_seat,
        "policy_step_count": policy_count,
        "physical_decision_count": physical_count,
        "learner_policy_step_count": learner_policy_count,
        "opponent_policy_step_count": opponent_policy_count,
        "learner_physical_decision_count": learner_physical_count,
        "opponent_physical_decision_count": opponent_physical_count,
    }


def decision(
    row_ordinal: int,
    actor_seat: str,
    actor_role: str,
    physical_ordinal: int,
    actor_physical_ordinal: int,
    substep_index: int,
    substep_count: int,
    action_seed: int,
    legal_count: int,
    selected_index: int,
    commitment_hex: str,
) -> dict[str, Any]:
    return {
        "row_ordinal_u64_hex": u64_hex(row_ordinal),
        "actor_seat": actor_seat,
        "actor_role": actor_role,
        "physical_decision_ordinal_u64_hex": u64_hex(physical_ordinal),
        "actor_physical_decision_ordinal_u64_hex": u64_hex(
            actor_physical_ordinal
        ),
        "substep_index_u32": substep_index,
        "substep_count_u32": substep_count,
        "action_seed_u64_hex": u64_hex(action_seed),
        "legal_action_count_u32": legal_count,
        "selected_index_u32": selected_index,
        "flat_action_v2_commitment_hex": commitment_hex,
    }


def terminal(
    episode_index: int,
    deck_hashes: tuple[int, int],
    outcome: str,
    winner: str,
    policy_count: int,
    physical_count: int,
) -> dict[str, Any]:
    return {
        "episode_index_u64_hex": u64_hex(episode_index),
        "deck_p0_hash_u64_hex": u64_hex(deck_hashes[0]),
        "deck_p1_hash_u64_hex": u64_hex(deck_hashes[1]),
        "outcome": outcome,
        "winner": winner,
        "classification": "natural",
        "terminal_code": "natural-game-over",
        "policy_step_count_u64_hex": u64_hex(policy_count),
        "physical_decision_count_u64_hex": u64_hex(physical_count),
    }


def trajectory_input(
    episode_index: int,
    environment_seed: int,
    deck_ids: tuple[str, str],
    deck_hashes: tuple[int, int],
    learner_seat: str,
    decisions: list[dict[str, Any]],
    terminal_record: dict[str, Any],
) -> dict[str, Any]:
    return {
        "episode_index_u64_hex": u64_hex(episode_index),
        "environment_seed_u64_hex": u64_hex(environment_seed),
        "deck_p0_id": deck_ids[0],
        "deck_p0_hash_u64_hex": u64_hex(deck_hashes[0]),
        "deck_p1_id": deck_ids[1],
        "deck_p1_hash_u64_hex": u64_hex(deck_hashes[1]),
        "learner_seat": learner_seat,
        "decisions": decisions,
        "terminal": terminal_record,
    }


def base_single_row_input() -> dict[str, Any]:
    episode_index = 7
    deck_hashes = (11, 13)
    return trajectory_input(
        episode_index,
        17,
        ("Burn", "Rally"),
        deck_hashes,
        "p0",
        [
            decision(
                0,
                "p0",
                "learner",
                0,
                0,
                0,
                1,
                19,
                2,
                1,
                "00112233445566778899aabbccddeeff",
            )
        ],
        terminal(episode_index, deck_hashes, "p0-win", "p0", 1, 1),
    )


def positive_inputs() -> list[tuple[str, dict[str, Any]]]:
    distinctive_episode = 0x0102_0304_0506_0708
    distinctive_hashes = (0x2122_2324_2526_2728, 0x3132_3334_3536_3738)
    distinctive = trajectory_input(
        distinctive_episode,
        0xF1F2_F3F4_F5F6_F7F8,
        ("Rally", "Burn"),
        distinctive_hashes,
        "p0",
        [
            decision(
                0,
                "p0",
                "learner",
                0,
                0,
                0,
                1,
                0x4142_4344_4546_4748,
                1,
                0,
                "000102030405060708090a0b0c0d0e0f",
            ),
            decision(
                1,
                "p1",
                "opponent",
                1,
                0,
                0,
                1,
                0x5152_5354_5556_5758,
                2,
                1,
                "f0e0d0c0b0a090807060504030201000",
            ),
        ],
        terminal(
            distinctive_episode,
            distinctive_hashes,
            "p0-win",
            "p0",
            2,
            2,
        ),
    )

    p1_episode = 0x7172_7374_7576_7778
    p1_hashes = (0x6162_6364_6566_6768, 0x5152_5354_5556_5758)
    p1_two_substep = trajectory_input(
        p1_episode,
        0x8182_8384_8586_8788,
        ("Burn", "Rally"),
        p1_hashes,
        "p1",
        [
            decision(
                0,
                "p0",
                "opponent",
                0,
                0,
                0,
                1,
                0x9192_9394_9596_9798,
                3,
                2,
                "102132435465768798a9bacbdcedfe0f",
            ),
            decision(
                1,
                "p1",
                "learner",
                1,
                0,
                0,
                2,
                0xA1A2_A3A4_A5A6_A7A8,
                4,
                1,
                "ffeeddccbbaa99887766554433221100",
            ),
            decision(
                2,
                "p1",
                "learner",
                1,
                0,
                1,
                2,
                0xB1B2_B3B4_B5B6_B7B8,
                64,
                63,
                "8899aabbccddeeff0011223344556677",
            ),
            decision(
                3,
                "p1",
                "learner",
                2,
                1,
                0,
                1,
                0xC1C2_C3C4_C5C6_C7C8,
                5,
                4,
                "0123456789abcdeffedcba9876543210",
            ),
        ],
        terminal(p1_episode, p1_hashes, "p1-win", "p1", 4, 3),
    )

    seed_pair_a = base_single_row_input()
    seed_pair_a["terminal"] = terminal(7, (11, 13), "draw", "none", 1, 1)
    seed_pair_a["decisions"][0]["legal_action_count_u32"] = 1
    seed_pair_a["decisions"][0]["selected_index_u32"] = 0
    seed_pair_a["decisions"][0]["action_seed_u64_hex"] = u64_hex(1)
    seed_pair_b = copy.deepcopy(seed_pair_a)
    seed_pair_b["decisions"][0]["action_seed_u64_hex"] = u64_hex(
        0x8000_0000_0000_0001
    )

    commitment_pair_clear = base_single_row_input()
    commitment_pair_clear["terminal"] = terminal(
        7, (11, 13), "draw", "none", 1, 1
    )
    commitment_pair_clear["decisions"][0][
        "flat_action_v2_commitment_hex"
    ] = "00000000000000000000000000000000"
    commitment_pair_set = copy.deepcopy(commitment_pair_clear)
    commitment_pair_set["decisions"][0][
        "flat_action_v2_commitment_hex"
    ] = "00000000000000000000000000000001"

    return sorted(
        [
            ("commitment-red-pair-bit-clear", commitment_pair_clear),
            ("commitment-red-pair-bit-set", commitment_pair_set),
            ("distinctive-endian-p0-win", distinctive),
            (
                "p1-learner-two-substep-and-second-group-p1-win",
                p1_two_substep,
            ),
            ("width-one-seed-red-pair-a", seed_pair_a),
            ("width-one-seed-red-pair-b", seed_pair_b),
        ],
        key=lambda item: item[0].encode("ascii"),
    )


def reject_inputs() -> list[tuple[str, dict[str, Any], str]]:
    cases: list[tuple[str, dict[str, Any], str]] = []

    def mutated(
        name: str, code: str, mutate: Any
    ) -> None:
        value = base_single_row_input()
        mutate(value)
        cases.append((name, value, code))

    mutated(
        "actor-physical-ordinal-mismatch",
        "malformed-physical-group",
        lambda value: value["decisions"][0].__setitem__(
            "actor_physical_decision_ordinal_u64_hex", u64_hex(1)
        ),
    )
    mutated(
        "empty-decision-stream",
        "empty-decision-stream",
        lambda value: (
            value.__setitem__("decisions", []),
            value["terminal"].__setitem__("policy_step_count_u64_hex", u64_hex(0)),
            value["terminal"].__setitem__(
                "physical_decision_count_u64_hex", u64_hex(0)
            ),
        ),
    )
    mutated(
        "episode-mismatch",
        "episode-mismatch",
        lambda value: value["terminal"].__setitem__(
            "episode_index_u64_hex", u64_hex(8)
        ),
    )
    mutated(
        "global-physical-ordinal-mismatch",
        "malformed-physical-group",
        lambda value: value["decisions"][0].__setitem__(
            "physical_decision_ordinal_u64_hex", u64_hex(1)
        ),
    )
    mutated(
        "halted-terminal",
        "non-natural-terminal",
        lambda value: value["terminal"].update(
            {
                "outcome": "halted",
                "winner": "none",
                "classification": "halted",
                "terminal_code": "fail-closed",
            }
        ),
    )
    mutated(
        "incomplete-two-substep-group",
        "malformed-physical-group",
        lambda value: (
            value["decisions"][0].__setitem__("substep_count_u32", 2),
            value["terminal"].__setitem__(
                "physical_decision_count_u64_hex", u64_hex(0)
            ),
        ),
    )
    mutated(
        "invalid-deck-id-empty",
        "invalid-deck-id",
        lambda value: value.__setitem__("deck_p0_id", ""),
    )
    mutated(
        "legal-width-0",
        "invalid-legal-action-count",
        lambda value: value["decisions"][0].update(
            {"legal_action_count_u32": 0, "selected_index_u32": 0}
        ),
    )
    mutated(
        "legal-width-65",
        "invalid-legal-action-count",
        lambda value: value["decisions"][0].__setitem__(
            "legal_action_count_u32", 65
        ),
    )
    mutated(
        "malformed-commitment-31-hex-chars",
        "malformed-commitment",
        lambda value: value["decisions"][0].__setitem__(
            "flat_action_v2_commitment_hex", "0" * 31
        ),
    )

    reordered = base_single_row_input()
    reordered["decisions"] = [
        decision(
            1,
            "p1",
            "opponent",
            1,
            0,
            0,
            1,
            23,
            2,
            0,
            "ffeeddccbbaa99887766554433221100",
        ),
        copy.deepcopy(reordered["decisions"][0]),
    ]
    reordered["terminal"] = terminal(7, (11, 13), "p0-win", "p0", 2, 2)
    cases.append(("reordered-rows", reordered, "row-ordinal-mismatch"))

    mutated(
        "role-mismatch",
        "actor-role-mismatch",
        lambda value: value["decisions"][0].__setitem__(
            "actor_role", "opponent"
        ),
    )
    mutated(
        "selected-index-equals-width",
        "selected-index-out-of-range",
        lambda value: value["decisions"][0].__setitem__("selected_index_u32", 2),
    )
    mutated(
        "terminal-count-mismatch",
        "terminal-count-mismatch",
        lambda value: value["terminal"].__setitem__(
            "policy_step_count_u64_hex", u64_hex(2)
        ),
    )
    mutated(
        "terminal-outcome-winner-mismatch",
        "non-natural-terminal",
        lambda value: value["terminal"].__setitem__("winner", "p1"),
    )
    mutated(
        "terminal-provenance-mismatch",
        "terminal-provenance-mismatch",
        lambda value: value["terminal"].__setitem__(
            "deck_p1_hash_u64_hex", u64_hex(99)
        ),
    )
    mutated(
        "truncated-terminal",
        "non-natural-terminal",
        lambda value: value["terminal"].update(
            {
                "outcome": "truncated",
                "winner": "none",
                "classification": "truncated",
                "terminal_code": "decision-cap",
            }
        ),
    )
    mutated(
        "zero-substep-count",
        "malformed-physical-group",
        lambda value: value["decisions"][0].__setitem__("substep_count_u32", 0),
    )
    return sorted(cases, key=lambda item: item[0].encode("ascii"))


def validate_names(records: list[tuple[Any, ...]], label: str) -> None:
    names = [record[0] for record in records]
    if not names or len(names) > MAX_CASES:
        raise AssertionError(f"{label} array violates its nonempty/cap rule")
    if any(not isinstance(name, str) or NAME_RE.fullmatch(name) is None for name in names):
        raise AssertionError(f"{label} contains an invalid name")
    expected = sorted(set(names), key=lambda name: name.encode("ascii"))
    if names != expected:
        raise AssertionError(f"{label} names are not unique strict ASCII order")


def assert_coverage(
    positives: list[tuple[str, dict[str, Any]]],
    rejects: list[tuple[str, dict[str, Any], str]],
) -> None:
    by_name = dict(positives)
    seats = {value["learner_seat"] for _, value in positives}
    roles = {
        row["actor_role"]
        for _, value in positives
        for row in value["decisions"]
    }
    outcomes = {value["terminal"]["outcome"] for _, value in positives}
    if seats != {"p0", "p1"} or roles != {"learner", "opponent"}:
        raise AssertionError("positive vectors do not cover both seats and roles")
    if outcomes != {"p0-win", "p1-win", "draw"}:
        raise AssertionError("positive vectors do not cover all natural outcomes")
    if not any(
        row["substep_count_u32"] == 2
        for _, value in positives
        for row in value["decisions"]
    ):
        raise AssertionError("positive vectors lack a two-substep group")

    learner_group_case = by_name[
        "p1-learner-two-substep-and-second-group-p1-win"
    ]
    learner_rows = [
        row
        for row in learner_group_case["decisions"]
        if row["actor_role"] == "learner"
    ]
    if learner_group_case["learner_seat"] != "p1" or any(
        row["actor_seat"] != "p1" for row in learner_rows
    ):
        raise AssertionError("named P1 learner discriminator has the wrong actor")
    learner_group_projection = [
        (
            row["physical_decision_ordinal_u64_hex"],
            row["actor_physical_decision_ordinal_u64_hex"],
            row["substep_index_u32"],
            row["substep_count_u32"],
        )
        for row in learner_rows
    ]
    if learner_group_projection != [
        (u64_hex(1), u64_hex(0), 0, 2),
        (u64_hex(1), u64_hex(0), 1, 2),
        (u64_hex(2), u64_hex(1), 0, 1),
    ]:
        raise AssertionError(
            "learner multi-substep/second-group discriminator drifted"
        )
    _stream, learner_group_receipt = evaluate_case(learner_group_case)
    expected_learner_group_split = {
        "policy_step_count": 4,
        "physical_decision_count": 3,
        "learner_policy_step_count": 3,
        "opponent_policy_step_count": 1,
        "learner_physical_decision_count": 2,
        "opponent_physical_decision_count": 1,
    }
    if any(
        learner_group_receipt[field] != expected
        for field, expected in expected_learner_group_split.items()
    ):
        raise AssertionError("learner multi-group receipt split drifted")

    seed_a = by_name["width-one-seed-red-pair-a"]
    seed_b = by_name["width-one-seed-red-pair-b"]
    seed_a_without = copy.deepcopy(seed_a)
    seed_b_without = copy.deepcopy(seed_b)
    seed_a_without["decisions"][0]["action_seed_u64_hex"] = "<red-pair>"
    seed_b_without["decisions"][0]["action_seed_u64_hex"] = "<red-pair>"
    if seed_a_without != seed_b_without:
        raise AssertionError("width-one seed red pair differs outside its seed")
    if seed_a["decisions"][0]["action_seed_u64_hex"] == u64_hex(0):
        raise AssertionError("width-one seed red pair must use nonzero seeds")

    commitment_clear = by_name["commitment-red-pair-bit-clear"]
    commitment_set = by_name["commitment-red-pair-bit-set"]
    clear_bytes = bytes.fromhex(
        commitment_clear["decisions"][0]["flat_action_v2_commitment_hex"]
    )
    set_bytes = bytes.fromhex(
        commitment_set["decisions"][0]["flat_action_v2_commitment_hex"]
    )
    differing_bits = sum((left ^ right).bit_count() for left, right in zip(clear_bytes, set_bytes))
    if differing_bits != 1:
        raise AssertionError("commitment red pair must differ by exactly one bit")
    clear_without = copy.deepcopy(commitment_clear)
    set_without = copy.deepcopy(commitment_set)
    clear_without["decisions"][0]["flat_action_v2_commitment_hex"] = "<red-pair>"
    set_without["decisions"][0]["flat_action_v2_commitment_hex"] = "<red-pair>"
    if clear_without != set_without:
        raise AssertionError("commitment red pair differs outside its commitment")

    reject_names = {name for name, _, _ in rejects}
    required_rejects = {
        "empty-decision-stream",
        "reordered-rows",
        "role-mismatch",
        "global-physical-ordinal-mismatch",
        "incomplete-two-substep-group",
        "legal-width-0",
        "legal-width-65",
        "selected-index-equals-width",
        "malformed-commitment-31-hex-chars",
        "terminal-outcome-winner-mismatch",
        "terminal-count-mismatch",
        "truncated-terminal",
        "halted-terminal",
    }
    if not required_rejects.issubset(reject_names):
        raise AssertionError("reject vectors do not cover the declared minimum matrix")


def build_payload() -> dict[str, Any]:
    positive_sources = positive_inputs()
    reject_sources = reject_inputs()
    validate_names(positive_sources, "positive")
    validate_names(reject_sources, "reject")
    assert_coverage(positive_sources, reject_sources)

    positive_cases = []
    for name, input_record in positive_sources:
        stream, _receipt = evaluate_case(input_record)
        positive_cases.append(
            {
                "name": name,
                "input": input_record,
                "stream_hex": stream.hex(),
                "expected_sha256": sha256_hex(stream),
            }
        )

    positive_by_name = {case["name"]: case for case in positive_cases}
    for left, right in (
        ("width-one-seed-red-pair-a", "width-one-seed-red-pair-b"),
        ("commitment-red-pair-bit-clear", "commitment-red-pair-bit-set"),
    ):
        if positive_by_name[left]["expected_sha256"] == positive_by_name[right][
            "expected_sha256"
        ]:
            raise AssertionError(f"declared trajectory red pair {left}/{right} collided")
    distinctive_stream = bytes.fromhex(
        positive_by_name["distinctive-endian-p0-win"]["stream_hex"]
    )
    if bytes.fromhex("0102030405060708") not in distinctive_stream:
        raise AssertionError("distinctive big-endian episode bytes are absent")

    reject_cases = []
    for name, input_record, expected_rejection in reject_sources:
        try:
            evaluate_case(input_record)
        except ContractRejection as error:
            if error.code != expected_rejection:
                raise AssertionError(
                    f"reject vector {name} produced {error.code}, "
                    f"expected {expected_rejection}"
                ) from error
        else:  # pragma: no cover - protects the generator contract itself
            raise AssertionError(f"reject vector {name} was unexpectedly admitted")
        reject_cases.append(
            {
                "name": name,
                "input": input_record,
                "expected_rejection": expected_rejection,
            }
        )

    return {
        "schema": SCHEMA,
        "generator_identity": GENERATOR_IDENTITY,
        "trajectory_identity": TRAJECTORY_IDENTITY,
        "vector_stream_identity": VECTOR_STREAM_IDENTITY,
        "positive_cases": positive_cases,
        "reject_cases": reject_cases,
    }


def golden_vector_stream(payload: dict[str, Any]) -> bytes:
    parts = [
        atom("domain", VECTOR_STREAM_IDENTITY.encode("ascii")),
        atom(
            "positive_case_count_u64be",
            len(payload["positive_cases"]).to_bytes(8, "big"),
        ),
    ]
    for case in payload["positive_cases"]:
        case_payload = b"".join(
            (
                atom("name_utf8", case["name"].encode("ascii")),
                atom("input_canonical_json", canonical_json_bytes(case["input"])),
                atom("stream_bytes", bytes.fromhex(case["stream_hex"])),
                atom("expected_sha256", bytes.fromhex(case["expected_sha256"])),
            )
        )
        parts.append(atom("positive_case", case_payload))
    parts.append(
        atom(
            "reject_case_count_u64be",
            len(payload["reject_cases"]).to_bytes(8, "big"),
        )
    )
    for case in payload["reject_cases"]:
        case_payload = b"".join(
            (
                atom("name_utf8", case["name"].encode("ascii")),
                atom("input_canonical_json", canonical_json_bytes(case["input"])),
                atom(
                    "expected_rejection_ascii",
                    case["expected_rejection"].encode("ascii"),
                ),
            )
        )
        parts.append(atom("reject_case", case_payload))
    return b"".join(parts)


def render() -> tuple[bytes, str]:
    payload = build_payload()
    artifact = canonical_json_bytes(payload)
    if len(artifact) > MAX_ARTIFACT_BYTES:
        raise AssertionError("trajectory golden artifact exceeds 4 MiB")
    return artifact, sha256_hex(golden_vector_stream(payload))


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--stdout", action="store_true")
    arguments = parser.parse_args()
    artifact, stream_sha256 = render()
    if arguments.stdout:
        sys.stdout.buffer.write(artifact)
        return 0
    if arguments.check:
        if not OUTPUT.is_file() or OUTPUT.read_bytes() != artifact:
            print("NATIVE_FULL_EPISODE_TRAJECTORY_GOLDENS: STALE", file=sys.stderr)
            return 1
        print("NATIVE_FULL_EPISODE_TRAJECTORY_GOLDENS: PASS")
        print(f"file_sha256={sha256_hex(artifact)}")
        print(f"vector_stream_sha256={stream_sha256}")
        return 0
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    OUTPUT.write_bytes(artifact)
    print(f"wrote {OUTPUT_RELATIVE.as_posix()}")
    print(f"file_sha256={sha256_hex(artifact)}")
    print(f"vector_stream_sha256={stream_sha256}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
