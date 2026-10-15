"""Bounded real-process check of the actor-facing Limited common Python client."""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import subprocess

from limited_common_session_v3 import LimitedCommonClientV3, PROTOCOL
from limited_decks_v1 import load_json, parse_dck
from limited_session_v1 import LimitedSessionError, _uint


def check(binary: Path, *, seed: int = 123) -> dict:
    command = [str(binary.resolve())]
    probe = subprocess.run(
        command, input=b'{"request_type":"current","schema_version":3,"request_id":"probe"}\n',
        capture_output=True, timeout=30, check=True)
    identity = load_json(probe.stdout)
    if (identity.get("protocol") != PROTOCOL
            or type(identity.get("schema_version")) is not int
            or identity["schema_version"] != 3):
        raise LimitedSessionError("unexpected probe protocol")
    catalog = _uint(identity.get("card_db_hash"), "card_db_hash")
    decks = (parse_dck("40 Forest"), parse_dck("40 Island"))
    transcripts = []
    server_timings = []
    terminal = None
    for _ in range(2):
        with LimitedCommonClientV3(command, expected_card_db_hash=catalog,
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
                state = reply["state"]
                if state["state_type"] == "terminal":
                    if state["classification"] != "natural":
                        raise LimitedSessionError("plumbing game did not end naturally")
                    terminal = state
                    break
                if state["state_type"] != "decision":
                    raise LimitedSessionError(f"unplayable common state: {state['code']}")
                actions = state["decision"]["actions"]
                index = next((a["action_index"] for a in actions
                              if a["label"].startswith(("Keep this hand", "Pass"))), 0)
                reply = client.step(reply, index)
            else:
                raise LimitedSessionError("plumbing game exceeded its bound")
            transcripts.append(client.transcript.hexdigest())
        for line in client.stderr:
            prefix = b"LIMITED_COMMON_TIMING_V3 "
            if line.startswith(prefix):
                server_timings.append(load_json(line[len(prefix):]))
    if transcripts[0] != transcripts[1]:
        raise LimitedSessionError("same-seed process transcripts differed")
    result = {"schema": "limited-common-client-check/v3", "seed": seed,
            "card_db_hash": catalog, "transcript_sha256": transcripts[0],
            "terminal": terminal}
    if server_timings:
        result["server_timings_v3"] = server_timings
    return result


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(check(args.binary), sort_keys=True))


if __name__ == "__main__":
    main()
