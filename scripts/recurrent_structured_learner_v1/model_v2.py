#!/usr/bin/env python3
"""Hard trust-projected recurrent actor-critic for the v2 fresh screen."""

from __future__ import annotations

import torch
from torch import Tensor

from model_v1 import PackedRows, RecurrentStructuredActorCritic


JOINT_LOG_RATIO_BUDGET = 0.49
BISECTION_STEPS = 16


def project_logits(
    parent_logits: Tensor,
    raw_logits: Tensor,
    action_mask: Tensor,
    substep_count: Tensor,
) -> Tensor:
    """Keep every possible physical-decision joint log ratio within 0.49."""
    parent_log_probability = torch.log_softmax(parent_logits, dim=1)
    delta = raw_logits - parent_logits
    low = torch.zeros((raw_logits.shape[0], 1), device=raw_logits.device)
    high = torch.ones_like(low)
    budget = (
        JOINT_LOG_RATIO_BUDGET
        / substep_count.to(raw_logits.dtype).clamp_min(1.0)
    ).unsqueeze(1)
    for _ in range(BISECTION_STEPS):
        middle = (low + high) * 0.5
        candidate = parent_logits + middle * delta
        candidate_log_probability = torch.log_softmax(candidate, dim=1)
        maximum = (
            (candidate_log_probability - parent_log_probability)
            .abs()
            .masked_fill(~action_mask, 0.0)
            .max(dim=1, keepdim=True)
            .values
        )
        within = maximum <= budget
        low = torch.where(within, middle, low)
        high = torch.where(within, high, middle)
    scale = low.detach()
    projected = parent_logits + scale * delta
    return projected.masked_fill(~action_mask, -1.0e9)


class TrustProjectedActorCritic(RecurrentStructuredActorCritic):
    def forward(
        self, batch: PackedRows, *, remove_digest: bool = False
    ) -> tuple[Tensor, Tensor]:
        raw_logits, value = super().forward(batch, remove_digest=remove_digest)
        if remove_digest:
            return raw_logits, value
        return (
            project_logits(
                batch.parent_logits,
                raw_logits,
                batch.action_mask,
                batch.substep_count,
            ),
            value,
        )

