"""Deck-variant panels for brewing prototype B0 (v1).

Collab design: LANES/brewing-b0-20261020/DESIGN.md (v6). A sibling of
nine_deck_public_panel_v1: candidate decks take the focal seat ("own"), the
nine frozen field decks the other seat, and every deck in a stage plays the
identical case list (the pairing). All play is game-one-only KeepSevenV2 in
the pinned public evaluator.

Subcommands (JSON in and out; outputs are written once, never overwritten):
  eligible  in-cards and exposure labels from the pool, registry and audit
  cases     a stage's case schedule from the frozen seed mapping
  plan      evaluator requests for a stage
  qualify   serial versus parallel parity and throughput (inside a claim)
  run       the yielding dispatcher (inside a host_slots claim)
  archive   seal completed requests into verified chunks; free raw output
            whose chunk has two verified copies
  pull      copy sealed chunks to the second machine and acknowledge them
  rows      one row per case and deck
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import struct
import subprocess
import sys
import threading
import time
import zipfile
from pathlib import Path

SCHEMA = "deck-variant-panel/v1"
RECEIPT_SCHEMA = "deck-variant-panel-qualification/v1"
SEED_DOMAIN = b"kernel-b0-case-seed/v1"
MAX_DECISIONS = 10000
MAX_POLICY_STEPS = 200000
OPPONENTS = 9
YIELD_EXIT = 75  # temporary: the host is wanted elsewhere; retry later
CAP_EXIT = 76  # terminal: the cumulative CPU cap is reached
LANE = "brewing-b0"


# ---------------------------------------------------------------- files


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with Path(path).open("rb") as stream:
        for block in iter(lambda: stream.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def pin(path: Path) -> dict:
    return {"path": str(Path(path)).replace("\\", "/"), "sha256": sha256_file(path)}


def checked(ref: dict) -> Path:
    path = Path(ref["path"])
    if sha256_file(path) != ref["sha256"]:
        raise ValueError(f"pinned file changed: {path}")
    return path


def require(ok, reason: str) -> None:
    if not ok:
        raise ValueError(reason)


def write_json(path: Path, value, *, exclusive: bool = True) -> dict:
    """Write through a temporary file and an atomic rename; ``exclusive`` refuses an existing target."""
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    require(not (exclusive and path.exists()), f"refusing to overwrite {path}")
    text = json.dumps(value, indent=1, sort_keys=True, allow_nan=False) + "\n"
    tmp = path.with_name(path.name + f".tmp-{os.getpid()}-{threading.get_ident()}")
    with tmp.open("x", encoding="utf-8", newline="\n") as stream:
        stream.write(text)
    os.replace(tmp, path)
    return pin(path)


def read_json(path: Path):
    return json.loads(Path(path).read_text(encoding="utf-8"))


def check_pins(value) -> None:
    """Verify every {path, sha256} pin inside a model descriptor."""
    if isinstance(value, list):
        for item in value:
            check_pins(item)
    elif isinstance(value, dict):
        if {"path", "sha256"} <= set(value):
            checked(value)
        else:
            for item in value.values():
                check_pins(item)


# ---------------------------------------------------------------- eligibility (section 4)

COLOR_LETTERS = set("WUBRG")


def mana_colors(cost: str | None) -> set[str]:
    """Color letters named in any mana symbol; hybrid halves count, Phyrexian P does not."""
    colors = set()
    for symbol in re.findall(r"\{([^}]+)\}", cost or ""):
        colors.update(part for part in symbol.split("/") if part in COLOR_LETTERS)
    return colors


def eligible_in_cards(cards: list[dict], pool: dict, deck_id: str, audit: dict,
                      pilots: dict[str, str]) -> dict:
    """Section 4's generic mana-cost compatibility filter plus audit-derived exposure labels."""
    by_name = {card["name"]: (index, card) for index, card in enumerate(cards)}
    names, focal = set(), None
    for deck in pool["decks"]:
        for zone in ("mainboard", "sideboard"):
            names.update(row["name"] for row in deck[zone]["cards"])
        if deck["id"] == deck_id:
            focal = {row["name"] for zone in ("mainboard", "sideboard") for row in deck[zone]["cards"]}
    require(focal is not None, f"pool lacks {deck_id}")
    for name, digest in pilots.items():
        source = audit["models"][name]["source"]["checkpoint"]["sha256"]
        require(source == digest, f"audit model {name} is bound to {source}, not {digest}")
    unupdated = {name: {row["card_id"] for row in audit["models"][name]["rows"]
                        if row["no_retained_embedding_update"]} for name in pilots}
    in_cards, excluded = [], []
    for name in sorted(names):
        index, card = by_name[name]
        if name in focal:
            ok = True
        elif card["is_land"]:
            ok = bool(card["produces_mana"]) and set(card["produces_mana"]) <= {"R", "C"}
        else:
            ok = mana_colors(card["mana_cost"]) <= {"R"}
        if not ok:
            excluded.append(name)
            continue
        primary = all(index not in rows for rows in unupdated.values())
        in_cards.append({"card_id": index, "primary": primary, "name": name})
    return {"in_cards": [{"card_id": row["card_id"], "primary": row["primary"]} for row in in_cards],
            "names": {str(row["card_id"]): row["name"] for row in in_cards},
            "exposure_limited": sorted(row["name"] for row in in_cards if not row["primary"]),
            "excluded_count": len(excluded), "pool_unique": len(names)}


# ---------------------------------------------------------------- cases (section 5)


def case_seed(master_seed: int, stage: str, family: str, opponent: int, block: int, seat: int,
              starting_player: int) -> int:
    payload = (SEED_DOMAIN + struct.pack(">Q", master_seed) + stage.encode() + b"\0" + family.encode()
               + b"\0" + struct.pack(">BIBB", opponent, block, seat, starting_player))
    return struct.unpack(">Q", hashlib.sha256(payload).digest()[:8])[0]


def stage_cases(master_seed: int, stage: str, family: str, blocks: int, pilots: list[str],
                mode: str) -> list[dict]:
    """Balanced blocks of four (seat x starting player) per opponent. ``alternate`` gives block b to
    pilots[b % len(pilots)]; ``all`` gives every case to every pilot (shared case list)."""
    require(mode in ("alternate", "all") and blocks > 0 and pilots, "bad case schedule")
    cases = []
    for opponent in range(OPPONENTS):
        for block in range(blocks):
            for seat in (0, 1):
                for start in (0, 1):
                    seed = case_seed(master_seed, stage, family, opponent, block, seat, start)
                    owners = [pilots[block % len(pilots)]] if mode == "alternate" else list(pilots)
                    cases.append({"opponent": opponent, "block": block, "seat": seat,
                                  "starting_player": start, "seed": seed, "pilots": owners})
    require(len({case["seed"] for case in cases}) == len(cases), "seed collision inside a stage")
    return cases


# ---------------------------------------------------------------- plans


def deck_entry(deck: dict) -> dict:
    return {"label": deck["label"], "mainboard": list(deck["mainboard"]), "sideboard": list(deck["sideboard"])}


def case_match(case: dict, focal: dict, field: dict) -> dict:
    seats = [focal, field] if case["seat"] == 0 else [field, focal]
    return {"config": {"deck_ids": [seats[0]["label"], seats[1]["label"]], "seed": case["seed"],
                       "game_one_chooser": case["starting_player"], "max_physical_decisions": MAX_DECISIONS,
                       "max_physical_games": 3, "max_policy_steps": MAX_POLICY_STEPS,
                       "opening_protocol": "keep_seven_v2"},
            "registered": [deck_entry(seats[0]), deck_entry(seats[1])], "postboard": None}


def build_plan(stage: str, decks: list[dict], field: list[dict], cases: list[dict], sources: dict,
               out: Path, native_root: str, request_size: int) -> dict:
    """Requests grouped by (focal pilot, seat); inside a group, cases in schedule order and decks in
    list order. The first deck is the incumbent."""
    out = Path(out)
    require(not out.exists(), "plan output must be fresh")
    require(len(field) == OPPONENTS and decks and decks[0]["role"] == "incumbent", "bad deck lists")
    labels = [deck["label"] for deck in decks]
    require(len(set(labels)) == len(labels), "deck labels must be unique")
    pilots = sorted({pilot for case in cases for pilot in case["pilots"]})
    require(set(pilots) <= set(sources["pilots"]), "case pilot lacks a source")
    requests = []
    for pilot in pilots:
        for seat in (0, 1):
            items = [(case, deck_index) for case in cases if pilot in case["pilots"] and case["seat"] == seat
                     for deck_index in range(len(decks))]
            for number, start in enumerate(range(0, len(items), request_size)):
                part = items[start:start + request_size]
                name = f"{pilot}-seat{seat}-r{number:04d}"
                srcs = [sources["pilots"][pilot], sources["opponent"]]
                if seat == 1:
                    srcs.reverse()
                request = {"sources": srcs, "cross_generation_evaluation": True, "capture_decisions": False,
                           "game_one_only": True, "output_directory": f"{native_root}/{name}",
                           "matches": [case_match(case, decks[d], field[case["opponent"]]) for case, d in part]}
                ref = write_json(out / "requests" / f"{name}.json", request)
                requests.append({"name": name, "pilot": pilot, "seat": seat, "request": ref,
                                 "items": [{"case": [c["opponent"], c["block"], c["seat"], c["starting_player"]],
                                            "seed": c["seed"], "deck": d} for c, d in part]})
    plan = {"schema": SCHEMA, "stage": stage, "decks": decks, "field_labels": [deck["label"] for deck in field],
            "sources": sources, "cases": len(cases), "pilots": pilots, "requests": requests,
            "matches": sum(len(item["items"]) for item in requests)}
    write_json(out / "plan.json", plan)
    return plan


# ---------------------------------------------------------------- native execution


def process_cpu_seconds(child: subprocess.Popen) -> float | None:
    """Kernel plus user CPU of a finished child (Windows), else None."""
    if os.name != "nt":
        return None
    import ctypes
    from ctypes import wintypes
    times = [wintypes.FILETIME() for _ in range(4)]
    ok = ctypes.windll.kernel32.GetProcessTimes(wintypes.HANDLE(int(child._handle)),
                                                *[ctypes.byref(t) for t in times])
    if not ok:
        return None
    to_s = lambda t: ((t.dwHighDateTime << 32) | t.dwLowDateTime) / 1e7
    return to_s(times[2]) + to_s(times[3])


def run_native(executable: str, request_path: Path) -> dict:
    log = Path(request_path).with_suffix(".log")
    output = Path(read_json(request_path)["output_directory"])
    output.parent.mkdir(parents=True, exist_ok=True)
    started = time.monotonic()
    with log.open("a", encoding="utf-8") as stream:
        child = subprocess.Popen([executable, str(request_path)], stdout=stream, stderr=subprocess.STDOUT,
                                 creationflags=getattr(subprocess, "BELOW_NORMAL_PRIORITY_CLASS", 0),
                                 env=dict(os.environ, CUDA_VISIBLE_DEVICES=""))
        code = child.wait()
        cpu = process_cpu_seconds(child)
    return {"exit": code, "wall_seconds": time.monotonic() - started, "cpu_seconds": cpu}


def match_files(output: Path) -> list[Path]:
    return sorted(Path(output).glob("match-*.json"))


def valid_prefix(output: Path, expected: list[dict]) -> int:
    """Count leading match files that parse and match their scheduled case; set aside the rest."""
    count = 0
    for index, path in enumerate(match_files(output)):
        ok = path.name == f"match-{index:06}.json" and index < len(expected)
        if ok:
            try:
                ok = read_json(path)["match"] == expected[index]
            except (ValueError, KeyError):
                ok = False
        if not ok:
            for rest in match_files(output)[index:]:
                rest.rename(rest.with_name(rest.name + f".setaside-{int(time.time())}"))
            break
        count += 1
    return count


def execute_request(executable: str, plan_dir: Path, item: dict, ledger: "Ledger", max_crashes: int = 3) -> dict:
    """Run one request to full coverage, resuming at the first unfinished case.

    Segment 0 is the request; segment k a continuation over the remaining matches. A failure record
    names a scientific failure (decision cap or other non-natural end), which is data: the case is
    recorded and play continues with the next one. A missing completion and failure is an
    environmental interruption: completed match files are kept and the next segment starts at the
    first unfinished case."""
    request_path = checked(item["request"])
    state_path = request_path.with_suffix(".state.json")
    if state_path.exists():
        return read_json(state_path)
    request = read_json(request_path)
    matches, base = request["matches"], request["output_directory"]
    segments, failures, interruptions, start, k, crashes = [], [], [], 0, 0, 0
    current = request_path

    def next_segment(new_start: int) -> Path:
        path = request_path.with_name(f"{request_path.stem}-c{k}.json")
        continuation = dict(request, matches=matches[new_start:], output_directory=f"{base}-c{k}")
        if path.exists():
            require(read_json(path) == continuation, f"existing continuation changed: {path}")
        else:
            write_json(path, continuation)
        return path

    while start < len(matches):
        segment_request = read_json(current)
        output = Path(segment_request["output_directory"])
        if not (output / "completion.json").exists() and not (output / "failure.json").exists():
            done = valid_prefix(output, segment_request["matches"]) if output.exists() else 0
            if done:  # interrupted after progress: keep the prefix, resume at the first unfinished case
                segments.append({"request": pin(current), "output_directory": str(output).replace("\\", "/"),
                                 "start": start, "count": done, "end": "interrupted"})
                interruptions.append({"segment": k, "kept": done})
                start += done
                k += 1
                if start < len(matches):
                    current = next_segment(start)
                continue
            if output.exists():
                output.rename(output.with_name(output.name + f".partial-{int(time.time())}"))
            reservation = ledger.begin(item["name"], k, len(segment_request["matches"]))
            result = run_native(executable, current)
            ledger.settle(reservation, result)
            if result["exit"] != 0 and not (output / "failure.json").exists() \
                    and not (output / "completion.json").exists():
                crashes += 1
                require(crashes <= max_crashes, f"{current} crashed {crashes} times without a failure record")
            continue
        done = len(match_files(output))
        if (output / "completion.json").exists():
            completion = read_json(output / "completion.json")
            require(done == len(segment_request["matches"]) == completion["matches"]
                    and [sha256_file(p) for p in match_files(output)] == completion["match_sha256"],
                    f"{output}: completion does not cover its match files")
            segments.append({"request": pin(current), "output_directory": str(output).replace("\\", "/"),
                             "start": start, "count": done, "end": "complete"})
            start += done
            break
        error = read_json(output / "failure.json")["error"]
        index = start + done
        seed = matches[index]["config"]["seed"]
        require(f"match seed {seed}," in error, f"{output}: failure does not name match {index}")
        segments.append({"request": pin(current), "output_directory": str(output).replace("\\", "/"),
                         "start": start, "count": done, "end": "failure"})
        failures.append({"index": index, "seed": seed, "error": error[-400:]})
        start = index + 1
        if start == len(matches):
            break
        k += 1
        current = next_segment(start)
    state = {"request": item["request"], "segments": segments, "failures": failures,
             "interruptions": interruptions}
    write_json(state_path, state)
    return state


# ---------------------------------------------------------------- ledger and yielding


class CapReached(Exception):
    """Admitting this native invocation would exceed the cumulative CPU cap."""


class Ledger:
    """One persistent CPU ledger (JSON lines) shared by qualification, runs and recovery.

    Every native invocation first appends a reservation at its projected CPU and then a settlement
    with the measured CPU. A reservation without a settlement (a killed dispatcher) stays charged at
    its projection; a settlement without a measurement keeps the projection too. Rows written before
    reservations existed (no ``op``) count at their measured or projected CPU."""

    def __init__(self, path: Path, cap_seconds: float, per_match_seconds: float):
        self.path, self.cap, self.per_match = Path(path), cap_seconds, per_match_seconds
        self.lock = threading.Lock()

    def _append(self, row: dict) -> None:
        self.path.parent.mkdir(parents=True, exist_ok=True)
        with self.path.open("a", encoding="utf-8", newline="\n") as stream:
            stream.write(json.dumps(row, sort_keys=True) + "\n")
            stream.flush()
            os.fsync(stream.fileno())

    def spent(self) -> float:
        if not self.path.exists():
            return 0.0
        reserved, settled, legacy = {}, {}, 0.0
        for line in self.path.read_text(encoding="utf-8").splitlines():
            row = json.loads(line)
            op = row.get("op")
            if op == "reserve":
                reserved[row["id"]] = row["projected_cpu_seconds"]
            elif op == "settle":
                settled[row["id"]] = row["cpu_seconds"]
            else:
                legacy += row["cpu_seconds"] if row["cpu_seconds"] is not None else row["projected_cpu_seconds"]
        total = legacy
        for rid, projected in reserved.items():
            measured = settled.get(rid)
            total += measured if measured is not None else projected
        return total

    def begin(self, key: str, segment: int, matches: int, kind: str = "run", per_match: float | None = None) -> str:
        projected = matches * (self.per_match if per_match is None else per_match)
        with self.lock:
            if self.spent() + projected > self.cap:
                raise CapReached(f"{key}: {projected:.0f} CPU-s would pass the cap")
            rid = f"{time.time_ns()}-{os.getpid()}-{threading.get_ident()}"
            self._append({"op": "reserve", "id": rid, "t": time.time(), "kind": kind, "request": key,
                          "segment": segment, "matches": matches, "projected_cpu_seconds": projected})
        return rid

    def settle(self, rid: str, result: dict) -> None:
        with self.lock:
            self._append({"op": "settle", "id": rid, "t": time.time(), "exit": result["exit"],
                          "wall_seconds": result["wall_seconds"], "cpu_seconds": result["cpu_seconds"]})


def tree_bytes(roots: list[Path]) -> int:
    total = 0
    for root in roots:
        for folder, _, files in os.walk(root):
            for name in files:
                try:
                    total += os.path.getsize(os.path.join(folder, name))
                except OSError:
                    pass
    return total


def yield_reason(slots_tool: str | None, stop_file: Path, footprint_roots: list[Path], footprint_limit: int,
                 lane: str = LANE, interactive_threshold: float = 0.0) -> str | None:
    if stop_file.exists():
        return "stop file"
    if interactive_threshold:
        import interactive_load_v1 as interactive
        if interactive.interactive_cores(5.0) > interactive_threshold \
                and interactive.interactive_cores(5.0) > interactive_threshold:
            return "interactive desktop use"
    if footprint_limit and tree_bytes(footprint_roots) > footprint_limit:
        return "hot footprint above limit"
    if slots_tool:
        try:
            status = json.loads(subprocess.run([sys.executable, "-B", slots_tool, "status"], capture_output=True,
                                               text=True, timeout=60).stdout)
        except (ValueError, subprocess.TimeoutExpired):
            return "host_slots status unreadable"
        if status.get("timed", {}).get("held"):
            return "timed reservation held"
        if any(ticket.get("lane") != lane for ticket in status.get("queue", [])):
            return "another job is queued"
    return None


def dispatch(plan_dir: Path, executable: str, workers: int, ledger: Ledger, slots_tool: str | None,
             stop_file: Path, footprint_roots: list[Path], footprint_limit: int, check_seconds: float = 15.0,
             interactive_threshold: float = 0.0, archive_dir: Path | None = None, seal_seconds: float = 60.0,
             max_files: int = 20000, max_bytes: int = int(1.5 * 2**30), ack_wait_seconds: float = 180.0,
             raw_root: Path | None = None) -> str:
    """Admit requests while nothing else wants the host; on a yield signal (or the CPU cap) admit
    nothing more, let in-flight requests finish (bounded by their size) and return 'yielded: ...'.
    Between batches, completed requests are sealed into verified chunks and raw output of
    acknowledged chunks is freed, so the hot footprint stays bounded while the stage runs."""
    plan = read_json(Path(plan_dir) / "plan.json")
    pending = [item for item in plan["requests"]
               if not Path(item["request"]["path"]).with_suffix(".state.json").exists()]
    lock, running, errors, capped, progress = threading.Lock(), set(), [], [], []
    by_name = {item["name"]: item for item in plan["requests"]}
    gated_since, cap_refusals = None, 0

    def one(item):
        try:
            execute_request(executable, plan_dir, item, ledger)
            progress.append(item["name"])
        except CapReached as error:
            with lock:
                pending.insert(0, item)  # not lost: retried once in-flight work has settled
            capped.append(str(error))
        except BaseException as error:  # surfaced after in-flight work drains
            errors.append(f"{item['name']}: {error}")
        finally:
            with lock:
                running.discard(item["name"])

    def seal():
        if archive_dir is not None:
            seal_chunks(plan_dir, archive_dir, max_files, max_bytes, final=False)
            for stage_archive in sorted(p for p in Path(archive_dir).parent.iterdir() if p.is_dir()):
                free_acknowledged(stage_archive, stage_archive)

    def raw_backlog() -> tuple[int, int, int]:
        """Raw match files and bytes not yet freed under ``raw_root`` (every stage's native output)
        and the matches of in-flight requests."""
        with lock:
            inflight = sum(len(by_name[name]["items"]) for name in running)
        files = size = 0
        for folder, _, names in os.walk(raw_root):
            for name in names:
                if name.startswith("match-") and name.endswith(".json"):
                    files += 1
                    try:
                        size += os.path.getsize(os.path.join(folder, name))
                    except OSError:
                        pass
        return files, size, inflight

    def over_two_chunks(next_matches: int) -> bool:
        files, size, inflight = raw_backlog()
        per_match = size / files if files else 64 * 1024  # projected bytes for not-yet-written output
        projected_files = files + inflight + next_matches
        return projected_files > 2 * max_files or size + (inflight + next_matches) * per_match > 2 * max_bytes

    reason, last_check, last_seal, threads = None, 0.0, time.monotonic(), []
    while pending or running:
        if not reason and time.monotonic() - last_check >= check_seconds:
            reason = yield_reason(slots_tool, stop_file, footprint_roots, footprint_limit,
                                  interactive_threshold=interactive_threshold)
            last_check = time.monotonic()
        if capped and not reason:
            with lock:
                busy = bool(running)
            if not busy:
                # Everything in flight has settled. Retry admission once; a second refusal with nothing
                # running and no progress since the last refusal means the cap itself is reached.
                if progress:
                    cap_refusals = 0
                    progress.clear()
                cap_refusals += 1
                capped.clear()
                if cap_refusals >= 2:
                    reason = "CPU cap reached"
            else:
                time.sleep(0.5)
                continue
        if time.monotonic() - last_seal >= seal_seconds:
            seal()
            last_seal = time.monotonic()
        with lock:
            free = workers - len(running)
        if not reason and not errors and pending and free > 0 and raw_root is not None \
                and over_two_chunks(len(pending[0]["items"])):
            # Backpressure: at most two chunks of raw output. Wait briefly for acknowledgements, then
            # yield the claim rather than hold idle cores.
            gated_since = gated_since or time.monotonic()
            if time.monotonic() - gated_since >= ack_wait_seconds:
                reason = "awaiting archive acknowledgement"
            else:
                time.sleep(5)
            continue
        gated_since = None
        if not reason and not errors and pending and free > 0:
            item = pending.pop(0)
            with lock:
                running.add(item["name"])
            thread = threading.Thread(target=one, args=(item,), daemon=False)
            thread.start()
            threads.append(thread)
            continue
        if (reason or errors) and not running:
            break
        time.sleep(0.5)
    for thread in threads:
        thread.join()
    seal()
    if errors:
        raise RuntimeError("; ".join(errors[:5]))
    remaining = [item for item in plan["requests"]
                 if not Path(item["request"]["path"]).with_suffix(".state.json").exists()]
    if not remaining:
        return "complete"
    return f"yielded: {reason or 'CPU cap reached'}"


# ---------------------------------------------------------------- qualification


def qualify(plan_dir: Path, root: Path, executable: str, workers: list[int], ledger: Ledger,
            sample_per_request: int | None = None) -> dict:
    """Run the plan's leading matches of every request serially, then split over k workers; require
    identical per-match hashes and record throughput and process CPU per match."""
    plan = read_json(Path(plan_dir) / "plan.json")
    root = Path(root)
    require(not root.exists(), "qualification root must be fresh")
    require(workers[0] == 1 and workers == sorted(set(workers)), "workers must start at 1, ascending")
    groups = []
    for item in plan["requests"]:
        request = read_json(checked(item["request"]))
        groups.append(dict(request, matches=request["matches"][:sample_per_request] if sample_per_request
                           else request["matches"]))
    trials, golden, serial_by_pilot = [], None, {}
    for k in workers:
        paths, keyed = [], []
        for g, request in enumerate(groups):
            if k == 1:
                parts = [request["matches"]]
            else:
                parts = [request["matches"][i::k] for i in range(k)]
            for i, part in enumerate(parts):
                if not part:
                    continue
                path = root / f"w{k}" / f"request-{g:03d}-{i:02d}.json"
                write_json(path, dict(request, matches=part,
                                      output_directory=str(root / f"w{k}" / f"native-{g:03d}-{i:02d}").replace("\\", "/")))
                paths.append(path)
                keyed.extend((g, json.dumps(m["config"], sort_keys=True), m["registered"][0]["label"],
                              m["registered"][1]["label"]) for m in part)
        started = time.monotonic()
        results = [None] * len(paths)

        def run_index(index):
            matches = len(read_json(paths[index])["matches"])
            reservation = ledger.begin(f"qual-w{k}-{paths[index].stem}", 0, matches, kind="qualification",
                                       per_match=1.0)
            results[index] = run_native(executable, paths[index])
            ledger.settle(reservation, results[index])

        threads, next_index = [], 0
        while next_index < len(paths) or any(t.is_alive() for t in threads):
            threads = [t for t in threads if t.is_alive()]
            while len(threads) < k and next_index < len(paths):
                thread = threading.Thread(target=run_index, args=(next_index,))
                thread.start()
                threads.append(thread)
                next_index += 1
            time.sleep(0.05)
        seconds = time.monotonic() - started
        hashes = {}
        for path, result in zip(paths, results):
            output = Path(read_json(path)["output_directory"])
            require(result["exit"] == 0 and (output / "completion.json").exists(), f"qualification request failed: {path}")
            completion = read_json(output / "completion.json")
            files = match_files(output)
            require([sha256_file(p) for p in files] == completion["match_sha256"], "completion hash mismatch")
            group = int(path.stem.split("-")[1])  # request-<group>-<part>: one pilot per group
            for p in files:
                match = read_json(p)
                key = (group, json.dumps(match["match"]["config"], sort_keys=True),
                       match["match"]["registered"][0]["label"], match["match"]["registered"][1]["label"])
                require(key not in hashes, "duplicate match inside one request group")
                hashes[key] = sha256_file(p)
        require(sorted(hashes) == sorted(keyed), "qualification output omits scheduled matches")
        golden = golden or hashes
        if k == 1:
            for g, (path, result) in enumerate(zip(paths, results)):
                pilot = plan["requests"][g]["pilot"]
                totals = serial_by_pilot.setdefault(pilot, [0.0, 0])
                totals[0] += result["cpu_seconds"] or 0.0
                totals[1] += len(read_json(path)["matches"])
        cpu = [r["cpu_seconds"] for r in results if r["cpu_seconds"] is not None]
        trials.append({"workers": k, "matches": len(hashes), "seconds": seconds,
                       "matches_per_second": len(hashes) / seconds, "parity": hashes == golden,
                       "cpu_seconds": sum(cpu) if len(cpu) == len(results) else None})
    require(all(trial["parity"] for trial in trials), "serial/parallel parity failed")
    alias_checks = label_invariance(root / "w1", plan["decks"])
    best = max(trials, key=lambda trial: trial["matches_per_second"])
    serial = trials[0]
    receipt = {"schema": RECEIPT_SCHEMA, "evaluator_sha256": sha256_file(Path(executable)),
               "plan": pin(Path(plan_dir) / "plan.json"), "sources": plan["sources"], "trials": trials,
               "selected_workers": best["workers"], "label_invariance_cases": alias_checks,
               "cpu_seconds_per_match": max(t["cpu_seconds"] / t["matches"] for t in trials if t["cpu_seconds"])
               if all(t["cpu_seconds"] for t in trials) else None,
               "serial_cpu_seconds_per_match": serial["cpu_seconds"] / serial["matches"] if serial["cpu_seconds"] else None,
               "serial_cpu_seconds_per_match_by_pilot": {p: t[0] / t[1] for p, t in serial_by_pilot.items() if t[1]}}
    write_json(root / "receipt.json", receipt)
    return receipt


def label_invariance(native_root: Path, decks: list[dict]) -> int:
    """A deck with role ``alias`` repeats the incumbent's lists under another label. Its games must be
    identical to the incumbent's on the same cases once the label is substituted back: the label is a
    name, not a model input."""
    aliases = [deck for deck in decks if deck["role"] == "alias"]
    if not aliases:
        return 0
    incumbent = decks[0]
    games: dict[tuple, dict] = {}
    for path in Path(native_root).rglob("match-*.json"):
        match = read_json(path)
        labels = match["match"]["config"]["deck_ids"]
        config = dict(match["match"]["config"], deck_ids=None)
        for alias in aliases:
            labels = [incumbent["label"] if label == alias["label"] else label for label in labels]
        group = path.parent.name.split("-")[1]  # native-<group>-<part>: one pilot per group
        key = group, json.dumps(config, sort_keys=True), tuple(labels)
        text = json.dumps(match["games"], sort_keys=True)
        for alias in aliases:
            text = text.replace(json.dumps(alias["label"]), json.dumps(incumbent["label"]))
        games.setdefault(key, {})[match["match"]["registered"][0]["label"] + "|" +
                                  match["match"]["registered"][1]["label"]] = text
    compared = 0
    for variants in games.values():
        texts = set(variants.values())
        if len(variants) > 1:
            require(len(texts) == 1, "games differ between the incumbent and its alias label")
            compared += 1
    require(compared > 0, "alias deck present but no comparable cases")
    return compared


# ---------------------------------------------------------------- archiving


def completed_outputs(plan_dir: Path) -> list[tuple[str, list[Path]]]:
    plan = read_json(Path(plan_dir) / "plan.json")
    done = []
    for item in plan["requests"]:
        state_path = Path(item["request"]["path"]).with_suffix(".state.json")
        if state_path.exists():
            state = read_json(state_path)
            done.append((item["name"], [Path(s["output_directory"]) for s in state["segments"]]))
    return done


def seal_chunks(plan_dir: Path, archive_dir: Path, max_files: int, max_bytes: int, final: bool) -> list[dict]:
    """Group completed requests (plan order) not yet sealed into chunks of at most max_files match files
    and max_bytes raw; seal a chunk once full, or the last one when ``final``. A sealed chunk is a zip
    plus a manifest whose member hashes were re-read from the zip and equal the completion records."""
    archive_dir = Path(archive_dir)
    archive_dir.mkdir(parents=True, exist_ok=True)
    sealed = set()
    for manifest in archive_dir.glob("chunk-*.json"):
        sealed.update(read_json(manifest)["requests"])
    queue = [(name, outs) for name, outs in completed_outputs(plan_dir) if name not in sealed
             and all(o.exists() for o in outs)]
    made, chunk, files, size = [], [], 0, 0

    def seal(entries):
        number = len(list(archive_dir.glob("chunk-*.json")))
        stem = archive_dir / f"chunk-{number:04d}"
        stale = Path(f"{stem}.zip.partial")
        if stale.exists():  # an interrupted seal never published; its raw output is still intact
            stale.unlink()
        members = []
        with zipfile.ZipFile(f"{stem}.zip.partial", "x", zipfile.ZIP_DEFLATED, compresslevel=6) as zf:
            for name, outs in entries:
                for out in outs:
                    for path in sorted(out.iterdir()):
                        if path.is_file():
                            arc = f"{name}/{out.name}/{path.name}"
                            zf.write(path, arc)
                            members.append({"arc": arc, "sha256": sha256_file(path)})
        with zipfile.ZipFile(f"{stem}.zip.partial") as zf:
            for member in members:
                require(sha256_bytes(zf.read(member["arc"])) == member["sha256"], "zip member differs")
        os.replace(f"{stem}.zip.partial", f"{stem}.zip")
        manifest = {"requests": [name for name, _ in entries], "members": members,
                    "outputs": {name: [str(o).replace("\\", "/") for o in outs] for name, outs in entries},
                    "zip": pin(Path(f"{stem}.zip"))}
        write_json(Path(f"{stem}.json"), manifest)
        made.append(manifest)

    for name, outs in queue:
        n = sum(len(match_files(o)) for o in outs)
        b = tree_bytes(outs)
        if chunk and (files + n > max_files or size + b > max_bytes):
            seal(chunk)
            chunk, files, size = [], 0, 0
        chunk.append((name, outs))
        files, size = files + n, size + b
    if chunk and (final or files >= max_files or size >= max_bytes):
        seal(chunk)
    return made


def free_acknowledged(archive_dir: Path, local_copy_dir: Path) -> list[str]:
    """Remove raw outputs of chunks whose remote copy is acknowledged and whose local second copy
    verifies; never touch anything else."""
    freed = []
    for manifest_path in sorted(Path(archive_dir).glob("chunk-*.json")):
        manifest = read_json(manifest_path)
        ack = manifest_path.with_suffix(".ack")
        if not ack.exists() or read_json(ack).get("zip_sha256") != manifest["zip"]["sha256"]:
            continue
        local = Path(local_copy_dir) / Path(manifest["zip"]["path"]).name
        if not local.exists():
            local.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(manifest["zip"]["path"], f"{local}.partial")
            os.replace(f"{local}.partial", local)
        require(sha256_file(local) == manifest["zip"]["sha256"], f"local copy differs: {local}")
        for outs in manifest["outputs"].values():
            for out in outs:
                if Path(out).exists():
                    shutil.rmtree(out)
                    freed.append(out)
    return freed


# ---------------------------------------------------------------- rows


def member_reader(plan_dir: Path, archive_dirs: list[Path]):
    """Read a match file from raw output, else from a sealed chunk."""
    index = {}
    for archive_dir in archive_dirs:
        for manifest_path in Path(archive_dir).glob("chunk-*.json"):
            manifest = read_json(manifest_path)
            zip_path = manifest_path.with_suffix(".zip")
            for name, outs in manifest["outputs"].items():
                for out in outs:
                    index[out] = (zip_path, f"{name}/{Path(out).name}")

    def read(output: str, filename: str) -> bytes:
        path = Path(output) / filename
        if path.exists():
            return path.read_bytes()
        zip_path, prefix = index[str(output).replace("\\", "/")]
        with zipfile.ZipFile(zip_path) as zf:
            return zf.read(f"{prefix}/{filename}")

    return read


def rows(plan_dir: Path, archive_dirs: list[Path]) -> list[dict]:
    plan = read_json(Path(plan_dir) / "plan.json")
    read = member_reader(plan_dir, archive_dirs)
    out = []
    for item in plan["requests"]:
        state = read_json(Path(item["request"]["path"]).with_suffix(".state.json"))
        request = read_json(checked(item["request"]))
        matches = request["matches"]
        located = {}
        for segment in state["segments"]:
            for i in range(segment["count"]):
                located[segment["start"] + i] = (segment["output_directory"], f"match-{i:06}.json")
        failures = {failure["index"]: failure for failure in state["failures"]}
        require(sorted(list(located) + list(failures)) == list(range(len(matches))),
                f"{item['name']}: matches and failures do not cover every case")
        for index, entry in enumerate(item["items"]):
            deck = plan["decks"][entry["deck"]]
            opponent, block, seat, start = entry["case"]
            row = {"stage": plan["stage"], "pilot": item["pilot"], "deck": entry["deck"], "deck_label": deck["label"],
                   "deck_role": deck["role"], "opponent": opponent, "block": block, "seat": seat,
                   "starting_player": start, "seed": entry["seed"], "request": item["name"]}
            if index in failures:
                out.append({**row, "complete": False, "win": None, "draw": None,
                            "technical": failures[index]["error"][-160:], "match_sha256": None})
                continue
            raw = read(*located[index])
            match = json.loads(raw)
            require(match["match"] == matches[index] and match.get("game_one_only") is True
                    and len(match["games"]) == 1, f"{item['name']}: match {index} differs from its schedule")
            winner = match["games"][0].get("winner")
            out.append({**row, "complete": True, "win": int(winner == seat), "draw": winner is None,
                        "technical": None, "match_sha256": sha256_bytes(raw)})
    return out


# ---------------------------------------------------------------- pull (second machine)


def pull(remote: str, remote_dir: str, local_dir: Path, ssh: list[str], scp: list[str]) -> list[str]:
    """Copy sealed chunks from the run host, verify their zip hash and acknowledge each one there."""
    local_dir = Path(local_dir)
    local_dir.mkdir(parents=True, exist_ok=True)
    listing = subprocess.run(ssh + [remote, f'dir /b "{remote_dir}\\chunk-*.json"'], capture_output=True,
                             text=True).stdout.split()
    acked = []
    for name in sorted(n for n in listing if n.endswith(".json")):
        stem = name[:-5]
        manifest_local = local_dir / name
        if not manifest_local.exists():
            subprocess.run(scp + [f"{remote}:/{remote_dir}/{name}".replace("\\", "/"), str(manifest_local)], check=True)
        manifest = read_json(manifest_local)
        zip_local = local_dir / f"{stem}.zip"
        if not zip_local.exists() or sha256_file(zip_local) != manifest["zip"]["sha256"]:
            subprocess.run(scp + [f"{remote}:/{remote_dir}/{stem}.zip".replace("\\", "/"), f"{zip_local}.partial"],
                           check=True)
            require(sha256_file(Path(f"{zip_local}.partial")) == manifest["zip"]["sha256"], f"copy of {stem} differs")
            os.replace(f"{zip_local}.partial", zip_local)
        ack_local = local_dir / f"{stem}.ack"
        ack_local.write_text(json.dumps({"zip_sha256": manifest["zip"]["sha256"],
                                         "copy": str(zip_local).replace("\\", "/")}), encoding="utf-8")
        subprocess.run(scp + [str(ack_local), f"{remote}:/{remote_dir}/{stem}.ack".replace("\\", "/")], check=True)
        acked.append(stem)
    return acked


# ---------------------------------------------------------------- CLI


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("op", choices=("eligible", "cases", "plan", "qualify", "run", "archive", "pull", "rows"))
    parser.add_argument("rest", nargs=argparse.REMAINDER)
    args = parser.parse_args(argv)
    sub = argparse.ArgumentParser()
    add = sub.add_argument
    for name in ("--deck", "--stage", "--family", "--pilots", "--mode", "--executable", "--slots-tool",
                 "--remote", "--remote-dir", "--pilot-checkpoints"):
        add(name)
    for name in ("--cards", "--pool", "--audit", "--out", "--decks", "--field", "--cases", "--sources", "--plan",
                 "--root", "--ledger", "--stop-file", "--archive-dir", "--local-copy-dir", "--receipt",
                 "--raw-root"):
        add(name, type=Path)
    add("--master-seed", type=int); add("--blocks", type=int); add("--request-size", type=int, default=240)
    add("--native-root"); add("--workers", default="1,2,4,8"); add("--sample-per-request", type=int)
    add("--cap-cpu-seconds", type=float); add("--footprint-limit", type=int, default=12 * 2**30)
    add("--footprint-roots", default=""); add("--max-files", type=int, default=20000)
    add("--max-bytes", type=int, default=int(1.5 * 2**30)); add("--final", action="store_true")
    add("--archive-dirs", default=""); add("--interactive-threshold", type=float, default=0.0)
    a = sub.parse_args(args.rest)
    if args.op == "eligible":
        pilots = dict(part.split("=") for part in a.pilot_checkpoints.split(","))
        result = eligible_in_cards(read_json(a.cards)["cards"], read_json(a.pool), a.deck, read_json(a.audit), pilots)
        result["inputs"] = {"cards": pin(a.cards), "pool": pin(a.pool), "audit": pin(a.audit), "pilots": pilots}
        write_json(a.out, result)
        print(json.dumps({"in_cards": len(result["in_cards"]), "exposure_limited": len(result["exposure_limited"])}))
    elif args.op == "cases":
        cases = stage_cases(a.master_seed, a.stage, a.family, a.blocks, a.pilots.split(","), a.mode)
        write_json(a.out, {"stage": a.stage, "master_seed": a.master_seed, "family": a.family, "blocks": a.blocks,
                           "mode": a.mode, "cases": cases})
        print(json.dumps({"cases": len(cases)}))
    elif args.op == "plan":
        decks = read_json(a.decks)["decks"]
        field = read_json(a.field)
        cases = read_json(a.cases)["cases"]
        plan = build_plan(a.stage, decks, field, cases, read_json(a.sources), a.out, a.native_root, a.request_size)
        print(json.dumps({"requests": len(plan["requests"]), "matches": plan["matches"]}))
    elif args.op == "qualify":
        plan = read_json(a.plan / "plan.json")
        check_pins(plan["sources"])
        ledger = Ledger(a.ledger, a.cap_cpu_seconds, 0.0)
        receipt = qualify(a.plan, a.root, a.executable, [int(x) for x in a.workers.split(",")], ledger,
                          a.sample_per_request)
        print(json.dumps({"selected_workers": receipt["selected_workers"],
                          "cpu_seconds_per_match": receipt["cpu_seconds_per_match"],
                          "trials": [(t["workers"], round(t["matches_per_second"], 2)) for t in receipt["trials"]]}))
    elif args.op == "run":
        plan = read_json(a.plan / "plan.json")
        receipt = read_json(a.receipt)
        require(receipt["schema"] == RECEIPT_SCHEMA and receipt["evaluator_sha256"] == sha256_file(Path(a.executable))
                and receipt["sources"] == plan["sources"] and all(t["parity"] for t in receipt["trials"]),
                "qualification receipt does not cover this evaluator and these sources")
        check_pins(plan["sources"])
        ledger = Ledger(a.ledger, a.cap_cpu_seconds, receipt["cpu_seconds_per_match"])
        roots = [Path(p) for p in a.footprint_roots.split(";") if p]
        outcome = dispatch(a.plan, a.executable, receipt["selected_workers"], ledger, a.slots_tool, a.stop_file,
                           roots, a.footprint_limit, interactive_threshold=a.interactive_threshold,
                           archive_dir=a.archive_dir, max_files=a.max_files, max_bytes=a.max_bytes,
                           raw_root=a.raw_root)
        print(json.dumps({"outcome": outcome, "ledger_cpu_seconds": ledger.spent()}))
        if outcome == "complete":
            return 0
        return CAP_EXIT if outcome == "yielded: CPU cap reached" else YIELD_EXIT
    elif args.op == "archive":
        made = seal_chunks(a.plan, a.archive_dir, a.max_files, a.max_bytes, a.final)
        freed = free_acknowledged(a.archive_dir, a.local_copy_dir) if a.local_copy_dir else []
        print(json.dumps({"sealed": len(made), "freed_outputs": len(freed)}))
    elif args.op == "pull":
        ssh = ["ssh", "-o", "BatchMode=yes", "-o", "ConnectTimeout=20"]
        scp = ["scp", "-q", "-o", "BatchMode=yes", "-o", "ConnectTimeout=20"]
        print(json.dumps({"acknowledged": pull(a.remote, a.remote_dir, a.archive_dir, ssh, scp)}))
    else:
        result = rows(a.plan, [Path(p) for p in a.archive_dirs.split(";") if p])
        plan = read_json(a.plan / "plan.json")
        require(len(result) == plan["matches"], "rows do not cover every scheduled match")
        require(not a.out.exists(), f"refusing to overwrite {a.out}")
        tmp = a.out.with_name(a.out.name + ".partial")
        with tmp.open("w", encoding="utf-8", newline="\n") as stream:
            stream.write("".join(json.dumps(row, sort_keys=True) + "\n" for row in result))
        os.replace(tmp, a.out)
        print(json.dumps({"rows": len(result), "complete": sum(row["complete"] for row in result)}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
