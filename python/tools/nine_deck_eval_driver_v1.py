"""P1/P3 panel evaluations for the nine-deck baseline (v1).

Each evaluation is one expanded-collector job through the supported launcher
(``native_expanded_dispatch_v1.py dispatch``, kind evaluation), nested under
one held host reservation: a checkpoint pilots every own deck against fixed
T1 on the ``dev`` cases (P1: 1,944 cases; P3: the Spy and CawGates subset,
432 cases). After a job the driver writes one row per game, keeps the full
trajectories of a seeded 2% case sample, and prunes the rest; a job with a
technical termination fails as incomplete and is never dropped silently.

CLI:
  launch --plan PLAN.json        take the host reservation and run ``run``
  run    --plan PLAN.json        evaluate every job in the plan not yet done
PLAN: {lane, runtime, decks, t1_source, placement, storage, wall_seconds,
       choice, choice_verification, compute_host_name?, root, cold_root, retained,
       jobs: [{id, panel: p1|p3, source}]}
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
import time

import host_reservation_v1 as reservations
import nine_deck_baseline_v1 as ndb

TOOLS = Path(__file__).resolve().parent
DISPATCH = TOOLS / "native_expanded_dispatch_v1.py"
PANEL_DECKS = {"p1": tuple(range(ndb.DECKS)), "p3": (4, 7)}
SAMPLE_PERCENT = 2


def sha256_file(path: Path) -> str:
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def pin(path: Path) -> dict:
    return {"path": str(Path(path)).replace("\\", "/"), "sha256": sha256_file(path)}


def write_json(path: Path, value) -> dict:
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=1, sort_keys=True) + "\n", encoding="utf-8")
    return pin(path)


def kept_case(case_id: str) -> bool:
    """Seeded 2% sample of panel cases whose full trajectories are retained."""
    digest = hashlib.sha256(ndb.key_bytes([f"{ndb.PREFIX}/retain-eval", case_id])).digest()
    return int.from_bytes(digest[:8], "little") % 100 < SAMPLE_PERCENT


def job_config(plan: dict, job: dict, native: Path) -> dict:
    decks = ndb.load_decks(Path(plan["decks"]))
    return ndb.panel_config(job["panel"], job["source"], plan["t1_source"], decks, str(native).replace("\\", "/"),
                            plan["placement"]["workers"], own_decks=PANEL_DECKS[job["panel"]])


def rows_from(collection_path: Path, job: dict) -> list[dict]:
    collection = json.loads(Path(collection_path).read_text(encoding="utf-8"))
    if not collection["complete"]:
        raise RuntimeError("incomplete collection")
    rows = []
    for item in collection["trajectories"]:
        data = Path(item["path"]).read_bytes()
        if hashlib.sha256(data).hexdigest() != item["sha256"]:
            raise RuntimeError(f"trajectory changed: {item['path']}")
        trajectory = json.loads(data)
        episode, terminal = trajectory["episode"], trajectory["terminal"]
        fields = dict((part[0], int(part[1:])) for part in episode["id"].split("-")[-4:])
        seat = episode["learner_seat"]
        natural = terminal["terminal_classification"] == "natural"
        winner = terminal.get("winner")
        score = None if not natural else (0.5 if winner is None else float(winner == f"p{seat}"))
        rows.append({"job": job["id"], "panel": job["panel"], "checkpoint_sha256": job["source"]["checkpoint"]["sha256"],
                     "own": fields["o"], "other": fields["t"], "repeat": fields["k"], "seat": seat,
                     "seed": episode["seed"], "starting_player": episode["starting_player"],
                     "terminal_classification": terminal["terminal_classification"], "score": score,
                     "trajectory_sha256": item["sha256"], "retained": kept_case(episode["id"])})
    return rows


def run_job(plan: dict, job: dict, state: dict) -> None:
    root = Path(plan["root"]) / job["id"]
    native, dispatch_root, cold = root / "native", root / "dispatch", Path(plan["cold_root"]) / job["id"]
    config_path = root / "config.json"
    root.mkdir(parents=True, exist_ok=True)
    config_path.write_text(json.dumps(job_config(plan, job, native), separators=(",", ":")) + "\n", encoding="utf-8")
    request = {"schema": "native-expanded-cpu-dispatch/v1", "kind": "evaluation", "lane": plan["lane"],
               "runtime": plan["runtime"], "config": pin(config_path), "root": str(dispatch_root).replace("\\", "/"),
               "cold_root": str(cold).replace("\\", "/"), "placement": plan["placement"], "storage": plan["storage"],
               "wall_seconds": plan["wall_seconds"], "choice": plan["choice"][job["panel"]],
               "choice_verification": plan["choice_verification"][job["panel"]]}
    request_pin = write_json(root / "request.json", request)
    extra = ["--compute-host-name", plan["compute_host_name"]] if plan.get("compute_host_name") else []
    done = subprocess.run([sys.executable, "-B", str(DISPATCH), "dispatch", request_pin["path"]] + extra,
                          capture_output=True, text=True, cwd=str(TOOLS.parents[1]))
    if done.returncode != 0:
        raise RuntimeError(f"dispatch refused for {job['id']}: {done.stderr[-1500:]}")
    while not (dispatch_root / "report.json").exists():
        execution = dispatch_root / "execution.json"
        if execution.exists() and json.loads(execution.read_text(encoding="utf-8")).get("error"):
            state[job["id"]] = {"status": "incomplete", "error": json.loads(execution.read_text())["error"]}
            return
        time.sleep(20)
    rows = rows_from(native / "collection.json", job)
    out = Path(plan["retained"]) / job["id"]
    out.mkdir(parents=True, exist_ok=True)
    data = "".join(json.dumps(row, sort_keys=True) + "\n" for row in rows).encode()
    (out / "rows.jsonl").write_bytes(data)
    kept = []
    collection = json.loads((native / "collection.json").read_text(encoding="utf-8"))
    for row, item in zip(rows, collection["trajectories"]):
        if row["retained"]:
            target = out / "trajectories" / Path(item["path"]).name
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(item["path"], target)
            if sha256_file(target) != item["sha256"]:
                raise RuntimeError("retained trajectory copy differs")
            kept.append(str(target))
    for name in ("collection.json",):
        shutil.copyfile(native / name, out / name)
    for name in ("report.json", "execution.json", "native-result.json"):
        if (dispatch_root / name).exists():
            shutil.copyfile(dispatch_root / name, out / name)
    size = sum(path.stat().st_size for path in native.rglob("*") if path.is_file())
    shutil.rmtree(native)
    for shard in cold.glob("native-*.zip"):
        shard.unlink()
    state[job["id"]] = {"status": "complete", "rows": {"path": str(out / "rows.jsonl"), "sha256": hashlib.sha256(data).hexdigest(),
                                                       "count": len(rows)},
                        "incomplete_games": sum(row["score"] is None for row in rows), "retained_trajectories": len(kept),
                        "pruned_bytes": size}


def run(plan_path: Path) -> int:
    token = os.environ.get(reservations.TOKEN_ENV)
    if not token or reservations.status(token).get("token_fate") != "holds":
        raise SystemExit("panel evaluation runs only inside a held host reservation")
    plan = json.loads(Path(plan_path).read_text(encoding="utf-8"))
    state_path = Path(plan["root"]) / "state.json"
    state = json.loads(state_path.read_text(encoding="utf-8")) if state_path.exists() else {}
    for job in plan["jobs"]:
        if state.get(job["id"], {}).get("status") == "complete":
            continue
        run_job(plan, job, state)
        write_json(state_path, state)
    return 0 if all(state.get(job["id"], {}).get("status") == "complete" for job in plan["jobs"]) else 1


def launch(plan_path: Path) -> dict:
    plan = json.loads(Path(plan_path).read_text(encoding="utf-8"))
    return reservations.dispatch(
        lane=plan["lane"], work_id="nine-deck-panels-" + Path(plan_path).stem,
        release_condition="panel evaluation jobs complete or failed",
        command=[sys.executable, "-B", str(Path(__file__).resolve()), "run", "--plan", str(Path(plan_path).resolve())],
        cwd=str(TOOLS.parents[1]),
        busy_pattern=r"native_expanded_training|expanded_deck_training|cargo|rustc|trainer\.exe")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("op", choices=("launch", "run"))
    parser.add_argument("--plan", type=Path, required=True)
    args = parser.parse_args(argv)
    if args.op == "launch":
        print(json.dumps(launch(args.plan)))
        return 0
    return run(args.plan)


if __name__ == "__main__":
    raise SystemExit(main())
