"""Start a fresh, locally journaled native human match in the background."""
from __future__ import annotations

import argparse
import json
import secrets
import subprocess
import sys
import time
import uuid
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--template", required=True, type=Path)
    parser.add_argument("--engine", required=True, type=Path)
    parser.add_argument("--sessions", required=True, type=Path)
    parser.add_argument("--card-text", type=Path)
    parser.add_argument("--human-seat", type=int, choices=(0, 1), default=0)
    parser.add_argument("--starting-player", type=int, choices=(0, 1), default=0)
    parser.add_argument("--seed", type=int)
    args = parser.parse_args()
    config = json.loads(args.template.read_text(encoding="utf-8"))
    if not args.engine.is_file():
        parser.error("The native match executable is missing.")
    folder = args.sessions.resolve() / (time.strftime("%Y%m%d-%H%M%S") + "-" + uuid.uuid4().hex[:8])
    folder.mkdir(parents=True, exist_ok=False)
    config.update(human_seat=args.human_seat, starting_player=args.starting_player,
                  seed=args.seed if args.seed is not None else secrets.randbits(63),
                  journal_path=str(folder / "journal.jsonl"))
    config_path = folder / "config.json"
    config_path.write_text(json.dumps(config, indent=2), encoding="utf-8")
    runtime = folder / "runtime"
    command = [sys.executable, "-B", str(Path(__file__).with_name("server.py")),
               "--engine", str(args.engine.resolve()), "--config", str(config_path),
               "--runtime-dir", str(runtime)]
    if args.card_text:
        command += ["--card-text", str(args.card_text.resolve())]
    with (folder / "server-stdout.log").open("xb") as output, (folder / "server-stderr.log").open("xb") as error:
        process = subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=output, stderr=error,
                                   creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
    for _ in range(150):
        if process.poll() is not None:
            raise RuntimeError(f"The local interface did not start. See {folder / 'server-stderr.log'}")
        try:
            metadata = json.loads((runtime / "server.json").read_text(encoding="utf-8"))
        except (FileNotFoundError, json.JSONDecodeError):
            time.sleep(0.1)
            continue
        print(json.dumps({"url": metadata["url"], "session_dir": str(folder)}, indent=2))
        return
    # Preserve the potentially running process and its startup logs on a slow host.
    raise RuntimeError(f"Startup is still pending. Inspect {folder} before launching another match.")


if __name__ == "__main__":
    main()
