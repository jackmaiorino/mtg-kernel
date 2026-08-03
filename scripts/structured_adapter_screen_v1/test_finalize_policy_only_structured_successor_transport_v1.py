#!/usr/bin/env python3
"""Tests for sealing measured structured-successor transport."""

from __future__ import annotations

import argparse
import hashlib
import json
import tempfile
import unittest
from pathlib import Path

import finalize_policy_only_structured_successor_transport_v1 as subject
import fit_complete_history_live_candidate_v1 as history_publish
import fit_policy_only_structured_successor_v1 as successor


def _sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


class FinalizeTransportTest(unittest.TestCase):
    def _package(self, root: Path) -> None:
        root.mkdir()
        weights = root / "weights.f32le"
        weights.write_bytes(b"weights")
        report = root / "report.json"
        report.write_bytes(
            history_publish._json_bytes(
                {
                    "schema": successor.REPORT_SCHEMA,
                    "transport": {
                        "maximum_absolute_logit_error": 1.0,
                        "parent_value_bit_exact": False,
                    },
                }
            )
        )
        candidate = {
            "schema": successor.SCHEMA,
            "report": {"sha256": _sha(report)},
            "weights": {"sha256": _sha(weights)},
            "composite_model_parameter_sha256": "a" * 64,
        }
        (root / successor.CANDIDATE_FILENAME).write_bytes(
            history_publish._json_bytes(candidate)
        )

    def test_seals_measured_transport_and_rebinds_report(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            root = base / "candidate"
            self._package(root)
            args = argparse.Namespace(
                root=root,
                maximum_absolute_logit_error=2.5e-5,
                parent_value_bit_exact=True,
                output=base / "transport.json",
            )
            result = subject.finalize(args)
            self.assertEqual(result["decision"], "PASS")
            report = json.loads((root / "report.json").read_text(encoding="utf-8"))
            candidate = json.loads(
                (root / successor.CANDIDATE_FILENAME).read_text(encoding="utf-8")
            )
            self.assertEqual(
                report["transport"]["maximum_absolute_logit_error"], 2.5e-5
            )
            self.assertEqual(candidate["report"]["sha256"], _sha(root / "report.json"))
            self.assertTrue(Path(str(root) + ".pretransport.report.json").exists())

    def test_rejects_failed_or_over_limit_transport(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            root = base / "candidate"
            self._package(root)
            for error, exact in ((3.1e-5, True), (2.0e-5, False)):
                with self.assertRaises(ValueError):
                    subject.finalize(
                        argparse.Namespace(
                            root=root,
                            maximum_absolute_logit_error=error,
                            parent_value_bit_exact=exact,
                            output=base / "transport.json",
                        )
                    )


if __name__ == "__main__":
    unittest.main()
