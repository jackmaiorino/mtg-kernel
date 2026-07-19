"""Deterministic deck-identified runner over the strict kernel RL client."""

from __future__ import annotations

import dataclasses
import os
import re
from pathlib import Path
from typing import Any

import torch

from . import __version__
from .action_sampling import fixed_categorical_sampler_contract, sample_fixed_categorical
from .artifact_io import json_values_equal_strict, read_json_file, read_regular_file_bytes
from .artifacts import require_new_or_empty_dir
from .client import Decision, KernelRlClient, PROTOCOL_NAME, PROTOCOL_VERSION, SCHEMA_VERSION, Terminal
from .determinism import (
    SeedDerivation,
    configure_torch_determinism,
    derive_env_seed,
    derive_sample_seed,
    derive_uniform_index,
    validate_positive_int,
    validate_uint63,
)
from .features import EncodedDecision, encode_decision
from .model import KernelPolicyValueNet, greedy_action
from .output_lock import OutputLock
from .runner_store import EPISODE_SCHEMA, RUN_SCHEMA, publish_runner_artifacts
from .training_store import (
    MAX_PHYSICAL_DECISIONS,
    MAX_POLICY_STEPS,
    SnapshotRef,
    TrainingStore,
    ValidatedChain,
    runtime_compatibility,
)

POLICIES = {"uniform", "greedy", "sampled"}
RUNNER_ARTIFACT_SCHEMA_VERSION = 5
V1_RUNNER_ARTIFACT_SCHEMA_VERSION = 1
DEFAULT_DECK_IDS = ("Burn", "Burn")
MAX_RUNNER_EPISODES = 262_144
_MAX_ENV_BINARY_BYTES = 1024 * 1024 * 1024
_HEX64_RE = re.compile(r"^[0-9a-f]{64}$")


def _runner_sampled_action_selection_contract() -> dict[str, Any]:
    return {
        "categorical_sampler": fixed_categorical_sampler_contract(),
        "inference": "torch.no_grad model forward; selector consumes detached logits",
        "mode": "sampled_softmax",
        "replacement": False,
        "temperature_hex": "0x1.0000000000000p+0",
    }


def _runner_policy_action_selection_contract(policy: str) -> dict[str, Any]:
    if policy == "uniform":
        return {
            "algorithm": "derive_uniform_index modulo legal_action_count",
            "inference": "unused",
            "mode": "uniform",
        }
    if policy == "greedy":
        return {
            "algorithm": "argmax over finite CPU float32 logits",
            "inference": "torch.no_grad model forward",
            "mode": "greedy",
            "tie_break": "lowest legal-action index",
        }
    if policy == "sampled":
        return _runner_sampled_action_selection_contract()
    raise ValueError(f"unsupported policy {policy}")


def _runner_seat_action_selection_contract(p0: str, p1: str) -> dict[str, Any]:
    return {
        "p0": _runner_policy_action_selection_contract(p0),
        "p1": _runner_policy_action_selection_contract(p1),
    }


def _runner_seed_derivation_contract() -> dict[str, Any]:
    contract = dataclasses.asdict(SeedDerivation())
    contract["outputs"] = {
        "environment_seed": "full unsigned 64-bit SplitMix64 output",
        "sampled_action_seed": "SplitMix64 output & 0x7fff_ffff",
        "uniform_action_index": "SplitMix64 output modulo legal_action_count",
    }
    contract["seat_encoding"] = {"p0": "0x5030", "p1": "0x5031"}
    return contract


RUNNER_ACTION_SELECTION_CONTRACT = {
    policy: _runner_policy_action_selection_contract(policy)
    for policy in sorted(POLICIES)
}
RUNNER_SEED_DERIVATION_CONTRACT = _runner_seed_derivation_contract()


def _policy_for_seat(seat: str, p0: str, p1: str) -> str:
    return p0 if seat == "p0" else p1


class InMemoryModelPolicy:
    """Inference-only adapter over an already validated training snapshot."""

    def __init__(self, model: KernelPolicyValueNet) -> None:
        self.model = model

    def logits_value(self, encoded: EncodedDecision) -> tuple[torch.Tensor, torch.Tensor]:
        with torch.no_grad():
            return self.model(encoded)


def select_action(
    decision: Decision,
    *,
    policy: str,
    base_seed: int,
    episode: int,
    model_policy: InMemoryModelPolicy | None,
) -> int:
    if policy not in POLICIES:
        raise ValueError(f"unsupported policy {policy}")
    if policy == "uniform":
        return int(
            derive_uniform_index(
                base_seed,
                episode,
                decision.physical_decision_id,
                decision.substep_index,
                decision.acting_player,
                len(decision.legal_actions),
            )
        )
    if model_policy is None:
        raise ValueError("greedy and sampled policies require a validated training checkpoint")
    encoded = encode_decision(decision.observation, decision.legal_actions)
    logits, _value = model_policy.logits_value(encoded)
    if policy == "greedy":
        return greedy_action(logits)
    seed = derive_sample_seed(
        base_seed,
        episode,
        decision.physical_decision_id,
        decision.substep_index,
        decision.acting_player,
    )
    return sample_fixed_categorical(logits, seed)


def _episode_record(episode: int, env_seed: int, terminal: Terminal, p0_policy: str, p1_policy: str) -> dict[str, Any]:
    return {
        "schema": EPISODE_SCHEMA,
        "episode": episode,
        "env_seed": env_seed,
        "deck_ids": list(terminal.deck_ids),
        "deck_hashes": list(terminal.deck_hashes),
        "terminal_outcome": terminal.terminal_outcome,
        "terminal_classification": terminal.terminal_classification,
        "terminal_code": terminal.terminal_code,
        "winner": terminal.winner,
        "terminal_reward": terminal.terminal_reward,
        "policy_step_count": terminal.policy_step_count,
        "physical_decision_count": terminal.physical_decision_count,
        "p0_policy": p0_policy,
        "p1_policy": p1_policy,
    }


def _aggregate(records: list[dict[str, Any]]) -> dict[str, int]:
    out = {"episodes": len(records), "p0_wins": 0, "p1_wins": 0, "draws": 0, "halted": 0, "truncated": 0}
    for record in records:
        if record["terminal_outcome"] == "p0_win":
            out["p0_wins"] += 1
        elif record["terminal_outcome"] == "p1_win":
            out["p1_wins"] += 1
        elif record["terminal_outcome"] == "draw":
            out["draws"] += 1
        elif record["terminal_outcome"] == "halted":
            out["halted"] += 1
        elif record["terminal_outcome"] == "truncated":
            out["truncated"] += 1
    return out


@dataclasses.dataclass(frozen=True, slots=True)
class _PolicyPreflight:
    chain: ValidatedChain | None
    ref: SnapshotRef | None
    model: KernelPolicyValueNet | None
    env_sha256: str
    expected_deck_hashes: tuple[int, int] | None
    expected_provenance: dict[str, Any] | None
    policy_source: dict[str, Any]
    runtime_compatibility: dict[str, Any]


def _normalized_real(path: str | Path) -> str:
    return os.path.normcase(os.path.normpath(os.path.realpath(os.path.abspath(os.fspath(path)))))


def _paths_overlap(left: str | Path, right: str | Path) -> bool:
    left_real = _normalized_real(left)
    right_real = _normalized_real(right)
    try:
        common = os.path.commonpath((left_real, right_real))
    except ValueError:
        return False
    return common == left_real or common == right_real


def _validate_exact_head(value: Any) -> str:
    if type(value) is not str or _HEX64_RE.fullmatch(value) is None:
        raise ValueError("expected_policy_head must be exactly 64 lowercase hexadecimal characters")
    return value


def _validate_loaded_model(model: KernelPolicyValueNet, context: str) -> None:
    if model.training:
        raise ValueError(f"{context} model must be in eval mode")
    for name, tensor in list(model.named_parameters()) + list(model.named_buffers()):
        if tensor.device.type != "cpu":
            raise ValueError(f"{context} model tensor {name} must be on CPU")
        if tensor.is_floating_point() and tensor.dtype != torch.float32:
            raise ValueError(f"{context} model tensor {name} must use float32")
    if any(parameter.requires_grad for parameter in model.parameters()):
        raise ValueError(f"{context} model parameters must be frozen")


def _require_model_matches_attestation(
    used: KernelPolicyValueNet,
    attested: KernelPolicyValueNet,
) -> None:
    used_state = used.state_dict()
    attested_state = attested.state_dict()
    if set(used_state) != set(attested_state):
        raise ValueError("runner policy model state keys changed during execution")
    for name in sorted(used_state):
        if not torch.equal(used_state[name], attested_state[name]):
            raise ValueError(f"runner policy model tensor {name} differs from its checkpoint attestation")


def _snapshot_manifest(ref: SnapshotRef, model_fingerprint: str) -> dict[str, Any]:
    return {
        "checkpoint_sha256": ref.checkpoint_sha256,
        "head": ref.head,
        "logical_state_sha256": ref.logical_state_sha256,
        "model_contract_fingerprint": model_fingerprint,
        "parent_head": ref.parent_head,
        "run_digest": ref.run_digest,
        "update": ref.update,
        "update_record_sha256": ref.update_record_sha256,
    }


def _policy_source_manifest(chain: ValidatedChain, ref: SnapshotRef) -> dict[str, Any]:
    source = chain.run_record
    return {
        "mode": "validated_training_head",
        "snapshot": _snapshot_manifest(ref, source["model"]["contract_fingerprint"]),
        "source_training": {
            "environment": source["environment"],
            "feature_contract": source["feature_contract"],
            "model_contract": source["model"],
            "protocol": source["protocol"],
            "protocol_provenance": source["protocol_provenance"],
            "run": {"schema": source["schema"], "sha256": ref.run_digest},
            "runtime_compatibility": source["compatibility"],
            "trainer_max_physical_decisions": source["trainer"]["max_physical_decisions"],
            "trainer_max_policy_steps": source["trainer"]["max_policy_steps"],
        },
    }


def _preflight_policy(
    *,
    training_store: str | Path | None,
    expected_policy_head: str | None,
    env_bin: str | Path,
    out_dir: str | Path,
    p0: str,
    p1: str,
    deck_ids: tuple[str, str],
    max_physical_decisions: int,
    max_policy_steps: int,
) -> _PolicyPreflight:
    runtime = runtime_compatibility()
    env_capture = read_regular_file_bytes(env_bin, max_bytes=_MAX_ENV_BINARY_BYTES, allow_empty=False)
    uses_model = p0 != "uniform" or p1 != "uniform"
    if not uses_model:
        if training_store is not None or expected_policy_head is not None:
            raise ValueError("training_store and expected_policy_head are only valid for greedy or sampled runner policies")
        return _PolicyPreflight(
            chain=None,
            ref=None,
            model=None,
            env_sha256=env_capture.sha256,
            expected_deck_hashes=None,
            expected_provenance=None,
            policy_source={"mode": "none_uniform_only"},
            runtime_compatibility=runtime,
        )
    if training_store is None or expected_policy_head is None:
        raise ValueError("greedy and sampled runner policies require training_store and expected_policy_head")
    expected_head = _validate_exact_head(expected_policy_head)
    chain = TrainingStore(training_store).validate_latest()
    ref = chain.head
    if ref.head != expected_head:
        raise ValueError("expected_policy_head does not match the validated training head")
    if ref.update < 1:
        raise ValueError("greedy and sampled runner policies require a trained head after update zero")
    source = chain.run_record
    if tuple(source["environment"]["deck_ids"]) != deck_ids:
        raise ValueError("runner deck_ids must exactly match the validated training run")
    if source["trainer"]["max_physical_decisions"] != max_physical_decisions:
        raise ValueError("runner max_physical_decisions must exactly match the validated training run")
    if source["trainer"]["max_policy_steps"] != max_policy_steps:
        raise ValueError("runner max_policy_steps must exactly match the validated training run")
    if not json_values_equal_strict(source["compatibility"], runtime):
        raise ValueError("runner runtime compatibility differs from the validated training run")
    if env_capture.sha256 != source["environment"]["binary_sha256"]:
        raise ValueError("runner environment binary hash differs from the validated training run")
    if _paths_overlap(ref.root, out_dir):
        raise ValueError("runner output root must not overlap the training store")
    policy = chain.load_policy(ref)
    if policy.ref.model_config.contract_fingerprint() != source["model"]["contract_fingerprint"]:
        raise ValueError("runner policy model contract differs from the validated training run")
    _validate_loaded_model(policy.model, "runner policy")
    return _PolicyPreflight(
        chain=chain,
        ref=ref,
        model=policy.model,
        env_sha256=env_capture.sha256,
        expected_deck_hashes=tuple(source["environment"]["deck_hashes"]),
        expected_provenance=source["protocol_provenance"],
        policy_source=_policy_source_manifest(chain, ref),
        runtime_compatibility=runtime,
    )


def run_episodes(
    *,
    env_bin: str | Path,
    out_dir: str | Path,
    episodes: int,
    base_seed: int,
    max_physical_decisions: int,
    max_policy_steps: int,
    p0: str,
    p1: str,
    deck_ids: tuple[str, str] = DEFAULT_DECK_IDS,
    timeout_s: float = 10.0,
    training_store: str | Path | None = None,
    expected_policy_head: str | None = None,
) -> dict[str, Any]:
    configure_torch_determinism()
    episodes = validate_positive_int(episodes, "episodes", maximum=MAX_RUNNER_EPISODES)
    base_seed = validate_uint63(base_seed, "base_seed")
    max_physical_decisions = validate_positive_int(
        max_physical_decisions, "max_physical_decisions", maximum=MAX_PHYSICAL_DECISIONS
    )
    max_policy_steps = validate_positive_int(
        max_policy_steps, "max_policy_steps", maximum=MAX_POLICY_STEPS
    )
    if type(p0) is not str or type(p1) is not str or p0 not in POLICIES or p1 not in POLICIES:
        raise ValueError("unsupported policy")
    if type(deck_ids) is not tuple or len(deck_ids) != 2:
        raise TypeError("deck_ids must be an exact two-item tuple")
    if any(type(deck_id) is not str or not deck_id for deck_id in deck_ids):
        raise ValueError("deck_ids entries must be nonempty strings")
    env_path = Path(env_bin)
    preflight = _preflight_policy(
        training_store=training_store,
        expected_policy_head=expected_policy_head,
        env_bin=env_path,
        out_dir=out_dir,
        p0=p0,
        p1=p1,
        deck_ids=deck_ids,
        max_physical_decisions=max_physical_decisions,
        max_policy_steps=max_policy_steps,
    )
    with OutputLock(out_dir) as output_lock:
        root = require_new_or_empty_dir(output_lock.path.parent)
        terminal_records: list[dict[str, Any]] = []
        provenance: dict[str, Any] | None = None
        deck_hashes: tuple[int, int] | None = preflight.expected_deck_hashes
        model_policy = None if preflight.model is None else InMemoryModelPolicy(preflight.model)
        with KernelRlClient(env_path, timeout_s=timeout_s) as client:
            for episode in range(episodes):
                env_seed = derive_env_seed(base_seed, episode)
                current: Decision | Terminal = client.reset(
                    episode_id=episode,
                    env_seed=env_seed,
                    max_physical_decisions=max_physical_decisions,
                    max_policy_steps=max_policy_steps,
                    deck_ids=deck_ids,
                )
                if current.deck_ids != deck_ids:
                    raise ValueError("runner environment deck_ids differ from requested deck_ids")
                if deck_hashes is None:
                    deck_hashes = current.deck_hashes
                elif current.deck_hashes != deck_hashes:
                    raise ValueError("runner environment deck_hashes drift")
                if preflight.expected_provenance is not None and not json_values_equal_strict(
                    current.provenance, preflight.expected_provenance
                ):
                    raise ValueError("runner environment provenance differs from the validated training run")
                if provenance is None:
                    provenance = current.provenance
                elif not json_values_equal_strict(provenance, current.provenance):
                    raise ValueError("runner environment provenance drift")
                while isinstance(current, Decision):
                    policy = _policy_for_seat(current.acting_player, p0, p1)
                    index = select_action(
                        current,
                        policy=policy,
                        base_seed=base_seed,
                        episode=episode,
                        model_policy=model_policy,
                    )
                    action = current.legal_actions[index]
                    current = client.step(action["selected_index"], action["stable_id"])
                    if current.deck_ids != deck_ids or current.deck_hashes != deck_hashes:
                        raise ValueError("runner environment deck identity drift")
                    if not json_values_equal_strict(provenance, current.provenance):
                        raise ValueError("runner environment provenance drift")
                terminal_records.append(_episode_record(episode, env_seed, current, p0, p1))
        aggregate = _aggregate(terminal_records)
        if deck_hashes is None or provenance is None:
            raise AssertionError("runner completed without resolving environment identity")
        if aggregate["halted"] != 0 or aggregate["truncated"] != 0:
            raise RuntimeError("halted/truncated episodes are not admissible")
        recaptured_env = read_regular_file_bytes(env_path, max_bytes=_MAX_ENV_BINARY_BYTES, allow_empty=False)
        if recaptured_env.sha256 != preflight.env_sha256:
            raise ValueError("runner environment binary changed during execution")
        if preflight.chain is not None and preflight.ref is not None and preflight.model is not None:
            attested = preflight.chain.load_policy(preflight.ref).model
            _require_model_matches_attestation(preflight.model, attested)
            _validate_loaded_model(preflight.model, "runner policy")
        manifest = {
            "schema": RUN_SCHEMA,
            "artifact_schema_version": RUNNER_ARTIFACT_SCHEMA_VERSION,
            "artifact_schemas": {"episode": EPISODE_SCHEMA, "run": RUN_SCHEMA},
            "action_selection": _runner_seat_action_selection_contract(p0, p1),
            "package": {"name": "mtg-kernel-rl", "version": __version__},
            "runner_runtime_compatibility": preflight.runtime_compatibility,
            "environment": {
                "binary_sha256": preflight.env_sha256,
                "deck_ids": list(deck_ids),
                "deck_hashes": list(deck_hashes),
                "protocol": {
                    "protocol": PROTOCOL_NAME,
                    "protocol_version": PROTOCOL_VERSION,
                    "schema_version": SCHEMA_VERSION,
                },
                "protocol_provenance": provenance,
            },
            "policy_source": preflight.policy_source,
            "seed_derivation": _runner_seed_derivation_contract(),
            "config": {
                "episodes": episodes,
                "base_seed": base_seed,
                "max_physical_decisions": max_physical_decisions,
                "max_policy_steps": max_policy_steps,
                "p0_policy": p0,
                "p1_policy": p1,
                "deck_ids": list(deck_ids),
            },
            "aggregate": aggregate,
            "publication": {
                "authoritative_file": "run.json",
                "data_files_published_first": ["episodes.jsonl"],
                "fresh_only": True,
                "resume": False,
            },
        }
        publish_runner_artifacts(root, episodes=terminal_records, manifest_without_files=manifest)
        return read_json_file(root / "run.json")
