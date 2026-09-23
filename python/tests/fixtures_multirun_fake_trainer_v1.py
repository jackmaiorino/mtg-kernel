"""Stand-in training executable for the multirun launcher tests.

Writes one output per completed update whose bytes depend only on the seed and
update index, so repeated executions are byte-identical. Two opt-in defects
exercise the launcher's refusals: FAKE_TRAINER_NOISE makes every execution
differ, and FAKE_TRAINER_CONTENTION makes outputs differ whenever another
instance is running at the same time.
"""

import argparse
import hashlib
import os
from pathlib import Path
import time


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--seed", type=int, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--updates", type=int, required=True)
    parser.add_argument("--stop-after", type=int, required=True)
    parser.add_argument("--update-seconds", type=float, default=0.2)
    parser.add_argument("--fail", action="store_true")
    arguments = parser.parse_args()

    arguments.out.mkdir(parents=True, exist_ok=True)
    marker = arguments.out.parent.parent / f"active-{os.getpid()}"
    marker.touch()
    try:
        (arguments.out / "run.json").write_text(f'{{"seed": {arguments.seed}, "updates": {arguments.updates}}}\n')
        state = hashlib.sha256(str(arguments.seed).encode()).digest()
        for update in range(1, arguments.stop_after + 1):
            time.sleep(arguments.update_seconds)
            state = hashlib.sha256(state + update.to_bytes(8, "big")).digest()
            payload = state
            if os.environ.get("FAKE_TRAINER_NOISE"):
                payload += os.urandom(8)
            if os.environ.get("FAKE_TRAINER_CONTENTION") and len(list(marker.parent.glob("active-*"))) > 1:
                payload += b"contended"
            (arguments.out / f"update-{update:08}.state.bin").write_bytes(payload)
        (arguments.out / "latest.txt").write_text(f"{arguments.stop_after}\n")
    finally:
        marker.unlink()
    if arguments.fail:
        raise SystemExit(7)


if __name__ == "__main__":
    main()
