"""Native step capture for a recorded Spellbench pauper-v2 game. Measurement only.

usage:
  sbcapture.py --adapter DIR --bridge EXE --catalog CATALOG.json --source-revision REV --engine ENGINE.jsonl[.gz]
               --spec SPEC.json --out JOB.json [--report REPORT.json]

The Spellbench adapter (kernel_engine_v2.py, imported read-only from DIR) is
not one wire decision per native step: it renames kinds, re-sorts rows,
buffers blocks, arrangements and trigger orders on the host side and may
reverse the seat order. So this tool drives the same adapter code and the
same bridge with the record's own host requests and logs every native step
the adapter sends the bridge. Every adapter answer must equal the recorded
one, or the tool stops.

For each target wire decision it then replays the prefix once per wire
candidate, choosing that candidate at the target and logging the native
steps until the adapter commits that decision's group to the bridge. A block
declaration is scanned natively one attacker and blocker pair at a time, so
one wire choice can be decided across several native steps. Every native
decision where two candidates' native rows part is a decision point; the
search runs at each, with each native row labelled by the wire candidates
(`sbroot.describe`) behind it. A point off the recorded line is one the game
reaches only after a different native row earlier in the same group.

The job (`--out`) is the `s4a-sbroot` job over all native steps: `i` is the
native step index, `seat` the native acting player, `kinds` the native row
kinds in flat row order and `row` the flat row the native step took. Each
decision point gets its own job beside it (`<out stem>-<key>.json`), ending
at that point.
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from sbroot import describe, fnv1a64, names, read_lines  # noqa: E402

FLAT = "x_kernel_flat_v4"
# The bridge's semantic kind names (agent_bridge_v1.rs at 92ec2393, the
# ActionSemanticV1 serializer) that differ from the flat action kind names of
# the same variants (FlatScorerActionKindV1). Every other name is the same.
BRIDGE_KIND = {"choose_option": "choose_effect_option", "choose_color": "choose_effect_color",
               "choose_number": "choose_effect_number", "choose_boolean": "choose_effect_boolean"}


def load_adapter(adapter: Path):
    sys.path.insert(0, str(adapter))
    import kernel_engine_v2  # noqa: PLC0415
    return kernel_engine_v2


class Capture:
    """The adapter over a logging peer. `native` holds every native step:
    the decision it answered and the candidate it chose."""

    def __init__(self, ke, bridge, catalog, source_revision):
        self.ke = ke
        self.peer = ke.NativePeer(bridge)
        self.native = []
        self.reset_request = None
        request = self.peer.request

        def logged(value):
            if value.get("request_type") == "reset":
                self.reset_request = value
            if value.get("request_type") == "step":
                cur = self.engine.current
                self.native.append({"decision": cur, "candidate": value["selection"]["candidate_id"]})
            return request(value)

        self.peer.request = logged
        self.engine = ke.KernelEngine(self.peer, catalog, source_revision=source_revision, known_cards=True)

    def send(self, message) -> dict:
        payload = json.dumps(message, separators=(",", ":")).encode()
        return json.loads(self.engine.handle(payload))

    def close(self):
        self.peer.close()


def native_row(decision: dict, candidate: int) -> dict:
    rows = decision["extensions"][FLAT]["row_candidate_ids"]
    raw = json.loads(decision["extensions"]["x_kernel_v5"]["observation_json"])
    return {"seat": int(raw["acting_player"][1:]), "step": decision["step"],
            "kinds": [BRIDGE_KIND.get(k, k) for k in (decision["candidates"][c]["semantic"]["kind"] for c in rows)],
            "row": rows.index(candidate),
            "names": [names(decision["candidates"][c]["semantic"]) for c in rows]}


def host_messages(engine_path: Path):
    lines = list(read_lines(engine_path))
    host = [x["message"] for x in lines if x["dir"] == "host_to_engine"]
    answers = [x["message"] for x in lines if x["dir"] == "engine_to_host"]
    return host, answers


def run(ke, a, catalog, host, answers, override=None):
    """Replay the host requests. `override` = (wire decision index, candidate)
    replaces that step's selection; the replay then goes on with the
    record's own selections only while the adapter still buffers that wire
    decision's group, and stops once the group has committed to the bridge
    (or a recorded selection no longer fits).
    Returns (capture, wire decision -> native steps taken before it was posed,
    answer mismatches)."""
    cap = Capture(ke, str(a.bridge), catalog, a.source_revision)
    posed, mismatches, decision_index, overridden = {}, [], 0, False
    try:
        for k, msg in enumerate(host):
            if msg.get("request_type") == "step":
                if override and decision_index == override[0]:
                    sd = last["seat_decision"]
                    msg = {**msg, "selection": {"candidate_id": override[1],
                           "semantic_echo": sd["candidates"][override[1]]["semantic"]}}
                    overridden = True
                decision_index += 1
            got = cap.send(msg)
            if overridden:
                if got.get("response_type") != "decision" or not cap.engine.buffer:
                    break
                last = got
                continue
            if k < len(answers) and got != answers[k]:
                mismatches.append({"host_index": k, "request_type": msg.get("request_type")})
                break
            last = got
            if got.get("response_type") == "decision":
                posed[decision_index] = len(cap.native)
    finally:
        cap.close()
    return cap, posed, mismatches


def decision_points(seqs: dict, start: int, played: int):
    """Every native decision where two candidates' native row sequences part.
    `seqs` maps wire candidate to its native rows from `start`. Yields
    (path rows, native index, {row: [candidates]}, on the recorded line)."""
    points = {}
    cands = list(seqs)
    for x in cands:
        for y in cands:
            if x >= y:
                continue
            a, b = seqs[x], seqs[y]
            d = 0
            while d < len(a) and d < len(b) and a[d]["row"] == b[d]["row"]:
                d += 1
            if d < len(a) and d < len(b):
                points[tuple(r["row"] for r in a[:d])] = d
    for path, d in sorted(points.items(), key=lambda kv: (kv[1], kv[0])):
        rows, at = {}, None
        for c, seq in seqs.items():
            if len(seq) > d and tuple(r["row"] for r in seq[:d]) == path:
                rows.setdefault(seq[d]["row"], []).append(c)
                at = seq
        recorded = tuple(r["row"] for r in seqs[played][:d]) == path
        yield [dict(r) for r in at[:d + 1]], start + d, rows, recorded


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--adapter", type=Path, required=True)
    ap.add_argument("--bridge", type=Path, required=True)
    ap.add_argument("--catalog", type=Path, required=True)
    ap.add_argument("--engine", type=Path, required=True)
    ap.add_argument("--spec", type=Path, required=True)
    ap.add_argument("--source-revision", required=True)
    ap.add_argument("--out", type=Path, required=True)
    ap.add_argument("--report", type=Path)
    a = ap.parse_args(argv)
    ke = load_adapter(a.adapter)
    catalog = json.loads(a.catalog.read_bytes())
    spec = json.loads(a.spec.read_text(encoding="utf-8"))
    host, answers = host_messages(a.engine)
    wire_decisions = [m["seat_decision"] for m in answers if m.get("response_type") == "decision"]
    played = [m["selection"]["candidate_id"] for m in host if m.get("request_type") == "step"]

    cap, posed, mismatches = run(ke, a, catalog, host, answers)
    if mismatches:
        print(json.dumps({"exact": False, "mismatches": mismatches}))
        return 1
    rec = [native_row(n["decision"], n["candidate"]) for n in cap.native]
    reset = cap.reset_request
    job = {"game_id": reset["game_id"], "episode_id": fnv1a64(reset["game_id"]), "game_seed": reset["game_seed"],
           "deck_ids": [s["deck"]["catalog_id"] for s in reset["seats"]],
           "max_decisions": reset["max_decisions"], "max_steps": reset["max_steps"],
           "steps": [{"i": n, "seat": r["seat"], "kinds": r["kinds"], "row": r["row"]} for n, r in enumerate(rec)],
           "targets": []}
    report = {"game_id": reset["game_id"], "exact_wire_answers": True, "native_steps": len(rec),
              "wire_decisions": len(wire_decisions), "native_seat_order": [s["seat"] for s in reset["seats"]],
              "wire_starting_seat": next(m for m in host if m.get("request_type") == "reset")["rules"]["starting_seat"],
              "posed": posed, "native": rec, "targets": {}}
    jobs = {}
    for t in spec["targets"]:
        i = int(t)
        sd = wire_decisions[i]
        start = posed[i]
        if len(sd["candidates"]) == 1:
            report["targets"][t] = {"wire": i, "status": "single_candidate"}
            continue
        seqs = {}
        for c in range(len(sd["candidates"])):
            alt, _, _ = run(ke, a, catalog, host, answers, override=(i, c))
            seqs[c] = [native_row(n["decision"], n["candidate"]) for n in alt.native][start:]
        if seqs[played[i]] != rec[start:start + len(seqs[played[i]])]:
            report["targets"][t] = {"wire": i, "status": "recorded_choice_did_not_reproduce"}
            continue
        name = lambda c: describe(sd["candidates"][c]["semantic"])
        points = []
        for path, n, rows, recorded in decision_points(seqs, start, played[i]):
            step = path[-1]
            key = f"{t}-n{n}" + ("" if recorded else "-alt" + "".join(str(r["row"]) for r in path[:-1]))
            steps = job["steps"][:start] + [{"i": start + k, "seat": r["seat"], "kinds": r["kinds"], "row": r["row"]}
                                            for k, r in enumerate(path)]
            jobs[key] = {**{k: v for k, v in job.items() if k != "steps"}, "steps": steps, "targets": [n]}
            points.append({"key": key, "native": n, "on_recorded_line": recorded,
                           "path": [r["row"] for r in path[:-1]], "seat": step["seat"], "kinds": step["kinds"],
                           "names": step["names"],
                           "row_labels": {str(r): [name(c) for c in cs] for r, cs in rows.items()}})
        report["targets"][t] = {"wire": i, "native_posed_at": start, "played": name(played[i]),
                                "native_sequences": {f"{c}: {name(c)}": [r["row"] for r in seq]
                                                     for c, seq in seqs.items()},
                                "points": points}
    a.out.write_text(json.dumps(job) + "\n", encoding="utf-8")
    for key, j in jobs.items():
        (a.out.parent / f"{a.out.stem}-{key}.json").write_text(json.dumps(j) + "\n", encoding="utf-8")
    if a.report:
        a.report.write_text(json.dumps(report, indent=1) + "\n", encoding="utf-8")
    print(json.dumps(report, indent=1))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
