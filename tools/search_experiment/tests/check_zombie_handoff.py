"""One Linux correctness check for non-child exit and stopped lease handoff."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time

import psutil

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import launch


def check():
    assert sys.platform == "linux"
    with tempfile.TemporaryDirectory(prefix="search-zombie-handoff-") as folder:
        root = Path(folder)
        state = root / "state"
        state.mkdir()
        controller = root / "build_driver_correctness.py"
        controller.write_text(
            "import pathlib,time\n"
            f"marker=pathlib.Path({str(root / 'PROGRESS-HANDOFF')!r})\n"
            "end=time.monotonic()+10\n"
            "while not marker.exists() and time.monotonic()<end: time.sleep(.01)\n"
            "time.sleep(.05)\n"
        )
        holder_code = (
            "import pathlib,subprocess,sys,time\n"
            f"child=subprocess.Popen([sys.executable,{str(controller)!r}])\n"
            "print(child.pid,flush=True)\n"
            f"stop=pathlib.Path({str(root / 'REAP')!r})\n"
            "end=time.monotonic()+20\n"
            "while not stop.exists() and time.monotonic()<end: time.sleep(.01)\n"
            "if child.poll() is None: child.terminate()\n"
            "child.wait(timeout=3)\n"
        )
        holder = subprocess.Popen([sys.executable, "-c", holder_code], stdout=subprocess.PIPE, text=True)
        try:
            pid = int(holder.stdout.readline())
            assert psutil.Process(pid).ppid() == holder.pid != os.getpid()
            lease = {"state": str(state), "pod_id": "synthetic-correctness-only",
                     "name": "synthetic-correctness-only", "deadline_epoch": time.time() + 3600,
                     "recovery_seconds": 600}
            spec = {"root": str(root), "lease_progress": lease}
            guard = dict(lease, epoch=time.time(), allow_new_dispatch=True, latched=None)
            launch.publish(state / "guard.json", guard)

            def progress():
                launch.publish(state / "progress.json", {"controller_pid": pid, "pod_id": lease["pod_id"],
                               "last_productive_epoch": time.time(), "last_activity_epoch": time.time()})

            progress()
            started = time.monotonic()
            launch.lease_progress_takeover(spec)
            live_exit_seconds = time.monotonic() - started
            assert psutil.Process(pid).status() == psutil.STATUS_ZOMBIE
            progress()
            started = time.monotonic()
            launch.lease_progress_takeover(spec)
            zombie_seconds = time.monotonic() - started
            assert zombie_seconds < 1
            before = (state / "progress.json").read_bytes()
            (root / "PROGRESS-HANDOFF").unlink()
            launch.publish(state / "guard.json", dict(guard, latched="controller_lost"))
            try:
                launch.lease_progress_takeover(spec)
            except ValueError:
                pass
            else:
                raise AssertionError("latched guard accepted handoff")
            assert (state / "progress.json").read_bytes() == before
            assert not (root / "PROGRESS-HANDOFF").exists()
            return {"passed": True, "live_nonchild_exit_to_zombie_seconds": live_exit_seconds,
                    "already_exited_nonchild_zombie_seconds": zombie_seconds,
                    "latched_guard_refused_before_progress_write": True,
                    "launch_sha256": hashlib.sha256(Path(launch.__file__).read_bytes()).hexdigest(),
                    "python": sys.version, "psutil": psutil.__version__}
        finally:
            (root / "REAP").touch()
            holder.wait(timeout=4)
            holder.stdout.close()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", required=True)
    args = parser.parse_args()
    result = check()
    Path(args.out).write_text(json.dumps(result, indent=1))
    print(json.dumps(result))
