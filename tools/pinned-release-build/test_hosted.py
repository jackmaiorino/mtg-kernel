"""Exercise admission and owned-process cleanup without invoking Docker."""

import json
import os
from pathlib import Path
import re
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


def import_hosted(value):
    """Import hosted.py in a fresh interpreter with PINNED_TARGET_CPU set to `value` (None: unset)."""
    env = {key: item for key, item in os.environ.items() if key != "PINNED_TARGET_CPU"}
    if value is not None:
        env["PINNED_TARGET_CPU"] = value
    return subprocess.run([sys.executable, "-B", "-c", "import hosted; print(hosted.TARGET_CPU)"], cwd=HERE, env=env,
                          capture_output=True, text=True, timeout=60)


def build_sh(value):
    """Run build.sh as far as its first privilege check with PINNED_TARGET_CPU set to `value` (None: unset)."""
    env = {key: item for key, item in os.environ.items() if key != "PINNED_TARGET_CPU"}
    if value is not None:
        env["PINNED_TARGET_CPU"] = value
    return subprocess.run(["bash", "build.sh", "0" * 40, "/work/out"], cwd=HERE, env=env, capture_output=True,
                          text=True, timeout=60)


class TargetVariantTests(unittest.TestCase):
    def test_the_environment_selects_the_variant_and_an_unknown_name_is_refused(self):
        self.assertEqual(import_hosted(None).stdout.strip(), "default")
        for name in hosted.TARGET_CPUS:
            self.assertEqual(import_hosted(name).stdout.strip(), name)
        for name in ("", "bogus", "znver5 -C x", "ZNVER5"):
            result = import_hosted(name)
            self.assertNotEqual(result.returncode, 0, name)
            self.assertIn("unknown PINNED_TARGET_CPU", result.stderr)

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

    @unittest.skipIf(os.name == "nt" or (hasattr(os, "geteuid") and os.geteuid() == 0),
                     "runs build.sh up to its root check: needs bash and a user that is not root")
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

    def exercise_cleanup(self, growth_stop=False, swapoff_error=False, target_cpu="default"):
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
            receipt = hosted.run(self.root, hosted.SOURCE, 60)

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
