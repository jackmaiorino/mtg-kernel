"""Target ownership and dispatch routing, without remote or native workloads."""
import json
import os
from pathlib import Path
import re
import shutil
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch
import zipfile

import public_training_dispatch_v2 as dispatch


class TargetOwnershipTests(unittest.TestCase):
    def context(self):
        reservations = SimpleNamespace(TOKEN_ENV="MTG_HOST_RESERVATION_TOKEN", status=Mock(return_value={"token_fate": "holds"}))
        slots = SimpleNamespace(CORES_ENV="HOST_SLOTS_CORES", token_hash=lambda token: "hash-"+token,
            timed_state=Mock(return_value=dict(token_sha256="hash-owned", cores=[0, 2], why="timed work declared its cores")),
            parse_cores=Mock(return_value=[0, 2]), current_affinity=Mock(return_value=[0, 2]))
        spec = dict(placement=dict(host="computehost", cpu_affinity=[0, 2]),
                    target_runtime=dict(host_reservation={"sha256": "r"}, host_slots={"sha256": "s"}))
        return reservations, slots, spec

    def test_accepts_exact_live_token_timed_cores_and_actual_affinity(self):
        reservations, slots, spec = self.context()
        with patch.dict(os.environ, {reservations.TOKEN_ENV: "owned", slots.CORES_ENV: "0,2"}), \
                patch.object(dispatch, "target_helpers", return_value=(reservations, slots)):
            result = dispatch.require_target_context(spec)
        self.assertEqual(result["reservation_sha256"], "hash-owned")
        self.assertEqual(result["cpu_affinity"], [0, 2])
        self.assertEqual(result["helpers"], spec["target_runtime"])

    def test_missing_stale_or_mismatched_context_refuses_before_gpu_or_spawn(self):
        for failure in ("no-runtime", "no-token", "stale-token", "other-token", "recorded-only", "timed-cores", "env-cores", "affinity"):
            with self.subTest(failure=failure):
                reservations, slots, spec = self.context()
                environment = {reservations.TOKEN_ENV: "owned", slots.CORES_ENV: "0,2"}
                if failure == "no-runtime": spec.pop("target_runtime")
                if failure == "no-token": environment.pop(reservations.TOKEN_ENV)
                if failure == "stale-token": reservations.status.return_value = {"token_fate": "released"}
                if failure == "other-token": slots.timed_state.return_value["token_sha256"] = "hash-other"
                if failure == "recorded-only": slots.timed_state.return_value["why"] = "the reservation recorded its cores when it was acquired"
                if failure == "timed-cores": slots.timed_state.return_value["cores"] = [0, 4]
                if failure == "env-cores": slots.parse_cores.return_value = [0, 4]
                if failure == "affinity": slots.current_affinity.return_value = [0, 2, 4]
                with patch.dict(os.environ, environment, clear=True), \
                        patch.object(dispatch.platform, "node", return_value="COMPUTEHOST"), \
                        patch.object(dispatch, "target_helpers", return_value=(reservations, slots)), \
                        patch.object(dispatch, "gpu_uuid") as gpu, patch.object(dispatch.subprocess, "Popen") as spawn:
                    with self.assertRaisesRegex(ValueError, "canonical runtime|held reservation"):
                        dispatch.worker(spec)
                    gpu.assert_not_called()
                    spawn.assert_not_called()

    def test_changed_helper_refuses_before_import(self):
        with tempfile.TemporaryDirectory() as temporary:
            helper = Path(temporary)/"host_reservation_v1.py"
            helper.write_text("original", encoding="utf-8")
            ref = dispatch.pin(helper)
            helper.write_text("changed", encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "changed pinned input"):
                dispatch.target_helpers(dict(host_reservation=ref))

    def test_remote_affinity_is_explicit_and_shared(self):
        for placements in ([{}], [{"cpu_affinity": [0, 0]}],
                           [{"cpu_affinity": [0, 2]}, {"cpu_affinity": [0, 4]}]):
            with self.subTest(placements=placements), self.assertRaisesRegex(ValueError, "CPU affinity"):
                dispatch.target_cores(placements)


class TerminalObservationTests(unittest.TestCase):
    def test_nested_completion_does_not_wait_for_or_release_outer_owner(self):
        event = dict(kind="nested-done", by_pid=123, by_process_creation_time=45, outcome="success")
        birth = dict(kind="adopt", pid=123, creation_time=45)
        reservations = Mock()
        reservations.status.return_value = dict(token_fate="holds", events=[birth, event])
        reservations.process_state.side_effect = ["alive", "absent"]
        with patch.object(dispatch.time, "sleep") as sleep:
            result = dispatch.wait_target_dispatch(dict(state="dispatched", nested=True, pid=123, token="outer"),
                                                   reservations, 30)
        self.assertEqual(result, event)
        self.assertEqual(reservations.status.call_count, 2)
        sleep.assert_called_once_with(1)
        reservations.release.assert_not_called()
        reservations.reclaim.assert_not_called()

    def test_old_completed_reused_pid_cannot_finish_current_nested_dispatch(self):
        old_birth = dict(kind="adopt", pid=123, creation_time=45)
        old_done = dict(kind="nested-done", by_pid=123, by_process_creation_time=45, outcome="success")
        new_birth = dict(kind="adopt", pid=123, creation_time=99)
        new_done = dict(kind="nested-done", by_pid=123, by_process_creation_time=99, outcome="success")
        reservations = Mock()
        reservations.status.side_effect = [dict(token_fate="holds", events=[old_birth, old_done, new_birth]),
            dict(token_fate="holds", events=[old_birth, old_done, new_birth, new_done])]
        reservations.process_state.return_value = "absent"
        with patch.object(dispatch.time, "sleep") as sleep:
            result = dispatch.wait_target_dispatch(dict(state="dispatched", nested=True, pid=123, token="outer"),
                reservations, 30, previous_events=[old_birth, old_done])
        self.assertEqual(result, new_done)
        self.assertEqual(reservations.status.call_count, 2)
        reservations.process_state.assert_called_once_with(123, 99)
        sleep.assert_called_once_with(1)

    def test_outer_completion_requires_canonical_release(self):
        event = dict(kind="release", outcome="success")
        reservations = Mock()
        reservations.status.return_value = dict(token_fate="released", token_events=[event])
        self.assertEqual(dispatch.wait_target_dispatch(dict(state="finished-before-handoff", nested=False, token="owned"),
                                                      reservations, 30), event)

    def test_unconfirmed_spawn_never_polls_releases_or_redispatches(self):
        reservations = Mock()
        with self.assertRaisesRegex(RuntimeError, "do not redispatch"):
            dispatch.wait_target_dispatch(dict(state="spawn-unconfirmed"), reservations, 30)
        self.assertEqual(reservations.mock_calls, [])


class ReservedAdapterTests(unittest.TestCase):
    def fixture(self, root):
        def put(name, value):
            path = root/name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(json.dumps(value), encoding="utf-8")
            return dispatch.pin(path)
        runtime = {key: put(key+".py", "fixture") for key in ("host_reservation", "host_slots", "worker", "script")}
        request = put("jobs/r1/request.json", {})
        put("jobs/r1/execution.json", dict(started_unix=105, finished_unix=110))
        put("export-execution.json", dict(files=1, zip_sha256="archive", seconds=2))
        group = put("group.json", dict(mode="sequential", target_runtime=runtime, jobs=dict(r1=dict(
            placement=dict(host="computehost", cpu_affinity=[0, 2], gpu_uuid="GPU-H0"), request=request, wall_seconds=30))))
        reservations = Mock()
        reservations.TOKEN_ENV = "MTG_HOST_RESERVATION_TOKEN"
        reservations.supervisor_python.return_value = "python.exe"
        reservations.dispatch.return_value = dict(state="dispatched", token="owned", nested=False, pid=123)
        slots = SimpleNamespace(format_cores=lambda cores: ",".join(map(str, cores)))
        return group, reservations, slots

    def test_adapter_uses_canonical_dispatch_timed_slots_and_measures_overhead(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            group, reservations, slots = self.fixture(root)
            with patch.object(dispatch, "target_helpers", return_value=(reservations, slots)), \
                    patch.object(dispatch.platform, "node", return_value="COMPUTEHOST"), \
                    patch.object(dispatch, "wait_target_dispatch", return_value=dict(kind="release", outcome="success")), \
                    patch.object(dispatch.time, "time", side_effect=[100, 115]):
                result = dispatch.reserve_worker_group(group["path"])
            self.assertTrue(result["complete"])
            self.assertEqual(result["startup_seconds"], 5)
            self.assertEqual(result["setup_seconds"], 5)
            self.assertEqual(result["recovery_and_release_seconds"], 5)
            kwargs = reservations.dispatch.call_args.kwargs
            command = kwargs["command"]
            self.assertEqual(command[:2], ["python.exe", "-B"])
            self.assertEqual(command[3:7], ["timed", "--cores", "0,2", "--"])
            self.assertEqual(command[-2:], ["-Python", "python.exe"])
            self.assertEqual(kwargs["busy_pattern"], dispatch.BUSY_PATTERN)
            self.assertEqual(dispatch.read(root/"reservation-execution.json"), result)
            reservations.release.assert_not_called()

    def test_setup_accounts_for_gaps_and_stagger_between_native_executions(self):
        for mode, second_start, second_end, expected_setup in (("sequential", 112, 117, 7),
                ("parallel", 108, 113, 8), ("device_queues", 108, 113, 8)):
            with self.subTest(mode=mode), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                group_ref, reservations, slots = self.fixture(root)
                group = dispatch.read(group_ref["path"])
                folder = root/"jobs/r2"
                folder.mkdir()
                request = folder/"request.json"
                request.write_text("{}", encoding="utf-8")
                dispatch.write(folder/"execution.json", dict(started_unix=second_start, finished_unix=second_end))
                group["mode"] = mode
                group["jobs"]["r2"] = dict(group["jobs"]["r1"], request=dispatch.pin(request),
                    placement=dict(host="computehost", cpu_affinity=[0, 2], gpu_uuid="GPU-H1"))
                Path(group_ref["path"]).write_text(json.dumps(group), encoding="utf-8")
                with patch.object(dispatch, "target_helpers", return_value=(reservations, slots)), \
                        patch.object(dispatch.platform, "node", return_value="COMPUTEHOST"), \
                        patch.object(dispatch, "wait_target_dispatch", return_value=dict(kind="release", outcome="success")), \
                        patch.object(dispatch.time, "time", side_effect=[100, 120]):
                    result = dispatch.reserve_worker_group(group_ref["path"])
                self.assertEqual(result["setup_seconds"], expected_setup)
                self.assertEqual(result["recovery_and_release_seconds"], 120-second_end)

    def test_guarded_adapter_routes_inside_timed_owner_and_requires_guard_completion(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            ref, reservations, slots = self.fixture(root)
            group = dispatch.read(ref["path"])
            group["target_limits"] = {}
            Path(ref["path"]).write_text(json.dumps(group))
            dispatch.write(root/"guard-result.json", dict(complete=True, final_sample=dict(active_processes=0)))
            with patch.object(dispatch, "target_helpers", return_value=(reservations, slots)), \
                    patch.object(dispatch, "target_guard_plan"), \
                    patch.object(dispatch.platform, "node", return_value="COMPUTEHOST"), \
                    patch.object(dispatch, "wait_target_dispatch", return_value=dict(outcome="success")), \
                    patch.object(dispatch.time, "time", side_effect=[100, 115]):
                result = dispatch.reserve_worker_group(ref["path"])
            command = reservations.dispatch.call_args.kwargs["command"]
            self.assertLess(command.index("timed"), command.index("--guarded-target"))
            self.assertNotIn("-File", command)
            self.assertTrue(result["guard"]["complete"])

    def test_nested_adapter_captures_event_boundary_before_dispatch(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            group, reservations, slots = self.fixture(root)
            slots.token_hash = lambda token: "hash-"+token
            slots.timed_state = Mock(return_value=dict(token_sha256="hash-outer", why="no core declaration"))
            old = dict(kind="nested-done", by_pid=123, by_process_creation_time=45, outcome="success")
            reservations.read_events.return_value = ([old], [])
            reservations.dispatch.return_value = dict(state="dispatched", token="outer", nested=True, pid=123)
            def dispatched(**kwargs):
                reservations.read_events.assert_called_once_with("outer")
                return reservations.dispatch.return_value
            reservations.dispatch.side_effect = dispatched
            with patch.dict(os.environ, {reservations.TOKEN_ENV: "outer"}), \
                    patch.object(dispatch, "target_helpers", return_value=(reservations, slots)), \
                    patch.object(dispatch.platform, "node", return_value="COMPUTEHOST"), \
                    patch.object(dispatch, "wait_target_dispatch", return_value=dict(outcome="success")) as wait, \
                    patch.object(dispatch.time, "time", side_effect=[100, 115]):
                result = dispatch.reserve_worker_group(group["path"])
            self.assertTrue(result["complete"])
            self.assertEqual(result["prior_event_count"], 1)
            self.assertEqual(wait.call_args.args[3], [old])

    def test_busy_refusal_or_lost_response_is_retained_without_retry(self):
        for lost in (False, True):
            with self.subTest(lost=lost), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                group, reservations, slots = self.fixture(root)
                if lost:
                    reservations.dispatch.return_value = dict(state="spawn-unconfirmed", token="owned", nested=False)
                else:
                    reservations.dispatch.side_effect = RuntimeError("foreign native work")
                with patch.object(dispatch, "target_helpers", return_value=(reservations, slots)), \
                        patch.object(dispatch.platform, "node", return_value="COMPUTEHOST"), patch("builtins.print"):
                    with self.assertRaisesRegex(RuntimeError, "foreign native work|do not redispatch"):
                        dispatch.reserve_worker_group(group["path"])
                self.assertFalse(dispatch.read(root/"reservation-execution.json")["complete"])
                reservations.dispatch.assert_called_once()
                reservations.status.assert_not_called()
                reservations.release.assert_not_called()

    def test_already_timed_outer_owner_is_refused_without_second_dispatch(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            group, reservations, slots = self.fixture(root)
            slots.token_hash = lambda token: "hash-"+token
            slots.timed_state = Mock(return_value=dict(token_sha256="hash-outer", why="timed work declared its cores"))
            with patch.dict(os.environ, {reservations.TOKEN_ENV: "outer"}), \
                    patch.object(dispatch, "target_helpers", return_value=(reservations, slots)), \
                    patch.object(dispatch.platform, "node", return_value="COMPUTEHOST"), patch("builtins.print"):
                with self.assertRaisesRegex(ValueError, "outer reservation already has a timed declaration"):
                    dispatch.reserve_worker_group(group["path"])
            reservations.dispatch.assert_not_called()
            reservations.release.assert_not_called()

    def test_transport_stages_helpers_and_charges_target_setup_and_recovery(self):
        self.transport_fixture()

    def test_transport_stages_guard_closure_and_campaign_root_without_config_changes(self):
        self.transport_fixture(TargetGuardTests().limits())

    def transport_fixture(self, target_limits=None):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            binary = root/"fixture.exe"
            binary.write_bytes(b"never executed")
            config = root/"config.json"
            config.write_text(json.dumps(dict(gpu_ordinal=1, updates=[[{}]])), encoding="utf-8")
            destination = root/"qualified-group"
            local = destination/"computehost"
            archive = root/"remote-export.zip"
            commands = []
            def run(command, **kwargs):
                commands.append(command)
                if command[0] == "ssh":
                    self.assertIn("--reserve-worker-group", command)
                    self.assertNotIn("--export", command)
                    group = dispatch.read(local/"group.json")
                    expected = {"host_reservation", "host_slots", "worker", "script"}
                    if target_limits is not None:
                        expected.update(dispatch.GUARD_MODULES)
                        self.assertEqual(group["target_limits"], target_limits)
                        self.assertIn(target_limits["group_root"]+"/worker.py", command)
                    self.assertEqual(set(group["target_runtime"]), expected)
                    for ref in group["target_runtime"].values():
                        self.assertEqual(dispatch.pin(local/Path(ref["path"]).name)["sha256"], ref["sha256"])
                    script = (local/"run.ps1").read_text()
                    mode = "--target-child" if target_limits is not None else "--worker-group"
                    self.assertIn('& $Python -B "$root/worker.py" '+mode, script)
                    self.assertEqual('--export "$root"' in script, target_limits is None)
                    request = dispatch.read(local/"jobs/r1/request.json")
                    self.assertEqual(request["config"], dispatch.read(config))
                    self.assertEqual(request["execution_gpu_ordinal"], 0)
                    self.assertEqual(group["jobs"]["r1"]["placement"]["cpu_affinity"], [0, 2])
                    files = {"jobs/r1/request.json": (local/"jobs/r1/request.json").read_bytes(),
                             "jobs/r1/execution.json": b'{}', "jobs/r1/recovery.json": b'{}',
                             "jobs/r1/outputs/completion.json": b'{"first_update":0,"next_update":1}'}
                    for name in ("optimizer.json", "checkpoint.json", "episode-000.json"):
                        files["jobs/r1/outputs/0000/"+name] = b'{}'
                    import hashlib
                    manifest = {name: hashlib.sha256(payload).hexdigest() for name, payload in files.items()}
                    with zipfile.ZipFile(archive, "x") as output:
                        for name, payload in files.items(): output.writestr(name, payload)
                        output.writestr("export-manifest.json", json.dumps(manifest))
                    kwargs["stdout"].write(json.dumps(dict(complete=True, startup_seconds=3, setup_seconds=4,
                        recovery_and_release_seconds=5, export=dict(zip_sha256=dispatch.pin(archive)["sha256"]))))
                elif command[0] == "scp" and command[2].endswith("/results.zip"):
                    shutil.copyfile(archive, command[3])
                return SimpleNamespace(returncode=0)
            placement = dict(host="computehost", gpu_ordinal=0, gpu_uuid="GPU-H0", workers=4,
                             cpu_affinity=[0, 2], remote_binding=dict(expected_hostname="HALEYSPC", input_root="C:/inputs"))
            with patch.object(dispatch, "preflight", return_value={}), patch.object(dispatch, "ssh_ps"), \
                    patch.object(dispatch.subprocess, "run", side_effect=run):
                report = dispatch.read(dispatch._dispatch_group(destination, dispatch.pin(binary),
                    {"r1": dispatch.pin(config)}, {"r1": placement}, "sequential", 1, 30,
                    target_limits=target_limits)["path"])
            self.assertEqual(sum(command[0] == "ssh" for command in commands), 1)
            self.assertEqual(report["reservation_setup_seconds"], 4)
            self.assertGreaterEqual(report["staging_seconds"], 4)
            self.assertGreaterEqual(report["recovery_seconds"], 5)


class BusyFamilyTests(unittest.TestCase):
    def test_busy_families_cover_all_known_training_and_replay_competitors(self):
        for process in ("stack_feature_training_v1.exe", "stack_policy_replay_v1.exe",
                        "own_registration_training_v1.exe", "phase1_native_actor.exe",
                        "phase1_native_actor_v2.exe", "cargo.exe", "rustc.exe", "trainer.exe",
                        "public_feature_training_v1.exe", "expanded_deck_training_v1.exe"):
            with self.subTest(process=process):
                self.assertIsNotNone(re.search(dispatch.BUSY_PATTERN, process, re.IGNORECASE))
        self.assertIsNone(re.search(dispatch.BUSY_PATTERN, "python.exe", re.IGNORECASE))

    def test_preflight_uses_the_same_canonical_post_acquire_busy_pattern(self):
        report = dict(host="COMPUTEHOST", active=[], disk_free_bytes=dispatch.DISK_RESERVE_BYTES, gpu=[])
        with patch.object(dispatch, "ssh_ps", return_value=json.dumps(report)) as ssh:
            dispatch.preflight("computehost", [], "C")
        self.assertIn("-match '"+dispatch.BUSY_PATTERN+"'", ssh.call_args.args[0])


class TargetGuardTests(unittest.TestCase):
    def limits(self):
        return dict(group_root="C:/mtg-node/campaign/group-001", cpu_seconds=3600, wall_seconds=900,
            storage=dict(accounting_roots=["C:/mtg-node/campaign"], max_physical_bytes=8*1024**3,
                         projected_additional_bytes=4*1024**3, reserve_bytes=60*1024**3),
            stop_files=["C:/mtg-node/STOP", "C:/mtg-node/campaign/STOP"])

    def test_campaign_cap_covers_group_and_retained_siblings(self):
        limits = self.limits()
        plan = dispatch.target_guard_plan(limits, [0, 2], limits["group_root"])
        self.assertEqual(plan["storage"], limits["storage"])
        self.assertEqual(plan["run_limits"], dict(cpu_seconds=3600, wall_seconds=900))
        for change in ("outside", "overlap", "reserve", "projection", "stop", "infinite"):
            with self.subTest(change=change):
                limits = self.limits()
                if change == "outside": limits["storage"]["accounting_roots"] = ["C:/mtg-node/other"]
                if change == "overlap": limits["storage"]["accounting_roots"].append(limits["group_root"])
                if change == "reserve": limits["storage"]["reserve_bytes"] = 1
                if change == "projection": limits["storage"]["projected_additional_bytes"] = 9*1024**3
                if change == "stop": limits["stop_files"] = []
                if change == "infinite": limits["wall_seconds"] = float("inf")
                with self.assertRaises(ValueError):
                    dispatch.target_guard_plan(limits, [0, 2], limits["group_root"])

    def test_entire_guard_import_closure_is_checked_before_import(self):
        with patch.object(dispatch, "checked", side_effect=ValueError("changed dependency")), \
                patch("importlib.import_module") as imported:
            with self.assertRaisesRegex(ValueError, "changed dependency"):
                dispatch.target_guard_class({name: {} for name in dispatch.GUARD_MODULES})
        imported.assert_not_called()

    def test_script_training_and_export_share_contained_guard_and_failure_receipt(self):
        for failure in ("none", "script", "watcher", "cpu", "wall", "descendants"):
            code = 9 if failure == "script" else 0
            with self.subTest(failure=failure), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                dispatch.write(root/"group.json", dict(target_limits=self.limits(), target_runtime={},
                    jobs=dict(r1=dict(placement=dict(cpu_affinity=[0, 2])))))
                instance = Mock()
                def construct(plan, directory, qualifying):
                    directory.mkdir()
                    return instance
                instance.spawn.return_value.wait.return_value = code
                instance.check.return_value = {"charged_physical_bound_bytes": 1}
                instance.error = "STOP requested" if failure == "watcher" else None
                instance.terminate_and_drain.return_value = dict(active_processes=1 if failure == "descendants" else 0,
                    cpu_seconds=3600 if failure == "cpu" else 1, seconds=900 if failure == "wall" else 1)
                with patch.object(dispatch, "target_guard_plan", return_value=dict(run_limits=dict(cpu_seconds=3600, wall_seconds=900))), \
                        patch.object(dispatch, "target_guard_class", return_value=construct), \
                        patch.object(dispatch, "target_helpers", return_value=(Mock(), Mock())), \
                        patch.object(dispatch, "checked", return_value=root/"run.ps1"):
                    # Script pin is obtained only after Guard admission.
                    group = dispatch.read(root/"group.json")
                    group["target_runtime"]["script"] = {}
                    (root/"group.json").write_text(json.dumps(group))
                    if failure != "none":
                        with self.assertRaisesRegex(RuntimeError, "contained target script failed|target Guard terminal failure"):
                            dispatch.guarded_target(root/"group.json")
                    else:
                        dispatch.guarded_target(root/"group.json")
                instance.spawn.assert_called_once()
                self.assertIn(str(root/"run.ps1"), instance.spawn.call_args.args[0])
                instance.quiet.assert_called_once()
                instance.terminate_and_drain.assert_called_once()
                instance.close.assert_called_once()
                self.assertEqual(dispatch.read(root/"guard-result.json")["complete"], failure == "none")


    def test_contained_profile_clears_experiments_preserves_owner_and_contains_caches(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            (root/"temp").mkdir()
            dispatch.write(root/"group.json", dict(jobs=dict(r1=dict(placement=dict(host="computehost")))))
            inherited = dict(MTG_HOST_RESERVATION_TOKEN="owner", HOST_SLOTS_CLAIM="claim", HOST_SLOTS_CORES="20,21",
                MTG_UNEXPECTED="remove", S4A_UNEXPECTED="remove", PROFILE_UNEXPECTED="remove", BUDGET_UNEXPECTED="remove",
                CUBECL_UNEXPECTED="remove", CUDA_VISIBLE_DEVICES="")
            with patch.dict(os.environ, inherited, clear=True), patch.object(dispatch.os, "chdir") as cwd, \
                    patch.object(dispatch, "worker_group") as worker, patch.object(dispatch, "export_group") as export:
                dispatch.target_child(root/"group.json")
                for key in inherited:
                    if key.endswith("UNEXPECTED") or key == "CUDA_VISIBLE_DEVICES":
                        self.assertNotIn(key, os.environ)
                self.assertEqual(os.environ["MTG_HOST_RESERVATION_TOKEN"], "owner")
                self.assertEqual(os.environ["HOST_SLOTS_CLAIM"], "claim")
                self.assertEqual(os.environ["HOST_SLOTS_CORES"], "20,21")
                self.assertEqual(os.environ["CUDA_CACHE_PATH"], str(root/"cuda-cache"))
                self.assertEqual(os.environ["CUDA_CACHE_MAXSIZE"], "67108864")
                self.assertEqual(os.environ["RUST_MIN_STACK"], "16777216")
                self.assertEqual(os.environ[dispatch.REMOTE_INPUT_ENV], dispatch.REMOTE_INPUT)
                self.assertEqual(os.environ["TEMP"], str(root/"temp"))
                cwd.assert_called_once_with(root)
                worker.assert_called_once_with(root/"group.json")
                export.assert_called_once_with(root)
            profile = dispatch.read(root/"runtime-profile.json")
            self.assertEqual(profile["cubecl_configuration"], dispatch.pin(root/"cubecl.toml"))
            self.assertIn((root/"cube-cache").as_posix(), (root/"cubecl.toml").read_text())


if __name__ == "__main__":
    unittest.main()
