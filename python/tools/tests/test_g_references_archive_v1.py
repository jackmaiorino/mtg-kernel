"""The superseded-root archive lease's steps, run locally on a fixture tree (no Pod, no provider)."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time
import unittest
import unittest.mock

import g_references_archive_v1 as archive
import g_references_qualify_v1 as qualify
import nine_deck_cloud_v1 as cloud
from common import read

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
        self.root = (Path(self.temp.name) / "workspace" / "old").resolve()
        self.out = Path(self.temp.name) / "container" / "out"
        campaign = self.root / "campaign"
        self.finals = {}
        for run, block in archive.BASELINES:
            cold = campaign / "cold" / run / f"b{block:03d}-a1"
            checkpoint = {"state_sha256": f"{run}{block}" * 8, "adam_step": 162 * block, "parameters": [block],
                          "trajectories": [save(cold / "native-0.zip", b"traj")]}
            save(cold / "archive.json", b"{}")  # shard hashes and timings: kept
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
        save(campaign / "qualification" / "empty-pool" / "trials-cold" / "w1" / "native-1.zip", b"c" * 10)
        save(self.root / "inputs" / "objects" / "abc" / "campaign.json", b"{}")  # staged and hash-cited: kept
        save(self.root / "tooling" / "abc" / "tool.py", b"print()")
        save(self.root / "control" / "lease-a" / "execute.log", b"log")
        save(self.root / "control" / "lease-a" / "results.tar.gz", b"packed")
        save(self.root / "control" / "lease-a" / "native-0.zip", b"z")  # not in a cold folder: kept

    def baselines(self):
        return pod(archive.POD_BASELINES, {"tools": str(TOOLS), "campaign": str(self.root / "campaign"),
                                           "pairs": archive.BASELINES, "out": str(self.out)})

    def pack(self, baselines, limit=600):
        checkpoints = [row["final_checkpoint"]["path"] for row in baselines["baselines"]]
        return pod(archive.POD_PACK, {"root": str(self.root), "campaign": str(self.root / "campaign"),
                                      "checkpoints": checkpoints, **archive.PACK_RULES, "out": str(self.out),
                                      "limit_seconds": limit})

    def prune_args(self, **changes):
        return {"root": str(self.root), "expected_root": str(self.root), "keep_root": str(self.root.parent / "q2"),
                "workspace": str(self.root.parent), "out": str(self.out)} | changes

    def test_baselines_use_the_qualification_identity(self):
        result = self.baselines()
        self.assertEqual(len(result["baselines"]), 9)
        for row in result["baselines"]:
            expected = qualify.checkpoint_identity(self.finals[(row["run"], row["block"])])
            self.assertEqual(row["identity"], {key: expected[key] for key in qualify.IDENTITY_KEYS})
        (self.root / "campaign" / "state" / "G5.json").unlink()
        missing = [row for row in self.baselines()["baselines"] if row["identity"] is None]
        self.assertEqual([(row["run"], row["block"]) for row in missing], [("G5", 1)])

    def test_pack_keeps_records_and_lists_regenerable_bulk(self):
        packed = self.pack(self.baselines())
        manifest = json.loads((self.out / "manifest.json").read_text())
        listing = json.loads((self.out / "prune-listing.json").read_text())
        kept = {item["path"] for item in manifest["files"]}  # relative to the root, POSIX on every host
        pruned = {item["path"]: item["kind"] for item in listing["files"]}
        for name in ("campaign/state/G1.json", "campaign/hot/G2/b001-a1/dispatch/report.json",
                     "campaign/retained/G2/b01/episode.json", "campaign/checkpoints/G1/b002.json",
                     "campaign/cold/G2/b001-a1/archive.json", "campaign/cold/G1/b001-a1/native-0.zip",
                     "inputs/objects/abc/campaign.json", "tooling/abc/tool.py", "control/lease-a/execute.log",
                     "control/lease-a/results.tar.gz", "control/lease-a/native-0.zip"):
            self.assertIn(name, kept)
        self.assertEqual(pruned, {"campaign/checkpoints/G1/ancestry.json": "checkpoint",
                                  "campaign/hot/G2/b001-a1/native/big.bin": "native",
                                  "campaign/cold/G1/b002-a1/native-0.zip": "shard",
                                  **{f"campaign/cold/{run}/b001-a1/native-0.zip": "shard"
                                     for run in ("G2", "G3", "G4", "G5", "G6", "G7", "G8")},
                                  "campaign/qualification/empty-pool/trials-cold/w1/native-1.zip": "shard"})
        every = {path.relative_to(self.root).as_posix() for path in self.root.rglob("*") if path.is_file()}
        self.assertEqual(kept | set(pruned), every)  # every file is archived or listed
        self.assertEqual(kept & set(pruned), set())
        self.assertEqual((packed["included"], packed["listed"]), (len(kept), len(pruned)))
        self.assertGreater(packed["archive_bytes"], 0)

    def test_the_pack_stops_on_its_time_bound(self):
        with self.assertRaises(RuntimeError):
            self.pack(self.baselines(), limit=0)

    @unittest.skipUnless(sys.platform.startswith("linux"), "links and permissions: POSIX")
    def test_links_or_an_unlistable_folder_stop_the_archive(self):
        os.symlink(self.root / "campaign" / "state", self.root / "campaign" / "linked")
        with self.assertRaises(RuntimeError):
            self.pack(self.baselines())
        os.unlink(self.root / "campaign" / "linked")
        locked = self.root / "campaign" / "exposure"
        save(locked / "rows.jsonl", b"{}")
        os.chmod(locked, 0)
        self.addCleanup(os.chmod, locked, 0o755)
        if os.access(locked, os.R_OK):
            self.skipTest("running as root: permissions do not lock the folder")
        with self.assertRaises(RuntimeError):
            self.pack(self.baselines())

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
        with self.assertRaises(ValueError):  # the manifest no longer hashes as reported
            archive.verify_copy(tampered, reported)
        with self.assertRaises(ValueError):  # a member differs from the manifest
            archive.verify_copy(tampered, {**reported, "manifest.json": archive.sha256_file(tampered / "manifest.json")})
        with self.assertRaises(RuntimeError):  # a Pod copy that differs from the verified one: nothing pruned
            pod(archive.POD_PRUNE, self.prune_args(verified={**reported, "records.tar.gz": "0" * 64}))
        with self.assertRaises(RuntimeError):  # never the campaign being kept
            pod(archive.POD_PRUNE, self.prune_args(keep_root=str(self.root), verified=reported))
        with self.assertRaises(RuntimeError):  # never outside the workspace
            pod(archive.POD_PRUNE, self.prune_args(workspace=str(self.root.parent.parent), verified=reported))
        self.assertTrue(self.root.exists())
        self.assertEqual(pod(archive.POD_PRUNE, self.prune_args(verified=reported)),
                         {"pruned": True, "was_present": True})
        self.assertFalse(self.root.exists())

    def cycle(self, failure, dest=None, resume=False, created=None):
        calls = []
        temp = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, temp, True)
        cycle = archive.ArchiveCycle.__new__(archive.ArchiveCycle)
        cycle.control, cycle.result = temp / "control", {}
        cycle.dest, cycle.budget, cycle.resume = dest or temp / "dest", 10**9, resume
        cycle.control.mkdir()
        cycle.old, cycle.base = cloud.WORKSPACE / "g-references-v1", cloud.WORKSPACE / "g-references-v1-q2"
        cycle.manifest = {"tooling": {"root": "/workspace/g-references-v1-q2/tooling/abc"}}
        cycle.sleep = lambda seconds: None
        created = time.time() - 300 if created is None else created
        cycle.lease = {"created_epoch": created, "deadline_epoch": created + archive.LEASE_HOURS * 3600,
                       "recovery_seconds": archive.LEASE_RECOVERY_SECONDS,
                       "startup_idle_seconds": archive.LEASE_STARTUP_IDLE_SECONDS}
        source, test = self.out, self
        baselines, packed = (None, None) if resume else (self.baselines(), self.pack(self.baselines()))
        if failure == "missing baseline":
            baselines["baselines"][3]["identity"] = None

        labels = {"baselines": "#pod:archive-baselines", "pack": "#pod:archive-pack",
                  "check": "#pod:archive-remaining", "prune": "#pod:archive-prune"}

        class Pod:
            """The detached protocol of a Pod step: a poll finds nothing, the start runs the step here, the next
            poll returns its result."""
            prune_for_real, done, gets = False, {}, 0

            def run(self, program, args, timeout=30, read_only=False):
                if program is archive.POD_DETACH_POLL:
                    name = args["name"]
                    return {"state": "done", "result": self.done[name]} if name in self.done else {"state": "absent"}
                assert program is archive.POD_DETACH_START, program
                name, step = args["name"], args["args"]
                calls.append(labels[name])
                if name == "baselines":
                    result = baselines
                elif name == "pack":
                    result = packed
                elif name == "check":
                    result = pod(archive.POD_REMAINING, {**step, "root": str(test.root)})
                elif self.prune_for_real:
                    result = pod(archive.POD_PRUNE, test.prune_args(verified=step["verified"]))
                else:
                    result = {"pruned": True, "was_present": True}
                self.done[name] = result
                return {"started": True}

            def get(self, remote, local, timeout=600):
                calls.append("get")
                self.gets += 1
                data = (source / Path(remote).name).read_bytes()
                if Path(remote).name == "records.tar.gz" and (
                        failure == "copy differs" or failure == "first copy differs" and self.gets == 1):
                    data += b"x"
                Path(local).write_bytes(data)
        cycle.pod = Pod()
        return cycle, calls

    def test_the_lease_runs_baselines_copy_verify_then_prune_and_stops_on_a_failed_check(self):
        cycle, calls = self.cycle(failure=None)
        cycle.watch()
        self.assertEqual(calls, ["#pod:archive-baselines", "#pod:archive-pack", "get", "get", "get", "get",
                                 "#pod:archive-prune"])
        self.assertTrue(cycle.result["pruned"] and cycle.result["complete"])
        self.assertTrue((cycle.dest / "VERIFIED.json").exists() and (cycle.dest / "PRUNE.json").exists())
        self.assertGreater(read(cycle.dest / "VERIFIED.json")["archive_bytes"], 0)
        retried, calls = self.cycle(failure="first copy differs")
        retried.watch()  # a download that arrives different is fetched again
        self.assertEqual(calls.count("get"), 5)
        self.assertTrue(retried.result["pruned"])
        for failure in ("missing baseline", "copy differs"):
            cycle, calls = self.cycle(failure=failure)
            with unittest.mock.patch.dict(archive.STEP_SECONDS, {"copy": 1}), self.assertRaises(ValueError):
                cycle.watch()
            self.assertNotIn("#pod:archive-prune", calls)  # any failed check stops before the prune
        cycle, calls = self.cycle(failure=None)
        cycle.budget = 1  # over the byte budget: no copy, no prune
        with self.assertRaises(ValueError):
            cycle.watch()
        self.assertEqual(calls, ["#pod:archive-baselines", "#pod:archive-pack"])
        # The pack could not end inside the startup-idle bound (its bound plus the margin, less a minute).
        bound = archive.LEASE_STARTUP_IDLE_SECONDS - archive.STARTUP_MARGIN_SECONDS - archive.STEP_SECONDS["pack"]
        late, calls = self.cycle(failure=None, created=time.time() - bound - 60)
        with self.assertRaisesRegex(ValueError, "startup-idle"):
            late.watch()
        self.assertEqual(calls, ["#pod:archive-baselines"])  # the pack could not end inside the bound

    def test_a_resumed_prune_reverifies_the_copy_and_tolerates_a_partial_root(self):
        cycle, _ = self.cycle(failure=None)
        cycle.watch()  # a full lease (its prune stubbed), to get a verified destination
        (cycle.dest / "PRUNE.json").unlink()  # as if the prune had not finished
        shutil.rmtree(self.root / "campaign" / "hot")  # a partly removed root
        other, calls = self.cycle(failure=None, dest=cycle.dest, resume=True)
        other.old = cloud.WORKSPACE / "another-root"  # a verified archive of one root never prunes another
        with self.assertRaisesRegex(ValueError, "not of"):
            other.watch()
        self.assertEqual(calls, [])
        resumed, calls = self.cycle(failure=None, dest=cycle.dest, resume=True)
        resumed.old = type(cloud.WORKSPACE)(str(self.root))  # the root this archive records
        resumed.pod.prune_for_real = True
        resumed.watch()
        self.assertEqual(calls, ["#pod:archive-remaining", "#pod:archive-prune"])  # no baselines, pack or copy
        self.assertFalse(self.root.exists())
        prune = read(resumed.dest / "PRUNE.json")
        self.assertEqual(prune["pod"], {"pruned": True, "was_present": True})
        self.assertIn("not on a regeneration receipt", prune["ruling"])
        self.assertEqual(set(prune["recipes"]), {"native", "shard", "checkpoint"})
        self.assertIn("campaign/cold/G2/b001-a1/archive.json", prune["cold_archive_records"])
        self.assertEqual(pod(archive.POD_PRUNE, self.prune_args(verified={})),  # absent: the end state
                         {"pruned": True, "was_present": False})
        (resumed.dest / "records.tar.gz").write_bytes(b"changed")
        (resumed.dest / "PRUNE.json").unlink()
        again, calls = self.cycle(failure=None, dest=resumed.dest, resume=True)
        with self.assertRaises(ValueError):  # the E: copy no longer matches VERIFIED.json: no prune
            again.watch()
        self.assertEqual(calls, [])


    def test_a_resumed_prune_refuses_files_the_archive_does_not_account_for(self):
        cycle, _ = self.cycle(failure=None)
        cycle.watch()
        (cycle.dest / "PRUNE.json").unlink()
        save(self.root / "campaign" / "state" / "new-run.json", b"{}")  # written after the archive
        resumed, calls = self.cycle(failure=None, dest=cycle.dest, resume=True)
        resumed.old = type(cloud.WORKSPACE)(str(self.root))
        resumed.pod.prune_for_real = True
        with self.assertRaisesRegex(ValueError, "neither the archive nor the listing"):
            resumed.watch()
        self.assertEqual(calls, ["#pod:archive-remaining"])  # nothing pruned
        self.assertTrue(self.root.exists())
        record = read(cycle.dest / "VERIFIED.json")
        record["files"].pop("baselines.json")
        (cycle.dest / "VERIFIED.json").write_text(json.dumps(record))
        again, calls = self.cycle(failure=None, dest=cycle.dest, resume=True)
        with self.assertRaisesRegex(ValueError, "verification record"):
            again.watch()

    def test_a_pack_at_its_bound_leaves_the_copy_and_the_prune_in_bounds(self):
        """With the launcher's bounds, a pack that ends at its own bound (after 10 min of Pod start and staging and
        the baselines at theirs) still leaves the copy, then the prune, inside the deadline and the startup-idle
        bound."""
        cycle, _ = self.cycle(failure=None)
        now = time.time()
        created = now - 600 - archive.STEP_SECONDS["baselines"] - archive.STEP_SECONDS["pack"]
        cycle.lease = {"created_epoch": created, "deadline_epoch": created + archive.LEASE_HOURS * 3600,
                       "recovery_seconds": archive.LEASE_RECOVERY_SECONDS,
                       "startup_idle_seconds": archive.LEASE_STARTUP_IDLE_SECONDS}
        cycle.in_bounds("copy")
        with unittest.mock.patch.object(archive.time, "time", return_value=now + archive.STEP_SECONDS["copy"]):
            cycle.in_bounds("prune")  # after the longest copy


    def test_a_dropped_poll_is_retried_and_the_step_is_not_started_twice(self):
        cycle, _ = self.cycle(failure=None)
        starts, polls = [], []

        class Flaky:
            def run(self, program, args, timeout=30, read_only=False):
                if program is archive.POD_DETACH_START:
                    starts.append(args["name"])
                    raise cloud.PodUnavailable("the start's answer was lost")  # but the step did start
                polls.append(1)
                if len(polls) == 2:
                    raise cloud.PodUnavailable("server not responding")
                if not starts:
                    return {"state": "absent"}
                return {"state": "done", "result": {"ok": True}} if len(polls) > 3 else {"state": "running",
                                                                                        "progress": {"phase": "walk"}}
        cycle.pod = Flaky()
        self.assertEqual(cycle.step("check", archive.POD_REMAINING, {"root": "/r", "known": []},
                                    progress="x.json"), {"ok": True})
        self.assertEqual(starts, ["check"])  # started once
        self.assertEqual(cycle.result["poll_errors"]["check"], 2)

    @unittest.skipUnless(sys.platform.startswith("linux"), "the detached step runs under sh on the Pod")
    def test_the_detached_programs_on_linux(self):
        out = Path(self.temp.name) / "detach"
        code = cloud.PRELUDE % json.dumps({"value": 7}) + "import time\ntime.sleep(1)\nprint(json.dumps({'v': ARGS['value']}))\n"
        self.assertEqual(pod(archive.POD_DETACH_POLL, {"out": str(out), "name": "job"})["state"], "absent")
        self.assertTrue(pod(archive.POD_DETACH_START, {"out": str(out), "name": "job", "code": code})["started"])
        with self.assertRaises(RuntimeError):  # never started twice
            pod(archive.POD_DETACH_START, {"out": str(out), "name": "job", "code": code})
        self.assertEqual(pod(archive.POD_DETACH_POLL, {"out": str(out), "name": "job"})["state"], "running")
        deadline = time.monotonic() + 30
        while True:
            state = pod(archive.POD_DETACH_POLL, {"out": str(out), "name": "job"})
            if state["state"] != "running" or time.monotonic() > deadline:
                break
            time.sleep(0.2)
        self.assertEqual(state, {"state": "done", "result": {"v": 7}, "progress": None})
        failing = cloud.PRELUDE % json.dumps({}) + "need(False, 'it broke')\n"
        pod(archive.POD_DETACH_START, {"out": str(out), "name": "bad", "code": failing})
        deadline = time.monotonic() + 30
        while pod(archive.POD_DETACH_POLL, {"out": str(out), "name": "bad"})["state"] == "running" \
                and time.monotonic() < deadline:
            time.sleep(0.2)
        state = pod(archive.POD_DETACH_POLL, {"out": str(out), "name": "bad"})
        self.assertEqual(state["state"], "failed")
        self.assertIn("it broke", state["error"])


if __name__ == "__main__":
    unittest.main()
