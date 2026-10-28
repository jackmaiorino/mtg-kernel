"""Qualification of the G(i) study on one host (collab LANES/g-references-20261028/DESIGN.md v1.2, section 6).

Timing and identity only, never a strength reading. One plan per candidate
host (Jack's PC, or a RunPod lease) drives, inside one held host reservation
(the campaign's ``host_slots_v1.py timed`` cores):

1. Launcher throughput evidence for the empty-pool family of
   ``permuted-units-own-history-v1`` (blocks 1 and 2): G1 block 1's one-update
   payload through ``native_expanded_dispatch_v1.py qualify`` serially (1 worker)
   and at the slot width, then side by side on the eight pack slots
   (``native_expanded_pack_v1.py qualify``); the choice from the plan's
   inventory (all three placements, at most 24 h old), with one ``check-choice``
   verification per family, written to the campaign's ``choice_pins``.
2. Item 1: G1's blocks 1 and 2 alone, then all eight lineages concurrently:
   G2..G8 in the campaign, with G1's blocks rerun in a scratch campaign as the
   eighth, whose block ends must be identical to the alone run's. Blocks 1 and 2
   of the study are kept and resumed if this host is chosen.
3. Item 3: G1's block 2 rerun in a scratch campaign with its native process
   killed at the block's midpoint; the driver reruns it from its starting
   checkpoint. The end must be identical to the straight block: the same state
   hash, Adam step and checkpoint content once output paths are set aside (the
   launcher's own serial/parallel comparison). The receipt keeps both file
   hashes and both state hashes (FROZEN.md disposition 5) and is what
   ARTIFACT-LAW's ancestry pruning needs.
4. The history family (blocks 3..741): the same launcher evidence on block 4's
   schedule (pool {1, 2}) from block 2's end, and the choice again over both.
5. Item 6: that synthetic block, straight and interrupted, which must be
   identical and draw both history checkpoints; it is discarded afterwards.
6. ``record``: item 2 (bytes per completed block and the projected eight-lineage
   peak, qualification leftovers included, against the 120 GB cap) and item 7
   (every dispatched config carries the runtime profile on all 1,620 episodes,
   and a stripped config is refused). Its ``pass`` is every check together; the
   driver runs no block past 2 without a placement record citing a passing one.

Items 4 and 5 (the Windows evaluator and the Spellbench scorer load a block-2
checkpoint) run on Jack's PC with the collab lane's tools. ``record``
summarizes everything into one qualification record for the run manifest.

CLI:
  plan    --campaign C.json --spec SPEC.json --out QDIR   offline: trial requests and the pack plan
  launch  --campaign C.json --plan QDIR/plan.json         take the reservation (timed cores) and start ``run``
  start   --campaign C.json --spec SPEC.json --out QDIR   plan unless planned, then launch (a lease's launch)
  run     --campaign C.json --plan QDIR/plan.json         inside the held reservation
  record  --campaign C.json --plan QDIR/plan.json --out QUALIFICATION.json
"""
from __future__ import annotations

import argparse
import copy
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import threading
import time

import g_references_campaign_v1 as driver
import g_references_v1 as g
import host_reservation_v1 as reservations
import native_expanded_dispatch_v1 as dispatch

TOOLS = Path(__file__).resolve().parent
PLAN_SCHEMA = "g-references-v1/qualification-plan"
RECORD_SCHEMA = driver.QUALIFICATION_SCHEMA
CHOICE = dispatch.CHOICE
POLL_SECONDS = 10
pin, write_json, now = driver.pin, driver.write_json, driver.now


def read(path):
    return json.loads(Path(path).read_text(encoding="utf-8"))


# ---------------------------------------------------------------- plan (offline)


def trial_plan(campaign: driver.Campaign, config_path: Path, spec: dict, out: Path, tag: str) -> dict:
    """One family's launcher evidence: serial and slot-width solo trial requests, and the eight-slot pack plan."""
    slots = [campaign.placement(run) for run in g.RUNS]
    first, trials = slots[0], {}
    for name, workers in (("w1", 1), (f"w{first['workers']}", first["workers"])):
        request = driver.request_for(campaign, "G1", pin(config_path), out / tag / "trials" / name,
                                     out / tag / "trials-cold" / name)
        for key in ("choice", "choice_verification", "lease_work_seconds"):
            request.pop(key, None)
        request.update(placement=dict(first, workers=workers, preparation_workers=workers), storage=spec["storage"],
                       wall_seconds=spec["wall_seconds"])
        trials[name] = write_json(out / tag / f"request-{name}.json", request)
    pack = {"schema": "native-expanded-pack-qualify/v1", "base_request": trials[f"w{first['workers']}"],
            "slots": [{key: slot[key] for key in ("cpu_affinity", "workers", "preparation_workers", "memory_bytes")}
                      for slot in slots],
            "root": str(out / tag / "pack"), "timeout_seconds": spec.get("timeout_seconds", 3600)}
    if campaign.raw.get("compute_host_name"):
        pack["compute_host_name"] = campaign.raw["compute_host_name"]
    return {"config": pin(config_path), "trials": trials, "pack": write_json(out / tag / "pack-plan.json", pack)}


def write_config(config: dict, path: Path) -> Path:
    g.require_runtime_rules(config)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(config, separators=(",", ":")) + "\n", encoding="utf-8")
    return path


def plan(campaign_path: Path, spec: dict, out: Path) -> dict:
    """The empty-pool family's launcher evidence plan (G1 block 1's payload) and the spec the history family's
    plan uses once blocks 1 and 2 exist (it needs block 2's end).

    spec: storage (the dispatcher's storage block for the trials), wall_seconds, inventory (pin of the
    three-placement inventory JSON), interrupt_at_update (default 81), timeout_seconds.
    """
    campaign = driver.Campaign(campaign_path)
    out = Path(out).resolve()
    out.mkdir(parents=True)
    config = g.block_config(driver.block_spec(campaign, "G1", 1, {"blocks": {}}, out / "unused-native"))
    families = {"empty-pool": trial_plan(campaign, write_config(config, out / "g1-b001-config.json"), spec, out,
                                         "empty-pool")}
    value = {"schema": PLAN_SCHEMA, "host": campaign.host(), "campaign": pin(campaign_path), "families": families,
             "spec": spec, "inventory": spec["inventory"], "interrupt_at_update": spec.get("interrupt_at_update", 81),
             "root": str(out)}
    return write_json(out / "plan.json", value)


# ---------------------------------------------------------------- run (inside the reservation)


def wait_report(root: Path, pid: int) -> dict:
    if driver.base.wait_block(root, pid) != "success":
        raise RuntimeError(f"qualification trial failed: {root}")
    return pin(root / "report.json")


def nested(args: list[str]) -> dict:
    done = subprocess.run([sys.executable, "-B", *args], capture_output=True, text=True, cwd=str(TOOLS.parents[1]))
    if done.returncode != 0:
        raise RuntimeError(f"{args[0]} failed: {done.stderr.strip()[-2000:]}")
    return json.loads(done.stdout.strip().splitlines()[-1])


def trials_and_pack(family: dict, out: Path, log) -> dict:
    reports = {}
    for name, request in family["trials"].items():
        report = Path(read(request["path"])["root"]) / "report.json"
        if not report.exists():
            result = nested([str(TOOLS / "native_expanded_dispatch_v1.py"), "qualify", request["path"]])
            wait_report(report.parent, result["pid"])
        reports[name] = pin(report)
        log(kind="trial", name=name, report=reports[name])
    pack_index = Path(read(family["pack"]["path"])["root"]) / "pack.json"
    if not pack_index.exists():
        done = subprocess.run([sys.executable, "-B", str(TOOLS / "native_expanded_pack_v1.py"), "qualify",
                               family["pack"]["path"]], capture_output=True, text=True, cwd=str(TOOLS.parents[1]))
        (pack_index.parent.parent / "pack.stdout").write_text(done.stdout, encoding="utf-8")
        if done.returncode != 0:
            raise RuntimeError("pack qualification failed: " + done.stderr.strip()[-2000:])
    log(kind="pack", index=pin(pack_index))
    return {"reports": reports, "pack": pin(pack_index)}


def make_choice(campaign: driver.Campaign, value: dict, evidence: dict) -> dict:
    """One choice over every family qualified so far (the plan's inventory with this host's reports and packs),
    and one check-choice verification per family; the pins go to the campaign's choice_pins."""
    out = Path(value["root"])
    inventory = json.loads(driver.collector.read_bytes_pinned(value["inventory"]))
    keys = {tag: dispatch.workload(read(value["families"][tag]["config"]["path"]), "training", family=driver.FAMILY)
            for tag in evidence}
    choice = {"schema": CHOICE, "inventory": inventory["inventory"],
              "qualifications": [report for tag in evidence for report in evidence[tag]["reports"].values()],
              "packs": [evidence[tag]["pack"] for tag in evidence],
              "study_runs": {keys[tag]: len(g.RUNS) for tag in evidence},
              "selected": {keys[tag]: evidence[tag]["pack"] for tag in evidence}}
    name = "-".join(sorted(evidence))
    choice_pin = write_json(out / f"choice-{name}.json", choice)
    verifications = {}
    for tag in evidence:
        request = driver.request_for(campaign, "G1", value["families"][tag]["config"], out / "check" / tag / "root",
                                     out / "check" / tag / "cold")
        target = out / f"choice-{name}-verification-{tag}.json"
        target.unlink(missing_ok=True)
        request.update(choice=choice_pin, choice_verification_output=str(target))
        request.pop("choice_verification", None)
        request.pop("lease_work_seconds", None)
        request_pin = write_json(out / f"check-choice-{name}-{tag}.json", request)
        nested([str(TOOLS / "native_expanded_dispatch_v1.py"), "check-choice", request_pin["path"]])
        verifications[keys[tag]] = pin(target)
    pins = {"choice": choice_pin, "choice_verification": verifications}
    write_json(Path(campaign.raw["choice_pins"]), pins)
    return pins


def scratch(campaign: driver.Campaign, name: str, stop_after: int, out: Path) -> driver.Campaign:
    """A copy of the campaign with its own roots (state, outputs) under ``out/scratch/name``."""
    raw = copy.deepcopy(campaign.raw)
    base = out / "scratch" / name
    raw["roots"] = {key: str(base / key) for key in ("state", "cold", "retained", "checkpoints", "exposure", "hot",
                                                      "doses")} | {"hot_by_run": {}}
    if "native" in campaign.raw["roots"]:
        raw["roots"]["native"] = str(Path(campaign.raw["roots"]["native"]) / "scratch" / name)
    raw["stop_after_block"] = stop_after
    for key in ("ancestry_receipt", "placement_record", "continuation"):
        raw.pop(key, None)
    path = base / "campaign.json"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(raw, indent=1), encoding="utf-8")
    copy_ = driver.Campaign(path)
    copy_.lease = campaign.lease  # on a pod, scratch blocks keep the guard's progress record advancing too
    return copy_


def seeded(source: driver.Campaign, target: driver.Campaign, run: str, blocks: dict[int, int]) -> None:
    """Seed a scratch lineage whose block ends are ``source``'s (scratch block k = source block blocks[k]),
    once: a restarted qualification resumes the scratch lineage's own state and attempts."""
    if target.run_state_path(run).exists():
        return
    state = source.run_state(run)
    out = {"run": run, "status": "pending", "blocks": {}}
    for block, origin in blocks.items():
        entry = state["blocks"][str(origin)]
        out["blocks"][str(block)] = {"attempts": [], "done": True, "final_checkpoint": entry["final_checkpoint"],
                                     "final_adam_step": entry["final_adam_step"],
                                     "final_state_sha256": entry["final_state_sha256"], "checkpoint_state": "kept",
                                     "seeded_from": {"block": origin, "state": str(source.run_state_path(run))}}
    target.save_state(run, out)


def drive_all(pairs: list[tuple[driver.Campaign, str]]) -> None:
    threads = [threading.Thread(target=driver.drive, args=pair, name=f"{pair[1]}") for pair in pairs]
    for thread in threads:
        thread.start()
    for thread in threads:
        thread.join()
    for campaign, run in pairs:
        if campaign.run_state(run)["status"] != "complete":
            raise RuntimeError(f"{run} did not complete in {campaign.path}: {campaign.run_state(run).get('error')}")


def killer(campaign: driver.Campaign, run: str, block: int, at_update: int, record: dict) -> threading.Thread:
    """Kill the block's first attempt's native process (by its pid, never by name) once it finished at_update updates."""
    def watch():
        while True:
            entry = campaign.run_state(run)["blocks"].get(str(block), {})
            attempts = entry.get("attempts", [])
            if attempts and attempts[0].get("outcome"):
                record.setdefault("killed", False)
                return
            if attempts:
                started = Path(attempts[0]["root"]) / "started.json"
                done = driver.completed_updates(Path(attempts[0]["native"]))
                if started.exists() and done >= at_update:
                    pid = json.loads(started.read_text(encoding="utf-8"))["pid"]
                    os.kill(pid, signal.SIGTERM)
                    record.update(killed=True, pid=pid, completed_updates=done, epoch=time.time())
                    return
            time.sleep(1)
    thread = threading.Thread(target=watch, name="interrupt", daemon=True)
    thread.start()
    return thread


def checkpoint_identity(item: dict) -> dict:
    """A checkpoint's file hash, its state hash, and its content without output paths (its trajectory pins
    reduced to their SHA256s, as the launcher compares serial and parallel trials): two runs of one block in
    different roots are identical when the content and the state hash are."""
    value = json.loads(driver.collector.read_bytes_pinned(item))
    value["trajectories"] = [trajectory["sha256"] for trajectory in value["trajectories"]]
    return {"file_sha256": item["sha256"], "state_sha256": value["state_sha256"], "adam_step": value["adam_step"],
            "content_sha256": dispatch.digest(value)}


IDENTITY_KEYS = ("state_sha256", "adam_step", "content_sha256")


def end_of(campaign: driver.Campaign, run: str, block: int) -> dict:
    return checkpoint_identity(campaign.run_state(run)["blocks"][str(block)]["final_checkpoint"])


def compare(campaign: driver.Campaign, run: str, block: int, straight: dict) -> dict:
    entry = campaign.run_state(run)["blocks"][str(block)]
    attempts = [{"attempt": a["attempt"], "outcome": a.get("outcome")} for a in entry["attempts"]]
    rerun = checkpoint_identity(entry["final_checkpoint"])
    interrupted = len(attempts) > 1 and attempts[0]["outcome"] == "interrupted"
    return {"straight": straight, "rerun": rerun, "attempts": attempts, "interrupted": interrupted,
            "identical": all(straight[key] == rerun[key] for key in IDENTITY_KEYS)}


def prune_scratch(campaign: driver.Campaign, out: Path, reason: str) -> dict:
    """Discard a scratch campaign's payloads (checkpoints, cold archives, native trees, retained trajectories),
    keeping its state, configs, requests, dispatch records and exposure rows; logged in out/PRUNE.jsonl."""
    removed = []
    for path in (campaign.checkpoints, campaign.cold, campaign.retained, campaign.native("G1") / "G1"):
        if path.exists():
            size = tree_bytes(path)
            shutil.rmtree(path)
            removed.append({"path": str(path), "bytes": size})
    entry = {"at": now(), "campaign": str(campaign.path), "removed": removed, "reason": reason}
    driver.append_jsonl(out / "PRUNE.jsonl", entry)
    return entry


def run(campaign_path: Path, plan_path: Path) -> int:
    token = os.environ.get(reservations.TOKEN_ENV)
    if not token or reservations.status(token).get("token_fate") != "holds":
        raise SystemExit("qualification runs only inside a held host reservation")
    campaign, value = driver.Campaign(campaign_path), read(plan_path)
    if not campaign.raw.get("choice_pins") or campaign.stop_after != driver.QUALIFICATION_BLOCKS:
        raise SystemExit("the qualification campaign names choice_pins and stops after block 2")
    out = Path(value["root"])
    events = out / "events.jsonl"

    def log(**fields):
        driver.append_jsonl(events, {"at": now(), "epoch": time.time(), **fields})
        if campaign.lease:
            campaign.lease.touch()

    clock = None
    if campaign.host() == "runpod":
        campaign.lease = driver.LeaseProgress(campaign)
        campaign.lease.start()
    else:
        clock = driver.WallClock(campaign)  # qualification blocks count toward the study's claim time
        clock.start()
    try:
        log(kind="start", host=campaign.host())
        evidence = {"empty-pool": trials_and_pack(value["families"]["empty-pool"], out, log)}
        log(kind="choice", pins=make_choice(campaign, value, evidence))
        # Item 1: G1 alone, then the eight together (G1's rerun in a scratch campaign as the eighth).
        log(kind="wave", wave="alone", runs=["G1"])
        drive_all([(campaign, "G1")])
        log(kind="wave-end", wave="alone")
        eighth = scratch(campaign, "concurrent-g1", 2, out)
        log(kind="wave", wave="eight", runs=list(g.RUNS))
        drive_all([(eighth, "G1")] + [(campaign, run) for run in g.RUNS[1:]])
        log(kind="wave-end", wave="eight")
        repeat = {}
        for block in (1, 2):
            alone, concurrent = end_of(campaign, "G1", block), end_of(eighth, "G1", block)
            repeat[str(block)] = {"alone": alone, "concurrent": concurrent,
                                  "identical": all(alone[key] == concurrent[key] for key in IDENTITY_KEYS)}
        write_json(out / "REPEAT-G1.json", {"schema": f"{g.PREFIX}/g1-alone-vs-concurrent", "blocks": repeat})
        prune_scratch(eighth, out, "item 1 repeat compared; the main campaign keeps G1's block ends")
        # Item 3: block 2 interrupted at its midpoint, rerun from its starting checkpoint.
        rerun = scratch(campaign, "interrupt-b002", 2, out)
        seeded(campaign, rerun, "G1", {1: 1})
        kill3 = {}
        killer(rerun, "G1", 2, value["interrupt_at_update"], kill3)
        log(kind="item3-start")
        drive_all([(rerun, "G1")])
        item3 = compare(rerun, "G1", 2, end_of(campaign, "G1", 2)) | {"kill": kill3}
        receipt = {"schema": driver.RECEIPT_SCHEMA, "runtime": campaign.raw["runtime"]["sha256"],
                   "recipe": g.PREFIX, "host": campaign.host(), "run": "G1", "block": 2, **item3}
        receipt["byte_identical"] = item3["identical"] and item3["interrupted"] and kill3.get("killed") is True
        receipt_pin = write_json(out / "REGENERATION-RECEIPT.json", receipt)
        log(kind="item3", receipt=receipt_pin, byte_identical=receipt["byte_identical"])
        prune_scratch(rerun, out, "item 3 compared; the regeneration receipt keeps both identities")
        # The history family: launcher evidence on block 4's schedule (pool {1, 2}) from block 2's end.
        straight = scratch(campaign, "synthetic-straight", 4, out)
        synthetic = {1: 1, 2: 2, 3: 2}
        seeded(campaign, straight, "G1", synthetic)
        config = g.block_config(driver.block_spec(straight, "G1", 4, straight.run_state("G1"), out / "unused-native"))
        history_config = write_config(config, out / "g1-synthetic-b004-config.json")
        if "history" not in value["families"]:
            value["families"]["history"] = trial_plan(campaign, history_config, value["spec"], out, "history")
            write_json(plan_path, value)
        evidence["history"] = trials_and_pack(value["families"]["history"], out, log)
        log(kind="choice", pins=make_choice(campaign, value, evidence))
        # Item 6: the synthetic history block, straight and interrupted, discarded afterwards.
        drive_all([(straight, "G1")])
        broken = scratch(campaign, "synthetic-interrupted", 4, out)
        seeded(campaign, broken, "G1", synthetic)
        kill6 = {}
        killer(broken, "G1", 4, value["interrupt_at_update"], kill6)
        drive_all([(broken, "G1")])
        attempt = straight.run_state("G1")["blocks"]["4"]["attempts"][-1]
        drawn = sorted({item["opponent"].get("id") for update in read(attempt["config"]["path"])["iterations"]
                        for item in update["episodes"] if item["opponent"]["kind"] == "fixed"})
        item6 = compare(broken, "G1", 4, end_of(straight, "G1", 4)) | {"kill": kill6, "history_ids_drawn": drawn}
        item6["pass"] = (item6["identical"] and item6["interrupted"] and kill6.get("killed") is True
                         and drawn == ["G1-history-b001", "G1-history-b002"])
        write_json(out / "ITEM6-SYNTHETIC.json", {"schema": f"{g.PREFIX}/synthetic-history-block", **item6})
        log(kind="item6", passed=item6["pass"], drawn=drawn)
        for discarded in (straight, broken):
            prune_scratch(discarded, out, "item 6 synthetic block, discarded after its comparison (DESIGN section 6)")
        passed = receipt["byte_identical"] and item6["pass"] and all(row["identical"] for row in repeat.values())
        log(kind="done", passed=passed)
        return 0 if passed else 1
    finally:
        if campaign.lease:
            campaign.lease.stop()
        if clock:
            clock.stop()


def launch(campaign_path: Path, plan_path: Path) -> dict:
    campaign = driver.Campaign(campaign_path)
    return reservations.dispatch(
        lane=campaign.raw["lane"], work_id="g-references-qualification",
        release_condition="G(i) qualification complete or failed",
        command=driver.declared_cores(campaign) + [sys.executable, "-B", str(Path(__file__).resolve()), "run",
                                                   "--campaign", str(Path(campaign_path).resolve()),
                                                   "--plan", str(Path(plan_path).resolve())],
        cwd=str(TOOLS.parents[1]), busy_pattern=driver.base.busy_pattern(campaign))


# ---------------------------------------------------------------- record


def tree_bytes(path: Path) -> int:
    path = Path(path)
    return sum(item.stat().st_size for item in path.rglob("*") if item.is_file()) if path.exists() else 0


def block_bytes(campaign: driver.Campaign) -> list[dict]:
    """Bytes of every completed block: the native tree at completion (PRUNE), its cold archive, retained records,
    exposure rows and the checkpoint."""
    pruned = {}
    if campaign.prune_log.exists():
        for line in campaign.prune_log.read_text(encoding="utf-8").splitlines():
            row = json.loads(line)
            if "retained_manifest" in row:
                pruned[(row["run"], row["block"])] = row["bytes"]
    rows = []
    for run in g.RUNS:
        for key, entry in campaign.run_state(run)["blocks"].items():
            if not entry.get("done"):
                continue
            block = int(key)
            attempt = entry["attempts"][-1]
            label = driver.label(block)
            rows.append({"run": run, "block": block, "seconds": attempt.get("seconds"),
                         "native_bytes": pruned.get((run, block)),
                         "cold_bytes": tree_bytes(Path(attempt["cold"])),
                         "retained_bytes": tree_bytes(campaign.retained / run / f"b{block:02d}"),
                         "exposure_bytes": sum(path.stat().st_size for path in campaign.exposure.glob(f"{run}-{label}-*")),
                         "dispatch_bytes": tree_bytes(Path(attempt["root"])),
                         "checkpoint_bytes": Path(entry["final_checkpoint"]["path"]).stat().st_size})
    return rows


def record(campaign_path: Path, plan_path: Path, out: Path, cap_bytes: int = 120 * 10**9) -> dict:
    campaign, value = driver.Campaign(campaign_path), read(plan_path)
    root = Path(value["root"])
    events = [json.loads(line) for line in (root / "events.jsonl").read_text(encoding="utf-8").splitlines()]
    stamp = {event["kind"] + ("-" + event["wave"] if "wave" in event else ""): event["epoch"] for event in events}
    rows = block_bytes(campaign)
    peak_native = max(row["native_bytes"] or 0 for row in rows)
    peak_cold = max(row["cold_bytes"] for row in rows)
    per_block_kept = max(row["retained_bytes"] + row["exposure_bytes"] + row["dispatch_bytes"] for row in rows)
    checkpoint = max(row["checkpoint_bytes"] for row in rows)
    lineages = len(g.RUNS)
    cold_blocks = campaign.raw["cold_keep_blocks"]
    projection = {"hot_peak_bytes": lineages * peak_native,
                  "cold_peak_bytes": lineages * (cold_blocks + 1) * peak_cold,
                  "records_bytes_study": lineages * g.BLOCKS * per_block_kept,
                  "checkpoint_bytes_study": lineages * (len(g.PERMANENT_BLOCKS) + 2) * checkpoint,
                  "qualification_bytes_kept": tree_bytes(root)}
    projection["study_peak_bytes"] = sum(projection.values())
    value_out = {"schema": RECORD_SCHEMA, "host": campaign.host(), "plan": pin(plan_path),
                 "campaign": pin(campaign_path), "runtime": campaign.raw["runtime"],
                 "item1": {"g1_alone_seconds": stamp.get("wave-end-alone", 0) - stamp.get("wave-alone", 0),
                           "eight_seconds": stamp.get("wave-end-eight", 0) - stamp.get("wave-eight", 0),
                           "block_seconds": [{k: row[k] for k in ("run", "block", "seconds")} for row in rows],
                           "updates_per_wall_hour_eight": (lineages * 2 * g.ndb.UPDATES_PER_BLOCK * 3600
                                                           / max(1, stamp.get("wave-end-eight", 0)
                                                                 - stamp.get("wave-eight", 0)))},
                 "item2": {"blocks": rows, "projection": projection, "cap_bytes": cap_bytes,
                           "launch_refused": projection["study_peak_bytes"] > cap_bytes},
                 "item3": read(root / "REGENERATION-RECEIPT.json"),
                 "item6": read(root / "ITEM6-SYNTHETIC.json"),
                 "repeat_g1": read(root / "REPEAT-G1.json"),
                 "choice": read(campaign.raw["choice_pins"]) if campaign.raw.get("choice_pins") else None,
                 "nonnatural_blocks_1_2": nonnatural(campaign),
                 "item7": runtime_record(campaign, root),
                 "events": events}
    done = [event for event in events if event["kind"] == "done"]
    value_out["checks"] = {"run_completed": bool(done) and done[-1].get("passed") is True,
                           "item2_storage_admits_launch": not value_out["item2"]["launch_refused"],
                           "item3_regeneration_receipt": value_out["item3"].get("byte_identical") is True,
                           "item6_synthetic_history_block": value_out["item6"].get("pass") is True,
                           "item1_g1_alone_equals_concurrent": all(row["identical"] for row in
                                                                   value_out["repeat_g1"]["blocks"].values()),
                           "item7_runtime_profile": value_out["item7"]["pass"]}
    # Items 4 and 5 (the Windows evaluator and Spellbench loads) have their own records on Jack's PC; the
    # placement record cites them beside this one.
    value_out["pass"] = all(value_out["checks"].values())
    return write_json(out, value_out)


def runtime_record(campaign: driver.Campaign, root: Path) -> dict:
    """Item 7 (AMENDMENT-1.md): every block config this qualification dispatched, its hash, and that all of its
    1,620 episodes carry the profile; plus the driver's refusal of a config with one episode stripped."""
    configs = sorted({path for path in [campaign.state / "configs", *(root / "scratch").glob("*/state/configs")]
                      if path.is_dir()})
    rows = []
    for folder in configs:
        for path in sorted(folder.glob("*.json")):
            config = read(path)
            try:
                count, ok = g.require_runtime_rules(config), True
            except ValueError:
                count, ok = None, False
            rows.append({"config": pin(path), "episodes": count, "profile_on_every_episode": ok})
    probe = read(rows[0]["config"]["path"]) if rows else None
    refused = None
    if probe is not None:
        probe["iterations"][0]["episodes"][0]["episode"].pop("runtime_rules", None)
        try:
            g.require_runtime_rules(probe)
            refused = False
        except ValueError:
            refused = True
    return {"runtime_rules": g.RUNTIME_RULES, "configs": rows,
            "pass": bool(rows) and all(row["profile_on_every_episode"] for row in rows) and refused is True,
            "refusal_of_a_stripped_episode": refused}


def nonnatural(campaign: driver.Campaign) -> dict:
    """Section 4 disclosure: the non-natural fraction of blocks 1 and 2 for every lineage (a number, not a gate)."""
    result = {}
    for run in g.RUNS:
        for block in (1, 2):
            path = campaign.exposure / f"{run}-{driver.label(block)}-exposure.json"
            if path.exists():
                summary = read(path)
                discarded = len(summary["ledger"])
                kept = summary["rows"]["count"]
                result[f"{run}-b{block}"] = {"discarded": discarded, "kept": kept,
                                             "fraction": discarded / (discarded + kept)}
    return result


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("op", choices=("plan", "launch", "start", "run", "record"))
    parser.add_argument("--campaign", type=Path, required=True)
    parser.add_argument("--spec", type=Path)
    parser.add_argument("--plan", type=Path)
    parser.add_argument("--out", type=Path)
    args = parser.parse_args(argv)
    if args.op == "plan":
        print(json.dumps(plan(args.campaign, read(args.spec), args.out)))
    elif args.op == "launch":
        print(json.dumps(launch(args.campaign, args.plan)))
    elif args.op == "start":  # a lease's one launch: plan once, then take the reservation
        path = args.out / "plan.json"
        if not path.exists():
            plan(args.campaign, read(args.spec), args.out)
        print(json.dumps(launch(args.campaign, path)))
    elif args.op == "run":
        return run(args.campaign, args.plan)
    else:
        print(json.dumps(record(args.campaign, args.plan, args.out)))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
