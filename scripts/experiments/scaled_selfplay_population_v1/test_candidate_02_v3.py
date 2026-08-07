from __future__ import annotations

import copy
import importlib.util
import json
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT_DIR = Path(__file__).resolve().parent
REPO_ROOT = SCRIPT_DIR.parents[2]
SPEC_PATH = REPO_ROOT / "docs" / "native_scaled_selfplay_candidate_02_v3_spec_v1.json"
sys.path.insert(0, str(SCRIPT_DIR))


def load_module(name: str, path: Path):
    module_spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(module_spec)
    sys.modules[name] = module
    module_spec.loader.exec_module(module)
    return module


ANALYZER = load_module("candidate_02_v3_analysis_tested", SCRIPT_DIR / "candidate_02_v3_analysis.py")
RUNNER = load_module("candidate_02_v3_runner_tested", SCRIPT_DIR / "candidate_02_v3.py")


def write_json(path: Path, value: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(ANALYZER.canonical_bytes(value))


def record(path: Path) -> dict:
    return {"path": str(path.resolve()), "bytes": path.stat().st_size, "sha256": ANALYZER.sha256_file(path)}


def raw_identity(binding: dict, has_bundle: bool) -> dict:
    value = {
        "checkpoint_manifest_sha256": binding["checkpoint_sha256"],
        "checkpoint_payload_sha256": binding["state_sha256"],
        "generation": binding["generation"],
        "model_parameter_sha256": binding["model_parameter_sha256"],
        "run_sha256": binding["run_sha256"],
    }
    if has_bundle:
        value["identity_bundle_sha256"] = binding["identity_bundle_sha256"]
    return value


def make_outcome(spec: dict, mode: str, chunk_index: int, arm: str, rank: int) -> dict:
    pair_count = spec["chunk_pair_count"]
    evaluation_seed = spec[mode]["first_evaluation_seed"] + chunk_index * spec["evaluation_seed_stride"]
    parent = spec["opponent_and_control"]
    candidate = spec["candidate"] if arm == "candidate" else parent
    episodes = []
    seat_counts = {"P0": {"wins": 0, "losses": 0, "draws": 0}, "P1": {"wins": 0, "losses": 0, "draws": 0}}
    for pair_index in range(pair_count):
        environment_seed = ANALYZER.trainer_environment_seed(evaluation_seed, pair_index)
        for leg, seat in enumerate(("P0", "P1")):
            episodes.append(
                {
                    "deck_hashes_u64": spec["expected_rally_deck_hashes_u64"],
                    "environment_seed": environment_seed,
                    "episode_index": pair_index * 2 + leg,
                    "learner_seat": seat,
                    "opponent_pool_member": "Primary",
                    "pair_index": pair_index,
                    "terminal_order_rank": rank,
                }
            )
            seat_counts[seat]["wins" if rank == 1 else "losses" if rank == -1 else "draws"] += 1
    overall = {key: seat_counts["P0"][key] + seat_counts["P1"][key] for key in seat_counts["P0"]}
    return {
        "candidate": raw_identity(candidate, True),
        "episode_count": pair_count * 2,
        "episodes": episodes,
        "evaluation_base_seed": evaluation_seed,
        "learner_outcomes": {"P0": seat_counts["P0"], "P1": seat_counts["P1"], "overall": overall},
        "opponent": raw_identity(parent, False),
        "pair_count": pair_count,
        "runtime": {
            "all_natural": True,
            "broker_batch_target": 1,
            "environment_randomization_v2": True,
            "sessions_per_worker": 1,
            "worker_count": 1,
        },
        "schema": ANALYZER.OUTCOME_SCHEMA,
    }


def make_arm_record(root: Path, spec: dict, mode: str, chunk_index: int, arm: str, rank: int) -> dict:
    arm_root = root / f"chunk-{chunk_index:03d}-{arm}"
    arm_root.mkdir(parents=True)
    outcome_path = arm_root / "outcome.json"
    stdout_path = arm_root / "stdout.log"
    stderr_path = arm_root / "stderr.log"
    write_json(outcome_path, make_outcome(spec, mode, chunk_index, arm, rank))
    stdout_path.write_bytes(b"test\n")
    stderr_path.write_bytes(b"")
    evaluation_seed = spec[mode]["first_evaluation_seed"] + chunk_index * spec["evaluation_seed_stride"]
    return {
        "label": f"chunk-{chunk_index:03d}-{arm}",
        "candidate_index": 0 if arm == "candidate" else 1,
        "opponent_index": 1,
        "pair_count": spec["chunk_pair_count"],
        "evaluation_seed": evaluation_seed,
        "exit_code": 0,
        "wall_seconds": 1.0,
        "stdout": record(stdout_path),
        "stderr": record(stderr_path),
        "outcome": record(outcome_path),
    }


class Candidate02StrictUnitTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.spec = json.loads(SPEC_PATH.read_text(encoding="utf-8"))

    def small_outcome(self, root: Path) -> tuple[dict, Path]:
        spec = copy.deepcopy(self.spec)
        spec["chunk_pair_count"] = 2
        path = root / "outcome.json"
        write_json(path, make_outcome(spec, "screen", 0, "candidate", 1))
        return spec, path

    def test_native_environment_kdf_matches_frozen_goldens(self) -> None:
        goldens = json.loads((REPO_ROOT / "data" / "native_trainer_schedule_v1_goldens.json").read_text())
        for vector in goldens["vectors"]:
            self.assertEqual(
                ANALYZER.trainer_environment_seed(vector["base_seed"], vector["pair_index"]),
                vector["environment_seed"],
            )

    def test_duplicate_json_key_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "duplicate.json"
            path.write_text('{"a":1,"a":2}', encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "duplicate JSON key"):
                ANALYZER.load_json_strict(path)

    def test_wrong_derived_environment_schedule_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            spec, path = self.small_outcome(Path(temp))
            value = json.loads(path.read_text())
            value["episodes"][0]["environment_seed"] += 1
            write_json(path, value)
            with self.assertRaisesRegex(ValueError, "derived environment seed mismatch"):
                ANALYZER.validate_outcome(path, spec, "screen", 0, "candidate")

    def test_row_reorder_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            spec, path = self.small_outcome(Path(temp))
            value = json.loads(path.read_text())
            value["episodes"][0], value["episodes"][1] = value["episodes"][1], value["episodes"][0]
            write_json(path, value)
            with self.assertRaisesRegex(ValueError, "episode row order changed"):
                ANALYZER.validate_outcome(path, spec, "screen", 0, "candidate")

    def test_nonintegral_terminal_rank_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            spec, path = self.small_outcome(Path(temp))
            value = json.loads(path.read_text())
            value["episodes"][0]["terminal_order_rank"] = 1.0
            write_json(path, value)
            with self.assertRaisesRegex(ValueError, "must be an exact integer"):
                ANALYZER.validate_outcome(path, spec, "screen", 0, "candidate")

    def test_freshness_overlap_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            spec = copy.deepcopy(self.spec)
            freshness = json.loads(Path(spec["freshness_manifest"]["path"]).read_text())
            freshness["excluded_evaluation_seed_intervals"][0]["end_inclusive"] = 3000000000
            freshness_path = root / "freshness.json"
            write_json(freshness_path, freshness)
            spec["freshness_manifest"] = record(freshness_path)
            spec_path = root / "spec.json"
            write_json(spec_path, spec)
            with self.assertRaisesRegex(ValueError, "overlaps revealed interval"):
                ANALYZER.validate_spec(spec_path)


class Candidate02ReconstructionTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.temp = tempfile.TemporaryDirectory()
        cls.root = Path(cls.temp.name)
        cls.spec = json.loads(SPEC_PATH.read_text(encoding="utf-8"))
        executable = Path(cls.spec["executable"]["path"])
        run_context = {
            "git": {"commit": "test", "executable_source_commit": cls.spec["executable"]["source_commit"]},
            "toolchain": {"test": True},
            "executable": {**record(executable), "source_commit": cls.spec["executable"]["source_commit"]},
            "spec": record(SPEC_PATH),
            "candidate": cls.spec["candidate"],
            "opponent_and_control": cls.spec["opponent_and_control"],
            "gpu_ordinal": "test",
            "terminal_reward_only": True,
        }
        write_json(cls.root / "gate-plan.json", RUNNER.build_plan(cls.spec, "screen", run_context, None, None, None))
        receipt = {
            "schema": ANALYZER.RECEIPT_SCHEMA,
            "chunk_index": 0,
            "evaluation_seed": cls.spec["screen"]["first_evaluation_seed"],
            "candidate_arm": make_arm_record(cls.root, cls.spec, "screen", 0, "candidate", 1),
            "control_arm": make_arm_record(cls.root, cls.spec, "screen", 0, "control", -1),
        }
        write_json(cls.root / "chunk-000-receipt.json", receipt)

    @classmethod
    def tearDownClass(cls) -> None:
        cls.temp.cleanup()

    def test_manifest_reconstruction_and_postdecision_exclusion(self) -> None:
        analysis = ANALYZER.build_analysis(self.root, SPEC_PATH, "screen", True)
        self.assertEqual(analysis["acquired_N"], self.spec["chunk_pair_count"])
        self.assertEqual(analysis["decision"], "SUCCESS")
        self.assertGreater(analysis["post_decision_acquired_clusters_excluded"], 0)
        self.assertTrue(analysis["trajectory_complete"])
        self.assertEqual(len(analysis["trajectory_records"]), self.spec["chunk_pair_count"])
        self.assertEqual(analysis["leg_counts_by_seat_at_acquired_N"]["P0"]["favorable"], self.spec["chunk_pair_count"])
        self.assertNotEqual(analysis["acquired_stream_sha256"], "")
        self.assertNotEqual(analysis["decision_prefix_stream_sha256"], "")

    def test_forged_initial_analysis_cannot_authorize_confirmation(self) -> None:
        analysis = ANALYZER.build_analysis(self.root, SPEC_PATH, "screen", True)
        analysis["mode"] = "initial"
        analysis["gate_id"] = self.spec["initial"]["gate_id"]
        retained = self.root / "forged-initial-analysis.json"
        write_json(retained, analysis)
        output = self.root / "forged-verification.json"
        with self.assertRaises(ValueError):
            ANALYZER.verify_existing(self.root, SPEC_PATH, retained, output)

    def test_duplicate_receipt_key_is_rejected(self) -> None:
        source = self.root / "chunk-000-receipt.json"
        original = source.read_text(encoding="utf-8")
        duplicate_root = self.root / "duplicate-receipt-run"
        duplicate_root.mkdir()
        write_json(duplicate_root / "gate-plan.json", json.loads((self.root / "gate-plan.json").read_text()))
        tampered = original[:-2] + ',"chunk_index":0}\n'
        (duplicate_root / "chunk-000-receipt.json").write_text(tampered, encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "duplicate JSON key"):
            ANALYZER.reconstruct(duplicate_root, SPEC_PATH, "screen")


if __name__ == "__main__":
    unittest.main()
