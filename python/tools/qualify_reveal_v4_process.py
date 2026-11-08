"""Bounded real bridge/adapter/pool/worker qualification, never a rated game.

Run inside a bounded host_slots job. The companion modules are loaded from the
reviewed checkout; this tool does not modify that checkout or replace peers.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

COMPANION = "622e3b705d8e0c91284e2ebbf02ed2f58a4f9030"


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
    parser.add_argument("--require-buffered", action="store_true")
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
    from kernel_engine_reveal_search_v4 import INFOSET, KernelRevealSearchEngineV4, RevealSearchNativePeerV4
    from kernel_search_reveal_v4 import RevealPoolV4, RevealSearchBotV4
    from spellbench import wire
    from spellbench.bot import Decision, GameStart
    from spellbench.digests import card_name_domain, deck_id
    from spellbench.host.validator import LiveValidator
    from spellbench.messages import Decision as EnvironmentDecision, EnvHelloOk, Rules
    from spellbench.reveal_meaning_v3 import SCHEMA

    class RecordedPool:
        def __init__(self, native):
            self.native = native
            self.searches = []
            self.points = 0

        def request_line(self, line):
            request = json.loads(line)
            assert request["payload"]["schema"] == INFOSET
            response = self.native.request_line(line)
            assert response.get("ok") is True, response
            if request.get("op") == "point":
                self.points += 1
            else:
                result = response["search"]
                assert result["payload_sha256"] == request["payload"]["sha256"]
                assert result["root_id"] == request["root_id"]
                assert result["incomplete"] is False, result
                assert 0 < result["units"] <= request["budget"], result
                assert result["charge"] == result["units"] + request["a_units"]
                self.searches.append({key: result[key] for key in
                                      ("root_id", "payload_sha256", "units", "pick", "tree_digests")})
            return response

    catalog = json.loads(args.catalog.read_bytes())
    entries = [next(row for row in catalog["catalog"] if row["catalog_id"] == name)
               for name in ("CawGates", "Faeries")]
    report = {"schema": "reveal-v4-process-qualification/v1", "qualified": False,
              "companion": head, "host_slot_claim": os.environ["HOST_SLOTS_CLAIM"],
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
                engine = KernelRevealSearchEngineV4(peer, catalog)
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
                case = {"first": first, "steps": 0, "buffered_commits": 0, "payloads": 0,
                        "refusals": 0, "searches": pool.searches}
                report["cases"].append(case)
                response = wire.strict_json_loads(engine.handle(wire.canonical_json_dumps(request)))
                for step in range(args.max_steps):
                    assert response["response_type"] == "decision", response
                    sd = validator.check(EnvironmentDecision.from_json(response))
                    ext = sd["extensions"].get(INFOSET, {})
                    case["payloads"] += "payload_json" in ext
                    case["refusals"] += "refusal" in ext
                    candidate = bots[sd["acting_seat"]].choose(Decision.from_request(
                        {"game_id": game, "decision": sd}))
                    validator.answered(sd, candidate)
                    # A completed multi-choice buffer invokes the reviewed
                    # full-point/key checks on every actual native commit.
                    buffered_end = (engine._group_keys is not None and
                                    sd["group"]["substep_index"] + 1 == sd["group"]["substep_count"])
                    request = {"protocol": "spellbench/v2", "request_type": "step",
                               "request_id": f"step-{step}", "game_id": game,
                               "expected_step": response["step"], "selection": {
                                   "candidate_id": candidate, "semantic_echo": sd["candidates"][candidate]["semantic"]}}
                    text = engine.handle(wire.canonical_json_dumps(request))
                    transcript.update(text)
                    response = wire.strict_json_loads(text)
                    assert response["response_type"] == "decision", response
                    case["steps"] += 1
                    case["buffered_commits"] += buffered_end
                    if pool.searches and (case["buffered_commits"] or not args.require_buffered):
                        validator.check(EnvironmentDecision.from_json(response))
                        break
                case["point_requests"] = pool.points
                case["transcript_sha256"] = transcript.hexdigest()
                assert pool.searches, "no real V4 worker search completed"
                if args.require_buffered:
                    assert case["buffered_commits"] > 0 and pool.points > 0, "no actual searched buffered commit"
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
