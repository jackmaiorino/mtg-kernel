#!/usr/bin/env python3
"""Stdlib-only generator for the native full-episode trajectory V2 audit
goldens.

Frozen contract: collab TO-CLAUDE.md, "CODEX VERIFIED COMMIT AND NEXT SLICE:
V2 FULL-EPISODE AUDIT, PHASE A" as superseded by "CODEX PHASE A CORRECTIONS
AFTER ADVERSARIAL REVIEW". Evidence only: this activates no runtime consumer.

The V2 envelope *wraps* the final raw32 V1 trajectory digest. It never copies
the V1 row or terminal serializer, and it never accepts an externally supplied
inner digest or inner start value. A positive input carries no inner digest and
no inner-root/inner-seat/inner-deck override field; the evaluator constructs and
finishes its own inner V1 accumulator from exactly the same start values the V2
envelope commits, then envelopes the digest it returned. Inner/outer provenance
laundering is therefore unrepresentable rather than merely detected afterward.

This module is an independent transcription of the frozen V1 transcript rules.
It does not import the V1 generator, call Rust, or consume any Rust-produced
digest.

Atom framing everywhere: u32be(tag_len) || tag || u64be(payload_len) || payload

Usage:
  python generate_native_full_episode_trajectory_v2_goldens.py
  python generate_native_full_episode_trajectory_v2_goldens.py --check
"""

from __future__ import annotations

import sys

sys.dont_write_bytecode = True

import argparse  # noqa: E402
import copy  # noqa: E402
import hashlib  # noqa: E402
import json  # noqa: E402
import re  # noqa: E402
from pathlib import Path  # noqa: E402
from typing import Any, NoReturn  # noqa: E402

# ---------------------------------------------------------------- identities

SCHEMA = "mtg_kernel_native_full_episode_trajectory_v2_goldens/v1"
GENERATOR_IDENTITY = (
    "mtg-kernel-native-full-episode-trajectory-v2-goldens-stdlib-python-v1"
)
TRAJECTORY_IDENTITY_V2 = "mtg-kernel-native-full-episode-trajectory-sha256-v2"
VECTOR_STREAM_IDENTITY = (
    "mtg-kernel-native-full-episode-trajectory-v2-golden-vector-stream-sha256-v1"
)

# ------------------------------------------------- frozen inner V1 authority

INNER_IDENTITY = "mtg-kernel-native-full-episode-trajectory-sha256-v1"
INNER_GOLDENS_SCHEMA = "mtg_kernel_native_full_episode_trajectory_goldens/v1"
INNER_GENERATOR_IDENTITY = (
    "mtg-kernel-native-full-episode-trajectory-goldens-stdlib-python-v1"
)
INNER_STREAM_IDENTITY = (
    "mtg-kernel-native-full-episode-trajectory-golden-vector-stream-sha256-v1"
)
INNER_GOLDENS_FILE_SHA256 = (
    "502a1b4ba296fdc4b2f4e8fd61cc5b4d64f152c9b84b4e11a85967f76c3bde8b"
)
INNER_STREAM_SHA256 = (
    "f5230cbbc0b87735e7aa14c89ce31e41ce769de3f4292cafe63dad4733168d7a"
)

# --------------------------------------- frozen environment/reset authority

ENV_IDENTITY = "mtg-kernel-environment-randomization-sha256-v2"
ENV_NAMESPACE = "environment-randomization-substream"
ENV_KDF_GOLDENS_SCHEMA = "mtg-kernel-environment-randomization-v2-goldens/v1"
ENV_KDF_GOLDENS_FILE_SHA256 = (
    "bc2b0d66f8e3eb608b6035321f23a214bbf5141aaf7305f50f606f6c85b4a3bc"
)
RESET_GOLDENS_SCHEMA = (
    "mtg-kernel-environment-randomization-v2-reset-physical-trajectory-goldens/v1"
)
RESET_GENERATOR_IDENTITY = (
    "mtg-kernel-environment-randomization-v2-reset-physical-trajectory-goldens"
    "-stdlib-python-v1"
)
RESET_PROJECTION_IDENTITY = (
    "mtg-kernel-environment-randomization-v2-physical-card-definition-projection/v1"
)
RESET_STREAM_IDENTITY = (
    "mtg-kernel-environment-randomization-v2-reset-physical-trajectory"
    "-portable-vector-stream-sha256-v1"
)
RESET_GOLDENS_FILE_SHA256 = (
    "18ec6cd138a76bce1bf06c6b794fe169fbe8d83c0a9265d0ff99119a4c4a16bc"
)
RESET_STREAM_SHA256 = (
    "97f8eeff002ec15f3e30f58fd1f1e477a8abf1db3a38e25aaeb810f87da2a085"
)

# ------------------------------ frozen trainer schedule and runtime catalog

TRAINER_SCHEDULE_IDENTITY = "mtg-kernel-native-trainer-schedule-sha256-v1"
TRAINER_SEED_VERSION = "kernel-python-rl-trainer-sha256-v2"
ENV_PYTHON_REFERENCE_RAW_FILE_SHA256 = (
    "9dd7e5357d98ff5a7ac302d285da91fb56cf0d422c5aef6bc9b53f2a5d822024"
)
NATIVE_SCHEDULE_GOLDENS_SCHEMA = "mtg_kernel_native_trainer_schedule_goldens/v1"
TRAINER_SCHEDULE_GOLDENS_FILE_SHA256 = (
    "6b2e1edbbe49b4e02f98794f9057f5c2bb8e3079d2ba8cb3e2a4b9ea6c34867c"
)
TRAIN_ENV_NAMESPACE = "train-env"

RUNTIME_DECK_CATALOG_SCHEMA = "kernel_runtime_decks/v1"
RUNTIME_DECK_PROTOCOL = "canonical-mainboard-bo1/v1"
RUNTIME_DECK_MATERIALIZATION_PROTOCOL = "xmage_xml_row_then_copy_ordinal/v1"
RUNTIME_DECK_HASH_ALGORITHM = "fnv1a64-serde-json-u16-array/v1"
RUNTIME_DECK_CATALOG_FILE_SHA256 = (
    "68e7602f3a4df6217119406973954630800c358a10fca9f28e6cf9f20fd3b851"
)
FROZEN_RUNTIME_DECKS = {
    "Burn": 0x5FDB_7B92_986B_6FC1,
    "Rally": 0x0C9F_01C2_5444_12BF,
}

U32_MAX = (1 << 32) - 1
U62_MAX = (1 << 62) - 1
U63_MAX = (1 << 63) - 1
U64_MAX = (1 << 64) - 1

MAX_DECISIONS = 4096
MAX_CASES = 256
MAX_ARTIFACT_BYTES = 4 * 1024 * 1024
NAME_RE = re.compile(r"[a-z0-9][a-z0-9-]{0,127}")

REPOSITORY_ROOT = Path(__file__).resolve().parents[2]
OUTPUT = REPOSITORY_ROOT / "data" / "native_full_episode_trajectory_v2_goldens.json"

# ------------------------------------------- closed portable reject vocabulary

REJECTION_CODES = (
    "authority-mismatch",
    "episode-index-outside-u63",
    "learner-seat-rule-mismatch",
    "invalid-deck-id",
    "runtime-deck-hash-mismatch",
    "empty-decision-stream",
    "episode-mismatch",
    "row-ordinal-mismatch",
    "actor-role-mismatch",
    "malformed-physical-group",
    "invalid-legal-action-count",
    "selected-index-out-of-range",
    "malformed-commitment",
    "counter-overflow",
    "non-natural-terminal",
    "terminal-provenance-mismatch",
    "terminal-count-mismatch",
    "schedule-integer-outside-u63",
    "pair-index-outside-episode-domain",
    "pair-episode-index-mismatch",
    "pair-environment-seed-mismatch",
    "pair-physical-deck-binding-mismatch",
)


class ContractRejection(ValueError):
    """A closed, portable rejection code."""

    def __init__(self, code: str) -> None:
        if code not in REJECTION_CODES:
            raise AssertionError(f"unknown rejection code: {code}")
        super().__init__(code)
        self.code = code


class ShapeError(AssertionError):
    """A strict-shape violation, distinct from a portable rejection."""


def reject(code: str) -> NoReturn:
    raise ContractRejection(code)


# ---------------------------------------------------------------- primitives


def sha256_hex(payload: bytes) -> str:
    return hashlib.sha256(payload).hexdigest()


def atom(tag: str, payload: bytes) -> bytes:
    encoded = tag.encode("utf-8")
    return (
        len(encoded).to_bytes(4, "big")
        + encoded
        + len(payload).to_bytes(8, "big")
        + payload
    )


def raw32(pin: str) -> bytes:
    if len(pin) != 64 or any(char not in "0123456789abcdef" for char in pin):
        raise ShapeError(f"pin {pin!r} is not 64 lowercase hex")
    return bytes.fromhex(pin)


def canonical_json_no_lf(value: Any) -> bytes:
    """Compact ASCII-sorted JSON with no trailing LF, for stream atoms."""
    return json.dumps(
        value,
        sort_keys=True,
        separators=(",", ":"),
        ensure_ascii=True,
        allow_nan=False,
    ).encode("ascii")


def canonical_file_bytes(value: Any) -> bytes:
    """Canonical artifact bytes: compact ASCII-sorted JSON with one final LF."""
    return canonical_json_no_lf(value) + b"\n"


def strict_json_loads(raw: bytes, label: str) -> Any:
    """Reject duplicate keys at every depth, every float literal, and every
    non-finite constant. Booleans in integer positions are caught downstream by
    the strict scalar checks, which exclude bool from int."""
    if b"\r" in raw:
        raise ShapeError(f"{label}: carriage returns are not permitted")
    try:
        text = raw.decode("ascii")
    except UnicodeDecodeError as error:
        raise ShapeError(f"{label}: bytes are not ASCII") from error

    def object_pairs(pairs):
        result: dict = {}
        for key, value in pairs:
            if key in result:
                raise ShapeError(f"{label}: duplicate object key {key!r}")
            result[key] = value
        return result

    def on_float(literal):
        raise ShapeError(f"{label}: float literal {literal!r} is not permitted")

    def on_constant(name):
        raise ShapeError(f"{label}: non-finite constant {name!r} is not permitted")

    return json.loads(
        text,
        object_pairs_hook=object_pairs,
        parse_float=on_float,
        parse_constant=on_constant,
    )


def require_bounded_int(value: Any, low: int, high: int, label: str) -> int:
    """Strict scalar: bool is excluded because it is an int subclass."""
    if isinstance(value, bool) or type(value) is not int:
        raise ShapeError(f"{label} is not a non-boolean JSON integer")
    if not low <= value <= high:
        raise ShapeError(f"{label} is outside [{low}, {high}]")
    return value


def require_lower_hex(value: Any, digits: int, label: str) -> str:
    if not isinstance(value, str) or len(value) != digits:
        raise ShapeError(f"{label} is not exactly {digits} characters")
    if any(char not in "0123456789abcdef" for char in value):
        raise ShapeError(f"{label} is not lowercase hex")
    return value


def require_exact_object(value: Any, fields: set[str], label: str) -> dict:
    if not isinstance(value, dict):
        raise ShapeError(f"{label} is not an object")
    if set(value) != fields:
        missing = sorted(fields - set(value))
        unknown = sorted(set(value) - fields)
        raise ShapeError(f"{label} field set violated; missing {missing}, unknown {unknown}")
    return value


def require_u32(value: Any, label: str) -> int:
    if type(value) is not int or not 0 <= value <= U32_MAX:
        raise ShapeError(f"{label} is not an admitted u32 JSON integer")
    return value


def parse_fixed_hex(value: Any, digits: int, label: str) -> int:
    if (
        not isinstance(value, str)
        or len(value) != digits
        or any(char not in "0123456789abcdef" for char in value)
    ):
        raise ShapeError(f"{label} is not exactly {digits} lowercase hex digits")
    return int(value, 16)


def u64_hex(value: int) -> str:
    if type(value) is not int or not 0 <= value <= U64_MAX:
        raise ShapeError("u64 literal out of range")
    return f"{value:016x}"


def ascii_printable(value: Any, label: str, maximum: int) -> bytes:
    if not isinstance(value, str):
        raise ShapeError(f"{label} is not a string")
    try:
        encoded = value.encode("ascii")
    except UnicodeEncodeError as error:
        raise ShapeError(f"{label} is not ASCII") from error
    if len(encoded) > maximum or any(not 0x20 <= byte <= 0x7E for byte in encoded):
        raise ShapeError(f"{label} violates its printable-ASCII bounds")
    return encoded


def seat_code(value: Any) -> int:
    if value == "p0":
        return 0
    if value == "p1":
        return 1
    raise ShapeError("seat outside the closed p0|p1 vocabulary")


def role_code(value: Any) -> int:
    if value == "learner":
        return 0
    if value == "opponent":
        return 1
    raise ShapeError("role outside the closed learner|opponent vocabulary")


def learner_seat_for_episode(episode_index: int) -> str:
    """Even episode means P0 learner; odd means P1 learner."""
    return "p0" if episode_index % 2 == 0 else "p1"


def derive_train_env_root(base_seed: int, pair_index: int) -> int:
    """Independent reimplementation of the frozen native train-env derivation.
    The version atom is the trainer seed version; the schedule identity is an
    artifact binding and is never hashed."""
    if not 0 <= base_seed <= U63_MAX:
        reject("schedule-integer-outside-u63")
    if not 0 <= pair_index <= U62_MAX:
        reject("pair-index-outside-episode-domain")
    hasher = hashlib.sha256()
    hasher.update(atom("version", TRAINER_SEED_VERSION.encode("utf-8")))
    hasher.update(atom("namespace", TRAIN_ENV_NAMESPACE.encode("utf-8")))
    hasher.update(atom("field-name", b"base_seed"))
    hasher.update(atom("u63", base_seed.to_bytes(8, "big")))
    hasher.update(atom("field-name", b"pair_index"))
    hasher.update(atom("u63", pair_index.to_bytes(8, "big")))
    return int.from_bytes(hasher.digest()[:8], "big") & U63_MAX


# ----------------------------------------- independent inner V1 reimplementation

INNER_DECISION_FIELDS = {
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
INNER_TERMINAL_FIELDS = {
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

# The V2 positive input. There is deliberately no inner digest field and no
# inner-root, inner-seat, or inner-deck override field: the inner accumulator is
# owned and is fed exactly these start values.
TRAJECTORY_INPUT_FIELDS = {
    "source_authorities",
    "episode_index_u64_hex",
    "pair_environment_seed_u64_hex",
    "deck_p0_id",
    "deck_p0_hash_u64_hex",
    "deck_p1_id",
    "deck_p1_hash_u64_hex",
    "learner_seat",
    "decisions",
    "terminal",
}
PAIR_INPUT_FIELDS = {
    "base_seed_u64_hex",
    "pair_index_u64_hex",
    "even_start",
    "odd_start",
}


def commitment_raw16(value: Any) -> bytes:
    encoded = ascii_printable(value, "commitment", 34)
    if len(encoded) != 32 or any(char not in b"0123456789abcdef" for char in encoded):
        reject("malformed-commitment")
    return bytes.fromhex(value)


def deck_id_ascii(value: Any) -> bytes:
    encoded = ascii_printable(value, "deck ID", 65)
    if not 1 <= len(encoded) <= 64:
        reject("invalid-deck-id")
    return encoded


def resolve_runtime_deck(deck_id: str, deck_hash: int) -> None:
    """Both physical deck IDs must resolve exactly in the runtime catalog and
    each supplied hash must equal that deck's frozen runtime hash."""
    if deck_id not in FROZEN_RUNTIME_DECKS:
        reject("invalid-deck-id")
    if deck_hash != FROZEN_RUNTIME_DECKS[deck_id]:
        reject("runtime-deck-hash-mismatch")


def inner_terminal_codes(terminal: dict) -> tuple[int, int, int, int]:
    outcome_codes = {"p0-win": 0, "p1-win": 1, "draw": 2, "truncated": 3, "halted": 4}
    winner_codes = {"none": 0, "p0": 1, "p1": 2}
    classification_codes = {"natural": 0, "truncated": 1, "halted": 2}
    terminal_code_codes = {"natural-game-over": 0, "decision-cap": 1, "fail-closed": 2}
    try:
        codes = (
            outcome_codes[terminal["outcome"]],
            winner_codes[terminal["winner"]],
            classification_codes[terminal["classification"]],
            terminal_code_codes[terminal["terminal_code"]],
        )
    except (KeyError, TypeError) as error:
        raise ShapeError("terminal value outside a closed vocabulary") from error
    if codes not in {(0, 1, 0, 0), (1, 2, 0, 0), (2, 0, 0, 0)}:
        reject("non-natural-terminal")
    return codes


def inner_decision_row_payload(row: dict) -> bytes:
    return b"".join(
        (
            atom(
                "row_ordinal_u64be",
                parse_fixed_hex(row["row_ordinal_u64_hex"], 16, "row ordinal").to_bytes(
                    8, "big"
                ),
            ),
            atom("actor_seat_u8", bytes((seat_code(row["actor_seat"]),))),
            atom("actor_role_u8", bytes((role_code(row["actor_role"]),))),
            atom(
                "physical_decision_ordinal_u64be",
                parse_fixed_hex(
                    row["physical_decision_ordinal_u64_hex"], 16, "physical ordinal"
                ).to_bytes(8, "big"),
            ),
            atom(
                "actor_physical_decision_ordinal_u64be",
                parse_fixed_hex(
                    row["actor_physical_decision_ordinal_u64_hex"],
                    16,
                    "actor physical ordinal",
                ).to_bytes(8, "big"),
            ),
            atom(
                "substep_index_u32be",
                require_u32(row["substep_index_u32"], "substep index").to_bytes(4, "big"),
            ),
            atom(
                "substep_count_u32be",
                require_u32(row["substep_count_u32"], "substep count").to_bytes(4, "big"),
            ),
            atom(
                "action_seed_u64be",
                parse_fixed_hex(row["action_seed_u64_hex"], 16, "action seed").to_bytes(
                    8, "big"
                ),
            ),
            atom(
                "legal_action_count_u32be",
                require_u32(row["legal_action_count_u32"], "legal count").to_bytes(
                    4, "big"
                ),
            ),
            atom(
                "selected_index_u32be",
                require_u32(row["selected_index_u32"], "selected index").to_bytes(
                    4, "big"
                ),
            ),
            atom(
                "flat_action_v2_commitment_raw16",
                commitment_raw16(row["flat_action_v2_commitment_hex"]),
            ),
        )
    )


def inner_terminal_row_payload(codes: tuple[int, int, int, int], counts: dict) -> bytes:
    return b"".join(
        (
            atom("terminal_outcome_u8", bytes((codes[0],))),
            atom("winner_option_u8", bytes((codes[1],))),
            atom("terminal_classification_u8", bytes((codes[2],))),
            atom("terminal_code_u8", bytes((codes[3],))),
            atom("policy_step_count_u64be", counts["policy"].to_bytes(8, "big")),
            atom("physical_decision_count_u64be", counts["physical"].to_bytes(8, "big")),
            atom(
                "learner_policy_step_count_u64be",
                counts["learner_policy"].to_bytes(8, "big"),
            ),
            atom(
                "opponent_policy_step_count_u64be",
                counts["opponent_policy"].to_bytes(8, "big"),
            ),
            atom(
                "learner_physical_decision_count_u64be",
                counts["learner_physical"].to_bytes(8, "big"),
            ),
            atom(
                "opponent_physical_decision_count_u64be",
                counts["opponent_physical"].to_bytes(8, "big"),
            ),
        )
    )


def owned_inner_stream(start: dict) -> bytes:
    """Construct, drive, and finish an owned inner V1 accumulator from exactly
    the V2 start values. Nothing here consults a supplied inner digest."""
    episode_index = parse_fixed_hex(start["episode_index_u64_hex"], 16, "episode index")
    if episode_index > U63_MAX:
        reject("episode-index-outside-u63")
    # The V2 pair root is passed verbatim as the V1 environment seed, full u64,
    # never masked.
    environment_seed = parse_fixed_hex(
        start["pair_environment_seed_u64_hex"], 16, "pair environment seed"
    )
    deck_p0_id = deck_id_ascii(start["deck_p0_id"])
    deck_p1_id = deck_id_ascii(start["deck_p1_id"])
    deck_p0_hash = parse_fixed_hex(start["deck_p0_hash_u64_hex"], 16, "deck P0 hash")
    deck_p1_hash = parse_fixed_hex(start["deck_p1_hash_u64_hex"], 16, "deck P1 hash")
    resolve_runtime_deck(start["deck_p0_id"], deck_p0_hash)
    resolve_runtime_deck(start["deck_p1_id"], deck_p1_hash)

    learner_seat = start["learner_seat"]
    learner_seat_encoded = seat_code(learner_seat)
    if learner_seat != learner_seat_for_episode(episode_index):
        reject("learner-seat-rule-mismatch")

    decisions = start["decisions"]
    if not isinstance(decisions, list) or len(decisions) > MAX_DECISIONS:
        raise ShapeError("decisions is not an array inside the declared cap")

    parts = [
        atom("domain", INNER_IDENTITY.encode("ascii")),
        atom("episode_index_u64be", episode_index.to_bytes(8, "big")),
        atom("environment_seed_u64be", environment_seed.to_bytes(8, "big")),
        atom("deck_p0_id_utf8", deck_p0_id),
        atom("deck_p0_hash_u64be", deck_p0_hash.to_bytes(8, "big")),
        atom("deck_p1_id_utf8", deck_p1_id),
        atom("deck_p1_hash_u64be", deck_p1_hash.to_bytes(8, "big")),
        atom("learner_seat_u8", bytes((learner_seat_encoded,))),
    ]

    learner_policy = opponent_policy = 0
    learner_physical = opponent_physical = 0
    open_group: tuple | None = None

    for expected_ordinal, raw_row in enumerate(decisions):
        row = require_exact_object(raw_row, INNER_DECISION_FIELDS, "decision")
        if parse_fixed_hex(row["row_ordinal_u64_hex"], 16, "row ordinal") != expected_ordinal:
            reject("row-ordinal-mismatch")
        actor_seat = row["actor_seat"]
        seat_code(actor_seat)
        actor_role = row["actor_role"]
        role_code(actor_role)
        if actor_role != ("learner" if actor_seat == learner_seat else "opponent"):
            reject("actor-role-mismatch")
        legal_count = require_u32(row["legal_action_count_u32"], "legal count")
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
            row["actor_physical_decision_ordinal_u64_hex"], 16, "actor physical ordinal"
        )
        if open_group is None:
            expected_physical = learner_physical + opponent_physical
            expected_actor_physical = (
                learner_physical if actor_role == "learner" else opponent_physical
            )
            if (
                substep_index != 0
                or physical_ordinal != expected_physical
                or actor_physical_ordinal != expected_actor_physical
            ):
                reject("malformed-physical-group")
        else:
            observed = (
                actor_seat,
                actor_role,
                physical_ordinal,
                actor_physical_ordinal,
                substep_count,
                substep_index,
            )
            if observed != open_group:
                reject("malformed-physical-group")

        parts.append(atom("decision_row", inner_decision_row_payload(row)))
        if actor_role == "learner":
            learner_policy += 1
        else:
            opponent_policy += 1
        if substep_index + 1 == substep_count:
            if actor_role == "learner":
                learner_physical += 1
            else:
                opponent_physical += 1
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

    terminal = require_exact_object(start["terminal"], INNER_TERMINAL_FIELDS, "terminal")
    # Production V1 precedence: terminal episode agreement is decided before
    # empty-stream or open-physical-group failures, so a compound-invalid input
    # reports episode-mismatch.
    if parse_fixed_hex(terminal["episode_index_u64_hex"], 16, "terminal episode") != episode_index:
        reject("episode-mismatch")
    if not decisions:
        reject("empty-decision-stream")
    if open_group is not None:
        reject("malformed-physical-group")
    terminal_p0 = parse_fixed_hex(terminal["deck_p0_hash_u64_hex"], 16, "terminal P0 hash")
    terminal_p1 = parse_fixed_hex(terminal["deck_p1_hash_u64_hex"], 16, "terminal P1 hash")
    if (terminal_p0, terminal_p1) != (deck_p0_hash, deck_p1_hash):
        reject("terminal-provenance-mismatch")
    codes = inner_terminal_codes(terminal)
    policy = learner_policy + opponent_policy
    physical = learner_physical + opponent_physical
    if policy > U64_MAX or physical > U64_MAX:
        reject("counter-overflow")
    if (
        parse_fixed_hex(terminal["policy_step_count_u64_hex"], 16, "terminal policy") != policy
        or parse_fixed_hex(
            terminal["physical_decision_count_u64_hex"], 16, "terminal physical"
        )
        != physical
        or policy != len(decisions)
    ):
        reject("terminal-count-mismatch")

    parts.append(
        atom(
            "terminal_row",
            inner_terminal_row_payload(
                codes,
                {
                    "policy": policy,
                    "physical": physical,
                    "learner_policy": learner_policy,
                    "opponent_policy": opponent_policy,
                    "learner_physical": learner_physical,
                    "opponent_physical": opponent_physical,
                },
            ),
        )
    )
    return b"".join(parts)


# ------------------------------------------------------------- V2 envelope

EXPECTED_SOURCE_AUTHORITIES = {
    "inner_trajectory": {
        "identity": INNER_IDENTITY,
        "goldens_schema": INNER_GOLDENS_SCHEMA,
        "goldens_generator_identity": INNER_GENERATOR_IDENTITY,
        "goldens_raw_file_sha256": INNER_GOLDENS_FILE_SHA256,
        "golden_semantic_stream_identity": INNER_STREAM_IDENTITY,
        "golden_semantic_stream_sha256": INNER_STREAM_SHA256,
    },
    "environment_randomization": {
        "identity": ENV_IDENTITY,
        "namespace": ENV_NAMESPACE,
        "python_reference_raw_file_sha256": ENV_PYTHON_REFERENCE_RAW_FILE_SHA256,
        "kdf_goldens_schema": ENV_KDF_GOLDENS_SCHEMA,
        "kdf_goldens_raw_file_sha256": ENV_KDF_GOLDENS_FILE_SHA256,
    },
    "reset_trajectory": {
        "goldens_schema": RESET_GOLDENS_SCHEMA,
        "generator_identity": RESET_GENERATOR_IDENTITY,
        "physical_projection_identity": RESET_PROJECTION_IDENTITY,
        "goldens_raw_file_sha256": RESET_GOLDENS_FILE_SHA256,
        "portable_semantic_stream_identity": RESET_STREAM_IDENTITY,
        "portable_semantic_stream_sha256": RESET_STREAM_SHA256,
    },
    "trainer_schedule": {
        "identity": TRAINER_SCHEDULE_IDENTITY,
        "seed_version": TRAINER_SEED_VERSION,
        "goldens_schema": NATIVE_SCHEDULE_GOLDENS_SCHEMA,
        "goldens_raw_file_sha256": TRAINER_SCHEDULE_GOLDENS_FILE_SHA256,
    },
    "runtime_deck_catalog": {
        "schema": RUNTIME_DECK_CATALOG_SCHEMA,
        "protocol": RUNTIME_DECK_PROTOCOL,
        "materialization_protocol": RUNTIME_DECK_MATERIALIZATION_PROTOCOL,
        "deck_hash_algorithm": RUNTIME_DECK_HASH_ALGORITHM,
        "catalog_raw_file_sha256": RUNTIME_DECK_CATALOG_FILE_SHA256,
    },
}
# The new V2 artifact hash and V2 semantic-stream hash are deliberately absent:
# either would make this object commit to a digest computed over itself.


def require_authorities(authorities: Any) -> None:
    if authorities != EXPECTED_SOURCE_AUTHORITIES:
        reject("authority-mismatch")


def v2_envelope(start: dict, inner_sha256_raw: bytes) -> bytes:
    """The exact frozen 34-atom V2 envelope. The inner digest is wrapped as the
    final atom; no V1 row or terminal bytes are re-serialized here. The envelope
    deliberately carries no hash of itself."""
    episode_index = parse_fixed_hex(start["episode_index_u64_hex"], 16, "episode index")
    if episode_index > U63_MAX:
        reject("episode-index-outside-u63")
    learner_seat = start["learner_seat"]
    if learner_seat != learner_seat_for_episode(episode_index):
        reject("learner-seat-rule-mismatch")
    return b"".join(
        (
            atom("domain", TRAJECTORY_IDENTITY_V2.encode("ascii")),
            atom("inner_trajectory_identity_utf8", INNER_IDENTITY.encode("utf-8")),
            atom("inner_trajectory_goldens_schema_utf8", INNER_GOLDENS_SCHEMA.encode("utf-8")),
            atom(
                "inner_trajectory_goldens_generator_identity_utf8",
                INNER_GENERATOR_IDENTITY.encode("utf-8"),
            ),
            atom(
                "inner_trajectory_golden_stream_identity_utf8",
                INNER_STREAM_IDENTITY.encode("utf-8"),
            ),
            atom("inner_trajectory_goldens_file_sha256_raw32", raw32(INNER_GOLDENS_FILE_SHA256)),
            atom("inner_trajectory_golden_stream_sha256_raw32", raw32(INNER_STREAM_SHA256)),
            atom("environment_randomization_identity_utf8", ENV_IDENTITY.encode("utf-8")),
            atom("environment_randomization_namespace_utf8", ENV_NAMESPACE.encode("utf-8")),
            atom(
                "environment_randomization_kdf_goldens_schema_utf8",
                ENV_KDF_GOLDENS_SCHEMA.encode("utf-8"),
            ),
            atom(
                "environment_randomization_kdf_goldens_file_sha256_raw32",
                raw32(ENV_KDF_GOLDENS_FILE_SHA256),
            ),
            atom("reset_trajectory_goldens_schema_utf8", RESET_GOLDENS_SCHEMA.encode("utf-8")),
            atom(
                "reset_trajectory_generator_identity_utf8",
                RESET_GENERATOR_IDENTITY.encode("utf-8"),
            ),
            atom(
                "reset_trajectory_physical_projection_identity_utf8",
                RESET_PROJECTION_IDENTITY.encode("utf-8"),
            ),
            atom(
                "reset_trajectory_vector_stream_identity_utf8",
                RESET_STREAM_IDENTITY.encode("utf-8"),
            ),
            atom("reset_trajectory_goldens_file_sha256_raw32", raw32(RESET_GOLDENS_FILE_SHA256)),
            atom("reset_trajectory_vector_stream_sha256_raw32", raw32(RESET_STREAM_SHA256)),
            atom("trainer_schedule_identity_utf8", TRAINER_SCHEDULE_IDENTITY.encode("utf-8")),
            atom("trainer_seed_version_utf8", TRAINER_SEED_VERSION.encode("utf-8")),
            atom(
                "trainer_schedule_goldens_file_sha256_raw32",
                raw32(TRAINER_SCHEDULE_GOLDENS_FILE_SHA256),
            ),
            atom("runtime_deck_catalog_schema_utf8", RUNTIME_DECK_CATALOG_SCHEMA.encode("utf-8")),
            atom("runtime_deck_protocol_utf8", RUNTIME_DECK_PROTOCOL.encode("utf-8")),
            atom(
                "runtime_deck_materialization_protocol_utf8",
                RUNTIME_DECK_MATERIALIZATION_PROTOCOL.encode("utf-8"),
            ),
            atom("runtime_deck_hash_algorithm_utf8", RUNTIME_DECK_HASH_ALGORITHM.encode("utf-8")),
            atom(
                "runtime_deck_catalog_file_sha256_raw32",
                raw32(RUNTIME_DECK_CATALOG_FILE_SHA256),
            ),
            atom("episode_index_u64be", episode_index.to_bytes(8, "big")),
            atom("pair_index_u64be", (episode_index // 2).to_bytes(8, "big")),
            atom(
                "pair_environment_seed_u64be",
                parse_fixed_hex(
                    start["pair_environment_seed_u64_hex"], 16, "pair environment seed"
                ).to_bytes(8, "big"),
            ),
            atom("deck_p0_id_utf8", start["deck_p0_id"].encode("utf-8")),
            atom(
                "deck_p0_hash_u64be",
                parse_fixed_hex(start["deck_p0_hash_u64_hex"], 16, "deck P0 hash").to_bytes(
                    8, "big"
                ),
            ),
            atom("deck_p1_id_utf8", start["deck_p1_id"].encode("utf-8")),
            atom(
                "deck_p1_hash_u64be",
                parse_fixed_hex(start["deck_p1_hash_u64_hex"], 16, "deck P1 hash").to_bytes(
                    8, "big"
                ),
            ),
            atom("learner_seat_u8", bytes((seat_code(learner_seat),))),
            atom("inner_trajectory_sha256_raw32", inner_sha256_raw),
        )
    )


def evaluate_trajectory(start_input: Any) -> tuple[bytes, bytes]:
    """Owned-inner evaluation: validate the start, build and finish the inner V1
    stream from it, then envelope the digest it produced."""
    start = require_exact_object(
        start_input, TRAJECTORY_INPUT_FIELDS, "trajectory input"
    )
    # Authority binding is checked first: drifted authority can never reach the
    # owned inner accumulator.
    require_authorities(start["source_authorities"])
    inner_stream = owned_inner_stream(start)
    inner_digest = hashlib.sha256(inner_stream).digest()
    return inner_stream, v2_envelope(start, inner_digest)


def evaluate_pair(pair_input: Any) -> tuple[str, str]:
    """Closed pair arithmetic. Both starts are validated independently before
    any pair comparison."""
    record = require_exact_object(pair_input, PAIR_INPUT_FIELDS, "pair input")
    base_seed = parse_fixed_hex(record["base_seed_u64_hex"], 16, "base seed")
    if base_seed > U63_MAX:
        reject("schedule-integer-outside-u63")
    pair_index = parse_fixed_hex(record["pair_index_u64_hex"], 16, "pair index")
    if pair_index > U62_MAX:
        reject("pair-index-outside-episode-domain")

    even_inner, even_envelope = evaluate_trajectory(record["even_start"])
    odd_inner, odd_envelope = evaluate_trajectory(record["odd_start"])

    even = record["even_start"]
    odd = record["odd_start"]
    even_episode = parse_fixed_hex(even["episode_index_u64_hex"], 16, "even episode")
    odd_episode = parse_fixed_hex(odd["episode_index_u64_hex"], 16, "odd episode")
    if even_episode // 2 != pair_index or odd_episode // 2 != pair_index:
        reject("pair-episode-index-mismatch")
    if even_episode != 2 * pair_index or odd_episode != 2 * pair_index + 1:
        reject("pair-episode-index-mismatch")
    if even["learner_seat"] != "p0" or odd["learner_seat"] != "p1":
        reject("learner-seat-rule-mismatch")

    derived = derive_train_env_root(base_seed, pair_index)
    for side in (even, odd):
        if parse_fixed_hex(side["pair_environment_seed_u64_hex"], 16, "root") != derived:
            reject("pair-environment-seed-mismatch")
    if (
        even["deck_p0_id"],
        even["deck_p0_hash_u64_hex"],
        even["deck_p1_id"],
        even["deck_p1_hash_u64_hex"],
    ) != (
        odd["deck_p0_id"],
        odd["deck_p0_hash_u64_hex"],
        odd["deck_p1_id"],
        odd["deck_p1_hash_u64_hex"],
    ):
        reject("pair-physical-deck-binding-mismatch")
    return sha256_hex(even_envelope), sha256_hex(odd_envelope)


# ------------------------------------------------------------------ fixtures

NATIVE_BASE_SEED = 71_501
NATIVE_PAIR_INDEX = 0
NATIVE_ROOT = 5_293_664_275_683_392_565
ROOT_940001 = 940_001
ROOT_940000 = 940_000
ROOT_U64_MAX = U64_MAX

BURN = "Burn"
RALLY = "Rally"
BURN_HASH = FROZEN_RUNTIME_DECKS[BURN]
RALLY_HASH = FROZEN_RUNTIME_DECKS[RALLY]


def build_decisions(learner_seat: str, groups: list[tuple[str, int]]) -> list[dict]:
    """Build a valid decision stream from (seat, substep_count) groups."""
    decisions: list[dict] = []
    row_ordinal = 0
    physical = 0
    actor_physical = {"learner": 0, "opponent": 0}
    for seat, substep_count in groups:
        role = "learner" if seat == learner_seat else "opponent"
        actor_ordinal = actor_physical[role]
        for substep_index in range(substep_count):
            decisions.append(
                {
                    "row_ordinal_u64_hex": u64_hex(row_ordinal),
                    "actor_seat": seat,
                    "actor_role": role,
                    "physical_decision_ordinal_u64_hex": u64_hex(physical),
                    "actor_physical_decision_ordinal_u64_hex": u64_hex(actor_ordinal),
                    "substep_index_u32": substep_index,
                    "substep_count_u32": substep_count,
                    "action_seed_u64_hex": u64_hex(0x5EED_0000 + row_ordinal),
                    "legal_action_count_u32": 4,
                    "selected_index_u32": row_ordinal % 4,
                    "flat_action_v2_commitment_hex": f"{0xC0_0000 + row_ordinal:032x}",
                }
            )
            row_ordinal += 1
        actor_physical[role] += 1
        physical += 1
    return decisions


def build_terminal(episode_index: int, outcome: str, policy: int, physical: int) -> dict:
    winner = {"p0-win": "p0", "p1-win": "p1", "draw": "none"}[outcome]
    return {
        "episode_index_u64_hex": u64_hex(episode_index),
        "deck_p0_hash_u64_hex": u64_hex(BURN_HASH),
        "deck_p1_hash_u64_hex": u64_hex(RALLY_HASH),
        "outcome": outcome,
        "winner": winner,
        "classification": "natural",
        "terminal_code": "natural-game-over",
        "policy_step_count_u64_hex": u64_hex(policy),
        "physical_decision_count_u64_hex": u64_hex(physical),
    }


def build_start(episode_index: int, root: int, outcome: str, groups: list[tuple[str, int]]) -> dict:
    learner_seat = learner_seat_for_episode(episode_index)
    decisions = build_decisions(learner_seat, groups)
    return {
        "source_authorities": copy.deepcopy(EXPECTED_SOURCE_AUTHORITIES),
        "episode_index_u64_hex": u64_hex(episode_index),
        "pair_environment_seed_u64_hex": u64_hex(root),
        "deck_p0_id": BURN,
        "deck_p0_hash_u64_hex": u64_hex(BURN_HASH),
        "deck_p1_id": RALLY,
        "deck_p1_hash_u64_hex": u64_hex(RALLY_HASH),
        "learner_seat": learner_seat,
        "decisions": decisions,
        "terminal": build_terminal(episode_index, outcome, len(decisions), len(groups)),
    }


# Both actor roles, all three natural outcomes, and a multi-substep group.
EVEN_GROUPS = [("p0", 3), ("p1", 1), ("p0", 1)]
ODD_GROUPS = [("p0", 1), ("p1", 2), ("p0", 1)]

POSITIVE_SPECS = [
    ("episode-0-native-root-learner-p0-p0-win", 0, NATIVE_ROOT, "p0-win", EVEN_GROUPS),
    ("episode-1-native-root-learner-p1-p1-win", 1, NATIVE_ROOT, "p1-win", ODD_GROUPS),
    ("episode-2-root-940000-draw-red-mate", 2, ROOT_940000, "draw", EVEN_GROUPS),
    ("episode-2-root-940001-draw", 2, ROOT_940001, "draw", EVEN_GROUPS),
    ("episode-3-root-u64-max-learner-p1-p0-win", 3, ROOT_U64_MAX, "p0-win", ODD_GROUPS),
]


def build_positive_cases() -> list[dict]:
    cases = []
    for name, episode, root, outcome, groups in POSITIVE_SPECS:
        start = build_start(episode, root, outcome, groups)
        inner_stream, envelope = evaluate_trajectory(start)
        cases.append(
            {
                "name": name,
                "input": start,
                "inner_stream_hex": inner_stream.hex(),
                "inner_sha256": sha256_hex(inner_stream),
                "v2_stream_hex": envelope.hex(),
                "v2_sha256": sha256_hex(envelope),
            }
        )
    cases.sort(key=lambda case: case["name"])
    return cases


def build_pair_positive_cases() -> list[dict]:
    even = build_start(0, NATIVE_ROOT, "p0-win", EVEN_GROUPS)
    odd = build_start(1, NATIVE_ROOT, "p1-win", ODD_GROUPS)
    record = {
        "base_seed_u64_hex": u64_hex(NATIVE_BASE_SEED),
        "pair_index_u64_hex": u64_hex(NATIVE_PAIR_INDEX),
        "even_start": even,
        "odd_start": odd,
    }
    even_sha, odd_sha = evaluate_pair(record)
    return [
        {
            "name": "pair-native-base-71501-index-0",
            "input": record,
            "even_trajectory_sha256": even_sha,
            "odd_trajectory_sha256": odd_sha,
        }
    ]


def mutate(base: dict, path: tuple, value: Any) -> dict:
    record = copy.deepcopy(base)
    target = record
    for part in path[:-1]:
        target = target[part]
    target[path[-1]] = value
    return record


def drop(base: dict, path: tuple) -> dict:
    record = copy.deepcopy(base)
    target = record
    for part in path[:-1]:
        target = target[part]
    del target[path[-1]]
    return record


def build_trajectory_reject_cases() -> list[dict]:
    base = build_start(0, NATIVE_ROOT, "p0-win", EVEN_GROUPS)
    cases: list[tuple[str, dict, str]] = []

    drifted = copy.deepcopy(base)
    drifted["source_authorities"]["environment_randomization"]["namespace"] = "drifted"
    cases.append(("authority-environment-namespace-drift", drifted, "authority-mismatch"))

    cases.append(
        (
            "episode-index-two-pow-63",
            mutate(base, ("episode_index_u64_hex",), u64_hex(1 << 63)),
            "episode-index-outside-u63",
        )
    )
    cases.append(
        (
            "learner-seat-parity-mismatch",
            mutate(base, ("learner_seat",), "p1"),
            "learner-seat-rule-mismatch",
        )
    )
    cases.append(
        (
            "deck-id-case-drift-burn",
            mutate(base, ("deck_p0_id",), "burn"),
            "invalid-deck-id",
        )
    )
    cases.append(
        (
            "deck-id-unknown-not-in-catalog",
            mutate(base, ("deck_p0_id",), "Nonexistent"),
            "invalid-deck-id",
        )
    )
    cases.append(
        (
            "runtime-deck-hash-mismatch-p1",
            mutate(base, ("deck_p1_hash_u64_hex",), u64_hex(RALLY_HASH ^ 1)),
            "runtime-deck-hash-mismatch",
        )
    )
    empty = copy.deepcopy(base)
    empty["decisions"] = []
    empty["terminal"]["policy_step_count_u64_hex"] = u64_hex(0)
    empty["terminal"]["physical_decision_count_u64_hex"] = u64_hex(0)
    cases.append(("empty-decision-stream", empty, "empty-decision-stream"))
    cases.append(
        (
            "row-ordinal-mismatch",
            mutate(base, ("decisions", 1, "row_ordinal_u64_hex"), u64_hex(7)),
            "row-ordinal-mismatch",
        )
    )
    cases.append(
        (
            "actor-role-mismatch",
            mutate(base, ("decisions", 0, "actor_role"), "opponent"),
            "actor-role-mismatch",
        )
    )
    cases.append(
        (
            "incomplete-physical-group",
            mutate(base, ("decisions", 2, "substep_count_u32"), 9),
            "malformed-physical-group",
        )
    )
    cases.append(
        (
            "malformed-group-substep-index-at-count",
            mutate(base, ("decisions", 0, "substep_index_u32"), 3),
            "malformed-physical-group",
        )
    )
    cases.append(
        (
            "legal-action-count-zero",
            mutate(base, ("decisions", 0, "legal_action_count_u32"), 0),
            "invalid-legal-action-count",
        )
    )
    cases.append(
        (
            "legal-action-count-sixty-five",
            mutate(base, ("decisions", 0, "legal_action_count_u32"), 65),
            "invalid-legal-action-count",
        )
    )
    cases.append(
        (
            "selected-index-equal-width",
            mutate(base, ("decisions", 0, "selected_index_u32"), 4),
            "selected-index-out-of-range",
        )
    )
    cases.append(
        (
            "malformed-commitment-short",
            mutate(base, ("decisions", 0, "flat_action_v2_commitment_hex"), "ab" * 15),
            "malformed-commitment",
        )
    )
    cases.append(
        (
            "terminal-episode-mismatch",
            mutate(base, ("terminal", "episode_index_u64_hex"), u64_hex(9)),
            "episode-mismatch",
        )
    )
    non_natural = copy.deepcopy(base)
    non_natural["terminal"]["outcome"] = "truncated"
    non_natural["terminal"]["winner"] = "none"
    non_natural["terminal"]["classification"] = "truncated"
    non_natural["terminal"]["terminal_code"] = "decision-cap"
    cases.append(("non-natural-terminal", non_natural, "non-natural-terminal"))
    cases.append(
        (
            "terminal-deck-provenance-mismatch",
            mutate(base, ("terminal", "deck_p0_hash_u64_hex"), u64_hex(RALLY_HASH)),
            "terminal-provenance-mismatch",
        )
    )
    cases.append(
        (
            "terminal-count-mismatch",
            mutate(base, ("terminal", "policy_step_count_u64_hex"), u64_hex(99)),
            "terminal-count-mismatch",
        )
    )

    compound_empty = copy.deepcopy(base)
    compound_empty["decisions"] = []
    compound_empty["terminal"]["policy_step_count_u64_hex"] = u64_hex(0)
    compound_empty["terminal"]["physical_decision_count_u64_hex"] = u64_hex(0)
    compound_empty["terminal"]["episode_index_u64_hex"] = u64_hex(9)
    cases.append(
        (
            "compound-empty-stream-and-terminal-episode-mismatch",
            compound_empty,
            "episode-mismatch",
        )
    )
    compound_open = copy.deepcopy(base)
    # Widen the final group's only row so the group is left open at stream end
    # without tripping the in-loop continuation check first.
    compound_open["decisions"][-1]["substep_count_u32"] = 9
    compound_open["terminal"]["episode_index_u64_hex"] = u64_hex(9)
    cases.append(
        (
            "compound-open-group-and-terminal-episode-mismatch",
            compound_open,
            "episode-mismatch",
        )
    )

    built = []
    for name, record, code in cases:
        try:
            evaluate_trajectory(record)
        except ContractRejection as error:
            observed = error.code
        else:
            raise AssertionError(f"trajectory reject {name!r} unexpectedly admitted")
        if observed != code:
            raise AssertionError(f"trajectory reject {name!r}: expected {code}, observed {observed}")
        built.append({"name": name, "input": record, "expected_code": code})
    built.sort(key=lambda case: case["name"])
    return built


def build_pair_reject_cases() -> list[dict]:
    even = build_start(0, NATIVE_ROOT, "p0-win", EVEN_GROUPS)
    odd = build_start(1, NATIVE_ROOT, "p1-win", ODD_GROUPS)
    base = {
        "base_seed_u64_hex": u64_hex(NATIVE_BASE_SEED),
        "pair_index_u64_hex": u64_hex(NATIVE_PAIR_INDEX),
        "even_start": even,
        "odd_start": odd,
    }
    cases: list[tuple[str, dict, str]] = []
    cases.append(
        (
            "pair-base-seed-two-pow-63",
            mutate(base, ("base_seed_u64_hex",), u64_hex(1 << 63)),
            "schedule-integer-outside-u63",
        )
    )
    cases.append(
        (
            "pair-index-two-pow-62",
            mutate(base, ("pair_index_u64_hex",), u64_hex(1 << 62)),
            "pair-index-outside-episode-domain",
        )
    )
    # An internally valid odd start whose episode is not 2k+1 for the declared
    # pair index. Episode 3 keeps parity (p1 learner) and a matching terminal,
    # so independent validation passes and the pair-stage check is what fires.
    drifted_pair = copy.deepcopy(base)
    drifted_pair["odd_start"] = build_start(3, NATIVE_ROOT, "p0-win", ODD_GROUPS)
    cases.append(
        ("pair-odd-episode-index-drift", drifted_pair, "pair-episode-index-mismatch")
    )
    cases.append(
        (
            "pair-odd-root-drift",
            mutate(base, ("odd_start", "pair_environment_seed_u64_hex"), u64_hex(NATIVE_ROOT - 1)),
            "pair-environment-seed-mismatch",
        )
    )
    swapped = copy.deepcopy(base)
    swapped["odd_start"]["deck_p0_id"] = RALLY
    swapped["odd_start"]["deck_p0_hash_u64_hex"] = u64_hex(RALLY_HASH)
    swapped["odd_start"]["deck_p1_id"] = BURN
    swapped["odd_start"]["deck_p1_hash_u64_hex"] = u64_hex(BURN_HASH)
    swapped["odd_start"]["terminal"]["deck_p0_hash_u64_hex"] = u64_hex(RALLY_HASH)
    swapped["odd_start"]["terminal"]["deck_p1_hash_u64_hex"] = u64_hex(BURN_HASH)
    cases.append(
        ("pair-odd-physical-deck-swap", swapped, "pair-physical-deck-binding-mismatch")
    )
    not_swapped = copy.deepcopy(base)
    not_swapped["odd_start"]["learner_seat"] = "p0"
    cases.append(
        ("pair-learner-seat-not-swapped", not_swapped, "learner-seat-rule-mismatch")
    )

    built = []
    for name, record, code in cases:
        try:
            evaluate_pair(record)
        except ContractRejection as error:
            observed = error.code
        else:
            raise AssertionError(f"pair reject {name!r} unexpectedly admitted")
        if observed != code:
            raise AssertionError(f"pair reject {name!r}: expected {code}, observed {observed}")
        built.append({"name": name, "input": record, "expected_code": code})
    built.sort(key=lambda case: case["name"])
    return built


# ---------------------------------------------------------- artifact + stream


def build_artifact() -> dict:
    artifact = {
        "schema": SCHEMA,
        "generator_identity": GENERATOR_IDENTITY,
        "trajectory_identity": TRAJECTORY_IDENTITY_V2,
        "vector_stream_identity": VECTOR_STREAM_IDENTITY,
        "source_authorities": copy.deepcopy(EXPECTED_SOURCE_AUTHORITIES),
        "positive_cases": build_positive_cases(),
        "pair_positive_cases": build_pair_positive_cases(),
        "trajectory_reject_cases": build_trajectory_reject_cases(),
        "pair_reject_cases": build_pair_reject_cases(),
    }
    audit_artifact(artifact)
    return artifact


ARTIFACT_FIELDS = {
    "schema",
    "generator_identity",
    "trajectory_identity",
    "vector_stream_identity",
    "source_authorities",
    "positive_cases",
    "pair_positive_cases",
    "trajectory_reject_cases",
    "pair_reject_cases",
}
POSITIVE_CASE_FIELDS = {
    "name",
    "input",
    "inner_stream_hex",
    "inner_sha256",
    "v2_stream_hex",
    "v2_sha256",
}
PAIR_POSITIVE_CASE_FIELDS = {
    "name",
    "input",
    "even_trajectory_sha256",
    "odd_trajectory_sha256",
}
REJECT_CASE_FIELDS = {"name", "input", "expected_code"}


def require_names(cases: Any, label: str) -> None:
    if not isinstance(cases, list) or not cases:
        raise ShapeError(f"{label} must be a nonempty array")
    if len(cases) > MAX_CASES:
        raise ShapeError(f"{label} exceeds the {MAX_CASES} case cap")
    names = []
    for case in cases:
        if not isinstance(case, dict) or not isinstance(case.get("name"), str):
            raise ShapeError(f"{label} entry has no string name")
        names.append(case["name"])
    for name in names:
        if NAME_RE.fullmatch(name) is None:
            raise ShapeError(f"{label}: name {name!r} violates the grammar")
    if names != sorted(names) or len(set(names)) != len(names):
        raise ShapeError(f"{label}: names are not strictly increasing and unique")


def require_hex_stream(value: Any, label: str) -> bytes:
    if not isinstance(value, str) or len(value) % 2 != 0 or not value:
        raise ShapeError(f"{label} is not a nonempty even-length hex string")
    if any(char not in "0123456789abcdef" for char in value):
        raise ShapeError(f"{label} is not lowercase hex")
    return bytes.fromhex(value)


AUTHORITY_BLOCK_FIELDS = {
    block: set(fields) for block, fields in EXPECTED_SOURCE_AUTHORITIES.items()
}


def require_authority_shape(authorities: Any, label: str) -> None:
    """Field-set only. Values may be wrong; that is authority-mismatch's job."""
    record = require_exact_object(authorities, set(AUTHORITY_BLOCK_FIELDS), label)
    for block, fields in AUTHORITY_BLOCK_FIELDS.items():
        require_exact_object(record[block], fields, f"{label}.{block}")


def require_trajectory_shape(candidate: Any, label: str) -> None:
    """Purely structural traversal of a trajectory record. It admits
    intentionally invalid values such as a malformed commitment or an
    out-of-domain episode index, and rejects only unknown or missing fields and
    array-shape violations. Running this before classification stops an early
    rejection such as authority-mismatch from hiding a shape defect."""
    record = require_exact_object(candidate, TRAJECTORY_INPUT_FIELDS, label)
    require_authority_shape(record["source_authorities"], f"{label}.source_authorities")
    decisions = record["decisions"]
    if not isinstance(decisions, list):
        raise ShapeError(f"{label}.decisions is not an array")
    if len(decisions) > MAX_DECISIONS:
        raise ShapeError(f"{label}.decisions exceeds the {MAX_DECISIONS} cap")
    for index, row in enumerate(decisions):
        require_exact_object(row, INNER_DECISION_FIELDS, f"{label}.decisions[{index}]")
    require_exact_object(record["terminal"], INNER_TERMINAL_FIELDS, f"{label}.terminal")


def require_pair_shape(candidate: Any, label: str) -> None:
    record = require_exact_object(candidate, PAIR_INPUT_FIELDS, label)
    require_trajectory_shape(record["even_start"], f"{label}.even_start")
    require_trajectory_shape(record["odd_start"], f"{label}.odd_start")


def audit_artifact(artifact: Any) -> None:
    """Closed validation: exact field sets at every level, frozen identities,
    strict scalars, caps and names, fixed-width lowercase hex, and full
    recomputation of every stored stream and digest."""
    record = require_exact_object(artifact, ARTIFACT_FIELDS, "artifact")
    for key, frozen in (
        ("schema", SCHEMA),
        ("generator_identity", GENERATOR_IDENTITY),
        ("trajectory_identity", TRAJECTORY_IDENTITY_V2),
        ("vector_stream_identity", VECTOR_STREAM_IDENTITY),
    ):
        if record[key] != frozen:
            raise ShapeError(f"artifact {key} is not the frozen identity")
    require_authorities(record["source_authorities"])

    for key in (
        "positive_cases",
        "pair_positive_cases",
        "trajectory_reject_cases",
        "pair_reject_cases",
    ):
        require_names(record[key], key)

    for case in record["positive_cases"]:
        require_exact_object(case, POSITIVE_CASE_FIELDS, f"positive case {case['name']}")
        inner_bytes = require_hex_stream(case["inner_stream_hex"], "inner_stream_hex")
        v2_bytes = require_hex_stream(case["v2_stream_hex"], "v2_stream_hex")
        require_lower_hex(case["inner_sha256"], 64, "inner_sha256")
        require_lower_hex(case["v2_sha256"], 64, "v2_sha256")
        require_trajectory_shape(case["input"], f"positive case {case['name']} input")
        inner_stream, envelope = evaluate_trajectory(case["input"])
        if inner_stream != inner_bytes or envelope != v2_bytes:
            raise ShapeError(f"{case['name']}: stored stream bytes drifted")
        if sha256_hex(inner_stream) != case["inner_sha256"]:
            raise ShapeError(f"{case['name']}: inner digest drifted")
        if sha256_hex(envelope) != case["v2_sha256"]:
            raise ShapeError(f"{case['name']}: V2 digest drifted")

    for case in record["pair_positive_cases"]:
        require_exact_object(
            case, PAIR_POSITIVE_CASE_FIELDS, f"pair positive case {case['name']}"
        )
        require_lower_hex(case["even_trajectory_sha256"], 64, "even_trajectory_sha256")
        require_lower_hex(case["odd_trajectory_sha256"], 64, "odd_trajectory_sha256")
        require_pair_shape(case["input"], f"pair positive case {case['name']} input")
        even_sha, odd_sha = evaluate_pair(case["input"])
        if (even_sha, odd_sha) != (
            case["even_trajectory_sha256"],
            case["odd_trajectory_sha256"],
        ):
            raise ShapeError(f"{case['name']}: stored pair digests drifted")

    for case in record["trajectory_reject_cases"]:
        require_exact_object(case, REJECT_CASE_FIELDS, f"trajectory reject {case['name']}")
        if case["expected_code"] not in REJECTION_CODES:
            raise ShapeError(f"{case['name']}: code outside the closed vocabulary")
        require_trajectory_shape(case["input"], f"trajectory reject {case['name']} input")
        try:
            evaluate_trajectory(case["input"])
        except ContractRejection as error:
            if error.code != case["expected_code"]:
                raise ShapeError(
                    f"{case['name']}: expected {case['expected_code']}, observed {error.code}"
                ) from error
        else:
            raise ShapeError(f"{case['name']}: reject input was admitted")

    for case in record["pair_reject_cases"]:
        require_exact_object(case, REJECT_CASE_FIELDS, f"pair reject {case['name']}")
        if case["expected_code"] not in REJECTION_CODES:
            raise ShapeError(f"{case['name']}: code outside the closed vocabulary")
        require_pair_shape(case["input"], f"pair reject {case['name']} input")
        try:
            evaluate_pair(case["input"])
        except ContractRejection as error:
            if error.code != case["expected_code"]:
                raise ShapeError(
                    f"{case['name']}: expected {case['expected_code']}, observed {error.code}"
                ) from error
        else:
            raise ShapeError(f"{case['name']}: pair reject input was admitted")

    body = canonical_file_bytes(record)
    if len(body) > MAX_ARTIFACT_BYTES:
        raise ShapeError("artifact exceeds its byte ceiling")
    if not body.isascii() or b"\r" in body:
        raise ShapeError("artifact must be ASCII without carriage returns")
    if not body.endswith(b"\n") or body.count(b"\n") != 1:
        raise ShapeError("artifact must end with exactly one LF")


def u32be(value: int) -> bytes:
    if not 0 <= value <= U32_MAX:
        raise ShapeError("count outside u32")
    return value.to_bytes(4, "big")


def semantic_stream(artifact: dict) -> bytes:
    parts = [
        atom("domain", VECTOR_STREAM_IDENTITY.encode("ascii")),
        atom("schema_utf8", artifact["schema"].encode("utf-8")),
        atom("generator_identity_utf8", artifact["generator_identity"].encode("utf-8")),
        atom("trajectory_identity_utf8", artifact["trajectory_identity"].encode("utf-8")),
        atom(
            "source_authorities_canonical_json_utf8",
            canonical_json_no_lf(artifact["source_authorities"]),
        ),
        atom("positive_case_count_u32be", u32be(len(artifact["positive_cases"]))),
    ]
    for case in artifact["positive_cases"]:
        parts.append(atom("positive_case_name_ascii", case["name"].encode("ascii")))
        parts.append(
            atom(
                "positive_case_input_canonical_json_utf8",
                canonical_json_no_lf(case["input"]),
            )
        )
        parts.append(atom("positive_case_inner_stream_raw", bytes.fromhex(case["inner_stream_hex"])))
        parts.append(atom("positive_case_inner_sha256_raw32", raw32(case["inner_sha256"])))
        parts.append(atom("positive_case_v2_stream_raw", bytes.fromhex(case["v2_stream_hex"])))
        parts.append(atom("positive_case_v2_sha256_raw32", raw32(case["v2_sha256"])))
    parts.append(
        atom("pair_positive_case_count_u32be", u32be(len(artifact["pair_positive_cases"])))
    )
    for case in artifact["pair_positive_cases"]:
        parts.append(atom("pair_positive_case_name_ascii", case["name"].encode("ascii")))
        parts.append(
            atom(
                "pair_positive_case_input_canonical_json_utf8",
                canonical_json_no_lf(case["input"]),
            )
        )
        parts.append(
            atom(
                "pair_positive_even_trajectory_sha256_raw32",
                raw32(case["even_trajectory_sha256"]),
            )
        )
        parts.append(
            atom(
                "pair_positive_odd_trajectory_sha256_raw32",
                raw32(case["odd_trajectory_sha256"]),
            )
        )
    parts.append(
        atom(
            "trajectory_reject_case_count_u32be",
            u32be(len(artifact["trajectory_reject_cases"])),
        )
    )
    for case in artifact["trajectory_reject_cases"]:
        parts.append(atom("trajectory_reject_case_name_ascii", case["name"].encode("ascii")))
        parts.append(
            atom(
                "trajectory_reject_case_input_canonical_json_utf8",
                canonical_json_no_lf(case["input"]),
            )
        )
        parts.append(
            atom("trajectory_reject_expected_code_ascii", case["expected_code"].encode("ascii"))
        )
    parts.append(atom("pair_reject_case_count_u32be", u32be(len(artifact["pair_reject_cases"]))))
    for case in artifact["pair_reject_cases"]:
        parts.append(atom("pair_reject_case_name_ascii", case["name"].encode("ascii")))
        parts.append(
            atom(
                "pair_reject_case_input_canonical_json_utf8",
                canonical_json_no_lf(case["input"]),
            )
        )
        parts.append(
            atom("pair_reject_expected_code_ascii", case["expected_code"].encode("ascii"))
        )
    return b"".join(parts)


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Generate or verify the native full-episode trajectory V2 goldens."
    )
    parser.add_argument("--out", type=Path, default=OUTPUT)
    parser.add_argument("--check", action="store_true")
    arguments = parser.parse_args()

    artifact = build_artifact()
    body = canonical_file_bytes(artifact)
    stream = semantic_stream(artifact)
    file_sha = sha256_hex(body)
    stream_sha = sha256_hex(stream)

    if arguments.check:
        if not arguments.out.exists():
            print(f"CHECK FAIL: {arguments.out} does not exist")
            return 1
        existing = arguments.out.read_bytes()
        audit_artifact(strict_json_loads(existing, "stored artifact"))
        if existing != body:
            print(
                f"CHECK FAIL: {arguments.out} differs from recomputation "
                f"(on disk {sha256_hex(existing)}, recomputed {file_sha})"
            )
            return 1
        print("CHECK OK")
    else:
        arguments.out.parent.mkdir(parents=True, exist_ok=True)
        arguments.out.write_bytes(body)
        print(f"wrote {arguments.out}")

    print(f"positive_cases          {len(artifact['positive_cases'])}")
    print(f"pair_positive_cases     {len(artifact['pair_positive_cases'])}")
    print(f"trajectory_reject_cases {len(artifact['trajectory_reject_cases'])}")
    print(f"pair_reject_cases       {len(artifact['pair_reject_cases'])}")
    print(f"file_bytes              {len(body)}")
    print(f"stream_bytes            {len(stream)}")
    print(f"raw_file_sha256         {file_sha}")
    print(f"stream_sha256           {stream_sha}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
