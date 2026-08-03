#!/usr/bin/env python3
"""Focused tests for the terminal-only structured policy rung pipeline."""

from __future__ import annotations

import json
from pathlib import Path
from types import SimpleNamespace
import tempfile
import unittest

import finalize_transport_v1 as finalizer
import run_pipeline_v1 as pipeline


def _metric(mean: float = 0.01, p90: float = 0.02, joint: float = 0.1) -> dict:
    return {
        "mean_total_variation": mean,
        "p90_total_variation": p90,
        "weighted_mean_kl": 0.001,
        "top_action_agreement": 0.99,
        "maximum_absolute_joint_log_ratio": joint,
        "policy_mass": 1.0,
        "policy_rows": 1,
        "physical_decisions": 1,
    }


class PipelineTests(unittest.TestCase):
    def test_shard_ranges_are_exact_and_contiguous(self) -> None:
        self.assertEqual(
            pipeline._shard_ranges(64, 4),
            [(0, 16), (16, 16), (32, 16), (48, 16)],
        )
        self.assertEqual(pipeline._shard_ranges(3, 2), [(0, 2), (2, 1)])

    def test_fit_gate_covers_overall_both_seats_and_joint_ratio(self) -> None:
        movement = {
            "overall": _metric(),
            "by_candidate_seat": {"0": _metric(), "1": _metric()},
        }
        self.assertEqual(pipeline._fit_gate(movement)["decision"], "PASS")
        movement["by_candidate_seat"]["1"] = _metric(mean=0.031)
        self.assertEqual(pipeline._fit_gate(movement)["decision"], "REJECT")
        movement["by_candidate_seat"]["1"] = _metric()
        movement["overall"] = _metric(joint=0.501)
        self.assertEqual(pipeline._fit_gate(movement)["decision"], "REJECT")

    def test_transport_finalizer_rebinds_report_and_preserves_backups(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            base = Path(temporary)
            root = base / "candidate"
            root.mkdir()
            weights = root / "weights.f32le"
            weights.write_bytes(b"weights")
            report = {
                "schema": pipeline.REPORT_SCHEMA,
                "transport": {
                    "maximum_absolute_logit_error": 1.0,
                    "parent_value_bit_exact": False,
                },
            }
            report_path = root / "report.json"
            report_path.write_bytes(pipeline.history_publish._json_bytes(report))
            candidate = {
                "schema": pipeline.CANDIDATE_SCHEMA,
                "report": {
                    "filename": "report.json",
                    "sha256": pipeline._sha256(report_path),
                },
                "weights": {
                    "filename": "weights.f32le",
                    "sha256": pipeline._sha256(weights),
                },
                "composite_model_parameter_sha256": "a" * 64,
            }
            candidate_path = root / pipeline.CANDIDATE_FILENAME
            candidate_path.write_bytes(pipeline.history_publish._json_bytes(candidate))
            output = base / "transport.json"
            result = finalizer.finalize(
                SimpleNamespace(
                    root=root,
                    maximum_absolute_logit_error=2.0e-6,
                    parent_value_bit_exact=True,
                    output=output,
                )
            )
            self.assertEqual(result["decision"], "PASS")
            self.assertTrue(Path(result["backups"]["candidate"]).is_file())
            self.assertTrue(Path(result["backups"]["report"]).is_file())
            rewritten = json.loads(report_path.read_text(encoding="utf-8"))
            rebound = json.loads(candidate_path.read_text(encoding="utf-8"))
            self.assertEqual(
                rewritten["transport"]["maximum_absolute_logit_error"], 2.0e-6
            )
            self.assertTrue(rewritten["transport"]["parent_value_bit_exact"])
            self.assertEqual(
                rebound["report"]["sha256"], pipeline._sha256(report_path)
            )
            self.assertTrue(output.is_file())


if __name__ == "__main__":
    unittest.main()
