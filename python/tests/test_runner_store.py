from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from mtg_kernel_rl.artifact_io import canonical_json_bytes, read_json_file, sha256_bytes
from mtg_kernel_rl.artifacts import set_fault_injector
from mtg_kernel_rl.output_lock import OutputLock
from mtg_kernel_rl.path_safety import OUTPUT_LOCK_FILE_NAME
from mtg_kernel_rl.rollout import run_episodes
from mtg_kernel_rl.runner_store import RUN_SCHEMA, validate_runner_artifacts
from mtg_kernel_rl.trainer import train
from mtg_kernel_rl.training_store import TrainingStore

from fixtures import fake_launcher


REPO_ROOT = Path(__file__).resolve().parents[2]


_HARD_EXIT_RUNNER = r"""
import os
import sys
from pathlib import Path

from mtg_kernel_rl.artifacts import set_fault_injector
from mtg_kernel_rl.rollout import run_episodes

launcher, out, wanted_boundary, marker = sys.argv[1:]
marker_path = Path(marker)

def injector(boundary, _path):
    if boundary == wanted_boundary:
        with marker_path.open("xb") as handle:
            handle.write(b"reached")
            handle.flush()
            os.fsync(handle.fileno())
        os._exit(91)

set_fault_injector(injector)
run_episodes(
    env_bin=Path(launcher),
    out_dir=Path(out),
    episodes=1,
    base_seed=71501,
    max_physical_decisions=8,
    max_policy_steps=16,
    p0="uniform",
    p1="uniform",
)
raise SystemExit(92)
"""


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


def _run_uniform_to(launcher: Path, out: Path) -> None:
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


def _trained_run(root: Path) -> Path:
    launcher = fake_launcher(root, "trained-runner-env")
    store = root / "training-store"
    train(
        env_bin=launcher,
        out_dir=store,
        base_seed=71_501,
        until_update=1,
        batch_episodes=2,
        learning_rate=0.001,
        value_coef=0.5,
        max_physical_decisions=8,
        max_policy_steps=16,
    )
    head = TrainingStore(store).validate_latest().head.head
    out = root / "trained-runner"
    run_episodes(
        env_bin=launcher,
        out_dir=out,
        episodes=1,
        base_seed=71_501,
        max_physical_decisions=8,
        max_policy_steps=16,
        p0="greedy",
        p1="uniform",
        training_store=store,
        expected_policy_head=head,
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

    def test_validator_rejects_policy_source_snapshot_source_training_and_aggregate_corruption(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_name:
            root = Path(tmp_name)
            pristine = _trained_run(root)
            manifest = read_json_file(pristine / "run.json")
            self.assertEqual(manifest["policy_source"]["snapshot"]["update"], 1)

            cases = (
                (
                    "policy-source",
                    lambda value: value["policy_source"].__setitem__("mode", "none_uniform_only"),
                    "neural runner policies require a validated training head",
                ),
                (
                    "snapshot",
                    lambda value: value["policy_source"]["snapshot"].__setitem__("update", 0),
                    "policy_source.snapshot.update must be [1, 1000000]",
                ),
                (
                    "source-training",
                    lambda value: value["policy_source"]["source_training"]["run"].__setitem__("schema", "bad"),
                    "source training run schema mismatch",
                ),
                (
                    "aggregate",
                    lambda value: value["aggregate"].__setitem__("draws", value["aggregate"]["draws"] + 1),
                    "runner aggregate is not derived from episode rows",
                ),
            )
            for name, mutate, expected_error in cases:
                with self.subTest(name=name):
                    target = root / name
                    shutil.copytree(pristine, target)
                    corrupted = read_json_file(target / "run.json")
                    mutate(corrupted)
                    (target / "run.json").write_bytes(canonical_json_bytes(corrupted))
                    with self.assertRaises(ValueError) as raised:
                        validate_runner_artifacts(target)
                    self.assertEqual(str(raised.exception), expected_error)

    def test_validator_rejects_wrong_type_hardlink_and_noncanonical_runner_artifacts(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_name:
            root = Path(tmp_name)
            pristine = _uniform_run(root, "pristine-topology")

            wrong_type = root / "wrong-type"
            shutil.copytree(pristine, wrong_type)
            (wrong_type / "episodes.jsonl").unlink()
            (wrong_type / "episodes.jsonl").mkdir()
            with self.assertRaisesRegex(ValueError, "artifact path must be a regular file"):
                validate_runner_artifacts(wrong_type)

            hardlinked = root / "hardlinked"
            shutil.copytree(pristine, hardlinked)
            sentinel = root / "hardlink-sentinel.json"
            sentinel.write_bytes((hardlinked / "run.json").read_bytes())
            expected_sentinel = sentinel.read_bytes()
            (hardlinked / "run.json").unlink()
            try:
                os.link(sentinel, hardlinked / "run.json")
            except OSError as exc:
                self.skipTest(f"hardlink unavailable on temporary filesystem: {exc}")
            with self.assertRaisesRegex(ValueError, "artifact file must not be hardlinked"):
                validate_runner_artifacts(hardlinked)
            self.assertEqual(sentinel.read_bytes(), expected_sentinel)

            noncanonical = root / "noncanonical"
            shutil.copytree(pristine, noncanonical)
            noncanonical_manifest = read_json_file(noncanonical / "run.json")
            (noncanonical / "run.json").write_text(json.dumps(noncanonical_manifest, indent=2), encoding="ascii")
            with self.assertRaises(ValueError) as raised:
                validate_runner_artifacts(noncanonical)
            self.assertEqual(
                str(raised.exception),
                "JSON artifact run.json is not canonical sorted ASCII JSON",
            )

    def test_validator_rejects_runner_artifact_symlink_without_touching_target(self) -> None:
        if not hasattr(os, "symlink"):
            self.skipTest("file symlink primitive unavailable")
        with tempfile.TemporaryDirectory() as tmp_name:
            root = Path(tmp_name)
            pristine = _uniform_run(root, "pristine-symlink")
            target = root / "symlinked"
            shutil.copytree(pristine, target)
            sentinel = root / "symlink-sentinel.json"
            sentinel.write_bytes((target / "run.json").read_bytes())
            expected_sentinel = sentinel.read_bytes()
            (target / "run.json").unlink()
            try:
                os.symlink(sentinel, target / "run.json")
            except (NotImplementedError, OSError) as exc:
                self.skipTest(f"file symlink unavailable: {exc}")
            with self.assertRaisesRegex(
                ValueError,
                r"^artifact tree contains link/reparse entry: run\.json$",
            ):
                validate_runner_artifacts(target)
            self.assertEqual(sentinel.read_bytes(), expected_sentinel)

    def test_all_runner_publication_fault_boundaries_are_fail_closed_and_recoverable(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_name:
            root = Path(tmp_name)
            launcher = fake_launcher(root, "fault-boundary-env")
            cases = (
                ("runner_episodes_publish_before", {OUTPUT_LOCK_FILE_NAME}, False),
                ("runner_episodes_publish_after", {OUTPUT_LOCK_FILE_NAME, "episodes.jsonl"}, False),
                ("runner_run_publish_before", {OUTPUT_LOCK_FILE_NAME, "episodes.jsonl"}, False),
                ("runner_run_publish_after", {OUTPUT_LOCK_FILE_NAME, "episodes.jsonl", "run.json"}, True),
            )
            for boundary, expected_entries, committed in cases:
                with self.subTest(boundary=boundary):
                    out = root / boundary
                    fired = False

                    def injector(actual: str, _path: Path | None) -> None:
                        nonlocal fired
                        if actual == boundary:
                            fired = True
                            raise RuntimeError(f"injected {boundary}")

                    previous = set_fault_injector(injector)
                    try:
                        with self.assertRaises(RuntimeError) as raised:
                            _run_uniform_to(launcher, out)
                    finally:
                        set_fault_injector(previous)
                    self.assertTrue(fired)
                    self.assertEqual(str(raised.exception), f"injected {boundary}")
                    self.assertEqual({path.name for path in out.iterdir()}, expected_entries)
                    if committed:
                        self.assertEqual(validate_runner_artifacts(out).episode_count, 1)
                        with self.assertRaises(FileExistsError):
                            _run_uniform_to(launcher, out)
                        self.assertEqual({path.name for path in out.iterdir()}, expected_entries)
                    else:
                        self.assertFalse((out / "run.json").exists())
                        with self.assertRaisesRegex(ValueError, "runner root entries mismatch"):
                            validate_runner_artifacts(out)
                        if (out / "episodes.jsonl").exists():
                            with self.assertRaises(FileExistsError):
                                _run_uniform_to(launcher, out)
                            (out / "episodes.jsonl").unlink()
                        _run_uniform_to(launcher, out)
                        self.assertEqual(validate_runner_artifacts(out).episode_count, 1)

    def test_hard_process_kill_before_authoritative_runner_metadata_is_recoverable(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_name:
            root = Path(tmp_name)
            launcher = fake_launcher(root, "hard-kill-env")
            out = root / "hard-kill-runner"
            marker = root / "hard-kill.marker"
            source_root = Path(__file__).resolve().parents[1]
            tests_root = Path(__file__).resolve().parent
            child_env = os.environ.copy()
            child_env["PYTHONPATH"] = os.pathsep.join((str(source_root), str(tests_root)))
            completed = subprocess.run(
                [
                    sys.executable,
                    "-c",
                    _HARD_EXIT_RUNNER,
                    str(launcher),
                    str(out),
                    "runner_run_publish_before",
                    str(marker),
                ],
                cwd=REPO_ROOT,
                env=child_env,
                capture_output=True,
                text=True,
                timeout=60,
                check=False,
            )
            self.assertEqual(
                completed.returncode,
                91,
                msg=f"stdout={completed.stdout!r} stderr={completed.stderr!r}",
            )
            self.assertEqual(marker.read_bytes(), b"reached")
            self.assertEqual({path.name for path in out.iterdir()}, {OUTPUT_LOCK_FILE_NAME, "episodes.jsonl"})
            with OutputLock(out):
                pass
            with self.assertRaisesRegex(ValueError, "runner root entries mismatch"):
                validate_runner_artifacts(out)
            with self.assertRaises(FileExistsError):
                _run_uniform_to(launcher, out)
            (out / "episodes.jsonl").unlink()
            _run_uniform_to(launcher, out)
            self.assertEqual(validate_runner_artifacts(out).episode_count, 1)


if __name__ == "__main__":
    unittest.main()
