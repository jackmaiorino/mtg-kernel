"""Tests for the HaleysPC check-only path (python/tools/haley_check_only_v1.py and haley_check_only_runner_v1.py).

Hermetic: no SSH, no HaleysPC; git runs locally on temporary repositories.
"""
from __future__ import annotations

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
TOOLS = REPO_ROOT / "python" / "tools"
if str(TOOLS) not in sys.path:
    sys.path.insert(0, str(TOOLS))

import haley_check_only_runner_v1 as runner  # noqa: E402
import haley_check_only_v1 as controller  # noqa: E402

COMMIT = "34decc2779d566616752f64ec63ab4823fff293c"
TC = "C:\\Users\\haley\\.rustup\\toolchains\\1.94.1-x86_64-pc-windows-msvc\\bin"


def plan(**overrides):
    files = {
        "bundle": {"path": controller.ROOT + "bundles/%s.bundle" % COMMIT, "sha256": "0" * 64},
        "launcher": {"path": controller.ROOT + "tools/%s/haley_check_only_runner_v1.py" % ("1" * 64), "sha256": "1" * 64},
        "helper": {"path": controller.ROOT + "tools/%s/host_reservation_v1.py" % ("2" * 64), "sha256": "2" * 64},
    }
    toolchain = {"cargo": {"path": TC + "\\cargo.exe", "sha256": "3" * 64},
                 "rustc": {"path": TC + "\\rustc.exe", "sha256": "4" * 64}}
    value = controller.make_plan("opus-panel-export", "panel-suite-001", COMMIT,
                                 ["test", "--release", "-p", "mtg-kernel", "--lib"], files, toolchain,
                                 "token-1", "5" * 64)
    value.update(overrides)
    return value


def git(cwd, *args):
    subprocess.run(["git", "-C", str(cwd)] + list(args), check=True, capture_output=True, text=True)


class RunnerTests(unittest.TestCase):
    def refused(self, call, *args):
        with self.assertRaises(SystemExit) as caught:
            call(*args)
        self.assertTrue(str(caught.exception).startswith("check-only refused: "), caught.exception)

    def test_a_valid_plan_passes_and_each_rule_refuses(self):
        runner.check_plan(plan())
        self.refused(runner.check_plan, plan(schema="other/v1"))
        self.refused(runner.check_plan, plan(host="jack"))
        self.refused(runner.check_plan, plan(commit=COMMIT[:8]))
        self.refused(runner.check_plan, plan(source_root="C:/Users/haley/src"))
        self.refused(runner.check_plan, plan(target_dir=controller.ROOT + "../target"))
        self.refused(runner.check_plan, plan(cargo_args=["run", "--bin", "anything"]))
        self.refused(runner.check_plan, plan(cargo_args=["test", "--target-dir", "C:/elsewhere"]))
        self.refused(runner.check_plan, plan(cargo_args=["test", "--config", "build.rustc='x'"]))
        relative = plan()
        relative["toolchain"]["rustc"]["path"] = "rustc.exe"
        self.refused(runner.check_plan, relative)

    def test_the_compiler_is_selected_by_a_backslash_build_rustc(self):
        # CODEX #590: the native-store capture rejects any forward slash in RUSTC.
        expected = "build.rustc='%s\\rustc.exe'" % TC
        self.assertEqual(runner.build_rustc_config(TC + "\\rustc.exe"), expected)
        self.assertEqual(runner.build_rustc_config(TC.replace("\\", "/") + "/rustc.exe"), expected)

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

    def test_the_cargo_command_runs_under_the_reservation_supervisor(self):
        command = runner.build_command(plan(), "C:\\py\\python.exe")
        split = command.index("--")
        self.assertEqual(command[2:split][:3], ["C:\\mtg-line-a\\check-only\\tools\\%s\\host_reservation_v1.py" % ("2" * 64),
                                                 "supervise", "--token"])
        self.assertIn("token-1", command[:split])
        self.assertEqual(command[split + 1:split + 4], [TC + "\\cargo.exe", "--config", "build.rustc='%s\\rustc.exe'" % TC])
        self.assertEqual(command[split + 4:], ["test", "--release", "-p", "mtg-kernel", "--lib"])

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


class ControllerTests(unittest.TestCase):
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
            outside = dict(item, remote="C:/Users/haley/x.json")
            with self.assertRaises(SystemExit):
                controller.staging_map([outside], "a" * 40)

    def test_the_plan_carries_what_the_dispatcher_checks(self):
        value = plan()
        self.assertEqual(value["documents"]["launcher"], value["files"]["launcher"])
        self.assertEqual(value["transport"]["dispatcher"]["sha256"], "5" * 64)
        self.assertEqual(value["reservation_token"], "token-1")

    def test_the_receipt_binds_the_remote_head_and_every_collected_file(self):
        state = {"lane": "opus-panel-export", "run_id": "run-1", "commit": COMMIT, "token": "token-1",
                 "cargo_args": ["test"], "toolchain": plan()["toolchain"]}
        completion = {"commit": COMMIT, "head": COMMIT, "clean": True, "argv": ["cargo"], "started_utc": "s",
                      "finished_utc": "f", "exit_code": 0, "test_counts": {"passed": 1, "failed": 0, "ignored": 0},
                      "toolchain": {"rustc_verbose_version_sha256": "9" * 64, "cargo_version": "cargo 1.94.1"},
                      "nonclaims": ["check-only"]}
        with tempfile.TemporaryDirectory() as tmp:
            (Path(tmp) / "completion.json").write_text(json.dumps(completion), encoding="utf-8")
            (Path(tmp) / "owner-completion.json").write_text(json.dumps({"complete": True}), encoding="utf-8")
            (Path(tmp) / "cargo.log").write_text("test result: ok. 1 passed; 0 failed; 0 ignored\n", encoding="utf-8")
            receipt = controller.make_receipt(state, tmp, {"staging-receipt-1.json": "6" * 64})
            self.assertEqual(receipt["schema"], "haley-check-only-receipt/v1")
            self.assertEqual(sorted(receipt["collected_files"]), ["cargo.log", "completion.json", "owner-completion.json"])
            self.assertTrue(receipt["owner_complete"])
            completion["head"] = "e" * 40
            (Path(tmp) / "completion.json").write_text(json.dumps(completion), encoding="utf-8")
            with self.assertRaises(SystemExit):
                controller.make_receipt(state, tmp, {})


if __name__ == "__main__":
    unittest.main()
