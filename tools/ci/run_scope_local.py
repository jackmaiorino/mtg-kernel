"""Run one CI scope's checks on a local machine (for example HaleysPC) at an exact head and print a record for the
pull request: command, head, result and minutes.

usage: python tools/ci/run_scope_local.py SCOPE --head SHA [--checkout DIR] [--record FILE]
The checkout (default: this repository) must be a directory you own; it is moved to the detached head with
`git fetch origin SHA` and `git checkout --detach SHA`. The commands come from tools/ci/ci_scope.py (the same
table the workflow uses), read from the checked-out head itself. Start it through the machine's own host
reservation or claim tool at low priority; this script does not take one.
"""
from __future__ import annotations

import argparse
import json
import platform
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path


GIT_BASH = Path("C:/Program Files/Git/bin/bash.exe")


def run(command: list[str] | str, cwd: Path, shell: bool = False) -> subprocess.CompletedProcess:
    """A shell command runs in bash (Git Bash on Windows), as the workflow runs it."""
    if shell:
        bash = str(GIT_BASH) if platform.system() == "Windows" else "bash"
        return subprocess.run([bash, "-e", "-o", "pipefail", "-c", command], cwd=cwd, text=True)
    return subprocess.run(command, cwd=cwd, text=True)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("scope")
    parser.add_argument("--head", required=True)
    parser.add_argument("--checkout", default=str(Path(__file__).resolve().parents[2]))
    parser.add_argument("--record")
    parser.add_argument("--no-uv", action="store_true",
                        help="run `uv run --no-sync python` commands with this Python instead (machines without uv)")
    args = parser.parse_args(argv)
    repo = Path(args.checkout)
    dirty = subprocess.run(["git", "status", "--porcelain", "--untracked-files=all"], cwd=repo,
                           capture_output=True, text=True).stdout.strip()
    if dirty:
        print(f"{repo} has local changes or untracked files; use a clean checkout of your own", file=sys.stderr)
        return 2
    for command in (["git", "fetch", "--quiet", "origin", args.head], ["git", "checkout", "--quiet", "--detach", args.head]):
        if run(command, repo).returncode:
            print(f"cannot move {repo} to {args.head}", file=sys.stderr)
            return 2
    head = subprocess.run(["git", "rev-parse", "HEAD"], cwd=repo, capture_output=True, text=True).stdout.strip()
    if not head.startswith(args.head) and args.head not in head:
        print(f"{repo} is at {head}, not {args.head}", file=sys.stderr)
        return 2
    listed = subprocess.run([sys.executable, "tools/ci/ci_scope.py", "commands", args.scope], cwd=repo,
                            capture_output=True, text=True)
    if listed.returncode:
        print(listed.stderr, file=sys.stderr)
        return 2
    rows, started = [], time.monotonic()
    commands = [line for line in listed.stdout.splitlines() if line.strip()]
    python = Path(sys.executable).as_posix()
    if args.no_uv:
        commands = [c.replace("uv run --no-sync python", f'"{python}"') for c in commands]
    # Plain `python` commands use this interpreter (CI's is the pinned setup-python one).
    commands = [f'"{python}"' + c[len("python"):] if c.startswith("python ") else c for c in commands]
    for command in commands:
        t0 = time.monotonic()
        code = run(command, repo, shell=True).returncode
        rows.append({"command": command, "exit": code, "minutes": round((time.monotonic() - t0) / 60, 1)})
    record = {"scope": args.scope, "head": head, "host": platform.node(), "os": platform.system(),
              "python": platform.python_version(), "uv": not args.no_uv,
              "finished": datetime.now(timezone.utc).isoformat(timespec="seconds"),
              "minutes": round((time.monotonic() - started) / 60, 1),
              "result": "pass" if all(r["exit"] == 0 for r in rows) else "fail", "commands": rows}
    if "trainer" in args.scope:
        sys.path.insert(0, str(repo / "tools" / "ci"))
        import ci_scope  # noqa: PLC0415 - from the checked-out head
        record["note"] = ci_scope.trainer_record()
    text = json.dumps(record, indent=1)
    print(text)
    print(f"\nFor the pull request: scope `{args.scope}` at `{head[:9]}` on {record['host']} ({record['os']}): "
          f"**{record['result']}** in {record['minutes']} min; commands: "
          + "; ".join(f"`{r['command']}` exit {r['exit']} ({r['minutes']} min)" for r in rows))
    if args.record:
        Path(args.record).write_text(text, encoding="utf-8")
    return 0 if record["result"] == "pass" else 1


if __name__ == "__main__":
    sys.exit(main())
