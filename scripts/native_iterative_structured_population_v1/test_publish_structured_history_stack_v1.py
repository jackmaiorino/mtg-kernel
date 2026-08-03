from __future__ import annotations

import argparse
import hashlib
import json
import tempfile
import unittest
from pathlib import Path
from unittest import mock

import torch

import publish_structured_history_stack_v1 as publisher
import run_screen as screen


EXPECTED_LAYOUT_SHA256 = (
    "a26dcabe7c3fb9144cdc5acec5698f9b988eac00530385a0bcf4e56789e52147"
)
EXPECTED_SYNTHETIC_COMPOSITE_SHA256 = (
    "3b4092fa664c921f9b3b21896008711081ef68c12f8286fab12c23ae75a01e2f"
)


class PublishStructuredHistoryStackTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tempdir = tempfile.TemporaryDirectory()
        self.root = Path(self.tempdir.name)
        self.parent_root = self.root / "parent-source"
        self.parent_root.mkdir()
        (self.parent_root / "checkpoint.json").write_text(
            json.dumps({"schema": "synthetic-parent"}) + "\n",
            encoding="utf-8",
        )
        (self.parent_root / "checkpoint.state.f32le").write_bytes(
            bytes(range(64))
        )
        self.synthetic_parent = {
            "manifest_sha256": publisher._sha256(
                self.parent_root / "checkpoint.json"
            ),
            "payload_sha256": publisher._sha256(
                self.parent_root / "checkpoint.state.f32le"
            ),
            "native_state_sha256": "1" * 64,
            "model_parameter_sha256": "2" * 64,
            "adam_step": 1,
        }
        self.expected_head = "3" * 40
        self.expected_cache_sha256 = "4" * 64

    def tearDown(self) -> None:
        self.tempdir.cleanup()

    def _make_state(
        self,
        fold: int,
        label: str,
        seat_conditioned: bool = False,
        value_offset: int = 0,
    ) -> Path:
        model = screen.StructuredAdapter(
            publisher.CARD_VOCAB,
            publisher.GROUP_VOCAB,
            publisher.HIDDEN_DIM,
            publisher.HISTORY_LENGTH,
            publisher.HISTORY_FEATURE_DIM,
            seat_conditioned,
        )
        with torch.no_grad():
            for ordinal, tensor in enumerate(model.state_dict().values()):
                tensor.copy_(
                    torch.full_like(
                        tensor,
                        float(value_offset + fold + ordinal + 1) / 1000.0,
                    )
                )
        state_path = self.root / f"{label}-fold-{fold}.state.pt"
        torch.save(model.state_dict(), state_path)
        return state_path

    def _write_fold_result(
        self,
        fold: int,
        state_path: Path,
        label: str,
        surrogate_offset: float = 0.0,
    ) -> Path:
        result = {
            "schema": publisher.scaled.SCHEMA,
            "fold": fold,
            "profile_only": False,
            "source": {
                "cache": "synthetic-cache.pt",
                "cache_sha256": self.expected_cache_sha256,
                "outcome_jsonl_sha256": "5" * 64,
                "pair_count": 2_048,
                "episode_count": 3_072,
                "row_count": 50_000,
                "physical_decision_count": 60_000,
            },
            "split": {
                "rule": "pair_index_mod_4",
                "fit_episode_count": 1_536,
                "heldout_episode_count": 512,
                "fit_physical_decision_count": 45_000,
                "heldout_physical_decision_count": 15_000,
            },
            "config": publisher._expected_fold_config(),
            "advantage_statistics_by_candidate_seat": {"0": {}, "1": {}},
            "training_history": [{"epoch": 1, "policy_loss": 0.1}],
            "timings": {"train_seconds": 1.0, "total_seconds": 2.0},
            "calibration": {
                "decision_sample_count": 8_192,
                "scale": 1.0 + fold / 10.0,
                "uncalibrated_fit_movement": {"mean_total_variation": 0.02},
                "calibrated_fit_movement": {"mean_total_variation": 0.03},
            },
            "heldout_surrogate": {
                "overall": {
                    "surrogate": surrogate_offset + 0.01 * (fold + 1)
                },
                "by_candidate_seat": {
                    "0": {"surrogate": 0.01},
                    "1": {"surrogate": 0.02},
                },
            },
            "heldout_movement": {
                "mean_total_variation": 0.03,
                "tv_weighted_samples": [[0.03, 1.0]],
                "max_absolute_joint_log_ratio": 0.1,
            },
            "diagnostics": {
                "permutation_max_logit_delta": 0.0,
                "reference_sample_count": 10,
                "reference_affected_count": 3,
            },
            "model_state": {
                "path": str(state_path),
                "sha256": publisher._sha256(state_path),
            },
            "non_claims": ["synthetic fixture"],
        }
        result_path = self.root / f"{label}-fold-{fold}.json"
        result_path.write_text(
            json.dumps(result, sort_keys=True, indent=2) + "\n",
            encoding="utf-8",
        )
        return result_path

    def _make_round(
        self,
        label: str,
        value_offset: int = 0,
        surrogate_offset: float = 0.0,
    ) -> tuple[list[Path], list[Path]]:
        states = [
            self._make_state(fold, label, value_offset=value_offset)
            for fold in range(4)
        ]
        results = [
            self._write_fold_result(
                fold,
                states[fold],
                label,
                surrogate_offset=surrogate_offset,
            )
            for fold in range(4)
        ]
        return results, states

    def _publish_args(
        self,
        fold_results: list[Path],
        fold_states: list[Path],
        output_root: Path,
        prior_stack_root: Path | None = None,
    ) -> argparse.Namespace:
        aggregate_result = self.root / f"aggregate-{output_root.name}.json"
        fold_payloads = [
            json.loads(path.read_text(encoding="utf-8"))
            for path in fold_results
        ]
        aggregate_result.write_text(
            json.dumps(
                {
                    "schema": publisher.scaled.AGGREGATE_SCHEMA,
                    "source_cache_sha256": self.expected_cache_sha256,
                    "config": publisher._expected_fold_config(),
                    "fold_results": [
                        {
                            "path": str(path),
                            "sha256": publisher._sha256(path),
                        }
                        for path in fold_results
                    ],
                    "fold_surrogates": {
                        str(payload["fold"]): payload["heldout_surrogate"][
                            "overall"
                        ]["surrogate"]
                        for payload in fold_payloads
                    },
                    "gate_config": {
                        "min_mean_total_variation": 0.0,
                        "max_mean_total_variation": 0.05,
                        "max_p90_total_variation": 0.15,
                        "max_absolute_joint_log_ratio": 0.75,
                    },
                    "gates": {
                        "aggregate_surrogate_positive": True,
                        "both_candidate_seats_surrogate_positive": True,
                        "at_least_three_of_four_folds_positive": True,
                    },
                    "pass": True,
                },
                sort_keys=True,
                indent=2,
            )
            + "\n",
            encoding="utf-8",
        )
        publication_report = self.root / "publication-reports" / (
            output_root.name + ".json"
        )
        return argparse.Namespace(
            fold_result=fold_results,
            fold_state=fold_states,
            aggregate_result=aggregate_result,
            parent_root=self.parent_root,
            output_root=output_root,
            publication_report=publication_report,
            expected_cache_sha256=self.expected_cache_sha256,
            expected_source_commit=self.expected_head,
            prior_stack_root=prior_stack_root,
            repo_root=self.root,
        )

    def _publish(self, args: argparse.Namespace) -> dict[str, object]:
        with mock.patch.object(
            publisher, "EXPECTED_PARENT", self.synthetic_parent
        ), mock.patch.object(
            publisher, "_git_head", return_value=self.expected_head
        ):
            return publisher.publish(args)

    def test_manifest_and_exact_inventory_match_loader_contract(self) -> None:
        results, states = self._make_round("round1")
        output_root = self.root / "stack-round-1"
        args = self._publish_args(results, states, output_root)
        summary = self._publish(args)

        self.assertEqual(
            {path.name for path in output_root.iterdir()},
            {publisher.STACK_FILENAME, "parent", "weights"},
        )
        self.assertEqual(
            {path.name for path in (output_root / "parent").iterdir()},
            {"checkpoint.json", "checkpoint.state.f32le"},
        )
        self.assertEqual(
            {path.name for path in (output_root / "weights").iterdir()},
            {"stage-000"},
        )
        self.assertEqual(
            {path.name for path in (output_root / "weights/stage-000").iterdir()},
            {f"member-{ordinal:03d}.f32le" for ordinal in range(4)},
        )
        self.assertNotIn("publication_report.json", {p.name for p in output_root.iterdir()})
        self.assertTrue(args.publication_report.is_file())
        self.assertNotIn(output_root, args.publication_report.resolve().parents)

        manifest = json.loads(
            (output_root / publisher.STACK_FILENAME).read_text(encoding="utf-8")
        )
        self.assertEqual(
            set(manifest),
            {
                "schema",
                "publication_encoding",
                "parent",
                "architecture",
                "weights",
                "composite_model_parameter_sha256",
            },
        )
        self.assertEqual(manifest["schema"], publisher.STACK_SCHEMA)
        self.assertEqual(
            set(manifest["parent"]),
            {
                "directory",
                "manifest_sha256",
                "payload_sha256",
                "native_state_sha256",
                "model_parameter_sha256",
                "adam_step",
            },
        )
        self.assertEqual(manifest["architecture"], publisher._architecture_manifest())
        self.assertEqual(
            set(manifest["weights"]),
            {
                "directory",
                "encoding",
                "sha256",
                "parameter_count",
                "parameter_layout_sha256",
                "stages",
            },
        )
        self.assertEqual(manifest["weights"]["directory"], "weights")
        self.assertEqual(manifest["weights"]["encoding"], publisher.WEIGHTS_ENCODING)
        self.assertEqual(manifest["weights"]["parameter_count"], 107_378)
        self.assertEqual(
            manifest["weights"]["parameter_layout_sha256"],
            EXPECTED_LAYOUT_SHA256,
        )
        self.assertEqual(len(manifest["weights"]["stages"]), 1)
        stage = manifest["weights"]["stages"][0]
        self.assertEqual(set(stage), {"ordinal", "directory", "members"})
        self.assertEqual(stage["ordinal"], 0)
        self.assertEqual(stage["directory"], "stage-000")
        self.assertEqual(len(stage["members"]), 4)
        for ordinal, member in enumerate(stage["members"]):
            self.assertEqual(
                member,
                {
                    "ordinal": ordinal,
                    "filename": f"member-{ordinal:03d}.f32le",
                    "sha256": member["sha256"],
                    "byte_count": publisher.MEMBER_BYTE_COUNT,
                },
            )
            self.assertEqual(member["sha256"], member["sha256"].lower())

        concatenated = b"".join(
            (
                output_root
                / "weights"
                / stage["directory"]
                / member["filename"]
            ).read_bytes()
            for member in stage["members"]
        )
        self.assertEqual(
            manifest["weights"]["sha256"],
            hashlib.sha256(concatenated).hexdigest(),
        )
        self.assertEqual(summary["weights_sha256"], manifest["weights"]["sha256"])
        publication_report = json.loads(
            args.publication_report.read_text(encoding="utf-8")
        )
        self.assertEqual(
            publication_report["schema"], publisher.PUBLICATION_REPORT_SCHEMA
        )
        self.assertTrue(
            publication_report["source"]["aggregate_result"][
                "aggregate_pass_required"
            ]
        )
        self.assertEqual(len(publication_report["members"]), 4)
        self.assertEqual(publication_report["manifest"], manifest)

    def test_append_preserves_prior_parent_weights_and_stage_bindings(self) -> None:
        first_results, first_states = self._make_round("round1")
        first_root = self.root / "stack-round-1"
        self._publish(self._publish_args(first_results, first_states, first_root))
        first_manifest = json.loads(
            (first_root / publisher.STACK_FILENAME).read_text(encoding="utf-8")
        )
        prior_stage_binding = first_manifest["weights"]["stages"][0]
        prior_bytes = {
            path.relative_to(first_root).as_posix(): path.read_bytes()
            for path in first_root.rglob("*")
            if path.is_file() and path.name != publisher.STACK_FILENAME
        }

        second_results, second_states = self._make_round(
            "round2", value_offset=100, surrogate_offset=0.2
        )
        second_root = self.root / "stack-round-2"
        self._publish(
            self._publish_args(
                second_results,
                second_states,
                second_root,
                prior_stack_root=first_root,
            )
        )
        second_manifest = json.loads(
            (second_root / publisher.STACK_FILENAME).read_text(encoding="utf-8")
        )
        self.assertEqual(len(second_manifest["weights"]["stages"]), 2)
        self.assertEqual(
            second_manifest["weights"]["stages"][0], prior_stage_binding
        )
        for relative_path, payload in prior_bytes.items():
            self.assertEqual((second_root / relative_path).read_bytes(), payload)
        self.assertEqual(
            {path.name for path in (second_root / "weights").iterdir()},
            {"stage-000", "stage-001"},
        )

    def test_rejects_failed_aggregate(self) -> None:
        results, states = self._make_round("failed-gate")
        output_root = self.root / "failed-gate-stack"
        args = self._publish_args(results, states, output_root)
        aggregate = json.loads(args.aggregate_result.read_text(encoding="utf-8"))
        aggregate["gates"]["aggregate_surrogate_positive"] = False
        aggregate["pass"] = False
        args.aggregate_result.write_text(
            json.dumps(aggregate, sort_keys=True, indent=2) + "\n",
            encoding="utf-8",
        )
        with self.assertRaisesRegex(ValueError, "did not pass every frozen gate"):
            self._publish(args)
        self.assertFalse(output_root.exists())

    def test_rejects_seat_conditioned_state_layout(self) -> None:
        results, states = self._make_round("seat-head")
        bad_state = self._make_state(3, "seat-head-bad", seat_conditioned=True)
        states[3] = bad_state
        results[3] = self._write_fold_result(3, bad_state, "seat-head-bad")
        output_root = self.root / "seat-head-stack"
        with self.assertRaisesRegex(
            ValueError,
            "exact non-seat-conditioned layout",
        ):
            self._publish(self._publish_args(results, states, output_root))
        self.assertFalse(output_root.exists())

    def test_parameter_layout_and_composite_framing_are_deterministic(self) -> None:
        layout_sha256 = publisher._parameter_layout_sha256(
            publisher._fixed_parameter_layout()
        )
        self.assertEqual(layout_sha256, EXPECTED_LAYOUT_SHA256)
        parent = {
            "manifest_sha256": "11" * 32,
            "payload_sha256": "22" * 32,
            "native_state_sha256": "33" * 32,
            "model_parameter_sha256": "44" * 32,
            "adam_step": 5,
        }
        members = [
            {
                "ordinal": ordinal,
                "filename": publisher._member_filename(ordinal),
                "sha256": format(0x66 + ordinal, "02x") * 32,
                "byte_count": publisher.MEMBER_BYTE_COUNT,
            }
            for ordinal in range(4)
        ]
        stages = [
            {
                "ordinal": 0,
                "directory": "stage-000",
                "members": members,
            }
        ]
        self.assertEqual(
            publisher._stack_composite_sha256(
                parent,
                layout_sha256,
                "55" * 32,
                stages,
            ),
            EXPECTED_SYNTHETIC_COMPOSITE_SHA256,
        )

    def test_rejects_publication_report_inside_output_root(self) -> None:
        results, states = self._make_round("inside-report")
        output_root = self.root / "inside-report-stack"
        args = self._publish_args(results, states, output_root)
        args.publication_report = output_root / "report.json"
        with self.assertRaisesRegex(
            ValueError,
            "must resolve outside output_root",
        ):
            self._publish(args)
        self.assertFalse(output_root.exists())


if __name__ == "__main__":
    unittest.main()
