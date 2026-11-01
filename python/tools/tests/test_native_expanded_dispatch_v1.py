"""Admission and output-equivalence checks, without native/model execution."""
import copy
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import threading
import time
import unittest
import unittest.mock
from unittest.mock import patch

import host_reservation_v1 as reservations
import native_expanded_dispatch_v1 as dispatch

LINUX = sys.platform.startswith("linux")


def owner():
    """A start barrier's owner: this test process, which stays alive."""
    pid, creation = reservations.self_identity()
    return {"pid": pid, "creation_time": creation}


def lease_guard_files(folder, pod="testpod", now=None, **guard):
    """A valid lease (lease_guard.validate) and a fresh unlatched guard.json for it."""
    now = time.time() if now is None else now
    lease = {"schema": "phase1-cloud-lease/v1", "name": "nine-deck-test", "network_volume_id": "volume",
             "created_epoch": now - 600, "deadline_epoch": now - 600 + 4 * 3600, "increment_cap_usd": 10,
             "total_cap_usd": 100, "prior_conservative_usd": 0, "rate_ceiling_usd_hour": 1.0,
             "storage_usd_hour": 0.1, "postrun_storage_reserve_usd": 0.5, "recovery_reserve_usd": 0.5,
             "funded_balance_usd": 50, "safety_multiplier": 1.25, "startup_idle_seconds": 900,
             "work_idle_seconds": 300, "recovery_seconds": 600, "poll_seconds": 30}
    status = {"reason": None, "funds_verified": True, "allow_new_dispatch": True, "funding_alert": None,
              "pod_id": pod, "name": lease["name"], "epoch": now, "pid": os.getpid(), "provider_verified": True,
              "provider_ok": True, "highest_rate_usd_hour": 1.0, "latched": None, "release_epoch": None} | guard
    Path(folder).mkdir(parents=True, exist_ok=True)
    (Path(folder) / "lease.json").write_text(json.dumps(lease), encoding="utf-8")
    (Path(folder) / "guard.json").write_text(json.dumps(status), encoding="utf-8")
    return lease


class AdmissionTestCase(unittest.TestCase):
    """A fixture config, runtime and serial/parallel qualification trials."""

    def setUp(self):
        dispatch.reset_emergency()  # this process's walks: each test starts as a fresh dispatch
        self.addCleanup(dispatch.reset_emergency)
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.number = 0
        self.runtime = {"engine_commit": "a" * 40, "tracked_tree_sha256": "b" * 64,
                        "binary": self.save({"test": "immutable executable pin"})}
        self.config = {
            "initial_source": {"checkpoint": {"path": "model.json", "sha256": "a" * 64}},
            "opponents": [{"id": "t1", "source": {"checkpoint": "fixed"}}],
            "iterations": [{"episodes": [{"episode": {"id": f"training-{i}", "seed": 17 + i,
                "learner_seat": 0, "registered": ["deck-a", "deck-b"]}, "opponent": "t1"}]} for i in range(64)],
            "learning_rate": .0001, "update_backend": {"kind": "cpu"},
            "loss_selection": {"lambda": .9}, "collection_workers": 2, "preparation_workers": 2,
            "output_directory": str(self.root / "production")}

    def save(self, value, path=None):
        self.number += 1
        path = path or self.root / (str(self.number) + ".json")
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(value), encoding="utf-8")
        return dispatch.pin(path)

    def request(self, workers):
        return {"schema": dispatch.SCHEMA, "kind": "training", "runtime": self.save(self.runtime),
                "config": self.save(self.config), "root": str(self.root/f"trial-{workers}"),
                "cold_root": str(self.root/f"cold-{workers}"),
                "placement": {"host": "desktop", "workers": workers, "preparation_workers": workers,
                              "cpu_affinity": [0, 1], "memory_bytes": 1024**3},
                "wall_seconds": 1000,
                "storage": {"accounting_roots": [str(self.root)], "max_logical_bytes": 1024**3,
                            "reserve_bytes": dispatch.DISK_RESERVE_BYTES, "projected_additional_bytes": 1000,
                            "projected_volume_bytes": {self.root.drive.upper(): 1000}}}

    def trial(self, workers, seconds, e_packet=None, update_seconds=None, stdout_updates=1,
              artifact_timing=None):
        """A one-update qualification report. With `update_seconds` it pins a
        stdout whose `stdout_updates` update lines carry that native timing, and
        records `artifact_timing` (by default a 1 s fingerprint and a 0.5 s
        archive; {} leaves it out)."""
        request = self.request(workers)
        original = dispatch.read(dispatch.checked(request["config"]))
        config = dispatch.placed_config(request, original, True)
        native = Path(config["output_directory"])
        self.save({"git_commit": self.runtime["engine_commit"],
                   "tracked_tree_sha256": self.runtime["tracked_tree_sha256"]}, native/"run.json")
        trajectory = self.save({"terminal": {"terminal_classification": "natural"}, "tensors": [1, 2]},
                               native/"episode.json")
        collection = self.save({"complete": True, "trajectories": [trajectory]}, native/"collection.json")
        e_supported = None
        if e_packet is not None:
            packet = self.save(e_packet, native/"update"/"e-supported-teacher.json")
            e_supported = {"continuation": {"treatment": True}, "packet": packet}
        checkpoint = self.save({"trajectories": [trajectory], "parameters": [123],
                                "first_moments": [456], "second_moments": [789], "adam_step": 32401}
                               | ({"e_supported": e_supported} if e_supported else {}),
                               native/"checkpoint.json")
        update = self.save({"complete": True, "checkpoint": checkpoint}
                           | ({"e_supported": {"packet": e_supported["packet"],
                                               "scheduled": e_packet["scheduled"]}} if e_supported else {}))
        receipt = self.save({"iteration": 0, "collection": collection, "update": update})
        result = {"complete": False, "completed_iterations": 1, "planned_iterations": 64,
                  "iterations": [receipt]}
        actual = dispatch.output_fingerprint(config, "training", result, self.runtime, True)
        report = {"schema": dispatch.SCHEMA, "qualification": True, "complete": True,
                  "request": self.save(request), "executed_config": self.save(config),
                  "native_result": self.save(result), "runtime": self.runtime,
                  "launcher_sha256": dispatch.pin(dispatch.__file__)["sha256"],
                  "workload": dispatch.workload(original, "training"), "placement": request["placement"],
                  "fingerprint": actual, "completed_games": 1, "completed_updates": 1, "seconds": seconds,
                  "execution": self.save({"exit_code": 0, "error": None, "seconds": seconds - 1}),
                  "run": dispatch.pin(native/"run.json"),
                  "archive": self.save({"mismatches": 0, "native_root": str(native),
                      "scheme": "two-deflate1-shards-full-readback/v1",
                      "shards": [{"archive": self.save({"archive": i})} for i in range(2)]})}
        if update_seconds is not None:
            stdout = Path(request["root"]) / "stdout.jsonl"
            stdout.parent.mkdir(parents=True, exist_ok=True)
            timing = {"schema": "phase1-scheduler-wall/v1", "iteration_wall_seconds": update_seconds}
            stdout.write_text("".join(json.dumps({"completed_iteration": i, "scheduler_timing": timing}) + "\n"
                                      for i in range(stdout_updates)) + json.dumps(result) + "\n",
                              encoding="utf-8")
            report["native_stdout"] = dispatch.pin(stdout)
            artifact = ({"fingerprint_seconds": 1, "archive_seconds": .5}
                        if artifact_timing is None else artifact_timing)
            if artifact:
                report["artifact_timing"] = artifact
        return self.save(report)

    def choice(self):
        one, two = self.trial(1, 10), self.trial(2, 6)
        inventory = {host: {"eligible": host == "desktop", "reason": "test placement census",
                            "checked_at": datetime.now(timezone.utc).isoformat(),
                            "evidence": self.save({"host": host}), "cpu_affinity": [0, 1],
                            "transport_seconds": 0} for host in ("desktop", "computehost", "runpod")}
        return {"schema": dispatch.CHOICE, "inventory": inventory,
                "qualifications": [one, two],
                "selected": {dispatch.workload(self.config, "training"): two}}


class NativeExpandedAdmissionTests(AdmissionTestCase):
    def test_replication_seeds_only_are_compatible(self):
        replica = copy.deepcopy(self.config)
        replica["iterations"][0]["episodes"][0]["episode"].update(id="replica", seed=999)
        self.assertEqual(dispatch.workload(replica, "training"), dispatch.workload(self.config, "training"))
        self.assertNotEqual(dispatch.workload(replica, "training", True),
                            dispatch.workload(self.config, "training", True))
        for key, value in (("learning_rate", .001), ("loss_selection", {"lambda": 1.0})):
            changed = copy.deepcopy(self.config); changed[key] = value
            self.assertNotEqual(dispatch.workload(changed, "training"), dispatch.workload(self.config, "training"))

    def test_single_pass_pinned_reader_rejects_same_path_replacement(self):
        item = self.save({"value": 1})
        validated = {}
        self.assertEqual(dispatch.read_pinned_json(item, validated_files=validated), {"value": 1})
        self.assertEqual(validated[str(Path(item["path"]).resolve())], item["sha256"])
        Path(item["path"]).write_text('{"value": 2}', encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "changed pinned input"):
            dispatch.read_pinned_json(item, validated_files=validated)

    def test_inference_shape_rehashes_before_reusing_parsed_shape(self):
        value = {"schema": "test", "source_import": "test", "feature_contract_digest": "a",
                 "feature_encoding_digest": "b", "card_db_hash": "c",
                 "parameters": [{"name": "a", "shape": [1], "values": [1]}]}
        item = self.save(value)
        dispatch._INFERENCE_SHAPES.clear()
        shape = dispatch.inference_shape({"checkpoint": item})
        shape["parameters"].clear()
        with patch.object(dispatch.json, "loads", side_effect=AssertionError("reparsed immutable checkpoint")):
            self.assertEqual(len(dispatch.inference_shape({"checkpoint": item})["parameters"]), 1)
        source = Path(item["path"])
        stat = source.stat()
        source.write_bytes(source.read_bytes().replace(b'"values": [1]', b'"values": [2]'))
        os.utime(source, ns=(stat.st_atime_ns, stat.st_mtime_ns))
        with self.assertRaisesRegex(ValueError, "changed pinned input"):
            dispatch.inference_shape({"checkpoint": item})
        replacement = dispatch.pin(source)
        self.assertEqual(dispatch.inference_shape({"checkpoint": replacement})["parameters"],
                         [("a", [1], 1)])

    def test_single_pass_recovery_reader_checks_local_bytes(self):
        source = self.root / "absent-source"
        recovered = self.root / "recovered"
        recovered.mkdir()
        local = self.save({"value": 1}, recovered / "checkpoint.json")
        item = {**local, "path": str(source / "checkpoint.json")}
        resolver = dispatch.artifact_reader([{"source_root": str(source), "local_root": str(recovered)}])
        self.assertEqual(dispatch.read_pinned_json(item, resolver), {"value": 1})
        Path(local["path"]).write_text('{"value": 2}', encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "changed pinned input"):
            dispatch.read_pinned_json(item, resolver)

    def test_serial_parallel_identical_saved_learning_outputs(self):
        first, second = dispatch.read(self.trial(1, 10)["path"]), dispatch.read(self.trial(2, 6)["path"])
        self.assertEqual(first["fingerprint"], second["fingerprint"])
        result = dispatch.read(second["native_result"]["path"])
        receipt = dispatch.read(result["iterations"][0]["path"])
        update = dispatch.read(receipt["update"]["path"])
        checkpoint = dispatch.read(update["checkpoint"]["path"])
        legacy_checkpoint = copy.deepcopy(checkpoint)
        legacy_checkpoint["trajectories"] = [item["sha256"] for item in checkpoint["trajectories"]]
        self.assertEqual(second["fingerprint"]["iterations"][0]["checkpoint_bits"],
                         dispatch.digest(legacy_checkpoint))
        checkpoint["first_moments"][0] += 1
        update["checkpoint"] = self.save(checkpoint)
        receipt["update"] = self.save(update)
        result["iterations"][0] = self.save(receipt)
        actual = dispatch.output_fingerprint(dispatch.read(second["executed_config"]["path"]),
                                             "training", result, self.runtime, True)
        self.assertNotEqual(first["fingerprint"], actual)

    def test_e_packet_is_bound_by_content_not_output_root(self):
        packet = {"schema": "mtg-kernel-e-supported-teacher-packet/v1", "scheduled": 1, "valid": 1}
        first = dispatch.read(self.trial(1, 10, packet)["path"])
        second = dispatch.read(self.trial(2, 6, packet)["path"])
        self.assertEqual(first["fingerprint"], second["fingerprint"])
        self.assertIn("e_packet_sha256", first["fingerprint"]["iterations"][0])
        changed = dispatch.read(self.trial(3, 6, packet | {"valid": 0})["path"])
        self.assertNotEqual(first["fingerprint"], changed["fingerprint"])
        result = dispatch.read(second["native_result"]["path"])
        receipt = dispatch.read(result["iterations"][0]["path"])
        update = dispatch.read(receipt["update"]["path"])
        update["e_supported"]["scheduled"] = 2
        receipt["update"] = self.save(update)
        result["iterations"][0] = self.save(receipt)
        with self.assertRaisesRegex(ValueError, "teacher packet"):
            dispatch.output_fingerprint(dispatch.read(second["executed_config"]["path"]),
                                        "training", result, self.runtime, True)

    def test_e_case_ids_are_replica_identifiers_but_slots_are_bound(self):
        config = copy.deepcopy(self.config)
        config["e_supported"] = {"treatment": True, "slots": [[{"trajectory_index": 0, "case_id": "r1/u0"}]]}
        replica = copy.deepcopy(config)
        replica["e_supported"]["slots"][0][0]["case_id"] = "r2/u0"
        self.assertEqual(dispatch.workload(replica, "training"), dispatch.workload(config, "training"))
        self.assertNotEqual(dispatch.workload(config, "training"), dispatch.workload(self.config, "training"))
        moved = copy.deepcopy(config)
        moved["e_supported"]["slots"][0][0]["trajectory_index"] = 1
        self.assertNotEqual(dispatch.workload(moved, "training"), dispatch.workload(config, "training"))
        control = copy.deepcopy(config)
        control["e_supported"]["treatment"] = False
        self.assertNotEqual(dispatch.workload(control, "training"), dispatch.workload(config, "training"))

    def test_exposure_projection_collection_preserves_fingerprint(self):
        report = dispatch.read(self.trial(1, 10)["path"])
        config = dispatch.read(report["executed_config"]["path"])
        result = dispatch.read(report["native_result"]["path"])
        projected = []
        actual = dispatch.output_fingerprint(config, "training", result, self.runtime, True,
                                             exposure_inputs=projected)
        self.assertEqual(actual, report["fingerprint"])
        self.assertEqual(len(projected), 1)
        self.assertEqual(projected[0][0]["source"]["sha256"], actual["iterations"][0]["trajectories"][0])

    def production(self, updates, trajectories=3):
        """A complete run of ``updates`` updates, each with its own trajectories and checkpoint."""
        config = copy.deepcopy(self.config)
        native = self.root / "production"
        config["output_directory"] = str(native)
        config["iterations"] = [{"episodes": [{"episode": {"id": f"u{index}-{t}", "seed": 17 + t,
                                                          "learner_seat": t % 2, "registered": ["deck-a", "deck-b"]},
                                              "opponent": "t1"} for t in range(trajectories)]}
                                for index in range(updates)]
        self.save({"git_commit": self.runtime["engine_commit"],
                   "tracked_tree_sha256": self.runtime["tracked_tree_sha256"]}, native/"run.json")
        receipts = []
        for index in range(updates):
            folder = native/f"{index:06d}"
            pins = [self.save({"terminal": {"terminal_classification": "natural"}, "tensors": [index, t]},
                              folder/f"episode-{t}.json") for t in range(trajectories)]
            collection = self.save({"complete": True, "trajectories": pins}, folder/"collection.json")
            checkpoint = self.save({"trajectories": pins, "parameters": [index, 2.5], "first_moments": [index],
                                    "second_moments": [1e-30], "adam_step": index + 1}, folder/"checkpoint.json")
            update = self.save({"complete": True, "checkpoint": checkpoint}, folder/"update.json")
            receipts.append(self.save({"iteration": index, "collection": collection, "update": update},
                                      folder/"complete.json"))
        return config, {"complete": True, "completed_iterations": updates, "planned_iterations": updates,
                        "iterations": receipts}

    def test_fingerprint_workers_match_serial_and_raise_the_first_failure_in_order(self):
        config, result = self.production(5)
        outputs = {}
        for workers in (1, 3, 8):
            validated, timing, projected = {}, {}, []
            fingerprint = dispatch.output_fingerprint(config, "training", result, self.runtime, False,
                                                      validated_files=validated, timing=timing,
                                                      exposure_inputs=projected, workers=workers)
            outputs[workers] = (fingerprint, validated, projected, sorted(timing))
        self.assertEqual(outputs[3], outputs[1])
        self.assertEqual(outputs[8], outputs[1])
        self.assertEqual(len(outputs[1][0]["iterations"]), 5)
        # Receipt, collection, three trajectories, update and checkpoint per update.
        self.assertEqual(len(outputs[1][1]), 5 * 7)
        # Changed bytes in updates 3 and 1: every worker count reports update 1's file.
        for index in (3, 1):
            receipt = dispatch.read(result["iterations"][index]["path"])
            collection = dispatch.read(receipt["collection"]["path"])
            Path(collection["trajectories"][0]["path"]).write_text("{}", encoding="utf-8")
        errors = []
        for workers in (1, 3):
            with self.assertRaisesRegex(ValueError, "changed pinned input") as caught:
                dispatch.output_fingerprint(config, "training", result, self.runtime, False, workers=workers)
            errors.append(str(caught.exception))
        self.assertEqual(errors[1], errors[0])
        self.assertIn("000001", errors[0])
        # One process per placed CPU, within the placement's memory limit.
        for cpus, memory, expected in ((16, 16 * 1024**3, 16), (16, 4 * 1024**3, 4), (2, 512 * 1024**2, 1)):
            self.assertEqual(dispatch.fingerprint_workers({"cpu_affinity": list(range(cpus)),
                                                           "memory_bytes": memory}), expected)

    def test_incomplete_non_natural_and_wrong_runtime_are_refused(self):
        report = dispatch.read(self.trial(1, 10)["path"])
        config = dispatch.read(report["executed_config"]["path"])
        result = dispatch.read(report["native_result"]["path"])
        bad = {**result, "completed_iterations": 0}
        with self.assertRaisesRegex(ValueError, "coverage"):
            dispatch.output_fingerprint(config, "training", bad, self.runtime, True)
        with self.assertRaisesRegex(ValueError, "runtime"):
            dispatch.output_fingerprint(config, "training", result,
                                        {**self.runtime, "engine_commit": "c" * 40}, True)
        trajectory = self.save({"terminal": {"terminal_classification": "truncated"}})
        collection = self.save({"complete": True, "trajectories": [trajectory]})
        with self.assertRaisesRegex(ValueError, "non-natural"):
            dispatch.collection_fingerprint(collection, 1)

    def test_profile_gae_is_boolean_and_training_only(self):
        request = self.request(1)
        self.assertFalse(dispatch.profile_gae_requested(request))
        for value in (True, False):
            request["profile_gae"] = value
            self.assertEqual(dispatch.profile_gae_requested(request), value)
        for value in (1, 0, "true", None, [], {}):
            request["profile_gae"] = value
            with self.assertRaisesRegex(ValueError, "profile_gae must be Boolean"):
                # Validation rejects malformed requests before runtime admission.
                dispatch.validate_request(request, True)
        request["kind"] = "evaluation"
        for value in (True, False):
            request["profile_gae"] = value
            with self.assertRaisesRegex(ValueError, "training requests only"):
                dispatch.validate_request(request, True)
        del request["profile_gae"]
        self.assertFalse(dispatch.profile_gae_requested(request))

    def test_native_child_environment_sets_or_clears_profile_flag(self):
        request = self.request(1)
        with patch.dict(os.environ, {"MTG_KERNEL_PROFILE_GAE_V1": "inherited", "PROFILE_TEST_KEEP": "yes"}):
            environment = dispatch.native_child_environment(request)
            self.assertNotIn("MTG_KERNEL_PROFILE_GAE_V1", environment)
            self.assertEqual(environment["PROFILE_TEST_KEEP"], "yes")
            request["profile_gae"] = True
            self.assertEqual(dispatch.native_child_environment(request)["MTG_KERNEL_PROFILE_GAE_V1"], "1")
            request["profile_gae"] = False
            self.assertNotIn("MTG_KERNEL_PROFILE_GAE_V1", dispatch.native_child_environment(request))
            self.assertEqual(os.environ["MTG_KERNEL_PROFILE_GAE_V1"], "inherited")
        with patch.dict(os.environ, {}, clear=True):
            request["profile_gae"] = True
            self.assertEqual(dispatch.native_child_environment(request), {"MTG_KERNEL_PROFILE_GAE_V1": "1"})

    def test_execute_passes_request_profile_flag_to_native_child(self):
        for index, value in enumerate((None, False, True)):
            request = self.request(1)
            request["root"] += f"-profile-{index}"
            request["cold_root"] += f"-profile-{index}"
            if value is not None:
                request["profile_gae"] = value
            request_ref = self.save(request)
            # Stop immediately after the mocked spawn. No native process or
            # fingerprint/archive work is performed by this propagation check.
            with patch.dict(os.environ, {"MTG_HOST_RESERVATION_TOKEN": "test-token",
                                         "MTG_KERNEL_PROFILE_GAE_V1": "inherited"}), \
                    patch.object(dispatch, "validate_request", return_value=(self.config, self.runtime)), \
                    patch.object(dispatch, "set_affinity"), \
                    patch("host_reservation_v1.status", return_value={"token_fate": "holds"}), \
                    patch("host_reservation_v1.record_descendant", side_effect=RuntimeError("mocked spawn complete")), \
                    patch.object(dispatch.subprocess, "Popen") as spawn:
                with self.assertRaisesRegex(RuntimeError, "mocked spawn complete"):
                    dispatch.execute(Path(request_ref["path"]), True)
                environment = spawn.call_args.kwargs["env"]
                if value:
                    self.assertEqual(environment["MTG_KERNEL_PROFILE_GAE_V1"], "1")
                else:
                    self.assertNotIn("MTG_KERNEL_PROFILE_GAE_V1", environment)

    def test_execute_waits_at_the_start_barrier_outside_its_seconds(self):
        request = self.request(1)
        release = self.root / "release.json"
        request["start_barrier"] = {"ready": str(self.root / "ready.json"), "release": str(release),
                                    "timeout_seconds": 30, "owner": owner()}
        request_ref = self.save(request)

        def popen(*args, **kwargs):
            self.assertTrue(release.exists())  # the native child starts only after the release
            child = unittest.mock.MagicMock(pid=4242, returncode=1)
            child.poll.return_value = 1
            return child
        recorded = []

        def record(token, pid):  # the reservation's lock, which packed trials take in turn
            recorded.append(time.time())
            time.sleep(0.2)
        timer = threading.Timer(0.5, lambda: release.write_text('{"go": true}'))
        self.addCleanup(timer.cancel)
        with patch.dict(os.environ, {"MTG_HOST_RESERVATION_TOKEN": "test-token"}), \
                patch.object(dispatch, "validate_request", return_value=(self.config, self.runtime)), \
                patch.object(dispatch, "set_affinity"), \
                patch.object(dispatch, "require_mapped_libraries"), \
                patch("host_reservation_v1.status", return_value={"token_fate": "holds"}), \
                patch("host_reservation_v1.record_descendant", side_effect=record), \
                patch.object(dispatch.subprocess, "Popen", side_effect=popen) as spawn:
            timer.start()
            with self.assertRaisesRegex(RuntimeError, "native process failed"):
                dispatch.execute(Path(request_ref["path"]), True)
        self.assertEqual(spawn.call_count, 1)
        self.assertEqual(dispatch.read(self.root / "ready.json")["request"], request_ref)
        execution = dispatch.read(Path(request["root"]) / "execution.json")
        self.assertGreaterEqual(execution["barrier_seconds"], 0.4)
        self.assertLess(execution["seconds"], execution["barrier_seconds"])
        # The start is taken at the spawn, before the reservation's lock, and the receipt binds it.
        started = dispatch.read(Path(request["root"]) / "started.json")
        self.assertLessEqual(started["started_unix"], recorded[0])
        self.assertEqual(execution["started_unix"], started["started_unix"])

    def test_only_a_qualification_receipt_records_its_start(self):
        """A production dispatch's execution record keeps its fields; a qualification's adds its start."""
        def popen(*args, **kwargs):
            child = unittest.mock.MagicMock(pid=4242, returncode=1)
            child.poll.return_value = 1
            return child
        for qualification in (False, True):
            request = self.request(1)
            request["root"] += f"-{qualification}"
            request["cold_root"] += f"-{qualification}"
            if not qualification:
                request["choice"] = {"path": "admitted", "sha256": "0" * 64}
            request_ref = self.save(request)
            with patch.dict(os.environ, {"MTG_HOST_RESERVATION_TOKEN": "test-token"}), \
                    patch.object(dispatch, "validate_request", return_value=(self.config, self.runtime)), \
                    patch.object(dispatch, "require_choice"), \
                    patch.object(dispatch, "placed_config", side_effect=lambda request, config, qualification: config), \
                    patch.object(dispatch, "set_affinity"), \
                    patch.object(dispatch, "require_mapped_libraries"), \
                    patch("host_reservation_v1.status", return_value={"token_fate": "holds"}), \
                    patch("host_reservation_v1.record_descendant"), \
                    patch.object(dispatch.subprocess, "Popen", side_effect=popen):
                with self.assertRaisesRegex(RuntimeError, "native process failed"):
                    dispatch.execute(Path(request_ref["path"]), qualification)
            execution = dispatch.read(Path(request["root"]) / "execution.json")
            started = dispatch.read(Path(request["root"]) / "started.json")
            if qualification:
                self.assertEqual(sorted(execution), ["child_seconds", "error", "exit_code", "seconds", "started_unix"])
                self.assertEqual(execution["started_unix"], started["started_unix"])
            else:
                self.assertEqual(sorted(execution), ["child_seconds", "error", "exit_code", "seconds"])

    def test_fastest_choice_rejects_profiled_qualification(self):
        choice = self.choice()
        request = self.request(2)
        report = dispatch.read(choice["qualifications"][0]["path"])
        original = dispatch.read(report["request"]["path"])
        original["profile_gae"] = True
        report["request"] = self.save(original)
        choice["qualifications"][0] = self.save(report)
        with patch.object(dispatch, "validate_request", return_value=(self.config, self.runtime)):
            with self.assertRaisesRegex(ValueError, "profiled qualification"):
                dispatch.require_choice(self.save(choice), request)

    def test_fastest_choice_requires_complete_serial_parallel_evidence(self):
        choice = self.choice()
        request = self.request(2)
        with patch.object(dispatch, "validate_request", return_value=(self.config, self.runtime)):
            choice_ref = self.save(choice)
            result = dispatch.require_choice(choice_ref, request, verify_outputs=True)
            self.assertEqual(result["projected_seconds"], 384)
            request["choice_verification"] = self.save({"schema": dispatch.CHOICE, "choice": choice_ref,
                                                       "selected": result, "outputs_verified": True})
            self.assertEqual(dispatch.require_choice(choice_ref, request), result)
            slower = copy.deepcopy(choice)
            slower["selected"][dispatch.workload(self.config, "training")] = slower["qualifications"][0]
            with self.assertRaisesRegex(ValueError, "fastest"):
                dispatch.require_choice(self.save(slower), request)
            choice["qualifications"] = choice["qualifications"][1:]
            with self.assertRaisesRegex(ValueError, "serial"):
                dispatch.require_choice(self.save(choice), request, verify_outputs=True)

    def test_training_projection_pays_start_up_once(self):
        # 64 updates. w1: a 10 s trial with a 6 s update, a 1 s fingerprint and a
        # 0.5 s archive pays 2.5 s once, then 6.5 s per update and the
        # fingerprint over one worker (1 GiB placement): 2.5 + 64 x 7.5.
        # w2: 6 s, 4 s, 1 s, 0.5 s.
        choice = self.choice()
        choice["qualifications"] = [self.trial(1, 10, update_seconds=6), self.trial(2, 6, update_seconds=4)]
        choice["selected"] = {dispatch.workload(self.config, "training"): choice["qualifications"][1]}
        report = dispatch.read(choice["qualifications"][0]["path"])
        self.assertEqual(dispatch.projected_seconds(report, self.config, "training"), 482.5)
        # Four fingerprint workers spread the fingerprint: 2.5 + 64 x (6.5 + 1/4).
        wide = report | {"placement": report["placement"] | {"cpu_affinity": [0, 1, 2, 3],
                                                              "memory_bytes": 4 * 1024**3}}
        self.assertEqual(dispatch.fingerprint_workers(wide["placement"]), 4)
        self.assertEqual(dispatch.projected_seconds(wide, self.config, "training"), 434.5)
        with patch.object(dispatch, "validate_request", return_value=(self.config, self.runtime)):
            result = dispatch.require_choice(self.save(choice), self.request(2), verify_outputs=True)
        self.assertEqual(result["projected_seconds"], .5 + 64 * 5.5)

    def test_training_projection_bases_and_timing_are_checked(self):
        choice = self.choice()
        request = self.request(2)
        with patch.object(dispatch, "validate_request", return_value=(self.config, self.runtime)):
            # One trial timed per update and one from before the pinned stdout.
            choice["qualifications"][1] = self.trial(2, 6, update_seconds=4)
            with self.assertRaisesRegex(ValueError, "one projection basis"):
                dispatch.require_choice(self.save(choice), request, verify_outputs=True)
            for kwargs, message in (({"update_seconds": 4, "stdout_updates": 0}, "exactly one update"),
                                    ({"update_seconds": 4, "stdout_updates": 2}, "exactly one update"),
                                    ({"update_seconds": 9.5}, "outlasts its native execution"),
                                    ({"update_seconds": 8.75}, "outlast its trial"),
                                    ({"update_seconds": 0}, "native update seconds"),
                                    ({"update_seconds": 4, "artifact_timing": {}}, "artifact timing"),
                                    ({"update_seconds": 4, "artifact_timing": {"fingerprint_seconds": 1}},
                                     "archive seconds")):
                report = dispatch.read(self.trial(1, 10, **kwargs)["path"])
                with self.assertRaisesRegex(ValueError, message):
                    dispatch.projected_seconds(report, self.config, "training")

    def test_changed_trajectory_and_eligible_unmeasured_host_are_refused(self):
        choice = self.choice()
        request = self.request(2)
        with patch.object(dispatch, "validate_request", return_value=(self.config, self.runtime)):
            other = copy.deepcopy(choice); other["inventory"]["computehost"]["eligible"] = True
            with self.assertRaisesRegex(ValueError, "lacks measured"):
                dispatch.require_choice(self.save(other), request)
            target = self.root/"trial-2/native/episode.json"
            target.write_text("changed")
            with self.assertRaisesRegex(ValueError, "changed pinned input"):
                dispatch.require_choice(self.save(choice), request, verify_outputs=True)

    def test_storage_includes_retained_outputs_and_projection(self):
        storage = self.request(2)["storage"]
        with patch.object(dispatch.shutil, "disk_usage", return_value=type("Disk", (), {"free": 100*1024**3})()):
            observed = dispatch.validate_storage(storage)["logical_bytes"]
            storage["max_logical_bytes"] = observed + 999
            with self.assertRaisesRegex(ValueError, "retained plus projected"):
                dispatch.validate_storage(storage)
            storage["max_logical_bytes"] = dispatch.CAP + 1
            with self.assertRaisesRegex(ValueError, "192 GiB"):
                dispatch.validate_storage(storage)

    def test_block_scratch_is_outside_the_cap_and_apart_from_accounting(self):
        storage = self.request(2)["storage"]
        other = tempfile.TemporaryDirectory()  # apart from the accounting root
        self.addCleanup(other.cleanup)
        scratch = Path(other.name)/"scratch"
        (scratch/"G1").mkdir(parents=True)
        (scratch/"G1"/"episode.json").write_bytes(b"x" * 10**6)
        with patch.object(dispatch.shutil, "disk_usage", return_value=type("Disk", (), {"free": 100*1024**3})()):
            before = dispatch.validate_storage(storage)["logical_bytes"]
            storage["scratch_roots"] = [str(scratch)]
            self.assertEqual(dispatch.validate_storage(storage)["logical_bytes"], before)
            storage["scratch_roots"] = [str(self.root/"inside")]
            with self.assertRaisesRegex(ValueError, "scratch root overlaps"):
                dispatch.validate_storage(storage)
            storage["scratch_roots"] = ["relative/scratch"]
            with self.assertRaisesRegex(ValueError, "absolute scratch roots"):
                dispatch.validate_storage(storage)

    def test_a_running_attempt_ends_only_on_a_physical_shortage(self):
        storage = self.request(2)["storage"]
        roomy = type("Disk", (), {"free": 100*1024**3, "total": 200*1024**3})()
        with patch.object(dispatch.shutil, "disk_usage", return_value=roomy):
            observed = dispatch.validate_storage(storage, 0)["logical_bytes"]
            storage["max_logical_bytes"] = observed - 1  # the cap (a dispatch gate) is passed
            with self.assertRaisesRegex(ValueError, "retained plus projected"):
                dispatch.running_storage(storage)  # without the opt-in, as before
            storage["emergency"] = {"volume_bytes": observed * 10, "margin_fraction": 0.1}
            self.assertEqual(dispatch.running_storage(storage)["logical_bytes"], observed)
            storage["emergency"]["volume_bytes"] = observed  # the provisioned volume less its margin is passed
            with self.assertRaisesRegex(ValueError, "storage emergency"):
                dispatch.running_storage(storage)
            storage["emergency"]["volume_bytes"] = observed * 10
            low = type("Disk", (), {"free": 1, "total": 200*1024**3})()
            with patch.object(dispatch.shutil, "disk_usage", return_value=low):  # a network volume's df says nothing
                self.assertEqual(dispatch.running_storage(storage)["logical_bytes"], observed)
            other = tempfile.TemporaryDirectory()
            self.addCleanup(other.cleanup)
            storage["scratch_roots"] = [other.name]
        tight = type("Disk", (), {"free": 10, "total": 1000})()
        with patch.object(dispatch.shutil, "disk_usage",
                          side_effect=lambda path: tight if Path(path) == Path(other.name).resolve() else roomy):
            with self.assertRaisesRegex(ValueError, "storage emergency: scratch volume"):
                dispatch.running_storage(storage)

    def test_storage_walk_survives_a_file_deleted_mid_walk(self):
        # Another job sharing the accounting root removes its outputs while the walk lists their folder.
        storage = self.request(2)["storage"]
        doomed = self.root/"sibling/native/raced-episode.json"
        doomed.parent.mkdir(parents=True)
        doomed.write_bytes(b"x" * 4096)
        scandir = dispatch.os.scandir

        def racing_scandir(path):
            listed = scandir(path)
            if Path(path) == doomed.parent.resolve() and doomed.exists():
                doomed.unlink()  # listed, then gone before its stat
            return listed

        with patch.object(dispatch.shutil, "disk_usage", return_value=type("Disk", (), {"free": 100*1024**3})()):
            with patch.object(dispatch.os, "scandir", racing_scandir):
                raced = dispatch.validate_storage(storage, 0)["logical_bytes"]
            self.assertFalse(doomed.exists())
            after = dispatch.validate_storage(storage, 0)["logical_bytes"]
            # freed (Linux: its lstat fails) or counted from the listing (Windows caches it): never an error
            self.assertIn(raced, (after, after + 4096))

    def test_the_emergency_sample_walks_at_most_every_ten_minutes(self):
        storage = self.request(2)["storage"]
        storage["emergency"] = {"volume_bytes": 10**12, "margin_fraction": 0.1}
        walks = []
        real = dispatch.accounting_bytes
        with patch.object(dispatch, "accounting_bytes", lambda s: walks.append(1) or real(s)):
            dispatch.running_storage(storage)  # nothing to sample yet: the one walk in place
            dispatch.running_storage(storage)
            self.assertEqual(len(walks), 1)
            dispatch.EMERGENCY_WALK["at"] -= dispatch.EMERGENCY_WALK_SECONDS + 1
            dispatch.running_storage(storage)  # due: in the background, the sample uses the last walk
            dispatch.EMERGENCY_WALK["thread"].join(10)
            self.assertEqual(len(walks), 2)
            dispatch.running_storage(storage)
            self.assertEqual(len(walks), 2)

    def admission(self, logical=1000, finished=0, age=5.0, shared=None, seconds=10.0, in_flight=0):
        """The driver's admission walk: completed ``age`` seconds ago after ``seconds`` of walking."""
        completed = time.time() - age
        return {"schema": dispatch.ADMISSION_SCHEMA, "logical_bytes": logical, "finished_since": finished,
                "walked_unix": completed - seconds, "completed_unix": completed, "shared_walk": shared,
                "in_flight": in_flight}

    def test_an_admission_walk_spares_the_dispatcher_its_own(self):
        """#463: the driver's admission walk passes in the request; the cap, reserve and projections are still
        checked on its numbers, and a stale or absent one means a walk as before."""
        storage = self.request(2)["storage"]
        roomy = type("Disk", (), {"free": 100*1024**3})()
        walks = []
        real = dispatch.accounting_bytes
        with patch.object(dispatch.shutil, "disk_usage", return_value=roomy), \
                patch.object(dispatch, "accounting_bytes", lambda s: walks.append(1) or real(s)):
            checked = dispatch.validate_storage(storage, admission=self.admission(logical=5000, finished=2))
            self.assertEqual((checked["logical_bytes"], checked["source"], walks), (5000 + 2 * 1000, "admission", []))
            self.assertEqual(dispatch.LAST_STORAGE["logical"], 7000)
            storage["max_logical_bytes"] = 7000 + 999  # the bound plus this block's projection passes the cap
            with self.assertRaisesRegex(ValueError, "retained plus projected"):
                dispatch.validate_storage(storage, admission=self.admission(logical=5000, finished=2))
            self.assertEqual(walks, [])
            storage["max_logical_bytes"] = 1024**3
            # The driver admits on a walk for 600 s after it completed; a long walk is judged by its completion.
            long_walk = self.admission(age=dispatch.ADMISSION_WALK_SECONDS - 60, seconds=665)
            self.assertEqual(dispatch.validate_storage(storage, admission=long_walk)["source"], "admission")
            stale = self.admission(age=dispatch.ADMISSION_WALK_SECONDS + 1)
            self.assertEqual(dispatch.validate_storage(storage, admission=stale)["source"], "walk")
            future = self.admission(age=-dispatch.CLOCK_SKEW_SECONDS - 1)
            self.assertEqual(dispatch.validate_storage(storage, admission=future)["source"], "walk")
            self.assertEqual(dispatch.validate_storage(storage)["source"], "walk")
            self.assertEqual(len(walks), 3)
            backwards = self.admission()
            backwards["completed_unix"] = backwards["walked_unix"] - 1
            for bad in ({"schema": "other"}, self.admission(logical=-1), self.admission(finished=1.5),
                    self.admission(in_flight=-1),
                        self.admission() | {"walked_unix": True}, backwards, self.admission(shared="relative/walk.json")):
                with self.assertRaises(ValueError):
                    dispatch.validate_storage(storage, admission=bad)
            storage["accounting_roots"] = ["relative"]  # the roots are checked without a walk too
            with self.assertRaisesRegex(ValueError, "absolute accounting roots"):
                dispatch.validate_storage(storage, admission=self.admission())

    def test_validation_seeds_the_emergency_samples_aged_by_the_walk_start(self):
        storage = self.request(2)["storage"]
        storage["emergency"] = {"volume_bytes": 10**12, "margin_fraction": 0.1}
        with patch.object(dispatch.shutil, "disk_usage", return_value=type("Disk", (), {"free": 100*1024**3})()):
            checked = dispatch.validate_storage(storage, admission=self.admission(logical=2242, age=120, seconds=30,
                                                                                 in_flight=2))
        self.assertEqual(checked["logical_bytes"], 2242)  # the cap check is the driver's bound
        dispatch.seed_emergency()
        self.assertEqual(dispatch.EMERGENCY_WALK["logical"], 2242 + 2 * 1000)  # with the siblings in flight
        self.assertAlmostEqual(time.monotonic() - dispatch.EMERGENCY_WALK["at"], 150, delta=5)
        with patch.object(dispatch, "accounting_bytes", side_effect=AssertionError("no walk")):
            self.assertEqual(dispatch.running_storage(storage)["logical_bytes"], 4242)
            # Own outputs count unless the sample's walk started after they were written.
            self.assertEqual(dispatch.running_storage(storage, pending=[(8, time.time())])["logical_bytes"], 4250)
            self.assertEqual(dispatch.running_storage(storage, pending=[(8, time.time() - 1000)])["logical_bytes"],
                             4242)

    def test_a_published_walk_serves_the_samples(self):
        """Eight attempts walking at once took 40 min a walk: with the driver's published walk (its one background
        walker), a sample reads it and the attempt walks only once the walker has stopped, never after its native
        part."""
        storage = self.request(2)["storage"]  # projected_additional_bytes 1000
        storage["emergency"] = {"volume_bytes": 10**12, "margin_fraction": 0.1}
        shared = self.root / "state" / "volume-walk.json"
        shared.parent.mkdir()
        walks = []
        now = time.time()
        dispatch.EMERGENCY_WALK.update(logical=1, unix=now - 5000, at=time.monotonic() - 5000)
        with patch.object(dispatch, "accounting_bytes", lambda s: walks.append(1) or ([], 777)):
            dispatch.publish_walk(shared, 555, now - 700, now - 30, "driver-background")
            self.assertEqual(dispatch.running_storage(storage, str(shared))["logical_bytes"], 555)
            self.assertEqual(dispatch.running_storage(storage, str(shared), pending=[(5, time.time())])
                             ["logical_bytes"], 560)
            dispatch.publish_walk(shared, 444, now - 900, now - 200, "driver")  # older than the sample: kept
            self.assertEqual(dispatch.running_storage(storage, str(shared))["logical_bytes"], 555)
            # A healthy walker between walks: a 665 s walk completed 1,900 s ago, the next not yet published.
            dispatch.publish_walk(shared, 600, now - 2565, now - 1900, "driver-background", in_flight=3)
            dispatch.EMERGENCY_WALK.update(unix=now - 2600)
            self.assertEqual(dispatch.running_storage(storage, str(shared))["logical_bytes"], 600 + 3 * 1000)
            self.assertIsNone(dispatch.EMERGENCY_WALK["thread"])  # no walk of its own while the walker is live
            self.assertEqual(walks, [])
            stopped = now - dispatch.EMERGENCY_FALLBACK_SECONDS - 60  # the walker stopped publishing
            dispatch.publish_walk(shared, 444, stopped - 600, stopped, "driver")
            dispatch.EMERGENCY_WALK.update(unix=stopped - 10)
            dispatch.close_emergency()
            dispatch.running_storage(storage, str(shared))
            self.assertIsNone(dispatch.EMERGENCY_WALK["thread"])  # after the native part: no walk of its own
            dispatch.EMERGENCY_WALK["closed"] = False
            dispatch.running_storage(storage, str(shared))  # in the native part: walks for itself, aside
            dispatch.EMERGENCY_WALK["thread"].join(10)
            self.assertEqual((walks, dispatch.running_storage(storage, str(shared))["logical_bytes"]), ([1], 777))
            dispatch.running_storage(storage, str(shared))
            self.assertEqual(walks, [1])  # its own walk is recent
        for record in ("{not json", json.dumps({"schema": dispatch.SHARED_WALK_SCHEMA, "logical_bytes": 1,
                                                "walked_unix": 10.0, "completed_unix": 5.0}),
                       json.dumps({"schema": dispatch.SHARED_WALK_SCHEMA, "logical_bytes": 1, "walked_unix": 1.0,
                                   "completed_unix": 5.0, "in_flight": -1})):
            shared.write_text(record)
            self.assertIsNone(dispatch.shared_walk(shared))

    def test_a_background_walk_error_ends_the_attempt_at_the_next_sample(self):
        storage = self.request(2)["storage"]
        storage["emergency"] = {"volume_bytes": 10**12, "margin_fraction": 0.1}
        dispatch.EMERGENCY_WALK.update(logical=1, unix=time.time(),
                                       at=time.monotonic() - dispatch.EMERGENCY_WALK_SECONDS - 1)
        walking = threading.Event()
        self.addCleanup(walking.set)

        def failing(storage):
            walking.wait(10)
            raise ValueError("accounting tree contains a link")
        with patch.object(dispatch, "accounting_bytes", side_effect=failing):
            self.assertEqual(dispatch.running_storage(storage)["logical_bytes"], 1)  # the walk runs aside
            walking.set()
            dispatch.EMERGENCY_WALK["thread"].join(10)
            with self.assertRaisesRegex(ValueError, "contains a link"):
                dispatch.running_storage(storage)

    def test_an_attempts_own_output_in_the_accounting_tree_counts_after_its_native_part(self):
        storage = self.request(2)["storage"]
        storage["emergency"] = {"volume_bytes": 10**12, "margin_fraction": 0.1}
        inside = self.root / "trial-x" / "native"
        inside.mkdir(parents=True)
        (inside / "episode.json").write_bytes(b"x" * 300)
        self.assertEqual(dispatch.own_output_bytes(storage, inside), 300)
        other = tempfile.TemporaryDirectory()  # block scratch: outside the accounting trees
        self.addCleanup(other.cleanup)
        (Path(other.name) / "episode.json").write_bytes(b"x" * 300)
        self.assertEqual(dispatch.own_output_bytes(storage, other.name), 0)
        self.assertEqual(dispatch.own_output_bytes({k: v for k, v in storage.items() if k != "emergency"}, inside), 0)

    def run_native(self, request, child_seconds, validate_seconds=0.0, child_wait=None, **patches):
        """execute() on a stand-in native child that runs ``child_seconds``; samples every 0.05 s. Returns
        execution.json, or None when no native child was spawned."""
        request_ref = self.save(request)

        class Child:
            pid, returncode = 4242, None

            def __init__(self):
                self.end = time.monotonic() + child_seconds

            def poll(self):
                if time.monotonic() >= self.end:
                    self.returncode = 0
                return self.returncode

            def wait(self, timeout=None):
                if child_wait is not None and timeout is not None and self.poll() is None:
                    return child_wait(timeout)
                left = self.end - time.monotonic()
                if timeout is not None and left > timeout:
                    time.sleep(timeout)
                    raise subprocess.TimeoutExpired("native", timeout)
                time.sleep(max(0.0, left))
                return self.poll()

            def kill(self):
                self.end = time.monotonic()

        def validate(request, qualification):
            time.sleep(validate_seconds)
            dispatch.validate_storage(request["storage"], admission=request.get("admission"))
            return self.config, self.runtime
        sample = {"rss_bytes": 0}
        with patch.dict(os.environ, {"MTG_HOST_RESERVATION_TOKEN": "test-token"}), \
                patch.object(dispatch, "validate_request", side_effect=validate), \
                patch.object(dispatch, "placed_config", side_effect=lambda request, config, qualification: config), \
                patch.object(dispatch, "set_affinity"), \
                patch.object(dispatch, "require_mapped_libraries"), \
                patch.object(dispatch, "process_sample", side_effect=lambda child: dict(sample)), \
                patch.object(dispatch, "MONITOR_POLL_SECONDS", 0.05), \
                patch.object(dispatch.shutil, "disk_usage",
                             return_value=type("Disk", (), {"free": 100*1024**3, "total": 200*1024**3})()), \
                patch("host_reservation_v1.status", return_value={"token_fate": "holds"}), \
                patch("host_reservation_v1.record_descendant"), \
                patch.object(dispatch.subprocess, "Popen", side_effect=lambda *a, **k: Child()) as spawn:
            for name, value in patches.items():
                stack = patch.object(dispatch, name, value)
                stack.start()
                self.addCleanup(stack.stop)
            with self.assertRaises(Exception):  # the stand-in writes no native result
                dispatch.execute(Path(request_ref["path"]), True)
            self.spawned = spawn.call_count
        path = Path(request["root"]) / "execution.json"
        return dispatch.read(path) if path.exists() else None

    def test_a_slow_walk_never_hides_the_native_end(self):
        """Regression for lease 20261101-1232 (#463): a sample's due walk (minutes on the network volume) ran
        inside the monitor loop, so execution.json waited for it after the native child had exited and the lease
        guard, seeing no native work, released the Pod. Now the walk runs aside: the end is recorded within one
        sample interval, and the attempt has left its native part, so the dispatcher's work after it is activity."""
        import nine_deck_campaign_v1 as campaign
        request = self.request(1)
        request["storage"]["emergency"] = {"volume_bytes": 10**12, "margin_fraction": 0.1}
        # The admission walk is old enough that the first sample's walk is due, and the walk is slow.
        request["admission"] = self.admission(logical=1000, age=dispatch.EMERGENCY_WALK_SECONDS + 60)
        release, walking = threading.Event(), threading.Event()
        self.addCleanup(release.set)

        def slow_walk(storage):
            walking.set()
            release.wait(30)
            return [], 2000
        with patch.object(dispatch, "accounting_bytes", side_effect=slow_walk):
            execution = self.run_native(request, child_seconds=0.3)
            self.assertTrue(walking.is_set())  # a sample's walk was due and started
            self.assertTrue(dispatch.EMERGENCY_WALK["thread"].is_alive())  # and still runs
        self.assertIsNone(execution["error"])
        self.assertLess(execution["child_seconds"], 0.3 + 2 * 0.05 + 0.5)
        self.assertIsNone(campaign.native_ticks(Path(request["root"])))  # outside the native part: activity counts
        release.set()
        dispatch.EMERGENCY_WALK["thread"].join(10)

    def test_an_interruption_record_tells_a_running_child_from_an_exited_one(self):
        """Positive per-attempt evidence of a stop from outside (AMENDMENT-3 draft): the signal, and whether the
        native child was still running or had exited, with its code."""
        running = unittest.mock.MagicMock()
        running.poll.return_value = None
        exited = unittest.mock.MagicMock()
        exited.poll.return_value = 3
        import signal as signals
        for name, child, expected in (("running", running, (True, True, None)), ("exited", exited, (True, False, 3)),
                                      ("before", None, (False, False, None))):
            root = self.root / name
            root.mkdir()
            dispatch.record_interruption(root, child, signals.SIGTERM)
            dispatch.record_interruption(root, exited, signals.SIGTERM)  # written once
            record = dispatch.read(root / "interruption.json")
            self.assertEqual((record["signal"], record["child_started"], record["child_running"],
                              record["child_exit_code"]), ("SIGTERM",) + expected)

    @unittest.skipUnless(LINUX, "delivers a POSIX signal to this process")
    def test_a_stop_from_outside_is_recorded_while_the_child_runs(self):
        import signal as signals
        request = self.request(1)
        before = signals.getsignal(signals.SIGTERM)
        original = subprocess.TimeoutExpired

        def stop_then_wait(timeout):
            os.kill(os.getpid(), signals.SIGTERM)  # the handler raises at once, inside the monitor loop
            time.sleep(1)
            raise original("native", timeout)
        execution = self.run_native(request, child_seconds=5.0, child_wait=stop_then_wait)
        record = dispatch.read(Path(request["root"]) / "interruption.json")
        self.assertEqual((record["signal"], record["child_running"]), ("SIGTERM", True))
        self.assertIn("stopped from outside by SIGTERM", execution["error"])
        self.assertEqual(signals.getsignal(signals.SIGTERM), before)  # restored

    def test_a_runpod_attempt_rechecks_its_lease_just_before_the_spawn(self):
        """Astra #464 round 1: a slow admission check must not start native work the lease no longer allows; the
        wall-time limit still counts from the dispatch's start."""
        request = self.request(1)
        request["placement"]["host"] = "runpod"
        checks = []

        def refuse(request):
            checks.append(1)
            raise ValueError("lease guard has stopped new work")
        self.assertIsNone(self.run_native(request, child_seconds=0.1, require_host=refuse))
        self.assertEqual((checks, self.spawned), ([1], 0))
        request = self.request(1)
        request["root"] += "-slow"
        request["cold_root"] += "-slow"
        request["wall_seconds"] = 0.5
        execution = self.run_native(request, child_seconds=0.3, validate_seconds=0.4)
        self.assertIn("wall-time limit", execution["error"])

    def test_evaluation_serial_payload_has_no_parallel_field(self):
        config = {"mode": "collect_parallel", "workers": 4, "source": {},
                  "output_directory": str(self.root/"evaluation"),
                  "episodes": [{"id": "a", "seed": 1}, {"id": "b", "seed": 2}]}
        request = self.request(1); request.update(kind="evaluation", episode_indices=[0, 1])
        actual = dispatch.placed_config(request, config, True)
        self.assertEqual(actual["mode"], "collect")
        self.assertNotIn("workers", actual)
        request["placement"]["workers"] = 2
        actual = dispatch.placed_config(request, config, True)
        self.assertEqual(actual["mode"], "collect_parallel")
        self.assertEqual(actual["workers"], 2)

    def test_evaluation_reuses_shape_but_binds_actual_model_and_schedule(self):
        initial = {"schema": "checkpoint/v1", "source_import": "fixed-import", "feature_contract_digest": "a",
                   "feature_encoding_digest": "b", "card_db_hash": "c",
                   "parameters": [{"name": "weight", "shape": [2], "values": [1, 2]}]}
        config = {"mode": "collect_parallel", "workers": 2, "source": {"play_import": "fixed",
                  "checkpoint": self.save(initial)}, "output_directory": str(self.root/"eval"),
                  "episodes": [{"id": "e0", "seed": 1, "deck": "a"}, {"id": "e1", "seed": 2, "deck": "b"}]}
        trained = copy.deepcopy(initial); trained["parameters"][0]["values"] = [11, 22]
        other = copy.deepcopy(config); other["source"]["checkpoint"] = self.save(trained)
        self.assertEqual(dispatch.workload(config, "evaluation"), dispatch.workload(other, "evaluation"))
        self.assertNotEqual(dispatch.workload(config, "evaluation", True), dispatch.workload(other, "evaluation", True))
        other["episodes"][0]["deck"] = "c"
        self.assertNotEqual(dispatch.workload(config, "evaluation"), dispatch.workload(other, "evaluation"))
        changed = copy.deepcopy(initial); changed["feature_contract_digest"] = "changed"
        other = copy.deepcopy(config); other["source"]["checkpoint"] = self.save(changed)
        self.assertNotEqual(dispatch.workload(config, "evaluation"), dispatch.workload(other, "evaluation"))

    def test_runtime_and_recovery_paths_do_not_change_runtime_identity(self):
        other = copy.deepcopy(self.runtime)
        other["binary"]["path"] = "C:/remote/engine.exe"
        self.assertEqual(dispatch.runtime_identity(self.runtime), dispatch.runtime_identity(other))
        remote = self.root/"remote"; local = self.root/"recovered"
        item = self.save({"same": "bytes"}, local/"nested/result.json")
        captured = {**item, "path": str(remote/"nested/result.json")}
        resolver = dispatch.artifact_reader([{"source_root": str(remote), "local_root": str(local)}])
        self.assertEqual(resolver(captured), local/"nested/result.json")
        (local/"nested/result.json").write_text("modified")
        with self.assertRaisesRegex(ValueError, "changed pinned"):
            resolver(captured)

    def checkpoint_source(self, values):
        return {"play_import": "fixed", "checkpoint": self.save({
            "schema": "checkpoint/v1", "source_import": "fixed-import", "feature_contract_digest": "a",
            "feature_encoding_digest": "b", "card_db_hash": "c",
            "parameters": [{"name": "weight", "shape": [2], "values": values}]})}

    def test_permuted_units_family_binds_episode_multiset_and_layout(self):
        block = copy.deepcopy(self.config)
        block["initial_source"] = self.checkpoint_source([1, 2])
        for index, update in enumerate(block["iterations"]):
            update["episodes"].append({"episode": {"id": f"second-{index}", "seed": 5000 + index, "learner_seat": 1,
                                       "registered": ["deck-b", f"deck-{index % 3}"]}, "opponent": "initial"})
        later = copy.deepcopy(block)
        later["initial_source"] = self.checkpoint_source([7, 9])
        later["iterations"].reverse()
        for update in later["iterations"]:
            update["episodes"].reverse()
        family = "permuted-units-v1"
        self.assertEqual(dispatch.workload(block, "training", family=family),
                         dispatch.workload(later, "training", family=family))
        self.assertNotEqual(dispatch.workload(block, "training"), dispatch.workload(later, "training"))
        moved = copy.deepcopy(block)
        moved["iterations"][0]["episodes"].append(moved["iterations"][1]["episodes"].pop())
        self.assertNotEqual(dispatch.workload(block, "training", family=family),
                            dispatch.workload(moved, "training", family=family))
        changed = copy.deepcopy(block)
        changed["iterations"][3]["episodes"][1]["opponent"] = "current"
        self.assertNotEqual(dispatch.workload(block, "training", family=family),
                            dispatch.workload(changed, "training", family=family))
        for key, value in (("learning_rate", .001), ("max_non_natural_episode_fraction", .1)):
            changed = copy.deepcopy(block); changed[key] = value
            self.assertNotEqual(dispatch.workload(block, "training", family=family),
                                dispatch.workload(changed, "training", family=family))
        with self.assertRaisesRegex(ValueError, "training only"):
            dispatch.workload({"mode": "collect", "source": {}, "episodes": [], "output_directory": "x"},
                              "evaluation", family=family)
        with self.assertRaisesRegex(ValueError, "unknown schedule family"):
            dispatch.workload(block, "training", family="other")

    def fresh_import(self, run, layout="layout-a"):
        """A fresh-initialization descriptor whose lineage fields differ by run and whose shape is ``layout``'s."""
        manifest = {"schema": "mtg-kernel-fresh-initialization/v1", "lineage_id": run,
                    "initializer": {"identity": "trainer-seeded-v1", "authority": "a", "seed_derivation": "d",
                                    "base_seed": len(run), "model_init_seed": 7 * len(run)},
                    "model": {"architecture": "net-8"}, "target": {"card_db_hash": "c"},
                    "payload": {"bytes": 8, "element_count": 2, "encoding": "f32", "layout": "rows",
                                "parameter_layout_sha256": layout, "tensor_count": 1, "sha256": run,
                                "model_parameter_sha256": run},
                    "parameters": [{"name": "w", "shape": [2], "sha256": run}]}
        return self.save({"schema": "mtg-kernel-fresh-initialization-source/v1",
                          "initialization": self.save(manifest, self.root / run / "initialization.json"),
                          "parameters": self.save({"bytes": run}, self.root / run / "parameters.json")},
                         self.root / run / "play-import.json")

    def test_own_history_family_covers_sibling_lineages_and_growing_history(self):
        import g_references_v1 as g
        decks = str(Path(__file__).resolve().parents[3] / "data" / "runtime_decks_v1.json")
        imports = {run: self.fresh_import(run) for run in ("G1", "G2", "G3")}

        def block(run, number, play=None):
            def fake(name):
                return {"path": f"C:/x/{name}.json", "sha256": format(abs(hash(name)) % 16**64, "064x")}
            return g.block_config({"run": run, "block": number, "runtime_decks": decks,
                                   "play_import": play or imports[run],
                                   "start_checkpoint": None if number == 1 else fake(f"{run}-{number - 1}"),
                                   "history": {str(end): fake(f"{run}-{end}") for end in g.history_pool(number)},
                                   "update_backend": {"kind": "cpu"}, "collection_workers": 8,
                                   "preparation_workers": 8, "output_directory": "C:/x/out"})
        family = "permuted-units-own-history-v1"
        key = lambda config: dispatch.workload(config, "training", family=family)
        # Blocks 1 and 2 (empty pool) are one family across lineages; blocks 3..741 (own history) another.
        self.assertEqual(len({key(block("G1", 1)), key(block("G2", 1)), key(block("G3", 2))}), 1)
        self.assertEqual(len({key(block("G1", 5)), key(block("G2", 17)), key(block("G3", 741)), key(block("G1", 3))}), 1)
        self.assertNotEqual(key(block("G1", 1)), key(block("G1", 5)))
        later = block("G1", 5)
        self.assertNotEqual(dispatch.workload(block("G2", 5), "training"), dispatch.workload(later, "training"))
        # The share of history episodes stays bound: one more or one fewer history unit is another family.
        for old, new in (("current", {"kind": "fixed", "id": later["opponents"][0]["id"]}), ("fixed", {"kind": "current"}),
                         ("fixed", {"kind": "initial"})):
            changed = copy.deepcopy(later)
            item = next(item for update in changed["iterations"] for item in update["episodes"]
                        if item["opponent"]["kind"] == old)
            item["opponent"] = new
            self.assertNotEqual(key(changed), key(later))
        for field, value in (("learning_rate", .001), ("max_non_natural_episode_fraction", .1)):
            changed = copy.deepcopy(later); changed[field] = value
            self.assertNotEqual(key(changed), key(later))
        # Another recipe's import shape is another family.
        self.assertNotEqual(key(block("G9", 5, self.fresh_import("G9", layout="layout-b")) if False else
                                block("G1", 5, self.fresh_import("G1b", layout="layout-b"))), key(later))
        stray = copy.deepcopy(later)
        stray["opponents"][0]["source"]["play_import"] = imports["G2"]
        with self.assertRaisesRegex(ValueError, "checkpoints of the start's import"):
            key(stray)
        foreign = copy.deepcopy(later)
        foreign["opponents"][0]["source"]["feature_transfer"] = {"expected_feature_contract_digest": "x"}
        with self.assertRaisesRegex(ValueError, "checkpoints of the start's import"):
            key(foreign)
        unnamed = copy.deepcopy(later)
        item = next(item for update in unnamed["iterations"] for item in update["episodes"]
                    if item["opponent"]["kind"] == "fixed")
        item["opponent"]["id"] = "G1-history-b999"
        with self.assertRaisesRegex(ValueError, "names no opponent"):
            key(unnamed)
        changed_bytes = copy.deepcopy(later)
        changed_bytes["initial_source"]["play_import"] = dict(imports["G1"], sha256="0" * 64)
        changed_bytes["opponents"] = [dict(o, source=dict(o["source"], play_import=changed_bytes["initial_source"]["play_import"]))
                                      for o in changed_bytes["opponents"]]
        with self.assertRaisesRegex(ValueError, "changed pinned input"):
            key(changed_bytes)
        moved = copy.deepcopy(later)
        moved["iterations"][0]["episodes"].append(moved["iterations"][1]["episodes"].pop())
        self.assertNotEqual(key(moved), key(later))

    def test_non_natural_tolerance_must_be_declared(self):
        self.config["max_non_natural_episode_fraction"] = 0.2
        request = self.request(1)
        # The request names the desktop: run its host checks on any test host.
        with patch.object(dispatch, "validate_storage"), patch.object(dispatch, "WINDOWS", True), \
                patch.object(dispatch.platform, "node", return_value=dispatch.HOSTS["desktop"]):
            with self.assertRaisesRegex(ValueError, "declared"):
                dispatch.validate_request(request, True)
            request["non_natural_tolerance"] = 0.1
            with self.assertRaisesRegex(ValueError, "declared"):
                dispatch.validate_request(request, True)
            request["non_natural_tolerance"] = 0.2
            dispatch.validate_request(request, True)
            request["schedule_family"] = "unknown"
            with self.assertRaisesRegex(ValueError, "schedule family"):
                dispatch.validate_request(request, True)

    def test_tolerant_ledger_enters_fingerprint_and_is_refused_otherwise(self):
        trajectory = self.save({"terminal": {"terminal_classification": "natural"}})
        ledger = self.save({"schema": "mtg-kernel-non-natural-collection-ledger/v1",
                            "entries": [{"slot": 0, "attempt": 1}]})
        collection = self.save({"complete": True, "trajectories": [trajectory], "non_natural_ledger": ledger})
        with self.assertRaisesRegex(ValueError, "non-natural collection attempt"):
            dispatch.collection_fingerprint(collection, 1)
        ledgers = []
        self.assertEqual(dispatch.collection_fingerprint(collection, 1, ledgers=ledgers), [trajectory["sha256"]])
        self.assertEqual(ledgers, [ledger["sha256"]])
        bad = self.save({"complete": True, "trajectories": [trajectory], "non_natural_ledger": self.save(
            {"schema": "mtg-kernel-non-natural-collection-ledger/v1", "entries": [{"slot": 3, "attempt": 1}]})})
        with self.assertRaisesRegex(ValueError, "invalid non-natural ledger"):
            dispatch.collection_fingerprint(bad, 1, ledgers=[])

    def test_storage_projection_preserves_future_volume_reserve(self):
        storage = self.request(2)["storage"]
        free = dispatch.DISK_RESERVE_BYTES + 999
        with patch.object(dispatch.shutil, "disk_usage", return_value=type("Disk", (), {"free": free})()):
            with self.assertRaisesRegex(ValueError, "reserve unavailable"):
                dispatch.validate_storage(storage)
            # Periodic actual-use checks do not charge the original projection twice.
            dispatch.validate_storage(storage, extra=0)

    # ---------------------------------------------------------------- Linux and runpod

    def linux_runtime(self, libraries=None):
        libraries = libraries or {name: {"path": "/usr/lib/x86_64-linux-gnu/" + name, "sha256": str(i) * 64}
                                  for i, name in enumerate(dispatch.LIBRARIES)}
        return {**copy.deepcopy(self.runtime), "platform": dispatch.PLATFORM, "libraries": libraries}

    def test_libraries_are_the_runtime_observation_set(self):
        dispatch.lease_guard()  # puts phase1_cloud on the import path
        import runtime_observation
        self.assertEqual(dispatch.LIBRARIES, runtime_observation.LIBRARIES)

    def test_linux_runtime_identity_pins_libraries_and_never_matches_windows(self):
        windows, linux = self.runtime, self.linux_runtime()
        self.assertEqual(dispatch.runtime_identity(windows), {"engine_commit": "a" * 40,
                         "tracked_tree_sha256": "b" * 64, "binary_sha256": windows["binary"]["sha256"]})
        identity = dispatch.runtime_identity(linux)
        self.assertEqual(identity["platform"], "linux-x86_64")
        self.assertEqual(identity["library_sha256"]["libm.so.6"], "2" * 64)
        self.assertNotEqual(identity, dispatch.runtime_identity(windows))
        moved = copy.deepcopy(linux); moved["libraries"]["libm.so.6"]["path"] = "/lib64/libm.so.6"
        self.assertEqual(dispatch.runtime_identity(moved), identity)
        changed = copy.deepcopy(linux); changed["libraries"]["libm.so.6"]["sha256"] = "f" * 64
        self.assertNotEqual(dispatch.runtime_identity(changed), identity)
        partial = copy.deepcopy(linux); del partial["libraries"]["libgcc_s.so.1"]
        with self.assertRaisesRegex(ValueError, "libgcc_s pins"):
            dispatch.runtime_for({"runtime": self.save(partial)})

    def test_linux_and_windows_qualifications_are_never_compatible(self):
        windows = self.runtime
        for trials, launch in ((self.linux_runtime(), windows), (windows, self.linux_runtime())):
            self.runtime = trials
            choice, request = self.choice(), self.request(2)
            with patch.object(dispatch, "validate_request", return_value=(self.config, launch)), \
                    self.assertRaisesRegex(ValueError, "qualification runtime differs"):
                dispatch.require_choice(self.save(choice), request, verify_outputs=True)
            with patch.object(dispatch, "validate_request", return_value=(self.config, trials)):
                dispatch.require_choice(self.save(choice), request, verify_outputs=True)

    def test_runpod_is_an_admissible_inventory_placement(self):
        choice = self.choice()
        for host in ("desktop", "runpod"):
            choice["inventory"][host]["eligible"] = host == "runpod"
        for entry in choice["qualifications"]:
            report = dispatch.read(entry["path"]); original = dispatch.read(report["request"]["path"])
            original["placement"]["host"] = report["placement"]["host"] = "runpod"
            report["request"] = self.save(original); entry.update(self.save(report))
        choice["selected"][dispatch.workload(self.config, "training")] = choice["qualifications"][1]
        request = self.request(2); request["placement"]["host"] = "runpod"
        with patch.object(dispatch, "validate_request", return_value=(self.config, self.runtime)):
            self.assertEqual(dispatch.require_choice(self.save(choice), request, verify_outputs=True)["qualification"],
                             choice["qualifications"][1])

    @unittest.skipUnless(LINUX, "Linux runtime checks")
    def test_linux_runtime_runs_only_on_linux_with_verified_pins_and_clean_loader(self):
        files = {name: self.save({"library": name}, self.root / "lib" / name) for name in dispatch.LIBRARIES}
        linux = self.linux_runtime(files)
        with patch.dict(os.environ, {}, clear=False):
            for name in [n for n in os.environ if n.startswith("LD_") or n == "GLIBC_TUNABLES"]:
                os.environ.pop(name)
            dispatch.host_runtime(linux)
            with self.assertRaisesRegex(ValueError, "platform differs"):
                dispatch.host_runtime(self.runtime)
            with patch.object(dispatch, "WINDOWS", True), self.assertRaisesRegex(ValueError, "platform differs"):
                dispatch.host_runtime(linux)
            with patch.object(dispatch.platform, "machine", return_value="aarch64"), \
                    self.assertRaisesRegex(ValueError, "platform differs"):
                dispatch.host_runtime(linux)
            with patch.dict(os.environ, {"LD_PRELOAD": "/tmp/interposer.so"}), \
                    self.assertRaisesRegex(ValueError, "loader environment"):
                dispatch.host_runtime(linux)
            (self.root / "lib" / "libm.so.6").write_text("other libm")
            with self.assertRaisesRegex(ValueError, "changed pinned input"):
                dispatch.host_runtime(linux)

    @unittest.skipUnless(LINUX, "symlinked library layout and /proc maps")
    def test_mapped_libraries_must_resolve_to_their_pins(self):
        real, link = self.root / "usr" / "lib", self.root / "lib"
        real.mkdir(parents=True); link.mkdir()
        libraries = {}
        for name in dispatch.LIBRARIES:
            target = real / ("libm-2.31.so" if name == "libm.so.6" else name)  # also an older soname layout
            target.write_text(name)
            (link / name).symlink_to(target)
            libraries[name] = {"path": str(link / name), "sha256": "0" * 64}
        runtime = self.linux_runtime(libraries)
        maps = self.root / "proc" / "4242" / "maps"; maps.parent.mkdir(parents=True)
        child = type("Child", (), {"pid": 4242, "poll": lambda self: None})()
        def mapping(paths):
            maps.write_text("".join(f"7f0000{i:04x}000-7f0000{i:04x}fff r-xp 00000000 08:01 {i} {path}\n"
                                    for i, path in enumerate(paths)) + "7ffd0000-7ffd1000 r-xp 0 0 0 [vdso]\n")
        resolved = [str(real / ("libm-2.31.so" if n == "libm.so.6" else n)) for n in dispatch.LIBRARIES]
        with patch.object(dispatch, "PROC", self.root / "proc"):
            mapping(resolved + [str(self.root / "binary")])
            self.assertEqual(set(dispatch.require_mapped_libraries(child, runtime)), set(dispatch.LIBRARIES))
            (self.root / "elsewhere").mkdir(); (self.root / "elsewhere" / "libm.so.6").write_text("other")
            mapping(resolved + [str(self.root / "elsewhere" / "libm.so.6")])
            with self.assertRaisesRegex(ValueError, "differs from its pin"):
                dispatch.require_mapped_libraries(child, runtime)
            mapping([path + " (deleted)" if path.endswith("libc.so.6") else path for path in resolved])
            with self.assertRaisesRegex(ValueError, "differs from its pin"):
                dispatch.require_mapped_libraries(child, runtime)
            mapping([path for path in resolved if not path.endswith("libgcc_s.so.1")])
            with self.assertRaisesRegex(ValueError, "not mapped"):
                dispatch.require_mapped_libraries(child, runtime, timeout=0.05)
            exited = type("Child", (), {"pid": 4242, "poll": lambda self: 0})()
            with self.assertRaisesRegex(ValueError, "not mapped"):
                dispatch.require_mapped_libraries(exited, runtime)

    @unittest.skipUnless(LINUX, "Linux process telemetry and affinity")
    def test_linux_telemetry_keeps_windows_fields_and_affinity_binds(self):
        child = subprocess.Popen([sys.executable, "-c", "import time; x = bytearray(1 << 24); time.sleep(30)"])
        self.addCleanup(child.wait)
        self.addCleanup(child.kill)
        time.sleep(0.3)
        sample = dispatch.process_sample(child)
        self.assertEqual(set(sample), {"at_unix", "pid", "rss_bytes", "cpu_seconds"})
        self.assertEqual(sample["pid"], child.pid)
        self.assertGreater(sample["rss_bytes"], 1 << 24)
        self.assertGreaterEqual(sample["cpu_seconds"], 0)
        with patch.object(dispatch.os, "sched_setaffinity") as bind, \
                patch.object(dispatch.os, "sched_getaffinity", return_value={0, 1}):
            dispatch.set_affinity([0, 1])
            bind.assert_called_once_with(0, [0, 1])
            with self.assertRaisesRegex(ValueError, "cannot bind"):
                dispatch.set_affinity([0, 1, 2])

    def test_placement_host_rules(self):
        request = self.request(1)
        request["placement"]["host"] = "aws"
        with self.assertRaisesRegex(ValueError, "no paid or unknown placement"):
            dispatch.require_host(request)
        for host in ("desktop", "computehost"):
            request["placement"]["host"] = host
            with patch.object(dispatch, "WINDOWS", False), patch.dict(os.environ, {"RUNPOD_POD_ID": "testpod"}), \
                    patch.object(dispatch.platform, "node", return_value=dispatch.HOSTS[host]), \
                    self.assertRaisesRegex(ValueError, "wrong target host"):
                dispatch.require_host(request)
        request["placement"]["host"] = "desktop"
        with patch.object(dispatch, "WINDOWS", True), \
                patch.object(dispatch.platform, "node", return_value=dispatch.HOSTS["desktop"]):
            dispatch.require_host(request)
            request["placement"]["host"] = "computehost"
            with self.assertRaisesRegex(ValueError, "wrong target host"):
                dispatch.require_host(request)
        request["placement"]["host"] = "runpod"
        with patch.object(dispatch, "WINDOWS", True), self.assertRaisesRegex(ValueError, "lease guard"):
            dispatch.require_host(request)
        with patch.object(dispatch, "runpod_lease") as lease:
            for extra, expected in (({}, 1000), ({"lease_work_seconds": 300}, 300),
                                    ({"lease_work_seconds": 5000}, 1000)):
                dispatch.require_host({**request, **extra})
                lease.assert_called_with(expected)

    @unittest.skipUnless(LINUX, "runpod placements exist only on Linux")
    def test_runpod_needs_this_pods_fresh_unlatched_guard_and_lease_time(self):
        folder, now = self.root / "guard", time.time()
        lease = lease_guard_files(folder, now=now)
        available = lease["deadline_epoch"] - lease["recovery_seconds"] - now
        env = {"RUNPOD_POD_ID": "testpod", dispatch.LEASE_GUARD_ENV: str(folder)}
        with patch.dict(os.environ, env):
            result = dispatch.runpod_lease(3600, now=now)
            self.assertEqual((result["pod_id"], result["available_seconds"]), ("testpod", available))
            with self.assertRaisesRegex(ValueError, "lease time exhausted"):
                dispatch.runpod_lease(available + 1, now=now)
            with self.assertRaisesRegex(ValueError, "lease guard is stale"):
                dispatch.runpod_lease(60, now=now + 3 * lease["poll_seconds"] + 31)
            with patch.object(dispatch, "WINDOWS", True), self.assertRaisesRegex(ValueError, "lease guard"):
                dispatch.runpod_lease(60, now=now)
            (folder / "stop-request.json").write_text("{}")
            with self.assertRaisesRegex(ValueError, "stopped new work"):
                dispatch.runpod_lease(60, now=now)
            (folder / "stop-request.json").unlink()
            for guard in ({"latched": "native_work_stalled_or_idle"}, {"release_epoch": now + 600},
                          {"provider_ok": False}, {"allow_new_dispatch": False}):
                lease_guard_files(folder, now=now, **guard)
                with self.assertRaisesRegex(ValueError, "stopped new work"):
                    dispatch.runpod_lease(60, now=now)
            lease_guard_files(folder, pod="otherpod", now=now)
            with self.assertRaisesRegex(ValueError, "another pod"):
                dispatch.runpod_lease(60, now=now)
            lease_guard_files(folder, now=now)
            lease["deadline_epoch"] = lease["created_epoch"] + 9 * 3600
            (folder / "lease.json").write_text(json.dumps(lease))
            with self.assertRaisesRegex(ValueError, "eight hours"):
                dispatch.runpod_lease(60, now=now)
        with patch.dict(os.environ, {dispatch.LEASE_GUARD_ENV: str(folder)}):
            os.environ.pop("RUNPOD_POD_ID", None)
            with self.assertRaisesRegex(ValueError, "lease guard"):
                dispatch.runpod_lease(60, now=now)

    def test_busy_pattern_is_unchanged_on_windows_and_names_the_linux_binary(self):
        with patch.object(dispatch, "WINDOWS", True):
            self.assertEqual(dispatch.busy_pattern("C:/bin/trainer.exe"),
                             r"native_expanded_training|expanded_deck_training|cargo|rustc|trainer\.exe")
        with patch.object(dispatch, "WINDOWS", False):
            pattern = re.compile(dispatch.busy_pattern("/opt/bin/nine.deck-trainer"), re.IGNORECASE)
        for name in ("nine.deck-trainer", "trainer", "trainer.exe", "native_expanded_training_run_v1", "cargo"):
            self.assertTrue(pattern.search(name), name)
        for name in ("python3.13", "trainer2", "nineXdeck-trainer"):
            self.assertFalse(pattern.search(name), name)


# Two SMT P-cores (CPUs 0-1 and 2-3) and two E-cores (CPUs 4 and 5).
class StartBarrierTests(unittest.TestCase):
    """A packed qualification trial waits after its checks for the pack tool's release."""

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.request_path = self.root / "request.json"
        self.request_path.write_text("{}")

    def barrier(self, name, **fields):
        return {"ready": str(self.root / f"{name}.ready.json"), "release": str(self.root / "release.json"),
                "timeout_seconds": 30, "owner": owner(), **fields}

    def test_only_training_qualification_trials_carry_a_barrier(self):
        request = {"kind": "training", "start_barrier": self.barrier("a")}
        self.assertEqual(dispatch.start_barrier(request, True), request["start_barrier"])
        self.assertIsNone(dispatch.start_barrier({"kind": "training"}, False))
        unowned = self.barrier("a")
        del unowned["owner"]
        cases = [(request, False), (dict(request, kind="evaluation"), True),
                 (dict(request, start_barrier=self.barrier("a", ready="a.json")), True),
                 (dict(request, start_barrier=self.barrier("a", extra=1)), True),
                 (dict(request, start_barrier=unowned), True)]
        for bad in ({"pid": 0, "creation_time": 1}, {"pid": 1, "creation_time": "1"},
                    {"pid": 1, "creation_time": 1, "extra": 1}, {"pid": 1}, [1, 1]):
            cases.append((dict(request, start_barrier=self.barrier("a", owner=bad)), True))
        for value, qualification in cases:
            with self.assertRaisesRegex(ValueError, "start barrier belongs"):
                dispatch.start_barrier(value, qualification)
        with self.assertRaisesRegex(ValueError, "start barrier timeout"):
            dispatch.start_barrier(dict(request, start_barrier=self.barrier("a", timeout_seconds=0)), True)

    def test_a_trial_waits_for_go_and_fails_without_it(self):
        release = self.root / "release.json"
        timer = threading.Timer(0.3, lambda: release.write_text('{"go": true}'))
        timer.start()
        self.addCleanup(timer.cancel)
        self.assertGreaterEqual(dispatch.await_release(self.barrier("a"), self.request_path, poll=0.01), 0.2)
        self.assertEqual(dispatch.read(self.root / "a.ready.json")["request"], dispatch.pin(self.request_path))
        release.write_text('{"go": false}')
        with self.assertRaisesRegex(ValueError, "without go"):
            dispatch.await_release(self.barrier("b"), self.request_path, poll=0.01)
        release.unlink()
        with self.assertRaisesRegex(ValueError, "timed out"):
            dispatch.await_release(self.barrier("c", timeout_seconds=0.05), self.request_path, poll=0.01)

    def test_a_trial_stops_waiting_once_its_pack_tool_has_ended(self):
        ended = subprocess.Popen([sys.executable, "-c", "pass"])
        creation = reservations.creation_time(ended.pid)  # readable until it is reaped
        ended.wait()
        gone = {"pid": ended.pid, "creation_time": creation}
        self.assertEqual(reservations.process_state(gone["pid"], gone["creation_time"]), "absent")
        started = time.monotonic()
        with self.assertRaisesRegex(ValueError, "pack tool ended without releasing it"):
            dispatch.await_release(self.barrier("a", owner=gone, timeout_seconds=600), self.request_path,
                                   poll=0.01, owner_poll=0.1)
        self.assertLess(time.monotonic() - started, 30)  # long before the backstop
        # A live owner keeps the trial waiting, here until the backstop.
        with self.assertRaisesRegex(ValueError, "timed out"):
            dispatch.await_release(self.barrier("b", timeout_seconds=0.3), self.request_path,
                                   poll=0.01, owner_poll=0.05)
        # A release its owner wrote before it ended still counts.
        (self.root / "release.json").write_text('{"go": true}')
        self.assertGreaterEqual(dispatch.await_release(self.barrier("c", owner=gone), self.request_path,
                                                       poll=0.01, owner_poll=0.0), 0)

    def test_a_release_written_as_its_pack_tool_ends_still_counts(self):
        # The tool renames the release into place and ends between the trial's look
        # at the release and its look at the tool.
        release = self.root / "release.json"

        def released_then_gone(*_):
            release.write_text('{"go": true}')
            return "absent"

        with patch.object(reservations, "process_state", side_effect=released_then_gone) as state:
            self.assertGreaterEqual(dispatch.await_release(self.barrier("a"), self.request_path,
                                                           poll=0.01, owner_poll=0.0), 0)
        state.assert_called_once()


TOPOLOGY = [{"cpus": "0-1", "efficiency_class": 1, "smt": True}, {"cpus": "2-3", "efficiency_class": 1, "smt": True},
            {"cpus": "4", "efficiency_class": 0, "smt": False}, {"cpus": "5", "efficiency_class": 0, "smt": False}]


class PackChoiceTests(AdmissionTestCase):
    """Side-by-side runs: a pack is chosen only on byte-equal trials that started
    together, when its waves beat the fastest solo allocation for the study."""

    def setUp(self):
        super().setUp()
        # Members run on one-worker slots, so their production config keeps the serial defaults.
        del self.config["collection_workers"], self.config["preparation_workers"]

    def slot(self, cpus):
        return {"host": "desktop", "cpu_affinity": cpus, "workers": 1, "preparation_workers": 1,
                "memory_bytes": 1024**3}

    def pack_trial(self, name, slot, seconds, start, child_seconds, tensors=(1, 2), barrier=False):
        request = self.request(slot["workers"])
        request.update(placement=dict(slot), root=str(self.root / name), cold_root=str(self.root / f"cold-{name}"))
        if barrier:
            request["start_barrier"] = {"ready": str(self.root / f"{name}.ready.json"),
                                        "release": str(self.root / "release.json"), "timeout_seconds": 600,
                                        "owner": owner()}
        original = dispatch.read(dispatch.checked(request["config"]))
        config = dispatch.placed_config(request, original, True)
        native = Path(config["output_directory"])
        self.save({"git_commit": self.runtime["engine_commit"],
                   "tracked_tree_sha256": self.runtime["tracked_tree_sha256"]}, native/"run.json")
        trajectory = self.save({"terminal": {"terminal_classification": "natural"}, "tensors": list(tensors)},
                               native/"episode.json")
        collection = self.save({"complete": True, "trajectories": [trajectory]}, native/"collection.json")
        checkpoint = self.save({"trajectories": [trajectory], "parameters": [123], "first_moments": [456],
                                "second_moments": [789], "adam_step": 32401}, native/"checkpoint.json")
        update = self.save({"complete": True, "checkpoint": checkpoint})
        receipt = self.save({"iteration": 0, "collection": collection, "update": update})
        result = {"complete": False, "completed_iterations": 1, "planned_iterations": 64, "iterations": [receipt]}
        request_pin = self.save(request)
        report = {"schema": dispatch.SCHEMA, "qualification": True, "complete": True, "request": request_pin,
                  "executed_config": self.save(config), "native_result": self.save(result),
                  "runtime": self.runtime, "launcher_sha256": dispatch.pin(dispatch.__file__)["sha256"],
                  "workload": dispatch.workload(original, "training"), "placement": dict(slot),
                  "fingerprint": dispatch.output_fingerprint(config, "training", result, self.runtime, True),
                  "completed_games": 1, "completed_updates": 1, "seconds": seconds,
                  "execution": self.save({"exit_code": 0, "error": None, "child_seconds": child_seconds,
                                          "seconds": seconds - 1, "started_unix": start}),
                  "run": dispatch.pin(native/"run.json"),
                  "archive": self.save({"mismatches": 0, "native_root": str(native),
                      "scheme": "two-deflate1-shards-full-readback/v1",
                      "shards": [{"archive": self.save({"archive": i})} for i in range(2)]})}
        return {"report": self.save(report),
                "started": self.save({"pid": 1, "started_unix": start, "placement": dict(slot),
                                      "request": request_pin})}

    def pack(self, packed_seconds=12, packed_start=1000, solo_overlap=False, changed=None, host="desktop",
             topology=TOPOLOGY):
        """Slots: two P-threads (CPUs 0 and 2) and two E-cores (4 and 5); one solo trial per kind.
        The packed trials start 0.1 s apart (a 0.3 s spread against 9 s native runs)."""
        slots = [dict(self.slot(cpus), host=host) for cpus in ([0], [2], [4], [5])]
        solo = [dict(slot=0, **self.pack_trial(f"solo-0-{host}", slots[0], 10, 0, 8)),
                dict(slot=2, **self.pack_trial(f"solo-2-{host}", slots[2], 11, 1005 if solo_overlap else 100, 8))]
        packed = [dict(slot=i, **self.pack_trial(f"packed-{i}-{host}", slot, packed_seconds, packed_start + i / 10, 9,
                                                 (1, 3) if changed == i else (1, 2)))
                  for i, slot in enumerate(slots)]
        return {"schema": dispatch.PACK, "workload": dispatch.workload(self.config, "training"),
                "slots": slots, "kinds": dispatch.slot_kinds(slots, topology), "topology": topology,
                "load": {point: self.load() for point in ("start", "packed", "end")},
                "solo": solo, "packed": packed}

    def load(self, cores=(0, 1, 2, 3, 4, 5), claims=(), token="a" * 64):
        """One recorded point: the qualifying reservation's declaration and the host's claims."""
        return {"at_unix": 1.0, "token_sha256": token, "cores": None if cores is None else list(cores),
                "claims": [{"id": "-".join(map(str, cpus)), "cores": list(cpus), "state": state, "lane": "other"}
                           for cpus, state in claims]}

    def study(self, runs, pack, select_pack=True):
        choice = self.choice()
        choice["inventory"]["desktop"]["cpu_affinity"] = [0, 1, 2, 3, 4, 5]
        choice["packs"] = [self.save(pack)]
        choice["study_runs"] = {dispatch.workload(self.config, "training"): runs}
        if select_pack:
            choice["selected"][dispatch.workload(self.config, "training")] = choice["packs"][0]
        return choice

    def admit(self, choice, placement, verify=True):
        request = self.request(1)
        request["placement"] = placement
        with patch.object(dispatch, "validate_request", return_value=(self.config, self.runtime)), \
                patch.object(dispatch, "host_topology", return_value=TOPOLOGY):
            return dispatch.require_choice(self.save(choice), request, verify_outputs=verify)

    def test_slot_kinds_name_core_type_and_workers(self):
        slots = [self.slot([0]), self.slot([0, 1]), self.slot([0, 2]), self.slot([4]),
                 dict(self.slot([0, 2]), workers=2)]
        self.assertEqual(dispatch.slot_kinds(slots, TOPOLOGY),
                         ["class1-cores1-cpus1-w1-p1", "class1-cores1-cpus2-w1-p1", "class1-cores2-cpus2-w1-p1",
                          "class0-cores1-cpus1-w1-p1", "class1-cores2-cpus2-w2-p1"])
        with self.assertRaisesRegex(ValueError, "spans core types"):
            dispatch.slot_kinds([self.slot([0, 4])], TOPOLOGY)
        with self.assertRaisesRegex(ValueError, "outside this host"):
            dispatch.slot_kinds([self.slot([9])], TOPOLOGY)

    def test_study_picks_a_faster_pack_and_admits_any_of_its_slots(self):
        # Solo: 6 s x 64 updates = 384 s a run, 8 runs = 3,072 s. Pack: 4 slots,
        # 12 s x 64 = 768 s a wave, two waves = 1,536 s.
        choice = self.study(8, self.pack())
        selected = self.admit(choice, self.slot([4]))
        self.assertEqual(selected["pack"], choice["packs"][0])
        self.assertEqual((selected["projected_seconds"], selected["slots"], selected["study_runs"]), (1536, 4, 8))
        self.assertEqual(selected["slot_seconds"], [768, 768, 768, 768])  # 12 s x 64 updates, each slot
        self.assertEqual(selected["slowdown"], {"class0-cores1-cpus1-w1-p1": 12 / 11,
                                                "class1-cores1-cpus1-w1-p1": 12 / 10})
        self.assertEqual(selected["background_claims"], 0)
        for slot in (self.slot([0]), self.slot([2]), self.slot([5])):
            self.assertEqual(self.admit(choice, slot), selected)
            self.assertEqual(dispatch.placed_config(dict(self.request(1), placement=slot), self.config, False),
                             self.config)  # what execute() runs on that slot
        with self.assertRaisesRegex(ValueError, "fastest"):
            self.admit(choice, self.slot([1]))
        request = self.request(1)
        request["placement"] = self.slot([2])
        choice_ref = self.save(choice)
        request["choice_verification"] = self.save({"schema": dispatch.CHOICE, "choice": choice_ref,
                                                   "selected": selected, "outputs_verified": True})
        with patch.object(dispatch, "validate_request", return_value=(self.config, self.runtime)), \
                patch.object(dispatch, "host_topology", return_value=TOPOLOGY):
            self.assertEqual(dispatch.require_choice(choice_ref, request), selected)

    def test_study_keeps_solo_when_the_pack_is_slower(self):
        # One run: solo 384 s against one 768 s wave.
        with self.assertRaisesRegex(ValueError, "fastest"):
            self.admit(self.study(1, self.pack()), self.slot([0]))
        choice = self.study(1, self.pack(), select_pack=False)
        selected = self.admit(choice, self.request(2)["placement"])
        self.assertEqual((selected["qualification"], selected["projected_seconds"], selected["study_runs"]),
                         (choice["qualifications"][1], 384, 1))

    def test_transport_counts_once_a_run_or_wave_and_the_queue_once(self):
        """Two runs on a host with 100 s transport and a 50 s queue: solo 2 x (384 + 100) + 50
        = 1,018 s, the pack one wave of 768 + 100 + 50 = 918 s. Without them the two tie at
        768 s, and the solo allocation, listed first, stays the fastest."""
        def priced(select_pack, transport=100, queue=50):
            choice = self.study(2, self.pack(), select_pack=select_pack)
            choice["inventory"]["desktop"].update(transport_seconds=transport, queue_seconds=queue,
                                                  queue_evidence=self.save({"queue": "measured"}))
            return choice
        selected = self.admit(priced(True), self.slot([0]))
        self.assertEqual((selected["projected_seconds"], selected["study_runs"]), (918, 2))
        with self.assertRaisesRegex(ValueError, "fastest"):
            self.admit(priced(False), self.request(2)["placement"])
        choice = priced(False, 0, 0)
        solo = self.admit(choice, self.request(2)["placement"])
        self.assertEqual((solo["qualification"], solo["projected_seconds"]), (choice["qualifications"][1], 768))
        with self.assertRaisesRegex(ValueError, "fastest"):
            self.admit(priced(True, 0, 0), self.slot([0]))

    def test_pack_refuses_changed_outputs_and_unproven_timing(self):
        with self.assertRaisesRegex(ValueError, "changed the sample or the saved learning outputs"):
            self.admit(self.study(8, self.pack(changed=3)), self.slot([0]))
        for start in (5000, 1000.91):  # far apart, or just past a tenth of the 9 s runs
            with self.assertRaisesRegex(ValueError, "did not start together"):
                pack = self.pack()
                pack["packed"][3] = dict(slot=3, **self.pack_trial(f"late-{start}", pack["slots"][3], 12, start, 9))
                self.admit(self.study(8, pack), self.slot([0]))
        with self.assertRaisesRegex(ValueError, "start time differs from its receipt"):
            pack = self.pack()  # a start record moved to fit, against the start its receipt binds
            moved = dispatch.read(pack["packed"][3]["started"]["path"])
            moved["started_unix"] -= 0.05
            pack["packed"][3]["started"] = self.save(moved)
            self.admit(self.study(8, pack), self.slot([0]))
        with self.assertRaisesRegex(ValueError, "solo trial overlapped"):
            self.admit(self.study(8, self.pack(solo_overlap=True)), self.slot([0]))
        with self.assertRaisesRegex(ValueError, "start record"):
            pack = self.pack()
            pack["packed"][1]["started"] = pack["packed"][0]["started"]
            self.admit(self.study(8, pack), self.slot([0]))

    def test_pack_needs_its_slots_kinds_and_trials(self):
        pack = self.pack()
        pack["solo"] = pack["solo"][:1]
        with self.assertRaisesRegex(ValueError, "one solo trial per slot kind"):
            self.admit(self.study(8, pack), self.slot([0]))
        pack = self.pack()
        pack["packed"] = pack["packed"][:3]
        with self.assertRaisesRegex(ValueError, "one packed trial per slot"):
            self.admit(self.study(8, pack), self.slot([0]))
        pack = self.pack()
        pack["slots"][1] = self.slot([0])
        with self.assertRaisesRegex(ValueError, "overlap"):
            self.admit(self.study(8, pack), self.slot([0]))
        pack = self.pack()
        pack["packed"][1], pack["packed"][2] = dict(pack["packed"][2], slot=1), dict(pack["packed"][1], slot=2)
        with self.assertRaisesRegex(ValueError, "placement differs from its slot"):
            self.admit(self.study(8, pack), self.slot([0]))
        choice = self.study(8, self.pack())
        choice["inventory"]["desktop"]["cpu_affinity"] = [0, 1, 2, 3]
        with self.assertRaisesRegex(ValueError, "eligible CPU set"):
            self.admit(choice, self.slot([0]))
        choice = self.study(8, self.pack())
        choice["study_runs"][dispatch.workload(self.config, "training")] = 0
        with self.assertRaisesRegex(ValueError, "study size"):
            self.admit(choice, self.slot([0]))
        del choice["study_runs"]
        with self.assertRaisesRegex(ValueError, "study size"):
            self.admit(choice, self.slot([0]))

    def test_pack_slots_share_one_worker_count(self):
        """A member's config implements its slot's counts, so mixed counts run fewer members per wave."""
        pack = self.pack()
        pack["slots"][3] = dict(pack["slots"][3], preparation_workers=2)
        with self.assertRaisesRegex(ValueError, "one worker count"):
            self.admit(self.study(8, pack), self.slot([0]))

    def test_pack_needs_its_declaration_and_records_background_load(self):
        """At each recorded point one declaration of the qualifying reservation held the slots
        and their SMT siblings (CPUs 1 and 3), and no other claim ran on it."""
        def admitted(**loads):
            pack = self.pack()
            pack["load"].update(loads)
            return self.admit(self.study(8, pack), self.slot([0]))
        pack = self.pack()
        del pack["load"]
        with self.assertRaisesRegex(ValueError, "record its host load"):
            self.admit(self.study(8, pack), self.slot([0]))
        for loads in ({"start": self.load(cores=(0, 2, 3, 4, 5))}, {"end": self.load(cores=None)},
                      {"packed": self.load(token="b" * 64)}, {"start": self.load(token=None)}):
            with self.assertRaisesRegex(ValueError, "one declaration"):
                admitted(**loads)
        with self.assertRaisesRegex(ValueError, "another claim ran"):
            admitted(packed=self.load(claims=[((3, 6), "running")]))
        quiet = admitted(start=self.load(claims=[((3,), "suspended")]))
        self.assertEqual(quiet["background_claims"], 0)
        loaded = admitted(start=self.load(claims=[((6, 7), "running")]), end=self.load(claims=[((6, 7), "running")]))
        self.assertEqual(loaded["background_claims"], 1)  # one claim, seen twice

    def test_a_packed_trial_cannot_price_a_solo_allocation(self):
        """A packed trial ran beside others, so it is no serial/parallel qualification."""
        choice = self.choice()
        packed = self.pack_trial("packed-solo", self.request(1)["placement"], 5, 0, 4, barrier=True)
        choice["qualifications"].append(packed["report"])
        with self.assertRaisesRegex(ValueError, "cannot price a solo allocation"):
            self.admit(choice, self.request(1)["placement"])

    def test_a_study_runs_in_one_layout(self):
        """Every family a pack choice selects for must use the same slots."""
        choice = self.study(8, self.pack())
        choice["selected"]["another-family"] = choice["packs"][0]
        self.admit(choice, self.slot([0]))
        choice["selected"]["another-family"] = choice["qualifications"][1]  # a solo allocation
        with self.assertRaisesRegex(ValueError, "one layout"):
            self.admit(choice, self.slot([0]))
        other = self.pack()
        other["slots"][3] = self.slot([1])  # an equal-shaped slot on other CPUs
        choice["selected"]["another-family"] = self.save(other)
        with self.assertRaisesRegex(ValueError, "one layout"):
            self.admit(choice, self.slot([0]))

    def test_kinds_come_from_the_recorded_topology(self):
        """On its own host a pack needs the topology it qualified on; another host's
        pack keeps its own recorded topology."""
        pack = self.pack(topology=[dict(row, efficiency_class=2) if row["cpus"] == "4" else row for row in TOPOLOGY])
        with self.assertRaisesRegex(ValueError, "topology changed"):
            self.admit(self.study(8, pack), self.slot([0]))
        choice = self.study(8, self.pack())
        computehost = [{"cpus": str(cpu), "efficiency_class": 0, "smt": False} for cpu in (0, 2, 4, 5)]
        remote = self.pack(host="computehost", topology=computehost)
        remote["solo"] = remote["solo"][:1]  # one kind there; this host's topology would make two
        choice["packs"].append(self.save(remote))
        choice["inventory"]["computehost"].update(eligible=True, cpu_affinity=[0, 1, 2, 4, 5], transport_seconds=0)
        choice["qualifications"] += [self.save(dict(dispatch.read(dispatch.checked(ref)),
                                                    placement=dict(dispatch.read(dispatch.checked(ref))["placement"],
                                                                   host="computehost")))
                                     for ref in choice["qualifications"]]
        with patch.object(dispatch, "qualification_evidence", side_effect=lambda report, *args: (
                    dispatch.read(dispatch.checked(report["request"])) | {"placement": report["placement"]},
                    self.config, None, "fingerprint")), \
                patch.object(dispatch, "qualification_sample", return_value="sample"), \
                patch.object(dispatch, "qualification_work",
                             side_effect=lambda report, *args: dispatch.read(dispatch.checked(report["execution"]))):
            # The computehost's slots (CPUs 0, 2, 4, 5, one class) classify by its record, not by this host.
            self.assertEqual(self.admit(choice, self.slot([0]))["pack"], choice["packs"][0])

    def test_verified_pack_detects_changed_trial_outputs(self):
        choice = self.study(8, self.pack())
        (self.root / "packed-2-desktop" / "native" / "episode.json").write_text("changed")
        with self.assertRaisesRegex(ValueError, "changed pinned input"):
            self.admit(choice, self.slot([0]))


if __name__ == "__main__":
    unittest.main()
