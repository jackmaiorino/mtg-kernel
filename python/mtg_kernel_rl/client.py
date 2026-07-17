"""Strict subprocess JSONL client for the Rust kernel RL environment."""

from __future__ import annotations

import copy
import json
import math
import os
import queue
import subprocess
import threading
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Iterable

SCHEMA_VERSION = 5
PROTOCOL_NAME = "kernel_rl_jsonl"
PROTOCOL_VERSION = 5
SURFACE_VERSION = 2
POLICY_SURFACE_VERSION = 5
MAX_LINE_BYTES = 8 * 1024 * 1024
U32 = 4_294_967_295
U64 = 18_446_744_073_709_551_615
I32_MIN = -2_147_483_648
I32_MAX = 2_147_483_647


class KernelRlError(Exception):
    pass


class ProtocolError(KernelRlError):
    pass


class EnvProcessError(KernelRlError):
    pass


@dataclass(frozen=True)
class Decision:
    episode_id: int
    step: int
    physical_decision_id: int
    substep_index: int
    substep_count: int
    acting_player: str
    observation: dict[str, Any]
    legal_actions: list[dict[str, Any]]
    provenance: dict[str, Any]
    deck_ids: tuple[str, str]
    deck_hashes: tuple[int, int]


@dataclass(frozen=True)
class Terminal:
    episode_id: int
    terminal_outcome: str
    terminal_classification: str
    terminal_code: str
    winner: str | None
    terminal_reward: list[int]
    policy_step_count: int
    physical_decision_count: int
    provenance: dict[str, Any]
    deck_ids: tuple[str, str]
    deck_hashes: tuple[int, int]


def _reject_duplicate_keys(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    out: dict[str, Any] = {}
    for key, value in pairs:
        if key in out:
            raise ProtocolError(f"duplicate JSON key: {key}")
        out[key] = value
    return out


def _reject_constant(value: str) -> None:
    raise ProtocolError(f"non-finite JSON constant rejected: {value}")


def _parse_float(value: str) -> float:
    parsed = float(value)
    if not math.isfinite(parsed):
        raise ProtocolError(f"non-finite JSON float rejected: {value}")
    return parsed


def _reject_nonfinite_tree(value: Any, context: str = "$") -> None:
    if isinstance(value, float) and not math.isfinite(value):
        raise ProtocolError(f"non-finite JSON float at {context}")
    if isinstance(value, dict):
        for key, child in value.items():
            _reject_nonfinite_tree(child, f"{context}.{key}")
    elif isinstance(value, list):
        for i, child in enumerate(value):
            _reject_nonfinite_tree(child, f"{context}[{i}]")


def strict_json_loads(line: str) -> dict[str, Any]:
    try:
        value = json.loads(
            line,
            object_pairs_hook=_reject_duplicate_keys,
            parse_constant=_reject_constant,
            parse_float=_parse_float,
        )
    except ProtocolError:
        raise
    except json.JSONDecodeError as exc:
        raise ProtocolError(f"stdout line is not strict JSON: {exc}") from exc
    if not isinstance(value, dict):
        raise ProtocolError("stdout response is not a JSON object")
    _reject_nonfinite_tree(value)
    return value


def strict_json_dumps(value: dict[str, Any]) -> bytes:
    return (json.dumps(value, ensure_ascii=False, separators=(",", ":"), allow_nan=False) + "\n").encode(
        "utf-8"
    )


def _keys(value: dict[str, Any], expected: Iterable[str], context: str) -> None:
    expected_set = set(expected)
    actual = set(value)
    missing = expected_set - actual
    extra = actual - expected_set
    if missing or extra:
        raise ProtocolError(f"{context} fields mismatch: missing={sorted(missing)} extra={sorted(extra)}")


def _int(value: Any, context: str, *, minimum: int | None = None, maximum: int | None = None) -> int:
    if type(value) is not int:
        raise ProtocolError(f"{context} must be an integer, got {type(value).__name__}")
    if minimum is not None and value < minimum:
        raise ProtocolError(f"{context} must be >= {minimum}")
    if maximum is not None and value > maximum:
        raise ProtocolError(f"{context} must be <= {maximum}")
    return value


def _str(value: Any, context: str) -> str:
    if type(value) is not str:
        raise ProtocolError(f"{context} must be a string")
    return value


def _nonempty_str(value: Any, context: str) -> str:
    text = _str(value, context)
    if not text:
        raise ProtocolError(f"{context} must be nonempty")
    return text


def _bool(value: Any, context: str) -> bool:
    if type(value) is not bool:
        raise ProtocolError(f"{context} must be a bool")
    return value


def _list(value: Any, context: str, length: int | None = None) -> list[Any]:
    if not isinstance(value, list):
        raise ProtocolError(f"{context} must be a list")
    if length is not None and len(value) != length:
        raise ProtocolError(f"{context} must have length {length}")
    return value


def _optional_seat(value: Any, context: str) -> str | None:
    if value is None:
        return None
    return _seat(value, context)


def _seat(value: Any, context: str) -> str:
    seat = _str(value, context)
    if seat not in {"p0", "p1"}:
        raise ProtocolError(f"{context} must be p0 or p1")
    return seat


def _validate_provenance(value: Any, expected: dict[str, Any] | None) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise ProtocolError("provenance must be an object")
    _keys(
        value,
        [
            "protocol",
            "protocol_version",
            "schema_version",
            "kernel_version",
            "surface_version",
            "policy_surface_version",
            "card_db_hash",
        ],
        "provenance",
    )
    if value["protocol"] != PROTOCOL_NAME:
        raise ProtocolError("unexpected protocol name")
    if _int(value["protocol_version"], "provenance.protocol_version", minimum=0, maximum=U32) != PROTOCOL_VERSION:
        raise ProtocolError("unexpected protocol version")
    if _int(value["schema_version"], "provenance.schema_version", minimum=0, maximum=U32) != SCHEMA_VERSION:
        raise ProtocolError("unexpected provenance schema version")
    _str(value["kernel_version"], "provenance.kernel_version")
    if _int(value["surface_version"], "provenance.surface_version", minimum=0, maximum=U32) != SURFACE_VERSION:
        raise ProtocolError("unexpected surface version")
    if _int(value["policy_surface_version"], "provenance.policy_surface_version", minimum=0, maximum=U32) != POLICY_SURFACE_VERSION:
        raise ProtocolError("unexpected policy surface version")
    _int(value["card_db_hash"], "provenance.card_db_hash", minimum=0, maximum=U64)
    if expected is not None and value != expected:
        raise ProtocolError("provenance drifted within process")
    return value


def _validate_legal_actions(actions: Any) -> list[dict[str, Any]]:
    from .features import FeatureSchemaError, validate_legal_actions_contract

    try:
        return validate_legal_actions_contract(actions)
    except FeatureSchemaError as exc:
        raise ProtocolError(str(exc)) from exc


def _validate_observation(value: Any, response: dict[str, Any], provenance: dict[str, Any]) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise ProtocolError("observation must be an object")
    _keys(
        value,
        [
            "schema_version",
            "kernel_version",
            "surface_version",
            "policy_surface_version",
            "card_db_hash",
            "acting_player",
            "step_index",
            "physical_decision_id",
            "substep_index",
            "substep_count",
            "projection",
            "own_hand",
            "known_library_cards",
            "known_hand_cards",
            "visible_projection_hash",
        ],
        "observation",
    )
    if _int(value["schema_version"], "observation.schema_version", minimum=0, maximum=U32) != SCHEMA_VERSION:
        raise ProtocolError("observation schema mismatch")
    if value["kernel_version"] != provenance["kernel_version"]:
        raise ProtocolError("observation kernel_version drift")
    if _int(value["surface_version"], "observation.surface_version", minimum=0, maximum=U32) != provenance["surface_version"]:
        raise ProtocolError("observation surface_version drift")
    if _int(value["policy_surface_version"], "observation.policy_surface_version", minimum=0, maximum=U32) != provenance["policy_surface_version"]:
        raise ProtocolError("observation policy_surface_version drift")
    if _int(value["card_db_hash"], "observation.card_db_hash", minimum=0, maximum=U64) != provenance["card_db_hash"]:
        raise ProtocolError("observation card_db_hash drift")
    if _seat(value["acting_player"], "observation.acting_player") != response["acting_player"]:
        raise ProtocolError("observation acting_player mismatch")
    if _int(value["step_index"], "observation.step_index", minimum=0, maximum=U64) != response["step"]:
        raise ProtocolError("observation step_index mismatch")
    if _int(
        value["physical_decision_id"],
        "observation.physical_decision_id",
        minimum=0,
        maximum=U64,
    ) != response["physical_decision_id"]:
        raise ProtocolError("observation physical_decision_id mismatch")
    for field in ("substep_index", "substep_count"):
        if _int(value[field], f"observation.{field}", minimum=0, maximum=U32) != response[field]:
            raise ProtocolError(f"observation {field} mismatch")
    _int(value["visible_projection_hash"], "observation.visible_projection_hash", minimum=0, maximum=U64)
    from .features import assert_observation_classified
    from .features import FeatureSchemaError

    try:
        assert_observation_classified(value)
    except FeatureSchemaError as exc:
        raise ProtocolError(str(exc)) from exc
    return value


def _validate_reward(value: Any, context: str) -> list[int]:
    reward = _list(value, context, length=2)
    out = [_int(reward[0], f"{context}[0]", minimum=I32_MIN, maximum=I32_MAX), _int(reward[1], f"{context}[1]", minimum=I32_MIN, maximum=I32_MAX)]
    return out


def _validate_deck_identity(
    deck_ids_value: Any,
    deck_hashes_value: Any,
    *,
    context: str,
) -> tuple[tuple[str, str], tuple[int, int]]:
    deck_ids_raw = _list(deck_ids_value, f"{context}.deck_ids", length=2)
    deck_hashes_raw = _list(deck_hashes_value, f"{context}.deck_hashes", length=2)
    deck_ids = tuple(
        _nonempty_str(value, f"{context}.deck_ids[{index}]")
        for index, value in enumerate(deck_ids_raw)
    )
    deck_hashes = tuple(
        _int(value, f"{context}.deck_hashes[{index}]", minimum=0, maximum=U64)
        for index, value in enumerate(deck_hashes_raw)
    )
    return (deck_ids[0], deck_ids[1]), (deck_hashes[0], deck_hashes[1])


def _stdout_reader(stream: Any, output: queue.Queue[bytes | BaseException], max_line_bytes: int) -> None:
    try:
        while True:
            line = stream.readline()
            if line == b"":
                output.put(b"")
                return
            if len(line) > max_line_bytes:
                output.put(ProtocolError("stdout line exceeds maximum size"))
                return
            output.put(line)
    except BaseException as exc:
        output.put(exc)


def _stderr_reader(stream: Any, chunks: list[bytes]) -> None:
    while True:
        chunk = stream.readline()
        if chunk == b"":
            return
        chunks.append(chunk)


class KernelRlClient:
    def __init__(
        self,
        env_bin: str | os.PathLike[str],
        *,
        timeout_s: float = 5.0,
        max_line_bytes: int = MAX_LINE_BYTES,
    ) -> None:
        self.env_bin = str(Path(env_bin))
        if not Path(self.env_bin).is_file():
            raise FileNotFoundError(self.env_bin)
        self.timeout_s = timeout_s
        self.max_line_bytes = max_line_bytes
        self._proc = subprocess.Popen(
            [self.env_bin],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            shell=False,
        )
        assert self._proc.stdin is not None
        assert self._proc.stdout is not None
        assert self._proc.stderr is not None
        self._stdout: queue.Queue[bytes | BaseException] = queue.Queue()
        self._stderr_chunks: list[bytes] = []
        self._stdout_thread = threading.Thread(
            target=_stdout_reader,
            args=(self._proc.stdout, self._stdout, max_line_bytes),
            daemon=True,
        )
        self._stderr_thread = threading.Thread(
            target=_stderr_reader,
            args=(self._proc.stderr, self._stderr_chunks),
            daemon=True,
        )
        self._stdout_thread.start()
        self._stderr_thread.start()
        self._request_counter = 0
        self._provenance: dict[str, Any] | None = None
        self._deck_ids: tuple[str, str] | None = None
        self._deck_hashes: tuple[int, int] | None = None
        self._episode_id: int | None = None
        self._expected_step: int | None = None
        self._physical_decision_id: int | None = None
        self._substep_index: int | None = None
        self._substep_count: int | None = None
        self._group_actor: str | None = None
        self._group_stage: str | None = None
        self._group_attacker: dict[str, Any] | None = None
        self._group_candidates: list[dict[str, Any]] | None = None
        self._group_selected: list[dict[str, Any]] | None = None
        self._current_legal_actions: list[dict[str, Any]] | None = None
        self._closed = False

    def stderr_text(self) -> str:
        return b"".join(self._stderr_chunks).decode("utf-8", errors="replace")

    def close(self) -> None:
        if self._closed:
            return
        self._closed = True
        proc = self._proc
        try:
            if proc.stdin is not None:
                proc.stdin.close()
        except OSError:
            pass
        if proc.poll() is None:
            proc.terminate()
            try:
                proc.wait(timeout=2)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait(timeout=2)
        self._stdout_thread.join(timeout=1)
        self._stderr_thread.join(timeout=1)
        for stream in (proc.stdout, proc.stderr):
            try:
                if stream is not None:
                    stream.close()
            except OSError:
                pass

    def __enter__(self) -> "KernelRlClient":
        return self

    def __exit__(self, exc_type: object, exc: object, tb: object) -> None:
        self.close()

    def _next_request_id(self) -> str:
        self._request_counter += 1
        return f"py-{self._request_counter:012d}"

    def reset(
        self,
        *,
        episode_id: int,
        env_seed: int,
        max_physical_decisions: int,
        max_policy_steps: int,
        deck_ids: tuple[str, str] = ("Burn", "Burn"),
    ) -> Decision:
        validated_episode_id = _int(episode_id, "episode_id", minimum=0, maximum=U64)
        if not isinstance(deck_ids, tuple) or len(deck_ids) != 2:
            raise ProtocolError("deck_ids must be a two-item tuple")
        normalized_deck_ids = tuple(
            _nonempty_str(value, f"deck_ids[{index}]")
            for index, value in enumerate(deck_ids)
        )
        requested_deck_ids = (normalized_deck_ids[0], normalized_deck_ids[1])
        request = {
            "request_type": "reset",
            "schema_version": SCHEMA_VERSION,
            "request_id": self._next_request_id(),
            "deck_ids": list(requested_deck_ids),
            "episode_id": validated_episode_id,
            "env_seed": _int(env_seed, "env_seed", minimum=0, maximum=U64),
            "max_physical_decisions": _int(
                max_physical_decisions,
                "max_physical_decisions",
                minimum=0,
                maximum=U64,
            ),
            "max_policy_steps": _int(
                max_policy_steps,
                "max_policy_steps",
                minimum=0,
                maximum=U64,
            ),
        }
        response = self._exchange(request)
        if not isinstance(response, Decision):
            raise ProtocolError("reset did not return an initial decision")
        return response

    def step(self, selected_index: int, selected_action_id: str) -> Decision | Terminal:
        if self._episode_id is None or self._expected_step is None:
            raise ProtocolError("step before reset")
        selected_index = _int(selected_index, "selected_index", minimum=0, maximum=U32)
        selected_action_id = _str(selected_action_id, "selected_action_id")
        if self._current_legal_actions is None:
            raise ProtocolError("step without a validated current legal-action set")
        if selected_index >= len(self._current_legal_actions):
            raise ProtocolError("selected_index is outside the current legal-action set")
        if self._current_legal_actions[selected_index]["stable_id"] != selected_action_id:
            raise ProtocolError("selected_action_id does not match selected_index")
        request = {
            "request_type": "step",
            "schema_version": SCHEMA_VERSION,
            "request_id": self._next_request_id(),
            "episode_id": self._episode_id,
            "expected_step": self._expected_step,
            "selected_index": selected_index,
            "selected_action_id": selected_action_id,
        }
        return self._exchange(request)

    def _next_group_transcript(
        self,
        *,
        observation: dict[str, Any],
        request: dict[str, Any],
        substep_index: int,
        substep_count: int,
    ) -> tuple[
        str | None,
        dict[str, Any] | None,
        list[dict[str, Any]] | None,
        list[dict[str, Any]] | None,
    ]:
        context = observation["projection"]["policy_surface_context"]
        stage = context["current_stage"]
        private = context["private_combat_selection"]
        starts_group = request["request_type"] == "reset" or (
            self._substep_index is not None
            and self._substep_count is not None
            and self._substep_index + 1 == self._substep_count
        )
        if starts_group:
            if stage == "surface":
                return None, None, None, None
            if private is None or substep_index != 0 or private["selected"] != []:
                raise ProtocolError(
                    "combat physical decision must begin at candidate 0 with empty selected history"
                )
            candidates = [
                copy.deepcopy(private["current_candidate"]),
                *copy.deepcopy(private["remaining_after_current"]),
            ]
            if len(candidates) != substep_count:
                raise ProtocolError("combat physical decision candidate sequence length mismatch")
            return (
                stage,
                copy.deepcopy(private["attacker"]),
                candidates,
                [],
            )

        if (
            private is None
            or self._group_stage is None
            or self._group_candidates is None
            or self._group_selected is None
            or self._current_legal_actions is None
            or self._substep_index is None
        ):
            raise ProtocolError("missing frozen combat physical-decision transcript")
        action = self._current_legal_actions[request["selected_index"]]
        semantic = action["semantic"]
        include = semantic.get("include")
        if type(include) is not bool:
            raise ProtocolError("combat substep action must carry an exact include boolean")
        expected_selected = copy.deepcopy(self._group_selected)
        if include:
            expected_selected.append(copy.deepcopy(self._group_candidates[self._substep_index]))
        expected_current = self._group_candidates[substep_index]
        expected_remaining = self._group_candidates[substep_index + 1 :]
        if stage != self._group_stage:
            raise ProtocolError("combat physical decision stage drift")
        if private["attacker"] != self._group_attacker:
            raise ProtocolError("combat physical decision fixed attacker drift")
        if private["candidate_count"] != len(self._group_candidates):
            raise ProtocolError("combat physical decision candidate count drift")
        if private["current_candidate"] != expected_current:
            raise ProtocolError("combat physical decision current candidate drift")
        if private["remaining_after_current"] != expected_remaining:
            raise ProtocolError("combat physical decision remaining candidate suffix drift")
        if private["selected"] != expected_selected:
            raise ProtocolError("combat physical decision selected history drift")
        return (
            self._group_stage,
            copy.deepcopy(self._group_attacker),
            copy.deepcopy(self._group_candidates),
            expected_selected,
        )

    def _exchange(self, request: dict[str, Any]) -> Decision | Terminal:
        if self._closed:
            raise EnvProcessError("client is closed")
        if self._proc.poll() is not None:
            raise EnvProcessError(f"environment exited before request: code={self._proc.returncode} stderr={self.stderr_text()!r}")
        assert self._proc.stdin is not None
        self._proc.stdin.write(strict_json_dumps(request))
        self._proc.stdin.flush()
        line = self._read_line()
        response = strict_json_loads(line.decode("utf-8"))
        return self._validate_response(response, request)

    def _read_line(self) -> bytes:
        try:
            item = self._stdout.get(timeout=self.timeout_s)
        except queue.Empty as exc:
            self.close()
            raise EnvProcessError(f"timeout waiting for environment stdout; stderr={self.stderr_text()!r}") from exc
        if isinstance(item, BaseException):
            self.close()
            raise item
        if item == b"":
            code = self._proc.poll()
            raise EnvProcessError(f"environment EOF; code={code} stderr={self.stderr_text()!r}")
        return item.rstrip(b"\r\n")

    def _validate_response(self, response: dict[str, Any], request: dict[str, Any]) -> Decision | Terminal:
        response_type = _str(response.get("response_type"), "response_type")
        if response_type == "error":
            _keys(response, ["response_type", "schema_version", "request_id", "error"], "error response")
            if _int(response["schema_version"], "error.schema_version", minimum=0, maximum=U32) != SCHEMA_VERSION:
                raise ProtocolError("error schema mismatch")
            if _str(response["request_id"], "error.request_id") != request["request_id"]:
                raise ProtocolError("error request_id mismatch")
            error = response["error"]
            if not isinstance(error, dict):
                raise ProtocolError("error payload must be object")
            _keys(error, ["code", "message"], "error")
            code = _nonempty_str(error["code"], "error.code")
            message = _nonempty_str(error["message"], "error.message")
            sanitized = " ".join(message.split())[:240]
            raise ProtocolError(f"environment error {code}: {sanitized}")
        if response_type == "decision":
            _keys(
                response,
                [
                    "response_type",
                    "schema_version",
                    "request_id",
                    "provenance",
                    "deck_ids",
                    "deck_hashes",
                    "episode_id",
                    "step",
                    "physical_decision_id",
                    "substep_index",
                    "substep_count",
                    "acting_player",
                    "observation",
                    "legal_actions",
                    "reward",
                ],
                "decision response",
            )
            if _int(response["schema_version"], "decision.schema_version", minimum=0, maximum=U32) != SCHEMA_VERSION:
                raise ProtocolError("decision schema mismatch")
            if _str(response["request_id"], "decision.request_id") != request["request_id"]:
                raise ProtocolError("decision request_id mismatch")
            provenance = _validate_provenance(response["provenance"], self._provenance)
            self._provenance = provenance
            deck_ids, deck_hashes = _validate_deck_identity(
                response["deck_ids"], response["deck_hashes"], context="decision"
            )
            expected_deck_ids = (
                tuple(request["deck_ids"])
                if request["request_type"] == "reset"
                else self._deck_ids
            )
            if expected_deck_ids is None or deck_ids != expected_deck_ids:
                raise ProtocolError("decision deck_ids mismatch")
            if (
                request["request_type"] != "reset"
                and self._deck_hashes is not None
                and deck_hashes != self._deck_hashes
            ):
                raise ProtocolError("decision deck_hashes drift")
            self._deck_ids = deck_ids
            self._deck_hashes = deck_hashes
            episode_id = _int(response["episode_id"], "decision.episode_id", minimum=0, maximum=U64)
            if episode_id != request["episode_id"]:
                raise ProtocolError("decision episode_id mismatch")
            step = _int(response["step"], "decision.step", minimum=0, maximum=U64)
            expected_step = 0 if request["request_type"] == "reset" else request["expected_step"] + 1
            if step != expected_step:
                raise ProtocolError(f"decision step drift: expected {expected_step}, got {step}")
            acting = _seat(response["acting_player"], "decision.acting_player")
            physical_decision_id = _int(
                response["physical_decision_id"],
                "decision.physical_decision_id",
                minimum=0,
                maximum=U64,
            )
            substep_index = _int(
                response["substep_index"],
                "decision.substep_index",
                minimum=0,
                maximum=U32,
            )
            substep_count = _int(
                response["substep_count"],
                "decision.substep_count",
                minimum=1,
                maximum=U32,
            )
            if substep_index >= substep_count:
                raise ProtocolError("decision substep_index must be < substep_count")
            if request["request_type"] == "reset":
                if (physical_decision_id, substep_index) != (0, 0):
                    raise ProtocolError("initial decision must begin physical decision 0 at substep 0")
            elif self._physical_decision_id is None or self._substep_index is None or self._substep_count is None:
                raise ProtocolError("missing prior physical decision state")
            elif self._substep_index + 1 < self._substep_count:
                if (
                    physical_decision_id != self._physical_decision_id
                    or substep_index != self._substep_index + 1
                    or substep_count != self._substep_count
                    or acting != self._group_actor
                ):
                    raise ProtocolError("physical decision substeps are not contiguous and stable")
            elif (
                physical_decision_id != self._physical_decision_id + 1
                or substep_index != 0
            ):
                raise ProtocolError("physical decision ids must advance exactly once after a completed group")
            observation = _validate_observation(response["observation"], response, provenance)
            legal_actions = _validate_legal_actions(response["legal_actions"])
            from .features import FeatureSchemaError, validate_decision_contract

            try:
                validate_decision_contract(observation, legal_actions)
            except FeatureSchemaError as exc:
                raise ProtocolError(str(exc)) from exc
            group_stage, group_attacker, group_candidates, group_selected = (
                self._next_group_transcript(
                    observation=observation,
                    request=request,
                    substep_index=substep_index,
                    substep_count=substep_count,
                )
            )
            if _validate_reward(response["reward"], "decision.reward") != [0, 0]:
                raise ProtocolError("intermediate decision reward must be [0, 0]")
            self._episode_id = episode_id
            self._expected_step = step
            self._physical_decision_id = physical_decision_id
            self._substep_index = substep_index
            self._substep_count = substep_count
            self._group_actor = acting
            self._group_stage = group_stage
            self._group_attacker = group_attacker
            self._group_candidates = group_candidates
            self._group_selected = group_selected
            self._current_legal_actions = copy.deepcopy(legal_actions)
            return Decision(
                episode_id,
                step,
                physical_decision_id,
                substep_index,
                substep_count,
                acting,
                observation,
                legal_actions,
                provenance,
                deck_ids,
                deck_hashes,
            )
        if response_type == "terminal":
            _keys(
                response,
                ["response_type", "schema_version", "request_id", "provenance", "deck_ids", "deck_hashes", "episode_id", "terminal_outcome", "terminal_classification", "terminal_code", "winner", "terminal_reward", "terminal_reason", "policy_step_count", "physical_decision_count"],
                "terminal response",
            )
            if _int(response["schema_version"], "terminal.schema_version", minimum=0, maximum=U32) != SCHEMA_VERSION:
                raise ProtocolError("terminal schema mismatch")
            if _str(response["request_id"], "terminal.request_id") != request["request_id"]:
                raise ProtocolError("terminal request_id mismatch")
            provenance = _validate_provenance(response["provenance"], self._provenance)
            self._provenance = provenance
            deck_ids, deck_hashes = _validate_deck_identity(
                response["deck_ids"], response["deck_hashes"], context="terminal"
            )
            expected_deck_ids = (
                tuple(request["deck_ids"])
                if request["request_type"] == "reset"
                else self._deck_ids
            )
            if expected_deck_ids is None or deck_ids != expected_deck_ids:
                raise ProtocolError("terminal deck_ids mismatch")
            if (
                request["request_type"] != "reset"
                and self._deck_hashes is not None
                and deck_hashes != self._deck_hashes
            ):
                raise ProtocolError("terminal deck_hashes drift")
            self._deck_ids = deck_ids
            self._deck_hashes = deck_hashes
            episode_id = _int(response["episode_id"], "terminal.episode_id", minimum=0, maximum=U64)
            if episode_id != request["episode_id"]:
                raise ProtocolError("terminal episode_id mismatch")
            policy_step_count = _int(response["policy_step_count"], "terminal.policy_step_count", minimum=0, maximum=U64)
            expected_policy_count = 0 if request["request_type"] == "reset" else request["expected_step"] + 1
            if policy_step_count != expected_policy_count:
                raise ProtocolError(
                    f"terminal policy_step_count mismatch: expected {expected_policy_count}, got {policy_step_count}"
                )
            physical_decision_count = _int(
                response["physical_decision_count"],
                "terminal.physical_decision_count",
                minimum=0,
                maximum=U64,
            )
            if request["request_type"] == "reset":
                expected_physical_count = 0
            else:
                if (
                    self._physical_decision_id is None
                    or self._substep_index is None
                    or self._substep_count is None
                ):
                    raise ProtocolError("missing prior physical decision state at terminal")
                if self._substep_index + 1 != self._substep_count:
                    raise ProtocolError("terminal response interrupted an incomplete physical decision")
                expected_physical_count = self._physical_decision_id + 1
            if physical_decision_count != expected_physical_count:
                raise ProtocolError(
                    "terminal physical_decision_count does not match completed groups"
                )
            outcome = _str(response["terminal_outcome"], "terminal.terminal_outcome")
            classification = _str(response["terminal_classification"], "terminal.terminal_classification")
            code = _str(response["terminal_code"], "terminal.terminal_code")
            reward = _validate_reward(response["terminal_reward"], "terminal.terminal_reward")
            winner = _optional_seat(response["winner"], "terminal.winner")
            _str(response["terminal_reason"], "terminal.terminal_reason")
            self._validate_terminal_tuple(outcome, classification, code, winner, reward)
            self._expected_step = None
            self._physical_decision_id = None
            self._substep_index = None
            self._substep_count = None
            self._group_actor = None
            self._group_stage = None
            self._group_attacker = None
            self._group_candidates = None
            self._group_selected = None
            self._current_legal_actions = None
            return Terminal(
                episode_id,
                outcome,
                classification,
                code,
                winner,
                reward,
                policy_step_count,
                physical_decision_count,
                provenance,
                deck_ids,
                deck_hashes,
            )
        raise ProtocolError(f"unsupported response_type {response_type}")

    @staticmethod
    def _validate_terminal_tuple(outcome: str, classification: str, code: str, winner: str | None, reward: list[int]) -> None:
        if classification != "natural" or code != "natural_game_over":
            raise ProtocolError("only natural terminal outcomes are admitted")
        expected: dict[str, tuple[str | None, list[int]]] = {
            "p0_win": ("p0", [1, -1]),
            "p1_win": ("p1", [-1, 1]),
            "draw": (None, [0, 0]),
        }
        if outcome not in expected:
            raise ProtocolError("terminal outcome is not a natural win/draw")
        expected_winner, expected_reward = expected[outcome]
        if winner != expected_winner or reward != expected_reward:
            raise ProtocolError("invalid natural terminal winner/reward tuple")
