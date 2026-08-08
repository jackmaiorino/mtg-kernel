#!/usr/bin/env python3
"""Generate cross-language vectors for the native trainer opponent sampler.

This generator is deliberately stdlib-only and independent.  It implements the
frozen SHA-256 trainer seed framing and unsigned modulo selection directly from
their declared contracts; it does not import the Python trainer or invoke Rust.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path
from typing import Any


SCHEMA = "mtg-kernel-native-opponent-sampler-cross-language-vectors/v1"
VECTOR_SCHEMA_VERSION = 1
SAMPLER_IDENTITY = "mtg-kernel-uniform-index-modulo-u64-v1"
SAMPLER_ALGORITHM = "selected-index-equals-action-seed-mod-legal-count"
LEGAL_COUNT_ZERO_RULE = "legal-count-zero-rejects-before-modulo"
WIDTH_ONE_SEED_RULE = (
    "derive-and-record-one-leaf-seed-for-every-substep-including-legal-count-one;"
    "then-selected-index-is-zero-and-the-next-substep-index-advances"
)
WIDTH_ONE_WITNESS_RULE = (
    "for-each-chain-emit-a-witness-for-every-non-final-substep-with-legal-count-one;"
    "pair-it-with-the-immediate-successor;exclude-a-final-width-one-substep-because-"
    "no-successor-exists;counterfactual_nonconsuming_next_substep_index_u32-equals-"
    "width_one_substep_index_u32-and-counterfactual_nonconsuming_next_action_seed_u63-"
    "equals-width_one_action_seed_u63-that-would-be-reused-if-the-substep-index-did-"
    "not-advance"
)
MODULO_BIAS_RULE = (
    "intentional-modulo-bias-no-rejection-sampling;when-legal-count-does-not-divide-"
    "the-action-seed-domain-low-residues-have-one-extra-preimage;changing-this-rule-"
    "requires-a-new-sampler-identity"
)
GENERATOR_IDENTITY = "stdlib-only-independent-sha256-uniform-modulo-reference-v1"

TRAINER_SCHEDULE_VERSION = "mtg-kernel-native-trainer-schedule-sha256-v1"
PYTHON_REFERENCE_SEED_VERSION = "kernel-python-rl-trainer-sha256-v2"
SCHEDULE_GOLDENS_SHA256 = (
    "6b2e1edbbe49b4e02f98794f9057f5c2bb8e3079d2ba8cb3e2a4b9ea6c34867c"
)
SEED_ATOM_FRAMING_IDENTITY = "u32be-tag-length-u64be-payload-length-atom-v1"
SEED_ATOM_FORMULA = "u32be(tag_utf8_byte_length)||tag_utf8||u64be(payload_byte_length)||payload"
SEED_TEXT_PAYLOAD_ENCODING = "UTF-8 for version, namespace, and field-name payloads"
SEED_U63_PAYLOAD_ENCODING = "exactly-8-byte-big-endian"
SEED_DERIVATION_ALGORITHM = (
    "sha256(ATOM(version,python-reference-seed-version)||ATOM(namespace,namespace)||"
    "ordered-ATOM(field-name,name)||ATOM(u63,u64be(value)))[:8]be&0x7fff_ffff_ffff_ffff"
)
GROUP_NAMESPACE = "train-opponent-action-group"
GROUP_FIELDS = (
    "base_seed",
    "episode_index",
    "opponent_physical_decision_index",
)
SUBSTEP_NAMESPACE = "train-opponent-action-substep"
SUBSTEP_FIELDS = ("group_seed", "substep_index")

SEMANTIC_STREAM_FRAMING_IDENTITY = "ordered-atom-stream-u32be-tag-u64be-payload-v1"
SEMANTIC_STREAM_ATOM_FORMULA = SEED_ATOM_FORMULA
SEMANTIC_STREAM_ORDER = (
    "ordered ATOMs: schema,vector-schema-version,sampler-identity,sampler-algorithm,"
    "legal-count-zero-rule,width-one-seed-rule,modulo-bias-rule,trainer-schedule-version,"
    "python-reference-seed-version,schedule-goldens-sha256,seed-atom-framing-identity,"
    "seed-text-payload-encoding,seed-u63-payload-encoding,seed-derivation-algorithm,"
    "width-one-witness-rule,group-namespace,group-field-count,each group-field,"
    "substep-namespace,substep-field-count,each substep-field,point-count,then each point "
    "point-name/action-seed-u64/legal-count-u32/selected-index-u32,rejection-count,then "
    "each rejection rejection-name/action-seed-u64/legal-count-u32/error-code,chain-count,"
    "then each chain chain-name/base-seed-u63/episode-index-u63/"
    "opponent-physical-decision-index-u63/opponent-group-seed-u63/substep-count,then each "
    "substep substep-index-u32/legal-count-u32/action-seed-u63/selected-index-u32,then "
    "width-one-witness-count and each witness width-one-substep-index-u32/"
    "width-one-action-seed-u63/next-substep-index-u32/next-action-seed-u63/"
    "counterfactual-nonconsuming-next-substep-index-u32/"
    "counterfactual-nonconsuming-next-action-seed-u63"
)

OUTPUT_RELATIVE = Path("data/native_opponent_sampler_vectors_v1.json")
GENERATOR_RELATIVE = Path(
    "python/tools/generate_native_opponent_sampler_vectors_v1.py"
)

U32_MAX = (1 << 32) - 1
U63_MAX = (1 << 63) - 1
U64_MAX = (1 << 64) - 1
LARGE_U32_PRIME = 4_294_967_291


POINT_INPUTS: tuple[tuple[str, int, int], ...] = (
    ("width-one-seed-zero", 0, 1),
    ("width-one-seed-u63-max", U63_MAX, 1),
    ("width-one-seed-u64-max", U64_MAX, 1),
    ("count-two-adjacent-zero", 0, 2),
    ("count-two-adjacent-one", 1, 2),
    ("count-two-adjacent-two", 2, 2),
    ("count-two-adjacent-three", 3, 2),
    ("count-three-first-wrap-minus-one", 2, 3),
    ("count-three-first-wrap", 3, 3),
    ("count-three-k5-minus-one", 14, 3),
    ("count-three-k5", 15, 3),
    ("count-three-u63-max", U63_MAX, 3),
    ("count-three-u64-max", U64_MAX, 3),
    ("count-five-k17-minus-one", 84, 5),
    ("count-five-k17", 85, 5),
    ("count-seven-u64-max", U64_MAX, 7),
    ("count-64-first-wrap-minus-one", 63, 64),
    ("count-64-first-wrap", 64, 64),
    ("count-64-u64-max", U64_MAX, 64),
    ("large-prime-first-wrap-minus-one", LARGE_U32_PRIME - 1, LARGE_U32_PRIME),
    ("large-prime-first-wrap", LARGE_U32_PRIME, LARGE_U32_PRIME),
    ("large-prime-second-wrap-minus-one", 2 * LARGE_U32_PRIME - 1, LARGE_U32_PRIME),
    ("large-prime-second-wrap", 2 * LARGE_U32_PRIME, LARGE_U32_PRIME),
    ("large-prime-u63-max", U63_MAX, LARGE_U32_PRIME),
    ("large-prime-u64-max-minus-one", U64_MAX - 1, LARGE_U32_PRIME),
    ("large-prime-u64-max", U64_MAX, LARGE_U32_PRIME),
    ("u32-max-first-wrap-minus-one", U32_MAX - 1, U32_MAX),
    ("u32-max-first-wrap", U32_MAX, U32_MAX),
    ("u32-max-second-wrap-minus-one", 2 * U32_MAX - 1, U32_MAX),
    ("u32-max-second-wrap", 2 * U32_MAX, U32_MAX),
    ("u32-max-u63-max", U63_MAX, U32_MAX),
    ("u32-max-u64-max-minus-one", U64_MAX - 1, U32_MAX),
    ("u32-max-u64-max", U64_MAX, U32_MAX),
)

REJECTION_INPUTS: tuple[tuple[str, int, int], ...] = (
    ("zero-count-seed-zero", 0, 0),
    ("zero-count-seed-u64-max", U64_MAX, 0),
)

CHAIN_INPUTS: tuple[tuple[str, int, int, int, tuple[int, ...]], ...] = (
    (
        "width-one-first-then-nonpowers",
        0,
        0,
        0,
        (1, 3, 64, LARGE_U32_PRIME),
    ),
    (
        "width-one-middle-shifts-following-seeds",
        71_501,
        2,
        3,
        (3, 1, 5, U32_MAX),
    ),
    (
        "u63-boundary-width-one-before-tail",
        U63_MAX,
        U63_MAX,
        U63_MAX,
        (LARGE_U32_PRIME, 64, 1, 3, 1),
    ),
)


def sha256_hex(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def canonical_json_bytes(value: Any) -> bytes:
    return (
        json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=True).encode(
            "ascii"
        )
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


def derive_seed(namespace: str, fields: tuple[tuple[str, int], ...]) -> int:
    framed = bytearray()
    framed += atom("version", PYTHON_REFERENCE_SEED_VERSION.encode("utf-8"))
    framed += atom("namespace", namespace.encode("utf-8"))
    for name, value in fields:
        if not 0 <= value <= U63_MAX:
            raise ValueError(f"{name} is outside the frozen u63 domain")
        framed += atom("field-name", name.encode("utf-8"))
        framed += atom("u63", value.to_bytes(8, "big"))
    return int.from_bytes(hashlib.sha256(framed).digest()[:8], "big") & U63_MAX


def derive_group_seed(base_seed: int, episode_index: int, decision_index: int) -> int:
    return derive_seed(
        GROUP_NAMESPACE,
        (
            ("base_seed", base_seed),
            ("episode_index", episode_index),
            ("opponent_physical_decision_index", decision_index),
        ),
    )


def derive_action_seed(group_seed: int, substep_index: int) -> int:
    if not 0 <= substep_index <= U32_MAX:
        raise ValueError("substep_index is outside the frozen u32 domain")
    return derive_seed(
        SUBSTEP_NAMESPACE,
        (("group_seed", group_seed), ("substep_index", substep_index)),
    )


def select_index(action_seed: int, legal_count: int) -> int:
    if not 0 <= action_seed <= U64_MAX:
        raise ValueError("action_seed is outside u64")
    if not 0 < legal_count <= U32_MAX:
        raise ValueError("legal_count must be in 1..=u32::MAX")
    return action_seed % legal_count


def build_points() -> list[dict[str, Any]]:
    return [
        {
            "name": name,
            "action_seed_u64": str(action_seed),
            "legal_count_u32": legal_count,
            "selected_index_u32": select_index(action_seed, legal_count),
        }
        for name, action_seed, legal_count in POINT_INPUTS
    ]


def build_rejections() -> list[dict[str, Any]]:
    result = []
    for name, action_seed, legal_count in REJECTION_INPUTS:
        try:
            select_index(action_seed, legal_count)
        except ValueError:
            pass
        else:  # pragma: no cover - protects the generator contract
            raise AssertionError(f"negative vector {name} was unexpectedly admitted")
        result.append(
            {
                "name": name,
                "action_seed_u64": str(action_seed),
                "legal_count_u32": legal_count,
                "expected_error": {"code": "empty-legal-action-set"},
            }
        )
    return result


def build_chains() -> list[dict[str, Any]]:
    chains: list[dict[str, Any]] = []
    for name, base_seed, episode_index, decision_index, widths in CHAIN_INPUTS:
        group_seed = derive_group_seed(base_seed, episode_index, decision_index)
        substeps = []
        for substep_index, legal_count in enumerate(widths):
            action_seed = derive_action_seed(group_seed, substep_index)
            substeps.append(
                {
                    "substep_index_u32": substep_index,
                    "legal_count_u32": legal_count,
                    "action_seed_u63": str(action_seed),
                    "selected_index_u32": select_index(action_seed, legal_count),
                }
            )
        witnesses = []
        for position, substep in enumerate(substeps[:-1]):
            if substep["legal_count_u32"] != 1:
                continue
            following = substeps[position + 1]
            witnesses.append(
                {
                    "width_one_substep_index_u32": substep["substep_index_u32"],
                    "width_one_action_seed_u63": substep["action_seed_u63"],
                    "next_substep_index_u32": following["substep_index_u32"],
                    "next_action_seed_u63": following["action_seed_u63"],
                    "counterfactual_nonconsuming_next_substep_index_u32": substep[
                        "substep_index_u32"
                    ],
                    "counterfactual_nonconsuming_next_action_seed_u63": substep[
                        "action_seed_u63"
                    ],
                }
            )
        if not witnesses:
            raise AssertionError(f"chain {name} lacks a width-one advancement witness")
        chains.append(
            {
                "name": name,
                "base_seed_u63": str(base_seed),
                "episode_index_u63": str(episode_index),
                "opponent_physical_decision_index_u63": str(decision_index),
                "opponent_group_seed_u63": str(group_seed),
                "substeps": substeps,
                "width_one_advancement_witnesses": witnesses,
            }
        )
    return chains


def append_semantic_atom(result: bytearray, tag: str, payload: bytes) -> None:
    result += atom(tag, payload)


def semantic_stream_bytes(value: dict[str, Any]) -> bytes:
    result = bytearray()

    def text_atom(tag: str, content: str) -> None:
        append_semantic_atom(result, tag, content.encode("utf-8"))

    def u32_atom(tag: str, content: int) -> None:
        append_semantic_atom(result, tag, content.to_bytes(4, "big"))

    def u64_atom(tag: str, content: int) -> None:
        append_semantic_atom(result, tag, content.to_bytes(8, "big"))

    sampler = value["sampler"]
    seed_chain = value["seed_chain"]
    text_atom("schema", value["schema"])
    u32_atom("vector-schema-version", value["vector_schema_version"])
    text_atom("sampler-identity", sampler["identity"])
    text_atom("sampler-algorithm", sampler["algorithm"])
    text_atom("legal-count-zero-rule", sampler["legal_count_zero_rule"])
    text_atom("width-one-seed-rule", sampler["width_one_seed_rule"])
    text_atom("modulo-bias-rule", sampler["modulo_bias_rule"])
    text_atom("trainer-schedule-version", seed_chain["trainer_schedule_version"])
    text_atom("python-reference-seed-version", seed_chain["python_reference_seed_version"])
    append_semantic_atom(
        result,
        "schedule-goldens-sha256",
        bytes.fromhex(seed_chain["schedule_goldens_sha256"]),
    )
    text_atom("seed-atom-framing-identity", seed_chain["atom_framing_identity"])
    text_atom(
        "seed-text-payload-encoding",
        seed_chain["payload_encodings"]["text"],
    )
    text_atom(
        "seed-u63-payload-encoding",
        seed_chain["payload_encodings"]["u63"],
    )
    text_atom("seed-derivation-algorithm", seed_chain["derivation_algorithm"])
    text_atom("width-one-witness-rule", seed_chain["witness_rule"])
    text_atom("group-namespace", seed_chain["group_namespace"])
    u32_atom("group-field-count", len(seed_chain["group_fields_ordered"]))
    for field in seed_chain["group_fields_ordered"]:
        text_atom("group-field", field)
    text_atom("substep-namespace", seed_chain["substep_namespace"])
    u32_atom("substep-field-count", len(seed_chain["substep_fields_ordered"]))
    for field in seed_chain["substep_fields_ordered"]:
        text_atom("substep-field", field)

    u32_atom("point-count", len(value["points"]))
    for point in value["points"]:
        text_atom("point-name", point["name"])
        u64_atom("action-seed-u64", int(point["action_seed_u64"]))
        u32_atom("legal-count-u32", point["legal_count_u32"])
        u32_atom("selected-index-u32", point["selected_index_u32"])

    u32_atom("rejection-count", len(value["rejections"]))
    for rejection in value["rejections"]:
        text_atom("rejection-name", rejection["name"])
        u64_atom("action-seed-u64", int(rejection["action_seed_u64"]))
        u32_atom("legal-count-u32", rejection["legal_count_u32"])
        text_atom("error-code", rejection["expected_error"]["code"])

    u32_atom("chain-count", len(value["chains"]))
    for chain in value["chains"]:
        text_atom("chain-name", chain["name"])
        u64_atom("base-seed-u63", int(chain["base_seed_u63"]))
        u64_atom("episode-index-u63", int(chain["episode_index_u63"]))
        u64_atom(
            "opponent-physical-decision-index-u63",
            int(chain["opponent_physical_decision_index_u63"]),
        )
        u64_atom("opponent-group-seed-u63", int(chain["opponent_group_seed_u63"]))
        u32_atom("substep-count", len(chain["substeps"]))
        for substep in chain["substeps"]:
            u32_atom("substep-index-u32", substep["substep_index_u32"])
            u32_atom("legal-count-u32", substep["legal_count_u32"])
            u64_atom("action-seed-u63", int(substep["action_seed_u63"]))
            u32_atom("selected-index-u32", substep["selected_index_u32"])
        witnesses = chain["width_one_advancement_witnesses"]
        u32_atom("width-one-witness-count", len(witnesses))
        for witness in witnesses:
            u32_atom(
                "width-one-substep-index-u32",
                witness["width_one_substep_index_u32"],
            )
            u64_atom(
                "width-one-action-seed-u63",
                int(witness["width_one_action_seed_u63"]),
            )
            u32_atom("next-substep-index-u32", witness["next_substep_index_u32"])
            u64_atom("next-action-seed-u63", int(witness["next_action_seed_u63"]))
            u32_atom(
                "counterfactual-nonconsuming-next-substep-index-u32",
                witness["counterfactual_nonconsuming_next_substep_index_u32"],
            )
            u64_atom(
                "counterfactual-nonconsuming-next-action-seed-u63",
                int(witness["counterfactual_nonconsuming_next_action_seed_u63"]),
            )
    return bytes(result)


def payload(repository_root: Path) -> dict[str, Any]:
    value: dict[str, Any] = {
        "schema": SCHEMA,
        "vector_schema_version": VECTOR_SCHEMA_VERSION,
        "sampler": {
            "identity": SAMPLER_IDENTITY,
            "algorithm": SAMPLER_ALGORITHM,
            "action_seed_domain": "unsigned-u64-inclusive-0-through-18446744073709551615",
            "legal_count_domain": "unsigned-u32-inclusive-1-through-4294967295",
            "legal_count_zero_rule": LEGAL_COUNT_ZERO_RULE,
            "width_one_seed_rule": WIDTH_ONE_SEED_RULE,
            "modulo_bias_rule": MODULO_BIAS_RULE,
        },
        "seed_chain": {
            "trainer_schedule_version": TRAINER_SCHEDULE_VERSION,
            "python_reference_seed_version": PYTHON_REFERENCE_SEED_VERSION,
            "schedule_goldens_sha256": SCHEDULE_GOLDENS_SHA256,
            "atom_framing_identity": SEED_ATOM_FRAMING_IDENTITY,
            "atom_formula": SEED_ATOM_FORMULA,
            "payload_encodings": {
                "text": SEED_TEXT_PAYLOAD_ENCODING,
                "u63": SEED_U63_PAYLOAD_ENCODING,
            },
            "derivation_algorithm": SEED_DERIVATION_ALGORITHM,
            "witness_rule": WIDTH_ONE_WITNESS_RULE,
            "seed_integer_domain": "u63 values encoded as exactly 8-byte big-endian",
            "substep_index_domain": "u32 converted without truncation then encoded as u63",
            "group_namespace": GROUP_NAMESPACE,
            "group_fields_ordered": list(GROUP_FIELDS),
            "substep_namespace": SUBSTEP_NAMESPACE,
            "substep_fields_ordered": list(SUBSTEP_FIELDS),
        },
        "authority": {
            "implementation": GENERATOR_IDENTITY,
            "generator": GENERATOR_RELATIVE.as_posix(),
            "generator_sha256": sha256_hex(
                (repository_root / GENERATOR_RELATIVE).read_bytes()
            ),
            "forbidden_dependencies": [
                "mtg_kernel_rl",
                "rust-ffi",
                "java",
                "numpy",
                "torch",
            ],
        },
        "semantic_stream": {
            "framing_identity": SEMANTIC_STREAM_FRAMING_IDENTITY,
            "atom_formula": SEMANTIC_STREAM_ATOM_FORMULA,
            "payload_encodings": {
                "text": "UTF-8",
                "counts-and-u32-values": "exactly-4-byte-big-endian",
                "u63-and-u64-values": "exactly-8-byte-big-endian",
                "sha256": "raw-32-bytes-decoded-from-lowercase-hex",
            },
            "order": SEMANTIC_STREAM_ORDER,
        },
        "point_count": len(POINT_INPUTS),
        "points": build_points(),
        "rejection_count": len(REJECTION_INPUTS),
        "rejections": build_rejections(),
        "chain_count": len(CHAIN_INPUTS),
        "chains": build_chains(),
        "nonclaims": [
            "not-a-bias-free-sampler",
            "not-a-seed-derivation-change",
            "not-learning-noninferiority-evidence",
            "not-end-to-end-throughput-evidence",
        ],
    }
    value["semantic_stream"]["sha256"] = sha256_hex(semantic_stream_bytes(value))
    return value


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    repository_root = Path(__file__).resolve().parents[2]
    output_path = repository_root / OUTPUT_RELATIVE
    expected = canonical_json_bytes(payload(repository_root))
    if args.check:
        if not output_path.is_file() or output_path.read_bytes() != expected:
            print("NATIVE_OPPONENT_SAMPLER_VECTORS: STALE", file=sys.stderr)
            return 1
        print("NATIVE_OPPONENT_SAMPLER_VECTORS: PASS")
        return 0
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_bytes(expected)
    print(f"wrote {output_path}")
    print(f"sha256={sha256_hex(expected)}")
    print(f"semantic_stream_sha256={payload(repository_root)['semantic_stream']['sha256']}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
