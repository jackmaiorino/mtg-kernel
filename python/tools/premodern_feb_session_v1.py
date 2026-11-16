"""Trusted two-seat client for kernel_limited_env --premodern-feb-v1."""
from __future__ import annotations

from copy import deepcopy
import math
from typing import Any, Sequence

from limited_elenda_session_v4 import _same_json_value
from limited_session_v1 import LimitedClientV1, LimitedSessionError, _uint

PROTOCOL = "kernel_premodern_feb_jsonl"
PROFILE = "mtg-kernel-premodern-feb-ordered/v1"
SCHEMA_VERSION = 1
SEATS = ("p0", "p1")


def _packet(value: Any, fields: set[str], description: str) -> dict[str, Any]:
    if type(value) is not dict or set(value) != fields:
        raise LimitedSessionError(f"invalid {description}")
    return value


def _u16_limbs(value: Any, width: int) -> bool:
    return (type(value) is list and len(value) == width
            and all(type(limb) is int and 0 <= limb <= 0xFFFF for limb in value))


def _tree(value: Any) -> None:
    packet = _packet(value, {"rows", "topology"}, "FEB numeric tree")
    rows, topology = packet["rows"], packet["topology"]
    if type(rows) is not list or type(topology) is not list or not rows or len(rows) != len(topology):
        raise LimitedSessionError("invalid FEB numeric tree dimensions")
    if not all(_u16_limbs(row, 12) for row in rows):
        raise LimitedSessionError("invalid FEB numeric tree row")
    for entry in topology:
        if type(entry) is not list or len(entry) != 2 or not all(_u16_limbs(part, 4) for part in entry):
            raise LimitedSessionError("invalid FEB numeric tree topology")


def _snapshot_id(value: Any) -> str:
    if type(value) is not str or len(value) != 32 or any(c not in "0123456789abcdef" for c in value):
        raise LimitedSessionError("invalid FEB snapshot handle")
    return value


class FebClientV1(LimitedClientV1):
    def __init__(self, command: Sequence[str], *, expected_card_db_hash: int,
                 timeout_s: float = 10.0) -> None:
        if not math.isfinite(timeout_s) or timeout_s <= 0:
            raise ValueError("timeout_s must be finite and positive")
        self.expected_card_db_hash = _uint(expected_card_db_hash, "card_db_hash")
        self._issued: dict[str, Any] | None = None
        self._accepted: tuple[dict[str, Any], int, str] | None = None
        self._snapshots: set[str] = set()
        super().__init__(command, timeout_s=timeout_s)
        self.schema_version = SCHEMA_VERSION

    def _validate_reply(self, reply: dict[str, Any], request: dict[str, Any]) -> str:
        if (type(reply) is not dict or reply.get("protocol") != PROTOCOL or reply.get("profile") != PROFILE
                or type(reply.get("schema_version")) is not int
                or reply["schema_version"] != SCHEMA_VERSION
                or reply.get("request_id") != request["request_id"]):
            raise LimitedSessionError("unexpected FEB protocol identity")
        if _uint(reply.get("card_db_hash"), "card_db_hash") != self.expected_card_db_hash:
            raise LimitedSessionError("unexpected FEB catalog identity")
        header = {"protocol", "schema_version", "request_id", "profile", "card_db_hash", "response_type"}
        kind = reply.get("response_type")
        if kind == "error":
            _packet(reply, header | {"code"}, "FEB error")
            if not isinstance(reply["code"], str) or not reply["code"]:
                raise LimitedSessionError("invalid FEB error code")
            return kind
        if kind != "state":
            raise LimitedSessionError("unknown FEB response type")

        command = request["request_type"]
        expected_fields = header | {"state"}
        if command == "step":
            expected_fields.add("receipt")
            receipt = {key: request[key] for key in ("prompt", "native_ordinal", "submission")}
            if not _same_json_value(reply.get("receipt"), receipt):
                raise LimitedSessionError("FEB action receipt changed")
        if command == "snapshot":
            expected_fields.add("snapshot_id")
            _snapshot_id(reply.get("snapshot_id"))
        _packet(reply, expected_fields, "FEB state response")

        state = reply["state"]
        if type(state) is not dict:
            raise LimitedSessionError("missing FEB state")
        episode = _uint(state.get("episode_id"), "episode_id")
        generation = _uint(state.get("authority_generation"), "authority_generation")
        if command == "reset" and episode != request["episode_id"]:
            raise LimitedSessionError("FEB reset changed the episode")
        if self._issued is not None:
            previous = self._issued["authority_generation"]
            if command != "reset" and episode != self._issued["episode_id"]:
                raise LimitedSessionError("FEB response changed the episode")
            if ((command in ("reset", "restore") and generation <= previous)
                    or (command not in ("reset", "restore") and generation != previous)):
                raise LimitedSessionError("unexpected FEB authority generation")
        if generation == 0:
            raise LimitedSessionError("invalid FEB authority generation")

        common = {"state_type", "episode_id", "authority_generation"}
        state_type = state.get("state_type")
        if state_type == "decision":
            _packet(state, common | {"prompt", "numeric", "human"}, "FEB decision")
            self._validate_decision(state)
        elif state_type == "terminal":
            _packet(state, common | {"winner"}, "FEB terminal")
            if state["winner"] not in (*SEATS, None):
                raise LimitedSessionError("invalid FEB winner")
        elif state_type == "native_halt":
            _packet(state, common, "FEB native halt")
        elif state_type == "unsupported":
            _packet(state, common | {"code"}, "FEB unsupported state")
            if not isinstance(state["code"], str) or not state["code"]:
                raise LimitedSessionError("invalid FEB unsupported code")
        else:
            raise LimitedSessionError("unknown FEB state type")
        return kind

    def _validate_decision(self, state: dict[str, Any]) -> None:
        prompt = _packet(state["prompt"], {"episode_id", "authority_generation", "actor", "prompt_seq"},
                         "FEB prompt")
        actor = prompt["actor"]
        step = _uint(prompt["prompt_seq"], "prompt_seq")
        if (actor not in SEATS or _uint(prompt["episode_id"], "prompt episode_id") != state["episode_id"]
                or _uint(prompt["authority_generation"], "prompt authority_generation") != state["authority_generation"]):
            raise LimitedSessionError("FEB prompt authority changed")
        numeric = _packet(state["numeric"], {"profile", "card_db_hash", "actor", "step", "observation", "actions"},
                          "FEB numeric view")
        human = _packet(state["human"], {"profile", "actor", "step", "ordered", "scene", "actions"},
                        "FEB Human view")
        for view in (numeric, human):
            if view["actor"] != actor or view["profile"] != PROFILE or _uint(view["step"], "view step") != step:
                raise LimitedSessionError("FEB views disagree on actor or step")
        if _uint(numeric["card_db_hash"], "numeric catalog") != self.expected_card_db_hash:
            raise LimitedSessionError("FEB numeric view catalog changed")
        _tree(numeric["observation"])
        actions, meanings = numeric["actions"], human["actions"]
        if type(actions) is not list or type(meanings) is not list or not actions or len(actions) != len(meanings):
            raise LimitedSessionError("FEB views disagree on legal ordinals")
        for ordinal, (tree, meaning) in enumerate(zip(actions, meanings)):
            _tree(tree)
            kind = meaning.get("kind") if type(meaning) is dict else None
            field = {"supported": "label", "unsupported": "action_kind"}.get(kind) if isinstance(kind, str) else None
            if (field is None or set(meaning) != {"kind", "native_ordinal", field}
                    or _uint(meaning["native_ordinal"], "native_ordinal") != ordinal
                    or not isinstance(meaning[field], str) or not meaning[field]):
                raise LimitedSessionError("invalid FEB Human ordinal")

    def _server_error(self, reply: dict[str, Any]) -> str:
        return reply["code"]

    def exchange(self, payload: dict[str, Any]) -> dict[str, Any]:
        reply = super().exchange(payload)
        self._issued = deepcopy(reply["state"])
        if payload["request_type"] in ("reset", "restore"):
            self._accepted = None
        if payload["request_type"] == "reset":
            self._snapshots.clear()
        if payload["request_type"] == "snapshot":
            self._snapshots.add(reply["snapshot_id"])
        return reply

    def reset(self, *, episode_id: int = 0, seed: int = 1,
              starting_player: str = "p0") -> dict[str, Any]:
        if starting_player not in SEATS:
            raise LimitedSessionError("invalid FEB starting seat")
        return self.exchange({
            "request_type": "reset", "expected_profile": PROFILE,
            "expected_card_db_hash": self.expected_card_db_hash,
            "episode_id": _uint(episode_id, "episode_id"), "seed": _uint(seed, "seed"),
            "starting_player": starting_player,
        })

    def current(self) -> dict[str, Any]:
        return self.exchange({"request_type": "current"})

    def step(self, reply: dict[str, Any], native_ordinal: int, *, human: bool = True) -> dict[str, Any]:
        state = reply.get("state") if type(reply) is dict else None
        submission = "human" if human else "numeric"
        if type(state) is not dict or state.get("state_type") != "decision":
            raise LimitedSessionError("stale or unavailable FEB prompt")
        if _same_json_value(state, self._issued):
            issued = self._issued
        elif (self._accepted is not None and _same_json_value(state, self._accepted[0])
              and native_ordinal == self._accepted[1] and submission == self._accepted[2]):
            issued = self._accepted[0]
        else:
            raise LimitedSessionError("stale or unavailable FEB prompt")
        assert issued is not None
        actions = issued["human"]["actions"]
        if _uint(native_ordinal, "native_ordinal") >= len(actions):
            raise LimitedSessionError("FEB ordinal is outside the current menu")
        if human and actions[native_ordinal]["kind"] != "supported":
            raise LimitedSessionError("unsupported_human_action")
        response = self.exchange({
            "request_type": "step", "prompt": issued["prompt"],
            "native_ordinal": native_ordinal, "submission": submission,
        })
        self._accepted = (issued, native_ordinal, submission)
        return response

    def snapshot(self) -> dict[str, Any]:
        return self.exchange({"request_type": "snapshot"})

    def restore(self, snapshot_id: str) -> dict[str, Any]:
        if _snapshot_id(snapshot_id) not in self._snapshots:
            raise LimitedSessionError("snapshot does not belong to the current FEB owner")
        return self.exchange({"request_type": "restore", "snapshot_id": snapshot_id})
