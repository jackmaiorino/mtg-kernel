"""Minimal external client for kernel_limited_env and a bounded plumbing smoke.

Standalone standard-library tooling. This is not a trainer or a playing policy.
"""

from __future__ import annotations

import argparse
from collections import deque
import hashlib
import json
from pathlib import Path
import queue
import subprocess
import sys
import threading
from typing import Any, Sequence

from limited_decks_v1 import ImportedDeck, load_json, parse_dck


MAX_LINE_BYTES = 8 * 1024 * 1024


class LimitedSessionError(RuntimeError):
    pass


def _uint(value: Any, field: str) -> int:
    if type(value) is not int or not 0 <= value <= 18_446_744_073_709_551_615:
        raise LimitedSessionError(f"invalid {field}")
    return value


class LimitedClientV1:
    def __init__(self, command: Sequence[str], *, timeout_s: float = 10.0,
                 engine_priority: bool = False) -> None:
        if timeout_s <= 0:
            raise ValueError("timeout_s must be positive")
        self.process = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                        stderr=subprocess.PIPE)
        self.timeout_s = timeout_s
        self.schema_version = 2 if engine_priority else 1
        self.lines: queue.Queue[bytes | None] = queue.Queue()
        self.stderr: deque[bytes] = deque(maxlen=20)
        self.next_request = 0
        self.transcript = hashlib.sha256()
        self.last_reply: dict[str, Any] | None = None
        self.closed = False
        self.reader = threading.Thread(target=self._read_stdout, daemon=True)
        self.error_reader = threading.Thread(target=self._read_stderr, daemon=True)
        self.reader.start()
        self.error_reader.start()

    def _read_stdout(self) -> None:
        assert self.process.stdout is not None
        while True:
            line = self.process.stdout.readline(MAX_LINE_BYTES + 1)
            self.lines.put(line or None)
            if not line or len(line) > MAX_LINE_BYTES:
                return

    def _read_stderr(self) -> None:
        assert self.process.stderr is not None
        for line in iter(self.process.stderr.readline, b""):
            self.stderr.append(line)

    def exchange(self, payload: dict[str, Any]) -> dict[str, Any]:
        if self.closed:
            raise LimitedSessionError("client is closed")
        request = {**payload, "schema_version": self.schema_version, "request_id": str(self.next_request)}
        self.next_request += 1
        raw = (json.dumps(request, ensure_ascii=False, separators=(",", ":"), allow_nan=False) + "\n").encode("utf-8")
        if len(raw) > MAX_LINE_BYTES:
            raise LimitedSessionError("request exceeds the line-size limit")
        try:
            assert self.process.stdin is not None
            self.process.stdin.write(raw)
            self.process.stdin.flush()
            line = self.lines.get(timeout=self.timeout_s)
            if line is None:
                raise LimitedSessionError("engine exited before replying")
            if len(line) > MAX_LINE_BYTES:
                raise LimitedSessionError("engine response exceeds the line-size limit")
            reply = load_json(line)
            if reply.get("protocol") != "kernel_limited_jsonl" or type(reply.get("schema_version")) is not int or reply["schema_version"] != self.schema_version:
                raise LimitedSessionError("unexpected Limited protocol identity")
            if self.schema_version == 2 and reply.get("priority_mode") != "engine_windows_v1":
                raise LimitedSessionError("unexpected Limited priority mode")
            if self.schema_version == 1 and "priority_mode" in reply:
                raise LimitedSessionError("unexpected priority mode in schema 1")
            if reply.get("request_id") != request["request_id"]:
                raise LimitedSessionError("response request_id mismatch")
            _uint(reply.get("card_db_hash"), "card_db_hash")
            kind = reply.get("response_type")
            if kind not in ("decision", "terminal", "error"):
                raise LimitedSessionError("unknown response type")
            if kind == "decision":
                decision = reply.get("decision")
                if not isinstance(decision, dict) or decision.get("schema_version") != 5:
                    raise LimitedSessionError("expected a nested policy V5 decision")
                _uint(decision.get("episode_id"), "episode_id")
                _uint(decision.get("step"), "step")
                actions = decision.get("legal_actions")
                if not isinstance(actions, list) or not actions:
                    raise LimitedSessionError("decision has no legal actions")
                for index, action in enumerate(actions):
                    if not isinstance(action, dict) or type(action.get("selected_index")) is not int or action["selected_index"] != index or not isinstance(action.get("stable_id"), str) or not action["stable_id"]:
                        raise LimitedSessionError("invalid legal action identity")
            elif kind == "terminal" and not isinstance(reply.get("terminal"), dict):
                raise LimitedSessionError("missing terminal payload")
            elif kind == "error" and (not isinstance(reply.get("error"), dict) or not isinstance(reply["error"].get("code"), str) or not isinstance(reply["error"].get("message"), str)):
                raise LimitedSessionError("invalid error payload")
        except queue.Empty as exc:
            self.close()
            raise LimitedSessionError("engine response timed out") from exc
        except (OSError, ValueError, UnicodeError, LimitedSessionError) as exc:
            self.close()
            raise LimitedSessionError(str(exc)) from exc
        self.transcript.update(raw)
        self.transcript.update(line)
        if kind == "error":
            error = reply["error"]
            raise LimitedSessionError(f"{error['code']}: {error['message']}")
        self.last_reply = reply
        return reply

    def reset(self, decks: tuple[ImportedDeck, ImportedDeck], *, episode_id: int = 0,
              env_seed: int = 1, max_steps: int = 4096) -> dict[str, Any]:
        return self.exchange({
            "request_type": "reset", "episode_id": episode_id, "env_seed": env_seed,
            "max_physical_decisions": max_steps, "max_policy_steps": max_steps,
            "decks": [{"cards": [{"name": row.name, "count": row.count} for row in deck.mainboard]} for deck in decks],
        })

    def step(self, decision: dict[str, Any], action_index: int) -> dict[str, Any]:
        if type(action_index) is not int or not 0 <= action_index < len(decision["legal_actions"]):
            raise LimitedSessionError("action index is outside the current menu")
        action = decision["legal_actions"][action_index]
        return self.exchange({
            "request_type": "step", "episode_id": decision["episode_id"],
            "expected_step": decision["step"], "selected_index": action["selected_index"],
            "selected_action_id": action["stable_id"],
        })

    def close(self) -> None:
        if self.closed:
            return
        self.closed = True
        if self.process.stdin is not None:
            try:
                self.process.stdin.close()
            except OSError:
                pass
        try:
            self.process.wait(timeout=1)
        except subprocess.TimeoutExpired:
            self.process.kill()
            self.process.wait()
        self.reader.join(timeout=1)
        self.error_reader.join(timeout=1)
        for stream in (self.process.stdout, self.process.stderr):
            if stream is not None:
                stream.close()

    def __enter__(self) -> LimitedClientV1:
        return self

    def __exit__(self, *_: Any) -> None:
        self.close()


def smoke(command: Sequence[str], decks: tuple[ImportedDeck, ImportedDeck], *, seed: int = 1,
          max_steps: int = 4096, engine_priority: bool = False) -> dict[str, Any]:
    with LimitedClientV1(command, engine_priority=engine_priority) as client:
        reply = client.reset(decks, env_seed=seed, max_steps=max_steps)
        for _ in range(max_steps):
            if reply["response_type"] == "terminal":
                return {"schema": f"kernel_limited_smoke/v{client.schema_version}", "env_seed": seed,
                        "transcript_sha256": client.transcript.hexdigest(), "terminal": reply["terminal"]}
            decision = reply["decision"]
            actions = decision["legal_actions"]
            index = next((i for i, action in enumerate(actions) if action.get("semantic", {}).get("action_kind") == "pass"), 0)
            reply = client.step(decision, index)
        if reply["response_type"] == "terminal":
            return {"schema": f"kernel_limited_smoke/v{client.schema_version}", "env_seed": seed,
                    "transcript_sha256": client.transcript.hexdigest(), "terminal": reply["terminal"]}
        raise LimitedSessionError("smoke exceeded its step bound")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--deck", type=Path, action="append", required=True)
    parser.add_argument("--seed", type=int, default=1)
    parser.add_argument("--max-steps", type=int, default=4096)
    parser.add_argument("--engine-priority-v1", action="store_true",
                        help="opt into schema 2 and expose engine priority windows")
    args = parser.parse_args(argv)
    try:
        if len(args.deck) != 2 or args.max_steps < 1 or not 0 <= args.seed <= 18_446_744_073_709_551_615:
            raise ValueError("provide two --deck paths, a u64 seed and a positive step bound")
        decks = tuple(parse_dck(path.read_text(encoding="utf-8-sig")) for path in args.deck)
        command = [str(args.binary.resolve())]
        if args.engine_priority_v1:
            command.append("--engine-priority-v1")
        result = smoke(command, decks, seed=args.seed, max_steps=args.max_steps,
                       engine_priority=args.engine_priority_v1)
        print(json.dumps(result, sort_keys=True, indent=2))
        return 0 if result["terminal"].get("terminal_classification") == "natural" else 2
    except (OSError, ValueError, LimitedSessionError) as exc:
        print(json.dumps({"error": str(exc)}), file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
