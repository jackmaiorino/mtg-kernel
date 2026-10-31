"""Offline coverage of the G(i) cloud controller's own parts: packaging, the launch command, statuses,
recovery scope and the study's lease caps. The shared lease cycle is covered by test_nine_deck_cloud_v1."""
import hashlib
import json
import os
import re
import subprocess
import sys
import tarfile
import tempfile
import time
import unittest
from pathlib import Path
from unittest.mock import patch

import g_references_campaign_v1 as driver
import g_references_cloud_v1 as gcloud
import g_references_v1 as g
import host_slots_v1 as slots
import native_expanded_dispatch_v1 as dispatch
import nine_deck_cloud_v1 as cloud
from common import read, write
from test_nine_deck_cloud_v1 import FAKE_ELF, pinned

REPO = Path(__file__).resolve().parents[3]
ROOT = "g-references-test"


class Fixture:
    def __init__(self, root):
        self.root = Path(root)
        local = self.root / "local"
        imports = {}
        for run in g.RUNS:
            init = pinned(local / run / "initialization.json", json.dumps({"lineage_id": run}).encode())
            params = pinned(local / run / "parameters.f32le", run.encode() * 64)
            imports[run] = pinned(local / f"play-import-{run}.json", json.dumps(
                {"initialization": init, "parameters": params, "schema": g.FRESH_SOURCE_SCHEMA}).encode())
        self.template = {"recipe": g.PREFIX, "lane": "gi", "decks": str(REPO / "data" / "runtime_decks_v1.json"),
                         "play_imports": imports, "placement": {"host": "desktop", "workers": 4,
                                                                 "preparation_workers": 4},
                         "runtime": {"path": "C:/r.json", "sha256": "0" * 64}, "storage": {}, "wall_seconds": 7200,
                         "cold_keep_blocks": 1, "roots": {"state": "C:/s"}}
        write(local / "campaign.json", self.template)
        self.binary = local / "bin" / "native_expanded_training_run_v1"
        self.binary.parent.mkdir(parents=True)
        self.binary.write_bytes(FAKE_ELF)
        evidence = pinned(local / "census.json", b'{"census": 1}')
        now = "2026-10-28T20:00:00+00:00"
        self.inventory = local / "inventory.json"
        write(self.inventory, {"inventory": {
            "runpod": {"eligible": True, "reason": "this lease", "checked_at": now, "evidence": evidence,
                       "cpu_affinity": list(range(32)), "transport_seconds": 0},
            "desktop": {"eligible": False, "reason": "qualified separately", "checked_at": now, "evidence": evidence},
            "computehost": {"eligible": False, "reason": "3x slower", "checked_at": now, "evidence": evidence}}})

    def spec(self, **changes):
        value = {"schema": gcloud.SPEC_SCHEMA, "mode": "qualification", "campaign_root": ROOT,
                 "campaign": str(self.root / "local/campaign.json"), "runs": list(g.RUNS),
                 "slots": [{"cpu_affinity": [4 * i + k for k in range(4)], "workers": 4, "preparation_workers": 4,
                            "memory_bytes": 6 << 30} for i in range(8)],
                 "native_root": "/root/g-native", "stop_after_block": 2,
                 "storage": {"max_logical_bytes": 120 * 10**9, "reserve_bytes": 60 << 30,
                             "projected_additional_bytes": 20 << 30},
                 "scratch": {"block_peak_bytes": 14 * 10**9, "factor": 1.25, "margin_fraction": 0.1},
                 "emergency": {"volume_bytes": 200 * 10**9, "margin_fraction": 0.1},
                 "records": {"window_blocks": 8, "stall_seconds": 2700},
                 "runtime": {"binary": str(self.binary), "engine_commit": "a" * 40, "tracked_tree_sha256": "b" * 64},
                 "hardware": {"min_memory_bytes": 60 << 30}, "inventory": str(self.inventory)}
        value.update(changes)
        path = self.root / f"spec-{time.monotonic_ns()}.json"
        write(path, value)
        return path


class PackageTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.fixture = Fixture(self.temp.name)

    def test_qualification_package(self):
        out = Path(self.temp.name) / "package"
        manifest = gcloud.package(self.fixture.spec(), out)
        self.assertEqual(gcloud.verified_package(out)["mode"], "qualification")
        campaign = json.loads((out / "campaign.json").read_text())
        self.assertEqual(campaign["recipe"], g.PREFIX)
        self.assertEqual(campaign["stop_after_block"], 2)
        self.assertEqual(campaign["roots"]["native"], "/root/g-native")
        self.assertTrue(campaign["roots"]["checkpoints"].startswith(f"/workspace/{ROOT}/campaign/"))
        self.assertEqual(campaign["storage"]["accounting_roots"], [f"/workspace/{ROOT}/campaign"])
        self.assertEqual(campaign["storage"]["scratch_roots"], ["/root/g-native"])  # amendment 2: block scratch
        self.assertEqual(campaign["scratch"], {"block_peak_bytes": 14 * 10**9, "factor": 1.25, "margin_fraction": 0.1})
        self.assertEqual(campaign["storage"]["emergency"], {"volume_bytes": 200 * 10**9, "margin_fraction": 0.1})
        self.assertEqual(campaign["records"], {"window_blocks": 8, "stall_seconds": 2700})
        self.assertEqual(campaign["host_slots"]["cores"], "0-31")
        self.assertEqual(campaign["placement_by_run"]["G3"]["cpu_affinity"], [8, 9, 10, 11])
        self.assertEqual(campaign["placement"]["host"], "runpod")
        members = {item["path"]: item for archive in manifest["archives"].values() for item in archive["members"]}
        for run in g.RUNS:
            item = campaign["play_imports"][run]
            name = item["path"].removeprefix(f"/workspace/{ROOT}/")
            self.assertEqual(members[name]["sha256"], item["sha256"])
        with tarfile.open(out / "inputs.tar.gz") as tar:
            descriptor = json.loads(tar.extractfile(campaign["play_imports"]["G5"]["path"]
                                                    .removeprefix(f"/workspace/{ROOT}/")).read())
        self.assertTrue(descriptor["parameters"]["path"].startswith(f"/workspace/{ROOT}/inputs/objects/"))
        self.assertEqual(descriptor["schema"], g.FRESH_SOURCE_SCHEMA)
        tooling = manifest["tooling"]["modules"]
        self.assertIn("g_references_campaign_v1.py", tooling)
        self.assertIn("g_references_qualify_v1.py", tooling)
        self.assertIn("g_references_v1.py", tooling)
        # Every script the Pod-side tools start by path travels with them.
        started = set()
        for name in tooling:
            started |= set(re.findall(r'TOOLS / "([a-z0-9_]+\.py)"', (cloud.TOOLS / name).read_text(encoding="utf-8")))
        self.assertIn("native_expanded_pack_v1.py", started)
        self.assertLessEqual(started, set(tooling))
        command = gcloud.driver_command(manifest)
        self.assertEqual(command[0].rsplit("/", 1)[1], "g_references_qualify_v1.py")
        self.assertEqual(command[1], "start")
        self.assertEqual(command[command.index("--out") + 1], f"/workspace/{ROOT}/campaign/qualification")

    def test_trials_take_their_own_storage_projection(self):
        def trial_storage(out):
            manifest = json.loads((out / "package.json").read_text())
            members = {item["path"]: item for archive in manifest["archives"].values() for item in archive["members"]}
            name = manifest["qualification_spec"]["path"].removeprefix(f"/workspace/{ROOT}/")
            with tarfile.open(out / "inputs.tar.gz") as tar:
                spec = json.loads(tar.extractfile(name).read())
            self.assertEqual(members[name]["sha256"], manifest["qualification_spec"]["sha256"])
            return manifest["storage"], spec["storage"]
        gcloud.package(self.fixture.spec(), Path(self.temp.name) / "p1")
        block, trial = trial_storage(Path(self.temp.name) / "p1")
        self.assertEqual(trial, block)  # default: the block's projection
        gcloud.package(self.fixture.spec(trial_projected_additional_bytes=10**9), Path(self.temp.name) / "p2")
        block, trial = trial_storage(Path(self.temp.name) / "p2")
        self.assertEqual(block["projected_additional_bytes"], 20 << 30)
        self.assertEqual((trial["projected_additional_bytes"], trial["projected_volume_bytes"]), (10**9, {"": 10**9}))
        self.assertEqual({k: v for k, v in trial.items() if not k.startswith("projected")},
                         {k: v for k, v in block.items() if not k.startswith("projected")})
        for bad in (0, (20 << 30) + 1, 1.5):
            with self.assertRaises(ValueError):
                gcloud.package(self.fixture.spec(trial_projected_additional_bytes=bad),
                               Path(self.temp.name) / f"bad-{bad}")

    def test_production_package_and_refusals(self):
        out = Path(self.temp.name) / "production"
        placement = {"path": f"/workspace/{ROOT}/campaign/qualification/PLACEMENT.json", "sha256": "d" * 64}
        with self.assertRaises(ValueError):
            gcloud.package(self.fixture.spec(mode="production", stop_after_block=494, choice_pins_sha256="c" * 64),
                           Path(self.temp.name) / "no-placement")
        manifest = gcloud.package(self.fixture.spec(mode="production", stop_after_block=494,
                                                    choice_pins_sha256="c" * 64, placement_record=placement), out)
        self.assertIn({"role": "placement_record", **placement}, manifest["volume_inputs"])
        self.assertEqual(json.loads((out / "campaign.json").read_text())["placement_record"], placement)
        self.assertEqual(manifest["volume_inputs"][0]["path"], f"/workspace/{ROOT}/campaign/qualification/choice-pins.json")
        command = gcloud.driver_command(manifest)
        self.assertEqual(command[1], "launch")
        self.assertEqual(command[-1], ",".join(g.RUNS))
        overlapping = self.fixture.spec(slots=[{"cpu_affinity": [0, 1], "workers": 2, "preparation_workers": 2,
                                                "memory_bytes": 1 << 30}] * 8)
        for spec in (overlapping, self.fixture.spec(stop_after_block=3), self.fixture.spec(runs=["G1"]),
                     self.fixture.spec(native_root="/workspace/hot")):
            with self.assertRaises(ValueError):
                gcloud.package(spec, Path(self.temp.name) / f"bad-{time.monotonic_ns()}")


class FakePod:
    def __init__(self, granted):
        self.granted, self.calls = granted, []

    def run(self, program, args, timeout=30, read_only=False):
        self.calls.append((program.split("\n", 1)[0], args))
        if program is gcloud.POD_AFFINITY:
            return self.granted
        if program is cloud.POD_LAUNCH:
            return {"pid": 7, "start_ticks": 1, "token": "t", "epoch": 1.0}
        raise AssertionError(program.split("\n", 1)[0])


class CpuBindingTests(unittest.TestCase):
    # The 2026-10-30 Pod's granted CPUs: 32 of a 192-CPU host, siblings 96 apart.
    GRANTED = [5, 9, 11, 20, 25, 27, 52, 53, 54, 66, 74, 78, 79, 83, 85, 89, 101, 105, 107, 116, 121, 123, 148, 149,
               150, 162, 170, 174, 175, 179, 181, 185]

    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name)
        self.package = self.root / "package"
        gcloud.package(Fixture(temp.name).spec(), self.package)

    def test_binding_rule(self):
        placement = {"cpu_affinity": list(range(32))}
        self.assertIsNone(gcloud.cpu_binding(placement, range(32)))
        self.assertIsNone(gcloud.cpu_binding(placement, range(64)))
        self.assertEqual(gcloud.cpu_binding(placement, list(reversed(self.GRANTED))), self.GRANTED)
        with self.assertRaises(ValueError):
            gcloud.cpu_binding(placement, self.GRANTED[:31])

    def cycle(self, granted):
        cycle = gcloud.Cycle.__new__(gcloud.Cycle)
        cycle.package_dir, cycle.control = self.package, self.root / "control"
        cycle.control.mkdir()
        cycle.manifest = read(self.package / "package.json")
        cycle.created, cycle.pod = {"id": "pod-1"}, FakePod(granted)
        cycle.lease = {"created_epoch": time.time(), "startup_idle_seconds": 1800}
        cycle.guard, cycle.remote_control = "/run/phase1/lease", "/workspace/control/lease"
        seen = []
        with patch.object(cloud.Cycle, "check_hardware", lambda self: seen.append(self.manifest["placement"])):
            cycle.check_hardware()
        cycle.launch()
        return cycle, seen[0], [args for name, args in cycle.pod.calls if name == "#pod:launch"][0]["env"]

    def test_a_scattered_cpuset_binds_logical_cpus(self):
        cycle, checked, env = self.cycle(self.GRANTED)
        self.assertEqual(checked["cpu_affinity"], self.GRANTED)  # the base check sees the granted CPUs
        self.assertEqual(cycle.manifest, read(self.package / "package.json"))  # everything else stays logical
        self.assertEqual(env[slots.CPU_BINDING_ENV], ",".join(map(str, self.GRANTED)))
        record = read(cycle.control / "cpu-binding.json")
        self.assertEqual((record["pod_id"], record["granted"], record["logical"]["16"]), ("pod-1", self.GRANTED, 101))

    def test_a_granted_placement_runs_as_built(self):
        cycle, checked, env = self.cycle(list(range(32)))
        self.assertEqual(checked, read(self.package / "package.json")["placement"])
        self.assertNotIn(slots.CPU_BINDING_ENV, env)
        self.assertFalse((cycle.control / "cpu-binding.json").exists())

    def test_host_slots_speak_logical_cpus_under_a_binding(self):
        sys_root = self.root / "sys"
        for cpu in self.GRANTED + [0, 96]:
            sibling = cpu + 96 if cpu < 96 else cpu - 96
            path = sys_root / f"devices/system/cpu/cpu{cpu}/topology/thread_siblings_list"
            path.parent.mkdir(parents=True)
            path.write_text(f"{min(cpu, sibling)},{max(cpu, sibling)}\n")
        real = slots.Path

        def fake(value, *rest):
            text = str(value)
            return real(sys_root, text.removeprefix("/sys/")) if text.startswith("/sys/") else real(value, *rest)
        binding = {slots.CPU_BINDING_ENV: ",".join(map(str, self.GRANTED))}
        with patch.dict(os.environ, binding), patch.object(slots.os, "name", "posix"), patch.object(slots, "Path", fake):
            self.assertEqual(slots.cpu_total(), 32)
            self.assertEqual(slots.physical_cpus([0, 1, 16]), [5, 9, 101])
            self.assertEqual(slots.logical_cpus({185, 5}), [0, 31])
            with self.assertRaises(ValueError):
                slots.physical_cpus([32])
            with self.assertRaises(ValueError):
                slots.logical_cpus([6])
            rows = slots.topology()
        self.assertEqual(sorted(slots.parse_cores(row["cpus"]) for row in rows), [[i, i + 16] for i in range(16)])
        with patch.dict(os.environ, {slots.CPU_BINDING_ENV: ""}):
            self.assertIsNone(slots.cpu_binding())
            self.assertEqual(slots.physical_cpus([3]), [3])

    def test_dispatch_binds_granted_cpus(self):
        calls = []
        with patch.dict(os.environ, {slots.CPU_BINDING_ENV: ",".join(map(str, self.GRANTED))}), \
                patch.object(slots.os, "name", "posix"), patch.object(dispatch, "WINDOWS", False), \
                patch.object(dispatch.os, "sched_setaffinity", lambda pid, cpus: calls.append(list(cpus)), create=True), \
                patch.object(dispatch.os, "sched_getaffinity", lambda pid: set(self.GRANTED[:4]), create=True):
            dispatch.set_affinity([0, 1, 2, 3])
        self.assertEqual(calls, [self.GRANTED[:4]])

    def test_a_bound_pod_runs_the_same_trainer_inputs(self):
        """The binding changes no hashed input: trainer configs and dispatch requests are built from the package's
        logical CPUs whatever the Pod granted, and the trainer's config takes worker counts, never CPU IDs."""
        campaign = driver.Campaign.__new__(driver.Campaign)
        campaign.raw = json.loads((self.package / "campaign.json").read_text())

        def inputs():
            out = {}
            for run in g.RUNS:
                spec = driver.block_spec(campaign, run, 1, {"blocks": {}}, Path("/root/g-native") / run)
                config = {"iterations": [{"episodes": [1]}], **spec}
                request = {"kind": "training", "placement": campaign.placement(run), "root": "/r"}
                out[run] = [cloud.encoded(spec), cloud.encoded(dispatch.placed_config(request, config, False))]
            return out
        unbound = inputs()
        with patch.dict(os.environ, {slots.CPU_BINDING_ENV: ",".join(map(str, self.GRANTED))}):
            bound = inputs()
        self.assertEqual(bound, unbound)
        for run, (spec, _) in unbound.items():
            self.assertNotIn(b"cpu_affinity", spec)
            self.assertEqual(json.loads(spec)["collection_workers"], 4)


class LeaseTests(unittest.TestCase):
    def test_recovery_includes_doses_and_qualification(self):
        self.assertIn('"doses", "qualification"', gcloud.POD_RECOVER)

    def conclude(self, runs):
        cycle = gcloud.Cycle.__new__(gcloud.Cycle)
        cycle.manifest, cycle.before, cycle.result = {"runs": list(runs), "mode": "production"}, {}, {}
        cycle.conclude({"runs": runs})
        return cycle.result

    def test_a_state_complete_at_an_earlier_target_is_pending(self):
        cycle = gcloud.Cycle.__new__(gcloud.Cycle)
        cycle.manifest = {"stop_after_block": 494, "mode": "production"}
        snapshot = {"runs": {"G1": {"status": "complete", "blocks_done": [1, 2]},
                             "G2": {"status": "complete", "blocks_done": list(range(1, 495))}}}
        with patch.object(cloud.Cycle, "observe", return_value=snapshot):
            observed = cycle.observe()
        self.assertEqual(observed["runs"]["G1"]["status"], "pending")
        self.assertEqual(observed["runs"]["G2"]["status"], "complete")

    def test_a_qualification_lease_is_done_only_when_the_tool_passed(self):
        cycle = gcloud.Cycle.__new__(gcloud.Cycle)
        cycle.manifest, cycle.before, cycle.result = {"stop_after_block": 2, "mode": "qualification",
                                                      "runs": ["G1"]}, {}, {}
        snapshot = {"runs": {"G1": {"status": "complete", "blocks_done": [1, 2]}}}
        for passed, status in ((False, "pending"), (True, "complete")):
            with patch.object(cloud.Cycle, "observe", return_value={"runs": {k: dict(v) for k, v in snapshot["runs"].items()}}),                     patch.object(gcloud.Cycle, "qualification_passed", return_value=passed):
                observed = cycle.observe()
            self.assertEqual(observed["runs"]["G1"]["status"], status)
            if passed:
                cycle.conclude(observed)
                self.assertTrue(cycle.result["complete"])
            else:
                with self.assertRaises(ValueError):
                    cycle.conclude(observed)

    def test_statuses(self):
        done = {"status": "complete", "blocks_done": [1, 2]}
        failed = {"status": "stopped", "blocks_done": [1], "dispositions": {"2": "failed: invalid block"}}
        tired = {"status": "stopped", "stop_reason": "lease_time_exhausted", "blocks_done": [1]}
        self.assertTrue(self.conclude({"G1": done, "G2": failed})["complete"])
        result = self.conclude({"G1": done, "G2": tired, "G3": failed})
        self.assertTrue(result["lease_stopped"])
        self.assertEqual(result["failed_lineages"], ["G3"])
        with self.assertRaises(ValueError):
            self.conclude({"G1": done, "G2": {"status": "stopped", "blocks_done": []}})
        held = {"status": "stopped", "blocks_done": [3], "stop_reason": "storage admission: records: copy-back stalled"}
        self.assertTrue(self.conclude({"G1": done, "G2": held})["lease_stopped"])  # amendment 2: resumable

    def test_recovery_first_quiesces_the_record_copy_back_on_the_pod(self):
        for answer in (True, "timed out"):
            with tempfile.TemporaryDirectory() as temp:
                cycle = gcloud.Cycle.__new__(gcloud.Cycle)
                cycle.control, cycle.result, cycle.base = Path(temp), {}, Path("/workspace/g")
                cycle.remote_ready, calls = True, []

                class Pod:
                    def run(self, program, args, timeout=30, read_only=False):
                        calls.append((program, args))
                        if program is gcloud.records.LIST:
                            return {"pending": ["G3-b005.json"]}
                        if program is gcloud.POD_QUIESCE_RECORDS:
                            return {"quiesced": answer}
                        raise AssertionError("recovery enumerated the volume")
                cycle.pod = Pod()
                if answer is True:
                    self.assertTrue(cycle.quiesce_records(timeout=5))
                    self.assertEqual(read(Path(temp) / "records-backlog-final.json")["pending"],
                                     ["G3-b005.json"])  # the controller's own final backlog, after quiescence
                else:
                    cycle.recover()  # no positive quiescence: recovery does not enumerate the volume
                    self.assertTrue(cycle.result["recovery"].startswith("skipped"))
                    self.assertNotIn("recovered", cycle.result)
            self.assertTrue(calls[0][0] is gcloud.POD_QUIESCE_RECORDS)
            self.assertEqual((calls[0][1]["lock"], calls[0][1]["quiesce"]),
                             (gcloud.records.POD_LOCK, gcloud.records.POD_QUIESCE))

    @unittest.skipIf(sys.platform == "win32", "fcntl: the Pod is Linux")
    def test_the_pod_quiesce_waits_for_a_deletion_in_progress(self):
        import fcntl
        with tempfile.TemporaryDirectory() as temp:
            lock, flag = Path(temp) / "lock", Path(temp) / "quiesce"
            code = cloud.PRELUDE % json.dumps({"lock": str(lock), "quiesce": str(flag), "timeout": 1}) + \
                gcloud.POD_QUIESCE_RECORDS
            with open(lock, "a") as held:
                fcntl.flock(held, fcntl.LOCK_EX)  # a deletion holds the lock
                busy = subprocess.run([sys.executable, "-B", "-"], input=code.encode(), capture_output=True)
            self.assertEqual(json.loads(busy.stdout.decode().splitlines()[-1]), {"quiesced": "timed out"})
            self.assertTrue(flag.exists())
            free = subprocess.run([sys.executable, "-B", "-"], input=code.encode(), capture_output=True)
            self.assertEqual(json.loads(free.stdout.decode().splitlines()[-1]), {"quiesced": True})

    def test_ledger_and_caps(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            now = time.time()
            for index, hours in enumerate((8, 7)):
                control = root / f"lease-{index}"
                write(control / "lease" / "lease.json", {"name": f"lease-{index}", "created_epoch": now - 90000,
                                                         "deadline_epoch": now, "rate_ceiling_usd_hour": 1.2})
                write(control / "lease" / "create-attempt.json", {"epoch": now - 80000})
                write(control / "release-confirmed.json", {"epoch": now - 80000 + hours * 3600})
            write(root / "never" / "lease" / "lease.json", {"name": "never", "created_epoch": 0, "deadline_epoch": 1,
                                                            "rate_ceiling_usd_hour": 1})
            ambiguous = root / "ambiguous"  # a create with no Pod id, closed by the lead
            write(ambiguous / "lease" / "lease.json", {"name": "ambiguous", "created_epoch": now - 50000,
                                                       "deadline_epoch": now, "rate_ceiling_usd_hour": 1.2})
            write(ambiguous / "lease" / "create-attempt.json", {"epoch": now - 50000})
            unclosed = gcloud.ledger(root, 200, now - 5 * 86400, now)
            write(ambiguous / "release-closed-by-lead.json", {"lease": "ambiguous", "usd": 0.0,
                                                              "closed_by": "research lead"})
            summary = gcloud.ledger(root, 200, now - 5 * 86400, now)
            write(ambiguous / "release-closed-by-lead.json", {"lease": "other", "usd": 0.0, "closed_by": "x"},
                  replace=True)
            with self.assertRaises(ValueError):
                gcloud.ledger(root, 200, now - 5 * 86400, now)
        self.assertAlmostEqual(unclosed["lease_seconds"], 15 * 3600 + 50000)
        closed = next(row for row in summary["leases"] if row["lease"] == "ambiguous")
        self.assertEqual((closed["usd_at_ceiling"], closed["seconds"]), (0.0, 0.0))
        self.assertIn("closed_by_lead", closed)
        self.assertAlmostEqual(summary["lease_seconds"], 15 * 3600)
        self.assertAlmostEqual(summary["compute_usd_at_ceiling"], 18.0)
        self.assertAlmostEqual(summary["volume_usd"], 5 / 30 * 200 * 0.07)
        lease = {"created_epoch": 0, "deadline_epoch": 8 * 3600, "rate_ceiling_usd_hour": 1.2}
        gcloud.require_within_caps(summary, lease, 200)
        with self.assertRaises(ValueError):
            gcloud.require_within_caps(dict(summary, lease_seconds=55 * 3600), lease, 200)
        with self.assertRaises(ValueError):
            gcloud.require_within_caps(dict(summary, total_usd=52.0), lease, 200)
        hour = {"created_epoch": 0, "deadline_epoch": 3600, "rate_ceiling_usd_hour": 1.2}
        gcloud.require_within_caps(dict(summary, total_usd=58.79), hour, 0)
        with self.assertRaises(ValueError):  # the volume's accrual over the lease counts too
            gcloud.require_within_caps(dict(summary, total_usd=58.79), hour, 200)


if __name__ == "__main__":
    unittest.main()
