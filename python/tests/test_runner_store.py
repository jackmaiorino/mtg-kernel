from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path

from mtg_kernel_rl.artifact_io import canonical_json_bytes, read_json_file, sha256_bytes
from mtg_kernel_rl.artifacts import set_fault_injector
from mtg_kernel_rl.rollout import run_episodes
from mtg_kernel_rl.runner_store import RUN_SCHEMA, validate_runner_artifacts

from fixtures import fake_launcher


def _uniform_run(root: Path, name: str = "runner") -> Path:
    launcher = fake_launcher(root, f"{name}-env")
    out = root / name
    run_episodes(
        env_bin=launcher,
        out_dir=out,
        episodes=2,
        base_seed=71_501,
        max_physical_decisions=8,
        max_policy_steps=16,
        p0="uniform",
        p1="uniform",
    )
    return out


class RunnerStoreTest(unittest.TestCase):
    def test_independent_validator_recomputes_rows_aggregate_and_file_binding(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_name:
            root = Path(tmp_name)
            out = _uniform_run(root)
            receipt = validate_runner_artifacts(out)
            manifest = read_json_file(out / "run.json")
            self.assertEqual(manifest["schema"], RUN_SCHEMA)
            self.assertEqual(receipt.episode_count, 2)
            self.assertEqual(receipt.policy_head, None)
            self.assertEqual(receipt.p0_wins + receipt.p1_wins + receipt.draws, 2)
            self.assertEqual(
                manifest["files"]["episodes.jsonl"]["sha256"],
                sha256_bytes((out / "episodes.jsonl").read_bytes()),
            )
            self.assertEqual(
                manifest["publication"]["data_files_published_first"],
                ["episodes.jsonl"],
            )

    def test_validator_rejects_file_hash_drift(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_name:
            out = _uniform_run(Path(tmp_name))
            manifest = read_json_file(out / "run.json")
            manifest["files"]["episodes.jsonl"]["sha256"] = "0" * 64
            (out / "run.json").write_bytes(canonical_json_bytes(manifest))
            with self.assertRaisesRegex(ValueError, "episodes hash mismatch"):
                validate_runner_artifacts(out)

    def test_validator_rejects_semantic_row_drift_even_when_file_metadata_is_repaired(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_name:
            out = _uniform_run(Path(tmp_name))
            rows = [json.loads(line) for line in (out / "episodes.jsonl").read_text(encoding="ascii").splitlines()]
            rows[0]["env_seed"] += 1
            episode_data = b"".join(canonical_json_bytes(row) for row in rows)
            (out / "episodes.jsonl").write_bytes(episode_data)
            manifest = read_json_file(out / "run.json")
            manifest["files"]["episodes.jsonl"] = {
                "row_count": len(rows),
                "sha256": sha256_bytes(episode_data),
                "size_bytes": len(episode_data),
            }
            (out / "run.json").write_bytes(canonical_json_bytes(manifest))
            with self.assertRaisesRegex(ValueError, "environment seed mismatch"):
                validate_runner_artifacts(out)

    def test_publication_fault_before_authoritative_metadata_leaves_no_committed_run(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_name:
            root = Path(tmp_name)
            launcher = fake_launcher(root, "fault-env")
            out = root / "fault-runner"

            def fail_before_run(boundary: str, _path: Path | None) -> None:
                if boundary == "runner_run_publish_before":
                    raise RuntimeError("injected metadata publication failure")

            previous = set_fault_injector(fail_before_run)
            try:
                with self.assertRaisesRegex(RuntimeError, "metadata publication failure"):
                    run_episodes(
                        env_bin=launcher,
                        out_dir=out,
                        episodes=1,
                        base_seed=71_501,
                        max_physical_decisions=8,
                        max_policy_steps=16,
                        p0="uniform",
                        p1="uniform",
                    )
            finally:
                set_fault_injector(previous)
            self.assertTrue((out / "episodes.jsonl").is_file())
            self.assertFalse((out / "run.json").exists())
            with self.assertRaisesRegex(ValueError, "runner root entries mismatch"):
                validate_runner_artifacts(out)


if __name__ == "__main__":
    unittest.main()
