from __future__ import annotations

import argparse
import copy
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from collect_corpus_v4 import (
    _anchor_command,
    _chunk_ranges,
    _popen_group_options,
    _resource_summary,
    _terminate_process_tree,
    _validate_collection_prerequisites,
    _validate_screen_and_identity_evidence,
    _validate_log_markers,
)
from compare_revealed_identity_v4 import compare
from _test_support import shard_rows, write_rows


class CollectCorpusV4Test(unittest.TestCase):
    BASE_SEED = 1_820_001

    def make_screen_evidence(
        self,
        root: Path,
        *,
        rate: float = 0.8,
        workers: int = 8,
        task_pairs: int = 4,
        available_memory: float = 32 * 1024**3,
    ) -> tuple[Path, Path, Path]:
        baseline_root = root / "baseline" / "tasks"
        screen_root = root / "screen"
        candidate_root = screen_root / "tasks"
        baseline_root.mkdir(parents=True)
        candidate_root.mkdir(parents=True)
        per_pair = []
        for pair in range(32):
            rows = shard_rows(pair, self.BASE_SEED)
            write_rows(baseline_root / f"gae8-pair-{pair:04d}.outcome.jsonl", rows)
            per_pair.append(rows)

        batched = [copy.deepcopy(per_pair[0][0])]
        record_ordinal = 1
        decision_offset = 0
        for rows in per_pair:
            for source in rows[1:]:
                row = copy.deepcopy(source)
                row["record_ordinal"] = record_ordinal
                record_ordinal += 1
                if row["record_type"] == "decision":
                    row["outcome_decision_ordinal"] += decision_offset
                else:
                    row["first_outcome_decision_ordinal"] += decision_offset
                batched.append(row)
            decision_offset += 2
        write_rows(candidate_root / "gae8-p000000-n032.outcome.jsonl", batched)

        (baseline_root.parent / "report.json").write_text("{}\n", encoding="utf-8")
        analysis_policy = {
            "outcome_based_early_analysis": False,
            "terminal_win_draw_loss_only": True,
        }
        panel = {
            "arm": "gae8",
            "opponent": "xmage-cp7",
            "cp7_skill": 7,
            "base_seed": self.BASE_SEED,
            "pair_start": 0,
            "pair_count": 32,
            "episode_count": 64,
            "workers": workers,
            "task_pairs": task_pairs,
        }
        manifest_path = screen_root / "manifest.json"
        manifest = {
            "schema": "mtg-kernel-current-net8-cp7-terminal-response-v4-manifest/v1",
            "panel": panel,
            "output_root": str(screen_root),
            "collection_prerequisites": None,
            "analysis_policy": analysis_policy,
        }
        manifest_path.write_text(json.dumps(manifest), encoding="utf-8")
        sample = {
            "elapsed_seconds": 0.0,
            "system_cpu_percent": 50.0,
            "process_tree_rss_bytes": 1024.0,
            "system_memory_total_bytes": 128 * 1024**3,
            "system_memory_available_bytes": available_memory,
            "system_memory_used_percent": 50.0,
            "gpu_utilization_percent": 0.0,
            "gpu_memory_used_mib": 9.0,
            "gpu_memory_total_mib": 6144.0,
        }
        sample_path = screen_root / "resource_samples.jsonl"
        sample_path.write_text(json.dumps(sample) + "\n", encoding="utf-8")
        resources = _resource_summary([sample])
        screen_path = screen_root / "report.json"
        screen = {
            "schema": "mtg-kernel-current-net8-cp7-terminal-response-v4-collection/v1",
            "status": "complete",
            "manifest": {
                "path": str(manifest_path),
                "sha256": hashlib.sha256(manifest_path.read_bytes()).hexdigest(),
            },
            "panel": panel,
            "elapsed_seconds": 64 / rate,
            "games_per_second": rate,
            "resource_usage": resources,
            "resource_samples": {
                "path": str(sample_path),
                "sha256": hashlib.sha256(sample_path.read_bytes()).hexdigest(),
                "byte_count": sample_path.stat().st_size,
            },
            "analysis_policy": analysis_policy,
        }
        screen_path.write_text(json.dumps(screen), encoding="utf-8")
        identity_path = screen_root / "identity.json"
        compare(
            baseline_root=baseline_root,
            candidate_root=candidate_root,
            report_path=identity_path,
            base_seed=self.BASE_SEED,
            first_pair=0,
            pair_count=32,
        )
        return baseline_root, screen_path, identity_path

    def test_chunk_ranges_keep_multiple_pairs_per_task(self) -> None:
        self.assertEqual(_chunk_ranges(3, 9, 4), [(3, 4), (7, 4), (11, 1)])

    def test_anchor_command_is_one_arm_and_batched(self) -> None:
        args = argparse.Namespace(
            mage_repo=Path(r"C:\repo\mage-kernel-anchor-spike-v1"),
            scorer_exe=Path(r"C:\bin\scorer.exe"),
            maven=Path(r"C:\maven\mvn.cmd"),
            base_seed=1930001,
        )
        package = {"root": r"D:\packages\gae8"}
        command = _anchor_command(args, package, 32, 16, Path(r"D:\out\shard.jsonl"))
        wire = " ".join(command)
        self.assertIn("XMageRallyAnchorSpike", wire)
        self.assertIn("--first-episode 64", wire)
        self.assertIn("--pairs 16", wire)
        self.assertIn("--opponent cp7", wire)
        self.assertNotIn("gae16", wire.lower())

    def test_multiple_pair_markers_are_strict(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            log = Path(temporary) / "task.log"
            log.write_text(
                "XMAGE_RALLY_ANCHOR_PAIR PASS base_seed=9 opponent=cp7 cp7_skill=7 "
                "episodes=4,5 pair_index=2 candidate_seats=p0,p1 winners=p0,p1\n"
                "XMAGE_RALLY_ANCHOR_PAIR PASS base_seed=9 opponent=cp7 cp7_skill=7 "
                "episodes=6,7 pair_index=3 candidate_seats=p0,p1 winners=p1,p0\n"
                "XMAGE_RALLY_ANCHOR_SPIKE PASS base_seed=9 opponent=cp7 cp7_skill=7 "
                "first_episode=4 pairs=2 games=4 candidate_wins=2\n",
                encoding="utf-8",
                newline="\n",
            )
            _validate_log_markers(log, base_seed=9, first_pair=2, pair_count=2)

    def test_resource_summary_reports_all_required_resources(self) -> None:
        summary = _resource_summary(
            [
                {
                    "elapsed_seconds": 0.0,
                    "system_cpu_percent": 80.0,
                    "process_tree_rss_bytes": 10.0,
                    "system_memory_total_bytes": 100.0,
                    "system_memory_available_bytes": 60.0,
                    "system_memory_used_percent": 40.0,
                    "gpu_utilization_percent": 0.0,
                    "gpu_memory_used_mib": 9.0,
                    "gpu_memory_total_mib": 1000.0,
                }
            ]
        )
        self.assertIn("system_cpu_percent", summary)
        self.assertIn("process_tree_rss_bytes", summary)
        self.assertIn("system_memory_available_bytes_minimum", summary)
        self.assertIn("gpu_1_utilization_percent", summary)

    def test_formal_collection_requires_passing_screen_and_identity(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            baseline_root, screen_path, identity_path = self.make_screen_evidence(root)
            args = argparse.Namespace(
                base_seed=1_930_001,
                pair_start=0,
                pairs=256,
                workers=8,
                throughput_screen_report=screen_path,
                revealed_identity_report=identity_path,
                topology_selection_report=None,
            )
            evidence = _validate_screen_and_identity_evidence(
                screen_path, identity_path, baseline_root=baseline_root
            )
            self.assertEqual(evidence["throughput_screen_report"]["games_per_second"], 0.8)
            expected_paths = {
                "attempt-01": (screen_path, identity_path),
                "attempt-02": (root / "unused-screen.json", root / "unused-identity.json"),
            }
            admitted = _validate_collection_prerequisites(
                args,
                baseline_root=baseline_root,
                expected_topology_paths=expected_paths,
            )
            self.assertNotIn("topology_selection_report", admitted)
            args.revealed_identity_report = None
            with self.assertRaisesRegex(RuntimeError, "requires throughput-screen"):
                _validate_collection_prerequisites(args)

    def test_above_trigger_alternative_still_requires_selection_report(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            baseline_root, screen_path, identity_path = self.make_screen_evidence(
                root, rate=0.7, workers=16, task_pairs=2
            )
            args = argparse.Namespace(
                base_seed=1_930_001,
                pair_start=0,
                pairs=256,
                workers=16,
                throughput_screen_report=screen_path,
                revealed_identity_report=identity_path,
                topology_selection_report=None,
            )
            expected_paths = {
                "attempt-01": (root / "unused-screen.json", root / "unused-identity.json"),
                "attempt-02": (screen_path, identity_path),
            }
            with self.assertRaisesRegex(RuntimeError, "requires the one-alternative"):
                _validate_collection_prerequisites(
                    args,
                    baseline_root=baseline_root,
                    expected_topology_paths=expected_paths,
                )

    def test_full_identity_evidence_rejects_pair_digest_tamper(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            baseline_root, screen_path, identity_path = self.make_screen_evidence(root)
            identity = json.loads(identity_path.read_text(encoding="utf-8"))
            identity["pairs"][3]["normalized_sha256"] = "0" * 64
            identity_path.write_text(json.dumps(identity), encoding="utf-8")
            with self.assertRaisesRegex(RuntimeError, "per-pair evidence mismatch"):
                _validate_screen_and_identity_evidence(
                    screen_path, identity_path, baseline_root=baseline_root
                )

    def test_screen_resource_gate_rejects_low_available_memory(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            baseline_root, screen_path, identity_path = self.make_screen_evidence(
                root, available_memory=16 * 1024**3 - 1
            )
            with self.assertRaisesRegex(RuntimeError, "resource and panel gate"):
                _validate_screen_and_identity_evidence(
                    screen_path, identity_path, baseline_root=baseline_root
                )

    def test_revealed_screen_cannot_claim_prior_gate_evidence(self) -> None:
        args = argparse.Namespace(
            base_seed=1_820_001,
            pair_start=0,
            pairs=32,
            workers=8,
            throughput_screen_report=None,
            revealed_identity_report=None,
            topology_selection_report=None,
        )
        self.assertIsNone(_validate_collection_prerequisites(args))

    @unittest.skipUnless(os.name == "nt", "Windows process-tree contract")
    def test_windows_process_tree_termination_checks_descendant_liveness(self) -> None:
        root = subprocess.Popen(
            [
                sys.executable,
                "-c",
                (
                    "import subprocess,sys,time; "
                    "p=subprocess.Popen([sys.executable,'-c','import time;time.sleep(60)']); "
                    "print(p.pid,flush=True); time.sleep(60)"
                ),
            ],
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            **_popen_group_options(),
        )
        try:
            child_line = root.stdout.readline() if root.stdout is not None else ""
            self.assertTrue(child_line.strip().isdigit())
            _terminate_process_tree(root)
            self.assertIsNotNone(root.poll())
        finally:
            if root.poll() is None:
                _terminate_process_tree(root)
            if root.stdout is not None:
                root.stdout.close()


if __name__ == "__main__":
    unittest.main()
