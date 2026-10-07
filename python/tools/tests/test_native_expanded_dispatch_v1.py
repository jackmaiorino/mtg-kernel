"""Admission and output-equivalence checks, without native/model execution."""
import copy
from datetime import datetime, timezone
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import native_expanded_dispatch_v1 as dispatch


class NativeExpandedAdmissionTests(unittest.TestCase):
    def setUp(self):
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

    def trial(self, workers, seconds):
        request = self.request(workers)
        original = dispatch.read(dispatch.checked(request["config"]))
        config = dispatch.placed_config(request, original, True)
        native = Path(config["output_directory"])
        self.save({"git_commit": self.runtime["engine_commit"],
                   "tracked_tree_sha256": self.runtime["tracked_tree_sha256"]}, native/"run.json")
        trajectory = self.save({"terminal": {"terminal_classification": "natural"}, "tensors": [1, 2]},
                               native/"episode.json")
        collection = self.save({"complete": True, "trajectories": [trajectory]}, native/"collection.json")
        checkpoint = self.save({"trajectories": [trajectory], "parameters": [123],
                                "first_moments": [456], "second_moments": [789], "adam_step": 32401},
                               native/"checkpoint.json")
        update = self.save({"complete": True, "checkpoint": checkpoint})
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

    def test_replication_seeds_only_are_compatible(self):
        replica = copy.deepcopy(self.config)
        replica["iterations"][0]["episodes"][0]["episode"].update(id="replica", seed=999)
        self.assertEqual(dispatch.workload(replica, "training"), dispatch.workload(self.config, "training"))
        self.assertNotEqual(dispatch.workload(replica, "training", True),
                            dispatch.workload(self.config, "training", True))
        for key, value in (("learning_rate", .001), ("loss_selection", {"lambda": 1.0})):
            changed = copy.deepcopy(self.config); changed[key] = value
            self.assertNotEqual(dispatch.workload(changed, "training"), dispatch.workload(self.config, "training"))

    def test_serial_parallel_identical_saved_learning_outputs(self):
        first, second = dispatch.read(self.trial(1, 10)["path"]), dispatch.read(self.trial(2, 6)["path"])
        self.assertEqual(first["fingerprint"], second["fingerprint"])
        result = dispatch.read(second["native_result"]["path"])
        receipt = dispatch.read(result["iterations"][0]["path"])
        update = dispatch.read(receipt["update"]["path"])
        checkpoint = dispatch.read(update["checkpoint"]["path"])
        checkpoint["first_moments"][0] += 1
        update["checkpoint"] = self.save(checkpoint)
        receipt["update"] = self.save(update)
        result["iterations"][0] = self.save(receipt)
        actual = dispatch.output_fingerprint(dispatch.read(second["executed_config"]["path"]),
                                             "training", result, self.runtime, True)
        self.assertNotEqual(first["fingerprint"], actual)

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

    def test_non_natural_tolerance_must_be_declared(self):
        self.config["max_non_natural_episode_fraction"] = 0.2
        request = self.request(1)
        with patch.object(dispatch, "validate_storage"):
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


if __name__ == "__main__":
    unittest.main()
