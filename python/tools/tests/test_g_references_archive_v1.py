"""The superseded-root archive lease's steps, run locally on a fixture tree (no Pod, no provider)."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import time
import unittest

import g_references_archive_v1 as archive
import g_references_qualify_v1 as qualify
import nine_deck_cloud_v1 as cloud

TOOLS = Path(archive.__file__).resolve().parent


def save(path, data):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)
    return {"path": str(path), "sha256": hashlib.sha256(data).hexdigest()}


def pod(program, args):
    """Run a Pod program here, as the controller's Pod.run does over SSH."""
    code = cloud.PRELUDE % json.dumps({**args, "schema": archive.SCHEMA}) + program
    done = subprocess.run([sys.executable, "-B", "-"], input=code.encode(), capture_output=True)
    if done.returncode != 0:
        raise RuntimeError(done.stderr.decode()[-2000:])
    return json.loads(done.stdout.decode().splitlines()[-1])


class ArchiveTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name) / "workspace" / "old"
        self.out = Path(self.temp.name) / "container" / "out"
        campaign = self.root / "campaign"
        self.finals = {}
        for run, block in archive.BASELINES:
            checkpoint = {"state_sha256": f"{run}{block}" * 8, "adam_step": 162 * block, "parameters": [block],
                          "trajectories": [save(campaign / "cold" / run / f"b{block}" / "t.json", b"traj")]}
            item = save(campaign / "checkpoints" / run / f"b{block:03d}.json", json.dumps(checkpoint).encode())
            self.finals[(run, block)] = item
        for run in {run for run, _ in archive.BASELINES}:
            blocks = {str(block): {"final_checkpoint": self.finals[(run, block)], "done": True}
                      for r, block in archive.BASELINES if r == run}
            save(campaign / "state" / f"{run}.json", json.dumps({"run": run, "blocks": blocks}).encode())
        save(campaign / "checkpoints" / "G1" / "ancestry.json", b"x" * 100)  # not a final checkpoint: pruned
        save(campaign / "hot" / "G2" / "b001-a1" / "dispatch" / "report.json", b"{}")
        save(campaign / "hot" / "G2" / "b001-a1" / "native" / "big.bin", b"n" * 1000)  # a native tree: pruned
        save(campaign / "retained" / "G2" / "b01" / "episode.json", b"e" * 50)
        save(campaign / "qualification" / "empty-pool" / "trials-cold" / "w1" / "a.bin", b"c" * 10)
        save(self.root / "tooling" / "abc" / "tool.py", b"print()")  # staged tooling: regenerable
        save(self.root / "control" / "lease-a" / "execute.log", b"log")
        save(self.root / "control" / "lease-a" / "results.tar.gz", b"packed")

    def baselines(self):
        return pod(archive.POD_BASELINES, {"tools": str(TOOLS), "campaign": str(self.root / "campaign"),
                                           "pairs": archive.BASELINES, "out": str(self.out)})

    def pack(self, baselines):
        checkpoints = [row["final_checkpoint"]["path"] for row in baselines["baselines"]]
        return pod(archive.POD_PACK, {"root": str(self.root), "campaign": str(self.root / "campaign"),
                                      "checkpoints": checkpoints, "excluded": list(archive.EXCLUDED_PARTS),
                                      "out": str(self.out)})

    def test_baselines_use_the_qualification_identity(self):
        result = self.baselines()
        self.assertEqual(len(result["baselines"]), 9)
        for row in result["baselines"]:
            expected = qualify.checkpoint_identity(self.finals[(row["run"], row["block"])])
            self.assertEqual(row["identity"], {key: expected[key] for key in qualify.IDENTITY_KEYS})
        (self.root / "campaign" / "state" / "G5.json").unlink()
        missing = [row for row in self.baselines()["baselines"] if row["identity"] is None]
        self.assertEqual([(row["run"], row["block"]) for row in missing], [("G5", 1)])

    def test_pack_keeps_records_and_final_checkpoints_and_lists_the_rest(self):
        packed = self.pack(self.baselines())
        manifest = json.loads((self.out / "manifest.json").read_text())
        listing = json.loads((self.out / "prune-listing.json").read_text())
        kept = {str(Path(item["path"]).relative_to(self.root).as_posix()) for item in manifest["files"]}
        pruned = {str(Path(item["path"]).relative_to(self.root).as_posix()) for item in listing["files"]}
        self.assertIn("campaign/state/G1.json", kept)
        self.assertIn("campaign/hot/G2/b001-a1/dispatch/report.json", kept)
        self.assertIn("campaign/retained/G2/b01/episode.json", kept)
        self.assertIn("campaign/checkpoints/G1/b002.json", kept)
        self.assertIn("control/lease-a/execute.log", kept)
        for name in ("campaign/checkpoints/G1/ancestry.json", "campaign/hot/G2/b001-a1/native/big.bin",
                     "campaign/cold/G1/b1/t.json", "campaign/qualification/empty-pool/trials-cold/w1/a.bin",
                     "tooling/abc/tool.py", "control/lease-a/results.tar.gz"):
            self.assertIn(name, pruned)
        self.assertEqual(len(kept & pruned), 0)
        every = {path.relative_to(self.root).as_posix() for path in self.root.rglob("*") if path.is_file()}
        self.assertEqual(kept | pruned, every)  # every file is archived or listed
        self.assertEqual((packed["included"], packed["listed"]), (len(kept), len(pruned)))

    def test_verify_then_prune_and_any_difference_stops_the_prune(self):
        baselines = self.baselines()
        packed = self.pack(baselines)
        reported = {**packed["files"], "baselines.json": baselines["sha256"]}
        dest = Path(self.temp.name) / "e" / "q1-superseded"
        dest.mkdir(parents=True)
        for name in archive.FILES:
            (dest / name).write_bytes((self.out / name).read_bytes())
        verified = archive.verify_copy(dest, reported)
        self.assertGreater(verified["members"], 0)
        tampered = Path(self.temp.name) / "tampered"
        tampered.mkdir()
        for name in archive.FILES:
            (tampered / name).write_bytes((self.out / name).read_bytes())
        manifest = json.loads((tampered / "manifest.json").read_text())
        manifest["files"][0]["sha256"] = "0" * 64
        (tampered / "manifest.json").write_text(json.dumps(manifest))
        with self.assertRaises(ValueError):  # the manifest no longer hashes as the Pod reported
            archive.verify_copy(tampered, reported)
        with self.assertRaises(ValueError):  # a member differs from the manifest
            archive.verify_copy(tampered, {**reported, "manifest.json": archive.sha256_file(tampered / "manifest.json")})
        prune = {"root": str(self.root), "expected_root": str(self.root), "keep_root": str(self.root.parent / "q2"),
                 "out": str(self.out)}
        with self.assertRaises(RuntimeError):  # a Pod copy that differs from the verified one: nothing pruned
            pod(archive.POD_PRUNE, {**prune, "verified": {**reported, "records.tar.gz": "0" * 64}})
        self.assertTrue(self.root.exists())
        with self.assertRaises(RuntimeError):  # never the campaign being kept
            pod(archive.POD_PRUNE, {**prune, "keep_root": str(self.root), "verified": reported})
        self.assertTrue(self.root.exists())
        self.assertEqual(pod(archive.POD_PRUNE, {**prune, "verified": reported}), {"pruned": True})
        self.assertFalse(self.root.exists())

    def test_the_lease_runs_baselines_copy_verify_then_prune_and_stops_on_a_failed_check(self):
        for failure in (None, "missing baseline", "copy differs"):
            calls = []
            temp = Path(tempfile.mkdtemp())
            self.addCleanup(__import__("shutil").rmtree, temp, True)
            cycle = archive.ArchiveCycle.__new__(archive.ArchiveCycle)
            cycle.control, cycle.result, cycle.dest = temp / "control", {}, temp / "dest"
            cycle.control.mkdir()
            cycle.old, cycle.base = cloud.WORKSPACE / "g-references-v1", cloud.WORKSPACE / "g-references-v1-q2"
            cycle.manifest = {"tooling": {"root": "/workspace/g-references-v1-q2/tooling/abc"}}
            cycle.lease = {"deadline_epoch": time.time() + 7200, "recovery_seconds": 600}
            source = self.out
            baselines, packed = self.baselines(), self.pack(self.baselines())
            if failure == "missing baseline":
                baselines["baselines"][3]["identity"] = None

            class Pod:
                def run(self, program, args, timeout=30, read_only=False):
                    calls.append(program.split("\n", 1)[0])
                    if program is archive.POD_BASELINES:
                        return baselines
                    if program is archive.POD_PACK:
                        return packed
                    return {"pruned": True}

                def get(self, remote, local, timeout=600):
                    calls.append("get")
                    data = (source / Path(remote).name).read_bytes()
                    if failure == "copy differs" and Path(remote).name == "records.tar.gz":
                        data += b"x"
                    Path(local).write_bytes(data)
            cycle.pod = Pod()
            if failure is None:
                cycle.watch()
                self.assertEqual(calls, ["#pod:archive-baselines", "#pod:archive-pack", "get", "get", "get", "get",
                                         "#pod:archive-prune"])
                self.assertTrue(cycle.result["pruned"] and cycle.result["complete"])
                self.assertTrue((cycle.dest / "VERIFIED.json").exists() and (cycle.dest / "PRUNE.json").exists())
            else:
                with self.assertRaises(ValueError):
                    cycle.watch()
                self.assertNotIn("#pod:archive-prune", calls)  # any failed check stops before the prune


if __name__ == "__main__":
    unittest.main()
