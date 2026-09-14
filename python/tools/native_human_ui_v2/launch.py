"""Start a fresh complete-agent human session using the existing browser UI."""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import secrets
import subprocess
import sys
import time
import uuid

CONFIG_SCHEMA = "mtg-kernel-human-match-config/v2"


def strict_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("Duplicate JSON key")
        result[key] = value
    return result


def read_json(path, limit):
    with path.open("rb") as stream:
        raw = stream.read(limit + 1)
    if len(raw) > limit:
        raise ValueError("Configuration or package exceeds its size limit")
    return raw, json.loads(raw, object_pairs_hook=strict_object)


def prepare_config(template, engine, *, human_seat, initial_chooser, seed, journal):
    _, config = read_json(template, 1024 * 1024)
    if config.get("schema") != CONFIG_SCHEMA:
        raise ValueError("The V2 launcher requires a complete-agent human V2 template")
    package_pin = config["package"]
    package_path = Path(package_pin["path"])
    if not package_path.is_absolute():
        raise ValueError("The package path must be absolute")
    raw, package = read_json(package_path, 1024 * 1024)
    if hashlib.sha256(raw).hexdigest() != package_pin["sha256"]:
        raise ValueError("The selected package differs from its pin")
    with engine.open("rb") as stream:
        engine_sha = hashlib.file_digest(stream, "sha256").hexdigest()
    if engine_sha != package["runtime"]["executable"]["sha256"]:
        raise ValueError("The selected engine differs from the package runtime")
    if human_seat not in (0, 1) or initial_chooser not in (0, 1) or not 0 <= seed < 2**64:
        raise ValueError("Invalid human seat, initial chooser or seed")
    config.update(human_seat=human_seat, initial_chooser=initial_chooser,
                  seed=seed, journal_path=str(journal))
    return config


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--template", required=True, type=Path)
    parser.add_argument("--engine", required=True, type=Path)
    parser.add_argument("--sessions", required=True, type=Path)
    parser.add_argument("--card-text", type=Path)
    parser.add_argument("--human-seat", type=int, choices=(0, 1), default=0)
    parser.add_argument("--initial-chooser", type=int, choices=(0, 1), default=0)
    parser.add_argument("--seed", type=int)
    args = parser.parse_args()
    folder = args.sessions.resolve() / (time.strftime("%Y%m%d-%H%M%S") + "-" + uuid.uuid4().hex[:8])
    config = prepare_config(args.template.resolve(), args.engine.resolve(),
                            human_seat=args.human_seat, initial_chooser=args.initial_chooser,
                            seed=args.seed if args.seed is not None else secrets.randbits(64),
                            journal=folder / "journal.jsonl")
    folder.mkdir(parents=True, exist_ok=False)
    config_path = folder / "config.json"
    with config_path.open("x", encoding="utf-8") as stream:
        json.dump(config, stream, indent=2, allow_nan=False)
        stream.write("\n")
    runtime = folder / "runtime"
    server = Path(__file__).resolve().parents[1] / "native_human_ui_v1" / "server.py"
    command = [sys.executable, "-B", str(server), "--engine", str(args.engine.resolve()),
               "--config", str(config_path), "--runtime-dir", str(runtime)]
    if args.card_text:
        command += ["--card-text", str(args.card_text.resolve())]
    with (folder / "server-stdout.log").open("xb") as output, (folder / "server-stderr.log").open("xb") as error:
        process = subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=output, stderr=error,
                                   creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0)
                                   | getattr(subprocess, "BELOW_NORMAL_PRIORITY_CLASS", 0))
    for _ in range(150):
        if process.poll() is not None:
            raise RuntimeError(f"The interface did not start. See {folder / 'server-stderr.log'}")
        try:
            metadata = json.loads((runtime / "server.json").read_text(encoding="utf-8"))
        except (FileNotFoundError, json.JSONDecodeError):
            time.sleep(0.1)
            continue
        print(json.dumps({"url": metadata["url"], "session_dir": str(folder),
                          "status": "browser server started; model validation occurs on first view"}, indent=2))
        return
    raise RuntimeError(f"Startup is still pending. Inspect {folder} before launching another match.")


if __name__ == "__main__":
    main()
