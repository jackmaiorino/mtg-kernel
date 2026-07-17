"""Non-authoritative opt-in phase timing and strict Rust profile collection."""

from __future__ import annotations

import contextlib
import json
import time
from dataclasses import dataclass
from typing import Any, Iterator

KERNEL_PROFILE_PREFIX = "MTG_KERNEL_PROFILE_V1\t"
KERNEL_PROFILE_SCHEMA = "kernel_rl_phase_profile/v1"
KERNEL_PROFILE_CLOCK = "std_instant_monotonic_ns/v1"
KERNEL_PHASES = (
    "parse",
    "decode",
    "retry",
    "reset",
    "step_validation",
    "step_integrity",
    "step_selection",
    "step_apply",
    "advance",
    "observe",
    "actions",
    "postbind",
    "response",
    "serialize",
    "write_flush",
)
PYTHON_PHASES = (
    "ipc_encode",
    "ipc_write_flush",
    "ipc_wait_read",
    "ipc_decode",
    "ipc_validate",
    "feature_tensor",
    "model_forward",
    "action_sample",
    "trajectory_hash",
    "loss_build",
    "backward",
    "optimizer",
    "checkpoint_build",
    "publication",
)


def _strict_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def _exact_keys(value: Any, expected: set[str], context: str) -> dict[str, Any]:
    if type(value) is not dict:
        raise ValueError(f"{context} must be an object")
    if set(value) != expected:
        raise ValueError(f"{context} keys mismatch")
    return value


def _u64(value: Any, context: str) -> int:
    if type(value) is not int or not 0 <= value <= (1 << 64) - 1:
        raise ValueError(f"{context} must be an unsigned 64-bit integer")
    return value


def parse_kernel_profile_stderr(stderr: bytes | str) -> dict[str, Any]:
    try:
        text = stderr.decode("utf-8", errors="strict") if isinstance(stderr, bytes) else stderr
    except UnicodeDecodeError as exc:
        raise ValueError("kernel profile stderr is not UTF-8") from exc
    if type(text) is not str:
        raise TypeError("stderr must be exact bytes or str")
    nonempty = [line for line in text.splitlines() if line]
    malformed_prefix = [
        line
        for line in nonempty
        if line.startswith("MTG_KERNEL_PROFILE_V1")
        and not line.startswith(KERNEL_PROFILE_PREFIX)
    ]
    if malformed_prefix:
        raise ValueError("malformed kernel phase profile prefix")
    records = [line[len(KERNEL_PROFILE_PREFIX) :] for line in nonempty if line.startswith(KERNEL_PROFILE_PREFIX)]
    if len(records) != 1:
        raise ValueError("kernel phase profile must contain exactly one record")
    if len(nonempty) != 1:
        raise ValueError("unexpected kernel stderr alongside phase profile")
    try:
        value = json.loads(
            records[0],
            object_pairs_hook=_strict_object,
            parse_constant=lambda token: (_ for _ in ()).throw(
                ValueError(f"non-finite JSON token: {token}")
            ),
        )
    except (json.JSONDecodeError, ValueError) as exc:
        raise ValueError("malformed kernel phase profile JSON") from exc
    root = _exact_keys(
        value,
        {
            "schema",
            "clock",
            "request_lines",
            "response_lines",
            "reset_requests",
            "step_requests",
            "phases",
        },
        "kernel phase profile",
    )
    if root["schema"] != KERNEL_PROFILE_SCHEMA or root["clock"] != KERNEL_PROFILE_CLOCK:
        raise ValueError("kernel phase profile schema or clock mismatch")
    request_lines = _u64(root["request_lines"], "request_lines")
    response_lines = _u64(root["response_lines"], "response_lines")
    reset_requests = _u64(root["reset_requests"], "reset_requests")
    step_requests = _u64(root["step_requests"], "step_requests")
    phases = _exact_keys(root["phases"], set(KERNEL_PHASES), "kernel phases")
    for phase in KERNEL_PHASES:
        counter = _exact_keys(
            phases[phase], {"count", "total_ns", "max_ns"}, f"kernel phase {phase}"
        )
        count = _u64(counter["count"], f"{phase}.count")
        total_ns = _u64(counter["total_ns"], f"{phase}.total_ns")
        max_ns = _u64(counter["max_ns"], f"{phase}.max_ns")
        if max_ns > total_ns:
            raise ValueError(f"{phase}.max_ns exceeds total_ns")
        if count == 0 and (total_ns != 0 or max_ns != 0):
            raise ValueError(f"empty {phase} counter has timing")
    counts = {phase: phases[phase]["count"] for phase in KERNEL_PHASES}
    if not (
        request_lines
        == response_lines
        == counts["parse"]
        == counts["serialize"]
        == counts["write_flush"]
    ):
        raise ValueError("kernel request/response outer count invariant failed")
    decoded_requests = reset_requests + step_requests
    if decoded_requests != counts["retry"] or decoded_requests != counts["response"]:
        raise ValueError("kernel decoded request count invariant failed")
    if not counts["decode"] <= counts["parse"]:
        raise ValueError("kernel decode count exceeds parse count")
    if decoded_requests > counts["decode"]:
        raise ValueError("kernel typed request count exceeds decode count")
    if counts["reset"] > reset_requests:
        raise ValueError("kernel reset execution count exceeds reset requests")
    for phase in ("step_validation", "step_integrity", "step_selection", "step_apply"):
        if counts[phase] > step_requests:
            raise ValueError(f"kernel {phase} count exceeds step requests")
    if not (
        counts["step_apply"]
        <= counts["step_selection"]
        <= counts["step_integrity"]
        <= counts["step_validation"]
        <= step_requests
    ):
        raise ValueError("kernel step phase count invariant failed")
    if not counts["postbind"] <= counts["actions"] <= counts["observe"] <= counts["advance"]:
        raise ValueError("kernel decision construction count invariant failed")
    if counts["advance"] > counts["reset"] + counts["step_apply"]:
        raise ValueError("kernel advance count exceeds reset plus applied steps")
    return root


@dataclass
class _Counter:
    count: int = 0
    total_ns: int = 0
    max_ns: int = 0

    def add(self, elapsed_ns: int) -> None:
        self.count += 1
        self.total_ns += elapsed_ns
        self.max_ns = max(self.max_ns, elapsed_ns)

    def payload(self) -> dict[str, int]:
        return {"count": self.count, "total_ns": self.total_ns, "max_ns": self.max_ns}


class PhaseRecorder:
    """External-only recorder; never serialize this into a training store."""

    def __init__(self) -> None:
        self._python = {phase: _Counter() for phase in PYTHON_PHASES}
        self._kernel_records: list[dict[str, Any]] = []

    @contextlib.contextmanager
    def measure(self, phase: str) -> Iterator[None]:
        if phase not in self._python:
            raise ValueError(f"unknown Python phase: {phase}")
        start = time.perf_counter_ns()
        try:
            yield
        finally:
            self._python[phase].add(time.perf_counter_ns() - start)

    def add_kernel_stderr(self, stderr: bytes | str) -> None:
        self._kernel_records.append(parse_kernel_profile_stderr(stderr))

    def snapshot(self) -> dict[str, Any]:
        return {
            "schema": "kernel_rl_training_phase_profile/v1",
            "clock": "python_perf_counter_ns/v1",
            "python_phases": {
                phase: self._python[phase].payload() for phase in PYTHON_PHASES
            },
            "kernel_records": list(self._kernel_records),
        }


@contextlib.contextmanager
def measure_optional(recorder: PhaseRecorder | None, phase: str) -> Iterator[None]:
    if recorder is None:
        yield
    else:
        with recorder.measure(phase):
            yield
