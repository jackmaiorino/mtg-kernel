#!/usr/bin/env python3
"""Focused hard-envelope tests for the projected v2 actor."""

from __future__ import annotations

import unittest

import torch

import model_v2


class ProjectionTests(unittest.TestCase):
    def test_all_action_log_ratios_fit_each_row_budget(self) -> None:
        generator = torch.Generator().manual_seed(123)
        parent = torch.randn((32, 11), generator=generator) * 4.0
        raw = torch.randn((32, 11), generator=generator) * 12.0
        mask = torch.ones_like(parent, dtype=torch.bool)
        mask[::2, -3:] = False
        parent = parent.masked_fill(~mask, -1.0e9)
        raw = raw.masked_fill(~mask, -1.0e9)
        substeps = torch.arange(1, 33).remainder(8).add(1)
        projected = model_v2.project_logits(parent, raw, mask, substeps)
        delta = (
            torch.log_softmax(projected, dim=1)
            - torch.log_softmax(parent, dim=1)
        ).abs().masked_fill(~mask, 0.0)
        bound = model_v2.JOINT_LOG_RATIO_BUDGET / substeps
        self.assertTrue(torch.all(delta.max(dim=1).values <= bound + 2.0e-5))

    def test_selected_physical_decision_joint_ratio_is_bounded(self) -> None:
        generator = torch.Generator().manual_seed(456)
        substeps = 9
        parent = torch.randn((substeps, 7), generator=generator) * 3.0
        raw = torch.randn((substeps, 7), generator=generator) * 9.0
        mask = torch.ones_like(parent, dtype=torch.bool)
        counts = torch.full((substeps,), substeps)
        projected = model_v2.project_logits(parent, raw, mask, counts)
        parent_lp = torch.log_softmax(parent, dim=1)
        projected_lp = torch.log_softmax(projected, dim=1)
        for action in range(parent.shape[1]):
            joint = float((projected_lp[:, action] - parent_lp[:, action]).sum())
            self.assertLessEqual(abs(joint), model_v2.JOINT_LOG_RATIO_BUDGET + 2.0e-5)


if __name__ == "__main__":
    unittest.main()
