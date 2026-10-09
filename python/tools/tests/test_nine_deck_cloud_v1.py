"""Offline coverage of the nine-deck cloud controller. No provider, network or ssh:
a fake provider, and a fake ssh/scp layer that runs the Pod programs locally
against a temporary directory standing in for the Pod's /workspace and /run."""
import hashlib
import io
import json
import os
import shutil
import subprocess
import sys
import tarfile
import tempfile
import time
import unittest
from datetime import UTC, datetime
from pathlib import Path, PurePosixPath
from unittest.mock import patch

import nine_deck_cloud_v1 as cloud
from common import read, write
from prepare_lease import prepare as prepare_lease

REPO = Path(__file__).resolve().parents[3]
LINUX = sys.platform.startswith("linux")
KEY = "controller-test-key-not-real"
ROOT = "nine-deck-test"
FAKE_ELF = b"\x7fELF\x02\x01\x01" + bytes(9) + b"\x02\x00\x3e\x00" + bytes(100)
FAKE_DRIVER = r'''import json, os, subprocess, sys
from pathlib import Path
mode, campaign_path, runs = sys.argv[1], sys.argv[2], sys.argv[3].split(",")
pod = os.environ["FAKE_POD"]
def local(path):
    return Path(path.replace("/workspace", pod + "/workspace"))
campaign = json.loads(Path(campaign_path).read_text())
state = local(campaign["roots"]["state"])
state.mkdir(parents=True, exist_ok=True)
(state / "driver-env.json").write_text(json.dumps(dict(os.environ)))
for run in runs:
    record = {"run": run, "blocks": {"1": {"attempts": [], "done": True}, "2": {"attempts": [], "done": True}},
              "status": {"complete": "complete", "exhausted": "stopped", "death": "running"}[mode]}
    if mode == "exhausted":
        record["stop_reason"] = "lease_time_exhausted"
    (state / (run + ".json")).write_text(json.dumps(record))
    kept = local(campaign["roots"]["checkpoints"]) / run / "block-02.json"
    kept.parent.mkdir(parents=True, exist_ok=True)
    kept.write_text(json.dumps({"run": run}))
child = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(0.5)"], start_new_session=True,
                         stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
print(json.dumps({"state": "dispatched", "pid": child.pid, "token": "faketoken"}))
'''


def sha_bytes(data):
    return hashlib.sha256(data).hexdigest()


def pinned(path, data):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)
    return {"path": str(path), "sha256": sha_bytes(data)}


def lease_value(name="nine-deck-test-lease", hours=6):
    now = time.time()
    return {"schema": "phase1-cloud-lease/v1", "name": name, "network_volume_id": "testvolume",
            "created_epoch": now, "deadline_epoch": now + hours * 3600, "increment_cap_usd": 10,
            "total_cap_usd": 100, "prior_conservative_usd": 0, "rate_ceiling_usd_hour": 1.0,
            "storage_usd_hour": 0.1, "postrun_storage_reserve_usd": 0.5, "recovery_reserve_usd": 0.5,
            "funded_balance_usd": 50, "safety_multiplier": 1.25, "startup_idle_seconds": 900,
            "work_idle_seconds": 300, "recovery_seconds": 600, "poll_seconds": 30}


class Fixture:
    """Local inputs, a fake Pod volume and prepared leases."""

    def __init__(self, root):
        self.root = Path(root)
        self.pod = self.root / "pod"
        local = self.root / "local"
        sources = {}
        for label in ("t1", "a48"):
            init = pinned(local / f"{label}-init.json", json.dumps({"lineage": label}).encode())
            params = pinned(local / f"{label}-params.f32le", label.encode() * 64)
            sources[label] = {
                "checkpoint": pinned(local / f"{label}.json", json.dumps({"checkpoint": label}).encode()),
                "feature_transfer": {"expected_feature_contract_digest": "c" * 64},
                "play_import": pinned(local / f"{label}-import.json", json.dumps(
                    {"initialization": init, "parameters": params, "schema": "import"}).encode())}
        self.template = {"lane": "test-lane", "decks": str(REPO / "data" / "runtime_decks_v1.json"),
                         "t1_source": sources["t1"], "a48_source": sources["a48"],
                         "placement": {"host": "desktop", "workers": 8, "preparation_workers": 8},
                         "runtime": {"path": "C:/r.json", "sha256": "0" * 64}, "storage": {}, "wall_seconds": 7200,
                         "choice": {"path": "C:/c.json", "sha256": "0" * 64}, "cold_keep_blocks": 2,
                         "compute_host_name": "COMPUTEHOST", "roots": {"state": "C:/s"}}
        write(local / "campaign.json", self.template)
        self.binary = local / "bin" / "native_expanded_training_run_v1"
        self.binary.parent.mkdir(parents=True)
        self.binary.write_bytes(FAKE_ELF)
        self.host_slots = local / "host_slots_v1.py"
        self.host_slots.write_text("import sys\n")
        self.choice = {}
        for role, name in (("choice", "choice.json"), ("choice_verification", "choice-verification.json")):
            data = json.dumps({"role": role}).encode()
            pinned(self.pod / "workspace" / ROOT / "qualification" / name, data)
            self.choice[role] = {"path": f"/workspace/{ROOT}/qualification/{name}", "sha256": sha_bytes(data)}
        self.slots = patch.object(cloud, "HOST_SLOTS_SHA256", hashlib.sha256(self.host_slots.read_bytes()).hexdigest())
        self.external_guard = local / "external_guard.py"
        self.external_guard.write_text("raise SystemExit('never run by tests')\n")
        self.ssh_key = local / "id_ed25519"
        self.ssh_key.write_text("not a key\n")
        self.count = 0

    def spec(self, **changes):
        value = {"schema": cloud.SPEC_SCHEMA, "campaign_root": ROOT, "campaign": str(self.root / "local/campaign.json"),
                 "runs": ["r1", "r2"],
                 "placement": {"cpu_affinity": [0, 1, 2, 3], "workers": 2, "preparation_workers": 2,
                               "memory_bytes": 8 << 30},
                 "storage": {"max_logical_bytes": 100 << 30, "reserve_bytes": 60 << 30,
                             "projected_additional_bytes": 10 << 30},
                 "lease_block_seconds": 3000,
                 "runtime": {"binary": str(self.binary), "engine_commit": "c69255d32e8cfa46974ff8c93cd9ce7b4f0a366d",
                             "tracked_tree_sha256": "f8375e9d046367c1f14014b873d5d1b1b973a2fb94c11c10430b8312a46f42fa"},
                 "hardware": {"min_memory_bytes": 32 << 30}, **self.choice}
        return value | changes

    def package(self, **changes):
        self.count += 1
        spec = self.root / f"spec-{self.count}.json"
        write(spec, self.spec(**changes))
        output = self.root / f"package-{self.count}"
        with self.slots:
            cloud.package(spec, output, self.host_slots)
        return output

    def control(self, name="nine-deck-test-lease", image=None, lease=None):
        control = self.root / ("control-" + name)
        lease = lease or lease_value(name)
        spec = {"schema": "phase1-cloud-lease-preparation/v1", "lease": lease,
                "account_observed_epoch": lease["created_epoch"], "quote_observed_epoch": lease["created_epoch"],
                "quoted_cpu_usd_hour": 0.96, "observed_pods": 0, "autopay_enabled": False,
                "public_ssh_key": "ssh-ed25519 AAAATEST controller"}
        if image:
            spec["image_name"] = image
        prepare_lease(spec, control / "lease", now=lease["created_epoch"] + 1)
        write(control / "funding.json", {"observed_utc": datetime.now(UTC).isoformat()})
        return control

    def guard(self, lease_name, pod_id):
        write(self.pod / "run/phase1" / lease_name / "guard.json",
              {"pod_id": pod_id, "name": lease_name, "epoch": time.time(), "provider_ok": True,
               "allow_new_dispatch": True, "latched": None, "release_epoch": None}, replace=True)


class FakeCloud:
    """prepare_lease's create client, lease_guard.Provider and verify_pod_absent's lookup."""

    def __init__(self):
        self.pods, self.created, self.deleted = {}, 0, []

    def create(self, body):
        assert body["env"]["RUNPOD_API_KEY"] == KEY  # injected in memory only
        self.created += 1
        pod = {"id": f"testpod{self.created}", "name": body["name"], "desiredStatus": "RUNNING", "costPerHr": 0.96,
               "networkVolumeId": body["networkVolumeId"], "cpuFlavorId": "cpu3c", "vcpuCount": 32,
               "publicIp": "203.0.113.7", "portMappings": {"22": 40022}}
        self.pods[pod["id"]] = pod
        return dict(pod)

    def provider(self, key, pod_id):
        assert key == KEY
        cloud_state = self

        class Provider:
            def call(self, method):
                if method == "DELETE":
                    cloud_state.deleted.append(pod_id)
                    cloud_state.pods.pop(pod_id, None)
                    return {}
                pod = cloud_state.pods.get(pod_id)
                return dict(pod) if pod else None
        return Provider()

    def lookup(self, pod_id):
        if pod_id in self.pods:
            return {"status": 200, "kind": "listed", "body": self.pods[pod_id], "reason": None}
        return {"status": 404, "kind": "not_found", "body": None, "reason": None}


def hardware_answer():
    cpu_info = "\n\n".join(f"processor\t: {cpu}\nmodel name\t: Test EPYC\nphysical id\t: 0\ncore id\t: {cpu // 2}"
                           for cpu in range(32))
    host = {"cpu_info": cpu_info, "affinity": list(range(32)), "memory": "MemTotal: 134217728 kB\n",
            "disk": {"total": 200 << 30, "used": 1 << 30, "free": 199 << 30}, "cpu_count": 32, "python": "3.12.14",
            "kernel": "test"}
    groups = {"/proc/self/mountinfo": "36 35 98:0 / /workspace rw,noatime master:1 - fuse.mfs mfs rw\n",
              "/sys/fs/cgroup/cpu.max": "3200000 100000\n", "/sys/fs/cgroup/memory.max": str(64 << 30) + "\n"}
    libraries = {name: [cloud.LIBRARY_DIRECTORY + name, digest] for name, digest in cloud.IMAGE_LIBRARIES.items()}
    return [host, groups, libraries]


class FakeRunner:
    """subprocess.run for ssh/scp: Pod programs run locally with Pod paths mapped into the fixture."""

    def __init__(self, pod, fail=()):
        self.pod, self.fail, self.calls = Path(pod), set(fail), []
        self.mapping = {"/workspace": str(self.pod / "workspace"), "/run/phase1": str(self.pod / "run/phase1"),
                        "/var/lib/mtg-node": str(self.pod / "var/lib/mtg-node")}

    def local(self, text):
        for remote, local in self.mapping.items():
            text = text.replace(remote, local)
        return text

    def __call__(self, argv, input=None, capture_output=False, timeout=None, check=False):
        self.calls.append(argv)
        if argv[0] == "scp":
            assert "-i" in argv and "StrictHostKeyChecking=yes" in argv
            source, target = (self.local(item.removeprefix("root@203.0.113.7:")) for item in argv[-2:])
            if "get" in self.fail and argv[-2].startswith("root@"):
                raise subprocess.CalledProcessError(1, argv, b"", b"scp: lost connection")
            shutil.copyfile(source, target)
            return subprocess.CompletedProcess(argv, 0, b"", b"")
        assert argv[0] == "ssh" and argv[-4:] == ["root@203.0.113.7", "python3", "-B", "-"]
        assert "StrictHostKeyChecking=accept-new" in argv and "BatchMode=yes" in argv
        code = input.decode()
        if "#pod:hardware" in code:
            return subprocess.CompletedProcess(argv, 0, json.dumps(hardware_answer()).encode(), b"")
        env = dict(os.environ, RUNPOD_API_KEY="pod-container-key", RUNPOD_PUBLIC_IP="203.0.113.7",
                   FAKE_POD=str(self.pod))
        done = subprocess.run([sys.executable, "-B", "-"], input=self.local(code).encode(), capture_output=True,
                              timeout=timeout, env=env, check=False)
        if check and done.returncode:
            raise subprocess.CalledProcessError(done.returncode, argv, done.stdout, done.stderr)
        return subprocess.CompletedProcess(argv, done.returncode, done.stdout, done.stderr)


class Base(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.fx = Fixture(self.temp.name)


class PackageTests(Base):
    def test_tooling_closure_is_complete_from_a_copy(self):
        files = cloud.closure()
        for name in ("nine_deck_campaign_v1.py", "native_expanded_dispatch_v1.py", "host_reservation_v1.py",
                     "nine_deck_baseline_v1.py", "nine_deck_exposure_collector_v1.py", "phase1_breadth_v1/catalog_v1.py",
                     "public_training_storage_v1.py", "compute_throughput_v3.py", "phase1_cloud/lease_guard.py",
                     "phase1_cloud/common.py"):
            self.assertIn(name, files)
        self.assertNotIn("nine_deck_cloud_v1.py", files)
        copy = Path(self.temp.name) / "copy"
        for name in files:
            (copy / name).parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(cloud.TOOLS / name, copy / name)
        self.assertEqual(sorted(cloud.require_closure_imports(copy)), files)
        (copy / "nine_deck_baseline_v1.py").unlink()
        with self.assertRaisesRegex(ValueError, "closure is incomplete"):
            cloud.require_closure_imports(copy)

    def test_runtime_json_pins_the_image_libraries(self):
        output = self.fx.package()
        manifest = read(output / "package.json")
        base = PurePosixPath("/workspace") / ROOT
        with tarfile.open(output / "runtime.tar.gz") as tar:
            names = tar.getnames()
            runtime = json.loads(tar.extractfile(str(PurePosixPath(manifest["runtime"]["path"]).relative_to(base))).read())
            binary = tar.getmember(next(name for name in names if name.endswith("native_expanded_training_run_v1")))
        self.assertEqual(runtime["platform"], "linux-x86_64")
        self.assertEqual(runtime["engine_commit"], "c69255d32e8cfa46974ff8c93cd9ce7b4f0a366d")
        self.assertEqual(runtime["tracked_tree_sha256"], self.fx.spec()["runtime"]["tracked_tree_sha256"])
        self.assertEqual(runtime["libraries"], {
            "ld-linux-x86-64.so.2": {"path": "/usr/lib/x86_64-linux-gnu/ld-linux-x86-64.so.2",
                                     "sha256": "02bcda52c1a5dfc236f94d9e5255b4a0e26347d8a372a5223b650e31f291ce3c"},
            "libc.so.6": {"path": "/usr/lib/x86_64-linux-gnu/libc.so.6",
                          "sha256": "6b4a45352fd0c540a9c7c718f35ce8c8e46a4e482f9d3885a910c32d1a0e1421"},
            "libm.so.6": {"path": "/usr/lib/x86_64-linux-gnu/libm.so.6",
                          "sha256": "7f2ca87f652f56b094462474b076749e90e689d0ecb9cb63c7679820b271b4e7"},
            "libgcc_s.so.1": {"path": "/usr/lib/x86_64-linux-gnu/libgcc_s.so.1",
                              "sha256": "2bd1552c47799ef67e701e81d4383061fd76059868e446e63560f0dd0d5ec14e"}})
        digest = sha_bytes(FAKE_ELF)
        self.assertEqual(runtime["binary"], {"path": f"{base}/runtime/{digest}/native_expanded_training_run_v1",
                                             "sha256": digest})
        self.assertEqual(binary.mode, 0o755)
        self.assertEqual(manifest["runtime_identity"]["platform"], "linux-x86_64")
        self.assertEqual(manifest["image"], cloud.IMAGE_REFERENCE)

    def test_runtime_refuses_a_non_linux_binary(self):
        self.fx.binary.write_bytes(b"MZ" + bytes(100))
        with self.assertRaisesRegex(ValueError, "Linux x86-64"):
            self.fx.package()

    def test_inputs_are_rewritten_for_the_pod_and_pinned(self):
        output = self.fx.package()
        manifest = read(output / "package.json")
        campaign = read(output / "campaign.json")
        base = f"/workspace/{ROOT}"
        members = {}
        for archive in manifest["archives"].values():
            with tarfile.open(output / archive["file"]) as tar:
                self.assertTrue(all(info.isreg() for info in tar))
                for info in tar:
                    members[info.name] = sha_bytes(tar.extractfile(info).read())
        self.assertEqual(members, {item["path"]: item["sha256"] for archive in manifest["archives"].values()
                                   for item in archive["members"]})

        def staged(item):  # a carried pin names a member with its bytes
            self.assertTrue(item["path"].startswith(base + "/"))
            self.assertEqual(members[str(PurePosixPath(item["path"]).relative_to(base))], item["sha256"])

        self.assertEqual(campaign["placement"], {"host": "runpod", "cpu_affinity": [0, 1, 2, 3], "workers": 2,
                                                 "preparation_workers": 2, "memory_bytes": 8 << 30})
        self.assertEqual(campaign["host_slots"]["cores"], "0-3")
        staged(campaign["host_slots"]["tool"])
        staged(campaign["runtime"])
        for label in ("t1_source", "a48_source"):
            source, original = campaign[label], self.fx.template[label]
            staged(source["checkpoint"])
            self.assertEqual(source["checkpoint"]["sha256"], original["checkpoint"]["sha256"])  # byte for byte
            self.assertEqual(source["feature_transfer"], original["feature_transfer"])
            staged(source["play_import"])
            self.assertNotEqual(source["play_import"]["sha256"], original["play_import"]["sha256"])
            with tarfile.open(output / "inputs.tar.gz") as tar:
                descriptor = json.loads(tar.extractfile(
                    str(PurePosixPath(source["play_import"]["path"]).relative_to(base))).read())
            for key in ("initialization", "parameters"):
                staged(descriptor[key])
                self.assertEqual(descriptor[key]["sha256"], read(original["play_import"]["path"])[key]["sha256"])
        decks = PurePosixPath(campaign["decks"]).relative_to(base)
        for name in ("data/runtime_decks_v1.json", "data/cards_v1.json",
                     "docs/research/sideboard_plan_inputs_2026-09/registered_nine_75s.json"):
            self.assertEqual(members[str(decks.parents[1] / name)],
                             hashlib.sha256((REPO / name).read_bytes()).hexdigest())
        self.assertEqual(campaign["roots"], {name: f"{base}/campaign/{name}" for name in cloud.ROOTS} | {"hot_by_run": {}})
        self.assertEqual(campaign["storage"]["accounting_roots"], [f"{base}/campaign"])
        self.assertEqual(campaign["storage"]["projected_volume_bytes"], {"": 10 << 30})
        self.assertEqual(campaign["choice"], self.fx.choice["choice"])
        self.assertEqual([item["path"] for item in manifest["volume_inputs"]],
                         [self.fx.choice["choice"]["path"], self.fx.choice["choice_verification"]["path"]])
        self.assertNotIn("compute_host_name", campaign)
        self.assertEqual(campaign["lease_block_seconds"], 3000)
        self.assertTrue(manifest["production_ready"])
        self.assertEqual(manifest["tooling"]["host_slots"]["source"],
                         "tree" if (cloud.TOOLS / cloud.HOST_SLOTS).is_file() else "external")
        self.assertEqual(cloud.verified_package(output)["campaign"], manifest["campaign"])

    def test_inputs_refuse_changed_or_misplaced_inputs(self):
        Path(self.fx.template["t1_source"]["checkpoint"]["path"]).write_text("changed")
        with self.assertRaisesRegex(ValueError, "SHA256 differs"):
            self.fx.package()
        Path(self.fx.template["t1_source"]["checkpoint"]["path"]).write_text(json.dumps({"checkpoint": "t1"}))
        with self.assertRaisesRegex(ValueError, "on the volume"):
            self.fx.package(choice={"path": "C:/qualification/choice.json", "sha256": "0" * 64})
        with self.assertRaisesRegex(ValueError, "invalid placement worker counts"):
            self.fx.package(placement={"cpu_affinity": [0], "workers": 2, "preparation_workers": 1,
                                       "memory_bytes": 1 << 30})
        self.assertFalse(read(self.fx.package(choice=None, choice_verification=None) / "package.json")["production_ready"])

    def test_package_archive_tampering_is_refused(self):
        output = self.fx.package()
        with (output / "tooling.tar.gz").open("ab") as stream:
            stream.write(b"\0")
        with self.assertRaisesRegex(ValueError, "package archive changed"):
            cloud.verified_package(output)


class PlanTests(Base):
    def test_plan_accepts_a_fitting_lease_and_allocates_nothing(self):
        result = cloud.plan(self.fx.package(), self.fx.control(), self.fx.external_guard)
        self.assertTrue(result["planning_valid"])
        self.assertFalse(result["allocated"])
        self.assertEqual(result["block_estimate_seconds"], 3000)

    def test_plan_refusals(self):
        package = self.fx.package()
        control = self.fx.control()
        lease = read(control / "lease/lease.json")
        with self.assertRaisesRegex(ValueError, "lease preparation expired"):
            cloud.plan(package, control, self.fx.external_guard, now=lease["created_epoch"] + 400)
        oversized = self.fx.control("nine-deck-oversized")
        write(oversized / "lease/lease.json", read(oversized / "lease/lease.json") | {
            "deadline_epoch": lease["created_epoch"] + 9 * 3600}, replace=True)
        with self.assertRaisesRegex(ValueError, "at most eight hours"):
            cloud.plan(package, oversized, self.fx.external_guard)
        wrong = self.fx.control("nine-deck-image", image="docker.io/library/python@sha256:" + "0" * 64)
        with self.assertRaisesRegex(ValueError, "Pod image differs"):
            cloud.plan(package, wrong, self.fx.external_guard)
        with self.assertRaisesRegex(ValueError, "a block cannot finish inside the lease"):
            cloud.plan(self.fx.package(lease_block_seconds=6 * 3600), control, self.fx.external_guard)
        with self.assertRaisesRegex(ValueError, "lacks the choice"):
            cloud.plan(self.fx.package(choice=None, choice_verification=None), control, self.fx.external_guard)
        with self.assertRaisesRegex(ValueError, "external guard unavailable"):
            cloud.plan(package, control, Path(self.temp.name) / "absent.py")
        edited = self.fx.control("nine-deck-edited")
        write(edited / "lease/lease.json", read(edited / "lease/lease.json") | {"poll_seconds": 20}, replace=True)
        with self.assertRaisesRegex(ValueError, "resident guard runs"):
            cloud.plan(package, edited, self.fx.external_guard)
        short = self.fx.control("nine-deck-recovery", lease=lease_value("nine-deck-recovery") | {"recovery_seconds": 120})
        with self.assertRaisesRegex(ValueError, "recovery window"):
            cloud.plan(package, short, self.fx.external_guard)

    def test_execute_without_the_flag_only_plans(self):
        package, control = self.fx.package(), self.fx.control()
        out = io.StringIO()
        with patch.object(cloud, "execute", side_effect=AssertionError("allocated")), \
                patch.dict(os.environ, {"RUNPOD_API_KEY": ""}), patch("sys.stdout", out):
            self.assertEqual(cloud.main(["execute", "--package", str(package), "--control", str(control),
                                         "--external-guard", str(self.fx.external_guard)]), 0)
        self.assertFalse(json.loads(out.getvalue())["allocated"])
        self.assertFalse((control / "lease/created-pod.json").exists())


@unittest.skipUnless(LINUX, "Pod programs read /proc")
class ExecuteTests(Base):
    def run_lease(self, mode, package=None, name="nine-deck-test-lease", fail=()):
        package = package or self.fx.package()
        control = self.fx.control(name)
        self.cloud = getattr(self, "cloud", None) or FakeCloud()
        self.fx.guard(name, f"testpod{self.cloud.created + 1}")
        driver_script = Path(self.temp.name) / "fake_driver.py"
        driver_script.write_text(FAKE_DRIVER)
        manifest = read(package / "package.json")
        self.runner = FakeRunner(self.fx.pod, fail)
        spawned = []

        def spawn(argv):
            spawned.append(argv)
            return type("Process", (), {"pid": 4242})()

        command = [str(driver_script), mode, manifest["campaign"]["path"], ",".join(manifest["runs"])]
        with patch.dict(os.environ, {"RUNPOD_API_KEY": KEY}), \
                patch.object(cloud, "driver_command", return_value=command):
            result = cloud.execute(package, control, self.fx.ssh_key, self.fx.external_guard, provider=self.cloud.provider,
                                   create_api=self.cloud, lookup_api=self.cloud, runner=self.runner, spawn=spawn,
                                   sleep=lambda seconds: None)
        self.assertEqual(spawned, [[sys.executable, str(self.fx.external_guard), str(control)]])
        self.assertEqual(read(control / "completion.json"), result)
        for path in control.rglob("*"):  # the API key is never written
            if path.is_file():
                self.assertNotIn(KEY.encode(), path.read_bytes(), path)
        return result, control

    def released(self, result, control, pod_id="testpod1"):
        self.assertTrue(result["release_confirmed"])
        self.assertNotIn(pod_id, self.cloud.pods)
        self.assertIn(pod_id, self.cloud.deleted)
        self.assertTrue((control / "release-authorized.json").exists())
        self.assertTrue(read(control / f"pod-absence-{pod_id}.json")["provider_absent"])

    def test_normal_completion_recovers_and_releases(self):
        result, control = self.run_lease("complete")
        self.assertTrue(result["complete"] and result["recovered"], result)
        self.assertFalse(result["lease_stopped"])
        self.assertEqual(result["blocks_completed"], {"r1": 2, "r2": 2})
        self.assertEqual(result["blocks_completed_this_lease"], {"r1": 2, "r2": 2})
        self.released(result, control)
        recovered = control / "recovered"
        self.assertEqual(read(recovered / "campaign/state/r1.json")["status"], "complete")
        self.assertTrue((recovered / "campaign/checkpoints/r2/block-02.json").exists())
        self.assertTrue((recovered / "guard/guard.json").exists())
        self.assertTrue((recovered / "control/launch.stdout").exists())
        guard = self.fx.pod / "run/phase1/nine-deck-test-lease"
        self.assertEqual((guard / "lease.json").read_bytes(), (control / "lease/lease.json").read_bytes())
        for name in ("worker-started.json", "guard-armed.json", "actual-hardware.json", "staging.json", "plan.json"):
            self.assertTrue((control / name).exists(), name)
        staging = read(control / "staging.json")
        self.assertTrue(all(staging[kind]["uploaded"] == staging[kind]["members"] for kind in ("runtime", "tooling", "inputs")))

    def test_worker_environment_has_no_api_key(self):
        self.run_lease("complete")
        env = json.loads((self.fx.pod / "workspace" / ROOT / "campaign/state/driver-env.json").read_text())
        self.assertNotIn("RUNPOD_API_KEY", env)
        self.assertNotIn("RUNPOD_PUBLIC_IP", env)
        self.assertNotIn(KEY, json.dumps(env))
        self.assertEqual(env["RUNPOD_POD_ID"], "testpod1")
        self.assertEqual(env["MTG_LEASE_GUARD_DIR"], str(self.fx.pod / "run/phase1/nine-deck-test-lease"))
        self.assertEqual(env["MTG_HOST_LOCK_ROOT"], str(self.fx.pod / "var/lib/mtg-node/host-lock"))
        self.assertEqual(env["HOST_SLOTS_ROOT"], env["MTG_HOST_LOCK_ROOT"])

    def test_lease_time_exhausted_stop_is_recovered_and_released(self):
        result, control = self.run_lease("exhausted")
        self.assertTrue(result["lease_stopped"] and result["recovered"], result)
        self.assertFalse(result["complete"])
        self.assertEqual(result["stop_reasons"], {"r1": "lease_time_exhausted", "r2": "lease_time_exhausted"})
        self.assertNotIn("error", result)
        self.released(result, control)

    def test_worker_death_is_an_error_that_still_recovers_and_releases(self):
        result, control = self.run_lease("death")
        self.assertFalse(result["complete"] or result["lease_stopped"])
        self.assertIn("stopped before its runs finished", result["error"])
        self.assertTrue(result["recovered"])
        self.released(result, control)

    def test_recovery_failure_still_releases_the_pod(self):
        result, control = self.run_lease("complete", fail={"get"})
        self.assertTrue(result["complete"])
        self.assertFalse(result["recovered"])
        self.assertEqual(result["recovery_error_type"], "CalledProcessError")
        self.released(result, control)

    def test_a_later_lease_resumes_without_restaging(self):
        package = self.fx.package()
        first, _ = self.run_lease("exhausted", package)
        self.assertTrue(first["lease_stopped"])
        second, control = self.run_lease("complete", package, name="nine-deck-test-lease-b")
        self.assertTrue(second["complete"] and second["recovered"] and second["release_confirmed"], second)
        staging = read(control / "staging.json")
        self.assertEqual({kind: staging[kind]["uploaded"] for kind in ("runtime", "tooling", "inputs")},
                         {"runtime": 0, "tooling": 0, "inputs": 0})
        self.assertFalse(list(control.glob("stage-*.tar.gz")))
        self.released(second, control, "testpod2")

    def test_volume_holding_different_bytes_is_refused_then_released(self):
        package = self.fx.package()
        manifest = read(package / "package.json")
        tool = self.fx.pod / "workspace" / ROOT / "tooling" / manifest["tooling"]["id"] / "python/tools/host_reservation_v1.py"
        tool.parent.mkdir(parents=True)
        tool.write_text("# different bytes\n")
        result, control = self.run_lease("complete", package)
        self.assertIn("volume holds different bytes", result["error"])
        self.assertFalse(result["complete"])
        self.assertFalse((control / "worker-started.json").exists())
        self.assertTrue(result["recovered"])
        self.released(result, control)


if __name__ == "__main__":
    unittest.main()
