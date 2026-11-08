"""Bounded real bridge/adapter/pool/worker qualification, never a rated game.

Run inside a bounded host_slots job. The companion modules are loaded from the
reviewed checkout; this tool does not modify that checkout or replace peers.
"""
from __future__ import annotations

import argparse
from copy import deepcopy
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

COMPANION = "622e3b705d8e0c91284e2ebbf02ed2f58a4f9030"
INFOSET = "x_kernel_infoset_reveal_v4"


def search_receipt(request, response, point, context):
    """Validate accounting; an incomplete tree pool is a valid fallback result."""
    assert response.get("ok") is True, response
    result = response["search"]
    assert result["payload_sha256"] == request["payload"]["sha256"]
    assert result["root_id"] == request["root_id"]
    assert type(result["incomplete"]) is bool, result
    assert type(result["units"]) is int and 0 <= result["units"] <= request["budget"], result
    assert result["charge"] == result["units"] + request["a_units"], result
    pick = result["pick"]
    if result["incomplete"]:
        assert pick is None, result
    else:
        assert type(pick) is int and 0 <= pick < len(result["merged"]), result
        assert result["units"] > 0, result
    keys = None if point is None else point.get("keys")
    if keys is not None:
        assert point["viewer_decision"] is True
        assert len(keys) == len(result["merged"])
        if not result["incomplete"]:
            assert point["eligible"] is True
    row = {key: deepcopy(result[key]) for key in (
        "root_id", "payload_sha256", "units", "charge", "pick", "incomplete",
        "menu_sha256", "excluded", "tree_digests")}
    row.update(path=list(request.get("path", [])), budget=request["budget"],
               a_units=request["a_units"], context=deepcopy(context), point_keys=deepcopy(keys),
               selected_key=None if pick is None or keys is None else keys[pick],
               successful=not result["incomplete"], merged=deepcopy(result["merged"]),
               trees=deepcopy(response.get("trees", [])))
    return row


def binds_committed_path(search, commit):
    """A successful pick must be the actual next row on this original group."""
    if not search["successful"] or search["incomplete"]:
        return False
    if search["payload_sha256"] != commit["payload_sha256"]:
        return False
    if any(search["context"].get(key) != commit["context"].get(key)
           for key in ("game", "acting_seat", "group_id")):
        return False
    depth = len(search["path"])
    if depth >= len(commit["path"]) or search["path"] != commit["path"][:depth]:
        return False
    point = commit["points"][depth]
    return (search["pick"] == commit["path"][depth]
            and search["point_keys"] == point["keys"]
            and search["selected_key"] == point["selected_key"]
            and search["selected_key"] == commit["expected_keys"][depth])


def bind_live_point(live_keys, live_pick, imported_point):
    """Translate through public row keys, never equate native and import order."""
    assert imported_point["viewer_decision"] is True
    keys = imported_point["keys"]
    assert len(keys) == len(live_keys) == len(set(keys)) == len(set(live_keys))
    assert set(keys) == set(live_keys), "actual and imported menus differ"
    selected_key = live_keys[live_pick]
    return {"keys": list(keys), "live_keys": list(live_keys),
            "pick": keys.index(selected_key), "live_pick": live_pick,
            "selected_key": selected_key}


class RecordedPool:
    def __init__(self, native):
        self.native = native
        self.searches = []
        self.points = []
        self.context = {}
        self._points = {}

    def request_line(self, line):
        request = json.loads(line)
        assert request["payload"]["schema"] == INFOSET
        response = self.native.request_line(line)
        assert response.get("ok") is True, response
        identity = (request["payload"]["sha256"], tuple(request.get("path", [])))
        if request.get("op") == "point":
            point = deepcopy(response["point"])
            self._points[identity] = (deepcopy(self.context), point)
            self.points.append({"payload_sha256": identity[0], "path": list(identity[1]),
                                "context": deepcopy(self.context), "point": point, "role": "entrant"})
        else:
            saved = self._points.get(identity)
            point = saved[1] if saved is not None and saved[0] == self.context else None
            row = search_receipt(request, response, point, self.context)
            row["search_index"] = len(self.searches)
            self.searches.append(row)
        return response

    def commit_point(self, payload, path):
        # Audit the actual commit's imported prefix using the same native pool.
        # This read-only request does not search or spend transition units.
        request = {"op": "point", "payload": payload, "path": list(path)}
        response = self.native.request_line(json.dumps(request).encode("utf-8"))
        assert response.get("ok") is True, response
        point = deepcopy(response["point"])
        self.points.append({"payload_sha256": payload["sha256"], "path": list(path),
                            "context": deepcopy(self.context), "point": point, "role": "commit_audit"})
        return point


def sha(path):
    with Path(path).open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--companion", type=Path, required=True)
    parser.add_argument("--bridge", type=Path, required=True)
    parser.add_argument("--search", type=Path, required=True)
    parser.add_argument("--catalog", type=Path, required=True)
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--source-sha256", required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--max-steps", type=int, default=128)
    parser.add_argument("--budget", type=int, default=32)
    parser.add_argument("--require-buffered", action="store_true", required=True)
    args = parser.parse_args()
    if not os.environ.get("HOST_SLOTS_CLAIM"):
        parser.error("run inside a bounded host_slots job with --on-overdue kill")
    if not 1 <= args.max_steps <= 512 or not 1 <= args.budget <= 256:
        parser.error("max-steps must be 1..512 and budget 1..256")
    companion = args.companion.resolve()
    head = subprocess.check_output(["git", "-C", str(companion), "rev-parse", "HEAD"], text=True).strip()
    if head != COMPANION:
        parser.error(f"companion must be exact reviewed {COMPANION}, got {head}")
    if subprocess.check_output(["git", "-C", str(companion), "status", "--porcelain", "--untracked-files=no"]):
        parser.error("companion tracked source must be clean")
    if sha(args.source) != args.source_sha256.lower():
        parser.error("source descriptor SHA-256 differs")
    # These are the actual reviewed adapter and entrant classes. Their peer and
    # pool constructors launch the real native bridge and worker processes.
    sys.path[:0] = [str(companion / "integrations/mtg_kernel"), str(companion / "python")]
    from kernel_engine_reveal_search_v4 import KernelRevealSearchEngineV4, RevealSearchNativePeerV4
    from kernel_search_reveal_v4 import RevealPoolV4, RevealSearchBotV4
    from kernel_engine_v2 import FLAT
    from spellbench import wire
    from spellbench.bot import Decision, GameStart
    from spellbench.digests import card_name_domain, deck_id
    from spellbench.host.validator import LiveValidator
    from spellbench.messages import Decision as EnvironmentDecision, EnvHelloOk, Rules
    from spellbench.reveal_meaning_v3 import SCHEMA

    class RecordedEngine(KernelRevealSearchEngineV4):
        """Observe real commits while retaining every reviewed adapter check."""
        def __init__(self, peer, catalog, pool):
            super().__init__(peer, catalog)
            self.audit_pool = pool
            self.commits = []
            self.context = {}
            self._recording_commit = None
            self._recording_payload = None

        def commit_buffer(self):
            if self._group_keys is None:
                return super().commit_buffer()
            wire_choices = tuple(self._group_wire)
            expected = self._group_keys[wire_choices]
            payload = json.loads(self.root["extensions"][INFOSET]["payload_json"])
            record = {"context": deepcopy(self.context), "root_step": self.root["step"],
                      "payload_sha256": payload["sha256"], "wire": list(wire_choices),
                      "expected_keys": list(expected), "path": [], "points": []}
            self._recording_commit = record
            self._recording_payload = payload
            try:
                # This runs the unchanged preview/full-point/selected-key
                # checks, then commits each native action through the peer.
                super().commit_buffer()
                assert [point["selected_key"] for point in record["points"]] == list(expected)
                self.commits.append(record)
            finally:
                self._recording_commit = None
                self._recording_payload = None

        def native_step(self, candidate):
            record = self._recording_commit
            if record is not None:
                view = self.current
                ids = view["extensions"][FLAT]["row_candidate_ids"]
                assert ids.count(candidate) == 1
                index = ids.index(candidate)
                keys = [self._live_key(view, row) for row in ids]
                imported = self.audit_pool.commit_point(self._recording_payload, record["path"])
                point = bind_live_point(keys, index, imported)
                # Native point counters may exceed the neutral wire integer
                # range; hash their JSON without recoding them as wire messages.
                point.update(native_step=view["step"], full_point_sha256=hashlib.sha256(
                    json.dumps(self._point(view), sort_keys=True, separators=(",", ":"),
                               ensure_ascii=False, allow_nan=False).encode("utf-8")).hexdigest())
            super().native_step(candidate)
            if record is not None:
                record["path"].append(point["pick"])
                record["points"].append(point)

    catalog = json.loads(args.catalog.read_bytes())
    entries = [next(row for row in catalog["catalog"] if row["catalog_id"] == name)
               for name in ("CawGates", "Faeries")]
    report = {"schema": "reveal-v4-process-qualification/v2", "qualified": False,
              "companion": head, "host_slot_claim": os.environ["HOST_SLOTS_CLAIM"],
              "bounds": {"max_steps_per_starting_seat": args.max_steps,
                         "native_transition_budget_per_search": args.budget},
              "inputs": {name: {"path": str(getattr(args, name).resolve()), "sha256": sha(getattr(args, name))}
                         for name in ("bridge", "search", "catalog", "source")}, "cases": []}
    args.out.parent.mkdir(parents=True, exist_ok=True)
    try:
        for first in ("p0", "p1"):
            peer = RevealSearchNativePeerV4(str(args.bridge.resolve()))
            native_pool = None
            try:
                native_pool = RevealPoolV4(str(args.search.resolve()), str(args.source.resolve()), 1)
                pool = RecordedPool(native_pool)
                engine = RecordedEngine(peer, catalog, pool)
                rules = {"opponent_decklist": "visible", "mulligan": "none",
                         "starting_player": "host_assigned", "starting_seat": first,
                         "card_name_domain": card_name_domain({card["name"] for entry in entries
                                                               for card in entry["decklist"]}),
                         "extensions": [SCHEMA, INFOSET], "probe": False}
                validator = LiveValidator(EnvHelloOk.from_json(engine.hello), Rules.from_json(rules),
                                          max_steps=1024, max_decisions=1024)
                game = f"reveal-v4-native-{first}"
                bots = {seat: RevealSearchBotV4(pool, budget=args.budget,
                                               allowance=args.max_steps * (args.budget + 1), a_units=1)
                        for seat in ("p0", "p1")}
                for seat, bot in bots.items():
                    bot.on_game_start(GameStart.from_request({"game_id": game, "agent_seed": 543,
                                                              "seat": seat}))
                request = {"protocol": "spellbench/v2", "request_type": "reset", "request_id": "reset",
                           "game_id": game, "format": "pauper-bo1", "game_secret": "ab" * 32,
                           "max_steps": 1024, "max_decisions": 1024, "rules": rules,
                           "seats": [{"seat": seat, "deck": {"catalog_id": entry["catalog_id"],
                                                              "deck_id": deck_id(entry["decklist"])}}
                                     for seat, entry in zip(("p0", "p1"), entries)]}
                transcript = hashlib.sha256()
                case = {"first": first, "steps": 0, "buffered_commits": 0,
                        "searched_buffered_commits": 0, "payloads": 0,
                        "refusals": 0, "searches": pool.searches,
                        "points": pool.points, "commits": engine.commits}
                report["cases"].append(case)
                response = wire.strict_json_loads(engine.handle(wire.canonical_json_dumps(request)))
                for step in range(args.max_steps):
                    assert response["response_type"] == "decision", response
                    sd = validator.check(EnvironmentDecision.from_json(response))
                    ext = sd["extensions"].get(INFOSET, {})
                    case["payloads"] += "payload_json" in ext
                    case["refusals"] += "refusal" in ext
                    context = {"game": game, "acting_seat": sd["acting_seat"],
                               "group_id": sd["group"]["group_id"],
                               "substep_index": sd["group"]["substep_index"], "step": response["step"]}
                    pool.context = engine.context = context
                    candidate = bots[sd["acting_seat"]].choose(Decision.from_request(
                        {"game_id": game, "decision": sd}))
                    case["successful_searches"] = sum(row["successful"] for row in pool.searches)
                    case["incomplete_searches"] = sum(row["incomplete"] for row in pool.searches)
                    validator.answered(sd, candidate)
                    commits_before = len(engine.commits)
                    request = {"protocol": "spellbench/v2", "request_type": "step",
                               "request_id": f"step-{step}", "game_id": game,
                               "expected_step": response["step"], "selection": {
                                   "candidate_id": candidate, "semantic_echo": sd["candidates"][candidate]["semantic"]}}
                    text = engine.handle(wire.canonical_json_dumps(request))
                    transcript.update(text)
                    response = wire.strict_json_loads(text)
                    assert response["response_type"] == "decision", response
                    case["steps"] += 1
                    for commit in engine.commits[commits_before:]:
                        commit["matching_searches"] = [row["search_index"] for row in pool.searches
                                                      if binds_committed_path(row, commit)]
                        case["buffered_commits"] += 1
                        case["searched_buffered_commits"] += bool(commit["matching_searches"])
                    if case["searched_buffered_commits"]:
                        validator.check(EnvironmentDecision.from_json(response))
                        break
                case["point_requests"] = len(pool.points)
                case["transcript_sha256"] = transcript.hexdigest()
                assert case.get("successful_searches", 0), "no successful real V4 worker search completed"
                assert case["searched_buffered_commits"] > 0, "no successful pick on an actual buffered commit path"
            finally:
                peer.close()
                if native_pool is not None:
                    native_pool.close()
        report["qualified"] = True
    except Exception as error:
        report["error"] = f"{type(error).__name__}: {error}"
        raise
    finally:
        args.out.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"qualified": True, "receipt": str(args.out.resolve())}))


if __name__ == "__main__":
    main()
