#!/usr/bin/env python3
"""Batched recurrent structured actor-critic for the v1 fixed-corpus screen."""

from __future__ import annotations

import math
from dataclasses import dataclass
from typing import Any

import torch
from torch import Tensor, nn


STATE_DIM = 219
OBJECT_DIM = 98
EDGE_DIM = 41
ACTION_DIM = 195
ACTION_EXPLICIT_DIM = 99
REF_DIM = 25
CARD_VOCAB = 136
GROUP_VOCAB = 12
HISTORY_DIM = ACTION_EXPLICIT_DIM + 2 + CARD_VOCAB


@dataclass
class PackedRows:
    state: Tensor
    history: Tensor
    history_mask: Tensor
    objects: Tensor
    object_cards: Tensor
    object_groups: Tensor
    object_mask: Tensor
    edge_features: Tensor
    edge_batch: Tensor
    edge_src: Tensor
    edge_tgt: Tensor
    actions: Tensor
    action_mask: Tensor
    parent_logits: Tensor
    parent_value: Tensor
    selected_index: Tensor
    ref_features: Tensor
    ref_batch: Tensor
    ref_action: Tensor
    ref_node: Tensor

    @property
    def row_count(self) -> int:
        return int(self.state.shape[0])


def _padded_shape(rows: list[dict[str, Any]], key: str) -> int:
    return max(int(row[key].shape[0]) for row in rows)


def pack_rows(
    rows: list[dict[str, Any]],
    device: torch.device,
    *,
    remove_refs: bool = False,
) -> PackedRows:
    if not rows:
        raise ValueError("cannot pack an empty row batch")
    batch = len(rows)
    max_history = max(1, _padded_shape(rows, "history_features"))
    max_objects = _padded_shape(rows, "object_features")
    max_actions = _padded_shape(rows, "action_features")
    if max_objects < 1 or max_actions < 1:
        raise ValueError("every row must contain an object and a legal action")

    state = torch.stack([row["state"] for row in rows])
    history = torch.zeros((batch, max_history, HISTORY_DIM), dtype=torch.float32)
    history_mask = torch.zeros((batch, max_history), dtype=torch.bool)
    objects = torch.zeros((batch, max_objects, OBJECT_DIM), dtype=torch.float32)
    object_cards = torch.zeros((batch, max_objects), dtype=torch.long)
    object_groups = torch.zeros((batch, max_objects), dtype=torch.long)
    object_mask = torch.zeros((batch, max_objects), dtype=torch.bool)
    actions = torch.zeros((batch, max_actions, ACTION_DIM), dtype=torch.float32)
    action_mask = torch.zeros((batch, max_actions), dtype=torch.bool)
    parent_logits = torch.full((batch, max_actions), -1.0e9, dtype=torch.float32)
    parent_value = torch.stack([row["old_value"].reshape(()) for row in rows])
    selected_index = torch.tensor(
        [int(row["selected_index"]) for row in rows], dtype=torch.long
    )

    edge_features: list[Tensor] = []
    edge_batch: list[Tensor] = []
    edge_src: list[Tensor] = []
    edge_tgt: list[Tensor] = []
    ref_features: list[Tensor] = []
    ref_batch: list[Tensor] = []
    ref_action: list[Tensor] = []
    ref_node: list[Tensor] = []
    for index, row in enumerate(rows):
        history_count = int(row["history_features"].shape[0])
        if history_count:
            history[index, :history_count] = row["history_features"]
            history_mask[index, :history_count] = True
        object_count = int(row["object_features"].shape[0])
        objects[index, :object_count] = row["object_features"]
        object_cards[index, :object_count] = row["object_card_ids"].long()
        object_groups[index, :object_count] = row["object_groups"].long()
        object_mask[index, :object_count] = True
        action_count = int(row["action_features"].shape[0])
        actions[index, :action_count] = row["action_features"]
        action_mask[index, :action_count] = True
        parent_logits[index, :action_count] = row["old_logits"]

        edge_count = int(row["edge_features"].shape[0])
        if edge_count:
            edge_features.append(row["edge_features"])
            edge_batch.append(torch.full((edge_count,), index, dtype=torch.long))
            edge_src.append(row["edge_src"].long())
            edge_tgt.append(row["edge_tgt"].long())
        if not remove_refs:
            ref_count = int(row["action_ref_features"].shape[0])
            if ref_count:
                ref_features.append(row["action_ref_features"])
                ref_batch.append(torch.full((ref_count,), index, dtype=torch.long))
                ref_action.append(row["ref_action_indices"].long())
                ref_node.append(row["ref_node_indices"].long())

    empty_edge_f = torch.zeros((0, EDGE_DIM), dtype=torch.float32)
    empty_ref_f = torch.zeros((0, REF_DIM), dtype=torch.float32)
    empty_i = torch.zeros((0,), dtype=torch.long)
    values = PackedRows(
        state=state,
        history=history,
        history_mask=history_mask,
        objects=objects,
        object_cards=object_cards,
        object_groups=object_groups,
        object_mask=object_mask,
        edge_features=torch.cat(edge_features) if edge_features else empty_edge_f,
        edge_batch=torch.cat(edge_batch) if edge_batch else empty_i,
        edge_src=torch.cat(edge_src) if edge_src else empty_i,
        edge_tgt=torch.cat(edge_tgt) if edge_tgt else empty_i,
        actions=actions,
        action_mask=action_mask,
        parent_logits=parent_logits,
        parent_value=parent_value,
        selected_index=selected_index,
        ref_features=torch.cat(ref_features) if ref_features else empty_ref_f,
        ref_batch=torch.cat(ref_batch) if ref_batch else empty_i,
        ref_action=torch.cat(ref_action) if ref_action else empty_i,
        ref_node=torch.cat(ref_node) if ref_node else empty_i,
    )
    def move(value: Tensor) -> Tensor:
        if device.type == "cuda":
            value = value.pin_memory()
        return value.to(device, non_blocking=True)

    return PackedRows(**{name: move(value) for name, value in vars(values).items()})


class RecurrentStructuredActorCritic(nn.Module):
    def __init__(self, dim: int = 128) -> None:
        super().__init__()
        if dim % 4:
            raise ValueError("model width must be divisible by four")
        self.dim = dim
        card_dim = dim // 2
        group_dim = dim // 4
        self.state = nn.Sequential(
            nn.Linear(STATE_DIM + 1, dim),
            nn.LayerNorm(dim),
            nn.GELU(),
            nn.Linear(dim, dim),
            nn.GELU(),
        )
        self.history = nn.GRU(
            HISTORY_DIM, dim, num_layers=2, batch_first=True
        )
        self.history_key = nn.Linear(dim, dim, bias=False)
        self.history_query = nn.Linear(dim, dim, bias=False)
        self.history_mix = nn.Sequential(
            nn.Linear(dim * 2, dim), nn.GELU(), nn.LayerNorm(dim)
        )
        self.card = nn.Embedding(CARD_VOCAB, card_dim)
        self.group = nn.Embedding(GROUP_VOCAB, group_dim)
        self.object = nn.Sequential(
            nn.Linear(OBJECT_DIM + card_dim + group_dim, dim),
            nn.LayerNorm(dim),
            nn.GELU(),
        )
        self.edge_rounds = nn.ModuleList(
            [
                nn.Sequential(
                    nn.Linear(dim + EDGE_DIM, dim), nn.GELU(), nn.Linear(dim, dim)
                )
                for _ in range(2)
            ]
        )
        self.object_norms = nn.ModuleList([nn.LayerNorm(dim) for _ in range(2)])
        self.group_mix = nn.Linear(dim, dim, bias=False)
        self.action = nn.Sequential(
            nn.Linear(ACTION_DIM + 1, dim), nn.LayerNorm(dim), nn.GELU()
        )
        self.ref = nn.Sequential(
            nn.Linear(REF_DIM + dim, dim), nn.LayerNorm(dim), nn.GELU()
        )
        self.action_query = nn.Linear(dim * 2, dim)
        self.cross_attention = nn.MultiheadAttention(
            dim, num_heads=4, batch_first=True
        )
        self.combine = nn.Sequential(
            nn.Linear(dim * 5, dim * 2),
            nn.GELU(),
            nn.LayerNorm(dim * 2),
            nn.Linear(dim * 2, dim),
            nn.GELU(),
        )
        self.policy_head = nn.Linear(dim, 1)
        self.value_head = nn.Sequential(nn.Linear(dim * 3, dim), nn.GELU(), nn.Linear(dim, 1))

    def _history_context(self, batch: PackedRows, state_h: Tensor) -> Tensor:
        history_h, _ = self.history(batch.history)
        scores = (
            self.history_key(history_h)
            * self.history_query(state_h).unsqueeze(1)
        ).sum(dim=-1) / math.sqrt(float(self.dim))
        safe_mask = batch.history_mask.clone()
        empty = ~safe_mask.any(dim=1)
        safe_mask[empty, 0] = True
        scores = scores.masked_fill(~safe_mask, -1.0e9)
        weights = torch.softmax(scores, dim=1)
        context = (weights.unsqueeze(-1) * history_h).sum(dim=1)
        return context * (~empty).to(context.dtype).unsqueeze(1)

    def _graph_objects(self, batch: PackedRows) -> tuple[Tensor, Tensor]:
        cards = batch.object_cards.remainder(CARD_VOCAB)
        groups = batch.object_groups.remainder(GROUP_VOCAB)
        object_h = self.object(
            torch.cat(
                (batch.objects, self.card(cards), self.group(groups)), dim=-1
            )
        )
        object_h = object_h * batch.object_mask.unsqueeze(-1)
        max_objects = object_h.shape[1]
        for edge_layer, norm in zip(self.edge_rounds, self.object_norms):
            flat = object_h.reshape(-1, self.dim)
            aggregate = torch.zeros_like(flat)
            degree = torch.zeros((flat.shape[0], 1), device=flat.device)
            if batch.edge_features.shape[0]:
                src = batch.edge_batch * max_objects + batch.edge_src
                tgt = batch.edge_batch * max_objects + batch.edge_tgt
                messages = edge_layer(
                    torch.cat((flat.index_select(0, src), batch.edge_features), dim=1)
                )
                aggregate.index_add_(0, tgt, messages)
                degree.index_add_(
                    0,
                    tgt,
                    torch.ones((tgt.shape[0], 1), device=flat.device),
                )
            object_h = norm(
                object_h + (aggregate / (1.0 + degree)).reshape_as(object_h)
            )
            object_h = object_h * batch.object_mask.unsqueeze(-1)

        flat = object_h.reshape(-1, self.dim)
        batch_index = torch.arange(
            object_h.shape[0], device=flat.device
        ).unsqueeze(1).expand_as(groups)
        group_index = (batch_index * GROUP_VOCAB + groups).reshape(-1)
        valid = batch.object_mask.reshape(-1)
        pooled = torch.zeros(
            (object_h.shape[0] * GROUP_VOCAB, self.dim), device=flat.device
        )
        counts = torch.zeros(
            (object_h.shape[0] * GROUP_VOCAB, 1), device=flat.device
        )
        pooled.index_add_(0, group_index[valid], flat[valid])
        counts.index_add_(
            0,
            group_index[valid],
            torch.ones((int(valid.sum()), 1), device=flat.device),
        )
        pooled = pooled / counts.clamp_min(1.0)
        group_context = pooled.index_select(0, group_index).reshape_as(object_h)
        object_h = (
            object_h + self.group_mix(group_context)
        ) * batch.object_mask.unsqueeze(-1)
        return object_h, pooled.reshape(object_h.shape[0], GROUP_VOCAB, self.dim)

    def forward(
        self, batch: PackedRows, *, remove_digest: bool = False
    ) -> tuple[Tensor, Tensor]:
        digest_value = (
            torch.zeros_like(batch.parent_value)
            if remove_digest
            else batch.parent_value
        )
        state_h = self.state(torch.cat((batch.state, digest_value.unsqueeze(1)), dim=1))
        history_h = self._history_context(batch, state_h)
        state_h = self.history_mix(torch.cat((state_h, history_h), dim=1))
        object_h, pooled_groups = self._graph_objects(batch)

        parent_log_probability = torch.log_softmax(batch.parent_logits, dim=1)
        if remove_digest:
            parent_log_probability = torch.zeros_like(parent_log_probability)
        parent_log_probability = parent_log_probability.masked_fill(
            ~batch.action_mask, 0.0
        )
        action_h = self.action(
            torch.cat((batch.actions, parent_log_probability.unsqueeze(-1)), dim=-1)
        )
        action_h = action_h * batch.action_mask.unsqueeze(-1)
        ref_aggregate = torch.zeros_like(action_h).reshape(-1, self.dim)
        ref_counts = torch.zeros(
            (ref_aggregate.shape[0], 1), device=ref_aggregate.device
        )
        if batch.ref_features.shape[0]:
            max_actions = action_h.shape[1]
            max_objects = object_h.shape[1]
            action_index = batch.ref_batch * max_actions + batch.ref_action
            object_index = batch.ref_batch * max_objects + batch.ref_node
            ref_h = self.ref(
                torch.cat(
                    (
                        batch.ref_features,
                        object_h.reshape(-1, self.dim).index_select(0, object_index),
                    ),
                    dim=1,
                )
            )
            ref_aggregate.index_add_(0, action_index, ref_h)
            ref_counts.index_add_(
                0,
                action_index,
                torch.ones((action_index.shape[0], 1), device=action_h.device),
            )
        ref_aggregate = (ref_aggregate / ref_counts.clamp_min(1.0)).reshape_as(
            action_h
        )
        query = self.action_query(torch.cat((action_h, ref_aggregate), dim=-1))
        context, _ = self.cross_attention(
            query,
            object_h,
            object_h,
            key_padding_mask=~batch.object_mask,
            need_weights=False,
        )
        state_expanded = state_h.unsqueeze(1).expand_as(action_h)
        joint = self.combine(
            torch.cat(
                (
                    action_h,
                    ref_aggregate,
                    context,
                    state_expanded,
                    action_h * context,
                ),
                dim=-1,
            )
        )
        logits = self.policy_head(joint).squeeze(-1).masked_fill(
            ~batch.action_mask, -1.0e9
        )
        object_count = batch.object_mask.sum(dim=1, keepdim=True).clamp_min(1)
        object_mean = object_h.sum(dim=1) / object_count
        action_count = batch.action_mask.sum(dim=1, keepdim=True).clamp_min(1)
        action_mean = (joint * batch.action_mask.unsqueeze(-1)).sum(dim=1) / action_count
        group_mean = pooled_groups.mean(dim=1)
        value = torch.tanh(
            self.value_head(
                torch.cat((state_h, object_mean + group_mean, action_mean), dim=1)
            ).squeeze(1)
        )
        return logits, value
