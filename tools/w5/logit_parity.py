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
  logit_parity.py compare --linux DIR --windows DIR --out RECEIPT.json
  logit_parity.py scorer --dump DIR --jobs FIXED.jsonl --scorer EXE [--scorer-arg ARG ...] --config SCORER.json
                         --out RECEIPT.json

A state is a scored focal decision with at least two legal actions. `dump` refuses a source that does not load
the pinned checkpoint files, a `--command` whose program is not the pinned binary, replay rows that do not cover
exactly the fixed jobs' focal decisions, and any record whose length differs from its declared layout or whose
logits are not finite. `compare` refuses different jobs, checkpoint bytes or decision sequences, uses the fixed
1e-5 tolerance (an argmax tie excuses only a switch between the two winners within 1e-5 on both hosts), and
requires byte-identical input tensors; the value difference is reported beside.

`scorer` ties a dump to the Spellbench bot's forward (E5): it sends every dumped decision's input tensor to
`spellbench_scorer_v1` (the bot's scorer binary and config, which must load the dump's source) and requires the
scorer's logits and value to equal the dump's bit for bit. Run on the rating host beside that host's dump, it shows
that the receipt's Windows forward is the forward the rated bot plays.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import platform
import re
import struct
import subprocess
import sys
import tempfile
from pathlib import Path

DUMP_SCHEMA = "w5-logit-dump/v1"
RECEIPT_SCHEMA = "w5-logit-parity/v1"
SCORER_RECEIPT_SCHEMA = "w5-scorer-parity/v1"
SCORER_READY = "mtg-kernel-spellbench-scorer-ready/v1"
SCORER_REQUEST = "mtg-kernel-spellbench-scorer-request/v1"
SCORER_CHOICE = "mtg-kernel-spellbench-scorer-choice/v1"
MIN_STATES = 10_000
TOLERANCE = 1e-5
HEADER = struct.Struct("<8I")
# stage4a/tensor_dump.rs record layout (native_policy_value_net_v1.rs dimensions)
OBJECT_DIM, EDGE_DIM, ACTION_DIM, ACTION_REF_DIM = 98, 41, 195, 25


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

def record_size(objects: int, edges: int, actions: int, refs: int, state_len: int) -> int:
    words = (state_len + objects * (OBJECT_DIM + 3) + edges * (EDGE_DIM + 2) + actions * ACTION_DIM
             + refs * (ACTION_REF_DIM + 3) + actions + 1)
    return HEADER.size + 4 * words


def decode(block: bytes) -> tuple[bytes, list[float], float, int]:
    """One TD_BIN decision record: (input bytes, logits, value, legal); see stage4a/tensor_dump.rs."""
    require(len(block) >= HEADER.size, "truncated tensor record")
    objects, edges, actions, refs, state_len, selected, legal, actor = HEADER.unpack_from(block, 0)
    require(actions == legal and actions >= 1, "tensor record legal count differs from its actions")
    require(len(block) == record_size(objects, edges, actions, refs, state_len),
            "tensor record length differs from its declared layout")
    tail = 4 * (actions + 1)
    logits = list(struct.unpack_from(f"<{actions}f", block, len(block) - tail))
    value = struct.unpack_from("<f", block, len(block) - 4)[0]
    require(all(math.isfinite(x) for x in logits) and math.isfinite(value), "non-finite logit or value")
    return block[:len(block) - tail], logits, value, legal


def expected_decisions(jobs_path: Path) -> list[tuple[str, list[int]]]:
    """Each fixed job's focal decision indices, in job order: what the replay must score."""
    out = []
    for line in Path(jobs_path).read_text(encoding="utf-8").splitlines():
        if line.strip():
            job = json.loads(line)
            out.append((job["id"], [i for i, d in enumerate(job["decisions"]) if d["actor"] == job["focal"]]))
    return out


def extract(rows_path: Path, tensors_path: Path, jobs_path: Path) -> list[dict]:
    data = tensors_path.read_bytes()
    rows = [json.loads(line) for line in rows_path.read_text(encoding="utf-8").splitlines() if line.strip()]
    expected = expected_decisions(jobs_path)
    require([r.get("id") for r in rows] == [job for job, _ in expected], "replay rows differ from the fixed jobs")
    out = []
    for row, (_, indices) in zip(rows, expected):
        require(row.get("kind") == "td_replay", f"replay failed: {row}")
        require([d[0] for d in row["decisions"]] == indices, f"{row['id']}: scored decisions differ from the job")
        base = row["bin_offset"]
        require(0 <= base and base + row["bin_bytes"] <= len(data), f"{row['id']}: block outside TD_BIN")
        for index, legal, selected, turn, start, length in row["decisions"]:
            require(0 <= start and start + length <= row["bin_bytes"], f"{row['id']}: record outside its block")
            inputs, logits, value, legal_rec = decode(data[base + start:base + start + length])
            require(legal_rec == legal, "row and record disagree on the legal count")
            out.append({"job": row["id"], "decision": index, "legal": legal,
                        "input_sha256": hashlib.sha256(inputs).hexdigest(),
                        "logits": logits, "value": value})
    return out


def referenced_files(value, base: Path) -> dict[str, str]:
    """Every existing file a source JSON names (absolute, or relative to the source), by SHA-256."""
    found = {}
    if isinstance(value, dict):
        for item in value.values():
            found.update(referenced_files(item, base))
    elif isinstance(value, list):
        for item in value:
            found.update(referenced_files(item, base))
    elif isinstance(value, str) and len(value) < 1024:
        for candidate in (Path(value), base / value):
            try:
                if candidate.is_file():
                    found[str(candidate)] = sha256_file(candidate)
                    break
            except OSError:
                pass
    return found


def source_pins(source: Path) -> dict[str, str]:
    return referenced_files(json.loads(Path(source).read_text(encoding="utf-8")), Path(source).parent)


def canonical_source(value, files: dict[str, str], base: Path):
    """The source with every file path replaced by the SHA-256 of its bytes: equal on both hosts exactly when
    they load the same files with the same settings and overrides."""
    if isinstance(value, dict):
        return {k: canonical_source(v, files, base) for k, v in value.items()}
    if isinstance(value, list):
        return [canonical_source(v, files, base) for v in value]
    if isinstance(value, str):
        for candidate in (str(Path(value)), str(base / value)):
            if candidate in files:
                return "sha256:" + files[candidate]
    return value


def value_identity(value, base: Path) -> str:
    """`source_identity` of a source value whose relative paths resolve against `base`."""
    canon = canonical_source(value, referenced_files(value, base), base)
    return hashlib.sha256(json.dumps(canon, sort_keys=True, separators=(",", ":")).encode()).hexdigest()


def source_identity(source: Path) -> str:
    return value_identity(json.loads(Path(source).read_text(encoding="utf-8")), Path(source).parent)


def dump(binary: Path, source: Path, pins: list[Path], jobs: Path, out: Path, command: list[str] | None) -> dict:
    loaded = source_pins(source)
    pin_hashes = sorted(sha256_file(x) for x in pins)
    require(pin_hashes and set(pin_hashes) <= set(loaded.values()),
            "the source does not load the pinned checkpoint files")
    command_files = {}
    if command:
        require(os.path.realpath(command[0]) == os.path.realpath(binary),
                "--command must run the pinned --binary")
        for arg in command[1:]:  # every file the command names (scripts, adapters, --flag=PATH) is pinned
            for part in (arg, arg.split("=", 1)[-1]):
                if part and Path(part).is_file():
                    command_files[part] = sha256_file(part)
    out.mkdir(parents=True, exist_ok=True)
    rows, tensors = out / "rows.jsonl", out / "tensors.bin"
    require(not rows.exists() and not tensors.exists(), f"{out} already holds a dump")
    argv = ([a.format(source=source, out=rows) for a in command] if command else
            [str(binary), f"source={source}", f"out={rows}", "mode=s4a-tensor-replay"])
    done = subprocess.run(argv, env=dict(os.environ, LABEL_JOBS=str(jobs), TD_BIN=str(tensors)),
                          capture_output=True, text=True)
    require(done.returncode == 0, f"replay failed ({done.returncode}): {done.stderr[-2000:]}")
    states = extract(rows, tensors, jobs)
    (out / "logits.jsonl").write_text("".join(json.dumps(s) + "\n" for s in states), encoding="utf-8",
                                      newline="\n")
    record = {"schema": DUMP_SCHEMA, "host_class": host_class(), "hostname": platform.node(),
              "binary": {"path": str(binary), "sha256": sha256_file(binary)}, "argv": argv,
              "source": {"path": str(source), "sha256": sha256_file(source)},
              "pins": pin_hashes, "source_files": loaded, "source_identity": source_identity(source),
              "command_files": command_files, "jobs_sha256": sha256_file(jobs),
              "rows_sha256": sha256_file(rows), "tensors_sha256": sha256_file(tensors),
              "logits_sha256": sha256_file(out / "logits.jsonl"), "decisions": len(states),
              "states": sum(1 for s in states if s["legal"] >= 2)}
    (out / "dump.json").write_text(json.dumps(record, indent=2) + "\n", encoding="utf-8", newline="\n")
    return record


# ---------------------------------------------------------------- the bot's scorer

# The scorer's tensor fields in TD_BIN record order: struct code (u32 float bits or i32) and length.
TENSOR_FIELDS = (
    ("state", "I", ("state", 1)), ("object_features", "I", ("objects", OBJECT_DIM)),
    ("object_card_ids", "i", ("objects", 1)), ("object_groups", "i", ("objects", 1)),
    ("object_node_ids", "i", ("objects", 1)), ("edge_features", "I", ("edges", EDGE_DIM)),
    ("edge_source_indices", "i", ("edges", 1)), ("edge_target_indices", "i", ("edges", 1)),
    ("action_features", "I", ("actions", ACTION_DIM)), ("action_ref_features", "I", ("refs", ACTION_REF_DIM)),
    ("action_ref_card_ids", "i", ("refs", 1)), ("action_ref_action_indices", "i", ("refs", 1)),
    ("action_ref_node_indices", "i", ("refs", 1)))


def decode_request(block: bytes) -> tuple[dict, list[int], int, int]:
    """One TD_BIN record as the scorer's tensor (floats as u32 bits), its logit bits, value bits and actor."""
    decode(block)  # layout and finiteness
    objects, edges, actions, refs, state_len, _selected, _legal, actor = HEADER.unpack_from(block, 0)
    counts = {"state": state_len, "objects": objects, "edges": edges, "actions": actions, "refs": refs}
    tensor, offset = {}, HEADER.size
    for field, code, (count, width) in TENSOR_FIELDS:
        n = counts[count] * width
        tensor[field] = list(struct.unpack_from(f"<{n}{code}", block, offset))
        offset += 4 * n
    logits = list(struct.unpack_from(f"<{actions}I", block, offset))
    value = struct.unpack_from("<I", block, offset + 4 * actions)[0]
    return tensor, logits, value, actor


def bits_to_float(bits: int) -> float:
    return struct.unpack("<f", struct.pack("<I", bits))[0]


def scorer_check(dump_dir: Path, jobs: Path, scorer: Path, config: Path, scorer_args: list[str] = ()) -> dict:
    record = json.loads((dump_dir / "dump.json").read_text(encoding="utf-8"))
    require(record.get("schema") == DUMP_SCHEMA, f"{dump_dir}: not a logit dump")
    rows_path, tensors_path = dump_dir / "rows.jsonl", dump_dir / "tensors.bin"
    require(sha256_file(rows_path) == record["rows_sha256"] and sha256_file(tensors_path) == record["tensors_sha256"],
            f"{dump_dir}: rows or tensors changed")
    require(sha256_file(jobs) == record["jobs_sha256"], "the jobs are not the dump's fixed states")
    settings = json.loads(Path(config).read_text(encoding="utf-8"))
    require(value_identity(settings.get("source"), Path(config).parent) == record["source_identity"],
            "the scorer config does not load the dump's source (files and settings)")
    data = tensors_path.read_bytes()
    rows = [json.loads(line) for line in rows_path.read_text(encoding="utf-8").splitlines() if line.strip()]
    require([r.get("id") for r in rows] == [job for job, _ in expected_decisions(jobs)],
            "replay rows differ from the fixed jobs")
    # Binary pipes: request lines end in LF on every system. Diagnostics go to a file, never a full pipe.
    errors = tempfile.TemporaryFile()
    argv = [str(scorer), *scorer_args, "--config", str(config)]  # as the bot runs it (--scorer-arg)
    proc = subprocess.Popen(argv, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                            stderr=errors)
    decisions = states = logit_mismatches = value_mismatches = 0
    worst, first_mismatch = 0.0, None
    try:
        ready = json.loads(proc.stdout.readline() or "null")
        require(isinstance(ready, dict) and ready.get("schema") == SCORER_READY, f"scorer did not start: {ready!r}")
        sampled = ready.get("selection") == "sampled-wide-v1"
        for row in rows:
            require(row.get("kind") == "td_replay", f"replay failed: {row}")
            base = row["bin_offset"]
            for index, legal, _selected, _turn, start, length in row["decisions"]:
                tensor, logits, value, actor = decode_request(data[base + start:base + start + length])
                request = {"schema": SCORER_REQUEST, "request_id": f"{row['id']}:{index}", "game_id": row["id"],
                           "seat": f"p{actor}", "step": index,
                           "feature_contract_digest": ready.get("feature_contract_digest"),
                           "feature_encoding_digest": ready.get("feature_encoding_digest"),
                           "row_candidate_ids": list(range(legal)), "sample_seed": decisions if sampled else None,
                           "tensor": tensor}
                proc.stdin.write(json.dumps(request, separators=(",", ":")).encode() + b"\n")
                proc.stdin.flush()
                choice = json.loads(proc.stdout.readline() or "null")
                require(isinstance(choice, dict) and choice.get("schema") == SCORER_CHOICE,
                        f"{row['id']}:{index}: scorer answered {choice!r}")
                got = choice["logits_bits"]
                require(len(got) == len(logits), f"{row['id']}:{index}: scorer row count differs")
                worst = max([worst] + [abs(bits_to_float(a) - bits_to_float(b)) for a, b in zip(got, logits)])
                if got != logits:
                    logit_mismatches += 1
                    first_mismatch = first_mismatch or [row["id"], index]
                value_mismatches += choice["value_bits"] != value
                decisions += 1
                states += legal >= 2
    finally:
        proc.stdin.close()
        proc.wait()
        errors.seek(0)
        stderr = errors.read().decode("utf-8", "replace")
        errors.close()
    require(decisions == record["decisions"], "the scorer did not see every dumped decision")
    checks = {"logits_bit_identical": logit_mismatches == 0, "values_bit_identical": value_mismatches == 0}
    return {"schema": SCORER_RECEIPT_SCHEMA, "pass": all(checks.values()), "checks": checks, "decisions": decisions,
            "states": states, "logit_mismatches": logit_mismatches, "value_mismatches": value_mismatches,
            "first_mismatch": first_mismatch, "max_abs_logit_diff": worst, "host_class": host_class(),
            "hostname": platform.node(), "argv": argv, "scorer": {"path": str(scorer), "sha256": sha256_file(scorer)},
            "config": {"path": str(config), "sha256": sha256_file(config)},
            "ready": {k: ready.get(k) for k in ("model_state_sha256", "model_architecture", "feature_contract_digest",
                                                "feature_encoding_digest", "selection", "sampler_identity")},
            "dump": {k: record[k] for k in ("host_class", "source_identity", "jobs_sha256", "tensors_sha256",
                                            "logits_sha256", "pins")},
            "scorer_stderr_tail": stderr[-2000:]}


# ---------------------------------------------------------------- the comparison

def load_dump(directory: Path) -> tuple[dict, list[dict]]:
    record = json.loads((directory / "dump.json").read_text(encoding="utf-8"))
    require(record.get("schema") == DUMP_SCHEMA, f"{directory}: not a logit dump")
    require(sha256_file(directory / "logits.jsonl") == record["logits_sha256"], f"{directory}: logits changed")
    states = [json.loads(line) for line in (directory / "logits.jsonl").read_text(encoding="utf-8").splitlines()
              if line.strip()]
    return record, states


def argmax(logits: list[float]) -> int:
    return max(range(len(logits)), key=lambda i: (logits[i], -i))


def compare(linux: tuple[dict, list[dict]], windows: tuple[dict, list[dict]], tolerance: float = TOLERANCE) -> dict:
    require(math.isfinite(tolerance) and 0 < tolerance <= TOLERANCE, f"tolerance must be in (0, {TOLERANCE}]")
    (ra, sa), (rb, sb) = linux, windows
    require(ra["host_class"].startswith("linux-"), "the first dump is not from a Linux host")
    require(rb["host_class"].startswith("windows-"), "the second dump is not from a Windows host")
    require(ra["jobs_sha256"] == rb["jobs_sha256"], "the dumps replay different fixed states")
    require(ra["pins"] == rb["pins"] and ra["pins"], "the dumps load different pinned checkpoint bytes")
    require(ra["source_identity"] == rb["source_identity"],
            "the dumps load different sources (files, settings or overrides beyond host paths)")
    require([(s["job"], s["decision"], s["legal"]) for s in sa] == [(s["job"], s["decision"], s["legal"]) for s in sb],
            "the dumps score different decision sequences")
    keys = [(s["job"], s["decision"]) for s in sa]
    require(len(set(keys)) == len(keys), "a decision is scored twice")
    worst, worst_at, value_worst, argmax_breaks, ties, inputs_differ = 0.0, None, 0.0, [], 0, 0
    for a, b in zip(sa, sb):
        require(all(math.isfinite(x) for x in a["logits"] + b["logits"]), "non-finite logits")
        diff = max(abs(x - y) for x, y in zip(a["logits"], b["logits"]))
        if diff > worst:
            worst, worst_at = diff, [a["job"], a["decision"]]
        value_worst = max(value_worst, abs(a["value"] - b["value"]))
        inputs_differ += a["input_sha256"] != b["input_sha256"]
        ia, ib = argmax(a["logits"]), argmax(b["logits"])
        if a["legal"] >= 2 and ia != ib:
            # a tie only between the two winners themselves, within the tolerance on both hosts
            if (abs(a["logits"][ia] - a["logits"][ib]) <= tolerance
                    and abs(b["logits"][ia] - b["logits"][ib]) <= tolerance):
                ties += 1
            else:
                argmax_breaks.append([a["job"], a["decision"]])
    states = sum(1 for s in sa if s["legal"] >= 2)
    checks = {"states_at_least_10000": states >= MIN_STATES, "max_abs_logit_within_tolerance": worst <= tolerance,
              "argmax_equal_except_ties": not argmax_breaks, "input_tensors_identical": inputs_differ == 0}
    return {"schema": RECEIPT_SCHEMA, "pass": all(checks.values()), "checks": checks, "tolerance": tolerance,
            "states": states, "decisions": len(sa), "max_abs_logit_diff": worst, "max_at": worst_at,
            "max_abs_value_diff": value_worst, "argmax_ties_within_tolerance": ties,
            "argmax_breaks": argmax_breaks[:20], "input_tensors_differ": inputs_differ,
            "checkpoint_pins": ra["pins"], "source_identity": ra["source_identity"], "jobs_sha256": ra["jobs_sha256"],
            "linux": {k: ra[k] for k in ("host_class", "hostname", "binary", "command_files", "logits_sha256",
                                         "tensors_sha256")},
            "windows": {k: rb[k] for k in ("host_class", "hostname", "binary", "command_files", "logits_sha256",
                                           "tensors_sha256")}}


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="op", required=True)
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
    p.add_argument("--out", type=Path, required=True)
    p = sub.add_parser("scorer")
    p.add_argument("--dump", type=Path, required=True)
    p.add_argument("--jobs", type=Path, required=True)
    p.add_argument("--scorer", type=Path, required=True)
    p.add_argument("--scorer-arg", action="append", default=[], help="argument placed before --config")
    p.add_argument("--config", type=Path, required=True)
    p.add_argument("--out", type=Path, required=True)
    args = parser.parse_args(argv)
    try:
        if args.op == "states":
            chosen, states = fixed_states(args.jobs, args.min_states)
            text = "".join(json.dumps(j, separators=(",", ":")) + "\n" for j in chosen)
            args.out.write_text(text, encoding="utf-8", newline="\n")
            result = {"jobs": len(chosen), "states": states, "sha256": sha256_file(args.out)}
        elif args.op == "dump":
            command = json.loads(args.command) if args.command else None
            result = dump(args.binary, args.source, args.pin, args.jobs, args.out, command)
        elif args.op == "scorer":
            result = scorer_check(args.dump, args.jobs, args.scorer, args.config, args.scorer_arg)
            args.out.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8", newline="\n")
        else:
            result = compare(load_dump(args.linux), load_dump(args.windows))
            args.out.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8", newline="\n")
    except ParityError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1
    print(json.dumps(result, indent=2))
    return 0 if result.get("pass", True) else 1


if __name__ == "__main__":
    raise SystemExit(main())
