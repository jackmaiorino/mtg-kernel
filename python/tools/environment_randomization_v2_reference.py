"""Independent stdlib-only reference for mtg-kernel-environment-randomization-sha256-v2.

Implements the frozen contract (collab, codex 2026-07-29 15:34): per-substream
SHA-256 derivation with length-framed atoms, full-width big-endian 8-byte seed
prefix with no u63 mask, SplitMix64, and the descending Fisher-Yates modulo
permutation. This module is the Python side of the mandatory cross-language
golden pair; it shares no code with the Rust implementation.

Usage:
  python environment_randomization_v2_reference.py generate <out.json>
  python environment_randomization_v2_reference.py verify <goldens.json>
"""

from __future__ import annotations

import hashlib
import json
import sys

IDENTITY = "mtg-kernel-environment-randomization-sha256-v2"
NAMESPACE = "environment-randomization-substream"
OWNERS = ("p0", "p1")
PURPOSES = ("initial-library-shuffle", "in-game-library-shuffle")
U64_MAX = 0xFFFF_FFFF_FFFF_FFFF
MASK = U64_MAX


class ContractError(ValueError):
    pass


def _atom(tag: str, payload: bytes) -> bytes:
    tag_bytes = tag.encode("utf-8")
    return (
        len(tag_bytes).to_bytes(4, "big")
        + tag_bytes
        + len(payload).to_bytes(8, "big")
        + payload
    )


def derive_seed(
    pair_environment_seed: int, physical_owner: str, purpose: str, ordinal: int
) -> int:
    if not (0 <= pair_environment_seed <= U64_MAX):
        raise ContractError("pair_environment_seed out of u64 range")
    if physical_owner not in OWNERS:
        raise ContractError(f"invalid physical_owner {physical_owner!r}")
    if purpose not in PURPOSES:
        raise ContractError(f"invalid purpose {purpose!r}")
    if not (0 <= ordinal < U64_MAX):
        # u64::MAX is rejected before derivation: the committed ordinal could
        # not advance afterward. Roots may equal u64::MAX; ordinals may not.
        raise ContractError("ordinal overflow: committed ordinal cannot advance")
    if purpose == "initial-library-shuffle" and ordinal != 0:
        raise ContractError("initial-library-shuffle requires ordinal zero")
    hasher = hashlib.sha256()
    hasher.update(_atom("version", IDENTITY.encode("utf-8")))
    hasher.update(_atom("namespace", NAMESPACE.encode("utf-8")))
    hasher.update(_atom("field-name", b"pair_environment_seed"))
    hasher.update(_atom("u64", pair_environment_seed.to_bytes(8, "big")))
    hasher.update(_atom("field-name", b"physical_owner"))
    hasher.update(_atom("str", physical_owner.encode("utf-8")))
    hasher.update(_atom("field-name", b"purpose"))
    hasher.update(_atom("str", purpose.encode("utf-8")))
    hasher.update(_atom("field-name", b"ordinal"))
    hasher.update(_atom("u64", ordinal.to_bytes(8, "big")))
    return int.from_bytes(hasher.digest()[:8], "big")


class SplitMix64:
    def __init__(self, seed: int) -> None:
        self.state = seed & MASK

    def next_u64(self) -> int:
        self.state = (self.state + 0x9E3779B97F4A7C15) & MASK
        z = self.state
        z = ((z ^ (z >> 30)) * 0xBF58476D1CE4E5B9) & MASK
        z = ((z ^ (z >> 27)) * 0x94D049BB133111EB) & MASK
        return (z ^ (z >> 31)) & MASK


def shuffled(ids: list[int], rng: SplitMix64) -> list[int]:
    v = list(ids)
    for i in range(len(v) - 1, 0, -1):
        j = rng.next_u64() % (i + 1)
        v[i], v[j] = v[j], v[i]
    return v


def permutation_for(seed: int, ids: list[int]) -> list[int]:
    return shuffled(ids, SplitMix64(seed))


SEED_CASES = [0, 1, 940001, 0x0123_4567_89AB_CDEF, U64_MAX]
ORDINALS = [0, 1, 2]
PERMUTATION_IDS = {
    "len0": [],
    "len1": [7],
    "len2": [11, 22],
    "normal5": [3, 1, 4, 15, 9],
    "duplicates": [9, 9, 9, 2, 2],
}
REJECT_CASES = [
    {"name": "invalid-owner", "input": {"pair_environment_seed": 1, "physical_owner": "P0", "purpose": "in-game-library-shuffle", "ordinal": 0}, "expected_code": "invalid-owner"},
    {"name": "invalid-purpose", "input": {"pair_environment_seed": 1, "physical_owner": "p0", "purpose": "live-shuffle", "ordinal": 0}, "expected_code": "invalid-purpose"},
    {"name": "nonzero-initial-ordinal", "input": {"pair_environment_seed": 1, "physical_owner": "p1", "purpose": "initial-library-shuffle", "ordinal": 1}, "expected_code": "nonzero-initial-ordinal"},
    {"name": "ordinal-overflow-u64max", "input": {"pair_environment_seed": 1, "physical_owner": "p0", "purpose": "in-game-library-shuffle", "ordinal": U64_MAX}, "expected_code": "ordinal-overflow"},
]
CONTRACT_DESCRIPTOR = {
    "atom": "u32be(tag_byte_len) || utf8(tag) || u64be(payload_byte_len) || payload",
    "ordered_atoms": [
        ["version", IDENTITY],
        ["namespace", NAMESPACE],
        ["field-name", "pair_environment_seed", "u64", "8-byte-big-endian"],
        ["field-name", "physical_owner", "str", "utf-8"],
        ["field-name", "purpose", "str", "utf-8"],
        ["field-name", "ordinal", "u64", "8-byte-big-endian"],
    ],
    "extraction": "first-8-sha256-digest-bytes-big-endian-no-mask",
    "shuffle_algorithm": "splitmix64-descending-fisher-yates-modulo",
    "owners": list(OWNERS),
    "purposes": list(PURPOSES),
    "initial_ordinal_rule": "initial-library-shuffle-requires-ordinal-zero",
    "overflow_rule": "ordinal-u64-max-rejected-before-derivation-roots-may-equal-u64-max",
}


def classify_rejection(case_input: dict) -> str:
    try:
        derive_seed(
            case_input["pair_environment_seed"],
            case_input["physical_owner"],
            case_input["purpose"],
            case_input["ordinal"],
        )
    except ContractError as error:
        message = str(error)
        if "physical_owner" in message:
            return "invalid-owner"
        if "purpose" in message and "invalid" in message:
            return "invalid-purpose"
        if "ordinal zero" in message:
            return "nonzero-initial-ordinal"
        if "overflow" in message:
            return "ordinal-overflow"
        raise AssertionError(f"unclassified rejection: {message}")
    raise AssertionError("reject case unexpectedly derived")


def build_goldens() -> dict:
    derived = []
    for root in SEED_CASES:
        for owner in OWNERS:
            for purpose in PURPOSES:
                ordinals = [0] if purpose == "initial-library-shuffle" else ORDINALS
                for ordinal in ordinals:
                    seed = derive_seed(root, owner, purpose, ordinal)
                    derived.append(
                        {
                            "pair_environment_seed": root,
                            "physical_owner": owner,
                            "purpose": purpose,
                            "ordinal": ordinal,
                            "derived_seed": seed,
                        }
                    )
    permutations = []
    for case_name, ids in PERMUTATION_IDS.items():
        for row in derived:
            permutations.append(
                {
                    "case": case_name,
                    "derived_seed": row["derived_seed"],
                    "input_ids": ids,
                    "permutation": permutation_for(row["derived_seed"], ids),
                }
            )
    rejects = []
    for case in REJECT_CASES:
        observed = classify_rejection(case["input"])
        if observed != case["expected_code"]:
            raise AssertionError(
                f"reject case {case['name']}: expected {case['expected_code']}, observed {observed}"
            )
        rejects.append(
            {
                "name": case["name"],
                "input": case["input"],
                "expected_code": case["expected_code"],
            }
        )
    return {
        "schema": "mtg-kernel-environment-randomization-v2-goldens/v1",
        "identity": IDENTITY,
        "namespace": NAMESPACE,
        "contract": CONTRACT_DESCRIPTOR,
        "derived_seeds": derived,
        "permutations": permutations,
        "reject_cases": rejects,
    }


def main() -> int:
    command, path = sys.argv[1], sys.argv[2]
    goldens = build_goldens()
    body_bytes = (json.dumps(goldens, sort_keys=True, indent=1) + "\n").encode("ascii")
    if b"\r" in body_bytes:
        raise AssertionError("golden bytes must not contain carriage returns")
    if command == "generate":
        with open(path, "wb") as f:
            f.write(body_bytes)
        print(f"wrote {path}: {len(goldens['derived_seeds'])} seeds, "
              f"{len(goldens['permutations'])} permutations, "
              f"{len(goldens['reject_cases'])} rejects")
        return 0
    if command == "verify":
        with open(path, "rb") as f:
            existing = f.read()
        if b"\r" in existing:
            print("VERIFY FAIL: artifact contains carriage returns")
            return 1
        if existing != body_bytes:
            print("VERIFY FAIL: goldens differ from independent recomputation")
            return 1
        print("VERIFY OK")
        return 0
    print(__doc__)
    return 2


if __name__ == "__main__":
    sys.exit(main())
