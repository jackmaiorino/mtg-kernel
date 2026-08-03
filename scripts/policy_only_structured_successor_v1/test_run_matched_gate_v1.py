#!/usr/bin/env python3
"""Unit tests for the native Pool3 structured successor matched gate."""

from __future__ import annotations

import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock


import run_matched_gate_v1 as subject


def _checkpoint(identity: dict[str, str], authority_kind: str) -> dict[str, object]:
    return {
        "authority_kind": authority_kind,
        "loaded_generation": 1,
        "loaded_checkpoint_sha256": identity["manifest"],
        "loaded_payload_sha256": identity["payload"],
        "loaded_train_state_sha256": identity["train_state"],
        "model_parameter_sha256": identity["model"],
    }


def _header(checkpoint: dict[str, object]) -> dict[str, object]:
    return {
        "record_type": "header",
        "export_contract": subject.strength.OUTCOME_CONTRACT,
        "selection_source": "candidate_checkpoint_policy",
        "checkpoint": checkpoint,
    }


def _outcome_row(
    *, pair_index: int, episode_id: int, seat: str, environment_seed: str, reward: int
) -> dict[str, object]:
    return {
        "record_type": "terminal",
        "base_seed_u64_hex": f"{subject.FORMAL_BASE_SEED:016x}",
        "pair_index": pair_index,
        "episode_id": episode_id,
        "candidate_seat": seat,
        "pair_environment_seed_u64_hex": environment_seed,
        "candidate_terminal_reward": reward,
        "terminal": {"terminal_classification": "natural"},
    }


def _write_outcome(
    path: Path,
    rewards: tuple[int, int],
    seed: str,
    checkpoint: dict[str, object],
) -> None:
    rows = [
        _header(checkpoint),
        _outcome_row(
            pair_index=0,
            episode_id=0,
            seat="p0",
            environment_seed=seed,
            reward=rewards[0],
        ),
        _outcome_row(
            pair_index=0,
            episode_id=1,
            seat="p1",
            environment_seed=seed,
            reward=rewards[1],
        ),
    ]
    path.write_text(
        "".join(json.dumps(row, sort_keys=True) + "\n" for row in rows),
        encoding="utf-8",
    )


def _write_teacher(path: Path, checkpoint: dict[str, object]) -> None:
    path.write_text(
        json.dumps(
            {
                "record_type": "header",
                "export_contract": subject.TEACHER_CONTRACT,
                "selection_source": subject.TEACHER_SELECTION_SOURCE,
                "checkpoint": checkpoint,
            },
            sort_keys=True,
        )
        + "\n",
        encoding="utf-8",
    )


class MatchedGateTest(unittest.TestCase):
    def test_formal_contract_and_topology_are_explicit(self) -> None:
        self.assertEqual(subject.FORMAL_BASE_SEED, 1_650_001)
        self.assertEqual(subject.FORMAL_TARGET_PAIRS, 1_024)
        self.assertEqual(
            subject.SCORER.name, "native_population_corpus_stdio_v1.exe"
        )
        self.assertEqual(
            str(subject.POOL_ROOT), r"D:\mtg-kernel-ladder-pilot-20260725\pool3"
        )
        self.assertEqual(len(subject.FORMAL_SCORER_SHA256), 64)
        self.assertEqual(len(subject.POOL_CONTRACT_SHA256), 64)
        with self.assertRaises(SystemExit):
            subject._arguments(
                [
                    "--evidence-root",
                    "evidence",
                    "--candidate-root",
                    "candidate",
                    "--parent-root",
                    "parent",
                ]
            )

    def test_profile_mode_is_bounded_for_both_topologies(self) -> None:
        for topology in ("sequential", "parallel"):
            args = subject._arguments(
                [
                    "--profile-pairs",
                    "5",
                    "--topology",
                    topology,
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
            self.assertEqual(args.topology, topology)

    def test_collection_arguments_bind_each_arm_to_its_native_root(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            args = type(
                "Args",
                (),
                {
                    "evidence_root": root,
                    "candidate_root": root / "candidate",
                    "parent_root": root / "parent",
                    "pool_root": subject.POOL_ROOT,
                    "scorer": subject.SCORER,
                    "base_seed": subject.FORMAL_BASE_SEED,
                    "target_pairs": 4,
                },
            )()
            candidate_args, candidate_paths = subject._collection_args(
                args, "candidate", 0
            )
            parent_args, parent_paths = subject._collection_args(args, "parent", 0)
            self.assertEqual(candidate_args.candidate_root, args.candidate_root)
            self.assertEqual(parent_args.candidate_root, args.parent_root)
            self.assertIn("candidate", candidate_paths["outcome_jsonl"])
            self.assertIn("parent", parent_paths["outcome_jsonl"])

    def test_candidate_identity_binds_package_and_exact_parent(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            report = root / "report.json"
            weights = root / "weights.f32le"
            parent_root = root / "parent"
            parent_root.mkdir()
            parent_manifest = parent_root / "checkpoint.json"
            parent_payload = parent_root / "checkpoint.state.f32le"
            report.write_text("{}\n", encoding="utf-8")
            weights.write_bytes(b"weights")
            parent_manifest.write_bytes(b"manifest")
            parent_payload.write_bytes(b"payload")
            parent = {
                "directory": "parent",
                "manifest_sha256": subject._sha256(parent_manifest),
                "payload_sha256": subject._sha256(parent_payload),
                "native_state_sha256": "d" * 64,
                "model_parameter_sha256": "e" * 64,
                "adam_step": 1,
            }
            composite = hashlib.sha256(
                subject.COMPOSITE_DOMAIN
                + bytes.fromhex(parent["model_parameter_sha256"])
                + weights.read_bytes()
            ).hexdigest()
            candidate = {
                "schema": subject.CANDIDATE_SCHEMA,
                "parent": parent,
                "weights": {
                    "filename": "weights.f32le",
                    "sha256": subject._sha256(weights),
                },
                "report": {
                    "filename": "report.json",
                    "sha256": subject._sha256(report),
                },
                "composite_model_parameter_sha256": composite,
            }
            candidate_path = root / subject.CANDIDATE_FILENAME
            candidate_path.write_text(json.dumps(candidate) + "\n", encoding="utf-8")
            with mock.patch.dict(
                subject.PARENT_IDENTITY,
                {
                    "manifest": parent["manifest_sha256"],
                    "payload": parent["payload_sha256"],
                    "train_state": parent["native_state_sha256"],
                    "model": parent["model_parameter_sha256"],
                },
            ):
                identity = subject._candidate_identity(root)
            self.assertEqual(identity["candidate_json_sha256"], subject._sha256(candidate_path))
            self.assertEqual(identity["report_sha256"], subject._sha256(report))
            self.assertEqual(identity["weights_sha256"], subject._sha256(weights))
            self.assertEqual(identity["composite_model_parameter_sha256"], composite)
            self.assertEqual(identity["parent"], parent)

    def test_native_panels_apply_noninferiority_gates(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            candidate_path = root / "candidate.outcome.jsonl"
            parent_path = root / "parent.outcome.jsonl"
            candidate_identity = {
                "manifest": "a" * 64,
                "payload": "b" * 64,
                "train_state": "c" * 64,
                "model": "d" * 64,
            }
            candidate_checkpoint = _checkpoint(
                candidate_identity,
                "xmage-cp7-outcome-structured-policy-successor-v1",
            )
            parent_checkpoint = _checkpoint(
                subject.PARENT_IDENTITY,
                "xmage-cp7-outcome-reinforce-derivative-v1",
            )
            _write_outcome(
                candidate_path, (1, 1), "0000000000000001", candidate_checkpoint
            )
            _write_outcome(
                parent_path, (-1, -1), "0000000000000001", parent_checkpoint
            )
            candidate_teacher = root / "candidate.teacher.jsonl"
            parent_teacher = root / "parent.teacher.jsonl"
            _write_teacher(candidate_teacher, candidate_checkpoint)
            _write_teacher(parent_teacher, parent_checkpoint)
            args = subject._arguments(
                [
                    "--profile-pairs",
                    "1",
                    "--topology",
                    "sequential",
                    "--evidence-root",
                    str(root),
                    "--candidate-root",
                    str(root),
                    "--parent-root",
                    str(root),
                ]
            )
            args.candidate_identity = candidate_identity
            candidate_result = {
                "paths": {
                    "outcome_jsonl": str(candidate_path),
                    "teacher_jsonl": str(candidate_teacher),
                },
                "report": {"teacher_sha256": "b" * 64},
            }
            parent_result = {
                "paths": {
                    "outcome_jsonl": str(parent_path),
                    "teacher_jsonl": str(parent_teacher),
                },
                "report": {"teacher_sha256": "c" * 64},
            }
            report = subject._adjudicate(args, candidate_result, parent_result)
            self.assertEqual(report["candidate_wins"], 2)
            self.assertEqual(report["parent_wins"], 0)
            self.assertTrue(all(report["gates"].values()))

    def test_environment_seed_mismatch_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            candidate_path = root / "candidate.outcome.jsonl"
            parent_path = root / "parent.outcome.jsonl"
            candidate_checkpoint = _checkpoint(
                subject.PARENT_IDENTITY,
                "xmage-cp7-outcome-structured-policy-successor-v1",
            )
            parent_checkpoint = _checkpoint(
                subject.PARENT_IDENTITY,
                "xmage-cp7-outcome-reinforce-derivative-v1",
            )
            _write_outcome(
                candidate_path, (1, 1), "0000000000000001", candidate_checkpoint
            )
            _write_outcome(
                parent_path, (1, 1), "0000000000000002", parent_checkpoint
            )
            candidate_teacher = root / "candidate.teacher.jsonl"
            parent_teacher = root / "parent.teacher.jsonl"
            _write_teacher(candidate_teacher, candidate_checkpoint)
            _write_teacher(parent_teacher, parent_checkpoint)
            args = subject._arguments(
                [
                    "--profile-pairs",
                    "1",
                    "--topology",
                    "parallel",
                    "--evidence-root",
                    str(root),
                    "--candidate-root",
                    str(root),
                    "--parent-root",
                    str(root),
                ]
            )
            args.candidate_identity = subject.PARENT_IDENTITY.copy()
            with self.assertRaisesRegex(RuntimeError, "matched terminal field differs"):
                subject._adjudicate(
                    args,
                    {
                        "paths": {
                            "outcome_jsonl": str(candidate_path),
                            "teacher_jsonl": str(candidate_teacher),
                        }
                    },
                    {
                        "paths": {
                            "outcome_jsonl": str(parent_path),
                            "teacher_jsonl": str(parent_teacher),
                        }
                    },
                )


if __name__ == "__main__":
    unittest.main()
