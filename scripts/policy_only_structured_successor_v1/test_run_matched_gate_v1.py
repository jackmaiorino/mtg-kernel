#!/usr/bin/env python3
"""Unit tests for the policy-only structured successor matched gate."""

from __future__ import annotations

import json
import hashlib
from pathlib import Path
import sys
import tempfile
import unittest


sys.path.insert(0, str(Path(__file__).resolve().parent))
import run_matched_gate_v1 as subject  # noqa: E402


def _pair_line(
    *, base_seed: int, pair_index: int, environment_seed: str, winners: str
) -> str:
    return (
        "XMAGE_RALLY_ANCHOR_PAIR PASS"
        f" base_seed={base_seed} episodes={pair_index * 2},{pair_index * 2 + 1}"
        f" pair_index={pair_index} environment_seed={environment_seed}"
        f" candidate_seats=p0,p1 opponent=cp7 cp7_skill=7 winners={winners}"
        " turns=4,5 rust_steps=6,7"
        " physical_decisions=8,9 candidate_priority_projections=1,1"
        " alignment=selected_action_projection\n"
    )


def _legs(pair_index: int, winners: str) -> str:
    rows = winners.split(",")
    return "".join(
        (
            "XMAGE_RALLY_ANCHOR_LEG PASS"
            f" episode={pair_index * 2 + offset} candidate={seat}"
            f" winner={winner} rust_steps=6 physical_decisions=8\n"
        )
        for offset, (seat, winner) in enumerate(zip(("p0", "p1"), rows))
    )


class MatchedGateTest(unittest.TestCase):
    def test_formal_defaults_are_fixed(self) -> None:
        args = subject._arguments(
            [
                "--self-test",
            ]
        )
        self.assertTrue(args.self_test)
        self.assertEqual(subject.FORMAL_BASE_SEED, 1_650_001)
        self.assertEqual(subject.FORMAL_TARGET_PAIRS, 1_024)
        self.assertEqual(subject.FORMAL_MAX_PAIRS, 1_280)
        self.assertEqual(subject.FORMAL_BATCH_PAIRS, 8)

    def test_profile_arguments_are_non_formal_and_bounded(self) -> None:
        args = subject._arguments(
            [
                "--profile-pairs",
                "5",
                "--evidence-root",
                "evidence",
                "--candidate-root",
                "candidate",
                "--parent-root",
                "parent",
            ]
        )
        self.assertFalse(args.formal)
        self.assertEqual(args.target_pairs, 5)
        self.assertEqual(args.max_pairs, 7)
        self.assertEqual(args.batch_pairs, 5)

    def test_candidate_identity_binds_all_package_parts_and_parent(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            report = root / "report.json"
            weights = root / "weights.f32le"
            report.write_text(json.dumps({"schema": "report"}) + "\n", encoding="utf-8")
            weights.write_bytes(b"weights")
            composite = hashlib.sha256(
                subject.COMPOSITE_DOMAIN
                + bytes.fromhex(subject.base.PARENT_IDENTITY["model"])
                + weights.read_bytes()
            ).hexdigest()
            candidate = {
                "schema": subject.CANDIDATE_SCHEMA,
                "parent": {
                    "directory": "parent",
                    "manifest_sha256": subject.base.PARENT_IDENTITY["manifest"],
                    "payload_sha256": subject.base.PARENT_IDENTITY["payload"],
                    "native_state_sha256": subject.base.PARENT_IDENTITY["train_state"],
                    "model_parameter_sha256": subject.base.PARENT_IDENTITY["model"],
                    "adam_step": 1,
                },
                "weights": {
                    "filename": "weights.f32le",
                    "sha256": subject.base._sha256(weights),
                },
                "report": {
                    "filename": "report.json",
                    "sha256": subject.base._sha256(report),
                },
                "composite_model_parameter_sha256": composite,
            }
            (root / subject.CANDIDATE_FILENAME).write_text(
                json.dumps(candidate) + "\n", encoding="utf-8"
            )
            identity = subject._candidate_identity(root)
            self.assertEqual(identity["payload"], subject.base._sha256(weights))
            self.assertEqual(identity["train_state"], subject.base._sha256(report))
            self.assertEqual(identity["model"], composite)
            self.assertEqual(identity["parent"], candidate["parent"])

    def test_pair_marker_requires_two_natural_transport_legs(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "pair.log"
            path.write_text(
                _legs(0, "p0,p1")
                + _pair_line(
                    base_seed=subject.FORMAL_BASE_SEED,
                    pair_index=0,
                    environment_seed="0000000000000001",
                    winners="p0,p1",
                ),
                encoding="utf-8",
            )
            self.assertTrue(subject._pair_marker(path, subject.FORMAL_BASE_SEED, 0))
            path.write_text(
                _pair_line(
                    base_seed=subject.FORMAL_BASE_SEED,
                    pair_index=0,
                    environment_seed="0000000000000001",
                    winners="p0,p1",
                ),
                encoding="utf-8",
            )
            self.assertFalse(subject._pair_marker(path, subject.FORMAL_BASE_SEED, 0))

    def test_adjudication_applies_noninferiority_gates(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            candidate_log = root / "candidate.log"
            parent_log = root / "parent.log"
            candidate_log.write_text(
                _legs(0, "p0,p1")
                + _pair_line(
                    base_seed=subject.FORMAL_BASE_SEED,
                    pair_index=0,
                    environment_seed="0000000000000001",
                    winners="p0,p1",
                ),
                encoding="utf-8",
            )
            parent_log.write_text(
                _legs(0, "p1,p0")
                + _pair_line(
                    base_seed=subject.FORMAL_BASE_SEED,
                    pair_index=0,
                    environment_seed="0000000000000001",
                    winners="p1,p0",
                ),
                encoding="utf-8",
            )
            args = subject._arguments(
                [
                    "--profile-pairs",
                    "1",
                    "--evidence-root",
                    str(root),
                    "--candidate-root",
                    str(root),
                    "--parent-root",
                    str(root),
                ]
            )
            args.candidate_identity = {"manifest": "a" * 64}
            tasks = {
                (0, "candidate"): {
                    "log": str(candidate_log),
                    "log_sha256": subject.base._sha256(candidate_log),
                },
                (0, "parent"): {
                    "log": str(parent_log),
                    "log_sha256": subject.base._sha256(parent_log),
                },
            }
            report = subject._adjudicate(args, [0], tasks, [], [])
            self.assertEqual(report["candidate_wins"], 2)
            self.assertEqual(report["parent_wins"], 0)
            self.assertTrue(all(report["gates"].values()))


if __name__ == "__main__":
    unittest.main()
