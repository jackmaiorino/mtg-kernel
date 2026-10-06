"""Run the workspace suite and its snapshot timing gate without contention."""

from __future__ import annotations

import json
from pathlib import Path
import subprocess
import sys


TIMING_TEST = "snapshot::tests::snapshot_clone_cost_is_bounded"


def main() -> int:
    if len(sys.argv) > 1 and sys.argv[1] == "--run":
        executable, *arguments = sys.argv[2:]
        if Path(executable).name.startswith("mtg_kernel-"):
            if arguments:
                raise ValueError("Workspace library runner expects no test arguments")
            result = subprocess.run([executable, "--skip", TIMING_TEST], check=False)
            if result.returncode:
                return result.returncode
            print(f"Running isolated timing gate: {TIMING_TEST}", flush=True)
            # Keep the short wall-clock measurement ahead of background
            # Windows services. Correctness tests retain normal priority.
            timing_options = ({"creationflags": subprocess.HIGH_PRIORITY_CLASS}
                              if sys.platform == "win32" else {})
            return subprocess.run([
                executable, TIMING_TEST, "--exact", "--include-ignored", "--test-threads=1", "--nocapture",
            ], check=False, **timing_options).returncode
        return subprocess.run([executable, *arguments], check=False).returncode

    if sys.argv[1:] not in ([], ["--no-fail-fast"]):
        raise ValueError("Workspace suite runner accepts only --no-fail-fast")
    runner = [sys.executable, str(Path(__file__).resolve()), "--run"]
    configuration = "target.'cfg(all())'.runner=" + json.dumps(runner)
    return subprocess.run([
        "cargo", "test", "--release", "--locked", "--config", configuration,
        "--workspace", "--all-targets", *sys.argv[1:],
    ], check=False).returncode


if __name__ == "__main__":
    raise SystemExit(main())
