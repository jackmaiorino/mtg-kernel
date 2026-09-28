"""Tests for the HaleysPC check-only path (python/tools/haley_check_only_v1.py, haley_check_only_runner_v1.py and
haley_check_only_dispatch_v1.py).

Hermetic: no SSH, no HaleysPC, no WMI; git runs locally on temporary repositories and the reservation helper is
a fake with the same entry points.
"""
from __future__ import annotations

import json
import subprocess
import sys
import tempfile
import types
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
TOOLS = REPO_ROOT / "python" / "tools"
if str(TOOLS) not in sys.path:
    sys.path.insert(0, str(TOOLS))

import haley_check_only_dispatch_v1 as dispatcher  # noqa: E402
import haley_check_only_runner_v1 as runner  # noqa: E402
import haley_check_only_v1 as controller  # noqa: E402

COMMIT = "34decc2779d566616752f64ec63ab4823fff293c"
TC = "C:\\Users\\haley\\.rustup\\toolchains\\1.94.1-x86_64-pc-windows-msvc\\bin"


def plan(**overrides):
    files = {name: {"path": controller.ROOT + "tools/%s/%s" % (str(i) * 64, name), "sha256": str(i) * 64}
             for i, name in enumerate(("launcher", "dispatch", "helper"), start=1)}
    files["bundle"] = {"path": controller.ROOT + "bundles/%s.bundle" % COMMIT, "sha256": "0" * 64}
    toolchain = {"cargo": {"path": TC + "\\cargo.exe", "sha256": "4" * 64},
                 "rustc": {"path": TC + "\\rustc.exe", "sha256": "5" * 64}}
    value = controller.make_plan("opus-panel-export", "panel-suite-001", COMMIT,
                                 ["test", "--release", "-p", "mtg-kernel", "--lib"], files, toolchain)
    value.update(overrides)
    return value


def fake_helper(fate="holds", lane="opus-panel-export", work_id="panel-suite-001"):
    calls = []
    module = types.SimpleNamespace(TOKEN_ENV="MTG_HOST_RESERVATION_TOKEN")
    module.status = lambda token: {"token_fate": fate, "record": {"lane": lane, "work_id": work_id, "token": token}}
    module.dispatch = lambda *args, **kwargs: calls.append((args, kwargs)) or {"state": "dispatched", "token": "t-1"}
    return module, calls


def git(cwd, *args):
    subprocess.run(["git", "-C", str(cwd)] + list(args), check=True, capture_output=True, text=True)


class RunnerTests(unittest.TestCase):
    def refused(self, call, *args):
        with self.assertRaises(SystemExit) as caught:
            call(*args)
        self.assertIn("refused: ", str(caught.exception))

    def test_a_valid_plan_passes_and_each_rule_refuses(self):
        runner.check_plan(plan())
        self.refused(runner.check_plan, plan(schema="other/v1"))
        self.refused(runner.check_plan, plan(host="jack"))
        self.refused(runner.check_plan, plan(commit=COMMIT[:8]))
        self.refused(runner.check_plan, plan(source_root="C:/Users/haley/src"))
        self.refused(runner.check_plan, plan(worker_root="C:/Users/haley/runs/x"))
        self.refused(runner.check_plan, plan(target_dir=controller.ROOT + "../target"))
        self.refused(runner.check_plan, plan(cargo_args=["run", "--bin", "anything"]))
        self.refused(runner.check_plan, plan(cargo_args=["test", "--target-dir", "C:/elsewhere"]))
        self.refused(runner.check_plan, plan(cargo_args=["test", "--config", "build.rustc='x'"]))
        self.refused(runner.check_plan, plan(reserve_bytes=59 * 2 ** 30))
        self.refused(runner.check_plan, plan(growth_bytes=0))
        relative = plan()
        relative["toolchain"]["rustc"]["path"] = "rustc.exe"
        self.refused(runner.check_plan, relative)

    def test_the_compiler_is_selected_by_a_backslash_build_rustc(self):
        # CODEX #590: the native-store capture rejects any forward slash in RUSTC.
        expected = "build.rustc='%s\\rustc.exe'" % TC
        self.assertEqual(runner.build_rustc_config(TC + "\\rustc.exe"), expected)
        self.assertEqual(runner.build_rustc_config(TC.replace("\\", "/") + "/rustc.exe"), expected)
        self.assertEqual(runner.build_command(plan()),
                         [TC + "\\cargo.exe", "--config", expected, "test", "--release", "-p", "mtg-kernel", "--lib"])

    def test_overrides_are_removed_and_the_run_is_isolated(self):
        base = {"PATH": "C:\\Windows", "RUSTC": "C:\\x\\rustc.exe", "rustflags": "-C target-cpu=native",
                "CARGO_TARGET_DIR": "C:\\shared", "RUSTC_WRAPPER": "sccache", "HOME": "C:\\Users\\haley"}
        env = runner.build_environment(base, plan())
        for name in ("RUSTC", "rustflags", "RUSTC_WRAPPER"):
            self.assertNotIn(name, env)
        self.assertEqual(env["PATH"].split(";")[0], TC)
        self.assertEqual(env["CARGO_TARGET_DIR"], "C:\\mtg-line-a\\check-only\\target\\opus-panel-export")
        self.assertEqual(env["TEMP"], env["TMP"])
        self.assertEqual(env["CARGO_INCREMENTAL"], "0")
        self.assertEqual(env["HOME"], "C:\\Users\\haley")

    def test_the_runner_runs_only_under_its_own_held_reservation(self):
        helper, _ = fake_helper()
        self.assertEqual(runner.held_reservation(plan(), helper, {"MTG_HOST_RESERVATION_TOKEN": "t-1"}), "t-1")
        self.refused(runner.held_reservation, plan(), helper, {})
        for other in (fake_helper(fate="released")[0], fake_helper(lane="opus-exit-teacher")[0],
                      fake_helper(work_id="another-run")[0]):
            self.refused(runner.held_reservation, plan(), other, {"MTG_HOST_RESERVATION_TOKEN": "t-1"})

    def test_the_build_never_takes_c_below_its_reserve(self):
        runner.check_space(plan(), 68 * 2 ** 30)
        self.refused(runner.check_space, plan(), 67 * 2 ** 30)
        self.assertEqual((plan()["reserve_bytes"], plan()["growth_bytes"]), (60 * 2 ** 30, 8 * 2 ** 30))

    def test_test_counts_sum_every_result_line(self):
        log = ("test result: ok. 3 passed; 0 failed; 1 ignored; 0 measured\n"
               "test result: FAILED. 134 passed; 3 failed; 31 ignored; 0 measured; 2297 filtered out\n")
        self.assertEqual(runner.test_counts(log), {"passed": 137, "failed": 3, "ignored": 32})

    def test_the_source_is_a_clean_detached_checkout_of_the_pinned_commit(self):
        with tempfile.TemporaryDirectory() as tmp:
            origin = Path(tmp) / "origin"
            origin.mkdir()
            git(origin, "init", "-q", "-b", "main")
            (origin / "a.txt").write_text("a\n", encoding="utf-8")
            git(origin, "add", "a.txt")
            git(origin, "-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "one")
            commit = subprocess.run(["git", "-C", str(origin), "rev-parse", "HEAD"], capture_output=True,
                                    text=True).stdout.strip()
            bundle = controller.make_bundle(origin, "main", commit, tmp)
            local = {"commit": commit, "source_root": str(Path(tmp) / "src"), "files": {"bundle": {"path": str(bundle)}}}
            self.assertEqual(runner.prepare_source(local), commit)
            self.assertEqual(runner.prepare_source(local), commit)
            (Path(tmp) / "src" / "a.txt").write_text("changed\n", encoding="utf-8")
            self.refused(runner.prepare_source, local)
            self.refused(controller.make_bundle, origin, "main", "f" * 40, tmp)


class DispatchTests(unittest.TestCase):
    def test_the_owner_command_is_the_runner_on_the_plan(self):
        value = plan()
        command = dispatcher.owner_command(value, controller.ROOT + "plans/panel-suite-001.json", "C:/py/python.exe")
        self.assertEqual(command, ["C:\\py\\python.exe", "-u", "C:\\mtg-line-a\\check-only\\tools\\%s\\launcher" % ("1" * 64),
                                   "--manifest", "C:\\mtg-line-a\\check-only\\plans\\panel-suite-001.json",
                                   "--host", "haleyspc", "--root", "C:\\mtg-line-a\\check-only\\runs\\panel-suite-001"])

    def test_the_reservation_is_acquired_and_the_supervisor_created_in_one_call(self):
        helper, calls = fake_helper()
        with tempfile.TemporaryDirectory() as tmp:
            plan_path = Path(tmp) / "plan.json"
            plan_path.write_text(json.dumps(plan()), encoding="utf-8")
            result = dispatcher.dispatch(plan(), str(plan_path), "C:/py/python.exe", helper)
        self.assertEqual(result["state"], "dispatched")
        (args, kwargs), = calls
        self.assertEqual(args[:3], ("opus-panel-export", "panel-suite-001",
                                    "the supervisor job is empty after the cargo command"))
        self.assertEqual(args[3][2:4], ["C:\\mtg-line-a\\check-only\\tools\\%s\\launcher" % ("1" * 64), "--manifest"])
        self.assertEqual(kwargs["cwd"], "C:\\mtg-line-a\\check-only")
        self.assertEqual(kwargs["busy_pattern"], dispatcher.BUSY_PATTERN)
        self.assertEqual(kwargs["python"], "C:\\py\\python.exe")
        self.assertEqual(kwargs["transport_record"]["kind"], "haley-check-only-v1")

    def test_pins_outside_the_root_are_refused_before_hashing(self):
        value = plan()
        value["files"]["helper"]["path"] = "C:/Users/haley/host_reservation_v1.py"
        with self.assertRaises(SystemExit):
            dispatcher.check_pins(value, controller.ROOT + "plans/x.json", value["files"]["dispatch"]["path"])


class FakeRemote:
    answers = []

    def powershell(self, script, timeout=1800, check=True):
        return types.SimpleNamespace(returncode=0, stdout=FakeRemote.answers.pop(0), stderr='')


class ControllerTests(unittest.TestCase):
    def test_the_dispatch_answer_is_the_last_stdout_line_as_an_object(self):
        self.assertEqual(controller.last_json('noise\n{"state": "held"}\n'), {"state": "held"})
        for text in ('', None, 'not json', '[1, 2]'):
            self.assertIsNone(controller.last_json(text))

    def test_only_a_definite_held_answer_is_retried(self):
        original = controller.Remote
        controller.Remote = FakeRemote
        try:
            with tempfile.TemporaryDirectory() as tmp:
                run_dir = Path(tmp)
                controller.write_json(run_dir / 'state.json', {'run_id': 'run-1', 'files': plan()['files'],
                                                               'paths': {'plan': controller.ROOT + 'plans/run-1.json'}})
                args = types.SimpleNamespace(run_dir=tmp)
                FakeRemote.answers = ['{"state": "held", "detail": "reserved"}',
                                      '{"state": "dispatched", "token": "t-1", "pid": 7}']
                with self.assertRaises(SystemExit):
                    controller.cmd_dispatch(args)
                self.assertFalse((run_dir / 'dispatch.json').exists())
                controller.cmd_dispatch(args)
                self.assertEqual(json.loads((run_dir / 'dispatch.json').read_text(encoding='utf-8'))['token'], 't-1')
                with self.assertRaises(SystemExit):
                    controller.cmd_dispatch(args)
                history = (run_dir / 'dispatch-attempts.jsonl').read_text(encoding='utf-8').splitlines()
                self.assertEqual([json.loads(line)['state'] for line in history], ['held', 'dispatched'])
            for lost in ('', '{"state": "spawn-unconfirmed", "token": "t-2"}', '{"state": "refused"}'):
                with tempfile.TemporaryDirectory() as tmp:
                    controller.write_json(Path(tmp) / 'state.json', {'run_id': 'run-2', 'files': plan()['files'],
                                                                     'paths': {'plan': controller.ROOT + 'x.json'}})
                    FakeRemote.answers = [lost, '{"state": "dispatched", "token": "t-3"}']
                    for _ in range(2):
                        with self.assertRaises(SystemExit):
                            controller.cmd_dispatch(types.SimpleNamespace(run_dir=tmp))
                    self.assertEqual(len(FakeRemote.answers), 1)
        finally:
            controller.Remote = original

    def test_remote_layout_and_staging_map(self):
        paths = controller.remote_paths("opus-panel-export", "run-1", COMMIT)
        self.assertTrue(all(p.startswith(controller.ROOT) for p in paths.values()))
        with tempfile.TemporaryDirectory() as tmp:
            local = Path(tmp) / "x.json"
            local.write_text("{}", encoding="utf-8")
            item = controller.entry(local, paths["plan"])
            self.assertEqual(item["bytes"], 2)
            staged = controller.staging_map([item], "a" * 40)
            self.assertEqual((staged["schema"], staged["host"]), ("g115-line-a-staging-map/v1", "haleyspc"))
            with self.assertRaises(SystemExit):
                controller.staging_map([dict(item, remote="C:/Users/haley/x.json")], "a" * 40)

    def test_the_plan_carries_its_roots_and_release_condition(self):
        value = plan()
        self.assertEqual(sorted(value["files"]), ["bundle", "dispatch", "helper", "launcher"])
        self.assertEqual(value["worker_root"], controller.ROOT + "runs/panel-suite-001")
        self.assertNotIn("reservation_token", value)
        runner.check_plan(value)

    def test_the_receipt_binds_the_remote_head_the_token_and_every_collected_file(self):
        state = {"lane": "opus-panel-export", "run_id": "run-1", "commit": COMMIT, "cargo_args": ["test"],
                 "toolchain": plan()["toolchain"]}
        completion = {"commit": COMMIT, "run_id": "run-1", "head": COMMIT, "clean": True, "argv": ["cargo"], "started_utc": "s",
                      "finished_utc": "f", "exit_code": 0, "test_counts": {"passed": 1, "failed": 0, "ignored": 0},
                      "toolchain": {"rustc_verbose_version_sha256": "9" * 64, "cargo_version": "cargo 1.94.1"},
                      "reservation_token": "t-1", "nonclaims": ["check-only"]}
        with tempfile.TemporaryDirectory() as tmp:
            folder = Path(tmp)
            (folder / "dispatch.json").write_text(json.dumps({"state": "dispatched", "token": "t-1", "pid": 7}), encoding="utf-8")
            (folder / "reservation-status.json").write_text(json.dumps({"token_fate": "released"}), encoding="utf-8")
            (folder / "cargo.log").write_text("test result: ok. 1 passed; 0 failed; 0 ignored\n", encoding="utf-8")
            (folder / "completion.json").write_text(json.dumps(completion), encoding="utf-8")
            receipt = controller.make_receipt(state, tmp, "6" * 64)
            self.assertEqual((receipt["schema"], receipt["outcome"]), ("haley-check-only-receipt/v1", "ran"))
            self.assertEqual(receipt["reservation"], {"token": "t-1", "dispatch_state": "dispatched", "supervisor_pid": 7,
                                                      "token_fate": "released", "runner_token": "t-1"})
            self.assertEqual(sorted(receipt["collected_files"]),
                             ["cargo.log", "completion.json", "dispatch.json", "reservation-status.json"])
            for field, value in (("head", "e" * 40), ("reservation_token", "t-2"), ("run_id", "run-0")):
                (folder / "completion.json").write_text(json.dumps(dict(completion, **{field: value})), encoding="utf-8")
                with self.assertRaises(SystemExit):
                    controller.make_receipt(state, tmp, "6" * 64)
            stopped = {key: completion[key] for key in ("commit", "run_id", "started_utc", "finished_utc", "nonclaims")}
            stopped.update({"exit_code": None, "refused": "check-only refused: cargo differs from its pin"})
            (folder / "cargo.log").unlink()
            (folder / "completion.json").write_text(json.dumps(stopped), encoding="utf-8")
            receipt = controller.make_receipt(state, tmp, "6" * 64)
            self.assertEqual((receipt["outcome"], receipt["exit_code"]), ("refused", None))
            self.assertIn("cargo differs", receipt["detail"])
            self.assertNotIn("test_counts", receipt)


if __name__ == "__main__":
    unittest.main()
