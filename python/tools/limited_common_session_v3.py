"""Client for the trusted two-seat kernel_limited_common_jsonl host.

Schema 3 requires exact V51 battlefield values on every wrapped card.
Replies contain only the current actor's safe prompt. A network host must route
them to that actor. This client is transport tooling, not a StateSpec injector,
MageZero model adapter, belief sampler or playing policy.
"""
from __future__ import annotations

from copy import deepcopy
import math
from typing import Any, Sequence

from limited_decks_v1 import ImportedDeck
from limited_session_v1 import LimitedClientV1, LimitedSessionError, _uint

PROTOCOL = "kernel_limited_common_jsonl"
DECISION_SCHEMA = "mtg-kernel-limited-common-decision/v3"


def _bounded_int(value: Any, field: str, minimum: int, maximum: int) -> int:
    if type(value) is not int or not minimum <= value <= maximum:
        raise LimitedSessionError(f"invalid exact battlefield {field}")
    return value


def _validate_battlefield(visible: dict[str, Any]) -> None:
    public = visible.get("public")
    battlefield = public.get("battlefield") if isinstance(public, dict) else None
    if not isinstance(battlefield, list) or len(battlefield) != 2:
        raise LimitedSessionError("missing two-seat exact battlefield roster")
    handles: set[str] = set()
    for seat, cards in zip(("p0", "p1"), battlefield):
        if not isinstance(cards, list):
            raise LimitedSessionError("invalid exact battlefield seat roster")
        for row in cards:
            if not isinstance(row, dict) or not isinstance(row.get("card"), dict):
                raise LimitedSessionError("missing wrapped battlefield card")
            reference = row["card"].get("stable")
            if (not isinstance(reference, dict)
                    or not isinstance(reference.get("handle"), str)
                    or not reference["handle"] or reference["handle"] in handles
                    or not isinstance(reference.get("name"), str) or not reference["name"]
                    or reference.get("owner") not in ("p0", "p1")
                    or reference.get("controller") != seat
                    or reference.get("zone") != "Battlefield"):
                raise LimitedSessionError("invalid safe battlefield card identity")
            handles.add(reference["handle"])
            _bounded_int(row.get("marked_damage"), "marked_damage", 0, (1 << 32) - 1)
            _bounded_int(row.get("plus_one_counters"), "plus_one_counters", 0, (1 << 31) - 1)
            _bounded_int(row.get("soul"), "soul", 0, (1 << 31) - 1)


def _same_json_value(left: Any, right: Any) -> bool:
    """Compare issued JSON without Python's bool/int/float equality coercion.

    The retained side is a private deepcopy of a decoded, validated reply.
    Accept only exact JSON value types and string keys on the caller side.
    Dictionary insertion order has no effect on prompt identity.
    """
    pending = [(left, right)]
    while pending:
        actual, expected = pending.pop()
        kind = type(actual)
        if kind is not type(expected):
            return False
        if kind is dict:
            if (any(type(key) is not str for key in actual)
                    or any(type(key) is not str for key in expected)
                    or actual.keys() != expected.keys()):
                return False
            pending.extend((actual[key], expected[key]) for key in actual)
        elif kind is list:
            if len(actual) != len(expected):
                return False
            pending.extend(zip(actual, expected))
        elif kind in (str, int, bool, type(None)):
            if actual != expected:
                return False
        elif kind is float:
            if not math.isfinite(actual) or not math.isfinite(expected) or actual != expected:
                return False
        else:
            return False
    return True


class LimitedCommonClientV3(LimitedClientV1):
    def __init__(self, command: Sequence[str], *, expected_card_db_hash: int,
                 timeout_s: float = 10.0) -> None:
        self.expected_card_db_hash = _uint(expected_card_db_hash, "card_db_hash")
        if not math.isfinite(timeout_s) or timeout_s <= 0:
            raise ValueError("timeout_s must be finite and positive")
        self._issued_state: dict[str, Any] | None = None
        self._snapshots: set[int] = set()
        super().__init__(command, timeout_s=timeout_s)
        self.schema_version = 3

    def _validate_reply(self, reply: dict[str, Any], request: dict[str, Any]) -> str:
        if (reply.get("protocol") != PROTOCOL
                or type(reply.get("schema_version")) is not int
                or reply["schema_version"] != 3):
            raise LimitedSessionError("unexpected Limited common protocol identity")
        if reply.get("request_id") != request["request_id"]:
            raise LimitedSessionError("response request_id mismatch")
        catalog = _uint(reply.get("card_db_hash"), "card_db_hash")
        if catalog != self.expected_card_db_hash:
            raise LimitedSessionError("unexpected catalog identity")
        for field, expected in (("priority_mode", "engine_windows_v1"),
                                ("combat_rules", "foundations_v1"),
                                ("mulligan_rules", "london_v1")):
            if reply.get(field) != expected:
                raise LimitedSessionError(f"unexpected {field}")
        kind = reply.get("response_type")
        if kind == "error":
            if not isinstance(reply.get("code"), str) or not reply["code"]:
                raise LimitedSessionError("invalid error code")
            return kind
        if kind != "state":
            raise LimitedSessionError("unexpected state response")
        state = reply.get("state")
        if not isinstance(state, dict):
            raise LimitedSessionError("missing common state")
        _uint(state.get("episode_id"), "episode_id")
        state_type = state.get("state_type")
        if state_type == "decision":
            _uint(state.get("authority_generation"), "authority_generation")
            decision = state.get("decision")
            if not isinstance(decision, dict) or decision.get("schema") != DECISION_SCHEMA:
                raise LimitedSessionError("unexpected common decision schema")
            if decision.get("actor") not in ("p0", "p1"):
                raise LimitedSessionError("invalid decision actor")
            _uint(decision.get("prompt_seq"), "prompt_seq")
            if not isinstance(decision.get("state"), dict):
                raise LimitedSessionError("missing visible state")
            _validate_battlefield(decision["state"])
            actions = decision.get("actions")
            if not isinstance(actions, list) or not actions:
                raise LimitedSessionError("decision has no legal actions")
            for index, action in enumerate(actions):
                if (not isinstance(action, dict)
                        or type(action.get("action_index")) is not int
                        or action["action_index"] != index
                        or not isinstance(action.get("label"), str)
                        or not action["label"]):
                    raise LimitedSessionError("invalid common action identity")
        elif state_type == "terminal":
            terminal_codes = {"natural": "natural_game_over",
                              "truncated": "decision_cap", "halted": "fail_closed"}
            classification = state.get("classification")
            if (not isinstance(classification, str) or classification not in terminal_codes
                    or state.get("code") != terminal_codes[classification]):
                raise LimitedSessionError("inconsistent terminal classification/code")
            if state.get("winner") not in (None, "p0", "p1"):
                raise LimitedSessionError("invalid terminal winner")
            _uint(state.get("policy_steps"), "policy_steps")
            _uint(state.get("physical_decisions"), "physical_decisions")
        elif state_type == "unsupported":
            if not isinstance(state.get("code"), str) or not state["code"]:
                raise LimitedSessionError("invalid unsupported code")
        else:
            raise LimitedSessionError("unknown common state type")
        if request["request_type"] == "snapshot":
            _uint(reply.get("snapshot_id"), "snapshot_id")
        return kind

    def _server_error(self, reply: dict[str, Any]) -> str:
        return reply["code"]

    def exchange(self, payload: dict[str, Any]) -> dict[str, Any]:
        reply = super().exchange(payload)
        self._issued_state = deepcopy(reply["state"])
        if payload["request_type"] == "reset":
            self._snapshots.clear()
        if payload["request_type"] == "snapshot":
            self._snapshots.add(reply["snapshot_id"])
        return reply

    def reset(self, decks: tuple[ImportedDeck, ImportedDeck], *, episode_id: int = 0,
              env_seed: int = 1, starting_player: str = "p0",
              max_steps: int = 4096) -> dict[str, Any]:
        if len(decks) != 2 or starting_player not in ("p0", "p1"):
            raise LimitedSessionError("provide two decks and a valid starting seat")
        _uint(episode_id, "episode_id")
        _uint(env_seed, "env_seed")
        if _uint(max_steps, "max_steps") == 0:
            raise LimitedSessionError("max_steps must be positive")
        return self.exchange({
            "request_type": "reset", "expected_card_db_hash": self.expected_card_db_hash,
            "episode_id": episode_id, "env_seed": env_seed,
            "starting_player": starting_player,
            "max_physical_decisions": max_steps, "max_policy_steps": max_steps,
            "decks": [{"cards": [{"name": row.name, "count": row.count}
                                 for row in deck.mainboard]} for deck in decks],
        })

    def current(self) -> dict[str, Any]:
        return self.exchange({"request_type": "current"})

    def step(self, reply: dict[str, Any], action_index: int) -> dict[str, Any]:
        state = self._issued_state
        if (state is None or state.get("state_type") != "decision"
                or type(reply) is not dict or not _same_json_value(reply.get("state"), state)):
            raise LimitedSessionError("stale or unavailable common prompt")
        actions = state["decision"]["actions"]
        if type(action_index) is not int or not 0 <= action_index < len(actions):
            raise LimitedSessionError("action index is outside the current menu")
        decision = state["decision"]
        return self.exchange({
            "request_type": "step", "episode_id": state["episode_id"],
            "authority_generation": state["authority_generation"],
            "actor": decision["actor"], "prompt_seq": decision["prompt_seq"],
            "action_index": action_index,
        })

    def snapshot(self) -> dict[str, Any]:
        return self.exchange({"request_type": "snapshot"})

    def restore(self, snapshot_id: int) -> dict[str, Any]:
        _uint(snapshot_id, "snapshot_id")
        if snapshot_id not in self._snapshots:
            raise LimitedSessionError("snapshot does not belong to the current game")
        return self.exchange({"request_type": "restore", "snapshot_id": snapshot_id})
