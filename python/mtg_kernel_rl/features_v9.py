"""Explicit Raid tensor successor; no predecessor model admission.

The unchanged V8 encoder supplies the first thirteen tensor groups. Two
actor-relative public declaration flags extend only the state tensor.
JSON validation checks consistency; the engine authenticates history.
"""
from __future__ import annotations

import hashlib
import json
from dataclasses import replace
from typing import Any

import torch

from . import features_v8 as predecessor

FeatureSchemaError = predecessor.FeatureSchemaError
FeatureSchema = predecessor.FeatureSchema
EncodedDecision = predecessor.EncodedDecision
FEATURE_SCHEMA_VERSION = "actor-relative-v9-python-1"
FEATURE_REGISTRY_VERSION = "rust-observation-v9-action-v5-registry-1"
ENCODING_CONTRACT_VERSION = "actor-relative-node-graph-18"
MODEL_CONTRACT_VERSION = "kernel-policy-value-net-11"
STATE_FEATURE_DIM = predecessor.STATE_FEATURE_DIM + 2


def _digest(value: Any) -> str:
    raw = json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False)
    return hashlib.sha256(raw.encode("utf-8")).hexdigest()


def feature_contract_fingerprint() -> str:
    return _digest({
        "registry_version": FEATURE_REGISTRY_VERSION,
        "observation_schema": 9,
        "predecessor_contract": predecessor.feature_contract_fingerprint(),
        "extensions": {
            "closed": True,
            "predecessor": "unchanged V8 extensions",
            "attacked_this_turn": {"classification": "model_input", "type": "bool[2]",
                                   "seat_order": ["p0", "p1"], "only_active_seat": True},
        },
    })


def encoding_contract_fingerprint() -> str:
    return _digest({
        "encoding_version": ENCODING_CONTRACT_VERSION,
        "predecessor_encoding": predecessor.encoding_contract_fingerprint(),
        "state_dim": STATE_FEATURE_DIM,
        "state_suffix": ["actor_attacked_this_turn", "opponent_attacked_this_turn"],
        "suffix_dtype": "float32",
        "suffix_values": [0.0, 1.0],
        "other_tensors": "unchanged V8",
    })


def model_contract_fingerprint(schema: FeatureSchema) -> str:
    return _digest({"model_contract_version": MODEL_CONTRACT_VERSION,
                    "feature_schema": schema.__dict__})


def classification_registry() -> dict[str, str]:
    registry = {}
    for path, kind in predecessor.classification_registry().items():
        if path.startswith("observation.extensions."):
            path = path.replace("observation.extensions.",
                                "observation.extensions.predecessor.", 1)
        registry[path] = kind
    registry["observation.extensions.attacked_this_turn.*"] = "model_input"
    return registry


def _predecessor_observation(observation: dict[str, Any]) -> tuple[dict[str, Any], list[bool]]:
    if not isinstance(observation, dict) or type(observation.get("schema_version")) is not int:
        raise FeatureSchemaError("V9 observation requires integer schema_version")
    if observation["schema_version"] != 9:
        raise FeatureSchemaError("V9 encoder requires observation schema 9")
    extensions = observation.get("extensions")
    if not isinstance(extensions, dict) or set(extensions) != {"predecessor", "attacked_this_turn"}:
        raise FeatureSchemaError("V9 extensions require exactly predecessor and attacked_this_turn")
    flags = extensions["attacked_this_turn"]
    if not isinstance(flags, list) or len(flags) != 2 or any(type(flag) is not bool for flag in flags):
        raise FeatureSchemaError("V9 attacked_this_turn requires two exact booleans")
    base = {**observation, "schema_version": 8, "extensions": extensions["predecessor"]}
    # The predecessor validates every inherited field, including unknown keys,
    # before the new flags can reach tensor encoding.
    predecessor.assert_observation_classified(base)
    active = base["projection"]["active_player"]
    if flags[1 if active == "p0" else 0]:
        raise FeatureSchemaError("V9 inactive seat cannot have attacked this turn")
    return base, flags


def encode_decision(observation: dict[str, Any], legal_actions: list[dict[str, Any]]) -> EncodedDecision:
    base, flags = _predecessor_observation(observation)
    encoded = predecessor.encode_decision(base, legal_actions)
    actor_index = 0 if observation["acting_player"] == "p0" else 1
    suffix = encoded.state.new_tensor([float(flags[actor_index]), float(flags[1 - actor_index])])
    state = torch.cat((encoded.state, suffix))
    schema = replace(encoded.schema, version=FEATURE_SCHEMA_VERSION,
                     registry_version=FEATURE_REGISTRY_VERSION,
                     contract_digest=feature_contract_fingerprint(),
                     encoding_digest=encoding_contract_fingerprint(), state_dim=STATE_FEATURE_DIM)
    return replace(encoded, schema=schema, state=state)
