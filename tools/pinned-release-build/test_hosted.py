"""Exercise admission and owned-process cleanup without invoking Docker."""

import json
from pathlib import Path
import signal
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

    def test_existing_root_is_refused(self):
        with self.assertRaises(FileExistsError):
            hosted.create_root(self.root)

    def test_capacity_refusal_records_resources_before_starting_docker(self):
        free = hosted.FREE_FLOOR + hosted.GROWTH - 1
        with (patch.object(hosted.shutil, "disk_usage", return_value=SimpleNamespace(free=free)),
              patch.object(hosted, "allocated_bytes", return_value=0), patch.object(hosted, "start") as start):
            receipt = hosted.run(self.root, hosted.SOURCE, 1)
        start.assert_not_called()
        self.assertFalse(receipt["pass"])
        self.assertIn("65.5 GiB", receipt["error"])
        self.assertEqual(receipt["resources"]["initial_free_bytes"], free)
        self.assertEqual(json.loads((self.root / "guard.json").read_bytes()), receipt)

    def test_owned_growth_stop_keeps_logs_and_stops_private_container_before_daemon(self):
        free = 70 * hosted.GIB
        daemon = Mock(pid=101, returncode=None)
        daemon.poll.return_value = None
        builder = Mock(pid=102, returncode=None)
        builder.poll.return_value = None
        events = []
        commands = []

        def start(command, log, env):
            commands.append((command, env))
            log.write_text("retained log\n", encoding="ascii")
            return daemon if len(commands) == 1 else builder

        def stop(command, **_):
            events.append(command)

        with (patch.object(hosted.shutil, "disk_usage", return_value=SimpleNamespace(free=free)),
              patch.object(hosted, "allocated_bytes", side_effect=[0, 0, 0, hosted.GROWTH + 1]),
              patch.object(Path, "is_socket", return_value=True),
              patch.object(hosted, "executable", side_effect=lambda name: f"/usr/bin/{name}"),
              patch.object(hosted, "start", side_effect=start), patch.object(hosted.subprocess, "run", side_effect=stop),
              patch.object(hosted.signal, "SIGKILL", 9, create=True),
              patch.object(hosted.os, "killpg", side_effect=lambda pid, sig: events.append((pid, sig)), create=True)):
            receipt = hosted.run(self.root, hosted.SOURCE, 60)

        docker = ["/usr/bin/docker", "--config", str(self.root / "client"), "--host", f"unix://{self.root / 'docker.sock'}"]
        self.assertEqual(events, [(102, signal.SIGTERM), [*docker, "stop", "--time", "3", hosted.CONTAINER],
                                  (101, signal.SIGTERM)])
        self.assertEqual(commands[0][1]["DOCKER_TMPDIR"], str(self.root / "tmp"))
        self.assertEqual(commands[0][1]["TMPDIR"], str(self.root / "tmp"))
        self.assertIn("--bridge=none", commands[0][0])
        self.assertIn("--iptables=false", commands[0][0])
        self.assertIn(f"type=bind,src={self.root / 'work'},dst=/work", commands[1][0])
        self.assertIn("--memory-swap=12g", commands[1][0])
        self.assertEqual(commands[1][0][-4:], [hosted.IMAGE, "/build.sh", hosted.SOURCE, "/work/out"])
        self.assertFalse(receipt["pass"])
        self.assertIn("growth exceeded", receipt["error"])
        self.assertEqual(receipt["resources"]["maximum_owned_allocated_bytes"], hosted.GROWTH + 1)
        self.assertTrue((self.root / "daemon.log").is_file())
        self.assertTrue((self.root / "container.log").is_file())
