#!/usr/bin/env python3
"""Stdlib-only generator for the portable environment-randomization-v2 reset
physical-trajectory goldens.

Frozen contract: collab TO-CLAUDE.md, "CODEX RULING: EXACT RESET-GOLDEN JSON
AND SEMANTIC BYTES". This tool owns only the physical card-definition
projection of the portable reset boundary: the state immediately after the
fourteen alternating opening draws and before session policy advance. It
imports no Rust, calls no kernel binary, reads no build output, and reads no
run logs.

Authority chain, each raw-hashed before parse or import:

  data/runtime_decks_v1.json                             strict parse
  python/tools/environment_randomization_v2_reference.py  importlib KDF/shuffle
  data/environment_randomization_v2/goldens_v1.json       KDF golden binding
  data/native_trainer_schedule_v1_goldens.json            schedule binding

The native `train-env` root derivation is reimplemented here independently and
uses `kernel-python-rl-trainer-sha256-v2` as its version atom. The native
schedule identity is an artifact binding, never the version atom.

Usage:
  python generate_environment_randomization_v2_reset_physical_trajectory_goldens_v1.py
  python generate_environment_randomization_v2_reset_physical_trajectory_goldens_v1.py --check
"""

from __future__ import annotations

import sys

# Set before any project import so the focused generator and unittest leave no
# __pycache__ artifact behind.
sys.dont_write_bytecode = True

import argparse  # noqa: E402
import copy  # noqa: E402
import hashlib  # noqa: E402
import importlib.util  # noqa: E402
import json  # noqa: E402
import re  # noqa: E402
from pathlib import Path  # noqa: E402

SCHEMA = "mtg-kernel-environment-randomization-v2-reset-physical-trajectory-goldens/v1"
GENERATOR_IDENTITY = (
    "mtg-kernel-environment-randomization-v2-reset-physical-trajectory-goldens"
    "-stdlib-python-v1"
)
PHYSICAL_PROJECTION_IDENTITY = (
    "mtg-kernel-environment-randomization-v2-physical-card-definition-projection/v1"
)
PORTABLE_VECTOR_STREAM_IDENTITY = (
    "mtg-kernel-environment-randomization-v2-reset-physical-trajectory"
    "-portable-vector-stream-sha256-v1"
)

RUNTIME_DECK_CATALOG_SHA256 = (
    "5ea19e8a08f0e9c9657e9a6a90382329785f27eeabbbe066e80e7025e8ee62c0"
)
PYTHON_REFERENCE_SHA256 = (
    "9dd7e5357d98ff5a7ac302d285da91fb56cf0d422c5aef6bc9b53f2a5d822024"
)
KDF_GOLDENS_SHA256 = "bc2b0d66f8e3eb608b6035321f23a214bbf5141aaf7305f50f606f6c85b4a3bc"
KDF_GOLDENS_SCHEMA = "mtg-kernel-environment-randomization-v2-goldens/v1"
NATIVE_SCHEDULE_IDENTITY = "mtg-kernel-native-trainer-schedule-sha256-v1"
NATIVE_SCHEDULE_GOLDENS_SCHEMA = "mtg_kernel_native_trainer_schedule_goldens/v1"
NATIVE_SCHEDULE_GOLDENS_SHA256 = (
    "6b2e1edbbe49b4e02f98794f9057f5c2bb8e3079d2ba8cb3e2a4b9ea6c34867c"
)
TRAINER_VERSION_ATOM = "kernel-python-rl-trainer-sha256-v2"
TRAIN_ENV_NAMESPACE = "train-env"

EXPECTED_CATALOG_SCHEMA = "kernel_runtime_decks/v1"
EXPECTED_CATALOG_PROTOCOL = "canonical-mainboard-bo1/v1"
EXPECTED_MATERIALIZATION_ORDER = "xmage_xml_row_then_copy_ordinal/v1"
EXPECTED_DECK_HASH_ALGORITHM = "fnv1a64-serde-json-u16-array/v1"

BURN_DECK_ID = "Burn"
RALLY_DECK_ID = "Rally"
OWNERS = ("p0", "p1")
INITIAL_SHUFFLE_PURPOSE = "initial-library-shuffle"
OPENING_HAND_COUNT = 7
OPENING_DRAW_ROUNDS = 7

MAX_ARTIFACT_BYTES = 1024 * 1024
MAX_RESET_CASES = 8
MAX_PAIRED_CASES = 8
MAX_REJECT_CASES = 32
MAX_CARDS_PER_DECK = 256
MAX_DRAW_EVENTS = 32

U16_MAX = 0xFFFF
U32_MAX = 0xFFFF_FFFF
U64_MAX = 0xFFFF_FFFF_FFFF_FFFF
U63_MAX = (1 << 63) - 1

# Unanchored bodies deliberately paired with `.fullmatch()`. An anchored
# `re.match(r"...$")` accepts a terminal LF, which none of these domains admit.
CASE_NAME_BODY = re.compile(r"[a-z0-9][a-z0-9-]{0,127}")
SHA256_BODY = re.compile(r"[0-9a-f]{64}")
DECK_HASH_BODY = re.compile(r"[0-9a-f]{16}")

EXACT_RESET_CASE_COUNT = 2
EXACT_PAIRED_CASE_COUNT = 1
EXACT_REJECT_CASE_COUNT = 6

# Stored, stable reject vocabulary. Exactly these six codes appear in the
# artifact; the structural codes below them are in-memory only and mint no
# production error variant.
CODE_SEAT = "learner-seat-rule-mismatch"
CODE_ROOT = "pair-environment-seed-mismatch"
CODE_DECKS = "physical-deck-binding-mismatch"
CODE_BIJECTION = "source-permutation-not-bijection"
CODE_RANGE = "source-permutation-index-out-of-range"
CODE_PROJECTION = "source-permutation-card-projection-mismatch"
STORED_REJECTION_CODES = (
    CODE_SEAT,
    CODE_ROOT,
    CODE_DECKS,
    CODE_BIJECTION,
    CODE_RANGE,
    CODE_PROJECTION,
)

# In-memory structural codes: proof of rejection only, never stored.
CODE_LENGTH = "source-permutation-length-mismatch"
CODE_TRAJECTORY = "hand-library-draw-inconsistent"
CODE_SCHEDULE_IDENTITY = "trainer-schedule-identity-mismatch"
CODE_EPISODE = "episode-index-rule-mismatch"
CODE_SHARED = "shared-reset-case-reference-mismatch"

REPO_ROOT = Path(__file__).resolve().parents[2]
RUNTIME_DECK_CATALOG_PATH = REPO_ROOT / "data" / "runtime_decks_v1.json"
PYTHON_REFERENCE_PATH = (
    REPO_ROOT / "python" / "tools" / "environment_randomization_v2_reference.py"
)
KDF_GOLDENS_PATH = (
    REPO_ROOT / "data" / "environment_randomization_v2" / "goldens_v1.json"
)
NATIVE_SCHEDULE_GOLDENS_PATH = (
    REPO_ROOT / "data" / "native_trainer_schedule_v1_goldens.json"
)
DEFAULT_OUTPUT_PATH = (
    REPO_ROOT
    / "data"
    / "environment_randomization_v2"
    / "reset_physical_trajectory_goldens_v1.json"
)


class ContractError(ValueError):
    """Any strict-authority or strict-shape violation."""


def require_full_match(value: object, body, label: str) -> str:
    """Full-string validation. `fullmatch` rejects a terminal LF that an
    anchored `match` would silently admit."""
    if not isinstance(value, str):
        raise ContractError(f"{label}: expected a string, observed {type(value).__name__}")
    if body.fullmatch(value) is None:
        raise ContractError(f"{label}: {value!r} fails full-string validation")
    return value


def require_case_name(value: object, label: str) -> str:
    return require_full_match(value, CASE_NAME_BODY, label)


def require_sha256_hex(value: object, label: str) -> str:
    return require_full_match(value, SHA256_BODY, label)


def require_deck_hash_hex(value: object, label: str) -> str:
    return require_full_match(value, DECK_HASH_BODY, label)


def require_non_bool_int(value: object, label: str) -> int:
    """`bool` is an `int` subclass and `60.0 == 60`, so equality alone would
    admit both. Types are checked before any equality gate."""
    if isinstance(value, bool) or not isinstance(value, int):
        raise ContractError(
            f"{label}: expected a non-boolean integer, observed {type(value).__name__}"
        )
    return value


# --------------------------------------------------------------------------
# Raw-byte authority gates
# --------------------------------------------------------------------------


def sha256_hex(payload: bytes) -> str:
    return hashlib.sha256(payload).hexdigest()


def read_pinned_bytes(path: Path, expected_sha256: str, label: str) -> bytes:
    require_sha256_hex(expected_sha256, f"{label} pinned SHA-256")
    raw = path.read_bytes()
    observed = sha256_hex(raw)
    if observed != expected_sha256:
        raise ContractError(
            f"{label}: raw SHA-256 mismatch at {path}; "
            f"expected {expected_sha256}, observed {observed}"
        )
    return raw


# --------------------------------------------------------------------------
# Strict JSON parsing
# --------------------------------------------------------------------------


def _reject_duplicate_keys(pairs: list[tuple[str, object]]) -> dict:
    result: dict = {}
    for key, value in pairs:
        if key in result:
            raise ContractError(f"duplicate JSON object key {key!r}")
        result[key] = value
    return result


def _reject_non_finite(name: str) -> object:
    raise ContractError(f"non-finite JSON constant {name!r}")


def _reject_float(literal: str) -> object:
    """No consumed catalog field is a float. Rejecting the literal itself also
    closes numeric overflow such as `1e999-> inf`, which `parse_constant`
    never observes."""
    raise ContractError(f"JSON float literal {literal!r} is not permitted")


def strict_json_loads(raw: bytes, label: str) -> object:
    if b"\r" in raw:
        raise ContractError(f"{label}: carriage returns are not permitted")
    try:
        text = raw.decode("ascii")
    except UnicodeDecodeError as error:
        raise ContractError(f"{label}: bytes are not ASCII") from error
    return json.loads(
        text,
        object_pairs_hook=_reject_duplicate_keys,
        parse_constant=_reject_non_finite,
        parse_float=_reject_float,
    )


def require_exact_fields(value: object, allowed: tuple[str, ...], label: str) -> dict:
    if not isinstance(value, dict):
        raise ContractError(f"{label}: expected a JSON object")
    observed = set(value)
    expected = set(allowed)
    if observed != expected:
        missing = sorted(expected - observed)
        unknown = sorted(observed - expected)
        raise ContractError(
            f"{label}: exact field set violated; missing {missing}, unknown {unknown}"
        )
    return value


CATALOG_TOP_LEVEL_FIELDS = (
    "schema",
    "protocol",
    "source_hash_normalization",
    "materialization",
    "card_ids",
    "decks",
)
CATALOG_MATERIALIZATION_FIELDS = (
    "order",
    "source_row_ordinal_base",
    "copy_ordinal_base",
)
CATALOG_CARD_IDS_FIELDS = ("assignment", "deck_hash_algorithm")
CATALOG_DECK_FIELDS = (
    "canonical_pool_order",
    "id",
    "source_path",
    "source_sha256",
    "mainboard_copy_count",
    "unique_mainboard_cards",
    "runtime_deck_hash",
    "materialized_mainboard",
)
CATALOG_COPY_FIELDS = ("source_row_ordinal", "copy_ordinal", "name", "card_id")


def fnv1a64_serde_json_u16_array(card_ids: list[int]) -> int:
    """The frozen `fnv1a64-serde-json-u16-array/v1` deck hash, recomputed here
    independently of the catalog's stored value."""
    payload = json.dumps(card_ids, separators=(",", ":")).encode("ascii")
    digest = 0xCBF2_9CE4_8422_2325
    for byte in payload:
        digest ^= byte
        digest = (digest * 0x0000_0100_0000_01B3) & U64_MAX
    return digest


class RuntimeDeck:
    __slots__ = ("deck_id", "card_ids", "deck_hash")

    def __init__(self, deck_id: str, card_ids: list[int], deck_hash: int) -> None:
        self.deck_id = deck_id
        self.card_ids = card_ids
        self.deck_hash = deck_hash

    @property
    def deck_hash_hex(self) -> str:
        return f"{self.deck_hash:016x}"


class RuntimeDeckCatalog:
    __slots__ = ("schema", "protocol", "materialization_order", "deck_hash_algorithm", "decks")

    def __init__(
        self,
        schema: str,
        protocol: str,
        materialization_order: str,
        deck_hash_algorithm: str,
        decks: dict[str, RuntimeDeck],
    ) -> None:
        self.schema = schema
        self.protocol = protocol
        self.materialization_order = materialization_order
        self.deck_hash_algorithm = deck_hash_algorithm
        self.decks = decks

    def by_id(self, deck_id: str) -> RuntimeDeck:
        """Exact-ID selection. Catalog array position is never used: `Rally`
        is `canonical_pool_order` 2 yet appears first in the `decks` array, so
        positional selection would silently swap the physical seats."""
        deck = self.decks.get(deck_id)
        if deck is None:
            raise ContractError(f"runtime deck id {deck_id!r} is absent from the catalog")
        return deck


def parse_runtime_deck_catalog(raw: bytes) -> RuntimeDeckCatalog:
    document = strict_json_loads(raw, "runtime deck catalog")
    top = require_exact_fields(document, CATALOG_TOP_LEVEL_FIELDS, "runtime catalog")
    schema = top["schema"]
    protocol = top["protocol"]
    if schema != EXPECTED_CATALOG_SCHEMA:
        raise ContractError(f"runtime catalog schema {schema!r} is not frozen")
    if protocol != EXPECTED_CATALOG_PROTOCOL:
        raise ContractError(f"runtime catalog protocol {protocol!r} is not frozen")

    materialization = require_exact_fields(
        top["materialization"], CATALOG_MATERIALIZATION_FIELDS, "runtime materialization"
    )
    materialization_order = materialization["order"]
    if materialization_order != EXPECTED_MATERIALIZATION_ORDER:
        raise ContractError(
            f"runtime materialization order {materialization_order!r} is not frozen"
        )

    card_ids_block = require_exact_fields(
        top["card_ids"], CATALOG_CARD_IDS_FIELDS, "runtime card_ids"
    )
    deck_hash_algorithm = card_ids_block["deck_hash_algorithm"]
    if deck_hash_algorithm != EXPECTED_DECK_HASH_ALGORITHM:
        raise ContractError(
            f"runtime deck hash algorithm {deck_hash_algorithm!r} is not frozen"
        )

    decks_value = top["decks"]
    if not isinstance(decks_value, list) or not decks_value:
        raise ContractError("runtime catalog decks must be a non-empty array")

    decks: dict[str, RuntimeDeck] = {}
    for position, deck_value in enumerate(decks_value):
        deck = require_exact_fields(
            deck_value, CATALOG_DECK_FIELDS, f"runtime deck at position {position}"
        )
        deck_id = deck["id"]
        if not isinstance(deck_id, str) or not deck_id:
            raise ContractError(f"runtime deck at position {position} has no string id")
        if deck_id in decks:
            raise ContractError(f"duplicate runtime deck id {deck_id!r}")
        require_printable_ascii_deck_id(deck_id)

        stored_hash = deck["runtime_deck_hash"]
        if not isinstance(stored_hash, str) or not stored_hash.startswith("0x"):
            raise ContractError(f"deck {deck_id!r}: runtime_deck_hash must be 0x-prefixed")
        require_deck_hash_hex(
            stored_hash[2:], f"deck {deck_id!r} runtime_deck_hash body"
        )
        stored_hash_value = int(stored_hash, 16)

        copies = deck["materialized_mainboard"]
        if not isinstance(copies, list) or not copies:
            raise ContractError(f"deck {deck_id!r}: materialized_mainboard must be non-empty")
        if len(copies) > MAX_CARDS_PER_DECK:
            raise ContractError(f"deck {deck_id!r}: exceeds {MAX_CARDS_PER_DECK} cards")

        card_ids: list[int] = []
        for copy_position, copy_value in enumerate(copies):
            entry = require_exact_fields(
                copy_value,
                CATALOG_COPY_FIELDS,
                f"deck {deck_id!r} materialized copy {copy_position}",
            )
            card_id = entry["card_id"]
            if not isinstance(card_id, int) or isinstance(card_id, bool):
                raise ContractError(
                    f"deck {deck_id!r} copy {copy_position}: card_id must be an integer"
                )
            if not 0 <= card_id <= U16_MAX:
                raise ContractError(
                    f"deck {deck_id!r} copy {copy_position}: card_id outside u16"
                )
            card_ids.append(card_id)

        mainboard_copy_count = require_non_bool_int(
            deck["mainboard_copy_count"], f"deck {deck_id!r} mainboard_copy_count"
        )
        if mainboard_copy_count != len(card_ids):
            raise ContractError(
                f"deck {deck_id!r}: mainboard_copy_count {mainboard_copy_count} "
                f"disagrees with {len(card_ids)} materialized copies"
            )
        unique_mainboard_cards = require_non_bool_int(
            deck["unique_mainboard_cards"], f"deck {deck_id!r} unique_mainboard_cards"
        )
        if unique_mainboard_cards != len(set(card_ids)):
            raise ContractError(
                f"deck {deck_id!r}: unique_mainboard_cards {unique_mainboard_cards} "
                f"disagrees with {len(set(card_ids))} distinct card ids"
            )

        # `materialized_mainboard` order is preserved exactly and the deck hash
        # is independently recomputed over that order.
        recomputed = fnv1a64_serde_json_u16_array(card_ids)
        if recomputed != stored_hash_value:
            raise ContractError(
                f"deck {deck_id!r}: independently recomputed deck hash "
                f"{recomputed:#018x} disagrees with catalog {stored_hash}"
            )
        decks[deck_id] = RuntimeDeck(deck_id, card_ids, recomputed)

    return RuntimeDeckCatalog(
        schema=schema,
        protocol=protocol,
        materialization_order=materialization_order,
        deck_hash_algorithm=deck_hash_algorithm,
        decks=decks,
    )


def require_printable_ascii_deck_id(deck_id: object) -> str:
    if not isinstance(deck_id, str):
        raise ContractError(f"deck id must be a string, observed {type(deck_id).__name__}")
    try:
        encoded = deck_id.encode("ascii")
    except UnicodeEncodeError as error:
        raise ContractError(f"deck id {deck_id!r} is not ASCII") from error
    if not encoded or len(encoded) > 64:
        raise ContractError(f"deck id {deck_id!r} must be 1..=64 ASCII bytes")
    if any(not 0x20 <= byte <= 0x7E for byte in encoded):
        raise ContractError(f"deck id {deck_id!r} must be printable ASCII")
    return deck_id


# --------------------------------------------------------------------------
# Imported KDF/shuffle authority and independent schedule derivation
# --------------------------------------------------------------------------


def import_reference_module(path: Path):
    spec = importlib.util.spec_from_file_location(
        "environment_randomization_v2_reference_portable_reset", path
    )
    if spec is None or spec.loader is None:
        raise ContractError(f"cannot import the KDF reference at {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def trainer_atom(tag: str, payload: bytes) -> bytes:
    tag_bytes = tag.encode("utf-8")
    return (
        len(tag_bytes).to_bytes(4, "big")
        + tag_bytes
        + len(payload).to_bytes(8, "big")
        + payload
    )


def derive_train_env_root(base_seed: int, pair_index: int) -> int:
    """Independent reimplementation of exactly the frozen native `train-env`
    schedule derivation needed for this slice. The version atom is
    `kernel-python-rl-trainer-sha256-v2`; the schedule identity is an artifact
    binding and is never hashed. Substituting the schedule identity yields the
    wrong root 3926161255480587309."""
    for name, value in (("base_seed", base_seed), ("pair_index", pair_index)):
        if not isinstance(value, int) or isinstance(value, bool):
            raise ContractError(f"{name} must be an integer")
        if not 0 <= value <= U63_MAX:
            raise ContractError(f"{name} outside u63 range")
    hasher = hashlib.sha256()
    hasher.update(trainer_atom("version", TRAINER_VERSION_ATOM.encode("utf-8")))
    hasher.update(trainer_atom("namespace", TRAIN_ENV_NAMESPACE.encode("utf-8")))
    hasher.update(trainer_atom("field-name", b"base_seed"))
    hasher.update(trainer_atom("u63", base_seed.to_bytes(8, "big")))
    hasher.update(trainer_atom("field-name", b"pair_index"))
    hasher.update(trainer_atom("u63", pair_index.to_bytes(8, "big")))
    return int.from_bytes(hasher.digest()[:8], "big") & U63_MAX


def learner_seat_for_episode(episode_index: int) -> str:
    return "p0" if episode_index % 2 == 0 else "p1"


# --------------------------------------------------------------------------
# Canonical bytes and portable semantic stream
# --------------------------------------------------------------------------


def canonical_file_bytes(artifact: dict) -> bytes:
    return (
        json.dumps(
            artifact,
            sort_keys=True,
            separators=(",", ":"),
            ensure_ascii=True,
            allow_nan=False,
        ).encode("ascii")
        + b"\n"
    )


def atom(tag: str, payload: bytes) -> bytes:
    tag_bytes = tag.encode("utf-8")
    return (
        len(tag_bytes).to_bytes(4, "big")
        + tag_bytes
        + len(payload).to_bytes(8, "big")
        + payload
    )


def cj(value: object) -> bytes:
    """Canonical JSON of a nested value with exactly one final LF."""
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


def u64be(value: int) -> bytes:
    if not 0 <= value <= U64_MAX:
        raise ContractError("count outside u64 range")
    return value.to_bytes(8, "big")


def portable_semantic_stream(artifact: dict) -> bytes:
    stream = bytearray()
    stream += atom(
        "domain", artifact["portable_vector_stream_identity"].encode("ascii")
    )
    stream += atom("artifact_schema_utf8", artifact["schema"].encode("ascii"))
    stream += atom(
        "environment_randomization_identity_utf8",
        artifact["environment_randomization_identity"].encode("ascii"),
    )
    stream += atom(
        "physical_projection_identity_utf8",
        artifact["physical_projection_identity"].encode("ascii"),
    )
    stream += atom(
        "source_authorities_canonical_json", cj(artifact["source_authorities"])
    )
    stream += atom(
        "projection_contract_canonical_json", cj(artifact["projection_contract"])
    )

    stream += atom("reset_case_count_u64be", u64be(len(artifact["reset_cases"])))
    for case in artifact["reset_cases"]:
        stream += atom(
            "reset_case",
            atom("name_utf8", case["name"].encode("ascii"))
            + atom("input_canonical_json", cj(case["input"]))
            + atom(
                "expected_projection_canonical_json",
                cj(case["expected_projection"]),
            ),
        )

    stream += atom(
        "paired_role_case_count_u64be", u64be(len(artifact["paired_role_cases"]))
    )
    for case in artifact["paired_role_cases"]:
        stream += atom(
            "paired_role_case",
            atom("name_utf8", case["name"].encode("ascii"))
            + atom("input_canonical_json", cj(case["input"]))
            + atom(
                "expected_shared_reset_case_name_utf8",
                case["expected_shared_reset_case_name"].encode("ascii"),
            ),
        )

    stream += atom("reject_case_count_u64be", u64be(len(artifact["reject_cases"])))
    for case in artifact["reject_cases"]:
        stream += atom(
            "reject_case",
            atom("name_utf8", case["name"].encode("ascii"))
            + atom("input_canonical_json", cj(case["input"]))
            + atom(
                "expected_rejection_ascii",
                case["expected_rejection"].encode("ascii"),
            ),
        )
    return bytes(stream)


# --------------------------------------------------------------------------
# Projection construction
# --------------------------------------------------------------------------


def build_owner_projection(
    reference, root: int, owner: str, deck: RuntimeDeck
) -> dict:
    derived_seed = reference.derive_seed(root, owner, INITIAL_SHUFFLE_PURPOSE, 0)

    # The mandatory source-index permutation: both decks contain duplicate card
    # ids, so a card-id projection alone could hide a swap of equal copies.
    source_indices = list(range(len(deck.card_ids)))
    source_index_permutation = reference.permutation_for(derived_seed, source_indices)
    # Independent direct shuffle of the repeated card-id array.
    card_permutation = reference.permutation_for(derived_seed, list(deck.card_ids))

    projected = [deck.card_ids[index] for index in source_index_permutation]
    if sorted(source_index_permutation) != source_indices:
        raise ContractError(
            f"{owner}: source-index permutation is not a bijection of 0..{len(deck.card_ids) - 1}"
        )
    if projected != card_permutation:
        raise ContractError(
            f"{owner}: source-index projection disagrees with the direct card-id shuffle"
        )

    return {
        "physical_owner": owner,
        "derived_initial_seed": derived_seed,
        "source_index_permutation": source_index_permutation,
        "card_definition_id_permutation": card_permutation,
        "opening_hand_card_definition_ids": card_permutation[:OPENING_HAND_COUNT],
        "remaining_library_card_definition_ids": card_permutation[OPENING_HAND_COUNT:],
    }


def build_draw_events(p0_projection: dict, p1_projection: dict) -> list[dict]:
    """Fourteen alternating committed draws. Event `2r` is P0 taking its
    permutation index `r`; event `2r+1` is P1 taking its permutation index
    `r`. Library index zero is the next draw."""
    events: list[dict] = []
    library_sizes = {
        "p0": len(p0_projection["card_definition_id_permutation"]),
        "p1": len(p1_projection["card_definition_id_permutation"]),
    }
    permutations = {
        "p0": p0_projection["card_definition_id_permutation"],
        "p1": p1_projection["card_definition_id_permutation"],
    }
    for draw_round in range(OPENING_DRAW_ROUNDS):
        for owner_offset, owner in enumerate(OWNERS):
            events.append(
                {
                    "global_event_ordinal": 2 * draw_round + owner_offset,
                    "owner_draw_ordinal": draw_round,
                    "physical_owner": owner,
                    "card_definition_id": permutations[owner][draw_round],
                    "owner_hand_count_after": draw_round + 1,
                    "owner_library_count_after": library_sizes[owner]
                    - (draw_round + 1),
                }
            )
    return events


def build_reset_case(
    reference, name: str, root: int, p0_deck: RuntimeDeck, p1_deck: RuntimeDeck
) -> dict:
    p0_projection = build_owner_projection(reference, root, "p0", p0_deck)
    p1_projection = build_owner_projection(reference, root, "p1", p1_deck)
    return {
        "name": name,
        "input": {
            "pair_environment_seed": root,
            "p0": {
                "physical_owner": "p0",
                "deck_id": p0_deck.deck_id,
                "runtime_deck_hash_u64_hex": p0_deck.deck_hash_hex,
                "source_card_definition_ids": list(p0_deck.card_ids),
            },
            "p1": {
                "physical_owner": "p1",
                "deck_id": p1_deck.deck_id,
                "runtime_deck_hash_u64_hex": p1_deck.deck_hash_hex,
                "source_card_definition_ids": list(p1_deck.card_ids),
            },
        },
        "expected_projection": {
            "p0": p0_projection,
            "p1": p1_projection,
            "draw_events": build_draw_events(p0_projection, p1_projection),
            "next_live_shuffle_ordinals": [0, 0],
        },
    }


def build_paired_role_case(name: str, base_seed: int, pair_index: int, root: int) -> dict:
    even_index = pair_index * 2
    odd_index = even_index + 1
    return {
        "name": name,
        "input": {
            "trainer_schedule_identity": NATIVE_SCHEDULE_IDENTITY,
            "base_seed": base_seed,
            "pair_index": pair_index,
            "even_episode": {
                "episode_index": even_index,
                "learner_seat": learner_seat_for_episode(even_index),
                "pair_environment_seed": root,
                "p0_deck_id": BURN_DECK_ID,
                "p1_deck_id": RALLY_DECK_ID,
            },
            "odd_episode": {
                "episode_index": odd_index,
                "learner_seat": learner_seat_for_episode(odd_index),
                "pair_environment_seed": root,
                "p0_deck_id": BURN_DECK_ID,
                "p1_deck_id": RALLY_DECK_ID,
            },
        },
        "expected_shared_reset_case_name": "burn-rally-native-base-71501-pair-0",
    }


# --------------------------------------------------------------------------
# Golden validators with frozen precedence
# --------------------------------------------------------------------------


def validate_reset_body(body: dict) -> str | None:
    """Frozen precedence: source-index length, then index range, then
    bijection, then source-index-to-card projection, then hand/library/draw
    consistency."""
    reset_input = body["input"]
    projection = body["expected_projection"]
    decks = {owner: reset_input[owner]["source_card_definition_ids"] for owner in OWNERS}
    projections = {owner: projection[owner] for owner in OWNERS}

    for owner in OWNERS:
        if len(projections[owner]["source_index_permutation"]) != len(decks[owner]):
            return CODE_LENGTH
    for owner in OWNERS:
        limit = len(decks[owner])
        if any(
            not 0 <= index < limit
            for index in projections[owner]["source_index_permutation"]
        ):
            return CODE_RANGE
    for owner in OWNERS:
        permutation = projections[owner]["source_index_permutation"]
        if sorted(permutation) != list(range(len(decks[owner]))):
            return CODE_BIJECTION
    for owner in OWNERS:
        permutation = projections[owner]["source_index_permutation"]
        projected = [decks[owner][index] for index in permutation]
        if projected != projections[owner]["card_definition_id_permutation"]:
            return CODE_PROJECTION
    for owner in OWNERS:
        owner_projection = projections[owner]
        card_permutation = owner_projection["card_definition_id_permutation"]
        if owner_projection["physical_owner"] != owner:
            return CODE_TRAJECTORY
        if (
            owner_projection["opening_hand_card_definition_ids"]
            != card_permutation[:OPENING_HAND_COUNT]
        ):
            return CODE_TRAJECTORY
        if (
            owner_projection["remaining_library_card_definition_ids"]
            != card_permutation[OPENING_HAND_COUNT:]
        ):
            return CODE_TRAJECTORY
    if projection["draw_events"] != build_draw_events(
        projections["p0"], projections["p1"]
    ):
        return CODE_TRAJECTORY
    if projection["next_live_shuffle_ordinals"] != [0, 0]:
        return CODE_TRAJECTORY
    return None


def validate_paired_body(body: dict, reset_case_roots: dict[str, int]) -> str | None:
    """Frozen precedence: schedule identity, then episode/index relationships,
    then learner seats, then derived/shared roots, then the shared-reset
    reference, then fixed physical deck bindings."""
    paired_input = body["input"]
    if paired_input["trainer_schedule_identity"] != NATIVE_SCHEDULE_IDENTITY:
        return CODE_SCHEDULE_IDENTITY

    even = paired_input["even_episode"]
    odd = paired_input["odd_episode"]
    pair_index = paired_input["pair_index"]
    if even["episode_index"] % 2 != 0:
        return CODE_EPISODE
    if odd["episode_index"] != even["episode_index"] + 1:
        return CODE_EPISODE
    if even["episode_index"] // 2 != pair_index or odd["episode_index"] // 2 != pair_index:
        return CODE_EPISODE

    if even["learner_seat"] != learner_seat_for_episode(even["episode_index"]):
        return CODE_SEAT
    if odd["learner_seat"] != learner_seat_for_episode(odd["episode_index"]):
        return CODE_SEAT

    derived_root = derive_train_env_root(paired_input["base_seed"], pair_index)
    if even["pair_environment_seed"] != derived_root:
        return CODE_ROOT
    if odd["pair_environment_seed"] != derived_root:
        return CODE_ROOT

    shared_name = body["expected_shared_reset_case_name"]
    if reset_case_roots.get(shared_name) != derived_root:
        return CODE_SHARED

    for episode in (even, odd):
        if episode["p0_deck_id"] != BURN_DECK_ID:
            return CODE_DECKS
        if episode["p1_deck_id"] != RALLY_DECK_ID:
            return CODE_DECKS
    return None


def classify_reject(reject_input: dict, reset_case_roots: dict[str, int]) -> str | None:
    kind = reject_input["kind"]
    case = reject_input["case"]
    if kind == "reset-projection":
        return validate_reset_body(case)
    if kind == "paired-role":
        return validate_paired_body(case, reset_case_roots)
    raise ContractError(f"unknown reject input kind {kind!r}")


# --------------------------------------------------------------------------
# Reject construction: single-field deltas off a named positive body
# --------------------------------------------------------------------------


def reset_body_of(case: dict) -> dict:
    return {
        "input": copy.deepcopy(case["input"]),
        "expected_projection": copy.deepcopy(case["expected_projection"]),
    }


def paired_body_of(case: dict) -> dict:
    return {
        "input": copy.deepcopy(case["input"]),
        "expected_shared_reset_case_name": case["expected_shared_reset_case_name"],
    }


# Frozen single-intent reject deltas. Each entry is the complete observed
# delta tuple of the reject body against a fresh copy of its named positive
# body. Anything else, in either direction, is a ContractError.
FROZEN_REJECT_DELTAS: dict[str, tuple] = {
    "paired-role-learner-seat-not-swapped": (
        ("replace", ("input", "odd_episode", "learner_seat"), "p1", "p0"),
    ),
    "paired-role-odd-environment-seed-drift": (
        (
            "replace",
            ("input", "odd_episode", "pair_environment_seed"),
            5_293_664_275_683_392_565,
            5_293_664_275_683_392_564,
        ),
    ),
    "paired-role-odd-physical-decks-swapped": (
        ("replace", ("input", "odd_episode", "p0_deck_id"), "Burn", "Rally"),
        ("replace", ("input", "odd_episode", "p1_deck_id"), "Rally", "Burn"),
    ),
    "reset-source-permutation-duplicate-index": (
        (
            "replace",
            ("expected_projection", "p0", "source_index_permutation", 17),
            37,
            36,
        ),
    ),
    "reset-source-permutation-index-out-of-range": (
        (
            "replace",
            ("expected_projection", "p0", "source_index_permutation", 0),
            36,
            60,
        ),
    ),
    "reset-source-permutation-projection-mismatch": (
        (
            "replace",
            ("expected_projection", "p0", "card_definition_id_permutation", 0),
            47,
            37,
        ),
    ),
}


DUPLICATE_INDEX_REJECT_NAME = "reset-source-permutation-duplicate-index"


def _is_plain_dict(value: object) -> bool:
    return type(value) is dict


def _is_plain_list(value: object) -> bool:
    return type(value) is list


def _delta_sort_key(delta: tuple) -> tuple:
    # Paths mix string keys and integer indices, so sort on their text form to
    # keep ordering total and deterministic.
    return (tuple(str(part) for part in delta[1]), delta[0])


def _walk_leaf_deltas(old: object, new: object, path: tuple, out: list) -> None:
    if _is_plain_dict(old) and _is_plain_dict(new):
        for key in sorted(set(old) | set(new)):
            if key not in new:
                out.append(("remove", path + (key,), old[key], None))
            elif key not in old:
                out.append(("add", path + (key,), None, new[key]))
            else:
                _walk_leaf_deltas(old[key], new[key], path + (key,), out)
        return
    if _is_plain_list(old) and _is_plain_list(new):
        common = min(len(old), len(new))
        for index in range(common):
            _walk_leaf_deltas(old[index], new[index], path + (index,), out)
        for index in range(common, len(old)):
            out.append(("remove", path + (index,), old[index], None))
        for index in range(common, len(new)):
            out.append(("add", path + (index,), None, new[index]))
        return
    if type(old) is not type(new) or old != new:
        out.append(("replace", path, old, new))


def body_leaf_deltas(positive_body: object, reject_body: object) -> tuple:
    """Deterministic recursive leaf diff of two complete bodies."""
    deltas: list = []
    _walk_leaf_deltas(positive_body, reject_body, (), deltas)
    deltas.sort(key=_delta_sort_key)
    return tuple(deltas)


def _same_scalar(observed: object, expected: object) -> bool:
    """Type-exact scalar equality. `36.0 == 36` is True in Python, so the type
    must be compared before the value or a float substitution slips through."""
    return type(observed) is type(expected) and observed == expected


def _same_path(observed_path: object, expected_path: object) -> bool:
    if not isinstance(observed_path, tuple) or not isinstance(expected_path, tuple):
        return False
    if len(observed_path) != len(expected_path):
        return False
    return all(
        _same_scalar(observed_part, expected_part)
        for observed_part, expected_part in zip(observed_path, expected_path)
    )


def _same_delta(observed: object, expected: object) -> bool:
    if not isinstance(observed, tuple) or not isinstance(expected, tuple):
        return False
    if len(observed) != 4 or len(expected) != 4:
        return False
    observed_op, observed_path, observed_old, observed_new = observed
    expected_op, expected_path, expected_old, expected_new = expected
    if not _same_scalar(observed_op, expected_op):
        return False
    if not _same_path(observed_path, expected_path):
        return False
    return _same_scalar(observed_old, expected_old) and _same_scalar(
        observed_new, expected_new
    )


def deltas_are_type_exact(observed: object, expected: object) -> bool:
    """Explicit entry-by-entry comparison of operation, path (including
    path-component types), old leaf type/value, and new leaf type/value, plus
    identical tuple length. Never falls back to ordinary tuple equality."""
    if not isinstance(observed, tuple) or not isinstance(expected, tuple):
        return False
    if len(observed) != len(expected):
        return False
    return all(
        _same_delta(observed_entry, expected_entry)
        for observed_entry, expected_entry in zip(observed, expected)
    )


def prove_reject_delta(name: str, positive_body: object, reject_body: object) -> tuple:
    """Prove a reject body differs from its named positive body by exactly the
    frozen delta. Runs before rejection classification, so a vacuous or
    over-broad mutation fails closed instead of being classified."""
    if name not in FROZEN_REJECT_DELTAS:
        raise ContractError(f"reject {name!r} has no frozen delta")
    expected = FROZEN_REJECT_DELTAS[name]
    observed = body_leaf_deltas(positive_body, reject_body)
    if not deltas_are_type_exact(observed, expected):
        raise ContractError(
            f"reject {name!r}: delta is not the ruled single intent; "
            f"expected {expected}, observed {observed}"
        )
    if name == DUPLICATE_INDEX_REJECT_NAME:
        # The duplicate witness carries semantic intent beyond its one leaf:
        # the bijection must break while every projected card byte survives.
        prove_duplicate_index_witness(positive_body, reject_body)
    return observed


def prove_duplicate_index_witness(positive_body: dict, reject_body: dict) -> None:
    """The duplicate-index witness must break the bijection without disturbing
    one projected card byte. Source copies 36 and 37 are both card 47, so
    positions 0 and 17 collide on an identical projection."""
    positive_permutation = positive_body["expected_projection"]["p0"][
        "source_index_permutation"
    ]
    reject_permutation = reject_body["expected_projection"]["p0"][
        "source_index_permutation"
    ]
    source_cards = positive_body["input"]["p0"]["source_card_definition_ids"]

    if (positive_permutation[0], positive_permutation[17]) != (36, 37):
        raise ContractError(
            "duplicate witness: positive positions 0/17 must be 36/37, observed "
            f"{positive_permutation[0]}/{positive_permutation[17]}"
        )
    if (reject_permutation[0], reject_permutation[17]) != (36, 36):
        raise ContractError(
            "duplicate witness: rejected positions 0/17 must be 36/36, observed "
            f"{reject_permutation[0]}/{reject_permutation[17]}"
        )
    if (source_cards[36], source_cards[37]) != (47, 47):
        raise ContractError(
            "duplicate witness: source copies 36/37 must both be card 47, observed "
            f"{source_cards[36]}/{source_cards[37]}"
        )
    positive_projected = [source_cards[index] for index in positive_permutation]
    reject_projected = [source_cards[index] for index in reject_permutation]
    if positive_projected != reject_projected:
        raise ContractError(
            "duplicate witness: the projected card list must be unchanged"
        )
    if sorted(reject_permutation) == sorted(positive_permutation):
        raise ContractError(
            "duplicate witness: the rejected permutation must not stay a bijection"
        )


def build_reject_cases(
    reset_cases: list[dict],
    paired_cases: list[dict],
    reset_case_roots: dict[str, int],
) -> list[dict]:
    by_name = {case["name"]: case for case in reset_cases}
    root_940001 = by_name["burn-rally-root-940001"]
    paired_positive = paired_cases[0]

    rejects: list[dict] = []

    # paired-role-learner-seat-not-swapped: odd_episode.learner_seat p1 -> p0
    body = paired_body_of(paired_positive)
    body["input"]["odd_episode"]["learner_seat"] = "p0"
    rejects.append(
        {
            "name": "paired-role-learner-seat-not-swapped",
            "input": {"kind": "paired-role", "case": body},
            "expected_rejection": CODE_SEAT,
        }
    )

    # paired-role-odd-environment-seed-drift: shared root drifts by one
    body = paired_body_of(paired_positive)
    body["input"]["odd_episode"]["pair_environment_seed"] = (
        body["input"]["odd_episode"]["pair_environment_seed"] - 1
    )
    rejects.append(
        {
            "name": "paired-role-odd-environment-seed-drift",
            "input": {"kind": "paired-role", "case": body},
            "expected_rejection": CODE_ROOT,
        }
    )

    # paired-role-odd-physical-decks-swapped: odd Burn/Rally -> Rally/Burn
    body = paired_body_of(paired_positive)
    odd = body["input"]["odd_episode"]
    odd["p0_deck_id"], odd["p1_deck_id"] = RALLY_DECK_ID, BURN_DECK_ID
    rejects.append(
        {
            "name": "paired-role-odd-physical-decks-swapped",
            "input": {"kind": "paired-role", "case": body},
            "expected_rejection": CODE_DECKS,
        }
    )

    # reset-source-permutation-duplicate-index: position 17 collides with 0.
    # Source copies 36 and 37 are both card 47, so the bijection breaks while
    # every stored card-projection byte stays identical.
    body = reset_body_of(root_940001)
    body["expected_projection"]["p0"]["source_index_permutation"][17] = 36
    rejects.append(
        {
            "name": "reset-source-permutation-duplicate-index",
            "input": {"kind": "reset-projection", "case": body},
            "expected_rejection": CODE_BIJECTION,
        }
    )

    # reset-source-permutation-index-out-of-range: p0 position 0 -> 60
    body = reset_body_of(root_940001)
    deck_length = len(body["input"]["p0"]["source_card_definition_ids"])
    if deck_length != 60:
        raise ContractError(f"root-940001 P0 deck length must be 60, observed {deck_length}")
    body["expected_projection"]["p0"]["source_index_permutation"][0] = deck_length
    rejects.append(
        {
            "name": "reset-source-permutation-index-out-of-range",
            "input": {"kind": "reset-projection", "case": body},
            "expected_rejection": CODE_RANGE,
        }
    )

    # reset-source-permutation-projection-mismatch: p0 card 0 takes card 1
    body = reset_body_of(root_940001)
    cards = body["expected_projection"]["p0"]["card_definition_id_permutation"]
    cards[0] = cards[1]
    rejects.append(
        {
            "name": "reset-source-permutation-projection-mismatch",
            "input": {"kind": "reset-projection", "case": body},
            "expected_rejection": CODE_PROJECTION,
        }
    )

    rejects.sort(key=lambda case: case["name"])

    positive_bodies = {
        "reset-projection": lambda: reset_body_of(root_940001),
        "paired-role": lambda: paired_body_of(paired_positive),
    }
    for case in rejects:
        kind = case["input"]["kind"]
        # Exact-delta proof first: a vacuous or over-broad mutation must fail
        # closed rather than be classified.
        prove_reject_delta(
            case["name"], positive_bodies[kind](), case["input"]["case"]
        )
        observed = classify_reject(case["input"], reset_case_roots)
        if observed != case["expected_rejection"]:
            raise ContractError(
                f"reject {case['name']!r}: expected {case['expected_rejection']}, "
                f"observed {observed}"
            )
    stored_codes = {case["expected_rejection"] for case in rejects}
    if stored_codes != set(STORED_REJECTION_CODES):
        raise ContractError(
            f"stored reject vocabulary drifted: {sorted(stored_codes)}"
        )
    return rejects


def require_exact_case_counts(artifact: dict) -> None:
    for key, expected in (
        ("reset_cases", EXACT_RESET_CASE_COUNT),
        ("paired_role_cases", EXACT_PAIRED_CASE_COUNT),
        ("reject_cases", EXACT_REJECT_CASE_COUNT),
    ):
        observed = len(artifact[key])
        if observed != expected:
            raise ContractError(
                f"{key}: expected exactly {expected} cases, observed {observed}"
            )


# --------------------------------------------------------------------------
# Artifact assembly and strict self-audit
# --------------------------------------------------------------------------


def build_artifact() -> dict:
    catalog_raw = read_pinned_bytes(
        RUNTIME_DECK_CATALOG_PATH, RUNTIME_DECK_CATALOG_SHA256, "runtime deck catalog"
    )
    reference_raw = read_pinned_bytes(
        PYTHON_REFERENCE_PATH, PYTHON_REFERENCE_SHA256, "environment randomization reference"
    )
    read_pinned_bytes(KDF_GOLDENS_PATH, KDF_GOLDENS_SHA256, "KDF goldens")
    read_pinned_bytes(
        NATIVE_SCHEDULE_GOLDENS_PATH,
        NATIVE_SCHEDULE_GOLDENS_SHA256,
        "native trainer schedule goldens",
    )
    if not reference_raw:
        raise ContractError("environment randomization reference is empty")

    catalog = parse_runtime_deck_catalog(catalog_raw)
    reference = import_reference_module(PYTHON_REFERENCE_PATH)

    burn = catalog.by_id(BURN_DECK_ID)
    rally = catalog.by_id(RALLY_DECK_ID)

    native_root = derive_train_env_root(71_501, 0)
    reset_cases = [
        build_reset_case(
            reference, "burn-rally-native-base-71501-pair-0", native_root, burn, rally
        ),
        build_reset_case(reference, "burn-rally-root-940001", 940_001, burn, rally),
    ]
    reset_cases.sort(key=lambda case: case["name"])
    reset_case_roots = {
        case["name"]: case["input"]["pair_environment_seed"] for case in reset_cases
    }

    paired_cases = [
        build_paired_role_case(
            "native-base-71501-pair-0-learner-role-swap", 71_501, 0, native_root
        )
    ]
    paired_cases.sort(key=lambda case: case["name"])

    reject_cases = build_reject_cases(reset_cases, paired_cases, reset_case_roots)

    artifact = {
        "schema": SCHEMA,
        "generator_identity": GENERATOR_IDENTITY,
        "environment_randomization_identity": reference.IDENTITY,
        "physical_projection_identity": PHYSICAL_PROJECTION_IDENTITY,
        "portable_vector_stream_identity": PORTABLE_VECTOR_STREAM_IDENTITY,
        "source_authorities": {
            "runtime_deck_catalog": {
                "schema": catalog.schema,
                "protocol": catalog.protocol,
                "materialization_order": catalog.materialization_order,
                "deck_hash_algorithm": catalog.deck_hash_algorithm,
                "raw_file_sha256": RUNTIME_DECK_CATALOG_SHA256,
            },
            "environment_randomization_python_reference": {
                "raw_file_sha256": PYTHON_REFERENCE_SHA256,
            },
            "environment_randomization_kdf_goldens": {
                "schema": KDF_GOLDENS_SCHEMA,
                "raw_file_sha256": KDF_GOLDENS_SHA256,
            },
            "native_trainer_schedule": {
                "identity": NATIVE_SCHEDULE_IDENTITY,
                "python_reference_seed_version": TRAINER_VERSION_ATOM,
                "goldens_schema": NATIVE_SCHEDULE_GOLDENS_SCHEMA,
                "goldens_raw_file_sha256": NATIVE_SCHEDULE_GOLDENS_SHA256,
            },
        },
        "projection_contract": {
            "card_definition_domain": "u16-runtime-card-definition-id",
            "source_copy_index_domain": "zero-based-materialized-mainboard-index",
            "library_order": "index-zero-is-next-draw",
            "initial_shuffle_purpose": INITIAL_SHUFFLE_PURPOSE,
            "initial_shuffle_ordinal": 0,
            "opening_hand_count": OPENING_HAND_COUNT,
            "opening_draw_rounds": OPENING_DRAW_ROUNDS,
            "opening_draw_order_per_round": list(OWNERS),
            "live_ordinals_after_reset": [0, 0],
            "authority_scope": (
                "stdlib-python-kdf-permutation-runtime-card-definition-and-draw"
                "-projection-only"
            ),
        },
        "reset_cases": reset_cases,
        "paired_role_cases": paired_cases,
        "reject_cases": reject_cases,
    }
    audit_artifact(artifact, reset_case_roots)
    return artifact


def require_strictly_increasing_names(cases: list[dict], label: str) -> None:
    names = [case["name"] for case in cases]
    for name in names:
        require_case_name(name, f"{label} case name")
    if names != sorted(names):
        raise ContractError(f"{label}: case names are not strictly increasing")
    if len(set(names)) != len(names):
        raise ContractError(f"{label}: duplicate case names")


def audit_artifact(artifact: dict, reset_case_roots: dict[str, int]) -> None:
    if artifact["schema"] != SCHEMA:
        raise ContractError("artifact schema drifted")
    if artifact["generator_identity"] != GENERATOR_IDENTITY:
        raise ContractError("generator identity drifted")
    if artifact["physical_projection_identity"] != PHYSICAL_PROJECTION_IDENTITY:
        raise ContractError("physical projection identity drifted")
    if artifact["portable_vector_stream_identity"] != PORTABLE_VECTOR_STREAM_IDENTITY:
        raise ContractError("portable stream identity drifted")

    if len(artifact["reset_cases"]) > MAX_RESET_CASES:
        raise ContractError("reset case ceiling exceeded")
    if len(artifact["paired_role_cases"]) > MAX_PAIRED_CASES:
        raise ContractError("paired case ceiling exceeded")
    if len(artifact["reject_cases"]) > MAX_REJECT_CASES:
        raise ContractError("reject case ceiling exceeded")
    require_exact_case_counts(artifact)

    require_strictly_increasing_names(artifact["reset_cases"], "reset_cases")
    require_strictly_increasing_names(artifact["paired_role_cases"], "paired_role_cases")
    require_strictly_increasing_names(artifact["reject_cases"], "reject_cases")

    for case in artifact["reset_cases"]:
        body = {"input": case["input"], "expected_projection": case["expected_projection"]}
        observed = validate_reset_body(body)
        if observed is not None:
            raise ContractError(f"positive reset case {case['name']!r} rejected: {observed}")
        for owner in OWNERS:
            deck_input = case["input"][owner]
            require_printable_ascii_deck_id(deck_input["deck_id"])
            require_deck_hash_hex(
                deck_input["runtime_deck_hash_u64_hex"],
                f"{case['name']} {owner} runtime_deck_hash_u64_hex",
            )
            if len(deck_input["source_card_definition_ids"]) > MAX_CARDS_PER_DECK:
                raise ContractError(f"{case['name']}: {owner} deck exceeds the card ceiling")
        if len(case["expected_projection"]["draw_events"]) > MAX_DRAW_EVENTS:
            raise ContractError(f"{case['name']}: draw-event ceiling exceeded")
        if len(case["expected_projection"]["draw_events"]) != 2 * OPENING_DRAW_ROUNDS:
            raise ContractError(f"{case['name']}: expected fourteen draw events")

    for case in artifact["paired_role_cases"]:
        body = {
            "input": case["input"],
            "expected_shared_reset_case_name": case["expected_shared_reset_case_name"],
        }
        observed = validate_paired_body(body, reset_case_roots)
        if observed is not None:
            raise ContractError(f"positive paired case {case['name']!r} rejected: {observed}")

    for case in artifact["reject_cases"]:
        reject_input = case["input"]
        if set(reject_input) != {"kind", "case"}:
            raise ContractError(
                f"reject {case['name']!r}: input must have exactly kind and case"
            )
        observed = classify_reject(reject_input, reset_case_roots)
        if observed != case["expected_rejection"]:
            raise ContractError(
                f"reject {case['name']!r}: expected {case['expected_rejection']}, "
                f"observed {observed}"
            )

    body_bytes = canonical_file_bytes(artifact)
    if len(body_bytes) > MAX_ARTIFACT_BYTES:
        raise ContractError("artifact exceeds the 1 MiB ceiling")
    if b"\r" in body_bytes:
        raise ContractError("canonical bytes must not contain carriage returns")
    if body_bytes[:1] == b"\xef":
        raise ContractError("canonical bytes must not carry a BOM")
    if not body_bytes.endswith(b"\n") or body_bytes.count(b"\n") != 1:
        raise ContractError("canonical bytes must end with exactly one LF")


def main() -> int:
    parser = argparse.ArgumentParser(
        description=(
            "Generate or verify the portable environment-randomization-v2 reset "
            "physical-trajectory goldens."
        )
    )
    parser.add_argument("--out", type=Path, default=DEFAULT_OUTPUT_PATH)
    parser.add_argument(
        "--check",
        action="store_true",
        help="verify the existing artifact byte-for-byte instead of writing it",
    )
    arguments = parser.parse_args()

    artifact = build_artifact()
    body_bytes = canonical_file_bytes(artifact)
    stream_bytes = portable_semantic_stream(artifact)
    file_sha256 = sha256_hex(body_bytes)
    stream_sha256 = sha256_hex(stream_bytes)

    if arguments.check:
        if not arguments.out.exists():
            print(f"CHECK FAIL: {arguments.out} does not exist")
            return 1
        existing = arguments.out.read_bytes()
        if existing != body_bytes:
            print(
                f"CHECK FAIL: {arguments.out} differs from independent recomputation "
                f"(on disk {sha256_hex(existing)}, recomputed {file_sha256})"
            )
            return 1
        print("CHECK OK")
    else:
        arguments.out.parent.mkdir(parents=True, exist_ok=True)
        arguments.out.write_bytes(body_bytes)
        print(f"wrote {arguments.out}")

    print(f"reset_cases         {len(artifact['reset_cases'])}")
    print(f"paired_role_cases   {len(artifact['paired_role_cases'])}")
    print(f"reject_cases        {len(artifact['reject_cases'])}")
    print(f"file_bytes          {len(body_bytes)}")
    print(f"stream_bytes        {len(stream_bytes)}")
    print(f"raw_file_sha256     {file_sha256}")
    print(f"stream_sha256       {stream_sha256}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
