#!/usr/bin/env python3
"""Generate and verify spellbench/v1 environment-role golden transcripts.

This tool is intentionally stdlib-only.  It drives the real built
``agent_bridge_v1`` binary over stdio through four self-checking scenarios
(hello handshake, framing/decode negatives, degenerate-game terminal edges,
and a capped Burn mirror with retry/reuse probes), validates every response
against the protocol's closed shapes (including an independent
``candidates_sha256`` recomputation), and writes byte-exact NDJSON
transcripts plus a schema-tagged manifest under ``data/agent_bridge_v1/``.

The child process environment pins ``SPELLBENCH_SOURCE_REVISION`` to a fixed
sentinel so the hello/provenance bytes are machine-independent.

Usage:
  python generate_agent_bridge_goldens.py --write
  python generate_agent_bridge_goldens.py --check
"""

from __future__ import annotations

import sys

# Set before any project import so the focused generator leaves no
# __pycache__ artifact behind.
sys.dont_write_bytecode = True

import argparse  # noqa: E402
import hashlib  # noqa: E402
import json  # noqa: E402
import math  # noqa: E402
import os  # noqa: E402
import subprocess  # noqa: E402
import tempfile  # noqa: E402
from dataclasses import dataclass  # noqa: E402
from pathlib import Path  # noqa: E402
from typing import Any, Callable, Iterable  # noqa: E402

SCHEMA = "kernel_agent_bridge_goldens/v1"
GENERATOR_IDENTITY = "mtg-kernel-agent-bridge-goldens-stdlib-python-v1"
PROTOCOL = "spellbench/v1"
FORMAT = "pauper-bo1"
ENV_BIN_VAR = "MTG_KERNEL_AGENT_BRIDGE_V1_BIN"
SOURCE_REVISION_ENV = "SPELLBENCH_SOURCE_REVISION"
SOURCE_REVISION_SENTINEL = "spellbench-golden-v1"

OUTPUT_DIR = Path("data/agent_bridge_v1")
MANIFEST_PATH = OUTPUT_DIR / "manifest.json"

MAX_SAFE_INTEGER = 1 << 53  # spec Section 2: every protocol integer has |x| <= 2^53

RESPONSE_TYPES = frozenset({"hello_ok", "decision", "terminal", "error"})
ERROR_CODES = frozenset(
    {
        "malformed_json",
        "malformed_request",
        "protocol_mismatch",
        "request_id_reuse_mismatch",
        "step_before_reset",
        "game_already_active",
        "game_id_mismatch",
        "expected_step_mismatch",
        "candidate_id_out_of_range",
        "semantic_echo_mismatch",
        "unsupported_format",
        "unsupported_deck",
        "game_already_terminal",
    }
)
SEMANTIC_KINDS = frozenset(
    {
        "pass",
        "play_land",
        "cast_spell",
        "activate_mana_ability",
        "activate_ability",
        "plot_spell",
        "choose_target",
        "choose_cost_target",
        "choose_cast_mode",
        "choose_kicker",
        "choose_spell_mode",
        "choose_option",
        "choose_effect_target",
        "finish_effect_selection",
        "choose_color",
        "choose_number",
        "choose_boolean",
        "finish_target_selection",
        "choose_optional_cost_use",
        "choose_optional_cost_which",
        "choose_spell_copy_payment",
        "choose_spell_copy_retarget",
        "choose_madness_cast",
        "discard",
        "choose_attacker_inclusion",
        "choose_blocker_inclusion",
        "order_triggers",
    }
)
OUTCOMES = frozenset({"p0_win", "p1_win", "draw", "truncated", "halted"})
CLASSIFICATIONS = frozenset({"natural", "truncated", "halted"})
PHASE_STEPS = frozenset(
    {
        "untap",
        "upkeep",
        "draw",
        "precombat_main",
        "beginning_of_combat",
        "declare_attackers",
        "declare_blockers",
        "combat_damage",
        "end_of_combat",
        "postcombat_main",
        "end_step",
        "cleanup",
    }
)


class GoldenError(RuntimeError):
    """Raised when a response or a stored artifact fails closed."""


@dataclass(frozen=True)
class Scenario:
    name: str
    description: str


def repo_root_from_script() -> Path:
    return Path(__file__).resolve().parents[2]


def sha256_hex(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def canonical_json(value: Any) -> str:
    """Spec Section 4.3 canonical JSON: sorted keys, compact separators,
    integers only, strings byte-exact (UTF-8, no ASCII folding)."""

    def walk(node: Any, path: str) -> None:
        if isinstance(node, bool):
            return
        if isinstance(node, float):
            raise GoldenError(f"canonical JSON rejects the float at {path}")
        if isinstance(node, dict):
            for key, child in node.items():
                walk(child, f"{path}.{key}")
        elif isinstance(node, list):
            for index, child in enumerate(node):
                walk(child, f"{path}[{index}]")

    walk(value, "$")
    try:
        return json.dumps(
            value,
            sort_keys=True,
            separators=(",", ":"),
            ensure_ascii=False,
            allow_nan=False,
        )
    except (TypeError, ValueError) as exc:
        raise GoldenError(f"cannot serialize canonical JSON: {exc}") from exc


def candidates_sha256(candidates: list[dict[str, Any]]) -> str:
    reduced = [
        {"candidate_id": candidate["candidate_id"], "semantic": candidate["semantic"]}
        for candidate in candidates
    ]
    return sha256_hex(canonical_json(reduced).encode("utf-8"))


def _reject_duplicate_keys(pairs: Iterable[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise GoldenError(f"duplicate JSON object key: {key!r}")
        result[key] = value
    return result


def loads_json_strict(raw: str, *, context: str) -> Any:
    def reject_constant(value: str) -> None:
        raise GoldenError(f"{context}: non-finite JSON constant {value!r}")

    def parse_finite_float(value: str) -> float:
        parsed = float(value)
        if not math.isfinite(parsed):
            raise GoldenError(f"{context}: non-finite JSON number {value!r}")
        raise GoldenError(f"{context}: fractional JSON number {value!r}")

    def parse_safe_int(value: str) -> int:
        parsed = int(value)
        if abs(parsed) > MAX_SAFE_INTEGER:
            raise GoldenError(f"{context}: integer outside |x| <= 2^53: {value}")
        return parsed

    try:
        return json.loads(
            raw,
            object_pairs_hook=_reject_duplicate_keys,
            parse_constant=reject_constant,
            parse_float=parse_finite_float,
            parse_int=parse_safe_int,
        )
    except json.JSONDecodeError as exc:
        raise GoldenError(f"{context}: invalid JSON: {exc}") from exc


def request_line(payload: dict[str, Any]) -> str:
    """Stable request bytes: canonical key order, compact separators."""
    return canonical_json(payload)


def hello_request(request_id: str) -> str:
    return request_line(
        {"request_type": "hello", "protocol": PROTOCOL, "request_id": request_id}
    )


def reset_request(
    request_id: str,
    game_id: str,
    game_seed: int,
    max_decisions: int,
    max_steps: int,
) -> str:
    return request_line(
        {
            "request_type": "reset",
            "protocol": PROTOCOL,
            "request_id": request_id,
            "game_id": game_id,
            "format": FORMAT,
            "seats": [
                {"seat": "p0", "deck": {"catalog_id": "Burn"}},
                {"seat": "p1", "deck": {"catalog_id": "Burn"}},
            ],
            "game_seed": game_seed,
            "max_decisions": max_decisions,
            "max_steps": max_steps,
        }
    )


def step_request(
    request_id: str, decision: dict[str, Any], candidate_index: int
) -> str:
    candidate = decision["candidates"][candidate_index]
    return request_line(
        {
            "request_type": "step",
            "protocol": PROTOCOL,
            "request_id": request_id,
            "game_id": decision["game_id"],
            "expected_step": decision["step"],
            "selection": {
                "candidate_id": candidate["candidate_id"],
                "semantic_echo": candidate["semantic"],
            },
        }
    )


def validate_response(line: str, request_id: str | None) -> dict[str, Any]:
    """Fail-closed structural conformance check for one response line."""
    value = loads_json_strict(line, context="response")
    if not isinstance(value, dict):
        raise GoldenError("response is not a JSON object")
    if value.get("protocol") != PROTOCOL:
        raise GoldenError(f"response protocol drift: {value.get('protocol')!r}")
    if value.get("request_id") != request_id and value.get("response_type") != "error":
        raise GoldenError(
            f"response request_id drift: {value.get('request_id')!r} != {request_id!r}"
        )
    response_type = value.get("response_type")
    if response_type not in RESPONSE_TYPES:
        raise GoldenError(f"unknown response_type {response_type!r}")
    if response_type == "error":
        error = value.get("error")
        if not isinstance(error, dict) or set(error.keys()) != {"code", "message"}:
            raise GoldenError(f"error response shape drift: {error!r}")
        if error["code"] not in ERROR_CODES:
            raise GoldenError(f"error code {error['code']!r} is outside the closed table")
        # Spec Section 4.1: the echoed id, or "" when the request's id is unreadable.
        expected_echo = "" if request_id is None else request_id
        if value.get("request_id") != expected_echo:
            raise GoldenError(
                f"error request_id drift: {value.get('request_id')!r} != {expected_echo!r}"
            )
        return value
    if response_type == "hello_ok":
        engine = value.get("engine")
        if not isinstance(engine, dict) or engine.get("name") != "mtg-kernel":
            raise GoldenError(f"hello_ok engine identity drift: {engine!r}")
        if engine.get("source_revision") != SOURCE_REVISION_SENTINEL:
            raise GoldenError("hello_ok source_revision is not the golden sentinel")
        if value.get("formats") != [FORMAT]:
            raise GoldenError(f"formats drift: {value.get('formats')!r}")
        if value.get("capabilities") != {"decklists_as_data": False}:
            raise GoldenError("capabilities drift")
        if value.get("extensions") != ["x_kernel_v5"]:
            raise GoldenError("extensions drift")
        return value
    provenance = value.get("provenance")
    if not isinstance(provenance, dict) or provenance.get("engine_name") != "mtg-kernel":
        raise GoldenError(f"provenance drift: {provenance!r}")
    if response_type == "terminal":
        if value.get("outcome") not in OUTCOMES:
            raise GoldenError(f"outcome drift: {value.get('outcome')!r}")
        if value.get("classification") not in CLASSIFICATIONS:
            raise GoldenError(f"classification drift: {value.get('classification')!r}")
        if not isinstance(value.get("step_count"), int):
            raise GoldenError("terminal step_count is not an integer")
        if not isinstance(value.get("decision_count"), int):
            raise GoldenError("terminal decision_count is not an integer")
        return value
    # decision
    candidates = value.get("candidates")
    if not isinstance(candidates, list) or not candidates:
        raise GoldenError("decision candidates must be a nonempty array")
    for index, candidate in enumerate(candidates):
        if set(candidate.keys()) != {"candidate_id", "semantic", "display_text"}:
            raise GoldenError(f"candidate {index} shape drift: {sorted(candidate.keys())!r}")
        if candidate["candidate_id"] != index:
            raise GoldenError(f"candidate {index} id is not dense")
        semantic = candidate["semantic"]
        if not isinstance(semantic, dict) or semantic.get("kind") not in SEMANTIC_KINDS:
            raise GoldenError(f"candidate {index} semantic kind drift: {semantic!r}")
        if semantic.get("kind") == "ambiguous":
            raise GoldenError("the refused ambiguous kind was emitted")
        if not (candidate["display_text"] is None or isinstance(candidate["display_text"], str)):
            raise GoldenError(f"candidate {index} display_text drift")
    if value.get("candidates_sha256") != candidates_sha256(candidates):
        raise GoldenError("candidates_sha256 does not match the semantic content")
    group = value.get("group")
    if not isinstance(group, dict):
        raise GoldenError("decision group is missing")
    if not (0 <= group.get("substep_index", -1) < max(group.get("substep_count", 0), 1)):
        raise GoldenError(f"group substep range drift: {group!r}")
    summary = value.get("state_summary")
    if not isinstance(summary, dict):
        raise GoldenError("state_summary is missing")
    if summary.get("phase_step") not in PHASE_STEPS:
        raise GoldenError(f"phase_step drift: {summary.get('phase_step')!r}")
    extensions = value.get("extensions")
    if not isinstance(extensions, dict) or "x_kernel_v5" not in extensions:
        raise GoldenError("extensions.x_kernel_v5 is missing")
    if any(not key.startswith("x_") for key in extensions):
        raise GoldenError("extension key outside the x_ namespace")
    kernel_v5 = extensions["x_kernel_v5"]
    if not isinstance(kernel_v5, dict) or set(kernel_v5) != {"observation_json", "legal_actions_json"}:
        raise GoldenError("x_kernel_v5 must carry exactly observation_json and legal_actions_json")
    if not all(isinstance(text, str) for text in kernel_v5.values()):
        raise GoldenError("x_kernel_v5 payloads must be JSON text")
    # The raw kernel payloads keep their full u64 range inside the JSON text.
    if not isinstance(json.loads(kernel_v5["observation_json"]), dict):
        raise GoldenError("x_kernel_v5 observation_json is not a JSON object")
    legal_actions = json.loads(kernel_v5["legal_actions_json"])
    if not isinstance(legal_actions, list) or len(legal_actions) != len(candidates):
        raise GoldenError("x_kernel_v5 legal_actions_json does not match the candidates")
    return value


class BridgeSession:
    """One bridge child process with an accumulating transcript."""

    def __init__(self, binary: Path) -> None:
        env = dict(os.environ)
        env[SOURCE_REVISION_ENV] = SOURCE_REVISION_SENTINEL
        self._proc = subprocess.Popen(
            [str(binary)],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            env=env,
        )
        assert self._proc.stdin is not None
        assert self._proc.stdout is not None
        self.transcript: list[tuple[str, str]] = []

    def exchange(self, line: str) -> dict[str, Any]:
        assert self._proc.stdin is not None and self._proc.stdout is not None
        self.transcript.append(("host_to_engine", line))
        self._proc.stdin.write(line.encode("utf-8") + b"\n")
        self._proc.stdin.flush()
        raw = self._proc.stdout.readline()
        if not raw:
            stderr = b""
            if self._proc.stderr is not None:
                stderr = self._proc.stderr.read()
            raise GoldenError(
                f"bridge closed stdout unexpectedly; stderr: {stderr.decode('utf-8', 'replace')}"
            )
        if not raw.endswith(b"\n"):
            raise GoldenError("bridge response line is not LF-terminated")
        try:
            text = raw[:-1].decode("utf-8", errors="strict")
        except UnicodeDecodeError as exc:
            raise GoldenError(f"bridge response is not UTF-8: {exc}") from exc
        request_id: str | None = None
        try:
            parsed_request = loads_json_strict(line, context="request")
            if isinstance(parsed_request, dict):
                candidate_id = parsed_request.get("request_id")
                if isinstance(candidate_id, str):
                    request_id = candidate_id
        except GoldenError:
            request_id = None
        response = validate_response(text, request_id)
        self.transcript.append(("engine_to_host", text))
        return response

    def close(self) -> None:
        assert self._proc.stdin is not None
        self._proc.stdin.close()
        returncode = self._proc.wait()
        if returncode != 0:
            stderr = b""
            if self._proc.stderr is not None:
                stderr = self._proc.stderr.read()
            raise GoldenError(
                f"bridge exited {returncode}; stderr: {stderr.decode('utf-8', 'replace')}"
            )

    def transcript_bytes(self) -> bytes:
        records = [
            canonical_json({"dir": direction, "line": line})
            for direction, line in self.transcript
        ]
        return ("\n".join(records) + "\n").encode("utf-8")


def expect_error(response: dict[str, Any], code: str) -> None:
    if response.get("response_type") != "error" or response["error"]["code"] != code:
        raise GoldenError(f"expected error {code}, got {response.get('response_type')}")


def expect_decision(response: dict[str, Any]) -> dict[str, Any]:
    if response.get("response_type") != "decision":
        raise GoldenError(f"expected decision, got {response.get('response_type')}")
    return response


def scenario_hello(session: BridgeSession) -> None:
    response = session.exchange(hello_request("h-1"))
    if response.get("response_type") != "hello_ok":
        raise GoldenError("hello did not return hello_ok")


def scenario_framing_errors(session: BridgeSession) -> None:
    expect_error(session.exchange("this is not json"), "malformed_json")
    expect_error(
        session.exchange('{"request_type":"hello","request_type":"hello"}'),
        "malformed_json",
    )
    # Fractional numbers are rejected as malformed_json before schema decode.
    expect_error(
        session.exchange(
            '{"expected_step":1.5,"game_id":"g-none","protocol":"spellbench/v1",'
            '"request_id":"frac","request_type":"step",'
            '"selection":{"candidate_id":0,"semantic_echo":{"kind":"pass"}}}'
        ),
        "malformed_json",
    )
    expect_error(session.exchange("[1,2,3]"), "malformed_request")
    expect_error(
        session.exchange(
            request_line(
                {
                    "request_type": "hello",
                    "protocol": "spellbench/v0",
                    "request_id": "p-1",
                }
            )
        ),
        "protocol_mismatch",
    )
    expect_error(
        session.exchange(
            request_line(
                {
                    "request_type": "hello",
                    "protocol": PROTOCOL,
                    "request_id": "m-1",
                    "surprise": True,
                }
            )
        ),
        "malformed_request",
    )
    expect_error(
        session.exchange(
            request_line(
                {
                    "request_type": "teleport",
                    "protocol": PROTOCOL,
                    "request_id": "m-2",
                }
            )
        ),
        "malformed_request",
    )
    expect_error(
        session.exchange(
            request_line(
                {
                    "request_type": "step",
                    "protocol": PROTOCOL,
                    "request_id": "s-early",
                    "game_id": "g-none",
                    "expected_step": 0,
                    "selection": {"candidate_id": 0, "semantic_echo": {"kind": "pass"}},
                }
            )
        ),
        "step_before_reset",
    )
    # The process stays usable after every failure.
    if session.exchange(hello_request("h-after"))["response_type"] != "hello_ok":
        raise GoldenError("bridge did not recover after framing errors")


def scenario_degenerate_game(session: BridgeSession) -> None:
    terminal = session.exchange(reset_request("r-1", "g-degenerate", 3, 0, 128))
    if terminal.get("response_type") != "terminal":
        raise GoldenError("zero-cap reset did not return a terminal")
    if terminal["outcome"] != "truncated" or terminal["classification"] != "truncated":
        raise GoldenError("degenerate game must truncate")
    expect_error(
        session.exchange(
            request_line(
                {
                    "request_type": "step",
                    "protocol": PROTOCOL,
                    "request_id": "s-late",
                    "game_id": "g-degenerate",
                    "expected_step": 0,
                    "selection": {"candidate_id": 0, "semantic_echo": {"kind": "pass"}},
                }
            )
        ),
        "game_already_terminal",
    )
    # A finished game frees the process; game ids stay unique per process.
    expect_error(
        session.exchange(reset_request("r-2", "g-degenerate", 3, 0, 128)),
        "malformed_request",
    )
    expect_decision(session.exchange(reset_request("r-3", "g-next", 3, 16, 2048)))
    expect_error(
        session.exchange(reset_request("r-4", "g-overlap", 3, 16, 2048)),
        "game_already_active",
    )


def scenario_burn_mirror_capped(session: BridgeSession) -> None:
    if session.exchange(hello_request("h-1"))["response_type"] != "hello_ok":
        raise GoldenError("hello did not return hello_ok")
    current = expect_decision(session.exchange(reset_request("r-1", "g-capped", 97531, 24, 3072)))
    if current["step"] != 0:
        raise GoldenError("reset decision must be step 0")
    steps = 0
    retried = False
    while current["response_type"] == "decision":
        line = step_request(f"s-{steps}", current, 0)
        following = session.exchange(line)
        if not retried and steps == 2:
            # Byte-identical retransmission returns the cached response.
            replayed_raw = session.exchange(line)
            if replayed_raw != following:
                raise GoldenError("retry replay diverged from the cached response")
            # Same request id, different payload fails closed.
            reused = loads_json_strict(line, context="retry-probe")
            reused["expected_step"] = current["step"] + 1
            expect_error(
                session.exchange(request_line(reused)), "request_id_reuse_mismatch"
            )
            retried = True
        current = following
        steps += 1
        if steps > 3072:
            raise GoldenError("capped game exceeded its step cap")
    if current["outcome"] != "truncated" or current["classification"] != "truncated":
        raise GoldenError("capped Burn mirror must truncate")
    if current["decision_count"] != 24:
        raise GoldenError("capped Burn mirror must stop at the decision cap")
    if current["step_count"] != steps:
        raise GoldenError("terminal step_count must equal answered steps")
    if not retried:
        raise GoldenError("game ended before the retry probe ran")


SCENARIOS: tuple[tuple[str, str, Callable[[BridgeSession], None]], ...] = (
    ("hello", "hello handshake and engine identity", scenario_hello),
    (
        "framing_errors",
        "malformed JSON, malformed requests, and protocol mismatch without a game",
        scenario_framing_errors,
    ),
    (
        "degenerate_game",
        "zero-cap reset terminal, game_already_terminal, game id reuse, next game, game_already_active",
        scenario_degenerate_game,
    ),
    (
        "burn_mirror_capped",
        "capped Burn mirror driven by candidate 0 with retry and reuse probes",
        scenario_burn_mirror_capped,
    ),
)


def resolve_binary(repo_root: Path) -> Path:
    override = os.environ.get(ENV_BIN_VAR)
    if override:
        path = Path(override)
    else:
        suffix = ".exe" if os.name == "nt" else ""
        path = repo_root / "target" / "release" / f"agent_bridge_v1{suffix}"
    if not path.is_file():
        raise GoldenError(
            f"agent_bridge_v1 binary not found at {path}; "
            f"build it with `cargo build --release --locked --bin agent_bridge_v1` "
            f"or set {ENV_BIN_VAR}"
        )
    return path


def run_scenarios(repo_root: Path) -> dict[str, bytes]:
    binary = resolve_binary(repo_root)
    outputs: dict[str, bytes] = {}
    for name, _description, driver in SCENARIOS:
        session = BridgeSession(binary)
        try:
            driver(session)
        finally:
            session.close()
        if not session.transcript:
            raise GoldenError(f"scenario {name} produced an empty transcript")
        outputs[f"{name}.transcript.jsonl"] = session.transcript_bytes()
    return outputs


def manifest_bytes(transcripts: dict[str, bytes]) -> bytes:
    scenarios = [
        {
            "name": name,
            "description": description,
            "file": f"{name}.transcript.jsonl",
            "sha256": sha256_hex(transcripts[f"{name}.transcript.jsonl"]),
            "line_count": transcripts[f"{name}.transcript.jsonl"].count(b"\n"),
        }
        for name, description, _driver in SCENARIOS
    ]
    manifest = {
        "schema": SCHEMA,
        "protocol": PROTOCOL,
        "generator": GENERATOR_IDENTITY,
        "engine_binary": {
            "env_var": ENV_BIN_VAR,
            "default_path": "target/release/agent_bridge_v1",
        },
        "environment": {SOURCE_REVISION_ENV: SOURCE_REVISION_SENTINEL},
        "scenarios": scenarios,
    }
    return (json.dumps(manifest, ensure_ascii=False, indent=2) + "\n").encode("utf-8")


def expected_outputs(repo_root: Path) -> dict[Path, bytes]:
    transcripts = run_scenarios(repo_root)
    outputs = {
        OUTPUT_DIR / name: content for name, content in transcripts.items()
    }
    outputs[MANIFEST_PATH] = manifest_bytes(transcripts)
    return outputs


def check_outputs(repo_root: Path) -> None:
    for relative_path, expected in expected_outputs(repo_root).items():
        path = repo_root / relative_path
        try:
            actual = path.read_bytes()
        except OSError as exc:
            raise GoldenError(f"cannot read golden file {path}: {exc}") from exc
        if relative_path.name.endswith(".jsonl"):
            for line_number, line in enumerate(actual.decode("utf-8").splitlines(), 1):
                record = loads_json_strict(
                    line, context=f"{relative_path.as_posix()}:{line_number}"
                )
                if record.get("dir") not in {"host_to_engine", "engine_to_host"}:
                    raise GoldenError(
                        f"{relative_path.as_posix()}:{line_number} has invalid dir"
                    )
        if actual != expected:
            raise GoldenError(
                f"{relative_path.as_posix()} is stale; "
                "run generate_agent_bridge_goldens.py --write"
            )


def _atomic_write(path: Path, data: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, temporary_name = tempfile.mkstemp(
        prefix=f".{path.name}.", suffix=".tmp", dir=path.parent
    )
    temporary = Path(temporary_name)
    try:
        with os.fdopen(fd, "wb") as handle:
            handle.write(data)
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temporary, path)
    finally:
        try:
            temporary.unlink()
        except FileNotFoundError:
            pass


def write_outputs(repo_root: Path) -> None:
    outputs = expected_outputs(repo_root)
    for relative_path, expected in outputs.items():
        path = repo_root / relative_path
        if path.exists() and path.read_bytes() == expected:
            continue
        _atomic_write(path, expected)
    check_outputs(repo_root)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--write", action="store_true", help="rewrite golden transcripts canonically")
    mode.add_argument("--check", action="store_true", help="fail unless golden transcripts are current")
    parser.add_argument(
        "--repo-root",
        type=Path,
        default=repo_root_from_script(),
        help="repository root (default: inferred from this script)",
    )
    args = parser.parse_args(argv)
    repo_root = args.repo_root.resolve()
    try:
        if args.write:
            write_outputs(repo_root)
        else:
            check_outputs(repo_root)
    except GoldenError as exc:
        print(f"AGENT_BRIDGE_GOLDENS: FAIL: {exc}", file=sys.stderr)
        return 1
    print(f"AGENT_BRIDGE_GOLDENS: {'WROTE' if args.write else 'PASS'}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
