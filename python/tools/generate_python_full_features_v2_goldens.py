#!/usr/bin/env python3
"""Generate Python-authoritative full-decision tensor goldens for native V2.

Update mode asks a test-only Rust emitter for production session fixtures. Check
mode reuses the checked-in fixtures and therefore requires no Rust build. Both
modes recompute every tensor through ``features.encode_decision`` and pin the
canonical observation bytes used by the state hash.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import sys
from pathlib import Path
from typing import Any


REPO_ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO_ROOT / "python"))

from mtg_kernel_rl import features as f  # noqa: E402


OUTPUT = REPO_ROOT / "data" / "flat_policy_v2" / "python_full_features_v2.json"
RUST_FIXTURE_MARKER = "NATIVE_FLAT_FULL_V2_FIXTURE="
TENSOR_FIELDS = (
    "state",
    "object_features",
    "object_card_ids",
    "object_groups",
    "object_node_ids",
    "edge_features",
    "edge_source_indices",
    "edge_target_indices",
    "action_features",
    "action_ref_features",
    "action_ref_card_ids",
    "action_ref_action_indices",
    "action_ref_node_indices",
)
FLOAT_FIELDS = {
    "state",
    "object_features",
    "edge_features",
    "action_features",
    "action_ref_features",
}


def canonical_payload_bytes(value: Any) -> bytes:
    return json.dumps(
        value, sort_keys=True, separators=(",", ":"), ensure_ascii=False
    ).encode("utf-8")


def collect_rust_fixtures() -> list[dict[str, Any]]:
    completed = subprocess.run(
        [
            "cargo",
            "test",
            "-p",
            "mtg-kernel",
            "emit_native_full_v2_fixtures",
            "--no-default-features",
            "--",
            "--ignored",
            "--nocapture",
        ],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        check=False,
    )
    if completed.returncode != 0:
        raise RuntimeError(
            "Rust fixture emitter failed:\n"
            f"stdout={completed.stdout}\nstderr={completed.stderr}"
        )
    fixtures = []
    for line in completed.stdout.splitlines():
        marker = line.find(RUST_FIXTURE_MARKER)
        if marker >= 0:
            fixtures.append(json.loads(line[marker + len(RUST_FIXTURE_MARKER) :]))
    if not fixtures:
        raise RuntimeError("Rust fixture emitter produced no marked fixture")
    return fixtures


def canonical_observation(observation: dict[str, Any]) -> tuple[str, bytes]:
    actor = observation["acting_player"]
    canonical = f._canonical_model_value(
        observation,
        f.OBSERVATION_SPEC,
        ("observation",),
        f._CanonicalContext(actor, observation),
    )
    rendered = json.dumps(
        canonical, sort_keys=True, separators=(",", ":"), ensure_ascii=False
    )
    return rendered, rendered.encode("utf-8")


def tensor_payload(tensor: Any, *, floating: bool) -> dict[str, Any]:
    contiguous = tensor.detach().cpu().contiguous()
    payload: dict[str, Any] = {"shape": list(contiguous.shape)}
    if floating:
        payload["f32_le_hex"] = contiguous.numpy().tobytes().hex()
    else:
        payload["i64_values"] = contiguous.reshape(-1).tolist()
    return payload


def state_digest_blocks(canonical: bytes) -> list[bytes]:
    return [
        hashlib.sha512(
            b"observation-state" + counter.to_bytes(4, "little") + canonical
        ).digest()
        for counter in range(6)
    ]


def case_payload(fixture: dict[str, Any]) -> dict[str, Any]:
    if fixture.get("schema") != "native-flat-full-v2-rust-fixture-v1":
        raise AssertionError(f"unexpected Rust fixture schema: {fixture.get('schema')!r}")
    observation = fixture["observation"]
    legal_actions = fixture["legal_actions"]
    encoded = f.encode_decision(observation, legal_actions)
    canonical_json, canonical_bytes = canonical_observation(observation)
    if canonical_json != fixture["canonical_observation_json"]:
        raise AssertionError(
            f"{fixture['name']}: Rust/Python canonical observation mismatch\n"
            f"rust={fixture['canonical_observation_json']}\npython={canonical_json}"
        )
    blocks = state_digest_blocks(canonical_bytes)
    state_bits = encoded.state[-f.STATE_HASH_DIM :].numpy().tobytes()
    independent = []
    for block in blocks:
        for offset in range(0, len(block), 4):
            word = int.from_bytes(block[offset : offset + 4], "little")
            independent.append((float(word) / float(0xFFFF_FFFF)) * 2.0 - 1.0)
    import struct

    independent_bits = b"".join(struct.pack("<f", value) for value in independent)
    if state_bits != independent_bits:
        raise AssertionError(f"{fixture['name']}: independent state digest mismatch")
    return {
        "name": fixture["name"],
        "coverage": [
            "production-v2-view",
            "full-13-tensor-parity",
            fixture["fixture_transform"],
        ],
        "rust_fixture": fixture,
        "canonical_observation_json": canonical_json,
        "state_sha512_blocks_hex": [block.hex() for block in blocks],
        "tensors": {
            field: tensor_payload(
                getattr(encoded, field), floating=field in FLOAT_FIELDS
            )
            for field in TENSOR_FIELDS
        },
    }


def build_payload(fixtures: list[dict[str, Any]]) -> dict[str, Any]:
    features_path = REPO_ROOT / "python" / "mtg_kernel_rl" / "features.py"
    cases = [case_payload(fixture) for fixture in fixtures]
    by_name = {case["name"]: case for case in cases}
    opening = by_name["burn-mirror-opening"]
    actor_swap = by_name["synthetic-actor-seat-swap"]
    if opening["canonical_observation_json"] != actor_swap["canonical_observation_json"]:
        raise AssertionError("actor-seat mirror changed canonical observation")
    if opening["tensors"] != actor_swap["tensors"]:
        raise AssertionError("actor-seat mirror changed an actor-relative tensor")

    synthetic_relations = by_name["synthetic-known-cards-object-relations-v1"]
    synthetic_observation = synthetic_relations["rust_fixture"]["observation"]
    if not any(synthetic_observation["known_hand_cards"]):
        raise AssertionError("synthetic fixture did not cover a non-empty known hand")
    if not any(synthetic_observation["known_library_cards"]):
        raise AssertionError("synthetic fixture did not cover a non-empty known library")
    relation_kinds = {
        relation["relation_kind"]
        for relation in synthetic_observation["projection"]["object_relations"]
    }
    if relation_kinds != {"attached_to", "exiled_by"}:
        raise AssertionError(
            "synthetic fixture must cover exactly AttachedTo and ExiledBy relations"
        )

    absent = by_name["burn-mirror-combat"]
    present_empty = by_name["burn-mirror-combat-present-empty"]
    if absent["canonical_observation_json"] == present_empty["canonical_observation_json"]:
        raise AssertionError("blocked-order red pair collided in canonical observation")
    if any(
        left == right
        for left, right in zip(
            absent["state_sha512_blocks_hex"],
            present_empty["state_sha512_blocks_hex"],
        )
    ):
        raise AssertionError("blocked-order red pair collided in a SHA-512 block")
    explicit_state_hex_len = (f.STATE_FEATURE_DIM - f.STATE_HASH_DIM) * 4 * 2
    absent_state = absent["tensors"]["state"]["f32_le_hex"]
    present_state = present_empty["tensors"]["state"]["f32_le_hex"]
    if absent_state[:explicit_state_hex_len] != present_state[:explicit_state_hex_len]:
        raise AssertionError("blocked-order red pair changed the explicit state prefix")
    if absent_state[explicit_state_hex_len:] == present_state[explicit_state_hex_len:]:
        raise AssertionError("blocked-order red pair collided in the state hash tail")

    payload: dict[str, Any] = {
        "schema": "mtg-kernel-python-full-features-golden/v2",
        "authority": "python/mtg_kernel_rl/features.py",
        "authority_sha256": hashlib.sha256(features_path.read_bytes()).hexdigest(),
        "python_contracts": {
            "feature_schema_version": f.FEATURE_SCHEMA_VERSION,
            "feature_registry_version": f.FEATURE_REGISTRY_VERSION,
            "encoding_contract_version": f.ENCODING_CONTRACT_VERSION,
            "model_contract_version": f.MODEL_CONTRACT_VERSION,
        },
        "dimensions": {
            "state": f.STATE_FEATURE_DIM,
            "object": f.OBJECT_FEATURE_DIM,
            "edge": f.EDGE_FEATURE_DIM,
            "action": f.ACTION_FEATURE_DIM,
            "action_ref": f.ACTION_REF_FEATURE_DIM,
            "object_groups": len(f.OBJECT_GROUPS),
        },
        "state_hash_contract": {
            "namespace_ascii": "observation-state",
            "counter_encoding": "u32_le",
            "block_count": 6,
            "block_hash": "sha512",
            "chunk_encoding": "u32_le",
            "chunk_to_float": "f64(chunk)/f64(0xffffffff)*2.0-1.0 then one f32 cast",
        },
        "cases": cases,
    }
    payload["payload_sha256"] = hashlib.sha256(canonical_payload_bytes(payload)).hexdigest()
    return payload


def rendered_payload(fixtures: list[dict[str, Any]]) -> bytes:
    return (
        json.dumps(build_payload(fixtures), indent=2, sort_keys=True, ensure_ascii=False)
        + "\n"
    ).encode("utf-8")


def checked_in_fixtures(path: Path) -> list[dict[str, Any]]:
    document = json.loads(path.read_text(encoding="utf-8"))
    return [case["rust_fixture"] for case in document["cases"]]


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--output", type=Path, default=OUTPUT)
    args = parser.parse_args()
    if args.check:
        if not args.output.exists():
            print(f"missing Python full-feature golden: {args.output}", file=sys.stderr)
            return 1
        fixtures = checked_in_fixtures(args.output)
        rendered = rendered_payload(fixtures)
        if args.output.read_bytes() != rendered:
            print(f"stale Python full-feature golden: {args.output}", file=sys.stderr)
            return 1
        return 0
    fixtures = collect_rust_fixtures()
    rendered = rendered_payload(fixtures)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_bytes(rendered)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
