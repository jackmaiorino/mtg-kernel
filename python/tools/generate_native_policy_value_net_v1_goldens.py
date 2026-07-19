"""Generate Torch-authoritative CPU forward goldens for the native reference.

The checked fixture deliberately covers the model architecture with small,
synthetic EncodedDecision tensors.  It is an inference-parity fixture, not a
training, throughput, or cross-libm bit-parity claim.
"""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import struct
import sys
from typing import Any

import torch


ROOT = Path(__file__).resolve().parents[2]
if str(ROOT / "python") not in sys.path:
    sys.path.insert(0, str(ROOT / "python"))

from mtg_kernel_rl.features import (  # noqa: E402
    ACTION_FEATURE_DIM,
    ACTION_REF_FEATURE_DIM,
    EDGE_FEATURE_DIM,
    EncodedDecision,
    FeatureSchema,
    OBJECT_FEATURE_DIM,
    OBJECT_GROUPS,
    STATE_FEATURE_DIM,
)
from mtg_kernel_rl.model import (  # noqa: E402
    INITIALIZER_RUNNER_FIXED_V1,
    KernelPolicyValueNet,
    ModelConfig,
)


SCHEMA = "native-policy-value-net-v1-torch-goldens-v1"
MODEL_AUTHORITY = ROOT / "python" / "mtg_kernel_rl" / "model.py"
EXPECTED_MODEL_AUTHORITY_SHA256 = "2e3e830d4212b8c8f8085861b2508c49a6d7192b9621cef087dd396e22d12c59"
OUTPUT = ROOT / "data" / "native_policy_value_net_v1" / "runner_fixed_forward_goldens_v1.json"


def _sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def _pattern(length: int, *, multiplier: int, modulus: int, center: int, denominator: int) -> list[float]:
    values = [((index * multiplier) % modulus - center) / denominator for index in range(length)]
    # All fixture denominators are powers of two, so JSON -> float32 is exact.
    return values


def _schema(config: ModelConfig) -> FeatureSchema:
    return FeatureSchema(
        version=config.feature_schema_version,
        registry_version=config.feature_registry_version,
        contract_digest=config.feature_contract_digest,
        encoding_digest=config.feature_encoding_digest,
        state_dim=config.state_dim,
        object_feature_dim=config.object_feature_dim,
        edge_feature_dim=config.edge_feature_dim,
        action_feature_dim=config.action_feature_dim,
        object_group_count=config.object_group_count,
        action_ref_feature_dim=config.action_ref_feature_dim,
    )


def _tensor_f32(values: list[float], rows: int | None = None) -> torch.Tensor:
    tensor = torch.tensor(values, dtype=torch.float32)
    if rows is not None:
        tensor = tensor.reshape(rows, -1)
    return tensor


def _tensor_i64(values: list[int]) -> torch.Tensor:
    return torch.tensor(values, dtype=torch.long)


def _case_inputs() -> list[dict[str, Any]]:
    return [
        {
            "name": "zero_edges_zero_action_refs",
            "state": _pattern(STATE_FEATURE_DIM, multiplier=7, modulus=17, center=8, denominator=8),
            "object_features": _pattern(2 * OBJECT_FEATURE_DIM, multiplier=5, modulus=23, center=11, denominator=16),
            "object_card_ids": [0, 65_536],
            "object_groups": [0, len(OBJECT_GROUPS) - 1],
            "object_node_ids": [0, 1],
            "edge_features": [],
            "edge_source_indices": [],
            "edge_target_indices": [],
            "action_features": _pattern(2 * ACTION_FEATURE_DIM, multiplier=11, modulus=29, center=14, denominator=16),
            "action_ref_features": [],
            "action_ref_card_ids": [],
            "action_ref_action_indices": [],
            "action_ref_node_indices": [],
        },
        {
            "name": "ordered_edges_and_action_refs",
            "state": _pattern(STATE_FEATURE_DIM, multiplier=13, modulus=31, center=15, denominator=16),
            "object_features": _pattern(3 * OBJECT_FEATURE_DIM, multiplier=9, modulus=27, center=13, denominator=8),
            "object_card_ids": [65_536, 1, 0],
            "object_groups": [3, 3, 11],
            "object_node_ids": [0, 1, 2],
            "edge_features": _pattern(3 * EDGE_FEATURE_DIM, multiplier=7, modulus=19, center=9, denominator=16),
            # The middle self-edge must be accumulated twice: once by each
            # ordered index_add pass in model.py.
            "edge_source_indices": [0, 2, 1],
            "edge_target_indices": [1, 2, 0],
            "action_features": _pattern(3 * ACTION_FEATURE_DIM, multiplier=17, modulus=37, center=18, denominator=16),
            "action_ref_features": _pattern(4 * ACTION_REF_FEATURE_DIM, multiplier=3, modulus=13, center=6, denominator=8),
            "action_ref_card_ids": [0, 65_536, 17, 1],
            "action_ref_action_indices": [1, 0, 1, 2],
            "action_ref_node_indices": [2, 0, 1, 2],
        },
    ]


def _encoded(case: dict[str, Any], config: ModelConfig) -> EncodedDecision:
    object_count = len(case["object_card_ids"])
    edge_count = len(case["edge_source_indices"])
    action_count = len(case["action_features"]) // ACTION_FEATURE_DIM
    ref_count = len(case["action_ref_card_ids"])
    return EncodedDecision(
        schema=_schema(config),
        state=_tensor_f32(case["state"]),
        object_features=_tensor_f32(case["object_features"], object_count),
        object_card_ids=_tensor_i64(case["object_card_ids"]),
        object_groups=_tensor_i64(case["object_groups"]),
        object_node_ids=_tensor_i64(case["object_node_ids"]),
        edge_features=torch.empty((0, EDGE_FEATURE_DIM), dtype=torch.float32)
        if edge_count == 0
        else _tensor_f32(case["edge_features"], edge_count),
        edge_source_indices=_tensor_i64(case["edge_source_indices"]),
        edge_target_indices=_tensor_i64(case["edge_target_indices"]),
        action_features=_tensor_f32(case["action_features"], action_count),
        action_ref_features=torch.empty((0, ACTION_REF_FEATURE_DIM), dtype=torch.float32)
        if ref_count == 0
        else _tensor_f32(case["action_ref_features"], ref_count),
        action_ref_card_ids=_tensor_i64(case["action_ref_card_ids"]),
        action_ref_action_indices=_tensor_i64(case["action_ref_action_indices"]),
        action_ref_node_indices=_tensor_i64(case["action_ref_node_indices"]),
    )


def _f32_bits(value: float) -> str:
    return f"0x{struct.unpack('<I', struct.pack('<f', value))[0]:08x}"


def _parameter_manifest(model: KernelPolicyValueNet) -> dict[str, Any]:
    digest = hashlib.sha256()
    ordered: list[dict[str, Any]] = []
    count = 0
    with torch.no_grad():
        for name, parameter in model.named_parameters():
            contiguous = parameter.detach().cpu().contiguous()
            name_bytes = name.encode("utf-8")
            shape = list(contiguous.shape)
            digest.update(struct.pack(">I", len(name_bytes)))
            digest.update(name_bytes)
            digest.update(struct.pack(">I", len(shape)))
            for dimension in shape:
                digest.update(struct.pack(">Q", dimension))
            raw = contiguous.numpy().astype("<f4", copy=False).tobytes(order="C")
            digest.update(struct.pack(">Q", contiguous.numel()))
            digest.update(raw)
            flat = contiguous.reshape(-1)
            ordered.append(
                {
                    "name": name,
                    "shape": shape,
                    "count": contiguous.numel(),
                    "first_bits": _f32_bits(float(flat[0])),
                    "last_bits": _f32_bits(float(flat[-1])),
                }
            )
            count += contiguous.numel()
    return {
        "digest_contract": "sha256(u32_be(name_len)||name||u32_be(rank)||u64_be(dims...)||u64_be(count)||f32_le_bytes), named_parameters order",
        "sha256": digest.hexdigest(),
        "count": count,
        "ordered": ordered,
    }


def _payload() -> dict[str, Any]:
    authority_sha = _sha256(MODEL_AUTHORITY)
    if authority_sha != EXPECTED_MODEL_AUTHORITY_SHA256:
        raise RuntimeError(
            "python model.py authority drifted: "
            f"expected {EXPECTED_MODEL_AUTHORITY_SHA256}, got {authority_sha}"
        )
    config = ModelConfig()
    model = KernelPolicyValueNet(config, initializer=INITIALIZER_RUNNER_FIXED_V1)
    cases: list[dict[str, Any]] = []
    with torch.no_grad():
        for raw in _case_inputs():
            encoded = _encoded(raw, config)
            logits, value = model(encoded)
            cases.append(
                {
                    **raw,
                    "torch_logits": [float(item) for item in logits],
                    "torch_logits_bits": [_f32_bits(float(item)) for item in logits],
                    "torch_value": float(value),
                    "torch_value_bits": _f32_bits(float(value)),
                }
            )
    return {
        "schema": SCHEMA,
        "authority": {
            "path": MODEL_AUTHORITY.relative_to(ROOT).as_posix(),
            "sha256": authority_sha,
            "torch_version": torch.__version__,
            "initializer": INITIALIZER_RUNNER_FIXED_V1,
            "numerical_claim": "Rust reproduces Torch CPU outputs within declared absolute and relative tolerances; no cross-libm or bit-parity claim",
            "absolute_tolerance": 2.0e-5,
            "relative_tolerance": 2.0e-5,
        },
        "model_config": config.to_dict(),
        "model_config_fingerprint": config.contract_fingerprint(),
        "parameter_manifest": _parameter_manifest(model),
        "cases": cases,
    }


def _encoded_payload(payload: dict[str, Any]) -> bytes:
    return (json.dumps(payload, sort_keys=True, indent=2) + "\n").encode("utf-8")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true", help="fail if the checked fixture drifts")
    args = parser.parse_args()
    expected = _encoded_payload(_payload())
    if args.check:
        actual = OUTPUT.read_bytes() if OUTPUT.exists() else b""
        if actual != expected:
            raise SystemExit(f"stale native policy/value golden: {OUTPUT}")
        print(f"PASS {OUTPUT.relative_to(ROOT)}")
        return 0
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    OUTPUT.write_bytes(expected)
    print(f"wrote {OUTPUT.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
