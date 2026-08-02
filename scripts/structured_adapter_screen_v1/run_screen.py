#!/usr/bin/env python3
"""CPU prototype for a parent-preserving structured residual adapter screen."""

from __future__ import annotations

import argparse
import copy
import json
import math
import random
import hashlib
import struct
import sys
import time
from pathlib import Path
from typing import Any, Iterable

import numpy as np
import torch
from torch import Tensor, nn


STATE_DIM = 219
OBJECT_DIM = 98
EDGE_DIM = 41
ACTION_DIM = 195
REF_DIM = 25
FOLDS = 4
SCRIPT_VERSION = "structured_adapter_screen_v1"
TEACHER_SHA256_PREFIX = "24211c"
OUTCOME_SHA256_PREFIX = "ee4224"


def _fail(message: str) -> None:
    raise ValueError(message)


def _u32(value: Any, name: str) -> int:
    if isinstance(value, str):
        value = int(value, 0)
    if not isinstance(value, int) or not 0 <= value <= 0xFFFFFFFF:
        _fail(f"{name} is not a u32")
    return value


def _f32_from_bits(value: Any, name: str) -> float:
    bits = _u32(value, name)
    return struct.unpack("<f", struct.pack("<I", bits))[0]


def _shape_product(shape: Iterable[Any], name: str) -> int:
    product = 1
    for index, dim in enumerate(shape):
        if not isinstance(dim, int) or dim < 0:
            _fail(f"{name}.shape[{index}] is invalid")
        product *= dim
    return product


def _flatten(values: Any) -> list[Any]:
    if isinstance(values, list):
        result: list[Any] = []
        for value in values:
            result.extend(_flatten(value))
        return result
    return [values]


def _decode_float(raw: Any, name: str, expected_shape: tuple[int, ...] | None = None) -> np.ndarray:
    shape: tuple[int, ...] | None = None
    bit_values = False
    values: Any = raw
    if isinstance(raw, dict):
        if "shape" in raw:
            shape = tuple(int(x) for x in raw["shape"])
        if "u32_values" in raw:
            values = raw["u32_values"]
            bit_values = True
        elif "f32_bits" in raw:
            values = raw["f32_bits"]
            bit_values = True
        elif "values" in raw:
            values = raw["values"]
        elif "data" in raw:
            values = raw["data"]
        else:
            _fail(f"{name} has no values")
    flat = _flatten(values)
    if bit_values:
        array = np.asarray([_u32(value, name) for value in flat], dtype=np.uint32).view(np.float32)
    else:
        try:
            array = np.asarray(flat, dtype=np.float32)
        except (TypeError, ValueError) as exc:
            _fail(f"{name} is not numeric: {exc}")
    if shape is None:
        if expected_shape is not None and array.size == math.prod(expected_shape):
            shape = expected_shape
        elif isinstance(values, list) and values and isinstance(values[0], list):
            shape = tuple(np.asarray(values).shape)
        else:
            shape = (int(array.size),)
    if _shape_product(shape, name) != int(array.size):
        _fail(f"{name} element count does not match shape")
    if expected_shape is not None and tuple(shape) != expected_shape:
        _fail(f"{name} shape {shape} expected {expected_shape}")
    array = np.ascontiguousarray(array.reshape(shape), dtype=np.float32)
    if not np.isfinite(array).all():
        _fail(f"{name} contains non-finite values")
    return array


def _decode_int(raw: Any, name: str, expected_length: int | None = None) -> np.ndarray:
    values: Any = raw
    shape: tuple[int, ...] | None = None
    if isinstance(raw, dict):
        if "shape" in raw:
            shape = tuple(int(x) for x in raw["shape"])
        for key in ("u32_values", "values", "data"):
            if key in raw:
                values = raw[key]
                break
        else:
            _fail(f"{name} has no integer values")
    flat = _flatten(values)
    result = np.asarray([_u32(value, name) for value in flat], dtype=np.int64)
    if shape is not None and _shape_product(shape, name) != int(result.size):
        _fail(f"{name} element count does not match shape")
    if expected_length is not None and result.size != expected_length:
        _fail(f"{name} length {result.size} expected {expected_length}")
    return result


def _lookup(row: dict[str, Any], *names: str, default: Any = None) -> Any:
    containers: list[dict[str, Any]] = [row]
    for key in ("tensors", "observation", "features", "data", "terminal", "provenance"):
        value = row.get(key)
        if isinstance(value, dict):
            containers.append(value)
    for container in containers:
        for name in names:
            if name in container:
                return container[name]
    return default


def _seat(value: Any, name: str, default: int | None = None) -> int:
    if value is None:
        if default is None:
            _fail(f"missing {name}")
        return default
    if isinstance(value, str):
        lowered = value.lower()
        if lowered in ("p0", "0", "seat0", "player0"):
            return 0
        if lowered in ("p1", "1", "seat1", "player1"):
            return 1
    if isinstance(value, int) and value in (0, 1):
        return value
    _fail(f"invalid {name}")


def _int_like(value: Any, name: str, default: int | None = None) -> int | None:
    if value is None:
        return default
    if isinstance(value, bool):
        _fail(f"invalid {name}")
    if isinstance(value, int):
        return value
    if isinstance(value, str):
        try:
            return int(value, 0)
        except ValueError:
            try:
                return int(value, 16)
            except ValueError:
                _fail(f"invalid {name}")
    _fail(f"invalid {name}")


def _episode_id(value: Any, name: str) -> str:
    if isinstance(value, (str, int)):
        return str(value)
    _fail(f"invalid {name}")


def _terminal_target(row: dict[str, Any], candidate_seat: int) -> float:
    direct = _lookup(row, "candidate_terminal_reward", default=None)
    if direct is not None:
        array = _decode_float(direct, "candidate_terminal_reward").reshape(-1)
        if array.size != 1 or float(array[0]) not in (-1.0, 0.0, 1.0):
            _fail("candidate_terminal_reward must be exactly -1, 0, or 1")
        return float(array[0])
    direct = _lookup(row, "value_target", "terminal_value", default=None)
    if direct is not None:
        return float(_decode_float(direct, "value_target").reshape(-1)[0])
    reward = _lookup(row, "terminal_reward", "reward", default=None)
    if reward is not None:
        array = _decode_float(reward, "terminal_reward")
        flat = array.reshape(-1)
        if flat.size == 1:
            return float(flat[0])
        if flat.size == 2:
            return float(flat[candidate_seat])
        _fail("terminal_reward must be scalar or two-seat")
    outcome = _lookup(row, "terminal_outcome", "outcome", "result", default=None)
    if isinstance(outcome, str):
        lowered = outcome.lower()
        if lowered in ("win", "won"):
            return 1.0
        if lowered in ("loss", "lost"):
            return -1.0
        if lowered in ("draw", "tie"):
            return 0.0
    _fail("outcome row has no terminal reward or value target")


def _merge_nested(parent: dict[str, Any], child: dict[str, Any]) -> dict[str, Any]:
    merged = dict(parent)
    merged.update(child)
    for key in ("tensors", "observation", "features", "data"):
        if isinstance(parent.get(key), dict) and isinstance(child.get(key), dict):
            value = dict(parent[key])
            value.update(child[key])
            merged[key] = value
    return merged


def _rows(path: Path) -> Iterable[dict[str, Any]]:
    with path.open("r", encoding="utf-8") as handle:
        for line_number, line in enumerate(handle, 1):
            if not line.strip():
                continue
            try:
                row = json.loads(line)
            except json.JSONDecodeError as exc:
                _fail(f"{path}:{line_number}: invalid JSON: {exc}")
            if not isinstance(row, dict):
                _fail(f"{path}:{line_number}: row is not an object")
            decisions = row.get("decisions")
            if isinstance(decisions, list):
                for child in decisions:
                    if not isinstance(child, dict):
                        _fail(f"{path}:{line_number}: nested decision is not an object")
                    yield _merge_nested(row, child)
            else:
                yield row


def _parse_example(row: dict[str, Any], is_outcome: bool) -> dict[str, Any]:
    state = _decode_float(_lookup(row, "state", "state_features"), "state", (STATE_DIM,))
    objects = _decode_float(_lookup(row, "object_features"), "object_features")
    if objects.ndim != 2 or objects.shape[1] != OBJECT_DIM:
        _fail(f"object_features shape {objects.shape} expected [N,{OBJECT_DIM}]")
    n_objects = int(objects.shape[0])
    card_ids = _decode_int(_lookup(row, "object_card_ids"), "object_card_ids", n_objects)
    groups = _decode_int(_lookup(row, "object_groups"), "object_groups", n_objects)
    edges = _decode_float(_lookup(row, "edge_features"), "edge_features")
    if edges.size == 0:
        edges = np.zeros((0, EDGE_DIM), dtype=np.float32)
    if edges.ndim != 2 or edges.shape[1] != EDGE_DIM:
        _fail(f"edge_features shape {edges.shape} expected [E,{EDGE_DIM}]")
    edge_src = _decode_int(_lookup(row, "edge_src", "edge_source_indices", "edge_src_indices"), "edge_src", len(edges))
    edge_tgt = _decode_int(_lookup(row, "edge_tgt", "edge_target_indices", "edge_tgt_indices"), "edge_tgt", len(edges))
    if np.any(edge_src >= n_objects) or np.any(edge_tgt >= n_objects):
        _fail("edge endpoint is outside object range")
    actions = _decode_float(_lookup(row, "action_features", "legal_action_features"), "action_features")
    if actions.ndim != 2 or actions.shape[1] != ACTION_DIM or actions.shape[0] == 0:
        _fail(f"action_features shape {actions.shape} expected [A,{ACTION_DIM}] with A > 0")
    n_actions = int(actions.shape[0])
    refs = _lookup(row, "action_ref_features", "ref_features", default=None)
    if refs is None:
        ref_features = np.zeros((0, REF_DIM), dtype=np.float32)
    else:
        ref_features = _decode_float(refs, "action_ref_features")
        if ref_features.size == 0:
            ref_features = np.zeros((0, REF_DIM), dtype=np.float32)
    if ref_features.ndim != 2 or ref_features.shape[1] != REF_DIM:
        _fail(f"action_ref_features shape {ref_features.shape} expected [R,{REF_DIM}]")
    n_refs = int(ref_features.shape[0])
    ref_actions_raw = _lookup(row, "action_ref_action_indices", "ref_action_indices", default=[])
    ref_nodes_raw = _lookup(row, "action_ref_node_indices", "ref_node_indices", default=[])
    ref_actions = _decode_int(ref_actions_raw, "action_ref_action_indices", n_refs)
    ref_nodes = _decode_int(ref_nodes_raw, "action_ref_node_indices", n_refs)
    if np.any(ref_actions >= n_actions) or np.any(ref_nodes >= n_objects):
        _fail("action reference endpoint is outside action or object range")
    old_logits = _decode_float(_lookup(row, "old_logits", "parent_logits", "policy_logits"), "old_logits")
    if old_logits.ndim != 1 or old_logits.shape[0] != n_actions:
        _fail(f"old_logits shape {old_logits.shape} expected [{n_actions}]")
    old_value_raw = _lookup(row, "old_value", "parent_value", "value_prediction", "value", default=None)
    if old_value_raw is None:
        _fail("missing old_value")
    old_value = float(_decode_float(old_value_raw, "old_value").reshape(-1)[0])
    selected_raw = _lookup(row, "selected_index", "selected_index_u32", "selected_action_index", default=None)
    if selected_raw is None:
        _fail("missing selected_index")
    selected = int(selected_raw)
    if not 0 <= selected < n_actions:
        _fail("selected_index is outside action range")
    pair = _int_like(_lookup(row, "pair_index", "pair_index_u64", "pair", default=None), "pair_index")
    if pair is None or pair < 0:
        _fail("missing or invalid pair_index")
    episode = _episode_id(_lookup(row, "episode", "episode_id", "episode_index", "episode_index_u64"), "episode")
    acting = _seat(_lookup(row, "acting_seat", "acting_player", "actor_seat", "actor"), "acting_seat")
    candidate = _seat(_lookup(row, "candidate_seat", "candidate_player"), "candidate_seat", acting)
    substeps_raw = _lookup(row, "substep_count", "substep_count_u32", "decision_substep_count", default=1)
    substeps_raw = _int_like(substeps_raw, "substep_count")
    if substeps_raw is None or substeps_raw < 1:
        _fail("invalid substep_count")
    substep_index = _int_like(_lookup(row, "substep_index", "substep_index_u32"), "substep_index", 0)
    if substep_index is None or substep_index >= substeps_raw:
        _fail("substep_index is outside substep_count")
    physical_group = _int_like(_lookup(row, "physical_decision_id", "physical_decision_id_u32", "physical_decision_id_u64", "physical_decision_ordinal", "physical_decision_ordinal_u64", "physical_decision_ordinal_u64_hex"), "physical_decision_id")
    if physical_group is None and not is_outcome:
        _fail("teacher row is missing physical_decision_id")
    decision_kind = _lookup(row, "decision_kind", "kind", default="unknown")
    if not isinstance(decision_kind, str):
        decision_kind = str(decision_kind)
    if is_outcome:
        terminal_classification = _lookup(row, "terminal_classification", "classification", default=None)
        terminal_code = _lookup(row, "terminal_code", default=None)
        if terminal_classification != "natural" or terminal_code not in (None, "natural-game-over"):
            _fail("outcome row does not have a natural terminal")
        if _lookup(row, "candidate_terminal_reward", default=None) is None:
            _fail("outcome row is missing candidate_terminal_reward")
    target = _terminal_target(row, candidate) if is_outcome else None
    return {
        "state": torch.from_numpy(state.copy()),
        "object_features": torch.from_numpy(objects.copy()),
        "object_card_ids": torch.from_numpy(card_ids.copy()),
        "object_groups": torch.from_numpy(groups.copy()),
        "edge_features": torch.from_numpy(edges.copy()),
        "edge_src": torch.from_numpy(edge_src.copy()),
        "edge_tgt": torch.from_numpy(edge_tgt.copy()),
        "action_features": torch.from_numpy(actions.copy()),
        "action_ref_features": torch.from_numpy(ref_features.copy()),
        "ref_action_indices": torch.from_numpy(ref_actions.copy()),
        "ref_node_indices": torch.from_numpy(ref_nodes.copy()),
        "old_logits": torch.from_numpy(old_logits.copy()),
        "old_value": torch.tensor(old_value, dtype=torch.float32),
        "selected_index": selected,
        "pair_index": pair,
        "acting_seat": acting,
        "candidate_seat": candidate,
        "episode": episode,
        "substep_count": substeps_raw,
        "substep_index": substep_index,
        "physical_group": physical_group,
        "decision_kind": decision_kind,
        "terminal_reward": target,
    }


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def _validate_teacher_groups(policy: list[dict[str, Any]]) -> None:
    groups: dict[tuple[str, int], list[dict[str, Any]]] = {}
    for example in policy:
        physical_group = example["physical_group"]
        if physical_group is None:
            _fail("teacher physical group is missing")
        groups.setdefault((example["episode"], physical_group), []).append(example)
    for key, rows in groups.items():
        counts = {row["substep_count"] for row in rows}
        seats = {row["acting_seat"] for row in rows}
        indexes = [row["substep_index"] for row in rows]
        if len(counts) != 1 or len(seats) != 1:
            _fail(f"teacher physical group {key} has inconsistent metadata")
        count = next(iter(counts))
        if len(rows) != count or sorted(indexes) != list(range(count)):
            _fail(f"teacher physical group {key} is incomplete")


def prepare_cache(teacher_path: Path, outcome_path: Path, cache_path: Path, teacher_sha_prefix: str, outcome_sha_prefix: str) -> dict[str, Any]:
    teacher_sha = _sha256(teacher_path)
    outcome_sha = _sha256(outcome_path)
    if teacher_sha_prefix and not teacher_sha.startswith(teacher_sha_prefix.lower()):
        _fail(f"teacher SHA-256 {teacher_sha} does not start with required {teacher_sha_prefix}")
    if outcome_sha_prefix and not outcome_sha.startswith(outcome_sha_prefix.lower()):
        _fail(f"outcome SHA-256 {outcome_sha} does not start with required {outcome_sha_prefix}")
    policy = [_parse_example(row, False) for row in _rows(teacher_path)]
    value = [_parse_example(row, True) for row in _rows(outcome_path)]
    if not policy or not value:
        _fail("teacher and outcome streams must both contain examples")
    _validate_teacher_groups(policy)
    card_max = max(int(example["object_card_ids"].max().item()) for example in policy + value)
    group_max = max(int(example["object_groups"].max().item()) for example in policy + value)
    payload = {
        "version": SCRIPT_VERSION,
        "policy": policy,
        "value": value,
        "card_max": card_max,
        "group_max": group_max,
        "source": {"teacher": str(teacher_path), "outcome": str(outcome_path), "teacher_sha256": teacher_sha, "outcome_sha256": outcome_sha},
    }
    cache_path.parent.mkdir(parents=True, exist_ok=True)
    torch.save(payload, cache_path)
    return {"cache": str(cache_path), "policy_examples": len(policy), "value_examples": len(value), "card_max": card_max, "group_max": group_max}


def _model_vocab(examples: list[dict[str, Any]]) -> tuple[int, int]:
    card_max = max(int(example["object_card_ids"].max().item()) for example in examples)
    group_max = max(int(example["object_groups"].max().item()) for example in examples)
    return min(max(card_max + 1, 2), 65536), min(max(group_max + 1, 2), 256)


class StructuredAdapter(nn.Module):
    def __init__(self, card_vocab: int, group_vocab: int, dim: int) -> None:
        super().__init__()
        self.dim = dim
        card_dim = max(8, dim // 2)
        group_dim = max(8, dim // 3)
        self.state = nn.Sequential(nn.Linear(STATE_DIM, dim), nn.Tanh(), nn.Linear(dim, dim), nn.Tanh())
        self.object = nn.Sequential(nn.Linear(OBJECT_DIM + card_dim + group_dim, dim), nn.Tanh())
        self.card = nn.Embedding(card_vocab, card_dim)
        self.group = nn.Embedding(group_vocab, group_dim)
        self.edge = nn.Sequential(nn.Linear(dim + EDGE_DIM, dim), nn.Tanh(), nn.Linear(dim, dim))
        self.group_mix = nn.Linear(dim, dim, bias=False)
        self.action = nn.Sequential(nn.Linear(ACTION_DIM, dim), nn.Tanh())
        self.ref = nn.Sequential(nn.Linear(REF_DIM + dim, dim), nn.Tanh())
        self.query = nn.Linear(dim * 2, dim)
        self.combine = nn.Sequential(nn.Linear(dim * 5, dim), nn.Tanh(), nn.Linear(dim, dim), nn.Tanh())
        self.policy_head = nn.Linear(dim, 1)
        self.value_head = nn.Linear(dim * 3, 1)
        nn.init.zeros_(self.policy_head.weight)
        nn.init.zeros_(self.policy_head.bias)
        nn.init.zeros_(self.value_head.weight)
        nn.init.zeros_(self.value_head.bias)
        self.card_vocab = card_vocab
        self.group_vocab = group_vocab

    def force_nonzero_residual(self, seed: int = 123) -> None:
        generator = torch.Generator(device="cpu").manual_seed(seed)
        with torch.no_grad():
            for head in (self.policy_head, self.value_head):
                head.weight.copy_(torch.randn(head.weight.shape, generator=generator) * 0.04)
                head.bias.copy_(torch.randn(head.bias.shape, generator=generator) * 0.02)

    def _one(self, example: dict[str, Any], remove_refs: bool = False) -> tuple[Tensor, Tensor]:
        state = example["state"]
        objects = example["object_features"]
        cards = example["object_card_ids"].long() % self.card_vocab
        groups = example["object_groups"].long() % self.group_vocab
        state_h = self.state(state)
        object_h = self.object(torch.cat((objects, self.card(cards), self.group(groups)), dim=1))
        edges = example["edge_features"]
        if edges.shape[0]:
            source = object_h[example["edge_src"].long()]
            messages = self.edge(torch.cat((source, edges), dim=1))
            aggregate = torch.zeros_like(object_h)
            aggregate.index_add_(0, example["edge_tgt"].long(), messages)
            degree = torch.zeros((object_h.shape[0], 1), dtype=object_h.dtype)
            degree.index_add_(0, example["edge_tgt"].long(), torch.ones((edges.shape[0], 1), dtype=object_h.dtype))
            object_h = object_h + aggregate / (1.0 + degree)
        pooled = torch.zeros((self.group_vocab, self.dim), dtype=object_h.dtype)
        counts = torch.zeros((self.group_vocab, 1), dtype=object_h.dtype)
        pooled.index_add_(0, groups, object_h)
        counts.index_add_(0, groups, torch.ones((object_h.shape[0], 1), dtype=object_h.dtype))
        pooled = pooled / counts.clamp_min(1.0)
        object_h = object_h + self.group_mix(pooled[groups])
        actions = self.action(example["action_features"])
        refs = example["action_ref_features"]
        ref_actions = example["ref_action_indices"].long()
        ref_nodes = example["ref_node_indices"].long()
        if remove_refs:
            refs = refs[:0]
            ref_actions = ref_actions[:0]
            ref_nodes = ref_nodes[:0]
        ref_aggregate = torch.zeros_like(actions)
        if refs.shape[0]:
            ref_h = self.ref(torch.cat((refs, object_h[ref_nodes]), dim=1))
            ref_aggregate.index_add_(0, ref_actions, ref_h)
            ref_counts = torch.zeros((actions.shape[0], 1), dtype=actions.dtype)
            ref_counts.index_add_(0, ref_actions, torch.ones((refs.shape[0], 1), dtype=actions.dtype))
            ref_aggregate = ref_aggregate / ref_counts.clamp_min(1.0)
        queries = self.query(torch.cat((actions, ref_aggregate), dim=1))
        keys = object_h / math.sqrt(float(self.dim))
        attention_logits = queries @ keys.transpose(0, 1)
        attention = torch.softmax(attention_logits, dim=1)
        contexts = attention @ object_h
        joint = self.combine(torch.cat((actions, ref_aggregate, contexts, state_h.expand_as(actions), actions * contexts), dim=1))
        residual_logits = self.policy_head(joint).squeeze(-1)
        object_mean = object_h.mean(dim=0)
        group_mean = pooled.mean(dim=0)
        action_mean = joint.mean(dim=0)
        residual_value = self.value_head(torch.cat((state_h, object_mean + group_mean, action_mean), dim=0)).squeeze()
        return residual_logits, residual_value

    def forward(self, example: dict[str, Any], remove_refs: bool = False) -> tuple[Tensor, Tensor]:
        residual_logits, residual_value = self._one(example, remove_refs=remove_refs)
        return example["old_logits"] + residual_logits, example["old_value"] + residual_value

    def forward_batch(self, examples: list[dict[str, Any]], remove_refs: bool = False) -> list[tuple[Tensor, Tensor]]:
        return [self.forward(example, remove_refs=remove_refs) for example in examples]


def _configure(seed: int, threads: int) -> None:
    random.seed(seed)
    np.random.seed(seed)
    torch.manual_seed(seed)
    torch.set_num_threads(max(1, threads))
    torch.use_deterministic_algorithms(True)


def _policy_weight(example: dict[str, Any]) -> float:
    return 1.0 / float(example["substep_count"])


def _episode_weights(examples: list[dict[str, Any]]) -> dict[str, float]:
    counts: dict[str, int] = {}
    for example in examples:
        counts[example["episode"]] = counts.get(example["episode"], 0) + 1
    return {episode: 1.0 / count for episode, count in counts.items()}


def _weighted_mean(values: list[float], weights: list[float]) -> float:
    denominator = sum(weights)
    return float(sum(value * weight for value, weight in zip(values, weights)) / max(denominator, 1e-12))


def _losses(model: StructuredAdapter, policy: list[dict[str, Any]], value: list[dict[str, Any]]) -> tuple[Tensor, Tensor]:
    policy_terms: list[Tensor] = []
    policy_weights: list[float] = []
    for example in policy:
        logits, _ = model(example)
        policy_terms.append(-torch.log_softmax(logits, dim=0)[example["selected_index"]])
        policy_weights.append(_policy_weight(example))
    value_episode_weights = _episode_weights(value)
    value_terms: list[Tensor] = []
    value_weights: list[float] = []
    for example in value:
        _, prediction = model(example)
        value_terms.append((prediction - example["terminal_reward"]) ** 2)
        value_weights.append(value_episode_weights[example["episode"]])
    policy_loss = torch.stack(policy_terms).mul(torch.tensor(policy_weights)).sum() / sum(policy_weights)
    value_loss = torch.stack(value_terms).mul(torch.tensor(value_weights)).sum() / sum(value_weights)
    return policy_loss, value_loss


def _batch_loss(model: StructuredAdapter, policy: list[dict[str, Any]], value: list[dict[str, Any]]) -> tuple[Tensor, Tensor]:
    policy_terms: list[Tensor] = []
    policy_weights: list[float] = []
    for example in policy:
        logits, _ = model(example)
        policy_terms.append(-torch.log_softmax(logits, dim=0)[example["selected_index"]])
        policy_weights.append(_policy_weight(example))
    if policy_terms:
        policy_loss = torch.stack(policy_terms).mul(torch.tensor(policy_weights)).sum() / sum(policy_weights)
    else:
        policy_loss = torch.zeros((), requires_grad=True)
    value_episode_weights = _episode_weights(value) if value else {}
    value_terms: list[Tensor] = []
    value_weights: list[float] = []
    for example in value:
        _, prediction = model(example)
        value_terms.append((prediction - example["terminal_reward"]) ** 2)
        value_weights.append(value_episode_weights[example["episode"]])
    if value_terms:
        value_loss = torch.stack(value_terms).mul(torch.tensor(value_weights)).sum() / sum(value_weights)
    else:
        value_loss = torch.zeros((), requires_grad=True)
    return policy_loss, value_loss


def _metric_sums(model: StructuredAdapter, examples: list[dict[str, Any]], kind: str) -> dict[str, Any]:
    model.eval()
    if kind == "policy":
        records: list[tuple[int, float, float, bool, bool, float]] = []
        with torch.no_grad():
            for example in examples:
                candidate, _ = model(example)
                parent = example["old_logits"]
                label = example["selected_index"]
                parent_nll = float(-torch.log_softmax(parent, dim=0)[label])
                candidate_nll = float(-torch.log_softmax(candidate, dim=0)[label])
                records.append((example["acting_seat"], parent_nll, candidate_nll, int(parent.argmax()) == label, int(candidate.argmax()) == label, _policy_weight(example), example["decision_kind"]))
        return {"records": records}
    episode_weights = _episode_weights(examples)
    records = []
    with torch.no_grad():
        for example in examples:
            _, candidate = model(example)
            parent = example["old_value"]
            target = example["terminal_reward"]
            records.append((example["candidate_seat"], float((parent - target) ** 2), float((candidate - target) ** 2), episode_weights[example["episode"]]))
    return {"records": records}


def _summarize_policy(records: list[tuple[int, float, float, bool, bool, float, str]]) -> dict[str, Any]:
    total_weight = sum(record[5] for record in records)
    result: dict[str, Any] = {"parent_nll": 0.0, "candidate_nll": 0.0, "parent_top1": 0.0, "candidate_top1": 0.0, "weight": total_weight, "count": len(records), "by_acting_seat": {}, "by_decision_kind": {}}
    for seat in (0, 1):
        subset = [record for record in records if record[0] == seat]
        weight = sum(record[5] for record in subset)
        result["by_acting_seat"][str(seat)] = {
            "parent_nll": sum(record[1] * record[5] for record in subset) / max(weight, 1e-12),
            "candidate_nll": sum(record[2] * record[5] for record in subset) / max(weight, 1e-12),
            "parent_top1": sum(float(record[3]) * record[5] for record in subset) / max(weight, 1e-12),
            "candidate_top1": sum(float(record[4]) * record[5] for record in subset) / max(weight, 1e-12),
            "weight": weight,
            "count": len(subset),
        }
    kinds = sorted({record[6] for record in records})
    for kind in kinds:
        subset = [record for record in records if record[6] == kind]
        weight = sum(record[5] for record in subset)
        parent = sum(record[1] * record[5] for record in subset) / max(weight, 1e-12)
        candidate = sum(record[2] * record[5] for record in subset) / max(weight, 1e-12)
        result["by_decision_kind"][kind] = {
            "parent_nll": parent,
            "candidate_nll": candidate,
            "relative_improvement": (parent - candidate) / max(parent, 1e-12),
            "parent_top1": sum(float(record[3]) * record[5] for record in subset) / max(weight, 1e-12),
            "candidate_top1": sum(float(record[4]) * record[5] for record in subset) / max(weight, 1e-12),
            "weight": weight,
            "count": len(subset),
        }
    result["parent_nll"] = sum(record[1] * record[5] for record in records) / max(total_weight, 1e-12)
    result["candidate_nll"] = sum(record[2] * record[5] for record in records) / max(total_weight, 1e-12)
    result["parent_top1"] = sum(float(record[3]) * record[5] for record in records) / max(total_weight, 1e-12)
    result["candidate_top1"] = sum(float(record[4]) * record[5] for record in records) / max(total_weight, 1e-12)
    result["relative_improvement"] = (result["parent_nll"] - result["candidate_nll"]) / max(result["parent_nll"], 1e-12)
    return result


def _summarize_value(records: list[tuple[int, float, float, float]]) -> dict[str, Any]:
    weight = sum(record[3] for record in records)
    result: dict[str, Any] = {"parent_mse": 0.0, "candidate_mse": 0.0, "weight": weight, "count": len(records), "by_candidate_seat": {}}
    for seat in (0, 1):
        subset = [record for record in records if record[0] == seat]
        seat_weight = sum(record[3] for record in subset)
        parent = sum(record[1] * record[3] for record in subset) / max(seat_weight, 1e-12)
        candidate = sum(record[2] * record[3] for record in subset) / max(seat_weight, 1e-12)
        result["by_candidate_seat"][str(seat)] = {"parent_mse": parent, "candidate_mse": candidate, "weight": seat_weight, "count": len(subset), "relative_improvement": (parent - candidate) / max(parent, 1e-12)}
    result["parent_mse"] = sum(record[1] * record[3] for record in records) / max(weight, 1e-12)
    result["candidate_mse"] = sum(record[2] * record[3] for record in records) / max(weight, 1e-12)
    result["relative_improvement"] = (result["parent_mse"] - result["candidate_mse"]) / max(result["parent_mse"], 1e-12)
    return result


def _permuted(example: dict[str, Any], generator: torch.Generator) -> dict[str, Any]:
    result = dict(example)
    n_objects = int(example["object_features"].shape[0])
    permutation = torch.randperm(n_objects, generator=generator)
    inverse = torch.empty_like(permutation)
    inverse[permutation] = torch.arange(n_objects)
    result["object_features"] = example["object_features"][permutation]
    result["object_card_ids"] = example["object_card_ids"][permutation]
    result["object_groups"] = example["object_groups"][permutation]
    result["edge_src"] = inverse[example["edge_src"].long()]
    result["edge_tgt"] = inverse[example["edge_tgt"].long()]
    result["ref_node_indices"] = inverse[example["ref_node_indices"].long()]
    return result


def _without_digest(example: dict[str, Any]) -> dict[str, Any]:
    result = dict(example)
    state = example["state"].clone()
    actions = example["action_features"].clone()
    state[STATE_DIM - 96 :] = 0.0
    actions[:, ACTION_DIM - 96 :] = 0.0
    result["state"] = state
    result["action_features"] = actions
    return result


def _ablation_metrics(model: StructuredAdapter, policy: list[dict[str, Any]], value: list[dict[str, Any]]) -> dict[str, Any]:
    policy_examples = [_without_digest(example) for example in policy]
    value_examples = [_without_digest(example) for example in value]
    return {
        "policy": _summarize_policy(_metric_sums(model, policy_examples, "policy")["records"]),
        "value": _summarize_value(_metric_sums(model, value_examples, "value")["records"]),
        "zeroed_state_slice": [STATE_DIM - 96, STATE_DIM],
        "zeroed_action_slice": [ACTION_DIM - 96, ACTION_DIM],
        "acceptance_gate": False,
    }


def _diagnostics(model: StructuredAdapter, examples: list[dict[str, Any]], seed: int) -> dict[str, Any]:
    generator = torch.Generator(device="cpu").manual_seed(seed)
    permutation_delta = 0.0
    permutation_count = 0
    ref_eligible = 0
    ref_affected = 0
    ref_max_delta = 0.0
    model.eval()
    with torch.no_grad():
        for example in examples:
            candidate_logits, candidate_value = model(example)
            permuted = _permuted(example, generator)
            perm_logits, perm_value = model(permuted)
            permutation_delta = max(permutation_delta, float((candidate_logits - perm_logits).abs().max()), float((candidate_value - perm_value).abs()))
            permutation_count += 1
            if example["action_ref_features"].shape[0]:
                ref_eligible += 1
                no_refs_logits, no_refs_value = model(example, remove_refs=True)
                delta = max(float((candidate_logits - no_refs_logits).abs().max()), float((candidate_value - no_refs_value).abs()))
                ref_max_delta = max(ref_max_delta, delta)
                if delta > 1e-4:
                    ref_affected += 1
    return {
        "permutation_max_delta": permutation_delta,
        "permutation_examples": permutation_count,
        "ref_removal_eligible": ref_eligible,
        "ref_removal_affected": ref_affected,
        "ref_removal_affected_rate": ref_affected / max(ref_eligible, 1),
        "ref_removal_max_delta": ref_max_delta,
    }


def run_fold(cache_path: Path, output_path: Path, fold: int, args: argparse.Namespace) -> dict[str, Any]:
    started = time.perf_counter()
    cache = torch.load(cache_path, map_location="cpu", weights_only=False)
    if cache.get("version") != SCRIPT_VERSION:
        _fail("cache version mismatch")
    policy = cache["policy"]
    value = cache["value"]
    all_examples = policy + value
    card_vocab, group_vocab = _model_vocab(all_examples)
    policy_train = [example for example in policy if example["pair_index"] % FOLDS != fold]
    policy_test = [example for example in policy if example["pair_index"] % FOLDS == fold]
    value_train = [example for example in value if example["pair_index"] % FOLDS != fold]
    value_test = [example for example in value if example["pair_index"] % FOLDS == fold]
    if not policy_train or not policy_test or not value_train or not value_test:
        _fail(f"fold {fold} lacks train or heldout examples")
    _configure(args.seed + fold, args.threads)
    model = StructuredAdapter(card_vocab, group_vocab, args.dim)
    optimizer = torch.optim.AdamW(model.parameters(), lr=args.lr, weight_decay=args.weight_decay)
    rng = random.Random(args.seed + fold)
    steps_per_epoch = max(math.ceil(len(policy_train) / args.batch_size), math.ceil(len(value_train) / args.batch_size))
    train_history: list[dict[str, float]] = []
    for epoch in range(args.epochs):
        model.train()
        policy_order = list(range(len(policy_train)))
        value_order = list(range(len(value_train)))
        rng.shuffle(policy_order)
        rng.shuffle(value_order)
        policy_loss_total = 0.0
        value_loss_total = 0.0
        for step in range(steps_per_epoch):
            policy_batch = [policy_train[policy_order[(step * args.batch_size + i) % len(policy_order)]] for i in range(min(args.batch_size, len(policy_order)))]
            value_batch = [value_train[value_order[(step * args.batch_size + i) % len(value_order)]] for i in range(min(args.batch_size, len(value_order)))]
            policy_loss, value_loss = _batch_loss(model, policy_batch, value_batch)
            loss = policy_loss + args.value_coefficient * value_loss
            optimizer.zero_grad(set_to_none=True)
            loss.backward()
            torch.nn.utils.clip_grad_norm_(model.parameters(), 5.0)
            optimizer.step()
            policy_loss_total += float(policy_loss.detach())
            value_loss_total += float(value_loss.detach())
        train_history.append({"epoch": epoch + 1, "policy_nll": policy_loss_total / steps_per_epoch, "value_mse": value_loss_total / steps_per_epoch})
    policy_metrics = _summarize_policy(_metric_sums(model, policy_test, "policy")["records"])
    value_metrics = _summarize_value(_metric_sums(model, value_test, "value")["records"])
    diagnostics = _diagnostics(model, policy_test + value_test, args.seed + fold + 1000)
    no_digest = _ablation_metrics(model, policy_test, value_test)
    result = {
        "schema": SCRIPT_VERSION,
        "fold": fold,
        "config": {"dim": args.dim, "epochs": args.epochs, "batch_size": args.batch_size, "lr": args.lr, "weight_decay": args.weight_decay, "value_coefficient": args.value_coefficient, "seed": args.seed, "threads": args.threads},
        "counts": {"policy_train": len(policy_train), "policy_heldout": len(policy_test), "value_train": len(value_train), "value_heldout": len(value_test), "train_pairs": sorted({e["pair_index"] for e in policy_train}), "heldout_pairs": sorted({e["pair_index"] for e in policy_test})},
        "train_metrics": {"final": train_history[-1], "history": train_history},
        "heldout": {"policy": policy_metrics, "value": value_metrics},
        "diagnostics": diagnostics,
        "no_digest_ablation": no_digest,
        "runtime_seconds": time.perf_counter() - started,
        "raw": {"policy_records": policy_metrics["count"], "policy_weight": policy_metrics["weight"], "value_records": value_metrics["count"], "value_weight": value_metrics["weight"]},
    }
    _write_json(output_path, result)
    return result


def _write_json(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8", newline="\n") as handle:
        json.dump(value, handle, sort_keys=True, indent=2, allow_nan=False)
        handle.write("\n")


def aggregate(paths: list[Path], output_path: Path) -> dict[str, Any]:
    if len(paths) != FOLDS:
        _fail("aggregate requires exactly four fold JSON files")
    results = [json.loads(path.read_text(encoding="utf-8")) for path in paths]
    if sorted(result.get("fold") for result in results) != list(range(FOLDS)):
        _fail("aggregate inputs must contain folds 0, 1, 2, and 3")

    policy_weight = sum(float(result["heldout"]["policy"]["weight"]) for result in results)
    value_weight = sum(float(result["heldout"]["value"]["weight"]) for result in results)
    parent_policy = sum(float(result["heldout"]["policy"]["parent_nll"]) * float(result["heldout"]["policy"]["weight"]) for result in results) / policy_weight
    candidate_policy = sum(float(result["heldout"]["policy"]["candidate_nll"]) * float(result["heldout"]["policy"]["weight"]) for result in results) / policy_weight
    parent_top1 = sum(float(result["heldout"]["policy"]["parent_top1"]) * float(result["heldout"]["policy"]["weight"]) for result in results) / policy_weight
    candidate_top1 = sum(float(result["heldout"]["policy"]["candidate_top1"]) * float(result["heldout"]["policy"]["weight"]) for result in results) / policy_weight
    parent_value = sum(float(result["heldout"]["value"]["parent_mse"]) * float(result["heldout"]["value"]["weight"]) for result in results) / value_weight
    candidate_value = sum(float(result["heldout"]["value"]["candidate_mse"]) * float(result["heldout"]["value"]["weight"]) for result in results) / value_weight
    policy_by_seat: dict[str, Any] = {}
    value_by_seat: dict[str, Any] = {}
    policy_by_kind: dict[str, Any] = {}
    for seat in ("0", "1"):
        p_weight = sum(float(result["heldout"]["policy"]["by_acting_seat"][seat]["weight"]) for result in results)
        v_weight = sum(float(result["heldout"]["value"]["by_candidate_seat"][seat]["weight"]) for result in results)
        p_parent = sum(float(result["heldout"]["policy"]["by_acting_seat"][seat]["parent_nll"]) * float(result["heldout"]["policy"]["by_acting_seat"][seat]["weight"]) for result in results) / max(p_weight, 1e-12)
        p_candidate = sum(float(result["heldout"]["policy"]["by_acting_seat"][seat]["candidate_nll"]) * float(result["heldout"]["policy"]["by_acting_seat"][seat]["weight"]) for result in results) / max(p_weight, 1e-12)
        p_parent_top1 = sum(float(result["heldout"]["policy"]["by_acting_seat"][seat]["parent_top1"]) * float(result["heldout"]["policy"]["by_acting_seat"][seat]["weight"]) for result in results) / max(p_weight, 1e-12)
        p_candidate_top1 = sum(float(result["heldout"]["policy"]["by_acting_seat"][seat]["candidate_top1"]) * float(result["heldout"]["policy"]["by_acting_seat"][seat]["weight"]) for result in results) / max(p_weight, 1e-12)
        v_parent = sum(float(result["heldout"]["value"]["by_candidate_seat"][seat]["parent_mse"]) * float(result["heldout"]["value"]["by_candidate_seat"][seat]["weight"]) for result in results) / max(v_weight, 1e-12)
        v_candidate = sum(float(result["heldout"]["value"]["by_candidate_seat"][seat]["candidate_mse"]) * float(result["heldout"]["value"]["by_candidate_seat"][seat]["weight"]) for result in results) / max(v_weight, 1e-12)
        policy_by_seat[seat] = {"parent_nll": p_parent, "candidate_nll": p_candidate, "relative_improvement": (p_parent - p_candidate) / max(p_parent, 1e-12), "parent_top1": p_parent_top1, "candidate_top1": p_candidate_top1, "top1_delta": p_candidate_top1 - p_parent_top1, "weight": p_weight}
        value_by_seat[seat] = {"parent_mse": v_parent, "candidate_mse": v_candidate, "relative_improvement": (v_parent - v_candidate) / max(v_parent, 1e-12), "weight": v_weight}
    kinds = sorted({kind for result in results for kind in result["heldout"]["policy"]["by_decision_kind"]})
    for kind in kinds:
        kind_weight = sum(float(result["heldout"]["policy"]["by_decision_kind"].get(kind, {}).get("weight", 0.0)) for result in results)
        kind_parent = sum(float(result["heldout"]["policy"]["by_decision_kind"].get(kind, {}).get("parent_nll", 0.0)) * float(result["heldout"]["policy"]["by_decision_kind"].get(kind, {}).get("weight", 0.0)) for result in results) / max(kind_weight, 1e-12)
        kind_candidate = sum(float(result["heldout"]["policy"]["by_decision_kind"].get(kind, {}).get("candidate_nll", 0.0)) * float(result["heldout"]["policy"]["by_decision_kind"].get(kind, {}).get("weight", 0.0)) for result in results) / max(kind_weight, 1e-12)
        policy_by_kind[kind] = {"parent_nll": kind_parent, "candidate_nll": kind_candidate, "relative_improvement": (kind_parent - kind_candidate) / max(kind_parent, 1e-12), "weight": kind_weight}
    policy_improvement = (parent_policy - candidate_policy) / max(parent_policy, 1e-12)
    value_improvement = (parent_value - candidate_value) / max(parent_value, 1e-12)
    perm_delta = max(float(result["diagnostics"]["permutation_max_delta"]) for result in results)
    eligible = sum(int(result["diagnostics"]["ref_removal_eligible"]) for result in results)
    affected = sum(int(result["diagnostics"]["ref_removal_affected"]) for result in results)
    ref_rate = affected / max(eligible, 1)
    gates = {
        "policy_nll_relative_improvement_ge_5pct": policy_improvement >= 0.05,
        "no_acting_seat_policy_nll_regression": all(policy_by_seat[seat]["relative_improvement"] >= 0.0 for seat in ("0", "1")),
        "policy_top1_noninferior_minus_0_5pp": candidate_top1 - parent_top1 >= -0.005,
        "value_mse_relative_improvement_ge_5pct": value_improvement >= 0.05,
        "no_candidate_seat_value_regression_over_2pct": all(value_by_seat[seat]["relative_improvement"] >= -0.02 for seat in ("0", "1")),
        "permutation_max_delta_le_1e-5": perm_delta <= 1e-5,
        "ref_removal_affected_rate_ge_20pct": ref_rate >= 0.20,
    }
    result = {
        "schema": SCRIPT_VERSION + ".aggregate",
        "fold_files": [str(path) for path in paths],
        "heldout": {"policy": {"parent_nll": parent_policy, "candidate_nll": candidate_policy, "relative_improvement": policy_improvement, "parent_top1": parent_top1, "candidate_top1": candidate_top1, "top1_delta": candidate_top1 - parent_top1, "by_acting_seat": policy_by_seat, "by_decision_kind": policy_by_kind, "weight": policy_weight}, "value": {"parent_mse": parent_value, "candidate_mse": candidate_value, "relative_improvement": value_improvement, "by_candidate_seat": value_by_seat, "weight": value_weight}},
        "diagnostics": {"permutation_max_delta": perm_delta, "ref_removal_eligible": eligible, "ref_removal_affected": affected, "ref_removal_affected_rate": ref_rate},
        "gates": gates,
        "pass": all(gates.values()),
    }
    _write_json(output_path, result)
    return result


def _self_example(index: int, rng: np.random.Generator) -> dict[str, Any]:
    n_objects = 2 + index % 4
    n_actions = 2 + index % 3
    n_edges = max(0, n_objects - 1 + index % 2)
    n_refs = 1 + index % 3
    objects = rng.normal(size=(n_objects, OBJECT_DIM)).astype(np.float32)
    groups = np.asarray([(i + index) % 3 for i in range(n_objects)], dtype=np.int64)
    edges = rng.normal(size=(n_edges, EDGE_DIM)).astype(np.float32)
    edge_src = np.asarray([i % n_objects for i in range(n_edges)], dtype=np.int64)
    edge_tgt = np.asarray([(i + 1) % n_objects for i in range(n_edges)], dtype=np.int64)
    refs = rng.normal(size=(n_refs, REF_DIM)).astype(np.float32)
    ref_actions = np.asarray([i % n_actions for i in range(n_refs)], dtype=np.int64)
    ref_nodes = np.asarray([i % n_objects for i in range(n_refs)], dtype=np.int64)
    return {
        "state": torch.from_numpy(rng.normal(size=STATE_DIM).astype(np.float32)),
        "object_features": torch.from_numpy(objects),
        "object_card_ids": torch.arange(1, n_objects + 1, dtype=torch.int64),
        "object_groups": torch.from_numpy(groups),
        "edge_features": torch.from_numpy(edges),
        "edge_src": torch.from_numpy(edge_src),
        "edge_tgt": torch.from_numpy(edge_tgt),
        "action_features": torch.from_numpy(rng.normal(size=(n_actions, ACTION_DIM)).astype(np.float32)),
        "action_ref_features": torch.from_numpy(refs),
        "ref_action_indices": torch.from_numpy(ref_actions),
        "ref_node_indices": torch.from_numpy(ref_nodes),
        "old_logits": torch.from_numpy(rng.normal(size=n_actions).astype(np.float32)),
        "old_value": torch.tensor(float(index) / 4.0, dtype=torch.float32),
        "selected_index": index % n_actions,
        "pair_index": index,
        "acting_seat": index % 2,
        "candidate_seat": (index + 1) % 2,
        "episode": str(index),
        "substep_count": 1 + index % 3,
        "substep_index": index % (1 + index % 3),
        "physical_group": index,
        "decision_kind": "synthetic",
        "terminal_reward": float(-1 if index % 3 == 0 else 1),
    }


def self_test(args: argparse.Namespace) -> dict[str, Any]:
    started = time.perf_counter()
    _configure(args.seed, args.threads)
    rng = np.random.default_rng(args.seed)
    examples = [_self_example(index, rng) for index in range(7)]
    model = StructuredAdapter(32, 8, args.dim)
    model.force_nonzero_residual(args.seed)
    with torch.no_grad():
        individual = [model(example) for example in examples]
        batched = model.forward_batch(examples)
    batching_delta = max(float((a[0] - b[0]).abs().max()) + float((a[1] - b[1]).abs()) for a, b in zip(individual, batched))
    permutation = _diagnostics(model, examples, args.seed + 1)
    optimizer = torch.optim.AdamW(model.parameters(), lr=1e-3)
    before = [parameter.detach().clone() for parameter in model.parameters()]
    policy_loss, value_loss = _batch_loss(model, examples[:3], examples[3:])
    optimizer.zero_grad(set_to_none=True)
    (policy_loss + value_loss).backward()
    optimizer.step()
    parameter_delta = max(float((after.detach() - before[index]).abs().max()) for index, after in enumerate(model.parameters()))
    checks = {"variable_sizes": True, "batching_max_delta_le_1e-6": batching_delta <= 1e-6, "permutation_max_delta_le_1e-5": permutation["permutation_max_delta"] <= 1e-5, "ref_removal_meaningful": permutation["ref_removal_affected_rate"] >= 0.20, "optimizer_step_changed_parameter": parameter_delta > 0.0}
    result = {"schema": SCRIPT_VERSION + ".self_test", "pass": all(checks.values()), "checks": checks, "batching_max_delta": batching_delta, "diagnostics": permutation, "optimizer_parameter_max_delta": parameter_delta, "runtime_seconds": time.perf_counter() - started, "config": {"dim": args.dim, "seed": args.seed, "threads": args.threads}}
    if not result["pass"]:
        _fail("self-test failed: " + json.dumps(checks, sort_keys=True))
    return result


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    modes = parser.add_mutually_exclusive_group(required=True)
    modes.add_argument("--prepare-cache", action="store_true")
    modes.add_argument("--self-test", action="store_true")
    modes.add_argument("--fold", type=int, choices=range(FOLDS))
    modes.add_argument("--aggregate", action="store_true")
    parser.add_argument("--teacher-jsonl", type=Path)
    parser.add_argument("--outcome-jsonl", type=Path)
    parser.add_argument("--cache", type=Path, default=Path("structured_adapter_cache.pt"))
    parser.add_argument("--output", type=Path, default=Path("structured_adapter_result.json"))
    parser.add_argument("--fold-results", type=Path, nargs="+")
    parser.add_argument("--dim", type=int, default=48)
    parser.add_argument("--epochs", type=int, default=20)
    parser.add_argument("--batch-size", type=int, default=32)
    parser.add_argument("--lr", type=float, default=3e-4)
    parser.add_argument("--weight-decay", type=float, default=1e-4)
    parser.add_argument("--value-coefficient", type=float, default=1.0)
    parser.add_argument("--seed", type=int, default=20260802)
    parser.add_argument("--threads", type=int, default=1)
    parser.add_argument("--teacher-sha256-prefix", default=TEACHER_SHA256_PREFIX)
    parser.add_argument("--outcome-sha256-prefix", default=OUTCOME_SHA256_PREFIX)
    return parser


def main(argv: list[str] | None = None) -> int:
    args = _parser().parse_args(argv)
    if args.dim < 8 or args.epochs < 1 or args.batch_size < 1 or args.lr <= 0 or args.threads < 1:
        _fail("invalid training configuration")
    if args.prepare_cache:
        if args.teacher_jsonl is None or args.outcome_jsonl is None:
            _fail("--prepare-cache requires --teacher-jsonl and --outcome-jsonl")
        result = prepare_cache(args.teacher_jsonl, args.outcome_jsonl, args.cache, args.teacher_sha256_prefix, args.outcome_sha256_prefix)
    elif args.self_test:
        result = self_test(args)
    elif args.fold is not None:
        result = run_fold(args.cache, args.output, args.fold, args)
    else:
        if args.fold_results is None:
            _fail("--aggregate requires --fold-results with four files")
        result = aggregate(args.fold_results, args.output)
    if args.self_test or args.prepare_cache:
        if args.output != Path("structured_adapter_result.json") or args.prepare_cache:
            _write_json(args.output, result)
    print(json.dumps(result, sort_keys=True, allow_nan=False))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (ValueError, OSError, RuntimeError) as exc:
        print(f"error: {exc}", file=sys.stderr)
        raise SystemExit(2)
