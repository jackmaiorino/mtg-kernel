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
* Every block config is refused unless all 1,620 episodes carry
  ``runtime_rules: "resolution-boundary-v1"`` (AMENDMENT-1.md); each attempt
  records its config pin, which the run manifest pins.
* Block scratch (AMENDMENT-2.md): with ``scratch`` and ``storage.scratch_roots``
  the native trees are outside the 120 GB cap. A block is admitted only when
  every in-flight tree (at least the measured peak times the factor) fits the
  scratch volume's free space less the margin, and retained plus every
  in-flight block's projection stays within the cap; otherwise it waits while
  another block is in flight, else stops at the block boundary. A slot frees
  only once its tree's prune is verified; a tree left behind latches dispatch.
  A storage refusal or kill, or an attempt whose native process never started,
  is not one of the block's three attempts.
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
PLACEMENT_SCHEMA = "g-references-v1/placement"
QUALIFICATION_SCHEMA = "g-references-v1/qualification"
CONTINUATION_SCHEMA = "g-references-v1/continuation"
QUALIFICATION_BLOCKS = 2  # blocks 1 and 2 run under qualification; later blocks need the placement record
SYNTHETIC_BLOCK = 4  # qualification item 6: block 4's schedule (pool {1, 2}) from block 2's end, in a scratch campaign
ITEM4_SCHEMA = "g-references-v1/qualification-4"  # the Windows evaluator loads a block-2 checkpoint (collab g_eval.py)
ITEM5_SCHEMA = "g-references-v1/qualification-5"  # the Spellbench scorer loads it and completes a game
STUDY_CAP_BYTES = 120 * 10**9  # DESIGN section 6: the study's artifact usage, on either host
STUDY_WALL_SECONDS = 60 * 3600  # DESIGN section 6: wall hours of training claim time on Jack's PC
now, pin, sha256_file, write_json, append_jsonl = base.now, base.pin, base.sha256_file, base.write_json, base.append_jsonl
# AMENDMENT-2.md: block scratch admission, the prune latch and uncounted storage attempts.
SCRATCH_LOCK = threading.Lock()
SCRATCH_INFLIGHT: set[str] = set()  # admitted blocks' native trees, for every campaign of this driver process
SCRATCH_LATCH = "SCRATCH-LATCH.json"
SCRATCH_LATCHED: list[str] = []  # set once in this process by a failed prune
ADMISSION_WAIT_SECONDS = 30
UNCOUNTED = ("unspawned", "storage-refused", "storage-interrupted")
STORAGE_MARKERS = ("retained plus projected storage exceeds allowance", "target-volume reserve unavailable",
                   "no space left on device", "[errno 28]")


def read_pinned(item: dict) -> dict:
    return json.loads(collector.read_bytes_pinned(item))


def item_records(placement: dict, checkpoints: dict, imports: dict) -> str | None:
    """Items 4 and 5: typed passing records of one of the qualified block-2 checkpoints with its lineage's own
    play import; item 4 under the native runtime profile (AMENDMENT-1.md), item 5 the board's own."""
    if not placement.get("item4") or not placement.get("item5"):
        return "the placement record cites no item 4 and item 5 records"
    four, five = read_pinned(placement["item4"]), read_pinned(placement["item5"])

    def binds(run, checkpoint, play_import) -> bool:  # a qualified lineage, both identities present and equal
        return (run in checkpoints and run in imports and isinstance(checkpoint, str) and len(checkpoint) == 64
                and checkpoint == checkpoints[run] and isinstance(play_import, str) and play_import == imports[run])
    if four.get("schema") != ITEM4_SCHEMA or four.get("passed") is not True \
            or four.get("runtime_rules", {}).get("value") != g.RUNTIME_RULES \
            or not binds(four.get("run"), four.get("checkpoint", {}).get("recorded", {}).get("sha256"),
                         four.get("play_import", {}).get("recorded", {}).get("sha256")):
        return "the item 4 record is not a passing evaluator load of a qualified block-2 checkpoint"
    # DESIGN section 6 item 5: G1's block-2 checkpoint through the Spellbench scorer with its own play import.
    if five.get("schema") != ITEM5_SCHEMA or five.get("passed") is not True or five.get("run") != "G1" \
            or not binds("G1", five.get("checkpoint_sha256"), five.get("play_import_sha256")) \
            or not five.get("games_completed"):
        return "the item 5 record is not a passing Spellbench load of G1's qualified block-2 checkpoint"
    return None


class Campaign(base.Campaign):
    def __init__(self, path: Path):
        super().__init__(path)
        if self.raw.get("recipe") != g.PREFIX:
            raise SystemExit(f"campaign recipe is not {g.PREFIX}")
        self.doses = Path(self.raw["roots"]["doses"])
        self.stop_after = self.raw.get("stop_after_block", g.STOP_RULE_BLOCK)
        if type(self.stop_after) is not int or not 1 <= self.stop_after <= g.BLOCKS:
            raise SystemExit("stop_after_block must be a block number")
        cap = self.raw["storage"].get("max_logical_bytes")
        if type(cap) is not int or not 0 < cap <= STUDY_CAP_BYTES:
            raise SystemExit("the storage allowance must be at most the study's 120 GB")
        # AMENDMENT-2.md: native trees outside the cap only as admitted block scratch (roots.native under
        # storage.scratch_roots, with the measured peak, its factor and the margin).
        spec, scratch = self.raw.get("scratch"), self.raw["storage"].get("scratch_roots")
        if spec is not None or scratch is not None:
            native = self.raw["roots"].get("native")
            valid = (isinstance(spec, dict) and set(spec) == {"block_peak_bytes", "factor", "margin_fraction"}
                     and type(spec["block_peak_bytes"]) is int and spec["block_peak_bytes"] > 0
                     and isinstance(spec["factor"], (int, float)) and spec["factor"] >= 1
                     and isinstance(spec["margin_fraction"], (int, float)) and 0 <= spec["margin_fraction"] < 1
                     and isinstance(scratch, list) and native is not None
                     and any(Path(native).is_relative_to(Path(root)) for root in scratch))
            if not valid:
                raise SystemExit("block scratch needs roots.native under storage.scratch_roots and a scratch spec")

    def admits(self, block: int) -> str | None:
        """None when block may run, else why not.

        Past block 2 only on the host the pinned placement record chose, citing a passing qualification
        record there with this campaign's runtime, and passing records of qualification items 4 and 5 (the
        Windows evaluator and the Spellbench scorer). Past D_b (494) only with a pinned "continue" decision.
        The one exception is qualification's own synthetic history block (``qualification_synthetic_block``,
        set only on a scratch campaign of the qualification tool, which stops at that block).
        """
        synthetic = self.raw.get("qualification_synthetic_block")
        if synthetic is not None and block == synthetic == self.stop_after == SYNTHETIC_BLOCK:
            return None
        if block > QUALIFICATION_BLOCKS:
            item = self.raw.get("placement_record")
            if not item:
                return "no placement record: qualification blocks only"
            placement = read_pinned(item)
            if placement.get("schema") != PLACEMENT_SCHEMA or placement.get("host") != self.host():
                return "the placement record does not choose this host"
            qualification = read_pinned(placement["qualification"])
            imports = {run: item["sha256"] for run, item in self.raw["play_imports"].items()}
            if qualification.get("schema") != QUALIFICATION_SCHEMA or qualification.get("pass") is not True \
                    or qualification.get("host") != self.host() \
                    or qualification.get("runtime", {}).get("sha256") != self.raw["runtime"]["sha256"] \
                    or qualification.get("play_imports") != imports:
                return "the placement record cites no passing qualification of this runtime and these inputs here"
            checkpoints = qualification.get("block2_checkpoints") or {}
            reason = item_records(placement, checkpoints, imports)
            if reason:
                return reason
        if block > g.STOP_RULE_BLOCK:
            item = self.raw.get("continuation")
            decision = read_pinned(item) if item else {}
            if decision.get("schema") != CONTINUATION_SCHEMA or decision.get("decision") != "continue":
                return "no pinned D_b continuation decision"
        return None

    def wall_root(self) -> Path:
        """The study clock's folder: ``wall_root`` (a qualification scratch campaign shares its main campaign's),
        else state/wall."""
        return Path(self.raw.get("wall_root", self.state / "wall"))

    def wall_used(self) -> float:
        """Claim seconds used by every driver session so far (refreshed each minute)."""
        return sum(max(0.0, record["last"] - record["start"])
                   for record in (json.loads(path.read_text(encoding="utf-8"))
                                  for path in sorted(self.wall_root().glob("*.json"))))

    def wall_left(self) -> float | None:
        """On Jack's PC, the study's remaining claim seconds (RunPod: None, the lease ledger bounds it)."""
        return None if self.host() == "runpod" else STUDY_WALL_SECONDS - self.wall_used()

    def wall_stop(self) -> str | None:
        """None while another block fits the study's 60 wall hours on Jack's PC (and always on RunPod)."""
        left = self.wall_left()
        if left is None:
            return None
        measured = [attempt["seconds"] for name in g.RUNS for entry in self.run_state(name)["blocks"].values()
                    for attempt in entry["attempts"] if attempt.get("host") == self.host() and attempt.get("seconds")]
        estimate = base.ESTIMATE_FACTOR * max(measured) if measured else self.raw["wall_seconds"]
        return "study_wall_cap" if estimate + base.LEASE_MARGIN_SECONDS > left else None

    def placement(self, run: str) -> dict:
        """The lineage's placement: its pack slot (``placement_by_run``), else the shared ``placement``."""
        return self.raw.get("placement_by_run", {}).get(run, self.raw["placement"])

    def host(self) -> str:
        return self.raw["placement"]["host"]

    def native(self, run: str) -> Path:
        """Native outputs (trajectories, the hot I/O): ``roots.native`` (a pod's container disk), else hot."""
        return Path(self.raw["roots"].get("native", self.hot(run)))

    def choice(self, config_pin: dict | None = None) -> tuple[dict, dict]:
        """The choice and the verification of ``workload``'s family. choice_pins holds one choice with a
        verification per family ({workload digest: pin}): blocks with an empty history pool and blocks with
        a pool are two families of the launcher's own-history binding."""
        source = self.raw.get("choice_pins")
        if source is None:
            return self.raw["choice"], self.raw["choice_verification"]
        if not Path(source).exists():  # before the choice exists (qualification): the launcher refuses a dispatch
            return None, None
        value = json.loads(Path(source).read_text(encoding="utf-8"))
        verification = value["choice_verification"]
        if isinstance(verification, dict) and "path" not in verification:
            config = json.loads(collector.read_bytes_pinned(config_pin))
            verification = verification.get(native_dispatch.workload(config, "training", family=FAMILY))
        return value["choice"], verification


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
    choice, verification = campaign.choice(config_pin)
    request = {"schema": "native-expanded-cpu-dispatch/v1", "kind": "training", "lane": raw["lane"],
               "runtime": raw["runtime"], "config": config_pin, "root": str(root), "cold_root": str(cold),
               "placement": campaign.placement(run), "storage": raw["storage"], "wall_seconds": raw["wall_seconds"],
               "schedule_family": FAMILY, "non_natural_tolerance": g.ndb.FROZEN["max_non_natural_episode_fraction"],
               "choice": choice, "choice_verification": verification}
    if lease_seconds is not None:
        request["lease_work_seconds"] = math.ceil(lease_seconds)
    left = campaign.wall_left()
    if left is not None:  # the launcher's timeout ends the attempt inside the study's claim time
        request["wall_seconds"] = max(1, min(raw["wall_seconds"], math.floor(left - base.LEASE_MARGIN_SECONDS)))
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
    """progress.json for the pod's resident lease guard.

    Activity is a newly completed native update (each run's ``iterations/*/update.json``
    count), a driver step finishing (``touch``: a dispatch, a collection, a
    retention, a block), or the base's CPU-time evidence (``observe``: the driver
    computing, as in the exposure collection, and an in-flight attempt's
    fingerprints and archive after its native part). On 2026-10-30 those two steps
    ran over five minutes between touches after a block's last update, and the
    guard released a working Pod. A run stuck without CPU work still goes idle, and
    the guard's work_idle_seconds bound releases the pod. ``completed_updates``
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
            stamp = time.time()
            alive = self.observe(stamp)  # native_alive from in-flight attempts' own processes
            for run, entry in list(self.active.items()):
                attempt = entry["attempts"][-1] if entry["attempts"] else None
                if attempt is None or "outcome" in attempt or "finished" in attempt:
                    continue
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
    """Prune a failed or interrupted attempt's native tree after keeping its records (the top-level files and
    the baseline's small per-update records), hashed, under retained/<run>/<block>-a<k>-failed."""
    native = Path(attempt["native"])
    if native.exists():
        target = campaign.retained / run / f"{label(block)}-a{attempt['attempt']}-failed"
        kept = []
        for path in sorted(native.rglob("*")):
            relative = path.relative_to(native)
            if path.is_file() and (len(relative.parts) == 1 or path.name in base.SMALL_RECORDS):
                destination = target / relative
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(path, destination)
                digest = sha256_file(path)
                if sha256_file(destination) != digest:
                    raise RuntimeError(f"failed-attempt record copy differs: {destination}")
                kept.append({"from": str(path), "to": str(destination), "sha256": digest})
        attempt["failed_records"] = write_json(target / "RETAINED.json", {"run": run, "block": block,
                                                                          "attempt": attempt["attempt"], "files": kept})
        size = sum(p.stat().st_size for p in native.rglob("*") if p.is_file())
        shutil.rmtree(native)
        append_jsonl(campaign.prune_log, {"at": now(), "run": run, "block": block, "attempt": attempt["attempt"],
                                          "pruned": str(native), "bytes": size, "reason": reason,
                                          "retained_records": attempt["failed_records"]})


FINISH_KEYS = ("exposure", "final_checkpoint", "final_adam_step", "final_state_sha256", "checkpoint_state",
               "embedding_gate", "embedding_gate_pass")


def native_lost(campaign: Campaign, run: str, block: int, entry: dict, record: dict) -> bool:
    """A completed attempt's native tree is gone while a finishing step still reads it (collection, the kept
    checkpoint, retention)."""
    if Path(record["native"]).exists():
        return False
    retained = "retained" in entry or (campaign.retained / run / f"b{block:02d}" / "RETAINED.json").exists()
    return not ("exposure" in entry and "final_checkpoint" in entry and retained)


def storage_failure(root: Path) -> bool:
    """The attempt failed on the launcher's storage check (or a full disk), not in its block."""
    texts = []
    for name in ("failure.json", "execution.json", "stderr.log"):
        if (root / name).exists():
            texts.append((root / name).read_text(encoding="utf-8", errors="replace")[-20000:].lower())
    return any(marker in text for text in texts for marker in STORAGE_MARKERS)


def counts(attempt: dict) -> bool:
    """Whether the attempt is one of its block's three (AMENDMENT-2.md item 4). A storage refusal or kill, an
    attempt that never started its native process, and one whose native process finished cleanly before the
    launcher failed without a record (attempts from before failure.json existed) are the dispatcher's, not
    the block's."""
    outcome = attempt.get("outcome")
    if outcome in UNCOUNTED:
        return False
    if outcome != "interrupted":
        return True
    root = Path(attempt["root"])
    if storage_failure(root):
        return False
    execution = root / "execution.json"
    if execution.exists() and not (root / "failure.json").exists() and not (root / "report.json").exists():
        record = json.loads(execution.read_text(encoding="utf-8"))
        return not (record.get("exit_code") == 0 and record.get("error") is None)
    return True


def counted(entry: dict) -> int:
    return sum(counts(attempt) for attempt in entry["attempts"])


def tree_bytes(path: Path) -> int:
    total = 0
    for item in path.rglob("*") if path.exists() else []:
        try:
            if item.is_file():
                total += item.stat().st_size
        except FileNotFoundError:  # pruned mid-walk: freed
            pass
    return total


def admit(campaign: Campaign, native: Path) -> str | None:
    """Register ``native`` as an admitted block's scratch tree (None), or say why the block waits
    (AMENDMENT-2.md item 2): every in-flight tree, each at no less than the measured peak times the factor,
    fits the scratch volume's free space less the margin, and retained plus each in-flight block's projected
    retained bytes stays within the cap. Checked under one lock for every campaign of this process."""
    latch = campaign.state / SCRATCH_LATCH
    if latch.exists():
        return "latched: " + json.loads(latch.read_text(encoding="utf-8"))["reason"]
    if SCRATCH_LATCHED:
        return "latched: " + SCRATCH_LATCHED[0]
    with SCRATCH_LOCK:
        others = sorted(SCRATCH_INFLIGHT)
        storage = campaign.raw["storage"]
        try:
            native_dispatch.validate_storage(storage, (len(others) + 1) * storage["projected_additional_bytes"])
        except ValueError as error:
            return f"retained: {error}"
        spec = campaign.raw.get("scratch")
        if spec:
            peak = math.ceil(spec["block_peak_bytes"] * spec["factor"])
            sizes = [tree_bytes(Path(path)) for path in others]
            anchor = Path(campaign.raw["roots"]["native"])
            while not anchor.exists():
                anchor = anchor.parent
            disk = shutil.disk_usage(anchor)
            available = disk.free + sum(sizes) - spec["margin_fraction"] * disk.total
            need = sum(max(peak, size) for size in sizes) + peak
            if need > available:
                return f"scratch: {len(others) + 1} blocks need {need} B, {math.floor(available)} B available"
        SCRATCH_INFLIGHT.add(str(native))
    return None


def release(campaign: Campaign, run: str, block: int, native: Path) -> None:
    """A block's scratch slot frees only once its tree is verified gone (AMENDMENT-2.md item 3); a tree
    still present latches the campaign, so no block is admitted until the latch is cleared."""
    with SCRATCH_LOCK:
        if native.exists():
            reason = f"scratch tree not pruned: {native}"
            SCRATCH_LATCHED.append(reason)  # every campaign of this process (qualification's scratch copies)
            write_json(campaign.state / SCRATCH_LATCH, {"at": now(), "run": run, "block": block, "reason": reason})
            campaign.event(run=run, block=block, kind="scratch-latch", native=str(native))
            return
        SCRATCH_INFLIGHT.discard(str(native))


def hold(campaign: Campaign, run: str, block: int, state: dict, reason: str, announce: bool) -> bool:
    """After a storage refusal: wait (True) while another admitted block can free space, else stop the run
    at this block boundary with the reason (False). A latch always stops."""
    with SCRATCH_LOCK:
        idle = not SCRATCH_INFLIGHT
    if idle or reason.startswith("latched"):
        state["stop_reason"] = "storage admission: " + reason
        campaign.save_state(run, state)
        campaign.event(run=run, block=block, kind="admission-stop", reason=reason)
        return False
    if announce:
        campaign.event(run=run, block=block, kind="admission-wait", reason=reason)
    time.sleep(ADMISSION_WAIT_SECONDS)
    return True


def finished(campaign: Campaign, run: str, block: int, state: dict, entry: dict, record: dict) -> str:
    result = finish_block(campaign, run, block, state, entry, record)
    release(campaign, run, block, Path(record["native"]))
    return result


def run_block(campaign: Campaign, run: str, block: int, state: dict) -> str:
    """The baseline's run_block with this recipe's config, request and failure rules."""
    entry = state["blocks"].setdefault(str(block), {"attempts": []})
    if campaign.lease:
        campaign.lease.active[run] = entry
    if entry["attempts"]:
        last = entry["attempts"][-1]
        if last.get("outcome") == "complete" and native_lost(campaign, run, block, entry, last):
            # A pod's container disk (roots.native) went with a released pod before the block was retained:
            # an interruption, so the block reruns from its block-start checkpoint (design section 4).
            last["outcome"] = "lost"
            for key in FINISH_KEYS:
                entry.pop(key, None)
            campaign.save_state(run, state)
            campaign.event(run=run, block=block, attempt=last["attempt"], kind="native-lost")
        elif last.get("outcome") == "complete":
            return finished(campaign, run, block, state, entry, last)
        if "outcome" not in last:
            # The driver (and with it the reservation's whole job) ended mid-block; an attempt whose native
            # process never started was refused at dispatch.
            started = (Path(last["root"]) / "started.json").exists()
            last["outcome"] = "interrupted" if started else "unspawned"
            discard(campaign, run, block, last, "partial output of an interrupted attempt")
            release(campaign, run, block, Path(last["native"]))
            campaign.save_state(run, state)
    waiting = False
    while counted(entry) < MAX_ATTEMPTS:
        estimate = None
        if campaign.host() == "runpod":
            estimate = block_estimate(campaign)
            reason = base.lease_stop(estimate)
            if reason:  # at a block boundary only; the next lease resumes here
                state["stop_reason"] = reason
                campaign.save_state(run, state)
                campaign.event(run=run, block=block, kind="lease-stop", reason=reason, estimate_seconds=estimate)
                return "stopped"
        reason = campaign.wall_stop()  # every fresh native attempt fits the study's claim time
        if reason:
            state["stop_reason"] = reason
            campaign.save_state(run, state)
            campaign.event(run=run, block=block, kind="gate-stop", reason=reason)
            return "stopped"
        attempt = len(entry["attempts"]) + 1
        folder = campaign.hot(run) / run / f"{label(block)}-a{attempt}"
        native, root = campaign.native(run) / run / f"{label(block)}-a{attempt}" / "native", folder / "dispatch"
        cold = campaign.cold / run / f"{label(block)}-a{attempt}"
        reason = admit(campaign, native)
        if reason:  # no attempt is made
            if not hold(campaign, run, block, state, reason, not waiting):
                return "stopped"
            waiting = True
            continue
        waiting = False
        config = g.block_config(block_spec(campaign, run, block, state, native))
        g.require_runtime_rules(config)  # amendment 1: refuse a block with any episode outside the profile
        config_path = campaign.state / "configs" / f"{run}-{label(block)}-a{attempt}.json"
        config_path.parent.mkdir(parents=True, exist_ok=True)
        config_path.write_text(json.dumps(config, separators=(",", ":")) + "\n", encoding="utf-8")
        request_pin = write_json(campaign.state / "requests" / f"{run}-{label(block)}-a{attempt}.json",
                                 request_for(campaign, run, pin(config_path), root, cold, estimate))
        record = {"attempt": attempt, "root": str(root), "native": str(native), "cold": str(cold),
                  "config": pin(config_path), "request": request_pin, "started": now()}
        entry["attempts"].append(record)
        campaign.save_state(run, state)
        campaign.event(run=run, block=block, attempt=attempt, kind="dispatch")
        started = time.monotonic()
        try:
            result = base.dispatch_block(Path(request_pin["path"]), campaign.raw.get("compute_host_name"))
        except RuntimeError as error:
            release(campaign, run, block, native)
            if not any(marker in str(error).lower() for marker in STORAGE_MARKERS):
                record["outcome"] = "unspawned"
                campaign.save_state(run, state)
                raise
            record["outcome"] = "storage-refused"  # uncounted
            campaign.save_state(run, state)
            campaign.event(run=run, block=block, attempt=attempt, kind="storage-refused")
            if not hold(campaign, run, block, state, "the launcher's storage check refused the dispatch", True):
                return "stopped"
            continue
        touch(campaign)
        record["dispatch"] = result
        if result.get("state") != "dispatched":
            record["outcome"] = "spawn-" + str(result.get("state"))
            release(campaign, run, block, native)
            campaign.save_state(run, state)
            return "stopped"
        outcome = base.wait_block(root, result["pid"])
        record["finished"] = now()
        if outcome == "success":
            record["outcome"] = "complete"
            record["report"] = pin(root / "report.json")
            record["seconds"], record["host"] = time.monotonic() - started, campaign.host()
            campaign.save_state(run, state)
            return finished(campaign, run, block, state, entry, record)
        kind = base.classify_failure(root)
        if kind == "interrupted" and storage_failure(root):
            kind = "storage-interrupted"  # uncounted: the block waits for admission and reruns
        record["outcome"] = kind
        discard(campaign, run, block, record, "partial output of a failed attempt (design: discarded and logged)")
        release(campaign, run, block, native)
        campaign.save_state(run, state)
        campaign.event(run=run, block=block, attempt=attempt, kind=kind)
        if kind == "invalid":
            entry["disposition"] = "failed: invalid block (non-natural, non-finite or NaN marker)"
            campaign.save_state(run, state)
            return "stopped"
        if kind == "storage-interrupted" and not hold(campaign, run, block, state,
                                                      "the launcher's storage check ended the attempt", True):
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
    accepted = (value.get("schema") == RECEIPT_SCHEMA and value.get("byte_identical") is True
                and value.get("interrupted") is True and value.get("recipe") == g.PREFIX
                and value.get("host") == campaign.host())
    if not accepted:
        raise SystemExit("ancestry receipt is not this recipe's interrupted-block regeneration receipt on this host")
    if value["runtime"] != campaign.raw["runtime"]["sha256"]:
        raise SystemExit("ancestry receipt was made with another runtime")
    return {"path": item["path"], "sha256": item["sha256"]}


def retire(campaign: Campaign, run: str, block: int, state: dict) -> None:
    """Block ends no longer among the last two and not permanent: compressed, or deleted under a receipt.

    Restart-safe: a step's state is saved before the file it retires is removed, and a pass first finishes
    any step a stopped driver left half done (a stale partial, a raw file beside its verified copy, a
    pending deletion)."""
    proof = receipt(campaign)
    for key, entry in sorted(state["blocks"].items(), key=lambda item: int(item[0])):
        end = int(key)
        if end > block - 2 or g.is_permanent(end) or entry.get("checkpoint_state") not in ("kept", "compressed", "deleting"):
            continue
        if entry["checkpoint_state"] == "compressed" and proof is None:
            if not entry.get("compression_logged") or Path(entry["final_checkpoint"]["path"]).exists():
                # finish the step a stopped pass left (the raw file, or its PRUNE entry)
                retire_one(campaign, run, end, entry, proof, lambda: campaign.save_state(run, state))
            continue
        retire_one(campaign, run, end, entry, proof, lambda: campaign.save_state(run, state))


def prune_once(campaign: Campaign, key: str, entry: dict) -> None:
    """Append a PRUNE entry unless one with this key is already among the log's last entries (a retried step)."""
    log = campaign.prune_log
    if log.exists():
        with log.open("rb") as stream:
            stream.seek(max(0, log.stat().st_size - (1 << 20)))
            tail = stream.read().decode("utf-8", errors="replace")
        if f'"key": "{key}"' in tail:
            return
    append_jsonl(log, {"key": key, **entry})


def gunzip_sha256(path: Path) -> str:
    with gzip.open(path, "rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def retire_one(campaign: Campaign, run: str, end: int, entry: dict, proof: dict | None, save) -> None:
    raw = Path(entry["final_checkpoint"]["path"])
    packed = raw.with_name(raw.name + ".gz")
    expected = entry["final_checkpoint"]["sha256"]
    nearest = max(block for block in g.PERMANENT_BLOCKS if block <= end)
    regenerate = {"from_permanent_block": nearest, "blocks_to_replay": end - nearest,
                  "configs": f"state/configs/{run}-b{nearest + 1:03d}..b{end:03d}", "runtime": campaign.raw["runtime"]}
    if entry["checkpoint_state"] in ("kept", "compressed") and proof is None:
        partial = packed.with_name(packed.name + ".partial")
        partial.unlink(missing_ok=True)
        if not (packed.exists() and gunzip_sha256(packed) == expected):
            with raw.open("rb") as source, partial.open("xb") as stream:
                with gzip.GzipFile(filename="", fileobj=stream, mode="wb", mtime=0) as sink:
                    shutil.copyfileobj(source, sink, 1 << 20)
            os.replace(partial, packed)
            if gunzip_sha256(packed) != expected:
                raise RuntimeError(f"compressed checkpoint differs: {packed}")
        if entry["checkpoint_state"] == "kept":
            entry["checkpoint_state"], entry["compressed_checkpoint"] = "compressed", pin(packed)
            entry["raw_bytes"] = raw.stat().st_size
            save()
        raw.unlink(missing_ok=True)
        prune_once(campaign, f"{run}-b{end:03d}-compressed", {
            "at": now(), "run": run, "block": end, "compressed": str(raw), "bytes": entry.get("raw_bytes"),
            "kept": entry["compressed_checkpoint"], "checkpoint_sha256": expected,
            "state_sha256": entry.get("final_state_sha256"),
            "reason": "ancestry of the doses, kept compressed until a regeneration receipt exists "
                      "(ARTIFACT-LAW clause 4)", "regenerate": regenerate})
        entry["compression_logged"] = True
        save()
        return
    if entry["checkpoint_state"] != "deleting":
        entry["checkpoint_state"] = "deleting"
        entry["deleting_bytes"] = sum(path.stat().st_size for path in (raw, packed) if path.exists())
        save()
    raw.unlink(missing_ok=True)
    packed.unlink(missing_ok=True)
    prune_once(campaign, f"{run}-b{end:03d}-deleted", {
        "at": now(), "run": run, "block": end, "pruned": [str(raw), str(packed)], "bytes": entry.get("deleting_bytes"),
        "checkpoint_sha256": expected, "state_sha256": entry.get("final_state_sha256"),
        "reason": "non-permanent ancestry; byte-identical regeneration receipt pinned", "receipt": proof,
        "regenerate": regenerate})
    entry["checkpoint_state"] = "deleted"
    save()


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
            if not (entry and entry["attempts"]):  # a started block always finishes or reruns
                reason = campaign.admits(block) or campaign.wall_stop()
                if reason:
                    state["status"], state["stop_reason"] = "stopped", reason
                    campaign.event(run=run, block=block, kind="gate-stop", reason=reason)
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
    clock = None
    if campaign.host() == "runpod":
        campaign.lease = LeaseProgress(campaign)
        campaign.lease.start()
    else:
        clock = WallClock(campaign)
        clock.start()
    threads = [threading.Thread(target=drive, args=(campaign, name), name=name) for name in runs]
    try:
        for thread in threads:
            thread.start()
        for thread in threads:
            thread.join()
    finally:
        if campaign.lease:
            campaign.lease.stop()
        if clock:
            clock.stop()
    return 0 if all(campaign.run_state(name)["status"] == "complete" for name in runs) else 1


class WallClock:
    """This session's claim time, state/wall/<start>-<pid>.json {start, last}, refreshed every minute and at exit
    (a session that dies counts to its last refresh)."""

    def __init__(self, campaign: Campaign, interval: float = 60):
        self.start_epoch = time.time()
        self.path = campaign.wall_root() / f"{int(self.start_epoch)}-{os.getpid()}.json"
        self.interval, self.done = interval, threading.Event()
        self.thread = threading.Thread(target=self.loop, name="wall-clock", daemon=True)

    def write(self) -> None:
        write_json(self.path, {"start": self.start_epoch, "last": time.time(), "pid": os.getpid()})

    def start(self) -> None:
        self.write()
        self.thread.start()

    def loop(self) -> None:
        while not self.done.wait(self.interval):
            self.write()

    def stop(self) -> None:
        self.done.set()
        self.thread.join()
        self.write()


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
