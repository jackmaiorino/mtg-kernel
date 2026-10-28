"""Campaign driver for the G(i) fresh-init reference lineages (v1).

Runs each assigned lineage's chained 162-update blocks (g_references_v1.py),
every block through the supported launcher (``native_expanded_dispatch_v1.py
dispatch``, schedule family ``permuted-units-own-history-v1``) nested under
one held host reservation, as nine_deck_campaign_v1.py does: its reservation,
dispatch, waiting, failure classification, retention and pruning are reused
unchanged. What differs (collab LANES/g-references-20261028/DESIGN.md v1.2,
sections 3, 4 and 6, and FROZEN.md):

* Block 1 starts from the lineage's fresh initialization (``play_imports``),
  block b from block b-1's checkpoint. The history pool is the lineage's
  permanent block-end checkpoints at or below b - 2.
* Blocks run up to ``stop_after_block`` (the D_b pause, 494, unless an
  amendment continues to 741). At a dose block the driver writes
  ``doses/<run>-b<block:04d>.json`` for the evaluator.
* Failure: an interrupted block reruns from its starting checkpoint, up to
  three attempts; a block classified ``invalid`` (the baseline driver's
  INVALIDITY_MARKERS and classify_failure) or one with non-finite losses ends
  the lineage. The baseline's other exposure checks and the block-1 embedding
  gate are run and recorded, never a stop (DESIGN section 4: no early stopping
  per lineage).
* Checkpoints: block ends stay while they are one of the last two, and
  permanently at 1, 2, 4, 8, every 16th block and each dose. An older block
  end is ancestry of the doses: it is deleted only when ``ancestry_receipt``
  pins a one-block byte-identical regeneration receipt for this recipe on this
  host; until then it is kept gzip-compressed. Every change goes to PRUNE.
* On a rented pod the guard's progress.json activity advances only with
  completed native updates or a finished driver step, never on a timer alone.
* The throughput choice and its verification are read before each block from
  ``choice_pins`` (a small JSON holding both pins) when the campaign names it,
  so a refreshed inventory applies from the next block on; the dispatcher
  checks both at every dispatch.

CLI:
  launch --campaign C.json --runs G1,G2   take the host reservation and start ``run``
  run    --campaign C.json --runs G1,G2   drive lineages (inside a held reservation)
  status --campaign C.json                one line per lineage
  retire-ancestry --campaign C.json       delete compressed ancestry once a receipt is pinned
"""
from __future__ import annotations

import argparse
import gzip
import hashlib
import json
import math
import os
from pathlib import Path
import shutil
import sys
import threading
import time

import g_references_v1 as g
import host_reservation_v1 as reservations
import native_expanded_dispatch_v1 as native_dispatch
import nine_deck_campaign_v1 as base
import nine_deck_exposure_collector_v1 as collector

FAMILY = "permuted-units-own-history-v1"
MAX_ATTEMPTS = base.MAX_ATTEMPTS
DOSE_SCHEMA = "g-references-v1/dose"
RECEIPT_SCHEMA = "g-references-v1/regeneration-receipt"
now, pin, sha256_file, write_json, append_jsonl = base.now, base.pin, base.sha256_file, base.write_json, base.append_jsonl


class Campaign(base.Campaign):
    def __init__(self, path: Path):
        super().__init__(path)
        if self.raw.get("recipe") != g.PREFIX:
            raise SystemExit(f"campaign recipe is not {g.PREFIX}")
        self.doses = Path(self.raw["roots"]["doses"])
        self.stop_after = self.raw.get("stop_after_block", g.STOP_RULE_BLOCK)
        if type(self.stop_after) is not int or not 1 <= self.stop_after <= g.BLOCKS:
            raise SystemExit("stop_after_block must be a block number")

    def placement(self, run: str) -> dict:
        """The lineage's placement: its pack slot (``placement_by_run``), else the shared ``placement``."""
        return self.raw.get("placement_by_run", {}).get(run, self.raw["placement"])

    def host(self) -> str:
        return self.raw["placement"]["host"]

    def native(self, run: str) -> Path:
        """Native outputs (trajectories, the hot I/O): ``roots.native`` (a pod's container disk), else hot."""
        return Path(self.raw["roots"].get("native", self.hot(run)))

    def choice(self) -> tuple[dict, dict]:
        source = self.raw.get("choice_pins")
        if source is None:
            return self.raw["choice"], self.raw["choice_verification"]
        if not Path(source).exists():  # before the choice exists (qualification): the launcher refuses a dispatch
            return None, None
        value = json.loads(Path(source).read_text(encoding="utf-8"))
        return value["choice"], value["choice_verification"]


def label(block: int) -> str:
    return collector.block_label(block, g)


def checkpoint_path(campaign: Campaign, run: str, block: int) -> Path:
    return campaign.checkpoints / run / f"block-{block:03d}.json"


def kept_checkpoint(state: dict, block: int) -> dict:
    """A block end the schedule still needs (the chain's last end or a pool member), uncompressed."""
    entry = state["blocks"][str(block)]
    if entry.get("checkpoint_state") not in (None, "kept"):
        raise RuntimeError(f"block {block}'s checkpoint is {entry['checkpoint_state']}, not kept")
    return entry["final_checkpoint"]


def block_spec(campaign: Campaign, run: str, block: int, state: dict, native: Path) -> dict:
    raw = campaign.raw
    return {"run": run, "block": block, "runtime_decks": raw["decks"], "play_import": raw["play_imports"][run],
            "start_checkpoint": None if block == 1 else kept_checkpoint(state, block - 1),
            "history": {str(end): kept_checkpoint(state, end) for end in g.history_pool(block)},
            "update_backend": {"kind": "cpu"}, "collection_workers": campaign.placement(run)["workers"],
            "preparation_workers": campaign.placement(run)["preparation_workers"], "output_directory": str(native)}


def request_for(campaign: Campaign, run: str, config_pin: dict, root: Path, cold: Path,
                lease_seconds: float | None = None) -> dict:
    raw = campaign.raw
    choice, verification = campaign.choice()
    request = {"schema": "native-expanded-cpu-dispatch/v1", "kind": "training", "lane": raw["lane"],
               "runtime": raw["runtime"], "config": config_pin, "root": str(root), "cold_root": str(cold),
               "placement": campaign.placement(run), "storage": raw["storage"], "wall_seconds": raw["wall_seconds"],
               "schedule_family": FAMILY, "non_natural_tolerance": g.ndb.FROZEN["max_non_natural_episode_fraction"],
               "choice": choice, "choice_verification": verification}
    if lease_seconds is not None:
        request["lease_work_seconds"] = math.ceil(lease_seconds)
    return request


def block_estimate(campaign: Campaign) -> float:
    """1.25x the longest block measured on runpod, else ``lease_block_seconds``, else ``wall_seconds``."""
    measured = [attempt["seconds"] for name in g.RUNS for entry in campaign.run_state(name)["blocks"].values()
                for attempt in entry["attempts"] if attempt.get("host") == "runpod" and attempt.get("seconds")]
    if measured:
        return base.ESTIMATE_FACTOR * max(measured)
    return campaign.raw.get("lease_block_seconds", campaign.raw["wall_seconds"])


def completed_updates(native: Path) -> int:
    """Updates the native run has finished: iterations holding an update record
    (``iterations/<n>/attempt-<k>/update/update.json``)."""
    folder = Path(native) / "iterations"
    try:
        return sum(1 for entry in folder.iterdir() if any(entry.glob("*/update/update.json")))
    except OSError:
        return 0


class LeaseProgress(base.LeaseProgress):
    """progress.json for the pod's resident lease guard, advanced by completed work only.

    Activity is a newly completed native update (each run's ``iterations/*/update.json``
    count), or a driver step finishing (``touch``: a dispatch, a collection, a
    retention, a block). A run stuck outside its native part therefore goes idle
    and the guard's work_idle_seconds bound releases the pod. ``completed_updates``
    counts this driver's updates for the lease adapter's session.
    """

    def __init__(self, campaign: Campaign):
        super().__init__(campaign)
        self.counts, self.total = {}, 0

    def touch(self) -> None:
        with self.lock:
            self.activity = time.time()

    def beat(self, finished: bool = False) -> None:
        with self.lock:
            stamp, alive = time.time(), False
            for run, entry in list(self.active.items()):
                attempt = entry["attempts"][-1] if entry["attempts"] else None
                if attempt is None or "outcome" in attempt:
                    continue
                ticks = base.native_ticks(Path(attempt["root"]))
                alive = alive or (ticks is not None and ticks >= 0)
                count = completed_updates(Path(attempt["native"]))
                key = attempt["native"]
                if count > self.counts.get(key, 0):
                    self.total += count - self.counts.get(key, 0)
                    self.counts[key] = count
                    self.activity = stamp
            write_json(self.path, {"pod_id": self.pod, "epoch": stamp, "last_productive_epoch": self.productive,
                                   "last_activity_epoch": self.activity, "native_alive": alive,
                                   "queued_work": alive, "finished": finished, "completed_updates": self.total})


def touch(campaign: Campaign) -> None:
    if campaign.lease:
        campaign.lease.touch()


def discard(campaign: Campaign, run: str, block: int, attempt: dict, reason: str) -> None:
    native = Path(attempt["native"])
    if native.exists():
        size = sum(p.stat().st_size for p in native.rglob("*") if p.is_file())
        shutil.rmtree(native)
        append_jsonl(campaign.prune_log, {"at": now(), "run": run, "block": block, "attempt": attempt["attempt"],
                                          "pruned": str(native), "bytes": size, "reason": reason})


def run_block(campaign: Campaign, run: str, block: int, state: dict) -> str:
    """The baseline's run_block with this recipe's config, request and failure rules."""
    entry = state["blocks"].setdefault(str(block), {"attempts": []})
    if campaign.lease:
        campaign.lease.active[run] = entry
    if entry["attempts"]:
        last = entry["attempts"][-1]
        if last.get("outcome") == "complete":
            return finish_block(campaign, run, block, state, entry, last)
        if "outcome" not in last:
            # The driver (and with it the reservation's whole job) ended mid-block.
            last["outcome"] = "interrupted"
            discard(campaign, run, block, last, "partial output of an interrupted attempt")
            campaign.save_state(run, state)
    while len(entry["attempts"]) < MAX_ATTEMPTS:
        estimate = None
        if campaign.host() == "runpod":
            estimate = block_estimate(campaign)
            reason = base.lease_stop(estimate)
            if reason:  # at a block boundary only; the next lease resumes here
                state["stop_reason"] = reason
                campaign.save_state(run, state)
                campaign.event(run=run, block=block, kind="lease-stop", reason=reason, estimate_seconds=estimate)
                return "stopped"
        attempt = len(entry["attempts"]) + 1
        folder = campaign.hot(run) / run / f"{label(block)}-a{attempt}"
        native, root = campaign.native(run) / run / f"{label(block)}-a{attempt}" / "native", folder / "dispatch"
        cold = campaign.cold / run / f"{label(block)}-a{attempt}"
        config = g.block_config(block_spec(campaign, run, block, state, native))
        config_path = campaign.state / "configs" / f"{run}-{label(block)}-a{attempt}.json"
        config_path.parent.mkdir(parents=True, exist_ok=True)
        config_path.write_text(json.dumps(config, separators=(",", ":")) + "\n", encoding="utf-8")
        request_pin = write_json(campaign.state / "requests" / f"{run}-{label(block)}-a{attempt}.json",
                                 request_for(campaign, run, pin(config_path), root, cold, estimate))
        record = {"attempt": attempt, "root": str(root), "native": str(native), "cold": str(cold),
                  "request": request_pin, "started": now()}
        entry["attempts"].append(record)
        campaign.save_state(run, state)
        campaign.event(run=run, block=block, attempt=attempt, kind="dispatch")
        started = time.monotonic()
        result = base.dispatch_block(Path(request_pin["path"]), campaign.raw.get("compute_host_name"))
        touch(campaign)
        record["dispatch"] = result
        if result.get("state") != "dispatched":
            record["outcome"] = "spawn-" + str(result.get("state"))
            campaign.save_state(run, state)
            return "stopped"
        outcome = base.wait_block(root, result["pid"])
        record["finished"] = now()
        if outcome == "success":
            record["outcome"] = "complete"
            record["report"] = pin(root / "report.json")
            record["seconds"], record["host"] = time.monotonic() - started, campaign.host()
            campaign.save_state(run, state)
            return finish_block(campaign, run, block, state, entry, record)
        kind = base.classify_failure(root)
        record["outcome"] = kind
        discard(campaign, run, block, record, "partial output of a failed attempt (design: discarded and logged)")
        campaign.save_state(run, state)
        campaign.event(run=run, block=block, attempt=attempt, kind=kind)
        if kind == "invalid":
            entry["disposition"] = "failed: invalid block (non-natural, non-finite or NaN marker)"
            campaign.save_state(run, state)
            return "stopped"
    entry["disposition"] = "technical: interrupted three times"
    campaign.save_state(run, state)
    return "stopped"


def fresh_embedding(play_import: dict) -> dict:
    """The fresh initialization's card embedding as a checkpoint-shaped tensor (u32 bits), for the embedding gate."""
    descriptor = json.loads(collector.read_bytes_pinned(play_import))
    manifest = json.loads(collector.read_bytes_pinned(descriptor["initialization"]))
    layout = next(item for item in manifest["parameters"] if item["name"] == "card_embedding.weight")
    payload = collector.read_bytes_pinned(descriptor["parameters"])
    raw = payload[layout["byte_offset"]:layout["byte_offset"] + layout["byte_count"]]
    values = list(memoryview(raw).cast("I"))
    return {"parameters": [{"name": layout["name"], "shape": layout["shape"], "values": values}]}


def finish_block(campaign: Campaign, run: str, block: int, state: dict, entry: dict, record: dict) -> str:
    """Collect, keep the block-end checkpoint, retain, prune, retire old ancestry and write a dose record.
    Each step is saved before the next, so a restart resumes after the last completed step."""
    native = Path(record["native"])
    if "exposure" not in entry:
        for name in (f"{run}-{label(block)}-episodes.jsonl", f"{run}-{label(block)}-exposure.json"):
            (campaign.exposure / name).unlink(missing_ok=True)
        summary = collector.collect(run, block, native, Path(campaign.raw["decks"]), campaign.exposure, generator=g)
        entry["exposure"] = {"verdict": summary["verdict"],
                             "failed_checks": sorted(k for k, v in summary["checks"].items() if not v["pass"]),
                             "summary": pin(campaign.exposure / f"{run}-{label(block)}-exposure.json")}
        campaign.save_state(run, state)
        touch(campaign)
    summary = json.loads(Path(entry["exposure"]["summary"]["path"]).read_text(encoding="utf-8"))
    if "final_checkpoint" not in entry:
        completion = json.loads((native / "completion.json").read_text(encoding="utf-8"))
        final = json.loads(Path(completion["iterations"][-1]["path"]).read_text(encoding="utf-8"))
        update = json.loads(Path(final["update"]["path"]).read_text(encoding="utf-8"))
        checkpoint = Path(update["checkpoint"]["path"])
        if not sha256_file(checkpoint) == update["checkpoint"]["sha256"] == summary["final_checkpoint_sha256"]:
            raise RuntimeError("final checkpoint does not match its update record")
        kept = checkpoint_path(campaign, run, block)
        kept.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(checkpoint, kept)
        if sha256_file(kept) != update["checkpoint"]["sha256"]:
            raise RuntimeError("retained checkpoint copy differs")
        entry["final_checkpoint"] = {"path": str(kept), "sha256": update["checkpoint"]["sha256"]}
        entry["final_adam_step"] = update["adam_step"]
        entry["final_state_sha256"] = json.loads(kept.read_bytes())["state_sha256"]
        entry["checkpoint_state"] = "kept"
        campaign.save_state(run, state)
    if block == 1 and "embedding_gate" not in entry:
        gate = collector.embedding_gate(fresh_embedding(campaign.raw["play_imports"][run]),
                                        json.loads(Path(entry["final_checkpoint"]["path"]).read_bytes()),
                                        g.ndb.load_decks(Path(campaign.raw["decks"])))
        entry["embedding_gate"] = write_json(campaign.exposure / f"{run}-b001-embedding-gate.json", gate)
        entry["embedding_gate_pass"] = gate["pass"]
        campaign.save_state(run, state)
    if "retained" not in entry:
        existing = campaign.retained / run / f"b{block:02d}" / "RETAINED.json"
        if not native.exists() and existing.exists():
            entry["retained"] = pin(existing)  # pruned after retention, before the state was saved
        else:
            entry["retained"] = base.retain_block(campaign, run, block, native, summary)
        entry["cold_root"] = record["cold"]
        campaign.save_state(run, state)
        touch(campaign)
    base.prune_cold(campaign, run, block - campaign.raw["cold_keep_blocks"] + 1, state)
    retire(campaign, run, block, state)
    if block in g.DOSES.values() and "dose" not in entry:
        entry["dose"] = write_dose(campaign, run, block, entry)
        campaign.save_state(run, state)
    failed = entry["exposure"]["failed_checks"]
    if "finite_losses" in failed:
        entry["disposition"] = "failed: invalid block (non-finite losses)"
    else:
        entry["done"] = True
        entry["recorded_checks"] = failed + ([] if block != 1 or entry["embedding_gate_pass"] else ["embedding_gate"])
    campaign.save_state(run, state)
    campaign.event(run=run, block=block, kind="complete", exposure=entry["exposure"]["verdict"],
                   final_checkpoint=entry["final_checkpoint"]["sha256"], failed=failed)
    touch(campaign)
    return "complete" if entry.get("done") else "stopped"


def write_dose(campaign: Campaign, run: str, block: int, entry: dict) -> dict:
    dose = next(name for name, end in g.DOSES.items() if end == block)
    record = {"schema": DOSE_SCHEMA, "run": run, "dose": dose, "block": block,
              "updates": block * g.ndb.UPDATES_PER_BLOCK, "adam_step": entry["final_adam_step"],
              "platform": "windows" if native_dispatch.WINDOWS else "linux",
              "source": g.continued_source(campaign.raw["play_imports"][run], entry["final_checkpoint"])}
    return write_json(campaign.doses / f"{run}-b{block:04d}.json", record)


def receipt(campaign: Campaign) -> dict | None:
    """The pinned one-block regeneration receipt (qualification item 3), or None."""
    item = campaign.raw.get("ancestry_receipt")
    if not item:
        return None
    value = json.loads(collector.read_bytes_pinned(item))
    if value.get("schema") != RECEIPT_SCHEMA or not value.get("byte_identical"):
        raise SystemExit("ancestry receipt is not a byte-identical regeneration receipt")
    if value["runtime"] != campaign.raw["runtime"]["sha256"]:
        raise SystemExit("ancestry receipt was made with another runtime")
    return {"path": item["path"], "sha256": item["sha256"]}


def retire(campaign: Campaign, run: str, block: int, state: dict) -> None:
    """Block ends no longer among the last two and not permanent: compressed, or deleted under a receipt."""
    proof = receipt(campaign)
    for key, entry in sorted(state["blocks"].items(), key=lambda item: int(item[0])):
        end = int(key)
        if end > block - 2 or g.is_permanent(end) or entry.get("checkpoint_state") not in ("kept", "compressed"):
            continue
        if entry["checkpoint_state"] == "compressed" and proof is None:
            continue
        retire_one(campaign, run, end, entry, proof)
        campaign.save_state(run, state)


def retire_one(campaign: Campaign, run: str, end: int, entry: dict, proof: dict | None) -> None:
    raw = Path(entry["final_checkpoint"]["path"])
    packed = raw.with_name(raw.name + ".gz")
    nearest = max(block for block in g.PERMANENT_BLOCKS if block <= end)
    regenerate = {"from_permanent_block": nearest, "blocks_to_replay": end - nearest,
                  "configs": f"state/configs/{run}-b{nearest + 1:03d}..b{end:03d}", "runtime": campaign.raw["runtime"]}
    if entry["checkpoint_state"] == "kept":
        if proof is None:
            partial = packed.with_name(packed.name + ".partial")
            with raw.open("rb") as source, partial.open("xb") as stream,                     gzip.GzipFile(filename="", fileobj=stream, mode="wb", mtime=0) as sink:
                shutil.copyfileobj(source, sink, 1 << 20)
            os.replace(partial, packed)
            with gzip.open(packed, "rb") as stream:
                digest = hashlib.file_digest(stream, "sha256").hexdigest()
            if digest != entry["final_checkpoint"]["sha256"]:
                raise RuntimeError(f"compressed checkpoint differs: {packed}")
            size = raw.stat().st_size
            raw.unlink()
            entry["checkpoint_state"], entry["compressed_checkpoint"] = "compressed", pin(packed)
            append_jsonl(campaign.prune_log, {"at": now(), "run": run, "block": end, "compressed": str(raw),
                                              "bytes": size, "kept": entry["compressed_checkpoint"],
                                              "checkpoint_sha256": entry["final_checkpoint"]["sha256"],
                                              "reason": "ancestry of the doses, kept compressed until a "
                                                        "regeneration receipt exists (ARTIFACT-LAW clause 4)",
                                              "regenerate": regenerate})
            return
        size, path = raw.stat().st_size, raw
    else:
        size, path = packed.stat().st_size, packed
    path.unlink()
    entry["checkpoint_state"] = "deleted"
    append_jsonl(campaign.prune_log, {"at": now(), "run": run, "block": end, "pruned": str(path), "bytes": size,
                                      "checkpoint_sha256": entry["final_checkpoint"]["sha256"],
                                      "state_sha256": entry.get("final_state_sha256"),
                                      "reason": "non-permanent ancestry; byte-identical regeneration receipt pinned",
                                      "receipt": proof, "regenerate": regenerate})


def drive(campaign: Campaign, run: str) -> None:
    state = campaign.run_state(run)
    state["status"] = "running"
    state.pop("stop_reason", None)
    campaign.save_state(run, state)
    try:
        for block in range(1, campaign.stop_after + 1):
            entry = state["blocks"].get(str(block))
            if entry and entry.get("done"):
                continue
            if entry and entry.get("disposition"):
                state["status"] = "stopped"
                return
            if run_block(campaign, run, block, state) != "complete":
                state["status"] = "stopped"
                return
            if campaign.lease:
                campaign.lease.completed()
        state["status"] = "complete"
        state["complete_through_block"] = campaign.stop_after
    except Exception as error:  # recorded, never retried automatically
        state["status"] = "error"
        state["error"] = f"{type(error).__name__}: {error}"
        raise
    finally:
        if campaign.lease:
            campaign.lease.active.pop(run, None)
        campaign.save_state(run, state)
        campaign.event(run=run, kind="run-" + state["status"])


def run(campaign: Campaign, runs: list[str]) -> int:
    token = os.environ.get(reservations.TOKEN_ENV)
    if not token or reservations.status(token).get("token_fate") != "holds":
        raise SystemExit("the campaign driver runs only inside a held host reservation")
    if campaign.host() == "runpod":
        campaign.lease = LeaseProgress(campaign)
        campaign.lease.start()
    threads = [threading.Thread(target=drive, args=(campaign, name), name=name) for name in runs]
    try:
        for thread in threads:
            thread.start()
        for thread in threads:
            thread.join()
    finally:
        if campaign.lease:
            campaign.lease.stop()
    return 0 if all(campaign.run_state(name)["status"] == "complete" for name in runs) else 1


def declared_cores(campaign: Campaign) -> list[str]:
    """``host_slots_v1.py timed`` prefix when the campaign declares cores: the union of the lineages' placements
    (the baseline's rule, with per-lineage pack slots)."""
    slots = campaign.raw.get("host_slots")
    if not slots:
        return []
    spec = str(slots["cores"])
    cores = sorted(int(part) for item in spec.split(",") for part in
                   (range(int(item.split("-")[0]), int(item.split("-")[-1]) + 1) if "-" in item else [item]))
    used = sorted({cpu for run in g.RUNS for cpu in campaign.placement(run)["cpu_affinity"]})
    if cores != used:
        raise SystemExit("declared cores differ from the placements' CPU affinity")
    tool = Path(slots["tool"]["path"])
    if sha256_file(tool) != slots["tool"]["sha256"]:
        raise SystemExit("host_slots tool differs from its pin")
    return [sys.executable, "-B", str(tool), "timed", "--cores", spec, "--"]


def launch(campaign_path: Path, runs: list[str]) -> dict:
    campaign = Campaign(campaign_path)
    return reservations.dispatch(
        lane=campaign.raw["lane"], work_id="g-references-" + "-".join(runs),
        release_condition="G(i) lineages " + ",".join(runs) + f" complete through block {campaign.stop_after} or stopped",
        command=declared_cores(campaign) + [sys.executable, "-B", str(Path(__file__).resolve()), "run",
                                                 "--campaign", str(Path(campaign_path).resolve()),
                                                 "--runs", ",".join(runs)],
        cwd=str(base.TOOLS.parents[1]),
        busy_pattern=base.busy_pattern(campaign))


def retire_ancestry(campaign: Campaign) -> dict:
    """With a receipt pinned, delete compressed ancestry of every lineage (the driver does it per block too)."""
    if receipt(campaign) is None:
        raise SystemExit("no ancestry_receipt pinned")
    counts = {}
    for name in g.RUNS:
        state = campaign.run_state(name)
        done = [int(k) for k, v in state["blocks"].items() if v.get("done")]
        before = sum(1 for v in state["blocks"].values() if v.get("checkpoint_state") == "deleted")
        if done:
            retire(campaign, name, max(done), state)
        counts[name] = sum(1 for v in state["blocks"].values() if v.get("checkpoint_state") == "deleted") - before
    return {"deleted": counts}


def status_lines(campaign: Campaign) -> list[str]:
    lines = []
    for name in g.RUNS:
        state = campaign.run_state(name)
        done = sorted(int(k) for k, v in state["blocks"].items() if v.get("done"))
        lines.append(f"{name} {state['status']} blocks_done={len(done)} last={done[-1] if done else 0}"
                     f" target={campaign.stop_after}"
                     + (f" stop_reason={state['stop_reason']}" if state.get("stop_reason") else ""))
    return lines


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("op", choices=("launch", "run", "status", "retire-ancestry"))
    parser.add_argument("--campaign", type=Path, required=True)
    parser.add_argument("--runs", default="")
    args = parser.parse_args(argv)
    runs = [name for name in args.runs.split(",") if name]
    if args.op in ("launch", "run") and (not runs or any(name not in g.RUNS for name in runs)
                                         or len(set(runs)) != len(runs)):
        raise SystemExit("--runs needs distinct lineage labels G1..G8")
    if args.op == "launch":
        print(json.dumps(launch(args.campaign, runs)))
        return 0
    if args.op == "run":
        return run(Campaign(args.campaign), runs)
    if args.op == "retire-ancestry":
        print(json.dumps(retire_ancestry(Campaign(args.campaign))))
        return 0
    print("\n".join(status_lines(Campaign(args.campaign))))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
