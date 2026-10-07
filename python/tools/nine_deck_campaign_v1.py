"""Campaign driver for the nine-deck baseline (v1).

Runs each assigned run's 20 blocks in order, every block through the
supported launcher (``native_expanded_dispatch_v1.py dispatch``) nested under
one held host reservation. After a block it runs the read-only exposure
collector, retains the records the design keeps, and prunes the rest.

Design rules applied here (collab LANES/nine-deck-baseline-design-20261007/DESIGN.md):

* Block b starts from block b-1's final checkpoint (T1 for block 1); the
  config and seeds of a block never change between attempts.
* Environmental interruption: the block reruns from its starting checkpoint
  with the identical schedule; partial outputs are discarded and logged. A
  block interrupted three times stops its run as a technical issue.
* Workload or numerical invalidity (failed exposure check, non-finite values,
  the non-natural limit): the run stops with a technical disposition.
* Retention: every per-iteration update, collection, ledger and completion
  record, per-episode rows, full trajectories of the seeded 2% update sample,
  and block-end checkpoints. Raw native output is pruned only after the rows
  and retained copies are verified; recovery archives are kept for the last
  ``cold_keep_blocks`` blocks of each run. Every deletion is logged in the
  campaign's PRUNE manifest.

CLI:
  launch --campaign C.json --runs r1,r2   take the host reservation and start ``run``
  run    --campaign C.json --runs r1,r2   drive runs (inside a held reservation)
  status --campaign C.json                one line per run
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import threading
import time
from datetime import datetime, timezone

import host_reservation_v1 as reservations
import nine_deck_baseline_v1 as ndb
import nine_deck_exposure_collector_v1 as collector

TOOLS = Path(__file__).resolve().parent
DISPATCH = TOOLS / "native_expanded_dispatch_v1.py"
MAX_ATTEMPTS = 3
POLL_SECONDS = 30
INVALIDITY_MARKERS = ("non-natural", "non_natural", "nonnatural", "not finite", "non-finite", "nan ")
SMALL_RECORDS = ("complete.json", "update.json", "collection.json", "non-natural.json", "collect-command.json",
                 "update-command.json", "update-input.json", "restarts.log")
LOCK = threading.Lock()


def now() -> str:
    return datetime.now(timezone.utc).isoformat()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with Path(path).open("rb") as stream:
        for chunk in iter(lambda: stream.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def pin(path: Path) -> dict:
    path = Path(path).resolve()
    return {"path": str(path), "sha256": sha256_file(path)}


def write_json(path: Path, value) -> dict:
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + ".partial")
    temporary.write_text(json.dumps(value, indent=1, sort_keys=True) + "\n", encoding="utf-8")
    os.replace(temporary, path)
    return pin(path)


def append_jsonl(path: Path, value) -> None:
    with LOCK:
        path.parent.mkdir(parents=True, exist_ok=True)
        with path.open("a", encoding="utf-8") as stream:
            stream.write(json.dumps(value, sort_keys=True) + "\n")


class Campaign:
    def __init__(self, path: Path):
        self.path = Path(path)
        self.raw = json.loads(self.path.read_text(encoding="utf-8"))
        roots = self.raw["roots"]
        self.state = Path(roots["state"])
        self.cold = Path(roots["cold"])
        self.retained = Path(roots["retained"])
        self.checkpoints = Path(roots["checkpoints"])
        self.exposure = Path(roots["exposure"])
        self.prune_log = self.state / "PRUNE.jsonl"
        self.events = self.state / "events.jsonl"

    def hot(self, run: str) -> Path:
        return Path(self.raw["roots"]["hot_by_run"].get(run, self.raw["roots"]["hot"]))

    def run_state_path(self, run: str) -> Path:
        return self.state / f"{run}.json"

    def run_state(self, run: str) -> dict:
        path = self.run_state_path(run)
        if path.exists():
            return json.loads(path.read_text(encoding="utf-8"))
        return {"run": run, "blocks": {}, "status": "pending"}

    def save_state(self, run: str, state: dict) -> None:
        state["updated"] = now()
        write_json(self.run_state_path(run), state)

    def event(self, **fields) -> None:
        append_jsonl(self.events, {"at": now(), **fields})


def chained_source(campaign: Campaign, state: dict, block: int) -> dict:
    """Block 1 starts from T1; block b from block b-1's retained final checkpoint."""
    t1 = campaign.raw["t1_source"]
    if block == 1:
        return t1
    previous = state["blocks"][str(block - 1)]
    return {"checkpoint": previous["final_checkpoint"], "feature_transfer": t1["feature_transfer"],
            "play_import": t1["play_import"]}


def block_config(campaign: Campaign, run: str, block: int, state: dict, native: Path) -> dict:
    raw = campaign.raw
    spec = {"run": run, "block": block, "runtime_decks": raw["decks"],
            "initial_source": chained_source(campaign, state, block), "fixed_opponent": raw["a48_source"],
            "update_backend": {"kind": "cpu"}, "collection_workers": raw["placement"]["workers"],
            "preparation_workers": raw["placement"]["preparation_workers"], "output_directory": str(native)}
    return ndb.block_config(spec)


def request_for(campaign: Campaign, run: str, block: int, config_pin: dict, root: Path, cold: Path) -> dict:
    raw = campaign.raw
    return {"schema": "native-expanded-cpu-dispatch/v1", "kind": "training", "lane": raw["lane"],
            "runtime": raw["runtime"], "config": config_pin, "root": str(root), "cold_root": str(cold),
            "placement": raw["placement"], "storage": raw["storage"], "wall_seconds": raw["wall_seconds"],
            "schedule_family": "permuted-units-v1", "non_natural_tolerance": ndb.FROZEN["max_non_natural_episode_fraction"],
            "choice": raw["choice"], "choice_verification": raw["choice_verification"]}


def classify_failure(root: Path) -> str:
    """'invalid' for workload/numerical invalidity, else 'interrupted'."""
    texts = []
    for name in ("execution.json", "stderr.log"):
        path = root / name
        if path.exists():
            texts.append(path.read_text(encoding="utf-8", errors="replace")[-20000:].lower())
    text = "\n".join(texts)
    return "invalid" if any(marker in text for marker in INVALIDITY_MARKERS) else "interrupted"


def dispatch_block(request_path: Path, compute_host_name: str | None = None) -> dict:
    extra = ["--compute-host-name", compute_host_name] if compute_host_name else []
    done = subprocess.run([sys.executable, "-B", str(DISPATCH), "dispatch", str(request_path)] + extra,
                          capture_output=True, text=True, cwd=str(TOOLS.parents[1]))
    if done.returncode != 0:
        raise RuntimeError(f"dispatch refused: {done.stderr.strip()[-2000:]}")
    return json.loads(done.stdout.strip().splitlines()[-1])


def wait_block(root: Path, pid: int) -> str:
    """'success' once report.json exists, 'failure' once the attempt ended without one."""
    creation = reservations.creation_time(pid)
    while True:
        if (root / "report.json").exists():
            return "success"
        execution = root / "execution.json"
        if execution.exists():
            record = json.loads(execution.read_text(encoding="utf-8"))
            if record.get("error"):
                return "failure"
        if creation is None or reservations.process_state(pid, creation) == "absent":
            time.sleep(5)
            return "success" if (root / "report.json").exists() else "failure"
        time.sleep(POLL_SECONDS)


def retain_block(campaign: Campaign, run: str, block: int, native: Path, summary: dict) -> dict:
    """Copy kept records, verify them, then prune the raw native tree."""
    target = campaign.retained / run / f"b{block:02d}"
    keep_updates = set(summary["retained_updates"])
    mapping = []
    for path in sorted(native.rglob("*")):
        if not path.is_file():
            continue
        relative = path.relative_to(native)
        parts = relative.parts
        keep = len(parts) == 1 or path.name in SMALL_RECORDS
        if not keep and parts[0] == "iterations" and path.name.startswith("episode-"):
            keep = int(parts[1]) in keep_updates
        if keep:
            destination = target / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(path, destination)
            digest = sha256_file(path)
            if sha256_file(destination) != digest:
                raise RuntimeError(f"retained copy differs: {destination}")
            mapping.append({"from": str(path), "to": str(destination), "sha256": digest})
    manifest = write_json(target / "RETAINED.json", {"run": run, "block": block, "native_root": str(native),
                                                     "retained_updates": sorted(keep_updates), "files": mapping})
    pruned_files, pruned_bytes = 0, 0
    for path in native.rglob("*"):
        if path.is_file():
            pruned_files += 1
            pruned_bytes += path.stat().st_size
    shutil.rmtree(native)
    append_jsonl(campaign.prune_log, {"at": now(), "run": run, "block": block, "pruned": str(native),
                                      "files": pruned_files, "bytes": pruned_bytes, "retained_manifest": manifest,
                                      "reason": "design retention rule: rows and retained copies verified",
                                      "recovery": "cold archive of this attempt, kept per cold window"})
    return manifest


def prune_cold(campaign: Campaign, run: str, keep_from_block: int, state: dict) -> None:
    for key, entry in state["blocks"].items():
        block = int(key)
        cold = entry.get("cold_root")
        if block >= keep_from_block or not cold or entry.get("cold_pruned"):
            continue
        shards = sorted(Path(cold).glob("native-*.zip"))
        size = sum(path.stat().st_size for path in shards)
        for path in shards:
            path.unlink()
        entry["cold_pruned"] = True
        append_jsonl(campaign.prune_log, {"at": now(), "run": run, "block": block, "pruned": [str(p) for p in shards],
                                          "bytes": size, "kept": str(Path(cold) / "archive.json"),
                                          "reason": "recovery window passed; rows, retained records and "
                                                    "block-end checkpoint verified"})


def run_block(campaign: Campaign, run: str, block: int, state: dict) -> str:
    entry = state["blocks"].setdefault(str(block), {"attempts": []})
    if entry["attempts"]:
        last = entry["attempts"][-1]
        if last.get("outcome") == "complete":
            # The driver stopped after the block completed: finish from the last saved step.
            return finish_block(campaign, run, block, state, entry, last)
        if "outcome" not in last:
            # The driver (and with it the reservation's whole job) ended mid-block.
            last["outcome"] = "interrupted"
            native = Path(last["native"])
            if native.exists():
                size = sum(p.stat().st_size for p in native.rglob("*") if p.is_file())
                shutil.rmtree(native)
                append_jsonl(campaign.prune_log, {"at": now(), "run": run, "block": block,
                                                  "attempt": last["attempt"], "pruned": str(native), "bytes": size,
                                                  "reason": "partial output of an interrupted attempt"})
            campaign.save_state(run, state)
    while len(entry["attempts"]) < MAX_ATTEMPTS:
        attempt = len(entry["attempts"]) + 1
        base = campaign.hot(run) / run / f"b{block:02d}-a{attempt}"
        native, root = base / "native", base / "dispatch"
        cold = campaign.cold / run / f"b{block:02d}-a{attempt}"
        config = block_config(campaign, run, block, state, native)
        config_path = campaign.state / "configs" / f"{run}-b{block:02d}-a{attempt}.json"
        config_path.parent.mkdir(parents=True, exist_ok=True)
        config_path.write_text(json.dumps(config, separators=(",", ":")) + "\n", encoding="utf-8")
        request_pin = write_json(campaign.state / "requests" / f"{run}-b{block:02d}-a{attempt}.json",
                                 request_for(campaign, run, block, pin(config_path), root, cold))
        record = {"attempt": attempt, "root": str(root), "native": str(native), "cold": str(cold),
                  "request": request_pin, "started": now()}
        entry["attempts"].append(record)
        campaign.save_state(run, state)
        campaign.event(run=run, block=block, attempt=attempt, kind="dispatch")
        result = dispatch_block(Path(request_pin["path"]), campaign.raw.get("compute_host_name"))
        record["dispatch"] = result
        if result.get("state") != "dispatched":
            record["outcome"] = "spawn-" + str(result.get("state"))
            campaign.save_state(run, state)
            return "stopped"
        outcome = wait_block(root, result["pid"])
        record["finished"] = now()
        if outcome == "success":
            record["outcome"] = "complete"
            record["report"] = pin(root / "report.json")
            campaign.save_state(run, state)
            return finish_block(campaign, run, block, state, entry, record)
        kind = classify_failure(root)
        record["outcome"] = kind
        if native.exists():
            size = sum(p.stat().st_size for p in native.rglob("*") if p.is_file())
            shutil.rmtree(native)
            append_jsonl(campaign.prune_log, {"at": now(), "run": run, "block": block, "attempt": attempt,
                                              "pruned": str(native), "bytes": size,
                                              "reason": "partial output of a failed attempt (design: discarded and logged)"})
        campaign.save_state(run, state)
        campaign.event(run=run, block=block, attempt=attempt, kind=kind)
        if kind == "invalid":
            entry["disposition"] = "technical: workload or numerical invalidity"
            return "stopped"
    entry["disposition"] = "technical: interrupted three times"
    campaign.save_state(run, state)
    return "stopped"


def finish_block(campaign: Campaign, run: str, block: int, state: dict, entry: dict, record: dict) -> str:
    """Collect, keep the block-end checkpoint, retain and prune. Each step is
    saved before the next, so a restart resumes after the last completed step."""
    native = Path(record["native"])
    if "exposure" not in entry:
        for name in (f"{run}-b{block:02d}-episodes.jsonl", f"{run}-b{block:02d}-exposure.json"):
            (campaign.exposure / name).unlink(missing_ok=True)
        summary = collector.collect(run, block, native, Path(campaign.raw["decks"]), campaign.exposure)
        entry["exposure"] = {"verdict": summary["verdict"],
                             "failed_checks": sorted(k for k, v in summary["checks"].items() if not v["pass"]),
                             "summary": pin(campaign.exposure / f"{run}-b{block:02d}-exposure.json")}
        campaign.save_state(run, state)
    summary = json.loads(Path(entry["exposure"]["summary"]["path"]).read_text(encoding="utf-8"))
    if "final_checkpoint" not in entry:
        completion = json.loads((native / "completion.json").read_text(encoding="utf-8"))
        final = json.loads(Path(completion["iterations"][-1]["path"]).read_text(encoding="utf-8"))
        update = json.loads(Path(final["update"]["path"]).read_text(encoding="utf-8"))
        checkpoint = Path(update["checkpoint"]["path"])
        if not sha256_file(checkpoint) == update["checkpoint"]["sha256"] == summary["final_checkpoint_sha256"]:
            raise RuntimeError("final checkpoint does not match its update record")
        kept = campaign.checkpoints / run / f"block-{block:02d}.json"
        kept.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(checkpoint, kept)
        if sha256_file(kept) != update["checkpoint"]["sha256"]:
            raise RuntimeError("retained checkpoint copy differs")
        entry["final_checkpoint"] = {"path": str(kept), "sha256": update["checkpoint"]["sha256"]}
        entry["final_adam_step"] = update["adam_step"]
        campaign.save_state(run, state)
    if block == 1 and "embedding_gate_pass" not in entry:
        gate = collector.embedding_gate(json.loads(Path(campaign.raw["t1_source"]["checkpoint"]["path"]).read_bytes()),
                                        json.loads(Path(entry["final_checkpoint"]["path"]).read_bytes()),
                                        ndb.load_decks(Path(campaign.raw["decks"])))
        entry["embedding_gate"] = write_json(campaign.exposure / f"{run}-b01-embedding-gate.json", gate)
        entry["embedding_gate_pass"] = gate["pass"]
        campaign.save_state(run, state)
    if "retained" not in entry:
        existing = campaign.retained / run / f"b{block:02d}" / "RETAINED.json"
        if not native.exists() and existing.exists():
            entry["retained"] = pin(existing)  # pruned after retention, before the state was saved
        else:
            entry["retained"] = retain_block(campaign, run, block, native, summary)
        entry["cold_root"] = record["cold"]
        campaign.save_state(run, state)
    prune_cold(campaign, run, block - campaign.raw["cold_keep_blocks"] + 1, state)
    failed = entry["exposure"]["failed_checks"] + (["embedding_gate"] if block == 1 and not entry["embedding_gate_pass"] else [])
    if failed:
        entry["disposition"] = ("technical: numerical invalidity" if "finite_losses" in failed
                                else "technical: exposure check failed") + f" ({', '.join(failed)})"
    else:
        entry["done"] = True
    campaign.save_state(run, state)
    campaign.event(run=run, block=block, kind="complete", exposure=entry["exposure"]["verdict"],
                   final_checkpoint=entry["final_checkpoint"]["sha256"], failed=failed)
    return "stopped" if failed else "complete"


def drive(campaign: Campaign, run: str) -> None:
    state = campaign.run_state(run)
    state["status"] = "running"
    campaign.save_state(run, state)
    try:
        for block in range(1, ndb.BLOCKS + 1):
            entry = state["blocks"].get(str(block))
            if entry and entry.get("done"):
                continue
            if entry and entry.get("disposition"):
                state["status"] = "stopped"
                return
            if run_block(campaign, run, block, state) != "complete":
                state["status"] = "stopped"
                return
        state["status"] = "complete"
    except Exception as error:  # recorded, never retried automatically
        state["status"] = "error"
        state["error"] = f"{type(error).__name__}: {error}"
        raise
    finally:
        campaign.save_state(run, state)
        campaign.event(run=run, kind="run-" + state["status"])


def run(campaign: Campaign, runs: list[str]) -> int:
    token = os.environ.get(reservations.TOKEN_ENV)
    if not token or reservations.status(token).get("token_fate") != "holds":
        raise SystemExit("the campaign driver runs only inside a held host reservation")
    threads = [threading.Thread(target=drive, args=(campaign, name), name=name) for name in runs]
    for thread in threads:
        thread.start()
    for thread in threads:
        thread.join()
    return 0 if all(campaign.run_state(name)["status"] == "complete" for name in runs) else 1


def launch(campaign_path: Path, runs: list[str]) -> dict:
    campaign = Campaign(campaign_path)
    return reservations.dispatch(
        lane=campaign.raw["lane"], work_id="nine-deck-" + "-".join(runs),
        release_condition="nine-deck baseline runs " + ",".join(runs) + " complete or stopped",
        command=[sys.executable, "-B", str(Path(__file__).resolve()), "run", "--campaign",
                 str(Path(campaign_path).resolve()), "--runs", ",".join(runs)],
        cwd=str(TOOLS.parents[1]),
        busy_pattern=r"native_expanded_training|expanded_deck_training|cargo|rustc|trainer\.exe")


def status_lines(campaign: Campaign) -> list[str]:
    lines = []
    for run_name in ndb.RUNS:
        state = campaign.run_state(run_name)
        done = sorted(int(k) for k, v in state["blocks"].items() if v.get("done"))
        lines.append(f"{run_name} {state['status']} blocks_done={len(done)} last={done[-1] if done else 0}")
    return lines


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("op", choices=("launch", "run", "status"))
    parser.add_argument("--campaign", type=Path, required=True)
    parser.add_argument("--runs", default="")
    args = parser.parse_args(argv)
    runs = [name for name in args.runs.split(",") if name]
    if args.op != "status" and (not runs or any(name not in ndb.RUNS for name in runs)):
        raise SystemExit("--runs needs run labels r1..r6")
    if args.op == "launch":
        print(json.dumps(launch(args.campaign, runs)))
        return 0
    if args.op == "run":
        return run(Campaign(args.campaign), runs)
    print("\n".join(status_lines(Campaign(args.campaign))))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
