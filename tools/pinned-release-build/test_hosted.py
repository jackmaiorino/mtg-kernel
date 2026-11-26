"""Exercise admission and owned-process cleanup without invoking Docker."""

from io import StringIO
import json
import os
from pathlib import Path
import re
import shlex
import shutil
import signal
import subprocess
import sys
import tempfile
import unittest
from types import SimpleNamespace
from unittest.mock import Mock, patch

import hosted

HERE = Path(__file__).resolve().parent
PORTABLE_SCHEMA = "fleet-combined-trainer-build/v1"
VARIANT_SCHEMA = "fleet-combined-trainer-build-target-variant/v1"
DEFAULT_BINS = ("native_expanded_training_run_v1", "checkpoint_container_v1")
BIN_SELECTIONS = (
    (None, DEFAULT_BINS),
    ("", DEFAULT_BINS),
    (", ".join(DEFAULT_BINS), DEFAULT_BINS),
    ("regret_census_v1", ("regret_census_v1",)),
    ("checkpoint_container_v1, regret_census_v1\tnative_expanded_training_run_v1",
     ("checkpoint_container_v1", "regret_census_v1", "native_expanded_training_run_v1")),
)
INVALID_SELECTIONS = (
    ("other_binary", "unknown PINNED_BINS binary"),
    ("regret_census_v1,checkpoint_container_v1 regret_census_v1", "duplicate PINNED_BINS binary"),
    (" , \t,\n", "empty PINNED_BINS list"),
    ("   ", "empty PINNED_BINS list"),
)


def import_hosted(value):
    """Import hosted.py in a fresh interpreter with PINNED_TARGET_CPU set to `value` (None: unset)."""
    env = {key: item for key, item in os.environ.items() if key != "PINNED_TARGET_CPU"}
    if value is not None:
        env["PINNED_TARGET_CPU"] = value
    return subprocess.run([sys.executable, "-B", "-c", "import hosted; print(hosted.TARGET_CPU)"], cwd=HERE, env=env,
                          capture_output=True, text=True, timeout=60)


def build_sh(target_cpu=None, bins=None):
    """Report the selected array while forcing the helper to stop at its platform check."""
    settings = "\n".join(f"unset {key}" if value is None else f"export {key}={shlex.quote(value)}"
                         for key, value in (("PINNED_TARGET_CPU", target_cpu), ("PINNED_BINS", bins)))
    script = settings + r'''
uname() { printf '%s\\n' unsupported-architecture; }
trap 'printf "%s\\n" "${bins[@]}"' EXIT
source ./build.sh
'''
    result = subprocess.run(["bash", "-s", "--", "0" * 40, "/work/out"], cwd=HERE,
                            input=script.encode("utf-8"), capture_output=True, timeout=60)
    return subprocess.CompletedProcess(result.args, result.returncode, result.stdout.decode(), result.stderr.decode())


class TargetVariantTests(unittest.TestCase):
    def test_the_environment_selects_the_variant_and_an_unknown_name_is_refused(self):
        self.assertEqual(import_hosted(None).stdout.strip(), "default")
        for name in hosted.TARGET_CPUS:
            self.assertEqual(import_hosted(name).stdout.strip(), name)
        for name in ("", "bogus", "znver5 -C x", "ZNVER5"):
            result = import_hosted(name)
            self.assertNotEqual(result.returncode, 0, name)
            self.assertIn("unknown PINNED_TARGET_CPU", result.stderr)

    def test_the_workflow_names_the_repository_by_id_so_a_move_to_another_owner_keeps_the_job(self):
        workflow = (HERE.parents[1] / ".github/workflows/pinned-release-build.yml").read_text(encoding="utf-8")
        condition = re.search(r"\n  build:\n(?:    #.*\n)*    if: >-\n((?:      .*\n)+)", workflow).group(1)
        self.assertIn("github.repository_id == '1303347549' &&", condition)
        self.assertIn("github.event.pull_request.head.repo.full_name == github.repository", condition)
        # No owner or repository name: either would skip the job after a rename or a transfer.
        self.assertNotRegex(condition, r"github\.repository\s*==\s*'")
        self.assertNotIn("repository_owner", condition)

    def test_the_workflow_the_guard_and_the_build_helper_name_the_same_variants(self):
        workflow = (HERE.parents[1] / ".github/workflows/pinned-release-build.yml").read_text(encoding="utf-8")
        options = re.search(r"target_cpu:\n(?:.*\n)*?\s+options:\n((?:\s+- \S+\n)+)", workflow).group(1)
        helper = (HERE / "build.sh").read_text(encoding="utf-8")
        cases = re.search(r'case "\$variant" in\n((?:.*\n)*?)esac', helper).group(1)
        labels = re.findall(r"^\s+([a-z0-9-]+)\) rustflags=", cases, flags=re.M)
        self.assertEqual(tuple(re.findall(r"- (\S+)", options)), hosted.TARGET_CPUS)
        self.assertEqual(tuple(labels), hosted.TARGET_CPUS)
        self.assertEqual(hosted.TARGET_CPUS[0], "default")
        # The value reaches the shell as an environment variable, never as script text.
        for script in re.findall(r"run: \|\n((?:(?: {10}.*)?\n)+)", workflow):
            self.assertNotIn("inputs.target_cpu", script)

    @unittest.skipUnless(shutil.which("bash"), "needs bash")
    def test_build_helper_gives_a_variant_another_manifest_schema_and_refuses_an_unknown_name(self):
        result = build_sh("bogus")
        self.assertEqual(result.returncode, 2)
        self.assertIn("Unknown PINNED_TARGET_CPU", result.stderr)
        for name in (None, "", "default"):
            result = build_sh(name)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn(f"Build variant: default; manifest schema: {PORTABLE_SCHEMA}; target flags: none\n",
                          result.stderr)
        seen = set()
        for name in hosted.TARGET_CPUS[1:]:
            result = build_sh(name)
            self.assertNotEqual(result.returncode, 0)
            line = re.search(r"Build variant: (\S+); manifest schema: (\S+); target flags: (.+)\n", result.stderr)
            self.assertEqual(line.group(1, 2), (name, VARIANT_SCHEMA))
            self.assertIn("-C target-cpu=", line.group(3))
            seen.add(line.group(3))
        self.assertEqual(len(seen), len(hosted.TARGET_CPUS) - 1)
        self.assertNotEqual(PORTABLE_SCHEMA, VARIANT_SCHEMA)


class HostedInputTests(unittest.TestCase):
    def setUp(self):
        self.enterContext(patch.dict(os.environ))
        os.environ.pop("PINNED_BINS", None)
        self.enterContext(patch.object(hosted.sys, "platform", "linux"))
        self.enterContext(patch.object(hosted.platform, "machine", return_value="x86_64"))
        self.enterContext(patch.object(hosted.os, "geteuid", return_value=0, create=True))
        self.enterContext(patch.object(hosted.os, "umask"))
        self.enterContext(patch.object(hosted.signal, "signal"))
        self.create_root = self.enterContext(patch.object(hosted, "create_root"))
        self.run_build = self.enterContext(patch.object(hosted, "run", return_value={"pass": True, "error": None}))

    def dispatch(self, source=hosted.SOURCE):
        self.run_build.reset_mock()
        with patch("sys.stdout", new_callable=StringIO):
            return hosted.main(["--source-commit", source, "--work-root", "/fake-build", "--wall-seconds", "60"])

    def test_the_two_experiment_sources_reach_the_build(self):
        for source in ("a561d7a3a4ea1762cf0f2ec9d8f8c2ac244e3acc", "46f7ecfec16dd0483f14865ff5a1af37fe6c3420"):
            with self.subTest(source=source):
                self.assertEqual(self.dispatch(source), 0)
                self.run_build.assert_called_once_with(Path("/fake-build"), source, 60, DEFAULT_BINS)

    def test_an_unknown_source_is_refused_before_creating_the_root(self):
        with patch("sys.stderr", new_callable=StringIO) as stderr, self.assertRaises(SystemExit) as rejected:
            self.dispatch("0" * 40)
        self.assertEqual(rejected.exception.code, 2)
        self.assertIn("invalid choice", stderr.getvalue())
        self.create_root.assert_not_called()
        self.run_build.assert_not_called()

    def test_default_and_explicit_bin_selections_reach_the_build_in_order(self):
        for value, expected in BIN_SELECTIONS:
            with self.subTest(value=value):
                if value is None:
                    os.environ.pop("PINNED_BINS", None)
                else:
                    os.environ["PINNED_BINS"] = value
                self.assertEqual(self.dispatch(), 0)
                self.run_build.assert_called_once_with(Path("/fake-build"), hosted.SOURCE, 60, expected)

    def test_invalid_bin_selections_are_refused_before_creating_the_root(self):
        for value, diagnostic in INVALID_SELECTIONS:
            with self.subTest(value=value):
                os.environ["PINNED_BINS"] = value
                with patch("sys.stderr", new_callable=StringIO) as stderr, self.assertRaises(SystemExit) as rejected:
                    self.dispatch()
                self.assertEqual(rejected.exception.code, 2)
                self.assertIn(diagnostic, stderr.getvalue())
                self.create_root.assert_not_called()
                self.run_build.assert_not_called()

    def test_the_workflow_passes_the_bin_input_through_the_environment(self):
        workflow = (HERE.parents[1] / ".github/workflows/pinned-release-build.yml").read_text(encoding="utf-8")
        self.assertRegex(workflow, r"\n      bins:\n        description: [^\n]+\n        required: false\n        type: string\n")
        self.assertIn("PINNED_BINS: ${{ inputs.bins }}", workflow)
        self.assertIn('sudo PINNED_TARGET_CPU="$PINNED_TARGET_CPU" PINNED_BINS="${PINNED_BINS:-}"', workflow)
        # The free-form value reaches the shell as an environment variable, never as script text.
        scripts = re.findall(r"run: \|\n((?:(?: {10}.*)?\n)+)", workflow)
        self.assertTrue(scripts)
        for script in scripts:
            self.assertNotIn("inputs.bins", script)

    @unittest.skipUnless(shutil.which("bash"), "needs bash")
    def test_build_helper_selects_the_default_pair_and_admits_explicit_binaries(self):
        for value, expected in BIN_SELECTIONS:
            with self.subTest(value=value):
                result = build_sh(bins=value)
                self.assertEqual(result.returncode, 1, result.stderr)
                self.assertEqual(tuple(result.stdout.splitlines()), expected)

    @unittest.skipUnless(shutil.which("bash"), "needs bash")
    def test_build_helper_refuses_unknown_duplicate_and_empty_bin_lists(self):
        for value, diagnostic in INVALID_SELECTIONS:
            with self.subTest(value=value):
                result = build_sh(bins=value)
                self.assertEqual(result.returncode, 2, result.stderr)
                self.assertIn(diagnostic.lower(), result.stderr.lower())


class BuildManifestTests(unittest.TestCase):
    def test_manifest_records_the_selected_binaries_under_their_own_keys(self):
        helper = (HERE / "build.sh").read_text(encoding="utf-8")
        writer = re.findall(r"<<'PY'\n(.*?)\nPY\n", helper, re.S)[-1]
        root = Path(self.enterContext(tempfile.TemporaryDirectory())).resolve()
        (root / "Cargo.lock").touch()
        (root / "ld").touch()

        def check_output(args, *, text):
            if args == ("gcc", "-print-prog-name=ld"):
                return str(root / "ld")
            if args[0] == "readelf":
                return "Advanced Micro Devices X86-64\n/lib64/ld-linux-x86-64.so.2"
            return "fixture output"

        for selected, keys in ((DEFAULT_BINS, ("trainer", "checkpoint_container")),
                               (("regret_census_v1",), ("regret_census",)),
                               (("checkpoint_container_v1", "regret_census_v1"),
                                ("checkpoint_container", "regret_census"))):
            with self.subTest(bins=selected):
                out = root / "-".join(keys)
                out.mkdir()
                for name in (*selected, "rust-components.json", "debian-packages.tsv"):
                    (out / name).write_bytes(name.encode())
                argv = ["-", str(root), str(out), hosted.SOURCE, "fixture-origin", "1", *selected]
                with (patch.object(sys, "argv", argv), patch.object(subprocess, "check_output", check_output),
                      patch.dict(os.environ, PINNED_MANIFEST_SCHEMA=PORTABLE_SCHEMA, PINNED_RUSTFLAGS="")):
                    exec(compile(writer, "build.sh manifest writer", "exec"), {})
                manifest = json.loads((out / "BUILD-MANIFEST.json").read_bytes())
                self.assertEqual(manifest["schema"], PORTABLE_SCHEMA)
                self.assertEqual(manifest["build"]["binaries"], list(selected))
                binary_keys = ("trainer", "checkpoint_container", "regret_census")
                self.assertEqual(tuple(key for key in manifest if key in binary_keys), keys)
                self.assertEqual(tuple(manifest[key]["name"] for key in keys), selected)


class HostedGuardTests(unittest.TestCase):
    def setUp(self):
        self.parent = Path(self.enterContext(tempfile.TemporaryDirectory())).resolve()
        self.root = self.parent / "work-root"
        hosted.create_root(self.root)
        self.enterContext(patch.object(hosted, "mem_available", return_value=14 * hosted.GIB))
        self.original_swap = {"type": "partition", "size_bytes": 8 * hosted.GIB, "used_bytes": 0, "priority": 5}
        self.swaps = {"/dev/previous": self.original_swap}
        self.enterContext(patch.object(hosted, "active_swaps", side_effect=lambda: dict(self.swaps)))

    def test_existing_root_is_refused(self):
        with self.assertRaises(FileExistsError):
            hosted.create_root(self.root)

    def test_capacity_refusal_records_resources_before_starting_docker(self):
        free = hosted.FREE_FLOOR + hosted.TOTAL_GROWTH - 1
        with (patch.object(hosted.shutil, "disk_usage", return_value=SimpleNamespace(free=free)),
              patch.object(hosted, "allocated_bytes", return_value=0), patch.object(hosted, "start") as start,
              patch.object(hosted, "swap_command") as swap):
            receipt = hosted.run(self.root, hosted.SOURCE, 1)
        start.assert_not_called()
        swap.assert_not_called()
        self.assertFalse(receipt["pass"])
        self.assertIn("81.5 GiB", receipt["error"])
        self.assertEqual(receipt["resources"]["initial_free_bytes"], free)
        self.assertEqual(receipt["swap"]["original"], self.swaps)
        self.assertEqual(json.loads((self.root / "guard.json").read_bytes()), receipt)

    def test_the_variant_reaches_the_container_and_the_receipt(self):
        with patch.object(hosted, "TARGET_CPU", "znver5"):
            self.exercise_cleanup(growth_stop=True, target_cpu="znver5")

    def test_the_selected_bins_reach_the_container_and_the_written_receipt(self):
        self.exercise_cleanup(bins=("regret_census_v1",))

    def exercise_cleanup(self, growth_stop=False, swapoff_error=False, target_cpu="default", bins=DEFAULT_BINS):
        free = 85 * hosted.GIB
        daemon = Mock(pid=101, returncode=None)
        daemon.poll.return_value = None
        builder = Mock(pid=102, returncode=None if growth_stop else 101)
        builder.poll.return_value = builder.returncode
        daemon.wait.side_effect = lambda **_: setattr(daemon.poll, "return_value", 0)
        events = []
        commands = []
        running = growth_stop

        def start(command, log, env):
            commands.append((command, env))
            log.write_text("retained log\n", encoding="ascii")
            return daemon if len(commands) == 1 else builder

        def swap(command, path, _log):
            self.assertEqual(path, self.root / "swapfile")
            events.append(command[0])
            if command[0] == "fallocate":
                path.write_bytes(b"x" * hosted.SWAP)
            elif command[0] == "swapon":
                self.swaps[str(path)] = {"type": "file", "size_bytes": hosted.SWAP, "used_bytes": 0, "priority": -2}
            elif command[0] == "swapoff":
                if swapoff_error:
                    raise OSError("swapoff failed")
                del self.swaps[str(path)]

        def docker(command, **_):
            nonlocal running
            self.assertEqual(command[:5], ["/usr/bin/docker", "--config", str(self.root / "client"), "--host",
                                            f"unix://{self.root / 'docker.sock'}"])
            self.assertEqual(command[-1], hosted.CONTAINER)
            action = command[6]
            events.append(action)
            if action == "stop":
                running = False
            state = {"Running": running, "OOMKilled": not growth_stop, "ExitCode": 0 if running else 101}
            return subprocess.CompletedProcess(command, 0, json.dumps(state).encode(), b"")

        with (patch.object(hosted.shutil, "disk_usage", return_value=SimpleNamespace(free=free)),
              patch.object(hosted, "SWAP", 16), patch.object(hosted, "TOTAL_GROWTH", hosted.GROWTH + 16),
              patch.object(hosted, "file_blocks", side_effect=lambda path: path.stat().st_size if path.exists() else 0),
              patch.object(hosted, "allocated_bytes", side_effect=[0, 0, 16, 16, 16,
                                                                   hosted.GROWTH + 17 if growth_stop else 16]),
              patch.object(Path, "is_socket", return_value=True),
              patch.object(hosted, "executable", side_effect=lambda name: f"/usr/bin/{name}"),
              patch.object(hosted, "start", side_effect=start), patch.object(hosted, "swap_command", side_effect=swap),
              patch.object(hosted.subprocess, "run", side_effect=docker),
              patch.object(hosted.signal, "SIGKILL", 9, create=True),
              patch.object(hosted.os, "killpg", side_effect=lambda pid, sig: events.append((pid, sig)), create=True)):
            receipt = hosted.run(self.root, hosted.SOURCE, 60, bins)

        self.assertEqual(commands[0][1]["DOCKER_TMPDIR"], str(self.root / "tmp"))
        self.assertEqual(commands[0][1]["TMPDIR"], str(self.root / "tmp"))
        self.assertIn("--bridge=none", commands[0][0])
        self.assertIn("--iptables=false", commands[0][0])
        self.assertIn(f"type=bind,src={self.root / 'work'},dst=/work", commands[1][0])
        self.assertIn("--memory-swap=28g", commands[1][0])
        self.assertNotIn("--rm", commands[1][0])
        self.assertEqual(commands[1][0][-4:], [hosted.IMAGE, "/build.sh", hosted.SOURCE, "/work/out"])
        # The target-flag variant reaches build.sh through the container's environment; the default is `default`.
        position = commands[1][0].index("PINNED_TARGET_CPU=" + target_cpu)
        self.assertEqual(commands[1][0][position - 1], "--env")
        self.assertEqual([item for item in commands[1][0] if item.startswith("PINNED_TARGET_CPU=")],
                         ["PINNED_TARGET_CPU=" + target_cpu])
        self.assertEqual(receipt["target_cpu"], target_cpu)
        bins_options = [item for item in commands[1][0] if item.startswith("PINNED_BINS=")]
        written_receipt = json.loads((self.root / "guard.json").read_bytes())
        if bins == DEFAULT_BINS:
            self.assertEqual(bins_options, [])
            self.assertNotIn("binaries", written_receipt)
        else:
            self.assertEqual(bins_options, ["PINNED_BINS=" + " ".join(bins)])
            self.assertEqual(commands[1][0][commands[1][0].index(bins_options[0]) - 1], "--env")
            self.assertEqual(written_receipt["binaries"], list(bins))
        self.assertFalse(receipt["pass"])
        self.assertTrue((self.root / "daemon.log").is_file())
        self.assertTrue((self.root / "container.log").is_file())
        self.assertTrue((self.root / "swapfile").is_file())
        self.assertEqual(receipt["swap"]["original"], {"/dev/previous": self.original_swap})
        self.assertEqual(receipt["swap"]["after_cleanup"]["/dev/previous"], self.original_swap)
        return events, receipt

    def test_owned_growth_stop_keeps_logs_and_disables_only_owned_swap_after_docker(self):
        events, receipt = self.exercise_cleanup(growth_stop=True)
        self.assertEqual(events, ["fallocate", "mkswap", "swapon", (102, signal.SIGTERM), "inspect", "stop", "inspect",
                                  "rm", (101, signal.SIGTERM), "swapoff"])
        self.assertIn("growth exceeded", receipt["error"])
        self.assertEqual(receipt["resources"]["maximum_owned_build_allocated_bytes"], hosted.GROWTH + 1)
        self.assertNotIn(str(self.root / "swapfile"), receipt["swap"]["after_cleanup"])

    def test_compiler_exit_captures_oom_before_removal_and_records_swapoff_failure(self):
        events, receipt = self.exercise_cleanup(swapoff_error=True)
        self.assertEqual(events, ["fallocate", "mkswap", "swapon", "inspect", "rm", (101, signal.SIGTERM), "swapoff"])
        self.assertTrue(receipt["container"]["before_cleanup"]["OOMKilled"])
        self.assertEqual(receipt["build_exit_code"], 101)
        self.assertIn("owned swapoff: swapoff failed", receipt["cleanup_errors"])
        self.assertIn(str(self.root / "swapfile"), receipt["swap"]["after_cleanup"])

    def test_container_that_was_never_created_needs_no_stop_or_removal(self):
        docker = ["/usr/bin/docker", "--host", f"unix://{self.root / 'docker.sock'}"]
        log = self.root / "container.log"
        diagnostic = f"Error response from daemon: No such container: {hosted.CONTAINER}".encode()
        receipt = {}
        with patch.object(hosted.subprocess, "run",
                          return_value=subprocess.CompletedProcess(docker, 1, b"", diagnostic)) as invoke:
            hosted.remove_container(docker, {}, log, receipt)
        invoke.assert_called_once()
        self.assertEqual(receipt, {"before_cleanup": None})
        self.assertEqual(log.read_bytes(), diagnostic)
