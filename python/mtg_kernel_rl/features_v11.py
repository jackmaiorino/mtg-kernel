"""FlatV8: the V8 inclusion contract on FlatV7.

W4 design (collab LANES/w4-combat-20261030/DESIGN.md v5.1, section 5, "V8
inclusion contract"). The frozen features_v10.py
(FlatV7) encoder supplies every tensor of every decision, London pregame
decisions included, and FlatV8 adds exactly two things:

- **Include bit.** One action column appended at the end of each action row
  (index 195, so the action width is 196): 1.0 on the ``include: true`` row of
  every attacker-inclusion and blocker-inclusion decision, 0.0 on every other
  row (exclude rows, every other action and every pregame row). Without it the
  include and exclude rows of one decision differ only in the action hash.
- **Pairing relation.** For every block already chosen in the pending blocker
  declaration, one ``(blocker node, attacker node)`` row of the decision's
  object nodes: first each pair of ``surface_context.private_blockers.
  accumulated`` (blocks for earlier attackers of the declaration), in order,
  then each blocker already selected for the current attacker
  (``policy_surface_context.private_combat_selection`` at a blocker-inclusion
  decision), in order. It is empty at every other decision. The model feeds it
  to a separate additive relation path whose output starts at zero.

Nothing else is added: no "would die", no "good block", no value judgment and
no new card reference (the attacker and blocker references already exist).
Every other tensor equals the FlatV7 tensor bit for bit.
"""
from __future__ import annotations

import hashlib
import json
from dataclasses import dataclass, replace
from typing import Any

import torch

from . import features_v7 as frozen_v4
from . import features_v10 as predecessor

FeatureSchemaError = predecessor.FeatureSchemaError
FeatureSchema = predecessor.FeatureSchema
FEATURE_SCHEMA_VERSION = "actor-relative-v11-python-1"
FEATURE_REGISTRY_VERSION = "rust-observation-v11-action-v7-registry-1"
ENCODING_CONTRACT_VERSION = "actor-relative-node-graph-20"
MODEL_CONTRACT_VERSION = predecessor.MODEL_CONTRACT_VERSION
STATE_FEATURE_DIM = predecessor.STATE_FEATURE_DIM
OBJECT_FEATURE_DIM = predecessor.OBJECT_FEATURE_DIM
EDGE_FEATURE_DIM = predecessor.EDGE_FEATURE_DIM
PREDECESSOR_ACTION_FEATURE_DIM = predecessor.ACTION_FEATURE_DIM
INCLUDE_COLUMN = PREDECESSOR_ACTION_FEATURE_DIM
ACTION_FEATURE_DIM = PREDECESSOR_ACTION_FEATURE_DIM + 1
ACTION_REF_FEATURE_DIM = predecessor.ACTION_REF_FEATURE_DIM
INCLUSION_ACTION_KINDS = ("choose_attacker_inclusion", "choose_blocker_inclusion")
PAIRING_ROW = ("blocker", "attacker")


@dataclass(frozen=True)
class EncodedDecision:
    """The FlatV7 tensors with 196-wide action rows, plus the pairing rows."""

    schema: FeatureSchema
    state: torch.Tensor
    object_features: torch.Tensor
    object_card_ids: torch.Tensor
    object_groups: torch.Tensor
    object_node_ids: torch.Tensor
    edge_features: torch.Tensor
    edge_source_indices: torch.Tensor
    edge_target_indices: torch.Tensor
    action_features: torch.Tensor
    action_ref_features: torch.Tensor
    action_ref_card_ids: torch.Tensor
    action_ref_action_indices: torch.Tensor
    action_ref_node_indices: torch.Tensor
    pairing_node_indices: torch.Tensor


def _digest(value: Any) -> str:
    raw = json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False)
    return hashlib.sha256(raw.encode("utf-8")).hexdigest()


def feature_contract_fingerprint() -> str:
    return _digest({
        "registry_version": FEATURE_REGISTRY_VERSION,
        "predecessor_contract": predecessor.feature_contract_fingerprint(),
        "observation": "unchanged FlatV7 observation and legal actions",
        "include_bit": {"source": "legal_action.semantic.include", "kinds": list(INCLUSION_ACTION_KINDS)},
        "pairing": {
            "sources": ["projection.surface_context.private_blockers.accumulated",
                        "projection.policy_surface_context.private_combat_selection"],
            "row": list(PAIRING_ROW),
        },
        "not_added": ["would die", "good block", "value judgment", "card reference"],
    })


def encoding_contract_fingerprint() -> str:
    return _digest({
        "encoding_version": ENCODING_CONTRACT_VERSION,
        "predecessor_encoding": predecessor.encoding_contract_fingerprint(),
        "dimensions": {"state": STATE_FEATURE_DIM, "object": OBJECT_FEATURE_DIM, "edge": EDGE_FEATURE_DIM,
                       "action": ACTION_FEATURE_DIM, "action_ref": ACTION_REF_FEATURE_DIM},
        "include_column": {
            "index": INCLUDE_COLUMN,
            "value": "1.0 on the include:true row of an attacker- or blocker-inclusion decision, else 0.0",
        },
        "pairing_node_indices": {
            "shape": ["pairs", 2],
            "row": list(PAIRING_ROW),
            "order": "accumulated pairs in order, then the current attacker's selected blockers in order",
            "present": "only during a blocker declaration with at least one pending block",
        },
        "other_tensors": "FlatV7 tensors bit for bit",
    })


def model_contract_fingerprint(schema: FeatureSchema) -> str:
    return _digest({"model_contract_version": MODEL_CONTRACT_VERSION,
                    "feature_schema": schema.__dict__})


def classification_registry() -> dict[str, str]:
    return predecessor.classification_registry()


def include_bits(legal_actions: list[dict[str, Any]]) -> list[float]:
    """The include column of each action row, in row order."""
    bits = []
    for action in legal_actions:
        semantic = action.get("semantic") if isinstance(action, dict) else None
        if not isinstance(semantic, dict):
            raise FeatureSchemaError("legal action requires a semantic object")
        inclusion = semantic.get("action_kind") in INCLUSION_ACTION_KINDS
        bits.append(1.0 if inclusion and semantic.get("include") is True else 0.0)
    return bits


def pending_block_refs(observation: dict[str, Any]) -> list[tuple[dict[str, Any], dict[str, Any]]]:
    """The pending declaration's ``(blocker, attacker)`` stable references."""
    projection = observation["projection"]
    actor = observation["acting_player"]
    pairs: list[tuple[dict[str, Any], dict[str, Any]]] = []
    blockers = projection["surface_context"].get("private_blockers")
    if blockers is not None:
        for blocker, attacker in blockers["accumulated"]:
            pairs.append((blocker, attacker))
    selection = projection["policy_surface_context"].get("private_combat_selection")
    if selection is not None and selection.get("attacker") is not None:
        pairs.extend((blocker, selection["attacker"]) for blocker in selection["selected"])
    for blocker, attacker in pairs:
        if blocker["controller"] != actor or attacker["controller"] == actor:
            raise FeatureSchemaError("a pending block pairs an own blocker with an opposing attacker")
    return pairs


def _base_observation(observation: dict[str, Any]) -> dict[str, Any] | None:
    """The V4 observation of an in-game decision, None at a pregame one."""
    base, london = predecessor._split_observation(observation)
    return None if london is not None else base


def pairing_node_indices(observation: dict[str, Any]) -> list[list[int]]:
    """The pairing rows as object node indices of the decision."""
    base = _base_observation(observation)
    if base is None:
        return []
    refs = pending_block_refs(base)
    if not refs:
        return []
    registry = frozen_v4._objects(base)[0]
    return [[registry.resolve(blocker), registry.resolve(attacker)] for blocker, attacker in refs]


def _schema_v11(schema: FeatureSchema) -> FeatureSchema:
    return replace(schema, version=FEATURE_SCHEMA_VERSION, registry_version=FEATURE_REGISTRY_VERSION,
                   contract_digest=feature_contract_fingerprint(),
                   encoding_digest=encoding_contract_fingerprint(),
                   action_feature_dim=ACTION_FEATURE_DIM)


def encode_decision(observation: dict[str, Any], legal_actions: list[dict[str, Any]]) -> EncodedDecision:
    encoded = predecessor.encode_decision(observation, legal_actions)
    bits = torch.tensor(include_bits(legal_actions), dtype=torch.float32).reshape(-1, 1)
    if bits.shape[0] != encoded.action_features.shape[0]:
        raise FeatureSchemaError("one include bit per action row")
    pairs = pairing_node_indices(observation)
    pairing = (torch.tensor(pairs, dtype=torch.long) if pairs
               else torch.empty((0, 2), dtype=torch.long))
    fields = {name: getattr(encoded, name) for name in encoded.__dataclass_fields__ if name != "schema"}
    fields["action_features"] = torch.cat([encoded.action_features, bits], dim=1)
    return EncodedDecision(schema=_schema_v11(encoded.schema), pairing_node_indices=pairing, **fields)
