"""Exercise admission and owned-process cleanup without invoking Docker."""

import json
from pathlib import Path
import signal
import subprocess
import tempfile
import unittest
from types import SimpleNamespace
from unittest.mock import Mock, patch

import hosted


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

    def exercise_cleanup(self, *, growth_stop=False, swapoff_error=False):
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
