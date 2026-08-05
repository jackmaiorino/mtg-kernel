from __future__ import annotations

import argparse
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
    _validate_log_markers,
)


class CollectCorpusV4Test(unittest.TestCase):
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
