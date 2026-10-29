"""Bounded schema-8 real-process correctness checker, not a Call admission run.

Two workers replay seed 123 over admitted basic-land decks to natural terminal.
Call printings remain Partial; genuine paid Call source cases are Rust fixtures.
This source checkpoint has not executed the real-engine checker.
"""
from __future__ import annotations

import argparse
from concurrent.futures import ThreadPoolExecutor, as_completed
import json
from pathlib import Path
import subprocess
from threading import Event
from time import monotonic

from limited_call_session_v8 import LimitedCallClientV8, PROTOCOL
from limited_decks_v1 import load_json, parse_dck
from limited_session_v1 import LimitedSessionError, _uint


def _check_game(command: list[str], catalog: int, seed: int, stop: Event) -> dict:
    started = monotonic()
    decks = (parse_dck("40 Forest"), parse_dck("40 Island"))
    server_timings = []
    actors = set()
    try:
        with LimitedCallClientV8(command, expected_card_db_hash=catalog,
                                   timeout_s=30) as client:
            reply = client.reset(decks, env_seed=seed, max_steps=10000)
            if reply["state"]["state_type"] != "decision":
                raise LimitedSessionError("reset did not produce a playable prompt")
            snapshot = client.snapshot()
            restored = client.restore(snapshot["snapshot_id"])
            sent = client.next_request
            try:
                client.step(reply, 0)
            except LimitedSessionError:
                pass
            else:
                raise LimitedSessionError("old prompt survived restore")
            if client.next_request != sent:
                raise LimitedSessionError("stale prompt sent an engine request")
            reply = restored
            for _ in range(10000):
                if stop.is_set():
                    raise LimitedSessionError("paired process check failed")
                state = reply["state"]
                if state["state_type"] == "terminal":
                    if state["classification"] != "natural":
                        raise LimitedSessionError("plumbing game did not end naturally")
                    terminal = state
                    break
                if state["state_type"] != "decision":
                    raise LimitedSessionError(f"unplayable common state: {state['code']}")
                decision = state["decision"]
                actor = decision["actor"]
                actors.add(actor)
                own_hand = decision["state"]["common"]["predecessor"]["own_hand"]
                if any(card["stable"]["owner"] != actor or card["stable"]["zone"] != "Hand" for card in own_hand):
                    raise LimitedSessionError("prompt exposes a foreign private hand")
                meanings = decision["state"]["next_five"]["meanings"]
                if [row["family"] for row in meanings] != ["Snack", "Replication", "Infernal", "Giada", "Goblin"]:
                    raise LimitedSessionError("missing complete graveyard-return ordered meanings")
                graveyard = decision["state"]["graveyard_return"]
                if [row["family"] for row in graveyard["meanings"]] != ["Alesha", "NineLives"]:
                    raise LimitedSessionError("missing complete inherited graveyard-return meanings")
                call = decision["state"]["call"]
                if call["meaning"]["card_id"] != 459 or call["meaning"]["owning_consumer_admitted"]:
                    raise LimitedSessionError("missing closed source-only Call meaning")
                actions = decision["actions"]
                index = next((a["action_index"] for a in actions
                              if a["label"].startswith(("Keep this hand", "Pass"))), 0)
                reply = client.step(reply, index)
            else:
                raise LimitedSessionError("plumbing game exceeded its bound")
            if actors != {"p0", "p1"}:
                raise LimitedSessionError("natural replay did not exercise both actor routes")
            transcript = client.transcript.hexdigest()
        for line in client.stderr:
            prefix = b"LIMITED_CALL_TIMING_V8 "
            if line.startswith(prefix):
                server_timings.append(load_json(line[len(prefix):]))
        return {"transcript": transcript, "terminal": terminal,
                "timings": server_timings, "actors": sorted(actors), "elapsed_seconds": monotonic() - started}
    except BaseException:
        stop.set()
        raise


def check(binary: Path, *, seed: int = 123) -> dict:
    command = [str(binary.resolve())]
    probe = subprocess.run(
        command, input=b'{"request_type":"current","schema_version":8,"request_id":"probe"}\n',
        capture_output=True, timeout=30, check=True)
    identity = load_json(probe.stdout)
    if (identity.get("protocol") != PROTOCOL
            or type(identity.get("schema_version")) is not int
            or identity["schema_version"] != 8):
        raise LimitedSessionError("unexpected probe protocol")
    catalog = _uint(identity.get("card_db_hash"), "card_db_hash")
    stop = Event()
    games: list[dict | None] = [None, None]
    started = monotonic()
    with ThreadPoolExecutor(max_workers=2, thread_name_prefix="limited-call-v8-replay") as pool:
        pending = {pool.submit(_check_game, command, catalog, seed, stop): index
                   for index in range(2)}
        try:
            for future in as_completed(pending):
                games[pending[future]] = future.result()
        except BaseException:
            stop.set()
            raise
    first, second = games
    assert first is not None and second is not None
    if first["transcript"] != second["transcript"] or first["terminal"] != second["terminal"]:
        raise LimitedSessionError("same-seed process transcripts differed")
    server_timings = first["timings"] + second["timings"]
    result = {"schema": "limited-call-client-check/v8", "seed": seed,
              "card_db_hash": catalog, "transcript_sha256": first["transcript"],
              "terminal": first["terminal"], "parallel_workers": 2, "actors": first["actors"],
              "replay_wall_seconds": monotonic() - started,
              "game_elapsed_seconds": [first["elapsed_seconds"], second["elapsed_seconds"]]}
    if server_timings:
        result["server_timings_v8"] = server_timings
    return result


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(check(args.binary), sort_keys=True))


if __name__ == "__main__":
    main()
