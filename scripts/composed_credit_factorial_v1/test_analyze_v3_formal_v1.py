import hashlib
import importlib.util
import json
import sys
import tempfile
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("analyze_v3_formal_v1.py")
MODULE_SPEC = importlib.util.spec_from_file_location("analyze_v3_formal_v1_test", MODULE_PATH)
analyzer = importlib.util.module_from_spec(MODULE_SPEC)
assert MODULE_SPEC.loader is not None
sys.modules[MODULE_SPEC.name] = analyzer
MODULE_SPEC.loader.exec_module(analyzer)


class FormalAnalyzerTests(unittest.TestCase):
    def _fixture(self, scores=(0.0, 0.5, 0.0, -0.5)):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            reference = analyzer.load_reference_module()
            first_pair = 4
            first_episode = 8
            clusters = []
            for ordinal, score in enumerate(scores):
                pair_index = first_pair + ordinal
                environment_seed = analyzer._native_environment_seed(7, pair_index)
                legs = (1.0 if score >= 0.5 else -1.0 if score <= -0.5 else 0.0, 0.0)
                records = []
                for seat, leg_score in zip(("p0", "p1"), legs):
                    episode = 2 * pair_index + (0 if seat == "p0" else 1)
                    records.append({
                        "episode_index": episode,
                        "environment_seed": environment_seed,
                        "opponent_component": "primary" if seat == "p0" else "uniform_floor",
                        "parent_return": 0.0,
                        "candidate_return": leg_score,
                        "leg_score": leg_score,
                    })
                clusters.append({"ordinal": ordinal, "pair_index": pair_index, "p0": records[0], "p1": records[1], "cluster_score": score})
            identifiers = []
            for cluster in clusters:
                identifiers.append(
                    "mtg-kernel-native-trainer-schedule-sha256-v2;"
                    f"base_seed=7;pair_index={cluster['pair_index']};"
                    f"episode_p0={cluster['p0']['episode_index']};p0_component={cluster['p0']['opponent_component']};"
                    f"episode_p1={cluster['p1']['episode_index']};p1_component={cluster['p1']['opponent_component']}"
                )
            schedule_sha = reference.canonical_ordered_identifier_sha256(identifiers)
            spec = analyzer.GateSpec(
                mode="initial", base_seed=7, first_episode_index=first_episode, first_pair_index=first_pair,
                max_clusters=len(clusters), pre_outcome_seed_schedule_sha256=schedule_sha,
                candidate_identity={"native_state_sha256": "a" * 64}, parent_identity={"native_state_sha256": "b" * 64},
            )
            chunk = {
                "schema": analyzer.RAW_CHUNK_SCHEMA,
                "mode": "initial",
                "chunk_index": 0,
                "first_cluster_ordinal": 0,
                "cluster_count": len(clusters),
                "clusters": clusters,
            }
            chunk_bytes = (json.dumps(chunk, indent=2, sort_keys=True) + "\n").encode()
            (root / "chunk-000.json").write_bytes(chunk_bytes)
            report = {
                "schema": analyzer.RAW_REPORT_SCHEMA, "mode": "initial", "status": "measurement-complete", "base_seed": 7,
                "reward": "natural-terminal-win-loss-draw-only/v1", "worker_count": 4, "sessions_per_worker": 16,
                "gpu_ordinal": 1,
                "first_episode_index": first_episode, "max_clusters": len(clusters), "observed_clusters": len(clusters),
                "pre_outcome_seed_schedule_sha256": schedule_sha, "candidate": {"native_state_sha256": "a" * 64},
                "parent": {"native_state_sha256": "b" * 64},
                "gate": {
                    "gate_id": "candidate-01-gae-initial", "gate_class": "LARGE-EFFECT",
                    "delta_worthwhile": 0.01, "delta_promote": 0.01, "alpha": 0.00875, "c": 0.5,
                    "conditional_mean_stability": "IID-MIXTURE", "blinded_pilot": "none",
                    "alpha_pool": "candidates", "alpha_consumed_at_launch": True,
                },
                "initial_success_authority": None,
                "chunks": [{"chunk_index": 0, "file_name": "chunk-000.json", "sha256": hashlib.sha256(chunk_bytes).hexdigest(), "first_cluster_ordinal": 0, "cluster_count": len(clusters)}],
                "nonclaims": ["This is not a pro-level or real-MTG claim."],
            }
            run_start = {
                "schema": "mtg-kernel-gae-v3-formal-run-start/v1", "mode": "initial",
                "status": "measurement-started", "base_seed": 7,
                "first_episode_index": first_episode, "max_clusters": len(clusters),
                "pre_outcome_seed_schedule_sha256": schedule_sha,
                "worker_count": 4, "sessions_per_worker": 16, "gpu_ordinal": 1,
                "parent_native_state_sha256": "b" * 64,
                "candidate_native_state_sha256": "a" * 64,
                "gate": report["gate"], "initial_success_authority": None,
            }
            run_start_bytes = (json.dumps(run_start, indent=2, sort_keys=True) + "\n").encode()
            (root / "run-start.json").write_bytes(run_start_bytes)
            report["run_start_sha256"] = hashlib.sha256(run_start_bytes).hexdigest()
            (root / "report.json").write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
            yield root, spec

    def test_valid_path_recomputes_scores_trajectory_and_decision(self):
        for root, spec in self._fixture():
            result = analyzer.analyze_artifact(root, spec=spec)
            self.assertEqual(result["status"], "analysis-complete")
            self.assertEqual(result["observed_clusters"], 4)
            self.assertEqual(result["scores"]["cluster_counts"]["0"], 2)
            self.assertEqual(result["realized_score_stream_sha256"], analyzer.load_reference_module().canonical_stream_sha256([0.0, 0.5, 0.0, -0.5]))
            self.assertEqual(len(result["trajectory"]), 4)
            self.assertIn(result["gate_decision"]["verdict"], {"CONTINUE", "INCONCLUSIVE-AT-MAX-N", "SUCCESS", "HARM", "INFORMATIVE-FUTILITY", "INVALID-EMPTY-CS"})

    def test_chunk_tamper_is_rejected_before_analysis_output(self):
        for root, spec in self._fixture():
            chunk_path = root / "chunk-000.json"
            chunk = json.loads(chunk_path.read_text(encoding="utf-8"))
            chunk["clusters"][0]["p0"]["candidate_return"] = 99.0
            chunk_path.write_text(json.dumps(chunk, indent=2, sort_keys=True) + "\n", encoding="utf-8")
            with self.assertRaises(analyzer.ArtifactValidationError):
                analyzer.analyze_artifact(root, spec=spec)
            self.assertFalse((root / "analysis.json").exists())

    def test_forged_environment_seed_and_fractional_terminal_return_are_rejected(self):
        for root, spec in self._fixture():
            report = json.loads((root / "report.json").read_text(encoding="utf-8"))
            chunk = json.loads((root / "chunk-000.json").read_text(encoding="utf-8"))
            chunk["clusters"][0]["p0"]["environment_seed"] += 1
            chunk["clusters"][0]["p1"]["environment_seed"] += 1
            chunk_bytes = (json.dumps(chunk, indent=2, sort_keys=True) + "\n").encode()
            (root / "chunk-000.json").write_bytes(chunk_bytes)
            report["chunks"][0]["sha256"] = hashlib.sha256(chunk_bytes).hexdigest()
            (root / "report.json").write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
            with self.assertRaises(analyzer.ArtifactValidationError):
                analyzer.analyze_artifact(root, spec=spec)
        for root, spec in self._fixture():
            report = json.loads((root / "report.json").read_text(encoding="utf-8"))
            chunk = json.loads((root / "chunk-000.json").read_text(encoding="utf-8"))
            chunk["clusters"][0]["p0"]["candidate_return"] = 0.5
            chunk_bytes = (json.dumps(chunk, indent=2, sort_keys=True) + "\n").encode()
            (root / "chunk-000.json").write_bytes(chunk_bytes)
            report["chunks"][0]["sha256"] = hashlib.sha256(chunk_bytes).hexdigest()
            (root / "report.json").write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
            with self.assertRaises(analyzer.ArtifactValidationError):
                analyzer.analyze_artifact(root, spec=spec)

    def test_cluster_order_and_identity_tamper_are_rejected(self):
        for root, spec in self._fixture():
            report = json.loads((root / "report.json").read_text(encoding="utf-8"))
            chunk = json.loads((root / "chunk-000.json").read_text(encoding="utf-8"))
            chunk["clusters"][1]["ordinal"] = 3
            chunk_bytes = (json.dumps(chunk, indent=2, sort_keys=True) + "\n").encode()
            (root / "chunk-000.json").write_bytes(chunk_bytes)
            report["chunks"][0]["sha256"] = hashlib.sha256(chunk_bytes).hexdigest()
            (root / "report.json").write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
            with self.assertRaises(analyzer.ArtifactValidationError):
                analyzer.analyze_artifact(root, spec=spec)
        for root, spec in self._fixture():
            report = json.loads((root / "report.json").read_text(encoding="utf-8"))
            report["candidate"]["native_state_sha256"] = "c" * 64
            (root / "report.json").write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
            with self.assertRaises(analyzer.ArtifactValidationError):
                analyzer.analyze_artifact(root, spec=spec)

    def test_score_component_schedule_and_mode_bindings_are_rejected(self):
        for root, spec in self._fixture():
            report = json.loads((root / "report.json").read_text(encoding="utf-8"))
            chunk = json.loads((root / "chunk-000.json").read_text(encoding="utf-8"))
            chunk["clusters"][0]["cluster_score"] = 0.25
            chunk["clusters"][0]["p0"].pop("opponent_component")
            chunk_bytes = (json.dumps(chunk, indent=2, sort_keys=True) + "\n").encode()
            (root / "chunk-000.json").write_bytes(chunk_bytes)
            report["chunks"][0]["sha256"] = hashlib.sha256(chunk_bytes).hexdigest()
            (root / "report.json").write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
            with self.assertRaises(analyzer.ArtifactValidationError):
                analyzer.analyze_artifact(root, spec=spec)
        for root, spec in self._fixture():
            report = json.loads((root / "report.json").read_text(encoding="utf-8"))
            report["pre_outcome_seed_schedule_sha256"] = "d" * 64
            (root / "report.json").write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
            with self.assertRaises(analyzer.ArtifactValidationError):
                analyzer.analyze_artifact(root, spec=spec)
        for root, spec in self._fixture():
            report = json.loads((root / "report.json").read_text(encoding="utf-8"))
            report["first_episode_index"] = 10
            (root / "report.json").write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
            with self.assertRaises(analyzer.ArtifactValidationError):
                analyzer.analyze_artifact(root, spec=spec)

    def test_analysis_output_is_create_exclusive(self):
        for root, spec in self._fixture():
            analyzer.analyze_artifact(root, spec=spec)
            with self.assertRaises(analyzer.ArtifactValidationError):
                analyzer.analyze_artifact(root, spec=spec)

    def test_gate_constants_and_initial_authority_are_bound(self):
        for root, spec in self._fixture():
            report = json.loads((root / "report.json").read_text(encoding="utf-8"))
            report["gate"]["alpha"] = 0.05
            (root / "report.json").write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
            with self.assertRaises(analyzer.ArtifactValidationError):
                analyzer.analyze_artifact(root, spec=spec)
        for root, spec in self._fixture():
            report = json.loads((root / "report.json").read_text(encoding="utf-8"))
            run_start = json.loads((root / "run-start.json").read_text(encoding="utf-8"))
            run_start["gate"]["alpha"] = 0.05
            run_start_bytes = (json.dumps(run_start, indent=2, sort_keys=True) + "\n").encode()
            (root / "run-start.json").write_bytes(run_start_bytes)
            report["run_start_sha256"] = hashlib.sha256(run_start_bytes).hexdigest()
            (root / "report.json").write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
            with self.assertRaises(analyzer.ArtifactValidationError):
                analyzer.analyze_artifact(root, spec=spec)

    def test_existing_analysis_verifier_recomputes_everything(self):
        for root, spec in self._fixture(scores=(0.5, 0.5, 0.5, 0.5)):
            retained = analyzer.analyze_artifact(root, spec=spec)
            # The injected four-cluster fixture is not expected to cross, so
            # exact recomputation must reject it as confirmation authority.
            self.assertNotEqual(retained["gate_decision"]["verdict"], "SUCCESS")
            with self.assertRaises(analyzer.ArtifactValidationError):
                analyzer.verify_existing_analysis(root, root / "analysis.json", spec=spec)
            retained["gate_decision"]["verdict"] = "SUCCESS"
            (root / "analysis.json").write_text(
                json.dumps(retained, indent=2, sort_keys=True) + "\n",
                encoding="utf-8",
            )
            with self.assertRaises(analyzer.ArtifactValidationError):
                analyzer.verify_existing_analysis(root, root / "analysis.json", spec=spec)
        for root, spec in self._fixture():
            report = json.loads((root / "report.json").read_text(encoding="utf-8"))
            report["initial_success_authority"] = {"verdict": "SUCCESS"}
            (root / "report.json").write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
            with self.assertRaises(analyzer.ArtifactValidationError):
                analyzer.analyze_artifact(root, spec=spec)


if __name__ == "__main__":
    unittest.main()
