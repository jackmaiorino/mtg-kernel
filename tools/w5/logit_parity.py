"""W5 Linux-to-Windows logit parity (collab LANES/lead/W5-DESIGN.md v1.3 section 5 and row E12).

Every Linux-trained endpoint, net-8 and S, carries a parity receipt for the Windows host class that rates it and
runs its H looks: the maximum absolute logit difference between the Linux and the Windows forward is at most 1e-5
on at least 10,000 fixed states, and the argmax is the same except where the top two logits are within 1e-5.

The fixed states are the focal decisions of recorded games (label-replay jobs, as `tools/spy_reset/panel.py jobs`
writes them). Each host replays them with the endpoint through `regret_census_v1 mode=s4a-tensor-replay` (the
recorded actions for both seats; the loaded policy scores every focal decision and records its V4 input tensor,
logits and value in TD_BIN), so both hosts score the same states. S endpoints use the same contract through the
E3 binary (`--command`, a JSON argv template with {source}, {out}; LABEL_JOBS and TD_BIN are set).

  logit_parity.py states --jobs JOBS.jsonl [--jobs ...] [--min-states 10000] --out FIXED.jsonl
  logit_parity.py dump --binary CENSUS --source SRC.json --pin CHECKPOINT [--pin ...] --jobs FIXED.jsonl --out DIR
                       [--command JSON]
  logit_parity.py compare --linux DIR --windows DIR [--tolerance 1e-5] --out RECEIPT.json

A state is a scored focal decision with at least two legal actions. `compare` refuses different jobs, pinned
checkpoint bytes or decision sequences; it reports whether the input tensors are byte-identical and the value
difference beside the logit rule.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import re
import struct
import subprocess
import sys
from pathlib import Path

DUMP_SCHEMA = "w5-logit-dump/v1"
RECEIPT_SCHEMA = "w5-logit-parity/v1"
MIN_STATES = 10_000
TOLERANCE = 1e-5
HEADER = struct.Struct("<8I")


class ParityError(Exception):
    pass


def require(ok: bool, message: str) -> None:
    if not ok:
        raise ParityError(message)


def sha256_file(path) -> str:
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for block in iter(lambda: handle.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def host_class() -> str:
    """`windows-<cpu>` or the W5 launcher's Linux class (`linux-zen5-9950x` on the nodes)."""
    if sys.platform.startswith("linux"):
        sys.path.insert(0, str(Path(__file__).resolve().parent))
        import w5box
        return w5box.host_class()
    model = platform.processor() or "unknown"
    if sys.platform == "win32":
        try:
            import winreg
            with winreg.OpenKey(winreg.HKEY_LOCAL_MACHINE, r"HARDWARE\DESCRIPTION\System\CentralProcessor\0") as key:
                model = winreg.QueryValueEx(key, "ProcessorNameString")[0]
        except OSError:
            pass
        return "windows-" + re.sub(r"[^a-z0-9]+", "-", model.lower()).strip("-")
    return sys.platform + "-" + re.sub(r"[^a-z0-9]+", "-", model.lower()).strip("-")


# ---------------------------------------------------------------- the fixed states

def scored_states(job: dict) -> int:
    return sum(1 for d in job["decisions"] if d["actor"] == job["focal"] and d["legal_action_count"] >= 2)


def fixed_states(paths: list[Path], minimum: int) -> tuple[list[dict], int]:
    """Jobs in id order until the scored states reach `minimum` (outcome-blind: ids and counts only)."""
    jobs = {}
    for path in paths:
        for line in Path(path).read_text(encoding="utf-8").splitlines():
            if line.strip():
                job = json.loads(line)
                require(jobs.get(job["id"], job) == job, f"job {job['id']} appears with two contents")
                jobs[job["id"]] = job
    chosen, states = [], 0
    for job_id in sorted(jobs):
        if states >= minimum:
            break
        chosen.append(jobs[job_id])
        states += scored_states(jobs[job_id])
    require(states >= minimum, f"only {states} scored states in the jobs; {minimum} required")
    return chosen, states


# ---------------------------------------------------------------- one host's dump

def decode(block: bytes) -> tuple[bytes, list[float], float, int]:
    """One TD_BIN decision record: (input bytes, logits, value, legal); see stage4a/tensor_dump.rs."""
    objects, edges, actions, refs, state_len, selected, legal, actor = HEADER.unpack_from(block, 0)
    tail = 4 * (actions + 1)
    require(len(block) > HEADER.size + tail and actions == legal, "malformed tensor record")
    logits = list(struct.unpack_from(f"<{actions}f", block, len(block) - tail))
    value = struct.unpack_from("<f", block, len(block) - 4)[0]
    return block[:len(block) - tail], logits, value, legal


def extract(rows_path: Path, tensors_path: Path) -> list[dict]:
    data = tensors_path.read_bytes()
    out = []
    for line in rows_path.read_text(encoding="utf-8").splitlines():
        if not line.strip():
            continue
        row = json.loads(line)
        require(row.get("kind") == "td_replay", f"replay failed: {row}")
        base = row["bin_offset"]
        for index, legal, selected, turn, start, length in row["decisions"]:
            inputs, logits, value, legal_rec = decode(data[base + start:base + start + length])
            require(legal_rec == legal, "row and record disagree on the legal count")
            out.append({"job": row["id"], "decision": index, "legal": legal,
                        "input_sha256": hashlib.sha256(inputs).hexdigest(),
                        "logits": logits, "value": value})
    return out


def dump(binary: Path, source: Path, pins: list[Path], jobs: Path, out: Path, command: list[str] | None) -> dict:
    out.mkdir(parents=True, exist_ok=True)
    rows, tensors = out / "rows.jsonl", out / "tensors.bin"
    require(not rows.exists() and not tensors.exists(), f"{out} already holds a dump")
    argv = ([a.format(source=source, out=rows) for a in command] if command else
            [str(binary), f"source={source}", f"out={rows}", "mode=s4a-tensor-replay"])
    done = subprocess.run(argv, env=dict(os.environ, LABEL_JOBS=str(jobs), TD_BIN=str(tensors)),
                          capture_output=True, text=True)
    require(done.returncode == 0, f"replay failed ({done.returncode}): {done.stderr[-2000:]}")
    states = extract(rows, tensors)
    (out / "logits.jsonl").write_text("".join(json.dumps(s) + "\n" for s in states), encoding="utf-8",
                                      newline="\n")
    record = {"schema": DUMP_SCHEMA, "host_class": host_class(), "hostname": platform.node(),
              "binary": {"path": str(binary), "sha256": sha256_file(binary)}, "argv": argv,
              "source": {"path": str(source), "sha256": sha256_file(source)},
              "pins": sorted(sha256_file(p) for p in pins), "jobs_sha256": sha256_file(jobs),
              "rows_sha256": sha256_file(rows), "tensors_sha256": sha256_file(tensors),
              "logits_sha256": sha256_file(out / "logits.jsonl"), "decisions": len(states),
              "states": sum(1 for s in states if s["legal"] >= 2)}
    (out / "dump.json").write_text(json.dumps(record, indent=2) + "\n", encoding="utf-8", newline="\n")
    return record


# ---------------------------------------------------------------- the comparison

def load_dump(directory: Path) -> tuple[dict, list[dict]]:
    record = json.loads((directory / "dump.json").read_text(encoding="utf-8"))
    require(record.get("schema") == DUMP_SCHEMA, f"{directory}: not a logit dump")
    require(sha256_file(directory / "logits.jsonl") == record["logits_sha256"], f"{directory}: logits changed")
    states = [json.loads(line) for line in (directory / "logits.jsonl").read_text(encoding="utf-8").splitlines()
              if line.strip()]
    return record, states


def top_two_gap(logits: list[float]) -> float:
    ordered = sorted(logits, reverse=True)
    return ordered[0] - ordered[1] if len(ordered) > 1 else float("inf")


def argmax(logits: list[float]) -> int:
    return max(range(len(logits)), key=lambda i: (logits[i], -i))


def compare(linux: tuple[dict, list[dict]], windows: tuple[dict, list[dict]], tolerance: float) -> dict:
    (ra, sa), (rb, sb) = linux, windows
    require(ra["host_class"].startswith("linux-"), "the first dump is not from a Linux host")
    require(rb["host_class"].startswith("windows-"), "the second dump is not from a Windows host")
    require(ra["jobs_sha256"] == rb["jobs_sha256"], "the dumps replay different fixed states")
    require(ra["pins"] == rb["pins"] and ra["pins"], "the dumps load different pinned checkpoint bytes")
    require([(s["job"], s["decision"], s["legal"]) for s in sa] == [(s["job"], s["decision"], s["legal"]) for s in sb],
            "the dumps score different decision sequences")
    worst, worst_at, value_worst, argmax_breaks, ties, inputs_differ = 0.0, None, 0.0, [], 0, 0
    for a, b in zip(sa, sb):
        diff = max(abs(x - y) for x, y in zip(a["logits"], b["logits"]))
        if diff > worst:
            worst, worst_at = diff, [a["job"], a["decision"]]
        value_worst = max(value_worst, abs(a["value"] - b["value"]))
        inputs_differ += a["input_sha256"] != b["input_sha256"]
        if a["legal"] >= 2 and argmax(a["logits"]) != argmax(b["logits"]):
            if min(top_two_gap(a["logits"]), top_two_gap(b["logits"])) <= tolerance:
                ties += 1
            else:
                argmax_breaks.append([a["job"], a["decision"]])
    states = sum(1 for s in sa if s["legal"] >= 2)
    checks = {"states_at_least_10000": states >= MIN_STATES, "max_abs_logit_within_tolerance": worst <= tolerance,
              "argmax_equal_except_ties": not argmax_breaks}
    return {"schema": RECEIPT_SCHEMA, "pass": all(checks.values()), "checks": checks, "tolerance": tolerance,
            "states": states, "decisions": len(sa), "max_abs_logit_diff": worst, "max_at": worst_at,
            "max_abs_value_diff": value_worst, "argmax_ties_within_tolerance": ties,
            "argmax_breaks": argmax_breaks[:20], "input_tensors_differ": inputs_differ,
            "checkpoint_pins": ra["pins"], "jobs_sha256": ra["jobs_sha256"],
            "linux": {k: ra[k] for k in ("host_class", "hostname", "binary", "logits_sha256", "tensors_sha256")},
            "windows": {k: rb[k] for k in ("host_class", "hostname", "binary", "logits_sha256", "tensors_sha256")}}


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)
    p = sub.add_parser("states")
    p.add_argument("--jobs", type=Path, action="append", required=True)
    p.add_argument("--min-states", type=int, default=MIN_STATES)
    p.add_argument("--out", type=Path, required=True)
    p = sub.add_parser("dump")
    p.add_argument("--binary", type=Path, required=True)
    p.add_argument("--source", type=Path, required=True)
    p.add_argument("--pin", type=Path, action="append", required=True)
    p.add_argument("--jobs", type=Path, required=True)
    p.add_argument("--out", type=Path, required=True)
    p.add_argument("--command", default=None, help="JSON argv template with {source} and {out}")
    p = sub.add_parser("compare")
    p.add_argument("--linux", type=Path, required=True)
    p.add_argument("--windows", type=Path, required=True)
    p.add_argument("--tolerance", type=float, default=TOLERANCE)
    p.add_argument("--out", type=Path, required=True)
    args = parser.parse_args(argv)
    try:
        if args.command == "states":
            chosen, states = fixed_states(args.jobs, args.min_states)
            text = "".join(json.dumps(j, separators=(",", ":")) + "\n" for j in chosen)
            args.out.write_text(text, encoding="utf-8", newline="\n")
            result = {"jobs": len(chosen), "states": states, "sha256": sha256_file(args.out)}
        elif args.command == "dump":
            command = json.loads(args.command) if args.command else None
            result = dump(args.binary, args.source, args.pin, args.jobs, args.out, command)
        else:
            result = compare(load_dump(args.linux), load_dump(args.windows), args.tolerance)
            args.out.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8", newline="\n")
    except ParityError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1
    print(json.dumps(result, indent=2))
    return 0 if result.get("pass", True) else 1


if __name__ == "__main__":
    raise SystemExit(main())
